//! Coordinator-owned semantic queries for immutable class applications.
//!
//! Planning owns the mutable interner. Relation sees only the immutable store
//! plus query-local normalization overlays, and durable writes promote together.

use crate::check::checker::eval::demand::{
    resolve_deferred_outer_layer, resolve_keyof_outer_layer,
};
use crate::check::checker::eval::{ConditionalEvaluator, DEFAULT_STEP_BUDGET};
use crate::check::infer::{infer_from_types_for_query, Candidates};
use crate::class_semantics::{DemandOutcome, Exhaustion, PublishedClassSurface, PublishedClasses};
use crate::relate::cache::RelationCache;
use crate::relate::{
    ReasonChain, Relater, RelationAttempt, RelationDemand, RelationNormalization, RelationOutcome,
};
use crate::types::repr::{ClassId, TypeParamId, TypeTag, Visibility};
use crate::types::store::{Store, TypeId};
use crate::types::{substitute, Interner};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

pub(crate) const MAX_CLASS_PROJECTION_EXPANSIONS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct AlphaBinderKey {
    forward: Vec<(TypeParamId, TypeParamId)>,
    reverse: Vec<(TypeParamId, TypeParamId)>,
}

type IdentitySeen = FxHashSet<(TypeId, TypeId, AlphaBinderKey)>;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueryDemandMeasure {
    pub root_calls: u64,
    pub planner_root_visits: u64,
    pub planner_visits: u64,
    pub overlay_hits: u64,
    pub visited_hits: u64,
    pub reentries: u64,
    pub pending_evaluations: u64,
    pub durable_evaluation_hits: u64,
    pub evaluation_expansions: u64,
    pub evaluation_identity_returns: u64,
    pub evaluation_changed_returns: u64,
    pub evaluation_memo_inserts: u64,
    pub durable_evaluation_inserts: u64,
    pub exhaustion_frontiers: u64,
    pub evaluation_budget_exhaustions: u64,
    pub evaluation_cycle_exhaustions: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QuerySourceColdMeasure {
    pub publication_calls: u64,
    pub publication_query_roots: u64,
    pub publication_edge_visits: u64,
    pub publication_unique_edges: u64,
    pub planner_transactions: u64,
    pub durable_memo_seed_copy_entries: u64,
    pub exhaustion_frontiers: u64,
}

#[cfg(test)]
thread_local! {
    static QUERY_DEMAND_MEASURE: std::cell::RefCell<QueryDemandMeasure> =
        std::cell::RefCell::new(QueryDemandMeasure::default());
    static QUERY_SOURCE_COLD_MEASURE: std::cell::RefCell<QuerySourceColdMeasure> =
        std::cell::RefCell::new(QuerySourceColdMeasure::default());
    static PUBLICATION_UNIQUE_EDGES: std::cell::RefCell<FxHashSet<(TypeId, TypeId)>> =
        std::cell::RefCell::new(FxHashSet::default());
    static QUERY_SOURCE_COLD_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) struct QuerySourceColdMeasureGuard {
    _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl Drop for QuerySourceColdMeasureGuard {
    fn drop(&mut self) {
        QUERY_SOURCE_COLD_ENABLED.with(|enabled| {
            assert!(enabled.get(), "source-cold measurement scope is not active");
            enabled.set(false);
        });
        QUERY_SOURCE_COLD_MEASURE
            .with(|measure| *measure.borrow_mut() = QuerySourceColdMeasure::default());
        PUBLICATION_UNIQUE_EDGES.with(|edges| edges.borrow_mut().clear());
    }
}

#[cfg(test)]
pub(crate) fn reset_query_demand_measure() {
    QUERY_DEMAND_MEASURE.with(|measure| *measure.borrow_mut() = QueryDemandMeasure::default());
}

#[cfg(test)]
pub(crate) fn start_query_source_cold_measure() -> QuerySourceColdMeasureGuard {
    QUERY_SOURCE_COLD_ENABLED.with(|enabled| {
        assert!(
            !enabled.get(),
            "source-cold measurement scope is already active"
        );
        enabled.set(true);
    });
    QUERY_SOURCE_COLD_MEASURE
        .with(|measure| *measure.borrow_mut() = QuerySourceColdMeasure::default());
    PUBLICATION_UNIQUE_EDGES.with(|edges| edges.borrow_mut().clear());
    QuerySourceColdMeasureGuard {
        _not_send: std::marker::PhantomData,
    }
}

#[cfg(test)]
pub(crate) fn query_demand_measure() -> QueryDemandMeasure {
    QUERY_DEMAND_MEASURE.with(|measure| *measure.borrow())
}

#[cfg(test)]
pub(crate) fn query_source_cold_measure() -> Option<QuerySourceColdMeasure> {
    QUERY_SOURCE_COLD_ENABLED.with(|enabled| {
        enabled
            .get()
            .then(|| QUERY_SOURCE_COLD_MEASURE.with(|measure| *measure.borrow()))
    })
}

#[cfg(test)]
fn measure_query_demand(update: impl FnOnce(&mut QueryDemandMeasure)) {
    QUERY_DEMAND_MEASURE.with(|measure| update(&mut measure.borrow_mut()));
}

#[cfg(test)]
fn measure_query_source_cold(update: impl FnOnce(&mut QuerySourceColdMeasure)) {
    QUERY_SOURCE_COLD_ENABLED.with(|enabled| {
        if enabled.get() {
            QUERY_SOURCE_COLD_MEASURE.with(|measure| update(&mut measure.borrow_mut()));
        }
    });
}

#[cfg(test)]
fn measure_publication_children(parent: TypeId, children: &[TypeId]) {
    if !QUERY_SOURCE_COLD_ENABLED.with(std::cell::Cell::get) {
        return;
    }
    let unique = PUBLICATION_UNIQUE_EDGES.with(|edges| {
        let mut edges = edges.borrow_mut();
        children
            .iter()
            .filter(|&&child| edges.insert((parent, child)))
            .count()
    });
    measure_query_source_cold(|measure| {
        measure.publication_edge_visits += u64::try_from(children.len()).unwrap();
        measure.publication_unique_edges += u64::try_from(unique).unwrap();
    });
}

/// Immutable published-class boundary consumed by query planning. Implementors
/// must return poison/pre-publication exhaustion before exposing any template.
pub(crate) trait PublishedClassLookup {
    fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface>;
    fn publication_identity(&self) -> &Arc<()>;
}

impl PublishedClassLookup for PublishedClasses {
    fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
        PublishedClasses::published_class(self, class)
    }

    fn publication_identity(&self) -> &Arc<()> {
        self.identity()
    }
}

/// Pass-local durable semantic-query state. A tainted query changes none of it.
#[derive(Clone, Default)]
pub(crate) struct SemanticQueryState {
    projection_memo: FxHashMap<TypeId, TypeId>,
    evaluation_memo: FxHashMap<TypeId, TypeId>,
    relation_cache: RelationCache,
    publication_clean: FxHashSet<TypeId>,
    publication_store_identity: Option<Arc<()>>,
    publication_snapshot_identity: Option<Arc<()>>,
}

impl SemanticQueryState {
    /// Start an isolated semantic-query overlay seeded from the durable parent.
    /// Callers promote it only after the enclosing operation is decisive.
    pub(crate) fn fork(&self) -> Self {
        self.clone()
    }

    #[cfg(test)]
    pub(crate) fn durable_lengths(&self) -> (usize, usize, usize) {
        (
            self.projection_memo.len(),
            self.evaluation_memo.len(),
            self.relation_cache.len(),
        )
    }
}

/// The sole mutable entry boundary for class-reachable semantic queries.
pub(crate) struct SemanticQueryCoordinator<'a, L: PublishedClassLookup + ?Sized> {
    interner: &'a mut Interner,
    published: &'a L,
    state: &'a mut SemanticQueryState,
    next_type_param: &'a mut u32,
}

impl<'a, L: PublishedClassLookup + ?Sized> SemanticQueryCoordinator<'a, L> {
    pub(crate) fn new(
        interner: &'a mut Interner,
        published: &'a L,
        state: &'a mut SemanticQueryState,
        next_type_param: &'a mut u32,
    ) -> Self {
        SemanticQueryCoordinator {
            interner,
            published,
            state,
            next_type_param,
        }
    }

    /// Demand one normalized outer shape. Successful untainted work promotes
    /// projection/evaluator memo entries together; exhaustion promotes nothing.
    pub(crate) fn demand(&mut self, root: TypeId) -> DemandOutcome<TypeId> {
        #[cfg(test)]
        measure_query_demand(|measure| measure.root_calls += 1);
        let transaction = ProjectionPlanner::new(
            self.interner,
            self.published,
            &self.state.projection_memo,
            &self.state.evaluation_memo,
            *self.next_type_param,
        )
        .plan_demand(root);
        *self.next_type_param = transaction.next_type_param;
        match transaction.plan.normalize(root) {
            Ok(normalized) if !transaction.planning_tainted => {
                self.commit_plan(transaction);
                DemandOutcome::Ready(normalized)
            }
            Ok(_) => DemandOutcome::Exhausted(
                transaction
                    .first_exhaustion
                    .clone()
                    .unwrap_or(Exhaustion::ClassProjectionBudget),
            ),
            Err(reason) => DemandOutcome::Exhausted(reason),
        }
    }

    /// Normalize a class application's arguments without erasing its nominal root.
    /// Call/constructor inference needs the class identity until the relation phase.
    pub(crate) fn normalize_class_application(&mut self, root: TypeId) -> DemandOutcome<TypeId> {
        let Some(application) = self.interner.store().class_instance_type(root).cloned() else {
            return self.demand(root);
        };
        let mut normalized = Vec::with_capacity(application.args.len());
        for argument in application.args {
            match self.demand(argument) {
                DemandOutcome::Ready(argument) => normalized.push(argument),
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            }
        }
        DemandOutcome::Ready(
            self.interner
                .intern_class_instance(application.class, normalized),
        )
    }

    /// Normalize two published roots in one query transaction, then compare their
    /// canonical identities. Interface heritage uses identity rather than
    /// assignability (`1` differs from `number`, and `any` differs from `string`).
    pub(crate) fn is_identical(&mut self, left: TypeId, right: TypeId) -> DemandOutcome<bool> {
        if let Some(reason) = publication_exhaustion(
            self.interner.store(),
            &[left, right],
            self.published,
            self.state,
        ) {
            return DemandOutcome::Exhausted(reason);
        }
        let transaction = ProjectionPlanner::new(
            self.interner,
            self.published,
            &self.state.projection_memo,
            &self.state.evaluation_memo,
            *self.next_type_param,
        )
        .plan(&[left, right]);
        *self.next_type_param = transaction.next_type_param;
        if transaction.planning_tainted {
            return DemandOutcome::Exhausted(
                transaction
                    .first_exhaustion
                    .clone()
                    .unwrap_or(Exhaustion::ClassProjectionBudget),
            );
        }
        let outcome = Self::identical_recursive(
            self.interner.store(),
            &transaction.plan,
            left,
            right,
            &mut FxHashSet::default(),
            &mut Vec::new(),
        );
        if matches!(outcome, DemandOutcome::Ready(_)) {
            self.commit_plan(transaction);
        }
        outcome
    }

    fn identical_recursive(
        store: &Store,
        plan: &ProjectionPlan,
        left: TypeId,
        right: TypeId,
        seen: &mut IdentitySeen,
        alpha_binders: &mut Vec<(TypeParamId, TypeParamId)>,
    ) -> DemandOutcome<bool> {
        let mut left = match plan.normalize(left) {
            Ok(left) => left,
            Err(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
        };
        let mut right = match plan.normalize(right) {
            Ok(right) => right,
            Err(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
        };
        left = match Self::collapse_exact_family_root(store, plan, left) {
            Ok(left) => left,
            Err(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
        };
        right = match Self::collapse_exact_family_root(store, plan, right) {
            Ok(right) => right,
            Err(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
        };
        if let (Some(left_param), Some(right_param)) =
            (store.type_param(left), store.type_param(right))
        {
            return DemandOutcome::Ready(Self::alpha_type_params_identical(
                left_param.id,
                right_param.id,
                alpha_binders,
            ));
        }
        if left == right {
            return DemandOutcome::Ready(true);
        }
        if !seen.insert((left, right, Self::alpha_binder_key(alpha_binders))) {
            return DemandOutcome::Ready(true);
        }
        let tag = store.tag(left);
        if tag != store.tag(right) {
            return DemandOutcome::Ready(false);
        }
        match tag {
            TypeTag::Object => {
                let left = store
                    .object_type(left)
                    .expect("object tag has payload")
                    .clone();
                let right = store
                    .object_type(right)
                    .expect("object tag has payload")
                    .clone();
                if left.properties.len() != right.properties.len()
                    || left.call_signatures.len() != right.call_signatures.len()
                    || left.construct_signatures.len() != right.construct_signatures.len()
                {
                    return DemandOutcome::Ready(false);
                }
                for (left, right) in left.properties.iter().zip(&right.properties) {
                    if left.name != right.name
                        || left.optional != right.optional
                        || left.visibility != right.visibility
                        || (left.visibility != Visibility::Public
                            && left.declaring_class != right.declaring_class)
                        || left.readonly != right.readonly
                        || left.is_accessor != right.is_accessor
                    {
                        return DemandOutcome::Ready(false);
                    }
                    match Self::identical_recursive(
                        store,
                        plan,
                        left.ty,
                        right.ty,
                        seen,
                        alpha_binders,
                    ) {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                    match Self::identical_optional(
                        store,
                        plan,
                        left.write_ty,
                        right.write_ty,
                        seen,
                        alpha_binders,
                    ) {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                for (left, right) in left.call_signatures.iter().zip(&right.call_signatures) {
                    match Self::identical_recursive(store, plan, *left, *right, seen, alpha_binders)
                    {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                for (left, right) in left
                    .construct_signatures
                    .iter()
                    .zip(&right.construct_signatures)
                {
                    match Self::identical_recursive(store, plan, *left, *right, seen, alpha_binders)
                    {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                match Self::identical_optional(
                    store,
                    plan,
                    left.string_index,
                    right.string_index,
                    seen,
                    alpha_binders,
                ) {
                    DemandOutcome::Ready(true) => {}
                    outcome => return outcome,
                }
                Self::identical_optional(
                    store,
                    plan,
                    left.number_index,
                    right.number_index,
                    seen,
                    alpha_binders,
                )
            }
            TypeTag::Function => {
                let left = store
                    .function_type(left)
                    .expect("function tag has payload")
                    .clone();
                let right = store
                    .function_type(right)
                    .expect("function tag has payload")
                    .clone();
                if left.type_params.len() != right.type_params.len()
                    || left.params.len() != right.params.len()
                {
                    return DemandOutcome::Ready(false);
                }
                let binder_start = alpha_binders.len();
                alpha_binders.extend(
                    left.type_params
                        .iter()
                        .zip(&right.type_params)
                        .map(|(left, right)| (left.id, right.id)),
                );
                let outcome = (|| {
                    for (left, right) in left.type_params.iter().zip(&right.type_params) {
                        match Self::identical_optional(
                            store,
                            plan,
                            left.constraint,
                            right.constraint,
                            seen,
                            alpha_binders,
                        ) {
                            DemandOutcome::Ready(true) => {}
                            outcome => return outcome,
                        }
                        match Self::identical_optional(
                            store,
                            plan,
                            left.default,
                            right.default,
                            seen,
                            alpha_binders,
                        ) {
                            DemandOutcome::Ready(true) => {}
                            outcome => return outcome,
                        }
                    }
                    match Self::identical_optional(
                        store,
                        plan,
                        left.receiver,
                        right.receiver,
                        seen,
                        alpha_binders,
                    ) {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                    for (left, right) in left.params.iter().zip(&right.params) {
                        if left.optional != right.optional
                            || left.has_default != right.has_default
                            || left.rest != right.rest
                        {
                            return DemandOutcome::Ready(false);
                        }
                        match Self::identical_recursive(
                            store,
                            plan,
                            left.ty,
                            right.ty,
                            seen,
                            alpha_binders,
                        ) {
                            DemandOutcome::Ready(true) => {}
                            outcome => return outcome,
                        }
                    }
                    Self::identical_recursive(store, plan, left.ret, right.ret, seen, alpha_binders)
                })();
                alpha_binders.truncate(binder_start);
                outcome
            }
            TypeTag::Array => {
                let left = store.array_type(left).unwrap().element;
                let right = store.array_type(right).unwrap().element;
                Self::identical_recursive(store, plan, left, right, seen, alpha_binders)
            }
            TypeTag::Tuple => {
                let left = store.tuple_type(left).unwrap().clone();
                let right = store.tuple_type(right).unwrap().clone();
                if left.elements.len() != right.elements.len()
                    || left.rest.map(|rest| rest.position) != right.rest.map(|rest| rest.position)
                {
                    return DemandOutcome::Ready(false);
                }
                for (left, right) in left.elements.iter().zip(&right.elements) {
                    match Self::identical_recursive(store, plan, *left, *right, seen, alpha_binders)
                    {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                Self::identical_optional(
                    store,
                    plan,
                    left.rest.map(|rest| rest.ty),
                    right.rest.map(|rest| rest.ty),
                    seen,
                    alpha_binders,
                )
            }
            TypeTag::Readonly => Self::identical_recursive(
                store,
                plan,
                store.readonly_operand(left).unwrap(),
                store.readonly_operand(right).unwrap(),
                seen,
                alpha_binders,
            ),
            TypeTag::Union | TypeTag::Intersection => {
                let left = if tag == TypeTag::Union {
                    store.union_members(left)
                } else {
                    store.intersection_members(left)
                }
                .unwrap()
                .to_vec();
                let right = if tag == TypeTag::Union {
                    store.union_members(right)
                } else {
                    store.intersection_members(right)
                }
                .unwrap()
                .to_vec();
                let left = match Self::flatten_normalized_family(store, plan, tag, &left) {
                    Ok(left) => left,
                    Err(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
                };
                let right = match Self::flatten_normalized_family(store, plan, tag, &right) {
                    Ok(right) => right,
                    Err(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
                };
                Self::identical_unordered(store, plan, &left, &right, seen, alpha_binders)
            }
            TypeTag::ClassInstance => {
                let left = store.class_instance_type(left).unwrap().clone();
                let right = store.class_instance_type(right).unwrap().clone();
                if left.class != right.class || left.args.len() != right.args.len() {
                    return DemandOutcome::Ready(false);
                }
                for (left, right) in left.args.into_iter().zip(right.args) {
                    match Self::identical_recursive(store, plan, left, right, seen, alpha_binders) {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                DemandOutcome::Ready(true)
            }
            TypeTag::Conditional => {
                let left = *store.conditional_type(left).expect("conditional payload");
                let right = *store.conditional_type(right).expect("conditional payload");
                if left.infer_count != right.infer_count
                    || left.distributive != right.distributive
                    || left.poisoned != right.poisoned
                {
                    return DemandOutcome::Ready(false);
                }
                for (left, right) in [
                    (left.check, right.check),
                    (left.extends_ty, right.extends_ty),
                    (left.true_branch, right.true_branch),
                    (left.false_branch, right.false_branch),
                ] {
                    match Self::identical_recursive(store, plan, left, right, seen, alpha_binders) {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                DemandOutcome::Ready(true)
            }
            TypeTag::Instantiation => {
                let left = store
                    .instantiation_type(left)
                    .expect("instantiation payload")
                    .clone();
                let right = store
                    .instantiation_type(right)
                    .expect("instantiation payload")
                    .clone();
                if left.args.len() != right.args.len() {
                    return DemandOutcome::Ready(false);
                }
                match Self::identical_recursive(
                    store,
                    plan,
                    left.base,
                    right.base,
                    seen,
                    alpha_binders,
                ) {
                    DemandOutcome::Ready(true) => {}
                    outcome => return outcome,
                }
                let mut remaining = right.args;
                for (left_key, left_value) in left.args {
                    let Some(position) = remaining.iter().position(|(right_key, _)| {
                        Self::alpha_type_params_identical(left_key, *right_key, alpha_binders)
                    }) else {
                        return DemandOutcome::Ready(false);
                    };
                    let (_, right_value) = remaining.remove(position);
                    match Self::identical_recursive(
                        store,
                        plan,
                        left_value,
                        right_value,
                        seen,
                        alpha_binders,
                    ) {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                DemandOutcome::Ready(true)
            }
            TypeTag::Mapped => {
                let left = *store.mapped_type(left).expect("mapped payload");
                let right = *store.mapped_type(right).expect("mapped payload");
                if left.homomorphic != right.homomorphic
                    || left.optional_modifier != right.optional_modifier
                    || left.readonly_modifier != right.readonly_modifier
                {
                    return DemandOutcome::Ready(false);
                }
                for (left, right) in [
                    (Some(left.key_source), Some(right.key_source)),
                    (Some(left.value_template), Some(right.value_template)),
                    (left.modifiers_source, right.modifiers_source),
                ] {
                    match Self::identical_optional(store, plan, left, right, seen, alpha_binders) {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                DemandOutcome::Ready(true)
            }
            TypeTag::Template => {
                let left = store.template_type(left).expect("template payload").clone();
                let right = store
                    .template_type(right)
                    .expect("template payload")
                    .clone();
                if left.texts != right.texts || left.holes.len() != right.holes.len() {
                    return DemandOutcome::Ready(false);
                }
                for (left, right) in left.holes.into_iter().zip(right.holes) {
                    match Self::identical_recursive(store, plan, left, right, seen, alpha_binders) {
                        DemandOutcome::Ready(true) => {}
                        outcome => return outcome,
                    }
                }
                DemandOutcome::Ready(true)
            }
            TypeTag::Keyof => Self::identical_recursive(
                store,
                plan,
                store.keyof_operand(left).expect("keyof payload"),
                store.keyof_operand(right).expect("keyof payload"),
                seen,
                alpha_binders,
            ),
            TypeTag::DeferredIndexedAccess => {
                let left = *store
                    .deferred_indexed_access_type(left)
                    .expect("indexed-access payload");
                let right = *store
                    .deferred_indexed_access_type(right)
                    .expect("indexed-access payload");
                match Self::identical_recursive(
                    store,
                    plan,
                    left.object,
                    right.object,
                    seen,
                    alpha_binders,
                ) {
                    DemandOutcome::Ready(true) => Self::identical_recursive(
                        store,
                        plan,
                        left.index,
                        right.index,
                        seen,
                        alpha_binders,
                    ),
                    outcome => outcome,
                }
            }
            TypeTag::Infer => DemandOutcome::Ready(
                store.infer_index(left).expect("infer payload")
                    == store.infer_index(right).expect("infer payload"),
            ),
            TypeTag::MappedValue => DemandOutcome::Ready(true),
            _ => DemandOutcome::Ready(false),
        }
    }

    fn identical_optional(
        store: &Store,
        plan: &ProjectionPlan,
        left: Option<TypeId>,
        right: Option<TypeId>,
        seen: &mut IdentitySeen,
        alpha_binders: &mut Vec<(TypeParamId, TypeParamId)>,
    ) -> DemandOutcome<bool> {
        match (left, right) {
            (Some(left), Some(right)) => {
                Self::identical_recursive(store, plan, left, right, seen, alpha_binders)
            }
            (None, None) => DemandOutcome::Ready(true),
            _ => DemandOutcome::Ready(false),
        }
    }

    fn alpha_binder_key(alpha_binders: &[(TypeParamId, TypeParamId)]) -> AlphaBinderKey {
        let mut forward_seen = FxHashSet::default();
        let mut forward = Vec::new();
        let mut reverse_seen = FxHashSet::default();
        let mut reverse = Vec::new();
        for &(left, right) in alpha_binders.iter().rev() {
            if forward_seen.insert(left) {
                forward.push((left, right));
            }
            if reverse_seen.insert(right) {
                reverse.push((right, left));
            }
        }
        forward.sort_unstable();
        reverse.sort_unstable();
        AlphaBinderKey { forward, reverse }
    }

    fn alpha_type_params_identical(
        left: TypeParamId,
        right: TypeParamId,
        alpha_binders: &[(TypeParamId, TypeParamId)],
    ) -> bool {
        let mapped_right = alpha_binders
            .iter()
            .rev()
            .find_map(|(candidate, mapped)| (*candidate == left).then_some(*mapped));
        let mapped_left = alpha_binders
            .iter()
            .rev()
            .find_map(|(mapped, candidate)| (*candidate == right).then_some(*mapped));
        match (mapped_right, mapped_left) {
            (Some(mapped), _) => mapped == right,
            (None, Some(_)) => false,
            (None, None) => left == right,
        }
    }

    fn identical_unordered(
        store: &Store,
        plan: &ProjectionPlan,
        left: &[TypeId],
        right: &[TypeId],
        seen: &mut IdentitySeen,
        alpha_binders: &[(TypeParamId, TypeParamId)],
    ) -> DemandOutcome<bool> {
        if left.len() != right.len() {
            return DemandOutcome::Ready(false);
        }
        let mut remaining = right.to_vec();
        for &candidate in left {
            let mut matched = None;
            for (position, &target) in remaining.iter().enumerate() {
                let mut trial_seen = seen.clone();
                let mut trial_binders = alpha_binders.to_vec();
                match Self::identical_recursive(
                    store,
                    plan,
                    candidate,
                    target,
                    &mut trial_seen,
                    &mut trial_binders,
                ) {
                    DemandOutcome::Ready(true) => {
                        matched = Some((position, trial_seen));
                        break;
                    }
                    DemandOutcome::Ready(false) => {}
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                }
            }
            let Some((position, trial_seen)) = matched else {
                return DemandOutcome::Ready(false);
            };
            *seen = trial_seen;
            remaining.remove(position);
        }
        DemandOutcome::Ready(remaining.is_empty())
    }

    fn flatten_normalized_family(
        store: &Store,
        plan: &ProjectionPlan,
        family: TypeTag,
        roots: &[TypeId],
    ) -> Result<Vec<TypeId>, Exhaustion> {
        let mut flattened = Vec::new();
        let mut stack = roots.to_vec();
        let mut expanded = FxHashSet::default();
        while let Some(root) = stack.pop() {
            let normalized = plan.normalize(root)?;
            if store.tag(normalized) == family && expanded.insert(normalized) {
                let members = if family == TypeTag::Union {
                    store.union_members(normalized)
                } else {
                    store.intersection_members(normalized)
                }
                .expect("normalized family tag has members");
                stack.extend(members.iter().copied());
            } else if store.tag(normalized) != family {
                flattened.push(normalized);
            }
        }
        flattened.sort_unstable();
        flattened.dedup();
        Ok(flattened)
    }

    fn collapse_exact_family_root(
        store: &Store,
        plan: &ProjectionPlan,
        root: TypeId,
    ) -> Result<TypeId, Exhaustion> {
        let tag = store.tag(root);
        if !matches!(tag, TypeTag::Union | TypeTag::Intersection) {
            return Ok(root);
        }
        let roots = if tag == TypeTag::Union {
            store.union_members(root)
        } else {
            store.intersection_members(root)
        }
        .expect("family root has members");
        let flattened = Self::flatten_normalized_family(store, plan, tag, roots)?;
        Ok(if flattened.len() == 1 {
            flattened[0]
        } else {
            root
        })
    }

    /// Plan, normalize, and relate one top-level assignability operation.
    pub(crate) fn is_assignable(&mut self, src: TypeId, tgt: TypeId) -> RelationOutcome {
        if let Some(reason) = publication_exhaustion(
            self.interner.store(),
            &[src, tgt],
            self.published,
            self.state,
        ) {
            return RelationOutcome::Exhausted(reason);
        }
        if src == tgt {
            return RelationOutcome::Yes;
        }
        if let Some(outcome) = self.same_class_covariant_argument_mismatch(src, tgt) {
            return outcome;
        }
        let mut planner = ProjectionPlanner::new(
            self.interner,
            self.published,
            &self.state.projection_memo,
            &self.state.evaluation_memo,
            *self.next_type_param,
        );
        planner.prepare_relation_roots(&[src, tgt]);
        let mut relation_cache = std::mem::take(&mut self.state.relation_cache);
        let mut outcome = loop {
            let well_known = planner.interner.well_known();
            let (attempt, returned_cache) = {
                let mut relater = Relater::planned(
                    planner.interner.store(),
                    well_known,
                    relation_cache,
                    &planner.plan,
                );
                let attempt = relater.is_assignable_attempt(src, tgt);
                let commit_cache = matches!(
                    attempt,
                    RelationAttempt::Decided(RelationOutcome::Yes | RelationOutcome::No(_))
                ) && !planner.planning_tainted;
                let cache = relater.finish_planned(commit_cache);
                (attempt, cache)
            };
            relation_cache = returned_cache;
            match attempt {
                RelationAttempt::Decided(outcome) => break outcome,
                RelationAttempt::Needs(demand) => planner.expand_relation_demand(demand),
            }
        };
        let transaction = planner.finish();
        *self.next_type_param = transaction.next_type_param;
        if matches!(outcome, RelationOutcome::Yes) && transaction.planning_tainted {
            outcome = RelationOutcome::Exhausted(
                transaction
                    .first_exhaustion
                    .clone()
                    .unwrap_or(Exhaustion::ClassProjectionBudget),
            );
        }
        let commit_plan =
            !transaction.planning_tainted && !matches!(outcome, RelationOutcome::Exhausted(_));
        self.state.relation_cache = relation_cache;
        if commit_plan {
            self.commit_plan(transaction);
        }
        outcome
    }

    /// A directly published public `value: T` is a finite structural witness for
    /// `C<S> -> C<T>`. Checking it before an unrelated recursive sibling avoids
    /// spending the projection budget on a mismatch already present at this layer.
    fn same_class_covariant_argument_mismatch(
        &mut self,
        src: TypeId,
        tgt: TypeId,
    ) -> Option<RelationOutcome> {
        let source = self.interner.store().class_instance_type(src)?.clone();
        let target = self.interner.store().class_instance_type(tgt)?.clone();
        if source.class != target.class || source.args.len() != target.args.len() {
            return None;
        }
        let surface = match self.published.published_class(source.class) {
            DemandOutcome::Ready(surface) => surface,
            DemandOutcome::Exhausted(exhaustion) => {
                return Some(RelationOutcome::Exhausted(exhaustion))
            }
        };
        let witnessed: FxHashSet<_> = self
            .interner
            .store()
            .object_type(surface.instance_template())
            .into_iter()
            .flat_map(|object| &object.properties)
            .filter(|property| property.visibility == Visibility::Public)
            .filter_map(|property| {
                self.interner
                    .store()
                    .type_param(property.ty)
                    .map(|parameter| parameter.id)
            })
            .collect();
        let parameters = surface.type_params().to_vec();
        for ((parameter, source), target) in
            parameters.into_iter().zip(source.args).zip(target.args)
        {
            if !witnessed.contains(&parameter) || source == target {
                continue;
            }
            match self.is_assignable(source, target) {
                RelationOutcome::Yes => {}
                RelationOutcome::No(_) => {
                    return Some(RelationOutcome::No(ReasonChain::leaf(src, tgt)))
                }
                RelationOutcome::Exhausted(exhaustion) => {
                    return Some(RelationOutcome::Exhausted(exhaustion))
                }
            }
        }
        None
    }

    /// Structurally collect inference candidates through the same overlays.
    /// Exhaustion discards the attempt's local candidates and every pending write.
    pub(crate) fn infer_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) -> DemandOutcome<()> {
        let transaction = ProjectionPlanner::new(
            self.interner,
            self.published,
            &self.state.projection_memo,
            &self.state.evaluation_memo,
            *self.next_type_param,
        )
        .plan(&[source, target]);
        *self.next_type_param = transaction.next_type_param;
        if transaction.planning_tainted {
            return DemandOutcome::Exhausted(
                transaction
                    .first_exhaustion
                    .clone()
                    .unwrap_or(Exhaustion::ClassProjectionBudget),
            );
        }
        let outcome = infer_from_types_for_query(
            self.interner,
            source,
            target,
            candidates,
            &transaction.plan,
        );
        if matches!(outcome, DemandOutcome::Ready(())) {
            if transaction.planning_tainted {
                return DemandOutcome::Exhausted(
                    transaction
                        .first_exhaustion
                        .clone()
                        .unwrap_or(Exhaustion::ClassProjectionBudget),
                );
            }
            self.commit_plan(transaction);
        }
        outcome
    }

    /// Check one overload signature against its implementation through the same
    /// projection/evaluation transaction as every other class-reachable relation.
    pub(crate) fn overload_implementation_compatible(
        &mut self,
        overload: TypeId,
        implementation: TypeId,
    ) -> RelationOutcome {
        if let Some(reason) = publication_exhaustion(
            self.interner.store(),
            &[overload, implementation],
            self.published,
            self.state,
        ) {
            return RelationOutcome::Exhausted(reason);
        }
        let mut planner = ProjectionPlanner::new(
            self.interner,
            self.published,
            &self.state.projection_memo,
            &self.state.evaluation_memo,
            *self.next_type_param,
        );
        planner.prepare_relation_roots(&[overload, implementation]);
        let mut relation_cache = std::mem::take(&mut self.state.relation_cache);
        let mut outcome = loop {
            let well_known = planner.interner.well_known();
            let (attempt, returned_cache) = {
                let mut relater = Relater::planned(
                    planner.interner.store(),
                    well_known,
                    relation_cache,
                    &planner.plan,
                );
                let attempt =
                    relater.overload_implementation_compatible_attempt(overload, implementation);
                let commit_cache = matches!(
                    attempt,
                    RelationAttempt::Decided(RelationOutcome::Yes | RelationOutcome::No(_))
                ) && !planner.planning_tainted;
                let cache = relater.finish_planned(commit_cache);
                (attempt, cache)
            };
            relation_cache = returned_cache;
            match attempt {
                RelationAttempt::Decided(outcome) => break outcome,
                RelationAttempt::Needs(demand) => planner.expand_relation_demand(demand),
            }
        };
        let transaction = planner.finish();
        *self.next_type_param = transaction.next_type_param;
        if matches!(outcome, RelationOutcome::Yes) && transaction.planning_tainted {
            outcome = RelationOutcome::Exhausted(
                transaction
                    .first_exhaustion
                    .clone()
                    .unwrap_or(Exhaustion::ClassProjectionBudget),
            );
        }
        let commit_plan =
            !transaction.planning_tainted && !matches!(outcome, RelationOutcome::Exhausted(_));
        self.state.relation_cache = relation_cache;
        if commit_plan {
            self.commit_plan(transaction);
        }
        outcome
    }

    fn commit_plan(&mut self, transaction: PlannedQuery) {
        #[cfg(test)]
        measure_query_demand(|measure| {
            measure.durable_evaluation_inserts +=
                u64::try_from(transaction.pending_evaluator_writes.len()).unwrap();
        });
        self.state
            .projection_memo
            .extend(transaction.pending_projection_writes);
        self.state
            .evaluation_memo
            .extend(transaction.pending_evaluator_writes);
        *self.next_type_param = transaction.next_type_param;
    }
}

/// Immutable overlays consumed by relation before identity/cache/cycle logic.
#[derive(Default)]
pub(crate) struct ProjectionPlan {
    class_projection_overlay: FxHashMap<TypeId, TypeId>,
    evaluation_overlay: FxHashMap<TypeId, TypeId>,
    resolved_evaluations: FxHashSet<TypeId>,
    frontier: FxHashMap<TypeId, Exhaustion>,
}

impl RelationNormalization for ProjectionPlan {
    fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
        let mut current = ty;
        let mut seen = FxHashSet::default();
        loop {
            if let Some(reason) = self.frontier.get(&current) {
                return Err(reason.clone());
            }
            if !seen.insert(current) {
                return Ok(current);
            }
            let next = self
                .evaluation_overlay
                .get(&current)
                .or_else(|| self.class_projection_overlay.get(&current))
                .copied();
            match next {
                Some(next) if next != current => current = next,
                _ => return Ok(current),
            }
        }
    }

    fn relation_demand(&self, store: &Store, ty: TypeId) -> Option<RelationDemand> {
        match store.tag(ty) {
            TypeTag::ClassInstance => Some(RelationDemand::ClassProjection(ty)),
            TypeTag::DeferredIndexedAccess
            | TypeTag::Keyof
            | TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::Mapped
            | TypeTag::Template
                if !self.resolved_evaluations.contains(&ty) =>
            {
                Some(RelationDemand::Evaluation(ty))
            }
            _ => None,
        }
    }
}

struct PlannedQuery {
    plan: ProjectionPlan,
    pending_projection_writes: FxHashMap<TypeId, TypeId>,
    pending_evaluator_writes: FxHashMap<TypeId, TypeId>,
    next_type_param: u32,
    planning_tainted: bool,
    first_exhaustion: Option<Exhaustion>,
}

struct ProjectionPlanner<'a, L: PublishedClassLookup + ?Sized> {
    interner: &'a mut Interner,
    published: &'a L,
    durable_projection_memo: &'a FxHashMap<TypeId, TypeId>,
    durable_evaluation_memo: &'a FxHashMap<TypeId, TypeId>,
    working_evaluation_memo: FxHashMap<TypeId, TypeId>,
    next_type_param: u32,
    plan: ProjectionPlan,
    pending_projection_writes: FxHashMap<TypeId, TypeId>,
    admitted_applications: FxHashSet<TypeId>,
    visiting: FxHashSet<TypeId>,
    visited: FxHashSet<TypeId>,
    planning_tainted: bool,
    first_exhaustion: Option<Exhaustion>,
    demand_outer_only: bool,
    evaluation_expansions: u32,
}

impl<'a, L: PublishedClassLookup + ?Sized> ProjectionPlanner<'a, L> {
    fn new(
        interner: &'a mut Interner,
        published: &'a L,
        durable_projection_memo: &'a FxHashMap<TypeId, TypeId>,
        durable_evaluation_memo: &'a FxHashMap<TypeId, TypeId>,
        next_type_param: u32,
    ) -> Self {
        let reusable_evaluations: FxHashMap<TypeId, TypeId> = durable_evaluation_memo
            .iter()
            .filter_map(|(&source, &result)| (source != result).then_some((source, result)))
            .collect();
        #[cfg(test)]
        measure_query_source_cold(|measure| {
            measure.planner_transactions += 1;
            measure.durable_memo_seed_copy_entries +=
                u64::try_from(reusable_evaluations.len()).unwrap();
        });
        ProjectionPlanner {
            interner,
            published,
            durable_projection_memo,
            durable_evaluation_memo,
            working_evaluation_memo: reusable_evaluations.clone(),
            next_type_param,
            plan: ProjectionPlan {
                evaluation_overlay: reusable_evaluations.clone(),
                resolved_evaluations: reusable_evaluations.keys().copied().collect(),
                ..ProjectionPlan::default()
            },
            pending_projection_writes: FxHashMap::default(),
            admitted_applications: FxHashSet::default(),
            visiting: FxHashSet::default(),
            visited: FxHashSet::default(),
            planning_tainted: false,
            first_exhaustion: None,
            demand_outer_only: false,
            evaluation_expansions: 0,
        }
    }

    fn plan(mut self, roots: &[TypeId]) -> PlannedQuery {
        for &root in roots {
            self.visit(root);
        }
        self.finish()
    }

    fn plan_demand(mut self, root: TypeId) -> PlannedQuery {
        self.demand_outer_only = true;
        match self.interner.store().tag(root) {
            TypeTag::ClassInstance => {
                self.project_class_outer(root);
            }
            TypeTag::DeferredIndexedAccess
            | TypeTag::Keyof
            | TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::Mapped
            | TypeTag::Template => {
                #[cfg(test)]
                measure_query_demand(|measure| measure.planner_root_visits += 1);
                self.visit(root);
            }
            TypeTag::Intrinsic
            | TypeTag::Literal
            | TypeTag::TypeParam
            | TypeTag::Infer
            | TypeTag::MappedValue
            | TypeTag::Object
            | TypeTag::Union
            | TypeTag::Intersection
            | TypeTag::Function
            | TypeTag::Array
            | TypeTag::Tuple
            | TypeTag::Readonly => {}
        }
        self.finish()
    }

    /// Admit only semantic roots. Nested operations are expanded one at a time
    /// when relation reaches them, so an independent mismatch can win before an
    /// untouched recursive frontier.
    fn prepare_relation_roots(&mut self, roots: &[TypeId]) {
        for &root in roots {
            match self.interner.store().tag(root) {
                TypeTag::ClassInstance => {
                    self.project_class_outer(root);
                }
                TypeTag::DeferredIndexedAccess
                | TypeTag::Keyof
                | TypeTag::Conditional
                | TypeTag::Instantiation
                | TypeTag::Mapped
                | TypeTag::Template => {
                    self.visit(root);
                }
                _ => {}
            }
        }
    }

    fn expand_relation_demand(&mut self, demand: RelationDemand) {
        match demand {
            RelationDemand::ClassProjection(application) => {
                self.project_class_outer(application);
            }
            RelationDemand::Evaluation(ty) => {
                self.visit(ty);
            }
        }
    }

    fn finish(self) -> PlannedQuery {
        let pending_evaluator_writes = self
            .working_evaluation_memo
            .iter()
            .filter_map(|(&key, &value)| {
                (self.durable_evaluation_memo.get(&key) != Some(&value)).then_some((key, value))
            })
            .collect();
        PlannedQuery {
            plan: self.plan,
            pending_projection_writes: self.pending_projection_writes,
            pending_evaluator_writes,
            next_type_param: self.next_type_param,
            planning_tainted: self.planning_tainted,
            first_exhaustion: self.first_exhaustion,
        }
    }

    fn visit(&mut self, ty: TypeId) -> TypeId {
        #[cfg(test)]
        measure_query_demand(|measure| measure.planner_visits += 1);
        if let Ok(normalized) = self.plan.normalize(ty) {
            if normalized != ty {
                #[cfg(test)]
                measure_query_demand(|measure| measure.overlay_hits += 1);
                self.visit_demand_result(normalized);
                return self.plan.normalize(ty).unwrap_or(normalized);
            }
        }
        if self.visited.contains(&ty) {
            #[cfg(test)]
            measure_query_demand(|measure| measure.visited_hits += 1);
            return ty;
        }
        if !self.visiting.insert(ty) {
            #[cfg(test)]
            measure_query_demand(|measure| measure.reentries += 1);
            return ty;
        }

        let result = match self.interner.store().tag(ty) {
            TypeTag::ClassInstance if self.demand_outer_only => self.project_class_outer(ty),
            TypeTag::ClassInstance => self.project_class(ty),
            TypeTag::DeferredIndexedAccess => self.evaluate_deferred_indexed(ty),
            TypeTag::Keyof => self.evaluate_keyof(ty),
            TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::Union
            | TypeTag::Mapped
            | TypeTag::Template => self.evaluate_existing(ty),
            _ => {
                let children = query_children(self.interner.store(), ty);
                for child in children {
                    self.visit(child);
                }
                ty
            }
        };

        self.visiting.remove(&ty);
        self.visited.insert(ty);
        result
    }

    fn project_class(&mut self, application: TypeId) -> TypeId {
        let projection = self.project_class_outer(application);
        self.visit(projection);
        projection
    }

    fn project_class_outer(&mut self, application: TypeId) -> TypeId {
        if !self.admitted_applications.contains(&application) {
            if self.admitted_applications.len() >= MAX_CLASS_PROJECTION_EXPANSIONS {
                self.mark_frontier(application, Exhaustion::ClassProjectionBudget);
                return application;
            }
            self.admitted_applications.insert(application);
        }

        let Some(instance) = self
            .interner
            .store()
            .class_instance_type(application)
            .cloned()
        else {
            return application;
        };
        let surface = match self.published.published_class(instance.class) {
            DemandOutcome::Ready(surface) => surface,
            DemandOutcome::Exhausted(reason) => {
                self.mark_frontier(application, reason);
                return application;
            }
        };
        assert_eq!(
            surface.type_params().len(),
            instance.args.len(),
            "complete ClassInstance vectors must match the published declaration arity"
        );

        let projection = self
            .durable_projection_memo
            .get(&application)
            .copied()
            .unwrap_or_else(|| {
                let map = surface
                    .type_params()
                    .iter()
                    .copied()
                    .zip(instance.args.iter().copied())
                    .collect();
                let projection = substitute(self.interner, surface.instance_template(), &map);
                self.pending_projection_writes
                    .insert(application, projection);
                projection
            });
        if projection != application {
            self.plan
                .class_projection_overlay
                .insert(application, projection);
        }
        projection
    }

    fn evaluate_deferred_indexed(&mut self, ty: TypeId) -> TypeId {
        let Some(access) = self
            .interner
            .store()
            .deferred_indexed_access_type(ty)
            .copied()
        else {
            return ty;
        };
        let shallow = resolve_deferred_outer_layer(self.interner, ty);
        if shallow != ty {
            self.record_evaluation(ty, shallow);
            self.visit_demand_result(shallow);
            return shallow;
        }
        let mut object = self.visit(access.object);
        if let Some(constraint) = self
            .interner
            .store()
            .type_param(object)
            .and_then(|parameter| self.interner.store().type_param_constraint(parameter.id))
            .filter(|constraint| *constraint != object)
        {
            object = self.visit(constraint);
        }
        let index = self.visit(access.index);
        let object = match self.plan.normalize(object) {
            Ok(object) => object,
            Err(reason) => {
                self.mark_frontier(ty, reason);
                return ty;
            }
        };
        let index = match self.plan.normalize(index) {
            Ok(index) => index,
            Err(reason) => {
                self.mark_frontier(ty, reason);
                return ty;
            }
        };
        let rebuilt = if object == access.object && index == access.index {
            ty
        } else {
            self.interner.intern_deferred_indexed_access(object, index)
        };
        let result = resolve_deferred_outer_layer(self.interner, rebuilt);
        self.record_evaluation(ty, result);
        self.visit_demand_result(result);
        result
    }

    fn evaluate_keyof(&mut self, ty: TypeId) -> TypeId {
        let Some(operand) = self.interner.store().keyof_operand(ty) else {
            return ty;
        };
        let shallow = resolve_keyof_outer_layer(self.interner, operand);
        if shallow != ty {
            self.record_evaluation(ty, shallow);
            self.visit_demand_result(shallow);
            return shallow;
        }
        let operand = self.visit(operand);
        let operand = match self.plan.normalize(operand) {
            Ok(operand) => operand,
            Err(reason) => {
                self.mark_frontier(ty, reason);
                return ty;
            }
        };
        let result = resolve_keyof_outer_layer(self.interner, operand);
        self.record_evaluation(ty, result);
        self.visit_demand_result(result);
        result
    }

    fn evaluate_existing(&mut self, ty: TypeId) -> TypeId {
        #[cfg(test)]
        measure_query_demand(|measure| measure.pending_evaluations += 1);
        if let Some(&result) = self.durable_evaluation_memo.get(&ty) {
            #[cfg(test)]
            measure_query_demand(|measure| measure.durable_evaluation_hits += 1);
            self.record_evaluation(ty, result);
            self.visit_demand_result(result);
            return result;
        }

        let children = query_children(self.interner.store(), ty);
        for child in children {
            self.visit(child);
        }

        if self.evaluation_expansions >= DEFAULT_STEP_BUDGET {
            self.mark_frontier(ty, Exhaustion::EvaluationBudget);
            return ty;
        }
        self.evaluation_expansions += 1;
        #[cfg(test)]
        measure_query_demand(|measure| measure.evaluation_expansions += 1);

        let (outcome, exhausted, cycle_detected) = {
            let mut evaluator = ConditionalEvaluator::new(
                self.interner,
                &mut self.next_type_param,
                &mut self.working_evaluation_memo,
                DEFAULT_STEP_BUDGET,
            );
            let outcome = evaluator.evaluate_planned(ty, &self.plan);
            (outcome, evaluator.exhausted, evaluator.cycle_detected)
        };
        if exhausted {
            self.mark_frontier(ty, Exhaustion::EvaluationBudget);
            return ty;
        }
        if cycle_detected {
            self.mark_frontier(ty, Exhaustion::EvaluationCycle { ty });
            return ty;
        }
        let result = match outcome {
            DemandOutcome::Ready(result) => result,
            DemandOutcome::Exhausted(reason) => {
                self.mark_frontier(ty, reason);
                return ty;
            }
        };
        #[cfg(test)]
        measure_query_demand(|measure| {
            if result == ty {
                measure.evaluation_identity_returns += 1;
            } else {
                measure.evaluation_changed_returns += 1;
            }
        });
        self.record_evaluation(ty, result);
        self.visit_demand_result(result);
        result
    }

    fn visit_demand_result(&mut self, result: TypeId) {
        if self.demand_outer_only {
            match self.interner.store().tag(result) {
                TypeTag::ClassInstance => {
                    self.project_class_outer(result);
                }
                TypeTag::DeferredIndexedAccess => {
                    self.visit(result);
                }
                _ => {}
            }
        } else {
            self.visit(result);
        }
    }

    fn record_evaluation(&mut self, source: TypeId, result: TypeId) {
        self.plan.resolved_evaluations.insert(source);
        #[cfg(test)]
        measure_query_demand(|measure| measure.evaluation_memo_inserts += 1);
        self.working_evaluation_memo.insert(source, result);
        if source != result {
            self.plan.evaluation_overlay.insert(source, result);
        }
    }

    fn mark_frontier(&mut self, ty: TypeId, reason: Exhaustion) {
        #[cfg(test)]
        measure_query_demand(|measure| {
            measure.exhaustion_frontiers += 1;
            match &reason {
                Exhaustion::EvaluationBudget => measure.evaluation_budget_exhaustions += 1,
                Exhaustion::EvaluationCycle { .. } => measure.evaluation_cycle_exhaustions += 1,
                _ => {}
            }
        });
        #[cfg(test)]
        measure_query_source_cold(|measure| measure.exhaustion_frontiers += 1);
        self.planning_tainted = true;
        if self.first_exhaustion.is_none() {
            self.first_exhaustion = Some(reason.clone());
        }
        self.plan.frontier.entry(ty).or_insert(reason);
    }
}

fn publication_exhaustion<L: PublishedClassLookup + ?Sized>(
    store: &Store,
    roots: &[TypeId],
    published: &L,
    state: &mut SemanticQueryState,
) -> Option<Exhaustion> {
    #[cfg(test)]
    measure_query_source_cold(|measure| {
        measure.publication_calls += 1;
        measure.publication_query_roots += u64::try_from(roots.len()).unwrap();
    });
    let store_identity = store.semantic_graph_identity();
    let publication_identity = published.publication_identity();
    let same_store = state
        .publication_store_identity
        .as_ref()
        .is_some_and(|identity| Arc::ptr_eq(identity, store_identity));
    let same_publication = state
        .publication_snapshot_identity
        .as_ref()
        .is_some_and(|identity| Arc::ptr_eq(identity, publication_identity));
    if !same_store || !same_publication {
        state.publication_clean.clear();
        state.publication_store_identity = Some(Arc::clone(store_identity));
        state.publication_snapshot_identity = Some(Arc::clone(publication_identity));
    }

    let mut stack = roots.to_vec();
    let mut seen = FxHashSet::default();
    while let Some(ty) = stack.pop() {
        if state.publication_clean.contains(&ty) || !seen.insert(ty) {
            continue;
        }
        if let Some(instance) = store.class_instance_type(ty) {
            if let DemandOutcome::Exhausted(reason) = published.published_class(instance.class) {
                return Some(reason);
            }
        }
        let children = query_children(store, ty);
        #[cfg(test)]
        measure_publication_children(ty, &children);
        stack.extend(children);
    }
    state.publication_clean.extend(seen);
    None
}

fn query_children(store: &Store, ty: TypeId) -> Vec<TypeId> {
    match store.tag(ty) {
        TypeTag::Object => store.object_type(ty).map_or_else(Vec::new, |object| {
            let mut children = Vec::new();
            for property in &object.properties {
                children.push(property.ty);
                children.extend(property.write_ty);
            }
            children.extend(object.string_index);
            children.extend(object.number_index);
            children.extend(object.call_signatures.iter().copied());
            children.extend(object.construct_signatures.iter().copied());
            children
        }),
        TypeTag::Function => store.function_type(ty).map_or_else(Vec::new, |function| {
            let mut children = Vec::new();
            children.extend(
                function
                    .type_params
                    .iter()
                    .flat_map(|parameter| [parameter.constraint, parameter.default])
                    .flatten(),
            );
            children.extend(function.receiver);
            children.extend(function.params.iter().map(|parameter| parameter.ty));
            children.push(function.ret);
            children
        }),
        TypeTag::TypeParam => store
            .type_param(ty)
            .and_then(|parameter| store.type_param_constraint(parameter.id))
            .into_iter()
            .collect(),
        TypeTag::Union => store
            .union_members(ty)
            .map_or_else(Vec::new, |members| members.to_vec()),
        TypeTag::Intersection => store
            .intersection_members(ty)
            .map_or_else(Vec::new, |members| members.to_vec()),
        TypeTag::Array => store
            .array_type(ty)
            .map_or_else(Vec::new, |array| vec![array.element]),
        TypeTag::Tuple => store.tuple_type(ty).map_or_else(Vec::new, |tuple| {
            let mut children = tuple.elements.clone();
            children.extend(tuple.rest.map(|rest| rest.ty));
            children
        }),
        TypeTag::Readonly => store.readonly_operand(ty).into_iter().collect(),
        TypeTag::Conditional => store
            .conditional_type(ty)
            .map_or_else(Vec::new, |conditional| {
                vec![
                    conditional.check,
                    conditional.extends_ty,
                    conditional.true_branch,
                    conditional.false_branch,
                ]
            }),
        TypeTag::Instantiation => {
            store
                .instantiation_type(ty)
                .map_or_else(Vec::new, |instantiation| {
                    let mut children = vec![instantiation.base];
                    children.extend(instantiation.args.iter().map(|(_, argument)| *argument));
                    children
                })
        }
        TypeTag::ClassInstance => store
            .class_instance_type(ty)
            .map_or_else(Vec::new, |instance| instance.args.clone()),
        TypeTag::Mapped => store.mapped_type(ty).map_or_else(Vec::new, |mapped| {
            let mut children = vec![mapped.key_source, mapped.value_template];
            children.extend(mapped.modifiers_source);
            children
        }),
        TypeTag::Template => store
            .template_type(ty)
            .map_or_else(Vec::new, |template| template.holes.clone()),
        TypeTag::Keyof => store.keyof_operand(ty).into_iter().collect(),
        TypeTag::DeferredIndexedAccess => store
            .deferred_indexed_access_type(ty)
            .map_or_else(Vec::new, |access| vec![access.object, access.index]),
        TypeTag::Intrinsic | TypeTag::Literal | TypeTag::Infer | TypeTag::MappedValue => Vec::new(),
    }
}

#[cfg(test)]
mod dom_source_cold_spec;

#[cfg(test)]
mod tests;

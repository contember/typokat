//! Coordinator-owned semantic queries for immutable class applications.
//!
//! Planning owns the mutable interner. Relation sees only the immutable store
//! plus query-local normalization overlays, and durable writes promote together.

use crate::check::checker::eval::demand::{
    index_requires_planner_visit, object_requires_demand, resolve_deferred_outer_layer,
    resolve_keyof_outer_layer,
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
type CompletedIdentityKey = (TypeId, TypeId);
type IdentityProgressKey = (TypeId, TypeId, AlphaBinderKey);

#[derive(Clone, Default)]
struct IdentityRetryState {
    sequences: FxHashMap<IdentityProgressKey, IdentitySequenceProgress>,
    coinductive_epoch: u64,
}

#[derive(Clone, Default)]
struct IdentitySequenceProgress {
    cursor: usize,
    recheck: Vec<usize>,
}

enum IdentityAttempt {
    Decided(DemandOutcome<bool>),
    Needs(RelationDemand),
}

enum ExactFamilyAttempt {
    Ready(TypeId),
    Needs(RelationDemand),
    Exhausted(Exhaustion),
}

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
    pub planner_visits: u64,
    pub planner_clean_finishes: u64,
    pub planner_tainted_finishes: u64,
    pub planner_zero_write_finishes: u64,
    pub planner_commits: u64,
    pub completed_relation_yes_hits: u64,
    pub completed_relation_no_hits: u64,
    pub completed_relation_yes_inserts: u64,
    pub completed_relation_no_inserts: u64,
    pub durable_identity_yes_hits: u64,
    pub durable_identity_no_hits: u64,
    pub durable_identity_yes_inserts: u64,
    pub durable_identity_no_inserts: u64,
    pub identity_recursive_calls: u64,
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
    static PROJECTION_CACHE_WRITES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static EVALUATOR_CACHE_WRITES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueryCacheWritesForTest {
    pub(crate) projection: u64,
    pub(crate) evaluator: u64,
}

#[cfg(test)]
pub(crate) struct QueryCacheWriteScopeForTest(QueryCacheWritesForTest);

#[cfg(test)]
impl QueryCacheWriteScopeForTest {
    pub(crate) fn start() -> Self {
        Self(QueryCacheWritesForTest {
            projection: PROJECTION_CACHE_WRITES.get(),
            evaluator: EVALUATOR_CACHE_WRITES.get(),
        })
    }

    pub(crate) fn finish(self) -> QueryCacheWritesForTest {
        QueryCacheWritesForTest {
            projection: PROJECTION_CACHE_WRITES
                .get()
                .saturating_sub(self.0.projection),
            evaluator: EVALUATOR_CACHE_WRITES
                .get()
                .saturating_sub(self.0.evaluator),
        }
    }
}

#[cfg(test)]
pub(crate) fn calibrate_query_cache_writes_for_test() -> QueryCacheWritesForTest {
    let scope = QueryCacheWriteScopeForTest::start();
    let mut projection = FxHashMap::default();
    let mut evaluator = FxHashMap::default();
    commit_query_cache_entries(
        &mut projection,
        &mut evaluator,
        FxHashMap::from_iter([(TypeId(1), TypeId(2))]),
        FxHashMap::from_iter([(TypeId(3), TypeId(4))]),
    );
    scope.finish()
}

fn commit_query_cache_entries(
    projection: &mut FxHashMap<TypeId, TypeId>,
    evaluator: &mut FxHashMap<TypeId, TypeId>,
    pending_projection: FxHashMap<TypeId, TypeId>,
    pending_evaluator: FxHashMap<TypeId, TypeId>,
) {
    #[cfg(test)]
    {
        PROJECTION_CACHE_WRITES.set(
            PROJECTION_CACHE_WRITES
                .get()
                .saturating_add(u64::try_from(pending_projection.len()).unwrap_or(u64::MAX)),
        );
        EVALUATOR_CACHE_WRITES.set(
            EVALUATOR_CACHE_WRITES
                .get()
                .saturating_add(u64::try_from(pending_evaluator.len()).unwrap_or(u64::MAX)),
        );
    }
    projection.extend(pending_projection);
    evaluator.extend(pending_evaluator);
}

#[cfg(test)]
mod cache_write_calibration_tests {
    use super::*;

    #[test]
    fn calibration_exercises_projection_and_evaluator_commit_hooks() {
        assert_eq!(
            calibrate_query_cache_writes_for_test(),
            QueryCacheWritesForTest {
                projection: 1,
                evaluator: 1,
            }
        );
    }
}

#[cfg(test)]
pub(crate) struct QuerySourceColdMeasureGuard {
    _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    _relation_guard: crate::relate::relation::RelationSourceColdMeasureGuard,
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
    let relation_guard = crate::relate::relation::start_relation_source_cold_measure();
    QuerySourceColdMeasureGuard {
        _not_send: std::marker::PhantomData,
        _relation_guard: relation_guard,
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
    fn require_class(&self, class: ClassId) -> DemandOutcome<()>;
    fn publication_identity(&self) -> &Arc<()>;
    fn observe_class_demand(&self, _class: ClassId) {}
    fn class_demand_observation_enabled(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum CompletedRelationOperation {
    Assignable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CompletedRelationKey {
    src: TypeId,
    tgt: TypeId,
    operation: CompletedRelationOperation,
}

impl CompletedRelationKey {
    fn assignable(src: TypeId, tgt: TypeId) -> Self {
        Self {
            src,
            tgt,
            operation: CompletedRelationOperation::Assignable,
        }
    }
}

#[derive(Clone)]
enum CompletedRelationOutcome {
    Yes,
    No(Arc<ReasonChain>),
}

impl CompletedRelationOutcome {
    fn from_outcome(outcome: &RelationOutcome) -> Option<Self> {
        match outcome {
            RelationOutcome::Yes => Some(Self::Yes),
            RelationOutcome::No(reason) => Some(Self::No(Arc::clone(reason))),
            RelationOutcome::Exhausted(_) => None,
        }
    }

    fn outcome(&self) -> RelationOutcome {
        match self {
            Self::Yes => RelationOutcome::Yes,
            Self::No(reason) => RelationOutcome::No(Arc::clone(reason)),
        }
    }
}

impl PublishedClassLookup for PublishedClasses {
    fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
        PublishedClasses::published_class(self, class)
    }

    fn require_class(&self, class: ClassId) -> DemandOutcome<()> {
        self.require(class)
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
    completed_identities: FxHashMap<CompletedIdentityKey, bool>,
    relation_cache: RelationCache,
    completed_relations: FxHashMap<CompletedRelationKey, CompletedRelationOutcome>,
    completed_relation_no_candidates: FxHashSet<CompletedRelationKey>,
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
    pub(crate) fn durable_lengths(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.projection_memo.len(),
            self.evaluation_memo.len(),
            self.relation_cache.len(),
            self.completed_relations.len(),
            self.completed_relation_no_candidates.len(),
        )
    }

    #[cfg(test)]
    pub(crate) fn completed_relation_len(&self) -> usize {
        self.completed_relations.len()
    }

    #[cfg(test)]
    pub(crate) fn completed_relation_no_candidate_len(&self) -> usize {
        self.completed_relation_no_candidates.len()
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
        refresh_semantic_context(interner.store(), published, state);
        SemanticQueryCoordinator {
            interner,
            published,
            state,
            next_type_param,
        }
    }

    fn observe_outer_class(&self, ty: TypeId) {
        if !self.published.class_demand_observation_enabled() {
            return;
        }
        if let Some(application) = self.interner.store().class_instance_type(ty) {
            self.published.observe_class_demand(application.class);
        }
    }

    /// Demand one normalized outer shape. Successful untainted work promotes
    /// projection/evaluator memo entries together; exhaustion promotes nothing.
    pub(crate) fn demand(&mut self, root: TypeId) -> DemandOutcome<TypeId> {
        self.observe_outer_class(root);
        #[cfg(test)]
        measure_query_demand(|measure| measure.root_calls += 1);
        if matches!(
            self.interner.store().tag(root),
            TypeTag::Intrinsic
                | TypeTag::Literal
                | TypeTag::TypeParam
                | TypeTag::Infer
                | TypeTag::MappedValue
                | TypeTag::Object
                | TypeTag::Intersection
                | TypeTag::Function
                | TypeTag::Array
                | TypeTag::Tuple
                | TypeTag::Readonly
        ) {
            return DemandOutcome::Ready(root);
        }
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
                self.commit_plan(transaction.into_commit());
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
        self.observe_outer_class(root);
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
        self.observe_outer_class(left);
        self.observe_outer_class(right);
        if let Some(reason) = publication_exhaustion(
            self.interner.store(),
            &[left, right],
            self.published,
            self.state,
        ) {
            return DemandOutcome::Exhausted(reason);
        }
        if left == right {
            return DemandOutcome::Ready(true);
        }
        let completed_key = Self::completed_identity_key(left, right);
        if let Some(&identical) = self.state.completed_identities.get(&completed_key) {
            #[cfg(test)]
            measure_query_source_cold(|measure| {
                if identical {
                    measure.durable_identity_yes_hits += 1;
                } else {
                    measure.durable_identity_no_hits += 1;
                }
            });
            return DemandOutcome::Ready(identical);
        }
        let mut planner = ProjectionPlanner::new(
            self.interner,
            self.published,
            &self.state.projection_memo,
            &self.state.evaluation_memo,
            *self.next_type_param,
        );
        let mut retry = IdentityRetryState::default();
        let outcome = loop {
            let attempt = Self::identical_retry_attempt(
                planner.interner.store(),
                &planner.plan,
                left,
                right,
                &mut FxHashSet::default(),
                &mut Vec::new(),
                &mut retry,
            );
            match attempt {
                IdentityAttempt::Decided(outcome) => break outcome,
                IdentityAttempt::Needs(demand) => planner.expand_relation_demand(demand),
            }
        };
        let transaction = planner.finish();
        *self.next_type_param = transaction.next_type_param;
        if transaction.planning_tainted {
            return DemandOutcome::Exhausted(
                transaction
                    .first_exhaustion
                    .clone()
                    .unwrap_or(Exhaustion::ClassProjectionBudget),
            );
        }
        if let DemandOutcome::Ready(identical) = &outcome {
            self.commit_plan(transaction.into_commit());
            #[cfg(test)]
            measure_query_source_cold(|measure| {
                if *identical {
                    measure.durable_identity_yes_inserts += 1;
                } else {
                    measure.durable_identity_no_inserts += 1;
                }
            });
            self.state
                .completed_identities
                .insert(completed_key, *identical);
        }
        outcome
    }

    fn completed_identity_key(left: TypeId, right: TypeId) -> CompletedIdentityKey {
        if left <= right {
            (left, right)
        } else {
            (right, left)
        }
    }

    #[cfg(test)]
    fn identical_attempt(
        store: &Store,
        plan: &ProjectionPlan<'_>,
        left: TypeId,
        right: TypeId,
        seen: &mut IdentitySeen,
        alpha_binders: &mut Vec<(TypeParamId, TypeParamId)>,
    ) -> IdentityAttempt {
        Self::identical_retry_attempt(
            store,
            plan,
            left,
            right,
            seen,
            alpha_binders,
            &mut IdentityRetryState::default(),
        )
    }

    fn identical_retry_attempt(
        store: &Store,
        plan: &ProjectionPlan<'_>,
        left: TypeId,
        right: TypeId,
        seen: &mut IdentitySeen,
        alpha_binders: &mut Vec<(TypeParamId, TypeParamId)>,
        retry: &mut IdentityRetryState,
    ) -> IdentityAttempt {
        #[cfg(test)]
        measure_query_source_cold(|measure| measure.identity_recursive_calls += 1);
        let mut left = match plan.normalize(left) {
            Ok(left) => left,
            Err(exhaustion) => {
                return IdentityAttempt::Decided(DemandOutcome::Exhausted(exhaustion))
            }
        };
        let mut right = match plan.normalize(right) {
            Ok(right) => right,
            Err(exhaustion) => {
                return IdentityAttempt::Decided(DemandOutcome::Exhausted(exhaustion))
            }
        };
        if let (Some(left_param), Some(right_param)) =
            (store.type_param(left), store.type_param(right))
        {
            return IdentityAttempt::Decided(DemandOutcome::Ready(
                Self::alpha_type_params_identical(left_param.id, right_param.id, alpha_binders),
            ));
        }
        if left == right {
            return IdentityAttempt::Decided(DemandOutcome::Ready(true));
        }
        left = match Self::prepare_exact_family_root(store, plan, left) {
            ExactFamilyAttempt::Ready(left) => left,
            ExactFamilyAttempt::Needs(demand) => return IdentityAttempt::Needs(demand),
            ExactFamilyAttempt::Exhausted(exhaustion) => {
                return IdentityAttempt::Decided(DemandOutcome::Exhausted(exhaustion))
            }
        };
        right = match Self::prepare_exact_family_root(store, plan, right) {
            ExactFamilyAttempt::Ready(right) => right,
            ExactFamilyAttempt::Needs(demand) => return IdentityAttempt::Needs(demand),
            ExactFamilyAttempt::Exhausted(exhaustion) => {
                return IdentityAttempt::Decided(DemandOutcome::Exhausted(exhaustion))
            }
        };
        if let (Some(left_param), Some(right_param)) =
            (store.type_param(left), store.type_param(right))
        {
            return IdentityAttempt::Decided(DemandOutcome::Ready(
                Self::alpha_type_params_identical(left_param.id, right_param.id, alpha_binders),
            ));
        }
        if left == right {
            return IdentityAttempt::Decided(DemandOutcome::Ready(true));
        }
        if let Some(demand) = plan
            .relation_demand(store, left)
            .or_else(|| plan.relation_demand(store, right))
        {
            return IdentityAttempt::Needs(demand);
        }
        let progress_key = (left, right, Self::alpha_binder_key(alpha_binders));
        if !seen.insert(progress_key.clone()) {
            retry.coinductive_epoch = retry.coinductive_epoch.saturating_add(1);
            return IdentityAttempt::Decided(DemandOutcome::Ready(true));
        }
        let tag = store.tag(left);
        if tag != store.tag(right) {
            return IdentityAttempt::Decided(DemandOutcome::Ready(false));
        }
        match tag {
            TypeTag::Object => {
                let left = store.object_type(left).expect("object tag has payload");
                let right = store.object_type(right).expect("object tag has payload");
                if left.properties.len() != right.properties.len()
                    || left.call_signatures.len() != right.call_signatures.len()
                    || left.construct_signatures.len() != right.construct_signatures.len()
                {
                    return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                }
                let property_end = left.properties.len();
                let call_end = property_end + left.call_signatures.len();
                let construct_end = call_end + left.construct_signatures.len();
                let string_index = construct_end;
                let number_index = string_index + 1;
                Self::identical_sequence(retry, progress_key, number_index + 1, |index, retry| {
                    if index < property_end {
                        let left = &left.properties[index];
                        let right = &right.properties[index];
                        if left.name != right.name
                            || left.optional != right.optional
                            || left.visibility != right.visibility
                            || (left.visibility != Visibility::Public
                                && left.declaring_class != right.declaring_class)
                            || left.readonly != right.readonly
                            || left.is_accessor != right.is_accessor
                        {
                            return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                        }
                        match Self::identical_retry_attempt(
                            store,
                            plan,
                            left.ty,
                            right.ty,
                            seen,
                            alpha_binders,
                            retry,
                        ) {
                            IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {}
                            outcome => return outcome,
                        }
                        return Self::identical_optional(
                            store,
                            plan,
                            left.write_ty,
                            right.write_ty,
                            seen,
                            alpha_binders,
                            retry,
                        );
                    }
                    if index < call_end {
                        let index = index - property_end;
                        return Self::identical_retry_attempt(
                            store,
                            plan,
                            left.call_signatures[index],
                            right.call_signatures[index],
                            seen,
                            alpha_binders,
                            retry,
                        );
                    }
                    if index < construct_end {
                        let index = index - call_end;
                        return Self::identical_retry_attempt(
                            store,
                            plan,
                            left.construct_signatures[index],
                            right.construct_signatures[index],
                            seen,
                            alpha_binders,
                            retry,
                        );
                    }
                    let (left, right) = if index == string_index {
                        (left.string_index, right.string_index)
                    } else {
                        debug_assert_eq!(index, number_index);
                        (left.number_index, right.number_index)
                    };
                    Self::identical_optional(store, plan, left, right, seen, alpha_binders, retry)
                })
            }
            TypeTag::Function => {
                let left = store.function_type(left).expect("function tag has payload");
                let right = store
                    .function_type(right)
                    .expect("function tag has payload");
                if left.type_params.len() != right.type_params.len()
                    || left.params.len() != right.params.len()
                {
                    return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                }
                let binder_start = alpha_binders.len();
                alpha_binders.extend(
                    left.type_params
                        .iter()
                        .zip(&right.type_params)
                        .map(|(left, right)| (left.id, right.id)),
                );
                let type_parameter_end = left.type_params.len() * 2;
                let receiver_index = type_parameter_end;
                let parameter_start = receiver_index + 1;
                let return_index = parameter_start + left.params.len();
                let function_progress_key = (
                    progress_key.0,
                    progress_key.1,
                    Self::alpha_binder_key(alpha_binders),
                );
                let outcome = Self::identical_sequence(
                    retry,
                    function_progress_key,
                    return_index + 1,
                    |index, retry| {
                        if index < type_parameter_end {
                            let parameter = index / 2;
                            let left = &left.type_params[parameter];
                            let right = &right.type_params[parameter];
                            let (left, right) = if index % 2 == 0 {
                                (left.constraint, right.constraint)
                            } else {
                                (left.default, right.default)
                            };
                            return Self::identical_optional(
                                store,
                                plan,
                                left,
                                right,
                                seen,
                                alpha_binders,
                                retry,
                            );
                        }
                        if index == receiver_index {
                            return Self::identical_optional(
                                store,
                                plan,
                                left.receiver,
                                right.receiver,
                                seen,
                                alpha_binders,
                                retry,
                            );
                        }
                        if index < return_index {
                            let parameter = index - parameter_start;
                            let left = &left.params[parameter];
                            let right = &right.params[parameter];
                            if left.optional != right.optional
                                || left.has_default != right.has_default
                                || left.rest != right.rest
                            {
                                return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                            }
                            return Self::identical_retry_attempt(
                                store,
                                plan,
                                left.ty,
                                right.ty,
                                seen,
                                alpha_binders,
                                retry,
                            );
                        }
                        debug_assert_eq!(index, return_index);
                        Self::identical_retry_attempt(
                            store,
                            plan,
                            left.ret,
                            right.ret,
                            seen,
                            alpha_binders,
                            retry,
                        )
                    },
                );
                alpha_binders.truncate(binder_start);
                outcome
            }
            TypeTag::Array => {
                let left = store.array_type(left).unwrap().element;
                let right = store.array_type(right).unwrap().element;
                Self::identical_retry_attempt(store, plan, left, right, seen, alpha_binders, retry)
            }
            TypeTag::Tuple => {
                let left = store.tuple_type(left).unwrap().clone();
                let right = store.tuple_type(right).unwrap().clone();
                if left.elements.len() != right.elements.len()
                    || left.rest.map(|rest| rest.position) != right.rest.map(|rest| rest.position)
                {
                    return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                }
                for (left, right) in left.elements.iter().zip(&right.elements) {
                    match Self::identical_retry_attempt(
                        store,
                        plan,
                        *left,
                        *right,
                        seen,
                        alpha_binders,
                        retry,
                    ) {
                        IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {}
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
                    retry,
                )
            }
            TypeTag::Readonly => Self::identical_retry_attempt(
                store,
                plan,
                store.readonly_operand(left).unwrap(),
                store.readonly_operand(right).unwrap(),
                seen,
                alpha_binders,
                retry,
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
                    Err(exhaustion) => {
                        return IdentityAttempt::Decided(DemandOutcome::Exhausted(exhaustion))
                    }
                };
                let right = match Self::flatten_normalized_family(store, plan, tag, &right) {
                    Ok(right) => right,
                    Err(exhaustion) => {
                        return IdentityAttempt::Decided(DemandOutcome::Exhausted(exhaustion))
                    }
                };
                Self::identical_unordered(store, plan, &left, &right, seen, alpha_binders, retry)
            }
            TypeTag::ClassInstance => {
                let left = store.class_instance_type(left).unwrap().clone();
                let right = store.class_instance_type(right).unwrap().clone();
                if left.class != right.class || left.args.len() != right.args.len() {
                    return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                }
                for (left, right) in left.args.into_iter().zip(right.args) {
                    match Self::identical_retry_attempt(
                        store,
                        plan,
                        left,
                        right,
                        seen,
                        alpha_binders,
                        retry,
                    ) {
                        IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {}
                        outcome => return outcome,
                    }
                }
                IdentityAttempt::Decided(DemandOutcome::Ready(true))
            }
            TypeTag::Conditional => {
                let left = *store.conditional_type(left).expect("conditional payload");
                let right = *store.conditional_type(right).expect("conditional payload");
                if left.infer_count != right.infer_count
                    || left.distributive != right.distributive
                    || left.poisoned != right.poisoned
                {
                    return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                }
                for (left, right) in [
                    (left.check, right.check),
                    (left.extends_ty, right.extends_ty),
                    (left.true_branch, right.true_branch),
                    (left.false_branch, right.false_branch),
                ] {
                    match Self::identical_retry_attempt(
                        store,
                        plan,
                        left,
                        right,
                        seen,
                        alpha_binders,
                        retry,
                    ) {
                        IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {}
                        outcome => return outcome,
                    }
                }
                IdentityAttempt::Decided(DemandOutcome::Ready(true))
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
                    return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                }
                match Self::identical_retry_attempt(
                    store,
                    plan,
                    left.base,
                    right.base,
                    seen,
                    alpha_binders,
                    retry,
                ) {
                    IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {}
                    outcome => return outcome,
                }
                let mut remaining = right.args;
                for (left_key, left_value) in left.args {
                    let Some(position) = remaining.iter().position(|(right_key, _)| {
                        Self::alpha_type_params_identical(left_key, *right_key, alpha_binders)
                    }) else {
                        return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                    };
                    let (_, right_value) = remaining.remove(position);
                    match Self::identical_retry_attempt(
                        store,
                        plan,
                        left_value,
                        right_value,
                        seen,
                        alpha_binders,
                        retry,
                    ) {
                        IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {}
                        outcome => return outcome,
                    }
                }
                IdentityAttempt::Decided(DemandOutcome::Ready(true))
            }
            TypeTag::Mapped => {
                let left = *store.mapped_type(left).expect("mapped payload");
                let right = *store.mapped_type(right).expect("mapped payload");
                if left.homomorphic != right.homomorphic
                    || left.optional_modifier != right.optional_modifier
                    || left.readonly_modifier != right.readonly_modifier
                {
                    return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                }
                for (left, right) in [
                    (Some(left.key_source), Some(right.key_source)),
                    (Some(left.value_template), Some(right.value_template)),
                    (left.modifiers_source, right.modifiers_source),
                ] {
                    match Self::identical_optional(
                        store,
                        plan,
                        left,
                        right,
                        seen,
                        alpha_binders,
                        retry,
                    ) {
                        IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {}
                        outcome => return outcome,
                    }
                }
                IdentityAttempt::Decided(DemandOutcome::Ready(true))
            }
            TypeTag::Template => {
                let left = store.template_type(left).expect("template payload").clone();
                let right = store
                    .template_type(right)
                    .expect("template payload")
                    .clone();
                if left.texts != right.texts || left.holes.len() != right.holes.len() {
                    return IdentityAttempt::Decided(DemandOutcome::Ready(false));
                }
                for (left, right) in left.holes.into_iter().zip(right.holes) {
                    match Self::identical_retry_attempt(
                        store,
                        plan,
                        left,
                        right,
                        seen,
                        alpha_binders,
                        retry,
                    ) {
                        IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {}
                        outcome => return outcome,
                    }
                }
                IdentityAttempt::Decided(DemandOutcome::Ready(true))
            }
            TypeTag::Keyof => Self::identical_retry_attempt(
                store,
                plan,
                store.keyof_operand(left).expect("keyof payload"),
                store.keyof_operand(right).expect("keyof payload"),
                seen,
                alpha_binders,
                retry,
            ),
            TypeTag::DeferredIndexedAccess => {
                let left = *store
                    .deferred_indexed_access_type(left)
                    .expect("indexed-access payload");
                let right = *store
                    .deferred_indexed_access_type(right)
                    .expect("indexed-access payload");
                match Self::identical_retry_attempt(
                    store,
                    plan,
                    left.object,
                    right.object,
                    seen,
                    alpha_binders,
                    retry,
                ) {
                    IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {
                        Self::identical_retry_attempt(
                            store,
                            plan,
                            left.index,
                            right.index,
                            seen,
                            alpha_binders,
                            retry,
                        )
                    }
                    outcome => outcome,
                }
            }
            TypeTag::Infer => IdentityAttempt::Decided(DemandOutcome::Ready(
                store.infer_index(left).expect("infer payload")
                    == store.infer_index(right).expect("infer payload"),
            )),
            TypeTag::MappedValue => IdentityAttempt::Decided(DemandOutcome::Ready(true)),
            _ => IdentityAttempt::Decided(DemandOutcome::Ready(false)),
        }
    }

    fn identical_sequence(
        retry: &mut IdentityRetryState,
        key: IdentityProgressKey,
        len: usize,
        mut compare: impl FnMut(usize, &mut IdentityRetryState) -> IdentityAttempt,
    ) -> IdentityAttempt {
        let mut progress = retry.sequences.get(&key).cloned().unwrap_or_default();
        for index in progress.recheck.clone() {
            let coinductive_epoch = retry.coinductive_epoch;
            match compare(index, retry) {
                IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {
                    if retry.coinductive_epoch == coinductive_epoch {
                        progress.recheck.retain(|candidate| *candidate != index);
                    }
                }
                outcome => {
                    retry.sequences.insert(key, progress);
                    return outcome;
                }
            }
        }
        while progress.cursor < len {
            let index = progress.cursor;
            let coinductive_epoch = retry.coinductive_epoch;
            match compare(index, retry) {
                IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {
                    progress.cursor += 1;
                    if retry.coinductive_epoch != coinductive_epoch {
                        progress.recheck.push(index);
                    }
                }
                outcome => {
                    retry.sequences.insert(key, progress);
                    return outcome;
                }
            }
        }
        retry.sequences.insert(key, progress);
        IdentityAttempt::Decided(DemandOutcome::Ready(true))
    }

    fn identical_optional(
        store: &Store,
        plan: &ProjectionPlan<'_>,
        left: Option<TypeId>,
        right: Option<TypeId>,
        seen: &mut IdentitySeen,
        alpha_binders: &mut Vec<(TypeParamId, TypeParamId)>,
        retry: &mut IdentityRetryState,
    ) -> IdentityAttempt {
        match (left, right) {
            (Some(left), Some(right)) => {
                Self::identical_retry_attempt(store, plan, left, right, seen, alpha_binders, retry)
            }
            (None, None) => IdentityAttempt::Decided(DemandOutcome::Ready(true)),
            _ => IdentityAttempt::Decided(DemandOutcome::Ready(false)),
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
        plan: &ProjectionPlan<'_>,
        left: &[TypeId],
        right: &[TypeId],
        seen: &mut IdentitySeen,
        alpha_binders: &[(TypeParamId, TypeParamId)],
        retry: &mut IdentityRetryState,
    ) -> IdentityAttempt {
        if left.len() != right.len() {
            return IdentityAttempt::Decided(DemandOutcome::Ready(false));
        }
        let mut remaining = right.to_vec();
        for &candidate in left {
            let mut matched = None;
            for (position, &target) in remaining.iter().enumerate() {
                let mut trial_seen = seen.clone();
                let mut trial_binders = alpha_binders.to_vec();
                let mut trial_retry = retry.clone();
                match Self::identical_retry_attempt(
                    store,
                    plan,
                    candidate,
                    target,
                    &mut trial_seen,
                    &mut trial_binders,
                    &mut trial_retry,
                ) {
                    IdentityAttempt::Decided(DemandOutcome::Ready(true)) => {
                        matched = Some((position, trial_seen, trial_retry));
                        break;
                    }
                    IdentityAttempt::Decided(DemandOutcome::Ready(false)) => {}
                    IdentityAttempt::Decided(DemandOutcome::Exhausted(exhaustion)) => {
                        return IdentityAttempt::Decided(DemandOutcome::Exhausted(exhaustion))
                    }
                    IdentityAttempt::Needs(demand) => return IdentityAttempt::Needs(demand),
                }
            }
            let Some((position, trial_seen, trial_retry)) = matched else {
                return IdentityAttempt::Decided(DemandOutcome::Ready(false));
            };
            *seen = trial_seen;
            *retry = trial_retry;
            remaining.remove(position);
        }
        IdentityAttempt::Decided(DemandOutcome::Ready(remaining.is_empty()))
    }

    fn flatten_normalized_family(
        store: &Store,
        plan: &ProjectionPlan<'_>,
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

    fn prepare_exact_family_root(
        store: &Store,
        plan: &ProjectionPlan<'_>,
        root: TypeId,
    ) -> ExactFamilyAttempt {
        let family = store.tag(root);
        if !matches!(family, TypeTag::Union | TypeTag::Intersection) {
            return ExactFamilyAttempt::Ready(root);
        }
        let mut stack = if family == TypeTag::Union {
            store.union_members(root)
        } else {
            store.intersection_members(root)
        }
        .expect("family root has members")
        .to_vec();
        let mut flattened = Vec::new();
        let mut expanded = FxHashSet::default();
        while let Some(child) = stack.pop() {
            let normalized = match plan.normalize(child) {
                Ok(normalized) => normalized,
                Err(exhaustion) => return ExactFamilyAttempt::Exhausted(exhaustion),
            };
            if let Some(demand) = plan.relation_demand(store, normalized) {
                return ExactFamilyAttempt::Needs(demand);
            }
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
        ExactFamilyAttempt::Ready(if flattened.len() == 1 {
            flattened[0]
        } else {
            root
        })
    }

    /// Plan, normalize, and relate one top-level assignability operation.
    pub(crate) fn is_assignable(&mut self, src: TypeId, tgt: TypeId) -> RelationOutcome {
        self.observe_outer_class(src);
        self.observe_outer_class(tgt);
        if let Some(reason) = publication_exhaustion(
            self.interner.store(),
            &[src, tgt],
            self.published,
            self.state,
        ) {
            return RelationOutcome::Exhausted(reason);
        }
        let completed_key = CompletedRelationKey::assignable(src, tgt);
        if let Some(completed) = self.state.completed_relations.get(&completed_key).cloned() {
            #[cfg(test)]
            measure_query_source_cold(|measure| match &completed {
                CompletedRelationOutcome::Yes => measure.completed_relation_yes_hits += 1,
                CompletedRelationOutcome::No(_) => measure.completed_relation_no_hits += 1,
            });
            return completed.outcome();
        }
        if src == tgt {
            return RelationOutcome::Yes;
        }
        if let Some(outcome) = self.same_class_covariant_argument_mismatch(src, tgt) {
            self.remember_completed_relation(completed_key, &outcome);
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
            self.commit_plan(transaction.into_commit());
            self.remember_completed_relation(completed_key, &outcome);
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
                    return Some(RelationOutcome::No(Arc::new(ReasonChain::leaf(src, tgt))))
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
            self.commit_plan(transaction.into_commit());
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
        self.observe_outer_class(implementation);
        self.observe_outer_class(overload);
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
            self.commit_plan(transaction.into_commit());
        }
        outcome
    }

    fn commit_plan(&mut self, transaction: PendingQueryCommit) {
        #[cfg(test)]
        measure_query_source_cold(|measure| measure.planner_commits += 1);
        #[cfg(test)]
        measure_query_demand(|measure| {
            measure.durable_evaluation_inserts +=
                u64::try_from(transaction.pending_evaluator_writes.len()).unwrap();
        });
        commit_query_cache_entries(
            &mut self.state.projection_memo,
            &mut self.state.evaluation_memo,
            transaction.pending_projection_writes,
            transaction.pending_evaluator_writes,
        );
        *self.next_type_param = transaction.next_type_param;
    }

    fn remember_completed_relation(
        &mut self,
        key: CompletedRelationKey,
        outcome: &RelationOutcome,
    ) {
        if matches!(outcome, RelationOutcome::No(_))
            && !self.state.completed_relation_no_candidates.remove(&key)
        {
            self.state.completed_relation_no_candidates.insert(key);
            return;
        }
        let Some(completed) = CompletedRelationOutcome::from_outcome(outcome) else {
            return;
        };
        if self.state.completed_relations.contains_key(&key) {
            return;
        }
        #[cfg(test)]
        measure_query_source_cold(|measure| match &completed {
            CompletedRelationOutcome::Yes => measure.completed_relation_yes_inserts += 1,
            CompletedRelationOutcome::No(_) => measure.completed_relation_no_inserts += 1,
        });
        self.state.completed_relations.insert(key, completed);
    }
}

/// Immutable overlays consumed by relation before identity/cache/cycle logic.
#[derive(Default)]
pub(crate) struct ProjectionPlan<'a> {
    class_projection_overlay: FxHashMap<TypeId, TypeId>,
    resolved_class_projections: FxHashSet<TypeId>,
    evaluation_overlay: FxHashMap<TypeId, TypeId>,
    resolved_evaluations: FxHashSet<TypeId>,
    frontier: FxHashMap<TypeId, Exhaustion>,
    durable_evaluation_memo: Option<&'a FxHashMap<TypeId, TypeId>>,
}

impl RelationNormalization for ProjectionPlan<'_> {
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
                .or_else(|| {
                    self.durable_evaluation_memo
                        .and_then(|memo| memo.get(&current))
                })
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
            TypeTag::ClassInstance if !self.resolved_class_projections.contains(&ty) => {
                Some(RelationDemand::ClassProjection(ty))
            }
            TypeTag::DeferredIndexedAccess
            | TypeTag::Keyof
            | TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::Mapped
            | TypeTag::Template
                if !self.resolved_evaluations.contains(&ty)
                    && self
                        .durable_evaluation_memo
                        .is_none_or(|memo| memo.get(&ty).is_none_or(|result| *result == ty)) =>
            {
                Some(RelationDemand::Evaluation(ty))
            }
            _ => None,
        }
    }
}

struct PlannedQuery<'a> {
    plan: ProjectionPlan<'a>,
    pending_projection_writes: FxHashMap<TypeId, TypeId>,
    pending_evaluator_writes: FxHashMap<TypeId, TypeId>,
    next_type_param: u32,
    planning_tainted: bool,
    first_exhaustion: Option<Exhaustion>,
}

struct PendingQueryCommit {
    pending_projection_writes: FxHashMap<TypeId, TypeId>,
    pending_evaluator_writes: FxHashMap<TypeId, TypeId>,
    next_type_param: u32,
}

impl PlannedQuery<'_> {
    fn into_commit(self) -> PendingQueryCommit {
        PendingQueryCommit {
            pending_projection_writes: self.pending_projection_writes,
            pending_evaluator_writes: self.pending_evaluator_writes,
            next_type_param: self.next_type_param,
        }
    }
}

struct ProjectionPlanner<'work, 'memo, L: PublishedClassLookup + ?Sized> {
    interner: &'work mut Interner,
    published: &'work L,
    durable_projection_memo: &'work FxHashMap<TypeId, TypeId>,
    durable_evaluation_memo: &'memo FxHashMap<TypeId, TypeId>,
    working_evaluation_memo: FxHashMap<TypeId, TypeId>,
    next_type_param: u32,
    plan: ProjectionPlan<'memo>,
    pending_projection_writes: FxHashMap<TypeId, TypeId>,
    admitted_applications: FxHashSet<TypeId>,
    visiting: FxHashSet<TypeId>,
    visited: FxHashSet<TypeId>,
    planning_tainted: bool,
    first_exhaustion: Option<Exhaustion>,
    evaluation_expansions: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SemanticVisitPolicy {
    Recursive,
    DemandOuterOnly,
    RelationRootOuterOnly,
}

impl SemanticVisitPolicy {
    fn operand_policy(self) -> Self {
        match self {
            Self::RelationRootOuterOnly => Self::Recursive,
            Self::Recursive | Self::DemandOuterOnly => self,
        }
    }

    fn admits_result_recursively(self) -> bool {
        self == Self::Recursive
    }
}

impl<'work, 'memo, L: PublishedClassLookup + ?Sized> ProjectionPlanner<'work, 'memo, L> {
    fn new(
        interner: &'work mut Interner,
        published: &'work L,
        durable_projection_memo: &'work FxHashMap<TypeId, TypeId>,
        durable_evaluation_memo: &'memo FxHashMap<TypeId, TypeId>,
        next_type_param: u32,
    ) -> Self {
        #[cfg(test)]
        measure_query_source_cold(|measure| {
            measure.planner_transactions += 1;
        });
        ProjectionPlanner {
            interner,
            published,
            durable_projection_memo,
            durable_evaluation_memo,
            working_evaluation_memo: FxHashMap::default(),
            next_type_param,
            plan: ProjectionPlan {
                durable_evaluation_memo: Some(durable_evaluation_memo),
                ..ProjectionPlan::default()
            },
            pending_projection_writes: FxHashMap::default(),
            admitted_applications: FxHashSet::default(),
            visiting: FxHashSet::default(),
            visited: FxHashSet::default(),
            planning_tainted: false,
            first_exhaustion: None,
            evaluation_expansions: 0,
        }
    }

    fn plan(mut self, roots: &[TypeId]) -> PlannedQuery<'memo> {
        for &root in roots {
            self.visit(root);
        }
        self.finish()
    }

    fn plan_demand(mut self, root: TypeId) -> PlannedQuery<'memo> {
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
                self.visit_demand_outer(root);
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
                    self.visit_relation_root(root);
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
                self.visit_relation_root(ty);
            }
        }
    }

    fn finish(self) -> PlannedQuery<'memo> {
        let pending_evaluator_writes: FxHashMap<TypeId, TypeId> = self
            .working_evaluation_memo
            .iter()
            .filter_map(|(&key, &value)| {
                (self.durable_evaluation_memo.get(&key) != Some(&value)).then_some((key, value))
            })
            .collect();
        #[cfg(test)]
        measure_query_source_cold(|measure| {
            if self.planning_tainted {
                measure.planner_tainted_finishes += 1;
            } else {
                measure.planner_clean_finishes += 1;
            }
            if self.pending_projection_writes.is_empty() && pending_evaluator_writes.is_empty() {
                measure.planner_zero_write_finishes += 1;
            }
        });
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
        self.visit_with_policy(ty, SemanticVisitPolicy::Recursive)
    }

    fn visit_demand_outer(&mut self, ty: TypeId) -> TypeId {
        self.visit_with_policy(ty, SemanticVisitPolicy::DemandOuterOnly)
    }

    fn visit_relation_root(&mut self, ty: TypeId) -> TypeId {
        self.visit_with_policy(ty, SemanticVisitPolicy::RelationRootOuterOnly)
    }

    fn visit_with_policy(&mut self, ty: TypeId, policy: SemanticVisitPolicy) -> TypeId {
        #[cfg(test)]
        {
            measure_query_demand(|measure| measure.planner_visits += 1);
            measure_query_source_cold(|measure| measure.planner_visits += 1);
        }
        if let Ok(normalized) = self.plan.normalize(ty) {
            if normalized != ty {
                #[cfg(test)]
                measure_query_demand(|measure| measure.overlay_hits += 1);
                self.visit_demand_result(normalized, policy);
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
            TypeTag::ClassInstance if !policy.admits_result_recursively() => {
                self.project_class_outer(ty)
            }
            TypeTag::ClassInstance => self.project_class(ty),
            TypeTag::DeferredIndexedAccess => self.evaluate_deferred_indexed(ty, policy),
            TypeTag::Keyof => self.evaluate_keyof(ty, policy),
            TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::Union
            | TypeTag::Mapped
            | TypeTag::Template => self.evaluate_existing(ty, policy),
            _ => {
                let children = query_children(self.interner.store(), ty);
                for child in children {
                    self.visit_with_policy(child, policy.operand_policy());
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
        self.plan.resolved_class_projections.insert(application);

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

    fn evaluate_deferred_indexed(&mut self, ty: TypeId, policy: SemanticVisitPolicy) -> TypeId {
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
            self.visit_demand_result(shallow, policy);
            return shallow;
        }
        let mut object = access.object;
        if object_requires_demand(self.interner.store(), object) {
            object = self.visit_with_policy(object, policy.operand_policy());
            if let Some(constraint) = self
                .interner
                .store()
                .type_param(object)
                .and_then(|parameter| self.interner.store().type_param_constraint(parameter.id))
                .filter(|constraint| *constraint != object)
            {
                object = self.visit_with_policy(constraint, policy.operand_policy());
            }
        }
        let index = if index_requires_planner_visit(self.interner.store(), access.index) {
            self.visit_with_policy(access.index, policy.operand_policy())
        } else {
            access.index
        };
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
        self.visit_demand_result(result, policy);
        result
    }

    fn evaluate_keyof(&mut self, ty: TypeId, policy: SemanticVisitPolicy) -> TypeId {
        let Some(operand) = self.interner.store().keyof_operand(ty) else {
            return ty;
        };
        let shallow = resolve_keyof_outer_layer(self.interner, operand);
        if shallow != ty {
            self.record_evaluation(ty, shallow);
            self.visit_demand_result(shallow, policy);
            return shallow;
        }
        let operand = self.visit_with_policy(operand, policy.operand_policy());
        let operand = match self.plan.normalize(operand) {
            Ok(operand) => operand,
            Err(reason) => {
                self.mark_frontier(ty, reason);
                return ty;
            }
        };
        let result = resolve_keyof_outer_layer(self.interner, operand);
        self.record_evaluation(ty, result);
        self.visit_demand_result(result, policy);
        result
    }

    fn evaluate_existing(&mut self, ty: TypeId, policy: SemanticVisitPolicy) -> TypeId {
        #[cfg(test)]
        measure_query_demand(|measure| measure.pending_evaluations += 1);
        let local = self.working_evaluation_memo.get(&ty).copied();
        let durable = local
            .is_none()
            .then(|| self.durable_evaluation_memo.get(&ty).copied())
            .flatten();
        if let Some(result) = local.or(durable) {
            #[cfg(test)]
            if durable.is_some() {
                measure_query_demand(|measure| measure.durable_evaluation_hits += 1);
            }
            self.record_evaluation(ty, result);
            self.visit_demand_result(result, policy);
            return result;
        }

        let children = query_children(self.interner.store(), ty);
        for child in children {
            self.visit_with_policy(child, policy.operand_policy());
        }

        if self.evaluation_expansions >= DEFAULT_STEP_BUDGET {
            self.mark_frontier(ty, Exhaustion::EvaluationBudget);
            return ty;
        }
        self.evaluation_expansions += 1;
        #[cfg(test)]
        measure_query_demand(|measure| measure.evaluation_expansions += 1);

        let (outcome, exhausted, cycle_detected) = {
            let mut evaluator = ConditionalEvaluator::with_parent_memo(
                self.interner,
                &mut self.next_type_param,
                self.durable_evaluation_memo,
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
        self.visit_demand_result(result, policy);
        result
    }

    fn visit_demand_result(&mut self, result: TypeId, policy: SemanticVisitPolicy) {
        match policy {
            SemanticVisitPolicy::Recursive => {
                self.visit_with_policy(result, SemanticVisitPolicy::Recursive);
            }
            SemanticVisitPolicy::DemandOuterOnly | SemanticVisitPolicy::RelationRootOuterOnly => {
                match self.interner.store().tag(result) {
                    TypeTag::ClassInstance => {
                        self.project_class_outer(result);
                    }
                    TypeTag::DeferredIndexedAccess => {
                        self.visit_with_policy(result, policy);
                    }
                    _ => {}
                }
            }
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
    refresh_semantic_context(store, published, state);
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

fn refresh_semantic_context<L: PublishedClassLookup + ?Sized>(
    store: &Store,
    published: &L,
    state: &mut SemanticQueryState,
) {
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
        state.projection_memo.clear();
        state.evaluation_memo.clear();
        state.completed_identities.clear();
        state.relation_cache = RelationCache::default();
        state.publication_clean.clear();
        state.completed_relations.clear();
        state.completed_relation_no_candidates.clear();
        state.publication_store_identity = Some(Arc::clone(store_identity));
        state.publication_snapshot_identity = Some(Arc::clone(publication_identity));
    }
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
mod event_listener_union_scaling_spec;

#[cfg(test)]
mod deferred_indexed_lazy_spec;

#[cfg(test)]
mod demand_identity_spec;

#[cfg(test)]
mod identity_memo_spec;

#[cfg(test)]
mod instantiation_root_lazy_spec;

#[cfg(test)]
mod relation_root_lazy_spec;

#[cfg(test)]
mod tests;

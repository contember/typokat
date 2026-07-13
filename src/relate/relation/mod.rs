//! Relation queries (`is_assignable` today; subtype/identity kinds are reserved).
//!
//! The core driver owns the soundness-critical cache, assume-true cycle stack,
//! and `Relation::No(ReasonChain)` failures.

mod advanced;
mod collections;
pub(crate) mod legacy_guard;
mod objects;
mod set_types;

use crate::class_semantics::{Exhaustion, PublishedClasses};
use crate::relate::cache::{RelationCache, RelationKey};
use crate::types::repr::{GenericTypeParam, IntrinsicKind, TypeParamId, TypeTag};
use crate::types::store::{Store, TypeId};
use crate::types::WellKnown;
use rustc_hash::{FxHashMap, FxHashSet};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RelationMeasure {
    pub stack_key_builds: u64,
    pub empty_context_stack_keys: u64,
    pub contextual_stack_keys: u64,
    pub binder_frames_scanned: u64,
    pub flattened_environment_entries: u64,
    pub environment_sort_items: u64,
    pub object_target_properties: u64,
    pub object_source_property_comparisons: u64,
}

#[cfg(test)]
thread_local! {
    static RELATION_MEASURE: std::cell::RefCell<RelationMeasure> = std::cell::RefCell::new(RelationMeasure::default());
}

#[cfg(test)]
pub(super) fn reset_relation_measure() {
    RELATION_MEASURE.with(|measure| *measure.borrow_mut() = RelationMeasure::default());
}

#[cfg(test)]
pub(super) fn relation_measure() -> RelationMeasure {
    RELATION_MEASURE.with(|measure| *measure.borrow())
}

#[cfg(test)]
fn measure_relation(update: impl FnOnce(&mut RelationMeasure)) {
    RELATION_MEASURE.with(|measure| update(&mut measure.borrow_mut()));
}

/// Distinct relation kinds must not share cache entries (architecture §6.1).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum RelationKind {
    Identity,
    Subtype,
    Assignable,
}

/// The assume-true cycle-guard set for the M31 merged-source recursion
/// ([`Relater::relate_source_members_to`]): a key is `(canonical-sorted candidate set,
/// target, relation kind)`. A run-local, per-query set (NOT the durable cache), so a
/// recursive object-target property terminates coinductively without ever caching a
/// provisional `Yes` (§6.3).
pub(super) type MergedInFlightSet = FxHashSet<(Vec<TypeId>, TypeId, RelationKind)>;

/// One link in a failure explanation; recursive variants wrap the first nested
/// cause so reporting can render a deterministic reason chain.
#[derive(Clone, Debug)]
pub enum Reason {
    /// The base mismatch: `src` is not assignable to `tgt`.
    Leaf { src: TypeId, tgt: TypeId },
    /// Required target property missing from the source object (`TK2741`).
    MissingProperty {
        name: String,
        src: TypeId,
        tgt: TypeId,
    },
    /// Present property has an incompatible value type (`TK2322`).
    Property {
        name: String,
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// Function source requires more parameters than the target can supply (`TK2322`).
    ParameterCount { src: TypeId, tgt: TypeId },
    /// Function parameter mismatch; `because` was built in the contravariant
    /// `tgt_param → src_param` direction (`TK2322`).
    Parameter {
        index: usize,
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// Function return mismatch in the covariant `src_ret → tgt_ret` direction
    /// (`TK2322`).
    ReturnType {
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// First union-source member that fails against the target (`TK2322`).
    UnionSourceMember {
        member: TypeId,
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// Source fits no union-target member; no single member is the cause (`TK2322`).
    NoUnionMember { src: TypeId, tgt: TypeId },
    /// Array element mismatch in the covariant `S → T` direction (`TK2322`).
    ArrayElement {
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// Tuple length mismatch; a terminal reason (`TK2322`).
    TupleLength { src: TypeId, tgt: TypeId },
    /// First positional tuple element mismatch (`TK2322`).
    TupleElement {
        index: usize,
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
    /// First source value that violates a target index signature (`TK2322`).
    IndexSignature {
        src: TypeId,
        tgt: TypeId,
        because: Box<Reason>,
    },
}

/// A non-empty chain of reasons explaining a relation failure, outermost first.
/// Built **only on the failing path** (architecture §6.4) so the success path
/// stays allocation-free.
#[derive(Clone, Debug)]
pub struct ReasonChain {
    pub head: Reason,
}

impl ReasonChain {
    pub(crate) fn leaf(src: TypeId, tgt: TypeId) -> ReasonChain {
        ReasonChain {
            head: Reason::Leaf { src, tgt },
        }
    }

    /// Wrap an arbitrary head reason (object-structural failures).
    fn of(head: Reason) -> ReasonChain {
        ReasonChain { head }
    }

    /// The outermost reason — the checker inspects its kind to pick the
    /// diagnostic code (missing property → `TK2741`, otherwise `TK2322`).
    pub fn head(&self) -> &Reason {
        &self.head
    }

    /// The (src, tgt) at the root of the failure — what the checker reports as
    /// the primary mismatch.
    pub fn root(&self) -> (TypeId, TypeId) {
        match &self.head {
            Reason::Leaf { src, tgt } => (*src, *tgt),
            Reason::MissingProperty { src, tgt, .. } => (*src, *tgt),
            Reason::Property { src, tgt, .. } => (*src, *tgt),
            Reason::ParameterCount { src, tgt } => (*src, *tgt),
            Reason::Parameter { src, tgt, .. } => (*src, *tgt),
            Reason::ReturnType { src, tgt, .. } => (*src, *tgt),
            Reason::UnionSourceMember { src, tgt, .. } => (*src, *tgt),
            Reason::NoUnionMember { src, tgt } => (*src, *tgt),
            Reason::ArrayElement { src, tgt, .. } => (*src, *tgt),
            Reason::TupleLength { src, tgt } => (*src, *tgt),
            Reason::TupleElement { src, tgt, .. } => (*src, *tgt),
            Reason::IndexSignature { src, tgt, .. } => (*src, *tgt),
        }
    }
}

/// The result of a relation query. Never a bare `bool`: a failure carries its
/// cause so reporting mode (M6) can render nested "because…" messages.
#[derive(Clone, Debug)]
pub enum Relation {
    Yes,
    No(ReasonChain),
}

/// ADR-0006 relation result. Unlike legacy [`Relation`], projection/evaluation
/// exhaustion remains an explicit third semantic outcome and has no boolean helper.
#[allow(dead_code)] // Dormant until WU1c installs the planned relation protocol.
#[derive(Clone, Debug)]
pub(crate) enum RelationOutcome {
    Yes,
    No(ReasonChain),
    Exhausted(Exhaustion),
}

/// Identical-only relation rule for an unevaluated deferred indexed access. Other
/// pairs remain for WU1c's projection planner and relation transaction.
#[allow(dead_code)] // Dormant until WU1c installs the planned relation protocol.
pub(crate) fn relate_unevaluated_deferred(
    published: &PublishedClasses,
    store: &Store,
    src: TypeId,
    tgt: TypeId,
) -> Option<RelationOutcome> {
    if let Some(reason) = legacy_guard::publication_exhaustion(store, &[src, tgt], published) {
        return Some(RelationOutcome::Exhausted(reason));
    }
    let deferred = store.tag(src) == TypeTag::DeferredIndexedAccess
        || store.tag(tgt) == TypeTag::DeferredIndexedAccess;
    deferred.then(|| {
        if src == tgt {
            RelationOutcome::Yes
        } else {
            RelationOutcome::No(ReasonChain::leaf(src, tgt))
        }
    })
}

impl Relation {
    pub fn is_yes(&self) -> bool {
        matches!(self, Relation::Yes)
    }
}

/// The relation engine. Borrows the store immutably (relation checking never
/// mutates the arena) and owns the cache + cycle stack.
pub struct Relater<'a> {
    store: &'a Store,
    well_known: WellKnown,
    cache: RelationCache,
    /// Assume-true-until-disproven stack (architecture §6.3): when a query
    /// re-enters a relation already in flight, we assume it holds and continue,
    /// resolving the fixpoint as the stack unwinds. It fires as of M5, where
    /// recursive/mutually-recursive types (`interface List { tail: List | null }`)
    /// re-enter an in-flight key and rely on this to terminate.
    stack: FxHashSet<StackRelationKey>,
    /// Temporary generic-binder alignments and one-way source specializations.
    /// Every relation below one of these frames bypasses the durable three-word
    /// cache because its verdict depends on this local environment.
    binder_contexts: Vec<BinderRelationContext>,
}

/// The in-flight cycle identity. The durable cache intentionally remains the
/// architecture's three-word [`RelationKey`], while an assume-true dependency
/// must additionally capture the binder environment that gives raw type
/// parameters their local meaning.
#[derive(Clone, PartialEq, Eq, Hash)]
struct StackRelationKey {
    relation: RelationKey,
    environment: StackBinderEnvironment,
}

/// The semantic part of an in-flight key. Context frames are lexical machinery;
/// repeated recursion through alpha-equivalent frames must see the same key, while
/// different alignments, constraints, or specializations must remain distinct.
#[derive(Clone, PartialEq, Eq, Hash)]
struct StackBinderEnvironment {
    source_to_target: Vec<(TypeParamId, TypeParamId)>,
    target_to_source: Vec<(TypeParamId, TypeParamId)>,
    constraints: Vec<(TypeParamId, Option<TypeId>)>,
    source_instantiations: Vec<(TypeParamId, TypeId)>,
}

type AssumedSet = FxHashSet<StackRelationKey>;

/// A local function-relation environment. Type-parameter ids stay persistent in
/// the store; this only gives their comparison a lexical meaning for one generic
/// signature pair, never a cache key.
#[derive(Default)]
struct BinderRelationContext {
    parameters: FxHashSet<TypeParamId>,
    constraints: FxHashMap<TypeParamId, Option<TypeId>>,
    source_to_target: FxHashMap<TypeParamId, TypeParamId>,
    target_to_source: FxHashMap<TypeParamId, TypeParamId>,
    instantiable_source: FxHashSet<TypeParamId>,
    source_instantiations: FxHashMap<TypeParamId, TypeId>,
}

impl BinderRelationContext {
    fn aligned(source: &[GenericTypeParam], target: &[GenericTypeParam]) -> Self {
        let mut context = BinderRelationContext::default();
        for (source_param, target_param) in source.iter().zip(target) {
            context.parameters.insert(source_param.id);
            context.parameters.insert(target_param.id);
            context
                .constraints
                .insert(source_param.id, source_param.constraint);
            context
                .constraints
                .insert(target_param.id, target_param.constraint);
            context
                .source_to_target
                .insert(source_param.id, target_param.id);
            context
                .target_to_source
                .insert(target_param.id, source_param.id);
        }
        context
    }

    fn source_specialization(source: &[GenericTypeParam]) -> Self {
        let mut context = BinderRelationContext::default();
        for source_param in source {
            context.parameters.insert(source_param.id);
            context
                .constraints
                .insert(source_param.id, source_param.constraint);
            context.instantiable_source.insert(source_param.id);
        }
        context
    }

    /// Construct signatures compare their common binders positionally. Extra source
    /// binders are eligible for specialization from a parameter occurrence; an
    /// extra target binder is only constrained when it actually occurs in the
    /// target shape. The caller decides those occurrence-sensitive details.
    fn construct_arity_specialization(
        source: &[GenericTypeParam],
        target: &[GenericTypeParam],
    ) -> Option<Self> {
        let shared = source.len().min(target.len());
        let mut context = Self::aligned(&source[..shared], &target[..shared]);
        for parameter in &source[shared..] {
            context.parameters.insert(parameter.id);
            context
                .constraints
                .insert(parameter.id, parameter.constraint);
            context.instantiable_source.insert(parameter.id);
        }
        for parameter in &target[shared..] {
            context.parameters.insert(parameter.id);
            context
                .constraints
                .insert(parameter.id, parameter.constraint);
        }
        Some(context)
    }
}

impl<'a> Relater<'a> {
    pub fn new(store: &'a Store, well_known: WellKnown) -> Self {
        Relater {
            store,
            well_known,
            cache: RelationCache::new(),
            stack: FxHashSet::default(),
            binder_contexts: Vec::new(),
        }
    }

    /// Is `src` assignable to `tgt`? Entry point used by the checker for
    /// annotation-vs-initializer checks (`TK2322`).
    pub fn is_assignable(&mut self, src: TypeId, tgt: TypeId) -> Relation {
        legacy_guard::reject_legacy_semantic_types(self.store, &[src, tgt]);
        // The outermost frame has no enclosing assumptions; `assumed` collects any
        // assume-true dependencies its subtree consumes (see `relate`). Whatever
        // survives here would be an assumption about a key with no enclosing
        // frame — impossible by construction, so it is simply dropped.
        let mut assumed = FxHashSet::default();
        self.relate(src, tgt, RelationKind::Assignable, &mut assumed)
    }

    /// Core relation driver: cache + cycle stack around the structural rules.
    ///
    /// `assumed` is the **provisional-assumption channel** (architecture §6.3): it
    /// accumulates the in-flight keys this computation depended on via the
    /// assume-true short-circuit. A `Yes` that rests on an assumption about an
    /// **ancestor** key (one still on the stack above this frame) is *provisional* —
    /// sound only under that assumption — and must NOT be committed to the durable
    /// cache, or a later INDEPENDENT query would read a spurious `true` and drop a
    /// real error. Each frame discharges the assumption about its **own** key (the
    /// fixpoint resolves at the cycle root, so a verdict that depended only on
    /// re-entry to its own key is genuine) and propagates any remaining ancestor
    /// assumptions to its caller.
    fn relate(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        legacy_guard::reject_legacy_semantic_type(self.store, src);
        legacy_guard::reject_legacy_semantic_type(self.store, tgt);
        // Identity fast path: `T` relates to `T` under every relation.
        if src == tgt {
            return Relation::Yes;
        }

        if let Some(result) = self.relate_contextual_type_params(src, tgt, kind, assumed) {
            return result;
        }

        let key = RelationKey::new(src, tgt, kind);
        let stack_key = self.stack_relation_key(key);
        let cacheable = self.binder_contexts.is_empty();

        // Cycle stack FIRST (architecture §6.3): re-entry on an in-flight relation
        // is assumed true and continues, resolving the fixpoint as the stack
        // unwinds. Checking the stack *before* the cache is what makes recursive
        // types terminate even on the rebuild path below: a relation cached as a
        // failure is recomputed (to rebuild its reason chain) **under** a stack
        // push, so a self-referential failure re-enters the same key, finds it in
        // flight, and terminates rather than recomputing forever (M5 — §6.3). The
        // assumed key is recorded so the caller's verdict is treated as provisional
        // until that key is discharged at its own root.
        if self.stack.contains(&stack_key) {
            assumed.insert(stack_key);
            return Relation::Yes;
        }

        // Cache: a previously-decided durable relation. Only **sound** verdicts are
        // ever stored (a genuine `false`, or a `true` that rested on no outstanding
        // assumption — see the commit below), so a cached hit is ground truth. A
        // cached success returns directly; a cached *failure* falls through to a
        // stack-guarded recompute so the checker still sees the precise
        // missing-vs-mismatch reason (the cache stores only the bool verdict —
        // architecture §6.1 — not the reason).
        let cached = cacheable.then(|| self.cache.get(key)).flatten();
        if cached == Some(true) {
            return Relation::Yes;
        }

        self.stack.insert(stack_key.clone());
        // This frame's own assumption accumulator. Children record the ancestor
        // keys (including, possibly, this frame's own key) they assumed true.
        let mut frame_assumed: AssumedSet = FxHashSet::default();
        let result = self.relate_uncached(src, tgt, kind, &mut frame_assumed);
        self.stack.remove(&stack_key);

        // Discharge the assumption about our OWN key: the fixpoint is resolved at
        // this root, so a dependency that was only on re-entry to `key` is genuine.
        frame_assumed.remove(&stack_key);
        // Anything left is an assumption about a key still in flight ABOVE us — this
        // verdict is provisional. Surface those to the caller so its cacheability
        // accounts for them too.
        let provisional = !frame_assumed.is_empty();
        assumed.extend(frame_assumed.iter().cloned());

        // Commit only sound verdicts on first decision:
        //   * a `false` is ALWAYS genuine — the assume-true rule only ever
        //     manufactures a spurious `true`, never a spurious `false`, so a `No`
        //     never depends on an assumption and is always cacheable;
        //   * a `true` is cacheable only when it rested on no outstanding ancestor
        //     assumption (otherwise it is provisional and would poison the cache).
        // A recompute of an already-cached failure must not re-insert.
        if cacheable && cached.is_none() {
            if !result.is_yes() {
                self.cache.insert(key, false);
            } else if !provisional {
                self.cache.insert(key, true);
            }
        }
        result
    }

    /// Run one relation operation under a local binder environment. Nested calls
    /// deliberately see this frame, but cannot read or write the durable cache.
    fn with_binder_context<R>(
        &mut self,
        context: BinderRelationContext,
        body: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.binder_contexts.push(context);
        let result = body(self);
        self.binder_contexts.pop();
        result
    }

    fn stack_relation_key(&self, relation: RelationKey) -> StackRelationKey {
        #[cfg(test)]
        measure_relation(|measure| {
            measure.stack_key_builds += 1;
            if self.binder_contexts.is_empty() {
                measure.empty_context_stack_keys += 1;
            } else {
                measure.contextual_stack_keys += 1;
                measure.binder_frames_scanned += self.binder_contexts.len() as u64;
            }
        });
        let mut source_to_target = FxHashMap::default();
        let mut target_to_source = FxHashMap::default();
        let mut constraints = FxHashMap::default();
        let mut source_instantiations = FxHashMap::default();

        // Flatten lexical frames outer-to-inner. The innermost binding is the
        // effective meaning of a persistent parameter id, so it deliberately
        // overrides an outer binding with the same id.
        for context in &self.binder_contexts {
            #[cfg(test)]
            measure_relation(|measure| {
                measure.flattened_environment_entries += (context.source_to_target.len()
                    + context.target_to_source.len()
                    + context.parameters.len()
                    + context.constraints.len()
                    + context.source_instantiations.len())
                    as u64;
            });
            source_to_target.extend(
                context
                    .source_to_target
                    .iter()
                    .map(|(&source, &target)| (source, target)),
            );
            target_to_source.extend(
                context
                    .target_to_source
                    .iter()
                    .map(|(&target, &source)| (target, source)),
            );
            for &parameter in &context.parameters {
                constraints.insert(
                    parameter,
                    context.constraints.get(&parameter).copied().flatten(),
                );
            }
            constraints.extend(
                context
                    .constraints
                    .iter()
                    .map(|(&parameter, &constraint)| (parameter, constraint)),
            );
            source_instantiations.extend(
                context
                    .source_instantiations
                    .iter()
                    .map(|(&parameter, &ty)| (parameter, ty)),
            );
        }

        let mut source_to_target: Vec<_> = source_to_target.into_iter().collect();
        let mut target_to_source: Vec<_> = target_to_source.into_iter().collect();
        let mut constraints: Vec<_> = constraints.into_iter().collect();
        let mut source_instantiations: Vec<_> = source_instantiations.into_iter().collect();
        source_to_target.sort_by_key(|(parameter, _)| *parameter);
        target_to_source.sort_by_key(|(parameter, _)| *parameter);
        constraints.sort_by_key(|(parameter, _)| *parameter);
        source_instantiations.sort_by_key(|(parameter, _)| *parameter);

        #[cfg(test)]
        measure_relation(|measure| {
            measure.environment_sort_items += (source_to_target.len()
                + target_to_source.len()
                + constraints.len()
                + source_instantiations.len()) as u64;
        });

        StackRelationKey {
            relation,
            environment: StackBinderEnvironment {
                source_to_target,
                target_to_source,
                constraints,
                source_instantiations,
            },
        }
    }

    /// Handle alpha-aligned binders and a generic source's temporary
    /// specialization before the context-free structural rules run.
    fn relate_contextual_type_params(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Option<Relation> {
        let src_param = self.store.type_param(src).map(|param| param.id);
        let tgt_param = self.store.type_param(tgt).map(|param| param.id);

        if let (Some(src_param), Some(tgt_param)) = (src_param, tgt_param) {
            for context in self.binder_contexts.iter().rev() {
                if context.source_to_target.get(&src_param) == Some(&tgt_param)
                    || context.target_to_source.get(&src_param) == Some(&tgt_param)
                {
                    return Some(Relation::Yes);
                }
            }
        }

        let binding =
            src_param
                .and_then(|id| {
                    self.binder_contexts
                        .iter()
                        .enumerate()
                        .rev()
                        .find_map(|(index, context)| {
                            context
                                .instantiable_source
                                .contains(&id)
                                .then_some((index, id, tgt, true))
                        })
                })
                .or_else(|| {
                    tgt_param.and_then(|id| {
                        self.binder_contexts.iter().enumerate().rev().find_map(
                            |(index, context)| {
                                context
                                    .instantiable_source
                                    .contains(&id)
                                    .then_some((index, id, src, false))
                            },
                        )
                    })
                });
        let (context_index, param, other, source_on_left) = binding?;

        let existing = self
            .binder_contexts
            .get(context_index)
            .and_then(|context| context.source_instantiations.get(&param).copied());
        if let Some(existing) = existing {
            return Some(if source_on_left {
                self.relate(existing, other, kind, assumed)
            } else {
                self.relate(other, existing, kind, assumed)
            });
        }

        let constraint = self
            .binder_contexts
            .get(context_index)
            .and_then(|context| context.constraints.get(&param).copied())
            .flatten();
        if let Some(context) = self.binder_contexts.get_mut(context_index) {
            context.source_instantiations.insert(param, other);
        }
        if let Some(constraint) = constraint {
            if !self.relate(other, constraint, kind, assumed).is_yes() {
                return Some(Relation::No(ReasonChain::leaf(src, tgt)));
            }
        }
        Some(Relation::Yes)
    }

    /// A persistent function descriptor overrides the declaration-side constraint
    /// column while its binder context is active. `Some(None)` is meaningful: an
    /// unconstrained descriptor must not fall back to stale store state.
    fn contextual_type_param_constraint(&self, ty: TypeId) -> Option<Option<TypeId>> {
        let param = self.store.type_param(ty)?.id;
        for context in self.binder_contexts.iter().rev() {
            if context.parameters.contains(&param) {
                return Some(context.constraints.get(&param).copied().flatten());
            }
        }
        None
    }

    /// The structural rules, run when the cache and cycle stack don't decide it.
    /// M0 scope: the intrinsic lattice and literal → base widening. M2 adds the
    /// object property-wise rule (width + depth). `assumed` is threaded through
    /// every recursive `relate` so provisional (assume-true) dependencies bubble up
    /// to the cache-commit site in [`Relater::relate`].
    fn relate_uncached(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let wk = self.well_known;

        // `any` relates to everything in both directions (architecture §6,
        // mvp-plan §4.4). The error type behaves the same so cascades are
        // suppressed.
        if self.is_any_like(src) || self.is_any_like(tgt) {
            return Relation::Yes;
        }

        // M24 — apparent type of a constrained type parameter: `TypeParam(T)` is
        // assignable to `tgt` whenever its **constraint** is (the constraint is `T`'s
        // apparent type). This is **one direction only** — `X → TypeParam(T)` stays
        // failing (a caller could instantiate `T` with a narrower subtype), so the rule
        // fires solely on a `TypeParam` *source*. It runs BEFORE the union-target rule so
        // a whole-union constraint (`K extends "x" | "y"` → `K → "x" | "y"`) relates its
        // constraint to the *entire* union rather than being decomposed member-by-member
        // (`"x" | "y" <: "x"` would spuriously fail). The constraint relation runs through
        // the ordinary [`Relater::relate`], so the cycle stack + cache-soundness apply
        // unchanged: a self-referential constraint (`<T extends { self: T }>`) re-enters
        // the same in-flight key and terminates via the assume-true stack, and a verdict
        // that rested on that assumption bubbles up through `assumed` (never durably cached
        // as a spurious `true`). A constraint that is itself a type parameter (`U extends
        // T`) recurses into this same rule, so constraint chains resolve. On failure it
        // falls through to the leaf below (naming the parameter, not its constraint).
        if self.store.tag(src) == TypeTag::TypeParam {
            let constraint = self
                .contextual_type_param_constraint(src)
                .unwrap_or_else(|| {
                    self.store
                        .type_param(src)
                        .and_then(|param| self.store.type_param_constraint(param.id))
                });
            if let Some(constraint) = constraint {
                if self.relate(constraint, tgt, kind, assumed).is_yes() {
                    return Relation::Yes;
                }
            }
        }

        // Union rules (mvp-plan §6, M4) run BEFORE the intrinsic/object/function
        // rules. They are checked source-first, then target-first:
        //
        //  - if `src` is a union, `src <: tgt` iff **every** member is assignable
        //    to `tgt` (the union is at least as wide as any one member), and
        //  - otherwise, if `tgt` is a union, `src <: tgt` iff `src` is assignable
        //    to **some** member (it lands in one of the alternatives).
        //
        // Both fire only when the relevant side is a `Union` node; a union always
        // has ≥ 2 members (the interner collapses the degenerate cases), so these
        // never spuriously match an intrinsic. The `src`-union case is tried first
        // so a union-to-union relation decomposes member-by-member on the source.
        // M25 — deferred conditionals (a conditional whose check still contains a free
        // declaration type parameter is an ordinary, context-free interned type, so the
        // cache stays sound). Rules (an identical node is already accepted by the
        // `src == tgt` fast path in `relate`):
        //
        //  - a conditional **source** is assignable to `tgt` iff **both branches** are —
        //    only when both branches are **closed** w.r.t. this node's own `infer`
        //    binders; otherwise a conservative `No` (the sound over-report direction).
        //    Runs BEFORE the union-target rule (like the M24 constraint rule) so a
        //    conditional source relates to the *whole* union target (both branches land
        //    in it), rather than being decomposed member-by-member;
        //  - nothing is assignable **into** a deferred conditional (or a lazy
        //    instantiation) except an identical one — conservative, matching probed tsc.
        if self.store.tag(src) == TypeTag::Conditional {
            return self.relate_conditional_source(src, tgt, kind, assumed);
        }
        if matches!(
            self.store.tag(tgt),
            TypeTag::Conditional | TypeTag::Instantiation | TypeTag::Mapped | TypeTag::Keyof
        ) {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        // M28 — a **symbolic string-intrinsic** source (`Uppercase<T>` over a pattern /
        // `string` / a free parameter) always denotes SOME string, so it is assignable
        // wherever `string` is (→ `string`, `string`-containing unions, `unknown` via the
        // rule below) and nowhere narrower — the WU3 "→ string allowed" rule. Runs before
        // the general instantiation-source `No` below; an identical node was already
        // accepted by the `src == tgt` fast path.
        if self.store.tag(src) == TypeTag::Instantiation {
            let marker_base = self
                .store
                .instantiation_type(src)
                .is_some_and(|inst| self.well_known.is_string_intrinsic_marker(inst.base));
            if marker_base {
                if self.relate(wk.string, tgt, kind, assumed).is_yes() {
                    return Relation::Yes;
                }
                return Relation::No(ReasonChain::leaf(src, tgt));
            }
        }
        // A lazy instantiation source that did not evaluate (should not reach here for
        // the M25 corpus) is conservatively unassignable to anything but an identical
        // node — the safe direction.
        if self.store.tag(src) == TypeTag::Instantiation {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        // M26 — a **deferred** mapped type (one over a free declaration type parameter,
        // e.g. `Ident<T>`) that did not evaluate is conservatively unassignable to
        // anything but an identical node (the `src == tgt` fast path already accepted
        // that): a `T`/literal source is NOT assignable into it, and it is NOT assignable
        // to any other target — mirroring the deferred-conditional model (nothing in
        // EITHER direction). tsc's homomorphic-identity allowance (`T` → `Ident<T>`) is
        // the documented over-report divergence (safe direction; deferred_generics.ts).
        if self.store.tag(src) == TypeTag::Mapped {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        // M28 — a **deferred `keyof`** source (`keyof T` over a free parameter) that did
        // not evaluate relates conservatively: nothing in either direction but an
        // identical node (the fast path) — mirroring the deferred mapped/conditional
        // model (sprint WU2 item 1: no permissive fallback anywhere).
        if self.store.tag(src) == TypeTag::Keyof {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }

        if self.store.tag(src) == TypeTag::Union {
            return self.relate_union_source(src, tgt, kind, assumed);
        }
        if self.store.tag(tgt) == TypeTag::Union {
            return self.relate_union_target(src, tgt, kind, assumed);
        }

        // Intersection rules (M31) — the structural **dual** of the union rules, run
        // right after them so a union side decomposes first (`(A | B) <: (C & D)`
        // decomposes on the union source, then each member on the intersection
        // target). The **target** rule is tried before the **source** rule so an
        // intersection-to-intersection relation decomposes on the target (AND of
        // "src relates to each member"), which is the correct, most-decomposed order.
        //
        //  - a **target** intersection `src <: A & B` requires `src` assignable to
        //    **every** member (AND — mirror of union *source*), so `{a:1} <: {a}&{b}`
        //    fails on the missing `{b}` member (the headline TK2741);
        //  - a **source** intersection `A & B <: tgt` succeeds if **some** member is
        //    assignable, OR the **merged apparent object** of the intersection is
        //    (both sound — an `A & B` value structurally satisfies every member).
        if self.store.tag(tgt) == TypeTag::Intersection {
            return self.relate_intersection_target(src, tgt, kind, assumed);
        }
        if self.store.tag(src) == TypeTag::Intersection {
            return self.relate_intersection_source(src, tgt, kind, assumed);
        }

        // `unknown` is the top type: everything is assignable TO it.
        if tgt == wk.unknown {
            return Relation::Yes;
        }

        // `never` is the bottom type: it is assignable to everything. (Nothing
        // is assignable *to* `never` except `never`, which the `src == tgt` fast
        // path already accepted.)
        if src == wk.never {
            return Relation::Yes;
        }

        // `void` accepts `undefined` (and `void` itself, via `src == tgt`).
        if tgt == wk.void && src == wk.undefined {
            return Relation::Yes;
        }

        // Literal-to-base widening uses the literal type for the decision; only
        // diagnostics widen the displayed source.
        if let Some(lit) = self.store.literal_value(src) {
            let base = self.intrinsic_id(lit.base_kind());
            if base == tgt {
                return Relation::Yes;
            }
        }

        // Template literal patterns (M27). A surviving template node is a *pattern*
        // (a `string`/`number` intrinsic hole) or a *deferred* node (a free declaration
        // type parameter hole; an identical one is already accepted by the `src == tgt`
        // fast path). These run AFTER `unknown`/`never`/`void`/literal-widening (so
        // `template <: unknown`, `never <: template`, and a literal's own base widening are
        // decided by those general rules) and BEFORE the object rule. A template **source**
        // flows to `string` and to a subsuming pattern; a template **target** is matched by
        // a string literal (anchored segment matching) — `string` itself matches only the
        // bare `` `${string}` `` hole.
        if self.store.tag(src) == TypeTag::Template {
            return self.relate_template_source(src, tgt);
        }
        if self.store.tag(tgt) == TypeTag::Template {
            return self.relate_template_target(src, tgt);
        }

        // b64 readonly array/tuple wrapper: readonly sources are not mutable arrays,
        // but a readonly target accepts another readonly wrapper by its operand and
        // accepts a mutable array/tuple source through the wrapped target shape.
        if self.store.tag(tgt) == TypeTag::Readonly {
            let Some(tgt_operand) = self.store.readonly_operand(tgt) else {
                return Relation::No(ReasonChain::leaf(src, tgt));
            };
            let src_operand = if self.store.tag(src) == TypeTag::Readonly {
                self.store.readonly_operand(src).unwrap_or(src)
            } else {
                src
            };
            return self.relate(src_operand, tgt_operand, kind, assumed);
        }
        if self.store.tag(src) == TypeTag::Readonly {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }

        // Object structural rule; `relate_objects` owns width/depth, nominal, optional,
        // call/construct, and index-signature details.
        if self.store.tag(src) == TypeTag::Object && self.store.tag(tgt) == TypeTag::Object {
            return self.relate_objects(src, tgt, kind, assumed);
        }

        // Function structural rule; `relate_functions` owns arity and variance.
        if self.store.tag(src) == TypeTag::Function && self.store.tag(tgt) == TypeTag::Function {
            return self.relate_functions(src, tgt, kind, assumed);
        }

        // F1/WU2: a callable object can relate to a plain function type through
        // its single call signature. The function-signature comparison itself is
        // delegated back through `relate`, so variance, cycle assumptions, and
        // cache-soundness are identical to ordinary function relations.
        if self.store.tag(src) == TypeTag::Object && self.store.tag(tgt) == TypeTag::Function {
            return self.relate_object_to_function(src, tgt, kind, assumed);
        }
        if self.store.tag(src) == TypeTag::Function && self.store.tag(tgt) == TypeTag::Object {
            return self.relate_function_to_object(src, tgt, kind, assumed);
        }

        // Array structural rule; `relate_arrays` owns element covariance.
        if self.store.tag(src) == TypeTag::Array && self.store.tag(tgt) == TypeTag::Array {
            return self.relate_arrays(src, tgt, kind, assumed);
        }

        // Tuple structural rule; `relate_tuples` owns length and positional checks.
        if self.store.tag(src) == TypeTag::Tuple && self.store.tag(tgt) == TypeTag::Tuple {
            return self.relate_tuples(src, tgt, kind, assumed);
        }

        // Tuple → array rule; the reverse remains the conservative fallthrough.
        if self.store.tag(src) == TypeTag::Tuple && self.store.tag(tgt) == TypeTag::Array {
            return self.relate_tuple_to_array(src, tgt, kind, assumed);
        }

        // Otherwise: not assignable. Build the leaf reason on this failing path.
        Relation::No(ReasonChain::leaf(src, tgt))
    }

    /// `any` or the error type — both relate to everything.
    fn is_any_like(&self, id: TypeId) -> bool {
        if id == self.well_known.any || id == self.well_known.error {
            return true;
        }
        // Defensive: any type explicitly flagged as containing the error type.
        self.store.tag(id) == TypeTag::Intrinsic
            && matches!(
                self.store.intrinsic_kind(id),
                Some(IntrinsicKind::Any) | Some(IntrinsicKind::Error)
            )
    }

    /// The well-known id for an intrinsic kind.
    fn intrinsic_id(&self, kind: IntrinsicKind) -> TypeId {
        let wk = self.well_known;
        match kind {
            IntrinsicKind::Error => wk.error,
            IntrinsicKind::Any => wk.any,
            IntrinsicKind::Unknown => wk.unknown,
            IntrinsicKind::Never => wk.never,
            IntrinsicKind::Void => wk.void,
            IntrinsicKind::Null => wk.null,
            IntrinsicKind::Undefined => wk.undefined,
            IntrinsicKind::Boolean => wk.boolean,
            IntrinsicKind::Number => wk.number,
            IntrinsicKind::String => wk.string,
            // M28 string-intrinsic markers.
            IntrinsicKind::Uppercase => wk.uppercase,
            IntrinsicKind::Lowercase => wk.lowercase,
            IntrinsicKind::Capitalize => wk.capitalize,
            IntrinsicKind::Uncapitalize => wk.uncapitalize,
            IntrinsicKind::ThisType => wk.this_type,
            IntrinsicKind::OmitThisParameter => wk.omit_this_parameter,
        }
    }
}

#[cfg(test)]
mod tests;

//! calls module (extracted from checker/mod.rs).

use super::classes::application::{
    build_class_application, ClassApplicationKind, ClassApplicationRequest, ExplicitClassArgument,
    SourceClassArguments,
};
use super::classes::surface_types::SurfaceTypeFactory;
use super::context::*;
use super::decls::alloc_type_param_ids;
use super::eval::{contains_deferred_argument, contains_deferred_keyof};
use super::expr::{contextual_literal_target, ContextualRewalk};
use super::function_groups::FunctionGroupDemand;
use super::type_groups::{
    PublishedTypeGroupSurface, PublishedTypeGroupTerminal, TypeEnvironmentState,
};
use crate::binder::declaration::ValueStorageId;
use crate::binder::namespace::QualifiedTypePathResolution;
use crate::binder::scope::ScopeId;
use crate::check::infer;
use crate::class_semantics::{
    ClassApplicationArguments, ClassConstructionState, DemandOutcome, Exhaustion,
};
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic};
use crate::relate::RelationOutcome;
use crate::span::Span;
use crate::types::repr::{
    ClassId, FunctionType, GenericTypeParam, IntrinsicKind, ParameterType, TupleRestType,
    TupleType, TypeParamId, TypeTag,
};
use crate::types::store::TypeId;
use crate::types::{instantiate_function, substitute, Interner, WellKnown};
use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, BindingPattern, CallExpression, Expression, FormalParameter,
    FormalParameterRest, FormalParameters, Function, FunctionBody, NewExpression,
    TSTypeParameterInstantiation,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHasher};
use std::hash::{Hash, Hasher};

#[cfg(test)]
mod contextual_duplicate_diagnostics_spec;
#[cfg(test)]
mod contextual_rewalk_scaling_spec;

/// How many declaration slots a memoized walk may depend on or republish, and how
/// many distinct environments one node keeps entries for. A walk outside these bounds
/// is walked again rather than replayed — the memo is an optimization, and neither an
/// unbounded replay list nor an unbounded bucket scan is worth carrying.
const ARGUMENT_WALK_SLOT_BUDGET: usize = 64;
const ARGUMENT_WALK_BUCKET_BUDGET: usize = 8;

/// Everything an argument walk of one expression node reads that the node itself does
/// **not** fix and that is not a declaration type (backlog `95`).
///
/// The premise of the reverted first attempt (`412f321`) — that `this` and the current
/// class are the only such state — was false: a contextual walk rebinds the callback
/// parameter, so a walk performed with the parameter unbound was served to one running
/// with it bound. Declaration types are therefore **not** modelled here at all but
/// tracked exactly: a memoized walk records which slots it read before writing them,
/// and an entry is only served while every one of them still holds the value the walk
/// saw (`DeclTypes::dependencies_since`/`dependencies_hold`).
///
/// What remains is the rest of the pass's mutable state, which divides in three:
///
/// * **Named here** — `this`, the current class, the base-constructor signature, the
///   in-constructor flag, the type-parameter frames and their static barriers, the
///   enclosing class chain, and `next_type_param`, which covers every id allocated
///   fresh by a walk of the subtree.
/// * **Refused** — the type-lowering, replay-evidence, class-construction and
///   loop-fixpoint contexts. [`Pass::argument_walk_environment`] is the only
///   constructor and returns `None` in all of them, so a walk running there is simply
///   never memoized.
/// * **Deliberately absent** — the interner, the semantic-query caches, the flow-node
///   arena and its memo, and the reservation ledgers. These are content-addressed or
///   append-only: growing them does not change the answer to a query that was already
///   asked, which is the same premise the relation cache itself rests on
///   (`docs/reference/invariants.md` §1). Narrowing in particular is not ambient — the
///   flow-node CFG resolves a reference from its own `(module scope, span)`, both of
///   which the node fixes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::check::checker) struct WalkEnvironment {
    module: ScopeId,
    scope: ScopeId,
    start: u32,
    end: u32,
    this: Option<TypeId>,
    class: Option<ClassId>,
    super_ctor: Option<TypeId>,
    in_ctor: bool,
    /// Fresh type-parameter ids are allocated from this counter, so a walk of a
    /// subtree that declares a generic cannot collide with an earlier one.
    next_type_param: u32,
    /// Hash of the type-parameter scope stack, its static barriers, and the enclosing
    /// class chain — the frames a type reference inside the argument resolves through.
    type_params: u64,
}

/// One raw (context-free) walk of a call/`new` argument whose shape a committed
/// contextual walk can supersede. The raw walk takes no contextual type, so the
/// environment is the whole key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::check::checker) struct ArgumentWalkKey {
    environment: WalkEnvironment,
}

/// One effect-discarding contextual re-walk: the same environment, plus the raw type
/// it refines, the already-resolved contextual type it is walked against, and which of
/// the two branches (`use_contextual_arrow`) it takes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::check::checker) struct ContextualWalkKey {
    environment: WalkEnvironment,
    raw: TypeId,
    context: TypeId,
    use_contextual_arrow: bool,
}

/// A memoized walk's whole observable output, plus the declaration environment it is
/// valid in: the slots it read before writing them (`reads`) must still hold those
/// values, and the slots it published (`writes`) are replayed so that skipping the
/// walk leaves the same environment behind that performing it would have.
pub(in crate::check::checker) struct MemoizedWalk<T> {
    value: T,
    reads: Vec<(ValueStorageId, Option<TypeId>)>,
    writes: Vec<NetDeclWrite>,
}

/// A served entry, detached from the memo so the pass can be mutated while it is
/// adopted: the answer, the declaration slots it depended on, and the ones it wrote.
type ServedWalk<T> = (T, Vec<(ValueStorageId, Option<TypeId>)>, Vec<NetDeclWrite>);

pub(in crate::check::checker) type MemoizedArgumentWalk = MemoizedWalk<(TypeId, Span)>;
pub(in crate::check::checker) type MemoizedContextualWalk =
    MemoizedWalk<((TypeId, Span), ContextualRewalk)>;

impl<T: Copy> MemoizedWalk<T> {
    /// The entry in `bucket` whose recorded reads all still hold, if any.
    fn serve(bucket: &[Self], decl_types: &DeclTypes) -> Option<ServedWalk<T>> {
        bucket
            .iter()
            .find(|entry| decl_types.dependencies_hold(&entry.reads))
            .map(|entry| (entry.value, entry.reads.clone(), entry.writes.clone()))
    }
}

impl<Ticket: Copy + PartialEq> Pass<'_, '_, Ticket> {
    /// The environment key for a walk of `span` in `scope`, or `None` when this walk
    /// may not be memoized at all.
    ///
    /// The refusals are the other half of the completeness argument: rather than model
    /// the type-lowering, replay-evidence, and loop-fixpoint contexts, a walk running
    /// in any of them is simply never memoized. Each of them is rare and none of them
    /// occurs in the shapes this memo exists for.
    fn argument_walk_environment(&self, scope: ScopeId, span: Span) -> Option<WalkEnvironment> {
        // Only inside a call/`new` argument frame: that frame owns the memo's lifetime
        // and is the only place the declaration observation log is open.
        if self.argument_walk_depth == 0 {
            return None;
        }
        // Replay evidence records what each walk demanded; skipping a walk would drop
        // its dependencies. Suppressed effects are prelude lowering, not user checking.
        if self.replay_trace.is_some() || self.suppress_effects {
            return None;
        }
        // Type-declaration lowering contexts. A walk under any of these resolves names
        // through frames this key does not model.
        if self.building_template
            || !self.cond_frames.is_empty()
            || !self.mapped_frames.is_empty()
            || self.resolving_alias.is_some()
            || self.resolving_conditional_alias.is_some()
            || !self.resolving_alias_stack.is_empty()
            || self.alias_indirection_depth != 0
            || self.annotation_depth != 0
        {
            return None;
        }
        // A narrowed reference resolved under an unfinished loop fixpoint depends on a
        // provisional seed that is not durable (invariants §1, narrowing).
        if self.flow_loop_depth != 0 || !self.flow_provisional.is_empty() {
            return None;
        }
        // Class construction states: member surfaces are still moving.
        if self.staged_class_validation.is_some()
            || self.current_body_this_environment.is_some()
            || !matches!(self.type_environment, TypeEnvironmentState::Published(_))
        {
            return None;
        }
        Some(WalkEnvironment {
            module: self.current_module,
            scope,
            start: span.start,
            end: span.end,
            this: self.current_this,
            class: self.current_class,
            super_ctor: self.current_super_ctor,
            in_ctor: self.current_in_ctor,
            next_type_param: self.next_type_param,
            type_params: self.type_param_environment_hash(),
        })
    }

    /// Hash the type-parameter frames a reference inside the argument resolves
    /// through. Sorted per frame so two frames with the same bindings hash alike
    /// whatever order the map iterates in, and **empty frames are skipped**: a
    /// non-generic function still pushes one, and `lookup_type_param` walks the whole
    /// stack, so an empty frame shadows nothing and changes no resolution.
    fn type_param_environment_hash(&self) -> u64 {
        let mut hasher = FxHasher::default();
        for frame in self
            .type_param_scopes
            .iter()
            .filter(|frame| !frame.is_empty())
        {
            let mut entries: Vec<(&str, u32)> = frame
                .iter()
                .map(|(name, ty)| (name.as_str(), ty.0))
                .collect();
            entries.sort_unstable();
            entries.hash(&mut hasher);
        }
        for barrier in self
            .static_class_type_param_barriers
            .iter()
            .filter(|barrier| !barrier.is_empty())
        {
            let mut ids: Vec<u32> = barrier.iter().map(|id| id.0).collect();
            ids.sort_unstable();
            ids.hash(&mut hasher);
        }
        self.enclosing_classes.hash(&mut hasher);
        hasher.finish()
    }

    /// Adopt a served entry: inherit its declaration dependencies so the enclosing
    /// walk records them as its own, and republish the bindings it left behind.
    fn adopt_memoized_walk(
        &mut self,
        reads: &[(ValueStorageId, Option<TypeId>)],
        writes: &[NetDeclWrite],
    ) {
        DeclTypes::replay_dependencies(reads);
        self.decl_types.apply_writes(writes);
    }

    /// Record a walk's answer under `key`, unless it read or wrote more declarations
    /// than the memo carries or the node already has enough distinct environments.
    fn memoize_walk<T>(
        bucket: &mut Vec<MemoizedWalk<T>>,
        reads: Vec<(ValueStorageId, Option<TypeId>)>,
        writes: Vec<NetDeclWrite>,
        value: T,
    ) {
        if bucket.len() >= ARGUMENT_WALK_BUCKET_BUDGET
            || reads.len() > ARGUMENT_WALK_SLOT_BUDGET
            || writes.len() > ARGUMENT_WALK_SLOT_BUDGET
        {
            return;
        }
        bucket.push(MemoizedWalk {
            value,
            reads,
            writes,
        });
    }

    pub(in crate::check::checker) fn contextual_walk_key(
        &self,
        scope: ScopeId,
        raw: (TypeId, Span),
        context: TypeId,
        use_contextual_arrow: bool,
    ) -> Option<ContextualWalkKey> {
        Some(ContextualWalkKey {
            environment: self.argument_walk_environment(scope, raw.1)?,
            raw: raw.0,
            context,
            use_contextual_arrow,
        })
    }

    /// Serve a memoized contextual walk, republishing what it left behind.
    pub(in crate::check::checker) fn memoized_contextual_walk(
        &mut self,
        key: &ContextualWalkKey,
    ) -> Option<((TypeId, Span), ContextualRewalk)> {
        let bucket = self.contextual_walk_memo.get(key)?;
        let (value, reads, writes) = MemoizedWalk::serve(bucket, &self.decl_types)?;
        self.adopt_memoized_walk(&reads, &writes);
        #[cfg(test)]
        measure_call(|measure| measure.contextual_memo_hits += 1);
        Some(value)
    }

    pub(in crate::check::checker) fn memoize_contextual_walk(
        &mut self,
        key: ContextualWalkKey,
        mark: usize,
        value: ((TypeId, Span), ContextualRewalk),
    ) {
        let reads = DeclTypes::dependencies_since(mark);
        let writes = DeclTypes::net_writes_since(mark);
        Self::memoize_walk(
            self.contextual_walk_memo.entry(key).or_default(),
            reads,
            writes,
            value,
        );
    }
}

/// The argument shapes a committed contextual walk can re-walk: an arrow function,
/// or a fresh object/array literal (also through parentheses, which
/// `context_can_shape_fresh_literal` sees through). Every other shape is walked
/// exactly once, so its raw walk reports in place and nothing about it changes.
///
/// Over-answering `true` is safe — the held effects simply commit unchanged when the
/// re-walk declines — so this only has to stay a superset of what
/// `infer_contextual_source_after_walked` re-walks.
fn contextual_rewalk_candidate_shape(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::ArrowFunctionExpression(_)
        | Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_) => true,
        Expression::ParenthesizedExpression(paren) => {
            contextual_rewalk_candidate_shape(&paren.expression)
        }
        _ => false,
    }
}

fn flatten_static_class_value_path<'a>(
    expression: &'a Expression<'_>,
    segments: &mut Vec<&'a str>,
) -> bool {
    match expression {
        Expression::Identifier(identifier) => {
            segments.push(identifier.name.as_str());
            true
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            flatten_static_class_value_path(&parenthesized.expression, segments)
        }
        Expression::StaticMemberExpression(member) => {
            if !flatten_static_class_value_path(&member.object, segments) {
                return false;
            }
            segments.push(member.property.name.as_str());
            true
        }
        _ => false,
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ContextualMeasurePhase {
    #[default]
    CandidateInference,
    CandidateTrial,
    CommittedCheck,
    ClassCtor,
    Other,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CallMeasure {
    pub raw_call_argument_walks: u64,
    pub raw_construct_argument_walks: u64,
    pub speculative_candidate_builds: u64,
    pub committed_candidate_builds: u64,
    pub candidate_trials: u64,
    pub candidate_matches: u64,
    pub candidate_mismatches: u64,
    pub candidate_arity_failures: u64,
    pub generic_preliminary_inference_runs: u64,
    pub generic_full_inference_runs: u64,
    pub callback_rewalks: [u64; 5],
    pub fresh_literal_rewalks: [u64; 5],
    pub speculative_diagnostic_rollback_events: u64,
    pub speculative_diagnostics_removed: u64,
    pub trial_receiver_relation_queries: u64,
    pub selected_receiver_relation_queries: u64,
    pub speculative_query_forks: u64,
    pub speculative_query_writes_discarded: u64,
    /// Effect-discarding contextual re-walks served from the memo (backlog `95`).
    pub contextual_memo_hits: u64,
    /// Raw argument walks served from the memo (backlog `95`).
    pub raw_argument_memo_hits: u64,
}

#[cfg(test)]
impl CallMeasure {
    /// Every contextual re-walk of an argument expression, whatever its phase or kind —
    /// the quantity that branches per nesting level when the walk is not memoized.
    pub(super) fn contextual_argument_walks(&self) -> u64 {
        self.callback_rewalks
            .iter()
            .chain(&self.fresh_literal_rewalks)
            .sum()
    }
}

#[cfg(test)]
thread_local! {
    static CALL_MEASURE: std::cell::RefCell<CallMeasure> = std::cell::RefCell::new(CallMeasure::default());
}

#[cfg(test)]
pub(super) fn reset_call_measure() {
    CALL_MEASURE.with(|measure| *measure.borrow_mut() = CallMeasure::default());
}

#[cfg(test)]
pub(super) fn call_measure() -> CallMeasure {
    CALL_MEASURE.with(|measure| *measure.borrow())
}

#[cfg(test)]
pub(super) fn measure_contextual_rewalk(phase: ContextualMeasurePhase, callback: bool) {
    CALL_MEASURE.with(|measure| {
        let measure = &mut *measure.borrow_mut();
        let slot = phase as usize;
        if callback {
            measure.callback_rewalks[slot] += 1;
        } else {
            measure.fresh_literal_rewalks[slot] += 1;
        }
    });
}

#[cfg(test)]
thread_local! {
    static CONTEXTUAL_MEASURE_PHASE: std::cell::Cell<ContextualMeasurePhase> = const { std::cell::Cell::new(ContextualMeasurePhase::Other) };
}

#[cfg(test)]
struct ContextualMeasurePhaseGuard<'a> {
    current: &'a std::cell::Cell<ContextualMeasurePhase>,
    previous: ContextualMeasurePhase,
}

#[cfg(test)]
impl Drop for ContextualMeasurePhaseGuard<'_> {
    fn drop(&mut self) {
        self.current.set(self.previous);
    }
}

#[cfg(test)]
pub(super) fn with_contextual_measure_phase<R>(
    phase: ContextualMeasurePhase,
    body: impl FnOnce() -> R,
) -> R {
    CONTEXTUAL_MEASURE_PHASE.with(|current| {
        let _restore = ContextualMeasurePhaseGuard {
            previous: current.replace(phase),
            current,
        };
        body()
    })
}

#[cfg(test)]
pub(super) fn contextual_measure_phase() -> ContextualMeasurePhase {
    CONTEXTUAL_MEASURE_PHASE.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn measure_contextual_rollback(removed: usize) {
    CALL_MEASURE.with(|measure| {
        let measure = &mut *measure.borrow_mut();
        measure.speculative_diagnostic_rollback_events += 1;
        measure.speculative_diagnostics_removed += removed as u64;
    });
}

#[cfg(test)]
fn measure_call(update: impl FnOnce(&mut CallMeasure)) {
    CALL_MEASURE.with(|measure| update(&mut measure.borrow_mut()));
}

macro_rules! contextual_source_after_walked {
    ($pass:expr, $scope:expr, $expr:expr, $context:expr, $raw:expr, $arrow:expr, $retain:expr, $phase:expr) => {{
        #[cfg(test)]
        {
            with_contextual_measure_phase($phase, || {
                $pass.infer_contextual_source_after_walked(
                    $scope, $expr, $context, $raw, $arrow, $retain,
                )
            })
        }
        #[cfg(not(test))]
        {
            $pass.infer_contextual_source_after_walked(
                $scope, $expr, $context, $raw, $arrow, $retain,
            )
        }
    }};
}

macro_rules! contextual_source_after_walked_reporting {
    ($pass:expr, $scope:expr, $expr:expr, $context:expr, $raw:expr, $arrow:expr, $retain:expr, $phase:expr) => {{
        #[cfg(test)]
        {
            with_contextual_measure_phase($phase, || {
                $pass.infer_contextual_source_after_walked_reporting(
                    $scope, $expr, $context, $raw, $arrow, $retain,
                )
            })
        }
        #[cfg(not(test))]
        {
            $pass.infer_contextual_source_after_walked_reporting(
                $scope, $expr, $context, $raw, $arrow, $retain,
            )
        }
    }};
}

macro_rules! contextual_inference_args {
    ($pass:expr, $scope:expr, $params:expr, $types:expr, $exprs:expr, $phase:expr) => {{
        #[cfg(test)]
        {
            $pass.contextual_inference_args($scope, $params, $types, $exprs, $phase)
        }
        #[cfg(not(test))]
        {
            $pass.contextual_inference_args($scope, $params, $types, $exprs)
        }
    }};
}

pub(in crate::check::checker) struct RetainedFunctionBodySurface<
    Ticket: Copy = super::events::UserRecordTicket,
> {
    pub type_param_frame: FxHashMap<String, TypeId>,
    pub receiver: Option<TypeId>,
    pub params: Vec<Option<ParameterType>>,
    pub declared_return: Option<TypeId>,
    pub tickets: Option<super::lexical_events::CallableTickets<Ticket>>,
}

pub(in crate::check::checker) enum FunctionReservation<
    Ticket: Copy = super::events::UserRecordTicket,
> {
    Ready(FunctionSurface<Ticket>),
    Unavailable(RetainedFunctionBodySurface<Ticket>),
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Check already-lowered type arguments against constraint sources. Signature
    /// default validation shares this with call-site explicit arguments.
    pub(in crate::check::checker) fn check_constraint_arguments(
        &mut self,
        args: &[(Option<TypeId>, TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) {
        if self.building_template {
            let owner = self.current_replay_owner();
            self.effect_stack
                .last_mut()
                .expect("construction constraint check requires a lexical owner")
                .push_constraint_check(
                    ConstraintCheckObligation {
                        checks: args.to_vec(),
                        substitutions: map.clone(),
                    },
                    owner,
                );
            return;
        }
        let outcome = self.check_constraint_arguments_outcome(args, map);
        if let (DemandOutcome::Exhausted(exhaustion), Some((_, _, span))) = (outcome, args.first())
        {
            self.own_type_demand(DemandOutcome::Exhausted(exhaustion), *span);
        }
    }

    fn check_constraint_arguments_outcome(
        &mut self,
        args: &[(Option<TypeId>, TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) -> DemandOutcome<bool> {
        // Build the (argument, substituted-constraint, span) checks up front — this needs
        // `&mut Interner` (substitution may intern new types), which cannot overlap the
        // relation engine's immutable store borrow below.
        let mut checks: Vec<(TypeId, TypeId, TypeId, Span)> = Vec::new();
        for &(raw_constraint, arg, span) in args {
            let Some(constraint) = raw_constraint else {
                continue;
            };
            let substituted = substitute(self.interner, constraint, map);
            // A substituted constraint may be pending; resolve it before relating.
            let evaluated = match self.evaluate_type(substituted) {
                DemandOutcome::Ready(evaluated) => evaluated,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            };
            // Concrete instantiation decides deferred `keyof` constraints later.
            if contains_deferred_keyof(self.interner, evaluated) {
                continue;
            }
            // Decidable argument compositions check precisely; deferred ones stay conservative.
            let evaluated_arg = match self.evaluate_type(arg) {
                DemandOutcome::Ready(evaluated) => evaluated,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            };
            checks.push((evaluated_arg, arg, evaluated, span));
        }
        if checks.is_empty() {
            return DemandOutcome::Ready(false);
        }

        // Render relation failures before mutating the effect stack.
        let mut failures: Vec<(String, String, Span, Vec<String>)> = Vec::new();
        for (evaluated_arg, written_arg, constraint, span) in checks {
            match self.with_semantic_query(|query| query.is_assignable(evaluated_arg, constraint)) {
                RelationOutcome::Yes => {}
                RelationOutcome::No(chain) => {
                    let store = self.interner.store();
                    // Preserve the written argument when evaluation remains deferred.
                    let render_id = if contains_deferred_argument(store, evaluated_arg) {
                        written_arg
                    } else {
                        evaluated_arg
                    };
                    let src = render_type(store, render_id, true);
                    let tgt = render_type(store, constraint, false);
                    let elaboration = render_reason_chain(store, chain.head());
                    failures.push((src, tgt, span, elaboration));
                }
                RelationOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            }
        }
        let failed = !failures.is_empty();
        for (src, tgt, span, elaboration) in failures {
            self.emit_diagnostic(
                Diagnostic::constraint_not_satisfied(span, &src, &tgt)
                    .with_elaboration(elaboration),
            );
        }
        DemandOutcome::Ready(failed)
    }
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Run speculative candidate work against a child semantic-query state.
    /// Diagnostics and obligations travel in `CheckerEffects`; query memo/cache
    /// writes are discarded independently when the candidate is not decisive.
    fn capture_speculative_candidate_effects<R>(
        &mut self,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> (R, CheckerEffects<Ticket>) {
        #[cfg(test)]
        let parent_lengths = self.semantic_queries.durable_lengths();
        self.semantic_queries.savepoint();
        let captured = self.capture_candidate_effects(produce);
        #[cfg(test)]
        let child_lengths = self.semantic_queries.durable_lengths();
        self.semantic_queries.rollback();
        #[cfg(test)]
        self.measure_discarded_candidate_queries(parent_lengths, child_lengths);
        captured
    }

    /// Isolate trial-only relation/evaluation writes. A selected candidate is
    /// rebuilt once against the durable parent after the trial succeeds.
    fn with_speculative_candidate_queries<R>(&mut self, produce: impl FnOnce(&mut Self) -> R) -> R {
        #[cfg(test)]
        let parent_lengths = self.semantic_queries.durable_lengths();
        self.semantic_queries.savepoint();
        let result = produce(self);
        #[cfg(test)]
        let child_lengths = self.semantic_queries.durable_lengths();
        self.semantic_queries.rollback();
        #[cfg(test)]
        self.measure_discarded_candidate_queries(parent_lengths, child_lengths);
        result
    }

    /// `child_lengths` is sampled before the rollback, so the discarded count keeps
    /// its original meaning: durable growth this speculative layer threw away.
    #[cfg(test)]
    fn measure_discarded_candidate_queries(
        &self,
        parent_lengths: (usize, usize, usize, usize, usize),
        child_lengths: (usize, usize, usize, usize, usize),
    ) {
        let discarded = child_lengths.0.saturating_sub(parent_lengths.0)
            + child_lengths.1.saturating_sub(parent_lengths.1)
            + child_lengths.2.saturating_sub(parent_lengths.2)
            + child_lengths.3.saturating_sub(parent_lengths.3)
            + child_lengths.4.saturating_sub(parent_lengths.4);
        measure_call(|measure| {
            measure.speculative_query_forks += 1;
            measure.speculative_query_writes_discarded +=
                u64::try_from(discarded).expect("query write count fits u64");
        });
        debug_assert_eq!(self.semantic_queries.durable_lengths(), parent_lengths);
    }

    /// Record the incomplete surface for a skipped spread call/`new` argument
    /// (`f(...xs)` / `new C(...xs)`, owner 71) — the argument collectors share this so
    /// no in-scope argument is silently dropped before arity/assignability checking.
    fn record_spread_argument_skip(&mut self, arg: &oxc_ast::ast::Argument<'_>) {
        self.record_incomplete(
            "call/call-arguments/spread-argument",
            Span::from_oxc(arg.span()),
            "spread call argument not visited",
        );
    }

    /// M24: check explicit type arguments against substituted constraints. The
    /// shared relation engine supplies `TK2344` reason chains; failed arguments
    /// still instantiate, matching tsc and avoiding cascades.
    pub(in crate::check::checker) fn check_type_argument_constraints(
        &mut self,
        type_params: &[TypeParamId],
        args: &[(TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) {
        let checks: Vec<(Option<TypeId>, TypeId, Span)> = type_params
            .iter()
            .zip(args)
            .map(|(&param, &(arg, span))| {
                (
                    self.interner.store().type_param_constraint(param),
                    arg,
                    span,
                )
            })
            .collect();
        if self.building_template {
            let owner = self.current_replay_owner();
            self.effect_stack
                .last_mut()
                .expect("construction constraint check requires a lexical owner")
                .push_constraint_check(
                    ConstraintCheckObligation {
                        checks,
                        substitutions: map.clone(),
                    },
                    owner,
                );
            return;
        }
        let outcome = self.check_constraint_arguments_outcome(&checks, map);
        if let (DemandOutcome::Exhausted(exhaustion), Some((_, span))) = (outcome, args.first()) {
            self.own_type_demand(DemandOutcome::Exhausted(exhaustion), *span);
        }
    }

    /// Check explicit function-signature arguments against their persistent
    /// descriptors. The descriptor can already have been rewritten by an outer
    /// class/interface substitution, unlike the declaration-side store column.
    fn check_signature_type_argument_constraints(
        &mut self,
        type_params: &[GenericTypeParam],
        args: &[(TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) -> DemandOutcome<bool> {
        let checks: Vec<(Option<TypeId>, TypeId, Span)> = type_params
            .iter()
            .zip(args)
            .map(|(param, &(arg, span))| (param.constraint, arg, span))
            .collect();
        self.check_constraint_arguments_outcome(&checks, map)
    }

    /// Walk one call/`new` argument for the type candidate selection needs, holding
    /// the walk's effects when a committed contextual walk may supersede them.
    ///
    /// The held batch is index-aligned with `arg_types` by
    /// [`Pass::hold_provisional_argument_effects`]; an argument whose type inference
    /// yields nothing is never paired with a parameter, so its effects commit here.
    ///
    /// A re-walkable shape is also memoized per call region (backlog `95`):
    /// re-executing the enclosing call — which is what a contextual re-walk of an
    /// enclosing argument does — otherwise re-walks this whole subtree, and that is
    /// one of the two factors that made nesting exponential. Only a walk that
    /// produced **no effects at all** is memoized, so serving it emits nothing that a
    /// real walk would have emitted and there is nothing to recover later; the
    /// declaration bindings it published are replayed so the environment it leaves
    /// behind is the one a real walk would leave.
    fn walk_raw_argument(
        &mut self,
        scope: ScopeId,
        argument_index: usize,
        arg_expr: &Expression<'_>,
    ) -> Option<(TypeId, Span)> {
        if !contextual_rewalk_candidate_shape(arg_expr) {
            let inferred = self.infer_expr(scope, arg_expr);
            if inferred.is_some() {
                self.hold_provisional_argument_effects(ProvisionalArgumentWalk::Settled);
            }
            return inferred;
        }
        let key = self
            .argument_walk_environment(scope, Span::from_oxc(arg_expr.span()))
            .map(|environment| ArgumentWalkKey { environment });
        if let Some(key) = &key {
            if let Some(served) = self
                .raw_argument_walk_memo
                .get(key)
                .and_then(|bucket| MemoizedWalk::serve(bucket, &self.decl_types))
            {
                let (value, reads, writes) = served;
                self.adopt_memoized_walk(&reads, &writes);
                #[cfg(test)]
                measure_call(|measure| measure.raw_argument_memo_hits += 1);
                self.hold_provisional_argument_effects(ProvisionalArgumentWalk::Memoized {
                    argument_index,
                });
                return Some(value);
            }
        }
        let mark = DeclTypes::log_mark();
        let (inferred, effects) =
            self.capture_candidate_effects(|pass| pass.infer_expr(scope, arg_expr));
        match inferred {
            Some(inferred) => {
                if let Some(key) = key {
                    let reads = DeclTypes::dependencies_since(mark);
                    let writes = DeclTypes::net_writes_since(mark);
                    Self::memoize_walk(
                        self.raw_argument_walk_memo.entry(key).or_default(),
                        reads,
                        writes,
                        inferred,
                    );
                }
                self.hold_provisional_argument_effects(ProvisionalArgumentWalk::Held(effects));
                Some(inferred)
            }
            None => {
                self.merge_candidate_effects(effects);
                None
            }
        }
    }

    /// Re-run every raw argument walk this call served from the memo and that nothing
    /// superseded, so its records exist and are parked where the un-memoized walk's
    /// were (backlog `95`).
    ///
    /// Runs on **every** exit path of the call, immediately before the frame closes, so
    /// the shapes `tests/cases/b92_contextual_duplicate_diagnostics/retained_raw_walks.ts`
    /// pins — a generic arrow, a multi-signature contextual target, failed overload
    /// resolution, a `never` parameter cutting the committed loop short — report exactly
    /// as they do without the memo. The batch is parked rather than emitted so it still
    /// merges at frame close, keeping record order identical too.
    pub(in crate::check::checker) fn rewalk_memoized_raw_arguments(
        &mut self,
        scope: ScopeId,
        arguments: &[Argument<'_>],
    ) {
        let Some(frame) = self.provisional_argument_effects.last() else {
            return;
        };
        let pending: Vec<(usize, usize)> = frame
            .iter()
            .enumerate()
            .filter_map(|(slot, walk)| match walk {
                ProvisionalArgumentWalk::Memoized { argument_index } => {
                    Some((slot, *argument_index))
                }
                ProvisionalArgumentWalk::Settled | ProvisionalArgumentWalk::Held(_) => None,
            })
            .collect();
        for (slot, argument_index) in pending {
            let Some(arg_expr) = arguments
                .get(argument_index)
                .and_then(|argument| argument.as_expression())
            else {
                continue;
            };
            let (_, effects) =
                self.capture_candidate_effects(|pass| pass.infer_expr(scope, arg_expr));
            if let Some(frame) = self.provisional_argument_effects.last_mut() {
                if let Some(entry) = frame.get_mut(slot) {
                    *entry = ProvisionalArgumentWalk::Held(effects);
                }
            }
        }
    }

    fn contextual_inference_args(
        &mut self,
        scope: ScopeId,
        params: &[ParameterType],
        arg_types: &[(TypeId, Span)],
        arg_exprs: &[&Expression<'_>],
        #[cfg(test)] phase: ContextualMeasurePhase,
    ) -> Vec<TypeId> {
        let targets = self.call_argument_targets(params, arg_types.len());
        arg_types
            .iter()
            .zip(arg_exprs)
            .zip(targets)
            .map(|(((arg_ty, arg_span), arg_expr), target)| {
                let Some(target) = target else {
                    return *arg_ty;
                };
                if self.should_keep_raw_array_inference_source(arg_expr, *arg_ty, target) {
                    return *arg_ty;
                }
                contextual_source_after_walked!(
                    self,
                    scope,
                    arg_expr,
                    target,
                    (*arg_ty, *arg_span),
                    true,
                    false,
                    phase
                )
                .0
            })
            .collect()
    }

    fn should_keep_raw_array_inference_source(
        &self,
        arg_expr: &Expression<'_>,
        arg_ty: TypeId,
        target: TypeId,
    ) -> bool {
        if !matches!(arg_expr, Expression::ArrayExpression(_)) {
            return false;
        }
        let Some(target_array) = self.interner.store().array_type(target) else {
            return false;
        };
        if self.interner.store().tag(target_array.element) != TypeTag::TypeParam {
            return false;
        }
        self.interner.store().array_type(arg_ty).is_some()
    }

    /// Infer and check a call. Callable callees are function types or objects with
    /// one call signature; non-callables still yield the error type silently until
    /// the dedicated diagnostic can account for dropped callability and overloads.
    pub(in crate::check::checker) fn infer_call(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        self.with_provisional_argument_effects(
            |pass| pass.infer_call_inner(scope, call),
            |pass| pass.rewalk_memoized_raw_arguments(scope, &call.arguments),
        )
    }

    fn infer_call_inner(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();
        let call_span = Span::from_oxc(call.span);

        // M12: a `super(args)` call (callee is `Super`) — check against the **base
        // constructor** signature in scope, not as an ordinary call. Handled before the
        // generic/callee machinery (`Super` is not an identifier and has no callee type).
        if matches!(call.callee, Expression::Super(_)) {
            return self.infer_super_call(scope, call, call_span);
        }

        // A member call receives the member object as its explicit receiver; infer it
        // once and reuse that result for the property lookup so nested expressions and
        // diagnostics retain their ordinary single evaluation.
        let mut callee = &call.callee;
        while let Expression::ParenthesizedExpression(paren) = callee {
            callee = &paren.expression;
        }
        let direct_function_group = match callee {
            Expression::Identifier(identifier) => self
                .resolve_value_replay(scope, identifier.name.as_str())
                .map(|symbol| self.demand_function_group_replay(symbol))
                .filter(|demand| !matches!(demand, FunctionGroupDemand::NotGroup)),
            _ => None,
        };
        let direct_standalone = self.complete_standalone_namespace_value(scope, callee);
        let mut function_group_blocked = false;
        let (inferred_callee, call_receiver) =
            match direct_function_group {
                Some(FunctionGroupDemand::Ready(ty) | FunctionGroupDemand::PrivateSelf(ty)) => {
                    (Some((ty, Span::from_oxc(callee.span()))), None)
                }
                Some(FunctionGroupDemand::Pending { report_use }) => {
                    if report_use {
                        self.record_incomplete(
                            "expr-infer/call-expression/function-group-pending",
                            call_span,
                            "merged function callable waits for body return inference",
                        );
                    }
                    function_group_blocked = true;
                    (None, None)
                }
                Some(FunctionGroupDemand::Unavailable) => {
                    function_group_blocked = true;
                    (None, None)
                }
                Some(FunctionGroupDemand::NotGroup) => unreachable!(),
                None => match callee {
                    Expression::StaticMemberExpression(member) => {
                        let inferred_receiver = self.infer_expr(scope, &member.object);
                        let inferred_callee =
                            match self.demand_class_value_surface(scope, &member.object) {
                                Some(DemandOutcome::Exhausted(exhaustion)) => {
                                    self.own_type_demand(
                                        DemandOutcome::Exhausted(exhaustion),
                                        Span::from_oxc(member.property.span),
                                    );
                                    None
                                }
                                Some(DemandOutcome::Ready(())) | None => inferred_receiver
                                    .and_then(|(receiver, _)| {
                                        self.infer_member_access_from_base(scope, receiver, member)
                                    }),
                            };
                        (inferred_callee, inferred_receiver)
                    }
                    Expression::ComputedMemberExpression(member) => {
                        let inferred_receiver = self.infer_expr(scope, &member.object);
                        let inferred_callee =
                            match self.demand_class_value_surface(scope, &member.object) {
                                Some(DemandOutcome::Exhausted(exhaustion)) => {
                                    self.infer_expr(scope, &member.expression);
                                    self.own_type_demand(
                                        DemandOutcome::Exhausted(exhaustion),
                                        Span::from_oxc(member.span),
                                    );
                                    None
                                }
                                Some(DemandOutcome::Ready(())) | None => inferred_receiver
                                    .and_then(|(receiver, _)| {
                                        self.infer_element_access_from_base(scope, receiver, member)
                                    }),
                            };
                        (inferred_callee, inferred_receiver)
                    }
                    _ => (self.infer_expr(scope, &call.callee), None),
                },
            };

        // Infer arguments up front and build `arg_fresh` in the same loop so M24
        // clamp provenance stays index-aligned with skipped out-of-subset args.
        let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(call.arguments.len());
        let mut arg_fresh: Vec<bool> = Vec::with_capacity(call.arguments.len());
        let mut arg_exprs: Vec<&Expression<'_>> = Vec::with_capacity(call.arguments.len());
        for (argument_index, arg) in call.arguments.iter().enumerate() {
            if let Some(arg_expr) = arg.as_expression() {
                #[cfg(test)]
                measure_call(|measure| measure.raw_call_argument_walks += 1);
                if let Some(inferred) = self.walk_raw_argument(scope, argument_index, arg_expr) {
                    arg_types.push(inferred);
                    arg_fresh.push(is_fresh_literal(arg_expr));
                    arg_exprs.push(arg_expr);
                }
            } else {
                // A spread argument `f(...xs)` is not paired against a parameter (owner 71).
                self.record_spread_argument_skip(arg);
            }
        }

        let Some((callee_ty, _)) = inferred_callee else {
            if function_group_blocked {
                return None;
            }
            return Some((wk.error, call_span));
        };
        let outcome = self.evaluate_type(callee_ty);
        let callee_ty = self.own_type_demand(outcome, call_span)?;
        let signatures = match self.callable_signatures(callee_ty) {
            DemandOutcome::Ready(CallableSignatures::Ready(signatures)) => signatures,
            DemandOutcome::Ready(CallableSignatures::ProvablyNone) => {
                self.emit_diagnostic(Diagnostic::expression_is_not_callable(call_span));
                return Some((wk.error, call_span));
            }
            DemandOutcome::Ready(CallableSignatures::Unavailable) => {
                if direct_standalone || self.provably_non_callable(callee_ty) {
                    self.emit_diagnostic(Diagnostic::expression_is_not_callable(call_span));
                }
                return Some((wk.error, call_span));
            }
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), call_span);
                return None;
            }
        };

        let candidate = match self.select_call_candidate(
            scope,
            call,
            &signatures,
            PreparedCallArgs {
                types: &arg_types,
                fresh: &arg_fresh,
                exprs: &arg_exprs,
            },
            call_span,
            call_receiver.map(|(receiver, _)| receiver),
        ) {
            DemandOutcome::Ready(Some(candidate)) => candidate,
            DemandOutcome::Ready(None) => return Some((wk.error, call_span)),
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), call_span);
                return None;
            }
        };
        if let DemandOutcome::Exhausted(exhaustion) =
            self.check_call_receiver(candidate.receiver, call_receiver, call_span)
        {
            self.own_type_demand(DemandOutcome::Exhausted(exhaustion), call_span);
            return None;
        }
        self.check_call_arguments(scope, &candidate.params, &arg_types, &arg_exprs, call_span);
        if candidate.inference_exhaustion.is_some() {
            return None;
        }
        let ret = match self.evaluate_type(candidate.ret) {
            DemandOutcome::Ready(ret) => ret,
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), call_span);
                return None;
            }
        };

        Some((ret, call_span))
    }

    /// Callable signatures after apparent-type resolution: a function type or an
    /// object's ordered call-signature list.
    fn callable_signatures(&mut self, callee_ty: TypeId) -> DemandOutcome<CallableSignatures> {
        let callee_ty = self.apparent_type(callee_ty);
        match self.interner.store().tag(callee_ty) {
            TypeTag::Function => DemandOutcome::Ready(CallableSignatures::Ready(vec![callee_ty])),
            TypeTag::Object => {
                let Some(object) = self.interner.store().object_type(callee_ty) else {
                    return DemandOutcome::Ready(CallableSignatures::Unavailable);
                };
                if object.call_signatures.is_empty() {
                    DemandOutcome::Ready(CallableSignatures::ProvablyNone)
                } else {
                    DemandOutcome::Ready(CallableSignatures::Ready(object.call_signatures.clone()))
                }
            }
            TypeTag::Union => self.union_callable_signatures(callee_ty),
            _ if self.provably_non_callable(callee_ty) => {
                DemandOutcome::Ready(CallableSignatures::ProvablyNone)
            }
            _ => DemandOutcome::Ready(CallableSignatures::Unavailable),
        }
    }

    /// Build the signatures callable on every member of a union. This follows
    /// TypeScript's ordered `getUnionSignatures` protocol: matching overload rows
    /// are combined once, and the first duplicate row remains the representative.
    fn union_callable_signatures(&mut self, union_ty: TypeId) -> DemandOutcome<CallableSignatures> {
        let Some(members) = self.interner.store().union_members(union_ty) else {
            return DemandOutcome::Ready(CallableSignatures::Unavailable);
        };
        let members = members.to_vec();
        let mut lists = Vec::with_capacity(members.len());
        for member in members {
            match self.callable_signatures(member) {
                DemandOutcome::Ready(CallableSignatures::Ready(signatures)) => {
                    lists.push(signatures)
                }
                DemandOutcome::Ready(CallableSignatures::ProvablyNone) => {
                    return DemandOutcome::Ready(CallableSignatures::ProvablyNone)
                }
                DemandOutcome::Ready(CallableSignatures::Unavailable) => {
                    return DemandOutcome::Ready(CallableSignatures::Unavailable)
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            }
        }

        match self.common_union_signatures(&lists) {
            DemandOutcome::Ready(CommonUnionSignatures::Ready(signatures))
                if signatures.is_empty() =>
            {
                DemandOutcome::Ready(CallableSignatures::ProvablyNone)
            }
            DemandOutcome::Ready(CommonUnionSignatures::Ready(signatures)) => {
                DemandOutcome::Ready(CallableSignatures::Ready(signatures))
            }
            DemandOutcome::Ready(CommonUnionSignatures::Unavailable) => {
                DemandOutcome::Ready(CallableSignatures::Unavailable)
            }
            DemandOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
        }
    }

    fn common_union_signatures(
        &mut self,
        lists: &[Vec<TypeId>],
    ) -> DemandOutcome<CommonUnionSignatures> {
        let mut result = Vec::new();
        let mut overloaded_list = None;
        let mut multiple_overloaded_lists = false;

        for (list_index, list) in lists.iter().enumerate() {
            if list.is_empty() {
                return DemandOutcome::Ready(CommonUnionSignatures::Ready(Vec::new()));
            }
            if list.len() > 1 {
                if overloaded_list.is_some() {
                    multiple_overloaded_lists = true;
                } else {
                    overloaded_list = Some(list_index);
                }
            }
            for &signature in list {
                match self.find_matching_signature(&result, signature, false, true) {
                    DemandOutcome::Ready(SignatureSearch::Found(_)) => continue,
                    DemandOutcome::Ready(SignatureSearch::NotFound) => {}
                    DemandOutcome::Ready(SignatureSearch::Unavailable) => {
                        return DemandOutcome::Ready(CommonUnionSignatures::Unavailable)
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                }
                let matches = match self.matching_union_signatures(lists, signature, list_index) {
                    DemandOutcome::Ready(SignatureMatches::Found(matches)) => matches,
                    DemandOutcome::Ready(SignatureMatches::NotFound) => continue,
                    DemandOutcome::Ready(SignatureMatches::Unavailable) => {
                        return DemandOutcome::Ready(CommonUnionSignatures::Unavailable)
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                };
                let mut combined = Some(signature);
                for matched in matches {
                    if matched != signature {
                        let Some(current) = combined else {
                            break;
                        };
                        combined = match self.combine_union_signature_pair(
                            current,
                            matched,
                            SignatureCombinationMode::Matching,
                        ) {
                            DemandOutcome::Ready(SignatureCombination::Combined(combined)) => {
                                Some(combined)
                            }
                            DemandOutcome::Ready(SignatureCombination::NoCommon) => None,
                            DemandOutcome::Ready(SignatureCombination::Unavailable) => {
                                return DemandOutcome::Ready(CommonUnionSignatures::Unavailable)
                            }
                            DemandOutcome::Exhausted(exhaustion) => {
                                return DemandOutcome::Exhausted(exhaustion)
                            }
                        };
                    }
                }
                if let Some(combined) = combined {
                    result.push(combined);
                }
            }
        }

        if !result.is_empty() || multiple_overloaded_lists {
            return DemandOutcome::Ready(CommonUnionSignatures::Ready(result));
        }

        let master_index = overloaded_list.unwrap_or(0);
        let mut fallback = lists[master_index].clone();
        for (list_index, list) in lists.iter().enumerate() {
            if list_index == master_index {
                continue;
            }
            let Some(&signature) = list.first() else {
                return DemandOutcome::Ready(CommonUnionSignatures::Ready(Vec::new()));
            };
            let mut combined = Vec::with_capacity(fallback.len());
            for existing in fallback {
                let next = match self.combine_union_signature_pair(
                    existing,
                    signature,
                    SignatureCombinationMode::Fallback,
                ) {
                    DemandOutcome::Ready(SignatureCombination::Combined(next)) => next,
                    DemandOutcome::Ready(SignatureCombination::NoCommon) => {
                        return DemandOutcome::Ready(CommonUnionSignatures::Ready(Vec::new()))
                    }
                    DemandOutcome::Ready(SignatureCombination::Unavailable) => {
                        return DemandOutcome::Ready(CommonUnionSignatures::Unavailable)
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                };
                combined.push(next);
            }
            fallback = combined;
        }
        DemandOutcome::Ready(CommonUnionSignatures::Ready(fallback))
    }

    fn matching_union_signatures(
        &mut self,
        lists: &[Vec<TypeId>],
        signature: TypeId,
        list_index: usize,
    ) -> DemandOutcome<SignatureMatches> {
        let generic = self
            .interner
            .store()
            .function_type(signature)
            .is_some_and(|function| !function.type_params.is_empty());
        if generic {
            if list_index != 0 {
                return DemandOutcome::Ready(SignatureMatches::NotFound);
            }
            for list in lists.iter().skip(1) {
                match self.find_matching_signature(list, signature, false, false) {
                    DemandOutcome::Ready(SignatureSearch::Found(_)) => {}
                    DemandOutcome::Ready(SignatureSearch::NotFound) => {
                        return DemandOutcome::Ready(SignatureMatches::NotFound)
                    }
                    DemandOutcome::Ready(SignatureSearch::Unavailable) => {
                        return DemandOutcome::Ready(SignatureMatches::Unavailable)
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                }
            }
            return DemandOutcome::Ready(SignatureMatches::Found(vec![signature]));
        }

        let mut matches = Vec::new();
        for (index, list) in lists.iter().enumerate() {
            let matched = if index == list_index {
                SignatureSearch::Found(signature)
            } else {
                match self.find_matching_signature(list, signature, false, true) {
                    DemandOutcome::Ready(SignatureSearch::Found(found)) => {
                        SignatureSearch::Found(found)
                    }
                    DemandOutcome::Ready(SignatureSearch::NotFound) => {
                        match self.find_matching_signature(list, signature, true, true) {
                            DemandOutcome::Ready(search) => search,
                            DemandOutcome::Exhausted(exhaustion) => {
                                return DemandOutcome::Exhausted(exhaustion)
                            }
                        }
                    }
                    DemandOutcome::Ready(SignatureSearch::Unavailable) => {
                        return DemandOutcome::Ready(SignatureMatches::Unavailable)
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                }
            };
            let matched = match matched {
                SignatureSearch::Found(matched) => matched,
                SignatureSearch::NotFound => {
                    return DemandOutcome::Ready(SignatureMatches::NotFound)
                }
                SignatureSearch::Unavailable => {
                    return DemandOutcome::Ready(SignatureMatches::Unavailable)
                }
            };
            if !matches.contains(&matched) {
                matches.push(matched);
            }
        }
        DemandOutcome::Ready(SignatureMatches::Found(matches))
    }

    fn find_matching_signature(
        &mut self,
        signatures: &[TypeId],
        target: TypeId,
        partial: bool,
        ignore_return: bool,
    ) -> DemandOutcome<SignatureSearch> {
        let mut saw_unavailable = false;
        for &source in signatures {
            match self.signatures_identical(source, target, partial, ignore_return) {
                DemandOutcome::Ready(SignatureComparison::Match) => {
                    return DemandOutcome::Ready(SignatureSearch::Found(source))
                }
                DemandOutcome::Ready(SignatureComparison::Mismatch) => {}
                DemandOutcome::Ready(SignatureComparison::Unavailable) => {
                    saw_unavailable = true;
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            }
        }
        DemandOutcome::Ready(if saw_unavailable {
            SignatureSearch::Unavailable
        } else {
            SignatureSearch::NotFound
        })
    }

    fn signatures_identical(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
        partial: bool,
        ignore_return: bool,
    ) -> DemandOutcome<SignatureComparison> {
        let Some(source) = self.interner.store().function_type(source_ty).cloned() else {
            return DemandOutcome::Ready(SignatureComparison::Unavailable);
        };
        let Some(target) = self.interner.store().function_type(target_ty).cloned() else {
            return DemandOutcome::Ready(SignatureComparison::Unavailable);
        };
        let Some(source_count) = self.effective_parameter_count(&source) else {
            return DemandOutcome::Ready(SignatureComparison::Unavailable);
        };
        let Some(target_count) = self.effective_parameter_count(&target) else {
            return DemandOutcome::Ready(SignatureComparison::Unavailable);
        };
        let source_arity = self.call_arity(&source.params);
        let target_arity = self.call_arity(&target.params);
        let source_rest = source_arity.max.is_none();
        let target_rest = target_arity.max.is_none();
        let exact_shape = source_count == target_count
            && source_arity.min == target_arity.min
            && source_rest == target_rest;
        let partial_shape = partial && source_arity.min <= target_arity.min;
        if !exact_shape && !partial_shape {
            return DemandOutcome::Ready(SignatureComparison::Mismatch);
        }

        let Some(map) = self.signature_alpha_map(&source, &target, true) else {
            return DemandOutcome::Ready(SignatureComparison::Mismatch);
        };
        if let Some(source_receiver) = source.receiver {
            let Some(target_receiver) = target.receiver else {
                return DemandOutcome::Ready(SignatureComparison::Mismatch);
            };
            let source_receiver = substitute(self.interner, source_receiver, &map);
            match self.compare_signature_types(source_receiver, target_receiver, partial, false) {
                DemandOutcome::Ready(true) => {}
                DemandOutcome::Ready(false) => {
                    return DemandOutcome::Ready(SignatureComparison::Mismatch)
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            }
        }
        for position in 0..target_count {
            let source_param =
                match self.signature_type_at_position(&source, position, target_count) {
                    SignaturePosition::Type(ty) => ty,
                    SignaturePosition::Missing => self.interner.well_known().any,
                    SignaturePosition::Unavailable => {
                        return DemandOutcome::Ready(SignatureComparison::Unavailable)
                    }
                };
            let target_param =
                match self.signature_type_at_position(&target, position, target_count) {
                    SignaturePosition::Type(ty) => ty,
                    SignaturePosition::Missing => self.interner.well_known().any,
                    SignaturePosition::Unavailable => {
                        return DemandOutcome::Ready(SignatureComparison::Unavailable)
                    }
                };
            let source_param = substitute(self.interner, source_param, &map);
            match self.compare_signature_types(source_param, target_param, partial, true) {
                DemandOutcome::Ready(true) => {}
                DemandOutcome::Ready(false) => {
                    return DemandOutcome::Ready(SignatureComparison::Mismatch)
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            }
        }
        if !ignore_return {
            let source_ret = substitute(self.interner, source.ret, &map);
            match self.compare_signature_types(source_ret, target.ret, partial, false) {
                DemandOutcome::Ready(true) => {}
                DemandOutcome::Ready(false) => {
                    return DemandOutcome::Ready(SignatureComparison::Mismatch)
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            }
        }
        DemandOutcome::Ready(SignatureComparison::Match)
    }

    fn compare_signature_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        partial: bool,
        reverse_partial: bool,
    ) -> DemandOutcome<bool> {
        if !partial {
            return self.with_semantic_query(|query| query.is_identical(source, target));
        }
        let (source, target) = if reverse_partial {
            (target, source)
        } else {
            (source, target)
        };
        match self.with_semantic_query(|query| query.is_assignable(source, target)) {
            RelationOutcome::Yes => DemandOutcome::Ready(true),
            RelationOutcome::No(_) => DemandOutcome::Ready(false),
            RelationOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
        }
    }

    fn signature_alpha_map(
        &mut self,
        source: &FunctionType,
        target: &FunctionType,
        compare_defaults: bool,
    ) -> Option<FxHashMap<TypeParamId, TypeId>> {
        if source.type_params.len() != target.type_params.len() {
            return None;
        }
        let mut map = FxHashMap::default();
        for (source_param, target_param) in source.type_params.iter().zip(&target.type_params) {
            let target_ty = self.interner.intern_type_param(target_param.id, "T");
            map.insert(source_param.id, target_ty);
        }
        let unknown = self.interner.well_known().unknown;
        for (source_param, target_param) in source.type_params.iter().zip(&target.type_params) {
            let source_constraint = source_param.constraint.unwrap_or(unknown);
            let target_constraint = target_param.constraint.unwrap_or(unknown);
            if substitute(self.interner, source_constraint, &map) != target_constraint {
                return None;
            }
            if compare_defaults {
                let source_default = source_param.default.unwrap_or(unknown);
                let target_default = target_param.default.unwrap_or(unknown);
                if substitute(self.interner, source_default, &map) != target_default {
                    return None;
                }
            }
        }
        Some(map)
    }

    fn combine_union_signature_pair(
        &mut self,
        left_ty: TypeId,
        right_ty: TypeId,
        mode: SignatureCombinationMode,
    ) -> DemandOutcome<SignatureCombination> {
        let Some(left) = self.interner.store().function_type(left_ty).cloned() else {
            return DemandOutcome::Ready(SignatureCombination::Unavailable);
        };
        let Some(right) = self.interner.store().function_type(right_ty).cloned() else {
            return DemandOutcome::Ready(SignatureCombination::Unavailable);
        };
        let right_map = if right.type_params.is_empty() || left.type_params.is_empty() {
            FxHashMap::default()
        } else {
            let Some(map) = self.signature_alpha_map(&right, &left, false) else {
                return DemandOutcome::Ready(SignatureCombination::NoCommon);
            };
            map
        };
        match self.signatures_identical(right_ty, left_ty, false, true) {
            DemandOutcome::Ready(SignatureComparison::Match) => {
                let right_ret = substitute(self.interner, right.ret, &right_map);
                let ret = self.interner.union(vec![left.ret, right_ret]);
                return DemandOutcome::Ready(SignatureCombination::Combined(
                    self.interner.intern_function(FunctionType {
                        type_params: left.type_params,
                        receiver: left.receiver,
                        params: left.params,
                        ret,
                    }),
                ));
            }
            DemandOutcome::Ready(SignatureComparison::Mismatch) => {}
            DemandOutcome::Ready(SignatureComparison::Unavailable) => {
                return DemandOutcome::Ready(SignatureCombination::Unavailable)
            }
            DemandOutcome::Exhausted(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
        }
        if let Some(params) = self.combine_structured_rest_parameters(&left, &right, &right_map) {
            let receiver = match (left.receiver, right.receiver) {
                (Some(left), Some(right)) => {
                    let right = substitute(self.interner, right, &right_map);
                    Some(self.interner.intersection(vec![left, right]))
                }
                (Some(left), None) => Some(left),
                (None, Some(right)) => Some(substitute(self.interner, right, &right_map)),
                (None, None) => None,
            };
            let right_ret = substitute(self.interner, right.ret, &right_map);
            let ret = self.interner.union(vec![left.ret, right_ret]);
            let type_params = if left.type_params.is_empty() {
                right.type_params
            } else {
                left.type_params
            };
            return DemandOutcome::Ready(SignatureCombination::Combined(
                self.interner.intern_function(FunctionType {
                    type_params,
                    receiver,
                    params,
                    ret,
                }),
            ));
        }
        let Some(left_count) = self.effective_parameter_count(&left) else {
            return DemandOutcome::Ready(SignatureCombination::Unavailable);
        };
        let Some(right_count) = self.effective_parameter_count(&right) else {
            return DemandOutcome::Ready(SignatureCombination::Unavailable);
        };
        let (longest_is_right, longest_count) = if left_count >= right_count {
            (false, left_count)
        } else {
            (true, right_count)
        };
        let left_arity = self.call_arity(&left.params);
        let right_arity = self.call_arity(&right.params);
        let combined_min = left_arity.min.max(right_arity.min);
        // A partial match keeps its representative's bound; fallback combines both shapes.
        let left_controls_arity = left_arity.min == combined_min;
        let right_controls_arity = right_arity.min == combined_min;
        let combined_rest = match mode {
            SignatureCombinationMode::Matching => {
                (left_controls_arity && left_arity.max.is_none())
                    || (right_controls_arity && right_arity.max.is_none())
            }
            SignatureCombinationMode::Fallback => {
                left_arity.max.is_none() || right_arity.max.is_none()
            }
        };
        let combined_max = [
            left_controls_arity.then_some(left_arity.max).flatten(),
            right_controls_arity.then_some(right_arity.max).flatten(),
        ]
        .into_iter()
        .flatten()
        .max();
        let parameter_count = if combined_rest {
            longest_count
        } else {
            combined_max.unwrap_or(longest_count)
        };
        let longest_rest = if longest_is_right {
            right_arity.max.is_none()
        } else {
            left_arity.max.is_none()
        };
        let needs_extra_rest = combined_rest && (!longest_rest || parameter_count <= combined_min);
        let mut params = Vec::with_capacity(parameter_count + usize::from(needs_extra_rest));
        let unknown = self.interner.well_known().unknown;

        for position in 0..parameter_count {
            let left_param = match self.signature_type_at_position(&left, position, parameter_count)
            {
                SignaturePosition::Type(ty) => ty,
                SignaturePosition::Missing => unknown,
                SignaturePosition::Unavailable => {
                    return DemandOutcome::Ready(SignatureCombination::Unavailable)
                }
            };
            let right_param =
                match self.signature_type_at_position(&right, position, parameter_count) {
                    SignaturePosition::Type(ty) => substitute(self.interner, ty, &right_map),
                    SignaturePosition::Missing => unknown,
                    SignaturePosition::Unavailable => {
                        return DemandOutcome::Ready(SignatureCombination::Unavailable)
                    }
                };
            let ty = self.interner.intersection(vec![left_param, right_param]);
            let name = self.combined_parameter_name(&left, &right, position);
            let is_rest = combined_rest && !needs_extra_rest && position + 1 == parameter_count;
            let optional = position >= left_arity.min && position >= right_arity.min;
            params.push(if is_rest {
                ParameterType::rest(name, self.interner.intern_array(ty))
            } else if optional {
                ParameterType::optional(name, ty)
            } else {
                ParameterType::required(name, ty)
            });
        }
        if needs_extra_rest {
            let (rest_source, rest_is_right) = if left_arity.max.is_none() {
                (&left, false)
            } else if right_arity.max.is_none() {
                (&right, true)
            } else {
                return DemandOutcome::Ready(SignatureCombination::NoCommon);
            };
            let mut ty = match self.signature_type_at_position(
                rest_source,
                parameter_count,
                parameter_count + 1,
            ) {
                SignaturePosition::Type(ty) => ty,
                SignaturePosition::Missing => {
                    return DemandOutcome::Ready(SignatureCombination::NoCommon)
                }
                SignaturePosition::Unavailable => {
                    return DemandOutcome::Ready(SignatureCombination::Unavailable)
                }
            };
            if rest_is_right {
                ty = substitute(self.interner, ty, &right_map);
            }
            params.push(ParameterType::rest("args", self.interner.intern_array(ty)));
        }

        let receiver = match (left.receiver, right.receiver) {
            (Some(left), Some(right)) => {
                let right = substitute(self.interner, right, &right_map);
                Some(self.interner.intersection(vec![left, right]))
            }
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(substitute(self.interner, right, &right_map)),
            (None, None) => None,
        };
        let right_ret = substitute(self.interner, right.ret, &right_map);
        let ret = self.interner.union(vec![left.ret, right_ret]);
        let type_params = if left.type_params.is_empty() {
            right.type_params
        } else {
            left.type_params
        };
        DemandOutcome::Ready(SignatureCombination::Combined(
            self.interner.intern_function(FunctionType {
                type_params,
                receiver,
                params,
                ret,
            }),
        ))
    }

    /// Keep a tuple rest's required prefix and suffix around its variadic middle.
    fn combine_structured_rest_parameters(
        &mut self,
        left: &FunctionType,
        right: &FunctionType,
        right_map: &FxHashMap<TypeParamId, TypeId>,
    ) -> Option<Vec<ParameterType>> {
        let left_fixed: Vec<&ParameterType> = left.fixed_params().collect();
        let right_fixed: Vec<&ParameterType> = right.fixed_params().collect();
        if left_fixed.len() != right_fixed.len() {
            return None;
        }
        let left_rest = left.rest_param()?;
        let right_rest = right.rest_param()?;
        let left_shapes = self.rest_call_shapes(left_rest.ty)?;
        let right_shapes = self.rest_call_shapes(right_rest.ty)?;
        let [left_shape] = left_shapes.as_slice() else {
            return None;
        };
        let [right_shape] = right_shapes.as_slice() else {
            return None;
        };
        let (Some(left_middle), Some(right_middle)) = (left_shape.variadic, right_shape.variadic)
        else {
            return None;
        };
        if left_shape.prefix.len() != right_shape.prefix.len()
            || left_shape.suffix.len() != right_shape.suffix.len()
        {
            return None;
        }

        let mut params = Vec::with_capacity(left_fixed.len() + 1);
        for (position, (left_param, right_param)) in left_fixed.iter().zip(right_fixed).enumerate()
        {
            let right_ty = substitute(self.interner, right_param.ty, right_map);
            let ty = self.interner.intersection(vec![left_param.ty, right_ty]);
            let name = if left_param.name == right_param.name {
                left_param.name.clone()
            } else {
                format!("arg{position}")
            };
            params.push(if left_param.optional && right_param.optional {
                ParameterType::optional(name, ty)
            } else {
                ParameterType::required(name, ty)
            });
        }

        let mut elements = Vec::with_capacity(left_shape.prefix.len() + left_shape.suffix.len());
        for (&left_ty, &right_ty) in left_shape.prefix.iter().zip(&right_shape.prefix) {
            let right_ty = substitute(self.interner, right_ty, right_map);
            elements.push(self.interner.intersection(vec![left_ty, right_ty]));
        }
        let rest_position = elements.len();
        for (&left_ty, &right_ty) in left_shape.suffix.iter().zip(&right_shape.suffix) {
            let right_ty = substitute(self.interner, right_ty, right_map);
            elements.push(self.interner.intersection(vec![left_ty, right_ty]));
        }
        let right_middle = substitute(self.interner, right_middle, right_map);
        let middle = self.interner.intersection(vec![left_middle, right_middle]);
        let middle = self.interner.intern_array(middle);
        let tuple = self.interner.intern_tuple_type(TupleType::with_rest(
            elements,
            TupleRestType::new(rest_position, middle),
        ));
        let name = if left_rest.name == right_rest.name {
            left_rest.name.clone()
        } else {
            "args".to_string()
        };
        params.push(ParameterType::rest(name, tuple));
        Some(params)
    }

    fn effective_parameter_count(&self, function: &FunctionType) -> Option<usize> {
        let fixed = function.total_fixed_param_count();
        let Some(rest) = function.rest_param() else {
            return Some(fixed);
        };
        let shapes = self.rest_call_shapes(rest.ty)?;
        if shapes.len() > 1 {
            return Some(fixed + 1);
        }
        let shape = shapes.first()?;
        Some(
            fixed + shape.prefix.len() + shape.suffix.len() + usize::from(shape.variadic.is_some()),
        )
    }

    fn signature_type_at_position(
        &mut self,
        function: &FunctionType,
        position: usize,
        total_count: usize,
    ) -> SignaturePosition {
        let fixed: Vec<&ParameterType> = function.fixed_params().collect();
        if let Some(parameter) = fixed.get(position) {
            return SignaturePosition::Type(parameter.ty);
        }
        let Some(rest) = function.rest_param() else {
            return SignaturePosition::Missing;
        };
        let Some(shapes) = self.rest_call_shapes(rest.ty) else {
            return SignaturePosition::Unavailable;
        };
        let Some(offset) = position.checked_sub(fixed.len()) else {
            return SignaturePosition::Missing;
        };
        let rest_count = total_count.saturating_sub(fixed.len());
        let mut types = Vec::new();
        for shape in shapes {
            if let Some(ty) = shape.element_at(offset, rest_count) {
                types.push(ty);
            }
        }
        if types.is_empty() {
            SignaturePosition::Missing
        } else {
            SignaturePosition::Type(self.interner.union(types))
        }
    }

    fn combined_parameter_name(
        &self,
        left: &FunctionType,
        right: &FunctionType,
        position: usize,
    ) -> String {
        let left = self.signature_parameter_name(left, position);
        let right = self.signature_parameter_name(right, position);
        match (left, right) {
            (Some(left), Some(right)) if left == right => left.to_string(),
            (Some(left), None) => left.to_string(),
            (None, Some(right)) => right.to_string(),
            _ => format!("arg{position}"),
        }
    }

    fn signature_parameter_name<'signature>(
        &self,
        function: &'signature FunctionType,
        position: usize,
    ) -> Option<&'signature str> {
        let fixed: Vec<&ParameterType> = function.fixed_params().collect();
        fixed
            .get(position)
            .map(|parameter| parameter.name.as_str())
            .or_else(|| {
                function
                    .rest_param()
                    .map(|parameter| parameter.name.as_str())
            })
    }

    /// Keep TK2349 on types whose represented shape is conclusive. Deferred and
    /// structural types may have dropped unsupported signatures, so they stay silent.
    fn provably_non_callable(&self, callee_ty: TypeId) -> bool {
        match self.interner.store().tag(callee_ty) {
            TypeTag::Literal => true,
            TypeTag::Intrinsic => matches!(
                self.interner.store().intrinsic_kind(callee_ty),
                Some(
                    IntrinsicKind::Boolean
                        | IntrinsicKind::Number
                        | IntrinsicKind::String
                        | IntrinsicKind::Null
                        | IntrinsicKind::Undefined
                        | IntrinsicKind::Void
                        | IntrinsicKind::Object
                        | IntrinsicKind::BigInt
                        | IntrinsicKind::Symbol
                )
            ),
            _ => false,
        }
    }

    fn select_call_candidate(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
        signatures: &[TypeId],
        args: PreparedCallArgs<'_, '_>,
        call_span: Span,
        receiver: Option<TypeId>,
    ) -> DemandOutcome<Option<CallCandidate>> {
        self.select_candidate(CandidateSelectionRequest {
            scope,
            signatures,
            type_arguments: call.type_arguments.as_deref(),
            args,
            span: call_span,
            receiver: CandidateReceiver::Call(receiver),
        })
    }

    /// Ordered overload selection shared by call and construct wrappers.
    fn select_candidate(
        &mut self,
        request: CandidateSelectionRequest<'_, '_, '_, '_>,
    ) -> DemandOutcome<Option<CallCandidate>> {
        let CandidateSelectionRequest {
            scope,
            signatures,
            type_arguments,
            args,
            span,
            receiver,
        } = request;
        let overload = signatures.len() > 1;
        if !overload {
            let Some(signature) = signatures.first().copied() else {
                return DemandOutcome::Ready(None);
            };
            return match self.instantiate_signature_candidate(SignatureCandidateRequest {
                scope,
                signature_ty: signature,
                type_arguments,
                args,
                call_receiver: receiver.inference_source(),
                commit_constraints: true,
                reject_inferred_constraint_violations: false,
            }) {
                Ok(candidate) => DemandOutcome::Ready(Some(candidate)),
                Err(CandidateBuildFailure::Constraint)
                | Err(CandidateBuildFailure::InferredConstraint)
                | Err(CandidateBuildFailure::Unavailable) => DemandOutcome::Ready(None),
                Err(CandidateBuildFailure::Exhausted(exhaustion)) => {
                    DemandOutcome::Exhausted(exhaustion)
                }
            };
        }

        let mut arity_failures: Vec<CallArity> = Vec::new();
        let mut saw_non_arity_failure = false;
        let mut first_constraint_failure: Option<CheckerEffects<Ticket>> = None;
        let mut first_other_failure: Option<CheckerEffects<Ticket>> = None;

        for signature in signatures {
            let (built, effects) = self.capture_speculative_candidate_effects(|pass| {
                pass.instantiate_signature_candidate(SignatureCandidateRequest {
                    scope,
                    signature_ty: *signature,
                    type_arguments,
                    args,
                    call_receiver: receiver.inference_source(),
                    commit_constraints: false,
                    reject_inferred_constraint_violations: true,
                })
            });
            let candidate = match built {
                Ok(candidate) => {
                    effects.records.discard();
                    if let Some(exhaustion) = candidate.inference_exhaustion.clone() {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                    candidate
                }
                Err(CandidateBuildFailure::Constraint) => {
                    #[cfg(test)]
                    measure_call(|measure| {
                        measure.speculative_diagnostics_removed +=
                            u64::try_from(effects.records.len()).expect("effect count fits u64");
                    });
                    if first_constraint_failure.is_none() {
                        first_constraint_failure = Some(effects);
                    }
                    saw_non_arity_failure = true;
                    continue;
                }
                Err(CandidateBuildFailure::Unavailable) => {
                    if first_other_failure.is_none() {
                        first_other_failure = Some(effects);
                    }
                    saw_non_arity_failure = true;
                    continue;
                }
                Err(CandidateBuildFailure::InferredConstraint) => {
                    effects.records.discard();
                    saw_non_arity_failure = true;
                    continue;
                }
                Err(CandidateBuildFailure::Exhausted(exhaustion)) => {
                    effects.records.discard();
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
            let trial = self.with_speculative_candidate_queries(|pass| {
                pass.try_call_candidate(
                    scope,
                    &candidate.params,
                    receiver.trial_receivers(&candidate),
                    args.types,
                    args.exprs,
                    span,
                )
            });
            match trial {
                CandidateTrial::Match => {
                    let (committed, effects) = self.capture_candidate_effects(|pass| {
                        pass.instantiate_signature_candidate(SignatureCandidateRequest {
                            scope,
                            signature_ty: *signature,
                            type_arguments,
                            args,
                            call_receiver: receiver.inference_source(),
                            commit_constraints: true,
                            reject_inferred_constraint_violations: true,
                        })
                    });
                    return match committed {
                        Ok(candidate) => {
                            if let Some(exhaustion) = candidate.inference_exhaustion.clone() {
                                effects.records.discard();
                                return DemandOutcome::Exhausted(exhaustion);
                            }
                            self.merge_candidate_effects(effects);
                            DemandOutcome::Ready(Some(candidate))
                        }
                        Err(CandidateBuildFailure::Constraint)
                        | Err(CandidateBuildFailure::Unavailable) => {
                            self.merge_candidate_effects(effects);
                            DemandOutcome::Ready(None)
                        }
                        Err(CandidateBuildFailure::InferredConstraint) => {
                            effects.records.discard();
                            DemandOutcome::Ready(None)
                        }
                        Err(CandidateBuildFailure::Exhausted(exhaustion)) => {
                            effects.records.discard();
                            DemandOutcome::Exhausted(exhaustion)
                        }
                    };
                }
                CandidateTrial::Arity(arity) => arity_failures.push(arity),
                CandidateTrial::Mismatch => saw_non_arity_failure = true,
                CandidateTrial::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            }
        }

        if let Some(effects) = first_constraint_failure {
            self.merge_candidate_effects(effects);
        } else if !arity_failures.is_empty() && !saw_non_arity_failure {
            self.emit_overload_arity_failure(&arity_failures, args.types.len(), span);
        } else {
            if let Some(effects) = first_other_failure {
                self.merge_candidate_effects(effects);
            }
            self.emit_diagnostic(Diagnostic::no_overload_matches(span));
        }
        DemandOutcome::Ready(None)
    }

    /// Build one callable or constructable signature candidate from its persistent
    /// generic descriptors. Both `f<T>(...)` and `new C<T>(...)` share this path so
    /// outer-substituted constraints/defaults cannot fall back to stale store state.
    fn instantiate_signature_candidate(
        &mut self,
        request: SignatureCandidateRequest<'_, '_>,
    ) -> Result<CallCandidate, CandidateBuildFailure> {
        let SignatureCandidateRequest {
            scope,
            signature_ty,
            type_arguments,
            args,
            call_receiver,
            commit_constraints,
            reject_inferred_constraint_violations,
        } = request;
        #[cfg(not(test))]
        let _ = commit_constraints;
        #[cfg(test)]
        measure_call(|measure| {
            if commit_constraints {
                measure.committed_candidate_builds += 1;
            } else {
                measure.speculative_candidate_builds += 1;
            }
        });
        let generic_params = self
            .interner
            .store()
            .function_type(signature_ty)
            .map(|function| function.type_params.clone())
            .ok_or(CandidateBuildFailure::Unavailable)?;
        let mut inference_exhaustion = None;
        let instantiated = if generic_params.is_empty() {
            signature_ty
        } else {
            let map = match type_arguments {
                Some(type_arguments) => {
                    let mut arg_infos: Vec<(TypeId, Span)> =
                        Vec::with_capacity(type_arguments.params.len());
                    for arg in &type_arguments.params {
                        arg_infos.push((
                            self.lower_annotation(scope, arg)
                                .ok_or(CandidateBuildFailure::Unavailable)?,
                            Span::from_oxc(arg.span()),
                        ));
                    }
                    let min = generic_params
                        .iter()
                        .filter(|param| param.default.is_none())
                        .count();
                    let max = generic_params.len();
                    if arg_infos.len() < min || arg_infos.len() > max {
                        let diagnostic = Diagnostic::wrong_type_argument_count(
                            Span::from_oxc(type_arguments.span),
                            min,
                            max,
                            arg_infos.len(),
                        );
                        return Err(self.candidate_constraint_failure(diagnostic));
                    }
                    let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
                    for (param, &(arg, _)) in generic_params.iter().zip(&arg_infos) {
                        map.insert(param.id, arg);
                    }
                    let map = self.complete_signature_type_arguments(&generic_params, map);
                    match self.check_signature_type_argument_constraints(
                        &generic_params,
                        &arg_infos,
                        &map,
                    ) {
                        DemandOutcome::Ready(false) => {}
                        DemandOutcome::Ready(true) => {
                            #[cfg(test)]
                            if !commit_constraints {
                                measure_call(|measure| {
                                    measure.speculative_diagnostic_rollback_events += 1;
                                });
                            }
                            return Err(CandidateBuildFailure::Constraint);
                        }
                        DemandOutcome::Exhausted(exhaustion) => {
                            return Err(CandidateBuildFailure::Exhausted(exhaustion))
                        }
                    }
                    map
                }
                None => {
                    let params: Vec<ParameterType> = self
                        .interner
                        .store()
                        .function_type(signature_ty)
                        .ok_or(CandidateBuildFailure::Unavailable)?
                        .params
                        .clone();
                    // Fix only source arguments that precede a direct contextual callback.
                    // This gives `on("event", listener)` its event-name `K` before
                    // contextualizing `listener`, without perturbing ordinary fresh-literal
                    // inference (which deliberately needs the full candidate policy).
                    let contextual_start = args
                        .exprs
                        .iter()
                        .position(|expr| matches!(expr, Expression::ArrowFunctionExpression(_)));
                    let contextual_params = if let Some(start) = contextual_start {
                        #[cfg(test)]
                        measure_call(|measure| measure.generic_preliminary_inference_runs += 1);
                        let raw_args: Vec<TypeId> = args.types[..start]
                            .iter()
                            .map(|(argument, _)| *argument)
                            .collect();
                        let preliminary = infer::infer_signature_type_arguments_from_params(
                            self.interner,
                            &mut self.next_type_param,
                            self.type_environment.published().classes(),
                            &mut self.semantic_queries,
                            infer::SignatureInferenceRequest {
                                type_params: &generic_params,
                                params: &params,
                                args: &raw_args,
                                fresh_args: &args.fresh[..start],
                                receiver: call_receiver.zip(
                                    self.interner
                                        .store()
                                        .function_type(signature_ty)
                                        .and_then(|function| function.receiver),
                                ),
                            },
                        );
                        let preliminary = match preliminary {
                            DemandOutcome::Ready(result) => {
                                inference_exhaustion = result.exhaustion;
                                result.arguments
                            }
                            DemandOutcome::Exhausted(exhaustion) => {
                                return Err(CandidateBuildFailure::Exhausted(exhaustion));
                            }
                        };
                        let unknown = self.interner.well_known().unknown;
                        let contextual_map: FxHashMap<TypeParamId, TypeId> = preliminary
                            .into_iter()
                            .filter(|(_, ty)| *ty != unknown)
                            .collect();
                        if contextual_map.is_empty() {
                            params.clone()
                        } else {
                            let contextual_signature =
                                instantiate_function(self.interner, signature_ty, &contextual_map);
                            self.interner
                                .store()
                                .function_type(contextual_signature)
                                .ok_or(CandidateBuildFailure::Unavailable)?
                                .params
                                .clone()
                        }
                    } else {
                        params.clone()
                    };
                    let contextual_params = match self.evaluate_parameters(contextual_params) {
                        DemandOutcome::Ready(params) => params,
                        DemandOutcome::Exhausted(exhaustion) => {
                            return Err(CandidateBuildFailure::Exhausted(exhaustion))
                        }
                    };
                    let inference_args = contextual_inference_args!(
                        self,
                        scope,
                        &contextual_params,
                        args.types,
                        args.exprs,
                        ContextualMeasurePhase::CandidateInference
                    );
                    #[cfg(test)]
                    measure_call(|measure| measure.generic_full_inference_runs += 1);
                    match infer::infer_signature_type_arguments_from_params(
                        self.interner,
                        &mut self.next_type_param,
                        self.type_environment.published().classes(),
                        &mut self.semantic_queries,
                        infer::SignatureInferenceRequest {
                            type_params: &generic_params,
                            params: &params,
                            args: &inference_args,
                            fresh_args: args.fresh,
                            receiver: call_receiver.zip(
                                self.interner
                                    .store()
                                    .function_type(signature_ty)
                                    .and_then(|function| function.receiver),
                            ),
                        },
                    ) {
                        DemandOutcome::Ready(result) => {
                            if inference_exhaustion.is_none() {
                                inference_exhaustion = result.exhaustion;
                            }
                            if reject_inferred_constraint_violations
                                && !result.constraint_violations.is_empty()
                            {
                                return Err(CandidateBuildFailure::InferredConstraint);
                            }
                            result.arguments
                        }
                        DemandOutcome::Exhausted(exhaustion) => {
                            return Err(CandidateBuildFailure::Exhausted(exhaustion));
                        }
                    }
                }
            };
            instantiate_function(self.interner, signature_ty, &map)
        };

        let func = self
            .interner
            .store()
            .function_type(instantiated)
            .ok_or(CandidateBuildFailure::Unavailable)?;
        let params = func.params.clone();
        let ret = func.ret;
        let receiver = func.receiver;
        let params = match self.evaluate_parameters(params) {
            DemandOutcome::Ready(params) => params,
            DemandOutcome::Exhausted(exhaustion) => {
                return Err(CandidateBuildFailure::Exhausted(exhaustion))
            }
        };
        let receiver = match receiver {
            Some(receiver) => match self.evaluate_call_boundary_type(receiver) {
                DemandOutcome::Ready(receiver) => Some(receiver),
                DemandOutcome::Exhausted(exhaustion) => {
                    return Err(CandidateBuildFailure::Exhausted(exhaustion))
                }
            },
            None => None,
        };
        Ok(CallCandidate {
            receiver,
            params,
            ret,
            inference_exhaustion,
        })
    }

    /// Fill omitted function binders in declaration order. Explicit arguments are
    /// retained verbatim (so their constraint error is reported as `TK2344`), while
    /// defaults observe the already-completed earlier bindings.
    fn complete_signature_type_arguments(
        &mut self,
        type_params: &[GenericTypeParam],
        mut map: FxHashMap<TypeParamId, TypeId>,
    ) -> FxHashMap<TypeParamId, TypeId> {
        let unknown = self.interner.well_known().unknown;
        for type_param in type_params {
            if map.contains_key(&type_param.id) {
                continue;
            }
            let value = type_param
                .default
                .map(|default| substitute(self.interner, default, &map))
                .or_else(|| {
                    type_param
                        .constraint
                        .map(|constraint| substitute(self.interner, constraint, &map))
                })
                .unwrap_or(unknown);
            map.insert(type_param.id, value);
        }
        map
    }

    fn candidate_constraint_failure(&mut self, diagnostic: Diagnostic) -> CandidateBuildFailure {
        self.emit_diagnostic(diagnostic);
        CandidateBuildFailure::Constraint
    }

    fn try_call_candidate(
        &mut self,
        scope: ScopeId,
        params: &[ParameterType],
        receivers: (Option<TypeId>, Option<TypeId>),
        arg_types: &[(TypeId, Span)],
        arg_exprs: &[&Expression<'_>],
        _call_span: Span,
    ) -> CandidateTrial {
        #[cfg(test)]
        measure_call(|measure| measure.candidate_trials += 1);
        match self.call_receiver_compatibility(receivers.0, receivers.1) {
            RelationOutcome::Yes => {}
            RelationOutcome::No(_) => {
                #[cfg(test)]
                measure_call(|measure| measure.candidate_mismatches += 1);
                return CandidateTrial::Mismatch;
            }
            RelationOutcome::Exhausted(exhaustion) => return CandidateTrial::Exhausted(exhaustion),
        }
        let arity = self.call_arity(params);
        if !self.call_arity_accepts(&arity, arg_types.len()) {
            #[cfg(test)]
            measure_call(|measure| measure.candidate_arity_failures += 1);
            return CandidateTrial::Arity(arity);
        }

        let alternatives = self.call_argument_target_alternatives(params, arg_types.len());
        if alternatives.is_empty() {
            #[cfg(test)]
            measure_call(|measure| measure.candidate_mismatches += 1);
            return CandidateTrial::Mismatch;
        }
        if alternatives.len() > 1 {
            #[cfg(test)]
            let compatibility =
                with_contextual_measure_phase(ContextualMeasurePhase::CandidateTrial, || {
                    self.compatible_call_argument_targets(params, arg_types, arg_exprs, scope)
                });
            #[cfg(not(test))]
            let compatibility =
                self.compatible_call_argument_targets(params, arg_types, arg_exprs, scope);
            return match compatibility {
                DemandOutcome::Ready(Some(_)) => {
                    #[cfg(test)]
                    measure_call(|measure| measure.candidate_matches += 1);
                    CandidateTrial::Match
                }
                DemandOutcome::Ready(None) => {
                    #[cfg(test)]
                    measure_call(|measure| measure.candidate_mismatches += 1);
                    CandidateTrial::Mismatch
                }
                DemandOutcome::Exhausted(exhaustion) => CandidateTrial::Exhausted(exhaustion),
            };
        }

        let targets = alternatives
            .into_iter()
            .next()
            .expect("one alternative has a first element");
        for (((arg_ty, arg_span), arg_expr), param_ty) in
            arg_types.iter().zip(arg_exprs).zip(targets)
        {
            let Some(param_ty) = param_ty else {
                continue;
            };
            let (src, _src_span) = contextual_source_after_walked!(
                self,
                scope,
                arg_expr,
                param_ty,
                (*arg_ty, *arg_span),
                true,
                false,
                ContextualMeasurePhase::CandidateTrial
            );
            let diagnostics = match self.check_excess_properties_for_target(arg_expr, param_ty) {
                DemandOutcome::Ready(diagnostics) => diagnostics,
                DemandOutcome::Exhausted(exhaustion) => {
                    return CandidateTrial::Exhausted(exhaustion);
                }
            };
            if !diagnostics.is_empty() {
                #[cfg(test)]
                measure_call(|measure| {
                    measure.speculative_diagnostic_rollback_events += 1;
                    measure.speculative_diagnostics_removed +=
                        u64::try_from(diagnostics.len()).expect("diagnostic count fits u64");
                });
                return CandidateTrial::Mismatch;
            }
            match self.with_semantic_query(|query| query.is_assignable(src, param_ty)) {
                RelationOutcome::Yes => {}
                RelationOutcome::No(_) => {
                    #[cfg(test)]
                    measure_call(|measure| measure.candidate_mismatches += 1);
                    return CandidateTrial::Mismatch;
                }
                RelationOutcome::Exhausted(exhaustion) => {
                    return CandidateTrial::Exhausted(exhaustion)
                }
            }
        }
        #[cfg(test)]
        measure_call(|measure| measure.candidate_matches += 1);
        CandidateTrial::Match
    }

    fn call_arity_accepts(&self, arity: &CallArity, got: usize) -> bool {
        if got < arity.min {
            return false;
        }
        arity.max.is_none_or(|max| got <= max)
    }

    fn emit_overload_arity_failure(&mut self, arities: &[CallArity], got: usize, span: Span) {
        let min = arities.iter().map(|arity| arity.min).min().unwrap_or(0);
        let max = if arities.iter().any(|arity| arity.max.is_none()) {
            None
        } else {
            arities.iter().filter_map(|arity| arity.max).max()
        };
        let unbounded_rest = arities.iter().any(|arity| arity.unbounded_rest);
        let diagnostic = if got < min && unbounded_rest {
            Diagnostic::wrong_min_argument_count(span, min, got)
        } else {
            self.wrong_bounded_argument_count(span, min, max.unwrap_or(min), got)
        };
        self.emit_diagnostic(diagnostic);
    }

    /// Check `super(args)` against the base constructor with the shared call
    /// machine. Arguments are always walked; missing base signatures collect no
    /// obligation and emit no `super`-specific diagnostic.
    fn infer_super_call(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
        call_span: Span,
    ) -> Option<(TypeId, Span)> {
        self.with_provisional_argument_effects(
            |pass| pass.infer_super_call_inner(scope, call, call_span),
            |pass| pass.rewalk_memoized_raw_arguments(scope, &call.arguments),
        )
    }

    fn infer_super_call_inner(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
        call_span: Span,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();

        // Infer every argument up front (skipping spreads — out of subset); this descends
        // into nested calls/`new`/functions inside the arguments.
        let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(call.arguments.len());
        let mut arg_exprs: Vec<&Expression<'_>> = Vec::with_capacity(call.arguments.len());
        for (argument_index, arg) in call.arguments.iter().enumerate() {
            if let Some(arg_expr) = arg.as_expression() {
                if let Some(inferred) = self.walk_raw_argument(scope, argument_index, arg_expr) {
                    arg_types.push(inferred);
                    arg_exprs.push(arg_expr);
                }
            } else {
                self.record_spread_argument_skip(arg);
            }
        }

        // The base constructor signature in scope. Absent → no obligation, no diagnostic.
        let Some(super_ctor) = self.current_super_ctor else {
            return Some((wk.error, call_span));
        };
        let params: Vec<ParameterType> = match self.interner.store().function_type(super_ctor) {
            Some(func) => func.params.clone(),
            // Defensive: the constructor is always interned as a function in `fill_class`.
            None => return Some((wk.error, call_span)),
        };
        let params = match self.evaluate_parameters(params) {
            DemandOutcome::Ready(params) => params,
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), call_span);
                return None;
            }
        };

        // Reuse the shared call-checking path: arity (TK2554) + argument assignability
        // (TK2345). The `super(...)` expression's value type is unused.
        self.check_call_arguments(scope, &params, &arg_types, &arg_exprs, call_span);

        Some((wk.error, call_span))
    }

    /// Shared M3 call/`new` argument checking: arity plus per-argument
    /// assignability. Fresh object/tuple literals use assignment-style diagnostics,
    /// matching tsc's literal-member reporting.
    fn check_call_arguments(
        &mut self,
        scope: ScopeId,
        params: &[ParameterType],
        arg_types: &[(TypeId, Span)],
        arg_exprs: &[&Expression<'_>],
        call_span: Span,
    ) {
        self.check_call_arity(params, arg_types.len(), call_span);

        let never = self.interner.well_known().never;
        let alternatives = self.call_argument_target_alternatives(params, arg_types.len());
        let union_rest_mismatch = alternatives.is_empty();
        let targets = if union_rest_mismatch {
            let fixed: Vec<&ParameterType> = params
                .iter()
                .filter(|parameter| parameter.is_fixed())
                .collect();
            (0..arg_types.len())
                .map(|index| fixed.get(index).map(|parameter| parameter.ty))
                .collect()
        } else if alternatives.len() == 1 {
            alternatives
                .into_iter()
                .next()
                .expect("one alternative has a first element")
        } else {
            #[cfg(test)]
            let compatibility =
                with_contextual_measure_phase(ContextualMeasurePhase::CommittedCheck, || {
                    self.compatible_call_argument_targets(params, arg_types, arg_exprs, scope)
                });
            #[cfg(not(test))]
            let compatibility =
                self.compatible_call_argument_targets(params, arg_types, arg_exprs, scope);
            match compatibility {
                DemandOutcome::Ready(Some(targets)) => targets,
                DemandOutcome::Ready(None) => alternatives
                    .into_iter()
                    .next()
                    .expect("multiple alternatives have a first element"),
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), call_span);
                    return;
                }
            }
        };
        for (index, (((arg_ty, arg_span), arg_expr), param_ty)) in
            arg_types.iter().zip(arg_exprs).zip(targets).enumerate()
        {
            let Some(param_ty) = param_ty else {
                continue;
            };
            let ((src, src_span), rewalk) = contextual_source_after_walked_reporting!(
                self,
                scope,
                arg_expr,
                param_ty,
                (*arg_ty, *arg_span),
                true,
                true,
                ContextualMeasurePhase::CommittedCheck
            );
            // This walk saw the argument with its instantiated contextual target, so
            // it is the walk that reports; the raw walk's copy never commits.
            if rewalk == ContextualRewalk::Rewalked {
                self.supersede_provisional_argument_effects(index);
            }
            match self.check_excess_properties_for_target(arg_expr, param_ty) {
                DemandOutcome::Ready(diagnostics) => {
                    for diagnostic in diagnostics {
                        self.emit_diagnostic(diagnostic);
                    }
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), *arg_span);
                    return;
                }
            }
            self.schedule_obligation(AssignObligation {
                src,
                tgt: param_ty,
                src_span,
                source_member_spans: Vec::new(),
                kind: self.call_argument_obligation_kind(arg_expr, param_ty),
            });
            // A `never` parameter makes this call candidate impossible. Later
            // contextual callback targets are recovery artifacts of that failure,
            // not independent argument errors.
            if param_ty == never {
                break;
            }
        }
        if union_rest_mismatch {
            self.schedule_union_rest_tuple_obligation(params, arg_types, call_span);
        }
    }

    fn schedule_union_rest_tuple_obligation(
        &mut self,
        params: &[ParameterType],
        arg_types: &[(TypeId, Span)],
        call_span: Span,
    ) {
        let fixed_count = params
            .iter()
            .filter(|parameter| parameter.is_fixed())
            .count();
        let Some(rest) = params.iter().find(|parameter| parameter.rest) else {
            return;
        };
        let rest_types: Vec<TypeId> = arg_types
            .iter()
            .skip(fixed_count)
            .map(|(ty, _)| *ty)
            .collect();
        if rest_types.len() < self.rest_parameter_arity(rest.ty).min {
            return;
        }
        let span = arg_types
            .get(fixed_count)
            .map(|(_, span)| *span)
            .unwrap_or(call_span);
        let src = self.interner.intern_tuple(rest_types);
        self.schedule_obligation(AssignObligation {
            src,
            tgt: rest.ty,
            src_span: span,
            source_member_spans: Vec::new(),
            kind: ObligationKind::Argument,
        });
    }

    fn compatible_call_argument_targets(
        &mut self,
        params: &[ParameterType],
        arg_types: &[(TypeId, Span)],
        arg_exprs: &[&Expression<'_>],
        scope: ScopeId,
    ) -> DemandOutcome<Option<Vec<Option<TypeId>>>> {
        let alternatives = self.call_argument_target_alternatives(params, arg_types.len());
        for targets in alternatives {
            let mut compatible = true;
            for (((arg_ty, arg_span), arg_expr), param_ty) in
                arg_types.iter().zip(arg_exprs).zip(&targets)
            {
                let Some(param_ty) = *param_ty else {
                    continue;
                };
                let (src, _) = self.infer_contextual_source_after_walked(
                    scope,
                    arg_expr,
                    param_ty,
                    (*arg_ty, *arg_span),
                    true,
                    false,
                );
                let diagnostics = match self.check_excess_properties_for_target(arg_expr, param_ty)
                {
                    DemandOutcome::Ready(diagnostics) => diagnostics,
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                };
                if !diagnostics.is_empty() {
                    compatible = false;
                    break;
                }
                match self.with_semantic_query(|query| query.is_assignable(src, param_ty)) {
                    RelationOutcome::Yes => {}
                    RelationOutcome::No(_) => {
                        compatible = false;
                        break;
                    }
                    RelationOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                }
            }
            if compatible {
                return DemandOutcome::Ready(Some(targets));
            }
        }
        DemandOutcome::Ready(None)
    }

    /// Check an explicit non-positional receiver after overload selection. Bare
    /// calls have `undefined` as their receiver; member calls provide their object.
    fn check_call_receiver(
        &mut self,
        target_receiver: Option<TypeId>,
        call_receiver: Option<(TypeId, Span)>,
        call_span: Span,
    ) -> DemandOutcome<()> {
        let Some(target_receiver) = target_receiver else {
            return DemandOutcome::Ready(());
        };
        #[cfg(test)]
        measure_call(|measure| measure.selected_receiver_relation_queries += 1);
        let (source_receiver, span) =
            call_receiver.unwrap_or((self.interner.well_known().undefined, call_span));
        match self
            .with_semantic_query(|query| query.is_assignable(source_receiver, target_receiver))
        {
            RelationOutcome::Yes => DemandOutcome::Ready(()),
            RelationOutcome::No(_) => {
                let store = self.interner.store();
                self.emit_diagnostic(Diagnostic::this_context_not_assignable(
                    span,
                    &render_type(store, source_receiver, false),
                    &render_type(store, target_receiver, false),
                ));
                DemandOutcome::Ready(())
            }
            RelationOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
        }
    }

    fn call_receiver_compatibility(
        &mut self,
        target_receiver: Option<TypeId>,
        call_receiver: Option<TypeId>,
    ) -> RelationOutcome {
        let Some(target_receiver) = target_receiver else {
            return RelationOutcome::Yes;
        };
        #[cfg(test)]
        measure_call(|measure| measure.trial_receiver_relation_queries += 1);
        let source_receiver = call_receiver.unwrap_or(self.interner.well_known().undefined);
        self.with_semantic_query(|query| query.is_assignable(source_receiver, target_receiver))
    }

    fn evaluate_parameters(
        &mut self,
        params: Vec<ParameterType>,
    ) -> DemandOutcome<Vec<ParameterType>> {
        let mut evaluated = Vec::with_capacity(params.len());
        for mut parameter in params {
            parameter.ty = match self.evaluate_call_boundary_type(parameter.ty) {
                DemandOutcome::Ready(ty) => ty,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            };
            evaluated.push(parameter);
        }
        DemandOutcome::Ready(evaluated)
    }

    fn evaluate_call_boundary_type(&mut self, ty: TypeId) -> DemandOutcome<TypeId> {
        if self.interner.store().tag(ty) == TypeTag::ClassInstance {
            return self.with_semantic_query(|query| query.normalize_class_application(ty));
        }
        if let Some(mut function) = self.interner.store().function_type(ty).cloned() {
            for parameter in &mut function.params {
                parameter.ty = match self.evaluate_type(parameter.ty) {
                    DemandOutcome::Ready(ty) => ty,
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                };
            }
            function.receiver = match function.receiver {
                Some(receiver) => match self.evaluate_type(receiver) {
                    DemandOutcome::Ready(receiver) => Some(receiver),
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion)
                    }
                },
                None => None,
            };
            function.ret = match self.evaluate_type(function.ret) {
                DemandOutcome::Ready(ret) => ret,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            };
            return DemandOutcome::Ready(self.interner.intern_function(function));
        }
        self.evaluate_type(ty)
    }

    fn check_call_arity(&mut self, params: &[ParameterType], got: usize, span: Span) {
        let arity = self.call_arity(params);
        if got < arity.min {
            let diagnostic = if arity.unbounded_rest {
                Diagnostic::wrong_min_argument_count(span, arity.min, got)
            } else {
                self.wrong_bounded_argument_count(
                    span,
                    arity.min,
                    arity.max.unwrap_or(arity.min),
                    got,
                )
            };
            self.emit_diagnostic(diagnostic);
            return;
        }
        if let Some(max) = arity.max {
            if got > max {
                let diagnostic = self.wrong_bounded_argument_count(span, arity.min, max, got);
                self.emit_diagnostic(diagnostic);
            }
        }
    }

    fn wrong_bounded_argument_count(
        &self,
        span: Span,
        min: usize,
        max: usize,
        got: usize,
    ) -> Diagnostic {
        if min == max {
            Diagnostic::wrong_argument_count(span, min, got)
        } else {
            Diagnostic::wrong_argument_count_range(span, min, max, got)
        }
    }

    fn call_arity(&self, params: &[ParameterType]) -> CallArity {
        let function = FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: params.to_vec(),
            ret: self.interner.well_known().void,
        };
        let fixed = function.total_fixed_param_count();
        let mut min = function.required_param_count();
        let mut max = Some(fixed);
        let mut unbounded_rest = false;
        if let Some(rest) = function.rest_param() {
            let rest_arity = self.rest_parameter_arity(rest.ty);
            min += rest_arity.min;
            max = rest_arity.max.map(|rest_max| fixed + rest_max);
            unbounded_rest = rest_arity.max.is_none();
        }
        CallArity {
            min,
            max,
            unbounded_rest,
        }
    }

    fn rest_parameter_arity(&self, rest_ty: TypeId) -> RestArity {
        if let Some(shapes) = self.rest_call_shapes(rest_ty) {
            let min = shapes.iter().map(RestCallShape::min_len).min().unwrap_or(0);
            let max = if self.interner.store().union_members(rest_ty).is_some() {
                // Union-rest mismatches are reported once against the tuple union.
                None
            } else if shapes.iter().any(|shape| shape.max_len().is_none()) {
                None
            } else {
                shapes.iter().filter_map(RestCallShape::max_len).max()
            };
            return RestArity { min, max };
        }
        RestArity { min: 0, max: None }
    }

    fn call_argument_targets(
        &mut self,
        params: &[ParameterType],
        arg_count: usize,
    ) -> Vec<Option<TypeId>> {
        let alternatives = self.call_argument_target_alternatives(params, arg_count);
        let Some(first) = alternatives.first() else {
            return vec![None; arg_count];
        };
        if alternatives.len() == 1 {
            return first.clone();
        }
        (0..arg_count)
            .map(|index| {
                let members: Vec<TypeId> = alternatives
                    .iter()
                    .filter_map(|targets| targets.get(index).copied().flatten())
                    .collect();
                (!members.is_empty()).then(|| self.interner.union(members))
            })
            .collect()
    }

    fn call_argument_target_alternatives(
        &self,
        params: &[ParameterType],
        arg_count: usize,
    ) -> Vec<Vec<Option<TypeId>>> {
        let fixed: Vec<&ParameterType> = params.iter().filter(|param| param.is_fixed()).collect();
        let rest = params.iter().find(|param| param.rest);
        let total_rest_args = arg_count.saturating_sub(fixed.len());
        let fixed_targets = || {
            (0..arg_count)
                .map(|index| fixed.get(index).map(|param| param.ty))
                .collect::<Vec<_>>()
        };
        let Some(rest) = rest else {
            return vec![fixed_targets()];
        };
        let Some(shapes) = self.rest_call_shapes(rest.ty) else {
            return if self.interner.store().union_members(rest.ty).is_some() {
                Vec::new()
            } else {
                vec![fixed_targets()]
            };
        };
        let alternatives: Vec<Vec<Option<TypeId>>> = shapes
            .iter()
            .filter(|shape| shape.accepts_len(total_rest_args))
            .map(|shape| {
                (0..arg_count)
                    .map(|index| {
                        if let Some(param) = fixed.get(index) {
                            return Some(param.ty);
                        }
                        shape.element_at(index - fixed.len(), total_rest_args)
                    })
                    .collect()
            })
            .collect();
        if !alternatives.is_empty() {
            return alternatives;
        }
        if self.interner.store().union_members(rest.ty).is_some() {
            return Vec::new();
        }
        vec![(0..arg_count)
            .map(|index| {
                if let Some(param) = fixed.get(index) {
                    return Some(param.ty);
                }
                self.rest_argument_target(rest.ty, index - fixed.len(), total_rest_args)
            })
            .collect()]
    }

    /// Contextual arrows use the same positional/rest expansion as calls, so an
    /// arrow against `(...args: [A, B]) => R` receives `A` then `B` bindings.
    pub(in crate::check::checker) fn contextual_parameter_target(
        &mut self,
        params: &[ParameterType],
        index: usize,
        parameter_count: usize,
    ) -> Option<TypeId> {
        let fixed_count = params
            .iter()
            .filter(|parameter| parameter.is_fixed())
            .count();
        let minimum_count = params
            .iter()
            .find(|parameter| parameter.rest)
            .map(|rest| fixed_count + self.rest_parameter_arity(rest.ty).min)
            .unwrap_or(fixed_count);
        self.call_argument_targets(params, parameter_count.max(minimum_count))
            .get(index)
            .copied()
            .flatten()
    }

    fn rest_argument_target(
        &self,
        rest_ty: TypeId,
        offset: usize,
        total_rest_args: usize,
    ) -> Option<TypeId> {
        self.rest_call_shapes(rest_ty)?
            .into_iter()
            .find(|shape| shape.accepts_len(total_rest_args))?
            .element_at(offset, total_rest_args)
    }

    fn rest_call_shapes(&self, rest_ty: TypeId) -> Option<Vec<RestCallShape>> {
        let rest_ty = self
            .interner
            .store()
            .readonly_operand(rest_ty)
            .unwrap_or(rest_ty);
        if let Some(members) = self.interner.store().union_members(rest_ty) {
            let members = members.to_vec();
            let mut shapes = Vec::with_capacity(members.len());
            for member in members {
                let member = self
                    .interner
                    .store()
                    .readonly_operand(member)
                    .unwrap_or(member);
                if self.interner.store().array_type(member).is_none()
                    && self.interner.store().tuple_type(member).is_none()
                {
                    return None;
                }
                shapes.extend(self.rest_call_shapes(member)?);
            }
            return Some(shapes);
        }
        if let Some(array) = self.interner.store().array_type(rest_ty) {
            return Some(vec![RestCallShape {
                prefix: Vec::new(),
                variadic: Some(array.element),
                suffix: Vec::new(),
            }]);
        }
        if let Some(tuple) = self.interner.store().tuple_type(rest_ty) {
            return self.tuple_call_shapes(tuple);
        }
        Some(vec![RestCallShape {
            prefix: Vec::new(),
            variadic: Some(rest_ty),
            suffix: Vec::new(),
        }])
    }

    fn tuple_call_shapes(&self, tuple: &TupleType) -> Option<Vec<RestCallShape>> {
        let Some(rest) = tuple.rest else {
            return Some(vec![RestCallShape {
                prefix: tuple.elements.clone(),
                variadic: None,
                suffix: Vec::new(),
            }]);
        };
        if rest.position > tuple.elements.len() {
            return None;
        }
        let suffix = tuple.elements[rest.position..].to_vec();
        self.rest_call_shapes(rest.ty)?
            .into_iter()
            .map(|rest_shape| {
                let mut prefix = tuple.elements[..rest.position].to_vec();
                prefix.extend(rest_shape.prefix);
                let mut combined_suffix = rest_shape.suffix;
                combined_suffix.extend_from_slice(&suffix);
                RestCallShape {
                    prefix,
                    variadic: rest_shape.variadic,
                    suffix: combined_suffix,
                }
            })
            .collect::<Vec<_>>()
            .into()
    }

    /// Return the contextual type for a tuple-literal position, including a represented
    /// rest segment and its trailing fixed suffix.
    pub(in crate::check::checker) fn tuple_context_element(
        &self,
        tuple: &TupleType,
        index: usize,
        total_elements: usize,
    ) -> Option<TypeId> {
        self.tuple_call_shapes(tuple)?
            .into_iter()
            .find(|shape| shape.accepts_len(total_elements))?
            .element_at(index, total_elements)
    }

    fn call_argument_obligation_kind(
        &self,
        arg_expr: &Expression<'_>,
        param_ty: TypeId,
    ) -> ObligationKind {
        let context = contextual_literal_target(self.interner.store(), param_ty);
        match arg_expr {
            Expression::ParenthesizedExpression(paren) => {
                self.call_argument_obligation_kind(&paren.expression, context)
            }
            Expression::ObjectExpression(_)
                if self.interner.store().object_type(context).is_some() =>
            {
                ObligationKind::FreshArgument
            }
            Expression::ArrayExpression(_)
                if self.interner.store().tag(context) == TypeTag::Tuple =>
            {
                ObligationKind::FreshArgument
            }
            _ => ObligationKind::Argument,
        }
    }

    /// Infer/check `new ClassName(args)` and return the instance type. Direct class
    /// constructors use shared call checks; generic classes instantiate constructor
    /// and instance types first. Non-class callees are walked but yield the error
    /// type without a `new`-specific diagnostic.
    pub(in crate::check::checker) fn infer_new(
        &mut self,
        scope: ScopeId,
        new_expr: &NewExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        self.with_provisional_argument_effects(
            |pass| pass.infer_new_inner(scope, new_expr),
            |pass| pass.rewalk_memoized_raw_arguments(scope, &new_expr.arguments),
        )
    }

    fn infer_new_inner(
        &mut self,
        scope: ScopeId,
        new_expr: &NewExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();
        let new_span = Span::from_oxc(new_expr.span);
        let direct_standalone = self.complete_standalone_namespace_value(scope, &new_expr.callee);

        // Resolve a direct class identifier, parenthesized class callee, or one-step
        // `const Alias = Class` before callee inference. Keep the class declaration
        // id for the generic constructor path below.
        let class_resolved = match self.class_new_target(scope, &new_expr.callee) {
            DemandOutcome::Ready(class_resolved) => class_resolved,
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), new_span);
                return None;
            }
        };

        // Always infer the callee for its side effects (resolving its name / emitting
        // `TK2304`, descending into a callee expression). For non-class callees the
        // inferred type is also used to find an object construct signature.
        let inferred_callee = self.infer_expr(scope, &new_expr.callee);

        // Infer every argument up front (skipping spreads — out of subset); this descends
        // into nested calls/`new`/functions inside the arguments. `arg_fresh` mirrors
        // `infer_call`'s: which arguments are fresh object/array literals (M24 clamp
        // exemption), built in the same loop so the vecs stay index-aligned.
        let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(new_expr.arguments.len());
        let mut arg_fresh: Vec<bool> = Vec::with_capacity(new_expr.arguments.len());
        let mut arg_exprs: Vec<&Expression<'_>> = Vec::with_capacity(new_expr.arguments.len());
        for (argument_index, arg) in new_expr.arguments.iter().enumerate() {
            if let Some(arg_expr) = arg.as_expression() {
                #[cfg(test)]
                measure_call(|measure| measure.raw_construct_argument_walks += 1);
                if let Some(inferred) = self.walk_raw_argument(scope, argument_index, arg_expr) {
                    arg_types.push(inferred);
                    arg_fresh.push(is_fresh_literal(arg_expr));
                    arg_exprs.push(arg_expr);
                }
            } else {
                self.record_spread_argument_skip(arg);
            }
        }

        // Not a known class: WU3 falls through to a single object construct
        // signature. If the callee is not constructable in the represented subset,
        // preserve the previous no-diagnostic/error-type behavior.
        let Some(info) = class_resolved else {
            if let Some((callee_ty, _)) = inferred_callee {
                let signatures = match self.construct_signatures(callee_ty) {
                    DemandOutcome::Ready(signatures) => signatures,
                    DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(DemandOutcome::Exhausted(exhaustion), new_span);
                        return None;
                    }
                };
                if !signatures.is_empty() {
                    match self.select_construct_candidate(
                        scope,
                        &signatures,
                        new_expr.type_arguments.as_deref(),
                        PreparedCallArgs {
                            types: &arg_types,
                            fresh: &arg_fresh,
                            exprs: &arg_exprs,
                        },
                        new_span,
                    ) {
                        DemandOutcome::Ready(Some(candidate)) => {
                            self.check_call_arguments(
                                scope,
                                &candidate.params,
                                &arg_types,
                                &arg_exprs,
                                new_span,
                            );
                            if candidate.inference_exhaustion.is_some() {
                                return None;
                            }
                            let ret = match self.evaluate_type(candidate.ret) {
                                DemandOutcome::Ready(ret) => ret,
                                DemandOutcome::Exhausted(exhaustion) => {
                                    self.own_type_demand(
                                        DemandOutcome::Exhausted(exhaustion),
                                        new_span,
                                    );
                                    return None;
                                }
                            };
                            return Some((ret, new_span));
                        }
                        DemandOutcome::Ready(None) => return Some((wk.error, new_span)),
                        DemandOutcome::Exhausted(exhaustion) => {
                            self.own_type_demand(DemandOutcome::Exhausted(exhaustion), new_span);
                            return None;
                        }
                    }
                }
                if direct_standalone || self.provably_non_callable(callee_ty) {
                    self.emit_diagnostic(Diagnostic::expression_is_not_constructable(new_span));
                }
            }
            return Some((wk.error, new_span));
        };

        // Backlog 20: constructor accessibility on a direct `new C(...)`. A
        // `private`/`protected` constructor reachable only from inside its declaring
        // class (and, for `protected`, its subclasses) emits `TK2673`/`TK2674` on the
        // whole `new` span; returns whether the constructor was inaccessible.
        let ctor_inaccessible = self.check_new_accessibility(&info, new_span);

        // M15: only the directly named class's abstract flag matters. Still run
        // argument checks; suppress when constructor accessibility already reported,
        // matching tsc's single accessibility error in that combination.
        if info.is_abstract && !ctor_inaccessible {
            self.emit_diagnostic(Diagnostic::abstract_instantiation(new_span));
        }

        // M16: instantiate a generic class's constructor + instance before the argument
        // checks. For a non-generic class this is the identity (`ctor`/`instance` unchanged),
        // so M11 behaviour is preserved. Explicit type arguments substitute directly; no type
        // arguments infer the parameters from the constructor argument types (M10 engine).
        let (ctor, instance) = self.new_class_substitution(
            scope,
            &info,
            new_expr,
            (&arg_types, &arg_fresh, &arg_exprs),
        )?;

        // A class constructor overload set publishes only its visible signatures on
        // the static side. Select those for direct non-generic construction; the
        // implementation signature remains body-only and must not accept extra calls.
        let constructor_overloads = self.direct_class_construct_overloads(&info, instance);
        if constructor_overloads.len() > 1 {
            match self.select_construct_candidate(
                scope,
                &constructor_overloads,
                None,
                PreparedCallArgs {
                    types: &arg_types,
                    fresh: &arg_fresh,
                    exprs: &arg_exprs,
                },
                new_span,
            ) {
                DemandOutcome::Ready(Some(candidate)) => {
                    self.check_call_arguments(
                        scope,
                        &candidate.params,
                        &arg_types,
                        &arg_exprs,
                        new_span,
                    );
                    return Some((instance, new_span));
                }
                DemandOutcome::Ready(None) => return Some((wk.error, new_span)),
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), new_span);
                    return None;
                }
            }
        }

        // The (instantiated) constructor signature's parameter types (zero for an implicit
        // constructor).
        let params: Vec<ParameterType> = match self.interner.store().function_type(ctor) {
            Some(func) => func.params.clone(),
            // Defensive: the constructor is always interned as a function in `fill_class`.
            None => Vec::new(),
        };
        let params = match self.evaluate_parameters(params) {
            DemandOutcome::Ready(params) => params,
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), new_span);
                return None;
            }
        };

        // Reuse the M3 call-checking path: arity (TK2554) + argument assignability
        // (TK2345). The `new` expression's type is the (instantiated) instance type.
        self.check_call_arguments(scope, &params, &arg_types, &arg_exprs, new_span);

        Some((instance, new_span))
    }

    /// Resolve the intentionally narrow class-value forms that retain class-only
    /// construction facts. Generic aliases remain on the existing structural path.
    fn class_new_target(
        &mut self,
        scope: ScopeId,
        callee: &Expression<'_>,
    ) -> DemandOutcome<Option<ClassInfo>> {
        let class_id = match callee {
            Expression::ParenthesizedExpression(paren) => {
                return self.class_new_target(scope, &paren.expression)
            }
            Expression::Identifier(ident) => {
                let Some(value_decl) = self.value_decl_id_replay(scope, ident.name.as_str()) else {
                    return DemandOutcome::Ready(None);
                };
                let class_decl = self
                    .class_value_aliases
                    .get(&value_decl)
                    .copied()
                    .unwrap_or(value_decl);
                let Some(binding) = self.class_value_bindings.get(&class_decl).copied() else {
                    return DemandOutcome::Ready(None);
                };
                if value_decl != class_decl && binding.has_header_type_params {
                    return DemandOutcome::Ready(None);
                }
                binding.class_id
            }
            Expression::StaticMemberExpression(_) => {
                let mut segments = Vec::new();
                if !flatten_static_class_value_path(callee, &mut segments) {
                    return DemandOutcome::Ready(None);
                }
                let root_is_namespace_value = self
                    .resolve_value_replay(scope, segments[0])
                    .and_then(|symbol| self.binder.symbols.get(symbol))
                    .and_then(|symbol| symbol.value)
                    .is_some_and(|storage| {
                        self.binder
                            .standalone_namespace_for_storage(storage)
                            .is_some()
                    });
                if !root_is_namespace_value {
                    return DemandOutcome::Ready(None);
                }
                let QualifiedTypePathResolution::TypeGroup(group) =
                    self.resolve_qualified_type_path_replay(scope, &segments)
                else {
                    return DemandOutcome::Ready(None);
                };
                match self.type_environment.published().groups().get(group) {
                    Some(PublishedTypeGroupTerminal::Ready(group)) => match group.surface {
                        PublishedTypeGroupSurface::Class(class_id) => class_id,
                        PublishedTypeGroupSurface::Template(_) => {
                            return DemandOutcome::Ready(None)
                        }
                    },
                    Some(PublishedTypeGroupTerminal::Unavailable(_)) | None => {
                        return DemandOutcome::Ready(None)
                    }
                }
            }
            _ => return DemandOutcome::Ready(None),
        };
        let surface = match self.published_class_replay(class_id) {
            DemandOutcome::Ready(surface) => surface,
            DemandOutcome::Exhausted(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
        };
        let Some(ctor) = surface.constructor_template() else {
            return DemandOutcome::Ready(None);
        };
        let metadata = self
            .class_new_metadata
            .get(&class_id)
            .copied()
            .expect("every published source class freezes its new metadata");
        DemandOutcome::Ready(Some(ClassInfo {
            ctor,
            class_id,
            is_abstract: metadata.is_abstract,
            ctor_visibility: metadata.ctor_visibility,
            ctor_declaring_class: metadata.ctor_declaring_class,
        }))
    }

    fn direct_class_construct_overloads(
        &mut self,
        info: &ClassInfo,
        instance: TypeId,
    ) -> Vec<TypeId> {
        let has_source_overloads = self
            .class_new_metadata
            .get(&info.class_id)
            .is_some_and(|metadata| metadata.has_source_overloads);
        if !has_source_overloads {
            return Vec::new();
        }
        let surface = match self.published_class_replay(info.class_id) {
            DemandOutcome::Ready(surface) => surface,
            DemandOutcome::Exhausted(_) => return Vec::new(),
        };
        let signatures = self
            .interner
            .store()
            .object_type(surface.static_template())
            .map_or_else(Vec::new, |object| object.construct_signatures.clone());
        let Some(application) = self.interner.store().class_instance_type(instance) else {
            return signatures;
        };
        let arguments = application.args.clone();
        let parameters = self
            .class_application_parameters
            .get(&info.class_id)
            .map(|parameters| {
                parameters
                    .iter()
                    .map(|parameter| parameter.application().id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let substitution: FxHashMap<TypeParamId, TypeId> =
            parameters.into_iter().zip(arguments).collect();
        signatures
            .into_iter()
            .map(|signature| substitute(self.interner, signature, &substitution))
            .collect()
    }

    /// Construct signatures after apparent-type resolution.
    fn construct_signatures(&mut self, callee_ty: TypeId) -> DemandOutcome<Vec<TypeId>> {
        let callee_ty = match self.demand_structural_apparent_type(callee_ty) {
            DemandOutcome::Ready(callee_ty) => callee_ty,
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        };
        if self.interner.store().tag(callee_ty) != TypeTag::Object {
            return DemandOutcome::Ready(Vec::new());
        }
        let Some(object) = self.interner.store().object_type(callee_ty) else {
            return DemandOutcome::Ready(Vec::new());
        };
        DemandOutcome::Ready(object.construct_signatures.clone())
    }

    fn select_construct_candidate(
        &mut self,
        scope: ScopeId,
        signatures: &[TypeId],
        type_arguments: Option<&TSTypeParameterInstantiation<'_>>,
        args: PreparedCallArgs<'_, '_>,
        span: Span,
    ) -> DemandOutcome<Option<CallCandidate>> {
        self.select_candidate(CandidateSelectionRequest {
            scope,
            signatures,
            type_arguments,
            args,
            span,
            receiver: CandidateReceiver::Construct,
        })
    }

    /// M16 generic-class substitution for `new`: explicit type args or M10
    /// constructor-argument inference build one map, then both constructor and
    /// instance type are substituted. Empty/non-generic maps are the M11 identity.
    fn new_class_substitution(
        &mut self,
        scope: ScopeId,
        info: &ClassInfo,
        new_expr: &NewExpression<'_>,
        args: (&[(TypeId, Span)], &[bool], &[&Expression<'_>]),
    ) -> Option<(TypeId, TypeId)> {
        let (arg_types, arg_fresh, arg_exprs) = args;
        // Non-generic class: no parameters to substitute — the M11 identity.
        let Some(type_params) =
            self.class_application_parameters
                .get(&info.class_id)
                .map(|parameters| {
                    parameters
                        .iter()
                        .map(|parameter| parameter.application().id)
                        .collect::<Vec<_>>()
                })
        else {
            self.own_type_demand(
                DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                    class: info.class_id,
                    state: ClassConstructionState::Published,
                }),
                Span::from_oxc(new_expr.span),
            );
            return None;
        };

        let descriptors = self
            .class_application_parameters
            .get(&info.class_id)
            .cloned()
            .unwrap_or_default();
        let parameters = descriptors
            .iter()
            .map(|parameter| *parameter.application())
            .collect::<Vec<_>>();
        let generic_params = descriptors
            .iter()
            .map(|parameter| GenericTypeParam {
                id: parameter.application().id,
                constraint: parameter.constraint(),
                default: None,
            })
            .collect::<Vec<_>>();

        let mut explicit_arguments = Vec::new();
        let inferred: Vec<(TypeParamId, TypeId)> = match new_expr.type_arguments.as_deref() {
            // Explicit type arguments: lower each and zip to the class's parameters.
            Some(args) => {
                let mut map = FxHashMap::default();
                // Kept aligned so the constraint check pairs each param with its own
                // argument even when an earlier one is unlowerable (out of subset).
                let mut checked_params: Vec<TypeParamId> = Vec::with_capacity(args.params.len());
                let mut arg_infos: Vec<(TypeId, Span)> = Vec::with_capacity(args.params.len());
                for (index, arg) in args.params.iter().enumerate() {
                    let lowered = self.lower_annotation(scope, arg);
                    explicit_arguments.push(match lowered {
                        Some(lowered) if lowered != self.interner.well_known().error => {
                            ExplicitClassArgument::Ready(lowered)
                        }
                        Some(_) | None => ExplicitClassArgument::Unavailable,
                    });
                    if let (Some(&param), Some(lowered)) = (type_params.get(index), lowered) {
                        map.insert(param, lowered);
                        checked_params.push(param);
                        arg_infos.push((lowered, Span::from_oxc(arg.span())));
                    }
                }
                // M24: each explicit type argument must satisfy its parameter's constraint.
                let checks: Vec<(Option<TypeId>, TypeId, Span)> = checked_params
                    .iter()
                    .zip(&arg_infos)
                    .map(|(&parameter, &(argument, span))| {
                        (
                            self.interner.store().type_param_constraint(parameter),
                            argument,
                            span,
                        )
                    })
                    .collect();
                if let DemandOutcome::Exhausted(exhaustion) =
                    self.check_constraint_arguments_outcome(&checks, &map)
                {
                    self.own_type_demand(
                        DemandOutcome::Exhausted(exhaustion),
                        Span::from_oxc(new_expr.span),
                    );
                    return None;
                }
                Vec::new()
            }
            // No type arguments: infer from the constructor argument types. The inference
            // targets are the *uninstantiated* constructor parameter types (they carry the
            // type parameters). Snapshot them before the mutable inference call.
            None => {
                let params: Vec<ParameterType> =
                    match self.interner.store().function_type(info.ctor) {
                        Some(func) => func.params.clone(),
                        None => Vec::new(),
                    };
                let args = contextual_inference_args!(
                    self,
                    scope,
                    &params,
                    arg_types,
                    arg_exprs,
                    ContextualMeasurePhase::ClassCtor
                );
                match infer::infer_partial_signature_type_arguments_from_params(
                    self.interner,
                    &mut self.next_type_param,
                    self.type_environment.published().classes(),
                    &mut self.semantic_queries,
                    infer::SignatureInferenceRequest {
                        type_params: &generic_params,
                        params: &params,
                        args: &args,
                        fresh_args: arg_fresh,
                        receiver: None,
                    },
                ) {
                    DemandOutcome::Ready(result) => {
                        if let Some(exhaustion) = result.exhaustion {
                            self.own_type_demand(
                                DemandOutcome::Exhausted(exhaustion),
                                Span::from_oxc(new_expr.span),
                            );
                            return None;
                        }
                        result.arguments
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(
                            DemandOutcome::Exhausted(exhaustion),
                            Span::from_oxc(new_expr.span),
                        );
                        return None;
                    }
                }
            }
        };
        let source_arguments = new_expr
            .type_arguments
            .as_ref()
            .map_or(SourceClassArguments::Omitted, |_| {
                SourceClassArguments::Explicit(&explicit_arguments)
            });
        let outcome = build_class_application(
            &mut SurfaceTypeFactory::new(self.interner),
            self.type_environment.published().classes(),
            ClassApplicationRequest {
                class: info.class_id,
                parameters: &parameters,
                source_arguments,
                inferred: &inferred,
                kind: ClassApplicationKind::NewExpression,
            },
        );
        let instance = match outcome {
            DemandOutcome::Ready(instance) => instance,
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::WrongArity {
                    expected_min,
                    expected_max,
                    actual,
                },
            )) => {
                let arity_span = new_expr
                    .type_arguments
                    .as_deref()
                    .map(|arguments| Span::from_oxc(arguments.span))
                    .unwrap_or_else(|| Span::from_oxc(new_expr.span));
                self.emit_diagnostic(Diagnostic::wrong_type_argument_count(
                    arity_span,
                    expected_min,
                    expected_max,
                    actual,
                ));
                return None;
            }
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::UnsupportedDefault { .. },
            )) => {
                self.record_incomplete(
                    "expr-infer/new-expression/class-default-argument",
                    Span::from_oxc(new_expr.span),
                    "class type-parameter default unavailable at application",
                );
                return None;
            }
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::InferenceIncomplete { .. },
            )) => {
                self.record_incomplete(
                    "expr-infer/new-expression/class-type-argument-inference",
                    Span::from_oxc(new_expr.span),
                    "class type arguments cannot be fully inferred",
                );
                return None;
            }
            DemandOutcome::Exhausted(Exhaustion::EvaluationInvalidNode { .. }) => {
                self.record_incomplete(
                    "semantic-query/invalid-evaluation-node",
                    Span::from_oxc(new_expr.span),
                    "semantic evaluation reached an invalid type node",
                );
                return None;
            }
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::UnavailableExplicitArgument { .. }
                | ClassApplicationArguments::TargetPoisoned { .. },
            ))
            | DemandOutcome::Exhausted(Exhaustion::ClassNotPublished { .. })
            | DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { .. })
            | DemandOutcome::Exhausted(Exhaustion::ClassInitializerPoison { .. })
            | DemandOutcome::Exhausted(Exhaustion::ClassSurfacePoison { .. })
            | DemandOutcome::Exhausted(Exhaustion::ClassProjectionBudget)
            | DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
            | DemandOutcome::Exhausted(Exhaustion::EvaluationCycle { .. }) => return None,
        };
        let application = self
            .interner
            .store()
            .class_instance_type(instance)
            .cloned()?;
        let map: FxHashMap<TypeParamId, TypeId> =
            type_params.into_iter().zip(application.args).collect();
        let ctor = substitute(self.interner, info.ctor, &map);
        Some((ctor, instance))
    }

    /// Reserve a function declaration's callable signature before its body is checked.
    /// Generic ids, constraints, parameters, and a declared return are established
    /// exactly once so callers can use the surface during the body-fill phase.
    pub(in crate::check::checker) fn reserve_function(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
    ) -> FunctionReservation<Ticket> {
        let retained = self
            .lexical_events
            .callable_at(
                super::lexical_events::source_ordinal(self.current_source),
                func.span.start,
            )
            .and_then(|site| self.lexical_events.callable(site))
            .map(|callable| (callable.binding.clone(), callable.tickets));
        let (type_params, tickets) = match retained {
            Some((binding, tickets)) => (
                binding
                    .expect("reserved callable binders must exist before signature lowering")
                    .type_params,
                Some(tickets),
            ),
            None => (
                alloc_type_param_ids(func.type_parameters.as_deref(), &mut self.next_type_param),
                None,
            ),
        };
        let type_param_frame =
            self.build_type_param_frame(func.type_parameters.as_deref(), &type_params);
        let (generic_params, receiver, params, declared_return) =
            self.with_type_params(type_param_frame.clone(), |pass| {
                let generic_params = pass.lower_signature_type_params(
                    enclosing,
                    func.type_parameters.as_deref(),
                    &type_params,
                );
                let fn_scope = pass
                    .binder
                    .fn_scopes
                    .get(&(pass.current_module, func.span.start))
                    .copied();
                let receiver = match func.this_param.as_ref() {
                    Some(this_param) => this_param
                        .type_annotation
                        .as_ref()
                        .and_then(|annotation| {
                            pass.lower_callable_annotation(
                                enclosing,
                                &annotation.type_annotation,
                                false,
                            )
                        })
                        .map(Some),
                    None => Some(None),
                };
                let params = pass.lower_parameter_slots(enclosing, fn_scope, &func.params, false);
                // Type references in the signature resolve from the enclosing scope,
                // while declared type parameters resolve through the pushed frame.
                let declared_return = match func.return_type.as_ref() {
                    Some(annotation) => pass
                        .lower_callable_annotation(enclosing, &annotation.type_annotation, false)
                        .map(Some),
                    None => Some(None),
                };
                (generic_params, receiver, params, declared_return)
            });
        let unavailable = generic_params.unavailable
            || receiver.is_none()
            || params.iter().any(Option::is_none)
            || declared_return.is_none();
        let receiver = receiver.flatten();
        let declared_return = declared_return.flatten();
        if unavailable {
            return FunctionReservation::Unavailable(RetainedFunctionBodySurface {
                type_param_frame,
                receiver,
                params,
                declared_return,
                tickets,
            });
        }
        let params = params
            .into_iter()
            .map(|parameter| parameter.expect("ready function retains every parameter"))
            .collect::<Vec<_>>();
        let ret = declared_return.unwrap_or_else(|| {
            let well_known = self.interner.well_known();
            if func.body.is_some() {
                // Backlog 76 owns pre-body return inference. `unknown` keeps forward
                // reads conservative without pretending the function returns `void`.
                well_known.unknown
            } else {
                well_known.void
            }
        });
        let function_ty = self.interner.intern_function(FunctionType {
            type_params: generic_params.params.clone(),
            receiver,
            params: params.clone(),
            ret,
        });
        FunctionReservation::Ready(FunctionSurface {
            receiver,
            params,
            generic_params: generic_params.params,
            type_param_frame,
            declared_return,
            function_ty,
            tickets,
        })
    }

    /// Check a previously reserved function body and return its completed callable
    /// type. Reservation has already installed parameters and constraints, so this
    /// pass visits only the body and cannot duplicate signature diagnostics.
    pub(in crate::check::checker) fn fill_reserved_function(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
        surface: &FunctionSurface<Ticket>,
    ) -> TypeId {
        match surface.tickets {
            Some(tickets) => self.with_ticket_effects(tickets.body, |pass| {
                pass.fill_reserved_function_inner(enclosing, func, surface)
            }),
            None => self.fill_reserved_function_inner(enclosing, func, surface),
        }
    }

    fn fill_reserved_function_inner(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
        surface: &FunctionSurface<Ticket>,
    ) -> TypeId {
        let params = surface.params.clone();
        let receiver = surface.receiver;
        let generic_params = surface.generic_params.clone();
        self.with_type_params(surface.type_param_frame.clone(), |pass| {
            let fn_scope = pass
                .binder
                .fn_scopes
                .get(&(pass.current_module, func.span.start))
                .copied();
            let body_scope = fn_scope.unwrap_or(enclosing);
            pass.bind_retained_parameter_types(fn_scope, &func.params, &surface.params);
            pass.check_reserved_parameter_initializers(body_scope, &func.params, &surface.params);
            let saved_this = pass.current_this;
            if let Some(receiver) = receiver {
                pass.current_this = Some(receiver);
            }
            let inferred_return = func
                .body
                .as_ref()
                .map(|body| pass.check_function_body(body_scope, body, surface.declared_return));
            pass.current_this = saved_this;
            let ret = resolve_return_type(pass.interner, surface.declared_return, inferred_return);
            pass.interner.intern_function(FunctionType {
                type_params: generic_params,
                receiver,
                params,
                ret,
            })
        })
    }

    /// Check the executable part of a callable whose public signature could not be
    /// completed. Successfully lowered binders and parameters remain available to
    /// the body, but no partial callable type is interned or published.
    pub(in crate::check::checker) fn check_retained_function_body(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
        surface: &RetainedFunctionBodySurface<Ticket>,
    ) {
        let type_param_frame = surface.type_param_frame.clone();
        let receiver = surface.receiver;
        let params = &surface.params;
        let declared_return = surface.declared_return;
        let tickets = surface.tickets;
        let check_body = |pass: &mut Self| {
            pass.with_type_params(type_param_frame, |pass| {
                let fn_scope = pass
                    .binder
                    .fn_scopes
                    .get(&(pass.current_module, func.span.start))
                    .copied();
                let body_scope = fn_scope.unwrap_or(enclosing);
                pass.bind_partial_retained_parameter_types(fn_scope, &func.params, params);
                pass.check_partial_retained_parameter_initializers(
                    body_scope,
                    &func.params,
                    params,
                );
                let saved_this = pass.current_this;
                if let Some(receiver) = receiver {
                    pass.current_this = Some(receiver);
                }
                if let Some(body) = &func.body {
                    pass.check_function_body(body_scope, body, declared_return);
                }
                pass.current_this = saved_this;
            });
        };
        match tickets {
            Some(tickets) => self.with_ticket_effects(tickets.body, check_body),
            None => check_body(self),
        }
    }

    /// Infer a function expression or class member type and check its body. Function
    /// declarations use the reserve/fill split above so their callable surfaces can
    /// be published to forward calls before executable statements are checked.
    pub(in crate::check::checker) fn infer_function(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
    ) -> TypeId {
        let tickets = self
            .lexical_events
            .callable_at(
                super::lexical_events::source_ordinal(self.current_source),
                func.span.start,
            )
            .and_then(|site| self.lexical_events.callable(site))
            .map(|callable| callable.tickets);
        let reservation = match tickets {
            Some(tickets) => self.with_ticket_effects(tickets.signature, |pass| {
                pass.reserve_function(enclosing, func)
            }),
            None => self.reserve_function(enclosing, func),
        };
        match reservation {
            FunctionReservation::Ready(surface) => {
                self.fill_reserved_function(enclosing, func, &surface)
            }
            FunctionReservation::Unavailable(surface) => {
                self.check_retained_function_body(enclosing, func, &surface);
                self.interner.well_known().error
            }
        }
    }

    /// Infer an arrow's type and check its body. Generic arrow type parameters are
    /// scoped to the signature/body only; they are not registered for explicit
    /// call-site type arguments.
    pub(in crate::check::checker) fn infer_arrow(
        &mut self,
        enclosing: ScopeId,
        arrow: &ArrowFunctionExpression<'_>,
    ) -> TypeId {
        let retained = self
            .lexical_events
            .callable_at(
                super::lexical_events::source_ordinal(self.current_source),
                arrow.span.start,
            )
            .and_then(|site| self.lexical_events.callable(site))
            .map(|callable| (callable.binding.clone(), callable.tickets));
        let (param_ids, tickets) = match retained {
            Some((binding, tickets)) => (
                binding
                    .expect("reserved callable binders must exist before arrow lowering")
                    .type_params,
                Some(tickets),
            ),
            None => (
                alloc_type_param_ids(arrow.type_parameters.as_deref(), &mut self.next_type_param),
                None,
            ),
        };
        let frame = self.build_type_param_frame(arrow.type_parameters.as_deref(), &param_ids);
        let lower_signature = |pass: &mut Self| {
            pass.with_type_params(frame.clone(), |pass| {
                pass.lower_type_param_constraints(
                    enclosing,
                    arrow.type_parameters.as_deref(),
                    &param_ids,
                );
                let fn_scope = pass
                    .binder
                    .fn_scopes
                    .get(&(pass.current_module, arrow.span.start))
                    .copied();
                let params = pass.lower_parameters(enclosing, fn_scope, &arrow.params, false);
                let declared_ret = arrow.return_type.as_ref().and_then(|ann| {
                    pass.lower_callable_annotation(enclosing, &ann.type_annotation, false)
                });
                (fn_scope, params, declared_ret)
            })
        };
        let (fn_scope, params, declared_ret) = match tickets {
            Some(tickets) => self.with_ticket_effects(tickets.signature, lower_signature),
            None => lower_signature(self),
        };
        let fill_body = |pass: &mut Self| {
            pass.with_type_params(frame, |pass| {
                let body_scope = fn_scope.unwrap_or(enclosing);
                pass.check_reserved_parameter_initializers(body_scope, &arrow.params, &params);
                pass.finish_arrow_inference(enclosing, arrow, fn_scope, params, declared_ret)
            })
        };
        match tickets {
            Some(tickets) => self.with_ticket_effects(tickets.body, fill_body),
            None => fill_body(self),
        }
    }

    /// Re-infer a non-generic arrow against a function parameter's shape. This is
    /// intentionally limited to call-site contextual typing: the first uncontextual
    /// walk has already handled side effects, while this pass supplies the callback
    /// parameter types needed for generic method inference and checking.
    pub(in crate::check::checker) fn infer_contextual_arrow(
        &mut self,
        enclosing: ScopeId,
        arrow: &ArrowFunctionExpression<'_>,
        context: TypeId,
    ) -> Option<TypeId> {
        self.infer_contextual_arrow_with_return_context(enclosing, arrow, context, true)
    }

    /// Contextualize an arrow's parameters and, when requested, its return. Call
    /// arguments retain their source return so the enclosing argument obligation owns
    /// a callback incompatibility (`TK2345`) instead of an inner assignment diagnostic.
    pub(in crate::check::checker) fn infer_contextual_arrow_with_return_context(
        &mut self,
        enclosing: ScopeId,
        arrow: &ArrowFunctionExpression<'_>,
        context: TypeId,
        contextual_return: bool,
    ) -> Option<TypeId> {
        if arrow.type_parameters.is_some() {
            return None;
        }
        let context = self.contextual_function_shape(context)?;
        let fn_scope = self
            .binder
            .fn_scopes
            .get(&(self.current_module, arrow.span.start))
            .copied();
        let params = self.lower_contextual_arrow_parameters(
            enclosing,
            fn_scope,
            &arrow.params,
            &context.params,
        );
        let declared_ret = match arrow.return_type.as_ref() {
            Some(ann) => self.lower_annotation(enclosing, &ann.type_annotation),
            None if !contextual_return => None,
            // An unresolved generic return is the inference variable itself. Infer
            // the callback return normally so that variable can receive a candidate;
            // a concrete expected return is checked in the arrow body instead.
            None if self.interner.store().tag(context.ret) == TypeTag::TypeParam => None,
            None => Some(context.ret),
        };
        Some(self.finish_arrow_inference(enclosing, arrow, fn_scope, params, declared_ret))
    }

    fn contextual_function_shape(&self, context: TypeId) -> Option<FunctionType> {
        let members = self
            .interner
            .store()
            .union_members(context)
            .map_or_else(|| vec![context], <[_]>::to_vec);
        let mut candidate = None;
        for member in members {
            let member = self.apparent_type(member);
            let signatures = match self.interner.store().tag(member) {
                TypeTag::Function => vec![member],
                TypeTag::Object => self
                    .interner
                    .store()
                    .object_type(member)
                    .map(|object| object.call_signatures.clone())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            for signature in signatures {
                if candidate.replace(signature).is_some() {
                    return None;
                }
            }
        }
        self.interner.store().function_type(candidate?).cloned()
    }

    fn lower_contextual_arrow_parameters(
        &mut self,
        enclosing: ScopeId,
        fn_scope: Option<ScopeId>,
        params: &FormalParameters<'_>,
        context: &[ParameterType],
    ) -> Vec<ParameterType> {
        let error_ty = self.interner.well_known().error;
        let mut lowered = Vec::with_capacity(parameter_count(params));
        for syntax in parameter_syntaxes(params) {
            let name = syntax.name().unwrap_or_default();
            let ty = match syntax {
                ParameterSyntax::Fixed { index, parameter } => parameter
                    .type_annotation
                    .as_ref()
                    .and_then(|ann| self.lower_annotation(enclosing, &ann.type_annotation))
                    .unwrap_or_else(|| {
                        self.contextual_parameter_target(context, index, params.items.len())
                            .unwrap_or(error_ty)
                    }),
                ParameterSyntax::Rest { parameter } => parameter
                    .type_annotation
                    .as_ref()
                    .and_then(|ann| self.lower_annotation(enclosing, &ann.type_annotation))
                    .unwrap_or_else(|| {
                        context
                            .iter()
                            .find(|parameter| parameter.rest)
                            .map(|parameter| parameter.ty)
                            .unwrap_or(error_ty)
                    }),
            };
            if let Some(scope) = fn_scope {
                if let Some(decl_id) = parameter_name(syntax.pattern())
                    .and_then(|name| self.binder.resolve_value(scope, &name))
                    .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                    .and_then(|symbol| symbol.value)
                {
                    self.publish_copied_decl_type_replay(decl_id, ty);
                }
            }
            lowered.push(syntax.with_type(name, ty));
        }
        lowered
    }

    fn finish_arrow_inference(
        &mut self,
        enclosing: ScopeId,
        arrow: &ArrowFunctionExpression<'_>,
        fn_scope: Option<ScopeId>,
        params: Vec<ParameterType>,
        declared_ret: Option<TypeId>,
    ) -> TypeId {
        let body_scope = fn_scope.unwrap_or(enclosing);

        let inferred_ret = if let Some(body_expr) = arrow.get_expression() {
            // Expression body `() => expr`: the return value is the expression.
            let value = match declared_ret {
                Some(ret) => self.infer_initializer(body_scope, body_expr, Some(ret)),
                None => self.infer_expr(body_scope, body_expr),
            };
            match (declared_ret, value) {
                // With a declared return type, the body expression is checked against
                // it (primary span = the expression), like a `return <expr>`.
                (Some(ret), Some((src, src_span))) => {
                    let target_ready = match self.check_excess_properties_for_target(body_expr, ret)
                    {
                        DemandOutcome::Ready(diagnostics) => {
                            for diagnostic in diagnostics {
                                self.emit_diagnostic(diagnostic);
                            }
                            true
                        }
                        DemandOutcome::Exhausted(exhaustion) => {
                            self.own_type_demand(DemandOutcome::Exhausted(exhaustion), src_span);
                            false
                        }
                    };
                    if target_ready {
                        self.schedule_obligation(AssignObligation {
                            src,
                            tgt: ret,
                            src_span,
                            source_member_spans: Vec::new(),
                            kind: ObligationKind::Assignment,
                        });
                    }
                    None
                }
                // No annotation: infer the return type from the body, widened.
                (None, Some((value_ty, _))) => Some(widen(self.interner, value_ty)),
                // An expression body that could not produce a type is not a
                // value-less body; retain recovery so the missing child cannot
                // fabricate a `void` return and trigger a cascading relation.
                (None, None) => Some(self.interner.well_known().error),
                _ => None,
            }
        } else {
            // Block body `() => { ... }`: same as a function body.
            Some(self.check_function_body(body_scope, &arrow.body, declared_ret))
        };

        let ret = resolve_return_type(self.interner, declared_ret, inferred_ret);
        self.interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params,
            ret,
        })
    }

    /// Lower a function's/arrow's parameters to `ParameterType`s and, when a function
    /// scope is known, record each parameter's type in `decl_types` so the body can
    /// resolve it. An un-annotated parameter is out of the MVP subset → the error
    /// type (no diagnostic), matching M0/M1 leniency.
    fn lower_parameters(
        &mut self,
        enclosing: ScopeId,
        fn_scope: Option<ScopeId>,
        params: &FormalParameters<'_>,
        check_initializers: bool,
    ) -> Vec<ParameterType> {
        let error_ty = self.interner.well_known().error;
        parameter_syntaxes(params)
            .zip(self.lower_parameter_slots(enclosing, fn_scope, params, check_initializers))
            .map(|(syntax, lowered)| {
                lowered.unwrap_or_else(|| {
                    syntax.with_type(syntax.name().unwrap_or_default(), error_ty)
                })
            })
            .collect()
    }

    fn lower_parameter_slots(
        &mut self,
        enclosing: ScopeId,
        fn_scope: Option<ScopeId>,
        params: &FormalParameters<'_>,
        check_initializers: bool,
    ) -> Vec<Option<ParameterType>> {
        let error_ty = self.interner.well_known().error;
        let mut lowered = Vec::with_capacity(parameter_count(params));
        let parameter_scope = fn_scope.unwrap_or(enclosing);
        for syntax in parameter_syntaxes(params) {
            let name = syntax.name().unwrap_or_default();
            let ty = match syntax {
                ParameterSyntax::Fixed { parameter, .. } => {
                    // Annotated type, or the error type for an un-annotated parameter. Type
                    // references in the annotation resolve from the enclosing scope.
                    let annotation_ty = parameter.type_annotation.as_ref().and_then(|ann| {
                        self.lower_callable_annotation(enclosing, &ann.type_annotation, false)
                    });
                    let ty = if parameter.type_annotation.is_some() {
                        annotation_ty
                    } else {
                        Some(error_ty)
                    };

                    // F4: object destructuring parameters run M13 access checks against the
                    // annotation type only; binding destructured names is deferred. The
                    // annotation resolves in the enclosing class context.
                    if let BindingPattern::ObjectPattern(object) = &parameter.pattern {
                        if let Some(annotation_ty) = annotation_ty {
                            match self.demand_apparent_type(annotation_ty) {
                                DemandOutcome::Ready(source) => {
                                    self.check_object_pattern_access(object, source);
                                }
                                DemandOutcome::Exhausted(exhaustion) => {
                                    self.own_type_demand(
                                        DemandOutcome::Exhausted(exhaustion),
                                        Span::from_oxc(object.span),
                                    );
                                }
                            }
                        }
                    }

                    if check_initializers {
                        if let (Some(init), Some(annotation_ty)) =
                            (&parameter.initializer, annotation_ty)
                        {
                            self.check_pattern_annotated_initializer(
                                parameter_scope,
                                Some(annotation_ty),
                                &parameter.pattern,
                                init,
                            );
                        }
                    }
                    ty
                }
                ParameterSyntax::Rest { parameter } => match parameter.type_annotation.as_ref() {
                    Some(ann) => {
                        self.lower_callable_annotation(enclosing, &ann.type_annotation, false)
                    }
                    None => Some(error_ty),
                },
            };

            let parameter = ty.map(|ty| syntax.with_type(name, ty));

            // Bind the parameter's type into the function scope so the body resolves
            // it (the binder declared the parameter symbol + value-storage id).
            if let (Some(scope), Some(parameter)) = (fn_scope, parameter.as_ref()) {
                if let Some(decl_id) = parameter_name(syntax.pattern())
                    .and_then(|n| self.binder.resolve_value(scope, &n))
                    .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                    .and_then(|s| s.value)
                {
                    self.publish_copied_decl_type_replay(decl_id, parameter.ty);
                }
            }

            lowered.push(parameter);
        }
        lowered
    }

    /// Parameter defaults are executable expressions, so declaration pre-reservation
    /// leaves them until the original function source position. Their parameter types
    /// were lowered once into the reserved surface.
    fn check_reserved_parameter_initializers(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        lowered: &[ParameterType],
    ) {
        for (param, parameter) in params.items.iter().zip(lowered) {
            if param.type_annotation.is_some() {
                if let Some(init) = &param.initializer {
                    self.check_pattern_annotated_initializer(
                        scope,
                        Some(parameter.ty),
                        &param.pattern,
                        init,
                    );
                }
            }
        }
    }

    fn bind_retained_parameter_types(
        &mut self,
        fn_scope: Option<ScopeId>,
        params: &FormalParameters<'_>,
        lowered: &[ParameterType],
    ) {
        let Some(scope) = fn_scope else {
            return;
        };
        for (parameter, lowered) in params.items.iter().zip(lowered) {
            let Some(decl_id) = parameter_name(&parameter.pattern)
                .and_then(|name| self.binder.resolve_value(scope, &name))
                .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                .and_then(|symbol| symbol.value)
            else {
                continue;
            };
            self.publish_copied_decl_type_replay(decl_id, lowered.ty);
        }
        if let (Some(rest), Some(lowered)) = (params.rest.as_ref(), lowered.get(params.items.len()))
        {
            let Some(decl_id) = parameter_name(&rest.rest.argument)
                .and_then(|name| self.binder.resolve_value(scope, &name))
                .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                .and_then(|symbol| symbol.value)
            else {
                return;
            };
            self.publish_copied_decl_type_replay(decl_id, lowered.ty);
        }
    }

    fn bind_partial_retained_parameter_types(
        &mut self,
        fn_scope: Option<ScopeId>,
        params: &FormalParameters<'_>,
        lowered: &[Option<ParameterType>],
    ) {
        let Some(scope) = fn_scope else {
            return;
        };
        for (parameter, lowered) in params.items.iter().zip(lowered) {
            let Some(lowered) = lowered else {
                continue;
            };
            let Some(decl_id) = parameter_name(&parameter.pattern)
                .and_then(|name| self.binder.resolve_value(scope, &name))
                .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                .and_then(|symbol| symbol.value)
            else {
                continue;
            };
            self.publish_copied_decl_type_replay(decl_id, lowered.ty);
        }
        if let (Some(rest), Some(Some(lowered))) =
            (params.rest.as_ref(), lowered.get(params.items.len()))
        {
            let Some(decl_id) = parameter_name(&rest.rest.argument)
                .and_then(|name| self.binder.resolve_value(scope, &name))
                .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                .and_then(|symbol| symbol.value)
            else {
                return;
            };
            self.publish_copied_decl_type_replay(decl_id, lowered.ty);
        }
    }

    fn check_partial_retained_parameter_initializers(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        lowered: &[Option<ParameterType>],
    ) {
        for (param, lowered) in params.items.iter().zip(lowered) {
            if let Some(initializer) = &param.initializer {
                self.check_pattern_annotated_initializer(
                    scope,
                    lowered.as_ref().map(|lowered| lowered.ty),
                    &param.pattern,
                    initializer,
                );
            }
        }
    }

    /// Walk a function body, checking returns against a declared type or inferring
    /// the first value return's widened type. Missing-return analysis (`TK2355`)
    /// remains deferred.
    fn check_function_body(
        &mut self,
        scope: ScopeId,
        body: &FunctionBody<'_>,
        declared_ret: Option<TypeId>,
    ) -> TypeId {
        let void_ty = self.interner.well_known().void;
        let mut inferred: Option<TypeId> = None;

        // M23: the function-boundary narrowing reset lives in the flow pre-pass (each
        // body is built at its own `START`, so a reference never sees the caller's
        // narrowing — the documented closure divergence). The check walk here just
        // descends into the body, via the shared list walker so a *local* overload
        // set is grouped exactly like a top-level one (M33).
        self.check_statement_list(scope, &body.statements, declared_ret, &mut inferred);

        inferred.unwrap_or(void_ty)
    }

    fn complete_standalone_namespace_value(
        &self,
        scope: ScopeId,
        expression: &Expression<'_>,
    ) -> bool {
        let Some(name) = direct_identifier_name(expression) else {
            return false;
        };
        let Some(value) = self
            .resolve_value_replay(scope, name)
            .and_then(|symbol| self.binder.symbols.get(symbol))
            .and_then(|symbol| symbol.value)
        else {
            return false;
        };
        let root = self
            .binder
            .standalone_namespace_for_storage(value)
            .map(|_| value)
            .or_else(|| self.standalone_namespace_value_aliases.get(&value).copied());
        let Some(root) = root else {
            return false;
        };
        let Some(namespace) = self.binder.standalone_namespace_for_storage(root) else {
            return false;
        };
        matches!(
            self.standalone_namespace_terminal_replay(namespace),
            Some(super::namespace_values::StandaloneNamespaceTerminal::Ready {
                storage,
                ..
            }) if storage == root
        )
    }
}

fn direct_identifier_name<'a>(expression: &'a Expression<'_>) -> Option<&'a str> {
    match expression {
        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
        Expression::ParenthesizedExpression(parenthesized) => {
            direct_identifier_name(&parenthesized.expression)
        }
        _ => None,
    }
}

struct CallCandidate {
    receiver: Option<TypeId>,
    params: Vec<ParameterType>,
    ret: TypeId,
    inference_exhaustion: Option<Exhaustion>,
}

enum CallableSignatures {
    Ready(Vec<TypeId>),
    ProvablyNone,
    Unavailable,
}

enum CommonUnionSignatures {
    Ready(Vec<TypeId>),
    Unavailable,
}

enum SignatureSearch {
    Found(TypeId),
    NotFound,
    Unavailable,
}

enum SignatureMatches {
    Found(Vec<TypeId>),
    NotFound,
    Unavailable,
}

enum SignatureComparison {
    Match,
    Mismatch,
    Unavailable,
}

enum SignatureCombination {
    Combined(TypeId),
    NoCommon,
    Unavailable,
}

enum SignatureCombinationMode {
    Matching,
    Fallback,
}

enum SignaturePosition {
    Type(TypeId),
    Missing,
    Unavailable,
}

#[derive(Copy, Clone)]
enum CandidateReceiver {
    Call(Option<TypeId>),
    Construct,
}

impl CandidateReceiver {
    fn inference_source(self) -> Option<TypeId> {
        match self {
            Self::Call(receiver) => receiver,
            Self::Construct => None,
        }
    }

    fn trial_receivers(self, candidate: &CallCandidate) -> (Option<TypeId>, Option<TypeId>) {
        match self {
            Self::Call(receiver) => (candidate.receiver, receiver),
            Self::Construct => (None, None),
        }
    }
}

#[derive(Copy, Clone)]
struct PreparedCallArgs<'a, 'ast> {
    types: &'a [(TypeId, Span)],
    fresh: &'a [bool],
    exprs: &'a [&'a Expression<'ast>],
}

struct CandidateSelectionRequest<'signatures, 'type_arguments, 'args, 'ast> {
    scope: ScopeId,
    signatures: &'signatures [TypeId],
    type_arguments: Option<&'type_arguments TSTypeParameterInstantiation<'ast>>,
    args: PreparedCallArgs<'args, 'ast>,
    span: Span,
    receiver: CandidateReceiver,
}

struct SignatureCandidateRequest<'a, 'ast> {
    scope: ScopeId,
    signature_ty: TypeId,
    type_arguments: Option<&'a TSTypeParameterInstantiation<'ast>>,
    args: PreparedCallArgs<'a, 'ast>,
    call_receiver: Option<TypeId>,
    commit_constraints: bool,
    reject_inferred_constraint_violations: bool,
}

enum CandidateTrial {
    Match,
    Arity(CallArity),
    Mismatch,
    Exhausted(Exhaustion),
}

enum CandidateBuildFailure {
    Constraint,
    InferredConstraint,
    Exhausted(Exhaustion),
    Unavailable,
}

#[derive(Clone)]
struct CallArity {
    min: usize,
    max: Option<usize>,
    unbounded_rest: bool,
}

#[derive(Clone, Copy)]
struct RestArity {
    min: usize,
    max: Option<usize>,
}

#[derive(Clone)]
struct RestCallShape {
    prefix: Vec<TypeId>,
    variadic: Option<TypeId>,
    suffix: Vec<TypeId>,
}

impl RestCallShape {
    fn min_len(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    fn max_len(&self) -> Option<usize> {
        if self.variadic.is_some() {
            None
        } else {
            Some(self.min_len())
        }
    }

    fn accepts_len(&self, len: usize) -> bool {
        if len < self.min_len() {
            return false;
        }
        self.variadic.is_some() || len == self.min_len()
    }

    fn element_at(&self, index: usize, len: usize) -> Option<TypeId> {
        if !self.accepts_len(len) || index >= len {
            return None;
        }
        if index < self.prefix.len() {
            return self.prefix.get(index).copied();
        }
        let suffix_start = len.saturating_sub(self.suffix.len());
        if index >= suffix_start {
            return self.suffix.get(index - suffix_start).copied();
        }
        self.variadic
    }
}

/// The function's return type: a declared annotation always wins; otherwise the
/// inferred type; otherwise `void` (a function with no body and no annotation,
/// which is out of the subset but handled defensively).
fn resolve_return_type(
    interner: &mut Interner,
    declared: Option<TypeId>,
    inferred: Option<TypeId>,
) -> TypeId {
    declared
        .or(inferred)
        .unwrap_or_else(|| interner.well_known().void)
}

/// Whether a call/`new` argument is a fresh object/array literal for the M24
/// clamp-to-constraint exemption. Freshness is syntactic; parentheses are
/// transparent.
fn is_fresh_literal(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => true,
        Expression::ParenthesizedExpression(paren) => is_fresh_literal(&paren.expression),
        _ => false,
    }
}

/// The parameter name of a binding pattern, if it is a plain identifier. `None`
/// for destructuring patterns (out of the M3 subset).
pub(in crate::check::checker) fn parameter_name(pattern: &BindingPattern<'_>) -> Option<String> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.to_string()),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(in crate::check::checker) enum ParameterSyntax<'params, 'ast> {
    Fixed {
        index: usize,
        parameter: &'params FormalParameter<'ast>,
    },
    Rest {
        parameter: &'params FormalParameterRest<'ast>,
    },
}

impl<'params, 'ast> ParameterSyntax<'params, 'ast> {
    pub(in crate::check::checker) fn pattern(self) -> &'params BindingPattern<'ast> {
        match self {
            ParameterSyntax::Fixed { parameter, .. } => &parameter.pattern,
            ParameterSyntax::Rest { parameter } => &parameter.rest.argument,
        }
    }

    pub(in crate::check::checker) fn name(self) -> Option<String> {
        parameter_name(self.pattern())
    }

    pub(in crate::check::checker) fn with_type(
        self,
        name: impl Into<String>,
        ty: TypeId,
    ) -> ParameterType {
        match self {
            ParameterSyntax::Fixed { parameter, .. } => parameter_from_shape(
                name,
                ty,
                parameter.optional,
                parameter.initializer.is_some(),
            ),
            ParameterSyntax::Rest { .. } => ParameterType::rest(name, ty),
        }
    }
}

pub(in crate::check::checker) fn parameter_syntaxes<'params, 'ast>(
    params: &'params FormalParameters<'ast>,
) -> impl Iterator<Item = ParameterSyntax<'params, 'ast>> + 'params {
    params
        .items
        .iter()
        .enumerate()
        .map(|(index, parameter)| ParameterSyntax::Fixed { index, parameter })
        .chain(
            params
                .rest
                .iter()
                .map(|parameter| ParameterSyntax::Rest { parameter }),
        )
}

pub(in crate::check::checker) fn parameter_count(params: &FormalParameters<'_>) -> usize {
    params.items.len() + usize::from(params.rest.is_some())
}

fn parameter_from_shape(
    name: impl Into<String>,
    ty: TypeId,
    optional: bool,
    has_default: bool,
) -> ParameterType {
    if has_default {
        ParameterType::defaulted(name, ty)
    } else if optional {
        ParameterType::optional(name, ty)
    } else {
        ParameterType::required(name, ty)
    }
}

/// Widen a type: a literal widens to its base intrinsic (`1` → `number`); every
/// other type passes through unchanged.
pub(in crate::check::checker) fn widen(interner: &mut Interner, ty: TypeId) -> TypeId {
    match interner.store().literal_value(ty) {
        Some(lit) => intrinsic_id(interner.well_known(), lit.base_kind()),
        None => ty,
    }
}

/// Well-known id for an intrinsic kind (small helper mirroring the relater's).
pub(in crate::check::checker) fn intrinsic_id(wk: WellKnown, kind: IntrinsicKind) -> TypeId {
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
        IntrinsicKind::Object => wk.object,
        IntrinsicKind::BigInt => wk.bigint,
        IntrinsicKind::Symbol => wk.symbol,
    }
}

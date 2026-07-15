//! calls module (extracted from checker/mod.rs).

use super::classes::application::{
    build_class_application, ClassApplicationKind, ClassApplicationRequest, ExplicitClassArgument,
    SourceClassArguments,
};
use super::classes::surface_types::SurfaceTypeFactory;
use super::context::*;
use super::decls::alloc_type_param_ids;
use super::decls::value_decl_id;
use super::eval::{contains_deferred_argument, contains_deferred_keyof};
use super::expr::contextual_literal_target;
use crate::binder::declaration::ValueStorageId;
use crate::binder::scope::ScopeId;
use crate::check::infer;
use crate::check::query::SemanticQueryCoordinator;
use crate::class_semantics::{
    ClassApplicationArguments, ClassConstructionState, DemandOutcome, Exhaustion,
};
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic};
use crate::relate::RelationOutcome;
use crate::span::Span;
use crate::types::repr::{
    FunctionType, GenericTypeParam, IntrinsicKind, ParameterType, TupleType, TypeParamId, TypeTag,
};
use crate::types::store::TypeId;
use crate::types::{instantiate_function, substitute, Interner, WellKnown};
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, CallExpression, Expression, FormalParameter,
    FormalParameterRest, FormalParameters, Function, FunctionBody, NewExpression,
    TSTypeParameterInstantiation,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

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

pub(in crate::check::checker) struct RetainedFunctionBodySurface {
    pub type_param_frame: FxHashMap<String, TypeId>,
    pub receiver: Option<TypeId>,
    pub params: Vec<Option<ParameterType>>,
    pub declared_return: Option<TypeId>,
    pub tickets: Option<super::lexical_events::CallableTickets>,
}

pub(in crate::check::checker) enum FunctionReservation {
    Ready(FunctionSurface),
    Unavailable(RetainedFunctionBodySurface),
}

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Run speculative candidate work against a child semantic-query state.
    /// Diagnostics and obligations travel in `CheckerEffects`; query memo/cache
    /// writes are discarded independently when the candidate is not decisive.
    fn capture_speculative_candidate_effects<R>(
        &mut self,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> (R, CheckerEffects) {
        #[cfg(test)]
        let parent_lengths = self.semantic_queries.durable_lengths();
        let child_queries = self.semantic_queries.fork();
        let parent_queries = std::mem::replace(&mut self.semantic_queries, child_queries);
        let captured = self.capture_candidate_effects(produce);
        #[cfg(test)]
        let child_queries = std::mem::replace(&mut self.semantic_queries, parent_queries);
        #[cfg(test)]
        self.measure_discarded_candidate_queries(parent_lengths, &child_queries);
        #[cfg(not(test))]
        {
            self.semantic_queries = parent_queries;
        }
        captured
    }

    /// Isolate trial-only relation/evaluation writes. A selected candidate is
    /// rebuilt once against the durable parent after the trial succeeds.
    fn with_speculative_candidate_queries<R>(&mut self, produce: impl FnOnce(&mut Self) -> R) -> R {
        #[cfg(test)]
        let parent_lengths = self.semantic_queries.durable_lengths();
        let child_queries = self.semantic_queries.fork();
        let parent_queries = std::mem::replace(&mut self.semantic_queries, child_queries);
        let result = produce(self);
        #[cfg(test)]
        let child_queries = std::mem::replace(&mut self.semantic_queries, parent_queries);
        #[cfg(test)]
        self.measure_discarded_candidate_queries(parent_lengths, &child_queries);
        #[cfg(not(test))]
        {
            self.semantic_queries = parent_queries;
        }
        result
    }

    #[cfg(test)]
    fn measure_discarded_candidate_queries(
        &self,
        parent_lengths: (usize, usize, usize),
        child_queries: &crate::check::query::SemanticQueryState,
    ) {
        let child_lengths = child_queries.durable_lengths();
        let discarded = child_lengths.0.saturating_sub(parent_lengths.0)
            + child_lengths.1.saturating_sub(parent_lengths.1)
            + child_lengths.2.saturating_sub(parent_lengths.2);
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
            self.effect_stack
                .last_mut()
                .expect("construction constraint check requires a lexical owner")
                .constraint_checks
                .push(ConstraintCheckObligation {
                    checks,
                    substitutions: map.clone(),
                });
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

    /// Check already-lowered type arguments against constraint sources. Signature
    /// default validation shares this with call-site explicit arguments.
    pub(in crate::check::checker) fn check_constraint_arguments(
        &mut self,
        args: &[(Option<TypeId>, TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) {
        if self.building_template {
            self.effect_stack
                .last_mut()
                .expect("construction constraint check requires a lexical owner")
                .constraint_checks
                .push(ConstraintCheckObligation {
                    checks: args.to_vec(),
                    substitutions: map.clone(),
                });
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
            // M28: a substituted constraint may be a pending computation (`K extends
            // keyof T` at `Pick<P, "q">` → `keyof P`) — resolve it through the shared
            // evaluator before relating, so the check runs against the VALUE
            // (`"a" | "b"`), driving the fixture's TK2344.
            let evaluated = match self.evaluate_type(substituted) {
                DemandOutcome::Ready(evaluated) => evaluated,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion)
                }
            };
            // M28: a substituted constraint still carrying deferred `keyof` cannot
            // be decided here; tsc lands that check at concrete instantiation.
            // Keyof only, so conditional/mapped constraints keep prior behavior.
            if contains_deferred_keyof(self.interner.store(), evaluated) {
                continue;
            }
            // M28: always evaluate the argument before checking. Decidable
            // compositions check precisely; still-deferred results check
            // conservatively (documented over-report for backlog 37 shapes).
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

        // Relate each argument to its constraint and render the failures under a single
        // immutable store borrow; push the diagnostics after it ends.
        let mut failures: Vec<(String, String, Span, Vec<String>)> = Vec::new();
        for (evaluated_arg, written_arg, constraint, span) in checks {
            match SemanticQueryCoordinator::new(
                self.interner,
                self.type_environment.published().classes(),
                &mut self.semantic_queries,
                &mut self.next_type_param,
            )
            .is_assignable(evaluated_arg, constraint)
            {
                RelationOutcome::Yes => {}
                RelationOutcome::No(chain) => {
                    let store = self.interner.store();
                    // Render the written argument when evaluation remains deferred;
                    // otherwise render the evaluated value, matching tsc-like output.
                    let render_id = if contains_deferred_argument(store, evaluated_arg) {
                        written_arg
                    } else {
                        evaluated_arg
                    };
                    let src = render_type(store, render_id, /* widen */ true);
                    let tgt = render_type(store, constraint, /* widen */ false);
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
        let (inferred_callee, call_receiver) = match callee {
            Expression::StaticMemberExpression(member) => {
                let inferred_receiver = self.infer_expr(scope, &member.object);
                let inferred_callee = match self.demand_class_value_surface(scope, &member.object) {
                    Some(DemandOutcome::Exhausted(exhaustion)) => {
                        self.own_type_demand(
                            DemandOutcome::Exhausted(exhaustion),
                            Span::from_oxc(member.property.span),
                        );
                        None
                    }
                    Some(DemandOutcome::Ready(())) | None => {
                        inferred_receiver.and_then(|(receiver, _)| {
                            self.infer_member_access_from_base(receiver, member)
                        })
                    }
                };
                (inferred_callee, inferred_receiver)
            }
            Expression::ComputedMemberExpression(member) => {
                let inferred_receiver = self.infer_expr(scope, &member.object);
                let inferred_callee = match self.demand_class_value_surface(scope, &member.object) {
                    Some(DemandOutcome::Exhausted(exhaustion)) => {
                        self.infer_expr(scope, &member.expression);
                        self.own_type_demand(
                            DemandOutcome::Exhausted(exhaustion),
                            Span::from_oxc(member.span),
                        );
                        None
                    }
                    Some(DemandOutcome::Ready(())) | None => {
                        inferred_receiver.and_then(|(receiver, _)| {
                            self.infer_element_access_from_base(scope, receiver, member)
                        })
                    }
                };
                (inferred_callee, inferred_receiver)
            }
            _ => (self.infer_expr(scope, &call.callee), None),
        };

        // Infer arguments up front and build `arg_fresh` in the same loop so M24
        // clamp provenance stays index-aligned with skipped out-of-subset args.
        let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(call.arguments.len());
        let mut arg_fresh: Vec<bool> = Vec::with_capacity(call.arguments.len());
        let mut arg_exprs: Vec<&Expression<'_>> = Vec::with_capacity(call.arguments.len());
        for arg in &call.arguments {
            if let Some(arg_expr) = arg.as_expression() {
                #[cfg(test)]
                measure_call(|measure| measure.raw_call_argument_walks += 1);
                if let Some(inferred) = self.infer_expr(scope, arg_expr) {
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
            return Some((wk.error, call_span));
        };
        let outcome = self.evaluate_type(callee_ty);
        let callee_ty = self.own_type_demand(outcome, call_span)?;
        let signatures = self.callable_signatures(callee_ty);
        if signatures.is_empty() {
            return Some((wk.error, call_span));
        }

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
    fn callable_signatures(&mut self, callee_ty: TypeId) -> Vec<TypeId> {
        let callee_ty = self.apparent_type(callee_ty);
        match self.interner.store().tag(callee_ty) {
            TypeTag::Function => vec![callee_ty],
            TypeTag::Object => {
                let Some(object) = self.interner.store().object_type(callee_ty) else {
                    return Vec::new();
                };
                object.call_signatures.clone()
            }
            _ => Vec::new(),
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
            }) {
                Ok(candidate) => DemandOutcome::Ready(Some(candidate)),
                Err(CandidateBuildFailure::Constraint)
                | Err(CandidateBuildFailure::Unavailable) => DemandOutcome::Ready(None),
                Err(CandidateBuildFailure::Exhausted(exhaustion)) => {
                    DemandOutcome::Exhausted(exhaustion)
                }
            };
        }

        let mut arity_failures: Vec<CallArity> = Vec::new();
        let mut saw_non_arity_failure = false;
        let mut first_constraint_failure: Option<CheckerEffects> = None;
        let mut first_other_failure: Option<CheckerEffects> = None;

        for signature in signatures {
            let (built, effects) = self.capture_speculative_candidate_effects(|pass| {
                pass.instantiate_signature_candidate(SignatureCandidateRequest {
                    scope,
                    signature_ty: *signature,
                    type_arguments,
                    args,
                    call_receiver: receiver.inference_source(),
                    commit_constraints: false,
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

        let targets = self.call_argument_targets(params, arg_types.len());
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
            match SemanticQueryCoordinator::new(
                self.interner,
                self.type_environment.published().classes(),
                &mut self.semantic_queries,
                &mut self.next_type_param,
            )
            .is_assignable(src, param_ty)
            {
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
        let wk = self.interner.well_known();

        // Infer every argument up front (skipping spreads — out of subset); this descends
        // into nested calls/`new`/functions inside the arguments.
        let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(call.arguments.len());
        let mut arg_exprs: Vec<&Expression<'_>> = Vec::with_capacity(call.arguments.len());
        for arg in &call.arguments {
            if let Some(arg_expr) = arg.as_expression() {
                if let Some(inferred) = self.infer_expr(scope, arg_expr) {
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
        let targets = self.call_argument_targets(params, arg_types.len());
        for (((arg_ty, arg_span), arg_expr), param_ty) in
            arg_types.iter().zip(arg_exprs).zip(targets)
        {
            let Some(param_ty) = param_ty else {
                continue;
            };
            let (src, src_span) = contextual_source_after_walked!(
                self,
                scope,
                arg_expr,
                param_ty,
                (*arg_ty, *arg_span),
                true,
                true,
                ContextualMeasurePhase::CommittedCheck
            );
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
                kind: self.call_argument_obligation_kind(arg_expr, param_ty),
            });
            // A `never` parameter makes this call candidate impossible. Later
            // contextual callback targets are recovery artifacts of that failure,
            // not independent argument errors.
            if param_ty == never {
                break;
            }
        }
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
        match SemanticQueryCoordinator::new(
            self.interner,
            self.type_environment.published().classes(),
            &mut self.semantic_queries,
            &mut self.next_type_param,
        )
        .is_assignable(source_receiver, target_receiver)
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
        SemanticQueryCoordinator::new(
            self.interner,
            self.type_environment.published().classes(),
            &mut self.semantic_queries,
            &mut self.next_type_param,
        )
        .is_assignable(source_receiver, target_receiver)
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
            return SemanticQueryCoordinator::new(
                self.interner,
                self.type_environment.published().classes(),
                &mut self.semantic_queries,
                &mut self.next_type_param,
            )
            .normalize_class_application(ty);
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
        if let Some(shape) = self.rest_call_shape(rest_ty) {
            return RestArity {
                min: shape.min_len(),
                max: shape.max_len(),
            };
        }
        RestArity { min: 0, max: None }
    }

    fn call_argument_targets(
        &self,
        params: &[ParameterType],
        arg_count: usize,
    ) -> Vec<Option<TypeId>> {
        let fixed: Vec<&ParameterType> = params.iter().filter(|param| param.is_fixed()).collect();
        let rest = params.iter().find(|param| param.rest);
        let total_rest_args = arg_count.saturating_sub(fixed.len());
        (0..arg_count)
            .map(|index| {
                if let Some(param) = fixed.get(index) {
                    return Some(param.ty);
                }
                let rest = rest?;
                self.rest_argument_target(rest.ty, index - fixed.len(), total_rest_args)
            })
            .collect()
    }

    /// Contextual arrows use the same positional/rest expansion as calls, so an
    /// arrow against `(...args: [A, B]) => R` receives `A` then `B` bindings.
    pub(in crate::check::checker) fn contextual_parameter_target(
        &self,
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
        self.rest_call_shape(rest_ty)?
            .element_at(offset, total_rest_args)
    }

    fn rest_call_shape(&self, rest_ty: TypeId) -> Option<RestCallShape> {
        let rest_ty = self
            .interner
            .store()
            .readonly_operand(rest_ty)
            .unwrap_or(rest_ty);
        if let Some(array) = self.interner.store().array_type(rest_ty) {
            return Some(RestCallShape {
                prefix: Vec::new(),
                variadic: Some(array.element),
                suffix: Vec::new(),
            });
        }
        if let Some(tuple) = self.interner.store().tuple_type(rest_ty) {
            return self.tuple_call_shape(tuple);
        }
        Some(RestCallShape {
            prefix: Vec::new(),
            variadic: Some(rest_ty),
            suffix: Vec::new(),
        })
    }

    fn tuple_call_shape(&self, tuple: &TupleType) -> Option<RestCallShape> {
        let Some(rest) = tuple.rest else {
            return Some(RestCallShape {
                prefix: tuple.elements.clone(),
                variadic: None,
                suffix: Vec::new(),
            });
        };
        if rest.position > tuple.elements.len() {
            return None;
        }
        let mut prefix = tuple.elements[..rest.position].to_vec();
        let suffix = tuple.elements[rest.position..].to_vec();
        let rest_shape = self.rest_call_shape(rest.ty)?;
        prefix.extend(rest_shape.prefix);
        let mut combined_suffix = rest_shape.suffix;
        combined_suffix.extend(suffix);
        Some(RestCallShape {
            prefix,
            variadic: rest_shape.variadic,
            suffix: combined_suffix,
        })
    }

    /// Return the contextual type for a tuple-literal position, including a represented
    /// rest segment and its trailing fixed suffix.
    pub(in crate::check::checker) fn tuple_context_element(
        &self,
        tuple: &TupleType,
        index: usize,
        total_elements: usize,
    ) -> Option<TypeId> {
        self.tuple_call_shape(tuple)?
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
        let wk = self.interner.well_known();
        let new_span = Span::from_oxc(new_expr.span);

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
        for arg in &new_expr.arguments {
            if let Some(arg_expr) = arg.as_expression() {
                #[cfg(test)]
                measure_call(|measure| measure.raw_construct_argument_walks += 1);
                if let Some(inferred) = self.infer_expr(scope, arg_expr) {
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
        let Some((decl_id, info)) = class_resolved else {
            if let Some((callee_ty, _)) = inferred_callee {
                let signatures = self.construct_signatures(callee_ty);
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
            decl_id,
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
    ) -> DemandOutcome<Option<(ValueStorageId, ClassInfo)>> {
        let callee = match callee {
            Expression::ParenthesizedExpression(paren) => {
                return self.class_new_target(scope, &paren.expression)
            }
            Expression::Identifier(ident) => ident,
            _ => return DemandOutcome::Ready(None),
        };
        let Some(value_decl) = value_decl_id(self.binder, scope, callee.name.as_str()) else {
            return DemandOutcome::Ready(None);
        };
        let class_decl = self
            .class_value_aliases
            .get(&value_decl)
            .copied()
            .unwrap_or(value_decl);
        let Some(binding) = self
            .lexical_events
            .classes()
            .iter()
            .filter_map(|reservation| reservation.binding.as_ref())
            .find(|binding| binding.value_decl == Some(class_decl))
            .cloned()
        else {
            return DemandOutcome::Ready(None);
        };
        if value_decl != class_decl && !binding.header_type_params.is_empty() {
            return DemandOutcome::Ready(None);
        }
        let surface = match self
            .type_environment
            .published()
            .classes()
            .published_class(binding.class_id)
        {
            DemandOutcome::Ready(surface) => surface,
            DemandOutcome::Exhausted(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
        };
        let Some(ctor) = surface.constructor_template() else {
            return DemandOutcome::Ready(None);
        };
        let metadata = self
            .class_new_metadata
            .get(&binding.class_id)
            .copied()
            .expect("every published source class freezes its new metadata");
        DemandOutcome::Ready(Some((
            class_decl,
            ClassInfo {
                ctor,
                class_id: binding.class_id,
                is_abstract: metadata.is_abstract,
                ctor_visibility: metadata.ctor_visibility,
                ctor_declaring_class: metadata.ctor_declaring_class,
            },
        )))
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
        let surface = match self
            .type_environment
            .published()
            .classes()
            .published_class(info.class_id)
        {
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
    fn construct_signatures(&mut self, callee_ty: TypeId) -> Vec<TypeId> {
        let callee_ty = self.apparent_type(callee_ty);
        if self.interner.store().tag(callee_ty) != TypeTag::Object {
            return Vec::new();
        }
        let Some(object) = self.interner.store().object_type(callee_ty) else {
            return Vec::new();
        };
        object.construct_signatures.clone()
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
        _decl_id: ValueStorageId,
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
    ) -> FunctionReservation {
        let retained = self
            .lexical_events
            .callable_at(self.current_module_ordinal, func.span.start)
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
                            pass.lower_annotation(enclosing, &annotation.type_annotation)
                        })
                        .map(Some),
                    None => Some(None),
                };
                let params = pass.lower_parameter_slots(enclosing, fn_scope, &func.params, false);
                // Type references in the signature resolve from the enclosing scope,
                // while declared type parameters resolve through the pushed frame.
                let declared_return = match func.return_type.as_ref() {
                    Some(annotation) => pass
                        .lower_annotation(enclosing, &annotation.type_annotation)
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
        surface: &FunctionSurface,
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
        surface: &FunctionSurface,
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
        surface: &RetainedFunctionBodySurface,
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
            .callable_at(self.current_module_ordinal, func.span.start)
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
            .callable_at(self.current_module_ordinal, arrow.span.start)
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
                let declared_ret = arrow
                    .return_type
                    .as_ref()
                    .and_then(|ann| pass.lower_annotation(enclosing, &ann.type_annotation));
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
        let context = self.interner.store().function_type(context).cloned()?;
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
                    self.decl_types.set(decl_id, ty);
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
                    let annotation_ty = parameter
                        .type_annotation
                        .as_ref()
                        .and_then(|ann| self.lower_annotation(enclosing, &ann.type_annotation));
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
                            self.check_annotated_initializer(
                                parameter_scope,
                                Some(annotation_ty),
                                init,
                            );
                        }
                    }
                    ty
                }
                ParameterSyntax::Rest { parameter } => match parameter.type_annotation.as_ref() {
                    Some(ann) => self.lower_annotation(enclosing, &ann.type_annotation),
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
                    self.decl_types.set(decl_id, parameter.ty);
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
                    self.check_annotated_initializer(scope, Some(parameter.ty), init);
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
            self.decl_types.set(decl_id, lowered.ty);
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
            self.decl_types.set(decl_id, lowered.ty);
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
            self.decl_types.set(decl_id, lowered.ty);
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
            self.decl_types.set(decl_id, lowered.ty);
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
                self.check_annotated_initializer(
                    scope,
                    lowered.as_ref().map(|lowered| lowered.ty),
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
}

struct CallCandidate {
    receiver: Option<TypeId>,
    params: Vec<ParameterType>,
    ret: TypeId,
    inference_exhaustion: Option<Exhaustion>,
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
}

enum CandidateTrial {
    Match,
    Arity(CallArity),
    Mismatch,
    Exhausted(Exhaustion),
}

enum CandidateBuildFailure {
    Constraint,
    Exhausted(Exhaustion),
    Unavailable,
}

#[derive(Clone)]
struct CallArity {
    min: usize,
    max: Option<usize>,
    unbounded_rest: bool,
}

struct RestArity {
    min: usize,
    max: Option<usize>,
}

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
    }
}

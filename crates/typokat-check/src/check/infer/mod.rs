//! Type-argument inference (M10; architecture §5.1).
//! Produces type-parameter bindings from call arguments; the relation engine still
//! decides assignability after substitution. Call-site candidates keep their source
//! contribution so incompatible arguments cannot be accepted by unioning every
//! source into a too-wide target. No candidate fixes to `unknown`; no candidate
//! still falls back to the parameter constraint or `unknown`. The `(source, target)`
//! guard makes recursive matching terminate.

mod context;
#[cfg(test)]
mod derivation_identity_spec;
mod helpers;
#[cfg(test)]
mod tests;

use context::InferenceContext;
use helpers::widen;

use crate::check::query::{PublishedClassLookup, SemanticQueryCoordinator, SemanticQueryState};
use crate::class_semantics::{DemandOutcome, Exhaustion, PublishedClasses};
use crate::relate::{RelationDemand, RelationNormalization, RelationOutcome};
use crate::types::repr::{
    GenericTypeParam, IntrinsicKind, ParameterType, PropertyKey, TypeParamId, TypeTag,
};
use crate::types::store::{Store, TypeId};
use crate::types::{substitute, DerivedType, Interner};
use rustc_hash::{FxHashMap, FxHashSet};

#[cfg(test)]
thread_local! {
    static QUERY_INFERENCE_ATTEMPTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_query_inference_attempts() {
    QUERY_INFERENCE_ATTEMPTS.with(|attempts| attempts.set(0));
}

#[cfg(test)]
pub(crate) fn query_inference_attempts() -> u64 {
    QUERY_INFERENCE_ATTEMPTS.with(std::cell::Cell::get)
}

/// The raw candidates per type parameter for conditional-`infer` mode. Call-site
/// inference uses [`CallSiteCandidates`] so it can keep source provenance separate
/// from conditional same-name union behavior.
pub type Candidates = FxHashMap<crate::types::repr::TypeParamId, Vec<TypeId>>;

pub(crate) enum InferenceAttempt {
    Complete(Candidates),
    Needs(RelationDemand),
    NeedsBatch(Vec<RelationDemand>),
    Exhausted(Exhaustion),
}

#[derive(Default)]
pub(crate) struct InferenceRetryState {
    demands: FxHashSet<RelationDemand>,
}

impl InferenceRetryState {
    pub(crate) fn observe(&mut self, attempt: InferenceAttempt) -> InferenceAttempt {
        let demands = match attempt {
            InferenceAttempt::Needs(demand) => vec![demand],
            InferenceAttempt::NeedsBatch(demands) => demands,
            attempt => return attempt,
        };
        let mut pending = demands
            .iter()
            .copied()
            .filter(|demand| self.demands.insert(*demand))
            .collect::<Vec<_>>();
        match pending.len() {
            1 => {
                if let Some(demand) = pending.pop() {
                    return InferenceAttempt::Needs(demand);
                }
            }
            2.. => return InferenceAttempt::NeedsBatch(std::mem::take(&mut pending)),
            _ => {}
        }
        let Some(demand) = demands.first().copied() else {
            return InferenceAttempt::Exhausted(Exhaustion::EvaluationBudget);
        };
        let exhaustion = match demand {
            RelationDemand::Evaluation(ty) => Exhaustion::EvaluationCycle { ty },
            RelationDemand::DerivedEvaluation(derived) => {
                Exhaustion::EvaluationCycle { ty: derived.ty }
            }
            RelationDemand::ClassProjection(_)
            | RelationDemand::DerivedClassProjection(_)
            | RelationDemand::ApparentSurface(_) => Exhaustion::ClassProjectionBudget,
        };
        InferenceAttempt::Exhausted(exhaustion)
    }
}

type CallSiteCandidates = FxHashMap<crate::types::repr::TypeParamId, Vec<CallSiteCandidate>>;

pub(crate) struct SignatureInferenceRequest<'a> {
    pub(crate) type_params: &'a [GenericTypeParam],
    pub(crate) params: &'a [ParameterType],
    pub(crate) args: &'a [TypeId],
    pub(crate) fresh_args: &'a [bool],
    pub(crate) receiver: Option<(TypeId, TypeId)>,
}

struct CandidateCollectionRequest<'a> {
    active_params: Option<&'a FxHashSet<TypeParamId>>,
    params: &'a [ParameterType],
    args: &'a [TypeId],
    fresh_args: &'a [bool],
    receiver: Option<(TypeId, TypeId)>,
}

struct ReturnContextInferenceRequest<'a> {
    type_params: &'a [GenericTypeParam],
    active_params: &'a FxHashSet<TypeParamId>,
    ordinary_bound: &'a FxHashSet<TypeParamId>,
    constraints: &'a FxHashMap<TypeParamId, Option<TypeId>>,
    baseline: FixedSignatureParams,
    source: TypeId,
    target: TypeId,
}

pub(crate) struct SignatureInferenceResult<T> {
    pub(crate) arguments: T,
    pub(crate) exhaustion: Option<Exhaustion>,
    pub(crate) constraint_violations: FxHashSet<TypeParamId>,
}

struct FixedSignatureParams {
    arguments: FxHashMap<TypeParamId, TypeId>,
    constraint_violations: FxHashSet<TypeParamId>,
}

struct CandidateCollectionResult {
    candidates: CallSiteCandidates,
    exhaustion: Option<Exhaustion>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct CallSiteCandidate {
    ty: TypeId,
    source: CallSiteSource,
    fresh: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CallSiteSource {
    Argument {
        index: usize,
        occurrence: usize,
    },
    DirectRest {
        start: usize,
        occurrence: usize,
    },
    /// The explicit `this` receiver is a semantic inference input, never a
    /// synthetic positional argument.
    Receiver {
        occurrence: usize,
    },
    /// Lower-priority evidence from the expected type of the call expression.
    ReturnContext {
        occurrence: usize,
    },
}

struct CandidateContribution {
    source: CallSiteSource,
    fresh: bool,
    candidates: Vec<TypeId>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum PrimitiveFamily {
    String,
    Number,
    Boolean,
    Null,
    Undefined,
    Void,
}

/// Structurally match in conditional-`infer` mode (M25). Unlike call-site
/// inference, union targets descend into members, template patterns capture
/// `infer` holes, and candidates are never widened.
pub fn infer_from_types_for_conditional(
    interner: &mut Interner,
    source: TypeId,
    target: TypeId,
    candidates: &mut Candidates,
) {
    let mut ctx = InferenceContext::for_conditional();
    ctx.infer(interner, source, target, candidates);
}

/// Query-local structural inference. Candidates are committed only when every
/// demanded normalization succeeds; an exhausted attempt contributes nothing.
#[cfg(test)]
pub(crate) fn infer_from_types_for_query(
    interner: &mut Interner,
    source: TypeId,
    target: TypeId,
    normalization: &dyn RelationNormalization,
) -> InferenceAttempt {
    infer_from_types_for_query_with_params(interner, source, target, normalization, None)
}

pub(crate) fn infer_from_derived_types_for_query_with_params(
    interner: &mut Interner,
    source: DerivedType,
    target: DerivedType,
    normalization: &dyn RelationNormalization,
    active_params: Option<&FxHashSet<TypeParamId>>,
) -> InferenceAttempt {
    #[cfg(test)]
    QUERY_INFERENCE_ATTEMPTS.with(|attempts| attempts.set(attempts.get().saturating_add(1)));
    let mut local = Candidates::default();
    let mut context = InferenceContext::for_query(normalization, active_params);
    context.infer_derived(interner, source, target, &mut local);
    if let Some(reason) = context.take_exhaustion() {
        return InferenceAttempt::Exhausted(reason);
    }
    let demands = context.take_demands();
    if let [demand] = demands.as_slice() {
        return InferenceAttempt::Needs(*demand);
    }
    if !demands.is_empty() {
        return InferenceAttempt::NeedsBatch(demands);
    }
    InferenceAttempt::Complete(local)
}

#[cfg(test)]
fn infer_from_derived_types_for_query(
    interner: &mut Interner,
    source: DerivedType,
    target: DerivedType,
    normalization: &dyn RelationNormalization,
) -> InferenceAttempt {
    infer_from_derived_types_for_query_with_params(interner, source, target, normalization, None)
}

#[cfg(test)]
pub(crate) fn infer_from_types_for_query_with_params(
    interner: &mut Interner,
    source: TypeId,
    target: TypeId,
    normalization: &dyn RelationNormalization,
    active_params: Option<&FxHashSet<TypeParamId>>,
) -> InferenceAttempt {
    let mut local = Candidates::default();
    let mut context = InferenceContext::for_query(normalization, active_params);
    context.infer(interner, source, target, &mut local);
    if let Some(reason) = context.take_exhaustion() {
        return InferenceAttempt::Exhausted(reason);
    }
    let demands = context.take_demands();
    if let [demand] = demands.as_slice() {
        return InferenceAttempt::Needs(*demand);
    }
    if !demands.is_empty() {
        return InferenceAttempt::NeedsBatch(demands);
    }
    InferenceAttempt::Complete(local)
}

/// Infer a generic function signature's arguments from the call arguments.
///
/// Function binders own their constraints/defaults, so this intentionally reads
/// the persistent descriptors rather than the store's declaration-side column.
/// That keeps an outer-substituted member signature sound at its later call site.
pub(crate) fn infer_signature_type_arguments_from_params(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    request: SignatureInferenceRequest<'_>,
) -> DemandOutcome<SignatureInferenceResult<FxHashMap<TypeParamId, TypeId>>> {
    infer_signature_type_arguments(interner, next_type_param, published, queries, request, None)
}

/// Infer a generic signature from its ordinary call inputs plus the call
/// expression's contextual result type. Argument and receiver candidates have
/// higher priority and replace contextual-return candidates for the same binder.
pub(crate) fn infer_signature_type_arguments_with_return_context(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    request: SignatureInferenceRequest<'_>,
    return_context: (TypeId, TypeId),
) -> DemandOutcome<SignatureInferenceResult<FxHashMap<TypeParamId, TypeId>>> {
    infer_signature_type_arguments(
        interner,
        next_type_param,
        published,
        queries,
        request,
        Some(return_context),
    )
}

fn infer_signature_type_arguments(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    request: SignatureInferenceRequest<'_>,
    return_context: Option<(TypeId, TypeId)>,
) -> DemandOutcome<SignatureInferenceResult<FxHashMap<TypeParamId, TypeId>>> {
    let SignatureInferenceRequest {
        type_params,
        params,
        args,
        fresh_args,
        receiver,
    } = request;
    let active_params: FxHashSet<_> = type_params.iter().map(|param| param.id).collect();
    queries.savepoint();
    let collection = collect_call_site_candidates_query(
        interner,
        next_type_param,
        published,
        queries,
        CandidateCollectionRequest {
            active_params: Some(&active_params),
            params,
            args,
            fresh_args,
            receiver,
        },
    );
    let candidates = collection.candidates;
    let exempt = fresh_exempt_params(&candidates);
    let ordinary_bound: FxHashSet<_> = candidates.keys().copied().collect();
    let constraints: FxHashMap<TypeParamId, Option<TypeId>> = type_params
        .iter()
        .map(|param| (param.id, param.constraint))
        .collect();
    let fixed = match fix_call_site_candidates(
        interner,
        next_type_param,
        published,
        queries,
        candidates,
        &constraints,
    ) {
        DemandOutcome::Ready(fixed) => fixed,
        DemandOutcome::Exhausted(exhaustion) => {
            queries.rollback();
            return DemandOutcome::Exhausted(exhaustion);
        }
    };
    let outcome = fix_signature_params(
        interner,
        next_type_param,
        published,
        queries,
        type_params,
        fixed,
        &exempt,
    );
    let ordinary_complete = collection.exhaustion.is_none();
    let outcome = match (outcome, return_context, ordinary_complete) {
        (DemandOutcome::Ready(fixed), Some((source, target)), true) => {
            DemandOutcome::Ready(apply_return_context_candidates(
                interner,
                next_type_param,
                published,
                queries,
                ReturnContextInferenceRequest {
                    type_params,
                    active_params: &active_params,
                    ordinary_bound: &ordinary_bound,
                    constraints: &constraints,
                    baseline: fixed,
                    source,
                    target,
                },
            ))
        }
        (outcome, _, _) => outcome,
    };
    if matches!(outcome, DemandOutcome::Ready(_)) && collection.exhaustion.is_none() {
        queries.commit();
    } else {
        queries.rollback();
    }
    match outcome {
        DemandOutcome::Ready(fixed) => DemandOutcome::Ready(SignatureInferenceResult {
            arguments: fixed.arguments,
            exhaustion: collection.exhaustion,
            constraint_violations: fixed.constraint_violations,
        }),
        DemandOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
    }
}

fn apply_return_context_candidates(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    request: ReturnContextInferenceRequest<'_>,
) -> FixedSignatureParams {
    let contextual_active: FxHashSet<_> = request
        .active_params
        .difference(request.ordinary_bound)
        .copied()
        .collect();
    if contextual_active.is_empty() {
        return request.baseline;
    }

    // Contextual-result evidence is optional and atomic. No collection, fixing,
    // evaluation, or relation failure may affect overload applicability.
    queries.savepoint();
    let contextual = collect_return_context_candidates_query(
        interner,
        next_type_param,
        published,
        queries,
        &contextual_active,
        request.source,
        request.target,
    );
    if contextual.exhaustion.is_some() || contextual.candidates.is_empty() {
        queries.rollback();
        return request.baseline;
    }
    let contextual = match fix_call_site_candidates(
        interner,
        next_type_param,
        published,
        queries,
        contextual.candidates,
        request.constraints,
    ) {
        DemandOutcome::Ready(contextual) if !contextual.is_empty() => contextual,
        DemandOutcome::Ready(_) | DemandOutcome::Exhausted(_) => {
            queries.rollback();
            return request.baseline;
        }
    };

    let baseline = &request.baseline.arguments;
    let mut proposal = baseline.clone();
    proposal.extend(
        contextual
            .iter()
            .map(|(&param, &candidate)| (param, candidate)),
    );

    // Defaults and constraint fallbacks can depend transitively on contextual
    // binders. Recompute the whole non-authoritative layer simultaneously until
    // it stabilizes; ordinary and contextual binders remain authoritative.
    let mut stabilized = false;
    for _ in 0..=request.type_params.len() {
        let mut next = proposal.clone();
        for type_param in request.type_params {
            let param = type_param.id;
            let value = if request.ordinary_bound.contains(&param) {
                baseline.get(&param).copied()
            } else if let Some(&candidate) = contextual.get(&param) {
                substitute_to_fixed_point(interner, candidate, &proposal)
            } else if let Some(default) = type_param.default {
                let Some(baseline_default) = substitute_to_fixed_point(interner, default, baseline)
                else {
                    queries.rollback();
                    return request.baseline;
                };
                let Some(proposed_default) =
                    substitute_to_fixed_point(interner, default, &proposal)
                else {
                    queries.rollback();
                    return request.baseline;
                };
                if proposed_default == baseline_default {
                    baseline.get(&param).copied()
                } else {
                    Some(proposed_default)
                }
            } else if let Some(constraint) = type_param.constraint {
                let Some(baseline_constraint) =
                    substitute_to_fixed_point(interner, constraint, baseline)
                else {
                    queries.rollback();
                    return request.baseline;
                };
                let Some(proposed_constraint) =
                    substitute_to_fixed_point(interner, constraint, &proposal)
                else {
                    queries.rollback();
                    return request.baseline;
                };
                if proposed_constraint == baseline_constraint {
                    baseline.get(&param).copied()
                } else {
                    match evaluate_return_context_constraint(
                        interner,
                        next_type_param,
                        published,
                        queries,
                        proposed_constraint,
                    ) {
                        Some(evaluated) => Some(evaluated),
                        None => {
                            queries.rollback();
                            return request.baseline;
                        }
                    }
                }
            } else {
                baseline.get(&param).copied()
            };
            let Some(value) = value else {
                queries.rollback();
                return request.baseline;
            };
            next.insert(param, value);
        }
        if next == proposal {
            stabilized = true;
            break;
        }
        proposal = next;
    }
    if !stabilized {
        queries.rollback();
        return request.baseline;
    }

    // Validate every constraint whose binder or substituted boundary changed.
    // This includes ordinary binders when a contextual later binder completes a
    // previously unresolved forward constraint.
    for type_param in request.type_params {
        let Some(raw_constraint) = type_param.constraint else {
            continue;
        };
        let Some(baseline_constraint) =
            substitute_to_fixed_point(interner, raw_constraint, baseline)
        else {
            queries.rollback();
            return request.baseline;
        };
        let Some(proposed_constraint) =
            substitute_to_fixed_point(interner, raw_constraint, &proposal)
        else {
            queries.rollback();
            return request.baseline;
        };
        let baseline_value = baseline.get(&type_param.id).copied();
        let proposed_value = proposal.get(&type_param.id).copied();
        let affected = contextual.contains_key(&type_param.id)
            || proposed_value != baseline_value
            || proposed_constraint != baseline_constraint;
        if !affected {
            continue;
        }
        let Some(candidate) = proposed_value else {
            queries.rollback();
            return request.baseline;
        };
        let Some(evaluated) = evaluate_return_context_constraint(
            interner,
            next_type_param,
            published,
            queries,
            proposed_constraint,
        ) else {
            queries.rollback();
            return request.baseline;
        };
        let relation = {
            let mut coordinator =
                SemanticQueryCoordinator::new(interner, published, queries, next_type_param);
            coordinator.is_assignable(candidate, evaluated)
        };
        match relation {
            RelationOutcome::Yes => {}
            RelationOutcome::No(_) | RelationOutcome::Exhausted(_) => {
                queries.rollback();
                return request.baseline;
            }
        }
    }

    queries.commit();
    FixedSignatureParams {
        arguments: proposal,
        constraint_violations: request.baseline.constraint_violations,
    }
}

fn evaluate_return_context_constraint(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    constraint: TypeId,
) -> Option<TypeId> {
    let demand = {
        let mut coordinator =
            SemanticQueryCoordinator::new(interner, published, queries, next_type_param);
        coordinator.demand(constraint)
    };
    let DemandOutcome::Ready(evaluated) = demand else {
        return None;
    };
    (!crate::check::checker::eval::contains_deferred_keyof(interner, evaluated))
        .then_some(evaluated)
}

fn substitute_to_fixed_point(
    interner: &mut Interner,
    ty: TypeId,
    map: &FxHashMap<TypeParamId, TypeId>,
) -> Option<TypeId> {
    let mut current = ty;
    for _ in 0..=map.len() {
        let next = substitute(interner, current, map);
        if next == current {
            return Some(current);
        }
        current = next;
    }
    (substitute(interner, current, map) == current).then_some(current)
}

fn collect_return_context_candidates_query(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    active_params: &FxHashSet<TypeParamId>,
    source: TypeId,
    target: TypeId,
) -> CandidateCollectionResult {
    let source = contextual_return_inference_source(interner.store(), source, target);
    let mut local = Candidates::default();
    let outcome = infer_types_for_active_params(
        SemanticQueryCoordinator::new(interner, published, queries, next_type_param),
        source,
        target,
        &mut local,
        Some(active_params),
    );
    let mut candidates = CallSiteCandidates::default();
    if matches!(outcome, DemandOutcome::Ready(())) {
        record_call_site_candidates(
            &mut candidates,
            local,
            |occurrence| CallSiteSource::ReturnContext { occurrence },
            false,
        );
    }
    CandidateCollectionResult {
        candidates,
        exhaustion: match outcome {
            DemandOutcome::Ready(()) => None,
            DemandOutcome::Exhausted(exhaustion) => Some(exhaustion),
        },
    }
}

fn contextual_return_inference_source(store: &Store, source: TypeId, target: TypeId) -> TypeId {
    let Some(operand) = store.readonly_operand(source) else {
        return source;
    };
    let source_tag = store.tag(operand);
    let target_tag = store.tag(target);
    if matches!(source_tag, TypeTag::Array | TypeTag::Tuple)
        && matches!(target_tag, TypeTag::Array | TypeTag::Tuple)
    {
        operand
    } else {
        source
    }
}

/// Infer only binders that received a real call-site candidate, in declaration order.
/// Defaults and missing-binder fallbacks belong to the caller's application layer.
pub(crate) fn infer_partial_signature_type_arguments_from_params(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    request: SignatureInferenceRequest<'_>,
) -> DemandOutcome<SignatureInferenceResult<Vec<(TypeParamId, TypeId)>>> {
    let SignatureInferenceRequest {
        type_params,
        params,
        args,
        fresh_args,
        receiver,
    } = request;
    let active_params: FxHashSet<_> = type_params.iter().map(|param| param.id).collect();
    queries.savepoint();
    let collection = collect_call_site_candidates_query(
        interner,
        next_type_param,
        published,
        queries,
        CandidateCollectionRequest {
            active_params: Some(&active_params),
            params,
            args,
            fresh_args,
            receiver,
        },
    );
    let candidates = collection.candidates;
    let exempt = fresh_exempt_params(&candidates);
    let constraints: FxHashMap<TypeParamId, Option<TypeId>> = type_params
        .iter()
        .map(|param| (param.id, param.constraint))
        .collect();
    let fixed = match fix_call_site_candidates(
        interner,
        next_type_param,
        published,
        queries,
        candidates,
        &constraints,
    ) {
        DemandOutcome::Ready(fixed) => fixed,
        DemandOutcome::Exhausted(exhaustion) => {
            queries.rollback();
            return DemandOutcome::Exhausted(exhaustion);
        }
    };
    let outcome = fix_present_signature_params(
        interner,
        next_type_param,
        published,
        queries,
        type_params,
        fixed,
        &exempt,
    );
    if matches!(outcome, DemandOutcome::Ready(_)) && collection.exhaustion.is_none() {
        queries.commit();
    } else {
        queries.rollback();
    }
    match outcome {
        DemandOutcome::Ready(arguments) => DemandOutcome::Ready(SignatureInferenceResult {
            arguments,
            exhaustion: collection.exhaustion,
            constraint_violations: FxHashSet::default(),
        }),
        DemandOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
    }
}

fn collect_call_site_candidates_query(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    request: CandidateCollectionRequest<'_>,
) -> CandidateCollectionResult {
    let CandidateCollectionRequest {
        active_params,
        params,
        args,
        fresh_args,
        receiver,
    } = request;
    let mut candidates: CallSiteCandidates = FxHashMap::default();
    let mut first_exhaustion = None;
    let targets = inference_argument_targets(interner, params, args);
    for (index, (&arg, param)) in args.iter().zip(&targets).enumerate() {
        let Some(param) = *param else {
            continue;
        };
        let mut local: Candidates = FxHashMap::default();
        // Keep candidates raw here; primitive-constrained parameters decide literal
        // preservation at fix time.
        let outcome = infer_types_for_active_params(
            SemanticQueryCoordinator::new(interner, published, queries, next_type_param),
            arg,
            param,
            &mut local,
            active_params,
        );
        if let DemandOutcome::Exhausted(exhaustion) = outcome {
            first_exhaustion.get_or_insert(exhaustion);
            continue;
        }
        let is_fresh = fresh_args.get(index).copied().unwrap_or(false);
        record_call_site_candidates(
            &mut candidates,
            local,
            |occurrence| CallSiteSource::Argument { index, occurrence },
            is_fresh,
        );
    }
    if let Some((rest_ty, rest_start)) = direct_rest_type_param(interner, params) {
        if rest_start <= args.len() {
            let tuple = interner.intern_tuple(args[rest_start..].to_vec());
            let mut local: Candidates = FxHashMap::default();
            let outcome = infer_types_for_active_params(
                SemanticQueryCoordinator::new(interner, published, queries, next_type_param),
                tuple,
                rest_ty,
                &mut local,
                active_params,
            );
            if let DemandOutcome::Exhausted(exhaustion) = outcome {
                first_exhaustion.get_or_insert(exhaustion);
            } else {
                let is_fresh = (rest_start..args.len())
                    .all(|index| fresh_args.get(index).copied().unwrap_or(false));
                record_call_site_candidates(
                    &mut candidates,
                    local,
                    |occurrence| CallSiteSource::DirectRest {
                        start: rest_start,
                        occurrence,
                    },
                    is_fresh,
                );
            }
        }
    }
    if let Some((source, target)) = receiver {
        let mut local: Candidates = FxHashMap::default();
        let outcome = infer_types_for_active_params(
            SemanticQueryCoordinator::new(interner, published, queries, next_type_param),
            source,
            target,
            &mut local,
            active_params,
        );
        if let DemandOutcome::Exhausted(exhaustion) = outcome {
            first_exhaustion.get_or_insert(exhaustion);
        } else {
            record_call_site_candidates(
                &mut candidates,
                local,
                |occurrence| CallSiteSource::Receiver { occurrence },
                false,
            );
        }
    }
    CandidateCollectionResult {
        candidates,
        exhaustion: first_exhaustion,
    }
}

fn infer_types_for_active_params<L: PublishedClassLookup + ?Sized>(
    mut coordinator: SemanticQueryCoordinator<'_, L>,
    source: TypeId,
    target: TypeId,
    candidates: &mut Candidates,
    active_params: Option<&FxHashSet<TypeParamId>>,
) -> DemandOutcome<()> {
    match active_params {
        Some(active_params) => {
            coordinator.infer_types_for_params(source, target, candidates, active_params)
        }
        None => coordinator.infer_types(source, target, candidates),
    }
}

#[cfg(test)]
fn collect_call_site_candidates(
    interner: &mut Interner,
    params: &[ParameterType],
    args: &[TypeId],
    fresh_args: &[bool],
    receiver: Option<(TypeId, TypeId)>,
) -> CallSiteCandidates {
    let published = PublishedClasses::empty();
    let mut queries = SemanticQueryState::default();
    let mut next_type_param = 0;
    collect_call_site_candidates_query(
        interner,
        &mut next_type_param,
        &published,
        &mut queries,
        CandidateCollectionRequest {
            active_params: None,
            params,
            args,
            fresh_args,
            receiver,
        },
    )
    .candidates
}

fn record_call_site_candidates(
    candidates: &mut CallSiteCandidates,
    local: Candidates,
    source: impl Fn(usize) -> CallSiteSource,
    fresh: bool,
) {
    for (param_id, cands) in local {
        candidates
            .entry(param_id)
            .or_default()
            .extend(
                cands
                    .into_iter()
                    .enumerate()
                    .map(|(occurrence, ty)| CallSiteCandidate {
                        ty,
                        source: source(occurrence),
                        fresh,
                    }),
            );
    }
}

fn fresh_exempt_params(candidates: &CallSiteCandidates) -> FxHashSet<TypeParamId> {
    candidates
        .iter()
        .filter_map(|(&param, cands)| {
            if !cands.is_empty() && cands.iter().all(|cand| cand.fresh) {
                Some(param)
            } else {
                None
            }
        })
        .collect()
}

fn inference_argument_targets(
    interner: &mut Interner,
    params: &[ParameterType],
    args: &[TypeId],
) -> Vec<Option<TypeId>> {
    let fixed: Vec<&ParameterType> = params.iter().filter(|param| param.is_fixed()).collect();
    let rest = params.iter().find(|param| param.rest);
    let mut targets: Vec<Option<TypeId>> = Vec::with_capacity(args.len());
    for index in 0..args.len() {
        if let Some(param) = fixed.get(index) {
            targets.push(Some(param.ty));
            continue;
        }
        let Some(rest) = rest else {
            targets.push(None);
            continue;
        };
        let rest_offset = index.saturating_sub(fixed.len());
        let total_rest_args = args.len().saturating_sub(fixed.len());
        targets.push(rest_inference_target(
            interner,
            rest.ty,
            rest_offset,
            total_rest_args,
        ));
    }
    targets
}

fn direct_rest_type_param(
    interner: &Interner,
    params: &[ParameterType],
) -> Option<(TypeId, usize)> {
    let rest = params.iter().find(|param| param.rest)?;
    let rest_ty = interner
        .store()
        .readonly_operand(rest.ty)
        .unwrap_or(rest.ty);
    if interner.store().tag(rest_ty) != TypeTag::TypeParam {
        return None;
    }
    let fixed = params.iter().filter(|param| param.is_fixed()).count();
    Some((rest_ty, fixed))
}

fn rest_inference_target(
    interner: &Interner,
    rest_ty: TypeId,
    offset: usize,
    total_rest_args: usize,
) -> Option<TypeId> {
    let rest_ty = interner
        .store()
        .readonly_operand(rest_ty)
        .unwrap_or(rest_ty);
    if interner.store().tag(rest_ty) == TypeTag::TypeParam {
        return None;
    }
    if let Some(array) = interner.store().array_type(rest_ty) {
        return Some(array.element);
    }
    if let Some(tuple) = interner.store().tuple_type(rest_ty) {
        return tuple_inference_target(interner, tuple, offset, total_rest_args);
    }
    Some(rest_ty)
}

fn tuple_inference_target(
    interner: &Interner,
    tuple: &crate::types::repr::TupleType,
    offset: usize,
    total_rest_args: usize,
) -> Option<TypeId> {
    let Some(rest) = tuple.rest else {
        return tuple.elements.get(offset).copied();
    };
    if offset < rest.position {
        return tuple.elements.get(offset).copied();
    }
    let fixed_after = tuple.elements.len().saturating_sub(rest.position);
    let tail_start = total_rest_args.saturating_sub(fixed_after);
    if offset >= tail_start {
        let tail_index = rest.position + (offset - tail_start);
        return tuple.elements.get(tail_index).copied();
    }
    rest_inference_target(interner, rest.ty, offset - rest.position, total_rest_args)
}

/// Fix call-site candidate contributions to one type per parameter. A single
/// source keeps the old M10 widening behavior; multiple sources fix to a candidate
/// that can expose incompatible later arguments to the ordinary relation replay.
fn fix_call_site_candidates(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    candidates: CallSiteCandidates,
    constraints: &FxHashMap<TypeParamId, Option<TypeId>>,
) -> DemandOutcome<FxHashMap<crate::types::repr::TypeParamId, TypeId>> {
    let mut map = FxHashMap::default();
    for (param, cands) in candidates {
        let constraint = constraints.get(&param).copied().flatten();
        let fixed = match fix_call_site_candidates_for_param(
            interner,
            next_type_param,
            published,
            queries,
            constraint,
            &cands,
        ) {
            DemandOutcome::Ready(fixed) => fixed,
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        };
        if let Some(fixed) = fixed {
            map.insert(param, fixed);
        }
    }
    DemandOutcome::Ready(map)
}

fn fix_call_site_candidates_for_param(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    constraint: Option<TypeId>,
    cands: &[CallSiteCandidate],
) -> DemandOutcome<Option<TypeId>> {
    let contributions = candidate_contributions(cands);
    let mut iter = contributions.iter();
    let Some(first) = iter.next() else {
        return DemandOutcome::Ready(None);
    };
    let primitive_constraint = constraint.is_some_and(|ty| is_primitive_ish(interner.store(), ty));
    if contributions.len() == 1 {
        return DemandOutcome::Ready(Some(fix_candidate_set(
            interner,
            constraint,
            first.candidates.iter().copied(),
        )));
    }

    let mut current = raw_candidate_set(interner, first.candidates.iter().copied());
    let mut current_prepared =
        fix_candidate_set(interner, constraint, first.candidates.iter().copied());
    let mut preserve_current = primitive_constraint || first.candidates.len() > 1;
    let mut current_fresh = first.fresh;

    for contribution in iter {
        let next = raw_candidate_set(interner, contribution.candidates.iter().copied());
        let next_prepared = fix_candidate_set(
            interner,
            constraint,
            contribution.candidates.iter().copied(),
        );

        let next_to_current = match is_assignable_type(
            interner,
            next_type_param,
            published,
            queries,
            next,
            current,
        ) {
            DemandOutcome::Ready(assignable) => assignable,
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        };
        if next_to_current {
            if current_fresh
                && !contribution.fresh
                && should_prefer_nonfresh_structural(interner.store(), current, next)
            {
                current = next;
                current_prepared = next_prepared;
                preserve_current = primitive_constraint || contribution.candidates.len() > 1;
                current_fresh = false;
            }
            continue;
        }
        let current_to_next = match is_assignable_type(
            interner,
            next_type_param,
            published,
            queries,
            current,
            next,
        ) {
            DemandOutcome::Ready(assignable) => assignable,
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        };
        if current_to_next {
            if !current_fresh
                && contribution.fresh
                && should_prefer_nonfresh_structural(interner.store(), next, current)
            {
                continue;
            }
            current = next;
            current_prepared = next_prepared;
            preserve_current = primitive_constraint || contribution.candidates.len() > 1;
            current_fresh = contribution.fresh;
            continue;
        }

        if mergeable_nullish_primitive(interner.store(), current, next) {
            current = interner.union(vec![current, next]);
            current_prepared = interner.union(vec![current_prepared, next_prepared]);
            preserve_current = true;
            current_fresh = current_fresh && contribution.fresh;
            continue;
        }

        if let Some(family) = same_primitive_family(interner.store(), current, next) {
            current = merge_same_primitive_family(interner, current, next, family);
            current_prepared =
                merge_same_primitive_family(interner, current_prepared, next_prepared, family);
            preserve_current = true;
            current_fresh = current_fresh && contribution.fresh;
            continue;
        }

        if should_union_fresh_unrelated(
            interner.store(),
            current,
            next,
            current_fresh,
            contribution.fresh,
        ) {
            current = interner.union(vec![current, next]);
            current_prepared = interner.union(vec![current_prepared, next_prepared]);
            preserve_current = true;
            current_fresh = true;
            continue;
        }

        return DemandOutcome::Ready(Some(if preserve_current {
            current
        } else {
            current_prepared
        }));
    }

    DemandOutcome::Ready(Some(current))
}

fn candidate_contributions(cands: &[CallSiteCandidate]) -> Vec<CandidateContribution> {
    let mut groups: Vec<CandidateContribution> = Vec::new();
    for cand in cands {
        if let Some(group) = groups.iter_mut().find(|group| group.source == cand.source) {
            group.fresh = group.fresh && cand.fresh;
            group.candidates.push(cand.ty);
            continue;
        }
        groups.push(CandidateContribution {
            source: cand.source,
            fresh: cand.fresh,
            candidates: vec![cand.ty],
        });
    }
    groups
}

fn raw_candidate_set(interner: &mut Interner, cands: impl Iterator<Item = TypeId>) -> TypeId {
    interner.union(cands.collect())
}

fn fix_candidate_set(
    interner: &mut Interner,
    constraint: Option<TypeId>,
    cands: impl Iterator<Item = TypeId>,
) -> TypeId {
    let prepared: Vec<TypeId> = if constraint.is_some_and(|ty| {
        is_primitive_ish(interner.store(), ty) || interner.store().tag(ty) == TypeTag::Keyof
    }) {
        cands.collect()
    } else {
        cands.map(|c| widen(interner, c)).collect()
    };
    interner.union(prepared)
}

fn is_assignable_type(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    source: TypeId,
    target: TypeId,
) -> DemandOutcome<bool> {
    match SemanticQueryCoordinator::new(interner, published, queries, next_type_param)
        .is_assignable(source, target)
    {
        RelationOutcome::Yes => DemandOutcome::Ready(true),
        RelationOutcome::No(_) => DemandOutcome::Ready(false),
        RelationOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
    }
}

fn should_union_fresh_unrelated(
    store: &Store,
    left: TypeId,
    right: TypeId,
    left_fresh: bool,
    right_fresh: bool,
) -> bool {
    left_fresh && right_fresh && !is_primitive_like(store, left) && !is_primitive_like(store, right)
}

fn should_prefer_nonfresh_structural(store: &Store, fresh_ty: TypeId, nonfresh_ty: TypeId) -> bool {
    !is_primitive_like(store, fresh_ty) && !is_primitive_like(store, nonfresh_ty)
}

fn mergeable_nullish_primitive(store: &Store, left: TypeId, right: TypeId) -> bool {
    let Some(mut families) = primitive_families(store, left) else {
        return false;
    };
    let Some(right_families) = primitive_families(store, right) else {
        return false;
    };
    for family in right_families {
        if !families.contains(&family) {
            families.push(family);
        }
    }
    let has_nullish = families
        .iter()
        .any(|family| matches!(family, PrimitiveFamily::Null | PrimitiveFamily::Undefined));
    if !has_nullish {
        return false;
    }

    let mut non_nullish: Vec<PrimitiveFamily> = Vec::new();
    for family in families {
        if matches!(family, PrimitiveFamily::Null | PrimitiveFamily::Undefined) {
            continue;
        }
        if !non_nullish.contains(&family) {
            non_nullish.push(family);
        }
    }
    non_nullish.len() <= 1
}

fn same_primitive_family(store: &Store, left: TypeId, right: TypeId) -> Option<PrimitiveFamily> {
    let mut families = primitive_families(store, left)?;
    for family in primitive_families(store, right)? {
        if !families.contains(&family) {
            families.push(family);
        }
    }
    match families.as_slice() {
        [family] => Some(*family),
        _ => None,
    }
}

fn is_primitive_like(store: &Store, ty: TypeId) -> bool {
    primitive_families(store, ty).is_some()
}

fn primitive_families(store: &Store, ty: TypeId) -> Option<Vec<PrimitiveFamily>> {
    match store.tag(ty) {
        TypeTag::Literal => {
            let lit = store.literal_value(ty)?;
            Some(vec![primitive_family_from_intrinsic(lit.base_kind())?])
        }
        TypeTag::Intrinsic => Some(vec![primitive_family_from_intrinsic(
            store.intrinsic_kind(ty)?,
        )?]),
        TypeTag::Union => {
            let mut families: Vec<PrimitiveFamily> = Vec::new();
            for &member in store.union_members(ty)? {
                for family in primitive_families(store, member)? {
                    if !families.contains(&family) {
                        families.push(family);
                    }
                }
            }
            Some(families)
        }
        _ => None,
    }
}

fn primitive_family_from_intrinsic(kind: IntrinsicKind) -> Option<PrimitiveFamily> {
    match kind {
        IntrinsicKind::String => Some(PrimitiveFamily::String),
        IntrinsicKind::Number => Some(PrimitiveFamily::Number),
        IntrinsicKind::Boolean => Some(PrimitiveFamily::Boolean),
        IntrinsicKind::Null => Some(PrimitiveFamily::Null),
        IntrinsicKind::Undefined => Some(PrimitiveFamily::Undefined),
        IntrinsicKind::Void => Some(PrimitiveFamily::Void),
        _ => None,
    }
}

fn merge_same_primitive_family(
    interner: &mut Interner,
    left: TypeId,
    right: TypeId,
    family: PrimitiveFamily,
) -> TypeId {
    if contains_primitive_base(interner.store(), left, family)
        || contains_primitive_base(interner.store(), right, family)
    {
        return primitive_base(interner, family);
    }
    interner.union(vec![left, right])
}

fn contains_primitive_base(store: &Store, ty: TypeId, family: PrimitiveFamily) -> bool {
    match store.tag(ty) {
        TypeTag::Intrinsic => {
            store
                .intrinsic_kind(ty)
                .and_then(primitive_family_from_intrinsic)
                == Some(family)
        }
        TypeTag::Union => store.union_members(ty).is_some_and(|members| {
            members
                .iter()
                .any(|&member| contains_primitive_base(store, member, family))
        }),
        _ => false,
    }
}

fn primitive_base(interner: &Interner, family: PrimitiveFamily) -> TypeId {
    let wk = interner.well_known();
    match family {
        PrimitiveFamily::String => wk.string,
        PrimitiveFamily::Number => wk.number,
        PrimitiveFamily::Boolean => wk.boolean,
        PrimitiveFamily::Null => wk.null,
        PrimitiveFamily::Undefined => wk.undefined,
        PrimitiveFamily::Void => wk.void,
    }
}

fn is_primitive_ish(store: &Store, ty: TypeId) -> bool {
    match store.tag(ty) {
        TypeTag::Literal | TypeTag::Template => true,
        TypeTag::Intrinsic => matches!(
            store.intrinsic_kind(ty),
            Some(
                IntrinsicKind::String
                    | IntrinsicKind::Number
                    | IntrinsicKind::Boolean
                    | IntrinsicKind::Null
                    | IntrinsicKind::Undefined
                    | IntrinsicKind::Void
            )
        ),
        TypeTag::Union => store
            .union_members(ty)
            .is_some_and(|members| members.iter().any(|&m| is_primitive_ish(store, m))),
        _ => false,
    }
}

/// Complete a partially-fixed map to cover **every** declared type parameter,
/// applying each parameter's **constraint** (M24) and default. Processing parameters
/// in declared order builds the map incrementally, so a constraint/default that
/// references an earlier parameter (`<T, U extends T = T>`) is substituted with that
/// parameter's already-fixed argument. For each parameter:
///
///  - **a candidate landed** → keep it, unless it **violates** the (substituted)
///    constraint, in which case **clamp to the constraint** (so the ordinary argument
///    check then reports `TK2345` against the constraint, matching tsc — the failure
///    surfaces there, never as `TK2344` on the inference path). The one exemption is
///    a parameter in `fresh_exempt` — **every** candidate it received came from a
///    fresh object/array literal argument (review F4: per-argument provenance),
///    which tsc contextually retypes against the constraint (a pass that is out of
///    scope, documented deferral), so clamping it would over-report; a parameter
///    with **any** non-fresh candidate (a typed value — primitive OR structural,
///    which tsc cannot reshape) clamps normally;
///  - **no candidate** → use the (substituted) declaration **default** when present,
///    otherwise fall back to the (substituted) **constraint** or **`unknown`** (the
///    sound M10 fallback — `unknown` cannot mask a downstream error the way `any`
///    would).
///
/// An **unconstrained** parameter is unchanged from the M10 behaviour (candidate as-is,
/// or `unknown`), so existing generic inference is untouched.
fn fix_signature_params(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    type_params: &[GenericTypeParam],
    fixed: FxHashMap<TypeParamId, TypeId>,
    fresh_exempt: &FxHashSet<TypeParamId>,
) -> DemandOutcome<FixedSignatureParams> {
    let wk = interner.well_known();
    // Start from **all** collected candidates — this preserves bindings for parameters
    // NOT in `type_params` (e.g. a derived generic class inheriting its base's
    // constructor, whose parameter belongs to the *base*'s list); the constraint pass
    // below only overrides the declared ones.
    let mut map = fixed;
    let mut constraint_violations = FxHashSet::default();
    for type_param in type_params {
        let param = type_param.id;
        // The parameter's constraint, substituted with the arguments fixed so far
        // (releases the immutable borrow before the `&mut` substitute). `None` when the
        // parameter is unconstrained.
        let raw_constraint = type_param.constraint;
        let constraint = match raw_constraint {
            Some(c) => {
                let substituted = substitute(interner, c, &map);
                match SemanticQueryCoordinator::new(interner, published, queries, next_type_param)
                    .demand(substituted)
                {
                    DemandOutcome::Ready(evaluated) => Some(evaluated),
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                }
            }
            None => None,
        };
        // M28 gate (mirroring the TK2344 gate in `check_type_argument_constraints`):
        // evaluate first so concrete `keyof` constraints relate by value, then skip
        // only constraints that still carry a deferred `keyof`.
        let constraint = constraint
            .filter(|&c| !crate::check::checker::eval::contains_deferred_keyof(interner, c));

        let value = match map.get(&param).copied() {
            Some(candidate) => match constraint {
                Some(c) => {
                    let satisfies = match SemanticQueryCoordinator::new(
                        interner,
                        published,
                        queries,
                        next_type_param,
                    )
                    .is_assignable(candidate, c)
                    {
                        RelationOutcome::Yes => true,
                        RelationOutcome::No(_) => {
                            constraint_violations.insert(param);
                            false
                        }
                        RelationOutcome::Exhausted(exhaustion) => {
                            return DemandOutcome::Exhausted(exhaustion);
                        }
                    };
                    // A violating candidate clamps to the constraint — unless EVERY
                    // candidate for this parameter came from a fresh literal argument
                    // (the contextual-typing deferral; per-argument provenance,
                    // review F4 — see the doc above).
                    if satisfies || fresh_exempt.contains(&param) {
                        candidate
                    } else {
                        c
                    }
                }
                None => candidate,
            },
            // No candidate: a declaration default wins over the substituted
            // constraint; otherwise retain the existing conservative `unknown`
            // fallback. Defaults are evaluated only after earlier parameters have
            // been fixed, so `<T, U = T>` observes the final `T` binding.
            None => type_param
                .default
                .map(|default| substitute(interner, default, &map))
                .or(constraint)
                .unwrap_or(wk.unknown),
        };
        map.insert(param, value);
    }

    DemandOutcome::Ready(FixedSignatureParams {
        arguments: map,
        constraint_violations,
    })
}

fn fix_present_signature_params(
    interner: &mut Interner,
    next_type_param: &mut u32,
    published: &PublishedClasses,
    queries: &mut SemanticQueryState,
    type_params: &[GenericTypeParam],
    mut fixed: FxHashMap<TypeParamId, TypeId>,
    fresh_exempt: &FxHashSet<TypeParamId>,
) -> DemandOutcome<Vec<(TypeParamId, TypeId)>> {
    let mut ordered = Vec::new();
    for type_param in type_params {
        let Some(candidate) = fixed.get(&type_param.id).copied() else {
            continue;
        };
        let constraint = match type_param.constraint {
            Some(constraint) => {
                let substituted = substitute(interner, constraint, &fixed);
                let evaluated = match SemanticQueryCoordinator::new(
                    interner,
                    published,
                    queries,
                    next_type_param,
                )
                .demand(substituted)
                {
                    DemandOutcome::Ready(evaluated) => evaluated,
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                };
                (!crate::check::checker::eval::contains_deferred_keyof(interner, evaluated))
                    .then_some(evaluated)
            }
            None => None,
        };
        let selected = match constraint {
            Some(constraint) if !fresh_exempt.contains(&type_param.id) => {
                match SemanticQueryCoordinator::new(interner, published, queries, next_type_param)
                    .is_assignable(candidate, constraint)
                {
                    RelationOutcome::Yes => candidate,
                    RelationOutcome::No(_) => constraint,
                    RelationOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                }
            }
            _ => candidate,
        };
        fixed.insert(type_param.id, selected);
        ordered.push((type_param.id, selected));
    }
    DemandOutcome::Ready(ordered)
}

#[cfg(test)]
mod wu7_measurements {
    use super::*;
    use crate::check::query::{
        query_demand_measure, reset_query_demand_measure, QueryDemandMeasure,
    };
    use crate::types::repr::{
        ConditionalType, FunctionType, LiteralValue, ObjectType, PropertyType,
    };
    use std::time::{Duration, Instant};

    #[derive(Clone, Copy)]
    enum ConstraintCorpus {
        ArchivedFunctionFanout,
        SharedPending,
        StructuralCycle,
        EvaluationExhaustion,
    }

    fn root_conditional(interner: &mut Interner, branch: TypeId) -> TypeId {
        let wk = interner.well_known();
        interner.intern_conditional(ConditionalType {
            check: wk.string,
            extends_ty: wk.string,
            true_branch: branch,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        })
    }

    fn runaway_evaluation(interner: &mut Interner) -> TypeId {
        let wk = interner.well_known();
        let param = TypeParamId(98_000);
        let param_ty = interner.intern_type_param(param, "T");
        let empty = interner.intern_object(ObjectType::default());
        let template = interner.reserve_conditional();
        let wrapped = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("value", param_ty)],
            ..Default::default()
        });
        let recur = interner.intern_instantiation(template, vec![(param, wrapped)]);
        interner.fill_conditional(
            template,
            ConditionalType {
                check: param_ty,
                extends_ty: empty,
                true_branch: recur,
                false_branch: wk.never,
                infer_count: 0,
                distributive: true,
                poisoned: false,
            },
        );
        interner.intern_instantiation(template, vec![(param, empty)])
    }

    fn measure_constraint_corpus(
        count: usize,
        corpus: ConstraintCorpus,
    ) -> (
        DemandOutcome<FixedSignatureParams>,
        QueryDemandMeasure,
        Duration,
    ) {
        let mut interner = Interner::with_intrinsics();
        let (constraint, branch) = match corpus {
            ConstraintCorpus::ArchivedFunctionFanout => {
                let literal = interner.intern_literal(LiteralValue::String("fanout".into()));
                let pending = interner.intern_instantiation(
                    interner.well_known().uppercase,
                    vec![(TypeParamId(98_001), literal)],
                );
                let metadata = (0..count)
                    .map(|index| GenericTypeParam {
                        id: TypeParamId(99_000 + u32::try_from(index).unwrap()),
                        constraint: Some(pending),
                        default: Some(pending),
                    })
                    .collect();
                let function = interner.intern_function(FunctionType {
                    type_params: metadata,
                    receiver: Some(pending),
                    params: Vec::new(),
                    ret: interner.well_known().void,
                });
                (function, function)
            }
            ConstraintCorpus::SharedPending => {
                let literal = interner.intern_literal(LiteralValue::String("fanout".into()));
                let pending = interner.intern_instantiation(
                    interner.well_known().uppercase,
                    vec![(TypeParamId(98_001), literal)],
                );
                let branch = interner.intern_tuple(vec![pending; count]);
                (root_conditional(&mut interner, branch), branch)
            }
            ConstraintCorpus::StructuralCycle => {
                let recursive = interner.reserve_object();
                interner.fill_object(
                    recursive,
                    ObjectType {
                        properties: vec![PropertyType::public("self", recursive)],
                        ..Default::default()
                    },
                );
                let branch = interner.intern_tuple(vec![recursive; count]);
                (root_conditional(&mut interner, branch), branch)
            }
            ConstraintCorpus::EvaluationExhaustion => {
                let runaway = runaway_evaluation(&mut interner);
                let branch = interner.intern_tuple(vec![runaway; count]);
                (root_conditional(&mut interner, branch), branch)
            }
        };
        let parameter = TypeParamId(98_002);
        let type_params = [GenericTypeParam {
            id: parameter,
            constraint: Some(constraint),
            default: None,
        }];
        let fixed = FxHashMap::from_iter([(parameter, branch)]);
        let published = PublishedClasses::empty();
        let mut queries = SemanticQueryState::default();
        let mut next_type_param = 98_003;

        reset_query_demand_measure();
        let started = Instant::now();
        let outcome = fix_signature_params(
            &mut interner,
            &mut next_type_param,
            &published,
            &mut queries,
            &type_params,
            fixed,
            &FxHashSet::default(),
        );
        let elapsed = started.elapsed();
        (outcome, query_demand_measure(), elapsed)
    }

    #[test]
    // WU1 removed this structural demand walk; scaling would benchmark substitution instead.
    fn measure_archived_constraint_function_fanout_is_no_longer_demand_walked() {
        let (outcome, measure, _) =
            measure_constraint_corpus(10, ConstraintCorpus::ArchivedFunctionFanout);
        assert!(matches!(outcome, DemandOutcome::Ready(_)));
        assert_eq!(
            measure,
            QueryDemandMeasure {
                root_calls: 1,
                ..Default::default()
            }
        );
    }

    #[test]
    fn measure_constraint_shared_pending_dag_uses_query_overlay() {
        let (outcome, measure, _) = measure_constraint_corpus(10, ConstraintCorpus::SharedPending);
        assert!(matches!(outcome, DemandOutcome::Ready(_)));
        assert_eq!(
            measure,
            QueryDemandMeasure {
                root_calls: 1,
                planner_root_visits: 1,
                planner_visits: 17,
                overlay_hits: 9,
                visited_hits: 1,
                reentries: 0,
                pending_evaluations: 2,
                durable_evaluation_hits: 0,
                evaluation_expansions: 2,
                evaluation_identity_returns: 0,
                evaluation_changed_returns: 2,
                evaluation_memo_inserts: 2,
                durable_evaluation_inserts: 2,
                exhaustion_frontiers: 0,
                evaluation_budget_exhaustions: 0,
                evaluation_cycle_exhaustions: 0,
            }
        );
    }

    #[test]
    fn measure_constraint_structural_cycle_returns_identity_without_taint() {
        let (outcome, measure, _) =
            measure_constraint_corpus(10, ConstraintCorpus::StructuralCycle);
        assert!(matches!(outcome, DemandOutcome::Ready(_)));
        assert_eq!(
            measure,
            QueryDemandMeasure {
                root_calls: 1,
                planner_root_visits: 1,
                planner_visits: 16,
                overlay_hits: 0,
                visited_hits: 10,
                reentries: 1,
                pending_evaluations: 1,
                durable_evaluation_hits: 0,
                evaluation_expansions: 1,
                evaluation_identity_returns: 0,
                evaluation_changed_returns: 1,
                evaluation_memo_inserts: 1,
                durable_evaluation_inserts: 1,
                exhaustion_frontiers: 0,
                evaluation_budget_exhaustions: 0,
                evaluation_cycle_exhaustions: 0,
            }
        );
    }

    #[test]
    fn measure_constraint_identity_demand_hits_the_durable_query_memo() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let identity = interner.intern_conditional(ConditionalType {
            check: wk.string,
            extends_ty: wk.string,
            true_branch: wk.string,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: true,
        });
        let first = TypeParamId(98_010);
        let second = TypeParamId(98_011);
        let type_params = [first, second].map(|id| GenericTypeParam {
            id,
            constraint: Some(identity),
            default: None,
        });
        let fixed = FxHashMap::from_iter([(first, identity), (second, identity)]);
        let published = PublishedClasses::empty();
        let mut queries = SemanticQueryState::default();
        let mut next_type_param = 98_012;

        reset_query_demand_measure();
        let outcome = fix_signature_params(
            &mut interner,
            &mut next_type_param,
            &published,
            &mut queries,
            &type_params,
            fixed,
            &FxHashSet::default(),
        );

        assert!(matches!(outcome, DemandOutcome::Ready(_)));
        assert_eq!(
            query_demand_measure(),
            QueryDemandMeasure {
                root_calls: 2,
                planner_root_visits: 2,
                planner_visits: 6,
                overlay_hits: 0,
                visited_hits: 2,
                reentries: 0,
                pending_evaluations: 2,
                durable_evaluation_hits: 1,
                evaluation_expansions: 1,
                evaluation_identity_returns: 1,
                evaluation_changed_returns: 0,
                evaluation_memo_inserts: 2,
                durable_evaluation_inserts: 1,
                exhaustion_frontiers: 0,
                evaluation_budget_exhaustions: 0,
                evaluation_cycle_exhaustions: 0,
            }
        );
    }

    #[test]
    fn measure_constraint_evaluation_exhaustion_discards_durable_writes() {
        let (outcome, measure, _) =
            measure_constraint_corpus(10, ConstraintCorpus::EvaluationExhaustion);
        assert!(matches!(
            outcome,
            DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
        ));
        assert_eq!(
            measure,
            QueryDemandMeasure {
                root_calls: 1,
                planner_root_visits: 1,
                planner_visits: 24,
                overlay_hits: 0,
                visited_hits: 13,
                reentries: 1,
                pending_evaluations: 4,
                durable_evaluation_hits: 0,
                evaluation_expansions: 4,
                evaluation_identity_returns: 1,
                evaluation_changed_returns: 2,
                evaluation_memo_inserts: 3,
                durable_evaluation_inserts: 0,
                exhaustion_frontiers: 1,
                evaluation_budget_exhaustions: 1,
                evaluation_cycle_exhaustions: 0,
            }
        );
    }

    #[test]
    fn contextual_constraint_exhaustion_discards_optional_proposal() {
        let mut interner = Interner::with_intrinsics();
        let parameter = TypeParamId(98_020);
        let parameter_ty = interner.intern_type_param(parameter, "T");
        let constraint = runaway_evaluation(&mut interner);
        let type_params = [GenericTypeParam {
            id: parameter,
            constraint: Some(constraint),
            default: None,
        }];
        let active_params = FxHashSet::from_iter([parameter]);
        let ordinary_bound = FxHashSet::default();
        let constraints = FxHashMap::from_iter([(parameter, Some(constraint))]);
        let unknown = interner.well_known().unknown;
        let baseline = FixedSignatureParams {
            arguments: FxHashMap::from_iter([(parameter, unknown)]),
            constraint_violations: FxHashSet::default(),
        };
        let source = interner.well_known().string;
        let published = PublishedClasses::empty();
        let mut queries = SemanticQueryState::default();
        let mut next_type_param = 98_021;

        let fixed = apply_return_context_candidates(
            &mut interner,
            &mut next_type_param,
            &published,
            &mut queries,
            ReturnContextInferenceRequest {
                type_params: &type_params,
                active_params: &active_params,
                ordinary_bound: &ordinary_bound,
                constraints: &constraints,
                baseline,
                source,
                target: parameter_ty,
            },
        );

        assert_eq!(fixed.arguments.get(&parameter), Some(&unknown));
        assert!(fixed.constraint_violations.is_empty());
    }

    #[test]
    #[ignore = "WU7 release measurement; run explicitly with --ignored --nocapture"]
    fn measure_constraint_demand_at_10k_and_100k_release() {
        for count in [10_000, 100_000] {
            let operations = u64::try_from(count).unwrap();
            for corpus in [
                ConstraintCorpus::SharedPending,
                ConstraintCorpus::StructuralCycle,
                ConstraintCorpus::EvaluationExhaustion,
            ] {
                let (outcome, measure, elapsed) = measure_constraint_corpus(count, corpus);
                match corpus {
                    ConstraintCorpus::SharedPending => {
                        assert!(matches!(outcome, DemandOutcome::Ready(_)));
                        assert_eq!(measure.planner_visits, operations + 7);
                        assert_eq!(measure.overlay_hits, operations - 1);
                        assert_eq!(measure.pending_evaluations, 2);
                    }
                    ConstraintCorpus::StructuralCycle => {
                        assert!(matches!(outcome, DemandOutcome::Ready(_)));
                        assert_eq!(measure.planner_visits, operations + 6);
                        assert_eq!(measure.visited_hits, operations);
                        assert_eq!(measure.reentries, 1);
                    }
                    ConstraintCorpus::EvaluationExhaustion => {
                        assert!(matches!(
                            outcome,
                            DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
                        ));
                        assert_eq!(measure.planner_visits, operations + 14);
                        assert_eq!(measure.visited_hits, operations + 3);
                        assert_eq!(measure.reentries, 1);
                        assert_eq!(measure.pending_evaluations, 4);
                        assert_eq!(measure.evaluation_expansions, 4);
                        assert_eq!(measure.evaluation_identity_returns, 1);
                        assert_eq!(measure.evaluation_changed_returns, 2);
                        assert_eq!(measure.evaluation_memo_inserts, 3);
                        assert_eq!(measure.exhaustion_frontiers, 1);
                        assert_eq!(measure.evaluation_budget_exhaustions, 1);
                        assert_eq!(measure.durable_evaluation_inserts, 0);
                    }
                    ConstraintCorpus::ArchivedFunctionFanout => unreachable!(),
                }
                let label = match corpus {
                    ConstraintCorpus::SharedPending => "shared-pending",
                    ConstraintCorpus::StructuralCycle => "structural-cycle",
                    ConstraintCorpus::EvaluationExhaustion => "evaluation-exhaustion",
                    ConstraintCorpus::ArchivedFunctionFanout => unreachable!(),
                };
                println!(
                    "constraint demand count={count} corpus={label} measure={measure:?} elapsed={elapsed:?}"
                );
            }
        }
    }
}

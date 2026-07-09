//! Type-argument inference (M10; architecture §5.1).
//! Produces type-parameter bindings from call arguments; the relation engine still
//! decides assignability after substitution. Call-site candidates keep their source
//! contribution so incompatible arguments cannot be accepted by unioning every
//! source into a too-wide target. No candidate fixes to `unknown`; no candidate
//! still falls back to the parameter constraint or `unknown`. The `(source, target)`
//! guard makes recursive matching terminate.

mod context;
mod helpers;
#[cfg(test)]
mod tests;

use context::InferenceContext;
use helpers::widen;

use crate::relate::Relater;
use crate::types::repr::{IntrinsicKind, ParameterType, TypeParamId, TypeTag};
use crate::types::store::{Store, TypeId};
use crate::types::{substitute, Interner};
use rustc_hash::{FxHashMap, FxHashSet};

/// The raw candidates per type parameter for conditional-`infer` mode. Call-site
/// inference uses [`CallSiteCandidates`] so it can keep source provenance separate
/// from conditional same-name union behavior.
pub type Candidates = FxHashMap<crate::types::repr::TypeParamId, Vec<TypeId>>;

type CallSiteCandidates = FxHashMap<crate::types::repr::TypeParamId, Vec<CallSiteCandidate>>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct CallSiteCandidate {
    ty: TypeId,
    source: CallSiteSource,
    fresh: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CallSiteSource {
    Argument { index: usize, occurrence: usize },
    DirectRest { start: usize, occurrence: usize },
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

/// Build the full instantiation map for a generic signature's parameters from a
/// call's argument types (the M10 entry point used by the checker).
///
/// Every declared parameter id gets an entry: one fixed from its collected
/// candidates if any landed, otherwise the parameter's **constraint** (M24) or
/// **`unknown`** (the sound no-candidate fallback — `unknown` cannot mask a
/// downstream error the way `any` would). The returned map is fed straight to the
/// existing M9 substitution.
///
/// `next_type_param` is the module-wide allocator used when constraint evaluation
/// freshens `infer` binders while fixing parameters.
///
/// `fresh_args` marks, positionally (parallel to `args`), which arguments are
/// **fresh object/array literals** at the call site. A parameter is exempt from the
/// M24 clamp-to-constraint (see [`fix_params`]) only when **EVERY** candidate it
/// received came from a fresh literal (review F4 — the exemption is per-ARGUMENT: a
/// fresh satisfying literal must not shield a separate non-fresh violating argument
/// binding the same parameter). tsc contextually retypes a fresh literal against
/// the constraint — a pass that is out of scope — so clamping an all-fresh parameter
/// would over-report; any non-fresh candidate is a typed value tsc cannot reshape,
/// so the parameter clamps normally. A missing/short slice means "not fresh" (the
/// conservative default for callers with no syntactic argument info, e.g. unit
/// tests).
pub fn infer_type_arguments(
    interner: &mut Interner,
    next_type_param: &mut u32,
    type_params: &[crate::types::repr::TypeParamId],
    params: &[TypeId],
    args: &[TypeId],
    fresh_args: &[bool],
) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    let shaped_params: Vec<ParameterType> = params
        .iter()
        .enumerate()
        .map(|(index, &ty)| ParameterType::required(format!("p{index}"), ty))
        .collect();
    infer_type_arguments_from_params(
        interner,
        next_type_param,
        type_params,
        &shaped_params,
        args,
        fresh_args,
    )
}

pub fn infer_type_arguments_from_params(
    interner: &mut Interner,
    next_type_param: &mut u32,
    type_params: &[crate::types::repr::TypeParamId],
    params: &[ParameterType],
    args: &[TypeId],
    fresh_args: &[bool],
) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    let candidates = collect_call_site_candidates(interner, params, args, fresh_args);
    let exempt = fresh_exempt_params(&candidates);
    let fixed = fix_call_site_candidates(interner, candidates);
    fix_params(interner, next_type_param, type_params, fixed, &exempt)
}

fn collect_call_site_candidates(
    interner: &mut Interner,
    params: &[ParameterType],
    args: &[TypeId],
    fresh_args: &[bool],
) -> CallSiteCandidates {
    let mut candidates: CallSiteCandidates = FxHashMap::default();
    let targets = inference_argument_targets(interner, params, args);
    for (index, (&arg, param)) in args.iter().zip(&targets).enumerate() {
        let Some(param) = *param else {
            continue;
        };
        let mut local: Candidates = FxHashMap::default();
        // Keep candidates raw here; primitive-constrained parameters decide literal
        // preservation at fix time.
        infer_from_types_raw(interner, arg, param, &mut local);
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
            infer_from_types_raw(interner, tuple, rest_ty, &mut local);
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
    candidates
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
    candidates: CallSiteCandidates,
) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    let mut map = FxHashMap::default();
    for (param, cands) in candidates {
        if let Some(fixed) = fix_call_site_candidates_for_param(interner, param, &cands) {
            map.insert(param, fixed);
        }
    }
    map
}

fn fix_call_site_candidates_for_param(
    interner: &mut Interner,
    param: TypeParamId,
    cands: &[CallSiteCandidate],
) -> Option<TypeId> {
    let contributions = candidate_contributions(cands);
    let mut iter = contributions.iter();
    let first = iter.next()?;
    let primitive_constraint = has_primitive_constraint(interner, param);
    if contributions.len() == 1 {
        return Some(fix_candidate_set(
            interner,
            param,
            first.candidates.iter().copied(),
        ));
    }

    let mut current = raw_candidate_set(interner, first.candidates.iter().copied());
    let mut current_prepared = fix_candidate_set(interner, param, first.candidates.iter().copied());
    let mut preserve_current = primitive_constraint || first.candidates.len() > 1;
    let mut current_fresh = first.fresh;

    for contribution in iter {
        let next = raw_candidate_set(interner, contribution.candidates.iter().copied());
        let next_prepared =
            fix_candidate_set(interner, param, contribution.candidates.iter().copied());

        if is_assignable_type(interner, next, current) {
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
        if is_assignable_type(interner, current, next) {
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

        return Some(if preserve_current {
            current
        } else {
            current_prepared
        });
    }

    Some(current)
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
    param: TypeParamId,
    cands: impl Iterator<Item = TypeId>,
) -> TypeId {
    let prepared: Vec<TypeId> = if has_primitive_constraint(interner, param) {
        cands.collect()
    } else {
        cands.map(|c| widen(interner, c)).collect()
    };
    interner.union(prepared)
}

fn is_assignable_type(interner: &Interner, source: TypeId, target: TypeId) -> bool {
    let store = interner.store();
    let mut relater = Relater::new(store, interner.well_known());
    relater.is_assignable(source, target).is_yes()
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

/// Whether a type parameter has a **primitive constraint** (tsc `hasPrimitiveConstraint`),
/// so literal candidates inferred for it are **not** widened (M24/M27). True when the
/// constraint is (or, for a union, contains) a primitive intrinsic
/// (`string`/`number`/`boolean`/`null`/`undefined`/`void`), a literal, or a template
/// literal pattern; false for an unconstrained parameter or an object/`unknown`/`any`
/// constraint.
fn has_primitive_constraint(interner: &Interner, param: TypeParamId) -> bool {
    match interner.store().type_param_constraint(param) {
        Some(constraint) => is_primitive_ish(interner.store(), constraint),
        None => false,
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

/// Structurally match an argument source against a parameter target, recording raw
/// candidates later fixed by [`fix_call_site_candidates`]. Call-site mode deliberately has
/// no union-target descent: ambiguous matches infer nothing, which is sound.
///
/// The small sound subset records direct type-parameter hits, shared object
/// properties, positional function parts, and equal-length union pairings. The
/// recursion is cycle-guarded.
fn infer_from_types_raw(
    interner: &mut Interner,
    source: TypeId,
    target: TypeId,
    candidates: &mut Candidates,
) {
    let mut ctx = InferenceContext::for_call_raw();
    ctx.infer(interner, source, target, candidates);
}

/// Complete a partially-fixed map to cover **every** declared type parameter,
/// applying each parameter's **constraint** (M24). Processing parameters in declared
/// order builds the map incrementally, so a constraint that references an earlier
/// parameter (`<T, U extends T>`) is substituted with that parameter's already-fixed
/// argument. For each parameter:
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
///  - **no candidate** → fall back to the (substituted) **constraint** when the
///    parameter has one, else to **`unknown`** (the sound M10 fallback — `unknown`
///    cannot mask a downstream error the way `any` would).
///
/// An **unconstrained** parameter is unchanged from the M10 behaviour (candidate as-is,
/// or `unknown`), so existing generic inference is untouched.
fn fix_params(
    interner: &mut Interner,
    next_type_param: &mut u32,
    type_params: &[crate::types::repr::TypeParamId],
    fixed: FxHashMap<crate::types::repr::TypeParamId, TypeId>,
    fresh_exempt: &FxHashSet<crate::types::repr::TypeParamId>,
) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    let wk = interner.well_known();
    // Start from **all** collected candidates — this preserves bindings for parameters
    // NOT in `type_params` (e.g. a derived generic class inheriting its base's
    // constructor, whose parameter belongs to the *base*'s list); the constraint pass
    // below only overrides the declared ones.
    let mut map = fixed;
    for &param in type_params {
        // The parameter's constraint, substituted with the arguments fixed so far
        // (releases the immutable borrow before the `&mut` substitute). `None` when the
        // parameter is unconstrained.
        let raw_constraint = interner.store().type_param_constraint(param);
        let constraint = raw_constraint.map(|c| {
            let substituted = substitute(interner, c, &map);
            let evaluated = crate::check::checker::eval::evaluate_inference_constraint(
                interner,
                next_type_param,
                substituted,
            );
            if evaluated.exhausted {
                substituted
            } else {
                evaluated.result
            }
        });
        // M28 gate (mirroring the TK2344 gate in `check_type_argument_constraints`):
        // evaluate first so concrete `keyof` constraints relate by value, then skip
        // only constraints that still carry a deferred `keyof`.
        let constraint = constraint.filter(|&c| {
            !crate::check::checker::eval::contains_deferred_keyof(interner.store(), c)
        });

        let value = match map.get(&param).copied() {
            Some(candidate) => match constraint {
                Some(c) => {
                    let satisfies = {
                        let store = interner.store();
                        let mut relater = Relater::new(store, wk);
                        relater.is_assignable(candidate, c).is_yes()
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
            // No candidate: fall back to the constraint if any, else `unknown`.
            None => constraint.unwrap_or(wk.unknown),
        };
        map.insert(param, value);
    }
    map
}

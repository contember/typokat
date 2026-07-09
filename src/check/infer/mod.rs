//! Type-argument inference (M10; architecture §5.1).
//! Produces type-parameter bindings from call arguments; the relation engine still
//! decides assignability after substitution. No candidate fixes to `unknown`, one
//! candidate fixes to itself, and multiple distinct candidates fix to their union.
//! Soundness rule: inference may be too wide, but never too narrow or `any`; the
//! `(source, target)` guard makes recursive matching terminate.

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

/// The raw candidates per type parameter; [`fix_candidates`] handles widening and
/// duplicate collapse.
pub type Candidates = FxHashMap<crate::types::repr::TypeParamId, Vec<TypeId>>;

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
    // Track candidate provenance so the constraint clamp can exempt fresh-only
    // object/array literal arguments.
    let mut candidates: Candidates = FxHashMap::default();
    let mut fresh_params: FxHashSet<crate::types::repr::TypeParamId> = FxHashSet::default();
    let mut nonfresh_params: FxHashSet<crate::types::repr::TypeParamId> = FxHashSet::default();
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
        for (param_id, cands) in local {
            if is_fresh {
                fresh_params.insert(param_id);
            } else {
                nonfresh_params.insert(param_id);
            }
            candidates.entry(param_id).or_default().extend(cands);
        }
    }
    if let Some((rest_ty, rest_start)) = direct_rest_type_param(interner, params) {
        if rest_start <= args.len() {
            let tuple = interner.intern_tuple(args[rest_start..].to_vec());
            let mut local: Candidates = FxHashMap::default();
            infer_from_types_raw(interner, tuple, rest_ty, &mut local);
            let is_fresh = (rest_start..args.len())
                .all(|index| fresh_args.get(index).copied().unwrap_or(false));
            for (param_id, cands) in local {
                if is_fresh {
                    fresh_params.insert(param_id);
                } else {
                    nonfresh_params.insert(param_id);
                }
                candidates.entry(param_id).or_default().extend(cands);
            }
        }
    }
    // Exempt = candidates came from fresh literals ONLY (review F4).
    let exempt: FxHashSet<crate::types::repr::TypeParamId> =
        fresh_params.difference(&nonfresh_params).copied().collect();
    let fixed = fix_candidates(interner, candidates);
    fix_params(interner, next_type_param, type_params, fixed, &exempt)
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

/// Fix raw candidate lists to one type per parameter. Call-site literals widen
/// unless the parameter has tsc's primitive constraint shape (M24/M27), where tsc
/// preserves the literal; several prepared candidates fix to their union.
fn fix_candidates(
    interner: &mut Interner,
    candidates: Candidates,
) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    let mut map = FxHashMap::default();
    for (param, cands) in candidates {
        if cands.is_empty() {
            continue;
        }
        let prepared: Vec<TypeId> = if has_primitive_constraint(interner, param) {
            cands
        } else {
            cands.iter().map(|&c| widen(interner, c)).collect()
        };
        let fixed = if prepared.len() == 1 {
            prepared[0]
        } else {
            interner.union(prepared)
        };
        map.insert(param, fixed);
    }
    map
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
/// candidates later fixed by [`fix_candidates`]. Call-site mode deliberately has
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

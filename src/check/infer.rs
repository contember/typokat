//! Type-argument inference (M10 — the *generative* inference engine, architecture
//! §5.1).
//!
//! When a generic function is called **without** explicit type arguments
//! (`identity(5)` rather than `identity<number>(5)`), the type arguments must be
//! *inferred* from the call arguments. This is a separate machine from the
//! relation engine: the relater **decides** whether one type is assignable to
//! another; this engine **produces** types (the type-parameter bindings) from a
//! set of constraints. Architecture §5.1 sizes it roughly as large as the relation
//! engine; M10 is its first, deliberately small slice (type-argument inference
//! only — contextual typing, `extends`-constraint–guided inference, inference
//! priorities, and variance-aware inference are later milestones).
//!
//! ## Shape
//!
//! Two phases, mirroring tsc's "candidate collection" then "fixing":
//!
//!  1. **Collect** ([`infer_from_types`]): structurally match each call-argument
//!     type against its parameter type, recording, for every type parameter, the
//!     argument types that landed against it (its *candidates*).
//!  2. **Fix** ([`fix`]): turn each parameter's candidate list into a single type —
//!     no candidates → **`unknown`** (the **sound** fallback: `unknown` accepts
//!     nothing downstream without a check, so it can never *mask* a real error the
//!     way `any` would); one candidate → that type; ≥ 2 distinct candidates → their
//!     **union** ([`Interner::union`]).
//!
//! The resulting `TypeParamId → TypeId` map is then fed to the **existing** M9
//! substitution to instantiate the signature, after which the **existing**
//! arity/argument/return checks run unchanged. Inference only *decides* the type
//! arguments; assignability is still verified by the relation engine — inference
//! never bypasses a check.
//!
//! ## Soundness
//!
//! Inferring too **wide** is acceptable (it can only over-report); inferring too
//! **narrow**, or to `any`, is **not** (it would drop a real error). Two choices
//! follow:
//!
//!  - the no-candidate fallback is `unknown`, never `any`;
//!  - each candidate is **widened** (a literal `5` → `number`) when recorded, so an
//!    inferred argument is never *narrower* than what a value of that type denotes.
//!    Widening can only make the inferred type wider, so it stays sound; it also
//!    matches the conventional top-level (non-contextual) inference result and the
//!    fixtures' "`T` inferred `number`" expectations.
//!
//! ## Termination
//!
//! Structural matching recurses through objects/functions/unions, which — via the
//! M5 reserve-then-fill — can be **self-referential** (`interface List { tail: List
//! | null }`). A naive walk would loop. [`InferenceContext`] therefore carries a
//! `visited` set of the `(source, target)` id pairs currently on the recursion
//! stack and short-circuits on re-entry. This is sound: re-entering a pair means it
//! is already contributing its candidates further up the stack, so revisiting adds
//! nothing. (Mirrors the relation engine's assume-true cycle stack and the
//! substitution engine's `in_progress` guard.)

use crate::relate::Relater;
use crate::types::repr::TypeTag;
use crate::types::store::{Store, TypeId};
use crate::types::{substitute, Interner};
use rustc_hash::{FxHashMap, FxHashSet};

/// The accumulating candidate set: for each type parameter, the (already widened)
/// argument types matched against it. A `Vec` rather than a set keeps the code
/// simple; duplicates are collapsed by [`Interner::union`] in [`fix`], so an
/// over-count never affects the fixed result.
pub type Candidates = FxHashMap<crate::types::repr::TypeParamId, Vec<TypeId>>;

/// Collect candidates for a whole call: match each argument type against its
/// corresponding parameter type, positionally, up to the shorter list. A surplus
/// argument or parameter contributes nothing (the M9/M3 arity check reports the
/// count mismatch separately; inference stays silent and sound).
///
/// `params` are the generic signature's parameter types (which contain the
/// `TypeParam`s to be inferred); `args` are the inferred call-argument types.
pub fn collect_call_candidates(
    interner: &mut Interner,
    params: &[TypeId],
    args: &[TypeId],
) -> Candidates {
    let mut candidates: Candidates = FxHashMap::default();
    for (&arg, &param) in args.iter().zip(params) {
        infer_from_types(interner, arg, param, &mut candidates);
    }
    candidates
}

/// Structurally match a source (argument) type against a target (parameter) type,
/// recording candidates for any type parameter found in the target (architecture
/// §5.1). The target is the type that may *contain* type parameters; the source is
/// the concrete argument type they are inferred from.
///
/// Cases (kept deliberately small and sound for M10):
///
///  - target is a **type parameter** → record the (widened) source as a candidate
///    for that parameter id;
///  - **both objects** → for each property present in **both**, recurse on the
///    property types (a property only on one side contributes no candidate);
///  - **both functions** → recurse **positionally** on the parameters (up to the
///    shorter list) and on the return type;
///  - **both unions** → best-effort: recurse **pairwise** on members **only when
///    the two unions have equal length** (a positional pairing); otherwise skip
///    (ambiguous — skipping is sound, it just infers nothing here);
///  - anything else → no candidate.
///
/// The recursion is cycle-guarded (see the module docs) so self-referential
/// argument/parameter types terminate.
pub fn infer_from_types(
    interner: &mut Interner,
    source: TypeId,
    target: TypeId,
    candidates: &mut Candidates,
) {
    let mut ctx = InferenceContext::new();
    ctx.infer(interner, source, target, candidates);
}

/// Structurally match a source against a target in **conditional-`infer` mode** (M25):
/// the same walker as [`infer_from_types`], but literal candidates are **not widened**
/// (tsc keeps `"x"` — review HIGH-1/HIGH-2) and a **union target** descends into its
/// members (review MEDIUM-3). Used by the conditional-type evaluator's extends test;
/// call-site type-argument inference (M10) keeps [`infer_from_types`].
pub fn infer_from_types_for_conditional(
    interner: &mut Interner,
    source: TypeId,
    target: TypeId,
    candidates: &mut Candidates,
) {
    let mut ctx = InferenceContext::for_conditional();
    ctx.infer(interner, source, target, candidates);
}

/// Fix every collected candidate list to a single type, producing the
/// `TypeParamId → TypeId` substitution map. A type parameter with **no** candidate
/// is **omitted** from the map; the caller maps an omitted parameter to `unknown`
/// (the sound fallback) via [`fix_params`], which knows the full parameter list. A
/// single candidate fixes to itself; ≥ 2 distinct candidates fix to their union.
fn fix(interner: &mut Interner, candidates: Candidates) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    let mut map = FxHashMap::default();
    for (param, cands) in candidates {
        let fixed = match cands.len() {
            0 => continue,
            1 => cands[0],
            // ≥ 2: union them (canonicalized — duplicates and order collapse, so
            // two identical candidates fix to that one type, not a 2-member union).
            _ => interner.union(cands),
        };
        map.insert(param, fixed);
    }
    map
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
    type_params: &[crate::types::repr::TypeParamId],
    params: &[TypeId],
    args: &[TypeId],
    fresh_args: &[bool],
) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    // Collect candidates per (arg, param) pair, tracking — per parameter — whether it
    // received a candidate from a FRESH argument and from a NON-fresh one (provenance
    // for the clamp exemption: exempt iff fresh-only).
    let mut candidates: Candidates = FxHashMap::default();
    let mut fresh_params: FxHashSet<crate::types::repr::TypeParamId> = FxHashSet::default();
    let mut nonfresh_params: FxHashSet<crate::types::repr::TypeParamId> = FxHashSet::default();
    for (index, (&arg, &param)) in args.iter().zip(params).enumerate() {
        let mut local: Candidates = FxHashMap::default();
        infer_from_types(interner, arg, param, &mut local);
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
    // Exempt = candidates came from fresh literals ONLY (review F4).
    let exempt: FxHashSet<crate::types::repr::TypeParamId> = fresh_params
        .difference(&nonfresh_params)
        .copied()
        .collect();
    let fixed = fix(interner, candidates);
    fix_params(interner, type_params, fixed, &exempt)
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
        let constraint = raw_constraint.map(|c| substitute(interner, c, &map));

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

/// The per-inference recursion state: the cycle guard for structural matching, plus
/// the **collection mode** (M25). Built once per entry-point call and dropped after.
struct InferenceContext {
    /// `(source, target)` id pairs currently on the recursion stack. Re-entering a
    /// pair short-circuits (it is already contributing candidates further up). See
    /// the module docs.
    visited: FxHashSet<(TypeId, TypeId)>,
    /// Widen literal candidates to their base (`5` → `number`) — the M10 **call-site**
    /// rule (an inferred type argument is never narrower than the value denotes).
    /// Conditional-type `infer` extraction (M25) must NOT widen: tsc keeps the literal
    /// (`ElementOf<"x"[]>` is `"x"`), and widening a contravariant-position candidate
    /// even corrupts the extends test itself (review findings HIGH-1/HIGH-2).
    widen_literals: bool,
    /// Descend into a union **target**'s members with a non-union source (M25,
    /// review MEDIUM-3): `number[]` against `string | (infer U)[]` collects `U` from the
    /// array member (same-name contributions union). Call-site inference keeps only the
    /// M10 equal-length pairwise union rule (no behavior change for m10).
    union_target_descent: bool,
}

impl InferenceContext {
    /// Call-site mode (M10): literal candidates widen; no union-target descent.
    fn new() -> Self {
        InferenceContext {
            visited: FxHashSet::default(),
            widen_literals: true,
            union_target_descent: false,
        }
    }

    /// Conditional-`infer` mode (M25): literals never widen; a union extends target
    /// collects from its members.
    fn for_conditional() -> Self {
        InferenceContext {
            visited: FxHashSet::default(),
            widen_literals: false,
            union_target_descent: true,
        }
    }

    /// Match `source` against `target`, recording candidates. The dispatch on the
    /// target's tag is what decides whether a type parameter is being inferred
    /// (target is a `TypeParam`) or whether to recurse structurally.
    fn infer(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        // A target type parameter is the one place a candidate is recorded.
        if interner.store().tag(target) == TypeTag::TypeParam {
            if let Some(param) = interner.store().type_param(target) {
                let id = param.id;
                // Call-site mode widens the candidate (literal → base) so the inferred
                // argument is never narrower than the value's type denotes — sound
                // (widening only goes wider) and matches the conventional top-level
                // result. Conditional-`infer` mode records the source AS-IS (tsc never
                // widens there — M25 review HIGH-1/HIGH-2).
                let candidate = if self.widen_literals {
                    widen(interner, source)
                } else {
                    source
                };
                candidates.entry(id).or_default().push(candidate);
            }
            return;
        }

        // Structural recursion is cycle-guarded: re-entering an in-flight
        // (source, target) pair adds nothing, so short-circuit.
        if !self.visited.insert((source, target)) {
            return;
        }

        // M25 (conditional mode): a union TARGET with a non-union source descends into
        // every member — a member the source does not shape-match contributes nothing,
        // and same-name contributions union. A union source keeps the pairwise arm.
        //
        // A **naked** infer member (`{ v: infer U } | infer U`) records the whole check
        // type as a LOW-priority candidate (tsc's inference priorities): it is DISCARDED
        // when any structural member of this union bound the same binder, and kept only
        // when no structural member did (`string | infer U` — the naked-only shape).
        // Structural members are therefore collected first (into a local set, so the
        // drop decision keys on THIS union's contributions), then merged.
        if self.union_target_descent
            && interner.store().tag(target) == TypeTag::Union
            && interner.store().tag(source) != TypeTag::Union
        {
            let members: Vec<TypeId> = interner
                .store()
                .union_members(target)
                .map(|m| m.to_vec())
                .unwrap_or_default();
            let (naked, structural): (Vec<TypeId>, Vec<TypeId>) = members
                .into_iter()
                .partition(|&m| interner.store().tag(m) == TypeTag::TypeParam);
            let mut structural_cands = Candidates::default();
            for member in structural {
                self.infer(interner, source, member, &mut structural_cands);
            }
            for member in naked {
                let bound_structurally = interner
                    .store()
                    .type_param(member)
                    .is_some_and(|p| structural_cands.get(&p.id).is_some_and(|c| !c.is_empty()));
                if bound_structurally {
                    continue;
                }
                // Kept: the ordinary TypeParam-target arm records it (mode-gated widen).
                self.infer(interner, source, member, candidates);
            }
            for (id, cands) in structural_cands {
                candidates.entry(id).or_default().extend(cands);
            }
            self.visited.remove(&(source, target));
            return;
        }

        match (interner.store().tag(source), interner.store().tag(target)) {
            (TypeTag::Object, TypeTag::Object) => {
                self.infer_objects(interner, source, target, candidates);
            }
            (TypeTag::Function, TypeTag::Function) => {
                self.infer_functions(interner, source, target, candidates);
            }
            (TypeTag::Union, TypeTag::Union) => {
                self.infer_unions(interner, source, target, candidates);
            }
            // M17: both arrays → recurse on the element (so a `T[]` parameter infers
            // `T` from a `number[]` argument, exactly as the object/function arms do
            // for their children). The element is itself interned, so this nests for
            // `T[][]`.
            (TypeTag::Array, TypeTag::Array) => {
                self.infer_arrays(interner, source, target, candidates);
            }
            // M25 (conditional-type `infer`): both tuples → recurse **positionally** on
            // each element up to the shorter list, exactly like function parameters. A
            // `[infer H, infer R]` extends target infers `H`/`R` from a `[string,
            // number]` check. (Reuses the shared inference machinery — the same one M10
            // uses for call-argument inference.)
            (TypeTag::Tuple, TypeTag::Tuple) => {
                self.infer_tuples(interner, source, target, candidates);
            }
            // Any other pairing (scalar, mismatched shapes, error type, …) yields
            // no candidate — inference simply learns nothing from it. Soundness is
            // preserved: the subsequent relation check still runs against whatever
            // the other parameters fixed to.
            _ => {}
        }

        self.visited.remove(&(source, target));
    }

    /// Both objects: recurse on the type of each property present in **both**. A
    /// property only on the source, or only on the target (e.g. a generic
    /// `{ value: T }` whose argument lacks `value`), contributes no candidate.
    fn infer_objects(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        // Snapshot the (name, type) pairs before recursing — the recursive `infer`
        // takes `&mut Interner` (it may intern a union while fixing widened
        // candidates), which cannot overlap the immutable side-table borrow.
        let Some(source_pairs) = property_pairs(interner.store(), source) else {
            return;
        };
        let Some(target_pairs) = property_pairs(interner.store(), target) else {
            return;
        };
        for (name, target_ty) in &target_pairs {
            if let Some((_, source_ty)) = source_pairs.iter().find(|(n, _)| n == name) {
                self.infer(interner, *source_ty, *target_ty, candidates);
            }
        }
    }

    /// Both arrays (M17): recurse on the **element** (`number[]` against `T[]`
    /// infers `T = number`). The element is interned, so the recursion is finite and
    /// nests naturally for `T[][]`.
    fn infer_arrays(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let Some(source_elem) = interner.store().array_type(source).map(|a| a.element) else {
            return;
        };
        let Some(target_elem) = interner.store().array_type(target).map(|a| a.element) else {
            return;
        };
        self.infer(interner, source_elem, target_elem, candidates);
    }

    /// Both tuples (M25): recurse **positionally** on each element up to the shorter
    /// list. A `[infer H, infer R]` target infers `H`/`R` from a `[string, number]`
    /// source; a length mismatch just pairs the common prefix (sound — a surplus
    /// element contributes nothing).
    fn infer_tuples(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let source_elems: Vec<TypeId> = match interner.store().tuple_type(source) {
            Some(t) => t.elements.clone(),
            None => return,
        };
        let target_elems: Vec<TypeId> = match interner.store().tuple_type(target) {
            Some(t) => t.elements.clone(),
            None => return,
        };
        for (&source_ty, &target_ty) in source_elems.iter().zip(&target_elems) {
            self.infer(interner, source_ty, target_ty, candidates);
        }
    }

    /// Both functions: recurse **positionally** on parameters (up to the shorter
    /// list) and on the return type. Parameter pairing is positional because that
    /// is how the relation engine compares function parameters (M3); a surplus
    /// parameter on either side contributes nothing.
    fn infer_functions(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let Some((source_params, source_ret)) = function_shape(interner.store(), source) else {
            return;
        };
        let Some((target_params, target_ret)) = function_shape(interner.store(), target) else {
            return;
        };
        for (&source_ty, &target_ty) in source_params.iter().zip(&target_params) {
            self.infer(interner, source_ty, target_ty, candidates);
        }
        self.infer(interner, source_ret, target_ret, candidates);
    }

    /// Both unions: a best-effort pairwise match. Only when the two unions have the
    /// **same number of members** are the (canonical-order) members paired
    /// positionally and recursed; an unequal count is ambiguous, so nothing is
    /// inferred (sound — skipping never invents a wrong candidate). This is kept
    /// intentionally simple for M10; precise union inference is a later milestone.
    fn infer_unions(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let source_members: Vec<TypeId> = match interner.store().union_members(source) {
            Some(m) => m.to_vec(),
            None => return,
        };
        let target_members: Vec<TypeId> = match interner.store().union_members(target) {
            Some(m) => m.to_vec(),
            None => return,
        };
        if source_members.len() != target_members.len() {
            return;
        }
        for (&source_ty, &target_ty) in source_members.iter().zip(&target_members) {
            self.infer(interner, source_ty, target_ty, candidates);
        }
    }
}

/// The `(name, type)` pairs of an object type, or `None` if `ty` is not an object.
fn property_pairs(store: &Store, ty: TypeId) -> Option<Vec<(String, TypeId)>> {
    store
        .object_type(ty)
        .map(|object| object.properties.iter().map(|p| (p.name.clone(), p.ty)).collect())
}

/// The (parameter types, return type) of a function type, or `None` if `ty` is not
/// a function. Parameters are returned positionally (the relation/inference order).
fn function_shape(store: &Store, ty: TypeId) -> Option<(Vec<TypeId>, TypeId)> {
    store
        .function_type(ty)
        .map(|function| (function.params.iter().map(|p| p.ty).collect(), function.ret))
}

/// Widen a candidate: a literal widens to its base intrinsic (`5` → `number`);
/// every other type passes through unchanged. Mirrors the checker's `widen` (kept
/// local so the inference engine has no back-dependency on the checker module).
fn widen(interner: &Interner, ty: TypeId) -> TypeId {
    match interner.store().literal_value(ty) {
        Some(lit) => intrinsic_id(interner, lit.base_kind()),
        None => ty,
    }
}

/// Well-known id for an intrinsic kind (the literal-widening targets only need the
/// three primitive bases, but the full mapping keeps the match exhaustive).
fn intrinsic_id(interner: &Interner, kind: crate::types::repr::IntrinsicKind) -> TypeId {
    use crate::types::repr::IntrinsicKind;
    let wk = interner.well_known();
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{
        FunctionType, LiteralValue, ObjectType, ParameterType, PropertyType, TypeParamId,
    };

    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType::public(name, ty)
    }

    /// A bare scalar argument matched against a type parameter fixes that parameter
    /// to the (widened) argument type: `identity(5)` infers `T = number`.
    #[test]
    fn infers_from_scalar_argument() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        // The argument `5` is a literal type.
        let five = interner.intern_literal(LiteralValue::Number(5.0));

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[t], &[five], &[]);
        assert_eq!(
            map.get(&TypeParamId(0)).copied(),
            Some(wk.number),
            "T inferred from `5` widens to number"
        );
    }

    /// M25 — the **conditional collection mode**: literal candidates are NOT widened
    /// (`"x"` stays `"x"`; call-site mode widens — pinned by
    /// [`infers_from_scalar_argument`] above), and a **union target** descends into its
    /// members (`number[]` against `string | T[]` infers `T = number`).
    #[test]
    fn conditional_mode_keeps_literals_and_descends_union_targets() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let x = interner.intern_literal(LiteralValue::String("x".to_string()));

        // No widening: `"x"` against `T` records the literal itself.
        let mut candidates = Candidates::default();
        infer_from_types_for_conditional(&mut interner, x, t, &mut candidates);
        assert_eq!(
            candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
            Some(&[x][..]),
            "conditional mode must record the un-widened literal"
        );

        // Union-target descent: `number[]` against `string | T[]` lands `T = number`
        // via the array member (the string member contributes nothing).
        let t_arr = interner.intern_array(t);
        let union_target = interner.union(vec![wk.string, t_arr]);
        let num_arr = interner.intern_array(wk.number);
        let mut candidates = Candidates::default();
        infer_from_types_for_conditional(&mut interner, num_arr, union_target, &mut candidates);
        assert_eq!(
            candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
            Some(&[wk.number][..]),
            "a union extends target must collect from its shape-matching member"
        );

        // The CALL-site entry point on the same union shape stays M10-conservative
        // (non-union source vs union target infers nothing — no m10 behavior change).
        let mut candidates = Candidates::default();
        infer_from_types(&mut interner, num_arr, union_target, &mut candidates);
        assert!(
            candidates.is_empty(),
            "call-site mode must not descend into union targets (M10 unchanged)"
        );
    }

    /// M25 round-4 — a **naked** infer union member's whole-check candidate is LOW
    /// priority: it is DISCARDED when a structural member of the same union bound the
    /// same binder (`{ v: T } | T` against `{ v: string }` → `T = string`, not
    /// `string | { v: string }`), and KEPT when no structural member did
    /// (`string | T` against `number` → `T = number`). A different-name naked member
    /// never blocks a structural binder (`A | B[]`).
    #[test]
    fn naked_union_member_candidate_yields_to_structural() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");

        // `{ v: T } | T` against `{ v: string }` → structural wins: T = string only.
        let v_t = interner.intern_object(ObjectType {
            properties: vec![prop("v", t)],
            ..Default::default()
        });
        let target = interner.union(vec![v_t, t]);
        let v_str = interner.intern_object(ObjectType {
            properties: vec![prop("v", wk.string)],
            ..Default::default()
        });
        let mut candidates = Candidates::default();
        infer_from_types_for_conditional(&mut interner, v_str, target, &mut candidates);
        assert_eq!(
            candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
            Some(&[wk.string][..]),
            "the naked member's whole-check candidate must be dropped"
        );

        // `string | T` against `number` → naked-only: the whole check IS the candidate.
        let target = interner.union(vec![wk.string, t]);
        let mut candidates = Candidates::default();
        infer_from_types_for_conditional(&mut interner, wk.number, target, &mut candidates);
        assert_eq!(
            candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
            Some(&[wk.number][..]),
            "a naked-only member keeps its whole-check candidate"
        );

        // `A | B[]` against `number[]`: B binds structurally; the naked A (a DIFFERENT
        // binder) still records the whole check — no cross-binder blocking.
        let a = interner.intern_type_param(TypeParamId(1), "A");
        let b = interner.intern_type_param(TypeParamId(2), "B");
        let b_arr = interner.intern_array(b);
        let target = interner.union(vec![a, b_arr]);
        let num_arr = interner.intern_array(wk.number);
        let mut candidates = Candidates::default();
        infer_from_types_for_conditional(&mut interner, num_arr, target, &mut candidates);
        assert_eq!(
            candidates.get(&TypeParamId(2)).map(|c| c.as_slice()),
            Some(&[wk.number][..]),
            "B = number from the structural member"
        );
        assert_eq!(
            candidates.get(&TypeParamId(1)).map(|c| c.as_slice()),
            Some(&[num_arr][..]),
            "A (different name) keeps its naked whole-check candidate"
        );
    }

    /// A candidate matched against a non-generic parameter is not recorded, but the
    /// return-bearing parameter still infers: `pick(1, \"x\")` infers `A = number`,
    /// `B = string` (each from its own parameter).
    #[test]
    fn infers_each_parameter_independently() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let a = interner.intern_type_param(TypeParamId(0), "A");
        let b = interner.intern_type_param(TypeParamId(1), "B");
        let one = interner.intern_literal(LiteralValue::Number(1.0));
        let x = interner.intern_literal(LiteralValue::String("x".to_string()));

        let map = infer_type_arguments(
            &mut interner,
            &[TypeParamId(0), TypeParamId(1)],
            &[a, b],
            &[one, x],
            &[],
        );
        assert_eq!(map.get(&TypeParamId(0)).copied(), Some(wk.number), "A = number");
        assert_eq!(map.get(&TypeParamId(1)).copied(), Some(wk.string), "B = string");
    }

    /// A type parameter nested inside an object parameter is inferred from the
    /// matching property of the argument object: `unwrap({ value: 1 })` with the
    /// parameter `{ value: T }` infers `T = number`. (Object-literal members arrive
    /// already widened, so the candidate is `number` here.)
    #[test]
    fn infers_from_object_property() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        // Parameter `{ value: T }`.
        let box_t = interner.intern_object(ObjectType {
            properties: vec![prop("value", t)],
            ..Default::default()
        });
        // Argument `{ value: number }` (member already widened by the checker).
        let arg = interner.intern_object(ObjectType {
            properties: vec![prop("value", wk.number)],
            ..Default::default()
        });

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[box_t], &[arg], &[]);
        assert_eq!(map.get(&TypeParamId(0)).copied(), Some(wk.number), "T = number");
    }

    /// A type parameter under a function parameter is inferred from both the
    /// parameter positions and the return type.
    #[test]
    fn infers_through_function_parameter() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        // Parameter type `(x: T) => T`.
        let target = interner.intern_function(FunctionType {
            params: vec![ParameterType {
                name: "x".to_string(),
                ty: t,
                optional: false,
            }],
            ret: t,
        });
        // Argument type `(x: number) => number`.
        let source = interner.intern_function(FunctionType {
            params: vec![ParameterType {
                name: "x".to_string(),
                ty: wk.number,
                optional: false,
            }],
            ret: wk.number,
        });

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[target], &[source], &[]);
        assert_eq!(map.get(&TypeParamId(0)).copied(), Some(wk.number), "T = number");
    }

    /// Two distinct candidates for one type parameter fix to their **union**:
    /// `both(1, \"s\")` (both parameters typed `T`) infers `T = number | string`.
    #[test]
    fn multiple_distinct_candidates_union() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let one = interner.intern_literal(LiteralValue::Number(1.0));
        let s = interner.intern_literal(LiteralValue::String("s".to_string()));
        let expected = interner.union(vec![wk.number, wk.string]);

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[t, t], &[one, s], &[]);
        assert_eq!(
            map.get(&TypeParamId(0)).copied(),
            Some(expected),
            "T = number | string"
        );
    }

    /// Two **equal** candidates collapse to that one type (not a 2-member union):
    /// `both(1, 2)` infers `T = number`.
    #[test]
    fn duplicate_candidates_collapse() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let one = interner.intern_literal(LiteralValue::Number(1.0));
        let two = interner.intern_literal(LiteralValue::Number(2.0));

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[t, t], &[one, two], &[]);
        assert_eq!(
            map.get(&TypeParamId(0)).copied(),
            Some(wk.number),
            "T = number (duplicates collapse)"
        );
    }

    /// A type parameter with **no** candidate falls back to `unknown` (the sound
    /// fallback), never `any`. Here the argument shape (a scalar) does not match the
    /// parameter shape (an object), so nothing is inferred for `T`.
    #[test]
    fn no_candidate_falls_back_to_unknown() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        // Parameter `{ value: T }`; argument is a bare `number` (shape mismatch).
        let box_t = interner.intern_object(ObjectType {
            properties: vec![prop("value", t)],
            ..Default::default()
        });

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[box_t], &[wk.number], &[]);
        assert_eq!(
            map.get(&TypeParamId(0)).copied(),
            Some(wk.unknown),
            "no candidate → unknown, never any"
        );
        assert_ne!(map.get(&TypeParamId(0)).copied(), Some(wk.any));
    }

    /// A self-referential argument/parameter pair terminates (cycle guard): a
    /// recursive nominal object matched against itself does not loop.
    #[test]
    fn self_referential_types_terminate() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        // A recursive nominal `List { head: number; tail: List | null }`.
        let list = interner.reserve_object();
        let list_or_null = interner.union(vec![list, wk.null]);
        interner.fill_object(
            list,
            ObjectType {
                properties: vec![prop("head", wk.number), prop("tail", list_or_null)],
                ..Default::default()
            },
        );

        // Matching `list` against itself must terminate; it has no type parameter,
        // so it infers nothing.
        let map = infer_type_arguments(&mut interner, &[], &[list], &[list], &[]);
        assert!(map.is_empty(), "no type params → empty map, and no infinite loop");
    }
}

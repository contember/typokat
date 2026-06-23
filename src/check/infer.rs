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

use crate::types::repr::TypeTag;
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
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
/// candidates if any landed, otherwise **`unknown`** (the sound no-candidate
/// fallback — `unknown` cannot mask a downstream error the way `any` would). The
/// returned map is fed straight to the existing M9 substitution.
pub fn infer_type_arguments(
    interner: &mut Interner,
    type_params: &[crate::types::repr::TypeParamId],
    params: &[TypeId],
    args: &[TypeId],
) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    let candidates = collect_call_candidates(interner, params, args);
    let fixed = fix(interner, candidates);
    fix_params(interner, type_params, fixed)
}

/// Complete a partially-fixed map to cover **every** declared type parameter: a
/// parameter already fixed (it had candidates) keeps its type; a parameter with no
/// candidate maps to **`unknown`**. Keying off the full `type_params` list (not the
/// candidate map) is what guarantees an un-inferred parameter falls back soundly
/// rather than silently surviving unsubstituted.
fn fix_params(
    interner: &Interner,
    type_params: &[crate::types::repr::TypeParamId],
    mut fixed: FxHashMap<crate::types::repr::TypeParamId, TypeId>,
) -> FxHashMap<crate::types::repr::TypeParamId, TypeId> {
    let unknown = interner.well_known().unknown;
    for &param in type_params {
        fixed.entry(param).or_insert(unknown);
    }
    fixed
}

/// The per-inference recursion state: the cycle guard for structural matching.
/// Built once per [`infer_from_types`] call and dropped after.
struct InferenceContext {
    /// `(source, target)` id pairs currently on the recursion stack. Re-entering a
    /// pair short-circuits (it is already contributing candidates further up). See
    /// the module docs.
    visited: FxHashSet<(TypeId, TypeId)>,
}

impl InferenceContext {
    fn new() -> Self {
        InferenceContext {
            visited: FxHashSet::default(),
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
                // Widen the candidate (literal → base) so the inferred argument is
                // never narrower than the value's type denotes — sound (widening
                // only goes wider) and matches the conventional top-level result.
                let widened = widen(interner, source);
                candidates.entry(id).or_default().push(widened);
            }
            return;
        }

        // Structural recursion is cycle-guarded: re-entering an in-flight
        // (source, target) pair adds nothing, so short-circuit.
        if !self.visited.insert((source, target)) {
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

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[t], &[five]);
        assert_eq!(
            map.get(&TypeParamId(0)).copied(),
            Some(wk.number),
            "T inferred from `5` widens to number"
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
        });
        // Argument `{ value: number }` (member already widened by the checker).
        let arg = interner.intern_object(ObjectType {
            properties: vec![prop("value", wk.number)],
        });

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[box_t], &[arg]);
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

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[target], &[source]);
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

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[t, t], &[one, s]);
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

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[t, t], &[one, two]);
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
        });

        let map = infer_type_arguments(&mut interner, &[TypeParamId(0)], &[box_t], &[wk.number]);
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
            },
        );

        // Matching `list` against itself must terminate; it has no type parameter,
        // so it infers nothing.
        let map = infer_type_arguments(&mut interner, &[], &[list], &[list]);
        assert!(map.is_empty(), "no type params → empty map, and no infinite loop");
    }
}

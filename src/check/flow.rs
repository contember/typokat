//! Flow analysis & narrowing operations (architecture §5, mvp-plan §3/§5).
//!
//! Control-flow narrowing (`typeof`, `in`, truthiness, equality, discriminated
//! unions, assertion functions) is *the* reason people love TS DX and must never
//! be sacrificed. It is flow-sensitive and needs a native interpreter model, so
//! the checker is structured around flow facts from the start (mvp-plan §5).
//!
//! ## What lives here (M7, extended in M8)
//!
//! The **reusable narrowing operations**: pure `(type, polarity) -> type`
//! functions that refine a union by filtering its members (typeof tag,
//! truthiness, `null`/`undefined` equality; M8 adds the **literal discriminant**
//! `x.prop === <lit>` and the **`in`-operator** `"prop" in x`). They take a
//! [`crate::types::Interner`] so the narrowed result is re-interned through
//! [`crate::types::Interner::union`] — `string | number` minus `number` collapses
//! to `string`, `{a} | null` minus `null` collapses to `{a}`. These are
//! intentionally **flow-model-agnostic**: they know nothing about `if`/`else`/
//! `switch`, scopes, or symbols, so they survive unchanged when the
//! structured-statement driver (in `checker.rs`) is eventually replaced by a full
//! flow-node CFG (see below).
//!
//! ## What is M7's structured-flow slice (in `checker.rs`, not here)
//!
//! M7 implements narrowing as a **structured narrowing environment** layered on the
//! existing in-order statement walk — the first, structured-control-flow slice of
//! the §5 flow interpreter. The driver (a `SymbolId -> TypeId` overlay, the
//! `if`/`else` fork-and-restore, and the guard analysis that turns a condition into
//! a `GuardFact`) lives in `checker.rs` because it is specific to the structured
//! walk. Only the operations below are general.
//!
//! ## Deferred to M8+ (the full flow-node CFG)
//!
//! [`FlowNode`] is the eventual general model for **unstructured** flow that the
//! structured driver cannot express: narrowing via early `return`/`throw`
//! (control-flow join), loops, and definite-assignment (`TK2454`) / reachability
//! (`TK2355`). When that lands, the binder/checker will build a flow graph and the
//! narrowing pass will refine a declared type along a given flow path — reusing the
//! operations in this module verbatim. The following narrowings are still deferred
//! (mvp-plan, README "Deferred checks"): assertion functions / type predicates
//! (`x is T`), `typeof`-discriminant combined with `in`, non-literal discriminants,
//! and `&&`/`||`/ternary condition narrowing. (M8 added the literal-discriminant,
//! `in`-operator, and `switch` narrowings via the structured driver in `checker.rs`.)

use crate::relate::{Relater, Relation};
use crate::types::repr::{IntrinsicKind, LiteralValue, TypeTag};
use crate::types::store::TypeId;
use crate::types::Interner;

/// A node in the control-flow graph. Reserved shape for the **unstructured**-flow
/// milestone (M8+); the structured M7 slice does not construct it (its driver is
/// the in-order statement walk in `checker.rs`).
#[allow(dead_code)] // TODO(M8): produced by the binder/checker flow pass.
pub enum FlowNode {
    /// The unreachable start sentinel.
    Unreachable,
    /// Flow start (function/module entry).
    Start,
    // TODO(M8): Assignment, Condition (true/false branch), Loop, Join, Call, …
}

/// The `typeof` tags M7 recognizes. `typeof x === "string" | "number" |
/// "boolean"` are the in-subset tags; `"object"`, `"function"`, `"undefined"`,
/// `"symbol"`, `"bigint"` are out of the M7 subset (an unrecognized tag yields no
/// guard fact, so it narrows nothing — sound).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TypeofTag {
    String,
    Number,
    Boolean,
}

impl TypeofTag {
    /// Parse a `typeof` comparison string literal into a recognized tag, or `None`
    /// for an out-of-subset tag (`"object"`, …). The unrecognized case must yield
    /// no narrowing, so the caller treats `None` as "not a guard".
    pub fn from_tag_literal(s: &str) -> Option<TypeofTag> {
        match s {
            "string" => Some(TypeofTag::String),
            "number" => Some(TypeofTag::Number),
            "boolean" => Some(TypeofTag::Boolean),
            _ => None,
        }
    }

    /// The intrinsic this tag selects, so a non-union (already-narrowed) operand
    /// can be matched against it too.
    fn intrinsic(self) -> IntrinsicKind {
        match self {
            TypeofTag::String => IntrinsicKind::String,
            TypeofTag::Number => IntrinsicKind::Number,
            TypeofTag::Boolean => IntrinsicKind::Boolean,
        }
    }
}

/// A recognized narrowing operation: the *shape* of the guard, independent of the
/// branch polarity (the driver applies `positive` for the then-branch and its
/// negation for the else-branch). Pairing one of these with a target `SymbolId`
/// (done in `checker.rs`) makes a guard fact — the symbol-keying that keeps
/// narrowing from leaking across symbols.
///
/// Not `Copy`: the M8 discriminant/`in` variants carry the property name (a
/// `String`). The driver passes it to [`narrow`] **by reference**, so a guard fact
/// is reused across both branches without cloning.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NarrowOp {
    /// `typeof x === <tag>` (the positive sense). The then-branch keeps the
    /// members matching the tag; the else-branch keeps the complement.
    Typeof(TypeofTag),
    /// Truthiness (`if (x)`, positive sense). The then-branch removes the always-
    /// falsy members (`null`/`undefined`); the else-branch removes the always-
    /// truthy members (objects/functions). Per the M7 subset, falsy-capable
    /// primitives (`string`/`number`/`boolean`/literals) are **not** split, so
    /// they survive into both branches (sound: each branch stays a superset of the
    /// precise narrowing).
    Truthy,
    /// `x === null` / `x === undefined` (the positive sense). The then-branch keeps
    /// only that nullish member; the else-branch removes it. The target nullish
    /// intrinsic is identified by `is_undefined`.
    EqNullish { is_undefined: bool },
    /// **Literal discriminant** (M8): `x.prop === <literal>` (the positive sense).
    /// The then-branch keeps the union members whose `prop` *could* equal the
    /// literal; the else-branch keeps the complement (the members whose `prop` can
    /// never be anything but the literal are removed). `property` is the
    /// discriminant property name; `literal` is the interned literal `TypeId` it is
    /// compared against. See [`narrow_by_discriminant`] for the exact (sound) rule.
    Discriminant { property: String, literal: TypeId },
    /// **`in`-operator** (M8): `"prop" in x` (the positive sense). The then-branch
    /// keeps the union members that *have* `prop`; the else-branch keeps those that
    /// can *lack* it. See [`narrow_by_in_operator`] for the exact (sound) rule.
    In { property: String },
}

/// Apply a narrowing operation to `ty` under a branch polarity, returning the
/// refined type (re-interned through [`Interner::union`]). `positive` is `true` in
/// the branch where the guard holds as written, `false` in the complementary
/// branch — the driver passes `true` for the then-branch and the polarity-flipped
/// value for the else-branch (and folds any `!` in the condition into it).
///
/// `op` is borrowed (not consumed) so the driver applies the same fact to both
/// branches. Every narrowing filters the **union members** of `ty` and re-interns,
/// so a removed member collapses the union (`string | number` minus `number` =
/// `string`; `{a} | null` minus `null` = `{a}`). A non-union `ty` is treated as a
/// one-member set, so the operations are total and compose with prior narrowings.
pub fn narrow(interner: &mut Interner, ty: TypeId, op: &NarrowOp, positive: bool) -> TypeId {
    match op {
        NarrowOp::Typeof(tag) => narrow_by_typeof(interner, ty, *tag, positive),
        NarrowOp::Truthy => narrow_by_truthiness(interner, ty, positive),
        NarrowOp::EqNullish { is_undefined } => {
            narrow_by_nullish_equality(interner, ty, *is_undefined, positive)
        }
        NarrowOp::Discriminant { property, literal } => {
            narrow_by_discriminant(interner, ty, property, *literal, positive)
        }
        NarrowOp::In { property } => narrow_by_in_operator(interner, ty, property, positive),
    }
}

/// Narrow `ty` by `typeof x === <tag>` (architecture §5).
///
///  - **then-branch** (`keep == true`): keep the members whose `typeof` tag is
///    `tag` — i.e. the matching primitive/literal members.
///  - **else-branch** (`keep == false`): keep the complement (everything that does
///    *not* match the tag).
///
/// A member matches a tag when it is the tag's base intrinsic or a literal of that
/// base (e.g. `"x"` matches `typeof === "string"`). Object/function/`null`/
/// `undefined`/`unknown` members never match a primitive tag. `any` is left
/// untouched (it is not a union; narrowing `any` keeps `any` — sound, and matches
/// tsc treating `any` as un-narrowable by `typeof`).
pub fn narrow_by_typeof(
    interner: &mut Interner,
    ty: TypeId,
    tag: TypeofTag,
    keep: bool,
) -> TypeId {
    filter_union(interner, ty, |store, member| {
        let matches = member_matches_typeof(store, member, tag);
        // Keep matching members in the then-branch, the complement in the else.
        matches == keep
    })
}

/// Narrow `ty` by truthiness (architecture §5).
///
///  - **then-branch** (`truthy == true`): remove the always-falsy members
///    (`null`, `undefined`).
///  - **else-branch** (`truthy == false`): remove the always-truthy members
///    (object and function types — and the error type, which behaves like `any`
///    and should not survive into a falsy branch as a spurious object).
///
/// Falsy-capable primitives (`string`/`number`/`boolean` and their literals) are
/// deliberately **not** split (mvp-plan M7 subset): they survive into both
/// branches. This keeps each branch a *superset* of the precise narrowing, so the
/// result is sound (an over-wide branch can only over-report, never under-report).
pub fn narrow_by_truthiness(interner: &mut Interner, ty: TypeId, truthy: bool) -> TypeId {
    let wk = interner.well_known();
    filter_union(interner, ty, |store, member| {
        if truthy {
            // Drop the always-falsy members.
            member != wk.null && member != wk.undefined
        } else {
            // Drop the always-truthy members (objects/functions are always truthy).
            !is_always_truthy(store, member)
        }
    })
}

/// Narrow `ty` by `x === null` / `x === undefined` (architecture §5).
///
///  - **then-branch** (`positive == true`): keep only the targeted nullish member
///    (`null` or `undefined`). If the union does not contain it, the result is
///    `never` — exactly tsc's verdict (the value cannot equal it).
///  - **else-branch** (`positive == false`): remove the targeted nullish member
///    (`{a} | null` minus `null` = `{a}`).
fn narrow_by_nullish_equality(
    interner: &mut Interner,
    ty: TypeId,
    is_undefined: bool,
    positive: bool,
) -> TypeId {
    let wk = interner.well_known();
    let target = if is_undefined { wk.undefined } else { wk.null };
    filter_union(interner, ty, |_store, member| {
        let is_target = member == target;
        // Keep only the target in the then-branch; drop it in the else-branch.
        is_target == positive
    })
}

/// Narrow `ty` by a **literal discriminant** `x.prop === <literal>` (M8,
/// architecture §5 — discriminated unions).
///
///  - **then-branch** (`positive == true`): keep a member `m` unless we can
///    *prove* it can never satisfy `m.prop === literal`. We can prove that only
///    when `m` has the (required) property `prop` and `literal` is **not**
///    assignable to `m.prop`'s type (so `literal` is not one of `m.prop`'s
///    possible values, e.g. `m.prop` is `"square"` and `literal` is `"circle"`).
///    Everything else is **kept**: a member with a wide `prop` (`string`) that
///    `literal` is assignable to, a member that *lacks* `prop`, or a non-object
///    member — none can be ruled out, so keeping them is sound (the branch stays a
///    superset of the precise narrowing → never a false negative).
///  - **else-branch** (`positive == false`): keep a member `m` unless `m.prop` is
///    **exactly** the literal (`m.prop == literal` by interned id, the only case
///    where `m.prop === literal` *always* holds, so `!==` excludes it). A member
///    with a wider `prop`, a member lacking `prop`, and a non-object member are all
///    kept (they can satisfy `x.prop !== literal`).
///
/// Soundness rests on hash-consing: literal types are interned, so two equal
/// literals share one `TypeId`. The "could equal" test reuses the relation engine
/// (`literal <: m.prop`), so a discriminant against a base or union type behaves
/// correctly. A fresh [`Relater`] is built on the interner's store (its own cache,
/// no shared state) and dropped before the mutable re-intern.
pub fn narrow_by_discriminant(
    interner: &mut Interner,
    ty: TypeId,
    property: &str,
    literal: TypeId,
    positive: bool,
) -> TypeId {
    let wk = interner.well_known();
    // `any`/error are un-narrowable tops: leave them as-is (never collapse).
    if ty == wk.any || ty == wk.error {
        return ty;
    }

    // Snapshot the member ids before any mutable re-intern. A non-union `ty` is a
    // single-member set (one decision), mirroring `filter_union`.
    let members: Vec<TypeId> = match interner.store().union_members(ty) {
        Some(members) => members.to_vec(),
        None => vec![ty],
    };

    // Decide each member with one relater over the immutable store (its borrow must
    // end before the mutable `union` below).
    let kept: Vec<TypeId> = {
        let store = interner.store();
        let mut relater = Relater::new(store, wk);
        members
            .iter()
            .copied()
            .filter(|&member| {
                let prop_ty = store
                    .object_type(member)
                    .and_then(|obj| obj.property(property))
                    .map(|prop| prop.ty);
                discriminant_keeps(&mut relater, member, prop_ty, literal, wk, positive)
            })
            .collect()
    };

    interner.union(kept)
}

/// Whether the discriminant narrowing **keeps** a member, given its discriminant
/// property type `prop_ty` (`None` if the member lacks the property or is not an
/// object). Pulled out so the (sound) keep rule is unit-testable and stated once.
fn discriminant_keeps(
    relater: &mut Relater<'_>,
    member: TypeId,
    prop_ty: Option<TypeId>,
    literal: TypeId,
    wk: crate::types::WellKnown,
    positive: bool,
) -> bool {
    // A member that is itself `any`/error is un-narrowable: always keep it (it can
    // never be ruled out). A union member is normally never `any`/error (the
    // interner absorbs them), but the guard is defensive.
    if member == wk.any || member == wk.error {
        return true;
    }
    let Some(prop_ty) = prop_ty else {
        // No (resolvable) discriminant property: cannot rule the member out in
        // either branch → keep it (conservative, sound).
        return true;
    };
    if positive {
        // then-branch: drop only when `literal` provably cannot be a value of
        // `m.prop` (so `m.prop === literal` is impossible).
        matches!(relater.is_assignable(literal, prop_ty), Relation::Yes)
    } else {
        // else-branch: drop only when `m.prop` is exactly the literal (so
        // `m.prop === literal` always holds and `!==` excludes the member).
        prop_ty != literal
    }
}

/// Narrow `ty` by the **`in` operator** `"prop" in x` (M8, architecture §5).
///
///  - **then-branch** (`positive == true`): keep a member only if it *has* (or
///    could have) `prop`. Concretely: keep an object member iff it declares `prop`;
///    keep an `any`/error member (un-narrowable). Drop object members that provably
///    lack `prop` and concrete non-object members (a primitive cannot satisfy `in`
///    at runtime), so `"prop" in x` narrows to the members carrying `prop`.
///  - **else-branch** (`positive == false`): keep a member unless it *provably
///    always has* `prop` — i.e. drop only an object member that declares `prop`
///    (all subset properties are required). An object lacking `prop`, an `any`/
///    error member, and a non-object member are all kept (they can satisfy
///    `!("prop" in x)`).
///
/// Keeping the un-prove-able cases (rather than dropping them) is what makes this
/// sound: each branch stays a superset of the precise narrowing, so no real error
/// is silently dropped.
pub fn narrow_by_in_operator(
    interner: &mut Interner,
    ty: TypeId,
    property: &str,
    positive: bool,
) -> TypeId {
    let wk = interner.well_known();
    filter_union(interner, ty, |store, member| {
        // `any`/error members are un-narrowable: keep in both branches. (A union
        // member is normally never `any`/error — the interner absorbs them — but be
        // defensive.)
        if member == wk.any || member == wk.error {
            return true;
        }
        let has_property = store
            .object_type(member)
            .is_some_and(|obj| obj.property(property).is_some());
        let is_object = store.tag(member) == TypeTag::Object;
        if positive {
            // Keep members that have (or might have) the property: object-with-prop,
            // and — handled above — any/error. Drop object-without-prop and concrete
            // non-objects.
            has_property
        } else {
            // Drop only members that provably always have the property (an object
            // declaring it). Keep object-without-prop and non-object members.
            !(is_object && has_property)
        }
    })
}

/// Whether a union member matches a `typeof` tag (its base intrinsic, or a literal
/// of that base).
fn member_matches_typeof(
    store: &crate::types::store::Store,
    member: TypeId,
    tag: TypeofTag,
) -> bool {
    match store.tag(member) {
        TypeTag::Intrinsic => store.intrinsic_kind(member) == Some(tag.intrinsic()),
        TypeTag::Literal => store.literal_value(member).map(LiteralValue::base_kind) == Some(tag.intrinsic()),
        // Objects/functions/unions/type-parameters/arrays/tuples never match a
        // primitive `typeof` tag (a nested union cannot appear as a member after
        // canonicalization, a type parameter only appears inside an uninstantiated
        // generic body, and an array's/tuple's `typeof` is `"object"` — out of the M7
        // tag subset; but the arm is exhaustive and defensive either way).
        TypeTag::Object
        | TypeTag::Function
        | TypeTag::Union
        | TypeTag::TypeParam
        | TypeTag::Array
        | TypeTag::Tuple => false,
    }
}

/// Whether a type is *always* truthy — used to drop members from a falsy branch.
/// Object and function types are always truthy. Everything else (primitives,
/// literals, `null`/`undefined`, `unknown`, `any`/error) is conservatively treated
/// as possibly-falsy so it is **kept** in the falsy branch (sound: never removes a
/// member that could legitimately be falsy).
fn is_always_truthy(store: &crate::types::store::Store, member: TypeId) -> bool {
    // M17: an array value is always truthy in JS (like an object/function), so it is
    // dropped from a falsy branch — a `T[] | null` narrowed by `if (a)` keeps only
    // `T[]` in the truthy branch and `null` in the falsy one.
    matches!(
        store.tag(member),
        TypeTag::Object | TypeTag::Function | TypeTag::Array
    )
}

/// Filter the members of a union `ty` by `keep`, re-interning the survivors through
/// [`Interner::union`] (which canonicalizes and collapses a 1-member result to the
/// member, a 0-member result to `never`).
///
/// A non-union `ty` is treated as a **single-member set**: `keep` is consulted once
/// for `ty` itself, yielding either `ty` unchanged or `never`. This makes every
/// narrowing total and lets it compose with a prior narrowing that already
/// collapsed a union down to one member (nested `if`s). `any`/the error type are
/// returned unchanged without consulting `keep` — they are un-narrowable tops and
/// must not collapse to `never`.
fn filter_union(
    interner: &mut Interner,
    ty: TypeId,
    keep: impl Fn(&crate::types::store::Store, TypeId) -> bool,
) -> TypeId {
    let wk = interner.well_known();
    // `any`/error are un-narrowable: leave them as-is (never collapse to `never`).
    if ty == wk.any || ty == wk.error {
        return ty;
    }

    match interner.store().union_members(ty) {
        Some(members) => {
            // Snapshot the member ids before the mutable re-intern: the predicate
            // borrows the store immutably, `union` needs `&mut`, so the borrow must
            // not be held across it.
            let members: Vec<TypeId> = members.to_vec();
            let kept: Vec<TypeId> = members
                .into_iter()
                .filter(|&m| keep(interner.store(), m))
                .collect();
            interner.union(kept)
        }
        // Non-union: a single-member set. Keep it or collapse to `never`.
        None => {
            if keep(interner.store(), ty) {
                ty
            } else {
                wk.never
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{LiteralValue, ObjectType, PropertyType};

    fn object_a(interner: &mut Interner) -> TypeId {
        let wk = interner.well_known();
        interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("a", wk.number)],
        })
    }

    /// typeof keep + complement over `string | number`.
    #[test]
    fn typeof_keeps_tag_and_complement() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let sn = interner.union(vec![wk.string, wk.number]);

        // then-branch of `typeof x === "string"` keeps `string`.
        assert_eq!(narrow_by_typeof(&mut interner, sn, TypeofTag::String, true), wk.string);
        // else-branch keeps the complement `number`.
        assert_eq!(narrow_by_typeof(&mut interner, sn, TypeofTag::String, false), wk.number);
        // `typeof === "number"` then-branch keeps `number`.
        assert_eq!(narrow_by_typeof(&mut interner, sn, TypeofTag::Number, true), wk.number);
        // `boolean` is absent → then-branch is `never`, else-branch is the whole union.
        assert_eq!(narrow_by_typeof(&mut interner, sn, TypeofTag::Boolean, true), wk.never);
        assert_eq!(narrow_by_typeof(&mut interner, sn, TypeofTag::Boolean, false), sn);
    }

    /// typeof over a >2-member union keeps the remaining members in the complement.
    #[test]
    fn typeof_complement_over_three_member_union_keeps_rest() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let snb = interner.union(vec![wk.string, wk.number, wk.boolean]);

        // else-branch of `typeof === "string"` keeps `number | boolean`.
        let complement = narrow_by_typeof(&mut interner, snb, TypeofTag::String, false);
        let expected = interner.union(vec![wk.number, wk.boolean]);
        assert_eq!(complement, expected);
    }

    /// typeof matches a literal by its base.
    #[test]
    fn typeof_matches_literal_by_base() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let lit = interner.intern_literal(LiteralValue::String("x".to_string()));
        let lit_or_num = interner.union(vec![lit, wk.number]);
        // `typeof === "string"` keeps the `"x"` literal member.
        assert_eq!(narrow_by_typeof(&mut interner, lit_or_num, TypeofTag::String, true), lit);
    }

    /// truthiness removes null/undefined in the then-branch, always-truthy objects
    /// in the else-branch.
    #[test]
    fn truthiness_then_removes_nullish_else_removes_object() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let obj = object_a(&mut interner);

        // `{ a } | null`.
        let obj_or_null = interner.union(vec![obj, wk.null]);
        assert_eq!(narrow_by_truthiness(&mut interner, obj_or_null, true), obj);
        assert_eq!(narrow_by_truthiness(&mut interner, obj_or_null, false), wk.null);

        // `{ a } | undefined`.
        let obj_or_undef = interner.union(vec![obj, wk.undefined]);
        assert_eq!(narrow_by_truthiness(&mut interner, obj_or_undef, true), obj);
        assert_eq!(narrow_by_truthiness(&mut interner, obj_or_undef, false), wk.undefined);
    }

    /// A falsy-capable primitive is NOT split — it survives into both branches.
    #[test]
    fn truthiness_does_not_split_primitives() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        // `string | null`: truthy branch removes null → `string` (string kept,
        // even though `""` is falsy — we don't split). Falsy branch keeps both
        // `string` (possibly-falsy) and `null`.
        let str_or_null = interner.union(vec![wk.string, wk.null]);
        assert_eq!(narrow_by_truthiness(&mut interner, str_or_null, true), wk.string);
        assert_eq!(narrow_by_truthiness(&mut interner, str_or_null, false), str_or_null);
    }

    /// null/undefined equality keep/remove.
    #[test]
    fn nullish_equality_keep_and_remove() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let obj = object_a(&mut interner);

        // `{ a } | null` vs `=== null` / `!== null`.
        let obj_or_null = interner.union(vec![obj, wk.null]);
        // `=== null` positive keeps only null.
        assert_eq!(narrow(&mut interner, obj_or_null, &NarrowOp::EqNullish { is_undefined: false }, true), wk.null);
        // `=== null` negative (else / `!== null` then) removes null.
        assert_eq!(narrow(&mut interner, obj_or_null, &NarrowOp::EqNullish { is_undefined: false }, false), obj);

        // `{ a } | undefined` vs `=== undefined`.
        let obj_or_undef = interner.union(vec![obj, wk.undefined]);
        assert_eq!(narrow(&mut interner, obj_or_undef, &NarrowOp::EqNullish { is_undefined: true }, true), wk.undefined);
        assert_eq!(narrow(&mut interner, obj_or_undef, &NarrowOp::EqNullish { is_undefined: true }, false), obj);
    }

    /// Build `{ <disc>: <disc_ty>; <extra>: number }` — a discriminated-union
    /// member with a discriminant property `disc` of type `disc_ty`.
    fn member(
        interner: &mut Interner,
        disc: &str,
        disc_ty: TypeId,
        extra: &str,
    ) -> TypeId {
        let wk = interner.well_known();
        interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public(disc, disc_ty),
                PropertyType::public(extra, wk.number),
            ],
        })
    }

    /// Discriminant narrowing keeps the matching member in the then-branch and the
    /// complement in the else-branch (the headline discriminated-union behaviour).
    #[test]
    fn discriminant_keeps_match_and_complement() {
        let mut interner = Interner::with_intrinsics();
        let circle_lit = interner.intern_literal(LiteralValue::String("circle".to_string()));
        let square_lit = interner.intern_literal(LiteralValue::String("square".to_string()));
        let circle = member(&mut interner, "kind", circle_lit, "radius");
        let square = member(&mut interner, "kind", square_lit, "side");
        let shape = interner.union(vec![circle, square]);

        // `kind === "circle"` then-branch keeps the circle member.
        assert_eq!(
            narrow_by_discriminant(&mut interner, shape, "kind", circle_lit, true),
            circle
        );
        // else-branch keeps the complement (square).
        assert_eq!(
            narrow_by_discriminant(&mut interner, shape, "kind", circle_lit, false),
            square
        );
        // `kind === "square"` then-branch keeps the square member.
        assert_eq!(
            narrow_by_discriminant(&mut interner, shape, "kind", square_lit, true),
            square
        );
    }

    /// A member with a WIDE discriminant (`string`, not a single literal) cannot be
    /// ruled out in EITHER branch — it survives both (soundness: never a false
    /// negative). A member LACKING the discriminant likewise survives both.
    #[test]
    fn discriminant_keeps_unprovable_members_in_both_branches() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let circle_lit = interner.intern_literal(LiteralValue::String("circle".to_string()));
        let circle = member(&mut interner, "kind", circle_lit, "radius");
        // A member whose `kind` is the wide `string`.
        let wide = member(&mut interner, "kind", wk.string, "extra");
        let shape = interner.union(vec![circle, wide]);

        // then-branch of `kind === "circle"`: `"circle"` is assignable to `string`,
        // so the wide member could match → kept alongside the circle member.
        assert_eq!(
            narrow_by_discriminant(&mut interner, shape, "kind", circle_lit, true),
            shape
        );
        // else-branch: the wide member is not *exactly* `"circle"`, so it can be
        // `!== "circle"` → kept; only the circle member (exactly `"circle"`) drops.
        assert_eq!(
            narrow_by_discriminant(&mut interner, shape, "kind", circle_lit, false),
            wide
        );

        // A member that lacks `kind` entirely survives both branches.
        let no_disc = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("other", wk.number)],
        });
        let shape2 = interner.union(vec![circle, no_disc]);
        assert_eq!(
            narrow_by_discriminant(&mut interner, shape2, "kind", circle_lit, true),
            shape2,
            "a member lacking the discriminant must not be narrowed out (then)"
        );
        assert_eq!(
            narrow_by_discriminant(&mut interner, shape2, "kind", circle_lit, false),
            no_disc,
            "the else-branch drops only the exactly-matching member"
        );
    }

    /// `in`-operator narrowing keeps the members that have the property in the
    /// then-branch and those that can lack it in the else-branch.
    #[test]
    fn in_operator_keeps_having_and_lacking() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        // `{ a: number }` and `{ b: string }`.
        let with_a = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("a", wk.number)],
        });
        let with_b = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("b", wk.string)],
        });
        let box_ty = interner.union(vec![with_a, with_b]);

        // `"a" in x` then-branch keeps the `{ a }` member.
        assert_eq!(
            narrow_by_in_operator(&mut interner, box_ty, "a", true),
            with_a
        );
        // else-branch keeps the complement `{ b }`.
        assert_eq!(
            narrow_by_in_operator(&mut interner, box_ty, "a", false),
            with_b
        );
    }

    /// `in` over a member that LACKS the property: removed in the then-branch, kept
    /// in the else-branch. A non-object member is handled soundly (kept in else).
    #[test]
    fn in_operator_handles_missing_property_and_non_objects() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let with_a = object_a(&mut interner);
        // `{ a } | null`: `null` is a non-object member.
        let a_or_null = interner.union(vec![with_a, wk.null]);

        // `"a" in x` then keeps the object-with-`a`; the non-object `null` is dropped.
        assert_eq!(
            narrow_by_in_operator(&mut interner, a_or_null, "a", true),
            with_a
        );
        // else keeps `null` (a non-object can satisfy `!("a" in x)`); the object that
        // always has `a` is dropped.
        assert_eq!(
            narrow_by_in_operator(&mut interner, a_or_null, "a", false),
            wk.null
        );
    }

    /// `any`/error are un-narrowable: never collapse to `never`.
    #[test]
    fn any_and_error_are_unnarrowable() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        // typeof on `any` keeps `any`.
        assert_eq!(narrow_by_typeof(&mut interner, wk.any, TypeofTag::String, true), wk.any);
        // null-equality on the error type keeps the error type.
        assert_eq!(
            narrow(&mut interner, wk.error, &NarrowOp::EqNullish { is_undefined: false }, true),
            wk.error
        );
        // truthiness on `any` keeps `any` in both branches.
        assert_eq!(narrow_by_truthiness(&mut interner, wk.any, true), wk.any);
        assert_eq!(narrow_by_truthiness(&mut interner, wk.any, false), wk.any);

        // discriminant / `in` on `any` keep `any` in both branches.
        let lit = interner.intern_literal(LiteralValue::String("x".to_string()));
        assert_eq!(narrow_by_discriminant(&mut interner, wk.any, "kind", lit, true), wk.any);
        assert_eq!(narrow_by_discriminant(&mut interner, wk.any, "kind", lit, false), wk.any);
        assert_eq!(narrow_by_in_operator(&mut interner, wk.error, "a", true), wk.error);
        assert_eq!(narrow_by_in_operator(&mut interner, wk.error, "a", false), wk.error);
    }
}

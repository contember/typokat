//! Flow analysis & narrowing operations (architecture §5, mvp-plan §3/§5).
//!
//! Control-flow narrowing (`typeof`, `in`, truthiness, equality, discriminated
//! unions, assertion functions) is *the* reason people love TS DX and must never
//! be sacrificed. It is flow-sensitive and needs a native interpreter model, so
//! the checker is structured around flow facts from the start (mvp-plan §5).
//!
//! ## What lives here (M7)
//!
//! The **reusable narrowing operations**: pure `(type, polarity) -> type`
//! functions that refine a union by filtering its members (typeof tag,
//! truthiness, `null`/`undefined` equality). They take a [`crate::types::Interner`]
//! so the narrowed result is re-interned through [`crate::types::Interner::union`]
//! — `string | number` minus `number` collapses to `string`, `{a} | null` minus
//! `null` collapses to `{a}`. These are intentionally **flow-model-agnostic**: they
//! know nothing about `if`/`else`, scopes, or symbols, so they survive unchanged
//! when the structured-statement driver (in `checker.rs`) is eventually replaced by
//! a full flow-node CFG (see below).
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
//! (control-flow join), loops, `switch`, and definite-assignment (`TK2454`) /
//! reachability (`TK2355`). When that lands, the binder/checker will build a flow
//! graph and the narrowing pass will refine a declared type along a given flow
//! path — reusing the operations in this module verbatim. The following narrowings
//! are also deferred (mvp-plan, README "Deferred checks"): `in`-operator,
//! discriminated-union / property-equality, `switch`, assertion functions / type
//! predicates (`x is T`), and `&&`/`||`/ternary condition narrowing.

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
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
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
}

/// Apply a narrowing operation to `ty` under a branch polarity, returning the
/// refined type (re-interned through [`Interner::union`]). `positive` is `true` in
/// the branch where the guard holds as written, `false` in the complementary
/// branch — the driver passes `true` for the then-branch and the polarity-flipped
/// value for the else-branch (and folds any `!` in the condition into it).
///
/// All four narrowings filter the **union members** of `ty` and re-intern, so a
/// removed member collapses the union (`string | number` minus `number` =
/// `string`; `{a} | null` minus `null` = `{a}`). A non-union `ty` is treated as a
/// one-member set, so the operations are total and compose with prior narrowings.
pub fn narrow(interner: &mut Interner, ty: TypeId, op: NarrowOp, positive: bool) -> TypeId {
    match op {
        NarrowOp::Typeof(tag) => narrow_by_typeof(interner, ty, tag, positive),
        NarrowOp::Truthy => narrow_by_truthiness(interner, ty, positive),
        NarrowOp::EqNullish { is_undefined } => {
            narrow_by_nullish_equality(interner, ty, is_undefined, positive)
        }
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
        // Objects/functions/unions never match a primitive `typeof` tag (a nested
        // union cannot appear as a member after canonicalization, but the arm is
        // exhaustive and defensive either way).
        TypeTag::Object | TypeTag::Function | TypeTag::Union => false,
    }
}

/// Whether a type is *always* truthy — used to drop members from a falsy branch.
/// Object and function types are always truthy. Everything else (primitives,
/// literals, `null`/`undefined`, `unknown`, `any`/error) is conservatively treated
/// as possibly-falsy so it is **kept** in the falsy branch (sound: never removes a
/// member that could legitimately be falsy).
fn is_always_truthy(store: &crate::types::store::Store, member: TypeId) -> bool {
    matches!(store.tag(member), TypeTag::Object | TypeTag::Function)
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
            properties: vec![PropertyType {
                name: "a".to_string(),
                ty: wk.number,
                optional: false,
            }],
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
        assert_eq!(narrow(&mut interner, obj_or_null, NarrowOp::EqNullish { is_undefined: false }, true), wk.null);
        // `=== null` negative (else / `!== null` then) removes null.
        assert_eq!(narrow(&mut interner, obj_or_null, NarrowOp::EqNullish { is_undefined: false }, false), obj);

        // `{ a } | undefined` vs `=== undefined`.
        let obj_or_undef = interner.union(vec![obj, wk.undefined]);
        assert_eq!(narrow(&mut interner, obj_or_undef, NarrowOp::EqNullish { is_undefined: true }, true), wk.undefined);
        assert_eq!(narrow(&mut interner, obj_or_undef, NarrowOp::EqNullish { is_undefined: true }, false), obj);
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
            narrow(&mut interner, wk.error, NarrowOp::EqNullish { is_undefined: false }, true),
            wk.error
        );
        // truthiness on `any` keeps `any` in both branches.
        assert_eq!(narrow_by_truthiness(&mut interner, wk.any, true), wk.any);
        assert_eq!(narrow_by_truthiness(&mut interner, wk.any, false), wk.any);
    }
}

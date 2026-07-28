//! Flow analysis and narrowing operations (architecture §5).
//! Reusable narrowing functions plus the checker CFG node model. Operations filter
//! types under guard facts; the checker decides symbol and branch polarity.

use crate::binder::symbol::SymbolId;
use crate::check::query::{PublishedClassLookup, SemanticQueryCoordinator, SemanticQueryState};
use crate::class_semantics::DemandOutcome;
use crate::relate::RelationOutcome;
use crate::types::repr::{IntrinsicKind, LiteralValue, TypeTag};
use crate::types::store::TypeId;
use crate::types::Interner;

/// Index into the checker's flow-node arena ([`crate::check`]'s `Pass::flow_nodes`).
/// The graph is built by a pre-pass over each function body / the module top level
/// (M23); a reference's narrowed type is a backward walk from its flow node applying
/// the [`narrow`] ops above (architecture §5).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FlowNodeId(pub u32);

impl FlowNodeId {
    /// Unreachable code (after `return`/`throw`/`break`, or a join whose
    /// antecedents are *all* unreachable — the mechanism that makes early-exit
    /// narrowing work). Reserved arena slot 0. It is filtered out at join
    /// construction, so the resolver never asks it for a real type.
    pub const UNREACHABLE: FlowNodeId = FlowNodeId(0);
    /// Flow start (function/module entry): a reference resolves to its **declared**
    /// type here (the backward walk's base case). Reserved arena slot 1.
    pub const START: FlowNodeId = FlowNodeId(1);
}

/// A node in the single control-flow narrowing model (architecture §5). Slots 0/1
/// are the [`FlowNodeId::UNREACHABLE`]/[`FlowNodeId::START`] sentinels.
pub enum FlowNode {
    /// Reserved sentinel: unreachable code / an all-unreachable join (slot 0).
    Unreachable,
    /// Reserved sentinel: flow start — the declared type (slot 1).
    Start,
    /// An assignment in straight-line flow. `None` resets to the declared type for
    /// complex/compound assignments, the sound over-report direction.
    Assignment {
        symbol: SymbolId,
        assigned: Option<TypeId>,
        antecedent: FlowNodeId,
    },
    /// A condition branch: the guard's `op` applied to `symbol` under `positive`
    /// (the branch polarity, with any leading `!` already folded in). Resolving the
    /// guarded `symbol` applies the narrowing; a *different* symbol passes through
    /// to `antecedent`.
    Condition {
        symbol: SymbolId,
        op: NarrowOp,
        positive: bool,
        antecedent: FlowNodeId,
    },
    /// A branch join — an `if`/`else` merge or the post-loop join of the exit edge
    /// with any `break` edges. The reference's type is the `union(...)` over the
    /// antecedents (which already exclude the unreachable ones, so an all-returning
    /// `if` collapses to its complement — early-exit narrowing).
    BranchLabel { antecedents: Vec<FlowNodeId> },
    /// A `while` loop label resolved by a single-unroll fixpoint. The provisional
    /// seed is never durably memoized; caching it across a back edge is a dropped-
    /// error false negative (invariants §1).
    LoopLabel {
        pre: Vec<FlowNodeId>,
        back: Vec<FlowNodeId>,
    },
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

/// A guard shape independent of branch polarity. The checker pairs it with a
/// target `SymbolId`, which keeps narrowing from leaking across symbols.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NarrowOp {
    /// `typeof x === <tag>` (the positive sense). The then-branch keeps the
    /// members matching the tag; the else-branch keeps the complement.
    Typeof(TypeofTag),
    /// Truthiness. Falsy-capable primitives are not split, so each branch remains
    /// a superset of the precise narrowing.
    Truthy,
    /// `x === null` / `x === undefined` (the positive sense). The then-branch keeps
    /// only that nullish member; the else-branch removes it. The target nullish
    /// intrinsic is identified by `is_undefined`.
    EqNullish { is_undefined: bool },
    /// Literal discriminant (M8): then keeps members whose property could equal the
    /// literal; else removes members whose property is exactly that literal.
    Discriminant { property: String, literal: TypeId },
    /// **`in`-operator** (M8): `"prop" in x` (the positive sense). The then-branch
    /// keeps the union members that *have* `prop`; the else-branch keeps those that
    /// can *lack* it. See [`narrow_by_in_operator`] for the exact (sound) rule.
    In { property: String },
}

struct DiscriminantQueryRequest<'a> {
    ty: TypeId,
    property: &'a str,
    literal: TypeId,
    positive: bool,
}

/// Apply a narrowing operation under branch polarity and re-intern the result.
/// Non-unions are treated as one-member sets, so operations are total and compose.
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

/// Production narrowing boundary for class-reachable guards. The whole guard
/// runs on a child query state; an exhausted constituent promotes no memo/cache
/// writes and lets the caller retain the pre-guard type conservatively.
pub(crate) fn narrow_query<L: PublishedClassLookup + ?Sized>(
    interner: &mut Interner,
    published: &L,
    state: &mut SemanticQueryState,
    next_type_param: &mut u32,
    ty: TypeId,
    op: &NarrowOp,
    positive: bool,
) -> DemandOutcome<TypeId> {
    state.savepoint();
    let outcome = match op {
        NarrowOp::Discriminant { property, literal } => narrow_by_discriminant_query(
            interner,
            published,
            state,
            next_type_param,
            DiscriminantQueryRequest {
                ty,
                property,
                literal: *literal,
                positive,
            },
        ),
        NarrowOp::In { property } => narrow_by_in_operator_query(
            interner,
            published,
            state,
            next_type_param,
            ty,
            property,
            positive,
        ),
        _ => DemandOutcome::Ready(narrow(interner, ty, op, positive)),
    };
    if matches!(outcome, DemandOutcome::Ready(_)) {
        state.commit();
    } else {
        state.rollback();
    }
    outcome
}

/// Narrow by `typeof x === <tag>`. Then keeps matching primitive/literal members;
/// else keeps the complement. `any` stays un-narrowable.
pub fn narrow_by_typeof(interner: &mut Interner, ty: TypeId, tag: TypeofTag, keep: bool) -> TypeId {
    filter_union(interner, ty, |store, member| {
        let matches = member_matches_typeof(store, member, tag);
        // Keep matching members in the then-branch, the complement in the else.
        matches == keep
    })
}

/// Narrow by truthiness. Falsy-capable primitives are deliberately not split, so
/// both branches remain safe supersets of the precise narrowing.
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

/// Narrow by strict null/undefined equality: then keeps only the targeted nullish
/// member, else removes it.
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
/// correctly. Each proof uses the guarded legacy relation kernel before the
/// mutable re-intern.
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

    // Decide each member while the store is borrowed immutably, then release it
    // before the mutable `union` below.
    let kept: Vec<TypeId> = {
        let store = interner.store();
        members
            .iter()
            .copied()
            .filter(|&member| {
                let prop_ty = store
                    .object_type(member)
                    .and_then(|obj| obj.property(property))
                    .map(|prop| prop.ty);
                discriminant_keeps(store, member, prop_ty, literal, wk, positive)
            })
            .collect()
    };

    interner.union(kept)
}

/// Whether the discriminant narrowing **keeps** a member, given its discriminant
/// property type `prop_ty` (`None` if the member lacks the property or is not an
/// object). Pulled out so the (sound) keep rule is unit-testable and stated once.
fn discriminant_keeps(
    store: &crate::types::store::Store,
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
        literal_could_equal_property(store, literal, prop_ty, wk)
    } else {
        // else-branch: drop only when `m.prop` is exactly the literal (so
        // `m.prop === literal` always holds and `!==` excludes the member).
        prop_ty != literal
    }
}

/// Conservative non-query proof used by flow unit tests and other intrinsic-only
/// callers. Query-owned shapes remain possible instead of being treated as a
/// relation failure.
fn literal_could_equal_property(
    store: &crate::types::store::Store,
    literal: TypeId,
    property: TypeId,
    wk: crate::types::WellKnown,
) -> bool {
    if literal == property || property == wk.any || property == wk.error {
        return true;
    }
    match store.tag(property) {
        TypeTag::Literal => false,
        TypeTag::Intrinsic => match store.literal_value(literal) {
            Some(value) => store.intrinsic_kind(property) == Some(value.base_kind()),
            None => false,
        },
        TypeTag::Union => store.union_members(property).is_none_or(|members| {
            members
                .iter()
                .any(|&member| literal_could_equal_property(store, literal, member, wk))
        }),
        TypeTag::Intersection => store.intersection_members(property).is_none_or(|members| {
            members
                .iter()
                .all(|&member| literal_could_equal_property(store, literal, member, wk))
        }),
        TypeTag::Readonly => store
            .readonly_operand(property)
            .is_none_or(|operand| literal_could_equal_property(store, literal, operand, wk)),
        TypeTag::Object | TypeTag::Function | TypeTag::Array | TypeTag::Tuple => false,
        TypeTag::TypeParam
        | TypeTag::Template
        | TypeTag::Conditional
        | TypeTag::Instantiation
        | TypeTag::ClassInstance
        | TypeTag::Infer
        | TypeTag::Mapped
        | TypeTag::MappedValue
        | TypeTag::Keyof
        | TypeTag::DeferredIndexedAccess
        | TypeTag::Declared => true,
    }
}

/// Query-aware discriminant decision for class-reachable property types.
/// Exhaustion remains typed so the caller can abandon the whole narrowing
/// attempt instead of dropping a union member on an invented mismatch.
pub(crate) fn discriminant_keeps_query<L: PublishedClassLookup + ?Sized>(
    coordinator: &mut SemanticQueryCoordinator<'_, L>,
    member: TypeId,
    prop_ty: Option<TypeId>,
    literal: TypeId,
    wk: crate::types::WellKnown,
    positive: bool,
) -> DemandOutcome<bool> {
    if member == wk.any || member == wk.error {
        return DemandOutcome::Ready(true);
    }
    let Some(prop_ty) = prop_ty else {
        return DemandOutcome::Ready(true);
    };
    if !positive {
        return DemandOutcome::Ready(prop_ty != literal);
    }
    match coordinator.is_assignable(literal, prop_ty) {
        RelationOutcome::Yes => DemandOutcome::Ready(true),
        RelationOutcome::No(_) => DemandOutcome::Ready(false),
        RelationOutcome::Exhausted(reason) => DemandOutcome::Exhausted(reason),
    }
}

fn narrow_by_discriminant_query<L: PublishedClassLookup + ?Sized>(
    interner: &mut Interner,
    published: &L,
    state: &mut SemanticQueryState,
    next_type_param: &mut u32,
    request: DiscriminantQueryRequest<'_>,
) -> DemandOutcome<TypeId> {
    let DiscriminantQueryRequest {
        ty,
        property,
        literal,
        positive,
    } = request;
    let wk = interner.well_known();
    if ty == wk.any || ty == wk.error {
        return DemandOutcome::Ready(ty);
    }
    let members = interner
        .store()
        .union_members(ty)
        .map_or_else(|| vec![ty], |members| members.to_vec());
    let mut kept = Vec::with_capacity(members.len());
    for member in members {
        let apparent = flow_apparent_type(interner, member);
        let demanded =
            match SemanticQueryCoordinator::new(interner, published, state, next_type_param)
                .demand(apparent)
            {
                DemandOutcome::Ready(demanded) => demanded,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
        let prop_ty = interner
            .store()
            .object_type(demanded)
            .and_then(|object| object.property(property))
            .map(|property| property.ty);
        let decision = discriminant_keeps_query(
            &mut SemanticQueryCoordinator::new(interner, published, state, next_type_param),
            demanded,
            prop_ty,
            literal,
            wk,
            positive,
        );
        match decision {
            DemandOutcome::Ready(true) => kept.push(member),
            DemandOutcome::Ready(false) => {}
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        }
    }
    DemandOutcome::Ready(interner.union(kept))
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

fn narrow_by_in_operator_query<L: PublishedClassLookup + ?Sized>(
    interner: &mut Interner,
    published: &L,
    state: &mut SemanticQueryState,
    next_type_param: &mut u32,
    ty: TypeId,
    property: &str,
    positive: bool,
) -> DemandOutcome<TypeId> {
    let wk = interner.well_known();
    if ty == wk.any || ty == wk.error {
        return DemandOutcome::Ready(ty);
    }
    let members = interner
        .store()
        .union_members(ty)
        .map_or_else(|| vec![ty], |members| members.to_vec());
    let mut kept = Vec::with_capacity(members.len());
    for member in members {
        if member == wk.any || member == wk.error {
            kept.push(member);
            continue;
        }
        let apparent = flow_apparent_type(interner, member);
        let demanded =
            match SemanticQueryCoordinator::new(interner, published, state, next_type_param)
                .demand(apparent)
            {
                DemandOutcome::Ready(demanded) => demanded,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
        let object = interner.store().object_type(demanded);
        let keep = match object {
            Some(object) => {
                let has_property = object.property(property).is_some();
                if positive {
                    has_property
                } else {
                    !has_property
                }
            }
            None if is_query_owned_shape(interner, demanded) => true,
            None => !positive,
        };
        if keep {
            kept.push(member);
        }
    }
    DemandOutcome::Ready(interner.union(kept))
}

fn flow_apparent_type(interner: &Interner, ty: TypeId) -> TypeId {
    let store = interner.store();
    let mut current = ty;
    let mut seen = rustc_hash::FxHashSet::default();
    while store.tag(current) == TypeTag::TypeParam {
        let Some(parameter) = store.type_param(current) else {
            break;
        };
        if !seen.insert(parameter.id) {
            break;
        }
        let Some(constraint) = store.type_param_constraint(parameter.id) else {
            break;
        };
        current = constraint;
    }
    current
}

fn is_query_owned_shape(interner: &Interner, ty: TypeId) -> bool {
    matches!(
        interner.store().tag(ty),
        TypeTag::TypeParam
            | TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::ClassInstance
            | TypeTag::Infer
            | TypeTag::Mapped
            | TypeTag::MappedValue
            | TypeTag::Keyof
            | TypeTag::DeferredIndexedAccess
    )
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
        // M27: a template literal type is a string subtype, so it matches `typeof x ===
        // "string"` (and no other tag) — kept correct so a template member is not
        // wrongly narrowed out of a `string` branch.
        TypeTag::Template => tag.intrinsic() == crate::types::repr::IntrinsicKind::String,
        // Composite/uninstantiated types never match primitive M7 `typeof` tags.
        TypeTag::Object
        | TypeTag::Function
        | TypeTag::Union
        // M31: an intersection never appears as a narrowable union member in the M7
        // `typeof` subset — defensively false (like the other composites here).
        | TypeTag::Intersection
        | TypeTag::TypeParam
        | TypeTag::Array
        | TypeTag::Tuple
        | TypeTag::Readonly
        // M25/M26/M28: a conditional / lazy instantiation / infer binder / mapped type /
        // mapped-value placeholder / deferred keyof never appears as a narrowable union
        // member in the M7 `typeof` subset — defensively false.
        | TypeTag::Conditional
        | TypeTag::Instantiation
        | TypeTag::ClassInstance
        | TypeTag::Infer
        | TypeTag::Mapped
        | TypeTag::MappedValue
        | TypeTag::Keyof
        | TypeTag::DeferredIndexedAccess
        | TypeTag::Declared => false,
    }
}

/// Whether a type is always truthy; everything else is kept in the falsy branch.
fn is_always_truthy(store: &crate::types::store::Store, member: TypeId) -> bool {
    // M17: an array value is always truthy in JS (like an object/function), so it is
    // dropped from a falsy branch — a `T[] | null` narrowed by `if (a)` keeps only
    // `T[]` in the truthy branch and `null` in the falsy one.
    matches!(
        store.tag(member),
        TypeTag::Object | TypeTag::Function | TypeTag::Array | TypeTag::Readonly
    )
}

/// Filter union members and re-intern survivors. Non-unions are one-member sets;
/// `any`/error are un-narrowable tops and must not collapse to `never`.
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
            ..Default::default()
        })
    }

    /// typeof keep + complement over `string | number`.
    #[test]
    fn typeof_keeps_tag_and_complement() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let sn = interner.union(vec![wk.string, wk.number]);

        // then-branch of `typeof x === "string"` keeps `string`.
        assert_eq!(
            narrow_by_typeof(&mut interner, sn, TypeofTag::String, true),
            wk.string
        );
        // else-branch keeps the complement `number`.
        assert_eq!(
            narrow_by_typeof(&mut interner, sn, TypeofTag::String, false),
            wk.number
        );
        // `typeof === "number"` then-branch keeps `number`.
        assert_eq!(
            narrow_by_typeof(&mut interner, sn, TypeofTag::Number, true),
            wk.number
        );
        // `boolean` is absent → then-branch is `never`, else-branch is the whole union.
        assert_eq!(
            narrow_by_typeof(&mut interner, sn, TypeofTag::Boolean, true),
            wk.never
        );
        assert_eq!(
            narrow_by_typeof(&mut interner, sn, TypeofTag::Boolean, false),
            sn
        );
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
        assert_eq!(
            narrow_by_typeof(&mut interner, lit_or_num, TypeofTag::String, true),
            lit
        );
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
        assert_eq!(
            narrow_by_truthiness(&mut interner, obj_or_null, false),
            wk.null
        );

        // `{ a } | undefined`.
        let obj_or_undef = interner.union(vec![obj, wk.undefined]);
        assert_eq!(narrow_by_truthiness(&mut interner, obj_or_undef, true), obj);
        assert_eq!(
            narrow_by_truthiness(&mut interner, obj_or_undef, false),
            wk.undefined
        );
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
        assert_eq!(
            narrow_by_truthiness(&mut interner, str_or_null, true),
            wk.string
        );
        assert_eq!(
            narrow_by_truthiness(&mut interner, str_or_null, false),
            str_or_null
        );
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
        assert_eq!(
            narrow(
                &mut interner,
                obj_or_null,
                &NarrowOp::EqNullish {
                    is_undefined: false
                },
                true
            ),
            wk.null
        );
        // `=== null` negative (else / `!== null` then) removes null.
        assert_eq!(
            narrow(
                &mut interner,
                obj_or_null,
                &NarrowOp::EqNullish {
                    is_undefined: false
                },
                false
            ),
            obj
        );

        // `{ a } | undefined` vs `=== undefined`.
        let obj_or_undef = interner.union(vec![obj, wk.undefined]);
        assert_eq!(
            narrow(
                &mut interner,
                obj_or_undef,
                &NarrowOp::EqNullish { is_undefined: true },
                true
            ),
            wk.undefined
        );
        assert_eq!(
            narrow(
                &mut interner,
                obj_or_undef,
                &NarrowOp::EqNullish { is_undefined: true },
                false
            ),
            obj
        );
    }

    /// Build `{ <disc>: <disc_ty>; <extra>: number }` — a discriminated-union
    /// member with a discriminant property `disc` of type `disc_ty`.
    fn member(interner: &mut Interner, disc: &str, disc_ty: TypeId, extra: &str) -> TypeId {
        let wk = interner.well_known();
        interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public(disc, disc_ty),
                PropertyType::public(extra, wk.number),
            ],
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        });
        let with_b = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("b", wk.string)],
            ..Default::default()
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
        assert_eq!(
            narrow_by_typeof(&mut interner, wk.any, TypeofTag::String, true),
            wk.any
        );
        // null-equality on the error type keeps the error type.
        assert_eq!(
            narrow(
                &mut interner,
                wk.error,
                &NarrowOp::EqNullish {
                    is_undefined: false
                },
                true
            ),
            wk.error
        );
        // truthiness on `any` keeps `any` in both branches.
        assert_eq!(narrow_by_truthiness(&mut interner, wk.any, true), wk.any);
        assert_eq!(narrow_by_truthiness(&mut interner, wk.any, false), wk.any);

        // discriminant / `in` on `any` keep `any` in both branches.
        let lit = interner.intern_literal(LiteralValue::String("x".to_string()));
        assert_eq!(
            narrow_by_discriminant(&mut interner, wk.any, "kind", lit, true),
            wk.any
        );
        assert_eq!(
            narrow_by_discriminant(&mut interner, wk.any, "kind", lit, false),
            wk.any
        );
        assert_eq!(
            narrow_by_in_operator(&mut interner, wk.error, "a", true),
            wk.error
        );
        assert_eq!(
            narrow_by_in_operator(&mut interner, wk.error, "a", false),
            wk.error
        );
    }
}

//! Pluggable structural hashing for the interner.
//!
//! Architecture §3.2 wants two identities per type: the run-local `TypeId(u32)`
//! and a **stable structural hash** (content-addressed, e.g. blake3) computed at
//! intern time for cross-run identity (disk cache, incrementality).
//!
//! The MVP deliberately does NOT compute blake3 (mvp-plan §7.1): we have no
//! incrementality yet, so paying for it would be cost without benefit. But the
//! interner is *shaped* around "a hash computed at intern time" so swapping the
//! function in here is a one-module change rather than the rewrite the doc warns
//! about. Concretely:
//!
//! - `structural_hash` is the live function (FxHash now) used for hash-consing.
//! - `StableHash` + `stable_hash` are the reserved slot for the blake3-style
//!   content hash. They are NOT populated in the MVP — `stable_hash` returns a
//!   documented placeholder and the `Store` keeps a (currently unused) column
//!   for it. Phase 4 fills this in without changing call sites.

use crate::types::repr::{
    IntrinsicKind, LiteralValue, ModifierOp, ParameterType, PropertyType, TypeParamId, TypeTag,
};
use crate::types::store::TypeId;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

/// The structural-hash key for a candidate type, hashed *before* it is interned.
/// Two structurally identical types must produce the same `StructuralKey` hash
/// so hash-consing collapses them (architecture §3: structural equality becomes
/// an id compare).
///
/// This intentionally mirrors the data the interner dedups on. As object/union/
/// function types come online (M2–M4) their structural keys are added here.
pub enum StructuralKey<'a> {
    Intrinsic(IntrinsicKind),
    Literal(&'a LiteralValue),
    /// An object type, keyed over its **canonical** property list (sorted by name
    /// by the interner before hashing) so `{ a; b }` and `{ b; a }` collide, plus
    /// its **index signatures** (M19): the string- and number-index value type ids
    /// are part of the key, so `{ [k: string]: number }` collides only with another
    /// object having the same members AND the same index signatures (and is distinct
    /// from `{}` / `{ [k: string]: string }`).
    Object {
        properties: &'a [PropertyType],
        string_index: Option<TypeId>,
        number_index: Option<TypeId>,
        call_signatures: &'a [TypeId],
        construct_signatures: &'a [TypeId],
    },
    /// A function type, keyed over its **positional** parameter list (never
    /// sorted) and return type. Two function types collide only when their
    /// parameters match in order (name, optionality, type) and their return types
    /// are the same interned id.
    Function {
        params: &'a [ParameterType],
        ret: TypeId,
    },
    /// A union type, keyed over its **canonical** member list (flattened, sorted
    /// by `TypeId`, deduped, `never`-free by the interner before hashing) so two
    /// unions with the same member set in any source order collide.
    Union(&'a [TypeId]),
    /// A **type parameter** (M9), keyed over its [`TypeParamId`] alone. The name
    /// is **not** part of the key — identity is the declaration site's unique id,
    /// so re-interning the same parameter collides while two parameters from
    /// distinct declarations never do (even if they share a source name).
    TypeParam(TypeParamId),
    /// An **array** type (`T[]` — M17), keyed over its **element** `TypeId` alone.
    /// `number[]` collides with `number[]`; `number[]` and `string[]` do not. The
    /// element is itself canonical (interned), so the key is decided by its id.
    Array(TypeId),
    /// A **tuple** type (`[A, B]` — M18), keyed over its **ordered** element list.
    /// Order is significant (the list is **not** sorted, unlike a union), so
    /// `[number, string]` and `[string, number]` hash differently, and so does
    /// `[number]` (length differs). Each element is itself canonical (interned), so
    /// the key is decided by the ordered sequence of ids.
    Tuple(&'a [TypeId]),
    /// A **conditional** type (`C extends E ? T : F` — M25), keyed over its four
    /// component ids **in order** (position is meaning — the branches are not
    /// interchangeable), plus `infer_count` and `distributive`. So
    /// `C extends E ? T : F` and `C extends E ? F : T` hash differently.
    Conditional {
        check: TypeId,
        extends_ty: TypeId,
        true_branch: TypeId,
        false_branch: TypeId,
        infer_count: u32,
        distributive: bool,
        poisoned: bool,
    },
    /// A **lazy instantiation** (M25), keyed over its base id and its
    /// **sorted-by-id** argument list. Two equal `(base, args)` collide.
    Instantiation {
        base: TypeId,
        args: &'a [(TypeParamId, TypeId)],
    },
    /// An **`infer` binder** (M25), keyed over its de Bruijn index alone.
    Infer(u32),
    /// A **mapped type** (M26), keyed over its whole shape (homomorphic flag, key
    /// source, value template, and both modifier operators) so two structurally equal
    /// mapped types collide.
    Mapped {
        homomorphic: bool,
        key_source: TypeId,
        value_template: TypeId,
        optional_modifier: ModifierOp,
        readonly_modifier: ModifierOp,
    },
    /// A **mapped-value placeholder** (M26). Identity is the tag alone (no payload), so
    /// every `T[K]` placeholder hash-conses to one node.
    MappedValue,
}

/// Live structural hash used for hash-consing (FxHash for speed). This is the
/// function `Interner` keys its dedup map on.
pub fn structural_hash(key: &StructuralKey<'_>) -> u64 {
    let mut h = FxHasher::default();
    match key {
        StructuralKey::Intrinsic(kind) => {
            // Tag-discriminate so an intrinsic and a (future) literal can never
            // collide on the same numeric payload.
            TypeTag::Intrinsic.hash_discriminant(&mut h);
            (*kind as u8).hash(&mut h);
        }
        StructuralKey::Literal(value) => {
            TypeTag::Literal.hash_discriminant(&mut h);
            hash_literal(value, &mut h);
        }
        StructuralKey::Object {
            properties,
            string_index,
            number_index,
            call_signatures,
            construct_signatures,
        } => {
            TypeTag::Object.hash_discriminant(&mut h);
            // M19: fold the index signatures into the key first (the value-type id,
            // or a sentinel for absent), so an object with an index signature hashes
            // distinctly from one without. The ids are canonical (interned), so
            // hashing them by value is order-stable.
            string_index.map(|v| v.0).hash(&mut h);
            number_index.map(|v| v.0).hash(&mut h);
            // F1/WU2: call signatures are part of object identity. Hash both the
            // count and each interned FunctionType id so `{ (x: number): string }`
            // is distinct from `{}` and from a same-member object with a different
            // call signature.
            call_signatures.len().hash(&mut h);
            for signature in *call_signatures {
                signature.0.hash(&mut h);
            }
            // F1/WU3: construct signatures are the `new` dual of call
            // signatures and participate in object identity the same way.
            construct_signatures.len().hash(&mut h);
            for signature in *construct_signatures {
                signature.0.hash(&mut h);
            }
            // Length first so prefixes of a longer property list cannot collide
            // with the shorter one under the streaming hasher.
            properties.len().hash(&mut h);
            for prop in *properties {
                // Properties arrive in canonical (name-sorted) order, so this is
                // order-independent across two structurally equal object types.
                prop.name.hash(&mut h);
                prop.optional.hash(&mut h);
                prop.ty.0.hash(&mut h);
                // M13: visibility + declaring class are part of a member's
                // identity, so a `private x` and a public `x` of the same name/type
                // hash differently, and a `private`/`protected` member from one
                // class differs from a same-named one from another. This is what
                // gives a class with a non-public member its **nominal** identity
                // (distinct interned `TypeId`) and keeps the relation cache sound
                // (distinct origins ⇒ distinct ids ⇒ distinct cache keys).
                (prop.visibility as u8).hash(&mut h);
                prop.declaring_class.map(|c| c.0).hash(&mut h);
                // M14: `readonly` is part of a member's structural identity too, so
                // a `readonly x` and a mutable `x` of the same name/type hash to
                // distinct objects (the flag is preserved through interning rather
                // than dropped). It does NOT affect assignability — the relation
                // engine ignores it — but keeping it in the identity is what lets the
                // assignment-target check read it back off the interned type.
                prop.readonly.hash(&mut h);
                // M15: `is_accessor` is folded in identically (preserved through
                // interning, ignored by the relation), so a get-only accessor is
                // distinct from a same-shape `readonly` field and the assignment-target
                // check can tell them apart (accessor = read-only everywhere; readonly
                // field = assignable in its declaring constructor).
                prop.is_accessor.hash(&mut h);
            }
        }
        StructuralKey::Function { params, ret } => {
            TypeTag::Function.hash_discriminant(&mut h);
            // Arity first so a shorter parameter list cannot collide with a
            // prefix of a longer one under the streaming hasher.
            params.len().hash(&mut h);
            for param in *params {
                // Parameters are positional (never sorted): hash in order so two
                // function types with the same parameter *types* in a different
                // order remain distinct.
                param.name.hash(&mut h);
                param.optional.hash(&mut h);
                param.ty.0.hash(&mut h);
            }
            ret.0.hash(&mut h);
        }
        StructuralKey::Union(members) => {
            TypeTag::Union.hash_discriminant(&mut h);
            // Arity first so a shorter member list cannot collide with a prefix
            // of a longer one under the streaming hasher.
            members.len().hash(&mut h);
            for member in *members {
                // Members arrive in canonical (TypeId-sorted) order, so this is
                // order-independent across two structurally equal union types.
                member.0.hash(&mut h);
            }
        }
        StructuralKey::TypeParam(id) => {
            TypeTag::TypeParam.hash_discriminant(&mut h);
            // Identity is the declaration-site id only (not the name).
            id.0.hash(&mut h);
        }
        StructuralKey::Array(element) => {
            TypeTag::Array.hash_discriminant(&mut h);
            // Identity is the (canonical) element id alone.
            element.0.hash(&mut h);
        }
        StructuralKey::Tuple(elements) => {
            TypeTag::Tuple.hash_discriminant(&mut h);
            // Length first so a shorter element list cannot collide with a prefix
            // of a longer one under the streaming hasher (and so `[number]` differs
            // from `[number, string]`).
            elements.len().hash(&mut h);
            for element in *elements {
                // Elements are hashed in **source order** (never sorted), so order
                // is part of identity: `[number, string]` and `[string, number]`
                // hash differently.
                element.0.hash(&mut h);
            }
        }
        StructuralKey::Conditional {
            check,
            extends_ty,
            true_branch,
            false_branch,
            infer_count,
            distributive,
            poisoned,
        } => {
            TypeTag::Conditional.hash_discriminant(&mut h);
            // Hashed in field order (position is meaning — the branches are not a set).
            check.0.hash(&mut h);
            extends_ty.0.hash(&mut h);
            true_branch.0.hash(&mut h);
            false_branch.0.hash(&mut h);
            infer_count.hash(&mut h);
            distributive.hash(&mut h);
            poisoned.hash(&mut h);
        }
        StructuralKey::Instantiation { base, args } => {
            TypeTag::Instantiation.hash_discriminant(&mut h);
            base.0.hash(&mut h);
            // Args arrive sorted by TypeParamId, so this is order-stable across two
            // structurally equal instantiations.
            args.len().hash(&mut h);
            for (param, arg) in *args {
                param.0.hash(&mut h);
                arg.0.hash(&mut h);
            }
        }
        StructuralKey::Infer(index) => {
            TypeTag::Infer.hash_discriminant(&mut h);
            index.hash(&mut h);
        }
        StructuralKey::Mapped {
            homomorphic,
            key_source,
            value_template,
            optional_modifier,
            readonly_modifier,
        } => {
            TypeTag::Mapped.hash_discriminant(&mut h);
            homomorphic.hash(&mut h);
            key_source.0.hash(&mut h);
            value_template.0.hash(&mut h);
            (*optional_modifier as u8).hash(&mut h);
            (*readonly_modifier as u8).hash(&mut h);
        }
        StructuralKey::MappedValue => {
            TypeTag::MappedValue.hash_discriminant(&mut h);
        }
    }
    h.finish()
}

/// Hash a literal deterministically. Floats are hashed by their bit pattern so
/// `NaN` and `-0.0`/`0.0` behave consistently between the hash and the
/// `LiteralValue` `PartialEq` used for the dedup-bucket tie-break.
fn hash_literal(value: &LiteralValue, h: &mut impl Hasher) {
    match value {
        LiteralValue::Number(n) => {
            0u8.hash(h);
            n.to_bits().hash(h);
        }
        LiteralValue::String(s) => {
            1u8.hash(h);
            s.hash(h);
        }
        LiteralValue::Boolean(b) => {
            2u8.hash(h);
            b.hash(h);
        }
    }
}

impl TypeTag {
    #[inline]
    fn hash_discriminant(self, h: &mut impl Hasher) {
        (self as u8).hash(h);
    }
}

/// Reserved cross-run stable hash type (architecture §3.2). A newtype around a
/// 32-byte digest so the eventual blake3 output drops in unchanged.
///
/// TODO(Phase 4): compute a real content hash (blake3 over the canonical
/// structure) at intern time and store it in `Store::stable_hash`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct StableHash(pub [u8; 32]);

/// Placeholder stable hash. NOT a real content hash — see the module docs and
/// mvp-plan §7.1. Returns the zero digest; the `Store` column is reserved but
/// unread in the MVP. Kept as a function (not `todo!()`) so it is never a
/// reachable panic.
#[allow(dead_code)] // TODO(Phase 4): wire into the interner at intern time.
pub fn stable_hash(_key: &StructuralKey<'_>) -> StableHash {
    StableHash::default()
}

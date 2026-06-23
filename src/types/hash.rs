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
    IntrinsicKind, LiteralValue, ParameterType, PropertyType, TypeParamId, TypeTag,
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
    /// by the interner before hashing) so `{ a; b }` and `{ b; a }` collide.
    Object(&'a [PropertyType]),
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
        StructuralKey::Object(properties) => {
            TypeTag::Object.hash_discriminant(&mut h);
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

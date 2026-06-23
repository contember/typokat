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

use crate::types::repr::{IntrinsicKind, LiteralValue, TypeTag};
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
    // TODO(M2): Object(&'a [PropertyType])
    // TODO(M4): Union(&'a [TypeId])  — over already-canonicalized members
    // TODO(M3): Function(&'a FunctionType)
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

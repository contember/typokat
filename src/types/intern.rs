//! The interner: hash-consing + canonicalization over the SoA `Store`.
//!
//! Architecture §3 / mvp-plan §4.2. Every type is constructed through here so
//! structurally identical types share one `TypeId` (structural equality becomes
//! an integer compare). The dedup index maps a structural hash → candidate ids;
//! ties within a bucket are resolved by a real structural comparison.
//!
//! Intrinsics are interned FIRST (in `IntrinsicKind::ALL` order) so they get
//! small, fixed, well-known ids exposed as `WellKnown` constants and used as
//! cheap constants throughout the checker and relation engine.

use crate::types::hash::{structural_hash, StructuralKey};
use crate::types::repr::{IntrinsicKind, LiteralValue, TypeFlags, TypeTag};
use crate::types::store::{Store, TypeId};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Well-known intrinsic type ids. Because intrinsics are interned first and in a
/// fixed order, these ids are stable for the life of the process and can be used
/// as constants (e.g. relation-engine fast paths, the checker's annotation
/// lowering).
#[derive(Copy, Clone, Debug)]
pub struct WellKnown {
    pub error: TypeId,
    pub any: TypeId,
    pub unknown: TypeId,
    pub never: TypeId,
    pub void: TypeId,
    pub null: TypeId,
    pub undefined: TypeId,
    pub boolean: TypeId,
    pub number: TypeId,
    pub string: TypeId,
}

pub struct Interner {
    store: Store,
    /// Structural hash → interned candidates sharing that hash (architecture
    /// §3.3). `SmallVec` because collisions are rare; the common case is a
    /// 1-element bucket.
    dedup: FxHashMap<u64, SmallVec<[TypeId; 2]>>,
    well_known: WellKnown,
}

impl Interner {
    /// Build an interner with the intrinsic types pre-interned in canonical
    /// order, returning it ready to use. The `WellKnown` table is filled as a
    /// side effect.
    pub fn with_intrinsics() -> Self {
        let mut interner = Interner {
            store: Store::new(),
            dedup: FxHashMap::default(),
            // Placeholder; overwritten below before any use.
            well_known: WellKnown {
                error: TypeId(0),
                any: TypeId(0),
                unknown: TypeId(0),
                never: TypeId(0),
                void: TypeId(0),
                null: TypeId(0),
                undefined: TypeId(0),
                boolean: TypeId(0),
                number: TypeId(0),
                string: TypeId(0),
            },
        };

        // Intern every intrinsic in the canonical order so ids are fixed.
        let mut ids = [TypeId(0); IntrinsicKind::ALL.len()];
        for (slot, kind) in IntrinsicKind::ALL.into_iter().enumerate() {
            ids[slot] = interner.intern_intrinsic(kind);
        }

        // Map kinds → ids positionally (ALL order is the source of truth).
        let id_of = |kind: IntrinsicKind| {
            let pos = IntrinsicKind::ALL
                .into_iter()
                .position(|k| k == kind)
                .expect("every IntrinsicKind is in ALL");
            ids[pos]
        };
        interner.well_known = WellKnown {
            error: id_of(IntrinsicKind::Error),
            any: id_of(IntrinsicKind::Any),
            unknown: id_of(IntrinsicKind::Unknown),
            never: id_of(IntrinsicKind::Never),
            void: id_of(IntrinsicKind::Void),
            null: id_of(IntrinsicKind::Null),
            undefined: id_of(IntrinsicKind::Undefined),
            boolean: id_of(IntrinsicKind::Boolean),
            number: id_of(IntrinsicKind::Number),
            string: id_of(IntrinsicKind::String),
        };

        interner
    }

    /// Read-only access to the underlying store (the relation engine and
    /// renderer borrow it).
    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn well_known(&self) -> WellKnown {
        self.well_known
    }

    /// Intern an intrinsic type, returning the shared id.
    pub fn intern_intrinsic(&mut self, kind: IntrinsicKind) -> TypeId {
        let key = StructuralKey::Intrinsic(kind);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.intrinsic_kind(id) == Some(kind)
        }) {
            return existing;
        }
        // The error type is the only intrinsic that carries a flag.
        let flags = if kind == IntrinsicKind::Error {
            TypeFlags::CONTAINS_ERROR
        } else {
            TypeFlags::EMPTY
        };
        let id = self.store.push_intrinsic(kind, flags);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a literal type, returning the shared id.
    pub fn intern_literal(&mut self, value: LiteralValue) -> TypeId {
        let key = StructuralKey::Literal(&value);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.literal_value(id) == Some(&value)
        }) {
            return existing;
        }
        let id = self.store.push_literal(value, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    // TODO(M4): pub fn union(&mut self, members: &mut Vec<TypeId>) -> TypeId
    //   Canonicalize before interning (architecture §3.3): flatten nested
    //   unions, sort by TypeId, dedup, normalize `X | never -> X`, collapse a
    //   1-member union to the member, then hash-cons like the helpers above.
    //
    // TODO(M2): pub fn intern_object(&mut self, obj: ObjectType) -> TypeId
    // TODO(M3): pub fn intern_function(&mut self, f: FunctionType) -> TypeId

    /// Look up an existing id in the dedup bucket for `hash`, accepting the first
    /// candidate for which `eq` confirms a real structural match.
    fn lookup(
        &self,
        hash: u64,
        eq: impl Fn(&Store, TypeId) -> bool,
    ) -> Option<TypeId> {
        let bucket = self.dedup.get(&hash)?;
        bucket.iter().copied().find(|&id| eq(&self.store, id))
    }
}

impl TypeTag {
    /// Convenience for assertions/tests: whether this tag is a cold side-table
    /// tag (vs the inline `Intrinsic`). Currently informational.
    #[allow(dead_code)] // TODO(M2+): used once side-table tags are constructed.
    pub fn has_side_table(self) -> bool {
        !matches!(self, TypeTag::Intrinsic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::LiteralValue;

    /// Hash-consing: structurally identical types share one `TypeId`
    /// (architecture §3, mvp-plan §6 — a correctness-critical invariant).
    #[test]
    fn hash_consing_dedups_intrinsics_and_literals() {
        let mut interner = Interner::with_intrinsics();

        // Re-interning an intrinsic returns the same well-known id.
        let number_again = interner.intern_intrinsic(IntrinsicKind::Number);
        assert_eq!(number_again, interner.well_known().number);

        // Equal literals collapse to one id; different literals do not.
        let a = interner.intern_literal(LiteralValue::Number(1.0));
        let b = interner.intern_literal(LiteralValue::Number(1.0));
        let c = interner.intern_literal(LiteralValue::Number(2.0));
        assert_eq!(a, b, "equal number literals must share an id");
        assert_ne!(a, c, "distinct number literals must not share an id");

        // Equal strings collapse; a string literal is distinct from a number
        // literal even if numerically suggestive.
        let s1 = interner.intern_literal(LiteralValue::String("x".to_string()));
        let s2 = interner.intern_literal(LiteralValue::String("x".to_string()));
        assert_eq!(s1, s2, "equal string literals must share an id");
        assert_ne!(s1, a, "string and number literals must be distinct types");

        // Booleans dedup per value.
        let t1 = interner.intern_literal(LiteralValue::Boolean(true));
        let t2 = interner.intern_literal(LiteralValue::Boolean(true));
        let f1 = interner.intern_literal(LiteralValue::Boolean(false));
        assert_eq!(t1, t2);
        assert_ne!(t1, f1);
    }

    /// The well-known intrinsic ids are assigned in `IntrinsicKind::ALL` order
    /// and are stable/small — the property the relation engine relies on.
    #[test]
    fn intrinsics_get_small_fixed_ids() {
        let interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        // Error is interned first (id 0), per ALL order.
        assert_eq!(wk.error, TypeId(0));
        // All ten intrinsics are distinct and within the first ten ids.
        let ids = [
            wk.error, wk.any, wk.unknown, wk.never, wk.void, wk.null, wk.undefined,
            wk.boolean, wk.number, wk.string,
        ];
        for (i, id) in ids.iter().enumerate() {
            assert!(id.0 < IntrinsicKind::ALL.len() as u32);
            // No duplicates among the well-known ids.
            assert_eq!(ids.iter().filter(|x| **x == *id).count(), 1, "dup at {i}");
        }
        assert_eq!(interner.store().len(), IntrinsicKind::ALL.len());
    }
}

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
use crate::types::repr::{
    FunctionType, IntrinsicKind, LiteralValue, ObjectType, ParameterType, PropertyType, TypeFlags,
    TypeTag,
};
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

    /// Intern an object type, returning the shared id.
    ///
    /// Canonicalization (mvp-plan §3.3): the property list is sorted by name
    /// before hashing/comparison so two object types that differ only in member
    /// order (`{ a; b }` vs `{ b; a }`) hash-cons to the **same** `TypeId`. The
    /// caller passes properties in source order; this owns the sort.
    pub fn intern_object(&mut self, mut object: ObjectType) -> TypeId {
        // Canonical order: sort by property name. The sort is stable, so the
        // relative order of any (illegal-in-the-subset) duplicate names is
        // preserved deterministically.
        object.properties.sort_by(|a, b| a.name.cmp(&b.name));

        let key = StructuralKey::Object(&object.properties);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store
                .object_type(id)
                .is_some_and(|existing| object_props_eq(&existing.properties, &object.properties))
        }) {
            return existing;
        }
        let id = self.store.push_object(object, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Reserve a **nominal** object type id with an empty body, returning the new
    /// id WITHOUT hash-consing it (M5 interfaces).
    ///
    /// This is the first half of the two-phase reserve-then-fill that makes
    /// recursive/mutually-recursive interfaces lowerable (mvp-plan M5, §3, §6.3):
    /// the id exists *before* the body is resolved, so a member annotation can
    /// reference the interface itself (`interface List { tail: List | null }`) or a
    /// sibling. The body is supplied later via [`Interner::fill_object`].
    ///
    /// Unlike [`Interner::intern_object`], a reserved interface is **not** added to
    /// the dedup index: an `interface` is nominal — two interface declarations with
    /// the same members are distinct types and each gets its own id — and, equally
    /// important, structurally hashing a self-referential object would not
    /// terminate (the hash would chase the cycle). Nominal ids therefore never go
    /// through `structural_hash`. (Aliases that resolve to a non-recursive
    /// structural type are still interned normally, so they keep sharing ids.)
    pub fn reserve_object(&mut self) -> TypeId {
        self.store
            .push_object(ObjectType::default(), TypeFlags::EMPTY)
    }

    /// Fill the body of a previously [reserved](Interner::reserve_object) object
    /// type in place (M5 interfaces, phase 2). The property list is sorted into
    /// canonical (name-sorted) order — matching `intern_object` — so the renderer
    /// and the relation engine see members in the same order they would for a
    /// structural object. The id is **not** added to the dedup index (it stays
    /// nominal); a no-op if `id` is not an object row.
    pub fn fill_object(&mut self, id: TypeId, mut object: ObjectType) {
        object.properties.sort_by(|a, b| a.name.cmp(&b.name));
        self.store.set_object(id, object);
    }

    /// Intern a function type, returning the shared id.
    ///
    /// Unlike object types, parameters are **positional** and are *not* sorted:
    /// parameter order is part of a function type's identity (mvp-plan §6.5; only
    /// object properties are canonicalized by name). Two function types hash-cons
    /// to the same `TypeId` only when their parameter lists match in order and
    /// their return types are the same interned id.
    pub fn intern_function(&mut self, function: FunctionType) -> TypeId {
        let key = StructuralKey::Function {
            params: &function.params,
            ret: function.ret,
        };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.function_type(id).is_some_and(|existing| {
                existing.ret == function.ret
                    && function_params_eq(&existing.params, &function.params)
            })
        }) {
            return existing;
        }
        let id = self.store.push_function(function, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a union type from its (un-canonicalized) member ids, returning the
    /// shared id of the canonical result.
    ///
    /// This is the heart of M4 (architecture §3.3 / mvp-plan §4.2). The members
    /// are canonicalized before interning so any two unions denoting the same set
    /// collapse to one `TypeId` (structural equality stays an integer compare):
    ///
    ///  1. **flatten** nested unions (`A | (B | C)` → `A | B | C`),
    ///  2. **absorb** the top type: any `any` member makes the whole union `any`;
    ///     otherwise any `unknown` member makes it `unknown` (the error type is
    ///     treated like `any` so cascades stay suppressed),
    ///  3. **drop** `never` members (`X | never` → `X`; `never` is the identity of
    ///     union),
    ///  4. **sort** by `TypeId` and **dedup** (`number | string` ≡ `string |
    ///     number`; `number | number` → `number`),
    ///  5. **collapse**: a 0-member union → `never`; a 1-member union → that
    ///     member (no union node is created).
    ///
    /// Only a genuine ≥ 2-member union is hash-consed into the store. The input
    /// `Vec` is consumed (drained); callers pass it by value.
    pub fn union(&mut self, mut members: Vec<TypeId>) -> TypeId {
        let wk = self.well_known;

        // 1. Flatten one level at a time until no member is itself a union. Union
        //    members are themselves canonical (interned through here), so a single
        //    expansion pass cannot reintroduce a nested union, but the loop is
        //    written defensively in case an un-interned union id is ever passed.
        let mut flat: Vec<TypeId> = Vec::with_capacity(members.len());
        while let Some(member) = members.pop() {
            match self.store.union_members(member) {
                Some(nested) => members.extend_from_slice(nested),
                None => flat.push(member),
            }
        }

        // 2. Absorption: `any`/error absorbs everything; failing that, `unknown`
        //    does. Either short-circuits the whole union to that top type.
        if flat.iter().any(|&m| m == wk.any || m == wk.error) {
            return wk.any;
        }
        if flat.contains(&wk.unknown) {
            return wk.unknown;
        }

        // 3. Drop `never` members — `never` is the identity element of `|`.
        flat.retain(|&m| m != wk.never);

        // 4. Sort by TypeId and dedup so member *order* and *multiplicity* do not
        //    affect identity.
        flat.sort_unstable();
        flat.dedup();

        // 5. Collapse the degenerate cases — never create a 0- or 1-member union.
        match flat.len() {
            0 => return wk.never,
            1 => return flat[0],
            _ => {}
        }

        // Hash-cons the canonical ≥ 2-member union like the other constructors.
        let key = StructuralKey::Union(&flat);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store
                .union_members(id)
                .is_some_and(|existing| existing == flat.as_slice())
        }) {
            return existing;
        }
        let id = self
            .store
            .push_union(flat.into_boxed_slice(), TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

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

/// Structural equality of two **canonical** (name-sorted) property lists — the
/// dedup-bucket tie-break for object types. Property types compare by `TypeId`,
/// which is itself canonical thanks to hash-consing, so nested object equality is
/// decided cheaply by id without recursing.
fn object_props_eq(a: &[PropertyType], b: &[PropertyType]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.name == y.name && x.optional == y.optional && x.ty == y.ty
        })
}

/// Positional equality of two parameter lists — the dedup-bucket tie-break for
/// function types. Parameters are compared in order (not sorted); types compare
/// by `TypeId` (canonical via hash-consing), so nested function/object equality
/// is decided by id without recursing.
fn function_params_eq(a: &[ParameterType], b: &[ParameterType]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.name == y.name && x.optional == y.optional && x.ty == y.ty
        })
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
    use crate::types::repr::{LiteralValue, ObjectType, PropertyType};

    /// Build a required property `name: ty`.
    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType {
            name: name.to_string(),
            ty,
            optional: false,
        }
    }

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

    /// Object hash-consing + canonicalization (mvp-plan §3.3, M2): two object
    /// types that differ only in member *order* must collapse to one `TypeId`,
    /// while a genuinely different shape (different property type) must not.
    #[test]
    fn object_canonicalization_dedups_by_member_set() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // `{ a: number; b: string }` and `{ b: string; a: number }` — same set,
        // different source order — hash-cons to the SAME id.
        let ab = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number), prop("b", wk.string)],
        });
        let ba = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.string), prop("a", wk.number)],
        });
        assert_eq!(ab, ba, "member order must not affect identity");

        // Re-interning the exact same shape returns the same id.
        let ab_again = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number), prop("b", wk.string)],
        });
        assert_eq!(ab, ab_again);

        // A different property *type* is a distinct object type.
        let ab_diff = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.string), prop("b", wk.string)],
        });
        assert_ne!(ab, ab_diff, "differing property types must not dedup");

        // A different property *set* (extra member) is distinct.
        let abc = interner.intern_object(ObjectType {
            properties: vec![
                prop("a", wk.number),
                prop("b", wk.string),
                prop("c", wk.boolean),
            ],
        });
        assert_ne!(ab, abc, "differing property sets must not dedup");

        // The canonical stored order is name-sorted regardless of input order.
        let stored = interner
            .store()
            .object_type(ba)
            .expect("ba is an object type");
        let names: Vec<&str> = stored.properties.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a", "b"], "stored order must be canonical (sorted)");

        // Nested object identity flows through: two outer objects whose nested
        // member is the *same* interned inner id dedup, exercising the by-id
        // property comparison.
        let outer1 = interner.intern_object(ObjectType {
            properties: vec![prop("a", ab)],
        });
        let outer2 = interner.intern_object(ObjectType {
            properties: vec![prop("a", ba)], // ba == ab
        });
        assert_eq!(outer1, outer2, "nested object identity must propagate");
    }

    /// Build a required parameter `name: ty`.
    fn param(name: &str, ty: TypeId) -> crate::types::repr::ParameterType {
        crate::types::repr::ParameterType {
            name: name.to_string(),
            ty,
            optional: false,
        }
    }

    /// Function hash-consing (M3): structurally identical function types share one
    /// `TypeId`, while a different parameter type, return type, or arity does not.
    /// Parameters are **positional**, so two functions whose parameter *types*
    /// appear in a different order remain distinct.
    #[test]
    fn function_interning_dedups_by_signature() {
        use crate::types::repr::FunctionType;

        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // `(x: number) => string`
        let f1 = interner.intern_function(FunctionType {
            params: vec![param("x", wk.number)],
            ret: wk.string,
        });
        // The exact same signature interns to the same id.
        let f1_again = interner.intern_function(FunctionType {
            params: vec![param("x", wk.number)],
            ret: wk.string,
        });
        assert_eq!(f1, f1_again, "identical function signatures must share an id");

        // A different return type is a distinct function type.
        let f_ret = interner.intern_function(FunctionType {
            params: vec![param("x", wk.number)],
            ret: wk.number,
        });
        assert_ne!(f1, f_ret, "differing return types must not dedup");

        // A different parameter type is a distinct function type.
        let f_param = interner.intern_function(FunctionType {
            params: vec![param("x", wk.string)],
            ret: wk.string,
        });
        assert_ne!(f1, f_param, "differing parameter types must not dedup");

        // Different arity is distinct.
        let f_arity = interner.intern_function(FunctionType {
            params: vec![param("x", wk.number), param("y", wk.string)],
            ret: wk.string,
        });
        assert_ne!(f1, f_arity, "differing arity must not dedup");

        // Parameters are positional: `(a: number, b: string)` and
        // `(a: string, b: number)` are the same arity with the same *set* of
        // parameter types but in a different order — they must NOT dedup.
        let ab = interner.intern_function(FunctionType {
            params: vec![param("a", wk.number), param("b", wk.string)],
            ret: wk.void,
        });
        let ba = interner.intern_function(FunctionType {
            params: vec![param("a", wk.string), param("b", wk.number)],
            ret: wk.void,
        });
        assert_ne!(ab, ba, "parameter order is part of function identity");
    }

    /// Union canonicalization + hash-consing (mvp-plan §3.3, M4 — a
    /// correctness-critical invariant). Order-independence, dedup, `never`-drop,
    /// single-member collapse, top-type absorption, and flatten are all asserted
    /// against the resulting `TypeId`s.
    #[test]
    fn union_canonicalization_and_hash_consing() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // Order-independence: `number | string` and `string | number` are the
        // same canonical `TypeId`.
        let ns = interner.union(vec![wk.number, wk.string]);
        let sn = interner.union(vec![wk.string, wk.number]);
        assert_eq!(ns, sn, "union member order must not affect identity");
        assert_eq!(
            interner.store().tag(ns),
            TypeTag::Union,
            "a 2-member union must be a union node"
        );
        // The stored members are sorted by TypeId.
        let members = interner
            .store()
            .union_members(ns)
            .expect("ns is a union")
            .to_vec();
        let mut sorted = members.clone();
        sorted.sort_unstable();
        assert_eq!(members, sorted, "stored members must be TypeId-sorted");
        assert_eq!(members.len(), 2);

        // Dedup: `number | number` collapses to plain `number` (no union node).
        let nn = interner.union(vec![wk.number, wk.number]);
        assert_eq!(nn, wk.number, "a duplicated single member collapses");

        // `never` is dropped: `number | never` → `number`.
        let n_never = interner.union(vec![wk.number, wk.never]);
        assert_eq!(n_never, wk.number, "never must be absorbed out of a union");

        // A union of a single distinct member collapses to that member.
        let single = interner.union(vec![wk.boolean]);
        assert_eq!(single, wk.boolean, "a 1-member union collapses to the member");

        // An empty union (or one of only `never`s) collapses to `never`.
        assert_eq!(interner.union(vec![]), wk.never, "empty union → never");
        assert_eq!(
            interner.union(vec![wk.never, wk.never]),
            wk.never,
            "a union of only never → never"
        );

        // Absorption: `any` swallows the whole union; `unknown` swallows when no
        // `any` is present.
        assert_eq!(
            interner.union(vec![wk.number, wk.any]),
            wk.any,
            "any absorbs the union"
        );
        assert_eq!(
            interner.union(vec![wk.number, wk.unknown]),
            wk.unknown,
            "unknown absorbs the union"
        );
        // `any` wins over `unknown` when both appear.
        assert_eq!(
            interner.union(vec![wk.unknown, wk.any]),
            wk.any,
            "any wins over unknown"
        );

        // Flatten: a nested union is expanded, then canonicalized. `(number |
        // string) | boolean` ≡ `number | string | boolean` (built directly),
        // sharing one id.
        let nsb_nested = interner.union(vec![ns, wk.boolean]);
        let nsb_flat = interner.union(vec![wk.number, wk.string, wk.boolean]);
        assert_eq!(nsb_nested, nsb_flat, "nested unions must flatten");
        assert_eq!(
            interner
                .store()
                .union_members(nsb_flat)
                .expect("nsb is a union")
                .len(),
            3,
            "flattened union has all three members"
        );

        // Re-interning the same canonical union returns the same id (hash-cons).
        let nsb_again = interner.union(vec![wk.boolean, wk.string, wk.number]);
        assert_eq!(nsb_flat, nsb_again, "identical unions hash-cons to one id");
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

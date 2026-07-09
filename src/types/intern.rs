//! The interner: hash-consing + canonicalization over the SoA `Store`.
//!
//! The dedup index maps structural hash to candidates, then confirms ties with
//! structural comparison. Intrinsics intern first, giving stable `WellKnown` ids.

use crate::types::hash::{structural_hash, StructuralKey};
use crate::types::repr::{
    ArrayType, ConditionalType, FunctionType, InstantiationType, IntrinsicKind, LiteralValue,
    MappedType, ObjectType, ParameterType, PropertyType, TemplateType, TupleType, TypeFlags,
    TypeParamId, TypeParamType, TypeTag,
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
    /// M28 string-intrinsic markers, used only as symbolic instantiation bases
    /// intercepted by evaluator identity checks.
    pub uppercase: TypeId,
    pub lowercase: TypeId,
    pub capitalize: TypeId,
    pub uncapitalize: TypeId,
}

impl WellKnown {
    /// Whether `id` is one of the four M28 string-intrinsic markers.
    pub fn is_string_intrinsic_marker(&self, id: TypeId) -> bool {
        id == self.uppercase
            || id == self.lowercase
            || id == self.capitalize
            || id == self.uncapitalize
    }
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
                uppercase: TypeId(0),
                lowercase: TypeId(0),
                capitalize: TypeId(0),
                uncapitalize: TypeId(0),
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
            uppercase: id_of(IntrinsicKind::Uppercase),
            lowercase: id_of(IntrinsicKind::Lowercase),
            capitalize: id_of(IntrinsicKind::Capitalize),
            uncapitalize: id_of(IntrinsicKind::Uncapitalize),
        };

        interner
    }

    /// Read-only access to the underlying store (the relation engine and
    /// renderer borrow it).
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Record a type parameter's `extends` constraint (M24) in the store-side
    /// constraint column, keyed by [`TypeParamId`]. The checker calls this while
    /// lowering a generic declaration's parameter list (frame active); the constraint
    /// is a side column, not folded into the interned `TypeParamType` identity.
    pub fn set_type_param_constraint(&mut self, id: TypeParamId, constraint: TypeId) {
        self.store.set_type_param_constraint(id, constraint);
    }

    /// Erase a type parameter's constraint (M24 — the `TK2313` circularity fix). A
    /// circular parameter records **no** constraint; see
    /// `Store::remove_type_param_constraint`.
    pub fn remove_type_param_constraint(&mut self, id: TypeParamId) {
        self.store.remove_type_param_constraint(id);
    }

    /// Record a reserved template row's alias display name (M28 round 3) — a
    /// rendering-only side column, never part of identity. The checker calls this
    /// right after `reserve_conditional`/`reserve_mapped` for a named alias, so a
    /// deferred instantiation renders as `Extract<K, string>` instead of the raw body.
    pub fn set_template_name(&mut self, id: TypeId, name: impl Into<String>) {
        self.store.set_template_name(id, name.into());
    }

    pub fn well_known(&self) -> WellKnown {
        self.well_known
    }

    /// Intern an intrinsic type, returning the shared id.
    pub fn intern_intrinsic(&mut self, kind: IntrinsicKind) -> TypeId {
        let key = StructuralKey::Intrinsic(kind);
        let hash = structural_hash(&key);
        if let Some(existing) =
            self.lookup(hash, |store, id| store.intrinsic_kind(id) == Some(kind))
        {
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
        if let Some(existing) =
            self.lookup(hash, |store, id| store.literal_value(id) == Some(&value))
        {
            return existing;
        }
        let id = self.store.push_literal(value, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern an object type. Properties are sorted by name before hashing and
    /// comparison, so source member order does not affect the shared `TypeId`.
    pub fn intern_object(&mut self, mut object: ObjectType) -> TypeId {
        // Canonical order: sort by property name. The sort is stable, so the
        // relative order of any (illegal-in-the-subset) duplicate names is
        // preserved deterministically.
        object.properties.sort_by(|a, b| a.name.cmp(&b.name));

        let key = StructuralKey::Object {
            properties: &object.properties,
            string_index: object.string_index,
            number_index: object.number_index,
            call_signatures: &object.call_signatures,
            construct_signatures: &object.construct_signatures,
        };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.object_type(id).is_some_and(|existing| {
                // M19: index signatures are part of identity, so two objects dedup
                // only when their members, index value types, and F1/WU2/WU3
                // signatures match.
                existing.string_index == object.string_index
                    && existing.number_index == object.number_index
                    && existing.call_signatures == object.call_signatures
                    && existing.construct_signatures == object.construct_signatures
                    && object_props_eq(&existing.properties, &object.properties)
            })
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

    /// Fill a reserved object in place. Properties are sorted like `intern_object`;
    /// the id stays nominal and is not added to the dedup index.
    pub fn fill_object(&mut self, id: TypeId, mut object: ObjectType) {
        object.properties.sort_by(|a, b| a.name.cmp(&b.name));
        self.store.set_object(id, object);
    }

    /// Intern a function type. Parameters are positional and never sorted; two
    /// functions share an id only when parameters match in order and return ids match.
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

    /// Intern a type-parameter type. Identity is the declaration-site
    /// [`TypeParamId`]; the source name is rendering-only.
    pub fn intern_type_param(&mut self, id: TypeParamId, name: impl Into<String>) -> TypeId {
        let key = StructuralKey::TypeParam(id);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, ty| {
            store.type_param(ty).map(|p| p.id) == Some(id)
        }) {
            return existing;
        }
        let interned = self.store.push_type_param(
            TypeParamType {
                id,
                name: name.into(),
            },
            TypeFlags::EMPTY,
        );
        self.dedup.entry(hash).or_default().push(interned);
        interned
    }

    /// Intern an array type. Identity is the canonical element id alone.
    pub fn intern_array(&mut self, element: TypeId) -> TypeId {
        let key = StructuralKey::Array(element);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.array_type(id).map(|a| a.element) == Some(element)
        }) {
            return existing;
        }
        let id = self
            .store
            .push_array(ArrayType { element }, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a tuple type. Identity is the ordered element-id list: order and
    /// arity are significant, and elements are stored in source order.
    pub fn intern_tuple(&mut self, elements: Vec<TypeId>) -> TypeId {
        let key = StructuralKey::Tuple(&elements);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store
                .tuple_type(id)
                .is_some_and(|existing| existing.elements == elements)
        }) {
            return existing;
        }
        let id = self
            .store
            .push_tuple(TupleType { elements }, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a readonly array/tuple wrapper. Identity is the wrapped operand id,
    /// but the tag is distinct from the operand so readonly sources cannot be treated
    /// as mutable arrays/tuples by structural accident.
    pub fn intern_readonly(&mut self, operand: TypeId) -> TypeId {
        let key = StructuralKey::Readonly(operand);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.readonly_operand(id) == Some(operand)
        }) {
            return existing;
        }
        let id = self.store.push_readonly(operand, TypeFlags::EMPTY);
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

        // Flatten defensively until no member is itself a union.
        let mut flat: Vec<TypeId> = Vec::with_capacity(members.len());
        while let Some(member) = members.pop() {
            match self.store.union_members(member) {
                Some(nested) => members.extend_from_slice(nested),
                None => flat.push(member),
            }
        }

        // Top-like members absorb the whole union and suppress cascades.
        if flat.iter().any(|&m| m == wk.any || m == wk.error) {
            return wk.any;
        }
        if flat.contains(&wk.unknown) {
            return wk.unknown;
        }

        // `never` is the identity element of `|`.
        flat.retain(|&m| m != wk.never);

        // Canonical member sets ignore order and multiplicity.
        flat.sort_unstable();
        flat.dedup();

        // Collapse 0-/1-member cases before store insertion.
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

    /// Intern an intersection type from its (un-canonicalized) member ids, returning
    /// the shared id of the canonical result (M31).
    ///
    /// The structural **dual** of [`Interner::union`]: same member-set canonicalization,
    /// but with **inverted** absorption/identity (architecture §3.3 / sprint plan):
    ///
    ///  1. **flatten** nested intersections (`A & (B & C)` → `A & B & C`),
    ///  2. **absorb**: any `any`/error member makes the whole intersection `any`
    ///     (same as union — cascades stay suppressed); otherwise any `never` member
    ///     makes it `never` (the DUAL of union's `unknown` absorption — `never` is the
    ///     bottom of `&`),
    ///  3. **drop** `unknown` members (`X & unknown` → `X`; `unknown` is the identity of
    ///     `&` — the DUAL of union dropping `never`),
    ///  4. **sort** by `TypeId` and **dedup** (`A & B` ≡ `B & A`; `A & A` → `A`),
    ///  5. **collapse**: a 0-member intersection → `unknown` (the DUAL of union → `never`);
    ///     a 1-member intersection → that member (no intersection node is created).
    ///
    /// Disjoint primitives (`string & number`) are **not** reduced to `never` (a
    /// documented deferral — the per-member target relation gives the correct verdict).
    /// Only a genuine ≥ 2-member intersection is hash-consed. The input `Vec` is consumed.
    pub fn intersection(&mut self, mut members: Vec<TypeId>) -> TypeId {
        let wk = self.well_known;

        // Flatten until no member is itself an intersection.
        let mut flat: Vec<TypeId> = Vec::with_capacity(members.len());
        while let Some(member) = members.pop() {
            match self.store.intersection_members(member) {
                Some(nested) => members.extend_from_slice(nested),
                None => flat.push(member),
            }
        }

        // `any`/error suppress cascades; otherwise `never` absorbs the intersection.
        if flat.iter().any(|&m| m == wk.any || m == wk.error) {
            return wk.any;
        }
        if flat.contains(&wk.never) {
            return wk.never;
        }

        // `unknown` is the identity element of `&`.
        flat.retain(|&m| m != wk.unknown);

        // Canonical member sets ignore order and multiplicity.
        flat.sort_unstable();
        flat.dedup();

        // Collapse 0-/1-member cases before store insertion.
        match flat.len() {
            0 => return wk.unknown,
            1 => return flat[0],
            _ => {}
        }

        // Hash-cons the canonical ≥ 2-member intersection like the other constructors.
        let key = StructuralKey::Intersection(&flat);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store
                .intersection_members(id)
                .is_some_and(|existing| existing == flat.as_slice())
        }) {
            return existing;
        }
        let id = self
            .store
            .push_intersection(flat.into_boxed_slice(), TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a conditional type. Identity is all four component ids in order plus
    /// `infer_count`, `distributive`, and `poisoned`; reserved templates are filled
    /// through [`Interner::fill_conditional`].
    pub fn intern_conditional(&mut self, conditional: ConditionalType) -> TypeId {
        let key = StructuralKey::Conditional {
            check: conditional.check,
            extends_ty: conditional.extends_ty,
            true_branch: conditional.true_branch,
            false_branch: conditional.false_branch,
            infer_count: conditional.infer_count,
            distributive: conditional.distributive,
            poisoned: conditional.poisoned,
        };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.conditional_type(id).is_some_and(|existing| {
                existing.check == conditional.check
                    && existing.extends_ty == conditional.extends_ty
                    && existing.true_branch == conditional.true_branch
                    && existing.false_branch == conditional.false_branch
                    && existing.infer_count == conditional.infer_count
                    && existing.distributive == conditional.distributive
                    && existing.poisoned == conditional.poisoned
            })
        }) {
            return existing;
        }
        let id = self.store.push_conditional(conditional, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Reserve a recursive conditional-alias template id without hash-consing. The
    /// id exists before lowering so self-recursive references can point at it as a
    /// lazy [`InstantiationType`] base; `fill_conditional` supplies the real body.
    pub fn reserve_conditional(&mut self) -> TypeId {
        // A placeholder body; overwritten by `fill_conditional`. The error type is a
        // safe neutral filler (never observed before the fill).
        let error = self.well_known.error;
        self.store.push_conditional(
            ConditionalType {
                check: error,
                extends_ty: error,
                true_branch: error,
                false_branch: error,
                infer_count: 0,
                distributive: false,
                poisoned: false,
            },
            TypeFlags::EMPTY,
        )
    }

    /// Fill the body of a previously [reserved](Interner::reserve_conditional)
    /// conditional template in place (M25). The id is **not** added to the dedup index
    /// (it stays nominal); a no-op if `id` is not a conditional row.
    pub fn fill_conditional(&mut self, id: TypeId, conditional: ConditionalType) {
        self.store.set_conditional(id, conditional);
    }

    /// Intern a **lazy instantiation** `substitute(base, args)` (M25). `args` are sorted
    /// by [`TypeParamId`] here so two equal instantiations share one id.
    pub fn intern_instantiation(&mut self, base: TypeId, mut args: Vec<(TypeParamId, TypeId)>) -> TypeId {
        args.sort_by_key(|(param, _)| param.0);
        let key = StructuralKey::Instantiation { base, args: &args };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store
                .instantiation_type(id)
                .is_some_and(|existing| existing.base == base && existing.args == args)
        }) {
            return existing;
        }
        let id = self
            .store
            .push_instantiation(InstantiationType { base, args }, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern an **`infer` binder** at the given de Bruijn index (M25). Identity is the
    /// index alone, so alpha-equivalent conditionals hash-cons to one node.
    pub fn intern_infer(&mut self, index: u32) -> TypeId {
        let key = StructuralKey::Infer(index);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| store.infer_index(id) == Some(index)) {
            return existing;
        }
        let id = self.store.push_infer(index, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a **mapped type** `{ [K in S]: V }` (M26). Identity is its whole
    /// [`MappedType`] shape (homomorphic flag, key source, value template, the M28
    /// modifiers source, and both modifier operators), so two structurally equal
    /// mapped types share one id.
    pub fn intern_mapped(&mut self, mapped: MappedType) -> TypeId {
        let key = StructuralKey::Mapped {
            homomorphic: mapped.homomorphic,
            key_source: mapped.key_source,
            value_template: mapped.value_template,
            modifiers_source: mapped.modifiers_source,
            optional_modifier: mapped.optional_modifier,
            readonly_modifier: mapped.readonly_modifier,
        };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.mapped_type(id).is_some_and(|existing| {
                existing.homomorphic == mapped.homomorphic
                    && existing.key_source == mapped.key_source
                    && existing.value_template == mapped.value_template
                    // M28: identity-bearing — maps differing only in modifiers source
                    // must not conflate (leader constraint, sprint WU2 item 2).
                    && existing.modifiers_source == mapped.modifiers_source
                    && existing.optional_modifier == mapped.optional_modifier
                    && existing.readonly_modifier == mapped.readonly_modifier
            })
        }) {
            return existing;
        }
        let id = self.store.push_mapped(mapped, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Reserve a recursive mapped-alias template id without hash-consing. The id
    /// exists before lowering so self-recursive references can point at it as a
    /// lazy [`InstantiationType`] base; `fill_mapped` supplies the real body.
    pub fn reserve_mapped(&mut self) -> TypeId {
        // A placeholder body; overwritten by `fill_mapped`. The error type is a safe
        // neutral filler (never observed before the fill).
        let error = self.well_known.error;
        self.store.push_mapped(
            MappedType {
                homomorphic: false,
                key_source: error,
                value_template: error,
                modifiers_source: None,
                optional_modifier: crate::types::repr::ModifierOp::Keep,
                readonly_modifier: crate::types::repr::ModifierOp::Keep,
            },
            TypeFlags::EMPTY,
        )
    }

    /// Fill the body of a previously [reserved](Interner::reserve_mapped) mapped
    /// template in place (M28). The id is **not** added to the dedup index (it stays
    /// nominal); a no-op if `id` is not a mapped row.
    pub fn fill_mapped(&mut self, id: TypeId, mapped: MappedType) {
        self.store.set_mapped(id, mapped);
    }

    /// Intern a **deferred `keyof`** node (M28). Identity is the operand id alone, so
    /// `keyof T` hash-conses to one node. Constructed only for a pending-computation
    /// operand — see [`TypeTag::Keyof`].
    pub fn intern_keyof(&mut self, operand: TypeId) -> TypeId {
        let key = StructuralKey::Keyof(operand);
        let hash = structural_hash(&key);
        if let Some(existing) =
            self.lookup(hash, |store, id| store.keyof_operand(id) == Some(operand))
        {
            return existing;
        }
        let id = self.store.push_keyof(operand, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a **template literal type** `` `a${T}b` `` (M27). Identity is its ordered
    /// text segments and hole ids (a template is a sequence — position is meaning), so
    /// two structurally equal templates share one id. The caller supplies the texts and
    /// holes in order (`texts.len() == holes.len() + 1`).
    pub fn intern_template(&mut self, template: TemplateType) -> TypeId {
        let key = StructuralKey::Template {
            texts: &template.texts,
            holes: &template.holes,
        };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.template_type(id).is_some_and(|existing| {
                existing.texts == template.texts && existing.holes == template.holes
            })
        }) {
            return existing;
        }
        let id = self.store.push_template(template, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern the **mapped-value placeholder** (`T[K]` — M26). Identity is the tag
    /// alone, so every placeholder hash-conses to one node.
    pub fn intern_mapped_value(&mut self) -> TypeId {
        let key = StructuralKey::MappedValue;
        let hash = structural_hash(&key);
        if let Some(existing) =
            self.lookup(hash, |store, id| store.tag(id) == TypeTag::MappedValue)
        {
            return existing;
        }
        let id = self.store.push_mapped_value(TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Look up an existing id in the dedup bucket for `hash`, accepting the first
    /// candidate for which `eq` confirms a real structural match.
    fn lookup(&self, hash: u64, eq: impl Fn(&Store, TypeId) -> bool) -> Option<TypeId> {
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
            // Match every identity-bearing property field; see `PropertyType`.
            x.name == y.name
                && x.optional == y.optional
                && x.ty == y.ty
                && x.visibility == y.visibility
                && x.declaring_class == y.declaring_class
                && x.readonly == y.readonly
                && x.is_accessor == y.is_accessor
        })
}

/// Positional equality of two parameter lists — the dedup-bucket tie-break for
/// function types. Parameters are compared in order (not sorted); types compare
/// by `TypeId` (canonical via hash-consing), so nested function/object equality
/// is decided by id without recursing.
fn function_params_eq(a: &[ParameterType], b: &[ParameterType]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.name == y.name && x.optional == y.optional && x.ty == y.ty)
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

    /// Build a required public property `name: ty`.
    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType::public(name, ty)
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
            ..Default::default()
        });
        let ba = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.string), prop("a", wk.number)],
            ..Default::default()
        });
        assert_eq!(ab, ba, "member order must not affect identity");

        // Re-interning the exact same shape returns the same id.
        let ab_again = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number), prop("b", wk.string)],
            ..Default::default()
        });
        assert_eq!(ab, ab_again);

        // A different property *type* is a distinct object type.
        let ab_diff = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.string), prop("b", wk.string)],
            ..Default::default()
        });
        assert_ne!(ab, ab_diff, "differing property types must not dedup");

        // A different property *set* (extra member) is distinct.
        let abc = interner.intern_object(ObjectType {
            properties: vec![
                prop("a", wk.number),
                prop("b", wk.string),
                prop("c", wk.boolean),
            ],
            ..Default::default()
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
            ..Default::default()
        });
        let outer2 = interner.intern_object(ObjectType {
            properties: vec![prop("a", ba)], // ba == ab,
            ..Default::default()
        });
        assert_eq!(outer1, outer2, "nested object identity must propagate");
    }

    /// M19: index signatures are part of object identity, distinguished by
    /// presence, index kind, value type, and coexistence with named members.
    #[test]
    fn index_signature_is_part_of_object_identity() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // `{ [k: string]: number }` interns consistently.
        let str_idx_num_a = interner.intern_object(ObjectType {
            string_index: Some(wk.number),
            ..Default::default()
        });
        let str_idx_num_b = interner.intern_object(ObjectType {
            string_index: Some(wk.number),
            ..Default::default()
        });
        assert_eq!(
            str_idx_num_a, str_idx_num_b,
            "{{ [k: string]: number }} must intern consistently"
        );

        // Distinct from the empty object `{}` (presence of the index signature).
        let empty = interner.intern_object(ObjectType::default());
        assert_ne!(str_idx_num_a, empty, "{{ [k: string]: number }} ≠ {{}}");

        // Distinct value type: `{ [k: string]: string }`.
        let str_idx_str = interner.intern_object(ObjectType {
            string_index: Some(wk.string),
            ..Default::default()
        });
        assert_ne!(
            str_idx_num_a, str_idx_str,
            "differing index value type ⇒ distinct identity"
        );

        // Distinct index kind: `{ [i: number]: number }` (number, not string).
        let num_idx_num = interner.intern_object(ObjectType {
            number_index: Some(wk.number),
            ..Default::default()
        });
        assert_ne!(
            str_idx_num_a, num_idx_num,
            "string-index ≠ number-index of the same value type"
        );

        // A named member coexists with the index signature in the identity:
        // `{ a: number }` ≠ `{ a: number; [k: string]: number }`.
        let a_only = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
            ..Default::default()
        });
        let a_and_index = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
            string_index: Some(wk.number),
            ..Default::default()
        });
        assert_ne!(
            a_only, a_and_index,
            "adding an index signature changes object identity"
        );
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
        assert_eq!(
            f1, f1_again,
            "identical function signatures must share an id"
        );

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
        assert_eq!(
            single, wk.boolean,
            "a 1-member union collapses to the member"
        );

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

    /// M31 intersection canonicalization: dual absorption/identity rules, flattening,
    /// dedup, collapse, and distinctness from unions over the same member set.
    #[test]
    fn intersection_canonicalization_and_hash_consing() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // Two object members so a genuine ≥ 2-member node is built (disjoint primitives
        // are NOT reduced, but two objects give a clean structural node).
        let a = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
            ..Default::default()
        });
        let b = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.string)],
            ..Default::default()
        });

        // Order-independence: `A & B` and `B & A` are the same canonical `TypeId`.
        let ab = interner.intersection(vec![a, b]);
        let ba = interner.intersection(vec![b, a]);
        assert_eq!(ab, ba, "intersection member order must not affect identity");
        assert_eq!(
            interner.store().tag(ab),
            TypeTag::Intersection,
            "a 2-member intersection must be an intersection node"
        );
        // The stored members are sorted by TypeId.
        let members = interner
            .store()
            .intersection_members(ab)
            .expect("ab is an intersection")
            .to_vec();
        let mut sorted = members.clone();
        sorted.sort_unstable();
        assert_eq!(members, sorted, "stored members must be TypeId-sorted");
        assert_eq!(members.len(), 2);

        // Dedup: `A & A` collapses to plain `A` (no intersection node).
        let aa = interner.intersection(vec![a, a]);
        assert_eq!(aa, a, "a duplicated single member collapses");

        // `unknown` is dropped: `A & unknown` → `A` (unknown is the identity of `&`).
        let a_unknown = interner.intersection(vec![a, wk.unknown]);
        assert_eq!(a_unknown, a, "unknown must be dropped from an intersection");

        // `never` absorbs: `A & never` → `never` (never is the bottom of `&`).
        let a_never = interner.intersection(vec![a, wk.never]);
        assert_eq!(a_never, wk.never, "never absorbs the whole intersection");

        // A single distinct member collapses to that member.
        let single = interner.intersection(vec![a]);
        assert_eq!(single, a, "a 1-member intersection collapses to the member");

        // An empty intersection (or one of only `unknown`s) collapses to `unknown`.
        assert_eq!(
            interner.intersection(vec![]),
            wk.unknown,
            "empty intersection → unknown"
        );
        assert_eq!(
            interner.intersection(vec![wk.unknown, wk.unknown]),
            wk.unknown,
            "an intersection of only unknown → unknown"
        );

        // Absorption: `any` swallows the whole intersection (cascade suppression),
        // winning over everything including `never`.
        assert_eq!(
            interner.intersection(vec![a, wk.any]),
            wk.any,
            "any absorbs the intersection"
        );
        assert_eq!(
            interner.intersection(vec![wk.never, wk.any]),
            wk.any,
            "any wins over never"
        );

        // Flatten: `(A & B) & C` ≡ `A & B & C` (built directly), sharing one id.
        let c = interner.intern_object(ObjectType {
            properties: vec![prop("c", wk.boolean)],
            ..Default::default()
        });
        let abc_nested = interner.intersection(vec![ab, c]);
        let abc_flat = interner.intersection(vec![a, b, c]);
        assert_eq!(abc_nested, abc_flat, "nested intersections must flatten");
        assert_eq!(
            interner
                .store()
                .intersection_members(abc_flat)
                .expect("abc is an intersection")
                .len(),
            3,
            "flattened intersection has all three members"
        );

        // Re-interning the same canonical intersection returns the same id (hash-cons).
        let abc_again = interner.intersection(vec![c, b, a]);
        assert_eq!(abc_flat, abc_again, "identical intersections hash-cons to one id");

        // A union and an intersection over the same member set never collide (distinct
        // discriminants), even though both are 2-member sets of {A, B}.
        let union_ab = interner.union(vec![a, b]);
        assert_ne!(
            union_ab, ab,
            "a union and an intersection over the same set must be distinct types"
        );
    }

    /// Array hash-consing (M17): an array's identity is its element id alone, so
    /// `number[]` interns consistently, `number[]` ≠ `string[]`, and `number[][]`
    /// nests (its element is `number[]`). The element is canonical, so the dedup
    /// tie-break is an id compare.
    #[test]
    fn array_interning_dedups_by_element() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // `number[]` interns to one shared id regardless of how many times built.
        let num_arr_a = interner.intern_array(wk.number);
        let num_arr_b = interner.intern_array(wk.number);
        assert_eq!(num_arr_a, num_arr_b, "number[] must intern consistently");
        assert_eq!(
            interner.store().tag(num_arr_a),
            TypeTag::Array,
            "an array is an Array-tagged row"
        );
        assert_eq!(
            interner.store().array_type(num_arr_a).map(|a| a.element),
            Some(wk.number),
            "the stored element is `number`"
        );

        // A different element is a distinct array type.
        let str_arr = interner.intern_array(wk.string);
        assert_ne!(num_arr_a, str_arr, "number[] ≠ string[]");

        // Nesting: `number[][]` has `number[]` as its element and is distinct from
        // `number[]`.
        let num_arr_arr = interner.intern_array(num_arr_a);
        assert_ne!(num_arr_arr, num_arr_a, "number[][] ≠ number[]");
        assert_eq!(
            interner.store().array_type(num_arr_arr).map(|a| a.element),
            Some(num_arr_a),
            "number[][]'s element is number[]"
        );

        // An array of a union element interns by that (canonical) union id —
        // member order in the union does not change the array's identity.
        let union_ns = interner.union(vec![wk.number, wk.string]);
        let union_sn = interner.union(vec![wk.string, wk.number]);
        let union_arr_a = interner.intern_array(union_ns);
        let union_arr_b = interner.intern_array(union_sn);
        assert_eq!(
            union_arr_a, union_arr_b,
            "(number | string)[] interns consistently via the canonical union element"
        );
    }

    /// Tuple hash-consing (M18): a tuple's identity is its **ordered** element
    /// list, so `[number, string]` interns consistently while `[number, string]`,
    /// `[string, number]` (order differs), and `[number]` (arity differs) are all
    /// distinct. Order is significant — the list is never sorted (unlike a union).
    #[test]
    fn tuple_interning_is_order_significant() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // `[number, string]` interns to one shared id regardless of how many times
        // built.
        let ns_a = interner.intern_tuple(vec![wk.number, wk.string]);
        let ns_b = interner.intern_tuple(vec![wk.number, wk.string]);
        assert_eq!(ns_a, ns_b, "[number, string] must intern consistently");
        assert_eq!(
            interner.store().tag(ns_a),
            TypeTag::Tuple,
            "a tuple is a Tuple-tagged row"
        );

        // Order matters: `[string, number]` is a DISTINCT tuple (NOT sorted into the
        // same canonical form a union would be).
        let sn = interner.intern_tuple(vec![wk.string, wk.number]);
        assert_ne!(
            ns_a, sn,
            "[number, string] ≠ [string, number] (order-significant)"
        );

        // Arity matters: `[number]` is distinct from `[number, string]`.
        let single = interner.intern_tuple(vec![wk.number]);
        assert_ne!(ns_a, single, "[number, string] ≠ [number] (arity differs)");
        assert_ne!(sn, single, "[string, number] ≠ [number]");

        // The stored element list preserves source order exactly.
        let stored = interner
            .store()
            .tuple_type(ns_a)
            .expect("ns_a is a tuple")
            .elements
            .clone();
        assert_eq!(
            stored,
            vec![wk.number, wk.string],
            "stored order is source order"
        );

        // The empty tuple `[]` is a valid distinct tuple (and interns consistently).
        let empty_a = interner.intern_tuple(vec![]);
        let empty_b = interner.intern_tuple(vec![]);
        assert_eq!(empty_a, empty_b, "[] interns consistently");
        assert_ne!(empty_a, single, "[] ≠ [number]");

        // A tuple is distinct from the array of the same element (different tags).
        let num_arr = interner.intern_array(wk.number);
        let num_tuple = interner.intern_tuple(vec![wk.number]);
        assert_ne!(num_arr, num_tuple, "number[] ≠ [number]");

        // Nesting: a tuple element may itself be a tuple, interned by the inner id.
        let nested_a = interner.intern_tuple(vec![ns_a, wk.boolean]);
        let nested_b = interner.intern_tuple(vec![ns_b, wk.boolean]); // ns_b == ns_a
        assert_eq!(
            nested_a, nested_b,
            "nested tuple identity propagates by element id"
        );
    }

    /// Template hash-consing (M27): a template's identity is its ordered text segments +
    /// hole ids, so `` `a-${string}` `` interns consistently, differs from
    /// `` `b-${string}` `` (text) and `` `a-${number}` `` (hole), and adjacent-hole
    /// templates (empty interior text) are representable and distinct.
    #[test]
    fn template_interning_is_structural() {
        use crate::types::repr::TemplateType;
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        let mk = |interner: &mut Interner, texts: &[&str], holes: Vec<TypeId>| {
            interner.intern_template(TemplateType {
                texts: texts.iter().map(|s| s.to_string()).collect(),
                holes,
            })
        };

        let a = mk(&mut interner, &["a-", ""], vec![wk.string]);
        let a2 = mk(&mut interner, &["a-", ""], vec![wk.string]);
        assert_eq!(a, a2, "identical templates hash-cons to one id");
        assert_eq!(interner.store().tag(a), TypeTag::Template);

        let b = mk(&mut interner, &["b-", ""], vec![wk.string]);
        assert_ne!(a, b, "differing text ⇒ distinct template");

        let num = mk(&mut interner, &["a-", ""], vec![wk.number]);
        assert_ne!(a, num, "differing hole type ⇒ distinct template");

        // Adjacent holes (empty interior text) are representable and distinct from a
        // separated form.
        let adjacent = mk(&mut interner, &["", "", ""], vec![wk.string, wk.string]);
        let separated = mk(&mut interner, &["", "-", ""], vec![wk.string, wk.string]);
        assert_ne!(adjacent, separated, "adjacency is part of identity");
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
            wk.error,
            wk.any,
            wk.unknown,
            wk.never,
            wk.void,
            wk.null,
            wk.undefined,
            wk.boolean,
            wk.number,
            wk.string,
        ];
        for (i, id) in ids.iter().enumerate() {
            assert!(id.0 < IntrinsicKind::ALL.len() as u32);
            // No duplicates among the well-known ids.
            assert_eq!(ids.iter().filter(|x| **x == *id).count(), 1, "dup at {i}");
        }
        assert_eq!(interner.store().len(), IntrinsicKind::ALL.len());
    }
}

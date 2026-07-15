//! Set-operator and computed-type interners: union/intersection, conditional,
//! instantiation, mapped, keyof, template.

use super::*;
use crate::types::repr::{
    ClassId, ClassInstanceType, ConditionalType, DeferredIndexedAccessType, InstantiationType,
    MappedType, TemplateType,
};

impl Interner {
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
    ///  2. **absorb**, in tsc's order (Never before Any — `getIntersectionType`): the
    ///     internal **error** type absorbs first (→ `any`, treated like `any` so
    ///     cascades stay suppressed); then any `never` member makes it `never` (the
    ///     annihilator of `&`, so `any & never` is `never`, matching tsc); then a
    ///     remaining `any` member absorbs (→ `any`). The error-first ordering keeps the
    ///     deliberate cascade suppression for the distinct internal error type
    ///     (`error & never` stays `any`), while source-level `any & never` normalizes to
    ///     `never`.
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

        // Absorption order mirrors tsc's `getIntersectionType` (Never before Any). The
        // internal error type absorbs FIRST (→ `any`) so a cascade stays suppressed even
        // against `never` (`error & never` = `any`); then `never` annihilates (so
        // source-level `any & never` = `never`); then a remaining `any` absorbs.
        if flat.contains(&wk.error) {
            return wk.any;
        }
        if flat.contains(&wk.never) {
            return wk.never;
        }
        if flat.contains(&wk.any) {
            return wk.any;
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
        let id = self.store.push_conditional(
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
        );
        self.register_reserved_type(id, super::ReservedTypeKind::Conditional)
    }

    /// Fill the body of a previously [reserved](Interner::reserve_conditional)
    /// conditional template in place (M25). The id is **not** added to the dedup index
    /// (it stays nominal).
    pub fn fill_conditional(&mut self, id: TypeId, conditional: ConditionalType) {
        self.fill_reserved_type_batch(vec![super::ReservedTypeFill::Conditional(id, conditional)])
            .expect("fill_conditional requires one pending reserved conditional");
    }

    /// Intern a **lazy instantiation** `substitute(base, args)` (M25). `args` are sorted
    /// by [`TypeParamId`] here so two equal instantiations share one id.
    pub fn intern_instantiation(
        &mut self,
        base: TypeId,
        mut args: Vec<(TypeParamId, TypeId)>,
    ) -> TypeId {
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

    /// Intern an immutable class application. The argument order is declaration
    /// order and is never canonicalized or routed through alias instantiation.
    pub(crate) fn intern_class_instance(&mut self, class: ClassId, args: Vec<TypeId>) -> TypeId {
        let key = StructuralKey::ClassInstance { class, args: &args };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store
                .class_instance_type(id)
                .is_some_and(|existing| existing.class == class && existing.args == args)
        }) {
            return existing;
        }
        let id = self
            .store
            .push_class_instance(ClassInstanceType { class, args }, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    /// Intern a deferred indexed access by its ordered `(object, index)` pair.
    pub(crate) fn intern_deferred_indexed_access(
        &mut self,
        object: TypeId,
        index: TypeId,
    ) -> TypeId {
        let key = StructuralKey::DeferredIndexedAccess { object, index };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store
                .deferred_indexed_access_type(id)
                .is_some_and(|existing| existing.object == object && existing.index == index)
        }) {
            return existing;
        }
        let id = self.store.push_deferred_indexed_access(
            DeferredIndexedAccessType { object, index },
            TypeFlags::EMPTY,
        );
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
        let id = self.store.push_mapped(
            MappedType {
                homomorphic: false,
                key_source: error,
                value_template: error,
                modifiers_source: None,
                optional_modifier: crate::types::repr::ModifierOp::Keep,
                readonly_modifier: crate::types::repr::ModifierOp::Keep,
            },
            TypeFlags::EMPTY,
        );
        self.register_reserved_type(id, super::ReservedTypeKind::Mapped)
    }

    /// Fill the body of a previously [reserved](Interner::reserve_mapped) mapped
    /// template in place (M28). The id is **not** added to the dedup index (it stays
    /// nominal).
    pub fn fill_mapped(&mut self, id: TypeId, mapped: MappedType) {
        self.fill_reserved_type_batch(vec![super::ReservedTypeFill::Mapped(id, mapped)])
            .expect("fill_mapped requires one pending reserved mapped type");
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
        if let Some(existing) = self.lookup(hash, |store, id| store.tag(id) == TypeTag::MappedValue)
        {
            return existing;
        }
        let id = self.store.push_mapped_value(TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }
}

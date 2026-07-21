//! Composite-type interners: objects, functions, arrays, tuples, readonly wrappers.

use super::*;
use crate::types::repr::{ArrayType, FunctionType, ObjectType, TupleType};

impl Interner {
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
        let id = self
            .store
            .push_object(ObjectType::default(), TypeFlags::EMPTY);
        self.register_reserved_type(id, super::ReservedTypeKind::Object)
    }

    /// Fill a reserved object in place. Properties are sorted like `intern_object`;
    /// the id stays nominal and is not added to the dedup index.
    pub fn fill_object(&mut self, id: TypeId, object: ObjectType) {
        self.fill_reserved_type_batch(vec![super::ReservedTypeFill::Object(id, object)])
            .expect("fill_object requires one pending reserved object");
    }

    /// Close an unpublished object reservation without making it structural.
    pub(crate) fn abandon_reserved_object(
        &mut self,
        id: TypeId,
    ) -> Result<(), super::ReservedTypeFillError> {
        self.fill_reserved_type_batch(vec![super::ReservedTypeFill::Object(
            id,
            ObjectType::default(),
        )])
    }

    /// Promote a caller-proven acyclic alias reservation into structural identity.
    pub(crate) fn promote_caller_certified_acyclic_reserved_object(
        &mut self,
        id: TypeId,
    ) -> Result<TypeId, super::ReservedObjectPromotionError> {
        let Some(reserved) = self.reserved_types.get(&id).copied() else {
            return Err(super::ReservedObjectPromotionError::NotReserved(id));
        };
        if reserved.kind != super::ReservedTypeKind::Object {
            return Err(super::ReservedObjectPromotionError::KindMismatch(id));
        }
        if reserved.state != super::ReservedTypeState::Frozen {
            return Err(super::ReservedObjectPromotionError::NotFrozen(id));
        }
        let Some(object) = self.store.object_type(id) else {
            return Err(super::ReservedObjectPromotionError::InvalidBackingRow(id));
        };
        let key = StructuralKey::Object {
            properties: &object.properties,
            string_index: object.string_index,
            number_index: object.number_index,
            call_signatures: &object.call_signatures,
            construct_signatures: &object.construct_signatures,
        };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, candidate| {
            store.object_type(candidate).is_some_and(|candidate| {
                candidate.string_index == object.string_index
                    && candidate.number_index == object.number_index
                    && candidate.call_signatures == object.call_signatures
                    && candidate.construct_signatures == object.construct_signatures
                    && object_props_eq(&candidate.properties, &object.properties)
            })
        }) {
            return Ok(existing);
        }

        self.reserved_types
            .remove(&id)
            .expect("validated object reservation remains registered");
        self.dedup.entry(hash).or_default().push(id);
        Ok(id)
    }

    /// Intern a function type. Generic binders and parameters are positional and
    /// never sorted; all call-observable fields participate in identity.
    pub fn intern_function(&mut self, function: FunctionType) -> TypeId {
        let key = StructuralKey::Function {
            type_params: &function.type_params,
            receiver: function.receiver,
            params: &function.params,
            ret: function.ret,
        };
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.function_type(id).is_some_and(|existing| {
                existing.type_params == function.type_params
                    && existing.receiver == function.receiver
                    && existing.ret == function.ret
                    && function_params_eq(&existing.params, &function.params)
            })
        }) {
            return existing;
        }
        let id = self.store.push_function(function, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
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

    /// Intern a fixed tuple type. Identity is the ordered element-id list: order
    /// and arity are significant, and elements are stored in source order.
    pub fn intern_tuple(&mut self, elements: Vec<TypeId>) -> TypeId {
        self.intern_tuple_type(TupleType::fixed(elements))
    }

    /// Intern a tuple type, including an optional rest segment.
    pub fn intern_tuple_type(&mut self, tuple: TupleType) -> TypeId {
        let key = StructuralKey::Tuple(&tuple);
        let hash = structural_hash(&key);
        if let Some(existing) = self.lookup(hash, |store, id| {
            store
                .tuple_type(id)
                .is_some_and(|existing| existing == &tuple)
        }) {
            return existing;
        }
        let id = self.store.push_tuple(tuple, TypeFlags::EMPTY);
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
}

//! Interner-only capability used while class surfaces are constructed.

use crate::types::repr::{
    ClassId, ConditionalType, FunctionType, LiteralValue, MappedType, ObjectType, TemplateType,
    TupleType, TypeParamId,
};
use crate::types::store::{Store, TypeId};
use crate::types::{Interner, WellKnown};

/// Deliberately exposes only immutable-node construction. It has no evaluator,
/// projector, relation engine, checker pass, or name-resolution callback.
pub(in crate::check::checker) struct SurfaceTypeFactory<'a> {
    interner: &'a mut Interner,
}

impl<'a> SurfaceTypeFactory<'a> {
    pub(in crate::check::checker) fn new(interner: &'a mut Interner) -> Self {
        SurfaceTypeFactory { interner }
    }

    pub(in crate::check::checker) fn store(&self) -> &Store {
        self.interner.store()
    }

    pub(in crate::check::checker) fn well_known(&self) -> WellKnown {
        self.interner.well_known()
    }

    pub(in crate::check::checker) fn intern_literal(&mut self, value: LiteralValue) -> TypeId {
        self.interner.intern_literal(value)
    }

    pub(in crate::check::checker) fn intern_type_param(
        &mut self,
        id: TypeParamId,
        name: impl Into<String>,
    ) -> TypeId {
        self.interner.intern_type_param(id, name)
    }

    pub(in crate::check::checker) fn intern_object(&mut self, object: ObjectType) -> TypeId {
        self.interner.intern_object(object)
    }

    pub(in crate::check::checker) fn set_type_param_constraint(
        &mut self,
        id: TypeParamId,
        constraint: TypeId,
    ) -> bool {
        self.interner.set_type_param_constraint(id, constraint)
    }

    pub(in crate::check::checker) fn intern_function(&mut self, function: FunctionType) -> TypeId {
        self.interner.intern_function(function)
    }

    pub(in crate::check::checker) fn intern_array(&mut self, element: TypeId) -> TypeId {
        self.interner.intern_array(element)
    }

    pub(in crate::check::checker) fn intern_tuple(&mut self, tuple: TupleType) -> TypeId {
        self.interner.intern_tuple_type(tuple)
    }

    pub(in crate::check::checker) fn intern_readonly(&mut self, operand: TypeId) -> TypeId {
        self.interner.intern_readonly(operand)
    }

    pub(in crate::check::checker) fn union(&mut self, members: Vec<TypeId>) -> TypeId {
        self.interner.union(members)
    }

    pub(in crate::check::checker) fn intersection(&mut self, members: Vec<TypeId>) -> TypeId {
        self.interner.intersection(members)
    }

    pub(in crate::check::checker) fn intern_conditional(
        &mut self,
        conditional: ConditionalType,
    ) -> TypeId {
        self.interner.intern_conditional(conditional)
    }

    pub(in crate::check::checker) fn intern_infer(&mut self, index: u32) -> TypeId {
        self.interner.intern_infer(index)
    }

    pub(in crate::check::checker) fn intern_mapped_value(&mut self) -> TypeId {
        self.interner.intern_mapped_value()
    }

    pub(in crate::check::checker) fn intern_instantiation(
        &mut self,
        base: TypeId,
        arguments: Vec<(TypeParamId, TypeId)>,
    ) -> TypeId {
        self.interner.intern_instantiation(base, arguments)
    }

    pub(in crate::check::checker) fn intern_mapped(&mut self, mapped: MappedType) -> TypeId {
        self.interner.intern_mapped(mapped)
    }

    pub(in crate::check::checker) fn intern_keyof(&mut self, operand: TypeId) -> TypeId {
        self.interner.intern_keyof(operand)
    }

    pub(in crate::check::checker) fn intern_template(&mut self, template: TemplateType) -> TypeId {
        self.interner.intern_template(template)
    }

    pub(in crate::check::checker) fn intern_class_instance(
        &mut self,
        class: ClassId,
        arguments: Vec<TypeId>,
    ) -> TypeId {
        self.interner.intern_class_instance(class, arguments)
    }

    pub(in crate::check::checker) fn intern_deferred_indexed_access(
        &mut self,
        object: TypeId,
        index: TypeId,
    ) -> TypeId {
        self.interner.intern_deferred_indexed_access(object, index)
    }
}

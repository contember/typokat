//! Class-owned callable content retained from the single signature lowering pass.

use super::super::lexical_events::{CallableSiteId, CallableTickets};
use super::application::ClassTypeParameterDefault;
use crate::types::repr::{GenericTypeParam, ParameterType, TypeParamId};
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum CallableOverloadRole {
    Single,
    Signature {
        ordinal: usize,
    },
    Implementation {
        ordinal: usize,
        hidden_from_public: bool,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct RetainedCallableTypeParameter<Ticket> {
    pub id: TypeParamId,
    pub constraint: Option<TypeId>,
    pub default: ClassTypeParameterDefault<Ticket>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct RetainedParameterProperty<Ticket> {
    pub parameter_index: usize,
    pub property_name: String,
    pub public_type: TypeId,
    pub owner: Ticket,
}

/// Everything body checking and publication need after the public signature has
/// been lowered exactly once. `core` owns the frame/receiver/parameters/return and
/// lexical tickets; this wrapper owns class-specific ordering and property rules.
#[derive(Clone, Debug)]
pub(in crate::check::checker) struct RetainedClassCallable<Ticket> {
    pub site: CallableSiteId,
    pub tickets: CallableTickets,
    pub type_params: Vec<GenericTypeParam>,
    pub type_param_frame: FxHashMap<String, TypeId>,
    pub receiver: Option<TypeId>,
    /// One slot per source parameter (plus rest). Failed annotation slots stay absent.
    pub params: Vec<Option<ParameterType>>,
    pub declared_return: Option<TypeId>,
    /// Absent when any signature child failed; no partial callable is ever interned.
    pub public_type: Option<TypeId>,
    pub type_parameters: Vec<RetainedCallableTypeParameter<Ticket>>,
    pub overload: CallableOverloadRole,
    pub parameter_properties: Vec<RetainedParameterProperty<Ticket>>,
}

impl<Ticket> RetainedClassCallable<Ticket> {
    pub(in crate::check::checker) fn owned_type_parameters(
        &self,
    ) -> impl Iterator<Item = TypeParamId> + '_ {
        self.type_parameters.iter().map(|parameter| parameter.id)
    }

    pub(in crate::check::checker) fn binder_alignment_is_valid(&self) -> bool {
        self.type_params.len() == self.type_parameters.len()
            && self
                .type_params
                .iter()
                .zip(&self.type_parameters)
                .all(|(core, retained)| {
                    let default_aligned = match retained.default {
                        ClassTypeParameterDefault::Absent
                        | ClassTypeParameterDefault::Unsupported(_) => core.default.is_none(),
                        ClassTypeParameterDefault::Ready(default) => core.default == Some(default),
                    };
                    core.id == retained.id
                        && core.constraint == retained.constraint
                        && default_aligned
                })
    }

    pub(in crate::check::checker) fn overload_metadata_is_valid(&self) -> bool {
        match self.overload {
            CallableOverloadRole::Single | CallableOverloadRole::Signature { .. } => true,
            CallableOverloadRole::Implementation {
                hidden_from_public, ..
            } => hidden_from_public,
        }
    }

    pub(in crate::check::checker) fn overload_ordinal(&self) -> Option<usize> {
        match self.overload {
            CallableOverloadRole::Single => None,
            CallableOverloadRole::Signature { ordinal }
            | CallableOverloadRole::Implementation { ordinal, .. } => Some(ordinal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::checker::events::{EventStore, ModuleOrdinal, UnitSlot};
    use crate::check::checker::lexical_events::LexicalReservations;
    use crate::types::repr::GenericTypeParam;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use rustc_hash::FxHashMap;

    #[test]
    fn retained_callable_reuses_the_same_binders_and_public_type() {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, "class C { m<T>() {} }", SourceType::ts()).parse();
        let mut events = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(
                ModuleOrdinal::new(0),
                UnitSlot::new(0),
                &parsed.program,
                &mut events,
            )
            .unwrap();
        let class = reservations.classes()[0].id;
        let member = reservations.class(class).unwrap().members[0];
        let callable = reservations.member(member).unwrap().callable.unwrap();
        let reserved = reservations.callable(callable).unwrap();
        let id = TypeParamId(7);
        let retained = RetainedClassCallable::<u32> {
            site: callable,
            tickets: reserved.tickets,
            type_params: vec![GenericTypeParam {
                id,
                constraint: Some(TypeId(1)),
                default: None,
            }],
            type_param_frame: FxHashMap::from_iter([("T".to_string(), TypeId(2))]),
            receiver: Some(TypeId(3)),
            params: Vec::new(),
            declared_return: Some(TypeId(4)),
            public_type: Some(TypeId(5)),
            type_parameters: vec![RetainedCallableTypeParameter {
                id,
                constraint: Some(TypeId(1)),
                default: ClassTypeParameterDefault::Absent,
            }],
            overload: CallableOverloadRole::Single,
            parameter_properties: Vec::new(),
        };
        assert!(retained.binder_alignment_is_valid());
        assert_eq!(retained.owned_type_parameters().collect::<Vec<_>>(), [id]);
        assert_eq!(retained.public_type, Some(TypeId(5)));
    }
}

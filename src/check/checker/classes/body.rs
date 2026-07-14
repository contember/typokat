//! Non-query class-body member capability.

use crate::types::repr::{ClassId, ObjectType, PropertyType, Visibility};
use crate::types::store::TypeId;
use std::collections::BTreeMap;

#[derive(Copy, Clone)]
pub(in crate::check::checker) struct BodyMemberMetadata {
    pub visibility: Visibility,
    pub declaring_class: Option<ClassId>,
    pub readonly: bool,
    pub is_accessor: bool,
}

#[derive(Copy, Clone)]
enum BodyMemberValue {
    Known {
        read_ty: TypeId,
        write_ty: Option<TypeId>,
    },
    Unavailable,
}

#[derive(Copy, Clone)]
struct BodyMemberSlot {
    value: BodyMemberValue,
    metadata: BodyMemberMetadata,
}

#[derive(Copy, Clone)]
pub(in crate::check::checker) enum BodyMemberLookup {
    Known {
        ty: TypeId,
        write_ty: Option<TypeId>,
        metadata: BodyMemberMetadata,
    },
    Unavailable(BodyMemberMetadata),
    Missing {
        definite: bool,
    },
}

#[derive(Clone, Default)]
pub(in crate::check::checker) struct BodyMemberEnvironment {
    members: BTreeMap<String, BodyMemberSlot>,
    missing_is_definite: bool,
}

impl BodyMemberEnvironment {
    fn from_object(object: &ObjectType, missing_is_definite: bool) -> Self {
        let members = object
            .properties
            .iter()
            .map(|property| {
                (
                    property.name.clone(),
                    BodyMemberSlot {
                        value: BodyMemberValue::Known {
                            read_ty: property.ty,
                            write_ty: property.write_ty,
                        },
                        metadata: BodyMemberMetadata::from_property(property),
                    },
                )
            })
            .collect();
        Self {
            members,
            missing_is_definite,
        }
    }

    pub fn retain_declaration(
        &mut self,
        name: String,
        metadata: BodyMemberMetadata,
        known_is_body_safe: bool,
    ) {
        match self.members.entry(name) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(BodyMemberSlot {
                    value: BodyMemberValue::Unavailable,
                    metadata,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let slot = entry.get_mut();
                if !known_is_body_safe {
                    slot.value = BodyMemberValue::Unavailable;
                }
                slot.metadata.readonly &= metadata.readonly;
                slot.metadata.is_accessor |= metadata.is_accessor;
            }
        }
    }

    pub fn lookup(&self, name: &str) -> BodyMemberLookup {
        match self.members.get(name).copied() {
            Some(BodyMemberSlot {
                value: BodyMemberValue::Known { read_ty, write_ty },
                metadata,
            }) => BodyMemberLookup::Known {
                ty: read_ty,
                write_ty,
                metadata,
            },
            Some(BodyMemberSlot {
                value: BodyMemberValue::Unavailable,
                metadata,
            }) => BodyMemberLookup::Unavailable(metadata),
            None => BodyMemberLookup::Missing {
                definite: self.missing_is_definite,
            },
        }
    }
}

impl BodyMemberMetadata {
    pub fn new(
        visibility: Visibility,
        declaring_class: Option<ClassId>,
        readonly: bool,
        is_accessor: bool,
    ) -> Self {
        Self {
            visibility,
            declaring_class,
            readonly,
            is_accessor,
        }
    }

    fn from_property(property: &PropertyType) -> Self {
        Self::new(
            property.visibility,
            property.declaring_class,
            property.readonly,
            property.is_accessor,
        )
    }
}

#[derive(Clone)]
pub(in crate::check::checker) struct BodyClassView {
    pub instance: BodyMemberEnvironment,
    pub static_side: BodyMemberEnvironment,
}

impl BodyClassView {
    pub fn from_objects(
        instance: &ObjectType,
        static_side: &ObjectType,
        missing_is_definite: bool,
    ) -> Self {
        Self {
            instance: BodyMemberEnvironment::from_object(instance, missing_is_definite),
            static_side: BodyMemberEnvironment::from_object(static_side, missing_is_definite),
        }
    }
}

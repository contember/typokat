use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralModeError {
    NotWidenableLiteral(TypeId),
    UnavailableEnum(TypeId),
}

impl Interner {
    pub fn intern_ordinary_literal(&mut self, value: LiteralValue) -> TypeId {
        let widenable = !matches!(value, LiteralValue::WellKnownSymbol(_));
        let regular = self.intern_literal(value);
        if widenable {
            self.intern_literal_wrapper(regular)
        } else {
            regular
        }
    }

    pub fn regularize_literal_type(&mut self, ty: TypeId) -> TypeId {
        if let Some(members) = self.store.union_members(ty) {
            let members = members.to_vec();
            let regular = members
                .into_iter()
                .map(|member| self.regularize_literal_type(member))
                .collect();
            return self.union(regular);
        }
        self.store.regular_literal_type(ty)
    }

    pub fn intern_widening_literal(&mut self, ty: TypeId) -> Result<TypeId, LiteralModeError> {
        let regular = self.store.regular_literal_type(ty);
        match self.store.tag(regular) {
            TypeTag::Literal => {
                if !matches!(
                    self.store.literal_value(regular),
                    Some(
                        LiteralValue::Number(_)
                            | LiteralValue::String(_)
                            | LiteralValue::Boolean(_)
                    )
                ) {
                    return Err(LiteralModeError::NotWidenableLiteral(regular));
                }
            }
            TypeTag::EnumMember => {
                let Some(whole) = self.store.enum_member_whole_type_id(regular) else {
                    return Err(LiteralModeError::UnavailableEnum(regular));
                };
                if self.reserved_enum_semantic_frontier(regular)
                    || self.reserved_enum_semantic_frontier(whole)
                {
                    return Err(LiteralModeError::UnavailableEnum(regular));
                }
            }
            _ => return Err(LiteralModeError::NotWidenableLiteral(regular)),
        }
        Ok(self.intern_literal_wrapper(regular))
    }

    fn intern_literal_wrapper(&mut self, regular: TypeId) -> TypeId {
        let hash = structural_hash(&StructuralKey::WideningLiteral(regular));
        if let Some(existing) = self.lookup(hash, |store, id| {
            store.widening_literal_operand(id) == Some(regular)
        }) {
            return existing;
        }
        let id = self.store.push_widening_literal(regular);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    pub fn widen_literal_type(&mut self, ty: TypeId) -> TypeId {
        if let Some(members) = self.store.union_members(ty) {
            let members = members.to_vec();
            let widened = members
                .into_iter()
                .map(|member| self.widen_literal_type(member))
                .collect();
            return self.union(widened);
        }
        let Some(regular) = self.store.widening_literal_operand(ty) else {
            return ty;
        };
        if let Some(value) = self.store.literal_value(regular) {
            return self.intern_intrinsic(value.base_kind());
        }
        self.store
            .enum_member_whole_type_id(regular)
            .expect("widening enum mode retains its immutable canonical family")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{EnumMemberFact, EnumValue, FunctionType, PropertyType};
    use crate::types::substitute;

    #[test]
    fn m1_modes_survive_intersections_frozen_forks_and_reference_census(
    ) -> Result<(), LiteralModeError> {
        let mut base = Interner::with_intrinsics();
        let regular = base.intern_literal(LiteralValue::Number(7.0));
        let widening = base.intern_widening_literal(regular)?;
        let string = base.well_known().string;
        let number = base.well_known().number;
        let branded = base.intersection(vec![number, widening]);
        assert_ne!(branded, base.well_known().never);
        assert_eq!(
            base.intersection(vec![string, widening]),
            base.well_known().never
        );
        let (references, _) = base.reference_records_for_test();
        assert!(references.contains(&(1, 1, 0, widening.0, regular.0)));
        base.freeze_as_base()
            .expect("complete literal base freezes");
        let mut fork = base.fork_delta().expect("literal base forks");
        assert_eq!(fork.intern_widening_literal(regular)?, widening);
        assert_eq!(
            fork.intern_ordinary_literal(LiteralValue::Number(7.0)),
            widening
        );
        assert_eq!(fork.intersection(vec![widening, number]), branded);
        assert_eq!(fork.widen_literal_type(widening), number);
        assert_eq!(fork.store().regular_literal_type(widening), regular);
        assert!(base.store().shares_base_rows_with(fork.store()));
        Ok(())
    }

    #[test]
    fn m1_enum_mode_collisions_keep_canonical_families_and_open_numeric_values(
    ) -> Result<(), LiteralModeError> {
        let mut interner = Interner::with_intrinsics();
        let mut owners = Vec::new();
        for name in ["First", "Second"] {
            let id = interner.next_enum_id();
            let whole = interner.intern_enum(EnumType {
                id,
                name: name.to_owned(),
                members: vec![
                    EnumMemberFact {
                        id: EnumMemberId(0),
                        name: "A".to_owned(),
                        value: EnumValue::String("same".to_owned()),
                    },
                    EnumMemberFact {
                        id: EnumMemberId(1),
                        name: "B".to_owned(),
                        value: EnumValue::String("same".to_owned()),
                    },
                    EnumMemberFact {
                        id: EnumMemberId(2),
                        name: "Computed".to_owned(),
                        value: EnumValue::OpaqueNumber,
                    },
                ],
                numeric_open: true,
            });
            let mut modes = Vec::new();
            for member_id in [EnumMemberId(0), EnumMemberId(1), EnumMemberId(2)] {
                let regular = interner
                    .store()
                    .enum_member_type_id(id, member_id)
                    .expect("published member");
                let widening = interner.intern_widening_literal(regular)?;
                assert_eq!(
                    interner.store().enum_member_type_id(id, member_id),
                    Some(regular)
                );
                assert_eq!(interner.widen_literal_type(widening), whole);
                assert_eq!(interner.widen_literal_type(regular), regular);
                modes.push(widening);
            }
            assert_ne!(modes[0], modes[1]);
            let union = interner.union(modes.clone());
            assert_eq!(
                interner.store().union_members(union).map(<[TypeId]>::len),
                Some(3)
            );
            owners.push(modes);
        }
        assert_ne!(owners[0], owners[1]);
        Ok(())
    }

    #[test]
    fn f1_literal_modes_are_stable_distinct_and_deduplicated() -> Result<(), LiteralModeError> {
        let mut interner = Interner::with_intrinsics();
        for value in [
            LiteralValue::Number(1.0),
            LiteralValue::String("one".to_owned()),
            LiteralValue::Boolean(true),
        ] {
            let regular = interner.intern_literal(value.clone());
            let widening = interner.intern_widening_literal(regular)?;
            assert_ne!(regular, widening);
            assert_eq!(interner.intern_literal(value.clone()), regular);
            assert_eq!(interner.intern_widening_literal(regular)?, widening);
            assert_eq!(interner.intern_widening_literal(widening)?, widening);
            assert_eq!(interner.store().literal_value(widening), Some(&value));
            assert_eq!(interner.store().regular_literal_type(widening), regular);
            assert_eq!(interner.widen_literal_type(regular), regular);
            let widened = interner.widen_literal_type(widening);
            assert_eq!(
                interner.store().intrinsic_kind(widened),
                Some(value.base_kind())
            );
        }
        Ok(())
    }

    #[test]
    fn f1_literal_union_algebra_preserves_per_value_modes() -> Result<(), LiteralModeError> {
        let mut interner = Interner::with_intrinsics();
        let one = interner.intern_literal(LiteralValue::Number(1.0));
        let two = interner.intern_literal(LiteralValue::Number(2.0));
        let fresh_one = interner.intern_widening_literal(one)?;
        let fresh_two = interner.intern_widening_literal(two)?;
        assert_eq!(interner.union(vec![one, fresh_one]), one);
        assert_eq!(interner.union(vec![fresh_one, one]), one);
        assert_eq!(interner.union(vec![fresh_one, fresh_one]), fresh_one);
        let mixed = interner.union(vec![one, fresh_two]);
        assert_eq!(interner.union(vec![fresh_two, one]), mixed);
        assert_eq!(
            interner.store().union_members(mixed),
            Some([one, fresh_two].as_slice())
        );
        let widened = interner.widen_literal_type(mixed);
        let expected = interner.union(vec![one, interner.well_known().number]);
        assert_eq!(widened, expected);
        let nested = interner.union(vec![fresh_one, fresh_two]);
        assert_eq!(interner.union(vec![nested, one]), mixed);
        assert_eq!(interner.union(vec![one, nested]), mixed);
        Ok(())
    }

    #[test]
    fn f1_enum_modes_keep_one_owner_and_reject_unavailable_members() -> Result<(), LiteralModeError>
    {
        let mut interner = Interner::with_intrinsics();
        let enum_id = interner.next_enum_id();
        let whole = interner.reserve_enum(enum_id, "Mode");
        let ready = interner.reserve_enum_member(enum_id, EnumMemberId(0), "Mode", "Ready");
        let invalid = interner.reserve_enum_member(enum_id, EnumMemberId(1), "Mode", "Invalid");
        assert_eq!(
            interner.intern_widening_literal(ready),
            Err(LiteralModeError::UnavailableEnum(ready))
        );
        interner
            .fill_enum(
                whole,
                EnumType {
                    id: enum_id,
                    name: "Mode".to_owned(),
                    members: vec![EnumMemberFact {
                        id: EnumMemberId(0),
                        name: "Ready".to_owned(),
                        value: EnumValue::String("ready".to_owned()),
                    }],
                    numeric_open: false,
                },
            )
            .expect("enum publication");
        let widening = interner.intern_widening_literal(ready)?;
        assert_ne!(ready, widening);
        assert_eq!(interner.intern_widening_literal(ready)?, widening);
        assert_eq!(
            interner
                .store()
                .enum_member_type_id(enum_id, EnumMemberId(0)),
            Some(ready)
        );
        assert_eq!(
            interner.store().enum_member_whole_type_id(widening),
            Some(whole)
        );
        assert_eq!(interner.union(vec![ready, widening]), ready);
        assert_eq!(interner.union(vec![widening, ready]), ready);
        assert_eq!(interner.widen_literal_type(widening), whole);
        assert!(interner.reserved_enum_semantic_frontier(invalid));
        assert_eq!(
            interner.intern_widening_literal(invalid),
            Err(LiteralModeError::UnavailableEnum(invalid))
        );
        let object = interner.intern_object(ObjectType::default());
        assert_eq!(
            interner.intern_widening_literal(object),
            Err(LiteralModeError::NotWidenableLiteral(object))
        );
        Ok(())
    }

    #[test]
    fn f1_substitution_preserves_member_tuple_and_return_modes() -> Result<(), LiteralModeError> {
        let mut interner = Interner::with_intrinsics();
        let parameter = TypeParamId(90_001);
        let parameter_ty = interner.intern_type_param(parameter, "T");
        let tuple = interner.intern_tuple(vec![parameter_ty]);
        let function = interner.intern_function(FunctionType {
            type_params: vec![],
            receiver: None,
            params: vec![],
            ret: parameter_ty,
        });
        let template = interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public("member", parameter_ty),
                PropertyType::public("tuple", tuple),
                PropertyType::public("function", function),
            ],
            ..Default::default()
        });
        let regular = interner.intern_literal(LiteralValue::Number(1.0));
        let widening = interner.intern_widening_literal(regular)?;
        let mut projections = Vec::new();
        for argument in [regular, widening] {
            let map = FxHashMap::from_iter([(parameter, argument)]);
            let result = substitute(&mut interner, template, &map);
            let object = interner
                .store()
                .object_type(result)
                .expect("object substitution");
            assert_eq!(
                object.property("member").map(|property| property.ty),
                Some(argument)
            );
            let tuple = object.property("tuple").expect("tuple property").ty;
            let function = object.property("function").expect("function property").ty;
            assert_eq!(
                interner.store().tuple_type(tuple).expect("tuple").elements,
                [argument]
            );
            assert_eq!(
                interner
                    .store()
                    .function_type(function)
                    .expect("function")
                    .ret,
                argument
            );
            assert_eq!(substitute(&mut interner, argument, &map), argument);
            projections.push(result);
        }
        assert_ne!(projections[0], projections[1]);
        Ok(())
    }
}

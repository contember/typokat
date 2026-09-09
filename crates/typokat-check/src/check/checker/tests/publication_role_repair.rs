use super::*;
use crate::check::checker::literal_publication::finalize_literal_type;
use crate::types::repr::{EnumMemberFact, EnumMemberId, EnumNumber, EnumType, EnumValue};
use crate::types::Interner;
use oxc_ast::ast::{ClassElement, Statement};

struct ReadyContext(TypeId);

impl SurfaceInitializerContext for ReadyContext {
    fn lower_annotation(&mut self, _: &TSType<'_>) -> DemandOutcome<TypeId> {
        DemandOutcome::Ready(self.0)
    }

    fn earlier_field(&self, _: &str) -> Option<TypeId> {
        None
    }

    fn earlier_method(&self, _: &str) -> Option<EarlierMethodSurface> {
        None
    }

    fn lexical_value(&self, _: u32, _: &str) -> Option<TypeId> {
        None
    }
}

#[test]
fn enum_publication_matches_kinds_without_replacing_canonical_source_ids() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    for value in [
        EnumValue::Number(EnumNumber::new(1.0).unwrap()),
        EnumValue::String("a".into()),
    ] {
        let kind = value.base_kind();
        let broad = interner.intern_intrinsic(kind);
        let other_broad = if broad == wk.number {
            wk.string
        } else {
            wk.number
        };
        let mut families = Vec::new();
        for name in ["E", "Other"] {
            let id = interner.next_enum_id();
            let whole = interner.intern_enum(EnumType {
                id,
                name: name.into(),
                members: vec![
                    EnumMemberFact {
                        id: EnumMemberId(0),
                        name: "A".into(),
                        value: value.clone(),
                    },
                    EnumMemberFact {
                        id: EnumMemberId(1),
                        name: "B".into(),
                        value: value.clone(),
                    },
                ],
                numeric_open: false,
            });
            let member = interner
                .store()
                .enum_member_type_id(id, EnumMemberId(0))
                .unwrap();
            let sibling = interner
                .store()
                .enum_member_type_id(id, EnumMemberId(1))
                .unwrap();
            let ordinary = interner.intern_widening_literal(member).unwrap();
            families.push((id, whole, member, sibling, ordinary));
        }
        let (id, whole, member, sibling, ordinary) = families[0];
        let (_, other_whole, other_member, _, _) = families[1];
        let primitive = interner.intern_ordinary_literal(if broad == wk.number {
            LiteralValue::Number(1.0)
        } else {
            LiteralValue::String("a".into())
        });
        let exact_primitive = interner.regularize_literal_type(primitive);
        let union_context = interner.union(vec![member, wk.undefined]);
        for target in [
            member,
            sibling,
            whole,
            other_member,
            other_whole,
            exact_primitive,
            union_context,
        ] {
            for role in [
                LiteralPublicationRole::MutableMember,
                LiteralPublicationRole::InferredReturn,
            ] {
                assert_eq!(
                    finalize_literal_type(&mut interner, ordinary, Some(target), role),
                    member
                );
                assert_eq!(
                    finalize_literal_type(&mut interner, primitive, Some(target), role),
                    exact_primitive
                );
            }
        }
        for arms in [
            vec![member, broad],
            vec![broad, member],
            vec![whole, broad],
            vec![broad, whole],
        ] {
            let context = interner.union(arms);
            for role in [
                LiteralPublicationRole::MutableMember,
                LiteralPublicationRole::InferredReturn,
            ] {
                assert_eq!(
                    finalize_literal_type(&mut interner, ordinary, Some(context), role),
                    whole
                );
                assert_eq!(
                    finalize_literal_type(&mut interner, member, Some(context), role),
                    member
                );
            }
        }
        for arms in [vec![whole, other_broad], vec![other_broad, whole]] {
            let context = interner.union(arms);
            assert_eq!(
                finalize_literal_type(
                    &mut interner,
                    ordinary,
                    Some(context),
                    LiteralPublicationRole::MutableMember
                ),
                member
            );
        }
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                ordinary,
                None,
                LiteralPublicationRole::MutableMember
            ),
            whole
        );
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                ordinary,
                Some(member),
                LiteralPublicationRole::ModeRetaining
            ),
            ordinary
        );
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                ordinary,
                Some(broad),
                LiteralPublicationRole::ConstOwned
            ),
            member
        );
        assert_eq!(interner.store().enum_whole_type_id(id), Some(whole));
        assert_eq!(
            interner.store().enum_member_type_id(id, EnumMemberId(0)),
            Some(member)
        );
        assert_eq!(interner.intern_widening_literal(member).unwrap(), ordinary);
        assert_eq!(
            interner.store().widening_literal_operand(ordinary),
            Some(member)
        );
        assert_ne!(member, sibling);
        assert_ne!(member, other_member);
        let array = interner.intern_array(ordinary);
        let grouped = interner.union(vec![ordinary, wk.undefined]);
        let regular_grouped = interner.regularize_literal_type(grouped);
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                grouped,
                Some(member),
                LiteralPublicationRole::MutableMember
            ),
            regular_grouped
        );
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                grouped,
                Some(member),
                LiteralPublicationRole::InferredReturn
            ),
            grouped
        );
        for role in [
            LiteralPublicationRole::MutableMember,
            LiteralPublicationRole::InferredReturn,
            LiteralPublicationRole::ModeRetaining,
            LiteralPublicationRole::ConstOwned,
        ] {
            assert_eq!(
                finalize_literal_type(&mut interner, array, Some(member), role),
                array
            );
            assert_eq!(
                interner.store().array_type(array).unwrap().element,
                ordinary
            );
        }
    }
}

#[test]
fn scalar_satisfies_applies_field_role_once_and_preserves_child_roles() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let ordinary = interner.intern_ordinary_literal(LiteralValue::String("a".into()));
    let exact = interner.regularize_literal_type(ordinary);
    let mixed = interner.union(vec![exact, wk.number]);
    let object = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", mixed)],
        ..Default::default()
    });
    let exact_object = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", exact)],
        ..Default::default()
    });
    let array = interner.intern_array(mixed);
    let exact_array = interner.intern_array(exact);
    for (initializer, target, mutable, readonly) in [
        (r#""a" satisfies "a" | number"#, mixed, wk.string, ordinary),
        (
            r#"(("a" satisfies "a" | number))"#,
            mixed,
            wk.string,
            ordinary,
        ),
        (
            r#"("a" as const) satisfies "a" | number"#,
            mixed,
            exact,
            exact,
        ),
        (r#"("a" as "a") satisfies "a""#, exact, exact, exact),
        (
            r#"{ value: "a" } satisfies { value: "a" | number }"#,
            object,
            exact_object,
            exact_object,
        ),
        (
            r#"["a"] satisfies ("a" | number)[]"#,
            array,
            exact_array,
            exact_array,
        ),
    ] {
        let source = format!("class C {{ field = {initializer}; }}");
        let run =
            crate::frontend::run_source_frontend(&source, |program| {
                let Statement::ClassDeclaration(class) = &program.body[0] else {
                    panic!("class source")
                };
                let ClassElement::PropertyDefinition(property) = &class.body.body[0] else {
                    panic!("field source")
                };
                let mut factory = SurfaceTypeFactory::new(&mut interner);
                for (readonly_field, expected) in [(false, mutable), (true, readonly)] {
                    let actual =
                        SurfaceInitializerInferer::new(&mut factory, &mut ReadyContext(target))
                            .infer(property.value.as_ref().unwrap(), readonly_field, false);
                    assert_eq!(
                        actual,
                        InitializerInference::Inferred(expected),
                        "{source}, readonly={readonly_field}"
                    );
                }
            });
        assert!(run.parse_errors.is_empty(), "{:?}", run.parse_errors);
        assert!(run.product.is_some());
    }
}

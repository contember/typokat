use crate::check::checker::classes::surface_types::SurfaceTypeFactory;
use crate::check::checker::literal_publication::{finalize_literal_type, LiteralPublicationRole};
use crate::types::repr::{LiteralValue, ObjectType, PropertyType, TypeParamId};
use crate::types::Interner;

#[test]
fn publication_class_producers_retain_ids_and_unavailable_admission() {
    use crate::check::checker::classes::initializer::{
        EarlierMethodSurface, InitializerInference, SurfaceInitializerContext,
        SurfaceInitializerInferer,
    };
    use crate::class_semantics::{DemandOutcome, Exhaustion};
    use crate::types::store::TypeId;
    use oxc_ast::ast::{ClassElement, Statement, TSType};
    struct UnavailableContext;
    impl SurfaceInitializerContext for UnavailableContext {
        fn lower_annotation(&mut self, _: &TSType<'_>) -> DemandOutcome<TypeId> {
            DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
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
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let ordinary = interner.intern_ordinary_literal(LiteralValue::String("a".into()));
    let exact = interner.regularize_literal_type(ordinary);
    let run = crate::frontend::run_source_frontend(
        r#"class Box { readonly ordinary = "a"; readonly exact = "a" as const; mutable = "a"; unavailable = "a" satisfies string; unsupported = true ? 1 : 2; }"#,
        |program| {
            let Statement::ClassDeclaration(class) = &program.body[0] else {
                panic!("class source")
            };
            let expected = [
                InitializerInference::Inferred(ordinary),
                InitializerInference::Inferred(exact),
                InitializerInference::Inferred(wk.string),
                InitializerInference::Unsupported,
                InitializerInference::Unsupported,
            ];
            let mut factory = SurfaceTypeFactory::new(&mut interner);
            for (member, expected) in class.body.body.iter().zip(expected) {
                let ClassElement::PropertyDefinition(property) = member else {
                    panic!("field source")
                };
                let actual = SurfaceInitializerInferer::new(&mut factory, &mut UnavailableContext)
                    .infer(property.value.as_ref().unwrap(), property.readonly, false);
                assert_eq!(actual, expected);
                assert_ne!(actual, InitializerInference::Inferred(wk.error));
            }
        },
    );
    assert!(run.parse_errors.is_empty(), "{:?}", run.parse_errors);
    assert!(run.product.is_some());
}

#[test]
fn publication_roles_preserve_complete_grouping_and_modes() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let yes = interner.intern_ordinary_literal(LiteralValue::Boolean(true));
    let one = interner.intern_ordinary_literal(LiteralValue::Number(1.0));
    let regular_yes = interner.regularize_literal_type(yes);
    let source = interner.union(vec![yes, one]);
    let regular_source = interner.regularize_literal_type(source);
    let mixed = interner.union(vec![regular_yes, one]);
    for context in [vec![wk.boolean, wk.number], vec![wk.number, wk.boolean]] {
        let context = interner.union(context);
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                source,
                Some(context),
                LiteralPublicationRole::MutableMember
            ),
            regular_source
        );
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                source,
                Some(context),
                LiteralPublicationRole::InferredReturn
            ),
            source
        );
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                mixed,
                Some(context),
                LiteralPublicationRole::InferredReturn
            ),
            mixed
        );
        let separate_yes = finalize_literal_type(
            &mut interner,
            yes,
            Some(context),
            LiteralPublicationRole::MutableMember,
        );
        let separate_one = finalize_literal_type(
            &mut interner,
            one,
            Some(context),
            LiteralPublicationRole::MutableMember,
        );
        let separate = interner.union(vec![separate_yes, separate_one]);
        let expected = interner.union(vec![regular_yes, wk.number]);
        assert_eq!(separate, expected);
        assert_ne!(separate, regular_source);
        let collapsed = interner.union(vec![yes, yes]);
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                collapsed,
                Some(context),
                LiteralPublicationRole::InferredReturn
            ),
            regular_yes
        );
    }
    assert_eq!(
        finalize_literal_type(
            &mut interner,
            one,
            None,
            LiteralPublicationRole::InferredReturn
        ),
        wk.number
    );
    assert_eq!(
        interner.widen_literal_type(source),
        interner.union(vec![wk.boolean, wk.number])
    );
    assert_eq!(
        interner.widen_literal_type(mixed),
        interner.union(vec![regular_yes, wk.number])
    );
}

#[test]
fn publication_matching_is_whole_source_kind_not_assignment_or_union_order() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let a = interner.intern_ordinary_literal(LiteralValue::String("a".into()));
    let b = interner.intern_literal(LiteralValue::String("b".into()));
    let exact_a = interner.regularize_literal_type(a);
    for arms in [vec![exact_a, wk.string], vec![wk.string, exact_a]] {
        let context = interner.union(arms);
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                a,
                Some(context),
                LiteralPublicationRole::MutableMember
            ),
            wk.string
        );
        assert_eq!(
            finalize_literal_type(
                &mut interner,
                exact_a,
                Some(context),
                LiteralPublicationRole::MutableMember
            ),
            exact_a
        );
    }
    assert_eq!(
        finalize_literal_type(
            &mut interner,
            a,
            Some(b),
            LiteralPublicationRole::MutableMember
        ),
        exact_a
    );
    let yes = interner.intern_ordinary_literal(LiteralValue::Boolean(true));
    let source = interner.union(vec![yes, wk.undefined]);
    let context = interner.union(vec![wk.undefined, wk.boolean]);
    let regular = interner.regularize_literal_type(source);
    assert_eq!(
        finalize_literal_type(
            &mut interner,
            source,
            Some(context),
            LiteralPublicationRole::MutableMember
        ),
        regular
    );
}

#[test]
fn publication_roles_and_class_forwarder_are_shallow_and_non_poison() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let ordinary = interner.intern_ordinary_literal(LiteralValue::String("a".into()));
    let exact = interner.regularize_literal_type(ordinary);
    let object = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("a", ordinary)],
        ..Default::default()
    });
    let array = interner.intern_array(ordinary);
    let tuple = interner.intern_tuple(vec![ordinary]);
    let readonly = interner.intern_readonly(tuple);
    let symbolic = interner.intern_type_param(TypeParamId(0), "T");
    assert!(interner.set_type_param_constraint(TypeParamId(0), wk.string));
    for role in [
        LiteralPublicationRole::MutableMember,
        LiteralPublicationRole::InferredReturn,
        LiteralPublicationRole::ModeRetaining,
        LiteralPublicationRole::ConstOwned,
    ] {
        for source in [object, array, tuple, readonly, symbolic] {
            let result = finalize_literal_type(&mut interner, source, Some(wk.string), role);
            assert_eq!(result, source);
            assert_ne!(result, wk.error);
            let result = SurfaceTypeFactory::new(&mut interner).finalize_literal_type(
                source,
                Some(wk.string),
                role,
            );
            assert_eq!(result, source);
        }
    }
    assert_eq!(
        interner
            .store()
            .object_type(object)
            .unwrap()
            .property("a")
            .unwrap()
            .ty,
        ordinary
    );
    assert_eq!(
        finalize_literal_type(
            &mut interner,
            ordinary,
            Some(symbolic),
            LiteralPublicationRole::MutableMember
        ),
        wk.string
    );
    let mut factory = SurfaceTypeFactory::new(&mut interner);
    assert_eq!(
        factory.finalize_literal_type(ordinary, None, LiteralPublicationRole::ModeRetaining),
        ordinary
    );
    assert_eq!(
        factory.finalize_literal_type(exact, None, LiteralPublicationRole::ModeRetaining),
        exact
    );
    assert_eq!(
        factory.finalize_literal_type(ordinary, None, LiteralPublicationRole::ConstOwned),
        exact
    );
    assert_eq!(
        factory.finalize_literal_type(ordinary, None, LiteralPublicationRole::MutableMember),
        wk.string
    );
}

#[test]
fn publication_role_contrasts_reject_downstream_writes() {
    let source = r#"
declare const flag: boolean;
const grouped = [flag ? true : 1] satisfies (boolean | number)[];
const separate = [true, 1] satisfies (boolean | number)[];
const returned = (() => { if (flag) return true; return 1; }) satisfies () => boolean | number;
let groupedCopy = grouped[0];
groupedCopy = 2;
groupedCopy = false;
let separateCopy = separate[0];
separateCopy = 2;
separateCopy = false;
const exact: true | 1 = returned();
let returnCopy = returned();
returnCopy = 2;
returnCopy = false;
const mismatch = { a: "a" } satisfies { a: "b" };
"#;
    let output = crate::check::test_support::check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str(), &source[diagnostic.span.range()]))
            .collect::<Vec<_>>(),
        [
            ("TK2322", "2"),
            ("TK2322", "false"),
            ("TK2322", "false"),
            ("TK2322", "a")
        ]
    );
}

use super::*;
use crate::class_semantics::{ClassConstructionState, PublishedClassPoison, PublishedClassSurface};
use crate::types::repr::{
    ClassId, ConditionalType, DeclaredRecipeNode, FunctionType, GenericTypeParam, LiteralValue,
    MappedType, ModifierOp, ObjectType, ParameterType, PropertyType, TemplateType, TupleRestType,
    TupleType, TypeParamId, Visibility,
};

fn published(
    class: ClassId,
    params: Vec<TypeParamId>,
    instance_template: TypeId,
    static_template: TypeId,
) -> PublishedClasses {
    PublishedClasses::from_publication(
        FxHashMap::from_iter([(class, ClassConstructionState::Published)]),
        FxHashMap::from_iter([(
            class,
            PublishedClassSurface::new(class, params, instance_template, static_template, None),
        )]),
        FxHashMap::default(),
    )
    .expect("test publication is complete")
}

fn library_object_context_for_test(
    interner: &mut Interner,
    object: TypeId,
) -> LibraryObjectRelationContext {
    let wk = interner.well_known();
    let parameter = TypeParamId(99_001);
    let parameter_ty = interner.intern_type_param(parameter, "T");
    let array = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("element", parameter_ty),
            PropertyType::public("length", wk.number),
        ],
        ..Default::default()
    });
    let readonly_array = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("element", parameter_ty),
            PropertyType::public("length", wk.number),
        ],
        ..Default::default()
    });
    let string = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("length", wk.number)],
        ..Default::default()
    });
    let empty = interner.intern_object(ObjectType::default());
    let callable_function = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("length", wk.number),
            PropertyType::public("name", wk.string),
        ],
        ..Default::default()
    });
    LibraryObjectRelationContext::new(
        object,
        (array, parameter),
        (readonly_array, parameter),
        string,
        empty,
        empty,
        callable_function,
    )
}

fn library_object_context_with_array_templates_for_test(
    interner: &mut Interner,
    object: TypeId,
    array: TypeId,
    readonly_array: TypeId,
    parameter: TypeParamId,
) -> LibraryObjectRelationContext {
    let wk = interner.well_known();
    let string = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("length", wk.number)],
        ..Default::default()
    });
    let empty = interner.intern_object(ObjectType::default());
    let callable_function = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("length", wk.number),
            PropertyType::public("name", wk.string),
        ],
        ..Default::default()
    });
    LibraryObjectRelationContext::new(
        object,
        (array, parameter),
        (readonly_array, parameter),
        string,
        empty,
        empty,
        callable_function,
    )
}

fn optional_property(name: &str, ty: TypeId) -> PropertyType {
    PropertyType {
        optional: true,
        ..PropertyType::public(name, ty)
    }
}

fn object_with_irrelevant_recursive_computations(
    interner: &mut Interner,
) -> (TypeId, TypeId, TypeId, TypeId) {
    let wk = interner.well_known();
    let root = interner.reserve_object();
    let mut irrelevant = root;
    for _ in 0..=DEFAULT_STEP_BUDGET {
        irrelevant = interner.intern_conditional(ConditionalType {
            check: wk.number,
            extends_ty: wk.number,
            true_branch: irrelevant,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
    }
    interner.fill_object(
        root,
        ObjectType {
            properties: vec![
                PropertyType::public("selected", wk.number),
                PropertyType::public("irrelevant", irrelevant),
            ],
            ..Default::default()
        },
    );
    let selected = interner.intern_literal(LiteralValue::String("selected".into()));
    let irrelevant_key = interner.intern_literal(LiteralValue::String("irrelevant".into()));
    let missing = interner.intern_literal(LiteralValue::String("missing".into()));
    (root, selected, irrelevant_key, missing)
}

#[test]
fn one_layer_projection_is_argument_sensitive_and_memoized() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_001);
    let param = TypeParamId(80_001);
    let param_ty = interner.intern_type_param(param, "T");
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", param_ty)],
        ..Default::default()
    });
    let published = published(class, vec![param], template, wk.error);
    let number_app = interner.intern_class_instance(class, vec![wk.number]);
    let string_app = interner.intern_class_instance(class, vec![wk.string]);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 90_000;

    let number_projection = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        match coordinator.demand(number_app) {
            DemandOutcome::Ready(projection) => projection,
            DemandOutcome::Exhausted(reason) => panic!("unexpected exhaustion: {reason:?}"),
        }
    };
    assert_eq!(
        interner
            .store()
            .object_type(number_projection)
            .and_then(|object| object.property("value"))
            .map(|property| property.ty),
        Some(wk.number)
    );
    assert_eq!(state.projection_memo.len(), 1);

    let same_projection = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        match coordinator.demand(number_app) {
            DemandOutcome::Ready(projection) => projection,
            DemandOutcome::Exhausted(reason) => panic!("unexpected exhaustion: {reason:?}"),
        }
    };
    assert_eq!(same_projection, number_projection);
    assert_eq!(state.projection_memo.len(), 1);

    let string_projection = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        match coordinator.demand(string_app) {
            DemandOutcome::Ready(projection) => projection,
            DemandOutcome::Exhausted(reason) => panic!("unexpected exhaustion: {reason:?}"),
        }
    };
    assert_ne!(string_projection, number_projection);
    assert_eq!(state.projection_memo.len(), 2);

    let empty_target = interner.intern_object(ObjectType::default());
    let relation = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.is_assignable(number_app, empty_target)
    };
    assert!(matches!(relation, RelationOutcome::Yes));
    assert!(!state.relation_cache.is_empty());
}

#[test]
fn regular_application_cycle_costs_one_budget_unit() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_002);
    let param = TypeParamId(80_002);
    let param_ty = interner.intern_type_param(param, "T");
    let recursive = interner.intern_class_instance(class, vec![param_ty]);
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("next", recursive)],
        ..Default::default()
    });
    let published = published(class, vec![param], template, wk.error);
    let application = interner.intern_class_instance(class, vec![wk.string]);
    let projection_memo = FxHashMap::default();
    let evaluator_memo = FxHashMap::default();
    let transaction = ProjectionPlanner::new(
        &mut interner,
        &published,
        &projection_memo,
        &evaluator_memo,
        0,
        false,
        None,
    )
    .plan(&[application]);

    assert!(!transaction.planning_tainted);
    assert_eq!(transaction.pending_projection_writes.len(), 1);
    assert!(transaction.plan.frontier.is_empty());
}

#[test]
fn non_regular_application_exhausts_exactly_at_129_and_writes_nothing() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_003);
    let param = TypeParamId(80_003);
    let param_ty = interner.intern_type_param(param, "T");
    let nested_arg = interner.intern_array(param_ty);
    let recursive = interner.intern_class_instance(class, vec![nested_arg]);
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("next", recursive)],
        ..Default::default()
    });
    let target_class = ClassId(80_103);
    let target_param = TypeParamId(80_103);
    let target_param_ty = interner.intern_type_param(target_param, "U");
    let target_nested_arg = interner.intern_array(target_param_ty);
    let target_recursive = interner.intern_class_instance(target_class, vec![target_nested_arg]);
    let target_template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("next", target_recursive)],
        ..Default::default()
    });
    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([
            (class, ClassConstructionState::Published),
            (target_class, ClassConstructionState::Published),
        ]),
        FxHashMap::from_iter([
            (
                class,
                PublishedClassSurface::new(class, vec![param], template, wk.error, None),
            ),
            (
                target_class,
                PublishedClassSurface::new(
                    target_class,
                    vec![target_param],
                    target_template,
                    wk.error,
                    None,
                ),
            ),
        ]),
        FxHashMap::default(),
    )
    .expect("test publication is complete");
    let application = interner.intern_class_instance(class, vec![wk.string]);
    let target_application = interner.intern_class_instance(target_class, vec![wk.string]);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let outcome = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.is_assignable(application, target_application)
    };
    assert!(matches!(
        outcome,
        RelationOutcome::Exhausted(Exhaustion::ClassProjectionBudget)
    ));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));

    let projection_memo = FxHashMap::default();
    let evaluator_memo = FxHashMap::default();
    let transaction = ProjectionPlanner::new(
        &mut interner,
        &published,
        &projection_memo,
        &evaluator_memo,
        0,
        false,
        None,
    )
    .plan(&[application]);
    assert_eq!(transaction.pending_projection_writes.len(), 128);
    assert_eq!(transaction.plan.frontier.len(), 1);
}

#[test]
fn non_regular_same_pair_succeeds_without_projecting_the_spine() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_007);
    let param = TypeParamId(80_007);
    let param_ty = interner.intern_type_param(param, "T");
    let nested_arg = interner.intern_array(param_ty);
    let recursive = interner.intern_class_instance(class, vec![nested_arg]);
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("next", recursive)],
        ..Default::default()
    });
    let published = published(class, vec![param], template, wk.error);
    let application = interner.intern_class_instance(class, vec![wk.string]);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let outcome = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.is_assignable(application, application)
    };
    assert!(matches!(outcome, RelationOutcome::Yes));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
}

#[test]
fn finite_same_class_argument_witness_precedes_recursive_frontier_without_nominalizing_phantoms() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_207);
    let parameter = TypeParamId(80_207);
    let parameter_ty = interner.intern_type_param(parameter, "T");
    let nested_argument = interner.intern_array(parameter_ty);
    let recursive = interner.intern_class_instance(class, vec![nested_argument]);
    let template = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("next", recursive),
            PropertyType::public("value", parameter_ty),
        ],
        ..Default::default()
    });
    let witnessed_published = published(class, vec![parameter], template, wk.error);
    let source = interner.intern_class_instance(class, vec![wk.string]);
    let target = interner.intern_class_instance(class, vec![wk.number]);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let mismatch = SemanticQueryCoordinator::new(
        &mut interner,
        &witnessed_published,
        &mut state,
        &mut next_type_param,
    )
    .is_assignable(source, target);
    assert!(matches!(mismatch, RelationOutcome::No(_)));

    let phantom_class = ClassId(80_208);
    let phantom_parameter = TypeParamId(80_208);
    let phantom_template = interner.intern_object(ObjectType::default());
    let phantom_published = published(
        phantom_class,
        vec![phantom_parameter],
        phantom_template,
        wk.error,
    );
    let phantom_source = interner.intern_class_instance(phantom_class, vec![wk.string]);
    let phantom_target = interner.intern_class_instance(phantom_class, vec![wk.number]);
    let compatible = SemanticQueryCoordinator::new(
        &mut interner,
        &phantom_published,
        &mut state,
        &mut next_type_param,
    )
    .is_assignable(phantom_source, phantom_target);
    assert!(matches!(compatible, RelationOutcome::Yes));
}

#[test]
fn outer_demand_does_not_descend_through_ordinary_wrappers() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_107);
    let param = TypeParamId(80_107);
    let param_ty = interner.intern_type_param(param, "T");
    let nested_arg = interner.intern_array(param_ty);
    let recursive = interner.intern_class_instance(class, vec![nested_arg]);
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("next", recursive)],
        ..Default::default()
    });
    let published = published(class, vec![param], template, wk.error);
    let application = interner.intern_class_instance(class, vec![wk.string]);
    let object = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", application)],
        ..Default::default()
    });
    let union = interner.union(vec![application, wk.string]);
    let array = interner.intern_array(application);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    for root in [object, union, array] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .demand(root);
        assert_eq!(outcome, DemandOutcome::Ready(root));
    }
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
}

#[test]
fn prepublication_state_precedes_same_pair_identity_and_writes_nothing() {
    let mut interner = Interner::with_intrinsics();
    let class = ClassId(80_104);
    let application = interner.intern_class_instance(class, Vec::new());

    for expected_state in [
        ClassConstructionState::Pending,
        ClassConstructionState::Building,
        ClassConstructionState::Built,
    ] {
        let published = PublishedClasses::forged(class, expected_state);
        let mut state = SemanticQueryState::default();
        let mut next_type_param = 0;
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(application, application);

        assert!(matches!(
            outcome,
            RelationOutcome::Exhausted(Exhaustion::ClassNotPublished {
                class: found,
                state: found_state,
            }) if found == class && found_state == expected_state
        ));
        assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    }
}

#[test]
fn nested_class_boundary_precedes_child_identity_and_writes_nothing() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_105);
    let application = interner.intern_class_instance(class, Vec::new());
    let source = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("extra", wk.number),
            PropertyType::public("value", application),
        ],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", application)],
        ..Default::default()
    });
    assert_ne!(source, target);

    for expected_state in [
        ClassConstructionState::Pending,
        ClassConstructionState::Building,
        ClassConstructionState::Built,
    ] {
        let published = PublishedClasses::forged(class, expected_state);
        let mut state = SemanticQueryState::default();
        let mut next_type_param = 0;
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target);

        assert!(matches!(
            outcome,
            RelationOutcome::Exhausted(Exhaustion::ClassNotPublished {
                class: found,
                state: found_state,
            }) if found == class && found_state == expected_state
        ));
        assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    }

    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([(class, ClassConstructionState::Poisoned)]),
        FxHashMap::default(),
        FxHashMap::from_iter([(class, PublishedClassPoison::Initializer)]),
    )
    .expect("poisoned test publication is complete");
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(source, target);

    assert!(matches!(
        outcome,
        RelationOutcome::Exhausted(Exhaustion::ClassInitializerPoison { class: found })
            if found == class
    ));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
}

#[test]
fn nested_equal_composite_cannot_hide_an_unpublished_or_poisoned_class() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_106);
    let application = interner.intern_class_instance(class, Vec::new());
    let wrappers = [
        ("array", interner.intern_array(application)),
        (
            "object",
            interner.intern_object(ObjectType {
                properties: vec![PropertyType::public("nested", application)],
                ..Default::default()
            }),
        ),
        (
            "function",
            interner.intern_function(FunctionType {
                type_params: Vec::new(),
                receiver: None,
                params: Vec::new(),
                ret: application,
            }),
        ),
        ("tuple", interner.intern_tuple(vec![application])),
    ];

    for (wrapper_name, wrapper) in wrappers {
        let source = interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public("extra", wk.number),
                PropertyType::public("value", wrapper),
            ],
            ..Default::default()
        });
        let target = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("value", wrapper)],
            ..Default::default()
        });
        assert_ne!(source, target);

        for expected_state in [
            ClassConstructionState::Pending,
            ClassConstructionState::Building,
            ClassConstructionState::Built,
        ] {
            let published = PublishedClasses::forged(class, expected_state);
            let mut state = SemanticQueryState::default();
            let mut next_type_param = 0;
            let outcome = SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, target);

            assert!(
                matches!(
                    outcome,
                    RelationOutcome::Exhausted(Exhaustion::ClassNotPublished {
                        class: found,
                        state: found_state,
                    }) if found == class && found_state == expected_state
                ),
                "equal {wrapper_name} wrapper hid {expected_state:?}",
            );
            assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
        }

        let published = PublishedClasses::from_publication(
            FxHashMap::from_iter([(class, ClassConstructionState::Poisoned)]),
            FxHashMap::default(),
            FxHashMap::from_iter([(class, PublishedClassPoison::Initializer)]),
        )
        .expect("poisoned test publication is complete");
        let mut state = SemanticQueryState::default();
        let mut next_type_param = 0;
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target);

        assert!(
            matches!(
                outcome,
                RelationOutcome::Exhausted(Exhaustion::ClassInitializerPoison { class: found })
                    if found == class
            ),
            "equal {wrapper_name} wrapper hid initializer poison",
        );
        assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    }
}

#[test]
fn identical_deferred_relation_remains_lazy_and_writes_nothing() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let deferred = interner.intern_deferred_indexed_access(wk.number, wk.string);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    reset_query_demand_measure();
    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(deferred, deferred);

    assert!(matches!(outcome, RelationOutcome::Yes));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    assert_eq!(query_demand_measure(), QueryDemandMeasure::default());
}

#[test]
fn overload_relation_does_not_plan_an_unrelated_deferred_result_tail() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (recursive, selected_key, _, _) =
        object_with_irrelevant_recursive_computations(&mut interner);
    let container = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("selected", recursive)],
        ..Default::default()
    });
    let deferred = interner.intern_deferred_indexed_access(container, selected_key);
    let selected_target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("selected", wk.number)],
        ..Default::default()
    });
    let overload = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", deferred)],
        ret: wk.void,
    });
    let implementation = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", selected_target)],
        ret: wk.void,
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .overload_implementation_compatible(overload, implementation);

    assert!(matches!(outcome, RelationOutcome::Yes), "{outcome:?}");
}

#[test]
fn lazy_relation_root_must_not_misbranch_conditional_through_class_projection() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_000);
    let key = interner.intern_literal(LiteralValue::String("value".into()));
    let indexed_object = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let deferred = interner.intern_deferred_indexed_access(indexed_object, key);
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("selected", deferred)],
        ..Default::default()
    });
    let published = published(class, Vec::new(), template, wk.error);
    let application = interner.intern_class_instance(class, Vec::new());
    let extends_ty = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("selected", wk.number)],
        ..Default::default()
    });
    let conditional = interner.intern_conditional(ConditionalType {
        check: application,
        extends_ty,
        true_branch: wk.number,
        false_branch: wk.string,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(conditional, wk.string);

    assert!(
        matches!(outcome, RelationOutcome::No(_)),
        "the conditional is number, so accepting it as string is a false negative: {outcome:?}"
    );
}

#[test]
fn overload_entry_rejects_equal_composite_class_boundaries_without_writes() {
    let mut interner = Interner::with_intrinsics();
    let class = ClassId(80_110);
    let application = interner.intern_class_instance(class, Vec::new());
    let wrapper = interner.intern_array(application);
    let overload = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("overload", wrapper)],
        ret: wrapper,
    });
    let implementation = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("implementation", wrapper)],
        ret: wrapper,
    });
    assert_ne!(overload, implementation);

    for expected_state in [
        ClassConstructionState::Pending,
        ClassConstructionState::Building,
        ClassConstructionState::Built,
    ] {
        let published = PublishedClasses::forged(class, expected_state);
        let mut state = SemanticQueryState::default();
        let mut next_type_param = 0;
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .overload_implementation_compatible(overload, implementation);

        assert!(matches!(
            outcome,
            RelationOutcome::Exhausted(Exhaustion::ClassNotPublished {
                class: found,
                state: found_state,
            }) if found == class && found_state == expected_state
        ));
        assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    }

    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([(class, ClassConstructionState::Poisoned)]),
        FxHashMap::default(),
        FxHashMap::from_iter([(class, PublishedClassPoison::Initializer)]),
    )
    .expect("poisoned test publication is complete");
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .overload_implementation_compatible(overload, implementation);

    assert!(matches!(
        outcome,
        RelationOutcome::Exhausted(Exhaustion::ClassInitializerPoison { class: found })
            if found == class
    ));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
}

#[test]
fn poison_precedes_same_pair_identity_and_cache() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_004);
    let application = interner.intern_class_instance(class, Vec::new());
    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([(class, ClassConstructionState::Poisoned)]),
        FxHashMap::default(),
        FxHashMap::from_iter([(class, PublishedClassPoison::Initializer)]),
    )
    .expect("poisoned test publication is complete");
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let outcome = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.is_assignable(application, application)
    };
    assert!(matches!(
        outcome,
        RelationOutcome::Exhausted(Exhaustion::ClassInitializerPoison { class: found })
            if found == class
    ));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    assert_eq!(wk.error, interner.well_known().error);
}

#[test]
fn deferred_index_and_keyof_see_query_local_class_projection() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_005);
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let published = published(class, Vec::new(), template, wk.error);
    let application = interner.intern_class_instance(class, Vec::new());
    let key = interner.intern_literal(LiteralValue::String("value".to_string()));
    let deferred = interner.intern_deferred_indexed_access(application, key);
    let keyof = interner.intern_keyof(application);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let deferred_outcome = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.demand(deferred)
    };
    assert_eq!(deferred_outcome, DemandOutcome::Ready(wk.number));

    let keyof_outcome = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.demand(keyof)
    };
    assert_eq!(keyof_outcome, DemandOutcome::Ready(key));
}

#[test]
fn keyof_demand_ignores_unrelated_recursive_object_values() {
    let mut interner = Interner::with_intrinsics();
    let (object, selected, irrelevant, _) =
        object_with_irrelevant_recursive_computations(&mut interner);
    let expected = interner.union(vec![selected, irrelevant]);
    let keyof = interner.intern_keyof(object);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    reset_query_demand_measure();
    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(keyof);
    let measure = query_demand_measure();

    assert_eq!(outcome, DemandOutcome::Ready(expected));
    assert_eq!(measure.evaluation_budget_exhaustions, 0, "{measure:?}");
    assert!(measure.evaluation_expansions < 16, "{measure:?}");
    assert!(measure.planner_visits < 16, "{measure:?}");
}

#[test]
fn deferred_indexed_demand_ignores_unrelated_recursive_object_values() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (object, selected, _, missing) =
        object_with_irrelevant_recursive_computations(&mut interner);
    let selected_access = interner.intern_deferred_indexed_access(object, selected);
    let missing_access = interner.intern_deferred_indexed_access(object, missing);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    reset_query_demand_measure();
    let selected_outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(selected_access);
    let selected_measure = query_demand_measure();

    assert_eq!(selected_outcome, DemandOutcome::Ready(wk.number));
    assert_eq!(
        selected_measure.evaluation_budget_exhaustions, 0,
        "{selected_measure:?}"
    );
    assert!(
        selected_measure.evaluation_expansions < 16,
        "{selected_measure:?}"
    );
    assert!(selected_measure.planner_visits < 16, "{selected_measure:?}");

    reset_query_demand_measure();
    let missing_outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(missing_access);
    let missing_measure = query_demand_measure();

    assert_eq!(missing_outcome, DemandOutcome::Ready(wk.error));
    assert_eq!(
        missing_measure.evaluation_budget_exhaustions, 0,
        "{missing_measure:?}"
    );
    assert!(
        missing_measure.evaluation_expansions < 16,
        "{missing_measure:?}"
    );
    assert!(missing_measure.planner_visits < 16, "{missing_measure:?}");
}

#[test]
fn conditional_mapped_and_indexed_results_reach_nested_class_projection() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_205);
    let class_template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let published = published(class, Vec::new(), class_template, wk.error);
    let application = interner.intern_class_instance(class, Vec::new());
    let nested = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("node", application)],
        ..Default::default()
    });
    let projected_nested = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("node", class_template)],
        ..Default::default()
    });
    let conditional = interner.intern_conditional(ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: nested,
        false_branch: wk.never,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let slot = interner.intern_literal(LiteralValue::String("slot".into()));
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: false,
        key_source: slot,
        value_template: application,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let projected_mapped = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("slot", class_template)],
        ..Default::default()
    });
    let node = interner.intern_literal(LiteralValue::String("node".into()));
    let indexed = interner.intern_deferred_indexed_access(nested, node);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    for (source, target) in [(conditional, projected_nested), (mapped, projected_mapped)] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target);
        assert!(matches!(outcome, RelationOutcome::Yes));
    }

    let indexed =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(indexed);
    assert_eq!(indexed, DemandOutcome::Ready(class_template));
}

#[test]
fn evaluator_overlay_is_transactional_and_relation_order_independent() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let conditional = interner.intern_conditional(ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: wk.string,
        false_branch: wk.boolean,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let yes = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.is_assignable(conditional, wk.string)
    };
    assert!(matches!(yes, RelationOutcome::Yes));
    let after_yes = state.durable_lengths();
    assert!(after_yes.1 > 0);

    let no = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.is_assignable(conditional, wk.number)
    };
    assert!(matches!(no, RelationOutcome::No(_)));

    let yes_again = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.is_assignable(conditional, wk.string)
    };
    assert!(matches!(yes_again, RelationOutcome::Yes));
}

#[test]
fn identity_normalizes_nested_aliases_without_mutating_the_published_shape() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter = TypeParamId(80_305);
    let parameter_ty = interner.intern_type_param(parameter, "T");
    let identity_template = interner.intern_conditional(ConditionalType {
        check: parameter_ty,
        extends_ty: wk.any,
        true_branch: parameter_ty,
        false_branch: wk.never,
        infer_count: 0,
        distributive: true,
        poisoned: false,
    });
    let identity_string =
        interner.intern_instantiation(identity_template, vec![(parameter, wk.string)]);
    let published_root = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", identity_string)],
        ..Default::default()
    });
    let normalized_root = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });
    let published_before = interner
        .store()
        .object_type(published_root)
        .expect("published root is an object")
        .clone();
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_identical(published_root, normalized_root);

    assert_eq!(outcome, DemandOutcome::Ready(true));
    let published_after = interner
        .store()
        .object_type(published_root)
        .expect("published root remains an object");
    assert_eq!(
        published_after.properties.len(),
        published_before.properties.len()
    );
    assert_eq!(published_after.string_index, published_before.string_index);
    assert_eq!(published_after.number_index, published_before.number_index);
    assert_eq!(
        published_after.call_signatures,
        published_before.call_signatures
    );
    assert_eq!(
        published_after.construct_signatures,
        published_before.construct_signatures
    );
    assert_eq!(
        interner
            .store()
            .object_type(published_root)
            .and_then(|object| object.property("value"))
            .map(|property| property.ty),
        Some(identity_string)
    );
    let (_, evaluations, relations, completed, no_candidates) = state.durable_lengths();
    assert!(evaluations > 0);
    assert_eq!(relations, 0);
    assert_eq!(completed, 0);
    assert_eq!(no_candidates, 0);
}

#[test]
fn identity_is_exact_for_literals_any_and_optional_properties() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let literal = interner.intern_literal(LiteralValue::Number(1.0));
    let required = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });
    let mut optional_property = PropertyType::public("value", wk.string);
    optional_property.optional = true;
    let optional = interner.intern_object(ObjectType {
        properties: vec![optional_property],
        ..Default::default()
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    for (left, right) in [
        (literal, wk.number),
        (wk.any, wk.string),
        (required, optional),
    ] {
        assert_eq!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_identical(left, right),
            DemandOutcome::Ready(false)
        );
    }
    assert_eq!(state.durable_lengths().2, 0);
}

#[test]
fn identity_aligns_generic_function_binders_and_handles_recursive_objects() {
    let mut interner = Interner::with_intrinsics();
    let left_parameter = TypeParamId(80_405);
    let right_parameter = TypeParamId(80_406);
    let left_parameter_ty = interner.intern_type_param(left_parameter, "T");
    let right_parameter_ty = interner.intern_type_param(right_parameter, "U");
    let left_function = interner.intern_function(FunctionType {
        type_params: vec![crate::types::repr::GenericTypeParam {
            id: left_parameter,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("left", left_parameter_ty)],
        ret: left_parameter_ty,
    });
    let right_function = interner.intern_function(FunctionType {
        type_params: vec![crate::types::repr::GenericTypeParam {
            id: right_parameter,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("right", right_parameter_ty)],
        ret: right_parameter_ty,
    });
    let left_recursive = interner.reserve_object();
    interner.fill_object(
        left_recursive,
        ObjectType {
            properties: vec![PropertyType::public("next", left_recursive)],
            ..Default::default()
        },
    );
    let right_recursive = interner.reserve_object();
    interner.fill_object(
        right_recursive,
        ObjectType {
            properties: vec![PropertyType::public("next", right_recursive)],
            ..Default::default()
        },
    );
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    for (left, right) in [
        (left_function, right_function),
        (left_recursive, right_recursive),
    ] {
        assert_eq!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_identical(left, right),
            DemandOutcome::Ready(true)
        );
    }
}

#[test]
fn identity_terminates_for_recursive_generic_function_shapes() {
    let mut interner = Interner::with_intrinsics();
    let left_parameter = TypeParamId(80_455);
    let right_parameter = TypeParamId(80_456);
    let left_parameter_ty = interner.intern_type_param(left_parameter, "T");
    let right_parameter_ty = interner.intern_type_param(right_parameter, "U");
    let left_recursive = interner.reserve_object();
    let right_recursive = interner.reserve_object();
    let left_function = interner.intern_function(FunctionType {
        type_params: vec![crate::types::repr::GenericTypeParam {
            id: left_parameter,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("left", left_parameter_ty)],
        ret: left_recursive,
    });
    let right_function = interner.intern_function(FunctionType {
        type_params: vec![crate::types::repr::GenericTypeParam {
            id: right_parameter,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("right", right_parameter_ty)],
        ret: right_recursive,
    });
    interner.fill_object(
        left_recursive,
        ObjectType {
            call_signatures: vec![left_function],
            ..Default::default()
        },
    );
    interner.fill_object(
        right_recursive,
        ObjectType {
            call_signatures: vec![right_function],
            ..Default::default()
        },
    );
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    assert_eq!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_identical(left_recursive, right_recursive),
        DemandOutcome::Ready(true)
    );
}

#[test]
fn identity_aligns_alpha_binders_through_every_deferred_tag() {
    fn generic(interner: &mut Interner, parameter: TypeParamId, ret: TypeId) -> TypeId {
        interner.intern_function(FunctionType {
            type_params: vec![crate::types::repr::GenericTypeParam {
                id: parameter,
                constraint: None,
                default: None,
            }],
            receiver: None,
            params: Vec::new(),
            ret,
        })
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let left_parameter = TypeParamId(80_465);
    let right_parameter = TypeParamId(80_466);
    let alias_parameter = TypeParamId(80_467);
    let left_param = interner.intern_type_param(left_parameter, "T");
    let right_param = interner.intern_type_param(right_parameter, "U");
    let alias_param = interner.intern_type_param(alias_parameter, "A");
    let mapped_value = interner.intern_mapped_value();
    let index = interner.intern_literal(LiteralValue::String("value".into()));
    let constraint = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("value", wk.string),
            PropertyType::public("other", wk.number),
        ],
        ..Default::default()
    });
    assert!(interner.set_type_param_constraint(left_parameter, constraint));
    assert!(interner.set_type_param_constraint(right_parameter, constraint));
    let alias_template = interner.intern_conditional(ConditionalType {
        check: alias_param,
        extends_ty: wk.any,
        true_branch: alias_param,
        false_branch: wk.never,
        infer_count: 0,
        distributive: true,
        poisoned: true,
    });

    let left_deferred = [
        interner.intern_conditional(ConditionalType {
            check: left_param,
            extends_ty: wk.any,
            true_branch: left_param,
            false_branch: wk.never,
            infer_count: 0,
            distributive: true,
            poisoned: true,
        }),
        interner.intern_keyof(left_param),
        interner.intern_template(TemplateType {
            texts: vec!["x".into(), String::new()],
            holes: vec![left_param],
        }),
        interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: left_param,
            value_template: mapped_value,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        }),
        interner.intern_instantiation(alias_template, vec![(alias_parameter, left_param)]),
        interner.intern_deferred_indexed_access(left_param, index),
    ];
    let right_deferred = [
        interner.intern_conditional(ConditionalType {
            check: right_param,
            extends_ty: wk.any,
            true_branch: right_param,
            false_branch: wk.never,
            infer_count: 0,
            distributive: true,
            poisoned: true,
        }),
        interner.intern_keyof(right_param),
        interner.intern_template(TemplateType {
            texts: vec!["x".into(), String::new()],
            holes: vec![right_param],
        }),
        interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: right_param,
            value_template: mapped_value,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        }),
        interner.intern_instantiation(alias_template, vec![(alias_parameter, right_param)]),
        interner.intern_deferred_indexed_access(right_param, index),
    ];
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    for (left, right) in left_deferred.into_iter().zip(right_deferred) {
        let left = generic(&mut interner, left_parameter, left);
        let right = generic(&mut interner, right_parameter, right);
        assert_eq!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_identical(left, right),
            DemandOutcome::Ready(true)
        );
    }

    let other = interner.intern_literal(LiteralValue::String("other".into()));
    let left_indexed = interner.intern_deferred_indexed_access(left_param, index);
    let right_indexed = interner.intern_deferred_indexed_access(right_param, other);
    let left = generic(&mut interner, left_parameter, left_indexed);
    let right = generic(&mut interner, right_parameter, right_indexed);
    assert_eq!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_identical(left, right),
        DemandOutcome::Ready(false)
    );
}

#[test]
fn identity_ignores_only_public_declaring_class_origins() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut left_public = PropertyType::public("value", wk.string);
    left_public.declaring_class = Some(ClassId(80_601));
    let mut right_public = PropertyType::public("value", wk.string);
    right_public.declaring_class = Some(ClassId(80_602));
    let left_public = interner.intern_object(ObjectType {
        properties: vec![left_public],
        ..Default::default()
    });
    let right_public = interner.intern_object(ObjectType {
        properties: vec![right_public],
        ..Default::default()
    });
    let mut left_private = PropertyType::public("value", wk.string);
    left_private.visibility = Visibility::Private;
    left_private.declaring_class = Some(ClassId(80_603));
    let mut right_private = PropertyType::public("value", wk.string);
    right_private.visibility = Visibility::Private;
    right_private.declaring_class = Some(ClassId(80_604));
    let left_private = interner.intern_object(ObjectType {
        properties: vec![left_private],
        ..Default::default()
    });
    let right_private = interner.intern_object(ObjectType {
        properties: vec![right_private],
        ..Default::default()
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    for (left, right, expected) in [
        (left_public, right_public, true),
        (left_private, right_private, false),
    ] {
        assert_eq!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_identical(left, right),
            DemandOutcome::Ready(expected)
        );
    }
}

#[test]
fn identity_exhaustion_discards_writes_and_skips_unrelated_children() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_505);
    let infer = interner.intern_infer(0);
    let allocating = interner.intern_conditional(ConditionalType {
        check: wk.string,
        extends_ty: infer,
        true_branch: infer,
        false_branch: wk.never,
        infer_count: 1,
        distributive: false,
        poisoned: false,
    });
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", allocating)],
        ..Default::default()
    });
    let published = published(class, Vec::new(), template, wk.error);
    let left = interner.intern_class_instance(class, Vec::new());
    let mut right = wk.string;
    for _ in 0..=DEFAULT_STEP_BUDGET {
        right = interner.intern_conditional(ConditionalType {
            check: wk.number,
            extends_ty: wk.number,
            true_branch: right,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
    }
    let mut state = SemanticQueryState::default();
    let initial_type_param = 90_505;
    let mut next_type_param = initial_type_param;

    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_identical(left, right);

    assert_eq!(
        outcome,
        DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
    );
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    assert_eq!(next_type_param, initial_type_param);
    assert!(matches!(
        published.published_class(class),
        DemandOutcome::Ready(surface) if surface.instance_template() == template
    ));
    assert_eq!(
        interner
            .store()
            .object_type(template)
            .and_then(|object| object.property("value"))
            .map(|property| property.ty),
        Some(allocating)
    );
}

#[test]
fn planner_bounds_successive_evaluator_results_as_one_query() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut root = wk.string;
    for _ in 0..=DEFAULT_STEP_BUDGET {
        root = interner.intern_conditional(ConditionalType {
            check: wk.number,
            extends_ty: wk.number,
            true_branch: root,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
    }
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(root);

    assert!(matches!(
        outcome,
        DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
    ));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
}

#[test]
fn exhausted_inference_attempt_contributes_no_candidate_or_write() {
    let mut interner = Interner::with_intrinsics();
    let class = ClassId(80_006);
    let application = interner.intern_class_instance(class, Vec::new());
    let parameter = TypeParamId(80_006);
    let target = interner.intern_type_param(parameter, "T");
    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([(class, ClassConstructionState::Poisoned)]),
        FxHashMap::default(),
        FxHashMap::from_iter([(class, PublishedClassPoison::Heritage)]),
    )
    .expect("poisoned test publication is complete");
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let mut candidates = Candidates::default();

    let outcome = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        );
        coordinator.infer_types(application, target, &mut candidates)
    };
    assert!(matches!(
        outcome,
        DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { class: found })
            if found == class
    ));
    assert!(candidates.is_empty());
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
}

#[test]
fn exhausted_inference_preserves_caller_candidates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(80_106);
    let application = interner.intern_class_instance(class, Vec::new());
    let parameter = TypeParamId(80_106);
    let target = interner.intern_type_param(parameter, "T");
    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([(class, ClassConstructionState::Poisoned)]),
        FxHashMap::default(),
        FxHashMap::from_iter([(class, PublishedClassPoison::Heritage)]),
    )
    .expect("poisoned test publication is complete");
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let existing_parameter = TypeParamId(80_206);
    let mut candidates = Candidates::from_iter([(existing_parameter, vec![wk.number])]);

    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .infer_types(application, target, &mut candidates);
    assert!(matches!(outcome, DemandOutcome::Exhausted(_)));
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates.get(&existing_parameter), Some(&vec![wk.number]));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
}

#[test]
fn relation_preserves_frontier_vs_earlier_mismatch_order() {
    fn recursive_surface(
        interner: &mut Interner,
        class: ClassId,
        parameter: TypeParamId,
        recursive_name: &str,
        kind_name: &str,
        kind: TypeId,
    ) -> TypeId {
        let parameter_ty = interner.intern_type_param(parameter, "T");
        let nested = interner.intern_array(parameter_ty);
        let recursive = interner.intern_class_instance(class, vec![nested]);
        interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public(recursive_name, recursive),
                PropertyType::public(kind_name, kind),
            ],
            ..Default::default()
        })
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source_kind = interner.intern_literal(LiteralValue::String("source".to_string()));
    let target_kind = interner.intern_literal(LiteralValue::String("target".to_string()));
    let source_class = ClassId(80_008);
    let target_class = ClassId(80_009);
    let source_param = TypeParamId(80_008);
    let target_param = TypeParamId(80_009);

    let source_exhausts_first = recursive_surface(
        &mut interner,
        source_class,
        source_param,
        "aNext",
        "zKind",
        source_kind,
    );
    let target_exhausts_first = recursive_surface(
        &mut interner,
        target_class,
        target_param,
        "aNext",
        "zKind",
        target_kind,
    );
    let published_exhausts_first = PublishedClasses::from_publication(
        FxHashMap::from_iter([
            (source_class, ClassConstructionState::Published),
            (target_class, ClassConstructionState::Published),
        ]),
        FxHashMap::from_iter([
            (
                source_class,
                PublishedClassSurface::new(
                    source_class,
                    vec![source_param],
                    source_exhausts_first,
                    wk.error,
                    None,
                ),
            ),
            (
                target_class,
                PublishedClassSurface::new(
                    target_class,
                    vec![target_param],
                    target_exhausts_first,
                    wk.error,
                    None,
                ),
            ),
        ]),
        FxHashMap::default(),
    )
    .expect("test publication is complete");
    let source = interner.intern_class_instance(source_class, vec![wk.string]);
    let target = interner.intern_class_instance(target_class, vec![wk.string]);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let exhausted = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published_exhausts_first,
            &mut state,
            &mut next_type_param,
        );
        coordinator.is_assignable(source, target)
    };
    assert!(matches!(
        exhausted,
        RelationOutcome::Exhausted(Exhaustion::ClassProjectionBudget)
    ));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));

    let early_source_class = ClassId(80_108);
    let early_target_class = ClassId(80_109);
    let early_source_param = TypeParamId(80_108);
    let early_target_param = TypeParamId(80_109);
    let early_source = recursive_surface(
        &mut interner,
        early_source_class,
        early_source_param,
        "zNext",
        "aKind",
        source_kind,
    );
    let early_target = recursive_surface(
        &mut interner,
        early_target_class,
        early_target_param,
        "zNext",
        "aKind",
        target_kind,
    );
    let published_early = PublishedClasses::from_publication(
        FxHashMap::from_iter([
            (early_source_class, ClassConstructionState::Published),
            (early_target_class, ClassConstructionState::Published),
        ]),
        FxHashMap::from_iter([
            (
                early_source_class,
                PublishedClassSurface::new(
                    early_source_class,
                    vec![early_source_param],
                    early_source,
                    wk.error,
                    None,
                ),
            ),
            (
                early_target_class,
                PublishedClassSurface::new(
                    early_target_class,
                    vec![early_target_param],
                    early_target,
                    wk.error,
                    None,
                ),
            ),
        ]),
        FxHashMap::default(),
    )
    .expect("test publication is complete");
    let early_source = interner.intern_class_instance(early_source_class, vec![wk.string]);
    let early_target = interner.intern_class_instance(early_target_class, vec![wk.string]);
    let mut early_state = SemanticQueryState::default();
    let early_mismatch = {
        let mut coordinator = SemanticQueryCoordinator::new(
            &mut interner,
            &published_early,
            &mut early_state,
            &mut next_type_param,
        );
        coordinator.is_assignable(early_source, early_target)
    };
    assert!(matches!(early_mismatch, RelationOutcome::No(_)));
    let (projections, evaluations, relations, completed, no_candidates) =
        early_state.durable_lengths();
    assert_eq!(projections, 2);
    assert_eq!(evaluations, 0);
    assert!(relations > 0);
    assert_eq!(completed, 0);
    assert_eq!(no_candidates, 1);
}

/// The production query path must alpha-align a DOM-style recursive listener's
/// deferred `Map[K]` event lookup without sharing a mismatched cached verdict.
#[test]
fn recursive_generic_listener_deferred_lookup_is_order_independent() {
    use crate::types::repr::GenericTypeParam;

    fn property(name: &str, ty: TypeId) -> PropertyType {
        PropertyType::public(name, ty)
    }

    fn event_map(interner: &mut Interner, payload: TypeId) -> TypeId {
        let event = interner.intern_object(ObjectType {
            properties: vec![property("payload", payload)],
            ..Default::default()
        });
        interner.intern_object(ObjectType {
            properties: vec![property("change", event)],
            ..Default::default()
        })
    }

    fn listener_root(interner: &mut Interner, binder_id: TypeParamId, event_map: TypeId) -> TypeId {
        let root = interner.reserve_object();
        let key = interner.intern_type_param(binder_id, "K");
        let event = interner.intern_deferred_indexed_access(event_map, key);
        let listener = interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: Some(root),
            params: vec![ParameterType::required("event", event)],
            ret: interner.well_known().void,
        });
        let constraint = interner.intern_keyof(event_map);
        let add_event_listener = interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id: binder_id,
                constraint: Some(constraint),
                default: None,
            }],
            receiver: None,
            params: vec![ParameterType::required("listener", listener)],
            ret: interner.well_known().void,
        });
        let string = interner.well_known().string;
        interner.fill_object(
            root,
            ObjectType {
                properties: vec![
                    property("addEventListener", add_event_listener),
                    property("align", string),
                    property("base", string),
                    property("self", root),
                ],
                ..Default::default()
            },
        );
        root
    }

    fn is_assignable(
        interner: &mut Interner,
        published: &PublishedClasses,
        state: &mut SemanticQueryState,
        next_type_param: &mut u32,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        match SemanticQueryCoordinator::new(interner, published, state, next_type_param)
            .is_assignable(source, target)
        {
            RelationOutcome::Yes => true,
            RelationOutcome::No(_) => false,
            RelationOutcome::Exhausted(exhaustion) => {
                panic!("listener relation unexpectedly exhausted: {exhaustion:?}")
            }
        }
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let number_map = event_map(&mut interner, wk.number);
    let string_map = event_map(&mut interner, wk.string);
    let left = listener_root(&mut interner, TypeParamId(90_351), number_map);
    let right = listener_root(&mut interner, TypeParamId(90_352), number_map);
    let incompatible = listener_root(&mut interner, TypeParamId(90_353), string_map);
    let published = PublishedClasses::empty();

    let mut mismatch_first_state = SemanticQueryState::default();
    let mut mismatch_first_next = 100_000;
    let mismatch_first = is_assignable(
        &mut interner,
        &published,
        &mut mismatch_first_state,
        &mut mismatch_first_next,
        left,
        incompatible,
    );
    let left_to_right_after_mismatch = is_assignable(
        &mut interner,
        &published,
        &mut mismatch_first_state,
        &mut mismatch_first_next,
        left,
        right,
    );
    let right_to_left_after_mismatch = is_assignable(
        &mut interner,
        &published,
        &mut mismatch_first_state,
        &mut mismatch_first_next,
        right,
        left,
    );

    let mut compatible_first_state = SemanticQueryState::default();
    let mut compatible_first_next = 110_000;
    let right_to_left_first = is_assignable(
        &mut interner,
        &published,
        &mut compatible_first_state,
        &mut compatible_first_next,
        right,
        left,
    );
    let left_to_right_second = is_assignable(
        &mut interner,
        &published,
        &mut compatible_first_state,
        &mut compatible_first_next,
        left,
        right,
    );
    let reverse_mismatch = is_assignable(
        &mut interner,
        &published,
        &mut compatible_first_state,
        &mut compatible_first_next,
        incompatible,
        right,
    );

    assert!(!mismatch_first);
    assert!(!reverse_mismatch);
    assert!(left_to_right_after_mismatch);
    assert!(right_to_left_after_mismatch);
    assert!(right_to_left_first);
    assert!(left_to_right_second);
}

fn poisoned_publication(classes: &[(ClassId, PublishedClassPoison)]) -> PublishedClasses {
    PublishedClasses::from_publication(
        classes
            .iter()
            .map(|(class, _)| (*class, ClassConstructionState::Poisoned))
            .collect(),
        FxHashMap::default(),
        classes.iter().copied().collect(),
    )
    .expect("poisoned test publication is complete")
}

#[test]
fn publication_clean_cache_skips_repeated_acyclic_and_recursive_graphs() {
    let mut interner = Interner::with_intrinsics();
    let first = interner.reserve_object();
    let second = interner.reserve_object();
    interner.fill_object(
        first,
        ObjectType {
            properties: vec![PropertyType::public("next", second)],
            ..Default::default()
        },
    );
    interner.fill_object(
        second,
        ObjectType {
            properties: vec![PropertyType::public("next", first)],
            ..Default::default()
        },
    );
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let guard = start_query_source_cold_measure();

    assert_eq!(
        publication_exhaustion(interner.store(), &[first], &published, &mut state),
        None
    );
    let cold = query_source_cold_measure().expect("measurement is active");
    assert_eq!(cold.publication_edge_visits, 2);
    assert_eq!(state.publication_clean.len(), 2);

    assert_eq!(
        publication_exhaustion(interner.store(), &[second], &published, &mut state),
        None
    );
    let warm = query_source_cold_measure().expect("measurement is active");
    assert_eq!(warm.publication_edge_visits, cold.publication_edge_visits);
    drop(guard);
}

#[test]
fn publication_clean_cache_preserves_first_poison_in_cyclic_order() {
    let mut interner = Interner::with_intrinsics();
    let first_poison = ClassId(81_001);
    let second_poison = ClassId(81_002);
    let first_application = interner.intern_class_instance(first_poison, Vec::new());
    let second_application = interner.intern_class_instance(second_poison, Vec::new());
    let first = interner.reserve_object();
    let second = interner.reserve_object();
    interner.fill_object(
        first,
        ObjectType {
            properties: vec![
                PropertyType::public("aPoison", first_application),
                PropertyType::public("zCycle", second),
            ],
            ..Default::default()
        },
    );
    interner.fill_object(
        second,
        ObjectType {
            properties: vec![
                PropertyType::public("aPoison", second_application),
                PropertyType::public("zCycle", first),
            ],
            ..Default::default()
        },
    );
    let published = poisoned_publication(&[
        (first_poison, PublishedClassPoison::Heritage),
        (second_poison, PublishedClassPoison::Surface),
    ]);
    let mut state = SemanticQueryState::default();

    for _ in 0..3 {
        assert_eq!(
            publication_exhaustion(interner.store(), &[first], &published, &mut state),
            Some(Exhaustion::ClassSurfacePoison {
                class: second_poison
            })
        );
        assert!(state.publication_clean.is_empty());
    }
    assert_eq!(
        publication_exhaustion(interner.store(), &[second], &published, &mut state),
        Some(Exhaustion::ClassHeritagePoison {
            class: first_poison
        })
    );
    assert!(state.publication_clean.is_empty());
}

#[test]
fn publication_clean_cache_invalidates_after_type_param_constraint_change() {
    let mut interner = Interner::with_intrinsics();
    let parameter = TypeParamId(81_003);
    let parameter_type = interner.intern_type_param(parameter, "T");
    let poison = ClassId(81_003);
    let poison_application = interner.intern_class_instance(poison, Vec::new());
    let published = poisoned_publication(&[(poison, PublishedClassPoison::Initializer)]);
    let mut state = SemanticQueryState::default();

    assert_eq!(
        publication_exhaustion(interner.store(), &[parameter_type], &published, &mut state),
        None
    );
    assert!(state.publication_clean.contains(&parameter_type));
    assert!(interner.set_type_param_constraint(parameter, poison_application));
    assert_eq!(
        publication_exhaustion(interner.store(), &[parameter_type], &published, &mut state),
        Some(Exhaustion::ClassInitializerPoison { class: poison })
    );
    assert!(state.publication_clean.is_empty());
}

#[test]
fn publication_clean_cache_invalidates_after_reserved_type_fill() {
    let mut interner = Interner::with_intrinsics();
    let root = interner.reserve_object();
    let poison = ClassId(81_004);
    let poison_application = interner.intern_class_instance(poison, Vec::new());
    let published = poisoned_publication(&[(poison, PublishedClassPoison::Heritage)]);
    let mut state = SemanticQueryState::default();

    assert_eq!(
        publication_exhaustion(interner.store(), &[root], &published, &mut state),
        None
    );
    assert!(state.publication_clean.contains(&root));
    interner.fill_object(
        root,
        ObjectType {
            properties: vec![PropertyType::public("poison", poison_application)],
            ..Default::default()
        },
    );
    assert_eq!(
        publication_exhaustion(interner.store(), &[root], &published, &mut state),
        Some(Exhaustion::ClassHeritagePoison { class: poison })
    );
    assert!(state.publication_clean.is_empty());
}

#[test]
fn publication_clean_cache_uses_snapshot_identity_and_clone_stability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let child = interner.intern_object(ObjectType::default());
    let root = interner.intern_array(child);
    let base_publication = PublishedClasses::empty();
    let cloned = base_publication.clone();
    let class = ClassId(81_005);
    let extension = published(class, Vec::new(), wk.error, wk.error);
    let extended = base_publication
        .clone()
        .extend(extension)
        .expect("disjoint publication extends");
    let mut state = SemanticQueryState::default();
    let guard = start_query_source_cold_measure();

    assert_eq!(
        publication_exhaustion(interner.store(), &[root], &base_publication, &mut state),
        None
    );
    let cold = query_source_cold_measure().expect("measurement is active");
    assert_eq!(cold.publication_edge_visits, 1);
    assert_eq!(
        publication_exhaustion(interner.store(), &[root], &cloned, &mut state),
        None
    );
    let cloned_warm = query_source_cold_measure().expect("measurement is active");
    assert_eq!(cloned_warm.publication_edge_visits, 1);
    assert_eq!(
        publication_exhaustion(interner.store(), &[root], &extended, &mut state),
        None
    );
    let extended_cold = query_source_cold_measure().expect("measurement is active");
    assert_eq!(extended_cold.publication_edge_visits, 2);
    drop(guard);
}

#[test]
fn publication_clean_cache_never_certifies_projection_overlays() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let ready = ClassId(81_006);
    let poison = ClassId(81_007);
    let poison_application = interner.intern_class_instance(poison, Vec::new());
    let template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("poison", poison_application)],
        ..Default::default()
    });
    let application = interner.intern_class_instance(ready, Vec::new());
    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([
            (ready, ClassConstructionState::Published),
            (poison, ClassConstructionState::Poisoned),
        ]),
        FxHashMap::from_iter([(
            ready,
            PublishedClassSurface::new(ready, Vec::new(), template, wk.error, None),
        )]),
        FxHashMap::from_iter([(poison, PublishedClassPoison::Surface)]),
    )
    .expect("test publication is complete");
    let mut state = SemanticQueryState::default();

    assert_eq!(
        publication_exhaustion(interner.store(), &[application], &published, &mut state),
        None
    );
    assert!(state.publication_clean.contains(&application));
    assert!(!state.publication_clean.contains(&template));
    assert!(!state.publication_clean.contains(&poison_application));

    let target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("poison", wk.number)],
        ..Default::default()
    });
    let mut next_type_param = 0;
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(application, target),
        RelationOutcome::Exhausted(Exhaustion::ClassSurfacePoison { class })
            if class == poison
    ));
    assert!(!state.publication_clean.contains(&template));
    assert!(!state.publication_clean.contains(&poison_application));
}

fn decidable_conditional(interner: &mut Interner, result: TypeId) -> TypeId {
    let wk = interner.well_known();
    interner.intern_conditional(ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: result,
        false_branch: wk.never,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    })
}

#[test]
fn borrowed_durable_identity_requires_explicit_admission() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let conditional = decidable_conditional(&mut interner, wk.string);
    let durable = FxHashMap::from_iter([(conditional, conditional)]);
    let plan = ProjectionPlan {
        durable_evaluation_memo: Some(&durable),
        ..ProjectionPlan::default()
    };

    assert_eq!(plan.normalize(conditional), Ok(conditional));
    assert!(matches!(
        plan.relation_demand(interner.store(), conditional),
        Some(RelationDemand::Evaluation(found)) if found == conditional
    ));

    let published = PublishedClasses::empty();
    let projection = FxHashMap::default();
    let transaction = ProjectionPlanner::new(
        &mut interner,
        &published,
        &projection,
        &durable,
        0,
        false,
        None,
    )
    .plan(&[conditional]);
    assert_eq!(transaction.plan.normalize(conditional), Ok(conditional));
    assert_eq!(
        transaction
            .plan
            .relation_demand(interner.store(), conditional),
        None
    );
    assert!(transaction.pending_evaluator_writes.is_empty());
}

#[test]
fn local_evaluation_override_wins_and_commits_only_the_delta() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let conditional = decidable_conditional(&mut interner, wk.number);
    let retained = decidable_conditional(&mut interner, wk.boolean);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState {
        evaluation_memo: FxHashMap::from_iter([(conditional, wk.number), (retained, wk.boolean)]),
        publication_store_identity: Some(Arc::clone(interner.store().semantic_graph_identity())),
        publication_snapshot_identity: Some(Arc::clone(published.identity())),
        ..SemanticQueryState::default()
    };
    let projection = FxHashMap::default();
    let transaction = {
        let mut planner = ProjectionPlanner::new(
            &mut interner,
            &published,
            &projection,
            &state.evaluation_memo,
            0,
            false,
            None,
        );
        planner.record_evaluation(conditional, wk.string);
        planner.finish()
    };
    assert_eq!(transaction.plan.normalize(conditional), Ok(wk.string));
    assert_eq!(
        transaction.pending_evaluator_writes,
        FxHashMap::from_iter([(conditional, wk.string)])
    );

    let commit = transaction.into_commit();
    let mut next_type_param = 0;
    SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
        .commit_plan(commit);
    assert_eq!(state.evaluation_memo.get(&conditional), Some(&wk.string));
    assert_eq!(state.evaluation_memo.get(&retained), Some(&wk.boolean));
    assert_eq!(state.evaluation_memo.len(), 2);
}

#[test]
fn tainted_transaction_discards_local_delta_and_preserves_parent() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let retained = decidable_conditional(&mut interner, wk.boolean);
    let completed = decidable_conditional(&mut interner, wk.string);
    let mut exhausting = wk.number;
    for _ in 0..=DEFAULT_STEP_BUDGET {
        exhausting = decidable_conditional(&mut interner, exhausting);
    }
    let source = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("aCompleted", completed),
            PropertyType::public("zExhausting", exhausting),
        ],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("aCompleted", wk.string),
            PropertyType::public("zExhausting", wk.number),
        ],
        ..Default::default()
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState {
        evaluation_memo: FxHashMap::from_iter([(retained, wk.boolean)]),
        publication_store_identity: Some(Arc::clone(interner.store().semantic_graph_identity())),
        publication_snapshot_identity: Some(Arc::clone(published.identity())),
        ..SemanticQueryState::default()
    };
    let original = state.evaluation_memo.clone();
    let mut next_type_param = 0;

    assert_eq!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_identical(source, target),
        DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
    );
    assert_eq!(state.evaluation_memo, original);
}

#[test]
fn cycle_tainted_declared_roots_recompute_per_context_without_durable_promotion() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter_id = TypeParamId(99_251);
    let parameter = interner.intern_type_param(parameter_id, "T");
    let a = interner.reserve_object();
    let b = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("back", a),
            PropertyType::public("value", parameter),
        ],
        ..Default::default()
    });
    interner.fill_object(
        a,
        ObjectType {
            properties: vec![
                PropertyType::public("child", b),
                PropertyType::public("value", parameter),
            ],
            ..Default::default()
        },
    );

    let b_inside_a = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("back", a),
            PropertyType::public("value", wk.number),
        ],
        ..Default::default()
    });
    let expected_a = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("child", b_inside_a),
            PropertyType::public("value", wk.number),
        ],
        ..Default::default()
    });
    let a_inside_b = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("child", b),
            PropertyType::public("value", wk.number),
        ],
        ..Default::default()
    });
    let expected_b = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("back", a_inside_b),
            PropertyType::public("value", wk.number),
        ],
        ..Default::default()
    });

    let number = interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.number));
    let a_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: a,
        parameters: vec![parameter_id],
        arguments: vec![number],
    });
    let b_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: b,
        parameters: vec![parameter_id],
        arguments: vec![number],
    });
    let a_root = interner.intern_declared(a_recipe, []);
    let b_root = interner.intern_declared(b_recipe, []);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    assert_eq!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(a_root),
        DemandOutcome::Ready(expected_a)
    );
    assert!(
        state.evaluation_memo.is_empty(),
        "a cycle-tainted result must not become durable"
    );
    assert_eq!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(b_root),
        DemandOutcome::Ready(expected_b)
    );
    assert!(
        state.evaluation_memo.is_empty(),
        "the second root must recompute in its own live-stack context"
    );
    assert_ne!(expected_a, expected_b);
}

#[test]
fn normalization_follows_borrowed_durable_chain_without_local_copies() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first = decidable_conditional(&mut interner, wk.number);
    let second = decidable_conditional(&mut interner, wk.string);
    let durable = FxHashMap::from_iter([(first, second), (second, wk.string)]);
    let published = PublishedClasses::empty();
    let projection = FxHashMap::default();
    let planner = ProjectionPlanner::new(
        &mut interner,
        &published,
        &projection,
        &durable,
        0,
        false,
        None,
    );
    assert_eq!(planner.plan.normalize(first), Ok(wk.string));
    assert!(planner.working_evaluation_memo.is_empty());
    assert!(planner.plan.evaluation_overlay.is_empty());
    let transaction = planner.finish();

    assert_eq!(transaction.plan.normalize(first), Ok(wk.string));
    assert!(transaction.plan.evaluation_overlay.is_empty());
    assert!(transaction.plan.resolved_evaluations.is_empty());
    assert!(transaction.pending_evaluator_writes.is_empty());
}

fn expect_no(outcome: RelationOutcome) -> Arc<ReasonChain> {
    match outcome {
        RelationOutcome::No(reason) => reason,
        RelationOutcome::Yes => panic!("expected relation failure, got success"),
        RelationOutcome::Exhausted(reason) => {
            panic!("expected relation failure, got exhaustion: {reason:?}")
        }
    }
}

#[test]
fn completed_failure_replays_pointer_identical_reason_and_keeps_directions_distinct() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let number = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let string = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let first = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(number, string),
    );
    let first_render = crate::diagnostics::render_reason_chain(interner.store(), first.head());
    let promoted = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(number, string),
    );
    let repeated = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(number, string),
    );
    assert!(Arc::ptr_eq(&promoted, &repeated));
    assert_eq!(
        crate::diagnostics::render_reason_chain(interner.store(), repeated.head()),
        first_render
    );

    let reverse_first = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(string, number),
    );
    let reverse_promoted = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(string, number),
    );
    let reverse_repeated = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(string, number),
    );
    assert!(!Arc::ptr_eq(&first, &reverse_first));
    assert!(!Arc::ptr_eq(&promoted, &reverse_promoted));
    assert!(Arc::ptr_eq(&reverse_promoted, &reverse_repeated));
    assert_eq!(state.completed_relation_len(), 2);
    assert_eq!(state.completed_relation_no_candidate_len(), 0);
    assert!(state.relation_cache.len() > state.completed_relation_len());
}

#[test]
fn one_shot_failures_retain_only_compact_admission_keys() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });

    for index in 0..128 {
        let source = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public(format!("value{index}"), wk.number)],
            ..Default::default()
        });
        expect_no(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, target),
        );
    }

    assert_eq!(state.completed_relation_len(), 0);
    assert_eq!(state.completed_relation_no_candidate_len(), 128);
}

#[test]
fn completed_success_hits_without_planning_again() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType::default());
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let guard = start_query_source_cold_measure();

    for _ in 0..2 {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, target),
            RelationOutcome::Yes
        ));
    }
    let measure = query_source_cold_measure().expect("measurement is active");
    assert_eq!(measure.completed_relation_yes_inserts, 1);
    assert_eq!(measure.completed_relation_yes_hits, 1);
    assert_eq!(measure.planner_transactions, 1);
    assert_eq!(state.completed_relation_len(), 1);
    drop(guard);
}

#[test]
fn canonical_object_checks_present_named_members_and_enforces_merged_indexes() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let named_only = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("prototypeMember", wk.string)],
        ..Default::default()
    });
    let indexed = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("prototypeMember", wk.string)],
        string_index: Some(wk.number),
        ..Default::default()
    });
    let compatible = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let incompatible = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });
    let matching_overlap = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("prototypeMember", wk.string)],
        ..Default::default()
    });
    let conflicting_overlap = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("prototypeMember", wk.number)],
        ..Default::default()
    });
    let empty = interner.intern_object(ObjectType::default());
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    state.set_library_object_context(Some(library_object_context_for_test(
        &mut interner,
        named_only,
    )));
    for source in [wk.number, compatible, empty, matching_overlap] {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, named_only),
            RelationOutcome::Yes
        ));
    }
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(conflicting_overlap, named_only),
        RelationOutcome::No(_)
    ));

    state.set_library_object_context(Some(library_object_context_for_test(
        &mut interner,
        indexed,
    )));
    for source in [compatible, empty] {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, indexed),
            RelationOutcome::Yes
        ));
    }
    for source in [wk.number, incompatible] {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, indexed),
            RelationOutcome::No(_)
        ));
    }
}

#[test]
fn canonical_object_enforces_merged_call_and_construct_signatures() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let call = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.string,
    });
    let wrong_call = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.string)],
        ret: wk.string,
    });
    let callable_object = interner.intern_object(ObjectType {
        call_signatures: vec![call],
        ..Default::default()
    });
    let constructable_object = interner.intern_object(ObjectType {
        construct_signatures: vec![call],
        ..Default::default()
    });
    let call_target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("prototypeMember", wk.string)],
        call_signatures: vec![call],
        ..Default::default()
    });
    let construct_target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("prototypeMember", wk.string)],
        construct_signatures: vec![call],
        ..Default::default()
    });
    let empty = interner.intern_object(ObjectType::default());
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    state.set_library_object_context(Some(library_object_context_for_test(
        &mut interner,
        call_target,
    )));
    for source in [call, callable_object] {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, call_target),
            RelationOutcome::Yes
        ));
    }
    for source in [wrong_call, empty, wk.number] {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, call_target),
            RelationOutcome::No(_)
        ));
    }

    state.set_library_object_context(Some(library_object_context_for_test(
        &mut interner,
        construct_target,
    )));
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(constructable_object, construct_target),
        RelationOutcome::Yes
    ));
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(empty, construct_target),
        RelationOutcome::No(_)
    ));
}

#[test]
fn canonical_object_number_index_uses_apparent_native_values_in_both_orders() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let target = interner.reserve_object();
    interner.fill_object(
        target,
        ObjectType {
            properties: vec![PropertyType::public("prototypeMember", wk.string)],
            number_index: Some(target),
            ..Default::default()
        },
    );
    let empty_array = interner.intern_array(wk.never);
    let object_array = interner.intern_array(target);
    let number_array = interner.intern_array(wk.number);
    let empty_tuple = interner.intern_tuple(Vec::new());
    let object_tuple = interner.intern_tuple(vec![target]);
    let number_tuple = interner.intern_tuple(vec![wk.number]);
    let object_rest = interner.intern_tuple_type(TupleType::with_rest(
        Vec::new(),
        TupleRestType::new(0, object_array),
    ));
    let number_rest = interner.intern_tuple_type(TupleType::with_rest(
        Vec::new(),
        TupleRestType::new(0, number_array),
    ));
    let readonly_object_array = interner.intern_readonly(object_array);
    let readonly_number_array = interner.intern_readonly(number_array);
    let string_literal = interner.intern_literal(LiteralValue::String("value".into()));
    let string_template = interner.intern_template(TemplateType {
        texts: vec!["value".into(), String::new()],
        holes: vec![wk.number],
    });
    let published = PublishedClasses::empty();

    for ordered in [
        [(object_array, true), (number_array, false)],
        [(number_array, false), (object_array, true)],
    ] {
        let mut state = SemanticQueryState::default();
        let mut next_type_param = 0;
        state.set_library_object_context(Some(library_object_context_for_test(
            &mut interner,
            target,
        )));
        for (source, expected) in ordered {
            let outcome = SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, target);
            assert_eq!(
                matches!(outcome, RelationOutcome::Yes),
                expected,
                "{outcome:?}"
            );
        }
    }

    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    state.set_library_object_context(Some(library_object_context_for_test(&mut interner, target)));
    for source in [
        wk.never,
        empty_array,
        object_array,
        empty_tuple,
        object_tuple,
        object_rest,
        readonly_object_array,
        wk.string,
        string_literal,
        string_template,
    ] {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, target),
            RelationOutcome::Yes
        ));
    }
    for source in [
        number_array,
        number_tuple,
        number_rest,
        readonly_number_array,
        wk.boolean,
        wk.number,
    ] {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, target),
            RelationOutcome::No(_)
        ));
    }

    let mixed_target = interner.intern_object(ObjectType {
        number_index: Some(target),
        string_index: Some(target),
        ..Default::default()
    });
    state.set_library_object_context(Some(library_object_context_for_test(
        &mut interner,
        mixed_target,
    )));
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(object_array, mixed_target),
        RelationOutcome::No(_)
    ));
}

#[test]
fn canonical_object_lazily_selects_each_native_apparent_surface() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let target = interner.intern_object(ObjectType {
        properties: vec![optional_property("length", wk.string)],
        ..Default::default()
    });
    let number_array = interner.intern_array(wk.number);
    let readonly_number_array = interner.intern_readonly(number_array);
    let tuple = interner.intern_tuple(vec![wk.number, wk.string]);
    let readonly_tuple = interner.intern_readonly(tuple);
    let template = interner.intern_template(TemplateType {
        texts: vec!["prefix".into(), String::new()],
        holes: vec![wk.number],
    });
    let function = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: wk.number,
    });
    let context = library_object_context_for_test(&mut interner, target);
    let initial_plan = ProjectionPlan {
        library_object_context: Some(context),
        ..ProjectionPlan::default()
    };
    assert_eq!(
        initial_plan.apparent_surface(interner.store(), number_array),
        ApparentSurface::Needs(RelationDemand::ApparentSurface(number_array))
    );
    assert_eq!(
        initial_plan.apparent_surface(interner.store(), wk.string),
        ApparentSurface::Ready(context.string)
    );
    assert_eq!(
        initial_plan.apparent_surface(interner.store(), template),
        ApparentSurface::Ready(context.string)
    );

    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    state.set_library_object_context(Some(context));
    for source in [
        number_array,
        readonly_number_array,
        tuple,
        readonly_tuple,
        wk.string,
        template,
        function,
    ] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target);
        assert!(matches!(outcome, RelationOutcome::No(_)), "{outcome:?}");
    }
    for source in [wk.number, wk.boolean, wk.object] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target);
        assert!(matches!(outcome, RelationOutcome::Yes), "{outcome:?}");
    }
}

#[test]
fn canonical_object_substitutes_array_elements_and_overrides_tuple_length() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter = TypeParamId(99_002);
    let parameter_ty = interner.intern_type_param(parameter, "T");
    let array_template = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("element", parameter_ty),
            PropertyType::public("length", wk.number),
        ],
        ..Default::default()
    });
    let target_element = interner.intern_object(ObjectType {
        properties: vec![optional_property("element", wk.string)],
        ..Default::default()
    });
    let length_two = interner.intern_literal(LiteralValue::Number(2.0));
    let target_length_two = interner.intern_object(ObjectType {
        properties: vec![optional_property("length", length_two)],
        ..Default::default()
    });
    let target_length_number = interner.intern_object(ObjectType {
        properties: vec![optional_property("length", wk.number)],
        ..Default::default()
    });
    let string_array = interner.intern_array(wk.string);
    let number_array = interner.intern_array(wk.number);
    let readonly_string_array = interner.intern_readonly(string_array);
    let readonly_number_array = interner.intern_readonly(number_array);
    let string_tuple = interner.intern_tuple(vec![wk.string]);
    let mixed_tuple = interner.intern_tuple(vec![wk.string, wk.number]);
    let tuple_two = interner.intern_tuple(vec![wk.string, wk.number]);
    let tuple_one = interner.intern_tuple(vec![wk.string]);
    let rest_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.string],
        TupleRestType::new(1, number_array),
    ));
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    state.set_library_object_context(Some(library_object_context_with_array_templates_for_test(
        &mut interner,
        target_element,
        array_template,
        array_template,
        parameter,
    )));
    for source in [string_array, readonly_string_array, string_tuple] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target_element);
        assert!(matches!(outcome, RelationOutcome::Yes), "{outcome:?}");
    }
    for source in [number_array, readonly_number_array, mixed_tuple] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target_element);
        assert!(matches!(outcome, RelationOutcome::No(_)), "{outcome:?}");
    }

    state.set_library_object_context(Some(library_object_context_with_array_templates_for_test(
        &mut interner,
        target_length_two,
        array_template,
        array_template,
        parameter,
    )));
    for (source, expected) in [
        (tuple_two, true),
        (tuple_one, false),
        (string_array, false),
        (rest_tuple, false),
    ] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target_length_two);
        assert_eq!(
            matches!(outcome, RelationOutcome::Yes),
            expected,
            "{outcome:?}"
        );
    }

    state.set_library_object_context(Some(library_object_context_with_array_templates_for_test(
        &mut interner,
        target_length_number,
        array_template,
        array_template,
        parameter,
    )));
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(rest_tuple, target_length_number),
        RelationOutcome::Yes
    ));
}

#[test]
fn cycle_tainted_array_surface_never_promotes_relation_state() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter = TypeParamId(99_004);
    let parameter_ty = interner.intern_type_param(parameter, "T");
    let array_template = interner.reserve_object();
    interner.fill_object(
        array_template,
        ObjectType {
            properties: vec![
                PropertyType::public("element", parameter_ty),
                PropertyType::public("length", wk.number),
                PropertyType::public("self", array_template),
            ],
            ..Default::default()
        },
    );
    let substitutions = FxHashMap::from_iter([(parameter, wk.number)]);
    assert!(matches!(
        substitute_with_outcome(&mut interner, array_template, &substitutions),
        SubstitutionOutcome::CycleTainted(_)
    ));

    let target = interner.intern_object(ObjectType {
        properties: vec![optional_property("element", wk.number)],
        ..Default::default()
    });
    let source = interner.intern_array(wk.number);
    let context = library_object_context_with_array_templates_for_test(
        &mut interner,
        target,
        array_template,
        array_template,
        parameter,
    );
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let guard = start_query_source_cold_measure();
    state.set_library_object_context(Some(context));

    for _ in 0..2 {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target);
        assert!(matches!(outcome, RelationOutcome::Yes), "{outcome:?}");
        assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    }
    assert_eq!(
        query_source_cold_measure()
            .expect("measure active")
            .planner_transactions,
        2
    );
    drop(guard);
}

#[test]
fn ordinary_object_targets_observe_finite_tuple_rest_elements_and_length() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));
    let inner = interner.intern_tuple(vec![wk.number]);
    let finite = interner.intern_tuple_type(TupleType::with_rest(
        Vec::new(),
        TupleRestType::new(0, inner),
    ));
    let readonly_finite = interner.intern_readonly(finite);
    let number_array = interner.intern_array(wk.number);
    let variadic = interner.intern_tuple_type(TupleType::with_rest(
        Vec::new(),
        TupleRestType::new(0, number_array),
    ));
    let malformed = interner.intern_tuple_type(TupleType::with_rest(
        Vec::new(),
        TupleRestType::new(1, inner),
    ));
    let length_one = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("length", one)],
        ..Default::default()
    });
    let length_two = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("length", two)],
        ..Default::default()
    });
    let element_number = interner.intern_object(ObjectType {
        properties: vec![optional_property("element", wk.number)],
        ..Default::default()
    });
    let element_tuple = interner.intern_object(ObjectType {
        properties: vec![optional_property("element", inner)],
        ..Default::default()
    });
    let library_object = interner.intern_object(ObjectType::default());
    let context = library_object_context_for_test(&mut interner, library_object);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    state.set_library_object_context(Some(context));

    for (source, target) in [
        (finite, length_one),
        (readonly_finite, length_one),
        (finite, element_number),
        (readonly_finite, element_number),
    ] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target);
        assert!(matches!(outcome, RelationOutcome::Yes), "{outcome:?}");
    }
    for (source, target) in [
        (finite, length_two),
        (readonly_finite, length_two),
        (variadic, length_one),
        (finite, element_tuple),
        (malformed, length_one),
    ] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, target);
        let RelationOutcome::No(reason) = outcome else {
            panic!("expected an ordinary apparent-surface mismatch, got {outcome:?}");
        };
        assert_eq!(reason.root(), (source, target));
    }
}

#[test]
fn native_weak_targets_require_real_wrapper_overlap_and_keep_reason_families() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let string_literal = interner.intern_literal(LiteralValue::String("literal".into()));
    let template = interner.intern_template(TemplateType {
        texts: vec!["x-".into(), String::new()],
        holes: vec![wk.string],
    });
    let function = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: wk.void,
    });
    let array = interner.intern_array(wk.number);
    let tuple = interner.intern_tuple(vec![wk.number, wk.string]);
    let readonly_array = interner.intern_readonly(array);
    let readonly_tuple = interner.intern_readonly(tuple);
    let weak = interner.intern_object(ObjectType {
        properties: vec![optional_property("unrelated", wk.number)],
        ..Default::default()
    });
    let optional_length = interner.intern_object(ObjectType {
        properties: vec![optional_property("length", wk.number)],
        ..Default::default()
    });
    let wrong_optional_length = interner.intern_object(ObjectType {
        properties: vec![optional_property("length", wk.string)],
        ..Default::default()
    });
    let required_length = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("length", wk.number)],
        ..Default::default()
    });
    let required_missing = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("required", wk.string)],
        ..Default::default()
    });
    let library_object = interner.intern_object(ObjectType::default());
    let context = library_object_context_for_test(&mut interner, library_object);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    state.set_library_object_context(Some(context));

    for source in [
        wk.number,
        wk.boolean,
        function,
        array,
        tuple,
        readonly_array,
        readonly_tuple,
        wk.string,
        template,
        string_literal,
    ] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, weak);
        let RelationOutcome::No(reason) = outcome else {
            panic!("expected a weak-target failure, got {outcome:?}");
        };
        assert_eq!(reason.root(), (source, weak));
    }
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(wk.object, weak),
        RelationOutcome::Yes
    ));

    for source in [
        wk.string,
        template,
        array,
        tuple,
        readonly_array,
        readonly_tuple,
    ] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, optional_length);
        assert!(matches!(outcome, RelationOutcome::Yes), "{outcome:?}");
    }
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(function, required_length),
        RelationOutcome::Yes
    ));
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(function, optional_length),
        RelationOutcome::No(_)
    ));

    let string_union_number = interner.union(vec![wk.string, wk.number]);
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(string_union_number, optional_length),
        RelationOutcome::No(_)
    ));
    let mismatch =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(wk.string, wrong_optional_length);
    let RelationOutcome::No(reason) = mismatch else {
        panic!("expected an overlapping-property mismatch, got {mismatch:?}");
    };
    assert!(matches!(
        reason.head(),
        crate::relate::Reason::Property { .. }
    ));
    assert_eq!(reason.root(), (wk.string, wrong_optional_length));

    for source in [
        wk.number,
        wk.boolean,
        function,
        wk.string,
        template,
        string_literal,
    ] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, required_missing);
        let RelationOutcome::No(reason) = outcome else {
            panic!("expected a required-property failure, got {outcome:?}");
        };
        assert!(matches!(reason.head(), crate::relate::Reason::Leaf { .. }));
        assert_eq!(reason.root(), (source, required_missing));
    }
    for source in [array, tuple, readonly_array, readonly_tuple, wk.object] {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(source, required_missing);
        let RelationOutcome::No(reason) = outcome else {
            panic!("expected a missing-property failure, got {outcome:?}");
        };
        assert!(matches!(
            reason.head(),
            crate::relate::Reason::MissingProperty { .. }
        ));
        assert_eq!(reason.root(), (source, required_missing));
    }
}

#[test]
fn canonical_object_nested_demand_keeps_the_raw_source_in_the_reason() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let target = interner.intern_object(ObjectType {
        properties: vec![optional_property("length", wk.string)],
        ..Default::default()
    });
    let array = interner.intern_array(wk.number);
    let source_outer = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", array)],
        ..Default::default()
    });
    let target_outer = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", target)],
        ..Default::default()
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    state.set_library_object_context(Some(library_object_context_for_test(&mut interner, target)));

    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(array, target);
    let RelationOutcome::No(reason) = outcome else {
        panic!("expected a raw-source failure, got {outcome:?}");
    };
    assert_eq!(reason.root(), (array, target));

    let mut nested_state = SemanticQueryState::default();
    nested_state
        .set_library_object_context(Some(library_object_context_for_test(&mut interner, target)));
    let nested = SemanticQueryCoordinator::new(
        &mut interner,
        &published,
        &mut nested_state,
        &mut next_type_param,
    )
    .is_assignable(source_outer, target_outer);
    let RelationOutcome::No(reason) = nested else {
        panic!("expected a nested raw-source failure, got {nested:?}");
    };
    let crate::relate::Reason::Property { because, .. } = reason.head() else {
        panic!(
            "expected an outer property failure, got {:?}",
            reason.head()
        );
    };
    let crate::relate::Reason::Property {
        src: child_source,
        tgt: child_target,
        ..
    } = &**because
    else {
        panic!("expected a child property failure, got {because:?}");
    };
    assert_eq!((*child_source, *child_target), (array, target));
}

#[test]
fn library_object_context_changes_invalidate_relations_and_unavailable_is_terminal() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let target = interner.intern_object(ObjectType {
        properties: vec![optional_property("length", wk.number)],
        ..Default::default()
    });
    let parameter = TypeParamId(99_003);
    let missing_length = interner.intern_object(ObjectType::default());
    let array = interner.intern_array(wk.string);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let guard = start_query_source_cold_measure();

    state.set_library_object_context(Some(library_object_context_with_array_templates_for_test(
        &mut interner,
        target,
        missing_length,
        missing_length,
        parameter,
    )));
    for _ in 0..2 {
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(array, target);
        assert!(matches!(outcome, RelationOutcome::No(_)), "{outcome:?}");
    }
    assert_eq!(
        query_source_cold_measure()
            .expect("measure active")
            .planner_transactions,
        2
    );
    assert_eq!(state.completed_relation_len(), 1);

    let replacement = library_object_context_for_test(&mut interner, target);
    state.set_library_object_context(Some(replacement));
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(array, target);
    assert!(matches!(outcome, RelationOutcome::Yes), "{outcome:?}");
    assert_eq!(state.completed_relation_len(), 1);
    state.set_library_object_context(None);
    assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
    let outcome =
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(array, target);
    assert!(matches!(outcome, RelationOutcome::No(_)), "{outcome:?}");
    drop(guard);
}

#[test]
fn demand_preserves_durable_union_normalization() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let conditional = decidable_conditional(&mut interner, wk.string);
    let union = interner.union(vec![conditional, wk.number]);
    let expected = interner.union(vec![wk.string, wk.number]);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let mut candidates = Candidates::default();

    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .infer_types(union, expected, &mut candidates),
        DemandOutcome::Ready(())
    ));
    assert_eq!(state.evaluation_memo.get(&union), Some(&expected));
    assert_eq!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .demand(union),
        DemandOutcome::Ready(expected)
    );
}

#[test]
fn semantic_graph_mutation_invalidates_cached_success_before_recertification() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter = TypeParamId(82_006);
    let parameter_type = interner.intern_type_param(parameter, "T");
    assert!(interner.set_type_param_constraint(parameter, wk.number));
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(parameter_type, wk.number),
        RelationOutcome::Yes
    ));
    assert_eq!(state.completed_relation_len(), 1);
    assert!(!state.relation_cache.is_empty());

    assert!(interner.remove_type_param_constraint(parameter));
    expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(parameter_type, wk.number),
    );
    assert_eq!(state.completed_relation_len(), 0);
    assert_eq!(state.completed_relation_no_candidate_len(), 1);
}

#[test]
fn semantic_graph_mutation_invalidates_non_relation_evaluation_memo() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let reserved = interner.reserve_object();
    let key = interner.intern_literal(LiteralValue::String("value".to_string()));
    let access = interner.intern_deferred_indexed_access(reserved, key);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    assert_eq!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(access),
        DemandOutcome::Ready(wk.error)
    );
    assert_eq!(state.evaluation_memo.get(&access), Some(&wk.error));

    interner.fill_object(
        reserved,
        ObjectType {
            properties: vec![PropertyType::public("value", wk.number)],
            ..Default::default()
        },
    );
    assert_eq!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .demand(access),
        DemandOutcome::Ready(wk.number)
    );
    assert_eq!(state.evaluation_memo.get(&access), Some(&wk.number));
}

#[test]
fn publication_change_invalidates_projection_and_relation_success() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(82_007);
    let number_template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let string_template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let application = interner.intern_class_instance(class, Vec::new());
    let number_publication = published(class, Vec::new(), number_template, wk.error);
    let string_publication = published(class, Vec::new(), string_template, wk.error);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    assert!(matches!(
        SemanticQueryCoordinator::new(
            &mut interner,
            &number_publication,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(application, target),
        RelationOutcome::Yes
    ));
    expect_no(
        SemanticQueryCoordinator::new(
            &mut interner,
            &string_publication,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(application, target),
    );
}

#[test]
fn completed_outcome_invalidates_after_reserved_fill_and_constraint_change() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let reserved = interner.reserve_object();
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(reserved, target),
    );
    assert_eq!(state.completed_relation_len(), 0);
    assert_eq!(state.completed_relation_no_candidate_len(), 1);
    interner.fill_object(
        reserved,
        ObjectType {
            properties: vec![PropertyType::public("value", wk.number)],
            ..Default::default()
        },
    );
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(reserved, target),
        RelationOutcome::Yes
    ));
    assert_eq!(state.completed_relation_len(), 1);

    let parameter = TypeParamId(82_001);
    let parameter_type = interner.intern_type_param(parameter, "T");
    let parameter_source = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", parameter_type)],
        ..Default::default()
    });
    expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(parameter_source, target),
    );
    assert!(interner.set_type_param_constraint(parameter, wk.number));
    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param,)
            .is_assignable(parameter_source, target),
        RelationOutcome::Yes
    ));
    assert_eq!(state.completed_relation_len(), 1);
}

#[test]
fn publication_poison_precedes_and_invalidates_completed_success() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(82_002);
    let template = interner.intern_object(ObjectType::default());
    let application = interner.intern_class_instance(class, Vec::new());
    let target = interner.intern_object(ObjectType::default());
    let ready = published(class, Vec::new(), template, wk.error);
    let poisoned = poisoned_publication(&[(class, PublishedClassPoison::Heritage)]);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    assert!(matches!(
        SemanticQueryCoordinator::new(&mut interner, &ready, &mut state, &mut next_type_param,)
            .is_assignable(application, target),
        RelationOutcome::Yes
    ));
    assert_eq!(state.completed_relation_len(), 1);
    assert!(matches!(
        SemanticQueryCoordinator::new(
            &mut interner,
            &poisoned,
            &mut state,
            &mut next_type_param,
        )
        .is_assignable(application, target),
        RelationOutcome::Exhausted(Exhaustion::ClassHeritagePoison { class: found })
            if found == class
    ));
    assert_eq!(state.completed_relation_len(), 0);
}

#[test]
fn exhausted_relation_never_promotes_completed_outcome() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut source = wk.number;
    for _ in 0..=DEFAULT_STEP_BUDGET {
        source = decidable_conditional(&mut interner, source);
    }
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    for _ in 0..2 {
        assert!(matches!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_assignable(source, wk.string),
            RelationOutcome::Exhausted(Exhaustion::EvaluationBudget)
        ));
        assert_eq!(state.completed_relation_len(), 0);
    }
}

#[test]
fn completed_outcomes_follow_query_state_savepoint_promotion_rules() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let number = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let string = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let parent_reason = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(number, string),
    );
    assert_eq!(state.completed_relation_len(), 0);
    assert_eq!(state.completed_relation_no_candidate_len(), 1);
    state.savepoint();
    let promoted = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(number, string),
    );
    let inherited_hit = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(number, string),
    );
    assert!(Arc::ptr_eq(&promoted, &inherited_hit));
    assert_eq!(
        crate::diagnostics::render_reason_chain(interner.store(), parent_reason.head()),
        crate::diagnostics::render_reason_chain(interner.store(), promoted.head())
    );
    expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(string, number),
    );
    expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(string, number),
    );
    assert_eq!(state.completed_relation_len(), 2);
    assert_eq!(state.completed_relation_no_candidate_len(), 0);
    // Rolling the layer back must restore the pre-savepoint counts exactly,
    // including the single `No` candidate the promotion consumed.
    state.rollback();
    assert_eq!(state.completed_relation_len(), 0);
    assert_eq!(state.completed_relation_no_candidate_len(), 1);
}

#[test]
fn completed_generic_failure_preserves_binder_and_fresh_id_result() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let infer = interner.intern_infer(0);
    let check = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });
    let extends_ty = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", infer)],
        ..Default::default()
    });
    let inferred = interner.intern_conditional(ConditionalType {
        check,
        extends_ty,
        true_branch: infer,
        false_branch: wk.never,
        infer_count: 1,
        distributive: false,
        poisoned: false,
    });
    let source_param = TypeParamId(82_003);
    let target_param = TypeParamId(82_004);
    let source_param_ty = interner.intern_type_param(source_param, "T");
    let target_param_ty = interner.intern_type_param(target_param, "U");
    let source = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: source_param,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("value", source_param_ty)],
        ret: inferred,
    });
    let target = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: target_param,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("value", target_param_ty)],
        ret: wk.number,
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 90_000;

    let first = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(source, target),
    );
    let after_first = next_type_param;
    let promoted = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(source, target),
    );
    let after_promotion = next_type_param;
    let repeated = expect_no(
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_assignable(source, target),
    );
    assert!(Arc::ptr_eq(&promoted, &repeated));
    assert_eq!(
        crate::diagnostics::render_reason_chain(interner.store(), first.head()),
        crate::diagnostics::render_reason_chain(interner.store(), repeated.head())
    );
    assert!(after_promotion >= after_first);
    assert_eq!(next_type_param, after_promotion);
    assert_eq!(state.completed_relation_len(), 1);
}

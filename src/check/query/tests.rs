use super::*;
use crate::class_semantics::{ClassConstructionState, PublishedClassPoison, PublishedClassSurface};
use crate::types::repr::{
    ClassId, ConditionalType, FunctionType, LiteralValue, MappedType, ModifierOp, ObjectType,
    ParameterType, PropertyType, TypeParamId,
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));

    let projection_memo = FxHashMap::default();
    let evaluator_memo = FxHashMap::default();
    let transaction = ProjectionPlanner::new(
        &mut interner,
        &published,
        &projection_memo,
        &evaluator_memo,
        0,
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
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
        assert_eq!(state.durable_lengths(), (0, 0, 0));
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
        assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
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
            assert_eq!(state.durable_lengths(), (0, 0, 0));
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
        assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
    assert_eq!(query_demand_measure(), QueryDemandMeasure::default());
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
        assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));
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
    assert_eq!(state.durable_lengths(), (0, 0, 0));

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
    let (projections, evaluations, relations) = early_state.durable_lengths();
    assert_eq!(projections, 2);
    assert_eq!(evaluations, 0);
    assert!(relations > 0);
}

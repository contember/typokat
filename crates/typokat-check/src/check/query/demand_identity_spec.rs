//! Acceptance spec for demand-driven exact-identity queries.

use super::*;
use crate::class_semantics::{ClassConstructionState, PublishedClassSurface, PublishedClasses};
use crate::types::repr::{
    ClassId, ConditionalType, FunctionType, GenericTypeParam, LiteralValue, ObjectType,
    ParameterType, PropertyType, TypeParamId,
};

const DEFERRED_DEPTH: usize = 256;
const EARLY_WORK_LIMIT: u64 = 16;
const INDEPENDENT_SMALL: usize = 16;
const INDEPENDENT_LARGE: usize = 32;
const MAX_NO_PROGRESS_ATTEMPTS: usize = 8;

#[derive(Clone, Copy, Debug)]
struct IdentityWork {
    demand: QueryDemandMeasure,
    source_cold: QuerySourceColdMeasure,
}

fn measured<T>(run: impl FnOnce() -> T) -> (T, IdentityWork) {
    reset_query_demand_measure();
    let guard = start_query_source_cold_measure();
    let result = run();
    let work = IdentityWork {
        demand: query_demand_measure(),
        source_cold: query_source_cold_measure().expect("measurement scope is active"),
    };
    drop(guard);
    (result, work)
}

fn recursive_deferred_pair(interner: &mut Interner) -> (TypeId, TypeId) {
    let wk = interner.well_known();
    let left = interner.reserve_object();
    let right = interner.reserve_object();
    let mut left_deferred = left;
    let mut right_deferred = right;

    for _ in 0..DEFERRED_DEPTH {
        left_deferred = interner.intern_conditional(ConditionalType {
            check: wk.number,
            extends_ty: wk.number,
            true_branch: left_deferred,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
        right_deferred = interner.intern_conditional(ConditionalType {
            check: wk.number,
            extends_ty: wk.number,
            true_branch: right_deferred,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
    }

    interner.fill_object(
        left,
        ObjectType {
            properties: vec![PropertyType::public("next", left_deferred)],
            ..Default::default()
        },
    );
    interner.fill_object(
        right,
        ObjectType {
            properties: vec![PropertyType::public("next", right_deferred)],
            ..Default::default()
        },
    );
    (left, right)
}

fn mismatch_around_recursive_sibling(
    interner: &mut Interner,
    mismatch_first: bool,
) -> (TypeId, TypeId) {
    let wk = interner.well_known();
    let (left_recursive, right_recursive) = recursive_deferred_pair(interner);
    let (mismatch_name, recursive_name) = if mismatch_first {
        ("a_mismatch", "z_recursive")
    } else {
        ("z_mismatch", "a_recursive")
    };
    let left = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public(mismatch_name, wk.string),
            PropertyType::public(recursive_name, left_recursive),
        ],
        ..Default::default()
    });
    let right = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public(mismatch_name, wk.number),
            PropertyType::public(recursive_name, right_recursive),
        ],
        ..Default::default()
    });
    (left, right)
}

fn independent_deferred_pairs(interner: &mut Interner, width: usize) -> Vec<(TypeId, TypeId)> {
    let wk = interner.well_known();
    let mut pairs = Vec::with_capacity(width);
    for index in 0..width {
        let left_check = interner.intern_literal(LiteralValue::Number((index * 2) as f64));
        let right_check = interner.intern_literal(LiteralValue::Number((index * 2 + 1) as f64));
        let left = interner.intern_conditional(ConditionalType {
            check: left_check,
            extends_ty: left_check,
            true_branch: wk.number,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
        let right = interner.intern_conditional(ConditionalType {
            check: right_check,
            extends_ty: right_check,
            true_branch: wk.number,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
        pairs.push((left, right));
    }
    pairs
}

fn independent_deferred_siblings(interner: &mut Interner, width: usize) -> (TypeId, TypeId) {
    let mut left_properties = Vec::with_capacity(width);
    let mut right_properties = Vec::with_capacity(width);
    for (index, (left, right)) in independent_deferred_pairs(interner, width)
        .into_iter()
        .enumerate()
    {
        let name = format!("property{index:03}");
        left_properties.push(PropertyType::public(name.clone(), left));
        right_properties.push(PropertyType::public(name, right));
    }
    let left = interner.intern_object(ObjectType {
        properties: left_properties,
        ..Default::default()
    });
    let right = interner.intern_object(ObjectType {
        properties: right_properties,
        ..Default::default()
    });
    (left, right)
}

fn recursive_then_independent_siblings(interner: &mut Interner, width: usize) -> (TypeId, TypeId) {
    let left_recursive = interner.reserve_object();
    let right_recursive = interner.reserve_object();
    interner.fill_object(
        left_recursive,
        ObjectType {
            properties: vec![PropertyType::public("next", left_recursive)],
            ..Default::default()
        },
    );
    interner.fill_object(
        right_recursive,
        ObjectType {
            properties: vec![PropertyType::public("next", right_recursive)],
            ..Default::default()
        },
    );
    let mut left_properties = vec![PropertyType::public("recursive", left_recursive)];
    let mut right_properties = vec![PropertyType::public("recursive", right_recursive)];
    for (index, (left, right)) in independent_deferred_pairs(interner, width)
        .into_iter()
        .enumerate()
    {
        let name = format!("property{index:03}");
        left_properties.push(PropertyType::public(name.clone(), left));
        right_properties.push(PropertyType::public(name, right));
    }
    let left = interner.intern_object(ObjectType {
        properties: left_properties,
        ..Default::default()
    });
    let right = interner.intern_object(ObjectType {
        properties: right_properties,
        ..Default::default()
    });
    (left, right)
}

fn independent_deferred_parameters(interner: &mut Interner, width: usize) -> (TypeId, TypeId) {
    let wk = interner.well_known();
    let mut left_params = Vec::with_capacity(width);
    let mut right_params = Vec::with_capacity(width);
    for (index, (left, right)) in independent_deferred_pairs(interner, width)
        .into_iter()
        .enumerate()
    {
        let name = format!("parameter{index:03}");
        left_params.push(ParameterType::required(name.clone(), left));
        right_params.push(ParameterType::required(name, right));
    }
    let left = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: left_params,
        ret: wk.void,
    });
    let right = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: right_params,
        ret: wk.void,
    });
    (left, right)
}

fn measure_independent_siblings(width: usize) -> IdentityWork {
    let mut interner = Interner::with_intrinsics();
    let (left, right) = independent_deferred_siblings(&mut interner, width);
    let (outcome, work) = measured(|| check_identity(&mut interner, left, right));
    assert_eq!(outcome, DemandOutcome::Ready(true));
    work
}

fn measure_recursive_then_independent_siblings(width: usize) -> IdentityWork {
    let mut interner = Interner::with_intrinsics();
    let (left, right) = recursive_then_independent_siblings(&mut interner, width);
    let (outcome, work) = measured(|| check_identity(&mut interner, left, right));
    assert_eq!(outcome, DemandOutcome::Ready(true));
    work
}

fn measure_independent_parameters(width: usize) -> IdentityWork {
    let mut interner = Interner::with_intrinsics();
    let (left, right) = independent_deferred_parameters(&mut interner, width);
    let (outcome, work) = measured(|| check_identity(&mut interner, left, right));
    assert_eq!(outcome, DemandOutcome::Ready(true));
    work
}

fn check_identity(interner: &mut Interner, left: TypeId, right: TypeId) -> DemandOutcome<bool> {
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let outcome =
        SemanticQueryCoordinator::new(interner, &published, &mut state, &mut next_type_param)
            .is_identical(left, right);
    outcome
}

fn bounded_identity_attempts(
    interner: &mut Interner,
    published: &PublishedClasses,
    left: TypeId,
    right: TypeId,
    limit: usize,
) -> Option<DemandOutcome<bool>> {
    let projection_memo = FxHashMap::default();
    let evaluation_memo = FxHashMap::default();
    let mut planner = ProjectionPlanner::new(
        interner,
        published,
        &projection_memo,
        &evaluation_memo,
        0,
        false,
        None,
    );
    for _ in 0..limit {
        match SemanticQueryCoordinator::<PublishedClasses>::identical_attempt(
            planner.interner.store(),
            &planner.plan,
            left,
            right,
            &mut FxHashSet::default(),
            &mut Vec::new(),
        ) {
            IdentityAttempt::Decided(outcome) => return Some(outcome),
            IdentityAttempt::Needs(demand) => planner.expand_relation_demand(demand),
        }
    }
    None
}

#[test]
fn immediate_mismatch_does_not_plan_an_unrelated_recursive_deferred_sibling() {
    let mut interner = Interner::with_intrinsics();
    let (left, right) = mismatch_around_recursive_sibling(&mut interner, true);

    let (outcome, work) = measured(|| check_identity(&mut interner, left, right));

    assert_eq!(outcome, DemandOutcome::Ready(false));
    assert!(
        work.demand.planner_visits <= EARLY_WORK_LIMIT,
        "an immediate mismatch planned the unrelated sibling: {work:#?}"
    );
    assert!(
        work.demand.evaluation_expansions <= EARLY_WORK_LIMIT,
        "an immediate mismatch evaluated the unrelated sibling: {work:#?}"
    );
    assert!(
        work.source_cold.identity_recursive_calls <= EARLY_WORK_LIMIT,
        "an immediate mismatch recursively compared the unrelated sibling: {work:#?}"
    );
}

#[test]
fn reversed_member_order_preserves_the_verdict_and_demands_the_late_sibling() {
    let mut interner = Interner::with_intrinsics();
    let (left, right) = mismatch_around_recursive_sibling(&mut interner, false);

    let (outcome, work) = measured(|| check_identity(&mut interner, left, right));

    assert_eq!(outcome, DemandOutcome::Ready(false));
    assert!(
        work.demand.evaluation_expansions >= u64::try_from(DEFERRED_DEPTH).unwrap(),
        "a mismatch after the recursive sibling must traverse that sibling: {work:#?}"
    );
}

#[test]
fn independent_deferred_siblings_do_not_rewalk_a_quadratic_prefix() {
    let small = measure_independent_siblings(INDEPENDENT_SMALL);
    let large = measure_independent_siblings(INDEPENDENT_LARGE);

    assert!(
        large.source_cold.identity_recursive_calls
            <= small.source_cold.identity_recursive_calls * 3,
        "doubling independent deferred siblings must remain near-linear: small={small:#?}, large={large:#?}"
    );
}

#[test]
fn a_recursive_first_property_does_not_disable_bounded_sibling_retries() {
    let small = measure_recursive_then_independent_siblings(INDEPENDENT_SMALL);
    let large = measure_recursive_then_independent_siblings(INDEPENDENT_LARGE);

    assert!(
        large.source_cold.identity_recursive_calls
            <= small.source_cold.identity_recursive_calls * 3,
        "a coinductive first property must not make later independent siblings quadratic: small={small:#?}, large={large:#?}"
    );
}

#[test]
fn independent_deferred_function_parameters_do_not_rewalk_a_quadratic_prefix() {
    let small = measure_independent_parameters(INDEPENDENT_SMALL);
    let large = measure_independent_parameters(INDEPENDENT_LARGE);

    assert!(
        large.source_cold.identity_recursive_calls
            <= small.source_cold.identity_recursive_calls * 3,
        "doubling independent function parameters must remain near-linear: small={small:#?}, large={large:#?}"
    );
}

#[test]
fn identity_class_projection_must_make_progress_or_terminate() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let class = ClassId(92_100);
    let application = interner.intern_class_instance(class, Vec::new());
    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([(class, ClassConstructionState::Published)]),
        FxHashMap::from_iter([(
            class,
            PublishedClassSurface::new(class, Vec::new(), application, wk.error, None),
        )]),
        FxHashMap::default(),
    )
    .expect("the adversarial self projection is a complete publication");

    let outcome = bounded_identity_attempts(
        &mut interner,
        &published,
        application,
        wk.number,
        MAX_NO_PROGRESS_ATTEMPTS,
    );

    assert_eq!(
        outcome,
        Some(DemandOutcome::Ready(false)),
        "identity repeated one class-projection demand without deciding"
    );
}

#[test]
fn equal_recursive_generics_keep_alpha_binders_across_deferred_demands() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let left_parameter = TypeParamId(92_001);
    let right_parameter = TypeParamId(92_002);
    let left_parameter_ty = interner.intern_type_param(left_parameter, "T");
    let right_parameter_ty = interner.intern_type_param(right_parameter, "U");
    let left_recursive = interner.reserve_object();
    let right_recursive = interner.reserve_object();
    let left_deferred = interner.intern_conditional(ConditionalType {
        check: left_parameter_ty,
        extends_ty: wk.any,
        true_branch: left_parameter_ty,
        false_branch: wk.never,
        infer_count: 0,
        distributive: true,
        poisoned: true,
    });
    let right_deferred = interner.intern_conditional(ConditionalType {
        check: right_parameter_ty,
        extends_ty: wk.any,
        true_branch: right_parameter_ty,
        false_branch: wk.never,
        infer_count: 0,
        distributive: true,
        poisoned: true,
    });
    let left_function = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: left_parameter,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("value", left_deferred)],
        ret: left_recursive,
    });
    let right_function = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: right_parameter,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("value", right_deferred)],
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

    let (outcome, work) =
        measured(|| check_identity(&mut interner, left_recursive, right_recursive));

    assert_eq!(outcome, DemandOutcome::Ready(true));
    assert!(work.source_cold.identity_recursive_calls > 0, "{work:#?}");
    assert_eq!(work.source_cold.durable_identity_yes_inserts, 1);
}

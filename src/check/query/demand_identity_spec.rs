//! Acceptance spec for demand-driven exact-identity queries.

use super::*;
use crate::class_semantics::PublishedClasses;
use crate::types::repr::{
    ConditionalType, FunctionType, GenericTypeParam, ObjectType, ParameterType, PropertyType,
    TypeParamId,
};

const DEFERRED_DEPTH: usize = 256;
const EARLY_WORK_LIMIT: u64 = 16;

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

fn check_identity(interner: &mut Interner, left: TypeId, right: TypeId) -> DemandOutcome<bool> {
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    SemanticQueryCoordinator::new(interner, &published, &mut state, &mut next_type_param)
        .is_identical(left, right)
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

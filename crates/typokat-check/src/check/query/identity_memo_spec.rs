//! Acceptance spec for durable exact-identity query memoization.

use super::*;
use crate::class_semantics::PublishedClasses;
use crate::types::repr::{
    FunctionType, GenericTypeParam, ObjectType, ParameterType, PropertyType, TypeParamId,
};

const REPEATS: u64 = 32;

fn generic_identity(
    interner: &mut Interner,
    parameter: TypeParamId,
    return_type: Option<TypeId>,
) -> TypeId {
    let parameter_type = interner.intern_type_param(parameter, "T");
    interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: parameter,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("value", parameter_type)],
        ret: return_type.unwrap_or(parameter_type),
    })
}

fn measured<T>(run: impl FnOnce() -> T) -> (T, QuerySourceColdMeasure) {
    let guard = start_query_source_cold_measure();
    let result = run();
    let measure = query_source_cold_measure().expect("measurement scope is active");
    drop(guard);
    (result, measure)
}

#[test]
fn identical_type_id_returns_before_planner_allocation_or_recursive_walk() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let root = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.string)],
        ..Default::default()
    });
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let (outcome, measure) = measured(|| {
        SemanticQueryCoordinator::new(&mut interner, &published, &mut state, &mut next_type_param)
            .is_identical(root, root)
    });

    assert_eq!(outcome, DemandOutcome::Ready(true));
    assert_eq!(
        measure.planner_transactions, 0,
        "TypeId identity must return before constructing a projection planner"
    );
    assert_eq!(measure.planner_visits, 0);
    assert_eq!(measure.identity_recursive_calls, 0);
    assert_eq!(measure.durable_identity_yes_inserts, 0);
}

#[test]
fn repeated_nontrivial_positive_and_negative_pairs_are_constant_time_memo_hits() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let left = generic_identity(&mut interner, TypeParamId(91_001), None);
    let alpha_renamed = generic_identity(&mut interner, TypeParamId(91_002), None);
    let body_mismatch = generic_identity(&mut interner, TypeParamId(91_003), Some(wk.string));
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let (warm, warm_measure) = measured(|| {
        let positive = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_identical(left, alpha_renamed);
        let negative = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .is_identical(left, body_mismatch);
        (positive, negative)
    });
    assert_eq!(
        warm,
        (DemandOutcome::Ready(true), DemandOutcome::Ready(false))
    );
    assert_eq!(warm_measure.durable_identity_yes_inserts, 1);
    assert_eq!(warm_measure.durable_identity_no_inserts, 1);

    let ((), repeat_measure) = measured(|| {
        for iteration in 0..REPEATS {
            let (positive_left, positive_right) = if iteration % 2 == 0 {
                (left, alpha_renamed)
            } else {
                (alpha_renamed, left)
            };
            assert_eq!(
                SemanticQueryCoordinator::new(
                    &mut interner,
                    &published,
                    &mut state,
                    &mut next_type_param,
                )
                .is_identical(positive_left, positive_right),
                DemandOutcome::Ready(true)
            );

            let (negative_left, negative_right) = if iteration % 2 == 0 {
                (left, body_mismatch)
            } else {
                (body_mismatch, left)
            };
            assert_eq!(
                SemanticQueryCoordinator::new(
                    &mut interner,
                    &published,
                    &mut state,
                    &mut next_type_param,
                )
                .is_identical(negative_left, negative_right),
                DemandOutcome::Ready(false)
            );
        }
    });

    assert_eq!(repeat_measure.durable_identity_yes_hits, REPEATS);
    assert_eq!(repeat_measure.durable_identity_no_hits, REPEATS);
    assert_eq!(
        repeat_measure.planner_transactions, 0,
        "a warmed identity pair must not allocate another planner"
    );
    assert_eq!(repeat_measure.planner_visits, 0);
    assert_eq!(
        repeat_measure.identity_recursive_calls, 0,
        "a warmed identity pair must not re-walk either type graph"
    );
    assert_eq!(repeat_measure.durable_identity_yes_inserts, 0);
    assert_eq!(repeat_measure.durable_identity_no_inserts, 0);
}

#[test]
fn alpha_renamed_generics_remain_identical_with_near_miss_ordering_guards() {
    fn two_parameter_identity(interner: &mut Interner) -> TypeId {
        let first = TypeParamId(91_020);
        let second = TypeParamId(91_021);
        let first_type = interner.intern_type_param(first, "T");
        interner.intern_type_param(second, "U");
        interner.intern_function(FunctionType {
            type_params: vec![
                GenericTypeParam {
                    id: first,
                    constraint: None,
                    default: None,
                },
                GenericTypeParam {
                    id: second,
                    constraint: None,
                    default: None,
                },
            ],
            receiver: None,
            params: vec![ParameterType::required("value", first_type)],
            ret: first_type,
        })
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let left = generic_identity(&mut interner, TypeParamId(91_011), None);
    let alpha_renamed = generic_identity(&mut interner, TypeParamId(91_012), None);
    let body_mismatch = generic_identity(&mut interner, TypeParamId(91_013), Some(wk.string));
    let binder_mismatch = two_parameter_identity(&mut interner);
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    for (left, right, expected) in [
        (left, alpha_renamed, true),
        (left, body_mismatch, false),
        (body_mismatch, left, false),
        (left, binder_mismatch, false),
        (binder_mismatch, left, false),
        (alpha_renamed, left, true),
    ] {
        assert_eq!(
            SemanticQueryCoordinator::new(
                &mut interner,
                &published,
                &mut state,
                &mut next_type_param,
            )
            .is_identical(left, right),
            DemandOutcome::Ready(expected),
            "a prior alpha-equivalent or near-miss query poisoned a later result"
        );
    }
}

#[test]
fn semantic_context_identity_change_invalidates_completed_identity_memo() {
    let mut interner = Interner::with_intrinsics();
    let left = generic_identity(&mut interner, TypeParamId(91_031), None);
    let right = generic_identity(&mut interner, TypeParamId(91_032), None);
    let first_publication = PublishedClasses::empty();
    let second_publication = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    let (first, first_measure) = measured(|| {
        SemanticQueryCoordinator::new(
            &mut interner,
            &first_publication,
            &mut state,
            &mut next_type_param,
        )
        .is_identical(left, right)
    });
    assert_eq!(first, DemandOutcome::Ready(true));
    assert_eq!(first_measure.durable_identity_yes_inserts, 1);

    let (same_context, same_context_measure) = measured(|| {
        SemanticQueryCoordinator::new(
            &mut interner,
            &first_publication,
            &mut state,
            &mut next_type_param,
        )
        .is_identical(right, left)
    });
    assert_eq!(same_context, DemandOutcome::Ready(true));
    assert_eq!(same_context_measure.durable_identity_yes_hits, 1);
    assert_eq!(same_context_measure.planner_transactions, 0);
    assert_eq!(same_context_measure.identity_recursive_calls, 0);

    let (new_context, new_context_measure) = measured(|| {
        SemanticQueryCoordinator::new(
            &mut interner,
            &second_publication,
            &mut state,
            &mut next_type_param,
        )
        .is_identical(left, right)
    });
    assert_eq!(new_context, DemandOutcome::Ready(true));
    assert_eq!(new_context_measure.durable_identity_yes_hits, 0);
    assert_eq!(
        new_context_measure.durable_identity_yes_inserts, 1,
        "the old context's completed identity result must not survive refresh"
    );
    assert_eq!(new_context_measure.planner_transactions, 1);
    assert!(new_context_measure.identity_recursive_calls > 0);
}

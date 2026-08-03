//! RED contract for demand-local generic instantiation roots.

use super::{
    query_source_cold_measure, start_query_source_cold_measure, QuerySourceColdMeasure,
    SemanticQueryCoordinator, SemanticQueryState,
};
use crate::class_semantics::PublishedClasses;
use crate::relate::relation::{relation_source_cold_measure, RelationSourceColdMeasure};
use crate::relate::{Reason, RelationOutcome};
use crate::types::repr::{
    ConditionalType, LiteralValue, ObjectType, PropertyType, TypeParamId, TypeTag,
};
use crate::types::{Interner, TypeId};

const PARAMETER: TypeParamId = TypeParamId(98_100);
const SMALL_TAIL_WIDTH: usize = 16;
const LARGE_TAIL_WIDTH: usize = 64;
const TAIL_DEPTH: usize = 8;
const UNRELATED_VISIT_ALLOWANCE: u64 = 4;

#[derive(Clone, Copy, Debug)]
struct Work {
    query: QuerySourceColdMeasure,
    relation: RelationSourceColdMeasure,
}

fn measured_assignable(
    interner: &mut Interner,
    source: TypeId,
    target: TypeId,
) -> (RelationOutcome, Work) {
    let published = PublishedClasses::empty();
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;
    let guard = start_query_source_cold_measure();
    let outcome =
        SemanticQueryCoordinator::new(interner, &published, &mut state, &mut next_type_param)
            .is_assignable(source, target);
    let work = Work {
        query: query_source_cold_measure().expect("query measurement remains active"),
        relation: relation_source_cold_measure().expect("relation measurement remains active"),
    };
    drop(guard);
    (outcome, work)
}

fn generic_base(interner: &mut Interner, width: usize, depth: usize) -> TypeId {
    let wk = interner.well_known();
    let parameter = interner.intern_type_param(PARAMETER, "T");
    let root = interner.reserve_object();
    let mut properties = Vec::with_capacity(width + 1);
    properties.push(PropertyType::public("a_selected", parameter));

    for branch in 0..width {
        let mut tail = root;
        for layer in 0..depth {
            let discriminator = interner.intern_literal(LiteralValue::Number(f64::from(
                u32::try_from(branch * depth + layer).expect("synthetic position fits u32"),
            )));
            tail = interner.intern_conditional(ConditionalType {
                check: discriminator,
                extends_ty: discriminator,
                true_branch: tail,
                false_branch: wk.never,
                infer_count: 0,
                distributive: false,
                poisoned: false,
            });
        }
        properties.push(PropertyType::public(format!("tail{branch:04}"), tail));
    }
    interner.fill_object(
        root,
        ObjectType {
            properties,
            ..Default::default()
        },
    );
    root
}

fn instantiate(interner: &mut Interner, width: usize, depth: usize, argument: TypeId) -> TypeId {
    let base = generic_base(interner, width, depth);
    let instantiation = interner.intern_instantiation(base, vec![(PARAMETER, argument)]);
    assert_eq!(interner.store().tag(instantiation), TypeTag::Instantiation);
    instantiation
}

fn selected_target(interner: &mut Interner, selected: TypeId) -> TypeId {
    interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("a_selected", selected)],
        ..Default::default()
    })
}

fn assignable_in_state(
    interner: &mut Interner,
    state: &mut SemanticQueryState,
    next_type_param: &mut u32,
    source: TypeId,
    target: TypeId,
) -> RelationOutcome {
    let published = PublishedClasses::empty();
    SemanticQueryCoordinator::new(interner, &published, state, next_type_param)
        .is_assignable(source, target)
}

fn measure_irrelevant_tail(width: usize) -> Work {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = instantiate(&mut interner, width, TAIL_DEPTH, wk.string);
    let target = selected_target(&mut interner, wk.string);
    let (outcome, work) = measured_assignable(&mut interner, source, target);
    assert!(
        matches!(outcome, RelationOutcome::Yes),
        "{outcome:?}; {work:#?}"
    );
    work
}

#[test]
fn widening_an_untouched_generic_base_does_not_add_relation_work() {
    let small = measure_irrelevant_tail(SMALL_TAIL_WIDTH);
    let large = measure_irrelevant_tail(LARGE_TAIL_WIDTH);

    assert!(
        large.query.planner_visits
            <= small
                .query
                .planner_visits
                .saturating_add(UNRELATED_VISIT_ALLOWANCE),
        "widening an untouched generic base must not add planning work: small={small:#?}, large={large:#?}"
    );
    assert_eq!(
        large.relation.uncached_relation_frames,
        small.relation.uncached_relation_frames,
        "only the selected instantiated property may reach relation: small={small:#?}, large={large:#?}"
    );
}

#[test]
fn an_unrelated_generic_base_tail_cannot_exhaust_a_selected_success() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = instantiate(
        &mut interner,
        usize::try_from(super::DEFAULT_STEP_BUDGET).expect("budget fits usize") + 1,
        2,
        wk.string,
    );
    let target = selected_target(&mut interner, wk.string);

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    assert!(
        matches!(outcome, RelationOutcome::Yes),
        "{outcome:?}; {work:#?}"
    );
    assert_eq!(work.query.planner_tainted_finishes, 0, "{work:#?}");
    assert_eq!(work.query.exhaustion_frontiers, 0, "{work:#?}");
}

#[test]
fn a_reached_conditional_argument_is_still_demanded() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let argument = interner.intern_conditional(ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: wk.number,
        false_branch: wk.never,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let source = instantiate(&mut interner, 0, 0, argument);
    let target = selected_target(&mut interner, wk.string);

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    assert!(
        matches!(outcome, RelationOutcome::No(_)),
        "the reached conditional evaluates to number: {outcome:?}; {work:#?}"
    );
    assert!(
        work.query.planner_visits >= 2,
        "the instantiation and reached conditional must cross the planner: {work:#?}"
    );
}

#[test]
fn a_reached_type_parameter_argument_still_uses_its_constraint() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let argument_id = TypeParamId(98_101);
    let argument = interner.intern_type_param(argument_id, "U");
    assert!(interner.set_type_param_constraint(argument_id, wk.string));
    let source = instantiate(&mut interner, 0, 0, argument);
    let target = selected_target(&mut interner, wk.string);

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    assert!(
        matches!(outcome, RelationOutcome::Yes),
        "the reached argument constraint is string: {outcome:?}; {work:#?}"
    );
}

#[test]
fn a_later_query_demands_a_tail_skipped_by_the_first_query() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter = interner.intern_type_param(PARAMETER, "T");
    let tail = interner.intern_conditional(ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: wk.number,
        false_branch: wk.never,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let base = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("a_selected", parameter),
            PropertyType::public("z_tail", tail),
        ],
        ..Default::default()
    });
    let source = interner.intern_instantiation(base, vec![(PARAMETER, wk.string)]);
    let selected = selected_target(&mut interner, wk.string);
    let demanded_tail = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("z_tail", wk.string)],
        ..Default::default()
    });
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    assert!(matches!(
        assignable_in_state(
            &mut interner,
            &mut state,
            &mut next_type_param,
            source,
            selected,
        ),
        RelationOutcome::Yes
    ));
    assert!(matches!(
        assignable_in_state(
            &mut interner,
            &mut state,
            &mut next_type_param,
            source,
            demanded_tail,
        ),
        RelationOutcome::No(_)
    ));
}

#[test]
fn a_demanded_exhausting_tail_never_enters_the_durable_root_cache() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter = interner.intern_type_param(PARAMETER, "T");
    let mut tail = wk.string;
    for position in 0..=super::DEFAULT_STEP_BUDGET {
        let discriminator = interner.intern_literal(LiteralValue::Number(f64::from(position)));
        tail = interner.intern_conditional(ConditionalType {
            check: discriminator,
            extends_ty: discriminator,
            true_branch: tail,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
    }
    let base = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("a_selected", parameter),
            PropertyType::public("z_tail", tail),
        ],
        ..Default::default()
    });
    let source = interner.intern_instantiation(base, vec![(PARAMETER, wk.string)]);
    let selected = selected_target(&mut interner, wk.string);
    let demanded_tail = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("z_tail", wk.string)],
        ..Default::default()
    });
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 0;

    assert!(matches!(
        assignable_in_state(
            &mut interner,
            &mut state,
            &mut next_type_param,
            source,
            selected,
        ),
        RelationOutcome::Yes
    ));
    assert!(state.evaluation_memo.contains_key(&source));

    for _ in 0..2 {
        assert!(matches!(
            assignable_in_state(
                &mut interner,
                &mut state,
                &mut next_type_param,
                source,
                demanded_tail,
            ),
            RelationOutcome::Exhausted(_)
        ));
        assert!(
            !state.evaluation_memo.contains_key(&tail),
            "a tainted tail evaluation must not become durable"
        );
    }
}

#[test]
fn an_early_instantiated_property_mismatch_keeps_the_first_reason() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = instantiate(
        &mut interner,
        usize::try_from(super::DEFAULT_STEP_BUDGET).expect("budget fits usize") + 1,
        2,
        wk.number,
    );
    let target = selected_target(&mut interner, wk.string);

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    let RelationOutcome::No(reason) = outcome else {
        panic!("expected the selected-property mismatch: {outcome:?}; {work:#?}");
    };
    assert!(
        matches!(
            reason.head(),
            Reason::Property { name, .. } if name.as_string() == Some("a_selected")
        ),
        "the first selected-property mismatch must remain authoritative: {reason:#?}"
    );
    assert_eq!(work.query.exhaustion_frontiers, 0, "{work:#?}");
}

#[test]
fn recursive_generic_object_substitution_still_terminates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = instantiate(&mut interner, 1, 1, wk.string);
    let target = selected_target(&mut interner, wk.string);

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    assert!(
        matches!(outcome, RelationOutcome::Yes),
        "{outcome:?}; {work:#?}"
    );
    assert_eq!(work.query.exhaustion_frontiers, 0, "{work:#?}");
}

#[test]
fn evaluator_active_instantiation_base_keeps_its_prerequisites() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter = interner.intern_type_param(PARAMETER, "T");
    let string_result = selected_target(&mut interner, wk.string);
    let number_result = selected_target(&mut interner, wk.number);
    let base = interner.intern_conditional(ConditionalType {
        check: parameter,
        extends_ty: wk.string,
        true_branch: string_result,
        false_branch: number_result,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let source = interner.intern_instantiation(base, vec![(PARAMETER, wk.number)]);
    let target = selected_target(&mut interner, wk.string);

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    assert!(
        matches!(outcome, RelationOutcome::No(_)),
        "the conditional base selects its number branch: {outcome:?}; {work:#?}"
    );
    assert!(
        work.query.planner_visits >= 4,
        "a conditional base must retain evaluator prerequisite planning: {work:#?}"
    );
}

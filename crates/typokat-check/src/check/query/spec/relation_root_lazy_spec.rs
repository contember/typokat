//! RED contract for demand-local outer-root relation planning.

use super::{
    query_source_cold_measure, start_query_source_cold_measure, QuerySourceColdMeasure,
    SemanticQueryCoordinator, SemanticQueryState,
};
use crate::class_semantics::PublishedClasses;
use crate::relate::relation::{relation_source_cold_measure, RelationSourceColdMeasure};
use crate::relate::{Reason, RelationOutcome};
use crate::types::repr::{ConditionalType, LiteralValue, ObjectType, PropertyType};
use crate::types::{Interner, TypeId};

const SMALL_TAIL_WIDTH: usize = 16;
const LARGE_TAIL_WIDTH: usize = 48;
const TAIL_DEPTH: usize = 16;
const EXHAUSTION_TAIL_DEPTH: usize = 2;
const UNRELATED_VISIT_ALLOWANCE: u64 = 8;

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

fn recursive_conditional_tail(interner: &mut Interner, width: usize, depth: usize) -> TypeId {
    let wk = interner.well_known();
    let root = interner.reserve_object();
    let mut properties = Vec::with_capacity(width + 1);
    properties.push(PropertyType::public("a_selected", wk.string));

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
        properties.push(PropertyType::public(format!("tail{branch:03}"), tail));
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

fn selected_index_root(interner: &mut Interner, selected: TypeId) -> TypeId {
    let container = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("selected", selected)],
        ..Default::default()
    });
    let key = interner.intern_literal(LiteralValue::String("selected".into()));
    interner.intern_deferred_indexed_access(container, key)
}

fn measure_irrelevant_tail(width: usize) -> Work {
    let mut interner = Interner::with_intrinsics();
    let selected = recursive_conditional_tail(&mut interner, width, TAIL_DEPTH);
    let source = selected_index_root(&mut interner, selected);
    let wk = interner.well_known();
    let target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("a_selected", wk.string)],
        ..Default::default()
    });

    let (outcome, work) = measured_assignable(&mut interner, source, target);
    assert!(
        matches!(outcome, RelationOutcome::Yes),
        "{outcome:?}; {work:#?}"
    );
    work
}

#[test]
fn outer_index_result_does_not_plan_an_unrelated_recursive_tail() {
    let small = measure_irrelevant_tail(SMALL_TAIL_WIDTH);
    let large = measure_irrelevant_tail(LARGE_TAIL_WIDTH);

    assert!(
        large.query.planner_visits
            <= small
                .query
                .planner_visits
                .saturating_add(UNRELATED_VISIT_ALLOWANCE),
        "widening an untouched result tail must not add relation-root planning work: small={small:#?}, large={large:#?}"
    );
    assert!(
        large.relation.uncached_relation_frames <= small.relation.uncached_relation_frames + 2,
        "the relation itself must stay outer-layer local: small={small:#?}, large={large:#?}"
    );
}

#[test]
fn an_unrelated_recursive_tail_cannot_exhaust_an_outer_success() {
    let mut interner = Interner::with_intrinsics();
    let selected = recursive_conditional_tail(
        &mut interner,
        usize::try_from(super::DEFAULT_STEP_BUDGET).expect("budget fits usize") + 1,
        EXHAUSTION_TAIL_DEPTH,
    );
    let source = selected_index_root(&mut interner, selected);
    let wk = interner.well_known();
    let target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("a_selected", wk.string)],
        ..Default::default()
    });

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    assert!(
        matches!(outcome, RelationOutcome::Yes),
        "{outcome:?}; {work:#?}"
    );
    assert_eq!(work.query.planner_tainted_finishes, 0, "{work:#?}");
    assert_eq!(work.query.exhaustion_frontiers, 0, "{work:#?}");
}

#[test]
fn demanded_outer_mismatch_preserves_the_first_failure() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let recursive = recursive_conditional_tail(&mut interner, SMALL_TAIL_WIDTH, TAIL_DEPTH);
    let selected = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("a_mismatch", wk.string),
            PropertyType::public("z_recursive", recursive),
        ],
        ..Default::default()
    });
    let source = selected_index_root(&mut interner, selected);
    let target = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("a_mismatch", wk.number),
            PropertyType::public("z_recursive", recursive),
        ],
        ..Default::default()
    });

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    let RelationOutcome::No(reason) = outcome else {
        panic!("expected the outer mismatch, got {outcome:?}; {work:#?}");
    };
    assert!(
        matches!(
            reason.head(),
            Reason::Property { name, .. } if name.as_string() == Some("a_mismatch")
        ),
        "the first decisive property must remain the failure reason: {reason:#?}"
    );
}

#[test]
fn a_reached_nested_semantic_property_is_still_demanded() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let demanded = interner.intern_conditional(ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: wk.string,
        false_branch: wk.never,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let selected = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("a_demanded", demanded)],
        ..Default::default()
    });
    let source = selected_index_root(&mut interner, selected);
    let target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("a_demanded", wk.string)],
        ..Default::default()
    });

    let (outcome, work) = measured_assignable(&mut interner, source, target);

    assert!(
        matches!(outcome, RelationOutcome::Yes),
        "{outcome:?}; {work:#?}"
    );
    assert!(
        work.query.planner_visits >= 2,
        "the reached conditional must cross the planner: {work:#?}"
    );
}

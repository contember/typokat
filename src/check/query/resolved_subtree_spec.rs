//! Acceptance spec for reusing clean projection-planning subtrees.

use super::*;
use crate::check::infer::Candidates;
use crate::class_semantics::{ClassConstructionState, PublishedClassSurface};
use crate::types::repr::{
    ClassId, ConditionalType, LiteralValue, ObjectType, PropertyType, TypeParamId,
};

const STRUCTURAL_DEPTH: usize = 96;

struct ResolvedGraph {
    source: TypeId,
    target: TypeId,
    candidate_parameter: TypeParamId,
    expected_candidate: TypeId,
    class: ClassId,
    class_template: TypeId,
    static_template: TypeId,
}

fn published(graph: &ResolvedGraph) -> PublishedClasses {
    PublishedClasses::from_publication(
        FxHashMap::from_iter([(graph.class, ClassConstructionState::Published)]),
        FxHashMap::from_iter([(
            graph.class,
            PublishedClassSurface::new(
                graph.class,
                Vec::new(),
                graph.class_template,
                graph.static_template,
                None,
            ),
        )]),
        FxHashMap::default(),
    )
    .expect("test publication is complete")
}

fn wrap(interner: &mut Interner, child: TypeId) -> TypeId {
    interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("next", child)],
        ..Default::default()
    })
}

fn wrap_with_marker(interner: &mut Interner, child: TypeId, marker: TypeId) -> TypeId {
    interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("marker", marker),
            PropertyType::public("next", child),
        ],
        ..Default::default()
    })
}

fn resolved_graph(interner: &mut Interner) -> ResolvedGraph {
    let wk = interner.well_known();
    let class = ClassId(93_001);
    let class_template = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", wk.number)],
        ..Default::default()
    });
    let application = interner.intern_class_instance(class, Vec::new());
    let conditional = interner.intern_conditional(ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: application,
        false_branch: wk.never,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let holder = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("slot", conditional)],
        ..Default::default()
    });
    let slot = interner.intern_literal(LiteralValue::String("slot".into()));
    let mut source = interner.intern_deferred_indexed_access(holder, slot);

    let candidate_parameter = TypeParamId(93_001);
    let candidate = interner.intern_type_param(candidate_parameter, "Candidate");
    let mut target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", candidate)],
        ..Default::default()
    });
    for _ in 0..STRUCTURAL_DEPTH {
        source = wrap(interner, source);
        target = wrap(interner, target);
    }

    ResolvedGraph {
        source,
        target,
        candidate_parameter,
        expected_candidate: wk.number,
        class,
        class_template,
        static_template: wk.error,
    }
}

fn infer_once(
    interner: &mut Interner,
    published: &PublishedClasses,
    state: &mut SemanticQueryState,
    next_type_param: &mut u32,
    source: TypeId,
    target: TypeId,
    parameter: TypeParamId,
    expected: TypeId,
) -> QuerySourceColdMeasure {
    let guard = start_query_source_cold_measure();
    let mut candidates = Candidates::default();
    let outcome = SemanticQueryCoordinator::new(interner, published, state, next_type_param)
        .infer_types(source, target, &mut candidates);
    let measure = query_source_cold_measure().expect("measurement scope is active");
    drop(guard);

    assert_eq!(outcome, DemandOutcome::Ready(()));
    assert_eq!(
        candidates,
        Candidates::from_iter([(parameter, vec![expected])])
    );
    measure
}

#[test]
fn repeated_inference_borrows_resolved_subtrees_and_only_plans_changed_frontier() {
    let mut interner = Interner::with_intrinsics();
    let graph = resolved_graph(&mut interner);
    let published = published(&graph);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 93_100;

    let first = infer_once(
        &mut interner,
        &published,
        &mut state,
        &mut next_type_param,
        graph.source,
        graph.target,
        graph.candidate_parameter,
        graph.expected_candidate,
    );
    assert!(
        first.planner_visits > (STRUCTURAL_DEPTH * 2) as u64,
        "the cold query must certify the structural graph once: {first:?}"
    );

    let repeated = infer_once(
        &mut interner,
        &published,
        &mut state,
        &mut next_type_param,
        graph.source,
        graph.target,
        graph.candidate_parameter,
        graph.expected_candidate,
    );
    let mut distinct_measures = Vec::new();
    for index in 0..16 {
        let marker = interner.intern_literal(LiteralValue::String(format!("shared-hub-{index}")));
        let distinct_source = wrap_with_marker(&mut interner, graph.source, marker);
        let distinct_target = wrap_with_marker(&mut interner, graph.target, marker);
        let distinct = infer_once(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
            distinct_source,
            distinct_target,
            graph.candidate_parameter,
            graph.expected_candidate,
        );
        distinct_measures.push(distinct);
    }

    for distinct in distinct_measures {
        assert!(
            distinct.planner_visits <= 8,
            "a distinct root may plan its new edge, but not re-walk the shared hub: {distinct:?}"
        );
    }
    assert!(
        repeated.planner_visits <= 4,
        "a clean root must not re-walk its already-resolved subtree: {repeated:?}"
    );
    assert_eq!(repeated.planner_zero_write_finishes, 1);
}

#[test]
fn resolved_subtree_certification_is_invalidated_by_semantic_context_identity() {
    let mut interner = Interner::with_intrinsics();
    let graph = resolved_graph(&mut interner);
    let first_publication = published(&graph);
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 93_200;

    let cold = infer_once(
        &mut interner,
        &first_publication,
        &mut state,
        &mut next_type_param,
        graph.source,
        graph.target,
        graph.candidate_parameter,
        graph.expected_candidate,
    );
    assert!(cold.planner_visits > (STRUCTURAL_DEPTH * 2) as u64);

    assert!(interner.set_type_param_constraint(graph.candidate_parameter, graph.expected_candidate));
    let after_graph_mutation = infer_once(
        &mut interner,
        &first_publication,
        &mut state,
        &mut next_type_param,
        graph.source,
        graph.target,
        graph.candidate_parameter,
        graph.expected_candidate,
    );
    assert!(
        after_graph_mutation.planner_visits > (STRUCTURAL_DEPTH * 2) as u64,
        "a semantic graph mutation must force recertification: {after_graph_mutation:?}"
    );

    let warmed_again = infer_once(
        &mut interner,
        &first_publication,
        &mut state,
        &mut next_type_param,
        graph.source,
        graph.target,
        graph.candidate_parameter,
        graph.expected_candidate,
    );

    let second_publication = published(&graph);
    let after_publication_change = infer_once(
        &mut interner,
        &second_publication,
        &mut state,
        &mut next_type_param,
        graph.source,
        graph.target,
        graph.candidate_parameter,
        graph.expected_candidate,
    );
    assert!(
        after_publication_change.planner_visits > (STRUCTURAL_DEPTH * 2) as u64,
        "a new publication identity must force recertification: {after_publication_change:?}"
    );
    assert!(
        warmed_again.planner_visits <= 4,
        "the recertified graph must become reusable again: {warmed_again:?}"
    );
}

#[test]
fn exhausted_planning_never_promotes_clean_subtree_certification() {
    let mut interner = Interner::with_intrinsics();
    let graph = resolved_graph(&mut interner);
    let published = published(&graph);
    let wk = interner.well_known();
    let mut exhausting = wk.string;
    for _ in 0..=DEFAULT_STEP_BUDGET {
        exhausting = interner.intern_conditional(ConditionalType {
            check: wk.number,
            extends_ty: wk.number,
            true_branch: exhausting,
            false_branch: wk.never,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
    }
    let source = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("aResolved", graph.source),
            PropertyType::public("zExhausting", exhausting),
        ],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("aResolved", graph.target),
            PropertyType::public("zExhausting", wk.string),
        ],
        ..Default::default()
    });
    let mut state = SemanticQueryState::default();
    let mut next_type_param = 93_300;
    let existing_parameter = TypeParamId(93_301);

    let mut run = || {
        let guard = start_query_source_cold_measure();
        let mut candidates =
            Candidates::from_iter([(existing_parameter, vec![graph.expected_candidate])]);
        let outcome = SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_param,
        )
        .infer_types(source, target, &mut candidates);
        let measure = query_source_cold_measure().expect("measurement scope is active");
        drop(guard);

        assert_eq!(
            outcome,
            DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
        );
        assert_eq!(
            candidates,
            Candidates::from_iter([(existing_parameter, vec![graph.expected_candidate])])
        );
        assert_eq!(state.durable_lengths(), (0, 0, 0, 0, 0));
        measure
    };

    let first = run();
    let repeated = run();
    assert_eq!(
        repeated.planner_visits, first.planner_visits,
        "an exhausted transaction must not certify even its clean prefix"
    );
    assert_eq!(repeated.planner_tainted_finishes, 1);
    assert_eq!(repeated.planner_commits, 0);
}

//! RED contract for repeated query planning over one recursive DOM listener hub.

use super::{query_source_cold_measure, start_query_source_cold_measure, QuerySourceColdMeasure};
use crate::check::checker::check_program;
use crate::diagnostics::Diagnostic;
use crate::driver::CheckOutput;
use crate::relate::relation::{relation_source_cold_measure, RelationSourceColdMeasure};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::time::{Duration, Instant};

const SMALL_TARGETS: usize = 64;
const LARGE_TARGETS: usize = 256;
const RELEASE_SAMPLES: usize = 5;
const REPEATED_SMALL: usize = 16;
const REPEATED_LARGE: usize = 64;

const DOM_LISTENER_HUB: &str = r#"interface EventRecord {
  readonly target: EventTarget;
  readonly currentTarget: EventTarget;
}
interface EventMap {
  change: EventRecord;
  ready: EventRecord;
}
interface EventTarget {
  readonly chain: EventTarget | null;
  readonly zReason: string;
  addEventListener<K extends keyof EventMap>(type: K, listener: (this: EventTarget, event: EventMap[K]) => void): void;
  addEventListener(type: string, listener: (this: EventTarget, event: EventRecord) => void): void;
  removeEventListener<K extends keyof EventMap>(type: K, listener: (this: EventTarget, event: EventMap[K]) => void): void;
  removeEventListener(type: string, listener: (this: EventTarget, event: EventRecord) => void): void;
}
interface GlobalHandlers {
  readonly chain: GlobalHandlers | null;
  readonly zReason: number;
  addEventListener<K extends keyof EventMap>(type: K, listener: (this: GlobalHandlers, event: EventMap[K]) => void): void;
  addEventListener(type: string, listener: (this: GlobalHandlers, event: EventRecord) => void): void;
  removeEventListener<K extends keyof EventMap>(type: K, listener: (this: GlobalHandlers, event: EventMap[K]) => void): void;
  removeEventListener(type: string, listener: (this: GlobalHandlers, event: EventRecord) => void): void;
}
"#;

#[derive(Clone, Copy, Debug)]
enum QueryOrder {
    EventTargetFirst,
    GlobalHandlersFirst,
}

impl QueryOrder {
    const ALL: [Self; 2] = [Self::EventTargetFirst, Self::GlobalHandlersFirst];

    fn heritage(self) -> &'static str {
        match self {
            Self::EventTargetFirst => "EventTarget, GlobalHandlers",
            Self::GlobalHandlersFirst => "GlobalHandlers, EventTarget",
        }
    }

    fn assignments(self) -> &'static str {
        match self {
            Self::EventTargetFirst => {
                "const eventTargetToGlobal: GlobalHandlers = eventTargetValue;\nconst globalToEventTarget: EventTarget = globalHandlersValue;\n"
            }
            Self::GlobalHandlersFirst => {
                "const globalToEventTarget: EventTarget = globalHandlersValue;\nconst eventTargetToGlobal: GlobalHandlers = eventTargetValue;\n"
            }
        }
    }
}

struct ColdRun {
    source: String,
    output: CheckOutput,
    elapsed: Duration,
}

#[derive(Debug, PartialEq, Eq)]
struct DirectionalReasons {
    event_target_to_global: String,
    global_to_event_target: String,
}

fn dom_listener_source(targets: usize, order: QueryOrder) -> String {
    let mut source = String::from(DOM_LISTENER_HUB);
    for target in 0..targets {
        source.push_str(&format!(
            "interface Target{target:03} extends {} {{ readonly self: Target{target:03}; }}\n",
            order.heritage()
        ));
    }
    source.push_str(
        "declare const eventTargetValue: EventTarget;\ndeclare const globalHandlersValue: GlobalHandlers;\n",
    );
    source.push_str(order.assignments());
    source
}

fn repeated_assignment_source(repetitions: usize, order: QueryOrder) -> String {
    let mut source = String::from(DOM_LISTENER_HUB);
    source.push_str(
        "declare const eventTargetValue: EventTarget;\ndeclare const globalHandlersValue: GlobalHandlers;\n",
    );
    for repetition in 0..repetitions {
        let event_target_to_global = format!(
            "const eventTargetToGlobal{repetition:03}: GlobalHandlers = eventTargetValue;\n"
        );
        let global_to_event_target = format!(
            "const globalToEventTarget{repetition:03}: EventTarget = globalHandlersValue;\n"
        );
        match order {
            QueryOrder::EventTargetFirst => {
                source.push_str(&event_target_to_global);
                source.push_str(&global_to_event_target);
            }
            QueryOrder::GlobalHandlersFirst => {
                source.push_str(&global_to_event_target);
                source.push_str(&event_target_to_global);
            }
        }
    }
    source
}

fn check_source_cold(source: String) -> ColdRun {
    let started = Instant::now();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
    let parse_errors = parsed.diagnostics.iter().map(ToString::to_string).collect();
    let mut interner = crate::types::Interner::with_intrinsics();
    let checked = check_program(&mut interner, &parsed.program);
    let output = CheckOutput {
        diagnostics: checked.diagnostics,
        parse_errors,
        incomplete: checked.incomplete,
    };
    ColdRun {
        source,
        output,
        elapsed: started.elapsed(),
    }
}

fn check_cold_uninstrumented(targets: usize, order: QueryOrder) -> ColdRun {
    assert_eq!(query_source_cold_measure(), None);
    let run = check_source_cold(dom_listener_source(targets, order));
    assert_eq!(query_source_cold_measure(), None);
    run
}

fn check_cold_measured(targets: usize, order: QueryOrder) -> (ColdRun, QuerySourceColdMeasure) {
    assert_eq!(query_source_cold_measure(), None);
    let guard = start_query_source_cold_measure();
    let run = check_source_cold(dom_listener_source(targets, order));
    let measure = query_source_cold_measure().expect("measurement scope remains active");
    drop(guard);
    assert_eq!(query_source_cold_measure(), None);
    (run, measure)
}

#[derive(Clone, Copy, Debug)]
struct ResidualMeasure {
    query: QuerySourceColdMeasure,
    relation: RelationSourceColdMeasure,
}

fn check_repeated_measured(repetitions: usize, order: QueryOrder) -> (ColdRun, ResidualMeasure) {
    assert_eq!(query_source_cold_measure(), None);
    assert_eq!(relation_source_cold_measure(), None);
    let guard = start_query_source_cold_measure();
    let run = check_source_cold(repeated_assignment_source(repetitions, order));
    let query = query_source_cold_measure().expect("query measurement scope remains active");
    let relation =
        relation_source_cold_measure().expect("relation measurement scope remains active");
    drop(guard);
    assert_eq!(query_source_cold_measure(), None);
    assert_eq!(relation_source_cold_measure(), None);
    (run, ResidualMeasure { query, relation })
}

const REASON_PROPERTY: &str = "  Types of property 'addEventListener' are incompatible.";
const EVENT_TARGET_TO_GLOBAL_MESSAGE: &str = r#"Type '{ addEventListener: { <K extends keyof { change: { currentTarget: ...; target: ... }; ready: { currentTarget: ...; target: ... } }>(type: K, listener: (this: ..., event: { change: { currentTarget: ...; target: ... }; ready: { currentTarget: ...; target: ... } }[K]) => void): void; (type: string, listener: (this: ......' is not assignable to type '{ addEventListener: { <K extends keyof { change: { currentTarget: { addEventListener: { <K extends ...>(type: K, listener: (this: ..., event: ...[K]) => void): void; (type: string, listener: (this: ..., event: ...) => void): void }; chain: null | ...; removeEventListener: { <K extends ...>(type: K, listener: (this: ...'"#;
const GLOBAL_TO_EVENT_TARGET_MESSAGE: &str = r#"Type '{ addEventListener: { <K extends keyof { change: { currentTarget: { addEventListener: { <K extends ...>(type: K, listener: (this: ..., event: ...[K]) => void): void; (type: string, listener: (this: ..., event: ...) => void): void }; chain: null | ...; removeEventListener: { <K extends ...>(type: K, listener: (this: ...' is not assignable to type '{ addEventListener: { <K extends keyof { change: { currentTarget: ...; target: ... }; ready: { currentTarget: ...; target: ... } }>(type: K, listener: (this: ..., event: { change: { currentTarget: ...; target: ... }; ready: { currentTarget: ...; target: ... } }[K]) => void): void; (type: string, listener: (this: ......'"#;
const EVENT_TARGET_TO_GLOBAL_LEAF: &str = r#"    Type '{ <K extends keyof { change: { currentTarget: { addEventListener: ...; chain: null | ...; removeEventListener: { <K extends ...>(type: K, listener: (this: ..., event: ...[K]) => void): void; (type: string, listener: (this: ..., event: ...) => void): void }; zReason: string }; target: { addEventListener: ...; chain: ...' is not assignable to type '{ <K extends keyof { change: { currentTarget: { addEventListener: { <K extends ...>(type: K, listener: (this: ..., event: ...[K]) => void): void; (type: string, listener: (this: ..., event: ...) => void): void }; chain: null | ...; removeEventListener: { <K extends ...>(type: K, listener: (this: ..., event: ...[K]) ...'."#;
const GLOBAL_TO_EVENT_TARGET_LEAF: &str = r#"    Type '{ <K extends keyof { change: { currentTarget: { addEventListener: { <K extends ...>(type: K, listener: (this: ..., event: ...[K]) => void): void; (type: string, listener: (this: ..., event: ...) => void): void }; chain: null | ...; removeEventListener: { <K extends ...>(type: K, listener: (this: ..., event: ...[K]) ...' is not assignable to type '{ <K extends keyof { change: { currentTarget: { addEventListener: ...; chain: null | ...; removeEventListener: { <K extends ...>(type: K, listener: (this: ..., event: ...[K]) => void): void; (type: string, listener: (this: ..., event: ...) => void): void }; zReason: string }; target: { addEventListener: ...; chain: ...'."#;

fn assert_directional_reason(diagnostic: &Diagnostic, message: &str, leaf: &str) {
    let expected = Diagnostic::not_assignable(diagnostic.span, message.to_string())
        .with_elaboration(vec![REASON_PROPERTY.to_string(), leaf.to_string()]);
    assert_eq!(diagnostic, &expected);
}

fn assert_semantics(run: &ColdRun, targets: usize, order: QueryOrder) -> DirectionalReasons {
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output
            .incomplete
            .iter()
            .map(|incomplete| (&incomplete.id, &incomplete.context))
            .collect::<Vec<_>>()
    );
    let expected_message = match order {
        QueryOrder::EventTargetFirst => {
            "Interface cannot simultaneously extend types 'EventTarget' and 'GlobalHandlers'."
        }
        QueryOrder::GlobalHandlersFirst => {
            "Interface cannot simultaneously extend types 'GlobalHandlers' and 'EventTarget'."
        }
    };
    assert_eq!(run.output.diagnostics.len(), targets + 2);
    for (target, diagnostic) in run.output.diagnostics[..targets].iter().enumerate() {
        assert_eq!(diagnostic.code, crate::diagnostics::DiagnosticCode::TK2320);
        assert_eq!(diagnostic.message, expected_message);
        assert_eq!(
            &run.source[diagnostic.span.range()],
            format!("Target{target:03}")
        );
    }
    let assignment_diagnostics = &run.output.diagnostics[targets..];
    let expected_spans = match order {
        QueryOrder::EventTargetFirst => ["eventTargetToGlobal", "globalToEventTarget"],
        QueryOrder::GlobalHandlersFirst => ["globalToEventTarget", "eventTargetToGlobal"],
    };
    for (diagnostic, expected_span) in assignment_diagnostics.iter().zip(expected_spans) {
        assert_eq!(diagnostic.code, crate::diagnostics::DiagnosticCode::TK2322);
        assert_eq!(&run.source[diagnostic.span.range()], expected_span);
    }
    let reason = |span: &str| {
        assignment_diagnostics
            .iter()
            .find(|diagnostic| &run.source[diagnostic.span.range()] == span)
            .expect("both directional assignment reasons are retained")
            .clone()
    };
    let event_target_to_global = reason("eventTargetToGlobal");
    let global_to_event_target = reason("globalToEventTarget");
    assert_directional_reason(
        &event_target_to_global,
        EVENT_TARGET_TO_GLOBAL_MESSAGE,
        EVENT_TARGET_TO_GLOBAL_LEAF,
    );
    assert_directional_reason(
        &global_to_event_target,
        GLOBAL_TO_EVENT_TARGET_MESSAGE,
        GLOBAL_TO_EVENT_TARGET_LEAF,
    );
    DirectionalReasons {
        event_target_to_global: event_target_to_global.message,
        global_to_event_target: global_to_event_target.message,
    }
}

fn assert_repeated_assignment_semantics(
    run: &ColdRun,
    repetitions: usize,
    order: QueryOrder,
) -> DirectionalReasons {
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(run.output.diagnostics.len(), 2 * repetitions);

    for repetition in 0..repetitions {
        let directions = match order {
            QueryOrder::EventTargetFirst => [
                (
                    format!("eventTargetToGlobal{repetition:03}"),
                    EVENT_TARGET_TO_GLOBAL_MESSAGE,
                    EVENT_TARGET_TO_GLOBAL_LEAF,
                ),
                (
                    format!("globalToEventTarget{repetition:03}"),
                    GLOBAL_TO_EVENT_TARGET_MESSAGE,
                    GLOBAL_TO_EVENT_TARGET_LEAF,
                ),
            ],
            QueryOrder::GlobalHandlersFirst => [
                (
                    format!("globalToEventTarget{repetition:03}"),
                    GLOBAL_TO_EVENT_TARGET_MESSAGE,
                    GLOBAL_TO_EVENT_TARGET_LEAF,
                ),
                (
                    format!("eventTargetToGlobal{repetition:03}"),
                    EVENT_TARGET_TO_GLOBAL_MESSAGE,
                    EVENT_TARGET_TO_GLOBAL_LEAF,
                ),
            ],
        };
        for (diagnostic, (span, message, leaf)) in run.output.diagnostics
            [2 * repetition..2 * repetition + 2]
            .iter()
            .zip(directions)
        {
            assert_eq!(diagnostic.code, crate::diagnostics::DiagnosticCode::TK2322);
            assert_eq!(&run.source[diagnostic.span.range()], span);
            assert_directional_reason(diagnostic, message, leaf);
        }
    }

    DirectionalReasons {
        event_target_to_global: EVENT_TARGET_TO_GLOBAL_MESSAGE.to_string(),
        global_to_event_target: GLOBAL_TO_EVENT_TARGET_MESSAGE.to_string(),
    }
}

#[test]
fn dom_listener_hub_preserves_semantics_and_reason_order_at_both_scales() {
    let mut expected_reasons = None;
    for order in QueryOrder::ALL {
        for targets in [SMALL_TARGETS, LARGE_TARGETS] {
            let run = check_cold_uninstrumented(targets, order);
            let reasons = assert_semantics(&run, targets, order);
            match &expected_reasons {
                Some(expected) => assert_eq!(&reasons, expected),
                None => expected_reasons = Some(reasons),
            }
        }
    }
}

#[test]
fn repeated_dom_listener_assignments_preserve_exact_diagnostics_and_order() {
    let mut expected_reasons = None;
    for order in QueryOrder::ALL {
        for repetitions in [REPEATED_SMALL, REPEATED_LARGE] {
            let run = check_source_cold(repeated_assignment_source(repetitions, order));
            let reasons = assert_repeated_assignment_semantics(&run, repetitions, order);
            match &expected_reasons {
                Some(expected) => assert_eq!(&reasons, expected),
                None => expected_reasons = Some(reasons),
            }
        }
    }
}

#[test]
fn source_cold_measurement_scope_clears_after_unwind() {
    assert_eq!(query_source_cold_measure(), None);
    assert_eq!(relation_source_cold_measure(), None);
    let unwind = std::panic::catch_unwind(|| {
        let _guard = start_query_source_cold_measure();
        assert_eq!(
            query_source_cold_measure(),
            Some(QuerySourceColdMeasure::default())
        );
        assert_eq!(
            relation_source_cold_measure(),
            Some(RelationSourceColdMeasure::default())
        );
        panic!("measurement unwind probe");
    });
    assert!(unwind.is_err());
    assert_eq!(query_source_cold_measure(), None);
    assert_eq!(relation_source_cold_measure(), None);

    let guard = start_query_source_cold_measure();
    assert_eq!(
        query_source_cold_measure(),
        Some(QuerySourceColdMeasure::default())
    );
    assert_eq!(
        relation_source_cold_measure(),
        Some(RelationSourceColdMeasure::default())
    );
    drop(guard);
    assert_eq!(query_source_cold_measure(), None);
    assert_eq!(relation_source_cold_measure(), None);
}

fn assert_accounting(measure: QuerySourceColdMeasure) {
    assert!(measure.publication_calls > 0, "{measure:?}");
    assert_eq!(
        measure.publication_query_roots,
        2 * measure.publication_calls,
        "{measure:?}"
    );
    assert!(measure.planner_transactions > 0, "{measure:?}");
    assert_eq!(
        measure.planner_transactions,
        measure.planner_clean_finishes + measure.planner_tainted_finishes,
        "{measure:?}"
    );
    assert!(
        measure.planner_commits <= measure.planner_clean_finishes,
        "{measure:?}"
    );
    assert!(
        measure.planner_zero_write_finishes <= measure.planner_transactions,
        "{measure:?}"
    );
    assert!(
        measure.exhaustion_frontiers <= measure.planner_transactions,
        "{measure:?}"
    );
}

#[test]
fn publication_edge_work_is_bounded_by_the_unique_reachable_graph() {
    for order in QueryOrder::ALL {
        let (small, small_measure) = check_cold_measured(SMALL_TARGETS, order);
        let (large, large_measure) = check_cold_measured(LARGE_TARGETS, order);
        assert_semantics(&small, SMALL_TARGETS, order);
        assert_semantics(&large, LARGE_TARGETS, order);
        assert_accounting(small_measure);
        assert_accounting(large_measure);
        assert!(
            large_measure.publication_edge_visits <= 5 * small_measure.publication_edge_visits,
            "small={:?}, large={:?}",
            small_measure,
            large_measure
        );
        for measure in [small_measure, large_measure] {
            let reachable_bound =
                4 * (measure.publication_unique_edges + measure.publication_query_roots);
            assert!(
                measure.publication_edge_visits <= reachable_bound,
                "bound={reachable_bound}, measure={measure:?}"
            );
        }
    }
}

#[test]
fn planner_transactions_borrow_durable_memos_without_seed_copies() {
    for order in QueryOrder::ALL {
        let (run, measure) = check_cold_measured(LARGE_TARGETS, order);
        assert_semantics(&run, LARGE_TARGETS, order);
        assert_accounting(measure);
        assert_eq!(measure.durable_memo_seed_copy_entries, 0, "{:#?}", measure);
    }
}

#[derive(Clone, Copy, Debug)]
struct ResidualDelta {
    planner_transactions: u64,
    planner_clean_finishes: u64,
    planner_tainted_finishes: u64,
    planner_zero_write_finishes: u64,
    planner_commits: u64,
    durable_true_cache_hits: u64,
    durable_false_reason_rebuilds: u64,
    uncached_relation_frames: u64,
}

impl ResidualDelta {
    fn between(large: ResidualMeasure, small: ResidualMeasure) -> Self {
        let difference = |large: u64, small: u64, name: &str| {
            large
                .checked_sub(small)
                .unwrap_or_else(|| panic!("{name} decreased from {small} to {large}"))
        };
        Self {
            planner_transactions: difference(
                large.query.planner_transactions,
                small.query.planner_transactions,
                "planner transactions",
            ),
            planner_clean_finishes: difference(
                large.query.planner_clean_finishes,
                small.query.planner_clean_finishes,
                "clean planner finishes",
            ),
            planner_tainted_finishes: difference(
                large.query.planner_tainted_finishes,
                small.query.planner_tainted_finishes,
                "tainted planner finishes",
            ),
            planner_zero_write_finishes: difference(
                large.query.planner_zero_write_finishes,
                small.query.planner_zero_write_finishes,
                "zero-write planner finishes",
            ),
            planner_commits: difference(
                large.query.planner_commits,
                small.query.planner_commits,
                "planner commits",
            ),
            durable_true_cache_hits: difference(
                large.relation.durable_true_cache_hits,
                small.relation.durable_true_cache_hits,
                "durable true cache hits",
            ),
            durable_false_reason_rebuilds: difference(
                large.relation.durable_false_reason_rebuilds,
                small.relation.durable_false_reason_rebuilds,
                "durable false reason rebuilds",
            ),
            uncached_relation_frames: difference(
                large.relation.uncached_relation_frames,
                small.relation.uncached_relation_frames,
                "uncached relation frames",
            ),
        }
    }
}

fn assert_residual_accounting(measure: ResidualMeasure) {
    assert_accounting(measure.query);
    assert!(
        measure.relation.durable_false_reason_rebuilds <= measure.relation.uncached_relation_frames,
        "{measure:#?}"
    );
}

#[test]
#[ignore = "RED: classify residual repeated-query planning work before optimizing it"]
fn repeated_assignment_residual_work_is_bounded_by_the_fixed_semantic_graph() {
    const TRANSACTION_DELTA_MAX: u64 = 8;
    const TAINTED_DELTA_MAX: u64 = 0;
    const ZERO_WRITE_DELTA_MAX: u64 = 8;
    const COMMIT_DELTA_MAX: u64 = 8;
    const FALSE_REBUILD_DELTA_MAX: u64 = 4;
    const UNCACHED_FRAME_DELTA_MAX: u64 = 16;

    let mut violations = Vec::new();
    let mut records = Vec::new();
    for order in QueryOrder::ALL {
        let (small_run, small) = check_repeated_measured(REPEATED_SMALL, order);
        let (large_run, large) = check_repeated_measured(REPEATED_LARGE, order);
        assert_repeated_assignment_semantics(&small_run, REPEATED_SMALL, order);
        assert_repeated_assignment_semantics(&large_run, REPEATED_LARGE, order);
        assert_residual_accounting(small);
        assert_residual_accounting(large);
        for (name, small_value, large_value) in [
            (
                "publication unique edges",
                small.query.publication_unique_edges,
                large.query.publication_unique_edges,
            ),
            (
                "publication edge visits",
                small.query.publication_edge_visits,
                large.query.publication_edge_visits,
            ),
        ] {
            if small_value != large_value {
                violations.push(format!(
                    "{order:?}: fixed-graph premise violated: {name} changed from {small_value} to {large_value}"
                ));
            }
        }
        let delta = ResidualDelta::between(large, small);
        assert_eq!(
            delta.planner_transactions,
            delta.planner_clean_finishes + delta.planner_tainted_finishes,
            "{delta:#?}"
        );
        let _durable_true_cache_hits = delta.durable_true_cache_hits;

        for (name, actual, maximum) in [
            (
                "planner transaction granularity",
                delta.planner_transactions,
                TRANSACTION_DELTA_MAX,
            ),
            (
                "taint preventing promotion",
                delta.planner_tainted_finishes,
                TAINTED_DELTA_MAX,
            ),
            (
                "zero-write planner finishes",
                delta.planner_zero_write_finishes,
                ZERO_WRITE_DELTA_MAX,
            ),
            ("planner commits", delta.planner_commits, COMMIT_DELTA_MAX),
            (
                "durable false-reason rebuilds",
                delta.durable_false_reason_rebuilds,
                FALSE_REBUILD_DELTA_MAX,
            ),
            (
                "uncached relation frames",
                delta.uncached_relation_frames,
                UNCACHED_FRAME_DELTA_MAX,
            ),
        ] {
            if actual > maximum {
                violations.push(format!(
                    "{order:?}: {name} delta {actual} exceeds {maximum}"
                ));
            }
        }
        records.push((order, small, large, delta));
    }

    assert!(
        violations.is_empty(),
        "classification violations:\n{}\nrecords={records:#?}",
        violations.join("\n")
    );
}

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
#[cfg_attr(debug_assertions, ignore = "requires a release libtest")]
#[cfg_attr(
    not(debug_assertions),
    ignore = "release-only source-cold median checkpoint"
)]
fn release_source_cold_256_target_median_is_at_most_six_times_64() {
    for order in QueryOrder::ALL {
        assert_semantics(
            &check_cold_uninstrumented(SMALL_TARGETS, order),
            SMALL_TARGETS,
            order,
        );
        assert_semantics(
            &check_cold_uninstrumented(LARGE_TARGETS, order),
            LARGE_TARGETS,
            order,
        );
        let mut small = Vec::with_capacity(RELEASE_SAMPLES);
        let mut large = Vec::with_capacity(RELEASE_SAMPLES);
        for _ in 0..RELEASE_SAMPLES {
            small.push(check_cold_uninstrumented(SMALL_TARGETS, order).elapsed);
            large.push(check_cold_uninstrumented(LARGE_TARGETS, order).elapsed);
        }
        let small = median(&mut small);
        let large = median(&mut large);
        assert!(
            large <= small.checked_mul(6).expect("duration threshold fits"),
            "order={order:?}, 64={small:?}, 256={large:?}"
        );
    }
}

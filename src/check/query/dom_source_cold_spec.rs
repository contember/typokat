//! RED contract for repeated query planning over one recursive DOM listener hub.

use super::{query_source_cold_measure, start_query_source_cold_measure, QuerySourceColdMeasure};
use crate::check::checker::check_program;
use crate::diagnostics::Diagnostic;
use crate::driver::CheckOutput;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::time::{Duration, Instant};

const SMALL_TARGETS: usize = 64;
const LARGE_TARGETS: usize = 256;
const RELEASE_SAMPLES: usize = 5;

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
    let mut source = String::from(
        r#"interface EventRecord {
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
"#,
    );
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
fn source_cold_measurement_scope_clears_after_unwind() {
    assert_eq!(query_source_cold_measure(), None);
    let unwind = std::panic::catch_unwind(|| {
        let _guard = start_query_source_cold_measure();
        assert_eq!(
            query_source_cold_measure(),
            Some(QuerySourceColdMeasure::default())
        );
        panic!("measurement unwind probe");
    });
    assert!(unwind.is_err());
    assert_eq!(query_source_cold_measure(), None);

    let guard = start_query_source_cold_measure();
    assert_eq!(
        query_source_cold_measure(),
        Some(QuerySourceColdMeasure::default())
    );
    drop(guard);
    assert_eq!(query_source_cold_measure(), None);
}

fn assert_accounting(measure: QuerySourceColdMeasure) {
    assert!(measure.publication_calls > 0, "{measure:?}");
    assert_eq!(
        measure.publication_query_roots,
        2 * measure.publication_calls,
        "{measure:?}"
    );
    assert!(measure.planner_transactions > 0, "{measure:?}");
    assert!(
        measure.exhaustion_frontiers <= measure.planner_transactions,
        "{measure:?}"
    );
}

#[test]
#[ignore = "RED: repeated publication scans still exceed the reachable-graph bound"]
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
#[ignore = "RED: planner transactions still copy durable evaluation memo seeds"]
fn planner_transactions_borrow_durable_memos_without_seed_copies() {
    for order in QueryOrder::ALL {
        let (run, measure) = check_cold_measured(LARGE_TARGETS, order);
        assert_semantics(&run, LARGE_TARGETS, order);
        assert_accounting(measure);
        assert_eq!(measure.durable_memo_seed_copy_entries, 0, "{:#?}", measure);
    }
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

//! RED contract for demand-local indexed access in DOM-style listener heritage.

use super::{query_source_cold_measure, start_query_source_cold_measure, QuerySourceColdMeasure};
use crate::check::checker::check_program;
use crate::driver::CheckOutput;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

const SMALL_MAP: usize = 16;
const LARGE_MAP: usize = 64;
const SMALL_FANOUT: usize = 8;
const LARGE_FANOUT: usize = 32;

#[derive(Clone, Copy)]
enum ListenerKey {
    Symbolic,
    Literal,
    Union,
}

struct MeasuredRun {
    output: CheckOutput,
    measure: QuerySourceColdMeasure,
}

fn check_measured(source: &str) -> MeasuredRun {
    assert_eq!(query_source_cold_measure(), None);
    let guard = start_query_source_cold_measure();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let parse_errors = parsed.diagnostics.iter().map(ToString::to_string).collect();
    let mut interner = crate::types::Interner::with_intrinsics();
    let checked = check_program(&mut interner, &parsed.program);
    let output = CheckOutput {
        diagnostics: checked.diagnostics,
        parse_errors,
        incomplete: checked.incomplete,
    };
    let measure = query_source_cold_measure().expect("measurement scope remains active");
    drop(guard);
    assert_eq!(query_source_cold_measure(), None);
    MeasuredRun { output, measure }
}

fn assert_clean(run: &MeasuredRun) {
    assert!(
        run.output.parse_errors.is_empty(),
        "parse={:?}, measure={:#?}",
        run.output.parse_errors,
        run.measure
    );
    assert!(
        run.output.diagnostics.is_empty(),
        "diagnostics={:?}, measure={:#?}",
        run.output.diagnostics,
        run.measure
    );
    assert!(
        run.output.incomplete.is_empty(),
        "incomplete={:?}, measure={:#?}",
        run.output.incomplete,
        run.measure
    );
}

fn append_targets(source: &mut String, fanout: usize, left: &str, right: &str) {
    for target in 0..fanout {
        source.push_str(&format!(
            "interface Target{target:03} extends {left}, {right} {{}}\n"
        ));
    }
}

fn simple_heritage_source(fanout: usize) -> String {
    let mut source = String::from(
        "interface LeftControl { readonly self: LeftControl; readonly stable: string; }\n\
         interface RightControl { readonly self: RightControl; readonly stable: string; }\n",
    );
    append_targets(&mut source, fanout, "LeftControl", "RightControl");
    source
}

fn listener_source(
    map_size: usize,
    fanout: usize,
    key: ListenerKey,
    toxic_irrelevant_payload: bool,
) -> String {
    assert!(map_size >= 2);
    let mut source = String::from(
        "interface SelectedPayload { readonly selected: string; }\n\
         interface OtherPayload { readonly other: number; }\n",
    );
    if toxic_irrelevant_payload {
        source.push_str("class Toxic<T> { next: Toxic<Toxic<T>>; }\n");
    }
    for payload in 2..map_size {
        source.push_str(&format!(
            "interface Payload{payload:03} {{ readonly value{payload:03}: string; }}\n"
        ));
    }
    source.push_str(
        "interface EventMap {\n\
           selected: SelectedPayload;\n\
           other: OtherPayload;\n",
    );
    for payload in 2..map_size {
        let ty = if toxic_irrelevant_payload && payload + 1 == map_size {
            "Toxic<string>".to_string()
        } else {
            format!("Payload{payload:03}")
        };
        source.push_str(&format!("  event{payload:03}: {ty};\n"));
    }
    source.push_str("}\n");

    let constraint = match key {
        ListenerKey::Symbolic => "keyof EventMap",
        ListenerKey::Literal => "\"selected\"",
        ListenerKey::Union => "\"selected\" | \"other\"",
    };
    for side in ["LeftListeners", "RightListeners"] {
        source.push_str(&format!(
            "interface {side} {{\n\
               addEventListener<K extends {constraint}>(type: K, listener: (event: EventMap[K]) => void): void;\n\
             }}\n"
        ));
    }
    append_targets(&mut source, fanout, "LeftListeners", "RightListeners");
    source
}

#[test]
fn simple_heritage_fanout_is_a_cheap_control() {
    let small = check_measured(&simple_heritage_source(SMALL_FANOUT));
    let large = check_measured(&simple_heritage_source(LARGE_FANOUT));
    assert_clean(&small);
    assert_clean(&large);

    for (fanout, run) in [(SMALL_FANOUT, &small), (LARGE_FANOUT, &large)] {
        assert_eq!(
            run.measure.planner_tainted_finishes, 0,
            "{:#?}",
            run.measure
        );
        assert_eq!(run.measure.exhaustion_frontiers, 0, "{:#?}", run.measure);
        assert!(
            run.measure.planner_visits <= 4,
            "plain recursive heritage must not need graph-wide planning: {:#?}",
            run.measure
        );
        assert_eq!(
            run.measure.durable_identity_yes_hits,
            u64::try_from(fanout - 1).expect("fanout fits u64"),
            "{:#?}",
            run.measure
        );
    }
    assert_eq!(
        large.measure.planner_visits, small.measure.planner_visits,
        "plain heritage work must be independent of repeated consumers"
    );
}

#[test]
fn symbolic_event_map_index_does_not_scan_unrelated_payload_values() {
    const UNRELATED_PAYLOAD_VISIT_ALLOWANCE: u64 = 16;

    let small = check_measured(&listener_source(
        SMALL_MAP,
        SMALL_FANOUT,
        ListenerKey::Symbolic,
        false,
    ));
    let large = check_measured(&listener_source(
        LARGE_MAP,
        SMALL_FANOUT,
        ListenerKey::Symbolic,
        false,
    ));
    assert_clean(&small);
    assert_clean(&large);
    assert_eq!(
        small.measure.planner_tainted_finishes, 0,
        "{:#?}",
        small.measure
    );
    assert_eq!(
        large.measure.planner_tainted_finishes, 0,
        "{:#?}",
        large.measure
    );
    assert_eq!(
        small.measure.exhaustion_frontiers, 0,
        "{:#?}",
        small.measure
    );
    assert_eq!(
        large.measure.exhaustion_frontiers, 0,
        "{:#?}",
        large.measure
    );
    assert!(
        large.measure.planner_visits
            <= small
                .measure
                .planner_visits
                .saturating_add(UNRELATED_PAYLOAD_VISIT_ALLOWANCE),
        "quadrupling unrelated EventMap payloads must not make an alpha-aligned EventMap[K] relation scan them: small={:#?}, large={:#?}",
        small.measure,
        large.measure
    );
}

#[test]
fn literal_and_union_event_keys_only_demand_selected_payloads() {
    const FIXED_KEY_VISIT_ALLOWANCE: u64 = 2;

    for key in [ListenerKey::Literal, ListenerKey::Union] {
        let small = check_measured(&listener_source(SMALL_MAP, SMALL_FANOUT, key, false));
        let large = check_measured(&listener_source(LARGE_MAP, SMALL_FANOUT, key, false));
        assert_clean(&small);
        assert_clean(&large);
        for run in [&small, &large] {
            assert_eq!(
                run.measure.planner_tainted_finishes, 0,
                "{:#?}",
                run.measure
            );
            assert_eq!(run.measure.exhaustion_frontiers, 0, "{:#?}", run.measure);
        }
        assert!(
            large.measure.planner_visits
                <= small
                    .measure
                    .planner_visits
                    .saturating_add(FIXED_KEY_VISIT_ALLOWANCE),
            "fixed keys must not demand unrelated EventMap payloads: small={:#?}, large={:#?}",
            small.measure,
            large.measure
        );
    }
}

#[test]
fn irrelevant_cyclic_event_payload_cannot_taint_clean_listener_heritage() {
    let run = check_measured(&listener_source(
        SMALL_MAP,
        SMALL_FANOUT,
        ListenerKey::Symbolic,
        true,
    ));
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.diagnostics.is_empty(),
        "{:?}",
        run.output.diagnostics
    );
    assert_eq!(
        run.measure.planner_tainted_finishes, 0,
        "{:#?}",
        run.measure
    );
    assert_eq!(run.measure.exhaustion_frontiers, 0, "{:#?}", run.measure);
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(
        run.measure.durable_identity_yes_inserts, 1,
        "{:#?}",
        run.measure
    );
    assert_eq!(
        run.measure.durable_identity_yes_hits,
        u64::try_from(SMALL_FANOUT - 1).expect("fanout fits u64"),
        "{:#?}",
        run.measure
    );
}

#[test]
fn repeated_identical_clean_listener_heritage_reuses_one_result() {
    let small = check_measured(&listener_source(
        LARGE_MAP,
        SMALL_FANOUT,
        ListenerKey::Symbolic,
        false,
    ));
    let large = check_measured(&listener_source(
        LARGE_MAP,
        LARGE_FANOUT,
        ListenerKey::Symbolic,
        false,
    ));
    assert_clean(&small);
    assert_clean(&large);

    for (fanout, run) in [(SMALL_FANOUT, &small), (LARGE_FANOUT, &large)] {
        assert_eq!(
            run.measure.durable_identity_yes_inserts, 1,
            "{:#?}",
            run.measure
        );
        assert_eq!(
            run.measure.durable_identity_yes_hits,
            u64::try_from(fanout - 1).expect("fanout fits u64"),
            "{:#?}",
            run.measure
        );
        assert_eq!(
            run.measure.planner_tainted_finishes, 0,
            "{:#?}",
            run.measure
        );
        assert_eq!(run.measure.exhaustion_frontiers, 0, "{:#?}", run.measure);
    }
    assert_eq!(
        large.measure.planner_visits, small.measure.planner_visits,
        "repeated clean consumers must reuse the first completed relation"
    );
}

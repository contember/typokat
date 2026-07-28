//! RED contract for DOM-style generic listener heritage over widening event maps.

use super::{query_source_cold_measure, start_query_source_cold_measure, QuerySourceColdMeasure};
use crate::check::checker::check_program;
use crate::check::test_support::CheckOutput;
use crate::diagnostics::DiagnosticCode;
use crate::relate::relation::{relation_source_cold_measure, RelationSourceColdMeasure};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

const SMALL_WIDTH: usize = 32;
const LARGE_WIDTH: usize = 64;

#[derive(Clone, Copy, Debug)]
struct Work {
    query: QuerySourceColdMeasure,
    relation: RelationSourceColdMeasure,
}

struct MeasuredRun {
    source: String,
    output: CheckOutput,
    work: Work,
}

fn check_measured(source: String) -> MeasuredRun {
    assert_eq!(query_source_cold_measure(), None);
    assert_eq!(relation_source_cold_measure(), None);
    let guard = start_query_source_cold_measure();
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
    let work = Work {
        query: query_source_cold_measure().expect("query measurement remains active"),
        relation: relation_source_cold_measure().expect("relation measurement remains active"),
    };
    drop(guard);
    assert_eq!(query_source_cold_measure(), None);
    assert_eq!(relation_source_cold_measure(), None);
    MeasuredRun {
        source,
        output,
        work,
    }
}

fn recursive_listener_source(width: usize) -> String {
    let mut source = String::from(
        "interface EventRecord {\n\
           readonly target: EventTarget;\n\
           readonly currentTarget: EventTarget;\n\
         }\n\
         interface BaseEventMap {\n",
    );
    for event in 0..width {
        source.push_str(&format!("  e{event:03}: EventRecord;\n"));
    }
    source.push_str("}\ninterface ExtraEventMap {\n");
    for event in 0..width {
        source.push_str(&format!("  x{event:03}: EventRecord;\n"));
    }
    source.push_str(
        r#"}
interface DerivedEventMap extends BaseEventMap, ExtraEventMap {}
interface EventTarget {
  addEventListener<K extends keyof BaseEventMap>(type: K, listener: (this: EventTarget, event: BaseEventMap[K]) => void): void;
  addEventListener(type: string, listener: (this: EventTarget, event: EventRecord) => void): void;
  removeEventListener<K extends keyof BaseEventMap>(type: K, listener: (this: EventTarget, event: BaseEventMap[K]) => void): void;
  removeEventListener(type: string, listener: (this: EventTarget, event: EventRecord) => void): void;
}
interface Base extends EventTarget {
  addEventListener<K extends keyof BaseEventMap>(type: K, listener: (this: Base, event: BaseEventMap[K]) => void): void;
  addEventListener(type: string, listener: (this: Base, event: EventRecord) => void): void;
  removeEventListener<K extends keyof BaseEventMap>(type: K, listener: (this: Base, event: BaseEventMap[K]) => void): void;
  removeEventListener(type: string, listener: (this: Base, event: EventRecord) => void): void;
}
interface Derived extends Base {
  addEventListener<K extends keyof DerivedEventMap>(type: K, listener: (this: Derived, event: DerivedEventMap[K]) => void): void;
  addEventListener(type: string, listener: (this: Derived, event: EventRecord) => void): void;
  removeEventListener<K extends keyof DerivedEventMap>(type: K, listener: (this: Derived, event: DerivedEventMap[K]) => void): void;
  removeEventListener(type: string, listener: (this: Derived, event: EventRecord) => void): void;
}
"#,
    );
    source
}

fn assert_listener_semantics(run: &MeasuredRun) {
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
    assert_eq!(
        run.output.diagnostics.len(),
        2,
        "{:#?}",
        run.output.diagnostics
    );
    for diagnostic in &run.output.diagnostics {
        assert_eq!(diagnostic.code, DiagnosticCode::TK2430);
        assert_eq!(
            diagnostic.message,
            "Interface 'Derived' incorrectly extends interface 'Base'."
        );
        assert_eq!(&run.source[diagnostic.span.range()], "Derived");
    }
}

#[test]
fn recursive_listener_union_superset_relation_work_is_near_linear() {
    let small = check_measured(recursive_listener_source(SMALL_WIDTH));
    let large = check_measured(recursive_listener_source(LARGE_WIDTH));
    assert_listener_semantics(&small);
    assert_listener_semantics(&large);

    assert!(
        large.work.query.planner_visits <= 2 * small.work.query.planner_visits + 16,
        "planner growth must stay near-linear: small={:#?}, large={:#?}",
        small.work,
        large.work
    );
    assert!(
        large.work.relation.uncached_relation_frames
            <= 2 * small.work.relation.uncached_relation_frames + 1_024,
        "exact union members must not be rediscovered quadratically: small={:#?}, large={:#?}",
        small.work,
        large.work
    );
}

#[test]
fn union_exact_member_prefilter_retains_structural_fallbacks() {
    let run = check_measured(
        r#"
type Source = "exact" | { value: string };
type Target = "exact" | { value: string; optional?: number };
declare const source: Source;
const target: Target = source;
"#
        .to_owned(),
    );
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
    assert!(
        run.output.diagnostics.is_empty(),
        "a non-identical member still needs general structural relation: {:#?}",
        run.output.diagnostics
    );
}

#[test]
fn union_exact_member_prefilter_preserves_first_failure_reason() {
    let run = check_measured(
        r#"
type Source = "exact" | { first: number } | { second: number };
type Target = "exact" | { first: string } | { second: string };
declare const source: Source;
const target: Target = source;
"#
        .to_owned(),
    );
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
    assert_eq!(
        run.output.diagnostics.len(),
        1,
        "{:#?}",
        run.output.diagnostics
    );
    let diagnostic = &run.output.diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::TK2322);
    assert_eq!(&run.source[diagnostic.span.range()], "target");
    assert!(
        diagnostic.message.starts_with("Type '{ first: number }'"),
        "the first failing canonical source member must remain the headline: {diagnostic:#?}"
    );
}

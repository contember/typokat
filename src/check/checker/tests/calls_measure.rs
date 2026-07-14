use super::super::calls::{
    call_measure, contextual_measure_phase, measure_contextual_rewalk, reset_call_measure,
    with_contextual_measure_phase, CallMeasure, ContextualMeasurePhase,
};
use super::super::check_program;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn measure_with_diagnostics(source: &str) -> (CallMeasure, Vec<(String, String)>) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();
    reset_call_measure();
    let result = check_program(&mut interner, &parsed.program);
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
    let diagnostics = result
        .diagnostics
        .into_iter()
        .map(|diagnostic| (diagnostic.code.as_str().to_string(), diagnostic.message))
        .collect();
    (call_measure(), diagnostics)
}

fn measure(source: &str) -> CallMeasure {
    let (measure, diagnostics) = measure_with_diagnostics(source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    measure
}

#[test]
fn contextual_measure_phase_restores_after_caught_nested_panic() {
    reset_call_measure();
    with_contextual_measure_phase(ContextualMeasurePhase::CandidateInference, || {
        let result = std::panic::catch_unwind(|| {
            with_contextual_measure_phase(ContextualMeasurePhase::CandidateTrial, || {
                panic!("test-only phase restoration");
            });
        });
        assert!(result.is_err());
        assert_eq!(
            contextual_measure_phase(),
            ContextualMeasurePhase::CandidateInference
        );
        measure_contextual_rewalk(contextual_measure_phase(), true);
    });
    assert_eq!(contextual_measure_phase(), ContextualMeasurePhase::Other);
    measure_contextual_rewalk(contextual_measure_phase(), true);
    assert_eq!(call_measure().callback_rewalks, [1, 0, 0, 0, 1]);
}

#[test]
fn measure_call_pipeline_callback_formula() {
    let source = r#"
        declare function fan<T = unknown>(tag: "ok", cb: (x: string) => void): void;
        declare function fan<T = unknown>(tag: "ok", cb: (x: number) => void): void;
        fan("ok", (x: number) => {});
    "#;
    let measure = measure(source);
    assert_eq!(measure.raw_call_argument_walks, 2);
    assert_eq!(measure.speculative_candidate_builds, 2);
    assert_eq!(measure.committed_candidate_builds, 1);
    assert_eq!(measure.candidate_trials, 2);
    assert_eq!(measure.candidate_matches, 1);
    assert_eq!(measure.candidate_mismatches, 1);
    assert_eq!(measure.generic_preliminary_inference_runs, 3);
    assert_eq!(measure.generic_full_inference_runs, 3);
    assert_eq!(measure.callback_rewalks, [3, 2, 1, 0, 0]);
}

#[test]
fn measure_call_pipeline_fresh_literal_formula() {
    let measure = measure(
        r#"
        declare function shape<T = unknown>(value: { tag: "a" }): void;
        declare function shape<T = unknown>(value: { tag: "b" }): void;
        shape({ tag: "b" });
    "#,
    );
    assert_eq!(measure.raw_call_argument_walks, 1);
    assert_eq!(measure.speculative_candidate_builds, 2);
    assert_eq!(measure.committed_candidate_builds, 1);
    assert_eq!(measure.candidate_trials, 2);
    assert_eq!(measure.candidate_matches, 1);
    assert_eq!(measure.candidate_mismatches, 1);
    assert_eq!(measure.generic_preliminary_inference_runs, 0);
    assert_eq!(measure.generic_full_inference_runs, 3);
    assert_eq!(measure.fresh_literal_rewalks, [3, 2, 1, 0, 0]);
}

#[test]
fn measure_call_pipeline_explicit_constraint_rollback_formula() {
    let measure = measure(
        r#"
        declare function exact<T extends string>(value: T): void;
        declare function exact<T extends number>(value: T): void;
        exact<number>(1);
    "#,
    );
    assert_eq!(measure.raw_call_argument_walks, 1);
    assert_eq!(measure.speculative_candidate_builds, 2);
    assert_eq!(measure.committed_candidate_builds, 1);
    assert_eq!(measure.candidate_trials, 1);
    assert_eq!(measure.candidate_matches, 1);
    assert_eq!(measure.speculative_diagnostic_rollback_events, 1);
    assert_eq!(measure.speculative_diagnostics_removed, 1);
}

#[test]
fn measure_call_pipeline_receiver_formula() {
    let measure = measure(
        r#"
        interface Api {
            tag: "ok";
            f<T = unknown>(this: { tag: "bad" }, cb: (x: number) => void): void;
            f<T = unknown>(this: { tag: "ok" }, cb: (x: number) => void): void;
        }
        declare const api: Api;
        api.f((x: number) => {});
    "#,
    );
    assert_eq!(measure.trial_receiver_relation_queries, 2);
    assert_eq!(measure.selected_receiver_relation_queries, 1);
    assert_eq!(measure.candidate_trials, 2);
    assert_eq!(measure.candidate_matches, 1);
    assert_eq!(measure.candidate_mismatches, 1);
}

#[test]
fn rejected_class_overload_candidate_discards_query_writes() {
    let measure = measure(
        r#"
        class A { x: number; }
        declare function choose(value: { x: string }): string;
        declare function choose(value: A): number;
        declare const value: A;
        const selected: number = choose(value);
    "#,
    );
    assert_eq!(measure.speculative_query_forks, 4);
    assert!(measure.speculative_query_writes_discarded > 0);
}

#[test]
fn measure_construct_pipeline_callback_formula() {
    let measure = measure(
        r#"
        declare const C: {
            new <T = unknown>(cb: (x: string) => void): number;
            new <T = unknown>(cb: (x: number) => void): number;
        };
        new C((x: number) => {});
    "#,
    );
    assert_eq!(measure.raw_construct_argument_walks, 1);
    assert_eq!(measure.speculative_candidate_builds, 2);
    assert_eq!(measure.committed_candidate_builds, 1);
    assert_eq!(measure.candidate_trials, 2);
    assert_eq!(measure.generic_preliminary_inference_runs, 3);
    assert_eq!(measure.callback_rewalks, [3, 2, 1, 0, 0]);
    assert_eq!(measure.trial_receiver_relation_queries, 0);
    assert_eq!(measure.selected_receiver_relation_queries, 0);
}

#[test]
fn measure_call_and_construct_constraint_failure_precedence() {
    let (measure, diagnostics) = measure_with_diagnostics(
        r#"
        interface CallSelector {
            <T extends string>(value: T): void;
            <T extends boolean>(value: T, extra: number): void;
        }
        declare const callSelector: CallSelector;
        callSelector<boolean>(true);

        interface ConstructSelector {
            new <T extends string>(value: T): { kind: "constraint" };
            new <T extends boolean>(value: T, extra: number): { kind: "arity" };
        }
        declare const ConstructSelector: ConstructSelector;
        new ConstructSelector<boolean>(true);

        interface ConstraintOrderCall {
            <T extends "first">(value: T): "first";
            <T extends number>(value: T): "last";
        }
        declare const constraintOrderCall: ConstraintOrderCall;
        constraintOrderCall<boolean>(true);
    "#,
    );

    assert_eq!(
        diagnostics,
        vec![
            (
                "TK2344".to_string(),
                "Type 'boolean' does not satisfy the constraint 'string'.".to_string(),
            ),
            (
                "TK2344".to_string(),
                "Type 'boolean' does not satisfy the constraint 'string'.".to_string(),
            ),
            (
                "TK2344".to_string(),
                "Type 'boolean' does not satisfy the constraint '\"first\"'.".to_string(),
            ),
        ]
    );
    assert_eq!(
        measure,
        CallMeasure {
            raw_call_argument_walks: 2,
            raw_construct_argument_walks: 1,
            speculative_candidate_builds: 6,
            committed_candidate_builds: 0,
            candidate_trials: 2,
            candidate_matches: 0,
            candidate_mismatches: 0,
            candidate_arity_failures: 2,
            generic_preliminary_inference_runs: 0,
            generic_full_inference_runs: 0,
            callback_rewalks: [0; 5],
            fresh_literal_rewalks: [0; 5],
            speculative_diagnostic_rollback_events: 4,
            speculative_diagnostics_removed: 4,
            trial_receiver_relation_queries: 0,
            selected_receiver_relation_queries: 0,
            speculative_query_forks: 8,
            speculative_query_writes_discarded: 4,
        }
    );
}

fn callback_corpus(calls: usize) -> String {
    let mut source = String::new();
    for index in 0..9 {
        source.push_str(&format!(
            "declare function fan<T = unknown>(tag: \"ok\", cb: (x: {{ tag: \"{index}\" }}) => void): void;\n"
        ));
    }
    for _ in 0..calls {
        source.push_str("fan(\"ok\", (x: { tag: \"8\" }) => {});\n");
    }
    source
}

#[test]
#[ignore = "WU4c scaled counter corpus; run explicitly with --ignored"]
fn measure_call_pipeline_scaled_callback_corpus() {
    for calls in [500, 5_000] {
        let measure = measure(&callback_corpus(calls));
        assert_eq!(
            measure.callback_rewalks,
            [10 * calls as u64, 9 * calls as u64, calls as u64, 0, 0]
        );
        assert_eq!(
            measure.callback_rewalks.iter().sum::<u64>(),
            20 * calls as u64
        );
    }
}

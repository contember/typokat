use super::super::calls::{
    call_measure, contextual_measure_phase, measure_contextual_rewalk, reset_call_measure,
    with_contextual_measure_phase, CallMeasure, ContextualMeasurePhase,
};
use super::super::check_program;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn measure(source: &str) -> CallMeasure {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();
    reset_call_measure();
    let result = check_program(&mut interner, &parsed.program);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
    call_measure()
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

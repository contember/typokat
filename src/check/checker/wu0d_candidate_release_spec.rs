//! Disabled RED release contract for WU0D Candidate B.
//!
//! Activate only with `#[cfg(test)] mod wu0d_candidate_release_spec;` in
//! `checker/mod.rs`. The implementation is test-only and default-off. It extends
//! neither the normal release binary nor ordinary test output.
//!
//! ## One-process emitter and external coordinator
//!
//! Build the cfg(test) release libtest once with:
//!
//! ```text
//! cargo test --release --lib --no-run
//! ```
//!
//! The external coordinator locates that exact prebuilt libtest and invokes one of
//! three hardwired ignored tests with libtest `--ignored --exact ... --nocapture`:
//!
//! - `check::checker::wu0d_candidate_release::wu0d_candidate_primary_probe_once`
//! - `check::checker::wu0d_candidate_release::wu0d_candidate_non_cycle_probe_once`
//! - `check::checker::wu0d_candidate_release::wu0d_candidate_reporter_control_probe_once`
//!
//! It launches fresh processes in the pinned A/B order and owns PID/freshness,
//! launch order, the exact probe filter, wall time, peak RSS, exit status,
//! prebuilt-binary digest, host digest, and workload-profile digest. None of those
//! facts is trusted from child stdout.
//!
//! Each test invokes one concrete embedded workload; no environment value selects or
//! relabels it. The primary probe is the one-process emitter around WU0B's existing
//! `run_injected_profile`. The non-cycle probe executes its embedded acyclic relation
//! workload, and the reporter-control probe executes its embedded reporting-only
//! workload. Each resolves the closed environment, starts baseline counters and
//! optionally the Candidate-B cache before the Pass is built, runs its fixed workload,
//! asks WU0B for its canonical semantic identity, and emits exactly one
//! `typokat-wu0d-candidate-v1` summary carrying the canonical workload name. Baseline
//! has no WU0D environment; Candidate B differs only by
//! `TYPOKAT_WU0D_CANDIDATE=candidate-b-v1`.
//!
//! The environment grammar is closed: absence means Off, that exact ASCII value
//! means Candidate B, and empty, alternate, duplicate, non-UTF-8, or any other
//! `TYPOKAT_WU0D_*` key is an error. There is no implicit profile, path, trace, or
//! tuning switch.
//!
//! ## Causal counters
//!
//! Every summary obeys checked arithmetic:
//!
//! - Off has `tainted_cache_hits = 0`, `tainted_cache_entries = 0`, and
//!   `avoided_visits = 0`.
//! - `eligible_requests = executed_runs + tainted_cache_hits`.
//! - `executed_runs = clean_outcomes + tainted_outcomes`.
//! - `executed_visits = memo_hits + expanded_visits`.
//! - avoided runs are not an emitted independent claim; they are exactly
//!   `tainted_cache_hits`.
//! - each inserted tainted entry retains its completed first-run visit weight;
//!   `avoided_visits` is the checked sum of that weight on later hits.
//!
//! For every A/B pair, eligible requests, clean outcomes, and the clean eager-cache
//! metrics match exactly; baseline tainted outcomes equal candidate tainted outcomes
//! plus candidate tainted-cache hits; and baseline executed visits equal candidate
//! executed visits plus candidate avoided visits. Overflow or saturation rejects.
//!
//! ## Semantic and timing evidence
//!
//! WU0B owns the canonical serializer for ordered diagnostics, ordered incomplete
//! surfaces, the ordered library ledger, and the frozen library product. The child
//! emits the byte length and SHA-256 digest of each component plus an aggregate
//! SHA-256 over `typokat-wu0d-semantic-v1` followed by each component in that order
//! as `u64be(byte_len) || bytes`. A/B compares every component identity and the
//! aggregate; timing and counters are excluded.
//!
//! Primary evidence is five fresh interleaved pairs from one prebuilt release libtest
//! on one host and one frozen profile: A1,B1,B2,A2,A3,B3,B4,A4,A5,B5. Every process
//! must finish in at most 5.00 seconds and peak at most 512 MiB. Median per-pair wall
//! improvement must be at least 20%; aggregate avoided runs and visits must each be
//! at least 80% of their baseline totals.
//!
//! The exact named controls are `non-cycle` and `reporter-control`; omissions,
//! duplicates, and unknown controls reject. The evaluator hardwires each evidence set
//! to its exact probe filter, external profile identity, and child workload identity;
//! changing labels cannot turn one workload into another. Each control independently
//! supplies five fresh interleaved A/B pairs with the same external identity checks and
//! semantic parity, and its own median regression must be at most 2%.
//!
//! The evaluator consumes externally captured raw stdout, not a pre-parsed trusted
//! summary. Missing, duplicate, malformed, noncanonical, or saturated summaries are
//! evidence failures. Libtest's unrelated harness lines are ignored; any line with
//! the WU0D prefix is parsed strictly, and exactly one such line must exist.

use super::wu0b_library::{
    canonical_wu0d_semantic_identity_from_components_for_test, Wu0dSemanticComponents,
    Wu0dSemanticIdentity,
};
use super::wu0d_candidate_release::{
    evaluate_candidate_b_release, parse_candidate_stdout,
    resolve_candidate_environment_bytes_for_test, resolve_candidate_environment_for_test,
    CacheMetrics, CandidateEnvironment, CandidateSummary, CandidateWorkload, ControlEvidence,
    GateDecision, NoGoReason, PairedReleaseRun, ProcessObservation,
};

const ENABLE_KEY: &str = "TYPOKAT_WU0D_CANDIDATE";
const ENABLE_VALUE: &str = "candidate-b-v1";
const BINARY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const HOST: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const PRIMARY_PROFILE: &str = "3333333333333333333333333333333333333333333333333333333333333333";
const NON_CYCLE_PROFILE: &str = "4444444444444444444444444444444444444444444444444444444444444444";
const REPORTER_CONTROL_PROFILE: &str =
    "5555555555555555555555555555555555555555555555555555555555555555";
const SEMANTIC: &str = "740c77fa0237ef4bde7f5b0031d3d0a7c977a39d6fd75325148a636c0595f08d";
const NON_CYCLE: &str = "non-cycle";
const REPORTER_CONTROL: &str = "reporter-control";
const PRIMARY_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_primary_probe_once";
const NON_CYCLE_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_non_cycle_probe_once";
const REPORTER_CONTROL_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_reporter_control_probe_once";
const MIB: u64 = 1_024 * 1_024;

fn environment(entries: &[(&str, &str)]) -> Result<CandidateEnvironment, String> {
    resolve_candidate_environment_for_test(entries.iter().copied())
        .map_err(|error| error.to_string())
}

fn semantic_components() -> Wu0dSemanticComponents {
    Wu0dSemanticComponents {
        diagnostics: b"TK2322|7|9|Type 'string' is not assignable".to_vec(),
        incomplete: b"library.generic.constraint|11|13".to_vec(),
        library_ledger: b"0|17|0|diagnostic\n1|19|0|incomplete".to_vec(),
        frozen_library_product: b"typokat-library-product-v1\0stable".to_vec(),
    }
}

fn semantic_identity() -> Wu0dSemanticIdentity {
    canonical_wu0d_semantic_identity_from_components_for_test(&semantic_components())
}

fn summary(candidate: bool) -> CandidateSummary {
    CandidateSummary {
        workload: CandidateWorkload::Primary,
        mode: if candidate {
            CandidateEnvironment::CandidateB
        } else {
            CandidateEnvironment::Off
        },
        eligible_requests: 100,
        executed_runs: if candidate { 20 } else { 100 },
        tainted_cache_hits: if candidate { 80 } else { 0 },
        clean_outcomes: 10,
        tainted_outcomes: if candidate { 10 } else { 90 },
        executed_visits: if candidate { 200 } else { 1_000 },
        memo_hits: if candidate { 60 } else { 300 },
        expanded_visits: if candidate { 140 } else { 700 },
        avoided_visits: if candidate { 800 } else { 0 },
        tainted_cache_entries: if candidate { 3 } else { 0 },
        clean_cache: CacheMetrics {
            hits: 71,
            misses: 29,
            entries: 23,
        },
        semantic: semantic_identity(),
        saturated: false,
    }
}

fn control_summary(candidate: bool, workload: CandidateWorkload) -> CandidateSummary {
    CandidateSummary {
        workload,
        mode: if candidate {
            CandidateEnvironment::CandidateB
        } else {
            CandidateEnvironment::Off
        },
        eligible_requests: 100,
        executed_runs: 100,
        tainted_cache_hits: 0,
        clean_outcomes: 100,
        tainted_outcomes: 0,
        executed_visits: 1_000,
        memo_hits: 300,
        expanded_visits: 700,
        avoided_visits: 0,
        tainted_cache_entries: 0,
        clean_cache: CacheMetrics {
            hits: 71,
            misses: 29,
            entries: 23,
        },
        semantic: semantic_identity(),
        saturated: false,
    }
}

fn captured_stdout(summary: &CandidateSummary, probe_filter: &str) -> Vec<u8> {
    format!(
        "running 1 test\n{}\ntest {probe_filter} ... ok\n",
        summary.render(),
    )
    .into_bytes()
}

fn process(
    set: &str,
    sequence: u8,
    launch_ordinal: u8,
    candidate: bool,
    workload: CandidateWorkload,
    probe_filter: &str,
    profile: &str,
    elapsed_us: u64,
) -> ProcessObservation {
    let summary = match workload {
        CandidateWorkload::Primary => summary(candidate),
        CandidateWorkload::NonCycle | CandidateWorkload::ReporterControl => {
            control_summary(candidate, workload)
        }
    };
    ProcessObservation {
        process_identity: format!("{set}-fresh-{sequence}"),
        launch_ordinal,
        probe_filter: probe_filter.into(),
        binary_identity: BINARY.into(),
        host_identity: HOST.into(),
        profile_identity: profile.into(),
        warm_filesystem_cache: true,
        release_libtest: true,
        exit_code: 0,
        elapsed_us,
        peak_rss_bytes: 400 * MIB,
        captured_stdout: captured_stdout(&summary, probe_filter),
    }
}

fn pair(
    set: &str,
    workload: CandidateWorkload,
    probe_filter: &str,
    profile: &str,
    index: u8,
    baseline_us: u64,
    candidate_us: u64,
) -> PairedReleaseRun {
    let (baseline_launch, candidate_launch) = match index {
        1 => (1, 2),
        2 => (4, 3),
        3 => (5, 6),
        4 => (8, 7),
        5 => (9, 10),
        _ => panic!("the contract defines exactly five pairs"),
    };
    PairedReleaseRun {
        pair: index,
        baseline: process(
            set,
            index * 2,
            baseline_launch,
            false,
            workload,
            probe_filter,
            profile,
            baseline_us,
        ),
        candidate: process(
            set,
            index * 2 + 1,
            candidate_launch,
            true,
            workload,
            probe_filter,
            profile,
            candidate_us,
        ),
    }
}

fn passing_pairs() -> Vec<PairedReleaseRun> {
    vec![
        pair(
            "primary",
            CandidateWorkload::Primary,
            PRIMARY_PROBE,
            PRIMARY_PROFILE,
            1,
            4_000_000,
            3_000_000,
        ),
        pair(
            "primary",
            CandidateWorkload::Primary,
            PRIMARY_PROBE,
            PRIMARY_PROFILE,
            2,
            4_100_000,
            3_200_000,
        ),
        pair(
            "primary",
            CandidateWorkload::Primary,
            PRIMARY_PROBE,
            PRIMARY_PROFILE,
            3,
            3_900_000,
            3_000_000,
        ),
        pair(
            "primary",
            CandidateWorkload::Primary,
            PRIMARY_PROBE,
            PRIMARY_PROFILE,
            4,
            4_200_000,
            3_300_000,
        ),
        pair(
            "primary",
            CandidateWorkload::Primary,
            PRIMARY_PROBE,
            PRIMARY_PROFILE,
            5,
            4_000_000,
            3_100_000,
        ),
    ]
}

fn control_pairs(
    name: &str,
    workload: CandidateWorkload,
    probe_filter: &str,
    profile: &str,
) -> Vec<PairedReleaseRun> {
    (1..=5)
        .map(|index| {
            pair(
                name,
                workload,
                probe_filter,
                profile,
                index,
                2_000_000,
                2_010_000,
            )
        })
        .collect()
}

fn passing_controls() -> Vec<ControlEvidence> {
    vec![
        ControlEvidence {
            name: NON_CYCLE.into(),
            pairs: control_pairs(
                NON_CYCLE,
                CandidateWorkload::NonCycle,
                NON_CYCLE_PROBE,
                NON_CYCLE_PROFILE,
            ),
        },
        ControlEvidence {
            name: REPORTER_CONTROL.into(),
            pairs: control_pairs(
                REPORTER_CONTROL,
                CandidateWorkload::ReporterControl,
                REPORTER_CONTROL_PROBE,
                REPORTER_CONTROL_PROFILE,
            ),
        },
    ]
}

fn replace_summary(process: &mut ProcessObservation, summary: CandidateSummary) {
    process.captured_stdout = captured_stdout(&summary, &process.probe_filter);
}

fn assert_reason(pairs: &[PairedReleaseRun], controls: &[ControlEvidence], expected: NoGoReason) {
    let decision = evaluate_candidate_b_release(pairs, controls);
    assert!(!decision.authorizes_candidate_b());
    assert!(
        decision.reasons().contains(&expected),
        "missing {expected:?} in {decision:?}"
    );
}

#[test]
fn environment_is_strict_default_off_and_has_one_switch() {
    assert_eq!(environment(&[]).unwrap(), CandidateEnvironment::Off);
    assert_eq!(
        environment(&[(ENABLE_KEY, ENABLE_VALUE)]).unwrap(),
        CandidateEnvironment::CandidateB,
    );
    for invalid in [
        vec![(ENABLE_KEY, "")],
        vec![(ENABLE_KEY, "1")],
        vec![(ENABLE_KEY, "candidate-b-v2")],
        vec![("TYPOKAT_WU0D_PROFILE", "1")],
        vec![(ENABLE_KEY, ENABLE_VALUE), (ENABLE_KEY, ENABLE_VALUE)],
        vec![(ENABLE_KEY, ENABLE_VALUE), ("TYPOKAT_WU0D_TRACE", "0")],
    ] {
        assert!(environment(&invalid).is_err());
    }
    assert!(
        resolve_candidate_environment_bytes_for_test(&[(ENABLE_KEY.as_bytes(), &[0xff])]).is_err()
    );
}

#[test]
fn summary_protocol_is_canonical_for_candidate_and_off() {
    for expected in [summary(false), summary(true)] {
        let line = expected.render();
        let parsed = parse_candidate_stdout(&captured_stdout(&expected, PRIMARY_PROBE)).unwrap();
        assert_eq!(parsed, expected);
        assert_eq!(parsed.render(), line);
        assert!(line.contains("workload=primary"));
    }
    for (workload, name, probe) in [
        (CandidateWorkload::NonCycle, NON_CYCLE, NON_CYCLE_PROBE),
        (
            CandidateWorkload::ReporterControl,
            REPORTER_CONTROL,
            REPORTER_CONTROL_PROBE,
        ),
    ] {
        let expected = control_summary(true, workload);
        let line = expected.render();
        assert_eq!(
            parse_candidate_stdout(&captured_stdout(&expected, probe)).unwrap(),
            expected,
        );
        assert!(line.contains(&format!("workload={name}")));
    }
    assert_eq!(summary(false).avoided_runs(), 0);
    assert_eq!(summary(true).avoided_runs(), 80);
}

#[test]
fn summary_parser_rejects_missing_duplicate_reordered_or_noncanonical_evidence() {
    let candidate = summary(true);
    let line = candidate.render();
    assert_eq!(
        parse_candidate_stdout(&captured_stdout(&candidate, PRIMARY_PROBE)).unwrap(),
        candidate
    );

    let malformed = [
        line.replace("workload=primary", "workload=other"),
        line.replace(" workload=primary", ""),
        line.replace("eligible_requests=100", "eligible_requests=99"),
        line.replace(
            "eligible_requests=100",
            "eligible_requests=18446744073709551616",
        ),
        line.replace("executed_runs=20", "executed_runs=21"),
        line.replace("executed_visits=200", "executed_visits=201"),
        line.replace("tainted_cache_hits=80", "tainted_cache_hits=080"),
        line.replace("mode=candidate-b", "mode=other"),
        line.replace(SEMANTIC, "ABC"),
        line.replace(" saturated=0", ""),
        format!("{line} eligible_requests=100"),
        format!("{line} unknown=0"),
        format!("{line} "),
    ];
    for malformed in malformed {
        assert!(parse_candidate_stdout(format!("{malformed}\n").as_bytes()).is_err());
    }

    let mut fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    fields.swap(2, 3);
    assert!(parse_candidate_stdout(format!("{}\n", fields.join(" ")).as_bytes()).is_err());
    assert!(parse_candidate_stdout(b"").is_err());
    assert!(parse_candidate_stdout(format!("{line}\n{line}\n").as_bytes()).is_err());
    assert!(
        parse_candidate_stdout(b"typokat-wu0d-candidate-v1 mode=candidate-b malformed\n").is_err()
    );

    let mut invalid_off = summary(false);
    invalid_off.executed_runs = 99;
    invalid_off.tainted_cache_hits = 1;
    invalid_off.tainted_outcomes = 89;
    assert!(parse_candidate_stdout(&captured_stdout(&invalid_off, PRIMARY_PROBE)).is_err());
}

#[test]
fn semantic_component_lengths_digests_and_aggregate_are_frozen() {
    let components = semantic_components();
    let identity = semantic_identity();
    assert_eq!(identity.aggregate_sha256, SEMANTIC);
    for (component, expected_len) in [
        (&identity.diagnostics, components.diagnostics.len()),
        (&identity.incomplete, components.incomplete.len()),
        (&identity.library_ledger, components.library_ledger.len()),
        (
            &identity.frozen_library_product,
            components.frozen_library_product.len(),
        ),
    ] {
        assert_eq!(component.byte_len, u64::try_from(expected_len).unwrap());
        assert_eq!(component.sha256.len(), 64);
        assert!(component
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    }

    for component in 0..4 {
        let mut changed = components.clone();
        match component {
            0 => changed.diagnostics.push(b'!'),
            1 => changed.incomplete.push(b'!'),
            2 => changed.library_ledger.push(b'!'),
            3 => changed.frozen_library_product.push(b'!'),
            _ => unreachable!(),
        }
        let changed = canonical_wu0d_semantic_identity_from_components_for_test(&changed);
        assert_ne!(changed, identity);
        assert_ne!(changed.aggregate_sha256, SEMANTIC);
    }
}

#[test]
fn five_primary_pairs_and_both_exact_controls_pass() {
    for pair in passing_pairs() {
        assert_eq!(pair.baseline.probe_filter, PRIMARY_PROBE);
        assert_eq!(pair.candidate.probe_filter, PRIMARY_PROBE);
        assert_eq!(pair.baseline.profile_identity, PRIMARY_PROFILE);
        assert_eq!(pair.candidate.profile_identity, PRIMARY_PROFILE);
        assert_eq!(
            parse_candidate_stdout(&pair.baseline.captured_stdout)
                .unwrap()
                .workload,
            CandidateWorkload::Primary,
        );
        assert_eq!(
            parse_candidate_stdout(&pair.candidate.captured_stdout)
                .unwrap()
                .workload,
            CandidateWorkload::Primary,
        );
    }
    for (control, expected_workload, expected_probe, expected_profile) in passing_controls()
        .into_iter()
        .zip([
            (
                CandidateWorkload::NonCycle,
                NON_CYCLE_PROBE,
                NON_CYCLE_PROFILE,
            ),
            (
                CandidateWorkload::ReporterControl,
                REPORTER_CONTROL_PROBE,
                REPORTER_CONTROL_PROFILE,
            ),
        ])
        .map(|(control, expected)| (control, expected.0, expected.1, expected.2))
    {
        for pair in control.pairs {
            assert_eq!(pair.baseline.probe_filter, expected_probe);
            assert_eq!(pair.candidate.probe_filter, expected_probe);
            assert_eq!(pair.baseline.profile_identity, expected_profile);
            assert_eq!(pair.candidate.profile_identity, expected_profile);
            let baseline = parse_candidate_stdout(&pair.baseline.captured_stdout).unwrap();
            let candidate = parse_candidate_stdout(&pair.candidate.captured_stdout).unwrap();
            assert_eq!(baseline.workload, expected_workload);
            assert_eq!(candidate.workload, expected_workload);
            assert_eq!(baseline.mode, CandidateEnvironment::Off);
            assert_eq!(candidate.mode, CandidateEnvironment::CandidateB);
            for summary in [baseline, candidate] {
                assert_eq!(summary.tainted_cache_hits, 0);
                assert_eq!(summary.tainted_cache_entries, 0);
                assert_eq!(summary.avoided_visits, 0);
                assert_eq!(summary.executed_runs, summary.eligible_requests);
            }
        }
    }
    assert_eq!(
        evaluate_candidate_b_release(&passing_pairs(), &passing_controls()),
        GateDecision::Go,
    );
}

#[test]
fn exact_deadline_and_memory_boundaries_are_admissible() {
    let mut pairs = passing_pairs();
    pairs[0].baseline.elapsed_us = 5_000_000;
    pairs[0].candidate.elapsed_us = 5_000_000;
    pairs[0].baseline.peak_rss_bytes = 512 * MIB;
    pairs[0].candidate.peak_rss_bytes = 512 * MIB;
    assert_eq!(
        evaluate_candidate_b_release(&pairs, &passing_controls()),
        GateDecision::Go,
    );
}

#[test]
fn malformed_or_saturated_raw_stdout_is_a_hard_no_go() {
    let controls = passing_controls();

    let mut missing = passing_pairs();
    missing[0].candidate.captured_stdout.clear();
    assert_reason(
        &missing,
        &controls,
        NoGoReason::SummaryMissingDuplicateMalformedOrSaturated,
    );

    let mut duplicate = passing_pairs();
    let line = summary(true).render();
    duplicate[0].candidate.captured_stdout = format!("{line}\n{line}\n").into_bytes();
    assert_reason(
        &duplicate,
        &controls,
        NoGoReason::SummaryMissingDuplicateMalformedOrSaturated,
    );

    let mut saturated = passing_pairs();
    let mut saturated_summary = summary(true);
    saturated_summary.saturated = true;
    replace_summary(&mut saturated[0].candidate, saturated_summary);
    assert_reason(
        &saturated,
        &controls,
        NoGoReason::SummaryMissingDuplicateMalformedOrSaturated,
    );
}

#[test]
fn process_pair_identity_resource_and_variant_failures_are_evaluated() {
    let controls = passing_controls();

    let mut evidence = passing_pairs();
    evidence.pop();
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::NotFiveFreshInterleavedPairs,
    );

    let mut evidence = passing_pairs();
    evidence[1].candidate.launch_ordinal = 2;
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::NotFiveFreshInterleavedPairs,
    );

    let mut evidence = passing_pairs();
    evidence[1].candidate.process_identity = evidence[0].candidate.process_identity.clone();
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::NotFiveFreshInterleavedPairs,
    );

    let mut evidence = passing_pairs();
    evidence[0].candidate.binary_identity = "9".repeat(64);
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::BinaryHostOrProfileMismatch,
    );

    let mut evidence = passing_pairs();
    evidence[0].candidate.probe_filter = NON_CYCLE_PROBE.into();
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::BinaryHostOrProfileMismatch,
    );

    let mut evidence = passing_pairs();
    evidence[0].candidate.profile_identity = NON_CYCLE_PROFILE.into();
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::BinaryHostOrProfileMismatch,
    );

    let mut evidence = passing_pairs();
    for pair in &mut evidence {
        pair.baseline.profile_identity = "6".repeat(64);
        pair.candidate.profile_identity = "6".repeat(64);
    }
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::BinaryHostOrProfileMismatch,
    );

    let mut evidence = passing_pairs();
    let mut relabeled = summary(true);
    relabeled.workload = CandidateWorkload::NonCycle;
    replace_summary(&mut evidence[0].candidate, relabeled);
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::BinaryHostOrProfileMismatch,
    );

    let mut evidence = passing_pairs();
    evidence[0].candidate.release_libtest = false;
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::NotReleaseLibtestOrWarmFilesystem,
    );

    let mut evidence = passing_pairs();
    evidence[0].candidate.exit_code = 1;
    assert_reason(&evidence, &controls, NoGoReason::ProcessFailed);

    let mut evidence = passing_pairs();
    evidence[0].candidate.elapsed_us = 5_000_001;
    assert_reason(&evidence, &controls, NoGoReason::DeadlineExceeded);

    let mut evidence = passing_pairs();
    evidence[0].candidate.peak_rss_bytes = 512 * MIB + 1;
    assert_reason(&evidence, &controls, NoGoReason::MemoryExceeded);

    let mut evidence = passing_pairs();
    replace_summary(&mut evidence[0].candidate, summary(false));
    assert_reason(&evidence, &controls, NoGoReason::VariantMismatch);
}

#[test]
fn semantic_counter_cache_and_improvement_failures_are_evaluated() {
    let controls = passing_controls();

    let mut evidence = passing_pairs();
    let mut changed = semantic_components();
    changed.frozen_library_product.push(b'!');
    let mut changed_summary = summary(true);
    changed_summary.semantic = canonical_wu0d_semantic_identity_from_components_for_test(&changed);
    replace_summary(&mut evidence[0].candidate, changed_summary);
    assert_reason(&evidence, &controls, NoGoReason::SemanticMismatch);

    let mut evidence = passing_pairs();
    let mut changed_summary = summary(true);
    changed_summary.eligible_requests = 99;
    changed_summary.executed_runs = 19;
    changed_summary.clean_outcomes = 9;
    replace_summary(&mut evidence[0].candidate, changed_summary);
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::CounterReconciliationFailed,
    );

    let mut evidence = passing_pairs();
    let mut changed_summary = summary(true);
    changed_summary.clean_cache.hits += 1;
    replace_summary(&mut evidence[0].candidate, changed_summary);
    assert_reason(&evidence, &controls, NoGoReason::CleanCacheMetricsChanged);

    let mut evidence = passing_pairs();
    for pair in &mut evidence {
        pair.candidate.elapsed_us = pair.baseline.elapsed_us;
    }
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::MedianImprovementBelowTwentyPercent,
    );

    let mut evidence = passing_pairs();
    for pair in &mut evidence {
        let mut changed_summary = summary(true);
        changed_summary.executed_runs = 21;
        changed_summary.tainted_cache_hits = 79;
        changed_summary.tainted_outcomes = 11;
        replace_summary(&mut pair.candidate, changed_summary);
    }
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::AffectedRunReductionBelowEightyPercent,
    );

    let mut evidence = passing_pairs();
    for pair in &mut evidence {
        let mut changed_summary = summary(true);
        changed_summary.executed_visits = 201;
        changed_summary.expanded_visits = 141;
        changed_summary.avoided_visits = 799;
        replace_summary(&mut pair.candidate, changed_summary);
    }
    assert_reason(
        &evidence,
        &controls,
        NoGoReason::AffectedVisitReductionBelowEightyPercent,
    );
}

#[test]
fn exact_named_control_sets_are_mandatory_and_independently_validated() {
    let pairs = passing_pairs();

    let mut controls = passing_controls();
    controls.pop();
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlsMissingDuplicateOrUnknown,
    );

    let mut controls = passing_controls();
    controls[1].name = "unknown".into();
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlsMissingDuplicateOrUnknown,
    );

    let mut controls = passing_controls();
    controls[1].name = controls[0].name.clone();
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlsMissingDuplicateOrUnknown,
    );

    let mut controls = passing_controls();
    controls[0].pairs[0].candidate.profile_identity = PRIMARY_PROFILE.into();
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlIdentityOrSemanticMismatch,
    );

    let mut controls = passing_controls();
    controls[0].pairs.pop();
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlIdentityOrSemanticMismatch,
    );

    let mut controls = passing_controls();
    controls[0].pairs[1].candidate.launch_ordinal = 2;
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlIdentityOrSemanticMismatch,
    );

    let mut controls = passing_controls();
    let mut changed_summary = control_summary(true, CandidateWorkload::NonCycle);
    let mut changed = semantic_components();
    changed.diagnostics.push(b'!');
    changed_summary.semantic = canonical_wu0d_semantic_identity_from_components_for_test(&changed);
    replace_summary(&mut controls[0].pairs[0].candidate, changed_summary);
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlIdentityOrSemanticMismatch,
    );

    let mut controls = passing_controls();
    for pair in &mut controls[0].pairs {
        pair.baseline.probe_filter = REPORTER_CONTROL_PROBE.into();
        pair.candidate.probe_filter = REPORTER_CONTROL_PROBE.into();
        pair.baseline.profile_identity = REPORTER_CONTROL_PROFILE.into();
        pair.candidate.profile_identity = REPORTER_CONTROL_PROFILE.into();
        replace_summary(
            &mut pair.baseline,
            control_summary(false, CandidateWorkload::ReporterControl),
        );
        replace_summary(
            &mut pair.candidate,
            control_summary(true, CandidateWorkload::ReporterControl),
        );
    }
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlIdentityOrSemanticMismatch,
    );

    let mut controls = passing_controls();
    for pair in &mut controls[1].pairs {
        pair.candidate.elapsed_us = 2_100_000;
    }
    assert_reason(
        &pairs,
        &controls,
        NoGoReason::ControlRegressionAboveTwoPercent,
    );
}

fn without_whitespace(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn source_section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("source-section start");
    let end = source[start..]
        .find(end)
        .map(|offset| start + offset)
        .expect("source-section end");
    &source[start..end]
}

fn source_window<'a>(source: &'a str, needle: &str, length: usize) -> &'a str {
    let start = source.find(needle).expect("source-window start");
    &source[start..source.len().min(start + length)]
}

fn assert_exact_ignored_probe(source: &str, probe: &str, workload_runner: &str) {
    let signature = format!("fn {probe}()");
    assert_eq!(source.matches(&signature).count(), 1);
    let start = source.find(&signature).expect("exact ignored probe");
    let attributes = &source[start.saturating_sub(300)..start];
    assert!(attributes.contains("#[test]"));
    assert!(attributes.contains("#[ignore"));

    let probe = source_window(source, &signature, 700);
    assert!(probe.contains(&format!("{workload_runner}(")));
    for other in [
        "run_candidate_primary_workload(",
        "run_candidate_non_cycle_workload(",
        "run_candidate_reporter_control_workload(",
    ] {
        if other != format!("{workload_runner}(") {
            assert!(!probe.contains(other));
        }
    }
    assert!(probe.contains("resolve_candidate_environment"));
    assert!(probe.contains(".render()"));
}

#[test]
fn candidate_is_cfg_test_only_and_three_exact_ignored_probes_own_fixed_workloads() {
    let implementation = include_str!("wu0d_candidate_release.rs");
    let checker_mod = include_str!("mod.rs");
    let context = include_str!("context.rs");
    let resolve = include_str!("decls/resolve.rs");
    let checker = include_str!("mod.rs");
    let wu0b = include_str!("wu0b_library.rs");
    let driver = include_str!("../../driver.rs");
    let crate_root = include_str!("../../lib.rs");

    let compact_mod = without_whitespace(checker_mod);
    assert_eq!(compact_mod.matches("modwu0d_candidate_release;").count(), 1);
    assert!(compact_mod.contains("#[cfg(test)]modwu0d_candidate_release;"));
    assert!(!compact_mod.contains("pubmodwu0d_candidate_release;"));
    assert!(!driver.contains("TYPOKAT_WU0D_"));
    assert!(!crate_root.contains("wu0d_candidate_release"));

    for (probe, runner) in [
        (
            "wu0d_candidate_primary_probe_once",
            "run_candidate_primary_workload",
        ),
        (
            "wu0d_candidate_non_cycle_probe_once",
            "run_candidate_non_cycle_workload",
        ),
        (
            "wu0d_candidate_reporter_control_probe_once",
            "run_candidate_reporter_control_workload",
        ),
    ] {
        assert_exact_ignored_probe(implementation, probe, runner);
    }
    assert!(!implementation.contains("fn wu0d_candidate_release_probe_once()"));

    for (runner, embedded_profile) in [
        (
            "fn run_candidate_primary_workload(",
            "PRIMARY_WORKLOAD_SOURCES",
        ),
        (
            "fn run_candidate_non_cycle_workload(",
            "NON_CYCLE_WORKLOAD_SOURCES",
        ),
        (
            "fn run_candidate_reporter_control_workload(",
            "REPORTER_CONTROL_WORKLOAD_SOURCES",
        ),
    ] {
        assert_eq!(implementation.matches(runner).count(), 1);
        let workload = source_window(implementation, runner, 1_500);
        assert!(workload.contains("run_injected_profile("));
        assert!(workload.contains(embedded_profile));
        assert!(workload.contains("canonical_wu0d_semantic_identity("));
    }
    assert!(implementation.contains("CandidateWorkload::Primary"));
    assert!(implementation.contains("CandidateWorkload::NonCycle"));
    assert!(implementation.contains("CandidateWorkload::ReporterControl"));
    assert!(implementation.contains("typokat-wu0d-candidate-v1"));
    assert!(
        context.contains("first_run_visit_weight") || resolve.contains("first_run_visit_weight")
    );
    assert!(
        implementation.contains("checked_add")
            || context.contains("checked_add")
            || resolve.contains("checked_add")
    );

    let pass_fields = without_whitespace(source_section(
        context,
        "struct Pass<'a, 'ast",
        "impl<'ast, Ticket: Copy + PartialEq> Deref",
    ));
    for field in [
        "cycle_tainted_application_cache:",
        "cycle_tainted_application_cache_measure:",
    ] {
        assert!(pass_fields.contains(&format!("#[cfg(test)]pub(incrate::check::checker){field}")));
    }

    let build = without_whitespace(source_section(
        checker,
        "fn build_pass_with_tickets",
        "// M5: named types",
    ));
    for initializer in [
        "cycle_tainted_application_cache:",
        "cycle_tainted_application_cache_measure:",
    ] {
        assert!(build.contains(&format!("#[cfg(test)]{initializer}")));
    }

    assert!(wu0b.contains("fn canonical_wu0d_semantic_identity("));
    assert!(wu0b.contains("Wu0dSemanticComponents"));
    assert!(wu0b.contains("frozen_library_product"));
    assert!(!implementation.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("pub fn")
            || line.starts_with("pub struct")
            || line.starts_with("pub enum")
            || line.starts_with("pub(crate)")
    }));

    for forbidden in [
        "TYPOKAT_WU0D_PROFILE",
        "TYPOKAT_WU0D_TRACE",
        "TYPOKAT_WU0D_PATH",
        "TYPOKAT_WU0D_LIMIT",
    ] {
        assert!(!implementation.contains(forbidden));
    }
}

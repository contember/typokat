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
//!
//! The checked-in validator consumes one bounded canonical evidence file selected by
//! the dedicated `TYPOKAT_WU0D_RELEASE_EVIDENCE_PATH` environment variable. The path
//! must be absolute and name a regular non-symlink file. The artifact is ASCII with
//! LF-only lines and a final LF: one version/count header followed by exactly thirty
//! launch-ordered process records (primary, non-cycle, reporter-control). Every
//! externally owned process fact is present in fixed field order. Text and raw stdout
//! are lossless lowercase hex with explicit byte lengths; unknown, duplicate,
//! reordered, oversized, or noncanonical evidence rejects. The validator prints one
//! stable decision line and authorizes Candidate B only for `GateDecision::Go`.

use super::wu0b_library::{
    canonical_wu0d_semantic_identity_from_components_for_test, run_injected_profile,
    InjectedLibrarySource, InjectedProfileRun, Wu0dDecodedClassTerminal, Wu0dFrozenProductSection,
    Wu0dSemanticComponents, Wu0dSemanticIdentity,
};
#[cfg(windows)]
use super::wu0d_candidate_release::resolve_candidate_environment_os_for_test;
use super::wu0d_candidate_release::{
    evaluate_candidate_b_release, parse_candidate_release_evidence, parse_candidate_stdout,
    render_candidate_release_validation, resolve_candidate_environment_bytes_for_test,
    resolve_candidate_environment_for_test, validate_candidate_b_release_evidence_file,
    CacheMetrics, CandidateEnvironment, CandidateSummary, CandidateWorkload, ControlEvidence,
    GateDecision, NoGoReason, PairedReleaseRun, ProcessObservation, MAX_RELEASE_EVIDENCE_BYTES,
    MAX_RELEASE_STDOUT_BYTES, NON_CYCLE_WORKLOAD_SOURCES, REPORTER_CONTROL_WORKLOAD_SOURCES,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const ENABLE_KEY: &str = "TYPOKAT_WU0D_CANDIDATE";
const ENABLE_VALUE: &str = "candidate-b-v1";
const BINARY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const HOST: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const PRIMARY_PROFILE: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const NON_CYCLE_PROFILE: &str = "1c664166f4c307f032958836642008c90c28cb21ff33215144c9188ac8afdd19";
const REPORTER_CONTROL_PROFILE: &str =
    "9f5e4ab6a334154e67fe1ead6e7e1038d9433f5fd66f7ed897a73cfd1d058d0b";
const SEMANTIC: &str = "740c77fa0237ef4bde7f5b0031d3d0a7c977a39d6fd75325148a636c0595f08d";
const NON_CYCLE: &str = "non-cycle";
const REPORTER_CONTROL: &str = "reporter-control";
const PRIMARY_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_primary_probe_once";
const NON_CYCLE_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_non_cycle_probe_once";
const REPORTER_CONTROL_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_reporter_control_probe_once";
const EVIDENCE_PATH_KEY: &str = "TYPOKAT_WU0D_RELEASE_EVIDENCE_PATH";
const EXPECTED_MAX_RELEASE_EVIDENCE_BYTES: usize = 4 * 1_024 * 1_024;
const EXPECTED_MAX_RELEASE_STDOUT_BYTES: usize = 128 * 1_024;
const MIB: u64 = 1_024 * 1_024;

fn environment(entries: &[(&str, &str)]) -> Result<CandidateEnvironment, String> {
    resolve_candidate_environment_for_test(entries.iter().copied())
        .map_err(|error| error.to_string())
}

fn length_framed_profile_identity(
    sources: &[super::wu0b_library::InjectedLibrarySource<'_>],
) -> String {
    let mut digest = Sha256::new();
    for source in sources {
        let name_len = u64::try_from(source.name.len()).expect("test profile name length fits u64");
        let source_len =
            u64::try_from(source.source.len()).expect("test profile source length fits u64");
        digest.update(name_len.to_be_bytes());
        digest.update(source_len.to_be_bytes());
        digest.update(source.name.as_bytes());
        digest.update(source.source.as_bytes());
    }
    format!("{:x}", digest.finalize())
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

#[derive(Copy, Clone)]
struct WorkloadFixture<'a> {
    workload: CandidateWorkload,
    probe_filter: &'a str,
    profile: &'a str,
}

fn process(
    set: &str,
    sequence: u8,
    launch_ordinal: u8,
    candidate: bool,
    fixture: WorkloadFixture<'_>,
    elapsed_us: u64,
) -> ProcessObservation {
    let summary = match fixture.workload {
        CandidateWorkload::Primary => summary(candidate),
        CandidateWorkload::NonCycle | CandidateWorkload::ReporterControl => {
            control_summary(candidate, fixture.workload)
        }
    };
    ProcessObservation {
        process_identity: format!("{set}-fresh-{sequence}"),
        launch_ordinal,
        probe_filter: fixture.probe_filter.into(),
        binary_identity: BINARY.into(),
        host_identity: HOST.into(),
        profile_identity: fixture.profile.into(),
        warm_filesystem_cache: true,
        release_libtest: true,
        exit_code: 0,
        elapsed_us,
        peak_rss_bytes: 400 * MIB,
        captured_stdout: captured_stdout(&summary, fixture.probe_filter),
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
    let fixture = WorkloadFixture {
        workload,
        probe_filter,
        profile,
    };
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
        baseline: process(set, index * 2, baseline_launch, false, fixture, baseline_us),
        candidate: process(
            set,
            index * 2 + 1,
            candidate_launch,
            true,
            fixture,
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

fn lower_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn evidence_process_line(
    set: &str,
    pair: u8,
    variant: &str,
    process: &ProcessObservation,
) -> String {
    format!(
        "process set={set} pair={pair} variant={variant} process_identity_len={} process_identity_hex={} launch_ordinal={} probe_filter_len={} probe_filter_hex={} binary_identity={} host_identity={} profile_identity={} warm_filesystem_cache={} release_libtest={} exit_code={} elapsed_us={} peak_rss_bytes={} stdout_len={} stdout_hex={}",
        process.process_identity.len(),
        lower_hex(process.process_identity.as_bytes()),
        process.launch_ordinal,
        process.probe_filter.len(),
        lower_hex(process.probe_filter.as_bytes()),
        process.binary_identity,
        process.host_identity,
        process.profile_identity,
        u8::from(process.warm_filesystem_cache),
        u8::from(process.release_libtest),
        process.exit_code,
        process.elapsed_us,
        process.peak_rss_bytes,
        process.captured_stdout.len(),
        lower_hex(&process.captured_stdout),
    )
}

fn evidence_set_lines(set: &str, pairs: &[PairedReleaseRun]) -> Vec<String> {
    let mut launches = pairs
        .iter()
        .flat_map(|pair| {
            [
                (
                    pair.baseline.launch_ordinal,
                    pair.pair,
                    "off",
                    &pair.baseline,
                ),
                (
                    pair.candidate.launch_ordinal,
                    pair.pair,
                    "candidate-b",
                    &pair.candidate,
                ),
            ]
        })
        .collect::<Vec<_>>();
    launches.sort_by_key(|(launch, _, _, _)| *launch);
    launches
        .into_iter()
        .map(|(_, pair, variant, process)| evidence_process_line(set, pair, variant, process))
        .collect()
}

fn evidence_artifact(primary: &[PairedReleaseRun], controls: &[ControlEvidence]) -> Vec<u8> {
    let non_cycle = controls
        .iter()
        .find(|control| control.name == NON_CYCLE)
        .expect("non-cycle fixture");
    let reporter = controls
        .iter()
        .find(|control| control.name == REPORTER_CONTROL)
        .expect("reporter-control fixture");
    let mut lines = vec!["typokat-wu0d-release-evidence-v1 process_count=30".to_owned()];
    lines.extend(evidence_set_lines("primary", primary));
    lines.extend(evidence_set_lines(NON_CYCLE, &non_cycle.pairs));
    lines.extend(evidence_set_lines(REPORTER_CONTROL, &reporter.pairs));
    format!("{}\n", lines.join("\n")).into_bytes()
}

fn evidence_artifact_authorizes(
    primary: &[PairedReleaseRun],
    controls: &[ControlEvidence],
) -> bool {
    let artifact = evidence_artifact(primary, controls);
    let parsed = parse_candidate_release_evidence(&artifact)
        .expect("structural evidence fixture must remain canonical");
    evaluate_candidate_b_release(&parsed.primary, &parsed.controls).authorizes_candidate_b()
}

fn changed_semantic_identity(marker: u8) -> Wu0dSemanticIdentity {
    let mut components = semantic_components();
    components.frozen_library_product.push(marker);
    canonical_wu0d_semantic_identity_from_components_for_test(&components)
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

#[cfg(windows)]
#[test]
fn windows_environment_classifies_raw_key_prefix_before_strict_unicode_decode() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let unrelated_non_unicode_key = OsString::from_wide(
        &"UNRELATED_"
            .encode_utf16()
            .chain(std::iter::once(0xd800))
            .collect::<Vec<_>>(),
    );
    let unrelated_non_unicode_value = OsString::from_wide(&[0xd800]);
    let entries = vec![
        (unrelated_non_unicode_key.clone(), OsString::from("ignored")),
        (
            OsString::from("UNRELATED_VALUE"),
            unrelated_non_unicode_value.clone(),
        ),
    ];
    assert_eq!(
        resolve_candidate_environment_os_for_test(entries.clone()).unwrap(),
        CandidateEnvironment::Off,
    );

    let mut enabled = entries;
    enabled.push((OsString::from(ENABLE_KEY), OsString::from(ENABLE_VALUE)));
    assert_eq!(
        resolve_candidate_environment_os_for_test(enabled).unwrap(),
        CandidateEnvironment::CandidateB,
    );
    assert!(resolve_candidate_environment_os_for_test(vec![(
        OsString::from(ENABLE_KEY),
        unrelated_non_unicode_value,
    )])
    .is_err());

    let invalid_wu0d_key = OsString::from_wide(
        &"TYPOKAT_WU0D_"
            .encode_utf16()
            .chain(std::iter::once(0xd800))
            .collect::<Vec<_>>(),
    );
    assert!(resolve_candidate_environment_os_for_test(vec![(
        invalid_wu0d_key,
        OsString::from("ignored"),
    )])
    .is_err());
}

#[test]
fn windows_environment_source_classifies_prefix_before_decoding_key_or_value() {
    let implementation = include_str!("wu0d_candidate_release.rs");
    let entry = source_top_level_function(
        implementation,
        "#[cfg(windows)]\nfn wu0d_environment_entry(",
    );
    let raw_key = entry.find("encode_wide").expect("raw Windows key units");
    let prefix = entry
        .find("starts_with")
        .expect("raw Windows WU0D prefix classification");
    let decode = entry
        .find("into_string")
        .expect("strict WU0D Unicode decode");
    assert!(raw_key < prefix && prefix < decode);
    assert!(entry.contains("\"TYPOKAT_WU0D_\".encode_utf16()"));
    assert!(entry.contains("return None"));
    assert!(!entry.contains("to_string_lossy"));

    let test_seam = source_top_level_function(
        implementation,
        "pub(super) fn resolve_candidate_environment_os_for_test(",
    );
    assert!(test_seam.contains("wu0d_environment_entry"));
    assert!(test_seam.contains("resolve_candidate_environment_bytes"));
}

#[test]
fn fixed_workload_profile_identities_are_independently_derived() {
    let primary = super::wu0b_profile::load_strict_profile().unwrap();
    assert_eq!(
        length_framed_profile_identity(&primary.injected_sources()),
        PRIMARY_PROFILE,
    );
    assert_eq!(
        length_framed_profile_identity(NON_CYCLE_WORKLOAD_SOURCES),
        NON_CYCLE_PROFILE,
    );
    assert_eq!(
        length_framed_profile_identity(REPORTER_CONTROL_WORKLOAD_SOURCES),
        REPORTER_CONTROL_PROFILE,
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
                assert_eq!(summary.tainted_outcomes, 0);
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
fn process_identity_is_unique_across_primary_and_both_controls() {
    let primary = passing_pairs();
    let controls = passing_controls();
    assert_eq!(
        evaluate_candidate_b_release(&primary, &controls),
        GateDecision::Go,
    );

    for control_index in 0..2 {
        let mut reused = passing_controls();
        reused[control_index].pairs[0].baseline.process_identity =
            primary[0].baseline.process_identity.clone();
        assert!(
            !evaluate_candidate_b_release(&primary, &reused).authorizes_candidate_b(),
            "a process identity reused across workload sets must reject",
        );
    }

    let mut reused = passing_controls();
    let reused_identity = reused[0].pairs[4].candidate.process_identity.clone();
    reused[1].pairs[4].candidate.process_identity = reused_identity;
    assert!(
        !evaluate_candidate_b_release(&primary, &reused).authorizes_candidate_b(),
        "a process identity reused across the two controls must reject",
    );
}

#[test]
fn raw_evidence_requires_nonempty_identity_and_nonzero_timing_and_rss_for_every_process() {
    let mut authorized = Vec::new();

    let mut primary = passing_pairs();
    primary[0].candidate.process_identity.clear();
    if evidence_artifact_authorizes(&primary, &passing_controls()) {
        authorized.push("primary empty process identity".to_owned());
    }

    let mut primary = passing_pairs();
    primary[0].candidate.elapsed_us = 0;
    if evidence_artifact_authorizes(&primary, &passing_controls()) {
        authorized.push("primary zero elapsed time".to_owned());
    }

    let mut primary = passing_pairs();
    primary[0].candidate.peak_rss_bytes = 0;
    if evidence_artifact_authorizes(&primary, &passing_controls()) {
        authorized.push("primary zero peak RSS".to_owned());
    }

    for control_index in 0..2 {
        let control_name = passing_controls()[control_index].name.clone();

        let mut controls = passing_controls();
        controls[control_index].pairs[0]
            .candidate
            .process_identity
            .clear();
        if evidence_artifact_authorizes(&passing_pairs(), &controls) {
            authorized.push(format!("{control_name} empty process identity"));
        }

        let mut controls = passing_controls();
        controls[control_index].pairs[0].candidate.elapsed_us = 0;
        if evidence_artifact_authorizes(&passing_pairs(), &controls) {
            authorized.push(format!("{control_name} zero elapsed time"));
        }

        let mut controls = passing_controls();
        controls[control_index].pairs[0].candidate.peak_rss_bytes = 0;
        if evidence_artifact_authorizes(&passing_pairs(), &controls) {
            authorized.push(format!("{control_name} zero peak RSS"));
        }
    }

    assert!(
        authorized.is_empty(),
        "incomplete external process facts authorized: {authorized:?}",
    );
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
fn zero_elapsed_denominators_never_authorize_primary_or_controls() {
    let mut authorized = Vec::new();

    let mut primary = passing_pairs();
    for index in [0, 3, 4] {
        primary[index].baseline.elapsed_us = 0;
        primary[index].candidate.elapsed_us = 0;
    }
    if evaluate_candidate_b_release(&primary, &passing_controls()).authorizes_candidate_b() {
        authorized.push("primary");
    }

    for control_index in 0..2 {
        let mut controls = passing_controls();
        let name = controls[control_index].name.clone();
        for pair_index in [0, 3, 4] {
            controls[control_index].pairs[pair_index]
                .baseline
                .elapsed_us = 0;
            controls[control_index].pairs[pair_index]
                .candidate
                .elapsed_us = 0;
        }
        if evaluate_candidate_b_release(&passing_pairs(), &controls).authorizes_candidate_b() {
            authorized.push(match name.as_str() {
                NON_CYCLE => "non-cycle control",
                REPORTER_CONTROL => "reporter control",
                _ => unreachable!("fixed control name"),
            });
        }
    }

    assert!(
        authorized.is_empty(),
        "zero-denominator timing evidence authorized: {authorized:?}",
    );
}

#[test]
fn semantic_identity_is_stable_across_all_ten_processes_per_workload() {
    let controls = passing_controls();
    let mut primary = passing_pairs();
    let drift = changed_semantic_identity(b'P');
    let pair = &mut primary[2];
    for process in [&mut pair.baseline, &mut pair.candidate] {
        let mut changed = parse_candidate_stdout(&process.captured_stdout).unwrap();
        changed.semantic = drift.clone();
        replace_summary(process, changed);
    }
    assert_reason(&primary, &controls, NoGoReason::SemanticMismatch);

    for (control_index, marker) in [(0, b'N'), (1, b'R')] {
        let primary = passing_pairs();
        let mut controls = passing_controls();
        let drift = changed_semantic_identity(marker);
        let pair = &mut controls[control_index].pairs[3];
        for process in [&mut pair.baseline, &mut pair.candidate] {
            let mut changed = parse_candidate_stdout(&process.captured_stdout).unwrap();
            changed.semantic = drift.clone();
            replace_summary(process, changed);
        }
        assert_reason(
            &primary,
            &controls,
            NoGoReason::ControlIdentityOrSemanticMismatch,
        );
    }
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
    let pair = &mut controls[0].pairs[0];
    for process in [&mut pair.baseline, &mut pair.candidate] {
        let mut changed = parse_candidate_stdout(&process.captured_stdout).unwrap();
        changed.clean_outcomes = 99;
        changed.tainted_outcomes = 1;
        replace_summary(process, changed);
    }
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

#[test]
fn canonical_evidence_artifact_round_trips_all_external_process_facts() {
    assert_eq!(
        MAX_RELEASE_EVIDENCE_BYTES,
        EXPECTED_MAX_RELEASE_EVIDENCE_BYTES,
    );
    assert_eq!(MAX_RELEASE_STDOUT_BYTES, EXPECTED_MAX_RELEASE_STDOUT_BYTES,);
    let primary = passing_pairs();
    let controls = passing_controls();
    let artifact = evidence_artifact(&primary, &controls);
    assert!(artifact.len() <= MAX_RELEASE_EVIDENCE_BYTES);
    assert!(primary
        .iter()
        .chain(controls.iter().flat_map(|control| control.pairs.iter()))
        .flat_map(|pair| [&pair.baseline, &pair.candidate])
        .all(|process| process.captured_stdout.len() <= MAX_RELEASE_STDOUT_BYTES));

    let parsed = parse_candidate_release_evidence(&artifact).unwrap();
    assert_eq!(parsed.primary, primary);
    assert_eq!(parsed.controls, controls);
    assert_eq!(
        evaluate_candidate_b_release(&parsed.primary, &parsed.controls),
        GateDecision::Go,
    );
}

#[test]
fn evidence_artifact_rejects_malformed_truncated_duplicate_unknown_and_misordered_input() {
    let artifact = evidence_artifact(&passing_pairs(), &passing_controls());
    let text = std::str::from_utf8(&artifact).unwrap();
    let lines = text
        .strip_suffix('\n')
        .expect("canonical fixture has final LF")
        .split('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 31);

    let mut truncated = lines.clone();
    truncated.pop();
    let mut duplicate_process = lines.clone();
    duplicate_process.insert(2, duplicate_process[1].clone());
    let mut unknown_field = lines.clone();
    unknown_field[1].push_str(" unknown=0");
    let mut duplicate_field = lines.clone();
    duplicate_field[1] = duplicate_field[1].replacen(" pair=1", " pair=1 pair=1", 1);
    let mut misordered = lines.clone();
    misordered.swap(1, 2);
    let mut wrong_count = lines.clone();
    wrong_count[0] = "typokat-wu0d-release-evidence-v1 process_count=29".to_owned();
    let mut length_mismatch = lines.clone();
    length_mismatch[1] = length_mismatch[1].replacen(" stdout_len=", " stdout_len=999", 1);
    let mut leading_plus_pair = lines.clone();
    leading_plus_pair[1] = leading_plus_pair[1].replacen(" pair=1", " pair=+1", 1);
    let mut leading_plus_elapsed = lines.clone();
    leading_plus_elapsed[1] =
        leading_plus_elapsed[1].replacen(" elapsed_us=4000000", " elapsed_us=+4000000", 1);
    let mut oversized_stdout = passing_pairs();
    oversized_stdout[0].baseline.captured_stdout = vec![b'x'; MAX_RELEASE_STDOUT_BYTES + 1];

    let mut malformed = vec![
        truncated.join("\n").into_bytes(),
        format!("{}\n", duplicate_process.join("\n")).into_bytes(),
        format!("{}\n", unknown_field.join("\n")).into_bytes(),
        format!("{}\n", duplicate_field.join("\n")).into_bytes(),
        format!("{}\n", misordered.join("\n")).into_bytes(),
        format!("{}\n", wrong_count.join("\n")).into_bytes(),
        format!("{}\n", length_mismatch.join("\n")).into_bytes(),
        format!("{}\n", leading_plus_pair.join("\n")).into_bytes(),
        format!("{}\n", leading_plus_elapsed.join("\n")).into_bytes(),
        text.replace('\n', "\r\n").into_bytes(),
        artifact[..artifact.len() - 1].to_vec(),
        vec![b'x'; MAX_RELEASE_EVIDENCE_BYTES + 1],
        evidence_artifact(&oversized_stdout, &passing_controls()),
    ];
    let mut non_utf8 = artifact.clone();
    non_utf8[0] = 0xff;
    malformed.push(non_utf8);
    let mut odd_hex = artifact.clone();
    let stdout_hex = text.find("stdout_hex=").expect("stdout hex field") + "stdout_hex=".len();
    odd_hex.remove(stdout_hex);
    malformed.push(odd_hex);

    for malformed in malformed {
        assert!(
            parse_candidate_release_evidence(&malformed).is_err(),
            "malformed evidence must reject",
        );
    }
}

#[test]
#[cfg(unix)]
fn evidence_file_path_is_absolute_regular_non_symlink_and_validation_output_is_stable() {
    assert!(validate_candidate_b_release_evidence_file(Path::new("relative.evidence")).is_err());
    let temp_dir = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(temp_dir.is_absolute());
    assert!(validate_candidate_b_release_evidence_file(&temp_dir).is_err());

    let path = temp_dir.join(format!(
        "typokat-wu0d-release-evidence-{}-{}.txt",
        std::process::id(),
        line!(),
    ));
    std::fs::write(
        &path,
        evidence_artifact(&passing_pairs(), &passing_controls()),
    )
    .unwrap();
    let decision = validate_candidate_b_release_evidence_file(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(decision, GateDecision::Go);

    let oversized = temp_dir.join(format!(
        "typokat-wu0d-release-oversized-{}-{}.txt",
        std::process::id(),
        line!(),
    ));
    let oversized_file = std::fs::File::create(&oversized).unwrap();
    oversized_file
        .set_len(u64::try_from(MAX_RELEASE_EVIDENCE_BYTES + 1).unwrap())
        .unwrap();
    drop(oversized_file);
    assert!(
        validate_candidate_b_release_evidence_file(&oversized).is_err(),
        "a sparse oversized artifact must reject before an unbounded read",
    );
    std::fs::remove_file(&oversized).unwrap();

    {
        use std::os::unix::fs::symlink;

        let target = temp_dir.join(format!(
            "typokat-wu0d-release-target-{}-{}.txt",
            std::process::id(),
            line!(),
        ));
        let link = temp_dir.join(format!(
            "typokat-wu0d-release-link-{}-{}.txt",
            std::process::id(),
            line!(),
        ));
        std::fs::write(
            &target,
            evidence_artifact(&passing_pairs(), &passing_controls()),
        )
        .unwrap();
        symlink(&target, &link).unwrap();
        assert!(validate_candidate_b_release_evidence_file(&link).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::remove_file(&target).unwrap();
    }

    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{mkfifoat, open, Mode, OFlags};
        use std::sync::mpsc::{channel, RecvTimeoutError};
        use std::time::Duration;

        let directory = open(
            &temp_dir,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap();
        let fifo_name = format!(
            "typokat-wu0d-release-fifo-{}-{}.txt",
            std::process::id(),
            line!(),
        );
        let fifo = temp_dir.join(&fifo_name);
        mkfifoat(&directory, fifo_name, Mode::RUSR | Mode::WUSR).unwrap();
        let worker_path = fifo.clone();
        let (sender, receiver) = channel();
        let worker = std::thread::spawn(move || {
            let rejected = validate_candidate_b_release_evidence_file(&worker_path).is_err();
            let _ = sender.send(rejected);
        });
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(rejected) => {
                worker.join().unwrap();
                assert!(rejected, "FIFO evidence must reject");
            }
            Err(RecvTimeoutError::Timeout) => {
                let writer = open(
                    &fifo,
                    OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .expect("FIFO rescue handle must open without a scheduled reader");
                worker.join().unwrap();
                drop(writer);
                std::fs::remove_file(&fifo).unwrap();
                panic!("FIFO evidence validation blocked before handle fstat");
            }
            Err(RecvTimeoutError::Disconnected) => {
                let joined = worker.join();
                std::fs::remove_file(&fifo).unwrap();
                assert!(joined.is_ok(), "FIFO evidence validator worker panicked");
                panic!("FIFO evidence validator worker disconnected");
            }
        }
        std::fs::remove_file(fifo).unwrap();
    }
}

#[test]
fn release_validation_output_is_stable_on_every_host() {
    assert_eq!(
        render_candidate_release_validation(&GateDecision::Go),
        "typokat-wu0d-release-validation-v1 decision=go reasons=none",
    );
    assert_eq!(
        render_candidate_release_validation(&GateDecision::NoGo(vec![
            NoGoReason::SemanticMismatch,
        ])),
        "typokat-wu0d-release-validation-v1 decision=no-go reasons=semantic-mismatch",
    );
}

#[cfg(not(unix))]
#[test]
fn evidence_file_validation_fails_closed_on_unsupported_non_unix_hosts() {
    let temp_dir = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let evidence = temp_dir.join(format!(
        "typokat-wu0d-release-non-unix-{}-{}.txt",
        std::process::id(),
        line!(),
    ));
    assert!(evidence.is_absolute());
    std::fs::write(
        &evidence,
        evidence_artifact(&passing_pairs(), &passing_controls()),
    )
    .unwrap();
    let error = validate_candidate_b_release_evidence_file(&evidence).unwrap_err();
    std::fs::remove_file(evidence).unwrap();
    assert_eq!(
        error.to_string(),
        "WU0D release evidence validation is unsupported on non-Unix hosts",
    );
}

#[test]
#[ignore = "release-only WU0D external evidence validator"]
fn wu0d_candidate_release_validate_evidence_file() {
    let path = std::env::var_os(EVIDENCE_PATH_KEY)
        .map(PathBuf::from)
        .expect("TYPOKAT_WU0D_RELEASE_EVIDENCE_PATH is required");
    let decision = validate_candidate_b_release_evidence_file(&path)
        .expect("strict WU0D release evidence must parse and validate");
    let rendered = render_candidate_release_validation(&decision);
    println!("{rendered}");
    assert!(decision.authorizes_candidate_b(), "{rendered}");
}

fn frozen_product_run(file_ordinal: usize, name: &str, source: &str) -> InjectedProfileRun {
    run_injected_profile(&[InjectedLibrarySource {
        file_ordinal: crate::source::LibraryFileOrdinal::new(file_ordinal),
        name,
        source,
    }])
    .expect("frozen-product witness profile")
}

fn frozen_product_section(run: &InjectedProfileRun, section: Wu0dFrozenProductSection) -> &[u8] {
    let product = &run.wu0d_semantic_components.frozen_library_product;
    let section = run.wu0d_frozen_product_section_for_test(section);
    assert!(!section.is_empty(), "a framed section cannot be empty");
    let start = product
        .windows(section.len())
        .position(|window| std::ptr::eq(window.as_ptr(), section.as_ptr()))
        .expect("section accessor must return a range of the actual final product buffer");
    assert_eq!(
        &product[start..start + section.len()],
        section,
        "section range must resolve exactly inside the final product",
    );
    section
}

fn frozen_product_section_range(
    run: &InjectedProfileRun,
    section: Wu0dFrozenProductSection,
) -> std::ops::Range<usize> {
    let product = &run.wu0d_semantic_components.frozen_library_product;
    let section = run.wu0d_frozen_product_section_for_test(section);
    let start = product
        .windows(section.len())
        .position(|window| std::ptr::eq(window.as_ptr(), section.as_ptr()))
        .expect("section must be borrowed from the final product");
    let end = start + section.len();
    assert_eq!(
        &product[start..end],
        section,
        "section range must identify its exact framed bytes",
    );
    start..end
}

#[test]
fn frozen_product_includes_source_ordinal_provenance_for_identical_bytes() {
    let source = "declare const Wu0dProvenance: number;\n";
    let first = frozen_product_run(0, "same.d.ts", source);
    let second = frozen_product_run(7, "same.d.ts", source);
    assert_eq!(
        frozen_product_section(&first, Wu0dFrozenProductSection::TypeStore),
        frozen_product_section(&second, Wu0dFrozenProductSection::TypeStore),
        "changing only provenance must not be smuggled through TypeStore rows",
    );
    assert_ne!(
        frozen_product_section(&first, Wu0dFrozenProductSection::SourceRecords),
        frozen_product_section(&second, Wu0dFrozenProductSection::SourceRecords),
        "LibraryFileOrdinal is semantic publication provenance",
    );
    assert_ne!(
        first.wu0d_semantic_components.frozen_library_product,
        second.wu0d_semantic_components.frozen_library_product,
    );
}

#[test]
fn frozen_product_includes_value_placement_and_namespace_publication_surfaces() {
    let global_left = frozen_product_run(0, "value.d.ts", "declare const Wu0dLeft: number;\n");
    let global_right = frozen_product_run(0, "value.d.ts", "declare const Wu0dRight: number;\n");
    assert_eq!(
        frozen_product_section(&global_left, Wu0dFrozenProductSection::TypeStore),
        frozen_product_section(&global_right, Wu0dFrozenProductSection::TypeStore),
    );
    assert_ne!(
        frozen_product_section(&global_left, Wu0dFrozenProductSection::GlobalValues),
        frozen_product_section(&global_right, Wu0dFrozenProductSection::GlobalValues),
        "global value keys must be encoded from the production collector",
    );

    let global = frozen_product_run(
        0,
        "placement.d.ts",
        "declare const Wu0dPlacement: number;\n",
    );
    let module = frozen_product_run(
        0,
        "placement.d.ts",
        "export declare const Wu0dPlacement: number;\n",
    );
    assert_eq!(
        frozen_product_section(&global, Wu0dFrozenProductSection::TypeStore),
        frozen_product_section(&module, Wu0dFrozenProductSection::TypeStore),
    );
    assert_ne!(
        frozen_product_section(&global, Wu0dFrozenProductSection::GlobalValues),
        frozen_product_section(&module, Wu0dFrozenProductSection::GlobalValues),
    );
    assert_ne!(
        frozen_product_section(&global, Wu0dFrozenProductSection::ModuleValues),
        frozen_product_section(&module, Wu0dFrozenProductSection::ModuleValues),
        "module value ownership must be encoded from the production collector",
    );

    let namespace_left = frozen_product_run(
        0,
        "namespace.d.ts",
        "declare namespace Wu0dLeft { const marker: number; }\n",
    );
    let namespace_right = frozen_product_run(
        0,
        "namespace.d.ts",
        "declare namespace Wu0dRight { const marker: number; }\n",
    );
    assert_eq!(
        frozen_product_section(&namespace_left, Wu0dFrozenProductSection::TypeStore),
        frozen_product_section(&namespace_right, Wu0dFrozenProductSection::TypeStore),
    );
    assert_ne!(
        frozen_product_section(&namespace_left, Wu0dFrozenProductSection::NamespaceValues,),
        frozen_product_section(&namespace_right, Wu0dFrozenProductSection::NamespaceValues,),
        "namespace ownership must be encoded from the production collector",
    );
}

#[test]
fn every_frozen_product_section_is_a_named_frame_inside_the_final_product() {
    let run = frozen_product_run(0, "sections.d.ts", "declare const value: number;\n");
    let sections = [
        Wu0dFrozenProductSection::SourceRecords,
        Wu0dFrozenProductSection::TypeStore,
        Wu0dFrozenProductSection::TypePublications,
        Wu0dFrozenProductSection::GlobalValues,
        Wu0dFrozenProductSection::ModuleValues,
        Wu0dFrozenProductSection::NamespaceValues,
        Wu0dFrozenProductSection::Classes,
    ];
    let ranges = sections.map(|section| frozen_product_section_range(&run, section));
    for left in 0..ranges.len() {
        for right in left + 1..ranges.len() {
            assert_ne!(ranges[left], ranges[right], "named sections cannot alias");
            assert!(
                ranges[left].end <= ranges[right].start || ranges[right].end <= ranges[left].start,
                "named section ranges must not overlap",
            );
        }
    }
}

#[test]
fn class_section_distinguishes_ready_and_each_canonical_poison_terminal_in_class_id_order() {
    let ready = frozen_product_run(0, "class.ts", "class Ready { value!: number; }\n");
    let initializer =
        frozen_product_run(0, "class.ts", "class InitializerPoison { value = +1; }\n");
    let heritage = frozen_product_run(
        0,
        "class.ts",
        "class HeritagePoison extends HeritagePoison {}\n",
    );
    let surface = frozen_product_run(
        0,
        "class.ts",
        "declare const seed: number; class SurfacePoison { value!: typeof seed; }\n",
    );
    let class_sections = [&ready, &initializer, &heritage, &surface]
        .map(|run| frozen_product_section(run, Wu0dFrozenProductSection::Classes));
    for left in 0..class_sections.len() {
        for right in left + 1..class_sections.len() {
            assert_ne!(
                class_sections[left], class_sections[right],
                "ready/heritage/initializer/surface terminals must remain distinct",
            );
        }
    }

    for (run, expected) in [
        (&ready, Wu0dDecodedClassTerminal::Ready),
        (&heritage, Wu0dDecodedClassTerminal::HeritagePoison),
        (&initializer, Wu0dDecodedClassTerminal::InitializerPoison),
        (&surface, Wu0dDecodedClassTerminal::SurfacePoison),
    ] {
        assert_eq!(
            run.wu0d_decoded_class_terminals_for_test().unwrap(),
            vec![(crate::types::repr::ClassId(0), expected)],
        );
    }

    let lexical = frozen_product_run(0, "order.ts", "class First {} class Second {}\n");
    assert_eq!(
        lexical.wu0d_decoded_class_terminals_for_test().unwrap(),
        vec![
            (
                crate::types::repr::ClassId(0),
                Wu0dDecodedClassTerminal::Ready,
            ),
            (
                crate::types::repr::ClassId(1),
                Wu0dDecodedClassTerminal::Ready,
            ),
        ],
        "class terminals must decode in stable ClassId order",
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

fn source_top_level_function<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source.find(signature).expect("top-level function start");
    let remainder = &source[start..];
    let end = remainder
        .find("\n}\n")
        .map(|offset| offset + 3)
        .expect("top-level function end");
    &remainder[..end]
}

fn assert_exact_ignored_probe(source: &str, probe: &str, workload_runner: &str) {
    let signature = format!("fn {probe}()");
    assert_eq!(source.matches(&signature).count(), 1);
    let start = source.find(&signature).expect("exact ignored probe");
    let attributes = &source[start.saturating_sub(300)..start];
    assert!(attributes.contains("#[test]"));
    assert!(attributes.contains("#[ignore"));

    let remainder = &source[start..];
    let end = remainder
        .find("\n}\n")
        .map(|offset| offset + 3)
        .expect("exact ignored probe body");
    let probe = &remainder[..end];
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
fn frozen_projection_is_post_semantic_and_encodes_every_observable_owner_and_terminal() {
    let wu0b = include_str!("wu0b_library.rs");
    let run = source_section(
        wu0b,
        "pub(crate) fn run_injected_profile(",
        "fn validate_parser_export_claims(",
    );
    let finish = run
        .find("finish_semantic_effects(&mut pass)")
        .expect("semantic effects must finish");
    let projection = run
        .find("canonical_frozen_library_product(")
        .expect("frozen product projection call");
    let return_product = run
        .find("wu0d_semantic_components")
        .expect("semantic components return");
    assert!(
        finish < projection && projection < return_product,
        "the projection must observe finished semantics while Pass/interner are still alive",
    );
    let projection_call = &run[projection..run.len().min(projection + 2_000)];
    assert!(projection_call.contains("pass.interner.store()"));
    assert!(projection_call.contains("pass.type_environment.published()"));
    if let Some(drop_pass) = run.find("drop(pass)") {
        assert!(projection < drop_pass);
    }
    for collector in [
        "collect_type_probes(",
        "collect_value_probes(",
        "collect_module_value_probes(",
    ] {
        let collected = run
            .find(collector)
            .unwrap_or_else(|| panic!("missing post-semantic {collector}"));
        assert!(
            finish < collected && collected < projection,
            "{collector} must observe the finished Pass",
        );
    }

    let projection_body = source_section(
        wu0b,
        "fn canonical_frozen_library_product(",
        "fn canonical_input_for_record(",
    );
    let compact = without_whitespace(projection_body);
    for received in [
        "source_records:",
        "type_publications:",
        "global_values:",
        "module_values:",
        "namespace_values:",
        "classes:",
        "store:",
    ] {
        assert!(
            compact.contains(received),
            "frozen projection does not explicitly receive {received}",
        );
    }
    for encoded in [
        "file_ordinal",
        "source_sha256",
        "source_bytes",
        "declaration_count",
        "participant_identities",
        "callable_members",
        "group_id",
        "TypeGroupUnavailableCause::UnsupportedComposition",
        "classes.canonical_terminals()",
        "CanonicalPublishedClassTerminal::Ready",
        "CanonicalPublishedClassTerminal::HeritagePoison",
        "CanonicalPublishedClassTerminal::InitializerPoison",
        "CanonicalPublishedClassTerminal::SurfacePoison",
        "class.0",
    ] {
        assert!(
            projection_body.contains(encoded),
            "frozen projection does not encode {encoded}",
        );
    }
    assert!(
        projection_body.contains("PublishedTypeGroupTerminal::Unavailable(unavailable)"),
        "unavailable group identity and cause must not collapse to one tag",
    );

    for section in [
        "SourceRecords",
        "TypeStore",
        "TypePublications",
        "GlobalValues",
        "ModuleValues",
        "NamespaceValues",
        "Classes",
    ] {
        assert!(
            projection_body.contains(&format!("Wu0dFrozenProductSection::{section}")),
            "missing named frozen-product section {section}",
        );
    }
    let frame = source_window(wu0b, "fn push_frozen_product_section(", 1_500);
    assert!(frame.contains("section.tag()"));
    assert!(frame.contains("u64::try_from"));
    assert!(frame.contains("payload.len()"));
    assert!(frame.contains("to_be_bytes()"));
    assert!(projection_body.contains("Wu0dFrozenLibraryProduct"));
    assert!(projection_body.contains("Wu0dFrozenProductSectionDescriptor"));
    assert!(projection_body.contains("Range<usize>"));

    let accessor = source_window(wu0b, "fn wu0d_frozen_product_section_for_test(", 1_500);
    assert!(accessor.contains("frozen_library_product"));
    assert!(accessor.contains("parse_frozen_product_section_range"));
    let compact_accessor = without_whitespace(accessor);
    assert!(compact_accessor.contains("parse_frozen_product_section_range(product,section)"));
    assert!(compact_accessor.contains("&product[range]"));
    assert!(!accessor.contains("to_vec()"));
    assert!(!accessor.contains("CanonicalBytes::new"));

    let range_parser = source_window(wu0b, "fn parse_frozen_product_section_range(", 2_500);
    assert!(range_parser.contains("requested.tag()"));
    assert!(range_parser.contains("u64::from_be_bytes"));
    assert!(range_parser.contains("Wu0dFrozenProductSection::ALL"));

    let class_accessor = source_window(wu0b, "fn wu0d_decoded_class_terminals_for_test(", 1_500);
    let compact_class_accessor = without_whitespace(class_accessor);
    assert!(compact_class_accessor.contains(
        "letsection=self.wu0d_frozen_product_section_for_test(Wu0dFrozenProductSection::Classes)"
    ));
    assert!(compact_class_accessor.contains("decode_wu0d_class_terminals_for_test(section)"));

    let class_decoder = source_top_level_function(wu0b, "fn decode_wu0d_class_terminals_for_test(");
    assert!(without_whitespace(class_decoder)
        .contains("fndecode_wu0d_class_terminals_for_test(section:&[u8])"));
    assert!(class_decoder.contains("u32::from_be_bytes"));
    assert!(class_decoder.contains("u64::from_be_bytes"));
    for terminal in [
        "Wu0dDecodedClassTerminal::Ready",
        "Wu0dDecodedClassTerminal::HeritagePoison",
        "Wu0dDecodedClassTerminal::InitializerPoison",
        "Wu0dDecodedClassTerminal::SurfacePoison",
    ] {
        assert!(class_decoder.contains(terminal));
    }
    for forbidden in [
        "canonical_terminals",
        "CanonicalPublishedClassTerminal",
        "PublishedClasses",
        "CanonicalBytes::new",
        "push_frozen_product_section",
    ] {
        assert!(
            !class_decoder.contains(forbidden),
            "class decoder must not consult side data or an alternate encoder: {forbidden}",
        );
    }
}

#[test]
fn evidence_validator_source_shape_is_bounded_strict_and_has_one_real_entry_point() {
    let implementation = include_str!("wu0d_candidate_release.rs");
    for required in [
        "MAX_RELEASE_EVIDENCE_BYTES",
        "MAX_RELEASE_STDOUT_BYTES",
        "fn parse_candidate_release_evidence(",
        "fn validate_candidate_b_release_evidence_file(",
        "fn render_candidate_release_validation(",
        "is_absolute",
        "evaluate_candidate_b_release(",
    ] {
        assert!(
            implementation.contains(required),
            "release evidence seam misses {required}",
        );
    }
    assert_eq!(
        implementation
            .matches("fn validate_candidate_b_release_evidence_file(")
            .count(),
        1,
    );
    let unix_reader = source_top_level_function(
        implementation,
        "#[cfg(unix)]\nfn read_candidate_release_evidence_file_bounded(",
    );
    for required in [
        "#[cfg(unix)]",
        "rustix::fs",
        "OFlags::RDONLY",
        "OFlags::NOFOLLOW",
        "OFlags::NONBLOCK",
        "OFlags::CLOEXEC",
        "Mode::empty()",
        "fstat(&descriptor)",
        "FileType::from_raw_mode(stat.st_mode).is_file()",
        "u64::try_from(stat.st_size)",
        "MAX_RELEASE_EVIDENCE_BYTES",
        "Read::take",
        "read_to_end",
    ] {
        assert!(
            unix_reader.contains(required),
            "Unix evidence reader misses {required}",
        );
    }
    let open = unix_reader.find("let descriptor = open(").unwrap();
    let fstat = unix_reader.find("fstat(&descriptor)").unwrap();
    let read = unix_reader.find("read_to_end").unwrap();
    assert!(
        open < fstat && fstat < read,
        "the path must be opened safely before handle metadata and bounded reads",
    );
    for forbidden in [
        "symlink_metadata",
        "std::fs::read(",
        "fs::read(",
        "std::fs::File::open(",
        "File::open(",
    ] {
        assert!(
            !unix_reader.contains(forbidden),
            "Unix evidence reader must not perform a blocking path operation: {forbidden}",
        );
    }

    let non_unix_reader = source_window(
        implementation,
        "#[cfg(not(unix))]\nfn read_candidate_release_evidence_file_bounded(",
        1_000,
    );
    assert!(non_unix_reader
        .contains("WU0D release evidence validation is unsupported on non-Unix hosts"));
    assert!(!non_unix_reader.contains("std::fs::read"));
    assert!(!non_unix_reader.contains("File::open"));

    let implementation_validator = source_top_level_function(
        implementation,
        "fn validate_candidate_b_release_evidence_file(",
    );
    assert!(implementation_validator.contains("read_candidate_release_evidence_file_bounded"));
    assert!(!implementation_validator.contains("symlink_metadata"));
    assert!(!implementation_validator.contains("File::open"));
    let validator = source_window(
        include_str!("wu0d_candidate_release_spec.rs"),
        "fn wu0d_candidate_release_validate_evidence_file()",
        1_200,
    );
    assert!(validator.contains("TYPOKAT_WU0D_RELEASE_EVIDENCE_PATH"));
    assert!(validator.contains("validate_candidate_b_release_evidence_file"));
    assert!(validator.contains("render_candidate_release_validation"));
    assert!(validator.contains("authorizes_candidate_b"));
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
        ("fn run_candidate_primary_workload(", "load_strict_profile"),
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

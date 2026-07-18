//! Disabled RED acceptance contract for the test-only WU0E diagnostic round.
//!
//! Activate this file only after its independent spec commit. WU0E diagnoses the WU0D primary
//! deadline; it does not authorize full-library loading, alter production, or weaken WU0D.
//!
//! ## One live-order pipeline
//!
//! The three exact modes `plain`, `measured-off`, and `candidate-b` use one strict-profile loader
//! and one canonical WU0B compiler body. The observed body preserves the following live call order.
//! `StatementFile` repeats once per canonical file ordinal; every other phase occurs once.
//!
//!  0. `profile-load`
//!  1. `parse`
//!  2. `bind`
//!  3. `lexical-reservation`
//!  4. `type-reservation` (type reservation, owner/class attachments, callable reservation)
//!  5. `pass-construction`
//!  6. `fill-parameter-metadata`
//!  7. `fill-interface-scc`
//!  8. `fill-conditional`
//!  9. `fill-mapped`
//! 10. `fill-object`
//! 11. `fill-remaining`
//! 12. `prepare-attached-namespace-values`
//! 13. `prepare-standalone-namespace-values`
//! 14. `publish-class-surfaces`
//! 15. `finalize-standalone-namespace-values`
//! 16. `precompute-standalone-namespace-aliases`
//! 17. `fill-pending-interfaces`
//! 18. `publish-type-groups`
//! 19. `validate-published-class-surfaces`
//! 20. `capture-lexical-evidence`
//! 21. `statement-file` (one phase instance per file, in canonical ordinal order)
//! 22. `semantic-finalization` (the sole `finish_semantic_effects` call)
//! 23. `collect-type-probes`
//! 24. `collect-global-value-probes`
//! 25. `collect-module-value-probes`
//! 26. `collect-namespace-value-probes`
//! 27. `freeze-library-product`
//! 28. `complete-semantic-batches`
//! 29. `consume-binder-outcomes`
//! 30. `finish-ledger` (snapshot, then finish)
//! 31. `build-semantic-components`
//! 32. `semantic-digest` (the existing canonical WU0D component digest)
//!
//! This only splits existing loops/direct calls; it must not reorder semantic work. The ordinary
//! `run_injected_profile` delegates to the same body with a disabled observer. WU0E contains no
//! parser, binder, checker, finalizer, projection, or semantic serializer clone.
//!
//! ## Observer and synchronous boundary commit
//!
//! The observer is a fixed array of timestamps/states. Its callback only reads the monotonic clock
//! and updates one existing slot: no allocation, formatting, I/O, locks, semantic inspection, or
//! hook in recursive substitution/evaluation/inference/relation/class publication. Immediately
//! after the callback returns, a separate boundary adapter formats, writes, and flushes one bounded
//! ASCII record before semantic execution resumes. It does not `fsync`. A recorded phase duration
//! is therefore a diagnostic upper bound that includes its one bounded enter-record flush; the
//! coordinator's monotonic wall time remains authoritative.
//! Every live transition and the controllable boundary fixture call the same
//! `DiagnosticBoundaryAdapter::enter_phase` / `exit_phase` API; neither may call observer or sink
//! primitives directly.
//!
//! `plain` installs neither cache measurement scope and never captures their counters.
//! `measured-off` and `candidate-b` both install the existing eager-cache scope plus, respectively,
//! the existing cycle-tainted baseline or Candidate-B scope. Both pairs start after profile loading
//! and before `Parse`, and end after `SemanticDigest`; no other mode-dependent compiler path exists.
//!
//! ## Protocol, termination, and validation
//!
//! Each LF-terminated ASCII record starts `typokat-wu0e-diagnostic-v1`. Integers are canonical
//! unsigned decimal; SHA-256 is lowercase hex; field order is exact. Phase records carry a monotonic
//! sequence and `item=none`, except `statement-file`, whose item is the exact file ordinal. The
//! measurement record reproduces the complete WU0D CandidateSummary counter surface, including
//! clean-cache metrics and mandatory `saturated=0`. `plain` emits no measurement.
//! The exclusive regular-file sink is opened before `run-start`; sink creation failure starts no
//! compiler work.
//!
//! A normal run must contain start, the exact expanded phase plan, optional measurement, semantic,
//! and finish. Externally proven `deadline`, `rss`, `stdout`, `stderr`, or `trace` containment may
//! validate the maximal LF-terminated state-machine prefix and a possible non-LF truncated tail.
//! `crash` and `infrastructure` never permit partial acceptance. A complete LF-terminated malformed
//! record always rejects; no malformed line is treated as truncation. Partial traces never infer a
//! semantic digest or a completed phase.
//!
//! After every workload, the coordinator invokes a distinct ignored WU0E trace validator through
//! the same frozen libtest. Its closed validation environment carries the trace path, expected mode,
//! and exact externally observed termination. The validator derives the exact statement ordinals
//! from the pinned strict profile rather than trusting an environment label. It accepts complete or
//! explicitly contained prefixes and prints a WU0E-only validation line. It neither imports/calls
//! the WU0D evaluator nor creates WU0D release evidence.
//!
//! ## External runner
//!
//! `tooling/wu0e-diagnostic/run.pl` builds and freezes one release libtest and pins one binary,
//! host, strict profile, and complete 88-file warm-inventory identity for all three fresh process
//! groups and their validators. Fixed bounds are 180 seconds, 1 GiB summed live non-zombie process-
//! group RSS, 128 KiB independently for stdout/stderr, and 256 KiB trace. Supervision retains an
//! exited leader as a zombie while descendants remain, samples group RSS at intervals no greater
//! than 10 ms,
//! terminates the whole group on any bound, drains boundedly, and verifies the binary before/after
//! every launch. It scrubs inherited WU0B–WU0E variables and installs only the exact workload or
//! validator allowlist. Its dossier is diagnostic only: completed mode digests must match, while a
//! killed digest is `unavailable`.
//! `BoundedPostRead` proves every final stdout/stderr/trace/time read is independently limit+1
//! bounded. `RssSamplingFailure` proves unreadable, missing, or malformed live-member RSS is an
//! infrastructure termination rather than zero. `RssArithmeticOverflow` proves page/byte and group
//! sums fail closed instead of wrapping.
//!
//! On Unix, trace validation opens only an absolute regular non-symlink path through a real
//! non-symlink parent, reads at most `MAX_DIAGNOSTIC_TRACE_BYTES + 1`, and compares pre/post opened-
//! handle and pathname device, inode, and size. Growth, replacement, disappearance, or metadata
//! drift rejects. Directory and FIFO inputs reject before a blocking read. On non-Unix the validator
//! is unsupported and fails closed without reading.

use super::wu0b_library::InjectedLibrarySource;
#[cfg(unix)]
use super::wu0e_diagnostic::validate_diagnostic_trace_file_with_post_read_hook_for_test;
use super::wu0e_diagnostic::RunnerSelfTestCase;
use super::wu0e_diagnostic::{
    parse_diagnostic_trace, parse_runner_self_test_report, reconcile_measurements,
    render_diagnostic_event_for_test, resolve_diagnostic_environment_bytes_for_test,
    resolve_diagnostic_environment_for_test, resolve_diagnostic_validation_environment_for_test,
    run_boundary_fixture_for_test, run_observed_profile_for_test,
    strict_profile_statement_ordinals_for_test, validate_completed_semantic_parity,
    validate_diagnostic_trace_file, BoundaryFixtureObservation, BoundaryTestClock,
    BoundaryTestSink, DiagnosticEnvironment, DiagnosticEvent, DiagnosticMeasurement,
    DiagnosticMode, DiagnosticPhase, DiagnosticPhaseKey, DiagnosticTermination,
    DiagnosticTraceSink, MeasurementSelection, ParsedDiagnosticTrace, MAX_DIAGNOSTIC_TRACE_BYTES,
};
use crate::source::LibraryFileOrdinal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

const PREFIX: &str = "typokat-wu0e-diagnostic-v1";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

const PHASES: [(DiagnosticPhase, &str); 33] = [
    (DiagnosticPhase::ProfileLoad, "profile-load"),
    (DiagnosticPhase::Parse, "parse"),
    (DiagnosticPhase::Bind, "bind"),
    (DiagnosticPhase::LexicalReservation, "lexical-reservation"),
    (DiagnosticPhase::TypeReservation, "type-reservation"),
    (DiagnosticPhase::PassConstruction, "pass-construction"),
    (
        DiagnosticPhase::FillParameterMetadata,
        "fill-parameter-metadata",
    ),
    (DiagnosticPhase::FillInterfaceScc, "fill-interface-scc"),
    (DiagnosticPhase::FillConditional, "fill-conditional"),
    (DiagnosticPhase::FillMapped, "fill-mapped"),
    (DiagnosticPhase::FillObject, "fill-object"),
    (DiagnosticPhase::FillRemaining, "fill-remaining"),
    (
        DiagnosticPhase::PrepareAttachedNamespaceValues,
        "prepare-attached-namespace-values",
    ),
    (
        DiagnosticPhase::PrepareStandaloneNamespaceValues,
        "prepare-standalone-namespace-values",
    ),
    (
        DiagnosticPhase::PublishClassSurfaces,
        "publish-class-surfaces",
    ),
    (
        DiagnosticPhase::FinalizeStandaloneNamespaceValues,
        "finalize-standalone-namespace-values",
    ),
    (
        DiagnosticPhase::PrecomputeStandaloneNamespaceAliases,
        "precompute-standalone-namespace-aliases",
    ),
    (
        DiagnosticPhase::FillPendingInterfaces,
        "fill-pending-interfaces",
    ),
    (DiagnosticPhase::PublishTypeGroups, "publish-type-groups"),
    (
        DiagnosticPhase::ValidatePublishedClassSurfaces,
        "validate-published-class-surfaces",
    ),
    (
        DiagnosticPhase::CaptureLexicalEvidence,
        "capture-lexical-evidence",
    ),
    (DiagnosticPhase::StatementFile, "statement-file"),
    (
        DiagnosticPhase::SemanticFinalization,
        "semantic-finalization",
    ),
    (DiagnosticPhase::CollectTypeProbes, "collect-type-probes"),
    (
        DiagnosticPhase::CollectGlobalValueProbes,
        "collect-global-value-probes",
    ),
    (
        DiagnosticPhase::CollectModuleValueProbes,
        "collect-module-value-probes",
    ),
    (
        DiagnosticPhase::CollectNamespaceValueProbes,
        "collect-namespace-value-probes",
    ),
    (
        DiagnosticPhase::FreezeLibraryProduct,
        "freeze-library-product",
    ),
    (
        DiagnosticPhase::CompleteSemanticBatches,
        "complete-semantic-batches",
    ),
    (
        DiagnosticPhase::ConsumeBinderOutcomes,
        "consume-binder-outcomes",
    ),
    (DiagnosticPhase::FinishLedger, "finish-ledger"),
    (
        DiagnosticPhase::BuildSemanticComponents,
        "build-semantic-components",
    ),
    (DiagnosticPhase::SemanticDigest, "semantic-digest"),
];

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "typokat-wu0e-{label}-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create owned WU0E scratch directory");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn environment(entries: &[(&str, &str)]) -> Result<DiagnosticEnvironment, String> {
    resolve_diagnostic_environment_for_test(entries.iter().copied())
        .map_err(|error| error.to_string())
}

fn phase_plan(statement_ordinals: &[u32]) -> Vec<DiagnosticPhaseKey> {
    let mut plan = PHASES[..21]
        .iter()
        .map(|(phase, _)| DiagnosticPhaseKey::singleton(*phase))
        .collect::<Vec<_>>();
    plan.extend(
        statement_ordinals
            .iter()
            .copied()
            .map(DiagnosticPhaseKey::statement_file),
    );
    plan.extend(
        PHASES[22..]
            .iter()
            .map(|(phase, _)| DiagnosticPhaseKey::singleton(*phase)),
    );
    plan
}

fn off_measurement() -> DiagnosticMeasurement {
    DiagnosticMeasurement {
        eligible_requests: 100,
        executed_runs: 100,
        tainted_cache_hits: 0,
        clean_outcomes: 10,
        tainted_outcomes: 90,
        executed_visits: 1_000,
        memo_hits: 300,
        expanded_visits: 700,
        avoided_visits: 0,
        tainted_cache_entries: 0,
        clean_cache_hits: 71,
        clean_cache_misses: 29,
        clean_cache_entries: 23,
        saturated: false,
    }
}

fn candidate_measurement() -> DiagnosticMeasurement {
    DiagnosticMeasurement {
        eligible_requests: 100,
        executed_runs: 20,
        tainted_cache_hits: 80,
        clean_outcomes: 10,
        tainted_outcomes: 10,
        executed_visits: 200,
        memo_hits: 60,
        expanded_visits: 140,
        avoided_visits: 800,
        tainted_cache_entries: 3,
        clean_cache_hits: 71,
        clean_cache_misses: 29,
        clean_cache_entries: 23,
        saturated: false,
    }
}

fn complete_events(
    mode: DiagnosticMode,
    statement_ordinals: &[u32],
    semantic: &str,
) -> Vec<DiagnosticEvent> {
    let mut elapsed_us = 0;
    let mut events = vec![DiagnosticEvent::RunStart { mode, elapsed_us }];
    for (sequence, key) in phase_plan(statement_ordinals).into_iter().enumerate() {
        elapsed_us += 10;
        events.push(DiagnosticEvent::PhaseEnter {
            sequence,
            key,
            elapsed_us,
        });
        elapsed_us += 10;
        events.push(DiagnosticEvent::PhaseExit {
            sequence,
            key,
            elapsed_us,
        });
    }
    let measurement = match mode {
        DiagnosticMode::Plain => None,
        DiagnosticMode::MeasuredOff => Some(off_measurement()),
        DiagnosticMode::CandidateB => Some(candidate_measurement()),
    };
    if let Some(measurement) = measurement {
        elapsed_us += 10;
        events.push(DiagnosticEvent::Measurement {
            mode,
            measurement,
            elapsed_us,
        });
    }
    elapsed_us += 10;
    events.push(DiagnosticEvent::Semantic {
        aggregate_sha256: semantic.to_owned(),
        elapsed_us,
    });
    elapsed_us += 10;
    events.push(DiagnosticEvent::RunFinish { elapsed_us });
    events
}

fn trace_bytes(events: &[DiagnosticEvent]) -> Vec<u8> {
    let mut text = events
        .iter()
        .map(render_diagnostic_event_for_test)
        .collect::<Vec<_>>()
        .join("\n");
    text.push('\n');
    text.into_bytes()
}

fn parse_complete(
    mode: DiagnosticMode,
    statement_ordinals: &[u32],
    semantic: &str,
) -> ParsedDiagnosticTrace {
    parse_diagnostic_trace(
        &trace_bytes(&complete_events(mode, statement_ordinals, semantic)),
        mode,
        statement_ordinals,
        DiagnosticTermination::Normal,
    )
    .expect("canonical completed trace")
}

fn function_window<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("source window start");
    let end = source[start..]
        .find(end)
        .map(|offset| start + offset)
        .expect("source window end");
    &source[start..end]
}

fn adapter_call_positions(source: &str, method: &str, phase: DiagnosticPhase) -> Vec<usize> {
    let phase = format!("DiagnosticPhase::{phase:?}");
    let item = if phase.ends_with("StatementFile") {
        "Some("
    } else {
        "None"
    };
    source
        .match_indices(method)
        .filter_map(|(index, _)| {
            let statement = &source[index..];
            let end = statement.find(';')?;
            let statement = &statement[..=end];
            (statement.contains(&phase) && statement.contains(item)).then_some(index)
        })
        .collect()
}

fn assert_phase_encloses(
    source: &str,
    phase: DiagnosticPhase,
    first_live_anchor: &str,
    last_live_anchor: &str,
) -> usize {
    let enters = adapter_call_positions(source, "boundary.enter_phase(", phase);
    let exits = adapter_call_positions(source, "boundary.exit_phase(", phase);
    assert_eq!(
        enters.len(),
        1,
        "{phase:?} must have exactly one shared-adapter enter call"
    );
    assert_eq!(
        exits.len(),
        1,
        "{phase:?} must have exactly one shared-adapter exit call"
    );
    let first = source.find(first_live_anchor).expect("first live anchor");
    let last = source[first..]
        .find(last_live_anchor)
        .map(|offset| first + offset)
        .expect("last live anchor");
    assert!(
        enters[0] < first && first <= last && last < exits[0],
        "{phase:?} transitions do not enclose live work"
    );
    enters[0]
}

#[test]
fn phase_enum_tokens_and_expanded_live_order_are_exact() {
    assert_eq!(DiagnosticPhase::ALL.len(), PHASES.len());
    for (ordinal, (phase, token)) in PHASES.iter().copied().enumerate() {
        assert_eq!(DiagnosticPhase::ALL[ordinal], phase);
        assert_eq!(phase.ordinal(), ordinal);
        assert_eq!(phase.as_str(), token);
        assert_eq!(DiagnosticPhase::parse(token), Some(phase));
    }
    let plan = phase_plan(&[0, 7, 81]);
    assert_eq!(plan.len(), PHASES.len() - 1 + 3);
    assert_eq!(plan[20].phase(), DiagnosticPhase::CaptureLexicalEvidence);
    assert_eq!(plan[21], DiagnosticPhaseKey::statement_file(0));
    assert_eq!(plan[22], DiagnosticPhaseKey::statement_file(7));
    assert_eq!(plan[23], DiagnosticPhaseKey::statement_file(81));
    assert_eq!(plan[24].phase(), DiagnosticPhase::SemanticFinalization);
    assert_eq!(
        plan.last().copied(),
        Some(DiagnosticPhaseKey::singleton(
            DiagnosticPhase::SemanticDigest
        ))
    );
}

#[test]
fn modes_install_exact_scope_pairs_without_an_implicit_default() {
    let cases = [
        (DiagnosticMode::Plain, "plain", MeasurementSelection::None),
        (
            DiagnosticMode::MeasuredOff,
            "measured-off",
            MeasurementSelection::EagerAndCycleBaseline,
        ),
        (
            DiagnosticMode::CandidateB,
            "candidate-b",
            MeasurementSelection::EagerAndCandidateB,
        ),
    ];
    for (mode, token, selection) in cases {
        assert_eq!(mode.as_str(), token);
        assert_eq!(DiagnosticMode::parse(token), Some(mode));
        assert_eq!(mode.measurement_selection(), selection);
    }
    for invalid in ["", "off", "candidate-b-v1", "Plain", "measured"] {
        assert_eq!(DiagnosticMode::parse(invalid), None);
    }
}

#[test]
fn workload_environment_is_closed_and_explicit() {
    for mode in ["plain", "measured-off", "candidate-b"] {
        let resolved = environment(&[
            ("TYPOKAT_WU0E_MODE", mode),
            ("TYPOKAT_WU0E_TRACE_PATH", "/tmp/wu0e/trace.log"),
        ])
        .unwrap();
        assert_eq!(resolved.mode.as_str(), mode);
        assert_eq!(resolved.trace_path, Path::new("/tmp/wu0e/trace.log"));
    }
    for invalid in [
        vec![],
        vec![("TYPOKAT_WU0E_MODE", "plain")],
        vec![("TYPOKAT_WU0E_TRACE_PATH", "/tmp/wu0e/trace.log")],
        vec![
            ("TYPOKAT_WU0E_MODE", "off"),
            ("TYPOKAT_WU0E_TRACE_PATH", "/tmp/wu0e/trace.log"),
        ],
        vec![
            ("TYPOKAT_WU0E_MODE", "plain"),
            ("TYPOKAT_WU0E_TRACE_PATH", "trace.log"),
        ],
        vec![
            ("TYPOKAT_WU0E_MODE", "plain"),
            ("TYPOKAT_WU0E_TRACE_PATH", "/tmp/wu0e/trace.log"),
            ("TYPOKAT_WU0E_EXTRA", "1"),
        ],
        vec![
            ("TYPOKAT_WU0E_MODE", "plain"),
            ("TYPOKAT_WU0E_MODE", "plain"),
            ("TYPOKAT_WU0E_TRACE_PATH", "/tmp/wu0e/trace.log"),
        ],
    ] {
        assert!(environment(&invalid).is_err());
    }
}

#[test]
fn workload_environment_rejects_non_utf8() {
    let valid = [
        (b"TYPOKAT_WU0E_MODE".as_slice(), b"plain".as_slice()),
        (
            b"TYPOKAT_WU0E_TRACE_PATH".as_slice(),
            b"/tmp/wu0e/trace.log".as_slice(),
        ),
    ];
    assert!(resolve_diagnostic_environment_bytes_for_test(valid).is_ok());
    let invalid_name = [
        (b"TYPOKAT_WU0E_MOD\xff".as_slice(), b"plain".as_slice()),
        (
            b"TYPOKAT_WU0E_TRACE_PATH".as_slice(),
            b"/tmp/wu0e/trace.log".as_slice(),
        ),
    ];
    assert!(resolve_diagnostic_environment_bytes_for_test(invalid_name).is_err());
    let invalid_value = [
        (b"TYPOKAT_WU0E_MODE".as_slice(), b"plain\xff".as_slice()),
        (
            b"TYPOKAT_WU0E_TRACE_PATH".as_slice(),
            b"/tmp/wu0e/trace.log".as_slice(),
        ),
    ];
    assert!(resolve_diagnostic_environment_bytes_for_test(invalid_value).is_err());
}

#[test]
fn validator_environment_has_a_separate_exact_allowlist() {
    let valid = [
        ("TYPOKAT_WU0E_VALIDATE_TRACE_PATH", "/tmp/wu0e/trace.log"),
        ("TYPOKAT_WU0E_VALIDATE_MODE", "candidate-b"),
        ("TYPOKAT_WU0E_VALIDATE_TERMINATION", "rss"),
    ];
    let resolved = resolve_diagnostic_validation_environment_for_test(valid).unwrap();
    assert_eq!(resolved.mode, DiagnosticMode::CandidateB);
    assert_eq!(resolved.termination, DiagnosticTermination::Rss);
    for (key, value) in valid {
        let mut missing = valid.to_vec();
        missing.retain(|(candidate, _)| *candidate != key);
        assert!(resolve_diagnostic_validation_environment_for_test(missing).is_err());
        let mut duplicate = valid.to_vec();
        duplicate.push((key, value));
        assert!(resolve_diagnostic_validation_environment_for_test(duplicate).is_err());
    }
    let mut unknown = valid.to_vec();
    unknown.push(("TYPOKAT_WU0E_EXTRA", "1"));
    assert!(resolve_diagnostic_validation_environment_for_test(unknown).is_err());
}

#[test]
fn validator_derives_the_exact_82_statement_ordinals_from_the_strict_profile() {
    let ordinals = strict_profile_statement_ordinals_for_test().unwrap();
    let expected = (0..82)
        .map(|ordinal| u32::try_from(ordinal).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ordinals, expected);
}

#[test]
fn protocol_renders_phase_keys_and_full_measurement_surface_exactly() {
    assert_eq!(
        render_diagnostic_event_for_test(&DiagnosticEvent::PhaseEnter {
            sequence: 21,
            key: DiagnosticPhaseKey::statement_file(7),
            elapsed_us: 123,
        }),
        "typokat-wu0e-diagnostic-v1 event=phase-enter sequence=21 phase=statement-file item=7 elapsed_us=123"
    );
    assert_eq!(
        render_diagnostic_event_for_test(&DiagnosticEvent::Measurement {
            mode: DiagnosticMode::MeasuredOff,
            measurement: off_measurement(),
            elapsed_us: 456,
        }),
        "typokat-wu0e-diagnostic-v1 event=measurement mode=measured-off eligible_requests=100 executed_runs=100 tainted_cache_hits=0 clean_outcomes=10 tainted_outcomes=90 executed_visits=1000 memo_hits=300 expanded_visits=700 avoided_visits=0 tainted_cache_entries=0 clean_cache_hits=71 clean_cache_misses=29 clean_cache_entries=23 saturated=0 elapsed_us=456"
    );
}

#[test]
fn completed_traces_require_exact_dynamic_phase_coverage() {
    for mode in [
        DiagnosticMode::Plain,
        DiagnosticMode::MeasuredOff,
        DiagnosticMode::CandidateB,
    ] {
        let parsed = parse_complete(mode, &[0, 1], SHA_A);
        assert_eq!(parsed.mode(), mode);
        assert_eq!(parsed.completed_phases(), phase_plan(&[0, 1]));
        assert_eq!(parsed.active_phase(), None);
        assert_eq!(parsed.semantic_sha256(), Some(SHA_A));
        assert!(parsed.finished());
        assert_eq!(
            parsed.measurement().is_some(),
            mode != DiagnosticMode::Plain
        );
    }

    let mut missing = complete_events(DiagnosticMode::Plain, &[0, 1], SHA_A);
    missing.remove(8);
    assert!(parse_diagnostic_trace(
        &trace_bytes(&missing),
        DiagnosticMode::Plain,
        &[0, 1],
        DiagnosticTermination::Normal,
    )
    .is_err());
    assert!(parse_diagnostic_trace(
        &trace_bytes(&complete_events(DiagnosticMode::Plain, &[0, 1], SHA_A)),
        DiagnosticMode::Plain,
        &[1, 0],
        DiagnosticTermination::Normal,
    )
    .is_err());
}

#[test]
fn measurement_arithmetic_and_pair_reconciliation_match_wu0d() {
    assert!(off_measurement()
        .validate(DiagnosticMode::MeasuredOff)
        .is_ok());
    assert!(candidate_measurement()
        .validate(DiagnosticMode::CandidateB)
        .is_ok());
    assert!(reconcile_measurements(&off_measurement(), &candidate_measurement()).is_ok());

    let mut malformed = off_measurement();
    malformed.eligible_requests += 1;
    assert!(malformed.validate(DiagnosticMode::MeasuredOff).is_err());
    let mut malformed = off_measurement();
    malformed.executed_runs += 1;
    assert!(malformed.validate(DiagnosticMode::MeasuredOff).is_err());
    let mut malformed = off_measurement();
    malformed.executed_visits += 1;
    assert!(malformed.validate(DiagnosticMode::MeasuredOff).is_err());
    let mut malformed = off_measurement();
    malformed.tainted_cache_hits = 1;
    assert!(malformed.validate(DiagnosticMode::MeasuredOff).is_err());
    let mut malformed = candidate_measurement();
    malformed.saturated = true;
    assert!(malformed.validate(DiagnosticMode::CandidateB).is_err());
    let mut overflow = off_measurement();
    overflow.clean_cache_hits = u64::MAX;
    overflow.clean_cache_misses = 1;
    assert!(overflow.validate(DiagnosticMode::MeasuredOff).is_err());

    let changes: [fn(&mut DiagnosticMeasurement); 7] = [
        |value: &mut DiagnosticMeasurement| value.eligible_requests += 1,
        |value: &mut DiagnosticMeasurement| value.clean_outcomes += 1,
        |value: &mut DiagnosticMeasurement| value.tainted_outcomes += 1,
        |value: &mut DiagnosticMeasurement| value.executed_visits += 1,
        |value: &mut DiagnosticMeasurement| value.clean_cache_hits += 1,
        |value: &mut DiagnosticMeasurement| value.clean_cache_misses += 1,
        |value: &mut DiagnosticMeasurement| value.clean_cache_entries += 1,
    ];
    for changed in changes {
        let mut candidate = candidate_measurement();
        changed(&mut candidate);
        assert!(reconcile_measurements(&off_measurement(), &candidate).is_err());
    }
}

#[test]
fn plain_cannot_capture_and_measured_modes_cannot_omit_or_relabel() {
    let mut plain = complete_events(DiagnosticMode::Plain, &[0], SHA_A);
    let semantic = plain.len() - 2;
    plain.insert(
        semantic,
        DiagnosticEvent::Measurement {
            mode: DiagnosticMode::MeasuredOff,
            measurement: off_measurement(),
            elapsed_us: 1_000,
        },
    );
    assert!(parse_diagnostic_trace(
        &trace_bytes(&plain),
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
    )
    .is_err());

    let mut measured = complete_events(DiagnosticMode::MeasuredOff, &[0], SHA_A);
    measured.retain(|event| !matches!(event, DiagnosticEvent::Measurement { .. }));
    assert!(parse_diagnostic_trace(
        &trace_bytes(&measured),
        DiagnosticMode::MeasuredOff,
        &[0],
        DiagnosticTermination::Normal,
    )
    .is_err());
}

#[test]
fn only_explicit_external_containment_accepts_a_valid_prefix() {
    let events = complete_events(DiagnosticMode::MeasuredOff, &[0, 1], SHA_A);
    let prefix = trace_bytes(&events[..12]);
    for termination in [
        DiagnosticTermination::Deadline,
        DiagnosticTermination::Rss,
        DiagnosticTermination::Stdout,
        DiagnosticTermination::Stderr,
        DiagnosticTermination::Trace,
    ] {
        let parsed =
            parse_diagnostic_trace(&prefix, DiagnosticMode::MeasuredOff, &[0, 1], termination)
                .expect("externally contained valid prefix");
        assert!(!parsed.finished());
        assert_eq!(parsed.semantic_sha256(), None);
    }
    for termination in [
        DiagnosticTermination::Normal,
        DiagnosticTermination::Crash,
        DiagnosticTermination::Infrastructure,
    ] {
        assert!(
            parse_diagnostic_trace(&prefix, DiagnosticMode::MeasuredOff, &[0, 1], termination,)
                .is_err()
        );
    }
}

#[test]
fn containment_never_promotes_measurement_or_semantic_without_run_finish() {
    let events = complete_events(DiagnosticMode::MeasuredOff, &[0], SHA_A);
    let measurement = events
        .iter()
        .position(|event| matches!(event, DiagnosticEvent::Measurement { .. }))
        .expect("measurement event");
    let measured_prefix = parse_diagnostic_trace(
        &trace_bytes(&events[..=measurement]),
        DiagnosticMode::MeasuredOff,
        &[0],
        DiagnosticTermination::Stdout,
    )
    .expect("contained trace with complete measurement record");
    assert!(measured_prefix.measurement().is_some());
    assert_eq!(measured_prefix.semantic_sha256(), None);
    assert!(!measured_prefix.finished());

    let semantic = events
        .iter()
        .position(|event| matches!(event, DiagnosticEvent::Semantic { .. }))
        .expect("semantic event");
    let semantic_prefix = parse_diagnostic_trace(
        &trace_bytes(&events[..=semantic]),
        DiagnosticMode::MeasuredOff,
        &[0],
        DiagnosticTermination::Stderr,
    )
    .expect("contained trace with semantic record but no run-finish");
    assert!(semantic_prefix.measurement().is_some());
    assert_eq!(semantic_prefix.semantic_sha256(), None);
    assert!(!semantic_prefix.finished());
}

#[test]
fn truncated_tail_is_distinct_from_a_complete_malformed_record() {
    let events = complete_events(DiagnosticMode::CandidateB, &[0], SHA_A);
    let mut truncated = trace_bytes(&events[..7]);
    truncated.extend_from_slice(b"typokat-wu0e-diagnostic-v1 event=phase-exit sequ");
    let parsed = parse_diagnostic_trace(
        &truncated,
        DiagnosticMode::CandidateB,
        &[0],
        DiagnosticTermination::Deadline,
    )
    .expect("non-LF terminal bytes are retained only as a truncated tail");
    assert!(parsed.truncated_tail());
    assert_eq!(parsed.semantic_sha256(), None);

    let mut malformed = trace_bytes(&events[..7]);
    malformed.extend_from_slice(b"typokat-wu0e-diagnostic-v1 event=phase-exit sequ\n");
    assert!(parse_diagnostic_trace(
        &malformed,
        DiagnosticMode::CandidateB,
        &[0],
        DiagnosticTermination::Deadline,
    )
    .is_err());
}

#[test]
fn malformed_noncanonical_foreign_and_oversized_traces_reject() {
    assert_eq!(MAX_DIAGNOSTIC_TRACE_BYTES, 256 * 1_024);
    let valid = String::from_utf8(trace_bytes(&complete_events(
        DiagnosticMode::Plain,
        &[0],
        SHA_A,
    )))
    .unwrap();
    let malformed = [
        valid.replacen("elapsed_us=0", "elapsed_us=00", 1),
        valid.replacen(PREFIX, "typokat-wu0d-candidate-v1", 1),
        valid.replacen(" item=none", " extra=1 item=none", 1),
        valid.replacen("\n", "\r\n", 1),
        valid.replacen("\n", "\n\n", 1),
        valid.trim_end_matches('\n').to_owned(),
        valid.replacen(SHA_A, &SHA_A.to_uppercase(), 1),
    ];
    for bytes in malformed {
        assert!(parse_diagnostic_trace(
            bytes.as_bytes(),
            DiagnosticMode::Plain,
            &[0],
            DiagnosticTermination::Normal,
        )
        .is_err());
    }
    assert!(parse_diagnostic_trace(
        &vec![b'x'; MAX_DIAGNOSTIC_TRACE_BYTES + 1],
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Trace,
    )
    .is_err());
}

#[test]
fn timestamp_and_phase_state_mutations_fail_closed() {
    fn rejects(events: Vec<DiagnosticEvent>) {
        assert!(parse_diagnostic_trace(
            &trace_bytes(&events),
            DiagnosticMode::Plain,
            &[0],
            DiagnosticTermination::Normal,
        )
        .is_err());
    }

    let canonical = complete_events(DiagnosticMode::Plain, &[0], SHA_A);

    let mut exit_before_enter = canonical.clone();
    exit_before_enter[2] = DiagnosticEvent::PhaseExit {
        sequence: 0,
        key: DiagnosticPhaseKey::singleton(DiagnosticPhase::ProfileLoad),
        elapsed_us: 9,
    };
    rejects(exit_before_enter);

    let mut enter_before_prior_exit = canonical.clone();
    enter_before_prior_exit[3] = DiagnosticEvent::PhaseEnter {
        sequence: 1,
        key: DiagnosticPhaseKey::singleton(DiagnosticPhase::Parse),
        elapsed_us: 19,
    };
    rejects(enter_before_prior_exit);

    let mut semantic_before_prior = canonical.clone();
    let semantic = semantic_before_prior
        .iter()
        .position(|event| matches!(event, DiagnosticEvent::Semantic { .. }))
        .unwrap();
    semantic_before_prior[semantic] = DiagnosticEvent::Semantic {
        aggregate_sha256: SHA_A.to_owned(),
        elapsed_us: 1,
    };
    rejects(semantic_before_prior);

    let mut saturated_elapsed = canonical.clone();
    let finish = saturated_elapsed.len() - 1;
    saturated_elapsed[finish] = DiagnosticEvent::RunFinish {
        elapsed_us: u64::MAX,
    };
    rejects(saturated_elapsed);

    let mut duplicate_enter = canonical.clone();
    duplicate_enter.insert(2, canonical[1].clone());
    rejects(duplicate_enter);

    let mut duplicate_exit = canonical.clone();
    duplicate_exit.insert(3, canonical[2].clone());
    rejects(duplicate_exit);

    let mut swapped = canonical;
    swapped.swap(1, 2);
    rejects(swapped);

    let mut measured = complete_events(DiagnosticMode::MeasuredOff, &[0], SHA_A);
    let measurement = measured
        .iter()
        .position(|event| matches!(event, DiagnosticEvent::Measurement { .. }))
        .unwrap();
    measured[measurement] = DiagnosticEvent::Measurement {
        mode: DiagnosticMode::MeasuredOff,
        measurement: off_measurement(),
        elapsed_us: 1,
    };
    assert!(parse_diagnostic_trace(
        &trace_bytes(&measured),
        DiagnosticMode::MeasuredOff,
        &[0],
        DiagnosticTermination::Normal,
    )
    .is_err());
}

#[test]
fn semantic_parity_requires_three_completed_modes_in_exact_order() {
    let plain = parse_complete(DiagnosticMode::Plain, &[0], SHA_A);
    let measured = parse_complete(DiagnosticMode::MeasuredOff, &[0], SHA_A);
    let candidate = parse_complete(DiagnosticMode::CandidateB, &[0], SHA_A);
    assert_eq!(
        validate_completed_semantic_parity([&plain, &measured, &candidate]).unwrap(),
        SHA_A
    );
    let changed = parse_complete(DiagnosticMode::CandidateB, &[0], SHA_B);
    assert!(validate_completed_semantic_parity([&plain, &measured, &changed]).is_err());
    assert!(validate_completed_semantic_parity([&plain, &candidate, &measured]).is_err());

    let partial_events = complete_events(DiagnosticMode::CandidateB, &[0], SHA_A);
    let partial = parse_diagnostic_trace(
        &trace_bytes(&partial_events[..8]),
        DiagnosticMode::CandidateB,
        &[0],
        DiagnosticTermination::Rss,
    )
    .unwrap();
    assert_eq!(partial.semantic_sha256(), None);
    assert!(validate_completed_semantic_parity([&plain, &measured, &partial]).is_err());
}

#[test]
fn same_binary_validator_accepts_exact_complete_and_contained_prefixes() {
    let scratch = ScratchDir::new("validator");
    let complete_path = scratch.join("complete.trace");
    std::fs::write(
        &complete_path,
        trace_bytes(&complete_events(DiagnosticMode::Plain, &[0], SHA_A)),
    )
    .unwrap();
    let complete = validate_diagnostic_trace_file(
        &complete_path,
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
    )
    .unwrap();
    assert_eq!(
        complete.render(),
        format!("typokat-wu0e-validation-v1 mode=plain termination=normal status=complete semantic_sha256={SHA_A}")
    );

    let prefix_path = scratch.join("prefix.trace");
    let events = complete_events(DiagnosticMode::CandidateB, &[0], SHA_A);
    std::fs::write(&prefix_path, trace_bytes(&events[..8])).unwrap();
    let prefix = validate_diagnostic_trace_file(
        &prefix_path,
        DiagnosticMode::CandidateB,
        &[0],
        DiagnosticTermination::Deadline,
    )
    .unwrap();
    assert!(prefix.render().contains("status=partial"));
    assert!(prefix.render().contains("semantic_sha256=unavailable"));
    assert!(validate_diagnostic_trace_file(
        &prefix_path,
        DiagnosticMode::CandidateB,
        &[0],
        DiagnosticTermination::Crash,
    )
    .is_err());
}

#[test]
#[cfg(unix)]
fn validator_reader_rejects_relative_symlink_directory_and_oversized_inputs() {
    use std::os::unix::fs::symlink;

    assert!(validate_diagnostic_trace_file(
        Path::new("relative.trace"),
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
    )
    .is_err());

    let scratch = ScratchDir::new("validator-reader");
    assert!(validate_diagnostic_trace_file(
        &scratch.0,
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
    )
    .is_err());

    let target = scratch.join("target.trace");
    std::fs::write(
        &target,
        trace_bytes(&complete_events(DiagnosticMode::Plain, &[0], SHA_A)),
    )
    .unwrap();
    let link = scratch.join("link.trace");
    symlink(&target, &link).unwrap();
    assert!(validate_diagnostic_trace_file(
        &link,
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
    )
    .is_err());

    let real_parent = scratch.join("real-parent");
    std::fs::create_dir(&real_parent).unwrap();
    let parent_trace = real_parent.join("trace.log");
    std::fs::write(
        &parent_trace,
        trace_bytes(&complete_events(DiagnosticMode::Plain, &[0], SHA_A)),
    )
    .unwrap();
    let parent_link = scratch.join("parent-link");
    symlink(&real_parent, &parent_link).unwrap();
    assert!(validate_diagnostic_trace_file(
        &parent_link.join("trace.log"),
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
    )
    .is_err());

    let oversized = scratch.join("oversized.trace");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(u64::try_from(MAX_DIAGNOSTIC_TRACE_BYTES + 1).unwrap())
        .unwrap();
    drop(file);
    assert!(validate_diagnostic_trace_file(
        &oversized,
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Trace,
    )
    .is_err());
}

#[test]
#[cfg(target_os = "linux")]
fn validator_reader_rejects_fifo_without_blocking() {
    use rustix::fs::{mkfifoat, open, Mode, OFlags};
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::Duration;

    let scratch = ScratchDir::new("validator-fifo");
    let directory = open(
        &scratch.0,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    mkfifoat(&directory, "trace.fifo", Mode::RUSR | Mode::WUSR).unwrap();
    let fifo = scratch.join("trace.fifo");
    let worker_path = fifo.clone();
    let (sender, receiver) = channel();
    let worker = std::thread::spawn(move || {
        let rejected = validate_diagnostic_trace_file(
            &worker_path,
            DiagnosticMode::Plain,
            &[0],
            DiagnosticTermination::Infrastructure,
        )
        .is_err();
        let _ = sender.send(rejected);
    });
    match receiver.recv_timeout(Duration::from_secs(1)) {
        Ok(rejected) => {
            worker.join().unwrap();
            assert!(rejected);
        }
        Err(RecvTimeoutError::Timeout) => {
            let rescue = open(
                &fifo,
                OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .expect("open FIFO rescue handle without waiting for a reader");
            worker.join().unwrap();
            drop(rescue);
            panic!("validator blocked on FIFO before rejecting its file type");
        }
        Err(RecvTimeoutError::Disconnected) => {
            assert!(worker.join().is_ok());
            panic!("validator FIFO worker disconnected");
        }
    }
}

#[test]
#[cfg(unix)]
fn validator_reader_rejects_post_read_growth_and_path_replacement() {
    use std::io::Write;

    let bytes = trace_bytes(&complete_events(DiagnosticMode::Plain, &[0], SHA_A));
    let scratch = ScratchDir::new("validator-stability");

    let growing = scratch.join("growing.trace");
    std::fs::write(&growing, &bytes).unwrap();
    let growth_path = growing.clone();
    assert!(validate_diagnostic_trace_file_with_post_read_hook_for_test(
        &growing,
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
        move || {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&growth_path)?;
            file.write_all(b"x")?;
            file.flush()
        },
    )
    .is_err());

    let replaced = scratch.join("replaced.trace");
    let displaced = scratch.join("displaced.trace");
    std::fs::write(&replaced, &bytes).unwrap();
    let replacement_path = replaced.clone();
    let displaced_path = displaced.clone();
    let replacement_bytes = bytes.clone();
    assert!(validate_diagnostic_trace_file_with_post_read_hook_for_test(
        &replaced,
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
        move || {
            std::fs::rename(&replacement_path, &displaced_path)?;
            std::fs::write(&replacement_path, &replacement_bytes)
        },
    )
    .is_err());
}

#[test]
#[cfg(not(unix))]
fn validator_reader_is_explicitly_unsupported_off_unix() {
    let scratch = ScratchDir::new("validator-non-unix");
    let path = scratch.join("trace.log");
    std::fs::write(
        &path,
        trace_bytes(&complete_events(DiagnosticMode::Plain, &[0], SHA_A)),
    )
    .unwrap();
    assert!(validate_diagnostic_trace_file(
        &path,
        DiagnosticMode::Plain,
        &[0],
        DiagnosticTermination::Normal,
    )
    .is_err());
}

#[test]
#[cfg(unix)]
fn trace_sink_uses_exclusive_regular_file_creation_and_real_parent_checks() {
    use std::os::unix::fs::symlink;

    let scratch = ScratchDir::new("sink-create");
    let trace = scratch.join("trace.log");
    let sink = DiagnosticTraceSink::create(&trace).unwrap();
    drop(sink);
    assert!(DiagnosticTraceSink::create(&trace).is_err());
    assert!(DiagnosticTraceSink::create(&scratch.0).is_err());
    assert!(DiagnosticTraceSink::create(&scratch.join("missing/trace.log")).is_err());

    let target = scratch.join("target.log");
    std::fs::write(&target, b"").unwrap();
    let link = scratch.join("trace-link.log");
    symlink(&target, &link).unwrap();
    assert!(DiagnosticTraceSink::create(&link).is_err());

    let real_parent = scratch.join("real-parent");
    std::fs::create_dir(&real_parent).unwrap();
    let parent_link = scratch.join("parent-link");
    symlink(&real_parent, &parent_link).unwrap();
    assert!(DiagnosticTraceSink::create(&parent_link.join("trace.log")).is_err());
}

#[test]
#[cfg(target_os = "linux")]
fn trace_sink_rejects_fifo_without_blocking() {
    use rustix::fs::{mkfifoat, open, Mode, OFlags};

    let scratch = ScratchDir::new("sink-fifo");
    let directory = open(
        &scratch.0,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    mkfifoat(&directory, "trace.fifo", Mode::RUSR | Mode::WUSR).unwrap();
    assert!(DiagnosticTraceSink::create(&scratch.join("trace.fifo")).is_err());
}

#[test]
fn trace_sink_surfaces_real_write_failure_and_enforces_bound_during_write() {
    let scratch = ScratchDir::new("sink-write");
    let readonly_path = scratch.join("readonly.log");
    std::fs::write(&readonly_path, b"").unwrap();
    let readonly = std::fs::File::open(&readonly_path).unwrap();
    let mut sink = DiagnosticTraceSink::from_file_for_test(readonly).unwrap();
    assert!(sink.write_bytes_for_test(b"record\n").is_err());

    let bounded_path = scratch.join("bounded.log");
    let bounded_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&bounded_path)
        .unwrap();
    let mut sink = DiagnosticTraceSink::from_file_for_test(bounded_file).unwrap();
    let exact = vec![b'x'; MAX_DIAGNOSTIC_TRACE_BYTES];
    sink.write_bytes_for_test(&exact).unwrap();
    assert!(sink.write_bytes_for_test(b"x").is_err());
    drop(sink);
    assert_eq!(
        std::fs::metadata(&bounded_path).unwrap().len(),
        u64::try_from(MAX_DIAGNOSTIC_TRACE_BYTES).unwrap()
    );
}

#[test]
fn tiny_real_profile_crosses_the_actual_canonical_boundaries_in_all_modes() {
    const SOURCES: &[InjectedLibrarySource<'static>] = &[
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "tiny-a.d.ts",
            source: "interface Wu0eBox<T> { value: T; }\ntype Wu0eConditional<T> = T extends string ? T : never;\n",
        },
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(1),
            name: "tiny-b.d.ts",
            source: "type Wu0eMapped<T> = { [K in keyof T]: T[K] };\ntype Wu0eNode = { next?: Wu0eNode };\ndeclare const wu0eBox: Wu0eBox<string>;\n",
        },
    ];

    let scratch = ScratchDir::new("integration");
    let mut traces = Vec::new();
    for mode in [
        DiagnosticMode::Plain,
        DiagnosticMode::MeasuredOff,
        DiagnosticMode::CandidateB,
    ] {
        let path = scratch.join(&format!("{}.trace", mode.as_str()));
        let trace = run_observed_profile_for_test(mode, SOURCES, &path)
            .expect("tiny profile executes the real WU0B pipeline");
        assert_eq!(trace.completed_phases(), phase_plan(&[0, 1]));
        assert_eq!(trace.measurement().is_some(), mode != DiagnosticMode::Plain);
        assert!(trace.semantic_sha256().is_some());
        traces.push(trace);
    }
    let [plain, measured, candidate] = traces.as_slice() else {
        panic!("exactly three diagnostic modes")
    };
    validate_completed_semantic_parity([plain, measured, candidate]).unwrap();
    reconcile_measurements(
        measured.measurement().expect("measured-off counters"),
        candidate.measurement().expect("candidate counters"),
    )
    .unwrap();
}

#[test]
fn cycle_bearing_generic_witness_proves_mode_fidelity_and_candidate_hits() {
    const SOURCES: &[InjectedLibrarySource<'static>] = &[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "cycle-bearing-generic.d.ts",
        source: "type Wu0eCycle = { self: Wu0eCycle };\ntype Wu0eCarrier<T> = { cycle: Wu0eCycle; value: T };\ndeclare const wu0eFirst: Wu0eCarrier<string>;\ndeclare const wu0eSecond: Wu0eCarrier<string>;\n",
    }];
    let scratch = ScratchDir::new("recursive-mode-fidelity");
    let off = run_observed_profile_for_test(
        DiagnosticMode::MeasuredOff,
        SOURCES,
        &scratch.join("off.trace"),
    )
    .expect("recursive generic measured-off witness");
    let candidate = run_observed_profile_for_test(
        DiagnosticMode::CandidateB,
        SOURCES,
        &scratch.join("candidate.trace"),
    )
    .expect("recursive generic Candidate-B witness");
    let off_measure = off.measurement().expect("measured-off counters");
    let candidate_measure = candidate.measurement().expect("candidate counters");
    assert_eq!(
        (
            off_measure.tainted_cache_hits,
            off_measure.tainted_cache_entries,
            off_measure.avoided_visits,
        ),
        (0, 0, 0)
    );
    assert!(candidate_measure.tainted_cache_hits > 0);
    assert!(candidate_measure.tainted_cache_entries > 0);
    assert!(candidate_measure.avoided_visits > 0);
    assert_eq!(off.semantic_sha256(), candidate.semantic_sha256());
    reconcile_measurements(off_measure, candidate_measure).unwrap();
}

#[test]
fn every_phase_transition_encloses_its_real_live_pipeline_location() {
    let wu0b = include_str!("wu0b_library.rs");
    let observed = function_window(
        wu0b,
        "fn run_injected_profile_observed(",
        "fn validate_parser_export_claims(",
    );
    let observed_signature = &observed[..observed.find('{').expect("observed signature")];
    assert!(observed_signature.contains("boundary: &mut DiagnosticBoundaryAdapter"));
    let phases = [
        (
            DiagnosticPhase::Parse,
            "canonical_inputs(sources)",
            "parser_export_claims",
        ),
        (
            DiagnosticPhase::Bind,
            "ProjectBinderBuilder::new",
            "semantic_scopes",
        ),
        (
            DiagnosticPhase::LexicalReservation,
            "LibraryEventLedger::default",
            "reserve_library_program",
        ),
        (
            DiagnosticPhase::TypeReservation,
            "Interner::with_intrinsics",
            "reserve_callable_type_params",
        ),
        (
            DiagnosticPhase::PassConstruction,
            "library_semantic_tickets",
            "build_pass_with_tickets",
        ),
        (
            DiagnosticPhase::FillParameterMetadata,
            "fill_type_group_parameter_metadata_range",
            "fill_type_group_parameter_metadata_range",
        ),
        (
            DiagnosticPhase::FillInterfaceScc,
            "construct_pending_interface_sccs_range",
            "construct_pending_interface_sccs_range",
        ),
        (
            DiagnosticPhase::FillConditional,
            "fill_conditional_aliases_range",
            "fill_conditional_aliases_range",
        ),
        (
            DiagnosticPhase::FillMapped,
            "fill_mapped_aliases_range",
            "fill_mapped_aliases_range",
        ),
        (
            DiagnosticPhase::FillObject,
            "fill_object_aliases_range",
            "fill_object_aliases_range",
        ),
        (
            DiagnosticPhase::FillRemaining,
            "fill_remaining_aliases_range",
            "fill_remaining_aliases_range",
        ),
        (
            DiagnosticPhase::PrepareAttachedNamespaceValues,
            "prepare_project_attached_namespace_values",
            "prepare_project_attached_namespace_values",
        ),
        (
            DiagnosticPhase::PrepareStandaloneNamespaceValues,
            "prepare_project_standalone_namespace_values",
            "prepare_project_standalone_namespace_values",
        ),
        (
            DiagnosticPhase::PublishClassSurfaces,
            "publish_class_surfaces",
            "publish_class_surfaces",
        ),
        (
            DiagnosticPhase::FinalizeStandaloneNamespaceValues,
            "finalize_standalone_namespace_values",
            "finalize_standalone_namespace_values",
        ),
        (
            DiagnosticPhase::PrecomputeStandaloneNamespaceAliases,
            "precompute_standalone_namespace_value_aliases",
            "precompute_standalone_namespace_value_aliases",
        ),
        (
            DiagnosticPhase::FillPendingInterfaces,
            "fill_pending_interfaces_range",
            "fill_pending_interfaces_range",
        ),
        (
            DiagnosticPhase::PublishTypeGroups,
            "publish_type_groups",
            "publish_type_groups",
        ),
        (
            DiagnosticPhase::ValidatePublishedClassSurfaces,
            "validate_published_class_surfaces",
            "validate_published_class_surfaces",
        ),
        (
            DiagnosticPhase::CaptureLexicalEvidence,
            "library_lexical_evidence",
            "library_lexical_evidence",
        ),
        (
            DiagnosticPhase::StatementFile,
            "pass.build_flow_graph",
            "pass.check_statements",
        ),
        (
            DiagnosticPhase::SemanticFinalization,
            "finish_semantic_effects",
            "finish_semantic_effects",
        ),
        (
            DiagnosticPhase::CollectTypeProbes,
            "collect_type_probes",
            "collect_type_probes",
        ),
        (
            DiagnosticPhase::CollectGlobalValueProbes,
            "collect_value_probes",
            "collect_value_probes",
        ),
        (
            DiagnosticPhase::CollectModuleValueProbes,
            "collect_module_value_probes",
            "collect_module_value_probes",
        ),
        (
            DiagnosticPhase::CollectNamespaceValueProbes,
            "collect_namespace_value_probes",
            "collect_namespace_value_probes",
        ),
        (
            DiagnosticPhase::FreezeLibraryProduct,
            "canonical_frozen_library_product",
            "canonical_frozen_library_product",
        ),
        (
            DiagnosticPhase::CompleteSemanticBatches,
            "complete_semantic_batches",
            "complete_semantic_batches",
        ),
        (
            DiagnosticPhase::ConsumeBinderOutcomes,
            "consume_binder_outcomes",
            "consume_binder_outcomes",
        ),
        (
            DiagnosticPhase::FinishLedger,
            "ledger.snapshot",
            "ledger.finish",
        ),
        (
            DiagnosticPhase::BuildSemanticComponents,
            "canonical_wu0d_semantic_components",
            "canonical_wu0d_semantic_components",
        ),
        (
            DiagnosticPhase::SemanticDigest,
            "canonical_wu0d_semantic_identity",
            "canonical_wu0d_semantic_identity",
        ),
    ];
    let mut previous = None;
    for (phase, first, last) in phases {
        let position = assert_phase_encloses(observed, phase, first, last);
        if let Some(previous) = previous {
            assert!(previous < position, "{phase:?} transition order drifted");
        }
        previous = Some(position);
    }
}

#[test]
fn boundary_fixture_and_live_pipeline_use_the_same_adapter_api() {
    let wu0e = include_str!("wu0e_diagnostic.rs");
    let fixture = function_window(
        wu0e,
        "pub(super) fn run_boundary_fixture_for_test(",
        "pub(super) fn run_observed_profile_for_test(",
    );
    assert_eq!(
        fixture.matches("DiagnosticBoundaryAdapter::new(").count(),
        1
    );
    assert_eq!(fixture.matches("boundary.enter_phase(").count(), 1);
    assert_eq!(fixture.matches("boundary.exit_phase(").count(), 1);
    for forbidden in [
        "observer.enter",
        "observer.exit",
        "sink.write",
        "sink.flush",
        "render_diagnostic_event_for_test",
    ] {
        assert!(
            !fixture.contains(forbidden),
            "boundary fixture bypassed the shared adapter through {forbidden}"
        );
    }

    let wu0b = include_str!("wu0b_library.rs");
    let observed = function_window(
        wu0b,
        "fn run_injected_profile_observed(",
        "fn validate_parser_export_claims(",
    );
    assert!(observed.contains("boundary: &mut DiagnosticBoundaryAdapter"));
    assert!(!observed.contains("observer.enter"));
    assert!(!observed.contains("observer.exit"));
    assert!(!observed.contains("sink.write"));
    assert!(!observed.contains("sink.flush"));
}

#[test]
fn boundary_callback_write_flush_precedes_semantic_continuation() {
    let scratch = ScratchDir::new("boundary-order");
    let path = scratch.join("order.trace");
    let (sink, control) = BoundaryTestSink::recording_file(&path).unwrap();
    let clock = BoundaryTestClock::new([0, 10]);
    let continued = Arc::new(AtomicBool::new(false));
    let continuation_flag = Arc::clone(&continued);
    run_boundary_fixture_for_test(
        DiagnosticMode::Plain,
        DiagnosticPhaseKey::singleton(DiagnosticPhase::ProfileLoad),
        clock,
        sink,
        move || continuation_flag.store(true, Ordering::SeqCst),
    )
    .unwrap();
    assert!(continued.load(Ordering::SeqCst));
    assert_eq!(
        control.observations(),
        [
            BoundaryFixtureObservation::Callback,
            BoundaryFixtureObservation::Write,
            BoundaryFixtureObservation::Flush,
            BoundaryFixtureObservation::SemanticContinuation,
        ]
    );
}

#[test]
fn blocked_then_aborted_enter_flush_leaves_a_durable_active_phase() {
    use std::time::Duration;

    let scratch = ScratchDir::new("boundary-blocked");
    let path = scratch.join("blocked.trace");
    let (sink, control) = BoundaryTestSink::blocking_file(&path).unwrap();
    let clock = BoundaryTestClock::new([0, 10]);
    let continued = Arc::new(AtomicBool::new(false));
    let continuation_flag = Arc::clone(&continued);
    let worker = std::thread::spawn(move || {
        run_boundary_fixture_for_test(
            DiagnosticMode::Plain,
            DiagnosticPhaseKey::singleton(DiagnosticPhase::ProfileLoad),
            clock,
            sink,
            move || continuation_flag.store(true, Ordering::SeqCst),
        )
    });
    control
        .wait_until_flush_blocked(Duration::from_secs(1))
        .expect("enter record reaches the blocking flush");
    let visible = std::fs::read(&path).expect("read boundary record while flush is blocked");
    let parsed = parse_diagnostic_trace(
        &visible,
        DiagnosticMode::Plain,
        &[],
        DiagnosticTermination::Deadline,
    )
    .expect("run-start plus durable phase-enter is a contained prefix");
    assert_eq!(
        parsed.active_phase(),
        Some(DiagnosticPhaseKey::singleton(DiagnosticPhase::ProfileLoad))
    );
    assert!(!parsed.finished());
    control.abort_flush();
    assert!(worker.join().unwrap().is_err());
    assert!(!continued.load(Ordering::SeqCst));
}

#[test]
#[ignore = "WU0E external process-containment runner self-test"]
fn runner_self_test_behaviorally_exercises_every_containment_invariant() {
    let output = std::process::Command::new("perl")
        .arg("tooling/wu0e-diagnostic/run.pl")
        .arg("--self-test")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("launch WU0E runner self-test");
    assert!(
        output.status.success(),
        "runner self-test failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = parse_runner_self_test_report(&output.stdout).expect("canonical self-test report");
    assert_eq!(report.passed_cases(), RunnerSelfTestCase::ALL.as_slice());
    assert_eq!(report.deadline_us(), 180_000_000);
    assert_eq!(report.max_process_group_rss_bytes(), 1_024 * 1_024 * 1_024);
    assert_eq!(report.max_stdout_bytes(), 128 * 1_024);
    assert_eq!(report.max_stderr_bytes(), 128 * 1_024);
    assert_eq!(report.max_trace_bytes(), 256 * 1_024);
    assert!(report.max_observed_rss_sample_interval_us() <= 10_000);
    for case in RunnerSelfTestCase::ALL {
        let observation = report.observation(*case).expect("one observation per case");
        assert_eq!(observation.case(), *case);
        assert!(!observation.fields().is_empty());
    }
    assert!(report
        .observation_bool(RunnerSelfTestCase::SetsidContainment, "group_isolated")
        .unwrap());
    assert!(report
        .observation_bool(RunnerSelfTestCase::ZombieLeaderReservation, "pgid_reserved")
        .unwrap());
    assert!(
        report
            .observation_u64(
                RunnerSelfTestCase::LeaderExitDescendantKill,
                "descendants_reaped"
            )
            .unwrap()
            > 0
    );
    let rss = report
        .observation_u64(RunnerSelfTestCase::SummedLiveGroupRss, "summed_rss_bytes")
        .unwrap();
    let largest = report
        .observation_u64(
            RunnerSelfTestCase::SummedLiveGroupRss,
            "largest_member_rss_bytes",
        )
        .unwrap();
    assert!(rss > largest);
    for (case, limit) in [
        (RunnerSelfTestCase::StdoutFlood, 128 * 1_024),
        (RunnerSelfTestCase::StderrFlood, 128 * 1_024),
        (RunnerSelfTestCase::TraceFlood, 256 * 1_024),
    ] {
        assert!(report.observation_u64(case, "observed_bytes").unwrap() > limit);
        assert!(report
            .observation_bool(case, "whole_group_terminated")
            .unwrap());
    }
    assert_eq!(
        report
            .observation_u64(RunnerSelfTestCase::BoundedPostRead, "max_read_bytes")
            .unwrap(),
        256 * 1_024 + 1
    );
    assert_eq!(
        report
            .observation_text(RunnerSelfTestCase::RssSamplingFailure, "termination")
            .unwrap(),
        "infrastructure"
    );
    assert!(!report
        .observation_bool(RunnerSelfTestCase::RssSamplingFailure, "rss_assumed_zero")
        .unwrap());
    assert!(report
        .observation_bool(
            RunnerSelfTestCase::RssArithmeticOverflow,
            "overflow_detected"
        )
        .unwrap());
    assert_eq!(
        report
            .observation_text(RunnerSelfTestCase::RssArithmeticOverflow, "termination")
            .unwrap(),
        "infrastructure"
    );
    assert_eq!(
        report
            .observation_u64(
                RunnerSelfTestCase::ValidatorAfterEachWorkload,
                "workload_count"
            )
            .unwrap(),
        3
    );
    assert_eq!(
        report
            .observation_u64(
                RunnerSelfTestCase::ValidatorAfterEachWorkload,
                "validator_count"
            )
            .unwrap(),
        3
    );
    assert!(report
        .observation_bool(
            RunnerSelfTestCase::ValidatorAfterEachWorkload,
            "validator_immediately_after_workload"
        )
        .unwrap());
    assert_eq!(
        report
            .observation_u64(
                RunnerSelfTestCase::SameBinaryValidator,
                "binary_identity_count"
            )
            .unwrap(),
        1
    );
    assert!(report
        .observation_bool(RunnerSelfTestCase::BinarySwap, "replacement_rejected")
        .unwrap());
    assert!(
        report
            .observation_u64(
                RunnerSelfTestCase::EnvironmentScrub,
                "removed_variable_count"
            )
            .unwrap()
            > 0
    );
    assert_eq!(
        report
            .observation_text(RunnerSelfTestCase::ExactPrimaryProbe, "compiler_command")
            .unwrap(),
        "frozen-libtest"
    );
    assert!(report
        .observation_bool(
            RunnerSelfTestCase::NoAlternateCompiler,
            "alternate_exec_observed"
        )
        .is_some_and(|observed| !observed));
    assert_eq!(
        report
            .observation_u64(
                RunnerSelfTestCase::SameBinaryHostProfileInventory,
                "identity_tuple_count"
            )
            .unwrap(),
        1
    );
    let text = std::str::from_utf8(&output.stdout).expect("self-test report is ASCII");
    let lines = text.lines().collect::<Vec<_>>();
    let observation_prefix = "typokat-wu0e-self-test-observation-v1 ";
    let observation_lines = lines
        .iter()
        .filter(|line| line.starts_with(observation_prefix))
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(observation_lines.len(), RunnerSelfTestCase::ALL.len());
    let removed = lines
        .iter()
        .copied()
        .filter(|line| *line != observation_lines[0])
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert!(parse_runner_self_test_report(removed.as_bytes()).is_err());
    let duplicated = format!("{text}{}\n", observation_lines[0]);
    assert!(parse_runner_self_test_report(duplicated.as_bytes()).is_err());
}

#[test]
fn canonical_compiler_and_fixed_observer_have_no_alternate_semantic_path() {
    let wu0b = include_str!("wu0b_library.rs");
    let wu0e = include_str!("wu0e_diagnostic.rs");
    let observer = include_str!("wu0e_observer.rs");

    assert_eq!(
        wu0b.matches("pub(crate) fn run_injected_profile(").count(),
        1
    );
    assert_eq!(wu0b.matches("fn run_injected_profile_observed(").count(), 1);
    let ordinary_wrapper = function_window(
        wu0b,
        "pub(crate) fn run_injected_profile(",
        "fn run_injected_profile_observed(",
    );
    assert_eq!(
        ordinary_wrapper
            .matches("run_injected_profile_observed(")
            .count(),
        1,
        "the ordinary entry delegates exactly once to the canonical observed body"
    );
    for forbidden in [
        "Parser::new",
        "ProjectBinderBuilder::new",
        "reserve_type_decls",
        "build_pass_with_tickets",
        "fill_type_decls",
        "prepare_project_",
        "publish_class_surfaces",
        "build_flow_graph",
        "check_statements",
        "finish_semantic_effects",
        "collect_type_probes",
        "canonical_frozen_library_product",
        "complete_semantic_batches",
        "consume_binder_outcomes",
    ] {
        assert!(
            !ordinary_wrapper.contains(forbidden),
            "ordinary wrapper duplicated compiler primitive {forbidden}"
        );
    }
    for forbidden in [
        "Parser::new",
        "ProjectBinderBuilder::new",
        "build_pass_with_tickets",
        "finish_semantic_effects",
        "collect_type_probes",
        "canonical_frozen_library_product",
    ] {
        assert!(
            !wu0e.contains(forbidden),
            "alternate WU0E compiler path: {forbidden}"
        );
    }
    for forbidden in [
        "String", "Vec<", "Box<", "format!", "write!", "File", "Mutex", "RwLock", "std::io",
        "std::fs",
    ] {
        assert!(
            !observer.contains(forbidden),
            "observer callback boundary contains {forbidden}"
        );
    }
}

#[test]
fn exact_ignored_workload_and_validator_probes_are_closed_and_self_identifying() {
    let wu0e = include_str!("wu0e_diagnostic.rs");
    let primary_name = "fn wu0e_primary_probe_once()";
    let validator_name = "fn wu0e_validate_trace_once()";
    let primary_start = wu0e.find(primary_name).expect("exact primary probe");
    let validator_start = wu0e.find(validator_name).expect("exact validator probe");
    assert!(primary_start < validator_start);
    let primary_attributes = &wu0e[primary_start.saturating_sub(160)..primary_start];
    let validator_attributes = &wu0e[validator_start.saturating_sub(160)..validator_start];
    assert!(primary_attributes.contains("#[test]") && primary_attributes.contains("#[ignore"));
    assert!(validator_attributes.contains("#[test]") && validator_attributes.contains("#[ignore"));

    let primary = &wu0e[primary_start..validator_start];
    assert_eq!(primary.matches("load_strict_profile(").count(), 1);
    assert_eq!(primary.matches("run_injected_profile_observed(").count(), 1);
    assert_phase_encloses(
        primary,
        DiagnosticPhase::ProfileLoad,
        "load_strict_profile(",
        "load_strict_profile(",
    );
    assert_eq!(
        primary
            .matches("start_eager_application_cache_measure(")
            .count(),
        1
    );
    assert_eq!(
        primary
            .matches("start_cycle_tainted_application_cache_baseline_measure(")
            .count(),
        1
    );
    assert_eq!(
        primary
            .matches("start_cycle_tainted_application_cache_measure(")
            .count(),
        1
    );
    let profile_exit_positions = adapter_call_positions(
        primary,
        "boundary.exit_phase(",
        DiagnosticPhase::ProfileLoad,
    );
    let [profile_exit] = profile_exit_positions.as_slice() else {
        panic!("exact shared-adapter profile-load exit")
    };
    let eager_scope = primary
        .find("start_eager_application_cache_measure(")
        .unwrap();
    let observed_call = primary.find("run_injected_profile_observed(").unwrap();
    assert!(*profile_exit < eager_scope && eager_scope < observed_call);

    let validator = &wu0e[validator_start..];
    assert_eq!(validator.matches("load_strict_profile(").count(), 1);
    assert_eq!(
        validator.matches("validate_diagnostic_trace_file(").count(),
        1
    );
    assert!(validator.contains("injected_sources"));
    assert!(validator.contains("file_ordinal"));
    assert!(!validator.contains("TYPOKAT_WU0E_VALIDATE_STATEMENT_ORDINALS"));
    assert!(!validator.contains("run_injected_profile_observed("));
}

#[test]
fn production_compiled_seams_are_test_only_and_ordinary_signatures_are_unchanged() {
    let decls = include_str!("decls/mod.rs");
    let seam = decls
        .find("fn fill_type_decls_range_observed_for_wu0e(")
        .expect("one test-only granular fill seam");
    assert!(decls[seam.saturating_sub(80)..seam].contains("#[cfg(test)]"));
    let seam_end = decls[seam..]
        .find("pub(in crate::check::checker) fn fill_pending_interfaces_range(")
        .map(|offset| seam + offset)
        .expect("bounded test-only fill seam");
    assert_eq!(
        decls.matches("wu0e").count(),
        decls[seam..seam_end].matches("wu0e").count(),
        "all WU0E references in the production-compiled decl module stay in one cfg(test) seam"
    );
    let ordinary = function_window(
        decls,
        "pub(in crate::check::checker) fn fill_type_decls_range(",
        "pub(in crate::check::checker) fn fill_pending_interfaces_range(",
    );
    let signature_end = ordinary.find('{').expect("ordinary fill signature");
    let signature = &ordinary[..signature_end];
    assert!(signature.contains("&mut self"));
    assert!(signature.contains("scope: ScopeId"));
    assert!(signature.contains("start: usize"));
    assert!(signature.contains("end: usize"));
    assert!(!signature.contains("observer") && !signature.contains("wu0e"));

    let context = include_str!("context.rs");
    let pass = function_window(
        context,
        "pub(in crate::check::checker) struct Pass<",
        "impl<'ast, Ticket: Copy + PartialEq> Deref for Pass<",
    );
    assert!(!pass.contains("DiagnosticObserver"));
    assert!(!pass.contains("wu0e"));
    assert!(!pass.contains("phase_observer"));

    assert!(!context.contains("wu0e"));
}

#[test]
fn observer_never_reaches_recursive_semantic_hot_paths() {
    for (name, source) in [
        (
            "substitution",
            include_str!("../../types/substitute/mod.rs"),
        ),
        (
            "substitution application",
            include_str!("../../types/substitute/apply.rs"),
        ),
        ("relation", include_str!("../../relate/relation/mod.rs")),
        (
            "advanced relation",
            include_str!("../../relate/relation/advanced.rs"),
        ),
        (
            "object relation",
            include_str!("../../relate/relation/objects.rs"),
        ),
        (
            "set relation",
            include_str!("../../relate/relation/set_types.rs"),
        ),
        ("evaluator", include_str!("eval/mod.rs")),
        ("evaluator demand", include_str!("eval/demand.rs")),
        ("evaluator extends", include_str!("eval/extends.rs")),
        ("evaluator mapped", include_str!("eval/mapped.rs")),
        ("class publication", include_str!("classes/publication.rs")),
        ("declaration resolution", include_str!("decls/resolve.rs")),
        ("inference", include_str!("../infer/mod.rs")),
        (
            "class semantic publication",
            include_str!("../../class_semantics.rs"),
        ),
    ] {
        assert!(
            !source.contains("wu0e") && !source.contains("DiagnosticObserver"),
            "WU0E observer leaked into {name} recursion"
        );
    }
}

#[test]
fn cfg_test_wiring_is_exact_and_no_driver_lib_or_public_api_is_added() {
    let checker = include_str!("mod.rs");
    let expected = "#[cfg(test)]\nmod wu0e_observer;\n#[cfg(test)]\nmod wu0e_diagnostic;\n#[cfg(test)]\nmod wu0e_diagnostic_spec;";
    assert_eq!(checker.matches(expected).count(), 1);
    assert!(!include_str!("../../driver.rs").contains("wu0e"));
    assert!(!include_str!("../../lib.rs").contains("wu0e"));
    assert!(!include_str!("../../../src/main.rs").contains("wu0e"));
}

#[test]
fn wu0d_and_wu0e_are_reciprocally_isolated_and_wu0d_gate_is_unchanged() {
    let wu0d = include_str!("wu0d_candidate_release.rs");
    let wu0d_runner = include_str!("../../../tooling/wu0d-release/run.pl");
    let wu0e = include_str!("wu0e_diagnostic.rs");
    let wu0e_runner = include_str!("../../../tooling/wu0e-diagnostic/run.pl");

    assert!(!wu0d.contains("wu0e"));
    assert!(!wu0d_runner.contains("wu0e"));
    assert!(!wu0e.contains("wu0d_candidate_release"));
    assert!(!wu0e_runner.contains("tooling/wu0d-release"));
    assert!(wu0d.contains("const MAX_ELAPSED_US: u64 = 5_000_000;"));
    assert!(wu0d_runner.contains("my $TIMEOUT_SECONDS = 5;"));
    for forbidden in [
        "GateDecision",
        "evaluate_candidate_b_release",
        "validate_candidate_b_release_evidence_file",
        "TYPOKAT_WU0D_RELEASE_EVIDENCE_PATH",
        "typokat-wu0d-release-evidence-v1",
        "authorizes_candidate_b",
    ] {
        assert!(!wu0e.contains(forbidden));
        assert!(!wu0e_runner.contains(forbidden));
    }
}

#[test]
fn runner_self_test_case_inventory_is_complete_and_stable() {
    assert_eq!(
        RunnerSelfTestCase::ALL,
        [
            RunnerSelfTestCase::SetsidContainment,
            RunnerSelfTestCase::PreSetsidDirectKill,
            RunnerSelfTestCase::ZombieLeaderReservation,
            RunnerSelfTestCase::LeaderExitDescendantKill,
            RunnerSelfTestCase::SummedLiveGroupRss,
            RunnerSelfTestCase::RssSamplingInterval,
            RunnerSelfTestCase::StdoutFlood,
            RunnerSelfTestCase::StderrFlood,
            RunnerSelfTestCase::TraceFlood,
            RunnerSelfTestCase::BoundedDrain,
            RunnerSelfTestCase::BoundedPostRead,
            RunnerSelfTestCase::RssSamplingFailure,
            RunnerSelfTestCase::RssArithmeticOverflow,
            RunnerSelfTestCase::BinarySwap,
            RunnerSelfTestCase::EnvironmentScrub,
            RunnerSelfTestCase::WorkloadAllowlist,
            RunnerSelfTestCase::ValidatorAllowlist,
            RunnerSelfTestCase::ValidatorAfterEachWorkload,
            RunnerSelfTestCase::ExactPrimaryProbe,
            RunnerSelfTestCase::NoAlternateCompiler,
            RunnerSelfTestCase::SameBinaryValidator,
            RunnerSelfTestCase::PrePostBinaryDigest,
            RunnerSelfTestCase::OneFrozenBinary,
            RunnerSelfTestCase::WarmInventoryBeforeEveryLaunch,
            RunnerSelfTestCase::SameBinaryHostProfileInventory,
            RunnerSelfTestCase::CrossModeIdentityParity,
        ]
    );
}

use super::context::{
    cycle_tainted_application_cache_measure, eager_application_cache_measure,
    start_cycle_tainted_application_cache_baseline_measure,
    start_cycle_tainted_application_cache_measure, start_eager_application_cache_measure,
    CycleTaintedApplicationCacheMeasure, EagerApplicationCacheMeasure,
};
use super::wu0b_library::{
    canonical_wu0d_semantic_identity, run_injected_profile, InjectedLibrarySource,
    Wu0dSemanticIdentity,
};
use crate::source::LibraryFileOrdinal;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt;
use std::io::Read;
use std::path::Path;

const PREFIX: &str = "typokat-wu0d-candidate-v1";
const ENABLE_KEY: &str = "TYPOKAT_WU0D_CANDIDATE";
const ENABLE_VALUE: &str = "candidate-b-v1";
const PRIMARY_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_primary_probe_once";
const NON_CYCLE_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_non_cycle_probe_once";
const REPORTER_CONTROL_PROBE: &str =
    "check::checker::wu0d_candidate_release::wu0d_candidate_reporter_control_probe_once";
const PRIMARY_PROFILE: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const NON_CYCLE_PROFILE: &str = "1c664166f4c307f032958836642008c90c28cb21ff33215144c9188ac8afdd19";
const REPORTER_CONTROL_PROFILE: &str =
    "9f5e4ab6a334154e67fe1ead6e7e1038d9433f5fd66f7ed897a73cfd1d058d0b";
const MAX_ELAPSED_US: u64 = 5_000_000;
const MAX_RSS_BYTES: u64 = 512 * 1_024 * 1_024;
pub(super) const MAX_RELEASE_EVIDENCE_BYTES: usize = 4 * 1_024 * 1_024;
pub(super) const MAX_RELEASE_STDOUT_BYTES: usize = 128 * 1_024;

pub(super) const NON_CYCLE_WORKLOAD_SOURCES: &[InjectedLibrarySource<'static>] =
    &[InjectedLibrarySource {
    file_ordinal: LibraryFileOrdinal::new(0),
    name: "non-cycle.d.ts",
    source: "interface Wu0dBox<T> { value: T; }\ndeclare const wu0dA: Wu0dBox<string>;\ndeclare const wu0dB: Wu0dBox<string>;\n",
    }];

pub(super) const REPORTER_CONTROL_WORKLOAD_SOURCES: &[InjectedLibrarySource<'static>] =
    &[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "reporter-control.ts",
        source:
            "declare namespace Wu0dReporterControl { export default function report(): void; }\n",
    }];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum CandidateEnvironment {
    Off,
    CandidateB,
}

impl CandidateEnvironment {
    fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::CandidateB => "candidate-b",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "candidate-b" => Some(Self::CandidateB),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum CandidateWorkload {
    Primary,
    NonCycle,
    ReporterControl,
}

impl CandidateWorkload {
    fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::NonCycle => "non-cycle",
            Self::ReporterControl => "reporter-control",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "primary" => Some(Self::Primary),
            "non-cycle" => Some(Self::NonCycle),
            "reporter-control" => Some(Self::ReporterControl),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CacheMetrics {
    pub(super) hits: u64,
    pub(super) misses: u64,
    pub(super) entries: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CandidateSummary {
    pub(super) workload: CandidateWorkload,
    pub(super) mode: CandidateEnvironment,
    pub(super) eligible_requests: u64,
    pub(super) executed_runs: u64,
    pub(super) tainted_cache_hits: u64,
    pub(super) clean_outcomes: u64,
    pub(super) tainted_outcomes: u64,
    pub(super) executed_visits: u64,
    pub(super) memo_hits: u64,
    pub(super) expanded_visits: u64,
    pub(super) avoided_visits: u64,
    pub(super) tainted_cache_entries: u64,
    pub(super) clean_cache: CacheMetrics,
    pub(super) semantic: Wu0dSemanticIdentity,
    pub(super) saturated: bool,
}

impl CandidateSummary {
    pub(super) fn avoided_runs(&self) -> u64 {
        self.tainted_cache_hits
    }

    pub(super) fn render(&self) -> String {
        format!(
            "{PREFIX} workload={} mode={} eligible_requests={} executed_runs={} tainted_cache_hits={} clean_outcomes={} tainted_outcomes={} executed_visits={} memo_hits={} expanded_visits={} avoided_visits={} tainted_cache_entries={} clean_cache_hits={} clean_cache_misses={} clean_cache_entries={} diagnostics_len={} diagnostics_sha256={} incomplete_len={} incomplete_sha256={} library_ledger_len={} library_ledger_sha256={} frozen_library_product_len={} frozen_library_product_sha256={} semantic_sha256={} saturated={}",
            self.workload.name(),
            self.mode.name(),
            self.eligible_requests,
            self.executed_runs,
            self.tainted_cache_hits,
            self.clean_outcomes,
            self.tainted_outcomes,
            self.executed_visits,
            self.memo_hits,
            self.expanded_visits,
            self.avoided_visits,
            self.tainted_cache_entries,
            self.clean_cache.hits,
            self.clean_cache.misses,
            self.clean_cache.entries,
            self.semantic.diagnostics.byte_len,
            self.semantic.diagnostics.sha256,
            self.semantic.incomplete.byte_len,
            self.semantic.incomplete.sha256,
            self.semantic.library_ledger.byte_len,
            self.semantic.library_ledger.sha256,
            self.semantic.frozen_library_product.byte_len,
            self.semantic.frozen_library_product.sha256,
            self.semantic.aggregate_sha256,
            u8::from(self.saturated),
        )
    }

    fn arithmetic_is_valid(&self) -> bool {
        self.executed_runs
            .checked_add(self.tainted_cache_hits)
            .is_some_and(|value| value == self.eligible_requests)
            && self
                .clean_outcomes
                .checked_add(self.tainted_outcomes)
                .is_some_and(|value| value == self.executed_runs)
            && self
                .memo_hits
                .checked_add(self.expanded_visits)
                .is_some_and(|value| value == self.executed_visits)
            && self
                .clean_cache
                .hits
                .checked_add(self.clean_cache.misses)
                .is_some()
            && (self.mode != CandidateEnvironment::Off
                || (self.tainted_cache_hits == 0
                    && self.tainted_cache_entries == 0
                    && self.avoided_visits == 0))
            && semantic_identity_is_valid(&self.semantic)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProcessObservation {
    pub(super) process_identity: String,
    pub(super) launch_ordinal: u8,
    pub(super) probe_filter: String,
    pub(super) binary_identity: String,
    pub(super) host_identity: String,
    pub(super) profile_identity: String,
    pub(super) warm_filesystem_cache: bool,
    pub(super) release_libtest: bool,
    pub(super) exit_code: i32,
    pub(super) elapsed_us: u64,
    pub(super) peak_rss_bytes: u64,
    pub(super) captured_stdout: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PairedReleaseRun {
    pub(super) pair: u8,
    pub(super) baseline: ProcessObservation,
    pub(super) candidate: ProcessObservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ControlEvidence {
    pub(super) name: String,
    pub(super) pairs: Vec<PairedReleaseRun>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CandidateReleaseEvidence {
    pub(super) primary: Vec<PairedReleaseRun>,
    pub(super) controls: Vec<ControlEvidence>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum NoGoReason {
    SummaryMissingDuplicateMalformedOrSaturated,
    NotFiveFreshInterleavedPairs,
    BinaryHostOrProfileMismatch,
    NotReleaseLibtestOrWarmFilesystem,
    ProcessFailed,
    DeadlineExceeded,
    MemoryExceeded,
    VariantMismatch,
    SemanticMismatch,
    CounterReconciliationFailed,
    CleanCacheMetricsChanged,
    MedianImprovementBelowTwentyPercent,
    AffectedRunReductionBelowEightyPercent,
    AffectedVisitReductionBelowEightyPercent,
    ControlsMissingDuplicateOrUnknown,
    ControlIdentityOrSemanticMismatch,
    ControlRegressionAboveTwoPercent,
}

impl NoGoReason {
    fn name(self) -> &'static str {
        match self {
            Self::SummaryMissingDuplicateMalformedOrSaturated => {
                "summary-missing-duplicate-malformed-or-saturated"
            }
            Self::NotFiveFreshInterleavedPairs => "not-five-fresh-interleaved-pairs",
            Self::BinaryHostOrProfileMismatch => "binary-host-or-profile-mismatch",
            Self::NotReleaseLibtestOrWarmFilesystem => "not-release-libtest-or-warm-filesystem",
            Self::ProcessFailed => "process-failed",
            Self::DeadlineExceeded => "deadline-exceeded",
            Self::MemoryExceeded => "memory-exceeded",
            Self::VariantMismatch => "variant-mismatch",
            Self::SemanticMismatch => "semantic-mismatch",
            Self::CounterReconciliationFailed => "counter-reconciliation-failed",
            Self::CleanCacheMetricsChanged => "clean-cache-metrics-changed",
            Self::MedianImprovementBelowTwentyPercent => "median-improvement-below-twenty-percent",
            Self::AffectedRunReductionBelowEightyPercent => {
                "affected-run-reduction-below-eighty-percent"
            }
            Self::AffectedVisitReductionBelowEightyPercent => {
                "affected-visit-reduction-below-eighty-percent"
            }
            Self::ControlsMissingDuplicateOrUnknown => "controls-missing-duplicate-or-unknown",
            Self::ControlIdentityOrSemanticMismatch => "control-identity-or-semantic-mismatch",
            Self::ControlRegressionAboveTwoPercent => "control-regression-above-two-percent",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GateDecision {
    Go,
    NoGo(Vec<NoGoReason>),
}

impl GateDecision {
    pub(super) fn authorizes_candidate_b(&self) -> bool {
        matches!(self, Self::Go)
    }

    pub(super) fn reasons(&self) -> &[NoGoReason] {
        match self {
            Self::Go => &[],
            Self::NoGo(reasons) => reasons,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CandidateEnvironmentError(String);

impl fmt::Display for CandidateEnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type CandidateEnvironmentEntry = Result<(Vec<u8>, Vec<u8>), CandidateEnvironmentError>;

fn resolve_candidate_environment() -> Result<CandidateEnvironment, CandidateEnvironmentError> {
    resolve_candidate_environment_os_for_test(std::env::vars_os())
}

pub(super) fn resolve_candidate_environment_os_for_test(
    entries: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<CandidateEnvironment, CandidateEnvironmentError> {
    let entries = entries
        .into_iter()
        .filter_map(wu0d_environment_entry)
        .collect::<Result<Vec<_>, _>>()?;
    resolve_candidate_environment_bytes(&entries)
}

#[cfg(unix)]
fn wu0d_environment_entry((key, value): (OsString, OsString)) -> Option<CandidateEnvironmentEntry> {
    use std::os::unix::ffi::OsStringExt;

    let key = key.into_vec();
    key.starts_with(b"TYPOKAT_WU0D_")
        .then(|| Ok((key, value.into_vec())))
}

#[cfg(windows)]
fn wu0d_environment_entry((key, value): (OsString, OsString)) -> Option<CandidateEnvironmentEntry> {
    use std::os::windows::ffi::OsStrExt;

    let prefix = "TYPOKAT_WU0D_".encode_utf16().collect::<Vec<_>>();
    let key_wide = key.as_os_str().encode_wide().collect::<Vec<_>>();
    if !key_wide.starts_with(&prefix) {
        return None;
    }
    match (key.into_string(), value.into_string()) {
        (Ok(key), Ok(value)) => Some(Ok((key.into_bytes(), value.into_bytes()))),
        _ => Some(Err(CandidateEnvironmentError(
            "WU0D environment is not UTF-8".to_owned(),
        ))),
    }
}

#[cfg(not(any(unix, windows)))]
fn wu0d_environment_entry((key, value): (OsString, OsString)) -> Option<CandidateEnvironmentEntry> {
    let Ok(key) = key.into_string() else {
        return None;
    };
    if !key.starts_with("TYPOKAT_WU0D_") {
        return None;
    }
    match value.into_string() {
        Ok(value) => Some(Ok((key.into_bytes(), value.into_bytes()))),
        Err(_) => Some(Err(CandidateEnvironmentError(
            "WU0D environment value is not UTF-8".to_owned(),
        ))),
    }
}

fn resolve_candidate_environment_bytes(
    entries: &[(Vec<u8>, Vec<u8>)],
) -> Result<CandidateEnvironment, CandidateEnvironmentError> {
    let mut mode = CandidateEnvironment::Off;
    let mut seen = false;
    for (key, value) in entries {
        let key = std::str::from_utf8(key)
            .map_err(|_| CandidateEnvironmentError("WU0D key is not UTF-8".to_owned()))?;
        let value = std::str::from_utf8(value)
            .map_err(|_| CandidateEnvironmentError("WU0D value is not UTF-8".to_owned()))?;
        if key != ENABLE_KEY {
            return Err(CandidateEnvironmentError(format!(
                "unknown WU0D environment key {key:?}"
            )));
        }
        if seen {
            return Err(CandidateEnvironmentError(
                "duplicate WU0D candidate key".to_owned(),
            ));
        }
        if value != ENABLE_VALUE {
            return Err(CandidateEnvironmentError(
                "invalid WU0D candidate value".to_owned(),
            ));
        }
        seen = true;
        mode = CandidateEnvironment::CandidateB;
    }
    Ok(mode)
}

pub(super) fn resolve_candidate_environment_for_test<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<CandidateEnvironment, CandidateEnvironmentError> {
    let entries = entries
        .into_iter()
        .filter(|(key, _)| key.starts_with("TYPOKAT_WU0D_"))
        .map(|(key, value)| (key.as_bytes().to_vec(), value.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    resolve_candidate_environment_bytes(&entries)
}

pub(super) fn resolve_candidate_environment_bytes_for_test(
    entries: &[(&[u8], &[u8])],
) -> Result<CandidateEnvironment, CandidateEnvironmentError> {
    let entries = entries
        .iter()
        .filter(|(key, _)| key.starts_with(b"TYPOKAT_WU0D_"))
        .map(|(key, value)| (key.to_vec(), value.to_vec()))
        .collect::<Vec<_>>();
    resolve_candidate_environment_bytes(&entries)
}

pub(super) fn parse_candidate_stdout(stdout: &[u8]) -> Result<CandidateSummary, String> {
    let stdout = std::str::from_utf8(stdout).map_err(|_| "stdout is not UTF-8".to_owned())?;
    let lines = stdout
        .lines()
        .filter(|line| line.starts_with(PREFIX))
        .collect::<Vec<_>>();
    let [line] = lines.as_slice() else {
        return Err("stdout must contain exactly one WU0D summary".to_owned());
    };
    parse_summary_line(line)
}

pub(super) fn parse_candidate_release_evidence(
    evidence: &[u8],
) -> Result<CandidateReleaseEvidence, String> {
    if evidence.len() > MAX_RELEASE_EVIDENCE_BYTES {
        return Err("release evidence exceeds the byte limit".to_owned());
    }
    if evidence.last() != Some(&b'\n') || evidence.contains(&b'\r') || !evidence.is_ascii() {
        return Err("release evidence must be final-LF ASCII without CR".to_owned());
    }
    let text = std::str::from_utf8(evidence)
        .map_err(|_| "release evidence is not strict UTF-8".to_owned())?;
    let lines = text
        .strip_suffix('\n')
        .ok_or_else(|| "release evidence is not final-LF terminated".to_owned())?
        .split('\n')
        .collect::<Vec<_>>();
    if lines.len() != 31 || lines[0] != "typokat-wu0d-release-evidence-v1 process_count=30" {
        return Err("release evidence has the wrong header or record count".to_owned());
    }

    let primary = parse_evidence_set("primary", &lines[1..11])?;
    let non_cycle = parse_evidence_set("non-cycle", &lines[11..21])?;
    let reporter = parse_evidence_set("reporter-control", &lines[21..31])?;
    Ok(CandidateReleaseEvidence {
        primary,
        controls: vec![
            ControlEvidence {
                name: "non-cycle".to_owned(),
                pairs: non_cycle,
            },
            ControlEvidence {
                name: "reporter-control".to_owned(),
                pairs: reporter,
            },
        ],
    })
}

fn parse_evidence_set(set: &str, lines: &[&str]) -> Result<Vec<PairedReleaseRun>, String> {
    if lines.len() != 10 {
        return Err("evidence set must contain ten process records".to_owned());
    }
    let mut baselines = (0..5).map(|_| None).collect::<Vec<_>>();
    let mut candidates = (0..5).map(|_| None).collect::<Vec<_>>();
    for (position, line) in lines.iter().enumerate() {
        let launch_ordinal =
            u8::try_from(position + 1).map_err(|_| "launch ordinal does not fit u8".to_owned())?;
        let (pair, candidate, process) = parse_evidence_process_line(line, set, launch_ordinal)?;
        let slot = usize::from(pair - 1);
        let target = if candidate {
            &mut candidates[slot]
        } else {
            &mut baselines[slot]
        };
        if target.replace(process).is_some() {
            return Err("duplicate pair variant in evidence set".to_owned());
        }
    }
    baselines
        .into_iter()
        .zip(candidates)
        .enumerate()
        .map(|(index, (baseline, candidate))| {
            Ok(PairedReleaseRun {
                pair: u8::try_from(index + 1)
                    .map_err(|_| "pair ordinal does not fit u8".to_owned())?,
                baseline: baseline.ok_or_else(|| "missing off process".to_owned())?,
                candidate: candidate.ok_or_else(|| "missing candidate process".to_owned())?,
            })
        })
        .collect()
}

fn expected_pair_variant(launch_ordinal: u8) -> Option<(u8, bool)> {
    match launch_ordinal {
        1 => Some((1, false)),
        2 => Some((1, true)),
        3 => Some((2, true)),
        4 => Some((2, false)),
        5 => Some((3, false)),
        6 => Some((3, true)),
        7 => Some((4, true)),
        8 => Some((4, false)),
        9 => Some((5, false)),
        10 => Some((5, true)),
        _ => None,
    }
}

fn parse_evidence_process_line(
    line: &str,
    expected_set: &str,
    expected_launch: u8,
) -> Result<(u8, bool, ProcessObservation), String> {
    let mut fields = line.split(' ');
    if fields.next() != Some("process") {
        return Err("evidence process prefix is invalid".to_owned());
    }
    if field(&mut fields, "set")? != expected_set {
        return Err("evidence workload sets are reordered".to_owned());
    }
    let pair = canonical_u8(field(&mut fields, "pair")?)?;
    let candidate = match field(&mut fields, "variant")? {
        "off" => false,
        "candidate-b" => true,
        _ => return Err("invalid evidence variant".to_owned()),
    };
    if expected_pair_variant(expected_launch) != Some((pair, candidate)) {
        return Err("evidence launch sequence is noncanonical".to_owned());
    }
    let process_identity_len = canonical_usize(field(&mut fields, "process_identity_len")?)?;
    let process_identity = decode_text_hex(
        field(&mut fields, "process_identity_hex")?,
        process_identity_len,
        MAX_RELEASE_STDOUT_BYTES,
    )?;
    let launch_ordinal = canonical_u8(field(&mut fields, "launch_ordinal")?)?;
    if launch_ordinal != expected_launch {
        return Err("evidence launch ordinal is reordered".to_owned());
    }
    let probe_filter_len = canonical_usize(field(&mut fields, "probe_filter_len")?)?;
    let probe_filter = decode_text_hex(
        field(&mut fields, "probe_filter_hex")?,
        probe_filter_len,
        MAX_RELEASE_STDOUT_BYTES,
    )?;
    let binary_identity = digest_field(&mut fields, "binary_identity")?;
    let host_identity = digest_field(&mut fields, "host_identity")?;
    let profile_identity = digest_field(&mut fields, "profile_identity")?;
    let warm_filesystem_cache = bit(field(&mut fields, "warm_filesystem_cache")?)?;
    let release_libtest = bit(field(&mut fields, "release_libtest")?)?;
    let exit_code = canonical_i32(field(&mut fields, "exit_code")?)?;
    let elapsed_us = number(field(&mut fields, "elapsed_us")?)?;
    let peak_rss_bytes = number(field(&mut fields, "peak_rss_bytes")?)?;
    let stdout_len = canonical_usize(field(&mut fields, "stdout_len")?)?;
    let captured_stdout = decode_hex(
        field(&mut fields, "stdout_hex")?,
        stdout_len,
        MAX_RELEASE_STDOUT_BYTES,
    )?;
    if fields.next().is_some() {
        return Err("evidence process has trailing fields".to_owned());
    }
    Ok((
        pair,
        candidate,
        ProcessObservation {
            process_identity,
            launch_ordinal,
            probe_filter,
            binary_identity,
            host_identity,
            profile_identity,
            warm_filesystem_cache,
            release_libtest,
            exit_code,
            elapsed_us,
            peak_rss_bytes,
            captured_stdout,
        },
    ))
}

fn canonical_usize(value: &str) -> Result<usize, String> {
    usize::try_from(number(value)?).map_err(|_| "integer does not fit usize".to_owned())
}

fn canonical_u8(value: &str) -> Result<u8, String> {
    u8::try_from(number(value)?).map_err(|_| "integer does not fit u8".to_owned())
}

fn canonical_i32(value: &str) -> Result<i32, String> {
    if value.is_empty()
        || value == "-0"
        || (value.starts_with('0') && value.len() > 1)
        || (value.starts_with("-0") && value.len() > 2)
        || value.starts_with('+')
    {
        return Err("noncanonical signed integer".to_owned());
    }
    value
        .parse::<i32>()
        .map_err(|_| "invalid signed integer".to_owned())
}

fn bit(value: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err("invalid evidence bit".to_owned()),
    }
}

fn digest_field<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    key: &str,
) -> Result<String, String> {
    let value = field(fields, key)?;
    if !is_lower_hex(value) {
        return Err(format!("invalid {key}"));
    }
    Ok(value.to_owned())
}

fn decode_text_hex(value: &str, length: usize, maximum: usize) -> Result<String, String> {
    String::from_utf8(decode_hex(value, length, maximum)?)
        .map_err(|_| "hex text field is not UTF-8".to_owned())
}

fn decode_hex(value: &str, length: usize, maximum: usize) -> Result<Vec<u8>, String> {
    if length > maximum || value.len() != length.saturating_mul(2) {
        return Err("hex field length is invalid or oversized".to_owned());
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(length);
    for pair in bytes.chunks_exact(2) {
        let high = lower_hex_nibble(pair[0]).ok_or_else(|| "invalid lowercase hex".to_owned())?;
        let low = lower_hex_nibble(pair[1]).ok_or_else(|| "invalid lowercase hex".to_owned())?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn lower_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

pub(super) fn validate_candidate_b_release_evidence_file(
    path: &Path,
) -> Result<GateDecision, String> {
    if !path.is_absolute() {
        return Err("release evidence path must be absolute".to_owned());
    }
    let bytes = read_candidate_release_evidence_file_bounded(path)?;
    let evidence = parse_candidate_release_evidence(&bytes)?;
    Ok(evaluate_candidate_b_release(
        &evidence.primary,
        &evidence.controls,
    ))
}

#[cfg(unix)]
fn read_candidate_release_evidence_file_bounded(path: &Path) -> Result<Vec<u8>, String> {
    use rustix::fs::{fstat, open, FileType, Mode, OFlags};

    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("cannot no-follow open release evidence: {error}"))?;
    let stat =
        fstat(&descriptor).map_err(|error| format!("cannot fstat release evidence: {error}"))?;
    if !FileType::from_raw_mode(stat.st_mode).is_file() {
        return Err("opened release evidence is not a regular file".to_owned());
    }
    let expected_size = u64::try_from(stat.st_size)
        .map_err(|_| "release evidence has an invalid size".to_owned())?;
    let maximum = u64::try_from(MAX_RELEASE_EVIDENCE_BYTES)
        .map_err(|_| "release evidence limit does not fit u64".to_owned())?;
    if expected_size > maximum {
        return Err("release evidence exceeds the byte limit".to_owned());
    }

    let mut file: std::fs::File = descriptor.into();
    let mut bytes = Vec::new();
    Read::read_to_end(&mut Read::take(&mut file, maximum + 1), &mut bytes)
        .map_err(|error| format!("cannot read release evidence: {error}"))?;
    let actual_size = u64::try_from(bytes.len())
        .map_err(|_| "release evidence read size does not fit u64".to_owned())?;
    if actual_size > maximum || actual_size != expected_size {
        return Err("release evidence changed size during bounded read".to_owned());
    }
    let after =
        fstat(&file).map_err(|error| format!("cannot re-fstat release evidence: {error}"))?;
    if !FileType::from_raw_mode(after.st_mode).is_file()
        || stat.st_dev != after.st_dev
        || stat.st_ino != after.st_ino
        || stat.st_size != after.st_size
    {
        return Err("release evidence changed during read".to_owned());
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn read_candidate_release_evidence_file_bounded(_path: &Path) -> Result<Vec<u8>, String> {
    Err("WU0D release evidence validation is unsupported on non-Unix hosts".to_owned())
}

pub(super) fn render_candidate_release_validation(decision: &GateDecision) -> String {
    match decision {
        GateDecision::Go => {
            "typokat-wu0d-release-validation-v1 decision=go reasons=none".to_owned()
        }
        GateDecision::NoGo(reasons) => format!(
            "typokat-wu0d-release-validation-v1 decision=no-go reasons={}",
            reasons
                .iter()
                .map(|reason| reason.name())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn parse_summary_line(line: &str) -> Result<CandidateSummary, String> {
    let mut fields = line.split(' ');
    if fields.next() != Some(PREFIX) {
        return Err("invalid summary prefix".to_owned());
    }
    let workload = CandidateWorkload::parse(field(&mut fields, "workload")?)
        .ok_or_else(|| "invalid workload".to_owned())?;
    let mode = CandidateEnvironment::parse(field(&mut fields, "mode")?)
        .ok_or_else(|| "invalid mode".to_owned())?;
    let eligible_requests = number(field(&mut fields, "eligible_requests")?)?;
    let executed_runs = number(field(&mut fields, "executed_runs")?)?;
    let tainted_cache_hits = number(field(&mut fields, "tainted_cache_hits")?)?;
    let clean_outcomes = number(field(&mut fields, "clean_outcomes")?)?;
    let tainted_outcomes = number(field(&mut fields, "tainted_outcomes")?)?;
    let executed_visits = number(field(&mut fields, "executed_visits")?)?;
    let memo_hits = number(field(&mut fields, "memo_hits")?)?;
    let expanded_visits = number(field(&mut fields, "expanded_visits")?)?;
    let avoided_visits = number(field(&mut fields, "avoided_visits")?)?;
    let tainted_cache_entries = number(field(&mut fields, "tainted_cache_entries")?)?;
    let clean_cache_hits = number(field(&mut fields, "clean_cache_hits")?)?;
    let clean_cache_misses = number(field(&mut fields, "clean_cache_misses")?)?;
    let clean_cache_entries = number(field(&mut fields, "clean_cache_entries")?)?;
    let diagnostics = component(&mut fields, "diagnostics_len", "diagnostics_sha256")?;
    let incomplete = component(&mut fields, "incomplete_len", "incomplete_sha256")?;
    let library_ledger = component(&mut fields, "library_ledger_len", "library_ledger_sha256")?;
    let frozen_library_product = component(
        &mut fields,
        "frozen_library_product_len",
        "frozen_library_product_sha256",
    )?;
    let aggregate_sha256 = field(&mut fields, "semantic_sha256")?.to_owned();
    let saturated = match field(&mut fields, "saturated")? {
        "0" => false,
        "1" => true,
        _ => return Err("invalid saturated bit".to_owned()),
    };
    if fields.next().is_some() {
        return Err("trailing summary fields".to_owned());
    }
    let summary = CandidateSummary {
        workload,
        mode,
        eligible_requests,
        executed_runs,
        tainted_cache_hits,
        clean_outcomes,
        tainted_outcomes,
        executed_visits,
        memo_hits,
        expanded_visits,
        avoided_visits,
        tainted_cache_entries,
        clean_cache: CacheMetrics {
            hits: clean_cache_hits,
            misses: clean_cache_misses,
            entries: clean_cache_entries,
        },
        semantic: Wu0dSemanticIdentity {
            diagnostics,
            incomplete,
            library_ledger,
            frozen_library_product,
            aggregate_sha256,
        },
        saturated,
    };
    if !summary.arithmetic_is_valid() || summary.render() != line {
        return Err("noncanonical or inconsistent summary".to_owned());
    }
    Ok(summary)
}

fn field<'a>(fields: &mut impl Iterator<Item = &'a str>, key: &str) -> Result<&'a str, String> {
    let raw = fields
        .next()
        .ok_or_else(|| format!("missing {key} field"))?;
    raw.strip_prefix(key)
        .and_then(|value| value.strip_prefix('='))
        .ok_or_else(|| format!("expected {key} field"))
}

fn number(value: &str) -> Result<u64, String> {
    if value.is_empty() || value.starts_with('+') || (value.len() > 1 && value.starts_with('0')) {
        return Err("noncanonical integer".to_owned());
    }
    value
        .parse::<u64>()
        .map_err(|_| "invalid integer".to_owned())
}

fn component<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    length_key: &str,
    digest_key: &str,
) -> Result<super::wu0b_library::Wu0dSemanticComponentIdentity, String> {
    let byte_len = number(field(fields, length_key)?)?;
    let sha256 = field(fields, digest_key)?.to_owned();
    if !is_lower_hex(&sha256) {
        return Err("invalid component digest".to_owned());
    }
    Ok(super::wu0b_library::Wu0dSemanticComponentIdentity { byte_len, sha256 })
}

fn is_lower_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn semantic_identity_is_valid(identity: &Wu0dSemanticIdentity) -> bool {
    is_lower_hex(&identity.diagnostics.sha256)
        && is_lower_hex(&identity.incomplete.sha256)
        && is_lower_hex(&identity.library_ledger.sha256)
        && is_lower_hex(&identity.frozen_library_product.sha256)
        && is_lower_hex(&identity.aggregate_sha256)
}

fn add_reason(reasons: &mut BTreeSet<NoGoReason>, reason: NoGoReason) {
    reasons.insert(reason);
}

pub(super) fn evaluate_candidate_b_release(
    primary: &[PairedReleaseRun],
    controls: &[ControlEvidence],
) -> GateDecision {
    let mut reasons = BTreeSet::new();
    if !all_process_identities_are_unique(primary, controls) {
        add_reason(&mut reasons, NoGoReason::NotFiveFreshInterleavedPairs);
    }
    let primary_summaries = validate_primary(primary, &mut reasons);

    let control_names = controls
        .iter()
        .map(|control| control.name.as_str())
        .collect::<BTreeSet<_>>();
    if controls.len() != 2
        || control_names.len() != 2
        || !control_names.contains("non-cycle")
        || !control_names.contains("reporter-control")
    {
        add_reason(&mut reasons, NoGoReason::ControlsMissingDuplicateOrUnknown);
    } else {
        let anchor = primary
            .first()
            .map(|pair| (&pair.baseline.binary_identity, &pair.baseline.host_identity));
        for (name, workload, probe, profile) in [
            (
                "non-cycle",
                CandidateWorkload::NonCycle,
                NON_CYCLE_PROBE,
                NON_CYCLE_PROFILE,
            ),
            (
                "reporter-control",
                CandidateWorkload::ReporterControl,
                REPORTER_CONTROL_PROBE,
                REPORTER_CONTROL_PROFILE,
            ),
        ] {
            let control = controls
                .iter()
                .find(|control| control.name == name)
                .expect("validated exact control set");
            validate_control(control, workload, probe, profile, anchor, &mut reasons);
        }
    }

    if let Some(summaries) = primary_summaries {
        validate_primary_gates(primary, &summaries, &mut reasons);
    }

    if reasons.is_empty() {
        GateDecision::Go
    } else {
        GateDecision::NoGo(reasons.into_iter().collect())
    }
}

fn all_process_identities_are_unique(
    primary: &[PairedReleaseRun],
    controls: &[ControlEvidence],
) -> bool {
    let mut identities = BTreeSet::new();
    primary
        .iter()
        .chain(controls.iter().flat_map(|control| &control.pairs))
        .flat_map(|pair| [&pair.baseline, &pair.candidate])
        .all(|process| identities.insert(process.process_identity.as_str()))
}

fn process_observations_are_complete(pairs: &[PairedReleaseRun]) -> bool {
    pairs
        .iter()
        .flat_map(|pair| [&pair.baseline, &pair.candidate])
        .all(|process| {
            !process.process_identity.is_empty()
                && process.elapsed_us > 0
                && process.peak_rss_bytes > 0
        })
}

fn validate_primary(
    pairs: &[PairedReleaseRun],
    reasons: &mut BTreeSet<NoGoReason>,
) -> Option<Vec<(CandidateSummary, CandidateSummary)>> {
    if !pair_shape_is_valid(pairs) {
        add_reason(reasons, NoGoReason::NotFiveFreshInterleavedPairs);
    }
    let mut summaries = Vec::with_capacity(pairs.len());
    let anchor = pairs
        .first()
        .map(|pair| (&pair.baseline.binary_identity, &pair.baseline.host_identity));
    let mut semantic_anchor = None;
    for pair in pairs {
        for process in [&pair.baseline, &pair.candidate] {
            if process.probe_filter != PRIMARY_PROBE
                || process.profile_identity != PRIMARY_PROFILE
                || !is_lower_hex(&process.binary_identity)
                || !is_lower_hex(&process.host_identity)
                || anchor.is_some_and(|(binary, host)| {
                    process.binary_identity != *binary || process.host_identity != *host
                })
            {
                add_reason(reasons, NoGoReason::BinaryHostOrProfileMismatch);
            }
            validate_process(process, reasons);
        }
        let Ok(baseline) = parse_candidate_stdout(&pair.baseline.captured_stdout) else {
            add_reason(
                reasons,
                NoGoReason::SummaryMissingDuplicateMalformedOrSaturated,
            );
            continue;
        };
        let Ok(candidate) = parse_candidate_stdout(&pair.candidate.captured_stdout) else {
            add_reason(
                reasons,
                NoGoReason::SummaryMissingDuplicateMalformedOrSaturated,
            );
            continue;
        };
        if baseline.saturated || candidate.saturated {
            add_reason(
                reasons,
                NoGoReason::SummaryMissingDuplicateMalformedOrSaturated,
            );
        }
        if baseline.workload != CandidateWorkload::Primary
            || candidate.workload != CandidateWorkload::Primary
        {
            add_reason(reasons, NoGoReason::BinaryHostOrProfileMismatch);
        }
        if baseline.mode != CandidateEnvironment::Off
            || candidate.mode != CandidateEnvironment::CandidateB
        {
            add_reason(reasons, NoGoReason::VariantMismatch);
        }
        validate_pair_semantics_and_counters(&baseline, &candidate, reasons, false);
        if semantic_anchor
            .as_ref()
            .is_some_and(|identity| identity != &baseline.semantic)
        {
            add_reason(reasons, NoGoReason::SemanticMismatch);
        } else if semantic_anchor.is_none() {
            semantic_anchor = Some(baseline.semantic.clone());
        }
        summaries.push((baseline, candidate));
    }
    (summaries.len() == 5).then_some(summaries)
}

fn validate_process(process: &ProcessObservation, reasons: &mut BTreeSet<NoGoReason>) {
    if process.process_identity.is_empty() {
        add_reason(reasons, NoGoReason::NotFiveFreshInterleavedPairs);
    }
    if !process.release_libtest || !process.warm_filesystem_cache {
        add_reason(reasons, NoGoReason::NotReleaseLibtestOrWarmFilesystem);
    }
    if process.exit_code != 0 {
        add_reason(reasons, NoGoReason::ProcessFailed);
    }
    if process.elapsed_us == 0 || process.elapsed_us > MAX_ELAPSED_US {
        add_reason(reasons, NoGoReason::DeadlineExceeded);
    }
    if process.peak_rss_bytes == 0 || process.peak_rss_bytes > MAX_RSS_BYTES {
        add_reason(reasons, NoGoReason::MemoryExceeded);
    }
}

fn pair_shape_is_valid(pairs: &[PairedReleaseRun]) -> bool {
    if pairs.len() != 5 {
        return false;
    }
    let mut identities = BTreeSet::new();
    for (position, pair) in pairs.iter().enumerate() {
        let Ok(expected_pair) = u8::try_from(position + 1) else {
            return false;
        };
        let expected_launch = match expected_pair {
            1 => (1, 2),
            2 => (4, 3),
            3 => (5, 6),
            4 => (8, 7),
            5 => (9, 10),
            _ => return false,
        };
        if pair.pair != expected_pair
            || (pair.baseline.launch_ordinal, pair.candidate.launch_ordinal) != expected_launch
            || pair.baseline.process_identity == pair.candidate.process_identity
            || !identities.insert(pair.baseline.process_identity.as_str())
            || !identities.insert(pair.candidate.process_identity.as_str())
        {
            return false;
        }
    }
    true
}

fn validate_pair_semantics_and_counters(
    baseline: &CandidateSummary,
    candidate: &CandidateSummary,
    reasons: &mut BTreeSet<NoGoReason>,
    control: bool,
) {
    let identity_reason = if control {
        NoGoReason::ControlIdentityOrSemanticMismatch
    } else {
        NoGoReason::SemanticMismatch
    };
    if baseline.semantic != candidate.semantic {
        add_reason(reasons, identity_reason);
    }
    if baseline.eligible_requests != candidate.eligible_requests
        || baseline.clean_outcomes != candidate.clean_outcomes
        || baseline
            .tainted_outcomes
            .checked_sub(candidate.tainted_outcomes)
            != Some(candidate.tainted_cache_hits)
        || candidate
            .executed_visits
            .checked_add(candidate.avoided_visits)
            != Some(baseline.executed_visits)
    {
        add_reason(
            reasons,
            if control {
                NoGoReason::ControlIdentityOrSemanticMismatch
            } else {
                NoGoReason::CounterReconciliationFailed
            },
        );
    }
    if baseline.clean_cache != candidate.clean_cache {
        add_reason(
            reasons,
            if control {
                NoGoReason::ControlIdentityOrSemanticMismatch
            } else {
                NoGoReason::CleanCacheMetricsChanged
            },
        );
    }
}

fn validate_primary_gates(
    pairs: &[PairedReleaseRun],
    summaries: &[(CandidateSummary, CandidateSummary)],
    reasons: &mut BTreeSet<NoGoReason>,
) {
    if !process_observations_are_complete(pairs) {
        return;
    }
    let mut improvements = pairs
        .iter()
        .map(|pair| {
            (
                pair.baseline
                    .elapsed_us
                    .saturating_sub(pair.candidate.elapsed_us),
                pair.baseline.elapsed_us,
            )
        })
        .collect::<Vec<_>>();
    improvements.sort_by(|left, right| {
        (u128::from(left.0) * u128::from(right.1)).cmp(&(u128::from(right.0) * u128::from(left.1)))
    });
    let median = improvements[2];
    if median.1 == 0 || u128::from(median.0) * 100 < u128::from(median.1) * 20 {
        add_reason(reasons, NoGoReason::MedianImprovementBelowTwentyPercent);
    }

    let eligible = summaries
        .iter()
        .map(|(baseline, _)| u128::from(baseline.eligible_requests))
        .sum::<u128>();
    let avoided_runs = summaries
        .iter()
        .map(|(_, candidate)| u128::from(candidate.avoided_runs()))
        .sum::<u128>();
    if eligible == 0 || avoided_runs * 100 < eligible * 80 {
        add_reason(reasons, NoGoReason::AffectedRunReductionBelowEightyPercent);
    }
    let visits = summaries
        .iter()
        .map(|(baseline, _)| u128::from(baseline.executed_visits))
        .sum::<u128>();
    let avoided_visits = summaries
        .iter()
        .map(|(_, candidate)| u128::from(candidate.avoided_visits))
        .sum::<u128>();
    if visits == 0 || avoided_visits * 100 < visits * 80 {
        add_reason(
            reasons,
            NoGoReason::AffectedVisitReductionBelowEightyPercent,
        );
    }
}

fn validate_control(
    control: &ControlEvidence,
    workload: CandidateWorkload,
    probe: &str,
    profile: &str,
    anchor: Option<(&String, &String)>,
    reasons: &mut BTreeSet<NoGoReason>,
) {
    let mut valid =
        pair_shape_is_valid(&control.pairs) && process_observations_are_complete(&control.pairs);
    let mut ratios = Vec::with_capacity(control.pairs.len());
    let mut semantic_anchor = None;
    for pair in &control.pairs {
        for process in [&pair.baseline, &pair.candidate] {
            valid &= process.probe_filter == probe
                && process.profile_identity == profile
                && process.release_libtest
                && process.warm_filesystem_cache
                && process.exit_code == 0
                && process.elapsed_us <= MAX_ELAPSED_US
                && process.peak_rss_bytes <= MAX_RSS_BYTES
                && is_lower_hex(&process.binary_identity)
                && is_lower_hex(&process.host_identity)
                && anchor.is_none_or(|(binary, host)| {
                    process.binary_identity == *binary && process.host_identity == *host
                });
        }
        let (Ok(baseline), Ok(candidate)) = (
            parse_candidate_stdout(&pair.baseline.captured_stdout),
            parse_candidate_stdout(&pair.candidate.captured_stdout),
        ) else {
            valid = false;
            continue;
        };
        valid &= !baseline.saturated
            && !candidate.saturated
            && baseline.workload == workload
            && candidate.workload == workload
            && baseline.mode == CandidateEnvironment::Off
            && candidate.mode == CandidateEnvironment::CandidateB
            && baseline.tainted_cache_hits == 0
            && candidate.tainted_cache_hits == 0
            && baseline.tainted_outcomes == 0
            && candidate.tainted_outcomes == 0
            && baseline.tainted_cache_entries == 0
            && candidate.tainted_cache_entries == 0
            && baseline.avoided_visits == 0
            && candidate.avoided_visits == 0;
        let before = reasons.len();
        validate_pair_semantics_and_counters(&baseline, &candidate, reasons, true);
        if semantic_anchor
            .as_ref()
            .is_some_and(|identity| identity != &baseline.semantic)
        {
            add_reason(reasons, NoGoReason::ControlIdentityOrSemanticMismatch);
        } else if semantic_anchor.is_none() {
            semantic_anchor = Some(baseline.semantic.clone());
        }
        valid &= reasons.len() == before;
        ratios.push((pair.candidate.elapsed_us, pair.baseline.elapsed_us));
    }
    if !valid || ratios.len() != 5 {
        add_reason(reasons, NoGoReason::ControlIdentityOrSemanticMismatch);
        return;
    }
    ratios.sort_by(|left, right| {
        (u128::from(left.0) * u128::from(right.1)).cmp(&(u128::from(right.0) * u128::from(left.1)))
    });
    let median = ratios[2];
    if median.1 == 0 || u128::from(median.0) * 100 > u128::from(median.1) * 102 {
        add_reason(reasons, NoGoReason::ControlRegressionAboveTwoPercent);
    }
}

fn candidate_summary(
    workload: CandidateWorkload,
    mode: CandidateEnvironment,
    cycle: CycleTaintedApplicationCacheMeasure,
    clean: EagerApplicationCacheMeasure,
    semantic: Wu0dSemanticIdentity,
) -> CandidateSummary {
    CandidateSummary {
        workload,
        mode,
        eligible_requests: cycle.eligible_requests,
        executed_runs: cycle.executed_runs,
        tainted_cache_hits: cycle.tainted_cache_hits,
        clean_outcomes: cycle.clean_outcomes,
        tainted_outcomes: cycle.tainted_outcomes,
        executed_visits: cycle.executed_visits,
        memo_hits: cycle.memo_hits,
        expanded_visits: cycle.expanded_visits,
        avoided_visits: cycle.avoided_visits,
        tainted_cache_entries: cycle.tainted_cache_entries,
        clean_cache: CacheMetrics {
            hits: clean.hits,
            misses: clean.misses,
            entries: clean.insertions,
        },
        semantic,
        saturated: cycle.saturated,
    }
}

fn run_candidate_primary_workload(mode: CandidateEnvironment) -> CandidateSummary {
    let profile = super::wu0b_profile::load_strict_profile().expect("strict WU0B profile");
    let primary_workload_sources = profile.injected_sources();
    let _clean_measure = start_eager_application_cache_measure();
    let _cycle_measure = match mode {
        CandidateEnvironment::Off => start_cycle_tainted_application_cache_baseline_measure(),
        CandidateEnvironment::CandidateB => start_cycle_tainted_application_cache_measure(),
    };
    let run =
        run_injected_profile(&primary_workload_sources).expect("exact WU0B profile execution");
    let semantic = canonical_wu0d_semantic_identity(&run.wu0d_semantic_components);
    let cycle = cycle_tainted_application_cache_measure().expect("WU0D cycle measure is active");
    let clean = eager_application_cache_measure().expect("WU0D clean measure is active");
    candidate_summary(CandidateWorkload::Primary, mode, cycle, clean, semantic)
}

fn run_candidate_non_cycle_workload(mode: CandidateEnvironment) -> CandidateSummary {
    let _clean_measure = start_eager_application_cache_measure();
    let _cycle_measure = match mode {
        CandidateEnvironment::Off => start_cycle_tainted_application_cache_baseline_measure(),
        CandidateEnvironment::CandidateB => start_cycle_tainted_application_cache_measure(),
    };
    let run = run_injected_profile(NON_CYCLE_WORKLOAD_SOURCES).expect("non-cycle control profile");
    let semantic = canonical_wu0d_semantic_identity(&run.wu0d_semantic_components);
    let cycle = cycle_tainted_application_cache_measure().expect("WU0D cycle measure is active");
    let clean = eager_application_cache_measure().expect("WU0D clean measure is active");
    let summary = candidate_summary(CandidateWorkload::NonCycle, mode, cycle, clean, semantic);
    assert_eq!(summary.tainted_cache_hits, 0);
    assert_eq!(summary.tainted_cache_entries, 0);
    summary
}

fn run_candidate_reporter_control_workload(mode: CandidateEnvironment) -> CandidateSummary {
    let _clean_measure = start_eager_application_cache_measure();
    let _cycle_measure = match mode {
        CandidateEnvironment::Off => start_cycle_tainted_application_cache_baseline_measure(),
        CandidateEnvironment::CandidateB => start_cycle_tainted_application_cache_measure(),
    };
    let run = run_injected_profile(REPORTER_CONTROL_WORKLOAD_SOURCES)
        .expect("reporter-control profile execution");
    assert!(!run.library_records.is_empty());
    let semantic = canonical_wu0d_semantic_identity(&run.wu0d_semantic_components);
    let cycle = cycle_tainted_application_cache_measure().expect("WU0D cycle measure is active");
    let clean = eager_application_cache_measure().expect("WU0D clean measure is active");
    let summary = candidate_summary(
        CandidateWorkload::ReporterControl,
        mode,
        cycle,
        clean,
        semantic,
    );
    assert!(summary.semantic.library_ledger.byte_len > 0);
    assert_eq!(summary.tainted_cache_hits, 0);
    assert_eq!(summary.tainted_cache_entries, 0);
    summary
}

#[test]
#[ignore = "release-only WU0D primary probe"]
fn wu0d_candidate_primary_probe_once() {
    let mode = resolve_candidate_environment().expect("strict WU0D environment");
    let summary = run_candidate_primary_workload(mode);
    println!("{}", summary.render());
}

#[test]
#[ignore = "release-only WU0D non-cycle control"]
fn wu0d_candidate_non_cycle_probe_once() {
    let mode = resolve_candidate_environment().expect("strict WU0D environment");
    let summary = run_candidate_non_cycle_workload(mode);
    println!("{}", summary.render());
}

#[test]
#[ignore = "release-only WU0D reporter control"]
fn wu0d_candidate_reporter_control_probe_once() {
    let mode = resolve_candidate_environment().expect("strict WU0D environment");
    let summary = run_candidate_reporter_control_workload(mode);
    println!("{}", summary.render());
}

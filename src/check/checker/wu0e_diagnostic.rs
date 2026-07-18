use super::context::{
    cycle_tainted_application_cache_measure, eager_application_cache_measure,
    start_cycle_tainted_application_cache_baseline_measure,
    start_cycle_tainted_application_cache_measure, start_eager_application_cache_measure,
};
use super::wu0b_library::{InjectedLibrarySource, InjectedProfileError, InjectedProfileRun};
use super::wu0e_observer::{DiagnosticObserver, ObserverClock};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const PREFIX: &str = "typokat-wu0e-diagnostic-v1";
pub(super) const MAX_DIAGNOSTIC_TRACE_BYTES: usize = 256 * 1_024;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum DiagnosticMode {
    Plain,
    MeasuredOff,
    CandidateB,
}

impl DiagnosticMode {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::MeasuredOff => "measured-off",
            Self::CandidateB => "candidate-b",
        }
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "plain" => Some(Self::Plain),
            "measured-off" => Some(Self::MeasuredOff),
            "candidate-b" => Some(Self::CandidateB),
            _ => None,
        }
    }

    pub(super) const fn measurement_selection(self) -> MeasurementSelection {
        match self {
            Self::Plain => MeasurementSelection::None,
            Self::MeasuredOff => MeasurementSelection::EagerAndCycleBaseline,
            Self::CandidateB => MeasurementSelection::EagerAndCandidateB,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum MeasurementSelection {
    None,
    EagerAndCycleBaseline,
    EagerAndCandidateB,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum DiagnosticTermination {
    Normal,
    Deadline,
    Rss,
    Stdout,
    Stderr,
    Trace,
    Crash,
    Infrastructure,
}

impl DiagnosticTermination {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Deadline => "deadline",
            Self::Rss => "rss",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::Trace => "trace",
            Self::Crash => "crash",
            Self::Infrastructure => "infrastructure",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "deadline" => Some(Self::Deadline),
            "rss" => Some(Self::Rss),
            "stdout" => Some(Self::Stdout),
            "stderr" => Some(Self::Stderr),
            "trace" => Some(Self::Trace),
            "crash" => Some(Self::Crash),
            "infrastructure" => Some(Self::Infrastructure),
            _ => None,
        }
    }

    const fn permits_partial(self) -> bool {
        matches!(
            self,
            Self::Deadline | Self::Rss | Self::Stdout | Self::Stderr | Self::Trace
        )
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum DiagnosticPhase {
    ProfileLoad,
    Parse,
    Bind,
    LexicalReservation,
    TypeReservation,
    PassConstruction,
    FillParameterMetadata,
    FillInterfaceScc,
    FillConditional,
    FillMapped,
    FillObject,
    FillRemaining,
    PrepareAttachedNamespaceValues,
    PrepareStandaloneNamespaceValues,
    PublishClassSurfaces,
    FinalizeStandaloneNamespaceValues,
    PrecomputeStandaloneNamespaceAliases,
    FillPendingInterfaces,
    PublishTypeGroups,
    ValidatePublishedClassSurfaces,
    CaptureLexicalEvidence,
    StatementFile,
    SemanticFinalization,
    CollectTypeProbes,
    CollectGlobalValueProbes,
    CollectModuleValueProbes,
    CollectNamespaceValueProbes,
    FreezeLibraryProduct,
    CompleteSemanticBatches,
    ConsumeBinderOutcomes,
    FinishLedger,
    BuildSemanticComponents,
    SemanticDigest,
}

impl DiagnosticPhase {
    pub(super) const ALL: [Self; 33] = [
        Self::ProfileLoad,
        Self::Parse,
        Self::Bind,
        Self::LexicalReservation,
        Self::TypeReservation,
        Self::PassConstruction,
        Self::FillParameterMetadata,
        Self::FillInterfaceScc,
        Self::FillConditional,
        Self::FillMapped,
        Self::FillObject,
        Self::FillRemaining,
        Self::PrepareAttachedNamespaceValues,
        Self::PrepareStandaloneNamespaceValues,
        Self::PublishClassSurfaces,
        Self::FinalizeStandaloneNamespaceValues,
        Self::PrecomputeStandaloneNamespaceAliases,
        Self::FillPendingInterfaces,
        Self::PublishTypeGroups,
        Self::ValidatePublishedClassSurfaces,
        Self::CaptureLexicalEvidence,
        Self::StatementFile,
        Self::SemanticFinalization,
        Self::CollectTypeProbes,
        Self::CollectGlobalValueProbes,
        Self::CollectModuleValueProbes,
        Self::CollectNamespaceValueProbes,
        Self::FreezeLibraryProduct,
        Self::CompleteSemanticBatches,
        Self::ConsumeBinderOutcomes,
        Self::FinishLedger,
        Self::BuildSemanticComponents,
        Self::SemanticDigest,
    ];
    pub(super) const COUNT: usize = Self::ALL.len();

    pub(super) fn ordinal(self) -> usize {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .expect("diagnostic phase belongs to the closed inventory")
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::ProfileLoad => "profile-load",
            Self::Parse => "parse",
            Self::Bind => "bind",
            Self::LexicalReservation => "lexical-reservation",
            Self::TypeReservation => "type-reservation",
            Self::PassConstruction => "pass-construction",
            Self::FillParameterMetadata => "fill-parameter-metadata",
            Self::FillInterfaceScc => "fill-interface-scc",
            Self::FillConditional => "fill-conditional",
            Self::FillMapped => "fill-mapped",
            Self::FillObject => "fill-object",
            Self::FillRemaining => "fill-remaining",
            Self::PrepareAttachedNamespaceValues => "prepare-attached-namespace-values",
            Self::PrepareStandaloneNamespaceValues => "prepare-standalone-namespace-values",
            Self::PublishClassSurfaces => "publish-class-surfaces",
            Self::FinalizeStandaloneNamespaceValues => "finalize-standalone-namespace-values",
            Self::PrecomputeStandaloneNamespaceAliases => "precompute-standalone-namespace-aliases",
            Self::FillPendingInterfaces => "fill-pending-interfaces",
            Self::PublishTypeGroups => "publish-type-groups",
            Self::ValidatePublishedClassSurfaces => "validate-published-class-surfaces",
            Self::CaptureLexicalEvidence => "capture-lexical-evidence",
            Self::StatementFile => "statement-file",
            Self::SemanticFinalization => "semantic-finalization",
            Self::CollectTypeProbes => "collect-type-probes",
            Self::CollectGlobalValueProbes => "collect-global-value-probes",
            Self::CollectModuleValueProbes => "collect-module-value-probes",
            Self::CollectNamespaceValueProbes => "collect-namespace-value-probes",
            Self::FreezeLibraryProduct => "freeze-library-product",
            Self::CompleteSemanticBatches => "complete-semantic-batches",
            Self::ConsumeBinderOutcomes => "consume-binder-outcomes",
            Self::FinishLedger => "finish-ledger",
            Self::BuildSemanticComponents => "build-semantic-components",
            Self::SemanticDigest => "semantic-digest",
        }
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|candidate| candidate.as_str() == value)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct DiagnosticPhaseKey {
    phase: DiagnosticPhase,
    item: Option<u32>,
}

impl DiagnosticPhaseKey {
    pub(super) const fn singleton(phase: DiagnosticPhase) -> Self {
        Self { phase, item: None }
    }

    pub(super) const fn statement_file(item: u32) -> Self {
        Self {
            phase: DiagnosticPhase::StatementFile,
            item: Some(item),
        }
    }

    pub(super) const fn phase(self) -> DiagnosticPhase {
        self.phase
    }

    fn item(self) -> Option<u32> {
        self.item
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiagnosticMeasurement {
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
    pub(super) clean_cache_hits: u64,
    pub(super) clean_cache_misses: u64,
    pub(super) clean_cache_entries: u64,
    pub(super) saturated: bool,
}

impl DiagnosticMeasurement {
    pub(super) fn validate(&self, mode: DiagnosticMode) -> Result<(), String> {
        if mode == DiagnosticMode::Plain || self.saturated {
            return Err("invalid diagnostic measurement mode or saturation".to_owned());
        }
        if self.executed_runs.checked_add(self.tainted_cache_hits) != Some(self.eligible_requests)
            || self.clean_outcomes.checked_add(self.tainted_outcomes) != Some(self.executed_runs)
            || self.memo_hits.checked_add(self.expanded_visits) != Some(self.executed_visits)
            || self
                .clean_cache_hits
                .checked_add(self.clean_cache_misses)
                .is_none()
        {
            return Err("invalid diagnostic measurement arithmetic".to_owned());
        }
        if mode == DiagnosticMode::MeasuredOff
            && (self.tainted_cache_hits != 0
                || self.tainted_cache_entries != 0
                || self.avoided_visits != 0)
        {
            return Err("measured-off exposed Candidate-B effects".to_owned());
        }
        Ok(())
    }
}

pub(super) fn reconcile_measurements(
    baseline: &DiagnosticMeasurement,
    candidate: &DiagnosticMeasurement,
) -> Result<(), String> {
    baseline.validate(DiagnosticMode::MeasuredOff)?;
    candidate.validate(DiagnosticMode::CandidateB)?;
    if baseline.eligible_requests != candidate.eligible_requests
        || baseline.clean_outcomes != candidate.clean_outcomes
        || baseline.tainted_outcomes
            != candidate
                .tainted_outcomes
                .checked_add(candidate.tainted_cache_hits)
                .ok_or_else(|| "tainted outcome reconciliation overflow".to_owned())?
        || baseline.executed_visits
            != candidate
                .executed_visits
                .checked_add(candidate.avoided_visits)
                .ok_or_else(|| "visit reconciliation overflow".to_owned())?
        || baseline.clean_cache_hits != candidate.clean_cache_hits
        || baseline.clean_cache_misses != candidate.clean_cache_misses
        || baseline.clean_cache_entries != candidate.clean_cache_entries
    {
        return Err("diagnostic measurement pair does not reconcile".to_owned());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DiagnosticEvent {
    RunStart {
        mode: DiagnosticMode,
        elapsed_us: u64,
    },
    PhaseEnter {
        sequence: usize,
        key: DiagnosticPhaseKey,
        elapsed_us: u64,
    },
    PhaseExit {
        sequence: usize,
        key: DiagnosticPhaseKey,
        elapsed_us: u64,
    },
    Measurement {
        mode: DiagnosticMode,
        measurement: DiagnosticMeasurement,
        elapsed_us: u64,
    },
    Semantic {
        aggregate_sha256: String,
        elapsed_us: u64,
    },
    RunFinish {
        elapsed_us: u64,
    },
}

pub(super) fn render_diagnostic_event_for_test(event: &DiagnosticEvent) -> String {
    match event {
        DiagnosticEvent::RunStart { mode, elapsed_us } => {
            format!("{PREFIX} event=run-start mode={} elapsed_us={elapsed_us}", mode.as_str())
        }
        DiagnosticEvent::PhaseEnter {
            sequence,
            key,
            elapsed_us,
        } => render_phase_event("phase-enter", *sequence, *key, *elapsed_us),
        DiagnosticEvent::PhaseExit {
            sequence,
            key,
            elapsed_us,
        } => render_phase_event("phase-exit", *sequence, *key, *elapsed_us),
        DiagnosticEvent::Measurement {
            mode,
            measurement,
            elapsed_us,
        } => format!(
            "{PREFIX} event=measurement mode={} eligible_requests={} executed_runs={} tainted_cache_hits={} clean_outcomes={} tainted_outcomes={} executed_visits={} memo_hits={} expanded_visits={} avoided_visits={} tainted_cache_entries={} clean_cache_hits={} clean_cache_misses={} clean_cache_entries={} saturated={} elapsed_us={elapsed_us}",
            mode.as_str(),
            measurement.eligible_requests,
            measurement.executed_runs,
            measurement.tainted_cache_hits,
            measurement.clean_outcomes,
            measurement.tainted_outcomes,
            measurement.executed_visits,
            measurement.memo_hits,
            measurement.expanded_visits,
            measurement.avoided_visits,
            measurement.tainted_cache_entries,
            measurement.clean_cache_hits,
            measurement.clean_cache_misses,
            measurement.clean_cache_entries,
            u8::from(measurement.saturated),
        ),
        DiagnosticEvent::Semantic {
            aggregate_sha256,
            elapsed_us,
        } => format!(
            "{PREFIX} event=semantic aggregate_sha256={aggregate_sha256} elapsed_us={elapsed_us}"
        ),
        DiagnosticEvent::RunFinish { elapsed_us } => {
            format!("{PREFIX} event=run-finish elapsed_us={elapsed_us}")
        }
    }
}

fn render_phase_event(
    event: &str,
    sequence: usize,
    key: DiagnosticPhaseKey,
    elapsed_us: u64,
) -> String {
    let item = key
        .item()
        .map_or_else(|| "none".to_owned(), |value| value.to_string());
    format!(
        "{PREFIX} event={event} sequence={sequence} phase={} item={item} elapsed_us={elapsed_us}",
        key.phase().as_str()
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiagnosticEnvironment {
    pub(super) mode: DiagnosticMode,
    pub(super) trace_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiagnosticValidationEnvironment {
    pub(super) trace_path: PathBuf,
    pub(super) mode: DiagnosticMode,
    pub(super) termination: DiagnosticTermination,
}

pub(super) fn resolve_diagnostic_environment_for_test<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<DiagnosticEnvironment, String> {
    let entries = entries
        .into_iter()
        .map(|(key, value)| (key.as_bytes(), value.as_bytes()));
    resolve_diagnostic_environment_bytes_for_test(entries)
}

pub(super) fn resolve_diagnostic_environment_bytes_for_test<'a>(
    entries: impl IntoIterator<Item = (&'a [u8], &'a [u8])>,
) -> Result<DiagnosticEnvironment, String> {
    let mut mode = None;
    let mut trace_path = None;
    for (key, value) in entries {
        let key = std::str::from_utf8(key).map_err(|_| "non-UTF-8 WU0E key".to_owned())?;
        let value = std::str::from_utf8(value).map_err(|_| "non-UTF-8 WU0E value".to_owned())?;
        match key {
            "TYPOKAT_WU0E_MODE" if mode.is_none() => {
                mode = DiagnosticMode::parse(value);
                if mode.is_none() {
                    return Err("invalid WU0E mode".to_owned());
                }
            }
            "TYPOKAT_WU0E_TRACE_PATH" if trace_path.is_none() => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("WU0E trace path must be absolute".to_owned());
                }
                trace_path = Some(path);
            }
            _ => return Err("unknown or duplicate WU0E workload variable".to_owned()),
        }
    }
    Ok(DiagnosticEnvironment {
        mode: mode.ok_or_else(|| "missing WU0E mode".to_owned())?,
        trace_path: trace_path.ok_or_else(|| "missing WU0E trace path".to_owned())?,
    })
}

pub(super) fn resolve_diagnostic_validation_environment_for_test<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<DiagnosticValidationEnvironment, String> {
    let mut trace_path = None;
    let mut mode = None;
    let mut termination = None;
    for (key, value) in entries {
        match key {
            "TYPOKAT_WU0E_VALIDATE_TRACE_PATH" if trace_path.is_none() => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("WU0E validation trace path must be absolute".to_owned());
                }
                trace_path = Some(path);
            }
            "TYPOKAT_WU0E_VALIDATE_MODE" if mode.is_none() => {
                mode = DiagnosticMode::parse(value);
                if mode.is_none() {
                    return Err("invalid WU0E validation mode".to_owned());
                }
            }
            "TYPOKAT_WU0E_VALIDATE_TERMINATION" if termination.is_none() => {
                termination = DiagnosticTermination::parse(value);
                if termination.is_none() {
                    return Err("invalid WU0E validation termination".to_owned());
                }
            }
            _ => return Err("unknown or duplicate WU0E validation variable".to_owned()),
        }
    }
    Ok(DiagnosticValidationEnvironment {
        trace_path: trace_path.ok_or_else(|| "missing WU0E validation trace path".to_owned())?,
        mode: mode.ok_or_else(|| "missing WU0E validation mode".to_owned())?,
        termination: termination.ok_or_else(|| "missing WU0E validation termination".to_owned())?,
    })
}

fn ensure_real_absolute_parent(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("path must be absolute".to_owned());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "path has no parent".to_owned())?;
    let mut current = PathBuf::new();
    for component in parent.components() {
        match component {
            Component::RootDir => current.push(component.as_os_str()),
            Component::Normal(value) => {
                current.push(value);
                let metadata = std::fs::symlink_metadata(&current)
                    .map_err(|error| format!("inspect trace parent: {error}"))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("trace parent must be a real directory".to_owned());
                }
            }
            _ => return Err("trace path contains a non-canonical component".to_owned()),
        }
    }
    Ok(())
}

pub(super) struct DiagnosticTraceSink {
    writer: SinkWriter,
    written: usize,
}

enum SinkWriter {
    File(File),
    Boundary(BoundaryTestSink),
}

impl DiagnosticTraceSink {
    pub(super) fn create(path: &Path) -> Result<Self, String> {
        ensure_real_absolute_parent(path)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("create WU0E trace: {error}"))?;
        Self::from_file(file)
    }

    fn from_file(file: File) -> Result<Self, String> {
        if !file
            .metadata()
            .map_err(|error| format!("inspect WU0E trace: {error}"))?
            .is_file()
        {
            return Err("WU0E trace is not a regular file".to_owned());
        }
        Ok(Self {
            writer: SinkWriter::File(file),
            written: 0,
        })
    }

    pub(super) fn from_file_for_test(file: File) -> Result<Self, String> {
        Self::from_file(file)
    }

    fn from_boundary(sink: BoundaryTestSink) -> Self {
        Self {
            writer: SinkWriter::Boundary(sink),
            written: 0,
        }
    }

    pub(super) fn write_bytes_for_test(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.write_record(bytes, false)
    }

    fn quiet_start(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.write_record(bytes, true)
    }

    fn write_record(&mut self, bytes: &[u8], quiet: bool) -> Result<(), String> {
        let next = self
            .written
            .checked_add(bytes.len())
            .ok_or_else(|| "WU0E trace size overflow".to_owned())?;
        if next > MAX_DIAGNOSTIC_TRACE_BYTES {
            return Err("WU0E trace exceeded its write bound".to_owned());
        }
        match &mut self.writer {
            SinkWriter::File(file) => {
                file.write_all(bytes)
                    .map_err(|error| format!("write WU0E trace: {error}"))?;
                file.flush()
                    .map_err(|error| format!("flush WU0E trace: {error}"))?;
            }
            SinkWriter::Boundary(sink) => sink.write_and_flush(bytes, quiet)?,
        }
        self.written = next;
        Ok(())
    }

    fn callback_observed(&self) {
        if let SinkWriter::Boundary(sink) = &self.writer {
            sink.observe(BoundaryFixtureObservation::Callback);
        }
    }

    fn semantic_continuation(&self) {
        if let SinkWriter::Boundary(sink) = &self.writer {
            sink.observe(BoundaryFixtureObservation::SemanticContinuation);
        }
    }
}

pub(super) struct DiagnosticBoundaryAdapter {
    mode: Option<DiagnosticMode>,
    observer: DiagnosticObserver,
    sink: Option<DiagnosticTraceSink>,
}

impl DiagnosticBoundaryAdapter {
    pub(super) fn disabled() -> Self {
        Self {
            mode: None,
            observer: DiagnosticObserver::new(ObserverClock::monotonic()),
            sink: None,
        }
    }

    fn new(
        mode: DiagnosticMode,
        clock: ObserverClock,
        mut sink: DiagnosticTraceSink,
    ) -> Result<Self, String> {
        let start = render_diagnostic_event_for_test(&DiagnosticEvent::RunStart {
            mode,
            elapsed_us: 0,
        }) + "\n";
        sink.quiet_start(start.as_bytes())?;
        Ok(Self {
            mode: Some(mode),
            observer: DiagnosticObserver::new(clock),
            sink: Some(sink),
        })
    }

    pub(super) fn enter_phase(
        &mut self,
        phase: DiagnosticPhase,
        item: Option<u32>,
    ) -> Result<(), InjectedProfileError> {
        let Some(_) = self.mode else {
            return Ok(());
        };
        let key = phase_key(phase, item).map_err(InjectedProfileError::CanonicalProjection)?;
        let (sequence, elapsed_us) = self
            .observer
            .enter(key)
            .map_err(|error| InjectedProfileError::CanonicalProjection(format!("{error:?}")))?;
        let event = DiagnosticEvent::PhaseEnter {
            sequence,
            key,
            elapsed_us,
        };
        self.commit_boundary(event)
            .map_err(InjectedProfileError::CanonicalProjection)
    }

    pub(super) fn exit_phase(
        &mut self,
        phase: DiagnosticPhase,
        item: Option<u32>,
    ) -> Result<(), InjectedProfileError> {
        let Some(_) = self.mode else {
            return Ok(());
        };
        let key = phase_key(phase, item).map_err(InjectedProfileError::CanonicalProjection)?;
        let (sequence, elapsed_us) = self
            .observer
            .exit(key)
            .map_err(|error| InjectedProfileError::CanonicalProjection(format!("{error:?}")))?;
        let event = DiagnosticEvent::PhaseExit {
            sequence,
            key,
            elapsed_us,
        };
        self.commit_boundary(event)
            .map_err(InjectedProfileError::CanonicalProjection)
    }

    fn commit_boundary(&mut self, event: DiagnosticEvent) -> Result<(), String> {
        let sink = self
            .sink
            .as_mut()
            .ok_or_else(|| "enabled WU0E boundary has no sink".to_owned())?;
        sink.callback_observed();
        let record = render_diagnostic_event_for_test(&event) + "\n";
        sink.write_record(record.as_bytes(), false)
    }

    fn emit(&mut self, event: DiagnosticEvent) -> Result<(), String> {
        let Some(sink) = self.sink.as_mut() else {
            return Ok(());
        };
        let record = render_diagnostic_event_for_test(&event) + "\n";
        sink.write_record(record.as_bytes(), false)
    }

    fn elapsed(&mut self) -> Result<u64, String> {
        self.observer
            .elapsed()
            .map_err(|error| format!("WU0E clock failure: {error:?}"))
    }

    fn semantic_continuation(&self) {
        if let Some(sink) = &self.sink {
            sink.semantic_continuation();
        }
    }
}

fn phase_key(phase: DiagnosticPhase, item: Option<u32>) -> Result<DiagnosticPhaseKey, String> {
    match (phase, item) {
        (DiagnosticPhase::StatementFile, Some(item)) => {
            Ok(DiagnosticPhaseKey::statement_file(item))
        }
        (DiagnosticPhase::StatementFile, None) => {
            Err("statement-file requires an ordinal".to_owned())
        }
        (_, Some(_)) => Err("singleton diagnostic phase cannot carry an item".to_owned()),
        (_, None) => Ok(DiagnosticPhaseKey::singleton(phase)),
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum BoundaryFixtureObservation {
    Callback,
    Write,
    Flush,
    SemanticContinuation,
}

#[derive(Clone)]
pub(super) struct BoundaryTestClock([u64; 2]);

impl BoundaryTestClock {
    pub(super) fn new(ticks: [u64; 2]) -> Self {
        Self(ticks)
    }
}

struct BoundaryControlState {
    observations: Vec<BoundaryFixtureObservation>,
    flush_blocked: bool,
    abort_flush: bool,
    observe_boundaries: bool,
}

impl Default for BoundaryControlState {
    fn default() -> Self {
        Self {
            observations: Vec::new(),
            flush_blocked: false,
            abort_flush: false,
            observe_boundaries: true,
        }
    }
}

#[derive(Clone)]
pub(super) struct BoundaryTestControl {
    shared: Arc<(Mutex<BoundaryControlState>, Condvar)>,
}

impl BoundaryTestControl {
    pub(super) fn observations(&self) -> Vec<BoundaryFixtureObservation> {
        self.shared
            .0
            .lock()
            .map(|state| state.observations.clone())
            .unwrap_or_default()
    }

    pub(super) fn wait_until_flush_blocked(&self, timeout: Duration) -> Result<(), String> {
        let state = self
            .shared
            .0
            .lock()
            .map_err(|_| "boundary test control poisoned".to_owned())?;
        let (state, wait) = self
            .shared
            .1
            .wait_timeout_while(state, timeout, |state| !state.flush_blocked)
            .map_err(|_| "boundary test control poisoned".to_owned())?;
        if wait.timed_out() && !state.flush_blocked {
            return Err("boundary flush did not block".to_owned());
        }
        Ok(())
    }

    pub(super) fn abort_flush(&self) {
        if let Ok(mut state) = self.shared.0.lock() {
            state.abort_flush = true;
            self.shared.1.notify_all();
        }
    }
}

pub(super) struct BoundaryTestSink {
    file: File,
    control: BoundaryTestControl,
    block_flush: bool,
}

impl BoundaryTestSink {
    pub(super) fn recording_file(path: &Path) -> Result<(Self, BoundaryTestControl), String> {
        Self::file(path, false)
    }

    pub(super) fn blocking_file(path: &Path) -> Result<(Self, BoundaryTestControl), String> {
        Self::file(path, true)
    }

    fn file(path: &Path, block_flush: bool) -> Result<(Self, BoundaryTestControl), String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("create boundary test sink: {error}"))?;
        let control = BoundaryTestControl {
            shared: Arc::new((Mutex::new(BoundaryControlState::default()), Condvar::new())),
        };
        Ok((
            Self {
                file,
                control: control.clone(),
                block_flush,
            },
            control,
        ))
    }

    fn observe(&self, observation: BoundaryFixtureObservation) {
        if let Ok(mut state) = self.control.shared.0.lock() {
            if !state.observe_boundaries {
                return;
            }
            state.observations.push(observation);
            if observation == BoundaryFixtureObservation::SemanticContinuation {
                state.observe_boundaries = false;
            }
        }
    }

    fn write_and_flush(&mut self, bytes: &[u8], quiet: bool) -> Result<(), String> {
        if !quiet {
            self.observe(BoundaryFixtureObservation::Write);
        }
        self.file
            .write_all(bytes)
            .map_err(|error| format!("write boundary test sink: {error}"))?;
        if quiet {
            return self
                .file
                .flush()
                .map_err(|error| format!("flush boundary test sink: {error}"));
        }
        self.observe(BoundaryFixtureObservation::Flush);
        if self.block_flush {
            let mut state = self
                .control
                .shared
                .0
                .lock()
                .map_err(|_| "boundary test control poisoned".to_owned())?;
            state.flush_blocked = true;
            self.control.shared.1.notify_all();
            while !state.abort_flush {
                state = self
                    .control
                    .shared
                    .1
                    .wait(state)
                    .map_err(|_| "boundary test control poisoned".to_owned())?;
            }
            return Err("boundary flush aborted".to_owned());
        }
        self.file
            .flush()
            .map_err(|error| format!("flush boundary test sink: {error}"))
    }
}

pub(super) fn run_boundary_fixture_for_test(
    mode: DiagnosticMode,
    key: DiagnosticPhaseKey,
    clock: BoundaryTestClock,
    sink: BoundaryTestSink,
    continuation: impl FnOnce(),
) -> Result<(), String> {
    let mut boundary = DiagnosticBoundaryAdapter::new(
        mode,
        ObserverClock::fixed(clock.0),
        DiagnosticTraceSink::from_boundary(sink),
    )?;
    let enter = boundary.enter_phase(key.phase(), key.item());
    enter.map_err(|error| format!("{error:?}"))?;
    boundary.semantic_continuation();
    continuation();
    let exit = boundary.exit_phase(key.phase(), key.item());
    exit.map_err(|error| format!("{error:?}"))?;
    Ok(())
}

fn canonical_u64(value: &str) -> Result<u64, String> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("non-canonical unsigned decimal".to_owned());
    }
    value
        .parse::<u64>()
        .map_err(|_| "unsigned decimal overflow".to_owned())
}

fn canonical_usize(value: &str) -> Result<usize, String> {
    usize::try_from(canonical_u64(value)?)
        .map_err(|_| "unsigned decimal does not fit usize".to_owned())
}

fn field<'a>(token: &'a str, key: &str) -> Result<&'a str, String> {
    token
        .strip_prefix(key)
        .ok_or_else(|| format!("expected field {key}"))
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_phase_key(phase: &str, item: &str) -> Result<DiagnosticPhaseKey, String> {
    let phase =
        DiagnosticPhase::parse(phase).ok_or_else(|| "unknown WU0E diagnostic phase".to_owned())?;
    if phase == DiagnosticPhase::StatementFile {
        let item = u32::try_from(canonical_u64(item)?)
            .map_err(|_| "statement ordinal does not fit u32".to_owned())?;
        Ok(DiagnosticPhaseKey::statement_file(item))
    } else if item == "none" {
        Ok(DiagnosticPhaseKey::singleton(phase))
    } else {
        Err("invalid WU0E phase item".to_owned())
    }
}

fn parse_event_line(line: &str) -> Result<DiagnosticEvent, String> {
    if !line.is_ascii() || line.contains('\r') {
        return Err("WU0E record is not canonical ASCII".to_owned());
    }
    let tokens = line.split(' ').collect::<Vec<_>>();
    if tokens.first().copied() != Some(PREFIX) || tokens.iter().any(|token| token.is_empty()) {
        return Err("invalid WU0E record prefix or spacing".to_owned());
    }
    let event = tokens
        .get(1)
        .copied()
        .ok_or_else(|| "missing WU0E event".to_owned())?;
    match event {
        "event=run-start" if tokens.len() == 4 => {
            let mode = DiagnosticMode::parse(field(tokens[2], "mode=")?)
                .ok_or_else(|| "invalid WU0E run mode".to_owned())?;
            let elapsed_us = canonical_u64(field(tokens[3], "elapsed_us=")?)?;
            Ok(DiagnosticEvent::RunStart { mode, elapsed_us })
        }
        "event=phase-enter" | "event=phase-exit" if tokens.len() == 6 => {
            let sequence = canonical_usize(field(tokens[2], "sequence=")?)?;
            let key = parse_phase_key(field(tokens[3], "phase=")?, field(tokens[4], "item=")?)?;
            let elapsed_us = canonical_u64(field(tokens[5], "elapsed_us=")?)?;
            if event == "event=phase-enter" {
                Ok(DiagnosticEvent::PhaseEnter {
                    sequence,
                    key,
                    elapsed_us,
                })
            } else {
                Ok(DiagnosticEvent::PhaseExit {
                    sequence,
                    key,
                    elapsed_us,
                })
            }
        }
        "event=measurement" if tokens.len() == 18 => {
            let mode = DiagnosticMode::parse(field(tokens[2], "mode=")?)
                .ok_or_else(|| "invalid WU0E measurement mode".to_owned())?;
            let counter = |index: usize, name: &str| -> Result<u64, String> {
                canonical_u64(field(tokens[index], name)?)
            };
            let saturated = match field(tokens[16], "saturated=")? {
                "0" => false,
                "1" => true,
                _ => return Err("invalid WU0E saturation bit".to_owned()),
            };
            let measurement = DiagnosticMeasurement {
                eligible_requests: counter(3, "eligible_requests=")?,
                executed_runs: counter(4, "executed_runs=")?,
                tainted_cache_hits: counter(5, "tainted_cache_hits=")?,
                clean_outcomes: counter(6, "clean_outcomes=")?,
                tainted_outcomes: counter(7, "tainted_outcomes=")?,
                executed_visits: counter(8, "executed_visits=")?,
                memo_hits: counter(9, "memo_hits=")?,
                expanded_visits: counter(10, "expanded_visits=")?,
                avoided_visits: counter(11, "avoided_visits=")?,
                tainted_cache_entries: counter(12, "tainted_cache_entries=")?,
                clean_cache_hits: counter(13, "clean_cache_hits=")?,
                clean_cache_misses: counter(14, "clean_cache_misses=")?,
                clean_cache_entries: counter(15, "clean_cache_entries=")?,
                saturated,
            };
            let elapsed_us = canonical_u64(field(tokens[17], "elapsed_us=")?)?;
            measurement.validate(mode)?;
            Ok(DiagnosticEvent::Measurement {
                mode,
                measurement,
                elapsed_us,
            })
        }
        "event=semantic" if tokens.len() == 4 => {
            let aggregate_sha256 = field(tokens[2], "aggregate_sha256=")?;
            if !lower_sha256(aggregate_sha256) {
                return Err("invalid WU0E semantic SHA-256".to_owned());
            }
            Ok(DiagnosticEvent::Semantic {
                aggregate_sha256: aggregate_sha256.to_owned(),
                elapsed_us: canonical_u64(field(tokens[3], "elapsed_us=")?)?,
            })
        }
        "event=run-finish" if tokens.len() == 3 => Ok(DiagnosticEvent::RunFinish {
            elapsed_us: canonical_u64(field(tokens[2], "elapsed_us=")?)?,
        }),
        _ => Err("unknown or malformed WU0E record".to_owned()),
    }
}

fn expanded_phase_plan(statement_ordinals: &[u32]) -> Vec<DiagnosticPhaseKey> {
    let mut plan = DiagnosticPhase::ALL[..21]
        .iter()
        .copied()
        .map(DiagnosticPhaseKey::singleton)
        .collect::<Vec<_>>();
    plan.extend(
        statement_ordinals
            .iter()
            .copied()
            .map(DiagnosticPhaseKey::statement_file),
    );
    plan.extend(
        DiagnosticPhase::ALL[22..]
            .iter()
            .copied()
            .map(DiagnosticPhaseKey::singleton),
    );
    plan
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ParsedDiagnosticTrace {
    mode: DiagnosticMode,
    completed_phases: Vec<DiagnosticPhaseKey>,
    active_phase: Option<DiagnosticPhaseKey>,
    measurement: Option<DiagnosticMeasurement>,
    semantic_sha256: Option<String>,
    finished: bool,
    truncated_tail: bool,
}

impl ParsedDiagnosticTrace {
    pub(super) const fn mode(&self) -> DiagnosticMode {
        self.mode
    }

    pub(super) fn completed_phases(&self) -> Vec<DiagnosticPhaseKey> {
        self.completed_phases.clone()
    }

    pub(super) const fn active_phase(&self) -> Option<DiagnosticPhaseKey> {
        self.active_phase
    }

    pub(super) fn measurement(&self) -> Option<&DiagnosticMeasurement> {
        self.measurement.as_ref()
    }

    pub(super) fn semantic_sha256(&self) -> Option<&str> {
        self.semantic_sha256.as_deref()
    }

    pub(super) const fn finished(&self) -> bool {
        self.finished
    }

    pub(super) const fn truncated_tail(&self) -> bool {
        self.truncated_tail
    }
}

pub(super) fn parse_diagnostic_trace(
    bytes: &[u8],
    expected_mode: DiagnosticMode,
    statement_ordinals: &[u32],
    termination: DiagnosticTermination,
) -> Result<ParsedDiagnosticTrace, String> {
    if bytes.len() > MAX_DIAGNOSTIC_TRACE_BYTES {
        return Err("WU0E trace exceeds its validation bound".to_owned());
    }
    if matches!(
        termination,
        DiagnosticTermination::Crash | DiagnosticTermination::Infrastructure
    ) {
        return Err("crash and infrastructure traces are not acceptable".to_owned());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "WU0E trace is not UTF-8".to_owned())?;
    let truncated_tail = !bytes.ends_with(b"\n");
    if truncated_tail && !termination.permits_partial() {
        return Err("unterminated WU0E trace without containment".to_owned());
    }
    let (complete, tail) = if truncated_tail {
        text.rsplit_once('\n')
            .map_or(("", text), |(head, tail)| (head, tail))
    } else {
        (text.strip_suffix('\n').unwrap_or(text), "")
    };
    if truncated_tail && !(tail.starts_with(PREFIX) || PREFIX.starts_with(tail)) {
        return Err("invalid WU0E truncated tail".to_owned());
    }
    let mut events = Vec::new();
    if !complete.is_empty() {
        for line in complete.split('\n') {
            if line.is_empty() {
                return Err("blank WU0E record".to_owned());
            }
            events.push(parse_event_line(line)?);
        }
    }
    if events.is_empty() {
        return Err("WU0E trace has no complete run-start".to_owned());
    }

    let plan = expanded_phase_plan(statement_ordinals);
    let mut completed = Vec::new();
    let mut active = None;
    let mut measurement = None;
    let mut pending_semantic = None;
    let mut finished = false;
    let mut last_elapsed = None;
    for (event_index, event) in events.into_iter().enumerate() {
        let elapsed = match &event {
            DiagnosticEvent::RunStart { elapsed_us, .. }
            | DiagnosticEvent::PhaseEnter { elapsed_us, .. }
            | DiagnosticEvent::PhaseExit { elapsed_us, .. }
            | DiagnosticEvent::Measurement { elapsed_us, .. }
            | DiagnosticEvent::Semantic { elapsed_us, .. }
            | DiagnosticEvent::RunFinish { elapsed_us } => *elapsed_us,
        };
        if elapsed == u64::MAX || last_elapsed.is_some_and(|previous| elapsed < previous) {
            return Err("WU0E timestamps are not monotonic and unsaturated".to_owned());
        }
        last_elapsed = Some(elapsed);
        match event {
            DiagnosticEvent::RunStart { mode, elapsed_us } => {
                if event_index != 0 || mode != expected_mode || elapsed_us != 0 {
                    return Err("invalid WU0E run-start".to_owned());
                }
            }
            DiagnosticEvent::PhaseEnter { sequence, key, .. } => {
                if finished
                    || active.is_some()
                    || sequence != completed.len()
                    || plan.get(sequence).copied() != Some(key)
                {
                    return Err("invalid WU0E phase-enter state".to_owned());
                }
                active = Some((sequence, key));
            }
            DiagnosticEvent::PhaseExit { sequence, key, .. } => {
                if finished || active != Some((sequence, key)) || sequence != completed.len() {
                    return Err("invalid WU0E phase-exit state".to_owned());
                }
                active = None;
                completed.push(key);
            }
            DiagnosticEvent::Measurement {
                mode,
                measurement: value,
                ..
            } => {
                if finished
                    || active.is_some()
                    || completed != plan
                    || expected_mode == DiagnosticMode::Plain
                    || mode != expected_mode
                    || measurement.is_some()
                    || pending_semantic.is_some()
                {
                    return Err("invalid WU0E measurement state".to_owned());
                }
                value.validate(mode)?;
                measurement = Some(value);
            }
            DiagnosticEvent::Semantic {
                aggregate_sha256, ..
            } => {
                if finished
                    || active.is_some()
                    || completed != plan
                    || pending_semantic.is_some()
                    || (expected_mode == DiagnosticMode::Plain && measurement.is_some())
                    || (expected_mode != DiagnosticMode::Plain && measurement.is_none())
                {
                    return Err("invalid WU0E semantic state".to_owned());
                }
                pending_semantic = Some(aggregate_sha256);
            }
            DiagnosticEvent::RunFinish { .. } => {
                if finished || active.is_some() || completed != plan || pending_semantic.is_none() {
                    return Err("invalid WU0E run-finish state".to_owned());
                }
                finished = true;
            }
        }
    }
    if termination == DiagnosticTermination::Normal && (!finished || truncated_tail) {
        return Err("normal WU0E run is incomplete".to_owned());
    }
    if finished && truncated_tail {
        return Err("finished WU0E trace has a trailing fragment".to_owned());
    }
    let semantic_sha256 = finished.then_some(pending_semantic).flatten();
    Ok(ParsedDiagnosticTrace {
        mode: expected_mode,
        completed_phases: completed,
        active_phase: active.map(|(_, key)| key),
        measurement,
        semantic_sha256,
        finished,
        truncated_tail,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiagnosticValidation {
    mode: DiagnosticMode,
    termination: DiagnosticTermination,
    complete: bool,
    semantic_sha256: Option<String>,
}

impl DiagnosticValidation {
    pub(super) fn render(&self) -> String {
        format!(
            "typokat-wu0e-validation-v1 mode={} termination={} status={} semantic_sha256={}",
            self.mode.as_str(),
            self.termination.as_str(),
            if self.complete { "complete" } else { "partial" },
            self.semantic_sha256.as_deref().unwrap_or("unavailable")
        )
    }
}

pub(super) fn validate_diagnostic_trace_file(
    path: &Path,
    mode: DiagnosticMode,
    statement_ordinals: &[u32],
    termination: DiagnosticTermination,
) -> Result<DiagnosticValidation, String> {
    validate_diagnostic_trace_file_with_hook(path, mode, statement_ordinals, termination, || Ok(()))
}

#[cfg(unix)]
pub(super) fn validate_diagnostic_trace_file_with_post_read_hook_for_test(
    path: &Path,
    mode: DiagnosticMode,
    statement_ordinals: &[u32],
    termination: DiagnosticTermination,
    hook: impl FnOnce() -> io::Result<()>,
) -> Result<DiagnosticValidation, String> {
    validate_diagnostic_trace_file_with_hook(path, mode, statement_ordinals, termination, hook)
}

#[cfg(unix)]
fn validate_diagnostic_trace_file_with_hook(
    path: &Path,
    mode: DiagnosticMode,
    statement_ordinals: &[u32],
    termination: DiagnosticTermination,
    hook: impl FnOnce() -> io::Result<()>,
) -> Result<DiagnosticValidation, String> {
    use rustix::fs::{fstat, open, FileType, Mode, OFlags};
    use std::os::unix::fs::MetadataExt;

    ensure_real_absolute_parent(path)?;
    let path_before = std::fs::symlink_metadata(path)
        .map_err(|error| format!("inspect WU0E trace path: {error}"))?;
    if path_before.file_type().is_symlink() || !path_before.is_file() {
        return Err("WU0E validation path is not a regular non-symlink".to_owned());
    }
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("no-follow open WU0E trace: {error}"))?;
    let before = fstat(&descriptor).map_err(|error| format!("fstat WU0E trace: {error}"))?;
    if !FileType::from_raw_mode(before.st_mode).is_file() {
        return Err("opened WU0E trace is not a regular file".to_owned());
    }
    let expected_size =
        u64::try_from(before.st_size).map_err(|_| "WU0E trace has an invalid size".to_owned())?;
    let maximum = u64::try_from(MAX_DIAGNOSTIC_TRACE_BYTES)
        .map_err(|_| "WU0E trace bound does not fit u64".to_owned())?;
    if expected_size > maximum {
        return Err("WU0E trace exceeds its validation bound".to_owned());
    }
    if path_before.dev() != before.st_dev
        || path_before.ino() != before.st_ino
        || path_before.size() != expected_size
    {
        return Err("WU0E trace identity changed before read".to_owned());
    }

    let mut file: File = descriptor.into();
    let mut bytes = Vec::new();
    Read::read_to_end(&mut Read::take(&mut file, maximum + 1), &mut bytes)
        .map_err(|error| format!("read WU0E trace: {error}"))?;
    hook().map_err(|error| format!("WU0E post-read hook failed: {error}"))?;
    let after = fstat(&file).map_err(|error| format!("re-fstat WU0E trace: {error}"))?;
    let path_after = std::fs::symlink_metadata(path)
        .map_err(|error| format!("re-inspect WU0E trace path: {error}"))?;
    let actual_size = u64::try_from(bytes.len())
        .map_err(|_| "WU0E trace read size does not fit u64".to_owned())?;
    if actual_size > maximum
        || actual_size != expected_size
        || !FileType::from_raw_mode(after.st_mode).is_file()
        || before.st_dev != after.st_dev
        || before.st_ino != after.st_ino
        || before.st_size != after.st_size
        || path_after.file_type().is_symlink()
        || !path_after.is_file()
        || path_after.dev() != after.st_dev
        || path_after.ino() != after.st_ino
        || path_after.size() != actual_size
    {
        return Err("WU0E trace changed during bounded read".to_owned());
    }
    let parsed = parse_diagnostic_trace(&bytes, mode, statement_ordinals, termination)?;
    Ok(DiagnosticValidation {
        mode,
        termination,
        complete: parsed.finished(),
        semantic_sha256: parsed.semantic_sha256().map(str::to_owned),
    })
}

#[cfg(not(unix))]
fn validate_diagnostic_trace_file_with_hook(
    _path: &Path,
    _mode: DiagnosticMode,
    _statement_ordinals: &[u32],
    _termination: DiagnosticTermination,
    _hook: impl FnOnce() -> io::Result<()>,
) -> Result<DiagnosticValidation, String> {
    Err("WU0E trace validation is unsupported on non-Unix hosts".to_owned())
}

pub(super) fn validate_completed_semantic_parity(
    traces: [&ParsedDiagnosticTrace; 3],
) -> Result<&str, String> {
    let expected_modes = [
        DiagnosticMode::Plain,
        DiagnosticMode::MeasuredOff,
        DiagnosticMode::CandidateB,
    ];
    let mut semantic = None;
    for (trace, expected_mode) in traces.into_iter().zip(expected_modes) {
        if trace.mode() != expected_mode || !trace.finished() {
            return Err("WU0E parity requires three completed modes in order".to_owned());
        }
        let current = trace
            .semantic_sha256()
            .ok_or_else(|| "completed WU0E trace lacks semantics".to_owned())?;
        if semantic.is_some_and(|value| value != current) {
            return Err("WU0E semantic digests differ".to_owned());
        }
        semantic = Some(current);
    }
    semantic.ok_or_else(|| "WU0E semantic parity has no digest".to_owned())
}

fn measurement_from_active_scopes(mode: DiagnosticMode) -> Result<DiagnosticMeasurement, String> {
    if mode == DiagnosticMode::Plain {
        return Err("plain WU0E mode has no measurement".to_owned());
    }
    let cycle = cycle_tainted_application_cache_measure()
        .ok_or_else(|| "WU0E cycle measurement is inactive".to_owned())?;
    let clean = eager_application_cache_measure()
        .ok_or_else(|| "WU0E eager measurement is inactive".to_owned())?;
    let measurement = DiagnosticMeasurement {
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
        clean_cache_hits: clean.hits,
        clean_cache_misses: clean.misses,
        clean_cache_entries: clean.insertions,
        saturated: cycle.saturated,
    };
    measurement.validate(mode)?;
    Ok(measurement)
}

fn run_with_measurement(
    mode: DiagnosticMode,
    sources: &[InjectedLibrarySource<'_>],
    boundary: &mut DiagnosticBoundaryAdapter,
) -> Result<(InjectedProfileRun, String, Option<DiagnosticMeasurement>), String> {
    let _eager_scope = (mode != DiagnosticMode::Plain).then(start_eager_application_cache_measure);
    let _baseline_scope = (mode == DiagnosticMode::MeasuredOff)
        .then(start_cycle_tainted_application_cache_baseline_measure);
    let _candidate_scope =
        (mode == DiagnosticMode::CandidateB).then(start_cycle_tainted_application_cache_measure);
    let (run, semantic_sha256) =
        super::wu0b_library::run_injected_profile_observed(sources, boundary)
            .map_err(|error| format!("WU0E profile failed: {error:?}"))?;
    let measurement = if mode == DiagnosticMode::Plain {
        None
    } else {
        Some(measurement_from_active_scopes(mode)?)
    };
    Ok((run, semantic_sha256, measurement))
}

fn finish_diagnostic_trace(
    mode: DiagnosticMode,
    boundary: &mut DiagnosticBoundaryAdapter,
    semantic_sha256: String,
    measurement: Option<DiagnosticMeasurement>,
) -> Result<(), String> {
    if let Some(measurement) = measurement {
        let elapsed_us = boundary.elapsed()?;
        boundary.emit(DiagnosticEvent::Measurement {
            mode,
            measurement,
            elapsed_us,
        })?;
    }
    let elapsed_us = boundary.elapsed()?;
    boundary.emit(DiagnosticEvent::Semantic {
        aggregate_sha256: semantic_sha256,
        elapsed_us,
    })?;
    let elapsed_us = boundary.elapsed()?;
    boundary.emit(DiagnosticEvent::RunFinish { elapsed_us })
}

pub(super) fn run_observed_profile_for_test(
    mode: DiagnosticMode,
    sources: &[InjectedLibrarySource<'_>],
    path: &Path,
) -> Result<ParsedDiagnosticTrace, String> {
    let sink = DiagnosticTraceSink::create(path)?;
    let mut boundary = DiagnosticBoundaryAdapter::new(mode, ObserverClock::monotonic(), sink)?;
    boundary
        .enter_phase(DiagnosticPhase::ProfileLoad, None)
        .map_err(|error| format!("{error:?}"))?;
    boundary
        .exit_phase(DiagnosticPhase::ProfileLoad, None)
        .map_err(|error| format!("{error:?}"))?;
    let (_, semantic_sha256, measurement) = run_with_measurement(mode, sources, &mut boundary)?;
    finish_diagnostic_trace(mode, &mut boundary, semantic_sha256, measurement)?;
    drop(boundary);
    let ordinals = sources
        .iter()
        .map(|source| {
            u32::try_from(source.file_ordinal.index())
                .map_err(|_| "library file ordinal does not fit u32".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_diagnostic_trace_file(path, mode, &ordinals, DiagnosticTermination::Normal)?;
    let bytes = std::fs::read(path).map_err(|error| format!("read WU0E test trace: {error}"))?;
    parse_diagnostic_trace(&bytes, mode, &ordinals, DiagnosticTermination::Normal)
}

pub(super) fn strict_profile_statement_ordinals_for_test() -> Result<Vec<u32>, String> {
    let profile = super::wu0b_profile::load_strict_profile()?;
    profile
        .injected_sources()
        .into_iter()
        .map(|source| {
            u32::try_from(source.file_ordinal.index())
                .map_err(|_| "strict profile file ordinal does not fit u32".to_owned())
        })
        .collect()
}

fn relevant_environment(prefix: &str) -> Result<Vec<(String, String)>, String> {
    let mut entries = Vec::new();
    for (key, value) in std::env::vars_os() {
        let key = key
            .into_string()
            .map_err(|_| "non-UTF-8 environment key".to_owned())?;
        if !key.starts_with(prefix) {
            continue;
        }
        let value = value
            .into_string()
            .map_err(|_| "non-UTF-8 WU0E environment value".to_owned())?;
        entries.push((key, value));
    }
    Ok(entries)
}

fn resolve_workload_environment() -> Result<DiagnosticEnvironment, String> {
    let entries = relevant_environment("TYPOKAT_WU0E_")?;
    resolve_diagnostic_environment_for_test(
        entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
}

fn resolve_validation_environment() -> Result<DiagnosticValidationEnvironment, String> {
    let entries = relevant_environment("TYPOKAT_WU0E_VALIDATE_")?;
    resolve_diagnostic_validation_environment_for_test(
        entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
}

#[test]
#[ignore = "WU0E diagnostic primary probe"]
fn wu0e_primary_probe_once() {
    let environment = resolve_workload_environment().expect("strict WU0E workload environment");
    let sink = DiagnosticTraceSink::create(&environment.trace_path)
        .expect("exclusive WU0E diagnostic trace");
    let mut boundary =
        DiagnosticBoundaryAdapter::new(environment.mode, ObserverClock::monotonic(), sink)
            .expect("WU0E boundary adapter");
    let profile_enter = boundary.enter_phase(DiagnosticPhase::ProfileLoad, None);
    profile_enter.expect("WU0E profile-load enter");
    let profile = super::wu0b_profile::load_strict_profile().expect("strict WU0B profile");
    let injected_sources = profile.injected_sources();
    let profile_exit = boundary.exit_phase(DiagnosticPhase::ProfileLoad, None);
    profile_exit.expect("WU0E profile-load exit");

    let _eager_scope = if environment.mode != DiagnosticMode::Plain {
        Some(start_eager_application_cache_measure())
    } else {
        None
    };
    let _baseline_scope = if environment.mode == DiagnosticMode::MeasuredOff {
        Some(start_cycle_tainted_application_cache_baseline_measure())
    } else {
        None
    };
    let _candidate_scope = if environment.mode == DiagnosticMode::CandidateB {
        Some(start_cycle_tainted_application_cache_measure())
    } else {
        None
    };
    let (_, semantic_sha256) =
        super::wu0b_library::run_injected_profile_observed(&injected_sources, &mut boundary)
            .expect("exact WU0B profile execution");
    let measurement = if environment.mode == DiagnosticMode::Plain {
        None
    } else {
        Some(
            measurement_from_active_scopes(environment.mode)
                .expect("active WU0E measurement scopes"),
        )
    };
    finish_diagnostic_trace(
        environment.mode,
        &mut boundary,
        semantic_sha256,
        measurement,
    )
    .expect("complete WU0E diagnostic trace");
}

#[test]
#[ignore = "WU0E diagnostic trace validator"]
fn wu0e_validate_trace_once() {
    let environment = resolve_validation_environment().expect("strict WU0E validation environment");
    let profile = super::wu0b_profile::load_strict_profile().expect("strict WU0B profile");
    let injected_sources = profile.injected_sources();
    let statement_ordinals = injected_sources
        .iter()
        .map(|source| {
            u32::try_from(source.file_ordinal.index())
                .expect("strict profile file ordinal fits u32")
        })
        .collect::<Vec<_>>();
    let validation = validate_diagnostic_trace_file(
        &environment.trace_path,
        environment.mode,
        &statement_ordinals,
        environment.termination,
    )
    .expect("valid bounded WU0E diagnostic trace");
    println!("{}", validation.render());
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RunnerSelfTestCase {
    SetsidContainment,
    PreSetsidDirectKill,
    ZombieLeaderReservation,
    LeaderExitDescendantKill,
    SummedLiveGroupRss,
    RssSamplingInterval,
    StdoutFlood,
    StderrFlood,
    TraceFlood,
    BoundedDrain,
    BoundedPostRead,
    RssSamplingFailure,
    RssArithmeticOverflow,
    BinarySwap,
    EnvironmentScrub,
    WorkloadAllowlist,
    ValidatorAllowlist,
    ValidatorAfterEachWorkload,
    ExactPrimaryProbe,
    NoAlternateCompiler,
    SameBinaryValidator,
    PrePostBinaryDigest,
    OneFrozenBinary,
    WarmInventoryBeforeEveryLaunch,
    SameBinaryHostProfileInventory,
    CrossModeIdentityParity,
}

const RUNNER_SELF_TEST_CASES: [RunnerSelfTestCase; 26] = [
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
];

#[derive(Copy, Clone, Debug)]
pub(super) struct RunnerSelfTestCases;

impl RunnerSelfTestCases {
    pub(super) const fn as_slice(self) -> &'static [RunnerSelfTestCase] {
        &RUNNER_SELF_TEST_CASES
    }

    fn iter(self) -> std::slice::Iter<'static, RunnerSelfTestCase> {
        RUNNER_SELF_TEST_CASES.iter()
    }

    pub(super) fn len(self) -> usize {
        RUNNER_SELF_TEST_CASES.len()
    }
}

impl IntoIterator for RunnerSelfTestCases {
    type Item = &'static RunnerSelfTestCase;
    type IntoIter = std::slice::Iter<'static, RunnerSelfTestCase>;

    fn into_iter(self) -> Self::IntoIter {
        RUNNER_SELF_TEST_CASES.iter()
    }
}

impl PartialEq<[RunnerSelfTestCase; 26]> for RunnerSelfTestCases {
    fn eq(&self, other: &[RunnerSelfTestCase; 26]) -> bool {
        RUNNER_SELF_TEST_CASES == *other
    }
}

impl RunnerSelfTestCase {
    pub(super) const ALL: RunnerSelfTestCases = RunnerSelfTestCases;

    const fn as_str(self) -> &'static str {
        match self {
            Self::SetsidContainment => "setsid-containment",
            Self::PreSetsidDirectKill => "pre-setsid-direct-kill",
            Self::ZombieLeaderReservation => "zombie-leader-reservation",
            Self::LeaderExitDescendantKill => "leader-exit-descendant-kill",
            Self::SummedLiveGroupRss => "summed-live-group-rss",
            Self::RssSamplingInterval => "rss-sampling-interval",
            Self::StdoutFlood => "stdout-flood",
            Self::StderrFlood => "stderr-flood",
            Self::TraceFlood => "trace-flood",
            Self::BoundedDrain => "bounded-drain",
            Self::BoundedPostRead => "bounded-post-read",
            Self::RssSamplingFailure => "rss-sampling-failure",
            Self::RssArithmeticOverflow => "rss-arithmetic-overflow",
            Self::BinarySwap => "binary-swap",
            Self::EnvironmentScrub => "environment-scrub",
            Self::WorkloadAllowlist => "workload-allowlist",
            Self::ValidatorAllowlist => "validator-allowlist",
            Self::ValidatorAfterEachWorkload => "validator-after-each-workload",
            Self::ExactPrimaryProbe => "exact-primary-probe",
            Self::NoAlternateCompiler => "no-alternate-compiler",
            Self::SameBinaryValidator => "same-binary-validator",
            Self::PrePostBinaryDigest => "pre-post-binary-digest",
            Self::OneFrozenBinary => "one-frozen-binary",
            Self::WarmInventoryBeforeEveryLaunch => "warm-inventory-before-every-launch",
            Self::SameBinaryHostProfileInventory => "same-binary-host-profile-inventory",
            Self::CrossModeIdentityParity => "cross-mode-identity-parity",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|candidate| candidate.as_str() == value)
    }

    fn expected_fields(self) -> &'static [&'static str] {
        match self {
            Self::SetsidContainment => &["group_isolated"],
            Self::PreSetsidDirectKill => &["direct_kill_attempted"],
            Self::ZombieLeaderReservation => &["pgid_reserved"],
            Self::LeaderExitDescendantKill => &["descendants_reaped"],
            Self::SummedLiveGroupRss => &["summed_rss_bytes", "largest_member_rss_bytes"],
            Self::RssSamplingInterval => &["max_interval_us"],
            Self::StdoutFlood | Self::StderrFlood | Self::TraceFlood => {
                &["observed_bytes", "whole_group_terminated"]
            }
            Self::BoundedDrain => &["drain_expired"],
            Self::BoundedPostRead => &["max_read_bytes"],
            Self::RssSamplingFailure => &["termination", "rss_assumed_zero"],
            Self::RssArithmeticOverflow => &["overflow_detected", "termination"],
            Self::BinarySwap => &["replacement_rejected"],
            Self::EnvironmentScrub => &["removed_variable_count"],
            Self::WorkloadAllowlist | Self::ValidatorAllowlist => &["exact_variable_count"],
            Self::ValidatorAfterEachWorkload => &[
                "workload_count",
                "validator_count",
                "validator_immediately_after_workload",
            ],
            Self::ExactPrimaryProbe => &["compiler_command"],
            Self::NoAlternateCompiler => &["alternate_exec_observed"],
            Self::SameBinaryValidator => &["binary_identity_count"],
            Self::PrePostBinaryDigest => &["verified_launches"],
            Self::OneFrozenBinary => &["build_count"],
            Self::WarmInventoryBeforeEveryLaunch => &["warm_count"],
            Self::SameBinaryHostProfileInventory => &["identity_tuple_count"],
            Self::CrossModeIdentityParity => &["parity"],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RunnerSelfTestObservation {
    case: RunnerSelfTestCase,
    fields: BTreeMap<String, String>,
}

impl RunnerSelfTestObservation {
    pub(super) const fn case(&self) -> RunnerSelfTestCase {
        self.case
    }

    pub(super) fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RunnerSelfTestReport {
    observations: Vec<RunnerSelfTestObservation>,
    deadline_us: u64,
    max_process_group_rss_bytes: u64,
    max_stdout_bytes: u64,
    max_stderr_bytes: u64,
    max_trace_bytes: u64,
    max_observed_rss_sample_interval_us: u64,
}

impl RunnerSelfTestReport {
    pub(super) fn passed_cases(&self) -> Vec<RunnerSelfTestCase> {
        self.observations.iter().map(|value| value.case).collect()
    }

    pub(super) const fn deadline_us(&self) -> u64 {
        self.deadline_us
    }

    pub(super) const fn max_process_group_rss_bytes(&self) -> u64 {
        self.max_process_group_rss_bytes
    }

    pub(super) const fn max_stdout_bytes(&self) -> u64 {
        self.max_stdout_bytes
    }

    pub(super) const fn max_stderr_bytes(&self) -> u64 {
        self.max_stderr_bytes
    }

    pub(super) const fn max_trace_bytes(&self) -> u64 {
        self.max_trace_bytes
    }

    pub(super) const fn max_observed_rss_sample_interval_us(&self) -> u64 {
        self.max_observed_rss_sample_interval_us
    }

    pub(super) fn observation(
        &self,
        case: RunnerSelfTestCase,
    ) -> Option<&RunnerSelfTestObservation> {
        self.observations.iter().find(|value| value.case == case)
    }

    pub(super) fn observation_bool(&self, case: RunnerSelfTestCase, key: &str) -> Option<bool> {
        match self.observation(case)?.fields.get(key)?.as_str() {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        }
    }

    pub(super) fn observation_u64(&self, case: RunnerSelfTestCase, key: &str) -> Option<u64> {
        canonical_u64(self.observation(case)?.fields.get(key)?).ok()
    }

    pub(super) fn observation_text(&self, case: RunnerSelfTestCase, key: &str) -> Option<&str> {
        self.observation(case)?.fields.get(key).map(String::as_str)
    }
}

pub(super) fn parse_runner_self_test_report(bytes: &[u8]) -> Result<RunnerSelfTestReport, String> {
    if !bytes.is_ascii() || bytes.contains(&b'\r') || !bytes.ends_with(b"\n") {
        return Err("runner self-test report must be canonical ASCII LF".to_owned());
    }
    let lines = std::str::from_utf8(bytes)
        .map_err(|_| "runner self-test report is not UTF-8".to_owned())?
        .lines()
        .collect::<Vec<_>>();
    if lines.len() != RunnerSelfTestCase::ALL.len() + 1 {
        return Err("runner self-test report has the wrong line count".to_owned());
    }
    let mut observations = Vec::with_capacity(RunnerSelfTestCase::ALL.len());
    let mut seen = BTreeSet::new();
    for (line, expected) in lines[..RunnerSelfTestCase::ALL.len()]
        .iter()
        .copied()
        .zip(RunnerSelfTestCase::ALL)
    {
        let mut tokens = line.split(' ');
        if tokens.next() != Some("typokat-wu0e-self-test-observation-v1") {
            return Err("invalid WU0E self-test observation prefix".to_owned());
        }
        let case = RunnerSelfTestCase::parse(field(
            tokens
                .next()
                .ok_or_else(|| "missing self-test case".to_owned())?,
            "case=",
        )?)
        .ok_or_else(|| "unknown self-test case".to_owned())?;
        if case != *expected || !seen.insert(case) {
            return Err("self-test case order or uniqueness drifted".to_owned());
        }
        let expected_fields = case.expected_fields();
        let mut fields = BTreeMap::new();
        for (token, expected_key) in tokens.zip(expected_fields.iter().copied()) {
            let prefix = format!("{expected_key}=");
            let value = field(token, &prefix)?;
            if value.is_empty()
                || fields
                    .insert(expected_key.to_owned(), value.to_owned())
                    .is_some()
            {
                return Err("invalid self-test observation field".to_owned());
            }
        }
        if fields.len() != expected_fields.len() {
            return Err("self-test observation field inventory drifted".to_owned());
        }
        observations.push(RunnerSelfTestObservation { case, fields });
    }

    let summary = lines[RunnerSelfTestCase::ALL.len()]
        .split(' ')
        .collect::<Vec<_>>();
    if summary.len() != 8 || summary[0] != "typokat-wu0e-self-test-v1" || summary[1] != "result=ok"
    {
        return Err("invalid WU0E self-test summary".to_owned());
    }
    Ok(RunnerSelfTestReport {
        observations,
        deadline_us: canonical_u64(field(summary[2], "deadline_us=")?)?,
        max_process_group_rss_bytes: canonical_u64(field(
            summary[3],
            "max_process_group_rss_bytes=",
        )?)?,
        max_stdout_bytes: canonical_u64(field(summary[4], "max_stdout_bytes=")?)?,
        max_stderr_bytes: canonical_u64(field(summary[5], "max_stderr_bytes=")?)?,
        max_trace_bytes: canonical_u64(field(summary[6], "max_trace_bytes=")?)?,
        max_observed_rss_sample_interval_us: canonical_u64(field(
            summary[7],
            "max_observed_rss_sample_interval_us=",
        )?)?,
    })
}

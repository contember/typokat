use super::*;

#[derive(Copy, Clone, Debug)]
pub struct InjectedLibrarySource<'source> {
    pub file_ordinal: LibraryFileOrdinal,
    pub name: &'source str,
    pub source: &'source str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InjectedProfileError {
    EmptyProfile,
    EmptyName {
        file_ordinal: LibraryFileOrdinal,
    },
    DuplicateName(String),
    DuplicateFileOrdinal(LibraryFileOrdinal),
    SourceKeyOverflow,
    Parse {
        file_ordinal: LibraryFileOrdinal,
        messages: Vec<String>,
    },
    Binder(String),
    Reservation(String),
    LibraryIdentitySelectionPending,
    Reporting(LibraryEventLedgerError),
    ReplayIndex(ReplayIndexGenerationError),
    ReplayIndexAdmission(ReplayIndexAdmissionError),
    SourceReexportEvidence(&'static str),
    CanonicalProjection(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryPhaseCounts {
    pub parse_units: usize,
    pub bind_units: usize,
    pub reserved_records: usize,
    pub filled_records: usize,
    pub publication_validations: usize,
    pub statement_check_units: usize,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryPhaseTimings {
    pub parse: Duration,
    pub bind: Duration,
    pub reserve_fill: Duration,
    pub publication_validation: Duration,
    pub statement_check: Duration,
}

#[cfg(any(test, feature = "test-utils"))]
impl LibraryPhaseTimings {
    pub(super) fn measured_total(&self) -> Duration {
        self.parse
            + self.bind
            + self.reserve_fill
            + self.publication_validation
            + self.statement_check
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeProbe {
    pub identity: TypeGroupId,
    pub declaration_identities: Vec<(LibraryFileOrdinal, TypeGroupId)>,
    pub declaration_count: usize,
    pub member_names: Vec<String>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureProbe {
    pub parameter_types: Vec<String>,
    pub return_type: String,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableMemberProbe {
    pub name: String,
    pub identity: ValueStorageId,
    pub source: ExactUnit,
    pub reservation_source: ExactUnit,
    pub source_start: u32,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueProbe {
    pub identity: ValueStorageId,
    pub(super) visible_type: Option<TypeId>,
    pub participant_identities: Vec<(LibraryFileOrdinal, ValueStorageId)>,
    pub declaration_count: usize,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
    pub member_names: Vec<String>,
    pub callable_members: Vec<CallableMemberProbe>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedSpanForTest {
    pub start: u32,
    pub end: u32,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedDiagnosticForTest {
    pub span: NormalizedSpanForTest,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub elaboration: Vec<String>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedIncompleteForTest {
    pub span: NormalizedSpanForTest,
    pub id: String,
    pub context: String,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NormalizedCheckerRecordForTest {
    Diagnostic(NormalizedDiagnosticForTest),
    Incomplete(NormalizedIncompleteForTest),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedUserEventRecordForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub module_ordinal: ModuleOrdinal,
    pub source_start: u32,
    pub event_ordinal: usize,
    pub record_ordinal: usize,
    pub record: NormalizedCheckerRecordForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedCheckResultForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub module_ordinal: ModuleOrdinal,
    pub unit_slot: UnitSlot,
    pub diagnostics: Vec<NormalizedDiagnosticForTest>,
    pub incomplete: Vec<NormalizedIncompleteForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedTypeProbeForTest {
    pub name: String,
    pub declaration_count: usize,
    pub member_names: Vec<String>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedModuleTypeProbeForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub probe: NormalizedTypeProbeForTest,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedCallableMemberProbeForTest {
    pub name: String,
    pub source_ordinal: LibraryFileOrdinal,
    pub reservation_source_ordinal: LibraryFileOrdinal,
    pub source_start: u32,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedValueProbeForTest {
    pub name: String,
    pub declaration_count: usize,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
    pub member_names: Vec<String>,
    pub callable_members: Vec<NormalizedCallableMemberProbeForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedSemanticProbesForTest {
    pub global_types: Vec<NormalizedTypeProbeForTest>,
    pub module_types: Vec<NormalizedModuleTypeProbeForTest>,
    pub global_values: Vec<NormalizedValueProbeForTest>,
}

#[derive(Debug)]
pub struct InjectedProfileRun {
    pub phase_counts: LibraryPhaseCounts,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_store_rows: usize,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_published_type_rows: usize,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_replacement_type_rows: usize,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_type_param_id: u32,
    #[cfg(any(test, feature = "test-utils"))]
    pub initial_class_id: u32,
    #[cfg(any(test, feature = "test-utils"))]
    pub phase_timings: LibraryPhaseTimings,
    #[cfg(any(test, feature = "test-utils"))]
    pub reserved_file_ordinals: Vec<LibraryFileOrdinal>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) reporting_receipts: Vec<LibraryReportingReceipt>,
    /// The library's own records — empty unless the caller asked for
    /// [`LibraryRecordRetention::Collect`]. Nothing downstream of a compile retains them
    /// (ADR-0018).
    pub library_records: Vec<(LibraryEventKey, CheckerRecord)>,
    pub(super) user_results: Vec<super::CheckResult>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) ordered_user_records: Vec<(LibraryFileOrdinal, CheckerRecord)>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) ordered_user_event_records: Vec<(
        LibraryFileOrdinal,
        super::super::events::EventKey,
        CheckerRecord,
    )>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) user_result_source_ordinals: BTreeMap<ModuleOrdinal, LibraryFileOrdinal>,
    #[cfg(any(test, feature = "test-utils"))]
    pub pass_source_units: Vec<ExactUnit>,
    #[cfg(any(test, feature = "test-utils"))]
    pub lexical_source_units: Vec<ExactUnit>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) global_types: BTreeMap<String, TypeProbe>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) module_types: BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) global_values: BTreeMap<String, ValueProbe>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) semantic_identities: LibrarySemanticIdentities,
}

/// The canonical byte projection of the library's own records.
///
/// This is artifact identity, not semantic output: the records themselves are what
/// ADR-0011 requires be preserved exactly, and they reach the base untouched. Nothing in a
/// compile reads these blobs, so the suite — not every process — projects them (ADR-0017).
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalLibraryEvidence {
    pub diagnostics: Vec<u8>,
    pub incompletes: Vec<u8>,
    pub ledger: Vec<u8>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompleteSourceRouteWorkForTest {
    pub profile_loads: u64,
    pub library_parse_units: u64,
    pub library_bind_units: u64,
    pub user_bind_units: u64,
    pub semantic_publications: u64,
    pub replay_trace_constructions: u64,
    pub replay_plan_constructions: u64,
    pub frozen_base_seals: u64,
    pub library_source_reparses: u64,
    pub library_evidence: Option<CanonicalLibraryEvidence>,
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static COMPLETE_SOURCE_ROUTE_WORK: RefCell<CompleteSourceRouteWorkForTest> =
        RefCell::new(CompleteSourceRouteWorkForTest::default());
    static COMPLETE_SOURCE_EVIDENCE_GENERATION: Cell<u64> = const { Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_complete_source_route_work_for_test(
    record: impl FnOnce(&mut CompleteSourceRouteWorkForTest),
) {
    COMPLETE_SOURCE_ROUTE_WORK.with(|work| record(&mut work.borrow_mut()));
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_complete_source_profile_load_for_test() {
    record_complete_source_route_work_for_test(|work| {
        work.profile_loads = work.profile_loads.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub fn record_complete_source_auxiliary_parse_work_for_test(
    parser_invocations: u64,
    source_reparses: u64,
) {
    record_complete_source_route_work_for_test(|work| {
        work.library_parse_units = work.library_parse_units.saturating_add(parser_invocations);
        work.library_source_reparses = work.library_source_reparses.saturating_add(source_reparses);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_complete_source_bind_for_test(library_count: usize, user_count: usize) {
    record_complete_source_route_work_for_test(|work| {
        work.library_bind_units = work
            .library_bind_units
            .saturating_add(u64::try_from(library_count).unwrap_or(u64::MAX));
        work.user_bind_units = work
            .user_bind_units
            .saturating_add(u64::try_from(user_count).unwrap_or(u64::MAX));
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn record_complete_source_publication_entry_for_test() {
    record_complete_source_route_work_for_test(|work| {
        work.semantic_publications = work.semantic_publications.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn record_replay_trace_construction_for_test() {
    record_complete_source_route_work_for_test(|work| {
        work.replay_trace_constructions = work.replay_trace_constructions.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn record_replay_plan_construction_for_test() {
    record_complete_source_route_work_for_test(|work| {
        work.replay_plan_constructions = work.replay_plan_constructions.saturating_add(1);
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn record_complete_source_evidence_for_test(evidence: CanonicalLibraryEvidence) {
    record_complete_source_route_work_for_test(|work| work.library_evidence = Some(evidence));
    COMPLETE_SOURCE_EVIDENCE_GENERATION
        .set(COMPLETE_SOURCE_EVIDENCE_GENERATION.get().saturating_add(1));
}

#[cfg(not(any(test, feature = "test-utils")))]
pub fn record_complete_source_profile_load_for_test() {}

#[cfg(any(test, feature = "test-utils"))]
pub fn complete_source_route_thread_receipt_for_test() -> CompleteSourceRouteWorkForTest {
    COMPLETE_SOURCE_ROUTE_WORK.with(|work| work.borrow().clone())
}

#[cfg(any(test, feature = "test-utils"))]
pub fn merge_complete_source_route_thread_receipt_for_test(
    receipt: CompleteSourceRouteWorkForTest,
) {
    record_complete_source_route_work_for_test(|work| {
        work.profile_loads = work.profile_loads.saturating_add(receipt.profile_loads);
        work.library_parse_units = work
            .library_parse_units
            .saturating_add(receipt.library_parse_units);
        work.library_bind_units = work
            .library_bind_units
            .saturating_add(receipt.library_bind_units);
        work.user_bind_units = work.user_bind_units.saturating_add(receipt.user_bind_units);
        work.semantic_publications = work
            .semantic_publications
            .saturating_add(receipt.semantic_publications);
        work.replay_trace_constructions = work
            .replay_trace_constructions
            .saturating_add(receipt.replay_trace_constructions);
        work.replay_plan_constructions = work
            .replay_plan_constructions
            .saturating_add(receipt.replay_plan_constructions);
        work.frozen_base_seals = work
            .frozen_base_seals
            .saturating_add(receipt.frozen_base_seals);
        work.library_source_reparses = work
            .library_source_reparses
            .saturating_add(receipt.library_source_reparses);
        if receipt.library_evidence.is_some() {
            work.library_evidence = receipt.library_evidence;
            COMPLETE_SOURCE_EVIDENCE_GENERATION
                .set(COMPLETE_SOURCE_EVIDENCE_GENERATION.get().saturating_add(1));
        }
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub struct CompleteSourceRouteWorkScopeForTest {
    start: CompleteSourceRouteWorkForTest,
    evidence_generation: u64,
}

#[cfg(any(test, feature = "test-utils"))]
impl CompleteSourceRouteWorkScopeForTest {
    pub fn start() -> Self {
        Self {
            start: complete_source_route_thread_receipt_for_test(),
            evidence_generation: COMPLETE_SOURCE_EVIDENCE_GENERATION.get(),
        }
    }

    pub fn finish(self) -> CompleteSourceRouteWorkForTest {
        let end = complete_source_route_thread_receipt_for_test();
        let evidence_changed =
            COMPLETE_SOURCE_EVIDENCE_GENERATION.get() != self.evidence_generation;
        CompleteSourceRouteWorkForTest {
            profile_loads: end.profile_loads.saturating_sub(self.start.profile_loads),
            library_parse_units: end
                .library_parse_units
                .saturating_sub(self.start.library_parse_units),
            library_bind_units: end
                .library_bind_units
                .saturating_sub(self.start.library_bind_units),
            user_bind_units: end
                .user_bind_units
                .saturating_sub(self.start.user_bind_units),
            semantic_publications: end
                .semantic_publications
                .saturating_sub(self.start.semantic_publications),
            replay_trace_constructions: end
                .replay_trace_constructions
                .saturating_sub(self.start.replay_trace_constructions),
            replay_plan_constructions: end
                .replay_plan_constructions
                .saturating_sub(self.start.replay_plan_constructions),
            frozen_base_seals: end
                .frozen_base_seals
                .saturating_sub(self.start.frozen_base_seals),
            library_source_reparses: end
                .library_source_reparses
                .saturating_sub(self.start.library_source_reparses),
            library_evidence: evidence_changed.then_some(end.library_evidence).flatten(),
        }
    }
}

impl InjectedProfileRun {
    #[cfg(any(test, feature = "test-utils"))]
    pub fn into_complete_source_user_results_for_test(self) -> Vec<super::CheckResult> {
        self.user_results
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn semantic_identities(&self) -> &LibrarySemanticIdentities {
        &self.semantic_identities
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn global_type_probe(&self, name: &str) -> Option<&TypeProbe> {
        self.global_types.get(name)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn module_type_probe(
        &self,
        file_ordinal: LibraryFileOrdinal,
        name: &str,
    ) -> Option<&TypeProbe> {
        self.module_types.get(&(file_ordinal, name.to_owned()))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn global_value_probe(&self, name: &str) -> Option<&ValueProbe> {
        self.global_values.get(name)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn normalized_user_event_store_ledger_for_test(
        &self,
    ) -> Vec<NormalizedUserEventRecordForTest> {
        self.ordered_user_event_records
            .iter()
            .map(
                |(source_ordinal, key, record)| NormalizedUserEventRecordForTest {
                    source_ordinal: *source_ordinal,
                    module_ordinal: key.module_ordinal,
                    source_start: key.source_start,
                    event_ordinal: key.event_ordinal,
                    record_ordinal: key.record_ordinal,
                    record: normalize_checker_record(record),
                },
            )
            .collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn normalized_user_check_results_for_test(
        &self,
    ) -> Result<Vec<NormalizedCheckResultForTest>, InjectedProfileError> {
        self.user_results
            .iter()
            .map(|result| {
                let source_ordinal = self
                    .user_result_source_ordinals
                    .get(&result.module_ordinal)
                    .copied()
                    .ok_or_else(|| {
                        InjectedProfileError::CanonicalProjection(format!(
                            "complete-source result omits source ordinal for user module {}",
                            result.module_ordinal.index(),
                        ))
                    })?;
                Ok(NormalizedCheckResultForTest {
                    source_ordinal,
                    module_ordinal: result.module_ordinal,
                    unit_slot: result.unit_slot,
                    diagnostics: result
                        .diagnostics
                        .iter()
                        .map(normalize_diagnostic)
                        .collect(),
                    incomplete: result.incomplete.iter().map(normalize_incomplete).collect(),
                })
            })
            .collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn normalized_raw_semantic_probes_for_test(
        &self,
    ) -> Result<NormalizedSemanticProbesForTest, InjectedProfileError> {
        let global_types = self
            .global_types
            .iter()
            .map(|(name, probe)| normalize_type_probe(name, probe))
            .collect();
        let module_types = self
            .module_types
            .iter()
            .map(
                |((source_ordinal, name), probe)| NormalizedModuleTypeProbeForTest {
                    source_ordinal: *source_ordinal,
                    probe: normalize_type_probe(name, probe),
                },
            )
            .collect();
        let global_values = self
            .global_values
            .iter()
            .map(|(name, probe)| {
                normalize_value_probe(name, probe, &self.user_result_source_ordinals)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(NormalizedSemanticProbesForTest {
            global_types,
            module_types,
            global_values,
        })
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn normalize_span(span: Span) -> NormalizedSpanForTest {
    NormalizedSpanForTest {
        start: span.start,
        end: span.end,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn normalize_diagnostic(
    diagnostic: &crate::diagnostics::Diagnostic,
) -> NormalizedDiagnosticForTest {
    NormalizedDiagnosticForTest {
        span: normalize_span(diagnostic.span),
        code: diagnostic.code.as_str().to_owned(),
        severity: match diagnostic.severity {
            crate::diagnostics::Severity::Error => "error",
            crate::diagnostics::Severity::Warning => "warning",
        }
        .to_owned(),
        message: diagnostic.message.clone(),
        elaboration: diagnostic.elaboration().to_vec(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn normalize_incomplete(
    incomplete: &crate::diagnostics::IncompleteSurface,
) -> NormalizedIncompleteForTest {
    NormalizedIncompleteForTest {
        span: normalize_span(incomplete.span),
        id: incomplete.id.clone(),
        context: incomplete.context.clone(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn normalize_checker_record(record: &CheckerRecord) -> NormalizedCheckerRecordForTest {
    match record {
        CheckerRecord::Diagnostic(diagnostic) => {
            NormalizedCheckerRecordForTest::Diagnostic(normalize_diagnostic(diagnostic))
        }
        CheckerRecord::Incomplete(incomplete) => {
            NormalizedCheckerRecordForTest::Incomplete(normalize_incomplete(incomplete))
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn normalize_type_probe(name: &str, probe: &TypeProbe) -> NormalizedTypeProbeForTest {
    NormalizedTypeProbeForTest {
        name: name.to_owned(),
        declaration_count: probe.declaration_count,
        member_names: probe.member_names.clone(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn normalized_library_source_ordinal(
    source: ExactUnit,
    user_source_ordinals: &BTreeMap<ModuleOrdinal, LibraryFileOrdinal>,
) -> Result<LibraryFileOrdinal, InjectedProfileError> {
    match source {
        SourceUnit::Library { file_ordinal } => Ok(file_ordinal),
        SourceUnit::User { module_ordinal, .. } => user_source_ordinals
            .get(&module_ordinal)
            .copied()
            .ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(format!(
                    "semantic probe omits source ordinal for user module {}",
                    module_ordinal.index(),
                ))
            }),
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn normalize_value_probe(
    name: &str,
    probe: &ValueProbe,
    user_source_ordinals: &BTreeMap<ModuleOrdinal, LibraryFileOrdinal>,
) -> Result<NormalizedValueProbeForTest, InjectedProfileError> {
    let callable_members = probe
        .callable_members
        .iter()
        .map(|member| {
            Ok(NormalizedCallableMemberProbeForTest {
                name: member.name.clone(),
                source_ordinal: normalized_library_source_ordinal(
                    member.source,
                    user_source_ordinals,
                )?,
                reservation_source_ordinal: normalized_library_source_ordinal(
                    member.reservation_source,
                    user_source_ordinals,
                )?,
                source_start: member.source_start,
                call_signature_count: member.call_signature_count,
                signatures: member.signatures.clone(),
            })
        })
        .collect::<Result<Vec<_>, InjectedProfileError>>()?;
    Ok(NormalizedValueProbeForTest {
        name: name.to_owned(),
        declaration_count: probe.declaration_count,
        call_signature_count: probe.call_signature_count,
        signatures: probe.signatures.clone(),
        member_names: probe.member_names.clone(),
        callable_members,
    })
}

pub(super) struct CanonicalInput<'source> {
    pub(super) file_ordinal: LibraryFileOrdinal,
    pub(super) source: &'source str,
    pub(super) kind: SourceFileKind,
    pub(super) source_key: ExactKey,
    pub(super) origin: CompilationOrigin,
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn independent_event_owner_sites_for_oracle(
    canonical: &[CanonicalInput<'_>],
    parsed: &[&Program<'_>],
    manifests: &[SourceDeclarationManifest<'_>],
    binder: &Binder,
) -> Result<Vec<CollisionReplayOwnerSite>, InjectedProfileError> {
    let mut allocator = IndependentEventOwnerSiteOracle::new();
    let mut reservations: LexicalReservations<(usize, usize)> = LexicalReservations::default();
    for ((input, program), manifest) in canonical.iter().zip(parsed).zip(manifests) {
        allocator.select_file(input.file_ordinal);
        reservations
            .reserve_program_with_declaration_manifest(program, manifest, &mut allocator)
            .map_err(|message| InjectedProfileError::Reservation(message.to_owned()))?;
        if matches!(input.origin, CompilationOrigin::Library(_)) {
            allocator.reserve_exact_site(
                Span::from_oxc(program.span),
                CollisionReplayEventPhase::Immediate,
            );
        }
    }
    for descriptor in independent_library_reporting_site_descriptors(binder) {
        allocator.select_file(descriptor.file_ordinal);
        allocator.reserve_exact_site(descriptor.span, CollisionReplayEventPhase::Immediate);
    }
    allocator.finish()
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn verify_independent_event_owner_sites(
    plan: &CollisionReplayPlan,
    independent: &[CollisionReplayOwnerSite],
) -> Result<(), InjectedProfileError> {
    let captured = plan
        .full_oracle_snapshot_for_test()
        .owner_sites
        .iter()
        .filter(|site| matches!(site.provenance, CollisionReplaySiteProvenance::Event { .. }))
        .cloned()
        .collect::<Vec<_>>();
    if captured != independent {
        return Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::IndependentOwnerSiteOracleMismatch,
        ));
    }
    Ok(())
}

pub(super) struct CanonicalLibraryFrontend<'source, 'ast> {
    pub(super) canonical: Vec<CanonicalInput<'source>>,
    pub(super) parsed: Vec<&'ast Program<'ast>>,
    pub(super) declaration_manifests: Vec<SourceDeclarationManifest<'ast>>,
    pub(super) binder: Binder,
    pub(super) module_scopes: Vec<ScopeId>,
    pub(super) semantic_scopes: Vec<ScopeId>,
    pub(super) user_start: Option<usize>,
    pub(super) user_units: Vec<(ModuleOrdinal, UnitSlot)>,
    pub(super) user_events: EventStore,
    pub(super) combined_lexical_events: Option<LexicalReservations<PrivateCombinedRecordTicket>>,
    pub(super) external_effects: BTreeMap<UserRecordTicket, CandidateEffects>,
    pub(super) module_placeholders: Vec<Vec<ImportPlaceholder>>,
    pub(super) collision_root_provenance: LibraryRootProjection,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) parse_elapsed: Duration,
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) bind_elapsed: Duration,
}

#[derive(Clone, Copy)]
enum TrustedLibraryMarkerShape {
    IntrinsicAlias,
    ConditionalAlias,
    EmptyInterface,
}

fn exact_type_parameter(
    declaration: Option<&oxc_ast::ast::TSTypeParameterDeclaration<'_>>,
    name: &str,
    constraint: Option<fn(&oxc_ast::ast::TSType<'_>) -> bool>,
) -> bool {
    let Some(parameter) = declaration
        .and_then(|declaration| (declaration.params.len() == 1).then(|| &declaration.params[0]))
    else {
        return false;
    };
    parameter.name.name == name
        && parameter.default.is_none()
        && !parameter.r#in
        && !parameter.out
        && !parameter.r#const
        && match (parameter.constraint.as_ref(), constraint) {
            (None, None) => true,
            (Some(actual), Some(expected)) => expected(actual),
            _ => false,
        }
}

fn is_string_keyword(ty: &oxc_ast::ast::TSType<'_>) -> bool {
    matches!(ty, oxc_ast::ast::TSType::TSStringKeyword(_))
}

fn exact_plain_type_reference(ty: &oxc_ast::ast::TSType<'_>, name: &str) -> bool {
    let oxc_ast::ast::TSType::TSTypeReference(reference) = ty else {
        return false;
    };
    matches!(
        &reference.type_name,
        oxc_ast::ast::TSTypeName::IdentifierReference(identifier)
            if identifier.name == name
    ) && reference.type_arguments.is_none()
}

fn exact_unary_type_reference(
    ty: &oxc_ast::ast::TSType<'_>,
    name: &str,
    argument_name: &str,
) -> bool {
    let oxc_ast::ast::TSType::TSTypeReference(reference) = ty else {
        return false;
    };
    let Some(arguments) = reference.type_arguments.as_ref() else {
        return false;
    };
    matches!(
        &reference.type_name,
        oxc_ast::ast::TSTypeName::IdentifierReference(identifier)
            if identifier.name == name
    ) && arguments.params.len() == 1
        && exact_plain_type_reference(&arguments.params[0], argument_name)
}

fn exact_infer_type(ty: &oxc_ast::ast::TSType<'_>, name: &str) -> bool {
    let oxc_ast::ast::TSType::TSInferType(infer) = ty else {
        return false;
    };
    let parameter = &infer.type_parameter;
    parameter.name.name == name
        && parameter.constraint.is_none()
        && parameter.default.is_none()
        && !parameter.r#in
        && !parameter.out
        && !parameter.r#const
}

fn exact_rest_function(
    ty: &oxc_ast::ast::TSType<'_>,
    rest_name: &str,
    rest_type: fn(&oxc_ast::ast::TSType<'_>) -> bool,
    return_type: fn(&oxc_ast::ast::TSType<'_>) -> bool,
) -> bool {
    let oxc_ast::ast::TSType::TSFunctionType(function) = ty else {
        return false;
    };
    let Some(rest) = function.params.rest.as_ref() else {
        return false;
    };
    let Some(rest_annotation) = rest.type_annotation.as_ref() else {
        return false;
    };
    matches!(
        &rest.rest.argument,
        oxc_ast::ast::BindingPattern::BindingIdentifier(identifier)
            if identifier.name == rest_name
    ) && function.type_parameters.is_none()
        && function.this_param.is_none()
        && function.params.kind == oxc_ast::ast::FormalParameterKind::Signature
        && function.params.items.is_empty()
        && rest.decorators.is_empty()
        && rest_type(&rest_annotation.type_annotation)
        && return_type(&function.return_type.type_annotation)
}

fn exact_omit_this_parameter_body(annotation: &oxc_ast::ast::TSType<'_>) -> bool {
    fn infer_a(ty: &oxc_ast::ast::TSType<'_>) -> bool {
        exact_infer_type(ty, "A")
    }
    fn infer_r(ty: &oxc_ast::ast::TSType<'_>) -> bool {
        exact_infer_type(ty, "R")
    }
    fn reference_a(ty: &oxc_ast::ast::TSType<'_>) -> bool {
        exact_plain_type_reference(ty, "A")
    }
    fn reference_r(ty: &oxc_ast::ast::TSType<'_>) -> bool {
        exact_plain_type_reference(ty, "R")
    }

    let oxc_ast::ast::TSType::TSConditionalType(outer) = annotation else {
        return false;
    };
    let oxc_ast::ast::TSType::TSConditionalType(inner) = &outer.false_type else {
        return false;
    };
    matches!(outer.check_type, oxc_ast::ast::TSType::TSUnknownKeyword(_))
        && exact_unary_type_reference(&outer.extends_type, "ThisParameterType", "T")
        && exact_plain_type_reference(&outer.true_type, "T")
        && exact_plain_type_reference(&inner.check_type, "T")
        && exact_rest_function(&inner.extends_type, "args", infer_a, infer_r)
        && exact_rest_function(&inner.true_type, "args", reference_a, reference_r)
        && exact_plain_type_reference(&inner.false_type, "T")
}

pub(super) fn seed_trusted_library_markers(
    roots: &LibraryRootProjection,
    declarations: &mut super::context::TypeDeclTable<'_>,
    resolved: &mut super::context::TypeResolvedTable,
    interner: &Interner,
) {
    let well_known = interner.well_known();
    for (name, marker, shape) in [
        (
            "Uppercase",
            well_known.uppercase,
            TrustedLibraryMarkerShape::IntrinsicAlias,
        ),
        (
            "Lowercase",
            well_known.lowercase,
            TrustedLibraryMarkerShape::IntrinsicAlias,
        ),
        (
            "Capitalize",
            well_known.capitalize,
            TrustedLibraryMarkerShape::IntrinsicAlias,
        ),
        (
            "Uncapitalize",
            well_known.uncapitalize,
            TrustedLibraryMarkerShape::IntrinsicAlias,
        ),
        (
            "ThisType",
            well_known.this_type,
            TrustedLibraryMarkerShape::EmptyInterface,
        ),
        (
            "OmitThisParameter",
            well_known.omit_this_parameter,
            TrustedLibraryMarkerShape::ConditionalAlias,
        ),
    ] {
        let Some(group) = roots
            .root_rows
            .binary_search_by(|row| row.name.as_str().cmp(name))
            .ok()
            .and_then(|index| roots.root_rows.get(index))
            .and_then(|row| row.ty)
        else {
            continue;
        };
        let trusted_shape = match (shape, declarations.get(group.index())) {
            (
                TrustedLibraryMarkerShape::IntrinsicAlias,
                Some(super::context::TypeDecl::Alias {
                    annotation,
                    param_decl,
                    name: declaration_name,
                    ..
                }),
            ) if declaration_name == name
                && exact_type_parameter(*param_decl, "S", Some(is_string_keyword))
                && matches!(annotation, oxc_ast::ast::TSType::TSIntrinsicKeyword(_)) =>
            {
                true
            }
            (
                TrustedLibraryMarkerShape::ConditionalAlias,
                Some(super::context::TypeDecl::Alias {
                    annotation,
                    param_decl,
                    name: declaration_name,
                    ..
                }),
            ) if declaration_name == name
                && exact_type_parameter(*param_decl, "T", None)
                && exact_omit_this_parameter_body(annotation) =>
            {
                true
            }
            (
                TrustedLibraryMarkerShape::EmptyInterface,
                Some(super::context::TypeDecl::Interface {
                    param_decl,
                    extends,
                    fragments,
                    ..
                }),
            ) if exact_type_parameter(*param_decl, "T", None)
                && extends.is_empty()
                && fragments.len() == 1
                && fragments
                    .iter()
                    .all(|fragment| fragment.members.is_empty() && fragment.extends.is_empty()) =>
            {
                true
            }
            _ => false,
        };
        if !trusted_shape {
            continue;
        }
        if let Some(slot) = resolved.get_mut(group.index()) {
            *slot = Some(marker);
        }
    }
}

pub(super) fn certify_library_values(roots: &LibraryRootProjection) -> CertifiedLibraryValues {
    let symbol = roots
        .root_rows
        .binary_search_by(|row| row.name.as_str().cmp("Symbol"))
        .ok()
        .and_then(|index| roots.root_rows.get(index))
        .and_then(|row| row.value);
    CertifiedLibraryValues { symbol }
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
thread_local! {
    pub(super) static CANONICAL_FRONTEND_ENTRIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub(super) static CANONICAL_FRONTEND_PARSE_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub(super) static CANONICAL_FRONTEND_BIND_BATCHES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub(super) static CANONICAL_FRONTEND_BIND_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub(super) static CANONICAL_FRONTEND_FULL_PRODUCTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub(super) static CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CanonicalLibraryFrontendWorkForTest {
    pub entries: u64,
    pub parse_units: u64,
    pub bind_batches: u64,
    pub bind_units: u64,
    pub full_source_products_consumed: u64,
    pub checkpoint_products_consumed: u64,
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
fn canonical_library_frontend_work_for_test() -> CanonicalLibraryFrontendWorkForTest {
    CanonicalLibraryFrontendWorkForTest {
        entries: CANONICAL_FRONTEND_ENTRIES.get(),
        parse_units: CANONICAL_FRONTEND_PARSE_UNITS.get(),
        bind_batches: CANONICAL_FRONTEND_BIND_BATCHES.get(),
        bind_units: CANONICAL_FRONTEND_BIND_UNITS.get(),
        full_source_products_consumed: CANONICAL_FRONTEND_FULL_PRODUCTS.get(),
        checkpoint_products_consumed: CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS.get(),
    }
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
pub struct CanonicalLibraryFrontendWorkScopeForTest(CanonicalLibraryFrontendWorkForTest);

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
impl CanonicalLibraryFrontendWorkScopeForTest {
    pub fn start() -> Self {
        Self(canonical_library_frontend_work_for_test())
    }

    pub fn finish(self) -> CanonicalLibraryFrontendWorkForTest {
        let end = canonical_library_frontend_work_for_test();
        CanonicalLibraryFrontendWorkForTest {
            entries: end.entries.saturating_sub(self.0.entries),
            parse_units: end.parse_units.saturating_sub(self.0.parse_units),
            bind_batches: end.bind_batches.saturating_sub(self.0.bind_batches),
            bind_units: end.bind_units.saturating_sub(self.0.bind_units),
            full_source_products_consumed: end
                .full_source_products_consumed
                .saturating_sub(self.0.full_source_products_consumed),
            checkpoint_products_consumed: end
                .checkpoint_products_consumed
                .saturating_sub(self.0.checkpoint_products_consumed),
        }
    }
}

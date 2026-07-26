//! Authoritative source-backed compiler for the pinned default library.

use super::profile::{ExactLibraryProfile, ExactLibrarySource};
use crate::binder::bind::LibraryBinderCheckpoint;
use crate::check::checker::library_compiler::{
    compile_owned_injected_profile, freeze_library_runtime_product, CompiledLibraryRuntimeProduct,
    InjectedLibrarySource, OwnedLibraryRuntimeState,
};
use crate::source::LibraryFileOrdinal;
use sha2::{Digest, Sha256};
use std::fmt;

#[cfg(test)]
thread_local! {
    static COMPILER_INVOCATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static COMPILER_SOURCE_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static COMPILER_PARSE_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static COMPILER_BIND_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static COMPILER_CHECK_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryCompilerWorkForTest {
    pub(crate) compiles: u64,
    pub(crate) parses: u64,
    pub(crate) binds: u64,
    pub(crate) checks: u64,
}

#[cfg(test)]
fn compiler_work_for_test() -> LibraryCompilerWorkForTest {
    LibraryCompilerWorkForTest {
        compiles: COMPILER_INVOCATIONS.get(),
        parses: COMPILER_PARSE_UNITS.get(),
        binds: COMPILER_BIND_UNITS.get(),
        checks: COMPILER_CHECK_UNITS.get(),
    }
}

#[cfg(test)]
pub(crate) struct LibraryCompilerWorkScopeForTest(LibraryCompilerWorkForTest);

#[cfg(test)]
impl LibraryCompilerWorkScopeForTest {
    pub(crate) fn start() -> Self {
        Self(compiler_work_for_test())
    }

    pub(crate) fn finish(self) -> LibraryCompilerWorkForTest {
        let end = compiler_work_for_test();
        LibraryCompilerWorkForTest {
            compiles: end.compiles.saturating_sub(self.0.compiles),
            parses: end.parses.saturating_sub(self.0.parses),
            binds: end.binds.saturating_sub(self.0.binds),
            checks: end.checks.saturating_sub(self.0.checks),
        }
    }
}

// SHA-256 of the preceding schema identity plus `|binder-snapshot-v2`.
pub const COMPILER_SCHEMA_SHA256: &str =
    "6cf27cde368f8b2ff3bdafd5fce8fb3550ec8e2264aab7249362e2294e3f5be0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryCompilationReport {
    pub parse_units: usize,
    pub bind_units: usize,
    pub statement_check_units: usize,
    pub reserved_records: usize,
    pub filled_records: usize,
    pub publication_validations: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceComponent {
    record_count: usize,
    byte_len: usize,
    sha256: String,
}

impl EvidenceComponent {
    pub const fn record_count(&self) -> usize {
        self.record_count
    }

    pub const fn byte_len(&self) -> usize {
        self.byte_len
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibrarySourceIdentity {
    ordinal: LibraryFileOrdinal,
    name: String,
    sha256: String,
    library_owned: bool,
}

impl LibrarySourceIdentity {
    pub const fn ordinal(&self) -> LibraryFileOrdinal {
        self.ordinal
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub const fn is_library_owned(&self) -> bool {
        self.library_owned
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryEvidence {
    profile_identity: String,
    diagnostics: EvidenceComponent,
    incompletes: EvidenceComponent,
    library_ledger: EvidenceComponent,
    source_identities: Vec<LibrarySourceIdentity>,
    semantic_identity: String,
}

impl LibraryEvidence {
    pub fn profile_identity(&self) -> &str {
        &self.profile_identity
    }

    pub const fn diagnostics(&self) -> &EvidenceComponent {
        &self.diagnostics
    }

    pub const fn incompletes(&self) -> &EvidenceComponent {
        &self.incompletes
    }

    pub const fn library_ledger(&self) -> &EvidenceComponent {
        &self.library_ledger
    }

    pub fn source_identities(&self) -> &[LibrarySourceIdentity] {
        &self.source_identities
    }

    pub fn semantic_identity(&self) -> &str {
        &self.semantic_identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibrarySemanticIdentity {
    runtime_projection: String,
    evidence: String,
}

impl LibrarySemanticIdentity {
    pub fn runtime_projection(&self) -> &str {
        &self.runtime_projection
    }

    pub fn evidence(&self) -> &str {
        &self.evidence
    }
}

pub struct LibraryRuntimeProjection {
    semantic_identity: String,
    pub(crate) _runtime: CompiledLibraryRuntimeProduct,
}

impl fmt::Debug for LibraryRuntimeProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LibraryRuntimeProjection")
            .field("semantic_identity", &self.semantic_identity)
            .finish_non_exhaustive()
    }
}

impl LibraryRuntimeProjection {
    pub fn semantic_identity(&self) -> &str {
        &self.semantic_identity
    }
}

#[derive(Debug)]
pub struct CompiledLibrary {
    profile_identity: String,
    report: LibraryCompilationReport,
    evidence: LibraryEvidence,
    runtime_projection: LibraryRuntimeProjection,
    semantic_identity: LibrarySemanticIdentity,
}

impl CompiledLibrary {
    pub fn profile_identity(&self) -> &str {
        &self.profile_identity
    }

    pub const fn report(&self) -> &LibraryCompilationReport {
        &self.report
    }

    pub const fn evidence(&self) -> &LibraryEvidence {
        &self.evidence
    }

    pub const fn runtime_projection(&self) -> &LibraryRuntimeProjection {
        &self.runtime_projection
    }

    #[cfg(test)]
    pub(crate) const fn replay_index_for_test(
        &self,
    ) -> &crate::check::checker::replay_index::AdmittedCollisionReplayIndex {
        &self.runtime_projection._runtime._replay_index
    }

    pub const fn semantic_identity(&self) -> &LibrarySemanticIdentity {
        &self.semantic_identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryCompilerError {
    SourceNotUtf8 { file_ordinal: usize, name: String },
    Compilation { message: String },
    RuntimeProduct { message: String },
}

impl fmt::Display for LibraryCompilerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotUtf8 { name, .. } => {
                write!(formatter, "library source {name:?} is not UTF-8")
            }
            Self::Compilation { message } => {
                write!(formatter, "library compilation failed: {message}")
            }
            Self::RuntimeProduct { message } => {
                write!(
                    formatter,
                    "library runtime product is incomplete: {message}"
                )
            }
        }
    }
}

impl std::error::Error for LibraryCompilerError {}

#[derive(Clone, Copy, Debug, Default)]
pub struct LibraryCompiler;

impl LibraryCompiler {
    pub const fn new() -> Self {
        Self
    }

    pub fn compile(
        &self,
        profile: &ExactLibraryProfile,
    ) -> Result<CompiledLibrary, LibraryCompilerError> {
        self.compile_sources(profile.profile_identity(), profile.sources())
    }

    #[doc(hidden)]
    pub fn compile_binder_checkpoint(
        &self,
        profile: &ExactLibraryProfile,
    ) -> Result<LibraryBinderCheckpoint, LibraryCompilerError> {
        #[cfg(test)]
        {
            COMPILER_INVOCATIONS.set(COMPILER_INVOCATIONS.get().saturating_add(1));
            COMPILER_SOURCE_BYTES.set(COMPILER_SOURCE_BYTES.get().saturating_add(
                profile.sources().iter().fold(0u64, |total, source| {
                    total.saturating_add(u64::try_from(source.bytes().len()).unwrap_or(u64::MAX))
                }),
            ));
        }
        let owned_sources = owned_library_sources(profile.sources())?;
        let injected = injected_library_sources(&owned_sources);
        let checkpoint =
            crate::check::checker::library_compiler::compile_library_binder_checkpoint(&injected)
                .map_err(|error| LibraryCompilerError::Compilation {
                message: format!("{error:?}"),
            })?;
        #[cfg(test)]
        {
            let unit_count = u64::try_from(injected.len()).unwrap_or(u64::MAX);
            COMPILER_PARSE_UNITS.set(COMPILER_PARSE_UNITS.get().saturating_add(unit_count));
            COMPILER_BIND_UNITS.set(COMPILER_BIND_UNITS.get().saturating_add(unit_count));
        }
        Ok(checkpoint)
    }

    fn compile_sources(
        &self,
        profile_identity: &str,
        sources: &[ExactLibrarySource],
    ) -> Result<CompiledLibrary, LibraryCompilerError> {
        #[cfg(test)]
        {
            COMPILER_INVOCATIONS.set(COMPILER_INVOCATIONS.get().saturating_add(1));
            let source_bytes = sources.iter().fold(0u64, |total, source| {
                total.saturating_add(u64::try_from(source.bytes().len()).unwrap_or(u64::MAX))
            });
            COMPILER_SOURCE_BYTES.set(COMPILER_SOURCE_BYTES.get().saturating_add(source_bytes));
        }
        let owned_sources = owned_library_sources(sources)?;
        let injected = injected_library_sources(&owned_sources);
        let (run, runtime) = compile_owned_injected_profile(&injected).map_err(|error| {
            LibraryCompilerError::Compilation {
                message: format!("{error:?}"),
            }
        })?;
        #[cfg(test)]
        record_phase_counts(&run.phase_counts);
        let runtime = freeze_library_runtime_product(runtime).map_err(|message| {
            LibraryCompilerError::RuntimeProduct {
                message: message.to_owned(),
            }
        })?;

        let diagnostics = component_identity(
            &run.evidence.diagnostics,
            run.library_records
                .iter()
                .filter(|(_, record)| record.is_diagnostic())
                .count(),
        );
        let incompletes = component_identity(
            &run.evidence.incompletes,
            run.library_records.len() - diagnostics.record_count,
        );
        let library_ledger = component_identity(&run.evidence.ledger, run.library_records.len());
        let source_identities = owned_sources
            .iter()
            .map(|(ordinal, name, source)| LibrarySourceIdentity {
                ordinal: *ordinal,
                name: name.clone(),
                sha256: digest(source),
                library_owned: true,
            })
            .collect::<Vec<_>>();
        let evidence_identity = aggregate_identity(
            b"typokat-library-evidence-v1",
            [
                diagnostics.sha256.as_str(),
                incompletes.sha256.as_str(),
                library_ledger.sha256.as_str(),
            ],
        );
        let runtime_identity = aggregate_identity(
            b"typokat-library-runtime-v1",
            [profile_identity, COMPILER_SCHEMA_SHA256],
        );
        let report = LibraryCompilationReport {
            parse_units: run.phase_counts.parse_units,
            bind_units: run.phase_counts.bind_units,
            statement_check_units: run.phase_counts.statement_check_units,
            reserved_records: run.phase_counts.reserved_records,
            filled_records: run.phase_counts.filled_records,
            publication_validations: run.phase_counts.publication_validations,
        };
        let evidence = LibraryEvidence {
            profile_identity: profile_identity.to_owned(),
            diagnostics,
            incompletes,
            library_ledger,
            source_identities,
            semantic_identity: evidence_identity.clone(),
        };
        let semantic_identity = LibrarySemanticIdentity {
            runtime_projection: runtime_identity.clone(),
            evidence: evidence_identity,
        };
        Ok(CompiledLibrary {
            profile_identity: profile_identity.to_owned(),
            report,
            evidence,
            runtime_projection: LibraryRuntimeProjection {
                semantic_identity: runtime_identity,
                _runtime: runtime,
            },
            semantic_identity,
        })
    }
}

/// Compile a profile straight into the owned runtime state the frozen base is sealed from.
///
/// This is the production route to a default-library base: no artifact is admitted, and the
/// evidence projection `compile` builds is deliberately skipped.
pub(crate) fn compile_owned_library_runtime(
    profile: &ExactLibraryProfile,
) -> Result<OwnedLibraryRuntimeState, LibraryCompilerError> {
    #[cfg(test)]
    {
        COMPILER_INVOCATIONS.set(COMPILER_INVOCATIONS.get().saturating_add(1));
        let source_bytes = profile.sources().iter().fold(0u64, |total, source| {
            total.saturating_add(u64::try_from(source.bytes().len()).unwrap_or(u64::MAX))
        });
        COMPILER_SOURCE_BYTES.set(COMPILER_SOURCE_BYTES.get().saturating_add(source_bytes));
    }
    let owned_sources = owned_library_sources(profile.sources())?;
    let injected = injected_library_sources(&owned_sources);
    let (run, runtime) = compile_owned_injected_profile(&injected).map_err(|error| {
        LibraryCompilerError::Compilation {
            message: format!("{error:?}"),
        }
    })?;
    #[cfg(test)]
    record_phase_counts(&run.phase_counts);
    #[cfg(not(test))]
    let _ = run;
    Ok(runtime)
}

type OwnedLibrarySource = (LibraryFileOrdinal, String, String);

fn owned_library_sources(
    sources: &[ExactLibrarySource],
) -> Result<Vec<OwnedLibrarySource>, LibraryCompilerError> {
    sources
        .iter()
        .map(|source| {
            let text = std::str::from_utf8(source.bytes()).map_err(|_| {
                LibraryCompilerError::SourceNotUtf8 {
                    file_ordinal: source.ordinal().index(),
                    name: source.name().to_owned(),
                }
            })?;
            Ok((source.ordinal(), source.name().to_owned(), text.to_owned()))
        })
        .collect()
}

fn injected_library_sources(owned: &[OwnedLibrarySource]) -> Vec<InjectedLibrarySource<'_>> {
    owned
        .iter()
        .map(|(ordinal, name, source)| InjectedLibrarySource {
            file_ordinal: *ordinal,
            name,
            source,
        })
        .collect()
}

#[cfg(test)]
fn record_phase_counts(counts: &crate::check::checker::library_compiler::LibraryPhaseCounts) {
    COMPILER_PARSE_UNITS.set(
        COMPILER_PARSE_UNITS
            .get()
            .saturating_add(u64::try_from(counts.parse_units).unwrap_or(u64::MAX)),
    );
    COMPILER_BIND_UNITS.set(
        COMPILER_BIND_UNITS
            .get()
            .saturating_add(u64::try_from(counts.bind_units).unwrap_or(u64::MAX)),
    );
    COMPILER_CHECK_UNITS.set(
        COMPILER_CHECK_UNITS
            .get()
            .saturating_add(u64::try_from(counts.statement_check_units).unwrap_or(u64::MAX)),
    );
}

fn component_identity(bytes: &[u8], record_count: usize) -> EvidenceComponent {
    EvidenceComponent {
        record_count,
        byte_len: bytes.len(),
        sha256: digest(bytes),
    }
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

fn aggregate_identity<'identity>(
    domain: &[u8],
    identities: impl IntoIterator<Item = &'identity str>,
) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for identity in identities {
        digest.update(
            u64::try_from(identity.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        digest.update(identity.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync_static<T: Send + Sync + 'static>(_: &T) {}

    const PROFILE_IDENTITY: &str =
        "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";

    #[test]
    fn library_compiler_separates_runtime_product_from_evidence() {
        let profile = ExactLibraryProfile::load_packaged().expect("exact packaged profile");
        let compiled = LibraryCompiler::new()
            .compile(&profile)
            .expect("complete source-backed library compilation");

        assert_send_sync_static(compiled.runtime_projection());
        assert_eq!(compiled.profile_identity(), PROFILE_IDENTITY);
        assert_eq!(compiled.report().parse_units, 82);
        assert_eq!(compiled.report().bind_units, 82);
        assert_eq!(compiled.report().statement_check_units, 82);
        assert_eq!(compiled.report().reserved_records, 42_496);
        assert_eq!(compiled.report().filled_records, 42_496);
        assert_eq!(compiled.report().publication_validations, 2_099);
        assert_eq!(compiled.evidence().diagnostics().record_count(), 265);
        assert_eq!(compiled.evidence().diagnostics().byte_len(), 125_251);
        assert_eq!(
            compiled.evidence().diagnostics().sha256(),
            "79ef18a2496c296b380e3d37dd71e589ad036614ce2fe0f9b49073cc3bf5d427"
        );
        assert_eq!(compiled.evidence().incompletes().record_count(), 610);
        assert_eq!(compiled.evidence().incompletes().byte_len(), 97_796);
        assert_eq!(
            compiled.evidence().incompletes().sha256(),
            "8c268088f8afd8048690584008c40a49cd3337b91f345b2e879d625525ccf6d8"
        );
        assert_eq!(compiled.evidence().library_ledger().record_count(), 875);
        assert_eq!(compiled.evidence().library_ledger().byte_len(), 223_016);
        assert_eq!(
            compiled.evidence().library_ledger().sha256(),
            "33204da8512a79ba77cc647f1f5641c91726e4a6aaa7b7a394d0851e5f7bd31c"
        );
        assert_eq!(compiled.evidence().source_identities().len(), 82);
        assert!(compiled
            .evidence()
            .source_identities()
            .iter()
            .all(|source| source.is_library_owned()));
        assert_eq!(
            compiled.semantic_identity().runtime_projection(),
            compiled.runtime_projection().semantic_identity()
        );
        assert_eq!(
            compiled.semantic_identity().evidence(),
            compiled.evidence().semantic_identity()
        );
        assert_eq!(compiled.evidence().profile_identity(), PROFILE_IDENTITY);
    }
}

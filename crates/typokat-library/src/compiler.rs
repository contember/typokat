//! Authoritative source-backed compiler for the pinned default library.

use super::profile::{ExactLibraryProfile, ExactLibrarySource};
use crate::binder::bind::LibraryBinderCheckpoint;
use crate::check::checker::library_compiler::{
    compile_owned_injected_base_profile_with_plan, compile_owned_injected_profile,
    freeze_library_runtime_product, CompiledLibraryRuntimeProduct, InjectedLibrarySource,
    OwnedLibraryRuntimeState,
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
pub struct LibrarySemanticIdentity {
    runtime_projection: String,
}

impl LibrarySemanticIdentity {
    pub fn runtime_projection(&self) -> &str {
        &self.runtime_projection
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
        let semantic_identity = LibrarySemanticIdentity {
            runtime_projection: runtime_identity.clone(),
        };
        Ok(CompiledLibrary {
            profile_identity: profile_identity.to_owned(),
            report,
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
/// This is the production route to a default-library base: no artifact is admitted, the
/// live source trace becomes the compact collision plan retained by the frozen base (ADR-0020),
/// and the library's own records are dropped rather than retained (ADR-0018) — the pinned suite
/// census in `tests/library_owned_records.rs` is their witness.
pub(crate) fn compile_owned_library_runtime(
    profile: &ExactLibraryProfile,
) -> Result<
    (
        OwnedLibraryRuntimeState,
        std::sync::Arc<crate::check::checker::replay_index::CollisionReplayPlan>,
    ),
    LibraryCompilerError,
> {
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
    let (run, runtime, collision_plan) = compile_owned_injected_base_profile_with_plan(&injected)
        .map_err(|error| LibraryCompilerError::Compilation {
        message: format!("{error:?}"),
    })?;
    #[cfg(test)]
    record_phase_counts(&run.phase_counts);
    #[cfg(not(test))]
    let _ = run;
    Ok((runtime, collision_plan))
}

pub(super) type OwnedLibrarySource = (LibraryFileOrdinal, String, String);

pub(super) fn owned_library_sources(
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

pub(super) fn injected_library_sources(
    owned: &[OwnedLibrarySource],
) -> Vec<InjectedLibrarySource<'_>> {
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
    use crate::check::checker::library_compiler::{
        canonical_library_evidence_for_test, run_injected_profile,
    };

    fn assert_send_sync_static<T: Send + Sync + 'static>(_: &T) {}

    const PROFILE_IDENTITY: &str =
        "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";

    fn digest(bytes: impl AsRef<[u8]>) -> String {
        format!("{:x}", Sha256::digest(bytes.as_ref()))
    }

    #[test]
    fn library_compiler_produces_one_frozen_runtime_product() {
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
        assert_eq!(
            compiled.semantic_identity().runtime_projection(),
            compiled.runtime_projection().semantic_identity()
        );
    }

    /// The canonical evidence projection is a suite assertion, not compile work (ADR-0017).
    ///
    /// It pins the library's own records byte-for-byte, but a digest only ever says that
    /// something moved. What moved is named by `tests/library_owned_records.rs` against
    /// `tests/fixtures/library-owned-records.txt` (ADR-0018); read that pin first.
    #[test]
    fn library_records_project_to_their_pinned_canonical_evidence() {
        let profile = ExactLibraryProfile::load_packaged().expect("exact packaged profile");
        let owned_sources = owned_library_sources(profile.sources()).expect("UTF-8 sources");
        let injected = injected_library_sources(&owned_sources);
        let run = run_injected_profile(&injected).expect("source-backed library compilation");
        let evidence = canonical_library_evidence_for_test(&injected, &run.library_records)
            .expect("canonical evidence projection");

        let diagnostic_count = run
            .library_records
            .iter()
            .filter(|(_, record)| record.is_diagnostic())
            .count();
        assert_eq!(diagnostic_count, 265);
        assert_eq!(evidence.diagnostics.len(), 125_251);
        assert_eq!(
            digest(&evidence.diagnostics),
            "79ef18a2496c296b380e3d37dd71e589ad036614ce2fe0f9b49073cc3bf5d427"
        );
        assert_eq!(run.library_records.len() - diagnostic_count, 433);
        assert_eq!(evidence.incompletes.len(), 69_619);
        assert_eq!(
            digest(&evidence.incompletes),
            "6febf130a617d6108b40e28e6aaecacb9aa247218a183e5ce0de11f274d18e51"
        );
        assert_eq!(run.library_records.len(), 698);
        assert_eq!(evidence.ledger.len(), 194_839);
        assert_eq!(
            digest(&evidence.ledger),
            "dbfe5132c3d37c5378c177df22b9f00ec1fab26713ce9b80a57c43a7dccb50e3"
        );
        assert_eq!(owned_sources.len(), 82);
    }
}

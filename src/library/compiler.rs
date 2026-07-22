//! Authoritative source-backed compiler for the pinned default library.

#[cfg(test)]
use super::profile::TestLibraryProfileInput;
use super::profile::{ExactLibraryProfile, ExactLibrarySource};
use crate::check::checker::library_compiler::{
    compile_owned_injected_profile, freeze_library_runtime_product, CompiledLibraryRuntimeProduct,
    InjectedLibrarySource,
};
use crate::source::LibraryFileOrdinal;
use sha2::{Digest, Sha256};
use std::fmt;

#[cfg(test)]
thread_local! {
    static COMPILER_INVOCATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static COMPILER_SOURCE_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn compiler_measurement_for_test() -> (u64, u64) {
    (COMPILER_INVOCATIONS.get(), COMPILER_SOURCE_BYTES.get())
}

pub const COMPILER_SCHEMA_SHA256: &str =
    "a78ea0521c7c375669bfdb08f0929a5e4b1d0b0d6928de60fbfe09b222a8bc65";

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
    pub(crate) fn from_parts(runtime_projection: String, evidence: String) -> Self {
        Self {
            runtime_projection,
            evidence,
        }
    }

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

    #[cfg(test)]
    pub(crate) fn compile_test_input(
        &self,
        input: &TestLibraryProfileInput,
    ) -> Result<CompiledLibrary, LibraryCompilerError> {
        self.compile_sources(input.profile_identity(), input.sources())
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
        let owned_sources = sources
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
            .collect::<Result<Vec<_>, LibraryCompilerError>>()?;
        let injected = owned_sources
            .iter()
            .map(|(ordinal, name, source)| InjectedLibrarySource {
                file_ordinal: *ordinal,
                name,
                source,
            })
            .collect::<Vec<_>>();
        let (run, runtime) = compile_owned_injected_profile(&injected).map_err(|error| {
            LibraryCompilerError::Compilation {
                message: format!("{error:?}"),
            }
        })?;
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

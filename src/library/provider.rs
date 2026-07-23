//! Instance-scoped, failure-caching publication of the canonical library base.

use super::base::FrozenLibraryBase;
use super::snapshot;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryInitStage {
    ArtifactAdmission,
    Header,
    Directory,
    Payload,
    Decode,
    DecodeInterner,
    DecodeBinder,
    CollisionReplayIndexAdmission,
    ReferenceValidation,
    Publication,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollisionReplayIndexViolation {
    InvalidEncoding,
    InvalidOwnerPartition,
    InvalidRootIndex,
    InvalidDependencyGraph,
    InvalidOwnerSites,
    InvalidSccPartition,
    InvalidStatementPartition,
    InvalidBaselinePartition,
    NonzeroGenerationHealthCounter,
    ManifestIdentityMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibrarySnapshotViolation {
    MalformedHeader,
    MalformedDirectory,
    InvalidPayload,
    InvalidEncoding,
    InvalidReference,
    IncompletePublication,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryInitCause {
    ArtifactIdentity {
        expected_bytes: usize,
        actual_bytes: usize,
        expected_sha256: String,
        actual_sha256: String,
    },
    InvalidId {
        id: u32,
        limit: usize,
    },
    SnapshotRejected {
        violation: LibrarySnapshotViolation,
    },
    WorkerPanicked {
        worker: &'static str,
    },
    WorkerSpawnFailed {
        worker: &'static str,
    },
    ReplayIndexRejected {
        violation: CollisionReplayIndexViolation,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryInitError {
    stage: LibraryInitStage,
    cause: LibraryInitCause,
}

impl LibraryInitError {
    pub(super) const fn new(stage: LibraryInitStage, cause: LibraryInitCause) -> Self {
        Self { stage, cause }
    }

    pub const fn stage(&self) -> LibraryInitStage {
        self.stage
    }

    pub const fn cause(&self) -> &LibraryInitCause {
        &self.cause
    }
}

impl fmt::Display for LibraryInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "embedded library initialization failed at {:?}: {:?}",
            self.stage, self.cause
        )
    }
}

impl std::error::Error for LibraryInitError {}

enum ProviderInput {
    Packaged,
    #[cfg(test)]
    Canonical(Vec<u8>),
    #[cfg(test)]
    PreAdmitted(snapshot::test_support::PreAdmittedSnapshot),
}

pub struct LibraryBaseProvider {
    result: OnceLock<Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>>>,
    typed_validation_sha256: OnceLock<[u8; 32]>,
    input: ProviderInput,
    attempts: AtomicU64,
    publications: AtomicU64,
    validation_us: AtomicU64,
    decode_us: AtomicU64,
    publication_us: AtomicU64,
}

impl LibraryBaseProvider {
    pub const fn new() -> Self {
        Self {
            result: OnceLock::new(),
            typed_validation_sha256: OnceLock::new(),
            input: ProviderInput::Packaged,
            attempts: AtomicU64::new(0),
            publications: AtomicU64::new(0),
            validation_us: AtomicU64::new(0),
            decode_us: AtomicU64::new(0),
            publication_us: AtomicU64::new(0),
        }
    }

    #[cfg(test)]
    pub(super) fn with_canonical_bytes_for_test(bytes: Vec<u8>) -> Self {
        Self::with_input(ProviderInput::Canonical(bytes))
    }

    #[cfg(test)]
    pub(super) fn with_pre_admitted_snapshot_for_test(
        snapshot: snapshot::test_support::PreAdmittedSnapshot,
    ) -> Self {
        Self::with_input(ProviderInput::PreAdmitted(snapshot))
    }

    #[cfg(test)]
    fn with_input(input: ProviderInput) -> Self {
        Self {
            result: OnceLock::new(),
            typed_validation_sha256: OnceLock::new(),
            input,
            attempts: AtomicU64::new(0),
            publications: AtomicU64::new(0),
            validation_us: AtomicU64::new(0),
            decode_us: AtomicU64::new(0),
            publication_us: AtomicU64::new(0),
        }
    }

    pub fn get(&self) -> Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>> {
        self.result.get_or_init(|| self.initialize()).clone()
    }

    fn initialize(&self) -> Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>> {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        let decoded = match &self.input {
            ProviderInput::Packaged => {
                let validation = Instant::now();
                let admitted = snapshot::admit_packaged_canonical().map_err(Arc::new)?;
                self.validation_us
                    .store(elapsed_us(validation), Ordering::Relaxed);
                let decode = Instant::now();
                let decoded = snapshot::decode_admitted_canonical(admitted).map_err(Arc::new)?;
                self.decode_us.store(elapsed_us(decode), Ordering::Relaxed);
                decoded
            }
            #[cfg(test)]
            ProviderInput::Canonical(bytes) => {
                let validation = Instant::now();
                let admitted =
                    snapshot::admit_canonical_for_test(bytes.clone()).map_err(Arc::new)?;
                self.validation_us
                    .store(elapsed_us(validation), Ordering::Relaxed);
                let decode = Instant::now();
                let decoded = snapshot::decode_admitted_canonical(admitted).map_err(Arc::new)?;
                self.decode_us.store(elapsed_us(decode), Ordering::Relaxed);
                decoded
            }
            #[cfg(test)]
            ProviderInput::PreAdmitted(snapshot) => {
                let validation = Instant::now();
                let decoded = snapshot::decode_pre_admitted(snapshot).map_err(Arc::new)?;
                self.validation_us
                    .store(elapsed_us(validation), Ordering::Relaxed);
                self.decode_us.store(1, Ordering::Relaxed);
                decoded
            }
        };
        let publication = Instant::now();
        if self
            .typed_validation_sha256
            .set(decoded.typed_validation_sha256)
            .is_err()
        {
            return Err(Arc::new(LibraryInitError::new(
                LibraryInitStage::Publication,
                LibraryInitCause::SnapshotRejected {
                    violation: LibrarySnapshotViolation::IncompletePublication,
                },
            )));
        }
        let base = Arc::new(FrozenLibraryBase::from_decoded(decoded).map_err(|_| {
            Arc::new(LibraryInitError::new(
                LibraryInitStage::Publication,
                LibraryInitCause::SnapshotRejected {
                    violation: LibrarySnapshotViolation::IncompletePublication,
                },
            ))
        })?);
        self.publication_us
            .store(elapsed_us(publication), Ordering::Relaxed);
        self.publications.fetch_add(1, Ordering::Relaxed);
        Ok(base)
    }

    #[cfg(test)]
    pub(super) fn measurement_for_test(&self) -> InitializationMeasurement {
        InitializationMeasurement {
            attempts: self.attempts.load(Ordering::Relaxed),
            publications: self.publications.load(Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub(super) fn typed_validation_sha256_for_test(&self) -> Result<String, LibraryInitError> {
        let digest = self.typed_validation_sha256.get().ok_or_else(|| {
            LibraryInitError::new(
                LibraryInitStage::Publication,
                LibraryInitCause::SnapshotRejected {
                    violation: LibrarySnapshotViolation::IncompletePublication,
                },
            )
        })?;
        Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
    }
}

impl Default for LibraryBaseProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn elapsed_us(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros())
        .unwrap_or(u64::MAX)
        .max(1)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InitializationMeasurement {
    pub(super) attempts: u64,
    pub(super) publications: u64,
}

#[cfg(test)]
pub(super) struct FrozenLibraryBaseReleaseProbe {
    pub(super) route: &'static str,
    pub(super) profile_sha256: &'static str,
    pub(super) schema_sha256: &'static str,
    pub(super) artifact_sha256: &'static str,
    pub(super) artifact_bytes: usize,
    pub(super) initializations: u64,
    pub(super) publications: u64,
    pub(super) compiler_invocations: u64,
    pub(super) generator_invocations: u64,
    pub(super) source_bytes_read: u64,
    pub(super) validation_us: u64,
    pub(super) decode_us: u64,
    pub(super) publication_us: u64,
    pub(super) typed_validation_sha256: String,
}

#[cfg(test)]
impl FrozenLibraryBaseReleaseProbe {
    pub(super) fn render(&self) -> String {
        format!(
            concat!(
                "TYPOKAT_LIBRARY_BASE_PROBE={{",
                "\"schema\":1,",
                "\"route\":\"{}\",",
                "\"profile_sha256\":\"{}\",",
                "\"schema_sha256\":\"{}\",",
                "\"artifact_sha256\":\"{}\",",
                "\"artifact_bytes\":{},",
                "\"typed_validation_sha256\":\"{}\",",
                "\"initializations\":{},",
                "\"publications\":{},",
                "\"compiler_invocations\":{},",
                "\"generator_invocations\":{},",
                "\"source_bytes_read\":{},",
                "\"validation_us\":{},",
                "\"decode_us\":{},",
                "\"publication_us\":{}",
                "}}"
            ),
            self.route,
            self.profile_sha256,
            self.schema_sha256,
            self.artifact_sha256,
            self.artifact_bytes,
            self.typed_validation_sha256,
            self.initializations,
            self.publications,
            self.compiler_invocations,
            self.generator_invocations,
            self.source_bytes_read,
            self.validation_us,
            self.decode_us,
            self.publication_us,
        )
    }
}

#[cfg(test)]
pub(super) fn frozen_library_base_release_probe_for_test(
) -> Result<FrozenLibraryBaseReleaseProbe, LibraryInitError> {
    let generation = super::artifact::measure_generation_for_test();
    let provider = LibraryBaseProvider::new();
    let base = provider.get().map_err(|error| (*error).clone())?;
    let typed_validation_sha256 = provider.typed_validation_sha256_for_test()?;
    let generation = generation.finish();
    let measurement = provider.measurement_for_test();
    Ok(FrozenLibraryBaseReleaseProbe {
        route: "production-frozen-library-base",
        profile_sha256: base.identity().profile_sha256(),
        schema_sha256: base.identity().schema_sha256(),
        artifact_sha256: base.identity().artifact_sha256(),
        artifact_bytes: base.identity().artifact_bytes(),
        initializations: measurement.attempts,
        publications: measurement.publications,
        compiler_invocations: generation.compiler_invocations,
        generator_invocations: generation.generator_invocations,
        source_bytes_read: generation.source_bytes_read,
        validation_us: provider.validation_us.load(Ordering::Relaxed),
        decode_us: provider.decode_us.load(Ordering::Relaxed),
        publication_us: provider.publication_us.load(Ordering::Relaxed),
        typed_validation_sha256,
    })
}

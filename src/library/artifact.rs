//! Canonical checked-in snapshot access and explicit generation boundary.

#[cfg(test)]
use super::compiler::CompiledLibrary;
#[cfg(test)]
use super::compiler::LibraryCompiler;
use super::compiler::{LibrarySemanticIdentity, COMPILER_SCHEMA_SHA256};
#[cfg(test)]
use super::profile::ExactLibraryProfile;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::fmt;
#[cfg(test)]
use std::path::PathBuf;

pub const CANONICAL_SNAPSHOT_BYTES: usize = 21_003_926;
pub const CANONICAL_SNAPSHOT_SHA256: &str =
    "47a8a6fd349f3b3fbb3aae1baccedbc67530edc35227707d79afac5395ca7d2f";
const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const DIAGNOSTICS_IDENTITY: &str =
    "34cc5c2c11c7d4199dd1230f88691e97b5afb09f319153d9e56d1a45f3220c86";
const INCOMPLETES_IDENTITY: &str =
    "8c268088f8afd8048690584008c40a49cd3337b91f345b2e879d625525ccf6d8";
const LEDGER_IDENTITY: &str = "2837d5a3f65e9e8fa86459ff0784fd795069a7201d17793eecd4b6bc8c14e069";
static PACKAGED_BYTES: [u8; CANONICAL_SNAPSHOT_BYTES] =
    *include_bytes!("typescript-6.0.3/canonical.snapshot");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotBinding {
    profile_identity: String,
    compiler_schema: String,
    semantic_identity: LibrarySemanticIdentity,
}

impl SnapshotBinding {
    fn canonical() -> Self {
        Self {
            profile_identity: PROFILE_IDENTITY.to_owned(),
            compiler_schema: COMPILER_SCHEMA_SHA256.to_owned(),
            semantic_identity: canonical_semantic_identity(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_profile_identity_for_test(&self, identity: &str) -> Self {
        let mut changed = self.clone();
        changed.profile_identity = identity.to_owned();
        changed
    }

    #[cfg(test)]
    pub(crate) fn with_compiler_schema_for_test(&self, identity: &str) -> Self {
        let mut changed = self.clone();
        changed.compiler_schema = identity.to_owned();
        changed
    }
}

#[derive(Clone, Debug)]
#[cfg(test)]
pub(crate) struct GeneratedCanonicalSnapshot {
    bytes: Vec<u8>,
    sha256: String,
}

#[cfg(test)]
impl GeneratedCanonicalSnapshot {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Clone, Debug)]
pub struct PackagedCanonicalSnapshot {
    binding: SnapshotBinding,
}

impl PackagedCanonicalSnapshot {
    pub const fn bytes(&self) -> &'static [u8] {
        &PACKAGED_BYTES
    }

    pub const fn sha256(&self) -> &'static str {
        CANONICAL_SNAPSHOT_SHA256
    }

    pub fn binding(&self) -> SnapshotBinding {
        self.binding.clone()
    }

    pub const fn semantic_identity(&self) -> &LibrarySemanticIdentity {
        &self.binding.semantic_identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedCanonicalSnapshot {
    bytes: usize,
    sha256: String,
    binding: SnapshotBinding,
}

impl VerifiedCanonicalSnapshot {
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn binding(&self) -> SnapshotBinding {
        self.binding.clone()
    }
}

pub(crate) struct AdmittedCanonicalSnapshot {
    bytes: Cow<'static, [u8]>,
}

impl AdmittedCanonicalSnapshot {
    pub(crate) fn into_bytes(self) -> Cow<'static, [u8]> {
        self.bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotVerificationError {
    ProfileIdentityMismatch {
        expected: String,
        actual: String,
    },
    CompilerSchemaMismatch {
        expected: String,
        actual: String,
    },
    ArtifactIdentityMismatch {
        expected_bytes: usize,
        actual_bytes: usize,
        expected_sha256: String,
        actual_sha256: String,
    },
    Generation(String),
    Io(String),
}

impl fmt::Display for SnapshotVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProfileIdentityMismatch { expected, actual } => {
                write!(
                    formatter,
                    "snapshot profile identity {actual} differs from {expected}"
                )
            }
            Self::CompilerSchemaMismatch { expected, actual } => {
                write!(
                    formatter,
                    "snapshot compiler schema {actual} differs from {expected}"
                )
            }
            Self::ArtifactIdentityMismatch { .. } => {
                formatter.write_str("canonical snapshot artifact identity differs")
            }
            Self::Generation(message) => write!(formatter, "snapshot generation failed: {message}"),
            Self::Io(message) => write!(formatter, "snapshot I/O failed: {message}"),
        }
    }
}

impl std::error::Error for SnapshotVerificationError {}

pub fn packaged_canonical_snapshot() -> PackagedCanonicalSnapshot {
    PackagedCanonicalSnapshot {
        binding: SnapshotBinding::canonical(),
    }
}

#[cfg(test)]
pub(crate) fn generate_canonical_snapshot(
    compiled: &CompiledLibrary,
) -> Result<GeneratedCanonicalSnapshot, SnapshotVerificationError> {
    record_generator_invocation();
    let bytes = crate::check::checker::generate_library_snapshot_archive(
        &compiled.runtime_projection()._runtime,
    )
    .map_err(SnapshotVerificationError::Generation)?;
    let sha256 = digest(&bytes);
    Ok(GeneratedCanonicalSnapshot { bytes, sha256 })
}

pub fn verify_canonical_snapshot(
    bytes: &[u8],
    binding: SnapshotBinding,
) -> Result<VerifiedCanonicalSnapshot, SnapshotVerificationError> {
    if binding.profile_identity != PROFILE_IDENTITY {
        return Err(SnapshotVerificationError::ProfileIdentityMismatch {
            expected: PROFILE_IDENTITY.to_owned(),
            actual: binding.profile_identity.clone(),
        });
    }
    if binding.compiler_schema != COMPILER_SCHEMA_SHA256 {
        return Err(SnapshotVerificationError::CompilerSchemaMismatch {
            expected: COMPILER_SCHEMA_SHA256.to_owned(),
            actual: binding.compiler_schema.clone(),
        });
    }
    let actual_sha256 = digest(bytes);
    if bytes.len() != CANONICAL_SNAPSHOT_BYTES || actual_sha256 != CANONICAL_SNAPSHOT_SHA256 {
        return Err(SnapshotVerificationError::ArtifactIdentityMismatch {
            expected_bytes: CANONICAL_SNAPSHOT_BYTES,
            actual_bytes: bytes.len(),
            expected_sha256: CANONICAL_SNAPSHOT_SHA256.to_owned(),
            actual_sha256,
        });
    }
    Ok(VerifiedCanonicalSnapshot {
        bytes: bytes.len(),
        sha256: actual_sha256,
        binding,
    })
}

pub(crate) fn verify_and_admit_canonical_snapshot(
    bytes: Cow<'static, [u8]>,
    binding: SnapshotBinding,
) -> Result<AdmittedCanonicalSnapshot, SnapshotVerificationError> {
    verify_canonical_snapshot(bytes.as_ref(), binding)?;
    Ok(AdmittedCanonicalSnapshot { bytes })
}

fn canonical_semantic_identity() -> LibrarySemanticIdentity {
    LibrarySemanticIdentity::from_parts(
        aggregate_identity(
            b"typokat-library-runtime-v1",
            [PROFILE_IDENTITY, COMPILER_SCHEMA_SHA256],
        ),
        aggregate_identity(
            b"typokat-library-evidence-v1",
            [DIAGNOSTICS_IDENTITY, INCOMPLETES_IDENTITY, LEDGER_IDENTITY],
        ),
    )
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GenerationMeasurement {
    pub(crate) compiler_invocations: u64,
    pub(crate) generator_invocations: u64,
    pub(crate) source_bytes_read: u64,
}

#[cfg(test)]
thread_local! {
    static GENERATION_INVOCATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) struct GenerationMeasurementScope {
    compiler: u64,
    generators: u64,
    source_bytes: u64,
}

#[cfg(test)]
pub(crate) fn measure_generation_for_test() -> GenerationMeasurementScope {
    let (compiler, source_bytes) = super::compiler::compiler_measurement_for_test();
    GenerationMeasurementScope {
        compiler,
        generators: GENERATION_INVOCATIONS.get(),
        source_bytes,
    }
}

#[cfg(test)]
impl GenerationMeasurementScope {
    pub(crate) fn finish(self) -> GenerationMeasurement {
        let (compiler, source_bytes) = super::compiler::compiler_measurement_for_test();
        GenerationMeasurement {
            compiler_invocations: compiler - self.compiler,
            generator_invocations: GENERATION_INVOCATIONS.get() - self.generators,
            source_bytes_read: source_bytes - self.source_bytes,
        }
    }
}

#[cfg(test)]
fn record_generator_invocation() {
    GENERATION_INVOCATIONS.set(GENERATION_INVOCATIONS.get().saturating_add(1));
}

#[cfg(test)]
#[test]
#[ignore = "explicit developer/CI snapshot generation command"]
fn generate_packaged_snapshot_for_tooling() -> Result<(), SnapshotVerificationError> {
    let path = std::env::var_os("TYPOKAT_LIBRARY_SNAPSHOT_OUTPUT")
        .map(PathBuf::from)
        .ok_or_else(|| SnapshotVerificationError::Io("missing output path".to_owned()))?;
    let profile = ExactLibraryProfile::load_packaged()
        .map_err(|error| SnapshotVerificationError::Generation(error.to_string()))?;
    let compiled = LibraryCompiler::new()
        .compile(&profile)
        .map_err(|error| SnapshotVerificationError::Generation(error.to_string()))?;
    let measurement = measure_generation_for_test();
    let generated = generate_canonical_snapshot(&compiled)?;
    let observed = measurement.finish();
    if observed
        != (GenerationMeasurement {
            compiler_invocations: 0,
            generator_invocations: 1,
            source_bytes_read: 0,
        })
    {
        return Err(SnapshotVerificationError::Generation(format!(
            "generator crossed the compiler boundary: {observed:?}"
        )));
    }
    std::fs::write(path, generated.bytes())
        .map_err(|error| SnapshotVerificationError::Io(error.to_string()))?;
    Ok(())
}

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
#[allow(
    clippy::enum_variant_names,
    reason = "the contract names the independently authenticated mismatch domains"
)]
pub enum CheckpointAuthenticationViolation {
    BinderDigestMismatch,
    RootDigestMismatch,
    PrefixMismatch,
    RetainedScopeMapsMismatch,
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
    CheckpointAuthenticationRejected {
        violation: CheckpointAuthenticationViolation,
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
    result: OnceLock<Result<Arc<InitializedLibrary>, Arc<LibraryInitError>>>,
    input: ProviderInput,
    attempts: AtomicU64,
    publications: AtomicU64,
    validation_us: AtomicU64,
    decode_us: AtomicU64,
    publication_us: AtomicU64,
}

struct InitializedLibrary {
    base: Arc<FrozenLibraryBase>,
    checkpoint_evidence:
        crate::check::checker::library_snapshot_codec::AdmittedLibrarySnapshotEvidence,
    #[cfg(test)]
    typed_validation_sha256: [u8; 32],
}

#[doc(hidden)]
pub struct LibraryProjectBinderContinuation {
    library_unit_count: usize,
    bound: crate::check::checker::BoundProjectBinder,
}

impl LibraryProjectBinderContinuation {
    #[doc(hidden)]
    pub const fn library_unit_count(&self) -> usize {
        self.library_unit_count
    }

    #[doc(hidden)]
    pub fn project_unit_count(&self) -> usize {
        self.bound.project_sources.len()
    }

    #[doc(hidden)]
    pub fn project_source_kind(
        &self,
        path: &str,
    ) -> Option<crate::binder::namespace::SourceFileKind> {
        self.bound
            .project_sources
            .iter()
            .find(|row| row.normalized_path == path)
            .map(|row| row.source_file_kind)
    }

    #[doc(hidden)]
    pub fn project_source_is_external(&self, path: &str) -> Option<bool> {
        self.bound
            .project_sources
            .iter()
            .find(|row| row.normalized_path == path)
            .map(|row| row.external_module)
    }

    #[cfg(test)]
    pub(crate) fn project_sources_for_test(
        &self,
    ) -> &[crate::check::checker::ProjectSourceBindingRow] {
        &self.bound.project_sources
    }

    #[cfg(test)]
    pub(crate) fn normalized_per_path_binding_shape_for_test(&self) -> Vec<String> {
        self.bound
            .normalized
            .normalized_per_path_binding_shape
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn normalized_import_export_shape_for_test(&self) -> Vec<String> {
        self.bound.normalized.normalized_import_export_shape.clone()
    }

    #[cfg(test)]
    pub(crate) fn normalized_namespace_shape_for_test(&self) -> Vec<String> {
        self.bound.normalized.normalized_namespace_shape.clone()
    }
}

impl LibraryBaseProvider {
    pub const fn new() -> Self {
        Self {
            result: OnceLock::new(),
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
            input,
            attempts: AtomicU64::new(0),
            publications: AtomicU64::new(0),
            validation_us: AtomicU64::new(0),
            decode_us: AtomicU64::new(0),
            publication_us: AtomicU64::new(0),
        }
    }

    pub fn get(&self) -> Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>> {
        self.initialized()
            .map(|initialized| initialized.base.clone())
    }

    #[doc(hidden)]
    pub fn authenticate_library_binder_checkpoint(
        &self,
        checkpoint: crate::binder::bind::UnauthenticatedLibraryBinderCheckpoint,
    ) -> Result<crate::binder::bind::AuthenticatedLibraryBinderCheckpoint, LibraryInitError> {
        let initialized = self.initialized().map_err(|error| error.as_ref().clone())?;
        let evidence = &initialized.checkpoint_evidence;
        checkpoint
            .authenticate(
                evidence
                    .section_digest(3)
                    .expect("canonical snapshot has a binder section"),
                evidence
                    .section_digest(9)
                    .expect("canonical snapshot has a root section"),
                evidence.library_prefixes(),
                evidence.retained_scope_maps_sha256(),
                evidence.library_units(),
            )
            .map_err(checkpoint_authentication_error)
    }

    #[doc(hidden)]
    pub fn continue_authenticated_library_project_binder(
        &self,
        checkpoint: crate::binder::bind::AuthenticatedLibraryBinderCheckpoint,
        inputs: Vec<crate::driver::FileInput>,
    ) -> Result<LibraryProjectBinderContinuation, LibraryInitError> {
        let library_unit_count = checkpoint.library_unit_count();
        let bound =
            crate::check::checker::library_compiler::continue_authenticated_library_project_binder(
                checkpoint, inputs,
            )
            .map_err(|_| {
                LibraryInitError::new(
                    LibraryInitStage::ReferenceValidation,
                    LibraryInitCause::SnapshotRejected {
                        violation: LibrarySnapshotViolation::InvalidReference,
                    },
                )
            })?;
        Ok(LibraryProjectBinderContinuation {
            library_unit_count,
            bound,
        })
    }

    #[cfg(test)]
    pub(super) fn continue_authenticated_library_binder_checkpoint_for_test<F>(
        &self,
        inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
        mutation: Option<super::base::CheckpointAuthenticationMutationForTest>,
        inspect: F,
    ) -> Result<super::base::AuthenticatedBinderContinuationReceiptForTest, LibraryInitError>
    where
        F: FnOnce(
            &crate::binder::bind::LibraryBinderCheckpointInspectionForTest<'_>,
            &crate::check::checker::library_snapshot_codec::AdmittedLibrarySnapshotEvidence,
            &crate::check::checker::replay_index::AdmittedCollisionReplayIndex,
        ),
    {
        let profile = super::profile::ExactLibraryProfile::load_packaged().map_err(|_| {
            LibraryInitError::new(
                LibraryInitStage::ReferenceValidation,
                LibraryInitCause::SnapshotRejected {
                    violation: LibrarySnapshotViolation::InvalidReference,
                },
            )
        })?;
        let mut checkpoint = super::compiler::LibraryCompiler::new()
            .compile_binder_checkpoint(&profile)
            .map_err(|_| {
                LibraryInitError::new(
                    LibraryInitStage::ReferenceValidation,
                    LibraryInitCause::SnapshotRejected {
                        violation: LibrarySnapshotViolation::InvalidReference,
                    },
                )
            })?;
        let initialized = self.initialized().map_err(|error| error.as_ref().clone())?;
        let evidence = &initialized.checkpoint_evidence;
        let base = &initialized.base;
        match mutation {
            Some(super::base::CheckpointAuthenticationMutationForTest::BinderDigest) => {
                checkpoint.mutate_binder_digest_for_test();
            }
            Some(super::base::CheckpointAuthenticationMutationForTest::RootDigest) => {
                checkpoint.mutate_root_digest_for_test();
            }
            Some(super::base::CheckpointAuthenticationMutationForTest::PrefixNextIds) => {
                checkpoint.mutate_prefix_for_test();
            }
            Some(super::base::CheckpointAuthenticationMutationForTest::FunctionScopes) => {
                checkpoint.mutate_function_scopes_for_test();
            }
            Some(super::base::CheckpointAuthenticationMutationForTest::FunctionDeclarationIds) => {
                checkpoint.mutate_function_declaration_ids_for_test();
            }
            Some(super::base::CheckpointAuthenticationMutationForTest::BlockScopes) => {
                checkpoint.mutate_block_scopes_for_test();
            }
            None => {}
        }
        let authenticated = checkpoint
            .authenticate(
                evidence
                    .section_digest(3)
                    .expect("canonical snapshot has a binder section"),
                evidence
                    .section_digest(9)
                    .expect("canonical snapshot has a root section"),
                evidence.library_prefixes(),
                evidence.retained_scope_maps_sha256(),
                evidence.library_units(),
            )
            .map_err(checkpoint_authentication_error)?;
        let inspection = authenticated.inspection_for_test();
        let checkpoint_ends = inspection.ends;
        let array_symbol = inspection.array_symbol;
        let array_type_group = inspection.array_type_group;
        inspect(&inspection, evidence, base.replay_index_for_test());
        let compiler_inputs = inputs
            .iter()
            .map(|input| crate::driver::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        let bound =
            crate::check::checker::library_compiler::continue_authenticated_library_project_binder(
                authenticated,
                compiler_inputs,
            )
            .map_err(|_| {
                LibraryInitError::new(
                    LibraryInitStage::ReferenceValidation,
                    LibraryInitCause::SnapshotRejected {
                        violation: LibrarySnapshotViolation::InvalidReference,
                    },
                )
            })?;
        let continuation = crate::check::checker::library_compiler::continuation_receipt_for_test(
            checkpoint_ends,
            array_symbol,
            array_type_group,
            bound,
        )
        .map_err(|_| {
            LibraryInitError::new(
                LibraryInitStage::ReferenceValidation,
                LibraryInitCause::SnapshotRejected {
                    violation: LibrarySnapshotViolation::InvalidReference,
                },
            )
        })?;
        let modules = evidence.library_units();
        let mapped_owner_sites = base
            .replay_index_for_test()
            .owner_sites
            .iter()
            .map(|site| super::base::MappedReplayOwnerSiteForTest {
                owner: site.owner,
                file_ordinal: site.file_ordinal,
                span: site.span,
                syntax_module: modules[site.file_ordinal.index()].module,
            })
            .collect();
        Ok(super::base::AuthenticatedBinderContinuationReceiptForTest {
            continuation,
            mapped_owner_sites,
        })
    }

    fn initialized(&self) -> Result<Arc<InitializedLibrary>, Arc<LibraryInitError>> {
        self.result.get_or_init(|| self.initialize()).clone()
    }

    fn initialize(&self) -> Result<Arc<InitializedLibrary>, Arc<LibraryInitError>> {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        let (decoded, checkpoint_evidence) = match &self.input {
            ProviderInput::Packaged => {
                let validation = Instant::now();
                let admitted = snapshot::admit_packaged_canonical().map_err(Arc::new)?;
                self.validation_us
                    .store(elapsed_us(validation), Ordering::Relaxed);
                let decode = Instant::now();
                let decoded = snapshot::decode_admitted_canonical_with_evidence(admitted)
                    .map_err(Arc::new)?;
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
                let decoded = snapshot::decode_admitted_canonical_with_evidence(admitted)
                    .map_err(Arc::new)?;
                self.decode_us.store(elapsed_us(decode), Ordering::Relaxed);
                decoded
            }
            #[cfg(test)]
            ProviderInput::PreAdmitted(snapshot) => {
                let validation = Instant::now();
                let decoded =
                    snapshot::decode_pre_admitted_with_evidence(snapshot).map_err(Arc::new)?;
                self.validation_us
                    .store(elapsed_us(validation), Ordering::Relaxed);
                self.decode_us.store(1, Ordering::Relaxed);
                decoded
            }
        };
        let publication = Instant::now();
        #[cfg(test)]
        let typed_validation_sha256 = decoded.typed_validation_sha256;
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
        Ok(Arc::new(InitializedLibrary {
            base,
            checkpoint_evidence,
            #[cfg(test)]
            typed_validation_sha256,
        }))
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
        let initialized = self.initialized().map_err(|error| error.as_ref().clone())?;
        let digest = &initialized.typed_validation_sha256;
        Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
    }
}

impl Default for LibraryBaseProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn checkpoint_authentication_error(
    mismatch: crate::binder::bind::LibraryBinderCheckpointMismatch,
) -> LibraryInitError {
    let violation = match mismatch {
        crate::binder::bind::LibraryBinderCheckpointMismatch::BinderDigest => {
            CheckpointAuthenticationViolation::BinderDigestMismatch
        }
        crate::binder::bind::LibraryBinderCheckpointMismatch::RootDigest => {
            CheckpointAuthenticationViolation::RootDigestMismatch
        }
        crate::binder::bind::LibraryBinderCheckpointMismatch::Prefix => {
            CheckpointAuthenticationViolation::PrefixMismatch
        }
        crate::binder::bind::LibraryBinderCheckpointMismatch::RetainedScopeMaps => {
            CheckpointAuthenticationViolation::RetainedScopeMapsMismatch
        }
    };
    LibraryInitError::new(
        LibraryInitStage::ReferenceValidation,
        LibraryInitCause::CheckpointAuthenticationRejected { violation },
    )
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

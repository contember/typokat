//! Instance-scoped, failure-caching publication of the source-compiled library base.

use super::base::FrozenLibraryBase;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryInitStage {
    ProfileLoad,
    Compile,
    CollisionReplayIndexAdmission,
    Publication,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollisionReplayIndexViolation {
    ManifestIdentityMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryInitCause {
    ProfileRejected {
        message: String,
    },
    CompilationFailed {
        message: String,
    },
    IncompletePublication {
        message: String,
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
            "default library initialization failed at {:?}: {:?}",
            self.stage, self.cause
        )
    }
}

impl std::error::Error for LibraryInitError {}

pub struct LibraryBaseProvider {
    result: OnceLock<Result<Arc<InitializedLibrary>, Arc<LibraryInitError>>>,
    attempts: AtomicU64,
    publications: AtomicU64,
    compile_us: AtomicU64,
    publication_us: AtomicU64,
}

struct InitializedLibrary {
    base: Arc<FrozenLibraryBase>,
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
            attempts: AtomicU64::new(0),
            publications: AtomicU64::new(0),
            compile_us: AtomicU64::new(0),
            publication_us: AtomicU64::new(0),
        }
    }

    pub fn get(&self) -> Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>> {
        self.initialized()
            .map(|initialized| initialized.base.clone())
    }

    /// Continue a compiled library binder checkpoint through the project's own files.
    ///
    /// Provenance is structural, not cryptographic: `LibraryBinderCheckpoint` is opaque and
    /// only `LibraryCompiler::compile_binder_checkpoint` can produce the value passed here.
    #[doc(hidden)]
    pub fn continue_library_project_binder(
        &self,
        checkpoint: crate::binder::bind::LibraryBinderCheckpoint,
        inputs: Vec<crate::driver::FileInput>,
    ) -> Result<LibraryProjectBinderContinuation, LibraryInitError> {
        let library_unit_count = checkpoint.library_unit_count();
        let bound = crate::check::checker::library_compiler::continue_library_project_binder(
            checkpoint, inputs,
        )
        .map_err(compilation_failed)?;
        Ok(LibraryProjectBinderContinuation {
            library_unit_count,
            bound,
        })
    }

    #[cfg(test)]
    pub(super) fn continue_library_binder_checkpoint_for_test<F>(
        &self,
        inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
        inspect: F,
    ) -> Result<super::base::LibraryBinderContinuationReceiptForTest, LibraryInitError>
    where
        F: FnOnce(
            &crate::binder::bind::LibraryBinderCheckpointInspectionForTest<'_>,
            &crate::check::checker::replay_index::AdmittedCollisionReplayIndex,
        ),
    {
        let profile = super::profile::ExactLibraryProfile::load_packaged().map_err(|error| {
            LibraryInitError::new(
                LibraryInitStage::ProfileLoad,
                LibraryInitCause::ProfileRejected {
                    message: error.to_string(),
                },
            )
        })?;
        let checkpoint = super::compiler::LibraryCompiler::new()
            .compile_binder_checkpoint(&profile)
            .map_err(|error| compilation_failed(error.to_string()))?;
        let initialized = self.initialized().map_err(|error| error.as_ref().clone())?;
        let base = &initialized.base;
        let inspection = checkpoint.inspection_for_test();
        let checkpoint_ends = inspection.ends;
        let array_symbol = inspection.array_symbol;
        let array_type_group = inspection.array_type_group;
        let library_modules = inspection
            .library_units
            .iter()
            .map(|unit| unit.module)
            .collect::<Vec<_>>();
        inspect(&inspection, base.replay_index_for_test());
        let compiler_inputs = inputs
            .iter()
            .map(|input| crate::driver::FileInput {
                name: input.path.to_owned(),
                source: input.source.to_owned(),
            })
            .collect::<Vec<_>>();
        let bound = crate::check::checker::library_compiler::continue_library_project_binder(
            checkpoint,
            compiler_inputs,
        )
        .map_err(compilation_failed)?;
        let continuation = crate::check::checker::library_compiler::continuation_receipt_for_test(
            checkpoint_ends,
            array_symbol,
            array_type_group,
            bound,
        )
        .map_err(compilation_failed)?;
        let mapped_owner_sites = base
            .replay_index_for_test()
            .owner_sites
            .iter()
            .map(|site| super::base::MappedReplayOwnerSiteForTest {
                owner: site.owner,
                file_ordinal: site.file_ordinal,
                span: site.span,
                syntax_module: library_modules[site.file_ordinal.index()],
            })
            .collect();
        Ok(super::base::LibraryBinderContinuationReceiptForTest {
            continuation,
            mapped_owner_sites,
        })
    }

    fn initialized(&self) -> Result<Arc<InitializedLibrary>, Arc<LibraryInitError>> {
        self.result.get_or_init(|| self.initialize()).clone()
    }

    fn initialize(&self) -> Result<Arc<InitializedLibrary>, Arc<LibraryInitError>> {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        let compile = Instant::now();
        let compiled = FrozenLibraryBase::compile_packaged_profile().map_err(Arc::new)?;
        self.compile_us
            .store(elapsed_us(compile), Ordering::Relaxed);
        let publication = Instant::now();
        let base = Arc::new(FrozenLibraryBase::publish(compiled).map_err(Arc::new)?);
        self.publication_us
            .store(elapsed_us(publication), Ordering::Relaxed);
        self.publications.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(InitializedLibrary { base }))
    }

    #[cfg(test)]
    pub(super) fn measurement_for_test(&self) -> InitializationMeasurement {
        InitializationMeasurement {
            attempts: self.attempts.load(Ordering::Relaxed),
            publications: self.publications.load(Ordering::Relaxed),
        }
    }
}

impl Default for LibraryBaseProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn compilation_failed(message: String) -> LibraryInitError {
    LibraryInitError::new(
        LibraryInitStage::Compile,
        LibraryInitCause::CompilationFailed { message },
    )
}

fn elapsed_us(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros())
        .unwrap_or(u64::MAX)
        .max(1)
}

/// The process-wide default-library base shared by every spec that needs one.
///
/// Source compilation of the 82 packaged declaration files is the only route to a base, so the
/// suite pays for it once per test binary instead of once per test.
#[cfg(test)]
pub(super) fn shared_library_base_provider_for_test() -> &'static LibraryBaseProvider {
    static SHARED: std::sync::LazyLock<LibraryBaseProvider> =
        std::sync::LazyLock::new(LibraryBaseProvider::new);
    &SHARED
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InitializationMeasurement {
    pub(super) attempts: u64,
    pub(super) publications: u64,
}

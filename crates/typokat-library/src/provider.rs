//! Instance-scoped, failure-caching publication of the immutable declaration product.

use std::fmt;
use std::sync::{Arc, OnceLock};

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "project-binder-probe")]
thread_local! {
    static PROBE_DECLARATION_PRODUCT_INITIALIZATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(feature = "project-binder-probe")]
#[derive(Clone, Copy)]
pub(crate) struct ProjectBinderInitializationWorkForProbe {
    pub(crate) declaration_products: u64,
}

#[cfg(feature = "project-binder-probe")]
pub(crate) struct ProjectBinderInitializationScopeForProbe(ProjectBinderInitializationWorkForProbe);

#[cfg(feature = "project-binder-probe")]
impl ProjectBinderInitializationScopeForProbe {
    pub(crate) fn start() -> Self {
        Self(project_binder_initialization_work_for_probe())
    }

    pub(crate) fn finish(self) -> ProjectBinderInitializationWorkForProbe {
        let end = project_binder_initialization_work_for_probe();
        ProjectBinderInitializationWorkForProbe {
            declaration_products: end
                .declaration_products
                .saturating_sub(self.0.declaration_products),
        }
    }
}

#[cfg(feature = "project-binder-probe")]
fn project_binder_initialization_work_for_probe() -> ProjectBinderInitializationWorkForProbe {
    ProjectBinderInitializationWorkForProbe {
        declaration_products: PROBE_DECLARATION_PRODUCT_INITIALIZATIONS.get(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryInitStage {
    ProfileLoad,
    Compile,
    Publication,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryInitCause {
    ProfileRejected { message: String },
    CompilationFailed { message: String },
    IncompletePublication { message: String },
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

    #[doc(hidden)]
    pub fn injected_initialization_failure(message: String) -> Self {
        Self::new(
            LibraryInitStage::ProfileLoad,
            LibraryInitCause::ProfileRejected { message },
        )
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

type ProductResult =
    Result<Arc<typokat_library_product::LibraryDeclarationProduct>, Arc<LibraryInitError>>;

/// Publishes exactly one immutable declaration product result per provider instance.
///
/// Both success and deterministic failure are cached. Concurrent callers therefore observe the
/// same `Arc`, and initialization is never retried after a typed failure.
pub struct LibraryBaseProvider {
    result: OnceLock<ProductResult>,
    #[cfg(test)]
    injected_failure: Option<Arc<LibraryInitError>>,
    #[cfg(test)]
    attempts: AtomicU64,
    #[cfg(test)]
    publications: AtomicU64,
}

impl LibraryBaseProvider {
    pub const fn new() -> Self {
        Self {
            result: OnceLock::new(),
            #[cfg(test)]
            injected_failure: None,
            #[cfg(test)]
            attempts: AtomicU64::new(0),
            #[cfg(test)]
            publications: AtomicU64::new(0),
        }
    }

    pub fn get(
        &self,
    ) -> Result<Arc<typokat_library_product::LibraryDeclarationProduct>, Arc<LibraryInitError>>
    {
        self.result.get_or_init(|| self.initialize()).clone()
    }

    fn initialize(&self) -> ProductResult {
        #[cfg(feature = "project-binder-probe")]
        PROBE_DECLARATION_PRODUCT_INITIALIZATIONS.set(
            PROBE_DECLARATION_PRODUCT_INITIALIZATIONS
                .get()
                .saturating_add(1),
        );
        #[cfg(test)]
        self.attempts.fetch_add(1, Ordering::Relaxed);

        #[cfg(test)]
        if let Some(error) = &self.injected_failure {
            return Err(Arc::clone(error));
        }

        let product = super::declaration_product::build_packaged_declaration_product()
            .map_err(|error| Arc::new(declaration_product_failed(error)))?;

        #[cfg(test)]
        self.publications.fetch_add(1, Ordering::Relaxed);
        Ok(product)
    }

    #[cfg(test)]
    fn with_failure_for_test(error: LibraryInitError) -> Self {
        Self {
            result: OnceLock::new(),
            injected_failure: Some(Arc::new(error)),
            attempts: AtomicU64::new(0),
            publications: AtomicU64::new(0),
        }
    }

    #[cfg(test)]
    fn measurement_for_test(&self) -> InitializationMeasurement {
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

pub(crate) fn declaration_product_failed(
    error: super::declaration_product::LibraryDeclarationProductBuildError,
) -> LibraryInitError {
    let stage = match &error {
        super::declaration_product::LibraryDeclarationProductBuildError::Profile(_) => {
            LibraryInitStage::ProfileLoad
        }
        _ => LibraryInitStage::Compile,
    };
    let cause = match stage {
        LibraryInitStage::ProfileLoad => LibraryInitCause::ProfileRejected {
            message: error.to_string(),
        },
        LibraryInitStage::Compile | LibraryInitStage::Publication => {
            LibraryInitCause::CompilationFailed {
                message: error.to_string(),
            }
        }
    };
    LibraryInitError::new(stage, cause)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InitializationMeasurement {
    attempts: u64,
    publications: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::thread;

    #[test]
    fn provider_caches_one_pointer_identical_product() {
        let provider = LibraryBaseProvider::new();
        let first = provider.get().expect("packaged product");
        let second = provider.get().expect("cached packaged product");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(
            provider.measurement_for_test(),
            InitializationMeasurement {
                attempts: 1,
                publications: 1,
            }
        );
    }

    #[test]
    fn provider_caches_the_same_deterministic_error_arc_across_threads() {
        let provider = Arc::new(LibraryBaseProvider::with_failure_for_test(
            LibraryInitError::injected_initialization_failure("provider fault".to_owned()),
        ));
        let barrier = Arc::new(Barrier::new(9));
        let workers = (0..8)
            .map(|_| {
                let provider = Arc::clone(&provider);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    match provider.get() {
                        Ok(_) => panic!("injected failure unexpectedly initialized a product"),
                        Err(error) => error,
                    }
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let errors = workers
            .into_iter()
            .map(|worker| worker.join().expect("provider worker"))
            .collect::<Vec<_>>();

        assert!(errors.iter().all(|error| Arc::ptr_eq(error, &errors[0])));
        assert_eq!(errors[0].stage(), LibraryInitStage::ProfileLoad);
        assert_eq!(
            provider.measurement_for_test(),
            InitializationMeasurement {
                attempts: 1,
                publications: 0,
            }
        );
    }
}

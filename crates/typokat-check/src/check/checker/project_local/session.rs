//! Project and publisher lifetime ownership.

use super::cache::DurableCache;
use super::entry::run_ordinary_project_local_semantics;
#[cfg(feature = "session-bench")]
use super::entry::run_ordinary_project_local_semantics_for_session_bench;
#[cfg(any(test, feature = "test-utils"))]
use super::observation;
use super::project_local_pass_port::Interner;
#[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
use crate::check::checker::bind_library_product_project_programs_unadmitted;
use crate::check::checker::{
    bind_library_product_project_programs, CheckResult, LibraryProductProjectBindingError,
};
use crate::frontend::{AdmittedSourceReexports, DefaultModuleCandidates, ProjectProgram};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
#[cfg(feature = "session-bench")]
use std::time::Instant;
use typokat_library_product::LibraryDeclarationProduct;

static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

fn next_identity() -> u64 {
    NEXT_IDENTITY
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |identity| {
            identity.checked_add(1)
        })
        .expect("project-local identity overflow")
}

/// One project-local check against one immutable library declaration product.
pub struct CheckerSession {
    product: Arc<LibraryDeclarationProduct>,
}

impl CheckerSession {
    pub fn new(product: Arc<LibraryDeclarationProduct>) -> Self {
        Self { product }
    }

    pub fn check_project_programs<'ast>(
        self,
        programs: &[ProjectProgram<'ast>],
        source_reexports: &AdmittedSourceReexports,
        default_modules: &DefaultModuleCandidates,
    ) -> Result<Vec<CheckResult>, CheckerSessionError> {
        let (library_units, bound) = bind_library_product_project_programs(
            &self.product,
            programs,
            source_reexports,
            default_modules,
        )
        .map_err(CheckerSessionError::Binding)?;
        run_ordinary_project_local_semantics(&self.product, library_units, bound, programs)
            .map_err(|error| CheckerSessionError::Semantic(error.to_string()))
    }

    /// Check one benchmark frame through the ordinary binder and semantic executor.
    #[cfg(feature = "session-bench")]
    #[doc(hidden)]
    pub fn check_project_programs_for_session_bench<'ast>(
        self,
        programs: &[ProjectProgram<'ast>],
        clock: &SessionBenchClock,
    ) -> Result<SessionBenchCheckReceipt, CheckerSessionError> {
        let binding_start_ns = clock.elapsed_ns();
        let (library_units, bound) =
            bind_library_product_project_programs_unadmitted(&self.product, programs)
                .map_err(CheckerSessionError::Binding)?;
        let binding_end_ns = clock.elapsed_ns();
        let publication_start_ns = binding_end_ns;
        let semantic = run_ordinary_project_local_semantics_for_session_bench(
            &self.product,
            library_units,
            bound,
            programs,
        )
        .map_err(|error| CheckerSessionError::Semantic(error.to_string()))?;
        let publication_end_ns = clock.elapsed_ns();
        Ok(SessionBenchCheckReceipt {
            results: semantic.results,
            binding_start_ns,
            binding_end_ns,
            publication_start_ns,
            publication_end_ns,
            materialized_product_owner_count: semantic.materialized_product_owner_count,
            project_identity: semantic.project_identity,
            publisher_identity: semantic.publisher_identity,
            cache_identity: semantic.cache_identity,
            session_weak_alive_after: semantic.session_weak_alive_after,
            cache_weak_alive_after: semantic.cache_weak_alive_after,
        })
    }

    /// Check a shadow project without admitting module export products.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn check_project_programs_unadmitted_for_test<'ast>(
        self,
        programs: &[ProjectProgram<'ast>],
    ) -> Result<Vec<CheckResult>, CheckerSessionError> {
        let (library_units, bound) =
            bind_library_product_project_programs_unadmitted(&self.product, programs)
                .map_err(CheckerSessionError::Binding)?;
        run_ordinary_project_local_semantics(&self.product, library_units, bound, programs)
            .map_err(|error| CheckerSessionError::Semantic(error.to_string()))
    }
}

/// Monotonic process-local clock shared by all benchmark frame receipts.
#[cfg(feature = "session-bench")]
#[doc(hidden)]
pub struct SessionBenchClock {
    origin: Instant,
}

#[cfg(feature = "session-bench")]
impl SessionBenchClock {
    pub fn start() -> Self {
        Self {
            origin: Instant::now(),
        }
    }

    pub fn elapsed_ns(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

/// Real checker results plus lifecycle evidence for one isolated benchmark frame.
#[cfg(feature = "session-bench")]
#[doc(hidden)]
pub struct SessionBenchCheckReceipt {
    pub results: Vec<CheckResult>,
    pub binding_start_ns: u64,
    pub binding_end_ns: u64,
    pub publication_start_ns: u64,
    pub publication_end_ns: u64,
    pub materialized_product_owner_count: usize,
    pub project_identity: u64,
    pub publisher_identity: u64,
    pub cache_identity: u64,
    pub session_weak_alive_after: bool,
    pub cache_weak_alive_after: bool,
}

#[derive(Debug)]
pub enum CheckerSessionError {
    Binding(LibraryProductProjectBindingError),
    Semantic(String),
}

impl fmt::Display for CheckerSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binding(error) => error.fmt(formatter),
            Self::Semantic(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for CheckerSessionError {}

pub(super) struct SessionCore {
    project: u64,
    publisher: u64,
    #[cfg(feature = "session-bench")]
    cache_identity: u64,
    cache: Rc<DurableCache>,
    interner: RefCell<Interner>,
}

impl SessionCore {
    pub(super) fn start() -> Rc<Self> {
        let project = next_identity();
        let publisher = next_identity();
        #[cfg(feature = "session-bench")]
        let cache_identity = next_identity();
        let cache = Rc::new(DurableCache::new(publisher));
        let session = Rc::new(Self {
            project,
            publisher,
            #[cfg(feature = "session-bench")]
            cache_identity,
            cache,
            interner: RefCell::new(Interner::with_intrinsics()),
        });
        #[cfg(any(test, feature = "test-utils"))]
        {
            observation::register_session(&session, &session.cache);
            observation::record_session_created(project, publisher);
        }
        session
    }

    pub(super) const fn project_identity(&self) -> u64 {
        self.project
    }

    pub(super) const fn publisher_identity(&self) -> u64 {
        self.publisher
    }

    #[cfg(feature = "session-bench")]
    pub(super) const fn cache_identity(&self) -> u64 {
        self.cache_identity
    }

    pub(super) fn cache(&self) -> &DurableCache {
        &self.cache
    }

    #[cfg(feature = "session-bench")]
    pub(super) fn cache_weak(&self) -> std::rc::Weak<DurableCache> {
        Rc::downgrade(&self.cache)
    }

    pub(super) fn with_interner<T>(&self, operation: impl FnOnce(&mut Interner) -> T) -> T {
        operation(&mut self.interner.borrow_mut())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn receipt_built(&self) {
        observation::record_receipt_built();
    }
}

impl Drop for SessionCore {
    fn drop(&mut self) {
        #[cfg(any(test, feature = "test-utils"))]
        observation::record_session_dropped(self.publisher);
    }
}

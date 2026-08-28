//! Project and publisher lifetime ownership.

use super::cache::DurableCache;
use super::entry::run_ordinary_project_local_semantics;
use super::observation;
use super::project_local_pass_port::Interner;
#[cfg(any(test, feature = "test-utils"))]
use crate::check::checker::bind_library_product_project_programs_unadmitted_for_test;
use crate::check::checker::{
    bind_library_product_project_programs, CheckResult, LibraryProductProjectBindingError,
};
use crate::frontend::{AdmittedSourceReexports, DefaultModuleCandidates, ProjectProgram};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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

    /// Check a shadow project without admitting module export products.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn check_project_programs_unadmitted_for_test<'ast>(
        self,
        programs: &[ProjectProgram<'ast>],
    ) -> Result<Vec<CheckResult>, CheckerSessionError> {
        let (library_units, bound) =
            bind_library_product_project_programs_unadmitted_for_test(&self.product, programs)
                .map_err(CheckerSessionError::Binding)?;
        run_ordinary_project_local_semantics(&self.product, library_units, bound, programs)
            .map_err(|error| CheckerSessionError::Semantic(error.to_string()))
    }
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
    cache: Rc<DurableCache>,
    interner: RefCell<Interner>,
}

impl SessionCore {
    pub(super) fn start() -> Rc<Self> {
        let project = next_identity();
        let publisher = next_identity();
        let cache = Rc::new(DurableCache::new(publisher));
        let session = Rc::new(Self {
            project,
            publisher,
            cache,
            interner: RefCell::new(Interner::with_intrinsics()),
        });
        observation::register_session(&session, &session.cache);
        observation::record_session_created(project, publisher);
        session
    }

    pub(super) const fn project_identity(&self) -> u64 {
        self.project
    }

    pub(super) const fn publisher_identity(&self) -> u64 {
        self.publisher
    }

    pub(super) fn cache(&self) -> &DurableCache {
        &self.cache
    }

    pub(super) fn with_interner<T>(&self, operation: impl FnOnce(&mut Interner) -> T) -> T {
        operation(&mut self.interner.borrow_mut())
    }

    pub(super) fn receipt_built(&self) {
        observation::record_receipt_built();
    }
}

impl Drop for SessionCore {
    fn drop(&mut self) {
        observation::record_session_dropped(self.publisher);
    }
}

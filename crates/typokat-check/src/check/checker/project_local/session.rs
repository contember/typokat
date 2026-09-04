//! Project and publisher lifetime ownership.

use super::cache::DurableCache;
#[cfg(test)]
use super::entry::prepare_ordinary_project_local_execution_receipt_for_test;
use super::entry::run_ordinary_project_local_semantics;
#[cfg(feature = "session-bench")]
use super::entry::run_ordinary_project_local_semantics_for_session_bench;
#[cfg(any(test, feature = "test-utils"))]
use super::observation;
use super::project_local_pass_port::Interner;
use crate::check::checker::{
    bind_library_product_project_programs, CheckResult, LibraryProductProjectBindingError,
    ProjectSourceReexportPolicy,
};
#[cfg(test)]
use crate::frontend::AdmittedSourceReexports;
use crate::frontend::{DefaultModuleCandidates, ProjectProgram};
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
        source_reexports: ProjectSourceReexportPolicy<'_>,
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

    #[cfg(test)]
    pub fn prepare_project_programs_for_test<'ast>(
        self,
        programs: &[ProjectProgram<'ast>],
        source_reexports: &AdmittedSourceReexports,
        default_modules: &DefaultModuleCandidates,
    ) -> Result<super::ProjectLocalOrdinaryPreparationReceiptForTest, CheckerSessionError> {
        let (library_units, bound) = bind_library_product_project_programs(
            &self.product,
            programs,
            ProjectSourceReexportPolicy::Admit(source_reexports),
            default_modules,
        )
        .map_err(CheckerSessionError::Binding)?;
        prepare_ordinary_project_local_execution_receipt_for_test(
            &self.product,
            library_units,
            bound,
            programs,
        )
        .map_err(|error| CheckerSessionError::Semantic(error.to_string()))
    }

    /// Check one benchmark frame through the ordinary binder and semantic executor.
    #[cfg(feature = "session-bench")]
    #[doc(hidden)]
    pub fn check_project_programs_for_session_bench<'ast>(
        self,
        programs: &[ProjectProgram<'ast>],
        source_reexports: ProjectSourceReexportPolicy<'_>,
        default_modules: &DefaultModuleCandidates,
        clock: &SessionBenchClock,
    ) -> Result<SessionBenchCheckReceipt, CheckerSessionError> {
        let mut lifecycle = OrdinarySessionBenchLifecycle;
        let product = consume_session_bench_checker(self, &mut lifecycle);
        let binding_start_ns = clock.elapsed_ns();
        let (library_units, bound) = bind_library_product_project_programs(
            &product,
            programs,
            source_reexports,
            default_modules,
        )
        .map_err(CheckerSessionError::Binding)?;
        let binding_end_ns = clock.elapsed_ns();
        let publication_start_ns = binding_end_ns;
        let semantic = run_ordinary_project_local_semantics_for_session_bench(
            &product,
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

    /// Reject a benchmark-only lifecycle fault at the owned session boundary.
    #[cfg(feature = "session-bench")]
    #[doc(hidden)]
    pub fn reject_fault_for_session_bench(
        self,
        fault: SessionBenchFault,
    ) -> SessionBenchFaultReceipt {
        let mut lifecycle = FaultedSessionBenchLifecycle::new(fault);
        let product = consume_session_bench_checker(self, &mut lifecycle);
        let shared_product_identity = product.profile_identity().to_owned();
        let session = SessionCore::start();
        let project_identity = session.project_identity();
        let publisher_identity = session.publisher_identity();
        let cache_identity = session.cache_identity();
        let session_weak = Rc::downgrade(&session);
        let cache_weak = session.cache_weak_with_policy(&mut lifecycle);
        drop(session);
        lifecycle.observe_retention(cache_weak.upgrade().is_some());
        lifecycle.clear_retention();
        SessionBenchFaultReceipt {
            fault,
            seam: lifecycle
                .activated_seam()
                .expect("supported session-bench fault reaches its owning lifecycle seam"),
            fault_hook_activations: lifecycle.activations(),
            rejected: lifecycle.rejected(),
            shared_product_identity,
            project_identity,
            publisher_identity,
            cache_identity,
            session_weak_alive_after: session_weak.upgrade().is_some(),
            cache_weak_alive_after: cache_weak.upgrade().is_some(),
        }
    }
}

/// Linux monotonic clock shared by all benchmark process receipts.
#[cfg(feature = "session-bench")]
#[doc(hidden)]
pub struct SessionBenchClock;

#[cfg(feature = "session-bench")]
impl SessionBenchClock {
    pub fn start() -> Self {
        Self
    }

    pub fn elapsed_ns(&self) -> u64 {
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        u64::try_from(now.tv_sec)
            .expect("CLOCK_MONOTONIC seconds are nonnegative")
            .checked_mul(1_000_000_000)
            .and_then(|seconds| {
                seconds.checked_add(
                    u64::try_from(now.tv_nsec)
                        .expect("CLOCK_MONOTONIC nanoseconds are nonnegative"),
                )
            })
            .expect("CLOCK_MONOTONIC nanosecond timestamp overflow")
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

/// Lifecycle fault requests supported by the isolated-session benchmark.
#[cfg(feature = "session-bench")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionBenchFault {
    ReuseSession,
    RetainCache,
}

/// The owning lifecycle seam that rejects a benchmark fault.
#[cfg(feature = "session-bench")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionBenchFaultSeam {
    SessionConsume,
    CacheRelease,
}

#[cfg(feature = "session-bench")]
trait SessionBenchLifecyclePolicy {
    fn session_consumed(&mut self) {}
    fn cache_releasing(&mut self, _cache: &Rc<DurableCache>) {}
}

#[cfg(feature = "session-bench")]
struct OrdinarySessionBenchLifecycle;

#[cfg(feature = "session-bench")]
impl SessionBenchLifecyclePolicy for OrdinarySessionBenchLifecycle {}

#[cfg(feature = "session-bench")]
fn consume_session_bench_checker<Policy: SessionBenchLifecyclePolicy>(
    session: CheckerSession,
    policy: &mut Policy,
) -> Arc<LibraryDeclarationProduct> {
    policy.session_consumed();
    session.product
}

#[cfg(feature = "session-bench")]
struct FaultedSessionBenchLifecycle {
    requested: SessionBenchFault,
    activations: usize,
    activated_seam: Option<SessionBenchFaultSeam>,
    retained_cache: Option<Rc<DurableCache>>,
    retention_observed: bool,
}

#[cfg(feature = "session-bench")]
impl FaultedSessionBenchLifecycle {
    const fn new(requested: SessionBenchFault) -> Self {
        Self {
            requested,
            activations: 0,
            activated_seam: None,
            retained_cache: None,
            retention_observed: false,
        }
    }

    fn activate(&mut self, seam: SessionBenchFaultSeam) {
        self.activations = self
            .activations
            .checked_add(1)
            .expect("session-bench fault activation overflow");
        self.activated_seam = Some(seam);
    }

    const fn activations(&self) -> usize {
        self.activations
    }

    const fn activated_seam(&self) -> Option<SessionBenchFaultSeam> {
        self.activated_seam
    }

    fn observe_retention(&mut self, retained: bool) {
        self.retention_observed = retained;
    }

    fn clear_retention(&mut self) {
        self.retained_cache = None;
    }

    const fn rejected(&self) -> bool {
        match self.requested {
            SessionBenchFault::ReuseSession => self.activations == 1,
            SessionBenchFault::RetainCache => self.activations == 1 && self.retention_observed,
        }
    }
}

#[cfg(feature = "session-bench")]
impl SessionBenchLifecyclePolicy for FaultedSessionBenchLifecycle {
    fn session_consumed(&mut self) {
        if self.requested == SessionBenchFault::ReuseSession {
            self.activate(SessionBenchFaultSeam::SessionConsume);
        }
    }

    fn cache_releasing(&mut self, cache: &Rc<DurableCache>) {
        if self.requested == SessionBenchFault::RetainCache {
            self.retained_cache = Some(Rc::clone(cache));
            self.activate(SessionBenchFaultSeam::CacheRelease);
        }
    }
}

/// Authenticated rejection receipt from a real session lifecycle.
#[cfg(feature = "session-bench")]
#[doc(hidden)]
pub struct SessionBenchFaultReceipt {
    fault: SessionBenchFault,
    seam: SessionBenchFaultSeam,
    fault_hook_activations: usize,
    rejected: bool,
    shared_product_identity: String,
    project_identity: u64,
    publisher_identity: u64,
    cache_identity: u64,
    session_weak_alive_after: bool,
    cache_weak_alive_after: bool,
}

#[cfg(feature = "session-bench")]
impl SessionBenchFaultReceipt {
    pub const fn fault(&self) -> SessionBenchFault {
        self.fault
    }

    pub const fn seam(&self) -> SessionBenchFaultSeam {
        self.seam
    }

    pub const fn fault_hook_activations(&self) -> usize {
        self.fault_hook_activations
    }

    pub const fn rejected(&self) -> bool {
        self.rejected
    }

    pub fn shared_product_identity(&self) -> &str {
        &self.shared_product_identity
    }

    pub const fn project_identity(&self) -> u64 {
        self.project_identity
    }

    pub const fn publisher_identity(&self) -> u64 {
        self.publisher_identity
    }

    pub const fn cache_identity(&self) -> u64 {
        self.cache_identity
    }

    pub const fn session_weak_alive_after(&self) -> bool {
        self.session_weak_alive_after
    }

    pub const fn cache_weak_alive_after(&self) -> bool {
        self.cache_weak_alive_after
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
        self.cache_weak_with_policy(&mut OrdinarySessionBenchLifecycle)
    }

    #[cfg(feature = "session-bench")]
    fn cache_weak_with_policy<Policy: SessionBenchLifecyclePolicy>(
        &self,
        policy: &mut Policy,
    ) -> std::rc::Weak<DurableCache> {
        policy.cache_releasing(&self.cache);
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

//! Project and publisher lifetime ownership.

use super::cache::DurableCache;
use super::observation;
use super::project_local_pass_port::Interner;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

fn next_identity() -> u64 {
    NEXT_IDENTITY
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |identity| {
            identity.checked_add(1)
        })
        .expect("project-local identity overflow")
}

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

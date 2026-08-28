//! Read-only append-only candidate audit view for tests.

use super::cache::DurableCache;
use super::session::SessionCore;
use super::ProjectLocalSemanticFaultForTest;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalFaultSeamForTest {
    DependencyProjection,
    SccCompletion,
    DurableCacheCommit,
    GlobalObjectPublication,
    OrdinaryDemandDispatch,
    UnavailableRollback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalSemanticPhaseForTest {
    Ordinary,
    ForceAll,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservationEventKindForTest {
    SessionCreated {
        project: u64,
        publisher: u64,
    },
    OwnerMaterialized {
        phase: ProjectLocalSemanticPhaseForTest,
        product_group: usize,
    },
    DurableCacheWrite {
        publisher: u64,
        product_group: usize,
    },
    DurableCacheHit {
        publisher: u64,
        product_group: usize,
    },
    FaultActivated {
        fault: ProjectLocalSemanticFaultForTest,
        seam: ProjectLocalFaultSeamForTest,
    },
    ReceiptBuilt,
    SessionDropped {
        publisher: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationEventForTest {
    pub stamp: u64,
    pub kind: ObservationEventKindForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeakLivenessForTest {
    pub identity: u64,
    pub upgraded_at_finish: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationWitnessKindForTest {
    ActualWeakRegistry,
    SyntheticSchema,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationReceiptForTest {
    pub events: Vec<ObservationEventForTest>,
    pub witness_kind: ObservationWitnessKindForTest,
    /// Upgrade results from implementation-owned `Weak<SessionCore>` rows.
    pub session_weak: Vec<WeakLivenessForTest>,
    /// Upgrade results from implementation-owned `Weak<DurableCache>` rows.
    pub cache_weak: Vec<WeakLivenessForTest>,
}

thread_local! {
    static ACTIVE_SCOPE_DEPTH: Cell<usize> = const { Cell::new(0) };
    static JOURNAL: RefCell<Vec<ObservationEventForTest>> = const { RefCell::new(Vec::new()) };
    static SESSION_REGISTRY: RefCell<Vec<(u64, Weak<SessionCore>)>> = const { RefCell::new(Vec::new()) };
    static CACHE_REGISTRY: RefCell<Vec<(u64, Weak<DurableCache>)>> = const { RefCell::new(Vec::new()) };
}

pub struct ObservationScopeForTest {
    cursor: usize,
    session_cursor: usize,
    cache_cursor: usize,
    active: bool,
}

impl ObservationScopeForTest {
    pub fn start() -> Self {
        ACTIVE_SCOPE_DEPTH.with(|depth| {
            depth.set(
                depth
                    .get()
                    .checked_add(1)
                    .expect("observation scope depth overflow"),
            );
        });
        Self {
            cursor: JOURNAL.with(|journal| journal.borrow().len()),
            session_cursor: SESSION_REGISTRY.with(|registry| registry.borrow().len()),
            cache_cursor: CACHE_REGISTRY.with(|registry| registry.borrow().len()),
            active: true,
        }
    }

    pub fn finish(mut self) -> ObservationReceiptForTest {
        let receipt = ObservationReceiptForTest {
            events: JOURNAL.with(|journal| journal.borrow()[self.cursor..].to_vec()),
            witness_kind: ObservationWitnessKindForTest::ActualWeakRegistry,
            session_weak: SESSION_REGISTRY.with(|registry| {
                registry.borrow()[self.session_cursor..]
                    .iter()
                    .map(|(identity, weak)| WeakLivenessForTest {
                        identity: *identity,
                        upgraded_at_finish: weak.upgrade().is_some(),
                    })
                    .collect()
            }),
            cache_weak: CACHE_REGISTRY.with(|registry| {
                registry.borrow()[self.cache_cursor..]
                    .iter()
                    .map(|(identity, weak)| WeakLivenessForTest {
                        identity: *identity,
                        upgraded_at_finish: weak.upgrade().is_some(),
                    })
                    .collect()
            }),
        };
        self.deactivate();
        receipt
    }

    fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        let clear = ACTIVE_SCOPE_DEPTH.with(|depth| {
            let remaining = depth
                .get()
                .checked_sub(1)
                .expect("observation scope depth underflow");
            depth.set(remaining);
            remaining == 0
        });
        if clear {
            JOURNAL.with(|journal| journal.borrow_mut().clear());
            SESSION_REGISTRY.with(|registry| registry.borrow_mut().clear());
            CACHE_REGISTRY.with(|registry| registry.borrow_mut().clear());
        }
    }
}

impl Drop for ObservationScopeForTest {
    fn drop(&mut self) {
        self.deactivate();
    }
}

fn observation_active() -> bool {
    ACTIVE_SCOPE_DEPTH.with(|depth| depth.get() != 0)
}

fn append(kind: ObservationEventKindForTest) {
    if !observation_active() {
        return;
    }
    JOURNAL.with(|journal| {
        let mut journal = journal.borrow_mut();
        let stamp = journal.last().map_or(1, |event| {
            event.stamp.checked_add(1).expect("audit stamp overflow")
        });
        journal.push(ObservationEventForTest { stamp, kind });
    });
}

pub(super) fn register_session(session: &Rc<SessionCore>, cache: &Rc<DurableCache>) {
    if !observation_active() {
        return;
    }
    SESSION_REGISTRY.with(|registry| {
        registry
            .borrow_mut()
            .push((session.project_identity(), Rc::downgrade(session)));
    });
    CACHE_REGISTRY.with(|registry| {
        registry
            .borrow_mut()
            .push((session.publisher_identity(), Rc::downgrade(cache)));
    });
}

pub fn observation_storage_counts_for_test() -> (usize, usize, usize) {
    (
        JOURNAL.with(|journal| journal.borrow().len()),
        SESSION_REGISTRY.with(|registry| registry.borrow().len()),
        CACHE_REGISTRY.with(|registry| registry.borrow().len()),
    )
}

pub(super) fn record_session_created(project: u64, publisher: u64) {
    append(ObservationEventKindForTest::SessionCreated { project, publisher });
}

pub(super) fn record_owner_materialized(
    phase: ProjectLocalSemanticPhaseForTest,
    product_group: usize,
) {
    append(ObservationEventKindForTest::OwnerMaterialized {
        phase,
        product_group,
    });
}

pub(super) fn record_cache_write(publisher: u64, product_group: usize) {
    append(ObservationEventKindForTest::DurableCacheWrite {
        publisher,
        product_group,
    });
}

pub(super) fn record_cache_hit(publisher: u64, product_group: usize) {
    append(ObservationEventKindForTest::DurableCacheHit {
        publisher,
        product_group,
    });
}

pub(super) fn record_fault(
    fault: ProjectLocalSemanticFaultForTest,
    seam: ProjectLocalFaultSeamForTest,
) {
    append(ObservationEventKindForTest::FaultActivated { fault, seam });
}

pub(super) fn record_receipt_built() {
    append(ObservationEventKindForTest::ReceiptBuilt);
}

pub(super) fn record_session_dropped(publisher: u64) {
    append(ObservationEventKindForTest::SessionDropped { publisher });
}

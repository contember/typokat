//! Read-only append-only candidate audit view for tests.

use super::cache::DurableCache;
use super::session::SessionCore;
use super::ProjectLocalSemanticFaultForTest;
use std::cell::RefCell;
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
    static JOURNAL: RefCell<Vec<ObservationEventForTest>> = const { RefCell::new(Vec::new()) };
    static SESSION_REGISTRY: RefCell<Vec<(u64, Weak<SessionCore>)>> = const { RefCell::new(Vec::new()) };
    static CACHE_REGISTRY: RefCell<Vec<(u64, Weak<DurableCache>)>> = const { RefCell::new(Vec::new()) };
}

pub struct ObservationScopeForTest {
    cursor: usize,
    session_cursor: usize,
    cache_cursor: usize,
}

impl ObservationScopeForTest {
    pub fn start() -> Self {
        Self {
            cursor: JOURNAL.with(|journal| journal.borrow().len()),
            session_cursor: SESSION_REGISTRY.with(|registry| registry.borrow().len()),
            cache_cursor: CACHE_REGISTRY.with(|registry| registry.borrow().len()),
        }
    }

    pub fn finish(self) -> ObservationReceiptForTest {
        ObservationReceiptForTest {
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
        }
    }
}

fn append(kind: ObservationEventKindForTest) {
    JOURNAL.with(|journal| {
        let mut journal = journal.borrow_mut();
        let stamp = journal.last().map_or(1, |event| {
            event.stamp.checked_add(1).expect("audit stamp overflow")
        });
        journal.push(ObservationEventForTest { stamp, kind });
    });
}

pub(super) fn register_session(session: &Rc<SessionCore>, cache: &Rc<DurableCache>) {
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

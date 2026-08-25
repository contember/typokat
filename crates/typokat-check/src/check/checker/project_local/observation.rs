//! Read-only append-only candidate audit view for tests.

use super::ProjectLocalSemanticFaultForTest;
use std::cell::RefCell;

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
}

pub struct ObservationScopeForTest {
    cursor: usize,
}

impl ObservationScopeForTest {
    pub fn start() -> Self {
        Self {
            cursor: JOURNAL.with(|journal| journal.borrow().len()),
        }
    }

    pub fn finish(self) -> ObservationReceiptForTest {
        ObservationReceiptForTest {
            events: JOURNAL.with(|journal| journal.borrow()[self.cursor..].to_vec()),
            witness_kind: ObservationWitnessKindForTest::ActualWeakRegistry,
            session_weak: Vec::new(),
            cache_weak: Vec::new(),
        }
    }
}

//! Canonical candidate fault activation sites.

use super::observation;
use super::{ProjectLocalFaultSeamForTest, ProjectLocalSemanticFaultForTest};

pub(super) struct FaultStateForTest {
    requested: Option<ProjectLocalSemanticFaultForTest>,
    reached: bool,
}

impl FaultStateForTest {
    pub(super) const fn new(requested: Option<ProjectLocalSemanticFaultForTest>) -> Self {
        Self {
            requested,
            reached: false,
        }
    }

    pub(super) fn activate(&mut self, expected: ProjectLocalSemanticFaultForTest) -> bool {
        if self.reached || !activate_if(self.requested, expected) {
            return false;
        }
        self.reached = true;
        true
    }

    pub(super) const fn requested(&self) -> Option<ProjectLocalSemanticFaultForTest> {
        self.requested
    }

    pub(super) const fn is_available(&self) -> bool {
        !self.reached
    }

    pub(super) fn record_publication(&mut self, reached: ProjectLocalSemanticFaultForTest) {
        debug_assert!(!self.reached);
        debug_assert_eq!(self.requested, Some(reached));
        let activated = activate_if(self.requested, reached);
        debug_assert!(activated);
        self.reached = true;
    }
}

pub(super) fn activate_if(
    requested: Option<ProjectLocalSemanticFaultForTest>,
    reached: ProjectLocalSemanticFaultForTest,
) -> bool {
    if requested != Some(reached) {
        return false;
    }
    observation::record_fault(reached, canonical_fault_seam(reached));
    true
}

pub(super) const fn canonical_fault_seam(
    fault: ProjectLocalSemanticFaultForTest,
) -> ProjectLocalFaultSeamForTest {
    match fault {
        ProjectLocalSemanticFaultForTest::OmitDependencyEdge => {
            ProjectLocalFaultSeamForTest::DependencyProjection
        }
        ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete => {
            ProjectLocalFaultSeamForTest::SccCompletion
        }
        ProjectLocalSemanticFaultForTest::WriteResolvingToDurableCache => {
            ProjectLocalFaultSeamForTest::DurableCacheCommit
        }
        ProjectLocalSemanticFaultForTest::PublishStaleGlobalObject => {
            ProjectLocalFaultSeamForTest::GlobalObjectPublication
        }
        ProjectLocalSemanticFaultForTest::InvokeForceAllFromOrdinaryDemand => {
            ProjectLocalFaultSeamForTest::OrdinaryDemandDispatch
        }
        ProjectLocalSemanticFaultForTest::PublishProvisionalUnavailableWrite => {
            ProjectLocalFaultSeamForTest::UnavailableRollback
        }
    }
}

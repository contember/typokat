//! Canonical candidate fault activation sites.

use super::observation;
use super::{ProjectLocalFaultSeamForTest, ProjectLocalSemanticFaultForTest};

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

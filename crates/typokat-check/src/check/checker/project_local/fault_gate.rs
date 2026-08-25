//! Canonical candidate fault activation sites.

use super::{ProjectLocalFaultSeamForTest, ProjectLocalSemanticFaultForTest};

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

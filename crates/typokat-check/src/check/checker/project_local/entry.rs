//! Capability-limited entry for the project-local semantic candidate.

use super::project_local_pass_port::{
    BoundProjectBinder, LibraryBinderUnit, LibraryDeclarationProduct, ProjectProgram,
};
use super::{fault_gate, ProjectLocalSemanticRequestForTest, ProjectLocalSemanticTraceForTest};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalCandidateErrorForTest {
    NotImplemented,
}

impl fmt::Display for ProjectLocalCandidateErrorForTest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented => {
                formatter.write_str("WU4 project-local semantic coordinator is not implemented")
            }
        }
    }
}

pub fn project_local_semantic_candidate_for_test<'ast>(
    product: &LibraryDeclarationProduct,
    library_units: Vec<LibraryBinderUnit>,
    bound: BoundProjectBinder,
    programs: &[ProjectProgram<'ast>],
    request: ProjectLocalSemanticRequestForTest,
) -> Result<ProjectLocalSemanticTraceForTest, ProjectLocalCandidateErrorForTest> {
    let fault_seam = request.fault.map(fault_gate::canonical_fault_seam);
    drop((product, library_units, bound, programs, request, fault_seam));
    Err(ProjectLocalCandidateErrorForTest::NotImplemented)
}

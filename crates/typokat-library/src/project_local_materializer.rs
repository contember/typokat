use super::project_local_symbolic_index::ProjectLocalSymbolicIndex;
use crate::check::checker::library_compiler::{
    ProjectLocalGroupPlanForTest, ProjectLocalMaterializationPlanForTest,
    ProjectLocalSourceAnchorPlanForTest,
};

impl ProjectLocalSymbolicIndex {
    pub(crate) fn materialization_plan(
        &self,
        demands: Vec<String>,
        force_all: bool,
    ) -> ProjectLocalMaterializationPlanForTest {
        ProjectLocalMaterializationPlanForTest {
            sources: self
                .sources
                .iter()
                .map(|source| ProjectLocalSourceAnchorPlanForTest {
                    file_ordinal: crate::source::LibraryFileOrdinal::new(source.ordinal),
                    source_sha256: source.sha256.clone(),
                    statement_starts: source.lexical_statement_starts.clone(),
                })
                .collect(),
            groups: self
                .groups
                .iter()
                .map(|group| ProjectLocalGroupPlanForTest {
                    symbolic_identity: group.identity.clone(),
                    name: group.name.clone(),
                })
                .collect(),
            dependencies: self
                .dependencies
                .iter()
                .map(|dependency| (dependency.from.clone(), dependency.to.clone()))
                .collect(),
            publication_sccs: self.publication_sccs.clone(),
            demands,
            force_all,
            pre_demand_barrier: None,
        }
    }
}

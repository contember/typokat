//! Deferred production validation data for project-local library groups.

use super::project_local_pass_port::{
    LibraryFileOrdinal, ObjectType, SourceSite, TypeId, TypeParamId,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::check::checker) enum ProjectLocalValidationProducer {
    Heritage {
        group: usize,
    },
    Constraint {
        group: usize,
        recipe: usize,
        parameter: usize,
    },
    QualifiedPath {
        recipe: usize,
    },
}

#[derive(Clone)]
pub(in crate::check::checker) struct ProjectLocalValidationSite {
    pub(in crate::check::checker) file_ordinal: LibraryFileOrdinal,
    pub(in crate::check::checker) owner_start: u32,
    pub(in crate::check::checker) site: SourceSite,
}

#[derive(Clone)]
pub(in crate::check::checker) struct ProjectLocalHeritageBase {
    pub(in crate::check::checker) group: usize,
    pub(in crate::check::checker) application: Vec<(TypeParamId, TypeId)>,
    pub(in crate::check::checker) name: String,
}

#[derive(Clone)]
pub(in crate::check::checker) struct ProjectLocalHeritageValidation {
    pub(in crate::check::checker) group: usize,
    pub(in crate::check::checker) site: ProjectLocalValidationSite,
    pub(in crate::check::checker) own: ObjectType,
    pub(in crate::check::checker) bases: Vec<ProjectLocalHeritageBase>,
    pub(in crate::check::checker) derived_name: String,
}

#[derive(Clone, Copy)]
pub(in crate::check::checker) enum ProjectLocalQualifiedPathDiagnosticKind {
    MissingName,
    MissingNamespace,
}

#[derive(Clone)]
pub(in crate::check::checker) struct ProjectLocalQualifiedPathValidation {
    pub(in crate::check::checker) group: usize,
    pub(in crate::check::checker) site: ProjectLocalValidationSite,
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) kind: ProjectLocalQualifiedPathDiagnosticKind,
}

#[derive(Clone)]
pub(in crate::check::checker) struct ProjectLocalConstraintValidation {
    pub(in crate::check::checker) group: usize,
    pub(in crate::check::checker) target_group: usize,
    pub(in crate::check::checker) parameter: usize,
    pub(in crate::check::checker) site: ProjectLocalValidationSite,
    pub(in crate::check::checker) argument: TypeId,
    pub(in crate::check::checker) application: Vec<TypeId>,
}

#[derive(Clone)]
pub(in crate::check::checker) enum ProjectLocalValidationCandidate {
    Heritage(ProjectLocalHeritageValidation),
    Constraint(ProjectLocalConstraintValidation),
    QualifiedPath(ProjectLocalQualifiedPathValidation),
}

impl ProjectLocalValidationCandidate {
    pub(in crate::check::checker) const fn group(&self) -> usize {
        match self {
            Self::Heritage(candidate) => candidate.group,
            Self::Constraint(candidate) => candidate.group,
            Self::QualifiedPath(candidate) => candidate.group,
        }
    }

    pub(in crate::check::checker) const fn site(&self) -> &ProjectLocalValidationSite {
        match self {
            Self::Heritage(candidate) => &candidate.site,
            Self::Constraint(candidate) => &candidate.site,
            Self::QualifiedPath(candidate) => &candidate.site,
        }
    }

    pub(in crate::check::checker) const fn requires_ready_group(&self) -> bool {
        !matches!(self, Self::QualifiedPath(_))
    }
}

#[derive(Clone)]
pub(in crate::check::checker) struct ProjectLocalValidationEvent {
    pub(in crate::check::checker) producer: ProjectLocalValidationProducer,
    pub(in crate::check::checker) candidate: ProjectLocalValidationCandidate,
}

#[derive(Default)]
pub(in crate::check::checker) struct ProjectLocalValidationAccumulator {
    events: BTreeMap<ProjectLocalValidationProducer, ProjectLocalValidationCandidate>,
}

impl ProjectLocalValidationAccumulator {
    pub(in crate::check::checker) fn stage(&mut self, event: ProjectLocalValidationEvent) {
        self.events.entry(event.producer).or_insert(event.candidate);
    }

    pub(in crate::check::checker) fn extend(
        &mut self,
        events: impl IntoIterator<Item = ProjectLocalValidationEvent>,
    ) {
        for event in events {
            self.stage(event);
        }
    }

    pub(in crate::check::checker) fn events(&self) -> Vec<ProjectLocalValidationEvent> {
        self.events
            .iter()
            .map(|(producer, candidate)| ProjectLocalValidationEvent {
                producer: producer.clone(),
                candidate: candidate.clone(),
            })
            .collect()
    }

    pub(in crate::check::checker) fn candidates_for(
        &self,
        ready_groups: &std::collections::BTreeSet<usize>,
        visited_groups: &std::collections::BTreeSet<usize>,
    ) -> Vec<ProjectLocalValidationCandidate> {
        self.events
            .values()
            .filter(|candidate| {
                let groups = if candidate.requires_ready_group() {
                    ready_groups
                } else {
                    visited_groups
                };
                groups.contains(&candidate.group())
            })
            .cloned()
            .collect()
    }
}

//! Stable production identities for project-local checker records.

use super::ProjectLocalLibraryRecordForTest;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum LibraryRecordOwnerIdentity {
    Declaration(usize),
    Member(usize),
    Heritage(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LibraryRecordProducer {
    pub(super) domain: &'static str,
    pub(super) subject: usize,
    pub(super) owner: LibraryRecordOwnerIdentity,
    pub(super) ordinal: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LibraryRecordEventIdentity {
    pub(super) producer: LibraryRecordProducer,
    pub(super) rule: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectLocalLibraryRecordEvent {
    pub(super) identity: LibraryRecordEventIdentity,
    pub(super) normalized: ProjectLocalLibraryRecordForTest,
}

#[derive(Default)]
pub(super) struct LibraryRecordAccumulator {
    events: BTreeMap<LibraryRecordEventIdentity, ProjectLocalLibraryRecordForTest>,
}

impl LibraryRecordAccumulator {
    pub(super) fn stage(&mut self, event: ProjectLocalLibraryRecordEvent) {
        self.events
            .entry(event.identity)
            .or_insert(event.normalized);
    }

    pub(super) fn extend(
        &mut self,
        events: impl IntoIterator<Item = ProjectLocalLibraryRecordEvent>,
    ) {
        for event in events {
            self.stage(event);
        }
    }

    pub(super) fn events(&self) -> Vec<ProjectLocalLibraryRecordEvent> {
        self.events
            .iter()
            .map(|(identity, normalized)| ProjectLocalLibraryRecordEvent {
                identity: identity.clone(),
                normalized: normalized.clone(),
            })
            .collect()
    }

    pub(super) fn normalized(&self) -> Vec<ProjectLocalLibraryRecordForTest> {
        let mut records = self.events.values().cloned().collect::<Vec<_>>();
        records.sort_by(|left, right| {
            (&left.source_path, left.owner_start, &left.record).cmp(&(
                &right.source_path,
                right.owner_start,
                &right.record,
            ))
        });
        records
    }
}

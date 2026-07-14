//! Deterministic checker record reservation and replay.

use crate::diagnostics::{Diagnostic, IncompleteSurface};
use std::collections::BTreeMap;

/// A module's position in the original driver input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ModuleOrdinal(usize);

impl ModuleOrdinal {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// A module's dependency-ordered slot in the checker.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct UnitSlot(usize);

impl UnitSlot {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// Stable identity of one lexically reserved event.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct EventId(usize);

impl EventId {
    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// Stable completion capability for one record position in an event.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RecordTicket {
    pub(crate) event: EventId,
    pub(crate) record_ordinal: usize,
}

/// The total replay order. Field order is intentional and is the checker contract.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct EventKey {
    pub(crate) module_ordinal: ModuleOrdinal,
    pub(crate) source_start: u32,
    pub(crate) event_ordinal: usize,
    pub(crate) record_ordinal: usize,
}

/// One checker-owned output record.
#[derive(Clone, Debug)]
pub(crate) enum CheckerRecord {
    Diagnostic(Diagnostic),
    Incomplete(IncompleteSurface),
}

/// An event always starts with one record ticket, completed by one ordered record group.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReservedEvent {
    pub(crate) id: EventId,
    pub(crate) primary: RecordTicket,
}

#[derive(Debug)]
struct EventMeta {
    module_ordinal: ModuleOrdinal,
    source_start: u32,
    event_ordinal: usize,
    next_record_ordinal: usize,
}

#[derive(Debug)]
enum Completion {
    Pending,
    Completed(Vec<CheckerRecord>),
}

/// Reservation or completion misuse. These failures are checker bugs, not user errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EventStoreError {
    UnknownEvent(EventId),
    UnknownRecord(RecordTicket),
    DuplicateCompletion(RecordTicket),
    Unfinished(Vec<EventKey>),
}

/// Checker-wide authority for deterministic records.
#[derive(Debug, Default)]
pub(crate) struct EventStore {
    events: Vec<EventMeta>,
    next_event_ordinal: BTreeMap<ModuleOrdinal, usize>,
    completions: BTreeMap<EventKey, Completion>,
}

impl EventStore {
    /// Reserve an event and its mandatory primary record position.
    pub(crate) fn reserve_event(
        &mut self,
        module_ordinal: ModuleOrdinal,
        source_start: u32,
    ) -> ReservedEvent {
        let event_ordinal = self.next_event_ordinal.entry(module_ordinal).or_insert(0);
        let id = EventId(self.events.len());
        let primary = RecordTicket {
            event: id,
            record_ordinal: 0,
        };
        let key = EventKey {
            module_ordinal,
            source_start,
            event_ordinal: *event_ordinal,
            record_ordinal: 0,
        };
        *event_ordinal += 1;
        self.events.push(EventMeta {
            module_ordinal,
            source_start,
            event_ordinal: key.event_ordinal,
            next_record_ordinal: 1,
        });
        self.completions.insert(key, Completion::Pending);
        ReservedEvent { id, primary }
    }

    /// Add another ordered record position to an existing event.
    pub(crate) fn reserve_record(
        &mut self,
        event: EventId,
    ) -> Result<RecordTicket, EventStoreError> {
        let Some(meta) = self.events.get_mut(event.index()) else {
            return Err(EventStoreError::UnknownEvent(event));
        };
        let ticket = RecordTicket {
            event,
            record_ordinal: meta.next_record_ordinal,
        };
        meta.next_record_ordinal += 1;
        let key = EventKey {
            module_ordinal: meta.module_ordinal,
            source_start: meta.source_start,
            event_ordinal: meta.event_ordinal,
            record_ordinal: ticket.record_ordinal,
        };
        self.completions.insert(key, Completion::Pending);
        Ok(ticket)
    }

    /// Complete exactly one reserved position with zero or more ordered records.
    pub(crate) fn complete(
        &mut self,
        ticket: RecordTicket,
        records: Vec<CheckerRecord>,
    ) -> Result<(), EventStoreError> {
        let key = self.key(ticket)?;
        let Some(completion) = self.completions.get_mut(&key) else {
            return Err(EventStoreError::UnknownRecord(ticket));
        };
        match completion {
            Completion::Pending => {
                *completion = Completion::Completed(records);
                Ok(())
            }
            Completion::Completed(_) => Err(EventStoreError::DuplicateCompletion(ticket)),
        }
    }

    /// Emit a lexically immediate record under an already-reserved source event.
    #[cfg(test)]
    pub(crate) fn emit_immediate(
        &mut self,
        event: EventId,
        record: CheckerRecord,
    ) -> Result<(), EventStoreError> {
        let ticket = self.reserve_record(event)?;
        self.complete(ticket, vec![record])
    }

    /// Atomically commit the selected candidate's completions.
    pub(crate) fn commit(&mut self, effects: CandidateEffects) -> Result<(), EventStoreError> {
        self.complete(effects.owner, effects.records)
    }

    /// Replay completed records in their reserved total order.
    pub(crate) fn finish(self) -> Result<Vec<(EventKey, CheckerRecord)>, EventStoreError> {
        let unfinished: Vec<EventKey> = self
            .completions
            .iter()
            .filter_map(|(key, completion)| {
                matches!(completion, Completion::Pending).then_some(*key)
            })
            .collect();
        if !unfinished.is_empty() {
            return Err(EventStoreError::Unfinished(unfinished));
        }
        Ok(self
            .completions
            .into_iter()
            .flat_map(|(key, completion)| {
                let Completion::Completed(records) = completion else {
                    unreachable!("pending completions were rejected before replay")
                };
                records.into_iter().map(move |record| (key, record))
            })
            .collect())
    }

    #[cfg(test)]
    pub(crate) fn event_count(&self) -> usize {
        self.events.len()
    }

    #[cfg(test)]
    pub(crate) fn record_count(&self) -> usize {
        self.completions.len()
    }

    fn key(&self, ticket: RecordTicket) -> Result<EventKey, EventStoreError> {
        let Some(meta) = self.events.get(ticket.event.index()) else {
            return Err(EventStoreError::UnknownEvent(ticket.event));
        };
        if ticket.record_ordinal >= meta.next_record_ordinal {
            return Err(EventStoreError::UnknownRecord(ticket));
        }
        Ok(EventKey {
            module_ordinal: meta.module_ordinal,
            source_start: meta.source_start,
            event_ordinal: meta.event_ordinal,
            record_ordinal: ticket.record_ordinal,
        })
    }
}

/// Records accumulated by one speculative candidate before the winner commits.
#[derive(Debug)]
pub(crate) struct CandidateEffects {
    owner: RecordTicket,
    records: Vec<CheckerRecord>,
}

impl CandidateEffects {
    pub(crate) fn new(owner: RecordTicket) -> Self {
        Self {
            owner,
            records: Vec::new(),
        }
    }

    pub(crate) fn diagnostic(&mut self, diagnostic: Diagnostic) {
        self.records.push(CheckerRecord::Diagnostic(diagnostic));
    }

    pub(crate) fn incomplete(&mut self, incomplete: IncompleteSurface) {
        self.records.push(CheckerRecord::Incomplete(incomplete));
    }

    pub(crate) fn record(&mut self, record: CheckerRecord) {
        self.records.push(record);
    }

    pub(crate) fn merge(&mut self, child: CandidateEffects) {
        assert_eq!(
            self.owner, child.owner,
            "nested effects must share one owner"
        );
        self.records.extend(child.records);
    }

    pub(crate) fn owner(&self) -> RecordTicket {
        self.owner
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn discard(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagnosticCode;
    use crate::span::Span;

    fn diagnostic(name: &str, start: u32) -> CheckerRecord {
        CheckerRecord::Diagnostic(Diagnostic::cannot_find_name(
            Span::new(start, start + 1),
            name,
        ))
    }

    fn diagnostic_name(record: CheckerRecord) -> String {
        let CheckerRecord::Diagnostic(diagnostic) = record else {
            panic!("expected diagnostic");
        };
        assert_eq!(diagnostic.code, DiagnosticCode::TK2304);
        diagnostic.message
    }

    #[test]
    fn source_start_precedes_reservation_and_completion_order() {
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let later = store.reserve_event(module, 20);
        let earlier = store.reserve_event(module, 10);
        store
            .complete(later.primary, vec![diagnostic("later", 20)])
            .unwrap();
        store
            .complete(earlier.primary, vec![diagnostic("earlier", 10)])
            .unwrap();

        let records = store.finish().unwrap();
        assert_eq!(
            diagnostic_name(records[0].1.clone()),
            "Cannot find name 'earlier'"
        );
        assert_eq!(
            diagnostic_name(records[1].1.clone()),
            "Cannot find name 'later'"
        );
    }

    #[test]
    fn same_source_uses_event_ordinal_despite_reverse_completion() {
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let first = store.reserve_event(module, 10);
        let second = store.reserve_event(module, 10);
        store
            .complete(second.primary, vec![diagnostic("second", 10)])
            .unwrap();
        store
            .complete(first.primary, vec![diagnostic("first", 10)])
            .unwrap();

        let records = store.finish().unwrap();
        assert_eq!(
            diagnostic_name(records[0].1.clone()),
            "Cannot find name 'first'"
        );
        assert_eq!(
            diagnostic_name(records[1].1.clone()),
            "Cannot find name 'second'"
        );
    }

    #[test]
    fn multi_record_event_replays_record_ordinal_after_reverse_completion() {
        let mut store = EventStore::default();
        let event = store.reserve_event(ModuleOrdinal::new(0), 10);
        let second = store.reserve_record(event.id).unwrap();
        let third = store.reserve_record(event.id).unwrap();
        store
            .complete(third, vec![diagnostic("third", 10)])
            .unwrap();
        store
            .complete(second, vec![diagnostic("second", 10)])
            .unwrap();
        store
            .complete(event.primary, vec![diagnostic("first", 10)])
            .unwrap();

        let records = store.finish().unwrap();
        let names: Vec<String> = records
            .into_iter()
            .map(|(_, record)| diagnostic_name(record))
            .collect();
        assert_eq!(
            names,
            [
                "Cannot find name 'first'",
                "Cannot find name 'second'",
                "Cannot find name 'third'",
            ]
        );
    }

    #[test]
    fn explicit_none_completes_without_replay() {
        let mut store = EventStore::default();
        let event = store.reserve_event(ModuleOrdinal::new(0), 10);
        store.complete(event.primary, Vec::new()).unwrap();
        assert!(store.finish().unwrap().is_empty());
    }

    #[test]
    fn immediate_record_uses_the_preallocated_event_and_next_local_ordinal() {
        let mut store = EventStore::default();
        let event = store.reserve_event(ModuleOrdinal::new(0), 10);
        store
            .emit_immediate(event.id, diagnostic("immediate", 12))
            .unwrap();
        store.complete(event.primary, Vec::new()).unwrap();

        let records = store.finish().unwrap();
        assert_eq!(records[0].0.source_start, 10);
        assert_eq!(records[0].0.record_ordinal, 1);
        assert_eq!(
            diagnostic_name(records[0].1.clone()),
            "Cannot find name 'immediate'"
        );
    }

    #[test]
    fn module_ordinal_precedes_dependency_completion_and_source_position() {
        let mut store = EventStore::default();
        let dependency = store.reserve_event(ModuleOrdinal::new(1), 0);
        let input_first = store.reserve_event(ModuleOrdinal::new(0), 100);
        store
            .complete(dependency.primary, vec![diagnostic("dependency", 0)])
            .unwrap();
        store
            .complete(input_first.primary, vec![diagnostic("input", 100)])
            .unwrap();

        let records = store.finish().unwrap();
        assert_eq!(records[0].0.module_ordinal, ModuleOrdinal::new(0));
        assert_eq!(records[1].0.module_ordinal, ModuleOrdinal::new(1));
    }

    #[test]
    fn candidate_helpers_preserve_diagnostic_and_incomplete_channels() {
        let mut store = EventStore::default();
        let event = store.reserve_event(ModuleOrdinal::new(0), 10);
        let mut effects = CandidateEffects::new(event.primary);
        effects.diagnostic(Diagnostic::cannot_find_name(Span::new(10, 11), "missing"));
        effects.incomplete(IncompleteSurface::new(
            "test/site",
            Span::new(20, 21),
            "test",
        ));
        store.commit(effects).unwrap();

        let records = store.finish().unwrap();
        let CheckerRecord::Diagnostic(diagnostic) = &records[0].1 else {
            panic!("expected diagnostic");
        };
        assert_eq!(diagnostic.code, DiagnosticCode::TK2304);
        let CheckerRecord::Incomplete(incomplete) = &records[1].1 else {
            panic!("expected incomplete record");
        };
        assert_eq!(incomplete.id, "test/site");
    }

    #[test]
    fn unfinished_and_duplicate_completion_are_failures() {
        let mut unfinished = EventStore::default();
        unfinished.reserve_event(ModuleOrdinal::new(0), 10);
        assert!(matches!(
            unfinished.finish(),
            Err(EventStoreError::Unfinished(keys)) if keys == vec![EventKey {
                module_ordinal: ModuleOrdinal::new(0),
                source_start: 10,
                event_ordinal: 0,
                record_ordinal: 0,
            }]
        ));

        let mut duplicate = EventStore::default();
        let event = duplicate.reserve_event(ModuleOrdinal::new(0), 10);
        duplicate.complete(event.primary, Vec::new()).unwrap();
        assert_eq!(
            duplicate.complete(event.primary, Vec::new()),
            Err(EventStoreError::DuplicateCompletion(event.primary))
        );
    }

    #[test]
    fn candidate_commit_is_atomic_and_discard_has_no_replay() {
        let mut store = EventStore::default();
        let selected = store.reserve_event(ModuleOrdinal::new(0), 10);
        let rejected = store.reserve_event(ModuleOrdinal::new(0), 20);

        let mut rejected_effects = CandidateEffects::new(rejected.primary);
        let CheckerRecord::Diagnostic(rejected_diagnostic) = diagnostic("rejected", 20) else {
            unreachable!();
        };
        rejected_effects.diagnostic(rejected_diagnostic);
        rejected_effects.discard();
        store.complete(rejected.primary, Vec::new()).unwrap();

        let mut selected_effects = CandidateEffects::new(selected.primary);
        let CheckerRecord::Diagnostic(selected_diagnostic) = diagnostic("selected", 10) else {
            unreachable!();
        };
        selected_effects.diagnostic(selected_diagnostic);
        store.commit(selected_effects).unwrap();

        assert_eq!(store.event_count(), 2);
        assert_eq!(store.record_count(), 2);
        let records = store.finish().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            diagnostic_name(records.into_iter().next().unwrap().1),
            "Cannot find name 'selected'"
        );
    }

    #[test]
    fn selected_nested_effects_merge_into_one_owner_in_order() {
        let mut store = EventStore::default();
        let event = store.reserve_event(ModuleOrdinal::new(0), 10);
        let mut parent = CandidateEffects::new(event.primary);
        let CheckerRecord::Diagnostic(first) = diagnostic("first", 10) else {
            unreachable!();
        };
        parent.diagnostic(first);

        let mut rejected = CandidateEffects::new(event.primary);
        let CheckerRecord::Diagnostic(rejected_record) = diagnostic("rejected", 10) else {
            unreachable!();
        };
        rejected.diagnostic(rejected_record);
        rejected.discard();

        let mut selected = CandidateEffects::new(event.primary);
        let CheckerRecord::Diagnostic(second) = diagnostic("second", 10) else {
            unreachable!();
        };
        selected.diagnostic(second);
        parent.merge(selected);
        store.commit(parent).unwrap();

        let names = store
            .finish()
            .unwrap()
            .into_iter()
            .map(|(_, record)| diagnostic_name(record))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            ["Cannot find name 'first'", "Cannot find name 'second'"]
        );
    }
}

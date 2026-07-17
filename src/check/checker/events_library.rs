//! Deterministic record reservation and replay for library files.

use super::reporting_record::CheckerRecord;
use crate::source::LibraryFileOrdinal;
use std::collections::BTreeMap;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LibraryEventId(usize);

impl LibraryEventId {
    const fn index(self) -> usize {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LibraryRecordTicket {
    pub(crate) event: LibraryEventId,
    pub(crate) record_ordinal: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibraryReservedEvent {
    pub(crate) id: LibraryEventId,
    pub(crate) primary: LibraryRecordTicket,
}

/// The total library replay order. Field order is intentional.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LibraryEventKey {
    pub(crate) file_ordinal: LibraryFileOrdinal,
    pub(crate) source_start: u32,
    pub(crate) event_ordinal: usize,
    pub(crate) record_ordinal: usize,
}

#[derive(Debug)]
struct LibraryEventMeta {
    file_ordinal: LibraryFileOrdinal,
    source_start: u32,
    event_ordinal: usize,
    next_record_ordinal: usize,
}

#[derive(Debug)]
enum LibraryCompletion {
    Pending,
    Completed(Vec<CheckerRecord>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LibraryEventLedgerError {
    UnknownEvent(LibraryEventId),
    UnknownRecord(LibraryRecordTicket),
    DuplicateCompletion(LibraryRecordTicket),
    Unfinished(Vec<LibraryEventKey>),
}

#[derive(Debug, Default)]
pub(crate) struct LibraryEventLedger {
    events: Vec<LibraryEventMeta>,
    next_event_ordinal: BTreeMap<LibraryFileOrdinal, usize>,
    completions: BTreeMap<LibraryEventKey, LibraryCompletion>,
}

impl LibraryEventLedger {
    pub(crate) fn reserve_event(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        source_start: u32,
    ) -> LibraryReservedEvent {
        let event_ordinal = self.next_event_ordinal.entry(file_ordinal).or_insert(0);
        let id = LibraryEventId(self.events.len());
        let primary = LibraryRecordTicket {
            event: id,
            record_ordinal: 0,
        };
        let key = LibraryEventKey {
            file_ordinal,
            source_start,
            event_ordinal: *event_ordinal,
            record_ordinal: 0,
        };
        *event_ordinal += 1;
        self.events.push(LibraryEventMeta {
            file_ordinal,
            source_start,
            event_ordinal: key.event_ordinal,
            next_record_ordinal: 1,
        });
        self.completions.insert(key, LibraryCompletion::Pending);
        LibraryReservedEvent { id, primary }
    }

    pub(crate) fn reserve_record(
        &mut self,
        event: LibraryEventId,
    ) -> Result<LibraryRecordTicket, LibraryEventLedgerError> {
        let Some(meta) = self.events.get_mut(event.index()) else {
            return Err(LibraryEventLedgerError::UnknownEvent(event));
        };
        let ticket = LibraryRecordTicket {
            event,
            record_ordinal: meta.next_record_ordinal,
        };
        meta.next_record_ordinal += 1;
        let key = LibraryEventKey {
            file_ordinal: meta.file_ordinal,
            source_start: meta.source_start,
            event_ordinal: meta.event_ordinal,
            record_ordinal: ticket.record_ordinal,
        };
        self.completions.insert(key, LibraryCompletion::Pending);
        Ok(ticket)
    }

    pub(crate) fn complete(
        &mut self,
        ticket: LibraryRecordTicket,
        records: Vec<CheckerRecord>,
    ) -> Result<(), LibraryEventLedgerError> {
        let key = self.key(ticket)?;
        let Some(completion) = self.completions.get_mut(&key) else {
            return Err(LibraryEventLedgerError::UnknownRecord(ticket));
        };
        match completion {
            LibraryCompletion::Pending => {
                *completion = LibraryCompletion::Completed(records);
                Ok(())
            }
            LibraryCompletion::Completed(_) => {
                Err(LibraryEventLedgerError::DuplicateCompletion(ticket))
            }
        }
    }

    pub(crate) fn finish(
        self,
    ) -> Result<Vec<(LibraryEventKey, CheckerRecord)>, LibraryEventLedgerError> {
        let unfinished = self
            .completions
            .iter()
            .filter_map(|(key, completion)| {
                matches!(completion, LibraryCompletion::Pending).then_some(*key)
            })
            .collect::<Vec<_>>();
        if !unfinished.is_empty() {
            return Err(LibraryEventLedgerError::Unfinished(unfinished));
        }

        Ok(self
            .completions
            .into_iter()
            .filter_map(|(key, completion)| match completion {
                LibraryCompletion::Pending => None,
                LibraryCompletion::Completed(records) => Some((key, records)),
            })
            .flat_map(|(key, records)| records.into_iter().map(move |record| (key, record)))
            .collect())
    }

    fn key(&self, ticket: LibraryRecordTicket) -> Result<LibraryEventKey, LibraryEventLedgerError> {
        let Some(meta) = self.events.get(ticket.event.index()) else {
            return Err(LibraryEventLedgerError::UnknownEvent(ticket.event));
        };
        if ticket.record_ordinal >= meta.next_record_ordinal {
            return Err(LibraryEventLedgerError::UnknownRecord(ticket));
        }
        Ok(LibraryEventKey {
            file_ordinal: meta.file_ordinal,
            source_start: meta.source_start,
            event_ordinal: meta.event_ordinal,
            record_ordinal: ticket.record_ordinal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::IncompleteSurface;
    use crate::span::Span;

    fn incomplete(id: &str, start: u32) -> CheckerRecord {
        CheckerRecord::Incomplete(IncompleteSurface::new(
            id,
            Span::new(start, start + 1),
            "library event test",
        ))
    }

    fn incomplete_id(record: CheckerRecord) -> String {
        let CheckerRecord::Incomplete(incomplete) = record else {
            panic!("expected incomplete record");
        };
        incomplete.id
    }

    #[test]
    fn file_and_source_order_precede_reservation_and_completion_order() {
        let mut ledger = LibraryEventLedger::default();
        let later_file = ledger.reserve_event(LibraryFileOrdinal::new(1), 0);
        let later_source = ledger.reserve_event(LibraryFileOrdinal::new(0), 20);
        let earlier_source = ledger.reserve_event(LibraryFileOrdinal::new(0), 10);

        ledger
            .complete(later_file.primary, vec![incomplete("later-file", 0)])
            .unwrap();
        ledger
            .complete(later_source.primary, vec![incomplete("later-source", 20)])
            .unwrap();
        ledger
            .complete(
                earlier_source.primary,
                vec![incomplete("earlier-source", 10)],
            )
            .unwrap();

        let records = ledger.finish().unwrap();
        let ids = records
            .into_iter()
            .map(|(_, record)| incomplete_id(record))
            .collect::<Vec<_>>();
        assert_eq!(ids, ["earlier-source", "later-source", "later-file"]);
    }

    #[test]
    fn same_source_replays_event_then_record_ordinal() {
        let mut ledger = LibraryEventLedger::default();
        let first = ledger.reserve_event(LibraryFileOrdinal::new(0), 10);
        let second = ledger.reserve_record(first.id).unwrap();
        let next_event = ledger.reserve_event(LibraryFileOrdinal::new(0), 10);

        ledger
            .complete(next_event.primary, vec![incomplete("next-event", 10)])
            .unwrap();
        ledger
            .complete(second, vec![incomplete("second-record", 10)])
            .unwrap();
        ledger
            .complete(first.primary, vec![incomplete("first-record", 10)])
            .unwrap();

        let records = ledger.finish().unwrap();
        let keys = records
            .iter()
            .map(|(key, _)| {
                (
                    key.file_ordinal.index(),
                    key.source_start,
                    key.event_ordinal,
                    key.record_ordinal,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(keys, [(0, 10, 0, 0), (0, 10, 0, 1), (0, 10, 1, 0)]);
    }

    #[test]
    fn unfinished_duplicate_and_unknown_operations_are_typed_failures() {
        let mut unfinished = LibraryEventLedger::default();
        unfinished.reserve_event(LibraryFileOrdinal::new(3), 7);
        assert!(matches!(
            unfinished.finish(),
            Err(LibraryEventLedgerError::Unfinished(keys)) if keys == vec![LibraryEventKey {
                file_ordinal: LibraryFileOrdinal::new(3),
                source_start: 7,
                event_ordinal: 0,
                record_ordinal: 0,
            }]
        ));

        let mut duplicate = LibraryEventLedger::default();
        let event = duplicate.reserve_event(LibraryFileOrdinal::new(0), 0);
        duplicate.complete(event.primary, Vec::new()).unwrap();
        assert_eq!(
            duplicate.complete(event.primary, Vec::new()),
            Err(LibraryEventLedgerError::DuplicateCompletion(event.primary))
        );

        let unknown_event = LibraryEventId(9);
        assert_eq!(
            duplicate.reserve_record(unknown_event),
            Err(LibraryEventLedgerError::UnknownEvent(unknown_event))
        );
        let unknown_record = LibraryRecordTicket {
            event: event.id,
            record_ordinal: 9,
        };
        assert_eq!(
            duplicate.complete(unknown_record, Vec::new()),
            Err(LibraryEventLedgerError::UnknownRecord(unknown_record))
        );
    }
}

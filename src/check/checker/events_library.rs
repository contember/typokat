//! Deterministic record reservation and replay for library files.

use super::context::CheckerRecordBatch;
use super::reporting_record::CheckerRecord;
use crate::source::LibraryFileOrdinal;
use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibraryEventLedgerSnapshot {
    pub(crate) reserved_records: usize,
    pub(crate) filled_records: usize,
    pub(crate) reserved_file_ordinals: Vec<LibraryFileOrdinal>,
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

    pub(crate) fn snapshot(&self) -> LibraryEventLedgerSnapshot {
        LibraryEventLedgerSnapshot {
            reserved_records: self.completions.len(),
            filled_records: self
                .completions
                .values()
                .filter(|completion| matches!(completion, LibraryCompletion::Completed(_)))
                .count(),
            reserved_file_ordinals: self
                .events
                .iter()
                .map(|event| event.file_ordinal)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }

    fn validate_batch_tickets(
        &self,
        tickets: impl IntoIterator<Item = LibraryRecordTicket>,
    ) -> Result<(), LibraryEventLedgerError> {
        let mut seen = BTreeSet::new();
        for ticket in tickets {
            if !seen.insert(ticket) {
                return Err(LibraryEventLedgerError::DuplicateCompletion(ticket));
            }
            let key = self.key(ticket)?;
            let Some(completion) = self.completions.get(&key) else {
                return Err(LibraryEventLedgerError::UnknownRecord(ticket));
            };
            if matches!(completion, LibraryCompletion::Completed(_)) {
                return Err(LibraryEventLedgerError::DuplicateCompletion(ticket));
            }
        }
        Ok(())
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

pub(crate) struct LibrarySemanticReportingAdapter<'ledger> {
    ledger: &'ledger mut LibraryEventLedger,
}

impl<'ledger> LibrarySemanticReportingAdapter<'ledger> {
    pub(crate) fn new(ledger: &'ledger mut LibraryEventLedger) -> Self {
        Self { ledger }
    }

    pub(in crate::check::checker) fn complete_semantic_batches(
        &mut self,
        batches: Vec<CheckerRecordBatch<LibraryRecordTicket>>,
    ) -> Result<(), LibraryEventLedgerError> {
        let batches = batches
            .into_iter()
            .map(CheckerRecordBatch::into_parts)
            .collect::<Vec<_>>();
        self.ledger
            .validate_batch_tickets(batches.iter().map(|(ticket, _)| *ticket))?;
        for (ticket, records) in batches {
            self.ledger.complete(ticket, records)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::context::CheckerEffects;
    use super::*;
    use crate::diagnostics::{Diagnostic, IncompleteSurface};
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

    fn batch(
        owner: LibraryRecordTicket,
        records: impl IntoIterator<Item = CheckerRecord>,
    ) -> CheckerRecordBatch<LibraryRecordTicket> {
        let mut effects = CheckerEffects::new(owner);
        for record in records {
            effects.records.record(record);
        }
        effects.records
    }

    fn assert_owned_terminal<T: Send + Sync + 'static>() {}

    #[test]
    fn ledger_snapshot_is_owned_and_shareable() {
        assert_owned_terminal::<LibraryEventLedgerSnapshot>();
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

    #[test]
    fn semantic_adapter_counts_zero_record_completion_as_filled() {
        let file_ordinal = LibraryFileOrdinal::new(4);
        let mut ledger = LibraryEventLedger::default();
        let event = ledger.reserve_event(file_ordinal, 9);
        assert_eq!(
            ledger.snapshot(),
            LibraryEventLedgerSnapshot {
                reserved_records: 1,
                filled_records: 0,
                reserved_file_ordinals: vec![file_ordinal],
            }
        );

        LibrarySemanticReportingAdapter::new(&mut ledger)
            .complete_semantic_batches(vec![batch(event.primary, [])])
            .unwrap();

        assert_eq!(
            ledger.snapshot(),
            LibraryEventLedgerSnapshot {
                reserved_records: 1,
                filled_records: 1,
                reserved_file_ordinals: vec![file_ordinal],
            }
        );
        assert!(ledger.finish().unwrap().is_empty());
    }

    #[test]
    fn semantic_adapter_preserves_records_within_one_ticket() {
        let mut ledger = LibraryEventLedger::default();
        let event = ledger.reserve_event(LibraryFileOrdinal::new(8), 20);
        LibrarySemanticReportingAdapter::new(&mut ledger)
            .complete_semantic_batches(vec![batch(
                event.primary,
                [
                    incomplete("first", 20),
                    CheckerRecord::Diagnostic(Diagnostic::cannot_find_name(
                        Span::new(21, 22),
                        "middle",
                    )),
                    incomplete("last", 22),
                ],
            )])
            .unwrap();

        let records = ledger.finish().unwrap();
        assert_eq!(records.len(), 3);
        assert!(matches!(
            &records[..],
            [
                (_, CheckerRecord::Incomplete(first)),
                (_, CheckerRecord::Diagnostic(middle)),
                (_, CheckerRecord::Incomplete(last)),
            ] if first.id == "first"
                && middle.span.start == 21
                && last.id == "last"
        ));
    }

    #[test]
    fn snapshot_reports_sorted_deduplicated_actual_file_ordinals() {
        let mut ledger = LibraryEventLedger::default();
        ledger.reserve_event(LibraryFileOrdinal::new(9), 0);
        ledger.reserve_event(LibraryFileOrdinal::new(2), 1);
        ledger.reserve_event(LibraryFileOrdinal::new(9), 2);
        ledger.reserve_event(LibraryFileOrdinal::new(5), 3);

        assert_eq!(
            ledger.snapshot().reserved_file_ordinals,
            [
                LibraryFileOrdinal::new(2),
                LibraryFileOrdinal::new(5),
                LibraryFileOrdinal::new(9),
            ]
        );
    }

    #[test]
    fn semantic_adapter_rejects_whole_invalid_batch_before_completion() {
        let mut duplicate_ledger = LibraryEventLedger::default();
        let duplicate = duplicate_ledger
            .reserve_event(LibraryFileOrdinal::new(1), 3)
            .primary;
        let duplicate_error = LibrarySemanticReportingAdapter::new(&mut duplicate_ledger)
            .complete_semantic_batches(vec![batch(duplicate, []), batch(duplicate, [])]);
        assert_eq!(
            duplicate_error,
            Err(LibraryEventLedgerError::DuplicateCompletion(duplicate))
        );
        assert_eq!(duplicate_ledger.snapshot().filled_records, 0);

        let mut unknown_ledger = LibraryEventLedger::default();
        let valid = unknown_ledger
            .reserve_event(LibraryFileOrdinal::new(2), 4)
            .primary;
        let unknown = LibraryRecordTicket {
            event: valid.event,
            record_ordinal: 99,
        };
        let unknown_error = LibrarySemanticReportingAdapter::new(&mut unknown_ledger)
            .complete_semantic_batches(vec![batch(valid, []), batch(unknown, [])]);
        assert_eq!(
            unknown_error,
            Err(LibraryEventLedgerError::UnknownRecord(unknown))
        );
        assert_eq!(unknown_ledger.snapshot().filled_records, 0);
    }

    #[test]
    fn semantic_adapter_keeps_pending_ticket_unfilled_when_peer_is_completed() {
        let mut ledger = LibraryEventLedger::default();
        let pending = ledger.reserve_event(LibraryFileOrdinal::new(6), 10).primary;
        let completed = ledger.reserve_event(LibraryFileOrdinal::new(6), 20).primary;
        ledger.complete(completed, Vec::new()).unwrap();

        let error = LibrarySemanticReportingAdapter::new(&mut ledger)
            .complete_semantic_batches(vec![batch(pending, []), batch(completed, [])]);
        assert_eq!(
            error,
            Err(LibraryEventLedgerError::DuplicateCompletion(completed))
        );
        assert_eq!(ledger.snapshot().filled_records, 1);
        assert!(matches!(
            ledger.finish(),
            Err(LibraryEventLedgerError::Unfinished(keys)) if keys == vec![LibraryEventKey {
                file_ordinal: LibraryFileOrdinal::new(6),
                source_start: 10,
                event_ordinal: 0,
                record_ordinal: 0,
            }]
        ));
    }

    #[test]
    fn semantic_adapter_keeps_pending_ticket_unfilled_when_peer_event_is_unknown() {
        let mut ledger = LibraryEventLedger::default();
        let pending = ledger.reserve_event(LibraryFileOrdinal::new(7), 12).primary;
        let unknown = LibraryRecordTicket {
            event: LibraryEventId(usize::MAX),
            record_ordinal: 0,
        };

        let error = LibrarySemanticReportingAdapter::new(&mut ledger)
            .complete_semantic_batches(vec![batch(pending, []), batch(unknown, [])]);
        assert_eq!(
            error,
            Err(LibraryEventLedgerError::UnknownEvent(unknown.event))
        );
        assert_eq!(ledger.snapshot().filled_records, 0);
        assert!(matches!(
            ledger.finish(),
            Err(LibraryEventLedgerError::Unfinished(keys)) if keys == vec![LibraryEventKey {
                file_ordinal: LibraryFileOrdinal::new(7),
                source_start: 12,
                event_ordinal: 0,
                record_ordinal: 0,
            }]
        ));
    }

    #[test]
    fn semantic_adapter_reverse_completion_replays_exact_ledger_keys() {
        let mut ledger = LibraryEventLedger::default();
        let later_file = ledger.reserve_event(LibraryFileOrdinal::new(4), 20).primary;
        let later_source = ledger.reserve_event(LibraryFileOrdinal::new(1), 30).primary;
        let earlier_source = ledger.reserve_event(LibraryFileOrdinal::new(1), 10).primary;

        LibrarySemanticReportingAdapter::new(&mut ledger)
            .complete_semantic_batches(vec![
                batch(earlier_source, [incomplete("earlier-source", 10)]),
                batch(later_source, [incomplete("later-source", 30)]),
                batch(later_file, [incomplete("later-file", 20)]),
            ])
            .unwrap();

        let records = ledger.finish().unwrap();
        assert_eq!(
            records.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            [
                LibraryEventKey {
                    file_ordinal: LibraryFileOrdinal::new(1),
                    source_start: 10,
                    event_ordinal: 1,
                    record_ordinal: 0,
                },
                LibraryEventKey {
                    file_ordinal: LibraryFileOrdinal::new(1),
                    source_start: 30,
                    event_ordinal: 0,
                    record_ordinal: 0,
                },
                LibraryEventKey {
                    file_ordinal: LibraryFileOrdinal::new(4),
                    source_start: 20,
                    event_ordinal: 0,
                    record_ordinal: 0,
                },
            ]
        );
        assert_eq!(
            records
                .into_iter()
                .map(|(_, record)| incomplete_id(record))
                .collect::<Vec<_>>(),
            ["earlier-source", "later-source", "later-file"]
        );
    }
}

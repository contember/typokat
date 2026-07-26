//! Deterministic checker record reservation and replay.

use super::reporting_record::CheckerRecord;
use crate::diagnostics::Diagnostic;
#[cfg(test)]
use crate::diagnostics::IncompleteSurface;
use crate::source::ModuleOrdinal;
use std::collections::BTreeMap;

#[cfg(test)]
thread_local! {
    static USER_EVENT_RESERVATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_user_event_reservation_for_test() {
    USER_EVENT_RESERVATIONS.set(USER_EVENT_RESERVATIONS.get().saturating_add(1));
}

#[cfg(test)]
pub(crate) struct UserEventReservationScopeForTest(u64);

#[cfg(test)]
impl UserEventReservationScopeForTest {
    pub(crate) fn start() -> Self {
        Self(USER_EVENT_RESERVATIONS.get())
    }

    pub(crate) fn finish(self) -> u64 {
        USER_EVENT_RESERVATIONS.get().saturating_sub(self.0)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UserEventReservationCalibrationForTest {
    pub(crate) event: u64,
    pub(crate) record: u64,
}

#[cfg(test)]
impl UserEventReservationCalibrationForTest {
    pub(crate) fn total(self) -> u64 {
        self.event.saturating_add(self.record)
    }
}

#[cfg(test)]
pub(crate) fn calibrate_user_event_reservations_for_test() -> UserEventReservationCalibrationForTest
{
    let event_scope = UserEventReservationScopeForTest::start();
    let mut store = EventStore::default();
    let event = store.reserve_event(ModuleOrdinal::new(0), 0);
    let event_count = event_scope.finish();

    let record_scope = UserEventReservationScopeForTest::start();
    store
        .reserve_record(event.id)
        .expect("calibration secondary record reserves");
    let record = record_scope.finish();

    UserEventReservationCalibrationForTest {
        event: event_count,
        record,
    }
}

#[cfg(test)]
thread_local! {
    static RESERVED_EVENTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static RESERVED_RECORD_POSITIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static MATERIALIZED_COMPLETION_SLOTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static MATERIALIZED_COMPLETION_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static REPLAYED_RECORDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Completion storage the store commits to while the lexical walk is still running — before
/// any semantic phase has produced a record. Reservation is unconditional and lexical, so
/// every field below is driven by how many statement sites a program *has*, never by how many
/// of them turn out to say anything.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EventCompletionWorkForTest {
    /// Events reserved by the lexical walk. One per source site the replay key must name.
    pub(crate) reserved_events: u64,
    /// Record positions reserved under those events — `immediate`/`deferred`/`incomplete` per
    /// site, plus a callable `body`. The replay contract has to *name* each of these; naming
    /// one does not by itself require the store to keep anything for it.
    pub(crate) reserved_record_positions: u64,
    /// Positions for which the store physically materialized a completion slot during
    /// reservation, holding nothing.
    pub(crate) materialized_completion_slots: u64,
    /// Bytes those slots retain. Each representation reports its own per-slot cost; the
    /// `BTreeMap` reports key + value only, so its figure is a lower bound that ignores
    /// B-tree node overhead and fill factor.
    pub(crate) materialized_completion_bytes: u64,
    /// Records the store actually replayed — the only thing a completion slot exists to carry.
    pub(crate) replayed_records: u64,
}

#[cfg(test)]
fn event_completion_work_for_test() -> EventCompletionWorkForTest {
    EventCompletionWorkForTest {
        reserved_events: RESERVED_EVENTS.get(),
        reserved_record_positions: RESERVED_RECORD_POSITIONS.get(),
        materialized_completion_slots: MATERIALIZED_COMPLETION_SLOTS.get(),
        materialized_completion_bytes: MATERIALIZED_COMPLETION_BYTES.get(),
        replayed_records: REPLAYED_RECORDS.get(),
    }
}

#[cfg(test)]
fn record_reserved_event() {
    RESERVED_EVENTS.set(RESERVED_EVENTS.get().saturating_add(1));
}

#[cfg(test)]
fn record_reserved_record_position() {
    RESERVED_RECORD_POSITIONS.set(RESERVED_RECORD_POSITIONS.get().saturating_add(1));
}

/// Account for the storage one newly reserved position materialized. A representation that
/// keeps no per-position storage reports zero bytes, and therefore no slot: nothing holds the
/// position's place except a bit in an event row the store already pays for.
#[cfg(test)]
fn record_materialized_completion_slot(bytes: usize) {
    if bytes == 0 {
        return;
    }
    MATERIALIZED_COMPLETION_SLOTS.set(MATERIALIZED_COMPLETION_SLOTS.get().saturating_add(1));
    MATERIALIZED_COMPLETION_BYTES.set(
        MATERIALIZED_COMPLETION_BYTES
            .get()
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX)),
    );
}

#[cfg(test)]
fn record_replayed_records(records: usize) {
    REPLAYED_RECORDS.set(
        REPLAYED_RECORDS
            .get()
            .saturating_add(u64::try_from(records).unwrap_or(u64::MAX)),
    );
}

#[cfg(test)]
pub(crate) struct EventCompletionWorkScopeForTest(EventCompletionWorkForTest);

#[cfg(test)]
impl EventCompletionWorkScopeForTest {
    pub(crate) fn start() -> Self {
        Self(event_completion_work_for_test())
    }

    pub(crate) fn finish(self) -> EventCompletionWorkForTest {
        let end = event_completion_work_for_test();
        EventCompletionWorkForTest {
            reserved_events: end.reserved_events.saturating_sub(self.0.reserved_events),
            reserved_record_positions: end
                .reserved_record_positions
                .saturating_sub(self.0.reserved_record_positions),
            materialized_completion_slots: end
                .materialized_completion_slots
                .saturating_sub(self.0.materialized_completion_slots),
            materialized_completion_bytes: end
                .materialized_completion_bytes
                .saturating_sub(self.0.materialized_completion_bytes),
            replayed_records: end.replayed_records.saturating_sub(self.0.replayed_records),
        }
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
pub(crate) struct UserRecordTicket {
    pub(crate) event: EventId,
    pub(crate) record_ordinal: usize,
}

pub(crate) const fn user_record_ticket_key(ticket: UserRecordTicket) -> (usize, usize) {
    (ticket.event.index(), ticket.record_ordinal)
}

/// The total replay order. Field order is intentional and is the checker contract.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct EventKey {
    pub(crate) module_ordinal: ModuleOrdinal,
    pub(crate) source_start: u32,
    pub(crate) event_ordinal: usize,
    pub(crate) record_ordinal: usize,
}

/// An event always starts with one record ticket, completed by one ordered record group.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReservedEvent {
    pub(crate) id: EventId,
    pub(crate) primary: UserRecordTicket,
}

/// Record positions one event can reserve. The lexical walk names at most four per site —
/// `immediate`, `deferred`, `incomplete`, and a callable `body` — so this ceiling is generous;
/// it is the width of [`EventMeta::completed`], and passing it is reported, never truncated.
const EVENT_RECORD_CAPACITY: usize = 32;

#[derive(Debug)]
struct EventMeta {
    module_ordinal: ModuleOrdinal,
    source_start: u32,
    event_ordinal: usize,
    next_record_ordinal: usize,
    /// One bit per record position already completed. This carries both duties the old
    /// per-position `Pending` slot carried, at one word per *event* instead of one slot per
    /// position: a set bit rejects a second fill, and `completed == reserved_mask(..)` is
    /// exactly "every reserved position was accounted for", which `finish` audits before it
    /// replays anything. A position reserved and never completed means the checker planned to
    /// check something and did not — that stays observable.
    completed: u32,
}

/// The completed-bit pattern an event reaches only when every position it reserved has been
/// filled exactly once.
const fn reserved_mask(positions: usize) -> u32 {
    if positions >= EVENT_RECORD_CAPACITY {
        u32::MAX
    } else {
        (1u32 << positions) - 1
    }
}

/// Bytes the completion representation materializes for one newly reserved record position.
///
/// **Zero, and that is the property `completion_slot_spec` exists to hold.** Records are stored;
/// positions are not. Reserving one adds a bit to an event's `completed` mask, which
/// [`EventMeta`] already carries whether or not the position is used. Any change that gives a
/// position its own slot again — a map entry, an arena element, a `Vec` of per-position state —
/// must report that slot's size here, or the guard measures nothing.
#[cfg(test)]
const fn reserved_position_bytes() -> usize {
    0
}

/// Reservation or completion misuse. These failures are checker bugs, not user errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EventStoreError {
    UnknownEvent(EventId),
    UnknownRecord(UserRecordTicket),
    DuplicateCompletion(UserRecordTicket),
    RecordCapacity(EventId),
    Unfinished(Vec<EventKey>),
}

/// Checker-wide authority for deterministic records.
#[derive(Debug, Default)]
pub(crate) struct EventStore {
    events: Vec<EventMeta>,
    next_event_ordinal: BTreeMap<ModuleOrdinal, usize>,
    /// Records in completion order, each already carrying the replay key of the position that
    /// produced it. Reservation writes nothing here: a position that never records leaves only
    /// its bit in the owning event's mask.
    records: Vec<(EventKey, CheckerRecord)>,
}

impl EventStore {
    /// Reserve an event and its mandatory primary record position.
    pub(crate) fn reserve_event(
        &mut self,
        module_ordinal: ModuleOrdinal,
        source_start: u32,
    ) -> ReservedEvent {
        #[cfg(test)]
        {
            record_user_event_reservation_for_test();
            record_reserved_event();
            record_reserved_record_position();
        }
        let event_ordinal = self.next_event_ordinal.entry(module_ordinal).or_insert(0);
        let id = EventId(self.events.len());
        let primary = UserRecordTicket {
            event: id,
            record_ordinal: 0,
        };
        let assigned_ordinal = *event_ordinal;
        *event_ordinal += 1;
        self.events.push(EventMeta {
            module_ordinal,
            source_start,
            event_ordinal: assigned_ordinal,
            next_record_ordinal: 1,
            completed: 0,
        });
        #[cfg(test)]
        record_materialized_completion_slot(reserved_position_bytes());
        ReservedEvent { id, primary }
    }

    /// Add another ordered record position to an existing event.
    pub(crate) fn reserve_record(
        &mut self,
        event: EventId,
    ) -> Result<UserRecordTicket, EventStoreError> {
        let Some(meta) = self.events.get_mut(event.index()) else {
            return Err(EventStoreError::UnknownEvent(event));
        };
        if meta.next_record_ordinal >= EVENT_RECORD_CAPACITY {
            return Err(EventStoreError::RecordCapacity(event));
        }
        // Secondary record positions are semantic reservations too.
        #[cfg(test)]
        {
            record_user_event_reservation_for_test();
            record_reserved_record_position();
        }
        let ticket = UserRecordTicket {
            event,
            record_ordinal: meta.next_record_ordinal,
        };
        meta.next_record_ordinal += 1;
        #[cfg(test)]
        record_materialized_completion_slot(reserved_position_bytes());
        Ok(ticket)
    }

    /// Complete exactly one reserved position with zero or more ordered records.
    pub(crate) fn complete(
        &mut self,
        ticket: UserRecordTicket,
        records: Vec<CheckerRecord>,
    ) -> Result<(), EventStoreError> {
        let Some(meta) = self.events.get_mut(ticket.event.index()) else {
            return Err(EventStoreError::UnknownEvent(ticket.event));
        };
        if ticket.record_ordinal >= meta.next_record_ordinal {
            return Err(EventStoreError::UnknownRecord(ticket));
        }
        // `record_ordinal` is below `next_record_ordinal`, which `reserve_record` keeps at or
        // under `EVENT_RECORD_CAPACITY`, so the shift stays inside the mask.
        let filled = 1u32 << ticket.record_ordinal;
        if meta.completed & filled != 0 {
            return Err(EventStoreError::DuplicateCompletion(ticket));
        }
        meta.completed |= filled;
        let key = EventKey {
            module_ordinal: meta.module_ordinal,
            source_start: meta.source_start,
            event_ordinal: meta.event_ordinal,
            record_ordinal: ticket.record_ordinal,
        };
        self.records
            .extend(records.into_iter().map(|record| (key, record)));
        Ok(())
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
    #[cfg(test)]
    pub(crate) fn commit(&mut self, effects: CandidateEffects) -> Result<(), EventStoreError> {
        self.complete(effects.owner, effects.records)
    }

    /// Replay completed records in their reserved total order.
    pub(crate) fn finish(mut self) -> Result<Vec<(EventKey, CheckerRecord)>, EventStoreError> {
        let unfinished = self.unfinished_positions();
        if !unfinished.is_empty() {
            return Err(EventStoreError::Unfinished(unfinished));
        }
        // Replay order is the four-key order and nothing else. Three facts make this stable
        // sort identical to iterating a map keyed by that tuple, without appealing to any
        // corpus:
        //
        // 1. A position's key is fixed when the lexical walk reserves it — `event_ordinal` by
        //    the event's position within its module, `record_ordinal` by the position's index
        //    within its event. Nothing after reservation can move a key, so completion order,
        //    SCC order, dependency slot, query order and cache hits cannot reach it.
        // 2. Each key is filled exactly once (`EventMeta::completed` rejects the second fill),
        //    so all records sharing a key came from one `complete` call and sit contiguously
        //    here, in that call's vector order.
        // 3. `sort_by_key` is stable: distinct keys come out in four-key order, and equal keys
        //    keep push order — which by (2) is the producer order within one position.
        self.records.sort_by_key(|(key, _)| *key);
        #[cfg(test)]
        record_replayed_records(self.records.len());
        Ok(self.records)
    }

    /// Positions that were reserved and never filled, in replay-key order.
    ///
    /// A reserved position that no owner completed means the checker named something to check
    /// and then did not — the silent-hole class this project over-reports to avoid — so the
    /// audit survives the storage change: the mask reconstructs it exactly, because positions
    /// are dense from `0` and every fill sets its bit.
    fn unfinished_positions(&self) -> Vec<EventKey> {
        let mut unfinished = Vec::new();
        for meta in &self.events {
            if meta.completed == reserved_mask(meta.next_record_ordinal) {
                continue;
            }
            for record_ordinal in 0..meta.next_record_ordinal {
                if meta.completed & (1u32 << record_ordinal) == 0 {
                    unfinished.push(EventKey {
                        module_ordinal: meta.module_ordinal,
                        source_start: meta.source_start,
                        event_ordinal: meta.event_ordinal,
                        record_ordinal,
                    });
                }
            }
        }
        unfinished.sort_unstable();
        unfinished
    }

    #[cfg(test)]
    pub(crate) fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Heap slots the store has allocated for records. Reservation must never move this.
    #[cfg(test)]
    fn stored_record_capacity(&self) -> usize {
        self.records.capacity()
    }

    /// Record positions reserved across every event — what the store has promised to account
    /// for, not what it has stored.
    #[cfg(test)]
    pub(crate) fn record_count(&self) -> usize {
        self.events
            .iter()
            .map(|meta| meta.next_record_ordinal)
            .sum()
    }
}

/// Records accumulated by one speculative candidate before the winner commits.
#[derive(Debug)]
pub(crate) struct CandidateEffects {
    owner: UserRecordTicket,
    records: Vec<CheckerRecord>,
}

impl CandidateEffects {
    pub(crate) fn new(owner: UserRecordTicket) -> Self {
        Self {
            owner,
            records: Vec::new(),
        }
    }

    pub(crate) fn diagnostic(&mut self, diagnostic: Diagnostic) {
        self.records.push(CheckerRecord::Diagnostic(diagnostic));
    }

    #[cfg(test)]
    pub(crate) fn incomplete(&mut self, incomplete: IncompleteSurface) {
        self.records.push(CheckerRecord::Incomplete(incomplete));
    }

    #[cfg(test)]
    pub(crate) fn merge(&mut self, child: CandidateEffects) {
        assert_eq!(
            self.owner, child.owner,
            "nested effects must share one owner"
        );
        self.records.extend(child.records);
    }

    pub(crate) fn into_parts(self) -> (UserRecordTicket, Vec<CheckerRecord>) {
        (self.owner, self.records)
    }

    #[cfg(test)]
    pub(crate) fn discard(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagnosticCode;
    use crate::span::Span;

    #[test]
    fn reservation_calibration_exercises_event_and_secondary_record_hooks() {
        assert_eq!(
            calibrate_user_event_reservations_for_test(),
            UserEventReservationCalibrationForTest {
                event: 1,
                record: 1,
            }
        );
    }

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

    /// Records now reach the store interleaved — several per position, positions completed in
    /// any order — and only the four-key order may decide how they come back out. This pins the
    /// two halves of that at once: across positions the key decides, and within one position the
    /// producer's order survives untouched, with unrelated records landing in between.
    #[test]
    fn interleaved_multi_record_completions_replay_in_four_key_order() {
        let mut store = EventStore::default();
        let late = store.reserve_event(ModuleOrdinal::new(1), 5);
        let early = store.reserve_event(ModuleOrdinal::new(0), 30);
        let early_second = store.reserve_record(early.id).unwrap();
        let earliest = store.reserve_event(ModuleOrdinal::new(0), 10);

        // Completion order is deliberately the reverse of replay order, and each call carries
        // more than one record so intra-position order is observable.
        store
            .complete(
                early_second,
                vec![
                    diagnostic("early-second-a", 30),
                    diagnostic("early-second-b", 30),
                ],
            )
            .unwrap();
        store
            .complete(late.primary, vec![diagnostic("late-a", 5)])
            .unwrap();
        store
            .complete(
                early.primary,
                vec![
                    diagnostic("early-first-a", 30),
                    diagnostic("early-first-b", 30),
                ],
            )
            .unwrap();
        store
            .complete(
                earliest.primary,
                vec![diagnostic("earliest-a", 10), diagnostic("earliest-b", 10)],
            )
            .unwrap();

        let names: Vec<String> = store
            .finish()
            .unwrap()
            .into_iter()
            .map(|(_, record)| diagnostic_name(record))
            .collect();
        assert_eq!(
            names,
            [
                "Cannot find name 'earliest-a'",
                "Cannot find name 'earliest-b'",
                "Cannot find name 'early-first-a'",
                "Cannot find name 'early-first-b'",
                "Cannot find name 'early-second-a'",
                "Cannot find name 'early-second-b'",
                "Cannot find name 'late-a'",
            ],
            "module ordinal, then source start, then record ordinal, then producer order"
        );
    }

    /// The unfinished audit is the tripwire for a position the checker named and then never
    /// checked. It has to name **every** such position, from any event, in replay-key order —
    /// not merely notice that one exists.
    #[test]
    fn the_unfinished_audit_names_every_never_filled_position_in_replay_order() {
        let mut store = EventStore::default();
        // Reserved and never filled at all.
        store.reserve_event(ModuleOrdinal::new(0), 40);
        let earlier = store.reserve_event(ModuleOrdinal::new(0), 10);
        // Reserved and never filled, with a filled position on either side of it.
        store.reserve_record(earlier.id).unwrap();
        let earlier_third = store.reserve_record(earlier.id).unwrap();

        // Fill out of order, so nothing about the audit can depend on completion order.
        store.complete(earlier_third, Vec::new()).unwrap();
        store.complete(earlier.primary, Vec::new()).unwrap();

        let key = |source_start, event_ordinal, record_ordinal| EventKey {
            module_ordinal: ModuleOrdinal::new(0),
            source_start,
            event_ordinal,
            record_ordinal,
        };
        let Err(EventStoreError::Unfinished(keys)) = store.finish() else {
            panic!("a never-filled position must fail the audit");
        };
        assert_eq!(
            keys,
            vec![key(10, 1, 1), key(40, 0, 0)],
            "an interior hole and a wholly unfilled event must both be reported, keyed"
        );
    }

    /// The completion mask is finite, so the store declares its width instead of silently
    /// dropping a position that would fall outside it. Production names at most four positions
    /// per event, so this reports a checker bug, never a program.
    #[test]
    fn reserving_past_the_record_capacity_is_reported() {
        let mut store = EventStore::default();
        let event = store.reserve_event(ModuleOrdinal::new(0), 10);
        for _ in 1..EVENT_RECORD_CAPACITY {
            store
                .reserve_record(event.id)
                .expect("positions below the capacity reserve");
        }
        assert_eq!(store.record_count(), EVENT_RECORD_CAPACITY);
        assert_eq!(
            store.reserve_record(event.id),
            Err(EventStoreError::RecordCapacity(event.id))
        );

        // The audit still holds at the boundary: a full event needs every one of its bits.
        for record_ordinal in 0..EVENT_RECORD_CAPACITY {
            store
                .complete(
                    UserRecordTicket {
                        event: event.id,
                        record_ordinal,
                    },
                    Vec::new(),
                )
                .expect("each reserved position completes once");
        }
        assert!(store.finish().unwrap().is_empty());
    }

    /// Reservation must buy storage for records, not for positions. The store's record vector
    /// is the only thing that grows with what a program says; a silent program leaves it empty
    /// however many positions its lexical walk named.
    #[test]
    fn reserving_positions_stores_nothing_until_a_record_arrives() {
        let mut store = EventStore::default();
        let mut tickets = Vec::new();
        for index in 0..64 {
            let event = store.reserve_event(ModuleOrdinal::new(0), index * 10);
            tickets.push(event.primary);
            tickets.push(store.reserve_record(event.id).unwrap());
            tickets.push(store.reserve_record(event.id).unwrap());
        }
        assert_eq!(store.record_count(), 192);
        assert_eq!(
            store.stored_record_capacity(),
            0,
            "192 reserved positions must not allocate record storage before any record exists"
        );

        for ticket in tickets {
            store.complete(ticket, Vec::new()).unwrap();
        }
        assert_eq!(
            store.stored_record_capacity(),
            0,
            "completing every position with nothing must still store nothing"
        );
        assert!(store.finish().unwrap().is_empty());
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

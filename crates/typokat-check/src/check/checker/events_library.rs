//! Deterministic record reservation and replay for library files.

use super::context::CheckerRecordBatch;
use super::replay_index::{
    CollisionReplayEventPhase, CollisionReplayOwnerSite, CollisionReplaySiteProvenance,
    OwnerSiteStorageMode, ReplayOwner, ReplayTraceSeed,
};
use super::reporting_record::CheckerRecord;
use crate::diagnostics::Severity;
use crate::source::LibraryFileOrdinal;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static EVENT_CAPTURE_CORRUPTION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) struct EventCaptureCorruptionScope;

#[cfg(any(test, feature = "test-utils"))]
impl EventCaptureCorruptionScope {
    pub(crate) fn start() -> Result<Self, &'static str> {
        if EVENT_CAPTURE_CORRUPTION.replace(true) {
            return Err("event capture corruption is already active");
        }
        Ok(Self)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Drop for EventCaptureCorruptionScope {
    fn drop(&mut self) {
        EVENT_CAPTURE_CORRUPTION.set(false);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibraryEventId(usize);

impl LibraryEventId {
    const fn index(self) -> usize {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibraryRecordTicket {
    pub event: LibraryEventId,
    pub record_ordinal: usize,
}

pub const fn library_record_ticket_key(ticket: LibraryRecordTicket) -> (usize, usize) {
    (ticket.event.index(), ticket.record_ordinal)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LibraryReservedEvent {
    pub id: LibraryEventId,
    pub primary: LibraryRecordTicket,
}

/// The total library replay order. Field order is intentional.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibraryEventKey {
    pub file_ordinal: LibraryFileOrdinal,
    pub source_start: u32,
    pub event_ordinal: usize,
    pub record_ordinal: usize,
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
    Completed {
        records: Vec<CheckerRecord>,
        record_count: u64,
        digest: [u8; 32],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryRecordFingerprint {
    pub key: LibraryEventKey,
    pub record_count: u64,
    pub digest: [u8; 32],
}

pub struct LibraryEventLedgerOutput {
    pub records: Vec<(LibraryEventKey, CheckerRecord)>,
    pub fingerprints: Vec<LibraryRecordFingerprint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryEventLedgerError {
    UnknownEvent(LibraryEventId),
    UnknownRecord(LibraryRecordTicket),
    DuplicateCompletion(LibraryRecordTicket),
    Unfinished(Vec<LibraryEventKey>),
    TraceDomainSealed,
    BinderReportingIncomplete,
    NoncontiguousReplayTicketReservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryEventLedgerSnapshot {
    pub reserved_records: usize,
    pub filled_records: usize,
    pub reserved_file_ordinals: Vec<LibraryFileOrdinal>,
}

#[derive(Debug)]
pub struct LibraryEventLedger {
    events: Vec<LibraryEventMeta>,
    next_event_ordinal: BTreeMap<LibraryFileOrdinal, usize>,
    completions: BTreeMap<LibraryEventKey, LibraryCompletion>,
    replay_trace_seed: Option<ReplayTraceSeed>,
    trace_domain_sealed: bool,
    binder_reporting_complete: bool,
    retain_records: bool,
}

pub(crate) struct LibraryReplayReservationDomain<'ledger> {
    ledger: &'ledger mut LibraryEventLedger,
}

impl Default for LibraryEventLedger {
    fn default() -> Self {
        Self::new(true)
    }
}

impl LibraryEventLedger {
    pub fn new(retain_records: bool) -> Self {
        Self::new_with_owner_site_storage(retain_records, OwnerSiteStorageMode::Flat)
    }

    pub fn new_with_owner_site_storage(
        retain_records: bool,
        owner_site_storage_mode: OwnerSiteStorageMode,
    ) -> Self {
        Self {
            events: Vec::new(),
            next_event_ordinal: BTreeMap::new(),
            completions: BTreeMap::new(),
            replay_trace_seed: Some(ReplayTraceSeed::new(owner_site_storage_mode)),
            trace_domain_sealed: false,
            binder_reporting_complete: false,
            retain_records,
        }
    }

    pub fn new_without_replay(retain_records: bool) -> Self {
        Self {
            events: Vec::new(),
            next_event_ordinal: BTreeMap::new(),
            completions: BTreeMap::new(),
            replay_trace_seed: None,
            trace_domain_sealed: false,
            binder_reporting_complete: false,
            retain_records,
        }
    }

    pub(crate) fn replay_reservation_domain(
        &mut self,
    ) -> Result<LibraryReplayReservationDomain<'_>, LibraryEventLedgerError> {
        if self.trace_domain_sealed {
            return Err(LibraryEventLedgerError::TraceDomainSealed);
        }
        Ok(LibraryReplayReservationDomain { ledger: self })
    }

    pub(crate) fn mark_binder_reporting_complete(&mut self) {
        self.binder_reporting_complete = true;
    }

    pub fn take_replay_trace_seed(&mut self) -> Result<ReplayTraceSeed, LibraryEventLedgerError> {
        if !self.binder_reporting_complete {
            return Err(LibraryEventLedgerError::BinderReportingIncomplete);
        }
        let Some(seed) = self.replay_trace_seed.as_mut() else {
            return Err(LibraryEventLedgerError::TraceDomainSealed);
        };
        if !seed.ticket_reservations_are_valid() {
            return Err(LibraryEventLedgerError::NoncontiguousReplayTicketReservation);
        }
        self.trace_domain_sealed = true;
        let missing_owner_site_count = seed.missing_owner_site_ticket_count();
        seed.missing_owner_site_count = u64::try_from(missing_owner_site_count).unwrap_or(u64::MAX);
        seed.trace_domain_sealed_after_binder_reporting = true;
        self.replay_trace_seed
            .take()
            .ok_or(LibraryEventLedgerError::TraceDomainSealed)
    }

    pub fn seal_reporting_without_replay(&mut self) -> Result<(), LibraryEventLedgerError> {
        if !self.binder_reporting_complete {
            return Err(LibraryEventLedgerError::BinderReportingIncomplete);
        }
        if self.replay_trace_seed.is_some() {
            return Err(LibraryEventLedgerError::NoncontiguousReplayTicketReservation);
        }
        self.trace_domain_sealed = true;
        Ok(())
    }

    fn record_replay_owner_site(
        &mut self,
        ticket: LibraryRecordTicket,
        span: crate::span::Span,
        phase: CollisionReplayEventPhase,
    ) {
        #[cfg(any(test, feature = "test-utils"))]
        let phase = if EVENT_CAPTURE_CORRUPTION.get() {
            match phase {
                CollisionReplayEventPhase::Immediate => CollisionReplayEventPhase::Deferred,
                CollisionReplayEventPhase::Deferred => CollisionReplayEventPhase::Incomplete,
                CollisionReplayEventPhase::Incomplete | CollisionReplayEventPhase::Body => {
                    CollisionReplayEventPhase::Immediate
                }
            }
        } else {
            phase
        };
        if self.replay_trace_seed.is_none() {
            return;
        }
        let Ok(key) = self.key(ticket) else {
            let Some(seed) = self.replay_trace_seed.as_mut() else {
                return;
            };
            seed.duplicate_owner_site_count = seed.duplicate_owner_site_count.saturating_add(1);
            return;
        };
        let site = CollisionReplayOwnerSite {
            owner: ReplayOwner::Statement(key),
            file_ordinal: key.file_ordinal,
            span,
            provenance: CollisionReplaySiteProvenance::Event { phase },
        };
        let (event, record) = library_record_ticket_key(ticket);
        let Some(seed) = self.replay_trace_seed.as_mut() else {
            return;
        };
        let duplicate = seed.record_ticket_owner_site((event, record), site);
        if duplicate != Some(false) {
            seed.duplicate_owner_site_count = seed.duplicate_owner_site_count.saturating_add(1);
        }
    }

    fn reserve_event_internal(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        source_start: u32,
    ) -> LibraryReservedEvent {
        self.reserve_event_internal_with_completion(file_ordinal, source_start, true)
    }

    fn reserve_event_internal_with_completion(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        source_start: u32,
        reserve_completion: bool,
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
        if reserve_completion {
            self.completions.insert(key, LibraryCompletion::Pending);
        }
        let ticket_key = (id.index(), primary.record_ordinal);
        if let Some(seed) = self.replay_trace_seed.as_mut() {
            seed.reserve_owner_site_ticket(ticket_key, key);
        }
        LibraryReservedEvent { id, primary }
    }

    fn reserve_record_internal(
        &mut self,
        event: LibraryEventId,
    ) -> Result<LibraryRecordTicket, LibraryEventLedgerError> {
        self.reserve_record_internal_with_completion(event, true)
    }

    fn reserve_record_internal_with_completion(
        &mut self,
        event: LibraryEventId,
        reserve_completion: bool,
    ) -> Result<LibraryRecordTicket, LibraryEventLedgerError> {
        let Some(meta) = self.events.get(event.index()) else {
            return Err(LibraryEventLedgerError::UnknownEvent(event));
        };
        let ticket = LibraryRecordTicket {
            event,
            record_ordinal: meta.next_record_ordinal,
        };
        let key = LibraryEventKey {
            file_ordinal: meta.file_ordinal,
            source_start: meta.source_start,
            event_ordinal: meta.event_ordinal,
            record_ordinal: ticket.record_ordinal,
        };
        let ticket_key = (event.index(), ticket.record_ordinal);
        if let Some(seed) = self.replay_trace_seed.as_mut() {
            if !seed.reserve_owner_site_ticket(ticket_key, key) {
                return Err(LibraryEventLedgerError::NoncontiguousReplayTicketReservation);
            }
        }
        let Some(meta) = self.events.get_mut(event.index()) else {
            return Err(LibraryEventLedgerError::UnknownEvent(event));
        };
        meta.next_record_ordinal += 1;
        if reserve_completion {
            self.completions.insert(key, LibraryCompletion::Pending);
        }
        Ok(ticket)
    }

    #[cfg(test)]
    pub fn reserve_event(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        source_start: u32,
    ) -> LibraryReservedEvent {
        self.reserve_event_internal(file_ordinal, source_start)
    }

    #[cfg(test)]
    pub fn reserve_record(
        &mut self,
        event: LibraryEventId,
    ) -> Result<LibraryRecordTicket, LibraryEventLedgerError> {
        self.reserve_record_internal(event)
    }

    pub fn complete(
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
                let record_count = u64::try_from(records.len()).unwrap_or(u64::MAX);
                let digest = structured_record_digest(&records);
                let records = if self.retain_records {
                    records
                } else {
                    Vec::new()
                };
                *completion = LibraryCompletion::Completed {
                    records,
                    record_count,
                    digest,
                };
                Ok(())
            }
            LibraryCompletion::Completed { .. } => {
                Err(LibraryEventLedgerError::DuplicateCompletion(ticket))
            }
        }
    }

    pub fn finish(self) -> Result<Vec<(LibraryEventKey, CheckerRecord)>, LibraryEventLedgerError> {
        Ok(self.finish_with_fingerprints()?.records)
    }

    pub(crate) fn reserved_record_keys(&self) -> Vec<LibraryEventKey> {
        self.completions.keys().copied().collect()
    }

    pub fn finish_with_fingerprints(
        self,
    ) -> Result<LibraryEventLedgerOutput, LibraryEventLedgerError> {
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

        let mut drained_records = Vec::new();
        let mut fingerprints = Vec::new();
        for (key, completion) in self.completions {
            let LibraryCompletion::Completed {
                records,
                record_count,
                digest,
            } = completion
            else {
                continue;
            };
            if record_count != 0 {
                fingerprints.push(LibraryRecordFingerprint {
                    key,
                    record_count,
                    digest,
                });
            }
            drained_records.extend(records.into_iter().map(|record| (key, record)));
        }
        Ok(LibraryEventLedgerOutput {
            records: drained_records,
            fingerprints,
        })
    }

    pub fn snapshot(&self) -> LibraryEventLedgerSnapshot {
        LibraryEventLedgerSnapshot {
            reserved_records: self.completions.len(),
            filled_records: self
                .completions
                .values()
                .filter(|completion| matches!(completion, LibraryCompletion::Completed { .. }))
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
            if matches!(completion, LibraryCompletion::Completed { .. }) {
                return Err(LibraryEventLedgerError::DuplicateCompletion(ticket));
            }
        }
        Ok(())
    }

    pub(crate) fn key(
        &self,
        ticket: LibraryRecordTicket,
    ) -> Result<LibraryEventKey, LibraryEventLedgerError> {
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

impl LibraryReplayReservationDomain<'_> {
    pub(crate) fn reserve_event(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        source_start: u32,
    ) -> LibraryReservedEvent {
        self.ledger
            .reserve_event_internal(file_ordinal, source_start)
    }

    pub(crate) fn reserve_record(
        &mut self,
        event: LibraryEventId,
    ) -> Result<LibraryRecordTicket, LibraryEventLedgerError> {
        self.ledger.reserve_record_internal(event)
    }

    pub(crate) fn reserve_event_with_omission(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        source_start: u32,
        omitted: Option<LibraryEventKey>,
    ) -> (LibraryReservedEvent, bool) {
        let event_ordinal = self
            .ledger
            .next_event_ordinal
            .get(&file_ordinal)
            .copied()
            .unwrap_or_default();
        let key = LibraryEventKey {
            file_ordinal,
            source_start,
            event_ordinal,
            record_ordinal: 0,
        };
        let disabled = omitted == Some(key);
        (
            self.ledger.reserve_event_internal_with_completion(
                file_ordinal,
                source_start,
                !disabled,
            ),
            disabled,
        )
    }

    pub(crate) fn reserve_record_with_omission(
        &mut self,
        event: LibraryEventId,
        omitted: Option<LibraryEventKey>,
    ) -> Result<(LibraryRecordTicket, bool), LibraryEventLedgerError> {
        let Some(meta) = self.ledger.events.get(event.index()) else {
            return Err(LibraryEventLedgerError::UnknownEvent(event));
        };
        let key = LibraryEventKey {
            file_ordinal: meta.file_ordinal,
            source_start: meta.source_start,
            event_ordinal: meta.event_ordinal,
            record_ordinal: meta.next_record_ordinal,
        };
        let disabled = omitted == Some(key);
        self.ledger
            .reserve_record_internal_with_completion(event, !disabled)
            .map(|ticket| (ticket, disabled))
    }

    pub(crate) fn record_replay_owner_site(
        &mut self,
        ticket: LibraryRecordTicket,
        span: crate::span::Span,
        phase: CollisionReplayEventPhase,
    ) {
        self.ledger.record_replay_owner_site(ticket, span, phase);
    }
}

fn structured_record_digest(records: &[CheckerRecord]) -> [u8; 32] {
    fn bytes(hasher: &mut Sha256, value: &[u8]) {
        hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(value);
    }

    fn span(hasher: &mut Sha256, span: crate::span::Span) {
        hasher.update(span.start.to_be_bytes());
        hasher.update(span.end.to_be_bytes());
    }

    let mut hasher = Sha256::new();
    hasher.update(b"typokat/collision-record-fingerprint/v1");
    hasher.update(
        u64::try_from(records.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for record in records {
        match record {
            CheckerRecord::Diagnostic(diagnostic) => {
                hasher.update([0]);
                bytes(&mut hasher, diagnostic.code.as_str().as_bytes());
                hasher.update([match diagnostic.severity {
                    Severity::Error => 0,
                    Severity::Warning => 1,
                }]);
                bytes(&mut hasher, diagnostic.message.as_bytes());
                span(&mut hasher, diagnostic.span);
                hasher.update(
                    u64::try_from(diagnostic.elaboration().len())
                        .unwrap_or(u64::MAX)
                        .to_be_bytes(),
                );
                for line in diagnostic.elaboration() {
                    bytes(&mut hasher, line.as_bytes());
                }
            }
            CheckerRecord::Incomplete(incomplete) => {
                hasher.update([1]);
                bytes(&mut hasher, incomplete.id.as_bytes());
                span(&mut hasher, incomplete.span);
                bytes(&mut hasher, incomplete.context.as_bytes());
            }
        }
    }
    hasher.finalize().into()
}

#[cfg(any(test, feature = "test-utils"))]
pub fn duplicate_owner_site_write_control_for_test() -> Result<(bool, bool), LibraryEventLedgerError>
{
    let mut all_rejected = true;
    let mut all_first_preserved = true;
    for mode in [
        OwnerSiteStorageMode::Flat,
        OwnerSiteStorageMode::Nested,
        OwnerSiteStorageMode::Ordered,
    ] {
        let mut ledger = LibraryEventLedger::new_with_owner_site_storage(false, mode);
        let event = ledger.reserve_event_internal(LibraryFileOrdinal::new(0), 5);
        let key = ledger.key(event.primary)?;
        let first = CollisionReplayOwnerSite {
            owner: ReplayOwner::Statement(key),
            file_ordinal: key.file_ordinal,
            span: crate::span::Span::new(5, 7),
            provenance: CollisionReplaySiteProvenance::Event {
                phase: CollisionReplayEventPhase::Immediate,
            },
        };
        ledger.record_replay_owner_site(
            event.primary,
            first.span,
            CollisionReplayEventPhase::Immediate,
        );
        ledger.record_replay_owner_site(
            event.primary,
            crate::span::Span::new(5, 9),
            CollisionReplayEventPhase::Deferred,
        );
        ledger.mark_binder_reporting_complete();
        let seed = ledger.take_replay_trace_seed()?;
        let (rejected, first_preserved) =
            seed.duplicate_write_control_for_test(library_record_ticket_key(event.primary), &first);
        all_rejected &= rejected;
        all_first_preserved &= first_preserved;
    }
    Ok((all_rejected, all_first_preserved))
}

#[cfg(any(test, feature = "test-utils"))]
pub fn elaboration_fingerprint_negative_control_for_test(
) -> Result<Option<[u8; 32]>, LibraryEventLedgerError> {
    fn fingerprint(
        diagnostic: crate::diagnostics::Diagnostic,
    ) -> Result<Option<[u8; 32]>, LibraryEventLedgerError> {
        let mut ledger = LibraryEventLedger::default();
        let event = ledger
            .replay_reservation_domain()?
            .reserve_event(LibraryFileOrdinal::new(0), 7);
        ledger.complete(event.primary, vec![CheckerRecord::Diagnostic(diagnostic)])?;
        let output = ledger.finish_with_fingerprints()?;
        Ok(output
            .fingerprints
            .first()
            .map(|fingerprint| fingerprint.digest))
    }

    let diagnostic =
        crate::diagnostics::Diagnostic::no_overload_matches(crate::span::Span::new(7, 9));
    let without = fingerprint(diagnostic.clone())?;
    let with = fingerprint(diagnostic.with_elaboration(vec![
        "  Type 'number' is not assignable to type 'string'.".to_owned(),
    ]))?;
    Ok(match (without, with) {
        (Some(without), Some(with)) if without != with => Some(with),
        _ => None,
    })
}

pub struct LibrarySemanticReportingAdapter<'ledger> {
    ledger: &'ledger mut LibraryEventLedger,
}

impl<'ledger> LibrarySemanticReportingAdapter<'ledger> {
    pub fn new(ledger: &'ledger mut LibraryEventLedger) -> Self {
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
    fn flat_replay_tickets_reject_records_after_a_later_event_starts() {
        let mut ledger = LibraryEventLedger::default();
        let first = ledger.reserve_event(LibraryFileOrdinal::new(0), 10);
        let _second = ledger.reserve_event(LibraryFileOrdinal::new(0), 20);

        assert_eq!(
            ledger.reserve_record(first.id),
            Err(LibraryEventLedgerError::NoncontiguousReplayTicketReservation)
        );
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

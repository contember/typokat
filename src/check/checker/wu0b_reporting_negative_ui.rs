//! Negative WU0B UI probes.
//!
//! Temporarily activate this sibling from `checker/mod.rs`; every function must fail with a
//! concrete domain type mismatch, never a privacy error.

use super::events::{EventKey, EventStore, UserRecordTicket};
use super::events_library::{LibraryEventKey, LibraryEventLedger, LibraryRecordTicket};
use super::reporting_record::CheckerRecord;
use crate::source::{LibraryFileOrdinal, ModuleOrdinal};

fn user_complete_rejects_library_ticket(users: &mut EventStore, library: LibraryRecordTicket) {
    let _ = users.complete(library, Vec::new());
}

fn library_complete_rejects_user_ticket(library: &mut LibraryEventLedger, user: UserRecordTicket) {
    let _ = library.complete(user, Vec::new());
}

fn user_reserve_rejects_library_ordinal(users: &mut EventStore) {
    let _ = users.reserve_event(LibraryFileOrdinal::new(0), 0);
}

fn library_reserve_rejects_user_ordinal(library: &mut LibraryEventLedger) {
    let _ = library.reserve_event(ModuleOrdinal::new(0), 0);
}

fn user_key_stream_rejects_library_records(library: LibraryEventLedger) {
    let records = library.finish();
    let Ok(records) = records else {
        return;
    };
    let _: Vec<(EventKey, CheckerRecord)> = records;
}

fn library_key_stream_rejects_user_records(users: EventStore) {
    let records = users.finish();
    let Ok(records) = records else {
        return;
    };
    let _: Vec<(LibraryEventKey, CheckerRecord)> = records;
}

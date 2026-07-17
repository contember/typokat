//! Disabled WU0B RED spec for source and reporting ownership.
//!
//! Activate with `#[cfg(test)] mod wu0b_reporting_spec;` in `checker/mod.rs` after the
//! neutral source types and both reporting authorities exist.
//! During review, temporarily activate `wu0b_reporting_negative_ui` at the same location;
//! compilation must fail on all six concrete domain inversions in that fixture.
//!
//! | Contract | Executable or review witness |
//! | --- | --- |
//! | Cross-domain operations are rejected | Concrete signature guards plus `wu0b_reporting_negative_ui.rs` cover both completion inversions, both reserve inversions, and both key-stream assignments. |
//! | `crate::source` stays acyclic | Existing `ModuleOrdinal`, `UnitSlot`, and `OriginalModuleOrdinal` move into the neutral module with the new source tags. |
//! | Library storage is native | `events_library.rs` owns library IDs, metadata, completion storage, and its exact `BTreeMap<LibraryEventKey, _>` without importing or wrapping the user ledger. |
//! | No post-hoc retag or split | Each authority reserves its final ordinal domain and finishes its own typed key stream. |
//! | The injected seam is real | `wu0b_library::run_injected_profile` drives `LibraryReportingConsumer`, returns all phase counters and typed binder receipts, and exposes exact library-ledger ownership. |
//! | Script globals are one semantic domain | Runtime probes expose real binder identities and published surfaces for cross-file interface and function/namespace merges. |
//! | Module privacy and global augmentation are exact | An external module keeps private declarations in its module scope while its `declare global` fragments reopen the shared compilation global. |
//! | Checker ownership is source-aware | The injected Pass and every lexical reservation retain `SourceUnit::Library`; no fabricated user module ordinal or execution slot enters the library path. |
//!
//! The neutral source types and origin-bearing binder records are deliberately crate-private.
//! WU0B is authorized to clean up those internal APIs; only the public driver/checker behavior and
//! default path are frozen. `SourceUnitKey` remains binder-local so private provenance cannot leak
//! through a public field.

use super::events::{EventKey, EventStore, EventStoreError, UserRecordTicket};
use super::events_library::{
    LibraryEventKey, LibraryEventLedger, LibraryEventLedgerError, LibraryRecordTicket,
};
use super::library_reporting::LibraryReportingFamily;
use super::reporting_record::CheckerRecord;
use super::wu0b_library::{run_injected_profile, InjectedLibrarySource};
use crate::binder::declaration::{TypeGroupId, ValueStorageId};
use crate::diagnostics::{DiagnosticCode, IncompleteSurface};
use crate::driver::{FileInput, FileReport};
use crate::source::{LibraryFileOrdinal, ModuleOrdinal, SourceOrdinal, SourceUnit, UnitSlot};
use crate::span::Span;
use std::collections::BTreeSet;
use std::path::Path;

trait NominalTicketDomain {}

impl NominalTicketDomain for UserRecordTicket {}
impl NominalTicketDomain for LibraryRecordTicket {}

trait NominalOrdinalDomain {}

impl NominalOrdinalDomain for ModuleOrdinal {}
impl NominalOrdinalDomain for LibraryFileOrdinal {}

trait NominalReportingAuthority {}

impl NominalReportingAuthority for EventStore {}
impl NominalReportingAuthority for LibraryEventLedger {}

trait NominalEventKeyDomain {}

impl NominalEventKeyDomain for EventKey {}
impl NominalEventKeyDomain for LibraryEventKey {}

fn assert_ticket_domain<T: NominalTicketDomain>() {}

fn assert_ordinal_domain<T: NominalOrdinalDomain>() {}

fn assert_reporting_authority<T: NominalReportingAuthority>() {}

fn assert_event_key_domain<T: NominalEventKeyDomain>() {}

fn reserve_user_primary(
    store: &mut EventStore,
    module_ordinal: ModuleOrdinal,
    source_start: u32,
) -> UserRecordTicket {
    store.reserve_event(module_ordinal, source_start).primary
}

fn complete_user(
    store: &mut EventStore,
    ticket: UserRecordTicket,
    records: Vec<CheckerRecord>,
) -> Result<(), EventStoreError> {
    store.complete(ticket, records)
}

fn finish_user(store: EventStore) -> Result<Vec<(EventKey, CheckerRecord)>, EventStoreError> {
    store.finish()
}

fn reserve_library_primary(
    ledger: &mut LibraryEventLedger,
    file_ordinal: LibraryFileOrdinal,
    source_start: u32,
) -> LibraryRecordTicket {
    ledger.reserve_event(file_ordinal, source_start).primary
}

fn complete_library(
    ledger: &mut LibraryEventLedger,
    ticket: LibraryRecordTicket,
    records: Vec<CheckerRecord>,
) -> Result<(), LibraryEventLedgerError> {
    ledger.complete(ticket, records)
}

fn finish_library(
    ledger: LibraryEventLedger,
) -> Result<Vec<(LibraryEventKey, CheckerRecord)>, LibraryEventLedgerError> {
    ledger.finish()
}

fn incomplete(id: &str, start: u32) -> CheckerRecord {
    CheckerRecord::Incomplete(IncompleteSurface::new(
        id,
        Span::new(start, start + 1),
        "WU0B reporting ownership witness",
    ))
}

fn user_key_tuple(key: &EventKey) -> (usize, u32, usize, usize) {
    (
        key.module_ordinal.index(),
        key.source_start,
        key.event_ordinal,
        key.record_ordinal,
    )
}

fn library_key_tuple(key: &LibraryEventKey) -> (usize, u32, usize, usize) {
    (
        key.file_ordinal.index(),
        key.source_start,
        key.event_ordinal,
        key.record_ordinal,
    )
}

fn incomplete_id(record: &CheckerRecord) -> Option<&str> {
    match record {
        CheckerRecord::Incomplete(incomplete) => Some(&incomplete.id),
        CheckerRecord::Diagnostic(_) => None,
    }
}

fn record_span(record: &CheckerRecord) -> Span {
    match record {
        CheckerRecord::Diagnostic(diagnostic) => diagnostic.span,
        CheckerRecord::Incomplete(incomplete) => incomplete.span,
    }
}

fn contains_rust_identifier(source: &str, expected: &str) -> bool {
    source
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .any(|identifier| identifier == expected)
}

fn without_whitespace(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn use_statements(source: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut collecting = false;

    for line in source.lines() {
        let trimmed = line.trim();
        let starts_use = trimmed.starts_with("use ")
            || trimmed.starts_with("pub use ")
            || trimmed.starts_with("pub(crate) use ")
            || trimmed.starts_with("pub(super) use ")
            || trimmed.starts_with("pub(self) use ")
            || (trimmed.starts_with("pub(in ") && trimmed.contains(") use "));
        if !collecting && starts_use {
            collecting = true;
        }
        if !collecting {
            continue;
        }
        current.push_str(trimmed);
        current.push(' ');
        if trimmed.contains(';') {
            statements.push(std::mem::take(&mut current));
            collecting = false;
        }
    }

    if collecting {
        statements.push(current);
    }
    statements
}

struct OwnedProfileSource {
    file_ordinal: LibraryFileOrdinal,
    name: String,
    source: String,
}

fn validated_profile_sources() -> Result<Vec<OwnedProfileSource>, String> {
    let manifest = include_str!("../../library/typescript-6.0.3/profile.toml")
        .parse::<toml::Value>()
        .map_err(|error| format!("profile.toml must parse: {error}"))?;
    let files = manifest
        .get("file")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "profile.toml must contain [[file]] rows".to_string())?;
    if files.len() != 82 {
        return Err(format!(
            "profile.toml must contain 82 rows, found {}",
            files.len()
        ));
    }

    let library_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/library/typescript-6.0.3/lib");
    let mut sources = Vec::with_capacity(files.len());
    for (position, row) in files.iter().enumerate() {
        let row = row
            .as_table()
            .ok_or_else(|| format!("profile row {position} must be a table"))?;
        let declared = row
            .get("ordinal")
            .and_then(toml::Value::as_integer)
            .ok_or_else(|| format!("profile row {position} must have an ordinal"))?;
        let declared = usize::try_from(declared)
            .map_err(|_| format!("profile row {position} ordinal must fit usize"))?;
        if declared != position {
            return Err(format!(
                "profile row {position} declares ordinal {declared}"
            ));
        }
        let name = row
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| format!("profile row {position} must have a name"))?;
        let source = std::fs::read_to_string(library_dir.join(name))
            .map_err(|error| format!("cannot read profile row {position} {name}: {error}"))?;
        sources.push(OwnedProfileSource {
            file_ordinal: LibraryFileOrdinal::new(position),
            name: name.to_string(),
            source,
        });
    }
    Ok(sources)
}

#[test]
fn ticket_ordinal_and_authority_domains_are_nominally_distinct() {
    assert_ticket_domain::<UserRecordTicket>();
    assert_ticket_domain::<LibraryRecordTicket>();
    assert_ordinal_domain::<ModuleOrdinal>();
    assert_ordinal_domain::<LibraryFileOrdinal>();
    assert_reporting_authority::<EventStore>();
    assert_reporting_authority::<LibraryEventLedger>();
    assert_event_key_domain::<EventKey>();
    assert_event_key_domain::<LibraryEventKey>();

    let mut users = EventStore::default();
    let user = reserve_user_primary(&mut users, ModuleOrdinal::new(3), 11);
    let mut library = LibraryEventLedger::default();
    let library_record = reserve_library_primary(&mut library, LibraryFileOrdinal::new(7), 11);

    assert!(complete_user(&mut users, user, Vec::new()).is_ok());
    assert!(complete_library(&mut library, library_record, Vec::new()).is_ok());
    assert!(finish_user(users).is_ok());
    assert!(finish_library(library).is_ok());
}

#[test]
fn negative_ui_fixture_inventories_every_cross_domain_inversion() {
    let fixture = include_str!("wu0b_reporting_negative_ui.rs");
    for probe in [
        "user_complete_rejects_library_ticket",
        "library_complete_rejects_user_ticket",
        "user_reserve_rejects_library_ordinal",
        "library_reserve_rejects_user_ordinal",
        "user_key_stream_rejects_library_records",
        "library_key_stream_rejects_user_records",
    ] {
        assert!(fixture.contains(probe), "missing negative UI probe {probe}");
    }
}

#[test]
fn source_unit_tags_keep_user_slots_out_of_library_units() {
    let user = SourceUnit::User {
        module_ordinal: ModuleOrdinal::new(4),
        unit_slot: UnitSlot::new(9),
    };
    let library = SourceUnit::Library {
        file_ordinal: LibraryFileOrdinal::new(12),
    };

    let SourceUnit::User {
        module_ordinal,
        unit_slot,
    } = user
    else {
        assert!(matches!(user, SourceUnit::User { .. }));
        return;
    };
    assert_eq!(module_ordinal, ModuleOrdinal::new(4));
    assert_eq!(unit_slot, UnitSlot::new(9));

    let SourceUnit::Library { file_ordinal } = library else {
        assert!(matches!(library, SourceUnit::Library { .. }));
        return;
    };
    assert_eq!(file_ordinal, LibraryFileOrdinal::new(12));
    assert_eq!(
        SourceOrdinal::User(module_ordinal),
        SourceOrdinal::User(ModuleOrdinal::new(4))
    );
    assert_eq!(
        SourceOrdinal::Library(file_ordinal),
        SourceOrdinal::Library(LibraryFileOrdinal::new(12))
    );
}

#[test]
fn checker_pass_stores_one_source_unit_without_user_coordinate_fallbacks() {
    let source = include_str!("context.rs");
    let production = source.split_once("#[cfg(test)]\nmod tests");
    assert!(
        production.is_some(),
        "context.rs must contain its test module"
    );
    let Some((production, _)) = production else {
        return;
    };
    let pass_definition = production
        .split_once("pub(in crate::check::checker) struct Pass")
        .and_then(|(_, tail)| tail.split_once("\n}\n"));
    assert!(pass_definition.is_some(), "missing Pass definition");
    let Some((pass_definition, _)) = pass_definition else {
        return;
    };
    let compact = without_whitespace(pass_definition);

    assert!(
        compact.contains("current_source:SourceUnit,"),
        "Pass must retain the exact user or library source unit"
    );
    for forbidden in ["current_module_ordinal", "current_unit_slot"] {
        assert!(
            !contains_rust_identifier(production, forbidden),
            "Pass production context retains fabricated user coordinate {forbidden}"
        );
    }
}

#[test]
fn neutral_source_module_defines_its_types_without_layer_back_edges() {
    let source = include_str!("../../source.rs");
    for definition in [
        "pub(crate) struct ModuleOrdinal",
        "pub(crate) struct UnitSlot",
        "pub(crate) struct OriginalModuleOrdinal",
        "pub(crate) struct LibraryFileOrdinal",
        "pub(crate) enum SourceOrdinal",
        "pub(crate) enum SourceUnit",
        "pub(crate) enum CompilationOrigin",
    ] {
        assert!(
            source.contains(definition),
            "missing neutral definition {definition}"
        );
    }
    assert!(
        use_statements(source).is_empty(),
        "neutral source module must be definitions-only and contain no imports or reexports"
    );
    for forbidden in [
        "SourceUnitKey",
        "::",
        "mod ",
        "mod\n",
        "extern crate",
        "include!",
        "#[path",
        "macro_rules!",
    ] {
        assert!(
            !source.contains(forbidden),
            "neutral source module contains forbidden path or module escape {forbidden}"
        );
    }
}

#[test]
fn shared_reporting_record_has_no_storage_or_authority() {
    let source = include_str!("reporting_record.rs");
    assert!(source.contains("pub(crate) enum CheckerRecord"));
    for forbidden in [
        "EventStore",
        "LibraryEventLedger",
        "UserRecordTicket",
        "LibraryRecordTicket",
        "ModuleOrdinal",
        "LibraryFileOrdinal",
        "EventKey",
        "LibraryEventKey",
        "BTreeMap",
        "Binder",
        "CompilationOrigin",
    ] {
        assert!(
            !contains_rust_identifier(source, forbidden),
            "shared record module contains reporting authority token {forbidden}"
        );
    }
}

#[test]
fn injected_profile_module_only_orchestrates_library_reporting() {
    let source = include_str!("wu0b_library.rs");
    for required in [
        "LibraryFileOrdinal",
        "LibraryEventLedger",
        "LibraryReportingConsumer",
        "consume_binder_outcomes",
    ] {
        assert!(source.contains(required), "injected seam misses {required}");
    }
    for forbidden in [
        "EventStore",
        "UserRecordTicket",
        "ModuleOrdinal",
        "UnitSlot",
        "SourceOrdinal",
        "SourceUnit",
        ".reserve_event(",
        ".reserve_record(",
        ".complete(",
    ] {
        assert!(
            !source.contains(forbidden),
            "injected seam contains reporting storage bypass {forbidden}"
        );
    }
}

#[test]
fn user_reporting_implementation_is_user_only_and_renames_its_ticket() {
    let source = include_str!("events.rs");
    let production = source.split_once("#[cfg(test)]\nmod tests");
    assert!(
        production.is_some(),
        "events.rs must contain its test module"
    );
    let Some((production, _)) = production else {
        return;
    };
    assert!(use_statements(production).iter().any(|statement| {
        contains_rust_identifier(statement, "reporting_record")
            && contains_rust_identifier(statement, "CheckerRecord")
    }));

    for definition in [
        "struct UserRecordTicket",
        "struct EventKey",
        "struct EventStore",
        "ticket: UserRecordTicket",
    ] {
        assert!(
            production.contains(definition),
            "missing concrete reporting definition {definition}"
        );
    }
    for forbidden_identifier in [
        "RecordTicket",
        "LibraryRecordTicket",
        "LibraryEventKey",
        "LibraryEventLedger",
        "LibraryFileOrdinal",
        "SourceOrdinal",
        "ReportingAuthority",
    ] {
        assert!(
            !contains_rust_identifier(production, forbidden_identifier),
            "user reporting module contains forbidden identifier {forbidden_identifier}"
        );
    }
    for forbidden_syntax in [
        "type UserRecordTicket =",
        "impl From<UserRecordTicket> for LibraryRecordTicket",
        "impl From<LibraryRecordTicket> for UserRecordTicket",
        "fn reserve_event<",
        "fn complete<",
        "fn finish<",
        "split_off",
        "retag",
    ] {
        assert!(
            !production.contains(forbidden_syntax),
            "user reporting module contains forbidden unification {forbidden_syntax}"
        );
    }
}

#[test]
fn library_reporting_storage_is_native_and_cannot_wrap_user_events() {
    let source = include_str!("events_library.rs");
    let production = source.split_once("#[cfg(test)]\nmod tests");
    assert!(
        production.is_some(),
        "events_library.rs must contain its test module"
    );
    let Some((production, _)) = production else {
        return;
    };
    assert!(use_statements(production).iter().any(|statement| {
        contains_rust_identifier(statement, "reporting_record")
            && contains_rust_identifier(statement, "CheckerRecord")
    }));

    for definition in [
        "struct LibraryEventId",
        "struct LibraryRecordTicket",
        "struct LibraryReservedEvent",
        "struct LibraryEventMeta",
        "enum LibraryCompletion",
        "struct LibraryEventKey",
        "struct LibraryEventLedger",
        "enum LibraryEventLedgerError",
        "ticket: LibraryRecordTicket",
    ] {
        assert!(
            production.contains(definition),
            "library ledger misses native storage definition {definition}"
        );
    }
    let compact = without_whitespace(production);
    let native_shape = compact
        .replace("pub(crate)", "")
        .replace("pub(super)", "")
        .replace("pub(self)", "");
    for native_storage in [
        "structLibraryEventId(usize);",
        "structLibraryRecordTicket{event:LibraryEventId,record_ordinal:usize,}",
        "structLibraryReservedEvent{id:LibraryEventId,primary:LibraryRecordTicket,}",
        "structLibraryEventKey{file_ordinal:LibraryFileOrdinal,source_start:u32,event_ordinal:usize,record_ordinal:usize,}",
        "structLibraryEventMeta{file_ordinal:LibraryFileOrdinal,source_start:u32,event_ordinal:usize,next_record_ordinal:usize,}",
        "enumLibraryCompletion{Pending,Completed(Vec<CheckerRecord>),}",
        "events:Vec<LibraryEventMeta>",
        "next_event_ordinal:BTreeMap<LibraryFileOrdinal,usize>",
        "completions:BTreeMap<LibraryEventKey,LibraryCompletion>",
        "Result<Vec<(LibraryEventKey,CheckerRecord)>,LibraryEventLedgerError>",
    ] {
        assert!(
            native_shape.contains(native_storage),
            "library ledger misses native storage shape {native_storage}"
        );
    }
    assert!(native_shape.contains(
        "structLibraryEventLedger{events:Vec<LibraryEventMeta>,next_event_ordinal:BTreeMap<LibraryFileOrdinal,usize>,completions:BTreeMap<LibraryEventKey,LibraryCompletion>,}"
    ));

    for forbidden_identifier in [
        "EventStore",
        "RecordTicket",
        "UserRecordTicket",
        "ModuleOrdinal",
        "UnitSlot",
        "EventKey",
        "SourceOrdinal",
        "ReportingAuthority",
    ] {
        assert!(
            !contains_rust_identifier(source, forbidden_identifier),
            "library ledger contains user or generic identifier {forbidden_identifier}"
        );
    }
    assert!(
        use_statements(source)
            .iter()
            .all(|statement| !contains_rust_identifier(statement, "events")),
        "library ledger must not import the user events module through any path form"
    );
    for forbidden_syntax in [
        "inner:",
        "fn reserve_event<",
        "fn complete<",
        "fn finish<",
        "split_off",
        "retag",
    ] {
        assert!(
            !production.contains(forbidden_syntax),
            "library ledger contains wrapper or generic storage escape {forbidden_syntax}"
        );
    }
}

#[test]
fn library_reporting_consumer_is_the_only_binder_to_ledger_boundary() {
    let source = include_str!("library_reporting.rs");
    for required in [
        "pub(crate) struct LibraryReportingConsumer",
        "fn consume_binder_outcomes(",
        "binder: &Binder",
        "LibraryReportingReceipt",
        "LocalAmbientExportAliasFailure",
        "PlacementIssue",
        "GlobalAugmentation",
        "UmdExportContext",
        "ExportContext",
        "NamespaceMember",
        "StandaloneNamespaceValueMember",
        ".reserve_event(",
        ".complete(",
        "CheckerRecord::Diagnostic",
        "CheckerRecord::Incomplete",
    ] {
        assert!(
            source.contains(required),
            "library reporting consumer misses {required}"
        );
    }
    let compact = without_whitespace(source);
    for required_shape in [
        "ledger:&'ledgermutLibraryEventLedger",
        "fnconsume_binder_outcomes(",
        "binder:&Binder",
        "Result<Vec<LibraryReportingReceipt>,LibraryEventLedgerError>",
    ] {
        assert!(
            compact.contains(required_shape),
            "library reporting consumer misses concrete shape {required_shape}"
        );
    }
    for forbidden_identifier in [
        "EventStore",
        "UserRecordTicket",
        "ModuleOrdinal",
        "UnitSlot",
        "EventKey",
    ] {
        assert!(
            !contains_rust_identifier(source, forbidden_identifier),
            "library reporting consumer contains user authority {forbidden_identifier}"
        );
    }
    assert!(
        use_statements(source)
            .iter()
            .all(|statement| !contains_rust_identifier(statement, "events")),
        "library reporting consumer must not import user events through brace or relative paths"
    );
}

#[test]
fn library_finish_orders_by_file_source_event_and_record() {
    let mut ledger = LibraryEventLedger::default();
    let file_one = ledger.reserve_event(LibraryFileOrdinal::new(1), 0);
    let file_zero_late = ledger.reserve_event(LibraryFileOrdinal::new(0), 20);
    let file_zero_first = ledger.reserve_event(LibraryFileOrdinal::new(0), 10);
    let file_zero_second = ledger.reserve_event(LibraryFileOrdinal::new(0), 10);
    let extra = ledger.reserve_record(file_zero_first.id);
    assert!(extra.is_ok(), "library event must accept a local record");
    let Ok(extra) = extra else {
        return;
    };

    assert!(ledger
        .complete(file_one.primary, vec![incomplete("file-one", 0)])
        .is_ok());
    assert!(ledger
        .complete(file_zero_late.primary, vec![incomplete("late", 20)])
        .is_ok());
    assert!(ledger
        .complete(file_zero_second.primary, vec![incomplete("second", 10)])
        .is_ok());
    assert!(ledger
        .complete(extra, vec![incomplete("extra", 10)])
        .is_ok());
    assert!(ledger
        .complete(file_zero_first.primary, vec![incomplete("first", 10)])
        .is_ok());

    let records = ledger.finish();
    assert!(records.is_ok(), "completed library ledger must finish");
    let Ok(records) = records else {
        return;
    };
    assert_eq!(
        records
            .iter()
            .map(|(key, _)| library_key_tuple(key))
            .collect::<Vec<_>>(),
        [
            (0, 10, 1, 0),
            (0, 10, 1, 1),
            (0, 10, 2, 0),
            (0, 20, 0, 0),
            (1, 0, 0, 0),
        ]
    );
}

#[test]
fn separate_finish_preserves_both_domains_without_renumbering() {
    let mut users = EventStore::default();
    let user = users.reserve_event(ModuleOrdinal::new(37), 50);
    let mut library = LibraryEventLedger::default();
    let library_record = library.reserve_event(LibraryFileOrdinal::new(81), 2);

    assert!(users
        .complete(user.primary, vec![incomplete("user", 50)])
        .is_ok());
    assert!(library
        .complete(library_record.primary, vec![incomplete("library", 2)])
        .is_ok());

    let user_records = users.finish();
    let library_records = library.finish();
    assert!(
        user_records.is_ok(),
        "user authority must finish independently"
    );
    assert!(
        library_records.is_ok(),
        "library authority must finish independently"
    );
    let (Ok(user_records), Ok(library_records)) = (user_records, library_records) else {
        return;
    };
    let user_records: Vec<(EventKey, CheckerRecord)> = user_records;
    let library_records: Vec<(LibraryEventKey, CheckerRecord)> = library_records;

    assert_eq!(
        user_records
            .iter()
            .map(|(key, _)| user_key_tuple(key))
            .collect::<Vec<_>>(),
        [(37, 50, 0, 0)]
    );
    assert_eq!(
        library_records
            .iter()
            .map(|(key, _)| library_key_tuple(key))
            .collect::<Vec<_>>(),
        [(81, 2, 0, 0)]
    );
}

#[test]
fn existing_user_event_key_order_is_unchanged() {
    let mut users = EventStore::default();
    let module_one = users.reserve_event(ModuleOrdinal::new(1), 0);
    let module_zero_late = users.reserve_event(ModuleOrdinal::new(0), 50);
    let module_zero_first = users.reserve_event(ModuleOrdinal::new(0), 10);
    let module_zero_second = users.reserve_event(ModuleOrdinal::new(0), 10);
    let extra = users.reserve_record(module_zero_first.id);
    assert!(
        extra.is_ok(),
        "user event must retain local record reservation"
    );
    let Ok(extra) = extra else {
        return;
    };

    assert!(users
        .complete(module_one.primary, vec![incomplete("module-one", 0)])
        .is_ok());
    assert!(users
        .complete(module_zero_late.primary, vec![incomplete("late", 50)])
        .is_ok());
    assert!(users
        .complete(module_zero_second.primary, vec![incomplete("second", 10)])
        .is_ok());
    assert!(users.complete(extra, vec![incomplete("extra", 10)]).is_ok());
    assert!(users
        .complete(module_zero_first.primary, vec![incomplete("first", 10)])
        .is_ok());

    let records = users.finish();
    assert!(records.is_ok(), "completed user store must finish");
    let Ok(records) = records else {
        return;
    };
    assert_eq!(
        records
            .iter()
            .map(|(key, _)| user_key_tuple(key))
            .collect::<Vec<_>>(),
        [
            (0, 10, 1, 0),
            (0, 10, 1, 1),
            (0, 10, 2, 0),
            (0, 50, 0, 0),
            (1, 0, 0, 0),
        ]
    );
}

#[test]
fn user_multi_record_completion_replays_each_group_in_exact_ticket_order() {
    let mut users = EventStore::default();
    let event = users.reserve_event(ModuleOrdinal::new(2), 40);
    let second = users.reserve_record(event.id);
    assert!(second.is_ok(), "user record reservation must succeed");
    let Ok(second) = second else {
        return;
    };

    assert!(complete_user(
        &mut users,
        second,
        vec![incomplete("second-a", 40), incomplete("second-b", 40)],
    )
    .is_ok());
    assert!(complete_user(
        &mut users,
        event.primary,
        vec![incomplete("first-a", 40), incomplete("first-b", 40)],
    )
    .is_ok());

    let records = finish_user(users);
    assert!(records.is_ok(), "completed user store must finish");
    let Ok(records) = records else {
        return;
    };
    assert_eq!(
        records
            .iter()
            .map(|(key, record)| (user_key_tuple(key), incomplete_id(record)))
            .collect::<Vec<_>>(),
        [
            ((2, 40, 0, 0), Some("first-a")),
            ((2, 40, 0, 0), Some("first-b")),
            ((2, 40, 0, 1), Some("second-a")),
            ((2, 40, 0, 1), Some("second-b")),
        ]
    );
}

#[test]
fn identical_spans_in_two_library_files_keep_distinct_owners() {
    let mut ledger = LibraryEventLedger::default();
    let first = ledger.reserve_event(LibraryFileOrdinal::new(10), 25);
    let second = ledger.reserve_event(LibraryFileOrdinal::new(11), 25);

    assert!(ledger
        .complete(first.primary, vec![incomplete("same-span", 25)])
        .is_ok());
    assert!(ledger
        .complete(second.primary, vec![incomplete("same-span", 25)])
        .is_ok());

    let records = ledger.finish();
    assert!(records.is_ok(), "both exact owners must finish");
    let Ok(records) = records else {
        return;
    };
    assert_eq!(
        records
            .iter()
            .map(|(key, _)| library_key_tuple(key))
            .collect::<Vec<_>>(),
        [(10, 25, 0, 0), (11, 25, 0, 0)]
    );
}

#[test]
fn script_interface_reopenings_share_one_real_type_identity_and_surface() -> Result<(), String> {
    let first_ordinal = LibraryFileOrdinal::new(20);
    let second_ordinal = LibraryFileOrdinal::new(21);
    let run = run_injected_profile(&[
        InjectedLibrarySource {
            file_ordinal: first_ordinal,
            name: "shared-interface-first.d.ts",
            source: "interface SharedLibraryShape { first: number; }",
        },
        InjectedLibrarySource {
            file_ordinal: second_ordinal,
            name: "shared-interface-second.d.ts",
            source: "interface SharedLibraryShape { second: string; }",
        },
    ])
    .map_err(|error| format!("shared interface witness failed: {error:?}"))?;

    let shared = run
        .global_type_probe("SharedLibraryShape")
        .ok_or_else(|| "missing shared global type probe".to_string())?;
    let mut declaration_identities: Vec<(LibraryFileOrdinal, TypeGroupId)> =
        shared.declaration_identities.clone();
    declaration_identities.sort_by_key(|(file_ordinal, _)| *file_ordinal);
    assert_eq!(
        declaration_identities,
        [
            (first_ordinal, shared.identity),
            (second_ordinal, shared.identity),
        ]
    );
    assert_eq!(
        shared
            .member_names
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["first", "second"])
    );
    assert_eq!(shared.declaration_count, 2);
    assert!(
        run.library_records.is_empty(),
        "clean interface reopening emitted records: {:?}",
        run.library_records
    );
    Ok(())
}

#[test]
fn cross_file_function_namespace_merges_publish_in_both_orders() -> Result<(), String> {
    let sources = [
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(30),
            name: "function-first.d.ts",
            source: "declare function FunctionFirst(value: number): string;",
        },
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(31),
            name: "function-first-namespace.d.ts",
            source: "declare namespace FunctionFirst { export const tag: number; }",
        },
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(32),
            name: "namespace-first.d.ts",
            source: "declare namespace NamespaceFirst { export const tag: string; }",
        },
        InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(33),
            name: "namespace-first-function.d.ts",
            source: "declare function NamespaceFirst(value: string): number;",
        },
    ];
    let reversed = sources
        .iter()
        .rev()
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.file_ordinal,
            name: source.name,
            source: source.source,
        })
        .collect::<Vec<_>>();
    let forward = run_injected_profile(&sources)
        .map_err(|error| format!("forward function/namespace witness failed: {error:?}"))?;
    let reverse_input = run_injected_profile(&reversed)
        .map_err(|error| format!("reversed function/namespace witness failed: {error:?}"))?;

    for run in [&forward, &reverse_input] {
        assert!(
            run.library_records.is_empty(),
            "clean function/namespace merges emitted records: {:?}",
            run.library_records
        );
        for (name, first_file, second_file, tag) in [
            (
                "FunctionFirst",
                LibraryFileOrdinal::new(30),
                LibraryFileOrdinal::new(31),
                "tag",
            ),
            (
                "NamespaceFirst",
                LibraryFileOrdinal::new(32),
                LibraryFileOrdinal::new(33),
                "tag",
            ),
        ] {
            let merged = run
                .global_value_probe(name)
                .ok_or_else(|| format!("missing merged global value {name}"))?;
            let mut participant_identities: Vec<(LibraryFileOrdinal, ValueStorageId)> =
                merged.participant_identities.clone();
            participant_identities.sort_by_key(|(file_ordinal, _)| *file_ordinal);
            // Namespace participants report the attached function owner's real storage.
            assert_eq!(
                participant_identities,
                [
                    (first_file, merged.identity),
                    (second_file, merged.identity),
                ]
            );
            assert_eq!(merged.declaration_count, 2);
            assert_eq!(merged.call_signature_count, 1);
            assert_eq!(
                merged
                    .member_names
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([tag])
            );
        }
    }
    Ok(())
}

#[test]
fn attached_callable_members_with_identical_offsets_keep_exact_library_owners_in_both_orders(
) -> Result<(), String> {
    let first_ordinal = LibraryFileOrdinal::new(34);
    let second_ordinal = LibraryFileOrdinal::new(35);
    let sources = [
        InjectedLibrarySource {
            file_ordinal: first_ordinal,
            name: "offset-callable-first.d.ts",
            source: "declare namespace OffsetCallable { export function alpha(value: number): string; }\ndeclare function OffsetCallable(): void;",
        },
        InjectedLibrarySource {
            file_ordinal: second_ordinal,
            name: "offset-callable-second.d.ts",
            source: "declare namespace OffsetCallable { export function bravo(value: string): number; }",
        },
    ];
    let reversed = sources
        .iter()
        .rev()
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.file_ordinal,
            name: source.name,
            source: source.source,
        })
        .collect::<Vec<_>>();
    let forward = run_injected_profile(&sources)
        .map_err(|error| format!("forward identical-offset witness failed: {error:?}"))?;
    let reverse_input = run_injected_profile(&reversed)
        .map_err(|error| format!("reversed identical-offset witness failed: {error:?}"))?;

    for run in [&forward, &reverse_input] {
        assert!(
            run.library_records.is_empty(),
            "clean identical-offset merge emitted records: {:?}",
            run.library_records
        );
        let merged = run
            .global_value_probe("OffsetCallable")
            .ok_or_else(|| "missing identical-offset merged global value".to_string())?;
        let mut participant_identities: Vec<(LibraryFileOrdinal, ValueStorageId)> =
            merged.participant_identities.clone();
        participant_identities.sort_by_key(|(file_ordinal, _)| *file_ordinal);
        assert_eq!(
            participant_identities,
            [
                (first_ordinal, merged.identity),
                (first_ordinal, merged.identity),
                (second_ordinal, merged.identity),
            ]
        );
        assert_eq!(merged.declaration_count, 3);
        assert_eq!(merged.call_signature_count, 1);
        assert_eq!(
            merged
                .member_names
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["alpha", "bravo"])
        );

        // These rows must come from the published member storages and their binder origins.
        let mut callable_members = merged.callable_members.clone();
        callable_members.sort_by(|left, right| left.name.cmp(&right.name));
        assert_eq!(callable_members.len(), 2);
        assert_ne!(callable_members[0].identity, callable_members[1].identity);
        assert_eq!(
            callable_members
                .iter()
                .map(|member| (
                    member.name.as_str(),
                    member.source,
                    member.source_start,
                    member.call_signature_count,
                ))
                .collect::<Vec<_>>(),
            [
                (
                    "alpha",
                    SourceUnit::Library {
                        file_ordinal: first_ordinal,
                    },
                    42,
                    1,
                ),
                (
                    "bravo",
                    SourceUnit::Library {
                        file_ordinal: second_ordinal,
                    },
                    42,
                    1,
                ),
            ]
        );
    }
    Ok(())
}

#[test]
fn external_module_privates_stay_local_while_declare_global_reopens_shared_type(
) -> Result<(), String> {
    let script_ordinal = LibraryFileOrdinal::new(40);
    let module_ordinal = LibraryFileOrdinal::new(41);
    let run = run_injected_profile(&[
        InjectedLibrarySource {
            file_ordinal: script_ordinal,
            name: "shared-global-script.d.ts",
            source: "interface SharedAugmentedShape { script: number; }",
        },
        InjectedLibrarySource {
            file_ordinal: module_ordinal,
            name: "private-module.d.ts",
            source: "export {}; interface ModulePrivateShape { privateMember: boolean; } declare global { interface SharedAugmentedShape { augmentation: string; } }",
        },
    ])
    .map_err(|error| format!("external-module isolation witness failed: {error:?}"))?;

    assert!(run.global_type_probe("ModulePrivateShape").is_none());
    let private = run
        .module_type_probe(module_ordinal, "ModulePrivateShape")
        .ok_or_else(|| "external-module private type was not retained locally".to_string())?;
    let private_identities: Vec<(LibraryFileOrdinal, TypeGroupId)> =
        private.declaration_identities.clone();
    assert_eq!(private_identities, [(module_ordinal, private.identity)]);
    assert_eq!(private.declaration_count, 1);
    assert_eq!(
        private
            .member_names
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["privateMember"])
    );

    let shared = run
        .global_type_probe("SharedAugmentedShape")
        .ok_or_else(|| "declare global did not reopen the compilation global".to_string())?;
    let mut shared_identities: Vec<(LibraryFileOrdinal, TypeGroupId)> =
        shared.declaration_identities.clone();
    shared_identities.sort_by_key(|(file_ordinal, _)| *file_ordinal);
    assert_eq!(
        shared_identities,
        [
            (script_ordinal, shared.identity),
            (module_ordinal, shared.identity),
        ]
    );
    assert_eq!(shared.declaration_count, 2);
    assert_eq!(
        shared
            .member_names
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["augmentation", "script"])
    );
    assert!(
        run.library_records.is_empty(),
        "clean module/global augmentation emitted records: {:?}",
        run.library_records
    );
    Ok(())
}

#[test]
fn injected_pass_and_lexical_owners_keep_library_source_units_end_to_end() -> Result<(), String> {
    let file_ordinal = LibraryFileOrdinal::new(50);
    let run = run_injected_profile(&[InjectedLibrarySource {
        file_ordinal,
        name: "source-aware-pass.ts",
        source: "const broken: number = 'wrong';",
    }])
    .map_err(|error| format!("source-aware Pass witness failed: {error:?}"))?;
    let expected = SourceUnit::Library { file_ordinal };

    assert_eq!(run.pass_source_units, [expected]);
    assert!(!run.lexical_source_units.is_empty());
    assert!(run
        .lexical_source_units
        .iter()
        .all(|source| *source == expected));
    assert_eq!(run.library_records.len(), 1);
    let (key, record) = run
        .library_records
        .first()
        .ok_or_else(|| "source-aware Pass emitted no record".to_string())?;
    assert_eq!(key.file_ordinal, file_ordinal);
    let CheckerRecord::Diagnostic(diagnostic) = record else {
        return Err("source-aware Pass emitted an incomplete instead of TK2322".to_string());
    };
    assert_eq!(diagnostic.code, DiagnosticCode::TK2322);
    assert_eq!(key.source_start, diagnostic.span.start);
    Ok(())
}

#[test]
fn all_binder_outcome_families_flow_through_the_real_library_consumer() -> Result<(), String> {
    enum ExpectedEmission {
        Diagnostic(DiagnosticCode),
        Incomplete {
            id: &'static str,
            context: &'static str,
        },
        None,
    }

    struct Case {
        file_ordinal: LibraryFileOrdinal,
        name: &'static str,
        source: &'static str,
        family: LibraryReportingFamily,
        expected_emission: ExpectedEmission,
    }

    let cases = [
        Case {
            file_ordinal: LibraryFileOrdinal::new(10),
            name: "local-ambient-export.d.ts",
            source: "declare namespace AliasOutput { interface Local {} export { A as B }; export { Local as A }; }",
            family: LibraryReportingFamily::LocalAmbientExportAliasFailure,
            expected_emission: ExpectedEmission::Diagnostic(DiagnosticCode::TK2661),
        },
        Case {
            file_ordinal: LibraryFileOrdinal::new(11),
            name: "placement.ts",
            source: "namespace Late { export const value = 1; } function Late(): void {}",
            family: LibraryReportingFamily::PlacementIssue,
            expected_emission: ExpectedEmission::Diagnostic(DiagnosticCode::TK2434),
        },
        Case {
            file_ordinal: LibraryFileOrdinal::new(12),
            name: "global-augmentation.d.ts",
            source: "declare global { interface InvalidScriptGlobal {} }",
            family: LibraryReportingFamily::GlobalAugmentation,
            expected_emission: ExpectedEmission::Diagnostic(DiagnosticCode::TK2669),
        },
        Case {
            file_ordinal: LibraryFileOrdinal::new(13),
            name: "umd-context.d.ts",
            source: "export as namespace ScriptUmd;",
            family: LibraryReportingFamily::UmdExportContext,
            expected_emission: ExpectedEmission::Diagnostic(DiagnosticCode::TK1314),
        },
        Case {
            file_ordinal: LibraryFileOrdinal::new(14),
            name: "export-context.d.ts",
            source: "declare namespace Exported { export default function f(): void; }",
            family: LibraryReportingFamily::ExportContext,
            expected_emission: ExpectedEmission::Incomplete {
                id: "library/export-context/future-tk1319",
                context: "library export-context TK1319 reporting is deferred beyond WU0B",
            },
        },
        Case {
            file_ordinal: LibraryFileOrdinal::new(15),
            name: "namespace-member.d.ts",
            source: "declare namespace MemberRoot { const value: number; }",
            family: LibraryReportingFamily::NamespaceMember,
            expected_emission: ExpectedEmission::None,
        },
        Case {
            file_ordinal: LibraryFileOrdinal::new(16),
            name: "standalone-namespace.d.ts",
            source: "declare namespace StandaloneRoot { const value: number; }",
            family: LibraryReportingFamily::StandaloneNamespaceValueMember,
            expected_emission: ExpectedEmission::None,
        },
    ];

    for case in cases {
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal: case.file_ordinal,
            name: case.name,
            source: case.source,
        }])
        .map_err(|error| format!("injected family witness failed: {error:?}"))?;
        let receipt = run
            .reporting_receipts
            .iter()
            .find(|receipt| receipt.family == case.family)
            .ok_or_else(|| format!("missing typed receipt for {:?}", case.family))?;

        assert_eq!(receipt.file_ordinal, case.file_ordinal);
        assert_eq!(receipt.observed_outcomes, 1);
        assert!(run
            .reporting_receipts
            .iter()
            .all(|receipt| receipt.file_ordinal == case.file_ordinal));
        assert!(run
            .library_records
            .iter()
            .all(|(key, record)| key.file_ordinal == case.file_ordinal
                && key.source_start == record_span(record).start));

        match case.expected_emission {
            ExpectedEmission::Diagnostic(expected_code) => {
                assert_eq!(receipt.emitted_records, 1);
                assert_eq!(run.library_records.len(), 1);
                assert!(run.library_records.iter().any(|(_, record)| matches!(
                    record,
                    CheckerRecord::Diagnostic(diagnostic) if diagnostic.code == expected_code
                )));
            }
            ExpectedEmission::Incomplete { id, context } => {
                assert_eq!(receipt.emitted_records, 1);
                assert_eq!(run.library_records.len(), 1);
                assert!(run.library_records.iter().any(|(_, record)| matches!(
                    record,
                    CheckerRecord::Incomplete(incomplete)
                        if incomplete.id == id && incomplete.context == context
                )));
            }
            ExpectedEmission::None => {
                assert_eq!(receipt.emitted_records, 0);
                assert!(run.library_records.is_empty());
            }
        }
    }
    Ok(())
}

#[test]
fn committed_registry_rows_drive_the_injected_profile_and_exact_owner_set() -> Result<(), String> {
    let owned = validated_profile_sources()?;
    let injected = owned
        .iter()
        .map(|row| InjectedLibrarySource {
            file_ordinal: row.file_ordinal,
            name: &row.name,
            source: &row.source,
        })
        .collect::<Vec<_>>();
    let run = run_injected_profile(&injected)
        .map_err(|error| format!("injected profile failed: {error:?}"))?;

    assert_eq!(run.phase_counts.parse_units, 82);
    assert_eq!(run.phase_counts.bind_units, 82);
    assert!(run.phase_counts.reserved_records > 0);
    assert_eq!(
        run.phase_counts.filled_records,
        run.phase_counts.reserved_records
    );
    assert!(run.phase_counts.publication_validations > 0);
    assert_eq!(run.phase_counts.statement_check_units, 82);

    let expected = owned
        .iter()
        .map(|row| row.file_ordinal)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        expected
            .iter()
            .map(|ordinal| ordinal.index())
            .collect::<Vec<_>>(),
        (0..82).collect::<Vec<_>>()
    );
    assert_eq!(run.reserved_file_ordinals.len(), 82);
    let reserved = run
        .reserved_file_ordinals
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(reserved, expected);

    let records: Vec<(LibraryEventKey, CheckerRecord)> = run.library_records;
    assert!(records
        .iter()
        .all(|(key, _)| expected.contains(&key.file_ordinal)));
    assert!(run
        .reporting_receipts
        .iter()
        .all(|receipt| expected.contains(&receipt.file_ordinal)));
    Ok(())
}

#[test]
fn public_driver_signatures_behavior_and_input_order_stay_unchanged() {
    let check: fn(&str) -> crate::driver::CheckOutput = crate::driver::check_source;
    let check_files: fn(Vec<FileInput>) -> Vec<FileReport> = crate::driver::check_files;
    let check_project: fn(Vec<FileInput>) -> Vec<FileReport> = crate::driver::check_project;

    let output = check("const value: number = 'wrong';");
    assert_eq!(output.parse_errors.len(), 0);
    assert_eq!(output.incomplete.len(), 0);
    assert_eq!(output.diagnostics.len(), 1);
    let Some(diagnostic) = output.diagnostics.first() else {
        return;
    };
    assert_eq!(diagnostic.code, DiagnosticCode::TK2322);

    let reports = check_files(vec![
        FileInput {
            name: "second.ts".to_string(),
            source: "const second: number = 'wrong';".to_string(),
        },
        FileInput {
            name: "first.ts".to_string(),
            source: "const first: number = 1;".to_string(),
        },
    ]);
    assert_eq!(
        reports
            .iter()
            .map(|report| report.name.as_str())
            .collect::<Vec<_>>(),
        ["second.ts", "first.ts"]
    );
    let (Some(second), Some(first)) = (reports.first(), reports.get(1)) else {
        return;
    };
    assert_eq!(second.output.diagnostics.len(), 1);
    assert_eq!(first.output.diagnostics.len(), 0);

    let project = check_project(vec![
        FileInput {
            name: "b.ts".to_string(),
            source: "import { value } from './a'; const copy: number = value;".to_string(),
        },
        FileInput {
            name: "a.ts".to_string(),
            source: "export const value: number = 1;".to_string(),
        },
    ]);
    assert_eq!(
        project
            .iter()
            .map(|report| report.name.as_str())
            .collect::<Vec<_>>(),
        ["b.ts", "a.ts"]
    );
    assert!(project
        .iter()
        .all(|report| report.output.diagnostics.is_empty()));
}

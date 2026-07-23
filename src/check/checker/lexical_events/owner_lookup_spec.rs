use super::*;
use crate::check::checker::events::EventStore;
use crate::source::{LibraryFileOrdinal, ModuleOrdinal, UnitSlot};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct TestTicket(u32);

fn site_tickets(base: u32) -> SiteTickets<TestTicket> {
    SiteTickets {
        immediate: TestTicket(base),
        deferred: TestTicket(base + 1),
        incomplete: TestTicket(base + 2),
    }
}

fn user_site(source_start: u32) -> SourceSite {
    SourceSite::user(ModuleOrdinal::new(0), UnitSlot::new(0), source_start)
}

fn library_site(source_start: u32) -> SourceSite {
    SourceSite {
        unit: SourceUnit::Library {
            file_ordinal: LibraryFileOrdinal::new(0),
        },
        source_start,
    }
}

fn index_first(
    index: &mut FxHashMap<(SourceOrdinal, u32), usize>,
    source: SourceSite,
    row_index: usize,
) {
    index
        .entry((source.ordinal(), source.source_start))
        .or_insert(row_index);
}

fn assert_site_phases(
    reservations: &LexicalReservations<TestTicket>,
    source: SourceOrdinal,
    source_start: u32,
    base: u32,
) {
    for (phase, expected) in [
        (LexicalOwnerPhase::Immediate, base),
        (LexicalOwnerPhase::Deferred, base + 1),
        (LexicalOwnerPhase::Incomplete, base + 2),
        (LexicalOwnerPhase::Body, base),
    ] {
        assert_eq!(
            reservations
                .owner_at(source, source_start, phase)
                .map(|owner| owner.ticket),
            Some(TestTicket(expected)),
        );
    }
}

#[test]
fn owner_lookup_preserves_priority_first_match_phase_and_source_domain() {
    let mut reservations = LexicalReservations::<TestTicket>::default();

    let callable_start = 10;
    for base in [100, 110] {
        let id = CallableSiteId(reservations.callables.len());
        reservations.callables.push(CallableReservation {
            id,
            owner_member: None,
            source: user_site(callable_start),
            tickets: CallableTickets {
                signature: TestTicket(base),
                deferred: TestTicket(base + 1),
                incomplete: TestTicket(base + 2),
                body: TestTicket(base + 3),
            },
            type_parameter_count: 0,
            binding: None,
        });
        reservations
            .callables_by_source
            .entry((SourceOrdinal::User(ModuleOrdinal::new(0)), callable_start))
            .or_default()
            .push(id);
    }
    let callable_shadow = DeclaratorReservation {
        source: user_site(callable_start),
        tickets: site_tickets(200),
    };
    let callable_shadow_index = reservations.declarators.len();
    reservations.declarators.push(callable_shadow.clone());
    index_first(
        &mut reservations.declarators_by_source,
        callable_shadow.source,
        callable_shadow_index,
    );

    let declarator_start = 20;
    for base in [200, 210] {
        let row = DeclaratorReservation {
            source: user_site(declarator_start),
            tickets: site_tickets(base),
        };
        let row_index = reservations.declarators.len();
        reservations.declarators.push(row.clone());
        index_first(
            &mut reservations.declarators_by_source,
            row.source,
            row_index,
        );
    }
    let declarator_shadow = NestedStatementReservation {
        source: user_site(declarator_start),
        tickets: site_tickets(300),
        callable: None,
    };
    let declarator_shadow_index = reservations.nested_statements.len();
    reservations
        .nested_statements
        .push(declarator_shadow.clone());
    index_first(
        &mut reservations.nested_statements_by_source,
        declarator_shadow.source,
        declarator_shadow_index,
    );

    let nested_start = 30;
    for base in [300, 310] {
        let row = NestedStatementReservation {
            source: user_site(nested_start),
            tickets: site_tickets(base),
            callable: None,
        };
        let row_index = reservations.nested_statements.len();
        reservations.nested_statements.push(row.clone());
        index_first(
            &mut reservations.nested_statements_by_source,
            row.source,
            row_index,
        );
    }
    let nested_shadow = MemberReservation {
        id: MemberSiteId(0),
        class: ClassSiteId(0),
        source: user_site(nested_start),
        tickets: site_tickets(400),
        callable: None,
    };
    let nested_shadow_index = reservations.members.len();
    reservations.members.push(nested_shadow.clone());
    index_first(
        &mut reservations.members_by_source,
        nested_shadow.source,
        nested_shadow_index,
    );

    let member_start = 40;
    for base in [400, 410] {
        let row = MemberReservation {
            id: MemberSiteId(reservations.members.len()),
            class: ClassSiteId(0),
            source: user_site(member_start),
            tickets: site_tickets(base),
            callable: None,
        };
        let row_index = reservations.members.len();
        reservations.members.push(row.clone());
        index_first(&mut reservations.members_by_source, row.source, row_index);
    }
    let member_shadow = TopLevelReservation {
        source: user_site(member_start),
        tickets: site_tickets(500),
        class: None,
        callable: None,
    };
    let member_shadow_index = reservations.top_level.len();
    reservations.top_level.push(member_shadow.clone());
    index_first(
        &mut reservations.top_level_by_source,
        member_shadow.source,
        member_shadow_index,
    );

    let top_level_start = 50;
    for base in [500, 510] {
        let row = TopLevelReservation {
            source: user_site(top_level_start),
            tickets: site_tickets(base),
            class: None,
            callable: None,
        };
        let row_index = reservations.top_level.len();
        reservations.top_level.push(row.clone());
        index_first(&mut reservations.top_level_by_source, row.source, row_index);
    }

    let shared_start = 60;
    for (source, base) in [
        (user_site(shared_start), 600),
        (library_site(shared_start), 700),
    ] {
        let row = DeclaratorReservation {
            source,
            tickets: site_tickets(base),
        };
        let row_index = reservations.declarators.len();
        reservations.declarators.push(row.clone());
        index_first(
            &mut reservations.declarators_by_source,
            row.source,
            row_index,
        );
    }

    let probe = LexicalOwnerLookupScope::start();
    let user = SourceOrdinal::User(ModuleOrdinal::new(0));
    for (phase, expected) in [
        (LexicalOwnerPhase::Immediate, 100),
        (LexicalOwnerPhase::Deferred, 101),
        (LexicalOwnerPhase::Incomplete, 102),
        (LexicalOwnerPhase::Body, 103),
    ] {
        assert_eq!(
            reservations
                .owner_at(user, callable_start, phase)
                .map(|owner| owner.ticket),
            Some(TestTicket(expected)),
            "callable must win and retain its distinct body ticket",
        );
    }
    assert_eq!(
        probe.finish(),
        0,
        "callable hits must not enter the non-callable lookup chain",
    );

    for (source_start, base, expected_probes) in [
        (declarator_start, 200, 4),
        (nested_start, 300, 8),
        (member_start, 400, 12),
        (top_level_start, 500, 16),
    ] {
        let probe = LexicalOwnerLookupScope::start();
        assert_site_phases(&reservations, user, source_start, base);
        assert_eq!(
            probe.finish(),
            expected_probes,
            "each phase must probe only the preceding priority categories",
        );
    }
    assert_site_phases(&reservations, user, shared_start, 600);
    assert_site_phases(
        &reservations,
        SourceOrdinal::Library(LibraryFileOrdinal::new(0)),
        shared_start,
        700,
    );
}

fn parsed_reservations(source: &str) -> LexicalReservations {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(!parsed.panicked);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let mut store = EventStore::default();
    let mut reservations = LexicalReservations::default();
    reservations
        .reserve_program(
            ModuleOrdinal::new(0),
            UnitSlot::new(0),
            &parsed.program,
            &mut store,
        )
        .unwrap();
    reservations
}

fn assert_parsed_site_is_indexed(
    reservations: &LexicalReservations,
    source_start: u32,
    expected: SiteTickets,
) {
    let source = SourceOrdinal::User(ModuleOrdinal::new(0));
    for (phase, ticket) in [
        (LexicalOwnerPhase::Immediate, expected.immediate),
        (LexicalOwnerPhase::Deferred, expected.deferred),
        (LexicalOwnerPhase::Incomplete, expected.incomplete),
        (LexicalOwnerPhase::Body, expected.immediate),
    ] {
        assert_eq!(
            reservations.owner_at(source, source_start, phase),
            Some(LexicalOwner { ticket }),
        );
    }
}

#[test]
fn parsed_reservation_paths_populate_every_non_callable_owner_index() {
    let source = "\
let declarator: number = 1;
{
    void 0;
}
class Example {
    member: number;
}
debugger;
";
    let reservations = parsed_reservations(source);

    let declarator = reservations
        .declarators
        .iter()
        .find(|row| {
            row.source.source_start == u32::try_from(source.find("declarator").unwrap()).unwrap()
        })
        .unwrap();
    assert_parsed_site_is_indexed(
        &reservations,
        declarator.source.source_start,
        declarator.tickets,
    );

    let nested_start = u32::try_from(source.find("void 0").unwrap()).unwrap();
    let nested = reservations
        .nested_statements
        .iter()
        .find(|row| row.source.source_start == nested_start)
        .unwrap();
    assert_parsed_site_is_indexed(&reservations, nested_start, nested.tickets);

    let member_start = u32::try_from(source.find("member:").unwrap()).unwrap();
    let member = reservations
        .members
        .iter()
        .find(|row| row.source.source_start == member_start)
        .unwrap();
    assert_parsed_site_is_indexed(&reservations, member_start, member.tickets);

    let top_level_start = u32::try_from(source.find("debugger").unwrap()).unwrap();
    let top_level = reservations
        .top_level
        .iter()
        .find(|row| row.source.source_start == top_level_start)
        .unwrap();
    assert_parsed_site_is_indexed(&reservations, top_level_start, top_level.tickets);
}

fn declarator_reservations(count: usize) -> (LexicalReservations, Vec<u32>) {
    let source = (0..count)
        .map(|index| format!("let value_{index}: number = {index};"))
        .collect::<Vec<_>>()
        .join("\n");
    let reservations = parsed_reservations(&source);
    let starts = reservations
        .declarators
        .iter()
        .map(|reservation| reservation.source.source_start)
        .collect();
    (reservations, starts)
}

fn measured_owner_lookups(
    reservations: &LexicalReservations,
    starts: &[u32],
    lookup_count: usize,
) -> u64 {
    let probe = LexicalOwnerLookupScope::start();
    let source = SourceOrdinal::User(ModuleOrdinal::new(0));
    for index in 0..lookup_count {
        let start = starts[index % starts.len()];
        assert!(reservations
            .owner_at(source, start, LexicalOwnerPhase::Immediate)
            .is_some());
    }
    probe.finish()
}

#[test]
fn owner_lookup_work_is_linear_and_table_position_independent() {
    let (reservations, starts) = declarator_reservations(256);
    for lookup_count in [1_000, 10_000, 100_000] {
        assert_eq!(
            measured_owner_lookups(&reservations, &starts, lookup_count),
            u64::try_from(lookup_count).unwrap(),
            "a declarator hit performs exactly one non-callable index probe",
        );
    }

    let (small, small_starts) = declarator_reservations(16);
    let (large, large_starts) = declarator_reservations(1_024);
    assert_eq!(measured_owner_lookups(&small, &small_starts, 1_024), 1_024,);
    assert_eq!(measured_owner_lookups(&large, &large_starts, 1_024), 1_024,);
}

fn indexed_declarations(count: usize) -> LexicalReservations<TestTicket> {
    let mut reservations = LexicalReservations::<TestTicket>::default();
    for index in 0..count {
        let source_start = u32::try_from(index).unwrap() * 8;
        let source = if index % 2 == 0 {
            user_site(source_start)
        } else {
            library_site(source_start)
        };
        let declaration_span = Span::new(source_start, source_start + 7);
        let binding_span = Span::new(source_start + 1, source_start + 6);
        let row_index = reservations.declarations.len();
        reservations.declarations.push(DeclarationReservation {
            source,
            kind: DeclarationKind::Interface,
            declaration_span,
            binding_span,
            owner: TestTicket(9),
        });
        reservations.declarations_by_binding.insert(
            (source.ordinal(), binding_span.start, binding_span.end),
            row_index,
        );
        reservations
            .attach_declaration_owner(
                DeclId(u32::try_from(index).unwrap()),
                source.ordinal(),
                DeclarationKind::Interface,
                declaration_span,
                binding_span,
            )
            .unwrap();
    }
    reservations
}

fn measured_declaration_lookups(
    reservations: &LexicalReservations<TestTicket>,
    lookup_count: usize,
) -> u64 {
    let probe = DeclarationReservationLookupScope::start();
    for index in 0..lookup_count {
        let declaration_index = index % reservations.declarations.len();
        let declaration = DeclId(u32::try_from(declaration_index).unwrap());
        let expected = &reservations.declarations[declaration_index];
        let observed = reservations
            .declaration_reservation(declaration)
            .expect("attached declaration remains indexed");
        assert_eq!(observed.source, expected.source);
        assert_eq!(observed.owner, expected.owner);
    }
    probe.finish()
}

#[test]
fn declaration_reservation_lookup_is_direct_exact_and_table_independent() {
    let small = indexed_declarations(16);
    let large = indexed_declarations(1_024);

    assert_eq!(measured_declaration_lookups(&small, 1_024), 1_024);
    assert_eq!(measured_declaration_lookups(&large, 1_024), 1_024);

    let last = DeclId(1_023);
    assert_eq!(
        large.declaration_reservation(last).unwrap().source,
        library_site(1_023 * 8),
        "duplicate owner tickets must not alias distinct declaration rows",
    );
}

#[test]
fn owner_and_declaration_lookup_hot_paths_have_no_scan_fallback() {
    let source = include_str!("../lexical_events.rs");
    let owner_start = source.find("    pub(crate) fn owner_at(").unwrap();
    let owner_end = source[owner_start..]
        .find("    pub(crate) fn attach_class_binding(")
        .map(|offset| owner_start + offset)
        .unwrap();
    let owner_at = &source[owner_start..owner_end];
    for index in [
        "declarators_by_source",
        "nested_statements_by_source",
        "members_by_source",
        "top_level_by_source",
    ] {
        assert!(
            owner_at.contains(index),
            "owner_at must read the concrete {index} index",
        );
    }
    assert!(!owner_at.contains(".iter()"));
    assert!(!owner_at.contains(".find("));

    let declaration_start = source
        .find("    pub(crate) fn declaration_reservation(")
        .unwrap();
    let declaration_end = source[declaration_start..]
        .find("    pub(crate) fn interface_occurrence_owner(")
        .map(|offset| declaration_start + offset)
        .unwrap();
    let declaration_lookup = &source[declaration_start..declaration_end];
    assert!(declaration_lookup.contains("declaration_reservations_by_decl"));
    assert!(declaration_lookup.contains(".get("));
    assert!(!declaration_lookup.contains(".iter()"));
    assert!(!declaration_lookup.contains(".find("));
}

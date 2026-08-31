//! The completion store must be sized by the records a program produces, not by the statement
//! sites its lexical walk reserves.
//!
//! ADR-0008 requires the lexical walk to *name* one owner per record position before any
//! semantic phase runs, and to replay by `(module ordinal, source start, event ordinal, record
//! ordinal)`. Naming is not storing: those ordinals are fixed by the walk's position, so a
//! position that never receives a record needs no slot to hold its place in that order.

use super::*;
use crate::check::checker::events::{
    EventCompletionWorkForTest, EventCompletionWorkScopeForTest, EventStore,
};
use crate::check::checker::reporting_record::CheckerRecord;
use crate::diagnostics::IncompleteSurface;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// One group of the shape `tooling/bench/typobench.py` deals into every `modules-*` file, minus
/// the cross-file imports: a type alias, an interface, an exported const, an exported function
/// and a local const. Reserving one group therefore produces the mix of events and record
/// positions per statement that the bench corpus measures on real files.
fn module_group(index: usize) -> String {
    format!(
        "export type Shape_{index} = {{ id: number; label: string }};\n\
         export interface Widget_{index} {{ shape: Shape_{index}; weight: number }}\n\
         export const seed_{index}: number = {index};\n\
         export function blend_{index}(w: Widget_{index}): number {{ return w.shape.id + w.weight; }}\n\
         const probe_{index}: number = seed_{index};\n"
    )
}

fn module_source(groups: usize) -> String {
    (0..groups).map(module_group).collect()
}

fn witness_record(index: usize) -> CheckerRecord {
    let start = u32::try_from(index).expect("witness index fits u32");
    CheckerRecord::Incomplete(IncompleteSurface::new(
        "completion-slot-spec/witness",
        Span::new(start, start.saturating_add(1)),
        "completion slot witness",
    ))
}

/// Reserve one program through the production lexical walk, then complete every reserved
/// position with `records_per_position` records. `0` is what a clean file does at every one of
/// its sites; `1` is the opposite end of the same axis.
fn completion_work(source: &str, records_per_position: usize) -> EventCompletionWorkForTest {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(!parsed.panicked, "the spec program parses");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let scope = EventCompletionWorkScopeForTest::start();
    let mut store = EventStore::default();
    let mut reservations = LexicalReservations::default();
    reservations
        .reserve_program(
            ModuleOrdinal::new(0),
            UnitSlot::new(0),
            &parsed.program,
            &mut store,
        )
        .expect("the lexical walk reserves every owner");

    let tickets = reservations.tickets();
    for (index, ticket) in tickets.iter().enumerate() {
        let records = (0..records_per_position)
            .map(|offset| witness_record(index + offset))
            .collect();
        store
            .complete(*ticket, records)
            .expect("each reserved position completes exactly once");
    }
    let replayed = store
        .finish()
        .expect("the walk's tickets are exactly the store's reserved positions");
    assert_eq!(
        replayed.len(),
        tickets.len() * records_per_position,
        "every completed record replays"
    );

    let work = scope.finish();
    assert_eq!(
        work.reserved_record_positions,
        u64::try_from(tickets.len()).expect("ticket count fits u64"),
        "the walk's ticket list must be exactly the store's reserved positions"
    );
    work
}

/// Statement groups in the program under test. Large enough that per-site substrate dominates
/// the fixed scaffolding, small enough to stay a fast unit test.
const SPEC_GROUPS: usize = 64;

/// Bytes a reserved record position may retain while it holds no record: one machine word for a
/// dense arena slot, with room for a small header. Enough for a dense representation, not enough
/// for a keyed node that stores the replay key beside the value.
const COMPLETION_SLOT_BYTE_BUDGET: u64 = 24;

/// A completion slot exists to carry a record. A program that records nothing must not be
/// charged one per record position.
///
/// Both runs below check the **same program**: the same statements, the same lexical walk, the
/// same reserved events, the same reserved record positions. The single axis that moves is the
/// ratio of recording positions to statement sites — `silent` records at none of them, `loud` at
/// all of them. Reservation substrate identical across that axis is substrate paid for having
/// statements, not for having anything to say about them; on the `modules` bench family, which
/// replays **zero** records at every corpus size, all of it is dead.
///
/// The budgets are deliberately loose enough not to dictate a representation. A dense arena
/// indexed by reservation ordinal that materializes the primary position only, and a per-event
/// completion mask that materializes nothing at all, both pass; one eager slot per record
/// position does not, whichever container holds it.
#[test]
fn a_program_that_records_nothing_retains_no_completion_slots() {
    let source = module_source(SPEC_GROUPS);
    let silent = completion_work(&source, 0);
    let loud = completion_work(&source, 1);

    // The control: the two runs differ only in what they record.
    assert_eq!(
        (silent.reserved_events, silent.reserved_record_positions),
        (loud.reserved_events, loud.reserved_record_positions),
        "the sweep must hold the program, and therefore the reservation, constant"
    );
    assert_eq!(
        silent.replayed_records, 0,
        "the silent run must replay nothing"
    );
    assert_eq!(
        loud.replayed_records, loud.reserved_record_positions,
        "the loud run must replay one record per reserved position"
    );

    assert_eq!(
        silent.materialized_completion_slots, loud.materialized_completion_slots,
        "reservation materialized {} completion slots either way — {SPEC_GROUPS} statement \
         groups commit the same storage whether every position records ({loud:?}) or none does \
         ({silent:?}), which is what it means for a cost to be paid for a program's shape",
        silent.materialized_completion_slots
    );

    // Naming a record position costs nothing; keeping a slot for it does. One slot per event is
    // the ceiling — three per site (`immediate`/`deferred`/`incomplete`) is the defect.
    assert!(
        silent.materialized_completion_slots <= silent.reserved_events,
        "{} events over {} record positions materialized {} completion slots holding nothing \
         ({silent:?}) — the `deferred` and `incomplete` positions are paid for at every site \
         before anything can record there; the budget is one slot per event ({})",
        silent.reserved_events,
        silent.reserved_record_positions,
        silent.materialized_completion_slots,
        silent.reserved_events
    );

    // Bytes, not just slots: whatever holds a position must be a dense entry, not a keyed node.
    let byte_budget = COMPLETION_SLOT_BYTE_BUDGET * silent.reserved_record_positions;
    assert!(
        silent.materialized_completion_bytes <= byte_budget,
        "{} record positions that never record retained {} bytes of completion storage \
         ({silent:?}) — {} bytes per position against a budget of \
         {COMPLETION_SLOT_BYTE_BUDGET}, and that figure is a lower bound: it counts the \
         `BTreeMap`'s key and value and ignores its node headers and fill factor",
        silent.reserved_record_positions,
        silent.materialized_completion_bytes,
        silent.materialized_completion_bytes / silent.reserved_record_positions.max(1),
    );
}

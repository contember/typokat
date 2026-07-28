//! Test-only library-domain adapter for the shared lexical reservation walk.

use super::events_library::{
    LibraryEventLedger, LibraryEventLedgerError, LibraryRecordTicket, LibraryReservedEvent,
};
use super::lexical_events::{LexicalReservationAllocator, LexicalReservations, SourceSite};
use super::{attach_class_bindings, attach_type_decl_owners, ModuleDeclarationSpans};
use crate::binder::{scope::ScopeId, Binder};
use crate::source::{LibraryFileOrdinal, SourceOrdinal, SourceUnit};
use oxc_ast::ast::Program;

pub(crate) type ExactUnit = SourceUnit;

pub(crate) const fn library_unit(file_ordinal: LibraryFileOrdinal) -> ExactUnit {
    SourceUnit::Library { file_ordinal }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibraryLexicalEvidence(Vec<SourceUnit>);

#[cfg(any(test, feature = "test-utils"))]
impl LibraryLexicalEvidence {
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, SourceUnit> {
        self.0.iter()
    }
}

struct LibraryReservationAllocator<'ledger> {
    file_ordinal: LibraryFileOrdinal,
    ledger: &'ledger mut LibraryEventLedger,
}

impl LexicalReservationAllocator for LibraryReservationAllocator<'_> {
    type Event = LibraryReservedEvent;
    type Ticket = LibraryRecordTicket;
    type Error = LibraryEventLedgerError;

    fn source_unit(&self) -> SourceUnit {
        SourceUnit::Library {
            file_ordinal: self.file_ordinal,
        }
    }

    fn reserve_event(&mut self, source_start: u32) -> (Self::Event, Self::Ticket) {
        let event = self.ledger.reserve_event(self.file_ordinal, source_start);
        (event, event.primary)
    }

    fn reserve_record(&mut self, event: Self::Event) -> Result<Self::Ticket, Self::Error> {
        self.ledger.reserve_record(event.id)
    }
}

impl LexicalReservations<LibraryRecordTicket> {
    pub(crate) fn reserve_library_program(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
        ledger: &mut LibraryEventLedger,
    ) -> Result<(), LibraryEventLedgerError> {
        let mut allocator = LibraryReservationAllocator {
            file_ordinal,
            ledger,
        };
        self.reserve_program_with(program, &mut allocator)?;
        let source = SourceSite {
            unit: allocator.source_unit(),
            source_start: program.span.start,
        };
        let (_, owner) = allocator.reserve_event(source.source_start);
        self.retain_source_anchor(source, owner);
        Ok(())
    }

    pub(crate) fn library_semantic_tickets(&self) -> Vec<LibraryRecordTicket> {
        let mut tickets = self.source_anchor_tickets();
        tickets.extend(self.tickets());
        tickets
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn library_lexical_evidence(&self) -> LibraryLexicalEvidence {
        LibraryLexicalEvidence(self.retained_source_units())
    }

    pub(crate) fn attach_library_declaration_owners(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        binder: &Binder,
        scope: ScopeId,
        program: &Program<'_>,
        spans: &ModuleDeclarationSpans,
    ) {
        attach_type_decl_owners(
            self,
            SourceOrdinal::Library(file_ordinal),
            binder,
            scope,
            program,
            spans,
        );
    }

    pub(in crate::check::checker) fn attach_library_class_bindings(
        &mut self,
        file_ordinal: LibraryFileOrdinal,
        binder: &Binder,
        scope: ScopeId,
        program: &Program<'_>,
        declarations: &super::context::TypeDeclTable<'_>,
    ) {
        attach_class_bindings(
            self,
            SourceOrdinal::Library(file_ordinal),
            binder,
            scope,
            program,
            declarations,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::context::CheckerEffects;
    use super::super::events_library::{
        LibraryEventLedgerSnapshot, LibrarySemanticReportingAdapter,
    };
    use super::super::reporting_record::CheckerRecord;
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use std::collections::BTreeSet;

    fn assert_owned_terminal<T: Send + Sync + 'static>() {}

    #[test]
    fn lexical_evidence_is_owned_and_shareable() {
        assert_owned_terminal::<LibraryLexicalEvidence>();
    }

    fn empty_batch(
        owner: LibraryRecordTicket,
    ) -> super::super::context::CheckerRecordBatch<LibraryRecordTicket> {
        CheckerEffects::new(owner).records
    }

    #[test]
    fn empty_reference_and_comment_only_programs_keep_real_terminal_anchors() {
        for (index, source) in [
            "",
            "/// <reference lib=\"es5\" />\n",
            "// library profile metadata only\n",
        ]
        .into_iter()
        .enumerate()
        {
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            assert!(parsed.program.body.is_empty());
            let file_ordinal = LibraryFileOrdinal::new(index);
            let expected_unit = SourceUnit::Library { file_ordinal };
            let mut reservations = LexicalReservations::default();
            let mut ledger = LibraryEventLedger::default();
            reservations
                .reserve_library_program(file_ordinal, &parsed.program, &mut ledger)
                .unwrap();

            assert_eq!(
                ledger.snapshot(),
                LibraryEventLedgerSnapshot {
                    reserved_records: 1,
                    filled_records: 0,
                    reserved_file_ordinals: vec![file_ordinal],
                }
            );
            assert_eq!(
                reservations
                    .library_lexical_evidence()
                    .iter()
                    .copied()
                    .collect::<Vec<_>>(),
                [expected_unit]
            );
            let tickets = reservations.library_semantic_tickets();
            assert_eq!(tickets.len(), 1);
            LibrarySemanticReportingAdapter::new(&mut ledger)
                .complete_semantic_batches(tickets.into_iter().map(empty_batch).collect())
                .unwrap();
            let terminal = ledger.snapshot();
            assert!(terminal.reserved_records > 0);
            assert_eq!(terminal.filled_records, terminal.reserved_records);
            let records: Vec<(super::super::events_library::LibraryEventKey, CheckerRecord)> =
                ledger.finish().unwrap();
            assert!(records.is_empty());
        }
    }

    #[test]
    fn evidence_retains_every_real_library_source_unit() {
        let source = r#"
interface Shape<T> extends Base { value: T; call(input: number): string; }
declare namespace Outer { export { Missing as Alias }; }
class Box<T extends object = {}> {
  [computed()] = function (value = 1) { return value; };
  method<U>(value = () => 1) { return value(); }
}
function outer(value = () => 1) {
  if (value) { const nested = class { field = () => 1; }; }
  return function inner(input = 1) { return input; };
}
const arrow = (input = 1) => class Inner { method() { return input; } };
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let first = LibraryFileOrdinal::new(3);
        let second = LibraryFileOrdinal::new(7);
        let mut reservations = LexicalReservations::default();
        let mut ledger = LibraryEventLedger::default();
        reservations
            .reserve_library_program(first, &parsed.program, &mut ledger)
            .unwrap();
        reservations
            .reserve_library_program(second, &parsed.program, &mut ledger)
            .unwrap();

        let evidence = reservations.library_lexical_evidence();
        assert!(!evidence.is_empty());
        let units = evidence.iter().copied().collect::<Vec<_>>();
        assert_eq!(units, reservations.retained_source_units());
        assert_eq!(
            units.iter().copied().collect::<BTreeSet<_>>(),
            BTreeSet::from([
                SourceUnit::Library {
                    file_ordinal: first,
                },
                SourceUnit::Library {
                    file_ordinal: second,
                },
            ])
        );
        let first_count = units
            .iter()
            .filter(|unit| {
                **unit
                    == SourceUnit::Library {
                        file_ordinal: first,
                    }
            })
            .count();
        let second_count = units
            .iter()
            .filter(|unit| {
                **unit
                    == SourceUnit::Library {
                        file_ordinal: second,
                    }
            })
            .count();
        assert!(first_count > 0);
        assert_eq!(first_count, second_count);
    }
}

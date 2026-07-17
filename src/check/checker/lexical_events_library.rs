//! Test-only library-domain adapter for the shared lexical reservation walk.

use super::events_library::{
    LibraryEventLedger, LibraryEventLedgerError, LibraryRecordTicket, LibraryReservedEvent,
};
use super::lexical_events::{LexicalReservationAllocator, LexicalReservations};
use crate::source::{LibraryFileOrdinal, SourceUnit};
use oxc_ast::ast::Program;

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
        self.reserve_program_with(program, &mut allocator)
    }
}

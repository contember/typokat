//! User-domain adapter for the shared lexical reservation walk.

use super::events::{EventStore, EventStoreError, ReservedEvent, UserRecordTicket};
use super::lexical_events::{LexicalReservationAllocator, LexicalReservations};
use super::PrivateCombinedRecordTicket;
use crate::source::{ModuleOrdinal, SourceUnit, UnitSlot};
use oxc_ast::ast::Program;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReservationError {
    EventStore(EventStoreError),
}

impl From<EventStoreError> for ReservationError {
    fn from(error: EventStoreError) -> Self {
        Self::EventStore(error)
    }
}

struct UserReservationAllocator<'store> {
    module_ordinal: ModuleOrdinal,
    unit_slot: UnitSlot,
    store: &'store mut EventStore,
}

impl LexicalReservationAllocator for UserReservationAllocator<'_> {
    type Event = ReservedEvent;
    type Ticket = UserRecordTicket;
    type Error = EventStoreError;

    fn source_unit(&self) -> SourceUnit {
        SourceUnit::User {
            module_ordinal: self.module_ordinal,
            unit_slot: self.unit_slot,
        }
    }

    fn reserve_event(&mut self, source_start: u32) -> (Self::Event, Self::Ticket) {
        let event = self.store.reserve_event(self.module_ordinal, source_start);
        (event, event.primary)
    }

    fn reserve_record(&mut self, event: Self::Event) -> Result<Self::Ticket, Self::Error> {
        self.store.reserve_record(event.id)
    }
}

impl LexicalReservationAllocator for PrivateCombinedUserReservationAllocator<'_> {
    type Event = ReservedEvent;
    type Ticket = PrivateCombinedRecordTicket;
    type Error = EventStoreError;

    fn source_unit(&self) -> SourceUnit {
        SourceUnit::User {
            module_ordinal: self.module_ordinal,
            unit_slot: self.unit_slot,
        }
    }

    fn reserve_event(&mut self, source_start: u32) -> (Self::Event, Self::Ticket) {
        let event = self.store.reserve_event(self.module_ordinal, source_start);
        (event, PrivateCombinedRecordTicket::User(event.primary))
    }

    fn reserve_record(&mut self, event: Self::Event) -> Result<Self::Ticket, Self::Error> {
        self.store
            .reserve_record(event.id)
            .map(PrivateCombinedRecordTicket::User)
    }
}

struct PrivateCombinedUserReservationAllocator<'store> {
    module_ordinal: ModuleOrdinal,
    unit_slot: UnitSlot,
    store: &'store mut EventStore,
}

impl LexicalReservations<UserRecordTicket> {
    /// Walk one user program in lexical order and reserve every record owner.
    pub(crate) fn reserve_program(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &Program<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let mut allocator = UserReservationAllocator {
            module_ordinal,
            unit_slot,
            store,
        };
        self.reserve_program_with(program, &mut allocator)
            .map_err(ReservationError::from)
    }

    pub(crate) fn reserve_continuation_program(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &Program<'_>,
        context: crate::binder::namespace::ModuleBindingContext,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let mut allocator = UserReservationAllocator {
            module_ordinal,
            unit_slot,
            store,
        };
        self.reserve_continuation_program_with(program, context, &mut allocator)
            .map_err(ReservationError::from)
    }
}

impl LexicalReservations<PrivateCombinedRecordTicket> {
    pub(crate) fn reserve_private_continuation_program(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &Program<'_>,
        context: crate::binder::namespace::ModuleBindingContext,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let mut allocator = PrivateCombinedUserReservationAllocator {
            module_ordinal,
            unit_slot,
            store,
        };
        self.reserve_continuation_program_with(program, context, &mut allocator)
            .map_err(ReservationError::from)
    }
}

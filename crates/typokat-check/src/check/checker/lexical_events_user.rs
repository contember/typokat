//! User-domain adapter for the shared lexical reservation walk.

use super::events::{EventStore, EventStoreError, ReservedEvent, UserRecordTicket};
use super::lexical_events::{LexicalReservationAllocator, LexicalReservations};
use super::PrivateCombinedRecordTicket;
use crate::binder::declaration::SourceDeclarationManifest;
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
    #[allow(dead_code)] // Compatibility tests may build a transient manifest.
    pub(crate) fn reserve_program<'ast>(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &'ast Program<'ast>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let manifest = SourceDeclarationManifest::build(program);
        self.reserve_user_program_with_declaration_manifest(
            module_ordinal,
            unit_slot,
            program,
            &manifest,
            store,
        )
    }

    pub(crate) fn reserve_user_program_with_declaration_manifest(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &Program<'_>,
        manifest: &SourceDeclarationManifest<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let mut allocator = UserReservationAllocator {
            module_ordinal,
            unit_slot,
            store,
        };
        LexicalReservations::reserve_program_with_declaration_manifest(
            self,
            program,
            manifest,
            &mut allocator,
        )
        .map_err(ReservationError::from)
    }

    #[allow(dead_code)] // Compatibility tests may build a transient manifest.
    pub(crate) fn reserve_continuation_program<'ast>(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &'ast Program<'ast>,
        context: crate::binder::namespace::ModuleBindingContext,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let manifest = SourceDeclarationManifest::build(program);
        self.reserve_user_continuation_program_with_declaration_manifest(
            module_ordinal,
            unit_slot,
            program,
            &manifest,
            context,
            store,
        )
    }

    pub(crate) fn reserve_user_continuation_program_with_declaration_manifest(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &Program<'_>,
        manifest: &SourceDeclarationManifest<'_>,
        context: crate::binder::namespace::ModuleBindingContext,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let mut allocator = UserReservationAllocator {
            module_ordinal,
            unit_slot,
            store,
        };
        LexicalReservations::reserve_continuation_program_with_declaration_manifest(
            self,
            program,
            manifest,
            context,
            &mut allocator,
        )
        .map_err(ReservationError::from)
    }
}

impl LexicalReservations<PrivateCombinedRecordTicket> {
    #[allow(dead_code)] // Compatibility tests may build a transient manifest.
    pub(crate) fn reserve_private_continuation_program<'ast>(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &'ast Program<'ast>,
        context: crate::binder::namespace::ModuleBindingContext,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let manifest = SourceDeclarationManifest::build(program);
        self.reserve_private_continuation_program_with_declaration_manifest(
            module_ordinal,
            unit_slot,
            program,
            &manifest,
            context,
            store,
        )
    }

    pub(crate) fn reserve_private_continuation_program_with_declaration_manifest(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &Program<'_>,
        manifest: &SourceDeclarationManifest<'_>,
        context: crate::binder::namespace::ModuleBindingContext,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let mut allocator = PrivateCombinedUserReservationAllocator {
            module_ordinal,
            unit_slot,
            store,
        };
        LexicalReservations::reserve_continuation_program_with_declaration_manifest(
            self,
            program,
            manifest,
            context,
            &mut allocator,
        )
        .map_err(ReservationError::from)
    }
}

//! Lexical event reservations retained across class/SCC/body phases.

use super::events::UserRecordTicket;
use crate::binder::declaration::{
    source_declaration_occurrences, DeclId, DeclarationKind, TypeGroupId, ValueStorageId,
};
#[cfg(test)]
use crate::source::{ModuleOrdinal, UnitSlot};
use crate::source::{SourceOrdinal, SourceUnit};
use crate::span::Span;
use crate::types::repr::{ClassId, TypeParamId};
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, ChainElement, Class, ClassElement, Declaration,
    ExportDefaultDeclarationKind, Expression, ForStatementInit, ForStatementLeft, FormalParameters,
    Function, ObjectPropertyKind, Program, PropertyKey, Statement, TSModuleDeclaration,
    TSModuleDeclarationBody, TSSignature, VariableDeclaration,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

#[cfg(test)]
thread_local! {
    static LEXICAL_OWNER_INDEX_PROBES: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    static DECLARATION_RESERVATION_INDEX_PROBES: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_lexical_owner_index_probe_for_test() {
    LEXICAL_OWNER_INDEX_PROBES.set(LEXICAL_OWNER_INDEX_PROBES.get().saturating_add(1));
}

#[cfg(test)]
fn record_declaration_reservation_index_probe_for_test() {
    DECLARATION_RESERVATION_INDEX_PROBES
        .set(DECLARATION_RESERVATION_INDEX_PROBES.get().saturating_add(1));
}

#[cfg(test)]
pub(crate) struct LexicalOwnerLookupScope(u64);

#[cfg(test)]
impl LexicalOwnerLookupScope {
    pub(crate) fn start() -> Self {
        Self(LEXICAL_OWNER_INDEX_PROBES.get())
    }

    pub(crate) fn finish(self) -> u64 {
        LEXICAL_OWNER_INDEX_PROBES.get().saturating_sub(self.0)
    }
}

#[cfg(test)]
pub(crate) struct DeclarationReservationLookupScope(u64);

#[cfg(test)]
impl DeclarationReservationLookupScope {
    pub(crate) fn start() -> Self {
        Self(DECLARATION_RESERVATION_INDEX_PROBES.get())
    }

    pub(crate) fn finish(self) -> u64 {
        DECLARATION_RESERVATION_INDEX_PROBES
            .get()
            .saturating_sub(self.0)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ClassSiteId(usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MemberSiteId(usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CallableSiteId(usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceSite {
    pub(crate) unit: SourceUnit,
    pub(crate) source_start: u32,
}

impl SourceSite {
    #[cfg(test)]
    pub(crate) const fn user(
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        source_start: u32,
    ) -> Self {
        Self {
            unit: SourceUnit::User {
                module_ordinal,
                unit_slot,
            },
            source_start,
        }
    }

    pub(crate) const fn ordinal(self) -> SourceOrdinal {
        source_ordinal(self.unit)
    }
}

pub(crate) const fn source_ordinal(source: SourceUnit) -> SourceOrdinal {
    match source {
        SourceUnit::User { module_ordinal, .. } => SourceOrdinal::User(module_ordinal),
        SourceUnit::Library { file_ordinal } => SourceOrdinal::Library(file_ordinal),
    }
}

/// Record positions retained by every callable reservation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteTickets<Ticket: Copy = UserRecordTicket> {
    pub(crate) immediate: Ticket,
    pub(crate) deferred: Ticket,
    pub(crate) incomplete: Ticket,
}

/// Record positions retained by every callable reservation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableTickets<Ticket: Copy = UserRecordTicket> {
    pub(crate) signature: Ticket,
    pub(crate) deferred: Ticket,
    pub(crate) incomplete: Ticket,
    pub(crate) body: Ticket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LexicalOwnerPhase {
    Immediate,
    Deferred,
    Incomplete,
    Body,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexicalOwner<Ticket: Copy = UserRecordTicket> {
    pub(crate) ticket: Ticket,
}

/// Neutral event reserved for one exact source declaration occurrence.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclarationReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) source: SourceSite,
    pub(crate) kind: DeclarationKind,
    pub(crate) declaration_span: Span,
    pub(crate) binding_span: Span,
    pub(crate) owner: Ticket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ExportAliasReservation<Ticket: Copy = UserRecordTicket> {
    source: SourceSite,
    local_span: Span,
    owner: Ticket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum InterfaceOccurrenceKind {
    Header,
    Member,
    Heritage,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct InterfaceOccurrenceReservation<Ticket: Copy = UserRecordTicket> {
    source: SourceSite,
    binding_start: u32,
    kind: InterfaceOccurrenceKind,
    owner: Ticket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct SourceAnchorReservation<Ticket: Copy = UserRecordTicket> {
    source: SourceSite,
    owner: Ticket,
}

/// Stable class identities attached after binder/type reservation and before fill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassBinding {
    pub(crate) class_id: ClassId,
    pub(crate) type_decl: TypeGroupId,
    pub(crate) value_decl: Option<ValueStorageId>,
    pub(crate) header_type_params: Vec<TypeParamId>,
}

/// Stable callable binders attached during the same reservation phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableBinding {
    pub(crate) type_params: Vec<TypeParamId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TopLevelReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) source: SourceSite,
    pub(crate) tickets: SiteTickets<Ticket>,
    pub(crate) class: Option<ClassSiteId>,
    pub(crate) callable: Option<CallableSiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NestedStatementReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) source: SourceSite,
    pub(crate) tickets: SiteTickets<Ticket>,
    pub(crate) callable: Option<CallableSiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclaratorReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) source: SourceSite,
    pub(crate) tickets: SiteTickets<Ticket>,
}

/// One lexical owner for an initializer's assignment relation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct InitializerReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) source: SourceSite,
    pub(crate) owner: Ticket,
}

/// One lexical owner per source class type-parameter default.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassDefaultReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) parameter_index: usize,
    pub(crate) source: SourceSite,
    pub(crate) owner: Ticket,
}

/// One lexical owner per source class type-parameter constraint.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassConstraintReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) parameter_index: usize,
    pub(crate) source: SourceSite,
    pub(crate) owner: Ticket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) id: ClassSiteId,
    pub(crate) source: SourceSite,
    pub(crate) tickets: SiteTickets<Ticket>,
    pub(crate) constraints: Vec<ClassConstraintReservation<Ticket>>,
    pub(crate) defaults: Vec<ClassDefaultReservation<Ticket>>,
    pub(crate) members: Vec<MemberSiteId>,
    pub(crate) binding: Option<ClassBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) id: MemberSiteId,
    pub(crate) class: ClassSiteId,
    pub(crate) source: SourceSite,
    pub(crate) tickets: SiteTickets<Ticket>,
    pub(crate) callable: Option<CallableSiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableReservation<Ticket: Copy = UserRecordTicket> {
    pub(crate) id: CallableSiteId,
    pub(crate) owner_member: Option<MemberSiteId>,
    pub(crate) source: SourceSite,
    pub(crate) tickets: CallableTickets<Ticket>,
    type_parameter_count: usize,
    pub(crate) binding: Option<CallableBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReservationStateError {
    UnknownClass(ClassSiteId),
    DuplicateClassBinding(ClassSiteId),
    DuplicateCallableBinding(CallableSiteId),
    MissingDeclarationOwner(DeclId),
}

pub(super) trait LexicalReservationAllocator {
    type Event: Copy;
    type Ticket: Copy + PartialEq;
    type Error;

    fn source_unit(&self) -> SourceUnit;
    fn reserve_event(&mut self, source_start: u32) -> (Self::Event, Self::Ticket);
    fn reserve_record(&mut self, event: Self::Event) -> Result<Self::Ticket, Self::Error>;
}

/// Persistent source-site table built before class construction, SCCs, and bodies.
#[derive(Debug)]
pub(crate) struct LexicalReservations<Ticket: Copy = UserRecordTicket> {
    top_level: Vec<TopLevelReservation<Ticket>>,
    top_level_by_source: FxHashMap<(SourceOrdinal, u32), usize>,
    nested_statements: Vec<NestedStatementReservation<Ticket>>,
    nested_statements_by_source: FxHashMap<(SourceOrdinal, u32), usize>,
    declarators: Vec<DeclaratorReservation<Ticket>>,
    declarators_by_source: FxHashMap<(SourceOrdinal, u32), usize>,
    initializers: Vec<InitializerReservation<Ticket>>,
    classes: Vec<ClassReservation<Ticket>>,
    members: Vec<MemberReservation<Ticket>>,
    members_by_source: FxHashMap<(SourceOrdinal, u32), usize>,
    callables: Vec<CallableReservation<Ticket>>,
    expression_site_tickets: Vec<SiteTickets<Ticket>>,
    #[cfg(test)]
    expression_sources: Vec<SourceSite>,
    source_anchors: Vec<SourceAnchorReservation<Ticket>>,
    declarations: Vec<DeclarationReservation<Ticket>>,
    export_aliases: Vec<ExportAliasReservation<Ticket>>,
    interface_occurrences: Vec<InterfaceOccurrenceReservation<Ticket>>,
    interface_occurrences_by_source:
        FxHashMap<(SourceOrdinal, u32, InterfaceOccurrenceKind, u32), usize>,
    declarations_by_binding: FxHashMap<(SourceOrdinal, u32, u32), usize>,
    export_aliases_by_local_span: FxHashMap<(SourceOrdinal, u32, u32), usize>,
    classes_by_source: BTreeMap<(SourceOrdinal, u32), Vec<ClassSiteId>>,
    callables_by_source: BTreeMap<(SourceOrdinal, u32), Vec<CallableSiteId>>,
    initializers_by_source: FxHashMap<(SourceUnit, u32), usize>,
    declaration_reservations_by_decl: FxHashMap<DeclId, usize>,
}

impl<Ticket: Copy> Default for LexicalReservations<Ticket> {
    fn default() -> Self {
        Self {
            top_level: Vec::new(),
            top_level_by_source: FxHashMap::default(),
            nested_statements: Vec::new(),
            nested_statements_by_source: FxHashMap::default(),
            declarators: Vec::new(),
            declarators_by_source: FxHashMap::default(),
            initializers: Vec::new(),
            classes: Vec::new(),
            members: Vec::new(),
            members_by_source: FxHashMap::default(),
            callables: Vec::new(),
            expression_site_tickets: Vec::new(),
            #[cfg(test)]
            expression_sources: Vec::new(),
            source_anchors: Vec::new(),
            declarations: Vec::new(),
            export_aliases: Vec::new(),
            interface_occurrences: Vec::new(),
            interface_occurrences_by_source: FxHashMap::default(),
            declarations_by_binding: FxHashMap::default(),
            export_aliases_by_local_span: FxHashMap::default(),
            classes_by_source: BTreeMap::new(),
            callables_by_source: BTreeMap::new(),
            initializers_by_source: FxHashMap::default(),
            declaration_reservations_by_decl: FxHashMap::default(),
        }
    }
}

impl<Ticket: Copy + PartialEq> LexicalReservations<Ticket> {
    /// Walk one program in lexical order and reserve all top-level/class/callable sites.
    pub(super) fn reserve_program_with<Allocator>(
        &mut self,
        program: &Program<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let unit = allocator.source_unit();
        let ordinal = source_ordinal(unit);
        for occurrence in source_declaration_occurrences(program) {
            let (_, owner) = allocator.reserve_event(occurrence.binding_span.start);
            let index = self.declarations.len();
            self.declarations.push(DeclarationReservation {
                source: SourceSite {
                    unit,
                    source_start: occurrence.declaration_span.start,
                },
                kind: occurrence.kind,
                declaration_span: occurrence.declaration_span,
                binding_span: occurrence.binding_span,
                owner,
            });
            let previous = self.declarations_by_binding.insert(
                (
                    ordinal,
                    occurrence.binding_span.start,
                    occurrence.binding_span.end,
                ),
                index,
            );
            debug_assert!(
                previous.is_none(),
                "one declaration per exact binding range"
            );
        }
        for statement in &program.body {
            match statement {
                Statement::TSModuleDeclaration(declaration) => {
                    self.reserve_export_aliases_in_module(unit, declaration, allocator)?;
                }
                Statement::ExportNamedDeclaration(export) => {
                    if let Some(Declaration::TSModuleDeclaration(declaration)) = &export.declaration
                    {
                        self.reserve_export_aliases_in_module(unit, declaration, allocator)?;
                    }
                }
                _ => {}
            }
        }
        self.reserve_interface_occurrences_in_statements(unit, &program.body, allocator);
        self.reserve_statement_list(unit, &program.body, true, allocator)
    }

    fn reserve_interface_occurrences_in_statements<Allocator>(
        &mut self,
        unit: SourceUnit,
        statements: &[Statement<'_>],
        allocator: &mut Allocator,
    ) where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        for statement in statements {
            match statement {
                Statement::TSInterfaceDeclaration(interface) => {
                    self.reserve_interface_occurrences(unit, interface, allocator);
                }
                Statement::ExportNamedDeclaration(export) => match &export.declaration {
                    Some(Declaration::TSInterfaceDeclaration(interface)) => {
                        self.reserve_interface_occurrences(unit, interface, allocator);
                    }
                    Some(Declaration::TSModuleDeclaration(module)) => {
                        self.reserve_interface_occurrences_in_module(unit, module, allocator);
                    }
                    Some(Declaration::TSGlobalDeclaration(global)) => {
                        self.reserve_interface_occurrences_in_statements(
                            unit,
                            &global.body.body,
                            allocator,
                        );
                    }
                    _ => {}
                },
                Statement::TSModuleDeclaration(module) => {
                    self.reserve_interface_occurrences_in_module(unit, module, allocator);
                }
                Statement::TSGlobalDeclaration(global) => {
                    self.reserve_interface_occurrences_in_statements(
                        unit,
                        &global.body.body,
                        allocator,
                    );
                }
                _ => {}
            }
        }
    }

    fn reserve_interface_occurrences_in_module<Allocator>(
        &mut self,
        unit: SourceUnit,
        module: &TSModuleDeclaration<'_>,
        allocator: &mut Allocator,
    ) where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        match &module.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                self.reserve_interface_occurrences_in_statements(unit, &block.body, allocator)
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
                self.reserve_interface_occurrences_in_module(unit, nested, allocator);
            }
            None => {}
        }
    }

    fn reserve_interface_occurrences<Allocator>(
        &mut self,
        unit: SourceUnit,
        interface: &oxc_ast::ast::TSInterfaceDeclaration<'_>,
        allocator: &mut Allocator,
    ) where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let binding_start = interface.id.span.start;
        self.reserve_interface_occurrence(
            unit,
            binding_start,
            InterfaceOccurrenceKind::Header,
            binding_start,
            allocator,
        );
        for heritage in &interface.extends {
            self.reserve_interface_occurrence(
                unit,
                binding_start,
                InterfaceOccurrenceKind::Heritage,
                heritage.span.start,
                allocator,
            );
        }
        for member in &interface.body.body {
            let source_start = match member {
                TSSignature::TSPropertySignature(signature) => signature.span.start,
                TSSignature::TSMethodSignature(signature) => signature.span.start,
                TSSignature::TSCallSignatureDeclaration(signature) => signature.span.start,
                TSSignature::TSConstructSignatureDeclaration(signature) => signature.span.start,
                TSSignature::TSIndexSignature(signature) => signature.span.start,
            };
            self.reserve_interface_occurrence(
                unit,
                binding_start,
                InterfaceOccurrenceKind::Member,
                source_start,
                allocator,
            );
        }
    }

    fn reserve_interface_occurrence<Allocator>(
        &mut self,
        unit: SourceUnit,
        binding_start: u32,
        kind: InterfaceOccurrenceKind,
        source_start: u32,
        allocator: &mut Allocator,
    ) where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let (_, owner) = allocator.reserve_event(source_start);
        let index = self.interface_occurrences.len();
        self.interface_occurrences
            .push(InterfaceOccurrenceReservation {
                source: SourceSite { unit, source_start },
                binding_start,
                kind,
                owner,
            });
        let previous = self.interface_occurrences_by_source.insert(
            (source_ordinal(unit), binding_start, kind, source_start),
            index,
        );
        debug_assert!(previous.is_none(), "one exact interface occurrence owner");
    }

    fn reserve_export_aliases_in_statement<Allocator>(
        &mut self,
        unit: SourceUnit,
        statement: &Statement<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        match statement {
            Statement::TSModuleDeclaration(declaration) => {
                self.reserve_export_aliases_in_module(unit, declaration, allocator)?;
            }
            Statement::ExportNamedDeclaration(export) => {
                if export.source.is_none() && export.declaration.is_none() {
                    for specifier in &export.specifiers {
                        let local_span = Span::from_oxc(specifier.local.span());
                        let (_, owner) = allocator.reserve_event(local_span.start);
                        let index = self.export_aliases.len();
                        self.export_aliases.push(ExportAliasReservation {
                            source: SourceSite {
                                unit,
                                source_start: local_span.start,
                            },
                            local_span,
                            owner,
                        });
                        let previous = self.export_aliases_by_local_span.insert(
                            (source_ordinal(unit), local_span.start, local_span.end),
                            index,
                        );
                        debug_assert!(
                            previous.is_none(),
                            "one export alias reservation per exact local span"
                        );
                    }
                }
                if let Some(Declaration::TSModuleDeclaration(declaration)) = &export.declaration {
                    self.reserve_export_aliases_in_module(unit, declaration, allocator)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn reserve_export_aliases_in_module<Allocator>(
        &mut self,
        unit: SourceUnit,
        declaration: &TSModuleDeclaration<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        match &declaration.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                for statement in &block.body {
                    self.reserve_export_aliases_in_statement(unit, statement, allocator)?;
                }
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
                self.reserve_export_aliases_in_module(unit, nested, allocator)?;
            }
            None => {}
        }
        Ok(())
    }

    fn reserve_statement_list<Allocator>(
        &mut self,
        unit: SourceUnit,
        statements: &[Statement<'_>],
        top_level: bool,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        for statement in statements {
            self.reserve_statement(unit, statement, top_level, allocator)?;
        }
        Ok(())
    }

    fn reserve_statement<Allocator>(
        &mut self,
        unit: SourceUnit,
        statement: &Statement<'_>,
        top_level: bool,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let source = SourceSite {
            unit,
            source_start: statement.span().start,
        };
        let (event, primary) = allocator.reserve_event(source.source_start);
        let tickets = reserve_site_tickets(event, primary, allocator)?;
        let mut class_site = None;
        let mut callable = None;
        if let Some(class) = statement_class(statement) {
            class_site = Some(self.reserve_class(source, class, tickets, allocator)?);
        } else if let Some(function) = statement_function(statement) {
            callable =
                Some(self.reserve_callable(source, event, tickets, None, function, allocator)?);
        }
        if top_level {
            let index = self.top_level.len();
            self.top_level.push(TopLevelReservation {
                source,
                tickets,
                class: class_site,
                callable,
            });
            // Source starts collide, so the index keeps the first row the linear scan found.
            self.top_level_by_source
                .entry((source.ordinal(), source.source_start))
                .or_insert(index);
        } else {
            let index = self.nested_statements.len();
            self.nested_statements.push(NestedStatementReservation {
                source,
                tickets,
                callable,
            });
            self.nested_statements_by_source
                .entry((source.ordinal(), source.source_start))
                .or_insert(index);
        }

        if let Some(declaration) = statement_variable_declaration(statement) {
            self.reserve_declarators(source, declaration, allocator)?;
        }
        if let Some(function) = statement_function(statement) {
            self.reserve_parameter_expressions(source, &function.params, allocator)?;
            if let Some(body) = function.body.as_ref() {
                self.reserve_statement_list(unit, &body.statements, false, allocator)?;
            }
            return Ok(());
        }
        if statement_class(statement).is_some() {
            return Ok(());
        }
        self.reserve_statement_expressions(source, statement, allocator)?;
        match statement {
            Statement::BlockStatement(block) => {
                self.reserve_statement_list(unit, &block.body, false, allocator)?
            }
            Statement::IfStatement(statement) => {
                self.reserve_statement(unit, &statement.consequent, false, allocator)?;
                if let Some(alternate) = statement.alternate.as_ref() {
                    self.reserve_statement(unit, alternate, false, allocator)?;
                }
            }
            Statement::DoWhileStatement(statement) => {
                self.reserve_statement(unit, &statement.body, false, allocator)?
            }
            Statement::WhileStatement(statement) => {
                self.reserve_statement(unit, &statement.body, false, allocator)?
            }
            Statement::ForStatement(statement) => {
                if let Some(ForStatementInit::VariableDeclaration(declaration)) = &statement.init {
                    self.reserve_declarators(source, declaration, allocator)?;
                }
                self.reserve_statement(unit, &statement.body, false, allocator)?;
            }
            Statement::ForInStatement(statement) => {
                if let ForStatementLeft::VariableDeclaration(declaration) = &statement.left {
                    self.reserve_declarators(source, declaration, allocator)?;
                }
                self.reserve_statement(unit, &statement.body, false, allocator)?;
            }
            Statement::ForOfStatement(statement) => {
                if let ForStatementLeft::VariableDeclaration(declaration) = &statement.left {
                    self.reserve_declarators(source, declaration, allocator)?;
                }
                self.reserve_statement(unit, &statement.body, false, allocator)?;
            }
            Statement::WithStatement(statement) => {
                self.reserve_statement(unit, &statement.body, false, allocator)?
            }
            Statement::SwitchStatement(statement) => {
                for case in &statement.cases {
                    self.reserve_statement_list(unit, &case.consequent, false, allocator)?;
                }
            }
            Statement::LabeledStatement(statement) => {
                self.reserve_statement(unit, &statement.body, false, allocator)?
            }
            Statement::TryStatement(statement) => {
                self.reserve_statement_list(unit, &statement.block.body, false, allocator)?;
                if let Some(handler) = &statement.handler {
                    self.reserve_statement_list(unit, &handler.body.body, false, allocator)?;
                }
                if let Some(finalizer) = &statement.finalizer {
                    self.reserve_statement_list(unit, &finalizer.body, false, allocator)?;
                }
            }
            Statement::TSModuleDeclaration(module) => {
                self.reserve_module_statements(unit, module, allocator)?;
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::TSModuleDeclaration(module)) = &export.declaration {
                    self.reserve_module_statements(unit, module, allocator)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn reserve_module_statements<Allocator>(
        &mut self,
        unit: SourceUnit,
        module: &TSModuleDeclaration<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        match &module.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                self.reserve_statement_list(unit, &block.body, false, allocator)?;
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
                self.reserve_module_statements(unit, nested, allocator)?;
            }
            None => {}
        }
        Ok(())
    }

    fn reserve_declarators<Allocator>(
        &mut self,
        parent: SourceSite,
        declaration: &VariableDeclaration<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        for declarator in &declaration.declarations {
            let source = SourceSite {
                source_start: declarator.span.start,
                ..parent
            };
            let (event, primary) = allocator.reserve_event(source.source_start);
            let tickets = reserve_site_tickets(event, primary, allocator)?;
            let index = self.declarators.len();
            self.declarators
                .push(DeclaratorReservation { source, tickets });
            self.declarators_by_source
                .entry((source.ordinal(), source.source_start))
                .or_insert(index);
            if let Some(initializer) = &declarator.init {
                self.reserve_initializer(source.unit, initializer, allocator);
                self.reserve_expression(source, initializer, allocator)?;
            }
        }
        Ok(())
    }

    fn reserve_statement_expressions<Allocator>(
        &mut self,
        source: SourceSite,
        statement: &Statement<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        match statement {
            Statement::DoWhileStatement(statement) => {
                self.reserve_expression(source, &statement.test, allocator)?;
            }
            Statement::ExpressionStatement(statement) => {
                self.reserve_expression(source, &statement.expression, allocator)?;
            }
            Statement::ForStatement(statement) => {
                if let Some(expression) = statement
                    .init
                    .as_ref()
                    .and_then(ForStatementInit::as_expression)
                {
                    self.reserve_expression(source, expression, allocator)?;
                }
                if let Some(test) = &statement.test {
                    self.reserve_expression(source, test, allocator)?;
                }
                if let Some(update) = &statement.update {
                    self.reserve_expression(source, update, allocator)?;
                }
            }
            Statement::ForInStatement(statement) => {
                self.reserve_expression(source, &statement.right, allocator)?;
            }
            Statement::ForOfStatement(statement) => {
                self.reserve_expression(source, &statement.right, allocator)?;
            }
            Statement::IfStatement(statement) => {
                self.reserve_expression(source, &statement.test, allocator)?;
            }
            Statement::ReturnStatement(statement) => {
                if let Some(argument) = &statement.argument {
                    self.reserve_expression(source, argument, allocator)?;
                }
            }
            Statement::SwitchStatement(statement) => {
                self.reserve_expression(source, &statement.discriminant, allocator)?;
                for case in &statement.cases {
                    if let Some(test) = &case.test {
                        self.reserve_expression(source, test, allocator)?;
                    }
                }
            }
            Statement::ThrowStatement(statement) => {
                self.reserve_expression(source, &statement.argument, allocator)?;
            }
            Statement::WhileStatement(statement) => {
                self.reserve_expression(source, &statement.test, allocator)?;
            }
            Statement::WithStatement(statement) => {
                self.reserve_expression(source, &statement.object, allocator)?;
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let Some(expression) = export.declaration.as_expression() {
                    self.reserve_expression(source, expression, allocator)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn reserve_expression<Allocator>(
        &mut self,
        source: SourceSite,
        expression: &Expression<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        match expression {
            Expression::TemplateLiteral(template) => {
                for expression in &template.expressions {
                    self.reserve_expression(source, expression, allocator)?;
                }
            }
            Expression::ArrayExpression(array) => {
                for element in &array.elements {
                    if let Some(expression) = element.as_expression() {
                        self.reserve_expression(source, expression, allocator)?;
                    } else if let ArrayExpressionElement::SpreadElement(spread) = element {
                        self.reserve_expression(source, &spread.argument, allocator)?;
                    }
                }
            }
            Expression::ArrowFunctionExpression(arrow) => {
                self.reserve_arrow_expression(source, arrow, allocator)?;
            }
            Expression::AssignmentExpression(assignment) => {
                self.reserve_expression(source, &assignment.right, allocator)?;
            }
            Expression::AwaitExpression(await_expression) => {
                self.reserve_expression(source, &await_expression.argument, allocator)?;
            }
            Expression::BinaryExpression(binary) => {
                self.reserve_expression(source, &binary.left, allocator)?;
                self.reserve_expression(source, &binary.right, allocator)?;
            }
            Expression::CallExpression(call) => {
                self.reserve_call_expression(source, call, allocator)?;
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(call) => {
                    self.reserve_call_expression(source, call, allocator)?;
                }
                ChainElement::TSNonNullExpression(expression) => {
                    self.reserve_expression(source, &expression.expression, allocator)?;
                }
                ChainElement::ComputedMemberExpression(member) => {
                    self.reserve_expression(source, &member.object, allocator)?;
                    self.reserve_expression(source, &member.expression, allocator)?;
                }
                ChainElement::StaticMemberExpression(member) => {
                    self.reserve_expression(source, &member.object, allocator)?;
                }
                ChainElement::PrivateFieldExpression(member) => {
                    self.reserve_expression(source, &member.object, allocator)?;
                }
            },
            Expression::ClassExpression(class) => {
                self.reserve_class_expression(source, class, allocator)?;
            }
            Expression::ConditionalExpression(conditional) => {
                self.reserve_expression(source, &conditional.test, allocator)?;
                self.reserve_expression(source, &conditional.consequent, allocator)?;
                self.reserve_expression(source, &conditional.alternate, allocator)?;
            }
            Expression::FunctionExpression(function) => {
                self.reserve_function_expression(source, function, allocator)?;
            }
            Expression::ImportExpression(import) => {
                self.reserve_expression(source, &import.source, allocator)?;
                if let Some(options) = &import.options {
                    self.reserve_expression(source, options, allocator)?;
                }
            }
            Expression::LogicalExpression(logical) => {
                self.reserve_expression(source, &logical.left, allocator)?;
                self.reserve_expression(source, &logical.right, allocator)?;
            }
            Expression::NewExpression(new_expression) => {
                self.reserve_expression(source, &new_expression.callee, allocator)?;
                self.reserve_arguments(source, &new_expression.arguments, allocator)?;
            }
            Expression::ObjectExpression(object) => {
                for property in &object.properties {
                    match property {
                        ObjectPropertyKind::ObjectProperty(property) => {
                            self.reserve_property_key(source, &property.key, allocator)?;
                            self.reserve_expression(source, &property.value, allocator)?;
                        }
                        ObjectPropertyKind::SpreadProperty(spread) => {
                            self.reserve_expression(source, &spread.argument, allocator)?;
                        }
                    }
                }
            }
            Expression::ParenthesizedExpression(parenthesized) => {
                self.reserve_expression(source, &parenthesized.expression, allocator)?;
            }
            Expression::SequenceExpression(sequence) => {
                for expression in &sequence.expressions {
                    self.reserve_expression(source, expression, allocator)?;
                }
            }
            Expression::TaggedTemplateExpression(tagged) => {
                self.reserve_expression(source, &tagged.tag, allocator)?;
                for expression in &tagged.quasi.expressions {
                    self.reserve_expression(source, expression, allocator)?;
                }
            }
            Expression::UnaryExpression(unary) => {
                self.reserve_expression(source, &unary.argument, allocator)?;
            }
            Expression::YieldExpression(yield_expression) => {
                if let Some(argument) = &yield_expression.argument {
                    self.reserve_expression(source, argument, allocator)?;
                }
            }
            Expression::PrivateInExpression(private_in) => {
                self.reserve_expression(source, &private_in.right, allocator)?;
            }
            Expression::TSAsExpression(assertion) => {
                self.reserve_expression(source, &assertion.expression, allocator)?;
            }
            Expression::TSSatisfiesExpression(assertion) => {
                self.reserve_expression(source, &assertion.expression, allocator)?;
            }
            Expression::TSTypeAssertion(assertion) => {
                self.reserve_expression(source, &assertion.expression, allocator)?;
            }
            Expression::TSNonNullExpression(expression) => {
                self.reserve_expression(source, &expression.expression, allocator)?;
            }
            Expression::TSInstantiationExpression(instantiation) => {
                self.reserve_expression(source, &instantiation.expression, allocator)?;
            }
            Expression::ComputedMemberExpression(member) => {
                self.reserve_expression(source, &member.object, allocator)?;
                self.reserve_expression(source, &member.expression, allocator)?;
            }
            Expression::StaticMemberExpression(member) => {
                self.reserve_expression(source, &member.object, allocator)?;
            }
            Expression::PrivateFieldExpression(member) => {
                self.reserve_expression(source, &member.object, allocator)?;
            }
            Expression::V8IntrinsicExpression(intrinsic) => {
                self.reserve_arguments(source, &intrinsic.arguments, allocator)?;
            }
            Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::Identifier(_)
            | Expression::MetaProperty(_)
            | Expression::Super(_)
            | Expression::ThisExpression(_)
            | Expression::UpdateExpression(_)
            | Expression::JSXElement(_)
            | Expression::JSXFragment(_) => {}
        }
        Ok(())
    }

    fn reserve_arguments<Allocator>(
        &mut self,
        source: SourceSite,
        arguments: &[Argument<'_>],
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        for argument in arguments {
            if let Some(expression) = argument.as_expression() {
                self.reserve_expression(source, expression, allocator)?;
            } else if let Argument::SpreadElement(spread) = argument {
                self.reserve_expression(source, &spread.argument, allocator)?;
            }
        }
        Ok(())
    }

    fn reserve_call_expression<Allocator>(
        &mut self,
        source: SourceSite,
        call: &oxc_ast::ast::CallExpression<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        self.reserve_expression(source, &call.callee, allocator)?;
        self.reserve_arguments(source, &call.arguments, allocator)
    }

    fn reserve_property_key<Allocator>(
        &mut self,
        source: SourceSite,
        key: &PropertyKey<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        if let Some(expression) = key.as_expression() {
            self.reserve_expression(source, expression, allocator)?;
        }
        Ok(())
    }

    fn reserve_parameter_expressions<Allocator>(
        &mut self,
        source: SourceSite,
        parameters: &FormalParameters<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        for parameter in &parameters.items {
            if let Some(initializer) = &parameter.initializer {
                self.reserve_initializer(source.unit, initializer, allocator);
                self.reserve_expression(source, initializer, allocator)?;
            }
        }
        Ok(())
    }

    fn reserve_initializer<Allocator>(
        &mut self,
        unit: SourceUnit,
        initializer: &Expression<'_>,
        allocator: &mut Allocator,
    ) where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let source = SourceSite {
            unit,
            source_start: initializer.span().start,
        };
        let (_, owner) = allocator.reserve_event(source.source_start);
        let index = self.initializers.len();
        self.initializers
            .push(InitializerReservation { source, owner });
        let previous = self
            .initializers_by_source
            .insert((source.unit, source.source_start), index);
        debug_assert!(previous.is_none(), "one exact initializer owner");
    }

    fn reserve_function_expression<Allocator>(
        &mut self,
        parent: SourceSite,
        function: &Function<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let source = SourceSite {
            source_start: function.span.start,
            ..parent
        };
        let (event, primary) = allocator.reserve_event(source.source_start);
        let tickets = reserve_site_tickets(event, primary, allocator)?;
        self.expression_site_tickets.push(tickets);
        #[cfg(test)]
        self.expression_sources.push(source);
        self.reserve_callable(source, event, tickets, None, function, allocator)?;
        self.reserve_parameter_expressions(source, &function.params, allocator)?;
        if let Some(body) = &function.body {
            self.reserve_statement_list(source.unit, &body.statements, false, allocator)?;
        }
        Ok(())
    }

    fn reserve_arrow_expression<Allocator>(
        &mut self,
        parent: SourceSite,
        arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let source = SourceSite {
            source_start: arrow.span.start,
            ..parent
        };
        let (event, primary) = allocator.reserve_event(source.source_start);
        let tickets = reserve_site_tickets(event, primary, allocator)?;
        self.expression_site_tickets.push(tickets);
        #[cfg(test)]
        self.expression_sources.push(source);
        self.reserve_arrow_callable(source, event, tickets, arrow, allocator)?;
        self.reserve_parameter_expressions(source, &arrow.params, allocator)?;
        if let Some(expression) = arrow.get_expression() {
            self.reserve_expression(source, expression, allocator)?;
        } else {
            self.reserve_statement_list(source.unit, &arrow.body.statements, false, allocator)?;
        }
        Ok(())
    }

    fn reserve_class_expression<Allocator>(
        &mut self,
        parent: SourceSite,
        class: &Class<'_>,
        allocator: &mut Allocator,
    ) -> Result<(), Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let source = SourceSite {
            source_start: class.span.start,
            ..parent
        };
        let (event, primary) = allocator.reserve_event(source.source_start);
        let tickets = reserve_site_tickets(event, primary, allocator)?;
        self.expression_site_tickets.push(tickets);
        #[cfg(test)]
        self.expression_sources.push(source);
        self.reserve_class(source, class, tickets, allocator)?;
        Ok(())
    }
}

impl<Ticket: Copy + PartialEq> LexicalReservations<Ticket> {
    pub(crate) fn top_level(&self) -> &[TopLevelReservation<Ticket>] {
        &self.top_level
    }

    pub(crate) fn classes(&self) -> &[ClassReservation<Ticket>] {
        &self.classes
    }

    pub(crate) fn class(&self, id: ClassSiteId) -> Option<&ClassReservation<Ticket>> {
        self.classes.get(id.0)
    }

    pub(crate) fn class_at(&self, source: SourceOrdinal, source_start: u32) -> Option<ClassSiteId> {
        self.classes_by_source
            .get(&(source, source_start))
            .and_then(|ids| ids.first())
            .copied()
    }

    pub(crate) fn member(&self, id: MemberSiteId) -> Option<&MemberReservation<Ticket>> {
        self.members.get(id.0)
    }

    pub(crate) fn callable(&self, id: CallableSiteId) -> Option<&CallableReservation<Ticket>> {
        self.callables.get(id.0)
    }

    pub(crate) fn callable_at(
        &self,
        source: SourceOrdinal,
        source_start: u32,
    ) -> Option<CallableSiteId> {
        self.callables_by_source
            .get(&(source, source_start))
            .and_then(|ids| ids.first())
            .copied()
    }

    pub(crate) fn attach_declaration_owner(
        &mut self,
        declaration: DeclId,
        source: SourceOrdinal,
        kind: DeclarationKind,
        declaration_span: Span,
        binding_span: Span,
    ) -> Result<(), ReservationStateError> {
        let index = *self
            .declarations_by_binding
            .get(&(source, binding_span.start, binding_span.end))
            .ok_or(ReservationStateError::MissingDeclarationOwner(declaration))?;
        let reservation = self
            .declarations
            .get(index)
            .ok_or(ReservationStateError::MissingDeclarationOwner(declaration))?;
        assert_eq!(
            reservation.kind, kind,
            "declaration kind must match source prewalk"
        );
        assert_eq!(
            reservation.declaration_span, declaration_span,
            "declaration node span must match source prewalk"
        );
        // Keyed by declaration, and merged fragments each get their own `DeclId`, so every
        // key is written exactly once — unlike the source-start indexes, which keep the first.
        self.declaration_reservations_by_decl
            .insert(declaration, index);
        Ok(())
    }

    pub(crate) fn export_alias_owner(
        &self,
        source: SourceOrdinal,
        local_span: Span,
    ) -> Option<LexicalOwner<Ticket>> {
        let reservation = self
            .export_aliases_by_local_span
            .get(&(source, local_span.start, local_span.end))
            .and_then(|index| self.export_aliases.get(*index))?;
        debug_assert_eq!(reservation.local_span, local_span);
        Some(LexicalOwner {
            ticket: reservation.owner,
        })
    }

    pub(crate) fn declaration_owner(&self, declaration: DeclId) -> Option<LexicalOwner<Ticket>> {
        self.declaration_reservation(declaration)
            .map(|reservation| LexicalOwner {
                ticket: reservation.owner,
            })
    }

    pub(crate) fn declaration_source(&self, declaration: DeclId) -> Option<SourceSite> {
        self.declaration_reservation(declaration)
            .map(|reservation| reservation.source)
    }

    pub(crate) fn declaration_reservation(
        &self,
        declaration: DeclId,
    ) -> Option<&DeclarationReservation<Ticket>> {
        #[cfg(test)]
        record_declaration_reservation_index_probe_for_test();
        self.declaration_reservations_by_decl
            .get(&declaration)
            .and_then(|index| self.declarations.get(*index))
    }

    pub(crate) fn interface_occurrence_owner(
        &self,
        declaration: DeclId,
        kind: InterfaceOccurrenceKind,
        source_start: u32,
    ) -> Option<Ticket> {
        let reservation = self.declaration_reservation(declaration)?;
        let index = self.interface_occurrences_by_source.get(&(
            reservation.source.ordinal(),
            reservation.binding_span.start,
            kind,
            source_start,
        ))?;
        let occurrence = self.interface_occurrences.get(*index)?;
        debug_assert_eq!(occurrence.binding_start, reservation.binding_span.start);
        debug_assert_eq!(occurrence.source.source_start, source_start);
        debug_assert_eq!(occurrence.kind, kind);
        Some(occurrence.owner)
    }

    pub(crate) fn owner_at(
        &self,
        source: SourceOrdinal,
        source_start: u32,
        phase: LexicalOwnerPhase,
    ) -> Option<LexicalOwner<Ticket>> {
        if let Some(callable) = self
            .callable_at(source, source_start)
            .and_then(|site| self.callable(site))
        {
            let ticket = match phase {
                LexicalOwnerPhase::Immediate => callable.tickets.signature,
                LexicalOwnerPhase::Deferred => callable.tickets.deferred,
                LexicalOwnerPhase::Incomplete => callable.tickets.incomplete,
                LexicalOwnerPhase::Body => callable.tickets.body,
            };
            return Some(LexicalOwner { ticket });
        }
        #[cfg(test)]
        record_lexical_owner_index_probe_for_test();
        let site = if let Some(index) = self.declarators_by_source.get(&(source, source_start)) {
            self.declarators.get(*index).map(|site| site.tickets)
        } else {
            #[cfg(test)]
            record_lexical_owner_index_probe_for_test();
            if let Some(index) = self
                .nested_statements_by_source
                .get(&(source, source_start))
            {
                self.nested_statements.get(*index).map(|site| site.tickets)
            } else {
                #[cfg(test)]
                record_lexical_owner_index_probe_for_test();
                if let Some(index) = self.members_by_source.get(&(source, source_start)) {
                    self.members.get(*index).map(|site| site.tickets)
                } else {
                    #[cfg(test)]
                    record_lexical_owner_index_probe_for_test();
                    self.top_level_by_source
                        .get(&(source, source_start))
                        .and_then(|index| self.top_level.get(*index))
                        .map(|site| site.tickets)
                }
            }
        }?;
        let ticket = match phase {
            LexicalOwnerPhase::Immediate | LexicalOwnerPhase::Body => site.immediate,
            LexicalOwnerPhase::Deferred => site.deferred,
            LexicalOwnerPhase::Incomplete => site.incomplete,
        };
        Some(LexicalOwner { ticket })
    }

    pub(crate) fn attach_class_binding(
        &mut self,
        site: ClassSiteId,
        binding: ClassBinding,
    ) -> Result<(), ReservationStateError> {
        let Some(reservation) = self.classes.get_mut(site.0) else {
            return Err(ReservationStateError::UnknownClass(site));
        };
        if reservation.binding.is_some() {
            return Err(ReservationStateError::DuplicateClassBinding(site));
        }
        reservation.binding = Some(binding);
        Ok(())
    }

    /// Allocate every callable binder during reservation, before class fill or bodies.
    pub(crate) fn reserve_callable_type_params(
        &mut self,
        next_type_param: &mut u32,
    ) -> Result<(), ReservationStateError> {
        if let Some(callable) = self
            .callables
            .iter()
            .find(|callable| callable.binding.is_some())
        {
            return Err(ReservationStateError::DuplicateCallableBinding(callable.id));
        }
        for callable in &mut self.callables {
            let mut type_params = Vec::with_capacity(callable.type_parameter_count);
            for _ in 0..callable.type_parameter_count {
                type_params.push(TypeParamId(*next_type_param));
                *next_type_param += 1;
            }
            callable.binding = Some(CallableBinding { type_params });
        }
        Ok(())
    }

    pub(crate) fn tickets(&self) -> Vec<Ticket> {
        let mut tickets = Vec::new();
        tickets.extend(
            self.declarations
                .iter()
                .map(|declaration| declaration.owner),
        );
        tickets.extend(self.export_aliases.iter().map(|alias| alias.owner));
        tickets.extend(
            self.interface_occurrences
                .iter()
                .map(|occurrence| occurrence.owner),
        );
        for site in &self.top_level {
            tickets.extend(site_tickets(site.tickets));
        }
        for site in &self.nested_statements {
            tickets.extend(site_tickets(site.tickets));
        }
        for declarator in &self.declarators {
            tickets.extend(site_tickets(declarator.tickets));
        }
        tickets.extend(
            self.initializers
                .iter()
                .map(|initializer| initializer.owner),
        );
        for class in &self.classes {
            tickets.extend(class.constraints.iter().map(|constraint| constraint.owner));
            tickets.extend(class.defaults.iter().map(|default| default.owner));
        }
        for member in &self.members {
            tickets.extend(site_tickets(member.tickets));
        }
        for site in &self.expression_site_tickets {
            tickets.extend(site_tickets(*site));
        }
        for callable in &self.callables {
            tickets.push(callable.tickets.body);
        }
        tickets
    }

    pub(crate) fn initializer_owner_at(
        &self,
        source: SourceUnit,
        source_start: u32,
    ) -> Option<Ticket> {
        self.initializers_by_source
            .get(&(source, source_start))
            .and_then(|index| self.initializers.get(*index))
            .map(|initializer| initializer.owner)
    }

    pub(crate) fn retain_source_anchor(&mut self, source: SourceSite, owner: Ticket) {
        self.source_anchors
            .push(SourceAnchorReservation { source, owner });
    }

    pub(crate) fn source_anchor_tickets(&self) -> Vec<Ticket> {
        self.source_anchors
            .iter()
            .map(|anchor| anchor.owner)
            .collect()
    }

    #[cfg(test)]
    fn structural_source_sites(&self) -> Vec<SourceSite> {
        let mut sites = Vec::new();
        sites.extend(
            self.declarations
                .iter()
                .map(|reservation| reservation.source),
        );
        sites.extend(
            self.export_aliases
                .iter()
                .map(|reservation| reservation.source),
        );
        sites.extend(
            self.interface_occurrences
                .iter()
                .map(|reservation| reservation.source),
        );
        sites.extend(self.top_level.iter().map(|reservation| reservation.source));
        sites.extend(
            self.nested_statements
                .iter()
                .map(|reservation| reservation.source),
        );
        sites.extend(
            self.declarators
                .iter()
                .map(|reservation| reservation.source),
        );
        sites.extend(
            self.initializers
                .iter()
                .map(|reservation| reservation.source),
        );
        for reservation in &self.classes {
            sites.push(reservation.source);
            sites.extend(
                reservation
                    .constraints
                    .iter()
                    .map(|constraint| constraint.source),
            );
            sites.extend(reservation.defaults.iter().map(|default| default.source));
        }
        sites.extend(self.members.iter().map(|reservation| reservation.source));
        sites.extend(self.callables.iter().map(|reservation| reservation.source));
        sites.extend(self.expression_sources.iter().copied());
        sites
    }

    #[cfg(test)]
    fn retained_source_sites(&self) -> Vec<SourceSite> {
        self.source_anchors
            .iter()
            .map(|anchor| anchor.source)
            .chain(self.structural_source_sites())
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn retained_source_units(&self) -> Vec<SourceUnit> {
        self.retained_source_sites()
            .into_iter()
            .map(|site| site.unit)
            .collect()
    }
}

impl<Ticket: Copy + Ord> LexicalReservations<Ticket> {
    pub(crate) fn class_ticket_owners(&self) -> BTreeMap<Ticket, ClassId> {
        let mut owners = BTreeMap::new();
        let insert = |owners: &mut BTreeMap<Ticket, ClassId>, ticket, class| {
            let previous = owners.insert(ticket, class);
            assert!(
                previous.is_none_or(|previous| previous == class),
                "one replay class owns each class ticket"
            );
        };
        for class in &self.classes {
            let Some(binding) = class.binding.as_ref() else {
                continue;
            };
            let class_id = binding.class_id;
            for ticket in [
                class.tickets.immediate,
                class.tickets.deferred,
                class.tickets.incomplete,
            ] {
                insert(&mut owners, ticket, class_id);
            }
            for ticket in class
                .constraints
                .iter()
                .map(|row| row.owner)
                .chain(class.defaults.iter().map(|row| row.owner))
            {
                insert(&mut owners, ticket, class_id);
            }
            for member_id in &class.members {
                let member = self
                    .member(*member_id)
                    .expect("class member reservation remains retained");
                for ticket in [
                    member.tickets.immediate,
                    member.tickets.deferred,
                    member.tickets.incomplete,
                ] {
                    insert(&mut owners, ticket, class_id);
                }
                if let Some(callable_id) = member.callable {
                    let callable = self
                        .callable(callable_id)
                        .expect("class callable reservation remains retained");
                    for ticket in [
                        callable.tickets.signature,
                        callable.tickets.deferred,
                        callable.tickets.incomplete,
                        callable.tickets.body,
                    ] {
                        insert(&mut owners, ticket, class_id);
                    }
                }
            }
        }
        owners
    }
}

impl<Ticket: Copy + PartialEq> LexicalReservations<Ticket> {
    fn reserve_class<Allocator>(
        &mut self,
        source: SourceSite,
        class: &Class<'_>,
        tickets: SiteTickets<Ticket>,
        allocator: &mut Allocator,
    ) -> Result<ClassSiteId, Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let id = ClassSiteId(self.classes.len());
        let mut constraints = Vec::new();
        let mut defaults = Vec::new();
        for (parameter_index, parameter) in class
            .type_parameters
            .as_deref()
            .into_iter()
            .flat_map(|parameters| parameters.params.iter())
            .enumerate()
        {
            if let Some(constraint) = parameter.constraint.as_ref() {
                let source = SourceSite {
                    unit: source.unit,
                    source_start: constraint.span().start,
                };
                let (_, owner) = allocator.reserve_event(source.source_start);
                constraints.push(ClassConstraintReservation {
                    parameter_index,
                    source,
                    owner,
                });
            }
            if let Some(default) = parameter.default.as_ref() {
                let source = SourceSite {
                    unit: source.unit,
                    source_start: default.span().start,
                };
                let (_, owner) = allocator.reserve_event(source.source_start);
                defaults.push(ClassDefaultReservation {
                    parameter_index,
                    source,
                    owner,
                });
            }
        }
        self.classes.push(ClassReservation {
            id,
            source,
            tickets,
            constraints,
            defaults,
            members: Vec::new(),
            binding: None,
        });
        self.classes_by_source
            .entry((source.ordinal(), class.span.start))
            .or_default()
            .push(id);

        if let Some(super_class) = &class.super_class {
            self.reserve_expression(source, super_class, allocator)?;
        }

        for element in &class.body.body {
            let element_source = SourceSite {
                unit: source.unit,
                source_start: element.span().start,
            };
            let (member_event, member_primary) =
                allocator.reserve_event(element_source.source_start);
            let member_tickets = reserve_site_tickets(member_event, member_primary, allocator)?;
            let member_id = MemberSiteId(self.members.len());
            let mut member = MemberReservation {
                id: member_id,
                class: id,
                source: element_source,
                tickets: member_tickets,
                callable: None,
            };
            if let ClassElement::MethodDefinition(method) = element {
                member.callable = Some(self.reserve_callable(
                    element_source,
                    member_event,
                    member_tickets,
                    Some(member_id),
                    &method.value,
                    allocator,
                )?);
                self.reserve_property_key(element_source, &method.key, allocator)?;
                self.reserve_parameter_expressions(
                    element_source,
                    &method.value.params,
                    allocator,
                )?;
            } else if let ClassElement::PropertyDefinition(property) = element {
                self.reserve_property_key(element_source, &property.key, allocator)?;
                if let Some(value) = &property.value {
                    self.reserve_initializer(element_source.unit, value, allocator);
                    self.reserve_expression(element_source, value, allocator)?;
                }
            } else if let ClassElement::AccessorProperty(property) = element {
                self.reserve_property_key(element_source, &property.key, allocator)?;
                if let Some(value) = &property.value {
                    self.reserve_initializer(element_source.unit, value, allocator);
                    self.reserve_expression(element_source, value, allocator)?;
                }
            }
            let member_index = self.members.len();
            self.members.push(member);
            self.members_by_source
                .entry((element_source.ordinal(), element_source.source_start))
                .or_insert(member_index);
            self.classes[id.0].members.push(member_id);
            match element {
                ClassElement::MethodDefinition(method) => {
                    if let Some(body) = method.value.body.as_ref() {
                        self.reserve_statement_list(
                            source.unit,
                            &body.statements,
                            false,
                            allocator,
                        )?;
                    }
                }
                ClassElement::StaticBlock(block) => {
                    self.reserve_statement_list(source.unit, &block.body, false, allocator)?;
                }
                ClassElement::PropertyDefinition(_)
                | ClassElement::AccessorProperty(_)
                | ClassElement::TSIndexSignature(_) => {}
            }
        }
        Ok(id)
    }

    fn reserve_callable<Allocator>(
        &mut self,
        source: SourceSite,
        event: Allocator::Event,
        site_tickets: SiteTickets<Ticket>,
        owner_member: Option<MemberSiteId>,
        function: &Function<'_>,
        allocator: &mut Allocator,
    ) -> Result<CallableSiteId, Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let id = CallableSiteId(self.callables.len());
        let body = allocator.reserve_record(event)?;
        self.callables.push(CallableReservation {
            id,
            owner_member,
            source,
            tickets: CallableTickets {
                signature: site_tickets.immediate,
                deferred: site_tickets.deferred,
                incomplete: site_tickets.incomplete,
                body,
            },
            type_parameter_count: function
                .type_parameters
                .as_ref()
                .map_or(0, |parameters| parameters.params.len()),
            binding: None,
        });
        self.callables_by_source
            .entry((source.ordinal(), function.span.start))
            .or_default()
            .push(id);
        Ok(id)
    }

    fn reserve_arrow_callable<Allocator>(
        &mut self,
        source: SourceSite,
        event: Allocator::Event,
        site_tickets: SiteTickets<Ticket>,
        arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
        allocator: &mut Allocator,
    ) -> Result<CallableSiteId, Allocator::Error>
    where
        Allocator: LexicalReservationAllocator<Ticket = Ticket>,
    {
        let id = CallableSiteId(self.callables.len());
        let body = allocator.reserve_record(event)?;
        self.callables.push(CallableReservation {
            id,
            owner_member: None,
            source,
            tickets: CallableTickets {
                signature: site_tickets.immediate,
                deferred: site_tickets.deferred,
                incomplete: site_tickets.incomplete,
                body,
            },
            type_parameter_count: arrow
                .type_parameters
                .as_ref()
                .map_or(0, |parameters| parameters.params.len()),
            binding: None,
        });
        self.callables_by_source
            .entry((source.ordinal(), arrow.span.start))
            .or_default()
            .push(id);
        Ok(id)
    }
}

fn reserve_site_tickets<Allocator>(
    event: Allocator::Event,
    primary: Allocator::Ticket,
    allocator: &mut Allocator,
) -> Result<SiteTickets<Allocator::Ticket>, Allocator::Error>
where
    Allocator: LexicalReservationAllocator,
{
    Ok(SiteTickets {
        immediate: primary,
        deferred: allocator.reserve_record(event)?,
        incomplete: allocator.reserve_record(event)?,
    })
}

fn site_tickets<Ticket: Copy>(tickets: SiteTickets<Ticket>) -> [Ticket; 3] {
    [tickets.immediate, tickets.deferred, tickets.incomplete]
}

fn statement_class<'ast>(statement: &'ast Statement<'ast>) -> Option<&'ast Class<'ast>> {
    match statement {
        Statement::ClassDeclaration(class) => Some(class),
        Statement::ExportNamedDeclaration(export) => match &export.declaration {
            Some(Declaration::ClassDeclaration(class)) => Some(class.as_ref()),
            _ => None,
        },
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::ClassDeclaration(class) => Some(class),
            _ => None,
        },
        _ => None,
    }
}

fn statement_function<'ast>(statement: &'ast Statement<'ast>) -> Option<&'ast Function<'ast>> {
    match statement {
        Statement::FunctionDeclaration(function) => Some(function),
        Statement::ExportNamedDeclaration(export) => match &export.declaration {
            Some(Declaration::FunctionDeclaration(function)) => Some(function.as_ref()),
            _ => None,
        },
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => Some(function),
            _ => None,
        },
        _ => None,
    }
}

fn statement_variable_declaration<'ast>(
    statement: &'ast Statement<'ast>,
) -> Option<&'ast VariableDeclaration<'ast>> {
    match statement {
        Statement::VariableDeclaration(declaration) => Some(declaration),
        Statement::ExportNamedDeclaration(export) => match &export.declaration {
            Some(Declaration::VariableDeclaration(declaration)) => Some(declaration),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
#[path = "lexical_events/completion_slot_spec.rs"]
mod completion_slot_spec;

#[cfg(test)]
#[path = "lexical_events/owner_lookup_spec.rs"]
mod owner_lookup_spec;

#[cfg(test)]
mod tests {
    use super::super::context::CheckerEffects;
    use super::super::events::{EventStore, EventStoreError};
    use super::super::events_library::{
        LibraryEventKey, LibraryEventLedger, LibraryEventLedgerError, LibraryRecordTicket,
        LibrarySemanticReportingAdapter,
    };
    use super::super::lexical_events_user::ReservationError as UserReservationError;
    use super::super::reporting_record::CheckerRecord;
    use super::*;
    use crate::diagnostics::IncompleteSurface;
    use crate::source::LibraryFileOrdinal;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use std::collections::BTreeSet;

    fn source_span(source: &str, needle: &str) -> Span {
        let start = source.find(needle).expect("test source contains needle");
        Span::new(
            u32::try_from(start).expect("source offset fits u32"),
            u32::try_from(start + needle.len()).expect("source offset fits u32"),
        )
    }

    fn test_record(index: usize) -> CheckerRecord {
        let start = u32::try_from(index).expect("test record index fits u32");
        CheckerRecord::Incomplete(IncompleteSurface::new(
            format!("lexical-reservation-test-{index}"),
            Span::new(start, start.saturating_add(1)),
            "lexical reservation adapter witness",
        ))
    }

    fn reservation_sources<Ticket: Copy + PartialEq>(
        reservations: &LexicalReservations<Ticket>,
    ) -> Vec<SourceSite> {
        reservations.structural_source_sites()
    }

    fn finish_user_reservations(
        mut store: EventStore,
        tickets: &[UserRecordTicket],
    ) -> Vec<(u32, usize, usize, String, Span)> {
        for (index, ticket) in tickets.iter().enumerate() {
            store
                .complete(*ticket, vec![test_record(index)])
                .expect("each user lexical ticket completes once");
        }
        store
            .finish()
            .expect("complete user lexical inventory finishes")
            .into_iter()
            .map(|(key, record)| {
                let CheckerRecord::Incomplete(record) = record else {
                    panic!("lexical parity emits only incompletes");
                };
                (
                    key.source_start,
                    key.event_ordinal,
                    key.record_ordinal,
                    record.id,
                    record.span,
                )
            })
            .collect()
    }

    fn finish_library_reservations(
        mut ledger: LibraryEventLedger,
        tickets: &[LibraryRecordTicket],
        anchors: &[LibraryRecordTicket],
    ) -> Vec<(u32, usize, usize, String, Span)> {
        let anchor_batches = anchors
            .iter()
            .map(|anchor| CheckerEffects::new(*anchor).records)
            .collect();
        LibrarySemanticReportingAdapter::new(&mut ledger)
            .complete_semantic_batches(anchor_batches)
            .expect("each library source anchor completes once");
        for (index, ticket) in tickets.iter().enumerate().rev() {
            ledger
                .complete(*ticket, vec![test_record(index)])
                .expect("each library lexical ticket completes once");
        }
        ledger
            .finish()
            .expect("complete library lexical inventory finishes")
            .into_iter()
            .map(|(key, record)| {
                let CheckerRecord::Incomplete(record) = record else {
                    panic!("lexical parity emits only incompletes");
                };
                (
                    key.source_start,
                    key.event_ordinal,
                    key.record_ordinal,
                    record.id,
                    record.span,
                )
            })
            .collect()
    }

    #[test]
    fn user_and_library_wrappers_expose_concrete_authority_errors() {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, "const value = 1;", SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());

        let mut user = LexicalReservations::<UserRecordTicket>::default();
        let mut store = EventStore::default();
        let user_result: Result<(), UserReservationError> = user.reserve_program(
            ModuleOrdinal::new(3),
            UnitSlot::new(2),
            &parsed.program,
            &mut store,
        );
        user_result.expect("user wrapper reserves through EventStore");

        let mut library = LexicalReservations::<LibraryRecordTicket>::default();
        let mut ledger = LibraryEventLedger::default();
        let library_result: Result<(), LibraryEventLedgerError> = library.reserve_library_program(
            LibraryFileOrdinal::new(5),
            &parsed.program,
            &mut ledger,
        );
        library_result.expect("library wrapper reserves through LibraryEventLedger");
    }

    #[test]
    fn rich_ast_has_identical_user_and_library_reservation_streams() {
        let source = r#"
interface Shape<T> extends Base {
  value: T;
  call(input: number): string;
  new (input: string): Shape<T>;
  [key: string]: unknown;
}
declare namespace Outer {
  export { Missing as Alias };
  export interface Nested { nested: boolean; }
}
class Box<T extends object = {}> extends Base {
  static { const staticArrow = (value = 1) => value; }
  [computed()] = function (value = 1) { return value; };
  method<U>(value = () => 1) { return value(); }
}
function outer<T>(value = () => 1) {
  if (value) { const nested = class { field = () => 1; }; }
  return function inner(input = 1) { return input; };
}
const arrow = (input = 1) => class Inner { method() { return input; } };
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let user_unit = SourceUnit::User {
            module_ordinal: ModuleOrdinal::new(11),
            unit_slot: UnitSlot::new(4),
        };
        let library_unit = SourceUnit::Library {
            file_ordinal: LibraryFileOrdinal::new(11),
        };
        let mut user = LexicalReservations::<UserRecordTicket>::default();
        let mut user_store = EventStore::default();
        user.reserve_program(
            ModuleOrdinal::new(11),
            UnitSlot::new(4),
            &parsed.program,
            &mut user_store,
        )
        .unwrap();
        let mut library = LexicalReservations::<LibraryRecordTicket>::default();
        let mut library_ledger = LibraryEventLedger::default();
        library
            .reserve_library_program(
                LibraryFileOrdinal::new(11),
                &parsed.program,
                &mut library_ledger,
            )
            .unwrap();

        let user_sources = reservation_sources(&user);
        let library_sources = reservation_sources(&library);
        let retained_row_count = user.declarations.len()
            + user.export_aliases.len()
            + user.interface_occurrences.len()
            + user.top_level.len()
            + user.nested_statements.len()
            + user.declarators.len()
            + user.initializers.len()
            + user.classes.len()
            + user
                .classes
                .iter()
                .map(|class| class.constraints.len() + class.defaults.len())
                .sum::<usize>()
            + user.members.len()
            + user.callables.len()
            + user.expression_sources.len();
        assert!(!user_sources.is_empty());
        assert!(!user.expression_sources.is_empty());
        assert_eq!(user_sources.len(), retained_row_count);
        assert!(user_sources.iter().all(|site| site.unit == user_unit));
        assert!(library_sources.iter().all(|site| site.unit == library_unit));
        assert_eq!(
            user_sources
                .iter()
                .map(|site| site.source_start)
                .collect::<Vec<_>>(),
            library_sources
                .iter()
                .map(|site| site.source_start)
                .collect::<Vec<_>>()
        );

        let user_tickets = user.tickets();
        let library_tickets = library.tickets();
        let library_anchors = library.source_anchor_tickets();
        assert_eq!(library_anchors.len(), 1);
        assert_eq!(user_tickets.len(), library_tickets.len());
        assert_eq!(
            user_tickets.iter().copied().collect::<BTreeSet<_>>().len(),
            user_tickets.len()
        );
        assert_eq!(
            library_tickets
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            library_tickets.len()
        );
        assert_eq!(
            finish_user_reservations(user_store, &user_tickets),
            finish_library_reservations(library_ledger, &library_tickets, &library_anchors)
        );
    }

    #[test]
    fn root_initializers_have_exact_user_and_library_owners_with_parity() {
        let source = r#"
const variable: number = "variable";
function functionDefault(parameter: number = "parameter") {}
class Container {
  property: number = "property";
  accessor accessorProperty: number = "accessor";
}
"#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let module = ModuleOrdinal::new(4);
        let slot = UnitSlot::new(2);
        let file = LibraryFileOrdinal::new(4);
        let user_unit = SourceUnit::User {
            module_ordinal: module,
            unit_slot: slot,
        };
        let library_unit = SourceUnit::Library { file_ordinal: file };
        let mut user = LexicalReservations::<UserRecordTicket>::default();
        let mut user_store = EventStore::default();
        user.reserve_program(module, slot, &parsed.program, &mut user_store)
            .unwrap();
        let mut library = LexicalReservations::<LibraryRecordTicket>::default();
        let mut library_ledger = LibraryEventLedger::default();
        library
            .reserve_library_program(file, &parsed.program, &mut library_ledger)
            .unwrap();

        let starts = [
            "\"variable\"",
            "\"parameter\"",
            "\"property\"",
            "\"accessor\"",
        ]
        .map(|needle| source_span(source, needle).start);
        assert_eq!(user.initializers.len(), starts.len());
        assert_eq!(library.initializers.len(), starts.len());
        let user_owners = starts.map(|start| {
            user.initializer_owner_at(user_unit, start)
                .expect("user initializer owns its exact root expression")
        });
        let library_owners = starts.map(|start| {
            library
                .initializer_owner_at(library_unit, start)
                .expect("library initializer owns its exact root expression")
        });
        assert_eq!(
            user_owners.into_iter().collect::<BTreeSet<_>>().len(),
            starts.len()
        );
        assert_eq!(
            library_owners.into_iter().collect::<BTreeSet<_>>().len(),
            starts.len()
        );

        let user_tickets = user.tickets();
        let library_tickets = library.tickets();
        assert_eq!(user_tickets.len(), library_tickets.len());
        assert_eq!(
            finish_user_reservations(user_store, &user_tickets),
            finish_library_reservations(
                library_ledger,
                &library_tickets,
                &library.source_anchor_tickets(),
            )
        );
    }

    #[test]
    fn identical_spans_in_two_library_files_keep_exact_sources_and_keys() {
        let source = "class Shared { method(value = 1) { return () => value; } }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let first = LibraryFileOrdinal::new(2);
        let second = LibraryFileOrdinal::new(7);
        let mut reservations = LexicalReservations::<LibraryRecordTicket>::default();
        let mut ledger = LibraryEventLedger::default();
        reservations
            .reserve_library_program(first, &parsed.program, &mut ledger)
            .unwrap();
        reservations
            .reserve_library_program(second, &parsed.program, &mut ledger)
            .unwrap();

        let sources = reservation_sources(&reservations);
        let first_unit = SourceUnit::Library {
            file_ordinal: first,
        };
        let second_unit = SourceUnit::Library {
            file_ordinal: second,
        };
        let first_sources = sources
            .iter()
            .filter(|site| site.unit == first_unit)
            .map(|site| site.source_start)
            .collect::<Vec<_>>();
        let second_sources = sources
            .iter()
            .filter(|site| site.unit == second_unit)
            .map(|site| site.source_start)
            .collect::<Vec<_>>();
        assert!(!first_sources.is_empty());
        assert_eq!(first_sources, second_sources);
        assert_eq!(first_sources.len() + second_sources.len(), sources.len());

        let class_start = reservations
            .classes
            .iter()
            .find(|class| class.source.unit == first_unit)
            .map(|class| class.source.source_start)
            .expect("first library class source");
        let first_class = reservations
            .class_at(SourceOrdinal::Library(first), class_start)
            .expect("first library class site");
        let second_class = reservations
            .class_at(SourceOrdinal::Library(second), class_start)
            .expect("second library class site");
        assert_ne!(first_class, second_class);
        let callable_start = reservations
            .callables_by_source
            .keys()
            .find(|(source, _)| *source == SourceOrdinal::Library(first))
            .map(|(_, source_start)| *source_start)
            .expect("first library callable source");
        let first_callable = reservations
            .callable_at(SourceOrdinal::Library(first), callable_start)
            .expect("first library callable site");
        let second_callable = reservations
            .callable_at(SourceOrdinal::Library(second), callable_start)
            .expect("second library callable site");
        assert_ne!(first_callable, second_callable);

        let anchor_batches = reservations
            .source_anchor_tickets()
            .into_iter()
            .map(|anchor| CheckerEffects::new(anchor).records)
            .collect();
        LibrarySemanticReportingAdapter::new(&mut ledger)
            .complete_semantic_batches(anchor_batches)
            .unwrap();
        let tickets = reservations.tickets();
        for (index, ticket) in tickets.iter().enumerate() {
            ledger.complete(*ticket, vec![test_record(index)]).unwrap();
        }
        let keys = ledger
            .finish()
            .expect("both library files have complete inventories")
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        let first_keys = keys
            .iter()
            .filter(|key| key.file_ordinal == first)
            .map(|key| (key.source_start, key.event_ordinal, key.record_ordinal))
            .collect::<Vec<_>>();
        let second_keys = keys
            .iter()
            .filter(|key| key.file_ordinal == second)
            .map(|key| (key.source_start, key.event_ordinal, key.record_ordinal))
            .collect::<Vec<_>>();
        assert!(!first_keys.is_empty());
        assert_eq!(first_keys, second_keys);
        assert_eq!(
            keys.iter().copied().collect::<BTreeSet<_>>().len(),
            keys.len()
        );
    }

    #[test]
    fn library_inventory_is_unique_complete_once_and_reports_exact_failures() {
        let source = "const value = () => class { method() { return 1; } };";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let file = LibraryFileOrdinal::new(13);
        let mut reservations = LexicalReservations::<LibraryRecordTicket>::default();
        let mut ledger = LibraryEventLedger::default();
        reservations
            .reserve_library_program(file, &parsed.program, &mut ledger)
            .unwrap();
        let tickets = reservations.tickets();
        let anchors = reservations.source_anchor_tickets();
        let inventory = anchors.iter().chain(&tickets).copied().collect::<Vec<_>>();
        assert!(!tickets.is_empty());
        assert_eq!(
            inventory.iter().copied().collect::<BTreeSet<_>>().len(),
            inventory.len()
        );
        let duplicate = tickets[0];
        ledger.complete(duplicate, Vec::new()).unwrap();
        assert_eq!(
            ledger.complete(duplicate, Vec::new()),
            Err(LibraryEventLedgerError::DuplicateCompletion(duplicate))
        );
        for ticket in tickets
            .iter()
            .copied()
            .filter(|ticket| *ticket != duplicate)
        {
            ledger.complete(ticket, Vec::new()).unwrap();
        }
        let anchor_batches = anchors
            .into_iter()
            .map(|anchor| CheckerEffects::new(anchor).records)
            .collect();
        LibrarySemanticReportingAdapter::new(&mut ledger)
            .complete_semantic_batches(anchor_batches)
            .unwrap();
        assert!(ledger.finish().unwrap().is_empty());

        let short_source = "const x = 1;";
        let short_allocator = Allocator::default();
        let short = Parser::new(&short_allocator, short_source, SourceType::ts()).parse();
        assert!(short.diagnostics.is_empty());
        let unfinished_file = LibraryFileOrdinal::new(21);
        let mut unfinished_reservations = LexicalReservations::<LibraryRecordTicket>::default();
        let mut unfinished_ledger = LibraryEventLedger::default();
        unfinished_reservations
            .reserve_library_program(unfinished_file, &short.program, &mut unfinished_ledger)
            .unwrap();
        let unfinished_tickets = unfinished_reservations.tickets();
        let anchor_batches = unfinished_reservations
            .source_anchor_tickets()
            .into_iter()
            .map(|anchor| CheckerEffects::new(anchor).records)
            .collect();
        LibrarySemanticReportingAdapter::new(&mut unfinished_ledger)
            .complete_semantic_batches(anchor_batches)
            .unwrap();
        let missing = unfinished_tickets[0];
        for ticket in unfinished_tickets
            .iter()
            .copied()
            .filter(|ticket| *ticket != missing)
        {
            unfinished_ledger.complete(ticket, Vec::new()).unwrap();
        }
        let expected_unfinished = vec![LibraryEventKey {
            file_ordinal: unfinished_file,
            source_start: u32::try_from(short_source.find('x').unwrap()).unwrap(),
            event_ordinal: 0,
            record_ordinal: 0,
        }];
        assert!(matches!(
            unfinished_ledger.finish(),
            Err(LibraryEventLedgerError::Unfinished(keys)) if keys == expected_unfinished
        ));
    }

    #[test]
    fn user_source_units_preserve_lookup_and_source_order() {
        let source = "class C {} function f() {}";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let first = ModuleOrdinal::new(0);
        let second = ModuleOrdinal::new(1);
        let class_start = u32::try_from(source.find("class C").unwrap()).unwrap();
        let callable_start = u32::try_from(source.find("function f").unwrap()).unwrap();
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();

        reservations
            .reserve_program(second, UnitSlot::new(0), &parsed.program, &mut store)
            .unwrap();
        reservations
            .reserve_program(first, UnitSlot::new(1), &parsed.program, &mut store)
            .unwrap();

        let first_class = reservations
            .class_at(SourceOrdinal::User(first), class_start)
            .unwrap();
        let second_class = reservations
            .class_at(SourceOrdinal::User(second), class_start)
            .unwrap();
        assert_ne!(first_class, second_class);
        assert!(reservations
            .callable_at(SourceOrdinal::User(first), callable_start)
            .is_some());
        assert!(reservations
            .callable_at(SourceOrdinal::User(second), callable_start)
            .is_some());
        assert_eq!(
            reservations.class(first_class).unwrap().source.unit,
            SourceUnit::User {
                module_ordinal: first,
                unit_slot: UnitSlot::new(1),
            }
        );
        assert_eq!(
            reservations
                .classes_by_source
                .keys()
                .filter(|(_, source_start)| *source_start == class_start)
                .copied()
                .collect::<Vec<_>>(),
            vec![
                (SourceOrdinal::User(first), class_start),
                (SourceOrdinal::User(second), class_start),
            ]
        );

        for ticket in reservations.tickets() {
            store.complete(ticket, Vec::new()).unwrap();
        }
        assert!(store.finish().unwrap().is_empty());
    }

    #[test]
    fn identical_user_and_library_spans_keep_distinct_lookup_domains() {
        use crate::source::LibraryFileOrdinal;

        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        enum LookupTicket {
            User,
            Library,
        }

        fn site_tickets(ticket: LookupTicket) -> SiteTickets<LookupTicket> {
            SiteTickets {
                immediate: ticket,
                deferred: ticket,
                incomplete: ticket,
            }
        }

        fn callable_tickets(ticket: LookupTicket) -> CallableTickets<LookupTicket> {
            CallableTickets {
                signature: ticket,
                deferred: ticket,
                incomplete: ticket,
                body: ticket,
            }
        }

        let source_start = 31;
        let user_ordinal = ModuleOrdinal::new(4);
        let library_ordinal = LibraryFileOrdinal::new(4);
        let user_source = SourceSite {
            unit: SourceUnit::User {
                module_ordinal: user_ordinal,
                unit_slot: UnitSlot::new(2),
            },
            source_start,
        };
        let library_source = SourceSite {
            unit: SourceUnit::Library {
                file_ordinal: library_ordinal,
            },
            source_start,
        };
        let mut reservations = LexicalReservations::<LookupTicket>::default();
        let declaration_span = Span::new(source_start, source_start + 8);
        let binding_span = Span::new(source_start + 2, source_start + 5);
        let local_span = Span::new(source_start + 6, source_start + 8);
        let mut source_declarations = Vec::new();

        for (source, ticket) in [
            (user_source, LookupTicket::User),
            (library_source, LookupTicket::Library),
        ] {
            reservations.top_level.push(TopLevelReservation {
                source,
                tickets: site_tickets(ticket),
                class: None,
                callable: None,
            });

            let class = ClassSiteId(reservations.classes.len());
            reservations.classes.push(ClassReservation {
                id: class,
                source,
                tickets: site_tickets(ticket),
                constraints: Vec::new(),
                defaults: Vec::new(),
                members: Vec::new(),
                binding: None,
            });
            reservations
                .classes_by_source
                .entry((source.ordinal(), source_start))
                .or_default()
                .push(class);

            let callable = CallableSiteId(reservations.callables.len());
            reservations.callables.push(CallableReservation {
                id: callable,
                owner_member: None,
                source,
                tickets: callable_tickets(ticket),
                type_parameter_count: 0,
                binding: None,
            });
            reservations
                .callables_by_source
                .entry((source.ordinal(), source_start))
                .or_default()
                .push(callable);

            let declaration = DeclId(
                u32::try_from(reservations.declarations.len())
                    .expect("test declaration count fits u32"),
            );
            let declaration_index = reservations.declarations.len();
            reservations.declarations.push(DeclarationReservation {
                source,
                kind: DeclarationKind::Interface,
                declaration_span,
                binding_span,
                owner: ticket,
            });
            reservations.declarations_by_binding.insert(
                (source.ordinal(), binding_span.start, binding_span.end),
                declaration_index,
            );
            reservations
                .attach_declaration_owner(
                    declaration,
                    source.ordinal(),
                    DeclarationKind::Interface,
                    declaration_span,
                    binding_span,
                )
                .unwrap();

            let export_index = reservations.export_aliases.len();
            reservations.export_aliases.push(ExportAliasReservation {
                source,
                local_span,
                owner: ticket,
            });
            reservations.export_aliases_by_local_span.insert(
                (source.ordinal(), local_span.start, local_span.end),
                export_index,
            );

            let occurrence_index = reservations.interface_occurrences.len();
            reservations
                .interface_occurrences
                .push(InterfaceOccurrenceReservation {
                    source,
                    binding_start: binding_span.start,
                    kind: InterfaceOccurrenceKind::Header,
                    owner: ticket,
                });
            reservations.interface_occurrences_by_source.insert(
                (
                    source.ordinal(),
                    binding_span.start,
                    InterfaceOccurrenceKind::Header,
                    source_start,
                ),
                occurrence_index,
            );
            source_declarations.push((source, declaration, ticket));
        }

        let user = SourceOrdinal::User(user_ordinal);
        let library = SourceOrdinal::Library(library_ordinal);
        let user_class = reservations.class_at(user, source_start).unwrap();
        let library_class = reservations.class_at(library, source_start).unwrap();
        let user_callable = reservations.callable_at(user, source_start).unwrap();
        let library_callable = reservations.callable_at(library, source_start).unwrap();

        assert_ne!(user_class, library_class);
        assert_ne!(user_callable, library_callable);
        assert_eq!(reservations.class(user_class).unwrap().source, user_source);
        assert_eq!(
            reservations.class(library_class).unwrap().source,
            library_source
        );
        assert_eq!(
            reservations.callable(user_callable).unwrap().source,
            user_source
        );
        assert_eq!(
            reservations.callable(library_callable).unwrap().source,
            library_source
        );
        for (source, declaration, ticket) in source_declarations {
            assert_eq!(reservations.declaration_source(declaration), Some(source));
            assert_eq!(
                reservations
                    .export_alias_owner(source.ordinal(), local_span)
                    .map(|owner| owner.ticket),
                Some(ticket)
            );
            assert_eq!(
                reservations.interface_occurrence_owner(
                    declaration,
                    InterfaceOccurrenceKind::Header,
                    source_start,
                ),
                Some(ticket)
            );
            assert_eq!(
                reservations
                    .owner_at(source.ordinal(), source_start, LexicalOwnerPhase::Immediate)
                    .map(|owner| owner.ticket),
                Some(ticket)
            );
        }
    }

    #[test]
    fn library_source_units_are_nominal_without_user_event_storage() {
        use super::super::events_library::{LibraryEventLedger, LibraryRecordTicket};
        use crate::source::LibraryFileOrdinal;

        let file_ordinal = LibraryFileOrdinal::new(12);
        let site = SourceSite {
            unit: SourceUnit::Library { file_ordinal },
            source_start: 38,
        };
        let _: LexicalReservations<LibraryRecordTicket> = LexicalReservations::default();
        let mut ledger = LibraryEventLedger::default();
        let event = ledger.reserve_event(file_ordinal, site.source_start);
        let tickets = SiteTickets {
            immediate: event.primary,
            deferred: ledger.reserve_record(event.id).unwrap(),
            incomplete: ledger.reserve_record(event.id).unwrap(),
        };
        let reservation = TopLevelReservation::<LibraryRecordTicket> {
            source: site,
            tickets,
            class: None,
            callable: None,
        };

        assert_eq!(site.ordinal(), SourceOrdinal::Library(file_ordinal));
        assert_ne!(
            SourceOrdinal::User(ModuleOrdinal::new(12)),
            SourceOrdinal::Library(file_ordinal)
        );
        assert_eq!(reservation.source, site);
        assert_eq!(reservation.tickets.immediate, event.primary);
        assert_eq!(reservation.tickets.deferred.record_ordinal, 1);
        assert_eq!(reservation.tickets.incomplete.record_ordinal, 2);
    }

    #[test]
    fn namespace_export_aliases_keep_exact_preallocated_local_owners() {
        let source = "\
export { TopLevel };
declare namespace N {
  interface Resolved {}
  export { MissingOne as First, Resolved as PublicResolved };
  export { PublicResolved as Chained };
  export namespace Child { export { MissingTwo as Second }; }
}
";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(module, UnitSlot::new(0), &parsed.program, &mut store)
            .unwrap();

        assert!(reservations
            .export_alias_owner(SourceOrdinal::User(module), source_span(source, "TopLevel"))
            .is_none());
        let first = reservations
            .export_alias_owner(
                SourceOrdinal::User(module),
                source_span(source, "MissingOne"),
            )
            .expect("first local alias owner");
        let resolved_start = source
            .find("Resolved as PublicResolved")
            .expect("resolved local export alias");
        let resolved_span = Span::new(
            u32::try_from(resolved_start).expect("source offset fits u32"),
            u32::try_from(resolved_start + "Resolved".len()).expect("source offset fits u32"),
        );
        let resolved = reservations
            .export_alias_owner(SourceOrdinal::User(module), resolved_span)
            .expect("resolved local alias owner");
        let chained_start = source
            .find("PublicResolved as Chained")
            .expect("alias-output local export alias");
        let chained_span = Span::new(
            u32::try_from(chained_start).expect("source offset fits u32"),
            u32::try_from(chained_start + "PublicResolved".len()).expect("source offset fits u32"),
        );
        let chained = reservations
            .export_alias_owner(SourceOrdinal::User(module), chained_span)
            .expect("alias-output local owner");
        let second = reservations
            .export_alias_owner(
                SourceOrdinal::User(module),
                source_span(source, "MissingTwo"),
            )
            .expect("nested local alias owner");
        assert_eq!(reservations.export_aliases.len(), 4);
        assert_eq!(first.ticket.record_ordinal, 0);
        assert_eq!(resolved.ticket.record_ordinal, 0);
        assert_eq!(chained.ticket.record_ordinal, 0);
        assert_eq!(second.ticket.record_ordinal, 0);
        assert_ne!(first.ticket, resolved.ticket);
        assert_ne!(resolved.ticket, chained.ticket);
        assert_ne!(chained.ticket, second.ticket);
    }

    #[test]
    fn class_and_non_class_sites_keep_lexical_interleaving() {
        let source = "const before = 1; class C { a = 1; m<T>() {} } const after = 2;";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
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

        assert_eq!(reservations.top_level().len(), 3);
        assert!(reservations.top_level()[0].class.is_none());
        let class_id = reservations.top_level()[1].class.unwrap();
        assert!(reservations.top_level()[2].class.is_none());
        assert_eq!(reservations.classes().len(), 1);
        assert_eq!(reservations.class(class_id).unwrap().members.len(), 2);

        for ticket in reservations.tickets() {
            store.complete(ticket, Vec::new()).unwrap();
        }
        let replay = store.finish().unwrap();
        assert!(replay.is_empty());
    }

    #[test]
    fn every_interface_header_heritage_and_member_has_one_exact_ticket() {
        let source = "\
interface Combined extends First, Second {
  value: boolean;
  [first: string]: number;
  [second: string]: string;
  [first: number]: 1;
  [second: number]: 2;
}
";
        let prelude_allocator = Allocator::default();
        let allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let binder = crate::binder::bind_module_with_prelude(&prelude.program, &parsed.program);
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
        super::super::attach_type_decl_owners(
            &mut reservations,
            SourceOrdinal::User(ModuleOrdinal::new(0)),
            &binder,
            binder.module,
            &parsed.program,
            &super::super::ModuleDeclarationSpans::index(&binder),
        );

        assert_eq!(
            reservations
                .interface_occurrences
                .iter()
                .filter(|occurrence| occurrence.kind == InterfaceOccurrenceKind::Header)
                .count(),
            1
        );
        assert_eq!(
            reservations
                .interface_occurrences
                .iter()
                .filter(|occurrence| occurrence.kind == InterfaceOccurrenceKind::Heritage)
                .count(),
            2
        );
        assert_eq!(
            reservations
                .interface_occurrences
                .iter()
                .filter(|occurrence| occurrence.kind == InterfaceOccurrenceKind::Member)
                .count(),
            5
        );
        let occurrence_owners: BTreeSet<_> = reservations
            .interface_occurrences
            .iter()
            .map(|occurrence| occurrence.owner)
            .collect();
        assert_eq!(occurrence_owners.len(), 8);
        let declaration = binder
            .type_groups
            .iter()
            .find(|group| group.name == "Combined")
            .and_then(|group| group.fragments.first())
            .map(|fragment| fragment.declaration)
            .expect("combined interface declaration");
        let declaration_owner = reservations
            .declaration_owner(declaration)
            .expect("declaration fallback owner");
        assert!(!occurrence_owners.contains(&declaration_owner.ticket));
        for occurrence in &reservations.interface_occurrences {
            assert_eq!(
                reservations.interface_occurrence_owner(
                    declaration,
                    occurrence.kind,
                    occurrence.source.source_start,
                ),
                Some(occurrence.owner),
                "each interface child resolves only through its exact occurrence key"
            );
        }
        let all_tickets = reservations.tickets();
        for owner in occurrence_owners {
            assert_eq!(
                all_tickets
                    .iter()
                    .filter(|ticket| **ticket == owner)
                    .count(),
                1,
                "each exact interface occurrence participates once in pending effects"
            );
        }

        for ticket in all_tickets {
            store.complete(ticket, Vec::new()).unwrap();
        }
        assert!(store.finish().unwrap().is_empty());
    }

    #[test]
    fn every_class_header_child_has_a_distinct_source_owner() {
        let source = "class C<T extends object = string, U extends unknown = number> {}";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
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

        let class = &reservations.classes()[0];
        let constraints = &class.constraints;
        assert_eq!(constraints.len(), 2);
        assert_eq!(constraints[0].parameter_index, 0);
        assert_eq!(constraints[1].parameter_index, 1);
        assert_eq!(
            constraints[0].source.source_start,
            u32::try_from(source.find("object").unwrap()).unwrap()
        );
        assert_eq!(
            constraints[1].source.source_start,
            u32::try_from(source.find("unknown").unwrap()).unwrap()
        );
        assert_ne!(constraints[0].owner, constraints[1].owner);

        let defaults = &class.defaults;
        assert_eq!(defaults.len(), 2);
        assert_eq!(defaults[0].parameter_index, 0);
        assert_eq!(defaults[1].parameter_index, 1);
        assert_eq!(
            defaults[0].source.source_start,
            u32::try_from(source.find("string").unwrap()).unwrap()
        );
        assert_eq!(
            defaults[1].source.source_start,
            u32::try_from(source.find("number").unwrap()).unwrap()
        );
        assert_ne!(defaults[0].owner, defaults[1].owner);
        assert_ne!(constraints[0].owner, defaults[0].owner);
        assert_ne!(constraints[1].owner, defaults[1].owner);

        for ticket in reservations.tickets() {
            store.complete(ticket, Vec::new()).unwrap();
        }
        assert!(store.finish().unwrap().is_empty());
    }

    #[test]
    fn repeated_type_group_fragments_keep_exact_declaration_sources() {
        let source = "class C {} interface C { value: number }";
        let prelude_allocator = Allocator::default();
        let allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let binder = crate::binder::bind_module_with_prelude(&prelude.program, &parsed.program);
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(module, UnitSlot::new(0), &parsed.program, &mut store)
            .unwrap();
        super::super::attach_type_decl_owners(
            &mut reservations,
            SourceOrdinal::User(module),
            &binder,
            binder.module,
            &parsed.program,
            &super::super::ModuleDeclarationSpans::index(&binder),
        );
        let group = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("C"))
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty)
            .and_then(|group| binder.type_groups.get(group))
            .expect("class-interface group");
        assert_eq!(group.fragments.len(), 2);
        for fragment in &group.fragments {
            assert_eq!(
                reservations.declaration_source(fragment.declaration),
                Some(SourceSite::user(
                    module,
                    UnitSlot::new(0),
                    fragment.site.declaration_span.start,
                ))
            );
        }
    }

    #[test]
    fn lexical_declaration_owners_keep_each_repeated_type_fragment_site() {
        let source = "interface M { first: number } interface M { last: string }";
        let prelude_allocator = Allocator::default();
        let allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let binder = crate::binder::bind_module_with_prelude(&prelude.program, &parsed.program);
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(module, UnitSlot::new(0), &parsed.program, &mut store)
            .unwrap();
        super::super::attach_type_decl_owners(
            &mut reservations,
            SourceOrdinal::User(module),
            &binder,
            binder.module,
            &parsed.program,
            &super::super::ModuleDeclarationSpans::index(&binder),
        );

        let symbol = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("M"))
            .and_then(|symbol| binder.symbols.get(symbol))
            .expect("merged interface symbol");
        let group = binder
            .type_groups
            .get(symbol.ty.expect("type group"))
            .expect("group row");
        assert_eq!(group.fragments.len(), 2);

        let first = group.fragments[0];
        let second = group.fragments[1];
        let first_owner = reservations
            .declaration_owner(first.declaration)
            .expect("first declaration owner");
        let second_owner = reservations
            .declaration_owner(second.declaration)
            .expect("second declaration owner");
        assert_ne!(first.declaration, second.declaration);
        assert_ne!(first_owner.ticket, second_owner.ticket);
        let first_reservation = reservations
            .declaration_reservation(first.declaration)
            .expect("first exact reservation");
        let second_reservation = reservations
            .declaration_reservation(second.declaration)
            .expect("second exact reservation");
        assert_eq!(first_reservation.kind, DeclarationKind::Interface);
        assert_eq!(
            first_reservation.declaration_span,
            first.site.declaration_span
        );
        assert_eq!(first_reservation.binding_span, first.site.binding_span);
        assert_eq!(second_reservation.kind, DeclarationKind::Interface);
        assert_eq!(
            second_reservation.declaration_span,
            second.site.declaration_span
        );
        assert_eq!(second_reservation.binding_span, second.site.binding_span);

        assert_eq!(
            reservations.declaration_source(first.declaration),
            Some(SourceSite::user(
                module,
                UnitSlot::new(0),
                first.site.declaration_span.start,
            ))
        );
        assert_eq!(
            reservations.declaration_source(second.declaration),
            Some(SourceSite::user(
                module,
                UnitSlot::new(0),
                second.site.declaration_span.start,
            ))
        );
    }

    #[test]
    fn every_binding_leaf_has_a_distinct_empty_declaration_event() {
        let source = "import Default, * as NS from 'pkg'; import type { A as Local } from './x'; const { a: [b], ...c } = value; function f(x, y, { z: [w] }) {} try {} catch ({ q, ...r }) {}";
        let prelude_allocator = Allocator::default();
        let allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let binder = crate::binder::bind_module_with_prelude(&prelude.program, &parsed.program);
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(module, UnitSlot::new(0), &parsed.program, &mut store)
            .unwrap();
        super::super::attach_type_decl_owners(
            &mut reservations,
            SourceOrdinal::User(module),
            &binder,
            binder.module,
            &parsed.program,
            &super::super::ModuleDeclarationSpans::index(&binder),
        );

        let declarations: Vec<_> = binder
            .declarations
            .iter()
            .filter(|declaration| declaration.site.module == binder.module)
            .collect();
        let owners: Vec<_> = declarations
            .iter()
            .map(|declaration| {
                let reservation = reservations
                    .declaration_reservation(declaration.id)
                    .expect("exact declaration reservation");
                assert_eq!(reservation.kind, declaration.kind);
                assert_eq!(
                    reservation.declaration_span,
                    declaration.site.declaration_span
                );
                assert_eq!(reservation.binding_span, declaration.site.binding_span);
                reservations
                    .declaration_owner(declaration.id)
                    .expect("declaration owner")
            })
            .collect();
        assert!(owners.iter().enumerate().all(|(index, owner)| owners
            .iter()
            .skip(index + 1)
            .all(|other| owner.ticket != other.ticket)));

        for ticket in reservations.tickets() {
            store.complete(ticket, Vec::new()).unwrap();
        }
        assert!(store.finish().unwrap().is_empty());
    }

    #[test]
    fn declaration_owner_replays_one_record_through_ordinary_pending_effects() {
        let source = "const value = 1;";
        let prelude_allocator = Allocator::default();
        let allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        let binder = crate::binder::bind_module_with_prelude(&prelude.program, &parsed.program);
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(module, UnitSlot::new(0), &parsed.program, &mut store)
            .unwrap();
        super::super::attach_type_decl_owners(
            &mut reservations,
            SourceOrdinal::User(module),
            &binder,
            binder.module,
            &parsed.program,
            &super::super::ModuleDeclarationSpans::index(&binder),
        );
        let declaration = binder
            .declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Variable)
            .expect("variable declaration");
        let owner = reservations
            .declaration_owner(declaration.id)
            .expect("declaration owner");

        let mut pending: Vec<_> = reservations
            .tickets()
            .into_iter()
            .map(super::super::CheckerEffects::new)
            .collect();
        let effects = pending
            .iter_mut()
            .find(|effects| effects.records.owner() == owner.ticket)
            .expect("declaration ticket participates in ordinary pending effects");
        effects
            .records
            .diagnostic(crate::diagnostics::Diagnostic::cannot_find_name(
                declaration.site.binding_span,
                "recorded",
            ));
        for effects in pending {
            let (owner, records) = effects.records.into_parts();
            store.complete(owner, records).unwrap();
        }
        assert_eq!(
            store.complete(owner.ticket, Vec::new()),
            Err(EventStoreError::DuplicateCompletion(owner.ticket))
        );
        let records = store.finish().unwrap();
        assert_eq!(records.len(), 1);
        let super::super::reporting_record::CheckerRecord::Diagnostic(diagnostic) = &records[0].1
        else {
            panic!("expected declaration diagnostic");
        };
        assert_eq!(diagnostic.code, crate::diagnostics::DiagnosticCode::TK2304);
        assert!(diagnostic.message.contains("recorded"));
    }

    #[test]
    fn lookups_and_binding_attachment_allocate_no_events_or_records() {
        let source = "class C<T> { m<U>() {} }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(
                ModuleOrdinal::new(3),
                UnitSlot::new(1),
                &parsed.program,
                &mut store,
            )
            .unwrap();
        let event_count = store.event_count();
        let record_count = store.record_count();
        let class = reservations
            .class_at(SourceOrdinal::User(ModuleOrdinal::new(3)), 0)
            .unwrap();
        let callable = reservations
            .class(class)
            .and_then(|class| class.members.first())
            .and_then(|member| reservations.member(*member))
            .and_then(|member| member.callable)
            .unwrap();
        let function_start = match &parsed.program.body[0] {
            Statement::ClassDeclaration(class) => match &class.body.body[0] {
                ClassElement::MethodDefinition(method) => method.value.span.start,
                _ => panic!("expected method"),
            },
            _ => panic!("expected class"),
        };
        assert_eq!(
            reservations.callable_at(SourceOrdinal::User(ModuleOrdinal::new(3)), function_start),
            Some(callable)
        );

        reservations
            .attach_class_binding(
                class,
                ClassBinding {
                    class_id: ClassId(7),
                    type_decl: TypeGroupId(8),
                    value_decl: Some(ValueStorageId(9)),
                    header_type_params: vec![TypeParamId(10)],
                },
            )
            .unwrap();
        let mut next_type_param = 11;
        reservations
            .reserve_callable_type_params(&mut next_type_param)
            .unwrap();
        assert_eq!(
            reservations.reserve_callable_type_params(&mut next_type_param),
            Err(ReservationStateError::DuplicateCallableBinding(callable))
        );

        assert_eq!(store.event_count(), event_count);
        assert_eq!(store.record_count(), record_count);
        assert_eq!(next_type_param, 12);
        assert_eq!(
            reservations
                .class(class)
                .unwrap()
                .binding
                .as_ref()
                .unwrap()
                .class_id,
            ClassId(7)
        );
        assert_eq!(
            reservations
                .callable(callable)
                .unwrap()
                .binding
                .as_ref()
                .unwrap()
                .type_params,
            [TypeParamId(11)]
        );
    }

    #[test]
    fn recursive_prewalk_reserves_local_callable_body_and_var_declarator() {
        let source = "function outer() { if (true) { var x: number = 1; function inner<T>() {} } }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(module, UnitSlot::new(0), &parsed.program, &mut store)
            .unwrap();

        assert_eq!(reservations.callables.len(), 2);
        assert_eq!(reservations.declarators.len(), 1);
        let declarator_start = source.find("x:").unwrap() as u32;
        assert!(reservations
            .owner_at(
                SourceOrdinal::User(module),
                declarator_start,
                LexicalOwnerPhase::Immediate,
            )
            .is_some());
        let inner_start = source.find("function inner").unwrap() as u32;
        let signature = reservations
            .owner_at(
                SourceOrdinal::User(module),
                inner_start,
                LexicalOwnerPhase::Immediate,
            )
            .unwrap();
        let body = reservations
            .owner_at(
                SourceOrdinal::User(module),
                inner_start,
                LexicalOwnerPhase::Body,
            )
            .unwrap();
        assert_eq!(signature.ticket.event, body.ticket.event);
        assert_ne!(signature.ticket, body.ticket);

        for ticket in reservations.tickets() {
            store.complete(ticket, Vec::new()).unwrap();
        }
        assert!(store.finish().unwrap().is_empty());
    }

    #[test]
    fn expression_callables_and_block_bodies_reserve_in_source_order() {
        let source = "use({ cb: true ? (() => { const a = 1; }) : function () { const b = 2; }, xs: [() => { const c = 3; }] });";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let module = ModuleOrdinal::new(0);
        let mut store = EventStore::default();
        let mut reservations = LexicalReservations::default();
        reservations
            .reserve_program(module, UnitSlot::new(0), &parsed.program, &mut store)
            .unwrap();

        let starts: Vec<u32> = reservations
            .callables
            .iter()
            .map(|callable| callable.source.source_start)
            .collect();
        assert_eq!(
            starts,
            vec![
                u32::try_from(source.find("() => { const a").unwrap()).unwrap(),
                u32::try_from(source.find("function ()").unwrap()).unwrap(),
                u32::try_from(source.find("() => { const c").unwrap()).unwrap(),
            ]
        );
        for start in &starts {
            let signature = reservations
                .owner_at(
                    SourceOrdinal::User(module),
                    *start,
                    LexicalOwnerPhase::Immediate,
                )
                .unwrap();
            let body = reservations
                .owner_at(SourceOrdinal::User(module), *start, LexicalOwnerPhase::Body)
                .unwrap();
            assert_eq!(signature.ticket.event, body.ticket.event);
            assert_ne!(signature.ticket, body.ticket);
        }
        for name in ["a =", "b =", "c ="] {
            let start = u32::try_from(source.find(name).unwrap()).unwrap();
            assert!(reservations
                .owner_at(
                    SourceOrdinal::User(module),
                    start,
                    LexicalOwnerPhase::Deferred,
                )
                .is_some());
        }

        for ticket in reservations.tickets() {
            store.complete(ticket, Vec::new()).unwrap();
        }
        assert!(store.finish().unwrap().is_empty());
    }

    #[test]
    fn interface_and_alias_qualified_errors_replay_at_their_declaration_owners() {
        let source = "interface First { value: ns.One } type Second = ns.Two;";
        let output = crate::driver::check_source(source);
        assert_eq!(output.diagnostics.len(), 2);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.code,
                    diagnostic.message.as_str(),
                    diagnostic.span.start,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    crate::diagnostics::DiagnosticCode::TK2503,
                    "Cannot find namespace 'ns'.",
                    u32::try_from(source.find("ns.One").unwrap()).unwrap(),
                ),
                (
                    crate::diagnostics::DiagnosticCode::TK2503,
                    "Cannot find namespace 'ns'.",
                    u32::try_from(source.find("ns.Two").unwrap()).unwrap(),
                ),
            ]
        );
        assert!(
            output
                .incomplete
                .iter()
                .all(|record| record.id != "annotation-lower/type-name/qualified-name"),
            "failed paths must not retain a qualified-name incomplete"
        );
    }
}

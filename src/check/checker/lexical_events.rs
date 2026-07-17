//! Lexical event reservations retained across class/SCC/body phases.

use super::events::{EventId, EventStore, EventStoreError, ReservedEvent, UserRecordTicket};
use crate::binder::declaration::{
    source_declaration_occurrences, DeclId, DeclarationKind, TypeGroupId, ValueStorageId,
};
use crate::source::{ModuleOrdinal, UnitSlot};
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ClassSiteId(usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MemberSiteId(usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CallableSiteId(usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceSite {
    pub(crate) module_ordinal: ModuleOrdinal,
    pub(crate) unit_slot: UnitSlot,
    pub(crate) source_start: u32,
}

/// Record positions retained by every callable reservation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteTickets {
    pub(crate) immediate: UserRecordTicket,
    pub(crate) deferred: UserRecordTicket,
    pub(crate) incomplete: UserRecordTicket,
}

/// Record positions retained by every callable reservation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableTickets {
    pub(crate) signature: UserRecordTicket,
    pub(crate) deferred: UserRecordTicket,
    pub(crate) incomplete: UserRecordTicket,
    pub(crate) body: UserRecordTicket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LexicalOwnerPhase {
    Immediate,
    Deferred,
    Incomplete,
    Body,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexicalOwner {
    pub(crate) event: EventId,
    pub(crate) ticket: UserRecordTicket,
}

/// Neutral event reserved for one exact source declaration occurrence.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclarationReservation {
    pub(crate) source: SourceSite,
    pub(crate) kind: DeclarationKind,
    pub(crate) declaration_span: Span,
    pub(crate) binding_span: Span,
    pub(crate) event: EventId,
    pub(crate) owner: UserRecordTicket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ExportAliasReservation {
    local_span: Span,
    event: EventId,
    owner: UserRecordTicket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum InterfaceOccurrenceKind {
    Header,
    Member,
    Heritage,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct InterfaceOccurrenceReservation {
    binding_start: u32,
    source_start: u32,
    kind: InterfaceOccurrenceKind,
    owner: UserRecordTicket,
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
pub(crate) struct TopLevelReservation {
    pub(crate) source: SourceSite,
    pub(crate) event: EventId,
    pub(crate) tickets: SiteTickets,
    pub(crate) class: Option<ClassSiteId>,
    pub(crate) callable: Option<CallableSiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NestedStatementReservation {
    pub(crate) source: SourceSite,
    pub(crate) event: EventId,
    pub(crate) tickets: SiteTickets,
    pub(crate) callable: Option<CallableSiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclaratorReservation {
    pub(crate) source: SourceSite,
    pub(crate) event: EventId,
    pub(crate) tickets: SiteTickets,
}

/// One lexical owner per source class type-parameter default.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassDefaultReservation {
    pub(crate) parameter_index: usize,
    pub(crate) source: SourceSite,
    pub(crate) event: EventId,
    pub(crate) owner: UserRecordTicket,
}

/// One lexical owner per source class type-parameter constraint.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassConstraintReservation {
    pub(crate) parameter_index: usize,
    pub(crate) source: SourceSite,
    pub(crate) event: EventId,
    pub(crate) owner: UserRecordTicket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassReservation {
    pub(crate) id: ClassSiteId,
    pub(crate) source: SourceSite,
    pub(crate) event: EventId,
    pub(crate) tickets: SiteTickets,
    pub(crate) constraints: Vec<ClassConstraintReservation>,
    pub(crate) defaults: Vec<ClassDefaultReservation>,
    pub(crate) members: Vec<MemberSiteId>,
    pub(crate) binding: Option<ClassBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberReservation {
    pub(crate) id: MemberSiteId,
    pub(crate) class: ClassSiteId,
    pub(crate) source: SourceSite,
    pub(crate) event: EventId,
    pub(crate) tickets: SiteTickets,
    pub(crate) callable: Option<CallableSiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableReservation {
    pub(crate) id: CallableSiteId,
    pub(crate) owner_member: Option<MemberSiteId>,
    pub(crate) source: SourceSite,
    pub(crate) event: EventId,
    pub(crate) tickets: CallableTickets,
    type_parameter_count: usize,
    pub(crate) binding: Option<CallableBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReservationError {
    UnknownClass(ClassSiteId),
    DuplicateClassBinding(ClassSiteId),
    DuplicateCallableBinding(CallableSiteId),
    MissingDeclarationOwner(DeclId),
    EventStore(EventStoreError),
}

impl From<EventStoreError> for ReservationError {
    fn from(error: EventStoreError) -> Self {
        Self::EventStore(error)
    }
}

/// Persistent source-site table built before class construction, SCCs, and bodies.
#[derive(Debug, Default)]
pub(crate) struct LexicalReservations {
    top_level: Vec<TopLevelReservation>,
    nested_statements: Vec<NestedStatementReservation>,
    declarators: Vec<DeclaratorReservation>,
    classes: Vec<ClassReservation>,
    members: Vec<MemberReservation>,
    callables: Vec<CallableReservation>,
    expression_site_tickets: Vec<SiteTickets>,
    declarations: Vec<DeclarationReservation>,
    export_aliases: Vec<ExportAliasReservation>,
    interface_occurrences: Vec<InterfaceOccurrenceReservation>,
    interface_occurrences_by_source:
        FxHashMap<(ModuleOrdinal, u32, InterfaceOccurrenceKind, u32), usize>,
    declarations_by_binding: FxHashMap<(ModuleOrdinal, u32, u32), usize>,
    export_aliases_by_local_span: FxHashMap<(ModuleOrdinal, u32, u32), usize>,
    classes_by_source: BTreeMap<(ModuleOrdinal, u32), Vec<ClassSiteId>>,
    callables_by_source: BTreeMap<(ModuleOrdinal, u32), Vec<CallableSiteId>>,
    declaration_owners: FxHashMap<DeclId, LexicalOwner>,
}

impl LexicalReservations {
    /// Walk one program in lexical order and reserve all top-level/class/callable sites.
    pub(crate) fn reserve_program(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        program: &Program<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        for occurrence in source_declaration_occurrences(program) {
            let event = store.reserve_event(module_ordinal, occurrence.binding_span.start);
            let index = self.declarations.len();
            self.declarations.push(DeclarationReservation {
                source: SourceSite {
                    module_ordinal,
                    unit_slot,
                    source_start: occurrence.declaration_span.start,
                },
                kind: occurrence.kind,
                declaration_span: occurrence.declaration_span,
                binding_span: occurrence.binding_span,
                event: event.id,
                owner: event.primary,
            });
            let previous = self.declarations_by_binding.insert(
                (
                    module_ordinal,
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
                    self.reserve_export_aliases_in_module(module_ordinal, declaration, store)?;
                }
                Statement::ExportNamedDeclaration(export) => {
                    if let Some(Declaration::TSModuleDeclaration(declaration)) = &export.declaration
                    {
                        self.reserve_export_aliases_in_module(module_ordinal, declaration, store)?;
                    }
                }
                _ => {}
            }
        }
        self.reserve_interface_occurrences_in_statements(module_ordinal, &program.body, store);
        self.reserve_statement_list(module_ordinal, unit_slot, &program.body, true, store)
    }

    fn reserve_interface_occurrences_in_statements(
        &mut self,
        module_ordinal: ModuleOrdinal,
        statements: &[Statement<'_>],
        store: &mut EventStore,
    ) {
        for statement in statements {
            match statement {
                Statement::TSInterfaceDeclaration(interface) => {
                    self.reserve_interface_occurrences(module_ordinal, interface, store);
                }
                Statement::ExportNamedDeclaration(export) => match &export.declaration {
                    Some(Declaration::TSInterfaceDeclaration(interface)) => {
                        self.reserve_interface_occurrences(module_ordinal, interface, store);
                    }
                    Some(Declaration::TSModuleDeclaration(module)) => {
                        self.reserve_interface_occurrences_in_module(module_ordinal, module, store);
                    }
                    Some(Declaration::TSGlobalDeclaration(global)) => {
                        self.reserve_interface_occurrences_in_statements(
                            module_ordinal,
                            &global.body.body,
                            store,
                        );
                    }
                    _ => {}
                },
                Statement::TSModuleDeclaration(module) => {
                    self.reserve_interface_occurrences_in_module(module_ordinal, module, store);
                }
                Statement::TSGlobalDeclaration(global) => {
                    self.reserve_interface_occurrences_in_statements(
                        module_ordinal,
                        &global.body.body,
                        store,
                    );
                }
                _ => {}
            }
        }
    }

    fn reserve_interface_occurrences_in_module(
        &mut self,
        module_ordinal: ModuleOrdinal,
        module: &TSModuleDeclaration<'_>,
        store: &mut EventStore,
    ) {
        match &module.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                self.reserve_interface_occurrences_in_statements(module_ordinal, &block.body, store)
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
                self.reserve_interface_occurrences_in_module(module_ordinal, nested, store);
            }
            None => {}
        }
    }

    fn reserve_interface_occurrences(
        &mut self,
        module_ordinal: ModuleOrdinal,
        interface: &oxc_ast::ast::TSInterfaceDeclaration<'_>,
        store: &mut EventStore,
    ) {
        let binding_start = interface.id.span.start;
        self.reserve_interface_occurrence(
            module_ordinal,
            binding_start,
            InterfaceOccurrenceKind::Header,
            binding_start,
            store,
        );
        for heritage in &interface.extends {
            self.reserve_interface_occurrence(
                module_ordinal,
                binding_start,
                InterfaceOccurrenceKind::Heritage,
                heritage.span.start,
                store,
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
                module_ordinal,
                binding_start,
                InterfaceOccurrenceKind::Member,
                source_start,
                store,
            );
        }
    }

    fn reserve_interface_occurrence(
        &mut self,
        module_ordinal: ModuleOrdinal,
        binding_start: u32,
        kind: InterfaceOccurrenceKind,
        source_start: u32,
        store: &mut EventStore,
    ) {
        let event = store.reserve_event(module_ordinal, source_start);
        let index = self.interface_occurrences.len();
        self.interface_occurrences
            .push(InterfaceOccurrenceReservation {
                binding_start,
                source_start,
                kind,
                owner: event.primary,
            });
        let previous = self
            .interface_occurrences_by_source
            .insert((module_ordinal, binding_start, kind, source_start), index);
        debug_assert!(previous.is_none(), "one exact interface occurrence owner");
    }

    fn reserve_export_aliases_in_statement(
        &mut self,
        module_ordinal: ModuleOrdinal,
        statement: &Statement<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        match statement {
            Statement::TSModuleDeclaration(declaration) => {
                self.reserve_export_aliases_in_module(module_ordinal, declaration, store)?;
            }
            Statement::ExportNamedDeclaration(export) => {
                if export.source.is_none() && export.declaration.is_none() {
                    for specifier in &export.specifiers {
                        let local_span = Span::from_oxc(specifier.local.span());
                        let event = store.reserve_event(module_ordinal, local_span.start);
                        let index = self.export_aliases.len();
                        self.export_aliases.push(ExportAliasReservation {
                            local_span,
                            event: event.id,
                            owner: event.primary,
                        });
                        let previous = self
                            .export_aliases_by_local_span
                            .insert((module_ordinal, local_span.start, local_span.end), index);
                        debug_assert!(
                            previous.is_none(),
                            "one export alias reservation per exact local span"
                        );
                    }
                }
                if let Some(Declaration::TSModuleDeclaration(declaration)) = &export.declaration {
                    self.reserve_export_aliases_in_module(module_ordinal, declaration, store)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn reserve_export_aliases_in_module(
        &mut self,
        module_ordinal: ModuleOrdinal,
        declaration: &TSModuleDeclaration<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        match &declaration.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                for statement in &block.body {
                    self.reserve_export_aliases_in_statement(module_ordinal, statement, store)?;
                }
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
                self.reserve_export_aliases_in_module(module_ordinal, nested, store)?;
            }
            None => {}
        }
        Ok(())
    }

    fn reserve_statement_list(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        statements: &[Statement<'_>],
        top_level: bool,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        for statement in statements {
            self.reserve_statement(module_ordinal, unit_slot, statement, top_level, store)?;
        }
        Ok(())
    }

    fn reserve_statement(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        statement: &Statement<'_>,
        top_level: bool,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let source = SourceSite {
            module_ordinal,
            unit_slot,
            source_start: statement.span().start,
        };
        let event = store.reserve_event(module_ordinal, source.source_start);
        let tickets = reserve_site_tickets(event, store)?;
        let mut class_site = None;
        let mut callable = None;
        if let Some(class) = statement_class(statement) {
            class_site = Some(self.reserve_class(source, class, event, tickets, store)?);
        } else if let Some(function) = statement_function(statement) {
            callable = Some(self.reserve_callable(source, event, tickets, None, function, store)?);
        }
        if top_level {
            self.top_level.push(TopLevelReservation {
                source,
                event: event.id,
                tickets,
                class: class_site,
                callable,
            });
        } else {
            self.nested_statements.push(NestedStatementReservation {
                source,
                event: event.id,
                tickets,
                callable,
            });
        }

        if let Some(declaration) = statement_variable_declaration(statement) {
            self.reserve_declarators(source, declaration, store)?;
        }
        if let Some(function) = statement_function(statement) {
            self.reserve_parameter_expressions(source, &function.params, store)?;
            if let Some(body) = function.body.as_ref() {
                self.reserve_statement_list(
                    module_ordinal,
                    unit_slot,
                    &body.statements,
                    false,
                    store,
                )?;
            }
            return Ok(());
        }
        if statement_class(statement).is_some() {
            return Ok(());
        }
        self.reserve_statement_expressions(source, statement, store)?;
        match statement {
            Statement::BlockStatement(block) => {
                self.reserve_statement_list(module_ordinal, unit_slot, &block.body, false, store)?
            }
            Statement::IfStatement(statement) => {
                self.reserve_statement(
                    module_ordinal,
                    unit_slot,
                    &statement.consequent,
                    false,
                    store,
                )?;
                if let Some(alternate) = statement.alternate.as_ref() {
                    self.reserve_statement(module_ordinal, unit_slot, alternate, false, store)?;
                }
            }
            Statement::DoWhileStatement(statement) => {
                self.reserve_statement(module_ordinal, unit_slot, &statement.body, false, store)?
            }
            Statement::WhileStatement(statement) => {
                self.reserve_statement(module_ordinal, unit_slot, &statement.body, false, store)?
            }
            Statement::ForStatement(statement) => {
                if let Some(ForStatementInit::VariableDeclaration(declaration)) = &statement.init {
                    self.reserve_declarators(source, declaration, store)?;
                }
                self.reserve_statement(module_ordinal, unit_slot, &statement.body, false, store)?;
            }
            Statement::ForInStatement(statement) => {
                if let ForStatementLeft::VariableDeclaration(declaration) = &statement.left {
                    self.reserve_declarators(source, declaration, store)?;
                }
                self.reserve_statement(module_ordinal, unit_slot, &statement.body, false, store)?;
            }
            Statement::ForOfStatement(statement) => {
                if let ForStatementLeft::VariableDeclaration(declaration) = &statement.left {
                    self.reserve_declarators(source, declaration, store)?;
                }
                self.reserve_statement(module_ordinal, unit_slot, &statement.body, false, store)?;
            }
            Statement::WithStatement(statement) => {
                self.reserve_statement(module_ordinal, unit_slot, &statement.body, false, store)?
            }
            Statement::SwitchStatement(statement) => {
                for case in &statement.cases {
                    self.reserve_statement_list(
                        module_ordinal,
                        unit_slot,
                        &case.consequent,
                        false,
                        store,
                    )?;
                }
            }
            Statement::LabeledStatement(statement) => {
                self.reserve_statement(module_ordinal, unit_slot, &statement.body, false, store)?
            }
            Statement::TryStatement(statement) => {
                self.reserve_statement_list(
                    module_ordinal,
                    unit_slot,
                    &statement.block.body,
                    false,
                    store,
                )?;
                if let Some(handler) = &statement.handler {
                    self.reserve_statement_list(
                        module_ordinal,
                        unit_slot,
                        &handler.body.body,
                        false,
                        store,
                    )?;
                }
                if let Some(finalizer) = &statement.finalizer {
                    self.reserve_statement_list(
                        module_ordinal,
                        unit_slot,
                        &finalizer.body,
                        false,
                        store,
                    )?;
                }
            }
            Statement::TSModuleDeclaration(module) => {
                self.reserve_module_statements(module_ordinal, unit_slot, module, store)?;
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::TSModuleDeclaration(module)) = &export.declaration {
                    self.reserve_module_statements(module_ordinal, unit_slot, module, store)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn reserve_module_statements(
        &mut self,
        module_ordinal: ModuleOrdinal,
        unit_slot: UnitSlot,
        module: &TSModuleDeclaration<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        match &module.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                self.reserve_statement_list(module_ordinal, unit_slot, &block.body, false, store)?;
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
                self.reserve_module_statements(module_ordinal, unit_slot, nested, store)?;
            }
            None => {}
        }
        Ok(())
    }

    fn reserve_declarators(
        &mut self,
        parent: SourceSite,
        declaration: &VariableDeclaration<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        for declarator in &declaration.declarations {
            let source = SourceSite {
                source_start: declarator.span.start,
                ..parent
            };
            let event = store.reserve_event(source.module_ordinal, source.source_start);
            let tickets = reserve_site_tickets(event, store)?;
            self.declarators.push(DeclaratorReservation {
                source,
                event: event.id,
                tickets,
            });
            if let Some(initializer) = &declarator.init {
                self.reserve_expression(source, initializer, store)?;
            }
        }
        Ok(())
    }

    fn reserve_statement_expressions(
        &mut self,
        source: SourceSite,
        statement: &Statement<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        match statement {
            Statement::DoWhileStatement(statement) => {
                self.reserve_expression(source, &statement.test, store)?;
            }
            Statement::ExpressionStatement(statement) => {
                self.reserve_expression(source, &statement.expression, store)?;
            }
            Statement::ForStatement(statement) => {
                if let Some(expression) = statement
                    .init
                    .as_ref()
                    .and_then(ForStatementInit::as_expression)
                {
                    self.reserve_expression(source, expression, store)?;
                }
                if let Some(test) = &statement.test {
                    self.reserve_expression(source, test, store)?;
                }
                if let Some(update) = &statement.update {
                    self.reserve_expression(source, update, store)?;
                }
            }
            Statement::ForInStatement(statement) => {
                self.reserve_expression(source, &statement.right, store)?;
            }
            Statement::ForOfStatement(statement) => {
                self.reserve_expression(source, &statement.right, store)?;
            }
            Statement::IfStatement(statement) => {
                self.reserve_expression(source, &statement.test, store)?;
            }
            Statement::ReturnStatement(statement) => {
                if let Some(argument) = &statement.argument {
                    self.reserve_expression(source, argument, store)?;
                }
            }
            Statement::SwitchStatement(statement) => {
                self.reserve_expression(source, &statement.discriminant, store)?;
                for case in &statement.cases {
                    if let Some(test) = &case.test {
                        self.reserve_expression(source, test, store)?;
                    }
                }
            }
            Statement::ThrowStatement(statement) => {
                self.reserve_expression(source, &statement.argument, store)?;
            }
            Statement::WhileStatement(statement) => {
                self.reserve_expression(source, &statement.test, store)?;
            }
            Statement::WithStatement(statement) => {
                self.reserve_expression(source, &statement.object, store)?;
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let Some(expression) = export.declaration.as_expression() {
                    self.reserve_expression(source, expression, store)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn reserve_expression(
        &mut self,
        source: SourceSite,
        expression: &Expression<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        match expression {
            Expression::TemplateLiteral(template) => {
                for expression in &template.expressions {
                    self.reserve_expression(source, expression, store)?;
                }
            }
            Expression::ArrayExpression(array) => {
                for element in &array.elements {
                    if let Some(expression) = element.as_expression() {
                        self.reserve_expression(source, expression, store)?;
                    } else if let ArrayExpressionElement::SpreadElement(spread) = element {
                        self.reserve_expression(source, &spread.argument, store)?;
                    }
                }
            }
            Expression::ArrowFunctionExpression(arrow) => {
                self.reserve_arrow_expression(source, arrow, store)?;
            }
            Expression::AssignmentExpression(assignment) => {
                self.reserve_expression(source, &assignment.right, store)?;
            }
            Expression::AwaitExpression(await_expression) => {
                self.reserve_expression(source, &await_expression.argument, store)?;
            }
            Expression::BinaryExpression(binary) => {
                self.reserve_expression(source, &binary.left, store)?;
                self.reserve_expression(source, &binary.right, store)?;
            }
            Expression::CallExpression(call) => {
                self.reserve_call_expression(source, call, store)?;
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(call) => {
                    self.reserve_call_expression(source, call, store)?;
                }
                ChainElement::TSNonNullExpression(expression) => {
                    self.reserve_expression(source, &expression.expression, store)?;
                }
                ChainElement::ComputedMemberExpression(member) => {
                    self.reserve_expression(source, &member.object, store)?;
                    self.reserve_expression(source, &member.expression, store)?;
                }
                ChainElement::StaticMemberExpression(member) => {
                    self.reserve_expression(source, &member.object, store)?;
                }
                ChainElement::PrivateFieldExpression(member) => {
                    self.reserve_expression(source, &member.object, store)?;
                }
            },
            Expression::ClassExpression(class) => {
                self.reserve_class_expression(source, class, store)?;
            }
            Expression::ConditionalExpression(conditional) => {
                self.reserve_expression(source, &conditional.test, store)?;
                self.reserve_expression(source, &conditional.consequent, store)?;
                self.reserve_expression(source, &conditional.alternate, store)?;
            }
            Expression::FunctionExpression(function) => {
                self.reserve_function_expression(source, function, store)?;
            }
            Expression::ImportExpression(import) => {
                self.reserve_expression(source, &import.source, store)?;
                if let Some(options) = &import.options {
                    self.reserve_expression(source, options, store)?;
                }
            }
            Expression::LogicalExpression(logical) => {
                self.reserve_expression(source, &logical.left, store)?;
                self.reserve_expression(source, &logical.right, store)?;
            }
            Expression::NewExpression(new_expression) => {
                self.reserve_expression(source, &new_expression.callee, store)?;
                self.reserve_arguments(source, &new_expression.arguments, store)?;
            }
            Expression::ObjectExpression(object) => {
                for property in &object.properties {
                    match property {
                        ObjectPropertyKind::ObjectProperty(property) => {
                            self.reserve_property_key(source, &property.key, store)?;
                            self.reserve_expression(source, &property.value, store)?;
                        }
                        ObjectPropertyKind::SpreadProperty(spread) => {
                            self.reserve_expression(source, &spread.argument, store)?;
                        }
                    }
                }
            }
            Expression::ParenthesizedExpression(parenthesized) => {
                self.reserve_expression(source, &parenthesized.expression, store)?;
            }
            Expression::SequenceExpression(sequence) => {
                for expression in &sequence.expressions {
                    self.reserve_expression(source, expression, store)?;
                }
            }
            Expression::TaggedTemplateExpression(tagged) => {
                self.reserve_expression(source, &tagged.tag, store)?;
                for expression in &tagged.quasi.expressions {
                    self.reserve_expression(source, expression, store)?;
                }
            }
            Expression::UnaryExpression(unary) => {
                self.reserve_expression(source, &unary.argument, store)?;
            }
            Expression::YieldExpression(yield_expression) => {
                if let Some(argument) = &yield_expression.argument {
                    self.reserve_expression(source, argument, store)?;
                }
            }
            Expression::PrivateInExpression(private_in) => {
                self.reserve_expression(source, &private_in.right, store)?;
            }
            Expression::TSAsExpression(assertion) => {
                self.reserve_expression(source, &assertion.expression, store)?;
            }
            Expression::TSSatisfiesExpression(assertion) => {
                self.reserve_expression(source, &assertion.expression, store)?;
            }
            Expression::TSTypeAssertion(assertion) => {
                self.reserve_expression(source, &assertion.expression, store)?;
            }
            Expression::TSNonNullExpression(expression) => {
                self.reserve_expression(source, &expression.expression, store)?;
            }
            Expression::TSInstantiationExpression(instantiation) => {
                self.reserve_expression(source, &instantiation.expression, store)?;
            }
            Expression::ComputedMemberExpression(member) => {
                self.reserve_expression(source, &member.object, store)?;
                self.reserve_expression(source, &member.expression, store)?;
            }
            Expression::StaticMemberExpression(member) => {
                self.reserve_expression(source, &member.object, store)?;
            }
            Expression::PrivateFieldExpression(member) => {
                self.reserve_expression(source, &member.object, store)?;
            }
            Expression::V8IntrinsicExpression(intrinsic) => {
                self.reserve_arguments(source, &intrinsic.arguments, store)?;
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

    fn reserve_arguments(
        &mut self,
        source: SourceSite,
        arguments: &[Argument<'_>],
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        for argument in arguments {
            if let Some(expression) = argument.as_expression() {
                self.reserve_expression(source, expression, store)?;
            } else if let Argument::SpreadElement(spread) = argument {
                self.reserve_expression(source, &spread.argument, store)?;
            }
        }
        Ok(())
    }

    fn reserve_call_expression(
        &mut self,
        source: SourceSite,
        call: &oxc_ast::ast::CallExpression<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        self.reserve_expression(source, &call.callee, store)?;
        self.reserve_arguments(source, &call.arguments, store)
    }

    fn reserve_property_key(
        &mut self,
        source: SourceSite,
        key: &PropertyKey<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        if let Some(expression) = key.as_expression() {
            self.reserve_expression(source, expression, store)?;
        }
        Ok(())
    }

    fn reserve_parameter_expressions(
        &mut self,
        source: SourceSite,
        parameters: &FormalParameters<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        for parameter in &parameters.items {
            if let Some(initializer) = &parameter.initializer {
                self.reserve_expression(source, initializer, store)?;
            }
        }
        Ok(())
    }

    fn reserve_function_expression(
        &mut self,
        parent: SourceSite,
        function: &Function<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let source = SourceSite {
            source_start: function.span.start,
            ..parent
        };
        let event = store.reserve_event(source.module_ordinal, source.source_start);
        let tickets = reserve_site_tickets(event, store)?;
        self.expression_site_tickets.push(tickets);
        self.reserve_callable(source, event, tickets, None, function, store)?;
        self.reserve_parameter_expressions(source, &function.params, store)?;
        if let Some(body) = &function.body {
            self.reserve_statement_list(
                source.module_ordinal,
                source.unit_slot,
                &body.statements,
                false,
                store,
            )?;
        }
        Ok(())
    }

    fn reserve_arrow_expression(
        &mut self,
        parent: SourceSite,
        arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let source = SourceSite {
            source_start: arrow.span.start,
            ..parent
        };
        let event = store.reserve_event(source.module_ordinal, source.source_start);
        let tickets = reserve_site_tickets(event, store)?;
        self.expression_site_tickets.push(tickets);
        self.reserve_arrow_callable(source, event, tickets, arrow, store)?;
        self.reserve_parameter_expressions(source, &arrow.params, store)?;
        if let Some(expression) = arrow.get_expression() {
            self.reserve_expression(source, expression, store)?;
        } else {
            self.reserve_statement_list(
                source.module_ordinal,
                source.unit_slot,
                &arrow.body.statements,
                false,
                store,
            )?;
        }
        Ok(())
    }

    fn reserve_class_expression(
        &mut self,
        parent: SourceSite,
        class: &Class<'_>,
        store: &mut EventStore,
    ) -> Result<(), ReservationError> {
        let source = SourceSite {
            source_start: class.span.start,
            ..parent
        };
        let event = store.reserve_event(source.module_ordinal, source.source_start);
        let tickets = reserve_site_tickets(event, store)?;
        self.expression_site_tickets.push(tickets);
        self.reserve_class(source, class, event, tickets, store)?;
        Ok(())
    }

    pub(crate) fn top_level(&self) -> &[TopLevelReservation] {
        &self.top_level
    }

    pub(crate) fn classes(&self) -> &[ClassReservation] {
        &self.classes
    }

    pub(crate) fn class(&self, id: ClassSiteId) -> Option<&ClassReservation> {
        self.classes.get(id.0)
    }

    pub(crate) fn class_at(
        &self,
        module_ordinal: ModuleOrdinal,
        source_start: u32,
    ) -> Option<ClassSiteId> {
        self.classes_by_source
            .get(&(module_ordinal, source_start))
            .and_then(|ids| ids.first())
            .copied()
    }

    pub(crate) fn member(&self, id: MemberSiteId) -> Option<&MemberReservation> {
        self.members.get(id.0)
    }

    pub(crate) fn callable(&self, id: CallableSiteId) -> Option<&CallableReservation> {
        self.callables.get(id.0)
    }

    pub(crate) fn callable_at(
        &self,
        module_ordinal: ModuleOrdinal,
        source_start: u32,
    ) -> Option<CallableSiteId> {
        self.callables_by_source
            .get(&(module_ordinal, source_start))
            .and_then(|ids| ids.first())
            .copied()
    }

    pub(crate) fn attach_declaration_owner(
        &mut self,
        declaration: DeclId,
        module_ordinal: ModuleOrdinal,
        kind: DeclarationKind,
        declaration_span: Span,
        binding_span: Span,
    ) -> Result<(), ReservationError> {
        let reservation = self
            .declarations_by_binding
            .get(&(module_ordinal, binding_span.start, binding_span.end))
            .and_then(|index| self.declarations.get(*index))
            .ok_or(ReservationError::MissingDeclarationOwner(declaration))?;
        assert_eq!(
            reservation.kind, kind,
            "declaration kind must match source prewalk"
        );
        assert_eq!(
            reservation.declaration_span, declaration_span,
            "declaration node span must match source prewalk"
        );
        let owner = LexicalOwner {
            event: reservation.event,
            ticket: reservation.owner,
        };
        self.declaration_owners.insert(declaration, owner);
        Ok(())
    }

    pub(crate) fn export_alias_owner(
        &self,
        module_ordinal: ModuleOrdinal,
        local_span: Span,
    ) -> Option<LexicalOwner> {
        let reservation = self
            .export_aliases_by_local_span
            .get(&(module_ordinal, local_span.start, local_span.end))
            .and_then(|index| self.export_aliases.get(*index))?;
        debug_assert_eq!(reservation.local_span, local_span);
        Some(LexicalOwner {
            event: reservation.event,
            ticket: reservation.owner,
        })
    }

    pub(crate) fn declaration_owner(&self, declaration: DeclId) -> Option<LexicalOwner> {
        self.declaration_owners.get(&declaration).copied()
    }

    pub(crate) fn declaration_source(&self, declaration: DeclId) -> Option<SourceSite> {
        self.declaration_reservation(declaration)
            .map(|reservation| reservation.source)
    }

    pub(crate) fn declaration_reservation(
        &self,
        declaration: DeclId,
    ) -> Option<&DeclarationReservation> {
        let owner = self.declaration_owner(declaration)?;
        self.declarations.iter().find(|reservation| {
            reservation.event == owner.event && reservation.owner == owner.ticket
        })
    }

    pub(crate) fn interface_occurrence_owner(
        &self,
        declaration: DeclId,
        kind: InterfaceOccurrenceKind,
        source_start: u32,
    ) -> Option<UserRecordTicket> {
        let reservation = self.declaration_reservation(declaration)?;
        let index = self.interface_occurrences_by_source.get(&(
            reservation.source.module_ordinal,
            reservation.binding_span.start,
            kind,
            source_start,
        ))?;
        let occurrence = self.interface_occurrences.get(*index)?;
        debug_assert_eq!(occurrence.binding_start, reservation.binding_span.start);
        debug_assert_eq!(occurrence.source_start, source_start);
        debug_assert_eq!(occurrence.kind, kind);
        Some(occurrence.owner)
    }

    pub(crate) fn owner_at(
        &self,
        module_ordinal: ModuleOrdinal,
        source_start: u32,
        phase: LexicalOwnerPhase,
    ) -> Option<LexicalOwner> {
        if let Some(callable) = self
            .callable_at(module_ordinal, source_start)
            .and_then(|site| self.callable(site))
        {
            let ticket = match phase {
                LexicalOwnerPhase::Immediate => callable.tickets.signature,
                LexicalOwnerPhase::Deferred => callable.tickets.deferred,
                LexicalOwnerPhase::Incomplete => callable.tickets.incomplete,
                LexicalOwnerPhase::Body => callable.tickets.body,
            };
            return Some(LexicalOwner {
                event: callable.event,
                ticket,
            });
        }
        let site = self
            .declarators
            .iter()
            .find(|site| {
                site.source.module_ordinal == module_ordinal
                    && site.source.source_start == source_start
            })
            .map(|site| (site.event, site.tickets))
            .or_else(|| {
                self.nested_statements
                    .iter()
                    .find(|site| {
                        site.source.module_ordinal == module_ordinal
                            && site.source.source_start == source_start
                    })
                    .map(|site| (site.event, site.tickets))
            })
            .or_else(|| {
                self.members
                    .iter()
                    .find(|site| {
                        site.source.module_ordinal == module_ordinal
                            && site.source.source_start == source_start
                    })
                    .map(|site| (site.event, site.tickets))
            })
            .or_else(|| {
                self.top_level
                    .iter()
                    .find(|site| {
                        site.source.module_ordinal == module_ordinal
                            && site.source.source_start == source_start
                    })
                    .map(|site| (site.event, site.tickets))
            })?;
        let ticket = match phase {
            LexicalOwnerPhase::Immediate | LexicalOwnerPhase::Body => site.1.immediate,
            LexicalOwnerPhase::Deferred => site.1.deferred,
            LexicalOwnerPhase::Incomplete => site.1.incomplete,
        };
        Some(LexicalOwner {
            event: site.0,
            ticket,
        })
    }

    pub(crate) fn attach_class_binding(
        &mut self,
        site: ClassSiteId,
        binding: ClassBinding,
    ) -> Result<(), ReservationError> {
        let Some(reservation) = self.classes.get_mut(site.0) else {
            return Err(ReservationError::UnknownClass(site));
        };
        if reservation.binding.is_some() {
            return Err(ReservationError::DuplicateClassBinding(site));
        }
        reservation.binding = Some(binding);
        Ok(())
    }

    /// Allocate every callable binder during reservation, before class fill or bodies.
    pub(crate) fn reserve_callable_type_params(
        &mut self,
        next_type_param: &mut u32,
    ) -> Result<(), ReservationError> {
        if let Some(callable) = self
            .callables
            .iter()
            .find(|callable| callable.binding.is_some())
        {
            return Err(ReservationError::DuplicateCallableBinding(callable.id));
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

    pub(crate) fn tickets(&self) -> Vec<UserRecordTicket> {
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

    fn reserve_class(
        &mut self,
        source: SourceSite,
        class: &Class<'_>,
        event: ReservedEvent,
        tickets: SiteTickets,
        store: &mut EventStore,
    ) -> Result<ClassSiteId, ReservationError> {
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
                    module_ordinal: source.module_ordinal,
                    unit_slot: source.unit_slot,
                    source_start: constraint.span().start,
                };
                let event = store.reserve_event(source.module_ordinal, source.source_start);
                constraints.push(ClassConstraintReservation {
                    parameter_index,
                    source,
                    event: event.id,
                    owner: event.primary,
                });
            }
            if let Some(default) = parameter.default.as_ref() {
                let source = SourceSite {
                    module_ordinal: source.module_ordinal,
                    unit_slot: source.unit_slot,
                    source_start: default.span().start,
                };
                let event = store.reserve_event(source.module_ordinal, source.source_start);
                defaults.push(ClassDefaultReservation {
                    parameter_index,
                    source,
                    event: event.id,
                    owner: event.primary,
                });
            }
        }
        self.classes.push(ClassReservation {
            id,
            source,
            event: event.id,
            tickets,
            constraints,
            defaults,
            members: Vec::new(),
            binding: None,
        });
        self.classes_by_source
            .entry((source.module_ordinal, class.span.start))
            .or_default()
            .push(id);

        if let Some(super_class) = &class.super_class {
            self.reserve_expression(source, super_class, store)?;
        }

        for element in &class.body.body {
            let element_source = SourceSite {
                module_ordinal: source.module_ordinal,
                unit_slot: source.unit_slot,
                source_start: element.span().start,
            };
            let member_event =
                store.reserve_event(source.module_ordinal, element_source.source_start);
            let member_tickets = reserve_site_tickets(member_event, store)?;
            let member_id = MemberSiteId(self.members.len());
            let mut member = MemberReservation {
                id: member_id,
                class: id,
                source: element_source,
                event: member_event.id,
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
                    store,
                )?);
                self.reserve_property_key(element_source, &method.key, store)?;
                self.reserve_parameter_expressions(element_source, &method.value.params, store)?;
            } else if let ClassElement::PropertyDefinition(property) = element {
                self.reserve_property_key(element_source, &property.key, store)?;
                if let Some(value) = &property.value {
                    self.reserve_expression(element_source, value, store)?;
                }
            } else if let ClassElement::AccessorProperty(property) = element {
                self.reserve_property_key(element_source, &property.key, store)?;
                if let Some(value) = &property.value {
                    self.reserve_expression(element_source, value, store)?;
                }
            }
            self.members.push(member);
            self.classes[id.0].members.push(member_id);
            match element {
                ClassElement::MethodDefinition(method) => {
                    if let Some(body) = method.value.body.as_ref() {
                        self.reserve_statement_list(
                            source.module_ordinal,
                            source.unit_slot,
                            &body.statements,
                            false,
                            store,
                        )?;
                    }
                }
                ClassElement::StaticBlock(block) => {
                    self.reserve_statement_list(
                        source.module_ordinal,
                        source.unit_slot,
                        &block.body,
                        false,
                        store,
                    )?;
                }
                ClassElement::PropertyDefinition(_)
                | ClassElement::AccessorProperty(_)
                | ClassElement::TSIndexSignature(_) => {}
            }
        }
        Ok(id)
    }

    fn reserve_callable(
        &mut self,
        source: SourceSite,
        event: ReservedEvent,
        site_tickets: SiteTickets,
        owner_member: Option<MemberSiteId>,
        function: &Function<'_>,
        store: &mut EventStore,
    ) -> Result<CallableSiteId, ReservationError> {
        let id = CallableSiteId(self.callables.len());
        let body = store.reserve_record(event.id)?;
        self.callables.push(CallableReservation {
            id,
            owner_member,
            source,
            event: event.id,
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
            .entry((source.module_ordinal, function.span.start))
            .or_default()
            .push(id);
        Ok(id)
    }

    fn reserve_arrow_callable(
        &mut self,
        source: SourceSite,
        event: ReservedEvent,
        site_tickets: SiteTickets,
        arrow: &oxc_ast::ast::ArrowFunctionExpression<'_>,
        store: &mut EventStore,
    ) -> Result<CallableSiteId, ReservationError> {
        let id = CallableSiteId(self.callables.len());
        let body = store.reserve_record(event.id)?;
        self.callables.push(CallableReservation {
            id,
            owner_member: None,
            source,
            event: event.id,
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
            .entry((source.module_ordinal, arrow.span.start))
            .or_default()
            .push(id);
        Ok(id)
    }
}

fn reserve_site_tickets(
    event: ReservedEvent,
    store: &mut EventStore,
) -> Result<SiteTickets, EventStoreError> {
    Ok(SiteTickets {
        immediate: event.primary,
        deferred: store.reserve_record(event.id)?,
        incomplete: store.reserve_record(event.id)?,
    })
}

fn site_tickets(tickets: SiteTickets) -> [UserRecordTicket; 3] {
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
mod tests {
    use super::*;
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
            .export_alias_owner(module, source_span(source, "TopLevel"))
            .is_none());
        let first = reservations
            .export_alias_owner(module, source_span(source, "MissingOne"))
            .expect("first local alias owner");
        let resolved_start = source
            .find("Resolved as PublicResolved")
            .expect("resolved local export alias");
        let resolved_span = Span::new(
            u32::try_from(resolved_start).expect("source offset fits u32"),
            u32::try_from(resolved_start + "Resolved".len()).expect("source offset fits u32"),
        );
        let resolved = reservations
            .export_alias_owner(module, resolved_span)
            .expect("resolved local alias owner");
        let chained_start = source
            .find("PublicResolved as Chained")
            .expect("alias-output local export alias");
        let chained_span = Span::new(
            u32::try_from(chained_start).expect("source offset fits u32"),
            u32::try_from(chained_start + "PublicResolved".len()).expect("source offset fits u32"),
        );
        let chained = reservations
            .export_alias_owner(module, chained_span)
            .expect("alias-output local owner");
        let second = reservations
            .export_alias_owner(module, source_span(source, "MissingTwo"))
            .expect("nested local alias owner");
        assert_eq!(reservations.export_aliases.len(), 4);
        assert_eq!(first.ticket.record_ordinal, 0);
        assert_eq!(resolved.ticket.record_ordinal, 0);
        assert_eq!(chained.ticket.record_ordinal, 0);
        assert_eq!(second.ticket.record_ordinal, 0);
        assert_ne!(first.event, resolved.event);
        assert_ne!(resolved.event, chained.event);
        assert_ne!(chained.event, second.event);
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
            ModuleOrdinal::new(0),
            &binder,
            binder.module,
            &parsed.program,
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
                    occurrence.source_start,
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
        assert_ne!(constraints[0].event, constraints[1].event);
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
        assert_ne!(defaults[0].event, defaults[1].event);
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
            module,
            &binder,
            binder.module,
            &parsed.program,
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
                Some(SourceSite {
                    module_ordinal: module,
                    unit_slot: UnitSlot::new(0),
                    source_start: fragment.site.declaration_span.start,
                })
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
            module,
            &binder,
            binder.module,
            &parsed.program,
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
        assert_ne!(first_owner.event, second_owner.event);
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
            Some(SourceSite {
                module_ordinal: module,
                unit_slot: UnitSlot::new(0),
                source_start: first.site.declaration_span.start,
            })
        );
        assert_eq!(
            reservations.declaration_source(second.declaration),
            Some(SourceSite {
                module_ordinal: module,
                unit_slot: UnitSlot::new(0),
                source_start: second.site.declaration_span.start,
            })
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
            module,
            &binder,
            binder.module,
            &parsed.program,
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
            .all(|other| owner.event != other.event && owner.ticket != other.ticket)));

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
            module,
            &binder,
            binder.module,
            &parsed.program,
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
            store.commit(effects.records).unwrap();
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
        let class = reservations.class_at(ModuleOrdinal::new(3), 0).unwrap();
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
            reservations.callable_at(ModuleOrdinal::new(3), function_start),
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
            Err(ReservationError::DuplicateCallableBinding(callable))
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
            .owner_at(module, declarator_start, LexicalOwnerPhase::Immediate)
            .is_some());
        let inner_start = source.find("function inner").unwrap() as u32;
        let signature = reservations
            .owner_at(module, inner_start, LexicalOwnerPhase::Immediate)
            .unwrap();
        let body = reservations
            .owner_at(module, inner_start, LexicalOwnerPhase::Body)
            .unwrap();
        assert_eq!(signature.event, body.event);
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
                .owner_at(module, *start, LexicalOwnerPhase::Immediate)
                .unwrap();
            let body = reservations
                .owner_at(module, *start, LexicalOwnerPhase::Body)
                .unwrap();
            assert_eq!(signature.event, body.event);
            assert_ne!(signature.ticket, body.ticket);
        }
        for name in ["a =", "b =", "c ="] {
            let start = u32::try_from(source.find(name).unwrap()).unwrap();
            assert!(reservations
                .owner_at(module, start, LexicalOwnerPhase::Deferred)
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

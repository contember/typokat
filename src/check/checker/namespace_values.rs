//! Query-free value payloads for namespaces attached to class/function owners.

use super::assignment::declared_from_init;
use super::calls::FunctionReservation;
use super::context::{ClassNamespacePropertyPayload, ClassNamespacePropertySourceOrder, Pass};
use super::function_groups::FunctionNamespacePayload;
use super::lexical_events::LexicalOwnerPhase;
use super::statements::{function_decl_from_statement, function_overload_group};
use crate::binder::declaration::{DeclId, TypeGroupId, ValueStorageId};
use crate::binder::namespace::{
    DeclarationOwner, MergeDeclarationKind, NamespacePublication,
    NamespaceValueAttachmentDisposition,
};
use crate::binder::scope::ScopeId;
use crate::span::Span;
use crate::types::repr::{ObjectType, PropertyType};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    Class, Declaration, Expression, Function, Statement, TSModuleDeclaration,
    TSModuleDeclarationBody, VariableDeclaration, VariableDeclarationKind, VariableDeclarator,
};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Default)]
pub(in crate::check::checker) struct NamespaceValueRegistry {
    prepared: FxHashMap<(ScopeId, u32), PreparedNamespaceMember>,
    consumed_fragments: FxHashSet<(ScopeId, u32)>,
    private_fragments: FxHashSet<(ScopeId, u32)>,
    ambient_fragments: FxHashSet<(ScopeId, u32)>,
    prepared_owners: FxHashSet<(ScopeId, String)>,
}

enum PreparedNamespaceMember {
    Variable {
        scope: ScopeId,
        annotation: Option<TypeId>,
    },
    Function {
        scope: ScopeId,
        reservation: FunctionReservation,
    },
    Class {
        scope: ScopeId,
    },
}

struct AttachmentInput {
    owner_scope: ScopeId,
    name: String,
    class_group: Option<TypeGroupId>,
    disposition: NamespaceValueAttachmentDisposition,
    fragments: Vec<FragmentInput>,
    members: Vec<OwnedMemberInput>,
    private_members: Vec<PrivateMemberInput>,
    unavailable_members: Vec<UnavailableMemberInput>,
    has_unavailable_metadata: bool,
}

#[derive(Copy, Clone)]
struct FragmentInput {
    module: ScopeId,
    source_start: u32,
    ambient: bool,
}

#[derive(Clone)]
struct OwnedMemberInput {
    declaration: DeclId,
    storage: ValueStorageId,
    scope: ScopeId,
    source: crate::binder::namespace::SourceUnitKey,
    source_start: u32,
    span: Span,
    owner_span: Span,
    name: String,
    kind: PreparedNamespaceValueKind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PreparedNamespaceValueKind {
    Variable,
    Function,
    Class,
}

#[derive(Copy, Clone)]
struct UnavailableMemberInput {
    declaration: DeclId,
    span: Span,
    kind: MergeDeclarationKind,
}

#[derive(Copy, Clone)]
struct PrivateMemberInput {
    scope: ScopeId,
    source_start: u32,
    kind: PreparedNamespaceValueKind,
}

struct StagedVariable {
    input: OwnedMemberInput,
    ty: TypeId,
    annotation: Option<TypeId>,
}

struct StagedFunction<'stmt, 'ast> {
    input: OwnedMemberInput,
    syntax: &'stmt Function<'ast>,
    reservation: FunctionReservation,
}

#[derive(Default)]
struct NamespaceSyntaxIndex<'stmt, 'ast> {
    variables: FxHashMap<u32, (VariableDeclarationKind, &'stmt VariableDeclarator<'ast>)>,
    functions: FxHashMap<u32, &'stmt Function<'ast>>,
    classes: FxHashMap<u32, &'stmt Class<'ast>>,
}

impl NamespaceValueRegistry {
    fn is_prepared_owner(&self, scope: ScopeId, name: &str) -> bool {
        self.prepared_owners.contains(&(scope, name.to_owned()))
    }

    fn mark_prepared_owner(&mut self, scope: ScopeId, name: String) {
        assert!(
            self.prepared_owners.insert((scope, name)),
            "attached namespace owner prepared twice"
        );
    }

    fn insert_member(
        &mut self,
        module: ScopeId,
        source_start: u32,
        member: PreparedNamespaceMember,
    ) {
        assert!(
            self.prepared
                .insert((module, source_start), member)
                .is_none(),
            "attached namespace member prepared twice"
        );
    }

    fn take_member(
        &mut self,
        module: ScopeId,
        source_start: u32,
    ) -> Option<PreparedNamespaceMember> {
        self.prepared.remove(&(module, source_start))
    }

    fn function_scope(&self, module: ScopeId, source_start: u32) -> Option<ScopeId> {
        match self.prepared.get(&(module, source_start)) {
            Some(PreparedNamespaceMember::Function { scope, .. }) => Some(*scope),
            Some(
                PreparedNamespaceMember::Variable { .. } | PreparedNamespaceMember::Class { .. },
            )
            | None => None,
        }
    }

    fn take_function(
        &mut self,
        module: ScopeId,
        source_start: u32,
    ) -> Option<(ScopeId, FunctionReservation)> {
        match self.prepared.remove(&(module, source_start)) {
            Some(PreparedNamespaceMember::Function { scope, reservation }) => {
                Some((scope, reservation))
            }
            Some(member @ PreparedNamespaceMember::Variable { .. }) => {
                self.prepared.insert((module, source_start), member);
                None
            }
            Some(member @ PreparedNamespaceMember::Class { .. }) => {
                self.prepared.insert((module, source_start), member);
                None
            }
            None => None,
        }
    }

    fn consume_fragment(&mut self, module: ScopeId, source_start: u32) {
        assert!(
            self.consumed_fragments.insert((module, source_start)),
            "attached namespace fragment consumed twice"
        );
    }

    fn is_consumed_fragment(&self, module: ScopeId, source_start: u32) -> bool {
        self.consumed_fragments.contains(&(module, source_start))
    }

    fn mark_private_fragment(&mut self, module: ScopeId, source_start: u32) {
        self.private_fragments.insert((module, source_start));
    }

    fn take_private_fragment(&mut self, module: ScopeId, source_start: u32) -> bool {
        self.private_fragments.remove(&(module, source_start))
    }

    fn mark_ambient_fragment(&mut self, module: ScopeId, source_start: u32) {
        self.ambient_fragments.insert((module, source_start));
    }

    fn is_ambient_fragment(&self, module: ScopeId, source_start: u32) -> bool {
        self.ambient_fragments.contains(&(module, source_start))
    }
}

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Prepare one module's attached namespace values before class/callable publication.
    pub(in crate::check::checker) fn prepare_attached_namespace_values(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
    ) {
        let mut syntax = NamespaceSyntaxIndex::default();
        index_namespace_statements(statements, false, &mut syntax);
        let attachments = collect_attachment_inputs(self, scope);
        for attachment in attachments {
            if self
                .namespace_values
                .is_prepared_owner(attachment.owner_scope, &attachment.name)
            {
                continue;
            }
            self.namespace_values
                .mark_prepared_owner(attachment.owner_scope, attachment.name.clone());
            self.prepare_namespace_attachment(attachment, &syntax);
        }
    }

    fn prepare_namespace_attachment(
        &mut self,
        attachment: AttachmentInput,
        syntax: &NamespaceSyntaxIndex<'_, '_>,
    ) {
        for fragment in &attachment.fragments {
            if fragment.ambient {
                self.namespace_values
                    .mark_ambient_fragment(fragment.module, fragment.source_start);
            }
        }
        self.prepare_private_namespace_members(&attachment, syntax);
        if !attachment.unavailable_members.is_empty() || attachment.has_unavailable_metadata {
            for member in &attachment.unavailable_members {
                if let Some((id, context)) = namespace_payload_unavailable(member.kind) {
                    self.record_namespace_attachment_unavailable(
                        member.declaration,
                        member.span,
                        id,
                        context,
                    );
                }
            }
            self.install_unavailable_function_payload(&attachment);
            return;
        }

        let mut first_kinds = FxHashMap::default();
        let mut has_duplicates = false;
        for member in &attachment.members {
            let Some(first_kind) = first_kinds.get(&member.name).copied() else {
                first_kinds.insert(member.name.clone(), member.kind);
                continue;
            };
            let Some(duplicate) = duplicate_property_kind(first_kind, member.kind) else {
                continue;
            };
            let (id, context) = namespace_payload_duplicate(duplicate);
            self.record_namespace_member_unavailable(member, id, context);
            has_duplicates = true;
        }
        if has_duplicates {
            self.install_unavailable_function_payload(&attachment);
            return;
        }

        let mut variables = Vec::new();
        let mut functions = Vec::new();
        let mut unavailable = false;
        for member in &attachment.members {
            match member.kind {
                PreparedNamespaceValueKind::Variable => {
                    let Some((kind, declarator)) =
                        syntax.variables.get(&member.source_start).copied()
                    else {
                        // The binder and syntax index share exact declaration starts.
                        unavailable = true;
                        continue;
                    };
                    let annotation = match &declarator.type_annotation {
                        Some(annotation) => self.with_lexical_effects(
                            declarator.span.start,
                            LexicalOwnerPhase::Immediate,
                            |pass| pass.lower_annotation(member.scope, &annotation.type_annotation),
                        ),
                        None => None,
                    };
                    let ty = match (&declarator.type_annotation, annotation) {
                        (Some(_), Some(annotation)) => Some(annotation),
                        (Some(_), None) => {
                            unavailable = true;
                            continue;
                        }
                        (None, _) => declarator.init.as_ref().and_then(|initializer| {
                            query_free_initializer_type(self, kind, initializer)
                        }),
                    };
                    let Some(ty) = ty else {
                        self.record_namespace_member_unavailable(
                            member,
                            "decl/variable-declaration/namespace-payload-inferred-initializer",
                            "namespace member initializer cannot be finalized before owner publication",
                        );
                        unavailable = true;
                        continue;
                    };
                    variables.push(StagedVariable {
                        input: member.clone(),
                        ty,
                        annotation,
                    });
                }
                PreparedNamespaceValueKind::Function => {
                    let Some(function) = syntax.functions.get(&member.source_start).copied() else {
                        // The binder and syntax index share exact declaration starts.
                        unavailable = true;
                        continue;
                    };
                    let reservation = self.reserve_namespace_function(member.scope, function);
                    functions.push(StagedFunction {
                        input: member.clone(),
                        syntax: function,
                        reservation,
                    });
                }
                PreparedNamespaceValueKind::Class => {
                    let span = syntax
                        .classes
                        .get(&member.source_start)
                        .map_or(member_span(member), |class| Span::from_oxc(class.span));
                    self.record_namespace_attachment_unavailable(
                        member.declaration,
                        span,
                        "decl/class-declaration/namespace-payload-static-cycle",
                        "attached namespace class value depends on class publication",
                    );
                    unavailable = true;
                }
            }
        }

        let mut properties = variables
            .iter()
            .map(|variable| PropertyType::public(variable.input.name.clone(), variable.ty))
            .collect::<Vec<_>>();
        let function_properties = self.stage_namespace_function_properties(&functions);
        let Some(function_properties) = function_properties else {
            self.install_unavailable_function_payload(&attachment);
            return;
        };
        properties.extend(function_properties);
        if unavailable {
            self.install_unavailable_function_payload(&attachment);
            return;
        }

        for variable in variables {
            assert!(self.decl_types.get(variable.input.storage).is_none());
            self.decl_types.set(variable.input.storage, variable.ty);
            self.namespace_values.insert_member(
                self.current_module,
                variable.input.source_start,
                PreparedNamespaceMember::Variable {
                    scope: variable.input.scope,
                    annotation: variable.annotation,
                },
            );
        }
        for function in functions {
            let property_ty = properties
                .iter()
                .find(|property| property.name == function.input.name)
                .map(|property| property.ty)
                .expect("prepared namespace function has one public property");
            assert!(self.decl_types.get(function.input.storage).is_none());
            self.decl_types.set(function.input.storage, property_ty);
            self.namespace_values.insert_member(
                self.current_module,
                function.input.source_start,
                PreparedNamespaceMember::Function {
                    scope: function.input.scope,
                    reservation: function.reservation,
                },
            );
        }
        for fragment in &attachment.fragments {
            self.namespace_values
                .consume_fragment(fragment.module, fragment.source_start);
        }
        match attachment.disposition {
            NamespaceValueAttachmentDisposition::AdmittedFunction => {
                let installed = self.install_function_namespace_payload(
                    attachment.owner_scope,
                    &attachment.name,
                    FunctionNamespacePayload::Ready(properties.clone()),
                );
                assert!(installed, "admitted function attachment must install");
            }
            NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42 => {
                let installed = self.install_function_namespace_payload(
                    attachment.owner_scope,
                    &attachment.name,
                    FunctionNamespacePayload::Ready(properties.clone()),
                );
                assert!(installed, "deferred function recovery must install");
            }
            NamespaceValueAttachmentDisposition::AdmittedClass => {
                let group = attachment
                    .class_group
                    .expect("admitted class attachment has a type group");
                let payload = self.class_namespace_property_payload(&attachment, properties);
                assert!(
                    self.install_class_namespace_payload(group, payload),
                    "class namespace payload installed twice"
                );
            }
            NamespaceValueAttachmentDisposition::TypeContainerOnly
            | NamespaceValueAttachmentDisposition::Rejected(_) => {
                unreachable!("collector retains only admitted value attachments")
            }
        }
    }

    fn prepare_private_namespace_members(
        &mut self,
        attachment: &AttachmentInput,
        syntax: &NamespaceSyntaxIndex<'_, '_>,
    ) {
        if !attachment.private_members.is_empty() {
            for fragment in &attachment.fragments {
                self.namespace_values
                    .mark_private_fragment(fragment.module, fragment.source_start);
            }
        }
        for member in &attachment.private_members {
            match member.kind {
                PreparedNamespaceValueKind::Variable => {
                    let Some((_, declarator)) = syntax.variables.get(&member.source_start).copied()
                    else {
                        continue;
                    };
                    let annotation = match &declarator.type_annotation {
                        Some(annotation) => self.with_lexical_effects(
                            declarator.span.start,
                            LexicalOwnerPhase::Immediate,
                            |pass| pass.lower_annotation(member.scope, &annotation.type_annotation),
                        ),
                        None => None,
                    };
                    self.namespace_values.insert_member(
                        self.current_module,
                        member.source_start,
                        PreparedNamespaceMember::Variable {
                            scope: member.scope,
                            annotation,
                        },
                    );
                }
                PreparedNamespaceValueKind::Function => {
                    let Some(function) = syntax.functions.get(&member.source_start).copied() else {
                        continue;
                    };
                    let reservation = self.reserve_namespace_function(member.scope, function);
                    self.namespace_values.insert_member(
                        self.current_module,
                        member.source_start,
                        PreparedNamespaceMember::Function {
                            scope: member.scope,
                            reservation,
                        },
                    );
                }
                PreparedNamespaceValueKind::Class => {
                    if syntax.classes.contains_key(&member.source_start) {
                        self.namespace_values.insert_member(
                            self.current_module,
                            member.source_start,
                            PreparedNamespaceMember::Class {
                                scope: member.scope,
                            },
                        );
                    }
                }
            }
        }
    }

    fn reserve_namespace_function(
        &mut self,
        scope: ScopeId,
        function: &Function<'_>,
    ) -> FunctionReservation {
        let tickets = self
            .lexical_events
            .callable_at(self.current_module_ordinal, function.span.start)
            .and_then(|site| self.lexical_events.callable(site))
            .map(|callable| callable.tickets);
        match tickets {
            Some(tickets) => self.with_ticket_effects(tickets.signature, |pass| {
                pass.reserve_function(scope, function)
            }),
            None => self.reserve_function(scope, function),
        }
    }

    fn class_namespace_property_payload(
        &self,
        attachment: &AttachmentInput,
        properties: Vec<PropertyType>,
    ) -> Vec<ClassNamespacePropertyPayload> {
        properties
            .into_iter()
            .map(|property| {
                let member = attachment
                    .members
                    .iter()
                    .find(|member| member.name == property.name)
                    .expect("published namespace property has one exact declaration");
                let owner = self
                    .lexical_events
                    .declaration_owner(member.declaration)
                    .expect("published namespace property retains its exact owner")
                    .ticket;
                ClassNamespacePropertyPayload {
                    property,
                    declaration: member.declaration,
                    owner_span: member.owner_span,
                    source_order: ClassNamespacePropertySourceOrder {
                        source: member.source,
                        source_start: member.span.start,
                        declaration_ordinal: member.declaration.0,
                    },
                    owner,
                }
            })
            .collect()
    }

    fn stage_namespace_function_properties(
        &mut self,
        functions: &[StagedFunction<'_, '_>],
    ) -> Option<Vec<PropertyType>> {
        let mut order = Vec::new();
        let mut groups: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        for (index, function) in functions.iter().enumerate() {
            if !groups.contains_key(&function.input.name) {
                order.push(function.input.name.clone());
            }
            groups
                .entry(function.input.name.clone())
                .or_default()
                .push(index);
        }
        let mut properties = Vec::new();
        for name in order {
            let indices = groups.get(&name).expect("function group index exists");
            let mut signatures = Vec::new();
            if indices.len() == 1 {
                let function = &functions[indices[0]];
                match &function.reservation {
                    FunctionReservation::Ready(surface)
                        if function.syntax.body.is_none() || surface.declared_return.is_some() =>
                    {
                        signatures.push(surface.function_ty);
                    }
                    FunctionReservation::Ready(_) => {
                        self.record_namespace_member_unavailable(
                            &function.input,
                            "decl/function-declaration/namespace-payload-inferred-return",
                            "namespace member return cannot be inferred before owner publication",
                        );
                        return None;
                    }
                    FunctionReservation::Unavailable(_) => return None,
                }
            } else {
                for index in indices {
                    let function = &functions[*index];
                    if function.syntax.body.is_some() {
                        continue;
                    }
                    match &function.reservation {
                        FunctionReservation::Ready(surface) => signatures.push(surface.function_ty),
                        FunctionReservation::Unavailable(_) => return None,
                    }
                }
                if signatures.is_empty() {
                    return None;
                }
            }
            let ty = if signatures.len() == 1 {
                signatures[0]
            } else {
                self.interner.intern_object(ObjectType {
                    call_signatures: signatures,
                    ..Default::default()
                })
            };
            properties.push(PropertyType::public(name, ty));
        }
        Some(properties)
    }

    fn install_unavailable_function_payload(&mut self, attachment: &AttachmentInput) {
        let expected = match attachment.disposition {
            NamespaceValueAttachmentDisposition::AdmittedFunction => {
                "admitted function attachment must install"
            }
            NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42 => {
                "deferred function recovery must install"
            }
            NamespaceValueAttachmentDisposition::AdmittedClass
            | NamespaceValueAttachmentDisposition::TypeContainerOnly
            | NamespaceValueAttachmentDisposition::Rejected(_) => return,
        };
        let installed = self.install_function_namespace_payload(
            attachment.owner_scope,
            &attachment.name,
            FunctionNamespacePayload::Unavailable { owner: None },
        );
        assert!(installed, "{expected}");
    }

    fn record_namespace_member_unavailable(
        &mut self,
        member: &OwnedMemberInput,
        id: &str,
        context: &str,
    ) {
        self.record_namespace_attachment_unavailable(
            member.declaration,
            member_span(member),
            id,
            context,
        );
    }

    fn record_namespace_attachment_unavailable(
        &mut self,
        declaration: DeclId,
        span: Span,
        id: &str,
        context: &str,
    ) {
        let owner = self
            .lexical_events
            .declaration_owner(declaration)
            .expect("attached namespace source has one exact declaration owner");
        self.with_ticket_effects(owner.ticket, |pass| {
            pass.record_incomplete(id, span, context);
        });
    }

    /// Consume a prepared namespace fragment at its source position.
    pub(in crate::check::checker) fn check_prepared_namespace_declaration(
        &mut self,
        declaration: &TSModuleDeclaration<'_>,
    ) -> bool {
        let consumed = self
            .namespace_values
            .is_consumed_fragment(self.current_module, declaration.span.start);
        let has_private_checks = self
            .namespace_values
            .take_private_fragment(self.current_module, declaration.span.start);
        let ambient = self
            .namespace_values
            .is_ambient_fragment(self.current_module, declaration.span.start);
        if !consumed && !has_private_checks {
            return false;
        }
        self.check_prepared_namespace_body(declaration, ambient);
        consumed
    }

    fn check_prepared_namespace_body(
        &mut self,
        declaration: &TSModuleDeclaration<'_>,
        ambient: bool,
    ) {
        match &declaration.body {
            Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
                self.check_prepared_namespace_statements(&block.body, ambient)
            }
            Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
                self.check_prepared_namespace_body(nested, ambient)
            }
            None => {}
        }
    }

    fn check_prepared_namespace_statements(&mut self, statements: &[Statement<'_>], ambient: bool) {
        let mut index = 0;
        while index < statements.len() {
            if function_decl_from_statement(&statements[index]).is_some() {
                let end =
                    function_overload_group(statements, index).map_or(index + 1, |(_, end)| end);
                if self.check_prepared_namespace_function_group(&statements[index..end], ambient) {
                    index = end;
                    continue;
                }
            }
            let statement = &statements[index];
            match statement {
                Statement::VariableDeclaration(declaration) => {
                    self.check_prepared_namespace_variable(declaration)
                }
                Statement::FunctionDeclaration(function) => {
                    self.check_prepared_namespace_function(function)
                }
                Statement::ClassDeclaration(class) => self.check_class_for_namespace(class),
                Statement::TSModuleDeclaration(nested) => {
                    self.check_prepared_namespace_body(nested, ambient)
                }
                Statement::ExportNamedDeclaration(export) => {
                    if let Some(declaration) = &export.declaration {
                        match declaration {
                            Declaration::VariableDeclaration(declaration) => {
                                self.check_prepared_namespace_variable(declaration)
                            }
                            Declaration::FunctionDeclaration(function) => {
                                self.check_prepared_namespace_function(function)
                            }
                            Declaration::ClassDeclaration(class) => {
                                self.check_class_for_namespace(class)
                            }
                            Declaration::TSModuleDeclaration(nested) => {
                                self.check_prepared_namespace_body(nested, ambient)
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn check_prepared_namespace_function_group(
        &mut self,
        statements: &[Statement<'_>],
        ambient: bool,
    ) -> bool {
        let functions = statements
            .iter()
            .filter_map(function_decl_from_statement)
            .collect::<Vec<_>>();
        let Some(scope) = functions.first().and_then(|function| {
            self.namespace_values
                .function_scope(self.current_module, function.span.start)
        }) else {
            return false;
        };
        if functions.iter().any(|function| {
            self.namespace_values
                .function_scope(self.current_module, function.span.start)
                != Some(scope)
        }) {
            return false;
        }
        let mut surfaces = FxHashMap::default();
        for function in functions {
            let Some((_, reservation)) = self
                .namespace_values
                .take_function(self.current_module, function.span.start)
            else {
                return false;
            };
            surfaces.insert(function.span.start, reservation);
        }
        self.validate_reserved_namespace_function_group(scope, statements, &mut surfaces, ambient)
    }

    fn check_prepared_namespace_variable(&mut self, declaration: &VariableDeclaration<'_>) {
        for declarator in &declaration.declarations {
            let Some(PreparedNamespaceMember::Variable { scope, annotation }) = self
                .namespace_values
                .take_member(self.current_module, declarator.span.start)
            else {
                continue;
            };
            self.with_lexical_effects(declarator.span.start, LexicalOwnerPhase::Deferred, |pass| {
                if let Some(initializer) = &declarator.init {
                    pass.check_annotated_initializer(scope, annotation, initializer);
                }
            });
        }
    }

    fn check_prepared_namespace_function(&mut self, function: &Function<'_>) {
        let Some(PreparedNamespaceMember::Function { scope, reservation }) = self
            .namespace_values
            .take_member(self.current_module, function.span.start)
        else {
            return;
        };
        match reservation {
            FunctionReservation::Ready(surface) => {
                self.fill_reserved_function(scope, function, &surface);
            }
            FunctionReservation::Unavailable(surface) => {
                self.check_retained_function_body(scope, function, &surface);
            }
        }
    }

    fn check_class_for_namespace(&mut self, class: &Class<'_>) {
        let Some(PreparedNamespaceMember::Class { scope }) = self
            .namespace_values
            .take_member(self.current_module, class.span.start)
        else {
            return;
        };
        self.check_class(scope, class);
    }
}

fn collect_attachment_inputs(pass: &Pass<'_, '_>, module: ScopeId) -> Vec<AttachmentInput> {
    let mut inputs = Vec::new();
    for record in pass.binder.namespaces.merges() {
        let Some(owner_scope) = declaration_owner_scope(pass, record.owner) else {
            continue;
        };
        let Some(attachment) = pass
            .binder
            .namespace_value_attachment(owner_scope, &record.name)
        else {
            continue;
        };
        let exposes_value_attachment = match attachment.disposition {
            NamespaceValueAttachmentDisposition::AdmittedFunction => true,
            NamespaceValueAttachmentDisposition::AdmittedClass => true,
            NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42 => true,
            NamespaceValueAttachmentDisposition::TypeContainerOnly
            | NamespaceValueAttachmentDisposition::Rejected(_) => false,
        };
        if !exposes_value_attachment
            || !attachment
                .fragments
                .iter()
                .any(|fragment| fragment.module == module)
        {
            continue;
        }
        let value_member_count = attachment
            .fragments
            .iter()
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| pass.binder.namespaces.member(*member))
            .filter(|member| {
                namespace_member_participates_in_payload(member.spaces.value, member.publication)
            })
            .count();
        let mut private_members = attachment
            .fragments
            .iter()
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| pass.binder.namespaces.member(*member))
            .filter(|member| {
                member.spaces.value
                    && matches!(member.publication, NamespacePublication::Private)
                    && matches!(
                        member.kind,
                        MergeDeclarationKind::Variable
                            | MergeDeclarationKind::Function
                            | MergeDeclarationKind::Class
                    )
            })
            .filter_map(|member| {
                let declaration = member.declaration?;
                let site = pass.binder.declarations.get(declaration)?.site;
                Some((
                    declaration,
                    PrivateMemberInput {
                        scope: site.scope?,
                        source_start: site.declaration_span.start,
                        kind: prepared_namespace_value_kind(member.kind)?,
                    },
                ))
            })
            .collect::<Vec<_>>();
        private_members.sort_by_key(|(declaration, member)| (member.source_start, declaration.0));
        private_members.dedup_by_key(|(declaration, _)| *declaration);
        let private_members = private_members
            .into_iter()
            .map(|(_, member)| member)
            .collect::<Vec<_>>();
        let mut members = Vec::new();
        let mut unavailable_members = Vec::new();
        for member in &attachment.members {
            let Some(storage) = member.value_storage else {
                unavailable_members.push(UnavailableMemberInput {
                    declaration: member.declaration,
                    span: member.site.declaration_span,
                    kind: member.kind,
                });
                continue;
            };
            let Some(kind) = prepared_namespace_value_kind(member.kind) else {
                unavailable_members.push(UnavailableMemberInput {
                    declaration: member.declaration,
                    span: member.site.declaration_span,
                    kind: member.kind,
                });
                continue;
            };
            members.push(OwnedMemberInput {
                declaration: member.declaration,
                storage,
                scope: member.scope,
                source: member.source,
                source_start: member.site.declaration_span.start,
                span: member.site.declaration_span,
                owner_span: member.site.binding_span,
                name: member.name.to_owned(),
                kind,
            });
        }
        let class_group = pass
            .binder
            .symbols
            .get(attachment.symbol)
            .and_then(|symbol| symbol.ty);
        let has_unavailable_metadata = value_member_count != attachment.members.len();
        inputs.push(AttachmentInput {
            owner_scope,
            name: attachment.name.to_owned(),
            class_group,
            disposition: attachment.disposition,
            fragments: attachment
                .fragments
                .iter()
                .map(|fragment| FragmentInput {
                    module: fragment.module,
                    source_start: fragment.source_start,
                    ambient: fragment.ambient,
                })
                .collect(),
            private_members,
            unavailable_members,
            has_unavailable_metadata,
            members,
        });
    }
    inputs
}

fn prepared_namespace_value_kind(kind: MergeDeclarationKind) -> Option<PreparedNamespaceValueKind> {
    match kind {
        MergeDeclarationKind::Variable => Some(PreparedNamespaceValueKind::Variable),
        MergeDeclarationKind::Function => Some(PreparedNamespaceValueKind::Function),
        MergeDeclarationKind::Class => Some(PreparedNamespaceValueKind::Class),
        MergeDeclarationKind::TypeAlias
        | MergeDeclarationKind::Interface
        | MergeDeclarationKind::Enum
        | MergeDeclarationKind::Namespace
        | MergeDeclarationKind::ImportAlias
        | MergeDeclarationKind::DeferredExport => None,
    }
}

fn namespace_payload_unavailable(
    kind: MergeDeclarationKind,
) -> Option<(&'static str, &'static str)> {
    match kind {
        MergeDeclarationKind::Enum => Some((
            "decl/enum-declaration/namespace-payload-unavailable",
            "attached namespace enum value is not modeled",
        )),
        MergeDeclarationKind::ImportAlias => Some((
            "decl/import-equals/namespace-payload-unavailable",
            "attached namespace import-equals value is not modeled",
        )),
        MergeDeclarationKind::Variable
        | MergeDeclarationKind::Function
        | MergeDeclarationKind::Class
        | MergeDeclarationKind::TypeAlias
        | MergeDeclarationKind::Interface
        | MergeDeclarationKind::Namespace
        | MergeDeclarationKind::DeferredExport => None,
    }
}

fn duplicate_property_kind(
    first: PreparedNamespaceValueKind,
    later: PreparedNamespaceValueKind,
) -> Option<PreparedNamespaceValueKind> {
    match (first, later) {
        (PreparedNamespaceValueKind::Function, PreparedNamespaceValueKind::Function) => None,
        (_, later) => Some(later),
    }
}

fn namespace_payload_duplicate(kind: PreparedNamespaceValueKind) -> (&'static str, &'static str) {
    match kind {
        PreparedNamespaceValueKind::Variable => (
            "decl/variable-declaration/namespace-payload-duplicate-value",
            "namespace variable duplicates an earlier exported value",
        ),
        PreparedNamespaceValueKind::Function => (
            "decl/function-declaration/namespace-payload-duplicate-value",
            "namespace function duplicates an earlier exported value",
        ),
        PreparedNamespaceValueKind::Class => (
            "decl/class-declaration/namespace-payload-duplicate-value",
            "namespace class duplicates an earlier exported value",
        ),
    }
}

fn namespace_member_participates_in_payload(
    has_value_space: bool,
    publication: NamespacePublication,
) -> bool {
    has_value_space && !matches!(publication, NamespacePublication::Private)
}

fn declaration_owner_scope(pass: &Pass<'_, '_>, owner: DeclarationOwner) -> Option<ScopeId> {
    match owner {
        DeclarationOwner::Lexical(scope) => Some(scope),
        DeclarationOwner::NamespacePublic(namespace) => pass
            .binder
            .namespaces
            .get(namespace)
            .map(|namespace| namespace.public_scope),
        DeclarationOwner::NamespacePrivate(fragment) => pass
            .binder
            .namespaces
            .fragment(fragment)
            .map(|fragment| fragment.private_scope),
        DeclarationOwner::CompilationGlobal => Some(pass.binder.compilation_global),
        DeclarationOwner::DeferredAmbientModule(_) => None,
    }
}

fn query_free_initializer_type(
    pass: &mut Pass<'_, '_>,
    kind: VariableDeclarationKind,
    initializer: &Expression<'_>,
) -> Option<TypeId> {
    let ty = match initializer {
        Expression::StringLiteral(literal) => {
            pass.interner
                .intern_literal(crate::types::repr::LiteralValue::String(
                    literal.value.to_string(),
                ))
        }
        Expression::NumericLiteral(literal) => pass
            .interner
            .intern_literal(crate::types::repr::LiteralValue::Number(literal.value)),
        Expression::BooleanLiteral(literal) => pass
            .interner
            .intern_literal(crate::types::repr::LiteralValue::Boolean(literal.value)),
        Expression::NullLiteral(_) => pass.interner.well_known().null,
        Expression::ParenthesizedExpression(parenthesized) => {
            return query_free_initializer_type(pass, kind, &parenthesized.expression)
        }
        _ => return None,
    };
    Some(declared_from_init(pass.interner, kind, ty))
}

fn member_span(member: &OwnedMemberInput) -> Span {
    member.span
}

fn index_namespace_statements<'stmt, 'ast>(
    statements: &'stmt [Statement<'ast>],
    inside_namespace: bool,
    index: &mut NamespaceSyntaxIndex<'stmt, 'ast>,
) {
    for statement in statements {
        match statement {
            Statement::TSModuleDeclaration(declaration) => index_namespace_body(declaration, index),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    index_namespace_declaration(declaration, inside_namespace, index);
                }
            }
            Statement::VariableDeclaration(declaration) if inside_namespace => {
                index_namespace_variable(declaration, index)
            }
            Statement::FunctionDeclaration(function) if inside_namespace => {
                index.functions.insert(function.span.start, function);
            }
            Statement::ClassDeclaration(class) if inside_namespace => {
                index.classes.insert(class.span.start, class);
            }
            _ => {}
        }
    }
}

fn index_namespace_declaration<'stmt, 'ast>(
    declaration: &'stmt Declaration<'ast>,
    inside_namespace: bool,
    index: &mut NamespaceSyntaxIndex<'stmt, 'ast>,
) {
    match declaration {
        Declaration::TSModuleDeclaration(declaration) => index_namespace_body(declaration, index),
        Declaration::VariableDeclaration(declaration) if inside_namespace => {
            index_namespace_variable(declaration, index)
        }
        Declaration::FunctionDeclaration(function) if inside_namespace => {
            index.functions.insert(function.span.start, function);
        }
        Declaration::ClassDeclaration(class) if inside_namespace => {
            index.classes.insert(class.span.start, class);
        }
        _ => {}
    }
}

fn index_namespace_variable<'stmt, 'ast>(
    declaration: &'stmt VariableDeclaration<'ast>,
    index: &mut NamespaceSyntaxIndex<'stmt, 'ast>,
) {
    for declarator in &declaration.declarations {
        index
            .variables
            .insert(declarator.span.start, (declaration.kind, declarator));
    }
}

fn index_namespace_body<'stmt, 'ast>(
    declaration: &'stmt TSModuleDeclaration<'ast>,
    index: &mut NamespaceSyntaxIndex<'stmt, 'ast>,
) {
    match &declaration.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            index_namespace_statements(&block.body, true, index)
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
            index_namespace_body(nested, index)
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        duplicate_property_kind, namespace_member_participates_in_payload,
        namespace_payload_duplicate, namespace_payload_unavailable, prepared_namespace_value_kind,
        MergeDeclarationKind, NamespacePublication, PreparedNamespaceValueKind,
    };
    use crate::driver::check_source;

    #[test]
    fn private_values_do_not_participate_in_owner_payloads() {
        assert!(!namespace_member_participates_in_payload(
            true,
            NamespacePublication::Private
        ));
        assert!(namespace_member_participates_in_payload(
            true,
            NamespacePublication::Explicit
        ));
        assert!(!namespace_member_participates_in_payload(
            false,
            NamespacePublication::Explicit
        ));
    }

    #[test]
    fn private_classes_check_bodies_without_joining_function_or_class_owner_payloads() {
        for owner in ["function Owner(): void {}", "class Owner {}"] {
            let source = format!(
                "{owner}\n\
                 namespace Owner {{\n\
                   export const tag: string = \"tag\";\n\
                   class Hidden {{\n\
                     field: number = \"bad\";\n\
                     method(): number {{ return \"bad\"; }}\n\
                   }}\n\
                 }}\n\
                 const tag: string = Owner.tag;\n\
                 const wrong: number = Owner.tag;\n\
                 Owner.Hidden;\n"
            );
            let output = check_source(&source);
            assert!(output.parse_errors.is_empty(), "{owner}");
            assert!(output.incomplete.is_empty(), "{owner}");
            let mut codes = output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>();
            codes.sort_unstable();
            assert_eq!(codes, ["TK2322", "TK2322", "TK2322", "TK2339"], "{owner}");
        }
    }

    #[test]
    fn ambient_namespace_signatures_are_declaration_only_but_nonambient_signatures_are_not() {
        let source = r#"
declare function AmbientOwner(): void;
declare namespace AmbientOwner {
  function g(value: number): number;
  function g(value: string): string;
}

function NonAmbientOwner(): void {}
namespace NonAmbientOwner {
  export function missing(value: number): number;
}
"#;
        let output = check_source(source);
        assert!(output.parse_errors.is_empty());
        assert!(output.incomplete.is_empty());
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2391"]
        );
    }

    #[test]
    fn unsupported_payload_kinds_keep_exact_incomplete_ids() {
        assert_eq!(
            namespace_payload_unavailable(MergeDeclarationKind::Enum)
                .expect("enum has one exact incomplete kind")
                .0,
            "decl/enum-declaration/namespace-payload-unavailable"
        );
        assert_eq!(
            namespace_payload_unavailable(MergeDeclarationKind::ImportAlias)
                .expect("import alias has one exact incomplete kind")
                .0,
            "decl/import-equals/namespace-payload-unavailable"
        );
        assert_eq!(
            prepared_namespace_value_kind(MergeDeclarationKind::Variable),
            Some(PreparedNamespaceValueKind::Variable)
        );
        assert_eq!(
            prepared_namespace_value_kind(MergeDeclarationKind::Enum),
            None
        );
        assert_eq!(
            namespace_payload_unavailable(MergeDeclarationKind::Variable),
            None
        );
    }

    #[test]
    fn duplicate_payload_properties_allow_only_function_overloads() {
        assert_eq!(
            duplicate_property_kind(
                PreparedNamespaceValueKind::Function,
                PreparedNamespaceValueKind::Function
            ),
            None
        );
        assert_eq!(
            duplicate_property_kind(
                PreparedNamespaceValueKind::Variable,
                PreparedNamespaceValueKind::Variable
            ),
            Some(PreparedNamespaceValueKind::Variable)
        );
        assert_eq!(
            duplicate_property_kind(
                PreparedNamespaceValueKind::Variable,
                PreparedNamespaceValueKind::Function
            ),
            Some(PreparedNamespaceValueKind::Function)
        );
        assert_eq!(
            namespace_payload_duplicate(PreparedNamespaceValueKind::Function).0,
            "decl/function-declaration/namespace-payload-duplicate-value"
        );
        assert_eq!(
            namespace_payload_duplicate(PreparedNamespaceValueKind::Class).0,
            "decl/class-declaration/namespace-payload-duplicate-value"
        );
    }
}

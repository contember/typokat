use super::deferred::*;
use super::metadata::WalkContext;
use super::models_ids::*;
use crate::binder::bind::{declare_type, BindState, FrozenPrefixWriteSite};
use crate::binder::declaration::{DeclId, DeclarationKind, TypeFragmentKind};
use crate::binder::scope::{Scope, ScopeId, ScopeKind};
use crate::binder::symbol::{Symbol, SymbolId};
use crate::span::Span;
use oxc_ast::ast::{
    Declaration, ImportDeclarationSpecifier, ImportOrExportKind, Statement, TSModuleDeclaration,
    TSModuleDeclarationBody, TSModuleDeclarationName, TSModuleReference, VariableDeclarationKind,
};

pub(super) fn walk_statements(
    state: &mut BindState,
    statements: &[Statement<'_>],
    context: WalkContext,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    for statement in statements {
        walk_statement(state, statement, context, false, unit, compilation_global);
    }
}

pub(super) fn walk_statement(
    state: &mut BindState,
    statement: &Statement<'_>,
    context: WalkContext,
    explicit: bool,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    if let Statement::ExportNamedDeclaration(export) = statement {
        let export_context = push_export_context(
            state,
            context,
            if export.declaration.is_some() {
                ExportContextKind::WrappedDeclaration
            } else {
                ExportContextKind::NamedList
            },
            export.source.is_some(),
            Span::from_oxc(export.span),
            unit,
        );
        let export_walk = WalkContext {
            active_export_context: export_context,
            ..context
        };
        if let Some(declaration) = &export.declaration {
            walk_declaration(
                state,
                declaration,
                export_walk,
                true,
                unit,
                compilation_global,
            );
        } else if context.member_owner().is_some() {
            for specifier in &export.specifiers {
                push_export_alias_member(state, export_walk, export, specifier, unit);
            }
        }
        return;
    }
    if let Some(declaration) = statement.as_declaration() {
        walk_declaration(
            state,
            declaration,
            context,
            explicit,
            unit,
            compilation_global,
        );
        return;
    }
    match statement {
        Statement::ExportAllDeclaration(export) => {
            let context = context_with_export(
                state,
                context,
                ExportContextKind::ExportAll,
                true,
                Span::from_oxc(export.span),
                unit,
            );
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit)
        }
        Statement::ExportDefaultDeclaration(export) => {
            let context = context_with_export(
                state,
                context,
                ExportContextKind::ExportDefault,
                false,
                Span::from_oxc(export.span),
                unit,
            );
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit)
        }
        Statement::TSExportAssignment(export) => {
            let context = context_with_export(
                state,
                context,
                ExportContextKind::ExportAssignment,
                false,
                Span::from_oxc(export.span),
                unit,
            );
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit)
        }
        Statement::ImportDeclaration(import) => {
            if let Some(specifiers) = &import.specifiers {
                for specifier in specifiers {
                    let local = specifier.local();
                    let specifier_type_only = matches!(
                        specifier,
                        ImportDeclarationSpecifier::ImportSpecifier(named)
                            if named.import_kind == ImportOrExportKind::Type
                    );
                    let outer_type_only = import.import_kind == ImportOrExportKind::Type;
                    let form = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(_) => ImportBindingForm::Named,
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {
                            ImportBindingForm::Default
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => {
                            ImportBindingForm::Namespace
                        }
                    };
                    let spaces = if outer_type_only || specifier_type_only {
                        DeclarationSpaces::TYPE
                    } else {
                        DeclarationSpaces::ALIAS
                    };
                    bind_named_declaration_with_syntax(
                        state,
                        context,
                        explicit,
                        local.name.as_str(),
                        local.span.start,
                        DeclarationKind::Import,
                        MergeDeclarationKind::ImportAlias,
                        spaces,
                        context.ambient,
                        unit,
                        DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
                            form,
                            outer_type_only,
                            specifier_type_only,
                            external_reference: true,
                            exported: false,
                        }),
                    );
                }
            }
        }
        Statement::TSNamespaceExportDeclaration(export) => {
            let declaration = if context.deferred_module.is_some() {
                state
                    .source_decl_at(export.id.span.start, DeclarationKind::NamespaceExport)
                    .expect("namespace export source declaration exists")
            } else {
                state.attach_declaration_scope(
                    export.id.span.start,
                    DeclarationKind::NamespaceExport,
                    context.lexical_scope,
                )
            };
            let umd_context = if !context.direct_top_level {
                UmdContext::FutureTk1316Nested
            } else if !unit.binding.external_module {
                UmdContext::FutureTk1314NonExternal
            } else if !unit.binding.declaration_file() {
                UmdContext::FutureTk1315Implementation
            } else {
                UmdContext::DeferredValidBacklog15
            };
            state.namespaces.umd_exports.push_local(UmdNamespaceExport {
                declaration,
                source: unit.source,
                origin: unit.origin,
                module: state.current_module,
                owner: context.owner,
                name: export.id.name.to_string(),
                span: Span::from_oxc(export.span),
                context: umd_context,
                export_assignment_target: None,
            });
        }
        Statement::BlockStatement(_)
        | Statement::IfStatement(_)
        | Statement::SwitchStatement(_)
        | Statement::WhileStatement(_)
        | Statement::LabeledStatement(_)
        | Statement::ForStatement(_)
        | Statement::ForInStatement(_)
        | Statement::ForOfStatement(_)
        | Statement::DoWhileStatement(_)
        | Statement::ThrowStatement(_)
        | Statement::TryStatement(_)
        | Statement::ExpressionStatement(_)
            if context.namespace.is_some() =>
        {
            crate::binder::bind::bind_statement(state, context.lexical_scope, statement)
        }
        _ => {}
    }
}

fn walk_declaration(
    state: &mut BindState,
    declaration: &Declaration<'_>,
    context: WalkContext,
    explicit: bool,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            bind_variable(state, declaration, context, explicit, unit)
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                bind_named_declaration(
                    state,
                    context,
                    explicit,
                    identifier.name.as_str(),
                    identifier.span.start,
                    DeclarationKind::Function,
                    MergeDeclarationKind::Function,
                    DeclarationSpaces::VALUE,
                    function.declare,
                    unit,
                );
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                bind_named_declaration(
                    state,
                    context,
                    explicit,
                    identifier.name.as_str(),
                    identifier.span.start,
                    DeclarationKind::Class,
                    MergeDeclarationKind::Class,
                    DeclarationSpaces::VALUE_TYPE,
                    class.declare,
                    unit,
                );
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => bind_named_declaration(
            state,
            context,
            explicit,
            alias.id.name.as_str(),
            alias.id.span.start,
            DeclarationKind::TypeAlias,
            MergeDeclarationKind::TypeAlias,
            DeclarationSpaces::TYPE,
            alias.declare,
            unit,
        ),
        Declaration::TSInterfaceDeclaration(interface) => bind_named_declaration(
            state,
            context,
            explicit,
            interface.id.name.as_str(),
            interface.id.span.start,
            DeclarationKind::Interface,
            MergeDeclarationKind::Interface,
            DeclarationSpaces::TYPE,
            interface.declare,
            unit,
        ),
        Declaration::TSEnumDeclaration(enumeration) => bind_named_declaration_with_syntax(
            state,
            context,
            explicit,
            enumeration.id.name.as_str(),
            enumeration.id.span.start,
            DeclarationKind::Enum,
            MergeDeclarationKind::Enum,
            DeclarationSpaces::VALUE_TYPE,
            enumeration.declare,
            unit,
            DeclarationSyntaxFacts::Enum {
                constant: enumeration.r#const,
            },
        ),
        Declaration::TSModuleDeclaration(namespace) => bind_module_declaration(
            state,
            namespace,
            context,
            explicit,
            false,
            unit,
            compilation_global,
        ),
        Declaration::TSGlobalDeclaration(global) => {
            bind_global(state, global, context, unit, compilation_global)
        }
        Declaration::TSImportEqualsDeclaration(import) => {
            bind_named_declaration_with_syntax(
                state,
                context,
                explicit,
                import.id.name.as_str(),
                import.id.span.start,
                DeclarationKind::ImportEquals,
                MergeDeclarationKind::ImportAlias,
                if import.import_kind == ImportOrExportKind::Type {
                    DeclarationSpaces::TYPE
                } else {
                    DeclarationSpaces::ALIAS
                },
                context.ambient,
                unit,
                DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
                    form: ImportBindingForm::ImportEquals,
                    outer_type_only: import.import_kind == ImportOrExportKind::Type,
                    specifier_type_only: false,
                    external_reference: matches!(
                        import.module_reference,
                        TSModuleReference::ExternalModuleReference(_)
                    ),
                    exported: false,
                }),
            );
        }
    }
}

fn bind_variable(
    state: &mut BindState,
    declaration: &oxc_ast::ast::VariableDeclaration<'_>,
    context: WalkContext,
    explicit: bool,
    unit: CompilationUnit,
) {
    let kind = match declaration.kind {
        VariableDeclarationKind::Var => VariableKind::Var,
        VariableDeclarationKind::Let => VariableKind::Let,
        VariableDeclarationKind::Const => VariableKind::Const,
        VariableDeclarationKind::Using => VariableKind::Using,
        VariableDeclarationKind::AwaitUsing => VariableKind::AwaitUsing,
    };
    for declarator in &declaration.declarations {
        for identifier in declarator.id.get_binding_identifiers() {
            bind_named_declaration_with_syntax(
                state,
                context,
                explicit,
                identifier.name.as_str(),
                identifier.span.start,
                DeclarationKind::Variable,
                MergeDeclarationKind::Variable,
                DeclarationSpaces::VALUE,
                declaration.declare,
                unit,
                DeclarationSyntaxFacts::Variable(kind),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn bind_named_declaration(
    state: &mut BindState,
    context: WalkContext,
    explicit: bool,
    name: &str,
    binding_start: u32,
    declaration_kind: DeclarationKind,
    merge_kind: MergeDeclarationKind,
    spaces: DeclarationSpaces,
    declared_ambient: bool,
    unit: CompilationUnit,
) {
    bind_named_declaration_with_syntax(
        state,
        context,
        explicit,
        name,
        binding_start,
        declaration_kind,
        merge_kind,
        spaces,
        declared_ambient,
        unit,
        DeclarationSyntaxFacts::None,
    );
}

#[allow(clippy::too_many_arguments)]
fn bind_named_declaration_with_syntax(
    state: &mut BindState,
    context: WalkContext,
    explicit: bool,
    name: &str,
    binding_start: u32,
    declaration_kind: DeclarationKind,
    merge_kind: MergeDeclarationKind,
    spaces: DeclarationSpaces,
    declared_ambient: bool,
    unit: CompilationUnit,
    syntax: DeclarationSyntaxFacts,
) {
    let continuation_global = state.continuation_compilation_global_for(binding_start);
    let declaration =
        state.attach_declaration_scope(binding_start, declaration_kind, context.lexical_scope);
    if merge_kind == MergeDeclarationKind::Enum && context.namespace.is_some() {
        state.ensure_value_storage(declaration);
    }
    let publication = context.publication(explicit);
    let owner = continuation_global.map_or_else(
        || context.declaration_owner(publication),
        |_| DeclarationOwner::CompilationGlobal,
    );
    let ambient = context.ambient || declared_ambient;
    let syntax = match syntax {
        DeclarationSyntaxFacts::Import(mut facts) => {
            facts.exported = publication != NamespacePublication::Private;
            DeclarationSyntaxFacts::Import(facts)
        }
        syntax => syntax,
    };
    push_placement(
        state,
        owner,
        name,
        declaration,
        merge_kind,
        spaces,
        ambient,
        unit,
        syntax,
    );
    let legal_global_type = context.global.is_some()
        && owner == DeclarationOwner::CompilationGlobal
        && matches!(
            merge_kind,
            MergeDeclarationKind::TypeAlias
                | MergeDeclarationKind::Interface
                | MergeDeclarationKind::Class
        );
    let already_bound_type = state.namespaces.uses_library_shared_globals()
        && state
            .declarations
            .get(declaration)
            .is_some_and(|row| row.type_group.is_some());
    if (context.namespace.is_some() || legal_global_type) && !already_bound_type {
        let fragment_kind = match merge_kind {
            MergeDeclarationKind::TypeAlias => Some(TypeFragmentKind::TypeAlias),
            MergeDeclarationKind::Interface => Some(TypeFragmentKind::Interface),
            MergeDeclarationKind::Class => Some(TypeFragmentKind::Class),
            MergeDeclarationKind::Enum => Some(TypeFragmentKind::Enum),
            _ => None,
        };
        if let (Some(fragment_kind), Some(scope)) =
            (fragment_kind, declaration_owner_scope(state, owner))
        {
            declare_type(state, scope, name, declaration, fragment_kind, unit.source);
        }
    }
    if context.member_owner().is_some() {
        let site = state
            .declarations
            .get(declaration)
            .expect("metadata declaration exists")
            .site;
        push_member(
            state,
            context,
            owner,
            Some(declaration),
            Some(name.to_string()),
            site.declaration_span,
            site.binding_span,
            spaces,
            merge_kind,
            publication,
            unit,
        );
    }
}

fn declaration_owner_scope(state: &BindState, owner: DeclarationOwner) -> Option<ScopeId> {
    match owner {
        DeclarationOwner::Lexical(scope) => Some(scope),
        DeclarationOwner::NamespacePublic(namespace) => state
            .namespaces
            .get(namespace)
            .map(|namespace| namespace.public_scope),
        DeclarationOwner::NamespacePrivate(fragment) => state
            .namespaces
            .fragment(fragment)
            .map(|fragment| fragment.private_scope),
        DeclarationOwner::CompilationGlobal => state.namespaces.compilation_global,
        DeclarationOwner::DeferredAmbientModule(_) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn bind_module_declaration(
    state: &mut BindState,
    declaration: &TSModuleDeclaration<'_>,
    context: WalkContext,
    explicit: bool,
    dotted: bool,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    let TSModuleDeclarationName::Identifier(identifier) = &declaration.id else {
        bind_deferred_module(state, declaration, context, unit, compilation_global);
        return;
    };
    let publication = if dotted {
        NamespacePublication::DottedImplicit
    } else {
        context.publication(explicit)
    };
    let Some(mut owner) = context.namespace_owner(publication) else {
        return;
    };
    let continuation_global = state.continuation_compilation_global_for(identifier.span.start);
    if continuation_global.is_some() {
        owner = NamespaceOwner::CompilationGlobal;
    }
    if context.direct_top_level
        && !unit.binding.external_module
        && continuation_global.is_none()
        && !state.continuation_active()
        && matches!(owner, NamespaceOwner::Lexical(scope) if scope == context.lexical_scope)
    {
        let occupied_local = state
            .graph
            .get(context.lexical_scope)
            .and_then(|scope| scope.lookup_local(identifier.name.as_str()))
            .and_then(|symbol| state.symbols.get(symbol))
            .is_some_and(|symbol| {
                symbol.value.is_some()
                    || symbol.declarations.iter().any(|declaration| {
                        state
                            .declarations
                            .get(*declaration)
                            .is_some_and(|declaration| declaration.kind == DeclarationKind::Enum)
                    })
            });
        if !occupied_local {
            owner = NamespaceOwner::Lexical(
                state
                    .namespaces
                    .script_namespace_root
                    .expect("script namespace root scope allocated"),
            );
        }
    }
    let declaration_id = state.attach_declaration_scope(
        identifier.span.start,
        DeclarationKind::Namespace,
        context.lexical_scope,
    );
    let namespace = namespace_for(state, owner, identifier.name.as_str());
    state
        .declarations
        .get_mut(declaration_id)
        .expect("namespace declaration exists")
        .namespace = Some(namespace);
    attach_namespace_symbol(state, namespace, declaration_id);
    let public_scope = state
        .namespaces
        .get(namespace)
        .expect("namespace exists")
        .public_scope;
    let private_scope = state.graph.push(
        Scope::new(ScopeKind::NamespacePrivate, Some(context.lexical_scope))
            .with_namespace_public(public_scope),
    );
    let fragment = NamespaceFragmentId(
        u32::try_from(state.namespaces.fragments.len()).expect("namespace fragment count fits u32"),
    );
    state.namespaces.fragments.push_local(NamespaceFragment {
        id: fragment,
        namespace,
        declaration: declaration_id,
        source: unit.source,
        origin: unit.origin,
        source_start: declaration.span.start,
        module: state.current_module,
        private_scope,
        lexical_parent: context.lexical_scope,
        public_scope,
        ambient: context.ambient || declaration.declare || unit.binding.declaration_file(),
        publication,
        instance_state: NamespaceInstanceState::NonInstantiated,
        members: Vec::new(),
    });
    // A namespace row inside the frozen prefix can never take another fragment (ADR-0011); refuse
    // the merge rather than panic, so the user's declaration stops being a crash (backlog 103).
    match state.namespaces.namespaces.get_mut_local(namespace.index()) {
        Some(row) => row.fragments.push(fragment),
        None => state.record_frozen_prefix_write(
            Some(declaration_id),
            FrozenPrefixWriteSite::NamespaceFragmentList,
        ),
    }

    let placement_owner = match owner {
        NamespaceOwner::Lexical(scope) => DeclarationOwner::Lexical(scope),
        NamespaceOwner::NamespacePublic(namespace) => DeclarationOwner::NamespacePublic(namespace),
        NamespaceOwner::FragmentPrivate(fragment) => DeclarationOwner::NamespacePrivate(fragment),
        NamespaceOwner::CompilationGlobal => DeclarationOwner::CompilationGlobal,
    };
    if let Some(participant) = push_placement(
        state,
        placement_owner,
        identifier.name.as_str(),
        declaration_id,
        MergeDeclarationKind::Namespace,
        DeclarationSpaces::NAMESPACE,
        context.ambient || declaration.declare || unit.binding.declaration_file(),
        unit,
        DeclarationSyntaxFacts::None,
    ) {
        participant.namespace_fragment = Some(fragment);
    }
    if context.member_owner().is_some() {
        let site = state
            .declarations
            .get(declaration_id)
            .expect("namespace declaration exists")
            .site;
        push_member(
            state,
            context,
            placement_owner,
            Some(declaration_id),
            Some(identifier.name.to_string()),
            site.declaration_span,
            site.binding_span,
            DeclarationSpaces::NAMESPACE,
            MergeDeclarationKind::Namespace,
            publication,
            unit,
        );
    }

    let nested_context = WalkContext {
        owner: DeclarationOwner::NamespacePrivate(fragment),
        lexical_scope: private_scope,
        namespace: Some((namespace, fragment)),
        global: None,
        deferred_module: None,
        ambient: context.ambient || declaration.declare || unit.binding.declaration_file(),
        ambient_export_list_mode: matches!(
            &declaration.body,
            Some(TSModuleDeclarationBody::TSModuleBlock(block))
                if (context.ambient || declaration.declare || unit.binding.declaration_file())
                    && block.body.iter().any(is_export_list_statement)
        ),
        active_export_context: None,
        direct_top_level: false,
    };
    match &declaration.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            reserve_member_headers(state, &block.body, nested_context);
            walk_statements(state, &block.body, nested_context, unit, compilation_global);
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => bind_module_declaration(
            state,
            nested,
            nested_context,
            true,
            true,
            unit,
            compilation_global,
        ),
        None => {}
    }
}

pub(super) fn is_export_list_statement(statement: &Statement<'_>) -> bool {
    matches!(
        statement,
        Statement::ExportNamedDeclaration(export) if export.declaration.is_none()
    )
}

pub(super) fn context_with_export(
    state: &mut BindState,
    context: WalkContext,
    kind: ExportContextKind,
    has_module_specifier: bool,
    span: Span,
    unit: CompilationUnit,
) -> WalkContext {
    WalkContext {
        active_export_context: push_export_context(
            state,
            context,
            kind,
            has_module_specifier,
            span,
            unit,
        ),
        ..context
    }
}

pub(super) fn push_export_context(
    state: &mut BindState,
    context: WalkContext,
    kind: ExportContextKind,
    has_module_specifier: bool,
    span: Span,
    unit: CompilationUnit,
) -> Option<ExportContextId> {
    let owner = if let Some((_, fragment)) = context.namespace {
        ExportContextOwner::NamespaceFragment(fragment)
    } else if let Some(global) = context.global {
        ExportContextOwner::GlobalAugmentation(global)
    } else if let Some(module) = context.deferred_module {
        ExportContextOwner::DeferredAmbientModule(module)
    } else {
        return None;
    };
    let deferred_module_kind = match owner {
        ExportContextOwner::DeferredAmbientModule(module) => state
            .namespaces
            .deferred_modules
            .get(module.index())
            .map(|module| module.kind),
        ExportContextOwner::NamespaceFragment(_) | ExportContextOwner::GlobalAugmentation(_) => {
            None
        }
    };
    let syntax = match owner {
        ExportContextOwner::GlobalAugmentation(_) => ExportSyntaxDisposition::FutureTk2666,
        ExportContextOwner::DeferredAmbientModule(_)
            if deferred_module_kind == Some(DeferredModuleKind::ModuleAugmentation) =>
        {
            ExportSyntaxDisposition::FutureTk2666
        }
        ExportContextOwner::DeferredAmbientModule(_) => ExportSyntaxDisposition::Valid,
        ExportContextOwner::NamespaceFragment(_) => match kind {
            ExportContextKind::WrappedDeclaration => ExportSyntaxDisposition::Valid,
            ExportContextKind::NamedList if !has_module_specifier && context.ambient => {
                ExportSyntaxDisposition::Valid
            }
            ExportContextKind::NamedList | ExportContextKind::ExportAll => {
                ExportSyntaxDisposition::FutureTk1194
            }
            ExportContextKind::ExportDefault => ExportSyntaxDisposition::FutureTk1319,
            ExportContextKind::ExportAssignment => ExportSyntaxDisposition::FutureTk1063,
        },
    };
    let resolution =
        if matches!(owner, ExportContextOwner::DeferredAmbientModule(_)) || has_module_specifier {
            ExportResolutionDisposition::DeferredBacklog15
        } else {
            ExportResolutionDisposition::NotRequired
        };
    let id = ExportContextId(
        u32::try_from(state.namespaces.export_contexts.len())
            .expect("export context count fits u32"),
    );
    state.namespaces.export_contexts.push_local(ExportContext {
        id,
        owner,
        kind,
        syntax,
        resolution,
        has_module_specifier,
        source: unit.source,
        origin: unit.origin,
        span,
        members: Vec::new(),
    });
    Some(id)
}

pub(super) fn reserve_member_headers(
    state: &mut BindState,
    statements: &[Statement<'_>],
    context: WalkContext,
) {
    for statement in statements {
        reserve_statement_header(state, statement, context, false);
    }
}

pub(super) fn reserve_statement_header(
    state: &mut BindState,
    statement: &Statement<'_>,
    context: WalkContext,
    explicit: bool,
) {
    if let Statement::ExportNamedDeclaration(export) = statement {
        if let Some(declaration) = &export.declaration {
            reserve_declaration_header(state, declaration, context, true);
        }
        return;
    }
    if let Some(declaration) = statement.as_declaration() {
        reserve_declaration_header(state, declaration, context, explicit);
        return;
    }
    if let Statement::ImportDeclaration(import) = statement {
        let publication = context.publication(explicit);
        let owner = context.declaration_owner(publication);
        if let Some(specifiers) = &import.specifiers {
            for specifier in specifiers {
                dormant_symbol_for_declaration_owner(state, owner, specifier.local().name.as_str());
            }
        }
    }
}

pub(super) fn reserve_declaration_header(
    state: &mut BindState,
    declaration: &Declaration<'_>,
    context: WalkContext,
    explicit: bool,
) {
    let publication = context.publication(explicit);
    let owner = context.declaration_owner(publication);
    let mut reserve = |name: &str| {
        dormant_symbol_for_declaration_owner(state, owner, name);
    };
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                for identifier in declarator.id.get_binding_identifiers() {
                    reserve(identifier.name.as_str());
                }
            }
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                reserve(identifier.name.as_str());
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                reserve(identifier.name.as_str());
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => reserve(alias.id.name.as_str()),
        Declaration::TSInterfaceDeclaration(interface) => reserve(interface.id.name.as_str()),
        Declaration::TSEnumDeclaration(enumeration) => reserve(enumeration.id.name.as_str()),
        Declaration::TSModuleDeclaration(namespace) => {
            if let TSModuleDeclarationName::Identifier(identifier) = &namespace.id {
                reserve(identifier.name.as_str());
            }
        }
        Declaration::TSImportEqualsDeclaration(import) => reserve(import.id.name.as_str()),
        Declaration::TSGlobalDeclaration(_) => {}
    }
}

fn namespace_for(state: &mut BindState, owner: NamespaceOwner, name: &str) -> NamespaceId {
    let key = NamespaceKey {
        owner,
        name: name.to_string(),
    };
    if let Some(namespace) = state.namespaces.namespace_keys.get(&key) {
        return *namespace;
    }
    let public_scope = state
        .graph
        .push(Scope::new(ScopeKind::NamespacePublic, None));
    let symbol = dormant_symbol_for_namespace_owner(state, owner, name);
    let id = NamespaceId(
        u32::try_from(state.namespaces.namespaces.len()).expect("namespace count fits u32"),
    );
    state.namespaces.namespaces.push_local(Namespace {
        id,
        owner,
        name: name.to_string(),
        public_scope,
        symbol,
        fragments: Vec::new(),
    });
    state
        .namespaces
        .aggregate_instance_states
        .push_local(NamespaceInstanceState::NonInstantiated);
    state.namespaces.standalone_value_storages.push_local(None);
    let _ = state.namespaces.namespace_keys.insert_local(key, id);
    id
}

fn attach_namespace_symbol(state: &mut BindState, namespace: NamespaceId, declaration: DeclId) {
    let symbol_id = state
        .namespaces
        .get(namespace)
        .expect("namespace exists")
        .symbol;
    if let Some(symbol) = state.symbols.get_mut(symbol_id) {
        symbol.ns = Some(namespace);
        symbol.blocks_namespace_lookup = false;
    }
    state.attach_symbol_declaration(symbol_id, declaration);
}

pub(super) fn dormant_symbol_for_namespace_owner(
    state: &mut BindState,
    owner: NamespaceOwner,
    name: &str,
) -> SymbolId {
    match owner {
        NamespaceOwner::Lexical(scope) => dormant_symbol_in_scope(state, scope, name),
        NamespaceOwner::NamespacePublic(namespace) => {
            let scope = state
                .namespaces
                .get(namespace)
                .expect("parent namespace exists")
                .public_scope;
            dormant_symbol_in_scope(state, scope, name)
        }
        NamespaceOwner::FragmentPrivate(fragment) => {
            let scope = state
                .namespaces
                .fragment(fragment)
                .expect("parent namespace fragment exists")
                .private_scope;
            dormant_symbol_in_scope(state, scope, name)
        }
        NamespaceOwner::CompilationGlobal => {
            let scope = state
                .namespaces
                .compilation_global
                .expect("compilation-global scope allocated");
            dormant_symbol_in_scope(state, scope, name)
        }
    }
}

pub(super) fn dormant_symbol_for_declaration_owner(
    state: &mut BindState,
    owner: DeclarationOwner,
    name: &str,
) -> Option<SymbolId> {
    match owner {
        DeclarationOwner::Lexical(scope) => state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name)),
        DeclarationOwner::NamespacePublic(namespace) => {
            let scope = state.namespaces.get(namespace)?.public_scope;
            Some(dormant_symbol_in_scope(state, scope, name))
        }
        DeclarationOwner::NamespacePrivate(fragment) => {
            let scope = state.namespaces.fragment(fragment)?.private_scope;
            Some(dormant_symbol_in_scope(state, scope, name))
        }
        DeclarationOwner::CompilationGlobal => {
            let scope = state.namespaces.compilation_global?;
            Some(dormant_symbol_in_scope(state, scope, name))
        }
        DeclarationOwner::DeferredAmbientModule(_) => None,
    }
}

fn dormant_symbol_in_scope(state: &mut BindState, scope: ScopeId, name: &str) -> SymbolId {
    if let Some(symbol) = state
        .graph
        .get(scope)
        .and_then(|scope| scope.lookup_local(name))
    {
        return symbol;
    }
    let symbol = state.symbols.push(Symbol::new(name));
    if state.graph.declare(scope, name, symbol).is_err() {
        state.record_frozen_prefix_write(None, FrozenPrefixWriteSite::ScopeDeclaration);
    }
    symbol
}

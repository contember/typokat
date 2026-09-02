use super::metadata::WalkContext;
use super::models_ids::*;
use super::walking::*;
use crate::binder::bind::BindState;
use crate::binder::declaration::{DeclId, DeclarationKind};
use crate::binder::scope::{Scope, ScopeId, ScopeKind};
use crate::binder::symbol::SymbolId;
use crate::source::CompilationOrigin;
use crate::span::Span;
use oxc_ast::ast::{
    Declaration, ImportOrExportKind, ModuleExportName, Statement, TSModuleDeclaration,
    TSModuleDeclarationBody, TSModuleDeclarationName,
};
use oxc_span::GetSpan;

pub(super) fn bind_deferred_module(
    state: &mut BindState,
    declaration: &TSModuleDeclaration<'_>,
    context: WalkContext,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    let TSModuleDeclarationName::StringLiteral(specifier) = &declaration.id else {
        return;
    };
    let declaration_id = state.attach_declaration_scope(
        specifier.span.start,
        DeclarationKind::Namespace,
        context.lexical_scope,
    );
    let id = DeferredModuleId(
        u32::try_from(state.namespaces.deferred_modules.len())
            .expect("deferred module count fits u32"),
    );
    state
        .namespaces
        .deferred_modules
        .push_local(DeferredAmbientModule {
            id,
            declaration: declaration_id,
            source: unit.source,
            origin: unit.origin,
            module: state.current_module,
            owner: context.owner,
            kind: if unit.binding.external_module {
                DeferredModuleKind::ModuleAugmentation
            } else {
                DeferredModuleKind::AmbientExternalModule
            },
            specifier: specifier.value.to_string(),
            span: Span::from_oxc(declaration.span),
        });
    let deferred_context = WalkContext {
        owner: DeclarationOwner::DeferredAmbientModule(id),
        lexical_scope: context.lexical_scope,
        namespace: None,
        global: None,
        deferred_module: Some(id),
        ambient: true,
        ambient_export_list_mode: false,
        active_export_context: None,
        direct_top_level: false,
    };
    if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &declaration.body {
        for statement in &block.body {
            match statement {
                Statement::TSGlobalDeclaration(_) | Statement::TSNamespaceExportDeclaration(_) => {
                    walk_statement(
                        state,
                        statement,
                        deferred_context,
                        false,
                        unit,
                        compilation_global,
                    )
                }
                _ => record_deferred_statement(state, id, statement, deferred_context, unit),
            }
        }
    }
}

fn record_deferred_statement(
    state: &mut BindState,
    module: DeferredModuleId,
    statement: &Statement<'_>,
    context: WalkContext,
    unit: CompilationUnit,
) {
    if let Statement::ExportNamedDeclaration(export) = statement {
        push_export_context(
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
        if let Some(declaration) = &export.declaration {
            record_deferred_declaration(state, module, declaration, true, unit);
        } else {
            state
                .namespaces
                .deferred_children
                .push_local(DeferredAmbientChild {
                    module,
                    declaration: None,
                    kind: DeferredChildKind::DeferredExport,
                    name: None,
                    span: Span::from_oxc(export.span),
                    binding_span: None,
                    source: unit.source,
                    origin: unit.origin,
                });
        }
        return;
    }
    if let Some(declaration) = statement.as_declaration() {
        record_deferred_declaration(state, module, declaration, false, unit);
        return;
    }
    match statement {
        Statement::ImportDeclaration(import) => {
            if let Some(specifiers) = &import.specifiers {
                for specifier in specifiers {
                    let local = specifier.local();
                    record_deferred_binding(
                        state,
                        module,
                        local.span.start,
                        DeclarationKind::Import,
                        DeferredChildKind::OrdinaryDeclaration,
                        local.name.as_str(),
                        unit,
                    );
                }
            }
        }
        Statement::ExportAllDeclaration(export) => {
            push_export_context(
                state,
                context,
                ExportContextKind::ExportAll,
                true,
                Span::from_oxc(export.span),
                unit,
            );
            record_deferred_export(state, module, Span::from_oxc(export.span), unit)
        }
        Statement::ExportDefaultDeclaration(export) => {
            push_export_context(
                state,
                context,
                ExportContextKind::ExportDefault,
                false,
                Span::from_oxc(export.span),
                unit,
            );
            record_deferred_export(state, module, Span::from_oxc(export.span), unit)
        }
        Statement::TSExportAssignment(export) => {
            push_export_context(
                state,
                context,
                ExportContextKind::ExportAssignment,
                false,
                Span::from_oxc(export.span),
                unit,
            );
            record_deferred_export(state, module, Span::from_oxc(export.span), unit)
        }
        _ => {}
    }
}

fn record_deferred_declaration(
    state: &mut BindState,
    module: DeferredModuleId,
    declaration: &Declaration<'_>,
    exported: bool,
    unit: CompilationUnit,
) {
    let kind = if exported {
        DeferredChildKind::ExportDeclaration
    } else {
        DeferredChildKind::OrdinaryDeclaration
    };
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                for identifier in declarator.id.get_binding_identifiers() {
                    record_deferred_binding(
                        state,
                        module,
                        identifier.span.start,
                        DeclarationKind::Variable,
                        kind,
                        identifier.name.as_str(),
                        unit,
                    );
                }
            }
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Function,
                    kind,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Class,
                    kind,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => record_deferred_binding(
            state,
            module,
            alias.id.span.start,
            DeclarationKind::TypeAlias,
            kind,
            alias.id.name.as_str(),
            unit,
        ),
        Declaration::TSInterfaceDeclaration(interface) => record_deferred_binding(
            state,
            module,
            interface.id.span.start,
            DeclarationKind::Interface,
            kind,
            interface.id.name.as_str(),
            unit,
        ),
        Declaration::TSEnumDeclaration(enumeration) => record_deferred_binding(
            state,
            module,
            enumeration.id.span.start,
            DeclarationKind::Enum,
            kind,
            enumeration.id.name.as_str(),
            unit,
        ),
        Declaration::TSModuleDeclaration(namespace) => {
            if let TSModuleDeclarationName::Identifier(identifier) = &namespace.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Namespace,
                    DeferredChildKind::NamespaceDeclaration,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Declaration::TSImportEqualsDeclaration(import) => record_deferred_binding(
            state,
            module,
            import.id.span.start,
            DeclarationKind::ImportEquals,
            kind,
            import.id.name.as_str(),
            unit,
        ),
        Declaration::TSGlobalDeclaration(_) => {}
    }
}

fn record_deferred_binding(
    state: &mut BindState,
    module: DeferredModuleId,
    binding_start: u32,
    declaration_kind: DeclarationKind,
    child_kind: DeferredChildKind,
    name: &str,
    unit: CompilationUnit,
) {
    let declaration = state.source_decl_at(binding_start, declaration_kind);
    let (span, binding_span) = declaration
        .and_then(|declaration| state.declarations.get(declaration))
        .map(|declaration| {
            (
                declaration.site.declaration_span,
                Some(declaration.site.binding_span),
            )
        })
        .unwrap_or((Span::new(binding_start, binding_start), None));
    state
        .namespaces
        .deferred_children
        .push_local(DeferredAmbientChild {
            module,
            declaration,
            kind: child_kind,
            name: Some(MetadataName::Identifier(name.to_string())),
            span,
            binding_span,
            source: unit.source,
            origin: unit.origin,
        });
}

fn record_deferred_export(
    state: &mut BindState,
    module: DeferredModuleId,
    span: Span,
    unit: CompilationUnit,
) {
    state
        .namespaces
        .deferred_children
        .push_local(DeferredAmbientChild {
            module,
            declaration: None,
            kind: DeferredChildKind::DeferredExport,
            name: None,
            span,
            binding_span: None,
            source: unit.source,
            origin: unit.origin,
        });
}

pub(super) fn bind_global(
    state: &mut BindState,
    declaration: &oxc_ast::ast::TSGlobalDeclaration<'_>,
    context: WalkContext,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    let declaration_id = if context.deferred_module.is_some() {
        state
            .source_decl_at(declaration.global_span.start, DeclarationKind::Global)
            .expect("global source declaration exists")
    } else {
        state.attach_declaration_scope(
            declaration.global_span.start,
            DeclarationKind::Global,
            context.lexical_scope,
        )
    };
    let owner = if let Some(module) = context.deferred_module {
        GlobalOwner::DeferredAmbientModule(module)
    } else if let Some((_, fragment)) = context.namespace {
        GlobalOwner::NamespaceFragment(fragment)
    } else {
        GlobalOwner::Lexical(context.lexical_scope)
    };
    let placement = if context.deferred_module.is_some() {
        GlobalPlacement::DeferredAmbientModule
    } else if context.direct_top_level && unit.binding.external_module {
        GlobalPlacement::DirectExternalModule
    } else if context.direct_top_level {
        GlobalPlacement::DirectScript
    } else {
        GlobalPlacement::NestedNamespace
    };
    let in_module_augmentation = context.deferred_module.is_some_and(|module| {
        state
            .namespaces
            .deferred_modules
            .get(module.index())
            .is_some_and(|module| module.kind == DeferredModuleKind::ModuleAugmentation)
    });
    let mut issues = Vec::new();
    if matches!(
        placement,
        GlobalPlacement::DirectScript | GlobalPlacement::NestedNamespace
    ) || in_module_augmentation
    {
        issues.push(GlobalIssue::FutureTk2669);
    }
    if !declaration.declare && !context.ambient {
        issues.push(GlobalIssue::FutureTk2670);
    }
    let legal = issues.is_empty();
    let default_global_scope = state.continuation_default_global_scope(compilation_global);
    let uses_lexical_overlay = legal
        && (state.continuation_active()
            || (placement == GlobalPlacement::DirectExternalModule
                && matches!(unit.origin, CompilationOrigin::User(_))));
    let overlay_scope = if uses_lexical_overlay {
        state.global_augmentation_overlay_scope(
            context.lexical_scope,
            declaration.global_span.start,
            default_global_scope,
        )
    } else {
        state.graph.push(Scope::new(
            ScopeKind::GlobalOverlay,
            Some(context.lexical_scope),
        ))
    };
    let default_global_owner = if default_global_scope == compilation_global {
        DeclarationOwner::CompilationGlobal
    } else {
        DeclarationOwner::Lexical(default_global_scope)
    };
    let id = GlobalAugmentationId(
        u32::try_from(state.namespaces.globals.len()).expect("global count fits u32"),
    );
    state.namespaces.globals.push_local(GlobalAugmentation {
        id,
        declaration: declaration_id,
        source: unit.source,
        origin: unit.origin,
        module: state.current_module,
        owner,
        body_span: Span::from_oxc(declaration.body.span),
        diagnostic_span: Span::from_oxc(declaration.global_span),
        target_scope: if legal {
            default_global_scope
        } else {
            overlay_scope
        },
        overlay_scope,
        placement,
        issues,
        declared: declaration.declare,
        members: Vec::new(),
    });
    let global_body = WalkContext {
        owner: if legal {
            default_global_owner
        } else {
            DeclarationOwner::Lexical(overlay_scope)
        },
        lexical_scope: if uses_lexical_overlay {
            overlay_scope
        } else if legal && state.namespaces.uses_library_shared_globals() {
            default_global_scope
        } else {
            overlay_scope
        },
        namespace: None,
        global: Some(id),
        deferred_module: None,
        ambient: true,
        ambient_export_list_mode: declaration.body.body.iter().any(is_export_list_statement),
        active_export_context: None,
        direct_top_level: false,
    };
    reserve_member_headers(state, &declaration.body.body, global_body);
    walk_statements(
        state,
        &declaration.body.body,
        global_body,
        unit,
        compilation_global,
    );
}

/// Rows a merge group may hold before [`push_placement`] stops comparing them one by one and
/// starts keeping a by-declaration index. Scanning below it stays a bounded constant, so the cost
/// of placing one declaration never grows with its group either side of the line.
const PLACEMENT_INDEX_THRESHOLD: usize = 8;

#[allow(clippy::too_many_arguments)]
/// Places `declaration` under `owner`/`name` and returns its row, so a caller that has
/// more to record reaches it through this one merge-key lookup instead of hunting for
/// the row it just wrote across every placement in the project.
pub(super) fn push_placement<'state>(
    state: &'state mut BindState,
    owner: DeclarationOwner,
    name: &str,
    declaration: DeclId,
    kind: MergeDeclarationKind,
    spaces: DeclarationSpaces,
    ambient: bool,
    unit: CompilationUnit,
    syntax: DeclarationSyntaxFacts,
) -> Option<&'state mut MergeParticipant> {
    let span = state
        .declarations
        .get(declaration)
        .expect("placement declaration exists")
        .site
        .declaration_span;
    let binding_span = state
        .declarations
        .get(declaration)
        .expect("placement declaration exists")
        .site
        .binding_span;
    let participant = MergeParticipant {
        declaration,
        kind,
        source: unit.source,
        origin: unit.origin,
        span,
        binding_span,
        ambient,
        spaces,
        syntax,
        namespace_fragment: None,
        namespace_instance: None,
    };
    let name = state.namespaces.intern_name(name);
    let key = MergeKey { owner, name };
    let Ok(entries) = state
        .namespaces
        .placements
        .get_or_insert_local_with(key, PlacementRows::default)
    else {
        return None;
    };
    // A merge group holds one row per declaration sharing the name, never the project — and the
    // overwhelming majority hold one or two, which is cheaper to compare than to index. Only a
    // group that outgrows the scan takes a by-declaration index, and once it has one a miss in
    // that index is proof of absence rather than a reason to fall back to the scan.
    let index = if entries.len() < PLACEMENT_INDEX_THRESHOLD {
        match entries.iter().position(|entry| {
            #[cfg(any(test, feature = "test-utils"))]
            record_placement_row_probe();
            entry.declaration == declaration
        }) {
            Some(index) => index,
            None => entries.push(participant),
        }
    } else {
        // Fold in every row the index does not cover yet — once per row over the whole build,
        // including everything a group placed while it was still below the threshold.
        let indexed = state
            .namespaces
            .placement_rows_indexed
            .entry(key)
            .or_default();
        for (position, entry) in entries.iter().enumerate().skip(*indexed) {
            #[cfg(any(test, feature = "test-utils"))]
            record_placement_row_probe();
            state
                .namespaces
                .placement_rows
                .insert((key, entry.declaration), position);
        }
        #[cfg(any(test, feature = "test-utils"))]
        record_placement_row_probe();
        let index = match state
            .namespaces
            .placement_rows
            .get(&(key, declaration))
            .copied()
        {
            Some(index) => index,
            None => {
                let index = entries.push(participant);
                state
                    .namespaces
                    .placement_rows
                    .insert((key, declaration), index);
                index
            }
        };
        *indexed = entries.len();
        index
    };
    let row = entries.get_mut(index)?;
    row.syntax = syntax;
    // Mirror the row's facts so the by-declaration read is a lookup, not a scan. Only a
    // row this build actually placed contributes, so a sealed base still answers `None`.
    if syntax == DeclarationSyntaxFacts::None {
        state.placement_syntax.remove(&declaration);
    } else {
        state.placement_syntax.insert(declaration, syntax);
    }
    Some(row)
}

fn placement_syntax(state: &BindState, declaration: DeclId) -> Option<DeclarationSyntaxFacts> {
    let syntax = state.placement_syntax.get(&declaration).copied();
    #[cfg(any(test, feature = "test-utils"))]
    if syntax.is_some() {
        record_placement_row_probe();
    }
    syntax
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_member(
    state: &mut BindState,
    context: WalkContext,
    target: DeclarationOwner,
    declaration: Option<DeclId>,
    name: Option<String>,
    declaration_span: Span,
    binding_span: Span,
    spaces: DeclarationSpaces,
    kind: MergeDeclarationKind,
    publication: NamespacePublication,
    unit: CompilationUnit,
) {
    let Some(owner) = context.member_owner() else {
        return;
    };
    let id = NamespaceMemberId(
        u32::try_from(state.namespaces.members.len()).expect("namespace member count fits u32"),
    );
    let symbol = name
        .as_deref()
        .and_then(|name| dormant_symbol_for_declaration_owner(state, target, name));
    if let (Some(symbol), Some(declaration)) = (symbol, declaration) {
        state.attach_symbol_declaration(symbol, declaration);
    }
    let local_name = name.clone().map(MetadataName::Identifier);
    let exported_name = (!matches!(publication, NamespacePublication::Private))
        .then(|| name.clone().map(MetadataName::Identifier))
        .flatten();
    let exported_span = exported_name.as_ref().map(|_| binding_span);
    let syntax = declaration
        .and_then(|declaration| placement_syntax(state, declaration))
        .unwrap_or(DeclarationSyntaxFacts::None);
    state.namespaces.members.push_local(NamespaceMember {
        id,
        owner,
        target,
        declaration,
        symbol,
        local_symbol: symbol,
        name,
        local_name,
        exported_name,
        declaration_span,
        specifier_span: None,
        binding_span,
        local_span: Some(binding_span),
        exported_span,
        source: unit.source,
        origin: unit.origin,
        module_specifier: None,
        outer_type_only: false,
        specifier_type_only: false,
        alias_context: None,
        alias_resolution: None,
        alias_space_intent: None,
        export_context: context.active_export_context,
        syntax,
        spaces,
        kind,
        publication,
    });
    match owner {
        NamespaceMemberOwner::Fragment(fragment) => state
            .namespaces
            .fragments
            .get_mut_local(fragment.index())
            .expect("namespace fragment exists")
            .members
            .push(id),
        NamespaceMemberOwner::GlobalAugmentation(global) => state
            .namespaces
            .globals
            .get_mut_local(global.index())
            .expect("global augmentation exists")
            .members
            .push(id),
        NamespaceMemberOwner::DeferredAmbientModule(_) => {}
    }
    attach_export_member(state, context.active_export_context, id);
}

pub(super) fn attach_export_member(
    state: &mut BindState,
    context: Option<ExportContextId>,
    member: NamespaceMemberId,
) {
    let Some(context) = context else {
        return;
    };
    if let Some(context) = state
        .namespaces
        .export_contexts
        .get_mut_local(context.index())
    {
        if !context.members.contains(&member) {
            context.members.push(member);
        }
    }
}

pub(super) fn push_deferred_export_member(
    state: &mut BindState,
    context: WalkContext,
    span: Span,
    unit: CompilationUnit,
) {
    push_member(
        state,
        context,
        context.declaration_owner(NamespacePublication::Explicit),
        None,
        None,
        span,
        span,
        DeclarationSpaces::ALIAS,
        MergeDeclarationKind::DeferredExport,
        NamespacePublication::Explicit,
        unit,
    );
}

pub(super) fn push_export_alias_member(
    state: &mut BindState,
    context: WalkContext,
    export: &oxc_ast::ast::ExportNamedDeclaration<'_>,
    specifier: &oxc_ast::ast::ExportSpecifier<'_>,
    unit: CompilationUnit,
) {
    let Some(owner) = context.member_owner() else {
        return;
    };
    let local_name = metadata_name(&specifier.local);
    let exported_name = metadata_name(&specifier.exported);
    let local_owner = context.declaration_owner(NamespacePublication::Private);
    let local_symbol = lookup_dormant_symbol(state, local_owner, local_name.text()).or_else(|| {
        context.namespace.and_then(|(namespace, _)| {
            lookup_dormant_symbol(
                state,
                DeclarationOwner::NamespacePublic(namespace),
                local_name.text(),
            )
        })
    });
    let Some(export_context) = context
        .active_export_context
        .and_then(|id| state.namespaces.export_contexts.get(id.index()))
    else {
        return;
    };
    let alias_context = match export_context.syntax {
        ExportSyntaxDisposition::Valid => AliasContext::ValidAmbient,
        ExportSyntaxDisposition::FutureTk1194 => AliasContext::InvalidFutureTk1194,
        ExportSyntaxDisposition::FutureTk2666 => AliasContext::InvalidAugmentationFutureTk2666,
        ExportSyntaxDisposition::FutureTk1319 | ExportSyntaxDisposition::FutureTk1063 => {
            unreachable!("named export aliases cannot have default or assignment syntax")
        }
    };
    let alias_resolution = export_context.resolution;
    let (target, publication, symbol) = if alias_context == AliasContext::ValidAmbient {
        let target = context.declaration_owner(NamespacePublication::Explicit);
        let symbol = dormant_symbol_for_declaration_owner(state, target, exported_name.text());
        (target, NamespacePublication::Explicit, symbol)
    } else {
        (local_owner, NamespacePublication::Private, None)
    };
    let outer_type_only = export.export_kind == ImportOrExportKind::Type;
    let specifier_type_only = specifier.export_kind == ImportOrExportKind::Type;
    let spaces = if outer_type_only || specifier_type_only {
        DeclarationSpaces::TYPE
    } else {
        DeclarationSpaces::NONE
    };
    let alias_space_intent = if outer_type_only || specifier_type_only {
        AliasSpaceIntent::Type
    } else {
        AliasSpaceIntent::UnresolvedValueOrType
    };
    let id = NamespaceMemberId(
        u32::try_from(state.namespaces.members.len()).expect("namespace member count fits u32"),
    );
    state.namespaces.members.push_local(NamespaceMember {
        id,
        owner,
        target,
        declaration: None,
        symbol,
        local_symbol,
        name: Some(exported_name.text().to_string()),
        local_name: Some(local_name),
        exported_name: Some(exported_name),
        declaration_span: Span::from_oxc(export.span),
        specifier_span: Some(Span::from_oxc(specifier.span)),
        binding_span: Span::from_oxc(specifier.exported.span()),
        local_span: Some(Span::from_oxc(specifier.local.span())),
        exported_span: Some(Span::from_oxc(specifier.exported.span())),
        source: unit.source,
        origin: unit.origin,
        module_specifier: export
            .source
            .as_ref()
            .map(|source| MetadataName::StringLiteral(source.value.to_string())),
        outer_type_only,
        specifier_type_only,
        alias_context: Some(alias_context),
        alias_resolution: Some(alias_resolution),
        alias_space_intent: Some(alias_space_intent),
        export_context: context.active_export_context,
        syntax: DeclarationSyntaxFacts::None,
        spaces,
        kind: MergeDeclarationKind::DeferredExport,
        publication,
    });
    match owner {
        NamespaceMemberOwner::Fragment(fragment) => {
            if let Some(fragment) = state.namespaces.fragments.get_mut_local(fragment.index()) {
                fragment.members.push(id);
            }
        }
        NamespaceMemberOwner::GlobalAugmentation(global) => {
            if let Some(global) = state.namespaces.globals.get_mut_local(global.index()) {
                global.members.push(id);
            }
        }
        NamespaceMemberOwner::DeferredAmbientModule(_) => {}
    }
    attach_export_member(state, context.active_export_context, id);
}

fn lookup_dormant_symbol(
    state: &BindState,
    owner: DeclarationOwner,
    name: &str,
) -> Option<SymbolId> {
    let scope = match owner {
        DeclarationOwner::Lexical(scope) => scope,
        DeclarationOwner::NamespacePublic(namespace) => {
            state.namespaces.get(namespace)?.public_scope
        }
        DeclarationOwner::NamespacePrivate(fragment) => {
            state.namespaces.fragment(fragment)?.private_scope
        }
        DeclarationOwner::CompilationGlobal => state.namespaces.compilation_global?,
        DeclarationOwner::DeferredAmbientModule(_) => return None,
    };
    state.graph.get(scope)?.lookup_local(name)
}

fn metadata_name(name: &ModuleExportName<'_>) -> MetadataName {
    match name {
        ModuleExportName::IdentifierName(identifier) => {
            MetadataName::Identifier(identifier.name.to_string())
        }
        ModuleExportName::IdentifierReference(identifier) => {
            MetadataName::Identifier(identifier.name.to_string())
        }
        ModuleExportName::StringLiteral(literal) => {
            MetadataName::StringLiteral(literal.value.to_string())
        }
    }
}

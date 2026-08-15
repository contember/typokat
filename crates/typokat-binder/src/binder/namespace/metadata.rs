use super::deferred::push_placement;
use super::lookup::*;
use super::models_ids::*;
use super::values::*;
use super::walking::*;
use crate::binder::bind::BindState;
use crate::binder::declaration::DeclarationKind;
use crate::binder::scope::ScopeId;
use oxc_ast::ast::{Expression, Program, Statement};

fn suppress_direct_global_this_variable_values(state: &mut BindState) {
    let mut affected = Vec::new();
    for declaration in state
        .declarations
        .local_declarations()
        .filter(|declaration| {
            declaration.kind == DeclarationKind::Variable
                && direct_script_global_this_conflict(
                    &state.declarations,
                    &state.namespaces,
                    &state.graph,
                    declaration.site.module,
                    declaration.site.binding_span.start,
                    declaration.kind,
                )
        })
    {
        for scope in [declaration.site.scope, state.namespaces.compilation_global]
            .into_iter()
            .flatten()
        {
            let Some(symbol) = state
                .graph
                .get(scope)
                .and_then(|scope| scope.lookup_local("globalThis"))
            else {
                continue;
            };
            if state
                .symbols
                .get(symbol)
                .is_some_and(|symbol| symbol.declarations.contains(&declaration.id))
                && !affected.contains(&symbol)
            {
                affected.push(symbol);
            }
        }
    }
    for symbol in affected {
        let replacement = state.symbols.get(symbol).and_then(|symbol| {
            symbol.declarations.iter().rev().find_map(|declaration| {
                let declaration = state.declarations.get(*declaration)?;
                let rejected = declaration.kind == DeclarationKind::Variable
                    && direct_script_global_this_conflict(
                        &state.declarations,
                        &state.namespaces,
                        &state.graph,
                        declaration.site.module,
                        declaration.site.binding_span.start,
                        declaration.kind,
                    );
                (!rejected).then_some(declaration.value_storage).flatten()
            })
        });
        if let Some(symbol) = state.symbols.get_mut(symbol) {
            symbol.value = replacement;
        }
    }
}

#[derive(Copy, Clone)]
pub(super) struct WalkContext {
    pub(super) owner: DeclarationOwner,
    pub(super) lexical_scope: ScopeId,
    pub(super) namespace: Option<(NamespaceId, NamespaceFragmentId)>,
    pub(super) global: Option<GlobalAugmentationId>,
    pub(super) deferred_module: Option<DeferredModuleId>,
    pub(super) ambient: bool,
    pub(super) ambient_export_list_mode: bool,
    pub(super) active_export_context: Option<ExportContextId>,
    pub(super) direct_top_level: bool,
}

impl WalkContext {
    pub(super) fn member_owner(self) -> Option<NamespaceMemberOwner> {
        if let Some((_, fragment)) = self.namespace {
            Some(NamespaceMemberOwner::Fragment(fragment))
        } else if let Some(global) = self.global {
            Some(NamespaceMemberOwner::GlobalAugmentation(global))
        } else {
            self.deferred_module
                .map(NamespaceMemberOwner::DeferredAmbientModule)
        }
    }

    pub(super) fn publication(self, explicit: bool) -> NamespacePublication {
        if explicit {
            NamespacePublication::Explicit
        } else if self.ambient && !self.ambient_export_list_mode {
            NamespacePublication::AmbientDefault
        } else {
            NamespacePublication::Private
        }
    }

    pub(super) fn declaration_owner(self, publication: NamespacePublication) -> DeclarationOwner {
        match self.namespace {
            Some((namespace, fragment)) => match publication {
                NamespacePublication::Explicit
                | NamespacePublication::AmbientDefault
                | NamespacePublication::DottedImplicit => {
                    DeclarationOwner::NamespacePublic(namespace)
                }
                NamespacePublication::Private => DeclarationOwner::NamespacePrivate(fragment),
            },
            None => self.owner,
        }
    }

    pub(super) fn namespace_owner(
        self,
        publication: NamespacePublication,
    ) -> Option<NamespaceOwner> {
        match self.namespace {
            Some((namespace, fragment)) => Some(match publication {
                NamespacePublication::Explicit
                | NamespacePublication::AmbientDefault
                | NamespacePublication::DottedImplicit => {
                    NamespaceOwner::NamespacePublic(namespace)
                }
                NamespacePublication::Private => NamespaceOwner::FragmentPrivate(fragment),
            }),
            None => Some(match self.owner {
                DeclarationOwner::Lexical(scope) => NamespaceOwner::Lexical(scope),
                DeclarationOwner::NamespacePublic(namespace) => {
                    NamespaceOwner::NamespacePublic(namespace)
                }
                DeclarationOwner::NamespacePrivate(fragment) => {
                    NamespaceOwner::FragmentPrivate(fragment)
                }
                DeclarationOwner::CompilationGlobal => NamespaceOwner::CompilationGlobal,
                DeclarationOwner::DeferredAmbientModule(_) => return None,
            }),
        }
    }
}

#[derive(Copy, Clone)]
pub(in crate::binder) enum NamespaceMetadataRoot {
    Module,
    LibrarySharedGlobal,
    /// A user suffix continuing a frozen library: script files own the delta global layer,
    /// because the library's own global scope is sealed (backlog 102).
    ContinuationSharedGlobal(ScopeId),
}

/// Collect namespace topology after ordinary declarations.
pub(in crate::binder) fn collect_namespace_metadata(
    state: &mut BindState,
    module: ScopeId,
    program: &Program<'_>,
    unit: CompilationUnit,
    compilation_global: ScopeId,
    script_namespace_root: ScopeId,
    root: NamespaceMetadataRoot,
) {
    state
        .namespaces
        .record_compilation_origin(unit.source, unit.origin);
    state.namespaces.source_units.push_local(SourceUnitRecord {
        source: unit.source,
        origin: unit.origin,
        module,
        context: unit.binding,
    });
    state.namespaces.compilation_global = Some(compilation_global);
    state.namespaces.script_namespace_root = Some(script_namespace_root);
    if matches!(root, NamespaceMetadataRoot::LibrarySharedGlobal) {
        state.namespaces.library_shared_globals = true;
    }
    let (owner, lexical_scope) = match root {
        NamespaceMetadataRoot::Module => (DeclarationOwner::Lexical(module), module),
        NamespaceMetadataRoot::LibrarySharedGlobal if unit.binding.external_module => {
            (DeclarationOwner::Lexical(module), module)
        }
        NamespaceMetadataRoot::LibrarySharedGlobal => {
            (DeclarationOwner::CompilationGlobal, compilation_global)
        }
        NamespaceMetadataRoot::ContinuationSharedGlobal(_) if unit.binding.external_module => {
            (DeclarationOwner::Lexical(module), module)
        }
        // A delta-side *lexical* owner: one project-wide domain that merges user script globals
        // with each other, while the sealed library domain stays untouched.
        NamespaceMetadataRoot::ContinuationSharedGlobal(delta_global) => {
            (DeclarationOwner::Lexical(delta_global), module)
        }
    };
    let context = WalkContext {
        owner,
        lexical_scope,
        namespace: None,
        global: None,
        deferred_module: None,
        ambient: unit.binding.declaration_file(),
        ambient_export_list_mode: false,
        active_export_context: None,
        direct_top_level: true,
    };
    walk_statements(state, &program.body, context, unit, compilation_global);
    let export_assignment_target = program.body.iter().find_map(|statement| {
        let Statement::TSExportAssignment(assignment) = statement else {
            return None;
        };
        let Expression::Identifier(identifier) = &assignment.expression else {
            return None;
        };
        Some(identifier.name.to_string())
    });
    for export in state.namespaces.umd_exports.local_slice_mut() {
        if export.source == unit.source {
            export.export_assignment_target = export_assignment_target.clone();
        }
    }
}

pub(in crate::binder) fn finalize_namespace_metadata(state: &mut BindState) {
    resolve_local_ambient_export_alias_targets(state);
    state
        .namespaces
        .classify()
        .unwrap_or_else(|error| panic!("namespace classification failed: {error}"));
    suppress_direct_global_this_variable_values(state);
}

pub(in crate::binder) fn fill_namespace_value_attachments(
    state: &mut BindState,
    program: &Program<'_>,
    plan: &NamespaceValueAttachmentPlan,
) {
    bind_namespace_value_attachment_members(state, program, plan);
}

/// Everything one project module contributes on its own. Classification is a whole-project
/// pass, so the builder runs it once after the module loop instead of once per module.
pub(crate) fn collect_project_namespace_metadata(
    state: &mut BindState,
    module: ScopeId,
    program: &Program<'_>,
    unit: CompilationUnit,
    compilation_global: ScopeId,
    script_namespace_root: ScopeId,
    delta_compilation_global: Option<ScopeId>,
) {
    collect_namespace_metadata(
        state,
        module,
        program,
        unit,
        compilation_global,
        script_namespace_root,
        match delta_compilation_global {
            Some(delta) => NamespaceMetadataRoot::ContinuationSharedGlobal(delta),
            None => NamespaceMetadataRoot::Module,
        },
    );
    publish_continuation_hoisted_variables(state, unit);
}

fn publish_continuation_hoisted_variables(state: &mut BindState, unit: CompilationUnit) {
    for (binding_start, name) in state.continuation_publication_sites() {
        let Some(declaration) = state.source_decl_at(binding_start, DeclarationKind::Variable)
        else {
            continue;
        };
        let key = MergeKey {
            owner: DeclarationOwner::CompilationGlobal,
            name: state.namespaces.intern_name(&name),
        };
        if state
            .namespaces
            .placements
            .get(&key)
            .is_some_and(|participants| {
                participants
                    .iter()
                    .any(|participant| participant.declaration == declaration)
            })
        {
            continue;
        }
        push_placement(
            state,
            DeclarationOwner::CompilationGlobal,
            &name,
            declaration,
            MergeDeclarationKind::Variable,
            DeclarationSpaces::VALUE,
            unit.binding.declaration_file(),
            unit,
            DeclarationSyntaxFacts::Variable(VariableKind::Var),
        );
    }
}

use super::layered::NamespaceTable;
use super::models_ids::*;
use crate::binder::bind::Binder;
use crate::binder::declaration::{DeclarationKind, DeclarationTable, TypeGroupId, ValueStorageId};
use crate::binder::scope::{ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::SymbolId;

pub(super) fn direct_script_global_this_conflict(
    declarations: &DeclarationTable,
    namespaces: &NamespaceTable,
    graph: &ScopeGraph,
    module: ScopeId,
    binding_start: u32,
    kind: DeclarationKind,
) -> bool {
    let Some(declaration) = declarations.declaration_at_site(module, binding_start, kind) else {
        return false;
    };
    let Some(unit) = namespaces.source_unit_for_module(module) else {
        return false;
    };
    if unit.context.external_module {
        return false;
    }
    declaration
        .site
        .scope
        .and_then(|scope| graph.get(scope))
        .is_some_and(|scope| matches!(scope.kind, ScopeKind::Module | ScopeKind::CompilationGlobal))
}

pub(super) fn global_this_namespace_is_type_only(
    namespaces: &NamespaceTable,
    script_namespace_root: Option<ScopeId>,
    namespace: NamespaceId,
) -> bool {
    namespaces.get(namespace).is_some_and(|namespace| {
        namespace.name == "globalThis"
            && match namespace.owner {
                NamespaceOwner::CompilationGlobal => true,
                NamespaceOwner::Lexical(owner) => Some(owner) == script_namespace_root,
                NamespaceOwner::NamespacePublic(_) | NamespaceOwner::FragmentPrivate(_) => false,
            }
    })
}

#[derive(Copy, Clone, Default)]
struct QualifiedSymbolView {
    namespace: Option<NamespaceId>,
    type_group: Option<TypeGroupId>,
    value: bool,
    unavailable: bool,
    deferred: Option<QualifiedTypePathDeferredReason>,
}

/// Every member listed by at least one **ambient** fragment of one namespace.
///
/// `some ambient fragment lists this member` is asked once per member of the namespace, and the
/// fragments it would scan grow with the reopens, so the answer is derived once as a union and
/// read back by lookup instead (backlog 88).
pub(super) fn ambient_member_union(
    fragments: &[&NamespaceFragment],
) -> rustc_hash::FxHashSet<NamespaceMemberId> {
    fragments
        .iter()
        .filter(|fragment| fragment.ambient)
        .flat_map(|fragment| fragment.members.iter().copied())
        .collect()
}

impl Binder {
    /// Return whether the exact global-augmentation site has placement issues.
    pub fn global_augmentation_has_issues(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Option<bool> {
        self.namespaces
            .global_augmentations_by_site
            .get(&(module, binding_start))
            .and_then(|global| self.namespaces.globals.get(global.index()))
            .map(|global| !global.issues.is_empty())
    }

    pub fn global_augmentation_scope(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Option<ScopeId> {
        #[cfg(any(test, feature = "test-utils"))]
        record_continuation_global_statement_query();
        self.namespaces
            .global_augmentations_by_site
            .get(&(module, binding_start))
            .and_then(|global| self.namespaces.globals.get(global.index()))
            .map(|global| global.overlay_scope)
    }

    /// The lexical root the binder used when walking statements from `module`.
    pub fn statement_lexical_root(&self, module: ScopeId) -> ScopeId {
        let is_script_continuation = self.namespaces.uses_library_shared_globals()
            && self
                .namespaces
                .source_unit_for_module(module)
                .is_some_and(|unit| !unit.context.external_module)
            && self.graph.get(module).and_then(|scope| scope.parent)
                == Some(self.script_namespace_root);
        if is_script_continuation {
            self.graph
                .get(self.script_namespace_root)
                .and_then(|scope| scope.parent)
                .unwrap_or(module)
        } else {
            module
        }
    }

    /// Whether one exact namespace declaration conflicts with the script-global `globalThis`.
    pub fn direct_script_global_this_conflict(
        &self,
        module: ScopeId,
        binding_start: u32,
        kind: DeclarationKind,
    ) -> bool {
        direct_script_global_this_conflict(
            &self.declarations,
            &self.namespaces,
            &self.graph,
            module,
            binding_start,
            kind,
        )
    }

    /// Whether one resolved value is a modeled direct-script `globalThis` conflict.
    pub fn direct_global_this_value_conflict(&self, symbol: SymbolId) -> bool {
        let Some(symbol) = self.symbols.get(symbol) else {
            return false;
        };
        if symbol.name != "globalThis" {
            return false;
        }
        symbol.declarations.iter().rev().any(|declaration| {
            self.declarations
                .get(*declaration)
                .is_some_and(|declaration| {
                    declaration.value_storage == symbol.value
                        && matches!(
                            declaration.kind,
                            DeclarationKind::Class | DeclarationKind::Function
                        )
                        && direct_script_global_this_conflict(
                            &self.declarations,
                            &self.namespaces,
                            &self.graph,
                            declaration.site.module,
                            declaration.site.binding_span.start,
                            declaration.kind,
                        )
                })
        })
    }

    /// Whether one namespace is a type-only contributor to the global `globalThis` surface.
    pub fn global_this_namespace_is_type_only(&self, module: ScopeId, binding_start: u32) -> bool {
        self.exact_declaration_at(module, binding_start, DeclarationKind::Namespace)
            .and_then(|declaration| declaration.namespace)
            .is_some_and(|namespace| {
                global_this_namespace_is_type_only(
                    &self.namespaces,
                    Some(self.script_namespace_root),
                    namespace,
                )
            })
    }

    pub fn global_augmentation_body_scope(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Option<ScopeId> {
        #[cfg(any(test, feature = "test-utils"))]
        record_continuation_global_statement_query();
        self.namespaces
            .global_augmentations_by_site
            .get(&(module, binding_start))
            .and_then(|global| self.namespaces.globals.get(global.index()))
            .map(|global| {
                let uses_lexical_overlay = self
                    .graph
                    .get(global.overlay_scope)
                    .is_some_and(|scope| scope.namespace_public == Some(global.target_scope));
                if uses_lexical_overlay
                    || !global.issues.is_empty()
                    || !self.namespaces.uses_library_shared_globals()
                {
                    global.overlay_scope
                } else {
                    global.target_scope
                }
            })
    }

    pub fn continuation_global_augmentation_body_scope(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Option<ScopeId> {
        let global = self
            .namespaces
            .global_augmentations_by_site
            .get(&(module, binding_start))
            .and_then(|global| self.namespaces.globals.get(global.index()))?;
        (global.issues.is_empty()
            && self
                .graph
                .get(global.overlay_scope)
                .is_some_and(|scope| scope.namespace_public == Some(global.target_scope)))
        .then_some(global.overlay_scope)
    }

    pub fn global_augmentation_requires_incomplete(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> bool {
        #[cfg(any(test, feature = "test-utils"))]
        record_continuation_global_statement_query();
        let Some(global) = self
            .namespaces
            .global_augmentations_by_site
            .get(&(module, binding_start))
            .and_then(|global| self.namespaces.globals.get(global.index()))
        else {
            return false;
        };
        if !global.issues.is_empty() {
            return false;
        }
        global.members.iter().any(|member| {
            let Some(member) = self.namespaces.member(*member) else {
                return true;
            };
            match member.kind {
                MergeDeclarationKind::Interface | MergeDeclarationKind::TypeAlias => false,
                MergeDeclarationKind::Namespace => {
                    let fragment = member
                        .declaration
                        .and_then(|declaration| {
                            self.namespaces.fragments_by_declaration.get(&declaration)
                        })
                        .and_then(|fragment| self.namespaces.fragment(*fragment));
                    fragment.is_none_or(|fragment| {
                        fragment.instance_state == NamespaceInstanceState::Instantiated
                    })
                }
                _ => true,
            }
        })
    }

    pub fn global_augmentation_value_storages(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Vec<ValueStorageId> {
        let Some(global) = self
            .namespaces
            .global_augmentations_by_site
            .get(&(module, binding_start))
            .and_then(|global| self.namespaces.globals.get(global.index()))
        else {
            return Vec::new();
        };
        global
            .members
            .iter()
            .filter_map(|member| self.namespaces.member(*member))
            .filter_map(|member| {
                member
                    .declaration
                    .and_then(|declaration| self.declarations.get(declaration))
                    .and_then(|declaration| declaration.value_storage)
                    .or_else(|| {
                        member
                            .symbol
                            .and_then(|symbol| self.symbols.get(symbol))
                            .and_then(|symbol| symbol.value)
                    })
            })
            .collect()
    }

    pub fn umd_export_requires_incomplete(&self, module: ScopeId, span_start: u32) -> bool {
        #[cfg(any(test, feature = "test-utils"))]
        record_continuation_umd_statement_query();
        self.namespaces
            .umd_exports_by_site
            .get(&(module, span_start))
            .and_then(|export| self.namespaces.umd_exports.get(*export))
            .is_some_and(|export| export.context == UmdContext::DeferredValidBacklog15)
    }

    pub fn library_export_default_reporting_owns(&self, module: ScopeId, span_start: u32) -> bool {
        #[cfg(any(test, feature = "test-utils"))]
        record_continuation_library_source_lookup();
        let source = self
            .namespaces
            .source_unit_for_module(module)
            .map(|unit| unit.source);
        #[cfg(any(test, feature = "test-utils"))]
        record_continuation_library_reporting_lookup();
        source.is_some_and(|source| {
            self.namespaces
                .library_export_default_sites
                .contains_key(&(source, span_start))
        })
    }

    pub fn library_module_reporting_owns(&self, module: ScopeId, source_start: u32) -> bool {
        #[cfg(any(test, feature = "test-utils"))]
        record_continuation_library_reporting_lookup();
        self.namespaces
            .library_module_reporting_sites
            .contains_key(&(module, source_start))
    }
}

impl Binder {
    pub fn local_ambient_export_alias_failures(&self) -> Vec<LocalAmbientExportAliasFailure> {
        let members = self
            .namespaces
            .members
            .local_iter()
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_ambient_alias_member_row();
            })
            .collect::<Vec<_>>();
        self.ambient_export_alias_failures_for(members)
    }

    pub fn all_ambient_export_alias_failures(&self) -> Vec<LocalAmbientExportAliasFailure> {
        let members = self.namespaces.members.iter().collect::<Vec<_>>();
        self.ambient_export_alias_failures_for(members)
    }

    fn ambient_export_alias_failures_for(
        &self,
        members: Vec<&NamespaceMember>,
    ) -> Vec<LocalAmbientExportAliasFailure> {
        members
            .into_iter()
            .filter(|member| {
                matches!(member.owner, NamespaceMemberOwner::Fragment(_))
                    && member.kind == MergeDeclarationKind::DeferredExport
                    && member.declaration.is_none()
                    && member.alias_context == Some(AliasContext::ValidAmbient)
                    && member.module_specifier.is_none()
                    && member.local_symbol.is_none()
            })
            .filter_map(|member| {
                let local_name = member.local_name.as_ref()?.text();
                let kind = match member.owner {
                    NamespaceMemberOwner::Fragment(fragment) => {
                        let fragment = self.namespaces.fragment(fragment)?;
                        let public_scope = self.namespaces.get(fragment.namespace)?.public_scope;
                        let has_dormant_symbol = [fragment.private_scope, public_scope]
                            .into_iter()
                            .any(|scope| {
                                self.graph
                                    .get(scope)
                                    .and_then(|scope| scope.lookup_local(local_name))
                                    .is_some()
                            });
                        if has_dormant_symbol {
                            LocalAmbientExportAliasFailureKind::NonLocal
                        } else {
                            LocalAmbientExportAliasFailureKind::Missing
                        }
                    }
                    NamespaceMemberOwner::GlobalAugmentation(_)
                    | NamespaceMemberOwner::DeferredAmbientModule(_) => {
                        LocalAmbientExportAliasFailureKind::Missing
                    }
                };
                Some(LocalAmbientExportAliasFailure {
                    origin: member.origin,
                    local_span: member.local_span?,
                    local_name: local_name.to_string(),
                    kind,
                })
            })
            .collect()
    }

    pub fn resolve_qualified_type_path(
        &self,
        scope: ScopeId,
        segments: &[&str],
    ) -> QualifiedTypePathResolution {
        self.resolve_qualified_type_path_traced(scope, segments, || {}, |_| {})
    }

    pub fn resolve_qualified_type_path_traced(
        &self,
        scope: ScopeId,
        segments: &[&str],
        mut compilation_root_probe: impl FnMut(),
        mut namespace_visit: impl FnMut(NamespaceId),
    ) -> QualifiedTypePathResolution {
        if segments.len() < 2 {
            return QualifiedTypePathResolution::MissingRoot { segment: 0 };
        }

        let root_name = segments[0];
        let mut current_scope = Some(scope);
        let mut saw_type_root = false;
        let root_namespace = loop {
            let Some(candidate_scope) = current_scope else {
                return if saw_type_root {
                    QualifiedTypePathResolution::TypeOnlyRoot { segment: 0 }
                } else {
                    QualifiedTypePathResolution::MissingRoot { segment: 0 }
                };
            };
            let scope_record = match self.graph.get(candidate_scope) {
                Some(scope) => scope,
                None => return QualifiedTypePathResolution::MissingRoot { segment: 0 },
            };
            if candidate_scope == self.compilation_global {
                compilation_root_probe();
            }
            let owning_namespace = self.namespace_for_lookup_scope(candidate_scope);
            let root_merge = owning_namespace
                .is_none()
                .then(|| self.root_merge_record(candidate_scope, root_name))
                .flatten();
            let root_deferred = root_merge.and_then(Self::merge_deferred_reason);
            let root_symbol = scope_record.lookup_local(root_name);
            if let Some(symbol) = root_symbol {
                if self
                    .symbols
                    .get(symbol)
                    .is_some_and(|symbol| symbol.blocks_namespace_lookup)
                {
                    return root_deferred.map_or(
                        QualifiedTypePathResolution::Unavailable { segment: 0 },
                        |reason| QualifiedTypePathResolution::Deferred { segment: 0, reason },
                    );
                }
                let view = self.qualified_symbol_view(owning_namespace, symbol, &mut Vec::new());
                if view.unavailable {
                    return QualifiedTypePathResolution::Unavailable { segment: 0 };
                }
                if let Some(reason) = view.deferred {
                    return QualifiedTypePathResolution::Deferred { segment: 0, reason };
                }
                if let Some(reason) = root_deferred {
                    let admitted_enum_namespace = reason == QualifiedTypePathDeferredReason::Enum
                        && view.namespace.is_some()
                        && root_merge.is_some_and(|record| {
                            record.classification.disposition == MergeDisposition::Admitted
                        });
                    if reason == QualifiedTypePathDeferredReason::Enum && !admitted_enum_namespace {
                        return QualifiedTypePathResolution::Deferred { segment: 0, reason };
                    }
                    let known_target = view.namespace.is_some() || view.type_group.is_some();
                    let admitted = root_merge.is_some_and(|record| {
                        record.classification.disposition == MergeDisposition::Admitted
                    });
                    let concrete_import_namespace = reason
                        == QualifiedTypePathDeferredReason::Import
                        && view.namespace.is_some()
                        && root_merge.is_some_and(Self::has_qualified_import_namespace_target);
                    if !known_target || (!admitted && !concrete_import_namespace) {
                        return QualifiedTypePathResolution::Deferred { segment: 0, reason };
                    }
                }
                if let Some(namespace) = view.namespace {
                    break namespace;
                }
                saw_type_root |= view.type_group.is_some()
                    || self
                        .symbols
                        .get(symbol)
                        .is_some_and(|symbol| symbol.ty.is_some());
            } else if let Some(reason) = root_deferred {
                return QualifiedTypePathResolution::Deferred { segment: 0, reason };
            }

            if scope_record.kind == ScopeKind::GlobalOverlay {
                if let Some(public_scope) = scope_record.namespace_public {
                    let public_record = match self.graph.get(public_scope) {
                        Some(scope) => scope,
                        None => {
                            return QualifiedTypePathResolution::MissingRoot { segment: 0 };
                        }
                    };
                    if public_scope == self.compilation_global {
                        compilation_root_probe();
                    }
                    let public_owner = self.namespace_for_lookup_scope(public_scope);
                    let public_root_merge = public_owner
                        .is_none()
                        .then(|| self.root_merge_record(public_scope, root_name))
                        .flatten();
                    let public_root_deferred =
                        public_root_merge.and_then(Self::merge_deferred_reason);
                    if let Some(symbol) = public_record.lookup_local(root_name) {
                        if self
                            .symbols
                            .get(symbol)
                            .is_some_and(|symbol| symbol.blocks_namespace_lookup)
                        {
                            return public_root_deferred.map_or(
                                QualifiedTypePathResolution::Unavailable { segment: 0 },
                                |reason| QualifiedTypePathResolution::Deferred {
                                    segment: 0,
                                    reason,
                                },
                            );
                        }
                        let view =
                            self.qualified_symbol_view(public_owner, symbol, &mut Vec::new());
                        if view.unavailable {
                            return QualifiedTypePathResolution::Unavailable { segment: 0 };
                        }
                        if let Some(reason) = view.deferred {
                            return QualifiedTypePathResolution::Deferred { segment: 0, reason };
                        }
                        if let Some(reason) = public_root_deferred {
                            let admitted_enum_namespace = reason
                                == QualifiedTypePathDeferredReason::Enum
                                && view.namespace.is_some()
                                && public_root_merge.is_some_and(|record| {
                                    record.classification.disposition == MergeDisposition::Admitted
                                });
                            if reason == QualifiedTypePathDeferredReason::Enum
                                && !admitted_enum_namespace
                            {
                                return QualifiedTypePathResolution::Deferred {
                                    segment: 0,
                                    reason,
                                };
                            }
                            let known_target =
                                view.namespace.is_some() || view.type_group.is_some();
                            let admitted = public_root_merge.is_some_and(|record| {
                                record.classification.disposition == MergeDisposition::Admitted
                            });
                            let concrete_import_namespace = reason
                                == QualifiedTypePathDeferredReason::Import
                                && view.namespace.is_some()
                                && public_root_merge
                                    .is_some_and(Self::has_qualified_import_namespace_target);
                            if !known_target || (!admitted && !concrete_import_namespace) {
                                return QualifiedTypePathResolution::Deferred {
                                    segment: 0,
                                    reason,
                                };
                            }
                        }
                        if let Some(namespace) = view.namespace {
                            break namespace;
                        }
                        saw_type_root |= view.type_group.is_some()
                            || self
                                .symbols
                                .get(symbol)
                                .is_some_and(|symbol| symbol.ty.is_some());
                    } else if let Some(reason) = public_root_deferred {
                        return QualifiedTypePathResolution::Deferred { segment: 0, reason };
                    }
                }
            }

            if scope_record.kind == ScopeKind::NamespacePrivate {
                if let Some(namespace) = owning_namespace {
                    let public_scope = self
                        .namespaces
                        .get(namespace)
                        .map(|namespace| namespace.public_scope);
                    if let Some(public_scope) = public_scope {
                        if let Some(symbol) = self
                            .graph
                            .get(public_scope)
                            .and_then(|scope| scope.lookup_local(root_name))
                        {
                            let view = self.qualified_symbol_view(
                                Some(namespace),
                                symbol,
                                &mut Vec::new(),
                            );
                            if view.unavailable {
                                return QualifiedTypePathResolution::Unavailable { segment: 0 };
                            }
                            if let Some(reason) = view.deferred {
                                return QualifiedTypePathResolution::Deferred {
                                    segment: 0,
                                    reason,
                                };
                            }
                            if let Some(namespace) = view.namespace {
                                break namespace;
                            }
                            saw_type_root |= view.type_group.is_some()
                                || self
                                    .symbols
                                    .get(symbol)
                                    .is_some_and(|symbol| symbol.ty.is_some());
                        }
                    }
                }
            }
            current_scope = scope_record.parent;
        };

        let mut namespace = root_namespace;
        namespace_visit(namespace);
        for (segment, name) in segments.iter().enumerate().skip(1) {
            let public_scope = match self.namespaces.get(namespace) {
                Some(namespace) => namespace.public_scope,
                None => return QualifiedTypePathResolution::MissingMember { segment },
            };
            let Some(symbol) = self
                .graph
                .get(public_scope)
                .and_then(|scope| scope.lookup_local(name))
            else {
                return QualifiedTypePathResolution::MissingMember { segment };
            };
            let view = self.qualified_symbol_view(Some(namespace), symbol, &mut Vec::new());
            if view.unavailable {
                return QualifiedTypePathResolution::Unavailable { segment };
            }
            if let Some(reason) = view.deferred {
                return QualifiedTypePathResolution::Deferred { segment, reason };
            }
            let leaf = segment + 1 == segments.len();
            if !leaf {
                if let Some(next) = view.namespace {
                    namespace = next;
                    namespace_visit(namespace);
                    continue;
                }
                return if view.type_group.is_some() {
                    QualifiedTypePathResolution::TypeOnlyIntermediate { segment }
                } else {
                    QualifiedTypePathResolution::MissingMember { segment }
                };
            }
            if let Some(group) = view.type_group {
                return QualifiedTypePathResolution::TypeGroup(group);
            }
            return if view.value {
                QualifiedTypePathResolution::ValueOnlyLeaf { segment }
            } else {
                QualifiedTypePathResolution::MissingMember { segment }
            };
        }

        QualifiedTypePathResolution::MissingMember {
            segment: segments.len() - 1,
        }
    }

    fn namespace_for_lookup_scope(&self, scope: ScopeId) -> Option<NamespaceId> {
        match self
            .namespaces
            .declaration_owners_by_scope
            .get(&scope)
            .copied()
        {
            Some(DeclarationOwner::NamespacePublic(namespace)) => Some(namespace),
            Some(DeclarationOwner::NamespacePrivate(fragment)) => self
                .namespaces
                .fragment(fragment)
                .map(|fragment| fragment.namespace),
            Some(DeclarationOwner::Lexical(_))
            | Some(DeclarationOwner::CompilationGlobal)
            | Some(DeclarationOwner::DeferredAmbientModule(_))
            | None => None,
        }
    }

    fn root_merge_record(&self, scope: ScopeId, name: &str) -> Option<&MergeRecord> {
        let key = MergeKey {
            owner: DeclarationOwner::Lexical(scope),
            name: self.namespaces.name_id(name)?,
        };
        self.namespaces
            .merge_indices
            .get(&key)
            .and_then(|index| self.namespaces.merges.get(*index))
    }

    fn merge_deferred_reason(record: &MergeRecord) -> Option<QualifiedTypePathDeferredReason> {
        if record
            .declarations
            .iter()
            .any(|declaration| declaration.kind == MergeDeclarationKind::ImportAlias)
        {
            Some(QualifiedTypePathDeferredReason::Import)
        } else if record
            .declarations
            .iter()
            .any(|declaration| declaration.kind == MergeDeclarationKind::Enum)
        {
            Some(QualifiedTypePathDeferredReason::Enum)
        } else {
            None
        }
    }

    fn has_qualified_import_namespace_target(record: &MergeRecord) -> bool {
        record
            .classification
            .compositions
            .iter()
            .any(|composition| {
                matches!(
                    composition.kind,
                    MergeCompositionKind::ImportNamespace(
                        ImportBindingForm::Named | ImportBindingForm::Default
                    )
                ) && matches!(
                    composition.disposition,
                    MergeDisposition::Admitted | MergeDisposition::DeferredBacklog15
                )
            })
    }

    fn qualified_symbol_view(
        &self,
        owner: Option<NamespaceId>,
        symbol: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> QualifiedSymbolView {
        if visited.contains(&symbol) {
            return QualifiedSymbolView {
                unavailable: true,
                ..QualifiedSymbolView::default()
            };
        }
        visited.push(symbol);
        let mut view = self
            .symbols
            .get(symbol)
            .map(|symbol| QualifiedSymbolView {
                namespace: symbol.ns,
                type_group: symbol.ty,
                value: symbol.value.is_some(),
                unavailable: false,
                deferred: None,
            })
            .unwrap_or_default();

        if let Some(owner) = owner {
            for member in self.namespace_members_for_symbol(owner, symbol) {
                match member.kind {
                    MergeDeclarationKind::ImportAlias => {
                        view.deferred = Some(QualifiedTypePathDeferredReason::Import);
                        break;
                    }
                    MergeDeclarationKind::Enum => {}
                    MergeDeclarationKind::DeferredExport if member.declaration.is_none() => {
                        if member.module_specifier.is_some() {
                            view.deferred = Some(QualifiedTypePathDeferredReason::Import);
                            break;
                        }
                        let Some(target) = member.local_symbol else {
                            view = QualifiedSymbolView {
                                unavailable: true,
                                ..QualifiedSymbolView::default()
                            };
                            break;
                        };
                        let mut target_view =
                            self.qualified_symbol_view(Some(owner), target, visited);
                        if member.alias_space_intent == Some(AliasSpaceIntent::Type) {
                            target_view.value = false;
                        }
                        view = target_view;
                        break;
                    }
                    _ => {
                        view.value |= member.spaces.value;
                    }
                }
            }
        }
        visited.pop();
        view
    }

    fn namespace_members_for_symbol(
        &self,
        namespace: NamespaceId,
        symbol: SymbolId,
    ) -> Vec<&NamespaceMember> {
        self.namespaces
            .get(namespace)
            .into_iter()
            .flat_map(|namespace| namespace.fragments.iter())
            .filter_map(|fragment| self.namespaces.fragment(*fragment))
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| self.namespaces.member(*member))
            .filter(|member| member.symbol == Some(symbol))
            .collect()
    }
}

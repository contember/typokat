use super::classification::*;
use super::lookup::*;
use super::models_ids::*;
use crate::binder::bind::{
    bind_class_declaration, bind_declarator, bind_enum_declaration, bind_function_declaration,
    BindState, Binder,
};
use crate::binder::declaration::{
    DeclId, DeclarationKind, TypeFragmentKind, TypeGroupId, ValueStorageId,
};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use oxc_ast::ast::{Declaration, Program, Statement, TSModuleDeclaration, TSModuleDeclarationBody};
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;

/// Append dormant namespace-owned slots after all lexical storage allocation.
pub(in crate::binder) fn allocate_dormant_namespace_value_storages(state: &mut BindState) {
    let candidates = state
        .namespaces
        .dormant_standalone_value_storage_candidates()
        .into_iter()
        .filter(|namespace| {
            !global_this_namespace_is_type_only(
                &state.namespaces,
                state.namespaces.script_namespace_root,
                *namespace,
            )
        })
        .collect::<Vec<_>>();
    for namespace in candidates {
        let storage = state.fresh_value_storage();
        let slot = state
            .namespaces
            .standalone_value_storages
            .get_mut_local(namespace.index())
            .expect("namespace storage side column is dense");
        assert!(
            slot.replace(storage).is_none(),
            "namespace storage is stable"
        );
        state
            .namespaces
            .standalone_storage_namespaces
            .insert_local(storage, namespace)
            .expect("namespace storage cannot replace a frozen index entry");
        let symbol = state
            .namespaces
            .get(namespace)
            .expect("namespace storage owner exists")
            .symbol;
        let root = state
            .symbols
            .get_mut(symbol)
            .expect("namespace root symbol exists");
        assert!(
            root.value.replace(storage).is_none(),
            "standalone root is dormant"
        );
    }

    let storage_free_type_side = state
        .namespaces
        .canonical_namespaces
        .local_iter()
        .inspect(|_| {
            #[cfg(any(test, feature = "test-utils"))]
            record_continuation_allocation_namespace_row();
        })
        .copied()
        .filter(|namespace| {
            state
                .namespaces
                .is_storage_free_type_side_namespace(*namespace)
        })
        .filter_map(|namespace| state.namespaces.get(namespace).map(|root| root.symbol))
        .collect::<Vec<_>>();
    for symbol in storage_free_type_side {
        if let Some(symbol) = state.symbols.get_mut(symbol) {
            symbol.blocks_value_lookup = true;
        }
    }
}

impl Binder {
    pub fn namespace_fragment_terminal_input(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Option<(NamespaceId, ScopeId, bool)> {
        let declaration =
            self.exact_declaration_at(module, binding_start, DeclarationKind::Namespace)?;
        let fragment = self
            .namespaces
            .fragments_by_declaration
            .get(&declaration.id)
            .and_then(|fragment| self.namespaces.fragment(*fragment))?;
        Some((fragment.namespace, fragment.private_scope, fragment.ambient))
    }

    /// Return the lexical body input for an admitted global namespace with no runtime surface.
    pub fn storage_free_global_namespace_body_input(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Option<(ScopeId, bool)> {
        let declaration =
            self.exact_declaration_at(module, binding_start, DeclarationKind::Namespace)?;
        let fragment = self
            .namespaces
            .fragments_by_declaration
            .get(&declaration.id)
            .and_then(|fragment| self.namespaces.fragment(*fragment))?;
        let namespace = self.namespaces.get(fragment.namespace)?;
        let record = self
            .namespaces
            .standalone_merge_record(fragment.namespace)?;
        if !self
            .namespaces
            .is_admitted_storage_free_global_namespace(record)
            || self
                .namespaces
                .standalone_value_storage(fragment.namespace)
                .is_some()
        {
            return None;
        }
        let symbol = self.symbols.get(namespace.symbol)?;
        if symbol.value.is_some() || !symbol.blocks_value_lookup {
            return None;
        }
        Some((fragment.private_scope, fragment.ambient))
    }

    pub fn namespace_fragment_private_scope(
        &self,
        module: ScopeId,
        source_start: u32,
    ) -> Option<ScopeId> {
        #[cfg(any(test, feature = "test-utils"))]
        record_continuation_fragment_scope_lookup();
        self.namespaces
            .fragment_private_scopes_by_site
            .get(&(module, source_start))
            .copied()
    }

    pub fn standalone_namespace_for_storage(&self, storage: ValueStorageId) -> Option<NamespaceId> {
        self.namespaces
            .standalone_storage_namespaces
            .get(&storage)
            .copied()
    }

    pub fn local_standalone_namespace_value_attachments(
        &self,
    ) -> Vec<StandaloneNamespaceValueAttachment<'_>> {
        let namespaces = self
            .namespaces
            .namespaces
            .changed_iter()
            .map(|(_, namespace)| namespace.id)
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_attachment_namespace_row();
            })
            .collect::<Vec<_>>();
        self.standalone_namespace_value_attachments_for(namespaces)
    }

    pub fn all_standalone_namespace_value_attachments(
        &self,
    ) -> Vec<StandaloneNamespaceValueAttachment<'_>> {
        let namespaces = self
            .namespaces
            .namespaces
            .iter()
            .map(|namespace| namespace.id)
            .collect::<Vec<_>>();
        self.standalone_namespace_value_attachments_for(namespaces)
    }

    fn standalone_namespace_value_attachments_for(
        &self,
        namespaces: Vec<NamespaceId>,
    ) -> Vec<StandaloneNamespaceValueAttachment<'_>> {
        namespaces
            .into_iter()
            .filter_map(|namespace| {
                let storage = self.namespaces.standalone_value_storage(namespace)?;
                let root = self.namespaces.get(namespace)?;
                let fragments = root
                    .fragments
                    .iter()
                    .filter_map(|fragment| self.namespaces.fragment(*fragment))
                    .collect::<Vec<_>>();
                // "some ambient fragment lists this member" read off one union instead of
                // re-scanning every fragment's member list per member (backlog 88).
                let ambient_members = ambient_member_union(&fragments);
                let mut members = fragments
                    .iter()
                    .flat_map(|fragment| fragment.members.iter())
                    .filter_map(|member| self.namespaces.member(*member))
                    .map(|member| {
                        let declaration = member.declaration;
                        let lexical = declaration.and_then(|id| self.declarations.get(id));
                        let child_namespace = member
                            .symbol
                            .and_then(|symbol| self.symbols.get(symbol))
                            .and_then(|symbol| symbol.ns);
                        StandaloneNamespaceValueMember {
                            member: member.id,
                            declaration,
                            symbol: member.symbol,
                            name: member.name.as_deref(),
                            source: member.source,
                            site: lexical.map(|declaration| declaration.site),
                            declaration_span: member.declaration_span,
                            local_span: member.local_span,
                            origin: member.origin,
                            value_storage: lexical
                                .and_then(|declaration| declaration.value_storage)
                                .or_else(|| {
                                    member
                                        .symbol
                                        .and_then(|symbol| self.symbols.get(symbol))
                                        .and_then(|symbol| symbol.value)
                                }),
                            alias_target_storage: member
                                .local_symbol
                                .and_then(|symbol| self.symbols.get(symbol))
                                .and_then(|symbol| symbol.value),
                            ambient: ambient_members.contains(&member.id),
                            child_namespace,
                            kind: member.kind,
                            publication: member.publication,
                            spaces: member.spaces,
                        }
                    })
                    .collect::<Vec<_>>();
                if self.namespaces.uses_library_shared_globals() {
                    members.sort_by_key(|member| {
                        (
                            member.origin,
                            member
                                .site
                                .map_or(u32::MAX, |site| site.declaration_span.start),
                            member
                                .declaration
                                .map_or(u32::MAX, |declaration| declaration.0),
                            member.member.0,
                        )
                    });
                } else {
                    members.sort_by_key(|member| {
                        (
                            member.source,
                            member
                                .site
                                .map_or(u32::MAX, |site| site.declaration_span.start),
                            member
                                .declaration
                                .map_or(u32::MAX, |declaration| declaration.0),
                            member.member.0,
                        )
                    });
                }
                Some(StandaloneNamespaceValueAttachment {
                    namespace,
                    storage,
                    symbol: root.symbol,
                    fragments,
                    members,
                })
            })
            .collect()
    }
}

impl Binder {
    /// Return the frozen namespace-side input for one lexical value owner.
    /// Only admitted owners and the exact backlog-42 callable recovery expose members.
    pub fn namespace_value_attachment(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<NamespaceValueAttachment<'_>> {
        let mut current = Some(scope);
        while let Some(candidate) = current {
            let scope = self.graph.get(candidate)?;
            if scope.lookup_local(name).is_some() {
                let owner = self.namespaces.declaration_owner_for_scope(candidate);
                return self.namespace_value_attachment_for_owner(owner, name);
            }
            if let Some(public) = scope.namespace_public {
                if self
                    .graph
                    .get(public)
                    .and_then(|scope| scope.lookup_local(name))
                    .is_some()
                {
                    let owner = self.namespaces.declaration_owner_for_scope(public);
                    return self.namespace_value_attachment_for_owner(owner, name);
                }
            }
            current = scope.parent;
        }
        None
    }

    pub fn namespace_value_attachment_for_owner(
        &self,
        owner: DeclarationOwner,
        name: &str,
    ) -> Option<NamespaceValueAttachment<'_>> {
        let key = MergeKey {
            owner,
            name: self.namespaces.name_id(name)?,
        };
        let record = self
            .namespaces
            .merge_indices
            .get(&key)
            .and_then(|index| self.namespaces.merges.get(*index))?;
        self.namespace_value_attachment_from_record(record)
    }

    /// Return the canonical enum descriptor input for one real type-group identity.
    pub fn enum_group(&self, group: TypeGroupId) -> Option<EnumGroupView<'_>> {
        let declaration = self
            .type_groups
            .get(group)?
            .fragments
            .iter()
            .find(|fragment| fragment.kind == TypeFragmentKind::Enum)?
            .declaration;
        let key = self
            .namespaces
            .merge_keys_by_declaration
            .get(&declaration)?;
        let record = self
            .namespaces
            .merge_indices
            .get(key)
            .and_then(|index| self.namespaces.merges.get(*index))?;
        self.enum_group_from_record(group, record)
    }

    /// Return enum groups whose merge rows changed in this continuation.
    pub fn local_enum_groups(&self) -> Vec<EnumGroupView<'_>> {
        let mut seen = BTreeSet::new();
        let mut groups = Vec::new();
        for record in self.namespaces.local_merges() {
            for group in record.declarations.iter().filter_map(|participant| {
                (participant.kind == MergeDeclarationKind::Enum)
                    .then(|| self.declarations.get(participant.declaration)?.type_group)
                    .flatten()
            }) {
                if seen.insert(group) {
                    if let Some(view) = self.enum_group_from_record(group, record) {
                        groups.push(view);
                    }
                }
            }
        }
        groups
    }

    fn enum_group_from_record<'a>(
        &'a self,
        group: TypeGroupId,
        record: &'a MergeRecord,
    ) -> Option<EnumGroupView<'a>> {
        let (buildability, issues) = enum_group_semantics(&record.declarations)?;
        let declarations = record
            .declarations
            .iter()
            .filter(|participant| {
                participant.kind == MergeDeclarationKind::Enum
                    && self
                        .declarations
                        .get(participant.declaration)
                        .is_some_and(|declaration| declaration.type_group == Some(group))
            })
            .copied()
            .collect::<Vec<_>>();
        let namespace_declarations = record
            .declarations
            .iter()
            .filter(|participant| participant.kind == MergeDeclarationKind::Namespace)
            .copied()
            .collect::<Vec<_>>();
        let namespace = namespace_declarations
            .iter()
            .filter_map(|participant| participant.namespace_fragment)
            .filter_map(|fragment| self.namespaces.fragment(fragment))
            .map(|fragment| fragment.namespace)
            .next();
        debug_assert!(namespace_declarations
            .iter()
            .filter_map(|participant| participant.namespace_fragment)
            .filter_map(|fragment| self.namespaces.fragment(fragment))
            .all(|fragment| Some(fragment.namespace) == namespace));
        Some(EnumGroupView {
            group,
            owner: record.owner,
            name: record.name.as_ref(),
            declarations,
            namespace,
            namespace_declarations,
            buildability,
            issues,
            placement_issues: record.placement_issues.clone(),
        })
    }

    fn namespace_value_attachment_from_record<'a>(
        &'a self,
        record: &'a MergeRecord,
    ) -> Option<NamespaceValueAttachment<'a>> {
        let disposition = namespace_value_attachment_disposition(record)?;
        let namespace = record.declarations.iter().find_map(|participant| {
            participant
                .namespace_fragment
                .and_then(|fragment| self.namespaces.fragment(fragment))
                .map(|fragment| fragment.namespace)
        })?;
        let symbol = self.namespaces.get(namespace)?.symbol;
        let exposes_members = matches!(
            disposition,
            NamespaceValueAttachmentDisposition::AdmittedFunction
                | NamespaceValueAttachmentDisposition::AdmittedClass
                | NamespaceValueAttachmentDisposition::AdmittedEnum
                | NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42
        );
        let fragments = if exposes_members {
            record
                .declarations
                .iter()
                .filter_map(|participant| participant.namespace_fragment)
                .filter_map(|fragment| self.namespaces.fragment(fragment))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        // Same union as the standalone attachment pass: one pass over the fragments instead of
        // one per member (backlog 88).
        let ambient_members = ambient_member_union(&fragments);
        let mut members = fragments
            .iter()
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| self.namespaces.member(*member))
            .filter(|member| {
                member.spaces.value && !matches!(member.publication, NamespacePublication::Private)
            })
            .filter_map(|member| {
                let declaration = member.declaration?;
                let lexical = self.declarations.get(declaration)?;
                let name = member.name.as_deref()?;
                Some(AttachedNamespaceValueMember {
                    member: member.id,
                    declaration,
                    name,
                    source: member.source,
                    origin: member.origin,
                    scope: lexical.site.scope?,
                    site: lexical.site,
                    value_storage: lexical.value_storage,
                    symbol: member.symbol,
                    kind: member.kind,
                    variable_kind: match member.syntax {
                        DeclarationSyntaxFacts::Variable(kind) => Some(kind),
                        _ => None,
                    },
                    publication: member.publication,
                    ambient: ambient_members.contains(&member.id),
                })
            })
            .collect::<Vec<_>>();
        if self.namespaces.uses_library_shared_globals() {
            members.sort_by_key(|member| {
                (
                    member.origin,
                    member.site.declaration_span.start,
                    member.declaration.0,
                )
            });
        } else {
            members.sort_by_key(|member| {
                (
                    member.source,
                    member.site.declaration_span.start,
                    member.declaration.0,
                )
            });
        }
        Some(NamespaceValueAttachment {
            owner: record.owner,
            name: &record.name,
            symbol,
            disposition,
            fragments,
            members,
        })
    }
}

pub(super) fn namespace_value_attachment_disposition(
    record: &MergeRecord,
) -> Option<NamespaceValueAttachmentDisposition> {
    let has_namespace = record
        .declarations
        .iter()
        .any(|participant| participant.kind == MergeDeclarationKind::Namespace);
    if !has_namespace {
        return None;
    }
    let has_function = record
        .classification
        .compositions
        .iter()
        .any(|composition| {
            composition.kind == MergeCompositionKind::FunctionNamespace
                && composition.disposition == MergeDisposition::Admitted
        });
    let has_enum = record
        .declarations
        .iter()
        .any(|participant| participant.kind == MergeDeclarationKind::Enum);
    let only_enum_function_namespace = record.declarations.iter().all(|participant| {
        matches!(
            participant.kind,
            MergeDeclarationKind::Enum
                | MergeDeclarationKind::Function
                | MergeDeclarationKind::Namespace
        )
    });
    if record.classification.disposition == MergeDisposition::RejectedFutureTk2567
        && has_function
        && has_enum
        && only_enum_function_namespace
    {
        return Some(NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42);
    }
    if record.classification.disposition != MergeDisposition::Admitted {
        return Some(NamespaceValueAttachmentDisposition::Rejected(
            record.classification.disposition,
        ));
    }
    let has_class = record
        .classification
        .compositions
        .iter()
        .any(|composition| {
            composition.kind == MergeCompositionKind::ClassNamespace
                && composition.disposition == MergeDisposition::Admitted
        });
    match (has_function, has_class, has_enum) {
        (true, false, false) => Some(NamespaceValueAttachmentDisposition::AdmittedFunction),
        (false, true, false) => Some(NamespaceValueAttachmentDisposition::AdmittedClass),
        (false, false, true) => Some(NamespaceValueAttachmentDisposition::AdmittedEnum),
        (false, false, false) => Some(NamespaceValueAttachmentDisposition::TypeContainerOnly),
        _ => Some(NamespaceValueAttachmentDisposition::Rejected(
            MergeDisposition::RejectedRedeclaration,
        )),
    }
}

#[derive(Clone)]
struct NamespaceValueBindingTarget {
    member: Option<NamespaceMemberId>,
    declaration: DeclId,
    name: String,
    /// The module that declares the member — the one fill that must bind it.
    module: ScopeId,
    scope: ScopeId,
    kind: MergeDeclarationKind,
    public_symbol: Option<SymbolId>,
}

/// Every namespace value attachment the batch will bind, derived once and filed by the module
/// that declares each member.
pub(crate) struct NamespaceValueAttachmentPlan {
    /// Sorted and deduplicated exactly once, so the surviving target of a declaration that two
    /// merges reach is decided by one whole-project neighbourhood rather than by whatever
    /// subset a module happened to see.
    targets: Vec<NamespaceValueBindingTarget>,
    /// Indexes into `targets`, ascending, so each module's fill keeps the project order.
    by_module: FxHashMap<ScopeId, Vec<usize>>,
    /// Whether the batch owes a whole-project replay after its fills; see
    /// `replay_namespace_value_attachments`.
    replays_every_target: bool,
}

impl NamespaceValueAttachmentPlan {
    fn targets_for(&self, module: ScopeId) -> &[usize] {
        self.by_module
            .get(&module)
            .map_or(&[][..], |indexes| indexes.as_slice())
    }
}

/// Derive the batch's value attachments once, from the merge set classification just froze.
///
/// Classification runs once before the fill loop, so the merge set no longer changes while the
/// batch fills. One collection therefore produces byte-for-byte the list every fill used to
/// rebuild for itself, and one sort/dedup keeps the whole-project neighbourhood that decides
/// which of two targets for the same declaration wins — narrowing the *scan* per module would
/// have changed that neighbourhood, which is why the plan is built whole and only its
/// application is split.
pub(in crate::binder) fn plan_namespace_value_attachments(
    state: &BindState,
) -> NamespaceValueAttachmentPlan {
    let mut targets = Vec::new();
    for record in state.namespaces.local_merges() {
        #[cfg(any(test, feature = "test-utils"))]
        record_finalization_attachment_merge_row();
        let attached = matches!(
            namespace_value_attachment_disposition(record),
            Some(
                NamespaceValueAttachmentDisposition::AdmittedFunction
                    | NamespaceValueAttachmentDisposition::AdmittedClass
                    | NamespaceValueAttachmentDisposition::AdmittedEnum
                    | NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42
            )
        );
        let storage_free_global_namespace = state
            .namespaces
            .is_admitted_storage_free_global_namespace(record);
        let inaccessible_owner = matches!(record.owner, DeclarationOwner::DeferredAmbientModule(_));
        if (!attached
            && !state.namespaces.is_admitted_instantiated_standalone(record)
            && !storage_free_global_namespace)
            || inaccessible_owner
        {
            continue;
        }
        for fragment in record
            .declarations
            .iter()
            .filter_map(|participant| participant.namespace_fragment)
            .filter_map(|fragment| state.namespaces.fragment(fragment))
        {
            for member in fragment
                .members
                .iter()
                .filter_map(|member| state.namespaces.member(*member))
                .filter(|member| {
                    member.spaces.value
                        && matches!(
                            member.kind,
                            MergeDeclarationKind::Variable
                                | MergeDeclarationKind::Function
                                | MergeDeclarationKind::Class
                                | MergeDeclarationKind::Enum
                        )
                        && (!storage_free_global_namespace
                            || member.kind == MergeDeclarationKind::Class)
                })
            {
                let (Some(declaration), Some(name)) = (member.declaration, member.name.as_ref())
                else {
                    continue;
                };
                let public_symbol = if matches!(member.publication, NamespacePublication::Private) {
                    None
                } else {
                    let Some(symbol) = member.symbol else {
                        continue;
                    };
                    Some(symbol)
                };
                // A merge is not owned by one module: `namespace N` reopened in a second file
                // leaves one record whose fragments live in two modules, and in a library batch
                // a `function` in one file merges with a `namespace` in another (the project
                // path never merges those two — a top-level function is owned by its module
                // scope while the namespace is owned by the shared script root, so they land in
                // different records). Keying on the *member's* declaring module is what makes
                // every participating module fill its own members; keying on the record's owner
                // would drop the other file's.
                let Some((module, scope)) =
                    state.declarations.get(declaration).and_then(|declaration| {
                        declaration
                            .site
                            .scope
                            .map(|scope| (declaration.site.module, scope))
                    })
                else {
                    continue;
                };
                targets.push(NamespaceValueBindingTarget {
                    member: Some(member.id),
                    declaration,
                    name: name.clone(),
                    module,
                    scope,
                    kind: member.kind,
                    public_symbol,
                });
            }
        }
    }
    if !state.namespaces.uses_library_shared_globals() {
        for global in state
            .namespaces
            .local_globals()
            .filter(|global| global.issues.is_empty())
        {
            for member in global
                .members
                .iter()
                .filter_map(|member| state.namespaces.member(*member))
                .filter(|member| {
                    member.spaces.value
                        && matches!(
                            member.kind,
                            MergeDeclarationKind::Variable
                                | MergeDeclarationKind::Function
                                | MergeDeclarationKind::Class
                        )
                })
            {
                let (Some(declaration), Some(name), Some(public_symbol)) =
                    (member.declaration, member.name.as_ref(), member.symbol)
                else {
                    continue;
                };
                if state
                    .namespaces
                    .merge_disposition_for_declaration(declaration)
                    != Some(MergeDisposition::Admitted)
                {
                    continue;
                }
                let Some((module, scope)) =
                    state.declarations.get(declaration).and_then(|declaration| {
                        declaration
                            .site
                            .scope
                            .map(|scope| (declaration.site.module, scope))
                    })
                else {
                    continue;
                };
                targets.push(NamespaceValueBindingTarget {
                    member: None,
                    declaration,
                    name: name.clone(),
                    module,
                    scope,
                    kind: member.kind,
                    public_symbol: Some(public_symbol),
                });
            }
        }
    }
    targets.sort_by_key(|target| {
        state
            .declarations
            .get(target.declaration)
            .map(|declaration| {
                (
                    declaration.site.declaration_span.start,
                    target.declaration.0,
                )
            })
            .unwrap_or((u32::MAX, u32::MAX))
    });
    targets.dedup_by_key(|target| target.declaration);
    let mut by_module: FxHashMap<ScopeId, Vec<usize>> = FxHashMap::default();
    for (index, target) in targets.iter().enumerate() {
        by_module.entry(target.module).or_default().push(index);
    }
    NamespaceValueAttachmentPlan {
        targets,
        by_module,
        replays_every_target: !state.namespaces.uses_library_shared_globals(),
    }
}

pub(super) fn bind_namespace_value_attachment_members(
    state: &mut BindState,
    program: &Program<'_>,
    plan: &NamespaceValueAttachmentPlan,
) {
    let indexes = plan.targets_for(state.current_module);
    // Only this module's declarations can be selected: `source_decl_at` resolves span starts
    // against `current_module`, so a foreign `DeclId` could never match this program anyway.
    let scopes = indexes
        .iter()
        .filter_map(|index| plan.targets.get(*index))
        .map(|target| (target.declaration, target.scope))
        .collect::<FxHashMap<_, _>>();
    bind_selected_namespace_value_statements(state, &program.body, &scopes);

    for target in indexes.iter().filter_map(|index| plan.targets.get(*index)) {
        apply_namespace_value_attachment(state, target);
    }
}

/// A library-shared-globals batch always filled one module's members at a time, but without
/// that flag every fill re-applied the *whole* project's target set — so the `symbol.value`
/// that survived for a name declared in two files is the globally last declaration's, not the
/// last file's. Replaying the whole plan once, in the same project order, reproduces exactly
/// that final state; the flagged path must not replay, because it never re-applied anything.
/// The replay is also what keeps a target whose module is outside this batch (a frozen library
/// module reached through a local merge) written, precisely as the repeated fills wrote it.
pub(in crate::binder) fn replay_namespace_value_attachments(
    state: &mut BindState,
    plan: &NamespaceValueAttachmentPlan,
) {
    if !plan.replays_every_target {
        return;
    }
    for target in &plan.targets {
        apply_namespace_value_attachment(state, target);
    }
}

fn apply_namespace_value_attachment(state: &mut BindState, target: &NamespaceValueBindingTarget) {
    let storage = state
        .declarations
        .get(target.declaration)
        .and_then(|declaration| declaration.value_storage);
    let local_symbol = state
        .graph
        .get(target.scope)
        .and_then(|scope| scope.lookup_local(&target.name));
    if let Some(member) = target.member {
        if let Some(member) = state.namespaces.members.get_mut_local(member.index()) {
            member.local_symbol = local_symbol;
        }
    }
    let Some(symbol) = target.public_symbol else {
        return;
    };
    let Some(storage) = storage else {
        return;
    };
    if let Some(symbol) = state.symbols.get_mut(symbol) {
        symbol.value = Some(storage);
        if target.kind == MergeDeclarationKind::Function
            && !symbol.function_values.contains(&storage)
        {
            symbol.function_values.push(storage);
        }
    }
}

fn bind_selected_namespace_value_statements(
    state: &mut BindState,
    statements: &[Statement<'_>],
    scopes: &FxHashMap<DeclId, ScopeId>,
) {
    for statement in statements {
        if let Statement::ExportNamedDeclaration(export) = statement {
            if let Some(declaration) = &export.declaration {
                bind_selected_namespace_value_declaration(state, declaration, scopes);
            }
            continue;
        }
        if let Some(declaration) = statement.as_declaration() {
            bind_selected_namespace_value_declaration(state, declaration, scopes);
        }
    }
}

fn bind_selected_namespace_value_declaration(
    state: &mut BindState,
    declaration: &Declaration<'_>,
    scopes: &FxHashMap<DeclId, ScopeId>,
) {
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            bind_selected_namespace_variable(state, declaration, scopes)
        }
        Declaration::FunctionDeclaration(function) => {
            let scope = function.id.as_ref().and_then(|identifier| {
                state
                    .source_decl_at(identifier.span.start, DeclarationKind::Function)
                    .and_then(|declaration| scopes.get(&declaration).copied())
            });
            if let Some(scope) = scope {
                bind_function_declaration(state, scope, function);
            }
        }
        Declaration::ClassDeclaration(class) => {
            let scope = class.id.as_ref().and_then(|identifier| {
                state
                    .source_decl_at(identifier.span.start, DeclarationKind::Class)
                    .and_then(|declaration| scopes.get(&declaration).copied())
            });
            if let Some(scope) = scope {
                bind_class_declaration(state, scope, class);
            }
        }
        Declaration::TSEnumDeclaration(enumeration) => {
            let scope = state
                .source_decl_at(enumeration.id.span.start, DeclarationKind::Enum)
                .and_then(|declaration| scopes.get(&declaration).copied());
            if let Some(scope) = scope {
                bind_enum_declaration(state, scope, enumeration);
            }
        }
        Declaration::TSModuleDeclaration(declaration) => {
            bind_selected_namespace_module_body(state, declaration, scopes)
        }
        Declaration::TSGlobalDeclaration(global) => {
            bind_selected_namespace_value_statements(state, &global.body.body, scopes)
        }
        _ => {}
    }
}

fn bind_selected_namespace_variable(
    state: &mut BindState,
    declaration: &oxc_ast::ast::VariableDeclaration<'_>,
    scopes: &FxHashMap<DeclId, ScopeId>,
) {
    for declarator in &declaration.declarations {
        let scope = declarator
            .id
            .get_binding_identifiers()
            .into_iter()
            .find_map(|identifier| {
                state
                    .source_decl_at(identifier.span.start, DeclarationKind::Variable)
                    .and_then(|declaration| scopes.get(&declaration).copied())
            });
        if let Some(scope) = scope {
            bind_declarator(state, scope, declaration.kind, declarator);
        }
    }
}

fn bind_selected_namespace_module_body(
    state: &mut BindState,
    declaration: &TSModuleDeclaration<'_>,
    scopes: &FxHashMap<DeclId, ScopeId>,
) {
    match &declaration.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            bind_selected_namespace_value_statements(state, &block.body, scopes)
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
            bind_selected_namespace_module_body(state, nested, scopes)
        }
        None => {}
    }
}

pub(super) fn resolve_local_ambient_export_alias_targets(state: &mut BindState) {
    let member_base = state.namespaces.members.base_len();
    #[cfg(any(test, feature = "test-utils"))]
    record_finalization_alias_member_rows(state.namespaces.members.local_len());
    let candidates = state
        .namespaces
        .members
        .local_iter()
        .enumerate()
        .filter(|(_, member)| {
            matches!(member.owner, NamespaceMemberOwner::Fragment(_))
                && member.kind == MergeDeclarationKind::DeferredExport
                && member.declaration.is_none()
                && member.alias_context == Some(AliasContext::ValidAmbient)
                && member.module_specifier.is_none()
        })
        .filter_map(|(local_index, member)| {
            let NamespaceMemberOwner::Fragment(fragment) = member.owner else {
                return None;
            };
            Some((
                member_base + local_index,
                fragment,
                member.local_name.as_ref()?.text().to_string(),
            ))
        })
        .collect::<Vec<_>>();

    for (index, fragment, local_name) in candidates {
        let Some(fragment) = state.namespaces.fragment(fragment) else {
            continue;
        };
        let private_scope = fragment.private_scope;
        let public_scope = state
            .namespaces
            .get(fragment.namespace)
            .map(|namespace| namespace.public_scope);
        let local_symbol =
            local_declaration_symbol(state, private_scope, &local_name).or_else(|| {
                public_scope.and_then(|scope| local_declaration_symbol(state, scope, &local_name))
            });
        if let Some(member) = state.namespaces.members.get_mut_local(index) {
            member.local_symbol = local_symbol;
        }
    }
}

fn local_declaration_symbol(state: &BindState, scope: ScopeId, name: &str) -> Option<SymbolId> {
    state
        .graph
        .get(scope)
        .and_then(|scope| scope.lookup_local(name))
        .filter(|symbol| {
            state
                .symbols
                .get(*symbol)
                .is_some_and(|symbol| !symbol.declarations.is_empty())
        })
}

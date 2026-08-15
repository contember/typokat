use super::models_ids::*;
use crate::binder::bind::{FrozenPrefixWrite, FrozenPrefixWriteSite};
use crate::binder::declaration::{DeclId, ValueStorageId};
use crate::binder::roots::{LibraryRootProjection, RootNameRow};
use crate::binder::scope::{FrozenScopeWrite, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{Symbol, SymbolId, SymbolTable};
use crate::source::CompilationOrigin;
use crate::types::layered::{LayeredMap, LayeredVec};
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;
use std::sync::Arc;

/// Canonical namespace, merge, ambient, and global publication metadata.
#[derive(Default)]
pub struct NamespaceTable {
    pub(super) namespaces: LayeredVec<Namespace>,
    pub(super) aggregate_instance_states: LayeredVec<NamespaceInstanceState>,
    pub(super) standalone_value_storages: LayeredVec<Option<ValueStorageId>>,
    pub(super) fragments: LayeredVec<NamespaceFragment>,
    pub(super) members: LayeredVec<NamespaceMember>,
    pub(super) namespace_keys: LayeredMap<NamespaceKey, NamespaceId>,
    pub(super) canonical_namespaces: LayeredVec<NamespaceId>,
    /// Interned merge-group names, indexed by [`NameId`]. The `Arc` is the single owner of the
    /// text: `names`, `name_ids` and every `MergeRecord` share one allocation per name.
    names: LayeredVec<Arc<str>>,
    /// Reverse index for [`NamespaceTable::intern_name`], shared by frozen-prefix forks.
    name_ids: LayeredMap<Arc<str>, NameId>,
    pub(super) placements: LayeredMap<MergeKey, Arc<Vec<MergeParticipant>>>,
    /// Row index of every placement participant, keyed by its group and declaration, so finding
    /// the row a declaration already owns is a lookup instead of a scan of the group.
    /// Not layered — a sealed group can never take another row, so this only ever describes
    /// rows a build can still reach.
    pub(super) placement_rows: FxHashMap<(MergeKey, DeclId), usize>,
    /// How many of each group's rows [`NamespaceTable::placement_rows`] already covers. Rows a
    /// restored or forked table brought with it are folded in on first touch, so a miss in that
    /// index is proof the group holds no row for the declaration, never a reason to scan it.
    pub(super) placement_rows_indexed: FxHashMap<MergeKey, usize>,
    pub(super) merges: LayeredVec<MergeRecord>,
    pub(super) merge_indices: LayeredMap<MergeKey, usize>,
    pub(super) merge_keys_by_declaration: LayeredMap<DeclId, MergeKey>,
    pub(super) standalone_storage_namespaces: LayeredMap<ValueStorageId, NamespaceId>,
    pub(super) declaration_owners_by_scope: LayeredMap<ScopeId, DeclarationOwner>,
    pub(super) fragments_by_declaration: LayeredMap<DeclId, NamespaceFragmentId>,
    pub(super) fragment_private_scopes_by_site: LayeredMap<(ScopeId, u32), ScopeId>,
    pub(super) global_augmentations_by_site: LayeredMap<(ScopeId, u32), GlobalAugmentationId>,
    pub(super) umd_exports_by_site: LayeredMap<(ScopeId, u32), usize>,
    pub(super) source_unit_indices_by_module: LayeredMap<ScopeId, usize>,
    pub(super) library_export_default_sites: LayeredMap<(SourceUnitKey, u32), bool>,
    pub(super) library_module_reporting_sites: LayeredMap<(ScopeId, u32), bool>,
    pub(super) globals: LayeredVec<GlobalAugmentation>,
    pub(super) deferred_modules: LayeredVec<DeferredAmbientModule>,
    pub(super) deferred_children: LayeredVec<DeferredAmbientChild>,
    pub(super) umd_exports: LayeredVec<UmdNamespaceExport>,
    pub(super) export_contexts: LayeredVec<ExportContext>,
    pub(super) source_units: LayeredVec<SourceUnitRecord>,
    compilation_origins_by_source: LayeredMap<SourceUnitKey, CompilationOrigin>,
    compilation_origin_conflicts: Vec<CompilationOriginIndexConflict>,
    pub(super) canonical_source_units: LayeredVec<usize>,
    pub(super) canonical_globals: LayeredVec<GlobalAugmentationId>,
    pub(super) canonical_deferred_modules: LayeredVec<DeferredModuleId>,
    pub(super) canonical_deferred_children: LayeredVec<usize>,
    pub(super) canonical_umd_exports: LayeredVec<usize>,
    pub(super) canonical_export_contexts: LayeredVec<ExportContextId>,
    pub(super) compilation_global: Option<ScopeId>,
    pub(super) script_namespace_root: Option<ScopeId>,
    pub(super) library_shared_globals: bool,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct NamespacePrimaryRows {
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) standalone_value_storages: Vec<Option<ValueStorageId>>,
    pub(crate) fragments: Vec<NamespaceFragment>,
    pub(crate) members: Vec<NamespaceMember>,
    pub(crate) placements: Vec<(DeclarationOwner, String, Vec<MergeParticipant>)>,
    pub(crate) globals: Vec<GlobalAugmentation>,
    pub(crate) deferred_modules: Vec<DeferredAmbientModule>,
    pub(crate) deferred_children: Vec<DeferredAmbientChild>,
    pub(crate) umd_exports: Vec<UmdNamespaceExport>,
    pub(crate) export_contexts: Vec<ExportContext>,
    pub(crate) source_units: Vec<SourceUnitRecord>,
    pub(crate) compilation_global: Option<ScopeId>,
    pub(crate) script_namespace_root: Option<ScopeId>,
    pub(crate) library_shared_globals: bool,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Copy, Clone, Default)]
pub(crate) struct NamespaceReferenceOffsets {
    pub(crate) placements: usize,
    pub(crate) deferred_children: usize,
    pub(crate) umd_exports: usize,
    pub(crate) canonical_namespaces: usize,
    pub(crate) canonical_source_units: usize,
    pub(crate) canonical_globals: usize,
    pub(crate) canonical_deferred_modules: usize,
    pub(crate) canonical_deferred_children: usize,
    pub(crate) canonical_umd_exports: usize,
    pub(crate) canonical_export_contexts: usize,
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) struct NamespaceReferenceRows {
    pub(crate) primary: NamespacePrimaryRows,
    pub(crate) offsets: NamespaceReferenceOffsets,
    pub(crate) canonical_namespaces: Vec<u32>,
    pub(crate) canonical_source_units: Vec<u32>,
    pub(crate) canonical_globals: Vec<u32>,
    pub(crate) canonical_deferred_modules: Vec<u32>,
    pub(crate) canonical_deferred_children: Vec<u32>,
    pub(crate) canonical_umd_exports: Vec<u32>,
    pub(crate) canonical_export_contexts: Vec<u32>,
}

pub(super) fn rebuild_canonical_suffix<T, LibraryKey: Ord, UserKey: Ord>(
    canonical: &mut LayeredVec<T>,
    source_range: std::ops::Range<usize>,
    mut row: impl FnMut(usize) -> T,
    library_order: bool,
    mut library_key: impl FnMut(&T) -> LibraryKey,
    mut user_key: impl FnMut(&T) -> UserKey,
) {
    canonical.clear_local();
    for index in source_range {
        canonical.push_local(row(index));
    }
    if library_order {
        canonical.local_slice_mut().sort_by_key(&mut library_key);
    } else {
        canonical.local_slice_mut().sort_by_key(&mut user_key);
    }
}

impl NamespaceTable {
    /// Store `name` once and hand back its id. A name the table already holds costs a hash
    /// lookup and nothing else — no allocation, no second copy of the text.
    pub(super) fn intern_name(&mut self, name: &str) -> NameId {
        if let Some(id) = self.name_ids.get(name) {
            return *id;
        }
        let text: Arc<str> = Arc::from(name);
        #[cfg(any(test, feature = "test-utils"))]
        record_merge_key_name_allocation(name);
        let id = NameId(u32::try_from(self.names.len()).expect("interned name count fits u32"));
        self.names.push_local(Arc::clone(&text));
        let _ = self.name_ids.insert_local(text, id);
        id
    }

    /// The id of an already-stored name. A lookup for a name no declaration ever used has no
    /// merge group either, so `None` is the answer, not a reason to intern it.
    pub(super) fn name_id(&self, name: &str) -> Option<NameId> {
        self.name_ids.get(name).copied()
    }

    pub(super) fn name_text(&self, id: NameId) -> &Arc<str> {
        self.names.get(id.index()).expect("interned name exists")
    }

    pub(super) fn uses_library_shared_globals(&self) -> bool {
        self.library_shared_globals
    }

    pub fn get(&self, id: NamespaceId) -> Option<&Namespace> {
        self.namespaces.get(id.index())
    }

    pub fn fragment(&self, id: NamespaceFragmentId) -> Option<&NamespaceFragment> {
        self.fragments.get(id.index())
    }

    pub fn member(&self, id: NamespaceMemberId) -> Option<&NamespaceMember> {
        self.members.get(id.index())
    }

    pub fn namespaces(&self) -> impl Iterator<Item = &Namespace> {
        self.canonical_namespaces
            .iter()
            .filter_map(|id| self.namespaces.get(id.index()))
    }

    pub fn prefix_namespace_replacements(&self) -> impl Iterator<Item = NamespaceId> + '_ {
        self.namespaces
            .prefix_replacements()
            .filter_map(|(index, _)| u32::try_from(index).ok().map(NamespaceId))
    }

    /// Whole-group instantiation after joining every reopening.
    pub fn aggregate_instance_state(&self, id: NamespaceId) -> Option<NamespaceInstanceState> {
        self.aggregate_instance_states.get(id.index()).copied()
    }

    /// Dormant owner slot for an admitted instantiated standalone namespace.
    pub fn standalone_value_storage(&self, id: NamespaceId) -> Option<ValueStorageId> {
        self.standalone_value_storages
            .get(id.index())
            .copied()
            .flatten()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn fragments(&self) -> impl Iterator<Item = &NamespaceFragment> {
        self.fragments.iter()
    }

    pub fn members(&self) -> impl Iterator<Item = &NamespaceMember> {
        self.members.iter()
    }

    pub fn merges(&self) -> impl Iterator<Item = &MergeRecord> {
        self.merges.iter().inspect(|_| {
            #[cfg(any(test, feature = "test-utils"))]
            record_continuation_merge_row();
        })
    }

    pub fn local_merges(&self) -> impl Iterator<Item = &MergeRecord> {
        self.merges
            .changed_iter()
            .map(|(_, record)| record)
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_merge_row();
            })
    }

    pub(crate) fn is_admitted_compilation_global_name(&self, name: &str) -> bool {
        self.compilation_global_merge_disposition(name) == Some(MergeDisposition::Admitted)
    }

    pub fn compilation_global_merge_disposition(&self, name: &str) -> Option<MergeDisposition> {
        let name = self.name_id(name)?;
        let key = MergeKey {
            owner: DeclarationOwner::CompilationGlobal,
            name,
        };
        self.merge_indices
            .get(&key)
            .and_then(|index| self.merges.get(*index))
            .map(|record| record.classification.disposition)
    }

    pub fn merge_disposition_for_declaration(
        &self,
        declaration: DeclId,
    ) -> Option<MergeDisposition> {
        let key = self.merge_keys_by_declaration.get(&declaration)?;
        self.merge_indices
            .get(key)
            .and_then(|index| self.merges.get(*index))
            .map(|record| record.classification.disposition)
    }

    pub fn declaration_owner_for_scope(&self, scope: ScopeId) -> DeclarationOwner {
        self.declaration_owners_by_scope
            .get(&scope)
            .copied()
            .unwrap_or(DeclarationOwner::Lexical(scope))
    }

    /// Exact source-ordered placement outcomes ready for checker emission.
    pub fn placement_issues(&self) -> impl Iterator<Item = &PlacementIssue> {
        let mut issues = self
            .merges
            .iter()
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_placement_merge_row();
            })
            .flat_map(|record| record.placement_issues.iter())
            .collect::<Vec<_>>();
        issues.sort_by_key(|issue| (issue.source, issue.origin, issue.span.start, issue.owner.0));
        issues.into_iter()
    }

    pub fn local_placement_issues(&self) -> impl Iterator<Item = &PlacementIssue> {
        let mut issues = self
            .merges
            .changed_iter()
            .map(|(_, record)| record)
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_placement_merge_row();
            })
            .flat_map(|record| record.placement_issues.iter())
            .collect::<Vec<_>>();
        issues.sort_by_key(|issue| (issue.source, issue.origin, issue.span.start, issue.owner.0));
        issues.into_iter()
    }

    pub(super) fn source_unit_for_module(&self, module: ScopeId) -> Option<&SourceUnitRecord> {
        let index = self.source_unit_indices_by_module.get(&module).copied()?;
        self.source_units
            .get(index)
            .filter(|unit| unit.module == module)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn source_units(&self) -> impl Iterator<Item = &SourceUnitRecord> {
        self.canonical_source_units
            .iter()
            .filter_map(|index| self.source_units.get(*index))
    }

    pub fn compilation_origin_for_source(
        &self,
        source: SourceUnitKey,
    ) -> Option<CompilationOrigin> {
        self.compilation_origins_by_source.get(&source).copied()
    }

    pub fn compilation_origin_conflicts(&self) -> &[CompilationOriginIndexConflict] {
        &self.compilation_origin_conflicts
    }

    pub fn validate_compilation_origin_index(&self) -> Result<(), CompilationOriginIndexConflict> {
        self.compilation_origin_conflicts
            .first()
            .copied()
            .map_or(Ok(()), Err)
    }

    pub(super) fn record_compilation_origin(
        &mut self,
        source: SourceUnitKey,
        origin: CompilationOrigin,
    ) {
        if let Some(retained) = self.compilation_origins_by_source.get(&source).copied() {
            if retained != origin {
                self.compilation_origin_conflicts.push(
                    CompilationOriginIndexConflict::ConflictingOrigin {
                        source,
                        retained,
                        attempted: origin,
                    },
                );
                return;
            }
        }
        if self
            .compilation_origins_by_source
            .insert_local(source, origin)
            .is_err()
        {
            self.compilation_origin_conflicts.push(
                CompilationOriginIndexConflict::SealedPrefixWrite {
                    source,
                    attempted: origin,
                },
            );
        }
    }

    pub fn globals(&self) -> impl Iterator<Item = &GlobalAugmentation> {
        self.canonical_globals
            .iter()
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_global_row();
            })
            .filter_map(|id| self.globals.get(id.index()))
    }

    pub fn local_globals(&self) -> impl Iterator<Item = &GlobalAugmentation> {
        self.canonical_globals
            .local_iter()
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_global_row();
            })
            .filter_map(|id| self.globals.get(id.index()))
    }

    /// Freeze legal global publication, then connect every user module in one cutover.
    pub(crate) fn finalize_global_scopes(
        &self,
        graph: &mut ScopeGraph,
        symbols: &mut SymbolTable,
        refused: &mut Vec<FrozenPrefixWrite>,
        mut root_projection: Option<&mut LibraryRootProjection>,
    ) -> Vec<RootNameRow> {
        let compilation_global = self
            .compilation_global
            .expect("compilation-global scope allocated");
        let script_namespace_root = self
            .script_namespace_root
            .expect("script namespace root scope allocated");
        let script_root = graph
            .get(script_namespace_root)
            .expect("script namespace root scope exists");
        assert_eq!(script_root.kind, ScopeKind::ScriptNamespaceRoot);
        assert_eq!(script_root.parent, Some(compilation_global));
        let mut unsafe_names = BTreeSet::new();
        let mut namespace_contributors = BTreeSet::new();
        let mut classify_record = |record: &MergeRecord| {
            if let Some(projection) = root_projection.as_deref_mut() {
                projection.observe_finalized_global_merge(
                    record,
                    self,
                    &mut namespace_contributors,
                );
            }
            if record.owner != DeclarationOwner::CompilationGlobal {
                return;
            }
            let safe = if self.uses_library_shared_globals() {
                record.classification.disposition == MergeDisposition::Admitted
            } else {
                self.raw_global_record_is_publishable(record)
            };
            if !safe {
                unsafe_names.insert(record.name.to_string());
            }
        };
        if self.merges.prefix_overrides_enabled() {
            for (_, record) in self.merges.changed_iter() {
                classify_record(record);
            }
        } else {
            for record in self.merges.iter() {
                classify_record(record);
            }
        }
        if let Some(projection) = root_projection.as_deref_mut() {
            projection.finish_namespace_normalization(&namespace_contributors);
        }

        let (safe_symbols, blocked_names) = {
            let mut blocked_names = Vec::new();
            for name in unsafe_names {
                match graph.remove(compilation_global, &name) {
                    Ok(Some(previous)) => blocked_names.push((name, previous)),
                    Ok(None) => {}
                    Err(FrozenScopeWrite) => refused.push(FrozenPrefixWrite {
                        declaration: None,
                        site: FrozenPrefixWriteSite::ScopeDeclaration,
                    }),
                }
            }
            let Some(global) = graph.get(compilation_global) else {
                refused.push(FrozenPrefixWrite {
                    declaration: None,
                    site: FrozenPrefixWriteSite::ScopeDeclaration,
                });
                return Vec::new();
            };
            let safe_symbols: Vec<(String, SymbolId)> = if global.symbols.prefix_overrides_enabled()
            {
                global
                    .symbols
                    .changed_iter()
                    .map(|(name, symbol)| (name.clone(), *symbol))
                    .collect()
            } else {
                global
                    .symbols
                    .iter()
                    .map(|(name, symbol)| (name.clone(), *symbol))
                    .collect()
            };
            (safe_symbols, blocked_names)
        };

        // Prevent deferred globals from falling through to module-local names.
        let blocked_symbols = blocked_names
            .into_iter()
            .map(|(name, previous)| {
                let mut symbol = Symbol::new(name.clone());
                symbol.blocks_type_lookup = true;
                symbol.blocks_value_lookup = true;
                symbol.blocks_namespace_lookup = true;
                (name, previous, symbols.push(symbol))
            })
            .collect::<Vec<_>>();

        for global in self
            .globals
            .iter()
            .filter(|global| global.issues.is_empty())
        {
            for (name, symbol) in &safe_symbols {
                // An overlay scope is always delta-side; a refusal is recorded, never dropped.
                match graph.declare(global.overlay_scope, name.clone(), *symbol) {
                    Ok(replaced) => assert_idempotent_overlay_publication(
                        replaced,
                        *symbol,
                        "global overlay cannot replace a different frozen symbol",
                    ),
                    Err(FrozenScopeWrite) => refused.push(FrozenPrefixWrite {
                        declaration: None,
                        site: FrozenPrefixWriteSite::ScopeDeclaration,
                    }),
                }
            }
            for (name, previous, symbol) in &blocked_symbols {
                match graph.declare(global.overlay_scope, name.clone(), *symbol) {
                    Ok(replaced) => assert!(
                        replaced
                            .is_none_or(|existing| existing == *symbol || existing == *previous),
                        "global overlay blocker cannot replace an unrelated symbol"
                    ),
                    Err(FrozenScopeWrite) => refused.push(FrozenPrefixWrite {
                        declaration: None,
                        site: FrozenPrefixWriteSite::ScopeDeclaration,
                    }),
                }
            }
        }

        self.publish_local_umd_export_assignment_aliases(graph, script_namespace_root, refused);
        for unit in &self.source_units {
            let module = graph
                .get_mut(unit.module)
                .expect("user module scope exists");
            assert_eq!(module.kind, ScopeKind::Module);
            module.parent = Some(script_namespace_root);
        }
        let mut root_rows = Vec::with_capacity(safe_symbols.len());
        for (name, symbol) in safe_symbols {
            if let Some(symbol) = symbols.get(symbol) {
                root_rows.push(RootNameRow {
                    name,
                    value: symbol.value,
                    ty: symbol.ty,
                    namespace: symbol.ns,
                });
            } else if let Some(projection) = root_projection.as_deref_mut() {
                projection.normalization_issue_count =
                    projection.normalization_issue_count.saturating_add(1);
            }
        }
        root_rows.sort_by(|left, right| left.name.cmp(&right.name));
        root_rows
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn deferred_modules(&self) -> impl Iterator<Item = &DeferredAmbientModule> {
        self.canonical_deferred_modules
            .iter()
            .filter_map(|id| self.deferred_modules.get(id.index()))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn deferred_children(&self) -> impl Iterator<Item = &DeferredAmbientChild> {
        self.canonical_deferred_children
            .iter()
            .filter_map(|index| self.deferred_children.get(*index))
    }

    pub fn umd_exports(&self) -> impl Iterator<Item = &UmdNamespaceExport> {
        self.canonical_umd_exports
            .iter()
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_umd_row();
            })
            .filter_map(|index| self.umd_exports.get(*index))
    }

    pub fn local_umd_exports(&self) -> impl Iterator<Item = &UmdNamespaceExport> {
        self.canonical_umd_exports
            .local_iter()
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_umd_row();
            })
            .filter_map(|index| self.umd_exports.get(*index))
    }

    pub(crate) fn publish_local_umd_export_assignment_aliases(
        &self,
        graph: &mut ScopeGraph,
        script_namespace_root: ScopeId,
        refused: &mut Vec<FrozenPrefixWrite>,
    ) {
        let aliases = self
            .local_umd_exports()
            .filter(|export| {
                export.context == UmdContext::DeferredValidBacklog15
                    && export.export_assignment_target.as_deref() == Some(export.name.as_str())
            })
            .filter_map(|export| {
                graph
                    .get(export.module)
                    .and_then(|module| module.lookup_local(&export.name))
                    .map(|symbol| (export.name.clone(), symbol))
            })
            .collect::<Vec<_>>();
        for (name, symbol) in aliases {
            match graph.declare(script_namespace_root, name, symbol) {
                Ok(replaced) => assert_idempotent_overlay_publication(
                    replaced,
                    symbol,
                    "UMD export alias cannot replace a different script-root symbol",
                ),
                Err(FrozenScopeWrite) => refused.push(FrozenPrefixWrite {
                    declaration: None,
                    site: FrozenPrefixWriteSite::ScopeDeclaration,
                }),
            }
        }
    }

    pub fn export_contexts(&self) -> impl Iterator<Item = &ExportContext> {
        self.canonical_export_contexts
            .iter()
            .filter_map(|id| self.export_contexts.get(id.index()))
    }

    pub fn len(&self) -> usize {
        self.namespaces.len()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_namespaces(&self) -> impl Iterator<Item = (NamespaceId, &Namespace)> {
        let base_len = self.namespaces.base_len();
        self.namespaces
            .local_iter()
            .enumerate()
            .map(move |(index, namespace)| {
                let id = u32::try_from(base_len + index).expect("namespace id fits u32");
                (NamespaceId(id), namespace)
            })
    }

    pub fn is_empty(&self) -> bool {
        self.namespaces.is_empty()
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.validate_compilation_origin_index()
            .map_err(|_| "namespace source-origin index contains a conflict")?;
        self.namespaces.freeze_as_base()?;
        self.aggregate_instance_states.freeze_as_base()?;
        self.standalone_value_storages.freeze_as_base()?;
        self.fragments.freeze_as_base()?;
        self.members.freeze_as_base()?;
        self.namespace_keys.freeze_as_base()?;
        self.canonical_namespaces.freeze_as_base()?;
        self.names.freeze_as_base()?;
        self.name_ids.freeze_as_base()?;
        self.placements.freeze_as_base()?;
        self.merges.freeze_as_base()?;
        self.merge_indices.freeze_as_base()?;
        self.merge_keys_by_declaration.freeze_as_base()?;
        self.standalone_storage_namespaces.freeze_as_base()?;
        self.declaration_owners_by_scope.freeze_as_base()?;
        self.fragments_by_declaration.freeze_as_base()?;
        self.fragment_private_scopes_by_site.freeze_as_base()?;
        self.global_augmentations_by_site.freeze_as_base()?;
        self.umd_exports_by_site.freeze_as_base()?;
        self.source_unit_indices_by_module.freeze_as_base()?;
        self.library_export_default_sites.freeze_as_base()?;
        self.library_module_reporting_sites.freeze_as_base()?;
        self.globals.freeze_as_base()?;
        self.deferred_modules.freeze_as_base()?;
        self.deferred_children.freeze_as_base()?;
        self.umd_exports.freeze_as_base()?;
        self.export_contexts.freeze_as_base()?;
        self.source_units.freeze_as_base()?;
        self.compilation_origins_by_source.freeze_as_base()?;
        self.canonical_source_units.freeze_as_base()?;
        self.canonical_globals.freeze_as_base()?;
        self.canonical_deferred_modules.freeze_as_base()?;
        self.canonical_deferred_children.freeze_as_base()?;
        self.canonical_umd_exports.freeze_as_base()?;
        self.canonical_export_contexts.freeze_as_base()?;
        Ok(())
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            namespaces: self.namespaces.fork_delta()?,
            aggregate_instance_states: self.aggregate_instance_states.fork_delta()?,
            standalone_value_storages: self.standalone_value_storages.fork_delta()?,
            fragments: self.fragments.fork_delta()?,
            members: self.members.fork_delta()?,
            namespace_keys: self.namespace_keys.fork_delta()?,
            canonical_namespaces: self.canonical_namespaces.fork_delta()?,
            names: self.names.fork_delta()?,
            name_ids: self.name_ids.fork_delta()?,
            placements: self.placements.fork_delta()?,
            placement_rows: FxHashMap::default(),
            placement_rows_indexed: FxHashMap::default(),
            merges: self.merges.fork_delta()?,
            merge_indices: self.merge_indices.fork_delta()?,
            merge_keys_by_declaration: self.merge_keys_by_declaration.fork_delta()?,
            standalone_storage_namespaces: self.standalone_storage_namespaces.fork_delta()?,
            declaration_owners_by_scope: self.declaration_owners_by_scope.fork_delta()?,
            fragments_by_declaration: self.fragments_by_declaration.fork_delta()?,
            fragment_private_scopes_by_site: self.fragment_private_scopes_by_site.fork_delta()?,
            global_augmentations_by_site: self.global_augmentations_by_site.fork_delta()?,
            umd_exports_by_site: self.umd_exports_by_site.fork_delta()?,
            source_unit_indices_by_module: self.source_unit_indices_by_module.fork_delta()?,
            library_export_default_sites: self.library_export_default_sites.fork_delta()?,
            library_module_reporting_sites: self.library_module_reporting_sites.fork_delta()?,
            globals: self.globals.fork_delta()?,
            deferred_modules: self.deferred_modules.fork_delta()?,
            deferred_children: self.deferred_children.fork_delta()?,
            umd_exports: self.umd_exports.fork_delta()?,
            export_contexts: self.export_contexts.fork_delta()?,
            source_units: self.source_units.fork_delta()?,
            compilation_origins_by_source: self.compilation_origins_by_source.fork_delta()?,
            compilation_origin_conflicts: Vec::new(),
            canonical_source_units: self.canonical_source_units.fork_delta()?,
            canonical_globals: self.canonical_globals.fork_delta()?,
            canonical_deferred_modules: self.canonical_deferred_modules.fork_delta()?,
            canonical_deferred_children: self.canonical_deferred_children.fork_delta()?,
            canonical_umd_exports: self.canonical_umd_exports.fork_delta()?,
            canonical_export_contexts: self.canonical_export_contexts.fork_delta()?,
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_shared_globals: self.library_shared_globals,
        })
    }

    pub(crate) fn fork_sparse_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            namespaces: self.namespaces.fork_sparse_delta()?,
            aggregate_instance_states: self.aggregate_instance_states.fork_sparse_delta()?,
            standalone_value_storages: self.standalone_value_storages.fork_sparse_delta()?,
            fragments: self.fragments.fork_sparse_delta()?,
            members: self.members.fork_sparse_delta()?,
            namespace_keys: self.namespace_keys.fork_delta()?,
            canonical_namespaces: self.canonical_namespaces.fork_sparse_delta()?,
            names: self.names.fork_sparse_delta()?,
            name_ids: self.name_ids.fork_delta()?,
            placements: self.placements.fork_sparse_delta()?,
            placement_rows: FxHashMap::default(),
            placement_rows_indexed: FxHashMap::default(),
            merges: self.merges.fork_sparse_delta()?,
            merge_indices: self.merge_indices.fork_sparse_delta()?,
            merge_keys_by_declaration: self.merge_keys_by_declaration.fork_sparse_delta()?,
            standalone_storage_namespaces: self.standalone_storage_namespaces.fork_delta()?,
            declaration_owners_by_scope: self.declaration_owners_by_scope.fork_delta()?,
            fragments_by_declaration: self.fragments_by_declaration.fork_delta()?,
            fragment_private_scopes_by_site: self.fragment_private_scopes_by_site.fork_delta()?,
            global_augmentations_by_site: self.global_augmentations_by_site.fork_delta()?,
            umd_exports_by_site: self.umd_exports_by_site.fork_delta()?,
            source_unit_indices_by_module: self.source_unit_indices_by_module.fork_delta()?,
            library_export_default_sites: self.library_export_default_sites.fork_delta()?,
            library_module_reporting_sites: self.library_module_reporting_sites.fork_delta()?,
            globals: self.globals.fork_sparse_delta()?,
            deferred_modules: self.deferred_modules.fork_sparse_delta()?,
            deferred_children: self.deferred_children.fork_sparse_delta()?,
            umd_exports: self.umd_exports.fork_sparse_delta()?,
            export_contexts: self.export_contexts.fork_sparse_delta()?,
            source_units: self.source_units.fork_sparse_delta()?,
            compilation_origins_by_source: self
                .compilation_origins_by_source
                .fork_sparse_delta()?,
            compilation_origin_conflicts: Vec::new(),
            canonical_source_units: self.canonical_source_units.fork_sparse_delta()?,
            canonical_globals: self.canonical_globals.fork_sparse_delta()?,
            canonical_deferred_modules: self.canonical_deferred_modules.fork_sparse_delta()?,
            canonical_deferred_children: self.canonical_deferred_children.fork_sparse_delta()?,
            canonical_umd_exports: self.canonical_umd_exports.fork_sparse_delta()?,
            canonical_export_contexts: self.canonical_export_contexts.fork_sparse_delta()?,
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_shared_globals: self.library_shared_globals,
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn shares_base_storage_with(&self, other: &Self) -> bool {
        self.namespaces.shares_base_with(&other.namespaces)
            && self
                .aggregate_instance_states
                .shares_base_with(&other.aggregate_instance_states)
            && self
                .standalone_value_storages
                .shares_base_with(&other.standalone_value_storages)
            && self.fragments.shares_base_with(&other.fragments)
            && self.members.shares_base_with(&other.members)
            && self.namespace_keys.shares_base_with(&other.namespace_keys)
            && self
                .canonical_namespaces
                .shares_base_with(&other.canonical_namespaces)
            && self.names.shares_base_with(&other.names)
            && self.name_ids.shares_base_with(&other.name_ids)
            && self.placements.shares_base_with(&other.placements)
            && self.merges.shares_base_with(&other.merges)
            && self.merge_indices.shares_base_with(&other.merge_indices)
            && self
                .merge_keys_by_declaration
                .shares_base_with(&other.merge_keys_by_declaration)
            && self
                .standalone_storage_namespaces
                .shares_base_with(&other.standalone_storage_namespaces)
            && self
                .declaration_owners_by_scope
                .shares_base_with(&other.declaration_owners_by_scope)
            && self
                .fragments_by_declaration
                .shares_base_with(&other.fragments_by_declaration)
            && self
                .fragment_private_scopes_by_site
                .shares_base_with(&other.fragment_private_scopes_by_site)
            && self
                .global_augmentations_by_site
                .shares_base_with(&other.global_augmentations_by_site)
            && self
                .umd_exports_by_site
                .shares_base_with(&other.umd_exports_by_site)
            && self
                .source_unit_indices_by_module
                .shares_base_with(&other.source_unit_indices_by_module)
            && self
                .library_export_default_sites
                .shares_base_with(&other.library_export_default_sites)
            && self
                .library_module_reporting_sites
                .shares_base_with(&other.library_module_reporting_sites)
            && self.globals.shares_base_with(&other.globals)
            && self
                .deferred_modules
                .shares_base_with(&other.deferred_modules)
            && self
                .deferred_children
                .shares_base_with(&other.deferred_children)
            && self.umd_exports.shares_base_with(&other.umd_exports)
            && self
                .export_contexts
                .shares_base_with(&other.export_contexts)
            && self.source_units.shares_base_with(&other.source_units)
            && self
                .compilation_origins_by_source
                .shares_base_with(&other.compilation_origins_by_source)
            && self
                .canonical_source_units
                .shares_base_with(&other.canonical_source_units)
            && self
                .canonical_globals
                .shares_base_with(&other.canonical_globals)
            && self
                .canonical_deferred_modules
                .shares_base_with(&other.canonical_deferred_modules)
            && self
                .canonical_deferred_children
                .shares_base_with(&other.canonical_deferred_children)
            && self
                .canonical_umd_exports
                .shares_base_with(&other.canonical_umd_exports)
            && self
                .canonical_export_contexts
                .shares_base_with(&other.canonical_export_contexts)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn base_family_sharing_with(&self, other: &Self) -> [bool; 2] {
        let indexes = self
            .aggregate_instance_states
            .shares_base_with(&other.aggregate_instance_states)
            && self
                .standalone_value_storages
                .shares_base_with(&other.standalone_value_storages)
            && self.fragments.shares_base_with(&other.fragments)
            && self.members.shares_base_with(&other.members)
            && self.namespace_keys.shares_base_with(&other.namespace_keys)
            && self
                .canonical_namespaces
                .shares_base_with(&other.canonical_namespaces)
            && self.name_ids.shares_base_with(&other.name_ids)
            && self.placements.shares_base_with(&other.placements)
            && self.merges.shares_base_with(&other.merges)
            && self.merge_indices.shares_base_with(&other.merge_indices)
            && self
                .merge_keys_by_declaration
                .shares_base_with(&other.merge_keys_by_declaration)
            && self
                .standalone_storage_namespaces
                .shares_base_with(&other.standalone_storage_namespaces)
            && self
                .declaration_owners_by_scope
                .shares_base_with(&other.declaration_owners_by_scope)
            && self
                .fragments_by_declaration
                .shares_base_with(&other.fragments_by_declaration)
            && self
                .fragment_private_scopes_by_site
                .shares_base_with(&other.fragment_private_scopes_by_site)
            && self.globals.shares_base_with(&other.globals)
            && self
                .deferred_modules
                .shares_base_with(&other.deferred_modules)
            && self
                .deferred_children
                .shares_base_with(&other.deferred_children)
            && self.umd_exports.shares_base_with(&other.umd_exports)
            && self
                .export_contexts
                .shares_base_with(&other.export_contexts)
            && self.source_units.shares_base_with(&other.source_units)
            && self
                .compilation_origins_by_source
                .shares_base_with(&other.compilation_origins_by_source)
            && self
                .canonical_source_units
                .shares_base_with(&other.canonical_source_units)
            && self
                .canonical_globals
                .shares_base_with(&other.canonical_globals)
            && self
                .canonical_deferred_modules
                .shares_base_with(&other.canonical_deferred_modules)
            && self
                .canonical_deferred_children
                .shares_base_with(&other.canonical_deferred_children)
            && self
                .canonical_umd_exports
                .shares_base_with(&other.canonical_umd_exports)
            && self
                .canonical_export_contexts
                .shares_base_with(&other.canonical_export_contexts)
            && self
                .global_augmentations_by_site
                .shares_base_with(&other.global_augmentations_by_site)
            && self
                .umd_exports_by_site
                .shares_base_with(&other.umd_exports_by_site)
            && self
                .source_unit_indices_by_module
                .shares_base_with(&other.source_unit_indices_by_module)
            && self
                .library_export_default_sites
                .shares_base_with(&other.library_export_default_sites)
            && self
                .library_module_reporting_sites
                .shares_base_with(&other.library_module_reporting_sites);
        [self.namespaces.shares_base_with(&other.namespaces), indexes]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_family_row_counts_for_test(&self) -> [usize; 2] {
        let indexes = self.aggregate_instance_states.local_len()
            + self.aggregate_instance_states.replacement_len()
            + self.standalone_value_storages.local_len()
            + self.standalone_value_storages.replacement_len()
            + self.fragments.local_len()
            + self.fragments.replacement_len()
            + self.members.local_len()
            + self.members.replacement_len()
            + self.namespace_keys.local_len()
            + self.namespace_keys.replacement_len()
            + self.canonical_namespaces.local_len()
            + self.canonical_namespaces.replacement_len()
            + self.name_ids.local_len()
            + self.name_ids.replacement_len()
            + self.placements.local_len()
            + self.placements.replacement_len()
            + self.merges.local_len()
            + self.merges.replacement_len()
            + self.merge_indices.local_len()
            + self.merge_indices.replacement_len()
            + self.merge_keys_by_declaration.local_len()
            + self.merge_keys_by_declaration.replacement_len()
            + self.standalone_storage_namespaces.local_len()
            + self.standalone_storage_namespaces.replacement_len()
            + self.declaration_owners_by_scope.local_len()
            + self.declaration_owners_by_scope.replacement_len()
            + self.fragments_by_declaration.local_len()
            + self.fragments_by_declaration.replacement_len()
            + self.fragment_private_scopes_by_site.local_len()
            + self.fragment_private_scopes_by_site.replacement_len()
            + self.globals.local_len()
            + self.globals.replacement_len()
            + self.deferred_modules.local_len()
            + self.deferred_modules.replacement_len()
            + self.deferred_children.local_len()
            + self.deferred_children.replacement_len()
            + self.umd_exports.local_len()
            + self.umd_exports.replacement_len()
            + self.export_contexts.local_len()
            + self.export_contexts.replacement_len()
            + self.source_units.local_len()
            + self.source_units.replacement_len()
            + self.compilation_origins_by_source.local_len()
            + self.compilation_origins_by_source.replacement_len()
            + self.canonical_source_units.local_len()
            + self.canonical_source_units.replacement_len()
            + self.canonical_globals.local_len()
            + self.canonical_globals.replacement_len()
            + self.canonical_deferred_modules.local_len()
            + self.canonical_deferred_modules.replacement_len()
            + self.canonical_deferred_children.local_len()
            + self.canonical_deferred_children.replacement_len()
            + self.canonical_umd_exports.local_len()
            + self.canonical_umd_exports.replacement_len()
            + self.canonical_export_contexts.local_len()
            + self.canonical_export_contexts.replacement_len()
            + self.global_augmentations_by_site.local_len()
            + self.global_augmentations_by_site.replacement_len()
            + self.umd_exports_by_site.local_len()
            + self.umd_exports_by_site.replacement_len()
            + self.source_unit_indices_by_module.local_len()
            + self.source_unit_indices_by_module.replacement_len()
            + self.library_export_default_sites.local_len()
            + self.library_export_default_sites.replacement_len()
            + self.library_module_reporting_sites.local_len();
        [
            self.namespaces.local_len() + self.namespaces.replacement_len(),
            indexes + self.library_module_reporting_sites.replacement_len(),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn primary_rows(&self) -> NamespacePrimaryRows {
        NamespacePrimaryRows {
            namespaces: self.namespaces.iter().cloned().collect(),
            standalone_value_storages: self.standalone_value_storages.iter().copied().collect(),
            fragments: self.fragments.iter().cloned().collect(),
            members: self.members.iter().cloned().collect(),
            placements: self
                .merges
                .iter()
                .map(|merge| {
                    (
                        merge.owner,
                        merge.name.to_string(),
                        (*merge.declarations).clone(),
                    )
                })
                .collect(),
            globals: self.globals.iter().cloned().collect(),
            deferred_modules: self.deferred_modules.iter().cloned().collect(),
            deferred_children: self.deferred_children.iter().cloned().collect(),
            umd_exports: self.umd_exports.iter().cloned().collect(),
            export_contexts: self.export_contexts.iter().cloned().collect(),
            source_units: self.source_units.iter().cloned().collect(),
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_shared_globals: self.library_shared_globals,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn reference_rows(&self, local_only: bool) -> NamespaceReferenceRows {
        if !local_only {
            return NamespaceReferenceRows {
                primary: self.primary_rows(),
                offsets: NamespaceReferenceOffsets::default(),
                canonical_namespaces: self
                    .canonical_namespaces
                    .iter()
                    .map(|namespace| namespace.0)
                    .collect(),
                canonical_source_units: self
                    .canonical_source_units
                    .iter()
                    .map(|index| self.source_units[*index].source.0)
                    .collect(),
                canonical_globals: self
                    .canonical_globals
                    .iter()
                    .map(|global| global.0)
                    .collect(),
                canonical_deferred_modules: self
                    .canonical_deferred_modules
                    .iter()
                    .map(|module| module.0)
                    .collect(),
                canonical_deferred_children: self
                    .canonical_deferred_children
                    .iter()
                    .map(|index| u32::try_from(*index).expect("deferred child index fits u32"))
                    .collect(),
                canonical_umd_exports: self
                    .canonical_umd_exports
                    .iter()
                    .map(|index| u32::try_from(*index).expect("UMD export index fits u32"))
                    .collect(),
                canonical_export_contexts: self
                    .canonical_export_contexts
                    .iter()
                    .map(|context| context.0)
                    .collect(),
            };
        }

        let primary = NamespacePrimaryRows {
            namespaces: self.namespaces.local_iter().cloned().collect(),
            standalone_value_storages: self
                .standalone_value_storages
                .local_iter()
                .copied()
                .collect(),
            fragments: self.fragments.local_iter().cloned().collect(),
            members: self.members.local_iter().cloned().collect(),
            placements: self
                .merges
                .local_iter()
                .map(|merge| {
                    (
                        merge.owner,
                        merge.name.to_string(),
                        (*merge.declarations).clone(),
                    )
                })
                .collect(),
            globals: self.globals.local_iter().cloned().collect(),
            deferred_modules: self.deferred_modules.local_iter().cloned().collect(),
            deferred_children: self.deferred_children.local_iter().cloned().collect(),
            umd_exports: self.umd_exports.local_iter().cloned().collect(),
            export_contexts: self.export_contexts.local_iter().cloned().collect(),
            source_units: self.source_units.local_iter().cloned().collect(),
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_shared_globals: self.library_shared_globals,
        };
        NamespaceReferenceRows {
            primary,
            offsets: NamespaceReferenceOffsets {
                placements: self.merges.base_len(),
                deferred_children: self.deferred_children.base_len(),
                umd_exports: self.umd_exports.base_len(),
                canonical_namespaces: self.canonical_namespaces.base_len(),
                canonical_source_units: self.canonical_source_units.base_len(),
                canonical_globals: self.canonical_globals.base_len(),
                canonical_deferred_modules: self.canonical_deferred_modules.base_len(),
                canonical_deferred_children: self.canonical_deferred_children.base_len(),
                canonical_umd_exports: self.canonical_umd_exports.base_len(),
                canonical_export_contexts: self.canonical_export_contexts.base_len(),
            },
            canonical_namespaces: self
                .canonical_namespaces
                .local_iter()
                .map(|namespace| namespace.0)
                .collect(),
            canonical_source_units: self
                .canonical_source_units
                .local_iter()
                .map(|index| self.source_units[*index].source.0)
                .collect(),
            canonical_globals: self
                .canonical_globals
                .local_iter()
                .map(|global| global.0)
                .collect(),
            canonical_deferred_modules: self
                .canonical_deferred_modules
                .local_iter()
                .map(|module| module.0)
                .collect(),
            canonical_deferred_children: self
                .canonical_deferred_children
                .local_iter()
                .map(|index| u32::try_from(*index).expect("deferred child index fits u32"))
                .collect(),
            canonical_umd_exports: self
                .canonical_umd_exports
                .local_iter()
                .map(|index| u32::try_from(*index).expect("UMD export index fits u32"))
                .collect(),
            canonical_export_contexts: self
                .canonical_export_contexts
                .local_iter()
                .map(|context| context.0)
                .collect(),
        }
    }
}

pub(super) fn assert_idempotent_overlay_publication(
    replaced: Option<SymbolId>,
    intended: SymbolId,
    message: &'static str,
) {
    assert!(
        replaced.is_none_or(|existing| existing == intended),
        "{message}"
    );
}

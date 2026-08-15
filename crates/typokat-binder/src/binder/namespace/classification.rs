use super::layered::{rebuild_canonical_suffix, NamespaceTable};
use super::models_ids::*;
use super::values::namespace_value_attachment_disposition;
use crate::source::CompilationOrigin;
use std::sync::Arc;

impl NamespaceTable {
    pub(super) fn classify(&mut self) -> Result<(), &'static str> {
        #[cfg(any(test, feature = "test-utils"))]
        record_finalization_classification();
        self.rebuild_local_fragment_declaration_index()?;
        self.rebuild_local_statement_site_indexes()?;
        self.compute_namespace_instance_states();
        let library_order = self.uses_library_shared_globals();
        for (_, namespace) in self.namespaces.changed_iter_mut() {
            if library_order {
                namespace.fragments.sort_by_key(|fragment| {
                    let fragment = self
                        .fragments
                        .get(fragment.index())
                        .expect("canonical library namespace fragment exists");
                    (
                        fragment.origin,
                        fragment.source_start,
                        fragment.declaration.0,
                    )
                });
            } else {
                namespace.fragments.sort_by_key(|fragment| {
                    self.fragments
                        .get(fragment.index())
                        .map(|fragment| {
                            (
                                fragment.source,
                                fragment.source_start,
                                fragment.declaration.0,
                            )
                        })
                        .unwrap_or((SourceUnitKey(u32::MAX), u32::MAX, u32::MAX))
                });
            }
        }
        rebuild_canonical_suffix(
            &mut self.canonical_namespaces,
            self.namespaces.base_len()..self.namespaces.len(),
            |index| NamespaceId(u32::try_from(index).expect("namespace count fits u32")),
            library_order,
            |id| {
                let namespace = &self.namespaces[id.index()];
                let first = namespace
                    .fragments
                    .first()
                    .and_then(|fragment| self.fragments.get(fragment.index()))
                    .expect("canonical library namespace has a fragment");
                (first.origin, first.source_start, namespace.name.clone())
            },
            |id| {
                let namespace = &self.namespaces[id.index()];
                let first = namespace
                    .fragments
                    .first()
                    .and_then(|fragment| self.fragments.get(fragment.index()));
                (
                    first
                        .map(|fragment| fragment.source)
                        .unwrap_or(SourceUnitKey(u32::MAX)),
                    first
                        .map(|fragment| fragment.source_start)
                        .unwrap_or(u32::MAX),
                    namespace.name.clone(),
                )
            },
        );
        self.merges.clear_local();
        // `clear_local` invalidates every suffix index. Keep only sealed-prefix keys so a sparse
        // continuation can still replace an affected base row at its stable index.
        self.merge_indices.clear_local();
        let changed_merges = self
            .placements
            .changed_iter()
            .map(|(key, participants)| {
                #[cfg(any(test, feature = "test-utils"))]
                record_finalization_merge_participant_rows(participants.len());
                // A group holding one declaration merged with nothing. Its row still costs a
                // placement entry and a merge record; what it must not cost is a second copy.
                #[cfg(any(test, feature = "test-utils"))]
                if participants.len() == 1 {
                    record_non_merging_participant_rows(participants.len());
                    record_non_merging_merge_record();
                }
                // Already in the canonical order the instance-state pass left them in, so the
                // record shares the vector instead of sorting a copy of it.
                let declarations = Arc::clone(participants);
                let classification = classify_group(&declarations);
                let placement_issues = placement_issues(&declarations);
                (
                    *key,
                    MergeRecord {
                        owner: key.owner,
                        name: Arc::clone(self.name_text(key.name)),
                        declarations,
                        classification,
                        placement_issues,
                    },
                )
            })
            .collect::<Vec<_>>();
        for (key, merge) in changed_merges {
            for participant in merge.declarations.iter() {
                self.merge_keys_by_declaration
                    .insert_local(participant.declaration, key)?;
            }
            if let Some(index) = self.merge_indices.get(&key).copied() {
                if let Some(slot) = self.merges.get_mut_local(index) {
                    *slot = merge;
                    continue;
                }
            }
            self.merges.push_local(merge);
        }
        if library_order {
            self.merges.local_slice_mut().sort_by(|left, right| {
                let left_key = left
                    .declarations
                    .first()
                    .map(|item| (item.origin, item.span.start))
                    .expect("canonical library merge has a declaration");
                let right_key = right
                    .declarations
                    .first()
                    .map(|item| (item.origin, item.span.start))
                    .expect("canonical library merge has a declaration");
                left_key
                    .cmp(&right_key)
                    .then_with(|| left.name.cmp(&right.name))
            });
        } else {
            self.merges.local_slice_mut().sort_by(|left, right| {
                let left_key = left
                    .declarations
                    .first()
                    .map(|item| (item.source, item.span.start))
                    .unwrap_or((SourceUnitKey(u32::MAX), u32::MAX));
                let right_key = right
                    .declarations
                    .first()
                    .map(|item| (item.source, item.span.start))
                    .unwrap_or((SourceUnitKey(u32::MAX), u32::MAX));
                left_key
                    .cmp(&right_key)
                    .then_with(|| left.name.cmp(&right.name))
            });
        }
        self.rebuild_local_merge_index()?;
        self.rebuild_local_declaration_owner_scope_index()?;
        rebuild_canonical_suffix(
            &mut self.canonical_globals,
            self.globals.base_len()..self.globals.len(),
            |index| GlobalAugmentationId(u32::try_from(index).expect("global count fits u32")),
            library_order,
            |id| {
                let global = &self.globals[id.index()];
                (global.origin, global.diagnostic_span.start, global.source)
            },
            |id| {
                let global = &self.globals[id.index()];
                (global.source, global.diagnostic_span.start, global.origin)
            },
        );
        rebuild_canonical_suffix(
            &mut self.canonical_deferred_modules,
            self.deferred_modules.base_len()..self.deferred_modules.len(),
            |index| DeferredModuleId(u32::try_from(index).expect("module count fits u32")),
            library_order,
            |id| {
                let module = &self.deferred_modules[id.index()];
                (module.origin, module.span.start, module.source)
            },
            |id| {
                let module = &self.deferred_modules[id.index()];
                (module.source, module.span.start, module.origin)
            },
        );
        rebuild_canonical_suffix(
            &mut self.canonical_source_units,
            self.source_units.base_len()..self.source_units.len(),
            |index| index,
            library_order,
            |index| {
                let unit = &self.source_units[*index];
                (unit.origin, unit.source)
            },
            |index| {
                let unit = &self.source_units[*index];
                (unit.source, unit.origin)
            },
        );
        rebuild_canonical_suffix(
            &mut self.canonical_deferred_children,
            self.deferred_children.base_len()..self.deferred_children.len(),
            |index| index,
            library_order,
            |index| {
                let child = &self.deferred_children[*index];
                (child.origin, child.span.start, child.source)
            },
            |index| {
                let child = &self.deferred_children[*index];
                (child.source, child.span.start, child.origin)
            },
        );
        rebuild_canonical_suffix(
            &mut self.canonical_umd_exports,
            self.umd_exports.base_len()..self.umd_exports.len(),
            |index| index,
            library_order,
            |index| {
                let export = &self.umd_exports[*index];
                (export.origin, export.span.start, export.source)
            },
            |index| {
                let export = &self.umd_exports[*index];
                (export.source, export.span.start, export.origin)
            },
        );
        rebuild_canonical_suffix(
            &mut self.canonical_export_contexts,
            self.export_contexts.base_len()..self.export_contexts.len(),
            |index| ExportContextId(u32::try_from(index).expect("export context count fits u32")),
            library_order,
            |id| {
                let context = &self.export_contexts[id.index()];
                (context.origin, context.span.start, context.source)
            },
            |id| {
                let context = &self.export_contexts[id.index()];
                (context.source, context.span.start, context.origin)
            },
        );
        Ok(())
    }

    fn rebuild_local_merge_index(&mut self) -> Result<(), &'static str> {
        self.merge_indices.clear_local();
        let base_len = self.merges.base_len();
        let rows = self
            .merges
            .local_iter()
            .enumerate()
            .map(|(offset, record)| {
                (
                    MergeKey {
                        owner: record.owner,
                        name: self
                            .name_id(&record.name)
                            .expect("merge record name is interned"),
                    },
                    base_len + offset,
                )
            })
            .collect::<Vec<_>>();
        for (key, index) in rows {
            #[cfg(any(test, feature = "test-utils"))]
            record_finalization_merge_index_row();
            self.merge_indices.insert_local(key, index)?;
        }
        Ok(())
    }

    fn rebuild_local_fragment_declaration_index(&mut self) -> Result<(), &'static str> {
        self.fragments_by_declaration.clear_local();
        self.fragment_private_scopes_by_site.clear_local();
        let rows = self
            .fragments
            .local_iter()
            .map(|fragment| {
                (
                    fragment.declaration,
                    fragment.id,
                    (fragment.module, fragment.source_start),
                    fragment.private_scope,
                )
            })
            .collect::<Vec<_>>();
        for (declaration, fragment, site, private_scope) in rows {
            if self
                .fragments_by_declaration
                .insert_local(declaration, fragment)
                .map_err(|_| "namespace fragment declaration index contains a duplicate")?
                .is_some()
            {
                return Err("namespace fragment declaration index contains a duplicate");
            }
            if self
                .fragment_private_scopes_by_site
                .insert_local(site, private_scope)
                .map_err(|_| "namespace fragment site index contains a duplicate")?
                .is_some()
            {
                return Err("namespace fragment site index contains a duplicate");
            }
        }
        Ok(())
    }

    fn rebuild_local_statement_site_indexes(&mut self) -> Result<(), &'static str> {
        self.global_augmentations_by_site.clear_local();
        self.umd_exports_by_site.clear_local();
        self.source_unit_indices_by_module.clear_local();
        self.library_export_default_sites.clear_local();
        self.library_module_reporting_sites.clear_local();
        let globals = self
            .globals
            .local_iter()
            .map(|global| ((global.module, global.diagnostic_span.start), global.id))
            .collect::<Vec<_>>();
        for (site, global) in globals {
            if self
                .global_augmentations_by_site
                .insert_local(site, global)
                .map_err(|_| "global augmentation site index contains a duplicate")?
                .is_some()
            {
                return Err("global augmentation site index contains a duplicate");
            }
        }
        let umd_base = self.umd_exports.base_len();
        let exports = self
            .umd_exports
            .local_iter()
            .enumerate()
            .map(|(offset, export)| ((export.module, export.span.start), umd_base + offset))
            .collect::<Vec<_>>();
        for (site, export) in exports {
            if self
                .umd_exports_by_site
                .insert_local(site, export)
                .map_err(|_| "UMD export site index contains a duplicate")?
                .is_some()
            {
                return Err("UMD export site index contains a duplicate");
            }
        }
        let source_base = self.source_units.base_len();
        let sources = self
            .source_units
            .local_iter()
            .enumerate()
            .map(|(offset, unit)| (unit.module, source_base + offset))
            .collect::<Vec<_>>();
        for (module, index) in sources {
            if self
                .source_unit_indices_by_module
                .insert_local(module, index)
                .map_err(|_| "source-module index contains a duplicate")?
                .is_some()
            {
                return Err("source-module index contains a duplicate");
            }
        }
        let default_sites = self
            .export_contexts
            .local_iter()
            .filter(|context| {
                context.kind == ExportContextKind::ExportDefault
                    && context.syntax == ExportSyntaxDisposition::FutureTk1319
                    && matches!(context.origin, CompilationOrigin::Library(_))
            })
            .map(|context| ((context.source, context.span.start), true))
            .collect::<Vec<_>>();
        for (site, owned) in default_sites {
            if self
                .library_export_default_sites
                .insert_local(site, owned)
                .map_err(|_| "library export-default reporting index contains a duplicate")?
                .is_some()
            {
                return Err("library export-default reporting index contains a duplicate");
            }
        }
        let module_sites = self
            .export_contexts
            .local_iter()
            .filter(|context| {
                context.syntax == ExportSyntaxDisposition::FutureTk1319
                    && matches!(context.origin, CompilationOrigin::Library(_))
            })
            .filter_map(|context| match context.owner {
                ExportContextOwner::NamespaceFragment(fragment) => self
                    .fragment(fragment)
                    .map(|fragment| ((fragment.module, fragment.source_start), true)),
                ExportContextOwner::GlobalAugmentation(_)
                | ExportContextOwner::DeferredAmbientModule(_) => None,
            })
            .collect::<Vec<_>>();
        for (site, owned) in module_sites {
            if self
                .library_module_reporting_sites
                .insert_local(site, owned)
                .map_err(|_| "library module-reporting index contains a duplicate")?
                .is_some()
            {
                return Err("library module-reporting index contains a duplicate");
            }
        }
        Ok(())
    }

    fn rebuild_local_declaration_owner_scope_index(&mut self) -> Result<(), &'static str> {
        self.declaration_owners_by_scope.clear_local();
        if let Some(scope) = self.compilation_global {
            if !self.declaration_owners_by_scope.contains_key(&scope) {
                self.declaration_owners_by_scope
                    .insert_local(scope, DeclarationOwner::CompilationGlobal)?;
            }
        }
        let namespace_base = self.namespaces.base_len();
        let namespace_rows = self
            .namespaces
            .local_iter()
            .enumerate()
            .map(|(offset, namespace)| {
                let id = NamespaceId(
                    u32::try_from(namespace_base + offset).expect("namespace id fits u32"),
                );
                (
                    namespace.public_scope,
                    DeclarationOwner::NamespacePublic(id),
                )
            })
            .collect::<Vec<_>>();
        for (scope, owner) in namespace_rows {
            self.declaration_owners_by_scope
                .insert_local(scope, owner)?;
        }
        let fragment_base = self.fragments.base_len();
        let fragment_rows = self
            .fragments
            .local_iter()
            .enumerate()
            .map(|(offset, fragment)| {
                let id = NamespaceFragmentId(
                    u32::try_from(fragment_base + offset).expect("fragment id fits u32"),
                );
                (
                    fragment.private_scope,
                    DeclarationOwner::NamespacePrivate(id),
                )
            })
            .collect::<Vec<_>>();
        for (scope, owner) in fragment_rows {
            self.declaration_owners_by_scope
                .insert_local(scope, owner)?;
        }
        Ok(())
    }

    /// Resolves every fragment's instantiation state and, in the same one-shot pass over the
    /// placement map, leaves each group in the total order classification reads — so the merge
    /// record can share the vector instead of sorting a copy of it.
    fn compute_namespace_instance_states(&mut self) {
        let fragment_base = self.fragments.base_len();
        let mut states = vec![NamespaceInstanceState::NonInstantiated; self.fragments.local_len()];
        for fragment in self.fragments.local_iter() {
            #[cfg(any(test, feature = "test-utils"))]
            record_continuation_instance_fragment_row();
            for member in fragment
                .members
                .iter()
                .filter_map(|member| self.members.get(member.index()))
            {
                let direct = match member.kind {
                    MergeDeclarationKind::Variable
                    | MergeDeclarationKind::Function
                    | MergeDeclarationKind::Class => NamespaceInstanceState::Instantiated,
                    MergeDeclarationKind::ImportAlias
                        if member.spaces.value
                            && matches!(
                                member.syntax,
                                DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
                                    exported: true,
                                    ..
                                })
                            ) =>
                    {
                        NamespaceInstanceState::Instantiated
                    }
                    MergeDeclarationKind::Enum
                        if member.syntax == (DeclarationSyntaxFacts::Enum { constant: false }) =>
                    {
                        NamespaceInstanceState::Instantiated
                    }
                    _ => NamespaceInstanceState::NonInstantiated,
                };
                let index = fragment.id.index() - fragment_base;
                states[index] = join_instance_state(states[index], direct);
            }
        }

        loop {
            let mut changed = false;
            for fragment in self.fragments.local_iter() {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_instance_fragment_row();
                let fragment_index = fragment.id.index() - fragment_base;
                let mut state = states[fragment_index];
                for member in fragment
                    .members
                    .iter()
                    .filter_map(|member| self.members.get(member.index()))
                    .filter(|member| member.kind == MergeDeclarationKind::Namespace)
                {
                    #[cfg(any(test, feature = "test-utils"))]
                    record_continuation_child_fragment_lookup();
                    let child = member
                        .declaration
                        .and_then(|declaration| self.fragments_by_declaration.get(&declaration))
                        .and_then(|fragment| self.fragments.get(fragment.index()));
                    if let Some(child) = child.filter(|child| child.id.index() >= fragment_base) {
                        state =
                            join_instance_state(state, states[child.id.index() - fragment_base]);
                    }
                }
                if state != states[fragment_index] {
                    states[fragment_index] = state;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for fragment in self.fragments.local_iter_mut() {
            fragment.instance_state = states[fragment.id.index() - fragment_base];
        }
        self.aggregate_instance_states.clear_local();
        for _ in 0..self.namespaces.local_len() {
            self.aggregate_instance_states
                .push_local(NamespaceInstanceState::NonInstantiated);
        }
        for fragment in self.fragments.local_iter() {
            if let Some(aggregate) = self
                .aggregate_instance_states
                .get_mut_local(fragment.namespace.index())
            {
                *aggregate = join_instance_state(*aggregate, fragment.instance_state);
            }
        }
        let library_order = self.uses_library_shared_globals();
        for participants in self.placements.changed_values_mut() {
            #[cfg(any(test, feature = "test-utils"))]
            record_placement_order_group();
            // Binding still owns every group here — the merge records that share these vectors
            // are rebuilt after this pass — so `make_mut` never copies.
            let participants = Arc::make_mut(participants);
            for participant in participants.iter_mut() {
                participant.namespace_instance = participant
                    .namespace_fragment
                    .and_then(|fragment| fragment.index().checked_sub(fragment_base))
                    .and_then(|index| states.get(index).copied());
            }
            // Leave each group in the total order classification reads it in, so the merge
            // record can share this vector rather than sort a copy of it. Declaration ids are
            // unique, so the order is total and sorting is idempotent.
            #[cfg(any(test, feature = "test-utils"))]
            let before = participants.clone();
            if library_order {
                participants.sort_by_key(|participant| {
                    (
                        participant.origin,
                        participant.span.start,
                        participant.declaration.0,
                    )
                });
            } else {
                participants.sort_by_key(|participant| {
                    (
                        participant.source,
                        participant.span.start,
                        participant.declaration.0,
                    )
                });
            }
            #[cfg(any(test, feature = "test-utils"))]
            if before != *participants {
                record_placement_row_reorder();
            }
        }
    }

    pub(super) fn dormant_standalone_value_storage_candidates(&self) -> Vec<NamespaceId> {
        self.canonical_namespaces
            .local_iter()
            .inspect(|_| {
                #[cfg(any(test, feature = "test-utils"))]
                record_continuation_allocation_namespace_row();
            })
            .copied()
            .filter(|namespace| {
                self.aggregate_instance_state(*namespace)
                    == Some(NamespaceInstanceState::Instantiated)
                    && self.standalone_value_storage(*namespace).is_none()
                    && (self.uses_library_shared_globals()
                        || !self.has_compilation_global_ancestor(*namespace))
                    && self
                        .standalone_merge_record(*namespace)
                        .is_some_and(|record| {
                            record.classification.disposition == MergeDisposition::Admitted
                                && !record.declarations.iter().any(|participant| {
                                    participant.kind == MergeDeclarationKind::Enum
                                })
                                && namespace_value_attachment_disposition(record)
                                    == Some(NamespaceValueAttachmentDisposition::TypeContainerOnly)
                        })
            })
            .collect()
    }

    pub(super) fn is_admitted_instantiated_standalone(&self, record: &MergeRecord) -> bool {
        !matches!(record.owner, DeclarationOwner::DeferredAmbientModule(_))
            && (record.owner != DeclarationOwner::CompilationGlobal
                || self.uses_library_shared_globals())
            && record.classification.disposition == MergeDisposition::Admitted
            && !record
                .declarations
                .iter()
                .any(|participant| participant.kind == MergeDeclarationKind::Enum)
            && namespace_value_attachment_disposition(record)
                == Some(NamespaceValueAttachmentDisposition::TypeContainerOnly)
            && record.declarations.iter().any(|participant| {
                participant.kind == MergeDeclarationKind::Namespace
                    && participant.namespace_instance == Some(NamespaceInstanceState::Instantiated)
            })
            && record
                .declarations
                .iter()
                .filter_map(|participant| participant.namespace_fragment)
                .filter_map(|fragment| self.fragment(fragment))
                .all(|fragment| {
                    self.uses_library_shared_globals()
                        || !self.has_compilation_global_ancestor(fragment.namespace)
                })
    }

    pub(super) fn raw_global_record_is_publishable(&self, record: &MergeRecord) -> bool {
        record.classification.disposition == MergeDisposition::Admitted
            && record.declarations.iter().all(|participant| {
                matches!(
                    participant.kind,
                    MergeDeclarationKind::Interface
                        | MergeDeclarationKind::TypeAlias
                        | MergeDeclarationKind::Variable
                        | MergeDeclarationKind::Function
                        | MergeDeclarationKind::Class
                        | MergeDeclarationKind::Namespace
                )
            })
    }

    pub(super) fn is_admitted_storage_free_global_namespace(&self, record: &MergeRecord) -> bool {
        record.owner == DeclarationOwner::CompilationGlobal
            && !self.uses_library_shared_globals()
            && self.raw_global_record_is_publishable(record)
            && record.declarations.iter().any(|participant| {
                participant.kind == MergeDeclarationKind::Namespace
                    && participant.namespace_instance == Some(NamespaceInstanceState::Instantiated)
            })
    }

    pub(super) fn is_storage_free_type_side_namespace(&self, namespace: NamespaceId) -> bool {
        self.standalone_value_storage(namespace).is_none()
            && self
                .standalone_merge_record(namespace)
                .is_some_and(|record| {
                    record.classification.disposition == MergeDisposition::Admitted
                        && namespace_value_attachment_disposition(record)
                            == Some(NamespaceValueAttachmentDisposition::TypeContainerOnly)
                })
            && (self.aggregate_instance_state(namespace)
                == Some(NamespaceInstanceState::NonInstantiated)
                || (!self.uses_library_shared_globals()
                    && self.has_compilation_global_ancestor(namespace)))
    }

    pub(super) fn standalone_merge_record(&self, id: NamespaceId) -> Option<&MergeRecord> {
        let namespace = self.get(id)?;
        let owner = match namespace.owner {
            NamespaceOwner::Lexical(scope) => DeclarationOwner::Lexical(scope),
            NamespaceOwner::NamespacePublic(parent) => DeclarationOwner::NamespacePublic(parent),
            NamespaceOwner::FragmentPrivate(fragment) => {
                DeclarationOwner::NamespacePrivate(fragment)
            }
            NamespaceOwner::CompilationGlobal => DeclarationOwner::CompilationGlobal,
        };
        let key = MergeKey {
            owner,
            name: self.name_id(&namespace.name)?,
        };
        self.merge_indices
            .get(&key)
            .and_then(|index| self.merges.get(*index))
    }

    fn has_compilation_global_ancestor(&self, id: NamespaceId) -> bool {
        let mut current = Some(id);
        let mut remaining = self.namespaces.len();
        while let Some(namespace) = current {
            if remaining == 0 {
                return true;
            }
            remaining -= 1;
            current = match self.get(namespace).map(|namespace| namespace.owner) {
                Some(NamespaceOwner::CompilationGlobal) => return true,
                Some(NamespaceOwner::NamespacePublic(parent)) => Some(parent),
                Some(NamespaceOwner::FragmentPrivate(fragment)) => {
                    self.fragment(fragment).map(|fragment| fragment.namespace)
                }
                Some(NamespaceOwner::Lexical(_)) | None => None,
            };
        }
        false
    }
}

fn join_instance_state(
    left: NamespaceInstanceState,
    right: NamespaceInstanceState,
) -> NamespaceInstanceState {
    match (left, right) {
        (NamespaceInstanceState::Instantiated, _) | (_, NamespaceInstanceState::Instantiated) => {
            NamespaceInstanceState::Instantiated
        }
        _ => NamespaceInstanceState::NonInstantiated,
    }
}

pub(super) fn enum_group_semantics(
    declarations: &[MergeParticipant],
) -> Option<(EnumGroupBuildability, Vec<EnumGroupIssue>)> {
    let enum_declarations = declarations
        .iter()
        .filter(|participant| participant.kind == MergeDeclarationKind::Enum)
        .collect::<Vec<_>>();
    let first_enum = declarations
        .iter()
        .position(|participant| participant.kind == MergeDeclarationKind::Enum)?;
    let only_enum_namespace = declarations.iter().all(|participant| {
        matches!(
            participant.kind,
            MergeDeclarationKind::Enum | MergeDeclarationKind::Namespace
        )
    });
    let constant = enum_declarations.first().and_then(|participant| {
        let DeclarationSyntaxFacts::Enum { constant } = participant.syntax else {
            return None;
        };
        Some(constant)
    });
    let constant_modifiers_match = constant.is_some_and(|constant| {
        enum_declarations
            .iter()
            .all(|participant| participant.syntax == DeclarationSyntaxFacts::Enum { constant })
    });
    let has_namespace = declarations
        .iter()
        .any(|participant| participant.kind == MergeDeclarationKind::Namespace);
    let const_enum_namespace = has_namespace && constant == Some(true);
    if only_enum_namespace && constant_modifiers_match && !const_enum_namespace {
        return Some((EnumGroupBuildability::Buildable, Vec::new()));
    }

    let issues = declarations
        .iter()
        .enumerate()
        .filter(|(index, participant)| {
            const_enum_namespace
                || participant.kind != MergeDeclarationKind::Namespace
                || *index < first_enum
        })
        .map(|(_, participant)| EnumGroupIssue {
            kind: EnumGroupIssueKind::FutureTk2567,
            owner: participant.declaration,
            source: participant.source,
            origin: participant.origin,
            span: participant.binding_span,
        })
        .collect();
    Some((EnumGroupBuildability::RejectedFutureTk2567, issues))
}

fn classify_group(declarations: &[MergeParticipant]) -> MergeClassification {
    use MergeDeclarationKind::{
        Class, Enum, Function, ImportAlias, Interface, Namespace, Variable,
    };
    let count = |kind| declarations.iter().filter(|item| item.kind == kind).count();
    let namespaces = count(Namespace);
    let interfaces = count(Interface);
    let functions = count(Function);
    let classes = count(Class);
    let variables = count(Variable);
    let imports = count(ImportAlias);
    let enums = count(Enum);
    let total = declarations.len();
    let enum_buildability = enum_group_semantics(declarations).map(|(state, _)| state);
    let enum_group_buildable = enum_buildability == Some(EnumGroupBuildability::Buildable);
    let mut compositions = Vec::new();
    let mut push = |kind, disposition| {
        let composition = MergeComposition { kind, disposition };
        if !compositions.contains(&composition) {
            compositions.push(composition);
        }
    };

    let slot_state = |slot: fn(DeclarationSpaces) -> bool| MergeSlotState {
        declarations: declarations.iter().filter(|item| slot(item.spaces)).count(),
        disposition: MergeSlotDisposition::Single,
    };
    let mut value = slot_state(|spaces| spaces.value);
    let mut r#type = slot_state(|spaces| spaces.r#type);
    let mut namespace = slot_state(|spaces| spaces.namespace);
    for slot in [&mut value, &mut r#type, &mut namespace] {
        if slot.declarations == 0 {
            slot.disposition = MergeSlotDisposition::Empty;
        }
    }

    if value.declarations > 1 {
        if enums == value.declarations && enum_group_buildable {
            value.disposition = MergeSlotDisposition::AdmittedMerge;
        } else if functions == value.declarations {
            value.disposition = MergeSlotDisposition::AdmittedMerge;
            push(
                MergeCompositionKind::FunctionGroup,
                MergeDisposition::Admitted,
            );
        } else if variables == value.declarations
            && declarations.iter().all(|item| {
                item.kind != Variable
                    || item.syntax == DeclarationSyntaxFacts::Variable(VariableKind::Var)
            })
        {
            value.disposition = MergeSlotDisposition::Deferred;
            push(
                MergeCompositionKind::VariableGroup,
                MergeDisposition::DeferredBacklog15,
            );
        } else if enums > 0 {
            value.disposition = MergeSlotDisposition::Rejected;
        } else {
            value.disposition = MergeSlotDisposition::Rejected;
            push(
                MergeCompositionKind::ConflictingValueDeclarations,
                MergeDisposition::RejectedRedeclaration,
            );
        }
    }

    if r#type.declarations > 1 {
        if enums == r#type.declarations && enum_group_buildable {
            r#type.disposition = MergeSlotDisposition::AdmittedMerge;
        } else if interfaces == r#type.declarations {
            r#type.disposition = MergeSlotDisposition::AdmittedMerge;
            push(
                MergeCompositionKind::InterfaceGroup,
                MergeDisposition::Admitted,
            );
        } else if classes == 1 && classes + interfaces == r#type.declarations {
            r#type.disposition = MergeSlotDisposition::AdmittedMerge;
            push(
                MergeCompositionKind::ClassInterface,
                MergeDisposition::Admitted,
            );
        } else if enums > 0 {
            r#type.disposition = MergeSlotDisposition::Rejected;
        } else {
            r#type.disposition = MergeSlotDisposition::Rejected;
            push(
                MergeCompositionKind::ConflictingTypeDeclarations,
                MergeDisposition::RejectedRedeclaration,
            );
        }
    }

    if namespace.declarations > 1 {
        namespace.disposition = MergeSlotDisposition::AdmittedMerge;
        push(
            MergeCompositionKind::NamespaceGroup,
            MergeDisposition::Admitted,
        );
    }
    let occupied_slots = [&value, &r#type, &namespace]
        .iter()
        .filter(|slot| slot.declarations > 0)
        .count();
    if total > 1 && occupied_slots > 1 {
        push(
            MergeCompositionKind::IndependentOccupiedSlots,
            MergeDisposition::Admitted,
        );
    }
    if namespaces > 0 && functions > 0 {
        push(
            MergeCompositionKind::FunctionNamespace,
            MergeDisposition::Admitted,
        );
    }
    if namespaces > 0 && classes > 0 {
        push(
            MergeCompositionKind::ClassNamespace,
            MergeDisposition::Admitted,
        );
    }
    if namespaces > 0 && interfaces > 0 {
        push(
            MergeCompositionKind::InterfaceNamespace,
            MergeDisposition::Admitted,
        );
    }
    if namespaces > 0 && variables > 0 {
        if declarations.iter().any(|item| {
            item.kind == Namespace
                && item.namespace_instance == Some(NamespaceInstanceState::Instantiated)
        }) {
            push(
                MergeCompositionKind::VariableNamespaceRuntime,
                MergeDisposition::RejectedRuntimeNamespace,
            );
        } else {
            push(
                MergeCompositionKind::VariableNamespaceNonInstantiated,
                MergeDisposition::Admitted,
            );
        }
    }
    if namespaces > 0 && imports > 0 {
        for facts in declarations.iter().filter_map(|item| match item.syntax {
            DeclarationSyntaxFacts::Import(facts) => Some(facts),
            _ => None,
        }) {
            let type_only = facts.outer_type_only || facts.specifier_type_only;
            let disposition = match facts.form {
                ImportBindingForm::Namespace | ImportBindingForm::ImportEquals => {
                    MergeDisposition::RejectedFutureTk2440
                }
                ImportBindingForm::Named | ImportBindingForm::Default if type_only => {
                    MergeDisposition::Admitted
                }
                ImportBindingForm::Named | ImportBindingForm::Default => {
                    MergeDisposition::DeferredBacklog15
                }
            };
            push(
                MergeCompositionKind::ImportNamespace(facts.form),
                disposition,
            );
        }
    }
    let enum_composition = total > 1 && enums > 0;
    if enum_composition {
        push(
            MergeCompositionKind::EnumComposition,
            if enum_group_buildable {
                MergeDisposition::Admitted
            } else {
                MergeDisposition::RejectedFutureTk2567
            },
        );
    }

    let disposition = if enum_buildability == Some(EnumGroupBuildability::RejectedFutureTk2567) {
        MergeDisposition::RejectedFutureTk2567
    } else if compositions
        .iter()
        .any(|item| item.disposition == MergeDisposition::RejectedFutureTk2440)
    {
        MergeDisposition::RejectedFutureTk2440
    } else if compositions
        .iter()
        .any(|item| item.disposition == MergeDisposition::RejectedRuntimeNamespace)
    {
        MergeDisposition::RejectedRuntimeNamespace
    } else if compositions
        .iter()
        .any(|item| item.disposition == MergeDisposition::RejectedRedeclaration)
    {
        MergeDisposition::RejectedRedeclaration
    } else if compositions
        .iter()
        .any(|item| item.disposition == MergeDisposition::DeferredBacklog15)
    {
        MergeDisposition::DeferredBacklog15
    } else {
        MergeDisposition::Admitted
    };
    MergeClassification {
        slots: MergeSlotSummary {
            value,
            r#type,
            namespace,
        },
        compositions,
        disposition,
    }
}

fn placement_issues(declarations: &[MergeParticipant]) -> Vec<PlacementIssue> {
    let last_value = declarations.iter().rfind(|item| {
        matches!(
            item.kind,
            MergeDeclarationKind::Function | MergeDeclarationKind::Class
        ) && !item.ambient
    });
    let Some(last_value) = last_value else {
        return Vec::new();
    };
    declarations
        .iter()
        .filter(|item| {
            item.kind == MergeDeclarationKind::Namespace
                && !item.ambient
                && item.namespace_instance == Some(NamespaceInstanceState::Instantiated)
                && (item.source, item.span.start) < (last_value.source, last_value.span.start)
        })
        .map(|item| PlacementIssue {
            kind: PlacementIssueKind::FutureTk2434,
            owner: item.declaration,
            source: item.source,
            origin: item.origin,
            span: item.binding_span,
        })
        .collect()
}

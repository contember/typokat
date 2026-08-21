use super::*;

struct NormalizedSourceRootCandidates {
    candidates: BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    namespace_contributors: BTreeSet<String>,
}

pub(super) fn apply_merge_root_semantics(
    candidate: &mut crate::binder::declaration::SourceGlobalBindingCandidate,
    slots: BTreeSet<SourceBindingSlot>,
    ordinary_contributor: bool,
    namespace_instantiated: bool,
) {
    candidate.slots = slots;
    if namespace_instantiated {
        candidate.slots.insert(SourceBindingSlot::Value);
    }
    candidate.global_object_contributor = ordinary_contributor || namespace_instantiated;
}

fn normalize_source_root_candidates(
    mut candidates: BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    binder: &Binder,
) -> NormalizedSourceRootCandidates {
    let mut namespace_contributors = BTreeSet::new();
    for record in binder.namespaces.merges().filter(|record| {
        record.owner == crate::binder::namespace::DeclarationOwner::CompilationGlobal
    }) {
        if record.classification.disposition != crate::binder::namespace::MergeDisposition::Admitted
        {
            candidates.remove(record.name.as_ref());
            continue;
        }
        let Some(candidate) = candidates.get_mut(record.name.as_ref()) else {
            continue;
        };
        let mut slots = BTreeSet::new();
        let mut ordinary_contributor = false;
        for declaration in record.declarations.iter() {
            if declaration.spaces.value {
                slots.insert(SourceBindingSlot::Value);
            }
            if declaration.spaces.r#type {
                slots.insert(SourceBindingSlot::Type);
            }
            if declaration.spaces.namespace {
                slots.insert(SourceBindingSlot::Namespace);
            }
            ordinary_contributor |= declaration.kind
                == crate::binder::namespace::MergeDeclarationKind::Function
                || matches!(
                    declaration.syntax,
                    crate::binder::namespace::DeclarationSyntaxFacts::Variable(
                        crate::binder::namespace::VariableKind::Var
                    )
                );
        }
        let namespace = record
            .declarations
            .iter()
            .find_map(|declaration| declaration.namespace_fragment)
            .and_then(|fragment| binder.namespaces.fragment(fragment))
            .map(|fragment| fragment.namespace);
        let namespace_instantiated = namespace.is_some_and(|namespace| {
            binder.namespaces.aggregate_instance_state(namespace)
                == Some(crate::binder::namespace::NamespaceInstanceState::Instantiated)
        });
        apply_merge_root_semantics(
            candidate,
            slots,
            ordinary_contributor,
            namespace_instantiated,
        );
        if namespace_instantiated {
            namespace_contributors.insert(record.name.to_string());
        }
    }
    NormalizedSourceRootCandidates {
        candidates,
        namespace_contributors,
    }
}

pub(super) fn build_collision_replay_plan(
    trace: ReplayDependencyTrace,
    root_provenance: LibraryRootProjection,
    baselines: Vec<super::replay_index::ReplayBaselineRecord>,
    prefix_cardinalities: [usize; 9],
    forbidden_work: CollisionPlanForbiddenWork,
) -> Result<CollisionReplayPlan, InjectedProfileError> {
    let LibraryRootProjection {
        candidates,
        explicit_global_this,
        contributor_sites,
        explicit_global_this_sites,
        root_rows,
        canonical_unit_count,
        source_census_count,
        uncertain_candidate_count,
        uncertain_relevant_syntax_count,
        normalization_issue_count,
    } = root_provenance;
    if source_census_count != canonical_unit_count
        || uncertain_candidate_count != 0
        || uncertain_relevant_syntax_count != 0
        || normalization_issue_count != 0
    {
        return Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::InvalidRootSlot("<binder-source-census>".to_owned()),
        ));
    }
    if forbidden_work
        != (CollisionPlanForbiddenWork {
            library_source_compiles: 1,
            ..CollisionPlanForbiddenWork::default()
        })
    {
        return Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::BaselinePartitionMismatch,
        ));
    }
    for site in contributor_sites {
        trace.record_owner_site(CollisionReplayOwnerSite {
            owner: ReplayOwner::GlobalObject,
            file_ordinal: site.file_ordinal,
            span: site.span,
            provenance: super::replay_index::CollisionReplaySiteProvenance::GlobalContributor {
                name: site.name,
                kind: site.kind,
                binder_owner: crate::binder::namespace::DeclarationOwner::CompilationGlobal,
            },
        });
    }
    for (file_ordinal, span) in explicit_global_this_sites {
        trace.record_owner_site(CollisionReplayOwnerSite {
            owner: ReplayOwner::GlobalObject,
            file_ordinal,
            span,
            provenance: super::replay_index::CollisionReplaySiteProvenance::ExplicitGlobalThis {
                binder_owner: crate::binder::namespace::DeclarationOwner::CompilationGlobal,
            },
        });
    }
    validate_root_census(&candidates, &root_rows, explicit_global_this)
        .map_err(InjectedProfileError::ReplayIndex)?;
    let mut roots = Vec::with_capacity(root_rows.len());
    {
        let _global_scope = trace.scope(ReplayOwner::GlobalObject);
        for row in root_rows {
            let candidate = candidates.get(&row.name).ok_or_else(|| {
                InjectedProfileError::ReplayIndex(ReplayIndexGenerationError::InvalidRootSlot(
                    row.name.clone(),
                ))
            })?;
            let contributor = candidate.global_object_contributor;
            if contributor {
                if let Some(value) = row.value {
                    trace.demand(ReplayOwner::Value(value));
                }
                if let Some(ty) = row.ty {
                    trace.demand(ReplayOwner::TypeGroup(ty));
                }
                if let Some(namespace) = row.namespace {
                    trace.demand(ReplayOwner::Namespace(namespace));
                }
            }
            roots.push(ReplayRootSlot {
                explicit_global_this: explicit_global_this && row.name == "globalThis",
                name: row.name,
                value: row.value,
                ty: row.ty,
                namespace: row.namespace,
                global_object_contributor: contributor,
            });
        }
    }

    trace
        .finish_compact_plan(
            roots,
            baselines,
            prefix_cardinalities,
            CollisionReplayConstructionEvidence {
                library_source_compiles: forbidden_work.library_source_compiles,
                binder_source_censuses: u64::try_from(source_census_count).unwrap_or(u64::MAX),
                canonical_source_units: u64::try_from(canonical_unit_count).unwrap_or(u64::MAX),
                second_source_censuses: forbidden_work.second_source_censuses,
                canonical_manifest_bytes: forbidden_work.canonical_manifest_bytes,
                rendered_record_digest_bytes: forbidden_work.rendered_record_digest_bytes,
                transitive_terminal_owner_entries: forbidden_work.transitive_terminal_owner_entries,
                eager_all_owner_scc_memberships: forbidden_work.eager_all_owner_scc_memberships,
                namespace_snapshot_rows: forbidden_work.namespace_snapshot_rows,
                runtime_snapshot_rows: forbidden_work.runtime_snapshot_rows,
                canonical_terminal_rows: forbidden_work.canonical_terminal_rows,
                full_semantic_projection_rows: forbidden_work.full_semantic_projection_rows,
                ticket_slots: 0,
                ticket_owner_ordered_map_inserts: 0,
                owner_site_inner_heap_allocations: 0,
                owner_site_dense_slot_writes: 0,
                owner_site_ordered_map_inserts: 0,
                trace_domain_sealed_after_binder_reporting: false,
            },
        )
        .map_err(InjectedProfileError::ReplayIndex)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_collision_replay_index(
    trace: ReplayDependencyTrace,
    interner: &Interner,
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    decl_types: &DeclTypes,
    namespace_terminals: &[FrozenNamespaceValueTerminalSnapshotRow],
    runtime: &FrozenCheckerRuntimeSnapshotParts,
    semantic_identities: Option<&LibrarySemanticIdentities>,
    canonical: &[CanonicalInput<'_>],
    parsed: &[&Program<'_>],
    module_scopes: &[ScopeId],
    class_declarations: &BTreeMap<ClassId, crate::binder::declaration::DeclId>,
    records: &[(LibraryEventKey, CheckerRecord)],
    #[cfg(any(test, feature = "test-utils"))] independent_event_owner_sites: Vec<
        CollisionReplayOwnerSite,
    >,
) -> Result<CollisionReplayIndex, InjectedProfileError> {
    fn add_owner_site(
        module_ordinals: &rustc_hash::FxHashMap<ScopeId, LibraryFileOrdinal>,
        sites: &mut BTreeSet<(ReplayOwner, LibraryFileOrdinal, u32, u32)>,
        invalid_sites: &mut u64,
        owner: ReplayOwner,
        module: ScopeId,
        span: Span,
    ) {
        if let Some(file_ordinal) = module_ordinals.get(&module).copied() {
            sites.insert((owner, file_ordinal, span.start, span.end));
        } else {
            *invalid_sites = invalid_sites.saturating_add(1);
        }
    }

    let statement_keys = trace.statement_keys();
    let module_ordinals = module_scopes
        .iter()
        .copied()
        .zip(canonical.iter().map(|input| input.file_ordinal))
        .collect::<rustc_hash::FxHashMap<_, _>>();
    #[cfg(any(test, feature = "test-utils"))]
    let mut exact_owner_sites = independent_event_owner_sites;
    let mut owners = Vec::new();
    owners.extend((0..binder.type_groups.len()).map(|index| {
        ReplayOwner::TypeGroup(TypeGroupId(
            u32::try_from(index).expect("type-group count fits u32"),
        ))
    }));
    owners.extend(
        (0..usize::try_from(binder.decl_count).unwrap_or(usize::MAX)).map(|index| {
            ReplayOwner::Value(ValueStorageId(
                u32::try_from(index).expect("value-storage count fits u32"),
            ))
        }),
    );
    owners.extend((0..binder.namespaces.len()).map(|index| {
        ReplayOwner::Namespace(NamespaceId(
            u32::try_from(index).expect("namespace count fits u32"),
        ))
    }));
    let class_terminals = published.classes().canonical_terminals().ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "replay generation saw a non-terminal class registry".to_owned(),
        )
    })?;
    owners.extend(
        class_terminals
            .iter()
            .map(|(class, _)| ReplayOwner::Class(*class)),
    );
    owners.push(ReplayOwner::GlobalObject);
    owners.extend(statement_keys.iter().copied().map(ReplayOwner::Statement));

    let mut invalid_sites = 0u64;
    let mut sites = BTreeSet::new();
    for index in 0..binder.type_groups.len() {
        let group = TypeGroupId(u32::try_from(index).expect("type-group count fits u32"));
        if let Some(row) = binder.type_groups.get(group) {
            for fragment in &row.fragments {
                add_owner_site(
                    &module_ordinals,
                    &mut sites,
                    &mut invalid_sites,
                    ReplayOwner::TypeGroup(group),
                    fragment.site.module,
                    fragment.site.declaration_span,
                );
                #[cfg(any(test, feature = "test-utils"))]
                if let (Some(file_ordinal), Some(declaration)) = (
                    module_ordinals.get(&fragment.site.module).copied(),
                    binder.declarations.get(fragment.declaration),
                ) {
                    let declaration_scope =
                        declaration.site.scope.unwrap_or(declaration.site.module);
                    exact_owner_sites.push(CollisionReplayOwnerSite {
                        owner: ReplayOwner::TypeGroup(group),
                        file_ordinal,
                        span: declaration.site.declaration_span,
                        provenance: collision_declaration_provenance(
                            binder,
                            declaration,
                            declaration_scope,
                        ),
                    });
                }
            }
        }
    }
    for declaration in binder.declarations.iter() {
        if let Some(storage) = declaration.value_storage {
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Value(storage),
                declaration.site.module,
                declaration.site.declaration_span,
            );
            #[cfg(any(test, feature = "test-utils"))]
            if let Some(file_ordinal) = module_ordinals.get(&declaration.site.module).copied() {
                let declaration_scope = declaration.site.scope.unwrap_or(declaration.site.module);
                exact_owner_sites.push(CollisionReplayOwnerSite {
                    owner: ReplayOwner::Value(storage),
                    file_ordinal,
                    span: declaration.site.declaration_span,
                    provenance: collision_declaration_provenance(
                        binder,
                        declaration,
                        declaration_scope,
                    ),
                });
            }
        }
    }
    for index in 0..binder.namespaces.len() {
        let namespace = NamespaceId(u32::try_from(index).expect("namespace count fits u32"));
        let Some(row) = binder.namespaces.get(namespace) else {
            continue;
        };
        for fragment_id in &row.fragments {
            let Some(fragment) = binder.namespaces.fragment(*fragment_id) else {
                invalid_sites = invalid_sites.saturating_add(1);
                continue;
            };
            let Some(declaration) = binder.declarations.get(fragment.declaration) else {
                invalid_sites = invalid_sites.saturating_add(1);
                continue;
            };
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Namespace(namespace),
                declaration.site.module,
                declaration.site.declaration_span,
            );
            #[cfg(any(test, feature = "test-utils"))]
            if let Some(file_ordinal) = module_ordinals.get(&declaration.site.module).copied() {
                let declaration_scope = declaration.site.scope.unwrap_or(declaration.site.module);
                let provenance =
                    collision_declaration_provenance(binder, declaration, declaration_scope);
                exact_owner_sites.push(CollisionReplayOwnerSite {
                    owner: ReplayOwner::Namespace(namespace),
                    file_ordinal,
                    span: declaration.site.declaration_span,
                    provenance: provenance.clone(),
                });
                if let Some(storage) = binder.namespaces.standalone_value_storage(namespace) {
                    exact_owner_sites.push(CollisionReplayOwnerSite {
                        owner: ReplayOwner::Value(storage),
                        file_ordinal,
                        span: declaration.site.declaration_span,
                        provenance,
                    });
                }
            }
            if let Some(storage) = binder.namespaces.standalone_value_storage(namespace) {
                add_owner_site(
                    &module_ordinals,
                    &mut sites,
                    &mut invalid_sites,
                    ReplayOwner::Value(storage),
                    declaration.site.module,
                    declaration.site.declaration_span,
                );
            }
        }
    }
    for (class, declaration) in class_declarations {
        if let Some(declaration) = binder.declarations.get(*declaration) {
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Class(*class),
                declaration.site.module,
                declaration.site.declaration_span,
            );
            #[cfg(any(test, feature = "test-utils"))]
            if let Some(file_ordinal) = module_ordinals.get(&declaration.site.module).copied() {
                let declaration_scope = declaration.site.scope.unwrap_or(declaration.site.module);
                exact_owner_sites.push(CollisionReplayOwnerSite {
                    owner: ReplayOwner::Class(*class),
                    file_ordinal,
                    span: declaration.site.declaration_span,
                    provenance: collision_declaration_provenance(
                        binder,
                        declaration,
                        declaration_scope,
                    ),
                });
            }
        } else {
            invalid_sites = invalid_sites.saturating_add(1);
        }
    }

    let mut candidates = BTreeMap::new();
    let mut explicit_global_this = false;
    let mut contributor_sites = Vec::new();
    let mut explicit_global_this_sites = Vec::new();
    for ((input, program), module) in canonical.iter().zip(parsed).zip(module_scopes) {
        let provenance = source_global_binding_census_with_provenance(
            program,
            ModuleBindingContext::for_program(program, input.kind),
        );
        record_collision_plan_forbidden_work(|work| {
            work.second_source_censuses = work.second_source_censuses.saturating_add(1);
        });
        explicit_global_this |= provenance.census.explicit_global_this;
        contributor_sites.extend(
            provenance
                .contributor_sites
                .into_iter()
                .map(|site| (site.name, site.kind, input.file_ordinal, site.span)),
        );
        explicit_global_this_sites.extend(
            provenance
                .explicit_global_this_sites
                .into_iter()
                .map(|span| (input.file_ordinal, span)),
        );
        for (name, candidate) in provenance.census.candidates {
            let aggregate = candidates
                .entry(name)
                .or_insert_with(crate::binder::declaration::SourceGlobalBindingCandidate::default);
            aggregate.slots.extend(candidate.slots);
            aggregate.global_object_contributor |= candidate.global_object_contributor;
        }
        let _ = module;
    }
    if let Some(input) = canonical.first() {
        sites.insert((ReplayOwner::GlobalObject, input.file_ordinal, 0, 0));
    }
    for key in &statement_keys {
        sites.insert((
            ReplayOwner::Statement(*key),
            key.file_ordinal,
            key.source_start,
            key.source_start,
        ));
    }

    let root_rows = collect_root_rows(binder)
        .map_err(|error| InjectedProfileError::CanonicalProjection(format!("{error:?}")))?;
    let normalized = normalize_source_root_candidates(candidates, binder);
    let candidates = normalized.candidates;
    for (name, kind, file_ordinal, span) in contributor_sites {
        let admitted = match kind {
            SourceGlobalContributorKind::Ordinary => candidates.contains_key(&name),
            SourceGlobalContributorKind::Namespace => {
                normalized.namespace_contributors.contains(&name)
            }
        };
        if admitted {
            sites.insert((
                ReplayOwner::GlobalObject,
                file_ordinal,
                span.start,
                span.end,
            ));
            #[cfg(any(test, feature = "test-utils"))]
            exact_owner_sites.push(CollisionReplayOwnerSite {
                owner: ReplayOwner::GlobalObject,
                file_ordinal,
                span,
                provenance: super::replay_index::CollisionReplaySiteProvenance::GlobalContributor {
                    name,
                    kind,
                    binder_owner: crate::binder::namespace::DeclarationOwner::CompilationGlobal,
                },
            });
        }
    }
    for (file_ordinal, span) in explicit_global_this_sites {
        sites.insert((
            ReplayOwner::GlobalObject,
            file_ordinal,
            span.start,
            span.end,
        ));
        #[cfg(any(test, feature = "test-utils"))]
        exact_owner_sites.push(CollisionReplayOwnerSite {
            owner: ReplayOwner::GlobalObject,
            file_ordinal,
            span,
            provenance: super::replay_index::CollisionReplaySiteProvenance::ExplicitGlobalThis {
                binder_owner: crate::binder::namespace::DeclarationOwner::CompilationGlobal,
            },
        });
    }
    validate_root_census(&candidates, &root_rows, explicit_global_this)
        .map_err(InjectedProfileError::ReplayIndex)?;
    let mut roots = Vec::with_capacity(root_rows.len());
    {
        let _global_scope = trace.scope(ReplayOwner::GlobalObject);
        for row in root_rows {
            let candidate = candidates
                .get(&row.name)
                .expect("validated root rows have exact source provenance");
            let contributor = candidate.global_object_contributor;
            if contributor {
                if let Some(value) = row.value {
                    trace.demand(ReplayOwner::Value(value));
                }
                if let Some(ty) = row.ty {
                    trace.demand(ReplayOwner::TypeGroup(ty));
                }
                if let Some(namespace) = row.namespace {
                    trace.demand(ReplayOwner::Namespace(namespace));
                }
            }
            roots.push(ReplayRootSlot {
                explicit_global_this: explicit_global_this && row.name == "globalThis",
                name: row.name,
                value: row.value,
                ty: row.ty,
                namespace: row.namespace,
                global_object_contributor: contributor,
            });
        }
    }

    let mut record_bytes = BTreeMap::<LibraryEventKey, Vec<Vec<u8>>>::new();
    #[cfg(any(test, feature = "test-utils"))]
    let mut record_rows = BTreeMap::<LibraryEventKey, Vec<&CheckerRecord>>::new();
    for (key, record) in records {
        let input = canonical_input_for_record(canonical, key.file_ordinal)?;
        record_bytes
            .entry(*key)
            .or_default()
            .push(canonical_record_bytes(canonical[input].source, record)?);
        #[cfg(any(test, feature = "test-utils"))]
        record_rows.entry(*key).or_default().push(record);
    }
    let baselines = owners
        .iter()
        .copied()
        .map(|owner| {
            let records = match owner {
                ReplayOwner::Statement(key) => {
                    record_bytes.get(&key).map_or(&[][..], Vec::as_slice)
                }
                _ => &[],
            };
            baseline_record(owner, records).map_err(InjectedProfileError::ReplayIndex)
        })
        .collect::<Result<Vec<_>, _>>()?;
    #[cfg(any(test, feature = "test-utils"))]
    let structured_baselines = record_rows
        .iter()
        .map(|(key, records)| {
            full_structured_record_baseline(ReplayOwner::Statement(*key), records)
        })
        .collect::<Vec<_>>();

    validate_terminal_class_dependencies(
        Some(&trace),
        interner,
        ReplayTerminalValidationInputs {
            binder,
            published,
            decl_types,
            namespace_terminals,
            runtime,
            semantic_identities,
        },
    )?;
    let owner_sites = sites
        .into_iter()
        .map(|(owner, file_ordinal, start, end)| ReplayOwnerSite {
            owner,
            file_ordinal,
            span: Span::new(start, end),
        })
        .collect();
    let replay_index = trace
        .finish(owners, roots, owner_sites, baselines, invalid_sites)
        .map_err(InjectedProfileError::ReplayIndex)?;
    #[cfg(any(test, feature = "test-utils"))]
    {
        canonicalize_collision_replay_owner_sites(&mut exact_owner_sites);
        FULL_COLLISION_PLAN_ORACLE.with(|oracle| {
            *oracle.borrow_mut() = Some(FullCollisionPlanOracleForTest {
                owner_sites: exact_owner_sites,
                baseline_records: structured_baselines,
                reverse_edges: replay_index.reverse_edges.clone(),
            });
        });
    }
    Ok(replay_index)
}

#[cfg(any(test, feature = "test-utils"))]
fn collision_declaration_provenance(
    binder: &Binder,
    declaration: &crate::binder::declaration::LexicalDeclaration,
    declaration_scope: ScopeId,
) -> super::replay_index::CollisionReplaySiteProvenance {
    let binder_owner = binder
        .namespaces
        .declaration_owner_for_scope(declaration_scope);
    let containing_namespace = match binder_owner {
        crate::binder::namespace::DeclarationOwner::NamespacePublic(namespace) => Some(namespace),
        crate::binder::namespace::DeclarationOwner::NamespacePrivate(fragment) => binder
            .namespaces
            .fragment(fragment)
            .map(|row| row.namespace),
        _ => None,
    };
    super::replay_index::CollisionReplaySiteProvenance::Declaration {
        declaration: declaration.id,
        kind: declaration.kind,
        binder_owner,
        containing_namespace,
    }
}

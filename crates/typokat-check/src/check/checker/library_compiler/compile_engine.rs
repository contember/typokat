use super::*;

pub(super) fn compile_owned_injected_frontend_for_route<Route: InjectedCompileRoute>(
    frontend: CanonicalLibraryFrontend<'_, '_>,
    replay_index_plan: ReplayIndexPlan,
    record_retention: LibraryRecordRetention,
    evidence_capture: CanonicalLibraryEvidenceCapture,
    owner_site_storage_mode: OwnerSiteStorageMode,
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        Option<std::sync::Arc<CollisionReplayPlan>>,
    ),
    InjectedProfileError,
> {
    let collision_plan_work_scope = CollisionPlanForbiddenWorkScope::start();
    record_collision_plan_forbidden_work(|work| {
        work.library_source_compiles = work.library_source_compiles.saturating_add(1);
    });
    let CanonicalLibraryFrontend {
        canonical,
        parsed,
        declaration_manifests,
        binder,
        module_scopes,
        semantic_scopes,
        user_start,
        user_units,
        mut user_events,
        combined_lexical_events,
        mut external_effects,
        module_placeholders,
        collision_root_provenance,
        #[cfg(any(test, feature = "test-utils"))]
        parse_elapsed,
        #[cfg(any(test, feature = "test-utils"))]
        bind_elapsed,
    } = frontend;
    validate_complete_source_user_units(canonical.len(), user_start, &user_units)?;
    #[cfg(any(test, feature = "test-utils"))]
    let user_file_ordinals =
        complete_source_user_file_ordinals_for_test(&canonical, user_start, &user_units)?;
    let source_unit_for_index = |index: usize, input: &CanonicalInput<'_>| {
        user_start
            .and_then(|start| index.checked_sub(start))
            .and_then(|user_index| user_units.get(user_index).copied())
            .map_or_else(
                || library_unit(input.file_ordinal),
                |(module_ordinal, unit_slot)| SourceUnit::User {
                    module_ordinal,
                    unit_slot,
                },
            )
    };
    #[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
    CANONICAL_FRONTEND_FULL_PRODUCTS.set(CANONICAL_FRONTEND_FULL_PRODUCTS.get().saturating_add(1));
    #[cfg(any(test, feature = "test-utils"))]
    let independent_event_owner_sites = if replay_index_plan == ReplayIndexPlan::None {
        Vec::new()
    } else {
        independent_event_owner_sites_for_oracle(
            &canonical,
            &parsed,
            &declaration_manifests,
            &binder,
        )?
    };

    #[cfg(any(test, feature = "test-utils"))]
    let reserve_fill_started = Instant::now();
    let retain_library_records = matches!(record_retention, LibraryRecordRetention::Collect);
    let mut ledger = if replay_index_plan == ReplayIndexPlan::None {
        LibraryEventLedger::new_without_replay(retain_library_records)
    } else {
        LibraryEventLedger::new_with_owner_site_storage(
            retain_library_records,
            owner_site_storage_mode,
        )
    };
    let mut lexical_events = Route::lexical_events(combined_lexical_events)?;
    for (index, ((input, program), manifest)) in canonical
        .iter()
        .zip(&parsed)
        .zip(&declaration_manifests)
        .enumerate()
    {
        if user_start.is_some_and(|user_start| index >= user_start) {
            continue;
        }
        Route::reserve_library_program(
            &mut lexical_events,
            input.file_ordinal,
            program,
            manifest,
            &mut ledger,
        )
        .map_err(InjectedProfileError::Reporting)?;
    }
    let mut declaration_inventories = Vec::with_capacity(canonical.len());
    let mut declaration_fragment_batch =
        super::decls::inventory::CanonicalFragmentAuthenticationBatch::default();
    for (((input, _program), module), manifest) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(declaration_manifests)
    {
        let inventory = super::decls::inventory::BoundTypeDeclarationInventory::build_in_batch(
            &binder,
            module,
            input.source_key,
            manifest,
            None,
            &mut declaration_fragment_batch,
        )
        .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
        declaration_inventories.push(inventory);
    }

    let combined_library_declarations = user_start.map(|_| {
        canonical
            .iter()
            .zip(&declaration_inventories)
            .filter(|(input, _)| matches!(input.origin, CompilationOrigin::Library(_)))
            .flat_map(|(_, inventory)| inventory.declarations().iter().map(|row| row.declaration))
            .collect::<rustc_hash::FxHashSet<_>>()
    });
    #[cfg(any(test, feature = "test-utils"))]
    let mut combined_library_provenance_checked = false;

    let mut interner = Interner::with_intrinsics();
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_decls: super::context::TypeDeclTable<'_> = Vec::new().into();
    let mut type_resolved: super::context::TypeResolvedTable =
        vec![None; binder.type_groups.len()].into();
    #[cfg(any(test, feature = "test-utils"))]
    let initial_store_rows = interner.store().len();
    #[cfg(any(test, feature = "test-utils"))]
    let initial_published_type_rows = type_decls.published_len();
    #[cfg(any(test, feature = "test-utils"))]
    let initial_replacement_type_rows = type_decls.replacement_indices().len();
    #[cfg(any(test, feature = "test-utils"))]
    let initial_type_param_id = next_type_param;
    #[cfg(any(test, feature = "test-utils"))]
    let initial_class_id = next_class_id;
    let declaration_spans = super::ModuleDeclarationSpans::index(&binder);
    for (index, (((input, _program), scope), inventory)) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(&declaration_inventories)
        .enumerate()
    {
        let reservation_receipt =
            if user_start.is_some() && matches!(input.origin, CompilationOrigin::Library(_)) {
                reserve_type_decls_for_combined_library_from_inventory(
                    super::decls::TypeDeclReservationState {
                        interner: &mut interner,
                        binder: &binder,
                        next_type_param: &mut next_type_param,
                        next_class_id: &mut next_class_id,
                        decls: &mut type_decls,
                        resolved: &mut type_resolved,
                    },
                    inventory,
                )
            } else if user_start.is_some() {
                let Some(library_declarations) = combined_library_declarations.as_ref() else {
                    return Err(InjectedProfileError::CanonicalProjection(
                        "complete-source library declaration provenance is missing".to_owned(),
                    ));
                };
                #[cfg(any(test, feature = "test-utils"))]
                if !combined_library_provenance_checked {
                    let legacy_library_declarations = binder
                        .declarations
                        .iter()
                        .filter(|declaration| {
                            declaration.type_group.is_some()
                                && matches!(
                                    declaration.kind,
                                    DeclarationKind::TypeAlias
                                        | DeclarationKind::Interface
                                        | DeclarationKind::Class
                                        | DeclarationKind::Enum
                                )
                                && lexical_events
                                    .declaration_source(declaration.id)
                                    .is_some_and(|source| {
                                        matches!(source.unit, SourceUnit::Library { .. })
                                    })
                        })
                        .map(|declaration| declaration.id)
                        .collect::<rustc_hash::FxHashSet<_>>();
                    assert_eq!(
                        library_declarations, &legacy_library_declarations,
                        "combined library inventory rows match legacy provenance",
                    );
                    combined_library_provenance_checked = true;
                }
                reserve_type_decls_for_combined_user_from_inventory(
                    super::decls::TypeDeclReservationState {
                        interner: &mut interner,
                        binder: &binder,
                        next_type_param: &mut next_type_param,
                        next_class_id: &mut next_class_id,
                        decls: &mut type_decls,
                        resolved: &mut type_resolved,
                    },
                    inventory,
                    library_declarations,
                )
            } else {
                reserve_type_decls_from_inventory(
                    &mut interner,
                    &binder,
                    inventory,
                    &mut next_type_param,
                    &mut next_class_id,
                    &mut type_decls,
                    &mut type_resolved,
                )
            };
        if let Some(user_index) = user_start.and_then(|start| index.checked_sub(start)) {
            let Some((module_ordinal, _)) = user_units.get(user_index).copied() else {
                return Err(InjectedProfileError::CanonicalProjection(
                    "complete-source user unit identity is missing".to_owned(),
                ));
            };
            Route::attach_user_owners(
                &mut lexical_events,
                module_ordinal,
                &binder,
                scope,
                &declaration_spans,
                &type_decls,
                inventory,
                reservation_receipt,
            )?;
        } else {
            Route::attach_library_owners(
                &mut lexical_events,
                input.file_ordinal,
                &binder,
                scope,
                &declaration_spans,
                &type_decls,
                inventory,
                reservation_receipt,
            )?;
        }
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
    seed_trusted_library_markers(
        &collision_root_provenance,
        &mut type_decls,
        &mut type_resolved,
        &interner,
    );

    let replay_class_declarations = if replay_index_plan == ReplayIndexPlan::Assemble {
        lexical_events
            .classes()
            .iter()
            .filter_map(|reservation| {
                let binding = reservation.binding.as_ref()?;
                Some((binding.class_id, binding.declaration))
            })
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };

    let reporting_receipts = LibraryReportingConsumer::new(&mut ledger)
        .consume_binder_outcomes(&binder)
        .map_err(InjectedProfileError::Reporting)?;
    #[cfg(not(any(test, feature = "test-utils")))]
    let _ = &reporting_receipts;
    if user_start.is_some() {
        enqueue_local_ambient_export_alias_diagnostics(
            &binder,
            &lexical_events,
            &mut external_effects,
        )
        .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
        enqueue_namespace_placement_diagnostics(&binder, &lexical_events, &mut external_effects)
            .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
        enqueue_enum_merge_diagnostics(&binder, &lexical_events, &mut external_effects)
            .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
        enqueue_ambient_context_diagnostics(&binder, &lexical_events, &mut external_effects)
            .map_err(|error| InjectedProfileError::Binder(error.to_owned()))?;
    }
    let mut decl_types = DeclTypes::new(binder.decl_count);
    let error = interner.well_known().error;
    super::seed_module_placeholder_errors(&mut decl_types, &module_placeholders, error);
    let user_declaration_contributors = binder
        .declarations
        .iter()
        .filter(|declaration| {
            lexical_events
                .declaration_source(declaration.id)
                .is_some_and(|source| matches!(source.unit, SourceUnit::User { .. }))
        })
        .map(|declaration| declaration.id)
        .collect::<rustc_hash::FxHashSet<_>>();
    let user_value_contributors = binder
        .declarations
        .iter()
        .filter(|declaration| user_declaration_contributors.contains(&declaration.id))
        .filter_map(|declaration| declaration.value_storage)
        .collect::<rustc_hash::FxHashSet<ValueStorageId>>();
    #[cfg(any(test, feature = "test-utils"))]
    if super::complete_source::complete_combined_replay_oracle_is_active() {
        let library_file_ordinals = canonical
            .iter()
            .filter(|input| matches!(input.origin, CompilationOrigin::Library(_)))
            .map(|input| input.file_ordinal)
            .collect::<BTreeSet<_>>();
        let library_event_count = independent_event_owner_sites
            .iter()
            .filter_map(|site| match site.owner {
                ReplayOwner::Statement(key)
                    if library_file_ordinals.contains(&key.file_ordinal) =>
                {
                    Some((key.file_ordinal, key.event_ordinal))
                }
                ReplayOwner::TypeGroup(_)
                | ReplayOwner::Value(_)
                | ReplayOwner::Namespace(_)
                | ReplayOwner::Class(_)
                | ReplayOwner::GlobalObject
                | ReplayOwner::Statement(_) => None,
            })
            .collect::<BTreeSet<_>>()
            .len();
        let mut user_ticket_rows = BTreeMap::new();
        let mut user_event_keys = BTreeMap::new();
        let mut mapped_tickets = BTreeSet::new();
        for (ticket, key) in user_events.reserved_ticket_keys_for_test() {
            let Some(file_ordinal) = user_file_ordinals.get(&key.module_ordinal).copied() else {
                return Err(InjectedProfileError::CanonicalProjection(format!(
                    "combined replay oracle event {} references unknown user module {}",
                    ticket.event.index(),
                    key.module_ordinal.index(),
                )));
            };
            if ticket.record_ordinal != key.record_ordinal {
                return Err(InjectedProfileError::CanonicalProjection(
                    "combined replay oracle user ticket record metadata is permuted".to_owned(),
                ));
            }
            let Some(mapped_event) = library_event_count.checked_add(ticket.event.index()) else {
                return Err(InjectedProfileError::SourceKeyOverflow);
            };
            let mapped_ticket = (mapped_event, ticket.record_ordinal);
            let event_key = LibraryEventKey {
                file_ordinal,
                source_start: key.source_start,
                event_ordinal: key.event_ordinal,
                record_ordinal: key.record_ordinal,
            };
            if !mapped_tickets.insert(mapped_ticket)
                || user_event_keys.insert(event_key, ticket).is_some()
                || user_ticket_rows
                    .insert(ticket, (mapped_ticket, event_key))
                    .is_some()
            {
                return Err(InjectedProfileError::CanonicalProjection(
                    "combined replay oracle user ticket metadata is not bijective".to_owned(),
                ));
            }
        }
        if user_ticket_rows.len() != user_event_keys.len()
            || user_ticket_rows.len() != mapped_tickets.len()
        {
            return Err(InjectedProfileError::CanonicalProjection(
                "combined replay oracle user ticket translation is incomplete".to_owned(),
            ));
        }
        super::complete_source::set_complete_combined_replay_oracle_user_tickets(
            library_event_count,
            user_ticket_rows,
        );
    }
    let pending_tickets = Route::pending_tickets(&lexical_events);
    #[cfg(any(test, feature = "test-utils"))]
    if super::complete_source::complete_combined_replay_oracle_is_active() {
        let translated = super::complete_source::complete_combined_replay_oracle_user_tickets()
            .ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "combined replay oracle user ticket translation is missing".to_owned(),
                )
            })?;
        let pending_users = pending_tickets
            .iter()
            .filter_map(|ticket| Route::oracle_user_ticket(*ticket))
            .collect::<BTreeSet<_>>();
        if pending_users.len() != translated.len()
            || pending_users
                .iter()
                .any(|ticket| !translated.contains_key(ticket))
        {
            return Err(InjectedProfileError::CanonicalProjection(
                "combined replay oracle pending user tickets do not match EventStore metadata"
                    .to_owned(),
            ));
        }
    }
    let mut pass = build_pass_with_tickets(
        &mut interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReportingPlan {
            reporting: PassReporting {
                source: library_unit(canonical[0].file_ordinal),
                lexical_events,
                suppress_effects: false,
            },
            pending_tickets,
            ticket_key: Route::ticket_key(),
        },
    );
    pass.terminalize_unavailable_type_groups();
    if user_start.is_some() {
        pass.install_early_native_array_groups(NativeArrayGroups::select_from_library_roots(
            &collision_root_provenance,
        ));
    }
    for effects in external_effects.into_values() {
        let (owner, records) = effects.into_parts();
        let mut combined = super::context::CheckerEffects::new(Route::user_ticket(owner)?);
        for record in records {
            combined.records.record(record);
        }
        pass.enqueue_effects(combined);
    }
    pass.certified_library_values = certify_library_values(&collision_root_provenance);
    if user_start.is_some() {
        pass.combined_source_library_value_precedence = true;
        for group in collision_root_provenance
            .root_rows
            .iter()
            .filter_map(|row| row.ty)
        {
            let user_fragment = binder.type_groups.get(group).is_some_and(|group| {
                group
                    .fragments
                    .iter()
                    .any(|fragment| user_declaration_contributors.contains(&fragment.declaration))
            });
            if user_fragment {
                pass.combined_user_library_type_groups.insert(group);
            }
        }
        for row in &collision_root_provenance.root_rows {
            let Some(library_participant) = row.value else {
                continue;
            };
            let Some(symbol_id) = pass.resolve_value_replay(binder.compilation_global, &row.name)
            else {
                continue;
            };
            let Some(symbol) = binder.symbols.get(symbol_id) else {
                continue;
            };
            if symbol.value.is_some_and(|current| {
                current != library_participant && user_value_contributors.contains(&current)
            }) {
                pass.install_authenticated_collision_value_occurrences(
                    symbol_id,
                    library_participant,
                );
                pass.private_collision_value_winners
                    .insert(symbol_id, library_participant);
                pass.private_collision_value_winners_by_name
                    .insert(row.name.clone(), library_participant);
            }
            if symbol.function_values.len() > 1
                && symbol.function_values.contains(&library_participant)
                && symbol
                    .function_values
                    .iter()
                    .any(|participant| user_value_contributors.contains(participant))
            {
                pass.function_group_precedence_tails_by_name
                    .insert(row.name.clone(), library_participant);
            }
        }
    }
    if replay_index_plan == ReplayIndexPlan::None {
        ledger
            .seal_reporting_without_replay()
            .map_err(InjectedProfileError::Reporting)?;
    } else {
        let mut replay_trace_seed = ledger
            .take_replay_trace_seed()
            .map_err(InjectedProfileError::Reporting)?;
        #[cfg(any(test, feature = "test-utils"))]
        if super::complete_source::complete_combined_replay_oracle_is_active() {
            let translated = super::complete_source::complete_combined_replay_oracle_user_tickets()
                .ok_or_else(|| {
                    InjectedProfileError::CanonicalProjection(
                        "combined replay oracle user ticket translation is missing".to_owned(),
                    )
                })?;
            let user_files = user_file_ordinals
                .values()
                .copied()
                .collect::<BTreeSet<_>>();
            let user_owner_site_rows = independent_event_owner_sites
                .iter()
                .filter_map(|site| match site.owner {
                    ReplayOwner::Statement(owner) if user_files.contains(&owner.file_ordinal) => {
                        Some((owner, site.clone()))
                    }
                    ReplayOwner::TypeGroup(_)
                    | ReplayOwner::Value(_)
                    | ReplayOwner::Namespace(_)
                    | ReplayOwner::Class(_)
                    | ReplayOwner::GlobalObject
                    | ReplayOwner::Statement(_) => None,
                })
                .collect::<Vec<_>>();
            let mut user_owner_sites = user_owner_site_rows
                .iter()
                .cloned()
                .collect::<BTreeMap<_, _>>();
            if user_owner_site_rows.len() != user_owner_sites.len() {
                return Err(InjectedProfileError::CanonicalProjection(
                    format!(
                        "combined replay oracle user owner-site mapping contains duplicate EventKeys: owner-site rows={}, unique owner keys={}",
                        user_owner_site_rows.len(),
                        user_owner_sites.len(),
                    ),
                ));
            }
            let mut user_event_offset = 0;
            while replay_trace_seed.contains_owner_site_ticket((user_event_offset, 0)) {
                user_event_offset = user_event_offset.saturating_add(1);
            }
            if super::complete_source::complete_combined_replay_oracle_user_event_offset()
                != Some(user_event_offset)
            {
                return Err(InjectedProfileError::CanonicalProjection(
                    "combined replay oracle library event censuses disagree".to_owned(),
                ));
            }
            let translated_key_count = translated.len();
            let translated_key_range = translated.values().map(|(_, key)| *key).fold(
                None::<(LibraryEventKey, LibraryEventKey)>,
                |range, key| match range {
                    Some((first, last)) => Some((first.min(key), last.max(key))),
                    None => Some((key, key)),
                },
            );
            for (_user_ticket, (mapped_ticket, owner)) in translated {
                if !replay_trace_seed.reserve_owner_site_ticket(mapped_ticket, owner) {
                    return Err(InjectedProfileError::CanonicalProjection(
                        "combined replay oracle could not seed a user statement owner".to_owned(),
                    ));
                }
                if let Some(site) = user_owner_sites.remove(&owner) {
                    if replay_trace_seed.record_ticket_owner_site(mapped_ticket, site)
                        != Some(false)
                    {
                        return Err(InjectedProfileError::CanonicalProjection(
                            "combined replay oracle could not attach an independent user owner site"
                                .to_owned(),
                        ));
                    }
                }
            }
            if !user_owner_sites.is_empty() {
                let unmatched = user_owner_sites.keys().take(3).copied().collect::<Vec<_>>();
                return Err(InjectedProfileError::CanonicalProjection(
                    format!(
                        "combined replay oracle owner sites contain an EventStore permutation: remaining={}, first={unmatched:?}, translated={}, translated-range={translated_key_range:?}",
                        user_owner_sites.len(),
                        translated_key_count,
                    ),
                ));
            }
        }
        #[cfg(any(test, feature = "test-utils"))]
        if FORCED_COLLISION_PLAN_FAILURE.get()
            == Some(ForcedCollisionPlanFailure::LateOwnerReservation)
        {
            let _late = ledger
                .replay_reservation_domain()
                .map_err(InjectedProfileError::Reporting)?;
        }
        replay_trace_seed.extend_owner_sites(pass.lexical_events.take_collision_owner_sites());
        pass.replay_trace = Some(ReplayDependencyTrace::new(replay_trace_seed));
    }
    pass.capture_compact_replay_dependencies = replay_index_plan != ReplayIndexPlan::None;

    let declaration_count = pass.type_decls.len();
    pass.fill_type_decls_range(binder.module, 0, declaration_count);
    let module_programs = module_scopes
        .iter()
        .copied()
        .zip(parsed.iter())
        .map(|(scope, program)| (scope, program.body.as_slice()))
        .collect::<Vec<_>>();
    if let Some(user_start) = user_start {
        for (index, ((scope, program), input)) in module_scopes
            .iter()
            .copied()
            .zip(&parsed)
            .zip(&canonical)
            .enumerate()
            .skip(user_start)
        {
            pass.current_module = scope;
            pass.current_source = source_unit_for_index(index, input);
            pass.reserve_local_type_annotation_surfaces(scope, &program.body);
        }
    }
    for (index, (((input, program), module), semantic_scope)) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
        .enumerate()
    {
        pass.current_module = module;
        pass.current_source = source_unit_for_index(index, input);
        pass.pre_reserve_query_free_var_annotation_surfaces(semantic_scope, &program.body);
    }
    pass.combined_user_source = user_start.is_some();
    pass.prepare_project_attached_namespace_values(&module_programs);
    pass.prepare_project_standalone_namespace_values(&module_programs);
    pass.combined_user_source = false;
    pass.publish_class_surfaces();
    let mut surfaces = Vec::with_capacity(canonical.len());
    for (index, (((input, program), module), semantic_scope)) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
        .enumerate()
    {
        pass.combined_user_source = user_start.is_some_and(|user_start| index >= user_start);
        pass.current_module = module;
        pass.current_source = source_unit_for_index(index, input);
        let mut reserved = pass.reserve_function_surfaces(semantic_scope, &program.body);
        pass.reserve_continuation_global_augmentation_surfaces(&program.body, &mut reserved);
        surfaces.push(reserved);
    }
    pass.fill_enum_decls_range(0, declaration_count);
    #[cfg(any(test, feature = "test-utils"))]
    let reserve_fill_elapsed = reserve_fill_started.elapsed();

    #[cfg(any(test, feature = "test-utils"))]
    let publication_validation_started = Instant::now();
    if user_start.is_none() {
        pass.finalize_standalone_namespace_values();
        pass.precompute_standalone_namespace_value_aliases(&module_programs);
    }
    pass.fill_pending_interfaces_range(binder.module, 0, declaration_count);
    if let Some(user_start) = user_start {
        let user_namespace_modules = module_scopes[user_start..]
            .iter()
            .copied()
            .collect::<rustc_hash::FxHashSet<_>>();
        pass.refresh_colliding_standalone_variable_surfaces(
            &module_programs,
            &user_namespace_modules,
        );
        pass.finalize_standalone_namespace_values();
        pass.precompute_standalone_namespace_value_aliases(&module_programs);
    }
    let publication = pass.publish_type_groups();
    if publication.library_identity_selection_pending() {
        return Err(InjectedProfileError::LibraryIdentitySelectionPending);
    }
    let publication_validations = publication.publication_validations();
    pass.validate_published_class_surfaces();
    #[cfg(any(test, feature = "test-utils"))]
    let lexical_source_units = pass
        .lexical_events
        .retained_source_units()
        .into_iter()
        .filter(|unit| matches!(unit, crate::source::SourceUnit::Library { .. }))
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let publication_validation_elapsed = publication_validation_started.elapsed();

    #[cfg(any(test, feature = "test-utils"))]
    let statement_check_started = Instant::now();
    let mut pass_source_units = Vec::with_capacity(canonical.len());
    let mut mixed_semantic_identities = None;
    let mut user_global_contributors = Vec::new();
    let mut user_global_contributor_names = Vec::new();
    if let Some(user_start) = user_start {
        let identities = LibrarySemanticIdentities::select_from_library_roots(
            &collision_root_provenance,
            pass.type_environment.published(),
            pass.interner.store(),
        );
        pass.install_library_semantic_identities(identities.clone());
        mixed_semantic_identities = Some(identities);
        let base_global_object = pass.with_replay_owner(ReplayOwner::GlobalObject, |pass| {
            intern_compiled_global_object(pass, &collision_root_provenance, &[])
        });
        pass.global_object_type = Some(base_global_object);

        #[cfg(any(test, feature = "test-utils"))]
        let oracle_global_object_scope =
            super::complete_source::complete_combined_replay_oracle_is_active()
                .then(|| {
                    pass.replay_trace
                        .as_ref()
                        .map(|trace| trace.scope(ReplayOwner::GlobalObject))
                })
                .flatten();
        for (((user_input, user_program), _user_module), semantic_scope) in canonical
            .iter()
            .zip(&parsed)
            .zip(module_scopes.iter().copied())
            .zip(semantic_scopes.iter().copied())
            .skip(user_start)
        {
            let census = source_global_binding_census_with_provenance(
                user_program,
                ModuleBindingContext::for_program(user_program, user_input.kind),
            );
            let mut contributor_names = rustc_hash::FxHashSet::default();
            for (name, candidate) in census.census.candidates {
                if name == "globalThis" || !candidate.global_object_contributor {
                    continue;
                }
                contributor_names.insert(name.clone());
                let Some(storage) = pass.value_decl_id_replay(semantic_scope, &name) else {
                    continue;
                };
                user_global_contributors.push((name, storage));
            }
            user_global_contributor_names.push(contributor_names);
        }
        #[cfg(any(test, feature = "test-utils"))]
        drop(oracle_global_object_scope);
    }
    for (index, (((input, program), module), semantic_scope)) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
        .enumerate()
    {
        pass.combined_user_source = user_start.is_some_and(|user_start| index >= user_start);
        pass.current_module = module;
        pass.current_source = source_unit_for_index(index, input);
        pass.reserve_var_annotation_surfaces(semantic_scope, &program.body);
    }
    pass.combined_user_source = false;
    if let Some(user_start) = user_start {
        let global_object = pass.with_replay_owner(ReplayOwner::GlobalObject, |pass| {
            intern_compiled_global_object(
                pass,
                &collision_root_provenance,
                &user_global_contributors,
            )
        });
        pass.global_object_type = Some(global_object);
        #[cfg(any(test, feature = "test-utils"))]
        let oracle_global_refresh_scope =
            super::complete_source::complete_combined_replay_oracle_is_active()
                .then(|| {
                    pass.replay_trace
                        .as_ref()
                        .map(|trace| trace.scope(ReplayOwner::GlobalObject))
                })
                .flatten();
        pass.refresh_user_global_object(
            canonical
                .iter()
                .zip(&parsed)
                .zip(module_scopes.iter().copied())
                .skip(user_start)
                .map(|((input, program), module)| {
                    (
                        module,
                        *program,
                        ModuleBindingContext::for_program(program, input.kind),
                    )
                }),
        );
        #[cfg(any(test, feature = "test-utils"))]
        drop(oracle_global_refresh_scope);
    }
    for (index, ((((input, program), module), semantic_scope), mut reserved)) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
        .zip(surfaces)
        .enumerate()
    {
        if user_start == Some(index) {
            pass.combined_user_source = true;
        }
        pass.current_module = module;
        pass.current_source = source_unit_for_index(index, input);
        pass_source_units.push(pass.current_source);
        pass.build_flow_graph(semantic_scope, &program.body);
        let mut no_return = None;
        if let Some(contributor_names) = user_start
            .and_then(|start| index.checked_sub(start))
            .and_then(|user_index| user_global_contributor_names.get(user_index))
        {
            pass.check_statement_list_with_global_contributors(
                semantic_scope,
                &program.body,
                None,
                &mut no_return,
                &mut reserved,
                contributor_names,
            );
        } else {
            pass.check_statement_list_with_surfaces(
                semantic_scope,
                &program.body,
                None,
                &mut no_return,
                &mut reserved,
            );
        }
        if matches!(pass.current_source, SourceUnit::User { .. }) {
            super::emit_test_incomplete(&mut pass);
        }
    }
    let batches = finish_semantic_effects(&mut pass);
    let semantic_identities = mixed_semantic_identities.unwrap_or_else(|| {
        LibrarySemanticIdentities::select(
            &binder,
            pass.type_environment.published(),
            pass.interner.store(),
        )
    });
    #[cfg(any(test, feature = "test-utils"))]
    let (global_types, module_types) = collect_type_probes(
        &binder,
        pass.type_environment.published(),
        pass.interner.store(),
        &canonical,
        &module_scopes,
    );
    #[cfg(any(test, feature = "test-utils"))]
    let global_values = collect_value_probes(
        &binder,
        &pass.decl_types,
        pass.interner.store(),
        &pass.namespace_values,
    );
    Route::complete_batches(batches, &mut ledger, &mut user_events)?;
    let global_object_type = pass.global_object_type.or_else(|| {
        Some(pass.with_replay_owner(ReplayOwner::GlobalObject, |pass| {
            intern_compiled_global_object(pass, &collision_root_provenance, &[])
        }))
    });
    let snapshot = ledger.snapshot();
    let ledger_output = if replay_index_plan == ReplayIndexPlan::None {
        super::events_library::LibraryEventLedgerOutput {
            records: ledger.finish().map_err(InjectedProfileError::Reporting)?,
            fingerprints: Vec::new(),
        }
    } else {
        ledger
            .finish_with_fingerprints()
            .map_err(InjectedProfileError::Reporting)?
    };
    let library_records = ledger_output.records;
    let ordered_user_records = user_events
        .finish()
        .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;
    #[cfg(any(test, feature = "test-utils"))]
    let ordered_user_record_evidence = ordered_user_records
        .iter()
        .map(|(key, record)| {
            let file_ordinal = user_file_ordinals
                .get(&key.module_ordinal)
                .copied()
                .ok_or_else(|| {
                    InjectedProfileError::CanonicalProjection(format!(
                        "complete-source user file ordinal mapping omits module ordinal {}",
                        key.module_ordinal.index()
                    ))
                })?;
            Ok((file_ordinal, record.clone()))
        })
        .collect::<Result<Vec<_>, InjectedProfileError>>()?;
    #[cfg(any(test, feature = "test-utils"))]
    let ordered_user_event_record_evidence = ordered_user_records
        .iter()
        .map(|(key, record)| {
            let source_ordinal = user_file_ordinals
                .get(&key.module_ordinal)
                .copied()
                .ok_or_else(|| {
                    InjectedProfileError::CanonicalProjection(format!(
                        "complete-source user file ordinal mapping omits module ordinal {}",
                        key.module_ordinal.index()
                    ))
                })?;
            Ok((source_ordinal, *key, record.clone()))
        })
        .collect::<Result<Vec<_>, InjectedProfileError>>()?;
    let user_records = ordered_user_records.into_iter().fold(
        BTreeMap::<ModuleOrdinal, Vec<_>>::new(),
        |mut records, (key, record)| {
            records.entry(key.module_ordinal).or_default().push(record);
            records
        },
    );
    let user_results = assemble_complete_source_user_results(&user_units, user_records)?;
    #[cfg(any(test, feature = "test-utils"))]
    if let (CanonicalLibraryEvidenceCapture::Capture, Some(library_count)) =
        (evidence_capture, user_start)
    {
        let evidence_sources = canonical[..library_count]
            .iter()
            .map(|input| InjectedLibrarySource {
                file_ordinal: input.file_ordinal,
                name: "",
                source: input.source,
            })
            .collect::<Vec<_>>();
        let evidence = canonical_library_evidence_for_test(&evidence_sources, &library_records)?;
        record_complete_source_evidence_for_test(evidence);
    }
    #[cfg(not(any(test, feature = "test-utils")))]
    let _ = evidence_capture;
    let replay_baselines = ledger_output
        .fingerprints
        .into_iter()
        .map(|fingerprint| super::replay_index::ReplayBaselineRecord {
            owner: ReplayOwner::Statement(fingerprint.key),
            record_count: fingerprint.record_count,
            digest: fingerprint.digest,
        })
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let statement_check_elapsed = statement_check_started.elapsed();

    let namespace_terminals = pass
        .namespace_values
        .try_freeze_terminals()
        .map_err(|message| InjectedProfileError::CanonicalProjection(message.to_owned()))?;
    let mut replay_trace = pass.replay_trace.take();
    if replay_index_plan != ReplayIndexPlan::None && replay_trace.is_none() {
        return Err(InjectedProfileError::CanonicalProjection(
            "source library compilation lost its replay trace".to_owned(),
        ));
    }
    let compact_only_replay_edges = pass.compact_only_replay_edges.borrow().clone();
    if replay_index_plan == ReplayIndexPlan::Assemble {
        if let Some(trace) = replay_trace.as_ref() {
            trace.remove_direct_dependencies(&compact_only_replay_edges);
        }
    }
    #[cfg(any(test, feature = "test-utils"))]
    if let Some(replay_trace) = replay_trace.as_ref() {
        match FORCED_COLLISION_PLAN_FAILURE.get() {
            Some(ForcedCollisionPlanFailure::UnownedTypedReference) => {
                let observation =
                    replay_trace.observe_typed_demand("forced-unowned-typed-reference");
                drop(observation);
            }
            Some(ForcedCollisionPlanFailure::RawSemanticAccess) => {
                replay_trace.record_raw_semantic_access();
            }
            Some(ForcedCollisionPlanFailure::ForbiddenProjection) => {
                let _ = snapshot_namespace_terminals_for_replay(&namespace_terminals)?;
            }
            Some(
                ForcedCollisionPlanFailure::EventCaptureCorruption
                | ForcedCollisionPlanFailure::LateOwnerReservation,
            ) => {}
            None => {}
        }
    }
    let super::context::Pass {
        type_environment,
        decl_types,
        next_type_param,
        class_application_parameters,
        class_new_metadata,
        class_parents,
        class_value_aliases,
        class_value_bindings,
        standalone_namespace_value_aliases,
        class_names,
        function_groups,
        certified_library_values,
        ..
    } = pass;
    let super::type_groups::TypeEnvironmentState::Published(published_types) = type_environment
    else {
        return Err(InjectedProfileError::CanonicalProjection(
            "owned library runtime requires a published environment".to_owned(),
        ));
    };
    let named_function_symbols = function_groups.frozen_symbols();
    let runtime = FrozenCheckerRuntimeMetadata {
        class_application_parameters,
        class_new_metadata,
        class_parents,
        class_value_aliases,
        class_value_bindings,
        standalone_namespace_value_aliases,
        class_names,
        namespace_terminals,
        named_function_symbols: named_function_symbols.into(),
        global_object_type,
        certified_library_values,
    };
    let selected_semantic_identities = semantic_identities
        .all_ready()
        .then_some(semantic_identities.clone());
    let (replay_index, collision_plan) = match replay_index_plan {
        ReplayIndexPlan::Assemble => {
            let replay_trace = replay_trace.take().ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "full replay assembly lost its source trace".to_owned(),
                )
            })?;
            let namespace_terminal_rows =
                snapshot_namespace_terminals_for_replay(&runtime.namespace_terminals)?;
            let runtime_parts = runtime
                .snapshot_parts()
                .map_err(|message| InjectedProfileError::CanonicalProjection(message.to_owned()))?;
            record_collision_plan_forbidden_work(|work| {
                let rows = runtime_parts
                    .class_application_parameters
                    .len()
                    .saturating_add(runtime_parts.class_new_metadata.len())
                    .saturating_add(runtime_parts.class_parents.len())
                    .saturating_add(runtime_parts.class_value_aliases.len())
                    .saturating_add(runtime_parts.class_value_bindings.len())
                    .saturating_add(runtime_parts.standalone_namespace_value_aliases.len())
                    .saturating_add(runtime_parts.class_names.len())
                    .saturating_add(runtime_parts.namespace_terminals.len())
                    .saturating_add(runtime_parts.named_function_symbols.len())
                    .saturating_add(usize::from(runtime_parts.global_object_type.is_some()));
                work.runtime_snapshot_rows = work
                    .runtime_snapshot_rows
                    .saturating_add(u64::try_from(rows).unwrap_or(u64::MAX));
            });
            let replay_index = build_collision_replay_index(
                replay_trace,
                &interner,
                &binder,
                &published_types,
                &decl_types,
                &namespace_terminal_rows,
                &runtime_parts,
                selected_semantic_identities.as_ref(),
                &canonical,
                &parsed,
                &module_scopes,
                &replay_class_declarations,
                &library_records,
                #[cfg(any(test, feature = "test-utils"))]
                independent_event_owner_sites,
            )?;
            #[cfg(any(test, feature = "test-utils"))]
            FULL_COLLISION_PLAN_ORACLE.with(|oracle| {
                if let Some(oracle) = oracle.borrow_mut().as_mut() {
                    oracle
                        .reverse_edges
                        .extend(compact_only_replay_edges.iter().copied());
                    oracle.reverse_edges.sort();
                    oracle.reverse_edges.dedup();
                }
            });
            #[cfg(any(test, feature = "test-utils"))]
            let mut replay_roots = collect_root_rows(&binder)
                .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?
                .into_iter()
                .map(|row| (row.name, row.value, row.ty, row.namespace))
                .collect::<Vec<_>>();
            #[cfg(not(any(test, feature = "test-utils")))]
            let replay_roots = collect_root_rows(&binder)
                .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?
                .into_iter()
                .map(|row| (row.name, row.value, row.ty, row.namespace))
                .collect::<Vec<_>>();
            #[cfg(any(test, feature = "test-utils"))]
            if super::complete_source::complete_combined_replay_oracle_is_active() {
                let populated_root_names = replay_index
                    .root_slots
                    .iter()
                    .filter(|root| {
                        root.value.is_some() || root.ty.is_some() || root.namespace.is_some()
                    })
                    .map(|root| root.name.as_str())
                    .collect::<BTreeSet<_>>();
                replay_roots.retain(|root| populated_root_names.contains(root.0.as_str()));
            }
            let replay_index = admit_generated_collision_replay_index(
                replay_index,
                ReplayIndexAdmissionLimits {
                    type_groups: binder.type_groups.len(),
                    value_storages: decl_types.len(),
                    namespaces: binder.namespaces.len(),
                    classes: usize::try_from(next_class_id)
                        .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
                    source_files: canonical
                        .iter()
                        .map(|input| input.file_ordinal.index().saturating_add(1))
                        .max()
                        .unwrap_or(0),
                    roots: &replay_roots,
                },
                None,
            )
            .map_err(InjectedProfileError::ReplayIndexAdmission)?;
            (Some(Box::new(replay_index)), None)
        }
        // The production path consumes the one live source trace directly into the compact plan.
        ReplayIndexPlan::Deferred => {
            let replay_trace = replay_trace.take().ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "deferred replay plan lost its source trace".to_owned(),
                )
            })?;
            let forbidden_work = collision_plan_work_scope.finish();
            let prefix_cardinalities = [
                interner.store().len(),
                usize::try_from(next_type_param)
                    .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
                usize::try_from(next_class_id)
                    .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
                binder.graph.len(),
                binder.symbols.len(),
                binder.declarations.len(),
                binder.type_groups.len(),
                binder.namespaces.len(),
                decl_types.len(),
            ];
            let plan = build_collision_replay_plan(
                replay_trace,
                collision_root_provenance,
                replay_baselines,
                prefix_cardinalities,
                forbidden_work,
            )?;
            #[cfg(any(test, feature = "test-utils"))]
            verify_independent_event_owner_sites(&plan, &independent_event_owner_sites)?;
            (None, Some(std::sync::Arc::new(plan)))
        }
        ReplayIndexPlan::None => {
            let _ = collision_plan_work_scope.finish();
            debug_assert!(replay_trace.is_none());
            (None, None)
        }
    };
    let runtime_state = OwnedLibraryRuntimeState {
        interner,
        binder,
        published_types,
        decl_types,
        semantic_identities: selected_semantic_identities,
        runtime,
        next_type_param,
        next_class_id,
        source_file_count: u32::try_from(canonical.len())
            .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
        library_modules: module_scopes.clone().into(),
        replay_index,
        private_collision_epoch: None,
    };

    let run = InjectedProfileRun {
        phase_counts: LibraryPhaseCounts {
            parse_units: parsed.len(),
            bind_units: module_scopes.len(),
            reserved_records: snapshot.reserved_records,
            filled_records: snapshot.filled_records,
            publication_validations,
            statement_check_units: pass_source_units.len(),
        },
        #[cfg(any(test, feature = "test-utils"))]
        initial_store_rows,
        #[cfg(any(test, feature = "test-utils"))]
        initial_published_type_rows,
        #[cfg(any(test, feature = "test-utils"))]
        initial_replacement_type_rows,
        #[cfg(any(test, feature = "test-utils"))]
        initial_type_param_id,
        #[cfg(any(test, feature = "test-utils"))]
        initial_class_id,
        #[cfg(any(test, feature = "test-utils"))]
        phase_timings: LibraryPhaseTimings {
            parse: parse_elapsed,
            bind: bind_elapsed,
            reserve_fill: reserve_fill_elapsed,
            publication_validation: publication_validation_elapsed,
            statement_check: statement_check_elapsed,
        },
        #[cfg(any(test, feature = "test-utils"))]
        reserved_file_ordinals: snapshot.reserved_file_ordinals,
        #[cfg(any(test, feature = "test-utils"))]
        reporting_receipts,
        library_records: match record_retention {
            LibraryRecordRetention::Drop => Vec::new(),
            LibraryRecordRetention::Collect => library_records,
        },
        user_results,
        #[cfg(any(test, feature = "test-utils"))]
        ordered_user_records: ordered_user_record_evidence,
        #[cfg(any(test, feature = "test-utils"))]
        ordered_user_event_records: ordered_user_event_record_evidence,
        #[cfg(any(test, feature = "test-utils"))]
        user_result_source_ordinals: user_file_ordinals,
        #[cfg(any(test, feature = "test-utils"))]
        pass_source_units,
        #[cfg(any(test, feature = "test-utils"))]
        lexical_source_units,
        #[cfg(any(test, feature = "test-utils"))]
        global_types,
        #[cfg(any(test, feature = "test-utils"))]
        module_types,
        #[cfg(any(test, feature = "test-utils"))]
        global_values,
        #[cfg(any(test, feature = "test-utils"))]
        semantic_identities,
    };
    Ok((run, runtime_state, collision_plan))
}

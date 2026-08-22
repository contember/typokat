use super::*;

/// Whether a compile assembles the legacy full oracle or retains the direct compact plan.
///
/// Full assembly remains a test oracle. Production consumes the live source trace into the compact
/// plan retained beside the frozen base (ADR-0020).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayIndexPlan {
    /// Assemble the index as part of this compile.
    Assemble,
    /// Skip full assembly and consume the live trace into the compact plan.
    Deferred,
    /// Complete combined-source oracle: retain no replay product.
    None,
}

/// Whether a compile hands its own records back, or drops them with the ledger.
///
/// The library's records are not user-facing errors — they are typokat's own model gaps
/// against a library real `tsc` checks clean — so no process retains them and the pinned
/// suite census is their sole witness (ADR-0018). Every compile completes and fingerprints the
/// ledger; the drop route releases each record payload immediately after fingerprinting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryRecordRetention {
    /// Drop the records with the ledger. The production route to a base uses this.
    Drop,
    /// Hand the records back to a caller that asked for them explicitly.
    Collect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CanonicalLibraryEvidenceCapture {
    Skip,
    Capture,
}

/// Compile a profile into its complete runtime product, collision replay index included.
pub fn compile_owned_injected_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<(InjectedProfileRun, OwnedLibraryRuntimeState), InjectedProfileError> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Assemble,
            LibraryRecordRetention::Collect,
            CanonicalLibraryEvidenceCapture::Skip,
            OwnerSiteStorageMode::Flat,
        )
        .map(|(run, runtime, _)| (run, runtime))
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_complete_source_replay_runtime_for_test(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<OwnedLibraryRuntimeState, String> {
    let (_, mut runtime, plan) = compile_owned_injected_base_profile_with_plan(sources)
        .map_err(|error| format!("{error:?}"))?;
    runtime.freeze_as_library_base().map_err(str::to_owned)?;
    let permit = acquire_private_collision_replay_permit().map_err(str::to_owned)?;
    runtime
        .fork_sparse_collision_epoch()
        .map_err(str::to_owned)?
        .install_complete_source_replay_plan_with_permit(plan, &[], permit)
        .map_err(str::to_owned)
}

pub fn compile_complete_source_replay_runtime(
    sources: &[InjectedLibrarySource<'_>],
    seeds: &[PrivateCollisionReplaySeed],
    permit: PrivateCollisionReplayPermitToken,
) -> Result<(OwnedLibraryRuntimeState, LibraryBinderCheckpoint), String> {
    let (_, mut runtime, plan) = compile_owned_injected_base_profile_with_plan(sources)
        .map_err(|error| format!("{error:?}"))?;
    runtime.freeze_as_library_base().map_err(str::to_owned)?;
    let checkpoint = runtime
        .complete_replay_binder_checkpoint()
        .map_err(str::to_owned)?;
    let runtime = runtime
        .fork_sparse_collision_epoch()
        .map_err(str::to_owned)?
        .install_complete_source_replay_plan_with_permit(plan, seeds, permit)
        .map_err(str::to_owned)?;
    Ok((runtime, checkpoint))
}

/// Compile a profile into the runtime state a shared default-library base is sealed from.
///
/// Full-oracle assembly is skipped; the direct compact plan is retained with the base (ADR-0020),
/// and the library's own records are dropped rather than carried into it (ADR-0018).
pub fn compile_owned_injected_base_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<(InjectedProfileRun, OwnedLibraryRuntimeState), InjectedProfileError> {
    compile_owned_injected_base_profile_with_plan(sources).map(|(run, runtime, _)| (run, runtime))
}

pub fn compile_owned_injected_base_profile_with_plan(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        std::sync::Arc<CollisionReplayPlan>,
    ),
    InjectedProfileError,
> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Deferred,
            LibraryRecordRetention::Drop,
            CanonicalLibraryEvidenceCapture::Skip,
            OwnerSiteStorageMode::Flat,
        )
        .and_then(|(run, runtime, plan)| {
            plan.map(|plan| (run, runtime, plan)).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "base source compilation did not retain its collision plan".to_owned(),
                )
            })
        })
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_owned_injected_base_profile_with_ordered_owner_sites_for_test(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        std::sync::Arc<CollisionReplayPlan>,
    ),
    InjectedProfileError,
> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Deferred,
            LibraryRecordRetention::Drop,
            CanonicalLibraryEvidenceCapture::Skip,
            OwnerSiteStorageMode::Ordered,
        )
        .and_then(|(run, runtime, plan)| {
            plan.map(|plan| (run, runtime, plan)).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "base source compilation did not retain its collision plan".to_owned(),
                )
            })
        })
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn compile_owned_injected_base_profile_with_nested_owner_sites_for_test(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<
    (
        InjectedProfileRun,
        OwnedLibraryRuntimeState,
        std::sync::Arc<CollisionReplayPlan>,
    ),
    InjectedProfileError,
> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Deferred,
            LibraryRecordRetention::Drop,
            CanonicalLibraryEvidenceCapture::Skip,
            OwnerSiteStorageMode::Nested,
        )
        .and_then(|(run, runtime, plan)| {
            plan.map(|plan| (run, runtime, plan)).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "base source compilation did not retain its collision plan".to_owned(),
                )
            })
        })
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn force_collision_plan_failure_for_test(
    sources: &[InjectedLibrarySource<'_>],
    failure: ForcedCollisionPlanFailure,
) -> Result<bool, String> {
    if FORCED_COLLISION_PLAN_FAILURE
        .replace(Some(failure))
        .is_some()
    {
        return Err("forced collision-plan failure is already active".to_owned());
    }
    let _scope = ForcedCollisionPlanFailureScope;
    let _event_capture_scope = if failure == ForcedCollisionPlanFailure::EventCaptureCorruption {
        Some(
            super::events_library::EventCaptureCorruptionScope::start()
                .map_err(|message| message.to_owned())?,
        )
    } else {
        None
    };
    match compile_owned_injected_base_profile_with_plan(sources) {
        Ok(_) => Ok(false),
        Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::TypedReferenceCoverage { .. },
        )) if matches!(
            failure,
            ForcedCollisionPlanFailure::UnownedTypedReference
                | ForcedCollisionPlanFailure::RawSemanticAccess
        ) =>
        {
            Ok(true)
        }
        Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::BaselinePartitionMismatch,
        )) if failure == ForcedCollisionPlanFailure::ForbiddenProjection => Ok(true),
        Err(InjectedProfileError::ReplayIndex(
            ReplayIndexGenerationError::IndependentOwnerSiteOracleMismatch,
        )) if failure == ForcedCollisionPlanFailure::EventCaptureCorruption => Ok(true),
        Err(InjectedProfileError::Reporting(LibraryEventLedgerError::TraceDomainSealed))
            if failure == ForcedCollisionPlanFailure::LateOwnerReservation =>
        {
            Ok(true)
        }
        Err(error) => Err(format!(
            "forced collision-plan failure reached the wrong guard: {error:?}"
        )),
    }
}

/// Compile a profile for its own records alone, then drop the semantic product.
///
/// The deliberate record-inspection route costs a full source compilation and answers only to a
/// caller that explicitly requested retained records (ADR-0018).
pub fn compile_owned_injected_records(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<Vec<(LibraryEventKey, CheckerRecord)>, InjectedProfileError> {
    with_canonical_library_frontend(sources, |frontend| {
        compile_owned_injected_frontend(
            frontend,
            ReplayIndexPlan::Deferred,
            LibraryRecordRetention::Collect,
            CanonicalLibraryEvidenceCapture::Skip,
            OwnerSiteStorageMode::Flat,
        )
    })
    .map(|(run, _, _)| run.library_records)
}

pub(super) fn intern_compiled_global_object<Ticket: Copy + PartialEq>(
    pass: &mut super::Pass<'_, '_, Ticket>,
    projection: &LibraryRootProjection,
    additional_contributors: &[(String, ValueStorageId)],
) -> TypeId {
    let mut contributors = projection
        .root_rows
        .iter()
        .filter(|row| {
            projection
                .candidates
                .get(&row.name)
                .is_some_and(|candidate| candidate.global_object_contributor)
        })
        .filter_map(|row| row.value.map(|storage| (row.name.clone(), storage)))
        .collect::<BTreeMap<_, _>>();
    contributors.extend(additional_contributors.iter().cloned());
    if pass.combined_source_library_value_precedence {
        contributors.extend(
            pass.private_collision_value_winners_by_name
                .iter()
                .map(|(name, storage)| (name.clone(), *storage)),
        );
    }
    let mut properties = contributors
        .into_iter()
        .filter(|(name, _)| name != "undefined")
        .filter_map(|(name, storage)| {
            pass.decl_type_replay(storage)
                .map(|ty| PropertyType::public(name, ty))
        })
        .collect::<Vec<_>>();
    properties.push(PropertyType::public(
        "undefined",
        pass.interner.well_known().undefined,
    ));
    pass.interner.intern_object(ObjectType {
        properties,
        ..Default::default()
    })
}

pub(super) trait InjectedCompileRoute {
    type Ticket: Copy + Ord + super::UserReportingOwner<Error = &'static str>;

    fn lexical_events(
        combined: Option<LexicalReservations<PrivateCombinedRecordTicket>>,
    ) -> Result<LexicalReservations<Self::Ticket>, InjectedProfileError>;

    fn reserve_library_program(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
        manifest: &SourceDeclarationManifest<'_>,
        ledger: &mut LibraryEventLedger,
    ) -> Result<(), LibraryEventLedgerError>;

    #[allow(clippy::too_many_arguments)]
    fn attach_library_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        binder: &Binder,
        scope: ScopeId,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
        inventory: &super::decls::inventory::BoundTypeDeclarationInventory<'_>,
        receipt: TypeDeclReservationReceipt,
    ) -> Result<(), InjectedProfileError>;

    #[allow(clippy::too_many_arguments)]
    fn attach_user_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        module_ordinal: ModuleOrdinal,
        binder: &Binder,
        scope: ScopeId,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
        inventory: &super::decls::inventory::BoundTypeDeclarationInventory<'_>,
        receipt: TypeDeclReservationReceipt,
    ) -> Result<(), InjectedProfileError>;

    fn pending_tickets(reservations: &LexicalReservations<Self::Ticket>) -> Vec<Self::Ticket>;

    fn ticket_key() -> fn(Self::Ticket) -> (usize, usize);

    fn user_ticket(owner: UserRecordTicket) -> Result<Self::Ticket, InjectedProfileError>;

    fn complete_batches(
        batches: Vec<CheckerRecordBatch<Self::Ticket>>,
        ledger: &mut LibraryEventLedger,
        user_events: &mut EventStore,
    ) -> Result<(), InjectedProfileError>;
}

pub(super) struct LegacyLibraryRoute;

impl InjectedCompileRoute for LegacyLibraryRoute {
    type Ticket = LibraryRecordTicket;

    fn lexical_events(
        combined: Option<LexicalReservations<PrivateCombinedRecordTicket>>,
    ) -> Result<LexicalReservations<Self::Ticket>, InjectedProfileError> {
        if combined.is_some() {
            return Err(InjectedProfileError::CanonicalProjection(
                "library-only compiler received user lexical reservations".to_owned(),
            ));
        }
        Ok(LexicalReservations::default())
    }

    fn reserve_library_program(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
        manifest: &SourceDeclarationManifest<'_>,
        ledger: &mut LibraryEventLedger,
    ) -> Result<(), LibraryEventLedgerError> {
        reservations.reserve_library_program_with_declaration_manifest(
            file_ordinal,
            program,
            manifest,
            ledger,
        )
    }

    fn attach_library_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        binder: &Binder,
        scope: ScopeId,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
        inventory: &super::decls::inventory::BoundTypeDeclarationInventory<'_>,
        receipt: TypeDeclReservationReceipt,
    ) -> Result<(), InjectedProfileError> {
        reservations.attach_library_declaration_owners(file_ordinal, binder, scope, spans);
        attach_class_bindings_from_inventory(
            reservations,
            SourceOrdinal::Library(file_ordinal),
            binder,
            inventory,
            declarations,
            receipt,
            None,
        )
        .map_err(|error| InjectedProfileError::Binder(error.to_owned()))
    }

    fn attach_user_owners(
        _: &mut LexicalReservations<Self::Ticket>,
        _: ModuleOrdinal,
        _: &Binder,
        _: ScopeId,
        _: &super::ModuleDeclarationSpans,
        _: &super::context::TypeDeclTable<'_>,
        _: &super::decls::inventory::BoundTypeDeclarationInventory<'_>,
        _: TypeDeclReservationReceipt,
    ) -> Result<(), InjectedProfileError> {
        Err(InjectedProfileError::CanonicalProjection(
            "library-only compiler received a user source".to_owned(),
        ))
    }

    fn pending_tickets(reservations: &LexicalReservations<Self::Ticket>) -> Vec<Self::Ticket> {
        reservations.library_semantic_tickets()
    }

    fn ticket_key() -> fn(Self::Ticket) -> (usize, usize) {
        library_record_ticket_key
    }

    fn user_ticket(_: UserRecordTicket) -> Result<Self::Ticket, InjectedProfileError> {
        Err(InjectedProfileError::CanonicalProjection(
            "library-only compiler received user effects".to_owned(),
        ))
    }

    fn complete_batches(
        batches: Vec<CheckerRecordBatch<Self::Ticket>>,
        ledger: &mut LibraryEventLedger,
        _: &mut EventStore,
    ) -> Result<(), InjectedProfileError> {
        LibrarySemanticReportingAdapter::new(ledger)
            .complete_semantic_batches(batches)
            .map_err(InjectedProfileError::Reporting)
    }
}

struct CompleteSourceRoute;

impl InjectedCompileRoute for CompleteSourceRoute {
    type Ticket = PrivateCombinedRecordTicket;

    fn lexical_events(
        combined: Option<LexicalReservations<PrivateCombinedRecordTicket>>,
    ) -> Result<LexicalReservations<Self::Ticket>, InjectedProfileError> {
        combined.ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "complete-source compiler lost user lexical reservations".to_owned(),
            )
        })
    }

    fn reserve_library_program(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
        manifest: &SourceDeclarationManifest<'_>,
        ledger: &mut LibraryEventLedger,
    ) -> Result<(), LibraryEventLedgerError> {
        reservations.reserve_complete_library_program_with_declaration_manifest(
            file_ordinal,
            program,
            manifest,
            ledger,
        )
    }

    fn attach_library_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        file_ordinal: LibraryFileOrdinal,
        binder: &Binder,
        scope: ScopeId,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
        inventory: &super::decls::inventory::BoundTypeDeclarationInventory<'_>,
        receipt: TypeDeclReservationReceipt,
    ) -> Result<(), InjectedProfileError> {
        reservations.attach_complete_library_declaration_owners(file_ordinal, binder, scope, spans);
        attach_class_bindings_from_inventory(
            reservations,
            SourceOrdinal::Library(file_ordinal),
            binder,
            inventory,
            declarations,
            receipt,
            None,
        )
        .map_err(|error| InjectedProfileError::Binder(error.to_owned()))
    }

    fn attach_user_owners(
        reservations: &mut LexicalReservations<Self::Ticket>,
        module_ordinal: ModuleOrdinal,
        binder: &Binder,
        scope: ScopeId,
        spans: &super::ModuleDeclarationSpans,
        declarations: &super::context::TypeDeclTable<'_>,
        inventory: &super::decls::inventory::BoundTypeDeclarationInventory<'_>,
        receipt: TypeDeclReservationReceipt,
    ) -> Result<(), InjectedProfileError> {
        attach_type_decl_owners(
            reservations,
            SourceOrdinal::User(module_ordinal),
            binder,
            scope,
            spans,
        );
        attach_class_bindings_from_inventory(
            reservations,
            SourceOrdinal::User(module_ordinal),
            binder,
            inventory,
            declarations,
            receipt,
            None,
        )
        .map_err(|error| InjectedProfileError::Binder(error.to_owned()))
    }

    fn pending_tickets(reservations: &LexicalReservations<Self::Ticket>) -> Vec<Self::Ticket> {
        let mut tickets = reservations.source_anchor_tickets();
        tickets.extend(reservations.tickets());
        tickets
    }

    fn ticket_key() -> fn(Self::Ticket) -> (usize, usize) {
        private_combined_record_ticket_key
    }

    fn user_ticket(owner: UserRecordTicket) -> Result<Self::Ticket, InjectedProfileError> {
        Ok(PrivateCombinedRecordTicket::User(owner))
    }

    fn complete_batches(
        batches: Vec<CheckerRecordBatch<Self::Ticket>>,
        ledger: &mut LibraryEventLedger,
        user_events: &mut EventStore,
    ) -> Result<(), InjectedProfileError> {
        for batch in batches {
            let (owner, records) = batch.into_parts();
            match owner {
                PrivateCombinedRecordTicket::Library(owner) => ledger
                    .complete(owner, records)
                    .map_err(InjectedProfileError::Reporting)?,
                PrivateCombinedRecordTicket::DisabledLibrary(_) => {}
                PrivateCombinedRecordTicket::User(owner) => user_events
                    .complete(owner, records)
                    .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?,
            }
        }
        Ok(())
    }
}

pub(super) fn compile_owned_injected_frontend(
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
    if frontend.user_start.is_some() {
        compile_owned_injected_frontend_for_route::<CompleteSourceRoute>(
            frontend,
            replay_index_plan,
            record_retention,
            evidence_capture,
            owner_site_storage_mode,
            #[cfg(any(test, feature = "test-utils"))]
            None,
        )
    } else {
        compile_owned_injected_frontend_for_route::<LegacyLibraryRoute>(
            frontend,
            replay_index_plan,
            record_retention,
            evidence_capture,
            owner_site_storage_mode,
            #[cfg(any(test, feature = "test-utils"))]
            None,
        )
    }
    .and_then(|outcome| match outcome {
        CompileEngineOutcome::Complete(complete) => Ok(*complete),
        #[cfg(any(test, feature = "test-utils"))]
        CompileEngineOutcome::ProjectLocal(_) => Err(InjectedProfileError::CanonicalProjection(
            "ordinary compiler returned a project-local materialization receipt".to_owned(),
        )),
    })
}

pub(super) fn validate_complete_source_user_units(
    canonical_len: usize,
    user_start: Option<usize>,
    user_units: &[(ModuleOrdinal, UnitSlot)],
) -> Result<(), InjectedProfileError> {
    if user_start.is_some_and(|start| canonical_len.saturating_sub(start) != user_units.len()) {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source user identities do not match the parsed suffix".to_owned(),
        ));
    }
    let mut seen = BTreeSet::new();
    if let Some((module_ordinal, _)) = user_units
        .iter()
        .find(|(module_ordinal, _)| !seen.insert(*module_ordinal))
    {
        return Err(InjectedProfileError::CanonicalProjection(format!(
            "complete-source user module ordinal {} is duplicated",
            module_ordinal.index()
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn complete_source_user_file_ordinals_for_test(
    canonical: &[CanonicalInput<'_>],
    user_start: Option<usize>,
    user_units: &[(ModuleOrdinal, UnitSlot)],
) -> Result<BTreeMap<ModuleOrdinal, LibraryFileOrdinal>, InjectedProfileError> {
    let Some(user_start) = user_start else {
        if user_units.is_empty() {
            return Ok(BTreeMap::new());
        }
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source user file ordinal mapping omits the suffix".to_owned(),
        ));
    };
    let suffix = canonical.get(user_start..).ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "complete-source user file ordinal mapping starts outside the canonical inputs"
                .to_owned(),
        )
    })?;
    if suffix.len() != user_units.len() {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source user file ordinal mapping is incomplete".to_owned(),
        ));
    }
    let mut expected_modules = BTreeSet::new();
    for (module_ordinal, _) in user_units {
        if !expected_modules.insert(*module_ordinal) {
            return Err(InjectedProfileError::CanonicalProjection(format!(
                "complete-source user file ordinal mapping duplicates module ordinal {}",
                module_ordinal.index()
            )));
        }
    }
    let mut file_ordinals = BTreeMap::new();
    let mut seen_file_ordinals = BTreeSet::new();
    for input in suffix {
        let CompilationOrigin::User(original_module_ordinal) = input.origin else {
            return Err(InjectedProfileError::CanonicalProjection(
                "complete-source user file ordinal mapping contains a library origin".to_owned(),
            ));
        };
        let module_ordinal = ModuleOrdinal::new(original_module_ordinal.index());
        if !expected_modules.contains(&module_ordinal) {
            return Err(InjectedProfileError::CanonicalProjection(format!(
                "complete-source user file ordinal mapping contains unexpected module ordinal {}",
                module_ordinal.index()
            )));
        }
        if file_ordinals
            .insert(module_ordinal, input.file_ordinal)
            .is_some()
        {
            return Err(InjectedProfileError::CanonicalProjection(format!(
                "complete-source user file ordinal mapping duplicates module ordinal {}",
                module_ordinal.index()
            )));
        }
        if !seen_file_ordinals.insert(input.file_ordinal) {
            return Err(InjectedProfileError::CanonicalProjection(format!(
                "complete-source user file ordinal mapping duplicates file ordinal {}",
                input.file_ordinal.index()
            )));
        }
    }
    if file_ordinals.len() != expected_modules.len() {
        return Err(InjectedProfileError::CanonicalProjection(
            "complete-source user file ordinal mapping omits a module ordinal".to_owned(),
        ));
    }
    Ok(file_ordinals)
}

pub(super) fn assemble_complete_source_user_results(
    user_units: &[(ModuleOrdinal, UnitSlot)],
    mut user_records: BTreeMap<ModuleOrdinal, Vec<CheckerRecord>>,
) -> Result<Vec<super::CheckResult>, InjectedProfileError> {
    let mut results = Vec::with_capacity(user_units.len());
    for (module_ordinal, unit_slot) in user_units {
        let mut diagnostics = Vec::new();
        let mut incomplete = Vec::new();
        for record in user_records.remove(module_ordinal).unwrap_or_default() {
            match record {
                CheckerRecord::Diagnostic(diagnostic) => diagnostics.push(diagnostic),
                CheckerRecord::Incomplete(record) => incomplete.push(record),
            }
        }
        results.push(super::CheckResult {
            module_ordinal: *module_ordinal,
            unit_slot: *unit_slot,
            diagnostics,
            incomplete,
        });
    }
    if let Some(module_ordinal) = user_records.keys().next() {
        return Err(InjectedProfileError::CanonicalProjection(format!(
            "complete-source retained records for unexpected user module ordinal {}",
            module_ordinal.index()
        )));
    }
    Ok(results)
}

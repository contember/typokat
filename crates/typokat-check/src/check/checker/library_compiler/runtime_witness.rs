#[cfg(any(test, feature = "test-utils"))]
use super::*;

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedBaseUserTimings {
    pub parse: Duration,
    pub bind: Duration,
    pub check: Duration,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct OwnedBaseUserRun {
    pub result: super::CheckResult,
    pub timings: OwnedBaseUserTimings,
    pub witness: OwnedBaseContinuationWitness,
    pub final_identity: OwnedBaseFinalIdentityWitness,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct OwnedBaseUserProjectRun {
    pub reports: Vec<crate::check::test_support::FileReport>,
    pub final_identity: OwnedBaseFinalIdentityWitness,
    pub cross_file: OwnedBaseCrossFileWitness,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedBaseCrossFileWitness {
    pub producer_type_group: TypeGroupId,
    pub consumer_type_group: TypeGroupId,
    pub producer_value_storage: ValueStorageId,
    pub consumer_value_storage: ValueStorageId,
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
thread_local! {
    static USER_SOURCE_PARSE_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static USER_SOURCE_BIND_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static USER_SOURCE_CHECK_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    pub(super) static USER_DELTA_FORKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static PRIVATE_REPLAY_PRODUCTION_CONTROL:
        std::cell::RefCell<Option<PrivateReplayProductionControlForTest>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateReplayProductionFaultForTest {
    None,
    InjectPostBindMutationOwnerAbsentFromPlan,
    OmitExpectedBaselineRecordDuringCompletion,
    DisableSealedExpectedBaselineOwnerBeforeCandidateReservation,
    OmitScheduledOwner(ReplayOwner),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default)]
pub struct PrivateReplayProductionTraceForTest {
    pub bind_completed: bool,
    pub sparse_candidate_execution_started: bool,
    pub completion_selection_started: bool,
    pub mutation_ledger_recorded: u64,
    pub containment_validation_started: u64,
    pub plan_owner_intersection_started: u64,
    pub injected_after_bind: bool,
    pub injected_owner: Option<ReplayOwner>,
    pub omitted_expected_baseline: Option<super::replay_index::ReplayBaselineRecord>,
    pub post_bind_mutation_owners: BTreeSet<ReplayOwner>,
    pub plan_owners: BTreeSet<ReplayOwner>,
    pub scheduled_owners: BTreeSet<ReplayOwner>,
    pub schedule_omission_installed: bool,
    pub omitted_scheduled_owner: Option<ReplayOwner>,
    pub completion_or_semantic_query_steps: u64,
    pub fault_injected: u64,
    pub candidate_reservation_started: u64,
    pub candidate_activation_started: u64,
    pub baseline_validation_started: u64,
    pub disabled_baseline_owner: Option<ReplayOwner>,
    pub epoch_library_record_baselines: Vec<super::replay_index::ReplayBaselineRecord>,
    pub candidate_reserved_library_record_owners: BTreeSet<ReplayOwner>,
    pub candidate_activated_library_record_owners: BTreeSet<ReplayOwner>,
    pub failure: Option<PrivateReplayProductionFailureForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateReplayProductionFailureForTest {
    MutationOwnerOutsidePlan(ReplayOwner),
    RequiredSourceNotLoaded(LibraryFileOrdinal),
    BaselineMissing,
    Other,
}

#[cfg(any(test, feature = "test-utils"))]
struct PrivateReplayProductionControlForTest {
    fault: PrivateReplayProductionFaultForTest,
    next_event: u64,
    trace: PrivateReplayProductionTraceForTest,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct PrivateReplayProductionScopeForTest;

#[cfg(any(test, feature = "test-utils"))]
impl PrivateReplayProductionScopeForTest {
    pub fn start(fault: PrivateReplayProductionFaultForTest) -> Result<Self, &'static str> {
        PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow_mut(|active| {
            if active.is_some() {
                return Err("private replay production scopes do not nest");
            }
            *active = Some(PrivateReplayProductionControlForTest {
                fault,
                next_event: 0,
                trace: PrivateReplayProductionTraceForTest::default(),
            });
            Ok(Self)
        })
    }

    pub fn finish(self) -> Result<PrivateReplayProductionTraceForTest, &'static str> {
        PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow_mut(|active| {
            active
                .take()
                .map(|control| control.trace)
                .ok_or("private replay production scope is not active")
        })
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn with_private_replay_production_control_for_test(
    update: impl FnOnce(&mut PrivateReplayProductionControlForTest),
) {
    PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow_mut(|active| {
        if let Some(active) = active.as_mut() {
            update(active);
        }
    });
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn private_replay_fault_for_test(
) -> PrivateReplayProductionFaultForTest {
    PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow(|active| {
        active
            .as_ref()
            .map_or(PrivateReplayProductionFaultForTest::None, |active| {
                active.fault
            })
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn private_replay_production_trace_active_for_test() -> bool {
    PRIVATE_REPLAY_PRODUCTION_CONTROL.with_borrow(|active| active.is_some())
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check::checker) fn record_private_replay_trace_for_test(
    update: impl FnOnce(&mut PrivateReplayProductionTraceForTest, u64),
) {
    with_private_replay_production_control_for_test(|active| {
        active.next_event = active.next_event.saturating_add(1);
        update(&mut active.trace, active.next_event);
    });
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
pub struct UserDeltaForkScopeForTest(u64);

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
impl UserDeltaForkScopeForTest {
    pub fn start() -> Self {
        Self(USER_DELTA_FORKS.get())
    }

    pub fn finish(self) -> u64 {
        USER_DELTA_FORKS.get().saturating_sub(self.0)
    }
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserSourceWorkForTest {
    pub parses: u64,
    pub binds: u64,
    pub checks: u64,
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
fn user_source_work_for_test() -> UserSourceWorkForTest {
    UserSourceWorkForTest {
        parses: USER_SOURCE_PARSE_CALLS.get(),
        binds: USER_SOURCE_BIND_CALLS.get(),
        checks: USER_SOURCE_CHECK_CALLS.get(),
    }
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
pub fn record_user_source_parses_for_test(count: usize) {
    USER_SOURCE_PARSE_CALLS.set(
        USER_SOURCE_PARSE_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
pub fn record_user_source_binds_for_test(count: usize) {
    USER_SOURCE_BIND_CALLS.set(
        USER_SOURCE_BIND_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
pub fn record_user_source_checks_for_test(count: usize) {
    USER_SOURCE_CHECK_CALLS.set(
        USER_SOURCE_CHECK_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
pub struct UserSourceWorkScopeForTest(UserSourceWorkForTest);

#[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
impl UserSourceWorkScopeForTest {
    pub fn start() -> Self {
        Self(user_source_work_for_test())
    }

    pub fn finish(self) -> UserSourceWorkForTest {
        let end = user_source_work_for_test();
        UserSourceWorkForTest {
            parses: end.parses.saturating_sub(self.0.parses),
            binds: end.binds.saturating_sub(self.0.binds),
            checks: end.checks.saturating_sub(self.0.checks),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBaseFinalIdentityWitness {
    pub ends: OwnedBaseFinalIdentityEnds,
    pub actual_ids: OwnedBaseActualIds,
    pub named_alias_types: BTreeMap<String, TypeId>,
    pub local_names: BTreeSet<String>,
    pub references: OwnedBaseReferenceSummary,
    pub reused_base_shape: Option<OwnedBaseReusedShapeWitness>,
    pub base_row_clone_counts: BTreeMap<&'static str, u64>,
    pub local_rows_written: u64,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedBaseActualIds {
    pub types: Vec<usize>,
    pub type_params: Vec<usize>,
    pub classes: Vec<usize>,
    pub scopes: Vec<usize>,
    pub symbols: Vec<usize>,
    pub declarations: Vec<usize>,
    pub type_groups: Vec<usize>,
    pub namespaces: Vec<usize>,
    pub value_storages: Vec<usize>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedBaseReferenceSummary {
    pub base_to_delta: u64,
    pub delta_to_base: u64,
    pub delta_to_delta: u64,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBaseFinalIdentityEnds {
    pub store: usize,
    pub declared_recipes: usize,
    pub type_params: usize,
    pub classes: usize,
    pub scopes: usize,
    pub symbols: usize,
    pub declarations: usize,
    pub type_groups: usize,
    pub namespaces: usize,
    pub value_storages: usize,
    pub source_units: usize,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct OwnedBaseReusedShapeWitness {
    pub type_id: TypeId,
    pub tag: TypeTag,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBaseContinuationWitness {
    pub base_store_len: usize,
    pub final_store_len: usize,
    pub base_type_group_count: usize,
    pub final_type_group_count: usize,
    pub base_decl_count: u32,
    pub final_decl_count: u32,
    pub source_key: u32,
    pub base_max_source_key: u32,
    pub array_group_stable: bool,
    pub document_value_stable: bool,
    pub store_prefix_stable: Option<bool>,
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn store_prefix_digest(
    store: &Store,
    type_len: usize,
    declared_recipe_len: usize,
) -> Result<String, String> {
    let mut bytes = CanonicalBytes::domain(b"typokat-owned-base-store-prefix-v1");
    bytes
        .usize(declared_recipe_len)
        .map_err(|error| format!("{error:?}"))?;
    for index in 0..declared_recipe_len {
        let recipe = crate::types::repr::DeclaredRecipeId(
            u32::try_from(index).map_err(|_| "declared recipe prefix overflow")?,
        );
        encode_declared_recipe_row(&mut bytes, store, recipe)
            .map_err(|error| format!("{error:?}"))?;
    }
    for index in 0..type_len {
        let id = TypeId(u32::try_from(index).map_err(|_| "type id prefix overflow")?);
        encode_store_row(&mut bytes, store, id).map_err(|error| format!("{error:?}"))?;
    }
    Ok(format!("{:x}", Sha256::digest(bytes.finish())))
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) struct FinalIdentityInspection<'a> {
    pub(super) base_store_len: usize,
    pub(super) base_value_storage_len: usize,
    pub(super) base_type_group_len: usize,
    pub(super) base_namespace_len: usize,
    pub(super) binder: &'a Binder,
    pub(super) published: &'a PublishedTypeEnvironment,
    pub(super) interner: &'a Interner,
    pub(super) decl_types: &'a DeclTypes,
    pub(super) next_type_param: u32,
    pub(super) next_class_id: u32,
    pub(super) actual_class_ids: Vec<ClassId>,
    pub(super) references: OwnedBaseReferenceSummary,
    pub(super) base_row_clone_counts: BTreeMap<&'static str, u64>,
    pub(super) local_rows_written: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn common_base_domain_limit(ends: &OwnedBaseFinalIdentityEnds, domain: u8) -> Option<usize> {
    match domain {
        1 => Some(ends.store),
        2 => Some(ends.type_params),
        3 => Some(ends.classes),
        4 => Some(ends.scopes),
        5 => Some(ends.symbols),
        6 => Some(ends.declarations),
        7 => Some(ends.type_groups),
        8 => Some(ends.namespaces),
        9 => Some(ends.value_storages),
        _ => None,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn classify_live_reference_with_limits(
    summary: &mut OwnedBaseReferenceSummary,
    owner_limit: Option<usize>,
    owner: u32,
    target_limit: Option<usize>,
    target: u32,
) {
    let Some(target_limit) = target_limit else {
        return;
    };
    let owner_is_base = owner_limit
        .is_some_and(|owner_limit| usize::try_from(owner).is_ok_and(|owner| owner < owner_limit));
    let target_is_base = usize::try_from(target).is_ok_and(|target| target < target_limit);
    match (owner_is_base, target_is_base) {
        (true, false) => summary.base_to_delta += 1,
        (false, true) => summary.delta_to_base += 1,
        (false, false) => summary.delta_to_delta += 1,
        (true, true) => {}
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn classify_live_reference(
    summary: &mut OwnedBaseReferenceSummary,
    base: &OwnedBaseFinalIdentityEnds,
    owner_domain: u8,
    owner: u32,
    target_domain: u8,
    target: u32,
) {
    classify_live_reference_with_limits(
        summary,
        common_base_domain_limit(base, owner_domain),
        owner,
        common_base_domain_limit(base, target_domain),
        target,
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn classify_interner_live_reference(
    summary: &mut OwnedBaseReferenceSummary,
    base: &OwnedBaseFinalIdentityEnds,
    owner_domain: u8,
    owner: u32,
    target_domain: u8,
    target: u32,
) {
    const DECLARED_RECIPE_DOMAIN: u8 = 10;
    let limit = |domain| {
        (domain == DECLARED_RECIPE_DOMAIN)
            .then_some(base.declared_recipes)
            .or_else(|| common_base_domain_limit(base, domain))
    };
    classify_live_reference_with_limits(
        summary,
        limit(owner_domain),
        owner,
        limit(target_domain),
        target,
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn classify_binder_live_reference(
    summary: &mut OwnedBaseReferenceSummary,
    base: &OwnedBaseFinalIdentityEnds,
    owner_domain: u8,
    owner: u32,
    target_domain: u8,
    target: u32,
) {
    const SOURCE_UNIT_DOMAIN: u8 = 10;
    let limit = |domain| {
        (domain == SOURCE_UNIT_DOMAIN)
            .then_some(base.source_units)
            .or_else(|| common_base_domain_limit(base, domain))
    };
    classify_live_reference_with_limits(
        summary,
        limit(owner_domain),
        owner,
        limit(target_domain),
        target,
    );
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn final_reference_summary<Ticket: Copy + PartialEq>(
    pass: &super::context::Pass<'_, '_, Ticket>,
    base: &OwnedBaseFinalIdentityEnds,
) -> OwnedBaseReferenceSummary {
    let mut summary = OwnedBaseReferenceSummary::default();
    for (owner_domain, target_domain, _, owner, target) in
        pass.interner.local_type_reference_records_for_test()
    {
        classify_interner_live_reference(
            &mut summary,
            base,
            owner_domain,
            owner,
            target_domain,
            target,
        );
    }

    for (owner_domain, owner, target_domain, target) in
        pass.binder.local_reference_records_for_test()
    {
        classify_binder_live_reference(
            &mut summary,
            base,
            owner_domain,
            owner,
            target_domain,
            target,
        );
    }

    for (owner, ty) in pass.decl_types.changed_slots() {
        classify_live_reference(&mut summary, base, 9, owner.0, 9, owner.0);
        if let Some(ty) = ty {
            classify_live_reference(&mut summary, base, 9, owner.0, 1, ty.0);
        }
    }

    let published = pass.type_environment.published();
    for (owner, terminal) in published.changed_group_terminals() {
        classify_live_reference(&mut summary, base, 7, owner.0, 7, owner.0);
        let PublishedTypeGroupTerminal::Ready(group) = terminal else {
            continue;
        };
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) | PublishedTypeGroupSurface::Enum(ty) => {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0)
            }
            PublishedTypeGroupSurface::Class(class) => {
                classify_live_reference(&mut summary, base, 7, owner.0, 3, class.0)
            }
        }
        for parameter in &group.parameters {
            classify_live_reference(&mut summary, base, 7, owner.0, 2, parameter.0);
        }
        for default in &group.parameter_defaults {
            if let PublishedTypeParameterDefault::Ready(ty) = default {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0);
            }
        }
        for alternative in &group.conflict_alternatives {
            for ty in &alternative.types {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0);
            }
        }
    }
    for (class, terminal) in published.changed_class_terminals() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        let OwnedPublishedClassTerminal::Ready(surface) = &terminal else {
            continue;
        };
        classify_live_reference(&mut summary, base, 3, class.0, 3, surface.class().0);
        for parameter in surface.type_params() {
            classify_live_reference(&mut summary, base, 3, class.0, 2, parameter.0);
        }
        for ty in [
            Some(surface.instance_template()),
            Some(surface.static_template()),
            surface.constructor_template(),
        ]
        .into_iter()
        .flatten()
        {
            classify_live_reference(&mut summary, base, 3, class.0, 1, ty.0);
        }
    }

    let namespace_terminals = pass
        .namespace_values
        .changed_terminal_snapshot_parts_for_test()
        .expect("completed changed namespace terminals snapshot");
    for row in namespace_terminals {
        classify_live_reference(&mut summary, base, 8, row.namespace.0, 8, row.namespace.0);
        if let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal {
            classify_live_reference(&mut summary, base, 8, row.namespace.0, 9, storage.0);
            classify_live_reference(&mut summary, base, 8, row.namespace.0, 1, ty.0);
        }
    }

    for (&class, parameters) in pass.class_application_parameters.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        for parameter in parameters {
            let parameter = (*parameter).snapshot_parts();
            classify_live_reference(&mut summary, base, 3, class.0, 2, parameter.id.0);
            if let Some(constraint) = parameter.constraint {
                classify_live_reference(&mut summary, base, 3, class.0, 1, constraint.0);
            }
            if let ClassTypeParameterDefault::Ready(default) = parameter.default {
                classify_live_reference(&mut summary, base, 3, class.0, 1, default.0);
            }
        }
    }
    for (&class, metadata) in pass.class_new_metadata.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        classify_live_reference(
            &mut summary,
            base,
            3,
            class.0,
            3,
            metadata.ctor_declaring_class.0,
        );
    }
    for (&class, &parent) in pass.class_parents.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, parent.0);
    }
    for (&alias, &target) in pass.class_value_aliases.local_iter() {
        classify_live_reference(&mut summary, base, 9, alias.0, 9, target.0);
    }
    for (&storage, binding) in pass.class_value_bindings.local_iter() {
        classify_live_reference(&mut summary, base, 9, storage.0, 3, binding.class_id.0);
    }
    for (&alias, &target) in pass.standalone_namespace_value_aliases.local_iter() {
        classify_live_reference(&mut summary, base, 9, alias.0, 9, target.0);
    }
    for &symbol in pass.named_function_symbols.local_iter() {
        classify_live_reference(&mut summary, base, 5, symbol.0, 5, symbol.0);
    }
    for (&class, _) in pass.class_names.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
    }
    if let Some(identities) = &pass.library_semantic_identities {
        for terminal in identities.terminals() {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                continue;
            };
            for (domain, target) in std::iter::once((7, identity.group.0))
                .chain(std::iter::once((1, identity.template.0)))
                .chain(identity.parameters.iter().map(|parameter| (2, parameter.0)))
            {
                let Some(limit) = common_base_domain_limit(base, domain) else {
                    continue;
                };
                if usize::try_from(target).is_ok_and(|target| target >= limit) {
                    summary.base_to_delta += 1;
                }
            }
        }
    }
    summary
}

#[rustfmt::skip]
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn final_identity_witness(inputs: FinalIdentityInspection<'_>) -> OwnedBaseFinalIdentityWitness {
    let FinalIdentityInspection {
        base_store_len,
        base_value_storage_len,
        base_type_group_len,
        base_namespace_len,
        binder,
        published,
        interner,
        decl_types,
        next_type_param,
        next_class_id,
        actual_class_ids,
        references,
        base_row_clone_counts,
        local_rows_written,
    } = inputs;
    let mut named_alias_types = BTreeMap::new();
    let mut local_names = BTreeSet::new();
    let base_type_group_end =
        u32::try_from(base_type_group_len).expect("base type-group end fits u32");
    let base_namespace_end =
        u32::try_from(base_namespace_len).expect("base namespace end fits u32");
    let mut pending_scopes = vec![(binder.module, String::new())];
    let mut visited_namespaces = BTreeSet::new();
    while let Some((scope_id, prefix)) = pending_scopes.pop() {
        let Some(scope) = binder.graph.get(scope_id) else {
            continue;
        };
        for (name, symbol_id) in &scope.symbols {
            let qualified = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            local_names.insert(qualified.clone());
            let Some(symbol) = binder.symbols.get(*symbol_id) else {
                continue;
            };
            if let Some(group) = symbol.ty.filter(|group| group.0 >= base_type_group_end) {
                let alias = binder.type_groups.get(group).is_some_and(|declaration| {
                    declaration
                        .fragments
                        .iter()
                        .any(|fragment| fragment.kind == TypeFragmentKind::TypeAlias)
                });
                if alias {
                    if let Some(PublishedTypeGroupTerminal::Ready(ready)) =
                        published.groups().get(group)
                    {
                        if let PublishedTypeGroupSurface::Template(ty) = ready.surface {
                            named_alias_types.insert(qualified.clone(), ty);
                        }
                    }
                }
            }
            if let Some(namespace) = symbol
                .ns
                .filter(|namespace| namespace.0 >= base_namespace_end)
                .filter(|namespace| visited_namespaces.insert(*namespace))
            {
                if let Some(public_scope) = binder
                    .namespaces
                    .get(namespace)
                    .map(|namespace| namespace.public_scope)
                {
                    pending_scopes.push((public_scope, qualified));
                }
            }
        }
    }

    let eligible_base_shape = |ty: TypeId| {
        (ty.index() < base_store_len)
            .then(|| interner.store().tag(ty))
            .filter(|tag| {
                matches!(
                    tag,
                    TypeTag::Object | TypeTag::ClassInstance | TypeTag::Function
                )
            })
            .map(|tag| OwnedBaseReusedShapeWitness { type_id: ty, tag })
    };
    let direct_user_shape = named_alias_types
        .values()
        .copied()
        .chain(
            (base_value_storage_len..decl_types.len())
                .map(|index| ValueStorageId(u32::try_from(index).expect("storage id fits u32")))
                .filter_map(|storage| decl_types.get(storage)),
        )
        .find_map(eligible_base_shape);
    let reused_base_shape = direct_user_shape.or_else(|| {
        const TYPE_DOMAIN: u8 = 1;
        interner
            .local_type_reference_records_for_test()
            .into_iter()
            .find_map(|(owner_domain, target_domain, _, owner, target)| {
                (owner_domain == TYPE_DOMAIN
                    && target_domain == TYPE_DOMAIN
                    && usize::try_from(owner).ok()? >= base_store_len
                    && usize::try_from(target).ok()? < base_store_len)
                    .then_some(TypeId(target))
                    .and_then(eligible_base_shape)
            })
    });

    OwnedBaseFinalIdentityWitness {
        ends: OwnedBaseFinalIdentityEnds {
            store: interner.store().len(),
            declared_recipes: interner.store().all_declared_recipes().count(),
            type_params: usize::try_from(next_type_param).expect("type parameter end fits usize"),
            classes: usize::try_from(next_class_id).expect("class end fits usize"),
            scopes: binder.graph.len(),
            symbols: binder.symbols.len(),
            declarations: binder.declarations.len(),
            type_groups: binder.type_groups.len(),
            namespaces: binder.namespaces.len(),
            value_storages: decl_types.len(),
            source_units: binder.checkpoint_ends().next_source,
        },
        actual_ids: OwnedBaseActualIds {
            types: interner
                .store()
                .local_type_ids_for_test()
                .map(TypeId::index)
                .collect(),
            type_params: interner
                .store()
                .local_type_param_ids_for_test()
                .map(|parameter| {
                    usize::try_from(parameter.0).expect("type parameter id fits usize")
                })
                .collect(),
            classes: actual_class_ids
                .into_iter()
                .map(|class| usize::try_from(class.0).expect("class id fits usize"))
                .collect(),
            scopes: binder
                .graph
                .local_scopes()
                .map(|(scope, _)| usize::try_from(scope.0).expect("scope id fits usize"))
                .collect(),
            symbols: binder
                .symbols
                .local_symbols()
                .map(|(symbol, _)| usize::try_from(symbol.0).expect("symbol id fits usize"))
                .collect(),
            declarations: binder
                .declarations
                .local_declarations()
                .map(|declaration| {
                    usize::try_from(declaration.id.0).expect("declaration id fits usize")
                })
                .collect(),
            type_groups: binder
                .type_groups
                .local_groups()
                .map(|group| usize::try_from(group.id.0).expect("type group id fits usize"))
                .collect(),
            namespaces: binder
                .namespaces
                .local_namespaces()
                .map(|(namespace, _)| {
                    usize::try_from(namespace.0).expect("namespace id fits usize")
                })
                .collect(),
            value_storages: decl_types
                .local_slots()
                .map(|(storage, _)| {
                    usize::try_from(storage.0).expect("value storage id fits usize")
                })
                .collect(),
        },
        named_alias_types,
        local_names,
        references,
        reused_base_shape,
        base_row_clone_counts,
        local_rows_written,
    }
}

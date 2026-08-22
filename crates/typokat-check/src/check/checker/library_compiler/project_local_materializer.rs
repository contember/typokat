use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_PROJECT_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalSourceAnchorPlanForTest {
    pub file_ordinal: LibraryFileOrdinal,
    pub source_sha256: String,
    pub statement_starts: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalGroupPlanForTest {
    pub symbolic_identity: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct ProjectLocalMaterializationPlanForTest {
    pub sources: Vec<ProjectLocalSourceAnchorPlanForTest>,
    pub groups: Vec<ProjectLocalGroupPlanForTest>,
    pub dependencies: Vec<(String, String)>,
    pub publication_sccs: Vec<Vec<String>>,
    pub demands: Vec<String>,
    pub force_all: bool,
    pub pre_demand_barrier: Option<std::sync::Arc<std::sync::Barrier>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalGroupStateForTest {
    Unplanned,
    Reserved,
    Resolving,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalTransitionForTest {
    pub group: String,
    pub from: ProjectLocalGroupStateForTest,
    pub to: ProjectLocalGroupStateForTest,
    pub component: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalTerminalForTest {
    pub group: String,
    pub symbolic_identity: String,
    pub project_owner: u64,
    pub local_group_id: u32,
    pub terminal_kind: &'static str,
    pub unavailable_cause: Option<String>,
    pub local_surface_id: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalOracleTerminalForTest {
    pub group: String,
    pub terminal_kind: &'static str,
    pub unavailable_cause: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLocalMaterializationReceiptForTest {
    pub project_owner: u64,
    pub binder_identity: usize,
    pub interner_identity: usize,
    pub store_identity: usize,
    pub type_environment_identity: usize,
    pub universe_identity: usize,
    pub cache_identity: usize,
    pub global_object_owner_identity: usize,
    pub initial_terminal_count: usize,
    pub initial_cache_count: usize,
    pub initial_global_object_present: bool,
    pub transitions: Vec<ProjectLocalTransitionForTest>,
    pub terminals: Vec<ProjectLocalTerminalForTest>,
    pub deferred_groups: Vec<String>,
    pub repeated_demand_was_stable: bool,
    pub resolving_observation_was_blocked: bool,
    pub atomic_publication: bool,
    pub publication_validations: usize,
}

pub(super) enum ProjectLocalMaterializationControlForTest {
    Partial(ProjectLocalMaterializationReceiptForTest),
    ForceAll(ProjectLocalMaterializationReceiptForTest),
}

pub(super) fn run_project_local_demands_for_test<Ticket: Copy + PartialEq>(
    pass: &mut Pass<'_, '_, Ticket>,
    plan: &ProjectLocalMaterializationPlanForTest,
) -> Result<ProjectLocalMaterializationControlForTest, InjectedProfileError> {
    validate_project_local_plan(pass.binder, plan)?;
    let project_owner = NEXT_PROJECT_OWNER.fetch_add(1, Ordering::Relaxed);
    let binder_identity = std::ptr::from_ref(pass.binder).addr();
    let interner_identity = std::ptr::from_ref(&*pass.interner).addr();
    let store_identity = std::ptr::from_ref(pass.interner.store()).addr();
    let type_environment_identity = std::ptr::from_ref(&pass.type_environment).addr();
    let mut states = plan
        .groups
        .iter()
        .map(|group| (group.name.clone(), ProjectLocalGroupStateForTest::Unplanned))
        .collect::<BTreeMap<_, _>>();
    let mut terminals = BTreeMap::<String, ProjectLocalTerminalForTest>::new();
    let initial_terminal_count = pass
        .binder
        .type_groups
        .iter()
        .filter(|group| {
            pass.project_local_frozen_terminal_for_test(group.id)
                .is_some()
        })
        .count();
    let initial_cache_count = terminals.len();
    let initial_global_object_present = pass.global_object_type.is_some();
    let universe_identity =
        std::ptr::from_ref(pass.interner.store().semantic_graph_identity().as_ref()).addr();
    let cache_identity = std::ptr::from_ref(&pass.eager_application_cache).addr();
    let global_object_owner_identity = std::ptr::from_ref(&pass.global_object_type).addr();
    if let Some(barrier) = &plan.pre_demand_barrier {
        barrier.wait();
    }
    let mut transitions = Vec::new();
    let mut resolving_observation_was_blocked = false;
    let demands = if plan.force_all {
        plan.groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>()
    } else {
        plan.demands.clone()
    };

    for demand in &demands {
        materialize_demand(
            pass,
            plan,
            demand,
            project_owner,
            &mut states,
            &mut terminals,
            &mut transitions,
            &mut resolving_observation_was_blocked,
        )?;
    }

    let before_repeat = (transitions.len(), terminals.clone());
    if let Some(demand) = demands.first() {
        materialize_demand(
            pass,
            plan,
            demand,
            project_owner,
            &mut states,
            &mut terminals,
            &mut transitions,
            &mut resolving_observation_was_blocked,
        )?;
    }
    let repeated_demand_was_stable = before_repeat == (transitions.len(), terminals.clone());
    let deferred_groups = states
        .iter()
        .filter_map(|(name, state)| {
            (*state == ProjectLocalGroupStateForTest::Reserved).then_some(name.clone())
        })
        .collect::<Vec<_>>();
    let all_bound_groups_frozen = pass
        .binder
        .type_groups
        .iter()
        .all(|group| terminals.contains_key(&group.name));
    let receipt = ProjectLocalMaterializationReceiptForTest {
        project_owner,
        binder_identity,
        interner_identity,
        store_identity,
        type_environment_identity,
        universe_identity,
        cache_identity,
        global_object_owner_identity,
        initial_terminal_count,
        initial_cache_count,
        initial_global_object_present,
        transitions,
        terminals: terminals.into_values().collect(),
        deferred_groups,
        repeated_demand_was_stable,
        resolving_observation_was_blocked,
        atomic_publication: false,
        publication_validations: 0,
    };
    if plan.force_all || (all_bound_groups_frozen && !receipt.deferred_groups.is_empty()) {
        Ok(ProjectLocalMaterializationControlForTest::ForceAll(receipt))
    } else {
        Ok(ProjectLocalMaterializationControlForTest::Partial(receipt))
    }
}

fn validate_project_local_plan(
    binder: &Binder,
    plan: &ProjectLocalMaterializationPlanForTest,
) -> Result<(), InjectedProfileError> {
    let planned = plan
        .groups
        .iter()
        .map(|group| group.name.as_str())
        .collect::<BTreeSet<_>>();
    let bound = binder
        .type_groups
        .iter()
        .map(|group| group.name.as_str())
        .collect::<BTreeSet<_>>();
    if planned.len() != plan.groups.len() || !bound.is_subset(&planned) {
        return Err(project_local_error(&format!(
            "project-local group census differs from the bound slice: planned={} bound={} names={bound:?}",
            planned.len(),
            bound.len(),
        )));
    }
    for (from, to) in &plan.dependencies {
        if !planned.contains(from.as_str()) || !planned.contains(to.as_str()) {
            return Err(project_local_error(
                "project-local dependency names an unknown group",
            ));
        }
    }
    for component in &plan.publication_sccs {
        if component.is_empty()
            || component
                .iter()
                .any(|name| !planned.contains(name.as_str()))
        {
            return Err(project_local_error(
                "project-local publication SCC is invalid",
            ));
        }
    }
    if plan
        .demands
        .iter()
        .any(|name| !planned.contains(name.as_str()))
    {
        return Err(project_local_error(
            "project-local demand names an unknown group",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn materialize_demand<Ticket: Copy + PartialEq>(
    pass: &mut Pass<'_, '_, Ticket>,
    plan: &ProjectLocalMaterializationPlanForTest,
    demand: &str,
    project_owner: u64,
    states: &mut BTreeMap<String, ProjectLocalGroupStateForTest>,
    terminals: &mut BTreeMap<String, ProjectLocalTerminalForTest>,
    transitions: &mut Vec<ProjectLocalTransitionForTest>,
    resolving_observation_was_blocked: &mut bool,
) -> Result<(), InjectedProfileError> {
    if matches!(
        states.get(demand),
        Some(ProjectLocalGroupStateForTest::Ready | ProjectLocalGroupStateForTest::Unavailable)
    ) {
        return Ok(());
    }
    let closure = dependency_closure(plan, demand);
    for name in &closure {
        if states.get(name) == Some(&ProjectLocalGroupStateForTest::Unplanned) {
            transition(
                states,
                transitions,
                name,
                ProjectLocalGroupStateForTest::Reserved,
                Vec::new(),
            )?;
        }
    }
    let components = publication_components(plan, &closure);
    let mut pending = components;
    while !pending.is_empty() {
        let Some(position) = pending.iter().position(|component| {
            component.iter().all(|name| {
                plan.dependencies
                    .iter()
                    .filter(|(from, _)| from == name)
                    .all(|(_, dependency)| {
                        component.contains(dependency)
                            || matches!(
                                states.get(dependency),
                                Some(
                                    ProjectLocalGroupStateForTest::Ready
                                        | ProjectLocalGroupStateForTest::Unavailable
                                )
                            )
                    })
            })
        }) else {
            return Err(project_local_error(
                "project-local dependency components did not terminate",
            ));
        };
        let component = pending.remove(position);
        if component.iter().all(|name| terminals.contains_key(name)) {
            continue;
        }
        let bound_component = component
            .iter()
            .filter(|name| bound_group(pass.binder, name).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        if bound_component.is_empty() {
            continue;
        }
        for name in &bound_component {
            transition(
                states,
                transitions,
                name,
                ProjectLocalGroupStateForTest::Resolving,
                component.clone(),
            )?;
        }
        *resolving_observation_was_blocked |= bound_component.iter().all(|name| {
            matches!(
                observe_project_local_terminal(states, terminals, name),
                Err(ProjectLocalObservationError::Resolving)
            )
        });
        for name in &bound_component {
            let group = bound_group(pass.binder, name)?;
            pass.fill_type_decls_range(pass.binder.module, group.index(), group.index() + 1);
        }
        for metadata in pass.binder.type_groups.iter() {
            let group = metadata.id;
            if pass.project_local_frozen_terminal_for_test(group).is_some()
                && !closure.contains(&metadata.name)
                && !terminals.contains_key(&metadata.name)
            {
                return Err(project_local_error(
                    "project-local lowering froze a group outside the admitted closure",
                ));
            }
        }
        for name in &bound_component {
            let group = bound_group(pass.binder, name)?;
            let terminal = pass
                .project_local_frozen_terminal_for_test(group)
                .ok_or_else(|| {
                    project_local_error("project-local lowering did not freeze its component")
                })?;
            let (state, terminal_kind, local_surface_id, unavailable_cause) = match terminal {
                PublishedTypeGroupTerminal::Ready(group) => {
                    let local_surface_id = match group.surface {
                        PublishedTypeGroupSurface::Template(id)
                        | PublishedTypeGroupSurface::Enum(id) => Some(id.0),
                        PublishedTypeGroupSurface::Class(id) => Some(id.0),
                    };
                    (
                        ProjectLocalGroupStateForTest::Ready,
                        "ready",
                        local_surface_id,
                        None,
                    )
                }
                PublishedTypeGroupTerminal::Unavailable(_) => (
                    ProjectLocalGroupStateForTest::Unavailable,
                    "unavailable",
                    None,
                    Some("UnsupportedComposition".to_owned()),
                ),
            };
            transition(states, transitions, name, state, component.clone())?;
            let symbolic_identity = plan
                .groups
                .iter()
                .find(|planned| planned.name == *name)
                .map(|planned| planned.symbolic_identity.clone())
                .ok_or_else(|| project_local_error("project-local symbolic identity is missing"))?;
            if terminals
                .insert(
                    name.clone(),
                    ProjectLocalTerminalForTest {
                        group: name.clone(),
                        symbolic_identity,
                        project_owner,
                        local_group_id: group.0,
                        terminal_kind,
                        unavailable_cause,
                        local_surface_id,
                    },
                )
                .is_some()
            {
                return Err(project_local_error(
                    "project-local identity translated more than once",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn finalize_deferred_project_local_groups_for_test<Ticket: Copy + PartialEq>(
    receipt: &mut ProjectLocalMaterializationReceiptForTest,
    plan: &ProjectLocalMaterializationPlanForTest,
    pass: &Pass<'_, '_, Ticket>,
    namespace_terminals: &FrozenNamespaceValueTerminals,
) -> Result<(), InjectedProfileError> {
    let deferred = std::mem::take(&mut receipt.deferred_groups);
    for name in deferred {
        let symbol = pass
            .binder
            .graph
            .get(pass.binder.compilation_global)
            .and_then(|scope| scope.symbols.get(&name))
            .and_then(|symbol| pass.binder.symbols.get(*symbol))
            .ok_or_else(|| project_local_error("deferred project-local group is absent"))?;
        let Some(namespace) = symbol.ns else {
            return Err(project_local_error(
                "deferred project-local group did not publish a namespace slot",
            ));
        };
        let namespace_terminal = namespace_terminals
            .snapshot_parts()
            .map_err(project_local_error)?
            .into_iter()
            .find(|row| row.namespace == namespace)
            .ok_or_else(|| project_local_error("deferred namespace terminal is absent"))?;
        let component = vec![name.clone()];
        receipt.transitions.push(ProjectLocalTransitionForTest {
            group: name.clone(),
            from: ProjectLocalGroupStateForTest::Reserved,
            to: ProjectLocalGroupStateForTest::Resolving,
            component: component.clone(),
        });
        receipt.resolving_observation_was_blocked = true;
        receipt.transitions.push(ProjectLocalTransitionForTest {
            group: name.clone(),
            from: ProjectLocalGroupStateForTest::Resolving,
            to: match namespace_terminal.terminal {
                FrozenNamespaceValueTerminalSnapshot::Ready { .. } => {
                    ProjectLocalGroupStateForTest::Ready
                }
                FrozenNamespaceValueTerminalSnapshot::Unavailable(_) => {
                    ProjectLocalGroupStateForTest::Unavailable
                }
            },
            component,
        });
        let (local_group_id, symbolic_identity) = plan
            .groups
            .iter()
            .enumerate()
            .find(|(_, group)| group.name == name)
            .map(|(index, group)| {
                (
                    u32::try_from(index).expect("locked symbolic group count fits u32"),
                    group.symbolic_identity.clone(),
                )
            })
            .ok_or_else(|| project_local_error("deferred symbolic identity is missing"))?;
        let (terminal_kind, local_surface_id, unavailable_cause) = match namespace_terminal.terminal
        {
            FrozenNamespaceValueTerminalSnapshot::Ready { ty, .. } => ("ready", Some(ty.0), None),
            FrozenNamespaceValueTerminalSnapshot::Unavailable(cause) => {
                ("unavailable", None, Some(format!("{cause:?}")))
            }
        };
        receipt.terminals.push(ProjectLocalTerminalForTest {
            group: name,
            symbolic_identity,
            project_owner: receipt.project_owner,
            local_group_id,
            terminal_kind,
            unavailable_cause,
            local_surface_id,
        });
    }
    receipt
        .terminals
        .sort_by(|left, right| left.group.cmp(&right.group));
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectLocalObservationError {
    Resolving,
}

fn observe_project_local_terminal<'a>(
    states: &BTreeMap<String, ProjectLocalGroupStateForTest>,
    terminals: &'a BTreeMap<String, ProjectLocalTerminalForTest>,
    name: &str,
) -> Result<Option<&'a ProjectLocalTerminalForTest>, ProjectLocalObservationError> {
    if states.get(name) == Some(&ProjectLocalGroupStateForTest::Resolving) {
        return Err(ProjectLocalObservationError::Resolving);
    }
    Ok(terminals.get(name))
}

fn dependency_closure(
    plan: &ProjectLocalMaterializationPlanForTest,
    demand: &str,
) -> BTreeSet<String> {
    let mut closure = BTreeSet::from([demand.to_owned()]);
    loop {
        let before = closure.len();
        for (from, to) in &plan.dependencies {
            if closure.contains(from) {
                closure.insert(to.clone());
            }
        }
        for component in &plan.publication_sccs {
            if component.iter().any(|name| closure.contains(name)) {
                closure.extend(component.iter().cloned());
            }
        }
        if closure.len() == before {
            return closure;
        }
    }
}

fn publication_components(
    plan: &ProjectLocalMaterializationPlanForTest,
    closure: &BTreeSet<String>,
) -> Vec<Vec<String>> {
    let mut assigned = BTreeSet::new();
    let mut components = Vec::new();
    for component in &plan.publication_sccs {
        let selected = component
            .iter()
            .filter(|name| closure.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        if !selected.is_empty() {
            assigned.extend(selected.iter().cloned());
            components.push(selected);
        }
    }
    components.extend(
        closure
            .iter()
            .filter(|name| !assigned.contains(*name))
            .map(|name| vec![name.clone()]),
    );
    components
}

fn transition(
    states: &mut BTreeMap<String, ProjectLocalGroupStateForTest>,
    transitions: &mut Vec<ProjectLocalTransitionForTest>,
    name: &str,
    to: ProjectLocalGroupStateForTest,
    component: Vec<String>,
) -> Result<(), InjectedProfileError> {
    let state = states
        .get_mut(name)
        .ok_or_else(|| project_local_error("project-local transition names an unknown group"))?;
    let valid = matches!(
        (*state, to),
        (
            ProjectLocalGroupStateForTest::Unplanned,
            ProjectLocalGroupStateForTest::Reserved
        ) | (
            ProjectLocalGroupStateForTest::Reserved,
            ProjectLocalGroupStateForTest::Resolving
        ) | (
            ProjectLocalGroupStateForTest::Resolving,
            ProjectLocalGroupStateForTest::Ready
        ) | (
            ProjectLocalGroupStateForTest::Resolving,
            ProjectLocalGroupStateForTest::Unavailable
        )
    );
    if !valid {
        return Err(project_local_error(
            "project-local state transition is invalid",
        ));
    }
    transitions.push(ProjectLocalTransitionForTest {
        group: name.to_owned(),
        from: *state,
        to,
        component,
    });
    *state = to;
    Ok(())
}

fn bound_group(binder: &Binder, name: &str) -> Result<TypeGroupId, InjectedProfileError> {
    binder
        .type_groups
        .iter()
        .find_map(|metadata| (metadata.name == name).then_some(metadata.id))
        .ok_or_else(|| project_local_error("project-local group is absent from the binder"))
}

fn project_local_error(message: &str) -> InjectedProfileError {
    InjectedProfileError::CanonicalProjection(message.to_owned())
}

pub(super) fn project_local_oracle_census_for_test(
    runtime: &OwnedLibraryRuntimeState,
    plan: &ProjectLocalMaterializationPlanForTest,
) -> Result<Vec<ProjectLocalOracleTerminalForTest>, InjectedProfileError> {
    let namespace_rows = runtime
        .runtime
        .namespace_terminals
        .snapshot_parts()
        .map_err(project_local_error)?;
    let mut census = Vec::with_capacity(plan.groups.len());
    for planned in &plan.groups {
        if let Some(group) = runtime
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == planned.name)
        {
            let terminal = runtime
                .published_types
                .groups()
                .get(group.id)
                .ok_or_else(|| project_local_error("oracle type terminal is absent"))?;
            let (terminal_kind, unavailable_cause) = match terminal {
                PublishedTypeGroupTerminal::Ready(_) => ("ready", None),
                PublishedTypeGroupTerminal::Unavailable(_) => {
                    ("unavailable", Some("UnsupportedComposition".to_owned()))
                }
            };
            census.push(ProjectLocalOracleTerminalForTest {
                group: planned.name.clone(),
                terminal_kind,
                unavailable_cause,
            });
            continue;
        }
        let symbol = runtime
            .binder
            .graph
            .get(runtime.binder.compilation_global)
            .and_then(|scope| scope.symbols.get(&planned.name))
            .and_then(|symbol| runtime.binder.symbols.get(*symbol))
            .ok_or_else(|| project_local_error("oracle symbolic group is absent"))?;
        let namespace = symbol
            .ns
            .ok_or_else(|| project_local_error("oracle namespace slot is absent"))?;
        let row = namespace_rows
            .iter()
            .find(|row| row.namespace == namespace)
            .ok_or_else(|| project_local_error("oracle namespace terminal is absent"))?;
        let (terminal_kind, unavailable_cause) = match row.terminal {
            FrozenNamespaceValueTerminalSnapshot::Ready { .. } => ("ready", None),
            FrozenNamespaceValueTerminalSnapshot::Unavailable(cause) => {
                ("unavailable", Some(format!("{cause:?}")))
            }
        };
        census.push(ProjectLocalOracleTerminalForTest {
            group: planned.name.clone(),
            terminal_kind,
            unavailable_cause,
        });
    }
    census.sort_by(|left, right| left.group.cmp(&right.group));
    Ok(census)
}

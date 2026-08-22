use super::project_local_symbolic_index::ProjectLocalSymbolicIndex;
use crate::check::checker::library_compiler::{
    compile_project_local_force_all_oracle_for_test, materialize_project_local_profile_for_test,
    InjectedLibrarySource, ProjectLocalGroupStateForTest, ProjectLocalMaterializationPlanForTest,
    ProjectLocalMaterializationReceiptForTest,
};
use crate::profile::ExactLibraryProfile;
use std::sync::Arc;

fn materialize(
    index: &ProjectLocalSymbolicIndex,
    demands: &[&str],
    force_all: bool,
) -> Result<ProjectLocalMaterializationReceiptForTest, String> {
    let plan = index.materialization_plan(
        demands.iter().map(|demand| (*demand).to_owned()).collect(),
        force_all,
    );
    materialize_with_plan(index, &plan)
}

fn materialize_with_plan(
    index: &ProjectLocalSymbolicIndex,
    plan: &ProjectLocalMaterializationPlanForTest,
) -> Result<ProjectLocalMaterializationReceiptForTest, String> {
    let profile = ExactLibraryProfile::load_packaged().map_err(|error| error.to_string())?;
    let selected = index
        .sources
        .iter()
        .map(|source| {
            profile
                .sources()
                .iter()
                .find(|profile_source| profile_source.ordinal().index() == source.ordinal)
                .ok_or_else(|| format!("packaged source {} is missing", source.ordinal))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sources = selected
        .iter()
        .map(|source| {
            Ok(InjectedLibrarySource {
                file_ordinal: source.ordinal(),
                name: source.name(),
                source: std::str::from_utf8(source.bytes())
                    .map_err(|_| "packaged source is not UTF-8".to_owned())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    materialize_project_local_profile_for_test(&sources, plan).map_err(|error| format!("{error:?}"))
}

#[test]
fn two_fresh_projects_own_every_runtime_component_before_demand() {
    let index = Arc::new(ProjectLocalSymbolicIndex::build(&[0, 23]).expect("symbolic index"));
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let first_thread = {
        let index = Arc::clone(&index);
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            let mut plan = index.materialization_plan(vec!["Array".to_owned()], false);
            plan.pre_demand_barrier = Some(barrier);
            materialize_with_plan(&index, &plan).expect("first project")
        })
    };
    let second_thread = {
        let index = Arc::clone(&index);
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            let mut plan = index.materialization_plan(vec!["Array".to_owned()], false);
            plan.pre_demand_barrier = Some(barrier);
            materialize_with_plan(&index, &plan).expect("second project")
        })
    };
    let first = first_thread.join().expect("first project thread");
    let second = second_thread.join().expect("second project thread");

    assert_ne!(first.project_owner, second.project_owner);
    assert_ne!(first.binder_identity, second.binder_identity);
    assert_ne!(first.interner_identity, second.interner_identity);
    assert_ne!(first.store_identity, second.store_identity);
    assert_ne!(
        first.type_environment_identity,
        second.type_environment_identity
    );
    assert_ne!(first.universe_identity, second.universe_identity);
    assert_ne!(first.cache_identity, second.cache_identity);
    assert_ne!(
        first.global_object_owner_identity,
        second.global_object_owner_identity
    );
    for receipt in [&first, &second] {
        assert_eq!(receipt.initial_terminal_count, 0);
        assert_eq!(receipt.initial_cache_count, 0);
        assert!(!receipt.initial_global_object_present);
    }
    assert_eq!(
        first
            .terminals
            .iter()
            .map(|terminal| (
                &terminal.group,
                &terminal.symbolic_identity,
                terminal.terminal_kind
            ))
            .collect::<Vec<_>>(),
        second
            .terminals
            .iter()
            .map(|terminal| (
                &terminal.group,
                &terminal.symbolic_identity,
                terminal.terminal_kind
            ))
            .collect::<Vec<_>>()
    );
    assert!(first
        .terminals
        .iter()
        .all(|terminal| terminal.project_owner == first.project_owner));
    assert!(second
        .terminals
        .iter()
        .all(|terminal| terminal.project_owner == second.project_owner));
}

#[test]
fn array_demand_reserves_and_freezes_its_publication_scc_atomically() {
    let index = ProjectLocalSymbolicIndex::build(&[0, 23]).expect("symbolic index");
    let receipt = materialize(&index, &["Array"], false).expect("Array materialization");
    let names = receipt
        .terminals
        .iter()
        .map(|terminal| terminal.group.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(names, ["Array", "ArrayConstructor", "ConcatArray"].into());
    assert!(receipt.repeated_demand_was_stable);
    assert!(receipt.resolving_observation_was_blocked);
    assert!(!receipt.atomic_publication);
    let first_resolving = receipt
        .transitions
        .iter()
        .position(|transition| transition.to == ProjectLocalGroupStateForTest::Resolving)
        .expect("resolving transition");
    assert!(receipt.transitions[..first_resolving]
        .iter()
        .all(|transition| transition.to == ProjectLocalGroupStateForTest::Reserved));
    let component = ["Array", "ArrayConstructor", "ConcatArray"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(receipt
        .transitions
        .iter()
        .filter(|transition| transition.to == ProjectLocalGroupStateForTest::Resolving)
        .all(|transition| transition.component == component));
}

#[test]
fn readonly_array_demand_adds_only_its_exact_dependency_closure() {
    let index = ProjectLocalSymbolicIndex::build(&[0, 23]).expect("symbolic index");
    let receipt =
        materialize(&index, &["ReadonlyArray"], false).expect("ReadonlyArray materialization");
    let names = receipt
        .terminals
        .iter()
        .map(|terminal| terminal.group.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        names,
        ["Array", "ArrayConstructor", "ConcatArray", "ReadonlyArray"].into()
    );
    assert_eq!(receipt.publication_validations, 0);
    assert!(!receipt.atomic_publication);
}

#[test]
fn force_all_reproduces_the_reflect_ready_gate_falsifier() {
    let index = ProjectLocalSymbolicIndex::build(&[0, 23]).expect("symbolic index");
    let receipt = materialize(&index, &[], true).expect("force-all materialization");

    let profile = ExactLibraryProfile::load_packaged().expect("packaged profile");
    let sources = index
        .sources
        .iter()
        .map(|planned| {
            let source = profile
                .sources()
                .iter()
                .find(|source| source.ordinal().index() == planned.ordinal)
                .expect("oracle source");
            InjectedLibrarySource {
                file_ordinal: source.ordinal(),
                name: source.name(),
                source: std::str::from_utf8(source.bytes()).expect("UTF-8 source"),
            }
        })
        .collect::<Vec<_>>();
    let oracle_plan = index.materialization_plan(Vec::new(), true);
    let oracle = compile_project_local_force_all_oracle_for_test(&sources, &oracle_plan)
        .expect("exact-anchor eager control");

    assert_eq!(receipt.terminals.len(), index.groups.len());
    assert_eq!(
        receipt
            .terminals
            .iter()
            .filter(|terminal| terminal.terminal_kind == "ready")
            .count(),
        11
    );
    let reflect = receipt
        .terminals
        .iter()
        .find(|terminal| terminal.group == "Reflect")
        .expect("Reflect terminal");
    assert_eq!(reflect.terminal_kind, "unavailable");
    assert_eq!(
        reflect.unavailable_cause.as_deref(),
        Some("FunctionSurfaceUnavailable")
    );
    assert!(receipt.atomic_publication);
    assert_eq!(receipt.publication_validations, 11);

    let candidate_census = receipt
        .terminals
        .iter()
        .map(|terminal| {
            (
                terminal.group.as_str(),
                terminal.terminal_kind,
                terminal.unavailable_cause.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    let eager_census = oracle
        .iter()
        .map(|terminal| {
            (
                terminal.group.as_str(),
                terminal.terminal_kind,
                terminal.unavailable_cause.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(candidate_census, eager_census);

    let frozen_wu0 = include_str!("../../../tooling/project-local-index-feasibility/spec.py");
    assert!(frozen_wu0.contains("if group[\"state\"] != \"Ready\":"));
}

#[test]
fn anchor_and_demand_mismatch_fail_closed_before_query_publication() {
    let index = ProjectLocalSymbolicIndex::build(&[0, 23]).expect("symbolic index");
    let mut bad_anchor = index.materialization_plan(vec!["Array".to_owned()], false);
    bad_anchor.sources[0].statement_starts[0] += 1;
    let profile = ExactLibraryProfile::load_packaged().expect("packaged profile");
    let sources = bad_anchor
        .sources
        .iter()
        .map(|planned| {
            let source = profile
                .sources()
                .iter()
                .find(|source| source.ordinal() == planned.file_ordinal)
                .expect("selected source");
            InjectedLibrarySource {
                file_ordinal: source.ordinal(),
                name: source.name(),
                source: std::str::from_utf8(source.bytes()).expect("UTF-8 source"),
            }
        })
        .collect::<Vec<_>>();
    let anchor_error = materialize_project_local_profile_for_test(&sources, &bad_anchor)
        .expect_err("foreign anchor must fail");
    assert!(format!("{anchor_error:?}").contains("anchor is missing or duplicated"));

    let demand_error =
        materialize(&index, &["UnknownGroup"], false).expect_err("unknown demand must fail");
    assert!(demand_error.contains("demand names an unknown group"));
}

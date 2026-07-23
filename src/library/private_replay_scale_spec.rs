//! Disabled RED contract for WU5 replay physical-work scaling.

use super::base::{PrivateExecutionForTest, UserDeltaProjectInputForTest};
use super::{FrozenLibraryBase, LibraryBaseProvider};
use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

const REPLAY_BASE_FAMILIES: &str = concat!(
    "store.rows,store.payload-tables,store.type-param-constraints,store.frozen-type-params,",
    "store.template-names,interner.dedup-buckets,interner.reserved-terminals,interner.well-known,",
    "binder.scopes,binder.symbols,binder.declarations,binder.declaration-site-index,",
    "binder.type-groups,binder.namespaces,binder.namespace-indexes,binder.module-sources,",
    "decl-types.slots,published-types.groups,published-types.classes,namespace-terminals,",
    "function-groups.symbols,class.application-parameters,class.parameter-defaults,class.parents,",
    "class.names,class.new-metadata,class.value-identities,class.aliases,semantic-identities,",
    "root-name-index.entries,next-ids,replay.root-slots,replay.owner-sites,replay.reverse-edges,",
    "replay.scc-membership,replay.statement-owners,replay.baseline-records",
);

const SCHEDULER_WORK_FAMILIES: [&str; 10] = [
    "seed-pushes",
    "seed-pops",
    "scc-queue-pushes",
    "scc-queue-pops",
    "edge-probes",
    "owner-set-probes",
    "owner-set-inserts",
    "sort-items",
    "dedup-items",
    "replay-allocations",
];

fn acquire() -> Arc<FrozenLibraryBase> {
    LibraryBaseProvider::new()
        .get()
        .expect("canonical frozen library base")
}

fn input<'source>(
    path: &'source str,
    source: &'source str,
) -> UserDeltaProjectInputForTest<'source> {
    UserDeltaProjectInputForTest { path, source }
}

fn assert_zero_unaffected_base_work(work: &super::base::PrivateReplayBaseWorkForTest) {
    let expected = REPLAY_BASE_FAMILIES.split(',').collect::<BTreeSet<_>>();
    for ledger in [
        &work.sequential_scans,
        &work.materializations,
        &work.clones,
        &work.remaps,
        &work.direct_iterations,
        &work.borrowed_iterations,
    ] {
        assert_eq!(ledger.keys().copied().collect::<BTreeSet<_>>(), expected);
        assert!(ledger.values().all(|count| *count == 0), "{ledger:#?}");
    }
}

#[test]
fn replay_work_receipts_detect_every_iteration_entry_point() {
    let calibration = FrozenLibraryBase::calibrate_private_replay_base_work_for_test()
        .expect("one calibrated operation per family and entry point");
    let expected = REPLAY_BASE_FAMILIES.split(',').collect::<BTreeSet<_>>();
    for ledger in [
        calibration.sequential_scans,
        calibration.materializations,
        calibration.clones,
        calibration.remaps,
        calibration.direct_iterations,
        calibration.borrowed_iterations,
    ] {
        assert_eq!(ledger.keys().copied().collect::<BTreeSet<_>>(), expected);
        assert!(ledger.values().all(|count| *count == 1), "{ledger:#?}");
    }

    let scheduler = FrozenLibraryBase::calibrate_private_replay_scheduler_work_for_test()
        .expect("one calibrated scheduler operation per family");
    assert_eq!(
        scheduler.keys().copied().collect::<BTreeSet<_>>(),
        SCHEDULER_WORK_FAMILIES.into_iter().collect::<BTreeSet<_>>()
    );
    assert!(
        scheduler.values().all(|count| *count == 1),
        "{scheduler:#?}"
    );
}

#[test]
fn unique_global_replay_work_is_independent_of_unaffected_frozen_base_size() {
    let source = r#"declare var WU5ScaleGlobal: { value: number };
function WU5ScaleFunction(): number { return 1; }
const value: number = globalThis.WU5ScaleGlobal.value;
const called: number = globalThis.WU5ScaleFunction();
"#;
    let comparison = FrozenLibraryBase::compare_private_replay_across_base_sizes_for_test(source)
        .expect("small and padded bases run the same private replay");

    assert_eq!(
        comparison.small.execution,
        PrivateExecutionForTest::SelectiveReplay
    );
    assert_eq!(
        comparison.large.execution,
        PrivateExecutionForTest::SelectiveReplay
    );
    assert_eq!(comparison.small.observable, comparison.large.observable);
    assert_eq!(
        comparison.small.candidate_owner_keys,
        comparison.large.candidate_owner_keys
    );
    assert_eq!(
        comparison.small.source_oracle_owner_keys,
        comparison.large.source_oracle_owner_keys
    );
    assert_eq!(
        comparison.small.candidate_owner_keys,
        comparison.small.source_oracle_owner_keys
    );
    assert_eq!(
        comparison.large.candidate_owner_keys,
        comparison.large.source_oracle_owner_keys
    );
    assert_eq!(
        comparison.small.physical_semantic_work,
        comparison.large.physical_semantic_work
    );
    assert_eq!(comparison.small.full_source_fallbacks, 0);
    assert_eq!(comparison.large.full_source_fallbacks, 0);
    assert!(comparison.large.frozen_rows > comparison.small.frozen_rows + 4_000);
    assert_zero_unaffected_base_work(&comparison.small.unaffected_base_work);
    assert_zero_unaffected_base_work(&comparison.large.unaffected_base_work);
}

fn assert_linear_scheduler_work(
    work: &std::collections::BTreeMap<&'static str, u64>,
    owner_count: usize,
    edge_count: usize,
) {
    assert_eq!(
        work.keys().copied().collect::<BTreeSet<_>>(),
        SCHEDULER_WORK_FAMILIES.into_iter().collect::<BTreeSet<_>>()
    );
    let input_size = u64::try_from(owner_count + edge_count + 1).expect("scheduler input fits u64");
    assert!(work.values().sum::<u64>() <= 12 * input_size, "{work:#?}");
}

#[test]
fn global_object_replay_is_linear_in_user_roots_and_matches_full_source_outputs() {
    let receipts = [1, 32, 64, 128, 256].map(|root_count| {
        FrozenLibraryBase::measure_unique_global_replay_for_test(root_count)
            .expect("unique globals replay")
    });

    for receipt in &receipts {
        assert_eq!(receipt.execution, PrivateExecutionForTest::SelectiveReplay);
        assert_eq!(receipt.full_source_fallbacks, 0);
        assert_eq!(receipt.dependency_edge_escapes, 0);
        assert_eq!(receipt.candidate_seed_keys, receipt.source_oracle_seed_keys);
        assert_eq!(
            receipt.candidate_owner_keys,
            receipt.source_oracle_owner_keys
        );
        assert_eq!(
            receipt.candidate_closure_edge_keys,
            receipt.source_oracle_closure_edge_keys
        );
        assert_eq!(receipt.scheduled_owner_keys, receipt.candidate_owner_keys);
        assert_eq!(
            receipt.considered_closure_edge_keys,
            receipt.candidate_closure_edge_keys
        );
        assert_eq!(
            receipt.candidate_semantics_by_source,
            receipt.full_source_semantics_by_source
        );
        assert_eq!(receipt.generated_root_count, receipt.requested_root_count);
        assert_eq!(
            receipt.generated_global_this_property_count,
            receipt.requested_root_count
        );
        assert!(receipt.candidate_seed_keys.contains("global-object"));
        assert_linear_scheduler_work(
            &receipt.scheduler_work,
            receipt.candidate_owner_keys.len() + receipt.user_owner_count,
            receipt.candidate_closure_edge_keys.len(),
        );
    }
    let fixed_library_visits = receipts[0].library_owner_visits;
    assert!(receipts
        .iter()
        .all(|receipt| receipt.library_owner_visits == fixed_library_visits));
}

fn assert_linear_closure_work(receipt: &super::base::DomReplayReceiptForTest) {
    assert_eq!(receipt.execution, PrivateExecutionForTest::SelectiveReplay);
    assert_eq!(receipt.full_source_fallbacks, 0);
    assert_eq!(receipt.dependency_edge_escapes, 0);
    assert_eq!(
        receipt.candidate_owner_keys,
        receipt.source_oracle_owner_keys
    );
    assert_eq!(
        receipt.candidate_closure_edge_keys,
        receipt.source_oracle_closure_edge_keys
    );
    assert_eq!(receipt.scheduled_owner_keys, receipt.candidate_owner_keys);
    assert_eq!(
        receipt.considered_closure_edge_keys,
        receipt.candidate_closure_edge_keys
    );
    assert_eq!(
        receipt.candidate_semantics_by_source,
        receipt.full_source_semantics_by_source
    );
    assert_eq!(receipt.generated_shape.keyof_event_map_uses, 1);
    assert_eq!(receipt.generated_shape.indexed_event_map_uses, 1);
    assert_eq!(
        receipt.generated_shape.overload_pairs,
        receipt.requested_width
    );
    assert_eq!(
        receipt.generated_shape.recursive_receivers,
        receipt.requested_width
    );
    assert_eq!(
        receipt.generated_shape.heritage_edges,
        receipt.requested_width
    );
    assert_eq!(receipt.generated_shape.collision_seed, "type:EventMap");
    assert_linear_scheduler_work(
        &receipt.scheduler_work,
        receipt.candidate_owner_keys.len(),
        receipt.candidate_closure_edge_keys.len(),
    );
}

#[test]
fn dom_listener_map_replays_the_exact_reverse_closure_in_linear_physical_work() {
    let narrow = FrozenLibraryBase::measure_dom_listener_replay_for_test(64, 0)
        .expect("64-owner DOM replay");
    let medium = FrozenLibraryBase::measure_dom_listener_replay_for_test(256, 0)
        .expect("256-owner DOM replay");
    let wide = FrozenLibraryBase::measure_dom_listener_replay_for_test(1_024, 0)
        .expect("1,024-owner DOM replay");
    let padded = FrozenLibraryBase::measure_dom_listener_replay_for_test(1_024, 4_096)
        .expect("padded DOM replay");

    for receipt in [&narrow, &medium, &wide, &padded] {
        assert_linear_closure_work(receipt);
    }
    assert!(medium.candidate_owner_keys.len() > narrow.candidate_owner_keys.len());
    assert!(wide.candidate_owner_keys.len() > medium.candidate_owner_keys.len());
    assert_eq!(wide.candidate_owner_keys, padded.candidate_owner_keys);
    assert_eq!(wide.physical_semantic_work, padded.physical_semantic_work);
    assert_zero_unaffected_base_work(&padded.unaffected_base_work);
}

#[test]
fn exact_locked_production_collision_uses_replay_without_source_fallback() {
    let root = crate::test_repository_root().join("tooling/full-lib-bench/workloads/collision");
    let augment = fs::read_to_string(root.join("00_augment.ts")).expect("locked augmentation");
    let consume = fs::read_to_string(root.join("99_consume.ts")).expect("locked consumer");
    let inputs = [
        input("/locked/00_augment.ts", &augment),
        input("/locked/99_consume.ts", &consume),
    ];
    let receipt = acquire()
        .check_routed_user_project_against_full_source_oracle_for_test(&inputs)
        .expect("locked collision candidate and oracle finish");

    assert_eq!(receipt.execution, PrivateExecutionForTest::SelectiveReplay);
    assert!(receipt.workload_lock_verified);
    assert_eq!(receipt.work.full_source_fallbacks, 0);
    assert_eq!(
        receipt.oracle.candidate_semantics_by_source,
        receipt.oracle.full_source_semantics_by_source
    );
    assert_eq!(
        receipt.work.replayed_owner_keys,
        receipt.work.authenticated_expected_reverse_closure
    );
}

fn fanout_projects(count: usize) -> Vec<Vec<(String, String)>> {
    (0..count)
        .map(|index| {
            let method = format!("wu5Fanout{index}");
            let source = format!(
                "interface Array<T> {{ {method}(): T; }}\n\
                 const own: number = [1, 2].{method}();\n\
                 const mapped: number[] = [1, 2].map((value) => value + 1);\n"
            );
            vec![(format!("/fanout/{index:02}/main.ts"), source)]
        })
        .collect()
}

#[test]
fn all_colliding_fanout_uses_32_distinct_private_projects_and_one_owned_permit() {
    let owned = fanout_projects(32);
    let projects = owned
        .iter()
        .map(|project| {
            project
                .iter()
                .map(|(path, source)| input(path, source))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let first = FrozenLibraryBase::run_all_colliding_projects_concurrently_for_test(&projects)
        .expect("first 32-project fanout");
    let second = FrozenLibraryBase::run_all_colliding_projects_concurrently_for_test(&projects)
        .expect("deterministic 32-project rerun");

    assert_eq!(first.route_receipts.len(), 32);
    assert_eq!(first.private_universe_drop_witnesses.len(), 32);
    assert!(first
        .route_receipts
        .iter()
        .all(|receipt| receipt.execution == PrivateExecutionForTest::SelectiveReplay));
    assert!(first
        .route_receipts
        .iter()
        .all(|receipt| receipt.file_count == 1 && receipt.full_source_fallbacks == 0));
    assert_eq!(first.start_barrier_arrivals, 32);
    assert_eq!(first.private_permit_acquisitions, 32);
    assert!(first.max_private_contenders >= 2);
    assert_eq!(first.max_private_concurrency, 1);
    assert_eq!(first.shared_base_mutations, 0);
    assert_eq!(first.cross_project_user_name_leaks, 0);
    assert_eq!(
        first.normalized_results_by_project,
        second.normalized_results_by_project
    );
    assert_eq!(first.normalized_results_by_project.len(), 32);
    for (index, result) in first.normalized_results_by_project.iter().enumerate() {
        assert_eq!(result.project_identity, format!("/fanout/{index:02}"));
        assert!(result
            .visible_user_methods
            .contains(&format!("wu5Fanout{index}")));
        assert_eq!(result.visible_user_methods.len(), 1);
    }
    assert!(first
        .private_universe_drop_witnesses
        .iter()
        .all(|dropped| *dropped));
    assert_eq!(first.private_lifecycle_epochs.len(), 32);
    assert!(first.private_lifecycle_epochs.iter().all(|epoch| {
        epoch.permit_acquired < epoch.private_state_dropped
            && epoch.private_state_dropped < epoch.permit_released
    }));
}

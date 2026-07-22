//! RED contract for the WU4 project route and base-size-independent delta cost.

use super::base::{
    BaseWorkCalibrationInjectionForTest, FrozenLibraryBase, UserDeltaProjectInputForTest,
};
use super::provider::LibraryBaseProvider;
use std::collections::BTreeSet;
use std::sync::Arc;

const ID_DOMAINS: [&str; 9] = [
    "types",
    "type-params",
    "classes",
    "scopes",
    "symbols",
    "declarations",
    "type-groups",
    "namespaces",
    "value-storages",
];

const BASE_ROW_FAMILIES: &str = concat!(
    "store.rows,store.payload-tables,store.type-param-constraints,store.frozen-type-params,",
    "store.template-names,interner.dedup-buckets,interner.reserved-terminals,interner.well-known,",
    "binder.scopes,binder.symbols,binder.declarations,binder.declaration-site-index,",
    "binder.type-groups,binder.namespaces,binder.namespace-indexes,binder.module-sources,",
    "decl-types.slots,published-types.groups,published-types.classes,namespace-terminals,",
    "function-groups.symbols,class.application-parameters,class.parameter-defaults,class.parents,",
    "class.names,class.new-metadata,class.value-identities,class.aliases,semantic-identities,",
    "root-name-index.entries,next-ids",
);

const CALIBRATION: BaseWorkCalibrationInjectionForTest = BaseWorkCalibrationInjectionForTest {
    sequential_scans: 3,
    materializations: 5,
    clones: 7,
    remaps: 11,
};

const MODEL_PATH: &str = "/project/model.ts";
const MODEL_SOURCE: &str = r#"export interface SharedShape { value: string }
export const sharedValue = "ok";
"#;

const CONSUMER_PATH: &str = "/project/consumer.ts";
const CONSUMER_SOURCE: &str = r#"import { SharedShape, sharedValue } from "./model";
const valueBad: number = sharedValue;
const shapeBad: SharedShape = { value: 123 };
void valueBad;
void shapeBad;
"#;

const ISOLATION_SOURCE: &str = r#"export type ProbeShape = SharedShape;
export const probeValue = sharedValue;
"#;

const SCALE_SOURCE: &str = r#"export namespace LocalSpace {
    export class Box<T> {
        constructor(public value: T) {}
    }
    export type LocalShapeA = { value: string };
    export type LocalShapeB = { value: string };
    export const label = "delta";
}
const good: LocalSpace.Box<string> = new LocalSpace.Box("ok");
const broken: string = 123;
void good;
void broken;
"#;

fn acquire() -> Arc<FrozenLibraryBase> {
    LibraryBaseProvider::new()
        .get()
        .expect("canonical frozen library base")
}

fn project_inputs() -> [UserDeltaProjectInputForTest<'static>; 2] {
    [
        UserDeltaProjectInputForTest {
            path: MODEL_PATH,
            source: MODEL_SOURCE,
        },
        UserDeltaProjectInputForTest {
            path: CONSUMER_PATH,
            source: CONSUMER_SOURCE,
        },
    ]
}

#[test]
fn one_project_delta_preserves_cross_file_symbols_and_is_deterministic() {
    let base = acquire();
    let base_identity = base.storage_identity_for_test();
    let projection_before = base
        .recompute_canonical_projection_for_test()
        .expect("frozen projection before project check")
        .typed_validation_sha256();
    let first = base
        .check_caller_certified_collision_free_user_project_for_test(&project_inputs())
        .expect("collision-free project checks against one private delta");
    let mut reversed_inputs = project_inputs();
    reversed_inputs.reverse();
    let second = base
        .check_caller_certified_collision_free_user_project_for_test(&reversed_inputs)
        .expect("reversed inputs receive an isolated private delta");

    assert_eq!(first, second);
    assert_eq!(
        first.normalized_diagnostics(),
        [
            concat!(
                "/project/consumer.ts:2:7-2:15 TK2322 ",
                "Type 'string' is not assignable to type 'number'"
            ),
            concat!(
                "/project/consumer.ts:3:7-3:15 TK2322 ",
                "Type '{ value: number }' is not assignable to type '{ value: string }'"
            ),
        ]
    );
    assert!(first.incompletes.is_empty(), "{:#?}", first.incompletes);
    assert!(first.initial_visible_user_names.is_empty());
    assert_eq!(
        first.cross_file.producer_type_group,
        first.cross_file.consumer_type_group
    );
    assert_eq!(
        first.cross_file.producer_value_storage,
        first.cross_file.consumer_value_storage
    );
    assert!(first
        .ranges
        .type_groups
        .range
        .contains(&first.cross_file.producer_type_group.index()));
    assert!(first
        .ranges
        .value_storages
        .range
        .contains(&first.cross_file.producer_value_storage.index()));
    assert!(first.references.delta_to_base > 0);
    assert!(first.references.delta_to_delta > 0);
    assert_eq!(first.references.base_to_delta, 0);
    assert_eq!(first.mutation.base_rows_written, 0);
    assert!(first.mutation.delta_discarded_after_check);
    assert_eq!(first.work.library_source_compiles, 0);
    assert_eq!(first.work.library_source_parses, 0);
    assert_eq!(first.work.library_source_binds, 0);
    assert_eq!(first.work.library_source_checks, 0);
    assert_eq!(first.work.snapshot_decodes, 0);
    assert_eq!(first.work.snapshot_validations, 0);
    assert_eq!(first.work.user_source_parses, 2);
    assert_eq!(first.work.user_source_binds, 2);
    assert_eq!(first.work.user_source_checks, 2);
    assert!(first.work.base_row_clones.values().all(|rows| *rows == 0));

    let isolation = base
        .check_caller_certified_collision_free_user_source_for_test(
            "/project/isolation.ts",
            ISOLATION_SOURCE,
        )
        .expect("unrelated source checks against a fresh delta");
    assert_eq!(
        isolation.normalized_diagnostics(),
        [
            "/project/isolation.ts:1:26-1:37 TK2304 Cannot find name 'SharedShape'",
            "/project/isolation.ts:2:27-2:38 TK2304 Cannot find name 'sharedValue'",
        ]
    );
    assert!(isolation.incompletes.is_empty());
    assert_eq!(base.storage_identity_for_test(), base_identity);
    assert_eq!(
        base.recompute_canonical_projection_for_test()
            .expect("frozen projection after project checks")
            .typed_validation_sha256(),
        projection_before
    );
}

#[test]
fn per_check_allocation_and_traversal_do_not_scale_with_frozen_base_size() {
    let witness = FrozenLibraryBase::compare_user_delta_cost_across_base_sizes_for_test(
        SCALE_SOURCE,
        CALIBRATION,
    )
    .expect("small and padded frozen bases run the same collision-free delta");

    let expected_domains = ID_DOMAINS.into_iter().collect::<BTreeSet<_>>();
    let small_prefixes = witness.small.frozen_prefixes.named_rows_for_test();
    let large_prefixes = witness.large.frozen_prefixes.named_rows_for_test();
    assert_eq!(
        small_prefixes.keys().copied().collect::<BTreeSet<_>>(),
        expected_domains
    );
    assert_eq!(
        large_prefixes.keys().copied().collect::<BTreeSet<_>>(),
        expected_domains
    );
    for (family, small) in &small_prefixes {
        let large = large_prefixes[family];
        assert!(large > *small, "large base must grow the {family} prefix");
    }
    assert!(
        large_prefixes.values().sum::<usize>() >= small_prefixes.values().sum::<usize>() + 4_096,
        "the large control must add at least 4096 frozen rows"
    );

    assert_eq!(witness.small.observable, witness.large.observable);
    for measure in [&witness.small, &witness.large] {
        assert_eq!(
            measure.observable.normalized_diagnostics(),
            [concat!(
                "/project/user-delta-scale.ts:10:7-10:13 TK2322 ",
                "Type 'number' is not assignable to type 'string'"
            )]
        );
        assert!(measure.observable.incompletes.is_empty());
        let left = measure
            .observable
            .local_alias_type("LocalSpace.LocalShapeA")
            .expect("first local alias type");
        let right = measure
            .observable
            .local_alias_type("LocalSpace.LocalShapeB")
            .expect("second local alias type");
        assert_eq!(left, right, "equal local aliases hash-cons once");
        assert!(measure.observable.local_type_range.contains(&left.index()));
    }

    assert_eq!(
        witness
            .small
            .local_allocations
            .keys()
            .copied()
            .collect::<BTreeSet<_>>(),
        expected_domains
    );
    assert_eq!(
        witness
            .large
            .local_allocations
            .keys()
            .copied()
            .collect::<BTreeSet<_>>(),
        expected_domains
    );
    assert_eq!(
        witness.small.local_allocations, witness.large.local_allocations,
        "user-owned allocation depends only on the user suffix"
    );
    assert!(witness
        .small
        .local_allocations
        .values()
        .all(|rows| *rows > 0));

    let expected_families = BASE_ROW_FAMILIES.split(',').collect::<BTreeSet<_>>();
    for measure in [&witness.small, &witness.large] {
        for counters in [
            &measure.base_rows_sequentially_scanned,
            &measure.base_rows_materialized,
            &measure.base_rows_cloned,
            &measure.base_rows_remapped,
        ] {
            assert_eq!(
                counters.keys().copied().collect::<BTreeSet<_>>(),
                expected_families,
                "every frozen row family has an explicit counter"
            );
            assert!(counters.values().all(|rows| *rows == 0));
        }
    }

    assert_eq!(
        witness.calibration.observed.sequential_scans,
        CALIBRATION.sequential_scans
    );
    assert_eq!(
        witness.calibration.observed.materializations,
        CALIBRATION.materializations
    );
    assert_eq!(witness.calibration.observed.clones, CALIBRATION.clones);
    assert_eq!(witness.calibration.observed.remaps, CALIBRATION.remaps);
}

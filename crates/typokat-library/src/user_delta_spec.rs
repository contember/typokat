//! Disabled RED contract for one caller-certified, collision-free user-source delta.
//!
//! WU4 owns the immutable-base/layered-delta route exercised here. WU5 owns collision
//! preflight and routing; this contract deliberately supplies the certification itself.

use super::base::FrozenLibraryBase;
use super::provider::shared_library_base_provider_for_test;
use std::collections::BTreeSet;
use std::sync::{Arc, Barrier};

const USER_PATH: &str = "/project/user-delta.ts";
const USER_SOURCE: &str = r#"export namespace DeltaSpace {
    export class Box<T> {
        constructor(public value: T) {}
    }

    export type LocalShapeA = { value: string };
    export type LocalShapeB = { value: string };
    export const label = "delta";
}
export type ExistingBaseShape = "arraybuffer" | "blob";
export type RunSentinel = DeltaSpace.LocalShapeA;

const good: DeltaSpace.Box<string> = new DeltaSpace.Box("ok");
const broken: string = 123;
void good;
void broken;
"#;

const ISOLATION_PATH: &str = "/project/isolation-probe.ts";
const ISOLATION_SOURCE: &str = concat!(
    "export type Probe = RunSentinel;\n",
    "export const value = DeltaSpace.label;\n",
);

const PUBLISHED_TEMPLATE_PATH: &str = "/project/published-template.ts";
const PUBLISHED_TEMPLATE_SOURCE: &str = r#"interface Holder {
    x: OmitThisParameter<string>;
}
declare const holder: Holder;
const accepted: string = holder.x;
const rejected: number = holder.x;
"#;

const BASE_ROW_FAMILIES: &str = concat!(
    "store.rows,store.payload-tables,store.type-param-constraints,store.frozen-type-params,",
    "store.template-names,interner.dedup-buckets,interner.reserved-terminals,",
    "interner.declared-recipes,interner.well-known,",
    "binder.scopes,binder.symbols,binder.declarations,binder.declaration-site-index,",
    "binder.type-groups,binder.namespaces,binder.namespace-indexes,binder.module-sources,",
    "decl-types.slots,published-types.groups,published-types.classes,namespace-terminals,",
    "function-groups.symbols,class.application-parameters,class.parameter-defaults,class.parents,",
    "class.names,class.new-metadata,class.value-identities,class.aliases,semantic-identities,",
    "root-name-index.entries,next-ids",
);

/// The process-wide source-compiled base. Compiling all 82 packaged sources costs seconds, so
/// every spec in this binary shares one.
fn acquire() -> Arc<FrozenLibraryBase> {
    shared_library_base_provider_for_test()
        .get()
        .expect("source-compiled default library base")
}

fn check_source(
    base: &FrozenLibraryBase,
    path: &str,
    source: &str,
) -> super::base::UserDeltaCheckReceiptForTest {
    // The production WU5 classifier will be the only source of this certification.
    base.check_caller_certified_collision_free_user_source_for_test(path, source)
        .expect("collision-free external module checks against a layered delta")
}

fn check(base: &FrozenLibraryBase) -> super::base::UserDeltaCheckReceiptForTest {
    check_source(base, USER_PATH, USER_SOURCE)
}

fn assert_dense_suffix(
    domain: &super::base::UserDeltaDomainRangeForTest,
    frozen_prefix: usize,
    family: &str,
) {
    let range = &domain.range;
    assert_eq!(
        range.start, frozen_prefix,
        "{family} starts at frozen prefix"
    );
    assert!(
        range.end > range.start,
        "{family} contributes a local suffix"
    );
    assert_eq!(
        domain.allocated_ids,
        range.clone().collect::<Vec<_>>(),
        "{family} ids exactly fill the dense suffix"
    );
}

fn assert_dense_suffix_ranges(
    frozen: &super::base::FrozenLibraryPrefixes,
    delta: &super::base::UserDeltaRangesForTest,
) {
    assert_dense_suffix(&delta.types, frozen.types, "types");
    assert_dense_suffix(&delta.type_params, frozen.type_params, "type params");
    assert_dense_suffix(&delta.classes, frozen.classes, "classes");
    assert_dense_suffix(&delta.scopes, frozen.scopes, "scopes");
    assert_dense_suffix(&delta.symbols, frozen.symbols, "symbols");
    assert_dense_suffix(&delta.declarations, frozen.declarations, "declarations");
    assert_dense_suffix(&delta.type_groups, frozen.type_groups, "type groups");
    assert_dense_suffix(&delta.namespaces, frozen.namespaces, "namespaces");
    assert_dense_suffix(
        &delta.value_storages,
        frozen.value_storages,
        "value storages",
    );
}

fn assert_exact_assignment_diagnostic(receipt: &super::base::UserDeltaCheckReceiptForTest) {
    assert!(
        receipt.initial_visible_user_names.is_empty(),
        "a fresh delta exposes no prior user names"
    );
    assert_eq!(
        receipt.normalized_diagnostics(),
        [concat!(
            "/project/user-delta.ts:14:7-14:13 TK2322 ",
            "Type 'number' is not assignable to type 'string'"
        )]
    );
    assert!(receipt.incompletes.is_empty(), "{:#?}", receipt.incompletes);
}

fn assert_exact_isolation_diagnostic(receipt: &super::base::UserDeltaCheckReceiptForTest) {
    assert!(
        receipt.initial_visible_user_names.is_empty(),
        "an isolation probe starts from an empty user suffix"
    );
    assert_eq!(
        receipt.normalized_diagnostics(),
        [
            "/project/isolation-probe.ts:1:21-1:32 TK2304 Cannot find name 'RunSentinel'",
            "/project/isolation-probe.ts:2:22-2:32 TK2304 Cannot find name 'DeltaSpace'",
        ]
    );
    assert!(receipt.incompletes.is_empty(), "{:#?}", receipt.incompletes);
}

fn assert_same_observable_result(
    expected: &super::base::UserDeltaCheckReceiptForTest,
    actual: &super::base::UserDeltaCheckReceiptForTest,
) {
    assert_eq!(actual, expected);
    assert_eq!(
        actual.initial_visible_user_names,
        expected.initial_visible_user_names
    );
    assert_eq!(actual.local_names, expected.local_names);
}

/// Everything about the frozen base a user delta must leave exactly as it found it.
fn base_oracle(base: &FrozenLibraryBase) -> super::base::FrozenBaseWitnessForTest {
    base.frozen_witness_for_test()
}

fn assert_no_forbidden_work(receipt: &super::base::UserDeltaCheckReceiptForTest) {
    let work = &receipt.work;
    assert_eq!(work.library_source_compiles, 0);
    assert_eq!(work.library_source_parses, 0);
    assert_eq!(work.library_source_binds, 0);
    assert_eq!(work.library_source_checks, 0);

    let expected = BASE_ROW_FAMILIES.split(',').collect::<BTreeSet<_>>();
    let observed = work
        .base_row_clones
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(observed, expected, "clone counters cover every base table");
    assert!(
        work.base_row_clones.values().all(|rows| *rows == 0),
        "no frozen row is cloned into a user delta"
    );
}

#[test]
fn collision_free_external_module_uses_dense_local_suffixes_and_real_interning() {
    let base = acquire();
    let base_pointer = Arc::as_ptr(&base);
    let base_identity = base.storage_identity_for_test();
    let frozen = base.prefixes_for_test().clone();
    let binary_type = base
        .named_type_for_test("BinaryType")
        .expect("lib.dom BinaryType is frozen in the canonical base");
    let base_shape = base
        .nonterminal_structural_type_probe_for_test()
        .expect("canonical base has a frozen nonterminal structural type");
    let receipt = check(&base);

    assert_exact_assignment_diagnostic(&receipt);
    assert_dense_suffix_ranges(&frozen, &receipt.ranges);
    assert_eq!(Arc::as_ptr(&base), base_pointer);
    assert_eq!(base.storage_identity_for_test(), base_identity);

    assert!(base_shape.base_type.index() < frozen.types);
    let base_reintern = base
        .reintern_structural_type_through_user_delta_for_test(base_shape.descriptor())
        .expect("frozen structural descriptor passes through the user-delta interner");
    assert_eq!(base_reintern.resolved_type, base_shape.base_type);
    assert_eq!(base_reintern.local_rows_added, 0);
    assert_eq!(base.storage_identity_for_test(), base_identity);

    let existing = *receipt
        .interning
        .named_alias_types
        .get("ExistingBaseShape")
        .expect("BinaryType-shaped user alias resolves");
    assert_eq!(existing, binary_type);
    assert!(existing.index() < frozen.types);
    assert!(!receipt.ranges.types.range.contains(&existing.index()));

    let left = receipt
        .interning
        .local_alias_type("DeltaSpace.LocalShapeA")
        .expect("first local alias type");
    let right = receipt
        .interning
        .local_alias_type("DeltaSpace.LocalShapeB")
        .expect("second local alias type");
    assert_eq!(left, right, "equal new local aliases hash-cons once");
    assert!(receipt.ranges.types.range.contains(&left.index()));

    assert_eq!(
        receipt.references.base_to_delta, 0,
        "a frozen base row now references a user-delta row: the delta leaked into the shared base"
    );
    assert!(receipt.references.delta_to_base > 0);
    assert!(receipt.references.delta_to_delta > 0);
    assert_eq!(receipt.mutation.base_rows_written, 0);
    assert!(receipt.mutation.local_rows_written > 0);
    assert!(receipt.mutation.delta_discarded_after_check);
}

#[test]
fn user_check_performs_no_library_work_or_base_row_clones() {
    let base = acquire();
    let receipt = check(&base);

    assert_no_forbidden_work(&receipt);
    assert!(receipt.work.user_source_parses > 0);
    assert!(receipt.work.user_source_binds > 0);
    assert!(receipt.work.user_source_checks > 0);
}

#[test]
fn packaged_omit_this_parameter_template_preserves_specialized_user_surface() {
    let base = acquire();
    let receipt = check_source(&base, PUBLISHED_TEMPLATE_PATH, PUBLISHED_TEMPLATE_SOURCE);

    assert_eq!(
        receipt.normalized_diagnostics(),
        [concat!(
            "/project/published-template.ts:6:7-6:15 TK2322 ",
            "Type 'string' is not assignable to type 'number'"
        )],
        "the frozen base's published template must preserve its specialized argument"
    );
    assert!(receipt.incompletes.is_empty(), "{:#?}", receipt.incompletes);
}

#[test]
fn two_sequential_runs_are_identical_and_do_not_publish_user_names() {
    let base = acquire();
    let base_identity = base.storage_identity_for_test();
    let oracle_before = base_oracle(&base);
    let first = check(&base);
    let second = check(&base);

    assert_same_observable_result(&first, &second);
    assert_exact_assignment_diagnostic(&first);
    assert_exact_assignment_diagnostic(&second);
    let isolation = check_source(&base, ISOLATION_PATH, ISOLATION_SOURCE);
    assert_exact_isolation_diagnostic(&isolation);
    assert_eq!(base.storage_identity_for_test(), base_identity);
    assert_eq!(base_oracle(&base), oracle_before);
    assert_no_forbidden_work(&first);
    assert_no_forbidden_work(&second);
    assert_no_forbidden_work(&isolation);
}

#[test]
fn thirty_two_concurrent_runs_share_one_base_but_no_user_names() {
    const CALLERS: usize = 32;

    let base = acquire();
    let barrier = Arc::new(Barrier::new(CALLERS));
    let base_identity = base.storage_identity_for_test();
    let oracle_before = base_oracle(&base);
    let runs = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(CALLERS);
        for _ in 0..CALLERS {
            let base = Arc::clone(&base);
            let barrier = Arc::clone(&barrier);
            handles.push(scope.spawn(move || {
                barrier.wait();
                let receipt = check(&base);
                (base, receipt)
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("user-check thread"))
            .collect::<Vec<_>>()
    });

    let expected = &runs[0].1;
    for (run_base, receipt) in &runs {
        assert!(Arc::ptr_eq(&base, run_base));
        assert_same_observable_result(expected, receipt);
        assert_exact_assignment_diagnostic(receipt);
        assert_no_forbidden_work(receipt);
    }
    let isolation = check_source(&base, ISOLATION_PATH, ISOLATION_SOURCE);
    assert_exact_isolation_diagnostic(&isolation);
    assert_no_forbidden_work(&isolation);
    assert_eq!(base.storage_identity_for_test(), base_identity);
    assert_eq!(base_oracle(&base), oracle_before);
}

#[test]
fn delta_construction_stays_private_and_test_receipts_stay_test_only() {
    let base_source = include_str!("base.rs");
    let module_source = include_str!("lib.rs");
    assert!(!module_source.lines().any(|line| {
        line.trim_start().starts_with("pub use") && line.to_ascii_lowercase().contains("delta")
    }));

    assert!(base_source
        .lines()
        .any(|line| line.trim() == "struct LayeredUserDelta {"));
    assert!(!base_source.lines().any(|line| {
        let line = line.trim();
        line.starts_with("pub") && line.contains("struct LayeredUserDelta")
    }));
    let delta_impl = base_source
        .split_once("impl LayeredUserDelta {")
        .map(|(_, body)| body)
        .expect("private layered-delta implementation");
    assert!(delta_impl.trim_start().starts_with("fn new("));

    let check_declaration =
        "pub(super) fn check_caller_certified_collision_free_user_source_for_test(";
    let check_offset = base_source
        .find(check_declaration)
        .expect("crate-private caller-certified check entry point");
    assert!(base_source[..check_offset]
        .lines()
        .rev()
        .take(4)
        .any(|line| line.trim() == "#[cfg(test)]"));
    assert_eq!(base_source.matches("LayeredUserDelta::new(").count(), 1);
    assert!(base_source[check_offset..].contains("LayeredUserDelta::new("));

    for declaration in [
        "pub(super) struct UserDeltaCheckReceiptForTest",
        "pub(super) struct UserDeltaRangesForTest",
        "pub(super) struct UserDeltaDomainRangeForTest",
    ] {
        let offset = base_source
            .find(declaration)
            .expect("test receipt declaration");
        assert!(
            base_source[..offset]
                .lines()
                .rev()
                .take(4)
                .any(|line| line.trim() == "#[cfg(test)]"),
            "{declaration} must stay test-only"
        );
    }
}

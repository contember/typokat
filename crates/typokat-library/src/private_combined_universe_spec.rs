//! Disabled RED contract for WU5 private combined-universe semantics.
//!
//! Candidate replay is always compared with the complete source-backed combined compiler. The
//! shared base is only the immutable routing oracle; it contributes no state to a private universe.

use super::base::{
    CollisionRouteForTest, PrivateCombinedReceiptForTest, PrivateExecutionForTest,
    UserDeltaProjectInputForTest,
};
use super::{FrozenLibraryBase, LibraryBaseProvider};
use std::sync::Arc;

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

fn check(inputs: &[UserDeltaProjectInputForTest<'_>]) -> PrivateCombinedReceiptForTest {
    acquire()
        .check_routed_user_project_against_full_source_oracle_for_test(inputs)
        .expect("private replay and full-source oracle both finish")
}

fn assert_private_replay(receipt: &PrivateCombinedReceiptForTest) {
    assert_eq!(
        receipt.preflight.route,
        CollisionRouteForTest::PrivateCombined
    );
    assert_eq!(receipt.execution, PrivateExecutionForTest::SelectiveReplay);
    assert!(!receipt.preflight.capability_issued);
    assert_eq!(
        receipt.oracle.candidate_semantics_by_source,
        receipt.oracle.full_source_semantics_by_source
    );
    assert_eq!(
        receipt.oracle.candidate_published_root_projection,
        receipt.oracle.full_source_published_root_projection
    );
    assert_eq!(
        receipt.oracle.candidate_semantic_identities,
        receipt.oracle.full_source_semantic_identities
    );
    assert_eq!(receipt.work.candidate_private_snapshot_decodes, 1);
    assert_eq!(receipt.work.candidate_library_parse_units, 82);
    assert_eq!(receipt.work.candidate_library_bind_units, 82);
    assert_eq!(receipt.work.oracle_library_parse_units, 82);
    assert_eq!(receipt.work.oracle_library_bind_units, 82);
    assert_eq!(receipt.work.full_source_fallbacks, 0);
    assert_eq!(receipt.work.dependency_edge_escapes, 0);
    assert_eq!(receipt.work.unexpected_library_records, 0);
    assert_eq!(
        receipt.work.replayed_owner_keys,
        receipt.work.authenticated_expected_reverse_closure
    );
    assert_eq!(
        receipt.universe.source_binder_prefix_digest,
        receipt.universe.snapshot_binder_prefix_digest
    );
    assert_eq!(
        receipt.universe.source_root_slot_digest,
        receipt.universe.snapshot_root_slot_digest
    );
    assert_eq!(receipt.universe.user_binder_rows_at_prefix_validation, 0);
    assert_eq!(
        receipt
            .universe
            .user_event_reservations_at_prefix_validation,
        0
    );
    assert_eq!(
        receipt.universe.semantic_allocations_at_prefix_validation,
        0
    );
    assert!(
        receipt
            .universe
            .semantic_prefix_authenticated_by_private_decode
    );
    assert!(receipt.universe.new_ids_begin_after_all_nine_prefixes);
    assert_eq!(receipt.universe.decoded_store_rows_overwritten, 0);
    assert_eq!(receipt.universe.decoded_interner_keys_overwritten, 0);
    assert!(receipt.universe.affected_replacements_are_append_only);
    assert_eq!(receipt.universe.shared_base_storage_references, 0);
    assert_eq!(receipt.universe.reachable_stale_affected_rows, 0);
    assert_eq!(
        receipt.events.user_reservation_cardinality,
        receipt.events.full_source_user_reservation_cardinality
    );
    assert_eq!(
        receipt.events.user_records_in_four_key_order,
        receipt.events.full_source_user_records_in_four_key_order
    );
    assert_eq!(receipt.events.library_events_in_user_domains, 0);
    assert_eq!(receipt.events.unvalidated_unaffected_baseline_owners, 0);
    assert_eq!(receipt.events.unmatched_replayed_library_records, 0);
    assert!(receipt.universe.private_state_dropped_after_reports);
}

#[test]
fn private_collision_build_merges_value_type_and_namespace_slots_in_one_universe() {
    let receipt = check(&[
        input(
            "/project/00_augment.ts",
            r#"interface Document { wu5Value: number; }
declare namespace Intl {
  interface WU5Options { enabled: boolean; }
}
declare function parseInt(value: "one"): 1;
"#,
        ),
        input(
            "/project/99_consume.ts",
            r#"declare const options: Intl.WU5Options;
const documentValue: number = document.wu5Value;
const wrongDocumentValue: string = document.wu5Value;
const enabled: boolean = options.enabled;
const wrongEnabled: string = options.enabled;
const one: 1 = parseInt("one");
const ordinary: number = parseInt("10");
const wrongOrdinary: string = parseInt("10");
"#,
        ),
    ]);

    assert_private_replay(&receipt);
    assert_eq!(receipt.normalized_diagnostics.len(), 3);
    assert!(
        receipt
            .normalized_diagnostics
            .iter()
            .all(|line| line.contains("TK2322")),
        "{:#?}",
        receipt.normalized_diagnostics
    );
    assert!(receipt.replay_seeds.contains("type:Document"));
    assert!(receipt.replay_seeds.contains("namespace:Intl"));
    assert!(receipt.replay_seeds.contains("value:parseInt"));
    assert_eq!(
        receipt.merged_identity.user_document_type_group,
        receipt.merged_identity.library_document_type_group
    );
    assert_eq!(
        receipt.merged_identity.user_intl_namespace,
        receipt.merged_identity.library_intl_namespace
    );
    assert_eq!(
        receipt.merged_identity.user_parse_int_storage,
        receipt.merged_identity.library_parse_int_storage
    );
    assert!(
        receipt
            .merged_identity
            .private_semantic_identities_reselected
    );
}

#[test]
fn colliding_forms_resolve_same_file_lexical_siblings_before_global_publication() {
    let receipt = check(&[input(
        "/project/colliding-lexical-siblings.ts",
        r#"interface WU5LocalType { marker: number; }
const WU5LocalValue: WU5LocalType = { marker: 1 };
class WU5LocalBase { local = WU5LocalValue; }

interface Array<T> extends WU5LocalType { local: WU5LocalType; }
function parseInt(value: WU5LocalType): WU5LocalType { return WU5LocalValue; }
class AddEventListenerOptions extends WU5LocalBase { local = WU5LocalValue; }
namespace Intl {
  export const local = WU5LocalValue;
  export interface UsesLocal extends WU5LocalType {}
}

declare const array: Array<number>;
declare const listenerOptions: AddEventListenerOptions;
const wrongArray: string = array.local.marker;
const wrongFunction: boolean = parseInt(WU5LocalValue).marker;
const wrongClass: bigint = listenerOptions.local.marker;
const wrongNamespace: symbol = Intl.local.marker;
"#,
    )]);

    assert_private_replay(&receipt);
    assert!(
        receipt
            .normalized_diagnostics
            .iter()
            .all(|line| !line.contains("TK2304")),
        "a global publication target must not replace the syntax module for lookup: {:#?}",
        receipt.normalized_diagnostics
    );
    assert_eq!(
        receipt
            .normalized_diagnostics
            .iter()
            .filter(|line| line.contains("TK2322"))
            .count(),
        4,
        "resolved sibling types must reach four observable negative assignments"
    );
    for seed in [
        "type:Array",
        "value:parseInt",
        "type:AddEventListenerOptions",
        "namespace:Intl",
    ] {
        assert!(receipt.replay_seeds.contains(seed), "{seed}");
    }
}

#[test]
fn nested_hoisted_regexp_collision_cannot_turn_native_errors_false_clean() {
    let receipt = check(&[input(
        "/project/nested-regexp-collision.ts",
        r#"declare const condition: boolean;
declare const source: { RegExp: number };
if (condition) {
  var { RegExp } = source;
}
const nativeResult: boolean = /native/.test("native");
const mustRemainAnError: string = /native/.test("native");
"#,
    )]);

    assert_private_replay(&receipt);
    assert!(receipt.replay_seeds.contains("value:RegExp"));
    assert!(receipt.replay_seeds.contains("global-object"));
    assert_eq!(receipt.normalized_diagnostics.len(), 1);
    assert!(receipt.normalized_diagnostics[0].contains("TK2322"));
}

fn array_project(reversed: bool) -> Vec<UserDeltaProjectInputForTest<'static>> {
    const AUGMENT_PATH: &str = "/project/augment.ts";
    const CONSUME_PATH: &str = "/project/consume.ts";
    const AUGMENT: &str = "interface Array<T> { wu5Collision(): T; }\n";
    const CONSUME: &str = r#"const value: number = [1, 2].wu5Collision();
const wrongValue: string = [1, 2].wu5Collision();
const mapped: number[] = [1, 2].map((value) => value + 1);
const wrongMapped: string[] = [1, 2].map((value) => value + 1);
"#;
    if reversed {
        vec![input(CONSUME_PATH, CONSUME), input(AUGMENT_PATH, AUGMENT)]
    } else {
        vec![input(AUGMENT_PATH, AUGMENT), input(CONSUME_PATH, CONSUME)]
    }
}

#[test]
fn private_combined_universe_is_order_invariant_by_normalized_source_identity() {
    let forward = check(&array_project(false));
    let reverse = check(&array_project(true));

    assert_private_replay(&forward);
    assert_private_replay(&reverse);
    assert_eq!(
        forward.normalized_semantics_by_source,
        reverse.normalized_semantics_by_source
    );
    assert_eq!(
        forward.normalized_event_and_ledger_records_by_source,
        reverse.normalized_event_and_ledger_records_by_source
    );
    assert_eq!(forward.normalized_diagnostics.len(), 2);
    assert!(forward.replay_seeds.contains("type:Array"));
    assert_eq!(
        forward.merged_identity.user_array_type_group,
        forward.merged_identity.library_array_type_group
    );
}

#[test]
fn unique_script_global_object_surface_uses_replay_not_full_source_semantics() {
    let receipt = check(&[
        input(
            "/project/00_global.ts",
            r#"declare var WU5UniqueGlobal: { count: number };
function WU5GlobalFunction(): number { return 1; }
let WU5GlobalLet = 1;
const WU5GlobalConst = 1;
class WU5GlobalClass { value = 1; }
namespace WU5GlobalNamespace { export const value = 1; }
"#,
        ),
        input(
            "/project/99_consume.ts",
            r#"const count: number = globalThis.WU5UniqueGlobal.count;
const wrongCount: string = globalThis.WU5UniqueGlobal.count;
const called: number = globalThis.WU5GlobalFunction();
const wrongCalled: string = globalThis.WU5GlobalFunction();
const namespaceValue: number = globalThis.WU5GlobalNamespace.value;
globalThis.WU5GlobalLet;
globalThis.WU5GlobalConst;
globalThis.WU5GlobalClass;
"#,
        ),
    ]);

    assert_private_replay(&receipt);
    assert!(receipt.replay_seeds.contains("global-object"));
    assert!(receipt.replay_seeds.contains("value:WU5UniqueGlobal"));
    assert!(receipt.replay_seeds.contains("value:WU5GlobalFunction"));
    assert!(receipt
        .replay_seeds
        .contains("namespace:WU5GlobalNamespace"));
    assert_eq!(receipt.work.full_source_fallbacks, 0);
}

#[test]
fn private_unavailable_merge_never_falls_back_to_the_frozen_prefix() {
    let receipt = check(&[
        input(
            "/project/00_augment.ts",
            r#"interface Array<T> {
  wu5Unsupported(value: symbol): T;
}
"#,
        ),
        input(
            "/project/99_consume.ts",
            r#"const baseMustNotSurvive: string = [1, 2].map((value) => value + 1);
[1, 2].wu5Unsupported;
"#,
        ),
    ]);

    assert_private_replay(&receipt);
    assert!(receipt
        .universe
        .affected_terminals_unavailable
        .contains("type:Array"));
    assert!(!receipt
        .universe
        .affected_terminals_from_frozen_prefix
        .contains("type:Array"));
    assert_eq!(receipt.universe.reachable_stale_affected_rows, 0);
}

#[test]
fn private_route_never_forks_or_references_the_shared_base() {
    let base = acquire();
    let shared_identity = base.storage_identity_for_test();
    let shared_projection = base
        .recompute_canonical_projection_for_test()
        .expect("shared projection before private check");
    let receipt = base
        .check_routed_user_project_against_full_source_oracle_for_test(&array_project(false))
        .expect("private Array replay");

    assert_private_replay(&receipt);
    assert_eq!(receipt.work.shared_delta_forks, 0);
    assert_eq!(receipt.universe.shared_base_storage_references, 0);
    assert_ne!(receipt.universe.private_storage_identity, shared_identity);
    assert_eq!(base.storage_identity_for_test(), shared_identity);
    assert_eq!(
        base.recompute_canonical_projection_for_test()
            .expect("shared projection after private check"),
        shared_projection
    );
}

#[test]
fn shared_external_module_matches_the_forced_private_semantic_oracle() {
    let inputs = [input(
        "/project/external.ts",
        r#"interface Array<T> { local: T; }
declare const local: Array<number>;
const value: number = local.local;
const wrong: string = local.local;
export {};
"#,
    )];
    let base = acquire();
    let shared = base
        .check_routed_user_project_for_test(&inputs)
        .expect("ordinary shared route");
    let private = base
        .check_forced_private_project_for_test(&inputs)
        .expect("forced private semantic oracle");

    assert_eq!(shared.preflight.route, CollisionRouteForTest::SharedDelta);
    assert_eq!(private.execution, PrivateExecutionForTest::SelectiveReplay);
    assert_eq!(
        shared.normalized_semantics_by_source,
        private.normalized_semantics_by_source
    );
    assert_eq!(
        shared.normalized_diagnostics,
        private.normalized_diagnostics
    );
}

#[test]
fn completed_private_run_cannot_leak_into_the_next_shared_delta() {
    let base = acquire();
    let private = base
        .check_routed_user_project_against_full_source_oracle_for_test(&array_project(false))
        .expect("private Array replay");
    assert_private_replay(&private);

    let isolation = base
        .check_routed_user_project_for_test(&[input(
            "/project/isolation.ts",
            r#"export {};
[1, 2].wu5Collision;
const mapped: number[] = [1, 2].map((value) => value + 1);
"#,
        )])
        .expect("fresh shared delta");
    assert_eq!(
        isolation.preflight.route,
        CollisionRouteForTest::SharedDelta
    );
    assert_eq!(isolation.normalized_diagnostics.len(), 1);
    assert!(isolation.normalized_diagnostics[0].contains("TK2339"));
}

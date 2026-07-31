//! Disabled RED contract for WU5 private combined-universe semantics.
//!
//! Candidate replay is always compared with the complete source-backed combined compiler. The
//! private epoch may share proven-unaffected immutable rows from the source-compiled base, while
//! every changed meaning, identity, cache, event, override, and suffix remains private.

use super::base::{
    CollisionRouteForTest, PrivateCombinedReceiptForTest, PrivateExecutionForTest,
    PrivateLifecycleIdentityFaultForTest, PrivateProductionFallbackFailureForTest,
    PrivateProductionReplayFailureForTest, PrivateProductionReplayFaultForTest,
    PrivateProductionRouteFaultForTest, PrivateReplayCandidateFailureForTest,
    PrivateReplayOwnerOmissionForTest, PrivateReplayValidationFailureForTest,
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
    assert!(receipt.work.candidate_affected_library_parse_units > 0);
    assert!(receipt.work.candidate_affected_library_parse_units < 82);
    assert_eq!(receipt.work.oracle_library_parse_units, 82);
    assert_eq!(receipt.work.oracle_library_bind_units, 82);
    assert_eq!(
        receipt.work.replayed_owner_keys,
        receipt.work.source_plan_expected_reverse_closure
    );
    assert!(receipt.universe.new_ids_begin_after_all_nine_prefixes);
    assert!(receipt.universe.shared_immutable_prefix_references > 0);
    assert!(receipt.universe.affected_replacements_are_append_only);
    assert_eq!(receipt.universe.reachable_stale_affected_rows, 0);
    assert_eq!(
        receipt.events.user_reservation_cardinality,
        receipt.events.full_source_user_reservation_cardinality
    );
    assert_eq!(
        receipt.events.user_records_in_four_key_order,
        receipt.events.full_source_user_records_in_four_key_order
    );
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

fn intl_inherited_payload_project(reversed: bool) -> Vec<UserDeltaProjectInputForTest<'static>> {
    const AUGMENT: UserDeltaProjectInputForTest<'static> = UserDeltaProjectInputForTest {
        path: "/project/00_augment.ts",
        source: r#"declare namespace Intl {
  interface NumberFormatOptions {
    b103Required: string;
  }
}
"#,
    };
    const CONSUME: UserDeltaProjectInputForTest<'static> = UserDeltaProjectInputForTest {
        path: "/project/99_consume.ts",
        source: "new Intl.NumberFormat(\"en\", {});\n",
    };
    if reversed {
        vec![CONSUME, AUGMENT]
    } else {
        vec![AUGMENT, CONSUME]
    }
}

#[test]
fn nested_namespace_inherited_payload_reaches_overload_validation_in_both_file_orders() {
    let forward = check(&intl_inherited_payload_project(false));
    let reverse = check(&intl_inherited_payload_project(true));

    for receipt in [&forward, &reverse] {
        assert_eq!(
            receipt.preflight.route,
            CollisionRouteForTest::PrivateCombined
        );
        assert_eq!(receipt.execution, PrivateExecutionForTest::SelectiveReplay);
        let oracle_rows = receipt
            .oracle
            .full_source_semantics_by_source
            .values()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            oracle_rows
                .iter()
                .filter(|row| {
                    row.contains("TK2769") && row.contains("No overload matches this call")
                })
                .count(),
            1,
            "tsc 6.0.3 rejects the missing inherited b103Required option"
        );
        assert!(
            oracle_rows
                .iter()
                .all(|row| !row.starts_with("incomplete ")),
            "the inherited payload must reach overload validation, not a silent incomplete"
        );
        assert!(
            receipt
                .normalized_semantics_by_source
                .values()
                .flatten()
                .all(|row| !row.starts_with("incomplete ")),
            "sparse replay must not hide the missing inherited member behind an incomplete"
        );
        assert_eq!(
            receipt.oracle.candidate_semantics_by_source,
            receipt.oracle.full_source_semantics_by_source,
            "sparse replay must preserve nested namespace interface payloads"
        );
        assert_eq!(
            receipt
                .normalized_diagnostics
                .iter()
                .filter(|line| {
                    line.contains("TK2769") && line.contains("No overload matches this call")
                })
                .count(),
            1
        );
    }
    assert_eq!(
        forward.normalized_semantics_by_source,
        reverse.normalized_semantics_by_source
    );
}

fn intl_same_name_function_overload_project(
    reversed: bool,
) -> Vec<UserDeltaProjectInputForTest<'static>> {
    const AUGMENT: UserDeltaProjectInputForTest<'static> = UserDeltaProjectInputForTest {
        path: "/project/00_augment.ts",
        source: r#"declare namespace Intl {
  interface B103Locale {
    b103: string;
  }
  function getCanonicalLocales(locales: B103Locale): string[];
}
"#,
    };
    const CONSUME: UserDeltaProjectInputForTest<'static> = UserDeltaProjectInputForTest {
        path: "/project/99_consume.ts",
        source: r#"const ok: string[] = Intl.getCanonicalLocales({ b103: "ok" });
const bad: string[] = Intl.getCanonicalLocales({});
"#,
    };
    if reversed {
        vec![CONSUME, AUGMENT]
    } else {
        vec![AUGMENT, CONSUME]
    }
}

#[test]
fn nested_namespace_same_name_function_overload_merges_in_both_file_orders() {
    let forward = check(&intl_same_name_function_overload_project(false));
    let reverse = check(&intl_same_name_function_overload_project(true));

    for receipt in [&forward, &reverse] {
        assert_eq!(
            receipt.preflight.route,
            CollisionRouteForTest::PrivateCombined
        );
        assert_eq!(receipt.execution, PrivateExecutionForTest::SelectiveReplay);
        let oracle_rows = receipt
            .oracle
            .full_source_semantics_by_source
            .values()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            oracle_rows
                .iter()
                .filter(|row| {
                    row.contains("TK2769") && row.contains("No overload matches this call")
                })
                .count(),
            1,
            "tsc 6.0.3 accepts the B103Locale overload and rejects only the empty object"
        );
        assert!(oracle_rows
            .iter()
            .all(|row| !row.starts_with("incomplete ")));
        assert_eq!(
            receipt.oracle.candidate_semantics_by_source,
            receipt.oracle.full_source_semantics_by_source,
            "sparse replay must merge a same-name function overload into the retained namespace"
        );
        assert_eq!(
            receipt
                .normalized_diagnostics
                .iter()
                .filter(|line| {
                    line.contains("TK2769") && line.contains("No overload matches this call")
                })
                .count(),
            1
        );
        assert!(receipt
            .normalized_semantics_by_source
            .values()
            .flatten()
            .all(|row| !row.starts_with("incomplete ")));
    }
    assert_eq!(
        forward.normalized_semantics_by_source,
        reverse.normalized_semantics_by_source
    );
}

fn window_inherited_payload_project(reversed: bool) -> Vec<UserDeltaProjectInputForTest<'static>> {
    const AUGMENT: UserDeltaProjectInputForTest<'static> = UserDeltaProjectInputForTest {
        path: "/project/00_augment.ts",
        source: "interface Window { b103Window: string; }\n",
    };
    const CONSUME: UserDeltaProjectInputForTest<'static> = UserDeltaProjectInputForTest {
        path: "/project/99_consume.ts",
        source: "const bad: number = window.b103Window;\n",
    };
    if reversed {
        vec![CONSUME, AUGMENT]
    } else {
        vec![AUGMENT, CONSUME]
    }
}

#[test]
fn window_inherited_payload_reaches_assignment_in_both_project_file_orders() {
    let forward = check(&window_inherited_payload_project(false));
    let reverse = check(&window_inherited_payload_project(true));

    for receipt in [&forward, &reverse] {
        let oracle_rows = receipt
            .oracle
            .full_source_semantics_by_source
            .values()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            oracle_rows
                .iter()
                .filter(|row| {
                    row.contains("TK2322")
                        && row.contains("Type 'string' is not assignable to type 'number'")
                })
                .count(),
            1
        );
        assert!(oracle_rows
            .iter()
            .all(|row| !row.starts_with("incomplete ")));
        assert_eq!(
            receipt.oracle.candidate_semantics_by_source,
            receipt.oracle.full_source_semantics_by_source
        );
        assert_eq!(receipt.normalized_diagnostics.len(), 1);
    }
    assert_eq!(
        forward.normalized_semantics_by_source,
        reverse.normalized_semantics_by_source
    );
}

fn webassembly_inherited_payload_project(
    reversed: bool,
) -> Vec<UserDeltaProjectInputForTest<'static>> {
    const AUGMENT: UserDeltaProjectInputForTest<'static> = UserDeltaProjectInputForTest {
        path: "/project/00_augment.ts",
        source: r#"declare namespace WebAssembly {
  interface MemoryDescriptor {
    b103Required: string;
  }
}
"#,
    };
    const CONSUME: UserDeltaProjectInputForTest<'static> = UserDeltaProjectInputForTest {
        path: "/project/99_consume.ts",
        source: "new WebAssembly.Memory({ initial: 1 });\n",
    };
    if reversed {
        vec![CONSUME, AUGMENT]
    } else {
        vec![AUGMENT, CONSUME]
    }
}

#[test]
fn webassembly_inherited_payload_reaches_constructor_argument_in_both_file_orders() {
    let forward = check(&webassembly_inherited_payload_project(false));
    let reverse = check(&webassembly_inherited_payload_project(true));

    for receipt in [&forward, &reverse] {
        let oracle_rows = receipt
            .oracle
            .full_source_semantics_by_source
            .values()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            oracle_rows
                .iter()
                .filter(|row| {
                    row.contains("TK2345")
                        && row.contains("not assignable to parameter")
                        && row.contains("b103Required")
                })
                .count(),
            1
        );
        assert!(oracle_rows
            .iter()
            .all(|row| !row.starts_with("incomplete ")));
        assert_eq!(
            receipt.oracle.candidate_semantics_by_source,
            receipt.oracle.full_source_semantics_by_source
        );
        assert_eq!(receipt.normalized_diagnostics.len(), 1);
    }
    assert_eq!(
        forward.normalized_semantics_by_source,
        reverse.normalized_semantics_by_source
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

fn production_collision_project() -> Vec<UserDeltaProjectInputForTest<'static>> {
    vec![
        input(
            "/project/00_augment.ts",
            r#"interface Array<T> {
  fullLibBenchFirst(): T;
}
"#,
        ),
        input(
            "/project/99_consume.ts",
            r#"const collisionNumber: number = [1, 2, 3].fullLibBenchFirst();
const wrongCollisionNumber: string = [1, 2, 3].fullLibBenchFirst();
const collisionMapped: number[] = [1, 2, 3].map((value) => value + 1);
const collisionDom: HTMLDivElement = document.createElement("div");

void [collisionNumber, collisionMapped, collisionDom];
"#,
        ),
    ]
}

fn relative_import_collision_project() -> Vec<UserDeltaProjectInputForTest<'static>> {
    vec![
        input(
            "/project/00_augment.ts",
            r#"interface Array<T> {
  fullLibBenchFirst(): T;
}
"#,
        ),
        input(
            "/project/value.ts",
            "export const values: number[] = [1, 2, 3];\n",
        ),
        input(
            "/project/99_consume.ts",
            r#"import { values } from "./value";
const collisionNumber: number = values.fullLibBenchFirst();
const wrongCollisionNumber: string = values.fullLibBenchFirst();
"#,
        ),
    ]
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
fn complete_source_oracle_is_independent_of_a_corrupted_sparse_schedule() {
    let inputs = production_collision_project();
    let healthy = check(&inputs);
    assert_private_replay(&healthy);

    // This typed fault seam must remove the admitted owner only after reverse closure. The
    // complete-source continuation still runs independently even when the candidate cannot.
    let adversarial = acquire()
        .check_routed_user_project_with_post_closure_owner_omission_against_full_source_oracle_for_test(
            &inputs,
            PrivateReplayOwnerOmissionForTest::RootTypeGroup {
                root_name: "Array".to_owned(),
            },
        )
        .expect("the independent complete-source oracle still finishes");
    assert!(adversarial.omission.was_in_closed_schedule);
    assert!(adversarial.omission.removed_after_closure);
    assert!(
        adversarial
            .candidate_execution
            .corrupted_schedule_installed_after_omission
    );
    assert!(adversarial.candidate_execution.started);
    assert!(
        adversarial
            .candidate_execution
            .completion_or_semantic_query_steps
            > 0,
        "the omission must reach an executing sparse candidate, not a synthetic receipt"
    );
    assert_eq!(
        adversarial.candidate_execution.omitted_owner,
        adversarial.omission.owner
    );
    assert_eq!(
        adversarial.full_source_oracle.semantics_by_source,
        healthy.oracle.full_source_semantics_by_source
    );
    assert_eq!(
        adversarial.full_source_oracle.published_root_projection,
        healthy.oracle.full_source_published_root_projection
    );
    let oracle_rows = adversarial
        .full_source_oracle
        .semantics_by_source
        .values()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(
        oracle_rows
            .iter()
            .filter(|row| row.contains("TK2322"))
            .count(),
        1,
        "tsc 6.0.3 reports only the deliberate number-to-string mismatch"
    );
    assert!(
        oracle_rows.iter().all(|row| !row.contains("TK2339")),
        "the complete-source oracle must retain the Array augmentation"
    );

    let candidate_exposes_the_omission = match &adversarial.candidate {
        Ok(candidate) => {
            candidate.semantics_by_source != adversarial.full_source_oracle.semantics_by_source
                || candidate.published_root_projection
                    != adversarial.full_source_oracle.published_root_projection
        }
        Err(failure) => matches!(
            failure,
            PrivateReplayCandidateFailureForTest::MissingScheduledOwner { owner }
                if owner == &adversarial.omission.owner
        ),
    };
    assert!(
        candidate_exposes_the_omission,
        "a corrupted candidate must diverge or fail typed; it cannot validate its own oracle"
    );
}

#[test]
fn post_bind_mutation_ledger_is_validated_before_plan_owner_intersection() {
    let receipt = acquire()
        .check_routed_user_project_with_production_replay_fault_for_test(
            &production_collision_project(),
            PrivateProductionReplayFaultForTest::InjectPostBindMutationOwnerAbsentFromPlan,
        )
        .expect("post-bind mutation fault reaches the production replay path");

    assert!(receipt.production_trace.bind_completed);
    assert!(receipt.fault.injected_after_bind);
    assert!(receipt
        .post_bind_mutation_ledger_owner_keys
        .contains(&receipt.fault.injected_owner_key));
    assert!(!receipt
        .plan_owner_keys
        .contains(&receipt.fault.injected_owner_key));
    assert!(
        receipt.production_trace.mutation_ledger_recorded
            < receipt.production_trace.containment_validation_started
            && receipt.production_trace.containment_validation_started
                < receipt.production_trace.plan_owner_intersection_started,
        "the complete mutation ledger must be validated before any plan-owner filtering"
    );
    let contained_or_expanded = match &receipt.candidate {
        Err(PrivateProductionReplayFailureForTest::MutationOwnerOutsidePlan { owner }) => {
            owner == &receipt.fault.injected_owner_key
        }
        Ok(candidate) => candidate
            .scheduled_owner_keys
            .contains(&receipt.fault.injected_owner_key),
        Err(_) => false,
    };
    assert!(
        contained_or_expanded,
        "an absent plan owner must fail containment or expand closure"
    );
}

#[test]
fn missing_expected_baseline_record_is_rejected_by_production_completion_selection() {
    let receipt = acquire()
        .check_routed_user_project_with_production_replay_fault_for_test(
            &production_collision_project(),
            PrivateProductionReplayFaultForTest::OmitExpectedBaselineRecordDuringCompletion,
        )
        .expect("baseline-record fault reaches production completion");

    assert!(receipt.production_trace.bind_completed);
    assert!(receipt.production_trace.sparse_candidate_execution_started);
    assert!(receipt.production_trace.completion_selection_started);
    assert!(receipt.fault.applied_in_completion_selection);
    assert!(matches!(
        receipt.candidate,
        Err(PrivateProductionReplayFailureForTest::BaselineValidation(
            PrivateReplayValidationFailureForTest::MissingExpected { ref fingerprint }
        )) if fingerprint == &receipt.fault.expected_baseline_fingerprint
    ));
}

#[test]
fn sealed_epoch_baseline_survives_candidate_reservation_and_activation_filtering() {
    let receipt = acquire()
        .check_routed_user_project_with_production_replay_fault_for_test(
            &production_collision_project(),
            PrivateProductionReplayFaultForTest::DisableSealedExpectedBaselineOwnerBeforeCandidateReservation,
        )
        .expect("pre-reservation baseline-owner fault reaches production replay");

    assert!(receipt.production_trace.bind_completed);
    assert!(receipt.fault.applied_before_candidate_reservation);
    assert!(
        receipt.production_trace.fault_injected
            < receipt.production_trace.candidate_reservation_started
            && receipt.production_trace.candidate_reservation_started
                < receipt.production_trace.candidate_activation_started
            && receipt.production_trace.candidate_activation_started
                < receipt.production_trace.baseline_validation_started,
        "the sealed owner must be disabled before candidate reservation and activation"
    );
    assert!(receipt
        .epoch_library_record_baseline_owner_keys
        .contains(&receipt.fault.disabled_baseline_owner_key));
    assert!(receipt
        .epoch_library_record_baseline_fingerprints
        .contains(&receipt.fault.expected_baseline_fingerprint));
    assert!(!receipt
        .candidate_reserved_library_record_owner_keys
        .contains(&receipt.fault.disabled_baseline_owner_key));
    assert!(!receipt
        .candidate_activated_library_record_owner_keys
        .contains(&receipt.fault.disabled_baseline_owner_key));
    assert!(matches!(
        receipt.candidate,
        Err(PrivateProductionReplayFailureForTest::BaselineValidation(
            PrivateReplayValidationFailureForTest::MissingExpected { ref fingerprint }
        )) if fingerprint == &receipt.fault.expected_baseline_fingerprint
    ));
}

#[test]
fn production_route_failures_fall_back_under_one_permit_with_instrumented_evidence() {
    let inputs = relative_import_collision_project();
    let healthy = check(&inputs);
    assert_private_replay(&healthy);
    let base = acquire();
    for fault in [
        PrivateProductionRouteFaultForTest::RejectPlanAdmissionAfterPrivatePreflight,
        PrivateProductionRouteFaultForTest::OmitExpectedBaselineDuringCheckerCompletion,
    ] {
        let fallback = base
            .check_routed_user_project_with_production_fault_and_fallback_for_test(
                &inputs,
                fault,
                PrivateLifecycleIdentityFaultForTest::None,
            )
            .expect("production route failure invokes complete-source fallback");

        assert_eq!(
            fallback.execution,
            PrivateExecutionForTest::CompleteSourceFallback
        );
        assert!(fallback.route_trace.preflight_classified_private);
        assert_eq!(fallback.route_trace.production_route_invocations, 1);
        assert_eq!(fallback.route_trace.relative_import_edges, 1);
        assert!(fallback.route_trace.fault_observed);
        match fault {
            PrivateProductionRouteFaultForTest::RejectPlanAdmissionAfterPrivatePreflight => {
                assert!(fallback.route_trace.plan_admission_attempted);
                assert!(!fallback.route_trace.private_runtime_fork_started);
                assert!(!fallback.route_trace.checker_started);
            }
            PrivateProductionRouteFaultForTest::OmitExpectedBaselineDuringCheckerCompletion => {
                assert!(fallback.route_trace.plan_admission_succeeded);
                assert!(fallback.route_trace.private_runtime_fork_started);
                assert!(fallback.route_trace.checker_started);
                assert!(fallback.route_trace.completion_selection_started);
                assert!(matches!(
                    fallback.sparse_failure,
                    PrivateProductionReplayFailureForTest::BaselineValidation(
                        PrivateReplayValidationFailureForTest::MissingExpected { .. }
                    )
                ));
            }
        }
        assert_eq!(
            fallback.normalized_semantics_by_source,
            fallback.full_source_oracle.semantics_by_source
        );
        assert_eq!(
            fallback.normalized_semantics_by_source,
            healthy.oracle.full_source_semantics_by_source
        );
        assert_eq!(
            fallback.published_root_projection,
            healthy.oracle.full_source_published_root_projection
        );
        let fallback_rows = fallback
            .normalized_semantics_by_source
            .values()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            fallback_rows
                .iter()
                .filter(|row| row.contains("TK2322"))
                .count(),
            1
        );
        assert!(fallback_rows
            .iter()
            .all(|row| !row.contains("TK2339") && !row.starts_with("incomplete ")));
        assert_eq!(fallback.measurement.production_route_failures, 1);
        assert_eq!(fallback.measurement.full_source_fallback_invocations, 1);
        assert_eq!(fallback.measurement.full_source_library_parse_units, 82);
        assert_eq!(fallback.measurement.full_source_library_bind_units, 82);
        assert_eq!(fallback.measurement.private_permit_acquisitions, 1);
        assert_eq!(fallback.measurement.shared_base_mutations, 0);
        assert!(
            fallback.lifecycle.permit_acquired < fallback.lifecycle.route_fault_observed
                && fallback.lifecycle.route_fault_observed
                    < fallback.lifecycle.complete_source_fallback_started
                && fallback.lifecycle.complete_source_fallback_started
                    < fallback.lifecycle.fallback_state_dropped
                && fallback.lifecycle.fallback_state_dropped < fallback.lifecycle.permit_released,
            "route failure, fallback work, and cleanup must stay inside one measured permit epoch"
        );
        assert!(fallback.identity.instrumented_production_hook_invocations > 0);
    }
}

#[test]
fn production_instrumentation_faults_change_observed_evidence_and_fail_attestation() {
    let inputs = relative_import_collision_project();
    let base = acquire();

    for fault in [
        PrivateLifecycleIdentityFaultForTest::AliasPrivateStorageToSharedBase,
        PrivateLifecycleIdentityFaultForTest::UseWrapperAddressAfterRelocation,
        PrivateLifecycleIdentityFaultForTest::UseHardCodedConstant,
        PrivateLifecycleIdentityFaultForTest::UseFabricatedSerialLifecycleToken,
        PrivateLifecycleIdentityFaultForTest::SuppressProductionHookCounterIncrement,
        PrivateLifecycleIdentityFaultForTest::ReportPrivateStateDroppedBeforeActualDrop,
    ] {
        let rejected = base
            .check_routed_user_project_with_production_fault_and_fallback_for_test(
                &inputs,
                PrivateProductionRouteFaultForTest::OmitExpectedBaselineDuringCheckerCompletion,
                fault,
            )
            .expect_err("controlled production evidence corruption must fail attestation");
        assert!(matches!(
            rejected,
            PrivateProductionFallbackFailureForTest::UntrustedProductionEvidence {
                fault: rejected_fault,
                production_hook_invocations,
                observation_before_fault,
                observation_after_fault,
                observed_change_count,
            } if rejected_fault == fault
                && production_hook_invocations > 0
                && observation_before_fault != observation_after_fault
                && observed_change_count > 0
        ));
    }
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
}

#[test]
fn private_unavailable_merge_never_falls_back_to_the_frozen_prefix() {
    let receipt = check(&[
        input(
            "/project/00_augment.ts",
            r#"interface Array<T> {
  [key: symbol]: T;
}
"#,
        ),
        input(
            "/project/99_consume.ts",
            "const baseMustNotSurvive: string = [1, 2].map((value) => value + 1);\n",
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
fn private_epoch_shares_only_immutable_unaffected_prefix_rows() {
    let base = acquire();
    let shared_identity = base.storage_identity_for_test();
    let shared_projection = base
        .recompute_canonical_projection_for_test()
        .expect("shared projection before private check");
    let receipt = base
        .check_routed_user_project_against_full_source_oracle_for_test(&array_project(false))
        .expect("private Array replay");

    assert_private_replay(&receipt);
    assert!(receipt.universe.shared_immutable_prefix_references > 0);
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

#[test]
fn classifier_false_negative_mutation_fails_closed_before_semantic_state() {
    let base = acquire();
    let before = base.storage_identity_for_test();
    let receipt = base
        .check_with_omitted_preflight_candidate_for_test(&array_project(false), "Array")
        .expect("classifier omission is contained");

    assert_private_replay(&receipt);
    assert_eq!(
        receipt.preflight.route,
        CollisionRouteForTest::PrivateCombined
    );
    assert!(receipt.preflight.false_negative_guard_fired);
    assert_eq!(receipt.preflight.semantic_ids_before_route, 0);
    assert_eq!(receipt.preflight.event_reservations_before_route, 0);
    assert_eq!(receipt.preflight.cache_entries_before_route, 0);
    assert_eq!(receipt.work.shared_delta_forks, 0);
    assert_eq!(receipt.work.full_source_fallbacks, 0);
    assert_eq!(
        receipt
            .normalized_diagnostics
            .iter()
            .filter(|line| line.contains("TK2322"))
            .count(),
        2
    );
    assert_eq!(base.storage_identity_for_test(), before);

    let isolation = base
        .check_routed_user_project_for_test(&[input(
            "/project/isolation.ts",
            "export {};\n[1, 2].wu5Collision;\n",
        )])
        .expect("fresh shared isolation project");
    assert_eq!(
        isolation.preflight.route,
        CollisionRouteForTest::SharedDelta
    );
    assert_eq!(isolation.normalized_diagnostics.len(), 1);
    assert!(isolation.normalized_diagnostics[0].contains("TK2339"));
}

#[test]
fn declare_global_routes_private_and_matches_the_complete_source_oracle() {
    let receipt = check(&[
        input(
            "/project/00_augment.ts",
            r#"export {};
declare global {
  interface RegExp { wu5GlobalTag(): string; }
}
"#,
        ),
        input(
            "/project/99_consume.ts",
            r#"export {};
const tag: string = /x/.wu5GlobalTag();
const wrongTag: number = /x/.wu5GlobalTag();
const native: boolean = /x/.test("x");
const wrongNative: string = /x/.test("x");
"#,
        ),
    ]);

    assert_private_replay(&receipt);
    assert!(receipt.replay_seeds.contains("type:RegExp"));
    assert_eq!(
        receipt
            .normalized_diagnostics
            .iter()
            .filter(|line| line.contains("TK2322"))
            .count(),
        2
    );
}

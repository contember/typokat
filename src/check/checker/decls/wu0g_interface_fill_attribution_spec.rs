//! Disabled RED acceptance contract for WU0G interface-fill attribution.
//!
//! Leader wiring after the standalone spec commit is intentionally default-off and excluded from
//! the WU0F uninstrumented control in `decls/mod.rs`:
//!
//! ```text
//! #[cfg(all(
//!     test,
//!     feature = "wu0-interface-fill-attribution",
//!     not(feature = "wu0-uninstrumented-control")
//! ))]
//! mod wu0g_interface_fill_attribution;
//! #[cfg(all(
//!     test,
//!     feature = "wu0-interface-fill-attribution",
//!     not(feature = "wu0-uninstrumented-control")
//! ))]
//! mod wu0g_interface_fill_attribution_spec;
//! ```
//!
//! Both Cargo features are empty and absent from `default`. Feature-off ordinary libtests contain
//! no WU0G field, branch, capture, or counter update. The implementation is a compile-time
//! diagnostic sidecar, never a production observer. One explicit run-local collector is owned by
//! the interface-fill pass. Each substitution owns a plain local accumulator and merges it only
//! after the whole application returns. Interface fill exposes a cumulative checkpoint only after
//! a whole heritage SCC freezes. A cooperative stop immediately after such a boundary may return
//! the completed prefix. A hard kill preserves no in-memory WU0G evidence; durable boundary output
//! would require a separately specified outer publisher and is not claimed here.
//!
//! No recursive visit performs TLS/`RefCell` access, allocation for telemetry, atomics, locks, or
//! I/O. The only cardinality state is one bounded exact application-key frequency map, keyed by
//! template `TypeId` plus the complete ordered `(TypeParamId, TypeId)` arguments. Visit, copy, and
//! interner counters are scalar fields on the local application accumulator. Candidate-B entries
//! retain the completed run's visit count so a hit can report avoided visits without estimating.
//! Exact-key count, aggregate argument count, and aggregate canonical-key bytes each have explicit
//! budgets. All additions saturate and set `saturated`; exhausting any budget also sets it. A
//! saturated or internally inconsistent report fails closed and cannot select an optimization.
//!
//! Counter meanings are deliberately narrow:
//!
//! - `R/U/Fmax`: eligible ready application requests, exact distinct keys, and greatest frequency.
//! - `E`: substitutions actually executed after the existing clean cache and Candidate-B.
//! - `V/Vmax/Dmax`: executed `Substitution::apply` entries, greatest visits in one execution, and
//!   greatest recursive apply depth. Memo hits and raw-id cycle re-entries are visits.
//! - clean/tainted are completed executed outcomes; cycle re-entry counts raw active-id encounters.
//! - Candidate-B hits and avoided visits exclude clean-cache hits and use the cached exact visit
//!   count of the completed tainted execution they replace.
//! - copied objects/properties/name bytes/signatures count snapshots cloned by substitution object
//!   arms, even if the object later proves unchanged. Name bytes are UTF-8 bytes, and signatures
//!   combine call and construct signature ids.
//! - substitution interner attempts/hits/new ids cover only interning calls made from an executed
//!   substitution. `attempts == hits + new_ids`; ambient lowering/fill interning is excluded.
//! - Relation, inference, evaluator, and overload work is intentionally absent. WU0G attributes
//!   the first finite barrier only; later-phase counters require separate evidence and a new spec.
//!
//! The closure matrix never truncates a file or source line. Its five entries are exact recursive
//! manifest closures rooted at ES5, ES2015, DOM, ES2025 without host libraries, and full. They are
//! a matrix, not a monotonic source-size ladder. If DOM is the first non-completing entry, the
//! fallback keeps the full 82-file universe parsed,
//! bound, and reserved, then fills dependency-first prefixes of whole interface-heritage SCCs.
//! Prefix targets are approximately 12.5/25/50/75/100 percent of interface-member weight; the
//! weight is the raw number of declared interface signatures across every merged fragment, never
//! inherited expanded size. The first prefix reaching each target wins. Every rung identity
//! length-frames the pinned profile identity, strategy version, target, and ordered whole-component
//! identities.
//!
//! Selection derives incremental causality and target-counter reduction from the first-to-last
//! deltas of five exact baseline/candidate ladder rows with matching rung identities; neither is a
//! caller-supplied percentage. Wall and instruction medians are likewise derived from the same five
//! structured fresh-process pairs, which share one workload and one baseline/control binary pair.

use super::super::wu0b_profile::load_strict_profile;
use super::wu0g_interface_fill_attribution::{
    build_dom_heritage_prefix_ladder_for_test, build_manifest_closure_matrix_for_test,
    evaluate_interface_fill_selection_for_test,
    full_universe_non_interface_frozen_terminal_inventory_for_test,
    measure_interface_fill_source_for_test, measure_manifest_closure_for_test,
    InterfaceFillAttributionMode, InterfaceFillAttributionReport, InterfaceFillBenchmarkPair,
    InterfaceFillBudgetKind, InterfaceFillCounterFamily, InterfaceFillLadderAttributionPoint,
    InterfaceFillLadderAttributionRow, InterfaceFillSelectionEvidence, InterfaceHeritageDependency,
    ManifestClosureKind, MAX_APPLICATION_ARGUMENTS_PER_KEY, MAX_COMPONENT_CHECKPOINTS,
    MAX_TRACKED_APPLICATION_ARGUMENTS, MAX_TRACKED_APPLICATION_KEYS,
    MAX_TRACKED_APPLICATION_KEY_BYTES,
};
use std::collections::BTreeSet;

const ATTRIBUTION_FEATURE: &str = "wu0-interface-fill-attribution";
const CONTROL_FEATURE: &str = "wu0-uninstrumented-control";
const HOT_GUARD: &str = "#[cfg(all(test, feature = \"wu0-interface-fill-attribution\", not(feature = \"wu0-uninstrumented-control\")))]";

const REPEATED_KEY: &str = r#"
interface RepeatedTemplate<T> { value: T }
interface RepeatedFirst extends RepeatedTemplate<string> {}
interface RepeatedSecond extends RepeatedTemplate<string> {}
interface RepeatedThird extends RepeatedTemplate<string> {}
"#;

const UNIQUE_KEYS: &str = r#"
interface UniqueTemplate<T> { value: T }
interface UniqueFirst extends UniqueTemplate<string> {}
interface UniqueSecond extends UniqueTemplate<number> {}
interface UniqueThird extends UniqueTemplate<boolean> {}
"#;

const DEEP_APPLY: &str = r#"
interface DeepTemplate<T> {
    value: { one: { two: { three: { four: { five: T } } } } };
}
interface DeepUse extends DeepTemplate<string> {}
"#;

const COPY_HEAVY: &str = r#"
interface CopyTemplate<T> {
    alpha: T;
    beta: T;
    gamma: { nested: T };
    delta: { nested: { leaf: T } };
    method(value: T): T;
    (value: T): T;
    new (value: T): { made: T };
}
interface CopyString extends CopyTemplate<string> {}
interface CopyNumber extends CopyTemplate<number> {}
"#;

const CYCLE_TAINTED: &str = r#"
type Cycle = { self: Cycle };
interface CycleTemplate<T> { cycle: Cycle; value: T }
interface CycleFirst extends CycleTemplate<string> {}
interface CycleSecond extends CycleTemplate<string> {}
"#;

fn measured(
    source: &str,
    mode: InterfaceFillAttributionMode,
) -> super::wu0g_interface_fill_attribution::MeasuredInterfaceFill {
    let run = measure_interface_fill_source_for_test(source, mode)
        .expect("the tiny interface-fill witness completes");
    assert_report_arithmetic(&run.attribution);
    assert!(!run.attribution.saturated, "tiny witness cannot saturate");
    assert!(run.attribution.is_complete, "tiny witness must finish");
    assert_eq!(
        run.component_checkpoints
            .last()
            .map(|checkpoint| &checkpoint.cumulative),
        Some(&run.attribution),
        "the final whole-component checkpoint is the completed run report"
    );
    run
}

fn assert_report_arithmetic(report: &InterfaceFillAttributionReport) {
    assert_eq!(
        report.executed_substitutions,
        report.cycle_clean_outcomes + report.cycle_tainted_outcomes
    );
    assert_eq!(
        report.application_requests,
        report.clean_cache_hits + report.candidate_b_hits + report.executed_substitutions
    );
    assert_eq!(
        report.application_frequency_sum_for_test(),
        report.application_requests
    );
    assert_eq!(
        report.application_frequency_entry_count_for_test(),
        report.unique_application_keys
    );
    assert_eq!(
        report.substitution_interner_attempts,
        report.substitution_interner_hits + report.substitution_interner_new_ids
    );
    assert!(report.unique_application_keys <= report.application_requests);
    assert_eq!(
        report.application_requests == 0,
        report.unique_application_keys == 0
    );
    assert_eq!(
        report.application_requests == 0,
        report.max_application_frequency == 0
    );
    assert!(report.max_application_frequency <= report.application_requests);
    assert!(report.clean_cache_hits <= report.application_requests);
    assert!(report.candidate_b_hits <= report.application_requests);
    assert!(report.executed_substitutions <= report.application_requests);
    assert_eq!(
        report.executed_substitutions == 0,
        report.substitution_visits == 0
    );
    assert_eq!(
        report.executed_substitutions == 0,
        report.max_visits_per_substitution == 0
    );
    assert_eq!(
        report.executed_substitutions == 0,
        report.max_apply_depth == 0
    );
    assert!(report.substitution_visits >= report.executed_substitutions);
    assert!(report.max_visits_per_substitution <= report.substitution_visits);
    assert!(report.max_apply_depth <= report.max_visits_per_substitution);
    assert!(report.cycle_reentries <= report.substitution_visits);
    if report.candidate_b_hits == 0 {
        assert_eq!(report.candidate_b_avoided_visits, 0);
    }
    assert_eq!(report.validate_exact(), Ok(()));
}

fn assert_cardinality(report: &InterfaceFillAttributionReport, r: u64, u: u64, fmax: u64) {
    assert_eq!(
        (
            report.application_requests,
            report.unique_application_keys,
            report.max_application_frequency,
        ),
        (r, u, fmax)
    );
}

fn is_lower_hex_identity(identity: &str) -> bool {
    identity.len() == 64
        && identity
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn compact(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn attribution_is_explicit_bounded_and_default_absent() {
    assert!(MAX_TRACKED_APPLICATION_KEYS >= 82);
    assert!(MAX_TRACKED_APPLICATION_KEYS <= 1_048_576);
    assert!(MAX_APPLICATION_ARGUMENTS_PER_KEY > 0);
    assert!(MAX_APPLICATION_ARGUMENTS_PER_KEY <= 65_536);
    assert!(MAX_TRACKED_APPLICATION_ARGUMENTS >= MAX_TRACKED_APPLICATION_KEYS);
    assert!(MAX_TRACKED_APPLICATION_ARGUMENTS <= 16_777_216);
    assert!(MAX_TRACKED_APPLICATION_KEY_BYTES >= MAX_TRACKED_APPLICATION_ARGUMENTS);
    assert!(MAX_TRACKED_APPLICATION_KEY_BYTES <= 256 * 1024 * 1024);
    assert!(MAX_COMPONENT_CHECKPOINTS >= 82);
    assert!(MAX_COMPONENT_CHECKPOINTS <= 1_048_576);

    let manifest: toml::Value =
        toml::from_str(include_str!("../../../../Cargo.toml")).expect("Cargo manifest");
    let features = manifest["features"].as_table().expect("features table");
    for feature in [ATTRIBUTION_FEATURE, CONTROL_FEATURE] {
        assert!(
            features[feature]
                .as_array()
                .expect("diagnostic feature array")
                .is_empty(),
            "diagnostic compile-time features remain empty"
        );
        assert!(!features["default"]
            .as_array()
            .expect("default feature array")
            .iter()
            .any(|entry| entry.as_str() == Some(feature)));
    }

    let decls = include_str!("mod.rs");
    let sidecar = include_str!("wu0g_interface_fill_attribution.rs");
    let substitution = include_str!("../../../types/substitute/mod.rs");
    let apply_arms = include_str!("../../../types/substitute/apply.rs");
    for forbidden in [
        "thread_local!",
        "RefCell<",
        "std::cell::RefCell",
        "std::sync::atomic",
        "Mutex<",
        "RwLock<",
    ] {
        assert!(
            !sidecar.contains(forbidden),
            "WU0G run-local attribution must not contain {forbidden}"
        );
    }
    for forbidden in [
        "println!",
        "eprintln!",
        "std::fs::File",
        "File::create(",
        "OpenOptions::new(",
        "write!(",
        "writeln!(",
    ] {
        assert!(
            !sidecar.contains(forbidden),
            "WU0G sidecar must return boundary snapshots instead of recursive I/O: {forbidden}"
        );
    }
    assert!(sidecar.contains("MAX_TRACKED_APPLICATION_KEYS"));
    assert!(sidecar.contains("MAX_TRACKED_APPLICATION_ARGUMENTS"));
    assert!(sidecar.contains("MAX_TRACKED_APPLICATION_KEY_BYTES"));
    assert!(sidecar.contains("checked_add"));
    assert!(sidecar.contains("saturated"));
    let compact_guard = compact(HOT_GUARD);
    let compact_decls = compact(decls);
    assert!(compact_decls.contains(&format!(
        "{compact_guard}modwu0g_interface_fill_attribution;"
    )));
    assert!(compact_decls.contains(&format!(
        "{compact_guard}modwu0g_interface_fill_attribution_spec;"
    )));

    let recursive_window = substitution
        .split_once("pub fn apply(&mut self")
        .expect("Substitution::apply")
        .1
        .split_once("fn canonical_blocked_context")
        .expect("end of apply")
        .0;
    for forbidden in [
        "thread_local",
        "RefCell",
        "println!",
        "eprintln!",
        "write!(",
        "writeln!(",
    ] {
        assert!(
            !recursive_window.contains(forbidden),
            "recursive apply cannot publish WU0G telemetry through {forbidden}"
        );
    }
    for recursive_source in [recursive_window, apply_arms] {
        for forbidden in [
            "thread_local!",
            "RefCell<",
            "std::cell::RefCell",
            "std::sync::atomic",
            "Mutex<",
            "RwLock<",
            "println!",
            "eprintln!",
            "std::fs::",
            "write!(",
            "writeln!(",
        ] {
            assert!(
                !recursive_source.contains(forbidden),
                "recursive WU0G apply arms/helpers cannot publish through {forbidden}"
            );
        }
    }

    for hot_source in [
        include_str!("../context.rs"),
        include_str!("../mod.rs"),
        include_str!("mod.rs"),
        include_str!("resolve.rs"),
        substitution,
        apply_arms,
    ] {
        let lines = hot_source.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//")
                || !(code.contains("wu0g_attribution")
                    || code.contains("wu0g_application")
                    || code.contains("wu0g_record_"))
            {
                continue;
            }
            let guard_window = compact(&lines[index.saturating_sub(6)..index].join("\n"));
            assert!(
                guard_window.contains(&compact_guard),
                "every WU0G hot hook is absent from ordinary and WU0F-control libtests: {code}"
            );
        }
    }
}

#[test]
fn repeated_key_separates_request_cardinality_from_execution_count() {
    let baseline = measured(REPEATED_KEY, InterfaceFillAttributionMode::Baseline);
    assert_cardinality(&baseline.attribution, 3, 1, 3);
    assert_eq!(baseline.attribution.clean_cache_hits, 2);
    assert_eq!(baseline.attribution.executed_substitutions, 1);
    assert_eq!(baseline.attribution.cycle_clean_outcomes, 1);
    assert_eq!(baseline.attribution.cycle_tainted_outcomes, 0);
    assert_eq!(baseline.attribution.candidate_b_hits, 0);

    let candidate = measured(REPEATED_KEY, InterfaceFillAttributionMode::CandidateB);
    assert_eq!(candidate.semantic_sha256, baseline.semantic_sha256);
    assert_eq!(candidate.attribution.application_requests, 3);
    assert_eq!(candidate.attribution.unique_application_keys, 1);
    assert_eq!(candidate.attribution.max_application_frequency, 3);
    assert_eq!(candidate.attribution.clean_cache_hits, 2);
    assert_eq!(candidate.attribution.executed_substitutions, 1);
    assert_eq!(candidate.attribution.candidate_b_hits, 0);
    assert_eq!(candidate.attribution.candidate_b_avoided_visits, 0);
}

#[test]
fn unique_keys_cannot_masquerade_as_repeated_work() {
    let run = measured(UNIQUE_KEYS, InterfaceFillAttributionMode::Baseline);
    assert_cardinality(&run.attribution, 3, 3, 1);
    assert_eq!(run.attribution.clean_cache_hits, 0);
    assert_eq!(run.attribution.executed_substitutions, 3);
    assert_eq!(run.attribution.cycle_clean_outcomes, 3);
    assert_eq!(run.attribution.cycle_tainted_outcomes, 0);
    assert_eq!(run.attribution.candidate_b_hits, 0);
}

#[test]
fn deep_apply_reports_visits_per_execution_and_real_stack_depth() {
    let run = measured(DEEP_APPLY, InterfaceFillAttributionMode::Baseline);
    assert_cardinality(&run.attribution, 1, 1, 1);
    assert_eq!(run.attribution.clean_cache_hits, 0);
    assert_eq!(run.attribution.executed_substitutions, 1);
    assert_eq!(
        run.attribution.substitution_visits,
        run.attribution.max_visits_per_substitution
    );
    assert!(run.attribution.substitution_visits >= 7);
    assert!(run.attribution.max_apply_depth >= 7);
    assert_eq!(run.attribution.cycle_reentries, 0);
}

#[test]
fn copy_heavy_witness_attributes_copy_and_substitution_only_interning_volume() {
    let run = measured(COPY_HEAVY, InterfaceFillAttributionMode::Baseline);
    assert_cardinality(&run.attribution, 2, 2, 1);
    assert_eq!(run.attribution.clean_cache_hits, 0);
    assert_eq!(run.attribution.executed_substitutions, 2);
    assert!(run.attribution.copied_objects >= 8);
    assert!(run.attribution.copied_properties >= 14);
    assert!(run.attribution.copied_property_name_bytes >= 60);
    assert!(run.attribution.copied_signatures >= 4);
    assert!(run.attribution.substitution_interner_attempts >= 8);
    assert!(run.attribution.substitution_interner_new_ids > 0);
    assert_eq!(
        run.attribution.substitution_interner_attempts,
        run.attribution.substitution_interner_hits + run.attribution.substitution_interner_new_ids
    );
}

#[test]
fn cycle_tainted_repeated_key_counts_candidate_b_hits_and_exact_avoided_visits() {
    let baseline = measured(CYCLE_TAINTED, InterfaceFillAttributionMode::Baseline);
    assert_cardinality(&baseline.attribution, 2, 1, 2);
    assert_eq!(baseline.attribution.clean_cache_hits, 0);
    assert_eq!(baseline.attribution.executed_substitutions, 2);
    assert_eq!(baseline.attribution.cycle_clean_outcomes, 0);
    assert_eq!(baseline.attribution.cycle_tainted_outcomes, 2);
    assert_eq!(baseline.attribution.cycle_reentries, 2);
    assert_eq!(baseline.attribution.candidate_b_hits, 0);

    let candidate = measured(CYCLE_TAINTED, InterfaceFillAttributionMode::CandidateB);
    assert_eq!(candidate.semantic_sha256, baseline.semantic_sha256);
    assert_cardinality(&candidate.attribution, 2, 1, 2);
    assert_eq!(candidate.attribution.clean_cache_hits, 0);
    assert_eq!(candidate.attribution.candidate_b_hits, 1);
    assert_eq!(candidate.attribution.executed_substitutions, 1);
    assert_eq!(candidate.attribution.cycle_tainted_outcomes, 1);
    assert!(candidate.attribution.candidate_b_avoided_visits > 0);
    assert_eq!(
        candidate.attribution.application_histogram_sha256,
        baseline.attribution.application_histogram_sha256,
        "the exact request histogram is identical before avoided visits are compared"
    );
    assert_eq!(
        candidate.attribution.application_order_sha256,
        baseline.attribution.application_order_sha256,
        "request order is identical before avoided visits are compared"
    );
    assert_eq!(
        candidate.attribution.frozen_state_sha256, baseline.attribution.frozen_state_sha256,
        "the application frozen-state sequence is identical before avoided visits are compared"
    );
    assert_eq!(
        baseline.attribution.substitution_visits - candidate.attribution.substitution_visits,
        candidate.attribution.candidate_b_avoided_visits,
        "avoided visits are retained exact completed-run counts, not an estimate"
    );
}

#[test]
fn every_counter_family_and_exact_key_budget_saturates_and_fails_closed() {
    for family in InterfaceFillCounterFamily::ALL {
        let mut report = InterfaceFillAttributionReport::maximal_consistent_for_test();
        report.force_counter_family_overflow_for_test(family);
        assert!(report.saturated, "counter family {family:?}");
        assert!(
            report.validate_exact().is_err(),
            "counter family {family:?}"
        );
    }
    for budget in InterfaceFillBudgetKind::ALL {
        let report = InterfaceFillAttributionReport::exhausted_budget_for_test(budget);
        assert!(report.saturated, "exact-key budget {budget:?}");
        assert!(
            report.validate_exact().is_err(),
            "exact-key budget {budget:?}"
        );
    }
}

#[test]
fn cooperative_component_stop_returns_only_completed_boundary_state() {
    let partial =
        super::wu0g_interface_fill_attribution::measure_interface_fill_until_boundary_for_test(
            CYCLE_TAINTED,
            InterfaceFillAttributionMode::Baseline,
            1,
        )
        .expect("stop after one completed component");
    assert!(partial.cooperatively_stopped);
    assert!(!partial.attribution.is_complete);
    assert_eq!(partial.component_checkpoints.len(), 1);
    assert_eq!(
        partial.component_checkpoints[0].cumulative,
        partial.attribution
    );
    assert_eq!(partial.attribution.completed_components, 1);
    assert_eq!(partial.attribution.in_flight_applications, 0);
    assert_eq!(partial.attribution.in_flight_components, 0);
    assert_report_arithmetic(&partial.attribution);
}

#[test]
fn manifest_closure_matrix_is_exact_host_separated_and_stably_identified() {
    let profile = load_strict_profile().expect("pinned TypeScript profile");
    let first = build_manifest_closure_matrix_for_test(&profile).expect("manifest closure matrix");
    let second = build_manifest_closure_matrix_for_test(&profile).expect("deterministic rebuild");
    assert_eq!(first, second);
    assert_eq!(first.len(), 5);

    let expected = [
        (ManifestClosureKind::Es5, "lib.es5.d.ts", 3_usize),
        (ManifestClosureKind::Es2015, "lib.es2015.d.ts", 13),
        (ManifestClosureKind::Dom, "lib.dom.d.ts", 15),
        (ManifestClosureKind::Es2025NoHost, "lib.es2025.d.ts", 76),
        (ManifestClosureKind::Full, "lib.es2025.full.d.ts", 82),
    ];
    for (rung, (kind, root, count)) in first.iter().zip(expected) {
        assert_eq!(rung.kind, kind);
        assert_eq!(rung.root_name, root);
        assert_eq!(rung.source_names.len(), count);
        assert!(rung.source_names.iter().any(|name| name == root));
        assert!(is_lower_hex_identity(&rung.identity));
        assert!(rung.is_dependency_closed);
        assert!(!rung.uses_source_truncation);
    }

    let dom = &first[2].source_names;
    assert!(dom.iter().any(|name| name == "lib.es5.d.ts"));
    assert!(dom.iter().any(|name| name == "lib.es2015.d.ts"));
    assert!(dom
        .iter()
        .any(|name| name == "lib.es2018.asynciterable.d.ts"));
    let es2025 = &first[3].source_names;
    for forbidden_prefix in ["lib.dom", "lib.webworker", "lib.scripthost"] {
        assert!(
            !es2025.iter().any(|name| name.starts_with(forbidden_prefix)),
            "host-free ES2025 contains {forbidden_prefix}"
        );
    }
}

#[test]
fn dom_fallback_keeps_full_universe_and_uses_minimal_dependency_first_whole_scc_prefixes() {
    let profile = load_strict_profile().expect("pinned TypeScript profile");
    let ladder =
        build_dom_heritage_prefix_ladder_for_test(&profile).expect("whole heritage-SCC ladder");
    let rebuilt =
        build_dom_heritage_prefix_ladder_for_test(&profile).expect("deterministic SCC rebuild");
    assert_eq!(ladder, rebuilt);
    assert_eq!(
        ladder
            .iter()
            .map(|rung| rung.target_basis_points)
            .collect::<Vec<_>>(),
        [1_250, 2_500, 5_000, 7_500, 10_000]
    );
    assert!(!ladder.is_empty());
    assert!(
        ladder.windows(2).all(|pair| {
            pair[0].identity != pair[1].identity
                && pair[0].selected_component_count < pair[1].selected_component_count
        }),
        "all five targets must resolve to distinct whole-SCC prefixes or planning fails closed"
    );
    let universe_identity = &ladder[0].universe_identity;
    let component_order = &ladder[0].dependency_first_components;
    let expected_non_interface_inventory =
        full_universe_non_interface_frozen_terminal_inventory_for_test(&profile)
            .expect("closed full-universe class/alias terminal inventory");
    assert_eq!(ladder[0].full_universe_source_names.len(), 82);
    assert!(is_lower_hex_identity(universe_identity));
    assert!(!component_order.is_empty());
    assert!(!expected_non_interface_inventory.is_empty());
    assert!(expected_non_interface_inventory
        .keys()
        .all(|identity| is_lower_hex_identity(identity)));
    assert_eq!(
        ladder[0].known_non_interface_frozen_terminals, expected_non_interface_inventory,
        "the SCC plan carries the closed full-universe frozen class/alias inventory"
    );

    let all_ids = component_order
        .iter()
        .map(|component| component.identity.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(all_ids.len(), component_order.len());
    let mut seen = BTreeSet::new();
    for component in component_order {
        assert!(is_lower_hex_identity(&component.identity));
        for dependency in &component.heritage_dependencies {
            match dependency {
                InterfaceHeritageDependency::InterfaceComponent(identity) => assert!(
                    seen.contains(identity),
                    "every known interface dependency precedes the dependent SCC"
                ),
                InterfaceHeritageDependency::NonInterfaceTerminal(identity) => {
                    assert!(is_lower_hex_identity(identity));
                    assert!(!all_ids.contains(identity));
                    assert!(
                        expected_non_interface_inventory.contains_key(identity),
                        "a non-interface heritage terminal must belong to the closed inventory"
                    );
                }
            }
        }
        seen.insert(component.identity.clone());
    }

    let total_weight = component_order
        .iter()
        .try_fold(0_u128, |sum, component| {
            sum.checked_add(u128::from(component.interface_member_weight))
        })
        .expect("total interface-member weight fits u128");
    assert!(total_weight > 0);
    let mut previous_count = 0;
    for rung in &ladder {
        assert_eq!(&rung.universe_identity, universe_identity);
        assert_eq!(rung.full_universe_source_names.len(), 82);
        assert_eq!(&rung.dependency_first_components, component_order);
        assert_eq!(
            rung.known_non_interface_frozen_terminals,
            expected_non_interface_inventory
        );
        assert!(rung.selected_component_count >= previous_count);
        assert!(rung.selected_component_count <= component_order.len());
        assert!(is_lower_hex_identity(&rung.identity));
        assert!(!rung.uses_file_or_line_truncation);

        let selected_weight = component_order[..rung.selected_component_count]
            .iter()
            .try_fold(0_u128, |sum, component| {
                sum.checked_add(u128::from(component.interface_member_weight))
            })
            .expect("selected interface-member weight fits u128");
        assert_eq!(u128::from(rung.selected_member_weight), selected_weight);
        assert_eq!(u128::from(rung.total_member_weight), total_weight);
        let selected_scaled = selected_weight
            .checked_mul(10_000)
            .expect("selected weighted percentage fits u128");
        let target_scaled = total_weight
            .checked_mul(u128::from(rung.target_basis_points))
            .expect("target weighted percentage fits u128");
        assert!(selected_scaled >= target_scaled);
        if rung.selected_component_count > 0 {
            let prior_weight = component_order[..rung.selected_component_count - 1]
                .iter()
                .try_fold(0_u128, |sum, component| {
                    sum.checked_add(u128::from(component.interface_member_weight))
                })
                .expect("prior interface-member weight fits u128");
            assert!(
                prior_weight
                    .checked_mul(10_000)
                    .expect("prior weighted percentage fits u128")
                    < target_scaled,
                "the first whole-SCC prefix reaching the target must win"
            );
        }
        previous_count = rung.selected_component_count;
    }
    let full = ladder.last().expect("100% SCC rung");
    assert_eq!(full.selected_component_count, component_order.len());
    assert_eq!(u128::from(full.selected_member_weight), total_weight);
}

#[test]
fn completed_rung_measurement_preserves_exact_semantics() {
    let profile = load_strict_profile().expect("pinned TypeScript profile");
    let matrix = build_manifest_closure_matrix_for_test(&profile).expect("manifest closure matrix");
    let es5 = &matrix[0];
    let baseline = measure_manifest_closure_for_test(es5, InterfaceFillAttributionMode::Baseline)
        .expect("ES5 baseline completes");
    let candidate =
        measure_manifest_closure_for_test(es5, InterfaceFillAttributionMode::CandidateB)
            .expect("ES5 Candidate-B completes");
    assert_eq!(baseline.semantic_sha256, candidate.semantic_sha256);
    assert!(baseline.attribution.is_complete && candidate.attribution.is_complete);
    assert!(!baseline.attribution.saturated && !candidate.attribution.saturated);
    assert_report_arithmetic(&baseline.attribution);
    assert_report_arithmetic(&candidate.attribution);
}

fn attribution_point(
    rung: char,
    target_counter: u64,
    total_work_counter: u64,
) -> InterfaceFillLadderAttributionPoint {
    InterfaceFillLadderAttributionPoint {
        rung_identity: rung.to_string().repeat(64),
        semantic_sha256: rung.to_ascii_uppercase().to_string().repeat(64),
        target_counter,
        total_work_counter,
        complete: true,
        saturated: false,
        arithmetic_valid: true,
    }
}

fn passing_ladder_rows() -> Vec<InterfaceFillLadderAttributionRow> {
    [
        ('1', 100, 200, 100, 200),
        ('2', 2_600, 5_200, 1_350, 3_000),
        ('3', 5_100, 10_200, 2_600, 5_500),
        ('4', 7_600, 15_200, 3_850, 8_000),
        ('5', 10_100, 20_200, 5_100, 11_000),
    ]
    .into_iter()
    .map(
        |(rung, baseline_target, baseline_total, candidate_target, candidate_total)| {
            InterfaceFillLadderAttributionRow {
                baseline: attribution_point(rung, baseline_target, baseline_total),
                candidate: attribution_point(rung, candidate_target, candidate_total),
            }
        },
    )
    .collect()
}

fn benchmark_pair(ordinal: u8) -> InterfaceFillBenchmarkPair {
    InterfaceFillBenchmarkPair {
        pair_identity: format!("{ordinal:x}").repeat(64),
        workload_identity: "a".repeat(64),
        baseline_binary_identity: "b".repeat(64),
        control_binary_identity: "c".repeat(64),
        baseline_semantic_sha256: "d".repeat(64),
        control_semantic_sha256: "d".repeat(64),
        baseline_complete: true,
        control_complete: true,
        baseline_wall_ns: 1_000,
        control_wall_ns: 800,
        baseline_instructions: 1_000_000,
        control_instructions: 800_000,
        baseline_max_rss_bytes: 512 * 1024 * 1024,
        control_max_rss_bytes: 512 * 1024 * 1024,
        baseline_oom_events: 0,
        control_oom_events: 0,
        baseline_containment_failures: 0,
        control_containment_failures: 0,
    }
}

fn passing_selection() -> InterfaceFillSelectionEvidence {
    InterfaceFillSelectionEvidence {
        ladder_attribution_rows: passing_ladder_rows(),
        repeated_profile_explained_cycle_basis_points: vec![3_000, 3_400, 3_200],
        predicted_end_to_end_improvement_basis_points: 2_000,
        interleaved_fresh_pairs: (1..=5).map(benchmark_pair).collect(),
        control_regressions_basis_points: vec![0, 100, 200],
    }
}

fn assert_selection_rejected(label: &str, evidence: &InterfaceFillSelectionEvidence) {
    assert!(
        !evaluate_interface_fill_selection_for_test(evidence).authorizes(),
        "selection must fail closed for {label}"
    );
}

#[test]
fn selection_gate_accepts_every_threshold_at_its_inclusive_boundary() {
    let decision = evaluate_interface_fill_selection_for_test(&passing_selection());
    assert_eq!(decision.derived_incremental_causality_basis_points(), 5_000);
    assert_eq!(
        decision.derived_target_counter_reduction_basis_points(),
        5_000
    );
    assert_eq!(
        decision.derived_median_wall_improvement_basis_points(),
        2_000
    );
    assert_eq!(
        decision.derived_median_instruction_improvement_basis_points(),
        2_000
    );
    assert!(decision.authorizes());
}

#[test]
fn selection_gate_rejects_each_causal_and_predicted_axis_independently() {
    let mut wrong_rung_identity = passing_selection();
    wrong_rung_identity.ladder_attribution_rows[2]
        .candidate
        .rung_identity = "f".repeat(64);
    assert_selection_rejected(
        "baseline/candidate rung identity mismatch",
        &wrong_rung_identity,
    );

    let mut duplicate_rung = passing_selection();
    duplicate_rung.ladder_attribution_rows[2] = duplicate_rung.ladder_attribution_rows[1].clone();
    assert_selection_rejected("duplicate ladder rung", &duplicate_rung);

    let mut wrong_rung_count = passing_selection();
    wrong_rung_count.ladder_attribution_rows.pop();
    assert_selection_rejected("ladder row cardinality", &wrong_rung_count);

    let mut weak_causality_from_counters = passing_selection();
    weak_causality_from_counters
        .ladder_attribution_rows
        .last_mut()
        .expect("last ladder row")
        .baseline
        .target_counter = 10_098;
    weak_causality_from_counters
        .ladder_attribution_rows
        .last_mut()
        .expect("last ladder row")
        .candidate
        .target_counter = 5_099;
    let weak_causality_decision =
        evaluate_interface_fill_selection_for_test(&weak_causality_from_counters);
    assert_eq!(
        weak_causality_decision.derived_incremental_causality_basis_points(),
        4_999
    );
    assert_eq!(
        weak_causality_decision.derived_target_counter_reduction_basis_points(),
        5_000
    );
    assert_selection_rejected(
        "derived incremental causality from explicit counter delta",
        &weak_causality_from_counters,
    );

    let mut weak_reduction_from_counters = passing_selection();
    weak_reduction_from_counters
        .ladder_attribution_rows
        .last_mut()
        .expect("last ladder row")
        .candidate
        .target_counter = 5_101;
    let weak_reduction_decision =
        evaluate_interface_fill_selection_for_test(&weak_reduction_from_counters);
    assert_eq!(
        weak_reduction_decision.derived_incremental_causality_basis_points(),
        5_000
    );
    assert_eq!(
        weak_reduction_decision.derived_target_counter_reduction_basis_points(),
        4_999
    );
    assert_selection_rejected(
        "derived target-counter reduction from explicit counter delta",
        &weak_reduction_from_counters,
    );

    let mut weak_share = passing_selection();
    weak_share.repeated_profile_explained_cycle_basis_points[1] = 2_999;
    assert_selection_rejected("one repeated profile share", &weak_share);

    let mut wrong_profile_count = passing_selection();
    wrong_profile_count
        .repeated_profile_explained_cycle_basis_points
        .pop();
    assert_selection_rejected("repeated profile cardinality", &wrong_profile_count);

    let mut weak_prediction = passing_selection();
    weak_prediction.predicted_end_to_end_improvement_basis_points = 1_999;
    assert_selection_rejected("predicted end-to-end improvement", &weak_prediction);
}

#[test]
fn selection_gate_rejects_each_paired_and_control_axis_independently() {
    let mut weak_wall = passing_selection();
    for pair in &mut weak_wall.interleaved_fresh_pairs[..3] {
        pair.control_wall_ns = 801;
    }
    assert_selection_rejected("paired wall median", &weak_wall);

    let mut weak_instructions = passing_selection();
    for pair in &mut weak_instructions.interleaved_fresh_pairs[..3] {
        pair.control_instructions = 801_000;
    }
    assert_selection_rejected("paired instruction median", &weak_instructions);

    let mut wrong_pair_count = passing_selection();
    wrong_pair_count.interleaved_fresh_pairs.pop();
    assert_selection_rejected("exactly five shared metric pairs", &wrong_pair_count);

    let mut duplicate_pair = passing_selection();
    duplicate_pair.interleaved_fresh_pairs[2].pair_identity = duplicate_pair
        .interleaved_fresh_pairs[1]
        .pair_identity
        .clone();
    assert_selection_rejected("unique pair identity", &duplicate_pair);

    let mut mismatched_workload = passing_selection();
    mismatched_workload.interleaved_fresh_pairs[2].workload_identity = "e".repeat(64);
    assert_selection_rejected("cherry-picked workload identity", &mismatched_workload);

    let mut mismatched_baseline = passing_selection();
    mismatched_baseline.interleaved_fresh_pairs[2].baseline_binary_identity = "e".repeat(64);
    assert_selection_rejected("cherry-picked baseline binary", &mismatched_baseline);

    let mut mismatched_control = passing_selection();
    mismatched_control.interleaved_fresh_pairs[2].control_binary_identity = "e".repeat(64);
    assert_selection_rejected("cherry-picked control binary", &mismatched_control);

    let mut regressed_control = passing_selection();
    regressed_control.control_regressions_basis_points[1] = 201;
    assert_selection_rejected("control regression", &regressed_control);

    let mut no_controls = passing_selection();
    no_controls.control_regressions_basis_points.clear();
    assert_selection_rejected("control cardinality", &no_controls);
}

#[test]
fn selection_gate_rejects_each_semantic_resource_and_evidence_axis_independently() {
    let mut ladder_semantic = passing_selection();
    ladder_semantic.ladder_attribution_rows[0]
        .candidate
        .semantic_sha256 = "e".repeat(64);
    assert_selection_rejected("ladder semantic parity", &ladder_semantic);

    let mut semantic = passing_selection();
    semantic.interleaved_fresh_pairs[0].control_semantic_sha256 = "e".repeat(64);
    assert_selection_rejected("semantic parity", &semantic);

    let mut rss = passing_selection();
    rss.interleaved_fresh_pairs[0].control_max_rss_bytes = 512 * 1024 * 1024 + 1;
    assert_selection_rejected("512 MiB RSS ceiling", &rss);

    let mut oom = passing_selection();
    oom.interleaved_fresh_pairs[0].control_oom_events = 1;
    assert_selection_rejected("OOM evidence", &oom);

    let mut containment = passing_selection();
    containment.interleaved_fresh_pairs[0].control_containment_failures = 1;
    assert_selection_rejected("containment failure", &containment);

    let mut incomplete = passing_selection();
    incomplete.interleaved_fresh_pairs[0].control_complete = false;
    assert_selection_rejected("incomplete evidence", &incomplete);

    let mut saturated = passing_selection();
    saturated.ladder_attribution_rows[0].candidate.saturated = true;
    assert_selection_rejected("saturated attribution", &saturated);

    let mut invalid_arithmetic = passing_selection();
    invalid_arithmetic.ladder_attribution_rows[0]
        .candidate
        .arithmetic_valid = false;
    assert_selection_rejected("invalid attribution arithmetic", &invalid_arithmetic);

    let mut incomplete_ladder = passing_selection();
    incomplete_ladder.ladder_attribution_rows[0]
        .candidate
        .complete = false;
    assert_selection_rejected("incomplete ladder attribution", &incomplete_ladder);
}

#[test]
fn first_wu0e_candidate_b_observation_cannot_pass_the_selection_gate() {
    let evidence = InterfaceFillSelectionEvidence {
        ladder_attribution_rows: Vec::new(),
        repeated_profile_explained_cycle_basis_points: vec![1_946, 1_749, 1_542],
        predicted_end_to_end_improvement_basis_points: 0,
        interleaved_fresh_pairs: Vec::new(),
        control_regressions_basis_points: Vec::new(),
    };
    assert!(!evaluate_interface_fill_selection_for_test(&evidence).authorizes());
}

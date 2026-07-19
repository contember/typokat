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
//! diagnostic sidecar, never a production observer. One explicit run-local collector is installed
//! immediately before each selected interface-SCC phase, after the actual preceding pipeline work
//! (type-parameter metadata for the initial phase; real publication work for the pending retry).
//! The observer never prefills objects/classes or clears the production eager cache. Work while the
//! observer is suspended cannot contribute attribution state, counters, or digests. Each
//! substitution owns a types-layer scalar accumulator and merges it only after the whole application
//! returns. Interface fill exposes a map-free cumulative checkpoint only after a named whole
//! heritage SCC freezes. A cooperative stop remains partial. A hard kill preserves no in-memory
//! WU0G evidence; durable boundary output would require a separately specified outer publisher and
//! is not claimed here.
//!
//! No recursive visit performs TLS/`RefCell` access, allocation for telemetry, atomics, locks, or
//! I/O. The only application-cardinality state is one bounded exact application-key frequency map,
//! keyed by template `TypeId` plus the complete ordered `(TypeParamId, TypeId)` arguments. Visit,
//! copy, and interner counters are scalar fields on the local application accumulator. Candidate-B
//! entries retain the completed run's visit count so a hit can report avoided visits without
//! estimating.
//! Exact-key count, aggregate argument count, and aggregate canonical-key bytes each have explicit
//! budgets. Checkpoints retain only scalars and digests under one aggregate byte budget; they never
//! clone the application map or Candidate-B entries. All additions are checked and set `saturated`
//! on overflow; exhausting any budget also sets it. A saturated or internally inconsistent report
//! fails closed and cannot select an optimization.
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
//! One runner rebuilds and authenticates each canonical rung, parses/binds/reserves all 82 sources,
//! observes external construction states through the unchanged WU0E schedule, and restricts
//! diagnostic execution to exactly the selected whole-SCC prefix. Readiness sweeps may not replace
//! that prefix with a same-sized set of later independent SCCs. Boundaries carry component identity
//! and exact group membership; a stall records every remaining selected identity and state before
//! returning no progress. Completion is plan-derived and requires every selected group frozen with
//! interface `template_fill == Done`.
//! Pending, deferred, no-progress, `Building`, missing, out-of-prefix, or plan/runtime-mismatched
//! state fails closed.
//!
//! External topology evidence preserves `CompleteRequired` versus `OpaqueOrderingOnly` edge
//! disposition. Alias expansion is transparent and resolved/unavailable groups are absorbed; actual
//! terminal edges therefore reach interface components or classes. Every encountered alias, class,
//! resolved, unavailable, or out-of-range group is nevertheless classified from its measured state;
//! no declaration-state variant may disappear through a wildcard and only measured frozen state is
//! described as frozen.
//!
//! Semantic parity hashes a WU0G-local canonical prefix product at a valid SCC boundary. It frames
//! profile/universe/rung identities, selected components and completion inventory, exhaustive
//! terminal evidence, full dense canonical `TypeStore` bytes, selected reserved/group construction
//! state, parameter/default/conflict state, canonical record bytes, and pending effects/obligations.
//! The test-only WU0B wrappers reuse private `CanonicalBytes` and `encode_store_row`; encoder
//! internals are not widened. Telemetry and caches are excluded, nested effect frames must be empty,
//! and numeric result `TypeId`s alone are insufficient: changing reserved object contents under the
//! same result id changes the digest. Fully completed end-to-end process evidence continues to use
//! the canonical WU0D semantic identity; a prefix does not masquerade as a WU0D product.
//!
//! Threshold arithmetic derives incremental causality and target-counter reduction from five
//! ordered cumulative baseline/candidate rows. Every percentage is recomputed from checked raw
//! numerators and denominators. Authorization is a separate gate: synthetic evidence is permanently
//! non-authorizing, even when every arithmetic threshold passes. Only authenticated, completed
//! pinned-DOM evidence may authorize. Causal evidence is the exact five-rung by two-mode process
//! product; timing evidence is five ordered AB/BA pairs of one fixed end-to-end workload. Raw cycle,
//! prediction, and control dossiers bind their own report/profile/workload/candidate/artifact inputs.
//! All dossiers share one framed experiment identity, and each process binds its binary, child,
//! request, sentinel, artifact, deadline, memory, RSS, nofile, mode, invocation, cgroup, exit, wait,
//! reap, and cleanup state. Every launch is delegated to exactly one canonical
//! `--wu0g-child-v1 REQUEST RESULT_DIR` runner invocation. Performance launches additionally bind
//! authenticated perf/prlimit executables, host, CPU affinity, perf version/event, raw perf wait
//! status, and the strict seven-field counter artifact. The outer WU0E supervisor remains the sole
//! termination adjudicator and waits/reaps its perf leader; the checker sentinel separately binds
//! the libtest child which perf forks in the same PGID and launch cgroup. A perf count is usable only
//! after normal whole-tree cleanup and an independently authenticated checker completion sentinel.
//! Timeout, memory kill, or no-progress is NO-GO/inconclusive, never positive evidence.
//! Identities are recomputed from domain-separated raw canonical bytes rather than accepted because
//! a caller supplied 64 lowercase hex characters. Valid-looking cross-domain substitutions,
//! reordered evidence, nonmonotonic counters, and out-of-range ratios fail closed.

use super::super::wu0b_profile::{load_strict_profile, OwnedProfileSource, StrictLibraryProfile};
use super::wu0g_interface_fill_attribution::{
    build_dom_heritage_prefix_ladder_for_test, build_manifest_closure_matrix_for_test,
    build_raw_external_topology_with_injection_for_test,
    canonical_interface_prefix_bytes_from_raw_for_test,
    canonical_reserved_object_same_type_id_witness_for_test,
    evaluate_interface_fill_thresholds_from_raw_for_test,
    full_universe_external_terminal_inventory_for_test,
    full_universe_external_topology_terminal_count_for_test, measure_application_budget_for_test,
    measure_copy_accounting_for_test, measure_dom_rung_for_test,
    measure_external_disposition_readiness_for_test,
    measure_interface_fill_cooperative_partial_for_test, measure_interface_fill_source_for_test,
    measure_manifest_closure_for_test, observe_wu0e_phase_snapshots_for_test,
    parse_wu0g_child_request_for_test, parse_wu0g_child_result_for_test,
    parse_wu0g_perf_artifact_for_test, run_pinned_dom_authorization_probe_for_test,
    run_selected_component_plan_for_test, run_wu0g_hardened_child_once_for_test,
    validate_interface_fill_authorization_from_raw_for_test,
    validate_measured_ladder_row_from_raw_for_test, validate_raw_completion_inventory_for_test,
    InterfaceFillAttributionLimits, InterfaceFillAttributionMode, InterfaceFillAttributionReport,
    InterfaceFillBudgetKind, InterfaceFillCounterFamily, InterfaceFillExternalConstructionState,
    InterfaceFillExternalTerminalKind, InterfaceFillPlanOutcome, InterfaceFillRemainingState,
    InterfaceHeritageDependency, InterfaceHeritageEdgeDisposition, ManifestClosureKind,
    MAX_APPLICATION_ARGUMENTS_PER_KEY, MAX_COMPONENT_CHECKPOINTS,
    MAX_TRACKED_APPLICATION_ARGUMENTS, MAX_TRACKED_APPLICATION_KEYS,
    MAX_TRACKED_APPLICATION_KEY_BYTES, MAX_TRACKED_CHECKPOINT_BYTES,
};
use crate::source::LibraryFileOrdinal;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const ATTRIBUTION_FEATURE: &str = "wu0-interface-fill-attribution";
const CONTROL_FEATURE: &str = "wu0-uninstrumented-control";
const WU0G_DEADLINE_MS: u64 = 30_000;
const WU0G_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
const WU0G_RSS_LIMIT_BYTES: u64 = 384 * 1024 * 1024;
const WU0G_NOFILE_LIMIT: u64 = 256;
const WU0G_PERF_EVENT: &str = "instructions:u";
const WU0G_STDOUT_CAP_BYTES: u64 = 128 * 1024;
const WU0G_STDERR_CAP_BYTES: u64 = 128 * 1024;
const WU0G_REQUEST_CAP_BYTES: u64 = 64 * 1024;
const WU0G_RESULT_CAP_BYTES: u64 = 64 * 1024;
const WU0G_SENTINEL_CAP_BYTES: u64 = 4 * 1024;
const WU0G_ARTIFACT_CAP_BYTES: u64 = 256 * 1024;
const WU0G_PERF_ARTIFACT_CAP_BYTES: u64 = 4 * 1024;
const WU0G_REQUEST_PROTOCOL_FIELDS: &[&str] = &[
    "artifact_relative_path",
    "binary_identity",
    "candidate_identity",
    "cpu_affinity",
    "deadline_ms",
    "host_identity",
    "kind",
    "launch_identity",
    "launch_ordinal",
    "libtest_relative_path",
    "memory_limit_bytes",
    "mode",
    "nofile_hard",
    "nofile_soft",
    "nonce",
    "pair_identity",
    "pair_ordinal",
    "perf_event",
    "perf_identity",
    "perf_version",
    "plan_identity",
    "prlimit_identity",
    "request_relative_path",
    "result_identity",
    "result_relative_path",
    "rss_limit_bytes",
    "rung_identity",
    "rung_ordinal",
    "semantic_artifact_relative_path",
    "sentinel_relative_path",
    "workload_identity",
];
const WU0G_RESULT_PROTOCOL_FIELDS: &[&str] = &[
    "artifact_identity",
    "artifact_size",
    "binary_identity",
    "cgroup_identity",
    "cgroup_populated_zero",
    "cgroup_removed",
    "cgroup_retained",
    "child_argv",
    "child_env",
    "child_fd_inventory",
    "child_identity",
    "cleanup_succeeded",
    "containment_failures",
    "deadline_ms",
    "deadline_readback_ms",
    "drain_complete",
    "exit_code",
    "host_identity",
    "launch_identity",
    "leader_pid",
    "leader_reaped",
    "leader_start_ticks",
    "max_rss_bytes",
    "membership_verified",
    "memory_limit_bytes",
    "memory_limit_readback_bytes",
    "nofile_hard",
    "nofile_hard_readback",
    "nofile_soft",
    "nofile_soft_readback",
    "oom_delta",
    "oom_kill_delta",
    "outer_raw_wait_status",
    "perf_artifact_identity",
    "perf_artifact_size",
    "perf_event",
    "perf_exit_code",
    "perf_identity",
    "perf_invocation",
    "perf_raw_wait_status",
    "perf_term_signal",
    "perf_version",
    "pgid",
    "pgid_empty",
    "plan_identity",
    "prlimit_identity",
    "readiness_seen",
    "request_content_identity",
    "result_identity",
    "rss_limit_bytes",
    "rss_limit_readback_bytes",
    "scope_abort_observed",
    "scope_abort_requested",
    "scope_identity",
    "sentinel_identity",
    "sentinel_size",
    "stderr_identity",
    "stderr_size",
    "stdout_identity",
    "stdout_size",
    "term_signal",
    "termination",
];
const HOT_GUARD: &str = "#[cfg(all(test, feature = \"wu0-interface-fill-attribution\", not(feature = \"wu0-uninstrumented-control\")))]";
const HOT_FALLBACK_GUARD: &str = "#[cfg(not(all(test, feature = \"wu0-interface-fill-attribution\", not(feature = \"wu0-uninstrumented-control\"))))]";

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

// The substitution prefilter proves param-free cycles identity for maps of up
// to 64 params, so this witness pins a 65-param template to keep the tainted
// walk — and Candidate-B's cache — reachable (see param_relevant_prefilter_spec).
static CYCLE_TAINTED: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let params = (0..65)
        .map(|index| format!("T{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = vec!["string"; 65].join(", ");
    format!(
        "\ninterface CycleRoot {{ self: CycleRoot }}\ninterface CycleTemplate<{params}> extends CycleRoot {{ value: T0 }}\ninterface CycleFirst extends CycleTemplate<{arguments}> {{}}\ninterface CycleSecond extends CycleTemplate<{arguments}> {{}}\n"
    )
});

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RawIdentityEvidence {
    pub(super) domain: String,
    pub(super) canonical_bytes: Vec<u8>,
    pub(super) claimed_sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawEvidenceDomain {
    Synthetic,
    PinnedDom82,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RawProbeMode {
    Baseline,
    CandidateB,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawProbeTermination {
    Complete,
    Deadline,
    MemoryLimit,
    NoProgress,
    StdoutLimit,
    StderrLimit,
    Infrastructure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RawBoundedObservation {
    pub(super) cap_bytes: u64,
    pub(super) observed_bytes: u64,
    pub(super) truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RawProcessStatus<'a> {
    scope_identity: &'a RawIdentityEvidence,
    cgroup_identity: &'a RawIdentityEvidence,
    leader_pid: u32,
    leader_start_ticks: u64,
    pgid: u32,
    readiness_seen: bool,
    membership_verified: bool,
    drain_complete: bool,
    cgroup_populated_zero: bool,
    pgid_empty: bool,
    cgroup_removed: bool,
    cgroup_retained: bool,
    scope_abort_requested: bool,
    scope_abort_observed: bool,
    termination: RawProbeTermination,
    deadline_ms: u64,
    deadline_readback_ms: u64,
    memory_limit_bytes: u64,
    memory_limit_readback_bytes: u64,
    rss_limit_bytes: u64,
    rss_limit_readback_bytes: u64,
    nofile_soft_limit: u64,
    nofile_hard_limit: u64,
    nofile_soft_readback: u64,
    nofile_hard_readback: u64,
    max_rss_bytes: u64,
    containment_failures: u64,
    cgroup_oom_delta: u64,
    cgroup_oom_kill_delta: u64,
    exit_code: Option<i32>,
    term_signal: Option<i32>,
    raw_wait_status: Option<i32>,
    waited: bool,
    reaped: bool,
    cleanup_succeeded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawLadderCounterPoint {
    pub(super) rung_identity: RawIdentityEvidence,
    pub(super) report_identity: RawIdentityEvidence,
    pub(super) semantic_identity: RawIdentityEvidence,
    pub(super) target_counter: u64,
    pub(super) total_work_counter: u64,
    pub(super) complete_component_identities: Vec<RawIdentityEvidence>,
    pub(super) saturated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawLadderCounterRow {
    pub(super) baseline: RawLadderCounterPoint,
    pub(super) candidate: RawLadderCounterPoint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawFreshProcessDossier {
    pub(super) plan_identity: RawIdentityEvidence,
    pub(super) profile_identity: RawIdentityEvidence,
    pub(super) universe_identity: RawIdentityEvidence,
    pub(super) workload_identity: RawIdentityEvidence,
    pub(super) candidate_identity: RawIdentityEvidence,
    pub(super) binary_identity: RawIdentityEvidence,
    pub(super) rung_identity: RawIdentityEvidence,
    pub(super) report_identity: RawIdentityEvidence,
    pub(super) semantic_identity: RawIdentityEvidence,
    pub(super) rung_ordinal: u8,
    pub(super) mode: RawProbeMode,
    pub(super) canonical_invocation: Vec<String>,
    pub(super) scope_identity: RawIdentityEvidence,
    pub(super) cgroup_identity: RawIdentityEvidence,
    pub(super) leader_pid: u32,
    pub(super) leader_start_ticks: u64,
    pub(super) pgid: u32,
    pub(super) readiness_seen: bool,
    pub(super) membership_verified: bool,
    pub(super) drain_complete: bool,
    pub(super) cgroup_populated_zero: bool,
    pub(super) pgid_empty: bool,
    pub(super) cgroup_removed: bool,
    pub(super) cgroup_retained: bool,
    pub(super) scope_abort_requested: bool,
    pub(super) scope_abort_observed: bool,
    pub(super) deadline_ms: u64,
    pub(super) deadline_readback_ms: u64,
    pub(super) memory_limit_bytes: u64,
    pub(super) memory_limit_readback_bytes: u64,
    pub(super) rss_limit_bytes: u64,
    pub(super) rss_limit_readback_bytes: u64,
    pub(super) nofile_soft_limit: u64,
    pub(super) nofile_hard_limit: u64,
    pub(super) nofile_soft_readback: u64,
    pub(super) nofile_hard_readback: u64,
    pub(super) termination: RawProbeTermination,
    pub(super) max_rss_bytes: u64,
    pub(super) containment_failures: u64,
    pub(super) cgroup_oom_delta: u64,
    pub(super) cgroup_oom_kill_delta: u64,
    pub(super) exit_code: Option<i32>,
    pub(super) term_signal: Option<i32>,
    pub(super) raw_wait_status: Option<i32>,
    pub(super) waited: bool,
    pub(super) reaped: bool,
    pub(super) cleanup_succeeded: bool,
    pub(super) request_content_identity: RawIdentityEvidence,
    pub(super) request_path_identity: RawIdentityEvidence,
    pub(super) result_identity: RawIdentityEvidence,
    pub(super) result_content_identity: RawIdentityEvidence,
    pub(super) result_path_identity: RawIdentityEvidence,
    pub(super) result_bound_plan_identity: RawIdentityEvidence,
    pub(super) result_bound_request_content_identity: RawIdentityEvidence,
    pub(super) stdout_identity: RawIdentityEvidence,
    pub(super) stderr_identity: RawIdentityEvidence,
    pub(super) child_identity: RawIdentityEvidence,
    pub(super) completion_sentinel_identity: RawIdentityEvidence,
    pub(super) child_artifact_identity: RawIdentityEvidence,
    pub(super) prlimit_identity: RawIdentityEvidence,
    pub(super) stdout_observation: RawBoundedObservation,
    pub(super) stderr_observation: RawBoundedObservation,
    pub(super) request_observation: RawBoundedObservation,
    pub(super) result_observation: RawBoundedObservation,
    pub(super) sentinel_observation: RawBoundedObservation,
    pub(super) semantic_artifact_observation: RawBoundedObservation,
}

impl RawFreshProcessDossier {
    fn process_status(&self) -> RawProcessStatus<'_> {
        RawProcessStatus {
            scope_identity: &self.scope_identity,
            cgroup_identity: &self.cgroup_identity,
            leader_pid: self.leader_pid,
            leader_start_ticks: self.leader_start_ticks,
            pgid: self.pgid,
            readiness_seen: self.readiness_seen,
            membership_verified: self.membership_verified,
            drain_complete: self.drain_complete,
            cgroup_populated_zero: self.cgroup_populated_zero,
            pgid_empty: self.pgid_empty,
            cgroup_removed: self.cgroup_removed,
            cgroup_retained: self.cgroup_retained,
            scope_abort_requested: self.scope_abort_requested,
            scope_abort_observed: self.scope_abort_observed,
            termination: self.termination,
            deadline_ms: self.deadline_ms,
            deadline_readback_ms: self.deadline_readback_ms,
            memory_limit_bytes: self.memory_limit_bytes,
            memory_limit_readback_bytes: self.memory_limit_readback_bytes,
            rss_limit_bytes: self.rss_limit_bytes,
            rss_limit_readback_bytes: self.rss_limit_readback_bytes,
            nofile_soft_limit: self.nofile_soft_limit,
            nofile_hard_limit: self.nofile_hard_limit,
            nofile_soft_readback: self.nofile_soft_readback,
            nofile_hard_readback: self.nofile_hard_readback,
            max_rss_bytes: self.max_rss_bytes,
            containment_failures: self.containment_failures,
            cgroup_oom_delta: self.cgroup_oom_delta,
            cgroup_oom_kill_delta: self.cgroup_oom_kill_delta,
            exit_code: self.exit_code,
            term_signal: self.term_signal,
            raw_wait_status: self.raw_wait_status,
            waited: self.waited,
            reaped: self.reaped,
            cleanup_succeeded: self.cleanup_succeeded,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawCycleProfileDossier {
    pub(super) plan_identity: RawIdentityEvidence,
    pub(super) profile_identity: RawIdentityEvidence,
    pub(super) workload_identity: RawIdentityEvidence,
    pub(super) candidate_identity: RawIdentityEvidence,
    pub(super) binary_identity: RawIdentityEvidence,
    pub(super) mode: RawProbeMode,
    pub(super) report_identity: RawIdentityEvidence,
    pub(super) semantic_identity: RawIdentityEvidence,
    pub(super) artifact_identity: RawIdentityEvidence,
    pub(super) artifact_content_identity: RawIdentityEvidence,
    pub(super) explained_cycle_visits: u64,
    pub(super) total_cycle_visits: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawPredictionDossier {
    pub(super) plan_identity: RawIdentityEvidence,
    pub(super) report_identity: RawIdentityEvidence,
    pub(super) profile_identity: RawIdentityEvidence,
    pub(super) workload_identity: RawIdentityEvidence,
    pub(super) candidate_identity: RawIdentityEvidence,
    pub(super) binary_identity: RawIdentityEvidence,
    pub(super) mode: RawProbeMode,
    pub(super) semantic_identity: RawIdentityEvidence,
    pub(super) artifact_identity: RawIdentityEvidence,
    pub(super) artifact_content_identity: RawIdentityEvidence,
    pub(super) total_end_to_end_work: u64,
    pub(super) attributed_target_work: u64,
    pub(super) baseline_target_work: u64,
    pub(super) candidate_target_work: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawControlPairDossier {
    pub(super) plan_identity: RawIdentityEvidence,
    pub(super) profile_identity: RawIdentityEvidence,
    pub(super) workload_identity: RawIdentityEvidence,
    pub(super) candidate_identity: RawIdentityEvidence,
    pub(super) baseline_binary_identity: RawIdentityEvidence,
    pub(super) candidate_binary_identity: RawIdentityEvidence,
    pub(super) baseline_report_identity: RawIdentityEvidence,
    pub(super) candidate_report_identity: RawIdentityEvidence,
    pub(super) baseline_semantic_identity: RawIdentityEvidence,
    pub(super) candidate_semantic_identity: RawIdentityEvidence,
    pub(super) baseline_artifact_identity: RawIdentityEvidence,
    pub(super) candidate_artifact_identity: RawIdentityEvidence,
    pub(super) baseline_artifact_content_identity: RawIdentityEvidence,
    pub(super) candidate_artifact_content_identity: RawIdentityEvidence,
    pub(super) baseline_measurement: u64,
    pub(super) candidate_measurement: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawLaunchOrder {
    Ab,
    Ba,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawPerformanceLaunch {
    pub(super) plan_identity: RawIdentityEvidence,
    pub(super) representative_workload_identity: RawIdentityEvidence,
    pub(super) candidate_identity: RawIdentityEvidence,
    pub(super) binary_identity: RawIdentityEvidence,
    pub(super) report_identity: RawIdentityEvidence,
    pub(super) semantic_identity: RawIdentityEvidence,
    pub(super) pair_identity: RawIdentityEvidence,
    pub(super) launch_identity: RawIdentityEvidence,
    pub(super) request_content_identity: RawIdentityEvidence,
    pub(super) request_path_identity: RawIdentityEvidence,
    pub(super) result_identity: RawIdentityEvidence,
    pub(super) result_content_identity: RawIdentityEvidence,
    pub(super) result_path_identity: RawIdentityEvidence,
    pub(super) result_bound_plan_identity: RawIdentityEvidence,
    pub(super) result_bound_request_content_identity: RawIdentityEvidence,
    pub(super) child_identity: RawIdentityEvidence,
    pub(super) completion_sentinel_identity: RawIdentityEvidence,
    pub(super) child_artifact_identity: RawIdentityEvidence,
    pub(super) perf_artifact_identity: RawIdentityEvidence,
    pub(super) prlimit_identity: RawIdentityEvidence,
    pub(super) perf_identity: RawIdentityEvidence,
    pub(super) host_identity: RawIdentityEvidence,
    pub(super) perf_version: String,
    pub(super) perf_event: String,
    pub(super) cpu_affinity: String,
    pub(super) pair_ordinal: u8,
    pub(super) launch_ordinal: u8,
    pub(super) mode: RawProbeMode,
    pub(super) canonical_invocation: Vec<String>,
    pub(super) perf_invocation: Vec<String>,
    pub(super) scope_identity: RawIdentityEvidence,
    pub(super) cgroup_identity: RawIdentityEvidence,
    pub(super) leader_pid: u32,
    pub(super) leader_start_ticks: u64,
    pub(super) pgid: u32,
    pub(super) readiness_seen: bool,
    pub(super) membership_verified: bool,
    pub(super) drain_complete: bool,
    pub(super) cgroup_populated_zero: bool,
    pub(super) pgid_empty: bool,
    pub(super) cgroup_removed: bool,
    pub(super) cgroup_retained: bool,
    pub(super) scope_abort_requested: bool,
    pub(super) scope_abort_observed: bool,
    pub(super) deadline_ms: u64,
    pub(super) deadline_readback_ms: u64,
    pub(super) memory_limit_bytes: u64,
    pub(super) memory_limit_readback_bytes: u64,
    pub(super) rss_limit_bytes: u64,
    pub(super) rss_limit_readback_bytes: u64,
    pub(super) nofile_soft_limit: u64,
    pub(super) nofile_hard_limit: u64,
    pub(super) nofile_soft_readback: u64,
    pub(super) nofile_hard_readback: u64,
    pub(super) termination: RawProbeTermination,
    pub(super) wall_ns: u64,
    pub(super) instructions: u64,
    pub(super) perf_runtime: u64,
    pub(super) max_rss_bytes: u64,
    pub(super) containment_failures: u64,
    pub(super) cgroup_oom_delta: u64,
    pub(super) cgroup_oom_kill_delta: u64,
    pub(super) exit_code: Option<i32>,
    pub(super) term_signal: Option<i32>,
    pub(super) raw_wait_status: Option<i32>,
    pub(super) perf_raw_wait_status: Option<i32>,
    pub(super) perf_exit_code: Option<i32>,
    pub(super) perf_term_signal: Option<i32>,
    pub(super) perf_waited: bool,
    pub(super) waited: bool,
    pub(super) reaped: bool,
    pub(super) cleanup_succeeded: bool,
    pub(super) stdout_identity: RawIdentityEvidence,
    pub(super) stderr_identity: RawIdentityEvidence,
    pub(super) stdout_observation: RawBoundedObservation,
    pub(super) stderr_observation: RawBoundedObservation,
    pub(super) request_observation: RawBoundedObservation,
    pub(super) result_observation: RawBoundedObservation,
    pub(super) sentinel_observation: RawBoundedObservation,
    pub(super) semantic_artifact_observation: RawBoundedObservation,
    pub(super) perf_artifact_observation: RawBoundedObservation,
}

impl RawPerformanceLaunch {
    fn process_status(&self) -> RawProcessStatus<'_> {
        RawProcessStatus {
            scope_identity: &self.scope_identity,
            cgroup_identity: &self.cgroup_identity,
            leader_pid: self.leader_pid,
            leader_start_ticks: self.leader_start_ticks,
            pgid: self.pgid,
            readiness_seen: self.readiness_seen,
            membership_verified: self.membership_verified,
            drain_complete: self.drain_complete,
            cgroup_populated_zero: self.cgroup_populated_zero,
            pgid_empty: self.pgid_empty,
            cgroup_removed: self.cgroup_removed,
            cgroup_retained: self.cgroup_retained,
            scope_abort_requested: self.scope_abort_requested,
            scope_abort_observed: self.scope_abort_observed,
            termination: self.termination,
            deadline_ms: self.deadline_ms,
            deadline_readback_ms: self.deadline_readback_ms,
            memory_limit_bytes: self.memory_limit_bytes,
            memory_limit_readback_bytes: self.memory_limit_readback_bytes,
            rss_limit_bytes: self.rss_limit_bytes,
            rss_limit_readback_bytes: self.rss_limit_readback_bytes,
            nofile_soft_limit: self.nofile_soft_limit,
            nofile_hard_limit: self.nofile_hard_limit,
            nofile_soft_readback: self.nofile_soft_readback,
            nofile_hard_readback: self.nofile_hard_readback,
            max_rss_bytes: self.max_rss_bytes,
            containment_failures: self.containment_failures,
            cgroup_oom_delta: self.cgroup_oom_delta,
            cgroup_oom_kill_delta: self.cgroup_oom_kill_delta,
            exit_code: self.exit_code,
            term_signal: self.term_signal,
            raw_wait_status: self.raw_wait_status,
            waited: self.waited,
            reaped: self.reaped,
            cleanup_succeeded: self.cleanup_succeeded,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawPerformancePairDossier {
    pub(super) pair_identity: RawIdentityEvidence,
    pub(super) pair_ordinal: u8,
    pub(super) order: RawLaunchOrder,
    pub(super) launches: Vec<RawPerformanceLaunch>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawThresholdEvidence {
    pub(super) experiment_identity: RawIdentityEvidence,
    pub(super) plan_identity: RawIdentityEvidence,
    pub(super) profile_identity: RawIdentityEvidence,
    pub(super) universe_identity: RawIdentityEvidence,
    pub(super) workload_identity: RawIdentityEvidence,
    pub(super) representative_workload_identity: RawIdentityEvidence,
    pub(super) candidate_identity: RawIdentityEvidence,
    pub(super) baseline_binary_identity: RawIdentityEvidence,
    pub(super) candidate_binary_identity: RawIdentityEvidence,
    pub(super) prlimit_identity: RawIdentityEvidence,
    pub(super) perf_identity: RawIdentityEvidence,
    pub(super) host_identity: RawIdentityEvidence,
    pub(super) perf_version: String,
    pub(super) perf_event: String,
    pub(super) cpu_affinity: String,
    pub(super) ladder_rows: Vec<RawLadderCounterRow>,
    pub(super) cycle_profiles: Vec<RawCycleProfileDossier>,
    pub(super) predictions: Vec<RawPredictionDossier>,
    pub(super) controls: Vec<RawControlPairDossier>,
    pub(super) causal_process_dossiers: Vec<RawFreshProcessDossier>,
    pub(super) performance_pairs: Vec<RawPerformancePairDossier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawAuthorizationEvidence {
    pub(super) evidence_domain: RawEvidenceDomain,
    pub(super) thresholds: RawThresholdEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawCanonicalTypeStoreRow {
    pub(super) type_id: u32,
    pub(super) canonical_payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawCanonicalGroupState {
    pub(super) group_id: u32,
    pub(super) selected: bool,
    pub(super) state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawCanonicalPrefixInput {
    pub(super) profile: Vec<u8>,
    pub(super) universe: Vec<u8>,
    pub(super) rung: Vec<u8>,
    pub(super) component_order_and_membership: Vec<Vec<u8>>,
    pub(super) external_inventory: Vec<Vec<u8>>,
    pub(super) dense_type_store: Vec<RawCanonicalTypeStoreRow>,
    pub(super) reserved_universe_group_states: Vec<RawCanonicalGroupState>,
    pub(super) parameter_defaults: Vec<Vec<u8>>,
    pub(super) parameter_conflicts: Vec<Vec<u8>>,
    pub(super) canonical_records: Vec<Vec<u8>>,
    pub(super) pending_effects: Vec<Vec<u8>>,
    pub(super) pending_obligations: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawSelectedComponentDirective {
    pub(super) identity: String,
    pub(super) group_ids: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawPlanDependency {
    pub(super) identity: String,
    pub(super) disposition: InterfaceHeritageEdgeDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawPlannedComponent {
    pub(super) identity: String,
    pub(super) group_ids: Vec<u32>,
    pub(super) dependencies: Vec<RawPlanDependency>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawPlanTerminalState {
    Pending,
    Frozen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawSelectedComponentPlan {
    pub(super) selected: Vec<RawPlannedComponent>,
    pub(super) external_states: Vec<(String, RawPlanTerminalState)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawExternalTopologyInjection {
    pub(super) identity: String,
    pub(super) out_of_range_group_id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawCopyAccountingInput {
    pub(super) property_name_byte_lengths: Vec<usize>,
    pub(super) call_signature_count: usize,
    pub(super) construct_signature_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawBudgetWitnessRequest {
    pub(super) limits: InterfaceFillAttributionLimits,
    pub(super) budget: InterfaceFillBudgetKind,
    pub(super) requested: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawCompletionGroupState {
    Pending,
    Deferred,
    NoProgress,
    Building,
    Frozen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawCompletionComponent {
    pub(super) identity: String,
    pub(super) group_ids: Vec<u32>,
    pub(super) group_states: Vec<RawCompletionGroupState>,
    pub(super) template_fill_done: Vec<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawCompletionInventory {
    pub(super) expected_components: Vec<RawSelectedComponentDirective>,
    pub(super) selected_components: Vec<RawSelectedComponentDirective>,
    pub(super) completed_components: Vec<RawCompletionComponent>,
    pub(super) remaining_selected_components: Vec<RawCompletionComponent>,
    pub(super) expected_external_topology_rows: Vec<Vec<u8>>,
    pub(super) external_topology_rows: Vec<Vec<u8>>,
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn raw_identity(domain: &str, canonical_bytes: impl Into<Vec<u8>>) -> RawIdentityEvidence {
    let canonical_bytes = canonical_bytes.into();
    let mut framed = Vec::new();
    framed.extend_from_slice(
        &u64::try_from(domain.len())
            .expect("identity domain length fits u64")
            .to_be_bytes(),
    );
    framed.extend_from_slice(domain.as_bytes());
    framed.extend_from_slice(
        &u64::try_from(canonical_bytes.len())
            .expect("identity payload length fits u64")
            .to_be_bytes(),
    );
    framed.extend_from_slice(&canonical_bytes);
    RawIdentityEvidence {
        domain: domain.to_owned(),
        canonical_bytes,
        claimed_sha256: sha256(&framed),
    }
}

fn bounded_observation(cap_bytes: u64, observed_bytes: u64) -> RawBoundedObservation {
    assert!(observed_bytes <= cap_bytes);
    RawBoundedObservation {
        cap_bytes,
        observed_bytes,
        truncated: false,
    }
}

fn protocol_record_bytes(
    header: &str,
    schema: &[&str],
    fields: &BTreeMap<String, String>,
) -> Vec<u8> {
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        schema
    );
    let mut bytes = format!("{header}\n").into_bytes();
    for key in schema {
        let value = fields.get(*key).expect("schema field exists");
        bytes.extend_from_slice(format!("{key}={}:{}\n", value.len(), value).as_bytes());
    }
    bytes
}

fn spec_owned_request_fields(kind: &str) -> BTreeMap<String, String> {
    let mut request = WU0G_REQUEST_PROTOCOL_FIELDS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                (*key).to_owned(),
                format!("{:064x}", index.checked_add(1).expect("field index")),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (key, value) in [
        ("artifact_relative_path", "artifacts/child.bin"),
        ("cpu_affinity", "0"),
        ("deadline_ms", "30000"),
        ("libtest_relative_path", "tools/frozen-libtest"),
        ("memory_limit_bytes", "536870912"),
        ("mode", "baseline"),
        ("nofile_hard", "256"),
        ("nofile_soft", "256"),
        ("nonce", "0123456789abcdef0123456789abcdef"),
        ("request_relative_path", "requests/launch-0.request"),
        ("result_relative_path", "results/launch-0"),
        ("rss_limit_bytes", "402653184"),
        ("semantic_artifact_relative_path", "artifacts/semantic.bin"),
        ("sentinel_relative_path", "artifacts/completion.sentinel"),
    ] {
        request.insert(key.to_owned(), value.to_owned());
    }
    request.insert("kind".to_owned(), kind.to_owned());
    match kind {
        "causal" => {
            for key in [
                "launch_identity",
                "launch_ordinal",
                "pair_identity",
                "pair_ordinal",
                "perf_event",
                "perf_identity",
                "perf_version",
            ] {
                request.insert(key.to_owned(), "none".to_owned());
            }
            request.insert("rung_ordinal".to_owned(), "0".to_owned());
        }
        "performance" => {
            request.insert("launch_ordinal".to_owned(), "0".to_owned());
            request.insert("pair_ordinal".to_owned(), "0".to_owned());
            request.insert("perf_event".to_owned(), WU0G_PERF_EVENT.to_owned());
            request.insert("perf_version".to_owned(), "perf version fixture".to_owned());
            request.insert("rung_identity".to_owned(), "none".to_owned());
            request.insert("rung_ordinal".to_owned(), "none".to_owned());
        }
        _ => panic!("spec-owned request kind must be canonical"),
    }
    request
}

fn spec_owned_request_bytes(kind: &str) -> Vec<u8> {
    protocol_record_bytes(
        "typokat-wu0g-child-request-v1",
        WU0G_REQUEST_PROTOCOL_FIELDS,
        &spec_owned_request_fields(kind),
    )
}

fn spec_owned_protocol_exchange() -> (Vec<u8>, Vec<u8>) {
    let request = spec_owned_request_fields("performance");
    let request_bytes = protocol_record_bytes(
        "typokat-wu0g-child-request-v1",
        WU0G_REQUEST_PROTOCOL_FIELDS,
        &request,
    );
    let request_content_identity = raw_identity("wu0g-request-content-v1", request_bytes.clone());

    let mut result = WU0G_RESULT_PROTOCOL_FIELDS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                (*key).to_owned(),
                format!("{:064x}", index.checked_add(101).expect("field index")),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (key, value) in [
        ("artifact_size", "4096"),
        ("cgroup_populated_zero", "1"),
        ("cgroup_removed", "1"),
        ("cgroup_retained", "0"),
        (
            "child_argv",
            "--ignored|--exact|check::checker::decls::wu0g_interface_fill_attribution_spec::wu0g_hardened_child_once|--nocapture",
        ),
        (
            "child_env",
            "TYPOKAT_WU0G_CHILD_REQUEST_FD|TYPOKAT_WU0G_CHILD_REQUEST_SHA256|TYPOKAT_WU0G_CHILD_RESULT_DIR_FD",
        ),
        (
            "child_fd_inventory",
            "stderr|stdin|stdout|request|result|libtest|prlimit|perf|perf-log",
        ),
        ("cleanup_succeeded", "1"),
        ("containment_failures", "0"),
        ("deadline_ms", "30000"),
        ("deadline_readback_ms", "30000"),
        ("drain_complete", "1"),
        ("exit_code", "0"),
        ("leader_pid", "1234"),
        ("leader_reaped", "1"),
        ("leader_start_ticks", "5678"),
        ("max_rss_bytes", "134217728"),
        ("memory_limit_bytes", "536870912"),
        ("memory_limit_readback_bytes", "536870912"),
        ("membership_verified", "1"),
        ("nofile_hard", "256"),
        ("nofile_hard_readback", "256"),
        ("nofile_soft", "256"),
        ("nofile_soft_readback", "256"),
        ("oom_delta", "0"),
        ("oom_kill_delta", "0"),
        ("outer_raw_wait_status", "0"),
        ("perf_artifact_size", "48"),
        ("perf_event", "instructions:u"),
        ("perf_exit_code", "0"),
        (
            "perf_invocation",
            "/usr/bin/perf|stat|--no-big-num|--no-scale|-x|;|-e|instructions:u|--log-fd|198|--|/proc/self/fd/197|--ignored|--exact|check::checker::decls::wu0g_interface_fill_attribution_spec::wu0g_hardened_child_once|--nocapture",
        ),
        ("perf_raw_wait_status", "0"),
        ("perf_term_signal", "none"),
        ("perf_version", "perf version fixture"),
        ("pgid", "1234"),
        ("pgid_empty", "1"),
        ("readiness_seen", "1"),
        ("rss_limit_bytes", "402653184"),
        ("rss_limit_readback_bytes", "402653184"),
        ("scope_abort_observed", "0"),
        ("scope_abort_requested", "0"),
        ("sentinel_size", "512"),
        ("stderr_size", "0"),
        ("stdout_size", "128"),
        ("term_signal", "none"),
        ("termination", "normal"),
    ] {
        result.insert(key.to_owned(), value.to_owned());
    }
    for key in [
        "binary_identity",
        "host_identity",
        "launch_identity",
        "perf_event",
        "perf_identity",
        "perf_version",
        "plan_identity",
        "prlimit_identity",
        "result_identity",
    ] {
        result.insert(
            key.to_owned(),
            request.get(key).expect("request binding field").clone(),
        );
    }
    result.insert(
        "request_content_identity".to_owned(),
        request_content_identity.claimed_sha256,
    );
    let result_bytes = protocol_record_bytes(
        "typokat-wu0g-child-result-v1",
        WU0G_RESULT_PROTOCOL_FIELDS,
        &result,
    );
    (request_bytes, result_bytes)
}

fn synthetic_82_source_profile() -> StrictLibraryProfile {
    let weights = [13_usize, 12, 25, 25, 25];
    let names = ["One", "Two", "Three", "Four", "Five"];
    let mut source = String::new();
    for (index, (name, weight)) in names.into_iter().zip(weights).enumerate() {
        source.push_str(&format!("interface WU0G{name}"));
        if index > 0 {
            source.push_str(&format!(" extends WU0G{}", names[index - 1]));
        }
        source.push_str(" {\n");
        for member in 0..weight {
            source.push_str(&format!("  member_{index}_{member}: number;\n"));
        }
        source.push_str("}\n");
    }
    let mut sources = vec![OwnedProfileSource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "wu0g.synthetic.000.d.ts".to_owned(),
        source,
    }];
    sources.extend((1..82).map(|ordinal| OwnedProfileSource {
        file_ordinal: LibraryFileOrdinal::new(ordinal),
        name: format!("wu0g.synthetic.{ordinal:03}.d.ts"),
        source: format!("// deliberately empty synthetic source {ordinal:03}\n"),
    }));
    StrictLibraryProfile { sources }
}

fn external_topology_profile() -> StrictLibraryProfile {
    let mut profile = synthetic_82_source_profile();
    profile.sources[0].source = r#"
interface WU0GRequiredTerminal { required: number }
type WU0GTransparentAlias = WU0GRequiredTerminal;
declare class WU0GClassTerminal { classMember: number }
type WU0GResolvedAbsorbed = { resolved: number };
type WU0GUnavailableAbsorbed = MissingWU0GType;
type WU0GOpaqueAlias = WU0GRequiredTerminal & string;
interface WU0GCompleteDependent extends WU0GTransparentAlias { complete: number }
interface WU0GOpaqueDependent extends WU0GOpaqueAlias { opaque: number }
interface WU0GClassDependent extends WU0GClassTerminal { classEdge: number }
interface WU0GResolvedDependent extends WU0GResolvedAbsorbed { resolvedEdge: number }
interface WU0GUnavailableDependent extends WU0GUnavailableAbsorbed { unavailableEdge: number }
"#
    .to_owned();
    profile
}

fn frame_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(
        &u64::try_from(bytes.len())
            .expect("framed input length fits u64")
            .to_be_bytes(),
    );
    output.extend_from_slice(bytes);
}

fn frame_identity(output: &mut Vec<u8>, identity: &RawIdentityEvidence) {
    frame_bytes(output, identity.domain.as_bytes());
    frame_bytes(output, &identity.canonical_bytes);
    frame_bytes(output, identity.claimed_sha256.as_bytes());
}

fn frame_strings(output: &mut Vec<u8>, strings: &[String]) {
    output.extend_from_slice(
        &u64::try_from(strings.len())
            .expect("string count fits u64")
            .to_be_bytes(),
    );
    for string in strings {
        frame_bytes(output, string.as_bytes());
    }
}

fn frame_bounded_observation(output: &mut Vec<u8>, observation: RawBoundedObservation) {
    output.extend_from_slice(&observation.cap_bytes.to_be_bytes());
    output.extend_from_slice(&observation.observed_bytes.to_be_bytes());
    output.push(u8::from(observation.truncated));
}

fn frame_process_status(output: &mut Vec<u8>, status: RawProcessStatus<'_>) {
    frame_identity(output, status.scope_identity);
    frame_identity(output, status.cgroup_identity);
    output.extend_from_slice(&status.leader_pid.to_be_bytes());
    output.extend_from_slice(&status.leader_start_ticks.to_be_bytes());
    output.extend_from_slice(&status.pgid.to_be_bytes());
    output.extend([
        u8::from(status.readiness_seen),
        u8::from(status.membership_verified),
        u8::from(status.drain_complete),
        u8::from(status.cgroup_populated_zero),
        u8::from(status.pgid_empty),
        u8::from(status.cgroup_removed),
        u8::from(status.cgroup_retained),
        u8::from(status.scope_abort_requested),
        u8::from(status.scope_abort_observed),
    ]);
    output.push(match status.termination {
        RawProbeTermination::Complete => 0,
        RawProbeTermination::Deadline => 1,
        RawProbeTermination::MemoryLimit => 2,
        RawProbeTermination::NoProgress => 3,
        RawProbeTermination::StdoutLimit => 4,
        RawProbeTermination::StderrLimit => 5,
        RawProbeTermination::Infrastructure => 6,
    });
    for value in [
        status.deadline_ms,
        status.deadline_readback_ms,
        status.memory_limit_bytes,
        status.memory_limit_readback_bytes,
        status.rss_limit_bytes,
        status.rss_limit_readback_bytes,
        status.nofile_soft_limit,
        status.nofile_hard_limit,
        status.nofile_soft_readback,
        status.nofile_hard_readback,
        status.max_rss_bytes,
        status.containment_failures,
        status.cgroup_oom_delta,
        status.cgroup_oom_kill_delta,
    ] {
        output.extend_from_slice(&value.to_be_bytes());
    }
    for value in [status.exit_code, status.term_signal, status.raw_wait_status] {
        output.push(u8::from(value.is_some()));
        output.extend_from_slice(&value.unwrap_or_default().to_be_bytes());
    }
    output.extend([
        u8::from(status.waited),
        u8::from(status.reaped),
        u8::from(status.cleanup_succeeded),
    ]);
}

fn plan_identity_from_raw(raw: &RawThresholdEvidence) -> RawIdentityEvidence {
    let mut bytes = Vec::new();
    for identity in [
        &raw.profile_identity,
        &raw.universe_identity,
        &raw.workload_identity,
        &raw.representative_workload_identity,
        &raw.candidate_identity,
        &raw.baseline_binary_identity,
        &raw.candidate_binary_identity,
        &raw.prlimit_identity,
        &raw.perf_identity,
        &raw.host_identity,
    ] {
        frame_identity(&mut bytes, identity);
    }
    for value in [&raw.perf_version, &raw.perf_event, &raw.cpu_affinity] {
        frame_bytes(&mut bytes, value.as_bytes());
    }
    for (rung_ordinal, row) in raw.ladder_rows.iter().enumerate() {
        bytes.push(u8::try_from(rung_ordinal).expect("rung ordinal fits u8"));
        frame_identity(&mut bytes, &row.baseline.rung_identity);
        frame_identity(&mut bytes, &row.candidate.rung_identity);
    }
    for dossier in &raw.causal_process_dossiers {
        bytes.push(dossier.rung_ordinal);
        bytes.push(match dossier.mode {
            RawProbeMode::Baseline => 0,
            RawProbeMode::CandidateB => 1,
        });
        frame_identity(&mut bytes, &dossier.binary_identity);
        frame_strings(&mut bytes, &dossier.canonical_invocation);
        for value in [
            dossier.deadline_ms,
            dossier.memory_limit_bytes,
            dossier.rss_limit_bytes,
            dossier.nofile_soft_limit,
            dossier.nofile_hard_limit,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    for pair in &raw.performance_pairs {
        frame_identity(&mut bytes, &pair.pair_identity);
        bytes.push(pair.pair_ordinal);
        bytes.push(match pair.order {
            RawLaunchOrder::Ab => 0,
            RawLaunchOrder::Ba => 1,
        });
        for launch in &pair.launches {
            for identity in [
                &launch.pair_identity,
                &launch.launch_identity,
                &launch.binary_identity,
            ] {
                frame_identity(&mut bytes, identity);
            }
            bytes.extend([launch.pair_ordinal, launch.launch_ordinal]);
            bytes.push(match launch.mode {
                RawProbeMode::Baseline => 0,
                RawProbeMode::CandidateB => 1,
            });
            frame_strings(&mut bytes, &launch.canonical_invocation);
            frame_strings(&mut bytes, &launch.perf_invocation);
            for value in [
                launch.deadline_ms,
                launch.memory_limit_bytes,
                launch.rss_limit_bytes,
                launch.nofile_soft_limit,
                launch.nofile_hard_limit,
            ] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
    }
    raw_identity("wu0g-plan-v1", bytes)
}

fn experiment_identity_from_raw(raw: &RawThresholdEvidence) -> RawIdentityEvidence {
    let mut bytes = Vec::new();
    frame_identity(&mut bytes, &raw.plan_identity);
    for row in &raw.ladder_rows {
        for point in [&row.baseline, &row.candidate] {
            for identity in [
                &point.rung_identity,
                &point.report_identity,
                &point.semantic_identity,
            ] {
                frame_identity(&mut bytes, identity);
            }
            bytes.extend_from_slice(&point.target_counter.to_be_bytes());
            bytes.extend_from_slice(&point.total_work_counter.to_be_bytes());
            bytes.push(u8::from(point.saturated));
            for component in &point.complete_component_identities {
                frame_identity(&mut bytes, component);
            }
        }
    }
    for dossier in &raw.cycle_profiles {
        for identity in [
            &dossier.profile_identity,
            &dossier.workload_identity,
            &dossier.candidate_identity,
            &dossier.binary_identity,
            &dossier.report_identity,
            &dossier.semantic_identity,
            &dossier.artifact_identity,
            &dossier.artifact_content_identity,
        ] {
            frame_identity(&mut bytes, identity);
        }
        bytes.push(match dossier.mode {
            RawProbeMode::Baseline => 0,
            RawProbeMode::CandidateB => 1,
        });
        bytes.extend_from_slice(&dossier.explained_cycle_visits.to_be_bytes());
        bytes.extend_from_slice(&dossier.total_cycle_visits.to_be_bytes());
    }
    for prediction in &raw.predictions {
        for identity in [
            &prediction.report_identity,
            &prediction.profile_identity,
            &prediction.workload_identity,
            &prediction.candidate_identity,
            &prediction.binary_identity,
            &prediction.semantic_identity,
            &prediction.artifact_identity,
            &prediction.artifact_content_identity,
        ] {
            frame_identity(&mut bytes, identity);
        }
        bytes.push(match prediction.mode {
            RawProbeMode::Baseline => 0,
            RawProbeMode::CandidateB => 1,
        });
        for value in [
            prediction.total_end_to_end_work,
            prediction.attributed_target_work,
            prediction.baseline_target_work,
            prediction.candidate_target_work,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    for control in &raw.controls {
        for identity in [
            &control.profile_identity,
            &control.workload_identity,
            &control.candidate_identity,
            &control.baseline_binary_identity,
            &control.candidate_binary_identity,
            &control.baseline_report_identity,
            &control.candidate_report_identity,
            &control.baseline_semantic_identity,
            &control.candidate_semantic_identity,
            &control.baseline_artifact_identity,
            &control.candidate_artifact_identity,
            &control.baseline_artifact_content_identity,
            &control.candidate_artifact_content_identity,
        ] {
            frame_identity(&mut bytes, identity);
        }
        bytes.extend_from_slice(&control.baseline_measurement.to_be_bytes());
        bytes.extend_from_slice(&control.candidate_measurement.to_be_bytes());
    }
    for dossier in &raw.causal_process_dossiers {
        for identity in [
            &dossier.profile_identity,
            &dossier.universe_identity,
            &dossier.workload_identity,
            &dossier.candidate_identity,
            &dossier.binary_identity,
            &dossier.rung_identity,
            &dossier.report_identity,
            &dossier.semantic_identity,
            &dossier.request_content_identity,
            &dossier.request_path_identity,
            &dossier.result_identity,
            &dossier.result_content_identity,
            &dossier.result_path_identity,
            &dossier.result_bound_plan_identity,
            &dossier.result_bound_request_content_identity,
            &dossier.stdout_identity,
            &dossier.stderr_identity,
            &dossier.child_identity,
            &dossier.completion_sentinel_identity,
            &dossier.child_artifact_identity,
            &dossier.prlimit_identity,
        ] {
            frame_identity(&mut bytes, identity);
        }
        bytes.push(match dossier.mode {
            RawProbeMode::Baseline => 0,
            RawProbeMode::CandidateB => 1,
        });
        bytes.push(dossier.rung_ordinal);
        frame_strings(&mut bytes, &dossier.canonical_invocation);
        for observation in [
            dossier.stdout_observation,
            dossier.stderr_observation,
            dossier.request_observation,
            dossier.result_observation,
            dossier.sentinel_observation,
            dossier.semantic_artifact_observation,
        ] {
            frame_bounded_observation(&mut bytes, observation);
        }
        frame_process_status(&mut bytes, dossier.process_status());
    }
    for pair in &raw.performance_pairs {
        frame_identity(&mut bytes, &pair.pair_identity);
        bytes.push(pair.pair_ordinal);
        bytes.push(match pair.order {
            RawLaunchOrder::Ab => 0,
            RawLaunchOrder::Ba => 1,
        });
        for launch in &pair.launches {
            for identity in [
                &launch.representative_workload_identity,
                &launch.candidate_identity,
                &launch.binary_identity,
                &launch.report_identity,
                &launch.semantic_identity,
                &launch.pair_identity,
                &launch.launch_identity,
                &launch.request_content_identity,
                &launch.request_path_identity,
                &launch.result_identity,
                &launch.result_content_identity,
                &launch.result_path_identity,
                &launch.result_bound_plan_identity,
                &launch.result_bound_request_content_identity,
                &launch.child_identity,
                &launch.completion_sentinel_identity,
                &launch.child_artifact_identity,
                &launch.perf_artifact_identity,
                &launch.prlimit_identity,
                &launch.perf_identity,
                &launch.host_identity,
                &launch.stdout_identity,
                &launch.stderr_identity,
            ] {
                frame_identity(&mut bytes, identity);
            }
            for value in [
                &launch.perf_version,
                &launch.perf_event,
                &launch.cpu_affinity,
            ] {
                frame_bytes(&mut bytes, value.as_bytes());
            }
            bytes.extend([launch.pair_ordinal, launch.launch_ordinal]);
            bytes.push(match launch.mode {
                RawProbeMode::Baseline => 0,
                RawProbeMode::CandidateB => 1,
            });
            frame_strings(&mut bytes, &launch.canonical_invocation);
            frame_strings(&mut bytes, &launch.perf_invocation);
            bytes.extend_from_slice(&launch.wall_ns.to_be_bytes());
            bytes.extend_from_slice(&launch.instructions.to_be_bytes());
            bytes.extend_from_slice(&launch.perf_runtime.to_be_bytes());
            for value in [
                launch.perf_exit_code,
                launch.perf_term_signal,
                launch.perf_raw_wait_status,
            ] {
                bytes.push(u8::from(value.is_some()));
                bytes.extend_from_slice(&value.unwrap_or_default().to_be_bytes());
            }
            bytes.push(u8::from(launch.perf_waited));
            for observation in [
                launch.stdout_observation,
                launch.stderr_observation,
                launch.request_observation,
                launch.result_observation,
                launch.sentinel_observation,
                launch.semantic_artifact_observation,
                launch.perf_artifact_observation,
            ] {
                frame_bounded_observation(&mut bytes, observation);
            }
            frame_process_status(&mut bytes, launch.process_status());
        }
    }
    raw_identity("wu0g-experiment-v1", bytes)
}

fn bind_plan_and_experiment_identities(raw: &mut RawThresholdEvidence) {
    let plan = plan_identity_from_raw(raw);
    raw.plan_identity = plan.clone();
    for dossier in &mut raw.cycle_profiles {
        dossier.plan_identity = plan.clone();
    }
    for dossier in &mut raw.predictions {
        dossier.plan_identity = plan.clone();
    }
    for dossier in &mut raw.controls {
        dossier.plan_identity = plan.clone();
    }
    for dossier in &mut raw.causal_process_dossiers {
        dossier.plan_identity = plan.clone();
        dossier.result_bound_plan_identity = plan.clone();
        dossier.result_bound_request_content_identity = dossier.request_content_identity.clone();
    }
    for pair in &mut raw.performance_pairs {
        for launch in &mut pair.launches {
            launch.plan_identity = plan.clone();
            launch.result_bound_plan_identity = plan.clone();
            launch.result_bound_request_content_identity = launch.request_content_identity.clone();
        }
    }
    raw.experiment_identity = experiment_identity_from_raw(raw);
}

fn visit_raw_identity_locations_mut(
    raw: &mut RawThresholdEvidence,
    visitor: &mut impl FnMut(String, &mut RawIdentityEvidence),
) {
    visitor("top.experiment".to_owned(), &mut raw.experiment_identity);
    visitor("top.plan".to_owned(), &mut raw.plan_identity);
    visitor("top.profile".to_owned(), &mut raw.profile_identity);
    visitor("top.universe".to_owned(), &mut raw.universe_identity);
    visitor("top.workload".to_owned(), &mut raw.workload_identity);
    visitor(
        "top.representative_workload".to_owned(),
        &mut raw.representative_workload_identity,
    );
    visitor("top.candidate".to_owned(), &mut raw.candidate_identity);
    visitor(
        "top.baseline_binary".to_owned(),
        &mut raw.baseline_binary_identity,
    );
    visitor(
        "top.candidate_binary".to_owned(),
        &mut raw.candidate_binary_identity,
    );
    visitor("top.prlimit".to_owned(), &mut raw.prlimit_identity);
    visitor("top.perf".to_owned(), &mut raw.perf_identity);
    visitor("top.host".to_owned(), &mut raw.host_identity);
    for (row_index, row) in raw.ladder_rows.iter_mut().enumerate() {
        for (mode, point) in [
            ("baseline", &mut row.baseline),
            ("candidate", &mut row.candidate),
        ] {
            visitor(
                format!("ladder.{row_index}.{mode}.rung"),
                &mut point.rung_identity,
            );
            visitor(
                format!("ladder.{row_index}.{mode}.report"),
                &mut point.report_identity,
            );
            visitor(
                format!("ladder.{row_index}.{mode}.semantic"),
                &mut point.semantic_identity,
            );
            for (component_index, component) in
                point.complete_component_identities.iter_mut().enumerate()
            {
                visitor(
                    format!("ladder.{row_index}.{mode}.component.{component_index}"),
                    component,
                );
            }
        }
    }
    for (index, dossier) in raw.cycle_profiles.iter_mut().enumerate() {
        for (name, identity) in [
            ("plan", &mut dossier.plan_identity),
            ("profile", &mut dossier.profile_identity),
            ("workload", &mut dossier.workload_identity),
            ("candidate", &mut dossier.candidate_identity),
            ("binary", &mut dossier.binary_identity),
            ("report", &mut dossier.report_identity),
            ("semantic", &mut dossier.semantic_identity),
            ("artifact", &mut dossier.artifact_identity),
            ("artifact_content", &mut dossier.artifact_content_identity),
        ] {
            visitor(format!("cycle.{index}.{name}"), identity);
        }
    }
    for (index, dossier) in raw.predictions.iter_mut().enumerate() {
        for (name, identity) in [
            ("plan", &mut dossier.plan_identity),
            ("report", &mut dossier.report_identity),
            ("profile", &mut dossier.profile_identity),
            ("workload", &mut dossier.workload_identity),
            ("candidate", &mut dossier.candidate_identity),
            ("binary", &mut dossier.binary_identity),
            ("semantic", &mut dossier.semantic_identity),
            ("artifact", &mut dossier.artifact_identity),
            ("artifact_content", &mut dossier.artifact_content_identity),
        ] {
            visitor(format!("prediction.{index}.{name}"), identity);
        }
    }
    for (index, dossier) in raw.controls.iter_mut().enumerate() {
        for (name, identity) in [
            ("plan", &mut dossier.plan_identity),
            ("profile", &mut dossier.profile_identity),
            ("workload", &mut dossier.workload_identity),
            ("candidate", &mut dossier.candidate_identity),
            ("baseline_binary", &mut dossier.baseline_binary_identity),
            ("candidate_binary", &mut dossier.candidate_binary_identity),
            ("baseline_report", &mut dossier.baseline_report_identity),
            ("candidate_report", &mut dossier.candidate_report_identity),
            ("baseline_semantic", &mut dossier.baseline_semantic_identity),
            (
                "candidate_semantic",
                &mut dossier.candidate_semantic_identity,
            ),
            ("baseline_artifact", &mut dossier.baseline_artifact_identity),
            (
                "candidate_artifact",
                &mut dossier.candidate_artifact_identity,
            ),
            (
                "baseline_artifact_content",
                &mut dossier.baseline_artifact_content_identity,
            ),
            (
                "candidate_artifact_content",
                &mut dossier.candidate_artifact_content_identity,
            ),
        ] {
            visitor(format!("control.{index}.{name}"), identity);
        }
    }
    for (index, dossier) in raw.causal_process_dossiers.iter_mut().enumerate() {
        for (name, identity) in [
            ("plan", &mut dossier.plan_identity),
            ("profile", &mut dossier.profile_identity),
            ("universe", &mut dossier.universe_identity),
            ("workload", &mut dossier.workload_identity),
            ("candidate", &mut dossier.candidate_identity),
            ("binary", &mut dossier.binary_identity),
            ("rung", &mut dossier.rung_identity),
            ("report", &mut dossier.report_identity),
            ("semantic", &mut dossier.semantic_identity),
            ("request_content", &mut dossier.request_content_identity),
            ("request_path", &mut dossier.request_path_identity),
            ("result", &mut dossier.result_identity),
            ("result_content", &mut dossier.result_content_identity),
            ("result_path", &mut dossier.result_path_identity),
            ("result_bound_plan", &mut dossier.result_bound_plan_identity),
            (
                "result_bound_request",
                &mut dossier.result_bound_request_content_identity,
            ),
            ("stdout", &mut dossier.stdout_identity),
            ("stderr", &mut dossier.stderr_identity),
            ("scope", &mut dossier.scope_identity),
            ("cgroup", &mut dossier.cgroup_identity),
            ("child", &mut dossier.child_identity),
            (
                "completion_sentinel",
                &mut dossier.completion_sentinel_identity,
            ),
            ("child_artifact", &mut dossier.child_artifact_identity),
            ("prlimit", &mut dossier.prlimit_identity),
        ] {
            visitor(format!("causal.{index}.{name}"), identity);
        }
    }
    for (pair_index, pair) in raw.performance_pairs.iter_mut().enumerate() {
        visitor(
            format!("performance.{pair_index}.pair"),
            &mut pair.pair_identity,
        );
        for (launch_index, launch) in pair.launches.iter_mut().enumerate() {
            for (name, identity) in [
                ("plan", &mut launch.plan_identity),
                (
                    "representative_workload",
                    &mut launch.representative_workload_identity,
                ),
                ("candidate", &mut launch.candidate_identity),
                ("binary", &mut launch.binary_identity),
                ("report", &mut launch.report_identity),
                ("semantic", &mut launch.semantic_identity),
                ("pair", &mut launch.pair_identity),
                ("launch", &mut launch.launch_identity),
                ("request_content", &mut launch.request_content_identity),
                ("request_path", &mut launch.request_path_identity),
                ("result", &mut launch.result_identity),
                ("result_content", &mut launch.result_content_identity),
                ("result_path", &mut launch.result_path_identity),
                ("result_bound_plan", &mut launch.result_bound_plan_identity),
                (
                    "result_bound_request",
                    &mut launch.result_bound_request_content_identity,
                ),
                ("stdout", &mut launch.stdout_identity),
                ("stderr", &mut launch.stderr_identity),
                ("scope", &mut launch.scope_identity),
                ("cgroup", &mut launch.cgroup_identity),
                ("child", &mut launch.child_identity),
                (
                    "completion_sentinel",
                    &mut launch.completion_sentinel_identity,
                ),
                ("child_artifact", &mut launch.child_artifact_identity),
                ("perf_artifact", &mut launch.perf_artifact_identity),
                ("prlimit", &mut launch.prlimit_identity),
                ("perf", &mut launch.perf_identity),
                ("host", &mut launch.host_identity),
            ] {
                visitor(
                    format!("performance.{pair_index}.{launch_index}.{name}"),
                    identity,
                );
            }
        }
    }
}

fn passing_raw_threshold_evidence() -> RawThresholdEvidence {
    let placeholder = raw_identity("wu0g-experiment-v1", Vec::new());
    let profile_identity = raw_identity("wu0g-profile-v1", b"synthetic-82-profile".to_vec());
    let universe_identity = raw_identity("wu0g-universe-v1", b"all-82-sources".to_vec());
    let workload_identity = raw_identity("wu0g-workload-v1", b"causal-ladder-workload".to_vec());
    let representative_workload_identity =
        raw_identity("wu0g-workload-v1", b"fixed-end-to-end-workload".to_vec());
    let candidate_identity = raw_identity("wu0g-candidate-v1", b"candidate-b-v1".to_vec());
    let baseline_binary_identity = raw_identity("wu0g-binary-v1", b"baseline-binary".to_vec());
    let candidate_binary_identity = raw_identity("wu0g-binary-v1", b"candidate-binary".to_vec());
    let prlimit_identity =
        raw_identity("wu0g-executable-v1", b"/usr/bin/prlimit synthetic".to_vec());
    let perf_identity = raw_identity("wu0g-executable-v1", b"/usr/bin/perf synthetic".to_vec());
    let host_identity = raw_identity("wu0g-host-v1", b"synthetic-host".to_vec());
    let perf_version = "perf version synthetic".to_owned();
    let cpu_affinity = "0".to_owned();
    let targets = [1_250_u16, 2_500, 5_000, 7_500, 10_000];
    let baseline_targets = [100_u64, 200, 300, 400, 500];
    let baseline_totals = [200_u64, 400, 600, 800, 1_000];
    let candidate_targets = [50_u64, 100, 150, 200, 250];
    let candidate_totals = [120_u64, 240, 360, 480, 600];
    let ladder_rows = targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| {
            let rung = raw_identity(
                "wu0g-rung-v1",
                format!("target={target};prefix={}", index + 1).into_bytes(),
            );
            let semantic = raw_identity(
                "wu0g-prefix-semantic-v1",
                format!("semantic-prefix-{}", index + 1).into_bytes(),
            );
            let complete = (0..=index)
                .map(|component| {
                    raw_identity(
                        "wu0g-component-v1",
                        format!("component-{component}").into_bytes(),
                    )
                })
                .collect::<Vec<_>>();
            RawLadderCounterRow {
                baseline: RawLadderCounterPoint {
                    rung_identity: rung.clone(),
                    report_identity: raw_identity(
                        "wu0g-report-v1",
                        format!("baseline-{target}").into_bytes(),
                    ),
                    semantic_identity: semantic.clone(),
                    target_counter: baseline_targets[index],
                    total_work_counter: baseline_totals[index],
                    complete_component_identities: complete.clone(),
                    saturated: false,
                },
                candidate: RawLadderCounterPoint {
                    rung_identity: rung,
                    report_identity: raw_identity(
                        "wu0g-report-v1",
                        format!("candidate-{target}").into_bytes(),
                    ),
                    semantic_identity: semantic,
                    target_counter: candidate_targets[index],
                    total_work_counter: candidate_totals[index],
                    complete_component_identities: complete,
                    saturated: false,
                },
            }
        })
        .collect::<Vec<_>>();
    let causal_process_dossiers = ladder_rows
        .iter()
        .enumerate()
        .flat_map(|(index, row)| {
            [RawProbeMode::Baseline, RawProbeMode::CandidateB]
                .into_iter()
                .map({
                    let placeholder = placeholder.clone();
                    let profile_identity = profile_identity.clone();
                    let universe_identity = universe_identity.clone();
                    let workload_identity = workload_identity.clone();
                    let candidate_identity = candidate_identity.clone();
                    let baseline_binary_identity = baseline_binary_identity.clone();
                    let candidate_binary_identity = candidate_binary_identity.clone();
                    let prlimit_identity = prlimit_identity.clone();
                    let row = row.clone();
                    move |mode| {
                        let point = match mode {
                            RawProbeMode::Baseline => &row.baseline,
                            RawProbeMode::CandidateB => &row.candidate,
                        };
                        let mode_ordinal = match mode {
                            RawProbeMode::Baseline => 0_u32,
                            RawProbeMode::CandidateB => 1,
                        };
                        let leader_pid = 1_000
                            + u32::try_from(index).expect("causal index fits u32") * 2
                            + mode_ordinal;
                        RawFreshProcessDossier {
                            plan_identity: placeholder.clone(),
                            profile_identity: profile_identity.clone(),
                            universe_identity: universe_identity.clone(),
                            workload_identity: workload_identity.clone(),
                            candidate_identity: candidate_identity.clone(),
                            binary_identity: match mode {
                                RawProbeMode::Baseline => baseline_binary_identity.clone(),
                                RawProbeMode::CandidateB => candidate_binary_identity.clone(),
                            },
                            rung_identity: point.rung_identity.clone(),
                            report_identity: point.report_identity.clone(),
                            semantic_identity: point.semantic_identity.clone(),
                            rung_ordinal: u8::try_from(index).expect("rung ordinal fits u8"),
                            mode,
                            canonical_invocation: vec![
                                "/usr/bin/perl".to_owned(),
                                "/fixture/tooling/wu0e-diagnostic/run.pl".to_owned(),
                                "--wu0g-child-v1".to_owned(),
                                format!("/fixture/request-causal-{index}-{mode:?}"),
                                format!("/fixture/result-causal-{index}-{mode:?}"),
                            ],
                            scope_identity: raw_identity(
                                "wu0g-scope-v1",
                                format!("causal-scope-{index}-{mode:?}").into_bytes(),
                            ),
                            cgroup_identity: raw_identity(
                                "wu0g-cgroup-v1",
                                format!("causal-cgroup-{index}-{mode:?}").into_bytes(),
                            ),
                            leader_pid,
                            leader_start_ticks: 10_000 + u64::from(leader_pid),
                            pgid: leader_pid,
                            readiness_seen: true,
                            membership_verified: true,
                            drain_complete: true,
                            cgroup_populated_zero: true,
                            pgid_empty: true,
                            cgroup_removed: true,
                            cgroup_retained: false,
                            scope_abort_requested: false,
                            scope_abort_observed: false,
                            deadline_ms: WU0G_DEADLINE_MS,
                            deadline_readback_ms: WU0G_DEADLINE_MS,
                            memory_limit_bytes: WU0G_MEMORY_LIMIT_BYTES,
                            memory_limit_readback_bytes: WU0G_MEMORY_LIMIT_BYTES,
                            rss_limit_bytes: WU0G_RSS_LIMIT_BYTES,
                            rss_limit_readback_bytes: WU0G_RSS_LIMIT_BYTES,
                            nofile_soft_limit: WU0G_NOFILE_LIMIT,
                            nofile_hard_limit: WU0G_NOFILE_LIMIT,
                            nofile_soft_readback: WU0G_NOFILE_LIMIT,
                            nofile_hard_readback: WU0G_NOFILE_LIMIT,
                            termination: RawProbeTermination::Complete,
                            max_rss_bytes: 128 * 1024 * 1024,
                            containment_failures: 0,
                            cgroup_oom_delta: 0,
                            cgroup_oom_kill_delta: 0,
                            exit_code: Some(0),
                            term_signal: None,
                            raw_wait_status: Some(0),
                            waited: true,
                            reaped: true,
                            cleanup_succeeded: true,
                            request_content_identity: raw_identity(
                                "wu0g-request-content-v1",
                                format!("causal-request-{index}-{mode:?}").into_bytes(),
                            ),
                            request_path_identity: raw_identity(
                                "wu0g-relative-path-v1",
                                format!("request-causal-{index}-{mode:?}").into_bytes(),
                            ),
                            result_identity: raw_identity(
                                "wu0g-result-v1",
                                format!("causal-result-{index}-{mode:?}").into_bytes(),
                            ),
                            result_content_identity: raw_identity(
                                "wu0g-result-content-v1",
                                format!("causal-result-content-{index}-{mode:?}").into_bytes(),
                            ),
                            result_path_identity: raw_identity(
                                "wu0g-relative-path-v1",
                                format!("result-causal-{index}-{mode:?}").into_bytes(),
                            ),
                            result_bound_plan_identity: placeholder.clone(),
                            result_bound_request_content_identity: raw_identity(
                                "wu0g-request-content-v1",
                                format!("causal-request-{index}-{mode:?}").into_bytes(),
                            ),
                            stdout_identity: raw_identity(
                                "wu0g-stdout-v1",
                                format!("causal-stdout-{index}-{mode:?}").into_bytes(),
                            ),
                            stderr_identity: raw_identity(
                                "wu0g-stderr-v1",
                                format!("causal-stderr-{index}-{mode:?}").into_bytes(),
                            ),
                            child_identity: raw_identity(
                                "wu0g-child-v1",
                                format!("causal-{index}-{mode:?}").into_bytes(),
                            ),
                            completion_sentinel_identity: raw_identity(
                                "wu0g-child-completion-sentinel-v1",
                                format!("causal-sentinel-{index}-{mode:?}").into_bytes(),
                            ),
                            child_artifact_identity: raw_identity(
                                "wu0g-child-artifact-v1",
                                format!("causal-artifact-{index}-{mode:?}").into_bytes(),
                            ),
                            prlimit_identity: prlimit_identity.clone(),
                            stdout_observation: bounded_observation(WU0G_STDOUT_CAP_BYTES, 128),
                            stderr_observation: bounded_observation(WU0G_STDERR_CAP_BYTES, 0),
                            request_observation: bounded_observation(WU0G_REQUEST_CAP_BYTES, 1_024),
                            result_observation: bounded_observation(WU0G_RESULT_CAP_BYTES, 2_048),
                            sentinel_observation: bounded_observation(WU0G_SENTINEL_CAP_BYTES, 512),
                            semantic_artifact_observation: bounded_observation(
                                WU0G_ARTIFACT_CAP_BYTES,
                                4_096,
                            ),
                        }
                    }
                })
        })
        .collect::<Vec<_>>();
    let cycle_profiles = [3_u64, 4, 5]
        .into_iter()
        .enumerate()
        .map(|(index, explained)| RawCycleProfileDossier {
            plan_identity: placeholder.clone(),
            profile_identity: raw_identity(
                "wu0g-profile-v1",
                format!("cycle-profile-{index}").into_bytes(),
            ),
            workload_identity: raw_identity(
                "wu0g-workload-v1",
                format!("cycle-workload-{index}").into_bytes(),
            ),
            candidate_identity: candidate_identity.clone(),
            binary_identity: candidate_binary_identity.clone(),
            mode: RawProbeMode::CandidateB,
            report_identity: raw_identity(
                "wu0g-report-v1",
                format!("cycle-report-{index}").into_bytes(),
            ),
            semantic_identity: raw_identity(
                "wu0g-semantic-v1",
                format!("cycle-semantic-{index}").into_bytes(),
            ),
            artifact_identity: raw_identity(
                "wu0g-artifact-v1",
                format!("cycle-artifact-{index}").into_bytes(),
            ),
            artifact_content_identity: raw_identity(
                "wu0g-artifact-content-v1",
                format!("cycle-artifact-content-{index}").into_bytes(),
            ),
            explained_cycle_visits: explained,
            total_cycle_visits: 10,
        })
        .collect::<Vec<_>>();
    let prediction = RawPredictionDossier {
        plan_identity: placeholder.clone(),
        report_identity: raw_identity("wu0g-report-v1", b"prediction-report".to_vec()),
        profile_identity: profile_identity.clone(),
        workload_identity: representative_workload_identity.clone(),
        candidate_identity: candidate_identity.clone(),
        binary_identity: candidate_binary_identity.clone(),
        mode: RawProbeMode::CandidateB,
        semantic_identity: raw_identity("wu0g-semantic-v1", b"prediction-semantic".to_vec()),
        artifact_identity: raw_identity("wu0g-artifact-v1", b"prediction-artifact".to_vec()),
        artifact_content_identity: raw_identity(
            "wu0g-artifact-content-v1",
            b"prediction-artifact-content".to_vec(),
        ),
        total_end_to_end_work: 1_000,
        attributed_target_work: 500,
        baseline_target_work: 500,
        candidate_target_work: 300,
    };
    let controls = [10_200_u64, 10_100, 10_000]
        .into_iter()
        .enumerate()
        .map(|(index, candidate_measurement)| RawControlPairDossier {
            plan_identity: placeholder.clone(),
            profile_identity: raw_identity(
                "wu0g-profile-v1",
                format!("control-profile-{index}").into_bytes(),
            ),
            workload_identity: raw_identity(
                "wu0g-control-workload-v1",
                format!("control-{index}").into_bytes(),
            ),
            candidate_identity: candidate_identity.clone(),
            baseline_binary_identity: baseline_binary_identity.clone(),
            candidate_binary_identity: candidate_binary_identity.clone(),
            baseline_report_identity: raw_identity(
                "wu0g-report-v1",
                format!("control-baseline-report-{index}").into_bytes(),
            ),
            candidate_report_identity: raw_identity(
                "wu0g-report-v1",
                format!("control-candidate-report-{index}").into_bytes(),
            ),
            baseline_semantic_identity: raw_identity(
                "wu0g-semantic-v1",
                format!("control-semantic-{index}").into_bytes(),
            ),
            candidate_semantic_identity: raw_identity(
                "wu0g-semantic-v1",
                format!("control-semantic-{index}").into_bytes(),
            ),
            baseline_artifact_identity: raw_identity(
                "wu0g-artifact-v1",
                format!("control-baseline-{index}").into_bytes(),
            ),
            candidate_artifact_identity: raw_identity(
                "wu0g-artifact-v1",
                format!("control-candidate-{index}").into_bytes(),
            ),
            baseline_artifact_content_identity: raw_identity(
                "wu0g-artifact-content-v1",
                format!("control-baseline-content-{index}").into_bytes(),
            ),
            candidate_artifact_content_identity: raw_identity(
                "wu0g-artifact-content-v1",
                format!("control-candidate-content-{index}").into_bytes(),
            ),
            baseline_measurement: 10_000,
            candidate_measurement,
        })
        .collect::<Vec<_>>();
    let performance_pairs = (0_u8..5)
        .map(|pair_ordinal| {
            let order = if pair_ordinal % 2 == 0 {
                RawLaunchOrder::Ab
            } else {
                RawLaunchOrder::Ba
            };
            let pair_identity = raw_identity(
                "wu0g-performance-pair-v1",
                format!("performance-pair-{pair_ordinal}").into_bytes(),
            );
            let modes = match order {
                RawLaunchOrder::Ab => [RawProbeMode::Baseline, RawProbeMode::CandidateB],
                RawLaunchOrder::Ba => [RawProbeMode::CandidateB, RawProbeMode::Baseline],
            };
            let launches = modes
                .into_iter()
                .enumerate()
                .map(|(position, mode)| {
                    let launch_ordinal = pair_ordinal
                        .checked_mul(2)
                        .and_then(|value| {
                            value.checked_add(u8::try_from(position).expect("position fits u8"))
                        })
                        .expect("launch ordinal fits u8");
                    RawPerformanceLaunch {
                        plan_identity: placeholder.clone(),
                        representative_workload_identity: representative_workload_identity.clone(),
                        candidate_identity: candidate_identity.clone(),
                        binary_identity: match mode {
                            RawProbeMode::Baseline => baseline_binary_identity.clone(),
                            RawProbeMode::CandidateB => candidate_binary_identity.clone(),
                        },
                        report_identity: raw_identity(
                            "wu0g-report-v1",
                            format!("performance-{pair_ordinal}-{mode:?}").into_bytes(),
                        ),
                        semantic_identity: raw_identity(
                            "wu0g-end-to-end-semantic-v1",
                            b"fixed-end-to-end-semantic".to_vec(),
                        ),
                        pair_identity: pair_identity.clone(),
                        launch_identity: raw_identity(
                            "wu0g-performance-launch-v1",
                            format!("performance-launch-{launch_ordinal}").into_bytes(),
                        ),
                        request_content_identity: raw_identity(
                            "wu0g-request-content-v1",
                            format!("performance-request-{launch_ordinal}").into_bytes(),
                        ),
                        request_path_identity: raw_identity(
                            "wu0g-relative-path-v1",
                            format!("request-performance-{launch_ordinal}").into_bytes(),
                        ),
                        result_identity: raw_identity(
                            "wu0g-result-v1",
                            format!("performance-result-{launch_ordinal}").into_bytes(),
                        ),
                        result_content_identity: raw_identity(
                            "wu0g-result-content-v1",
                            format!("performance-result-content-{launch_ordinal}").into_bytes(),
                        ),
                        result_path_identity: raw_identity(
                            "wu0g-relative-path-v1",
                            format!("result-performance-{launch_ordinal}").into_bytes(),
                        ),
                        result_bound_plan_identity: placeholder.clone(),
                        result_bound_request_content_identity: raw_identity(
                            "wu0g-request-content-v1",
                            format!("performance-request-{launch_ordinal}").into_bytes(),
                        ),
                        child_identity: raw_identity(
                            "wu0g-child-v1",
                            format!("performance-child-{launch_ordinal}").into_bytes(),
                        ),
                        completion_sentinel_identity: raw_identity(
                            "wu0g-child-completion-sentinel-v1",
                            format!("performance-sentinel-{launch_ordinal}").into_bytes(),
                        ),
                        child_artifact_identity: raw_identity(
                            "wu0g-child-artifact-v1",
                            format!("performance-artifact-{launch_ordinal}").into_bytes(),
                        ),
                        perf_artifact_identity: raw_identity(
                            "wu0g-perf-artifact-v1",
                            format!("performance-count-{launch_ordinal}").into_bytes(),
                        ),
                        prlimit_identity: prlimit_identity.clone(),
                        perf_identity: perf_identity.clone(),
                        host_identity: host_identity.clone(),
                        perf_version: perf_version.clone(),
                        perf_event: WU0G_PERF_EVENT.to_owned(),
                        cpu_affinity: cpu_affinity.clone(),
                        pair_ordinal,
                        launch_ordinal,
                        mode,
                        canonical_invocation: vec![
                            "/usr/bin/perl".to_owned(),
                            "/fixture/tooling/wu0e-diagnostic/run.pl".to_owned(),
                            "--wu0g-child-v1".to_owned(),
                            format!("/fixture/request-performance-{launch_ordinal}"),
                            format!("/fixture/result-performance-{launch_ordinal}"),
                        ],
                        perf_invocation: vec![
                            "/usr/bin/perf".to_owned(),
                            "stat".to_owned(),
                            "--no-big-num".to_owned(),
                            "--no-scale".to_owned(),
                            "-x".to_owned(),
                            ";".to_owned(),
                            "-e".to_owned(),
                            WU0G_PERF_EVENT.to_owned(),
                            "--log-fd".to_owned(),
                            "198".to_owned(),
                            "--".to_owned(),
                            "/proc/self/fd/197".to_owned(),
                            "--ignored".to_owned(),
                            "--exact".to_owned(),
                            "check::checker::decls::wu0g_interface_fill_attribution_spec::wu0g_hardened_child_once".to_owned(),
                            "--nocapture".to_owned(),
                        ],
                        scope_identity: raw_identity(
                            "wu0g-scope-v1",
                            format!("performance-scope-{launch_ordinal}").into_bytes(),
                        ),
                        cgroup_identity: raw_identity(
                            "wu0g-cgroup-v1",
                            format!("performance-cgroup-{launch_ordinal}").into_bytes(),
                        ),
                        leader_pid: 2_000 + u32::from(launch_ordinal),
                        leader_start_ticks: 20_000 + u64::from(launch_ordinal),
                        pgid: 2_000 + u32::from(launch_ordinal),
                        readiness_seen: true,
                        membership_verified: true,
                        drain_complete: true,
                        cgroup_populated_zero: true,
                        pgid_empty: true,
                        cgroup_removed: true,
                        cgroup_retained: false,
                        scope_abort_requested: false,
                        scope_abort_observed: false,
                        deadline_ms: WU0G_DEADLINE_MS,
                        deadline_readback_ms: WU0G_DEADLINE_MS,
                        memory_limit_bytes: WU0G_MEMORY_LIMIT_BYTES,
                        memory_limit_readback_bytes: WU0G_MEMORY_LIMIT_BYTES,
                        rss_limit_bytes: WU0G_RSS_LIMIT_BYTES,
                        rss_limit_readback_bytes: WU0G_RSS_LIMIT_BYTES,
                        nofile_soft_limit: WU0G_NOFILE_LIMIT,
                        nofile_hard_limit: WU0G_NOFILE_LIMIT,
                        nofile_soft_readback: WU0G_NOFILE_LIMIT,
                        nofile_hard_readback: WU0G_NOFILE_LIMIT,
                        termination: RawProbeTermination::Complete,
                        wall_ns: match mode {
                            RawProbeMode::Baseline => 1_000,
                            RawProbeMode::CandidateB => 800,
                        },
                        instructions: match mode {
                            RawProbeMode::Baseline => 10_000,
                            RawProbeMode::CandidateB => 8_000,
                        },
                        perf_runtime: 1_000,
                        max_rss_bytes: 128 * 1024 * 1024,
                        containment_failures: 0,
                        cgroup_oom_delta: 0,
                        cgroup_oom_kill_delta: 0,
                        exit_code: Some(0),
                        term_signal: None,
                        raw_wait_status: Some(0),
                        perf_raw_wait_status: Some(0),
                        perf_exit_code: Some(0),
                        perf_term_signal: None,
                        perf_waited: true,
                        waited: true,
                        reaped: true,
                        cleanup_succeeded: true,
                        stdout_identity: raw_identity(
                            "wu0g-stdout-v1",
                            format!("performance-stdout-{launch_ordinal}").into_bytes(),
                        ),
                        stderr_identity: raw_identity(
                            "wu0g-stderr-v1",
                            format!("performance-stderr-{launch_ordinal}").into_bytes(),
                        ),
                        stdout_observation: bounded_observation(WU0G_STDOUT_CAP_BYTES, 128),
                        stderr_observation: bounded_observation(WU0G_STDERR_CAP_BYTES, 0),
                        request_observation: bounded_observation(WU0G_REQUEST_CAP_BYTES, 1_024),
                        result_observation: bounded_observation(WU0G_RESULT_CAP_BYTES, 2_048),
                        sentinel_observation: bounded_observation(WU0G_SENTINEL_CAP_BYTES, 512),
                        semantic_artifact_observation: bounded_observation(
                            WU0G_ARTIFACT_CAP_BYTES,
                            4_096,
                        ),
                        perf_artifact_observation: bounded_observation(
                            WU0G_PERF_ARTIFACT_CAP_BYTES,
                            48,
                        ),
                    }
                })
                .collect();
            RawPerformancePairDossier {
                pair_identity,
                pair_ordinal,
                order,
                launches,
            }
        })
        .collect::<Vec<_>>();
    let mut raw = RawThresholdEvidence {
        experiment_identity: placeholder,
        plan_identity: raw_identity("wu0g-plan-v1", Vec::new()),
        profile_identity,
        universe_identity,
        workload_identity,
        representative_workload_identity,
        candidate_identity,
        baseline_binary_identity,
        candidate_binary_identity,
        prlimit_identity,
        perf_identity,
        host_identity,
        perf_version,
        perf_event: WU0G_PERF_EVENT.to_owned(),
        cpu_affinity,
        ladder_rows,
        cycle_profiles,
        predictions: vec![prediction],
        controls,
        causal_process_dossiers,
        performance_pairs,
    };
    bind_plan_and_experiment_identities(&mut raw);
    raw
}

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
        Some(&run.attribution.scalar_snapshot_for_test()),
        "the final whole-component checkpoint is the completed scalar/digest snapshot"
    );
    assert_eq!(run.completion.validate_exact(), Ok(()));
    assert!(run.completion.is_complete());
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

fn source_window<'a>(source: &'a str, start_needle: &str, end_needle: &str) -> &'a str {
    let start = *code_offsets(source, start_needle)
        .first()
        .unwrap_or_else(|| panic!("missing source window start {start_needle}"));
    let body_start = start + start_needle.len();
    let end = code_offsets(&source[body_start..], end_needle)
        .first()
        .map(|offset| body_start + offset)
        .unwrap_or_else(|| panic!("missing source window end {end_needle}"));
    &source[body_start..end]
}

fn source_item_body<'a>(source: &'a str, owner: &str) -> &'a str {
    let matches = code_token_offsets(source, owner);
    assert_eq!(matches.len(), 1, "source item owner is unique: {owner}");
    let owner_start = matches
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("missing source item {owner}"));
    let open = code_offsets(&source[owner_start..], "{")
        .into_iter()
        .next()
        .map(|offset| owner_start + offset)
        .expect("source item body");
    let end = delimited_end(source, open, b'{', b'}')
        .unwrap_or_else(|| panic!("unterminated source item {owner}"));
    &source[open..end]
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn code_token_offsets(text: &str, needle: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let needle_bytes = needle.as_bytes();
    code_offsets(text, needle)
        .into_iter()
        .filter(|offset| {
            let starts_with_identifier = needle_bytes
                .first()
                .is_some_and(|byte| is_identifier_byte(*byte));
            let ends_with_identifier = needle_bytes
                .last()
                .is_some_and(|byte| is_identifier_byte(*byte));
            (!starts_with_identifier || *offset == 0 || !is_identifier_byte(bytes[*offset - 1]))
                && (!ends_with_identifier
                    || bytes
                        .get(*offset + needle_bytes.len())
                        .is_none_or(|byte| !is_identifier_byte(*byte)))
        })
        .collect()
}

fn macro_arm_body<'a>(source: &'a str, arm_anchor: &str) -> &'a str {
    let anchor = *code_offsets(source, arm_anchor)
        .first()
        .unwrap_or_else(|| panic!("missing macro arm {arm_anchor}"));
    let arrow = code_offsets(&source[anchor..], "=>")
        .first()
        .map(|offset| anchor + offset)
        .expect("macro arm arrow");
    let open = code_offsets(&source[arrow..], "{")
        .first()
        .map(|offset| arrow + offset)
        .expect("macro arm body");
    let end = delimited_end(source, open, b'{', b'}').expect("terminated macro arm body");
    &source[open..end]
}

fn skip_quoted(bytes: &[u8], quote: usize) -> usize {
    let mut cursor = quote + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            b'"' => return cursor + 1,
            _ => cursor += 1,
        }
    }
    bytes.len()
}

fn raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let hashes_start = cursor;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    let hashes = cursor - hashes_start;
    cursor += 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes.get(cursor + 1..cursor + 1 + hashes)
                == Some(&bytes[hashes_start..hashes_start + hashes])
        {
            return Some(cursor + 1 + hashes);
        }
        cursor += 1;
    }
    Some(bytes.len())
}

fn char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    if cursor >= bytes.len() || bytes[cursor] == b'\n' {
        return None;
    }
    if bytes[cursor] == b'\\' {
        cursor = (cursor + 2).min(bytes.len());
    } else {
        cursor += 1;
        while cursor < bytes.len() && (bytes[cursor] & 0b1100_0000) == 0b1000_0000 {
            cursor += 1;
        }
    }
    (bytes.get(cursor) == Some(&b'\'')).then_some(cursor + 1)
}

fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start + 2;
    let mut depth = 1_u32;
    while cursor < bytes.len() {
        if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            depth += 1;
            cursor += 2;
        } else if bytes.get(cursor..cursor + 2) == Some(b"*/") {
            depth -= 1;
            cursor += 2;
            if depth == 0 {
                return cursor;
            }
        } else {
            cursor += 1;
        }
    }
    bytes.len()
}

fn code_offsets(text: &str, needle: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let needle = needle.as_bytes();
    let mut offsets = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes.get(cursor..cursor + 2) == Some(b"//") {
            cursor = bytes[cursor..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| cursor + offset + 1);
        } else if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            cursor = block_comment_end(bytes, cursor);
        } else if let Some(end) = raw_string_end(bytes, cursor) {
            cursor = end;
        } else if bytes[cursor] == b'"' {
            cursor = skip_quoted(bytes, cursor);
        } else if bytes[cursor] == b'\'' {
            cursor = char_literal_end(bytes, cursor).unwrap_or(cursor + 1);
        } else if bytes.get(cursor..cursor + needle.len()) == Some(needle) {
            offsets.push(cursor);
            cursor += needle.len();
        } else {
            cursor += 1;
        }
    }
    offsets
}

fn code_brace_depth_at(text: &str, target: usize) -> u32 {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    let mut depth = 0_u32;
    while cursor < target {
        if bytes.get(cursor..cursor + 2) == Some(b"//") {
            cursor = bytes[cursor..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| cursor + offset + 1);
        } else if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            cursor = block_comment_end(bytes, cursor);
        } else if let Some(end) = raw_string_end(bytes, cursor) {
            cursor = end;
        } else if bytes[cursor] == b'"' {
            cursor = skip_quoted(bytes, cursor);
        } else if bytes[cursor] == b'\'' {
            cursor = char_literal_end(bytes, cursor).unwrap_or(cursor + 1);
        } else {
            match bytes[cursor] {
                b'{' => depth += 1,
                b'}' => depth = depth.checked_sub(1).expect("balanced source body"),
                _ => {}
            }
            cursor += 1;
        }
    }
    depth
}

fn top_level_receiver_calls(body: &str, receivers: &[&str]) -> Vec<String> {
    let mut calls = receivers
        .iter()
        .flat_map(|receiver| {
            let needle = format!("{receiver}.");
            code_offsets(body, &needle)
                .into_iter()
                .filter(|offset| code_brace_depth_at(body, *offset) == 1)
                .map(move |offset| (offset, needle.len()))
        })
        .filter_map(|(offset, prefix_len)| {
            let start = offset + prefix_len;
            let end = body.as_bytes()[start..]
                .iter()
                .position(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'))
                .map_or(body.len(), |length| start + length);
            let mut call = end;
            while body
                .as_bytes()
                .get(call)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                call += 1;
            }
            (body.as_bytes().get(call) == Some(&b'('))
                .then(|| (offset, body[start..end].to_owned()))
        })
        .collect::<Vec<_>>();
    calls.sort_by_key(|(offset, _)| *offset);
    calls.into_iter().map(|(_, method)| method).collect()
}

fn executable_calls(body: &str) -> Vec<(u32, String)> {
    let bytes = body.as_bytes();
    let mut calls = Vec::new();
    for open in code_offsets(body, "(") {
        let mut end = open;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0
            && (is_identifier_byte(bytes[start - 1]) || matches!(bytes[start - 1], b'.' | b':'))
        {
            start -= 1;
        }
        if start == end {
            continue;
        }
        let call = &body[start..end];
        let leaf = call.rsplit(['.', ':']).next().expect("call leaf");
        if matches!(leaf, "if" | "while" | "for" | "match") {
            continue;
        }
        calls.push((code_brace_depth_at(body, start), call.to_owned()));
    }
    calls
}

fn executable_assignment_targets(body: &str) -> Vec<String> {
    code_offsets(body, "=")
        .into_iter()
        .filter(|offset| {
            let bytes = body.as_bytes();
            !matches!(
                bytes.get(offset.saturating_sub(1)),
                Some(b'=' | b'!' | b'<' | b'>')
            ) && bytes.get(offset + 1) != Some(&b'=')
        })
        .map(|offset| {
            body[..offset]
                .lines()
                .next_back()
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .collect()
}

fn type_aliases_whose_rhs_contains(text: &str, needle: &str) -> Vec<String> {
    code_token_offsets(text, "type")
        .into_iter()
        .filter_map(|start| {
            let tail = &text[start..];
            let equals = code_offsets(tail, "=").first().copied()?;
            let end = code_offsets(tail, ";").first().copied()?;
            (equals < end && tail[equals + 1..end].contains(needle))
                .then(|| tail[..=end].to_owned())
        })
        .collect()
}

fn delimited_end(text: &str, start: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut cursor = start;
    let mut depth = 0_u32;
    while cursor < bytes.len() {
        if bytes.get(cursor..cursor + 2) == Some(b"//") {
            cursor = bytes[cursor..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| cursor + offset + 1);
            continue;
        }
        if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            cursor = block_comment_end(bytes, cursor);
            continue;
        }
        if let Some(end) = raw_string_end(bytes, cursor) {
            cursor = end;
            continue;
        }
        if bytes[cursor] == b'"' {
            cursor = skip_quoted(bytes, cursor);
            continue;
        }
        if bytes[cursor] == b'\'' {
            cursor = char_literal_end(bytes, cursor).unwrap_or(cursor + 1);
            continue;
        }
        match bytes[cursor] {
            byte if byte == open => depth += 1,
            byte if byte == close => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(cursor + 1);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn attribute_blocks(text: &str, name: &[u8]) -> Vec<(usize, usize, String)> {
    code_offsets(text, "#")
        .into_iter()
        .filter_map(|start| {
            let bytes = text.as_bytes();
            let mut cursor = start + 1;
            while bytes
                .get(cursor)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'!') {
                cursor += 1;
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| byte.is_ascii_whitespace())
                {
                    cursor += 1;
                }
            }
            if bytes.get(cursor) != Some(&b'[') {
                return None;
            }
            let bracket = cursor;
            cursor += 1;
            while bytes
                .get(cursor)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                cursor += 1;
            }
            if bytes.get(cursor..cursor + name.len()) != Some(name) {
                return None;
            }
            cursor += name.len();
            while bytes
                .get(cursor)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                cursor += 1;
            }
            if bytes.get(cursor) != Some(&b'(') {
                return None;
            }
            Some((start, bracket))
        })
        .map(|(start, bracket)| {
            let end = delimited_end(text, bracket, b'[', b']')
                .unwrap_or_else(|| panic!("unterminated cfg attribute at byte {start}"));
            (start, end, compact(&text[start..end]))
        })
        .collect()
}

fn cfg_attribute_blocks(text: &str) -> Vec<(usize, usize, String)> {
    attribute_blocks(text, b"cfg")
}

fn cfg_owned_code(text: &str, attribute_end: usize) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut start = attribute_end;
    loop {
        while bytes
            .get(start)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            start += 1;
        }
        if bytes.get(start..start + 2) == Some(b"//") {
            start = bytes[start..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| start + offset + 1);
        } else if bytes.get(start..start + 2) == Some(b"/*") {
            start = block_comment_end(bytes, start);
        } else {
            break;
        }
    }
    if start >= bytes.len() {
        return Err("cfg attribute owns no code".to_owned());
    }
    if bytes[start] == b'{' {
        let end = delimited_end(text, start, b'{', b'}')
            .ok_or_else(|| "unterminated cfg-owned block".to_owned())?;
        return Ok(text[start..end].to_owned());
    }
    let end = bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start + offset);
    Ok(text[start..end].trim().to_owned())
}

fn cfg_owned_anchor(text: &str, attribute_end: usize) -> Result<String, String> {
    let owned = cfg_owned_code(text, attribute_end)?;
    let mut lines = owned.lines().map(str::trim).filter(|line| !line.is_empty());
    let first = lines
        .next()
        .ok_or_else(|| "empty cfg-owned code".to_owned())?;
    let anchor = if first == "{" {
        lines
            .next()
            .ok_or_else(|| "empty cfg-owned block".to_owned())?
    } else {
        first
    };
    Ok(compact(anchor))
}

fn cfg_macro_invocations(text: &str) -> Vec<String> {
    code_offsets(text, "cfg!")
        .into_iter()
        .filter(|start| {
            let bytes = text.as_bytes();
            if *start > 0
                && (bytes[*start - 1].is_ascii_alphanumeric() || bytes[*start - 1] == b'_')
            {
                return false;
            }
            let mut cursor = *start + 4;
            while bytes
                .get(cursor)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                cursor += 1;
            }
            bytes.get(cursor) == Some(&b'(')
        })
        .map(|start| {
            let end = delimited_end(text, start, b'(', b')')
                .unwrap_or_else(|| panic!("unterminated cfg! at byte {start}"));
            compact(&text[start..end])
        })
        .collect()
}

fn rust_sources_under_src() -> Vec<(String, String)> {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pending = vec![repository.join("src")];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let mut entries = std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("walk {}: {error}", directory.display()))
            .map(|entry| entry.unwrap_or_else(|error| panic!("walk entry: {error}")))
            .collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(repository)
                .expect("src path remains below repository")
                .to_str()
                .unwrap_or_else(|| panic!("non-UTF-8 source path: {}", path.display()))
                .replace('\\', "/");
            let metadata = std::fs::symlink_metadata(&path)
                .unwrap_or_else(|error| panic!("metadata {}: {error}", path.display()));
            let kind = metadata.file_type();
            if kind.is_symlink() {
                panic!("symlink under audited src tree: {}", path.display());
            }
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if !kind.is_file()
                || path.extension().and_then(|extension| extension.to_str()) != Some("rs")
            {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            files.push((relative, text));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn read_repo_source(relative: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
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
    assert!(MAX_TRACKED_CHECKPOINT_BYTES >= MAX_COMPONENT_CHECKPOINTS);
    assert!(MAX_TRACKED_CHECKPOINT_BYTES <= 256 * 1024 * 1024);

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
    let types_owned = read_repo_source("src/types/substitute/wu0g.rs");
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
    assert!(sidecar.contains("MAX_TRACKED_CHECKPOINT_BYTES"));
    assert!(sidecar.contains("checked_add"));
    assert!(sidecar.contains("saturated"));
    let compact_guard = compact(HOT_GUARD);
    let compact_decls = compact(decls);
    assert!(compact_decls.contains(&format!(
        "{compact_guard}pub(incrate::check::checker)modwu0g_interface_fill_attribution;"
    )));
    assert!(compact_decls.contains(&format!(
        "{compact_guard}modwu0g_interface_fill_attribution_spec;"
    )));

    let recursive_window = source_item_body(substitution, "pub fn apply(&mut self");
    assert!(recursive_window.contains("if self.map.is_empty()"));
    assert!(recursive_window.contains("if self.in_progress.contains(&ty)"));
    assert!(recursive_window.contains("self.completed.get(&key)"));
    assert!(recursive_window.contains("match interner.store().tag(ty)"));
    let original_order = [
        "if self.map.is_empty()",
        "if self.in_progress.contains(&ty)",
        "let key = (ty, self.canonical_blocked_context())",
        "self.completed.get(&key)",
        "let start_cycle_epoch = self.cycle_epoch",
        "match interner.store().tag(ty)",
        "self.completed.insert(key, result)",
    ]
    .into_iter()
    .map(|landmark| {
        let offsets = code_offsets(recursive_window, landmark);
        assert_eq!(offsets.len(), 1, "exact apply landmark {landmark}");
        offsets[0]
    })
    .collect::<Vec<_>>();
    assert!(original_order.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(
        !substitution.contains("fn apply_inner"),
        "default/control retain the original apply owner and recursive frame shape"
    );
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
    for recursive_source in [recursive_window, apply_arms, types_owned.as_str()] {
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
    assert!(
        !types_owned.contains("crate::check"),
        "the isolated types-owned WU0G accumulator cannot depend upward"
    );
    assert!(!apply_arms.contains("crate::check"));
    let compact_substitution = compact(substitution);
    let allowed_wu0c_upward_dependencies = [
        "crate::check::checker::SubstitutionAttribution",
        "crate::check::checker::capture_wu0c_substitution_attribution(map)",
    ];
    assert_eq!(
        code_offsets(substitution, "crate::check").len(),
        allowed_wu0c_upward_dependencies.len(),
        "only the unchanged pre-existing WU0C references are allowlisted"
    );
    for allowed in allowed_wu0c_upward_dependencies {
        assert_eq!(compact_substitution.matches(allowed).count(), 1);
    }
    let substitution_fields = source_item_body(substitution, "pub struct Substitution");
    let wu0g_fields = substitution_fields
        .lines()
        .filter(|line| line.contains("wu0g:"))
        .map(compact)
        .collect::<Vec<_>>();
    assert_eq!(wu0g_fields, ["wu0g:wu0g::SubstitutionAccumulator,"]);
    let scalar = source_item_body(types_owned.as_str(), "struct SubstitutionAccumulator");
    let fields = scalar
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(','))
        .map(compact)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        fields,
        [
            "visits:u64,",
            "current_depth:u64,",
            "max_depth:u64,",
            "cycle_reentries:u64,",
            "copied_objects:u64,",
            "copied_properties:u64,",
            "copied_property_name_bytes:u64,",
            "copied_signatures:u64,",
            "interner_attempts:u64,",
            "interner_hits:u64,",
            "interner_new_ids:u64,",
            "saturated:bool,",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        "the types layer owns exactly one scalar-only application accumulator"
    );
    for forbidden in [
        "Atomic",
        "Mutex",
        "RwLock",
        "OnceLock",
        "OnceCell",
        "thread_local",
        "std::cell",
        "std::io",
        "std::fs",
        "parking_lot",
    ] {
        assert!(
            !types_owned.contains(forbidden),
            "types-owned hook aliases {forbidden}"
        );
    }
    assert!(!decls.contains("use wu0g_interface_fill_attribution::SubstitutionAccumulator"));
    assert!(!sidecar.contains("struct SubstitutionAccumulator"));
}

#[test]
fn feature_surface_is_reverse_closed_and_preserves_the_original_apply_call_graph() {
    let expected = [
        (
            "src/check/checker/context.rs",
            HOT_GUARD,
            "pub(in crate::check::checker) wu0g_attribution:",
        ),
        (
            "src/check/checker/mod.rs",
            HOT_GUARD,
            "wu0g_attribution: None,",
        ),
        (
            "src/check/checker/decls/mod.rs",
            HOT_GUARD,
            "pub(in crate::check::checker) mod wu0g_interface_fill_attribution;",
        ),
        (
            "src/check/checker/decls/mod.rs",
            HOT_GUARD,
            "mod wu0g_interface_fill_attribution_spec;",
        ),
        (
            "src/check/checker/decls/mod.rs",
            HOT_GUARD,
            "if !self.wu0g_admit_component_before_construction(&component) {",
        ),
        (
            "src/check/checker/decls/mod.rs",
            HOT_GUARD,
            "if self.wu0g_record_component_boundary(&component) {",
        ),
        (
            "src/check/checker/decls/resolve.rs",
            HOT_GUARD,
            "if let Some(outcome) = self.wu0g_application_resolve(",
        ),
        ("src/types/substitute/mod.rs", HOT_GUARD, "mod wu0g;"),
        (
            "src/types/substitute/mod.rs",
            HOT_GUARD,
            "macro_rules! wu0g_record {",
        ),
        (
            "src/types/substitute/mod.rs",
            HOT_FALLBACK_GUARD,
            "macro_rules! wu0g_record {",
        ),
        (
            "src/types/substitute/mod.rs",
            HOT_GUARD,
            "wu0g: wu0g::SubstitutionAccumulator,",
        ),
        (
            "src/types/substitute/mod.rs",
            HOT_GUARD,
            "wu0g: wu0g::SubstitutionAccumulator::default(),",
        ),
    ];

    let mut observed = Vec::new();
    for (relative, source) in rust_sources_under_src() {
        assert!(
            attribute_blocks(&source, b"cfg_attr")
                .iter()
                .all(|(_, _, attribute)| !attribute.contains(ATTRIBUTION_FEATURE)),
            "feature-bearing cfg_attr in {relative}"
        );
        assert!(
            cfg_macro_invocations(&source)
                .iter()
                .all(|invocation| !invocation.contains(ATTRIBUTION_FEATURE)),
            "feature-bearing cfg! in {relative}"
        );
        for (_, end, guard) in cfg_attribute_blocks(&source)
            .into_iter()
            .filter(|(_, _, guard)| guard.contains(ATTRIBUTION_FEATURE))
        {
            observed.push((
                relative.clone(),
                guard,
                cfg_owned_anchor(&source, end).expect("feature cfg owns exact code"),
            ));
        }
    }
    observed.sort();
    let mut expected = expected
        .into_iter()
        .map(|(file, guard, anchor)| (file.to_owned(), compact(guard), compact(anchor)))
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(
        observed, expected,
        "the feature cfg inventory is exact and reverse-closed"
    );

    let substitution = include_str!("../../../types/substitute/mod.rs");
    let apply = include_str!("../../../types/substitute/apply.rs");
    let types_owned = read_repo_source("src/types/substitute/wu0g.rs");
    let sidecar = include_str!("wu0g_interface_fill_attribution.rs");
    for (name, source) in [
        ("apply arms", apply),
        ("types-owned WU0G", types_owned.as_str()),
        ("checker sidecar", sidecar),
    ] {
        assert!(
            cfg_attribute_blocks(source)
                .iter()
                .all(|(_, _, guard)| !guard.contains(ATTRIBUTION_FEATURE))
                && attribute_blocks(source, b"cfg_attr")
                    .iter()
                    .all(|(_, _, guard)| !guard.contains(ATTRIBUTION_FEATURE))
                && cfg_macro_invocations(source)
                    .iter()
                    .all(|guard| !guard.contains(ATTRIBUTION_FEATURE)),
            "{name} is wholly owned by its module/macro guard and has no nested feature cfg"
        );
    }
    assert!(!substitution.contains("fn apply_inner"));
    assert!(!apply.contains("apply_attributed_nested_substitution"));
    assert_eq!(substitution.matches("wu0g_record!(visit_").count(), 2);
    assert_eq!(
        substitution.matches("wu0g_record!(cycle_reentry").count(),
        1
    );
    assert_eq!(apply.matches("wu0g_record!(object_copy").count(), 1);
    assert_eq!(apply.matches("wu0g_record!(interner_attempt").count(), 16);
    assert_eq!(apply.matches("wu0g_record!(mapped_nested").count(), 1);
    assert_eq!(
        apply.matches("interner.store().len()").count(),
        0,
        "before/after Store length reads must remain wholly feature-owned"
    );

    let fallback_guard_end = attribute_blocks(substitution, b"cfg")
        .into_iter()
        .find(|(_, _, guard)| compact(guard) == compact(HOT_FALLBACK_GUARD))
        .map(|(_, end, _)| end)
        .expect("one default/control fallback macro guard");
    let fallback = source_item_body(
        &substitution[fallback_guard_end..],
        "macro_rules! wu0g_record",
    );
    for counter_arm in ["visit_enter", "visit_exit", "cycle_reentry", "object_copy"] {
        assert_eq!(
            compact(macro_arm_body(fallback, counter_arm)),
            "{}",
            "fallback counter arm {counter_arm} is structurally empty"
        );
    }
    assert_eq!(
        compact(macro_arm_body(fallback, "interner_attempt")),
        "{{$expression}}",
        "default/control interner attempts are exactly the original expression"
    );
    let mapped_fallback = compact(macro_arm_body(fallback, "mapped_nested"));
    assert_eq!(
        mapped_fallback, "{substitute_with_outcome(interner,ty,&member_map)}",
        "the complete default/control mapped arm is the normalized original expression"
    );
    for forbidden in [
        "wu0g_application_substitute_with_outcome",
        "SubstitutionAccumulator",
        "checked_add",
        "record_",
        "capture_",
    ] {
        assert!(
            !fallback.contains(forbidden),
            "fallback macro performs {forbidden}"
        );
    }

    let ignored_text = format!(
        "// #[cfg(feature = \"{ATTRIBUTION_FEATURE}\")]\nconst TEXT: &str = \"cfg!(feature = '{ATTRIBUTION_FEATURE}')\";\n"
    );
    assert!(cfg_attribute_blocks(&ignored_text).is_empty());
    assert!(cfg_macro_invocations(&ignored_text).is_empty());
    let rogue_cfg_attr = format!(
        "#[cfg_attr(feature = \"{ATTRIBUTION_FEATURE}\", allow(dead_code))]\nfn rogue() {{}}"
    );
    assert!(attribute_blocks(&rogue_cfg_attr, b"cfg_attr")
        .iter()
        .any(|(_, _, guard)| guard.contains(ATTRIBUTION_FEATURE)));
    let rogue_cfg_macro = format!("let _ = cfg!(feature = \"{ATTRIBUTION_FEATURE}\");");
    assert!(cfg_macro_invocations(&rogue_cfg_macro)
        .iter()
        .any(|guard| guard.contains(ATTRIBUTION_FEATURE)));
    let adversarial = format!(
        r###"
#![cfg(feature = "{ATTRIBUTION_FEATURE}")]
# [cfg(feature = "{ATTRIBUTION_FEATURE}")]
# ! [cfg(feature = "{ATTRIBUTION_FEATURE}")]
const CHARACTER: char = ']';
const RAW: &str = r##"#[cfg(feature = \"{ATTRIBUTION_FEATURE}\")] cfg!(feature = \"{ATTRIBUTION_FEATURE}\")"##;
/* outer /* #[cfg(feature = "{ATTRIBUTION_FEATURE}")] */ comment */
#[cfg(all(test, feature = "{ATTRIBUTION_FEATURE}"))]
fn actual_decoy() {{ panic!("prefix decoy"); }}
fn actual() {{ let braces = ('{{', '}}'); }}
"###
    );
    assert_eq!(cfg_attribute_blocks(&adversarial).len(), 4);
    assert_eq!(cfg_macro_invocations(&adversarial).len(), 0);
    assert!(source_item_body(&adversarial, "fn actual").contains("let braces"));
    assert!(
        std::panic::catch_unwind(|| { source_item_body("fn wanted_decoy() {}", "fn wanted") })
            .is_err()
    );
    let spaced_cfg_attr = format!(
        "# [cfg_attr(feature = \"{ATTRIBUTION_FEATURE}\", allow(dead_code))]\nfn rogue() {{}}"
    );
    assert!(attribute_blocks(&spaced_cfg_attr, b"cfg_attr")
        .iter()
        .any(|(_, _, guard)| guard.contains(ATTRIBUTION_FEATURE)));
}

#[test]
fn collector_installation_and_canonical_projection_have_the_required_layering() {
    let sidecar = include_str!("wu0g_interface_fill_attribution.rs");
    let wu0b = include_str!("../wu0b_library.rs");
    let schedule = source_item_body(
        wu0b,
        "fn run_wu0e_fill_schedule_with_interface_observer_for_test(",
    );
    let exact_operations = [
        "fill_type_group_parameter_metadata_range",
        "before_initial_interface_scc",
        "construct_pending_interface_sccs_range",
        "after_initial_interface_scc",
        "fill_conditional_aliases_range",
        "fill_mapped_aliases_range",
        "fill_object_aliases_range",
        "fill_remaining_aliases_range",
        "prepare_project_attached_namespace_values",
        "prepare_project_standalone_namespace_values",
        "publish_class_surfaces",
        "finalize_standalone_namespace_values",
        "precompute_standalone_namespace_value_aliases",
        "before_pending_interface_scc",
        "fill_pending_interfaces_range",
        "after_pending_interface_scc",
        "publish_type_groups",
        "validate_published_class_surfaces",
    ];
    assert_eq!(
        top_level_receiver_calls(schedule, &["pass", "observer"]),
        exact_operations,
        "the parsed top-level WU0E operation sequence is exact"
    );
    let expected_executable_calls = exact_operations
        .iter()
        .map(|operation| {
            let receiver = if operation.starts_with("before_") || operation.starts_with("after_") {
                "observer"
            } else {
                "pass"
            };
            (1_u32, format!("{receiver}.{operation}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        executable_calls(schedule),
        expected_executable_calls,
        "every executable call at every nesting depth is allowlisted"
    );
    assert!(
        executable_assignment_targets(schedule).is_empty(),
        "the wrapper contains no hidden field/local mutation"
    );
    for forbidden in [
        "prefill",
        "clear_eager_application_cache",
        ".clear()",
        "std::mem::take",
        "std::mem::replace",
        ".drain(",
        "retain(|_, _| false)",
    ] {
        assert!(
            !schedule.contains(forbidden),
            "WU0E schedule performs {forbidden}"
        );
    }
    for forbidden in [
        "prefill_object_aliases",
        "prefill_classes",
        "fill_explicit_interface_prerequisites_for_wu0g",
        "clear_wu0g_prefill_application_state",
    ] {
        assert!(
            !sidecar.contains(forbidden),
            "WU0G must observe the pinned phase order, not add {forbidden}"
        );
    }

    for required in [
        "canonical_interface_prefix_semantic_v1",
        "canonical_type_store_bytes_for_test",
        "canonical_library_record_bytes_for_test",
        "reserved_universe_group_states",
        "template_fill",
        "construction_state",
        "type_parameter_defaults",
        "conflict_state",
        "pending_effects",
        "pending_obligations",
        "effect_stack",
    ] {
        assert!(
            sidecar.contains(required),
            "canonical prefix projection misses {required}"
        );
    }
    assert!(!sidecar.contains("semantic_sha256 = self.report.frozen_state_sha256"));
    let wrapper = source_window(
        wu0b,
        "fn canonical_type_store_bytes_for_test(",
        "fn canonical_library_record_bytes_for_test(",
    );
    assert!(wrapper.contains("CanonicalBytes::domain"));
    assert!(wrapper.contains("encode_store_row"));
    assert!(wrapper.contains("for raw in 0..store.len()"));

    let runner = source_window(
        sidecar,
        "fn measure_dom_rung_for_test(",
        "fn canonical_interface_prefix_bytes_from_raw_for_test(",
    );
    for required in [
        "build_dom_heritage_prefix_ladder_for_test",
        "run_wu0e_fill_schedule_with_interface_observer_for_test",
        "components[..canonical.selected_component_count]",
        "full_universe_source_names",
        "selected_component.identity",
        "selected_component.group_ids",
        "plan_runtime_mismatch",
        "record_remaining_selected_before_no_progress",
    ] {
        assert!(runner.contains(required), "DOM runner misses {required}");
    }
    assert!(!runner.contains("take(canonical.selected_component_count)"));
    for forbidden in [
        "fill_object_aliases_range",
        "fill_remaining_aliases_range",
        "publish_class_surfaces",
        "clear_eager_application_cache",
        "prefill_object_aliases",
        "prefill_classes",
    ] {
        assert!(
            !runner.contains(forbidden),
            "the runner must reuse the real observed schedule, not synthesize {forbidden}"
        );
    }

    let probe = source_item_body(sidecar, "fn run_pinned_dom_ladder_probe_for_test(");
    for required in [
        "run_wu0e_hardened_child_for_test",
        "per_rung_deadline",
        "memory_max_bytes",
        "InterfaceFillAttributionMode::Baseline",
        "InterfaceFillAttributionMode::CandidateB",
    ] {
        assert!(
            probe.contains(required),
            "pinned coordinator misses {required}"
        );
    }
    assert!(!probe.contains("measure_dom_rung_for_test("));

    let planner = source_window(
        sidecar,
        "fn build_heritage_plan_data(",
        "fn component_identity(",
    );
    for declaration in [
        "TypeDecl::Alias",
        "TypeDecl::Class",
        "TypeDecl::Resolved",
        "TypeDecl::Unavailable",
    ] {
        assert!(
            planner.contains(declaration),
            "external classification omits {declaration}"
        );
    }
    for disposition in ["CompleteRequired", "OpaqueOrderingOnly"] {
        assert!(
            planner.contains(disposition),
            "heritage edge loses {disposition}"
        );
    }
    assert!(planner.contains("Poisoned"));
    assert!(!planner.contains("TypeDecl::Resolved { .. } | TypeDecl::Unavailable { .. } => None"));

    let snapshots = observe_wu0e_phase_snapshots_for_test(&external_topology_profile())
        .expect("phase snapshots from the real observed WU0E schedule");
    assert_eq!(snapshots.phase_names, exact_operations);
    for suspended in [
        "fill_conditional_aliases_range",
        "fill_mapped_aliases_range",
        "fill_object_aliases_range",
        "fill_remaining_aliases_range",
        "prepare_project_attached_namespace_values",
        "prepare_project_standalone_namespace_values",
        "publish_class_surfaces",
        "finalize_standalone_namespace_values",
        "precompute_standalone_namespace_value_aliases",
    ] {
        let before = snapshots.before(suspended).expect("phase entry snapshot");
        let after = snapshots.after(suspended).expect("phase exit snapshot");
        assert_eq!(before.attribution_counters, after.attribution_counters);
        assert_eq!(before.attribution_digest, after.attribution_digest);
        assert_eq!(before.eager_cache_identity, after.eager_cache_identity);
        assert_eq!(
            before.eager_cache_canonical_bytes,
            after.eager_cache_canonical_bytes
        );
        assert!(!before.collector_active && !after.collector_active);
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

    let apply = include_str!("../../../types/substitute/apply.rs");
    let object_copy = source_window(
        apply,
        "pub(super) fn apply_object",
        "self.in_progress.insert",
    );
    assert!(object_copy.contains("wu0g_record!(object_copy"));
    assert!(!object_copy.contains(".sum()"));
    assert!(!object_copy.contains("call_signatures.len() + construct_signatures.len()"));
    let types_owned = read_repo_source("src/types/substitute/wu0g.rs");
    let checked_copy = source_item_body(&types_owned, "fn checked_object_copy_cardinality(");
    assert!(checked_copy.contains("try_fold"));
    assert!(checked_copy.contains("checked_add"));
    let overflow_input = RawCopyAccountingInput {
        property_name_byte_lengths: vec![usize::MAX, 1],
        call_signature_count: usize::MAX,
        construct_signature_count: 1,
    };
    let overflow = measure_copy_accounting_for_test(&overflow_input);
    assert!(overflow.saturated);
    assert!(overflow.validate_exact().is_err());
}

#[test]
fn cycle_tainted_repeated_key_counts_candidate_b_hits_and_exact_avoided_visits() {
    let baseline = measured(&CYCLE_TAINTED, InterfaceFillAttributionMode::Baseline);
    assert_cardinality(&baseline.attribution, 2, 1, 2);
    assert_eq!(baseline.attribution.clean_cache_hits, 0);
    assert_eq!(baseline.attribution.executed_substitutions, 2);
    assert_eq!(baseline.attribution.cycle_clean_outcomes, 0);
    assert_eq!(baseline.attribution.cycle_tainted_outcomes, 2);
    assert_eq!(baseline.attribution.cycle_reentries, 2);
    assert_eq!(baseline.attribution.candidate_b_hits, 0);

    let candidate = measured(&CYCLE_TAINTED, InterfaceFillAttributionMode::CandidateB);
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
    assert_eq!(
        InterfaceFillCounterFamily::ALL,
        [
            InterfaceFillCounterFamily::Application,
            InterfaceFillCounterFamily::Execution,
            InterfaceFillCounterFamily::Visit,
            InterfaceFillCounterFamily::Copy,
            InterfaceFillCounterFamily::Interner,
            InterfaceFillCounterFamily::Component,
        ]
    );
    assert!(!InterfaceFillCounterFamily::ALL.is_empty());
    for family in InterfaceFillCounterFamily::ALL {
        let mut report = InterfaceFillAttributionReport::maximal_consistent_for_test();
        report.force_counter_family_overflow_for_test(family);
        assert!(report.saturated, "counter family {family:?}");
        assert!(
            report.validate_exact().is_err(),
            "counter family {family:?}"
        );
    }
    let limits = InterfaceFillAttributionLimits {
        max_application_keys: 3,
        max_arguments_per_key: 2,
        max_application_arguments: 4,
        max_application_key_bytes: 64,
        max_component_checkpoints: 2,
        max_checkpoint_bytes: 2_048,
    };
    assert_eq!(
        InterfaceFillBudgetKind::ALL,
        [
            InterfaceFillBudgetKind::Keys,
            InterfaceFillBudgetKind::ArgumentsPerKey,
            InterfaceFillBudgetKind::Arguments,
            InterfaceFillBudgetKind::KeyBytes,
            InterfaceFillBudgetKind::ComponentCheckpoints,
            InterfaceFillBudgetKind::CheckpointBytes,
        ]
    );
    assert!(!InterfaceFillBudgetKind::ALL.is_empty());
    for budget in InterfaceFillBudgetKind::ALL {
        let limit = match budget {
            InterfaceFillBudgetKind::Keys => limits.max_application_keys,
            InterfaceFillBudgetKind::ArgumentsPerKey => limits.max_arguments_per_key,
            InterfaceFillBudgetKind::Arguments => limits.max_application_arguments,
            InterfaceFillBudgetKind::KeyBytes => limits.max_application_key_bytes,
            InterfaceFillBudgetKind::ComponentCheckpoints => limits.max_component_checkpoints,
            InterfaceFillBudgetKind::CheckpointBytes => limits.max_checkpoint_bytes,
        };
        let at_limit = measure_application_budget_for_test(&RawBudgetWitnessRequest {
            limits,
            budget,
            requested: limit,
        })
        .expect("real collector at-limit witness");
        let over = limit.checked_add(1).expect("tiny witness limit + 1");
        let limit_plus_one = measure_application_budget_for_test(&RawBudgetWitnessRequest {
            limits,
            budget,
            requested: over,
        })
        .expect("real collector limit+1 witness");
        assert_eq!(at_limit.observed, limit);
        assert_eq!(
            limit_plus_one.observed, over,
            "the witness injects raw cap+1 work instead of toggling saturation"
        );
        assert_eq!(at_limit.retained, limit, "retained at cap {budget:?}");
        assert_eq!(
            limit_plus_one.retained, limit,
            "retained after cap {budget:?}"
        );
        assert!(!at_limit.attribution.saturated, "at-limit {budget:?}");
        assert_eq!(at_limit.attribution.validate_exact(), Ok(()));
        assert!(limit_plus_one.attribution.saturated, "limit+1 {budget:?}");
        assert!(limit_plus_one.attribution.validate_exact().is_err());
    }
    let sidecar = include_str!("wu0g_interface_fill_attribution.rs");
    assert!(!sidecar.contains("exhausted_budget_for_test"));
    let boundary_helper = source_window(
        sidecar,
        "fn measure_application_budget_for_test(",
        "fn measure_copy_accounting_for_test(",
    );
    assert!(boundary_helper.contains("InterfaceFillAttributionCollector::new_with_limits"));
    assert!(boundary_helper.contains("record_request"));
    assert!(boundary_helper.contains("component_boundary"));
}

#[test]
fn cooperative_stop_is_identity_based_and_returns_exact_partial_construction_state() {
    let partial = measure_interface_fill_cooperative_partial_for_test(
        &CYCLE_TAINTED,
        InterfaceFillAttributionMode::Baseline,
    )
    .expect("cooperative identity-bound partial");
    assert!(partial.cooperatively_stopped);
    assert!(!partial.attribution.is_complete);
    assert!(!partial.completion.is_complete());
    assert!(!partial.component_checkpoints.is_empty());
    assert_eq!(
        partial
            .component_checkpoints
            .last()
            .expect("completed boundary")
            .cumulative,
        partial.attribution.scalar_snapshot_for_test()
    );
    assert!(
        partial
            .completion
            .completed_components
            .iter()
            .all(|completed| completed.all_groups_frozen()
                && completed.interface_template_fill_done())
    );
    assert!(!partial.completion.remaining_selected_components.is_empty());
    assert!(partial
        .completion
        .remaining_selected_components
        .iter()
        .all(|remaining| matches!(
            remaining.state,
            InterfaceFillRemainingState::Pending
                | InterfaceFillRemainingState::Deferred
                | InterfaceFillRemainingState::NoProgress
        )));
    assert_eq!(partial.attribution.in_flight_applications, 0);
    assert_eq!(partial.attribution.in_flight_components, 0);
    assert_eq!(partial.completion.validate_exact(), Ok(()));
    assert_report_arithmetic(&partial.attribution);

    let sidecar = include_str!("wu0g_interface_fill_attribution.rs");
    assert!(!sidecar.contains("stop_after_components"));
    assert!(sidecar.contains("record_remaining_selected_before_no_progress"));
}

#[test]
fn report_validation_derives_cross_counter_arithmetic_and_rejects_impossible_avoidance() {
    let baseline = measured(REPEATED_KEY, InterfaceFillAttributionMode::Baseline);
    let mut mutations = Vec::new();
    let mut avoided_without_hit = baseline.attribution.clone();
    avoided_without_hit.candidate_b_avoided_visits = 1;
    mutations.push(("avoided visits without a hit", avoided_without_hit));
    let mut application_mismatch = baseline.attribution.clone();
    application_mismatch.application_requests += 1;
    mutations.push(("application arithmetic", application_mismatch));
    let mut interner_mismatch = baseline.attribution.clone();
    interner_mismatch.substitution_interner_attempts += 1;
    mutations.push(("interner arithmetic", interner_mismatch));
    let real_candidate = measured(&CYCLE_TAINTED, InterfaceFillAttributionMode::CandidateB);
    assert!(real_candidate.attribution.candidate_b_hits > 0);
    let mut inflated_avoidance = real_candidate.attribution.clone();
    inflated_avoidance.candidate_b_avoided_visits = inflated_avoidance
        .candidate_b_avoided_visits
        .checked_add(1)
        .expect("real avoided visits + 1");
    assert_ne!(inflated_avoidance, real_candidate.attribution);
    assert!(
        inflated_avoidance.validate_exact().is_err(),
        "per-key retained visit counts reject an avoided-visits-only inflation"
    );
    for (label, invalid) in mutations {
        assert_ne!(invalid, baseline.attribution, "mutation changes raw input");
        assert!(invalid.validate_exact().is_err(), "reject {label}");
    }
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

    let base = first[0].clone();
    let mut manifest_mutations = Vec::new();
    let mut forged_identity = base.clone();
    forged_identity.identity = "f".repeat(64);
    manifest_mutations.push(("identity", forged_identity));
    let mut kind = base.clone();
    kind.kind = ManifestClosureKind::Full;
    manifest_mutations.push(("kind", kind));
    let mut root = base.clone();
    root.root_name = "lib.not-es5.d.ts".to_owned();
    manifest_mutations.push(("root", root));
    let mut closed = base.clone();
    closed.is_dependency_closed = false;
    manifest_mutations.push(("dependency-closed flag", closed));
    let mut truncation = base.clone();
    truncation.uses_source_truncation = true;
    manifest_mutations.push(("truncation flag", truncation));
    for (label, mutation) in manifest_mutations {
        assert_ne!(mutation, base, "manifest {label} mutation is non-noop");
        assert!(
            measure_manifest_closure_for_test(&mutation, InterfaceFillAttributionMode::Baseline)
                .is_err(),
            "reject manifest {label}"
        );
    }
    let mut missing_dependency = first[1].clone();
    missing_dependency
        .source_names
        .retain(|name| name != "lib.es5.d.ts");
    assert!(measure_manifest_closure_for_test(
        &missing_dependency,
        InterfaceFillAttributionMode::Baseline
    )
    .is_err());
    let mut reordered = first[1].clone();
    reordered.source_names.swap(0, 1);
    assert!(
        measure_manifest_closure_for_test(&reordered, InterfaceFillAttributionMode::Baseline)
            .is_err()
    );
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
    let expected_external_inventory = full_universe_external_terminal_inventory_for_test(&profile)
        .expect("exhaustive full-universe external construction inventory");
    let external_by_identity = expected_external_inventory
        .iter()
        .map(|terminal| (terminal.identity.clone(), terminal))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(ladder[0].full_universe_source_names.len(), 82);
    assert!(is_lower_hex_identity(universe_identity));
    assert!(!component_order.is_empty());
    assert!(!expected_external_inventory.is_empty());
    assert_eq!(
        external_by_identity.len(),
        expected_external_inventory.len()
    );
    assert!(expected_external_inventory
        .iter()
        .all(|terminal| is_lower_hex_identity(&terminal.identity)));
    assert_eq!(
        expected_external_inventory
            .iter()
            .filter(|terminal| terminal.is_topology_terminal)
            .count(),
        full_universe_external_topology_terminal_count_for_test(&profile)
            .expect("actual external topology terminal count"),
        "every actual topology terminal is classified exactly once"
    );
    for terminal in &expected_external_inventory {
        match terminal.kind {
            InterfaceFillExternalTerminalKind::AliasTransparent
            | InterfaceFillExternalTerminalKind::ResolvedAbsorbed
            | InterfaceFillExternalTerminalKind::UnavailableAbsorbed => {
                assert!(!terminal.is_topology_terminal)
            }
            InterfaceFillExternalTerminalKind::OutOfRange => {
                assert!(terminal.is_topology_terminal);
                assert_eq!(
                    terminal.construction_state,
                    InterfaceFillExternalConstructionState::OutOfRange
                );
            }
            InterfaceFillExternalTerminalKind::ClassTerminal
            | InterfaceFillExternalTerminalKind::InterfaceComponent => {}
        }
        assert_ne!(
            terminal.construction_state,
            InterfaceFillExternalConstructionState::Building,
            "in-flight construction is not terminal evidence"
        );
        if terminal.description_calls_state_frozen() {
            assert!(matches!(
                terminal.construction_state,
                InterfaceFillExternalConstructionState::FrozenInstalled
                    | InterfaceFillExternalConstructionState::FrozenUnavailable
            ));
            assert!(terminal.measured_frozen_state_sha256.is_some());
        }
    }
    assert_eq!(
        ladder[0].external_terminal_inventory, expected_external_inventory,
        "the SCC plan carries the exhaustive measured external inventory"
    );

    let all_ids = component_order
        .iter()
        .map(|component| component.identity.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(all_ids.len(), component_order.len());
    let mut seen = BTreeSet::new();
    for component in component_order {
        assert!(is_lower_hex_identity(&component.identity));
        assert!(!component.group_ids.is_empty());
        assert_eq!(
            component
                .group_ids
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            component.group_ids.len(),
            "component identity is bound to exact unique group membership"
        );
        for dependency in &component.heritage_dependencies {
            match dependency {
                InterfaceHeritageDependency::InterfaceComponent {
                    identity,
                    disposition,
                } => {
                    assert!(matches!(
                        disposition,
                        InterfaceHeritageEdgeDisposition::CompleteRequired
                            | InterfaceHeritageEdgeDisposition::OpaqueOrderingOnly
                    ));
                    assert!(
                        seen.contains(identity),
                        "every known interface dependency precedes the dependent SCC"
                    );
                }
                InterfaceHeritageDependency::ExternalTerminal {
                    identity,
                    disposition,
                } => {
                    assert!(is_lower_hex_identity(identity));
                    assert!(!all_ids.contains(identity));
                    let terminal = external_by_identity
                        .get(identity)
                        .expect("external heritage terminal belongs to the closed inventory");
                    assert!(matches!(
                        terminal.kind,
                        InterfaceFillExternalTerminalKind::ClassTerminal
                            | InterfaceFillExternalTerminalKind::InterfaceComponent
                    ));
                    if *disposition == InterfaceHeritageEdgeDisposition::CompleteRequired {
                        assert!(matches!(
                            terminal.construction_state,
                            InterfaceFillExternalConstructionState::FrozenInstalled
                                | InterfaceFillExternalConstructionState::FrozenUnavailable
                        ));
                    }
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
            rung.external_terminal_inventory,
            expected_external_inventory
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
fn spec_owned_82_source_fixture_drives_the_real_five_rung_runner() {
    let profile = synthetic_82_source_profile();
    assert_eq!(profile.sources.len(), 82);
    assert_eq!(
        profile
            .sources
            .iter()
            .map(|source| source.file_ordinal.index())
            .collect::<Vec<_>>(),
        (0..82).collect::<Vec<_>>()
    );
    assert_eq!(
        profile
            .sources
            .iter()
            .map(|source| source.name.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        82,
        "all synthetic source names are unique"
    );
    let ladder = build_dom_heritage_prefix_ladder_for_test(&profile)
        .expect("the real planner accepts the spec-owned profile");
    assert_eq!(ladder.len(), 5);
    assert_eq!(
        ladder
            .iter()
            .map(|rung| rung.target_basis_points)
            .collect::<Vec<_>>(),
        [1_250, 2_500, 5_000, 7_500, 10_000]
    );
    assert_eq!(
        ladder
            .iter()
            .map(|rung| rung.selected_member_weight)
            .collect::<Vec<_>>(),
        [13, 25, 50, 75, 100]
    );
    let components = &ladder[0].dependency_first_components;
    assert_eq!(components.len(), 5);
    assert_eq!(
        components
            .iter()
            .map(|component| component.interface_member_weight)
            .collect::<Vec<_>>(),
        [13, 12, 25, 25, 25]
    );
    assert_eq!(
        components
            .iter()
            .flat_map(|component| component.group_names.iter().map(String::as_str))
            .collect::<Vec<_>>(),
        ["WU0GOne", "WU0GTwo", "WU0GThree", "WU0GFour", "WU0GFive"]
    );
    assert!(components.iter().all(|component| {
        !component.group_ids.is_empty()
            && component.group_ids.len() == component.group_names.len()
            && component
                .group_ids
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                == component.group_ids.len()
    }));

    for rung in &ladder {
        let baseline =
            measure_dom_rung_for_test(&profile, rung, InterfaceFillAttributionMode::Baseline)
                .expect("baseline spec rung completes");
        let candidate =
            measure_dom_rung_for_test(&profile, rung, InterfaceFillAttributionMode::CandidateB)
                .expect("Candidate-B spec rung completes");
        for measured in [&baseline, &candidate] {
            let expected_names = profile
                .sources
                .iter()
                .map(|source| source.name.clone())
                .collect::<Vec<_>>();
            assert_eq!(measured.parsed_source_names, expected_names);
            assert_eq!(measured.bound_source_names, expected_names);
            assert_eq!(measured.reserved_source_names, expected_names);
            assert_eq!(measured.rung_identity, rung.identity);
            assert_eq!(measured.universe_identity, rung.universe_identity);
            assert_eq!(
                measured.measurement.raw_completion.selected_components,
                components[..rung.selected_component_count]
                    .iter()
                    .map(|component| RawSelectedComponentDirective {
                        identity: component.identity.clone(),
                        group_ids: component.group_ids.clone(),
                    })
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                validate_raw_completion_inventory_for_test(&measured.measurement.raw_completion),
                Ok(())
            );
            assert_eq!(
                measured
                    .measurement
                    .raw_completion
                    .completed_components
                    .iter()
                    .map(|component| RawSelectedComponentDirective {
                        identity: component.identity.clone(),
                        group_ids: component.group_ids.clone(),
                    })
                    .collect::<Vec<_>>(),
                measured.measurement.raw_completion.selected_components
            );
            assert!(measured
                .measurement
                .raw_completion
                .remaining_selected_components
                .is_empty());
            assert!(measured
                .measurement
                .raw_completion
                .completed_components
                .iter()
                .all(|component| component
                    .group_states
                    .iter()
                    .all(|state| *state == RawCompletionGroupState::Frozen)
                    && component.template_fill_done.iter().all(|done| *done)));
            assert_eq!(
                sha256(&measured.measurement.semantic_canonical_bytes),
                measured.measurement.semantic_sha256,
                "the spec independently hashes implementation-exposed canonical bytes"
            );
        }
        assert_eq!(
            baseline.measurement.semantic_canonical_bytes,
            candidate.measurement.semantic_canonical_bytes
        );
        validate_measured_ladder_row_from_raw_for_test(rung, &baseline, &candidate)
            .expect("private opaque evidence is constructed only from raw measured reports");
    }

    let sidecar = include_str!("wu0g_interface_fill_attribution.rs");
    for forbidden in [
        "build_sealed_tiny_dom_ladder_fixture_for_test",
        "expected_report",
        "sealed_synthetic_selection_domain",
    ] {
        assert!(
            !sidecar.contains(forbidden),
            "sidecar must not self-oracle via {forbidden}"
        );
    }
}

#[test]
fn runner_rejects_every_mutated_framed_rung_input() {
    let profile = external_topology_profile();
    let ladder =
        build_dom_heritage_prefix_ladder_for_test(&profile).expect("canonical rung inputs");
    let base = ladder[2].clone();
    assert!(base.full_universe_source_names.len() >= 2);
    assert!(base.dependency_first_components.len() >= 2);
    assert!(!base.external_terminal_inventory.is_empty());
    let mut mutations = Vec::new();
    let mut target = base.clone();
    target.target_basis_points += 1;
    mutations.push(("target", target));
    let mut selected_count = base.clone();
    selected_count.selected_component_count += 1;
    mutations.push(("selected component count", selected_count));
    let mut selected_weight = base.clone();
    selected_weight.selected_member_weight += 1;
    mutations.push(("selected member weight", selected_weight));
    let mut total_weight = base.clone();
    total_weight.total_member_weight += 1;
    mutations.push(("total member weight", total_weight));
    let mut truncation = base.clone();
    truncation.uses_file_or_line_truncation = true;
    mutations.push(("truncation bool", truncation));
    let mut universe = base.clone();
    universe.universe_identity = "f".repeat(64);
    mutations.push(("universe", universe));
    let mut source_order = base.clone();
    source_order.full_universe_source_names.swap(0, 1);
    mutations.push(("source order", source_order));
    let mut weights = base.clone();
    weights.dependency_first_components[0].interface_member_weight += 1;
    mutations.push(("component weights", weights));
    let mut component_order = base.clone();
    component_order.dependency_first_components.swap(0, 1);
    mutations.push(("component order", component_order));
    let mut component_identity = base.clone();
    component_identity.dependency_first_components[0].identity = "d".repeat(64);
    mutations.push(("component identity", component_identity));
    let mut name_membership = base.clone();
    name_membership.dependency_first_components[0].group_names[0].push_str("Wrong");
    mutations.push(("component name membership", name_membership));
    let mut membership = base.clone();
    let repeated_group = membership.dependency_first_components[0].group_ids[0];
    membership.dependency_first_components[0]
        .group_ids
        .push(repeated_group);
    mutations.push(("component membership", membership));
    let mut dependency = base.clone();
    let disposition = dependency
        .dependency_first_components
        .iter_mut()
        .flat_map(|component| component.heritage_dependencies.iter_mut())
        .map(|dependency| match dependency {
            InterfaceHeritageDependency::InterfaceComponent { disposition, .. }
            | InterfaceHeritageDependency::ExternalTerminal { disposition, .. } => disposition,
        })
        .next()
        .expect("fixture has a heritage dependency");
    *disposition = match *disposition {
        InterfaceHeritageEdgeDisposition::CompleteRequired => {
            InterfaceHeritageEdgeDisposition::OpaqueOrderingOnly
        }
        InterfaceHeritageEdgeDisposition::OpaqueOrderingOnly => {
            InterfaceHeritageEdgeDisposition::CompleteRequired
        }
    };
    mutations.push(("dependency disposition", dependency));
    let mut dependency_identity = base.clone();
    let identity = dependency_identity
        .dependency_first_components
        .iter_mut()
        .flat_map(|component| component.heritage_dependencies.iter_mut())
        .map(|dependency| match dependency {
            InterfaceHeritageDependency::InterfaceComponent { identity, .. }
            | InterfaceHeritageDependency::ExternalTerminal { identity, .. } => identity,
        })
        .next()
        .expect("fixture has dependency identity");
    *identity = "c".repeat(64);
    mutations.push(("dependency identity", dependency_identity));
    let mut external = base.clone();
    external.external_terminal_inventory.remove(0);
    mutations.push(("external inventory", external));
    let mut identity = base.clone();
    identity.identity = "e".repeat(64);
    mutations.push(("rung identity", identity));
    for (label, mutation) in mutations {
        assert_ne!(mutation, base, "{label} mutation is non-noop");
        assert!(
            measure_dom_rung_for_test(&profile, &mutation, InterfaceFillAttributionMode::Baseline)
                .is_err(),
            "runner rejects mutated {label}"
        );
    }
}

#[test]
fn selected_prefix_preconstruction_gate_stalls_before_later_independent_scc() {
    let blocked_terminal =
        raw_identity("wu0g-terminal-v1", b"blocked-terminal".to_vec()).claimed_sha256;
    let first_identity =
        raw_identity("wu0g-component-v1", b"selected-first".to_vec()).claimed_sha256;
    let later_identity =
        raw_identity("wu0g-component-v1", b"selected-later-independent".to_vec()).claimed_sha256;
    let plan = RawSelectedComponentPlan {
        selected: vec![
            RawPlannedComponent {
                identity: first_identity.clone(),
                group_ids: vec![10],
                dependencies: vec![RawPlanDependency {
                    identity: blocked_terminal.clone(),
                    disposition: InterfaceHeritageEdgeDisposition::CompleteRequired,
                }],
            },
            RawPlannedComponent {
                identity: later_identity.clone(),
                group_ids: vec![11],
                dependencies: Vec::new(),
            },
        ],
        external_states: vec![(blocked_terminal, RawPlanTerminalState::Pending)],
    };
    let partial = run_selected_component_plan_for_test(&plan)
        .expect("the exact low-level selected component plan is well formed");
    assert_eq!(partial.outcome, InterfaceFillPlanOutcome::NoProgress);
    assert!(partial.completed_components.is_empty());
    assert_eq!(
        partial
            .remaining_components
            .iter()
            .map(|component| component.identity.as_str())
            .collect::<Vec<_>>(),
        [first_identity.as_str(), later_identity.as_str()]
    );
    assert_eq!(
        partial.remaining_components[0].group_states,
        [RawCompletionGroupState::Deferred]
    );
    assert_eq!(
        partial.remaining_components[1].group_states,
        [RawCompletionGroupState::Pending],
        "the later independently-ready SCC is untouched"
    );
    assert_eq!(partial.mutated_group_ids, Vec::<u32>::new());

    let scheduler = source_item_body(
        include_str!("mod.rs"),
        "fn construct_pending_interface_sccs(",
    );
    let ready = code_offsets(scheduler, "interface_component_is_ready")[0];
    let pre = code_offsets(scheduler, "wu0g_admit_component_before_construction")[0];
    let construct = code_offsets(scheduler, "construct_interface_component")[0];
    let post = code_offsets(scheduler, "wu0g_record_component_boundary")[0];
    assert!(ready < pre && pre < construct && construct < post);
    let pre_hook = source_item_body(
        include_str!("wu0g_interface_fill_attribution.rs"),
        "fn wu0g_admit_component_before_construction",
    );
    assert!(pre_hook.contains("component.identity"));
    assert!(pre_hook.contains("component.group_ids"));
}

#[test]
fn external_topology_is_exact_and_complete_required_differs_from_opaque_ordering() {
    let profile = external_topology_profile();
    let injection = RawExternalTopologyInjection {
        identity: raw_identity("wu0g-out-of-range-v1", b"independent-injection".to_vec())
            .claimed_sha256,
        out_of_range_group_id: u32::MAX,
    };
    let topology = build_raw_external_topology_with_injection_for_test(&profile, &injection)
        .expect("spec-owned topology fixture is planned by the real binder/checker");
    assert_eq!(
        topology
            .rows
            .iter()
            .map(|row| row.symbol_name.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        topology.rows.len(),
        "topology row names are unique before map construction"
    );
    assert_eq!(
        topology
            .rows
            .iter()
            .map(|row| row.identity.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        topology.rows.len(),
        "topology row identities are unique before map construction"
    );
    let rows = topology
        .rows
        .iter()
        .map(|row| (row.symbol_name.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let expected = [
        (
            "WU0GTransparentAlias",
            InterfaceFillExternalTerminalKind::AliasTransparent,
            false,
        ),
        (
            "WU0GOpaqueAlias",
            InterfaceFillExternalTerminalKind::AliasTransparent,
            false,
        ),
        (
            "WU0GClassTerminal",
            InterfaceFillExternalTerminalKind::ClassTerminal,
            true,
        ),
        (
            "WU0GResolvedAbsorbed",
            InterfaceFillExternalTerminalKind::ResolvedAbsorbed,
            false,
        ),
        (
            "WU0GUnavailableAbsorbed",
            InterfaceFillExternalTerminalKind::UnavailableAbsorbed,
            false,
        ),
        (
            "WU0GRequiredTerminal",
            InterfaceFillExternalTerminalKind::InterfaceComponent,
            true,
        ),
    ];
    for (name, kind, terminal) in expected {
        let row = rows
            .get(name)
            .unwrap_or_else(|| panic!("missing topology row {name}"));
        assert_eq!(row.kind, kind);
        assert_eq!(row.is_topology_terminal, terminal);
        assert!(is_lower_hex_identity(&row.identity));
        assert_ne!(
            row.construction_state,
            InterfaceFillExternalConstructionState::Building
        );
    }
    let out_of_range = topology
        .rows
        .iter()
        .find(|row| row.identity == injection.identity)
        .expect("independently injected out-of-range row");
    assert_eq!(
        out_of_range.kind,
        InterfaceFillExternalTerminalKind::OutOfRange
    );
    assert_eq!(
        out_of_range.construction_state,
        InterfaceFillExternalConstructionState::OutOfRange
    );
    assert_eq!(rows.len(), expected.len() + 1);
    assert_eq!(
        topology.edges.len(),
        3,
        "duplicate raw edges fail before set normalization"
    );
    assert_eq!(
        topology
            .edges
            .iter()
            .map(|edge| {
                (
                    edge.source_symbol_name.as_str(),
                    edge.target_symbol_name.as_str(),
                    edge.disposition,
                )
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            (
                "WU0GCompleteDependent",
                "WU0GRequiredTerminal",
                InterfaceHeritageEdgeDisposition::CompleteRequired,
            ),
            (
                "WU0GOpaqueDependent",
                "WU0GRequiredTerminal",
                InterfaceHeritageEdgeDisposition::OpaqueOrderingOnly,
            ),
            (
                "WU0GClassDependent",
                "WU0GClassTerminal",
                InterfaceHeritageEdgeDisposition::CompleteRequired,
            ),
        ]),
        "the complete external edge set is exact and has no extras"
    );
    let readiness = measure_external_disposition_readiness_for_test(&profile)
        .expect("real readiness evaluation with the terminal held incomplete");
    let state_by_identity = readiness
        .states
        .iter()
        .map(|state| (state.identity.as_str(), state.construction_state))
        .collect::<BTreeMap<_, _>>();
    let derive_blocked = |source: &str| {
        readiness.edges.iter().any(|edge| {
            edge.source_symbol_name == source
                && edge.disposition == InterfaceHeritageEdgeDisposition::CompleteRequired
                && !matches!(
                    state_by_identity.get(edge.target_identity.as_str()),
                    Some(
                        InterfaceFillExternalConstructionState::FrozenInstalled
                            | InterfaceFillExternalConstructionState::FrozenUnavailable
                    )
                )
        })
    };
    assert!(derive_blocked("WU0GCompleteDependent"));
    assert!(!derive_blocked("WU0GOpaqueDependent"));
    assert!(readiness.edges.iter().any(|edge| {
        edge.source_symbol_name == "WU0GOpaqueDependent"
            && edge.disposition == InterfaceHeritageEdgeDisposition::OpaqueOrderingOnly
    }));
}

#[test]
fn completion_validator_rejects_direct_raw_inventory_mutations() {
    let selected = RawSelectedComponentDirective {
        identity: raw_identity("wu0g-component-v1", b"selected-component".to_vec()).claimed_sha256,
        group_ids: vec![7, 8],
    };
    let base = RawCompletionInventory {
        expected_components: vec![selected.clone()],
        selected_components: vec![selected.clone()],
        completed_components: vec![RawCompletionComponent {
            identity: selected.identity.clone(),
            group_ids: selected.group_ids.clone(),
            group_states: vec![RawCompletionGroupState::Frozen; 2],
            template_fill_done: vec![true; 2],
        }],
        remaining_selected_components: Vec::new(),
        expected_external_topology_rows: vec![b"class:complete-required:frozen".to_vec()],
        external_topology_rows: vec![b"class:complete-required:frozen".to_vec()],
    };
    assert_eq!(validate_raw_completion_inventory_for_test(&base), Ok(()));

    let mut mutations = Vec::new();
    let mut missing_expected = base.clone();
    missing_expected.expected_components.clear();
    mutations.push(("missing expected component", missing_expected));
    let mut outside_prefix = base.clone();
    outside_prefix
        .selected_components
        .push(RawSelectedComponentDirective {
            identity: "later-independent".to_owned(),
            group_ids: vec![9],
        });
    mutations.push(("selected outside canonical prefix", outside_prefix));
    let mut wrong_membership = base.clone();
    wrong_membership.completed_components[0].group_ids.reverse();
    mutations.push(("group membership mismatch", wrong_membership));
    let mut building = base.clone();
    building.completed_components[0].group_states[0] = RawCompletionGroupState::Building;
    mutations.push(("building is not complete", building));
    let mut fill_pending = base.clone();
    fill_pending.completed_components[0].template_fill_done[0] = false;
    mutations.push(("template fill not done", fill_pending));
    let mut deferred_selected = base.clone();
    deferred_selected.completed_components.clear();
    deferred_selected
        .remaining_selected_components
        .push(RawCompletionComponent {
            identity: selected.identity.clone(),
            group_ids: selected.group_ids.clone(),
            group_states: vec![RawCompletionGroupState::Deferred; 2],
            template_fill_done: vec![false; 2],
        });
    mutations.push(("selected component deferred", deferred_selected));
    let mut missing_external = base.clone();
    missing_external.external_topology_rows.clear();
    mutations.push(("missing external classification", missing_external));
    for (label, mutation) in mutations {
        assert_ne!(mutation, base, "raw mutation for {label} changes input");
        assert!(
            validate_raw_completion_inventory_for_test(&mutation).is_err(),
            "reject {label}"
        );
    }
}

#[test]
fn every_canonical_prefix_section_changes_independently_hashed_raw_bytes() {
    let profile = synthetic_82_source_profile();
    let ladder =
        build_dom_heritage_prefix_ladder_for_test(&profile).expect("real synthetic ladder");
    let rung = ladder
        .iter()
        .find(|rung| rung.target_basis_points == 7_500)
        .expect("75% rung");
    let measured =
        measure_dom_rung_for_test(&profile, rung, InterfaceFillAttributionMode::Baseline)
            .expect("real 75%-rung measurement");
    let base = measured.measurement.raw_canonical_sections.clone();
    assert_eq!(
        base.dense_type_store
            .iter()
            .map(|row| row.type_id)
            .collect::<Vec<_>>(),
        (0..u32::try_from(base.dense_type_store.len()).expect("store length fits u32"))
            .collect::<Vec<_>>(),
        "the real runner exposes the full dense TypeStore"
    );
    assert!(base
        .reserved_universe_group_states
        .iter()
        .any(|state| !state.selected));
    let base_bytes = canonical_interface_prefix_bytes_from_raw_for_test(&base)
        .expect("canonical raw prefix encoding");
    assert_eq!(
        base_bytes, measured.measurement.semantic_canonical_bytes,
        "the spec-owned raw-section encoder reproduces the real runner bytes exactly"
    );
    let base_digest = sha256(&base_bytes);
    let mut mutations = Vec::new();
    let mut profile = base.clone();
    profile.profile.push(b'!');
    mutations.push(("profile", profile));
    let mut universe = base.clone();
    universe.universe.push(b'!');
    mutations.push(("universe", universe));
    let mut rung = base.clone();
    rung.rung.push(b'!');
    mutations.push(("rung", rung));
    let mut component = base.clone();
    component.component_order_and_membership[0].push(b'!');
    mutations.push(("component order and membership", component));
    let mut external = base.clone();
    external.external_inventory[0].push(b'!');
    mutations.push(("external inventory", external));
    let mut group_state = base.clone();
    let unselected = group_state
        .reserved_universe_group_states
        .iter_mut()
        .find(|state| !state.selected)
        .expect("unselected reserved universe group");
    unselected.state.push_str(":changed");
    mutations.push(("whole reserved universe group state", group_state));
    let mut defaults = base.clone();
    defaults.parameter_defaults[0].push(b'!');
    mutations.push(("parameter defaults", defaults));
    let mut conflicts = base.clone();
    conflicts.parameter_conflicts[0].push(b'!');
    mutations.push(("parameter conflicts", conflicts));
    let mut records = base.clone();
    records.canonical_records[0].push(b'!');
    mutations.push(("canonical records", records));
    let mut effects = base.clone();
    effects.pending_effects[0].push(b'!');
    mutations.push(("pending effects", effects));
    let mut obligations = base.clone();
    obligations.pending_obligations[0].push(b'!');
    mutations.push(("pending obligations", obligations));

    for (section, mutation) in mutations {
        assert_ne!(mutation, base, "raw {section} mutation changes input");
        let mutated_bytes = canonical_interface_prefix_bytes_from_raw_for_test(&mutation)
            .unwrap_or_else(|error| panic!("encode mutated {section}: {error}"));
        assert_ne!(mutated_bytes, base_bytes, "raw bytes observe {section}");
        assert_ne!(
            sha256(&mutated_bytes),
            base_digest,
            "digest observes {section}"
        );
    }

    let same_id = canonical_reserved_object_same_type_id_witness_for_test()
        .expect("narrow WU0B wrapper mutates an actual reserved object in an actual Store");
    assert_eq!(same_id.original_type_id, same_id.mutated_type_id);
    assert_ne!(same_id.original_store_row, same_id.mutated_store_row);
    let original_sections = &same_id.original_sections;
    let mutated_sections = &same_id.mutated_sections;
    assert_eq!(original_sections.profile, mutated_sections.profile);
    assert_eq!(original_sections.universe, mutated_sections.universe);
    assert_eq!(original_sections.rung, mutated_sections.rung);
    assert_eq!(
        original_sections.component_order_and_membership,
        mutated_sections.component_order_and_membership
    );
    assert_eq!(
        original_sections.external_inventory,
        mutated_sections.external_inventory
    );
    assert_eq!(
        original_sections.reserved_universe_group_states,
        mutated_sections.reserved_universe_group_states
    );
    assert_eq!(
        original_sections.parameter_defaults,
        mutated_sections.parameter_defaults
    );
    assert_eq!(
        original_sections.parameter_conflicts,
        mutated_sections.parameter_conflicts
    );
    assert_eq!(
        original_sections.canonical_records,
        mutated_sections.canonical_records
    );
    assert_eq!(
        original_sections.pending_effects,
        mutated_sections.pending_effects
    );
    assert_eq!(
        original_sections.pending_obligations,
        mutated_sections.pending_obligations
    );
    assert_eq!(
        original_sections.dense_type_store.len(),
        mutated_sections.dense_type_store.len()
    );
    let changed_rows = original_sections
        .dense_type_store
        .iter()
        .zip(&mutated_sections.dense_type_store)
        .filter(|(original, mutated)| original != mutated)
        .collect::<Vec<_>>();
    assert_eq!(changed_rows.len(), 1);
    assert_eq!(changed_rows[0].0.type_id, same_id.original_type_id);
    assert_eq!(changed_rows[0].1.type_id, same_id.mutated_type_id);
    assert_eq!(changed_rows[0].0, &same_id.original_store_row);
    assert_eq!(changed_rows[0].1, &same_id.mutated_store_row);
    let original = canonical_interface_prefix_bytes_from_raw_for_test(&same_id.original_sections)
        .expect("original actual-store sections");
    let mutated = canonical_interface_prefix_bytes_from_raw_for_test(&same_id.mutated_sections)
        .expect("mutated actual-store sections");
    assert_ne!(original, mutated);
    assert_ne!(sha256(&original), sha256(&mutated));
}

#[test]
fn checkpoints_remain_map_free_and_share_the_real_aggregate_limit() {
    let run = measured(&CYCLE_TAINTED, InterfaceFillAttributionMode::Baseline);
    assert!(run.component_checkpoints.len() <= MAX_COMPONENT_CHECKPOINTS);
    assert!(run.component_checkpoints_retained_bytes <= MAX_TRACKED_CHECKPOINT_BYTES);
    let sidecar = include_str!("wu0g_interface_fill_attribution.rs");
    assert_eq!(
        sidecar
            .matches("application_entries: BTreeMap<ApplicationKey, ApplicationEntry>")
            .count(),
        1,
        "exactly one application map owns histogram and Candidate-B state"
    );
    assert_eq!(sidecar.matches("BTreeMap<ApplicationKey").count(), 1);
    assert!(
        type_aliases_whose_rhs_contains(sidecar, "ApplicationKey").is_empty(),
        "no type alias may hide any ApplicationKey state owner"
    );
    let collector = source_item_body(sidecar, "struct InterfaceFillAttributionCollector");
    assert_eq!(
        collector
            .lines()
            .map(compact)
            .filter(|line| line.contains("ApplicationKey"))
            .collect::<Vec<_>>(),
        ["application_entries:BTreeMap<ApplicationKey,ApplicationEntry>,"],
        "the collector has no secondary direct or aliased ApplicationKey owner"
    );
    let application_map_fields = sidecar
        .lines()
        .map(compact)
        .filter(|line| line.contains("ApplicationKey") && line.contains("Map<"))
        .collect::<Vec<_>>();
    assert_eq!(
        application_map_fields,
        ["application_entries:BTreeMap<ApplicationKey,ApplicationEntry>,"]
    );
    let checkpoint = source_item_body(sidecar, "struct InterfaceFillComponentCheckpoint");
    let checkpoint_fields = checkpoint
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(','))
        .map(compact)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        checkpoint_fields,
        [
            "completed_component_identity_sha256:[u8;32],",
            "completed_group_membership_sha256:[u8;32],",
            "pub(super)cumulative:InterfaceFillAttributionSnapshot,",
            "retained_bytes:usize,",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        "checkpoint storage is exactly scalar/digest-only"
    );
    for forbidden in [
        "BTreeMap",
        "FxHashMap",
        "Vec<",
        "ApplicationEntry",
        "CandidateEntry",
        "InterfaceFillAttributionReport",
        "ApplicationKey",
    ] {
        assert!(
            !checkpoint.contains(forbidden),
            "checkpoint retains {forbidden}"
        );
    }
    for forbidden in [
        "Vec<ApplicationKey>",
        "type ApplicationKeyCollection",
        "type ApplicationEntries",
        "application_entries: Vec<",
        "HashMap<ApplicationKey",
        "FxHashMap<ApplicationKey",
        "type ApplicationKeyMap",
        "type ApplicationStateMap",
        "type ApplicationMap",
    ] {
        assert!(
            !sidecar.contains(forbidden),
            "alternate application owner {forbidden}"
        );
    }
}

fn assert_thresholds_rejected(label: &str, raw: &RawThresholdEvidence) {
    assert!(
        !evaluate_interface_fill_thresholds_from_raw_for_test(raw).thresholds_pass(),
        "raw thresholds must fail closed for {label}"
    );
}

fn assert_successful_process_status(status: RawProcessStatus) {
    assert_ne!(status.scope_identity, status.cgroup_identity);
    assert!(status.leader_pid > 0 && status.leader_start_ticks > 0);
    assert_eq!(status.pgid, status.leader_pid);
    assert!(status.readiness_seen && status.membership_verified);
    assert!(status.drain_complete && status.cgroup_populated_zero && status.pgid_empty);
    assert!(status.cgroup_removed && !status.cgroup_retained);
    assert!(!status.scope_abort_requested && !status.scope_abort_observed);
    assert_eq!(status.termination, RawProbeTermination::Complete);
    assert_eq!(status.deadline_readback_ms, status.deadline_ms);
    assert_eq!(
        status.memory_limit_readback_bytes,
        status.memory_limit_bytes
    );
    assert_eq!(status.rss_limit_readback_bytes, status.rss_limit_bytes);
    assert_eq!(status.nofile_soft_readback, status.nofile_soft_limit);
    assert_eq!(status.nofile_hard_readback, status.nofile_hard_limit);
    assert!(status.deadline_ms > 0 && status.deadline_ms <= WU0G_DEADLINE_MS);
    assert!(status.memory_limit_bytes > 0 && status.memory_limit_bytes <= WU0G_MEMORY_LIMIT_BYTES);
    assert!(status.rss_limit_bytes > 0 && status.rss_limit_bytes <= status.memory_limit_bytes);
    assert!(status.max_rss_bytes <= status.rss_limit_bytes);
    assert!(status.nofile_soft_limit > 0);
    assert!(status.nofile_soft_limit <= status.nofile_hard_limit);
    assert!(status.nofile_hard_limit <= WU0G_NOFILE_LIMIT);
    assert_eq!(status.containment_failures, 0);
    assert_eq!(status.cgroup_oom_delta, 0);
    assert_eq!(status.cgroup_oom_kill_delta, 0);
    assert_eq!(status.exit_code, Some(0));
    assert_eq!(status.term_signal, None);
    assert_eq!(status.raw_wait_status, Some(0));
    assert!(status.waited && status.reaped && status.cleanup_succeeded);
}

fn assert_bounded_observation(observation: RawBoundedObservation, expected_cap: u64) {
    assert_eq!(observation.cap_bytes, expected_cap);
    assert!(observation.observed_bytes <= observation.cap_bytes);
    assert!(!observation.truncated);
}

fn assert_hardened_runner_invocation(invocation: &[String]) {
    assert_eq!(invocation.len(), 5);
    assert_eq!(invocation[0], "/usr/bin/perl");
    assert!(invocation[1].ends_with("/tooling/wu0e-diagnostic/run.pl"));
    assert_eq!(invocation[2], "--wu0g-child-v1");
    assert!(!invocation[3].is_empty(), "one canonical request file");
    assert!(!invocation[4].is_empty(), "one canonical result directory");
}

fn assert_exact_perf_invocation(invocation: &[String]) {
    assert_eq!(
        invocation,
        [
            "/usr/bin/perf",
            "stat",
            "--no-big-num",
            "--no-scale",
            "-x",
            ";",
            "-e",
            WU0G_PERF_EVENT,
            "--log-fd",
            "198",
            "--",
            "/proc/self/fd/197",
            "--ignored",
            "--exact",
            "check::checker::decls::wu0g_interface_fill_attribution_spec::wu0g_hardened_child_once",
            "--nocapture",
        ]
    );
}

fn assert_exact_causal_process_matrix(raw: &RawThresholdEvidence, expected_rungs: &[String]) {
    let expected = expected_rungs
        .iter()
        .enumerate()
        .flat_map(|(rung_ordinal, rung)| {
            [RawProbeMode::Baseline, RawProbeMode::CandidateB]
                .into_iter()
                .map(move |mode| {
                    (
                        u8::try_from(rung_ordinal).expect("rung ordinal fits u8"),
                        rung.clone(),
                        mode,
                    )
                })
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(expected.len(), 10);
    let observed = raw
        .causal_process_dossiers
        .iter()
        .map(|dossier| {
            (
                dossier.rung_ordinal,
                dossier.rung_identity.claimed_sha256.clone(),
                dossier.mode,
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(observed, expected);
    assert_eq!(raw.causal_process_dossiers.len(), expected.len());
    let mut children = BTreeSet::new();
    let mut requests = BTreeSet::new();
    let mut sentinels = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    for dossier in &raw.causal_process_dossiers {
        assert_eq!(dossier.plan_identity, raw.plan_identity);
        assert_eq!(dossier.profile_identity, raw.profile_identity);
        assert_eq!(dossier.universe_identity, raw.universe_identity);
        assert_eq!(dossier.workload_identity, raw.workload_identity);
        assert_eq!(dossier.candidate_identity, raw.candidate_identity);
        assert_eq!(dossier.prlimit_identity, raw.prlimit_identity);
        assert_hardened_runner_invocation(&dossier.canonical_invocation);
        assert!(requests.insert(dossier.request_content_identity.claimed_sha256.clone()));
        assert!(children.insert(dossier.child_identity.claimed_sha256.clone()));
        assert!(sentinels.insert(dossier.completion_sentinel_identity.claimed_sha256.clone()));
        assert!(artifacts.insert(dossier.child_artifact_identity.claimed_sha256.clone()));
        for (observation, cap) in [
            (dossier.stdout_observation, WU0G_STDOUT_CAP_BYTES),
            (dossier.stderr_observation, WU0G_STDERR_CAP_BYTES),
            (dossier.request_observation, WU0G_REQUEST_CAP_BYTES),
            (dossier.result_observation, WU0G_RESULT_CAP_BYTES),
            (dossier.sentinel_observation, WU0G_SENTINEL_CAP_BYTES),
            (
                dossier.semantic_artifact_observation,
                WU0G_ARTIFACT_CAP_BYTES,
            ),
        ] {
            assert_bounded_observation(observation, cap);
        }
        assert_successful_process_status(dossier.process_status());
        let row = raw
            .ladder_rows
            .iter()
            .find(|row| row.baseline.rung_identity == dossier.rung_identity)
            .expect("causal process rung belongs to ladder");
        let point = match dossier.mode {
            RawProbeMode::Baseline => &row.baseline,
            RawProbeMode::CandidateB => &row.candidate,
        };
        assert_eq!(dossier.report_identity, point.report_identity);
        assert_eq!(dossier.semantic_identity, point.semantic_identity);
        assert_eq!(
            dossier.binary_identity,
            match dossier.mode {
                RawProbeMode::Baseline => raw.baseline_binary_identity.clone(),
                RawProbeMode::CandidateB => raw.candidate_binary_identity.clone(),
            }
        );
    }
    assert_eq!(children.len(), 10);
    assert_eq!(requests.len(), 10);
    assert_eq!(sentinels.len(), 10);
    assert_eq!(artifacts.len(), 10);
}

fn assert_exact_performance_schedule(raw: &RawThresholdEvidence) {
    let expected = [
        (0, RawLaunchOrder::Ab, 0, RawProbeMode::Baseline),
        (0, RawLaunchOrder::Ab, 1, RawProbeMode::CandidateB),
        (1, RawLaunchOrder::Ba, 2, RawProbeMode::CandidateB),
        (1, RawLaunchOrder::Ba, 3, RawProbeMode::Baseline),
        (2, RawLaunchOrder::Ab, 4, RawProbeMode::Baseline),
        (2, RawLaunchOrder::Ab, 5, RawProbeMode::CandidateB),
        (3, RawLaunchOrder::Ba, 6, RawProbeMode::CandidateB),
        (3, RawLaunchOrder::Ba, 7, RawProbeMode::Baseline),
        (4, RawLaunchOrder::Ab, 8, RawProbeMode::Baseline),
        (4, RawLaunchOrder::Ab, 9, RawProbeMode::CandidateB),
    ];
    let observed = raw
        .performance_pairs
        .iter()
        .flat_map(|pair| {
            pair.launches.iter().map(|launch| {
                (
                    pair.pair_ordinal,
                    pair.order,
                    launch.launch_ordinal,
                    launch.mode,
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed, expected,
        "exact ordered interleaved AB/BA schedule"
    );
    assert_eq!(raw.performance_pairs.len(), 5);
    let mut children = BTreeSet::new();
    let mut launches = BTreeSet::new();
    let mut requests = BTreeSet::new();
    let mut sentinels = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut perf_artifacts = BTreeSet::new();
    let mut pairs = BTreeSet::new();
    for pair in &raw.performance_pairs {
        assert_eq!(pair.launches.len(), 2);
        assert!(pairs.insert(pair.pair_identity.claimed_sha256.clone()));
        for launch in &pair.launches {
            assert_eq!(launch.pair_ordinal, pair.pair_ordinal);
            assert_eq!(launch.pair_identity, pair.pair_identity);
            assert_eq!(launch.plan_identity, raw.plan_identity);
            assert_eq!(
                launch.representative_workload_identity,
                raw.representative_workload_identity
            );
            assert_eq!(launch.candidate_identity, raw.candidate_identity);
            assert_eq!(launch.prlimit_identity, raw.prlimit_identity);
            assert_eq!(launch.perf_identity, raw.perf_identity);
            assert_eq!(launch.host_identity, raw.host_identity);
            assert_eq!(launch.perf_version, raw.perf_version);
            assert_eq!(launch.perf_event, raw.perf_event);
            assert_eq!(launch.cpu_affinity, raw.cpu_affinity);
            assert_hardened_runner_invocation(&launch.canonical_invocation);
            assert_exact_perf_invocation(&launch.perf_invocation);
            assert!(launches.insert(launch.launch_identity.claimed_sha256.clone()));
            assert!(requests.insert(launch.request_content_identity.claimed_sha256.clone()));
            assert!(children.insert(launch.child_identity.claimed_sha256.clone()));
            assert!(sentinels.insert(launch.completion_sentinel_identity.claimed_sha256.clone()));
            assert!(artifacts.insert(launch.child_artifact_identity.claimed_sha256.clone()));
            assert!(perf_artifacts.insert(launch.perf_artifact_identity.claimed_sha256.clone()));
            for (observation, cap) in [
                (launch.stdout_observation, WU0G_STDOUT_CAP_BYTES),
                (launch.stderr_observation, WU0G_STDERR_CAP_BYTES),
                (launch.request_observation, WU0G_REQUEST_CAP_BYTES),
                (launch.result_observation, WU0G_RESULT_CAP_BYTES),
                (launch.sentinel_observation, WU0G_SENTINEL_CAP_BYTES),
                (
                    launch.semantic_artifact_observation,
                    WU0G_ARTIFACT_CAP_BYTES,
                ),
                (
                    launch.perf_artifact_observation,
                    WU0G_PERF_ARTIFACT_CAP_BYTES,
                ),
            ] {
                assert_bounded_observation(observation, cap);
            }
            assert!(launch.instructions > 0 && launch.perf_runtime > 0);
            assert_eq!(launch.perf_exit_code, Some(0));
            assert_eq!(launch.perf_term_signal, None);
            assert_eq!(launch.perf_raw_wait_status, Some(0));
            assert!(launch.perf_waited);
            assert_successful_process_status(launch.process_status());
        }
        assert_eq!(
            pair.launches[0].semantic_identity,
            pair.launches[1].semantic_identity
        );
    }
    assert_eq!(children.len(), 10);
    assert_eq!(launches.len(), 10);
    assert_eq!(requests.len(), 10);
    assert_eq!(sentinels.len(), 10);
    assert_eq!(artifacts.len(), 10);
    assert_eq!(perf_artifacts.len(), 10);
    assert_eq!(pairs.len(), 5);
    assert_eq!(
        raw.performance_pairs
            .iter()
            .flat_map(|pair| &pair.launches)
            .map(|launch| &launch.semantic_identity)
            .collect::<BTreeSet<_>>()
            .len(),
        1,
        "all five AB/BA pairs share one semantic identity"
    );
}

#[test]
fn raw_authenticated_dossiers_derive_thresholds_and_synthetic_never_authorizes() {
    let raw = passing_raw_threshold_evidence();
    assert_eq!(experiment_identity_from_raw(&raw), raw.experiment_identity);
    let decision = evaluate_interface_fill_thresholds_from_raw_for_test(&raw);
    assert_eq!(decision.incremental_causality_basis_points(), 5_000);
    assert_eq!(decision.target_counter_reduction_basis_points(), 5_000);
    assert_eq!(decision.predicted_end_to_end_basis_points(), 2_000);
    assert_eq!(decision.median_wall_improvement_basis_points(), 2_000);
    assert_eq!(
        decision.median_instruction_improvement_basis_points(),
        2_000
    );
    assert!(decision.thresholds_pass());
    let expected_rungs = [1_250_u16, 2_500, 5_000, 7_500, 10_000]
        .into_iter()
        .enumerate()
        .map(|(index, target)| {
            raw_identity(
                "wu0g-rung-v1",
                format!("target={target};prefix={}", index + 1).into_bytes(),
            )
            .claimed_sha256
        })
        .collect::<Vec<_>>();
    assert_exact_causal_process_matrix(&raw, &expected_rungs);
    assert_exact_performance_schedule(&raw);
    let mut launch_owned_identities = BTreeSet::new();
    for dossier in &raw.causal_process_dossiers {
        for identity in [
            &dossier.request_content_identity,
            &dossier.request_path_identity,
            &dossier.result_identity,
            &dossier.result_content_identity,
            &dossier.result_path_identity,
            &dossier.child_identity,
            &dossier.completion_sentinel_identity,
            &dossier.child_artifact_identity,
        ] {
            assert!(launch_owned_identities.insert(identity.claimed_sha256.clone()));
        }
    }
    for launch in raw.performance_pairs.iter().flat_map(|pair| &pair.launches) {
        for identity in [
            &launch.request_content_identity,
            &launch.request_path_identity,
            &launch.result_identity,
            &launch.result_content_identity,
            &launch.result_path_identity,
            &launch.launch_identity,
            &launch.child_identity,
            &launch.completion_sentinel_identity,
            &launch.child_artifact_identity,
            &launch.perf_artifact_identity,
        ] {
            assert!(launch_owned_identities.insert(identity.claimed_sha256.clone()));
        }
    }
    for identities in [
        raw.cycle_profiles
            .iter()
            .map(|dossier| dossier.profile_identity.claimed_sha256.as_str())
            .collect::<Vec<_>>(),
        raw.cycle_profiles
            .iter()
            .map(|dossier| dossier.workload_identity.claimed_sha256.as_str())
            .collect::<Vec<_>>(),
        raw.cycle_profiles
            .iter()
            .map(|dossier| dossier.report_identity.claimed_sha256.as_str())
            .collect::<Vec<_>>(),
        raw.cycle_profiles
            .iter()
            .map(|dossier| dossier.artifact_identity.claimed_sha256.as_str())
            .collect::<Vec<_>>(),
        raw.controls
            .iter()
            .map(|dossier| dossier.profile_identity.claimed_sha256.as_str())
            .collect::<Vec<_>>(),
        raw.controls
            .iter()
            .map(|dossier| dossier.workload_identity.claimed_sha256.as_str())
            .collect::<Vec<_>>(),
        raw.controls
            .iter()
            .map(|dossier| dossier.baseline_report_identity.claimed_sha256.as_str())
            .collect::<Vec<_>>(),
        raw.controls
            .iter()
            .map(|dossier| dossier.baseline_artifact_identity.claimed_sha256.as_str())
            .collect::<Vec<_>>(),
    ] {
        assert_eq!(identities.iter().copied().collect::<BTreeSet<_>>().len(), 3);
    }
    assert!(raw.cycle_profiles.iter().all(|dossier| {
        dossier.profile_identity != dossier.workload_identity
            && dossier.report_identity != dossier.artifact_identity
            && dossier.artifact_identity != dossier.artifact_content_identity
    }));
    let synthetic = RawAuthorizationEvidence {
        evidence_domain: RawEvidenceDomain::Synthetic,
        thresholds: raw,
    };
    assert!(!validate_interface_fill_authorization_from_raw_for_test(&synthetic).authorizes());
    let forged = RawAuthorizationEvidence {
        evidence_domain: RawEvidenceDomain::PinnedDom82,
        thresholds: synthetic.thresholds.clone(),
    };
    assert!(!validate_interface_fill_authorization_from_raw_for_test(&forged).authorizes());
}

#[test]
fn all_counter_series_and_raw_dossier_families_fail_closed() {
    let base = passing_raw_threshold_evidence();
    let mut mutations = Vec::new();
    for (label, mutate) in [
        ("baseline target", 0_u8),
        ("baseline total", 1),
        ("candidate target", 2),
        ("candidate total", 3),
    ] {
        let mut raw = base.clone();
        match mutate {
            0 => raw.ladder_rows[3].baseline.target_counter = 1,
            1 => raw.ladder_rows[3].baseline.total_work_counter = 1,
            2 => raw.ladder_rows[3].candidate.target_counter = 1,
            3 => raw.ladder_rows[3].candidate.total_work_counter = 1,
            _ => unreachable!(),
        }
        mutations.push((label, raw));
    }
    let mut missing_cycle = base.clone();
    missing_cycle.cycle_profiles.pop();
    mutations.push(("missing cycle dossier", missing_cycle));
    let mut extra_cycle = base.clone();
    extra_cycle
        .cycle_profiles
        .push(extra_cycle.cycle_profiles[0].clone());
    mutations.push(("extra cycle dossier", extra_cycle));
    let mut zero_cycle = base.clone();
    zero_cycle.cycle_profiles[0].total_cycle_visits = 0;
    mutations.push(("zero cycle denominator", zero_cycle));
    let mut over_cycle = base.clone();
    over_cycle.cycle_profiles[0].explained_cycle_visits = 11;
    mutations.push(("cycle ratio over one", over_cycle));
    let mut weak_cycle = base.clone();
    weak_cycle.cycle_profiles[0].explained_cycle_visits = 2_999;
    weak_cycle.cycle_profiles[0].total_cycle_visits = 10_000;
    mutations.push(("weak cycle ratio", weak_cycle));
    let mut missing_prediction = base.clone();
    missing_prediction.predictions.clear();
    mutations.push(("missing prediction dossier", missing_prediction));
    let mut extra_prediction = base.clone();
    extra_prediction
        .predictions
        .push(extra_prediction.predictions[0].clone());
    mutations.push(("extra prediction dossier", extra_prediction));
    let mut zero_prediction = base.clone();
    zero_prediction.predictions[0].total_end_to_end_work = 0;
    mutations.push(("zero prediction denominator", zero_prediction));
    let mut zero_baseline_target = base.clone();
    zero_baseline_target.predictions[0].baseline_target_work = 0;
    mutations.push(("zero baseline target work", zero_baseline_target));
    let mut candidate_over_baseline = base.clone();
    candidate_over_baseline.predictions[0].candidate_target_work = 501;
    mutations.push(("candidate target exceeds baseline", candidate_over_baseline));
    let mut over_prediction = base.clone();
    over_prediction.predictions[0].attributed_target_work = 1_001;
    mutations.push(("prediction ratio over one", over_prediction));
    let mut weak_prediction = base.clone();
    weak_prediction.predictions[0].candidate_target_work = 301;
    mutations.push(("weak checked prediction formula", weak_prediction));
    let mut missing_control = base.clone();
    missing_control.controls.pop();
    mutations.push(("missing control dossier", missing_control));
    let mut extra_control = base.clone();
    extra_control
        .controls
        .push(extra_control.controls[0].clone());
    mutations.push(("extra control dossier", extra_control));
    let mut zero_control = base.clone();
    zero_control.controls[0].baseline_measurement = 0;
    mutations.push(("zero control denominator", zero_control));
    let mut over_control = base.clone();
    over_control.controls[0].candidate_measurement = 20_001;
    mutations.push(("control ratio over one", over_control));
    let mut regressed_control = base.clone();
    regressed_control.controls[0].candidate_measurement = 10_201;
    mutations.push(("control regression above 200bp", regressed_control));
    for (label, mut raw) in mutations {
        assert_ne!(raw, base, "{label} mutation is non-noop");
        bind_plan_and_experiment_identities(&mut raw);
        assert_thresholds_rejected(label, &raw);
    }
}

#[test]
fn exact_process_schedule_and_containment_status_fail_closed() {
    let base = passing_raw_threshold_evidence();
    let mut mutations = Vec::new();
    let mut missing_causal = base.clone();
    missing_causal.causal_process_dossiers.pop();
    mutations.push(("missing causal process", missing_causal));
    let mut extra_causal = base.clone();
    extra_causal
        .causal_process_dossiers
        .push(extra_causal.causal_process_dossiers[0].clone());
    mutations.push(("extra causal process", extra_causal));
    let mut missing_pair = base.clone();
    missing_pair.performance_pairs.pop();
    mutations.push(("missing performance pair", missing_pair));
    let mut extra_pair = base.clone();
    extra_pair
        .performance_pairs
        .push(extra_pair.performance_pairs[0].clone());
    mutations.push(("extra performance pair", extra_pair));
    let mut order = base.clone();
    order.performance_pairs[1].order = RawLaunchOrder::Ab;
    mutations.push(("AB/BA order", order));
    let mut missing_launch = base.clone();
    missing_launch.performance_pairs[0].launches.pop();
    mutations.push(("missing performance launch", missing_launch));
    let mut duplicate_launch = base.clone();
    let repeated_launch = duplicate_launch.performance_pairs[0].launches[0].clone();
    duplicate_launch.performance_pairs[0]
        .launches
        .push(repeated_launch);
    mutations.push(("duplicate performance launch", duplicate_launch));
    let mut causal_rung_ordinal = base.clone();
    causal_rung_ordinal.causal_process_dossiers[0].rung_ordinal = 5;
    mutations.push(("invalid causal rung ordinal", causal_rung_ordinal));
    let mut pair_ordinal = base.clone();
    pair_ordinal.performance_pairs[0].pair_ordinal = 5;
    mutations.push(("invalid pair ordinal", pair_ordinal));
    let mut launch_ordinal = base.clone();
    launch_ordinal.performance_pairs[0].launches[0].launch_ordinal = 10;
    mutations.push(("invalid launch ordinal", launch_ordinal));
    let mut duplicate_pair_identity = base.clone();
    duplicate_pair_identity.performance_pairs[1].pair_identity = duplicate_pair_identity
        .performance_pairs[0]
        .pair_identity
        .clone();
    mutations.push(("duplicate pair identity", duplicate_pair_identity));
    let mut duplicate_launch_identity = base.clone();
    duplicate_launch_identity.performance_pairs[0].launches[1].launch_identity =
        duplicate_launch_identity.performance_pairs[0].launches[0]
            .launch_identity
            .clone();
    mutations.push(("duplicate launch identity", duplicate_launch_identity));
    let mut duplicate_request = base.clone();
    duplicate_request.performance_pairs[0].launches[1].request_content_identity =
        duplicate_request.performance_pairs[0].launches[0]
            .request_content_identity
            .clone();
    mutations.push(("duplicate request identity", duplicate_request));
    let mut duplicate_sentinel = base.clone();
    duplicate_sentinel.performance_pairs[0].launches[1].completion_sentinel_identity =
        duplicate_sentinel.performance_pairs[0].launches[0]
            .completion_sentinel_identity
            .clone();
    mutations.push(("duplicate completion sentinel", duplicate_sentinel));
    let mut duplicate_perf_artifact = base.clone();
    duplicate_perf_artifact.performance_pairs[0].launches[1].perf_artifact_identity =
        duplicate_perf_artifact.performance_pairs[0].launches[0]
            .perf_artifact_identity
            .clone();
    mutations.push(("duplicate perf artifact", duplicate_perf_artifact));
    let mut child = base.clone();
    child.performance_pairs[0].launches[1].child_identity = child.performance_pairs[0].launches[0]
        .child_identity
        .clone();
    mutations.push(("duplicate child", child));
    let mut artifact = base.clone();
    artifact.performance_pairs[0].launches[1].child_artifact_identity =
        artifact.performance_pairs[0].launches[0]
            .child_artifact_identity
            .clone();
    mutations.push(("duplicate child artifact", artifact));
    let mut causal_child = base.clone();
    causal_child.causal_process_dossiers[1].child_identity = causal_child.causal_process_dossiers
        [0]
    .child_identity
    .clone();
    mutations.push(("duplicate causal child", causal_child));
    let mut causal_artifact = base.clone();
    causal_artifact.causal_process_dossiers[1].child_artifact_identity = causal_artifact
        .causal_process_dossiers[0]
        .child_artifact_identity
        .clone();
    mutations.push(("duplicate causal child artifact", causal_artifact));
    let mut cycle_profile = base.clone();
    cycle_profile.cycle_profiles[1].profile_identity =
        cycle_profile.cycle_profiles[0].profile_identity.clone();
    mutations.push(("duplicate cycle profile identity", cycle_profile));
    let mut control_workload = base.clone();
    control_workload.controls[1].workload_identity =
        control_workload.controls[0].workload_identity.clone();
    mutations.push(("duplicate control workload identity", control_workload));
    let mut cycle_binary = base.clone();
    cycle_binary.cycle_profiles[0].binary_identity = base.baseline_binary_identity.clone();
    mutations.push(("cycle cross-binary substitution", cycle_binary));
    let mut prediction_report = base.clone();
    prediction_report.predictions[0].report_identity =
        base.cycle_profiles[0].report_identity.clone();
    mutations.push(("prediction cross-report substitution", prediction_report));
    let mut control_report = base.clone();
    control_report.controls[0].candidate_report_identity =
        base.controls[0].baseline_report_identity.clone();
    mutations.push(("control cross-report substitution", control_report));
    let mut workload = base.clone();
    workload.performance_pairs[0].launches[0].representative_workload_identity =
        workload.workload_identity.clone();
    mutations.push(("causal workload masquerades as repetition", workload));
    let mut zero_deadline = base.clone();
    zero_deadline.causal_process_dossiers[0].deadline_ms = 0;
    mutations.push(("zero deadline", zero_deadline));
    let mut oversized_deadline = base.clone();
    oversized_deadline.causal_process_dossiers[0].deadline_ms = WU0G_DEADLINE_MS + 1;
    mutations.push(("oversized deadline", oversized_deadline));
    let mut deadline_readback = base.clone();
    deadline_readback.performance_pairs[0].launches[0].deadline_readback_ms -= 1;
    mutations.push(("deadline readback mismatch", deadline_readback));
    let mut zero_memory = base.clone();
    zero_memory.causal_process_dossiers[0].memory_limit_bytes = 0;
    mutations.push(("zero memory limit", zero_memory));
    let mut oversized_memory = base.clone();
    oversized_memory.performance_pairs[0].launches[0].memory_limit_bytes =
        WU0G_MEMORY_LIMIT_BYTES + 1;
    mutations.push(("oversized memory limit", oversized_memory));
    let mut memory_readback = base.clone();
    memory_readback.causal_process_dossiers[0].memory_limit_readback_bytes -= 1;
    mutations.push(("memory readback mismatch", memory_readback));
    let mut zero_rss = base.clone();
    zero_rss.performance_pairs[0].launches[0].rss_limit_bytes = 0;
    mutations.push(("zero RSS limit", zero_rss));
    let mut oversized_rss = base.clone();
    oversized_rss.causal_process_dossiers[0].rss_limit_bytes = WU0G_MEMORY_LIMIT_BYTES + 1;
    mutations.push(("oversized RSS limit", oversized_rss));
    let mut rss_readback = base.clone();
    rss_readback.performance_pairs[0].launches[0].rss_limit_readback_bytes -= 1;
    mutations.push(("RSS readback mismatch", rss_readback));
    let mut zero_nofile = base.clone();
    zero_nofile.causal_process_dossiers[0].nofile_soft_limit = 0;
    mutations.push(("zero nofile limit", zero_nofile));
    let mut oversized_nofile = base.clone();
    oversized_nofile.performance_pairs[0].launches[0].nofile_hard_limit = WU0G_NOFILE_LIMIT + 1;
    mutations.push(("oversized nofile limit", oversized_nofile));
    let mut nofile_readback = base.clone();
    nofile_readback.causal_process_dossiers[0].nofile_soft_readback -= 1;
    mutations.push(("nofile readback mismatch", nofile_readback));
    let mut nofile_hard_readback = base.clone();
    nofile_hard_readback.causal_process_dossiers[0].nofile_hard_readback -= 1;
    mutations.push(("nofile hard readback mismatch", nofile_hard_readback));
    let mut max_rss_over_limit = base.clone();
    max_rss_over_limit.performance_pairs[0].launches[0].max_rss_bytes = WU0G_RSS_LIMIT_BYTES + 1;
    mutations.push(("max RSS exceeds RSS limit", max_rss_over_limit));
    let mut inserted_runner_arg = base.clone();
    inserted_runner_arg.causal_process_dossiers[0]
        .canonical_invocation
        .insert(2, "--verbose".to_owned());
    mutations.push(("inserted runner invocation argument", inserted_runner_arg));
    let mut deleted_runner_arg = base.clone();
    deleted_runner_arg.causal_process_dossiers[0]
        .canonical_invocation
        .remove(2);
    mutations.push(("deleted runner invocation argument", deleted_runner_arg));
    let mut trailing_runner_arg = base.clone();
    trailing_runner_arg.causal_process_dossiers[0]
        .canonical_invocation
        .push("--extra".to_owned());
    mutations.push(("trailing runner invocation argument", trailing_runner_arg));
    for (label, position, replacement) in [
        ("altered perf command", 0, "/tmp/perf"),
        ("altered perf subcommand", 1, "record"),
        ("removed no-big-num", 2, "--quiet"),
        ("altered no-scale", 3, "--scale"),
        ("altered separator", 5, ","),
        ("altered perf event", 7, "cycles:u"),
        ("altered perf log fd", 8, "--output"),
        ("perf attach mode", 10, "-p"),
        ("perf shell payload", 11, "/bin/sh"),
    ] {
        let mut raw = base.clone();
        raw.performance_pairs[0].launches[0].perf_invocation[position] = replacement.to_owned();
        mutations.push((label, raw));
    }
    for (label, token) in [
        ("insert perf system-wide", "-a"),
        ("insert perf pid", "--pid"),
        ("insert perf tid", "--tid"),
        ("insert perf cgroup", "--cgroup"),
        ("insert perf no-inherit", "--no-inherit"),
    ] {
        let mut raw = base.clone();
        raw.performance_pairs[0].launches[0]
            .perf_invocation
            .insert(2, token.to_owned());
        mutations.push((label, raw));
    }
    let mut deleted_perf_arg = base.clone();
    deleted_perf_arg.performance_pairs[0].launches[0]
        .perf_invocation
        .remove(2);
    mutations.push(("deleted perf argv", deleted_perf_arg));
    let mut trailing_perf_arg = base.clone();
    trailing_perf_arg.performance_pairs[0].launches[0]
        .perf_invocation
        .push("-a".to_owned());
    mutations.push(("trailing perf argv", trailing_perf_arg));
    let mut perf_version = base.clone();
    perf_version.performance_pairs[0].launches[0]
        .perf_version
        .push_str(" drift");
    mutations.push(("perf version drift", perf_version));
    let mut perf_event = base.clone();
    perf_event.performance_pairs[0].launches[0].perf_event = "cycles:u".to_owned();
    mutations.push(("perf event drift", perf_event));
    let mut affinity = base.clone();
    affinity.performance_pairs[0].launches[0].cpu_affinity = "1".to_owned();
    mutations.push(("CPU affinity drift", affinity));
    let mut perf_identity = base.clone();
    perf_identity.performance_pairs[0].launches[0].perf_identity = raw_identity(
        "wu0g-executable-v1",
        b"different authenticated perf".to_vec(),
    );
    mutations.push(("perf identity drift", perf_identity));
    let mut prlimit_identity = base.clone();
    prlimit_identity.causal_process_dossiers[0].prlimit_identity = raw_identity(
        "wu0g-executable-v1",
        b"different authenticated prlimit".to_vec(),
    );
    mutations.push(("prlimit identity drift", prlimit_identity));
    let mut libtest_identity = base.clone();
    libtest_identity.performance_pairs[0].launches[0].binary_identity = raw_identity(
        "wu0g-binary-v1",
        b"different authenticated libtest".to_vec(),
    );
    mutations.push(("libtest identity drift", libtest_identity));
    let mut host_identity = base.clone();
    host_identity.performance_pairs[0].launches[0].host_identity =
        raw_identity("wu0g-host-v1", b"different host".to_vec());
    mutations.push(("host identity drift", host_identity));
    let mut leader_identity = base.clone();
    leader_identity.causal_process_dossiers[0].leader_start_ticks = 0;
    mutations.push(("missing leader start", leader_identity));
    let mut pgid = base.clone();
    pgid.causal_process_dossiers[0].pgid += 1;
    mutations.push(("PGID mismatch", pgid));
    let mut readiness = base.clone();
    readiness.causal_process_dossiers[0].readiness_seen = false;
    mutations.push(("readiness missing", readiness));
    let mut membership = base.clone();
    membership.performance_pairs[0].launches[0].membership_verified = false;
    mutations.push(("membership missing", membership));
    let mut drain = base.clone();
    drain.causal_process_dossiers[0].drain_complete = false;
    mutations.push(("drain incomplete", drain));
    let mut populated = base.clone();
    populated.performance_pairs[0].launches[0].cgroup_populated_zero = false;
    mutations.push(("cgroup still populated", populated));
    let mut pgid_empty = base.clone();
    pgid_empty.causal_process_dossiers[0].pgid_empty = false;
    mutations.push(("PGID not empty", pgid_empty));
    let mut retained = base.clone();
    retained.performance_pairs[0].launches[0].cgroup_removed = false;
    retained.performance_pairs[0].launches[0].cgroup_retained = true;
    retained.performance_pairs[0].launches[0].scope_abort_requested = true;
    retained.performance_pairs[0].launches[0].scope_abort_observed = true;
    mutations.push(("retained cgroup is inconclusive", retained));
    let mut raw_wait = base.clone();
    raw_wait.causal_process_dossiers[0].raw_wait_status = Some(1);
    mutations.push(("raw wait mismatch", raw_wait));
    let mut perf_raw_wait = base.clone();
    perf_raw_wait.performance_pairs[0].launches[0].perf_raw_wait_status = Some(1);
    mutations.push(("perf raw wait mismatch", perf_raw_wait));
    let mut output_cap = base.clone();
    output_cap.causal_process_dossiers[0]
        .stdout_observation
        .cap_bytes -= 1;
    mutations.push(("stdout cap mismatch", output_cap));
    let mut result_size = base.clone();
    result_size.performance_pairs[0].launches[0]
        .result_observation
        .observed_bytes = WU0G_RESULT_CAP_BYTES + 1;
    mutations.push(("result size over cap", result_size));
    let mut sentinel_truncated = base.clone();
    sentinel_truncated.causal_process_dossiers[0]
        .sentinel_observation
        .truncated = true;
    mutations.push(("sentinel truncated", sentinel_truncated));
    let mut coherent_all_five_semantic_mismatch = base.clone();
    let mut distinct_pair_semantics = BTreeSet::new();
    for pair in &mut coherent_all_five_semantic_mismatch.performance_pairs {
        let alternate_semantic = raw_identity(
            "wu0g-semantic-v1",
            format!("coherent-pair-{}-semantic-mismatch", pair.pair_ordinal).into_bytes(),
        );
        assert!(distinct_pair_semantics.insert(alternate_semantic.claimed_sha256.clone()));
        for launch in &mut pair.launches {
            launch.semantic_identity = alternate_semantic.clone();
        }
    }
    assert_eq!(distinct_pair_semantics.len(), 5);
    assert!(coherent_all_five_semantic_mismatch
        .performance_pairs
        .iter()
        .all(|pair| pair.launches[0].semantic_identity == pair.launches[1].semantic_identity));
    mutations.push((
        "coherent all-five semantic mismatch",
        coherent_all_five_semantic_mismatch,
    ));
    let mut zero_instructions = base.clone();
    zero_instructions.performance_pairs[0].launches[0].instructions = 0;
    mutations.push(("zero perf count", zero_instructions));
    let mut zero_perf_runtime = base.clone();
    zero_perf_runtime.performance_pairs[0].launches[0].perf_runtime = 0;
    mutations.push(("zero perf runtime", zero_perf_runtime));
    let mut zero_candidate_instructions = base.clone();
    zero_candidate_instructions.performance_pairs[0].launches[1].instructions = 0;
    mutations.push(("zero candidate instructions", zero_candidate_instructions));
    let mut zero_candidate_wall = base.clone();
    zero_candidate_wall.performance_pairs[0].launches[1].wall_ns = 0;
    mutations.push(("zero candidate wall", zero_candidate_wall));
    let mut perf_exit = base.clone();
    perf_exit.performance_pairs[0].launches[0].perf_exit_code = Some(1);
    mutations.push(("perf exit contradicts sentinel", perf_exit));
    let mut perf_signal = base.clone();
    perf_signal.performance_pairs[0].launches[0].perf_term_signal = Some(9);
    mutations.push(("perf signal contradicts sentinel", perf_signal));
    let mut perf_wait = base.clone();
    perf_wait.performance_pairs[0].launches[0].perf_waited = false;
    mutations.push(("perf wait missing", perf_wait));
    for (label, termination) in [
        ("deadline partial perf count", RawProbeTermination::Deadline),
        ("OOM partial perf count", RawProbeTermination::MemoryLimit),
        (
            "no-progress partial perf count",
            RawProbeTermination::NoProgress,
        ),
        (
            "stdout partial perf count",
            RawProbeTermination::StdoutLimit,
        ),
        (
            "stderr partial perf count",
            RawProbeTermination::StderrLimit,
        ),
        (
            "infrastructure partial perf count",
            RawProbeTermination::Infrastructure,
        ),
    ] {
        let mut raw = base.clone();
        raw.performance_pairs[0].launches[0].termination = termination;
        assert!(raw.performance_pairs[0].launches[0].instructions > 0);
        mutations.push((label, raw));
    }
    let mut oom = base.clone();
    oom.causal_process_dossiers[0].cgroup_oom_delta = 1;
    mutations.push(("cgroup oom delta", oom));
    let mut oom_kill = base.clone();
    oom_kill.performance_pairs[0].launches[0].cgroup_oom_kill_delta = 1;
    mutations.push(("cgroup oom_kill delta", oom_kill));
    let mut exit = base.clone();
    exit.causal_process_dossiers[0].exit_code = Some(1);
    mutations.push(("child exit", exit));
    let mut signal = base.clone();
    signal.performance_pairs[0].launches[0].term_signal = Some(9);
    mutations.push(("child signal", signal));
    let mut wait = base.clone();
    wait.causal_process_dossiers[0].waited = false;
    mutations.push(("child wait", wait));
    let mut reap = base.clone();
    reap.performance_pairs[0].launches[0].reaped = false;
    mutations.push(("child reap", reap));
    let mut cleanup = base.clone();
    cleanup.causal_process_dossiers[0].cleanup_succeeded = false;
    mutations.push(("child cleanup", cleanup));
    let mut perf_cleanup = base.clone();
    perf_cleanup.performance_pairs[0].launches[0].cleanup_succeeded = false;
    assert!(perf_cleanup.performance_pairs[0].launches[0].instructions > 0);
    mutations.push(("perf count after cleanup failure", perf_cleanup));
    for (label, mut raw) in mutations {
        assert_ne!(raw, base);
        bind_plan_and_experiment_identities(&mut raw);
        assert_thresholds_rejected(label, &raw);
    }

    let mut result_binding = base.clone();
    result_binding.causal_process_dossiers[0].result_bound_request_content_identity =
        result_binding.causal_process_dossiers[1]
            .request_content_identity
            .clone();
    assert_ne!(
        result_binding.causal_process_dossiers[0].result_bound_request_content_identity,
        result_binding.causal_process_dossiers[0].request_content_identity,
    );
    assert_eq!(plan_identity_from_raw(&result_binding), base.plan_identity);
    result_binding.experiment_identity = experiment_identity_from_raw(&result_binding);
    assert_eq!(
        experiment_identity_from_raw(&result_binding),
        result_binding.experiment_identity,
    );
    assert_thresholds_rejected("result request binding replay", &result_binding);
}

#[test]
fn experiment_identity_is_length_framed_and_binds_every_dossier() {
    let base = passing_raw_threshold_evidence();
    assert_eq!(plan_identity_from_raw(&base), base.plan_identity);
    assert_eq!(
        experiment_identity_from_raw(&base),
        base.experiment_identity
    );
    let mut shifted = base.clone();
    let launch = &mut shifted.performance_pairs[0].launches[0];
    let original_left = launch.perf_version.as_bytes().to_vec();
    let original_right = launch.perf_event.as_bytes().to_vec();
    let byte = launch
        .perf_event
        .chars()
        .next()
        .expect("nonempty adjacent perf event");
    assert!(byte.is_ascii(), "the perf protocol is ASCII");
    launch.perf_version.push(byte);
    launch.perf_event.remove(0);
    assert_eq!(
        [original_left, original_right].concat(),
        [
            launch.perf_version.as_bytes().to_vec(),
            launch.perf_event.as_bytes().to_vec(),
        ]
        .concat(),
        "unframed concatenation would collide"
    );
    assert_eq!(
        plan_identity_from_raw(&shifted),
        base.plan_identity,
        "per-launch observed perf strings are post-plan evidence"
    );
    assert_ne!(
        experiment_identity_from_raw(&shifted),
        base.experiment_identity
    );
    assert_thresholds_rejected("boundary-shift identity collision", &shifted);

    let mut post_launch_rehash = base.clone();
    post_launch_rehash.causal_process_dossiers[0].result_content_identity = raw_identity(
        "wu0g-result-content-v1",
        b"coherent different result bytes".to_vec(),
    );
    assert_eq!(
        plan_identity_from_raw(&post_launch_rehash),
        base.plan_identity
    );
    assert_ne!(
        experiment_identity_from_raw(&post_launch_rehash),
        base.experiment_identity
    );

    let mut cross_request_replay = base.clone();
    cross_request_replay.performance_pairs[0].launches[0].request_content_identity = base
        .causal_process_dossiers[0]
        .request_content_identity
        .clone();
    cross_request_replay.performance_pairs[0].launches[0].result_bound_request_content_identity =
        cross_request_replay.performance_pairs[0].launches[0]
            .request_content_identity
            .clone();
    bind_plan_and_experiment_identities(&mut cross_request_replay);
    assert_thresholds_rejected(
        "coherent cross-family request replay",
        &cross_request_replay,
    );

    let mut cross_sentinel_replay = base.clone();
    cross_sentinel_replay.performance_pairs[0].launches[0].completion_sentinel_identity = base
        .causal_process_dossiers[0]
        .completion_sentinel_identity
        .clone();
    bind_plan_and_experiment_identities(&mut cross_sentinel_replay);
    assert_thresholds_rejected(
        "coherent cross-family completion sentinel replay",
        &cross_sentinel_replay,
    );

    let mut cross_semantic_artifact_replay = base.clone();
    cross_semantic_artifact_replay.performance_pairs[0].launches[0].child_artifact_identity = base
        .causal_process_dossiers[0]
        .child_artifact_identity
        .clone();
    bind_plan_and_experiment_identities(&mut cross_semantic_artifact_replay);
    assert_thresholds_rejected(
        "coherent cross-family semantic artifact replay",
        &cross_semantic_artifact_replay,
    );

    let mut cross_perf_artifact_replay = base.clone();
    cross_perf_artifact_replay.performance_pairs[1].launches[0].perf_artifact_identity =
        base.performance_pairs[0].launches[0]
            .perf_artifact_identity
            .clone();
    bind_plan_and_experiment_identities(&mut cross_perf_artifact_replay);
    assert_thresholds_rejected(
        "coherent cross-pair perf artifact replay",
        &cross_perf_artifact_replay,
    );
}

#[test]
fn every_raw_identity_location_rejects_domain_bytes_and_claim_mutations() {
    let base = passing_raw_threshold_evidence();
    let mut probe = base.clone();
    let mut paths = Vec::new();
    visit_raw_identity_locations_mut(&mut probe, &mut |path, _| paths.push(path));
    assert_eq!(paths.iter().collect::<BTreeSet<_>>().len(), paths.len());
    assert!(!paths.is_empty());
    for path in paths {
        for axis in ["domain", "bytes", "claim"] {
            let mut mutation = base.clone();
            let mut found = false;
            visit_raw_identity_locations_mut(&mut mutation, &mut |candidate, identity| {
                if candidate != path {
                    return;
                }
                assert!(!found, "identity path is unique");
                found = true;
                match axis {
                    "domain" => identity.domain.push_str("-wrong"),
                    "bytes" => identity.canonical_bytes.push(b'!'),
                    "claim" => {
                        identity.claimed_sha256 = if identity.claimed_sha256 == "a".repeat(64) {
                            "b".repeat(64)
                        } else {
                            "a".repeat(64)
                        }
                    }
                    _ => unreachable!(),
                }
            });
            assert!(found, "visited {path}");
            assert_ne!(mutation, base, "{path} {axis} mutation is non-noop");
            assert_thresholds_rejected(&format!("{path} {axis}"), &mutation);
        }
    }
}

#[test]
fn implementation_owned_state_and_disposition_inventories_are_exact() {
    assert_eq!(
        InterfaceFillExternalTerminalKind::ALL,
        [
            InterfaceFillExternalTerminalKind::AliasTransparent,
            InterfaceFillExternalTerminalKind::ResolvedAbsorbed,
            InterfaceFillExternalTerminalKind::UnavailableAbsorbed,
            InterfaceFillExternalTerminalKind::ClassTerminal,
            InterfaceFillExternalTerminalKind::InterfaceComponent,
            InterfaceFillExternalTerminalKind::OutOfRange,
        ]
    );
    assert_eq!(
        InterfaceFillExternalConstructionState::ALL,
        [
            InterfaceFillExternalConstructionState::Pending,
            InterfaceFillExternalConstructionState::Building,
            InterfaceFillExternalConstructionState::FrozenInstalled,
            InterfaceFillExternalConstructionState::FrozenUnavailable,
            InterfaceFillExternalConstructionState::Poisoned,
            InterfaceFillExternalConstructionState::OutOfRange,
        ]
    );
    assert_eq!(
        InterfaceHeritageEdgeDisposition::ALL,
        [
            InterfaceHeritageEdgeDisposition::CompleteRequired,
            InterfaceHeritageEdgeDisposition::OpaqueOrderingOnly,
        ]
    );
    assert_eq!(
        InterfaceFillRemainingState::ALL,
        [
            InterfaceFillRemainingState::Pending,
            InterfaceFillRemainingState::Deferred,
            InterfaceFillRemainingState::NoProgress,
        ]
    );
    assert_eq!(
        ManifestClosureKind::ALL,
        [
            ManifestClosureKind::Es5,
            ManifestClosureKind::Es2015,
            ManifestClosureKind::Dom,
            ManifestClosureKind::Es2025NoHost,
            ManifestClosureKind::Full,
        ]
    );
}

#[test]
fn authorization_evidence_constructors_and_fields_are_private() {
    let sidecar = include_str!("wu0g_interface_fill_attribution.rs");
    for owner in [
        "struct ValidatedLadderCounterPoint",
        "struct ValidatedLadderCounterRow",
        "struct ValidatedFreshProcessDossier",
        "struct ValidatedCycleProfileDossier",
        "struct ValidatedPredictionDossier",
        "struct ValidatedControlPairDossier",
        "struct ValidatedPerformancePairDossier",
        "struct ValidatedInterfaceFillAuthorizationEvidence",
    ] {
        let body = source_item_body(sidecar, owner);
        assert!(
            body.lines().all(|line| {
                let line = line.trim_start();
                !line.starts_with("pub ")
                    && !line.starts_with("pub(")
                    && !line.starts_with("pub(crate)")
                    && !line.starts_with("pub(super)")
            }),
            "{owner} fields stay private"
        );
    }
    for forbidden in [
        "build_passing_interface_fill_selection_for_test",
        "mutate_interface_fill_selection_for_test",
        "InterfaceFillSelectionMutation",
        "InterfaceFillInvalidIdentityForm",
    ] {
        assert!(
            !sidecar.contains(forbidden),
            "no implementation-owned oracle {forbidden}"
        );
    }
}

fn replace_protocol_field(bytes: &[u8], key: &str, value: &str) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).expect("protocol fixture is UTF-8");
    let prefix = format!("{key}=");
    let mut replaced = false;
    let mut output = String::new();
    for line in text.lines() {
        if line.starts_with(&prefix) {
            assert!(!replaced, "protocol key is unique");
            replaced = true;
            output.push_str(&format!("{key}={}:{}\n", value.len(), value));
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    assert!(replaced, "protocol key exists");
    output.into_bytes()
}

#[test]
fn child_request_result_and_perf_parsers_fail_closed_on_raw_bytes() {
    let (request, result) = spec_owned_protocol_exchange();
    let causal_request = spec_owned_request_bytes("causal");
    assert!(!String::from_utf8_lossy(&request).contains("experiment_identity"));
    assert!(!String::from_utf8_lossy(&request).contains("request_content_identity"));
    assert!(parse_wu0g_child_request_for_test(&request).is_ok());
    assert!(parse_wu0g_child_request_for_test(&causal_request).is_ok());
    assert!(parse_wu0g_child_result_for_test(&request, &result).is_ok());

    for (label, mutation) in [
        ("missing", {
            let mut lines = request.split(|byte| *byte == b'\n').collect::<Vec<_>>();
            lines.remove(2);
            lines.join(&b'\n')
        }),
        ("duplicate", {
            let mut lines = request.split(|byte| *byte == b'\n').collect::<Vec<_>>();
            lines.insert(2, lines[1]);
            lines.join(&b'\n')
        }),
        ("unknown", {
            let mut lines = request.split(|byte| *byte == b'\n').collect::<Vec<_>>();
            lines.insert(2, b"unknown=1:x");
            lines.join(&b'\n')
        }),
        ("reordered", {
            let mut lines = request.split(|byte| *byte == b'\n').collect::<Vec<_>>();
            lines.swap(1, 2);
            lines.join(&b'\n')
        }),
    ] {
        assert!(
            parse_wu0g_child_request_for_test(&mutation).is_err(),
            "request parser accepts {label} field mutation"
        );
    }

    let mut request_mutations = vec![
        (
            "invalid kind",
            replace_protocol_field(&request, "kind", "validator"),
        ),
        (
            "invalid causal mode",
            replace_protocol_field(&causal_request, "mode", "plain"),
        ),
        (
            "invalid performance mode",
            replace_protocol_field(&request, "mode", "measured-off"),
        ),
        (
            "causal rung ordinal absent",
            replace_protocol_field(&causal_request, "rung_ordinal", "none"),
        ),
        (
            "causal rung ordinal out of range",
            replace_protocol_field(&causal_request, "rung_ordinal", "5"),
        ),
        (
            "performance pair ordinal absent",
            replace_protocol_field(&request, "pair_ordinal", "none"),
        ),
        (
            "performance pair ordinal out of range",
            replace_protocol_field(&request, "pair_ordinal", "5"),
        ),
        (
            "performance launch ordinal absent",
            replace_protocol_field(&request, "launch_ordinal", "none"),
        ),
        (
            "performance launch ordinal out of range",
            replace_protocol_field(&request, "launch_ordinal", "10"),
        ),
        (
            "RSS exceeds memory below both maxima",
            replace_protocol_field(
                &replace_protocol_field(&request, "memory_limit_bytes", "268435456"),
                "rss_limit_bytes",
                "314572800",
            ),
        ),
        (
            "nofile soft exceeds hard",
            replace_protocol_field(&request, "nofile_hard", "128"),
        ),
    ];
    for (key, value) in [
        ("launch_identity", "f".repeat(64)),
        ("launch_ordinal", "0".to_owned()),
        ("pair_identity", "f".repeat(64)),
        ("pair_ordinal", "0".to_owned()),
        ("perf_event", WU0G_PERF_EVENT.to_owned()),
        ("perf_identity", "f".repeat(64)),
        ("perf_version", "perf version fixture".to_owned()),
    ] {
        request_mutations.push((
            "causal field is inapplicable",
            replace_protocol_field(&causal_request, key, &value),
        ));
    }
    for (key, value) in [
        ("rung_identity", "f".repeat(64)),
        ("rung_ordinal", "0".to_owned()),
    ] {
        request_mutations.push((
            "performance field is inapplicable",
            replace_protocol_field(&request, key, &value),
        ));
    }
    for (key, source) in [
        ("binary_identity", &causal_request),
        ("candidate_identity", &causal_request),
        ("host_identity", &causal_request),
        ("plan_identity", &causal_request),
        ("prlimit_identity", &causal_request),
        ("result_identity", &causal_request),
        ("rung_identity", &causal_request),
        ("workload_identity", &causal_request),
        ("launch_identity", &request),
        ("pair_identity", &request),
        ("perf_identity", &request),
    ] {
        request_mutations.push((
            "malformed identity",
            replace_protocol_field(source, key, &"f".repeat(63)),
        ));
    }
    for (key, maximum) in [
        ("deadline_ms", WU0G_DEADLINE_MS),
        ("memory_limit_bytes", WU0G_MEMORY_LIMIT_BYTES),
        ("rss_limit_bytes", WU0G_RSS_LIMIT_BYTES),
        ("nofile_soft", WU0G_NOFILE_LIMIT),
        ("nofile_hard", WU0G_NOFILE_LIMIT),
    ] {
        request_mutations.push((
            "zero request limit",
            replace_protocol_field(&request, key, "0"),
        ));
        request_mutations.push((
            "oversized request limit",
            replace_protocol_field(
                &request,
                key,
                &maximum.checked_add(1).expect("limit increment").to_string(),
            ),
        ));
    }
    for (label, mutation) in request_mutations {
        assert!(
            parse_wu0g_child_request_for_test(&mutation).is_err(),
            "request parser accepts {label}"
        );
    }

    for (label, key, value) in [
        ("forged plan", "plan_identity", "f".repeat(64)),
        (
            "forged request binding",
            "request_content_identity",
            "e".repeat(64),
        ),
        ("forged result", "result_identity", "d".repeat(64)),
        ("forged tool", "perf_identity", "c".repeat(64)),
        ("forged artifact", "artifact_identity", "b".repeat(64)),
        ("malformed size", "artifact_size", "-1".to_owned()),
        ("malformed status", "termination", "complete".to_owned()),
        ("zero limit", "deadline_ms", "0".to_owned()),
        ("zero leader PID", "leader_pid", "0".to_owned()),
        (
            "zero leader start ticks",
            "leader_start_ticks",
            "0".to_owned(),
        ),
        ("leader/PGID mismatch", "pgid", "1235".to_owned()),
        (
            "limit readback mismatch",
            "memory_limit_readback_bytes",
            "1".to_owned(),
        ),
    ] {
        let mutation = replace_protocol_field(&result, key, &value);
        assert!(
            parse_wu0g_child_result_for_test(&request, &mutation).is_err(),
            "result parser accepts {label}"
        );
    }
    for (label, mutation) in [
        ("missing", {
            let mut lines = result.split(|byte| *byte == b'\n').collect::<Vec<_>>();
            lines.remove(2);
            lines.join(&b'\n')
        }),
        ("duplicate", {
            let mut lines = result.split(|byte| *byte == b'\n').collect::<Vec<_>>();
            lines.insert(2, lines[1]);
            lines.join(&b'\n')
        }),
        ("unknown", {
            let mut lines = result.split(|byte| *byte == b'\n').collect::<Vec<_>>();
            lines.insert(2, b"unknown=1:x");
            lines.join(&b'\n')
        }),
        ("reordered", {
            let mut lines = result.split(|byte| *byte == b'\n').collect::<Vec<_>>();
            lines.swap(1, 2);
            lines.join(&b'\n')
        }),
    ] {
        assert!(
            parse_wu0g_child_result_for_test(&request, &mutation).is_err(),
            "result parser accepts {label} field mutation"
        );
    }

    let valid_perf = b"12345;;instructions:u;67890;100.00;;\n";
    assert_eq!(valid_perf.iter().filter(|byte| **byte == b';').count(), 6);
    assert_eq!(
        parse_wu0g_perf_artifact_for_test(valid_perf),
        Ok((12_345, 67_890))
    );
    for invalid in [
        b"<not-supported>;;instructions:u;67890;100.00;;\n".as_slice(),
        b"<not-counted>;;instructions:u;67890;100.00;;\n",
        b"+12345;;instructions:u;67890;100.00;;\n",
        b"12345;;instructions:u;+67890;100.00;;\n",
        b"012345;;instructions:u;67890;100.00;;\n",
        b"12345;;instructions:u;067890;100.00;;\n",
        b"0;;instructions:u;67890;100.00;;\n",
        b"12345;;instructions:u;0;100.00;;\n",
        b"18446744073709551616;;instructions:u;67890;100.00;;\n",
        b"12345;;instructions:u;18446744073709551616;100.00;;\n",
        b"12345;;cycles:u;67890;100.00;;\n",
        b"12345;;instructions:u;67890;99.99;;\n",
        b"12345;;instructions:u;67890;100.00;\n",
        b"12345;;instructions:u;67890;100.00;;;\n",
        b"12345;;instructions:u;67890;100.00;;\r\n",
        b"12345;;instructions:u;67890;100.00;;",
        b"12345;;instructions:u;67890;100.00;;\n# comment\n",
        b"12345;;instructions:u;67890;100.00;;\n12345;;instructions:u;67890;100.00;;\n",
    ] {
        assert!(parse_wu0g_perf_artifact_for_test(invalid).is_err());
    }
    let exactly_oversized = vec![b'0'; 4_097];
    assert_eq!(exactly_oversized.len(), 4_097);
    assert!(parse_wu0g_perf_artifact_for_test(&exactly_oversized).is_err());
}

#[test]
#[ignore = "invoked exactly once by the authenticated WU0G child runner"]
fn wu0g_hardened_child_once() {
    run_wu0g_hardened_child_once_for_test()
        .expect("authenticated child writes one canonical sentinel and artifact");
}

#[test]
#[ignore = "WU0G pinned authorization gate; bounded causal and interleaved fresh child processes"]
fn only_completed_authenticated_pinned_dom_evidence_can_authorize() {
    let profile = load_strict_profile().expect("pinned TypeScript profile");
    let ladder = build_dom_heritage_prefix_ladder_for_test(&profile).expect("pinned DOM ladder");
    assert_eq!(
        ladder
            .iter()
            .map(|rung| rung.target_basis_points)
            .collect::<Vec<_>>(),
        [1_250, 2_500, 5_000, 7_500, 10_000]
    );
    let expected_rungs = ladder
        .iter()
        .map(|rung| rung.identity.clone())
        .collect::<Vec<_>>();
    let raw = run_pinned_dom_authorization_probe_for_test(
        &profile,
        std::time::Duration::from_secs(30),
        512 * 1024 * 1024,
    )
    .expect("hardened pinned coordinator returns raw dossiers");
    assert_eq!(raw.evidence_domain, RawEvidenceDomain::PinnedDom82);
    assert_exact_causal_process_matrix(&raw.thresholds, &expected_rungs);
    assert_exact_performance_schedule(&raw.thresholds);
    let all_complete = raw
        .thresholds
        .causal_process_dossiers
        .iter()
        .all(|dossier| {
            dossier.termination == RawProbeTermination::Complete
                && dossier.containment_failures == 0
                && dossier.cgroup_oom_delta == 0
                && dossier.cgroup_oom_kill_delta == 0
                && dossier.exit_code == Some(0)
                && dossier.term_signal.is_none()
                && dossier.waited
                && dossier.reaped
                && dossier.cleanup_succeeded
                && dossier.max_rss_bytes <= dossier.memory_limit_bytes
        })
        && raw.thresholds.performance_pairs.iter().all(|pair| {
            pair.launches.iter().all(|launch| {
                launch.termination == RawProbeTermination::Complete
                    && launch.containment_failures == 0
                    && launch.cgroup_oom_delta == 0
                    && launch.cgroup_oom_kill_delta == 0
                    && launch.exit_code == Some(0)
                    && launch.term_signal.is_none()
                    && launch.waited
                    && launch.reaped
                    && launch.cleanup_succeeded
                    && launch.max_rss_bytes <= launch.memory_limit_bytes
            })
        });
    assert!(
        all_complete,
        "the executable authorization gate cannot pass on containment failures"
    );
    let decision = validate_interface_fill_authorization_from_raw_for_test(&raw);
    assert!(decision.authorizes());
}

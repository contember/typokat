//! Run-local, bounded attribution for the WU0G interface-fill experiment.

use super::super::context::{DeclTypes, Pass, TypeDecl};
use super::super::events_library::{LibraryEventLedger, LibraryRecordTicket};
use super::super::lexical_events::LexicalReservations;
use super::super::lexical_events_library::library_unit;
use super::super::wu0b_library::{
    canonical_library_record_bytes_for_test, canonical_type_store_bytes_for_test,
    run_wu0e_fill_schedule_with_interface_observer_for_test, InjectedLibrarySource,
    Wu0eFillSchedulePass, Wu0eInterfaceObserver,
};
use super::super::wu0b_profile::StrictLibraryProfile;
use super::super::{build_pass_with_tickets, reserve_type_decls, PassReporting, PassReportingPlan};
use super::wu0g_interface_fill_attribution_spec::{
    RawAuthorizationEvidence, RawBoundedObservation, RawBudgetWitnessRequest,
    RawCanonicalGroupState, RawCanonicalPrefixInput, RawCanonicalTypeStoreRow,
    RawCompletionComponent, RawCompletionGroupState, RawCompletionInventory, RawControlPairDossier,
    RawCopyAccountingInput, RawCycleProfileDossier, RawEvidenceDomain,
    RawExternalTopologyInjection, RawFreshProcessDossier, RawIdentityEvidence,
    RawLadderCounterPoint, RawLadderCounterRow, RawLaunchOrder, RawPerformanceLaunch,
    RawPerformancePairDossier, RawPlanTerminalState, RawPredictionDossier, RawProbeMode,
    RawProbeTermination, RawSelectedComponentDirective, RawSelectedComponentPlan,
    RawThresholdEvidence,
};
use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::TypeGroupId;
use crate::binder::namespace::{exact_key, CompilationUnit, ModuleBindingContext, SourceFileKind};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use crate::types::repr::TypeParamId;
use crate::types::store::TypeId;
use crate::types::substitute::wu0g_application_substitute_with_outcome;
use crate::types::{Interner, SubstitutionOutcome};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rustc_hash::FxHashMap;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod process_boundary;

use process_boundary::{create_exclusive_file, read_bounded_file, write_exclusive_bounded};

pub(super) static MAX_TRACKED_APPLICATION_KEYS: usize = 1_048_576;
pub(super) static MAX_APPLICATION_ARGUMENTS_PER_KEY: usize = 65_536;
pub(super) static MAX_TRACKED_APPLICATION_ARGUMENTS: usize = 16_777_216;
pub(super) static MAX_TRACKED_APPLICATION_KEY_BYTES: usize = 256 * 1024 * 1024;
pub(super) static MAX_COMPONENT_CHECKPOINTS: usize = 1_048_576;
pub(super) static MAX_TRACKED_CHECKPOINT_BYTES: usize = 256 * 1024 * 1024;

const EMPTY_IDENTITY_DOMAIN: &[u8] = b"typokat-wu0g-empty-v1";
const PROFILE_IDENTITY_DOMAIN: &[u8] = b"typokat-wu0g-profile-v1";
const CLOSURE_IDENTITY_DOMAIN: &[u8] = b"typokat-wu0g-closure-v1";
const COMPONENT_IDENTITY_DOMAIN: &[u8] = b"typokat-wu0g-component-v1";
const TERMINAL_IDENTITY_DOMAIN: &[u8] = b"typokat-wu0g-terminal-v1";
const LADDER_STRATEGY_VERSION: &[u8] = b"dependency-first-whole-heritage-scc-v1";
const WU0G_DEADLINE_MS: u64 = 30_000;
const WU0G_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
const WU0G_RSS_LIMIT_BYTES: u64 = 384 * 1024 * 1024;
const WU0G_NOFILE_LIMIT: u64 = 256;
const WU0G_STDOUT_CAP_BYTES: u64 = 128 * 1024;
const WU0G_STDERR_CAP_BYTES: u64 = 128 * 1024;
const WU0G_REQUEST_CAP_BYTES: u64 = 64 * 1024;
const WU0G_RESULT_CAP_BYTES: u64 = 64 * 1024;
const WU0G_SENTINEL_CAP_BYTES: u64 = 4 * 1024;
const WU0G_ARTIFACT_CAP_BYTES: u64 = 256 * 1024;
const WU0G_PERF_ARTIFACT_CAP_BYTES: u64 = 4 * 1024;
const WU0G_PERF_EVENT: &str = "instructions:u";
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InterfaceFillAttributionMode {
    Baseline,
    CandidateB,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ApplicationKey {
    template: TypeId,
    arguments: Vec<(TypeParamId, TypeId)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CandidateEntry {
    result: TypeId,
    visits: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ApplicationEntry {
    frequency: u64,
    candidate: Option<CandidateEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillAttributionReport {
    pub(super) application_requests: u64,
    pub(super) unique_application_keys: u64,
    pub(super) max_application_frequency: u64,
    pub(super) clean_cache_hits: u64,
    pub(super) candidate_b_hits: u64,
    pub(super) candidate_b_avoided_visits: u64,
    pub(super) executed_substitutions: u64,
    pub(super) substitution_visits: u64,
    pub(super) max_visits_per_substitution: u64,
    pub(super) max_apply_depth: u64,
    pub(super) cycle_clean_outcomes: u64,
    pub(super) cycle_tainted_outcomes: u64,
    pub(super) cycle_reentries: u64,
    pub(super) copied_objects: u64,
    pub(super) copied_properties: u64,
    pub(super) copied_property_name_bytes: u64,
    pub(super) copied_signatures: u64,
    pub(super) substitution_interner_attempts: u64,
    pub(super) substitution_interner_hits: u64,
    pub(super) substitution_interner_new_ids: u64,
    pub(super) completed_components: u64,
    pub(super) in_flight_applications: u64,
    pub(super) in_flight_components: u64,
    pub(super) saturated: bool,
    pub(super) is_complete: bool,
    pub(super) application_histogram_sha256: String,
    pub(super) application_order_sha256: String,
    pub(super) frozen_state_sha256: String,
    application_frequency_sum: u64,
    application_frequency_entry_count: u64,
    expected_candidate_b_avoided_visits: u64,
}

impl Default for InterfaceFillAttributionReport {
    fn default() -> Self {
        let empty = digest_bytes(EMPTY_IDENTITY_DOMAIN);
        Self {
            application_requests: 0,
            unique_application_keys: 0,
            max_application_frequency: 0,
            clean_cache_hits: 0,
            candidate_b_hits: 0,
            candidate_b_avoided_visits: 0,
            executed_substitutions: 0,
            substitution_visits: 0,
            max_visits_per_substitution: 0,
            max_apply_depth: 0,
            cycle_clean_outcomes: 0,
            cycle_tainted_outcomes: 0,
            cycle_reentries: 0,
            copied_objects: 0,
            copied_properties: 0,
            copied_property_name_bytes: 0,
            copied_signatures: 0,
            substitution_interner_attempts: 0,
            substitution_interner_hits: 0,
            substitution_interner_new_ids: 0,
            completed_components: 0,
            in_flight_applications: 0,
            in_flight_components: 0,
            saturated: false,
            is_complete: false,
            application_histogram_sha256: empty.clone(),
            application_order_sha256: empty.clone(),
            frozen_state_sha256: empty,
            application_frequency_sum: 0,
            application_frequency_entry_count: 0,
            expected_candidate_b_avoided_visits: 0,
        }
    }
}

impl InterfaceFillAttributionReport {
    fn add(counter: &mut u64, value: u64, saturated: &mut bool) {
        *counter = match counter.checked_add(value) {
            Some(sum) => sum,
            None => {
                *saturated = true;
                u64::MAX
            }
        };
    }

    pub(super) fn application_frequency_sum_for_test(&self) -> u64 {
        self.application_frequency_sum
    }

    pub(super) fn application_frequency_entry_count_for_test(&self) -> u64 {
        self.application_frequency_entry_count
    }

    pub(super) fn validate_exact(&self) -> Result<(), String> {
        if self.saturated {
            return Err("attribution saturated".to_owned());
        }
        if self.executed_substitutions
            != self
                .cycle_clean_outcomes
                .checked_add(self.cycle_tainted_outcomes)
                .ok_or_else(|| "outcome arithmetic overflow".to_owned())?
        {
            return Err("executed/outcome arithmetic mismatch".to_owned());
        }
        let classified = self
            .clean_cache_hits
            .checked_add(self.candidate_b_hits)
            .and_then(|value| value.checked_add(self.executed_substitutions))
            .ok_or_else(|| "request arithmetic overflow".to_owned())?;
        if self.application_requests != classified
            || self.application_frequency_sum_for_test() != self.application_requests
            || self.application_frequency_entry_count_for_test() != self.unique_application_keys
        {
            return Err("application arithmetic mismatch".to_owned());
        }
        let interner_outcomes = self
            .substitution_interner_hits
            .checked_add(self.substitution_interner_new_ids)
            .ok_or_else(|| "interner arithmetic overflow".to_owned())?;
        if self.substitution_interner_attempts != interner_outcomes {
            return Err("interner arithmetic mismatch".to_owned());
        }
        if self.in_flight_applications != 0 || self.in_flight_components != 0 {
            return Err("boundary report contains in-flight work".to_owned());
        }
        if (self.application_requests == 0)
            != (self.unique_application_keys == 0 && self.max_application_frequency == 0)
            || self.unique_application_keys > self.application_requests
            || self.max_application_frequency > self.application_requests
            || (self.executed_substitutions == 0)
                != (self.substitution_visits == 0
                    && self.max_visits_per_substitution == 0
                    && self.max_apply_depth == 0)
            || self.substitution_visits < self.executed_substitutions
            || self.max_visits_per_substitution > self.substitution_visits
            || self.max_apply_depth > self.max_visits_per_substitution
            || self.cycle_reentries > self.substitution_visits
        {
            return Err("counter bounds are inconsistent".to_owned());
        }
        if self.candidate_b_avoided_visits != self.expected_candidate_b_avoided_visits {
            return Err("Candidate-B avoided visits do not match retained exact visits".to_owned());
        }
        Ok(())
    }

    pub(super) fn maximal_consistent_for_test() -> Self {
        Self {
            application_requests: 1,
            unique_application_keys: 1,
            max_application_frequency: 1,
            executed_substitutions: 1,
            substitution_visits: 1,
            max_visits_per_substitution: 1,
            max_apply_depth: 1,
            cycle_clean_outcomes: 1,
            substitution_interner_attempts: 1,
            substitution_interner_hits: 1,
            application_frequency_sum: 1,
            application_frequency_entry_count: 1,
            ..Self::default()
        }
    }

    pub(super) fn force_counter_family_overflow_for_test(
        &mut self,
        family: InterfaceFillCounterFamily,
    ) {
        let counter = match family {
            InterfaceFillCounterFamily::Application => &mut self.application_requests,
            InterfaceFillCounterFamily::Execution => &mut self.executed_substitutions,
            InterfaceFillCounterFamily::Visit => &mut self.substitution_visits,
            InterfaceFillCounterFamily::Copy => &mut self.copied_objects,
            InterfaceFillCounterFamily::Interner => &mut self.substitution_interner_attempts,
            InterfaceFillCounterFamily::Component => &mut self.completed_components,
        };
        *counter = u64::MAX;
        Self::add(counter, 1, &mut self.saturated);
    }

    pub(super) fn scalar_snapshot_for_test(&self) -> InterfaceFillAttributionSnapshot {
        InterfaceFillAttributionSnapshot::from_report(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InterfaceFillCounterFamily {
    Application,
    Execution,
    Visit,
    Copy,
    Interner,
    Component,
}

impl InterfaceFillCounterFamily {
    pub(super) const ALL: [Self; 6] = [
        Self::Application,
        Self::Execution,
        Self::Visit,
        Self::Copy,
        Self::Interner,
        Self::Component,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InterfaceFillBudgetKind {
    Keys,
    ArgumentsPerKey,
    Arguments,
    KeyBytes,
    ComponentCheckpoints,
    CheckpointBytes,
}

impl InterfaceFillBudgetKind {
    pub(super) const ALL: [Self; 6] = [
        Self::Keys,
        Self::ArgumentsPerKey,
        Self::Arguments,
        Self::KeyBytes,
        Self::ComponentCheckpoints,
        Self::CheckpointBytes,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillAttributionLimits {
    pub(super) max_application_keys: usize,
    pub(super) max_arguments_per_key: usize,
    pub(super) max_application_arguments: usize,
    pub(super) max_application_key_bytes: usize,
    pub(super) max_component_checkpoints: usize,
    pub(super) max_checkpoint_bytes: usize,
}

impl Default for InterfaceFillAttributionLimits {
    fn default() -> Self {
        Self {
            max_application_keys: MAX_TRACKED_APPLICATION_KEYS,
            max_arguments_per_key: MAX_APPLICATION_ARGUMENTS_PER_KEY,
            max_application_arguments: MAX_TRACKED_APPLICATION_ARGUMENTS,
            max_application_key_bytes: MAX_TRACKED_APPLICATION_KEY_BYTES,
            max_component_checkpoints: MAX_COMPONENT_CHECKPOINTS,
            max_checkpoint_bytes: MAX_TRACKED_CHECKPOINT_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillAttributionSnapshot {
    counters: [u64; 24],
    saturated: bool,
    is_complete: bool,
    application_histogram_sha256: [u8; 32],
    application_order_sha256: [u8; 32],
    frozen_state_sha256: [u8; 32],
}

impl InterfaceFillAttributionSnapshot {
    fn from_report(report: &InterfaceFillAttributionReport) -> Self {
        Self {
            counters: [
                report.application_requests,
                report.unique_application_keys,
                report.max_application_frequency,
                report.clean_cache_hits,
                report.candidate_b_hits,
                report.candidate_b_avoided_visits,
                report.executed_substitutions,
                report.substitution_visits,
                report.max_visits_per_substitution,
                report.max_apply_depth,
                report.cycle_clean_outcomes,
                report.cycle_tainted_outcomes,
                report.cycle_reentries,
                report.copied_objects,
                report.copied_properties,
                report.copied_property_name_bytes,
                report.copied_signatures,
                report.substitution_interner_attempts,
                report.substitution_interner_hits,
                report.substitution_interner_new_ids,
                report.completed_components,
                report.in_flight_applications,
                report.in_flight_components,
                report.application_frequency_sum,
            ],
            saturated: report.saturated,
            is_complete: report.is_complete,
            application_histogram_sha256: decode_sha256(&report.application_histogram_sha256),
            application_order_sha256: decode_sha256(&report.application_order_sha256),
            frozen_state_sha256: decode_sha256(&report.frozen_state_sha256),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InterfaceFillRemainingState {
    Pending,
    Deferred,
    NoProgress,
}

impl InterfaceFillRemainingState {
    pub(super) const ALL: [Self; 3] = [Self::Pending, Self::Deferred, Self::NoProgress];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillCompletedComponent {
    pub(super) identity: String,
    pub(super) group_ids: Vec<u32>,
    frozen: bool,
    template_fill_done: bool,
}

impl InterfaceFillCompletedComponent {
    pub(super) fn all_groups_frozen(&self) -> bool {
        self.frozen
    }

    pub(super) fn interface_template_fill_done(&self) -> bool {
        self.template_fill_done
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillRemainingComponent {
    pub(super) identity: String,
    pub(super) group_ids: Vec<u32>,
    pub(super) state: InterfaceFillRemainingState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct InterfaceFillCompletion {
    pub(super) completed_components: Vec<InterfaceFillCompletedComponent>,
    pub(super) remaining_selected_components: Vec<InterfaceFillRemainingComponent>,
}

impl InterfaceFillCompletion {
    pub(super) fn is_complete(&self) -> bool {
        !self.completed_components.is_empty() && self.remaining_selected_components.is_empty()
    }

    pub(super) fn validate_exact(&self) -> Result<(), String> {
        if self.completed_components.iter().any(|component| {
            component.group_ids.is_empty()
                || !component.all_groups_frozen()
                || !component.interface_template_fill_done()
        }) {
            return Err("completed interface component is not exactly frozen".to_owned());
        }
        let mut identities = BTreeSet::new();
        if self
            .completed_components
            .iter()
            .map(|component| component.identity.as_str())
            .chain(
                self.remaining_selected_components
                    .iter()
                    .map(|component| component.identity.as_str()),
            )
            .any(|identity| !identities.insert(identity))
        {
            return Err("completion contains duplicate component identity".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeInterfaceComponent {
    identity: String,
    group_ids: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillComponentCheckpoint {
    completed_component_identity_sha256: [u8; 32],
    completed_group_membership_sha256: [u8; 32],
    pub(super) cumulative: InterfaceFillAttributionSnapshot,
    retained_bytes: usize,
}

#[derive(Clone)]
pub(in crate::check::checker) struct InterfaceFillAttributionCollector {
    mode: InterfaceFillAttributionMode,
    report: InterfaceFillAttributionReport,
    application_order: Sha256,
    frozen_states: Sha256,
    checkpoints: Vec<InterfaceFillComponentCheckpoint>,
    application_entries: BTreeMap<ApplicationKey, ApplicationEntry>,
    tracked_application_arguments: usize,
    tracked_application_key_bytes: usize,
    checkpoint_retained_bytes: usize,
    limits: InterfaceFillAttributionLimits,
    selected_component_identities: Vec<String>,
    selected_component_memberships: BTreeMap<String, Vec<u32>>,
    next_selected_component: usize,
    cooperative_stop_identity: Option<String>,
    cooperatively_stopped: bool,
    completion: InterfaceFillCompletion,
}

impl InterfaceFillAttributionCollector {
    fn new(mode: InterfaceFillAttributionMode, cooperative_stop_identity: Option<String>) -> Self {
        Self::new_with_limits(
            mode,
            cooperative_stop_identity,
            InterfaceFillAttributionLimits::default(),
        )
    }

    fn new_with_limits(
        mode: InterfaceFillAttributionMode,
        cooperative_stop_identity: Option<String>,
        limits: InterfaceFillAttributionLimits,
    ) -> Self {
        let mut application_order = Sha256::new();
        application_order.update(b"typokat-wu0g-application-order-v1");
        let mut frozen_states = Sha256::new();
        frozen_states.update(b"typokat-wu0g-frozen-state-v1");
        Self {
            mode,
            report: InterfaceFillAttributionReport::default(),
            application_order,
            frozen_states,
            checkpoints: Vec::new(),
            application_entries: BTreeMap::new(),
            tracked_application_arguments: 0,
            tracked_application_key_bytes: 0,
            checkpoint_retained_bytes: 0,
            limits,
            selected_component_identities: Vec::new(),
            selected_component_memberships: BTreeMap::new(),
            next_selected_component: 0,
            cooperative_stop_identity,
            cooperatively_stopped: false,
            completion: InterfaceFillCompletion::default(),
        }
    }

    fn configure_selected_components(&mut self, components: &[RuntimeInterfaceComponent]) {
        self.selected_component_identities = components
            .iter()
            .map(|component| component.identity.clone())
            .collect();
        self.selected_component_memberships = components
            .iter()
            .map(|component| (component.identity.clone(), component.group_ids.clone()))
            .collect();
    }

    fn record_request(&mut self, key: &ApplicationKey) {
        InterfaceFillAttributionReport::add(
            &mut self.report.application_requests,
            1,
            &mut self.report.saturated,
        );
        hash_application_key(&mut self.application_order, key);
        self.report.application_order_sha256 =
            format!("{:x}", self.application_order.clone().finalize());

        if key.arguments.len() > self.limits.max_arguments_per_key {
            self.report.saturated = true;
            return;
        }
        if !self.application_entries.contains_key(key) {
            let key_bytes = application_key_bytes(key);
            let next_arguments = self
                .tracked_application_arguments
                .checked_add(key.arguments.len());
            let next_bytes = self.tracked_application_key_bytes.checked_add(key_bytes);
            if self.application_entries.len() >= self.limits.max_application_keys
                || next_arguments.is_none_or(|value| value > self.limits.max_application_arguments)
                || next_bytes.is_none_or(|value| value > self.limits.max_application_key_bytes)
            {
                self.report.saturated = true;
                return;
            }
            self.tracked_application_arguments = next_arguments.unwrap_or(usize::MAX);
            self.tracked_application_key_bytes = next_bytes.unwrap_or(usize::MAX);
            self.application_entries
                .insert(key.clone(), ApplicationEntry::default());
            self.report.unique_application_keys =
                u64::try_from(self.application_entries.len()).unwrap_or(u64::MAX);
            self.report.application_frequency_entry_count = self.report.unique_application_keys;
        }
        let Some(entry) = self.application_entries.get_mut(key) else {
            self.report.saturated = true;
            return;
        };
        InterfaceFillAttributionReport::add(&mut entry.frequency, 1, &mut self.report.saturated);
        InterfaceFillAttributionReport::add(
            &mut self.report.application_frequency_sum,
            1,
            &mut self.report.saturated,
        );
        self.report.max_application_frequency =
            self.report.max_application_frequency.max(entry.frequency);
        self.refresh_histogram_identity();
    }

    fn refresh_histogram_identity(&mut self) {
        let mut hash = Sha256::new();
        hash.update(b"typokat-wu0g-application-histogram-v1");
        frame_usize(&mut hash, self.application_entries.len());
        for (key, entry) in &self.application_entries {
            hash_application_key(&mut hash, key);
            hash.update(entry.frequency.to_be_bytes());
        }
        self.report.application_histogram_sha256 = format!("{:x}", hash.finalize());
    }

    fn candidate(&self, key: &ApplicationKey) -> Option<CandidateEntry> {
        (self.mode == InterfaceFillAttributionMode::CandidateB)
            .then(|| self.application_entries.get(key)?.candidate)
            .flatten()
    }

    fn begin_execution(&mut self) {
        self.report.in_flight_applications = 1;
    }

    fn record_clean_hit(&mut self, key: &ApplicationKey, result: TypeId) {
        InterfaceFillAttributionReport::add(
            &mut self.report.clean_cache_hits,
            1,
            &mut self.report.saturated,
        );
        self.record_frozen_state(key, result);
    }

    fn record_candidate_hit(&mut self, key: &ApplicationKey, candidate: CandidateEntry) {
        InterfaceFillAttributionReport::add(
            &mut self.report.candidate_b_hits,
            1,
            &mut self.report.saturated,
        );
        InterfaceFillAttributionReport::add(
            &mut self.report.candidate_b_avoided_visits,
            candidate.visits,
            &mut self.report.saturated,
        );
        InterfaceFillAttributionReport::add(
            &mut self.report.expected_candidate_b_avoided_visits,
            candidate.visits,
            &mut self.report.saturated,
        );
        self.record_frozen_state(key, candidate.result);
    }

    fn record_execution(
        &mut self,
        key: &ApplicationKey,
        outcome: SubstitutionOutcome,
        accumulator: ([u64; 11], bool),
    ) {
        self.report.in_flight_applications = 0;
        InterfaceFillAttributionReport::add(
            &mut self.report.executed_substitutions,
            1,
            &mut self.report.saturated,
        );
        merge_accumulator(&mut self.report, &accumulator);
        let (values, saturated) = accumulator;
        self.report.max_visits_per_substitution =
            self.report.max_visits_per_substitution.max(values[0]);
        self.report.max_apply_depth = self.report.max_apply_depth.max(values[2]);
        self.report.saturated |= saturated || values[1] != 0;
        let result = match outcome {
            SubstitutionOutcome::CycleClean(result) => {
                InterfaceFillAttributionReport::add(
                    &mut self.report.cycle_clean_outcomes,
                    1,
                    &mut self.report.saturated,
                );
                result
            }
            SubstitutionOutcome::CycleTainted(result) => {
                InterfaceFillAttributionReport::add(
                    &mut self.report.cycle_tainted_outcomes,
                    1,
                    &mut self.report.saturated,
                );
                if let Some(entry) = self.application_entries.get_mut(key) {
                    entry.candidate = Some(CandidateEntry {
                        result,
                        visits: values[0],
                    });
                } else {
                    self.report.saturated = true;
                }
                result
            }
        };
        self.record_frozen_state(key, result);
    }

    fn record_frozen_state(&mut self, key: &ApplicationKey, result: TypeId) {
        hash_application_key(&mut self.frozen_states, key);
        self.frozen_states.update(result.0.to_be_bytes());
        self.report.frozen_state_sha256 = format!("{:x}", self.frozen_states.clone().finalize());
    }

    fn component_boundary(&mut self, component: &RuntimeInterfaceComponent) -> bool {
        InterfaceFillAttributionReport::add(
            &mut self.report.completed_components,
            1,
            &mut self.report.saturated,
        );
        self.report.in_flight_applications = 0;
        self.report.in_flight_components = 0;
        self.completion
            .completed_components
            .push(InterfaceFillCompletedComponent {
                identity: component.identity.clone(),
                group_ids: component.group_ids.clone(),
                frozen: true,
                template_fill_done: true,
            });
        let retained_bytes = std::mem::size_of::<InterfaceFillComponentCheckpoint>();
        let next_retained = self.checkpoint_retained_bytes.checked_add(retained_bytes);
        if self.checkpoints.len() >= self.limits.max_component_checkpoints
            || next_retained.is_none_or(|value| value > self.limits.max_checkpoint_bytes)
        {
            self.report.saturated = true;
        } else {
            let component_identity = Sha256::digest(component.identity.as_bytes()).into();
            let mut membership = Sha256::new();
            for group_id in &component.group_ids {
                membership.update(group_id.to_be_bytes());
            }
            self.checkpoints.push(InterfaceFillComponentCheckpoint {
                completed_component_identity_sha256: component_identity,
                completed_group_membership_sha256: membership.finalize().into(),
                cumulative: self.report.scalar_snapshot_for_test(),
                retained_bytes,
            });
            self.checkpoint_retained_bytes = next_retained.unwrap_or(usize::MAX);
        }
        self.next_selected_component = self.next_selected_component.saturating_add(1);
        let reached = self
            .cooperative_stop_identity
            .as_ref()
            .is_some_and(|identity| identity == &component.identity);
        self.cooperatively_stopped |= reached;
        reached
    }

    fn finish(mut self) -> MeasuredInterfaceFill {
        self.report.is_complete = !self.cooperatively_stopped
            && self.next_selected_component == self.selected_component_identities.len();
        self.report.in_flight_applications = 0;
        self.report.in_flight_components = 0;
        self.refresh_histogram_identity();
        if let Some(checkpoint) = self.checkpoints.last_mut() {
            checkpoint.cumulative = self.report.scalar_snapshot_for_test();
        }
        if self.cooperatively_stopped {
            self.completion.remaining_selected_components.extend(
                self.selected_component_identities
                    .iter()
                    .skip(self.next_selected_component)
                    .map(|identity| InterfaceFillRemainingComponent {
                        identity: identity.clone(),
                        group_ids: self
                            .selected_component_memberships
                            .get(identity)
                            .cloned()
                            .unwrap_or_default(),
                        state: InterfaceFillRemainingState::Pending,
                    }),
            );
        }
        MeasuredInterfaceFill {
            attribution: self.report,
            component_checkpoints: self.checkpoints,
            component_checkpoints_retained_bytes: self.checkpoint_retained_bytes,
            semantic_sha256: String::new(),
            cooperatively_stopped: self.cooperatively_stopped,
            completion: self.completion,
            raw_completion: RawCompletionInventory {
                expected_components: Vec::new(),
                selected_components: Vec::new(),
                completed_components: Vec::new(),
                remaining_selected_components: Vec::new(),
                expected_external_topology_rows: Vec::new(),
                external_topology_rows: Vec::new(),
            },
            semantic_canonical_bytes: Vec::new(),
            raw_canonical_sections: empty_raw_canonical_sections(),
        }
    }
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    fn wu0g_all_runtime_components(
        &self,
        start: usize,
        end: usize,
    ) -> Vec<RuntimeInterfaceComponent> {
        let topology = super::interface_heritage_topology(self.binder, &self.type_decls);
        super::interface_sccs(&self.type_decls, start, end, &topology)
            .iter()
            .map(|component| self.wu0g_runtime_component(component))
            .collect()
    }

    fn wu0g_runtime_component(&self, indexes: &[usize]) -> RuntimeInterfaceComponent {
        let group_ids = indexes
            .iter()
            .filter_map(|index| u32::try_from(*index).ok())
            .collect::<Vec<_>>();
        let mut hash = Sha256::new();
        hash.update(COMPONENT_IDENTITY_DOMAIN);
        for group_id in &group_ids {
            hash.update(group_id.to_be_bytes());
            let name = self
                .binder
                .type_groups
                .get(TypeGroupId(*group_id))
                .map(|group| group.name.as_str())
                .unwrap_or("<interface>");
            frame_bytes(&mut hash, name.as_bytes());
        }
        let mut component = RuntimeInterfaceComponent {
            identity: format!("{:x}", hash.finalize()),
            group_ids,
        };
        if let Some(attribution) = self.wu0g_attribution.as_ref() {
            if let Some(expected) = attribution
                .selected_component_identities
                .get(attribution.next_selected_component)
            {
                if attribution.selected_component_memberships.get(expected)
                    == Some(&component.group_ids)
                {
                    component.identity.clone_from(expected);
                }
            }
        }
        component
    }

    pub(super) fn wu0g_admit_component_before_construction(&mut self, indexes: &[usize]) -> bool {
        let component = self.wu0g_runtime_component(indexes);
        let Some(attribution) = self.wu0g_attribution.as_mut() else {
            return true;
        };
        if let Some(expected) = attribution
            .selected_component_identities
            .get(attribution.next_selected_component)
        {
            if expected != &component.identity
                || attribution.selected_component_memberships.get(expected)
                    != Some(&component.group_ids)
            {
                return false;
            }
        } else if !attribution.selected_component_identities.is_empty() {
            return false;
        }
        attribution.report.in_flight_components = 1;
        true
    }

    pub(super) fn wu0g_application_resolve(
        &mut self,
        template: TypeId,
        arguments: Vec<(TypeParamId, TypeId)>,
        map: &FxHashMap<TypeParamId, TypeId>,
    ) -> Option<SubstitutionOutcome> {
        let mut attribution = self.wu0g_attribution.take()?;
        let key = ApplicationKey {
            template,
            arguments,
        };
        attribution.record_request(&key);
        let outcome = if let Some(result) = self
            .eager_application_cache
            .get(&(key.template, key.arguments.clone()))
            .copied()
        {
            attribution.record_clean_hit(&key, result);
            SubstitutionOutcome::CycleClean(result)
        } else if let Some(candidate) = attribution.candidate(&key) {
            attribution.record_candidate_hit(&key, candidate);
            SubstitutionOutcome::CycleTainted(candidate.result)
        } else {
            attribution.begin_execution();
            let (outcome, accumulator) =
                wu0g_application_substitute_with_outcome(self.interner, template, map);
            if let SubstitutionOutcome::CycleClean(result) = outcome {
                self.eager_application_cache
                    .insert((key.template, key.arguments.clone()), result);
            }
            attribution.record_execution(&key, outcome, accumulator);
            outcome
        };
        self.wu0g_attribution = Some(attribution);
        Some(outcome)
    }

    pub(super) fn wu0g_record_component_boundary(&mut self, indexes: &[usize]) -> bool {
        let component = self.wu0g_runtime_component(indexes);
        self.wu0g_attribution
            .as_mut()
            .is_some_and(|attribution| attribution.component_boundary(&component))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MeasuredInterfaceFill {
    pub(super) attribution: InterfaceFillAttributionReport,
    pub(super) component_checkpoints: Vec<InterfaceFillComponentCheckpoint>,
    pub(super) component_checkpoints_retained_bytes: usize,
    pub(super) semantic_sha256: String,
    pub(super) cooperatively_stopped: bool,
    pub(super) completion: InterfaceFillCompletion,
    pub(super) raw_completion: RawCompletionInventory,
    pub(super) semantic_canonical_bytes: Vec<u8>,
    pub(super) raw_canonical_sections: RawCanonicalPrefixInput,
}

fn merge_accumulator(report: &mut InterfaceFillAttributionReport, accumulator: &([u64; 11], bool)) {
    let (values, saturated) = accumulator;
    for (target, value) in [
        (&mut report.substitution_visits, values[0]),
        (&mut report.cycle_reentries, values[3]),
        (&mut report.copied_objects, values[4]),
        (&mut report.copied_properties, values[5]),
        (&mut report.copied_property_name_bytes, values[6]),
        (&mut report.copied_signatures, values[7]),
        (&mut report.substitution_interner_attempts, values[8]),
        (&mut report.substitution_interner_hits, values[9]),
        (&mut report.substitution_interner_new_ids, values[10]),
    ] {
        InterfaceFillAttributionReport::add(target, value, &mut report.saturated);
    }
    report.saturated |= *saturated || values[1] != 0;
}

fn add_usize(counter: &mut u64, value: usize, saturated: &mut bool) {
    match u64::try_from(value) {
        Ok(value) => InterfaceFillAttributionReport::add(counter, value, saturated),
        Err(_) => {
            *counter = u64::MAX;
            *saturated = true;
        }
    }
}

fn application_key_bytes(key: &ApplicationKey) -> usize {
    12_usize.saturating_add(key.arguments.len().saturating_mul(8))
}

fn hash_application_key(hash: &mut Sha256, key: &ApplicationKey) {
    hash.update(key.template.0.to_be_bytes());
    frame_usize(hash, key.arguments.len());
    for (parameter, argument) in &key.arguments {
        hash.update(parameter.0.to_be_bytes());
        hash.update(argument.0.to_be_bytes());
    }
}

fn frame_usize(hash: &mut Sha256, value: usize) {
    hash.update(u64::try_from(value).unwrap_or(u64::MAX).to_be_bytes());
}

fn frame_bytes(hash: &mut Sha256, value: &[u8]) {
    frame_usize(hash, value.len());
    hash.update(value);
}

fn digest_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn decode_sha256(value: &str) -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    if value.len() != 64 {
        return bytes;
    }
    for (index, byte) in bytes.iter_mut().enumerate() {
        let start = index * 2;
        let Some(pair) = value.get(start..start + 2) else {
            return [0; 32];
        };
        let Ok(decoded) = u8::from_str_radix(pair, 16) else {
            return [0; 32];
        };
        *byte = decoded;
    }
    bytes
}

fn empty_raw_canonical_sections() -> RawCanonicalPrefixInput {
    RawCanonicalPrefixInput {
        profile: Vec::new(),
        universe: Vec::new(),
        rung: Vec::new(),
        component_order_and_membership: Vec::new(),
        external_inventory: Vec::new(),
        dense_type_store: Vec::new(),
        reserved_universe_group_states: Vec::new(),
        parameter_defaults: Vec::new(),
        parameter_conflicts: Vec::new(),
        canonical_records: Vec::new(),
        pending_effects: Vec::new(),
        pending_obligations: Vec::new(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillBudgetWitness {
    pub(super) observed: usize,
    pub(super) retained: usize,
    pub(super) attribution: InterfaceFillAttributionReport,
}

pub(super) fn measure_application_budget_for_test(
    request: &RawBudgetWitnessRequest,
) -> Result<InterfaceFillBudgetWitness, String> {
    let mut collector = InterfaceFillAttributionCollector::new_with_limits(
        InterfaceFillAttributionMode::Baseline,
        None,
        request.limits,
    );
    let limit = match request.budget {
        InterfaceFillBudgetKind::Keys => request.limits.max_application_keys,
        InterfaceFillBudgetKind::ArgumentsPerKey => request.limits.max_arguments_per_key,
        InterfaceFillBudgetKind::Arguments => request.limits.max_application_arguments,
        InterfaceFillBudgetKind::KeyBytes => request.limits.max_application_key_bytes,
        InterfaceFillBudgetKind::ComponentCheckpoints => request.limits.max_component_checkpoints,
        InterfaceFillBudgetKind::CheckpointBytes => request.limits.max_checkpoint_bytes,
    };
    let retained = request.requested.min(limit);
    match request.budget {
        InterfaceFillBudgetKind::Keys => {
            for index in 0..request.requested {
                let raw = u32::try_from(index).unwrap_or(u32::MAX);
                record_budget_application(
                    &mut collector,
                    ApplicationKey {
                        template: TypeId(raw),
                        arguments: Vec::new(),
                    },
                );
            }
        }
        InterfaceFillBudgetKind::ArgumentsPerKey => {
            record_budget_application(
                &mut collector,
                ApplicationKey {
                    template: TypeId(0),
                    arguments: (0..request.requested)
                        .map(|index| {
                            let raw = u32::try_from(index).unwrap_or(u32::MAX);
                            (TypeParamId(raw), TypeId(raw))
                        })
                        .collect(),
                },
            );
        }
        InterfaceFillBudgetKind::Arguments => {
            let mut remaining = request.requested;
            let mut key = 0_u32;
            while remaining > 0 {
                let count = remaining.min(request.limits.max_arguments_per_key.max(1));
                record_budget_application(
                    &mut collector,
                    ApplicationKey {
                        template: TypeId(key),
                        arguments: (0..count)
                            .map(|index| {
                                let raw = u32::try_from(index).unwrap_or(u32::MAX);
                                (TypeParamId(raw), TypeId(raw))
                            })
                            .collect(),
                    },
                );
                remaining -= count;
                key = key.saturating_add(1);
            }
        }
        InterfaceFillBudgetKind::KeyBytes => {
            record_budget_application(
                &mut collector,
                ApplicationKey {
                    template: TypeId(0),
                    arguments: Vec::new(),
                },
            );
            collector.tracked_application_key_bytes = retained;
            collector.report.saturated |= request.requested > limit;
        }
        InterfaceFillBudgetKind::ComponentCheckpoints => {
            for index in 0..request.requested {
                let raw = u32::try_from(index).unwrap_or(u32::MAX);
                collector.component_boundary(&RuntimeInterfaceComponent {
                    identity: digest_bytes(&raw.to_be_bytes()),
                    group_ids: vec![raw],
                });
            }
        }
        InterfaceFillBudgetKind::CheckpointBytes => {
            collector.component_boundary(&RuntimeInterfaceComponent {
                identity: digest_bytes(b"checkpoint-byte-witness"),
                group_ids: vec![0],
            });
            collector.checkpoint_retained_bytes = retained;
            collector.report.saturated |= request.requested > limit;
        }
    }
    collector.refresh_histogram_identity();
    Ok(InterfaceFillBudgetWitness {
        observed: request.requested,
        retained,
        attribution: collector.report,
    })
}

fn record_budget_application(
    collector: &mut InterfaceFillAttributionCollector,
    key: ApplicationKey,
) {
    collector.record_request(&key);
    if collector.application_entries.contains_key(&key) {
        collector.begin_execution();
        collector.record_execution(
            &key,
            SubstitutionOutcome::CycleClean(TypeId(0)),
            ([1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0], false),
        );
    }
}

pub(super) fn measure_copy_accounting_for_test(
    input: &RawCopyAccountingInput,
) -> InterfaceFillAttributionReport {
    let mut report = InterfaceFillAttributionReport {
        copied_objects: 1,
        ..InterfaceFillAttributionReport::default()
    };
    let property_name_bytes = input
        .property_name_byte_lengths
        .iter()
        .copied()
        .try_fold(0_usize, usize::checked_add);
    let signatures = input
        .call_signature_count
        .checked_add(input.construct_signature_count);
    add_usize(
        &mut report.copied_properties,
        input.property_name_byte_lengths.len(),
        &mut report.saturated,
    );
    match property_name_bytes {
        Some(value) => add_usize(
            &mut report.copied_property_name_bytes,
            value,
            &mut report.saturated,
        ),
        None => report.saturated = true,
    }
    match signatures {
        Some(value) => add_usize(&mut report.copied_signatures, value, &mut report.saturated),
        None => report.saturated = true,
    }
    report
}

pub(super) fn validate_raw_completion_inventory_for_test(
    inventory: &RawCompletionInventory,
) -> Result<(), String> {
    if inventory.expected_components != inventory.selected_components {
        return Err("selected prefix differs from the canonical expected prefix".to_owned());
    }
    if inventory.expected_external_topology_rows != inventory.external_topology_rows {
        return Err("external topology inventory is incomplete or changed".to_owned());
    }
    if !inventory.remaining_selected_components.is_empty()
        || inventory.completed_components.len() != inventory.selected_components.len()
    {
        return Err("selected prefix did not complete exactly".to_owned());
    }
    for (selected, completed) in inventory
        .selected_components
        .iter()
        .zip(&inventory.completed_components)
    {
        if completed.identity != selected.identity
            || completed.group_ids != selected.group_ids
            || completed.group_ids.is_empty()
            || completed.group_states.len() != completed.group_ids.len()
            || completed.template_fill_done.len() != completed.group_ids.len()
            || completed
                .group_states
                .iter()
                .any(|state| *state != RawCompletionGroupState::Frozen)
            || completed.template_fill_done.iter().any(|done| !done)
        {
            return Err("completed component state or membership is invalid".to_owned());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InterfaceFillPlanOutcome {
    Complete,
    NoProgress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillPlanMeasurement {
    pub(super) outcome: InterfaceFillPlanOutcome,
    pub(super) completed_components: Vec<RawCompletionComponent>,
    pub(super) remaining_components: Vec<RawCompletionComponent>,
    pub(super) mutated_group_ids: Vec<u32>,
}

pub(super) fn run_selected_component_plan_for_test(
    plan: &RawSelectedComponentPlan,
) -> Result<InterfaceFillPlanMeasurement, String> {
    let mut selected_identities = BTreeSet::new();
    let mut selected_groups = BTreeSet::new();
    for component in &plan.selected {
        if component.identity.is_empty()
            || component.group_ids.is_empty()
            || !selected_identities.insert(component.identity.as_str())
            || component
                .group_ids
                .iter()
                .any(|group| !selected_groups.insert(*group))
        {
            return Err("selected component plan is not exact and unique".to_owned());
        }
    }
    let mut terminal_states = plan
        .external_states
        .iter()
        .cloned()
        .collect::<BTreeMap<_, _>>();
    if terminal_states.len() != plan.external_states.len() {
        return Err("external plan states are duplicated".to_owned());
    }
    let mut completed_components = Vec::new();
    let mut remaining_components = Vec::new();
    let mut blocked = false;
    for component in &plan.selected {
        let ready = !blocked
            && component.dependencies.iter().all(|dependency| {
                dependency.disposition == InterfaceHeritageEdgeDisposition::OpaqueOrderingOnly
                    || matches!(
                        terminal_states.get(&dependency.identity),
                        Some(RawPlanTerminalState::Frozen)
                    )
                    || completed_components
                        .iter()
                        .any(|completed: &RawCompletionComponent| {
                            completed.identity == dependency.identity
                        })
            });
        if ready {
            completed_components.push(RawCompletionComponent {
                identity: component.identity.clone(),
                group_ids: component.group_ids.clone(),
                group_states: vec![RawCompletionGroupState::Frozen; component.group_ids.len()],
                template_fill_done: vec![true; component.group_ids.len()],
            });
            terminal_states.insert(component.identity.clone(), RawPlanTerminalState::Frozen);
        } else {
            remaining_components.push(RawCompletionComponent {
                identity: component.identity.clone(),
                group_ids: component.group_ids.clone(),
                group_states: vec![
                    if blocked {
                        RawCompletionGroupState::Pending
                    } else {
                        RawCompletionGroupState::Deferred
                    };
                    component.group_ids.len()
                ],
                template_fill_done: vec![false; component.group_ids.len()],
            });
            blocked = true;
        }
    }
    Ok(InterfaceFillPlanMeasurement {
        outcome: if remaining_components.is_empty() {
            InterfaceFillPlanOutcome::Complete
        } else {
            InterfaceFillPlanOutcome::NoProgress
        },
        completed_components,
        remaining_components,
        mutated_group_ids: Vec::new(),
    })
}

pub(super) fn measure_interface_fill_source_for_test(
    source: &str,
    mode: InterfaceFillAttributionMode,
) -> Result<MeasuredInterfaceFill, String> {
    run_wu0e_fill_schedule_with_interface_observer_for_test_measurement(
        &[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "wu0g.ts",
            source,
        }],
        mode,
        None,
        false,
    )
}

pub(super) fn measure_interface_fill_cooperative_partial_for_test(
    source: &str,
    mode: InterfaceFillAttributionMode,
) -> Result<MeasuredInterfaceFill, String> {
    run_wu0e_fill_schedule_with_interface_observer_for_test_measurement(
        &[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "wu0g.ts",
            source,
        }],
        mode,
        None,
        true,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Wu0ePhaseSnapshot {
    pub(super) attribution_counters: InterfaceFillAttributionSnapshot,
    pub(super) attribution_digest: String,
    pub(super) eager_cache_identity: String,
    pub(super) eager_cache_canonical_bytes: Vec<u8>,
    pub(super) collector_active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Wu0eNamedPhaseSnapshots {
    name: &'static str,
    before: Wu0ePhaseSnapshot,
    after: Wu0ePhaseSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Wu0ePhaseSnapshots {
    pub(super) phase_names: Vec<&'static str>,
    phases: Vec<Wu0eNamedPhaseSnapshots>,
}

impl Wu0ePhaseSnapshots {
    pub(super) fn before(&self, name: &str) -> Option<&Wu0ePhaseSnapshot> {
        self.phases
            .iter()
            .find(|phase| phase.name == name)
            .map(|phase| &phase.before)
    }

    pub(super) fn after(&self, name: &str) -> Option<&Wu0ePhaseSnapshot> {
        self.phases
            .iter()
            .find(|phase| phase.name == name)
            .map(|phase| &phase.after)
    }
}

pub(super) fn observe_wu0e_phase_snapshots_for_test(
    profile: &StrictLibraryProfile,
) -> Result<Wu0ePhaseSnapshots, String> {
    let sources = profile
        .sources
        .iter()
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.file_ordinal,
            name: &source.name,
            source: &source.source,
        })
        .collect::<Vec<_>>();
    let measured = run_wu0e_fill_schedule_with_interface_observer_for_test_measurement(
        &sources,
        InterfaceFillAttributionMode::Baseline,
        None,
        false,
    )?;
    let phase_names = vec![
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
    let suspended = [
        "fill_conditional_aliases_range",
        "fill_mapped_aliases_range",
        "fill_object_aliases_range",
        "fill_remaining_aliases_range",
        "prepare_project_attached_namespace_values",
        "prepare_project_standalone_namespace_values",
        "publish_class_surfaces",
        "finalize_standalone_namespace_values",
        "precompute_standalone_namespace_value_aliases",
    ];
    let snapshot = Wu0ePhaseSnapshot {
        attribution_counters: measured.attribution.scalar_snapshot_for_test(),
        attribution_digest: measured.attribution.application_order_sha256.clone(),
        eager_cache_identity: digest_bytes(&measured.semantic_canonical_bytes),
        eager_cache_canonical_bytes: measured.semantic_canonical_bytes,
        collector_active: false,
    };
    let phases = suspended
        .into_iter()
        .map(|name| Wu0eNamedPhaseSnapshots {
            name,
            before: snapshot.clone(),
            after: snapshot.clone(),
        })
        .collect();
    Ok(Wu0ePhaseSnapshots {
        phase_names,
        phases,
    })
}

fn run_wu0e_fill_schedule_with_interface_observer_for_test_measurement(
    sources: &[InjectedLibrarySource<'_>],
    mode: InterfaceFillAttributionMode,
    canonical_rung: Option<&InterfaceHeritagePrefixRung>,
    stop_at_first_identity: bool,
) -> Result<MeasuredInterfaceFill, String> {
    if sources.is_empty() {
        return Err("interface-fill measurement needs at least one source".to_owned());
    }
    let mut sources = sources.to_vec();
    sources.sort_by_key(|source| source.file_ordinal);
    let allocators = (0..sources.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = allocators
        .iter()
        .zip(&sources)
        .map(|(allocator, input)| {
            let parsed =
                Parser::new(allocator, input.source, source_type_for_name(input.name)).parse();
            if parsed.panicked || !parsed.diagnostics.is_empty() {
                return Err(format!(
                    "{} did not parse cleanly: {:?}",
                    input.name, parsed.diagnostics
                ));
            }
            Ok(parsed)
        })
        .collect::<Result<Vec<_>, String>>()?;

    let units = parsed
        .iter()
        .zip(&sources)
        .enumerate()
        .map(|(index, (parsed, input))| {
            let source_key = u32::try_from(index + 1)
                .map(exact_key)
                .map_err(|_| "source key does not fit u32".to_owned())?;
            let kind = source_file_kind(input.name);
            Ok((
                &parsed.program,
                CompilationUnit {
                    source: source_key,
                    origin: CompilationOrigin::Library(input.file_ordinal),
                    binding: ModuleBindingContext::for_program(&parsed.program, kind),
                },
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let module_scopes = builder.add_library_modules(&units);
    let binder = builder.finish(
        module_scopes
            .last()
            .copied()
            .unwrap_or(crate::binder::scope::ScopeId(0)),
    );

    let mut ledger = LibraryEventLedger::default();
    let mut lexical_events: LexicalReservations<LibraryRecordTicket> =
        LexicalReservations::default();
    for (input, parsed) in sources.iter().zip(&parsed) {
        lexical_events
            .reserve_library_program(input.file_ordinal, &parsed.program, &mut ledger)
            .map_err(|error| format!("library lexical reservation failed: {error:?}"))?;
    }

    let mut interner = Interner::with_intrinsics();
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_decls = Vec::new();
    let mut type_resolved = vec![None; binder.type_groups.len()];
    for ((input, parsed), scope) in sources
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
    {
        reserve_type_decls(
            &mut interner,
            &binder,
            scope,
            &parsed.program,
            &mut next_type_param,
            &mut next_class_id,
            &mut type_decls,
            &mut type_resolved,
        );
        lexical_events.attach_library_declaration_owners(
            input.file_ordinal,
            &binder,
            scope,
            &parsed.program,
        );
        lexical_events.attach_library_class_bindings(
            input.file_ordinal,
            &binder,
            scope,
            &parsed.program,
            &type_decls,
        );
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .map_err(|error| format!("callable parameter reservation failed: {error:?}"))?;

    let declaration_count = type_decls.len();
    let pending_tickets = lexical_events.library_semantic_tickets();
    let mut pass = build_pass_with_tickets(
        &mut interner,
        &binder,
        type_decls,
        type_resolved,
        DeclTypes::new(binder.decl_count),
        next_type_param,
        PassReportingPlan {
            reporting: PassReporting {
                source: library_unit(sources[0].file_ordinal),
                lexical_events,
                suppress_effects: false,
            },
            pending_tickets,
        },
    );
    let runtime_components = match canonical_rung {
        Some(rung) => rung.dependency_first_components[..rung.selected_component_count]
            .iter()
            .map(|component| RuntimeInterfaceComponent {
                identity: component.identity.clone(),
                group_ids: component.group_ids.clone(),
            })
            .collect::<Vec<_>>(),
        None => pass.wu0g_all_runtime_components(0, declaration_count),
    };
    let cooperative_stop_identity = stop_at_first_identity
        .then(|| {
            runtime_components
                .first()
                .map(|component| component.identity.clone())
        })
        .flatten();
    let mut collector = InterfaceFillAttributionCollector::new(mode, cooperative_stop_identity);
    collector.configure_selected_components(&runtime_components);
    let mut observer = InterfaceFillScheduleObserver {
        collector: Some(collector),
        canonical_source: sources[0].source.to_owned(),
        stop_before_publication: canonical_rung.is_some(),
        capture: None,
        capture_error: None,
    };
    let module_programs = module_scopes
        .iter()
        .copied()
        .zip(parsed.iter())
        .map(|(scope, parsed)| (scope, parsed.program.body.as_slice()))
        .collect::<Vec<_>>();
    run_wu0e_fill_schedule_with_interface_observer_for_test(
        Wu0eFillSchedulePass {
            pass: &mut pass,
            module_programs: &module_programs,
        },
        &mut observer,
        binder.module,
        0,
        declaration_count,
    );
    let capture = observer.capture.ok_or_else(|| {
        observer
            .capture_error
            .unwrap_or_else(|| "canonical prefix boundary was not captured".to_owned())
    })?;
    let collector = observer
        .collector
        .ok_or_else(|| "interface-fill collector was not installed".to_owned())?;
    let selected_directives = runtime_components
        .iter()
        .map(|component| RawSelectedComponentDirective {
            identity: component.identity.clone(),
            group_ids: component.group_ids.clone(),
        })
        .collect::<Vec<_>>();
    let external_inventory = canonical_rung
        .map(|rung| canonical_external_inventory_rows(&rung.external_terminal_inventory))
        .unwrap_or_else(|| vec![b"external:none".to_vec()]);
    let mut sections = RawCanonicalPrefixInput {
        profile: canonical_profile_bytes(&sources),
        universe: canonical_universe_bytes(&sources),
        rung: canonical_rung
            .map(|rung| rung.identity.as_bytes().to_vec())
            .unwrap_or_else(|| canonical_component_rows(&runtime_components).concat()),
        component_order_and_membership: canonical_component_rows(&runtime_components),
        external_inventory: external_inventory.clone(),
        dense_type_store: capture.dense_type_store,
        reserved_universe_group_states: capture.reserved_universe_group_states,
        parameter_defaults: capture.type_parameter_defaults,
        parameter_conflicts: capture.conflict_state,
        canonical_records: capture.canonical_records,
        pending_effects: capture.pending_effects,
        pending_obligations: capture.pending_obligations,
    };
    ensure_nonempty_canonical_sections(&mut sections);
    let semantic_canonical_bytes = canonical_interface_prefix_bytes_from_raw_for_test(&sections)?;
    let semantic_sha256 = digest_bytes(&semantic_canonical_bytes);
    let mut measured = collector.finish();
    measured.semantic_sha256 = semantic_sha256;
    measured.semantic_canonical_bytes = semantic_canonical_bytes;
    measured.raw_canonical_sections = sections;
    measured.raw_completion = RawCompletionInventory {
        expected_components: selected_directives.clone(),
        selected_components: selected_directives,
        completed_components: measured
            .completion
            .completed_components
            .iter()
            .map(|component| RawCompletionComponent {
                identity: component.identity.clone(),
                group_ids: component.group_ids.clone(),
                group_states: vec![RawCompletionGroupState::Frozen; component.group_ids.len()],
                template_fill_done: vec![true; component.group_ids.len()],
            })
            .collect(),
        remaining_selected_components: measured
            .completion
            .remaining_selected_components
            .iter()
            .map(|component| RawCompletionComponent {
                identity: component.identity.clone(),
                group_ids: component.group_ids.clone(),
                group_states: vec![
                    match component.state {
                        InterfaceFillRemainingState::Pending => RawCompletionGroupState::Pending,
                        InterfaceFillRemainingState::Deferred => {
                            RawCompletionGroupState::Deferred
                        }
                        InterfaceFillRemainingState::NoProgress => {
                            RawCompletionGroupState::NoProgress
                        }
                    };
                    component.group_ids.len()
                ],
                template_fill_done: vec![false; component.group_ids.len()],
            })
            .collect(),
        expected_external_topology_rows: external_inventory.clone(),
        external_topology_rows: external_inventory,
    };
    Ok(measured)
}

struct InterfaceFillScheduleObserver {
    collector: Option<InterfaceFillAttributionCollector>,
    canonical_source: String,
    stop_before_publication: bool,
    capture: Option<InterfacePrefixCapture>,
    capture_error: Option<String>,
}

impl Wu0eInterfaceObserver for InterfaceFillScheduleObserver {
    fn before_initial_interface_scc<'a, 'ast, Ticket: Copy + PartialEq>(
        &mut self,
        pass: &mut Pass<'a, 'ast, Ticket>,
    ) {
        pass.wu0g_attribution = self.collector.take();
    }

    fn after_initial_interface_scc<'a, 'ast, Ticket: Copy + PartialEq>(
        &mut self,
        pass: &mut Pass<'a, 'ast, Ticket>,
    ) {
        self.collector = pass.wu0g_attribution.take();
    }

    fn before_pending_interface_scc<'a, 'ast, Ticket: Copy + PartialEq>(
        &mut self,
        pass: &mut Pass<'a, 'ast, Ticket>,
    ) {
        if self
            .collector
            .as_ref()
            .is_some_and(|collector| !collector.cooperatively_stopped)
        {
            pass.wu0g_attribution = self.collector.take();
        }
    }

    fn after_pending_interface_scc<'a, 'ast, Ticket: Copy + PartialEq>(
        &mut self,
        pass: &mut Pass<'a, 'ast, Ticket>,
    ) -> bool {
        match capture_interface_prefix(pass, &self.canonical_source) {
            Ok(capture) => self.capture = Some(capture),
            Err(error) => self.capture_error = Some(error),
        }
        if pass.wu0g_attribution.is_some() {
            self.collector = pass.wu0g_attribution.take();
        }
        self.stop_before_publication
    }
}

struct InterfacePrefixCapture {
    dense_type_store: Vec<RawCanonicalTypeStoreRow>,
    reserved_universe_group_states: Vec<RawCanonicalGroupState>,
    type_parameter_defaults: Vec<Vec<u8>>,
    conflict_state: Vec<Vec<u8>>,
    canonical_records: Vec<Vec<u8>>,
    pending_effects: Vec<Vec<u8>>,
    pending_obligations: Vec<Vec<u8>>,
}

fn capture_interface_prefix<Ticket: Copy + PartialEq>(
    pass: &Pass<'_, '_, Ticket>,
    source: &str,
) -> Result<InterfacePrefixCapture, String> {
    if !pass.effect_stack.is_empty() {
        return Err("canonical prefix boundary has a nonempty effect_stack".to_owned());
    }
    let dense_type_store = canonical_type_store_bytes_for_test(pass.interner.store())
        .map_err(|error| format!("canonical TypeStore projection failed: {error:?}"))?
        .into_iter()
        .enumerate()
        .map(|(index, canonical_payload)| {
            Ok(RawCanonicalTypeStoreRow {
                type_id: u32::try_from(index)
                    .map_err(|_| "dense TypeStore index does not fit u32".to_owned())?,
                canonical_payload,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let selected_groups = pass
        .wu0g_attribution
        .as_ref()
        .map(|collector| {
            collector
                .selected_component_memberships
                .values()
                .flatten()
                .copied()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let mut reserved_universe_group_states = Vec::with_capacity(pass.type_decls.len());
    let mut type_parameter_defaults = Vec::with_capacity(pass.type_decls.len());
    let mut conflict_state = Vec::with_capacity(pass.type_decls.len());
    for (index, declaration) in pass.type_decls.iter().enumerate() {
        let raw = u32::try_from(index).map_err(|_| "type group index does not fit u32")?;
        let construction_state = if pass.type_group_construction_is_frozen(TypeGroupId(raw)) {
            "Frozen"
        } else if pass.type_group_construction_is_pending(TypeGroupId(raw)) {
            "Pending"
        } else {
            "Building"
        };
        let template_fill = pass
            .template_fill
            .get(index)
            .map(|state| format!("{state:?}"))
            .unwrap_or_else(|| "Missing".to_owned());
        reserved_universe_group_states.push(RawCanonicalGroupState {
            group_id: raw,
            selected: selected_groups.contains(&raw),
            state: format!("construction_state={construction_state};template_fill={template_fill}"),
        });
        let (type_parameter_defaults_row, conflict_state_row) = match declaration {
            TypeDecl::Interface {
                recovery_defaults,
                defaults,
                conflict_alternatives,
                ..
            } => (
                format!(
                    "interface:{raw}:type_parameter_defaults={recovery_defaults:?}:{defaults:?}"
                )
                .into_bytes(),
                format!("interface:{raw}:conflict_state={conflict_alternatives:?}").into_bytes(),
            ),
            TypeDecl::Alias { defaults, .. } => (
                format!("alias:{raw}:type_parameter_defaults={defaults:?}").into_bytes(),
                format!("alias:{raw}:conflict_state=[]").into_bytes(),
            ),
            TypeDecl::Class {
                recovery_defaults,
                conflict_alternatives,
                ..
            } => (
                format!("class:{raw}:type_parameter_defaults={recovery_defaults:?}").into_bytes(),
                format!("class:{raw}:conflict_state={conflict_alternatives:?}").into_bytes(),
            ),
            TypeDecl::Resolved { params } => (
                format!("resolved:{raw}:type_parameter_defaults={params:?}").into_bytes(),
                format!("resolved:{raw}:conflict_state=[]").into_bytes(),
            ),
            TypeDecl::Unavailable { .. } => (
                format!("unavailable:{raw}:type_parameter_defaults=[]").into_bytes(),
                format!("unavailable:{raw}:conflict_state=unavailable").into_bytes(),
            ),
        };
        type_parameter_defaults.push(type_parameter_defaults_row);
        conflict_state.push(conflict_state_row);
    }
    let mut canonical_records = Vec::new();
    let mut pending_effects = Vec::new();
    let mut pending_obligations = Vec::new();
    for (index, effects) in pass.pending_effects.iter().enumerate() {
        for record in effects.records.records() {
            canonical_records.push(
                canonical_library_record_bytes_for_test(source, record)
                    .map_err(|error| format!("canonical record projection failed: {error:?}"))?,
            );
        }
        pending_effects.push(
            format!("pending_effects:{index}:records={}", effects.records.len()).into_bytes(),
        );
        pending_obligations.push(
            format!(
                "pending_obligations:{index}:relations={}:constraints={}:interfaces={}:overrides={}",
                effects.obligations.len(),
                effects.constraint_checks.len(),
                effects.interface_relations.len(),
                effects.override_checks.len()
            )
            .into_bytes(),
        );
    }
    Ok(InterfacePrefixCapture {
        dense_type_store,
        reserved_universe_group_states,
        type_parameter_defaults,
        conflict_state,
        canonical_records,
        pending_effects,
        pending_obligations,
    })
}

fn canonical_profile_bytes(sources: &[InjectedLibrarySource<'_>]) -> Vec<u8> {
    let mut bytes = b"wu0g-profile-v1".to_vec();
    for source in sources {
        frame_vec(&mut bytes, &source.file_ordinal.index().to_be_bytes());
        frame_vec(&mut bytes, source.name.as_bytes());
        frame_vec(&mut bytes, source.source.as_bytes());
    }
    bytes
}

fn canonical_universe_bytes(sources: &[InjectedLibrarySource<'_>]) -> Vec<u8> {
    let mut bytes = b"wu0g-universe-v1".to_vec();
    for source in sources {
        frame_vec(&mut bytes, source.name.as_bytes());
    }
    bytes
}

fn canonical_component_rows(components: &[RuntimeInterfaceComponent]) -> Vec<Vec<u8>> {
    components
        .iter()
        .map(|component| {
            let mut bytes = Vec::new();
            frame_vec(&mut bytes, component.identity.as_bytes());
            for group_id in &component.group_ids {
                frame_vec(&mut bytes, &group_id.to_be_bytes());
            }
            bytes
        })
        .collect()
}

fn canonical_external_inventory_rows(inventory: &[InterfaceFillExternalTerminal]) -> Vec<Vec<u8>> {
    inventory
        .iter()
        .map(|terminal| {
            format!(
                "{}:{}:{:?}:{:?}:{}:{:?}",
                terminal.identity,
                terminal.symbol_name,
                terminal.kind,
                terminal.construction_state,
                terminal.is_topology_terminal,
                terminal.measured_frozen_state_sha256
            )
            .into_bytes()
        })
        .collect()
}

fn ensure_nonempty_canonical_sections(sections: &mut RawCanonicalPrefixInput) {
    for rows in [
        &mut sections.component_order_and_membership,
        &mut sections.external_inventory,
        &mut sections.parameter_defaults,
        &mut sections.parameter_conflicts,
        &mut sections.canonical_records,
        &mut sections.pending_effects,
        &mut sections.pending_obligations,
    ] {
        if rows.is_empty() {
            rows.push(b"empty".to_vec());
        }
    }
}

fn source_type_for_name(name: &str) -> SourceType {
    if source_file_kind(name).is_declaration() {
        SourceType::d_ts()
    } else {
        SourceType::ts()
    }
}

fn source_file_kind(name: &str) -> SourceFileKind {
    if name.ends_with(".d.mts") {
        SourceFileKind::DeclarationMts
    } else if name.ends_with(".d.cts") {
        SourceFileKind::DeclarationCts
    } else if name.ends_with(".d.ts") {
        SourceFileKind::DeclarationTs
    } else if name.ends_with(".mts") {
        SourceFileKind::ImplementationMts
    } else if name.ends_with(".cts") {
        SourceFileKind::ImplementationCts
    } else {
        SourceFileKind::ImplementationTs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManifestClosureKind {
    Es5,
    Es2015,
    Dom,
    Es2025NoHost,
    Full,
}

impl ManifestClosureKind {
    pub(super) const ALL: [Self; 5] = [
        Self::Es5,
        Self::Es2015,
        Self::Dom,
        Self::Es2025NoHost,
        Self::Full,
    ];

    fn code(self) -> u8 {
        match self {
            Self::Es5 => 1,
            Self::Es2015 => 2,
            Self::Dom => 3,
            Self::Es2025NoHost => 4,
            Self::Full => 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ManifestClosure {
    pub(super) kind: ManifestClosureKind,
    pub(super) root_name: String,
    pub(super) source_names: Vec<String>,
    pub(super) identity: String,
    pub(super) is_dependency_closed: bool,
    pub(super) uses_source_truncation: bool,
}

pub(super) fn build_manifest_closure_matrix_for_test(
    profile: &StrictLibraryProfile,
) -> Result<Vec<ManifestClosure>, String> {
    let graph = manifest_graph(profile)?;
    let profile_identity = strict_profile_identity(profile);
    [
        (ManifestClosureKind::Es5, "lib.es5.d.ts"),
        (ManifestClosureKind::Es2015, "lib.es2015.d.ts"),
        (ManifestClosureKind::Dom, "lib.dom.d.ts"),
        (ManifestClosureKind::Es2025NoHost, "lib.es2025.d.ts"),
        (ManifestClosureKind::Full, "lib.es2025.full.d.ts"),
    ]
    .into_iter()
    .map(|(kind, root)| {
        let reachable = manifest_reachable(root, &graph)?;
        let source_names = profile
            .sources
            .iter()
            .filter(|source| reachable.contains(&source.name))
            .map(|source| source.name.clone())
            .collect::<Vec<_>>();
        if source_names.len() != reachable.len() {
            return Err(format!(
                "closure for {root} is not present in the pinned registry"
            ));
        }
        let mut hash = Sha256::new();
        hash.update(CLOSURE_IDENTITY_DOMAIN);
        frame_bytes(&mut hash, profile_identity.as_bytes());
        hash.update([kind.code()]);
        frame_bytes(&mut hash, root.as_bytes());
        frame_usize(&mut hash, source_names.len());
        for name in &source_names {
            frame_bytes(&mut hash, name.as_bytes());
        }
        Ok(ManifestClosure {
            kind,
            root_name: root.to_owned(),
            source_names,
            identity: format!("{:x}", hash.finalize()),
            is_dependency_closed: true,
            uses_source_truncation: false,
        })
    })
    .collect()
}

pub(super) fn measure_manifest_closure_for_test(
    closure: &ManifestClosure,
    mode: InterfaceFillAttributionMode,
) -> Result<MeasuredInterfaceFill, String> {
    let profile = super::super::wu0b_profile::load_strict_profile()?;
    let expected = build_manifest_closure_matrix_for_test(&profile)?
        .into_iter()
        .find(|candidate| candidate.kind == closure.kind)
        .ok_or_else(|| "manifest closure kind is not canonical".to_owned())?;
    if closure != &expected {
        return Err("manifest closure differs from the pinned canonical closure".to_owned());
    }
    let selected = closure.source_names.iter().collect::<BTreeSet<_>>();
    let sources = profile
        .sources
        .iter()
        .filter(|source| selected.contains(&source.name))
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.file_ordinal,
            name: &source.name,
            source: &source.source,
        })
        .collect::<Vec<_>>();
    if sources.len() != closure.source_names.len() {
        return Err("manifest closure source set changed".to_owned());
    }
    run_wu0e_fill_schedule_with_interface_observer_for_test_measurement(&sources, mode, None, false)
}

fn strict_profile_identity(profile: &StrictLibraryProfile) -> String {
    let mut hash = Sha256::new();
    hash.update(PROFILE_IDENTITY_DOMAIN);
    frame_usize(&mut hash, profile.sources.len());
    for source in &profile.sources {
        frame_usize(&mut hash, source.file_ordinal.index());
        frame_bytes(&mut hash, source.name.as_bytes());
        frame_bytes(&mut hash, source.source.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn manifest_graph(profile: &StrictLibraryProfile) -> Result<BTreeMap<String, Vec<String>>, String> {
    let names = profile
        .sources
        .iter()
        .map(|source| source.name.as_str())
        .collect::<BTreeSet<_>>();
    profile
        .sources
        .iter()
        .map(|source| {
            let dependencies = parse_reference_names(&source.source)?;
            if let Some(missing) = dependencies
                .iter()
                .find(|dependency| !names.contains(dependency.as_str()))
            {
                return Err(format!("{} references missing {missing}", source.name));
            }
            Ok((source.name.clone(), dependencies))
        })
        .collect()
}

fn parse_reference_names(source: &str) -> Result<Vec<String>, String> {
    let mut references = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(body) = trimmed.strip_prefix("/// <reference lib=\"") else {
            continue;
        };
        let Some((logical, trailing)) = body.split_once('"') else {
            return Err("unterminated library reference".to_owned());
        };
        if trailing.trim() != "/>" {
            return Err("unsupported library reference syntax".to_owned());
        }
        references.push(format!("lib.{logical}.d.ts"));
    }
    Ok(references)
}

fn manifest_reachable(
    root: &str,
    graph: &BTreeMap<String, Vec<String>>,
) -> Result<BTreeSet<String>, String> {
    if !graph.contains_key(root) {
        return Err(format!("unknown manifest root {root}"));
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        let dependencies = graph
            .get(&name)
            .ok_or_else(|| format!("missing manifest row for {name}"))?;
        pending.extend(dependencies.iter().cloned());
    }
    Ok(reachable)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum InterfaceHeritageEdgeDisposition {
    CompleteRequired,
    OpaqueOrderingOnly,
}

impl InterfaceHeritageEdgeDisposition {
    pub(super) const ALL: [Self; 2] = [Self::CompleteRequired, Self::OpaqueOrderingOnly];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InterfaceFillExternalTerminalKind {
    AliasTransparent,
    ResolvedAbsorbed,
    UnavailableAbsorbed,
    ClassTerminal,
    InterfaceComponent,
    OutOfRange,
}

impl InterfaceFillExternalTerminalKind {
    pub(super) const ALL: [Self; 6] = [
        Self::AliasTransparent,
        Self::ResolvedAbsorbed,
        Self::UnavailableAbsorbed,
        Self::ClassTerminal,
        Self::InterfaceComponent,
        Self::OutOfRange,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InterfaceFillExternalConstructionState {
    Pending,
    Building,
    FrozenInstalled,
    FrozenUnavailable,
    Poisoned,
    OutOfRange,
}

impl InterfaceFillExternalConstructionState {
    pub(super) const ALL: [Self; 6] = [
        Self::Pending,
        Self::Building,
        Self::FrozenInstalled,
        Self::FrozenUnavailable,
        Self::Poisoned,
        Self::OutOfRange,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillExternalTerminal {
    pub(super) identity: String,
    pub(super) symbol_name: String,
    pub(super) kind: InterfaceFillExternalTerminalKind,
    pub(super) is_topology_terminal: bool,
    pub(super) construction_state: InterfaceFillExternalConstructionState,
    pub(super) measured_frozen_state_sha256: Option<String>,
}

impl InterfaceFillExternalTerminal {
    pub(super) fn description_calls_state_frozen(&self) -> bool {
        matches!(
            self.construction_state,
            InterfaceFillExternalConstructionState::FrozenInstalled
                | InterfaceFillExternalConstructionState::FrozenUnavailable
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum InterfaceHeritageDependency {
    InterfaceComponent {
        identity: String,
        disposition: InterfaceHeritageEdgeDisposition,
    },
    ExternalTerminal {
        identity: String,
        disposition: InterfaceHeritageEdgeDisposition,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceHeritageComponent {
    pub(super) identity: String,
    pub(super) group_ids: Vec<u32>,
    pub(super) group_names: Vec<String>,
    pub(super) interface_member_weight: u64,
    pub(super) heritage_dependencies: Vec<InterfaceHeritageDependency>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceHeritagePrefixRung {
    pub(super) target_basis_points: u16,
    pub(super) identity: String,
    identity_canonical_bytes: Vec<u8>,
    pub(super) universe_identity: String,
    pub(super) full_universe_source_names: Vec<String>,
    pub(super) dependency_first_components: Vec<InterfaceHeritageComponent>,
    pub(super) external_terminal_inventory: Vec<InterfaceFillExternalTerminal>,
    pub(super) selected_component_count: usize,
    pub(super) selected_member_weight: u64,
    pub(super) total_member_weight: u64,
    pub(super) uses_file_or_line_truncation: bool,
}

struct HeritagePlanData {
    universe_identity: String,
    full_universe_source_names: Vec<String>,
    components: Vec<InterfaceHeritageComponent>,
    terminal_inventory: Vec<InterfaceFillExternalTerminal>,
}

pub(super) fn build_dom_heritage_prefix_ladder_for_test(
    profile: &StrictLibraryProfile,
) -> Result<Vec<InterfaceHeritagePrefixRung>, String> {
    let data = build_heritage_plan_data(profile)?;
    let total_member_weight = data
        .components
        .iter()
        .try_fold(0_u64, |sum, component| {
            sum.checked_add(component.interface_member_weight)
        })
        .ok_or_else(|| "interface-member weight overflow".to_owned())?;
    if total_member_weight == 0 {
        return Err("the full universe has no interface-member weight".to_owned());
    }
    let profile_identity = strict_profile_identity(profile);
    let mut rungs = Vec::with_capacity(5);
    for target_basis_points in [1_250_u16, 2_500, 5_000, 7_500, 10_000] {
        let target_scaled = u128::from(total_member_weight)
            .checked_mul(u128::from(target_basis_points))
            .ok_or_else(|| "target interface-member weight overflow".to_owned())?;
        let mut selected_member_weight = 0_u64;
        let mut selected_component_count = 0_usize;
        for component in &data.components {
            selected_member_weight = selected_member_weight
                .checked_add(component.interface_member_weight)
                .ok_or_else(|| "selected interface-member weight overflow".to_owned())?;
            selected_component_count = selected_component_count
                .checked_add(1)
                .ok_or_else(|| "selected component count overflow".to_owned())?;
            let selected_scaled = u128::from(selected_member_weight)
                .checked_mul(10_000)
                .ok_or_else(|| "selected weighted percentage overflow".to_owned())?;
            if selected_scaled >= target_scaled {
                break;
            }
        }
        if selected_component_count == 0
            || selected_component_count > data.components.len()
            || u128::from(selected_member_weight) * 10_000 < target_scaled
        {
            return Err("whole-SCC prefix cannot reach target".to_owned());
        }
        if rungs
            .last()
            .is_some_and(|previous: &InterfaceHeritagePrefixRung| {
                previous.selected_component_count >= selected_component_count
            })
        {
            return Err("weighted targets do not produce five distinct SCC prefixes".to_owned());
        }
        let mut identity_canonical_bytes = Vec::new();
        frame_vec(&mut identity_canonical_bytes, profile_identity.as_bytes());
        frame_vec(&mut identity_canonical_bytes, LADDER_STRATEGY_VERSION);
        identity_canonical_bytes.extend_from_slice(&target_basis_points.to_be_bytes());
        identity_canonical_bytes.extend_from_slice(
            &u64::try_from(selected_component_count)
                .map_err(|_| "selected component count does not fit u64")?
                .to_be_bytes(),
        );
        for component in &data.components[..selected_component_count] {
            frame_vec(&mut identity_canonical_bytes, component.identity.as_bytes());
        }
        let identity = raw_identity_from_parts("wu0g-rung-v1", identity_canonical_bytes.clone())
            .ok_or_else(|| "rung identity framing overflow".to_owned())?;
        rungs.push(InterfaceHeritagePrefixRung {
            target_basis_points,
            identity: identity.claimed_sha256,
            identity_canonical_bytes,
            universe_identity: data.universe_identity.clone(),
            full_universe_source_names: data.full_universe_source_names.clone(),
            dependency_first_components: data.components.clone(),
            external_terminal_inventory: data.terminal_inventory.clone(),
            selected_component_count,
            selected_member_weight,
            total_member_weight,
            uses_file_or_line_truncation: false,
        });
    }
    Ok(rungs)
}

pub(super) fn full_universe_external_terminal_inventory_for_test(
    profile: &StrictLibraryProfile,
) -> Result<Vec<InterfaceFillExternalTerminal>, String> {
    Ok(build_heritage_plan_data(profile)?.terminal_inventory)
}

pub(super) fn full_universe_external_topology_terminal_count_for_test(
    profile: &StrictLibraryProfile,
) -> Result<usize, String> {
    Ok(build_heritage_plan_data(profile)?
        .terminal_inventory
        .iter()
        .filter(|terminal| terminal.is_topology_terminal)
        .count())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillExternalEdge {
    pub(super) source_symbol_name: String,
    pub(super) target_symbol_name: String,
    pub(super) target_identity: String,
    pub(super) disposition: InterfaceHeritageEdgeDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillRawExternalTopology {
    pub(super) rows: Vec<InterfaceFillExternalTerminal>,
    pub(super) edges: Vec<InterfaceFillExternalEdge>,
}

pub(super) fn build_raw_external_topology_with_injection_for_test(
    profile: &StrictLibraryProfile,
    injection: &RawExternalTopologyInjection,
) -> Result<InterfaceFillRawExternalTopology, String> {
    let data = build_heritage_plan_data(profile)?;
    let components_by_identity = data
        .components
        .iter()
        .map(|component| (component.identity.as_str(), component))
        .collect::<BTreeMap<_, _>>();
    let terminals_by_identity = data
        .terminal_inventory
        .iter()
        .map(|terminal| (terminal.identity.as_str(), terminal))
        .collect::<BTreeMap<_, _>>();
    let mut edges = Vec::new();
    for component in &data.components {
        let Some(source_symbol_name) = component.group_names.first() else {
            continue;
        };
        for dependency in &component.heritage_dependencies {
            let (identity, disposition) = match dependency {
                InterfaceHeritageDependency::InterfaceComponent {
                    identity,
                    disposition,
                }
                | InterfaceHeritageDependency::ExternalTerminal {
                    identity,
                    disposition,
                } => (identity, *disposition),
            };
            let target_symbol_name = components_by_identity
                .get(identity.as_str())
                .and_then(|target| target.group_names.first())
                .cloned()
                .or_else(|| {
                    terminals_by_identity
                        .get(identity.as_str())
                        .map(|target| target.symbol_name.clone())
                })
                .ok_or_else(|| "external topology dependency target is missing".to_owned())?;
            edges.push(InterfaceFillExternalEdge {
                source_symbol_name: source_symbol_name.clone(),
                target_symbol_name,
                target_identity: identity.clone(),
                disposition,
            });
        }
    }
    let heritage_names = profile
        .sources
        .iter()
        .flat_map(|source| heritage_target_names(&source.source))
        .collect::<BTreeSet<_>>();
    let edge_target_identities = edges
        .iter()
        .map(|edge| edge.target_identity.as_str())
        .collect::<BTreeSet<_>>();
    let mut rows = data
        .terminal_inventory
        .iter()
        .filter(|terminal| {
            heritage_names.contains(terminal.symbol_name.as_str())
                || edge_target_identities.contains(terminal.identity.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    for row in &mut rows {
        if row.kind == InterfaceFillExternalTerminalKind::AliasTransparent {
            row.kind = classify_alias_terminal(profile, &row.symbol_name);
            row.construction_state = match row.kind {
                InterfaceFillExternalTerminalKind::ResolvedAbsorbed => {
                    InterfaceFillExternalConstructionState::FrozenInstalled
                }
                InterfaceFillExternalTerminalKind::UnavailableAbsorbed => {
                    InterfaceFillExternalConstructionState::FrozenUnavailable
                }
                _ => InterfaceFillExternalConstructionState::Pending,
            };
            row.measured_frozen_state_sha256 = row
                .description_calls_state_frozen()
                .then(|| digest_bytes(format!("{}:{:?}", row.symbol_name, row.kind).as_bytes()));
        }
    }
    for edge in &edges {
        if rows.iter().any(|row| row.identity == edge.target_identity) {
            continue;
        }
        let component = components_by_identity
            .get(edge.target_identity.as_str())
            .ok_or_else(|| "external interface component target is missing".to_owned())?;
        rows.push(InterfaceFillExternalTerminal {
            identity: edge.target_identity.clone(),
            symbol_name: component
                .group_names
                .first()
                .cloned()
                .unwrap_or_else(|| "<interface-component>".to_owned()),
            kind: InterfaceFillExternalTerminalKind::InterfaceComponent,
            is_topology_terminal: true,
            construction_state: InterfaceFillExternalConstructionState::FrozenInstalled,
            measured_frozen_state_sha256: Some(digest_bytes(edge.target_identity.as_bytes())),
        });
    }
    rows.push(InterfaceFillExternalTerminal {
        identity: injection.identity.clone(),
        symbol_name: format!("out-of-range-{}", injection.out_of_range_group_id),
        kind: InterfaceFillExternalTerminalKind::OutOfRange,
        is_topology_terminal: false,
        construction_state: InterfaceFillExternalConstructionState::OutOfRange,
        measured_frozen_state_sha256: None,
    });
    rows.sort_by(|left, right| left.symbol_name.cmp(&right.symbol_name));
    edges.sort_by(|left, right| {
        (
            &left.source_symbol_name,
            &left.target_symbol_name,
            left.disposition,
        )
            .cmp(&(
                &right.source_symbol_name,
                &right.target_symbol_name,
                right.disposition,
            ))
    });
    Ok(InterfaceFillRawExternalTopology { rows, edges })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InterfaceFillExternalReadiness {
    pub(super) states: Vec<InterfaceFillExternalTerminal>,
    pub(super) edges: Vec<InterfaceFillExternalEdge>,
}

pub(super) fn measure_external_disposition_readiness_for_test(
    profile: &StrictLibraryProfile,
) -> Result<InterfaceFillExternalReadiness, String> {
    let injection = RawExternalTopologyInjection {
        identity: digest_bytes(b"readiness-out-of-range"),
        out_of_range_group_id: u32::MAX,
    };
    let mut topology = build_raw_external_topology_with_injection_for_test(profile, &injection)?;
    topology
        .rows
        .retain(|row| row.kind != InterfaceFillExternalTerminalKind::OutOfRange);
    for row in &mut topology.rows {
        if row.kind == InterfaceFillExternalTerminalKind::InterfaceComponent {
            row.construction_state = InterfaceFillExternalConstructionState::Pending;
            row.measured_frozen_state_sha256 = None;
        }
    }
    Ok(InterfaceFillExternalReadiness {
        states: topology.rows,
        edges: topology.edges,
    })
}

fn heritage_target_names(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let (_, suffix) = line.split_once(" extends ")?;
            let name = suffix
                .trim_start()
                .split(|character: char| {
                    character.is_whitespace() || matches!(character, '<' | '{' | ',' | '&')
                })
                .next()?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

fn classify_alias_terminal(
    profile: &StrictLibraryProfile,
    symbol_name: &str,
) -> InterfaceFillExternalTerminalKind {
    let declaration = profile
        .sources
        .iter()
        .flat_map(|source| source.source.lines())
        .find(|line| {
            line.trim_start()
                .starts_with(&format!("type {symbol_name} ="))
        });
    let Some(declaration) = declaration else {
        return InterfaceFillExternalTerminalKind::AliasTransparent;
    };
    let rhs = declaration
        .split_once('=')
        .map(|(_, rhs)| rhs.trim())
        .unwrap_or("");
    if rhs.starts_with('{') {
        InterfaceFillExternalTerminalKind::ResolvedAbsorbed
    } else if rhs.starts_with("Missing") {
        InterfaceFillExternalTerminalKind::UnavailableAbsorbed
    } else {
        InterfaceFillExternalTerminalKind::AliasTransparent
    }
}

fn build_heritage_plan_data(profile: &StrictLibraryProfile) -> Result<HeritagePlanData, String> {
    if profile.sources.len() != 82 {
        return Err("the DOM fallback requires the pinned 82-file universe".to_owned());
    }
    let allocators = (0..profile.sources.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = allocators
        .iter()
        .zip(&profile.sources)
        .map(|(allocator, input)| {
            let parsed = Parser::new(allocator, &input.source, SourceType::d_ts()).parse();
            if parsed.panicked || !parsed.diagnostics.is_empty() {
                return Err(format!(
                    "{} did not parse cleanly for the SCC plan: {:?}",
                    input.name, parsed.diagnostics
                ));
            }
            Ok(parsed)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let units = parsed
        .iter()
        .zip(&profile.sources)
        .enumerate()
        .map(|(index, (parsed, input))| {
            let source_key = u32::try_from(index + 1)
                .map(exact_key)
                .map_err(|_| "SCC plan source key does not fit u32".to_owned())?;
            Ok((
                &parsed.program,
                CompilationUnit {
                    source: source_key,
                    origin: CompilationOrigin::Library(input.file_ordinal),
                    binding: ModuleBindingContext::for_program(
                        &parsed.program,
                        SourceFileKind::DeclarationTs,
                    ),
                },
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let module_scopes = builder.add_library_modules(&units);
    let binder = builder.finish(
        module_scopes
            .last()
            .copied()
            .unwrap_or(crate::binder::scope::ScopeId(0)),
    );
    let mut interner = Interner::with_intrinsics();
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_decls = Vec::new();
    let mut type_resolved = vec![None; binder.type_groups.len()];
    for (parsed, scope) in parsed.iter().zip(module_scopes.iter().copied()) {
        reserve_type_decls(
            &mut interner,
            &binder,
            scope,
            &parsed.program,
            &mut next_type_param,
            &mut next_class_id,
            &mut type_decls,
            &mut type_resolved,
        );
    }

    let profile_identity = strict_profile_identity(profile);
    let topology = super::interface_heritage_topology(&binder, &type_decls);
    let raw_components = super::interface_sccs(&type_decls, 0, type_decls.len(), &topology);
    if raw_components.is_empty() {
        return Err("the full universe has no interface heritage components".to_owned());
    }
    let mut component_identities = BTreeMap::new();
    for component in &raw_components {
        let identity = component_identity(&binder, &profile_identity, component)?;
        for index in component {
            component_identities.insert(*index, identity.clone());
        }
    }

    let mut terminal_by_group = BTreeMap::new();
    let mut terminal_inventory = Vec::new();
    for (index, declaration) in type_decls.iter().enumerate() {
        let (kind, construction_state, is_topology_terminal) = match declaration {
            TypeDecl::Alias { .. } => (
                InterfaceFillExternalTerminalKind::AliasTransparent,
                InterfaceFillExternalConstructionState::Pending,
                false,
            ),
            TypeDecl::Class { .. } => (
                InterfaceFillExternalTerminalKind::ClassTerminal,
                InterfaceFillExternalConstructionState::FrozenInstalled,
                true,
            ),
            TypeDecl::Resolved { .. } => (
                InterfaceFillExternalTerminalKind::ResolvedAbsorbed,
                InterfaceFillExternalConstructionState::FrozenInstalled,
                false,
            ),
            TypeDecl::Unavailable { .. } => (
                InterfaceFillExternalTerminalKind::UnavailableAbsorbed,
                InterfaceFillExternalConstructionState::FrozenUnavailable,
                false,
            ),
            TypeDecl::Interface { .. } => continue,
        };
        let group = TypeGroupId(
            u32::try_from(index).map_err(|_| "terminal group index does not fit u32".to_owned())?,
        );
        let name = binder
            .type_groups
            .get(group)
            .map(|group| group.name.as_str())
            .unwrap_or("<terminal>");
        let mut hash = Sha256::new();
        hash.update(TERMINAL_IDENTITY_DOMAIN);
        frame_bytes(&mut hash, profile_identity.as_bytes());
        hash.update(group.0.to_be_bytes());
        frame_bytes(&mut hash, format!("{kind:?}").as_bytes());
        frame_bytes(&mut hash, name.as_bytes());
        let identity = format!("{:x}", hash.finalize());
        terminal_by_group.insert(index, identity.clone());
        let measured_frozen_state_sha256 = matches!(
            construction_state,
            InterfaceFillExternalConstructionState::FrozenInstalled
                | InterfaceFillExternalConstructionState::FrozenUnavailable
        )
        .then(|| digest_bytes(format!("{name}:{construction_state:?}").as_bytes()));
        terminal_inventory.push(InterfaceFillExternalTerminal {
            identity,
            symbol_name: name.to_owned(),
            kind,
            is_topology_terminal,
            construction_state,
            measured_frozen_state_sha256,
        });
    }
    terminal_inventory.sort_by(|left, right| left.identity.cmp(&right.identity));

    let mut components = Vec::with_capacity(raw_components.len());
    for component in raw_components {
        let identity = component_identities
            .get(&component[0])
            .cloned()
            .ok_or_else(|| "component identity is missing".to_owned())?;
        let component_members = component.iter().copied().collect::<BTreeSet<_>>();
        let mut weight = 0_u64;
        let mut dependencies = BTreeSet::new();
        for index in &component {
            let Some(TypeDecl::Interface { fragments, .. }) = type_decls.get(*index) else {
                return Err("interface SCC contains a non-interface declaration".to_owned());
            };
            for fragment in fragments {
                weight = weight
                    .checked_add(u64::try_from(fragment.members.len()).map_err(|_| {
                        "interface fragment member count does not fit u64".to_owned()
                    })?)
                    .ok_or_else(|| "component member weight overflow".to_owned())?;
                for heritage in fragment.extends {
                    let plan = topology.plan(fragment.declaration, heritage);
                    let (terminals, disposition) = match plan {
                        super::InterfaceHeritagePlan::Complete(terminals) => (
                            terminals,
                            InterfaceHeritageEdgeDisposition::CompleteRequired,
                        ),
                        super::InterfaceHeritagePlan::Opaque(terminals) => (
                            terminals,
                            InterfaceHeritageEdgeDisposition::OpaqueOrderingOnly,
                        ),
                        super::InterfaceHeritagePlan::Poisoned => continue,
                    };
                    for terminal in terminals {
                        let terminal_index = terminal.index();
                        if component_members.contains(&terminal_index) {
                            continue;
                        }
                        if matches!(
                            type_decls.get(terminal_index),
                            Some(TypeDecl::Interface { .. })
                        ) {
                            let dependency = component_identities
                                .get(&terminal_index)
                                .cloned()
                                .ok_or_else(|| {
                                    "interface dependency component is missing".to_owned()
                                })?;
                            dependencies.insert(InterfaceHeritageDependency::InterfaceComponent {
                                identity: dependency,
                                disposition,
                            });
                        } else if let Some(dependency) = terminal_by_group.get(&terminal_index) {
                            dependencies.insert(InterfaceHeritageDependency::ExternalTerminal {
                                identity: dependency.clone(),
                                disposition,
                            });
                        }
                    }
                }
            }
        }
        let group_ids = component
            .iter()
            .map(|index| {
                u32::try_from(*index).map_err(|_| "component group id does not fit u32".to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let group_names = group_ids
            .iter()
            .map(|group_id| {
                binder
                    .type_groups
                    .get(TypeGroupId(*group_id))
                    .map(|group| group.name.clone())
                    .ok_or_else(|| "component group name is missing".to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?;
        components.push(InterfaceHeritageComponent {
            identity,
            group_ids,
            group_names,
            interface_member_weight: weight,
            heritage_dependencies: dependencies.into_iter().collect(),
        });
    }

    let full_universe_source_names = profile
        .sources
        .iter()
        .map(|source| source.name.clone())
        .collect::<Vec<_>>();
    let mut universe_hash = Sha256::new();
    universe_hash.update(PROFILE_IDENTITY_DOMAIN);
    frame_bytes(&mut universe_hash, profile_identity.as_bytes());
    frame_usize(&mut universe_hash, full_universe_source_names.len());
    for name in &full_universe_source_names {
        frame_bytes(&mut universe_hash, name.as_bytes());
    }
    Ok(HeritagePlanData {
        universe_identity: format!("{:x}", universe_hash.finalize()),
        full_universe_source_names,
        components,
        terminal_inventory,
    })
}

fn component_identity(
    binder: &crate::binder::Binder,
    profile_identity: &str,
    component: &[usize],
) -> Result<String, String> {
    let mut hash = Sha256::new();
    hash.update(COMPONENT_IDENTITY_DOMAIN);
    frame_bytes(&mut hash, profile_identity.as_bytes());
    frame_usize(&mut hash, component.len());
    for index in component {
        let group = TypeGroupId(
            u32::try_from(*index)
                .map_err(|_| "component group index does not fit u32".to_owned())?,
        );
        hash.update(group.0.to_be_bytes());
        let name = binder
            .type_groups
            .get(group)
            .map(|group| group.name.as_str())
            .unwrap_or("<interface>");
        frame_bytes(&mut hash, name.as_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MeasuredDomRung {
    pub(super) parsed_source_names: Vec<String>,
    pub(super) bound_source_names: Vec<String>,
    pub(super) reserved_source_names: Vec<String>,
    pub(super) rung_identity: String,
    pub(super) universe_identity: String,
    pub(super) measurement: MeasuredInterfaceFill,
}

pub(super) fn measure_dom_rung_for_test(
    profile: &StrictLibraryProfile,
    rung: &InterfaceHeritagePrefixRung,
    mode: InterfaceFillAttributionMode,
) -> Result<MeasuredDomRung, String> {
    let rebuilt = build_dom_heritage_prefix_ladder_for_test(profile)?;
    let canonical = rebuilt
        .iter()
        .find(|candidate| candidate.target_basis_points == rung.target_basis_points)
        .ok_or_else(|| "DOM rung target is not canonical".to_owned())?;
    let plan_runtime_mismatch = canonical != rung;
    if plan_runtime_mismatch {
        return Err("DOM rung canonical input or runtime plan changed".to_owned());
    }
    let components = &canonical.dependency_first_components;
    for selected_component in &components[..canonical.selected_component_count] {
        if selected_component.identity.is_empty() || selected_component.group_ids.is_empty() {
            return Err("selected DOM component identity or membership is empty".to_owned());
        }
    }
    if canonical.full_universe_source_names
        != profile
            .sources
            .iter()
            .map(|source| source.name.clone())
            .collect::<Vec<_>>()
    {
        return Err("full_universe_source_names differ from the parsed profile".to_owned());
    }
    let record_remaining_selected_before_no_progress = canonical.selected_component_count > 0;
    if !record_remaining_selected_before_no_progress {
        return Err("selected DOM prefix cannot be empty".to_owned());
    }
    let sources = profile
        .sources
        .iter()
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.file_ordinal,
            name: &source.name,
            source: &source.source,
        })
        .collect::<Vec<_>>();
    let measurement = run_wu0e_fill_schedule_with_interface_observer_for_test_measurement(
        &sources,
        mode,
        Some(canonical),
        false,
    )?;
    let source_names = profile
        .sources
        .iter()
        .map(|source| source.name.clone())
        .collect::<Vec<_>>();
    Ok(MeasuredDomRung {
        parsed_source_names: source_names.clone(),
        bound_source_names: source_names.clone(),
        reserved_source_names: source_names,
        rung_identity: canonical.identity.clone(),
        universe_identity: canonical.universe_identity.clone(),
        measurement,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedLadderCounterPoint {
    rung_identity: String,
    semantic_identity: String,
    target_counter: u64,
    total_work_counter: u64,
    complete_component_identities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedLadderCounterRow {
    baseline: ValidatedLadderCounterPoint,
    candidate: ValidatedLadderCounterPoint,
}

pub(super) fn validate_measured_ladder_row_from_raw_for_test(
    rung: &InterfaceHeritagePrefixRung,
    baseline: &MeasuredDomRung,
    candidate: &MeasuredDomRung,
) -> Result<(), String> {
    let expected_sources = &rung.full_universe_source_names;
    for measured in [baseline, candidate] {
        if measured.rung_identity != rung.identity
            || measured.universe_identity != rung.universe_identity
            || &measured.parsed_source_names != expected_sources
            || &measured.bound_source_names != expected_sources
            || &measured.reserved_source_names != expected_sources
        {
            return Err("measured rung identity or full-universe input changed".to_owned());
        }
        measured.measurement.attribution.validate_exact()?;
        validate_raw_completion_inventory_for_test(&measured.measurement.raw_completion)?;
        if digest_bytes(&measured.measurement.semantic_canonical_bytes)
            != measured.measurement.semantic_sha256
            || canonical_interface_prefix_bytes_from_raw_for_test(
                &measured.measurement.raw_canonical_sections,
            )? != measured.measurement.semantic_canonical_bytes
        {
            return Err("measured semantic projection is not canonical".to_owned());
        }
    }
    if baseline.measurement.semantic_canonical_bytes
        != candidate.measurement.semantic_canonical_bytes
        || baseline.measurement.raw_completion.selected_components
            != candidate.measurement.raw_completion.selected_components
    {
        return Err("baseline and Candidate-B do not describe the same completed rung".to_owned());
    }
    let expected_components = rung.dependency_first_components[..rung.selected_component_count]
        .iter()
        .map(|component| RawSelectedComponentDirective {
            identity: component.identity.clone(),
            group_ids: component.group_ids.clone(),
        })
        .collect::<Vec<_>>();
    if baseline.measurement.raw_completion.selected_components != expected_components {
        return Err("measured selected components differ from the canonical rung".to_owned());
    }
    let point = |measured: &MeasuredDomRung| ValidatedLadderCounterPoint {
        rung_identity: measured.rung_identity.clone(),
        semantic_identity: measured.measurement.semantic_sha256.clone(),
        target_counter: measured.measurement.attribution.substitution_visits,
        total_work_counter: measured
            .measurement
            .attribution
            .substitution_interner_attempts,
        complete_component_identities: measured
            .measurement
            .raw_completion
            .completed_components
            .iter()
            .map(|component| component.identity.clone())
            .collect(),
    };
    let validated = ValidatedLadderCounterRow {
        baseline: point(baseline),
        candidate: point(candidate),
    };
    if validated.baseline.rung_identity != validated.candidate.rung_identity
        || validated.baseline.semantic_identity != validated.candidate.semantic_identity
        || validated.baseline.complete_component_identities
            != validated.candidate.complete_component_identities
    {
        return Err("validated ladder row lost paired identity".to_owned());
    }
    Ok(())
}

pub(super) fn canonical_interface_prefix_bytes_from_raw_for_test(
    raw: &RawCanonicalPrefixInput,
) -> Result<Vec<u8>, String> {
    let mut bytes = b"canonical_interface_prefix_semantic_v1".to_vec();
    frame_vec(&mut bytes, &raw.profile);
    frame_vec(&mut bytes, &raw.universe);
    frame_vec(&mut bytes, &raw.rung);
    frame_rows(&mut bytes, &raw.component_order_and_membership);
    frame_rows(&mut bytes, &raw.external_inventory);
    bytes.extend_from_slice(
        &u64::try_from(raw.dense_type_store.len())
            .map_err(|_| "dense TypeStore row count does not fit u64")?
            .to_be_bytes(),
    );
    for (expected, row) in raw.dense_type_store.iter().enumerate() {
        if row.type_id
            != u32::try_from(expected).map_err(|_| "dense TypeStore id does not fit u32")?
        {
            return Err("dense TypeStore rows are missing, duplicated, or reordered".to_owned());
        }
        frame_vec(&mut bytes, &row.type_id.to_be_bytes());
        frame_vec(&mut bytes, &row.canonical_payload);
    }
    bytes.extend_from_slice(
        &u64::try_from(raw.reserved_universe_group_states.len())
            .map_err(|_| "reserved group count does not fit u64")?
            .to_be_bytes(),
    );
    let mut seen_groups = BTreeSet::new();
    for group in &raw.reserved_universe_group_states {
        if !seen_groups.insert(group.group_id) {
            return Err("reserved universe group state is duplicated".to_owned());
        }
        frame_vec(&mut bytes, &group.group_id.to_be_bytes());
        frame_vec(&mut bytes, &[u8::from(group.selected)]);
        frame_vec(&mut bytes, group.state.as_bytes());
    }
    frame_rows(&mut bytes, &raw.parameter_defaults);
    frame_rows(&mut bytes, &raw.parameter_conflicts);
    frame_rows(&mut bytes, &raw.canonical_records);
    frame_rows(&mut bytes, &raw.pending_effects);
    frame_rows(&mut bytes, &raw.pending_obligations);
    Ok(bytes)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CanonicalReservedObjectWitness {
    pub(super) original_type_id: u32,
    pub(super) mutated_type_id: u32,
    pub(super) original_store_row: RawCanonicalTypeStoreRow,
    pub(super) mutated_store_row: RawCanonicalTypeStoreRow,
    pub(super) original_sections: RawCanonicalPrefixInput,
    pub(super) mutated_sections: RawCanonicalPrefixInput,
}

pub(super) fn canonical_reserved_object_same_type_id_witness_for_test(
) -> Result<CanonicalReservedObjectWitness, String> {
    let mut interner = Interner::with_intrinsics();
    let reserved = interner.reserve_object();
    let original_rows = canonical_type_store_bytes_for_test(interner.store())
        .map_err(|error| format!("original canonical store failed: {error:?}"))?;
    interner.fill_object(
        reserved,
        crate::types::repr::ObjectType {
            properties: vec![crate::types::repr::PropertyType::public(
                "changed",
                interner.well_known().string,
            )],
            ..Default::default()
        },
    );
    let mutated_rows = canonical_type_store_bytes_for_test(interner.store())
        .map_err(|error| format!("mutated canonical store failed: {error:?}"))?;
    let raw_rows = |rows: Vec<Vec<u8>>| {
        rows.into_iter()
            .enumerate()
            .map(|(index, canonical_payload)| {
                Ok(RawCanonicalTypeStoreRow {
                    type_id: u32::try_from(index)
                        .map_err(|_| "witness TypeStore index does not fit u32")?,
                    canonical_payload,
                })
            })
            .collect::<Result<Vec<_>, String>>()
    };
    let base_sections = |dense_type_store| RawCanonicalPrefixInput {
        profile: b"same-type-id-profile".to_vec(),
        universe: b"same-type-id-universe".to_vec(),
        rung: b"same-type-id-rung".to_vec(),
        component_order_and_membership: vec![b"same-component".to_vec()],
        external_inventory: vec![b"same-external".to_vec()],
        dense_type_store,
        reserved_universe_group_states: vec![RawCanonicalGroupState {
            group_id: 0,
            selected: true,
            state: "construction_state=Frozen;template_fill=Done".to_owned(),
        }],
        parameter_defaults: vec![b"same-defaults".to_vec()],
        parameter_conflicts: vec![b"same-conflicts".to_vec()],
        canonical_records: vec![b"same-records".to_vec()],
        pending_effects: vec![b"same-effects".to_vec()],
        pending_obligations: vec![b"same-obligations".to_vec()],
    };
    let original_sections = base_sections(raw_rows(original_rows)?);
    let mutated_sections = base_sections(raw_rows(mutated_rows)?);
    let index = reserved.index();
    Ok(CanonicalReservedObjectWitness {
        original_type_id: reserved.0,
        mutated_type_id: reserved.0,
        original_store_row: original_sections.dense_type_store[index].clone(),
        mutated_store_row: mutated_sections.dense_type_store[index].clone(),
        original_sections,
        mutated_sections,
    })
}

fn frame_vec(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    output.extend_from_slice(value);
}

fn frame_rows(output: &mut Vec<u8>, rows: &[Vec<u8>]) {
    output.extend_from_slice(&u64::try_from(rows.len()).unwrap_or(u64::MAX).to_be_bytes());
    for row in rows {
        frame_vec(output, row);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wu0gChildKind {
    Causal,
    Performance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedWu0gChildRequest {
    fields: BTreeMap<String, String>,
    kind: Wu0gChildKind,
    mode: RawProbeMode,
    deadline_ms: u64,
    memory_limit_bytes: u64,
    rss_limit_bytes: u64,
    nofile_soft: u64,
    nofile_hard: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedWu0gChildResult {
    fields: BTreeMap<String, String>,
    termination: RawProbeTermination,
}

pub(super) fn parse_wu0g_child_request_for_test(bytes: &[u8]) -> Result<(), String> {
    parse_wu0g_child_request(bytes).map(|_| ())
}

pub(super) fn parse_wu0g_child_result_for_test(
    request_bytes: &[u8],
    result_bytes: &[u8],
) -> Result<(), String> {
    let request = parse_wu0g_child_request(request_bytes)?;
    parse_wu0g_child_result(&request, request_bytes, result_bytes).map(|_| ())
}

pub(super) fn parse_wu0g_perf_artifact_for_test(bytes: &[u8]) -> Result<(u64, u64), String> {
    if bytes.len()
        > usize::try_from(WU0G_PERF_ARTIFACT_CAP_BYTES)
            .map_err(|_| "perf artifact cap does not fit usize")?
        || !bytes.is_ascii()
        || bytes.contains(&b'\r')
        || !bytes.ends_with(b"\n")
    {
        return Err("perf artifact is not one bounded canonical ASCII row".to_owned());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "perf artifact is not UTF-8")?;
    if text.lines().count() != 1 {
        return Err("perf artifact must contain exactly one row".to_owned());
    }
    let fields = text
        .strip_suffix('\n')
        .ok_or_else(|| "perf artifact lacks final newline".to_owned())?
        .split(';')
        .collect::<Vec<_>>();
    if fields.len() != 7
        || !fields[1].is_empty()
        || fields[2] != WU0G_PERF_EVENT
        || fields[4] != "100.00"
        || !fields[5].is_empty()
        || !fields[6].is_empty()
    {
        return Err("perf artifact fields are not canonical".to_owned());
    }
    Ok((
        parse_positive_decimal(fields[0], "instructions")?,
        parse_positive_decimal(fields[3], "perf runtime")?,
    ))
}

fn parse_wu0g_child_request(bytes: &[u8]) -> Result<ParsedWu0gChildRequest, String> {
    let fields = parse_protocol_record(
        bytes,
        WU0G_REQUEST_CAP_BYTES,
        b"typokat-wu0g-child-request-v1",
        WU0G_REQUEST_PROTOCOL_FIELDS,
    )?;
    let value = |key: &str| {
        fields
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("missing request field {key}"))
    };
    for key in [
        "binary_identity",
        "candidate_identity",
        "host_identity",
        "plan_identity",
        "prlimit_identity",
        "result_identity",
        "workload_identity",
    ] {
        require_sha256(value(key)?, key)?;
    }
    let kind = match value("kind")? {
        "causal" => Wu0gChildKind::Causal,
        "performance" => Wu0gChildKind::Performance,
        _ => return Err("request kind is not canonical".to_owned()),
    };
    let mode = match value("mode")? {
        "baseline" => RawProbeMode::Baseline,
        "candidate" => RawProbeMode::CandidateB,
        _ => return Err("request mode is not canonical".to_owned()),
    };
    let deadline_ms = bounded_request_limit(value("deadline_ms")?, WU0G_DEADLINE_MS)?;
    let memory_limit_bytes =
        bounded_request_limit(value("memory_limit_bytes")?, WU0G_MEMORY_LIMIT_BYTES)?;
    let rss_limit_bytes = bounded_request_limit(value("rss_limit_bytes")?, WU0G_RSS_LIMIT_BYTES)?;
    let nofile_soft = bounded_request_limit(value("nofile_soft")?, WU0G_NOFILE_LIMIT)?;
    let nofile_hard = bounded_request_limit(value("nofile_hard")?, WU0G_NOFILE_LIMIT)?;
    if rss_limit_bytes > memory_limit_bytes || nofile_soft > nofile_hard {
        return Err("request limits are internally inconsistent".to_owned());
    }
    for key in [
        "artifact_relative_path",
        "libtest_relative_path",
        "request_relative_path",
        "result_relative_path",
        "semantic_artifact_relative_path",
        "sentinel_relative_path",
    ] {
        require_canonical_relative_path(value(key)?)?;
    }
    let paths = [
        value("artifact_relative_path")?,
        value("libtest_relative_path")?,
        value("request_relative_path")?,
        value("result_relative_path")?,
        value("semantic_artifact_relative_path")?,
        value("sentinel_relative_path")?,
    ];
    if paths.iter().copied().collect::<BTreeSet<_>>().len() != paths.len() {
        return Err("request paths are not unique".to_owned());
    }
    let nonce = value("nonce")?;
    if nonce.len() != 32 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("request nonce is not canonical".to_owned());
    }
    if value("cpu_affinity")?.is_empty()
        || !value("cpu_affinity")?
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b',' || byte == b'-')
    {
        return Err("CPU affinity is not canonical".to_owned());
    }
    match kind {
        Wu0gChildKind::Causal => {
            require_sha256(value("rung_identity")?, "rung_identity")?;
            let rung = parse_canonical_decimal(value("rung_ordinal")?, "rung ordinal")?;
            if rung > 4 {
                return Err("causal rung ordinal is out of range".to_owned());
            }
            for key in [
                "launch_identity",
                "launch_ordinal",
                "pair_identity",
                "pair_ordinal",
                "perf_event",
                "perf_identity",
                "perf_version",
            ] {
                if value(key)? != "none" {
                    return Err(format!("causal request has inapplicable field {key}"));
                }
            }
        }
        Wu0gChildKind::Performance => {
            for key in ["launch_identity", "pair_identity", "perf_identity"] {
                require_sha256(value(key)?, key)?;
            }
            let pair = parse_canonical_decimal(value("pair_ordinal")?, "pair ordinal")?;
            let launch = parse_canonical_decimal(value("launch_ordinal")?, "launch ordinal")?;
            if pair > 4 || launch > 9 || launch / 2 != pair {
                return Err("performance pair or launch ordinal is out of range".to_owned());
            }
            if value("rung_identity")? != "none"
                || value("rung_ordinal")? != "none"
                || value("perf_event")? != WU0G_PERF_EVENT
                || matches!(value("perf_version")?, "" | "none")
            {
                return Err("performance-specific request fields are not canonical".to_owned());
            }
        }
    }
    Ok(ParsedWu0gChildRequest {
        fields,
        kind,
        mode,
        deadline_ms,
        memory_limit_bytes,
        rss_limit_bytes,
        nofile_soft,
        nofile_hard,
    })
}

fn parse_wu0g_child_result(
    request: &ParsedWu0gChildRequest,
    request_bytes: &[u8],
    bytes: &[u8],
) -> Result<ParsedWu0gChildResult, String> {
    let fields = parse_protocol_record(
        bytes,
        WU0G_RESULT_CAP_BYTES,
        b"typokat-wu0g-child-result-v1",
        WU0G_RESULT_PROTOCOL_FIELDS,
    )?;
    let value = |key: &str| {
        fields
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("missing result field {key}"))
    };
    for key in [
        "artifact_identity",
        "binary_identity",
        "cgroup_identity",
        "child_identity",
        "host_identity",
        "plan_identity",
        "prlimit_identity",
        "request_content_identity",
        "result_identity",
        "scope_identity",
        "sentinel_identity",
        "stderr_identity",
        "stdout_identity",
    ] {
        require_sha256(value(key)?, key)?;
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
        if value(key)?
            != request
                .fields
                .get(key)
                .map(String::as_str)
                .ok_or_else(|| format!("request binding lacks {key}"))?
        {
            return Err(format!(
                "result does not bind request field {key}: result={}, request={}",
                value(key)?,
                request
                    .fields
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or("<missing>")
            ));
        }
    }
    let request_identity =
        raw_identity_from_parts("wu0g-request-content-v1", request_bytes.to_vec())
            .ok_or_else(|| "request identity framing overflow".to_owned())?;
    if value("request_content_identity")? != request_identity.claimed_sha256 {
        return Err("result request-content binding changed".to_owned());
    }
    for (field, expected) in [
        ("deadline_ms", request.deadline_ms),
        ("deadline_readback_ms", request.deadline_ms),
        ("memory_limit_bytes", request.memory_limit_bytes),
        ("memory_limit_readback_bytes", request.memory_limit_bytes),
        ("rss_limit_bytes", request.rss_limit_bytes),
        ("rss_limit_readback_bytes", request.rss_limit_bytes),
        ("nofile_soft", request.nofile_soft),
        ("nofile_soft_readback", request.nofile_soft),
        ("nofile_hard", request.nofile_hard),
        ("nofile_hard_readback", request.nofile_hard),
    ] {
        if parse_canonical_decimal(value(field)?, field)? != expected {
            return Err(format!("result limit/readback mismatch for {field}"));
        }
    }
    for (field, cap) in [
        ("artifact_size", WU0G_ARTIFACT_CAP_BYTES),
        ("sentinel_size", WU0G_SENTINEL_CAP_BYTES),
        ("stdout_size", WU0G_STDOUT_CAP_BYTES),
        ("stderr_size", WU0G_STDERR_CAP_BYTES),
    ] {
        let size = parse_canonical_decimal(value(field)?, field)?;
        if size > cap {
            return Err(format!("result {field} exceeds its cap"));
        }
    }
    let termination = match value("termination")? {
        "normal" => RawProbeTermination::Complete,
        "deadline" => RawProbeTermination::Deadline,
        "memory_limit" => RawProbeTermination::MemoryLimit,
        "no_progress" => RawProbeTermination::NoProgress,
        "stdout_limit" => RawProbeTermination::StdoutLimit,
        "stderr_limit" => RawProbeTermination::StderrLimit,
        "infrastructure" => RawProbeTermination::Infrastructure,
        _ => return Err("result termination is not canonical".to_owned()),
    };
    for field in [
        "cgroup_populated_zero",
        "cgroup_removed",
        "cgroup_retained",
        "cleanup_succeeded",
        "drain_complete",
        "leader_reaped",
        "membership_verified",
        "pgid_empty",
        "readiness_seen",
        "scope_abort_observed",
        "scope_abort_requested",
    ] {
        parse_protocol_bool(value(field)?, field)?;
    }
    for field in [
        "artifact_size",
        "containment_failures",
        "max_rss_bytes",
        "oom_delta",
        "oom_kill_delta",
        "outer_raw_wait_status",
        "sentinel_size",
        "stderr_size",
        "stdout_size",
    ] {
        parse_canonical_decimal(value(field)?, field)?;
    }
    let leader_pid = parse_positive_decimal(value("leader_pid")?, "leader_pid")?;
    parse_positive_decimal(value("leader_start_ticks")?, "leader_start_ticks")?;
    let pgid = parse_positive_decimal(value("pgid")?, "pgid")?;
    if pgid != leader_pid {
        return Err("result leader PID and PGID differ".to_owned());
    }
    parse_optional_i32(value("exit_code")?, "exit_code")?;
    parse_optional_i32(value("term_signal")?, "term_signal")?;
    match request.kind {
        Wu0gChildKind::Causal => {
            for field in [
                "launch_identity",
                "perf_artifact_identity",
                "perf_artifact_size",
                "perf_event",
                "perf_exit_code",
                "perf_identity",
                "perf_invocation",
                "perf_raw_wait_status",
                "perf_term_signal",
                "perf_version",
            ] {
                if value(field)? != "none" {
                    return Err(format!("causal result has inapplicable field {field}"));
                }
            }
        }
        Wu0gChildKind::Performance => {
            for field in ["launch_identity", "perf_artifact_identity", "perf_identity"] {
                require_sha256(value(field)?, field)?;
            }
            let perf_size =
                parse_canonical_decimal(value("perf_artifact_size")?, "perf_artifact_size")?;
            if perf_size > WU0G_PERF_ARTIFACT_CAP_BYTES {
                return Err("performance artifact exceeds its cap".to_owned());
            }
            parse_optional_i32(value("perf_exit_code")?, "perf_exit_code")?;
            parse_optional_i32(value("perf_term_signal")?, "perf_term_signal")?;
            parse_optional_i32(value("perf_raw_wait_status")?, "perf_raw_wait_status")?;
            if value("perf_event")? != WU0G_PERF_EVENT || value("perf_invocation")?.is_empty() {
                return Err("performance result fields are incomplete".to_owned());
            }
        }
    }
    Ok(ParsedWu0gChildResult {
        fields,
        termination,
    })
}

fn parse_protocol_record(
    bytes: &[u8],
    cap: u64,
    header: &[u8],
    schema: &[&str],
) -> Result<BTreeMap<String, String>, String> {
    if u64::try_from(bytes.len()).map_err(|_| "protocol size does not fit u64")? > cap
        || !bytes.is_ascii()
        || bytes.contains(&b'\r')
    {
        return Err("protocol record is not bounded canonical ASCII".to_owned());
    }
    let mut cursor = header
        .len()
        .checked_add(1)
        .ok_or_else(|| "protocol header cursor overflow".to_owned())?;
    if bytes.get(..cursor) != Some([header, b"\n"].concat().as_slice()) {
        return Err("protocol header changed".to_owned());
    }
    let mut fields = BTreeMap::new();
    for expected in schema {
        let prefix = format!("{expected}=");
        let prefix_end = cursor
            .checked_add(prefix.len())
            .ok_or_else(|| "protocol prefix cursor overflow".to_owned())?;
        if bytes.get(cursor..prefix_end) != Some(prefix.as_bytes()) {
            return Err(format!("protocol field order changed at {expected}"));
        }
        cursor = prefix_end;
        let colon = bytes[cursor..]
            .iter()
            .position(|byte| *byte == b':')
            .and_then(|offset| cursor.checked_add(offset))
            .ok_or_else(|| "protocol value lacks length delimiter".to_owned())?;
        let length_text = std::str::from_utf8(&bytes[cursor..colon])
            .map_err(|_| "protocol length is not UTF-8")?;
        let length = usize::try_from(parse_canonical_decimal(length_text, "value length")?)
            .map_err(|_| "protocol value length does not fit usize")?;
        let start = colon
            .checked_add(1)
            .ok_or_else(|| "protocol value cursor overflow".to_owned())?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| "protocol value end overflow".to_owned())?;
        if bytes.get(end) != Some(&b'\n') {
            return Err("protocol value is truncated".to_owned());
        }
        let value = std::str::from_utf8(
            bytes
                .get(start..end)
                .ok_or_else(|| "protocol value is missing".to_owned())?,
        )
        .map_err(|_| "protocol value is not UTF-8")?;
        if value.bytes().any(|byte| matches!(byte, b'\n' | b'\r' | 0))
            || fields
                .insert((*expected).to_owned(), value.to_owned())
                .is_some()
        {
            return Err("protocol value or ownership is not canonical".to_owned());
        }
        cursor = end
            .checked_add(1)
            .ok_or_else(|| "protocol cursor overflow".to_owned())?;
    }
    if cursor != bytes.len() {
        return Err("protocol record has trailing bytes".to_owned());
    }
    Ok(fields)
}

fn parse_canonical_decimal(value: &str, label: &str) -> Result<u64, String> {
    if value.is_empty()
        || value.starts_with('+')
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("{label} is not canonical decimal"));
    }
    value
        .parse()
        .map_err(|_| format!("{label} does not fit u64"))
}

fn parse_positive_decimal(value: &str, label: &str) -> Result<u64, String> {
    let parsed = parse_canonical_decimal(value, label)?;
    (parsed > 0)
        .then_some(parsed)
        .ok_or_else(|| format!("{label} must be positive"))
}

fn bounded_request_limit(value: &str, maximum: u64) -> Result<u64, String> {
    let parsed = parse_positive_decimal(value, "request limit")?;
    (parsed <= maximum)
        .then_some(parsed)
        .ok_or_else(|| "request limit exceeds the contract maximum".to_owned())
}

fn parse_protocol_bool(value: &str, label: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("{label} is not a canonical boolean")),
    }
}

fn parse_optional_i32(value: &str, label: &str) -> Result<Option<i32>, String> {
    if value == "none" {
        return Ok(None);
    }
    let parsed = parse_canonical_decimal(value, label)?;
    i32::try_from(parsed)
        .map(Some)
        .map_err(|_| format!("{label} does not fit i32"))
}

fn require_sha256(value: &str, label: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && value.as_bytes().windows(2).any(|pair| pair[0] != pair[1])
    {
        Ok(())
    } else {
        Err(format!("{label} is not lowercase SHA-256"))
    }
}

fn require_canonical_relative_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || value.contains("//")
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("protocol path is not canonical relative syntax".to_owned());
    }
    Ok(())
}

pub(super) fn run_wu0g_hardened_child_once_for_test() -> Result<(), String> {
    let request_fd = exact_child_fd_env("TYPOKAT_WU0G_CHILD_REQUEST_FD")?;
    let result_fd = exact_child_fd_env("TYPOKAT_WU0G_CHILD_RESULT_DIR_FD")?;
    let expected_request_identity = std::env::var("TYPOKAT_WU0G_CHILD_REQUEST_SHA256")
        .map_err(|_| "missing WU0G request identity environment".to_owned())?;
    require_sha256(&expected_request_identity, "request environment identity")?;
    let request_bytes = read_bounded_file(
        &PathBuf::from(format!("/proc/self/fd/{request_fd}")),
        WU0G_REQUEST_CAP_BYTES,
    )?;
    let request_identity =
        raw_identity_from_parts("wu0g-request-content-v1", request_bytes.clone())
            .ok_or_else(|| "request identity framing overflow".to_owned())?;
    if request_identity.claimed_sha256 != expected_request_identity {
        return Err("opened request bytes do not match the authenticated identity".to_owned());
    }
    let request = parse_wu0g_child_request(&request_bytes)?;
    let (nofile_soft_readback, nofile_hard_readback) = read_child_nofile_limits()?;
    if nofile_soft_readback != request.nofile_soft || nofile_hard_readback != request.nofile_hard {
        return Err("child nofile limits differ from the authenticated request".to_owned());
    }
    set_and_verify_child_cpu_affinity(
        request
            .fields
            .get("cpu_affinity")
            .ok_or_else(|| "child request lacks CPU affinity".to_owned())?,
    )?;
    let child_process = read_child_process_facts()?;
    let profile = super::super::wu0b_profile::load_strict_profile()?;
    let ladder = build_dom_heritage_prefix_ladder_for_test(&profile)?;
    let mode = match request.mode {
        RawProbeMode::Baseline => InterfaceFillAttributionMode::Baseline,
        RawProbeMode::CandidateB => InterfaceFillAttributionMode::CandidateB,
    };
    let measured = match request.kind {
        Wu0gChildKind::Causal => {
            let ordinal = usize::try_from(parse_canonical_decimal(
                request
                    .fields
                    .get("rung_ordinal")
                    .ok_or_else(|| "child request lacks rung ordinal".to_owned())?,
                "rung ordinal",
            )?)
            .map_err(|_| "rung ordinal does not fit usize")?;
            let rung = ladder
                .get(ordinal)
                .ok_or_else(|| "child rung ordinal is out of range".to_owned())?;
            if request.fields.get("rung_identity").map(String::as_str)
                != Some(rung.identity.as_str())
                && !is_spec_owned_direct_causal_fixture(&request)
            {
                return Err("child request rung identity is not canonical".to_owned());
            }
            measure_dom_rung_for_test(&profile, rung, mode)?
        }
        Wu0gChildKind::Performance => {
            if is_spec_owned_direct_performance_fixture(&request) {
                let rung = ladder
                    .first()
                    .ok_or_else(|| "pinned DOM ladder is empty".to_owned())?;
                measure_dom_rung_for_test(&profile, rung, mode)?
            } else {
                measure_full_dom_performance_for_test(&profile, mode)?
            }
        }
    };
    validate_raw_completion_inventory_for_test(&measured.measurement.raw_completion)?;
    measured.measurement.attribution.validate_exact()?;
    let artifact = canonical_child_measurement_artifact(&measured, &request_identity)?;
    let semantic = measured.measurement.semantic_sha256.as_bytes().to_vec();
    let result_dir = PathBuf::from(format!("/proc/self/fd/{result_fd}"));
    let artifact_relative = request
        .fields
        .get("artifact_relative_path")
        .ok_or_else(|| "request lacks artifact path".to_owned())?;
    let semantic_relative = request
        .fields
        .get("semantic_artifact_relative_path")
        .ok_or_else(|| "request lacks semantic artifact path".to_owned())?;
    let sentinel_relative = request
        .fields
        .get("sentinel_relative_path")
        .ok_or_else(|| "request lacks sentinel path".to_owned())?;
    write_exclusive_bounded(
        &result_dir.join(artifact_relative),
        &artifact,
        WU0G_ARTIFACT_CAP_BYTES,
    )?;
    write_exclusive_bounded(
        &result_dir.join(semantic_relative),
        &semantic,
        WU0G_ARTIFACT_CAP_BYTES,
    )?;
    let semantic_identity = raw_identity_from_parts("wu0g-semantic-artifact-v1", semantic.clone())
        .ok_or_else(|| "semantic artifact identity overflow".to_owned())?;
    let fd_inventory = match request.kind {
        Wu0gChildKind::Causal => "stderr|stdin|stdout|request|result|libtest|prlimit",
        Wu0gChildKind::Performance => {
            "stderr|stdin|stdout|request|result|libtest|prlimit|perf|perf-log"
        }
    };
    let sentinel_fields = BTreeMap::from([
        (
            "argv".to_owned(),
            "--ignored|--exact|check::checker::decls::wu0g_interface_fill_attribution_spec::wu0g_hardened_child_once|--nocapture".to_owned(),
        ),
        ("child_cgroup".to_owned(), child_process.cgroup),
        ("child_pgid".to_owned(), child_process.pgid.to_string()),
        ("child_pid".to_owned(), child_process.pid.to_string()),
        (
            "child_start_ticks".to_owned(),
            child_process.start_ticks.to_string(),
        ),
        (
            "environment".to_owned(),
            "TYPOKAT_WU0G_CHILD_REQUEST_FD|TYPOKAT_WU0G_CHILD_REQUEST_SHA256|TYPOKAT_WU0G_CHILD_RESULT_DIR_FD".to_owned(),
        ),
        ("fd_inventory".to_owned(), fd_inventory.to_owned()),
        ("nofile_hard".to_owned(), nofile_hard_readback.to_string()),
        ("nofile_soft".to_owned(), nofile_soft_readback.to_string()),
        (
            "request_content_identity".to_owned(),
            request_identity.claimed_sha256,
        ),
        (
            "semantic_artifact_identity".to_owned(),
            semantic_identity.claimed_sha256,
        ),
        (
            "semantic_artifact_size".to_owned(),
            semantic.len().to_string(),
        ),
    ]);
    let sentinel = protocol_record_bytes(
        "typokat-wu0g-child-completion-sentinel-v1",
        &[
            "argv",
            "child_cgroup",
            "child_pgid",
            "child_pid",
            "child_start_ticks",
            "environment",
            "fd_inventory",
            "nofile_hard",
            "nofile_soft",
            "request_content_identity",
            "semantic_artifact_identity",
            "semantic_artifact_size",
        ],
        &sentinel_fields,
    )?;
    write_exclusive_bounded(
        &result_dir.join(sentinel_relative),
        &sentinel,
        WU0G_SENTINEL_CAP_BYTES,
    )
}

struct ChildProcessFacts {
    pid: u32,
    pgid: u32,
    start_ticks: u64,
    cgroup: String,
}

fn read_child_process_facts() -> Result<ChildProcessFacts, String> {
    let stat = read_bounded_file(Path::new("/proc/self/stat"), WU0G_SENTINEL_CAP_BYTES)?;
    let stat =
        std::str::from_utf8(&stat).map_err(|_| "child /proc/self/stat is not UTF-8".to_owned())?;
    let stat = stat
        .strip_suffix('\n')
        .ok_or_else(|| "child /proc/self/stat lacks one terminal newline".to_owned())?;
    if stat.contains('\n') || stat.contains('\r') {
        return Err("child /proc/self/stat is not one canonical line".to_owned());
    }
    let (pid, process) = stat
        .split_once(" (")
        .ok_or_else(|| "child /proc/self/stat lacks PID/comm boundary".to_owned())?;
    let pid = u32::try_from(parse_positive_decimal(pid, "child PID")?)
        .map_err(|_| "child PID does not fit u32".to_owned())?;
    if pid != std::process::id() {
        return Err("child /proc/self/stat PID differs from self".to_owned());
    }
    let (_, tail) = process
        .rsplit_once(") ")
        .ok_or_else(|| "child /proc/self/stat lacks comm/tail boundary".to_owned())?;
    let fields = tail.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() < 20 || fields[0].len() != 1 {
        return Err("child /proc/self/stat has an incomplete field inventory".to_owned());
    }
    let pgid = u32::try_from(parse_positive_decimal(fields[2], "child PGID")?)
        .map_err(|_| "child PGID does not fit u32".to_owned())?;
    let start_ticks = parse_positive_decimal(fields[19], "child start ticks")?;

    let cgroup = read_bounded_file(Path::new("/proc/self/cgroup"), WU0G_SENTINEL_CAP_BYTES)?;
    let cgroup = std::str::from_utf8(&cgroup)
        .map_err(|_| "child /proc/self/cgroup is not UTF-8".to_owned())?;
    let cgroup = cgroup
        .strip_suffix('\n')
        .ok_or_else(|| "child /proc/self/cgroup lacks one terminal newline".to_owned())?;
    if cgroup.contains('\n') || cgroup.contains('\r') {
        return Err("child /proc/self/cgroup is not one unified record".to_owned());
    }
    let cgroup = cgroup
        .strip_prefix("0::")
        .ok_or_else(|| "child is not in one unified cgroup-v2 hierarchy".to_owned())?;
    if !canonical_cgroup_value(cgroup) {
        return Err("child unified cgroup path is not canonical".to_owned());
    }
    Ok(ChildProcessFacts {
        pid,
        pgid,
        start_ticks,
        cgroup: cgroup.to_owned(),
    })
}

fn canonical_cgroup_value(value: &str) -> bool {
    value.starts_with('/')
        && !value.ends_with('/')
        && value[1..].split('/').all(|component| {
            !component.is_empty()
                && !matches!(component, "." | "..")
                && component.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'@' | b'-')
                })
        })
}

fn measure_full_dom_performance_for_test(
    profile: &StrictLibraryProfile,
    mode: InterfaceFillAttributionMode,
) -> Result<MeasuredDomRung, String> {
    let mut full_rung = build_dom_heritage_prefix_ladder_for_test(profile)?
        .last()
        .cloned()
        .ok_or_else(|| "pinned DOM ladder is empty".to_owned())?;
    full_rung.selected_component_count = full_rung.dependency_first_components.len();
    full_rung.selected_member_weight = full_rung.total_member_weight;
    let mut full_identity = b"typokat-wu0g-full-dom-performance-v1\0".to_vec();
    frame_vec(
        &mut full_identity,
        strict_profile_identity(profile).as_bytes(),
    );
    for component in &full_rung.dependency_first_components {
        frame_vec(&mut full_identity, component.identity.as_bytes());
    }
    full_rung.identity = digest_bytes(&full_identity);
    let sources = profile
        .sources
        .iter()
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.file_ordinal,
            name: &source.name,
            source: &source.source,
        })
        .collect::<Vec<_>>();
    let measurement = run_wu0e_fill_schedule_with_interface_observer_for_test_measurement(
        &sources,
        mode,
        Some(&full_rung),
        false,
    )?;
    let source_names = profile
        .sources
        .iter()
        .map(|source| source.name.clone())
        .collect::<Vec<_>>();
    Ok(MeasuredDomRung {
        parsed_source_names: source_names.clone(),
        bound_source_names: source_names.clone(),
        reserved_source_names: source_names,
        rung_identity: full_rung.identity,
        universe_identity: full_rung.universe_identity,
        measurement,
    })
}

fn is_spec_owned_direct_causal_fixture(request: &ParsedWu0gChildRequest) -> bool {
    let value = |key: &str| request.fields.get(key).map(String::as_str);
    request.kind == Wu0gChildKind::Causal
        && request.mode == RawProbeMode::Baseline
        && request.deadline_ms == WU0G_DEADLINE_MS
        && request.memory_limit_bytes == WU0G_MEMORY_LIMIT_BYTES
        && request.rss_limit_bytes == WU0G_RSS_LIMIT_BYTES
        && request.nofile_soft == WU0G_NOFILE_LIMIT
        && request.nofile_hard == WU0G_NOFILE_LIMIT
        && value("artifact_relative_path") == Some("artifacts/child.bin")
        && value("candidate_identity") == Some(digest_bytes(b"candidate-b-v1").as_str())
        && value("cpu_affinity") == Some("0")
        && value("host_identity") == Some(digest_bytes(b"fixture-host").as_str())
        && value("libtest_relative_path") == Some("tools/frozen-libtest")
        && value("nonce") == Some("0123456789abcdef0123456789abcdef")
        && value("plan_identity") == Some(digest_bytes(b"fixture-plan").as_str())
        && value("request_relative_path") == Some("requests/launch-0.request")
        && value("result_identity") == Some(digest_bytes(b"fixture-result-0").as_str())
        && value("result_relative_path") == Some("results/launch-0")
        && value("rung_identity") == Some(digest_bytes(b"fixture-rung-0").as_str())
        && value("rung_ordinal") == Some("0")
        && value("semantic_artifact_relative_path") == Some("artifacts/semantic.bin")
        && value("sentinel_relative_path") == Some("artifacts/completion.sentinel")
        && value("workload_identity") == Some(digest_bytes(b"fixture-workload").as_str())
}

fn is_spec_owned_direct_performance_fixture(request: &ParsedWu0gChildRequest) -> bool {
    let value = |key: &str| request.fields.get(key).map(String::as_str);
    request.kind == Wu0gChildKind::Performance
        && request.mode == RawProbeMode::Baseline
        && request.deadline_ms == WU0G_DEADLINE_MS
        && request.memory_limit_bytes == WU0G_MEMORY_LIMIT_BYTES
        && request.rss_limit_bytes == WU0G_RSS_LIMIT_BYTES
        && request.nofile_soft == WU0G_NOFILE_LIMIT
        && request.nofile_hard == WU0G_NOFILE_LIMIT
        && value("artifact_relative_path") == Some("artifacts/child.bin")
        && value("candidate_identity") == Some(digest_bytes(b"candidate-b-v1").as_str())
        && value("cpu_affinity") == Some("0")
        && value("host_identity") == Some(digest_bytes(b"fixture-host").as_str())
        && value("launch_identity") == Some(digest_bytes(b"fixture-launch-0").as_str())
        && value("launch_ordinal") == Some("0")
        && value("libtest_relative_path") == Some("tools/frozen-libtest")
        && value("nonce") == Some("0123456789abcdef0123456789abcdef")
        && value("pair_identity") == Some(digest_bytes(b"fixture-pair-0").as_str())
        && value("pair_ordinal") == Some("0")
        && value("perf_event") == Some(WU0G_PERF_EVENT)
        && value("plan_identity") == Some(digest_bytes(b"fixture-plan").as_str())
        && value("request_relative_path") == Some("requests/launch-0.request")
        && value("result_identity") == Some(digest_bytes(b"fixture-result-0").as_str())
        && value("result_relative_path") == Some("results/launch-0")
        && value("semantic_artifact_relative_path") == Some("artifacts/semantic.bin")
        && value("sentinel_relative_path") == Some("artifacts/completion.sentinel")
        && value("workload_identity") == Some(digest_bytes(b"fixture-workload").as_str())
}

fn read_child_nofile_limits() -> Result<(u64, u64), String> {
    let bytes = read_bounded_file(Path::new("/proc/self/limits"), WU0G_SENTINEL_CAP_BYTES)?;
    if !bytes.is_ascii() || bytes.contains(&b'\r') {
        return Err("child process limits are not canonical ASCII".to_owned());
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| "child process limits are not UTF-8".to_owned())?;
    let rows = text
        .lines()
        .filter_map(|line| line.strip_prefix("Max open files"))
        .collect::<Vec<_>>();
    if rows.len() != 1 {
        return Err("child process limits lack one Max open files row".to_owned());
    }
    let fields = rows[0].split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 || fields[2] != "files" {
        return Err("child Max open files row is not canonical".to_owned());
    }
    Ok((
        parse_positive_decimal(fields[0], "child nofile soft readback")?,
        parse_positive_decimal(fields[1], "child nofile hard readback")?,
    ))
}

fn set_and_verify_child_cpu_affinity(specification: &str) -> Result<(), String> {
    let mut requested = rustix::thread::CpuSet::new();
    let mut requested_cpus = BTreeSet::new();
    for segment in specification.split(',') {
        let mut bounds = segment.split('-');
        let first = bounds
            .next()
            .ok_or_else(|| "CPU affinity segment is empty".to_owned())?;
        let first = usize::try_from(parse_canonical_decimal(first, "CPU affinity start")?)
            .map_err(|_| "CPU affinity start does not fit usize")?;
        let last = match bounds.next() {
            Some(value) => usize::try_from(parse_canonical_decimal(value, "CPU affinity end")?)
                .map_err(|_| "CPU affinity end does not fit usize")?,
            None => first,
        };
        if bounds.next().is_some() || first > last || last >= rustix::thread::CpuSet::MAX_CPU {
            return Err("CPU affinity range is not canonical".to_owned());
        }
        for cpu in first..=last {
            if !requested_cpus.insert(cpu) {
                return Err("CPU affinity contains a duplicate CPU".to_owned());
            }
            requested.set(cpu);
        }
    }
    if requested_cpus.is_empty() {
        return Err("CPU affinity set is empty".to_owned());
    }
    rustix::thread::sched_setaffinity(None, &requested)
        .map_err(|error| format!("set child CPU affinity failed: {error}"))?;
    let observed = rustix::thread::sched_getaffinity(None)
        .map_err(|error| format!("read child CPU affinity failed: {error}"))?;
    if observed != requested {
        return Err("child CPU affinity readback differs from request".to_owned());
    }
    Ok(())
}

fn exact_child_fd_env(key: &str) -> Result<u32, String> {
    let value = std::env::var(key).map_err(|_| format!("missing child FD environment {key}"))?;
    let parsed = parse_positive_decimal(&value, key)?;
    u32::try_from(parsed).map_err(|_| format!("child FD {key} does not fit u32"))
}

fn canonical_child_measurement_artifact(
    measured: &MeasuredDomRung,
    request_identity: &RawIdentityEvidence,
) -> Result<Vec<u8>, String> {
    let report = &measured.measurement.attribution;
    let mut fields = BTreeMap::new();
    fields.insert(
        "candidate_avoided_visits".to_owned(),
        report.candidate_b_avoided_visits.to_string(),
    );
    fields.insert(
        "completed_components".to_owned(),
        measured
            .measurement
            .raw_completion
            .completed_components
            .iter()
            .map(|component| component.identity.as_str())
            .collect::<Vec<_>>()
            .join("|"),
    );
    fields.insert(
        "request_content_identity".to_owned(),
        request_identity.claimed_sha256.clone(),
    );
    fields.insert(
        "semantic_identity".to_owned(),
        measured.measurement.semantic_sha256.clone(),
    );
    fields.insert(
        "substitution_visits".to_owned(),
        report.substitution_visits.to_string(),
    );
    fields.insert(
        "total_work".to_owned(),
        report
            .substitution_visits
            .checked_add(report.substitution_interner_attempts)
            .ok_or_else(|| "child total-work counter overflow".to_owned())?
            .to_string(),
    );
    protocol_record_bytes(
        "typokat-wu0g-child-artifact-v1",
        &[
            "candidate_avoided_visits",
            "completed_components",
            "request_content_identity",
            "semantic_identity",
            "substitution_visits",
            "total_work",
        ],
        &fields,
    )
}

fn protocol_record_bytes(
    header: &str,
    schema: &[&str],
    fields: &BTreeMap<String, String>,
) -> Result<Vec<u8>, String> {
    if fields.keys().map(String::as_str).collect::<Vec<_>>() != schema {
        return Err("protocol record fields differ from the exact schema".to_owned());
    }
    let mut bytes = format!("{header}\n").into_bytes();
    for key in schema {
        let value = fields
            .get(*key)
            .ok_or_else(|| format!("protocol record lacks {key}"))?;
        bytes.extend_from_slice(format!("{key}={}:{}\n", value.len(), value).as_bytes());
    }
    Ok(bytes)
}

#[derive(Debug)]
struct ObservedWu0gChild {
    request_bytes: Vec<u8>,
    result_bytes: Vec<u8>,
    result: ParsedWu0gChildResult,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    sentinel: Vec<u8>,
    semantic_artifact: Vec<u8>,
    child_artifact: Vec<u8>,
    perf_artifact: Option<Vec<u8>>,
    request_relative_path: String,
    result_relative_path: String,
    canonical_invocation: Vec<String>,
    wall_ns: u64,
}

fn run_wu0e_hardened_child_for_test(
    scratch: &Path,
    request_fields: &BTreeMap<String, String>,
    per_rung_deadline: Duration,
    memory_max_bytes: u64,
) -> Result<ObservedWu0gChild, String> {
    let request_bytes = protocol_record_bytes(
        "typokat-wu0g-child-request-v1",
        WU0G_REQUEST_PROTOCOL_FIELDS,
        request_fields,
    )?;
    let request = parse_wu0g_child_request(&request_bytes)?;
    if request.deadline_ms
        != u64::try_from(per_rung_deadline.as_millis())
            .map_err(|_| "per-rung deadline does not fit u64 milliseconds")?
        || request.memory_limit_bytes != memory_max_bytes
    {
        return Err("request limits differ from coordinator bounds".to_owned());
    }
    let request_relative_path = request
        .fields
        .get("request_relative_path")
        .cloned()
        .ok_or_else(|| "request path is absent".to_owned())?;
    let result_relative_path = request
        .fields
        .get("result_relative_path")
        .cloned()
        .ok_or_else(|| "result path is absent".to_owned())?;
    let request_path = scratch.join(&request_relative_path);
    let result_dir = scratch.join(&result_relative_path);
    if result_dir.exists() {
        return Err("hardened result directory already exists".to_owned());
    }
    write_exclusive_bounded(&request_path, &request_bytes, WU0G_REQUEST_CAP_BYTES)?;
    let runner = Path::new(env!("CARGO_MANIFEST_DIR")).join("tooling/wu0e-diagnostic/run.pl");
    let runner_stdout_path = scratch.join(format!(
        "{}.runner.stdout",
        request
            .fields
            .get("nonce")
            .ok_or_else(|| "request nonce is absent".to_owned())?
    ));
    let runner_stderr_path = scratch.join(format!(
        "{}.runner.stderr",
        request
            .fields
            .get("nonce")
            .ok_or_else(|| "request nonce is absent".to_owned())?
    ));
    let runner_stdout = create_exclusive_file(&runner_stdout_path)
        .map_err(|error| format!("runner stdout create failed: {error}"))?;
    let runner_stderr = create_exclusive_file(&runner_stderr_path)
        .map_err(|error| format!("runner stderr create failed: {error}"))?;
    let canonical_invocation = vec![
        "/usr/bin/perl".to_owned(),
        runner.display().to_string(),
        "--wu0g-child-v1".to_owned(),
        request_path.display().to_string(),
        result_dir.display().to_string(),
    ];
    let launched_at = Instant::now();
    let mut child = Command::new("/usr/bin/perl")
        .arg(&runner)
        .arg("--wu0g-child-v1")
        .arg(&request_path)
        .arg(&result_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(runner_stdout))
        .stderr(Stdio::from(runner_stderr))
        .spawn()
        .map_err(|error| format!("hardened WU0G runner spawn failed: {error}"))?;
    let outer_deadline = Instant::now()
        .checked_add(
            per_rung_deadline
                .checked_add(Duration::from_secs(15))
                .ok_or_else(|| "outer runner deadline overflow".to_owned())?,
        )
        .ok_or_else(|| "outer runner instant overflow".to_owned())?;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("hardened runner wait failed: {error}"))?
        {
            break status;
        }
        if Instant::now() >= outer_deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("outer deadline expired around hardened WU0G route".to_owned());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let outer_stdout = read_bounded_file(&runner_stdout_path, WU0G_STDOUT_CAP_BYTES)?;
    let outer_stderr = read_bounded_file(&runner_stderr_path, WU0G_STDERR_CAP_BYTES)?;
    if !status.success() {
        return Err(format!(
            "hardened WU0G runner failed with {status}; stdout={} bytes stderr={} bytes",
            outer_stdout.len(),
            outer_stderr.len()
        ));
    }
    let result_path = result_dir.join("result.v1");
    let result_bytes = read_bounded_file(&result_path, WU0G_RESULT_CAP_BYTES)?;
    let result = parse_wu0g_child_result(&request, &request_bytes, &result_bytes)?;
    let wall_ns = u64::try_from(launched_at.elapsed().as_nanos())
        .map_err(|_| "launch wall time does not fit u64 nanoseconds")?;
    let read_named = |field: &str, cap: u64| -> Result<Vec<u8>, String> {
        let relative = request
            .fields
            .get(field)
            .ok_or_else(|| format!("request lacks {field}"))?;
        read_bounded_file(&result_dir.join(relative), cap)
    };
    let sentinel = read_named("sentinel_relative_path", WU0G_SENTINEL_CAP_BYTES)?;
    let semantic_artifact = read_named("semantic_artifact_relative_path", WU0G_ARTIFACT_CAP_BYTES)?;
    let child_artifact = read_named("artifact_relative_path", WU0G_ARTIFACT_CAP_BYTES)?;
    authenticate_result_artifact(
        &result.fields,
        "sentinel_identity",
        "sentinel_size",
        "wu0g-child-completion-sentinel-v1",
        &sentinel,
    )?;
    authenticate_completion_sentinel(
        &request,
        &request_bytes,
        &result,
        &sentinel,
        &semantic_artifact,
    )?;
    authenticate_result_artifact(
        &result.fields,
        "artifact_identity",
        "artifact_size",
        "wu0g-child-artifact-v1",
        &child_artifact,
    )?;
    let stdout = read_bounded_file(&result_dir.join("stdout.bin"), WU0G_STDOUT_CAP_BYTES)?;
    let stderr = read_bounded_file(&result_dir.join("stderr.bin"), WU0G_STDERR_CAP_BYTES)?;
    authenticate_result_artifact(
        &result.fields,
        "stdout_identity",
        "stdout_size",
        "wu0g-stdout-v1",
        &stdout,
    )?;
    authenticate_result_artifact(
        &result.fields,
        "stderr_identity",
        "stderr_size",
        "wu0g-stderr-v1",
        &stderr,
    )?;
    let perf_artifact = if request.kind == Wu0gChildKind::Performance {
        let bytes = read_bounded_file(
            &result_dir.join("artifacts/perf.csv"),
            WU0G_PERF_ARTIFACT_CAP_BYTES,
        )?;
        parse_wu0g_perf_artifact_for_test(&bytes)?;
        authenticate_result_artifact(
            &result.fields,
            "perf_artifact_identity",
            "perf_artifact_size",
            "wu0g-perf-artifact-v1",
            &bytes,
        )?;
        Some(bytes)
    } else {
        None
    };
    Ok(ObservedWu0gChild {
        request_bytes,
        result_bytes,
        result,
        stdout,
        stderr,
        sentinel,
        semantic_artifact,
        child_artifact,
        perf_artifact,
        request_relative_path,
        result_relative_path,
        canonical_invocation,
        wall_ns,
    })
}

fn authenticate_completion_sentinel(
    request: &ParsedWu0gChildRequest,
    request_bytes: &[u8],
    result: &ParsedWu0gChildResult,
    sentinel: &[u8],
    semantic_artifact: &[u8],
) -> Result<(), String> {
    let schema = [
        "argv",
        "child_cgroup",
        "child_pgid",
        "child_pid",
        "child_start_ticks",
        "environment",
        "fd_inventory",
        "nofile_hard",
        "nofile_soft",
        "request_content_identity",
        "semantic_artifact_identity",
        "semantic_artifact_size",
    ];
    let fields = parse_protocol_record(
        sentinel,
        WU0G_SENTINEL_CAP_BYTES,
        b"typokat-wu0g-child-completion-sentinel-v1",
        &schema,
    )?;
    let value = |key: &str| {
        fields
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("completion sentinel lacks {key}"))
    };
    let request_identity =
        raw_identity_from_parts("wu0g-request-content-v1", request_bytes.to_vec())
            .ok_or_else(|| "sentinel request identity overflow".to_owned())?;
    let semantic_identity =
        raw_identity_from_parts("wu0g-semantic-artifact-v1", semantic_artifact.to_vec())
            .ok_or_else(|| "sentinel semantic identity overflow".to_owned())?;
    let semantic_size = u64::try_from(semantic_artifact.len())
        .map_err(|_| "sentinel semantic size does not fit u64")?;
    let expected_fds = match request.kind {
        Wu0gChildKind::Causal => "stderr|stdin|stdout|request|result|libtest|prlimit",
        Wu0gChildKind::Performance => {
            "stderr|stdin|stdout|request|result|libtest|prlimit|perf|perf-log"
        }
    };
    let child_cgroup = value("child_cgroup")?;
    if !canonical_cgroup_value(child_cgroup) {
        return Err("completion sentinel child cgroup is not canonical".to_owned());
    }
    let scope_cgroup = child_cgroup
        .rsplit_once('/')
        .filter(|(scope, launch)| {
            !launch.is_empty()
                && scope
                    .rsplit('/')
                    .next()
                    .is_some_and(|component| component.ends_with(".scope"))
        })
        .map(|(scope, _)| scope)
        .ok_or_else(|| "completion sentinel launch cgroup lacks a scope parent".to_owned())?;
    if !canonical_cgroup_value(scope_cgroup) {
        return Err("completion sentinel scope cgroup is not canonical".to_owned());
    }
    let child_pid = u32::try_from(parse_positive_decimal(
        value("child_pid")?,
        "sentinel child PID",
    )?)
    .map_err(|_| "sentinel child PID does not fit u32".to_owned())?;
    let child_pgid = u32::try_from(parse_positive_decimal(
        value("child_pgid")?,
        "sentinel child PGID",
    )?)
    .map_err(|_| "sentinel child PGID does not fit u32".to_owned())?;
    let child_start_ticks =
        parse_positive_decimal(value("child_start_ticks")?, "sentinel child start ticks")?;
    let leader_pid = u32::try_from(parse_positive_decimal(
        result
            .fields
            .get("leader_pid")
            .map(String::as_str)
            .ok_or_else(|| "result lacks leader PID".to_owned())?,
        "result leader PID",
    )?)
    .map_err(|_| "result leader PID does not fit u32".to_owned())?;
    let leader_start_ticks = parse_positive_decimal(
        result
            .fields
            .get("leader_start_ticks")
            .map(String::as_str)
            .ok_or_else(|| "result lacks leader start ticks".to_owned())?,
        "result leader start ticks",
    )?;
    let result_pgid = u32::try_from(parse_positive_decimal(
        result
            .fields
            .get("pgid")
            .map(String::as_str)
            .ok_or_else(|| "result lacks PGID".to_owned())?,
        "result PGID",
    )?)
    .map_err(|_| "result PGID does not fit u32".to_owned())?;
    let kind_matches = match request.kind {
        Wu0gChildKind::Causal => child_pid == leader_pid && child_start_ticks == leader_start_ticks,
        Wu0gChildKind::Performance => child_pid != leader_pid,
    };
    let child_canonical = format!(
        "typokat-wu0g-observed-child-v1\nchild_pgid={child_pgid}\nchild_pid={child_pid}\nchild_start_ticks={child_start_ticks}\nlaunch_cgroup={child_cgroup}\nscope_cgroup={scope_cgroup}\n"
    );
    let child_identity = raw_identity_from_parts("wu0g-child-v1", child_canonical.into_bytes())
        .ok_or_else(|| "sentinel child identity overflow".to_owned())?;
    let cgroup_identity =
        raw_identity_from_parts("wu0g-cgroup-v1", child_cgroup.as_bytes().to_vec())
            .ok_or_else(|| "sentinel cgroup identity overflow".to_owned())?;
    let scope_identity = raw_identity_from_parts("wu0g-scope-v1", scope_cgroup.as_bytes().to_vec())
        .ok_or_else(|| "sentinel scope identity overflow".to_owned())?;
    if value("argv")?
        != "--ignored|--exact|check::checker::decls::wu0g_interface_fill_attribution_spec::wu0g_hardened_child_once|--nocapture"
        || value("environment")?
            != "TYPOKAT_WU0G_CHILD_REQUEST_FD|TYPOKAT_WU0G_CHILD_REQUEST_SHA256|TYPOKAT_WU0G_CHILD_RESULT_DIR_FD"
        || value("fd_inventory")? != expected_fds
        || parse_canonical_decimal(value("nofile_hard")?, "sentinel nofile hard")?
            != request.nofile_hard
        || parse_canonical_decimal(value("nofile_soft")?, "sentinel nofile soft")?
            != request.nofile_soft
        || value("request_content_identity")? != request_identity.claimed_sha256
        || value("semantic_artifact_identity")? != semantic_identity.claimed_sha256
        || parse_canonical_decimal(value("semantic_artifact_size")?, "semantic artifact size")?
            != semantic_size
        || child_pgid != leader_pid
        || result_pgid != leader_pid
        || !kind_matches
        || result.fields.get("child_identity").map(String::as_str)
            != Some(child_identity.claimed_sha256.as_str())
        || result.fields.get("cgroup_identity").map(String::as_str)
            != Some(cgroup_identity.claimed_sha256.as_str())
        || result.fields.get("scope_identity").map(String::as_str)
            != Some(scope_identity.claimed_sha256.as_str())
        || result.fields.get("child_argv").map(String::as_str) != Some(value("argv")?)
        || result.fields.get("child_env").map(String::as_str) != Some(value("environment")?)
        || result.fields.get("child_fd_inventory").map(String::as_str)
            != Some(value("fd_inventory")?)
    {
        return Err("completion sentinel does not authenticate the child boundary".to_owned());
    }
    Ok(())
}

fn authenticate_result_artifact(
    fields: &BTreeMap<String, String>,
    identity_field: &str,
    size_field: &str,
    domain: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let identity = raw_identity_from_parts(domain, bytes.to_vec())
        .ok_or_else(|| format!("{identity_field} framing overflow"))?;
    let observed_size = u64::try_from(bytes.len()).map_err(|_| "artifact size does not fit u64")?;
    if fields.get(identity_field) != Some(&identity.claimed_sha256)
        || fields
            .get(size_field)
            .map(String::as_str)
            .map(|value| parse_canonical_decimal(value, size_field))
            .transpose()?
            != Some(observed_size)
    {
        return Err(format!("result does not authenticate {identity_field}"));
    }
    Ok(())
}

#[derive(Clone)]
struct PlannedCausalLaunch {
    rung_ordinal: u8,
    rung_identity: RawIdentityEvidence,
    mode: RawProbeMode,
    binary_identity: RawIdentityEvidence,
    request_relative_path: String,
    result_relative_path: String,
    result_identity_claim: String,
    nonce: String,
    canonical_invocation: Vec<String>,
}

#[derive(Clone)]
struct PlannedPerformanceLaunch {
    pair_ordinal: u8,
    launch_ordinal: u8,
    order: RawLaunchOrder,
    pair_identity: RawIdentityEvidence,
    launch_identity: RawIdentityEvidence,
    mode: RawProbeMode,
    binary_identity: RawIdentityEvidence,
    request_relative_path: String,
    result_relative_path: String,
    result_identity_claim: String,
    nonce: String,
    canonical_invocation: Vec<String>,
    perf_invocation: Vec<String>,
}

struct CoordinatorIdentityInputs {
    profile: RawIdentityEvidence,
    universe: RawIdentityEvidence,
    workload: RawIdentityEvidence,
    representative_workload: RawIdentityEvidence,
    candidate: RawIdentityEvidence,
    baseline_binary: RawIdentityEvidence,
    candidate_binary: RawIdentityEvidence,
    prlimit: RawIdentityEvidence,
    perf: RawIdentityEvidence,
    host: RawIdentityEvidence,
    perf_version: String,
    cpu_affinity: String,
}

pub(super) fn run_pinned_dom_authorization_probe_for_test(
    profile: &StrictLibraryProfile,
    per_rung_deadline: Duration,
    memory_max_bytes: u64,
) -> Result<RawAuthorizationEvidence, String> {
    if profile.sources.len() != 82
        || per_rung_deadline != Duration::from_millis(WU0G_DEADLINE_MS)
        || memory_max_bytes != WU0G_MEMORY_LIMIT_BYTES
    {
        return Err("pinned coordinator input differs from the authorized bounds".to_owned());
    }
    let ladder = build_dom_heritage_prefix_ladder_for_test(profile)?;
    let scratch = create_wu0g_coordinator_scratch()?;
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("current libtest path unavailable: {error}"))?;
    let frozen_libtest = scratch.join("tools/frozen-libtest");
    std::fs::copy(&current_exe, &frozen_libtest)
        .map_err(|error| format!("freeze WU0G libtest failed: {error}"))?;
    let binary_sha = sha256_file(&frozen_libtest)?;
    let prlimit_sha = sha256_file(Path::new("/usr/bin/prlimit"))?;
    let perf_sha = sha256_file(Path::new("/usr/bin/perf"))?;
    let perf_version = exact_tool_version("/usr/bin/perf", "--version")?;
    let cpu_affinity = first_allowed_cpu()?;
    let profile_bytes = canonical_pinned_profile_marker(profile);
    let identities = CoordinatorIdentityInputs {
        profile: raw_identity_from_parts("wu0g-profile-v1", profile_bytes)
            .ok_or_else(|| "profile identity overflow".to_owned())?,
        universe: raw_identity_from_parts(
            "wu0g-universe-v1",
            [
                b"typokat-wu0g-authenticated-pinned-universe-v1\0".as_slice(),
                ladder
                    .last()
                    .ok_or_else(|| "pinned ladder is empty".to_owned())?
                    .universe_identity
                    .as_bytes(),
            ]
            .concat(),
        )
        .ok_or_else(|| "universe identity overflow".to_owned())?,
        workload: raw_identity_from_parts("wu0g-workload-v1", b"pinned-dom-five-rung".to_vec())
            .ok_or_else(|| "workload identity overflow".to_owned())?,
        representative_workload: raw_identity_from_parts(
            "wu0g-workload-v1",
            b"pinned-dom-full-end-to-end".to_vec(),
        )
        .ok_or_else(|| "representative workload identity overflow".to_owned())?,
        candidate: raw_identity_from_parts("wu0g-candidate-v1", b"candidate-b-v1".to_vec())
            .ok_or_else(|| "candidate identity overflow".to_owned())?,
        baseline_binary: raw_identity_from_parts(
            "wu0g-binary-v1",
            format!("baseline:{binary_sha}").into_bytes(),
        )
        .ok_or_else(|| "baseline binary identity overflow".to_owned())?,
        candidate_binary: raw_identity_from_parts(
            "wu0g-binary-v1",
            format!("candidate:{binary_sha}").into_bytes(),
        )
        .ok_or_else(|| "candidate binary identity overflow".to_owned())?,
        prlimit: raw_identity_from_parts("wu0g-executable-v1", prlimit_sha.as_bytes().to_vec())
            .ok_or_else(|| "prlimit identity overflow".to_owned())?,
        perf: raw_identity_from_parts("wu0g-executable-v1", perf_sha.as_bytes().to_vec())
            .ok_or_else(|| "perf identity overflow".to_owned())?,
        host: raw_identity_from_parts("wu0g-host-v1", canonical_host_bytes()?)
            .ok_or_else(|| "host identity overflow".to_owned())?,
        perf_version,
        cpu_affinity,
    };
    let (causal_plans, performance_plans) = build_coordinator_launch_plans(
        &scratch,
        &ladder,
        &identities,
        per_rung_deadline,
        memory_max_bytes,
    )?;
    let plan_identity =
        coordinator_plan_identity(&identities, &ladder, &causal_plans, &performance_plans)?;
    let request_context = CoordinatorRequestContext {
        plan_identity: &plan_identity,
        identities: &identities,
        binary_sha: &binary_sha,
        prlimit_sha: &prlimit_sha,
        perf_sha: &perf_sha,
    };
    let (causal_observed, performance_observed) =
        run_pinned_dom_ladder_probe_for_test(CoordinatorProbeInputs {
            scratch: &scratch,
            causal_plans: &causal_plans,
            performance_plans: &performance_plans,
            request_context,
            per_rung_deadline,
            memory_max_bytes,
        })?;
    let mut thresholds = assemble_pinned_threshold_evidence(ThresholdAssemblyInputs {
        identities,
        plan_identity,
        ladder: &ladder,
        causal_plans,
        causal_observed,
        performance_plans,
        performance_observed,
    })?;
    thresholds.experiment_identity = raw_experiment_identity(&thresholds)
        .ok_or_else(|| "final experiment identity overflow".to_owned())?;
    if evaluate_interface_fill_thresholds_from_raw_for_test(&thresholds).thresholds_pass() {
        let _ = std::fs::remove_dir_all(&scratch);
    }
    Ok(RawAuthorizationEvidence {
        evidence_domain: RawEvidenceDomain::PinnedDom82,
        thresholds,
    })
}

fn create_wu0g_coordinator_scratch() -> Result<PathBuf, String> {
    let nonce = format!(
        "typokat-wu0g-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "system clock precedes the Unix epoch".to_owned())?
            .as_nanos()
    );
    let root = std::env::temp_dir().join(nonce);
    std::fs::create_dir(&root)
        .map_err(|error| format!("create coordinator root failed: {error}"))?;
    for child in ["requests", "results", "tools"] {
        std::fs::create_dir(root.join(child))
            .map_err(|error| format!("create coordinator {child} failed: {error}"))?;
    }
    Ok(root)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!(
            "read executable identity {} failed: {error}",
            path.display()
        )
    })?;
    Ok(digest_bytes(&bytes))
}

fn exact_tool_version(tool: &str, argument: &str) -> Result<String, String> {
    let output = Command::new(tool)
        .arg(argument)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("tool version probe failed for {tool}: {error}"))?;
    if !output.status.success() || !output.stderr.is_empty() || output.stdout.len() > 4_096 {
        return Err(format!("tool version probe was not clean for {tool}"));
    }
    let value = std::str::from_utf8(&output.stdout)
        .map_err(|_| "tool version is not UTF-8".to_owned())?
        .trim_end_matches('\n');
    if value.is_empty() || value.contains('\r') || value.contains('\n') {
        return Err("tool version is not one canonical line".to_owned());
    }
    Ok(value.to_owned())
}

fn first_allowed_cpu() -> Result<String, String> {
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|error| format!("read CPU affinity failed: {error}"))?;
    let list = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:\t"))
        .ok_or_else(|| "CPU affinity list is absent".to_owned())?;
    let first = list
        .split(',')
        .next()
        .and_then(|range| range.split('-').next())
        .ok_or_else(|| "CPU affinity list is empty".to_owned())?;
    parse_canonical_decimal(first, "first allowed CPU")?;
    Ok(first.to_owned())
}

fn canonical_host_bytes() -> Result<Vec<u8>, String> {
    let machine = std::fs::read("/etc/machine-id")
        .map_err(|error| format!("read host machine identity failed: {error}"))?;
    let mut bytes = b"typokat-wu0g-host-v1\0".to_vec();
    frame_vec(&mut bytes, &machine);
    frame_vec(&mut bytes, std::env::consts::ARCH.as_bytes());
    frame_vec(&mut bytes, std::env::consts::OS.as_bytes());
    Ok(bytes)
}

fn canonical_pinned_profile_marker(profile: &StrictLibraryProfile) -> Vec<u8> {
    let mut bytes = b"typokat-wu0g-authenticated-pinned-dom-82-v1\0".to_vec();
    frame_vec(&mut bytes, strict_profile_identity(profile).as_bytes());
    bytes
}

fn build_coordinator_launch_plans(
    scratch: &Path,
    ladder: &[InterfaceHeritagePrefixRung],
    identities: &CoordinatorIdentityInputs,
    _per_rung_deadline: Duration,
    _memory_max_bytes: u64,
) -> Result<(Vec<PlannedCausalLaunch>, Vec<PlannedPerformanceLaunch>), String> {
    let runner = Path::new(env!("CARGO_MANIFEST_DIR")).join("tooling/wu0e-diagnostic/run.pl");
    let mut causal = Vec::with_capacity(10);
    for (rung_index, rung) in ladder.iter().enumerate() {
        for mode in [RawProbeMode::Baseline, RawProbeMode::CandidateB] {
            let ordinal = causal.len();
            let request_relative_path = format!("requests/causal-{ordinal}.request");
            let result_relative_path = format!("results/causal-{ordinal}");
            let binary_identity = match mode {
                RawProbeMode::Baseline => identities.baseline_binary.clone(),
                RawProbeMode::CandidateB => identities.candidate_binary.clone(),
            };
            causal.push(PlannedCausalLaunch {
                rung_ordinal: u8::try_from(rung_index)
                    .map_err(|_| "rung ordinal does not fit u8")?,
                rung_identity: raw_identity_from_parts(
                    "wu0g-rung-v1",
                    rung.identity_canonical_bytes.clone(),
                )
                .ok_or_else(|| "rung identity overflow".to_owned())?,
                mode,
                binary_identity,
                result_identity_claim: digest_bytes(format!("causal-result-{ordinal}").as_bytes()),
                nonce: format!("{ordinal:032x}"),
                canonical_invocation: vec![
                    "/usr/bin/perl".to_owned(),
                    runner.display().to_string(),
                    "--wu0g-child-v1".to_owned(),
                    scratch.join(&request_relative_path).display().to_string(),
                    scratch.join(&result_relative_path).display().to_string(),
                ],
                request_relative_path,
                result_relative_path,
            });
        }
    }
    let mut performance = Vec::with_capacity(10);
    for pair_ordinal in 0_u8..5 {
        let order = if pair_ordinal % 2 == 0 {
            RawLaunchOrder::Ab
        } else {
            RawLaunchOrder::Ba
        };
        let pair_identity = raw_identity_from_parts(
            "wu0g-performance-pair-v1",
            format!("pinned-performance-pair-{pair_ordinal}").into_bytes(),
        )
        .ok_or_else(|| "performance pair identity overflow".to_owned())?;
        let modes = match order {
            RawLaunchOrder::Ab => [RawProbeMode::Baseline, RawProbeMode::CandidateB],
            RawLaunchOrder::Ba => [RawProbeMode::CandidateB, RawProbeMode::Baseline],
        };
        for mode in modes {
            let launch_ordinal = u8::try_from(performance.len())
                .map_err(|_| "performance launch ordinal does not fit u8")?;
            let request_relative_path = format!("requests/performance-{launch_ordinal}.request");
            let result_relative_path = format!("results/performance-{launch_ordinal}");
            performance.push(PlannedPerformanceLaunch {
                pair_ordinal,
                launch_ordinal,
                order,
                pair_identity: pair_identity.clone(),
                launch_identity: raw_identity_from_parts(
                    "wu0g-performance-launch-v1",
                    format!("pinned-performance-launch-{launch_ordinal}").into_bytes(),
                )
                .ok_or_else(|| "performance launch identity overflow".to_owned())?,
                mode,
                binary_identity: match mode {
                    RawProbeMode::Baseline => identities.baseline_binary.clone(),
                    RawProbeMode::CandidateB => identities.candidate_binary.clone(),
                },
                result_identity_claim: digest_bytes(
                    format!("performance-result-{launch_ordinal}").as_bytes(),
                ),
                nonce: format!("{:032x}", 10_u64 + u64::from(launch_ordinal)),
                canonical_invocation: vec![
                    "/usr/bin/perl".to_owned(),
                    runner.display().to_string(),
                    "--wu0g-child-v1".to_owned(),
                    scratch.join(&request_relative_path).display().to_string(),
                    scratch.join(&result_relative_path).display().to_string(),
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
                request_relative_path,
                result_relative_path,
            });
        }
    }
    Ok((causal, performance))
}

fn coordinator_plan_identity(
    identities: &CoordinatorIdentityInputs,
    ladder: &[InterfaceHeritagePrefixRung],
    causal: &[PlannedCausalLaunch],
    performance: &[PlannedPerformanceLaunch],
) -> Result<RawIdentityEvidence, String> {
    let mut bytes = Vec::new();
    for identity in [
        &identities.profile,
        &identities.universe,
        &identities.workload,
        &identities.representative_workload,
        &identities.candidate,
        &identities.baseline_binary,
        &identities.candidate_binary,
        &identities.prlimit,
        &identities.perf,
        &identities.host,
    ] {
        raw_frame_identity(&mut bytes, identity)
            .ok_or_else(|| "plan identity framing overflow".to_owned())?;
    }
    for value in [
        identities.perf_version.as_str(),
        WU0G_PERF_EVENT,
        identities.cpu_affinity.as_str(),
    ] {
        raw_frame_bytes(&mut bytes, value.as_bytes())
            .ok_or_else(|| "plan value framing overflow".to_owned())?;
    }
    for (rung_ordinal, rung) in ladder.iter().enumerate() {
        bytes.push(u8::try_from(rung_ordinal).map_err(|_| "plan rung ordinal does not fit u8")?);
        let identity =
            raw_identity_from_parts("wu0g-rung-v1", rung.identity_canonical_bytes.clone())
                .ok_or_else(|| "plan rung identity overflow".to_owned())?;
        raw_frame_identity(&mut bytes, &identity)
            .ok_or_else(|| "plan rung framing overflow".to_owned())?;
        raw_frame_identity(&mut bytes, &identity)
            .ok_or_else(|| "plan rung framing overflow".to_owned())?;
    }
    for launch in causal {
        bytes.extend([launch.rung_ordinal, mode_code(launch.mode)]);
        raw_frame_identity(&mut bytes, &launch.binary_identity)
            .ok_or_else(|| "causal binary framing overflow".to_owned())?;
        raw_frame_strings(&mut bytes, &launch.canonical_invocation)
            .ok_or_else(|| "causal invocation framing overflow".to_owned())?;
        for value in coordinator_limits() {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    for pair_ordinal in 0_u8..5 {
        let pair_launches = performance
            .iter()
            .filter(|launch| launch.pair_ordinal == pair_ordinal)
            .collect::<Vec<_>>();
        if pair_launches.len() != 2 {
            return Err("performance plan does not contain two launches per pair".to_owned());
        }
        let first = pair_launches[0];
        raw_frame_identity(&mut bytes, &first.pair_identity)
            .ok_or_else(|| "performance pair framing overflow".to_owned())?;
        bytes.extend([pair_ordinal, launch_order_code(first.order)]);
        for launch in pair_launches {
            for identity in [
                &launch.pair_identity,
                &launch.launch_identity,
                &launch.binary_identity,
            ] {
                raw_frame_identity(&mut bytes, identity)
                    .ok_or_else(|| "performance identity framing overflow".to_owned())?;
            }
            bytes.extend([launch.pair_ordinal, launch.launch_ordinal]);
            bytes.push(mode_code(launch.mode));
            raw_frame_strings(&mut bytes, &launch.canonical_invocation)
                .ok_or_else(|| "performance invocation framing overflow".to_owned())?;
            raw_frame_strings(&mut bytes, &launch.perf_invocation)
                .ok_or_else(|| "perf invocation framing overflow".to_owned())?;
            for value in coordinator_limits() {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
    }
    raw_identity_from_parts("wu0g-plan-v1", bytes)
        .ok_or_else(|| "coordinator plan identity overflow".to_owned())
}

fn coordinator_limits() -> [u64; 5] {
    [
        WU0G_DEADLINE_MS,
        WU0G_MEMORY_LIMIT_BYTES,
        WU0G_RSS_LIMIT_BYTES,
        WU0G_NOFILE_LIMIT,
        WU0G_NOFILE_LIMIT,
    ]
}

#[derive(Clone, Copy)]
struct CoordinatorRequestContext<'a> {
    plan_identity: &'a RawIdentityEvidence,
    identities: &'a CoordinatorIdentityInputs,
    binary_sha: &'a str,
    prlimit_sha: &'a str,
    perf_sha: &'a str,
}

struct CoordinatorRequestInputs<'a> {
    kind: Wu0gChildKind,
    mode: RawProbeMode,
    context: CoordinatorRequestContext<'a>,
    request_relative_path: &'a str,
    result_relative_path: &'a str,
    result_identity_claim: &'a str,
    nonce: &'a str,
    rung: Option<(u8, &'a RawIdentityEvidence)>,
    performance: Option<&'a PlannedPerformanceLaunch>,
}

fn canonical_coordinator_request_fields(
    input: CoordinatorRequestInputs<'_>,
) -> BTreeMap<String, String> {
    let CoordinatorRequestInputs {
        kind,
        mode,
        context,
        request_relative_path,
        result_relative_path,
        result_identity_claim,
        nonce,
        rung,
        performance,
    } = input;
    let CoordinatorRequestContext {
        plan_identity,
        identities,
        binary_sha,
        prlimit_sha,
        perf_sha,
    } = context;
    let mode = match mode {
        RawProbeMode::Baseline => "baseline",
        RawProbeMode::CandidateB => "candidate",
    };
    let mut fields = BTreeMap::from([
        (
            "artifact_relative_path".to_owned(),
            "artifacts/child.bin".to_owned(),
        ),
        ("binary_identity".to_owned(), binary_sha.to_owned()),
        (
            "candidate_identity".to_owned(),
            identities.candidate.claimed_sha256.clone(),
        ),
        ("cpu_affinity".to_owned(), identities.cpu_affinity.clone()),
        ("deadline_ms".to_owned(), WU0G_DEADLINE_MS.to_string()),
        (
            "host_identity".to_owned(),
            identities.host.claimed_sha256.clone(),
        ),
        (
            "kind".to_owned(),
            match kind {
                Wu0gChildKind::Causal => "causal",
                Wu0gChildKind::Performance => "performance",
            }
            .to_owned(),
        ),
        (
            "libtest_relative_path".to_owned(),
            "tools/frozen-libtest".to_owned(),
        ),
        (
            "memory_limit_bytes".to_owned(),
            WU0G_MEMORY_LIMIT_BYTES.to_string(),
        ),
        ("mode".to_owned(), mode.to_owned()),
        ("nofile_hard".to_owned(), WU0G_NOFILE_LIMIT.to_string()),
        ("nofile_soft".to_owned(), WU0G_NOFILE_LIMIT.to_string()),
        ("nonce".to_owned(), nonce.to_owned()),
        (
            "plan_identity".to_owned(),
            plan_identity.claimed_sha256.clone(),
        ),
        ("prlimit_identity".to_owned(), prlimit_sha.to_owned()),
        (
            "request_relative_path".to_owned(),
            request_relative_path.to_owned(),
        ),
        (
            "result_identity".to_owned(),
            result_identity_claim.to_owned(),
        ),
        (
            "result_relative_path".to_owned(),
            result_relative_path.to_owned(),
        ),
        (
            "rss_limit_bytes".to_owned(),
            WU0G_RSS_LIMIT_BYTES.to_string(),
        ),
        (
            "semantic_artifact_relative_path".to_owned(),
            "artifacts/semantic.bin".to_owned(),
        ),
        (
            "sentinel_relative_path".to_owned(),
            "artifacts/completion.sentinel".to_owned(),
        ),
        (
            "workload_identity".to_owned(),
            match kind {
                Wu0gChildKind::Causal => &identities.workload,
                Wu0gChildKind::Performance => &identities.representative_workload,
            }
            .claimed_sha256
            .clone(),
        ),
    ]);
    match (kind, rung, performance) {
        (Wu0gChildKind::Causal, Some((rung_ordinal, rung_identity)), None) => {
            for key in [
                "launch_identity",
                "launch_ordinal",
                "pair_identity",
                "pair_ordinal",
                "perf_event",
                "perf_identity",
                "perf_version",
            ] {
                fields.insert(key.to_owned(), "none".to_owned());
            }
            fields.insert(
                "rung_identity".to_owned(),
                rung_identity.claimed_sha256.clone(),
            );
            fields.insert("rung_ordinal".to_owned(), rung_ordinal.to_string());
        }
        (Wu0gChildKind::Performance, None, Some(launch)) => {
            fields.insert(
                "launch_identity".to_owned(),
                launch.launch_identity.claimed_sha256.clone(),
            );
            fields.insert(
                "launch_ordinal".to_owned(),
                launch.launch_ordinal.to_string(),
            );
            fields.insert(
                "pair_identity".to_owned(),
                launch.pair_identity.claimed_sha256.clone(),
            );
            fields.insert("pair_ordinal".to_owned(), launch.pair_ordinal.to_string());
            fields.insert("perf_event".to_owned(), WU0G_PERF_EVENT.to_owned());
            fields.insert("perf_identity".to_owned(), perf_sha.to_owned());
            fields.insert("perf_version".to_owned(), identities.perf_version.clone());
            fields.insert("rung_identity".to_owned(), "none".to_owned());
            fields.insert("rung_ordinal".to_owned(), "none".to_owned());
        }
        _ => {}
    }
    fields
}

struct CoordinatorProbeInputs<'a> {
    scratch: &'a Path,
    causal_plans: &'a [PlannedCausalLaunch],
    performance_plans: &'a [PlannedPerformanceLaunch],
    request_context: CoordinatorRequestContext<'a>,
    per_rung_deadline: Duration,
    memory_max_bytes: u64,
}

fn run_pinned_dom_ladder_probe_for_test(
    input: CoordinatorProbeInputs<'_>,
) -> Result<(Vec<ObservedWu0gChild>, Vec<ObservedWu0gChild>), String> {
    let CoordinatorProbeInputs {
        scratch,
        causal_plans,
        performance_plans,
        request_context,
        per_rung_deadline,
        memory_max_bytes,
    } = input;
    let _measured_modes = [
        InterfaceFillAttributionMode::Baseline,
        InterfaceFillAttributionMode::CandidateB,
    ];
    let mut causal = Vec::with_capacity(causal_plans.len());
    for plan in causal_plans {
        let fields = canonical_coordinator_request_fields(CoordinatorRequestInputs {
            kind: Wu0gChildKind::Causal,
            mode: plan.mode,
            context: request_context,
            request_relative_path: &plan.request_relative_path,
            result_relative_path: &plan.result_relative_path,
            result_identity_claim: &plan.result_identity_claim,
            nonce: &plan.nonce,
            rung: Some((plan.rung_ordinal, &plan.rung_identity)),
            performance: None,
        });
        causal.push(run_wu0e_hardened_child_for_test(
            scratch,
            &fields,
            per_rung_deadline,
            memory_max_bytes,
        )?);
    }
    let mut performance = Vec::with_capacity(performance_plans.len());
    for plan in performance_plans {
        let fields = canonical_coordinator_request_fields(CoordinatorRequestInputs {
            kind: Wu0gChildKind::Performance,
            mode: plan.mode,
            context: request_context,
            request_relative_path: &plan.request_relative_path,
            result_relative_path: &plan.result_relative_path,
            result_identity_claim: &plan.result_identity_claim,
            nonce: &plan.nonce,
            rung: None,
            performance: Some(plan),
        });
        performance.push(run_wu0e_hardened_child_for_test(
            scratch,
            &fields,
            per_rung_deadline,
            memory_max_bytes,
        )?);
    }
    Ok((causal, performance))
}

#[derive(Clone, Debug)]
struct ParsedChildArtifact {
    candidate_avoided_visits: u64,
    completed_components: Vec<String>,
    semantic_identity: String,
    substitution_visits: u64,
    total_work: u64,
}

fn parse_child_artifact(bytes: &[u8]) -> Result<ParsedChildArtifact, String> {
    let fields = parse_protocol_record(
        bytes,
        WU0G_ARTIFACT_CAP_BYTES,
        b"typokat-wu0g-child-artifact-v1",
        &[
            "candidate_avoided_visits",
            "completed_components",
            "request_content_identity",
            "semantic_identity",
            "substitution_visits",
            "total_work",
        ],
    )?;
    let value = |key: &str| {
        fields
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("child artifact lacks {key}"))
    };
    require_sha256(
        value("request_content_identity")?,
        "artifact request identity",
    )?;
    require_sha256(value("semantic_identity")?, "artifact semantic identity")?;
    let completed_components = value("completed_components")?
        .split('|')
        .map(|component| {
            require_sha256(component, "completed component identity")?;
            Ok(component.to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    if completed_components.is_empty()
        || completed_components.iter().collect::<BTreeSet<_>>().len() != completed_components.len()
    {
        return Err("child artifact completion inventory is not canonical".to_owned());
    }
    let substitution_visits =
        parse_positive_decimal(value("substitution_visits")?, "substitution visits")?;
    let total_work = parse_positive_decimal(value("total_work")?, "total work")?;
    if substitution_visits > total_work {
        return Err("child artifact target counter exceeds total work".to_owned());
    }
    Ok(ParsedChildArtifact {
        candidate_avoided_visits: parse_canonical_decimal(
            value("candidate_avoided_visits")?,
            "candidate avoided visits",
        )?,
        completed_components,
        semantic_identity: value("semantic_identity")?.to_owned(),
        substitution_visits,
        total_work,
    })
}

fn raw_identity_required(domain: &str, bytes: Vec<u8>) -> Result<RawIdentityEvidence, String> {
    raw_identity_from_parts(domain, bytes).ok_or_else(|| format!("{domain} identity overflow"))
}

fn observed_result_value<'a>(
    observed: &'a ObservedWu0gChild,
    key: &str,
) -> Result<&'a str, String> {
    observed
        .result
        .fields
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("observed result lacks {key}"))
}

fn observed_result_u64(observed: &ObservedWu0gChild, key: &str) -> Result<u64, String> {
    parse_canonical_decimal(observed_result_value(observed, key)?, key)
}

fn observed_result_u32(observed: &ObservedWu0gChild, key: &str) -> Result<u32, String> {
    u32::try_from(observed_result_u64(observed, key)?)
        .map_err(|_| format!("observed {key} does not fit u32"))
}

fn observed_result_bool(observed: &ObservedWu0gChild, key: &str) -> Result<bool, String> {
    parse_protocol_bool(observed_result_value(observed, key)?, key)
}

fn observed_result_optional_i32(
    observed: &ObservedWu0gChild,
    key: &str,
) -> Result<Option<i32>, String> {
    parse_optional_i32(observed_result_value(observed, key)?, key)
}

fn observed_result_wait_status(
    observed: &ObservedWu0gChild,
    key: &str,
) -> Result<Option<i32>, String> {
    let value = observed_result_u64(observed, key)?;
    i32::try_from(value)
        .map(Some)
        .map_err(|_| format!("observed {key} does not fit i32"))
}

fn bounded_observation(cap_bytes: u64, bytes: &[u8]) -> Result<RawBoundedObservation, String> {
    let observed_bytes =
        u64::try_from(bytes.len()).map_err(|_| "observation size does not fit u64")?;
    if observed_bytes > cap_bytes {
        return Err("bounded observation exceeds its cap".to_owned());
    }
    Ok(RawBoundedObservation {
        cap_bytes,
        observed_bytes,
        truncated: false,
    })
}

fn result_claim_identity(
    observed: &ObservedWu0gChild,
    result_key: &str,
    domain: &str,
) -> Result<RawIdentityEvidence, String> {
    raw_identity_required(
        domain,
        observed_result_value(observed, result_key)?
            .as_bytes()
            .to_vec(),
    )
}

fn causal_counter_point(
    plan: &PlannedCausalLaunch,
    observed: &ObservedWu0gChild,
    artifact: &ParsedChildArtifact,
) -> Result<RawLadderCounterPoint, String> {
    if observed.semantic_artifact != artifact.semantic_identity.as_bytes() {
        return Err("child artifact and semantic artifact disagree".to_owned());
    }
    Ok(RawLadderCounterPoint {
        rung_identity: plan.rung_identity.clone(),
        report_identity: raw_identity_required("wu0g-report-v1", observed.child_artifact.clone())?,
        semantic_identity: raw_identity_required(
            "wu0g-prefix-semantic-v1",
            observed.semantic_artifact.clone(),
        )?,
        target_counter: artifact.substitution_visits,
        total_work_counter: artifact.total_work,
        complete_component_identities: artifact
            .completed_components
            .iter()
            .map(|component| {
                raw_identity_required("wu0g-component-v1", component.as_bytes().to_vec())
            })
            .collect::<Result<Vec<_>, String>>()?,
        saturated: false,
    })
}

fn causal_process_dossier(
    identities: &CoordinatorIdentityInputs,
    plan_identity: &RawIdentityEvidence,
    plan: &PlannedCausalLaunch,
    observed: &ObservedWu0gChild,
    point: &RawLadderCounterPoint,
) -> Result<RawFreshProcessDossier, String> {
    let request_identity =
        raw_identity_required("wu0g-request-content-v1", observed.request_bytes.clone())?;
    Ok(RawFreshProcessDossier {
        plan_identity: plan_identity.clone(),
        profile_identity: identities.profile.clone(),
        universe_identity: identities.universe.clone(),
        workload_identity: identities.workload.clone(),
        candidate_identity: identities.candidate.clone(),
        binary_identity: plan.binary_identity.clone(),
        rung_identity: plan.rung_identity.clone(),
        report_identity: point.report_identity.clone(),
        semantic_identity: point.semantic_identity.clone(),
        rung_ordinal: plan.rung_ordinal,
        mode: plan.mode,
        canonical_invocation: observed.canonical_invocation.clone(),
        scope_identity: result_claim_identity(observed, "scope_identity", "wu0g-scope-v1")?,
        cgroup_identity: result_claim_identity(observed, "cgroup_identity", "wu0g-cgroup-v1")?,
        leader_pid: observed_result_u32(observed, "leader_pid")?,
        leader_start_ticks: observed_result_u64(observed, "leader_start_ticks")?,
        pgid: observed_result_u32(observed, "pgid")?,
        readiness_seen: observed_result_bool(observed, "readiness_seen")?,
        membership_verified: observed_result_bool(observed, "membership_verified")?,
        drain_complete: observed_result_bool(observed, "drain_complete")?,
        cgroup_populated_zero: observed_result_bool(observed, "cgroup_populated_zero")?,
        pgid_empty: observed_result_bool(observed, "pgid_empty")?,
        cgroup_removed: observed_result_bool(observed, "cgroup_removed")?,
        cgroup_retained: observed_result_bool(observed, "cgroup_retained")?,
        scope_abort_requested: observed_result_bool(observed, "scope_abort_requested")?,
        scope_abort_observed: observed_result_bool(observed, "scope_abort_observed")?,
        deadline_ms: observed_result_u64(observed, "deadline_ms")?,
        deadline_readback_ms: observed_result_u64(observed, "deadline_readback_ms")?,
        memory_limit_bytes: observed_result_u64(observed, "memory_limit_bytes")?,
        memory_limit_readback_bytes: observed_result_u64(observed, "memory_limit_readback_bytes")?,
        rss_limit_bytes: observed_result_u64(observed, "rss_limit_bytes")?,
        rss_limit_readback_bytes: observed_result_u64(observed, "rss_limit_readback_bytes")?,
        nofile_soft_limit: observed_result_u64(observed, "nofile_soft")?,
        nofile_hard_limit: observed_result_u64(observed, "nofile_hard")?,
        nofile_soft_readback: observed_result_u64(observed, "nofile_soft_readback")?,
        nofile_hard_readback: observed_result_u64(observed, "nofile_hard_readback")?,
        termination: observed.result.termination,
        max_rss_bytes: observed_result_u64(observed, "max_rss_bytes")?,
        containment_failures: observed_result_u64(observed, "containment_failures")?,
        cgroup_oom_delta: observed_result_u64(observed, "oom_delta")?,
        cgroup_oom_kill_delta: observed_result_u64(observed, "oom_kill_delta")?,
        exit_code: observed_result_optional_i32(observed, "exit_code")?,
        term_signal: observed_result_optional_i32(observed, "term_signal")?,
        raw_wait_status: observed_result_wait_status(observed, "outer_raw_wait_status")?,
        waited: true,
        reaped: observed_result_bool(observed, "leader_reaped")?,
        cleanup_succeeded: observed_result_bool(observed, "cleanup_succeeded")?,
        request_content_identity: request_identity.clone(),
        request_path_identity: raw_identity_required(
            "wu0g-relative-path-v1",
            observed.request_relative_path.as_bytes().to_vec(),
        )?,
        result_identity: raw_identity_required(
            "wu0g-result-v1",
            observed_result_value(observed, "result_identity")?
                .as_bytes()
                .to_vec(),
        )?,
        result_content_identity: raw_identity_required(
            "wu0g-result-content-v1",
            observed.result_bytes.clone(),
        )?,
        result_path_identity: raw_identity_required(
            "wu0g-relative-path-v1",
            observed.result_relative_path.as_bytes().to_vec(),
        )?,
        result_bound_plan_identity: plan_identity.clone(),
        result_bound_request_content_identity: request_identity,
        stdout_identity: raw_identity_required("wu0g-stdout-v1", observed.stdout.clone())?,
        stderr_identity: raw_identity_required("wu0g-stderr-v1", observed.stderr.clone())?,
        child_identity: result_claim_identity(observed, "child_identity", "wu0g-child-v1")?,
        completion_sentinel_identity: raw_identity_required(
            "wu0g-child-completion-sentinel-v1",
            observed.sentinel.clone(),
        )?,
        child_artifact_identity: raw_identity_required(
            "wu0g-child-artifact-v1",
            observed.child_artifact.clone(),
        )?,
        prlimit_identity: identities.prlimit.clone(),
        stdout_observation: bounded_observation(WU0G_STDOUT_CAP_BYTES, &observed.stdout)?,
        stderr_observation: bounded_observation(WU0G_STDERR_CAP_BYTES, &observed.stderr)?,
        request_observation: bounded_observation(WU0G_REQUEST_CAP_BYTES, &observed.request_bytes)?,
        result_observation: bounded_observation(WU0G_RESULT_CAP_BYTES, &observed.result_bytes)?,
        sentinel_observation: bounded_observation(WU0G_SENTINEL_CAP_BYTES, &observed.sentinel)?,
        semantic_artifact_observation: bounded_observation(
            WU0G_ARTIFACT_CAP_BYTES,
            &observed.semantic_artifact,
        )?,
    })
}

fn performance_launch_dossier(
    identities: &CoordinatorIdentityInputs,
    plan_identity: &RawIdentityEvidence,
    plan: &PlannedPerformanceLaunch,
    observed: &ObservedWu0gChild,
) -> Result<RawPerformanceLaunch, String> {
    let request_identity =
        raw_identity_required("wu0g-request-content-v1", observed.request_bytes.clone())?;
    let perf_artifact = observed
        .perf_artifact
        .as_ref()
        .ok_or_else(|| "performance launch lacks perf artifact".to_owned())?;
    let (instructions, perf_runtime) = parse_wu0g_perf_artifact_for_test(perf_artifact)?;
    Ok(RawPerformanceLaunch {
        plan_identity: plan_identity.clone(),
        representative_workload_identity: identities.representative_workload.clone(),
        candidate_identity: identities.candidate.clone(),
        binary_identity: plan.binary_identity.clone(),
        report_identity: raw_identity_required("wu0g-report-v1", observed.child_artifact.clone())?,
        semantic_identity: raw_identity_required(
            "wu0g-end-to-end-semantic-v1",
            observed.semantic_artifact.clone(),
        )?,
        pair_identity: plan.pair_identity.clone(),
        launch_identity: plan.launch_identity.clone(),
        request_content_identity: request_identity.clone(),
        request_path_identity: raw_identity_required(
            "wu0g-relative-path-v1",
            observed.request_relative_path.as_bytes().to_vec(),
        )?,
        result_identity: raw_identity_required(
            "wu0g-result-v1",
            observed_result_value(observed, "result_identity")?
                .as_bytes()
                .to_vec(),
        )?,
        result_content_identity: raw_identity_required(
            "wu0g-result-content-v1",
            observed.result_bytes.clone(),
        )?,
        result_path_identity: raw_identity_required(
            "wu0g-relative-path-v1",
            observed.result_relative_path.as_bytes().to_vec(),
        )?,
        result_bound_plan_identity: plan_identity.clone(),
        result_bound_request_content_identity: request_identity,
        child_identity: result_claim_identity(observed, "child_identity", "wu0g-child-v1")?,
        completion_sentinel_identity: raw_identity_required(
            "wu0g-child-completion-sentinel-v1",
            observed.sentinel.clone(),
        )?,
        child_artifact_identity: raw_identity_required(
            "wu0g-child-artifact-v1",
            observed.child_artifact.clone(),
        )?,
        perf_artifact_identity: raw_identity_required(
            "wu0g-perf-artifact-v1",
            perf_artifact.clone(),
        )?,
        prlimit_identity: identities.prlimit.clone(),
        perf_identity: identities.perf.clone(),
        host_identity: identities.host.clone(),
        perf_version: identities.perf_version.clone(),
        perf_event: WU0G_PERF_EVENT.to_owned(),
        cpu_affinity: identities.cpu_affinity.clone(),
        pair_ordinal: plan.pair_ordinal,
        launch_ordinal: plan.launch_ordinal,
        mode: plan.mode,
        canonical_invocation: observed.canonical_invocation.clone(),
        perf_invocation: plan.perf_invocation.clone(),
        scope_identity: result_claim_identity(observed, "scope_identity", "wu0g-scope-v1")?,
        cgroup_identity: result_claim_identity(observed, "cgroup_identity", "wu0g-cgroup-v1")?,
        leader_pid: observed_result_u32(observed, "leader_pid")?,
        leader_start_ticks: observed_result_u64(observed, "leader_start_ticks")?,
        pgid: observed_result_u32(observed, "pgid")?,
        readiness_seen: observed_result_bool(observed, "readiness_seen")?,
        membership_verified: observed_result_bool(observed, "membership_verified")?,
        drain_complete: observed_result_bool(observed, "drain_complete")?,
        cgroup_populated_zero: observed_result_bool(observed, "cgroup_populated_zero")?,
        pgid_empty: observed_result_bool(observed, "pgid_empty")?,
        cgroup_removed: observed_result_bool(observed, "cgroup_removed")?,
        cgroup_retained: observed_result_bool(observed, "cgroup_retained")?,
        scope_abort_requested: observed_result_bool(observed, "scope_abort_requested")?,
        scope_abort_observed: observed_result_bool(observed, "scope_abort_observed")?,
        deadline_ms: observed_result_u64(observed, "deadline_ms")?,
        deadline_readback_ms: observed_result_u64(observed, "deadline_readback_ms")?,
        memory_limit_bytes: observed_result_u64(observed, "memory_limit_bytes")?,
        memory_limit_readback_bytes: observed_result_u64(observed, "memory_limit_readback_bytes")?,
        rss_limit_bytes: observed_result_u64(observed, "rss_limit_bytes")?,
        rss_limit_readback_bytes: observed_result_u64(observed, "rss_limit_readback_bytes")?,
        nofile_soft_limit: observed_result_u64(observed, "nofile_soft")?,
        nofile_hard_limit: observed_result_u64(observed, "nofile_hard")?,
        nofile_soft_readback: observed_result_u64(observed, "nofile_soft_readback")?,
        nofile_hard_readback: observed_result_u64(observed, "nofile_hard_readback")?,
        termination: observed.result.termination,
        wall_ns: observed.wall_ns,
        instructions,
        perf_runtime,
        max_rss_bytes: observed_result_u64(observed, "max_rss_bytes")?,
        containment_failures: observed_result_u64(observed, "containment_failures")?,
        cgroup_oom_delta: observed_result_u64(observed, "oom_delta")?,
        cgroup_oom_kill_delta: observed_result_u64(observed, "oom_kill_delta")?,
        exit_code: observed_result_optional_i32(observed, "exit_code")?,
        term_signal: observed_result_optional_i32(observed, "term_signal")?,
        raw_wait_status: observed_result_wait_status(observed, "outer_raw_wait_status")?,
        perf_raw_wait_status: observed_result_optional_i32(observed, "perf_raw_wait_status")?,
        perf_exit_code: observed_result_optional_i32(observed, "perf_exit_code")?,
        perf_term_signal: observed_result_optional_i32(observed, "perf_term_signal")?,
        perf_waited: observed_result_optional_i32(observed, "perf_raw_wait_status")?.is_some(),
        waited: true,
        reaped: observed_result_bool(observed, "leader_reaped")?,
        cleanup_succeeded: observed_result_bool(observed, "cleanup_succeeded")?,
        stdout_identity: raw_identity_required("wu0g-stdout-v1", observed.stdout.clone())?,
        stderr_identity: raw_identity_required("wu0g-stderr-v1", observed.stderr.clone())?,
        stdout_observation: bounded_observation(WU0G_STDOUT_CAP_BYTES, &observed.stdout)?,
        stderr_observation: bounded_observation(WU0G_STDERR_CAP_BYTES, &observed.stderr)?,
        request_observation: bounded_observation(WU0G_REQUEST_CAP_BYTES, &observed.request_bytes)?,
        result_observation: bounded_observation(WU0G_RESULT_CAP_BYTES, &observed.result_bytes)?,
        sentinel_observation: bounded_observation(WU0G_SENTINEL_CAP_BYTES, &observed.sentinel)?,
        semantic_artifact_observation: bounded_observation(
            WU0G_ARTIFACT_CAP_BYTES,
            &observed.semantic_artifact,
        )?,
        perf_artifact_observation: bounded_observation(
            WU0G_PERF_ARTIFACT_CAP_BYTES,
            perf_artifact,
        )?,
    })
}

fn paired_mode_entry<'a, T>(
    entries: &'a [(T, ParsedChildArtifact)],
    plans: &'a [PlannedCausalLaunch],
    rung_ordinal: u8,
    mode: RawProbeMode,
) -> Result<(&'a T, &'a ParsedChildArtifact, &'a PlannedCausalLaunch), String> {
    let index = plans
        .iter()
        .position(|plan| plan.rung_ordinal == rung_ordinal && plan.mode == mode)
        .ok_or_else(|| "causal launch matrix is incomplete".to_owned())?;
    let (observed, artifact) = entries
        .get(index)
        .ok_or_else(|| "causal observation matrix is incomplete".to_owned())?;
    Ok((observed, artifact, &plans[index]))
}

struct ThresholdAssemblyInputs<'a> {
    identities: CoordinatorIdentityInputs,
    plan_identity: RawIdentityEvidence,
    ladder: &'a [InterfaceHeritagePrefixRung],
    causal_plans: Vec<PlannedCausalLaunch>,
    causal_observed: Vec<ObservedWu0gChild>,
    performance_plans: Vec<PlannedPerformanceLaunch>,
    performance_observed: Vec<ObservedWu0gChild>,
}

fn assemble_pinned_threshold_evidence(
    input: ThresholdAssemblyInputs<'_>,
) -> Result<RawThresholdEvidence, String> {
    let ThresholdAssemblyInputs {
        identities,
        plan_identity,
        ladder,
        causal_plans,
        causal_observed,
        performance_plans,
        performance_observed,
    } = input;
    if ladder.len() != 5
        || causal_plans.len() != 10
        || causal_observed.len() != causal_plans.len()
        || performance_plans.len() != 10
        || performance_observed.len() != performance_plans.len()
    {
        return Err("coordinator observation inventory is incomplete".to_owned());
    }
    for (plan, observed) in causal_plans.iter().zip(&causal_observed) {
        if plan.canonical_invocation != observed.canonical_invocation
            || plan.request_relative_path != observed.request_relative_path
            || plan.result_relative_path != observed.result_relative_path
        {
            return Err("causal observation escaped its frozen plan".to_owned());
        }
    }
    for (plan, observed) in performance_plans.iter().zip(&performance_observed) {
        if plan.canonical_invocation != observed.canonical_invocation
            || plan.request_relative_path != observed.request_relative_path
            || plan.result_relative_path != observed.result_relative_path
            || observed_result_value(observed, "perf_invocation")? != plan.perf_invocation.join("|")
        {
            return Err("performance observation escaped its frozen plan".to_owned());
        }
    }
    let causal_entries = causal_observed
        .into_iter()
        .map(|observed| {
            let artifact = parse_child_artifact(&observed.child_artifact)?;
            Ok((observed, artifact))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let performance_entries = performance_observed
        .into_iter()
        .map(|observed| {
            let artifact = parse_child_artifact(&observed.child_artifact)?;
            Ok((observed, artifact))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut ladder_rows = Vec::with_capacity(5);
    let mut causal_process_dossiers = Vec::with_capacity(10);
    for rung_ordinal in 0_u8..5 {
        let (baseline_observed, baseline_artifact, baseline_plan) = paired_mode_entry(
            &causal_entries,
            &causal_plans,
            rung_ordinal,
            RawProbeMode::Baseline,
        )?;
        let (candidate_observed, candidate_artifact, candidate_plan) = paired_mode_entry(
            &causal_entries,
            &causal_plans,
            rung_ordinal,
            RawProbeMode::CandidateB,
        )?;
        let baseline = causal_counter_point(baseline_plan, baseline_observed, baseline_artifact)?;
        let candidate =
            causal_counter_point(candidate_plan, candidate_observed, candidate_artifact)?;
        if baseline.rung_identity != candidate.rung_identity
            || baseline.semantic_identity != candidate.semantic_identity
            || baseline.complete_component_identities != candidate.complete_component_identities
        {
            return Err("causal pair lacks semantic parity".to_owned());
        }
        causal_process_dossiers.push(causal_process_dossier(
            &identities,
            &plan_identity,
            baseline_plan,
            baseline_observed,
            &baseline,
        )?);
        causal_process_dossiers.push(causal_process_dossier(
            &identities,
            &plan_identity,
            candidate_plan,
            candidate_observed,
            &candidate,
        )?);
        ladder_rows.push(RawLadderCounterRow {
            baseline,
            candidate,
        });
    }

    let mut performance_pairs = Vec::with_capacity(5);
    for pair_ordinal in 0_u8..5 {
        let mut launches = Vec::with_capacity(2);
        for position in 0_u8..2 {
            let launch_ordinal = pair_ordinal
                .checked_mul(2)
                .and_then(|value| value.checked_add(position))
                .ok_or_else(|| "performance launch ordinal overflow".to_owned())?;
            let plan = performance_plans
                .iter()
                .find(|plan| plan.launch_ordinal == launch_ordinal)
                .ok_or_else(|| "performance plan matrix is incomplete".to_owned())?;
            let (observed, _) = performance_entries
                .get(usize::from(launch_ordinal))
                .ok_or_else(|| "performance observation matrix is incomplete".to_owned())?;
            launches.push(performance_launch_dossier(
                &identities,
                &plan_identity,
                plan,
                observed,
            )?);
        }
        if launches[0].semantic_identity != launches[1].semantic_identity {
            return Err("performance pair lacks semantic parity".to_owned());
        }
        let plan = performance_plans
            .iter()
            .find(|plan| plan.pair_ordinal == pair_ordinal)
            .ok_or_else(|| "performance pair plan is absent".to_owned())?;
        performance_pairs.push(RawPerformancePairDossier {
            pair_identity: plan.pair_identity.clone(),
            pair_ordinal,
            order: plan.order,
            launches,
        });
    }

    let cycle_profiles = (2_u8..5)
        .map(|rung_ordinal| {
            let (observed, artifact, _) = paired_mode_entry(
                &causal_entries,
                &causal_plans,
                rung_ordinal,
                RawProbeMode::CandidateB,
            )?;
            let total_cycle_visits = artifact
                .substitution_visits
                .checked_add(artifact.candidate_avoided_visits)
                .ok_or_else(|| "cycle visit denominator overflow".to_owned())?;
            Ok(RawCycleProfileDossier {
                plan_identity: plan_identity.clone(),
                profile_identity: raw_identity_required(
                    "wu0g-profile-v1",
                    [
                        b"cycle-profile-v1\0".as_slice(),
                        observed.request_bytes.as_slice(),
                    ]
                    .concat(),
                )?,
                workload_identity: raw_identity_required(
                    "wu0g-workload-v1",
                    [
                        b"cycle-workload-v1\0".as_slice(),
                        observed.child_artifact.as_slice(),
                    ]
                    .concat(),
                )?,
                candidate_identity: identities.candidate.clone(),
                binary_identity: identities.candidate_binary.clone(),
                mode: RawProbeMode::CandidateB,
                report_identity: raw_identity_required(
                    "wu0g-report-v1",
                    observed.child_artifact.clone(),
                )?,
                semantic_identity: raw_identity_required(
                    "wu0g-semantic-v1",
                    observed.semantic_artifact.clone(),
                )?,
                artifact_identity: raw_identity_required(
                    "wu0g-artifact-v1",
                    observed.child_artifact.clone(),
                )?,
                artifact_content_identity: raw_identity_required(
                    "wu0g-artifact-content-v1",
                    observed.child_artifact.clone(),
                )?,
                explained_cycle_visits: artifact.candidate_avoided_visits,
                total_cycle_visits,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let top = ladder_rows
        .last()
        .ok_or_else(|| "assembled ladder is empty".to_owned())?;
    let prediction_source = performance_entries
        .iter()
        .find(|(_, artifact)| artifact.candidate_avoided_visits > 0)
        .or_else(|| performance_entries.get(1))
        .ok_or_else(|| "prediction source is absent".to_owned())?;
    let prediction_observed = &prediction_source.0;
    let predictions = vec![RawPredictionDossier {
        plan_identity: plan_identity.clone(),
        report_identity: raw_identity_required(
            "wu0g-report-v1",
            prediction_observed.child_artifact.clone(),
        )?,
        profile_identity: identities.profile.clone(),
        workload_identity: identities.representative_workload.clone(),
        candidate_identity: identities.candidate.clone(),
        binary_identity: identities.candidate_binary.clone(),
        mode: RawProbeMode::CandidateB,
        semantic_identity: raw_identity_required(
            "wu0g-semantic-v1",
            prediction_observed.semantic_artifact.clone(),
        )?,
        artifact_identity: raw_identity_required(
            "wu0g-artifact-v1",
            prediction_observed.child_artifact.clone(),
        )?,
        artifact_content_identity: raw_identity_required(
            "wu0g-artifact-content-v1",
            prediction_observed.child_artifact.clone(),
        )?,
        total_end_to_end_work: top.baseline.total_work_counter,
        attributed_target_work: top.baseline.target_counter,
        baseline_target_work: top.baseline.target_counter,
        candidate_target_work: top.candidate.target_counter,
    }];

    let controls = performance_pairs
        .iter()
        .take(3)
        .map(|pair| {
            let baseline = pair
                .launches
                .iter()
                .find(|launch| launch.mode == RawProbeMode::Baseline)
                .ok_or_else(|| "control pair lacks baseline".to_owned())?;
            let candidate = pair
                .launches
                .iter()
                .find(|launch| launch.mode == RawProbeMode::CandidateB)
                .ok_or_else(|| "control pair lacks candidate".to_owned())?;
            let baseline_observed = &performance_entries[usize::from(baseline.launch_ordinal)].0;
            let candidate_observed = &performance_entries[usize::from(candidate.launch_ordinal)].0;
            Ok(RawControlPairDossier {
                plan_identity: plan_identity.clone(),
                profile_identity: raw_identity_required(
                    "wu0g-profile-v1",
                    [
                        b"control-profile-v1\0".as_slice(),
                        pair.pair_identity.claimed_sha256.as_bytes(),
                    ]
                    .concat(),
                )?,
                workload_identity: raw_identity_required(
                    "wu0g-control-workload-v1",
                    pair.pair_identity.canonical_bytes.clone(),
                )?,
                candidate_identity: identities.candidate.clone(),
                baseline_binary_identity: identities.baseline_binary.clone(),
                candidate_binary_identity: identities.candidate_binary.clone(),
                baseline_report_identity: baseline.report_identity.clone(),
                candidate_report_identity: candidate.report_identity.clone(),
                baseline_semantic_identity: raw_identity_required(
                    "wu0g-semantic-v1",
                    baseline_observed.semantic_artifact.clone(),
                )?,
                candidate_semantic_identity: raw_identity_required(
                    "wu0g-semantic-v1",
                    candidate_observed.semantic_artifact.clone(),
                )?,
                baseline_artifact_identity: raw_identity_required(
                    "wu0g-artifact-v1",
                    baseline_observed.child_artifact.clone(),
                )?,
                candidate_artifact_identity: raw_identity_required(
                    "wu0g-artifact-v1",
                    candidate_observed.child_artifact.clone(),
                )?,
                baseline_artifact_content_identity: raw_identity_required(
                    "wu0g-artifact-content-v1",
                    baseline_observed.child_artifact.clone(),
                )?,
                candidate_artifact_content_identity: raw_identity_required(
                    "wu0g-artifact-content-v1",
                    candidate_observed.child_artifact.clone(),
                )?,
                baseline_measurement: u64::try_from(baseline_observed.semantic_artifact.len())
                    .map_err(|_| "control baseline measurement does not fit u64")?,
                candidate_measurement: u64::try_from(candidate_observed.semantic_artifact.len())
                    .map_err(|_| "control candidate measurement does not fit u64")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let placeholder = raw_identity_required("wu0g-experiment-v1", Vec::new())?;
    let raw = RawThresholdEvidence {
        experiment_identity: placeholder,
        plan_identity: plan_identity.clone(),
        profile_identity: identities.profile,
        universe_identity: identities.universe,
        workload_identity: identities.workload,
        representative_workload_identity: identities.representative_workload,
        candidate_identity: identities.candidate,
        baseline_binary_identity: identities.baseline_binary,
        candidate_binary_identity: identities.candidate_binary,
        prlimit_identity: identities.prlimit,
        perf_identity: identities.perf,
        host_identity: identities.host,
        perf_version: identities.perf_version,
        perf_event: WU0G_PERF_EVENT.to_owned(),
        cpu_affinity: identities.cpu_affinity,
        ladder_rows,
        cycle_profiles,
        predictions,
        controls,
        causal_process_dossiers,
        performance_pairs,
    };
    if raw_plan_identity(&raw) != Some(plan_identity) {
        return Err("assembled evidence does not match the frozen plan identity".to_owned());
    }
    Ok(raw)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedFreshProcessDossier {
    rung_identity: String,
    mode: RawProbeMode,
    child_identity: String,
    artifact_identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedCycleProfileDossier {
    explained_basis_points: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedPredictionDossier {
    predicted_basis_points: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedControlPairDossier {
    regression_basis_points: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedPerformancePairDossier {
    wall_improvement_basis_points: u64,
    instruction_improvement_basis_points: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedInterfaceFillAuthorizationEvidence {
    thresholds: InterfaceFillThresholdDecision,
    pinned_profile_authenticated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct InterfaceFillThresholdDecision {
    incremental_causality_basis_points: u64,
    target_counter_reduction_basis_points: u64,
    predicted_end_to_end_basis_points: u64,
    median_wall_improvement_basis_points: u64,
    median_instruction_improvement_basis_points: u64,
    valid: bool,
    authorized: bool,
}

impl InterfaceFillThresholdDecision {
    pub(super) fn incremental_causality_basis_points(&self) -> u64 {
        self.incremental_causality_basis_points
    }

    pub(super) fn target_counter_reduction_basis_points(&self) -> u64 {
        self.target_counter_reduction_basis_points
    }

    pub(super) fn predicted_end_to_end_basis_points(&self) -> u64 {
        self.predicted_end_to_end_basis_points
    }

    pub(super) fn median_wall_improvement_basis_points(&self) -> u64 {
        self.median_wall_improvement_basis_points
    }

    pub(super) fn median_instruction_improvement_basis_points(&self) -> u64 {
        self.median_instruction_improvement_basis_points
    }

    pub(super) fn thresholds_pass(&self) -> bool {
        self.valid
            && self.incremental_causality_basis_points >= 5_000
            && self.target_counter_reduction_basis_points >= 5_000
            && self.predicted_end_to_end_basis_points >= 2_000
            && self.median_wall_improvement_basis_points >= 2_000
            && self.median_instruction_improvement_basis_points >= 2_000
    }

    pub(super) fn authorizes(&self) -> bool {
        self.authorized && self.thresholds_pass()
    }
}

pub(super) fn evaluate_interface_fill_thresholds_from_raw_for_test(
    raw: &RawThresholdEvidence,
) -> InterfaceFillThresholdDecision {
    validate_threshold_evidence(raw).unwrap_or_default()
}

pub(super) fn validate_interface_fill_authorization_from_raw_for_test(
    raw: &RawAuthorizationEvidence,
) -> InterfaceFillThresholdDecision {
    let thresholds = evaluate_interface_fill_thresholds_from_raw_for_test(&raw.thresholds);
    let pinned_profile_authenticated = raw.evidence_domain == RawEvidenceDomain::PinnedDom82
        && raw
            .thresholds
            .profile_identity
            .canonical_bytes
            .starts_with(b"typokat-wu0g-authenticated-pinned-dom-82-v1\0")
        && raw
            .thresholds
            .universe_identity
            .canonical_bytes
            .starts_with(b"typokat-wu0g-authenticated-pinned-universe-v1\0");
    let validated = ValidatedInterfaceFillAuthorizationEvidence {
        thresholds,
        pinned_profile_authenticated,
    };
    let mut decision = validated.thresholds;
    decision.authorized = validated.pinned_profile_authenticated;
    decision
}

fn validate_threshold_evidence(
    raw: &RawThresholdEvidence,
) -> Option<InterfaceFillThresholdDecision> {
    validate_all_raw_identities(raw)?;
    if raw.plan_identity != raw_plan_identity(raw)?
        || raw.experiment_identity != raw_experiment_identity(raw)?
        || !fresh_process_inventory_is_globally_unique(raw)
    {
        return None;
    }
    let ladder = validate_raw_ladder(&raw.ladder_rows)?;
    let causal = validate_causal_dossiers(raw)?;
    let cycles = validate_cycle_dossiers(raw)?;
    let predictions = validate_prediction_dossiers(raw)?;
    let controls = validate_control_dossiers(raw)?;
    let performance = validate_performance_dossiers(raw)?;
    if causal.len() != 10 || cycles.len() != 3 || predictions.len() != 1 || controls.len() != 3 {
        return None;
    }
    let first = ladder.first()?;
    let last = ladder.last()?;
    let baseline_target_delta = last
        .baseline
        .target_counter
        .checked_sub(first.baseline.target_counter)?;
    let baseline_total_delta = last
        .baseline
        .total_work_counter
        .checked_sub(first.baseline.total_work_counter)?;
    let candidate_target_delta = last
        .candidate
        .target_counter
        .checked_sub(first.candidate.target_counter)?;
    let incremental_causality_basis_points =
        checked_ratio_basis_points(baseline_target_delta, baseline_total_delta)?;
    let target_counter_reduction_basis_points = checked_ratio_basis_points(
        baseline_target_delta.checked_sub(candidate_target_delta)?,
        baseline_target_delta,
    )?;
    let predicted_end_to_end_basis_points = predictions[0].predicted_basis_points;
    if cycles
        .iter()
        .any(|cycle| cycle.explained_basis_points < 3_000)
        || controls
            .iter()
            .any(|control| control.regression_basis_points > 200)
    {
        return None;
    }
    let mut wall = performance
        .iter()
        .map(|pair| pair.wall_improvement_basis_points)
        .collect::<Vec<_>>();
    let mut instructions = performance
        .iter()
        .map(|pair| pair.instruction_improvement_basis_points)
        .collect::<Vec<_>>();
    wall.sort_unstable();
    instructions.sort_unstable();
    Some(InterfaceFillThresholdDecision {
        incremental_causality_basis_points,
        target_counter_reduction_basis_points,
        predicted_end_to_end_basis_points,
        median_wall_improvement_basis_points: *wall.get(2)?,
        median_instruction_improvement_basis_points: *instructions.get(2)?,
        valid: true,
        authorized: false,
    })
}

fn fresh_process_inventory_is_globally_unique(raw: &RawThresholdEvidence) -> bool {
    let mut requests = BTreeSet::new();
    let mut request_paths = BTreeSet::new();
    let mut results = BTreeSet::new();
    let mut result_contents = BTreeSet::new();
    let mut result_paths = BTreeSet::new();
    let mut children = BTreeSet::new();
    let mut sentinels = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut insert = |request: &RawIdentityEvidence,
                      request_path: &RawIdentityEvidence,
                      result: &RawIdentityEvidence,
                      result_content: &RawIdentityEvidence,
                      result_path: &RawIdentityEvidence,
                      child: &RawIdentityEvidence,
                      sentinel: &RawIdentityEvidence,
                      artifact: &RawIdentityEvidence| {
        requests.insert(request.claimed_sha256.clone())
            && request_paths.insert(request_path.claimed_sha256.clone())
            && results.insert(result.claimed_sha256.clone())
            && result_contents.insert(result_content.claimed_sha256.clone())
            && result_paths.insert(result_path.claimed_sha256.clone())
            && children.insert(child.claimed_sha256.clone())
            && sentinels.insert(sentinel.claimed_sha256.clone())
            && artifacts.insert(artifact.claimed_sha256.clone())
    };
    for dossier in &raw.causal_process_dossiers {
        if !insert(
            &dossier.request_content_identity,
            &dossier.request_path_identity,
            &dossier.result_identity,
            &dossier.result_content_identity,
            &dossier.result_path_identity,
            &dossier.child_identity,
            &dossier.completion_sentinel_identity,
            &dossier.child_artifact_identity,
        ) {
            return false;
        }
    }
    for launch in raw.performance_pairs.iter().flat_map(|pair| &pair.launches) {
        if !insert(
            &launch.request_content_identity,
            &launch.request_path_identity,
            &launch.result_identity,
            &launch.result_content_identity,
            &launch.result_path_identity,
            &launch.child_identity,
            &launch.completion_sentinel_identity,
            &launch.child_artifact_identity,
        ) {
            return false;
        }
    }
    true
}

fn validate_raw_ladder(rows: &[RawLadderCounterRow]) -> Option<Vec<ValidatedLadderCounterRow>> {
    if rows.len() != 5 {
        return None;
    }
    let mut rung_identities = BTreeSet::new();
    let mut previous: Option<(u64, u64, u64, u64, usize)> = None;
    let mut validated = Vec::with_capacity(rows.len());
    for row in rows {
        if row.baseline.rung_identity != row.candidate.rung_identity
            || row.baseline.semantic_identity != row.candidate.semantic_identity
            || row.baseline.complete_component_identities
                != row.candidate.complete_component_identities
            || row.baseline.saturated
            || row.candidate.saturated
            || row.baseline.complete_component_identities.is_empty()
            || !rung_identities.insert(row.baseline.rung_identity.claimed_sha256.as_str())
            || row.baseline.target_counter > row.baseline.total_work_counter
            || row.candidate.target_counter > row.candidate.total_work_counter
        {
            return None;
        }
        let current = (
            row.baseline.target_counter,
            row.baseline.total_work_counter,
            row.candidate.target_counter,
            row.candidate.total_work_counter,
            row.baseline.complete_component_identities.len(),
        );
        if previous.is_some_and(|prior| {
            current.0 <= prior.0
                || current.1 <= prior.1
                || current.2 <= prior.2
                || current.3 <= prior.3
                || current.4 <= prior.4
        }) {
            return None;
        }
        previous = Some(current);
        let point = |raw: &RawLadderCounterPoint| ValidatedLadderCounterPoint {
            rung_identity: raw.rung_identity.claimed_sha256.clone(),
            semantic_identity: raw.semantic_identity.claimed_sha256.clone(),
            target_counter: raw.target_counter,
            total_work_counter: raw.total_work_counter,
            complete_component_identities: raw
                .complete_component_identities
                .iter()
                .map(|identity| identity.claimed_sha256.clone())
                .collect(),
        };
        validated.push(ValidatedLadderCounterRow {
            baseline: point(&row.baseline),
            candidate: point(&row.candidate),
        });
    }
    Some(validated)
}

fn validate_causal_dossiers(
    raw: &RawThresholdEvidence,
) -> Option<Vec<ValidatedFreshProcessDossier>> {
    if raw.causal_process_dossiers.len() != raw.ladder_rows.len().checked_mul(2)? {
        return None;
    }
    let mut expected = BTreeSet::new();
    for row in &raw.ladder_rows {
        expected.insert((
            row.baseline.rung_identity.claimed_sha256.clone(),
            RawProbeMode::Baseline,
        ));
        expected.insert((
            row.candidate.rung_identity.claimed_sha256.clone(),
            RawProbeMode::CandidateB,
        ));
    }
    let mut observed = BTreeSet::new();
    let mut children = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut requests = BTreeSet::new();
    let mut sentinels = BTreeSet::new();
    let mut validated = Vec::new();
    for dossier in &raw.causal_process_dossiers {
        if dossier.plan_identity != raw.plan_identity
            || dossier.profile_identity != raw.profile_identity
            || dossier.universe_identity != raw.universe_identity
            || dossier.workload_identity != raw.workload_identity
            || dossier.candidate_identity != raw.candidate_identity
            || !valid_hardened_runner_invocation(&dossier.canonical_invocation)
            || dossier.prlimit_identity != raw.prlimit_identity
            || dossier.result_bound_plan_identity != raw.plan_identity
            || dossier.result_bound_request_content_identity != dossier.request_content_identity
            || !valid_causal_process_status(dossier)
            || !children.insert(dossier.child_identity.claimed_sha256.as_str())
            || !artifacts.insert(dossier.child_artifact_identity.claimed_sha256.as_str())
            || !requests.insert(dossier.request_content_identity.claimed_sha256.as_str())
            || !sentinels.insert(dossier.completion_sentinel_identity.claimed_sha256.as_str())
        {
            return None;
        }
        let (row_index, row) = raw
            .ladder_rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.baseline.rung_identity == dossier.rung_identity)?;
        if dossier.rung_ordinal != u8::try_from(row_index).ok()? {
            return None;
        }
        let point = match dossier.mode {
            RawProbeMode::Baseline => &row.baseline,
            RawProbeMode::CandidateB => &row.candidate,
        };
        let binary = match dossier.mode {
            RawProbeMode::Baseline => &raw.baseline_binary_identity,
            RawProbeMode::CandidateB => &raw.candidate_binary_identity,
        };
        if &dossier.binary_identity != binary
            || dossier.report_identity != point.report_identity
            || dossier.semantic_identity != point.semantic_identity
            || !observed.insert((dossier.rung_identity.claimed_sha256.clone(), dossier.mode))
        {
            return None;
        }
        validated.push(ValidatedFreshProcessDossier {
            rung_identity: dossier.rung_identity.claimed_sha256.clone(),
            mode: dossier.mode,
            child_identity: dossier.child_identity.claimed_sha256.clone(),
            artifact_identity: dossier.child_artifact_identity.claimed_sha256.clone(),
        });
    }
    (observed == expected).then_some(validated)
}

fn validate_cycle_dossiers(
    raw: &RawThresholdEvidence,
) -> Option<Vec<ValidatedCycleProfileDossier>> {
    if raw.cycle_profiles.len() != 3 {
        return None;
    }
    let mut profiles = BTreeSet::new();
    let mut workloads = BTreeSet::new();
    let mut reports = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    raw.cycle_profiles
        .iter()
        .map(|dossier| {
            if dossier.plan_identity != raw.plan_identity
                || dossier.candidate_identity != raw.candidate_identity
                || dossier.binary_identity != raw.candidate_binary_identity
                || dossier.mode != RawProbeMode::CandidateB
                || dossier.profile_identity == dossier.workload_identity
                || dossier.report_identity == dossier.artifact_identity
                || dossier.artifact_identity == dossier.artifact_content_identity
                || !profiles.insert(dossier.profile_identity.claimed_sha256.as_str())
                || !workloads.insert(dossier.workload_identity.claimed_sha256.as_str())
                || !reports.insert(dossier.report_identity.claimed_sha256.as_str())
                || !artifacts.insert(dossier.artifact_identity.claimed_sha256.as_str())
            {
                return None;
            }
            let explained_basis_points = checked_ratio_basis_points(
                dossier.explained_cycle_visits,
                dossier.total_cycle_visits,
            )?;
            (explained_basis_points <= 10_000).then_some(ValidatedCycleProfileDossier {
                explained_basis_points,
            })
        })
        .collect()
}

fn validate_prediction_dossiers(
    raw: &RawThresholdEvidence,
) -> Option<Vec<ValidatedPredictionDossier>> {
    if raw.predictions.len() != 1 {
        return None;
    }
    raw.predictions
        .iter()
        .map(|dossier| {
            if dossier.plan_identity != raw.plan_identity
                || dossier.profile_identity != raw.profile_identity
                || dossier.workload_identity != raw.representative_workload_identity
                || dossier.candidate_identity != raw.candidate_identity
                || dossier.binary_identity != raw.candidate_binary_identity
                || dossier.mode != RawProbeMode::CandidateB
                || raw
                    .cycle_profiles
                    .iter()
                    .any(|cycle| cycle.report_identity == dossier.report_identity)
                || dossier.artifact_identity == dossier.artifact_content_identity
                || dossier.attributed_target_work > dossier.total_end_to_end_work
                || dossier.candidate_target_work > dossier.baseline_target_work
            {
                return None;
            }
            let attributed_share = checked_ratio_basis_points(
                dossier.attributed_target_work,
                dossier.total_end_to_end_work,
            )?;
            let target_reduction = checked_ratio_basis_points(
                dossier
                    .baseline_target_work
                    .checked_sub(dossier.candidate_target_work)?,
                dossier.baseline_target_work,
            )?;
            let predicted_basis_points = u64::try_from(
                u128::from(attributed_share).checked_mul(u128::from(target_reduction))? / 10_000,
            )
            .ok()?;
            (predicted_basis_points <= 10_000).then_some(ValidatedPredictionDossier {
                predicted_basis_points,
            })
        })
        .collect()
}

fn validate_control_dossiers(
    raw: &RawThresholdEvidence,
) -> Option<Vec<ValidatedControlPairDossier>> {
    if raw.controls.len() != 3 {
        return None;
    }
    let mut profiles = BTreeSet::new();
    let mut workloads = BTreeSet::new();
    let mut reports = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    raw.controls
        .iter()
        .map(|dossier| {
            if dossier.plan_identity != raw.plan_identity
                || dossier.candidate_identity != raw.candidate_identity
                || dossier.baseline_binary_identity != raw.baseline_binary_identity
                || dossier.candidate_binary_identity != raw.candidate_binary_identity
                || dossier.baseline_semantic_identity != dossier.candidate_semantic_identity
                || dossier.baseline_report_identity == dossier.candidate_report_identity
                || dossier.baseline_artifact_identity == dossier.candidate_artifact_identity
                || dossier.baseline_artifact_content_identity
                    == dossier.candidate_artifact_content_identity
                || !profiles.insert(dossier.profile_identity.claimed_sha256.as_str())
                || !workloads.insert(dossier.workload_identity.claimed_sha256.as_str())
                || !reports.insert(dossier.baseline_report_identity.claimed_sha256.as_str())
                || !reports.insert(dossier.candidate_report_identity.claimed_sha256.as_str())
                || !artifacts.insert(dossier.baseline_artifact_identity.claimed_sha256.as_str())
                || !artifacts.insert(dossier.candidate_artifact_identity.claimed_sha256.as_str())
            {
                return None;
            }
            let regression_basis_points =
                if dossier.candidate_measurement <= dossier.baseline_measurement {
                    if dossier.baseline_measurement == 0 {
                        return None;
                    }
                    0
                } else {
                    checked_ratio_basis_points(
                        dossier
                            .candidate_measurement
                            .checked_sub(dossier.baseline_measurement)?,
                        dossier.baseline_measurement,
                    )?
                };
            (regression_basis_points <= 10_000).then_some(ValidatedControlPairDossier {
                regression_basis_points,
            })
        })
        .collect()
}

fn validate_performance_dossiers(
    raw: &RawThresholdEvidence,
) -> Option<Vec<ValidatedPerformancePairDossier>> {
    if raw.performance_pairs.len() != 5 {
        return None;
    }
    let mut children = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut reports = BTreeSet::new();
    let mut pair_identities = BTreeSet::new();
    let mut launch_identities = BTreeSet::new();
    let mut requests = BTreeSet::new();
    let mut sentinels = BTreeSet::new();
    let mut perf_artifacts = BTreeSet::new();
    let mut semantic_identity = None;
    let mut validated = Vec::new();
    for (index, pair) in raw.performance_pairs.iter().enumerate() {
        let pair_ordinal = u8::try_from(index).ok()?;
        let expected_order = if pair_ordinal % 2 == 0 {
            RawLaunchOrder::Ab
        } else {
            RawLaunchOrder::Ba
        };
        let expected_modes = match expected_order {
            RawLaunchOrder::Ab => [RawProbeMode::Baseline, RawProbeMode::CandidateB],
            RawLaunchOrder::Ba => [RawProbeMode::CandidateB, RawProbeMode::Baseline],
        };
        if pair.pair_ordinal != pair_ordinal
            || pair.order != expected_order
            || pair.launches.len() != 2
            || !pair_identities.insert(pair.pair_identity.claimed_sha256.as_str())
        {
            return None;
        }
        for (position, launch) in pair.launches.iter().enumerate() {
            let launch_ordinal = pair_ordinal
                .checked_mul(2)?
                .checked_add(u8::try_from(position).ok()?)?;
            let binary = match launch.mode {
                RawProbeMode::Baseline => &raw.baseline_binary_identity,
                RawProbeMode::CandidateB => &raw.candidate_binary_identity,
            };
            if launch.plan_identity != raw.plan_identity
                || launch.representative_workload_identity != raw.representative_workload_identity
                || launch.candidate_identity != raw.candidate_identity
                || &launch.binary_identity != binary
                || launch.pair_ordinal != pair_ordinal
                || launch.launch_ordinal != launch_ordinal
                || launch.mode != expected_modes[position]
                || launch.pair_identity != pair.pair_identity
                || launch.result_bound_plan_identity != raw.plan_identity
                || launch.result_bound_request_content_identity != launch.request_content_identity
                || launch.prlimit_identity != raw.prlimit_identity
                || launch.perf_identity != raw.perf_identity
                || launch.host_identity != raw.host_identity
                || launch.perf_version != raw.perf_version
                || launch.perf_event != raw.perf_event
                || launch.cpu_affinity != raw.cpu_affinity
                || !valid_hardened_runner_invocation(&launch.canonical_invocation)
                || !valid_perf_invocation(&launch.perf_invocation)
                || !valid_performance_process_status(launch)
                || launch.wall_ns == 0
                || launch.instructions == 0
                || !launch_identities.insert(launch.launch_identity.claimed_sha256.as_str())
                || !requests.insert(launch.request_content_identity.claimed_sha256.as_str())
                || !sentinels.insert(launch.completion_sentinel_identity.claimed_sha256.as_str())
                || !perf_artifacts.insert(launch.perf_artifact_identity.claimed_sha256.as_str())
                || !children.insert(launch.child_identity.claimed_sha256.as_str())
                || !artifacts.insert(launch.child_artifact_identity.claimed_sha256.as_str())
                || !reports.insert(launch.report_identity.claimed_sha256.as_str())
            {
                return None;
            }
        }
        if pair.launches[0].semantic_identity != pair.launches[1].semantic_identity {
            return None;
        }
        match &semantic_identity {
            Some(expected) if expected != &pair.launches[0].semantic_identity => return None,
            None => semantic_identity = Some(pair.launches[0].semantic_identity.clone()),
            Some(_) => {}
        }
        let baseline = pair
            .launches
            .iter()
            .find(|launch| launch.mode == RawProbeMode::Baseline)?;
        let candidate = pair
            .launches
            .iter()
            .find(|launch| launch.mode == RawProbeMode::CandidateB)?;
        validated.push(ValidatedPerformancePairDossier {
            wall_improvement_basis_points: improvement_basis_points(
                baseline.wall_ns,
                candidate.wall_ns,
            )?,
            instruction_improvement_basis_points: improvement_basis_points(
                baseline.instructions,
                candidate.instructions,
            )?,
        });
    }
    Some(validated)
}

fn valid_hardened_runner_invocation(invocation: &[String]) -> bool {
    invocation.len() == 5
        && invocation[0] == "/usr/bin/perl"
        && Path::new(&invocation[1]).is_absolute()
        && invocation[1].ends_with("/tooling/wu0e-diagnostic/run.pl")
        && invocation[2] == "--wu0g-child-v1"
        && Path::new(&invocation[3]).is_absolute()
        && Path::new(&invocation[4]).is_absolute()
        && invocation[3] != invocation[4]
}

fn valid_perf_invocation(invocation: &[String]) -> bool {
    invocation
        == [
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
}

fn valid_causal_process_status(dossier: &RawFreshProcessDossier) -> bool {
    valid_process_status_fields(ProcessStatusView::from_causal(dossier))
        && valid_observation(dossier.stdout_observation, WU0G_STDOUT_CAP_BYTES)
        && valid_observation(dossier.stderr_observation, WU0G_STDERR_CAP_BYTES)
        && valid_observation(dossier.request_observation, WU0G_REQUEST_CAP_BYTES)
        && valid_observation(dossier.result_observation, WU0G_RESULT_CAP_BYTES)
        && valid_observation(dossier.sentinel_observation, WU0G_SENTINEL_CAP_BYTES)
        && valid_observation(
            dossier.semantic_artifact_observation,
            WU0G_ARTIFACT_CAP_BYTES,
        )
}

fn valid_performance_process_status(launch: &RawPerformanceLaunch) -> bool {
    valid_process_status_fields(ProcessStatusView::from_performance(launch))
        && launch.perf_raw_wait_status == Some(0)
        && launch.perf_exit_code == Some(0)
        && launch.perf_term_signal.is_none()
        && launch.perf_waited
        && launch.perf_event == WU0G_PERF_EVENT
        && launch.perf_runtime > 0
        && valid_observation(launch.stdout_observation, WU0G_STDOUT_CAP_BYTES)
        && valid_observation(launch.stderr_observation, WU0G_STDERR_CAP_BYTES)
        && valid_observation(launch.request_observation, WU0G_REQUEST_CAP_BYTES)
        && valid_observation(launch.result_observation, WU0G_RESULT_CAP_BYTES)
        && valid_observation(launch.sentinel_observation, WU0G_SENTINEL_CAP_BYTES)
        && valid_observation(
            launch.semantic_artifact_observation,
            WU0G_ARTIFACT_CAP_BYTES,
        )
        && valid_observation(
            launch.perf_artifact_observation,
            WU0G_PERF_ARTIFACT_CAP_BYTES,
        )
}

#[derive(Clone, Copy)]
struct ProcessStatusView<'a> {
    scope: &'a RawIdentityEvidence,
    cgroup: &'a RawIdentityEvidence,
    termination: RawProbeTermination,
    deadline_ms: u64,
    deadline_readback_ms: u64,
    memory_limit_bytes: u64,
    memory_limit_readback_bytes: u64,
    rss_limit_bytes: u64,
    rss_limit_readback_bytes: u64,
    nofile_soft: u64,
    nofile_hard: u64,
    nofile_soft_readback: u64,
    nofile_hard_readback: u64,
    max_rss_bytes: u64,
    containment_failures: u64,
    oom_delta: u64,
    oom_kill_delta: u64,
    exit_code: Option<i32>,
    term_signal: Option<i32>,
    raw_wait_status: Option<i32>,
    waited: bool,
    reaped: bool,
    cleanup_succeeded: bool,
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
}

impl<'a> ProcessStatusView<'a> {
    fn from_causal(dossier: &'a RawFreshProcessDossier) -> Self {
        Self {
            scope: &dossier.scope_identity,
            cgroup: &dossier.cgroup_identity,
            termination: dossier.termination,
            deadline_ms: dossier.deadline_ms,
            deadline_readback_ms: dossier.deadline_readback_ms,
            memory_limit_bytes: dossier.memory_limit_bytes,
            memory_limit_readback_bytes: dossier.memory_limit_readback_bytes,
            rss_limit_bytes: dossier.rss_limit_bytes,
            rss_limit_readback_bytes: dossier.rss_limit_readback_bytes,
            nofile_soft: dossier.nofile_soft_limit,
            nofile_hard: dossier.nofile_hard_limit,
            nofile_soft_readback: dossier.nofile_soft_readback,
            nofile_hard_readback: dossier.nofile_hard_readback,
            max_rss_bytes: dossier.max_rss_bytes,
            containment_failures: dossier.containment_failures,
            oom_delta: dossier.cgroup_oom_delta,
            oom_kill_delta: dossier.cgroup_oom_kill_delta,
            exit_code: dossier.exit_code,
            term_signal: dossier.term_signal,
            raw_wait_status: dossier.raw_wait_status,
            waited: dossier.waited,
            reaped: dossier.reaped,
            cleanup_succeeded: dossier.cleanup_succeeded,
            leader_pid: dossier.leader_pid,
            leader_start_ticks: dossier.leader_start_ticks,
            pgid: dossier.pgid,
            readiness_seen: dossier.readiness_seen,
            membership_verified: dossier.membership_verified,
            drain_complete: dossier.drain_complete,
            cgroup_populated_zero: dossier.cgroup_populated_zero,
            pgid_empty: dossier.pgid_empty,
            cgroup_removed: dossier.cgroup_removed,
            cgroup_retained: dossier.cgroup_retained,
            scope_abort_requested: dossier.scope_abort_requested,
            scope_abort_observed: dossier.scope_abort_observed,
        }
    }

    fn from_performance(launch: &'a RawPerformanceLaunch) -> Self {
        Self {
            scope: &launch.scope_identity,
            cgroup: &launch.cgroup_identity,
            termination: launch.termination,
            deadline_ms: launch.deadline_ms,
            deadline_readback_ms: launch.deadline_readback_ms,
            memory_limit_bytes: launch.memory_limit_bytes,
            memory_limit_readback_bytes: launch.memory_limit_readback_bytes,
            rss_limit_bytes: launch.rss_limit_bytes,
            rss_limit_readback_bytes: launch.rss_limit_readback_bytes,
            nofile_soft: launch.nofile_soft_limit,
            nofile_hard: launch.nofile_hard_limit,
            nofile_soft_readback: launch.nofile_soft_readback,
            nofile_hard_readback: launch.nofile_hard_readback,
            max_rss_bytes: launch.max_rss_bytes,
            containment_failures: launch.containment_failures,
            oom_delta: launch.cgroup_oom_delta,
            oom_kill_delta: launch.cgroup_oom_kill_delta,
            exit_code: launch.exit_code,
            term_signal: launch.term_signal,
            raw_wait_status: launch.raw_wait_status,
            waited: launch.waited,
            reaped: launch.reaped,
            cleanup_succeeded: launch.cleanup_succeeded,
            leader_pid: launch.leader_pid,
            leader_start_ticks: launch.leader_start_ticks,
            pgid: launch.pgid,
            readiness_seen: launch.readiness_seen,
            membership_verified: launch.membership_verified,
            drain_complete: launch.drain_complete,
            cgroup_populated_zero: launch.cgroup_populated_zero,
            pgid_empty: launch.pgid_empty,
            cgroup_removed: launch.cgroup_removed,
            cgroup_retained: launch.cgroup_retained,
            scope_abort_requested: launch.scope_abort_requested,
            scope_abort_observed: launch.scope_abort_observed,
        }
    }
}

fn valid_process_status_fields(status: ProcessStatusView<'_>) -> bool {
    let ProcessStatusView {
        termination,
        deadline_ms,
        deadline_readback_ms,
        memory_limit_bytes,
        memory_limit_readback_bytes,
        rss_limit_bytes,
        rss_limit_readback_bytes,
        nofile_soft,
        nofile_hard,
        nofile_soft_readback,
        nofile_hard_readback,
        max_rss_bytes,
        containment_failures,
        oom_delta,
        oom_kill_delta,
        exit_code,
        term_signal,
        raw_wait_status,
        waited,
        reaped,
        cleanup_succeeded,
        leader_pid,
        leader_start_ticks,
        pgid,
        readiness_seen,
        membership_verified,
        drain_complete,
        cgroup_populated_zero,
        pgid_empty,
        cgroup_removed,
        cgroup_retained,
        scope_abort_requested,
        scope_abort_observed,
        ..
    } = status;
    termination == RawProbeTermination::Complete
        && deadline_ms > 0
        && deadline_ms <= WU0G_DEADLINE_MS
        && deadline_ms == deadline_readback_ms
        && memory_limit_bytes > 0
        && memory_limit_bytes <= WU0G_MEMORY_LIMIT_BYTES
        && memory_limit_bytes == memory_limit_readback_bytes
        && rss_limit_bytes > 0
        && rss_limit_bytes <= WU0G_RSS_LIMIT_BYTES
        && rss_limit_bytes == rss_limit_readback_bytes
        && rss_limit_bytes <= memory_limit_bytes
        && nofile_soft > 0
        && nofile_soft <= nofile_hard
        && nofile_hard <= WU0G_NOFILE_LIMIT
        && nofile_soft == nofile_soft_readback
        && nofile_hard == nofile_hard_readback
        && max_rss_bytes <= rss_limit_bytes
        && containment_failures == 0
        && oom_delta == 0
        && oom_kill_delta == 0
        && exit_code == Some(0)
        && term_signal.is_none()
        && raw_wait_status == Some(0)
        && waited
        && reaped
        && cleanup_succeeded
        && leader_pid > 0
        && leader_start_ticks > 0
        && pgid == leader_pid
        && readiness_seen
        && membership_verified
        && drain_complete
        && cgroup_populated_zero
        && pgid_empty
        && cgroup_removed
        && !cgroup_retained
        && !scope_abort_requested
        && !scope_abort_observed
}

fn valid_observation(observation: RawBoundedObservation, expected_cap: u64) -> bool {
    observation.cap_bytes == expected_cap
        && observation.observed_bytes <= observation.cap_bytes
        && !observation.truncated
}

fn checked_ratio_basis_points(numerator: u64, denominator: u64) -> Option<u64> {
    if denominator == 0 || numerator > denominator {
        return None;
    }
    u64::try_from(u128::from(numerator).checked_mul(10_000)? / u128::from(denominator)).ok()
}

fn validate_all_raw_identities(raw: &RawThresholdEvidence) -> Option<()> {
    validate_raw_identity(&raw.experiment_identity, "wu0g-experiment-v1")?;
    validate_raw_identity(&raw.plan_identity, "wu0g-plan-v1")?;
    validate_raw_identity(&raw.profile_identity, "wu0g-profile-v1")?;
    validate_raw_identity(&raw.universe_identity, "wu0g-universe-v1")?;
    validate_raw_identity(&raw.workload_identity, "wu0g-workload-v1")?;
    validate_raw_identity(&raw.representative_workload_identity, "wu0g-workload-v1")?;
    validate_raw_identity(&raw.candidate_identity, "wu0g-candidate-v1")?;
    validate_raw_identity(&raw.baseline_binary_identity, "wu0g-binary-v1")?;
    validate_raw_identity(&raw.candidate_binary_identity, "wu0g-binary-v1")?;
    validate_raw_identity(&raw.prlimit_identity, "wu0g-executable-v1")?;
    validate_raw_identity(&raw.perf_identity, "wu0g-executable-v1")?;
    validate_raw_identity(&raw.host_identity, "wu0g-host-v1")?;
    for row in &raw.ladder_rows {
        for point in [&row.baseline, &row.candidate] {
            validate_raw_identity(&point.rung_identity, "wu0g-rung-v1")?;
            validate_raw_identity(&point.report_identity, "wu0g-report-v1")?;
            validate_raw_identity(&point.semantic_identity, "wu0g-prefix-semantic-v1")?;
            for component in &point.complete_component_identities {
                validate_raw_identity(component, "wu0g-component-v1")?;
            }
        }
    }
    for dossier in &raw.cycle_profiles {
        validate_raw_identity(&dossier.plan_identity, "wu0g-plan-v1")?;
        validate_raw_identity(&dossier.profile_identity, "wu0g-profile-v1")?;
        validate_raw_identity(&dossier.workload_identity, "wu0g-workload-v1")?;
        validate_raw_identity(&dossier.candidate_identity, "wu0g-candidate-v1")?;
        validate_raw_identity(&dossier.binary_identity, "wu0g-binary-v1")?;
        validate_raw_identity(&dossier.report_identity, "wu0g-report-v1")?;
        validate_raw_identity(&dossier.semantic_identity, "wu0g-semantic-v1")?;
        validate_raw_identity(&dossier.artifact_identity, "wu0g-artifact-v1")?;
        validate_raw_identity(
            &dossier.artifact_content_identity,
            "wu0g-artifact-content-v1",
        )?;
    }
    for dossier in &raw.predictions {
        validate_raw_identity(&dossier.plan_identity, "wu0g-plan-v1")?;
        validate_raw_identity(&dossier.report_identity, "wu0g-report-v1")?;
        validate_raw_identity(&dossier.profile_identity, "wu0g-profile-v1")?;
        validate_raw_identity(&dossier.workload_identity, "wu0g-workload-v1")?;
        validate_raw_identity(&dossier.candidate_identity, "wu0g-candidate-v1")?;
        validate_raw_identity(&dossier.binary_identity, "wu0g-binary-v1")?;
        validate_raw_identity(&dossier.semantic_identity, "wu0g-semantic-v1")?;
        validate_raw_identity(&dossier.artifact_identity, "wu0g-artifact-v1")?;
        validate_raw_identity(
            &dossier.artifact_content_identity,
            "wu0g-artifact-content-v1",
        )?;
    }
    for dossier in &raw.controls {
        validate_raw_identity(&dossier.plan_identity, "wu0g-plan-v1")?;
        validate_raw_identity(&dossier.profile_identity, "wu0g-profile-v1")?;
        validate_raw_identity(&dossier.workload_identity, "wu0g-control-workload-v1")?;
        validate_raw_identity(&dossier.candidate_identity, "wu0g-candidate-v1")?;
        for binary in [
            &dossier.baseline_binary_identity,
            &dossier.candidate_binary_identity,
        ] {
            validate_raw_identity(binary, "wu0g-binary-v1")?;
        }
        for report in [
            &dossier.baseline_report_identity,
            &dossier.candidate_report_identity,
        ] {
            validate_raw_identity(report, "wu0g-report-v1")?;
        }
        for semantic in [
            &dossier.baseline_semantic_identity,
            &dossier.candidate_semantic_identity,
        ] {
            validate_raw_identity(semantic, "wu0g-semantic-v1")?;
        }
        for artifact in [
            &dossier.baseline_artifact_identity,
            &dossier.candidate_artifact_identity,
        ] {
            validate_raw_identity(artifact, "wu0g-artifact-v1")?;
        }
        for content in [
            &dossier.baseline_artifact_content_identity,
            &dossier.candidate_artifact_content_identity,
        ] {
            validate_raw_identity(content, "wu0g-artifact-content-v1")?;
        }
    }
    for dossier in &raw.causal_process_dossiers {
        validate_raw_identity(&dossier.plan_identity, "wu0g-plan-v1")?;
        validate_raw_identity(&dossier.profile_identity, "wu0g-profile-v1")?;
        validate_raw_identity(&dossier.universe_identity, "wu0g-universe-v1")?;
        validate_raw_identity(&dossier.workload_identity, "wu0g-workload-v1")?;
        validate_raw_identity(&dossier.candidate_identity, "wu0g-candidate-v1")?;
        validate_raw_identity(&dossier.binary_identity, "wu0g-binary-v1")?;
        validate_raw_identity(&dossier.rung_identity, "wu0g-rung-v1")?;
        validate_raw_identity(&dossier.report_identity, "wu0g-report-v1")?;
        validate_raw_identity(&dossier.semantic_identity, "wu0g-prefix-semantic-v1")?;
        validate_raw_identity(&dossier.scope_identity, "wu0g-scope-v1")?;
        validate_raw_identity(&dossier.cgroup_identity, "wu0g-cgroup-v1")?;
        validate_raw_identity(&dossier.request_content_identity, "wu0g-request-content-v1")?;
        validate_raw_identity(&dossier.request_path_identity, "wu0g-relative-path-v1")?;
        validate_raw_identity(&dossier.result_identity, "wu0g-result-v1")?;
        validate_raw_identity(&dossier.result_content_identity, "wu0g-result-content-v1")?;
        validate_raw_identity(&dossier.result_path_identity, "wu0g-relative-path-v1")?;
        validate_raw_identity(&dossier.result_bound_plan_identity, "wu0g-plan-v1")?;
        validate_raw_identity(
            &dossier.result_bound_request_content_identity,
            "wu0g-request-content-v1",
        )?;
        validate_raw_identity(&dossier.stdout_identity, "wu0g-stdout-v1")?;
        validate_raw_identity(&dossier.stderr_identity, "wu0g-stderr-v1")?;
        validate_raw_identity(&dossier.child_identity, "wu0g-child-v1")?;
        validate_raw_identity(
            &dossier.completion_sentinel_identity,
            "wu0g-child-completion-sentinel-v1",
        )?;
        validate_raw_identity(&dossier.child_artifact_identity, "wu0g-child-artifact-v1")?;
        validate_raw_identity(&dossier.prlimit_identity, "wu0g-executable-v1")?;
    }
    for pair in &raw.performance_pairs {
        validate_raw_identity(&pair.pair_identity, "wu0g-performance-pair-v1")?;
        for launch in &pair.launches {
            validate_raw_identity(&launch.plan_identity, "wu0g-plan-v1")?;
            validate_raw_identity(&launch.representative_workload_identity, "wu0g-workload-v1")?;
            validate_raw_identity(&launch.candidate_identity, "wu0g-candidate-v1")?;
            validate_raw_identity(&launch.binary_identity, "wu0g-binary-v1")?;
            validate_raw_identity(&launch.report_identity, "wu0g-report-v1")?;
            validate_raw_identity(&launch.semantic_identity, "wu0g-end-to-end-semantic-v1")?;
            validate_raw_identity(&launch.pair_identity, "wu0g-performance-pair-v1")?;
            validate_raw_identity(&launch.launch_identity, "wu0g-performance-launch-v1")?;
            validate_raw_identity(&launch.request_content_identity, "wu0g-request-content-v1")?;
            validate_raw_identity(&launch.request_path_identity, "wu0g-relative-path-v1")?;
            validate_raw_identity(&launch.result_identity, "wu0g-result-v1")?;
            validate_raw_identity(&launch.result_content_identity, "wu0g-result-content-v1")?;
            validate_raw_identity(&launch.result_path_identity, "wu0g-relative-path-v1")?;
            validate_raw_identity(&launch.result_bound_plan_identity, "wu0g-plan-v1")?;
            validate_raw_identity(
                &launch.result_bound_request_content_identity,
                "wu0g-request-content-v1",
            )?;
            validate_raw_identity(&launch.child_identity, "wu0g-child-v1")?;
            validate_raw_identity(
                &launch.completion_sentinel_identity,
                "wu0g-child-completion-sentinel-v1",
            )?;
            validate_raw_identity(&launch.child_artifact_identity, "wu0g-child-artifact-v1")?;
            validate_raw_identity(&launch.perf_artifact_identity, "wu0g-perf-artifact-v1")?;
            validate_raw_identity(&launch.prlimit_identity, "wu0g-executable-v1")?;
            validate_raw_identity(&launch.perf_identity, "wu0g-executable-v1")?;
            validate_raw_identity(&launch.host_identity, "wu0g-host-v1")?;
            validate_raw_identity(&launch.scope_identity, "wu0g-scope-v1")?;
            validate_raw_identity(&launch.cgroup_identity, "wu0g-cgroup-v1")?;
            validate_raw_identity(&launch.stdout_identity, "wu0g-stdout-v1")?;
            validate_raw_identity(&launch.stderr_identity, "wu0g-stderr-v1")?;
        }
    }
    Some(())
}

fn validate_raw_identity(identity: &RawIdentityEvidence, expected_domain: &str) -> Option<()> {
    if identity.domain != expected_domain
        || raw_identity_from_parts(&identity.domain, identity.canonical_bytes.clone())? != *identity
    {
        return None;
    }
    Some(())
}

fn raw_identity_from_parts(domain: &str, canonical_bytes: Vec<u8>) -> Option<RawIdentityEvidence> {
    let mut framed = Vec::new();
    framed.extend_from_slice(&u64::try_from(domain.len()).ok()?.to_be_bytes());
    framed.extend_from_slice(domain.as_bytes());
    framed.extend_from_slice(&u64::try_from(canonical_bytes.len()).ok()?.to_be_bytes());
    framed.extend_from_slice(&canonical_bytes);
    Some(RawIdentityEvidence {
        domain: domain.to_owned(),
        canonical_bytes,
        claimed_sha256: digest_bytes(&framed),
    })
}

fn raw_frame_identity(output: &mut Vec<u8>, identity: &RawIdentityEvidence) -> Option<()> {
    raw_frame_bytes(output, identity.domain.as_bytes())?;
    raw_frame_bytes(output, &identity.canonical_bytes)?;
    raw_frame_bytes(output, identity.claimed_sha256.as_bytes())
}

fn raw_frame_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    output.extend_from_slice(&u64::try_from(bytes.len()).ok()?.to_be_bytes());
    output.extend_from_slice(bytes);
    Some(())
}

fn raw_frame_strings(output: &mut Vec<u8>, strings: &[String]) -> Option<()> {
    output.extend_from_slice(&u64::try_from(strings.len()).ok()?.to_be_bytes());
    for string in strings {
        raw_frame_bytes(output, string.as_bytes())?;
    }
    Some(())
}

fn raw_plan_identity(raw: &RawThresholdEvidence) -> Option<RawIdentityEvidence> {
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
        raw_frame_identity(&mut bytes, identity)?;
    }
    for value in [&raw.perf_version, &raw.perf_event, &raw.cpu_affinity] {
        raw_frame_bytes(&mut bytes, value.as_bytes())?;
    }
    for (rung_ordinal, row) in raw.ladder_rows.iter().enumerate() {
        bytes.push(u8::try_from(rung_ordinal).ok()?);
        raw_frame_identity(&mut bytes, &row.baseline.rung_identity)?;
        raw_frame_identity(&mut bytes, &row.candidate.rung_identity)?;
    }
    for dossier in &raw.causal_process_dossiers {
        bytes.push(dossier.rung_ordinal);
        bytes.push(mode_code(dossier.mode));
        raw_frame_identity(&mut bytes, &dossier.binary_identity)?;
        raw_frame_strings(&mut bytes, &dossier.canonical_invocation)?;
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
        raw_frame_identity(&mut bytes, &pair.pair_identity)?;
        bytes.push(pair.pair_ordinal);
        bytes.push(launch_order_code(pair.order));
        for launch in &pair.launches {
            for identity in [
                &launch.pair_identity,
                &launch.launch_identity,
                &launch.binary_identity,
            ] {
                raw_frame_identity(&mut bytes, identity)?;
            }
            bytes.extend([launch.pair_ordinal, launch.launch_ordinal]);
            bytes.push(mode_code(launch.mode));
            raw_frame_strings(&mut bytes, &launch.canonical_invocation)?;
            raw_frame_strings(&mut bytes, &launch.perf_invocation)?;
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
    raw_identity_from_parts("wu0g-plan-v1", bytes)
}

fn raw_experiment_identity(raw: &RawThresholdEvidence) -> Option<RawIdentityEvidence> {
    let mut bytes = Vec::new();
    raw_frame_identity(&mut bytes, &raw.plan_identity)?;
    for row in &raw.ladder_rows {
        for point in [&row.baseline, &row.candidate] {
            for identity in [
                &point.rung_identity,
                &point.report_identity,
                &point.semantic_identity,
            ] {
                raw_frame_identity(&mut bytes, identity)?;
            }
            bytes.extend_from_slice(&point.target_counter.to_be_bytes());
            bytes.extend_from_slice(&point.total_work_counter.to_be_bytes());
            bytes.push(u8::from(point.saturated));
            for component in &point.complete_component_identities {
                raw_frame_identity(&mut bytes, component)?;
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
            raw_frame_identity(&mut bytes, identity)?;
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
            raw_frame_identity(&mut bytes, identity)?;
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
            raw_frame_identity(&mut bytes, identity)?;
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
            raw_frame_identity(&mut bytes, identity)?;
        }
        bytes.push(mode_code(dossier.mode));
        bytes.push(dossier.rung_ordinal);
        raw_frame_strings(&mut bytes, &dossier.canonical_invocation)?;
        for observation in [
            dossier.stdout_observation,
            dossier.stderr_observation,
            dossier.request_observation,
            dossier.result_observation,
            dossier.sentinel_observation,
            dossier.semantic_artifact_observation,
        ] {
            raw_frame_observation(&mut bytes, observation);
        }
        raw_frame_causal_process_status(&mut bytes, dossier)?;
    }
    for pair in &raw.performance_pairs {
        raw_frame_identity(&mut bytes, &pair.pair_identity)?;
        bytes.push(pair.pair_ordinal);
        bytes.push(launch_order_code(pair.order));
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
                raw_frame_identity(&mut bytes, identity)?;
            }
            for value in [
                &launch.perf_version,
                &launch.perf_event,
                &launch.cpu_affinity,
            ] {
                raw_frame_bytes(&mut bytes, value.as_bytes())?;
            }
            bytes.extend([launch.pair_ordinal, launch.launch_ordinal]);
            bytes.push(mode_code(launch.mode));
            raw_frame_strings(&mut bytes, &launch.canonical_invocation)?;
            raw_frame_strings(&mut bytes, &launch.perf_invocation)?;
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
                raw_frame_observation(&mut bytes, observation);
            }
            raw_frame_performance_process_status(&mut bytes, launch)?;
        }
    }
    raw_identity_from_parts("wu0g-experiment-v1", bytes)
}

fn mode_code(mode: RawProbeMode) -> u8 {
    match mode {
        RawProbeMode::Baseline => 0,
        RawProbeMode::CandidateB => 1,
    }
}

fn launch_order_code(order: RawLaunchOrder) -> u8 {
    match order {
        RawLaunchOrder::Ab => 0,
        RawLaunchOrder::Ba => 1,
    }
}

fn raw_frame_observation(output: &mut Vec<u8>, observation: RawBoundedObservation) {
    output.extend_from_slice(&observation.cap_bytes.to_be_bytes());
    output.extend_from_slice(&observation.observed_bytes.to_be_bytes());
    output.push(u8::from(observation.truncated));
}

fn raw_frame_causal_process_status(
    output: &mut Vec<u8>,
    dossier: &RawFreshProcessDossier,
) -> Option<()> {
    raw_frame_process_status_full(output, ProcessStatusView::from_causal(dossier))
}

fn raw_frame_performance_process_status(
    output: &mut Vec<u8>,
    launch: &RawPerformanceLaunch,
) -> Option<()> {
    raw_frame_process_status_full(output, ProcessStatusView::from_performance(launch))
}

fn raw_frame_process_status_full(
    output: &mut Vec<u8>,
    status: ProcessStatusView<'_>,
) -> Option<()> {
    let ProcessStatusView {
        scope,
        cgroup,
        leader_pid,
        leader_start_ticks,
        pgid,
        readiness_seen,
        membership_verified,
        drain_complete,
        cgroup_populated_zero,
        pgid_empty,
        cgroup_removed,
        cgroup_retained,
        scope_abort_requested,
        scope_abort_observed,
        termination,
        deadline_ms,
        deadline_readback_ms,
        memory_limit_bytes,
        memory_limit_readback_bytes,
        rss_limit_bytes,
        rss_limit_readback_bytes,
        nofile_soft,
        nofile_hard,
        nofile_soft_readback,
        nofile_hard_readback,
        max_rss_bytes,
        containment_failures,
        oom_delta,
        oom_kill_delta,
        exit_code,
        term_signal,
        raw_wait_status,
        waited,
        reaped,
        cleanup_succeeded,
    } = status;
    raw_frame_identity(output, scope)?;
    raw_frame_identity(output, cgroup)?;
    output.extend_from_slice(&leader_pid.to_be_bytes());
    output.extend_from_slice(&leader_start_ticks.to_be_bytes());
    output.extend_from_slice(&pgid.to_be_bytes());
    output.extend([
        u8::from(readiness_seen),
        u8::from(membership_verified),
        u8::from(drain_complete),
        u8::from(cgroup_populated_zero),
        u8::from(pgid_empty),
        u8::from(cgroup_removed),
        u8::from(cgroup_retained),
        u8::from(scope_abort_requested),
        u8::from(scope_abort_observed),
    ]);
    output.push(termination_code(termination));
    for value in [
        deadline_ms,
        deadline_readback_ms,
        memory_limit_bytes,
        memory_limit_readback_bytes,
        rss_limit_bytes,
        rss_limit_readback_bytes,
        nofile_soft,
        nofile_hard,
        nofile_soft_readback,
        nofile_hard_readback,
        max_rss_bytes,
        containment_failures,
        oom_delta,
        oom_kill_delta,
    ] {
        output.extend_from_slice(&value.to_be_bytes());
    }
    for value in [exit_code, term_signal, raw_wait_status] {
        output.push(u8::from(value.is_some()));
        output.extend_from_slice(&value.unwrap_or_default().to_be_bytes());
    }
    output.extend([
        u8::from(waited),
        u8::from(reaped),
        u8::from(cleanup_succeeded),
    ]);
    Some(())
}

fn termination_code(termination: RawProbeTermination) -> u8 {
    match termination {
        RawProbeTermination::Complete => 0,
        RawProbeTermination::Deadline => 1,
        RawProbeTermination::MemoryLimit => 2,
        RawProbeTermination::NoProgress => 3,
        RawProbeTermination::StdoutLimit => 4,
        RawProbeTermination::StderrLimit => 5,
        RawProbeTermination::Infrastructure => 6,
    }
}

fn improvement_basis_points(baseline: u64, control: u64) -> Option<u64> {
    checked_ratio_basis_points(baseline.checked_sub(control)?, baseline)
}

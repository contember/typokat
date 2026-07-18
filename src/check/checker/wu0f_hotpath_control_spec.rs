//! Disabled RED contract for the WU0F compile-time hot-path control.
//!
//! Activate after this spec's independent commit with adjacent cfg(test) modules
//! `wu0f_hotpath_control` and `wu0f_hotpath_control_spec`. The default-off Cargo feature
//! `wu0-uninstrumented-control` answers one question: how much of exact WU0E `plain` release-
//! libtest time is test-only instrumentation overhead?
//!
//! The feature removes only the enumerated execution-time hooks, contextual wrappers, and inactive
//! collector/cache/attribution branches. `Pass`/`Substitution` instrumentation fields and cold APIs
//! remain ordinary cfg(test); their control-build captures return `None`, preserving layout and
//! compile coverage while removing TLS initialization work. The feature libtest must compile and
//! run the exact WU0F probes, but measurement-dependent whole-suite success is not required. Core
//! semantic owners, modules, WU0B's canonical compiler body, and WU0E's coarse phase observer are
//! never feature-gated. Feature-off test behavior is unchanged.
//!
//! A separate coordinator builds baseline and control release libtests once in separate target
//! directories from identical source, lockfile, rustc, release profile/flags, pinned workload
//! profile, and warm inventory. Feature polarity is absent versus exactly this feature. Every
//! launch uses the same host, containment, limits, closed environment, trace policy, and identity
//! validation. It runs compile-state, the ordinary-build counter-coverage preflight, three
//! completing semantic witnesses, then the primary in explicit alternating schedules. Each launch
//! binds the frozen binary SHA of its declared variant and records wall time, max RSS, and
//! instructions. Completing witnesses must have exact
//! record/profile/semantic parity; completing primaries must have trace semantic parity. A timeout
//! never invents a digest.
//!
//! The coordinator is diagnostic-only. It never invokes, parses, emits, or claims release evidence
//! and exposes no authorization decision. Existing release modules and runners are outside WU0F's
//! implementation scope and remain untouched.

use super::wu0b_library::{
    canonical_wu0d_semantic_identity_from_components_for_test, run_injected_profile,
    InjectedLibrarySource,
};
use super::wu0e_diagnostic::{run_observed_profile_for_test, DiagnosticMode};
use super::wu0f_hotpath_control::{
    parse_control_evidence_for_test, parse_control_runner_self_test_report,
    render_control_evidence_for_test, ControlRunnerSelfTestCase,
};
use crate::source::LibraryFileOrdinal;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const FEATURE: &str = "wu0-uninstrumented-control";
const GUARD: &str = "#[cfg(all(test, not(feature = \"wu0-uninstrumented-control\")))]";
const FALLBACK_GUARD: &str = "#[cfg(any(not(test), feature = \"wu0-uninstrumented-control\"))]";
const WU0G_GUARD: &str = "#[cfg(all(test, feature = \"wu0-interface-fill-attribution\", not(feature = \"wu0-uninstrumented-control\")))]";
const WU0E_MODE_REJECTION_GUARD: &str =
    "#[cfg(all(test, feature = \"wu0-uninstrumented-control\"))]";
const WU0E_MODE_REJECTION_ANCHOR: &str = "if environment.mode != DiagnosticMode::Plain {";
const SELF_SPEC: &str = "src/check/checker/wu0f_hotpath_control_spec.rs";
const SELF_BAD_MODE_GUARD: &str = "#[cfg(feature = \"wu0-uninstrumented-control\")]";
const SELF_COUNTER_GUARD: &str = "#[cfg(not(feature = \"wu0-uninstrumented-control\"))]";
const CONTROL_RUNNER: &str = "tooling/wu0f-uninstrumented-control/run.pl";
const PRIMARY_PROBE: &str = "check::checker::wu0e_diagnostic::wu0e_primary_probe_once";
const COMPILE_STATE_PROBE: &str =
    "check::checker::wu0f_hotpath_control_spec::wu0f_compile_state_probe_once";
const COUNTER_COVERAGE_PROBE: &str =
    "check::checker::wu0f_hotpath_control_spec::wu0f_ordinary_counter_coverage_probe_once";
const RECURSIVE_PROBE: &str =
    "check::checker::wu0f_hotpath_control_spec::wu0f_recursive_witness_probe_once";
const MAPPED_PROBE: &str =
    "check::checker::wu0f_hotpath_control_spec::wu0f_mapped_witness_probe_once";
const OVERLOAD_PROBE: &str =
    "check::checker::wu0f_hotpath_control_spec::wu0f_overload_witness_probe_once";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const SHA_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const SHA_E: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const SHA_F: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const SHA_ONE: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const BASELINE_BINARY_SHA256: &str =
    "8888888888888888888888888888888888888888888888888888888888888888";
const CONTROL_BINARY_SHA256: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

const RECURSIVE_SOURCES: &[InjectedLibrarySource<'static>] = &[InjectedLibrarySource {
    file_ordinal: LibraryFileOrdinal::new(0),
    name: "wu0f-recursive.ts",
    source: "interface Wu0fSource<T> { value: T; next: Wu0fSource<T>; }\n\
             interface Wu0fTarget<T> { value: T; next: Wu0fTarget<T>; }\n\
             declare const wu0fRecursiveSource: Wu0fSource<string>;\n\
             const wu0fRecursiveTarget: Wu0fTarget<string> = wu0fRecursiveSource;\n",
}];

const MAPPED_SOURCES: &[InjectedLibrarySource<'static>] = &[InjectedLibrarySource {
    file_ordinal: LibraryFileOrdinal::new(0),
    name: "wu0f-mapped.ts",
    source: "interface Wu0fMappedInput { readonly id: string; count?: number; }\n\
             type Wu0fMapped<T> = { [K in keyof T]-?: T[K] };\n\
             declare const wu0fMapped: Wu0fMapped<Wu0fMappedInput>;\n\
             const wu0fMappedCount: number = wu0fMapped.count;\n",
}];

const OVERLOAD_SOURCES: &[InjectedLibrarySource<'static>] = &[InjectedLibrarySource {
    file_ordinal: LibraryFileOrdinal::new(0),
    name: "wu0f-overload.ts",
    source: "interface Wu0fPicker {\n\
               <T extends string>(value: T): T;\n\
               <T extends number>(value: T): T;\n\
             }\n\
             declare const wu0fPick: Wu0fPicker;\n\
             declare const wu0fLiteral: \"wu0f\";\n\
             const wu0fPicked: \"wu0f\" = wu0fPick(wu0fLiteral);\n",
}];

struct HotSite {
    file: &'static str,
    anchor: &'static str,
    total: usize,
    guarded: usize,
}

struct FeatureCfgSite {
    file: &'static str,
    guard: &'static str,
    owned_anchor: &'static str,
    count: usize,
}

const FEATURE_CFG_INVENTORY: &[FeatureCfgSite] = &[
    FeatureCfgSite {
        file: "src/relate/relation/mod.rs",
        guard: GUARD,
        owned_anchor: "measure_relation(|",
        count: 3,
    },
    FeatureCfgSite {
        file: "src/relate/relation/objects.rs",
        guard: GUARD,
        owned_anchor: "super::measure_relation(|",
        count: 4,
    },
    FeatureCfgSite {
        file: "src/check/infer/helpers.rs",
        guard: GUARD,
        owned_anchor: "measure_inference(|",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/infer/context.rs",
        guard: GUARD,
        owned_anchor: "super::helpers::measure_inference(|",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/query/mod.rs",
        guard: GUARD,
        owned_anchor: "measure_query_demand(|",
        count: 13,
    },
    FeatureCfgSite {
        file: "src/check/checker/eval/instantiation.rs",
        guard: GUARD,
        owned_anchor: "measure_infer_rewrite(|",
        count: 6,
    },
    FeatureCfgSite {
        file: "src/check/checker/eval/mapped.rs",
        guard: GUARD,
        owned_anchor: "measure_mapped_rewrite(|",
        count: 5,
    },
    FeatureCfgSite {
        file: "src/check/checker/eval/mapped.rs",
        guard: GUARD,
        owned_anchor: "if root_visit {",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/eval/mapped.rs",
        guard: GUARD,
        owned_anchor: "begin_mapped_assembly();",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/eval/mapped.rs",
        guard: GUARD,
        owned_anchor: "measure_mapped_property_context(",
        count: 3,
    },
    FeatureCfgSite {
        file: "src/check/checker/eval/mapped.rs",
        guard: GUARD,
        owned_anchor: "note_mapped_reintern();",
        count: 14,
    },
    FeatureCfgSite {
        file: "src/check/checker/calls.rs",
        guard: GUARD,
        owned_anchor: "measure_call(|",
        count: 15,
    },
    FeatureCfgSite {
        file: "src/check/checker/expr.rs",
        guard: GUARD,
        owned_anchor: "super::calls::measure_contextual_",
        count: 4,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: GUARD,
        owned_anchor: "if let Some(collector) = self.measurement.as_ref()",
        count: 7,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: GUARD,
        owned_anchor: "if let Some(collector) = self.run_visit_measurement.as_ref()",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: GUARD,
        owned_anchor: "let wu0c_visit = self",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: GUARD,
        owned_anchor: "if let (Some(attribution), Some(visit))",
        count: 4,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: GUARD,
        owned_anchor: "if let Some(attribution) =",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/resolve.rs",
        guard: GUARD,
        owned_anchor: "record_eager_application_cache_measure(",
        count: 11,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/resolve.rs",
        guard: GUARD,
        owned_anchor: "if self.cycle_tainted_application_cache_measure.is_some()",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/resolve.rs",
        guard: GUARD,
        owned_anchor: "if let Some(attribution) = &self.wu0c_attribution",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/resolve.rs",
        guard: GUARD,
        owned_anchor: "let wu0c_application =",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/resolve.rs",
        guard: GUARD,
        owned_anchor: "if let Some(application) = &wu0c_application",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0b_library.rs",
        guard: GUARD,
        owned_anchor: "super::wu0c_attribution::register_wu0c_family_tokens",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0b_library.rs",
        guard: GUARD,
        owned_anchor: "super::wu0c_attribution::enter_wu0c_phase",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0e_diagnostic.rs",
        guard: GUARD,
        owned_anchor: "let _eager_scope =",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0e_diagnostic.rs",
        guard: GUARD,
        owned_anchor: "let _baseline_scope =",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0e_diagnostic.rs",
        guard: GUARD,
        owned_anchor: "let _candidate_scope =",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/checker/calls.rs",
        guard: GUARD,
        owned_anchor: "with_contextual_measure_phase",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/calls.rs",
        guard: FALLBACK_GUARD,
        owned_anchor: "$pass.infer_contextual_source_after_walked",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/calls.rs",
        guard: GUARD,
        owned_anchor: "$pass.contextual_inference_args",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/calls.rs",
        guard: FALLBACK_GUARD,
        owned_anchor: "$pass.contextual_inference_args",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/calls.rs",
        guard: GUARD,
        owned_anchor: "phase: ContextualMeasurePhase",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: GUARD,
        owned_anchor: "SUBSTITUTION_MEASURE.with",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: GUARD,
        owned_anchor: "SUBSTITUTION_RUN_VISIT_MEASURE.with",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: FALLBACK_GUARD,
        owned_anchor: "None",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/checker/context.rs",
        guard: GUARD,
        owned_anchor: "EAGER_APPLICATION_CACHE_MEASURE.with",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/context.rs",
        guard: GUARD,
        owned_anchor: "CYCLE_TAINTED_APPLICATION_CACHE_CAPTURE.with",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/context.rs",
        guard: FALLBACK_GUARD,
        owned_anchor: "None",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0c_attribution.rs",
        guard: GUARD,
        owned_anchor: "CURRENT_SESSION.with",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0c_attribution.rs",
        guard: GUARD,
        owned_anchor: "capture_substitution_from_active(map)",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0c_attribution.rs",
        guard: FALLBACK_GUARD,
        owned_anchor: "None",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/check/checker/context.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_attribution",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_attribution: None",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "mod wu0g_interface_fill_attribution;",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "mod wu0g_interface_fill_attribution_spec;",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "self.wu0g_record_component_boundary()",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/check/checker/decls/resolve.rs",
        guard: WU0G_GUARD,
        owned_anchor: "self.wu0g_application_resolve(",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_attribution:",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_attribution,",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_record_visit_",
        count: 2,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_record_cycle_reentry",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_application_substitute_with_outcome",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "fn wu0g_record_interner_attempt",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/mod.rs",
        guard: WU0G_GUARD,
        owned_anchor: "fn wu0g_record_object_copy",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/apply.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_record_object_copy(",
        count: 1,
    },
    FeatureCfgSite {
        file: "src/types/substitute/apply.rs",
        guard: WU0G_GUARD,
        owned_anchor: "wu0g_record_interner_attempt(",
        count: 16,
    },
    FeatureCfgSite {
        file: "src/check/checker/wu0e_diagnostic.rs",
        guard: WU0E_MODE_REJECTION_GUARD,
        owned_anchor: WU0E_MODE_REJECTION_ANCHOR,
        count: 1,
    },
];

/// Concrete runtime sites only. Cold definitions deliberately do not appear here.
const HOT_SITES: &[HotSite] = &[
    HotSite {
        file: "src/relate/relation/mod.rs",
        anchor: "measure_relation(|",
        total: 3,
        guarded: 3,
    },
    HotSite {
        file: "src/relate/relation/objects.rs",
        anchor: "super::measure_relation(|",
        total: 4,
        guarded: 4,
    },
    HotSite {
        file: "src/check/infer/helpers.rs",
        anchor: "measure_inference(|",
        total: 2,
        guarded: 2,
    },
    HotSite {
        file: "src/check/infer/context.rs",
        anchor: "super::helpers::measure_inference(|",
        total: 2,
        guarded: 2,
    },
    HotSite {
        file: "src/check/query/mod.rs",
        anchor: "measure_query_demand(|",
        total: 13,
        guarded: 13,
    },
    HotSite {
        file: "src/check/checker/eval/instantiation.rs",
        anchor: "measure_infer_rewrite(|",
        total: 6,
        guarded: 6,
    },
    // One mapped-rewrite call is inside its cold helper and one is owned by the guarded root_visit branch.
    HotSite {
        file: "src/check/checker/eval/mapped.rs",
        anchor: "measure_mapped_rewrite(|",
        total: 7,
        guarded: 5,
    },
    HotSite {
        file: "src/check/checker/eval/mapped.rs",
        anchor: "if root_visit {",
        total: 1,
        guarded: 1,
    },
    HotSite {
        file: "src/check/checker/eval/mapped.rs",
        anchor: "begin_mapped_assembly();",
        total: 1,
        guarded: 1,
    },
    HotSite {
        file: "src/check/checker/eval/mapped.rs",
        anchor: "measure_mapped_property_context(mapped.",
        total: 3,
        guarded: 3,
    },
    HotSite {
        file: "src/check/checker/eval/mapped.rs",
        anchor: "note_mapped_reintern();",
        total: 14,
        guarded: 14,
    },
    // The sixteenth call is inside the cold measure_discarded_candidate_queries helper.
    HotSite {
        file: "src/check/checker/calls.rs",
        anchor: "measure_call(|",
        total: 16,
        guarded: 15,
    },
    HotSite {
        file: "src/check/checker/expr.rs",
        anchor: "super::calls::measure_contextual_",
        total: 4,
        guarded: 4,
    },
    HotSite {
        file: "src/types/substitute/mod.rs",
        anchor: "if let Some(collector) = self.measurement.as_ref()",
        total: 7,
        guarded: 7,
    },
    HotSite {
        file: "src/types/substitute/mod.rs",
        anchor: "if let Some(collector) = self.run_visit_measurement.as_ref()",
        total: 2,
        guarded: 2,
    },
    HotSite {
        file: "src/types/substitute/mod.rs",
        anchor: "let wu0c_visit = self",
        total: 1,
        guarded: 1,
    },
    HotSite {
        file: "src/types/substitute/mod.rs",
        anchor:
            "if let (Some(attribution), Some(visit)) = (self.wu0c_attribution.as_ref(), wu0c_visit)",
        total: 4,
        guarded: 4,
    },
    HotSite {
        file: "src/types/substitute/mod.rs",
        anchor: "if let Some(attribution) = substitution.wu0c_attribution.as_ref()",
        total: 1,
        guarded: 1,
    },
    // Eleven eager calls are direct; three more live inside the one guarded candidate branch.
    HotSite {
        file: "src/check/checker/decls/resolve.rs",
        anchor: "record_eager_application_cache_measure(",
        total: 14,
        guarded: 11,
    },
    HotSite {
        file: "src/check/checker/decls/resolve.rs",
        anchor: "if self.cycle_tainted_application_cache_measure.is_some()",
        total: 1,
        guarded: 1,
    },
    HotSite {
        file: "src/check/checker/decls/resolve.rs",
        anchor: "if let Some(attribution) = &self.wu0c_attribution",
        total: 1,
        guarded: 1,
    },
    HotSite {
        file: "src/check/checker/decls/resolve.rs",
        anchor: "let wu0c_application =",
        total: 1,
        guarded: 1,
    },
    // Four application finishes are inside the candidate branch; two are direct.
    HotSite {
        file: "src/check/checker/decls/resolve.rs",
        anchor: "if let Some(application) = &wu0c_application",
        total: 6,
        guarded: 2,
    },
    HotSite {
        file: "src/check/checker/wu0b_library.rs",
        anchor: "super::wu0c_attribution::register_wu0c_family_tokens",
        total: 1,
        guarded: 1,
    },
    HotSite {
        file: "src/check/checker/wu0b_library.rs",
        anchor: "super::wu0c_attribution::enter_wu0c_phase",
        total: 2,
        guarded: 2,
    },
    HotSite {
        file: "src/check/checker/wu0e_diagnostic.rs",
        anchor: "let _eager_scope =",
        total: 2,
        guarded: 2,
    },
    HotSite {
        file: "src/check/checker/wu0e_diagnostic.rs",
        anchor: "let _baseline_scope =",
        total: 2,
        guarded: 2,
    },
    HotSite {
        file: "src/check/checker/wu0e_diagnostic.rs",
        anchor: "let _candidate_scope =",
        total: 2,
        guarded: 2,
    },
];

const COLD_TEST_SURFACE: &[(&str, &str, usize)] = &[
    ("src/relate/relation/mod.rs", "fn measure_relation(", 1),
    ("src/check/infer/helpers.rs", "fn measure_inference(", 1),
    ("src/check/query/mod.rs", "fn measure_query_demand(", 1),
    (
        "src/check/checker/eval/instantiation.rs",
        "fn measure_infer_rewrite(",
        1,
    ),
    (
        "src/check/checker/eval/mapped.rs",
        "fn measure_mapped_rewrite(",
        1,
    ),
    ("src/check/checker/calls.rs", "fn measure_call(", 1),
    (
        "src/types/substitute/mod.rs",
        "fn capture_substitution_measurement(",
        1,
    ),
    (
        "src/types/substitute/mod.rs",
        "fn capture_substitution_run_visit_measure(",
        1,
    ),
    (
        "src/check/checker/context.rs",
        "fn capture_eager_application_cache_measure(",
        1,
    ),
    (
        "src/check/checker/context.rs",
        "fn capture_cycle_tainted_application_cache_measure(",
        1,
    ),
    (
        "src/check/checker/wu0c_attribution.rs",
        "fn capture_wu0c_pass_attribution(",
        1,
    ),
    (
        "src/check/checker/wu0c_attribution.rs",
        "fn capture_wu0c_substitution_attribution(",
        1,
    ),
    (
        "src/types/substitute/mod.rs",
        "measurement: Option<SubstitutionMeasureCollector>",
        1,
    ),
    (
        "src/types/substitute/mod.rs",
        "run_visit_measurement: Option<SubstitutionRunVisitMeasureCollector>",
        1,
    ),
    (
        "src/types/substitute/mod.rs",
        "wu0c_attribution: Option<crate::check::checker::SubstitutionAttribution>",
        1,
    ),
    (
        "src/types/substitute/mod.rs",
        "let measurement = capture_substitution_measurement();",
        1,
    ),
    (
        "src/types/substitute/mod.rs",
        "let run_visit_measurement = capture_substitution_run_visit_measure();",
        1,
    ),
    (
        "src/types/substitute/mod.rs",
        "let wu0c_attribution = crate::check::checker::capture_wu0c_substitution_attribution(map);",
        1,
    ),
    (
        "src/check/checker/context.rs",
        "pub(in crate::check::checker) eager_application_cache_measure:",
        1,
    ),
    (
        "src/check/checker/context.rs",
        "pub(in crate::check::checker) cycle_tainted_application_cache:",
        1,
    ),
    (
        "src/check/checker/context.rs",
        "pub(in crate::check::checker) cycle_tainted_application_cache_measure:",
        1,
    ),
    (
        "src/check/checker/context.rs",
        "pub(in crate::check::checker) panic_before_cycle_tainted_application_cache_publish:",
        1,
    ),
    (
        "src/check/checker/context.rs",
        "pub(in crate::check::checker) wu0c_attribution:",
        1,
    ),
    (
        "src/check/checker/mod.rs",
        "let cycle_tainted_application_cache_capture =",
        1,
    ),
    (
        "src/check/checker/mod.rs",
        "eager_application_cache_measure: context::capture_eager_application_cache_measure()",
        1,
    ),
    (
        "src/check/checker/mod.rs",
        "cycle_tainted_application_cache: cycle_tainted_application_cache_capture",
        1,
    ),
    (
        "src/check/checker/mod.rs",
        "cycle_tainted_application_cache_measure: cycle_tainted_application_cache_capture",
        1,
    ),
    (
        "src/check/checker/mod.rs",
        "panic_before_cycle_tainted_application_cache_publish: false",
        1,
    ),
    (
        "src/check/checker/mod.rs",
        "wu0c_attribution: wu0c_attribution::capture_wu0c_pass_attribution()",
        1,
    ),
];

const CONTROL_NOOP_CAPTURES: &[(&str, &str, &str)] = &[
    (
        "src/types/substitute/mod.rs",
        "fn capture_substitution_measurement(",
        "SUBSTITUTION_MEASURE.with",
    ),
    (
        "src/types/substitute/mod.rs",
        "fn capture_substitution_run_visit_measure(",
        "SUBSTITUTION_RUN_VISIT_MEASURE.with",
    ),
    (
        "src/check/checker/context.rs",
        "fn capture_eager_application_cache_measure(",
        "EAGER_APPLICATION_CACHE_MEASURE.with",
    ),
    (
        "src/check/checker/context.rs",
        "fn capture_cycle_tainted_application_cache_measure(",
        "CYCLE_TAINTED_APPLICATION_CACHE_CAPTURE.with",
    ),
    (
        "src/check/checker/wu0c_attribution.rs",
        "fn capture_wu0c_pass_attribution(",
        "CURRENT_SESSION.with",
    ),
    (
        "src/check/checker/wu0c_attribution.rs",
        "fn capture_wu0c_substitution_attribution(",
        "capture_substitution_from_active(map)",
    ),
];

const SEMANTIC_OWNERS: &[(&str, &str)] = &[
    ("src/relate/relation/mod.rs", "fn relate("),
    ("src/check/infer/context.rs", "fn infer_objects("),
    ("src/check/query/mod.rs", "pub(crate) fn demand("),
    (
        "src/check/checker/eval/mapped.rs",
        "pub(super) fn replace_mapped_value(",
    ),
    (
        "src/check/checker/calls.rs",
        "pub(in crate::check::checker) fn infer_call(",
    ),
    ("src/types/substitute/mod.rs", "pub fn apply("),
    (
        "src/check/checker/decls/resolve.rs",
        "fn substitute_ready_type_group_application_with_outcome(",
    ),
    (
        "src/check/checker/wu0b_library.rs",
        "pub(super) fn run_injected_profile_observed(",
    ),
];

const ORDINARY_COUNTER_TESTS: &[&str] = &[
    "relate::relation::tests::measure_relation_counts_actual_empty_context_key_and_property_scans",
    "check::infer::tests::measure_inference_counts_actual_snapshots_and_property_scans",
    "check::infer::wu7_measurements::measure_constraint_shared_pending_dag_uses_query_overlay",
    "check::checker::eval::tests::measure_infer_rewrite_counts_shared_dag_and_cycle_sibling",
    "check::checker::eval::tests::measure_mapped_property_fanout_pins_complete_context_repetition",
    "check::checker::tests::calls_measure::measure_call_pipeline_callback_formula",
    "types::substitute::tests::measure_substitution_hotpaths_counter_only",
    "check::checker::decls::eager_application_cache_spec::ready_cache_misses_then_hits_and_uses_exact_declaration_order",
    "check::checker::wu0e_diagnostic_spec::cycle_bearing_generic_witness_proves_mode_fidelity_and_candidate_hits",
    "check::checker::wu0c_post_cache_attribution_spec::exact_checkpoint_at_4096_is_direct_reborrow_safe_and_emits_exact_shape",
];

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "typokat-wu0f-{label}-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create WU0F scratch directory");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source(relative: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

fn rust_sources_under_src() -> Vec<(String, String)> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = repository.join("src");
    let mut pending = vec![src];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let mut entries = std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("walk {}: {error}", directory.display()))
            .map(|entry| entry.unwrap_or_else(|error| panic!("walk entry: {error}")))
            .collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
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
            let relative = path
                .strip_prefix(repository)
                .expect("src path remains below repository")
                .to_str()
                .unwrap_or_else(|| panic!("non-UTF-8 Rust path: {}", path.display()))
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            files.push((relative, text));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn code_occurrences<'a>(text: &'a str, anchor: &str) -> Vec<(usize, &'a str)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//") && line.contains(anchor))
        .collect()
}

fn immediate_attribute(text: &str, line_index: usize) -> Option<&str> {
    text.lines()
        .take(line_index)
        .filter(|line| !line.trim().is_empty())
        .last()
        .map(str::trim)
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

fn attribute_blocks(text: &str, prefix: &str) -> Vec<(usize, usize, String)> {
    let name = match prefix {
        "#[cfg(" => b"cfg".as_slice(),
        "#[cfg_attr(" => b"cfg_attr".as_slice(),
        _ => panic!("unsupported attribute prefix {prefix}"),
    };
    code_offsets(text, "#[")
        .into_iter()
        .filter(|start| {
            let bytes = text.as_bytes();
            let mut cursor = *start + 2;
            while bytes
                .get(cursor)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                cursor += 1;
            }
            if bytes.get(cursor..cursor + name.len()) != Some(name) {
                return false;
            }
            cursor += name.len();
            while bytes
                .get(cursor)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                cursor += 1;
            }
            bytes.get(cursor) == Some(&b'(')
        })
        .map(|start| {
            let end = delimited_end(text, start, b'[', b']')
                .unwrap_or_else(|| panic!("unterminated {prefix} at byte {start}"));
            let start_line = text[..start].bytes().filter(|byte| *byte == b'\n').count();
            let end_line = text[..end].bytes().filter(|byte| *byte == b'\n').count();
            (start_line, end_line, compact_attribute(&text[start..end]))
        })
        .collect()
}

fn cfg_attribute_blocks(text: &str) -> Vec<(usize, usize, String)> {
    attribute_blocks(text, "#[cfg(")
}

fn immediate_attribute_block(text: &str, line_index: usize) -> Option<String> {
    let lines = text.lines().collect::<Vec<_>>();
    let end = (0..line_index)
        .rev()
        .find(|index| !lines[*index].trim().is_empty())?;
    cfg_attribute_blocks(text)
        .into_iter()
        .find_map(|(_, block_end, attribute)| (block_end == end).then_some(attribute))
}

fn compact_attribute(attribute: &str) -> String {
    attribute
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn cfg_owned_code(text: &str, attribute_end: usize) -> Result<String, String> {
    let lines = text.lines().collect::<Vec<_>>();
    if let Some((_, suffix)) = lines[attribute_end].split_once(']') {
        if !suffix.trim().is_empty() {
            return Ok(suffix.trim().to_owned());
        }
    }
    let start = (attribute_end + 1..lines.len())
        .find(|index| !lines[*index].trim().is_empty())
        .ok_or_else(|| "cfg attribute owns no code".to_owned())?;
    let first = lines[start].trim();
    if !first.starts_with('{') {
        return Ok(first.to_owned());
    }
    let mut depth = 0_i32;
    for end in start..lines.len() {
        for character in lines[end].chars() {
            match character {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                return Err("malformed cfg-owned block".to_owned());
            }
        }
        if depth == 0 {
            return Ok(lines[start..=end]
                .iter()
                .map(|line| line.trim())
                .collect::<Vec<_>>()
                .join("\n"));
        }
    }
    Err("unterminated cfg-owned block".to_owned())
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
    Ok(compact_attribute(anchor))
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
            compact_attribute(&text[start..end])
        })
        .collect()
}

fn validate_control_capture_body(body: &str, real_work: &str) -> Result<(), String> {
    let feature_blocks = cfg_attribute_blocks(body)
        .into_iter()
        .filter(|(_, _, attribute)| attribute.contains(FEATURE))
        .collect::<Vec<_>>();
    let expected = [compact_attribute(GUARD), compact_attribute(FALLBACK_GUARD)];
    let observed = feature_blocks
        .iter()
        .map(|(_, _, attribute)| attribute.clone())
        .collect::<Vec<_>>();
    if observed != expected {
        return Err(format!("capture guard polarity drift: {observed:?}"));
    }
    let baseline = cfg_owned_code(body, feature_blocks[0].1)?;
    if !baseline.contains(real_work) || compact_attribute(&baseline) == "None" {
        return Err("baseline capture branch does not own the real work".to_owned());
    }
    let control = cfg_owned_code(body, feature_blocks[1].1)?;
    if compact_attribute(&control) != "None" || control.contains(real_work) {
        return Err("control capture branch is not exactly None".to_owned());
    }
    Ok(())
}

fn audit_non_spec_feature_cfg(relative: &str, text: &str) -> Result<Vec<usize>, String> {
    for (_, _, attribute) in attribute_blocks(text, "#[cfg_attr(") {
        if attribute.contains(FEATURE) {
            return Err(format!("feature-bearing cfg_attr in {relative}"));
        }
    }
    if cfg_macro_invocations(text)
        .iter()
        .any(|invocation| invocation.contains(FEATURE))
    {
        return Err(format!("feature-bearing cfg! in {relative}"));
    }
    let mut matches_by_attribute = Vec::new();
    for (_, end, attribute) in cfg_attribute_blocks(text)
        .into_iter()
        .filter(|(_, _, attribute)| attribute.contains(FEATURE))
    {
        let owned_anchor = cfg_owned_anchor(text, end)?;
        let matches = FEATURE_CFG_INVENTORY
            .iter()
            .enumerate()
            .filter(|(_, expected)| {
                expected.file == relative
                    && compact_attribute(expected.guard) == attribute
                    && owned_anchor.contains(&compact_attribute(expected.owned_anchor))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let [index] = matches.as_slice() else {
            return Err(format!(
                "unowned or ambiguous feature cfg in {relative}: guard={attribute} anchor={owned_anchor} matches={matches:?}"
            ));
        };
        matches_by_attribute.push(*index);
    }
    Ok(matches_by_attribute)
}

fn assert_exact_self_probe_cfg(text: &str) {
    let feature_attributes = cfg_attribute_blocks(text)
        .into_iter()
        .filter_map(|(_, _, attribute)| attribute.contains(FEATURE).then_some(attribute))
        .collect::<Vec<_>>();
    assert_eq!(
        feature_attributes,
        [
            compact_attribute(SELF_BAD_MODE_GUARD),
            compact_attribute(SELF_COUNTER_GUARD),
        ],
        "WU0F spec has exactly its two feature-bearing cfg attributes"
    );
    assert!(attribute_blocks(text, "#[cfg_attr(")
        .into_iter()
        .all(|(_, _, attribute)| !attribute.contains(FEATURE)));

    let compact = compact_attribute(text);
    let bad_owner = format!(
        "{}#[test]fnbad_mode_runs_exact_primary_and_fails_before_trace_creation_or_profile_load(",
        compact_attribute(SELF_BAD_MODE_GUARD)
    );
    assert_eq!(compact.matches(&bad_owner).count(), 1);
    let counter_owner = format!(
        "{}#[test]#[ignore=\"WU0Fordinarylibtestcountercoverage\"]fnwu0f_ordinary_counter_coverage_probe_once(",
        compact_attribute(SELF_COUNTER_GUARD)
    );
    assert_eq!(compact.matches(&counter_owner).count(), 1);

    let feature_macros = cfg_macro_invocations(text)
        .into_iter()
        .filter(|invocation| invocation.contains(FEATURE))
        .collect::<Vec<_>>();
    let expected_macro = format!("cfg!(feature=\"{FEATURE}\")");
    assert_eq!(feature_macros, [expected_macro.clone()]);
    let compile_probe = item_body(text, "fn wu0f_compile_state_probe_once(");
    assert_eq!(cfg_macro_invocations(compile_probe), [expected_macro]);
}

fn item_body<'a>(text: &'a str, owner: &str) -> &'a str {
    let owner_start = text
        .find(owner)
        .unwrap_or_else(|| panic!("missing `{owner}`"));
    let open = text[owner_start..]
        .find('{')
        .map(|offset| owner_start + offset)
        .unwrap_or_else(|| panic!("missing body for `{owner}`"));
    let mut depth = 0_u32;
    for (offset, character) in text[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open..=open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated body for `{owner}`")
}

fn profile_sha256(sources: &[InjectedLibrarySource<'_>]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"typokat-wu0f-witness-profile-v1");
    for source in sources {
        digest.update(
            u64::try_from(source.file_ordinal.index())
                .unwrap()
                .to_be_bytes(),
        );
        digest.update(u64::try_from(source.name.len()).unwrap().to_be_bytes());
        digest.update(source.name.as_bytes());
        digest.update(u64::try_from(source.source.len()).unwrap().to_be_bytes());
        digest.update(source.source.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn witness_semantic(label: &str, sources: &[InjectedLibrarySource<'_>]) -> (String, String) {
    let ordinary = run_injected_profile(sources).expect("ordinary witness completes");
    let expected = canonical_wu0d_semantic_identity_from_components_for_test(
        &ordinary.wu0d_semantic_components,
    )
    .aggregate_sha256;
    let scratch = Scratch::new(label);
    let trace =
        run_observed_profile_for_test(DiagnosticMode::Plain, sources, &scratch.join("plain.trace"))
            .expect("plain observed witness completes");
    assert!(trace.finished());
    assert!(trace.measurement().is_none());
    assert_eq!(trace.semantic_sha256(), Some(expected.as_str()));
    (profile_sha256(sources), expected)
}

fn emit_witness(label: &str, sources: &[InjectedLibrarySource<'_>]) {
    let (profile, semantic) = witness_semantic(label, sources);
    println!(
        "typokat-wu0f-witness-v1 witness={label} mode=plain profile_sha256={profile} \
semantic_sha256={semantic}"
    );
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(DIGITS[usize::from(byte >> 4)]));
        result.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    result
}

fn swap_launch_payloads(evidence: &str, left_seq: usize, right_seq: usize) -> String {
    let mut lines = evidence.lines().map(str::to_owned).collect::<Vec<_>>();
    let left_prefix = format!("launch seq={left_seq} ");
    let right_prefix = format!("launch seq={right_seq} ");
    let left = lines
        .iter()
        .position(|line| line.starts_with(&left_prefix))
        .expect("left launch");
    let right = lines
        .iter()
        .position(|line| line.starts_with(&right_prefix))
        .expect("right launch");
    let left_payload = lines[left][left_prefix.len()..].to_owned();
    let right_payload = lines[right][right_prefix.len()..].to_owned();
    lines[left] = format!("{left_prefix}{right_payload}");
    lines[right] = format!("{right_prefix}{left_payload}");
    lines.join("\n") + "\n"
}

fn evidence_fixture() -> String {
    let ids =
        ["a", "b", "c", "d", "e", "f", "1", "2", "3", "4", "5", "6"].map(|digit| digit.repeat(64));
    let [source_id, lock, rustc, profile, flags, workload, inventory, host, containment, limits, env, trace] =
        ids;
    let mut lines = vec![
        "typokat-wu0f-control-evidence-v1 builds=2 launches=13 schedule=compile-AB,coverage-A,witness-ABBAAB,primary-ABBA diagnostic_only=1 release_authority=none".to_owned(),
        format!("identity source={source_id} lock={lock} rustc={rustc} cargo_profile={profile} common_flags={flags} workload_profile={workload} warm_inventory={inventory} host={host} containment={containment} limits={limits} environment={env} trace_policy={trace}"),
        format!("build ordinal=0 variant=baseline target={} binary_sha256={BASELINE_BINARY_SHA256} feature=absent", "7".repeat(64)),
        format!("build ordinal=1 variant=control target={} binary_sha256={CONTROL_BINARY_SHA256} feature={FEATURE}", "9".repeat(64)),
    ];
    let coverage_record = format!(
        "typokat-wu0f-counter-coverage-v1 control=0 witnesses={}\n",
        ORDINARY_COUNTER_TESTS.len()
    );
    let schedule = [
        ("baseline", "compile-state", "0", "typokat-wu0f-compile-state-v1 control=0\n", "unavailable", "unavailable", "unavailable"),
        ("control", "compile-state", "0", "typokat-wu0f-compile-state-v1 control=1\n", "unavailable", "unavailable", "unavailable"),
        ("baseline", "counter-coverage", "0", coverage_record.as_str(), "unavailable", "unavailable", "unavailable"),
        ("baseline", "recursive", "0", "typokat-wu0f-witness-v1 witness=recursive mode=plain profile_sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa semantic_sha256=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n", "unavailable", SHA_A, SHA_B),
        ("control", "recursive", "0", "typokat-wu0f-witness-v1 witness=recursive mode=plain profile_sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa semantic_sha256=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n", "unavailable", SHA_A, SHA_B),
        ("control", "mapped", "0", "typokat-wu0f-witness-v1 witness=mapped mode=plain profile_sha256=cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc semantic_sha256=dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\n", "unavailable", SHA_C, SHA_D),
        ("baseline", "mapped", "0", "typokat-wu0f-witness-v1 witness=mapped mode=plain profile_sha256=cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc semantic_sha256=dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\n", "unavailable", SHA_C, SHA_D),
        ("baseline", "overload", "0", "typokat-wu0f-witness-v1 witness=overload mode=plain profile_sha256=eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee semantic_sha256=ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\n", "unavailable", SHA_E, SHA_F),
        ("control", "overload", "0", "typokat-wu0f-witness-v1 witness=overload mode=plain profile_sha256=eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee semantic_sha256=ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\n", "unavailable", SHA_E, SHA_F),
        ("baseline", "primary", "0", "", SHA_C, "unavailable", SHA_ONE),
        ("control", "primary", "0", "", SHA_C, "unavailable", SHA_ONE),
        ("control", "primary", "1", "", SHA_C, "unavailable", SHA_ONE),
        ("baseline", "primary", "1", "", SHA_C, "unavailable", SHA_ONE),
    ];
    for (
        seq,
        (variant, workload_name, repetition, record, trace_digest, witness_profile, semantic),
    ) in schedule.into_iter().enumerate()
    {
        let binary_sha256 = match variant {
            "baseline" => BASELINE_BINARY_SHA256,
            "control" => CONTROL_BINARY_SHA256,
            _ => unreachable!("closed fixture variant"),
        };
        lines.push(format!(
            "launch seq={seq} variant={variant} binary_sha256={binary_sha256} workload={workload_name} repetition={repetition} wall_us={} rss_bytes={} instructions={} termination=normal trace_sha256={trace_digest} witness_profile_sha256={witness_profile} semantic_sha256={semantic} record_hex={}",
            1000 + seq,
            1_000_000 + seq,
            10_000 + seq,
            hex(record.as_bytes()),
        ));
    }
    lines.join("\n") + "\n"
}

#[test]
fn feature_is_empty_default_off_and_test_only() {
    let manifest: toml::Value = toml::from_str(include_str!("../../../Cargo.toml")).unwrap();
    let features = manifest["features"].as_table().expect("features table");
    assert!(features[FEATURE]
        .as_array()
        .expect("control feature array")
        .is_empty());
    assert!(!features
        .get("default")
        .and_then(toml::Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(FEATURE))));
    for relative in ["src/driver.rs", "src/lib.rs", "src/main.rs"] {
        assert!(!source(relative).contains(FEATURE));
    }
}

#[test]
fn cfg_parser_preserves_multiline_and_inline_ownership_and_detects_alternate_forms() {
    let multiline = "#[cfg(all(\n    test,\n    not(feature = \"wu0-uninstrumented-control\")\n))]\nhot_call();\n";
    let multiline_blocks = cfg_attribute_blocks(multiline);
    let [(_, multiline_end, multiline_guard)] = multiline_blocks.as_slice() else {
        panic!("one multiline cfg")
    };
    assert_eq!(multiline_guard, &compact_attribute(GUARD));
    assert_eq!(
        cfg_owned_anchor(multiline, *multiline_end).unwrap(),
        "hot_call();"
    );

    let inline = format!("{GUARD} phase: ContextualMeasurePhase,\n");
    let inline_blocks = cfg_attribute_blocks(&inline);
    let [(_, inline_end, inline_guard)] = inline_blocks.as_slice() else {
        panic!("one inline cfg")
    };
    assert_eq!(inline_guard, &compact_attribute(GUARD));
    assert_eq!(
        cfg_owned_anchor(&inline, *inline_end).unwrap(),
        "phase:ContextualMeasurePhase,"
    );

    let cfg_attr =
        format!("#[cfg_attr(test, allow(dead_code), doc = \"{FEATURE}\")]\nfn item() {{}}\n");
    assert_eq!(attribute_blocks(&cfg_attr, "#[cfg_attr(").len(), 1);
    assert_eq!(
        cfg_macro_invocations(&format!("let enabled = cfg!(feature = \"{FEATURE}\");")),
        [format!("cfg!(feature=\"{FEATURE}\")")]
    );
}

#[test]
fn every_enumerated_runtime_site_has_the_exact_immediate_guard() {
    for site in HOT_SITES {
        let text = source(site.file);
        let occurrences = code_occurrences(&text, site.anchor);
        assert_eq!(
            occurrences.len(),
            site.total,
            "{} `{}` inventory drift",
            site.file,
            site.anchor
        );
        let guarded = occurrences
            .iter()
            .filter(|(line, _)| immediate_attribute(&text, *line) == Some(GUARD))
            .count();
        assert_eq!(
            guarded, site.guarded,
            "{} `{}` guard drift",
            site.file, site.anchor
        );
    }
}

#[test]
fn contextual_wrappers_select_the_normal_semantic_path_in_control_builds() {
    let calls = source("src/check/checker/calls.rs");
    assert_eq!(
        calls
            .matches(&format!(
                "{GUARD}\n        {{\n            with_contextual_measure_phase"
            ))
            .count(),
        1
    );
    assert_eq!(calls.matches(&format!("{FALLBACK_GUARD}\n        {{\n            $pass.infer_contextual_source_after_walked")).count(), 1);
    assert_eq!(
        calls
            .matches(&format!(
                "{GUARD}\n        {{\n            $pass.contextual_inference_args"
            ))
            .count(),
        1
    );
    assert_eq!(
        calls
            .matches(&format!(
                "{FALLBACK_GUARD}\n        {{\n            $pass.contextual_inference_args"
            ))
            .count(),
        1
    );
    assert_eq!(
        calls
            .matches(&format!(
                "#[cfg(all(test, not(feature = \"{FEATURE}\")))] phase: ContextualMeasurePhase"
            ))
            .count(),
        1
    );
}

#[test]
fn cold_measurement_apis_fields_and_initializers_remain_plain_cfg_test() {
    for (relative, anchor, expected) in COLD_TEST_SURFACE {
        let text = source(relative);
        let occurrences = code_occurrences(&text, anchor);
        assert_eq!(occurrences.len(), *expected);
        assert!(occurrences
            .iter()
            .all(|(line, _)| immediate_attribute(&text, *line) == Some("#[cfg(test)]")));
    }
    let checker = source("src/check/checker/mod.rs");
    for module in [
        "mod wu0c_attribution;",
        "mod wu0d_candidate_release;",
        "mod wu0d_candidate_release_spec;",
    ] {
        let [(line, _)] = code_occurrences(&checker, module).as_slice() else {
            panic!("one cold module {module}")
        };
        assert_eq!(immediate_attribute(&checker, *line), Some("#[cfg(test)]"));
    }
}

#[test]
fn control_captures_keep_the_cold_api_but_replace_tls_work_with_none() {
    for (relative, owner, real_work) in CONTROL_NOOP_CAPTURES {
        let text = source(relative);
        let body = item_body(&text, owner);
        validate_control_capture_body(body, real_work)
            .unwrap_or_else(|error| panic!("{relative}:{owner}: {error}"));
    }
}

#[test]
fn control_capture_validator_rejects_reversed_branch_bodies() {
    let good = format!(
        "fn capture() -> Option<u8> {{\n{GUARD}\n{{ TLS.with(|_| Some(1)) }}\n{FALLBACK_GUARD}\nNone\n}}"
    );
    validate_control_capture_body(item_body(&good, "fn capture("), "TLS.with").unwrap();
    let reversed = format!(
        "fn capture() -> Option<u8> {{\n{GUARD}\nNone\n{FALLBACK_GUARD}\n{{ TLS.with(|_| Some(1)) }}\n}}"
    );
    assert!(
        validate_control_capture_body(item_body(&reversed, "fn capture("), "TLS.with").is_err()
    );
}

#[test]
fn feature_cfg_inventory_is_reverse_closed_and_rejects_alternate_cfg_forms() {
    let mut observed_counts = vec![0_usize; FEATURE_CFG_INVENTORY.len()];
    let mut observed_total = 0_usize;
    let mut self_seen = false;
    for (relative, text) in rust_sources_under_src() {
        if relative == SELF_SPEC {
            assert!(!self_seen, "WU0F spec enumerated twice");
            self_seen = true;
            assert_exact_self_probe_cfg(&text);
            continue;
        }
        for index in
            audit_non_spec_feature_cfg(&relative, &text).unwrap_or_else(|error| panic!("{error}"))
        {
            observed_counts[index] += 1;
            observed_total += 1;
        }
    }
    assert!(self_seen, "WU0F spec omitted from recursive src audit");
    assert_eq!(
        observed_total,
        FEATURE_CFG_INVENTORY
            .iter()
            .map(|expected| expected.count)
            .sum::<usize>(),
        "closed feature cfg inventory total"
    );
    for (expected, observed) in FEATURE_CFG_INVENTORY.iter().zip(observed_counts) {
        assert_eq!(
            observed, expected.count,
            "feature cfg inventory drift: {} `{}` `{}`",
            expected.file, expected.guard, expected.owned_anchor
        );
    }
}

#[test]
fn recursive_feature_audit_rejects_a_formerly_unlisted_rust_file() {
    let rogue = format!("{GUARD}\nfn unrelated_semantic_owner() {{}}\n");
    assert!(audit_non_spec_feature_cfg("src/new_unlisted_module.rs", &rogue).is_err());
}

#[test]
fn semantic_owners_and_modules_are_never_immediately_feature_gated() {
    for (relative, owner) in SEMANTIC_OWNERS {
        let text = source(relative);
        let line = text
            .lines()
            .position(|line| line.contains(owner))
            .expect("semantic owner");
        assert!(
            !immediate_attribute_block(&text, line)
                .is_some_and(|attribute| attribute.contains(FEATURE)),
            "feature gated owner {relative}:{owner}"
        );
    }
    let checker = source("src/check/checker/mod.rs");
    for module in [
        "mod calls;",
        "mod eval;",
        "pub(crate) mod wu0b_library;",
        "mod wu0e_observer;",
        "mod wu0e_diagnostic;",
    ] {
        let line = checker
            .lines()
            .position(|line| line.contains(module))
            .expect("owner module");
        assert!(!immediate_attribute_block(&checker, line)
            .is_some_and(|attribute| attribute.contains(FEATURE)));
    }
}

#[test]
fn witness_profiles_complete_and_coarse_plain_observation_is_semantically_neutral() {
    for (label, sources) in [
        ("recursive", RECURSIVE_SOURCES),
        ("mapped", MAPPED_SOURCES),
        ("overload", OVERLOAD_SOURCES),
    ] {
        let (profile, semantic) = witness_semantic(label, sources);
        assert_eq!(profile.len(), 64);
        assert_eq!(semantic.len(), 64);
    }
}

#[test]
fn canonical_evidence_parser_enforces_build_launch_metric_and_parity_contract() {
    let valid = evidence_fixture();
    let parsed = parse_control_evidence_for_test(valid.as_bytes()).expect("valid WU0F evidence");
    assert_eq!(render_control_evidence_for_test(&parsed), valid);
    let missing_coverage = valid
        .lines()
        .filter(|line| !line.contains("workload=counter-coverage"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let coverage_line = valid
        .lines()
        .find(|line| line.contains("workload=counter-coverage"))
        .expect("coverage launch");
    let failing_coverage = valid.replacen(
        coverage_line,
        &coverage_line.replace("termination=normal", "termination=exit-101"),
        1,
    );
    let coverage_from_control_binary = valid.replacen(
        coverage_line,
        &coverage_line.replace(BASELINE_BINARY_SHA256, CONTROL_BINARY_SHA256),
        1,
    );
    let control_compile_line = valid
        .lines()
        .find(|line| line.starts_with("launch seq=1 "))
        .expect("control compile-state launch");
    let generic_binary_mismatch = valid.replacen(
        control_compile_line,
        &control_compile_line.replace(CONTROL_BINARY_SHA256, SHA_A),
        1,
    );
    let reordered_coverage = swap_launch_payloads(&valid, 2, 3);
    for invalid in vec![
        valid.replacen("builds=2", "builds=1", 1),
        valid.replacen("feature=absent", &format!("feature={FEATURE}"), 1),
        valid.replacen(
            &format!("target={}", "7".repeat(64)),
            &format!("target={}", "9".repeat(64)),
            1,
        ),
        valid.replacen(
            "schedule=compile-AB,coverage-A,witness-ABBAAB,primary-ABBA",
            "schedule=compile-AB,witness-ABBAAB,coverage-A,primary-ABBA",
            1,
        ),
        missing_coverage,
        failing_coverage,
        coverage_from_control_binary,
        generic_binary_mismatch,
        reordered_coverage,
        valid.replacen("rss_bytes=1000000", "rss_bytes=unavailable", 1),
        valid.replacen("instructions=10000", "instructions=unavailable", 1),
        valid.replacen(
            &hex(b"typokat-wu0f-compile-state-v1 control=1\n"),
            &hex(b"typokat-wu0f-compile-state-v1 control=0\n"),
            1,
        ),
        valid.replacen("semantic_sha256=bbbb", "semantic_sha256=cccc", 1),
        valid.replacen(
            &format!("semantic_sha256={SHA_ONE}"),
            &format!("semantic_sha256={SHA_C}"),
            1,
        ),
        valid.replacen("release_authority=none", "release_authority=go", 1),
    ] {
        assert!(parse_control_evidence_for_test(invalid.as_bytes()).is_err());
    }
}

#[test]
fn coordinator_surface_is_diagnostic_only_and_has_no_release_evidence_vocabulary() {
    let runner = source(CONTROL_RUNNER);
    let implementation = source("src/check/checker/wu0f_hotpath_control.rs");
    for required in [
        "--self-test",
        "--no-run",
        "--exact",
        COUNTER_COVERAGE_PROBE,
        COMPILE_STATE_PROBE,
        RECURSIVE_PROBE,
        MAPPED_PROBE,
        OVERLOAD_PROBE,
        PRIMARY_PROBE,
        "diagnostic_only=1",
        "release_authority=none",
    ] {
        assert!(runner.contains(required), "coordinator lacks `{required}`");
    }
    for text in [&runner, &implementation] {
        let lower = text.to_ascii_lowercase();
        for forbidden in [
            "wu0d",
            "release-evidence",
            "gatedecision",
            "authorizes_release",
            "authorization=go",
            "release_authority=go",
        ] {
            assert!(!lower.contains(forbidden), "WU0F contains `{forbidden}`");
        }
    }
}

#[cfg(feature = "wu0-uninstrumented-control")]
#[test]
fn bad_mode_runs_exact_primary_and_fails_before_trace_creation_or_profile_load() {
    let scratch = Scratch::new("bad-mode");
    let diagnostic = source("src/check/checker/wu0e_diagnostic.rs");
    let primary = diagnostic
        .split_once("fn wu0e_primary_probe_once()")
        .expect("exact primary probe")
        .1;
    let resolve = primary.find("resolve_workload_environment()").unwrap();
    let sink = primary
        .find("DiagnosticTraceSink::create(&environment.trace_path)")
        .unwrap();
    let load = primary.find("load_strict_profile()").unwrap();
    assert!(resolve < sink && sink < load);
    for mode in ["measured-off", "candidate-b"] {
        let trace = scratch.join(&format!("{mode}.trace"));
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--ignored", "--exact", PRIMARY_PROBE, "--nocapture"]);
        for (key, _) in std::env::vars().filter(|(key, _)| key.starts_with("TYPOKAT_WU0E_")) {
            command.env_remove(key);
        }
        let output = command
            .env("TYPOKAT_WU0E_MODE", mode)
            .env("TYPOKAT_WU0E_TRACE_PATH", &trace)
            .output()
            .expect("run exact feature primary with rejected mode");
        assert!(!output.status.success());
        assert!(!trace.exists(), "rejected mode created a trace");
    }
}

#[test]
#[ignore = "WU0F executable coordinator self-test"]
fn coordinator_self_test_is_executable_and_parser_checked() {
    let output = Command::new("perl")
        .args([CONTROL_RUNNER, "--self-test"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("launch WU0F coordinator self-test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report =
        parse_control_runner_self_test_report(&output.stdout).expect("canonical self-test report");
    assert_eq!(
        ControlRunnerSelfTestCase::ALL,
        [
            ControlRunnerSelfTestCase::BuildIsolation,
            ControlRunnerSelfTestCase::IdentityParity,
            ControlRunnerSelfTestCase::FeaturePolarity,
            ControlRunnerSelfTestCase::ContainmentParity,
            ControlRunnerSelfTestCase::AlternatingSchedule,
            ControlRunnerSelfTestCase::PerLaunchMetrics,
            ControlRunnerSelfTestCase::CompileStatePolarity,
            ControlRunnerSelfTestCase::CounterCoverage,
            ControlRunnerSelfTestCase::WitnessParity,
            ControlRunnerSelfTestCase::PrimaryParity,
            ControlRunnerSelfTestCase::DiagnosticOnly,
        ]
    );
    assert_eq!(
        report.passed_cases(),
        ControlRunnerSelfTestCase::ALL.as_slice()
    );
}

#[cfg(not(feature = "wu0-uninstrumented-control"))]
#[test]
#[ignore = "WU0F ordinary libtest counter coverage"]
fn wu0f_ordinary_counter_coverage_probe_once() {
    let executable = std::env::current_exe().expect("current libtest");
    for test in ORDINARY_COUNTER_TESTS {
        let output = Command::new(&executable)
            .args(["--exact", test, "--nocapture"])
            .env("RUST_TEST_THREADS", "1")
            .output()
            .unwrap_or_else(|error| panic!("launch {test}: {error}"));
        assert!(
            output.status.success(),
            "counter witness {test} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    println!(
        "typokat-wu0f-counter-coverage-v1 control=0 witnesses={}",
        ORDINARY_COUNTER_TESTS.len()
    );
}

#[test]
#[ignore = "WU0F compile-state probe"]
fn wu0f_compile_state_probe_once() {
    println!(
        "typokat-wu0f-compile-state-v1 control={}",
        u8::from(cfg!(feature = "wu0-uninstrumented-control"))
    );
}

#[test]
#[ignore = "WU0F recursive semantic witness"]
fn wu0f_recursive_witness_probe_once() {
    emit_witness("recursive", RECURSIVE_SOURCES);
}

#[test]
#[ignore = "WU0F mapped semantic witness"]
fn wu0f_mapped_witness_probe_once() {
    emit_witness("mapped", MAPPED_SOURCES);
}

#[test]
#[ignore = "WU0F overload semantic witness"]
fn wu0f_overload_witness_probe_once() {
    emit_witness("overload", OVERLOAD_SOURCES);
}

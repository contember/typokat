//! Conformance harness (mvp-plan §6, `tests/cases/README.md`).
//!
//! This is the spec corpus runner: for each `.ts` fixture it parses the inline
//! `// error[CODE]: substring` markers, runs the checker, maps each diagnostic's
//! primary-span START to a 1-based source line, and asserts that — per line — the
//! multiset of expected codes equals the multiset of actual codes, and each
//! expected substring is contained (case-sensitive) in the corresponding
//! diagnostic's fully-rendered text.
//!
//! M0–M22 (`m0_assign_primitives/`, `m1_binder_inference/`, `m2_objects/`,
//! `m3_functions/`, `m4_unions/`, `m5_named_recursive/`, `m6_reporting/`,
//! `m7_narrowing/`, `m8_discriminated/`, `m9_generics/`, `m10_inference/`,
//! `m11_classes/`, `m12_inheritance/`, `m13_modifiers/`, `m14_readonly/`,
//! `m15_accessors/`, `m16_generic_classes/`, `m17_arrays/`, `m18_tuples/`,
//! `m19_index_sig/`, `m20_keyof/`, `m21_optional/`, `m22_unresolved_type/`) are
//! enabled. `MILESTONE_DIRS` enables whole corpora; `ENABLED_FIXTURES` gates a
//! selected flat subset while the rest of its corpus remains disabled.
//!
//! Every corpus also names the **base** its fixtures are checked against
//! ([`FixtureBase`]): `crates/typokat-check/src/prelude.ts` (production) or the full TypeScript 6.0.3 default
//! library. The base is declared once per directory in `MILESTONE_DIRS` and applies to that
//! directory's `ENABLED_FIXTURES` / `ENABLED_PROJECT_FIXTURES` rows too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use typokat::check::test_support::{
    check_project as check_project_without_library,
    check_source as check_source_without_library,
};
use typokat::driver::{check_project, check_source, CheckOutput};
use typokat::frontend::FileInput;
use typokat::span::LineIndex;

use FixtureBase::{Library, Prelude};

/// Which type universe a corpus's fixtures are checked in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FixtureBase {
    /// `crates/typokat-check/src/prelude.ts` — the production path (`check_source` / `check_project`).
    Prelude,
    /// The full TypeScript 6.0.3 default library (`check_source_with_library` /
    /// `check_project_with_library`). Backlog-14 scaffolding: the library entry points are a
    /// temporary sibling of the production ones and go away with the WU7 cutover.
    Library,
}

/// Milestone fixture directories under `tests/cases/`, each with its enablement flag and the
/// base it is checked against. Only enabled dirs run. Enable later milestones here as they are
/// implemented.
const MILESTONE_DIRS: &[(&str, bool, FixtureBase)] = &[
    ("m0_assign_primitives", true, Prelude),
    ("m1_binder_inference", true, Prelude),
    ("m2_objects", true, Prelude),
    ("m3_functions", true, Prelude),
    ("m4_unions", true, Prelude),
    ("m5_named_recursive", true, Prelude),
    ("m6_reporting", true, Prelude),
    ("m7_narrowing", true, Prelude),
    ("m8_discriminated", true, Prelude),
    ("m9_generics", true, Prelude),
    ("m10_inference", true, Prelude),
    ("m11_classes", true, Prelude),
    ("m12_inheritance", true, Prelude),
    ("m13_modifiers", true, Prelude),
    ("m14_readonly", true, Prelude),
    ("m15_accessors", true, Prelude),
    ("m16_generic_classes", true, Prelude),
    ("m17_arrays", true, Prelude),
    ("m18_tuples", true, Prelude),
    ("m19_index_sig", true, Prelude),
    ("m20_keyof", true, Prelude),
    ("m21_optional", true, Prelude),
    ("m22_unresolved_type", true, Prelude),
    ("m23_unstructured_narrowing", true, Prelude),
    ("m24_generic_constraints", true, Prelude),
    ("m25_conditional_types", true, Prelude),
    ("m26_mapped_types", true, Prelude),
    ("m27_template_literals", true, Prelude),
    ("m28_utility_types", true, Prelude),
    // M29 modules are project fixtures (subdirectories with multiple files), not
    // flat single-file fixtures. Registered false by the spec commit; the
    // implementation commit extends the harness before flipping this on.
    ("m29_modules", true, Prelude),
    ("m30_contextual_literals", true, Prelude),
    // M31 intersection types. Registered false by the spec commit; the
    // implementation commit flips this on.
    ("m31_intersections", true, Prelude),
    ("m32_signature_shape", true, Prelude),
    // M33 function overloads. Registered false by the spec commit; the
    // implementation commit flips this on.
    ("m33_function_overloads", true, Prelude),
    // Bug-fix corpora (official-suite findings / backlog items). Each is
    // committed `false` as a behavior-neutral spec, then flipped `true` by the
    // commit that lands its fix. See tests/cases/README.md ("Bug-fix corpora").
    ("f1_object_interface_methods", true, Prelude),
    ("f1_object_interface_call", true, Prelude),
    ("f1_object_interface_construct", true, Prelude),
    ("f3_class_member_collection", true, Prelude),
    ("f4_destructuring_access", true, Prelude),
    ("f5_union_readonly", true, Prelude),
    // Backlog corpora (same mechanism, named by backlog item ID).
    // Backlog 41 generic methods/signatures.
    ("b41_generic_methods", true, Prelude),
    ("b06_class_completeness", true, Prelude),
    ("b20_ctor_accessibility", true, Prelude),
    // Backlog 22 — parenthesized/const-aliased class values must retain abstract
    // and constructor-accessibility facts. WU0 commits disabled; WU1 enables.
    ("b22_new_callee_forms", true, Prelude),
    ("b28_interface_extends", true, Prelude),
    ("b29_alias_cycles", true, Prelude),
    ("b30_negative_literals", true, Prelude),
    // Backlog 32 — `keyof` must observe declarations filled later in source
    // order. WU0 commits the corpus disabled; the fix enables it.
    ("b32_eager_keyof_forward", true, Prelude),
    // Backlog 30 JS-exact number stringification.
    ("b30_numeric_stringify", true, Prelude),
    ("b55_template_memo", true, Prelude),
    // Backlog 56 — cycles must diagnose and taint ancestor evaluator frames so
    // no error-derived value reaches the durable memo. WU0 disabled; WU2 enables.
    ("b56_instantiation_cycles", true, Prelude),
    // Project fixtures (subdirectories, m29 convention) — see `PROJECT_DIRS`.
    ("b58_project_scopes", true, Prelude),
    ("b61_field_initializers", true, Prelude),
    ("b53_cfg_assignments", true, Prelude),
    ("b57_tuple_array_infer", true, Prelude),
    ("b64_readonly_infer_binder", true, Prelude),
    ("b34_fix_params_keyof", true, Prelude),
    ("b33_as_cast_assignability", true, Prelude),
    ("b54_labeled_statements", true, Prelude),
    ("b59_modules_hygiene", true, Prelude),
    ("b65_inference_candidate_policy", true, Prelude),
    // Backlog 45 — binary operator result typing plus the arithmetic operand rule
    // (TK2362/TK2363) and the `+` general mismatch (TK2365). Enabled by its spec
    // commit, which is RED until the implementation lands.
    ("b45_operator_result_typing", true, Prelude),
    // Backlog 101 — a ternary and a logical expression carry a real VALUE type (the
    // arm union / the falsy-truthy-nullish split joined with the right operand)
    // instead of the error type, so everything downstream of them is checked again.
    // Enabled by its spec commit, which is RED until the implementation lands.
    ("b101_conditional_logical_values", true, Prelude),
    // Backlog 104 — the excess-property check must descend through a ternary / logical
    // into the operands the target contextually shapes, instead of recognising an object
    // literal only DIRECTLY in the checked position. Enabled by its spec commit, which is
    // RED until the implementation lands.
    ("b104_excess_property_descent", true, Prelude),
    // Backlog 87 — the reason chain must be bounded like the type renderer next to it:
    // past REASON_DEPTH_LIMIT nesting levels it collapses to one elision line plus the
    // retained innermost cause, and the indent stops growing. Enabled by its spec commit,
    // which is RED until the implementation lands.
    ("b87_reason_chain_depth", true, Prelude),
    // Backlog 100 — a composed condition (`&&`, `||`, and `!` over them) must narrow
    // the branch it guards. `analyze_guard` has no `LogicalExpression` arm and
    // `build_flow_logical` joins both senses back together, so today the guarded branch
    // reads the DECLARED union. Enabled by its spec commit, which is RED until the
    // implementation lands. See tests/cases/README.md ("Bug-fix corpora").
    ("b100_logical_condition_narrowing", true, Prelude),
    // ADR-0016 review finding — a cached failure must not change WHICH failure a
    // later, independent statement reports. Committed `false`: the trio only goes
    // green with the `getUnmatchedProperty`-style presence pass in `relate_objects`,
    // which is filed separately. See `docs/reference/divergences.md`.
    ("b91_missing_property_presence", false, Prelude),
    // Backlog 92 — one error nested `d` levels deep inside contextually typed
    // arguments is reported once, not 2^d times: exactly one of the raw and the
    // committed argument walk commits per level. The per-line multiset rule in
    // `compare_fixture_output` is what turns a single marker into an
    // exactly-once assertion — no marker-format change is involved.
    ("b92_contextual_duplicate_diagnostics", true, Prelude),
    // Backlog 67 shipped; backlog 66 remains disabled behind its protected-lineage
    // architecture stop gate (backlog 63d).
    ("b67_utility_alias_constraint", true, Prelude),
    // Backlog 70 — explicit receiver slots, receiver call diagnostics, lib.es5
    // receiver utilities, and contextual ThisType.
    ("b70_this_parameter_typing", true, Prelude),
    // Backlog 77 — conditional infer must extract the last represented object
    // call-signature return. WU0 commits disabled; WU3 enables.
    ("b77_returntype_call_signatures", true, Prelude),
    // Backlog 78 — generic class const aliases retain substitution and class facts.
    // Review byproduct from backlog 22; independently disabled.
    ("b78_generic_class_value_aliases", false, Prelude),
    ("b66_protected_override_compat", false, Prelude),
    // Backlog 38 minimal ambient prelude. WU1 commits this corpus disabled;
    // WU2 flips it on with the canonical prelude declarations.
    ("b38_minimal_ambient_prelude", true, Prelude),
    // Backlog 38 follow-up: prelude lookup must not cross a type-only import
    // barrier or a module export boundary. Project-shaped; see PROJECT_DIRS.
    ("b38_prelude_lookup_boundaries", true, Prelude),
    // Soundness-review-fixes sprint (2026-07-10) WU0 corpora. Each is committed
    // `false` (behavior-neutral spec); WU1-WU3 flip only their own dir when the
    // fix lands, the deferred-ledger dir stays `false` beyond this sprint. See
    // docs/archive/sprint-2026-07-10-soundness-review-fixes.md and
    // tests/cases/README.md ("Soundness-review corpora").
    // WU1 — nested assignment expressions, complete return inference, loop/throw
    // body checking (findings 1-3).
    ("sr_wu1_expressions", true, Prelude),
    // WU2 — switch-local scope boundary + local function overloads (findings 5-6).
    ("sr_wu2_scope_overloads", true, Prelude),
    // WU2 — type-only export/value separation (finding 4). Project-shaped; also
    // registered in PROJECT_DIRS below.
    ("sr_wu2_export_space", true, Prelude),
    // WU3 — any&never, source-intersection nominal origin, string-index keyof,
    // recursive mapped types, deep-annotation depth guard (findings 7-10 + backlog
    // 63k). NOTE: recursive_mapped.ts and deep_annotation.ts stack-overflow at HEAD
    // and only become safe to run once WU3's recursion/depth guards land.
    ("sr_wu3_types_recursion", true, Prelude),
    // Deferred ledger — known under-reports from backlogs 18, 60, 62, 66, and
    // 76. Backlogs 56 and 77 moved to isolated corpora above.
    ("sr_deferred_ledger", false, Prelude),
    // Rewrite/hotpath-hardening sprint (2026-07-13) WU0 acceptance corpus.
    // Enabled after WU1-WU3 landed their owning fixes; overload_trial_depth.ts
    // continues to pin tsc parity with the architecture probes.
    ("sr_rewrite_hotpath_wu0", true, Prelude),
    // WU7 — deep acyclic generic metadata reaches both auxiliary structural
    // walkers. Enabled after their separate heap task/value stacks landed; their
    // distinct rewrite and constraint-evaluation policies remain covered directly.
    ("sr_rewrite_hotpath_wu7", true, Prelude),
    // WU8 — mapped-value replacement uses its own local iterative work stack;
    // the enabled corpus pins both the shallow source route and generic metadata.
    ("sr_rewrite_hotpath_wu8", true, Prelude),
    // Semantic-duplication/layering sprint WU1 — immutable class applications,
    // one-time class surface lowering, lexical effects, and typed exhaustion.
    ("sr_semantic_duplication", true, Prelude),
    // Cross-module/opposite-order class publication. Project-shaped; also
    // registered in PROJECT_DIRS below.
    ("sr_semantic_duplication_project", true, Prelude),
    // Completeness-accounting sprint (2026-07-10) — surface-accounting corpus
    // (backlog 73). ENABLED by WU3 (expression child slots), WU4 (statement
    // containers), and WU5 (annotation / signature / class-member accounting): the
    // fixtures emit their prescribed `incomplete[<role/surface/slot>]` records and/or
    // ordinary `error[TK…]`, diffed by `compare_incomplete_output` /
    // `compare_fixture_output`. See tests/cases/README.md ("Surface-accounting corpus").
    ("b73_surface_accounting", true, Prelude),
    // Backlog 73 expression-shape tail. WU1 makes every remaining `infer_expr`
    // shape explicit, retaining nested diagnostics before incomplete emissions.
    ("b73_expression_shape_tail", true, Prelude),
    // Backlog 74 declaration-hoisting parity. Enabled after WU1 and WU2 land.
    ("b74_declaration_hoisting", true, Prelude),
    // Namespace/declaration-merging sprint. A mixed flat/project corpus: the directory stays
    // `false` and its admitted slice is enabled fixture by fixture through `ENABLED_FIXTURES`
    // and `ENABLED_PROJECT_FIXTURES`. Registered here so every corpus declares its base.
    ("b43_namespaces_declaration_merging", false, Prelude),
    // Backlog 14 full TypeScript 6.0.3 default-library loading — the only corpora checked
    // against `Library`. Both directories stay `false`: the fixtures that already pass are
    // enabled one at a time through `ENABLED_FIXTURES` / `ENABLED_PROJECT_FIXTURES`, and the
    // rest wait for the loader defect fixes.
    ("b14_full_lib_loading", false, Library),
    ("b14_full_lib_loading_project", false, Library),
    // Backlog 102 — a binder write that lands inside the frozen library prefix must either reach
    // a delta-side scope (fresh script globals) or be recorded (a library-owned row), never
    // vanish. Checked against `Library`, because the prefix only exists on that base.
    ("b102_frozen_prefix_writes", true, Library),
    ("b102_frozen_prefix_writes_project", true, Library),
    // Backlog 103 correctness tier — collisions route through one sparse private epoch.
    ("b103_library_merge_correctness", true, Library),
    ("b103_library_merge_correctness_project", true, Library),
];

/// Milestone dirs whose fixtures are **project subdirectories** (multiple `.ts`
/// files per project, checked together via [`check_project`]) rather than flat
/// single-file fixtures. Every other dir is a flat corpus. Keep in sync with the
/// project-shaped corpora in `MILESTONE_DIRS`.
const PROJECT_DIRS: &[&str] = &[
    "m29_modules",
    "b58_project_scopes",
    "b59_modules_hygiene",
    "b38_prelude_lookup_boundaries",
    // Soundness-review WU2 type-only export/value separation (finding 4).
    "sr_wu2_export_space",
    "sr_semantic_duplication_project",
    // Backlog 14 collision/preflight routing matrix, checked against the `Library` base.
    "b14_full_lib_loading_project",
    // Backlog 102 cross-file script globals, checked against the `Library` base.
    "b102_frozen_prefix_writes_project",
    // Backlog 103 successful private-merge acceptance, checked against the `Library` base.
    "b103_library_merge_correctness_project",
];

/// Selected project fixtures enabled before their mixed flat/project corpus closes. Each runs
/// against the base its directory declares in `MILESTONE_DIRS`.
const ENABLED_PROJECT_FIXTURES: &[(&str, &str)] = &[
    // Backlog 14 (`Library` base). The other ten projects await WU6 isolated verification,
    // expectation reconciliation, and residual model-gap work.
    ("b14_full_lib_loading_project", "duplicate_global_deferred"),
    ("b14_full_lib_loading_project", "fast_external_module"),
    (
        "b43_namespaces_declaration_merging",
        "wu5_global_augmentation_forward",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu5_global_augmentation_reverse",
    ),
    ("b43_namespaces_declaration_merging", "wu6a_project_forward"),
    ("b43_namespaces_declaration_merging", "wu6a_project_reverse"),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_cross_space_forward",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_cross_space_reverse",
    ),
];

/// Flat fixtures that are enabled before their containing corpus can run in
/// full. Keep this list path-sorted so execution and failure aggregation stay
/// deterministic. Each runs against the base its directory declares in `MILESTONE_DIRS`.
const ENABLED_FIXTURES: &[(&str, &str)] = &[
    // Backlog 14 (`Library` base). The other four flat fixtures await WU6 isolated verification,
    // expectation reconciliation, and residual model-gap work.
    (
        "b14_full_lib_loading",
        "generic_application_cache_diagnostics.ts",
    ),
    ("b14_full_lib_loading", "arrays_tuples_readonly.ts"),
    ("b14_full_lib_loading", "global_values.ts"),
    ("b14_full_lib_loading", "intrinsic_aliases.ts"),
    ("b14_full_lib_loading", "iterator_library_local_nonleak.ts"),
    ("b14_full_lib_loading", "library_identity_shadowing.ts"),
    (
        "b14_full_lib_loading",
        "native_array_annotation_identity.ts",
    ),
    (
        "b14_full_lib_loading",
        "primitive_object_function_members.ts",
    ),
    ("b14_full_lib_loading", "promise_iterators_generators.ts"),
    ("b14_full_lib_loading", "regexp_literals.ts"),
    (
        "b43_namespaces_declaration_merging",
        "class_interface_conflicts.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "class_interface_namespace_generics.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "class_interface_namespace_nominal.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "class_interface_namespace_order_matrix.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "class_interface_namespace_orders.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "class_interface_recursion.ts",
    ),
    ("b43_namespaces_declaration_merging", "degraded_chimera.ts"),
    (
        "b43_namespaces_declaration_merging",
        "global_augmentation.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "global_missing_declare_negative.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "global_script_negative.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "global_value_publication_deferred.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "interface_conflicts.ts",
    ),
    ("b43_namespaces_declaration_merging", "interface_members.ts"),
    (
        "b43_namespaces_declaration_merging",
        "interface_recursion.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "interface_var_constructors.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "keep_pairs_forward.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "keep_pairs_reverse.ts",
    ),
    ("b43_namespaces_declaration_merging", "namespace_forms.ts"),
    ("b43_namespaces_declaration_merging", "umd_export.d.ts"),
    (
        "b43_namespaces_declaration_merging",
        "umd_export_negatives.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "umd_export_nonmodule.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu2_annotation_recovery.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu2_ambient_export_alias_list.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu2_checker_local_qualified_roots.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu2_interface_traversal.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu2_qualified_contexts.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_cyclic_heritage_recovery.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_deferred_heritage_identity.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_forward_reopening_qualified_leaf.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_heritage_index_conflicts.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_independent_heritage_conflicts.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_merged_interface_recovery.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_qualified_alias_chain.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_qualified_class_surfaces.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_qualified_generic_leaves.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_sole_supplied_constraint_orders.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu3_unavailable_header_metadata.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu4_class_namespace_matrix.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu4_function_namespace_inferred_boundary.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu4_function_namespace_matrix.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu4_interface_namespace_type_only.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu4_namespace_member_validation.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu4_namespace_payload_incomplete_ledger.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu4_standalone_namespace_boundary.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6_ambient_namespace_body_lookup.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6_local_array_heritage.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_first_class_values.ts",
    ),
    ("b43_namespaces_declaration_merging", "wu6a_mutability.ts"),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_nested_dotted_values.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_ambient_export_alias.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_alias_dependencies.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_attached_const.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_body_traversal.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_class_dependencies.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_nested_owner_merges.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_private_unsupported_declarations.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_qualified_missing_child.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_root_provenance.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_static_cycle_events.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_review_type_only_fallthrough.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_root_operations.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu6a_type_only_negative.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu7_official_callable_relation.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu7_official_interface_validation.ts",
    ),
    (
        "b43_namespaces_declaration_merging",
        "wu7_official_named_tuple.ts",
    ),
];

/// An expectation parsed from a single inline marker.
#[derive(Debug, Clone)]
struct ExpectedMarker {
    code: String,
    /// Optional case-sensitive substring the rendered diagnostic must contain.
    substring: Option<String>,
}

/// An expectation parsed from an inline `// incomplete[<stable-id>]` marker — the
/// third-outcome analogue of [`ExpectedMarker`]. The `id` is matched by exact
/// identity (same discipline as error codes); the optional substring is a
/// case-sensitive `contains` against the record's rendered text.
#[derive(Debug, Clone)]
struct ExpectedIncomplete {
    id: String,
    substring: Option<String>,
}

/// The M0 conformance entry point. Runs every enabled milestone dir and reports
/// all failures together so a regression is debuggable in one shot.
#[test]
fn conformance() {
    let cases_root = cases_root();
    let mut failures: Vec<String> = Vec::new();
    let mut files_checked = 0usize;

    for (dir, enabled, base) in MILESTONE_DIRS {
        if !*enabled {
            continue;
        }
        let dir_path = cases_root.join(dir);
        if PROJECT_DIRS.contains(dir) {
            let mut projects = discover_project_dirs(&dir_path);
            projects.sort();
            assert!(
                !projects.is_empty(),
                "no project fixtures found in {}",
                dir_path.display()
            );
            for project in projects {
                let fixtures = discover_ts_files_recursive(&project);
                files_checked += fixtures.len();
                if let Err(project_failures) = run_project_fixture(&project, fixtures, *base) {
                    failures.extend(project_failures);
                }
            }
            continue;
        }
        let mut fixtures = discover_ts_files(&dir_path);
        fixtures.sort();
        assert!(
            !fixtures.is_empty(),
            "no .ts fixtures found in {}",
            dir_path.display()
        );
        for fixture in fixtures {
            files_checked += 1;
            if let Err(file_failures) = run_fixture(&fixture, *base) {
                failures.extend(file_failures);
            }
        }
    }

    for (dir, file) in ENABLED_FIXTURES {
        let fixture = cases_root.join(dir).join(file);
        assert!(
            fixture.is_file(),
            "fixture not found: {}",
            fixture.display()
        );
        files_checked += 1;
        if let Err(file_failures) = run_fixture(&fixture, fixture_base(dir)) {
            failures.extend(file_failures);
        }
    }

    for (dir, project) in ENABLED_PROJECT_FIXTURES {
        let base = fixture_base(dir);
        let project = cases_root.join(dir).join(project);
        assert!(
            project.is_dir(),
            "project fixture not found: {}",
            project.display()
        );
        let fixtures = discover_ts_files_recursive(&project);
        files_checked += fixtures.len();
        if let Err(project_failures) = run_project_fixture(&project, fixtures, base) {
            failures.extend(project_failures);
        }
    }

    assert!(files_checked > 0, "no fixtures were checked");

    if !failures.is_empty() {
        panic!(
            "conformance failures ({} across {} files):\n\n{}",
            failures.len(),
            files_checked,
            failures.join("\n\n")
        );
    }
}

/// The base a fixture directory is checked against, from its `MILESTONE_DIRS` row. Every
/// directory must have one — an `ENABLED_FIXTURES` entry for an unregistered directory is a
/// harness bug, not a silent `Prelude` default.
fn fixture_base(dir: &str) -> FixtureBase {
    MILESTONE_DIRS
        .iter()
        .find(|(name, _, _)| *name == dir)
        .map(|(_, _, base)| *base)
        .unwrap_or_else(|| panic!("fixture dir {dir} is not registered in MILESTONE_DIRS"))
}

/// Run one fixture against `base`; return `Err(messages)` on any mismatch.
fn run_fixture(path: &Path, base: FixtureBase) -> Result<(), Vec<String>> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));

    // A declaration file's context is filename-derived, so it runs through the one-file
    // project path even when it is a flat fixture.
    let declaration_file = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(".d."));
    let one_file_project = || {
        vec![FileInput {
            name: path.display().to_string(),
            source: source.clone(),
        }]
    };
    let output = match (base, declaration_file) {
        (FixtureBase::Prelude, true) => {
            raw_project_reports(one_file_project())
                .pop()
                .expect("one-file declaration project report")
                .output
        }
        (FixtureBase::Prelude, false) => raw_output(check_source_without_library(&source)),
        (FixtureBase::Library, true) => match check_project(one_file_project()) {
            Ok(mut reports) => {
                reports
                    .pop()
                    .expect("one-file declaration project report")
                    .output
            }
            Err(error) => return Err(vec![library_base_failure(path, &error.to_string())]),
        },
        (FixtureBase::Library, false) => match check_source(&source) {
            Ok(output) => output,
            Err(error) => return Err(vec![library_base_failure(path, &error.to_string())]),
        },
    };
    compare_fixture_output(path, &source, &output)
}

fn library_base_failure(path: &Path, error: &str) -> String {
    format!(
        "{}: the default-library base is unavailable: {error}",
        display_path(path)
    )
}

fn run_project_fixture(
    project: &Path,
    mut fixtures: Vec<PathBuf>,
    base: FixtureBase,
) -> Result<(), Vec<String>> {
    fixtures.sort();
    let mut inputs = Vec::with_capacity(fixtures.len());
    for fixture in &fixtures {
        let source = std::fs::read_to_string(fixture)
            .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", fixture.display()));
        inputs.push(FileInput {
            name: fixture.display().to_string(),
            source,
        });
    }
    let reports = match base {
        FixtureBase::Prelude => raw_project_reports(inputs),
        FixtureBase::Library => match check_project(inputs) {
            Ok(reports) => reports,
            Err(error) => {
                return Err(vec![library_base_failure(project, &error.to_string())]);
            }
        },
    };
    let mut failures = Vec::new();
    for (fixture, report) in fixtures.iter().zip(&reports) {
        if let Err(file_failures) = compare_fixture_output(fixture, &report.source, &report.output)
        {
            failures.extend(file_failures);
        }
    }
    if reports.len() != fixtures.len() {
        failures.push(format!(
            "{}: project report count mismatch, expected {}, got {}",
            display_path(project),
            fixtures.len(),
            reports.len()
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn raw_project_reports(inputs: Vec<FileInput>) -> Vec<typokat::driver::FileReport> {
    check_project_without_library(inputs)
        .into_iter()
        .map(|report| typokat::driver::FileReport {
            name: report.name,
            source: report.source,
            output: raw_output(report.output),
        })
        .collect()
}

fn raw_output(output: typokat::check::test_support::CheckOutput) -> CheckOutput {
    CheckOutput {
        diagnostics: output.diagnostics,
        parse_errors: output.parse_errors,
        incomplete: output.incomplete,
    }
}

fn compare_fixture_output(
    path: &Path,
    source: &str,
    output: &typokat::driver::CheckOutput,
) -> Result<(), Vec<String>> {
    let expected = parse_markers(&display_path(path), source);
    let line_index = LineIndex::new(source);

    // A parse error in an M0 fixture is always a harness/spec problem — surface
    // it loudly rather than masking it as "0 diagnostics".
    if !output.parse_errors.is_empty() {
        return Err(vec![format!(
            "{}: unexpected parse error(s): {}",
            display_path(path),
            output.parse_errors.join("; ")
        )]);
    }

    // line -> list of (code, rendered_text)
    let mut actual: BTreeMap<u32, Vec<(String, String)>> = BTreeMap::new();
    for diag in &output.diagnostics {
        let line = line_index.line_of(diag.span.start);
        actual
            .entry(line)
            .or_default()
            .push((diag.code.as_str().to_string(), diag.rendered_text()));
    }

    let mut failures = Vec::new();
    let all_lines: std::collections::BTreeSet<u32> = expected
        .keys()
        .copied()
        .chain(actual.keys().copied())
        .collect();

    for line in all_lines {
        let empty_exp = Vec::new();
        let empty_act = Vec::new();
        let exp = expected.get(&line).unwrap_or(&empty_exp);
        let act = actual.get(&line).unwrap_or(&empty_act);

        // 1. Multiset of codes must match exactly.
        let exp_codes = sorted_codes(exp.iter().map(|m| m.code.as_str()));
        let act_codes = sorted_codes(act.iter().map(|(c, _)| c.as_str()));
        if exp_codes != act_codes {
            failures.push(format!(
                "{}:{}: code mismatch\n    expected: {:?}\n    actual:   {:?}",
                display_path(path),
                line,
                exp_codes,
                act_codes,
            ));
            // Skip substring checks on this line when codes already disagree.
            continue;
        }

        // 2. Each expected substring must appear in SOME actual diagnostic on the
        //    line with the same code. Match greedily so duplicate codes on a line
        //    pair up correctly.
        let mut remaining: Vec<&(String, String)> = act.iter().collect();
        for marker in exp {
            let Some(substr) = &marker.substring else {
                continue;
            };
            let pos = remaining
                .iter()
                .position(|(code, text)| *code == marker.code && text.contains(substr));
            match pos {
                Some(i) => {
                    remaining.remove(i);
                }
                None => {
                    let rendered: Vec<&str> = act
                        .iter()
                        .filter(|(c, _)| *c == marker.code)
                        .map(|(_, t)| t.as_str())
                        .collect();
                    failures.push(format!(
                        "{}:{}: no [{}] diagnostic contains substring {:?}\n    rendered: {:?}",
                        display_path(path),
                        line,
                        marker.code,
                        substr,
                        rendered,
                    ));
                }
            }
        }
    }

    // 3. Incomplete-surface markers — the third outcome. Same per-line, exact-identity
    //    diff discipline as error codes: an unexpected record or a missing expected one
    //    fails.
    if let Err(incomplete_failures) = compare_incomplete_output(path, source, output, &line_index) {
        failures.extend(incomplete_failures);
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// Diff the `// incomplete[<id>]` markers against the checker's incomplete channel,
/// per source line, with the same exact-multiset + optional-substring rules as the
/// error markers.
fn compare_incomplete_output(
    path: &Path,
    source: &str,
    output: &typokat::driver::CheckOutput,
    line_index: &LineIndex,
) -> Result<(), Vec<String>> {
    let expected = parse_incomplete_markers(&display_path(path), source);

    // line -> list of (id, rendered_text)
    let mut actual: BTreeMap<u32, Vec<(String, String)>> = BTreeMap::new();
    for rec in &output.incomplete {
        let line = line_index.line_of(rec.span.start);
        actual
            .entry(line)
            .or_default()
            .push((rec.id.clone(), rec.rendered_text()));
    }

    let mut failures = Vec::new();
    let all_lines: std::collections::BTreeSet<u32> = expected
        .keys()
        .copied()
        .chain(actual.keys().copied())
        .collect();

    for line in all_lines {
        let empty_exp = Vec::new();
        let empty_act = Vec::new();
        let exp = expected.get(&line).unwrap_or(&empty_exp);
        let act = actual.get(&line).unwrap_or(&empty_act);

        // 1. Multiset of stable ids must match exactly.
        let exp_ids = sorted_codes(exp.iter().map(|m| m.id.as_str()));
        let act_ids = sorted_codes(act.iter().map(|(id, _)| id.as_str()));
        if exp_ids != act_ids {
            failures.push(format!(
                "{}:{}: incomplete-identity mismatch\n    expected: {:?}\n    actual:   {:?}",
                display_path(path),
                line,
                exp_ids,
                act_ids,
            ));
            continue;
        }

        // 2. Each expected substring must appear in SOME actual record on the line with
        //    the same id (greedy pairing for duplicate ids).
        let mut remaining: Vec<&(String, String)> = act.iter().collect();
        for marker in exp {
            let Some(substr) = &marker.substring else {
                continue;
            };
            let pos = remaining
                .iter()
                .position(|(id, text)| *id == marker.id && text.contains(substr));
            match pos {
                Some(i) => {
                    remaining.remove(i);
                }
                None => {
                    let rendered: Vec<&str> = act
                        .iter()
                        .filter(|(id, _)| *id == marker.id)
                        .map(|(_, t)| t.as_str())
                        .collect();
                    failures.push(format!(
                        "{}:{}: no incomplete[{}] record contains substring {:?}\n    rendered: {:?}",
                        display_path(path),
                        line,
                        marker.id,
                        substr,
                        rendered,
                    ));
                }
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// Parse inline markers from a source file.
///
/// A marker lives in a trailing line-comment on the line where the diagnostic's
/// primary span starts. The recognized form is `error[TK<digits>]` optionally
/// followed by `: <substring>`. Multiple markers on one line are separated by
/// ` | `. Prose comments (not matching the `error[TK...]` pattern) are ignored.
///
/// **Malformed markers fail loudly.** Any ` | `-separated comment segment that is
/// *intended* as a marker — i.e. starts with `error[` — but does not parse as a
/// well-formed `error[TK<digits>]` (a missing `]`, a non-`TK`/non-digit code, …)
/// panics with the file, line, and segment. A silently dropped marker would turn a
/// typo like `error[TK2322` or `error[TK12x]` into "no expectation" and hide a real
/// diagnostic — the sharpest failure mode this corpus exists to prevent. Prose
/// segments that merely contain `error[` mid-text are still ignored.
fn parse_markers(display: &str, source: &str) -> BTreeMap<u32, Vec<ExpectedMarker>> {
    let mut map: BTreeMap<u32, Vec<ExpectedMarker>> = BTreeMap::new();
    for (idx, line) in source.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        // Only the comment tail can carry markers. Find the first `//`.
        let Some(comment_at) = line.find("//") else {
            continue;
        };
        let comment = &line[comment_at + 2..];
        // Quick reject: a comment without `error[` carries no markers.
        if !comment.contains("error[") {
            continue;
        }
        for segment in comment.split(" | ") {
            let trimmed = segment.trim();
            // A segment is only *intended* as a marker if it starts with `error[`;
            // otherwise it is prose (even if `error[` appears mid-text).
            if !trimmed.starts_with("error[") {
                continue;
            }
            match parse_one_marker(trimmed) {
                Some(marker) => {
                    // Reject a second `error[` in the segment's residual (after the
                    // code's closing `]`): a space-joined `error[..] error[..]` would
                    // otherwise absorb the trailing marker as inert text. Markers on
                    // one line must be ` | `-separated.
                    let close = trimmed.find(']').expect("parsed marker has `]`");
                    if trimmed[close + 1..].contains("error[") {
                        panic!(
                            "{display}:{line_no}: comment segment {segment:?} contains \
                             a second `error[` after a parsed marker — separate \
                             multiple markers with ` | `, or the trailing marker is \
                             silently dropped."
                        );
                    }
                    map.entry(line_no).or_default().push(marker)
                }
                None => panic!(
                    "{display}:{line_no}: malformed conformance marker in comment \
                     segment {segment:?} — expected `error[TK<digits>]` optionally \
                     followed by `: <substring>`. A dropped marker would silently \
                     hide a real diagnostic; fix or remove the segment."
                ),
            }
        }
    }
    map
}

/// Parse a single `error[CODE]` / `error[CODE]: substring` marker from a comment
/// segment, or `None` if the segment is not a marker.
fn parse_one_marker(segment: &str) -> Option<ExpectedMarker> {
    let segment = segment.trim();
    let rest = segment.strip_prefix("error[")?;
    let close = rest.find(']')?;
    let code = &rest[..close];
    // Validate the code shape: `TK` followed by digits. Ignores prose that
    // happens to contain "error[" but isn't a real marker.
    if !is_valid_code(code) {
        return None;
    }
    let after = rest[close + 1..].trim_start();
    let substring = after
        .strip_prefix(':')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(ExpectedMarker {
        code: code.to_string(),
        substring,
    })
}

/// A valid code is `TK` followed by one or more ASCII digits.
fn is_valid_code(code: &str) -> bool {
    match code.strip_prefix("TK") {
        Some(digits) => !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

/// Parse inline `// incomplete[<stable-id>]` / `: substring` markers, mirroring
/// [`parse_markers`]. A comment segment that *starts with* `incomplete[` but does not
/// parse as a well-formed marker panics with file/line/segment — a silently dropped
/// incomplete marker would hide a false-clean the corpus exists to catch. Prose that
/// merely mentions `incomplete[` mid-text is ignored. Multiple markers on one line are
/// ` | `-separated; a second `incomplete[` in a segment's residual is rejected.
fn parse_incomplete_markers(display: &str, source: &str) -> BTreeMap<u32, Vec<ExpectedIncomplete>> {
    let mut map: BTreeMap<u32, Vec<ExpectedIncomplete>> = BTreeMap::new();
    for (idx, line) in source.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        let Some(comment_at) = line.find("//") else {
            continue;
        };
        let comment = &line[comment_at + 2..];
        if !comment.contains("incomplete[") {
            continue;
        }
        for segment in comment.split(" | ") {
            let trimmed = segment.trim();
            if !trimmed.starts_with("incomplete[") {
                continue;
            }
            match parse_one_incomplete(trimmed) {
                Some(marker) => {
                    let close = trimmed.find(']').expect("parsed marker has `]`");
                    if trimmed[close + 1..].contains("incomplete[") {
                        panic!(
                            "{display}:{line_no}: comment segment {segment:?} contains a second \
                             `incomplete[` after a parsed marker — separate multiple markers with \
                             ` | `, or the trailing marker is silently dropped."
                        );
                    }
                    map.entry(line_no).or_default().push(marker)
                }
                None => panic!(
                    "{display}:{line_no}: malformed incomplete marker in comment segment \
                     {segment:?} — expected `incomplete[<stable-id>]` optionally followed by \
                     `: <substring>`. A dropped marker would silently hide a false-clean; fix or \
                     remove the segment."
                ),
            }
        }
    }
    map
}

/// Parse a single `incomplete[<id>]` / `incomplete[<id>]: substring` marker, or `None`
/// if the segment is not a well-formed marker (empty id / missing `]`).
fn parse_one_incomplete(segment: &str) -> Option<ExpectedIncomplete> {
    let segment = segment.trim();
    let rest = segment.strip_prefix("incomplete[")?;
    let close = rest.find(']')?;
    let id = &rest[..close];
    if !is_valid_incomplete_id(id) {
        return None;
    }
    let after = rest[close + 1..].trim_start();
    let substring = after
        .strip_prefix(':')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(ExpectedIncomplete {
        id: id.to_string(),
        substring,
    })
}

/// A valid stable id is a non-empty `role/surface/slot`-shaped token: lowercase
/// letters, digits, `/`, and `-` only (no whitespace, no `]`).
fn is_valid_incomplete_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'/' || b == b'-')
}

fn sorted_codes<'a>(codes: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut v: Vec<String> = codes.map(str::to_string).collect();
    v.sort();
    v
}

/// All `.ts` files directly under `dir` (non-recursive; the corpus is flat per
/// milestone).
fn discover_ts_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("ts") {
            out.push(path);
        }
    }
    out
}

fn discover_project_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out
}

fn discover_ts_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    discover_ts_files_recursive_into(dir, &mut out);
    out.sort();
    out
}

fn discover_ts_files_recursive_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            discover_ts_files_recursive_into(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ts") {
            out.push(path);
        }
    }
}

/// The `tests/cases` directory, relative to the crate manifest.
fn cases_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("cases")
}

/// A short, stable path for failure messages (relative to `tests/cases`).
fn display_path(path: &Path) -> String {
    let root = cases_root();
    path.strip_prefix(&root)
        .unwrap_or(path)
        .display()
        .to_string()
}

// --- Marker-parser witnesses (sprint WU5, finding 4) -------------------------

/// Run `parse_markers` catching any panic, with panic output suppressed so a
/// deliberately-malformed witness doesn't spam the test log.
#[cfg(test)]
fn parse_markers_catch(src: &str) -> Result<BTreeMap<u32, Vec<ExpectedMarker>>, ()> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| parse_markers("witness.ts", src)).map_err(|_| ());
    std::panic::set_hook(prev);
    result
}

/// Well-formed markers parse: bare code, code + substring, multiple ` | `-joined
/// markers, and prose that merely mentions `error[` mid-text is ignored.
#[test]
fn parse_markers_accepts_well_formed_and_ignores_prose() {
    let src = concat!(
        "const a = 1; // error[TK2322]\n",
        "const b = 2; // error[TK2345]: Argument of type\n",
        "const c = 3; // error[TK2341]: Property 'priv' is private | error[TK2445]: is protected\n",
        "const d = 4; // this comment references error[ish] prose, not a marker\n",
        "const e = 5; // plain prose comment\n",
    );
    let map = parse_markers_catch(src).expect("well-formed corpus must parse");
    assert_eq!(map[&1].len(), 1);
    assert_eq!(map[&1][0].code, "TK2322");
    assert_eq!(map[&2][0].substring.as_deref(), Some("Argument of type"));
    assert_eq!(map[&3].len(), 2, "both ` | `-joined markers parse");
    assert_eq!(map[&3][1].code, "TK2445");
    assert!(
        !map.contains_key(&4),
        "prose containing `error[` is ignored"
    );
    assert!(!map.contains_key(&5));
}

/// A marker missing its closing `]` must fail loudly, not be dropped.
#[test]
fn parse_markers_rejects_unterminated_code() {
    assert!(
        parse_markers_catch("const x = 1; // error[TK2322\n").is_err(),
        "an unterminated `error[TK2322` marker must panic, not silently drop"
    );
}

/// A code with a non-digit tail (`TK12x`) is malformed and must fail.
#[test]
fn parse_markers_rejects_non_digit_code() {
    assert!(
        parse_markers_catch("const x = 1; // error[TK12x]\n").is_err(),
        "`error[TK12x]` is not a valid code and must panic"
    );
}

/// A code missing the `TK` prefix (`error[2322]`) is malformed and must fail.
#[test]
fn parse_markers_rejects_missing_tk_prefix() {
    assert!(
        parse_markers_catch("const x = 1; // error[2322]\n").is_err(),
        "`error[2322]` (no `TK`) must panic"
    );
}

/// Two markers joined by a plain space (not ` | `) must fail loudly: the trailing
/// `error[TK2345]` would otherwise be absorbed as inert residual text and dropped.
#[test]
fn parse_markers_rejects_space_joined_double_marker() {
    assert!(
        parse_markers_catch("const x = 1; // error[TK2322] error[TK2345]\n").is_err(),
        "space-joined double markers must panic — use ` | ` to separate markers"
    );
}

/// Control: a substring assertion containing the literal word "error" (without
/// `[`) is legitimate text and still parses.
#[test]
fn parse_markers_accepts_substring_containing_word_error() {
    let map =
        parse_markers_catch("const x = 1; // error[TK2322]: reports an error for this line\n")
            .expect("a substring containing the word `error` must parse");
    assert_eq!(map[&1][0].code, "TK2322");
    assert_eq!(
        map[&1][0].substring.as_deref(),
        Some("reports an error for this line")
    );
}

/// A malformed marker after a ` | ` separator must fail even when the first
/// segment is well-formed — a dropped second marker would hide a diagnostic.
#[test]
fn parse_markers_rejects_malformed_second_segment() {
    assert!(
        parse_markers_catch("const x = 1; // error[TK2322] | error[TK99\n").is_err(),
        "a malformed segment after ` | ` must panic"
    );
}

/// The entire committed corpus (every `.ts` fixture, enabled or not) must parse
/// under the strict rules — proving the hardening rewrites no real markers.
#[test]
fn parse_markers_accepts_entire_corpus() {
    let mut files = Vec::new();
    collect_all_ts(&cases_root(), &mut files);
    assert!(!files.is_empty(), "expected fixtures under tests/cases");
    for path in files {
        let source = std::fs::read_to_string(&path).unwrap();
        parse_markers_catch(&source).unwrap_or_else(|_| {
            panic!(
                "strict marker parse rejected existing fixture {}",
                path.display()
            )
        });
    }
}

// --- Incomplete-marker parser witnesses (sprint WU3) -------------------------

#[cfg(test)]
fn parse_incomplete_catch(src: &str) -> Result<BTreeMap<u32, Vec<ExpectedIncomplete>>, ()> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result =
        std::panic::catch_unwind(|| parse_incomplete_markers("witness.ts", src)).map_err(|_| ());
    std::panic::set_hook(prev);
    result
}

/// Well-formed incomplete markers parse: bare id, id + substring, ` | `-joined pair;
/// prose mentioning `incomplete[` mid-text is ignored.
#[test]
fn parse_incomplete_accepts_well_formed_and_ignores_prose() {
    let src = concat!(
        "const a = 1; // incomplete[expr-infer/template-literal/interpolation]\n",
        "const b = 2; // incomplete[expr-infer/object-literal/computed-key]: computed\n",
        "const c = 3; // incomplete[expr-infer/array-literal/spread-element] | incomplete[call/call-arguments/spread-argument]\n",
        "const d = 4; // mentions incomplete[ish] prose, not a marker\n",
        "const e = 5; // plain prose\n",
    );
    let map = parse_incomplete_catch(src).expect("well-formed incomplete corpus must parse");
    assert_eq!(map[&1][0].id, "expr-infer/template-literal/interpolation");
    assert_eq!(map[&2][0].substring.as_deref(), Some("computed"));
    assert_eq!(map[&3].len(), 2, "both ` | `-joined markers parse");
    assert_eq!(map[&3][1].id, "call/call-arguments/spread-argument");
    assert!(
        !map.contains_key(&4),
        "prose mentioning `incomplete[` ignored"
    );
    assert!(!map.contains_key(&5));
}

/// A marker missing its `]` must fail loudly rather than be silently dropped.
#[test]
fn parse_incomplete_rejects_unterminated_id() {
    assert!(
        parse_incomplete_catch("const x = 1; // incomplete[expr-infer/x\n").is_err(),
        "an unterminated incomplete marker must panic, not silently drop"
    );
}

/// An id with an illegal character (uppercase / space) is malformed and must fail.
#[test]
fn parse_incomplete_rejects_illegal_id() {
    assert!(
        parse_incomplete_catch("const x = 1; // incomplete[Expr Infer]\n").is_err(),
        "an id outside [a-z0-9/-] must panic"
    );
}

/// Two incomplete markers joined by a plain space (not ` | `) must fail loudly.
#[test]
fn parse_incomplete_rejects_space_joined_double_marker() {
    assert!(
        parse_incomplete_catch("const x = 1; // incomplete[a/b/c] incomplete[d/e/f]\n").is_err(),
        "space-joined double incomplete markers must panic — use ` | `"
    );
}

/// The entire committed corpus parses under the strict incomplete-marker rules.
#[test]
fn parse_incomplete_accepts_entire_corpus() {
    let mut files = Vec::new();
    collect_all_ts(&cases_root(), &mut files);
    assert!(!files.is_empty(), "expected fixtures under tests/cases");
    for path in files {
        let source = std::fs::read_to_string(&path).unwrap();
        parse_incomplete_catch(&source).unwrap_or_else(|_| {
            panic!(
                "strict incomplete-marker parse rejected existing fixture {}",
                path.display()
            )
        });
    }
}

#[cfg(test)]
fn collect_all_ts(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read cases dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_all_ts(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ts") {
            out.push(path);
        }
    }
}

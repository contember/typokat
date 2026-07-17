use oxc_allocator::Allocator;
use oxc_ast::ast::TSModuleDeclarationName;
use oxc_ast::AstKind;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

const READINESS_PATH: &str = "tests/fixtures/lib-es2025-full-6.0.3/readiness.toml";
const LEDGER_PATH: &str = "tests/fixtures/lib-es2025-full-6.0.3/ledger.toml";
const BRIDGES_PATH: &str = "tests/fixtures/lib-es2025-full-6.0.3/bridges.toml";
const ROUTING_PATH: &str = "tests/fixtures/lib-es2025-full-6.0.3/routing.toml";
const CONFORMANCE_MANIFEST_PATH: &str =
    "tests/fixtures/lib-es2025-full-6.0.3/conformance-invocations.toml";
const PROFILE_PATH: &str = "src/library/typescript-6.0.3/profile.toml";
const PROFILE_LIB_DIR: &str = "src/library/typescript-6.0.3/lib";
const OFFICIAL_SCOREBOARD_PATH: &str = "tooling/official-suite/scoreboard.txt";
const OFFICIAL_CORPUS_DIR: &str = "tooling/official-suite/corpus";
const RAW_ROUTING_DIR: &str = "tests/fixtures/lib-es2025-full-6.0.3/raw-routing";
const MAX_RAW_ROUTING_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;
const PROFILE_SHA256: &str = "1edef1b5e870024834762267ec532c3054f3b2279e9181844e21648243eb1407";
const PROFILE_REGISTRY_SHA256: &str =
    "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const TYPESCRIPT_VERSION: &str = "6.0.3";
const UPSTREAM_REVISION: &str = "050880ce59e30b356b686bd3144efe24f875ebc8";
const PROTOTYPE_ENTRY: &str = "crate::check::checker::wu0b_library::run_injected_profile";
const RELEASE_PROBE_FILTER: &str = "check::checker::wu0b_library::tests::wu0b_release_probe_once";
const RELEASE_BUILD_COMMAND: &str = "cargo test --release --lib --no-run --message-format=json";

// Deliberately unapproved until a separate post-measurement leader review commit.
const APPROVED_PRIVATE_FALLBACK_WALL_MS: Option<usize> = None;
const APPROVED_ROUTING_PRIVATE_FRACTION_PPM: Option<usize> = None;
const APPROVED_ROUTING_PROJECTED_SUITE_MS: Option<usize> = None;
const APPROVED_ROUTING_OBSERVED_SUITE_MS: Option<usize> = None;
const APPROVED_BENCHMARK_SAMPLE_COUNT: Option<usize> = None;
const APPROVED_FANOUT_WORKERS: Option<usize> = None;
const APPROVED_BRIDGE_MATRIX_SHA256: Option<&str> = None;
const APPROVED_CONFORMANCE_INVOCATION_MANIFEST_SHA256: Option<&str> = None;
const APPROVED_OFFICIAL_SOURCE_MANIFEST_SHA256: Option<&str> = None;
const APPROVED_ROUTING_EVIDENCE_MANIFEST_SHA256: Option<&str> = None;

const BRIDGE_IDS: &[&str] = &[
    "mutable-array-tuple",
    "readonly-array-tuple",
    "primitive-wrapper-members",
    "object-apparent-members",
    "callable-function-members",
    "regexp-literal",
    "global-this",
    "string-intrinsics",
    "this-type-omit-this-parameter",
];
const ROUTING_CORPORA: &[&str] = &["committed-conformance", "official-suite"];
const ROUTE_CATEGORIES: &[&str] = &[
    "collision",
    "unique-global-object",
    "declare-global",
    "namespace-umd",
    "classifier-uncertain",
];

const READINESS_KEYS: &[&str] = &[
    "schema",
    "status",
    "target",
    "typescript_version",
    "upstream_revision",
    "checker_revision",
    "profile_path",
    "profile_sha256",
    "profile_registry_sha256",
    "prototype_entry",
    "benchmark_host",
    "reason",
    "conclusion",
    "phase_sample",
    "cold_sample",
    "artifacts",
    "protocol",
    "warm",
    "private",
    "freeze",
    "verdict",
];
const ARTIFACT_KEYS: &[&str] = &[
    "ledger_path",
    "ledger_sha256",
    "bridges_path",
    "bridges_sha256",
    "routing_path",
    "routing_sha256",
    "conformance_manifest_path",
    "conformance_manifest_sha256",
];
const PROTOCOL_KEYS: &[&str] = &[
    "release_build_command",
    "release_probe_filter",
    "memory_tool",
    "cold_processes",
    "ordinary_warm_filesystem_cache",
    "release_profile",
    "tiny_source_sha256",
];
const PHASE_KEYS: &[&str] = &[
    "process",
    "registry_validation_us",
    "parse_us",
    "bind_us",
    "reserve_fill_us",
    "publication_validation_us",
    "statement_check_us",
    "total_us",
];
const COLD_KEYS: &[&str] = &[
    "process",
    "wall_ms",
    "max_rss_mib",
    "exit_code",
    "diagnostics",
    "incompletes",
];
const WARM_KEYS: &[&str] = &[
    "status",
    "sample_count",
    "delta_baseline_p50_us",
    "delta_baseline_p95_us",
    "shared_base_p50_us",
    "shared_base_p95_us",
    "max_rss_mib",
    "library_parse_count",
    "library_bind_count",
    "library_check_count",
    "base_sized_clone_count",
    "per_check_allocation_independent",
    "delta_baseline_samples_us",
    "shared_base_samples_us",
    "pointer_identity",
];
const POINTER_IDENTITY_KEYS: &[&str] = &["checks", "addresses", "base_digest"];
const PRIVATE_KEYS: &[&str] = &[
    "status",
    "fallback_wall_ms",
    "fallback_max_rss_mib",
    "shared_base_retained",
    "fanout_workers",
    "fanout_single_permit",
    "fanout_max_rss_mib",
    "fanout_linear_wall_ms",
    "fanout_observed_wall_ms",
];
const FREEZE_KEYS: &[&str] = &[
    "status",
    "ast_free",
    "send_sync_static",
    "all_reserved_terminal",
    "no_allocator",
    "no_construction_drafts",
    "no_pass_local_cache",
    "root_index_complete",
    "universe_local_identities",
];
const VERDICT_KEYS: &[&str] = &["claimed", "rationale"];

const LEDGER_KEYS: &[&str] = &[
    "schema",
    "status",
    "profile_path",
    "profile_sha256",
    "profile_registry_sha256",
    "collector_revision",
    "source_census_sha256",
    "file_count",
    "declaration_count",
    "outcome_count",
    "unavailable_count",
    "diagnostic_count",
    "incomplete_count",
    "parser_error_count",
    "library_event_count",
    "file",
    "declaration",
    "outcome",
];
const LEDGER_FILE_KEYS: &[&str] = &[
    "ordinal",
    "name",
    "sha256",
    "source_kind",
    "parse_errors",
    "declarations",
    "outcomes",
];
const DECLARATION_KEYS: &[&str] = &[
    "file_ordinal",
    "declaration_start",
    "declaration_end",
    "binding_start",
    "binding_end",
    "kind",
    "owner",
    "availability",
];
const OUTCOME_KEYS: &[&str] = &[
    "file_ordinal",
    "source_start",
    "event_ordinal",
    "record_ordinal",
    "kind",
    "identity",
    "owner",
];

const BRIDGES_KEYS: &[&str] = &[
    "schema",
    "status",
    "profile_sha256",
    "collector_revision",
    "candidate_count",
    "consumer_count",
    "unexplained_consumer_count",
    "matrix_sha256",
    "candidate",
];
const BRIDGE_ROW_KEYS: &[&str] = &[
    "id",
    "status",
    "decision",
    "identity_role",
    "declaration_identity",
    "declaration_file",
    "declaration_start",
    "declaration_end",
    "consumers",
    "good_witness",
    "bad_witness",
    "shadowing_witness",
    "missing_member_witness",
    "shared_universe_witness",
    "private_universe_witness",
    "oracle_good",
    "oracle_bad",
    "typokat_current",
    "owner",
    "rationale",
    "falsifier",
];
const WITNESS_KEYS: &[&str] = &["path", "sha256"];

const ROUTING_KEYS: &[&str] = &[
    "schema",
    "status",
    "profile_sha256",
    "collector_revision",
    "classifier",
    "candidate_collector",
    "summary",
    "approval",
    "corpus",
    "route",
];
const ROUTING_SUMMARY_KEYS: &[&str] = &[
    "invocation_count",
    "source_count",
    "private_count",
    "expected_rebuild_count",
    "private_fraction_ppm",
    "projected_suite_ms",
    "observed_suite_ms",
];
const APPROVAL_KEYS: &[&str] = &["decision", "approver", "date", "evidence_manifest_sha256"];
const CORPUS_KEYS: &[&str] = &[
    "id",
    "status",
    "input_manifest_sha256",
    "invocation_count",
    "source_count",
    "candidate_count",
    "private_count",
    "expected_rebuild_count",
    "collision_count",
    "unique_global_object_count",
    "declare_global_count",
    "namespace_umd_count",
    "classifier_uncertain_count",
    "projected_suite_ms",
    "observed_suite_ms",
];
const ROUTE_KEYS: &[&str] = &[
    "corpus",
    "invocation",
    "input_sha256",
    "source_count",
    "collected_roots",
    "candidate_count",
    "categories",
    "expected_route",
    "observed_route",
    "projected_ms",
    "observed_ms",
    "observed_output",
    "forced_private_output",
];
const CONFORMANCE_MANIFEST_KEYS: &[&str] = &[
    "schema",
    "status",
    "source_path",
    "source_sha256",
    "invocation_count",
    "source_count",
    "manifest_sha256",
    "invocation",
];
const CONFORMANCE_INVOCATION_KEYS: &[&str] = &["id", "kind", "source"];
const RAW_ROUTE_ARTIFACT_KEYS: &[&str] = &[
    "schema",
    "checker_revision",
    "corpus",
    "invocation",
    "mode",
    "input_sha256",
    "source_count",
    "collected_roots",
    "categories",
    "candidate_count",
    "route",
    "elapsed_ms",
    "semantic_output_sha256",
];

#[derive(Clone)]
struct Document {
    bytes: Vec<u8>,
    value: Value,
}

#[derive(Clone)]
struct Bundle {
    readiness: Document,
    ledger: Document,
    bridges: Document,
    routing: Document,
    conformance_manifest: Document,
    profile: Document,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SourceOccurrence {
    file_ordinal: usize,
    declaration_start: usize,
    declaration_end: usize,
    binding_start: usize,
    binding_end: usize,
    kind: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InvocationInput {
    source_count: usize,
    input_sha256: String,
}

#[derive(Clone)]
struct RouteOutputArtifact {
    path: String,
    file_sha256: String,
    mode: String,
    source_count: usize,
    collected_roots: Vec<String>,
    categories: Vec<String>,
    candidate_count: usize,
    route: String,
    elapsed_ms: usize,
    semantic_output_sha256: String,
}

#[derive(Copy, Clone)]
struct RouteArtifactContext<'a> {
    checker_revision: &'a str,
    corpus: &'a str,
    invocation: &'a str,
    mode: &'a str,
    input_sha256: &'a str,
}

#[derive(Clone)]
struct RoutingEvidenceRecord {
    corpus: String,
    invocation: String,
    input_sha256: String,
    source_count: usize,
    collected_roots: Vec<String>,
    categories: Vec<String>,
    candidate_count: usize,
    expected_route: String,
    observed_route: String,
    projected_ms: usize,
    observed_ms: usize,
    observed: RouteOutputArtifact,
    forced: RouteOutputArtifact,
}

#[derive(Copy, Clone)]
struct RoutingEvidenceMetadata<'a> {
    schema: usize,
    status: &'a str,
    profile_sha256: &'a str,
    checker_revision: &'a str,
    classifier: &'a str,
    candidate_collector: &'a str,
    official_manifest_sha256: &'a str,
    conformance_manifest_sha256: &'a str,
    official_source_manifest_sha256: &'a str,
}

#[derive(Default)]
struct SourceCensusVisitor {
    occurrences: Vec<(usize, usize, usize, usize, &'static str)>,
}

impl SourceCensusVisitor {
    fn push(&mut self, kind: &'static str, declaration: oxc_span::Span, binding: oxc_span::Span) {
        self.occurrences.push((
            usize::try_from(declaration.start).expect("OXC span offset fits usize"),
            usize::try_from(declaration.end).expect("OXC span offset fits usize"),
            usize::try_from(binding.start).expect("OXC span offset fits usize"),
            usize::try_from(binding.end).expect("OXC span offset fits usize"),
            kind,
        ));
    }

    fn push_pattern(
        &mut self,
        kind: &'static str,
        declaration: oxc_span::Span,
        pattern: &oxc_ast::ast::BindingPattern<'_>,
    ) {
        for identifier in pattern.get_binding_identifiers() {
            self.push(kind, declaration, identifier.span);
        }
    }
}

impl<'a> Visit<'a> for SourceCensusVisitor {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        match kind {
            AstKind::VariableDeclarator(node) => self.push_pattern("variable", node.span, &node.id),
            AstKind::Function(node) => {
                if let Some(id) = &node.id {
                    self.push("function", node.span, id.span);
                }
            }
            AstKind::Class(node) => {
                if let Some(id) = &node.id {
                    self.push("class", node.span, id.span);
                }
            }
            AstKind::FormalParameter(node) => {
                self.push_pattern("parameter", node.span, &node.pattern)
            }
            AstKind::FormalParameterRest(node) => {
                self.push_pattern("parameter", node.span, &node.rest.argument)
            }
            AstKind::CatchClause(node) => {
                if let Some(parameter) = &node.param {
                    self.push_pattern("catch-parameter", parameter.span, &parameter.pattern);
                }
            }
            AstKind::ImportDeclaration(node) => {
                if let Some(specifiers) = &node.specifiers {
                    for specifier in specifiers {
                        self.push("import", node.span, specifier.local().span);
                    }
                }
            }
            AstKind::TSTypeAliasDeclaration(node) => {
                self.push("type-alias", node.span, node.id.span)
            }
            AstKind::TSInterfaceDeclaration(node) => {
                self.push("interface", node.span, node.id.span)
            }
            AstKind::TSEnumDeclaration(node) => self.push("enum", node.span, node.id.span),
            AstKind::TSModuleDeclaration(node) => {
                let binding = match &node.id {
                    TSModuleDeclarationName::Identifier(id) => id.span,
                    TSModuleDeclarationName::StringLiteral(literal) => literal.span,
                };
                self.push("namespace", node.span, binding);
            }
            AstKind::TSImportEqualsDeclaration(node) => {
                self.push("import-equals", node.span, node.id.span)
            }
            AstKind::TSNamespaceExportDeclaration(node) => {
                self.push("namespace-export", node.span, node.id.span)
            }
            AstKind::TSGlobalDeclaration(node) => self.push("global", node.span, node.global_span),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Pending,
    Go,
    NoGo,
}

#[derive(Clone)]
struct GateFacts {
    cold_samples: usize,
    cold_under_limits: bool,
    phases_complete: bool,
    warm_under_ratio: bool,
    warm_zero_library_work: bool,
    private_under_limits: bool,
    fanout_bounded: bool,
    frozen_ast_free_send_sync: bool,
    ledger_complete: bool,
    bridges_complete: bool,
    routing_complete_and_approved: bool,
    release_protocol: bool,
    code_approvals_complete: bool,
}

#[test]
#[ignore = "WU0B evidence is PENDING; run only after the reviewed release evidence commit"]
fn wu0b_completed_evidence_satisfies_adr_0011() -> Result<(), String> {
    match validate_bundle(&load_bundle()?)? {
        Verdict::Go => Ok(()),
        Verdict::Pending => Err("WU0B evidence is still PENDING; GO is forbidden".to_owned()),
        Verdict::NoGo => Err("WU0B hard gates recompute to NO-GO".to_owned()),
    }
}

#[test]
fn pending_manifests_are_well_formed_but_cannot_be_go() -> Result<(), String> {
    let bundle = load_bundle()?;
    if validate_bundle(&bundle)? != Verdict::Pending {
        return Err("the committed RED evidence must remain explicitly PENDING".to_owned());
    }
    Ok(())
}

#[test]
fn changing_only_the_claim_cannot_fabricate_go() -> Result<(), String> {
    let mut bundle = load_bundle()?;
    root_mut(&mut bundle.readiness)?.insert("conclusion".to_owned(), Value::String("GO".into()));
    child_table_mut(root_mut(&mut bundle.readiness)?, "verdict")?
        .insert("claimed".to_owned(), Value::String("GO".into()));
    if validate_bundle(&bundle).is_ok() {
        return Err("a GO label over PENDING evidence was accepted".to_owned());
    }
    Ok(())
}

#[test]
fn exact_schema_rejects_unknown_keys() {
    let mut table = Table::new();
    for key in OUTCOME_KEYS {
        table.insert((*key).to_owned(), Value::String(String::new()));
    }
    table.insert("module_ordinal".to_owned(), Value::Integer(0));
    assert!(exact_keys(&table, OUTCOME_KEYS, "outcome").is_err());
}

#[test]
fn library_event_schema_rejects_user_domain_keys() {
    let mut outcome = Table::new();
    outcome.insert("file_ordinal".into(), Value::Integer(0));
    outcome.insert("source_start".into(), Value::Integer(1));
    outcome.insert("event_ordinal".into(), Value::Integer(0));
    outcome.insert("record_ordinal".into(), Value::Integer(0));
    outcome.insert("kind".into(), Value::String("diagnostic".into()));
    outcome.insert("identity".into(), Value::String("TK2304".into()));
    outcome.insert(
        "owner".into(),
        Value::String("library:0:1:interface".into()),
    );
    assert!(validate_outcome_row(&outcome, 82).is_ok());
    outcome.insert("unit_slot".into(), Value::Integer(0));
    assert!(validate_outcome_row(&outcome, 82).is_err());
}

#[test]
fn evidence_hash_binding_rejects_byte_mutation() {
    let bytes = b"evidence\n";
    let digest = sha256_hex(bytes);
    assert!(verify_sha256(bytes, &digest, "fixture").is_ok());
    assert!(verify_sha256(b"evidence!\n", &digest, "fixture").is_err());
}

#[test]
fn hard_gate_recomputation_rejects_false_go_mutations() {
    let passing = GateFacts {
        cold_samples: 5,
        cold_under_limits: true,
        phases_complete: true,
        warm_under_ratio: true,
        warm_zero_library_work: true,
        private_under_limits: true,
        fanout_bounded: true,
        frozen_ast_free_send_sync: true,
        ledger_complete: true,
        bridges_complete: true,
        routing_complete_and_approved: true,
        release_protocol: true,
        code_approvals_complete: true,
    };
    assert_eq!(recompute_verdict(&passing), Verdict::Go);
    for mutate in [
        |facts: &mut GateFacts| facts.cold_under_limits = false,
        |facts: &mut GateFacts| facts.warm_zero_library_work = false,
        |facts: &mut GateFacts| facts.frozen_ast_free_send_sync = false,
        |facts: &mut GateFacts| facts.routing_complete_and_approved = false,
    ] {
        let mut facts = passing.clone();
        mutate(&mut facts);
        assert_eq!(recompute_verdict(&facts), Verdict::NoGo);
    }
}

#[test]
fn ledger_rejects_one_invented_or_missing_source_occurrence() -> Result<(), String> {
    let source = "interface Shape {} declare const value: number;";
    let expected = source_occurrences(0, source)?;
    assert_eq!(expected.len(), 2);
    let mut reported = expected.clone();
    let removed = reported.iter().next().cloned().expect("two occurrences");
    reported.remove(&removed);
    assert_ne!(reported, expected);
    reported.insert(SourceOccurrence {
        file_ordinal: 0,
        declaration_start: 0,
        declaration_end: 1,
        binding_start: 0,
        binding_end: 1,
        kind: "interface",
    });
    assert_ne!(reported, expected);
    Ok(())
}

#[test]
fn benchmark_samples_and_limits_are_recomputed_fail_closed() {
    let baseline = [100, 110, 120, 130, 140];
    let shared_at_boundary = [100, 110, 120, 130, 175];
    assert_eq!(percentiles(&baseline), Some((120, 140)));
    assert_eq!(percentiles(&shared_at_boundary), Some((120, 175)));
    assert!(within_125_percent(175, 140));
    assert!(!within_125_percent(176, 140));
    assert!(!within_125_percent(
        9_000_000_000_000_000_000,
        4_000_000_000_000_000_000,
    ));
    assert!(checked_sum_usize(&[usize::MAX, 1]).is_none());
    assert!(within_approved_limit(5_000, Some(5_000)));
    assert!(!within_approved_limit(5_001, Some(5_000)));
    assert!(!within_approved_limit(1, None));
    assert_eq!(percentiles(&[0, 1, 2]), None);
    assert!(cold_measurement_valid(5_000, 512, 0, 0, 0));
    assert!(!cold_measurement_valid(5_001, 512, 0, 0, 0));
    assert!(!cold_measurement_valid(5_000, 513, 0, 0, 0));
}

#[test]
fn fanout_derives_linear_wall_and_caps_rss_at_fallback() {
    assert!(fanout_measurement_valid(100, 512, 32, 512, 3_200, 4_000));
    assert!(!fanout_measurement_valid(100, 512, 32, 513, 3_200, 4_000,));
    assert!(!fanout_measurement_valid(100, 512, 32, 512, 3_199, 4_000,));
    assert!(!fanout_measurement_valid(100, 512, 32, 512, 3_200, 4_001,));
}

#[test]
fn pointer_identity_rejects_one_changed_middle_address() {
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let row = |checks: usize, addresses: Vec<&str>| {
        let mut table = Table::new();
        table.insert(
            "checks".into(),
            Value::Integer(i64::try_from(checks).expect("test check count fits i64")),
        );
        table.insert(
            "addresses".into(),
            Value::Array(
                addresses
                    .into_iter()
                    .map(|address| Value::String(address.into()))
                    .collect(),
            ),
        );
        table.insert("base_digest".into(), Value::String(digest.into()));
        Value::Table(table)
    };
    let mut rows = vec![
        row(1, vec!["0xabc"]),
        row(2, vec!["0xabc"; 2]),
        row(32, vec!["0xabc"; 32]),
    ];
    assert_eq!(validate_pointer_identity(&rows), Ok(true));
    rows[2]
        .as_table_mut()
        .expect("pointer row")
        .get_mut("addresses")
        .and_then(Value::as_array_mut)
        .expect("address array")[16] = Value::String("0xdef".into());
    assert!(validate_pointer_identity(&rows).is_err());
}

#[test]
fn routing_rejects_zero_time_or_mismatched_forced_private_output() {
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(route_measurement_valid(1, 1, digest, digest));
    assert!(!route_measurement_valid(0, 1, digest, digest));
    assert!(!route_measurement_valid(
        1,
        1,
        digest,
        "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    ));
    assert!(within_approved_limit(250_000, Some(250_000)));
    assert!(!within_approved_limit(250_001, Some(250_000)));
}

#[test]
fn routing_evidence_approval_covers_decision_and_timing_mutations() {
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let revision = "0123456789abcdef0123456789abcdef01234567";
    let metadata = RoutingEvidenceMetadata {
        schema: 1,
        status: "COMPLETE",
        profile_sha256: digest,
        checker_revision: revision,
        classifier: "binder::ModuleBindingContext::for_program",
        candidate_collector: "binder-shared-preflight-candidate-collector",
        official_manifest_sha256: digest,
        conformance_manifest_sha256: digest,
        official_source_manifest_sha256: digest,
    };
    let artifact = |mode: &str, path: &str, route: &str, elapsed_ms: usize| RouteOutputArtifact {
        path: path.to_owned(),
        file_sha256: digest.to_owned(),
        mode: mode.to_owned(),
        source_count: 1,
        collected_roots: vec!["root".to_owned()],
        categories: vec!["collision".to_owned()],
        candidate_count: 1,
        route: route.to_owned(),
        elapsed_ms,
        semantic_output_sha256: digest.to_owned(),
    };
    let valid = RoutingEvidenceRecord {
        corpus: "committed-conformance".to_owned(),
        invocation: "case".to_owned(),
        input_sha256: digest.to_owned(),
        source_count: 1,
        collected_roots: vec!["root".to_owned()],
        categories: vec!["collision".to_owned()],
        candidate_count: 1,
        expected_route: "private".to_owned(),
        observed_route: "private".to_owned(),
        projected_ms: 20,
        observed_ms: 10,
        observed: artifact("observed", "observed.toml", "private", 10),
        forced: artifact("forced-private", "forced.toml", "private", 20),
    };
    let approved = routing_evidence_manifest_sha256(&metadata, std::slice::from_ref(&valid));
    assert!(matrix_is_code_approved(&approved, Some(&approved)));
    for mutated in [
        {
            let mut row = valid.clone();
            row.categories.clear();
            row
        },
        {
            let mut row = valid.clone();
            row.collected_roots[0] = "rewritten-root".to_owned();
            row
        },
        {
            let mut row = valid.clone();
            row.expected_route = "fast".to_owned();
            row
        },
        {
            let mut row = valid.clone();
            row.observed_route = "fast".to_owned();
            row
        },
        {
            let mut row = valid.clone();
            row.projected_ms = 1;
            row
        },
        {
            let mut row = valid.clone();
            row.observed_ms = 1;
            row
        },
        {
            let mut row = valid.clone();
            row.observed.categories.clear();
            row
        },
        {
            let mut row = valid.clone();
            row.observed.route = "fast".to_owned();
            row
        },
        {
            let mut row = valid.clone();
            row.forced.elapsed_ms = 1;
            row
        },
    ] {
        let digest = routing_evidence_manifest_sha256(&metadata, &[mutated]);
        assert!(!matrix_is_code_approved(&digest, Some(&approved)));
    }

    let mut second = valid.clone();
    second.corpus = "official-suite".to_owned();
    second.invocation = "other".to_owned();
    second.observed.path = "other-observed.toml".to_owned();
    second.forced.path = "other-forced.toml".to_owned();
    assert_eq!(
        routing_evidence_manifest_sha256(&metadata, &[valid.clone(), second.clone()]),
        routing_evidence_manifest_sha256(&metadata, &[second, valid]),
    );

    let changed_metadata = RoutingEvidenceMetadata {
        classifier: "rewritten-classifier",
        ..metadata
    };
    assert_ne!(
        routing_evidence_manifest_sha256(&changed_metadata, &[]),
        routing_evidence_manifest_sha256(&metadata, &[]),
    );
}

#[test]
fn raw_route_artifact_parser_rejects_one_context_mutation() -> Result<(), String> {
    let revision = "0123456789abcdef0123456789abcdef01234567";
    let input = "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let semantic = "2123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let valid = format!(
        "schema = 1\nchecker_revision = \"{revision}\"\ncorpus = \"official-suite\"\ninvocation = \"case.ts\"\nmode = \"observed\"\ninput_sha256 = \"{input}\"\nsource_count = 1\ncollected_roots = [\"root\"]\ncategories = [\"collision\"]\ncandidate_count = 1\nroute = \"private\"\nelapsed_ms = 10\nsemantic_output_sha256 = \"{semantic}\"\n"
    );
    let observed_context = RouteArtifactContext {
        checker_revision: revision,
        corpus: "official-suite",
        invocation: "case.ts",
        mode: "observed",
        input_sha256: input,
    };
    assert_eq!(
        parse_route_output_artifact(valid.as_bytes(), "fake.toml", semantic, &observed_context,)?
            .semantic_output_sha256,
        semantic,
    );
    assert!(parse_route_output_artifact(
        valid.as_bytes(),
        "fake.toml",
        semantic,
        &RouteArtifactContext {
            mode: "forced-private",
            ..observed_context
        },
    )
    .is_err());
    for mutation in [
        valid.replace(
            "collected_roots = [\"root\"]",
            "collected_roots = [\"root\", \"root\"]",
        ),
        valid.replace(
            "categories = [\"collision\"]",
            "categories = [\"unique-global-object\", \"collision\"]",
        ),
        valid.replace("candidate_count = 1", "candidate_count = 0"),
        valid.replace("elapsed_ms = 10", "elapsed_ms = 0"),
    ] {
        assert!(parse_route_output_artifact(
            mutation.as_bytes(),
            "fake.toml",
            semantic,
            &observed_context,
        )
        .is_err());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn raw_route_reader_is_descriptor_relative_and_no_follow() -> Result<(), String> {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!(
        "typokat-wu0b-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let raw = root.join("raw-routing");
    fs::create_dir_all(&raw).map_err(|error| error.to_string())?;
    fs::write(raw.join("real.toml"), b"schema = 1\n").map_err(|error| error.to_string())?;
    let evidence = NoFollowEvidenceDir::open_beneath(&root, Path::new("raw-routing"))?;
    assert_eq!(evidence.read_toml(Path::new("real.toml"))?, b"schema = 1\n");
    assert!(evidence.read_toml(Path::new("nested/real.toml")).is_err());
    assert!(evidence.read_toml(Path::new("../real.toml")).is_err());
    symlink("real.toml", raw.join("link.toml")).map_err(|error| error.to_string())?;
    assert!(evidence.read_toml(Path::new("link.toml")).is_err());

    fs::rename(&raw, root.join("retained-raw")).map_err(|error| error.to_string())?;
    fs::create_dir(&raw).map_err(|error| error.to_string())?;
    fs::write(raw.join("real.toml"), b"replacement").map_err(|error| error.to_string())?;
    assert_eq!(evidence.read_toml(Path::new("real.toml"))?, b"schema = 1\n");

    fs::create_dir(root.join("real-prefix")).map_err(|error| error.to_string())?;
    symlink("real-prefix", root.join("linked-prefix")).map_err(|error| error.to_string())?;
    assert!(NoFollowEvidenceDir::open_beneath(&root, Path::new("linked-prefix")).is_err());
    fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn raw_route_reader_rejects_nonregular_and_oversized_children() -> Result<(), String> {
    use std::os::unix::net::UnixListener;

    let root = std::env::temp_dir().join(format!(
        "typokat-wu0b-special-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let raw = root.join("raw-routing");
    fs::create_dir_all(&raw).map_err(|error| error.to_string())?;
    fs::create_dir(raw.join("directory.toml")).map_err(|error| error.to_string())?;
    let _socket = UnixListener::bind(raw.join("socket.toml")).map_err(|error| error.to_string())?;
    let oversized =
        fs::File::create(raw.join("oversized.toml")).map_err(|error| error.to_string())?;
    oversized
        .set_len(MAX_RAW_ROUTING_ARTIFACT_BYTES + 1)
        .map_err(|error| error.to_string())?;

    let evidence = NoFollowEvidenceDir::open_beneath(&root, Path::new("raw-routing"))?;
    rustix::fs::mkfifoat(
        &evidence.directory,
        "fifo.toml",
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .map_err(|error| error.to_string())?;
    assert!(evidence.read_toml(Path::new("directory.toml")).is_err());
    assert!(evidence.read_toml(Path::new("socket.toml")).is_err());
    assert!(evidence.read_toml(Path::new("fifo.toml")).is_err());
    assert!(evidence.read_toml(Path::new("oversized.toml")).is_err());
    drop(_socket);
    fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    Ok(())
}

#[test]
fn conformance_manifest_rejects_one_broken_project_group() -> Result<(), String> {
    let mut document =
        load_document(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CONFORMANCE_MANIFEST_PATH))?;
    let rows = root_mut(&mut document)?
        .get_mut("invocation")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "missing invocation rows".to_owned())?;
    let project = rows
        .iter_mut()
        .filter_map(Value::as_table_mut)
        .find(|row| {
            row.get("kind").and_then(Value::as_str) == Some("project")
                && row
                    .get("source")
                    .and_then(Value::as_array)
                    .is_some_and(|sources| sources.len() > 1)
        })
        .ok_or_else(|| "expected a multi-source project invocation".to_owned())?;
    project.insert("kind".into(), Value::String("single".into()));
    assert!(validate_conformance_invocation_manifest(&document).is_err());
    Ok(())
}

#[test]
fn official_source_manifest_binds_ids_to_pinned_bytes() -> Result<(), String> {
    let single = b"// @strict: true\nconst value = 1;\n".to_vec();
    let multi =
        b"// @strict: true\n// @filename: a.ts\nexport {};\n// @filename: b.ts\nexport {};\n"
            .to_vec();
    let fake = BTreeMap::from([
        ("single.ts".to_owned(), single.clone()),
        ("multi.ts".to_owned(), multi.clone()),
    ]);
    let (sources, _) = build_official_source_manifest(fake.keys().cloned(), |id| {
        fake.get(id)
            .cloned()
            .ok_or_else(|| format!("missing fake source {id}"))
    })?;
    assert_eq!(sources["single.ts"].source_count, 1);
    assert_eq!(sources["multi.ts"].source_count, 2);
    expect_equal_string(
        &sources["single.ts"].input_sha256,
        &sha256_hex(&single),
        "official source SHA",
    )?;
    assert!(expect_equal_string(
        "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &sources["single.ts"].input_sha256,
        "mutated official source SHA",
    )
    .is_err());
    Ok(())
}

#[test]
fn bridge_contract_rejects_duplicate_candidate_and_unapproved_matrix() {
    let mut ids = BRIDGE_IDS.to_vec();
    ids[8] = ids[0];
    assert!(!bridge_ids_are_exact(&ids));
    assert!(bridge_ids_are_exact(BRIDGE_IDS));
    assert!(!matrix_is_code_approved(
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        None,
    ));
}

#[test]
fn code_pinned_measurement_approvals_keep_go_unreachable() {
    assert!(APPROVED_PRIVATE_FALLBACK_WALL_MS.is_none());
    assert!(APPROVED_ROUTING_PRIVATE_FRACTION_PPM.is_none());
    assert!(APPROVED_ROUTING_PROJECTED_SUITE_MS.is_none());
    assert!(APPROVED_ROUTING_OBSERVED_SUITE_MS.is_none());
    assert!(APPROVED_BENCHMARK_SAMPLE_COUNT.is_none());
    assert!(APPROVED_FANOUT_WORKERS.is_none());
    assert!(APPROVED_BRIDGE_MATRIX_SHA256.is_none());
    assert!(APPROVED_CONFORMANCE_INVOCATION_MANIFEST_SHA256.is_none());
    assert!(APPROVED_OFFICIAL_SOURCE_MANIFEST_SHA256.is_none());
    assert!(APPROVED_ROUTING_EVIDENCE_MANIFEST_SHA256.is_none());
}

#[test]
fn mixed_checker_and_collector_revisions_are_rejected() {
    let revision = "0123456789abcdef0123456789abcdef01234567";
    let other = "1123456789abcdef0123456789abcdef01234567";
    let mut readiness = Table::new();
    readiness.insert("checker_revision".into(), Value::String(revision.into()));
    let collector = |value: &str| {
        let mut table = Table::new();
        table.insert("collector_revision".into(), Value::String(value.into()));
        table
    };
    assert!(validate_complete_revision_binding(
        &readiness,
        &collector(revision),
        &collector(revision),
        &collector(revision),
    )
    .is_ok());
    assert!(validate_complete_revision_binding(
        &readiness,
        &collector(revision),
        &collector(other),
        &collector(revision),
    )
    .is_err());
}

fn load_bundle() -> Result<Bundle, String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Ok(Bundle {
        readiness: load_document(&root.join(READINESS_PATH))?,
        ledger: load_document(&root.join(LEDGER_PATH))?,
        bridges: load_document(&root.join(BRIDGES_PATH))?,
        routing: load_document(&root.join(ROUTING_PATH))?,
        conformance_manifest: load_document(&root.join(CONFORMANCE_MANIFEST_PATH))?,
        profile: load_document(&root.join(PROFILE_PATH))?,
    })
}

fn load_document(path: &Path) -> Result<Document, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
    let value = text
        .parse::<Value>()
        .map_err(|error| format!("invalid TOML {}: {error}", path.display()))?;
    Ok(Document { bytes, value })
}

fn validate_bundle(bundle: &Bundle) -> Result<Verdict, String> {
    let readiness = root(&bundle.readiness)?;
    let ledger = root(&bundle.ledger)?;
    let bridges = root(&bundle.bridges)?;
    let routing = root(&bundle.routing)?;
    let profile = root(&bundle.profile)?;

    validate_readiness_shape(readiness)?;
    validate_ledger_shape(ledger)?;
    validate_bridges_shape(bridges)?;
    validate_routing_shape(routing)?;
    validate_conformance_manifest_shape(root(&bundle.conformance_manifest)?)?;
    validate_profile_binding(bundle, profile)?;

    let statuses = [
        required_string(readiness, "status")?,
        required_string(ledger, "status")?,
        required_string(bridges, "status")?,
        required_string(routing, "status")?,
    ];
    if statuses.iter().all(|status| *status == "PENDING") {
        validate_pending(bundle)?;
        return Ok(Verdict::Pending);
    }
    if !statuses.iter().all(|status| *status == "COMPLETE") {
        return Err(format!("mixed or invalid evidence statuses: {statuses:?}"));
    }
    reject_sentinels(&bundle.readiness.value, "readiness")?;
    reject_sentinels(&bundle.ledger.value, "ledger")?;
    reject_sentinels(&bundle.bridges.value, "bridges")?;
    reject_sentinels(&bundle.routing.value, "routing")?;
    validate_complete_revision_binding(readiness, ledger, bridges, routing)?;

    let ledger_complete = validate_complete_ledger(ledger, profile)?;
    let bridges_complete = validate_complete_bridges(bridges, profile)?;
    let routing_approved = validate_complete_routing(routing, &bundle.conformance_manifest)?;
    let facts = readiness_gate_facts(
        readiness,
        ledger_complete,
        bridges_complete,
        routing_approved,
    )?;
    let computed = recompute_verdict(&facts);
    let expected = match computed {
        Verdict::Go => "GO",
        Verdict::NoGo => "NO-GO",
        Verdict::Pending => return Err("complete evidence recomputed to PENDING".to_owned()),
    };
    expect_string(readiness, "conclusion", expected)?;
    expect_string(required_table(readiness, "verdict")?, "claimed", expected)?;
    Ok(computed)
}

fn validate_complete_revision_binding(
    readiness: &Table,
    ledger: &Table,
    bridges: &Table,
    routing: &Table,
) -> Result<(), String> {
    let revision = required_string(readiness, "checker_revision")?;
    validate_lower_hex(revision, 40)?;
    for (label, document) in [
        ("ledger", ledger),
        ("bridges", bridges),
        ("routing", routing),
    ] {
        let collector = required_string(document, "collector_revision")?;
        if collector != revision {
            return Err(format!(
                "{label} collector revision {collector} differs from checker revision {revision}"
            ));
        }
    }
    Ok(())
}

fn validate_readiness_shape(table: &Table) -> Result<(), String> {
    exact_keys(table, READINESS_KEYS, "readiness root")?;
    expect_usize(table, "schema", 1)?;
    for key in [
        "status",
        "target",
        "typescript_version",
        "upstream_revision",
        "checker_revision",
        "profile_path",
        "profile_sha256",
        "profile_registry_sha256",
        "prototype_entry",
        "benchmark_host",
        "reason",
        "conclusion",
    ] {
        required_string(table, key)?;
    }
    exact_keys(
        required_table(table, "artifacts")?,
        ARTIFACT_KEYS,
        "artifacts",
    )?;
    exact_keys(
        required_table(table, "protocol")?,
        PROTOCOL_KEYS,
        "protocol",
    )?;
    let warm = required_table(table, "warm")?;
    exact_keys(warm, WARM_KEYS, "warm")?;
    required_usize_array(warm, "delta_baseline_samples_us")?;
    required_usize_array(warm, "shared_base_samples_us")?;
    for row in required_array(warm, "pointer_identity")? {
        exact_keys(
            as_table(row, "pointer identity")?,
            POINTER_IDENTITY_KEYS,
            "pointer identity",
        )?;
    }
    exact_keys(required_table(table, "private")?, PRIVATE_KEYS, "private")?;
    exact_keys(required_table(table, "freeze")?, FREEZE_KEYS, "freeze")?;
    exact_keys(required_table(table, "verdict")?, VERDICT_KEYS, "verdict")?;
    for (index, row) in required_array(table, "phase_sample")?.iter().enumerate() {
        exact_keys(
            as_table(row, &format!("phase_sample[{index}]"))?,
            PHASE_KEYS,
            "phase sample",
        )?;
    }
    for (index, row) in required_array(table, "cold_sample")?.iter().enumerate() {
        exact_keys(
            as_table(row, &format!("cold_sample[{index}]"))?,
            COLD_KEYS,
            "cold sample",
        )?;
    }
    Ok(())
}

fn validate_ledger_shape(table: &Table) -> Result<(), String> {
    exact_keys(table, LEDGER_KEYS, "ledger root")?;
    expect_usize(table, "schema", 1)?;
    for key in [
        "status",
        "profile_path",
        "profile_sha256",
        "profile_registry_sha256",
        "collector_revision",
        "source_census_sha256",
    ] {
        required_string(table, key)?;
    }
    for key in [
        "file_count",
        "declaration_count",
        "outcome_count",
        "unavailable_count",
        "diagnostic_count",
        "incomplete_count",
        "parser_error_count",
        "library_event_count",
    ] {
        required_usize(table, key)?;
    }
    for row in required_array(table, "file")? {
        exact_keys(
            as_table(row, "ledger file")?,
            LEDGER_FILE_KEYS,
            "ledger file",
        )?;
    }
    for row in required_array(table, "declaration")? {
        exact_keys(
            as_table(row, "declaration")?,
            DECLARATION_KEYS,
            "declaration",
        )?;
    }
    for row in required_array(table, "outcome")? {
        exact_keys(as_table(row, "outcome")?, OUTCOME_KEYS, "outcome")?;
    }
    Ok(())
}

fn validate_bridges_shape(table: &Table) -> Result<(), String> {
    exact_keys(table, BRIDGES_KEYS, "bridges root")?;
    expect_usize(table, "schema", 1)?;
    for row in required_array(table, "candidate")? {
        let row = as_table(row, "bridge candidate")?;
        exact_keys(row, BRIDGE_ROW_KEYS, "bridge candidate")?;
        for key in [
            "good_witness",
            "bad_witness",
            "shadowing_witness",
            "missing_member_witness",
            "shared_universe_witness",
            "private_universe_witness",
        ] {
            exact_keys(required_table(row, key)?, WITNESS_KEYS, key)?;
        }
    }
    Ok(())
}

fn validate_routing_shape(table: &Table) -> Result<(), String> {
    exact_keys(table, ROUTING_KEYS, "routing root")?;
    expect_usize(table, "schema", 1)?;
    exact_keys(
        required_table(table, "summary")?,
        ROUTING_SUMMARY_KEYS,
        "routing summary",
    )?;
    exact_keys(
        required_table(table, "approval")?,
        APPROVAL_KEYS,
        "routing approval",
    )?;
    for row in required_array(table, "corpus")? {
        exact_keys(
            as_table(row, "routing corpus")?,
            CORPUS_KEYS,
            "routing corpus",
        )?;
    }
    for row in required_array(table, "route")? {
        let row = as_table(row, "route")?;
        exact_keys(row, ROUTE_KEYS, "route")?;
        exact_keys(
            required_table(row, "observed_output")?,
            WITNESS_KEYS,
            "observed output",
        )?;
        exact_keys(
            required_table(row, "forced_private_output")?,
            WITNESS_KEYS,
            "forced-private output",
        )?;
    }
    Ok(())
}

fn validate_conformance_manifest_shape(table: &Table) -> Result<(), String> {
    exact_keys(
        table,
        CONFORMANCE_MANIFEST_KEYS,
        "conformance invocation manifest",
    )?;
    expect_usize(table, "schema", 1)?;
    for row in required_array(table, "invocation")? {
        exact_keys(
            as_table(row, "conformance invocation")?,
            CONFORMANCE_INVOCATION_KEYS,
            "conformance invocation",
        )?;
    }
    Ok(())
}

fn validate_profile_binding(bundle: &Bundle, profile: &Table) -> Result<(), String> {
    verify_sha256(&bundle.profile.bytes, PROFILE_SHA256, "profile.toml")?;
    expect_usize(profile, "schema", 1)?;
    expect_string(profile, "typescript_version", TYPESCRIPT_VERSION)?;
    expect_string(profile, "upstream_revision", UPSTREAM_REVISION)?;
    expect_usize(profile, "file_count", 82)?;
    expect_usize(profile, "script_file_count", 81)?;
    expect_usize(profile, "external_module_file_count", 1)?;
    expect_usize(profile, "reference_edge_count", 110)?;
    expect_usize(profile, "source_bytes", 2_936_611)?;
    expect_usize(profile, "source_lf", 58_349)?;
    expect_string(profile, "length_framed_sha256", PROFILE_REGISTRY_SHA256)?;

    let readiness = root(&bundle.readiness)?;
    expect_string(readiness, "typescript_version", TYPESCRIPT_VERSION)?;
    expect_string(readiness, "upstream_revision", UPSTREAM_REVISION)?;
    expect_string(readiness, "profile_path", PROFILE_PATH)?;
    expect_string(readiness, "profile_sha256", PROFILE_SHA256)?;
    expect_string(
        readiness,
        "profile_registry_sha256",
        PROFILE_REGISTRY_SHA256,
    )?;
    expect_string(readiness, "prototype_entry", PROTOTYPE_ENTRY)?;
    for document in [&bundle.ledger, &bundle.bridges, &bundle.routing] {
        expect_string(root(document)?, "profile_sha256", PROFILE_SHA256)?;
    }
    expect_string(root(&bundle.ledger)?, "profile_path", PROFILE_PATH)?;
    expect_string(
        root(&bundle.ledger)?,
        "profile_registry_sha256",
        PROFILE_REGISTRY_SHA256,
    )?;

    let artifacts = required_table(readiness, "artifacts")?;
    verify_artifact(bundle, artifacts, "ledger", LEDGER_PATH)?;
    verify_artifact(bundle, artifacts, "bridges", BRIDGES_PATH)?;
    verify_artifact(bundle, artifacts, "routing", ROUTING_PATH)?;
    verify_artifact(
        bundle,
        artifacts,
        "conformance_manifest",
        CONFORMANCE_MANIFEST_PATH,
    )?;
    validate_conformance_invocation_manifest(&bundle.conformance_manifest)?;
    Ok(())
}

fn verify_artifact(
    bundle: &Bundle,
    artifacts: &Table,
    name: &str,
    path: &str,
) -> Result<(), String> {
    expect_string(artifacts, &format!("{name}_path"), path)?;
    let document = match name {
        "ledger" => &bundle.ledger,
        "bridges" => &bundle.bridges,
        "routing" => &bundle.routing,
        "conformance_manifest" => &bundle.conformance_manifest,
        _ => return Err(format!("unknown evidence artifact {name}")),
    };
    verify_sha256(
        &document.bytes,
        required_string(artifacts, &format!("{name}_sha256"))?,
        name,
    )
}

fn validate_pending(bundle: &Bundle) -> Result<(), String> {
    let readiness = root(&bundle.readiness)?;
    expect_string(readiness, "conclusion", "PENDING")?;
    expect_string(readiness, "checker_revision", "PENDING")?;
    expect_string(required_table(readiness, "verdict")?, "claimed", "PENDING")?;
    for section in ["warm", "private", "freeze"] {
        expect_string(required_table(readiness, section)?, "status", "PENDING")?;
    }
    let warm = required_table(readiness, "warm")?;
    for key in [
        "delta_baseline_samples_us",
        "shared_base_samples_us",
        "pointer_identity",
    ] {
        if !required_array(warm, key)?.is_empty() {
            return Err(format!("PENDING warm evidence must keep {key} empty"));
        }
    }
    if !required_array(readiness, "phase_sample")?.is_empty()
        || !required_array(readiness, "cold_sample")?.is_empty()
    {
        return Err("PENDING readiness must not carry measurement samples".to_owned());
    }
    let ledger = root(&bundle.ledger)?;
    expect_string(ledger, "source_census_sha256", "PENDING")?;
    for key in ["file", "declaration", "outcome"] {
        if !required_array(ledger, key)?.is_empty() {
            return Err(format!("PENDING ledger must keep {key} empty"));
        }
    }
    let bridges = root(&bundle.bridges)?;
    expect_string(bridges, "matrix_sha256", "PENDING")?;
    let ids = bridge_ids(bridges)?;
    if ids != BRIDGE_IDS.iter().map(|id| (*id).to_owned()).collect() {
        return Err(format!("wrong fixed bridge candidates: {ids:?}"));
    }
    for row in required_array(bridges, "candidate")? {
        let row = as_table(row, "bridge candidate")?;
        expect_string(row, "status", "PENDING")?;
        expect_string(row, "decision", "PENDING")?;
        if !required_array(row, "consumers")?.is_empty() {
            return Err("PENDING bridge consumers must be empty".to_owned());
        }
    }
    let routing = root(&bundle.routing)?;
    expect_string(required_table(routing, "approval")?, "decision", "PENDING")?;
    if !required_array(routing, "route")?.is_empty() {
        return Err("PENDING routing must not contain measured route rows".to_owned());
    }
    let corpus_ids: BTreeSet<_> = required_array(routing, "corpus")?
        .iter()
        .map(|row| {
            let row = as_table(row, "routing corpus")?;
            expect_string(row, "status", "PENDING")?;
            Ok(required_string(row, "id")?.to_owned())
        })
        .collect::<Result<_, String>>()?;
    if corpus_ids != ROUTING_CORPORA.iter().map(|id| (*id).to_owned()).collect() {
        return Err(format!("wrong routing corpora: {corpus_ids:?}"));
    }
    Ok(())
}

fn validate_complete_ledger(ledger: &Table, profile: &Table) -> Result<bool, String> {
    expect_usize(ledger, "file_count", 82)?;
    expect_usize(ledger, "parser_error_count", 0)?;
    let files = required_array(ledger, "file")?;
    let profile_files = required_array(profile, "file")?;
    if files.len() != profile_files.len() {
        return Err("ledger must contain every profile file".to_owned());
    }
    let root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut source_lengths = Vec::with_capacity(files.len());
    let mut expected_declarations = BTreeSet::new();
    for (ordinal, (row, profile_row)) in files.iter().zip(profile_files).enumerate() {
        let row = as_table(row, "ledger file")?;
        let profile_row = as_table(profile_row, "profile file")?;
        let name = required_string(profile_row, "name")?;
        let expected_sha = required_string(profile_row, "sha256")?;
        expect_usize(row, "ordinal", ordinal)?;
        expect_string(row, "name", name)?;
        expect_string(row, "sha256", expected_sha)?;
        expect_string(
            row,
            "source_kind",
            required_string(profile_row, "source_kind")?,
        )?;
        expect_usize(row, "parse_errors", 0)?;
        required_usize(row, "declarations")?;
        required_usize(row, "outcomes")?;
        let source = fs::read(root_path.join(PROFILE_LIB_DIR).join(name))
            .map_err(|error| format!("cannot read profile source {name}: {error}"))?;
        verify_sha256(&source, expected_sha, name)?;
        let source = std::str::from_utf8(&source)
            .map_err(|error| format!("profile source {name} is not UTF-8: {error}"))?;
        source_lengths.push(source.len());
        expected_declarations.extend(source_occurrences(ordinal, source)?);
    }

    let declarations = required_array(ledger, "declaration")?;
    let outcomes = required_array(ledger, "outcome")?;
    expect_usize(ledger, "declaration_count", expected_declarations.len())?;
    expect_usize(ledger, "outcome_count", outcomes.len())?;
    expect_usize(ledger, "library_event_count", outcomes.len())?;
    expect_string(
        ledger,
        "source_census_sha256",
        &source_census_sha256(&expected_declarations),
    )?;
    let mut owners = BTreeSet::new();
    let mut unavailable_owners = BTreeSet::new();
    let mut observed_declarations = BTreeSet::new();
    let mut declaration_counts = vec![0usize; files.len()];
    for row in declarations {
        let row = as_table(row, "declaration")?;
        let file = required_usize(row, "file_ordinal")?;
        if file >= files.len() {
            return Err(format!(
                "declaration has invalid library file ordinal {file}"
            ));
        }
        let declaration_start = required_usize(row, "declaration_start")?;
        let declaration_end = required_usize(row, "declaration_end")?;
        let binding_start = required_usize(row, "binding_start")?;
        let binding_end = required_usize(row, "binding_end")?;
        if declaration_start > binding_start
            || binding_start >= binding_end
            || binding_end > declaration_end
            || declaration_end > source_lengths[file]
        {
            return Err(format!("invalid declaration/binding span in file {file}"));
        }
        let kind = declaration_kind(required_string(row, "kind")?)?;
        if !observed_declarations.insert(SourceOccurrence {
            file_ordinal: file,
            declaration_start,
            declaration_end,
            binding_start,
            binding_end,
            kind,
        }) {
            return Err("duplicate source declaration occurrence".to_owned());
        }
        declaration_counts[file] += 1;
        let owner = required_string(row, "owner")?.to_owned();
        if owner.is_empty() || !owners.insert(owner.clone()) {
            return Err(format!("missing or duplicate declaration owner {owner:?}"));
        }
        let availability = required_string(row, "availability")?;
        if availability == "unavailable" {
            unavailable_owners.insert(owner);
        } else if availability != "ready" {
            return Err(format!("invalid declaration availability {availability:?}"));
        }
    }
    if observed_declarations != expected_declarations {
        return Err("ledger declaration rows differ from the independent source census".to_owned());
    }
    expect_usize(ledger, "unavailable_count", unavailable_owners.len())?;
    let mut event_keys = BTreeSet::new();
    let mut unavailable_outcomes = BTreeSet::new();
    let mut diagnostics = 0usize;
    let mut incompletes = 0usize;
    let mut outcome_counts = vec![0usize; files.len()];
    for row in outcomes {
        let row = as_table(row, "outcome")?;
        validate_outcome_row(row, files.len())?;
        let file = required_usize(row, "file_ordinal")?;
        if required_usize(row, "source_start")? >= source_lengths[file] {
            return Err(format!("outcome source offset is outside file {file}"));
        }
        outcome_counts[file] += 1;
        let owner = required_string(row, "owner")?;
        if !owners.contains(owner) {
            return Err(format!("outcome has no live declaration owner: {owner}"));
        }
        let key = (
            required_usize(row, "file_ordinal")?,
            required_usize(row, "source_start")?,
            required_usize(row, "event_ordinal")?,
            required_usize(row, "record_ordinal")?,
        );
        if !event_keys.insert(key) {
            return Err(format!("duplicate library event key {key:?}"));
        }
        match required_string(row, "kind")? {
            "diagnostic" => diagnostics += 1,
            "incomplete" => incompletes += 1,
            "unavailable" => {
                unavailable_outcomes.insert(owner.to_owned());
            }
            kind => return Err(format!("invalid library outcome kind {kind:?}")),
        }
    }
    if unavailable_outcomes != unavailable_owners {
        return Err("every unavailable declaration needs one owned unavailable outcome".to_owned());
    }
    expect_usize(ledger, "diagnostic_count", diagnostics)?;
    expect_usize(ledger, "incomplete_count", incompletes)?;
    for (ordinal, row) in files.iter().enumerate() {
        let row = as_table(row, "ledger file")?;
        expect_usize(row, "declarations", declaration_counts[ordinal])?;
        expect_usize(row, "outcomes", outcome_counts[ordinal])?;
    }
    Ok(true)
}

fn validate_outcome_row(row: &Table, file_count: usize) -> Result<(), String> {
    exact_keys(row, OUTCOME_KEYS, "library outcome")?;
    if required_usize(row, "file_ordinal")? >= file_count {
        return Err("library outcome file ordinal is out of range".to_owned());
    }
    for key in ["source_start", "event_ordinal", "record_ordinal"] {
        required_usize(row, key)?;
    }
    for key in ["kind", "identity", "owner"] {
        if required_string(row, key)?.is_empty() {
            return Err(format!("library outcome {key} cannot be empty"));
        }
    }
    Ok(())
}

fn source_occurrences(
    file_ordinal: usize,
    source: &str,
) -> Result<BTreeSet<SourceOccurrence>, String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(format!(
            "independent source census found {} parser diagnostics in file {file_ordinal}",
            parsed.diagnostics.len()
        ));
    }
    let mut visitor = SourceCensusVisitor::default();
    visitor.visit_program(&parsed.program);
    visitor
        .occurrences
        .into_iter()
        .map(
            |(declaration_start, declaration_end, binding_start, binding_end, kind)| {
                let occurrence = SourceOccurrence {
                    file_ordinal,
                    declaration_start,
                    declaration_end,
                    binding_start,
                    binding_end,
                    kind,
                };
                if declaration_start > binding_start
                    || binding_start >= binding_end
                    || binding_end > declaration_end
                    || declaration_end > source.len()
                {
                    return Err(format!(
                        "OXC produced an invalid declaration span: {occurrence:?}"
                    ));
                }
                Ok(occurrence)
            },
        )
        .collect()
}

fn declaration_kind(kind: &str) -> Result<&'static str, String> {
    match kind {
        "variable" => Ok("variable"),
        "function" => Ok("function"),
        "class" => Ok("class"),
        "parameter" => Ok("parameter"),
        "catch-parameter" => Ok("catch-parameter"),
        "import" => Ok("import"),
        "type-alias" => Ok("type-alias"),
        "interface" => Ok("interface"),
        "enum" => Ok("enum"),
        "namespace" => Ok("namespace"),
        "import-equals" => Ok("import-equals"),
        "namespace-export" => Ok("namespace-export"),
        "global" => Ok("global"),
        _ => Err(format!("unknown declaration kind {kind:?}")),
    }
}

fn source_census_sha256(census: &BTreeSet<SourceOccurrence>) -> String {
    let mut hasher = Sha256::new();
    for row in census {
        for value in [
            row.file_ordinal,
            row.declaration_start,
            row.declaration_end,
            row.binding_start,
            row.binding_end,
        ] {
            hasher.update(
                u64::try_from(value)
                    .expect("census value fits u64")
                    .to_be_bytes(),
            );
        }
        hasher.update(
            u64::try_from(row.kind.len())
                .expect("kind length fits u64")
                .to_be_bytes(),
        );
        hasher.update(row.kind.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn validate_complete_bridges(bridges: &Table, profile: &Table) -> Result<bool, String> {
    expect_usize(bridges, "candidate_count", BRIDGE_IDS.len())?;
    expect_usize(bridges, "unexplained_consumer_count", 0)?;
    let candidates = required_array(bridges, "candidate")?;
    if candidates.len() != BRIDGE_IDS.len() {
        return Err(format!(
            "bridge matrix has {} rows, expected {}",
            candidates.len(),
            BRIDGE_IDS.len()
        ));
    }
    let ids = bridge_ids(bridges)?;
    if ids.len() != candidates.len()
        || ids != BRIDGE_IDS.iter().map(|id| (*id).to_owned()).collect()
    {
        return Err(format!("wrong fixed bridge candidates: {ids:?}"));
    }
    let profile_files: BTreeMap<_, _> = required_array(profile, "file")?
        .iter()
        .map(|row| {
            let row = as_table(row, "profile file")?;
            Ok((
                required_string(row, "name")?.to_owned(),
                required_string(row, "sha256")?.to_owned(),
            ))
        })
        .collect::<Result<_, String>>()?;
    let root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut consumer_count = 0usize;
    for row in candidates {
        let row = as_table(row, "bridge candidate")?;
        expect_string(row, "status", "COMPLETE")?;
        let id = required_string(row, "id")?;
        let decision = required_string(row, "decision")?;
        if !matches!(decision, "required" | "not-required" | "deferred") {
            return Err(format!("bridge {id} has invalid decision {decision:?}"));
        }
        if matches!(id, "mutable-array-tuple" | "regexp-literal") && decision != "required" {
            return Err(format!("ADR-0011 requires bridge {id}"));
        }
        if decision == "required" && required_string(row, "identity_role")? == "none" {
            return Err(format!(
                "required bridge {id} needs a universe-local identity role"
            ));
        }
        let declaration_file = required_string(row, "declaration_file")?;
        let expected_source_sha = profile_files
            .get(declaration_file)
            .ok_or_else(|| format!("bridge {id} declaration is outside the pinned profile"))?;
        let source = fs::read(root_path.join(PROFILE_LIB_DIR).join(declaration_file))
            .map_err(|error| format!("cannot read bridge source {declaration_file}: {error}"))?;
        verify_sha256(&source, expected_source_sha, declaration_file)?;
        let start = required_usize(row, "declaration_start")?;
        let end = required_usize(row, "declaration_end")?;
        let identity = required_string(row, "declaration_identity")?;
        if start >= end || source.get(start..end) != Some(identity.as_bytes()) {
            return Err(format!(
                "bridge {id} declaration identity/span does not match {declaration_file}"
            ));
        }
        for key in [
            "identity_role",
            "oracle_good",
            "oracle_bad",
            "typokat_current",
            "owner",
            "rationale",
            "falsifier",
        ] {
            if required_string(row, key)?.is_empty() {
                return Err(format!("bridge {id} lacks {key}"));
            }
        }
        for key in [
            "good_witness",
            "bad_witness",
            "shadowing_witness",
            "missing_member_witness",
            "shared_universe_witness",
            "private_universe_witness",
        ] {
            validate_witness(required_table(row, key)?, &format!("bridge {id} {key}"))?;
        }
        let consumers = required_string_array(row, "consumers")?;
        if consumers.is_empty()
            || consumers.iter().collect::<BTreeSet<_>>().len() != consumers.len()
        {
            return Err(format!(
                "bridge {id} needs a unique complete consumer inventory"
            ));
        }
        consumer_count += consumers.len();
    }
    expect_usize(bridges, "consumer_count", consumer_count)?;
    let matrix_sha = bridge_matrix_sha256(candidates)?;
    expect_string(bridges, "matrix_sha256", &matrix_sha)?;
    Ok(matrix_is_code_approved(
        &matrix_sha,
        APPROVED_BRIDGE_MATRIX_SHA256,
    ))
}

fn bridge_ids(bridges: &Table) -> Result<BTreeSet<String>, String> {
    required_array(bridges, "candidate")?
        .iter()
        .map(|row| Ok(required_string(as_table(row, "bridge candidate")?, "id")?.to_owned()))
        .collect()
}

fn bridge_ids_are_exact(ids: &[&str]) -> bool {
    ids.len() == BRIDGE_IDS.len()
        && ids.iter().copied().collect::<BTreeSet<_>>().len() == BRIDGE_IDS.len()
        && ids.iter().copied().collect::<BTreeSet<_>>()
            == BRIDGE_IDS.iter().copied().collect::<BTreeSet<_>>()
}

fn bridge_matrix_sha256(candidates: &[Value]) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for candidate in candidates {
        let row = as_table(candidate, "bridge candidate")?;
        let encoded = toml::to_string(row)
            .map_err(|error| format!("cannot canonicalize bridge matrix: {error}"))?;
        hasher.update(
            u64::try_from(encoded.len())
                .expect("bridge row length fits u64")
                .to_be_bytes(),
        );
        hasher.update(encoded.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn matrix_is_code_approved(actual: &str, approved: Option<&str>) -> bool {
    approved == Some(actual)
}

fn validate_witness(witness: &Table, label: &str) -> Result<(), String> {
    let path = Path::new(required_string(witness, "path")?);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!("{label} path must be repository-relative"));
    }
    let bytes = fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path))
        .map_err(|error| format!("cannot read {label} {}: {error}", path.display()))?;
    verify_sha256(&bytes, required_string(witness, "sha256")?, label)
}

fn validate_route_output_artifact(
    evidence_dir: &NoFollowEvidenceDir,
    witness: &Table,
    checker_revision: &str,
    corpus: &str,
    invocation: &str,
    mode: &str,
    input_sha256: &str,
) -> Result<RouteOutputArtifact, String> {
    exact_keys(witness, WITNESS_KEYS, "route output witness")?;
    let path = required_string(witness, "path")?;
    let child = raw_route_child(Path::new(path), Path::new(RAW_ROUTING_DIR))?;
    let bytes = evidence_dir.read_toml(child)?;
    let file_sha256 = required_string(witness, "sha256")?;
    verify_sha256(&bytes, file_sha256, path)?;
    parse_route_output_artifact(
        &bytes,
        path,
        file_sha256,
        &RouteArtifactContext {
            checker_revision,
            corpus,
            invocation,
            mode,
            input_sha256,
        },
    )
}

fn parse_route_output_artifact(
    bytes: &[u8],
    path: &str,
    file_sha256: &str,
    context: &RouteArtifactContext<'_>,
) -> Result<RouteOutputArtifact, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| format!("raw route artifact {path} is not UTF-8: {error}"))?;
    let value = text
        .parse::<Value>()
        .map_err(|error| format!("invalid raw route artifact {path}: {error}"))?;
    let artifact = value
        .as_table()
        .ok_or_else(|| format!("raw route artifact {path} must be a TOML table"))?;
    exact_keys(artifact, RAW_ROUTE_ARTIFACT_KEYS, "raw route artifact")?;
    expect_usize(artifact, "schema", 1)?;
    expect_string(artifact, "checker_revision", context.checker_revision)?;
    expect_string(artifact, "corpus", context.corpus)?;
    expect_string(artifact, "invocation", context.invocation)?;
    expect_string(artifact, "mode", context.mode)?;
    expect_string(artifact, "input_sha256", context.input_sha256)?;
    let source_count = required_usize(artifact, "source_count")?;
    if source_count == 0 {
        return Err(format!("raw route artifact {path} has zero sources"));
    }
    let collected_roots = required_string_array(artifact, "collected_roots")?;
    let categories = required_string_array(artifact, "categories")?;
    validate_routing_collections(&collected_roots, &categories, path)?;
    let candidate_count = required_usize(artifact, "candidate_count")?;
    if candidate_count != collected_roots.len() {
        return Err(format!(
            "raw route artifact {path} candidate count differs from roots"
        ));
    }
    let route = required_string(artifact, "route")?;
    if !matches!(route, "fast" | "private") {
        return Err(format!(
            "raw route artifact {path} has invalid route {route:?}"
        ));
    }
    let elapsed_ms = required_usize(artifact, "elapsed_ms")?;
    if elapsed_ms == 0 {
        return Err(format!("raw route artifact {path} has zero elapsed time"));
    }
    let semantic_output_sha256 = required_string(artifact, "semantic_output_sha256")?;
    validate_lower_hex(semantic_output_sha256, 64)?;
    Ok(RouteOutputArtifact {
        path: path.to_owned(),
        file_sha256: file_sha256.to_owned(),
        mode: context.mode.to_owned(),
        source_count,
        collected_roots,
        categories,
        candidate_count,
        route: route.to_owned(),
        elapsed_ms,
        semantic_output_sha256: semantic_output_sha256.to_owned(),
    })
}

fn validate_routing_collections(
    roots: &[String],
    categories: &[String],
    label: &str,
) -> Result<(), String> {
    if roots.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(format!("{label} collected roots must be sorted and unique"));
    }
    let canonical_categories: Vec<_> = ROUTE_CATEGORIES
        .iter()
        .filter(|candidate| categories.iter().any(|category| category == **candidate))
        .map(|category| (*category).to_owned())
        .collect();
    if canonical_categories != categories {
        return Err(format!(
            "{label} categories must be unique and in canonical order"
        ));
    }
    Ok(())
}

fn raw_route_child<'a>(relative: &'a Path, required_prefix: &Path) -> Result<&'a Path, String> {
    if relative.is_absolute()
        || relative == required_prefix
        || !relative.starts_with(required_prefix)
        || relative
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("toml")
    {
        return Err(format!(
            "raw route artifact must be a .toml file under {}",
            required_prefix.display()
        ));
    }
    let child = relative
        .strip_prefix(required_prefix)
        .map_err(|error| format!("cannot strip raw-routing prefix: {error}"))?;
    let mut components = child.components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("raw route artifact must be one normal .toml child of raw-routing".to_owned());
    }
    Ok(child)
}

#[cfg(unix)]
struct NoFollowEvidenceDir {
    directory: std::fs::File,
}

#[cfg(unix)]
impl NoFollowEvidenceDir {
    fn open_beneath(root: &Path, relative_directory: &Path) -> Result<Self, String> {
        use rustix::fs::{open, openat, Mode, OFlags};

        if relative_directory.is_absolute()
            || relative_directory
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err("raw-routing directory must be normalized and relative".to_owned());
        }
        let root_fd = open(
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| format!("cannot no-follow open root {}: {error}", root.display()))?;
        let mut parent: std::fs::File = root_fd.into();
        let mut saw_component = false;
        for component in relative_directory.components() {
            let std::path::Component::Normal(name) = component else {
                unreachable!("relative directory components were validated")
            };
            saw_component = true;
            let child = openat(
                &parent,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| {
                format!(
                    "descriptor-relative no-follow directory open failed for {}: {error}",
                    relative_directory.display()
                )
            })?;
            parent = child.into();
        }
        if !saw_component {
            return Err("raw-routing directory path is empty".to_owned());
        }
        Ok(Self { directory: parent })
    }

    fn read_toml(&self, child: &Path) -> Result<Vec<u8>, String> {
        use rustix::fs::{fstat, openat, FileType, Mode, OFlags};

        let mut components = child.components();
        let Some(std::path::Component::Normal(name)) = components.next() else {
            return Err("raw route artifact name must be one normal component".to_owned());
        };
        if components.next().is_some()
            || child.extension().and_then(|extension| extension.to_str()) != Some("toml")
        {
            return Err("raw route artifact name must be one normal .toml component".to_owned());
        }
        let descriptor = openat(
            &self.directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| format!("cannot no-follow open {}: {error}", child.display()))?;
        let before = fstat(&descriptor)
            .map_err(|error| format!("cannot fstat {}: {error}", child.display()))?;
        if !FileType::from_raw_mode(before.st_mode).is_file() {
            return Err(format!(
                "raw route artifact is not regular: {}",
                child.display()
            ));
        }
        let expected_size = u64::try_from(before.st_size)
            .map_err(|_| format!("raw route artifact has invalid size: {}", child.display()))?;
        if expected_size > MAX_RAW_ROUTING_ARTIFACT_BYTES {
            return Err(format!(
                "raw route artifact exceeds size cap: {}",
                child.display()
            ));
        }

        let mut file: std::fs::File = descriptor.into();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(
            &mut std::io::Read::take(&mut file, MAX_RAW_ROUTING_ARTIFACT_BYTES + 1),
            &mut bytes,
        )
        .map_err(|error| format!("cannot read {}: {error}", child.display()))?;
        let actual_size = u64::try_from(bytes.len()).map_err(|_| {
            format!(
                "raw route artifact size does not fit u64: {}",
                child.display()
            )
        })?;
        if actual_size > MAX_RAW_ROUTING_ARTIFACT_BYTES || actual_size != expected_size {
            return Err(format!(
                "raw route artifact changed size during read: {}",
                child.display()
            ));
        }
        let after = fstat(&file)
            .map_err(|error| format!("cannot re-fstat {}: {error}", child.display()))?;
        if !FileType::from_raw_mode(after.st_mode).is_file()
            || before.st_dev != after.st_dev
            || before.st_ino != after.st_ino
            || before.st_size != after.st_size
        {
            return Err(format!(
                "raw route artifact changed during read: {}",
                child.display()
            ));
        }
        Ok(bytes)
    }
}

#[cfg(not(unix))]
struct NoFollowEvidenceDir;

#[cfg(not(unix))]
impl NoFollowEvidenceDir {
    fn open_beneath(_root: &Path, _relative_directory: &Path) -> Result<Self, String> {
        Err("COMPLETE raw-routing validation requires Unix openat no-follow traversal".to_owned())
    }

    fn read_toml(&self, _child: &Path) -> Result<Vec<u8>, String> {
        Err("COMPLETE raw-routing validation requires Unix openat no-follow traversal".to_owned())
    }
}

fn validate_complete_routing(
    routing: &Table,
    conformance_manifest: &Document,
) -> Result<bool, String> {
    let evidence_dir = NoFollowEvidenceDir::open_beneath(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        Path::new(RAW_ROUTING_DIR),
    )?;
    expect_string(
        routing,
        "classifier",
        "binder::ModuleBindingContext::for_program",
    )?;
    expect_string(
        routing,
        "candidate_collector",
        "binder-shared-preflight-candidate-collector",
    )?;
    let (official_invocations, official_digest, official_source_manifest_sha) =
        committed_official_invocations()?;
    let (conformance_invocations, conformance_digest) =
        committed_conformance_invocations(conformance_manifest)?;
    let corpus_rows = required_array(routing, "corpus")?;
    let mut corpora = BTreeMap::new();
    for row in corpus_rows {
        let row = as_table(row, "routing corpus")?;
        expect_string(row, "status", "COMPLETE")?;
        let id = required_string(row, "id")?.to_owned();
        let expected_digest = match id.as_str() {
            "official-suite" => &official_digest,
            "committed-conformance" => &conformance_digest,
            _ => return Err(format!("unknown routing corpus {id}")),
        };
        expect_string(row, "input_manifest_sha256", expected_digest)?;
        if corpora.insert(id.clone(), row).is_some() {
            return Err(format!("duplicate routing corpus {id}"));
        }
    }
    if corpora.keys().cloned().collect::<BTreeSet<_>>()
        != ROUTING_CORPORA.iter().map(|id| (*id).to_owned()).collect()
    {
        return Err("routing evidence must cover both fixed corpora".to_owned());
    }
    #[derive(Default)]
    struct Counts {
        invocations: usize,
        sources: usize,
        candidates: usize,
        private: usize,
        projected_ms: usize,
        observed_ms: usize,
        categories: BTreeMap<String, usize>,
    }
    let mut counts: BTreeMap<String, Counts> = BTreeMap::new();
    let mut invocations = BTreeSet::new();
    let mut artifact_paths = BTreeSet::new();
    let mut routing_evidence_rows = Vec::new();
    let checker_revision = required_string(routing, "collector_revision")?;
    for row in required_array(routing, "route")? {
        let row = as_table(row, "route")?;
        let corpus = required_string(row, "corpus")?.to_owned();
        if !corpora.contains_key(&corpus) {
            return Err(format!("route references unknown corpus {corpus}"));
        }
        let invocation = required_string(row, "invocation")?;
        if invocation.is_empty() || !invocations.insert((corpus.clone(), invocation.to_owned())) {
            return Err(format!(
                "missing or duplicate route invocation {invocation:?}"
            ));
        }
        let input_sha = required_string(row, "input_sha256")?;
        match corpus.as_str() {
            "official-suite" => {
                let expected = official_invocations.get(invocation).ok_or_else(|| {
                    format!("official invocation is absent from scoreboard: {invocation}")
                })?;
                expect_equal_string(input_sha, &expected.input_sha256, "official input SHA")?;
                expect_usize(row, "source_count", expected.source_count)?;
            }
            "committed-conformance" => {
                let expected = conformance_invocations.get(invocation).ok_or_else(|| {
                    format!("conformance invocation is absent from committed sources: {invocation}")
                })?;
                expect_equal_string(input_sha, &expected.input_sha256, "conformance input SHA")?;
                expect_usize(row, "source_count", expected.source_count)?;
            }
            _ => unreachable!("corpus membership checked above"),
        }
        let roots = required_string_array(row, "collected_roots")?;
        let categories = required_string_array(row, "categories")?;
        validate_routing_collections(&roots, &categories, invocation)?;
        expect_usize(row, "candidate_count", roots.len())?;
        let expected_route = if categories.is_empty() {
            "fast"
        } else {
            "private"
        };
        expect_string(row, "expected_route", expected_route)?;
        expect_string(row, "observed_route", expected_route)?;
        let projected_ms = required_usize(row, "projected_ms")?;
        let observed_ms = required_usize(row, "observed_ms")?;
        let source_count = required_usize(row, "source_count")?;
        if source_count == 0 {
            return Err(format!("route {invocation} has zero sources"));
        }
        let observed_evidence = required_table(row, "observed_output")?;
        let forced_evidence = required_table(row, "forced_private_output")?;
        for path in [
            required_string(observed_evidence, "path")?,
            required_string(forced_evidence, "path")?,
        ] {
            if !artifact_paths.insert(path.to_owned()) {
                return Err(format!("route {invocation} reuses raw output file {path}"));
            }
        }
        let observed_artifact = validate_route_output_artifact(
            &evidence_dir,
            observed_evidence,
            checker_revision,
            &corpus,
            invocation,
            "observed",
            input_sha,
        )?;
        let forced_artifact = validate_route_output_artifact(
            &evidence_dir,
            forced_evidence,
            checker_revision,
            &corpus,
            invocation,
            "forced-private",
            input_sha,
        )?;
        if observed_artifact.source_count != source_count
            || observed_artifact.collected_roots != roots
            || observed_artifact.categories != categories
            || observed_artifact.candidate_count != roots.len()
            || observed_artifact.route != required_string(row, "observed_route")?
            || observed_artifact.elapsed_ms != observed_ms
        {
            return Err(format!(
                "route {invocation} summary differs from observed raw artifact"
            ));
        }
        if forced_artifact.source_count != source_count
            || forced_artifact.collected_roots != roots
            || forced_artifact.categories != categories
            || forced_artifact.candidate_count != roots.len()
            || forced_artifact.route != "private"
        {
            return Err(format!(
                "route {invocation} classifier fields differ in forced-private raw artifact"
            ));
        }
        let derived_projected_ms = if expected_route == "private" {
            forced_artifact.elapsed_ms
        } else {
            observed_artifact.elapsed_ms
        };
        if projected_ms != derived_projected_ms {
            return Err(format!(
                "route {invocation} projected_ms is not derived from raw execution"
            ));
        }
        let observed_output = &observed_artifact.semantic_output_sha256;
        let forced_output = &forced_artifact.semantic_output_sha256;
        if !route_measurement_valid(projected_ms, observed_ms, observed_output, forced_output) {
            return Err(format!(
                "route {invocation} has invalid timing or output equivalence"
            ));
        }
        routing_evidence_rows.push(RoutingEvidenceRecord {
            corpus: corpus.clone(),
            invocation: invocation.to_owned(),
            input_sha256: input_sha.to_owned(),
            source_count,
            collected_roots: roots.clone(),
            categories: categories.clone(),
            candidate_count: roots.len(),
            expected_route: expected_route.to_owned(),
            observed_route: required_string(row, "observed_route")?.to_owned(),
            projected_ms,
            observed_ms,
            observed: observed_artifact,
            forced: forced_artifact,
        });
        let entry = counts.entry(corpus).or_default();
        checked_add_to(&mut entry.invocations, 1, "route invocation")?;
        checked_add_to(&mut entry.sources, source_count, "route source")?;
        checked_add_to(&mut entry.candidates, roots.len(), "route candidate")?;
        checked_add_to(
            &mut entry.projected_ms,
            projected_ms,
            "route projected time",
        )?;
        checked_add_to(&mut entry.observed_ms, observed_ms, "route observed time")?;
        if expected_route == "private" {
            checked_add_to(&mut entry.private, 1, "private route")?;
        }
        for category in categories {
            checked_add_to(
                entry.categories.entry(category).or_default(),
                1,
                "route category",
            )?;
        }
    }

    let official_seen: BTreeSet<_> = invocations
        .iter()
        .filter(|(corpus, _)| corpus == "official-suite")
        .map(|(_, invocation)| invocation.clone())
        .collect();
    let conformance_seen: BTreeSet<_> = invocations
        .iter()
        .filter(|(corpus, _)| corpus == "committed-conformance")
        .map(|(_, invocation)| invocation.clone())
        .collect();
    if official_seen != official_invocations.keys().cloned().collect()
        || conformance_seen != conformance_invocations.keys().cloned().collect()
    {
        return Err(
            "routing rows do not exactly cover the two committed invocation sets".to_owned(),
        );
    }

    let mut total_invocations = 0usize;
    let mut total_sources = 0usize;
    let mut total_private = 0usize;
    let mut total_projected = 0usize;
    let mut total_observed = 0usize;
    for (id, corpus) in corpora {
        let actual = counts.remove(&id).unwrap_or_default();
        expect_usize(corpus, "invocation_count", actual.invocations)?;
        expect_usize(corpus, "source_count", actual.sources)?;
        expect_usize(corpus, "candidate_count", actual.candidates)?;
        expect_usize(corpus, "private_count", actual.private)?;
        expect_usize(corpus, "expected_rebuild_count", actual.private)?;
        for (category, field) in [
            ("collision", "collision_count"),
            ("unique-global-object", "unique_global_object_count"),
            ("declare-global", "declare_global_count"),
            ("namespace-umd", "namespace_umd_count"),
            ("classifier-uncertain", "classifier_uncertain_count"),
        ] {
            expect_usize(
                corpus,
                field,
                *actual.categories.get(category).unwrap_or(&0),
            )?;
        }
        checked_add_to(
            &mut total_invocations,
            actual.invocations,
            "total invocation",
        )?;
        checked_add_to(&mut total_sources, actual.sources, "total source")?;
        checked_add_to(&mut total_private, actual.private, "total private")?;
        expect_usize(corpus, "projected_suite_ms", actual.projected_ms)?;
        expect_usize(corpus, "observed_suite_ms", actual.observed_ms)?;
        checked_add_to(
            &mut total_projected,
            actual.projected_ms,
            "total projected time",
        )?;
        checked_add_to(
            &mut total_observed,
            actual.observed_ms,
            "total observed time",
        )?;
    }
    let summary = required_table(routing, "summary")?;
    expect_usize(summary, "invocation_count", total_invocations)?;
    expect_usize(summary, "source_count", total_sources)?;
    expect_usize(summary, "private_count", total_private)?;
    expect_usize(summary, "expected_rebuild_count", total_private)?;
    expect_usize(summary, "projected_suite_ms", total_projected)?;
    expect_usize(summary, "observed_suite_ms", total_observed)?;
    let fraction = private_fraction_ppm(total_private, total_invocations)?;
    expect_usize(summary, "private_fraction_ppm", fraction)?;

    let approval = required_table(routing, "approval")?;
    expect_string(approval, "decision", "APPROVED")?;
    validate_approval_identity(required_string(approval, "approver")?, "approver")?;
    validate_iso_date(required_string(approval, "date")?)?;
    routing_evidence_rows.sort_by(|left, right| {
        routing_corpus_rank(&left.corpus)
            .cmp(&routing_corpus_rank(&right.corpus))
            .then_with(|| left.invocation.cmp(&right.invocation))
    });
    let routing_evidence_sha = routing_evidence_manifest_sha256(
        &RoutingEvidenceMetadata {
            schema: required_usize(routing, "schema")?,
            status: required_string(routing, "status")?,
            profile_sha256: required_string(routing, "profile_sha256")?,
            checker_revision,
            classifier: required_string(routing, "classifier")?,
            candidate_collector: required_string(routing, "candidate_collector")?,
            official_manifest_sha256: &official_digest,
            conformance_manifest_sha256: &conformance_digest,
            official_source_manifest_sha256: &official_source_manifest_sha,
        },
        &routing_evidence_rows,
    );
    expect_string(approval, "evidence_manifest_sha256", &routing_evidence_sha)?;
    Ok(total_invocations > 0
        && total_projected > 0
        && total_observed > 0
        && within_approved_limit(fraction, APPROVED_ROUTING_PRIVATE_FRACTION_PPM)
        && within_approved_limit(total_projected, APPROVED_ROUTING_PROJECTED_SUITE_MS)
        && within_approved_limit(total_observed, APPROVED_ROUTING_OBSERVED_SUITE_MS)
        && matrix_is_code_approved(
            &conformance_digest,
            APPROVED_CONFORMANCE_INVOCATION_MANIFEST_SHA256,
        )
        && matrix_is_code_approved(
            &official_source_manifest_sha,
            APPROVED_OFFICIAL_SOURCE_MANIFEST_SHA256,
        )
        && matrix_is_code_approved(
            &routing_evidence_sha,
            APPROVED_ROUTING_EVIDENCE_MANIFEST_SHA256,
        ))
}

fn committed_official_invocations(
) -> Result<(BTreeMap<String, InvocationInput>, String, String), String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(OFFICIAL_SCOREBOARD_PATH);
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "cannot read official scoreboard {}: {error}",
            path.display()
        )
    })?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("official scoreboard is not UTF-8: {error}"))?;
    let mut ids = Vec::new();
    for line in text.lines().filter(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('#')
    }) {
        let id = line
            .rsplit('\t')
            .next()
            .ok_or_else(|| format!("invalid official scoreboard row {line:?}"))?;
        ids.push(id.to_owned());
    }
    if ids.len() != 874 {
        return Err(format!(
            "official scoreboard has {} ids, expected 874",
            ids.len()
        ));
    }
    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(OFFICIAL_CORPUS_DIR);
    let (sources, source_manifest_sha) = build_official_source_manifest(ids, |id| {
        fs::read(corpus_root.join(id))
            .map_err(|error| format!("cannot read pinned official source {id}: {error}"))
    })?;
    Ok((sources, sha256_hex(&bytes), source_manifest_sha))
}

fn build_official_source_manifest(
    ids: impl IntoIterator<Item = String>,
    mut read_source: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<(BTreeMap<String, InvocationInput>, String), String> {
    let mut sources = BTreeMap::new();
    for id in ids {
        let relative = Path::new(&id);
        if id.is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!("invalid official scoreboard id {id:?}"));
        }
        let source = read_source(&id)?;
        let input = InvocationInput {
            source_count: parse_units_source_count(
                std::str::from_utf8(&source)
                    .map_err(|error| format!("official source {id} is not UTF-8: {error}"))?,
            ),
            input_sha256: sha256_hex(&source),
        };
        if sources.insert(id.clone(), input).is_some() {
            return Err(format!("duplicate official scoreboard id {id:?}"));
        }
    }
    let mut hasher = Sha256::new();
    for (id, input) in &sources {
        update_framed(&mut hasher, id.as_bytes());
        update_framed(&mut hasher, input.input_sha256.as_bytes());
        update_framed(&mut hasher, input.source_count.to_string().as_bytes());
    }
    Ok((sources, format!("{:x}", hasher.finalize())))
}

fn parse_units_source_count(source: &str) -> usize {
    let mut units = 0usize;
    let mut current_named = false;
    let mut current_has_lines = false;
    for line in source.split('\n') {
        if let Some(name) = option_directive_name(line) {
            if name == "filename" {
                if current_named || current_has_lines {
                    units += 1;
                }
                current_named = true;
                current_has_lines = false;
            }
        } else {
            current_has_lines = true;
        }
    }
    if current_named || current_has_lines {
        units += 1;
    }
    units.max(1)
}

fn option_directive_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("//")?.trim_start();
    let rest = rest.strip_prefix('@')?;
    let name_end = rest
        .find(|character: char| !(character.is_alphanumeric() || character == '_'))
        .unwrap_or(rest.len());
    if name_end == 0 {
        return None;
    }
    let (name, rest) = rest.split_at(name_end);
    rest.trim_start().strip_prefix(':')?;
    Some(name.to_ascii_lowercase())
}

fn validate_conformance_invocation_manifest(document: &Document) -> Result<(), String> {
    committed_conformance_invocations(document).map(|_| ())
}

fn committed_conformance_invocations(
    document: &Document,
) -> Result<(BTreeMap<String, InvocationInput>, String), String> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = root(document)?;
    expect_string(manifest, "status", "UNAPPROVED")?;
    expect_string(manifest, "source_path", "tests/conformance.rs")?;
    let harness = fs::read(repo_root.join("tests/conformance.rs"))
        .map_err(|error| format!("cannot read tests/conformance.rs: {error}"))?;
    verify_sha256(
        &harness,
        required_string(manifest, "source_sha256")?,
        "conformance harness",
    )?;
    let mut invocations = BTreeMap::new();
    let mut manifest_hasher = Sha256::new();
    let mut total_sources = 0usize;
    for row in required_array(manifest, "invocation")? {
        let row = as_table(row, "conformance invocation")?;
        let id = required_string(row, "id")?;
        let kind = required_string(row, "kind")?;
        if !matches!(kind, "single" | "project") {
            return Err(format!("invalid conformance invocation kind {kind:?}"));
        }
        let source_entries = required_string_array(row, "source")?;
        if source_entries.is_empty() || (kind == "single" && source_entries.len() != 1) {
            return Err(format!(
                "invalid source count for conformance invocation {id}"
            ));
        }
        update_framed(&mut manifest_hasher, kind.as_bytes());
        update_framed(&mut manifest_hasher, id.as_bytes());
        let mut input_hasher = Sha256::new();
        let mut source_paths = BTreeSet::new();
        for entry in source_entries {
            let (path, expected_sha) = entry
                .split_once('\t')
                .ok_or_else(|| format!("invalid source binding in conformance invocation {id}"))?;
            let relative = Path::new(path);
            if relative.is_absolute()
                || !relative.starts_with("tests/cases")
                || relative
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
                || !source_paths.insert(path.to_owned())
            {
                return Err(format!("invalid source path {path:?} in invocation {id}"));
            }
            let bytes = fs::read(repo_root.join(relative))
                .map_err(|error| format!("cannot read conformance source {path}: {error}"))?;
            verify_sha256(&bytes, expected_sha, path)?;
            update_framed(&mut manifest_hasher, path.as_bytes());
            update_framed(&mut manifest_hasher, expected_sha.as_bytes());
            update_framed(&mut input_hasher, path.as_bytes());
            update_framed(&mut input_hasher, expected_sha.as_bytes());
        }
        if kind == "single" && source_paths.first().map(String::as_str) != Some(id) {
            return Err(format!(
                "single-file invocation id must equal its source path: {id}"
            ));
        }
        if kind == "project"
            && source_paths
                .iter()
                .any(|source| !Path::new(source).starts_with(id))
        {
            return Err(format!(
                "project invocation {id} contains an outside source"
            ));
        }
        total_sources += source_paths.len();
        if invocations
            .insert(
                id.to_owned(),
                InvocationInput {
                    source_count: source_paths.len(),
                    input_sha256: format!("{:x}", input_hasher.finalize()),
                },
            )
            .is_some()
        {
            return Err(format!("duplicate conformance invocation {id}"));
        }
    }
    expect_usize(manifest, "invocation_count", invocations.len())?;
    expect_usize(manifest, "source_count", total_sources)?;
    let digest = format!("{:x}", manifest_hasher.finalize());
    expect_string(manifest, "manifest_sha256", &digest)?;
    Ok((invocations, digest))
}

fn update_framed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(
        u64::try_from(bytes.len())
            .expect("manifest field length fits u64")
            .to_be_bytes(),
    );
    hasher.update(bytes);
}

fn update_routing_evidence_manifest(hasher: &mut Sha256, row: &RoutingEvidenceRecord) {
    for value in [
        row.corpus.as_str(),
        row.invocation.as_str(),
        row.input_sha256.as_str(),
        row.expected_route.as_str(),
        row.observed_route.as_str(),
    ] {
        update_framed(hasher, value.as_bytes());
    }
    update_manifest_usize(hasher, row.source_count);
    update_manifest_usize(hasher, row.collected_roots.len());
    for root in &row.collected_roots {
        update_framed(hasher, root.as_bytes());
    }
    update_manifest_usize(hasher, row.categories.len());
    for category in &row.categories {
        update_framed(hasher, category.as_bytes());
    }
    update_manifest_usize(hasher, row.candidate_count);
    update_manifest_usize(hasher, row.projected_ms);
    update_manifest_usize(hasher, row.observed_ms);
    for artifact in [&row.observed, &row.forced] {
        for value in [
            artifact.path.as_str(),
            artifact.file_sha256.as_str(),
            artifact.mode.as_str(),
            artifact.route.as_str(),
            artifact.semantic_output_sha256.as_str(),
        ] {
            update_framed(hasher, value.as_bytes());
        }
        update_manifest_usize(hasher, artifact.source_count);
        update_manifest_usize(hasher, artifact.collected_roots.len());
        for root in &artifact.collected_roots {
            update_framed(hasher, root.as_bytes());
        }
        update_manifest_usize(hasher, artifact.categories.len());
        for category in &artifact.categories {
            update_framed(hasher, category.as_bytes());
        }
        update_manifest_usize(hasher, artifact.candidate_count);
        update_manifest_usize(hasher, artifact.elapsed_ms);
    }
}

fn update_manifest_usize(hasher: &mut Sha256, value: usize) {
    hasher.update(
        u64::try_from(value)
            .expect("routing evidence value fits u64")
            .to_be_bytes(),
    );
}

fn routing_evidence_manifest_sha256(
    metadata: &RoutingEvidenceMetadata<'_>,
    rows: &[RoutingEvidenceRecord],
) -> String {
    let mut hasher = Sha256::new();
    update_framed(&mut hasher, b"typokat/wu0b/routing-evidence/v2");
    update_manifest_usize(&mut hasher, metadata.schema);
    for value in [
        metadata.status,
        metadata.profile_sha256,
        metadata.checker_revision,
        metadata.classifier,
        metadata.candidate_collector,
        metadata.official_manifest_sha256,
        metadata.conformance_manifest_sha256,
        metadata.official_source_manifest_sha256,
    ] {
        update_framed(&mut hasher, value.as_bytes());
    }
    let mut ordered: Vec<_> = rows.iter().collect();
    ordered.sort_by(|left, right| {
        routing_corpus_rank(&left.corpus)
            .cmp(&routing_corpus_rank(&right.corpus))
            .then_with(|| left.invocation.cmp(&right.invocation))
    });
    update_manifest_usize(&mut hasher, ordered.len());
    for row in ordered {
        update_routing_evidence_manifest(&mut hasher, row);
    }
    format!("{:x}", hasher.finalize())
}

fn routing_corpus_rank(corpus: &str) -> usize {
    ROUTING_CORPORA
        .iter()
        .position(|candidate| *candidate == corpus)
        .unwrap_or(usize::MAX)
}

fn route_measurement_valid(
    projected_ms: usize,
    observed_ms: usize,
    observed_output: &str,
    forced_private_output: &str,
) -> bool {
    projected_ms > 0
        && observed_ms > 0
        && is_lower_hex(observed_output, 64)
        && observed_output == forced_private_output
}

fn readiness_gate_facts(
    readiness: &Table,
    ledger_complete: bool,
    bridges_complete: bool,
    routing_complete_and_approved: bool,
) -> Result<GateFacts, String> {
    validate_lower_hex(required_string(readiness, "checker_revision")?, 40)?;
    validate_approval_identity(
        required_string(readiness, "benchmark_host")?,
        "benchmark host",
    )?;
    let protocol = required_table(readiness, "protocol")?;
    expect_string(protocol, "release_build_command", RELEASE_BUILD_COMMAND)?;
    expect_string(protocol, "release_probe_filter", RELEASE_PROBE_FILTER)?;
    let release_protocol = required_bool(protocol, "release_profile")?
        && required_bool(protocol, "ordinary_warm_filesystem_cache")?
        && required_usize(protocol, "cold_processes")? == 5
        && required_string(protocol, "memory_tool")? == "/usr/bin/time -v"
        && validate_lower_hex(required_string(protocol, "tiny_source_sha256")?, 64).is_ok();

    let phases = required_array(readiness, "phase_sample")?;
    let mut phase_processes = BTreeSet::new();
    let mut phases_complete = phases.len() == 5;
    for row in phases {
        let row = as_table(row, "phase sample")?;
        let process = required_usize(row, "process")?;
        phases_complete &= phase_processes.insert(process) && (1..=5).contains(&process);
        let measured = [
            "registry_validation_us",
            "parse_us",
            "bind_us",
            "reserve_fill_us",
            "publication_validation_us",
            "statement_check_us",
        ]
        .into_iter()
        .map(|key| required_usize(row, key))
        .collect::<Result<Vec<_>, _>>()?;
        let total = required_usize(row, "total_us")?;
        let measured_total = checked_sum_usize(&measured)
            .ok_or_else(|| "phase timing sum overflowed usize".to_owned())?;
        phases_complete &= measured.iter().all(|value| *value > 0) && total >= measured_total;
    }
    let cold = required_array(readiness, "cold_sample")?;
    let mut cold_processes = BTreeSet::new();
    let mut cold_under_limits = cold.len() == 5;
    for row in cold {
        let row = as_table(row, "cold sample")?;
        let process = required_usize(row, "process")?;
        cold_under_limits &= cold_processes.insert(process)
            && (1..=5).contains(&process)
            && cold_measurement_valid(
                required_usize(row, "wall_ms")?,
                required_usize(row, "max_rss_mib")?,
                required_usize(row, "exit_code")?,
                required_usize(row, "diagnostics")?,
                required_usize(row, "incompletes")?,
            );
    }

    let warm = required_table(readiness, "warm")?;
    expect_string(warm, "status", "COMPLETE")?;
    let baseline_samples = required_usize_array(warm, "delta_baseline_samples_us")?;
    let shared_samples = required_usize_array(warm, "shared_base_samples_us")?;
    let sample_count = required_usize(warm, "sample_count")?;
    if baseline_samples.len() != sample_count || shared_samples.len() != sample_count {
        return Err("warm raw sample arrays must match sample_count".to_owned());
    }
    let (computed_baseline_p50, computed_baseline_p95) = percentiles(&baseline_samples)
        .ok_or_else(|| "warm baseline samples must be nonzero".to_owned())?;
    let (computed_shared_p50, computed_shared_p95) = percentiles(&shared_samples)
        .ok_or_else(|| "warm shared samples must be nonzero".to_owned())?;
    let baseline_p50 = required_usize(warm, "delta_baseline_p50_us")?;
    let baseline_p95 = required_usize(warm, "delta_baseline_p95_us")?;
    let shared_p50 = required_usize(warm, "shared_base_p50_us")?;
    let shared_p95 = required_usize(warm, "shared_base_p95_us")?;
    expect_usize(warm, "delta_baseline_p50_us", computed_baseline_p50)?;
    expect_usize(warm, "delta_baseline_p95_us", computed_baseline_p95)?;
    expect_usize(warm, "shared_base_p50_us", computed_shared_p50)?;
    expect_usize(warm, "shared_base_p95_us", computed_shared_p95)?;
    let warm_under_ratio = sample_count > 0
        && required_usize(warm, "max_rss_mib")? > 0
        && required_usize(warm, "max_rss_mib")? <= 512
        && within_125_percent(shared_p50, baseline_p50)
        && within_125_percent(shared_p95, baseline_p95)
        && validate_pointer_identity(required_array(warm, "pointer_identity")?)?;
    let warm_zero_library_work = [
        "library_parse_count",
        "library_bind_count",
        "library_check_count",
        "base_sized_clone_count",
    ]
    .into_iter()
    .all(|key| required_usize(warm, key) == Ok(0))
        && required_bool(warm, "per_check_allocation_independent")?;

    let private = required_table(readiness, "private")?;
    expect_string(private, "status", "COMPLETE")?;
    let fallback_wall = required_usize(private, "fallback_wall_ms")?;
    let fallback_rss = required_usize(private, "fallback_max_rss_mib")?;
    let private_under_limits = fallback_wall > 0
        && within_approved_limit(fallback_wall, APPROVED_PRIVATE_FALLBACK_WALL_MS)
        && fallback_rss > 0
        && fallback_rss <= 512
        && required_bool(private, "shared_base_retained")?;
    let fanout_linear = required_usize(private, "fanout_linear_wall_ms")?;
    let fanout_observed = required_usize(private, "fanout_observed_wall_ms")?;
    let fanout_workers = required_usize(private, "fanout_workers")?;
    let fanout_rss = required_usize(private, "fanout_max_rss_mib")?;
    let fanout_bounded = APPROVED_FANOUT_WORKERS == Some(fanout_workers)
        && required_bool(private, "fanout_single_permit")?
        && fanout_measurement_valid(
            fallback_wall,
            fallback_rss,
            fanout_workers,
            fanout_rss,
            fanout_linear,
            fanout_observed,
        );

    let freeze = required_table(readiness, "freeze")?;
    expect_string(freeze, "status", "COMPLETE")?;
    let frozen_ast_free_send_sync = [
        "ast_free",
        "send_sync_static",
        "all_reserved_terminal",
        "no_allocator",
        "no_construction_drafts",
        "no_pass_local_cache",
        "root_index_complete",
        "universe_local_identities",
    ]
    .into_iter()
    .all(|key| required_bool(freeze, key) == Ok(true));

    Ok(GateFacts {
        cold_samples: cold.len(),
        cold_under_limits,
        phases_complete,
        warm_under_ratio,
        warm_zero_library_work,
        private_under_limits,
        fanout_bounded,
        frozen_ast_free_send_sync,
        ledger_complete,
        bridges_complete,
        routing_complete_and_approved,
        release_protocol,
        code_approvals_complete: APPROVED_BENCHMARK_SAMPLE_COUNT == Some(sample_count)
            && APPROVED_PRIVATE_FALLBACK_WALL_MS.is_some()
            && APPROVED_FANOUT_WORKERS.is_some(),
    })
}

fn recompute_verdict(facts: &GateFacts) -> Verdict {
    if facts.cold_samples == 5
        && facts.cold_under_limits
        && facts.phases_complete
        && facts.warm_under_ratio
        && facts.warm_zero_library_work
        && facts.private_under_limits
        && facts.fanout_bounded
        && facts.frozen_ast_free_send_sync
        && facts.ledger_complete
        && facts.bridges_complete
        && facts.routing_complete_and_approved
        && facts.release_protocol
        && facts.code_approvals_complete
    {
        Verdict::Go
    } else {
        Verdict::NoGo
    }
}

fn percentiles(samples: &[usize]) -> Option<(usize, usize)> {
    if samples.is_empty() || samples.contains(&0) {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let nearest_rank = |percent: usize| {
        let rank = (sorted.len() * percent).div_ceil(100);
        sorted[rank.saturating_sub(1)]
    };
    Some((nearest_rank(50), nearest_rank(95)))
}

fn within_125_percent(observed: usize, baseline: usize) -> bool {
    observed > 0
        && baseline > 0
        && u128::try_from(observed)
            .expect("usize fits u128")
            .checked_mul(4)
            .is_some_and(|observed| {
                u128::try_from(baseline)
                    .expect("usize fits u128")
                    .checked_mul(5)
                    .is_some_and(|baseline| observed <= baseline)
            })
}

fn checked_sum_usize(values: &[usize]) -> Option<usize> {
    values
        .iter()
        .try_fold(0usize, |sum, value| sum.checked_add(*value))
}

fn checked_add_to(target: &mut usize, value: usize, label: &str) -> Result<(), String> {
    *target = target
        .checked_add(value)
        .ok_or_else(|| format!("{label} aggregate overflowed usize"))?;
    Ok(())
}

fn private_fraction_ppm(private: usize, total: usize) -> Result<usize, String> {
    if total == 0 {
        return Ok(0);
    }
    let numerator = u128::try_from(private)
        .expect("usize fits u128")
        .checked_mul(1_000_000)
        .ok_or_else(|| "private fraction numerator overflowed u128".to_owned())?;
    let denominator = u128::try_from(total).expect("usize fits u128");
    usize::try_from(numerator.div_ceil(denominator))
        .map_err(|_| "private fraction does not fit usize".to_owned())
}

fn cold_measurement_valid(
    wall_ms: usize,
    max_rss_mib: usize,
    exit_code: usize,
    diagnostics: usize,
    incompletes: usize,
) -> bool {
    wall_ms > 0
        && wall_ms <= 5_000
        && max_rss_mib > 0
        && max_rss_mib <= 512
        && exit_code == 0
        && diagnostics == 0
        && incompletes == 0
}

fn fanout_measurement_valid(
    fallback_wall_ms: usize,
    fallback_rss_mib: usize,
    workers: usize,
    fanout_rss_mib: usize,
    recorded_linear_wall_ms: usize,
    observed_wall_ms: usize,
) -> bool {
    fallback_wall_ms > 0
        && fallback_rss_mib > 0
        && workers > 1
        && fanout_rss_mib > 0
        && fanout_rss_mib <= fallback_rss_mib
        && fallback_wall_ms.checked_mul(workers) == Some(recorded_linear_wall_ms)
        && within_125_percent(observed_wall_ms, recorded_linear_wall_ms)
}

fn within_approved_limit(observed: usize, approved: Option<usize>) -> bool {
    observed > 0 && approved.is_some_and(|limit| observed <= limit)
}

fn validate_pointer_identity(rows: &[Value]) -> Result<bool, String> {
    if rows.len() != 3 {
        return Err("pointer identity evidence must contain exactly 1/2/32 checks".to_owned());
    }
    let mut checks = BTreeSet::new();
    let mut base_digest = None;
    let mut shared_address: Option<String> = None;
    for row in rows {
        let row = as_table(row, "pointer identity")?;
        let count = required_usize(row, "checks")?;
        if !checks.insert(count) || ![1, 2, 32].contains(&count) {
            return Err(format!("invalid pointer identity check count {count}"));
        }
        let addresses = required_string_array(row, "addresses")?;
        if addresses.len() != count {
            return Err(format!(
                "pointer evidence for {count} checks has {} addresses",
                addresses.len()
            ));
        }
        for address in addresses {
            if !valid_pointer_address(&address) {
                return Err(format!("invalid base pointer address {address:?}"));
            }
            if let Some(expected) = &shared_address {
                if address != expected.as_str() {
                    return Err("not all 35 pointer observations identify the same base".to_owned());
                }
            } else {
                shared_address = Some(address);
            }
        }
        let digest = required_string(row, "base_digest")?;
        validate_lower_hex(digest, 64)?;
        if let Some(expected) = base_digest {
            if digest != expected {
                return Err("pointer evidence rows identify different shared bases".to_owned());
            }
        } else {
            base_digest = Some(digest);
        }
    }
    Ok(checks == [1, 2, 32].into_iter().collect())
}

fn valid_pointer_address(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .is_some_and(|hex| !hex.is_empty() && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn reject_sentinels(value: &Value, label: &str) -> Result<(), String> {
    match value {
        Value::String(text)
            if ["PENDING", "FABRICATED", "TODO"]
                .iter()
                .any(|sentinel| text.to_ascii_uppercase().contains(sentinel)) =>
        {
            Err(format!(
                "{label} contains forbidden evidence sentinel {text:?}"
            ))
        }
        Value::Array(values) => {
            for value in values {
                reject_sentinels(value, label)?;
            }
            Ok(())
        }
        Value::Table(table) => {
            for value in table.values() {
                reject_sentinels(value, label)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_approval_identity(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.eq_ignore_ascii_case("unknown") {
        return Err(format!("{label} must be concrete"));
    }
    Ok(())
}

fn validate_iso_date(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(format!("approval date must be YYYY-MM-DD, got {value:?}"));
    }
    Ok(())
}

fn verify_sha256(bytes: &[u8], expected: &str, label: &str) -> Result<(), String> {
    validate_lower_hex(expected, 64)?;
    let actual = sha256_hex(bytes);
    if actual != expected {
        return Err(format!(
            "{label} SHA-256 mismatch: expected {expected}, found {actual}"
        ));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_lower_hex(value: &str, len: usize) -> Result<(), String> {
    if !is_lower_hex(value, len) {
        return Err(format!(
            "expected {len} lowercase hex characters, got {value:?}"
        ));
    }
    Ok(())
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn exact_keys(table: &Table, expected: &[&str], label: &str) -> Result<(), String> {
    let actual: BTreeSet<_> = table.keys().map(String::as_str).collect();
    let expected: BTreeSet<_> = expected.iter().copied().collect();
    if actual != expected {
        let missing: Vec<_> = expected.difference(&actual).copied().collect();
        let extra: Vec<_> = actual.difference(&expected).copied().collect();
        return Err(format!(
            "{label} keys differ; missing={missing:?}, extra={extra:?}"
        ));
    }
    Ok(())
}

fn root(document: &Document) -> Result<&Table, String> {
    document
        .value
        .as_table()
        .ok_or_else(|| "TOML root must be a table".to_owned())
}

fn root_mut(document: &mut Document) -> Result<&mut Table, String> {
    document
        .value
        .as_table_mut()
        .ok_or_else(|| "TOML root must be a table".to_owned())
}

fn as_table<'a>(value: &'a Value, label: &str) -> Result<&'a Table, String> {
    value
        .as_table()
        .ok_or_else(|| format!("{label} must be a table"))
}

fn required_value<'a>(table: &'a Table, key: &str) -> Result<&'a Value, String> {
    table.get(key).ok_or_else(|| format!("missing key {key:?}"))
}

fn required_table<'a>(table: &'a Table, key: &str) -> Result<&'a Table, String> {
    as_table(required_value(table, key)?, key)
}

fn child_table_mut<'a>(table: &'a mut Table, key: &str) -> Result<&'a mut Table, String> {
    table
        .get_mut(key)
        .and_then(Value::as_table_mut)
        .ok_or_else(|| format!("{key:?} must be a table"))
}

fn required_array<'a>(table: &'a Table, key: &str) -> Result<&'a [Value], String> {
    required_value(table, key)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{key:?} must be an array"))
}

fn required_string<'a>(table: &'a Table, key: &str) -> Result<&'a str, String> {
    required_value(table, key)?
        .as_str()
        .ok_or_else(|| format!("{key:?} must be a string"))
}

fn required_string_array(table: &Table, key: &str) -> Result<Vec<String>, String> {
    required_array(table, key)?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{key:?} entries must be strings"))
        })
        .collect()
}

fn required_usize_array(table: &Table, key: &str) -> Result<Vec<usize>, String> {
    required_array(table, key)?
        .iter()
        .map(|value| {
            let value = value
                .as_integer()
                .ok_or_else(|| format!("{key:?} entries must be integers"))?;
            usize::try_from(value)
                .map_err(|_| format!("{key:?} entries must be non-negative usizes"))
        })
        .collect()
}

fn required_usize(table: &Table, key: &str) -> Result<usize, String> {
    let value = required_value(table, key)?
        .as_integer()
        .ok_or_else(|| format!("{key:?} must be an integer"))?;
    usize::try_from(value).map_err(|_| format!("{key:?} must be a non-negative usize"))
}

fn required_bool(table: &Table, key: &str) -> Result<bool, String> {
    required_value(table, key)?
        .as_bool()
        .ok_or_else(|| format!("{key:?} must be a boolean"))
}

fn expect_string(table: &Table, key: &str, expected: &str) -> Result<(), String> {
    let actual = required_string(table, key)?;
    if actual != expected {
        return Err(format!("{key:?}: expected {expected:?}, found {actual:?}"));
    }
    Ok(())
}

fn expect_equal_string(actual: &str, expected: &str, label: &str) -> Result<(), String> {
    if actual != expected {
        return Err(format!("{label}: expected {expected:?}, found {actual:?}"));
    }
    Ok(())
}

fn expect_usize(table: &Table, key: &str, expected: usize) -> Result<(), String> {
    let actual = required_usize(table, key)?;
    if actual != expected {
        return Err(format!("{key:?}: expected {expected}, found {actual}"));
    }
    Ok(())
}

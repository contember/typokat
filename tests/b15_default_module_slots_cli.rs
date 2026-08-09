//! Disabled oracle and RED contract for distinct default module slots.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use typokat::diagnostics::{self, DiagnosticFormat, Severity};
use typokat::driver::{check_project, check_project_once};
use typokat::frontend::FileInput;

const CONTRACT: &str = include_str!("cases/b15_default_module_slots/contract.json");
const B72_CONTRACT: &str = include_str!("cases/b72_bundler_project_tracer/contract.json");
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn from_case(case: &Value, order: &str) -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let case_id = case["id"].as_str().expect("case id");
        let root = std::env::temp_dir().join(format!(
            "typokat-b15-default-{case_id}-{order}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create default-slot temporary project");
        for file in case["files"].as_array().expect("case files") {
            let relative = file["path"].as_str().expect("file path");
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create default-slot source parent");
            }
            std::fs::write(&path, file["text"].as_str().expect("file text"))
                .expect("write default-slot source");
        }
        let files = oracle_run(case, order)["files"]
            .as_array()
            .expect("oracle files");
        let config = json!({
            "compilerOptions": {
                "strict": true,
                "noEmit": true,
                "module": "ESNext",
                "moduleResolution": "Bundler"
            },
            "files": files
        });
        std::fs::write(
            root.join("tsconfig.json"),
            serde_json::to_string_pretty(&config).expect("serialize default-slot config"),
        )
        .expect("write default-slot config");
        Self { root }
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases/b15_default_module_slots")
}

fn contract() -> Value {
    serde_json::from_str(CONTRACT).expect("default-slot contract is valid JSON")
}

fn case_by_id<'a>(contract: &'a Value, id: &str) -> &'a Value {
    contract["cases"]
        .as_array()
        .expect("contract cases")
        .iter()
        .find(|case| case["id"] == id)
        .unwrap_or_else(|| panic!("missing default-slot case {id}"))
}

fn oracle_run<'a>(case: &'a Value, order: &str) -> &'a Value {
    case["oracle_runs"]
        .as_array()
        .expect("oracle runs")
        .iter()
        .find(|run| run["order"] == order)
        .unwrap_or_else(|| panic!("missing {order} oracle run"))
}

fn project_args(project: &Path) -> Vec<String> {
    vec![
        "check".to_owned(),
        "--format".to_owned(),
        "compact".to_owned(),
        "--project-summary".to_owned(),
        "json".to_owned(),
        project.to_string_lossy().into_owned(),
    ]
}

fn run_typokat(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typokat"))
        .args(args)
        .output()
        .expect("run typokat default-slot contract")
}

fn parse_summary(output: &Output, label: &str) -> Value {
    assert!(output.stdout.ends_with(b"\n"), "{label} JSONL newline");
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "{label} emits one summary"
    );
    serde_json::from_slice(&output.stdout).expect("default-slot summary is JSON")
}

fn normalized_oracle_lines(output: &Output, case_dir: &Path, case_id: &str) -> Vec<String> {
    assert!(output.stderr.is_empty(), "{case_id} tsc stderr");
    String::from_utf8_lossy(&output.stdout)
        .replace(
            &case_dir.to_string_lossy().into_owned(),
            &format!("<case:{case_id}>"),
        )
        .lines()
        .map(str::to_owned)
        .collect()
}

fn raw_file_paths(root: &Path) -> Vec<String> {
    fn visit(root: &Path, directory: &Path, paths: &mut Vec<String>) {
        for entry in std::fs::read_dir(directory).expect("read raw fixture directory") {
            let entry = entry.expect("raw fixture entry");
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, paths);
            } else {
                paths.push(
                    path.strip_prefix(root)
                        .expect("raw fixture stays under case root")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut paths = Vec::new();
    visit(root, root, &mut paths);
    paths.sort();
    paths
}

#[test]
fn contract_is_machine_complete_and_matches_raw_fixtures() {
    let contract = contract();
    assert_eq!(contract["schema"], 1);
    assert_eq!(contract["oracle"]["version"], "Version 6.0.3");
    assert_eq!(contract["summary"]["case_count"], 33);
    assert_eq!(contract["summary"]["run_count"], 66);
    assert_eq!(contract["summary"]["root_order_byte_identical_count"], 33);
    assert_eq!(contract["summary"]["admitted_case_count"], 18);
    assert_eq!(contract["summary"]["deferred_control_case_count"], 15);

    let admitted = [
        "01_named_class",
        "02_anonymous_class",
        "03_named_function",
        "04_anonymous_function",
        "05_literal_expression",
        "06_object_expression",
        "07_arrow_expression",
        "07b_expression_owned_diagnostic",
        "08_identifier_value",
        "09_identifier_class",
        "09b_identifier_function",
        "10_type_only_default_import",
        "10b_type_only_value_only_default",
        "11_missing_physical_target",
        "12_missing_resolved_default",
        "13_default_named_isolation",
        "14_value_only_wrong_type",
        "24c_identifier_interface",
    ];
    let namespace_controls = [
        "15_named_default_class_namespace",
        "16_named_default_function_namespace",
        "17_namespace_identifier_default",
        "18_class_namespace_identifier_default",
        "19_function_namespace_identifier_default",
        "24d_identifier_enum_unsupported",
    ];
    let syntax_controls = [
        "14b_default_interface_unsupported",
        "20_local_export_list_default",
        "21_named_default_import_syntax",
        "22a_source_default_as_named",
        "22b_source_named_as_default",
        "22c_source_default_as_default",
        "23_mixed_default_named_import",
        "24_mixed_default_namespace_import",
    ];
    assert_eq!(contract["classifications"]["admitted"], json!(admitted));
    assert_eq!(
        contract["classifications"]["namespace_provenance_controls"],
        json!(namespace_controls)
    );
    assert_eq!(
        contract["classifications"]["deferred_syntax_controls"],
        json!(syntax_controls)
    );
    assert_eq!(
        contract["classifications"]["duplicate_default_controls"],
        json!(["25_duplicate_default_matrix"])
    );

    let mut partition = BTreeMap::new();
    for (classification, ids) in [
        ("admitted", admitted.as_slice()),
        (
            "namespace-provenance-control",
            namespace_controls.as_slice(),
        ),
        ("deferred-syntax-control", syntax_controls.as_slice()),
        (
            "duplicate-default-control",
            ["25_duplicate_default_matrix"].as_slice(),
        ),
    ] {
        for id in ids {
            assert_eq!(
                partition.insert(*id, classification),
                None,
                "duplicate {id}"
            );
        }
    }
    let contract_ids = contract["cases"]
        .as_array()
        .expect("contract cases")
        .iter()
        .map(|case| case["id"].as_str().expect("case id"))
        .collect::<Vec<_>>();
    assert_eq!(contract_ids, partition.keys().copied().collect::<Vec<_>>());

    let root = corpus_root();
    let mut raw_case_ids = Vec::new();
    let mut raw_root_files = Vec::new();
    for entry in std::fs::read_dir(&root).expect("read default-slot corpus") {
        let entry = entry.expect("default-slot root entry");
        if entry.path().is_dir() {
            raw_case_ids.push(entry.file_name().to_string_lossy().into_owned());
        } else {
            raw_root_files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    raw_case_ids.sort();
    raw_root_files.sort();
    assert_eq!(raw_case_ids, contract_ids);
    assert_eq!(raw_root_files, ["contract.json"]);

    for case in contract["cases"].as_array().expect("contract cases") {
        let id = case["id"].as_str().expect("case id");
        let project = root.join(id);
        assert_eq!(case["classification"], partition[id], "{id} classification");
        let mut expected_paths = case["files"]
            .as_array()
            .expect("case files")
            .iter()
            .map(|file| file["path"].as_str().expect("file path").to_owned())
            .collect::<Vec<_>>();
        expected_paths.extend([
            "tsconfig.json".to_owned(),
            "tsconfig.normal.json".to_owned(),
            "tsconfig.reverse.json".to_owned(),
        ]);
        expected_paths.sort();
        assert_eq!(
            raw_file_paths(&project),
            expected_paths,
            "{id} raw file set"
        );
        for file in case["files"].as_array().expect("case files") {
            let relative = file["path"].as_str().expect("file path");
            assert_eq!(
                std::fs::read_to_string(project.join(relative)).expect("read raw fixture"),
                file["text"].as_str().expect("contract file text"),
                "{id}/{relative}"
            );
        }
        let compiler_options = json!({
            "strict": true,
            "noEmit": true,
            "module": "ESNext",
            "moduleResolution": "Bundler"
        });
        let normal_files = oracle_run(case, "normal")["files"]
            .as_array()
            .expect("normal oracle files")
            .clone();
        let mut reversed_files = normal_files.clone();
        reversed_files.reverse();
        assert_eq!(oracle_run(case, "reverse")["files"], json!(reversed_files));
        for (name, order) in [
            ("tsconfig.json", "normal"),
            ("tsconfig.normal.json", "normal"),
            ("tsconfig.reverse.json", "reverse"),
        ] {
            let config: Value = serde_json::from_str(
                &std::fs::read_to_string(project.join(name)).expect("read oracle config"),
            )
            .expect("oracle config is JSON");
            assert_eq!(
                config,
                json!({
                    "compilerOptions": compiler_options,
                    "files": oracle_run(case, order)["files"]
                }),
                "{id}/{name}"
            );
            assert_eq!(oracle_run(case, order)["stderr"], "", "{id}/{order} stderr");
        }
        assert_eq!(
            oracle_run(case, "normal")["ordered_normalized_diagnostics"],
            oracle_run(case, "reverse")["ordered_normalized_diagnostics"],
            "{id} oracle root order"
        );
        assert_eq!(case["root_order_byte_identical"], true, "{id} order claim");

        let post = &case["typokat_after_wu6"];
        let summary = &post["summary"];
        for key in [
            "schema",
            "profile",
            "config",
            "roots",
            "files",
            "resolutions",
            "project_notices",
            "parse_errors",
            "incomplete",
            "diagnostics",
        ] {
            assert!(!summary[key].is_null(), "{id} missing post-WU6 {key}");
        }
        assert!(post["stderr"].is_string(), "{id} missing frozen stderr");
    }
}

#[test]
fn duplicate_contract_covers_every_real_producer_pair_and_reverse_order() {
    let contract = contract();
    let matrix = &contract["duplicate_default_matrix"];
    assert_eq!(
        matrix["producer_kinds"]
            .as_object()
            .expect("producer kinds")
            .len(),
        5
    );
    assert_eq!(
        matrix["unordered_pairs"]
            .as_array()
            .expect("unordered pairs")
            .len(),
        15
    );
    assert_eq!(
        matrix["reverse_order_controls"]
            .as_array()
            .expect("reverse pairs")
            .len(),
        10
    );
    assert_eq!(
        matrix["ordered_pairs"]
            .as_array()
            .expect("ordered pairs")
            .len(),
        25
    );

    let case = case_by_id(&contract, "25_duplicate_default_matrix");
    let fixture_paths = case["files"]
        .as_array()
        .expect("duplicate files")
        .iter()
        .map(|file| file["path"].as_str().expect("duplicate path"))
        .filter(|path| path.starts_with("pair_"))
        .collect::<Vec<_>>();
    assert_eq!(fixture_paths.len(), 25);
    for pair in matrix["ordered_pairs"].as_array().expect("ordered pairs") {
        let path = format!("pair_{}.ts", pair.as_str().expect("pair").to_lowercase());
        assert!(fixture_paths.contains(&path.as_str()), "missing {path}");
    }
    let canonical_pairs = [
        "CC", "CE", "CF", "CL", "CR", "EC", "EE", "EF", "EL", "ER", "FC", "FE", "FF", "FL", "FR",
        "LC", "LE", "LF", "LL", "LR", "RC", "RE", "RF", "RL", "RR",
    ];
    let producer = |kind| match kind {
        'C' => "direct-class",
        'F' => "direct-function",
        'E' => "default-expression",
        'L' => "local-export-list-default",
        'R' => "source-export-to-default",
        other => panic!("unknown duplicate producer {other}"),
    };
    let expected_notices = canonical_pairs
        .iter()
        .map(|pair| {
            let mut kinds = pair.chars();
            let first = kinds.next().expect("first producer");
            let second = kinds.next().expect("second producer");
            assert!(kinds.next().is_none(), "two producer kinds");
            format!(
                "unsupported-duplicate-default-export pair_{}.ts producers={},{}",
                pair.to_lowercase(),
                producer(first),
                producer(second)
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        case["typokat_after_wu6"]["summary"]["project_notices"],
        json!(expected_notices),
        "duplicate notices use canonical path order"
    );
}

#[test]
fn frozen_b72_module_boundaries_are_referenced_not_copied() {
    let contract = contract();
    let b72: Value = serde_json::from_str(B72_CONTRACT).expect("B72 contract is JSON");
    let expected = [
        (
            "unsupported_star_reexport",
            "unsupported-module-form star-reexport barrel.ts:1:1 ./source.js",
        ),
        (
            "unsupported_namespace_import",
            "unsupported-module-form namespace-import main.ts:1:1 ./source.js",
        ),
        (
            "unsupported_namespace_reexport",
            "unsupported-module-form namespace-reexport barrel.ts:1:1 ./source.js",
        ),
        (
            "unsupported_cycle",
            "unsupported-module-cycle a.ts -> b.ts -> a.ts",
        ),
        (
            "unsupported_bare_specifier",
            "unsupported-module-specifier bare main.ts:1:1 fixture-package",
        ),
        (
            "unsupported_export_assignment",
            "unsupported-module-form export-assignment main.ts:2:1",
        ),
        (
            "unsupported_import_equals",
            "unsupported-module-form import-equals main.ts:1:1 ./source.js",
        ),
    ];
    let expected_ids = expected.map(|(id, _)| id);
    assert_eq!(
        contract["frozen_b72_contract"]["case_ids"],
        json!(expected_ids),
        "exact frozen B72 case identities"
    );
    for (id, notice) in expected {
        let case = b72["cases"]
            .as_array()
            .expect("B72 cases")
            .iter()
            .find(|case| case["id"] == id)
            .unwrap_or_else(|| panic!("missing B72 case {id}"));
        assert_eq!(
            case["typokat"]["summary"]["project_notices"],
            json!([notice]),
            "{id}"
        );
    }
}

#[test]
#[ignore = "requires the pinned local TypeScript 6.0.3 binary"]
fn pinned_tsc_oracle_matches_every_fixture_in_both_orders() {
    let contract = contract();
    let binary = contract["oracle"]["binary"]
        .as_str()
        .expect("oracle binary");
    let version = Command::new(binary)
        .arg("--version")
        .output()
        .expect("run tsc version");
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "Version 6.0.3"
    );

    for case in contract["cases"].as_array().expect("contract cases") {
        let id = case["id"].as_str().expect("case id");
        let project = corpus_root().join(id);
        for order in ["normal", "reverse"] {
            let output = Command::new(binary)
                .current_dir(&project)
                .args([
                    "--pretty",
                    "false",
                    "--strict",
                    "--noEmit",
                    "--module",
                    "esnext",
                    "--moduleResolution",
                    "bundler",
                    "-p",
                    &format!("tsconfig.{order}.json"),
                ])
                .output()
                .expect("run pinned tsc oracle");
            let expected = oracle_run(case, order);
            assert_eq!(
                output.status.code(),
                expected["exit"].as_i64().map(|code| code as i32),
                "{id}/{order} exit"
            );
            assert_eq!(
                normalized_oracle_lines(&output, &project, id),
                expected["ordered_normalized_diagnostics"]
                    .as_array()
                    .expect("oracle diagnostics")
                    .iter()
                    .map(|line| line.as_str().expect("oracle line").to_owned())
                    .collect::<Vec<_>>(),
                "{id}/{order} diagnostics"
            );
        }
    }
}

#[test]
#[ignore = "pre-WU6 RED proof; intentionally becomes obsolete at cutover"]
fn current_production_route_is_red_only_through_default_notices() {
    let contract = contract();
    for id in contract["classifications"]["admitted"]
        .as_array()
        .expect("admitted ids")
    {
        let id = id.as_str().expect("admitted id");
        let case = case_by_id(&contract, id);
        let mut outputs = Vec::new();
        for order in ["normal", "reverse"] {
            let project = TempProject::from_case(case, order);
            let output = run_typokat(&project_args(&project.root));
            assert_eq!(output.status.code(), Some(3), "{id}/{order} exit");
            let summary = parse_summary(&output, &format!("{id}/{order}"));
            assert!(
                summary["diagnostics"]
                    .as_array()
                    .expect("diagnostics")
                    .is_empty(),
                "{id}/{order}"
            );
            assert!(
                summary["incomplete"]
                    .as_array()
                    .expect("incomplete")
                    .is_empty(),
                "{id}/{order}"
            );
            assert!(
                summary["parse_errors"]
                    .as_array()
                    .expect("parse errors")
                    .is_empty(),
                "{id}/{order}"
            );
            let notices = summary["project_notices"].as_array().expect("notices");
            assert!(!notices.is_empty(), "{id}/{order} has a default notice");
            assert!(
                notices.iter().all(|notice| {
                    notice.as_str().is_some_and(|notice| {
                        notice.starts_with("unsupported-module-form default-import")
                            || notice.starts_with("unsupported-module-form default-export")
                    })
                }),
                "{id}/{order} has only frozen default notices: {notices:?}"
            );
            outputs.push((output.stdout, output.stderr));
        }
        assert_eq!(outputs[0], outputs[1], "{id} current root-order bytes");
    }
}

fn assert_post_wu6_public_inputs(case: &Value, order: &str) -> (Vec<u8>, Vec<u8>) {
    let id = case["id"].as_str().expect("case id");
    let expected = &case["typokat_after_wu6"];
    let expected_stderr = expected["stderr"].as_str().expect("frozen stderr");
    let project = TempProject::from_case(case, order);
    let config = project.root.join("tsconfig.json");
    let mut outputs = Vec::new();
    for (invocation, input) in [
        ("directory", project.root.as_path()),
        ("config", config.as_path()),
    ] {
        let output = run_typokat(&project_args(input));
        let label = format!("{id}/{order}/{invocation}");
        assert_eq!(
            output.status.code(),
            expected["exit"].as_i64().map(|code| code as i32),
            "{label} exit; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            parse_summary(&output, &label),
            expected["summary"],
            "{label} summary"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            expected_stderr,
            "{label} stderr"
        );
        outputs.push((output.stdout, output.stderr));
    }
    assert_eq!(outputs[0], outputs[1], "{id}/{order} public input bytes");
    outputs.remove(0)
}

#[test]
#[ignore = "RED until WU6 atomically activates default module slots"]
fn post_wu6_admitted_projects_match_exact_summaries_in_both_orders() {
    let contract = contract();
    for id in contract["classifications"]["admitted"]
        .as_array()
        .expect("admitted ids")
    {
        let id = id.as_str().expect("admitted id");
        let case = case_by_id(&contract, id);
        let mut outputs = Vec::new();
        for order in ["normal", "reverse"] {
            outputs.push(assert_post_wu6_public_inputs(case, order));
        }
        assert_eq!(outputs[0], outputs[1], "{id} post-WU6 root-order bytes");
    }
}

#[test]
#[ignore = "RED until WU6 activates exact fail-closed default controls"]
fn post_wu6_deferred_forms_keep_exact_non_clean_identities() {
    let contract = contract();
    for case in contract["cases"].as_array().expect("contract cases") {
        if case["classification"] == "admitted" {
            continue;
        }
        let id = case["id"].as_str().expect("case id");
        let mut outputs = Vec::new();
        for order in ["normal", "reverse"] {
            outputs.push(assert_post_wu6_public_inputs(case, order));
        }
        assert_eq!(outputs[0], outputs[1], "{id} deferred root-order bytes");
    }
}

fn route_inputs(project: &Path, names: &[&str]) -> Vec<FileInput> {
    names
        .iter()
        .map(|name| FileInput {
            name: (*name).to_owned(),
            source: std::fs::read_to_string(project.join(name)).expect("read route fixture"),
        })
        .collect()
}

fn assert_frozen_route_report(report: &typokat::driver::FileReport, name: &str) {
    assert_eq!(report.name, name);
    assert!(report.output.parse_errors.is_empty(), "{name} parse errors");
    let incomplete = report
        .output
        .incomplete
        .iter()
        .map(|item| {
            (
                item.id.as_str(),
                item.span.start,
                item.span.end,
                item.context.as_str(),
            )
        })
        .collect::<Vec<_>>();
    let diagnostics = report
        .output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code.as_str(),
                diagnostic.span.start,
                diagnostic.span.end,
                diagnostic.message.as_str(),
                diagnostic.severity,
            )
        })
        .collect::<Vec<_>>();
    assert!(
        report
            .output
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.elaboration().is_empty()),
        "{name} diagnostic elaboration"
    );
    let mut rendered = Vec::new();
    diagnostics::render_to_writer_with_format(
        &mut rendered,
        &report.name,
        &report.source,
        &report.output.diagnostics,
        DiagnosticFormat::Compact,
    )
    .expect("render frozen route diagnostics");
    diagnostics::render_incomplete_to_writer_with_format(
        &mut rendered,
        &report.name,
        &report.source,
        &report.output.incomplete,
        DiagnosticFormat::Compact,
    )
    .expect("render frozen route incomplete");
    match name {
        "source.ts" => {
            assert_eq!(
                report.source,
                "export default class Widget { value: number = 1; }\n\
                 const local: Widget = new Widget();\n\
                 export const localValue: number = local.value;\n"
            );
            assert_eq!(
                incomplete,
                [(
                    "decl/export-default/self",
                    0,
                    50,
                    "export default not modeled"
                )]
            );
            assert_eq!(
                diagnostics,
                [
                    (
                        "TK2304",
                        64,
                        70,
                        "Cannot find name 'Widget'",
                        Severity::Error
                    ),
                    (
                        "TK2304",
                        77,
                        83,
                        "Cannot find name 'Widget'",
                        Severity::Error
                    ),
                ]
            );
            assert_eq!(
                String::from_utf8(rendered).expect("source route UTF-8"),
                "source.ts(2,14): error TK2304: Cannot find name 'Widget'\n\
                 source.ts(2,27): error TK2304: Cannot find name 'Widget'\n\
                 source.ts(1,1): incomplete[decl/export-default/self]: export default not modeled\n"
            );
        }
        "consumer.ts" => {
            assert_eq!(
                report.source,
                "import Widget from \"./source.js\";\n\
                 const good: Widget = new Widget();\n\
                 const bad: string = good.value;\n"
            );
            assert!(incomplete.is_empty());
            assert_eq!(
                diagnostics,
                [
                    (
                        "TK2304",
                        46,
                        52,
                        "Cannot find name 'Widget'",
                        Severity::Error
                    ),
                    (
                        "TK2304",
                        59,
                        65,
                        "Cannot find name 'Widget'",
                        Severity::Error
                    ),
                ]
            );
            assert_eq!(
                String::from_utf8(rendered).expect("consumer route UTF-8"),
                "consumer.ts(2,13): error TK2304: Cannot find name 'Widget'\n\
                 consumer.ts(2,26): error TK2304: Cannot find name 'Widget'\n"
            );
        }
        other => panic!("unexpected route report {other}"),
    }
}

#[test]
#[ignore = "enabled with the WU6 public cutover; explicit and legacy bytes stay frozen"]
fn explicit_and_legacy_routes_retain_the_pre_cutover_baseline() {
    let project = corpus_root().join("01_named_class");
    for names in [["source.ts", "consumer.ts"], ["consumer.ts", "source.ts"]] {
        let explicit = check_project_once(route_inputs(&project, &names)).expect("explicit route");
        let legacy = check_project(route_inputs(&project, &names)).expect("legacy route");
        assert_eq!(explicit.len(), 2);
        assert_eq!(legacy.len(), 2);
        assert_eq!(
            explicit[0].output.project_notices,
            [
                "unsupported-module-form default-import consumer.ts:1:1 ./source.js",
                "unsupported-module-form default-export source.ts:1:1",
            ]
        );
        assert!(explicit[1].output.project_notices.is_empty());
        for report in &explicit {
            assert_frozen_route_report(report, &report.name);
        }
        for report in &legacy {
            assert!(report.output.project_notices.is_empty());
            assert_frozen_route_report(report, &report.name);
        }
    }
}

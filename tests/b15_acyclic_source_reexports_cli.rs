//! Public Bundler acceptance plus frozen explicit and legacy route guards.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use typokat::driver::{
    check_bundler_project_once, check_project, check_project_once, CheckOutput, FileReport,
};
use typokat::frontend::{FileInput, ProjectRoot};

const CONTRACT: &str = include_str!("cases/b15_acyclic_source_reexports/contract.json");
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn from_case(case: &Value, order: &str) -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "typokat-b15-{}-{order}-{}-{id}",
            case["id"].as_str().expect("case id"),
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create B15 temporary project");
        for file in case["files"].as_array().expect("case files") {
            let relative = file["path"].as_str().expect("file path");
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create B15 source parent");
            }
            std::fs::write(&path, file["text"].as_str().expect("file text"))
                .expect("write B15 source");
        }
        let run = oracle_run(case, order);
        let config = json!({
            "compilerOptions": {
                "strict": true,
                "noEmit": true,
                "module": "ESNext",
                "moduleResolution": "Bundler"
            },
            "files": run["config"]["files"]
        });
        std::fs::write(
            root.join("tsconfig.json"),
            serde_json::to_string_pretty(&config).expect("serialize B15 config"),
        )
        .expect("write B15 config");
        Self { root }
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases/b15_acyclic_source_reexports")
}

fn contract() -> Value {
    serde_json::from_str(CONTRACT).expect("B15 source re-export contract is valid JSON")
}

fn case_by_id<'a>(contract: &'a Value, id: &str) -> &'a Value {
    contract["cases"]
        .as_array()
        .expect("contract cases")
        .iter()
        .find(|case| case["id"] == id)
        .unwrap_or_else(|| panic!("missing contract case {id}"))
}

fn oracle_run<'a>(case: &'a Value, order: &str) -> &'a Value {
    case["runs"]
        .as_array()
        .expect("oracle runs")
        .iter()
        .find(|run| run["order"] == order)
        .unwrap_or_else(|| panic!("missing {order} oracle run"))
}

fn run(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typokat"))
        .args(args)
        .output()
        .expect("run typokat B15 contract")
}

fn run_in(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typokat"))
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run typokat B15 explicit route")
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

fn route_inputs(project: &Path, names: &[&str]) -> Vec<FileInput> {
    names
        .iter()
        .map(|name| FileInput {
            name: (*name).to_owned(),
            source: std::fs::read_to_string(project.join(name)).expect("read route fixture"),
        })
        .collect()
}

fn route_roots(project: &Path, names: &[&str]) -> Vec<ProjectRoot> {
    names
        .iter()
        .map(|name| ProjectRoot {
            identity: (*name).to_owned(),
            path: project.join(name),
            exists: true,
        })
        .collect()
}

fn assert_output(output: &CheckOutput, notices: &[&str]) {
    assert!(output.diagnostics.is_empty());
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty());
    assert_eq!(
        output.project_notices,
        notices
            .iter()
            .map(|notice| (*notice).to_owned())
            .collect::<Vec<_>>()
    );
    assert!(!output.has_errors());
    assert_eq!(output.is_incomplete(), !notices.is_empty());
}

fn assert_report(report: &FileReport, project: &Path, name: &str, notices: &[&str]) {
    assert_eq!(report.name, name);
    assert_eq!(
        report.source,
        std::fs::read_to_string(project.join(name)).expect("read expected route source")
    );
    assert_output(&report.output, notices);
}

fn parse_summary(output: &Output, label: &str) -> Value {
    assert!(output.stdout.ends_with(b"\n"), "{label} JSONL newline");
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "{label} one summary"
    );
    serde_json::from_slice(&output.stdout).expect("B15 project summary is JSON")
}

#[test]
fn oracle_contract_is_machine_complete_and_matches_fixtures() {
    let contract = contract();
    assert_eq!(contract["oracle"]["version"], "Version 6.0.3");
    assert_eq!(contract["summary"]["case_count"], 31);
    assert_eq!(contract["summary"]["run_count"], 62);
    assert_eq!(contract["summary"]["root_order_byte_identical_count"], 31);
    assert_eq!(contract["summary"]["post_wu6_exact_summary_case_count"], 18);
    assert_eq!(contract["summary"]["post_wu6_exact_summary_run_count"], 36);
    for case in contract["cases"].as_array().expect("contract cases") {
        let id = case["id"].as_str().expect("case id");
        let project = corpus_root().join(id);
        for file in case["files"].as_array().expect("case files") {
            let relative = file["path"].as_str().expect("file path");
            assert_eq!(
                std::fs::read_to_string(project.join(relative)).expect("read oracle fixture"),
                file["text"].as_str().expect("oracle file text"),
                "{id}/{relative}"
            );
        }
        let normal = oracle_run(case, "normal");
        let reverse = oracle_run(case, "reverse");
        assert_eq!(normal["stdout"], reverse["stdout"], "{id} oracle stdout");
        assert_eq!(normal["stderr"], "", "{id} normal stderr");
        assert_eq!(reverse["stderr"], "", "{id} reverse stderr");
        assert_eq!(case["root_order_byte_identical"], true, "{id} oracle order");
        for order in ["normal", "reverse"] {
            let actual: Value = serde_json::from_str(
                &std::fs::read_to_string(project.join(format!("tsconfig.{order}.json")))
                    .expect("read oracle config"),
            )
            .expect("oracle config JSON");
            assert_eq!(actual["files"], oracle_run(case, order)["config"]["files"]);
        }
    }

    for id in ["07a_missing_module_single", "07c_missing_module_multi"] {
        let case = case_by_id(&contract, id);
        for run in case["typokat_after_wu6"]["runs"]
            .as_array()
            .expect("post-WU6 runs")
        {
            assert_eq!(
                run["summary"]["resolutions"],
                json!(["barrel.ts:1:1 source-reexport ./missing.js -> unresolved"]),
                "{id} has one declaration-owned missing resolution"
            );
        }
    }
}

#[test]
fn deferred_bundler_results_retain_pre_change_bytes() {
    let contract = contract();
    for id in contract["lifecycle"]["frozen_pre_change_case_ids"]
        .as_array()
        .expect("frozen pre-change case ids")
    {
        let id = id.as_str().expect("pre-change id");
        let expected = contract["pre_change"]["cases"]
            .as_array()
            .expect("pre-change cases")
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap_or_else(|| panic!("missing frozen pre-change entry {id}"));
        let output = run(&project_args(&corpus_root().join(id)));
        assert_eq!(
            output.status.code(),
            expected["exit"].as_i64().map(|code| code as i32),
            "{id} exit"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected["stdout"].as_str().expect("pre-change stdout"),
            "{id} stdout"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            expected["stderr"].as_str().expect("pre-change stderr"),
            "{id} stderr"
        );
    }
}

#[test]
fn default_import_export_moves_to_exact_clean_wu6_output_in_both_root_orders() {
    let contract = contract();
    let case = case_by_id(&contract, "13a_default_import_export");
    let expected = &contract["post_wu6"]["default_import_export"];
    let mut outputs = Vec::new();

    for order in ["normal", "reverse"] {
        let oracle = oracle_run(case, order);
        assert_eq!(oracle["exit"], 0, "{order} pinned tsc exit");
        assert_eq!(oracle["stdout"], "", "{order} pinned tsc stdout");
        assert_eq!(oracle["stderr"], "", "{order} pinned tsc stderr");

        let project = TempProject::from_case(case, order);
        let output = run(&project_args(&project.root));
        assert_eq!(
            output.status.code(),
            expected["exit"].as_i64().map(|code| code as i32),
            "{order} typokat exit"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected["stdout"].as_str().expect("post-WU6 stdout"),
            "{order} typokat stdout"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            expected["stderr"].as_str().expect("post-WU6 stderr"),
            "{order} typokat stderr"
        );

        let summary = parse_summary(&output, &format!("13a_default_import_export/{order}"));
        assert_eq!(
            summary["resolutions"],
            json!([expected["resolution"]]),
            "{order} admitted default resolution"
        );
        assert_eq!(
            summary["files"]["checked"],
            json!(["consumer.ts", "source.ts"])
        );
        assert_eq!(summary["files"]["skipped"], json!([]));
        assert_eq!(summary["project_notices"], json!([]));
        assert_eq!(summary["diagnostics"], json!([]));
        outputs.push((output.stdout, output.stderr));
    }

    assert_eq!(outputs[0], outputs[1], "13a root-order bytes");
}

#[test]
fn bundler_cutover_preserves_explicit_and_legacy_route_bytes() {
    let contract = contract();
    let project = corpus_root().join("route_baseline");
    let frozen = &contract["pre_change"]["bundler_source_reexport"];
    let admitted = &contract["post_wu6"]["bundler_source_reexport"];
    let notice = frozen["notice"].as_str().expect("baseline notice");

    for input in [&project, &project.join("tsconfig.json")] {
        let cli = run(&project_args(input));
        assert_eq!(cli.status.code(), Some(0));
        assert_eq!(
            String::from_utf8_lossy(&cli.stdout),
            admitted["stdout"].as_str().expect("admitted stdout")
        );
        assert!(cli.stderr.is_empty());
    }

    for names in [["barrel.ts", "source.ts"], ["source.ts", "barrel.ts"]] {
        let explicit_cli = run_in(
            &project,
            &["check", "--format", "compact", names[0], names[1]],
        );
        assert_eq!(explicit_cli.status.code(), Some(3));
        assert!(explicit_cli.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&explicit_cli.stderr),
            format!("project: {notice}\n")
        );

        let bundler = check_bundler_project_once(
            route_inputs(&project, &names),
            project.clone(),
            route_roots(&project, &names),
        )
        .expect("Bundler baseline runs");
        assert_eq!(
            bundler.inventory.resolutions,
            [admitted["resolution"]
                .as_str()
                .expect("admitted resolution")]
        );
        assert!(bundler.inventory.notices.is_empty());
        assert!(bundler.inventory.parse_errors.is_empty());
        assert!(bundler.inventory.missing_module_locations.is_empty());
        assert_eq!(bundler.reports.len(), 2);
        assert_report(&bundler.reports[0], &project, names[0], &[]);
        assert_report(&bundler.reports[1], &project, names[1], &[]);

        let explicit =
            check_project_once(route_inputs(&project, &names)).expect("explicit baseline runs");
        assert_eq!(explicit.len(), 2);
        assert_report(&explicit[0], &project, names[0], &[notice]);
        assert_report(&explicit[1], &project, names[1], &[]);

        let legacy = check_project(route_inputs(&project, &names)).expect("legacy baseline runs");
        assert_eq!(legacy.len(), 2);
        assert_report(&legacy[0], &project, names[0], &[]);
        assert_report(&legacy[1], &project, names[1], &[]);
    }
}

#[test]
fn admitted_bundler_projects_match_oracle_in_both_root_orders() {
    let contract = contract();
    let mut ids = contract["classifications"]["admitted"]
        .as_array()
        .expect("admitted ids")
        .clone();
    ids.extend(
        contract["classifications"]["empty_noop"]
            .as_array()
            .expect("empty no-op ids")
            .iter()
            .cloned(),
    );
    let mut failures = Vec::new();
    for id in ids {
        let id = id.as_str().expect("case id");
        let case = case_by_id(&contract, id);
        let mut outputs = Vec::new();
        for order in ["normal", "reverse"] {
            let expected = case["typokat_after_wu6"]["runs"]
                .as_array()
                .expect("post-WU6 runs")
                .iter()
                .find(|run| run["order"] == order)
                .unwrap_or_else(|| panic!("missing {id}/{order} post-WU6 summary"));
            let expected_exit = expected["exit"].as_i64().expect("post-WU6 exit") as i32;
            let project = TempProject::from_case(case, order);
            let output = run(&project_args(&project.root));
            if output.status.code() != Some(expected_exit) {
                failures.push(format!(
                    "{id}/{order}: expected exit {expected_exit}, got {:?}; stderr={}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
            let summary = parse_summary(&output, &format!("{id}/{order}"));
            if summary != expected["summary"] {
                failures.push(format!(
                    "{id}/{order}: summary {summary:?} != {:?}",
                    expected["summary"]
                ));
            }
            outputs.push((output.stdout, output.stderr));
        }
        if outputs.len() == 2 && outputs[0] != outputs[1] {
            failures.push(format!("{id}: root orders are not byte-identical"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn namespace_provenance_fails_closed_with_exact_identities() {
    let contract = contract();
    let matrix: [(&str, &[&str]); 5] = [
        (
            "10a_namespace_only",
            &["unsupported-source-reexport-namespace-provenance barrel.ts:1:1 ./source.js Alias"],
        ),
        (
            "10b_class_namespace",
            &["unsupported-source-reexport-namespace-provenance barrel.ts:1:1 ./source.js Merged"],
        ),
        (
            "10c_function_namespace",
            &["unsupported-source-reexport-namespace-provenance barrel.ts:1:1 ./source.js merged"],
        ),
        (
            "10d_aliased_namespace_chain",
            &[
                "unsupported-source-reexport-namespace-provenance barrel-a.ts:1:1 ./source.js First",
                "unsupported-source-reexport-namespace-provenance barrel-b.ts:1:1 ./barrel-a.js Final",
            ],
        ),
        (
            "13g_namespace_bearing_target",
            &["unsupported-source-reexport-namespace-provenance barrel.ts:1:1 ./source.js callable"],
        ),
    ];
    let mut failures = Vec::new();
    for (id, expected_notices) in matrix {
        let case = case_by_id(&contract, id);
        let mut outputs = Vec::new();
        for order in ["normal", "reverse"] {
            let project = TempProject::from_case(case, order);
            let output = run(&project_args(&project.root));
            if output.status.code() != Some(3) {
                failures.push(format!("{id}/{order}: exit {:?}", output.status.code()));
                continue;
            }
            let summary = parse_summary(&output, &format!("{id}/{order}"));
            if summary["project_notices"] != json!(expected_notices) {
                failures.push(format!(
                    "{id}/{order}: notices {:?}",
                    summary["project_notices"]
                ));
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            let expected_stderr = expected_notices
                .iter()
                .map(|notice| format!("project: {notice}\n"))
                .collect::<String>();
            if stderr != expected_stderr || stderr.contains("TK2305") {
                failures.push(format!("{id}/{order}: stderr {stderr:?}"));
            }
            outputs.push((output.stdout, output.stderr));
        }
        if outputs.len() == 2 && outputs[0] != outputs[1] {
            failures.push(format!("{id}: root orders are not byte-identical"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn source_reexport_cycles_keep_the_exact_cycle_identity() {
    let contract = contract();
    let expected_notice = "unsupported-module-cycle a.ts -> b.ts -> a.ts";
    let mut failures = Vec::new();
    for id in ["12a_reexport_cycle", "12b_mixed_import_reexport_cycle"] {
        let case = case_by_id(&contract, id);
        let mut outputs = Vec::new();
        for order in ["normal", "reverse"] {
            let project = TempProject::from_case(case, order);
            let output = run(&project_args(&project.root));
            if output.status.code() != Some(3) {
                failures.push(format!("{id}/{order}: exit {:?}", output.status.code()));
                continue;
            }
            let summary = parse_summary(&output, &format!("{id}/{order}"));
            if summary["project_notices"] != json!([expected_notice]) {
                failures.push(format!(
                    "{id}/{order}: notices {:?}",
                    summary["project_notices"]
                ));
            }
            if summary["diagnostics"] != json!([])
                || String::from_utf8_lossy(&output.stderr)
                    != format!("project: {expected_notice}\n")
            {
                failures.push(format!(
                    "{id}/{order}: non-cycle output {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            outputs.push((output.stdout, output.stderr));
        }
        if outputs.len() == 2 && outputs[0] != outputs[1] {
            failures.push(format!("{id}: root orders are not byte-identical"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

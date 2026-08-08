//! Black-box preservation guards plus disabled RED contract for the Bundler project tracer.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CONTRACT: &str = include_str!("cases/b72_bundler_project_tracer/contract.json");

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases/b72_bundler_project_tracer")
}

fn contract() -> Value {
    serde_json::from_str(CONTRACT).expect("WU1 project contract is valid JSON")
}

fn contract_entry(section: &str, id: &str) -> Value {
    contract()[section]
        .as_array()
        .unwrap_or_else(|| panic!("contract section {section} is an array"))
        .iter()
        .find(|case| case["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("missing {section} entry {id}"))
}

fn expected_case(id: &str) -> Value {
    contract_entry("cases", id)
}

fn run(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typokat"))
        .args(args)
        .output()
        .expect("run typokat project contract")
}

fn run_in(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typokat"))
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run typokat explicit-file contract")
}

fn project_args(input: &Path) -> Vec<String> {
    vec![
        "check".to_owned(),
        "--project-summary".to_owned(),
        "json".to_owned(),
        input.to_string_lossy().into_owned(),
    ]
}

fn assert_one_summary(case_id: &str, expected: &Value, input: &Path) -> Output {
    let output = run(&project_args(input));
    assert_eq!(
        output.status.code(),
        expected["typokat"]["exit"].as_i64().map(|code| code as i32),
        "{case_id} exit; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"), "{case_id} JSONL newline");
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "{case_id} emits one JSON line"
    );
    let summary_text = String::from_utf8_lossy(&output.stdout);
    let mut previous = None;
    for key in contract()["summary_exact_keys"]
        .as_array()
        .expect("summary keys are an array")
    {
        let needle = format!("\"{}\":", key.as_str().expect("summary key"));
        let position = summary_text
            .find(&needle)
            .unwrap_or_else(|| panic!("{case_id} summary is missing {needle}"));
        if let Some(previous) = previous {
            assert!(position > previous, "{case_id} top-level key order");
        }
        previous = Some(position);
    }
    let actual: Value =
        serde_json::from_slice(&output.stdout).expect("project summary is valid JSON");
    assert_eq!(actual, expected["typokat"]["summary"], "{case_id} summary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let summary = &expected["typokat"]["summary"];
    for diagnostic in summary["diagnostics"]
        .as_array()
        .expect("diagnostics are an array")
    {
        let identity = diagnostic
            .as_str()
            .expect("diagnostic identity")
            .split_whitespace()
            .last()
            .expect("diagnostic code");
        assert!(stderr.contains(identity), "{case_id} renders {identity}");
    }
    for incomplete in summary["incomplete"]
        .as_array()
        .expect("incomplete records are an array")
    {
        let identity = incomplete
            .as_str()
            .expect("incomplete identity")
            .split_whitespace()
            .last()
            .expect("incomplete id");
        assert!(
            stderr.contains(&format!("incomplete[{identity}]")),
            "{case_id} renders {identity}"
        );
    }
    if !summary["parse_errors"]
        .as_array()
        .expect("parse errors are an array")
        .is_empty()
    {
        assert!(
            stderr.contains("Unexpected token"),
            "{case_id} parse stderr"
        );
    }
    if summary["diagnostics"].as_array().is_some_and(Vec::is_empty)
        && summary["incomplete"].as_array().is_some_and(Vec::is_empty)
        && summary["parse_errors"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && summary["project_notices"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        assert!(output.stderr.is_empty(), "{case_id} clean stderr");
    }
    output
}

fn assert_summary(case_id: &str, input: &Path) -> Output {
    let expected = expected_case(case_id);
    assert_one_summary(case_id, &expected, input)
}

#[test]
fn directory_and_config_inputs_are_byte_identical() {
    for case in contract()["cases"].as_array().expect("cases are an array") {
        let case_id = case["id"].as_str().expect("case id");
        let project = corpus_root().join(case_id);
        let directory = assert_summary(case_id, &project);
        let config = assert_summary(case_id, &project.join("tsconfig.json"));
        assert_eq!(directory.stdout, config.stdout, "{case_id} stdout");
        assert_eq!(directory.stderr, config.stderr, "{case_id} stderr");
    }
}

#[test]
fn unsupported_projects_fail_closed_with_complete_identities() {
    for case_id in [
        "unsupported_bare_specifier",
        "unsupported_cycle",
        "unsupported_default_import",
        "unsupported_default_export",
        "unsupported_export_as_namespace",
        "unsupported_export_assignment",
        "unsupported_import_equals",
        "unsupported_mixed_default_named",
        "unsupported_mixed_default_namespace",
        "unsupported_namespace_import",
        "unsupported_namespace_reexport",
        "unsupported_profile",
        "unsupported_root_globs",
        "unsupported_side_effect_import",
        "unsupported_star_reexport",
        "unsupported_with_diagnostic",
    ] {
        let project = corpus_root().join(case_id);
        let output = assert_summary(case_id, &project);
        let config = assert_summary(case_id, &project.join("tsconfig.json"));
        assert_eq!(output.stdout, config.stdout, "{case_id} stdout");
        assert_eq!(output.stderr, config.stderr, "{case_id} stderr");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unsupported"),
            "{case_id} renders its non-clean project result"
        );
    }
}

#[test]
fn config_boundary_is_explicit_and_fail_closed() {
    for case in contract()["config_boundary_cases"]
        .as_array()
        .expect("config boundary cases are an array")
    {
        let case_id = case["id"].as_str().expect("config case id");
        let expected = json!({
            "typokat": {
                "exit": 3,
                "summary": {
                    "schema": 1,
                    "profile": "bundler",
                    "config": "tsconfig.json",
                    "roots": case["roots"],
                    "files": {
                        "checked": [],
                        "skipped": case["skipped"],
                        "excluded": []
                    },
                    "resolutions": [],
                    "project_notices": case["project_notices"],
                    "parse_errors": [],
                    "incomplete": [],
                    "diagnostics": []
                }
            }
        });
        let project = corpus_root().join(case_id);
        let output = assert_one_summary(case_id, &expected, &project);
        let config = assert_one_summary(case_id, &expected, &project.join("tsconfig.json"));
        assert_eq!(output.stdout, config.stdout, "{case_id} stdout");
        assert_eq!(output.stderr, config.stderr, "{case_id} stderr");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unsupported")
                || String::from_utf8_lossy(&output.stderr).contains("missing"),
            "{case_id} renders the config notice"
        );
    }
}

#[test]
fn explicit_file_lists_do_not_keep_silently_filtered_imports_clean() {
    for case_id in [
        "unsupported_bare_specifier",
        "unsupported_cycle",
        "unsupported_default_import",
        "unsupported_default_export",
        "unsupported_export_as_namespace",
        "unsupported_export_assignment",
        "unsupported_import_equals",
        "unsupported_mixed_default_named",
        "unsupported_mixed_default_namespace",
        "unsupported_namespace_import",
        "unsupported_namespace_reexport",
        "unsupported_side_effect_import",
        "unsupported_source_reexport",
        "unsupported_star_reexport",
    ] {
        let project = corpus_root().join(case_id);
        let mut files = std::fs::read_dir(&project)
            .expect("read contract project")
            .map(|entry| entry.expect("read contract entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ts"))
            .collect::<Vec<_>>();
        files.sort();
        let mut args = vec![
            "check".to_owned(),
            "--format".to_owned(),
            "compact".to_owned(),
        ];
        args.extend(files.iter().map(|path| path.to_string_lossy().into_owned()));
        let output = run(&args);
        assert_eq!(output.status.code(), Some(3), "{case_id} must be non-clean");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unsupported"),
            "{case_id} identifies the filtered form"
        );
    }
}

#[test]
fn admitted_explicit_file_output_is_byte_identical() {
    let extensionless = run_in(
        &corpus_root().join("admitted_files_extensionless"),
        &["check", "--format", "compact", "main.ts", "value.ts"],
    );
    assert_eq!(extensionless.status.code(), Some(1));
    assert!(extensionless.stdout.is_empty());
    assert_eq!(
        extensionless.stderr,
        b"main.ts(3,7): error TK2322: Type 'number' is not assignable to type 'string'\n"
    );

    let js_substitution = run_in(
        &corpus_root().join("admitted_files_js_substitution"),
        &["check", "--format", "compact", "main.ts", "value.ts"],
    );
    assert_eq!(js_substitution.status.code(), Some(1));
    assert!(js_substitution.stdout.is_empty());
    assert_eq!(
        js_substitution.stderr,
        b"main.ts(1,10): error TK2307: Cannot find module './value.js'\n"
    );
}

#[test]
fn malformed_and_ambiguous_project_inputs_are_exact_usage_failures() {
    let error_case = |id: &str| contract_entry("cli_error_cases", id);

    let missing_project = corpus_root().join("missing_directory_config");
    let missing = run(&project_args(&missing_project));
    let missing_expected = error_case("missing_directory_config");
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr),
        missing_expected["stderr"]
            .as_str()
            .expect("missing-config stderr")
            .replace("<project>", &missing_project.to_string_lossy())
    );

    let malformed_project = corpus_root().join("malformed_config");
    let malformed = run(&project_args(&malformed_project.join("tsconfig.json")));
    let malformed_expected = error_case("malformed_config");
    assert_eq!(malformed.status.code(), Some(2));
    assert!(malformed.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&malformed.stderr),
        malformed_expected["stderr"]
            .as_str()
            .expect("malformed-config stderr")
            .replace("<project>", &malformed_project.to_string_lossy())
    );
    let malformed_directory = run(&project_args(&malformed_project));
    assert_eq!(malformed_directory.status.code(), Some(2));
    assert!(malformed_directory.stdout.is_empty());
    assert_eq!(malformed_directory.stderr, malformed.stderr);

    let project = corpus_root().join("admitted_files_extensionless");
    let diagnostic_source = corpus_root().join("incomplete_with_diagnostic/main.ts");
    let mixed = run(&[
        "check".to_owned(),
        "--project-summary".to_owned(),
        "json".to_owned(),
        project.to_string_lossy().into_owned(),
        diagnostic_source.to_string_lossy().into_owned(),
    ]);
    assert_eq!(mixed.status.code(), Some(2));
    assert!(mixed.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&mixed.stderr),
        error_case("mixed_project_and_source")["stderr"]
            .as_str()
            .expect("mixed-input stderr")
    );
    assert!(!String::from_utf8_lossy(&mixed.stderr).contains("TK2322"));

    let second_project = corpus_root().join("admitted_files_js_substitution");
    let two_projects = run(&[
        "check".to_owned(),
        "--project-summary".to_owned(),
        "json".to_owned(),
        project.to_string_lossy().into_owned(),
        second_project.to_string_lossy().into_owned(),
    ]);
    assert_eq!(two_projects.status.code(), Some(2));
    assert!(two_projects.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&two_projects.stderr),
        error_case("two_project_inputs")["stderr"]
            .as_str()
            .expect("two-project stderr")
    );
}

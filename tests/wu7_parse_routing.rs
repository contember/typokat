//! RED contract: malformed user input stays on the ordinary parse-diagnostic channel.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use typokat::driver::{check_files, check_project, check_source, CheckOutput};
use typokat::frontend::FileInput;

const BIN: &str = env!("CARGO_BIN_EXE_typokat");
const MALFORMED_GLOBAL_SOURCE: &str =
    "export {};\ndeclare global { interface Array<T> { broken: T; }\n";
// Oxc recovers from this TS1063 with `panicked == false`.
const RECOVERABLE_PARSE_SOURCE: &str =
    "declare namespace Broken { export = Broken; }\nconst semanticLeak: string = 1;\n";
const CLEAN_SIBLING_SOURCE: &str = "export const clean = 1;\n";

fn assert_parse_diagnostic(label: &str, output: &CheckOutput) {
    assert!(
        !output.parse_errors.is_empty(),
        "{label} must preserve the parser diagnostic"
    );
    assert!(
        output.incomplete.is_empty(),
        "{label} must not turn malformed syntax into an incomplete semantic run"
    );
}

fn assert_recoverable_parse_only(label: &str, output: &CheckOutput) {
    assert_eq!(
        output.parse_errors.len(),
        1,
        "{label} must preserve the one recoverable parser diagnostic"
    );
    assert_eq!(
        output.parse_errors[0], "An export assignment cannot be used in a namespace.",
        "{label} must preserve the canonical recoverable parser diagnostic"
    );
    assert!(
        output.diagnostics.is_empty(),
        "{label} must not semantically check the recovered AST"
    );
    assert!(
        output.incomplete.is_empty(),
        "{label} must not report semantic incompletes from the recovered AST"
    );
}

#[test]
fn malformed_global_source_is_not_a_library_initialization_failure() {
    let output = check_source(MALFORMED_GLOBAL_SOURCE)
        .expect("malformed user syntax is an ordinary check result");

    assert_parse_diagnostic("check_source", &output);
}

#[test]
fn malformed_global_project_is_not_a_library_initialization_failure() {
    let reports = check_project(vec![FileInput {
        name: "/wu7/malformed-global.ts".to_owned(),
        source: MALFORMED_GLOBAL_SOURCE.to_owned(),
    }])
    .expect("malformed project syntax is an ordinary check result");

    assert_eq!(reports.len(), 1);
    assert_parse_diagnostic("check_project", &reports[0].output);
}

#[test]
fn cli_reports_malformed_global_source_as_exit_one() {
    let path = unique_temp_path();
    fs::write(&path, MALFORMED_GLOBAL_SOURCE).expect("write malformed CLI probe");
    let output = Command::new(BIN)
        .args(["check", "--format", "compact"])
        .arg(&path)
        .output()
        .expect("run typokat check");
    let _ = fs::remove_file(path);

    assert_eq!(output.status.code(), Some(1), "{output:#?}");
    assert!(output.stdout.is_empty(), "{output:#?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"), "{output:#?}");
    assert!(
        !stderr.contains("failed to initialize embedded TypeScript 6.0.3 library")
            && !stderr.contains("parse-rejected"),
        "parser diagnostics must not cross the provider-error boundary: {output:#?}"
    );
}

#[test]
fn recoverable_parse_diagnostic_stops_check_source_before_semantics() {
    let output = check_source(RECOVERABLE_PARSE_SOURCE)
        .expect("recoverable user syntax is an ordinary check result");

    assert_recoverable_parse_only("check_source", &output);
}

#[test]
fn recoverable_parse_diagnostic_stops_only_its_check_files_pipeline() {
    let reports = check_files(vec![
        FileInput {
            name: "/wu7/recoverable.ts".to_owned(),
            source: RECOVERABLE_PARSE_SOURCE.to_owned(),
        },
        FileInput {
            name: "/wu7/clean-sibling.ts".to_owned(),
            source: CLEAN_SIBLING_SOURCE.to_owned(),
        },
    ])
    .expect("recoverable user syntax is an ordinary parallel check result");

    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].name, "/wu7/recoverable.ts");
    assert_eq!(reports[0].source, RECOVERABLE_PARSE_SOURCE);
    assert_recoverable_parse_only("check_files recoverable report", &reports[0].output);
    assert_eq!(reports[1].name, "/wu7/clean-sibling.ts");
    assert_eq!(reports[1].source, CLEAN_SIBLING_SOURCE);
    assert!(reports[1].output.parse_errors.is_empty());
    assert!(reports[1].output.diagnostics.is_empty());
    assert!(reports[1].output.incomplete.is_empty());
}

#[test]
fn recoverable_parse_diagnostic_stops_the_project_before_semantics() {
    let reports = check_project(vec![
        FileInput {
            name: "/wu7/clean-sibling.ts".to_owned(),
            source: CLEAN_SIBLING_SOURCE.to_owned(),
        },
        FileInput {
            name: "/wu7/recoverable.ts".to_owned(),
            source: RECOVERABLE_PARSE_SOURCE.to_owned(),
        },
    ])
    .expect("recoverable user syntax is an ordinary project check result");

    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].name, "/wu7/clean-sibling.ts");
    assert_eq!(reports[0].source, CLEAN_SIBLING_SOURCE);
    assert!(reports[0].output.parse_errors.is_empty());
    assert!(reports[0].output.diagnostics.is_empty());
    assert!(reports[0].output.incomplete.is_empty());
    assert_eq!(reports[1].name, "/wu7/recoverable.ts");
    assert_eq!(reports[1].source, RECOVERABLE_PARSE_SOURCE);
    assert_recoverable_parse_only("check_project recoverable report", &reports[1].output);
}

#[test]
fn cli_reports_only_the_recoverable_parse_diagnostic() {
    let path = unique_temp_path();
    fs::write(&path, RECOVERABLE_PARSE_SOURCE).expect("write recoverable CLI probe");
    let output = Command::new(BIN)
        .args(["check", "--format", "compact"])
        .arg(&path)
        .output()
        .expect("run typokat check");
    let _ = fs::remove_file(path);

    assert_eq!(output.status.code(), Some(1), "{output:#?}");
    assert!(output.stdout.is_empty(), "{output:#?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr, "error: An export assignment cannot be used in a namespace.\n",
        "the CLI must render only the ordinary parser diagnostic: {output:#?}"
    );
}

fn unique_temp_path() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "typokat-wu7-malformed-global-{}-{nanos}.ts",
        std::process::id()
    ))
}

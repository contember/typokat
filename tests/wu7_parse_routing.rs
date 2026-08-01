//! RED contract: malformed user input stays on the ordinary parse-diagnostic channel.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use typokat::driver::{check_project, check_source, CheckOutput};
use typokat::frontend::FileInput;

const BIN: &str = env!("CARGO_BIN_EXE_typokat");
const MALFORMED_GLOBAL_SOURCE: &str =
    "export {};\ndeclare global { interface Array<T> { broken: T; }\n";

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

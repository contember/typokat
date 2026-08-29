//! Acceptance for committing library terminals demanded by user annotations.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn ordinary_record_annotation_commits_its_demanded_library_terminal() {
    let request = serde_json::to_string(&json!({
        "case_id": "record-annotation-publication",
        "schema": 1,
        "source": "function f(value: Record<string, unknown>) {}\n",
    }))
    .expect("request JSON")
        + "\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_typokat-session-batch"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn isolated-session runner");
    child
        .stdin
        .take()
        .expect("runner stdin")
        .write_all(request.as_bytes())
        .expect("write runner request");
    let output = child.wait_with_output().expect("collect runner output");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let receipt: Value = serde_json::from_slice(&output.stdout).expect("runner receipt JSON");
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

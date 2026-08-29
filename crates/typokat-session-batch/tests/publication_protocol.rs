//! Acceptance for committing library terminals demanded by user annotations.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn invoke(case_id: &str, source: &str) -> Value {
    let request = serde_json::to_string(&json!({
        "case_id": case_id,
        "schema": 1,
        "source": source,
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
    serde_json::from_slice(&output.stdout).expect("runner receipt JSON")
}

#[test]
fn ordinary_record_annotation_commits_its_demanded_library_terminal() {
    let receipt = invoke(
        "record-annotation-publication",
        "function f(value: Record<string, unknown>) {}\n",
    );
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

#[test]
fn global_array_binding_keeps_its_typed_incomplete_receipt() {
    let receipt = invoke("global-array-binding-publication", "var [value] = [1];\n");
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 3,
            "incompletes": ["bind/binding-pattern/array-pattern"],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_switch_retains_discriminated_union_members() {
    let receipt = invoke(
        "project-local-switch-narrowing",
        "type Item = { kind: true; a: string } | { kind: false; b: string };\n\
         function select(item: Item) {\n\
           switch (item.kind) {\n\
             case true: return item.a;\n\
             case false: return item.b;\n\
           }\n\
         }\n",
    );
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

#[test]
fn project_local_in_operator_retains_alias_union_members() {
    let receipt = invoke(
        "project-local-in-narrowing",
        "type Item = { a: string } | { b: string };\n\
         function select(item: Item) {\n\
           if (\"a\" in item) return item.a;\n\
           return item.b;\n\
         }\n",
    );
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

#[test]
fn project_local_enum_aliases_retain_comparison_overlap() {
    let receipt = invoke(
        "project-local-enum-alias-overlap",
        "enum Choice { Unknown, Yes, No }\n\
         type YesNo = Choice.Yes | Choice.No;\n\
         function inspect(a: Choice.Yes, b: YesNo) {\n\
           if (a === b) {}\n\
           switch (b) {\n\
             case Choice.Yes: break;\n\
             case Choice.No: break;\n\
           }\n\
         }\n",
    );
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

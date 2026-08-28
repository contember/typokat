//! Acceptance for the multi-file single-project runner linkage.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn canonical(value: &Value) -> String {
    serde_json::to_string(value).expect("canonical JSON")
}

fn invoke(input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typokat-session-batch"))
        .arg("--single-project")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn isolated-session runner");
    child
        .stdin
        .take()
        .expect("runner stdin")
        .write_all(input.as_bytes())
        .expect("write single-project request");
    child.wait_with_output().expect("collect runner output")
}

fn invoke_request(request: Value) -> Output {
    invoke(&(canonical(&request) + "\n"))
}

fn assert_rejected(input: &str, message: &str) {
    let output = invoke(input);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .expect("UTF-8 runner error")
            .contains(message),
        "rejection must name {message:?}"
    );
}

#[test]
fn single_project_checks_one_real_ordered_multi_file_project() {
    let request = json!({
        "case_id": "arbitrary-cross-file-witness",
        "files": [
            {
                "path": "a.ts",
                "source": "export const value = \"text\"; // ž\n",
            },
            {
                "path": "b.ts",
                "source": concat!(
                    "import { value } from \"./a\";\n",
                    "const wrong: number = value;\n",
                    "\n",
                    "\n",
                    "\n",
                    "\n",
                    "\n",
                    "\n",
                    "\n",
                    "const wrong_late: number = value;\n",
                ),
            },
        ],
        "schema": 1,
    });
    let output = invoke_request(request);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"\n"));
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );

    let line = std::str::from_utf8(&output.stdout)
        .expect("UTF-8 runner response")
        .strip_suffix('\n')
        .expect("one newline");
    let receipt: Value = serde_json::from_str(line).expect("single-project response JSON");
    assert_eq!(line, canonical(&receipt), "response must be canonical JSON");
    assert_eq!(
        receipt
            .as_object()
            .expect("single-project response object")
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "cache_weak_alive_after",
            "case_id",
            "checker_identity",
            "file_count",
            "program_identity",
            "retained_frame_state",
            "schema",
            "semantic",
            "semantic_identity",
            "session_weak_alive_after",
            "shared_product_identity",
            "tool",
        ])
    );
    assert_eq!(receipt["schema"], 1);
    assert_eq!(receipt["tool"], "typokat");
    assert_eq!(receipt["case_id"], "arbitrary-cross-file-witness");
    assert_eq!(receipt["file_count"], 2);
    assert_eq!(receipt["retained_frame_state"], 0);
    assert_eq!(receipt["session_weak_alive_after"], false);
    assert_eq!(receipt["cache_weak_alive_after"], false);
    assert_eq!(
        receipt["shared_product_identity"],
        typokat_library::embedded_library_profile_metadata().profile_identity()
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": ["b.ts:2:7:2322", "b.ts:10:7:2322"],
            "exit_code": 1,
            "incompletes": [],
            "parse_errors": [],
        })
    );
    let identities = [
        receipt["program_identity"].as_str(),
        receipt["checker_identity"].as_str(),
        receipt["semantic_identity"].as_str(),
    ];
    assert!(identities
        .iter()
        .all(|identity| identity.is_some_and(|value| !value.is_empty())));
    assert_eq!(
        identities
            .into_iter()
            .flatten()
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
}

#[test]
fn single_project_rejects_noncanonical_or_nonexclusive_requests() {
    assert_rejected("", "exactly one canonical JSON request");
    assert_rejected(
        "{\"schema\":1,\"case_id\":\"case\",\"files\":[{\"path\":\"a.ts\",\"source\":\"\"}]}\n",
        "single-project request is not canonical",
    );
    let request = canonical(&json!({
        "case_id": "case",
        "files": [{"path": "a.ts", "source": ""}],
        "schema": 1,
    }));
    assert_rejected(
        &format!("{request}\n{request}\n"),
        "exactly one canonical JSON request",
    );
    assert_rejected(
        &(canonical(&json!({
            "case_id": "case",
            "extra": true,
            "files": [{"path": "a.ts", "source": ""}],
            "schema": 1,
        })) + "\n"),
        "single-project request",
    );
}

#[test]
fn single_project_rejects_empty_unsafe_duplicate_or_reordered_paths() {
    assert_rejected(
        &(canonical(&json!({"case_id": "case", "files": [], "schema": 1})) + "\n"),
        "single-project file list is empty",
    );
    for path in [
        "",
        "/a.ts",
        "../a.ts",
        "./a.ts",
        "a/../b.ts",
        "a\\b.ts",
        "a.js",
    ] {
        assert_rejected(
            &(canonical(&json!({
                "case_id": "case",
                "files": [{"path": path, "source": ""}],
                "schema": 1,
            })) + "\n"),
            "single-project file path is unsafe",
        );
    }
    for paths in [["a.ts", "a.ts"], ["b.ts", "a.ts"]] {
        assert_rejected(
            &(canonical(&json!({
                "case_id": "case",
                "files": paths.map(|path| json!({"path": path, "source": ""})),
                "schema": 1,
            })) + "\n"),
            "single-project file paths must be strictly ordered and unique",
        );
    }
}

//! Acceptance for exact incomplete identities in session-batch semantic receipts.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Output, Stdio};

const UNRESOLVED_CASES: [&str; 4] = [
    "conformance/types/intersection/intersectionWithUnionConstraint.ts",
    "conformance/types/keyof/keyofAndIndexedAccessErrors.ts",
    "conformance/types/literal/numericStringLiteralTypes.ts",
    "conformance/types/tuple/wideningTuples2.ts",
];

const EXACT_INCOMPLETE_SOURCE: &str = concat!(
    "const value = 1;\n",
    "type First = typeof value;\n",
    "class Indexed { [key: string]: number; }\n",
    "type Second = typeof value;\n",
);

fn invoke(arguments: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typokat-session-batch"))
        .args(arguments)
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
        .expect("write runner request");
    child.wait_with_output().expect("collect runner output")
}

fn canonical_line(value: Value) -> String {
    serde_json::to_string(&value).expect("canonical JSON") + "\n"
}

fn semantic_rows(output: &Output) -> Vec<Value> {
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout.clone())
        .expect("UTF-8 runner output")
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line).expect("runner receipt JSON")["semantic"].clone()
        })
        .collect()
}

fn exact_incomplete_semantic() -> Value {
    json!({
        "diagnostics": [],
        "exit_code": 3,
        "incompletes": [
            "annotation-lower/type-query/typeof",
            "annotation-lower/type-query/typeof",
            "class/class-index-signature/self",
        ],
        "parse_errors": [],
    })
}

#[test]
fn ordinary_frames_preserve_sorted_incomplete_identities_with_multiplicity() {
    let request = canonical_line(json!({
        "case_id": "ordinary-exact-incomplete-identities",
        "schema": 1,
        "source": EXACT_INCOMPLETE_SOURCE,
    }));
    let output = invoke(&[], &request);
    assert_eq!(semantic_rows(&output), vec![exact_incomplete_semantic()]);
}

#[test]
fn single_project_preserves_sorted_incomplete_identities_with_multiplicity() {
    let request = canonical_line(json!({
        "case_id": "single-project-exact-incomplete-identities",
        "files": [{"path": "main.ts", "source": EXACT_INCOMPLETE_SOURCE}],
        "schema": 1,
    }));
    let output = invoke(&["--single-project"], &request);
    assert_eq!(semantic_rows(&output), vec![exact_incomplete_semantic()]);
}

#[test]
fn only_named_scoreboard_unresolved_rows_use_the_stable_class_token() {
    let input = UNRESOLVED_CASES
        .iter()
        .map(|case_id| {
            canonical_line(json!({
                "case_id": case_id,
                "schema": 1,
                "source": EXACT_INCOMPLETE_SOURCE,
            }))
        })
        .collect::<String>();
    let output = invoke(&[], &input);
    assert_eq!(
        semantic_rows(&output),
        vec![
            json!({
                "diagnostics": [],
                "exit_code": 3,
                "incompletes": ["unresolved"],
                "parse_errors": [],
            });
            UNRESOLVED_CASES.len()
        ]
    );
}

#[test]
fn named_scoreboard_unresolved_rows_ignore_underlying_semantic_shape() {
    let input = UNRESOLVED_CASES
        .iter()
        .flat_map(|case_id| {
            ["const clean = 1;\n", "const value: string = 1;\n"]
                .into_iter()
                .map(move |source| {
                    canonical_line(json!({
                        "case_id": case_id,
                        "schema": 1,
                        "source": source,
                    }))
                })
        })
        .collect::<String>();
    let output = invoke(&[], &input);
    assert_eq!(
        semantic_rows(&output),
        vec![
            json!({
                "diagnostics": [],
                "exit_code": 3,
                "incompletes": ["unresolved"],
                "parse_errors": [],
            });
            UNRESOLVED_CASES.len() * 2
        ]
    );
}

#[test]
fn parse_rejection_uses_only_the_stable_class_token() {
    let request = canonical_line(json!({
        "case_id": "ordinary-parse-rejected",
        "schema": 1,
        "source": "const = ;\n",
    }));
    let output = invoke(&[], &request);
    assert_eq!(
        semantic_rows(&output),
        vec![json!({
            "diagnostics": [],
            "exit_code": 2,
            "incompletes": [],
            "parse_errors": ["parse-error"],
        })]
    );
}

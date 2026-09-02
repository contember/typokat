//! RED protocol acceptance for isolated concurrent session execution.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};

const CASE_ID: &str = "post-panel-clean-frame";
const SOURCE: &str = "const clean = 1;\n";

fn canonical(value: &Value) -> String {
    serde_json::to_string(value).expect("canonical JSON")
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[test]
fn concurrent_two_sessions_are_isolated_and_overlap_at_one_barrier() {
    let request = canonical(&json!({
        "case_id": CASE_ID,
        "schema": 1,
        "source": SOURCE,
    })) + "\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_typokat-session-batch"))
        .args(["--concurrency", "2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn concurrent isolated-session runner");
    child
        .stdin
        .take()
        .expect("runner stdin")
        .write_all(request.as_bytes())
        .expect("write canonical frame request");
    let output = child.wait_with_output().expect("collect concurrent output");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"\n"));
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "concurrency mode must emit exactly one JSON array"
    );
    let line = std::str::from_utf8(&output.stdout)
        .expect("UTF-8 runner output")
        .strip_suffix('\n')
        .expect("one trailing newline");
    let response: Value = serde_json::from_str(line).expect("concurrency receipt JSON");
    assert_eq!(
        line,
        canonical(&response),
        "response must be canonical JSON"
    );
    let receipts = response.as_array().expect("concurrency receipt array");
    assert_eq!(receipts.len(), 2);

    let expected_keys = BTreeSet::from([
        "barrier_ns",
        "cache_identity",
        "cache_weak_alive_after",
        "end_ns",
        "identity",
        "publisher_identity",
        "raw_output_sha256",
        "semantic",
        "session_weak_alive_after",
        "start_ns",
    ]);
    let expected_semantic = json!({
        "diagnostics": [],
        "exit_code": 0,
        "incompletes": [],
        "parse_errors": [],
    });
    let expected_semantic_sha256 = sha256(&canonical(&expected_semantic));
    let mut identity_sets = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut starts = Vec::with_capacity(2);
    let mut ends = Vec::with_capacity(2);
    let mut shared_barrier = None;

    for receipt in receipts {
        let object = receipt.as_object().expect("session receipt object");
        assert_eq!(
            object.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            expected_keys
        );
        assert_eq!(receipt["semantic"], expected_semantic);
        assert_eq!(
            receipt["raw_output_sha256"].as_str(),
            Some(expected_semantic_sha256.as_str()),
            "semantic digest must authenticate the recomputed canonical semantic JSON"
        );
        assert_eq!(receipt["session_weak_alive_after"], false);
        assert_eq!(receipt["cache_weak_alive_after"], false);

        for (set, key) in
            identity_sets
                .iter_mut()
                .zip(["identity", "publisher_identity", "cache_identity"])
        {
            let identity = receipt[key]
                .as_str()
                .filter(|identity| !identity.is_empty())
                .expect("nonempty local identity");
            assert!(set.insert(identity), "{key} must be unique per session");
        }

        let start = receipt["start_ns"].as_u64().expect("integer start_ns");
        let barrier = receipt["barrier_ns"].as_u64().expect("integer barrier_ns");
        let end = receipt["end_ns"].as_u64().expect("integer end_ns");
        assert!(start <= barrier && barrier <= end);
        assert_eq!(
            *shared_barrier.get_or_insert(barrier),
            barrier,
            "every session must report the same barrier"
        );
        starts.push(start);
        ends.push(end);
    }

    assert!(identity_sets.iter().all(|set| set.len() == 2));
    assert_eq!(
        identity_sets
            .iter()
            .flat_map(|set| set.iter())
            .copied()
            .collect::<BTreeSet<_>>()
            .len(),
        6,
        "program, publisher, and cache identities must all be distinct"
    );
    assert!(
        starts.into_iter().max().expect("latest start")
            < ends.into_iter().min().expect("earliest end"),
        "the two session intervals must actually overlap"
    );
}

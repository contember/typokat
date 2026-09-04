#![cfg(feature = "counting-allocator")]
//! RED protocol acceptance for the separate counting-allocator run.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};

const CASE_ID: &str = "post-panel-clean-frame";
const SOURCE: &str = "const clean: Array<number> = [];\n";

fn canonical(value: &Value) -> String {
    serde_json::to_string(value).expect("canonical JSON")
}

fn binary_sha256() -> String {
    let binary = std::fs::read(env!("CARGO_BIN_EXE_typokat-session-batch"))
        .expect("read current session-batch binary");
    format!("{:x}", Sha256::digest(binary))
}

fn assert_real_clean_frame(receipt: &Value) {
    let object = receipt.as_object().expect("ordinary frame receipt object");
    assert_eq!(
        object.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "cache_weak_alive_after",
            "case_id",
            "checker_identity",
            "lifecycle_disposition",
            "materialized_product_owner_count",
            "ordinal",
            "phases",
            "product_initializations",
            "program_identity",
            "retained_frame_state",
            "route_end_ns",
            "route_start_ns",
            "semantic",
            "semantic_identity",
            "session_weak_alive_after",
            "shared_product_identity",
        ])
    );
    assert_eq!(receipt["ordinal"], 0);
    assert_eq!(receipt["case_id"], CASE_ID);
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
    assert_eq!(receipt["lifecycle_disposition"], "semantic");
    assert_eq!(receipt["retained_frame_state"], 0);
    assert_eq!(receipt["product_initializations"], 1);
    assert_eq!(receipt["session_weak_alive_after"], false);
    assert_eq!(receipt["cache_weak_alive_after"], false);
    assert_eq!(
        receipt["shared_product_identity"],
        typokat_library::embedded_library_profile_metadata().profile_identity()
    );
    let materialized = receipt["materialized_product_owner_count"]
        .as_u64()
        .expect("materialized product owner count");
    assert!(
        materialized > 0,
        "the real session must own demanded library work"
    );
    assert!(
        materialized < 100,
        "a narrow Array frame must not force the full library: {materialized} owners"
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

    let route_start = receipt["route_start_ns"]
        .as_u64()
        .expect("integer route start");
    let route_end = receipt["route_end_ns"].as_u64().expect("integer route end");
    assert!(route_start <= route_end);
    let phases = receipt["phases"]
        .as_object()
        .expect("real lifecycle phases");
    assert_eq!(
        phases.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "binding",
            "parse_prepare",
            "product_initialization",
            "publication_check",
            "release",
        ])
    );
    for phase in phases.values() {
        let phase = phase.as_object().expect("phase interval object");
        assert_eq!(
            phase.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from(["end_ns", "start_ns"])
        );
        let start = phase["start_ns"].as_u64().expect("integer phase start");
        let end = phase["end_ns"].as_u64().expect("integer phase end");
        assert!(route_start <= start && start <= end && end <= route_end);
    }
}

#[test]
fn allocation_probe_follows_real_frames_with_exact_untimed_metadata() {
    let request = canonical(&json!({
        "case_id": CASE_ID,
        "schema": 1,
        "source": SOURCE,
    })) + "\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_typokat-session-batch"))
        .arg("--allocation-probe")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn counting-allocation runner");
    child
        .stdin
        .take()
        .expect("runner stdin")
        .write_all(request.as_bytes())
        .expect("write canonical frame request");
    let output = child.wait_with_output().expect("collect allocation output");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(output.stdout.ends_with(b"\n"));
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        2,
        "one frame receipt must precede exactly one allocation receipt"
    );
    let stdout = std::str::from_utf8(&output.stdout).expect("UTF-8 runner output");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    let frame: Value = serde_json::from_str(lines[0]).expect("ordinary frame receipt JSON");
    let allocation: Value = serde_json::from_str(lines[1]).expect("allocation receipt JSON");
    assert_real_clean_frame(&frame);
    assert_eq!(
        lines[1],
        canonical(&allocation),
        "allocation receipt must be canonical JSON"
    );

    let object = allocation
        .as_object()
        .expect("allocation metadata receipt object");
    assert_eq!(
        object.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "allocation_count",
            "binary_sha256",
            "instrumentation",
            "timed",
            "tool",
        ])
    );
    assert_eq!(allocation["tool"], "typokat");
    assert_eq!(allocation["timed"], false);
    assert_eq!(allocation["instrumentation"], "counting-allocator");
    assert!(
        allocation["allocation_count"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "allocation_count must be a positive integer"
    );
    let digest = allocation["binary_sha256"]
        .as_str()
        .expect("binary digest string");
    assert_eq!(digest.len(), 64);
    assert!(digest
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    assert_eq!(digest, binary_sha256(), "digest must identify this binary");
}

//! Acceptance for lifecycle fault controls on the isolated-session runner.

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use typokat_check::check::checker::project_local::{
    CheckerSession, SessionBenchFault, SessionBenchFaultSeam,
};
use typokat_library::LibraryBaseProvider;

fn invoke_request(request: &str) -> Output {
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
        .write_all(format!("{request}\n").as_bytes())
        .expect("write fault request");
    child.wait_with_output().expect("collect runner output")
}

fn invoke_fault(fault: &str) -> Output {
    invoke_request(&format!(r#"{{"fault":"{fault}","schema":1}}"#))
}

fn assert_rejected_fault(fault: &str, expected_seam: &str) {
    let output = invoke_fault(fault);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty(), "fault stderr must stay empty");
    assert!(output.stdout.ends_with(b"\n"));
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).expect("fault receipt JSON");
    let object = receipt.as_object().expect("fault receipt object");
    assert_eq!(
        object
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            "cache_identity",
            "cache_weak_alive_after",
            "fault",
            "fault_hook_activations",
            "fault_seam",
            "project_identity",
            "publisher_identity",
            "rejected",
            "schema",
            "session_weak_alive_after",
            "shared_product_identity",
        ])
    );
    assert_eq!(receipt["schema"], 1);
    assert_eq!(receipt["fault"], fault);
    assert_eq!(receipt["fault_seam"], expected_seam);
    assert_eq!(receipt["fault_hook_activations"], 1);
    assert_eq!(receipt["rejected"], true);
    assert_eq!(receipt["session_weak_alive_after"], false);
    assert_eq!(receipt["cache_weak_alive_after"], false);
    assert_eq!(
        receipt["shared_product_identity"],
        typokat_library::embedded_library_profile_metadata().profile_identity()
    );
    let identities = [
        receipt["project_identity"].as_str(),
        receipt["publisher_identity"].as_str(),
        receipt["cache_identity"].as_str(),
    ];
    assert!(identities
        .iter()
        .all(|identity| identity.is_some_and(|value| !value.is_empty())));
    assert_eq!(
        identities
            .into_iter()
            .flatten()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
}

#[test]
fn rejects_session_reuse_through_the_live_runner() {
    assert_rejected_fault("reuse-session", "session-consume");
}

#[test]
fn rejects_cache_retention_through_the_live_runner() {
    assert_rejected_fault("retain-cache", "cache-release");
}

#[test]
fn typed_fault_receipts_come_from_the_live_session_lifecycle() {
    let product = LibraryBaseProvider::new()
        .get()
        .expect("declaration product");
    let mut identity_triples = Vec::new();
    for (fault, seam) in [
        (
            SessionBenchFault::ReuseSession,
            SessionBenchFaultSeam::SessionConsume,
        ),
        (
            SessionBenchFault::RetainCache,
            SessionBenchFaultSeam::CacheRelease,
        ),
    ] {
        let receipt = CheckerSession::new(product.clone()).reject_fault_for_session_bench(fault);
        assert_eq!(receipt.fault(), fault);
        assert_eq!(receipt.seam(), seam);
        assert_eq!(receipt.fault_hook_activations(), 1);
        assert!(receipt.rejected());
        assert_eq!(
            receipt.shared_product_identity(),
            product.profile_identity()
        );
        assert_ne!(receipt.project_identity(), receipt.publisher_identity());
        assert_ne!(receipt.project_identity(), receipt.cache_identity());
        assert_ne!(receipt.publisher_identity(), receipt.cache_identity());
        identity_triples.push((
            receipt.project_identity(),
            receipt.publisher_identity(),
            receipt.cache_identity(),
        ));
        assert!(!receipt.session_weak_alive_after());
        assert!(!receipt.cache_weak_alive_after());
    }
    assert_ne!(identity_triples[0], identity_triples[1]);
}

#[test]
fn accepts_exactly_the_two_contract_fault_names() {
    for fault in [
        "invented-lifecycle",
        "reuse-checker",
        "retain-frame",
        "ReuseSession",
        "reuse-session-extra",
        "",
    ] {
        let output = invoke_fault(fault);
        assert_eq!(output.status.code(), Some(2), "fault {fault:?}");
        assert!(output.stdout.is_empty(), "fault {fault:?}");
        assert!(!output.stderr.is_empty(), "fault {fault:?}");
    }

    for request in [
        r#"{"fault":"reuse-session","schema":2}"#,
        r#"{"extra":true,"fault":"reuse-session","schema":1}"#,
        r#"{"fault":"retain-cache"}"#,
    ] {
        let output = invoke_request(request);
        assert_eq!(output.status.code(), Some(2), "request {request}");
        assert!(output.stdout.is_empty(), "request {request}");
        assert!(!output.stderr.is_empty(), "request {request}");
    }
}

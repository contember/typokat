//! Black-box contract for the public default-library provider probe.

use std::process::Command;

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_typokat");
const PROFILE_SHA256: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";

#[test]
fn library_info_reports_the_embedded_profile_and_production_route() {
    let output = Command::new(BIN)
        .args(["library-info", "--format", "json"])
        .output()
        .expect("run typokat library-info");

    assert_eq!(output.status.code(), Some(0), "{output:#?}");
    assert!(output.stderr.is_empty(), "{output:#?}");

    let observed: Value =
        serde_json::from_slice(&output.stdout).expect("library-info emits valid JSON");
    assert_eq!(
        observed,
        serde_json::json!({
            "schema": 1,
            "profile_sha256": PROFILE_SHA256,
            "file_count": 82,
            "provider_route": "production-default-library"
        })
    );
}

#[test]
fn library_info_rejects_non_json_formats() {
    let output = Command::new(BIN)
        .args(["library-info", "--format", "compact"])
        .output()
        .expect("run typokat library-info");

    assert_eq!(output.status.code(), Some(2), "{output:#?}");
    assert!(output.stdout.is_empty(), "{output:#?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("unknown library-info format 'compact'; expected 'json'"),
        "{output:#?}"
    );
}

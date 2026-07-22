//! External fresh-process gate for canonical frozen-library-base initialization.

use std::process::Command;

const EXPECTED_RECORD: &str = concat!(
    "typokat-library-base-v1 windows=3 samples=30 p95_wall_us_max=120000 ",
    "rss_bytes_max=536870912 route=production-frozen-library-base ",
    "artifact_sha256=af97017b22c9f8ff3726de9dbd49a3039cf70f2dd5a4fd9df9f71328be721dd0 ",
    "projection_identities=1 initializations=1 publications=1 outcome=GO"
);

#[test]
#[ignore = "runs the production frozen-base probe in three fresh-process windows"]
fn canonical_frozen_library_base_retains_release_headroom() {
    let tests = Command::new("python3")
        .args([
            "-m",
            "unittest",
            "tooling/library-base/test_verify.py",
            "-v",
        ])
        .output()
        .expect("run frozen-base coordinator adversary tests");
    assert!(
        tests.status.success(),
        "coordinator tests failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&tests.stdout),
        String::from_utf8_lossy(&tests.stderr)
    );

    let output = Command::new("python3")
        .arg("tooling/library-base/verify.py")
        .output()
        .expect("run frozen-base fresh-process gate");
    assert!(
        output.status.success(),
        "fresh-process gate failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)
            .expect("coordinator record must be UTF-8")
            .trim(),
        EXPECTED_RECORD
    );
    assert!(output.stderr.is_empty(), "coordinator wrote stderr");
}

//! External package-extraction gate for the production default-library artifact.

use std::process::Command;

const EXPECTED_RECORD: &str = concat!(
    "typokat-library-package-v1 clean_roots=2 generations=2 ",
    "artifact_bytes=21003926 ",
    "artifact_sha256=47a8a6fd349f3b3fbb3aae1baccedbc67530edc35227707d79afac5395ca7d2f ",
    "profile_sha256=ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d ",
    "dts_sources=82 licenses=2 cargo_checks=2 build_scripts=0 build_generations=0 ",
    "source_mutations=0"
);

#[test]
#[ignore = "package verifier runs two clean generations, extraction, and cargo check"]
fn clean_generation_and_cargo_package_are_reproducible_and_complete() {
    let tests = Command::new("python3")
        .args([
            "-m",
            "unittest",
            "tooling/library-package/test_verify.py",
            "-v",
        ])
        .output()
        .expect("run package-coordinator adversary tests");
    assert!(
        tests.status.success(),
        "package-coordinator tests failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&tests.stdout),
        String::from_utf8_lossy(&tests.stderr)
    );

    let output = Command::new("python3")
        .arg("tooling/library-package/verify.py")
        .output()
        .expect("run package verifier");
    assert!(
        output.status.success(),
        "package verifier failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)
            .expect("package record must be UTF-8")
            .trim(),
        EXPECTED_RECORD
    );
    assert!(output.stderr.is_empty(), "package verifier wrote stderr");
}

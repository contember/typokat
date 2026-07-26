//! External package-extraction gate for the default-library sources.

use std::process::Command;

const EXPECTED_RECORD: &str = concat!(
    "typokat-library-package-v1 clean_roots=2 ",
    "profile_sha256=ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d ",
    "dts_sources=82 licenses=2 cargo_checks=2 build_scripts=0 ",
    "source_mutations=0"
);

#[test]
#[ignore = "package verifier packages two clean roots, extracts them, and runs cargo check"]
fn cargo_package_ships_every_library_source_and_checks_clean() {
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

//! WU5 acceptance: CI must execute the ignored default-library package gate.

#[test]
fn ci_runs_the_clean_library_package_gate() {
    let workflow = std::fs::read_to_string(".github/workflows/ci.yml")
        .expect("read the checked-in CI workflow");
    let job_start = workflow
        .find("\n  library-package:\n")
        .expect("CI must define the library-package job");
    let mut lines = workflow[job_start + 1..].lines();
    assert_eq!(lines.next(), Some("  library-package:"));
    let job = lines
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        job.contains("cargo test --test library_package_assets"),
        "the library-package job must invoke its integration boundary"
    );
    assert!(
        job.contains("cargo_package_ships_every_library_source_and_checks_clean"),
        "the job must select the current clean-source package verifier"
    );
    assert!(
        job.contains("-- --ignored --exact --nocapture"),
        "the expensive ignored gate must run exactly and expose its evidence"
    );
}

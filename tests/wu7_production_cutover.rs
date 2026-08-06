//! Acceptance for WU7's atomic production default-library cutover.
//!
//! These tests exercise the public and process boundaries after cutover. They pin the unified
//! full-library route, result-bearing worker edges, and the absence of a production prelude bypass.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use typokat::driver::{check_files, check_project, check_source, CheckOutput};
use typokat::frontend::FileInput;

const BIN: &str = env!("CARGO_BIN_EXE_typokat");
const FULL_LIBRARY_CLEAN: &str =
    include_str!("../tooling/full-lib-bench/workloads/fast-clean/main.ts");

fn root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(root.join("Cargo.lock").is_file());
    root
}

fn assert_clean(label: &str, output: &CheckOutput) {
    assert!(
        output.diagnostics.is_empty()
            && output.parse_errors.is_empty()
            && output.incomplete.is_empty(),
        "{label} did not use the complete production library: diagnostics={:?}, parse={:?}, incomplete={:?}",
        output.diagnostics,
        output.parse_errors,
        output.incomplete,
    );
}

#[test]
fn public_single_parallel_and_project_modes_agree_on_the_full_library() {
    let single = check_source(FULL_LIBRARY_CLEAN).expect("single mode initializes the full base");
    assert_clean("single", &single);

    let parallel = check_files(vec![FileInput {
        name: "/wu7/parallel.ts".to_owned(),
        source: FULL_LIBRARY_CLEAN.to_owned(),
    }])
    .expect("parallel mode initializes the full base before rayon");
    assert_eq!(parallel.len(), 1);
    assert_clean("parallel", &parallel[0].output);

    let project = check_project(vec![FileInput {
        name: "/wu7/project.ts".to_owned(),
        source: FULL_LIBRARY_CLEAN.to_owned(),
    }])
    .expect("project mode initializes the full base");
    assert_eq!(project.len(), 1);
    assert_clean("project", &project[0].output);
}

#[test]
fn parallel_mode_preserves_filename_derived_declaration_semantics() {
    let source = "export as namespace Valid; export = Valid; declare function Valid(): void;";
    let input = || FileInput {
        name: "/wu7/valid.d.ts".to_owned(),
        source: source.to_owned(),
    };
    let parallel =
        check_files(vec![input()]).expect("parallel declaration mode initializes the full base");
    let project =
        check_project(vec![input()]).expect("project declaration mode initializes the full base");
    assert_eq!(parallel.len(), 1);
    assert_eq!(project.len(), 1);

    let shape = |output: &CheckOutput| {
        (
            format!("{:?}", output.diagnostics),
            output.parse_errors.clone(),
            output
                .incomplete
                .iter()
                .map(|surface| surface.id.clone())
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        shape(&parallel[0].output),
        shape(&project[0].output),
        "check_files must classify the provided .d.ts name instead of inventing input.ts"
    );
}

#[test]
fn production_sources_have_explicit_cli_and_provider_routes_and_no_prelude_asset() {
    let root = root();
    assert!(
        !root.join("crates/typokat-check/src/prelude.ts").exists(),
        "the minimal prelude asset must be deleted only with the atomic cutover"
    );

    let checker = fs::read_to_string(root.join("crates/typokat-check/src/check/checker/mod.rs"))
        .expect("read checker source");
    let checker_api = fs::read_to_string(root.join("crates/typokat-check/src/check/mod.rs"))
        .expect("read checker API source");
    let driver = fs::read_to_string(root.join("crates/typokat-driver/src/driver.rs"))
        .expect("read driver source");
    let main = fs::read_to_string(root.join("src/main.rs")).expect("read CLI source");
    let facade = fs::read_to_string(root.join("src/lib.rs")).expect("read facade source");
    let conformance =
        fs::read_to_string(root.join("tests/conformance.rs")).expect("read conformance source");

    let production_sources = production_rust_sources(&root);
    for forbidden in [
        "PRELUDE_SOURCE",
        "bootstrap_trusted_prelude",
        "include_str!(\"../../prelude.ts\")",
    ] {
        let offenders = production_sources
            .iter()
            .filter(|(_, source)| source.contains(forbidden))
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>();
        assert!(
            offenders.is_empty(),
            "retired production bootstrap remains reachable: {forbidden}: {offenders:?}"
        );
    }
    for forbidden in ["check_source_with_library", "check_project_with_library"] {
        let offenders = production_sources
            .iter()
            .filter(|(_, source)| source.contains(forbidden))
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>();
        assert!(
            offenders.is_empty(),
            "the temporary second public route must disappear at cutover: {forbidden}: {offenders:?}"
        );
    }
    let package_path_offenders = package_text_files(&root)
        .into_iter()
        .filter(|(_, source)| source.contains("crates/typokat-check/src/prelude.ts"))
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    assert!(
        package_path_offenders.is_empty(),
        "packaged sources still advertise the retired prelude asset: {package_path_offenders:?}"
    );
    assert!(
        !conformance.contains("FixtureBase")
            && !conformance.contains("check_source_with_library")
            && !conformance.contains("check_project_with_library"),
        "conformance must use the same single production route as the CLI"
    );
    assert!(
        !facade.contains("pub use typokat_check::check"),
        "the facade must not expose a raw prelude-backed checker bypass"
    );
    assert!(
        !checker.contains("pub fn check_program<'ast>")
            && !checker.contains("pub fn check_project_programs<'ast>")
            && !checker_api.contains("pub use checker::check_project_programs")
            && !checker_api.contains("pub use checker::{check_program"),
        "raw prelude-era checker entry points must be private test support or removed"
    );
    for forbidden in [
        ".expect(\"spawn check worker\")",
        ".expect(\"check worker panicked\")",
        "std::panic::resume_unwind",
    ] {
        assert!(
            !driver.contains(forbidden),
            "worker failure must return through the typed infrastructure boundary: {forbidden}"
        );
    }
    assert!(
        main.contains("failed to initialize embedded TypeScript 6.0.3 library:"),
        "CLI exit 2 needs the stable initialization-failure prefix"
    );
    assert!(
        !main.contains("CURRENT_LIBRARY_ROUTE") && !main.contains("\"production-default-library\""),
        "library-info must derive its route from the initialized production provider"
    );
    assert!(
        main.contains("production_library_route"),
        "library-info must attest through the ordinary driver singleton"
    );
    assert!(
        main.contains("check_project_once") && main.contains("production_cli_route"),
        "ordinary CLI checks need the explicit complete-source lifecycle"
    );
}

#[test]
fn cli_full_library_clean_probe_is_silent() {
    let output = run_cli_probe("full-library-clean", "ts", FULL_LIBRARY_CLEAN);

    assert_eq!(output.status.code(), Some(0), "{output:#?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:#?}"
    );
}

#[test]
fn cli_full_library_error_probe_reports_the_user_error() {
    let source = format!("{FULL_LIBRARY_CLEAN}\nexport const wu7Wrong: number = \"wrong\";\n");
    let output = run_cli_probe("full-library-error", "ts", &source);

    assert_eq!(output.status.code(), Some(1), "{output:#?}");
    assert!(output.stdout.is_empty(), "{output:#?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("error TK2322"),
        "{output:#?}"
    );
}

#[test]
fn cli_preserves_declaration_filename_semantics() {
    let source = "export as namespace Valid; export = Valid; declare function Valid(): void;";
    let output = run_cli_probe("valid-declaration", "d.ts", source);

    assert_eq!(output.status.code(), Some(3), "{output:#?}");
    assert!(output.stdout.is_empty(), "{output:#?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("incomplete[decl/namespace-export/self]")
            && stderr.contains("incomplete[decl/export-assignment/self]"),
        "{output:#?}"
    );
    assert!(
        !stderr.contains("error[TK") && !stderr.contains("error TK"),
        "{output:#?}"
    );
}

fn run_cli_probe(tag: &str, extension: &str, source: &str) -> std::process::Output {
    let path = unique_temp_path(tag, extension);
    fs::write(&path, source).expect("write WU7 CLI probe");
    let output = Command::new(BIN)
        .args(["check", "--format", "compact"])
        .arg(&path)
        .output()
        .expect("run typokat check");
    let _ = fs::remove_file(path);
    output
}

fn unique_temp_path(tag: &str, extension: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "typokat-{tag}-{}-{nanos}.{extension}",
        std::process::id()
    ))
}

fn production_rust_sources(root: &std::path::Path) -> Vec<(String, String)> {
    text_files_under(root, &[root.join("src"), root.join("crates")])
        .into_iter()
        .filter(|(path, _)| {
            path.ends_with(".rs")
                && !path.contains("/tests/")
                && !path.ends_with("_spec.rs")
                && !path.ends_with("/tests.rs")
                && !path.ends_with("/test_support.rs")
        })
        .collect()
}

fn package_text_files(root: &std::path::Path) -> Vec<(String, String)> {
    text_files_under(
        root,
        &[
            root.join("crates/typokat-check"),
            root.join("crates/typokat-driver"),
            root.join("crates/typokat-library"),
        ],
    )
    .into_iter()
    .filter(|(path, _)| !path.ends_with(".d.ts"))
    .collect()
}

fn text_files_under(root: &std::path::Path, roots: &[PathBuf]) -> Vec<(String, String)> {
    let mut pending = roots.to_vec();
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("read source directory") {
            let path = entry.expect("source directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(source) = fs::read_to_string(&path) {
                files.push((
                    path.strip_prefix(root)
                        .expect("source under repository root")
                        .to_string_lossy()
                        .into_owned(),
                    source,
                ));
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

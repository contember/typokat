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
        "{label} did not use the production checker session: diagnostics={:?}, parse={:?}, incomplete={:?}",
        output.diagnostics,
        output.parse_errors,
        output.incomplete,
    );
}

#[test]
fn public_single_parallel_and_project_modes_agree_on_the_full_library() {
    let single = check_source(FULL_LIBRARY_CLEAN).expect("single mode initializes one session");
    assert_clean("single", &single);

    let parallel = check_files(vec![FileInput {
        name: "/wu7/parallel.ts".to_owned(),
        source: FULL_LIBRARY_CLEAN.to_owned(),
    }])
    .expect("parallel mode initializes the product before rayon");
    assert_eq!(parallel.len(), 1);
    assert_clean("parallel", &parallel[0].output);

    let project = check_project(vec![FileInput {
        name: "/wu7/project.ts".to_owned(),
        source: FULL_LIBRARY_CLEAN.to_owned(),
    }])
    .expect("project mode initializes one session");
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
    let parallel = check_files(vec![input()]).expect("parallel declaration mode uses one session");
    let project = check_project(vec![input()]).expect("project declaration mode uses one session");
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
fn production_sources_use_only_checker_session_lifecycle() {
    let root = root();
    assert!(
        !root.join("crates/typokat-check/src/prelude.ts").exists(),
        "the minimal prelude asset must be deleted only with the atomic cutover"
    );
    for retired in [
        "tests/wu6_complete_source_cli.rs",
        "tests/library_checkpoint_release_compile.rs",
        "tests/project_local_library_candidate.rs",
        "tests/wu4_force_all_audit_boundary.rs",
        "examples/one_pass_probe.rs",
    ] {
        assert!(
            !root.join(retired).exists(),
            "retired lifecycle-only surface still exists: {retired}"
        );
    }

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

    let lifecycle_sources = lifecycle_surface_sources(&root);
    for forbidden in [
        "PRELUDE_SOURCE",
        "bootstrap_trusted_prelude",
        "include_str!(\"../../prelude.ts\")",
        "FrozenLibrary",
        "LibraryBinderCheckpoint",
        "CollisionReplay",
        "PrivateCollisionEpoch",
        "CompleteSource",
        "complete_source",
        "ReplayDependencyTrace",
        "ReplayOwner",
        "OwnedLibraryRuntimeState",
        "PrivateCombinedRecordTicket",
        "LibraryCompiler",
        "library_compiler",
        "fork_sparse",
        "check_project_programs_with_library",
        "check_private_project_programs_with_library",
        "check_project_programs_with_owned_library",
        "check_program_with_owned_library",
        "check_complete_source",
        "production_cli_route",
        "production-complete-source-once",
        "production-default-library",
        "project-local-driver-shadow",
        "SharedColdParity",
        "shared_executions",
        "cold_executions",
        "conformance_full_shared_cold_parity",
        "ProbeSetupControl",
        "retained_setup_control_bytes",
        "semantic_base_initializations",
        "library_full_source_products_consumed",
        "library_checkpoint_products_consumed",
        "compiler_compiles",
        "compiler_parses",
        "compiler_binds",
        "compiler_checks",
        "user_binds",
        "user_checks",
        "user_delta_forks",
        "library_frontend_entries",
        "library_parse_units",
        "library_bind_batches",
        "library_ast_bind_units",
        "LibraryProjectBinderContinuation",
        "packaged_library_source_inputs",
        "declaration_product_for_shadow",
        "declaration_product_for_session_bench",
        "continue_library_project_binder",
        "CompleteSourceDeclarationOracleForTest",
        "CompleteSourceBlackBoxOracleForTest",
        "PreparedProjectDeclarationUniverseForTest",
        "ProjectDeclarationUniverseMetricsForTest",
        "replay_index",
        "private_collision_",
        "combined_user_source",
        "capture_compact_replay_dependencies",
        "events_library",
        "lexical_events_library",
        "collision_preflight",
    ] {
        let offenders = lifecycle_sources
            .iter()
            .filter(|(_, source)| source.contains(forbidden))
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>();
        assert!(
            offenders.is_empty(),
            "retired production lifecycle remains reachable: {forbidden}: {offenders:?}"
        );
    }
    for forbidden in ["check_source_with_library", "check_project_with_library"] {
        let offenders = lifecycle_sources
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
            && !conformance.contains("SharedColdParity")
            && !conformance.contains("check_project_programs_with_library"),
        "conformance must use the same production CheckerSession lifecycle"
    );
    assert!(
        !facade.contains("pub use typokat_check::check"),
        "the facade must not expose a raw checker bypass"
    );
    assert!(
        !checker.contains("pub fn check_program<'ast>")
            && !checker.contains("pub fn check_project_programs<'ast>")
            && !checker_api.contains("pub use checker::check_project_programs")
            && !checker_api.contains("pub use checker::{check_program"),
        "raw checker entry points must be private test support or removed"
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
        main.contains("production_library_route")
            && main.contains("check_project_once")
            && main.contains("\"provider_route\": route"),
        "CLI checks and library-info must attest one production CheckerSession lifecycle"
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

fn lifecycle_surface_sources(root: &std::path::Path) -> Vec<(String, String)> {
    let mut sources = text_files_under(
        root,
        &[
            root.join("src"),
            root.join("crates"),
            root.join("tests"),
            root.join("tooling"),
            root.join("examples"),
            root.join(".github"),
        ],
    );
    sources.push((
        "Cargo.toml".to_owned(),
        fs::read_to_string(root.join("Cargo.toml")).expect("read root manifest"),
    ));
    sources.retain(|(path, _)| {
        (path.ends_with(".rs")
            || path.ends_with(".py")
            || path.ends_with(".toml")
            || path.ends_with(".yml")
            || path.ends_with(".yaml"))
            && path != "tests/wu7_production_cutover.rs"
    });
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    sources
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

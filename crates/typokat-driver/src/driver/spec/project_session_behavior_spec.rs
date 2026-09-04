//! Observable driver behavior through the production checker-session route.

use super::{
    check_bundler_project_once, check_files, check_project, check_project_once, check_source,
    CheckOutput, FileReport,
};
use crate::frontend::{FileInput, ProjectRoot};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_PROJECT_ORDINAL: AtomicU64 = AtomicU64::new(0);

struct TempBundlerProject {
    directory: PathBuf,
}

impl TempBundlerProject {
    fn new(case: &str, files: &[(&str, &str)]) -> (Self, Vec<FileInput>) {
        let ordinal = TEMP_PROJECT_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "typokat-session-{case}-{}-{ordinal}",
            std::process::id(),
        ));
        fs::create_dir_all(&directory).expect("create temporary Bundler project");
        let inputs = files
            .iter()
            .map(|(name, source)| {
                let path = directory.join(name);
                fs::write(&path, source).expect("write temporary Bundler source");
                input(&path.to_string_lossy(), source)
            })
            .collect();
        (Self { directory }, inputs)
    }
}

impl Drop for TempBundlerProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn input(name: &str, source: &str) -> FileInput {
    FileInput {
        name: name.to_owned(),
        source: source.to_owned(),
    }
}

fn fixture_inputs(project: &Path, names: &[&str]) -> Vec<FileInput> {
    names
        .iter()
        .map(|name| {
            let path = project.join(name);
            FileInput {
                name: path.to_string_lossy().into_owned(),
                source: fs::read_to_string(path).expect("read driver fixture"),
            }
        })
        .collect()
}

fn explicit_fixture_inputs(project: &Path, names: &[&str]) -> Vec<FileInput> {
    names
        .iter()
        .map(|name| FileInput {
            name: (*name).to_owned(),
            source: fs::read_to_string(project.join(name)).expect("read explicit-route fixture"),
        })
        .collect()
}

fn bundler_roots(inputs: &[FileInput]) -> Vec<ProjectRoot> {
    inputs
        .iter()
        .map(|input| ProjectRoot {
            identity: input.name.clone(),
            path: PathBuf::from(&input.name),
            exists: true,
        })
        .collect()
}

fn diagnostic_codes(reports: &[FileReport]) -> Vec<&str> {
    reports
        .iter()
        .flat_map(|report| &report.output.diagnostics)
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn output_codes(output: &CheckOutput) -> Vec<&str> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn report_shape(reports: &[FileReport]) -> Vec<String> {
    reports
        .iter()
        .map(|report| {
            let diagnostics = report
                .output
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    format!(
                        "{}:{:?}:{}:{:?}:{:?}",
                        diagnostic.code.as_str(),
                        diagnostic.severity,
                        diagnostic.message,
                        diagnostic.span,
                        diagnostic.elaboration(),
                    )
                })
                .collect::<Vec<_>>();
            let incomplete = report
                .output
                .incomplete
                .iter()
                .map(|record| format!("{}:{:?}:{}", record.id, record.span, record.context))
                .collect::<Vec<_>>();
            format!(
                "{}|parse={:?}|diagnostics={diagnostics:?}|incomplete={incomplete:?}|notices={:?}",
                report.name, report.output.parse_errors, report.output.project_notices,
            )
        })
        .collect()
}

fn semantic_shape(output: &CheckOutput) -> (Vec<String>, Vec<String>) {
    let diagnostics = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}:{:?}:{}:{:?}:{:?}",
                diagnostic.code.as_str(),
                diagnostic.severity,
                diagnostic.message,
                diagnostic.span,
                diagnostic.elaboration(),
            )
        })
        .collect();
    let incomplete = output
        .incomplete
        .iter()
        .map(|record| format!("{}:{:?}:{}", record.id, record.span, record.context))
        .collect();
    (diagnostics, incomplete)
}

#[test]
fn public_source_file_and_project_shapes_preserve_diagnostics_and_order() {
    let source = check_source("const bad: string = 1;\n").expect("check one source");
    assert_eq!(output_codes(&source), ["TK2322"]);

    let files = check_files(vec![
        input("/session/files-a.ts", "const bad: string = 1;\n"),
        input("/session/files-b.ts", "const ok: number = 1;\n"),
    ])
    .expect("check independent files");
    assert_eq!(
        files
            .iter()
            .map(|report| report.name.as_str())
            .collect::<Vec<_>>(),
        ["/session/files-a.ts", "/session/files-b.ts"],
    );
    assert_eq!(output_codes(&files[0].output), ["TK2322"]);
    assert!(files[1].output.diagnostics.is_empty());

    let project = vec![
        input(
            "/session/00_augment.ts",
            "interface Array<T> { sessionHead(): T; }\n",
        ),
        input(
            "/session/99_use.ts",
            "const ok: number = [1].sessionHead();\nconst bad: string = [1].sessionHead();\n",
        ),
    ];
    let repeated = check_project(project.clone()).expect("check repeated project mode");
    let standalone = check_project_once(project).expect("check standalone project mode");
    assert_eq!(diagnostic_codes(&repeated), ["TK2322"]);
    assert_eq!(report_shape(&standalone), report_shape(&repeated));
    assert_eq!(
        super::production_library_route().expect("attest production library route"),
        "production-checker-session",
    );
}

#[test]
fn bundler_default_reexport_notice_is_reported_without_semantic_leakage() {
    let project_directory = typokat_core::test_support::repository_root()
        .join("tests/cases/b15_default_module_slots/22c_source_default_as_default");
    let inputs = fixture_inputs(
        &project_directory,
        &["barrel.ts", "consumer.ts", "source.ts"],
    );
    let roots = bundler_roots(&inputs);
    let run = check_bundler_project_once(inputs, project_directory, roots)
        .expect("check supported default re-export");

    assert_eq!(run.inventory.notices.len(), 1);
    assert!(run.inventory.notices[0].ends_with("barrel.ts:1:1 ./source.js default->default"));
    assert!(run
        .reports
        .iter()
        .all(|report| report.output.diagnostics.is_empty()));
    assert_eq!(run.reports[0].output.project_notices, run.inventory.notices);
    assert!(run.reports[1..]
        .iter()
        .all(|report| report.output.project_notices.is_empty()));
}

#[test]
fn admitted_bundler_default_module_runs_through_the_session() {
    let project_directory = typokat_core::test_support::repository_root()
        .join("tests/cases/b15_default_module_slots/01_named_class");
    let inputs = fixture_inputs(&project_directory, &["consumer.ts", "source.ts"]);
    let roots = bundler_roots(&inputs);
    let run = check_bundler_project_once(inputs, project_directory, roots)
        .expect("check admitted Bundler defaults");

    assert!(run.inventory.notices.is_empty(), "{:#?}", run.inventory);
    assert_eq!(diagnostic_codes(&run.reports), ["TK2322"]);
}

#[test]
fn explicit_project_modes_preserve_import_diagnostics() {
    let cases = [
        (
            "missing module",
            vec![input(
                "/session/import-errors/missing-module.ts",
                "import { missing } from './absent.js';\nmissing;\n",
            )],
            vec!["TK2307"],
        ),
        (
            "missing named export",
            vec![
                input(
                    "/session/import-errors/source.ts",
                    "export const present = 1;\n",
                ),
                input(
                    "/session/import-errors/missing-export.ts",
                    "import { missing } from './source';\nmissing;\n",
                ),
            ],
            vec!["TK2305"],
        ),
    ];

    for (case, inputs, expected_codes) in cases {
        let standalone = check_project_once(inputs.clone()).expect("standalone import diagnostics");
        assert_eq!(
            diagnostic_codes(&standalone),
            expected_codes,
            "standalone {case}"
        );

        let repeated = check_project(inputs).expect("repeated-project import diagnostics");
        assert_eq!(
            diagnostic_codes(&repeated),
            expected_codes,
            "repeated {case}"
        );
    }
}

#[test]
fn bundler_mode_preserves_import_diagnostics() {
    let cases = [
        (
            "missing-module",
            vec![(
                "missing-module.ts",
                "import { missing } from './absent.js';\nmissing;\n",
            )],
            vec!["TK2307"],
        ),
        (
            "missing-export",
            vec![
                ("source.ts", "export const present = 1;\n"),
                (
                    "missing-export.ts",
                    "import { missing } from './source.js';\nmissing;\n",
                ),
            ],
            vec!["TK2305"],
        ),
    ];

    for (case, files, expected_codes) in cases {
        let (project, inputs) = TempBundlerProject::new(case, &files);
        let roots = bundler_roots(&inputs);
        let run = check_bundler_project_once(inputs, project.directory.clone(), roots)
            .expect("Bundler import diagnostics");
        assert_eq!(diagnostic_codes(&run.reports), expected_codes, "{case}");
    }
}

#[test]
fn explicit_modes_preserve_semantics_and_route_specific_notices_in_both_orders() {
    let root = typokat_core::test_support::repository_root();
    let projects: [(PathBuf, [&str; 2], &[&str]); 2] = [
        (
            root.join("tests/cases/b15_default_module_slots/01_named_class"),
            ["source.ts", "consumer.ts"],
            &[
                "unsupported-module-form default-import consumer.ts:1:1 ./source.js",
                "unsupported-module-form default-export source.ts:1:1",
            ],
        ),
        (
            root.join("tests/cases/b15_acyclic_source_reexports/route_baseline"),
            ["barrel.ts", "source.ts"],
            &["unsupported-module-form source-reexport barrel.ts:1:1 ./source.js"],
        ),
    ];

    for (project, forward, standalone_notices) in projects {
        for names in [forward, [forward[1], forward[0]]] {
            let inputs = explicit_fixture_inputs(&project, &names);
            let standalone = check_project_once(inputs.clone()).expect("standalone explicit mode");
            let repeated = check_project(inputs).expect("repeated explicit mode");
            assert_eq!(standalone.len(), repeated.len(), "{project:?} {names:?}");
            for (standalone_report, repeated_report) in standalone.iter().zip(&repeated) {
                assert_eq!(
                    semantic_shape(&standalone_report.output),
                    semantic_shape(&repeated_report.output),
                    "{project:?} {names:?}",
                );
            }
            assert_eq!(
                standalone[0].output.project_notices,
                standalone_notices
                    .iter()
                    .map(|notice| (*notice).to_owned())
                    .collect::<Vec<_>>(),
                "{project:?} {names:?}",
            );
            assert!(standalone[1..]
                .iter()
                .all(|report| report.output.project_notices.is_empty()));
            assert!(repeated
                .iter()
                .all(|report| report.output.project_notices.is_empty()));
        }
    }
}

#[test]
fn each_project_call_uses_a_fresh_checker_session() {
    let repeated_with_augmentation = check_project(vec![
        input(
            "/session/repeated-augment.ts",
            "interface Array<T> { repeatedFresh(): T; }\n",
        ),
        input(
            "/session/repeated-use.ts",
            "const value: number = [1].repeatedFresh();\n",
        ),
    ])
    .expect("first repeated-project session");
    assert!(diagnostic_codes(&repeated_with_augmentation).is_empty());
    let repeated_without_augmentation = check_project(vec![input(
        "/session/repeated-isolated.ts",
        "const value: number = [1].repeatedFresh();\n",
    )])
    .expect("second repeated-project session");
    assert!(repeated_without_augmentation
        .iter()
        .flat_map(|report| &report.output.diagnostics)
        .any(|diagnostic| diagnostic.message.contains("repeatedFresh")));

    let standalone_with_augmentation = check_project_once(vec![
        input(
            "/session/standalone-augment.ts",
            "interface Array<T> { standaloneFresh(): T; }\n",
        ),
        input(
            "/session/standalone-use.ts",
            "const value: number = [1].standaloneFresh();\n",
        ),
    ])
    .expect("first standalone session");
    assert!(diagnostic_codes(&standalone_with_augmentation).is_empty());
    let standalone_without_augmentation = check_project_once(vec![input(
        "/session/standalone-isolated.ts",
        "const value: number = [1].standaloneFresh();\n",
    )])
    .expect("second standalone session");
    assert!(standalone_without_augmentation
        .iter()
        .flat_map(|report| &report.output.diagnostics)
        .any(|diagnostic| diagnostic.message.contains("standaloneFresh")));
}

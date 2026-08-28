//! RED acceptance for WU5's project-local driver shadow.
//!
//! Public dispatch remains unchanged until WU7. These tests exercise the one candidate driver
//! continuation that WU6 will compare with the complete-source control.

use super::{
    check_bundler_project_once, check_project, check_project_local_shadow_for_test,
    check_project_once, DriverError, FileReport, ProjectLocalShadowRouteForTest,
};
use crate::check::checker::project_local::{ObservationEventKindForTest, ObservationScopeForTest};
use crate::frontend::{FileInput, ProjectRoot};
use crate::library::{LibraryBaseProvider, LibraryInitError};
use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn input(name: &str, source: &str) -> FileInput {
    FileInput {
        name: name.to_owned(),
        source: source.to_owned(),
    }
}

fn fixture_inputs(project: &std::path::Path, names: &[&str]) -> Vec<FileInput> {
    names
        .iter()
        .map(|name| {
            let path = project.join(name);
            FileInput {
                name: path.to_string_lossy().into_owned(),
                source: fs::read_to_string(path).expect("read WU5 shadow fixture"),
            }
        })
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

fn diagnostic_codes(reports: &[FileReport]) -> Vec<&str> {
    reports
        .iter()
        .flat_map(|report| &report.output.diagnostics)
        .map(|diagnostic| diagnostic.code.as_str())
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

#[test]
fn project_local_shadow_matches_complete_source_reports_without_switching_public_dispatch() {
    let inputs = vec![
        input(
            "/wu5/00_augment.ts",
            "interface Array<T> { wu5Head(): T; }\n",
        ),
        input(
            "/wu5/99_use.ts",
            "const ok: number = [1].wu5Head();\nconst bad: string = [1].wu5Head();\n",
        ),
    ];
    let legacy = check_project_once(inputs.clone()).expect("complete-source shadow control");
    let provider = LibraryBaseProvider::new();
    let observations = ObservationScopeForTest::start();
    let candidate = check_project_local_shadow_for_test(
        inputs,
        ProjectLocalShadowRouteForTest::Standalone,
        || provider.declaration_product_for_shadow(),
    )
    .expect("project-local standalone shadow")
    .reports;
    let receipt = observations.finish();

    assert_eq!(report_shape(&candidate), report_shape(&legacy));
    assert_eq!(
        super::production_cli_route(),
        "production-complete-source-once"
    );
    assert_eq!(
        receipt
            .events
            .iter()
            .filter(|event| matches!(
                event.kind,
                ObservationEventKindForTest::SessionCreated { .. }
            ))
            .count(),
        1,
    );
    assert_eq!(
        receipt
            .events
            .iter()
            .filter(|event| matches!(
                event.kind,
                ObservationEventKindForTest::SessionDropped { .. }
            ))
            .count(),
        1,
    );
    assert!(receipt
        .session_weak
        .iter()
        .chain(&receipt.cache_weak)
        .all(|weak| !weak.upgraded_at_finish));
}

#[test]
fn project_local_shadow_preserves_operation_specific_initialization_precedence() {
    let malformed = vec![input(
        "/wu5/malformed.ts",
        "declare namespace Broken { export = Broken; }\n",
    )];

    let repeated_calls = Cell::new(0);
    let repeated = check_project_local_shadow_for_test(
        malformed.clone(),
        ProjectLocalShadowRouteForTest::RepeatedProject,
        || {
            repeated_calls.set(repeated_calls.get() + 1);
            Err(Arc::new(LibraryInitError::injected_initialization_failure(
                "WU5 repeated failure".to_owned(),
            )))
        },
    );
    assert!(matches!(
        repeated,
        Err(DriverError::LibraryInitialization(_))
    ));
    assert_eq!(repeated_calls.get(), 1, "repeated checks acquire first");

    let standalone_calls = Cell::new(0);
    let standalone = check_project_local_shadow_for_test(
        malformed.clone(),
        ProjectLocalShadowRouteForTest::Standalone,
        || {
            standalone_calls.set(standalone_calls.get() + 1);
            Err(Arc::new(LibraryInitError::injected_initialization_failure(
                "WU5 standalone failure".to_owned(),
            )))
        },
    )
    .expect("standalone reports malformed user input before product acquisition");
    assert_eq!(standalone_calls.get(), 0);
    assert_eq!(standalone.reports.len(), 1);
    assert_eq!(standalone.reports[0].output.parse_errors.len(), 1);

    let bundler_calls = Cell::new(0);
    let bundler = check_project_local_shadow_for_test(
        malformed,
        ProjectLocalShadowRouteForTest::Bundler {
            project_directory: PathBuf::from("/wu5"),
            roots: vec![ProjectRoot {
                identity: "malformed.ts".to_owned(),
                path: PathBuf::from("/wu5/malformed.ts"),
                exists: true,
            }],
        },
        || {
            bundler_calls.set(bundler_calls.get() + 1);
            Err(Arc::new(LibraryInitError::injected_initialization_failure(
                "WU5 bundler failure".to_owned(),
            )))
        },
    )
    .expect("Bundler reports malformed user input before product acquisition");
    assert_eq!(bundler_calls.get(), 0);
    assert_eq!(bundler.reports[0].output.parse_errors.len(), 1);

    let repeated_empty_calls = Cell::new(0);
    let repeated_empty = check_project_local_shadow_for_test(
        Vec::new(),
        ProjectLocalShadowRouteForTest::RepeatedProject,
        || {
            repeated_empty_calls.set(repeated_empty_calls.get() + 1);
            Err(Arc::new(LibraryInitError::injected_initialization_failure(
                "WU5 repeated empty failure".to_owned(),
            )))
        },
    );
    assert!(matches!(
        repeated_empty,
        Err(DriverError::LibraryInitialization(_))
    ));
    assert_eq!(repeated_empty_calls.get(), 1);

    for route in [
        ProjectLocalShadowRouteForTest::Standalone,
        ProjectLocalShadowRouteForTest::Bundler {
            project_directory: PathBuf::from("/wu5"),
            roots: Vec::new(),
        },
    ] {
        let calls = Cell::new(0);
        let empty = check_project_local_shadow_for_test(Vec::new(), route, || {
            calls.set(calls.get() + 1);
            Err(Arc::new(LibraryInitError::injected_initialization_failure(
                "WU5 deferred empty failure".to_owned(),
            )))
        })
        .expect("standalone and Bundler empty projects return before acquisition");
        assert!(empty.reports.is_empty());
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn project_local_shadow_finalizes_default_reexport_notice_before_the_gate() {
    let project_directory = typokat_core::test_support::repository_root()
        .join("tests/cases/b15_default_module_slots/22c_source_default_as_default");
    let inputs = ["barrel.ts", "consumer.ts", "source.ts"]
        .into_iter()
        .map(|name| {
            let path = project_directory.join(name);
            FileInput {
                name: path.to_string_lossy().into_owned(),
                source: fs::read_to_string(path).expect("read default-reexport witness"),
            }
        })
        .collect::<Vec<_>>();
    let roots = inputs
        .iter()
        .map(|input| ProjectRoot {
            identity: input.name.clone(),
            path: PathBuf::from(&input.name),
            exists: true,
        })
        .collect();
    let provider = LibraryBaseProvider::new();
    let acquisitions = Cell::new(0);
    let run = check_project_local_shadow_for_test(
        inputs,
        ProjectLocalShadowRouteForTest::Bundler {
            project_directory,
            roots,
        },
        || {
            acquisitions.set(acquisitions.get() + 1);
            provider.declaration_product_for_shadow()
        },
    )
    .expect("supported default re-export reaches the project-local session");

    assert_eq!(acquisitions.get(), 0);
    assert_eq!(run.inventory.notices.len(), 1);
    assert!(run.inventory.notices[0].ends_with("barrel.ts:1:1 ./source.js default->default"));
    assert!(run
        .reports
        .iter()
        .flat_map(|report| &report.output.diagnostics)
        .next()
        .is_none());
    assert_eq!(run.reports[0].output.project_notices, run.inventory.notices);
    assert!(run.reports[1..]
        .iter()
        .all(|report| report.output.project_notices.is_empty()));
}

#[test]
fn project_local_shadow_runs_admitted_bundler_defaults_through_the_session() {
    let project_directory = typokat_core::test_support::repository_root()
        .join("tests/cases/b15_default_module_slots/01_named_class");
    let inputs = fixture_inputs(&project_directory, &["consumer.ts", "source.ts"]);
    let roots = inputs
        .iter()
        .map(|input| ProjectRoot {
            identity: input.name.clone(),
            path: PathBuf::from(&input.name),
            exists: true,
        })
        .collect();
    let provider = LibraryBaseProvider::new();
    let acquisitions = Cell::new(0);
    let observations = ObservationScopeForTest::start();
    let run = check_project_local_shadow_for_test(
        inputs,
        ProjectLocalShadowRouteForTest::Bundler {
            project_directory,
            roots,
        },
        || {
            acquisitions.set(acquisitions.get() + 1);
            provider.declaration_product_for_shadow()
        },
    )
    .expect("admitted Bundler defaults reach the project-local session");
    let receipt = observations.finish();

    assert_eq!(acquisitions.get(), 1);
    assert!(run.inventory.notices.is_empty(), "{:#?}", run.inventory);
    assert_eq!(
        run.reports
            .iter()
            .flat_map(|report| &report.output.diagnostics)
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2322"],
    );
    assert_eq!(
        receipt
            .events
            .iter()
            .filter(|event| matches!(
                event.kind,
                ObservationEventKindForTest::SessionCreated { .. }
            ))
            .count(),
        1,
    );
}

#[test]
fn project_local_shadow_preserves_reportable_import_diagnostics() {
    let cases = [
        (
            "missing module",
            vec![input(
                "/wu5/import-errors/missing-module.ts",
                "import { missing } from './absent.js';\nmissing;\n",
            )],
            vec!["TK2307"],
        ),
        (
            "missing named export",
            vec![
                input(
                    "/wu5/import-errors/source.ts",
                    "export const present = 1;\n",
                ),
                input(
                    "/wu5/import-errors/missing-export.ts",
                    "import { missing } from './source.js';\nmissing;\n",
                ),
            ],
            vec!["TK2305"],
        ),
    ];
    let provider = LibraryBaseProvider::new();

    for (case, inputs, expected_codes) in cases {
        let standalone_control =
            check_project_once(inputs.clone()).expect("standalone import-error control");
        assert_eq!(
            diagnostic_codes(&standalone_control),
            expected_codes,
            "{case}"
        );
        let standalone = check_project_local_shadow_for_test(
            inputs.clone(),
            ProjectLocalShadowRouteForTest::Standalone,
            || provider.declaration_product_for_shadow(),
        )
        .expect("standalone import-error shadow");
        assert_eq!(
            report_shape(&standalone.reports),
            report_shape(&standalone_control),
            "standalone {case}",
        );

        let repeated_control =
            check_project(inputs.clone()).expect("repeated import-error control");
        assert_eq!(
            diagnostic_codes(&repeated_control),
            expected_codes,
            "{case}"
        );
        let repeated = check_project_local_shadow_for_test(
            inputs,
            ProjectLocalShadowRouteForTest::RepeatedProject,
            || provider.declaration_product_for_shadow(),
        )
        .expect("repeated import-error shadow");
        assert_eq!(
            report_shape(&repeated.reports),
            report_shape(&repeated_control),
            "repeated {case}",
        );
    }
}

#[test]
fn project_local_shadow_preserves_bundler_import_diagnostics() {
    let project_directory = PathBuf::from("/wu5/bundler-import-errors");
    let cases = [
        (
            "missing module",
            vec![input(
                "/wu5/bundler-import-errors/missing-module.ts",
                "import { missing } from './absent.js';\nmissing;\n",
            )],
            vec!["TK2307"],
        ),
        (
            "missing named export",
            vec![
                input(
                    "/wu5/bundler-import-errors/source.ts",
                    "export const present = 1;\n",
                ),
                input(
                    "/wu5/bundler-import-errors/missing-export.ts",
                    "import { missing } from './source.js';\nmissing;\n",
                ),
            ],
            vec!["TK2305"],
        ),
    ];
    let provider = LibraryBaseProvider::new();

    for (case, inputs, expected_codes) in cases {
        let roots = bundler_roots(&inputs);
        let control =
            check_bundler_project_once(inputs.clone(), project_directory.clone(), roots.clone())
                .expect("Bundler import-error control");
        assert_eq!(diagnostic_codes(&control.reports), expected_codes, "{case}");
        let candidate = check_project_local_shadow_for_test(
            inputs,
            ProjectLocalShadowRouteForTest::Bundler {
                project_directory: project_directory.clone(),
                roots,
            },
            || provider.declaration_product_for_shadow(),
        )
        .expect("Bundler import-error shadow");

        assert_eq!(candidate.inventory, control.inventory, "{case}");
        assert_eq!(
            report_shape(&candidate.reports),
            report_shape(&control.reports),
            "Bundler {case}",
        );
    }
}

#[test]
fn project_local_shadow_preserves_each_explicit_routes_pre_cutover_contract() {
    let root = typokat_core::test_support::repository_root();
    let projects = [
        (
            root.join("tests/cases/b15_default_module_slots/01_named_class"),
            ["source.ts", "consumer.ts"],
        ),
        (
            root.join("tests/cases/b15_acyclic_source_reexports/route_baseline"),
            ["barrel.ts", "source.ts"],
        ),
    ];
    let provider = LibraryBaseProvider::new();

    for (project, forward) in projects {
        for names in [forward, [forward[1], forward[0]]] {
            let inputs = fixture_inputs(&project, &names);

            let standalone_control =
                check_project_once(inputs.clone()).expect("standalone pre-cutover control");
            let standalone_acquisitions = Cell::new(0);
            let standalone = check_project_local_shadow_for_test(
                inputs.clone(),
                ProjectLocalShadowRouteForTest::Standalone,
                || {
                    standalone_acquisitions.set(standalone_acquisitions.get() + 1);
                    provider.declaration_product_for_shadow()
                },
            )
            .expect("standalone project-local shadow");
            assert_eq!(
                report_shape(&standalone.reports),
                report_shape(&standalone_control),
                "standalone {project:?} {names:?}",
            );
            assert_eq!(
                standalone_acquisitions.get(),
                1,
                "explicit notices are reported alongside semantic results",
            );

            let repeated_control =
                check_project(inputs.clone()).expect("repeated pre-cutover control");
            let repeated_acquisitions = Cell::new(0);
            let repeated = check_project_local_shadow_for_test(
                inputs,
                ProjectLocalShadowRouteForTest::RepeatedProject,
                || {
                    repeated_acquisitions.set(repeated_acquisitions.get() + 1);
                    provider.declaration_product_for_shadow()
                },
            )
            .expect("repeated project-local shadow");
            assert_eq!(
                report_shape(&repeated.reports),
                report_shape(&repeated_control),
                "repeated {project:?} {names:?}",
            );
            assert_eq!(repeated_acquisitions.get(), 1);
        }
    }
}

#[test]
fn project_local_shadow_reuses_only_the_product_across_fresh_sessions() {
    let provider = LibraryBaseProvider::new();
    let product = provider
        .declaration_product_for_shadow()
        .expect("packaged declaration product");
    let same_product = provider
        .declaration_product_for_shadow()
        .expect("cached packaged declaration product");
    assert!(Arc::ptr_eq(&product, &same_product));
    let observations = ObservationScopeForTest::start();
    for ordinal in 0..2 {
        let shared = Arc::clone(&product);
        let run = check_project_local_shadow_for_test(
            vec![input(
                &format!("/wu5/session-{ordinal}.ts"),
                "const value: number = 1;\n",
            )],
            ProjectLocalShadowRouteForTest::RepeatedProject,
            || Ok(shared),
        )
        .expect("fresh repeated project-local session");
        assert_eq!(run.reports.len(), 1);
    }
    let receipt = observations.finish();
    let identities = receipt
        .events
        .iter()
        .filter_map(|event| match event.kind {
            ObservationEventKindForTest::SessionCreated { project, publisher } => {
                Some((project, publisher))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(identities.len(), 2);
    assert_ne!(identities[0].0, identities[1].0);
    assert_ne!(identities[0].1, identities[1].1);
    assert_eq!(receipt.session_weak.len(), 2);
    assert_eq!(receipt.cache_weak.len(), 2);
    assert!(receipt
        .session_weak
        .iter()
        .chain(&receipt.cache_weak)
        .all(|weak| !weak.upgraded_at_finish));
}

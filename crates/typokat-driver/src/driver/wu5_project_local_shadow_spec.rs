//! RED acceptance for WU5's project-local driver shadow.
//!
//! Public dispatch remains unchanged until WU7. These tests exercise the one candidate driver
//! continuation that WU6 will compare with the complete-source control.

use super::{
    check_project_local_shadow_for_test, check_project_once, DriverError, FileReport,
    ProjectLocalShadowRouteForTest,
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
}

#[test]
fn project_local_shadow_activates_supported_default_reexports_before_the_notice_gate() {
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
}

#[test]
fn project_local_shadow_reuses_only_the_product_across_fresh_sessions() {
    let provider = LibraryBaseProvider::new();
    let product = provider
        .declaration_product_for_shadow()
        .expect("packaged declaration product");
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

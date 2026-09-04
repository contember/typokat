//! Unit contract for the production declaration-product provider, sessions, and workers.
//!
//! Every assertion below calls the real production entry points. Fault injection perturbs
//! provider or worker results, while trace evidence is recorded only at production hooks.

use super::test_support::{
    classify_driver_failure_for_test, DriverFailureKindForTest, ProductionDriverFaultForTest,
    ProductionDriverFaultTraceScopeForTest, ProviderTraceFaultForTest,
};
use super::DriverError;
use crate::frontend::{FileInput, ProjectRoot};
use crate::library::LibraryInitError;
use std::fs;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::Duration;

fn inputs(count: usize) -> Vec<FileInput> {
    (0..count)
        .map(|index| FileInput {
            name: format!("/wu7/{index:02}.ts"),
            source: format!("export const value{index}: number = {index};\n"),
        })
        .collect()
}

fn malformed_inputs() -> Vec<FileInput> {
    vec![FileInput {
        name: "/wu7/malformed.ts".to_owned(),
        source: "declare namespace Broken { export = Broken; }\n".to_owned(),
    }]
}

fn bundler_inputs() -> (Vec<FileInput>, PathBuf, Vec<ProjectRoot>) {
    let directory = typokat_core::test_support::repository_root()
        .join("tests/cases/b72_bundler_project_tracer/admitted_files_extensionless");
    let inputs = ["value.ts", "main.ts"]
        .into_iter()
        .map(|name| FileInput {
            name: name.to_owned(),
            source: fs::read_to_string(directory.join(name))
                .expect("read authentic Bundler fixture"),
        })
        .collect::<Vec<_>>();
    let roots = inputs
        .iter()
        .map(|input| ProjectRoot {
            identity: input.name.clone(),
            path: directory.join(&input.name),
            exists: true,
        })
        .collect();
    (inputs, directory, roots)
}

fn library_failure<T>(
    result: Result<T, DriverError>,
    message: &'static str,
) -> Arc<LibraryInitError> {
    match result {
        Ok(_) => panic!("{message}"),
        Err(DriverError::LibraryInitialization(error)) => error,
        Err(error) => panic!("{message}: {error}"),
    }
}

fn driver_failure<T>(result: Result<T, DriverError>, message: &'static str) -> DriverError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

#[test]
fn deterministic_initialization_failure_precedes_user_parsing_and_is_one_cached_arc() {
    let scope = ProductionDriverFaultTraceScopeForTest::install(
        ProductionDriverFaultForTest::ProviderInitialization(
            "injected deterministic failure".to_owned(),
        ),
    );

    let first = library_failure(
        super::check_source("declare namespace Broken { export = Broken; }\n"),
        "source mode must expose initialization failure before parse errors",
    );
    let second = library_failure(
        super::check_project(malformed_inputs()),
        "project mode must expose initialization failure before parse errors",
    );
    let third = library_failure(
        super::check_project_once(malformed_inputs()),
        "standalone mode must expose initialization failure before parse errors",
    );
    let fourth = library_failure(
        super::check_files(malformed_inputs()),
        "parallel mode must expose initialization failure before parse errors",
    );
    let (mut bundler_files, bundler_directory, bundler_roots) = bundler_inputs();
    bundler_files[0].source = "declare namespace Broken { export = Broken; }\n".to_owned();
    let fifth = library_failure(
        super::check_bundler_project_once(bundler_files, bundler_directory, bundler_roots),
        "Bundler mode must expose initialization failure before parse errors",
    );
    let route = match super::production_library_route() {
        Ok(_) => panic!("library-info attestation must expose initialization failure"),
        Err(error) => error,
    };
    assert!(Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(&first, &third));
    assert!(Arc::ptr_eq(&first, &fourth));
    assert!(Arc::ptr_eq(&first, &fifth));
    assert!(Arc::ptr_eq(&first, &route));

    let trace_context = scope.context();
    let errors = std::thread::scope(|threads| {
        (0..32)
            .map(|index| {
                let trace_context = trace_context.clone();
                threads.spawn(move || {
                    trace_context.run(|| {
                        if index % 2 == 0 {
                            super::check_source("export const value = 1;\n").map(|_| ())
                        } else {
                            super::check_files(inputs(2)).map(|_| ())
                        }
                    })
                })
            })
            .map(|worker| {
                worker
                    .join()
                    .expect("fault-injection caller does not panic")
                    .unwrap_err()
            })
            .collect::<Vec<_>>()
    });
    assert!(errors.iter().all(|error| {
        matches!(
            error,
            DriverError::LibraryInitialization(library) if Arc::ptr_eq(&first, library)
        )
    }));

    let trace = scope.finish();
    assert_eq!(trace.provider_initialization_attempts, 1);
    assert_eq!(trace.provider_publications, 0);
    assert_eq!(trace.provider_acquisitions, 38);
    assert_eq!(trace.worker_starts, 0);
    assert_eq!(trace.rayon_worker_starts, 0);
    assert_eq!(trace.reports_exposed, 0);
}

#[test]
fn fault_trace_scope_excludes_a_preexisting_foreign_invocation() {
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let (_owned_error, foreign_reports, trace) = std::thread::scope(|threads| {
        let foreign = threads.spawn(move || {
            ready_sender.send(()).expect("foreign thread reports ready");
            release_receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("foreign thread is released while the owned scope is active");
            super::check_project_once(inputs(1))
        });
        ready_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("foreign thread starts before scope installation");

        let scope = ProductionDriverFaultTraceScopeForTest::install(
            ProductionDriverFaultForTest::ProviderInitialization(
                "owned invocation must use the product provider".to_owned(),
            ),
        );
        release_sender
            .send(())
            .expect("foreign invocation is released");
        let owned_error = library_failure(
            super::check_project_once(inputs(1)),
            "owned standalone invocation must expose provider failure",
        );
        let foreign_reports = foreign
            .join()
            .expect("foreign standalone caller does not panic")
            .expect("foreign standalone invocation uses the ordinary provider");
        let trace = scope.finish();
        (owned_error, foreign_reports, trace)
    });

    assert_eq!(foreign_reports.len(), 1);
    assert_eq!(trace.provider_initialization_attempts, 1);
    assert_eq!(trace.provider_publications, 0);
    assert_eq!(trace.provider_acquisitions, 1);
    assert_eq!(trace.worker_starts, 0);
    assert_eq!(trace.reports_exposed, 0);
}

#[test]
fn thirty_two_real_public_callers_share_one_published_product() {
    let bundler_project = bundler_inputs();
    let scope = ProductionDriverFaultTraceScopeForTest::install(ProductionDriverFaultForTest::None);
    let trace_context = scope.context();
    std::thread::scope(|threads| {
        let workers = (0..32)
            .map(|index| {
                let trace_context = trace_context.clone();
                let bundler_call = (index % 5 == 4).then(|| bundler_project.clone());
                threads.spawn(move || {
                    trace_context.run(|| match index % 5 {
                        0 => super::check_source("export const value = 1;\n").map(|_| ()),
                        1 => super::check_files(inputs(1)).map(|_| ()),
                        2 => super::check_project(inputs(1)).map(|_| ()),
                        3 => super::check_project_once(inputs(1)).map(|_| ()),
                        _ => {
                            let (files, directory, roots) =
                                bundler_call.expect("Bundler caller owns prepared fixture");
                            super::check_bundler_project_once(files, directory, roots).map(|_| ())
                        }
                    })
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker
                .join()
                .expect("fault-free public caller does not panic")
                .expect("fault-free public caller checks");
        }
    });

    let route = super::production_library_route().expect("route uses the same provider");
    assert_eq!(route, "production-checker-session");

    let trace = scope.finish();
    assert_eq!(trace.provider_initialization_attempts, 1);
    assert_eq!(trace.provider_publications, 1);
    assert_eq!(trace.provider_acquisitions, 33);
    assert_ne!(trace.provider_instance_identity, 0);
    assert_ne!(trace.published_product_identity, 0);
    assert!(trace
        .worker_product_identities
        .iter()
        .all(|identity| *identity == trace.published_product_identity));
}

#[test]
fn real_parallel_path_acquires_once_before_one_two_and_thirty_two_rayon_workers() {
    for count in [1, 2, 32] {
        let scope =
            ProductionDriverFaultTraceScopeForTest::install(ProductionDriverFaultForTest::None);
        let reports = super::check_files(inputs(count))
            .expect("parallel mode initializes and checks through the real entry point");
        assert_eq!(reports.len(), count);

        let trace = scope.finish();
        assert_eq!(trace.provider_initialization_attempts, 1);
        assert_eq!(trace.provider_publications, 1);
        assert_eq!(trace.provider_acquisitions, 1);
        assert_eq!(trace.rayon_worker_starts, count);
        assert!(trace.provider_acquired_before_rayon);
        assert_eq!(trace.worker_product_identities.len(), count);
        assert!(trace
            .worker_product_identities
            .iter()
            .all(|identity| *identity == trace.published_product_identity));
    }
}

#[test]
fn trace_negative_controls_perturb_evidence_at_real_hooks() {
    let inside_rayon = ProductionDriverFaultTraceScopeForTest::install(
        ProductionDriverFaultForTest::ProviderTrace(ProviderTraceFaultForTest::AcquireInsideRayon),
    );
    super::check_files(inputs(2))
        .expect("the trace-order fault changes evidence, not user semantics");
    let trace = inside_rayon.finish();
    assert!(!trace.provider_acquired_before_rayon);
    assert_eq!(trace.rayon_worker_starts, 2);

    let replaced_identity = ProductionDriverFaultTraceScopeForTest::install(
        ProductionDriverFaultForTest::ProviderTrace(
            ProviderTraceFaultForTest::ReplaceOneWorkerProduct,
        ),
    );
    super::check_files(inputs(2)).expect("the identity fault changes evidence, not user semantics");
    let trace = replaced_identity.finish();
    assert!(trace
        .worker_product_identities
        .iter()
        .any(|identity| *identity != trace.published_product_identity));
}

#[test]
fn real_worker_spawn_and_join_failures_are_typed_and_expose_no_partial_reports() {
    for (fault, expected_kind) in [
        (
            ProductionDriverFaultForTest::WorkerSpawn,
            DriverFailureKindForTest::WorkerSpawn,
        ),
        (
            ProductionDriverFaultForTest::WorkerJoin,
            DriverFailureKindForTest::WorkerJoin,
        ),
    ] {
        for mode in ["source", "project", "parallel", "standalone", "bundler"] {
            let scope = ProductionDriverFaultTraceScopeForTest::install(fault.clone());
            let error = match mode {
                "source" => driver_failure(
                    super::check_source("export const value = 1;\n"),
                    "source worker fault must be returned",
                ),
                "project" => driver_failure(
                    super::check_project(inputs(3)),
                    "project worker fault must be returned",
                ),
                "parallel" => driver_failure(
                    super::check_files(inputs(3)),
                    "parallel worker fault must discard every report",
                ),
                "standalone" => driver_failure(
                    super::check_project_once(inputs(3)),
                    "standalone worker fault must discard every report",
                ),
                "bundler" => {
                    let (files, directory, roots) = bundler_inputs();
                    driver_failure(
                        super::check_bundler_project_once(files, directory, roots),
                        "Bundler worker fault must discard every report",
                    )
                }
                _ => unreachable!("closed mode matrix"),
            };
            assert_eq!(classify_driver_failure_for_test(&error), expected_kind);
            let trace = scope.finish();
            assert_eq!(trace.reports_exposed, 0);
        }
    }
}

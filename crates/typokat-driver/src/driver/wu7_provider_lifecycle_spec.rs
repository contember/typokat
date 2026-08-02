//! RED unit contract for WU7's private provider/worker fault-and-trace scope.
//!
//! Every assertion below calls the real production entry points. The private scope may substitute
//! a provider result or perturb trace evidence, but it must record only hooks reached by the real
//! provider, worker, rayon, and report-publication path.

use super::test_support::{
    classify_driver_failure_for_test, DriverFailureKindForTest, ProductionDriverFaultForTest,
    ProductionDriverFaultTraceScopeForTest, ProviderTraceFaultForTest,
};
use super::DriverError;
use crate::frontend::FileInput;
use crate::library::LibraryInitError;
use std::sync::Arc;

fn inputs(count: usize) -> Vec<FileInput> {
    (0..count)
        .map(|index| FileInput {
            name: format!("/wu7/{index:02}.ts"),
            source: format!("export const value{index}: number = {index};\n"),
        })
        .collect()
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
fn deterministic_initialization_failure_is_one_cached_arc_through_real_public_functions() {
    let scope = ProductionDriverFaultTraceScopeForTest::install(
        ProductionDriverFaultForTest::ProviderInitialization(
            "injected deterministic failure".to_owned(),
        ),
    );

    let first = library_failure(
        super::check_source("export const first = 1;\n"),
        "single mode must expose initialization failure",
    );
    let second = library_failure(
        super::check_project(inputs(2)),
        "project mode must expose initialization failure",
    );
    let route = match super::production_library_route() {
        Ok(_) => panic!("library-info attestation must expose initialization failure"),
        Err(error) => error,
    };
    assert!(Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(&first, &route));

    let errors = std::thread::scope(|threads| {
        (0..32)
            .map(|index| {
                threads.spawn(move || {
                    if index % 2 == 0 {
                        super::check_source("export const value = 1;\n").map(|_| ())
                    } else {
                        super::check_files(inputs(2)).map(|_| ())
                    }
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
    assert_eq!(trace.provider_acquisitions, 35);
    assert_eq!(trace.worker_starts, 0);
    assert_eq!(trace.rayon_worker_starts, 0);
    assert_eq!(trace.reports_exposed, 0);
}

#[test]
fn thirty_two_real_public_callers_share_one_published_provider_and_base() {
    let scope = ProductionDriverFaultTraceScopeForTest::install(ProductionDriverFaultForTest::None);
    std::thread::scope(|threads| {
        let workers = (0..32)
            .map(|index| {
                threads.spawn(move || {
                    if index % 3 == 0 {
                        super::check_source("export const value = 1;\n").map(|_| ())
                    } else if index % 3 == 1 {
                        super::check_files(inputs(1)).map(|_| ())
                    } else {
                        super::check_project(inputs(1)).map(|_| ())
                    }
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
    assert_eq!(route, "production-default-library");

    let trace = scope.finish();
    assert_eq!(trace.provider_initialization_attempts, 1);
    assert_eq!(trace.provider_publications, 1);
    assert_eq!(trace.provider_acquisitions, 33);
    assert_ne!(trace.provider_instance_identity, 0);
    assert_ne!(trace.published_base_identity, 0);
    assert!(trace
        .worker_base_identities
        .iter()
        .all(|identity| *identity == trace.published_base_identity));
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
        assert_eq!(trace.worker_base_identities.len(), count);
        assert!(trace
            .worker_base_identities
            .iter()
            .all(|identity| *identity == trace.published_base_identity));
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
            ProviderTraceFaultForTest::ReplaceOneWorkerBase,
        ),
    );
    super::check_files(inputs(2)).expect("the identity fault changes evidence, not user semantics");
    let trace = replaced_identity.finish();
    assert!(trace
        .worker_base_identities
        .iter()
        .any(|identity| *identity != trace.published_base_identity));
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
        for mode in ["single", "project", "parallel"] {
            let scope = ProductionDriverFaultTraceScopeForTest::install(fault.clone());
            let error = match mode {
                "single" => driver_failure(
                    super::check_source("export const value = 1;\n"),
                    "single worker fault must be returned",
                ),
                "project" => driver_failure(
                    super::check_project(inputs(3)),
                    "project worker fault must be returned",
                ),
                "parallel" => driver_failure(
                    super::check_files(inputs(3)),
                    "parallel worker fault must discard every report",
                ),
                _ => unreachable!("closed mode matrix"),
            };
            assert_eq!(classify_driver_failure_for_test(&error), expected_kind);
            let trace = scope.finish();
            assert_eq!(trace.reports_exposed, 0);
        }
    }
}

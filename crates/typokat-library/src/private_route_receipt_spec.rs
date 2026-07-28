//! RED contract for preserving authenticated private-route inputs.
//!
//! The scheduler cannot reconstruct preflight seeds after binding. The production route therefore
//! carries the exact classifier receipt and the admitted process-local replay plan forward.

use super::base::{CollisionRouteForTest, RoutedLibraryProject, UserDeltaProjectInputForTest};
use super::compiler::LibraryCompilerWorkScopeForTest;
use super::provider::shared_library_base_provider_for_test;
use super::FrozenLibraryBase;
use crate::check::checker::events::UserEventReservationScopeForTest;
use crate::check::checker::library_compiler::UserDeltaForkScopeForTest;
use crate::check::query::QueryCacheWriteScopeForTest;
use crate::frontend::FileInput;
use crate::relate::cache::RelationCacheWriteScopeForTest;
use crate::types::layered::LocalRowAllocationScopeForTest;
use std::collections::BTreeSet;
use std::sync::Arc;

fn acquire() -> Arc<FrozenLibraryBase> {
    shared_library_base_provider_for_test()
        .get()
        .expect("source-compiled default library base")
}

fn test_input<'source>(
    path: &'source str,
    source: &'source str,
) -> UserDeltaProjectInputForTest<'source> {
    UserDeltaProjectInputForTest { path, source }
}

fn route_input(path: &str, source: &str) -> FileInput {
    FileInput {
        name: path.to_owned(),
        source: source.to_owned(),
    }
}

fn route_without_semantic_work(
    base: &FrozenLibraryBase,
    inputs: &[FileInput],
) -> RoutedLibraryProject {
    let delta_forks = UserDeltaForkScopeForTest::start();
    let local_rows = LocalRowAllocationScopeForTest::start();
    let events = UserEventReservationScopeForTest::start();
    let queries = QueryCacheWriteScopeForTest::start();
    let relations = RelationCacheWriteScopeForTest::start();
    let compiler = LibraryCompilerWorkScopeForTest::start();
    let routed = base.route_user_project(inputs).expect("production route");
    let compiler = compiler.finish();
    let queries = queries.finish();

    assert_eq!(delta_forks.finish(), 0);
    assert_eq!(local_rows.finish(), 0);
    assert_eq!(events.finish(), 0);
    assert_eq!(queries.evaluator, 0);
    assert_eq!(queries.projection, 0);
    assert_eq!(relations.finish(), 0);
    assert_eq!(compiler.compiles, 0);
    assert_eq!(compiler.parses, 0);
    assert_eq!(compiler.binds, 0);
    assert_eq!(compiler.checks, 0);
    routed
}

fn frozen_prefixes(base: &FrozenLibraryBase) -> [usize; 9] {
    let prefixes = base.prefixes_for_test();
    [
        prefixes.types,
        prefixes.type_params,
        prefixes.classes,
        prefixes.scopes,
        prefixes.symbols,
        prefixes.declarations,
        prefixes.type_groups,
        prefixes.namespaces,
        prefixes.value_storages,
    ]
}

#[test]
fn private_route_preserves_the_exact_preflight_receipt_and_admitted_plan() {
    let base = acquire();
    let path = "/project/collisions.ts";
    let source = r#"
interface Array<T> { b103RouteReceipt(): T }
declare namespace Intl { interface B103RouteReceipt {} }
declare var document: Document;
"#;
    let expected = base
        .preflight_user_project_for_test(&[test_input(path, source)])
        .expect("independent preflight receipt");
    assert_eq!(expected.route, CollisionRouteForTest::PrivateCombined);
    assert_eq!(
        expected.reasons,
        BTreeSet::from([
            "frozen-root-name-collision".to_owned(),
            "global-object-contributor".to_owned(),
        ])
    );

    let routed = route_without_semantic_work(&base, &[route_input(path, source)]);

    let private = match routed {
        RoutedLibraryProject::Private(private) => private,
        RoutedLibraryProject::Shared(_) => panic!("colliding input escaped to the shared route"),
    };
    let receipt = private.route_receipt_for_test();
    assert_eq!(
        receipt.module_classifications,
        expected.module_classifications
    );
    assert_eq!(receipt.candidates, expected.candidates);
    assert_eq!(receipt.reasons, expected.reasons);
    assert!(
        Arc::ptr_eq(
            private.collision_plan_for_test(),
            base.collision_plan_for_test()
        ),
        "the route must clone the base Arc, not the plan value"
    );
    assert!(private
        .collision_plan_for_test()
        .admit_for_frozen_base(frozen_prefixes(&base), base.identity().profile_sha256())
        .is_ok());
    assert!(private
        .collision_plan_for_test()
        .admit_for_frozen_base(frozen_prefixes(&base), "wrong-profile-identity")
        .is_err());
}

#[test]
fn shared_route_does_not_manufacture_a_private_receipt_or_repeat_library_work() {
    let base = acquire();
    let path = "/project/fresh.ts";
    let source = "export {};\ninterface Array<T> { b103ModuleLocalRouteReceipt(): T }\n";
    let expected = base
        .preflight_user_project_for_test(&[test_input(path, source)])
        .expect("independent shared preflight receipt");
    assert_eq!(expected.route, CollisionRouteForTest::SharedDelta);
    assert!(expected.reasons.is_empty());

    let routed = route_without_semantic_work(&base, &[route_input(path, source)]);

    match routed {
        RoutedLibraryProject::Shared(_) => {}
        RoutedLibraryProject::Private(_) => panic!("fresh input manufactured a private receipt"),
    }
}

//! Disabled RED contract for WU5 collision preflight.
//!
//! Preflight is the only issuer of the collision-free capability. It must finish before any
//! user/library semantic mutation and must share the binder's module classifier and binding walker.

use super::base::{
    CollisionPreflightReceiptForTest, CollisionRouteForTest, ModuleClassificationForTest,
    PreflightSlotForTest, UserDeltaProjectInputForTest,
};
use super::{FrozenLibraryBase, LibraryBaseProvider};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn acquire() -> Arc<FrozenLibraryBase> {
    LibraryBaseProvider::new()
        .get()
        .expect("canonical frozen library base")
}

fn input<'source>(
    path: &'source str,
    source: &'source str,
) -> UserDeltaProjectInputForTest<'source> {
    UserDeltaProjectInputForTest { path, source }
}

fn preflight(inputs: &[UserDeltaProjectInputForTest<'_>]) -> CollisionPreflightReceiptForTest {
    acquire()
        .preflight_user_project_for_test(inputs)
        .expect("preflight returns a deterministic route receipt")
}

fn names(receipt: &CollisionPreflightReceiptForTest) -> BTreeSet<String> {
    receipt
        .candidates
        .iter()
        .map(|candidate| candidate.name.clone())
        .collect()
}

fn assert_no_semantic_mutation(receipt: &CollisionPreflightReceiptForTest) {
    assert_eq!(receipt.work.delta_forks, 0);
    assert_eq!(receipt.work.delta_local_rows, 0);
    assert_eq!(receipt.work.user_event_reservations, 0);
    assert_eq!(receipt.work.durable_evaluator_cache_writes, 0);
    assert_eq!(receipt.work.durable_projection_cache_writes, 0);
    assert_eq!(receipt.work.relation_cache_writes, 0);
    assert_eq!(receipt.work.private_library_compiles, 0);
}

fn assert_shared(receipt: &CollisionPreflightReceiptForTest) {
    assert_eq!(receipt.route, CollisionRouteForTest::SharedDelta);
    assert!(receipt.capability_issued);
    assert_no_semantic_mutation(receipt);
}

fn assert_private(receipt: &CollisionPreflightReceiptForTest) {
    assert_eq!(receipt.route, CollisionRouteForTest::PrivateCombined);
    assert!(!receipt.capability_issued);
    assert_no_semantic_mutation(receipt);
}

fn assert_rejected(receipt: &CollisionPreflightReceiptForTest) {
    assert_eq!(
        receipt.route,
        CollisionRouteForTest::RejectedBeforeSemantics
    );
    assert!(!receipt.capability_issued);
    assert_no_semantic_mutation(receipt);
}

#[test]
fn module_classification_is_exactly_the_binder_classification() {
    let receipt = preflight(&[
        input("/project/script.ts", "const value = 1;\n"),
        input("/project/script.d.ts", "declare const value: number;\n"),
        input(
            "/project/import.ts",
            "import { value } from './dep'; void value;\n",
        ),
        input(
            "/project/export-named.ts",
            "const value = 1; export { value };\n",
        ),
        input("/project/export-default.ts", "export default 1;\n"),
        input("/project/export-all.ts", "export * from './dep';\n"),
        input(
            "/project/export-equals.ts",
            "declare const value: number; export = value;\n",
        ),
        input(
            "/project/import-require.ts",
            "import value = require('./dep');\n",
        ),
        input("/project/import-internal.ts", "import value = WU5.Space;\n"),
        input(
            "/project/import-meta.ts",
            "const value = import.meta.url;\n",
        ),
        input(
            "/project/namespace-export.d.ts",
            "export as namespace WU5Umd; declare namespace WU5Umd {}\n",
        ),
        input("/project/plain.mts", "const value = 1;\n"),
        input("/project/plain.cts", "const value = 1;\n"),
        input("/project/plain.d.mts", "declare const value: number;\n"),
        input("/project/plain.d.cts", "declare const value: number;\n"),
    ]);

    assert_private(&receipt);
    assert_eq!(
        receipt.module_classifications,
        [
            (
                "/project/export-all.ts",
                ModuleClassificationForTest::External
            ),
            (
                "/project/export-default.ts",
                ModuleClassificationForTest::External
            ),
            (
                "/project/export-equals.ts",
                ModuleClassificationForTest::External
            ),
            (
                "/project/export-named.ts",
                ModuleClassificationForTest::External
            ),
            (
                "/project/import-internal.ts",
                ModuleClassificationForTest::Script
            ),
            (
                "/project/import-meta.ts",
                ModuleClassificationForTest::External
            ),
            (
                "/project/import-require.ts",
                ModuleClassificationForTest::External
            ),
            ("/project/import.ts", ModuleClassificationForTest::External),
            (
                "/project/namespace-export.d.ts",
                ModuleClassificationForTest::Script
            ),
            ("/project/plain.cts", ModuleClassificationForTest::External),
            ("/project/plain.d.cts", ModuleClassificationForTest::Script),
            ("/project/plain.d.mts", ModuleClassificationForTest::Script),
            ("/project/plain.mts", ModuleClassificationForTest::External),
            ("/project/script.d.ts", ModuleClassificationForTest::Script),
            ("/project/script.ts", ModuleClassificationForTest::Script),
        ]
    );
}

#[test]
fn unique_script_lexical_and_type_roots_stay_shared() {
    let receipt = preflight(&[input(
        "/project/unique.ts",
        r#"interface WU5Interface { value: number }
type WU5Alias = WU5Interface;
let WU5Let = 1;
const WU5Const = 1;
class WU5Class { value = 1; }
enum WU5Enum { Value }
declare const acquire: () => Disposable;
using WU5Using = acquire();
import WU5Internal = WU5Namespace.Member;
"#,
    )]);

    assert_shared(&receipt);
    assert_eq!(
        names(&receipt),
        BTreeSet::from([
            "WU5Alias".to_owned(),
            "WU5Class".to_owned(),
            "WU5Const".to_owned(),
            "WU5Enum".to_owned(),
            "WU5Interface".to_owned(),
            "WU5Internal".to_owned(),
            "WU5Let".to_owned(),
            "WU5Using".to_owned(),
            "acquire".to_owned(),
        ])
    );
    assert!(receipt
        .candidates
        .iter()
        .all(|candidate| !candidate.global_object_contributor));
}

#[test]
fn script_global_object_contributors_route_private_without_collision() {
    let receipt = preflight(&[input(
        "/project/globals.ts",
        r#"declare var WU5UniqueVar: number;
function WU5UniqueFunction(): number { return 1; }
namespace WU5UniqueNamespace { export const value = 1; }
"#,
    )]);

    assert_private(&receipt);
    assert_eq!(
        names(&receipt),
        BTreeSet::from([
            "WU5UniqueFunction".to_owned(),
            "WU5UniqueNamespace".to_owned(),
            "WU5UniqueVar".to_owned(),
        ])
    );
    assert!(receipt
        .candidates
        .iter()
        .all(|candidate| candidate.global_object_contributor));
    assert!(receipt.reasons.contains("global-object-contributor"));
}

#[test]
fn frozen_name_collision_routes_every_value_type_namespace_and_cross_slot_case_private() {
    let sources = [
        (
            "/project/type.ts",
            "interface Array<T> { added: T }\n",
            "Array",
        ),
        (
            "/project/alias.ts",
            "type Promise<T> = { value: T };\n",
            "Promise",
        ),
        ("/project/value.ts", "const document = 1;\n", "document"),
        ("/project/class.ts", "class RegExp {}\n", "RegExp"),
        ("/project/namespace.ts", "namespace Intl {}\n", "Intl"),
        (
            "/project/cross-type.ts",
            "interface document { marker: number }\n",
            "document",
        ),
        (
            "/project/cross-value.ts",
            "const AddEventListenerOptions = 1;\n",
            "AddEventListenerOptions",
        ),
        (
            "/project/cross-namespace.ts",
            "type Intl = number;\n",
            "Intl",
        ),
    ];

    for (path, source, expected_name) in sources {
        let receipt = preflight(&[input(path, source)]);
        assert_private(&receipt);
        assert_eq!(names(&receipt), BTreeSet::from([expected_name.to_owned()]));
        assert!(receipt.reasons.contains("frozen-root-name-collision"));
    }
}

fn slots(receipt: &CollisionPreflightReceiptForTest, name: &str) -> BTreeSet<PreflightSlotForTest> {
    receipt
        .candidates
        .iter()
        .find(|candidate| candidate.name == name)
        .unwrap_or_else(|| panic!("missing candidate {name}"))
        .slots
        .clone()
}

#[test]
fn candidate_slot_masks_are_exact_and_conservative() {
    let receipt = preflight(&[input(
        "/project/slots.ts",
        r#"var WU5Var = 1;
function WU5Function(): void {}
class WU5Class {}
interface WU5Interface {}
type WU5Alias = number;
enum WU5Enum { Value }
namespace WU5Namespace {}
import WU5Import = WU5Namespace;
"#,
    )]);

    assert_private(&receipt);
    let value = BTreeSet::from([PreflightSlotForTest::Value]);
    let ty = BTreeSet::from([PreflightSlotForTest::Type]);
    let value_type = BTreeSet::from([PreflightSlotForTest::Value, PreflightSlotForTest::Type]);
    let value_namespace =
        BTreeSet::from([PreflightSlotForTest::Value, PreflightSlotForTest::Namespace]);
    let all = BTreeSet::from([
        PreflightSlotForTest::Value,
        PreflightSlotForTest::Type,
        PreflightSlotForTest::Namespace,
    ]);
    assert_eq!(slots(&receipt, "WU5Var"), value);
    assert_eq!(slots(&receipt, "WU5Function"), value);
    assert_eq!(slots(&receipt, "WU5Class"), value_type);
    assert_eq!(slots(&receipt, "WU5Interface"), ty);
    assert_eq!(slots(&receipt, "WU5Alias"), ty);
    assert_eq!(slots(&receipt, "WU5Enum"), value_type);
    assert_eq!(slots(&receipt, "WU5Namespace"), value_namespace);
    assert_eq!(slots(&receipt, "WU5Import"), all);
}

#[test]
fn import_equals_masks_match_value_and_type_only_binder_slots() {
    let value = preflight(&[input(
        "/project/import-equals.ts",
        "import WU5Value = WU5.Space;\n",
    )]);
    assert_shared(&value);
    assert_eq!(
        slots(&value, "WU5Value"),
        BTreeSet::from([
            PreflightSlotForTest::Value,
            PreflightSlotForTest::Type,
            PreflightSlotForTest::Namespace,
        ])
    );
    let typed = preflight(&[input(
        "/project/import-type-equals.ts",
        "import type WU5Type = WU5.Space;\n",
    )]);
    assert_private(&typed);
    assert_eq!(
        slots(&typed, "WU5Type"),
        BTreeSet::from([PreflightSlotForTest::Type])
    );
}

#[test]
fn binding_pattern_census_collects_every_nested_leaf() {
    let receipt = preflight(&[input(
        "/project/destructuring.ts",
        r#"declare const source: {
  a: [number];
  b: { c: number };
  tail: { value: number };
};
if (source) {
  var { a: [Array = 1], b: { c: document }, ...Intl } = source;
}
"#,
    )]);

    assert_private(&receipt);
    assert_eq!(
        names(&receipt),
        BTreeSet::from([
            "Array".to_owned(),
            "Intl".to_owned(),
            "document".to_owned(),
            "source".to_owned(),
        ])
    );
    let leaf_names = receipt
        .candidates
        .iter()
        .filter(|candidate| candidate.global_object_contributor)
        .map(|candidate| candidate.name.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(leaf_names, BTreeSet::from(["Array", "Intl", "document"]));
}

#[test]
fn nested_hoisted_var_leaves_route_private_across_every_statement_container() {
    let receipt = preflight(&[input(
        "/project/hoists.ts",
        r#"declare const condition: boolean;
declare const values: number[];
{ var WU5Block = 1; }
if (condition) { var WU5If = 1; } else { var WU5Else = 1; }
switch (values.length) { case 0: var WU5Switch = 1; }
while (condition) { var WU5While = 1; break; }
do { var WU5Do = 1; } while (false);
for (var WU5For = 0; WU5For < 1; WU5For++) {}
for (var WU5ForIn in { value: 1 }) {}
for (var WU5ForOf of values) {}
WU5Label: { var WU5LabelVar = 1; break WU5Label; }
try { var WU5Try = 1; } catch { var WU5Catch = 1; } finally { var WU5Finally = 1; }
"#,
    )]);

    assert_private(&receipt);
    for expected in [
        "WU5Block",
        "WU5Catch",
        "WU5Do",
        "WU5Else",
        "WU5Finally",
        "WU5For",
        "WU5ForIn",
        "WU5ForOf",
        "WU5If",
        "WU5LabelVar",
        "WU5Switch",
        "WU5Try",
        "WU5While",
    ] {
        assert!(
            names(&receipt).contains(expected),
            "missing hoisted leaf {expected}"
        );
    }
}

#[test]
fn loop_lexicals_and_nested_lexical_declarations_do_not_become_global_roots() {
    let receipt = preflight(&[input(
        "/project/nested-lexical.ts",
        r#"for (let Array = 0; Array < 1; Array++) {}
{ function RegExp() {} class Promise {} interface Intl {} type Document = number; }
"#,
    )]);
    assert_shared(&receipt);
    assert!(receipt.candidates.is_empty(), "{:#?}", receipt.candidates);
}

#[test]
fn named_function_expressions_do_not_leak_but_declarations_do() {
    let receipt = preflight(&[input(
        "/project/function-forms.ts",
        "const local = function Array() {}; function WU5Declared() {}\n",
    )]);
    assert_private(&receipt);
    assert!(!names(&receipt).contains("Array"));
    assert!(names(&receipt).contains("local"));
    assert!(names(&receipt).contains("WU5Declared"));
}

#[test]
fn function_class_and_namespace_boundaries_do_not_leak_nested_var_candidates() {
    let receipt = preflight(&[input(
        "/project/boundaries.ts",
        r#"function localFunction() { var Array = 1; return Array; }
class WU5BoundaryClass { method() { var document = 1; return document; } }
namespace WU5BoundaryNamespace { var RegExp = 1; }
"#,
    )]);

    assert_private(&receipt);
    let observed = names(&receipt);
    assert!(observed.contains("localFunction"));
    assert!(observed.contains("WU5BoundaryClass"));
    assert!(observed.contains("WU5BoundaryNamespace"));
    assert!(!observed.contains("Array"));
    assert!(!observed.contains("document"));
    assert!(!observed.contains("RegExp"));
}

#[test]
fn external_module_locals_are_ignored_but_declare_global_is_still_censused() {
    let local = preflight(&[input(
        "/project/local.ts",
        "interface Array<T> { local: T }\nconst document = 1;\nexport {};\n",
    )]);
    assert_shared(&local);
    assert!(local.candidates.is_empty());

    let unique_type = preflight(&[input(
        "/project/unique.ts",
        "export {}; declare global { interface WU5UniqueGlobalType { value: number } }\n",
    )]);
    assert_shared(&unique_type);
    assert_eq!(
        names(&unique_type),
        BTreeSet::from(["WU5UniqueGlobalType".to_owned()])
    );

    let collision = preflight(&[input(
        "/project/collision.ts",
        "export {}; declare global { interface Array<T> { added: T } }\n",
    )]);
    assert_private(&collision);

    let value = preflight(&[input(
        "/project/value.ts",
        "export {}; declare global { var WU5UniqueGlobalValue: number }\n",
    )]);
    assert_private(&value);
    assert!(value.reasons.contains("global-object-contributor"));
}

#[test]
fn external_module_inventory_finds_nested_global_and_umd_without_reclassifying_local_namespace() {
    let nested_global = preflight(&[input(
        "/project/nested-global.ts",
        "export {}; { global { interface Array<T> { nested: T } } }\n",
    )]);
    assert_private(&nested_global);
    assert_eq!(names(&nested_global), BTreeSet::from(["Array".to_owned()]));

    let nested_umd = preflight(&[input(
        "/project/nested-umd.d.ts",
        "declare module 'pkg' { export as namespace NestedUmd; }\n",
    )]);
    assert_private(&nested_umd);
    assert!(nested_umd.reasons.contains("umd-global"));

    let local_namespace = preflight(&[input(
        "/project/local-namespace.ts",
        "export {}; namespace Local { export const value = 1; }\n",
    )]);
    assert_shared(&local_namespace);
    assert!(local_namespace.candidates.is_empty());
}

#[test]
fn declare_global_value_contributor_mask_matches_global_object_semantics() {
    let receipt = preflight(&[input(
        "/project/global-mask.ts",
        r#"export {};
declare global {
  var WU5Var: number;
  let WU5Let: number;
  const WU5Const: number;
  class WU5Class {}
  enum WU5Enum { Value }
}
"#,
    )]);
    assert_private(&receipt);
    for name in ["WU5Let", "WU5Const", "WU5Class", "WU5Enum"] {
        assert!(
            !receipt
                .candidates
                .iter()
                .find(|candidate| candidate.name == name)
                .expect("declare-global candidate")
                .global_object_contributor
        );
    }
    assert!(
        receipt
            .candidates
            .iter()
            .find(|candidate| candidate.name == "WU5Var")
            .expect("declare-global var")
            .global_object_contributor
    );
}

#[test]
fn global_this_umd_and_classifier_uncertainty_fail_closed() {
    let global_this = preflight(&[input(
        "/project/global-this.d.ts",
        "declare namespace globalThis { let WU5Explicit: number; }\n",
    )]);
    assert_private(&global_this);
    assert!(global_this.reasons.contains("explicit-global-this"));

    let umd = preflight(&[input(
        "/project/umd.d.ts",
        "export as namespace WU5Umd; export = WU5Umd; declare function WU5Umd(): void;\n",
    )]);
    assert_private(&umd);
    assert!(umd.reasons.contains("umd-global"));

    let uncertain = acquire()
        .preflight_user_project_with_uncertainty_for_test(&[input(
            "/project/uncertain.ts",
            "const value = 1;\n",
        )])
        .expect("injected uncertainty returns a receipt");
    assert_private(&uncertain);
    assert!(uncertain.reasons.contains("classifier-uncertainty"));
}

#[test]
fn hard_parse_failure_is_rejected_before_semantics() {
    let receipt = preflight(&[input("/project/broken.ts", "const broken = {")]);
    assert_rejected(&receipt);
    assert!(receipt.reasons.contains("parse-rejected"));
}

#[test]
fn benign_with_statement_without_global_bindings_stays_shared() {
    let receipt = preflight(&[input(
        "/project/with.ts",
        "declare const object: object; with (object) { object; }\n",
    )]);
    assert_shared(&receipt);
}

#[test]
fn real_uncertain_and_invalid_placement_forms_still_route_private() {
    let with_statement = preflight(&[input(
        "/project/with.ts",
        "declare const object: object; with (object) { var WU5With = 1; }\n",
    )]);
    assert_private(&with_statement);
    assert!(names(&with_statement).contains("WU5With"));

    let nested_global = preflight(&[input(
        "/project/nested-global.ts",
        "namespace WU5Outer { declare global { interface Array<T> { nested: T } } }\n",
    )]);
    assert_private(&nested_global);
    assert!(names(&nested_global).contains("Array"));

    let ambient_module = preflight(&[input(
        "/project/ambient.d.ts",
        "declare module 'pkg' { global { interface Array<T> { ambient: T } } }\n",
    )]);
    assert_private(&ambient_module);
    assert!(names(&ambient_module).contains("Array"));
}

#[test]
fn project_route_and_candidate_names_are_order_path_comment_and_rename_invariant() {
    let first = [
        input("/a/one.ts", "// first\ninterface Array<T> { added: T }\n"),
        input("/a/two.ts", "declare var WU5Global: number;\n"),
    ];
    let second = [
        input(
            "/renamed/two.ts",
            "// renamed\ndeclare var WU5Global: number;\n",
        ),
        input("/renamed/one.ts", "interface Array<T> { added: T }\n"),
    ];
    let first = preflight(&first);
    let second = preflight(&second);

    assert_private(&first);
    assert_private(&second);
    assert_eq!(names(&first), names(&second));
    assert_eq!(first.reasons, second.reasons);
}

fn project_inputs(directory: &Path) -> Vec<(String, String)> {
    let mut paths = fs::read_dir(directory)
        .expect("B14 project directory")
        .map(|entry| entry.expect("B14 directory entry").path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension == "ts")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path).expect("B14 source");
            (path.to_string_lossy().into_owned(), source)
        })
        .collect()
}

#[test]
fn b14_routing_matrix_is_exactly_two_shared_and_ten_private() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases/b14_full_lib_loading_project");
    let mut observed = Vec::new();
    for entry in fs::read_dir(root).expect("B14 routing corpus") {
        let path = entry.expect("B14 project entry").path();
        if !path.is_dir() {
            continue;
        }
        let owned = project_inputs(&path);
        let inputs = owned
            .iter()
            .map(|(path, source)| input(path, source))
            .collect::<Vec<_>>();
        let receipt = preflight(&inputs);
        assert_no_semantic_mutation(&receipt);
        observed.push((
            path.file_name()
                .and_then(|name| name.to_str())
                .expect("UTF-8 B14 project name")
                .to_owned(),
            receipt.route,
        ));
    }
    observed.sort_by(|left, right| left.0.cmp(&right.0));

    let shared = observed
        .iter()
        .filter(|(_, route)| *route == CollisionRouteForTest::SharedDelta)
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    let private = observed
        .iter()
        .filter(|(_, route)| *route == CollisionRouteForTest::PrivateCombined)
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(shared, ["fast_external_module", "zz_shared_base_isolation"]);
    assert_eq!(private.len(), 10, "{private:#?}");
}

#[test]
fn preflight_finishes_before_any_semantic_mutation() {
    let shared = preflight(&[input(
        "/project/shared.ts",
        "export const value: number = 1;\n",
    )]);
    let private = preflight(&[input(
        "/project/private.ts",
        "interface Array<T> { added: T }\n",
    )]);
    assert_shared(&shared);
    assert_private(&private);
}

#[test]
fn preflight_work_receipt_detects_every_forbidden_mutation_family() {
    let calibrated = acquire()
        .calibrate_preflight_work_receipt_for_test()
        .expect("test-only calibration returns one write per family");
    assert_eq!(calibrated.delta_forks, 1);
    assert_eq!(calibrated.delta_local_rows, 4);
    assert_eq!(calibrated.user_event_reservations, 2);
    assert_eq!(calibrated.durable_evaluator_cache_writes, 1);
    assert_eq!(calibrated.durable_projection_cache_writes, 1);
    assert_eq!(calibrated.relation_cache_writes, 2);
    assert_eq!(calibrated.private_library_compiles, 1);
}

#[test]
fn collision_free_capability_is_issued_only_for_a_shared_receipt() {
    let shared = preflight(&[input(
        "/project/shared.ts",
        "export interface Array<T> { local: T }\n",
    )]);
    let private = preflight(&[input(
        "/project/private.ts",
        "interface Array<T> { merged: T }\n",
    )]);
    assert!(shared.capability_issued);
    assert!(!private.capability_issued);

    let library_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/library");
    let constructor = ["CollisionFreeUserDelta", "Capability(())"].concat();
    let mut owners = Vec::new();
    for entry in fs::read_dir(library_root).expect("library source directory") {
        let path = entry.expect("library source entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("library source text");
        if source.contains(&constructor) {
            owners.push(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("UTF-8 source name")
                    .to_owned(),
            );
        }
    }
    owners.sort();
    assert_eq!(owners, ["collision_preflight.rs"]);
}

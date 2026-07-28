//! Type assertions retain their asserted value type while unimplemented overlap
//! validation remains explicit and lexically ordered.

use crate::check::test_support::{check_project, check_source};
use crate::frontend::FileInput;

fn incomplete_ids(source: &str) -> Vec<String> {
    let output = check_source(source);
    assert!(
        output.parse_errors.is_empty(),
        "unexpected parse errors: {:?}",
        output.parse_errors
    );
    assert!(
        output.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        output.diagnostics
    );
    output
        .incomplete
        .into_iter()
        .map(|incomplete| incomplete.id)
        .collect()
}

#[test]
fn assignable_up_and_down_assertions_are_proven_without_incomplete() {
    let source = "\
interface Base { value: number; }
interface Extended { value: number; extra: string; }
declare const base: Base;
declare const extended: Extended;
const upAs: Base = extended as Base;
const downAs: Extended = base as Extended;
const upAngle: Base = <Base>extended;
const downAngle: Extended = <Extended>base;
";

    assert!(incomplete_ids(source).is_empty());
}

#[test]
fn invalid_primitive_object_and_nominal_assertions_are_accounted() {
    let source = "\
interface LeftShape { left: string; }
interface RightShape { right: number; }
declare const leftShape: LeftShape;
class LeftNominal { private identity: number = 1; }
class RightNominal { private identity: number = 1; }
declare const leftNominal: LeftNominal;
const primitive = \"value\" as number;
const object = <RightShape>leftShape;
const nominal = leftNominal as RightNominal;
";

    assert_eq!(
        incomplete_ids(source),
        [
            "expr-infer/as-assertion/compatibility",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
        ]
    );
}

#[test]
fn nested_assertions_replay_once_each_in_source_evaluation_order() {
    let source = "\
interface A { a: string; }
interface B { b: number; }
interface C { c: boolean; }
declare const source: A;
const nested = (<B>source) as C;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.incomplete.len(), 2, "{:?}", output.incomplete);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/type-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
        ]
    );
    assert!(
        output.incomplete[0].span.start > output.incomplete[1].span.start,
        "the inner angle assertion starts after the outer parenthesized assertion"
    );
}

#[test]
fn const_error_and_poisoned_assertions_do_not_add_compatibility_records() {
    let source = "\
const literal = 1 as const;
const unresolvedSource = Missing as number;
const unresolvedTarget = 1 as typeof Missing;
const seed = 1;
class Poisoned { value = seed; }
declare const poisoned: Poisoned;
const unavailable = poisoned as { other: string };
class ImplicitAnyPoisoned { missing; }
declare const implicitAnyPoisoned: ImplicitAnyPoisoned;
const surfaceUnavailable = implicitAnyPoisoned as { other: string };
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2304"]
    );
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "annotation-lower/type-query/typeof",
            "class/property-definition/initializer-inference",
            "class/property-definition/implicit-any",
        ]
    );
}

#[test]
fn assertion_projection_exhaustion_keeps_one_existing_budget_record() {
    let source = "\
class RecursiveSource<T> { next!: RecursiveSource<T[]>; }
class RecursiveTarget<T> { next!: RecursiveTarget<T[]>; }
declare const source: RecursiveSource<string>;
const asserted = source as RecursiveTarget<string>;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        ["relation/class-projection-budget"]
    );
}

#[test]
fn project_assertions_relate_after_cross_file_class_publication() {
    let reports = check_project(vec![
        FileInput {
            name: "use.ts".into(),
            source: "import { Left, Right } from './classes';\ndeclare const left: Left;\nconst invalid = left as Right;".into(),
        },
        FileInput {
            name: "classes.ts".into(),
            source: "export class Left { private identity: number = 1; }\nexport class Right { private identity: number = 1; }".into(),
        },
    ]);

    assert_eq!(reports[0].name, "use.ts");
    assert!(
        reports
            .iter()
            .all(|report| report.output.parse_errors.is_empty()),
        "unexpected project parse error"
    );
    assert!(
        reports
            .iter()
            .all(|report| report.output.diagnostics.is_empty()),
        "unexpected project diagnostics"
    );
    assert_eq!(
        reports[0]
            .output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        ["expr-infer/as-assertion/compatibility"]
    );
    assert!(reports[1].output.incomplete.is_empty());
}

#[test]
fn asserted_types_still_feed_downstream_assignment_checks() {
    let source = "\
const asBad: number = \"value\" as string;
const angleBad: number = <string>\"value\";
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2322", "TK2322"]
    );
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
}

#[test]
fn optional_chain_assertions_schedule_once_and_keep_chain_accounting() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
declare const left: Left;
(left as Right)?.right;
(<Right>left)?.right;
(left as Left)?.left;
(<Left>left)?.left;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/optional-chain/self",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/optional-chain/self",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/optional-chain/self",
            "expr-infer/optional-chain/self",
        ]
    );
}

#[test]
fn update_target_assertions_schedule_once_with_valid_controls() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
declare let left: Left;
(left as Right)++;
(<Right>left)++;
(left as Left)++;
(<Left>left)++;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/as-assertion/compatibility",
            "expr-infer/type-assertion/compatibility",
        ]
    );
}

#[test]
fn update_target_nested_scope_static_members_use_syntax_only_accounting() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
interface Third { third: boolean; }
declare const left: Left;
++((function(local: Left) { MissingFunctionBody; return { prop: local as Right }; }).prop);
(((local: Left) => ({ prop: <Right>local })).prop)++;
++((function(local: Left) { return { prop: local as Left }; }).prop);
((((class { static prop = 0; field = left as Right; method() { MissingClassBody; } })!) satisfies unknown) as { prop: number }).prop++;
++((() => ({ prop: ((<Right>left) as Third) })).prop);
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/class-expression/self",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
        ]
    );
}

#[test]
fn update_target_nested_scope_without_assertions_still_records_gap_once() {
    let source = "\
++((function() { MissingFunctionBody; return { prop: 0 }; }).prop);
((() => { MissingArrowBody; return { prop: 0 }; }).prop)++;
++((class { static prop = 0; method() { MissingClassBody; } }).prop);
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/class-expression/self",
        ]
    );
}

#[test]
fn update_target_nested_scope_guard_covers_every_representable_target_family() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
declare const left: Left;
declare let values: number[];
++((function(local: Left) { MissingComputedObjectBody; return [local as Right]; })()[0]);
values[(() => { MissingComputedKeyBody; return 0; })]++;
values[class { field = left as Right }]++;
(values[() => 0] as unknown)++;
(<unknown>values[() => 0])++;
(values[() => 0] satisfies unknown)++;
(values[() => 0]!)++;
class Holder {
    #value = 0;
    run() { ((() => this).#value)++; }
}
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/class-expression/self",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/update-expression/nested-scope-target",
            "expr-infer/update-expression/nested-scope-target",
        ]
    );
}

#[test]
fn optional_chain_assertions_preserve_child_diagnostic_order() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
declare const left: Left;
((MissingFirst, left) as Right)?.right;
(<Right>(MissingSecond, left))?.right;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    let lines = crate::span::LineIndex::new(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                lines.line_of(diagnostic.span.start),
                diagnostic.code.as_str()
            ))
            .collect::<Vec<_>>(),
        [(4, "TK2304"), (5, "TK2304")]
    );
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/optional-chain/self",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/optional-chain/self",
            "expr-infer/type-assertion/compatibility",
        ]
    );
}

#[test]
fn assignment_lhs_assertions_schedule_once_with_valid_controls() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
declare let left: Left;
(left as Right) = { right: 1 };
(<Right>left) = { right: 1 };
(left as Left) = { left: \"ok\" };
(<Left>left) = { left: \"ok\" };
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/as-assertion/compatibility",
            "expr-infer/type-assertion/compatibility",
        ]
    );
}

#[test]
fn assignment_lhs_assertions_keep_child_then_rhs_diagnostic_order() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
declare let byKey: { [key: string]: Left };
(byKey[MissingAsChild] as Right) = MissingAsRhs;
(<Right>byKey[MissingAngleChild]) = MissingAngleRhs;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    let spans = output
        .diagnostics
        .iter()
        .map(|diagnostic| &source[diagnostic.span.range()])
        .collect::<Vec<_>>();
    assert_eq!(
        spans,
        [
            "MissingAsChild",
            "MissingAsRhs",
            "MissingAngleChild",
            "MissingAngleRhs",
        ]
    );
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/as-assertion/compatibility",
            "expr-infer/type-assertion/compatibility",
        ]
    );
}

#[test]
fn deferred_assignment_targets_keep_rhs_only_traversal() {
    let source = "\
declare let values: { [key: string]: number };
declare let first: number;
values[MissingComputedTarget] = MissingComputedRhs;
[first] = [MissingArrayRhs];
({ value: first } = { value: MissingObjectRhs });
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.span.range()])
            .collect::<Vec<_>>(),
        ["MissingComputedRhs", "MissingArrayRhs", "MissingObjectRhs",]
    );
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
}

#[test]
fn assignment_target_wrapper_matrix_finds_only_incompatible_assertions() {
    let source = "\
interface Left { left: string; slot: number; }
interface Right { right: number; slot: number; }
declare let left: Left;
declare let keyed: { [key: string]: number };
(left as Right)! = 1;
(<Right>left)! = 1;
((left as Right) satisfies Left) = 1;
((<Right>left) satisfies Left) = 1;
((left as Right)!)[0] = 1;
((<Right>left)!)[0] = 1;
keyed[(left as Right)!] = 1;
keyed[(<Right>left)!] = 1;
((left as Right)!).slot = 1;
((<Right>left)!).slot = 1;
class Holder {
    #slot = 0;
    check() {
        ((left as Right)!).#slot = 1;
        ((<Right>left)!).#slot = 1;
    }
}
[(left as Right)!] = [1];
[(<Right>left)! = 1] = [1];
[...(left as Right)!] = [];
({ value: (<Right>left)! } = { value: 1 });
({ [(left as Right)!]: (<Right>left)! } = {});
({ ...(<Right>left)! } = {});
(left as Left)! = 1;
(<Left>left)! = 1;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let ids = output
        .incomplete
        .iter()
        .filter(|incomplete| incomplete.id.ends_with("assertion/compatibility"))
        .map(|incomplete| incomplete.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 19, "{ids:?}");
    assert_eq!(
        ids.iter()
            .filter(|id| **id == "expr-infer/as-assertion/compatibility")
            .count(),
        9
    );
    assert_eq!(
        ids.iter()
            .filter(|id| **id == "expr-infer/type-assertion/compatibility")
            .count(),
        10
    );
}

#[test]
fn nested_assignment_target_assertions_are_scheduled_once_each() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
interface Third { third: boolean; }
declare let left: Left;
((<Right>left) as Third)! = 1;
(<Third>(left as Right))! = 1;
((left as Left) as Left)! = 1;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .filter(|incomplete| incomplete.id.ends_with("assertion/compatibility"))
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/type-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/type-assertion/compatibility",
        ]
    );
}

#[test]
fn assignment_target_assertions_keep_child_then_rhs_diagnostic_order() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
declare let left: Left;
declare let keyed: { [key: string]: number };
declare let leftByKey: { [key: string]: Left };
keyed[(leftByKey[MissingKeyChild] as Right)!] = MissingKeyRhs;
[(leftByKey[MissingArrayChild] as Right)!] = [MissingArrayRhs];
[left = leftByKey[MissingDefaultChild] as Right] = [MissingDefaultRhs];
({ [(leftByKey[MissingPropertyKeyChild] as Right)!]: (leftByKey[MissingBindingChild] as Right)! } = MissingObjectRhs);
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.span.range()])
            .collect::<Vec<_>>(),
        [
            "MissingKeyChild",
            "MissingKeyRhs",
            "MissingArrayChild",
            "MissingArrayRhs",
            "MissingDefaultChild",
            "MissingDefaultRhs",
            "MissingPropertyKeyChild",
            "MissingBindingChild",
            "MissingObjectRhs",
        ]
    );
    assert_eq!(
        output
            .incomplete
            .iter()
            .filter(|incomplete| incomplete.id.ends_with("assertion/compatibility"))
            .count(),
        5
    );
}

#[test]
fn nested_callable_assignment_target_shells_fallback_without_scope_leaks() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
interface Third { third: boolean; }
declare let left: Left;
declare let keyed: { [key: string]: number };
declare let sink: unknown;
keyed[<T extends Left>(local: T) => (MissingArrowChild, local) as Right] = MissingArrowRhs;
keyed[(local: Left) => { MissingBlockBody; const nested = (<Right>local) as Third; }] = MissingBlockRhs;
keyed[(local: Left) => { const valid = local as Left; }] = 1;
[sink = function(local: Left) { MissingFunctionBody; return <Right>local; }] = [MissingFunctionRhs];
[sink = function(local: Left) { return <Left>local; }] = [1];
keyed[class { field = left as Right; }] = 1;
keyed[<T extends Left>(local: T) => local as T] = 1;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.span.range()])
            .collect::<Vec<_>>(),
        ["MissingArrowRhs", "MissingBlockRhs", "MissingFunctionRhs",]
    );
    assert_eq!(
        output
            .incomplete
            .iter()
            .filter(|incomplete| incomplete.id.ends_with("assertion/compatibility"))
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/as-assertion/compatibility",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
        ]
    );
}

#[test]
fn static_member_nested_scope_bases_account_assertions_without_body_inference() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
interface Third { third: boolean; }
declare const left: Left;
(function(local: Left) { MissingFunctionBody; return { prop: local as Right }; }).prop = MissingFunctionRhs;
((local: Left) => ({ prop: <Right>local })).prop = MissingArrowRhs;
((((class { static prop = 0; field = left as Right; method() { MissingClassBody; } })!) satisfies unknown) as { prop: number }).prop = MissingClassRhs;
(function(local: Left) { return { prop: local as Left }; }).prop = MissingValidRhs;
((((() => ({ prop: 0 }))!) satisfies unknown) as { prop: number }).prop = MissingWrappedRhs;
(() => ({ prop: ((<Right>left) as Third) })).prop = MissingNestedRhs;
(() => class { field = left as Right }).prop = MissingNestedClassRhs;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.span.range()])
            .collect::<Vec<_>>(),
        [
            "MissingFunctionRhs",
            "MissingArrowRhs",
            "MissingClassRhs",
            "MissingValidRhs",
            "MissingWrappedRhs",
            "MissingNestedRhs",
            "MissingNestedClassRhs",
        ]
    );
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/class-expression/self",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/type-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/class-expression/self",
            "expr-infer/as-assertion/compatibility",
        ]
    );
}

#[test]
fn static_member_nested_scope_bases_always_record_the_base_gap() {
    let source = "\
(function() { MissingFunctionBody; return { prop: 0 }; }).prop = MissingFunctionRhs;
(() => { MissingArrowBody; return { prop: 0 }; }).prop = MissingArrowRhs;
(class { static prop = 0; method() { MissingClassBody; } }).prop = MissingClassRhs;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.span.range()])
            .collect::<Vec<_>>(),
        ["MissingFunctionRhs", "MissingArrowRhs", "MissingClassRhs",]
    );
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/class-expression/self",
        ]
    );
}

#[test]
fn static_member_nested_scope_base_pins_write_checks_and_compound_gap() {
    let source = "\
interface Box { readonly prop: number; }
(((function() { MissingMismatchBody; return { prop: 0 }; }) as unknown) as Box).prop = \"wrong\";
((() => ({ prop: 0 })) as Box).prop += MissingCompoundRhs;
(class { static readonly prop = 0; method() { MissingReadonlyBody; } }).prop = MissingReadonlyRhs;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str(), &source[diagnostic.span.range()]))
            .collect::<Vec<_>>(),
        [
            ("TK2304", "MissingCompoundRhs"),
            ("TK2304", "MissingReadonlyRhs"),
        ]
    );
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        [
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/as-assertion/compatibility",
            "expr-infer/assignment-expression/nested-scope-target",
            "expr-infer/class-expression/self",
        ]
    );
}

#[test]
fn assignment_nested_scope_guard_covers_every_representable_target_family() {
    let source = "\
interface Left { left: string; }
interface Right { right: number; }
declare const left: Left;
declare let values: number[];
declare let sink: unknown;
(function(local: Left) { MissingStaticBody; return { prop: local as Right }; }).prop = MissingStaticRhs;
(function(local: Left) { MissingComputedObjectBody; return [local as Right]; })()[0] = MissingComputedObjectRhs;
values[() => { MissingComputedKeyBody; return 0; }] = MissingComputedKeyRhs;
(values[() => 0] as unknown) = MissingAsRhs;
(<unknown>values[() => 0]) = MissingTypeRhs;
(values[() => 0] satisfies unknown) = MissingSatisfiesRhs;
values[() => 0]! = MissingNonNullRhs;
[values[() => 0]] = [MissingArrayRhs];
({ value: values[() => 0] } = { value: MissingObjectRhs });
[sink = function(local: Left) { MissingDefaultBody; return local as Right; }] = [MissingDefaultRhs];
[...values[() => 0]] = [];
({ [class { field = left as Right }]: values[() => 0] } = MissingKeyRhs);
class Holder {
    #value = 0;
    run() { (() => this).#value = MissingPrivateRhs; }
}
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| &source[diagnostic.span.range()])
            .collect::<Vec<_>>(),
        [
            "MissingStaticRhs",
            "MissingComputedObjectRhs",
            "MissingComputedKeyRhs",
            "MissingAsRhs",
            "MissingTypeRhs",
            "MissingSatisfiesRhs",
            "MissingNonNullRhs",
            "MissingArrayRhs",
            "MissingObjectRhs",
            "MissingDefaultRhs",
            "MissingKeyRhs",
            "MissingPrivateRhs",
        ]
    );
    let ids = output
        .incomplete
        .iter()
        .map(|incomplete| incomplete.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids.iter()
            .filter(|id| **id == "expr-infer/assignment-expression/nested-scope-target")
            .count(),
        13,
        "{ids:?}"
    );
    assert_eq!(
        ids.iter()
            .filter(|id| id.ends_with("assertion/compatibility"))
            .count(),
        6,
        "{ids:?}"
    );
    assert_eq!(
        ids.iter()
            .filter(|id| **id == "expr-infer/class-expression/self")
            .count(),
        1,
        "{ids:?}"
    );
}

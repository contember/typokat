//! Acceptance for committing library terminals demanded by user annotations.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn invoke(case_id: &str, source: &str) -> Value {
    let request = serde_json::to_string(&json!({
        "case_id": case_id,
        "schema": 1,
        "source": source,
    }))
    .expect("request JSON")
        + "\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_typokat-session-batch"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn isolated-session runner");
    child
        .stdin
        .take()
        .expect("runner stdin")
        .write_all(request.as_bytes())
        .expect("write runner request");
    let output = child.wait_with_output().expect("collect runner output");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).expect("runner receipt JSON")
}

#[test]
fn ordinary_record_annotation_commits_its_demanded_library_terminal() {
    let receipt = invoke(
        "record-annotation-publication",
        "function f(value: Record<string, unknown>) {}\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn global_array_binding_keeps_its_typed_incomplete_receipt() {
    let receipt = invoke("global-array-binding-publication", "var [value] = [1];\n");
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 3,
            "incompletes": ["bind/binding-pattern/array-pattern"],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_switch_retains_discriminated_union_members() {
    let receipt = invoke(
        "project-local-switch-narrowing",
        "type Item = { kind: true; a: string } | { kind: false; b: string };\n\
         function select(item: Item) {\n\
           switch (item.kind) {\n\
             case true: return item.a;\n\
             case false: return item.b;\n\
           }\n\
         }\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_in_operator_retains_alias_union_members() {
    let receipt = invoke(
        "project-local-in-narrowing",
        "type Item = { a: string } | { b: string };\n\
         function select(item: Item) {\n\
           if (\"a\" in item) return item.a;\n\
           return item.b;\n\
         }\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_enum_aliases_retain_comparison_overlap() {
    let receipt = invoke(
        "project-local-enum-alias-overlap",
        "enum Choice { Unknown, Yes, No }\n\
         type YesNo = Choice.Yes | Choice.No;\n\
         function inspect(a: Choice.Yes, b: YesNo) {\n\
           if (a === b) {}\n\
           switch (b) {\n\
             case Choice.Yes: break;\n\
             case Choice.No: break;\n\
           }\n\
         }\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_enum_alias_constraints_retain_comparison_overlap() {
    let receipt = invoke(
        "project-local-enum-alias-constraint-overlap",
        "enum Choice { Yes, No }\n\
         type YesNo = Choice.Yes | Choice.No;\n\
         function inspect<T extends YesNo>(value: T) {\n\
           if (value === Choice.Yes) {}\n\
           switch (value) { case Choice.Yes: break; }\n\
         }\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_nested_enum_alias_constraints_retain_comparison_overlap() {
    let receipt = invoke(
        "project-local-nested-enum-alias-constraint-overlap",
        "enum Choice { Yes, No }\n\
         type OnlyYes = Choice.Yes;\n\
         type WithNo<T> = T | Choice.No;\n\
         function inspect<T extends OnlyYes, U extends T | Choice.No>(value: U) {\n\
           if (value === Choice.Yes) {}\n\
           switch (value) { case Choice.Yes: break; }\n\
         }\n\
         function inspectAlias<T extends OnlyYes, U extends WithNo<T>>(value: U) {\n\
           if (value === Choice.Yes) {}\n\
           switch (value) { case Choice.Yes: break; }\n\
         }\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_disjoint_enum_aliases_remain_non_overlapping() {
    let receipt = invoke(
        "project-local-disjoint-enum-aliases",
        "enum Choice { Yes, No }\n\
         type OnlyYes = Choice.Yes;\n\
         type OnlyNo = Choice.No;\n\
         function constrained<T extends OnlyNo>(value: T) {\n\
           if (value === Choice.Yes) {}\n\
           switch (value) { case Choice.Yes: break; }\n\
         }\n\
         function aliases(left: OnlyNo, right: OnlyYes) {\n\
           if (left === right) {}\n\
           switch (left) { case right: break; }\n\
         }\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": ["5:2367", "6:2678", "9:2367", "10:2678"],
            "exit_code": 1,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_native_relation_identities_are_cold_order_independent() {
    for (case_id, source) in [
        (
            "project-local-boolean-wrapper-cold",
            "let source: boolean = true;\nlet target: Boolean = source;\n",
        ),
        (
            "project-local-number-wrapper-cold",
            "let source: number = 1;\nlet target: Number = source;\n",
        ),
        (
            "project-local-string-wrapper-cold",
            "let source: string = \"value\";\nlet target: String = source;\n",
        ),
        (
            "project-local-object-wrapper-cold",
            "let source: number = 1;\nlet target: Object = source;\n",
        ),
        (
            "project-local-number-wrapper-after-object",
            "let prior: Object = {};\n\
             let source: number = 1;\n\
             let target: Number = source;\n",
        ),
        (
            "project-local-wrapper-alias-target",
            "type Boxed = Boolean;\n\
             let source: boolean = true;\n\
             let target: Boxed = source;\n",
        ),
        (
            "project-local-wrapper-union-target",
            "let source: boolean = true;\n\
             let target: Boolean | { missing(): void } = source;\n",
        ),
        (
            "project-local-primitive-alias-source",
            "type Raw = boolean;\n\
             let source: Raw = true;\n\
             let target: Boolean = source;\n",
        ),
        (
            "project-local-constrained-primitive-source",
            "function assign<T extends boolean>(source: T) {\n\
               let target: Boolean = source;\n\
             }\n",
        ),
    ] {
        let receipt = invoke(case_id, source);
        assert_eq!(
            receipt["semantic"],
            json!({
                "diagnostics": [],
                "exit_code": 0,
                "incompletes": [],
                "parse_errors": [],
            }),
            "{case_id}"
        );
    }
}

#[test]
fn project_local_native_relation_context_keeps_augmentation_and_direction() {
    let receipt = invoke(
        "project-local-native-wrapper-augmentation",
        "interface Boolean { marker(): string }\n\
         interface Same { marker(): string }\n\
         interface Missing { missing(): string }\n\
         let primitive: boolean = true;\n\
         let wrapper: Boolean = primitive;\n\
         let structural: Same = primitive;\n\
         primitive = wrapper;\n\
         let missing: Missing = primitive;\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": ["7:2322", "8:2322"],
            "exit_code": 1,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_native_relation_context_reaches_every_relation_boundary() {
    for (case_id, source) in [
        (
            "project-local-native-nested-object-relation",
            "interface Source { value: boolean }\n\
             interface Target { value: Boolean }\n\
             declare let source: Source;\n\
             let target: Target = source;\n",
        ),
        (
            "project-local-native-nested-function-relation",
            "declare let source: (value: Boolean) => boolean;\n\
             let target: (value: boolean) => Boolean = source;\n",
        ),
        (
            "project-local-native-nested-index-relation",
            "interface Source { [key: string]: boolean }\n\
             interface Target { [key: string]: Boolean }\n\
             declare let source: Source;\n\
             let target: Target = source;\n",
        ),
        (
            "project-local-native-inference-relation",
            "declare function preserve<T extends Boolean>(value: T): T;\n\
             const inferred = preserve(true);\n\
             const exact: true = inferred;\n",
        ),
        (
            "project-local-native-overload-relation",
            "function convert(value: boolean): boolean;\n\
             function convert(value: Boolean): boolean { return true; }\n",
        ),
        (
            "project-local-native-flow-relation",
            "type Item =\n\
               | { kind: { length: number }; value: string }\n\
               | { kind: number; other: string };\n\
             function inspect(item: Item) {\n\
               if (item.kind === \"x\") return item.value;\n\
               return \"\";\n\
             }\n",
        ),
        (
            "project-local-native-multi-demand-relation",
            "interface BooleanSource { value: boolean }\n\
             interface BooleanTarget { value: Boolean }\n\
             interface NumberSource { value: number }\n\
             interface NumberTarget { value: Number }\n\
             declare let booleanSource: BooleanSource;\n\
             let booleanTarget: BooleanTarget = booleanSource;\n\
             declare let numberSource: NumberSource;\n\
             let numberTarget: NumberTarget = numberSource;\n",
        ),
    ] {
        let receipt = invoke(case_id, source);
        assert_eq!(
            receipt["semantic"],
            json!({
                "diagnostics": [],
                "exit_code": 0,
                "incompletes": [],
                "parse_errors": [],
            }),
            "{case_id}"
        );
    }
}

#[test]
fn project_local_class_surfaces_instantiate_zero_argument_aliases() {
    let receipt = invoke(
        "project-local-class-surface-zero-argument-alias",
        "type State = { data: number };\n\
         class C { state!: State; method() { const value: number = this.state.data; } }\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_deferred_keyof_satisfies_the_property_key_domain() {
    let receipt = invoke(
        "project-local-deferred-keyof-property-key-domain",
        "type DeferredRecord<S extends object> = Record<keyof S, () => void>;\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_signature_constraints_do_not_depend_on_alias_warmup_order() {
    let receipt = invoke(
        "project-local-signature-constraint-order",
        "type Warm = Record<string, string>;\n\
         function update<T extends Record<K, string>, K extends string>(obj: T): void {}\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_class_surfaces_demand_lazy_library_aliases() {
    let receipt = invoke(
        "project-local-class-surface-lazy-library-alias",
        "class Sample<P> { props!: Readonly<P>; }\n\
         class Derived<T> extends Sample<T> {}\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_inference_sees_published_alias_constraints() {
    let receipt = invoke(
        "project-local-inference-published-alias-constraint",
        "type Allowed = 0 | 1;\n\
         declare function get<I extends Allowed>(index: I): void;\n\
         get(0);\n\
         get(1);\n\
         get(2);\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": ["5:2345"],
            "exit_code": 1,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_constrained_template_infer_preserves_numeric_constraints() {
    let receipt = invoke(
        "project-local-constrained-template-infer",
        "type Parsed<S extends string> =\n\
           S extends `${infer N extends number}` ? N : never;\n\
         interface Indexer {\n\
           get<I extends Parsed<\"0\" | \"1\">>(index: I): void;\n\
         }\n\
         declare const value: Indexer;\n\
         value.get(0);\n\
         value.get(1);\n\
         value.get(2);\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": ["9:2345"],
            "exit_code": 1,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_computed_keys_use_the_certified_symbol_value() {
    let receipt = invoke(
        "project-local-certified-symbol-value",
        "type Branded = string & {\n  [Symbol.species]: \"brand\";\n};\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 0,
            "incompletes": [],
            "parse_errors": [],
        })
    );
}

#[test]
fn project_local_computed_keys_reject_a_shadowed_symbol_value() {
    let receipt = invoke(
        "project-local-shadowed-symbol-value",
        "function inspect(Symbol: { species: \"local\" }) {\n\
           type Branded = string & { [Symbol.species]: \"brand\" };\n\
         }\n",
    );
    assert_eq!(
        receipt["semantic"],
        json!({
            "diagnostics": [],
            "exit_code": 3,
            "incompletes": ["signature/property-signature/computed-key"],
            "parse_errors": [],
        })
    );
}

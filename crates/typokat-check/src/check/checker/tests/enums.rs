use crate::check::test_support::{
    check_project, check_source, check_source_with_enum_publication_trace,
};
use crate::frontend::FileInput;

fn codes(source: &str) -> Vec<String> {
    let out = check_source(source);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    out.diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_owned())
        .collect()
}

#[test]
fn enum_computed_initializer_uses_reserved_annotated_function_signature() {
    for source in [
        "declare function nextNumber(): number; enum E { A = nextNumber() }",
        "enum E { A = nextNumber() } declare function nextNumber(): number;",
    ] {
        let out = check_source(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
    }
}

#[test]
fn enum_function_signature_reservation_waits_for_interface_fill_in_both_orders() {
    for source in [
        "interface I { p: number } declare function f(x: I): number; enum E { A = f({}) }",
        "enum E { A = f({}) } declare function f(x: I): number; interface I { p: number }",
    ] {
        assert_eq!(codes(source), ["TK2345"]);
    }
}

#[test]
fn enum_class_typed_callable_uses_published_class_surface_in_both_orders() {
    for source in [
        "class C { p: number = 0 } declare function f(x: C): number; enum E { A = f({}) }",
        "enum E { A = f({}) } declare function f(x: C): number; class C { p: number = 0 }",
    ] {
        assert_eq!(codes(source), ["TK2345"]);
        let out = check_source(source);
        assert!(
            out.incomplete.is_empty(),
            "class-typed enum initializer must not defer: {:?}",
            out.incomplete
        );
    }
}

#[test]
fn enum_callable_with_class_surface_reaching_pending_enum_defers_without_false_diagnostic() {
    let out = check_source(
        "class C { value: E = E.A } declare function f(x: C): number; enum E { A = 1, B = f({ value: 0 }) }",
    );
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(!out.incomplete.is_empty(), "pending enum cycle must defer");
}

#[test]
fn enum_instance_relation_ignores_pending_enum_in_static_class_surface() {
    let out = check_source(
        "class C { p: number = 0; static value: E = E.A } declare function f(x: C): number; enum E { A = 1, B = f({ p: 0 }) }",
    );
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn duplicate_enum_member_initializer_is_checked_once() {
    let mut diagnostics = codes("enum E { A = 1, A = missing }");
    diagnostics.sort();
    assert_eq!(diagnostics, ["TK2300", "TK2300", "TK2304"]);

    let mut reference_diagnostics = codes("enum E { A = 1, A = A }");
    reference_diagnostics.sort();
    assert_eq!(reference_diagnostics, ["TK2300", "TK2300"]);
}

#[test]
fn duplicate_enum_member_keeps_first_endpoint_and_checks_both_initializers() {
    let out =
        check_source("enum E { A = firstMissing, A = secondMissing } const exact: E.A = E.A;");
    let mut diagnostics = out
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    diagnostics.sort_unstable();
    assert_eq!(diagnostics, ["TK2300", "TK2300", "TK2304", "TK2304"]);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn enum_template_initializer_visits_each_interpolation_once_without_generic_incomplete() {
    let numeric =
        check_source("declare function nextNumber(): number; enum E { A = `${nextNumber()}` }");
    assert_eq!(
        numeric
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK18033"]
    );
    assert!(numeric.incomplete.is_empty(), "{:?}", numeric.incomplete);

    let missing = check_source("enum E { A = `${missing}` }");
    assert_eq!(
        missing
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2304", "TK18033"]
    );
    assert!(missing.incomplete.is_empty(), "{:?}", missing.incomplete);
}

#[test]
fn enum_reference_diagnostics_do_not_suppress_sibling_initializer_errors() {
    let mut ordinary = codes("enum E { A = A + missing }");
    ordinary.sort();
    assert_eq!(ordinary, ["TK2304", "TK2565"]);

    let mut forward = codes("enum E { A = B + missing, B = 1 }");
    forward.sort();
    assert_eq!(forward, ["TK2304", "TK2651"]);

    let mut constant = codes("const enum E { A = A + missing }");
    constant.sort();
    assert_eq!(constant, ["TK2304", "TK2474", "TK2565"]);
}

#[test]
fn ambient_const_enum_uses_exact_auto_numbering() {
    for source in [
        "declare const enum E { A, B } const a: E.A = E.A; const b: E.B = E.B;",
        "declare const enum E { A = 4, B } const b: E.B = E.B;",
    ] {
        let out = check_source(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    }
}

#[test]
fn ordinary_ambient_missing_members_are_independently_opaque() {
    let out =
        check_source("declare enum E { Fixed = 4, Later, Text = 'x', AfterText } const e: E = 99;");
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

#[test]
fn nested_enum_remains_explicitly_incomplete() {
    let out = check_source("function f(x: string) { enum E { A = x } }");
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out
        .incomplete
        .iter()
        .any(|entry| entry.id == "decl/enum-declaration/self"));
}

#[test]
fn illegal_enum_merge_reports_every_owned_declaration() {
    let mut diagnostics = codes("enum E { A } function E(): void {}");
    diagnostics.sort();
    assert_eq!(diagnostics, ["TK2567", "TK2567"]);
}

#[test]
fn rejected_enum_function_group_publishes_no_enum_member_endpoint() {
    let out =
        check_source("enum E { A } function E(): void {} const member = E.A; const typed: E = 1;");
    let mut diagnostics = out
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    diagnostics.sort_unstable();
    assert_eq!(diagnostics, ["TK2339", "TK2567", "TK2567", "TK2709"]);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn rejected_enum_group_never_leaks_pending_shell_into_annotations() {
    for source in [
        "enum E { A } function E(): void {} type Alias = E; declare function use(value: E): void;",
        "type Alias = E; type Member = E.A; declare function use(value: E, member: E.A): void; enum E { A } function E(): void {}",
        "namespace E { export const tag = 1; } enum E { A } function E(): void {} type Alias = E; declare function use(value: E): void;",
    ] {
        let out = check_source(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
        assert_eq!(
            out.diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_str() == "TK2709")
                .count(),
            2,
            "{source}"
        );
    }
}

#[test]
fn qualified_enum_reference_diagnostic_owns_the_whole_expression() {
    for (source, code, needle) in [
        ("enum E { A = E.A }", "TK2565", "E.A"),
        ("enum E { A = E.B, B = 1 }", "TK2651", "E.B"),
    ] {
        let out = check_source(source);
        let diagnostic = out
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_str() == code)
            .expect("owned enum reference diagnostic");
        let start = usize::try_from(diagnostic.span.start).expect("span start");
        let end = usize::try_from(diagnostic.span.end).expect("span end");
        assert_eq!(&source[start..end], needle);
    }
}

#[test]
fn merged_enum_auto_numbering_restarts_for_each_fragment() {
    let source = "enum E { A = 1 } enum E { B } const b: E.B = E.B;";
    let out = check_source(source);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
}

#[test]
fn production_enum_publication_is_one_atomic_replay_component() {
    let (out, trace) =
        check_source_with_enum_publication_trace("enum E { First = 1 } enum E { Second = 2 }");
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
    assert_eq!(trace.reservations, 1);
    assert_eq!(trace.fills, 1);
    assert_eq!(trace.ordered_member_names, ["First", "Second"]);
    assert!(trace.readonly_value_properties);
    assert!(!trace.terminal_visible_before_freeze);

    let group = trace.enum_group.expect("type group owner");
    let values = trace
        .owners
        .iter()
        .copied()
        .filter(|owner| {
            matches!(
                owner,
                crate::check::checker::replay_index::ReplayOwner::Value(_)
            ) && trace.edges.contains(&(*owner, group))
                && trace.edges.contains(&(group, *owner))
        })
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2);
    for value in values {
        assert!(trace.edges.contains(&(value, group)));
        assert!(trace.edges.contains(&(group, value)));
    }
}

#[test]
fn enum_namespace_value_payload_joins_the_atomic_descriptor_in_both_orders() {
    for source in [
        "enum E { A = 'a' } namespace E { export const label = 'enum'; } const member: E.A = E.A; const label: string = E.label;",
        "namespace E { export const label = 'enum'; } enum E { A = 'a' } const member: E.A = E.A; const label: string = E.label;",
    ] {
        let (out, trace) = check_source_with_enum_publication_trace(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
        assert_eq!(trace.reservations, 1);
        assert_eq!(trace.fills, 1);
        assert_eq!(trace.ordered_member_names, ["A"]);
        assert!(trace.readonly_value_properties);
        assert!(!trace.terminal_visible_before_freeze);
    }
}

#[test]
fn namespace_interface_resolves_reserved_exact_enum_member_in_both_orders() {
    for source in [
        "enum Status { Ready = 'ready' } namespace Status { export interface Payload { status: Status.Ready; } } const payload: Status.Payload = { status: Status.Ready };",
        "namespace Status { export interface Payload { status: Status.Ready; } } enum Status { Ready = 'ready' } const payload: Status.Payload = { status: Status.Ready };",
    ] {
        let out = check_source(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
    }
}

#[test]
fn enum_namespace_member_collision_reports_both_owners_in_every_order() {
    for source in [
        "enum E { A = 1 } namespace E { export const A = 2; }",
        "namespace E { export const A = 2; } enum E { A = 1 }",
        "enum E { First = 0 } enum E { A = 1 } namespace E { export const A = 2; }",
        "enum E { A = 1 } namespace E { export interface A {} }",
        "namespace E { export type A = number; } enum E { A = 1 }",
    ] {
        let out = check_source(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
        assert_eq!(
            out.diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2300", "TK2300"],
            "{source}"
        );
    }
}

#[test]
fn enum_namespace_collision_census_does_not_repeat_existing_duplicate_owners() {
    for (source, expected) in [
        (
            "enum E { A = 1, A = 2 } namespace E { export const A = 3; }",
            3,
        ),
        (
            "enum E { A = 1 } namespace E { export interface A {} export interface A {} }",
            3,
        ),
    ] {
        let out = check_source(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
        assert_eq!(
            out.diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_str() == "TK2300")
                .count(),
            expected,
            "{source}"
        );
    }
}

#[test]
fn project_enum_namespace_slots_preserve_identity_across_root_order() {
    for inputs in [
        vec![
            FileInput {
                name: "source.ts".into(),
                source: "export enum Status { Ready = 'ready', Done = 'done' } export namespace Status { export const label = 'status'; export interface Payload { status: Status; } }".into(),
            },
            FileInput {
                name: "consumer.ts".into(),
                source: "import { Status } from './source'; const member: Status.Ready = Status.Ready; const payload: Status.Payload = { status: Status.Done }; const label: string = Status.label; const wrong: Status.Done = Status.Ready;".into(),
            },
        ],
        vec![
            FileInput {
                name: "consumer.ts".into(),
                source: "import { Status } from './source'; const member: Status.Ready = Status.Ready; const payload: Status.Payload = { status: Status.Done }; const label: string = Status.label; const wrong: Status.Done = Status.Ready;".into(),
            },
            FileInput {
                name: "source.ts".into(),
                source: "export enum Status { Ready = 'ready', Done = 'done' } export namespace Status { export const label = 'status'; export interface Payload { status: Status; } }".into(),
            },
        ],
    ] {
        let reports = check_project(inputs);
        assert!(reports
            .iter()
            .all(|report| report.output.parse_errors.is_empty()));
        assert!(reports
            .iter()
            .all(|report| report.output.incomplete.is_empty()));
        let diagnostics = reports
            .iter()
            .flat_map(|report| &report.output.diagnostics)
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>();
        assert_eq!(diagnostics, ["TK2322"]);
    }
}

#[test]
fn project_enum_initializer_is_deterministic_across_root_order() {
    for inputs in [
        vec![
            FileInput {
                name: "enum.ts".into(),
                source: "import { nextNumber } from './function'; enum E { A = nextNumber() }"
                    .into(),
            },
            FileInput {
                name: "function.ts".into(),
                source: "export declare function nextNumber(): number;".into(),
            },
        ],
        vec![
            FileInput {
                name: "function.ts".into(),
                source: "export declare function nextNumber(): number;".into(),
            },
            FileInput {
                name: "enum.ts".into(),
                source: "import { nextNumber } from './function'; enum E { A = nextNumber() }"
                    .into(),
            },
        ],
    ] {
        let reports = check_project(inputs);
        assert!(reports
            .iter()
            .all(|report| report.output.parse_errors.is_empty()));
        assert!(
            reports
                .iter()
                .all(|report| report.output.diagnostics.is_empty()),
            "{:?}",
            reports
                .iter()
                .map(|report| (&report.name, &report.output.diagnostics))
                .collect::<Vec<_>>()
        );
        assert!(
            reports
                .iter()
                .all(|report| report.output.incomplete.is_empty()),
            "{:?}",
            reports
                .iter()
                .map(|report| (&report.name, &report.output.incomplete))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn enum_annotations_are_stable_across_source_order_and_composition() {
    for source in [
        "enum E { A } function f<T extends E>(xs: E[], x: T): E { return x; }",
        "function f<T extends E>(xs: E[], x: T): E { return x; } enum E { A }",
    ] {
        let out = check_source(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
    }

    for source in [
        "declare function consume(x: E): number; enum E { A, B = consume(E.A) }",
        "enum E { A, B = consume(E.A) } declare function consume(x: E): number;",
    ] {
        let out = check_source(source);
        assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(out.incomplete.len(), 1, "{:?}", out.incomplete);
        assert_eq!(
            out.incomplete[0].id,
            "semantic-query/invalid-evaluation-node"
        );
    }
}

#[test]
fn enum_type_arguments_are_rejected_before_publication() {
    for source in [
        "enum E { A } type Bad = E<number>;",
        "type Bad = E<number>; enum E { A }",
    ] {
        let out = check_source(source);
        assert_eq!(
            out.diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2315"]
        );
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
    }
}

#[test]
fn enum_members_drive_access_calls_and_existing_operators() {
    let source = r#"
        enum Numeric { Zero }
        enum Text { Value = "value" }
        const zero: Numeric.Zero = Numeric["Zero"];
        const text: Text.Value = Text[`Value`];
        const n: number = Numeric.Zero - 1;
        const s: string = Text.Value + "!";
        Numeric.Missing;
        Numeric["Missing"];
        Numeric[`Missing`];
        Numeric();
        Numeric.Zero();
        Text.Value();
    "#;
    let out = check_source(source);
    assert_eq!(
        out.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2339", "TK2339", "TK2339", "TK2349", "TK2349", "TK2349"]
    );
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn heterogeneous_enum_typeof_splits_exact_members() {
    let source = r#"
        enum Mixed { Numeric = 0, Text = "text" }
        function inspect(value: Mixed) {
            if (typeof value === "number") {
                const ok: Mixed.Numeric = value;
                const bad: Mixed.Text = value;
            } else {
                const ok: Mixed.Text = value;
                const bad: Mixed.Numeric = value;
            }
        }
    "#;
    let out = check_source(source);
    assert_eq!(
        out.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2322", "TK2322"]
    );
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn enum_consumer_queries_are_deterministic_across_source_order() {
    for source in [
        r#"
            enum State { Ready = "ready", Done = "done" }
            function inspect(state: State) {
                if (state === State.Ready) {
                    const bad: State.Done = state;
                }
            }
            const missing = State.Missing;
        "#,
        r#"
            function inspect(state: State) {
                if (state === State.Ready) {
                    const bad: State.Done = state;
                }
            }
            const missing = State.Missing;
            enum State { Ready = "ready", Done = "done" }
        "#,
    ] {
        let out = check_source(source);
        assert_eq!(
            out.diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2322", "TK2339"]
        );
        assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
    }
}

#[test]
fn distinct_opaque_enum_members_do_not_overlap() {
    let source = r#"
        declare function next(): number;
        enum Opaque { First = next(), Second = next() }
        function inspect() {
            const value: Opaque.Second = Opaque.Second;
            if (value === Opaque.First) {
                const unreachable: never = value;
            } else {
                const kept: Opaque.Second = value;
            }
            switch (value) {
                case Opaque.First: {
                    const unreachable: never = value;
                    break;
                }
                default: {
                    const kept: Opaque.Second = value;
                }
            }
        }
    "#;
    let out = check_source(source);
    assert_eq!(
        out.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2367", "TK2678"]
    );
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn enum_equality_respects_type_parameter_constraints() {
    let source = r#"
        enum E { First, Second }
        enum S { First = "first" }
        function broad<T extends E>(value: T) {
            if (value === E.First) {
                const exact: E.First = value;
                const bad: E.Second = value;
            }
        }
        function unconstrained<T>(value: T) {
            if (value === E.First) {
                const stillGeneric: E.First = value;
            }
        }
        function primitive<T extends string>(value: T) {
            if (value === S.First) {
                const stillGeneric: S.First = value;
            }
        }
    "#;
    let out = check_source(source);
    assert_eq!(
        out.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2322", "TK2322", "TK2322"]
    );
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn enum_equality_refines_broad_primitives_to_exact_members() {
    let source = r#"
        enum Numeric { First, Second }
        enum Text { First = "first", Second = "second" }
        function numeric(value: number) {
            if (value === Numeric.First) {
                const exact: Numeric.First = value;
                const bad: Numeric.Second = value;
            }
        }
        function text(value: string) {
            if (value === Text.First) {
                const exact: Text.First = value;
                const bad: Text.Second = value;
            }
        }
        function rawLiteral(value: "first") {
            if (value === Text.First) {
                const stillRaw: Text.First = value;
            }
        }
    "#;
    let out = check_source(source);
    assert_eq!(
        out.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2322", "TK2322", "TK2322"]
    );
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn enum_typeof_respects_type_parameter_constraints() {
    let source = r#"
        enum Numeric { First, Second }
        function inspect<T extends Numeric>(value: T) {
            if (typeof value === "number") {
                const kept: T = value;
            } else {
                const unreachable: never = value;
            }
        }
    "#;
    let out = check_source(source);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

#[test]
fn enum_constrained_comparisons_emit_disjoint_raw_diagnostics() {
    let source = r#"
        enum Numeric { Zero = 0, One = 1 }
        function inspect<T extends Numeric>(value: T) {
            if (value === 99) {
                const unreachable: never = value;
            }
            switch (value) {
                case 99: {
                    const unreachable: never = value;
                    break;
                }
            }
        }
    "#;
    let out = check_source(source);
    assert_eq!(
        out.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2367", "TK2678"]
    );
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
}

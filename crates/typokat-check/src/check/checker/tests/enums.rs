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

//! Flat object binding defaults: an unannotated default contributes its own type
//! (`union(source minus undefined, default)` under declaration widening) while an
//! annotated pattern keeps the default obligation and the declared leaf type.
//!
//! Literal modes are observed through a `let` copy: a surviving widening member lets
//! `copy = "other"` pass, a regular member rejects it (ADR-0031 collapse).

use crate::check::test_support::check_source;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    let index = crate::span::LineIndex::new(source);
    let mut rows: Vec<(u32, String)> = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                index.line_of(diagnostic.span.start),
                diagnostic.code.as_str().to_owned(),
            )
        })
        .collect();
    rows.sort();
    rows
}

struct Report {
    diagnostics: Vec<(u32, String)>,
    incomplete: Vec<(u32, String)>,
}

fn report(source: &str) -> Report {
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    let index = crate::span::LineIndex::new(source);
    let mut diagnostics: Vec<(u32, String)> = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                index.line_of(diagnostic.span.start),
                diagnostic.code.as_str().to_owned(),
            )
        })
        .collect();
    diagnostics.sort();
    let mut incomplete: Vec<(u32, String)> = output
        .incomplete
        .iter()
        .map(|incomplete| (index.line_of(incomplete.span.start), incomplete.id.clone()))
        .collect();
    incomplete.sort();
    Report {
        diagnostics,
        incomplete,
    }
}

const UNTYPED_DEFAULT: &str = "bind/binding-pattern/assignment-pattern";

fn tk2322_at(lines: &[u32]) -> Vec<(u32, String)> {
    lines
        .iter()
        .map(|line| (*line, "TK2322".to_owned()))
        .collect()
}

const OPTIONAL_SOURCE: &str = "declare const source: { value?: \"bar\" };\n";

#[test]
fn let_same_value_default_keeps_the_literal_and_different_value_widens() {
    let source = format!(
        "{OPTIONAL_SOURCE}let {{ value: same = \"bar\" }} = source;
let {{ value: different = \"foo\" }} = source;
const sameRead: \"bar\" = same;
same = \"different\";
different = \"different\";
const differentWrong: \"bar\" | \"foo\" = different;
"
    );
    assert_eq!(diagnostics(&source), tk2322_at(&[5, 7]));
}

#[test]
fn const_default_combination_retains_both_modes() {
    let source = format!(
        "{OPTIONAL_SOURCE}const {{ value: fixed = \"foo\" }} = source;
const {{ value: fixedSame = \"bar\" }} = source;
const read: \"bar\" | \"foo\" = fixed;
const wrong: \"bar\" = fixed;
let copyFixed = fixed;
copyFixed = \"other\";
const sameRead: \"bar\" = fixedSame;
let copySame = fixedSame;
copySame = \"other\";
"
    );
    assert_eq!(diagnostics(&source), tk2322_at(&[5, 10]));
}

#[test]
fn explicit_non_widening_default_combines_regular_literals() {
    let source = format!(
        "{OPTIONAL_SOURCE}let {{ value: asserted = \"foo\" as const }} = source;
let {{ value: exact = \"foo\" as \"foo\" }} = source;
asserted = \"bar\";
asserted = \"foo\";
asserted = \"different\";
let copyAsserted = asserted;
copyAsserted = \"other\";
let copyExact = exact;
copyExact = \"other\";
"
    );
    assert_eq!(diagnostics(&source), tk2322_at(&[6, 8, 10]));
}

#[test]
fn undefined_only_source_publishes_the_widened_default() {
    let source = "declare const source: { value: undefined };
let { value: only = \"bar\" } = source;
let { value: literalOnly = \"bar\" } = { value: undefined };
only = \"different\";
only = 1;
literalOnly = \"different\";
literalOnly = 1;
";
    assert_eq!(diagnostics(source), tk2322_at(&[5, 7]));
}

#[test]
fn mixed_kind_default_widens_only_its_own_contribution() {
    let source = format!(
        "{OPTIONAL_SOURCE}let {{ value: mixed = 123 }} = source;
mixed = 456;
mixed = \"bar\";
mixed = \"wrong\";
"
    );
    assert_eq!(diagnostics(&source), tk2322_at(&[5]));
}

#[test]
fn annotated_pattern_keeps_the_default_obligation_and_declared_leaf() {
    let source = format!(
        "{OPTIONAL_SOURCE}let {{ value: annotated = \"foo\" }}: {{ value?: \"bar\" }} = source;
annotated = \"bar\";
annotated = \"foo\";
const {{ value: annotatedEmpty = \"foo\" }}: {{ value?: \"bar\" }} = {{}};
const annotatedEmptyRead: \"bar\" = annotatedEmpty;
"
    );
    assert_eq!(diagnostics(&source), tk2322_at(&[2, 4, 5]));
}

#[test]
fn callback_default_keeps_the_source_parameter_context() {
    let source = "declare const source: { cb?: (n: number) => number };
let { cb: callback = (n) => {
  const wrong: string = n;
  return n;
} } = source;
callback(\"wrong\");
callback(1);
";
    assert_eq!(
        diagnostics(source),
        vec![(3, "TK2322".to_owned()), (6, "TK2345".to_owned())]
    );
}

#[test]
fn nullable_source_keeps_null_and_removes_only_undefined() {
    let source = "declare const source: { value: \"bar\" | null | undefined };
let { value: nullable = \"foo\" as const } = source;
nullable = null;
nullable = \"foo\";
nullable = undefined;
";
    assert_eq!(diagnostics(source), tk2322_at(&[5]));
}

#[test]
fn error_typed_default_keeps_the_source_leaf_and_its_own_diagnostic() {
    let source = format!(
        "{OPTIONAL_SOURCE}let {{ value: broken = undeclaredDefault }} = source;
broken = \"bar\";
broken = 1;
"
    );
    let checked = report(&source);
    assert_eq!(
        checked.diagnostics,
        vec![(2, "TK2304".to_owned()), (4, "TK2322".to_owned())]
    );
    assert!(checked.incomplete.is_empty(), "{:?}", checked.incomplete);
}

#[test]
fn later_leaves_and_missing_properties_keep_their_diagnostics() {
    let source = "declare const source: { first: string; value?: \"bar\" };
let { first, value: fromDefault = \"foo\", missing = 1 } = source;
const firstWrong: number = first;
const fromDefaultWrong: number = fromDefault;
";
    assert_eq!(
        diagnostics(source),
        vec![
            (2, "TK2339".to_owned()),
            (3, "TK2322".to_owned()),
            (4, "TK2322".to_owned()),
        ]
    );
}

#[test]
fn silently_untyped_defaults_keep_the_source_leaf_and_record_the_pattern_gap() {
    let cases = [
        ("declare const src: { value?: string };\nconst { value: v = this } = src;\nconst bad: number = v;\n", 2, 3),
        ("declare const src: { value?: string };\nfunction f() {\n  const { value: v = new.target } = src;\n  const bad: number = v;\n}\n", 3, 4),
        ("declare const src: { value?: string };\nconst { value: v = import.meta.url } = src;\nconst bad: number = v;\n", 2, 3),
        ("declare const src: { value?: string };\nconst { value: v = w } = src;\nconst w = v;\nconst bad: number = v;\n", 2, 4),
        ("declare const src: { value?: string };\nconst { value: v = v } = src;\nconst bad: number = v;\n", 2, 3),
        ("declare const src: { value?: string };\nconst o = {\n  d: \"s\",\n  m() {\n    const { value: v = this.d } = src;\n    const bad: number = v;\n  },\n};\n", 5, 6),
        ("declare const src: { value?: string };\nclass C {\n  m() {\n    const { value: v = super.toString() } = src;\n    const bad: number = v;\n  }\n}\n", 4, 5),
    ];
    for (source, pattern_line, use_line) in cases {
        let checked = report(source);
        assert_eq!(checked.diagnostics, tk2322_at(&[use_line]), "{source}");
        assert_eq!(
            checked.incomplete,
            vec![(pattern_line, UNTYPED_DEFAULT.to_owned())],
            "{source}"
        );
    }
}

#[test]
fn child_recorded_untyped_defaults_keep_the_source_leaf_without_a_second_record() {
    let tagged = "declare const src: { value?: string };
declare function tag(s: unknown): string;
const { value: v = tag`hi` } = src;
const bad: number = v;
v.toFixed(2);
v();
";
    let checked = report(tagged);
    assert_eq!(
        checked.diagnostics,
        vec![
            (4, "TK2322".to_owned()),
            (5, "TK2339".to_owned()),
            (6, "TK2349".to_owned()),
        ]
    );
    assert_eq!(
        checked.incomplete,
        vec![(3, "expr-infer/tagged-template/self".to_owned())]
    );

    let cases = [
        ("declare const src: { value?: string };\ndeclare const ready: string;\nasync function f() {\n  const { value: v = await ready } = src;\n  const bad: number = v;\n}\n", 4, "expr-infer/await-expression/self", 5),
        ("declare const src: { value?: unknown };\nconst { value: v = class {} } = src;\nconst bad: number = v;\n", 2, "expr-infer/class-expression/self", 3),
        ("declare const src: { value?: string };\nfunction* g() {\n  const { value: v = (yield 1) } = src;\n  const bad: number = v;\n}\n", 3, "expr-infer/yield-expression/self", 4),
    ];
    for (source, gap_line, gap_id, use_line) in cases {
        let checked = report(source);
        assert_eq!(checked.diagnostics, tk2322_at(&[use_line]), "{source}");
        assert_eq!(
            checked.incomplete,
            vec![(gap_line, gap_id.to_owned())],
            "{source}"
        );
    }

    let multi_leaf = "declare const src: { a?: string; b?: string };
declare function tag(s: unknown): string;
const { a = tag`x`, b = \"ok\" } = src;
const badA: number = a;
const badB: number = b;
let { value: mutable = tag`z` } = src as { value?: string };
mutable = 5;
const badMutable: number = mutable;
";
    let checked = report(multi_leaf);
    assert_eq!(checked.diagnostics, tk2322_at(&[4, 5, 7, 8]));
    assert_eq!(
        checked.incomplete,
        vec![
            (3, "expr-infer/tagged-template/self".to_owned()),
            (6, "expr-infer/tagged-template/self".to_owned()),
        ]
    );
}

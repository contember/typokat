//! `Enum[readyKey] as const`: a ready singleton string identifier key at a direct enum
//! value root yields the same member as the syntactic `Enum.A as const`; aliases,
//! shadows and unavailable keys stay outside the modeled operand syntax.

use crate::check::test_support::check_source;

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

const ENUM: &str = "enum Keyed { A, B }\n";

#[test]
fn ready_parenthesized_identifier_key_behaves_as_the_syntactic_member() {
    let source = format!(
        "{ENUM}const key = \"A\";
const ready = (Keyed)[(key)] as const;
const syntactic = Keyed.A as const;
const exact: Keyed.A = ready;
const wrong: Keyed.B = ready;
let widenedReady = ready;
widenedReady = Keyed.B;
let widenedSyntactic = syntactic;
widenedSyntactic = Keyed.B;
"
    );
    let report = report(&source);
    assert_eq!(
        report.diagnostics,
        vec![
            (6, "TK2322".to_owned()),
            (8, "TK2322".to_owned()),
            (10, "TK2322".to_owned()),
        ]
    );
    assert!(report.incomplete.is_empty(), "{:?}", report.incomplete);
}

#[test]
fn missing_identifier_key_keeps_the_indexed_access_gap() {
    let source =
        format!("{ENUM}const missing = \"Missing\";\nconst value = Keyed[missing] as const;\n");
    let report = report(&source);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        report.incomplete,
        vec![(3, "expr-infer/element-access/implicit-any-index".to_owned())]
    );
}

#[test]
fn alias_shadow_and_unready_keys_stay_invalid_operands() {
    let source = format!(
        "{ENUM}const key = \"A\";
const Alias = Keyed;
const aliased = Alias[key] as const;
function shadow(Keyed: {{ A: number }}) {{
  const shadowed = Keyed[key] as const;
}}
let mutableKey = \"A\";
const unready = Keyed[mutableKey] as const;
const objectKey = ({{ A: 1 }})[key] as const;
"
    );
    let report = report(&source);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let invalid = "expr-infer/as-const/invalid-operand".to_owned();
    assert_eq!(
        report.incomplete,
        vec![
            (4, invalid.clone()),
            (6, invalid.clone()),
            (9, invalid.clone()),
            (10, invalid),
        ]
    );
}

#[test]
fn unresolved_key_walks_once_and_stays_invalid() {
    let source = format!("{ENUM}const value = Keyed[undeclaredKey] as const;\n");
    let report = report(&source);
    assert_eq!(report.diagnostics, vec![(2, "TK2304".to_owned())]);
    assert_eq!(
        report.incomplete,
        vec![(2, "expr-infer/as-const/invalid-operand".to_owned())]
    );
}

#[test]
fn class_field_seeds_a_ready_annotated_key_at_a_direct_enum_root() {
    let source = format!(
        "{ENUM}declare const publishedKey: \"A\";
class Consumer {{
  field = Keyed[publishedKey] as const;
  wrapped = (Keyed)[(publishedKey)] as const;
}}
declare const consumer: Consumer;
const exact: Keyed.A = consumer.field;
const wrappedExact: Keyed.A = consumer.wrapped;
const wrong: Keyed.B = consumer.field;
const wrappedWrong: Keyed.B = consumer.wrapped;
"
    );
    let report = report(&source);
    assert_eq!(
        report.diagnostics,
        vec![(10, "TK2322".to_owned()), (11, "TK2322".to_owned())]
    );
    assert!(report.incomplete.is_empty(), "{:?}", report.incomplete);
}

#[test]
fn class_field_alias_root_and_missing_member_stay_unpublished() {
    let source = format!(
        "{ENUM}declare const publishedKey: \"A\";
declare const missingKey: \"Missing\";
const Alias = Keyed;
class Consumer {{
  aliased = Alias[publishedKey] as const;
  missing = Keyed[missingKey] as const;
}}
"
    );
    let report = report(&source);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert!(
        report
            .incomplete
            .iter()
            .any(|(line, id)| *line == 6 && id == "expr-infer/as-const/invalid-operand"),
        "{:?}",
        report.incomplete
    );
    assert!(
        report
            .incomplete
            .iter()
            .any(|(line, id)| *line == 7 && id == "expr-infer/element-access/implicit-any-index"),
        "{:?}",
        report.incomplete
    );
}

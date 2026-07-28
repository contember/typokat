//! M19 end-to-end tests for index signatures.
//! Pins indexed access, object-literal assignability, and excess-property
//! suppression only for index-signature targets. Fixture acceptance lives in
//! `m19_index_sig/`.

use crate::check::test_support::check_source;

/// Run the checker and return the sorted `(1-based line, code)` of every
/// diagnostic, keyed on its primary-span start line (matching the conformance
/// harness's mapping).
fn diags(source: &str) -> Vec<(u32, String)> {
    let out = check_source(source);
    assert!(
        out.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        out.parse_errors
    );
    let index = crate::span::LineIndex::new(source);
    let mut v: Vec<(u32, String)> = out
        .diagnostics
        .iter()
        .map(|d| (index.line_of(d.span.start), d.code.as_str().to_string()))
        .collect();
    v.sort();
    v
}

/// Object-literal → string-index-sig **assignability** is value-checked: every
/// member value must fit the index value type. All-`number` values are ok; a
/// `string` value is **one** `TK2322` (not one per excess key — excess is
/// suppressed against an index-sig target).
#[test]
fn object_literal_to_string_index_is_value_checked() {
    let src = "\
const dict: { [k: string]: number } = { a: 1, b: 2 };
const bad: { [k: string]: number } = { a: 1, b: \"x\" };
const empty: { [k: string]: number } = {};
";
    // Line 1 ok; line 2 one TK2322 (b's value string ≁ number); line 3 ok.
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// **Access** through a string index signature yields its value type, both via
/// element access `dict["a"]` / dynamic `dict[key]` and via property access
/// `dict.a`. The value type is `number`, so binding it to `number` is ok and to
/// `string` is `TK2322`.
#[test]
fn access_through_string_index_yields_value_type() {
    let src = "\
const dict: { [k: string]: number } = { a: 1 };
const v: number = dict[\"a\"];
const v2: number = dict.a;
const v3: string = dict[\"a\"];
let key: string = \"k\";
const v4: number = dict[key];
";
    // Only line 4 fails (number value bound to a string annotation).
    assert_eq!(diags(src), vec![(4, "TK2322".to_string())]);
}

/// A **number** index signature: numeric-named object-literal members are
/// accepted (no excess), and numeric-keyed access yields the value type — here
/// `string`, so `nums[0]` is ok against `string` and `TK2322` against `number`.
#[test]
fn number_index_access_and_assignability() {
    let src = "\
const nums: { [i: number]: string } = { 0: \"a\", 1: \"b\" };
const s: string = nums[0];
const sBad: number = nums[0];
";
    // Line 1 ok (numeric members accepted); line 3 is the value-type mismatch.
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// The excess-property check (`TK2353`) is suppressed **only** against an
/// index-sig target. A plain object target (no index signature) STILL reports
/// excess (M2 unchanged) — so the suppression is scoped, not global.
#[test]
fn excess_suppressed_only_for_index_sig_targets() {
    let src = "\
const ok: { [k: string]: number } = { a: 1, b: 2 };
const bad: { a: number } = { a: 1, x: 0 };
";
    // Line 1 has NO excess (index-sig target accepts `b`); line 2 reports the
    // excess `x` (plain object target — M2 behaviour preserved).
    assert_eq!(diags(src), vec![(2, "TK2353".to_string())]);
}

/// A property NOT present and NOT covered by a string index signature is still
/// `TK2339`: a number index signature does not provide string-keyed property
/// access, so `obj.missing` on a number-index-only object is missing.
#[test]
fn property_not_covered_by_number_index_is_missing() {
    let src = "\
const obj: { [i: number]: string } = { 0: \"a\" };
const x = obj.missing;
";
    // `missing` is a string key; a number index signature does not cover it.
    assert_eq!(diags(src), vec![(2, "TK2339".to_string())]);
}

/// **Freshness descends into an index value** (the M2 excess check is not lost
/// under an index signature): a fresh object literal used as a **string** index
/// value still gets its own excess check, so a property the value type does not
/// name is `TK2353`. The key itself stays suppressed (any key is accepted).
#[test]
fn fresh_value_under_string_index_still_excess_checked() {
    let src = "\
const a: { [k: string]: { v: number } } = { foo: { v: 1, extra: 9 } };
";
    // `foo` (the key) is accepted by the index sig, but `extra` is excess against
    // the value type `{ v: number }` → one TK2353 on `extra`.
    assert_eq!(diags(src), vec![(1, "TK2353".to_string())]);
}

/// The same freshness descent applies to a **number** index value: a fresh object
/// literal at a numeric key is excess-checked against the number index value type.
#[test]
fn fresh_value_under_number_index_still_excess_checked() {
    let src = "\
const a: { [i: number]: { v: number } } = { 0: { v: 1, extra: 9 } };
";
    // `0` (numeric key) is accepted, but `extra` is excess against `{ v: number }`.
    assert_eq!(diags(src), vec![(1, "TK2353".to_string())]);
}

/// Scoping intact: a value that **does** fit the index value type produces no
/// excess (the descent only reports a genuinely-excess nested property), and the
/// key is never itself reported as excess.
#[test]
fn fresh_value_matching_index_value_is_clean() {
    let src = "\
const a: { [k: string]: { v: number } } = { foo: { v: 1 } };
";
    assert!(
        diags(src).is_empty(),
        "a matching nested value must be clean"
    );
}

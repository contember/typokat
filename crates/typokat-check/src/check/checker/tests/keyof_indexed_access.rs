//! M20 end-to-end tests for `keyof T` and indexed-access types.
//! Pins eager concrete-object evaluation, index-signature keys, and graceful
//! error-type fallback for missing/out-of-scope cases. Fixture acceptance lives
//! in `m20_keyof/`.

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

/// `keyof T` for an object is the `union(...)` of its property **names** as
/// **string-literal** types: `keyof { x; y }` is `"x" | "y"`. A member of that
/// union is ok; a string outside it is `TK2322`.
#[test]
fn keyof_object_is_key_literal_union() {
    let src = "\
interface Point { x: number; y: number; }
let k: keyof Point = \"x\";
k = \"y\";
k = \"z\";
";
    // Lines 2–3 ok ("x"/"y" are keys); line 4 fails ("z" ∉ "x" | "y").
    assert_eq!(diags(src), vec![(4, "TK2322".to_string())]);
}

/// Class applications defer `keyof` until the query coordinator projects their
/// public instance surface; private and protected members never become keys.
#[test]
fn keyof_class_projects_only_public_members() {
    let src = "\
class Model {
  public visible: number;
  private secret: string;
  protected inherited: boolean;
}
let key: keyof Model = \"visible\";
key = \"secret\";
key = \"inherited\";
";
    assert_eq!(
        diags(src),
        vec![(7, "TK2322".to_string()), (8, "TK2322".to_string())]
    );
}

/// `keyof {}` (no members, no index signature) is **`never`** (the empty union
/// collapses to `never`). Nothing is assignable to `never`, so binding a string to
/// it is `TK2322` — and, crucially, the empty case does not crash.
#[test]
fn keyof_empty_object_is_never() {
    let src = "\
type Empty = {};
let k: keyof Empty = \"anything\";
";
    // `keyof {}` is `never`; the string literal is not assignable to `never`.
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// `keyof T` includes the **index-signature key type**. A string index sig covers
/// numeric keys too (tsc: numeric keys coerce to strings), so `keyof { [k: string]: V }`
/// is `string | number` — both a string and a number are assignable to it. A number index
/// sig contributes only `number`.
#[test]
fn keyof_includes_index_signature_keys() {
    let src = "\
type Dict = { [k: string]: number };
let sk: keyof Dict = \"k\";
let skNum: keyof Dict = 1;
type NumDict = { [i: number]: string };
let nk: keyof NumDict = 0;
let nkBad: keyof NumDict = \"x\";
";
    // `keyof Dict` is `string | number` (lines 2 and 3 both clean — a string AND a number
    // are keys); `keyof NumDict` is `number` (line 6: string ≁ number).
    assert_eq!(diags(src), vec![(6, "TK2322".to_string())]);
}

/// An indexed-access type `T["literal"]` is the type of that named property:
/// `Rec["a"]` is `number`. Binding `number` is ok; binding `string` is `TK2322`.
#[test]
fn indexed_access_string_literal_is_property_type() {
    let src = "\
interface Rec { a: number; b: string; }
const r1: Rec[\"a\"] = 1;
const r2: Rec[\"b\"] = \"s\";
const rBad: Rec[\"a\"] = \"s\";
";
    // Line 4 fails (`Rec["a"]` is `number`, not `string`).
    assert_eq!(diags(src), vec![(4, "TK2322".to_string())]);
}

/// A **union** key distributes: `T["a" | "b"]` is `T["a"] | T["b"]`. For
/// `Rec` that is `number | string`, so a `number` and a `string` both fit, but a
/// `boolean` does not (`TK2322`).
#[test]
fn indexed_access_union_key_distributes() {
    let src = "\
interface Rec { a: number; b: string; }
const u: Rec[\"a\" | \"b\"] = 1;
const u2: Rec[\"a\" | \"b\"] = \"s\";
const uBad: Rec[\"a\" | \"b\"] = true;
";
    // Lines 2–3 ok (number/string ∈ number | string); line 4 fails (boolean ∉).
    assert_eq!(diags(src), vec![(4, "TK2322".to_string())]);
}

/// `T[keyof T]` is the union of **all** value types. For `Point` (both props
/// `number`) the union dedups to plain `number`, so `"s"` is `TK2322` while `1` is
/// ok — exercising the `keyof` → union-key → indexed-access composition end to end.
#[test]
fn indexed_access_keyof_yields_value_union() {
    let src = "\
interface Point { x: number; y: number; }
const all: Point[keyof Point] = 1;
const allBad: Point[keyof Point] = \"s\";
";
    // `Point[keyof Point]` is `number` (deduped); line 3 fails.
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// A missing key degrades to the error type without a crash or cascade.
#[test]
fn indexed_access_missing_key_is_error_type_no_crash() {
    let src = "\
interface Rec { a: number; }
const bad: Rec[\"zzz\"] = 1;
const used: string = bad;
";
    // `Rec["zzz"]` is the error type: no diagnostic on its declaration, and it is
    // freely assignable (error suppresses cascade) — so the whole snippet is clean.
    assert!(
        diags(src).is_empty(),
        "a missing indexed-access key must fall back to the error type silently"
    );
}

/// `keyof` of a **non-object** (here a primitive) is out of the M20 scope → the
/// error type, silently (no diagnostic, no crash). `keyof number` is not the
/// object case, so the result is `any`-like and binding anything to it is clean.
#[test]
fn keyof_non_object_is_error_type_no_crash() {
    let src = "\
let k: keyof number = \"anything\";
";
    // `keyof number` is out of scope → error type; no diagnostic.
    assert!(
        diags(src).is_empty(),
        "`keyof` of a non-object must fall back to the error type silently"
    );
}

/// Indexed access into a **tuple** by a numeric-literal key yields the positional
/// **element** type (reusing the M18 positional lookup): `T[0]` is the first
/// element's type. An out-of-range index falls back to the error type (no crash).
#[test]
fn indexed_access_tuple_numeric_literal_is_element() {
    let src = "\
type T = [number, string];
const first: T[0] = 1;
const firstBad: T[0] = \"s\";
const second: T[1] = \"s\";
const oob: T[5] = 123;
";
    // `T[0]` is `number` (line 3 fails: string ≁ number); `T[1]` is `string`
    // (line 4 ok); `T[5]` is out of range → error type (line 5 clean).
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// Generic `keyof T` stays a deferred node and relates conservatively; generic
/// indexed access remains the silent M20 out-of-scope fallback.
#[test]
fn generic_keyof_defers_and_indexed_access_falls_back_no_crash() {
    let src = "\
function f<T>(x: T): void {
  let k: keyof T = x;
  let v: T[\"a\"] = x;
}
";
    // Line 2: `T` ≁ deferred `keyof T` → TK2322 (the sound over-report tsc shares).
    // Line 3: `T["a"]` is still the error type → clean (cascade suppressed). No panic.
    assert_eq!(
        diags(src),
        vec![(2, "TK2322".to_string())],
        "deferred keyof rejects a plain T source; generic indexed access stays silent"
    );
}

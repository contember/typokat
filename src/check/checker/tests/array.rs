//! M17 end-to-end tests for array types.
//! Pins widened element-union inference, `never[]`, element access, synthesized
//! `length`, `Array<T>` equivalence, and covariant assignability. Fixture
//! acceptance lives in `m17_arrays/`.

use crate::driver::check_source;

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

/// Array literals infer the union of widened element types: homogeneous numbers
/// become `number[]`, while `[1, "x"]` rejects a `number[]` annotation.
#[test]
fn array_literal_element_inference_is_union() {
    let src = "\
const nums: number[] = [1, 2, 3];
const bad: number[] = [1, \"x\"];
";
    // Line 1 ok (number[]); line 2 is (number | string)[] → number[] → TK2322.
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// An **empty** array literal `[]` infers `never[]`, which is assignable to any
/// `T[]` (the bottom element under covariance) — so it is accepted against a
/// `number[]` and a `string[][]` alike, with no diagnostic.
#[test]
fn empty_array_literal_is_never_array_assignable_anywhere() {
    let src = "\
const a: number[] = [];
const b: string[][] = [];
";
    assert!(
        diags(src).is_empty(),
        "empty array literal must be assignable to any T[]"
    );
}

/// Element access `a[i]` yields the **element type** for **any** index (M17 does
/// not strict-check the index): `nums[0]` and `nums[i]` are both `number`, so a
/// `number` target is ok and a `string` target is `TK2322`. The index expression
/// is still walked (an unresolved index name would be `TK2304`).
#[test]
fn element_access_yields_element_type_for_any_index() {
    let src = "\
const nums: number[] = [1, 2, 3];
const n: number = nums[0];
const s: string = nums[0];
let i: number = 0;
const m: number = nums[i];
const bad: string = nums[i];
";
    // Line 3 (string target, number element) and line 6 (same with a var index).
    assert_eq!(
        diags(src),
        vec![(3, "TK2322".to_string()), (6, "TK2322".to_string())]
    );
}

/// `.length` is synthesized on an array → `number` (so `const len: number =
/// nums.length` is clean), while every other array member needs `lib.d.ts` and is
/// deferred → `TK2339` (`nums.push`, `nums.map`).
#[test]
fn length_is_synthesized_other_members_are_deferred() {
    let src = "\
const nums: number[] = [1, 2, 3];
const len: number = nums.length;
const p = nums.push;
const mapper = nums.map;
";
    // Lines 3 and 4 access deferred array members → TK2339; `length` (line 2) ok.
    assert_eq!(
        diags(src),
        vec![(3, "TK2339".to_string()), (4, "TK2339".to_string())]
    );
}

/// `Array<T>` is the **same** type as `T[]`: `Array<number>` accepts a numeric
/// array, `Array<string>` rejects `[1]` (→ `TK2322`), and an `Array<number>` value
/// is assignable to a `number[]` annotation (same interned id), both ways.
#[test]
fn array_generic_syntax_equals_postfix_syntax() {
    let src = "\
const arr: Array<number> = [1, 2];
const arrBad: Array<string> = [1];
const post: number[] = arr;
const generic: Array<number> = post;
";
    // Only line 2 errors: Array<string> vs (number)[]. The cross-syntax
    // assignments (lines 3, 4) are the same type → clean.
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// Array assignability is **covariant**: `string[]` is NOT assignable to
/// `number[]` (→ `TK2322`), nested `number[][]` accepts `[[1], [2]]` while
/// `[["x"]]` is rejected, and a same-element array assigns freely.
#[test]
fn covariant_assignability_and_nesting() {
    let src = "\
let a: number[];
let b: string[];
a = b;
const nested: number[][] = [[1], [2]];
const nestedBad: number[][] = [[\"x\"]];
let c: number[];
let d: number[];
c = d;
";
    // Line 3 (string[] → number[]) and line 5 (string[][] → number[][]).
    assert_eq!(
        diags(src),
        vec![(3, "TK2322".to_string()), (5, "TK2322".to_string())]
    );
}

/// A `T[]` element inside a **generic** instantiates correctly (substitution
/// rewrites the element): `interface Box<T> { items: T[] }` instantiated as
/// `Box<number>` has `items: number[]`, so a `number[]` is assignable to it and a
/// `string[]` is not (→ `TK2322`).
#[test]
fn generic_array_element_substitutes() {
    let src = "\
interface Box<T> { items: T[]; }
const ok: Box<number> = { items: [1, 2] };
const bad: Box<number> = { items: [\"x\"] };
";
    // Line 3: items is string[] vs number[] → TK2322.
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// A non-array base for element access is **out of M17 scope** (index signatures
/// are M18+): `x[0]` on a `number` emits **no** diagnostic and yields the error
/// type, so nothing downstream over-reports and the checker does not crash.
#[test]
fn non_array_element_access_is_out_of_scope_no_diagnostic() {
    let src = "\
const x: number = 5;
const y: number = x[0];
";
    assert!(
        diags(src).is_empty(),
        "element access on a non-array is out of M17 scope (no diagnostic, no crash)"
    );
}

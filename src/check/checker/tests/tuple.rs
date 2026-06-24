//! M18 end-to-end tests for **tuple types** (`[A, B]`, positional assignability,
//! length, indexed access, contextual typing, tuple → array). These drive the
//! whole pipeline (parse → bind → check) and assert the `(line, code)`
//! diagnostics, pinning the invariants the reviewer should scrutinize: the
//! **contextual** array-literal-as-tuple path does not regress the M17 array
//! inference (a non-tuple-context literal still infers an array), positional +
//! length assignability is sound, a tuple failure is a **single** diagnostic, and
//! tuple → array holds. The per-fixture acceptance lives in the conformance corpus
//! (`m18_tuples/`).

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

/// **Contextual typing** — an array literal in a tuple position is typed
/// positionally as a tuple: `[1, "x"]` against `[number, string]` is ok, while
/// `["x", 1]` produces **one** `TK2322` (position 0 mismatch). This is the key new
/// M18 mechanism; the literal element types are checked **position-by-position**,
/// not collapsed into an array union.
#[test]
fn contextual_array_literal_is_typed_as_tuple() {
    let src = "\
const t: [number, string] = [1, \"x\"];
const swapped: [number, string] = [\"x\", 1];
";
    // Line 1 ok; line 2 a single position-0 mismatch.
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A **literal-type** tuple target preserves the literal element types under
/// contextual typing (they are **not** widened): `[1, 2] = [1, 2]` is ok, while
/// `[1, 2] = [1, 3]` is `TK2322` (`3` is not assignable to the literal `2`). This
/// is consistent with the scalar literal path (`const x: 1 = 1`) and is the case
/// the widening bug regressed.
#[test]
fn contextual_literal_type_tuple_target() {
    let src = "\
const lit: [1, 2] = [1, 2];
const litBad: [1, 2] = [1, 3];
";
    // Line 1 ok (literal elements preserved); line 2 a position-1 literal mismatch.
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// A **wrong-length** array literal in a tuple position is a **length** mismatch
/// (one `TK2322`), not an array-vs-tuple confusion: `[1]` (too few) and
/// `[1, "x", 2]` (too many) each produce exactly one diagnostic.
#[test]
fn contextual_tuple_length_mismatch_is_single_diagnostic() {
    let src = "\
const ok: [number, string] = [1, \"x\"];
const short: [number, string] = [1];
const long: [number, string] = [1, \"x\", 2];
";
    assert_eq!(
        diags(src),
        vec![(2, "TK2322".to_string()), (3, "TK2322".to_string())]
    );
}

/// Tuple **element access** by a literal index yields that position's type:
/// `t[0]` is `number`, `t[1]` is `string`. A `string` target for `t[0]` is
/// `TK2322`; a `number` target for `t[0]` and a `string` target for `t[1]` are ok.
#[test]
fn tuple_element_access_by_literal_index() {
    let src = "\
const t: [number, string] = [1, \"x\"];
const a: number = t[0];
const b: string = t[1];
const c: string = t[0];
const d: number = t[1];
";
    // Line 4 (string target, number element 0) and line 5 (number target, string
    // element 1) mismatch; lines 2, 3 are ok.
    assert_eq!(
        diags(src),
        vec![(4, "TK2322".to_string()), (5, "TK2322".to_string())]
    );
}

/// An **out-of-range** literal index or a **non-literal** index on a tuple is out
/// of the M18 subset → the error type (no diagnostic, no crash), so nothing
/// downstream over-reports.
#[test]
fn tuple_out_of_range_or_dynamic_index_is_deferred() {
    let src = "\
const t: [number, string] = [1, \"x\"];
let i: number = 0;
const a: string = t[5];
const b: string = t[i];
";
    assert!(
        diags(src).is_empty(),
        "out-of-range / dynamic tuple index is deferred (no diagnostic, error type)"
    );
}

/// A tuple is assignable to the **array** of (a supertype of) its element types:
/// `[number, string]` → `(number | string)[]` is ok, while `[number, string]` →
/// `number[]` is `TK2322` (the `string` element does not fit).
#[test]
fn tuple_assignable_to_array_of_element_union() {
    let src = "\
const t: [number, string] = [1, \"x\"];
const arr: (number | string)[] = t;
const bad: number[] = t;
";
    // Line 2 ok (tuple → (number|string)[]); line 3 a TK2322 (string ⊄ number).
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

/// **No regression of M17**: an array literal with **no** tuple context still
/// infers an **array** (the union of widened element types), not a tuple. So
/// `const xs = [1, 2]` is `number[]` (assignable to a `number[]` annotation and
/// usable where an array is expected), and a heterogeneous `[1, "x"]` against a
/// `number[]` annotation is still the M17 `(number | string)[]` → `number[]`
/// `TK2322` — proving the contextual path did not hijack the array inference.
#[test]
fn no_tuple_context_array_literal_still_infers_array() {
    let src = "\
const xs = [1, 2];
const ys: number[] = xs;
const direct: number[] = [1, 2];
const bad: number[] = [1, \"x\"];
const tupleFromInferred: [number, number] = xs;
";
    // Line 4 is the M17 (number | string)[] → number[] mismatch. Line 5: `xs` was
    // inferred as `number[]` (NOT a tuple), so assigning it to a `[number, number]`
    // tuple is a TK2322 (array → tuple is not assignable) — this is exactly what
    // pins that a context-free literal is an array, not a tuple.
    assert_eq!(
        diags(src),
        vec![(4, "TK2322".to_string()), (5, "TK2322".to_string())]
    );
}

/// Nested tuples check positionally at depth: `[[number], string]` accepts
/// `[[1], "x"]` and rejects `[["x"], "x"]` (inner position-0 string ⊄ number),
/// a single `TK2322`. Confirms the contextual tuple typing and the positional
/// relation both recurse.
#[test]
fn nested_tuple_checks_positionally() {
    let src = "\
const ok: [[number], string] = [[1], \"x\"];
const bad: [[number], string] = [[\"x\"], \"x\"];
";
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// Tuple **order is significant**: a `[number, string]` value is NOT assignable to
/// a `[string, number]` annotation (one `TK2322`), unlike a union which would be
/// order-insensitive. Uses an identifier source (no contextual literal typing) so
/// the tuple→tuple relation itself is exercised.
#[test]
fn tuple_order_is_significant_on_assignment() {
    let src = "\
let a: [number, string];
let b: [string, number];
a = b;
let c: [number, string];
let d: [number, string];
c = d;
";
    // Line 3 ([string, number] → [number, string]) mismatches; line 6 (same type)
    // is clean.
    assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
}

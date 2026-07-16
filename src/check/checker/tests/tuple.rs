//! M18 end-to-end tests for tuple types.
//! Pins contextual tuple literals without regressing M17 arrays, positional and
//! length assignability, single tuple diagnostics, and tuple-to-array relation.
//! Fixture acceptance lives in `m18_tuples/`.

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

/// Contextual tuple literals are checked position-by-position, not collapsed
/// into an array union.
#[test]
fn contextual_array_literal_is_typed_as_tuple() {
    let src = "\
const t: [number, string] = [1, \"x\"];
const swapped: [number, string] = [\"x\", 1];
";
    // Line 1 ok; line 2 a single position-0 mismatch.
    assert_eq!(diags(src), vec![(2, "TK2322".to_string())]);
}

/// Literal-type tuple targets preserve literal elements under contextual typing;
/// this guards the old widening regression.
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

/// No tuple context still uses M17 array inference, not tuple inference.
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

/// Named tuple labels carry no type identity, including when the label wraps a
/// rest segment with a required suffix.
#[test]
fn named_tuple_labels_erase_and_preserve_rest_shape() {
    let src = "\
type NamedFixed = [text: string, count: number];
type PlainFixed = [string, number];
const namedFixed: NamedFixed = [\"ok\", 1];
const plainFixed: PlainFixed = [\"ok\", 1];
const namedAsPlain: PlainFixed = namedFixed;
const plainAsNamed: NamedFixed = plainFixed;
const namedBad: NamedFixed = [\"ok\", \"bad\"];
const plainBad: PlainFixed = [\"ok\", \"bad\"];
declare function namedRest(...args: [...strs: string[], n2: number]): void;
declare function plainRest(...args: [...string[], number]): void;
namedRest(1);
namedRest(\"a\", \"b\", 1);
plainRest(1);
plainRest(\"a\", \"b\", 1);
namedRest(\"a\", \"bad\");
plainRest(\"a\", \"bad\");
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);

    let index = crate::span::LineIndex::new(src);
    let actual: Vec<(u32, &str)> = out
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                index.line_of(diagnostic.span.start),
                diagnostic.code.as_str(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        vec![(7, "TK2322"), (8, "TK2322"), (15, "TK2345"), (16, "TK2345"),]
    );
    for diagnostic in &out.diagnostics[2..] {
        assert_eq!(
            diagnostic.message,
            "Argument of type 'string' is not assignable to parameter of type 'number'"
        );
    }
}

/// A named optional member stays unavailable under the existing optional tuple
/// owner; supporting labels must not claim optional members as complete.
#[test]
fn named_optional_tuple_member_keeps_optional_owner() {
    let src = "type NamedOptional = [value?: string];";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.incomplete.len(), 1, "{:?}", out.incomplete);
    let incomplete = &out.incomplete[0];
    assert_eq!(
        incomplete.id,
        "annotation-lower/tuple-optional-element/self"
    );
    assert_eq!(
        incomplete.context,
        "optional tuple element aborts tuple lowering"
    );
    assert_eq!(&src[incomplete.span.range()], "value?: string");
}

/// A rest segment is interned only when its lowered container is provably an
/// array or tuple; labels do not weaken that boundary.
#[test]
fn tuple_rest_requires_provably_array_like_container() {
    let src = "\
type PlainBad = [string, ...number];
type NamedBad = [first: string, ...rest: number];
type PlainArray = [string, ...number[]];
type NamedArray = [first: string, ...rest: number[]];
type PlainTuple = [string, ...[number, boolean]];
type NamedTuple = [first: string, ...rest: [number, boolean]];
type PlainGeneric<T extends unknown[]> = [string, ...T];
type NamedGeneric<T extends unknown[]> = [first: string, ...rest: T];
type UnknownGeneric<T> = [string, ...T];
type NonArrayGeneric<T extends number> = [string, ...T];
type InferRest<T> = T extends [unknown, ...infer R] ? R : never;
type UnprovenInferRest<T> = T extends infer R ? [...R] : never;
const plainArray: PlainArray = [\"ok\", 1, 2];
const namedArray: NamedArray = [\"ok\", 1, 2];
const plainTuple: PlainTuple = [\"ok\", 1, true];
const namedTuple: NamedTuple = [\"ok\", 1, true];
const plainGeneric: PlainGeneric<[number, boolean]> = [\"ok\", 1, true];
const namedGeneric: NamedGeneric<[number, boolean]> = [\"ok\", 1, true];
const inferredRest: InferRest<[string, number, boolean]> = [1, true];
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let index = crate::span::LineIndex::new(src);
    assert_eq!(
        out.incomplete
            .iter()
            .map(|incomplete| (
                index.line_of(incomplete.span.start),
                incomplete.id.as_str(),
                incomplete.context.as_str(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                1,
                "annotation-lower/tuple-rest-element/non-array",
                "tuple rest element is not provably array-like",
            ),
            (
                2,
                "annotation-lower/tuple-rest-element/non-array",
                "tuple rest element is not provably array-like",
            ),
            (
                9,
                "annotation-lower/tuple-rest-element/non-array",
                "tuple rest element is not provably array-like",
            ),
            (
                10,
                "annotation-lower/tuple-rest-element/non-array",
                "tuple rest element is not provably array-like",
            ),
            (
                12,
                "annotation-lower/tuple-rest-element/non-array",
                "tuple rest element is not provably array-like",
            ),
        ]
    );
}

/// An infer declaration in the conditional extends tuple remains array-like,
/// and its binder reference in the true branch retains the captured tuple.
#[test]
fn conditional_extends_infer_rest_keeps_true_branch_reference() {
    let src = "\
type Captured<T> = T extends [...infer R] ? R : never;
const captured: Captured<[string, number]> = [\"ok\", 1];
const capturedBad: Captured<[string, number]> = [\"ok\", \"bad\"];
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
    let index = crate::span::LineIndex::new(src);
    assert_eq!(
        out.diagnostics
            .iter()
            .map(|diagnostic| (
                index.line_of(diagnostic.span.start),
                diagnostic.code.as_str(),
            ))
            .collect::<Vec<_>>(),
        [(3, "TK2322")]
    );
}

/// Every infer constraint is withheld by the infer owner itself; a tuple-rest
/// wrapper must not add its older non-array record.
#[test]
fn constrained_infer_uses_central_owner_without_tuple_rest_duplicate() {
    let src = "\
type Direct<T> = T extends infer R extends string ? R : never;
type Rest<T> = T extends [...infer R extends readonly unknown[]] ? R : never;
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let index = crate::span::LineIndex::new(src);
    assert_eq!(
        out.incomplete
            .iter()
            .map(|incomplete| (index.line_of(incomplete.span.start), incomplete.id.as_str(),))
            .collect::<Vec<_>>(),
        [
            (1, "annotation-lower/infer-type/constraint"),
            (2, "annotation-lower/infer-type/constraint"),
        ]
    );
}

/// Parentheses do not hide a direct infer-rest declaration from the array-like
/// boundary, and the captured tuple remains precise in the true branch.
#[test]
fn parenthesized_conditional_infer_rest_keeps_true_branch_reference() {
    let src = "\
type Captured<T> = T extends [...(infer R)] ? R : never;
const captured: Captured<[string, number]> = [\"ok\", 1];
const capturedBad: Captured<[string, number]> = [\"ok\", \"bad\"];
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.incomplete.is_empty(), "{:?}", out.incomplete);
    let index = crate::span::LineIndex::new(src);
    assert_eq!(
        out.diagnostics
            .iter()
            .map(|diagnostic| (
                index.line_of(diagnostic.span.start),
                diagnostic.code.as_str(),
            ))
            .collect::<Vec<_>>(),
        [(3, "TK2322")]
    );
}

/// Fresh infer declarations are accepted only while lowering the innermost
/// conditional's extends type, never its true branch or a nested check type.
#[test]
fn conditional_infer_declaration_phase_withholds_true_and_nested_check() {
    let src = "\
type Misplaced<T> = T extends unknown ? [...infer R] : never;
type Nested<T> = T extends [...infer Outer]
  ? ([...infer Inner] extends unknown[] ? Outer : never)
  : never;
";
    let out = check_source(src);
    assert!(out.parse_errors.is_empty(), "{:?}", out.parse_errors);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let index = crate::span::LineIndex::new(src);
    assert_eq!(
        out.incomplete
            .iter()
            .map(|incomplete| (
                index.line_of(incomplete.span.start),
                incomplete.id.as_str(),
                &src[incomplete.span.range()],
            ))
            .collect::<Vec<_>>(),
        [
            (
                1,
                "annotation-lower/tuple-rest-element/non-array",
                "...infer R",
            ),
            (
                3,
                "annotation-lower/tuple-rest-element/non-array",
                "...infer Inner",
            ),
        ]
    );
}

//! M20 end-to-end tests for `keyof T` and indexed-access types.
//! Pins eager concrete-object evaluation, index-signature keys, and graceful
//! error-type fallback for missing/out-of-scope cases. Fixture acceptance lives
//! in `m20_keyof/`.

use crate::check::checker::expr::{
    combine_exact_symbol_member_lookups, exact_symbol_receiver_guard, object_element_access,
    ElementAccessLookup, ExactSymbolReceiverGuard,
};
use crate::check::checker::library_compiler::{
    check_caller_certified_collision_free_source_with_owned_library,
    compile_owned_injected_profile, InjectedLibrarySource,
};
use crate::check::test_support::check_source;
use crate::diagnostics::DiagnosticCode;
use crate::source::LibraryFileOrdinal;
use crate::types::repr::{LiteralValue, ObjectType, PropertyType, WellKnownSymbol};
use crate::types::Interner;

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

fn incompletes(source: &str) -> Vec<(u32, String)> {
    let out = check_source(source);
    assert!(
        out.parse_errors.is_empty(),
        "unexpected parse error(s): {:?}",
        out.parse_errors
    );
    let index = crate::span::LineIndex::new(source);
    out.incomplete
        .iter()
        .map(|record| (index.line_of(record.span.start), record.id.clone()))
        .collect()
}

#[test]
fn exact_symbol_object_lookup_distinguishes_missing_from_recovery_types() {
    let mut interner = Interner::with_intrinsics();
    let number = interner.well_known().number;
    let error = interner.well_known().error;
    let iterator_key =
        interner.intern_literal(LiteralValue::WellKnownSymbol(WellKnownSymbol::Iterator));
    let async_iterator_key = interner.intern_literal(LiteralValue::WellKnownSymbol(
        WellKnownSymbol::AsyncIterator,
    ));
    let present = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            number,
        )],
        ..ObjectType::default()
    });
    let recovery = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            error,
        )],
        ..ObjectType::default()
    });
    let missing = interner.intern_object(ObjectType::default());

    assert_eq!(
        object_element_access(interner.store(), present, Some(iterator_key)),
        ElementAccessLookup::Found(number)
    );
    assert_eq!(
        object_element_access(interner.store(), recovery, Some(iterator_key)),
        ElementAccessLookup::Found(error),
        "a present recovery-typed property is not a missing property"
    );
    assert_eq!(
        object_element_access(interner.store(), missing, Some(iterator_key)),
        ElementAccessLookup::MissingObjectKey
    );
    assert_eq!(
        object_element_access(interner.store(), present, Some(async_iterator_key)),
        ElementAccessLookup::MissingObjectKey
    );
}

#[test]
fn exact_symbol_union_lookup_requires_every_member_to_support_the_key() {
    let mut interner = Interner::with_intrinsics();
    let number = interner.well_known().number;
    let string = interner.well_known().string;
    let iterator_key =
        interner.intern_literal(LiteralValue::WellKnownSymbol(WellKnownSymbol::Iterator));
    let first = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            number,
        )],
        ..ObjectType::default()
    });
    let second = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            string,
        )],
        ..ObjectType::default()
    });
    let missing = interner.intern_object(ObjectType::default());

    let expected = interner.union(vec![number, string]);
    let found =
        combine_exact_symbol_member_lookups(&mut interner, &[first, second], Some(iterator_key));
    assert_eq!(found, ElementAccessLookup::Found(expected));
    assert_eq!(
        combine_exact_symbol_member_lookups(&mut interner, &[first, missing], Some(iterator_key),),
        ElementAccessLookup::MissingObjectKey
    );
}

#[test]
fn exact_symbol_lookup_rejects_non_object_receiver_routes() {
    let mut interner = Interner::with_intrinsics();
    let number = interner.well_known().number;
    let string = interner.well_known().string;
    let iterator_key =
        interner.intern_literal(LiteralValue::WellKnownSymbol(WellKnownSymbol::Iterator));
    let array = interner.intern_array(number);
    let tuple = interner.intern_tuple(vec![number, string]);

    for receiver in [string, array, tuple] {
        assert_eq!(
            object_element_access(interner.store(), receiver, Some(iterator_key)),
            ElementAccessLookup::UnsupportedReceiver
        );
    }
}

#[test]
fn exact_symbol_receiver_guard_preserves_owner_specific_and_recovery_routes() {
    let interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    assert_eq!(
        exact_symbol_receiver_guard(wk.unknown, &wk),
        ExactSymbolReceiverGuard::Unknown
    );
    for receiver in [wk.null, wk.undefined] {
        assert_eq!(
            exact_symbol_receiver_guard(receiver, &wk),
            ExactSymbolReceiverGuard::Nullish
        );
    }
    for receiver in [wk.any, wk.error, wk.never] {
        assert_eq!(
            exact_symbol_receiver_guard(receiver, &wk),
            ExactSymbolReceiverGuard::Recovery(wk.error)
        );
    }
    assert_eq!(
        exact_symbol_receiver_guard(wk.string, &wk),
        ExactSymbolReceiverGuard::Continue
    );
}

#[test]
fn certified_computed_and_wrapped_symbol_accesses_share_exact_authority() -> Result<(), String> {
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "computed-symbol-auth.d.ts",
        source: "declare const Symbol: {};",
    }])
    .map_err(|error| format!("{error:?}"))?;
    let run = check_caller_certified_collision_free_source_with_owned_library(
        state,
        r#"
            interface ExactSymbols {
                [Symbol.iterator](): number;
                [Symbol.asyncIterator](): string;
            }
            declare const value: ExactSymbols;
            const directWrong: string = value[Symbol.iterator]();
            const computedWrong: string = value[Symbol["iterator"]]();
            const keyWrappedWrong: string = value[Symbol[("iterator")]]();
            const objectWrappedWrong: string = value[(Symbol)["iterator"]]();
            const wholeWrappedWrong: string = value[(Symbol["iterator"])]();
            const asyncWrong: number = value[Symbol["asyncIterator"]]();
        "#,
    )?;
    assert!(
        run.result.incomplete.is_empty(),
        "{:?}",
        run.result.incomplete
    );
    assert_eq!(
        run.result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![DiagnosticCode::TK2322; 6]
    );
    Ok(())
}

#[test]
fn exact_symbol_nullable_union_routes_through_nullish_owner() {
    let mut interner = Interner::with_intrinsics();
    let number = interner.well_known().number;
    let null = interner.well_known().null;
    let undefined = interner.well_known().undefined;
    let iterator_key =
        interner.intern_literal(LiteralValue::WellKnownSymbol(WellKnownSymbol::Iterator));
    let supported = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            number,
        )],
        ..ObjectType::default()
    });

    for nullish in [null, undefined] {
        assert_eq!(
            combine_exact_symbol_member_lookups(
                &mut interner,
                &[supported, nullish],
                Some(iterator_key),
            ),
            ElementAccessLookup::NullishReceiver
        );
    }
}

#[test]
fn broad_symbol_element_access_fails_closed_once_for_objects_and_object_unions() {
    let object = "\
declare const key: symbol;
declare const value: { x: number };
value[key];
";
    assert_eq!(
        incompletes(object),
        vec![(3, "expr-infer/element-access/implicit-any-index".to_owned())]
    );

    let union = "\
declare const key: symbol;
declare const value: { x: number } | { y: string };
value[key];
";
    assert_eq!(
        incompletes(union),
        vec![(3, "expr-infer/element-access/implicit-any-index".to_owned())]
    );
}

#[test]
fn broad_symbol_element_access_fails_closed_for_mixed_unions_and_intersections() {
    let mixed_union = "\
declare const key: symbol;
declare const value: { item: number } | string;
value[key];
";
    assert_eq!(
        incompletes(mixed_union),
        vec![(3, "expr-infer/element-access/implicit-any-index".to_owned())]
    );

    let intersection = "\
declare const key: symbol;
declare const value: { item: number } & { tag: \"x\" };
value[key];
";
    assert_eq!(
        incompletes(intersection),
        vec![(3, "expr-infer/element-access/implicit-any-index".to_owned())]
    );
}

#[test]
fn broad_symbol_guard_does_not_capture_supported_or_recovery_routes() {
    let source = "\
declare const stringKey: string;
declare const numberKey: number;
declare const broadSymbol: symbol;
declare const stringIndex: { [key: string]: number };
declare const numberIndex: { [key: number]: string };
declare const anyBase: any;
stringIndex[stringKey];
numberIndex[numberKey];
anyBase[broadSymbol];
MissingBase[broadSymbol];
";
    assert!(
        incompletes(source).is_empty(),
        "string/number index and any/error recovery routes stay outside the guard"
    );
}

#[test]
fn broad_symbol_guard_covers_array_and_primitive_receivers() {
    let source = "\
declare const key: symbol;
declare const arrayBase: number[];
declare const numberBase: number;
arrayBase[key];
numberBase[key];
";
    assert_eq!(
        incompletes(source),
        vec![
            (4, "expr-infer/element-access/implicit-any-index".to_owned()),
            (5, "expr-infer/element-access/implicit-any-index".to_owned()),
        ]
    );
}

#[test]
fn broad_symbol_guard_routes_unknown_nullish_and_never_separately() {
    let source = "\
declare const key: symbol;
declare const unknownBase: unknown;
declare const neverBase: never;
declare const nullBase: null;
declare const undefinedBase: undefined;
unknownBase[key];
neverBase[key];
nullBase[key];
undefinedBase[key];
";
    assert_eq!(
        incompletes(source),
        vec![
            (6, "expr-infer/element-access/unknown-receiver".to_owned()),
            (8, "expr-infer/element-access/nullish-receiver".to_owned()),
            (9, "expr-infer/element-access/nullish-receiver".to_owned()),
        ],
        "unknown and nullish receivers fail closed once; never remains tsc-clean"
    );
}

#[test]
fn shadowed_symbol_member_expression_is_not_an_authenticated_symbol_key() {
    let source = "\
declare const Symbol: { iterator: \"local\" };
declare const value: { local: number };
const clean: number = value[Symbol.iterator];
const wrong: string = value[Symbol.iterator];
const computedClean: number = value[Symbol[\"iterator\"]];
const computedWrong: string = value[(Symbol)[(\"iterator\")]];
";
    assert!(incompletes(source).is_empty());
    assert_eq!(
        diags(source),
        vec![(4, "TK2322".to_owned()), (6, "TK2322".to_owned())]
    );
}

#[test]
fn inferred_string_literal_key_selects_the_exact_property() {
    let source = "\
declare const key: \"local\";
declare const value: { local: number };
const clean: number = value[key];
const wrong: string = value[key];
";
    assert!(incompletes(source).is_empty());
    assert_eq!(diags(source), vec![(4, "TK2322".to_owned())]);
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

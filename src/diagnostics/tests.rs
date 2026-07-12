use super::*;
use crate::relate::Reason;
use crate::span::Span;
use crate::types::repr::{
    FunctionType, GenericTypeParam, LiteralValue, ObjectType, ParameterType, PropertyType,
    TupleRestType, TupleType, TypeParamId,
};
use crate::types::store::TypeId;
use crate::types::Interner;

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

/// A single `Leaf` head — the M0–M5 scalar mismatch — renders **no**
/// elaboration: the headline already states it, so there is no redundant
/// "Types of …" wrapper and the rendered diagnostic stays exactly one line.
/// This is the no-regression guarantee for the earlier milestones.
#[test]
fn single_leaf_chain_has_no_elaboration() {
    let mut interner = Interner::with_intrinsics();
    let lit_str = interner.intern_literal(LiteralValue::String("x".to_string()));
    let wk = interner.well_known();
    let store = interner.store();

    let head = Reason::Leaf {
        src: lit_str,
        tgt: wk.number,
    };
    let lines = render_reason_chain(store, &head);
    assert!(
        lines.is_empty(),
        "a single-Leaf chain must produce no elaboration lines, got {lines:?}"
    );

    // And through a diagnostic: the rendered text is the one headline line,
    // unchanged from M0 (the leaf substring lives in the headline itself).
    let diag = Diagnostic::not_assignable(
        Span::new(0, 1),
        "Type 'string' is not assignable to type 'number'".to_string(),
    )
    .with_elaboration(lines);
    assert_eq!(
        diag.rendered_text(),
        "error[TK2322]: Type 'string' is not assignable to type 'number'"
    );
    assert!(!diag.rendered_text().contains('\n'));
}

/// A nested `Property → Property → Leaf` chain renders the tsc-style cascade,
/// indented one level per depth, with the scalar leaf line **present** and
/// last. This is the M6 nested-object (`TK2322`) shape from `nested.ts`.
#[test]
fn nested_property_chain_renders_leaf_last() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // { a: { b: string } } (src) vs { a: { b: number } } (tgt).
    let inner_str = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.string)],
        ..Default::default()
    });
    let inner_num = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number)],
        ..Default::default()
    });
    let outer_src = interner.intern_object(ObjectType {
        properties: vec![prop("a", inner_str)],
        ..Default::default()
    });
    let outer_tgt = interner.intern_object(ObjectType {
        properties: vec![prop("a", inner_num)],
        ..Default::default()
    });
    let store = interner.store();

    // The reason chain the relation engine builds for this failure.
    let head = Reason::Property {
        name: "a".to_string(),
        src: outer_src,
        tgt: outer_tgt,
        because: Box::new(Reason::Property {
            name: "b".to_string(),
            src: inner_str,
            tgt: inner_num,
            because: Box::new(Reason::Leaf {
                src: wk.string,
                tgt: wk.number,
            }),
        }),
    };

    let lines = render_reason_chain(store, &head);
    assert_eq!(
        lines,
        vec![
            "  Types of property 'a' are incompatible.".to_string(),
            "    Types of property 'b' are incompatible.".to_string(),
            "      Type 'string' is not assignable to type 'number'.".to_string(),
        ]
    );

    // The leaf substring asserted by the corpus must be in the fully rendered
    // diagnostic text (headline + elaboration).
    let rendered = Diagnostic::not_assignable(
        Span::new(0, 1),
        "Type '{ a: { b: string } }' is not assignable to type '{ a: { b: number } }'".to_string(),
    )
    .with_elaboration(lines)
    .rendered_text();
    assert!(rendered.contains("Type 'string' is not assignable to type 'number'"));
}

/// A `MissingProperty` head is terminal — the headline (`TK2741`) states the
/// missing property in full — so it renders no elaboration.
#[test]
fn missing_property_head_has_no_elaboration() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let a_only = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    let ab = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    let store = interner.store();

    let head = Reason::MissingProperty {
        name: "b".to_string(),
        src: a_only,
        tgt: ab,
    };
    assert!(render_reason_chain(store, &head).is_empty());
}

/// A function `Parameter → Leaf` chain names the parameter (from the source
/// signature) and nests its leaf cause underneath.
#[test]
fn parameter_chain_names_parameter_and_nests_leaf() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let str_to_num = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        params: vec![ParameterType::required("x", wk.string)],
        ret: wk.number,
    });
    let num_to_num = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        params: vec![ParameterType::required("x", wk.number)],
        ret: wk.number,
    });
    let store = interner.store();

    // Contravariant parameter failure: tgt param `number` not assignable to
    // src param `string` (the engine builds the leaf in the tgt→src order).
    let head = Reason::Parameter {
        index: 0,
        src: str_to_num,
        tgt: num_to_num,
        because: Box::new(Reason::Leaf {
            src: wk.number,
            tgt: wk.string,
        }),
    };
    let lines = render_reason_chain(store, &head);
    assert_eq!(
        lines,
        vec![
            "  Types of parameters 'x' are incompatible.".to_string(),
            "    Type 'number' is not assignable to type 'string'.".to_string(),
        ]
    );
}

/// A union-source head descends straight into the offending member's reason
/// (the headline already names that member via the checker's `headline_src`).
/// When the member's cause is a scalar leaf, the elaboration is empty.
#[test]
fn union_source_head_descends_into_member_cause() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let union = interner.union(vec![wk.string, wk.number]);
    let store = interner.store();

    // `string | number` not assignable to `number`: the `string` member fails
    // with a scalar leaf. Headline reports `string`; elaboration is empty.
    let head = Reason::UnionSourceMember {
        member: wk.string,
        src: union,
        tgt: wk.number,
        because: Box::new(Reason::Leaf {
            src: wk.string,
            tgt: wk.number,
        }),
    };
    assert!(render_reason_chain(store, &head).is_empty());

    // But a union member with a *nested* cause renders that cause.
    let inner_str = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.string)],
        ..Default::default()
    });
    let inner_num = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number)],
        ..Default::default()
    });
    let store = interner.store();
    let head = Reason::UnionSourceMember {
        member: inner_str,
        src: union,
        tgt: inner_num,
        because: Box::new(Reason::Property {
            name: "b".to_string(),
            src: inner_str,
            tgt: inner_num,
            because: Box::new(Reason::Leaf {
                src: wk.string,
                tgt: wk.number,
            }),
        }),
    };
    let lines = render_reason_chain(store, &head);
    assert_eq!(
        lines,
        vec![
            "  Types of property 'b' are incompatible.".to_string(),
            "    Type 'string' is not assignable to type 'number'.".to_string(),
        ]
    );
}

/// M17 array rendering: `number[]`, the nested `number[][]`, and the
/// parenthesized `(number | string)[]` / `((x: number) => string)[]` element
/// forms (a union/function element binds ambiguously under bare `[]`).
#[test]
fn array_type_renders_with_parenthesized_element() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let num_arr = interner.intern_array(wk.number);
    assert_eq!(render_type(interner.store(), num_arr, false), "number[]");

    // Nested: no parentheses for an array element.
    let num_arr_arr = interner.intern_array(num_arr);
    assert_eq!(
        render_type(interner.store(), num_arr_arr, false),
        "number[][]"
    );

    // A union element IS parenthesized.
    let union = interner.union(vec![wk.number, wk.string]);
    let union_arr = interner.intern_array(union);
    let rendered = render_type(interner.store(), union_arr, false);
    assert!(
        rendered == "(number | string)[]" || rendered == "(string | number)[]",
        "a union element must be parenthesized, got {rendered:?}"
    );

    // A function element IS parenthesized.
    let func = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        params: vec![ParameterType::required("x", wk.number)],
        ret: wk.string,
    });
    let func_arr = interner.intern_array(func);
    assert_eq!(
        render_type(interner.store(), func_arr, false),
        "((x: number) => string)[]"
    );
}

/// M32/WU2 signature-shape rendering: required-only rendering stays unchanged,
/// while optional/rest parameters have a stable TS-like display.
#[test]
fn function_signature_shape_renders_optional_and_rest_params() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let string_array = interner.intern_array(wk.string);
    let func = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        params: vec![
            ParameterType::required("x", wk.number),
            ParameterType::optional("y", wk.boolean),
            ParameterType::rest("args", string_array),
        ],
        ret: wk.void,
    });

    assert_eq!(
        render_type(interner.store(), func, false),
        "(x: number, y?: boolean, ...args: string[]) => void"
    );
}

#[test]
fn generic_function_signature_renders_persistent_binders() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(90), "T");
    let func = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: TypeParamId(90),
            constraint: Some(wk.number),
            default: Some(wk.string),
        }],
        params: vec![ParameterType::required("value", t)],
        ret: t,
    });

    assert_eq!(
        render_type(interner.store(), func, false),
        "<T extends number = string>(value: T) => T"
    );
}

#[test]
fn arity_diagnostics_render_range_and_at_least_messages() {
    assert_eq!(
        Diagnostic::wrong_argument_count_range(Span::new(0, 1), 1, 2, 0).rendered_text(),
        "error[TK2554]: Expected 1-2 arguments, but got 0"
    );
    assert_eq!(
        Diagnostic::wrong_min_argument_count(Span::new(0, 1), 1, 0).rendered_text(),
        "error[TK2555]: Expected at least 1 arguments, but got 0"
    );
}

/// M31 intersection rendering: `A & B` joins members with ` & ` (canonical,
/// TypeId-sorted order — asserted order-independently, like unions), a **union**
/// member is parenthesized (`(A | B) & C`), and an intersection element inside an
/// array is parenthesized (`(A & B)[]`).
#[test]
fn intersection_type_renders_with_ampersand_and_parens() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `string & number` — a bare two-member node (disjoint primitives are not
    // reduced). Rendered with ` & `, in TypeId order (unstable, so accept either).
    let sn = interner.intersection(vec![wk.string, wk.number]);
    let rendered = render_type(interner.store(), sn, false);
    assert!(
        rendered == "string & number" || rendered == "number & string",
        "an intersection joins members with ` & `, got {rendered:?}"
    );

    // A **union** member is parenthesized inside an intersection: `(number | string) & boolean`.
    let union = interner.union(vec![wk.number, wk.string]);
    let with_union = interner.intersection(vec![union, wk.boolean]);
    let rendered = render_type(interner.store(), with_union, false);
    assert!(
        rendered.contains("(number | string)") || rendered.contains("(string | number)"),
        "a union member must be parenthesized inside an intersection, got {rendered:?}"
    );
    assert!(
        rendered.contains(" & "),
        "still ` & `-joined, got {rendered:?}"
    );

    // An intersection element inside an array is parenthesized: `(string & number)[]`.
    let sn_arr = interner.intern_array(sn);
    let rendered = render_type(interner.store(), sn_arr, false);
    assert!(
        rendered == "(string & number)[]" || rendered == "(number & string)[]",
        "an intersection array element must be parenthesized, got {rendered:?}"
    );
}

/// M18 tuple rendering: `[number, string]` (order preserved, `, `-separated,
/// square-bracketed) and the empty tuple `[]`.
#[test]
fn tuple_type_renders_in_brackets() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let num_str = interner.intern_tuple(vec![wk.number, wk.string]);
    assert_eq!(
        render_type(interner.store(), num_str, false),
        "[number, string]"
    );

    // Order is preserved (not sorted): [string, number] renders in that order.
    let str_num = interner.intern_tuple(vec![wk.string, wk.number]);
    assert_eq!(
        render_type(interner.store(), str_num, false),
        "[string, number]"
    );

    // The empty tuple renders as `[]`.
    let empty = interner.intern_tuple(vec![]);
    assert_eq!(render_type(interner.store(), empty, false), "[]");

    // A nested tuple element renders inline (the outer brackets delimit it).
    let nested = interner.intern_tuple(vec![num_str, wk.boolean]);
    assert_eq!(
        render_type(interner.store(), nested, false),
        "[[number, string], boolean]"
    );

    let string_array = interner.intern_array(wk.string);
    let rest_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.number, wk.boolean],
        TupleRestType::new(1, string_array),
    ));
    assert_eq!(
        render_type(interner.store(), rest_tuple, false),
        "[number, ...string[], boolean]"
    );
}

/// M18 — a `TupleElement` head renders the offending element's nested cause (the
/// headline already states the `[…]`/`[…]` mismatch). A scalar leaf element yields
/// exactly one nested line; a `TupleLength` head (terminal) renders **none**.
#[test]
fn tuple_reason_heads_render_element_cause_and_length_terminal() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let str_num = interner.intern_tuple(vec![wk.string, wk.number]);
    let num_str = interner.intern_tuple(vec![wk.number, wk.string]);
    let num_only = interner.intern_tuple(vec![wk.number]);
    let store = interner.store();

    // [string, number] not assignable to [number, string]: position 0 fails
    // (string→number) — one nested line.
    let head = Reason::TupleElement {
        index: 0,
        src: str_num,
        tgt: num_str,
        because: Box::new(Reason::Leaf {
            src: wk.string,
            tgt: wk.number,
        }),
    };
    assert_eq!(
        render_reason_chain(store, &head),
        vec!["  Type 'string' is not assignable to type 'number'.".to_string()]
    );

    // A length mismatch is terminal — the headline states the two tuple types in
    // full, so no elaboration.
    let len_head = Reason::TupleLength {
        src: num_only,
        tgt: num_str,
    };
    assert!(render_reason_chain(store, &len_head).is_empty());
}

/// M17 — an `ArrayElement` head renders the element's nested cause (the headline
/// already states the `S[]`/`T[]` mismatch). A scalar leaf element yields exactly
/// one nested line.
#[test]
fn array_element_head_renders_element_cause() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let str_arr = interner.intern_array(wk.string);
    let num_arr = interner.intern_array(wk.number);
    let store = interner.store();

    // string[] not assignable to number[]: the element `string`→`number` fails.
    let head = Reason::ArrayElement {
        src: str_arr,
        tgt: num_arr,
        because: Box::new(Reason::Leaf {
            src: wk.string,
            tgt: wk.number,
        }),
    };
    let lines = render_reason_chain(store, &head);
    assert_eq!(
        lines,
        vec!["  Type 'string' is not assignable to type 'number'.".to_string()]
    );
}

/// M17 — a **union-element** array mismatch (`(number | string)[]` not assignable
/// to `number[]`) renders a **single** nested line (the offending member), not the
/// doubled "member line + identical leaf" the shared `reason_lines` union arm would
/// produce when nested. This pins the `element_reason_lines` collapse.
#[test]
fn array_union_element_renders_single_line() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let union = interner.union(vec![wk.number, wk.string]);
    let union_arr = interner.intern_array(union);
    let num_arr = interner.intern_array(wk.number);
    let store = interner.store();

    // `(number | string)[]` not assignable to `number[]`: the element's `string`
    // member fails (UnionSourceMember → Leaf).
    let head = Reason::ArrayElement {
        src: union_arr,
        tgt: num_arr,
        because: Box::new(Reason::UnionSourceMember {
            member: wk.string,
            src: union,
            tgt: wk.number,
            because: Box::new(Reason::Leaf {
                src: wk.string,
                tgt: wk.number,
            }),
        }),
    };
    let lines = render_reason_chain(store, &head);
    assert_eq!(
        lines,
        vec!["  Type 'string' is not assignable to type 'number'.".to_string()],
        "a union-element array mismatch must render a single nested line, not a doubled one"
    );
}

// --- Renderer column pinning (sprint WU5, finding 5) -------------------------

/// Render diagnostics to a String in the requested format via the public entry
/// point (the same path the CLI uses).
fn render(source: &str, diags: &[Diagnostic], format: DiagnosticFormat) -> String {
    let mut buf: Vec<u8> = Vec::new();
    render_to_writer_with_format(&mut buf, "test.ts", source, diags, format).unwrap();
    String::from_utf8(buf).unwrap()
}

fn diag_at(span: Span) -> Diagnostic {
    Diagnostic::not_assignable(
        span,
        "Type 'string' is not assignable to type 'number'".into(),
    )
}

/// Compact output pins the exact `(line, column)` — and two diagnostics that share
/// a line are told apart by their (byte-based) start columns.
#[test]
fn compact_distinguishes_same_line_by_column() {
    let src = "0123456789\n0123456789\n";
    let out = render(
        src,
        &[diag_at(Span::new(3, 5)), diag_at(Span::new(6, 7))],
        DiagnosticFormat::Compact,
    );
    assert!(out.contains("test.ts(1,4):"), "first diag at col 4:\n{out}");
    assert!(
        out.contains("test.ts(1,7):"),
        "second diag same line, col 7:\n{out}"
    );
}

/// Compact columns are byte offsets: a leading tab counts as one column.
#[test]
fn compact_tab_is_one_column() {
    let out = render(
        "\t\tx = 1;",
        &[diag_at(Span::new(2, 3))],
        DiagnosticFormat::Compact,
    );
    assert!(
        out.contains("test.ts(1,3):"),
        "'x' after two tabs at col 3:\n{out}"
    );
}

/// Compact columns count multibyte and wide characters in bytes.
#[test]
fn compact_utf8_columns_are_byte_based() {
    // 'é' = 2 bytes -> '=' at byte 2 -> column 3.
    let out = render(
        "é=x;",
        &[diag_at(Span::new(2, 3))],
        DiagnosticFormat::Compact,
    );
    assert!(
        out.contains("test.ts(1,3):"),
        "'=' after 2-byte 'é' at col 3:\n{out}"
    );
    // '🎉' = 4 bytes -> 'x' at byte 4 -> column 5.
    let out = render(
        "🎉x;",
        &[diag_at(Span::new(4, 5))],
        DiagnosticFormat::Compact,
    );
    assert!(
        out.contains("test.ts(1,5):"),
        "'x' after 4-byte emoji at col 5:\n{out}"
    );
}

/// Compact reports a diagnostic that starts on a later line at that line.
#[test]
fn compact_multiline_reports_correct_line() {
    let src = "aaa\nbbbb\nccc";
    let out = render(src, &[diag_at(Span::new(6, 8))], DiagnosticFormat::Compact);
    assert!(
        out.contains("test.ts(2,3):"),
        "byte 6 is line 2 col 3:\n{out}"
    );
}

/// An EOF span (start == source length) renders one-past-the-end without panicking.
#[test]
fn compact_eof_span() {
    let src = "abc";
    let out = render(src, &[diag_at(Span::new(3, 3))], DiagnosticFormat::Compact);
    assert!(out.contains("test.ts(1,4):"), "EOF span at col 4:\n{out}");
}

/// Rich output puts the exact `:line:column` in its location header, so two
/// diagnostics on one line are distinguishable, and the caret underline width
/// pins the span's END column (end − start bytes).
#[test]
fn rich_header_columns_and_caret_width() {
    let src = "const x: number = \"hi\";\nconst y: number = \"yo\";";
    let out = render(
        src,
        &[diag_at(Span::new(18, 22)), diag_at(Span::new(42, 46))],
        DiagnosticFormat::Rich,
    );
    assert!(
        out.contains("test.ts:1:19"),
        "first diag start col 19:\n{out}"
    );
    assert!(
        out.contains("test.ts:2:19"),
        "second diag on line 2:\n{out}"
    );
    // A width-4 span underlines with exactly four carets.
    assert!(
        out.contains("^^^^"),
        "caret width pins the 4-byte span end:\n{out}"
    );
    assert!(
        !out.contains("^^^^^"),
        "caret must not exceed the span width:\n{out}"
    );
}

/// Rich rendering handles multiline and EOF spans without panicking.
#[test]
fn rich_multiline_and_eof_spans_render() {
    let src = "aaa\nbbb\nccc";
    let out = render(src, &[diag_at(Span::new(2, 6))], DiagnosticFormat::Rich);
    assert!(
        out.contains("test.ts:1:3"),
        "multiline span starts line 1 col 3:\n{out}"
    );
    let eof = render(src, &[diag_at(Span::new(11, 11))], DiagnosticFormat::Rich);
    assert!(
        eof.contains("test.ts:3:4"),
        "EOF span at line 3 col 4:\n{eof}"
    );
}

/// M17 — a **nested-array** mismatch (`string[][]` not assignable to `number[][]`)
/// nests each array level: the inner `string[]`/`number[]` line, then the scalar
/// leaf, each one indent deeper. Pins that `ArrayElement` is NOT a pure
/// head-collapse (it prints intermediate array lines).
#[test]
fn array_nested_element_nests_each_level() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let str_arr = interner.intern_array(wk.string);
    let num_arr = interner.intern_array(wk.number);
    let str_arr_arr = interner.intern_array(str_arr);
    let num_arr_arr = interner.intern_array(num_arr);
    let store = interner.store();

    let head = Reason::ArrayElement {
        src: str_arr_arr,
        tgt: num_arr_arr,
        because: Box::new(Reason::ArrayElement {
            src: str_arr,
            tgt: num_arr,
            because: Box::new(Reason::Leaf {
                src: wk.string,
                tgt: wk.number,
            }),
        }),
    };
    let lines = render_reason_chain(store, &head);
    assert_eq!(
        lines,
        vec![
            "  Type 'string[]' is not assignable to type 'number[]'.".to_string(),
            "    Type 'string' is not assignable to type 'number'.".to_string(),
        ]
    );
}

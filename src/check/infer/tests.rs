use super::*;
use crate::types::repr::{
    FunctionType, LiteralValue, ObjectType, ParameterType, PropertyType, TupleRestType, TupleType,
    TypeParamId,
};

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

/// A bare scalar argument matched against a type parameter fixes that parameter
/// to the (widened) argument type: `identity(5)` infers `T = number`.
#[test]
fn infers_from_scalar_argument() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    // The argument `5` is a literal type.
    let five = interner.intern_literal(LiteralValue::Number(5.0));
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t],
        &[five],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "T inferred from `5` widens to number"
    );
}

/// M25 — the **conditional collection mode**: literal candidates are NOT widened
/// (`"x"` stays `"x"`; call-site mode widens — pinned by
/// [`infers_from_scalar_argument`] above), and a **union target** descends into its
/// members (`number[]` against `string | T[]` infers `T = number`).
#[test]
fn conditional_mode_keeps_literals_and_descends_union_targets() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let x = interner.intern_literal(LiteralValue::String("x".to_string()));

    // No widening: `"x"` against `T` records the literal itself.
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, x, t, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[x][..]),
        "conditional mode must record the un-widened literal"
    );

    // Union-target descent: `number[]` against `string | T[]` lands `T = number`
    // via the array member (the string member contributes nothing).
    let t_arr = interner.intern_array(t);
    let union_target = interner.union(vec![wk.string, t_arr]);
    let num_arr = interner.intern_array(wk.number);
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, num_arr, union_target, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.number][..]),
        "a union extends target must collect from its shape-matching member"
    );

    // The CALL-site collection mode on the same union shape stays M10-conservative
    // (non-union source vs union target infers nothing — no m10 behavior change).
    let mut candidates = Candidates::default();
    infer_from_types_raw(&mut interner, num_arr, union_target, &mut candidates);
    assert!(
        candidates.is_empty(),
        "call-site mode must not descend into union targets (M10 unchanged)"
    );
}

/// M25 round-4 — a **naked** infer union member's whole-check candidate is LOW
/// priority: it is DISCARDED when a structural member of the same union bound the
/// same binder (`{ v: T } | T` against `{ v: string }` → `T = string`, not
/// `string | { v: string }`), and KEPT when no structural member did
/// (`string | T` against `number` → `T = number`). A different-name naked member
/// never blocks a structural binder (`A | B[]`).
#[test]
fn naked_union_member_candidate_yields_to_structural() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");

    // `{ v: T } | T` against `{ v: string }` → structural wins: T = string only.
    let v_t = interner.intern_object(ObjectType {
        properties: vec![prop("v", t)],
        ..Default::default()
    });
    let target = interner.union(vec![v_t, t]);
    let v_str = interner.intern_object(ObjectType {
        properties: vec![prop("v", wk.string)],
        ..Default::default()
    });
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, v_str, target, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.string][..]),
        "the naked member's whole-check candidate must be dropped"
    );

    // `string | T` against `number` → naked-only: the whole check IS the candidate.
    let target = interner.union(vec![wk.string, t]);
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, wk.number, target, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.number][..]),
        "a naked-only member keeps its whole-check candidate"
    );

    // `A | B[]` against `number[]`: B binds structurally; the naked A (a DIFFERENT
    // binder) still records the whole check — no cross-binder blocking.
    let a = interner.intern_type_param(TypeParamId(1), "A");
    let b = interner.intern_type_param(TypeParamId(2), "B");
    let b_arr = interner.intern_array(b);
    let target = interner.union(vec![a, b_arr]);
    let num_arr = interner.intern_array(wk.number);
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, num_arr, target, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(2)).map(|c| c.as_slice()),
        Some(&[wk.number][..]),
        "B = number from the structural member"
    );
    assert_eq!(
        candidates.get(&TypeParamId(1)).map(|c| c.as_slice()),
        Some(&[num_arr][..]),
        "A (different name) keeps its naked whole-check candidate"
    );
}

/// A candidate matched against a non-generic parameter is not recorded, but the
/// return-bearing parameter still infers: `pick(1, \"x\")` infers `A = number`,
/// `B = string` (each from its own parameter).
#[test]
fn infers_each_parameter_independently() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let a = interner.intern_type_param(TypeParamId(0), "A");
    let b = interner.intern_type_param(TypeParamId(1), "B");
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let x = interner.intern_literal(LiteralValue::String("x".to_string()));
    let mut next_type_param = 2;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0), TypeParamId(1)],
        &[a, b],
        &[one, x],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "A = number"
    );
    assert_eq!(
        map.get(&TypeParamId(1)).copied(),
        Some(wk.string),
        "B = string"
    );
}

/// A type parameter nested inside an object parameter is inferred from the
/// matching property of the argument object: `unwrap({ value: 1 })` with the
/// parameter `{ value: T }` infers `T = number`. (Object-literal members arrive
/// already widened, so the candidate is `number` here.)
#[test]
fn infers_from_object_property() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    // Parameter `{ value: T }`.
    let box_t = interner.intern_object(ObjectType {
        properties: vec![prop("value", t)],
        ..Default::default()
    });
    // Argument `{ value: number }` (member already widened by the checker).
    let arg = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[box_t],
        &[arg],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "T = number"
    );
}

/// A type parameter under a function parameter is inferred from both the
/// parameter positions and the return type.
#[test]
fn infers_through_function_parameter() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    // Parameter type `(x: T) => T`.
    let target = interner.intern_function(FunctionType {
        params: vec![ParameterType::required("x", t)],
        ret: t,
    });
    // Argument type `(x: number) => number`.
    let source = interner.intern_function(FunctionType {
        params: vec![ParameterType::required("x", wk.number)],
        ret: wk.number,
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[target],
        &[source],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "T = number"
    );
}

/// Two distinct candidates for one type parameter fix to their **union**:
/// `both(1, \"s\")` (both parameters typed `T`) infers `T = number | string`.
#[test]
fn multiple_distinct_candidates_union() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let s = interner.intern_literal(LiteralValue::String("s".to_string()));
    let expected = interner.union(vec![wk.number, wk.string]);
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[one, s],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(expected),
        "T = number | string"
    );
}

/// Two **equal** candidates collapse to that one type (not a 2-member union):
/// `both(1, 2)` infers `T = number`.
#[test]
fn duplicate_candidates_collapse() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[one, two],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "T = number (duplicates collapse)"
    );
}

/// A type parameter with **no** candidate falls back to `unknown` (the sound
/// fallback), never `any`. Here the argument shape (a scalar) does not match the
/// parameter shape (an object), so nothing is inferred for `T`.
#[test]
fn no_candidate_falls_back_to_unknown() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    // Parameter `{ value: T }`; argument is a bare `number` (shape mismatch).
    let box_t = interner.intern_object(ObjectType {
        properties: vec![prop("value", t)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[box_t],
        &[wk.number],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.unknown),
        "no candidate → unknown, never any"
    );
    assert_ne!(map.get(&TypeParamId(0)).copied(), Some(wk.any));
}

/// M24/M27 — a parameter with a **primitive constraint** keeps the inferred literal
/// (tsc `hasPrimitiveConstraint`): `mk<T extends string>("x")` infers `T = "x"`, while
/// an unconstrained `id<U>("x")` widens to `string`.
#[test]
fn primitive_constraint_preserves_literal() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let x = interner.intern_literal(LiteralValue::String("x".to_string()));

    // `<T extends string>`: literal preserved.
    let t = interner.intern_type_param(TypeParamId(0), "T");
    interner.set_type_param_constraint(TypeParamId(0), wk.string);
    let mut next_type_param = 1;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t],
        &[x],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(x),
        "a string-constrained parameter keeps the literal `\"x\"`"
    );

    // `<U>` (unconstrained): widened.
    let u = interner.intern_type_param(TypeParamId(1), "U");
    let mut next_type_param = 2;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(1)],
        &[u],
        &[x],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(1)).copied(),
        Some(wk.string),
        "an unconstrained parameter widens `\"x\"` → string"
    );
}

/// M27 — template-pattern `infer` capture: matching a string literal against a
/// template extends target captures each `infer` hole (a freshened type parameter) as
/// a NON-widened string-literal candidate, non-greedily on the first separator; a
/// failed anchor records nothing.
#[test]
fn template_infer_captures_segments() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();

    // Extends pattern `` `${L}:${R}` `` with L, R freshened infer parameters.
    let l = interner.intern_type_param(TypeParamId(0), "L");
    let r = interner.intern_type_param(TypeParamId(1), "R");
    let pattern = interner.intern_template(TemplateType {
        texts: vec![String::new(), ":".to_string(), String::new()],
        holes: vec![l, r],
    });

    // "a:b:c" — first `:` anchors (non-greedy): L = "a", R = "b:c".
    let check = interner.intern_literal(LiteralValue::String("a:b:c".to_string()));
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, check, pattern, &mut candidates);
    let a = interner.intern_literal(LiteralValue::String("a".to_string()));
    let bc = interner.intern_literal(LiteralValue::String("b:c".to_string()));
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[a][..]),
        "L = \"a\""
    );
    assert_eq!(
        candidates.get(&TypeParamId(1)).map(|c| c.as_slice()),
        Some(&[bc][..]),
        "R = \"b:c\""
    );

    // A source with no `:` separator records nothing (no match → false branch).
    let no_sep = interner.intern_literal(LiteralValue::String("abc".to_string()));
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, no_sep, pattern, &mut candidates);
    assert!(
        candidates.is_empty(),
        "a non-matching source records no candidate"
    );
}

#[test]
fn conditional_tuple_rest_infer_captures_middle_as_tuple() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let r = interner.intern_type_param(TypeParamId(0), "R");
    let source = interner.intern_tuple(vec![wk.string, wk.number, wk.boolean]);
    let target = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.unknown],
        TupleRestType::new(1, r),
    ));
    let expected = interner.intern_tuple(vec![wk.number, wk.boolean]);

    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);

    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[expected][..]),
        "`[unknown, ...infer R]` captures the remaining tuple segment"
    );
}

#[test]
fn conditional_function_rest_infer_captures_parameter_tuple() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let a = interner.intern_type_param(TypeParamId(0), "A");
    let source = interner.intern_function(FunctionType {
        params: vec![
            ParameterType::required("x", wk.string),
            ParameterType::required("y", wk.number),
        ],
        ret: wk.boolean,
    });
    let target = interner.intern_function(FunctionType {
        params: vec![ParameterType::rest("args", a)],
        ret: wk.unknown,
    });
    let expected = interner.intern_tuple(vec![wk.string, wk.number]);

    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);

    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[expected][..]),
        "`(...args: infer A)` captures fixed parameters as a tuple"
    );
}

#[test]
fn call_site_rest_array_infers_from_each_variadic_argument() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let t_array = interner.intern_array(t);
    let expected = interner.union(vec![wk.number, wk.string]);
    let mut next_type_param = 1;

    let map = infer_type_arguments_from_params(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[ParameterType::rest("args", t_array)],
        &[wk.number, wk.string],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(expected),
        "T[] rest parameters infer T from every rest argument"
    );
}

/// A self-referential argument/parameter pair terminates (cycle guard): a
/// recursive nominal object matched against itself does not loop.
#[test]
fn self_referential_types_terminate() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    // A recursive nominal `List { head: number; tail: List | null }`.
    let list = interner.reserve_object();
    let list_or_null = interner.union(vec![list, wk.null]);
    interner.fill_object(
        list,
        ObjectType {
            properties: vec![prop("head", wk.number), prop("tail", list_or_null)],
            ..Default::default()
        },
    );

    // Matching `list` against itself must terminate; it has no type parameter,
    // so it infers nothing.
    let mut next_type_param = 0;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[],
        &[list],
        &[list],
        &[],
    );
    assert!(
        map.is_empty(),
        "no type params → empty map, and no infinite loop"
    );
}

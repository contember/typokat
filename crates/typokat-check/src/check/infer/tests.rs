use super::*;
use crate::types::repr::{
    FunctionType, LiteralValue, ObjectType, ParameterType, PropertyType, TupleRestType, TupleType,
    TypeParamId, WellKnownSymbol,
};
use std::time::Instant;

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn infer_type_arguments(
    interner: &mut Interner,
    next_type_param: &mut u32,
    type_params: &[TypeParamId],
    params: &[TypeId],
    args: &[TypeId],
    fresh_args: &[bool],
) -> FxHashMap<TypeParamId, TypeId> {
    let params: Vec<_> = params
        .iter()
        .enumerate()
        .map(|(index, &ty)| ParameterType::required(format!("p{index}"), ty))
        .collect();
    infer_type_arguments_from_params(
        interner,
        next_type_param,
        type_params,
        &params,
        args,
        fresh_args,
    )
}

fn infer_type_arguments_from_params(
    interner: &mut Interner,
    next_type_param: &mut u32,
    type_params: &[TypeParamId],
    params: &[ParameterType],
    args: &[TypeId],
    fresh_args: &[bool],
) -> FxHashMap<TypeParamId, TypeId> {
    let type_params: Vec<_> = type_params
        .iter()
        .map(|&id| GenericTypeParam {
            id,
            constraint: interner.store().type_param_constraint(id),
            default: None,
        })
        .collect();
    let published = PublishedClasses::empty();
    let mut queries = SemanticQueryState::default();
    match infer_signature_type_arguments_from_params(
        interner,
        next_type_param,
        &published,
        &mut queries,
        SignatureInferenceRequest {
            type_params: &type_params,
            params,
            args,
            fresh_args,
            receiver: None,
        },
    ) {
        DemandOutcome::Ready(result) => result.arguments,
        DemandOutcome::Exhausted(exhaustion) => {
            panic!("intrinsic-only inference unexpectedly exhausted: {exhaustion:?}")
        }
    }
}

fn measured_inference_pairs(
    interner: &mut Interner,
    count: usize,
    width: usize,
    target_param: TypeId,
) -> Vec<(TypeId, TypeId)> {
    let number = interner.well_known().number;
    (0..count)
        .map(|group| {
            let source = interner.reserve_object();
            let target = interner.reserve_object();
            let source_properties: Vec<_> = (0..width)
                .map(|index| prop(&format!("g{group:06}_p{index:02}"), number))
                .collect();
            let target_properties: Vec<_> = (0..width)
                .map(|index| prop(&format!("g{group:06}_p{index:02}"), target_param))
                .collect();
            interner.fill_object(
                source,
                ObjectType {
                    properties: source_properties,
                    ..Default::default()
                },
            );
            interner.fill_object(
                target,
                ObjectType {
                    properties: target_properties,
                    ..Default::default()
                },
            );
            (source, target)
        })
        .collect()
}

#[test]
fn measure_inference_counts_actual_snapshots_and_property_scans() {
    let mut interner = Interner::with_intrinsics();
    let target_param = interner.intern_type_param(TypeParamId(90_400), "T");
    let pairs = measured_inference_pairs(&mut interner, 2, 3, target_param);
    super::helpers::reset_inference_measure();
    for (source, target) in pairs {
        let mut candidates = Candidates::default();
        infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);
    }
    assert_eq!(
        super::helpers::inference_measure(),
        super::helpers::InferenceMeasure {
            object_snapshot_vectors: 4,
            object_snapshot_entries: 12,
            object_snapshot_name_bytes: 132,
            object_target_properties: 6,
            object_source_property_comparisons: 6,
        }
    );
}

#[test]
fn ordered_object_inference_cursor_preserves_candidates_and_duplicate_parity() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t_id = TypeParamId(90_402);
    let t = interner.intern_type_param(t_id, "T");

    // The input orders are deliberately scrambled; interning establishes the stable
    // key order used by the cursor. Source-only keys occur before, between, and
    // after the target names.
    let extras_source = interner.intern_object(ObjectType {
        properties: vec![
            prop("e", wk.boolean),
            prop("b", wk.number),
            prop("c", wk.boolean),
            prop("a", wk.boolean),
            prop("d", wk.string),
        ],
        ..Default::default()
    });
    let extras_target = interner.intern_object(ObjectType {
        properties: vec![prop("d", t), prop("b", t)],
        ..Default::default()
    });

    let missing_source = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number), prop("d", wk.string)],
        ..Default::default()
    });
    let missing_target = interner.intern_object(ObjectType {
        properties: vec![prop("d", t), prop("a", t), prop("b", t)],
        ..Default::default()
    });

    // Reserved objects admit the internal duplicate-name shape. Stable sorting and
    // first-match behavior mean both duplicate target members use the first source.
    let duplicate_source = interner.reserve_object();
    interner.fill_object(
        duplicate_source,
        ObjectType {
            properties: vec![prop("d", wk.number), prop("d", wk.string)],
            ..Default::default()
        },
    );
    let duplicate_target = interner.reserve_object();
    interner.fill_object(
        duplicate_target,
        ObjectType {
            properties: vec![prop("d", t), prop("d", t)],
            ..Default::default()
        },
    );

    super::helpers::reset_inference_measure();
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, extras_source, extras_target, &mut candidates);
    assert_eq!(
        candidates.get(&t_id).map(|values| values.as_slice()),
        Some(&[wk.number, wk.string][..]),
        "target canonical order determines candidate contribution order",
    );
    assert_eq!(
        super::helpers::inference_measure().object_target_properties,
        2
    );
    assert_eq!(
        super::helpers::inference_measure().object_source_property_comparisons,
        4
    );

    super::helpers::reset_inference_measure();
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(
        &mut interner,
        missing_source,
        missing_target,
        &mut candidates,
    );
    assert_eq!(
        candidates.get(&t_id).map(|values| values.as_slice()),
        Some(&[wk.number, wk.string][..]),
        "a missing target property contributes no candidate while later matches do",
    );
    assert_eq!(
        super::helpers::inference_measure().object_target_properties,
        3
    );
    assert_eq!(
        super::helpers::inference_measure().object_source_property_comparisons,
        3
    );

    super::helpers::reset_inference_measure();
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(
        &mut interner,
        duplicate_source,
        duplicate_target,
        &mut candidates,
    );
    assert_eq!(
        candidates.get(&t_id).map(|values| values.as_slice()),
        Some(&[wk.number, wk.number][..]),
        "duplicate target names retain the prior first-source-member semantics",
    );
    assert_eq!(
        super::helpers::inference_measure().object_source_property_comparisons,
        1
    );

    let mut next_type_param = 90_403;
    let fixed = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[t_id],
        &[duplicate_target],
        &[duplicate_source],
        &[],
    );
    assert_eq!(
        fixed.get(&t_id).copied(),
        Some(wk.number),
        "duplicate candidates keep the existing first-source fixing result",
    );
}

#[test]
#[ignore = "WU4 release measurement; run explicitly with --ignored --nocapture"]
fn measure_inference_hotpaths_release() {
    const WIDTH: usize = 8;
    const NAME_BYTES: usize = 11;
    for count in [10_000, 100_000] {
        let mut interner = Interner::with_intrinsics();
        let target_param = interner.intern_type_param(TypeParamId(90_401), "T");
        let pairs = measured_inference_pairs(&mut interner, count, WIDTH, target_param);
        super::helpers::reset_inference_measure();
        let started = Instant::now();
        for (source, target) in pairs {
            let mut candidates = Candidates::default();
            infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);
            assert_eq!(
                candidates.get(&TypeParamId(90_401)).map(Vec::len),
                Some(WIDTH)
            );
        }
        let elapsed = started.elapsed();
        let measure = super::helpers::inference_measure();
        assert_eq!(measure.object_snapshot_vectors, (count * 2) as u64);
        assert_eq!(measure.object_snapshot_entries, (count * WIDTH * 2) as u64);
        assert_eq!(
            measure.object_snapshot_name_bytes,
            (count * WIDTH * 2 * NAME_BYTES) as u64
        );
        assert_eq!(measure.object_target_properties, (count * WIDTH) as u64);
        assert_eq!(
            measure.object_source_property_comparisons,
            (count * WIDTH) as u64
        );
        println!(
            "WU4 inference count={count} width={WIDTH} elapsed_ms={} counters={measure:?}",
            elapsed.as_millis()
        );
    }
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
    struct IdentityNormalization;
    impl RelationNormalization for IdentityNormalization {
        fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
            Ok(ty)
        }
    }
    let mut candidates = Candidates::default();
    let outcome = infer_from_types_for_query(
        &mut interner,
        num_arr,
        union_target,
        &mut candidates,
        &IdentityNormalization,
    );
    assert!(matches!(outcome, DemandOutcome::Ready(())));
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

#[test]
fn object_inference_matches_iterator_and_async_iterator_by_exact_key() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter_id = TypeParamId(90_301);
    let parameter = interner.intern_type_param(parameter_id, "T");
    let iterator_target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            parameter,
        )],
        ..Default::default()
    });
    let iterator_source = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            wk.string,
        )],
        ..Default::default()
    });
    let async_source = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::AsyncIterator,
            wk.number,
        )],
        ..Default::default()
    });
    let mut next_type_param = 90_302;

    let matched = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[parameter_id],
        &[iterator_target],
        &[iterator_source],
        &[],
    );
    assert_eq!(matched.get(&parameter_id), Some(&wk.string));

    let mismatched = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[parameter_id],
        &[iterator_target],
        &[async_source],
        &[],
    );
    assert_eq!(mismatched.get(&parameter_id), Some(&wk.unknown));
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
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("x", t)],
        ret: t,
    });
    // Argument type `(x: number) => number`.
    let source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
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

/// A function receiver is a structural child for inference but never a positional
/// call argument.
#[test]
fn infers_through_function_receiver() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(t),
        params: vec![ParameterType::required("value", wk.unknown)],
        ret: wk.unknown,
    });
    let source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wk.number),
        params: vec![ParameterType::required("value", wk.unknown)],
        ret: wk.unknown,
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
    assert_eq!(map.get(&TypeParamId(0)).copied(), Some(wk.number));
}

#[test]
fn conditional_infer_callable_object_uses_last_call_signature() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let r = interner.intern_type_param(TypeParamId(0), "R");
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: r,
    });
    let first = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.string)],
        ret: wk.number,
    });
    let last = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.string,
    });
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("tag", wk.boolean)],
        call_signatures: vec![first, last],
        ..Default::default()
    });

    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);

    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.string][..]),
        "only the final call signature contributes the inferred return",
    );
}

/// Incompatible candidates from different arguments do not union into a target
/// wide enough to accept both; fixing keeps a replay target that will reject the
/// later string argument.
#[test]
fn incompatible_multi_source_candidates_fix_to_first_prepared() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let s = interner.intern_literal(LiteralValue::String("s".to_string()));
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
        Some(wk.number),
        "T fixes to number so the replayed string argument can fail"
    );
}

/// Compatible same-family literal candidates from different arguments keep their
/// literal union, matching `both(1, 2)` returning `1 | 2`.
#[test]
fn same_family_literal_candidates_union() {
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));
    let expected = interner.union(vec![one, two]);
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
        Some(expected),
        "T = 1 | 2"
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

#[test]
fn call_site_candidates_remember_argument_sources() {
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let s = interner.intern_literal(LiteralValue::String("s".to_string()));

    let candidates = collect_call_site_candidates(
        &mut interner,
        &[
            ParameterType::required("a", t),
            ParameterType::required("b", t),
        ],
        &[one, s],
        &[true, false],
        None,
    );

    let cands = candidates
        .get(&TypeParamId(0))
        .expect("T receives candidates");
    assert_eq!(cands.len(), 2);
    assert_eq!(cands[0].ty, one);
    assert_eq!(
        cands[0].source,
        CallSiteSource::Argument {
            index: 0,
            occurrence: 0
        }
    );
    assert!(cands[0].fresh);
    assert_eq!(cands[1].ty, s);
    assert_eq!(
        cands[1].source,
        CallSiteSource::Argument {
            index: 1,
            occurrence: 0
        }
    );
    assert!(!cands[1].fresh);
}

#[test]
fn call_site_tuple_expansion_keeps_distinct_occurrences() {
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let t_array = interner.intern_array(t);
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));
    let tuple = interner.intern_tuple(vec![one, two]);

    let candidates = collect_call_site_candidates(
        &mut interner,
        &[ParameterType::required("items", t_array)],
        &[tuple],
        &[false],
        None,
    );

    let cands = candidates
        .get(&TypeParamId(0))
        .expect("T receives tuple element candidates");
    assert_eq!(cands.len(), 2);
    assert_eq!(cands[0].ty, one);
    assert_eq!(
        cands[0].source,
        CallSiteSource::Argument {
            index: 0,
            occurrence: 0
        }
    );
    assert_eq!(cands[1].ty, two);
    assert_eq!(
        cands[1].source,
        CallSiteSource::Argument {
            index: 0,
            occurrence: 1
        }
    );
}

#[test]
fn single_argument_occurrences_do_not_union_incompatible_candidates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let target = interner.intern_object(ObjectType {
        properties: vec![prop("a", t), prop("b", t)],
        ..Default::default()
    });
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
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
        "the second property replays against number and fails"
    );
}

#[test]
fn nonprimitive_multi_source_candidates_do_not_union() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let object_x = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number)],
        ..Default::default()
    });
    let object_y = interner.intern_object(ObjectType {
        properties: vec![prop("y", wk.number)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[object_x, object_y],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(object_x),
        "typed object candidates from separate arguments must replay"
    );
}

#[test]
fn fresh_structural_candidate_yields_to_typed_candidate() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let fresh_object = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number)],
        ..Default::default()
    });
    let typed_object = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number), prop("y", wk.number)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[fresh_object, typed_object],
        &[true, false],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(typed_object),
        "the typed structural candidate replays the earlier fresh literal"
    );
}

#[test]
fn nullish_multi_source_candidates_merge() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let expected = interner.union(vec![wk.null, wk.undefined]);
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t, t],
        &[wk.null, wk.null, wk.undefined],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(expected),
        "null and undefined same-T candidates merge"
    );
}

#[test]
fn nullish_and_single_primitive_family_candidates_merge() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let s = interner.intern_literal(LiteralValue::String("s".to_string()));
    let expected = interner.union(vec![wk.null, s]);
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[wk.null, s],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(expected),
        "nullish plus one primitive family merges, unlike number plus string"
    );

    let u = interner.intern_type_param(TypeParamId(1), "U");
    let constrained = interner.union(vec![wk.null, wk.undefined, wk.string]);
    interner.set_type_param_constraint(TypeParamId(1), constrained);
    let expected = interner.union(vec![s, wk.undefined]);
    let mut next_type_param = 2;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(1)],
        &[u, u],
        &[s, wk.undefined],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(1)).copied(),
        Some(expected),
        "primitive-constrained nullish candidates still merge with string"
    );
}

#[test]
fn call_site_rest_array_candidates_keep_argument_sources() {
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let t_array = interner.intern_array(t);
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));

    let candidates = collect_call_site_candidates(
        &mut interner,
        &[ParameterType::rest("args", t_array)],
        &[one, two],
        &[false, false],
        None,
    );

    let cands = candidates
        .get(&TypeParamId(0))
        .expect("T receives rest element candidates");
    assert_eq!(cands.len(), 2);
    assert_eq!(
        cands[0].source,
        CallSiteSource::Argument {
            index: 0,
            occurrence: 0
        }
    );
    assert_eq!(
        cands[1].source,
        CallSiteSource::Argument {
            index: 1,
            occurrence: 0
        }
    );
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
        type_params: Vec::new(),
        receiver: None,
        params: vec![
            ParameterType::required("x", wk.string),
            ParameterType::required("y", wk.number),
        ],
        ret: wk.boolean,
    });
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
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
        Some(wk.number),
        "incompatible rest arguments fix to number so the string replay can fail"
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

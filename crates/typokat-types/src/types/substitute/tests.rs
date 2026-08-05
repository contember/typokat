use super::*;
use crate::types::repr::{
    ClassId, FunctionType, GenericTypeParam, LiteralValue, ObjectType, ParameterType, PropertyType,
    TupleRestType, TupleType, WellKnownSymbol,
};
use crate::types::DerivationEdge;

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

/// A bare type parameter is replaced by its argument; an unmapped parameter is
/// left untouched.
#[test]
fn type_param_is_replaced() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let u = interner.intern_type_param(TypeParamId(1), "U");

    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.number);

    assert_eq!(substitute(&mut interner, t, &map), wk.number, "T → number");
    assert_eq!(
        substitute(&mut interner, u, &map),
        u,
        "unmapped U is untouched"
    );
    // An intrinsic is unaffected.
    assert_eq!(substitute(&mut interner, wk.string, &map), wk.string);
}

#[test]
fn substitution_preserves_well_known_symbol_property_key() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter_id = TypeParamId(90_300);
    let parameter = interner.intern_type_param(parameter_id, "T");
    let object = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            parameter,
        )],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(parameter_id, wk.string)]);
    let substituted = substitute(&mut interner, object, &map);
    let property = interner
        .store()
        .object_type(substituted)
        .and_then(|object| object.properties.first())
        .expect("substituted object retains its property");
    assert_eq!(
        property.key.as_well_known_symbol(),
        Some(WellKnownSymbol::Iterator)
    );
    assert_eq!(property.ty, wk.string);
}

#[test]
fn substitution_rewrites_class_arguments_in_order() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(100), "T");
    let u = interner.intern_type_param(TypeParamId(101), "U");
    let instance = interner.intern_class_instance(ClassId(4), vec![t, u]);
    let expected = interner.intern_class_instance(ClassId(4), vec![wk.string, wk.number]);
    let map = FxHashMap::from_iter([(TypeParamId(100), wk.string), (TypeParamId(101), wk.number)]);

    assert_eq!(substitute(&mut interner, instance, &map), expected);
}

#[test]
fn substitution_rewrites_both_deferred_indexed_access_operands() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(102), "T");
    let u = interner.intern_type_param(TypeParamId(103), "U");
    let access = interner.intern_deferred_indexed_access(t, u);
    let expected = interner.intern_deferred_indexed_access(wk.string, wk.number);
    let map = FxHashMap::from_iter([(TypeParamId(102), wk.string), (TypeParamId(103), wk.number)]);

    assert_eq!(substitute(&mut interner, access, &map), expected);
}

/// Substitution recurses everywhere: a type parameter nested inside an object,
/// a function, and a union is all replaced.
#[test]
fn substitution_recurses_into_object_function_union() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");

    // { value: T }
    let box_t = interner.intern_object(ObjectType {
        properties: vec![prop("value", t)],
        ..Default::default()
    });
    // (this: T, x: T) => T
    let fn_t = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(t),
        params: vec![ParameterType::required("x", t)],
        ret: t,
    });
    // T | null
    let t_or_null = interner.union(vec![t, wk.null]);

    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.number);

    // { value: number }
    let box_num = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let fn_num = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wk.number),
        params: vec![ParameterType::required("x", wk.number)],
        ret: wk.number,
    });
    let num_or_null = interner.union(vec![wk.number, wk.null]);

    assert_eq!(substitute(&mut interner, box_t, &map), box_num);
    assert_eq!(substitute(&mut interner, fn_t, &map), fn_num);
    assert_eq!(substitute(&mut interner, t_or_null, &map), num_or_null);
}

#[test]
fn substitution_rewrites_accessor_write_type() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(70), "T");

    let mut property = prop("value", wk.number);
    property.write_ty = Some(t);
    let template = interner.intern_object(ObjectType {
        properties: vec![property],
        ..Default::default()
    });

    let mut expected_property = prop("value", wk.number);
    expected_property.write_ty = Some(wk.string);
    let expected = interner.intern_object(ObjectType {
        properties: vec![expected_property],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(TypeParamId(70), wk.string)]);

    assert_eq!(substitute(&mut interner, template, &map), expected);
}

/// M32/WU2: substitution rewrites every function parameter child while preserving
/// required/optional/default/rest shape.
#[test]
fn substitution_preserves_function_signature_shape() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let u = interner.intern_type_param(TypeParamId(1), "U");
    let t_array = interner.intern_array(t);

    let shaped = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![
            ParameterType::required("a", t),
            ParameterType::optional("b", u),
            ParameterType::defaulted("c", t),
            ParameterType::rest("args", t_array),
        ],
        ret: u,
    });

    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.number);
    map.insert(TypeParamId(1), wk.string);

    let number_array = interner.intern_array(wk.number);
    let expected = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![
            ParameterType::required("a", wk.number),
            ParameterType::optional("b", wk.string),
            ParameterType::defaulted("c", wk.number),
            ParameterType::rest("args", number_array),
        ],
        ret: wk.string,
    });

    assert_eq!(substitute(&mut interner, shaped, &map), expected);
}

#[test]
fn outer_substitution_preserves_inner_generic_binder() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let outer = interner.intern_type_param(TypeParamId(80), "T");
    let inner = interner.intern_type_param(TypeParamId(81), "U");
    let callback = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", outer)],
        ret: inner,
    });
    let map = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: TypeParamId(81),
            constraint: Some(outer),
            default: Some(outer),
        }],
        receiver: None,
        params: vec![ParameterType::required("transform", callback)],
        ret: inner,
    });
    let box_template = interner.intern_object(ObjectType {
        properties: vec![prop("map", map)],
        ..Default::default()
    });

    let mut substitutions = FxHashMap::default();
    substitutions.insert(TypeParamId(80), wk.number);
    substitutions.insert(TypeParamId(81), wk.string);
    let box_number = substitute(&mut interner, box_template, &substitutions);

    let expected_callback = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.number)],
        ret: inner,
    });
    let expected_map = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: TypeParamId(81),
            constraint: Some(wk.number),
            default: Some(wk.number),
        }],
        receiver: None,
        params: vec![ParameterType::required("transform", expected_callback)],
        ret: inner,
    });
    let expected_box = interner.intern_object(ObjectType {
        properties: vec![prop("map", expected_map)],
        ..Default::default()
    });

    assert_eq!(
        box_number, expected_box,
        "Box<T>.map<U> keeps U after T substitution"
    );
}

/// Substitution rewrites an array's element (M17): `T[]` → `number[]`, nested
/// `T[][]` → `number[][]`, and an unmapped element leaves the array untouched
/// (the no-op path returns the original id).
#[test]
fn substitution_rewrites_array_element() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let u = interner.intern_type_param(TypeParamId(1), "U");

    // T[] and T[][].
    let arr_t = interner.intern_array(t);
    let arr_arr_t = interner.intern_array(arr_t);
    // U[] (its element is not in the map).
    let arr_u = interner.intern_array(u);

    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.number);

    let arr_num = interner.intern_array(wk.number);
    let arr_arr_num = interner.intern_array(arr_num);

    assert_eq!(
        substitute(&mut interner, arr_t, &map),
        arr_num,
        "T[] → number[]"
    );
    assert_eq!(
        substitute(&mut interner, arr_arr_t, &map),
        arr_arr_num,
        "T[][] → number[][]"
    );
    assert_eq!(
        substitute(&mut interner, arr_u, &map),
        arr_u,
        "U[] (unmapped element) is unchanged"
    );
}

/// Substitution rewrites each tuple element **positionally** (M18): `[T, U]` →
/// `[number, string]`, order preserved; an unmapped element survives; a tuple
/// with no mapped element is returned unchanged (no-op path).
#[test]
fn substitution_rewrites_tuple_elements_positionally() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let u = interner.intern_type_param(TypeParamId(1), "U");

    // [T, U] and [U, T] (order matters).
    let tu = interner.intern_tuple(vec![t, u]);
    let ut = interner.intern_tuple(vec![u, t]);
    // [string, T] — only T is mapped.
    let str_t = interner.intern_tuple(vec![wk.string, t]);
    // [string, boolean] — no mapped element (unchanged under T → number).
    let str_bool = interner.intern_tuple(vec![wk.string, wk.boolean]);

    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.number);
    map.insert(TypeParamId(1), wk.string);

    // [T, U] → [number, string]; [U, T] → [string, number] (order preserved).
    let num_str = interner.intern_tuple(vec![wk.number, wk.string]);
    let str_num = interner.intern_tuple(vec![wk.string, wk.number]);
    assert_eq!(
        substitute(&mut interner, tu, &map),
        num_str,
        "[T, U] → [number, string]"
    );
    assert_eq!(
        substitute(&mut interner, ut, &map),
        str_num,
        "[U, T] → [string, number] (positional, order preserved)"
    );

    // [string, T] → [string, number] (only the mapped element rewritten).
    let str_num_via = interner.intern_tuple(vec![wk.string, wk.number]);
    assert_eq!(substitute(&mut interner, str_t, &map), str_num_via);

    // No mapped element → unchanged id (no-op path returns the original).
    assert_eq!(
        substitute(&mut interner, str_bool, &map),
        str_bool,
        "a tuple with no mapped element is unchanged"
    );
}

/// M32/WU2: substitution rewrites tuple rest child types as well as fixed elements.
#[test]
fn substitution_rewrites_tuple_rest_shape() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let u = interner.intern_type_param(TypeParamId(1), "U");
    let t_array = interner.intern_array(t);

    let tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![u],
        TupleRestType::new(0, t_array),
    ));

    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.number);
    map.insert(TypeParamId(1), wk.string);

    let number_array = interner.intern_array(wk.number);
    let expected = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.string],
        TupleRestType::new(0, number_array),
    ));

    assert_eq!(substitute(&mut interner, tuple, &map), expected);

    let no_mapped = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.boolean],
        TupleRestType::new(1, number_array),
    ));
    assert_eq!(
        substitute(&mut interner, no_mapped, &map),
        no_mapped,
        "a tuple rest shape with no mapped child is unchanged"
    );
}

/// Nested substitution flows through `Box<Box<number>>`-style nesting: the
/// outer object's property is itself an object referencing the parameter.
#[test]
fn substitution_flows_through_nested_objects() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");

    // { value: { value: T } }
    let inner = interner.intern_object(ObjectType {
        properties: vec![prop("value", t)],
        ..Default::default()
    });
    let outer = interner.intern_object(ObjectType {
        properties: vec![prop("value", inner)],
        ..Default::default()
    });

    // Instantiate with T → { value: number }.
    let box_num = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), box_num);

    // inner = { value: T }  →  { value: { value: number } }
    let inner_subst = interner.intern_object(ObjectType {
        properties: vec![prop("value", box_num)],
        ..Default::default()
    });
    // outer = { value: inner } = { value: { value: T } }  →
    //         { value: { value: { value: number } } }
    let outer_subst = interner.intern_object(ObjectType {
        properties: vec![prop("value", inner_subst)],
        ..Default::default()
    });
    assert_eq!(substitute(&mut interner, inner, &map), inner_subst);
    assert_eq!(substitute(&mut interner, outer, &map), outer_subst);
}

#[test]
fn derived_substitution_retains_the_exact_nested_path() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter_id = TypeParamId(89_001);
    let parameter = interner.intern_type_param(parameter_id, "T");
    let inner = interner.intern_object(ObjectType {
        properties: vec![prop("value", parameter)],
        ..Default::default()
    });
    let outer = interner.intern_object(ObjectType {
        properties: vec![prop("nested", inner)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(parameter_id, DerivedType::plain(wk.string))]);

    let result = substitute_derived(&mut interner, outer, &map);
    let nested = interner
        .store()
        .object_type(result.ty)
        .and_then(|object| object.properties.first())
        .map(|property| property.ty)
        .expect("substitution preserves the nested property");
    assert_eq!(interner.derivation_identity(result), outer);
    let nested = interner.derivation_child(result, DerivationEdge::ObjectProperty(0), nested);
    assert_eq!(interner.derivation_identity(nested), inner);
}

#[test]
fn hash_consed_results_keep_occurrence_specific_identities() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first_id = TypeParamId(89_011);
    let second_id = TypeParamId(89_012);
    let first_parameter = interner.intern_type_param(first_id, "A");
    let second_parameter = interner.intern_type_param(second_id, "B");
    let first = interner.intern_object(ObjectType {
        properties: vec![prop("value", first_parameter)],
        ..Default::default()
    });
    let second = interner.intern_object(ObjectType {
        properties: vec![prop("value", second_parameter)],
        ..Default::default()
    });

    let first_result = substitute_derived(
        &mut interner,
        first,
        &FxHashMap::from_iter([(first_id, DerivedType::plain(wk.string))]),
    );
    let second_result = substitute_derived(
        &mut interner,
        second,
        &FxHashMap::from_iter([(second_id, DerivedType::plain(wk.string))]),
    );

    assert_eq!(first_result.ty, second_result.ty);
    assert_eq!(interner.derivation_identity(first_result), first);
    assert_eq!(interner.derivation_identity(second_result), second);
}

#[test]
fn freeze_discards_derivations_and_rejects_sibling_local_tokens() {
    let mut base = Interner::with_intrinsics();
    let wk = base.well_known();
    let parameter_id = TypeParamId(89_021);
    let parameter = base.intern_type_param(parameter_id, "T");
    let inner = base.intern_object(ObjectType {
        properties: vec![prop("value", parameter)],
        ..Default::default()
    });
    let outer = base.intern_object(ObjectType {
        properties: vec![prop("nested", inner)],
        ..Default::default()
    });
    let base_result = substitute_derived(
        &mut base,
        outer,
        &FxHashMap::from_iter([(parameter_id, DerivedType::plain(wk.number))]),
    );
    base.freeze_as_base().expect("complete graph freezes");

    let mut first = base.fork_delta().expect("first sealed graph forks");
    let mut second = base.fork_delta().expect("second sealed graph forks");
    for fork in [&first, &second] {
        assert_eq!(fork.derivation_identity(base_result), base_result.ty);
        let nested_ty = fork
            .store()
            .object_type(base_result.ty)
            .and_then(|object| object.properties.first())
            .map(|property| property.ty)
            .expect("base result retains nested object");
        let nested =
            fork.derivation_child(base_result, DerivationEdge::ObjectProperty(0), nested_ty);
        assert_eq!(nested, DerivedType::plain(nested_ty));
    }

    let first_literal = first.intern_literal(LiteralValue::String("first".to_owned()));
    let first_local = substitute_derived(
        &mut first,
        inner,
        &FxHashMap::from_iter([(parameter_id, DerivedType::plain(first_literal))]),
    );
    let second_literal = second.intern_literal(LiteralValue::String("second".to_owned()));
    let second_local = substitute_derived(
        &mut second,
        inner,
        &FxHashMap::from_iter([(parameter_id, DerivedType::plain(second_literal))]),
    );
    assert_eq!(
        first_local.ty, second_local.ty,
        "dense sibling suffixes intentionally reuse numeric TypeIds"
    );
    assert_eq!(first.derivation_identity(first_local), inner);
    assert_eq!(second.derivation_identity(second_local), inner);
    assert_eq!(
        second.derivation_identity(first_local),
        first_local.ty,
        "a foreign sibling token must fall back to its semantic TypeId"
    );
}

#[test]
fn derivation_graph_retains_a_recursive_structural_edge() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter_id = TypeParamId(89_031);
    let parameter = interner.intern_type_param(parameter_id, "T");
    let template = interner.reserve_object();
    interner.fill_object(
        template,
        ObjectType {
            properties: vec![prop("next", template), prop("value", parameter)],
            ..Default::default()
        },
    );
    let result = interner.reserve_object();
    interner.fill_object(
        result,
        ObjectType {
            properties: vec![prop("next", result), prop("value", wk.string)],
            ..Default::default()
        },
    );
    let derived = super::derivation::derive_substitution(
        &mut interner,
        template,
        result,
        &FxHashMap::from_iter([(parameter_id, DerivedType::plain(wk.string))]),
    );

    assert_eq!(interner.derivation_identity(derived), template);
    assert_eq!(
        interner.derivation_child(derived, DerivationEdge::ObjectProperty(0), result),
        derived,
        "the recursive child keeps the reserved root token"
    );
}

/// Two distinct instantiations are distinct interned types, and the same
/// instantiation re-interns to the same id (instantiation interning is
/// consistent): `Box<number>` interns consistently and `Box<number>` ≠
/// `Box<string>`.
#[test]
fn instantiation_interning_is_consistent_and_distinct() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let box_t = interner.intern_object(ObjectType {
        properties: vec![prop("value", t)],
        ..Default::default()
    });

    let mut num_map = FxHashMap::default();
    num_map.insert(TypeParamId(0), wk.number);
    let mut str_map = FxHashMap::default();
    str_map.insert(TypeParamId(0), wk.string);

    let box_num_a = substitute(&mut interner, box_t, &num_map);
    let box_num_b = substitute(&mut interner, box_t, &num_map);
    let box_str = substitute(&mut interner, box_t, &str_map);

    assert_eq!(box_num_a, box_num_b, "Box<number> interns consistently");
    assert_ne!(box_num_a, box_str, "Box<number> ≠ Box<string>");
}

/// M25: substitution must not capture conditional `infer` binders, even under
/// nesting; only free declaration parameters rewrite.
#[test]
fn substitute_does_not_capture_infer_binders_under_nesting() {
    use crate::types::repr::ConditionalType;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let infer0 = interner.intern_infer(0);
    let arr_infer0 = interner.intern_array(infer0);

    // Inner: `T extends (infer U)[] ? U : T` (infer in extends + true, T in check/false).
    let inner = interner.intern_conditional(ConditionalType {
        check: t,
        extends_ty: arr_infer0,
        true_branch: infer0,
        false_branch: t,
        infer_count: 1,
        distributive: true,
        poisoned: false,
    });
    // Outer nests the inner conditional as the true branch: `T extends string ? <inner> : T`.
    let outer = interner.intern_conditional(ConditionalType {
        check: t,
        extends_ty: wk.string,
        true_branch: inner,
        false_branch: t,
        infer_count: 0,
        distributive: true,
        poisoned: false,
    });

    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.number);
    let result = substitute(&mut interner, outer, &map);

    // The result: `number extends string ? <inner'> : number` where inner' is
    // `number extends (infer U)[] ? U : number` — the infer nodes UNCHANGED.
    let store = interner.store();
    let outer_c = store
        .conditional_type(result)
        .expect("outer is conditional");
    assert_eq!(outer_c.check, wk.number, "T → number in outer check");
    assert_eq!(outer_c.false_branch, wk.number, "T → number in outer false");
    let inner_c = store
        .conditional_type(outer_c.true_branch)
        .expect("inner is conditional");
    assert_eq!(inner_c.check, wk.number, "T → number in inner check");
    assert_eq!(inner_c.false_branch, wk.number, "T → number in inner false");
    // No capture: the infer binder is untouched in both extends and true positions.
    assert_eq!(
        inner_c.extends_ty, arr_infer0,
        "infer extends unchanged (no capture)"
    );
    assert_eq!(
        inner_c.true_branch, infer0,
        "infer true branch unchanged (no capture)"
    );
}

/// M25 distribution guard: union/`never`/`boolean` check arguments defer to lazy
/// instantiation; single non-distributing arguments plainly rewrite.
#[test]
fn distributive_conditional_defers_on_union_plain_on_single() {
    use crate::types::repr::ConditionalType;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let yes = interner.intern_literal(crate::types::repr::LiteralValue::String("yes".into()));
    let no = interner.intern_literal(crate::types::repr::LiteralValue::String("no".into()));

    // `T extends string ? "yes" : "no"` (distributive — naked check param).
    let cond = interner.intern_conditional(ConditionalType {
        check: t,
        extends_ty: wk.string,
        true_branch: yes,
        false_branch: no,
        infer_count: 0,
        distributive: true,
        poisoned: false,
    });

    // A union argument defers as an instantiation of the ORIGINAL node.
    let union = interner.union(vec![wk.string, wk.number]);
    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), union);
    let deferred = substitute(&mut interner, cond, &map);
    let inst = interner
        .store()
        .instantiation_type(deferred)
        .expect("a union check argument must defer as a lazy instantiation");
    assert_eq!(inst.base, cond, "the instantiation wraps the original node");
    assert_eq!(inst.args, vec![(TypeParamId(0), union)]);

    // `never` distributes too (→ never at evaluation), so it must also defer.
    let mut never_map = FxHashMap::default();
    never_map.insert(TypeParamId(0), wk.never);
    let deferred_never = substitute(&mut interner, cond, &never_map);
    assert!(
        interner
            .store()
            .instantiation_type(deferred_never)
            .is_some(),
        "a `never` check argument must defer (it distributes to `never`)"
    );

    // A single non-distributing argument takes the PLAIN rewrite — a concrete
    // conditional, never a wrap (the evaluator's per-member path relies on this).
    let mut single_map = FxHashMap::default();
    single_map.insert(TypeParamId(0), wk.string);
    let plain = substitute(&mut interner, cond, &single_map);
    let plain_cond = interner
        .store()
        .conditional_type(plain)
        .expect("a single check argument must plainly rewrite to a concrete conditional");
    assert_eq!(plain_cond.check, wk.string);
}

/// M26: substitution rewrites mapped key/value templates without capturing the
/// node's own `MappedValue` placeholder.
#[test]
fn substitute_maps_over_mapped_without_capturing_value_placeholder() {
    use crate::types::repr::{MappedType, ModifierOp};

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let value_placeholder = interner.intern_mapped_value();
    // Value template `T[K] | null` — the placeholder inside a union.
    let value_template = interner.union(vec![value_placeholder, wk.null]);

    // `{ [K in keyof T]: T[K] | null }` — key source is the bare parameter `T`.
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: t,
        value_template,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });

    // The concrete source `P = { a: number }`.
    let p = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), p);

    let result = substitute(&mut interner, mapped, &map);
    let out = interner
        .store()
        .mapped_type(result)
        .copied()
        .expect("result is a mapped type");
    assert_eq!(out.key_source, p, "T → P in the key source");
    assert_eq!(
        out.value_template, value_template,
        "the value template (with its MappedValue placeholder) is untouched"
    );
    assert!(out.homomorphic);
}

/// M28: substitution rewrites mapped modifiers source and deferred-`keyof`
/// operands as pure rewrites; evaluation still happens only at demand sites.
#[test]
fn substitute_rewrites_modifiers_source_and_keyof_operand() {
    use crate::types::repr::{MappedType, ModifierOp};

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let k = interner.intern_type_param(TypeParamId(1), "K");
    let placeholder = interner.intern_mapped_value();

    // `{ [P in K]: T[P] }` — the Pick shape: key source K, modifiers source T.
    let pick = interner.intern_mapped(MappedType {
        homomorphic: false,
        key_source: k,
        value_template: placeholder,
        modifiers_source: Some(t),
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let p = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    let a_key = interner.intern_literal(crate::types::repr::LiteralValue::String("a".into()));
    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), p);
    map.insert(TypeParamId(1), a_key);

    let result = substitute(&mut interner, pick, &map);
    let out = interner
        .store()
        .mapped_type(result)
        .copied()
        .expect("result is a mapped type");
    assert_eq!(out.key_source, a_key, "K → \"a\" in the key source");
    assert_eq!(
        out.modifiers_source,
        Some(p),
        "T → P in the modifiers source"
    );
    assert_eq!(
        out.value_template, placeholder,
        "the bound placeholder is never captured"
    );

    // `keyof T` → `keyof P`: a pure rewrite (no evaluation in substitution).
    let keyof_t = interner.intern_keyof(t);
    let rewritten = substitute(&mut interner, keyof_t, &map);
    assert_eq!(
        interner.store().keyof_operand(rewritten),
        Some(p),
        "the operand is rewritten and the node stays a deferred keyof"
    );
    // An unmapped operand leaves the node unchanged (no-op path).
    let u = interner.intern_type_param(TypeParamId(9), "U");
    let keyof_u = interner.intern_keyof(u);
    assert_eq!(substitute(&mut interner, keyof_u, &map), keyof_u);
}

/// M26 mapped distribution guard: naked-parameter union arguments distribute,
/// but direct `keyof (A | B)` maps remain single nodes for common-key semantics.
#[test]
fn homomorphic_mapped_distributes_over_naked_param_union_only() {
    use crate::types::repr::{MappedType, ModifierOp};

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let placeholder = interner.intern_mapped_value();
    let a = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    let b = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.string)],
        ..Default::default()
    });
    let ab = interner.union(vec![a, b]);

    // `Ident<T> = { [K in keyof T]: T[K] }`.
    let ident = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: t,
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });

    // Naked-param union argument distributes: Ident<A | B> = Ident<A> | Ident<B>.
    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), ab);
    let distributed = substitute(&mut interner, ident, &map);
    let ident_a = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: a,
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let ident_b = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: b,
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let expected = interner.union(vec![ident_a, ident_b]);
    assert_eq!(distributed, expected, "Ident<A | B> = Ident<A> | Ident<B>");

    // `never` distributes to zero members → never.
    let mut never_map = FxHashMap::default();
    never_map.insert(TypeParamId(0), wk.never);
    assert_eq!(
        substitute(&mut interner, ident, &never_map),
        wk.never,
        "Ident<never> = never"
    );

    // The DIRECT union form (key source already a union, not a parameter) must NOT
    // distribute: an unrelated substitution leaves it a single mapped node.
    let direct = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: ab,
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut other_map = FxHashMap::default();
    other_map.insert(TypeParamId(9), wk.string);
    assert_eq!(
        substitute(&mut interner, direct, &other_map),
        direct,
        "the direct-union form stays a single mapped node (no distribution)"
    );
}

/// M27 — `substitute` rewrites a template's **holes** (its text segments untouched):
/// `` `tag:${T}` `` → `` `tag:${string}` `` under `T → string`; a template with no
/// mapped hole is returned unchanged (no-op path).
#[test]
fn substitution_rewrites_template_holes() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");

    // `` `tag:${T}` `` and `` `x${U}` `` (U unmapped).
    let tag_t = interner.intern_template(TemplateType {
        texts: vec!["tag:".to_string(), String::new()],
        holes: vec![t],
    });
    let u = interner.intern_type_param(TypeParamId(1), "U");
    let x_u = interner.intern_template(TemplateType {
        texts: vec!["x".to_string(), String::new()],
        holes: vec![u],
    });

    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.string);

    let tag_string = interner.intern_template(TemplateType {
        texts: vec!["tag:".to_string(), String::new()],
        holes: vec![wk.string],
    });
    assert_eq!(
        substitute(&mut interner, tag_t, &map),
        tag_string,
        "`tag:${{T}}` → `tag:${{string}}`"
    );
    assert_eq!(
        substitute(&mut interner, x_u, &map),
        x_u,
        "a template with no mapped hole is unchanged"
    );
}

/// A self-referential **nominal** object (no type parameter) substitutes to
/// itself and **terminates** — the cycle guard returns the original id on
/// re-entry rather than looping.
#[test]
fn self_referential_nominal_object_terminates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // A recursive nominal interface `List { head: number; tail: List | null }`.
    let list = interner.reserve_object();
    let list_or_null = interner.union(vec![list, wk.null]);
    interner.fill_object(
        list,
        ObjectType {
            properties: vec![prop("head", wk.number), prop("tail", list_or_null)],
            ..Default::default()
        },
    );

    // Substituting T → string over the recursive `list` must terminate. The
    // list has no type parameter, so the result is the list itself (its nominal
    // identity is preserved — it is not re-interned into a structural copy).
    let mut map = FxHashMap::default();
    map.insert(TypeParamId(0), wk.string);
    assert_eq!(
        substitute(&mut interner, list, &map),
        list,
        "a recursive nominal object with no type param substitutes to itself"
    );
}

#[test]
fn measure_substitution_distinguishes_exact_blocked_context_repeats() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(90_200);
    let t = interner.intern_type_param(id, "T");
    let shared = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", t)],
        ret: t,
    });
    let root = interner.intern_tuple(vec![shared, shared]);
    let mut map = FxHashMap::default();
    map.insert(id, wk.number);

    let same_context = {
        let _scope = super::start_substitution_measure();
        let mut substitution = Substitution::new(&map);
        assert_ne!(substitution.apply(&mut interner, root), root);
        super::substitution_measure().expect("the counter scope must remain enabled")
    };
    assert_eq!(same_context.runs, 1);
    assert_eq!(same_context.apply_visits, 5);
    assert_eq!(same_context.type_id_repeats, 2);
    assert_eq!(same_context.exact_context_repeats, 2);
    assert_eq!(same_context.type_param_map_hits, 1);
    assert_eq!(same_context.blocked_type_param_hits, 0);
    assert_eq!(same_context.cycle_reentries, 0);
    assert_eq!(same_context.completed_memo_entries, 3);
    assert_eq!(same_context.completed_memo_hits, 2);
    assert_eq!(same_context.cycle_tainted_skips, 0);

    let wrapper = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("value", shared)],
        ret: wk.void,
    });
    let adversary = interner.intern_tuple(vec![shared, wrapper, shared]);
    let (result, measure) = {
        let _scope = super::start_substitution_measure();
        let mut substitution = Substitution::new(&map);
        let result = substitution.apply(&mut interner, adversary);
        let measure = super::substitution_measure().expect("the counter scope must remain enabled");
        (result, measure)
    };
    assert_ne!(result, adversary);
    // Prefilter: the wrapper subtree is fully shadowed (free {T} − binder T = ∅)
    // and skipped, dropping its 5 visits (wrapper, shared@[T], t@[T]×2, void).
    assert_eq!(measure.apply_visits, 5);
    // Prefilter: only t and shared repeat once each at the empty context now.
    assert_eq!(measure.type_id_repeats, 2);
    assert_eq!(measure.exact_context_repeats, 2);
    assert_eq!(measure.type_param_map_hits, 1);
    // Prefilter: the blocked leaf inside the skipped wrapper is never reached.
    assert_eq!(measure.blocked_type_param_hits, 0);
    assert_eq!(measure.cycle_reentries, 0);
    // Prefilter: the skipped subtree's 4 entries (wrapper, shared@[T], t@[T], void) are gone.
    assert_eq!(measure.completed_memo_entries, 3);
    // Prefilter: the former t@[T] reuse is now inside the skip (t@[] and shared@[] remain).
    assert_eq!(measure.completed_memo_hits, 2);
    assert_eq!(measure.cycle_tainted_skips, 0);
    // Prefilter: with no blocked-context revisit, both repeat counters agree.
    assert_eq!(
        measure.exact_context_repeats, measure.type_id_repeats,
        "the shared function is only ever visited under the empty context now"
    );

    let recursive = interner.reserve_object();
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("self", recursive)],
            ..Default::default()
        },
    );
    let cycle = {
        let _scope = super::start_substitution_measure();
        let mut substitution = Substitution::new(&map);
        assert_eq!(substitution.apply(&mut interner, recursive), recursive);
        super::substitution_measure().expect("the counter scope must remain enabled")
    };
    // Prefilter: the param-free self-cycle is proven identity without opening a
    // single frame, so every walk-derived counter drops to zero (was 2/1/1/1/0/0/1).
    assert_eq!(cycle.apply_visits, 0);
    assert_eq!(cycle.type_id_repeats, 0);
    assert_eq!(cycle.exact_context_repeats, 0);
    assert_eq!(cycle.cycle_reentries, 0);
    assert_eq!(cycle.completed_memo_entries, 0);
    assert_eq!(cycle.completed_memo_hits, 0);
    assert_eq!(cycle.cycle_tainted_skips, 0);
    assert!(cycle.prefilter_skips >= 1);
}

#[test]
#[ignore = "WU4 counter-only measurement; exact-context snapshots make timing unavailable"]
fn measure_substitution_hotpaths_counter_only() {
    for count in [10_000, 100_000] {
        let mut samples = Vec::new();
        for _ in 0..5 {
            let mut interner = Interner::with_intrinsics();
            let wk = interner.well_known();
            let id = TypeParamId(90_210);
            let t = interner.intern_type_param(id, "T");
            let shared = interner.intern_function(FunctionType {
                type_params: Vec::new(),
                receiver: None,
                params: vec![ParameterType::required("value", t)],
                ret: t,
            });
            let root = interner.intern_tuple(vec![shared; count]);
            let mut map = FxHashMap::default();
            map.insert(id, wk.number);
            let _scope = super::start_substitution_measure();
            let mut substitution = Substitution::new(&map);
            assert_ne!(substitution.apply(&mut interner, root), root);
            let measure =
                super::substitution_measure().expect("the counter scope must remain enabled");
            assert_eq!(measure.apply_visits, (count + 3) as u64);
            assert_eq!(measure.type_id_repeats, count as u64);
            assert_eq!(measure.exact_context_repeats, count as u64);
            assert_eq!(measure.type_param_map_hits, 1);
            assert_eq!(measure.completed_memo_entries, 3);
            assert_eq!(measure.completed_memo_hits, count as u64);
            assert_eq!(measure.cycle_tainted_skips, 0);
            samples.push(measure);
        }
        println!(
            "substitution count={count} timing=unavailable_exact_context_snapshot counters={samples:?}"
        );
    }
}

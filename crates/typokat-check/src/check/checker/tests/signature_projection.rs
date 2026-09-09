use super::*;
use crate::types::repr::{LiteralValue, ObjectType, PropertyType, TupleRestType, TypeParamId};

#[test]
fn ready_fixed_slots_are_independent_of_unused_rest_alternatives() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let symbolic = interner.intern_type_param(TypeParamId(41), "T");
    let numbers = interner.intern_array(wk.number);
    let strings = interner.intern_array(wk.string);
    assert!(interner.set_type_param_constraint(TypeParamId(41), numbers));
    let ordinary = interner.intern_ordinary_literal(LiteralValue::String("fixed".into()));
    let regular = interner.regularize_literal_type(ordinary);
    let explicit_undefined = interner.union(vec![regular, wk.undefined]);
    for members in [vec![symbolic, numbers], vec![numbers, symbolic]] {
        let opaque_union = interner.union(members);
        let nested = interner.intern_tuple_type(TupleType::with_rest(
            vec![wk.boolean],
            TupleRestType::new(1, opaque_union),
        ));
        let readonly = interner.intern_readonly(opaque_union);
        let ready_union = interner.union(vec![numbers, strings]);
        let reversed_ready_union = interner.union(vec![strings, numbers]);
        for (rest, ready, ordinary_fixed_available) in [
            (opaque_union, false, false),
            (nested, false, true),
            (readonly, false, true),
            (ready_union, true, true),
            (reversed_ready_union, true, true),
        ] {
            for (first, second) in [(symbolic, explicit_undefined), (ordinary, regular)] {
                let params = [
                    ParameterType::optional("first", first),
                    ParameterType::required("second", second),
                    ParameterType::rest("args", rest),
                ];
                for count in [0, 1, 2, 4] {
                    for (index, declared) in [first, second].into_iter().enumerate() {
                        assert_eq!(
                            contextual_parameter_target(&mut interner, &params, index, count),
                            ordinary_fixed_available.then_some(declared),
                        );
                        assert_eq!(
                            ready_contextual_parameter_target(&mut interner, &params, index, count),
                            Some(declared),
                        );
                    }
                }
                if !ready {
                    assert_eq!(
                        ready_contextual_parameter_target(&mut interner, &params, 2, 4),
                        None
                    );
                }
                assert!(params[0].optional);
                assert_eq!(params[0].ty, first);
            }
        }
    }
}

#[test]
fn ready_projection_matches_ordinary_positioning_and_preserves_declared_ids() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let symbolic = interner.intern_type_param(TypeParamId(1), "T");
    let ordinary = interner.intern_ordinary_literal(LiteralValue::String("a".into()));
    let regular = interner.regularize_literal_type(ordinary);
    let explicit_undefined = interner.union(vec![regular, wk.undefined]);
    let numbers = interner.intern_array(wk.number);
    let strings = interner.intern_array(wk.string);
    let symbols = interner.intern_array(symbolic);
    let readonly_symbols = interner.intern_readonly(symbols);
    let tuple = interner.intern_tuple(vec![wk.number, wk.string]);
    let alternatives = interner.union(vec![numbers, strings]);
    let number_or_string = interner.union(vec![wk.number, wk.string]);
    let inner = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.number, wk.string],
        TupleRestType::new(1, symbols),
    ));
    let nested = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.boolean, wk.undefined],
        TupleRestType::new(1, inner),
    ));
    let object = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", symbolic)],
        ..Default::default()
    });
    let identities = interner.intern_tuple(vec![
        ordinary,
        regular,
        symbolic,
        object,
        explicit_undefined,
    ]);
    let cases = [
        (
            vec![ParameterType::required("value", symbolic)],
            1,
            vec![symbolic],
        ),
        (
            vec![ParameterType::rest("args", numbers)],
            3,
            vec![wk.number; 3],
        ),
        (
            vec![
                ParameterType::required("first", wk.string),
                ParameterType::rest("args", numbers),
            ],
            3,
            vec![wk.string, wk.number, wk.number],
        ),
        (
            vec![ParameterType::rest("args", tuple)],
            1,
            vec![wk.number, wk.string],
        ),
        (
            vec![ParameterType::rest("args", alternatives)],
            2,
            vec![number_or_string; 2],
        ),
        (
            vec![ParameterType::rest("args", nested)],
            6,
            vec![
                wk.boolean,
                wk.number,
                symbolic,
                symbolic,
                wk.string,
                wk.undefined,
            ],
        ),
        (
            vec![ParameterType::rest("args", nested)],
            1,
            vec![wk.boolean, wk.number, wk.string, wk.undefined],
        ),
        (
            vec![
                ParameterType::optional("first", explicit_undefined),
                ParameterType::rest("args", tuple),
            ],
            1,
            vec![explicit_undefined, wk.number, wk.string],
        ),
        (
            vec![ParameterType::rest("args", identities)],
            5,
            vec![ordinary, regular, symbolic, object, explicit_undefined],
        ),
        (
            vec![ParameterType::rest("args", readonly_symbols)],
            2,
            vec![symbolic; 2],
        ),
    ];
    for (params, count, expected) in cases {
        for (index, target) in expected.into_iter().enumerate() {
            assert_eq!(
                contextual_parameter_target(&mut interner, &params, index, count),
                Some(target)
            );
            assert_eq!(
                ready_contextual_parameter_target(&mut interner, &params, index, count),
                Some(target)
            );
        }
    }
}

#[test]
fn ready_projection_rejects_only_needed_opaque_containers_without_bound_shaping() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let opaque = interner.intern_type_param(TypeParamId(1), "T");
    let numbers = interner.intern_array(wk.number);
    assert!(interner.set_type_param_constraint(TypeParamId(1), numbers));
    let nested =
        interner.intern_tuple_type(TupleType::with_rest(vec![], TupleRestType::new(0, opaque)));
    let readonly = interner.intern_readonly(opaque);
    let alternatives = interner.union(vec![opaque, numbers]);
    for rest in [opaque, nested, readonly] {
        let params = [ParameterType::rest("args", rest)];
        assert_eq!(
            contextual_parameter_target(&mut interner, &params, 0, 1),
            Some(opaque)
        );
        assert_eq!(
            ready_contextual_parameter_target(&mut interner, &params, 0, 1),
            None
        );
        let prefix = [
            ParameterType::optional("first", wk.string),
            ParameterType::rest("args", rest),
        ];
        assert_eq!(
            ready_contextual_parameter_target(&mut interner, &prefix, 0, 1),
            Some(wk.string)
        );
        assert_eq!(
            ready_contextual_parameter_target(&mut interner, &prefix, 1, 2),
            None
        );
    }
    let params = [ParameterType::rest("args", alternatives)];
    assert_eq!(
        contextual_parameter_target(&mut interner, &params, 0, 1),
        None
    );
    assert_eq!(
        ready_contextual_parameter_target(&mut interner, &params, 0, 1),
        None
    );
    let tuple = interner.intern_tuple(vec![opaque]);
    let params = [ParameterType::rest("args", tuple)];
    assert_eq!(
        ready_contextual_parameter_target(&mut interner, &params, 0, 1),
        Some(opaque)
    );
    let params = [ParameterType::required("first", opaque)];
    assert_eq!(
        ready_contextual_parameter_target(&mut interner, &params, 0, 1),
        Some(opaque)
    );
}

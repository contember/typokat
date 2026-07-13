use super::*;
use crate::check::checker::eval::keyof::{contains_deferred_keyof, keyof_of_object};
use crate::types::repr::{FunctionType, GenericTypeParam, ObjectType, ParameterType, PropertyType};
use std::time::Instant;

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn omit_this_parameter(interner: &mut Interner, argument: TypeId) -> TypeId {
    let marker = interner.well_known().omit_this_parameter;
    interner.intern_instantiation(marker, vec![(TypeParamId(90_100), argument)])
}

#[test]
fn inference_constraint_evaluation_preserves_function_metadata_while_evaluating_children() {
    let mut interner = Interner::with_intrinsics();
    let uppercase = interner.well_known().uppercase;
    let parameter = TypeParamId(90_130);
    let upper = |interner: &mut Interner, parameter: TypeParamId, text: &str| {
        let literal = interner.intern_literal(LiteralValue::String(text.into()));
        interner.intern_instantiation(uppercase, vec![(parameter, literal)])
    };
    let constraint = upper(&mut interner, parameter, "constraint");
    let default = upper(&mut interner, parameter, "default");
    let receiver = upper(&mut interner, parameter, "receiver");
    let optional = upper(&mut interner, parameter, "optional");
    let defaulted = upper(&mut interner, parameter, "defaulted");
    let rest_element = upper(&mut interner, parameter, "rest");
    let rest = interner.intern_array(rest_element);
    let ret = upper(&mut interner, parameter, "return");
    let second_default = upper(&mut interner, parameter, "second");
    let first = TypeParamId(90_131);
    let second = TypeParamId(90_132);
    let source = interner.intern_function(FunctionType {
        type_params: vec![
            GenericTypeParam {
                id: first,
                constraint: Some(constraint),
                default: Some(default),
            },
            GenericTypeParam {
                id: second,
                constraint: None,
                default: Some(second_default),
            },
        ],
        receiver: Some(receiver),
        params: vec![
            ParameterType::optional("optional", optional),
            ParameterType::defaulted("defaulted", defaulted),
            ParameterType::rest("tail", rest),
        ],
        ret,
    });
    let mut next = 90_133;

    let evaluation = evaluate_inference_constraint(&mut interner, &mut next, source);

    assert!(!evaluation.exhausted);
    assert!(!evaluation.cycle_detected);
    assert_ne!(evaluation.result, source);
    let constraint = interner.intern_literal(LiteralValue::String("CONSTRAINT".into()));
    let default = interner.intern_literal(LiteralValue::String("DEFAULT".into()));
    let second_default = interner.intern_literal(LiteralValue::String("SECOND".into()));
    let receiver = interner.intern_literal(LiteralValue::String("RECEIVER".into()));
    let optional = interner.intern_literal(LiteralValue::String("OPTIONAL".into()));
    let defaulted = interner.intern_literal(LiteralValue::String("DEFAULTED".into()));
    let rest_element = interner.intern_literal(LiteralValue::String("REST".into()));
    let rest = interner.intern_array(rest_element);
    let ret = interner.intern_literal(LiteralValue::String("RETURN".into()));
    let function = interner
        .store()
        .function_type(evaluation.result)
        .unwrap()
        .clone();

    assert_eq!(
        function.type_params,
        vec![
            GenericTypeParam {
                id: first,
                constraint: Some(constraint),
                default: Some(default),
            },
            GenericTypeParam {
                id: second,
                constraint: None,
                default: Some(second_default),
            },
        ]
    );
    assert_eq!(function.receiver, Some(receiver));
    assert_eq!(
        function.params[0],
        ParameterType::optional("optional", optional)
    );
    assert_eq!(
        function.params[1],
        ParameterType::defaulted("defaulted", defaulted)
    );
    assert_eq!(function.params[2], ParameterType::rest("tail", rest));
    assert_eq!(function.ret, ret);

    let unchanged = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: TypeParamId(90_134),
            constraint: Some(receiver),
            default: Some(default),
        }],
        receiver: Some(receiver),
        params: vec![ParameterType::optional("optional", optional)],
        ret,
    });
    let unchanged_evaluation = evaluate_inference_constraint(&mut interner, &mut next, unchanged);
    assert_eq!(unchanged_evaluation.result, unchanged);
    assert!(!unchanged_evaluation.exhausted);
    assert!(!unchanged_evaluation.cycle_detected);
}

fn assert_recursive_function_binder_cycle_preserves_identity(in_constraint: bool) {
    let mut interner = Interner::with_intrinsics();
    let uppercase = interner.well_known().uppercase;
    let recursive = interner.reserve_object();
    let receiver_literal = interner.intern_literal(LiteralValue::String("receiver".into()));
    let receiver =
        interner.intern_instantiation(uppercase, vec![(TypeParamId(90_140), receiver_literal)]);
    let (constraint, default) = if in_constraint {
        (Some(recursive), None)
    } else {
        (None, Some(recursive))
    };
    let function = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: TypeParamId(90_141),
            constraint,
            default,
        }],
        receiver: Some(receiver),
        params: vec![ParameterType::required(
            "value",
            interner.well_known().number,
        )],
        ret: interner.well_known().void,
    });
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("back", function)],
            ..Default::default()
        },
    );
    let sibling_literal = interner.intern_literal(LiteralValue::String("sibling".into()));
    let sibling =
        interner.intern_instantiation(uppercase, vec![(TypeParamId(90_142), sibling_literal)]);
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("function", function), prop("sibling", sibling)],
        ..Default::default()
    });
    let mut next = 90_143;

    super::extends::reset_constraint_eval_measure();
    let evaluation = evaluate_inference_constraint(&mut interner, &mut next, source);

    assert!(!evaluation.exhausted);
    assert!(!evaluation.cycle_detected);
    assert_ne!(evaluation.result, source);
    let expected_sibling = interner.intern_literal(LiteralValue::String("SIBLING".into()));
    let result = interner.store().object_type(evaluation.result).unwrap();
    assert_eq!(result.property("function").unwrap().ty, function);
    assert_eq!(result.property("sibling").unwrap().ty, expected_sibling);
    assert_eq!(
        interner
            .store()
            .object_type(recursive)
            .unwrap()
            .property("back")
            .unwrap()
            .ty,
        function
    );
    let measure = super::extends::constraint_eval_measure();
    assert_eq!(measure.structural_entries, 3);
    assert_eq!(measure.structural_reentries, 1);
    assert_eq!(measure.tainted_identity_returns, 2);
    assert_eq!(measure.re_interns, 1);
}

#[test]
fn inference_constraint_evaluation_preserves_constraint_cycle_identity() {
    assert_recursive_function_binder_cycle_preserves_identity(true);
}

#[test]
fn inference_constraint_evaluation_preserves_default_cycle_identity() {
    assert_recursive_function_binder_cycle_preserves_identity(false);
}

#[test]
fn inference_constraint_evaluation_discards_partial_function_on_pending_cycle() {
    let mut interner = Interner::with_intrinsics();
    let uppercase = interner.well_known().uppercase;
    let literal = interner.intern_literal(LiteralValue::String("constraint".into()));
    let changed_constraint =
        interner.intern_instantiation(uppercase, vec![(TypeParamId(90_150), literal)]);
    let (template, param, _) = maybe_loop_template(&mut interner);
    let string = interner.well_known().string;
    let pending_cycle = instantiate_one(&mut interner, template, param, string);
    let source = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: TypeParamId(90_151),
            constraint: Some(changed_constraint),
            default: Some(pending_cycle),
        }],
        receiver: None,
        params: Vec::new(),
        ret: interner.well_known().void,
    });
    let mut next = 90_152;

    let evaluation = evaluate_inference_constraint(&mut interner, &mut next, source);

    assert_eq!(
        evaluation.result, source,
        "a pending cycle after a changed child must not expose a partial function"
    );
    assert!(evaluation.exhausted);
    assert!(evaluation.cycle_detected);
}

#[test]
fn infer_rewrite_preserves_generic_metadata_and_signature_shape() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let outer_infer = interner.intern_infer(0);
    let u_id = TypeParamId(90_110);
    let u = interner.intern_type_param(u_id, "U");
    let u_array = interner.intern_array(u);
    let source = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: u_id,
            constraint: Some(outer_infer),
            default: Some(outer_infer),
        }],
        receiver: Some(outer_infer),
        params: vec![
            ParameterType::optional("value", u),
            ParameterType::rest("tail", u_array),
        ],
        ret: u,
    });
    let mut next = 90_111;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    let rewritten = evaluator.substitute_infers(source, &[wk.string]);
    drop(evaluator);

    let function = interner.store().function_type(rewritten).unwrap();
    assert_eq!(function.type_params.len(), 1);
    assert_eq!(function.type_params[0].id, u_id);
    assert_eq!(function.type_params[0].constraint, Some(wk.string));
    assert_eq!(function.type_params[0].default, Some(wk.string));
    assert_eq!(function.receiver, Some(wk.string));
    assert_eq!(function.params[0].ty, u);
    assert!(function.params[0].optional);
    assert_eq!(function.params[1].ty, u_array);
    assert!(function.params[1].rest);
    assert_eq!(function.ret, u);
}

#[test]
fn infer_rewrite_terminates_on_recursive_shape_without_changing_identity() {
    let mut interner = Interner::with_intrinsics();
    let recursive = interner.reserve_object();
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("self", recursive)],
            ..Default::default()
        },
    );
    let mut next = 90_120;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    assert_eq!(evaluator.substitute_infers(recursive, &[]), recursive);
    assert_eq!(evaluator.substitute_infers(recursive, &[]), recursive);
}

#[test]
fn infer_rewrite_keeps_mutual_cycle_identity() {
    let mut interner = Interner::with_intrinsics();
    let first = interner.reserve_object();
    let second = interner.reserve_object();
    interner.fill_object(
        first,
        ObjectType {
            properties: vec![prop("second", second)],
            ..Default::default()
        },
    );
    interner.fill_object(
        second,
        ObjectType {
            properties: vec![prop("first", first)],
            ..Default::default()
        },
    );
    let mut next = 90_120;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    assert_eq!(evaluator.substitute_infers(first, &[]), first);
    assert_eq!(evaluator.substitute_infers(second, &[]), second);
}

#[test]
fn infer_rewrite_handles_10k_acyclic_generic_metadata_depth() {
    const DEPTH: u32 = 10_000;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut source = interner.intern_infer(0);
    for index in 0..DEPTH {
        source = interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id: TypeParamId(95_000 + index),
                constraint: Some(source),
                default: None,
            }],
            receiver: None,
            params: Vec::new(),
            ret: wk.void,
        });
    }

    let mut next = 90_120;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    let rewritten = evaluator.substitute_infers(source, &[wk.string]);
    assert!(!evaluator.exhausted);
    drop(evaluator);

    let mut current = rewritten;
    for index in (0..DEPTH).rev() {
        let function = interner
            .store()
            .function_type(current)
            .expect("the deep metadata chain must remain a function");
        assert_eq!(function.type_params[0].id, TypeParamId(95_000 + index));
        current = function.type_params[0]
            .constraint
            .expect("the deep metadata chain must retain every constraint");
    }
    assert_eq!(current, wk.string);
}

#[test]
fn infer_rewrite_keeps_recursive_infer_graph_identity_in_both_traversal_orders() {
    let mut interner = Interner::with_intrinsics();
    let outer_infer = interner.intern_infer(0);
    let infer_before_back_edge = interner.reserve_object();
    interner.fill_object(
        infer_before_back_edge,
        ObjectType {
            properties: vec![
                prop("a_value", outer_infer),
                prop("z_self", infer_before_back_edge),
            ],
            ..Default::default()
        },
    );
    let back_edge_before_infer = interner.reserve_object();
    interner.fill_object(
        back_edge_before_infer,
        ObjectType {
            properties: vec![
                prop("a_self", back_edge_before_infer),
                prop("z_value", outer_infer),
            ],
            ..Default::default()
        },
    );
    let fresh = interner.well_known().string;
    let mut next = 90_121;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    assert_eq!(
        evaluator.substitute_infers(infer_before_back_edge, &[fresh]),
        infer_before_back_edge,
        "an infer visited before a recursive back-edge must not expose a partial clone"
    );
    assert_eq!(
        evaluator.substitute_infers(back_edge_before_infer, &[fresh]),
        back_edge_before_infer,
        "a recursive back-edge visited before an infer must not expose a partial clone"
    );
}

#[test]
fn infer_rewrite_keeps_recursive_child_but_rewrites_acyclic_sibling() {
    let mut interner = Interner::with_intrinsics();
    let recursive = interner.reserve_object();
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("self", recursive)],
            ..Default::default()
        },
    );
    let outer_infer = interner.intern_infer(0);
    let shared_acyclic = interner.intern_object(ObjectType {
        properties: vec![prop("value", outer_infer)],
        ..Default::default()
    });
    let source = interner.intern_object(ObjectType {
        properties: vec![
            prop("first", shared_acyclic),
            prop("recursive", recursive),
            prop("second", shared_acyclic),
        ],
        ..Default::default()
    });
    let fresh = interner.well_known().string;
    let mut next = 90_122;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    let first = evaluator.substitute_infers(source, &[fresh]);
    let second = evaluator.substitute_infers(source, &[fresh]);
    drop(evaluator);

    assert_ne!(first, source, "the acyclic sibling must be freshened");
    assert_eq!(
        first, second,
        "completed acyclic output keeps interned identity"
    );
    let rewritten = interner.store().object_type(first).unwrap();
    assert_eq!(rewritten.property("recursive").unwrap().ty, recursive);
    let first_child = rewritten.property("first").unwrap().ty;
    assert_ne!(first_child, shared_acyclic);
    assert_eq!(first_child, rewritten.property("second").unwrap().ty);
    assert_eq!(
        interner
            .store()
            .object_type(first_child)
            .unwrap()
            .property("value")
            .unwrap()
            .ty,
        fresh
    );
}

#[test]
fn measure_infer_rewrite_counts_shared_dag_and_cycle_sibling() {
    let mut interner = Interner::with_intrinsics();
    let outer_infer = interner.intern_infer(0);
    let shared = interner.intern_object(ObjectType {
        properties: vec![prop("value", outer_infer)],
        ..Default::default()
    });
    let dag = interner.intern_tuple(vec![shared, shared]);
    let fresh = interner.well_known().string;
    let recursive = interner.reserve_object();
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("self", recursive), prop("value", outer_infer)],
            ..Default::default()
        },
    );
    let mut next = 90_123;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    super::instantiation::reset_infer_rewrite_measure();
    let rewritten = evaluator.substitute_infers(dag, &[fresh]);
    assert_ne!(rewritten, dag);
    assert_eq!(
        super::instantiation::infer_rewrite_measure(),
        super::instantiation::InferRewriteMeasure {
            top_level_runs: 1,
            visits: 4,
            memo_hits: 1,
            memo_inserts: 3,
            reentries: 0,
            tainted_identity_returns: 0,
        }
    );
    super::instantiation::reset_infer_rewrite_measure();
    assert_eq!(evaluator.substitute_infers(recursive, &[fresh]), recursive);
    assert_eq!(
        super::instantiation::infer_rewrite_measure(),
        super::instantiation::InferRewriteMeasure {
            top_level_runs: 1,
            visits: 3,
            memo_hits: 0,
            memo_inserts: 1,
            reentries: 1,
            tainted_identity_returns: 1,
        }
    );
}

#[test]
fn measure_constraint_evaluator_counts_function_metadata_fanout() {
    let mut interner = Interner::with_intrinsics();
    let uppercase = interner.well_known().uppercase;
    let literal = interner.intern_literal(LiteralValue::String("fanout".into()));
    let upper = interner.intern_instantiation(uppercase, vec![(TypeParamId(90_160), literal)]);
    let source = interner.intern_function(FunctionType {
        type_params: vec![
            GenericTypeParam {
                id: TypeParamId(90_161),
                constraint: Some(upper),
                default: Some(upper),
            },
            GenericTypeParam {
                id: TypeParamId(90_162),
                constraint: Some(upper),
                default: Some(upper),
            },
        ],
        receiver: Some(upper),
        params: Vec::new(),
        ret: interner.well_known().void,
    });
    let mut next = 90_163;

    super::extends::reset_constraint_eval_measure();
    let evaluation = evaluate_inference_constraint(&mut interner, &mut next, source);

    assert_ne!(evaluation.result, source);
    let measure = super::extends::constraint_eval_measure();
    assert_eq!(measure.structural_entries, 1);
    assert_eq!(measure.structural_reentries, 0);
    assert_eq!(measure.metadata_children, 4);
    assert_eq!(measure.signature_children, 2);
    assert_eq!(measure.pending_calls, 5);
    assert_eq!(measure.tainted_identity_returns, 0);
    assert_eq!(measure.exhausted_identity_returns, 0);
    assert_eq!(measure.re_interns, 1);
}

#[test]
#[ignore = "WU4 release measurement; run explicitly with --ignored --nocapture"]
fn measure_infer_rewrite_hotpaths_release() {
    for count in [10_000, 100_000] {
        let mut samples = Vec::new();
        for _ in 0..5 {
            let mut interner = Interner::with_intrinsics();
            let infer = interner.intern_infer(0);
            let shared = interner.intern_object(ObjectType {
                properties: vec![prop("value", infer)],
                ..Default::default()
            });
            let root = interner.intern_tuple(vec![shared; count]);
            let fresh = interner.well_known().string;
            let mut next = 90_170;
            let mut memo = FxHashMap::default();
            let mut evaluator =
                ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);
            super::instantiation::reset_infer_rewrite_measure();
            let started = Instant::now();
            assert_ne!(evaluator.substitute_infers(root, &[fresh]), root);
            let elapsed = started.elapsed();
            let measure = super::instantiation::infer_rewrite_measure();
            assert_eq!(measure.visits, count as u64 + 2);
            assert_eq!(measure.memo_hits, count as u64 - 1);
            assert_eq!(measure.memo_inserts, 3);
            samples.push(elapsed);
        }
        samples.sort_unstable();
        println!(
            "infer rewrite count={count} samples={samples:?} median={:?} range={:?}..{:?}",
            samples[2], samples[0], samples[4]
        );
    }
}

#[test]
#[ignore = "WU4 release measurement; run explicitly with --ignored --nocapture"]
fn measure_constraint_evaluator_hotpaths_release() {
    for count in [10_000, 100_000] {
        let mut samples = Vec::new();
        for _ in 0..5 {
            let mut interner = Interner::with_intrinsics();
            let uppercase = interner.well_known().uppercase;
            let literal = interner.intern_literal(LiteralValue::String("fanout".into()));
            let upper =
                interner.intern_instantiation(uppercase, vec![(TypeParamId(90_180), literal)]);
            let type_params = (0..count)
                .map(|index| GenericTypeParam {
                    id: TypeParamId(91_000 + index as u32),
                    constraint: Some(upper),
                    default: Some(upper),
                })
                .collect();
            let source = interner.intern_function(FunctionType {
                type_params,
                receiver: Some(upper),
                params: Vec::new(),
                ret: interner.well_known().void,
            });
            let mut next = 92_000;
            super::extends::reset_constraint_eval_measure();
            let started = Instant::now();
            let evaluation = evaluate_inference_constraint(&mut interner, &mut next, source);
            let elapsed = started.elapsed();
            assert_ne!(evaluation.result, source);
            let measure = super::extends::constraint_eval_measure();
            assert_eq!(measure.metadata_children, (count * 2) as u64);
            assert_eq!(measure.signature_children, 2);
            assert_eq!(measure.pending_calls, (count * 2 + 1) as u64);
            assert_eq!(measure.evaluate_calls, (count * 4 + 4) as u64);
            assert_eq!(measure.re_interns, 1);
            samples.push(elapsed);
        }
        samples.sort_unstable();
        println!(
            "constraint evaluator count={count} samples={samples:?} median={:?} range={:?}..{:?}",
            samples[2], samples[0], samples[4]
        );
    }
}

#[test]
fn omit_this_parameter_preserves_optional_rest_and_default_shape() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let boolean_array = interner.intern_array(wk.boolean);
    let source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wk.string),
        params: vec![
            ParameterType::defaulted("defaulted", wk.number),
            ParameterType::rest("tail", boolean_array),
        ],
        ret: wk.void,
    });
    let result = omit_this_parameter(&mut interner, source);
    let mut next = 1;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);
    let transformed = evaluator.evaluate(result);
    let function = interner.store().function_type(transformed).unwrap();

    assert_eq!(function.receiver, None);
    assert!(function.params[0].optional);
    assert!(function.params[0].has_default);
    assert!(function.params[1].rest);
}

#[test]
fn omit_this_parameter_uses_last_overload_and_preserves_no_receiver_identity() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wk.string),
        params: vec![ParameterType::required("first", wk.string)],
        ret: wk.number,
    });
    let last = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wk.number),
        params: vec![ParameterType::optional("last", wk.number)],
        ret: wk.string,
    });
    let overload = interner.intern_object(ObjectType {
        call_signatures: vec![first, last],
        ..Default::default()
    });
    let receiverless = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.number,
    });
    let transformed = omit_this_parameter(&mut interner, overload);
    let preserved = omit_this_parameter(&mut interner, receiverless);
    let mut next = 1;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    let transformed = evaluator.evaluate(transformed);
    assert_eq!(evaluator.evaluate(preserved), receiverless);
    let function = interner.store().function_type(transformed).unwrap();
    assert_eq!(function.receiver, None);
    assert_eq!(function.ret, wk.string);
    assert!(function.params[0].optional);
}

#[test]
fn omit_this_parameter_erases_generic_binders_and_keeps_open_guard_deferred() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(90_101), "T");
    let generic = interner.intern_function(FunctionType {
        type_params: vec![crate::types::repr::GenericTypeParam {
            id: TypeParamId(90_101),
            constraint: Some(wk.number),
            default: Some(wk.string),
        }],
        receiver: Some(t),
        params: vec![ParameterType::required("value", t)],
        ret: t,
    });
    let open = interner.intern_type_param(TypeParamId(90_102), "U");
    let transformed = omit_this_parameter(&mut interner, generic);
    let deferred = omit_this_parameter(&mut interner, open);
    let mut next = 1;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    let transformed = evaluator.evaluate(transformed);
    assert_eq!(evaluator.evaluate(deferred), deferred);
    drop(evaluator);
    let function = interner.store().function_type(transformed).unwrap();
    assert!(function.type_params.is_empty());
    assert_eq!(function.receiver, None);
    assert_eq!(function.params[0].ty, wk.number);
    assert_eq!(function.ret, wk.number);
}

#[test]
fn omit_this_parameter_keeps_generic_receiver_when_unknown_satisfies_effective_receiver() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(90_103), "T");
    let generic = interner.intern_function(FunctionType {
        type_params: vec![crate::types::repr::GenericTypeParam {
            id: TypeParamId(90_103),
            constraint: None,
            default: Some(wk.number),
        }],
        receiver: Some(t),
        params: vec![ParameterType::required("value", t)],
        ret: t,
    });
    let marker = omit_this_parameter(&mut interner, generic);
    let mut next = 1;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    assert_eq!(evaluator.evaluate(marker), generic);
}

#[test]
fn omit_this_parameter_keeps_mixed_receiver_union_for_unknown_guard() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let generic_param = TypeParamId(91_001);
    let generic_receiver = interner.intern_type_param(generic_param, "T");
    let with_unknown_receiver = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: generic_param,
            constraint: None,
            default: None,
        }],
        receiver: Some(generic_receiver),
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.void,
    });
    let with_concrete_receiver = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wk.string),
        params: vec![ParameterType::required("value", wk.boolean)],
        ret: wk.void,
    });
    let union = interner.union(vec![with_unknown_receiver, with_concrete_receiver]);
    let marker = omit_this_parameter(&mut interner, union);
    let mut next = 1;
    let mut memo = FxHashMap::default();
    let mut evaluator =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    assert_eq!(evaluator.evaluate(marker), union);
}

#[test]
fn deferred_keyof_walks_function_receivers() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let receiver = interner.intern_type_param(TypeParamId(90_001), "T");
    let function = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(receiver),
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.void,
    });
    let deferred = interner.intern_keyof(function);

    assert!(contains_deferred_keyof(interner.store(), deferred));
}

fn instantiate_one(
    interner: &mut Interner,
    template: TypeId,
    param: TypeParamId,
    argument: TypeId,
) -> TypeId {
    interner.intern_instantiation(template, vec![(param, argument)])
}

fn substituted_conditional(
    interner: &mut Interner,
    template: TypeId,
    param: TypeParamId,
    argument: TypeId,
) -> TypeId {
    let mut map = FxHashMap::default();
    map.insert(param, argument);
    substitute(interner, template, &map)
}

fn maybe_loop_template(interner: &mut Interner) -> (TypeId, TypeParamId, TypeId) {
    let wk = interner.well_known();
    let param = TypeParamId(0);
    let t = interner.intern_type_param(param, "T");
    let template = interner.reserve_conditional();
    let recur = instantiate_one(interner, template, param, t);
    let done = interner.intern_literal(LiteralValue::String("done".into()));
    interner.fill_conditional(
        template,
        ConditionalType {
            check: t,
            extends_ty: wk.string,
            true_branch: recur,
            false_branch: done,
            infer_count: 0,
            distributive: true,
            poisoned: false,
        },
    );
    (template, param, done)
}

/// An in-flight cycle is reported separately from budget exhaustion and must taint every
/// active memo frame, while a terminating sibling of the same template remains cacheable.
#[test]
fn instantiation_cycle_taints_ancestors_without_poisoning_terminating_sibling() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (template, param, done) = maybe_loop_template(&mut interner);
    let cycle = instantiate_one(&mut interner, template, param, wk.string);
    let cycle_conditional = substituted_conditional(&mut interner, template, param, wk.string);
    let terminating = instantiate_one(&mut interner, template, param, wk.number);
    let terminating_conditional =
        substituted_conditional(&mut interner, template, param, wk.number);
    let mut next = 1;
    let mut memo = FxHashMap::default();
    let mut ev =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    assert_eq!(
        ev.evaluate(cycle),
        wk.any,
        "the union assembly preserves the existing error-to-any cascade behavior"
    );
    assert!(ev.cycle_detected, "the in-flight re-entry is observable");
    assert!(!ev.exhausted, "a cycle is not a budget exhaustion");
    assert!(
        !ev.memo.contains_key(&cycle) && !ev.memo.contains_key(&cycle_conditional),
        "neither the recursive root nor its active conditional ancestor may memoize error"
    );
    assert!(ev.in_flight.is_empty(), "every active frame must drain");
    assert!(
        ev.cycle_tainted.is_empty(),
        "SetMemo consumes every frame taint"
    );

    assert_eq!(ev.evaluate(terminating), done);
    assert!(
        !ev.cycle_detected,
        "a later independent root must not inherit the prior cycle status"
    );
    assert_eq!(ev.memo.get(&terminating), Some(&done));
    assert_eq!(ev.memo.get(&terminating_conditional), Some(&done));
}

/// Reversing the demand order cannot make the terminating instantiation depend on the
/// later cycle, and repeated lookup still sees its durable result.
#[test]
fn terminating_instantiation_stays_memoized_before_a_later_cycle() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (template, param, done) = maybe_loop_template(&mut interner);
    let cycle = instantiate_one(&mut interner, template, param, wk.string);
    let terminating = instantiate_one(&mut interner, template, param, wk.number);
    let mut next = 1;
    let mut memo = FxHashMap::default();

    {
        let mut ev =
            ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);
        assert_eq!(ev.evaluate(terminating), done);
        assert!(!ev.cycle_detected);
        assert_eq!(ev.memo.get(&terminating), Some(&done));
    }
    {
        let mut ev =
            ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);
        assert_eq!(ev.evaluate(cycle), wk.any);
        assert!(ev.cycle_detected);
        assert!(!ev.memo.contains_key(&cycle));
    }
    {
        let mut ev =
            ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);
        assert_eq!(ev.evaluate(terminating), done);
        assert!(!ev.cycle_detected);
    }
}

/// Mutual aliases must taint both templates' active roots rather than memoizing either
/// error-derived result.
#[test]
fn mutual_instantiation_cycle_taints_every_active_template() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let ping_param = TypeParamId(10);
    let pong_param = TypeParamId(11);
    let ping_t = interner.intern_type_param(ping_param, "PingT");
    let pong_t = interner.intern_type_param(pong_param, "PongT");
    let ping = interner.reserve_conditional();
    let pong = interner.reserve_conditional();
    let ping_to_pong = instantiate_one(&mut interner, pong, pong_param, ping_t);
    let pong_to_ping = instantiate_one(&mut interner, ping, ping_param, pong_t);
    interner.fill_conditional(
        ping,
        ConditionalType {
            check: ping_t,
            extends_ty: wk.string,
            true_branch: ping_to_pong,
            false_branch: wk.never,
            infer_count: 0,
            distributive: true,
            poisoned: false,
        },
    );
    interner.fill_conditional(
        pong,
        ConditionalType {
            check: pong_t,
            extends_ty: wk.string,
            true_branch: pong_to_ping,
            false_branch: wk.never,
            infer_count: 0,
            distributive: true,
            poisoned: false,
        },
    );
    let root = instantiate_one(&mut interner, ping, ping_param, wk.string);
    let ping_conditional = substituted_conditional(&mut interner, ping, ping_param, wk.string);
    let pong_conditional = substituted_conditional(&mut interner, pong, pong_param, wk.string);
    let mut next = 12;
    let mut memo = FxHashMap::default();
    let mut ev =
        ConditionalEvaluator::new(&mut interner, &mut next, &mut memo, DEFAULT_STEP_BUDGET);

    assert_eq!(ev.evaluate(root), wk.any);
    assert!(ev.cycle_detected);
    assert!(!ev.exhausted);
    assert!(
        !ev.memo.contains_key(&root)
            && !ev.memo.contains_key(&ping_conditional)
            && !ev.memo.contains_key(&pong_conditional),
        "all active mutual-cycle frames must remain absent from the durable memo"
    );
    assert!(ev.in_flight.is_empty());
    assert!(ev.cycle_tainted.is_empty());
}

/// Witness (architecture §7.2 item b): a ~10 000-deep nested `{ v: … }` type
/// evaluated by an `Unwrap`-style recursive conditional resolves to the innermost
/// type **without overflowing the native stack**. Built programmatically via the
/// interner (a parsed fixture would stress the parser instead), and run with a raised
/// budget so the work-stack — not the step budget — is what proves termination.
#[test]
fn deep_recursive_unwrap_does_not_overflow_the_native_stack() {
    const DEPTH: usize = 10_000;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // The recursive template: `type Unwrap<T> = T extends { v: infer U } ? Unwrap<U> : T`.
    // T = TypeParamId(0); the true branch is a lazy self-instantiation carrying the
    // infer binder as the argument.
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let infer0 = interner.intern_infer(0);
    let extends = interner.intern_object(ObjectType {
        properties: vec![prop("v", infer0)],
        ..Default::default()
    });
    let template = interner.reserve_conditional();
    let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), infer0)]);
    interner.fill_conditional(
        template,
        ConditionalType {
            check: t,
            extends_ty: extends,
            true_branch: recur,
            false_branch: t,
            infer_count: 1,
            distributive: true,
            poisoned: false,
        },
    );

    // Build the 10 000-deep check type `{ v: { v: … { v: number } … } }`, innermost
    // out (iteratively — no recursion here either).
    let mut deep = wk.number;
    for _ in 0..DEPTH {
        deep = interner.intern_object(ObjectType {
            properties: vec![prop("v", deep)],
            ..Default::default()
        });
    }

    // `Unwrap<deep>` — evaluate with a budget above the depth so termination is the
    // work-stack's doing, not the budget's.
    let root = interner.intern_instantiation(template, vec![(TypeParamId(0), deep)]);
    let mut next_type_param: u32 = 1;
    let mut memo = FxHashMap::default();
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        (DEPTH as u32) + 1000,
    );
    let result = ev.evaluate(root);
    assert!(!ev.exhausted, "the raised budget must not be exhausted");
    assert_eq!(
        result, wk.number,
        "Unwrap fully descends to the innermost `number`"
    );
}

/// A terminating shallow `Unwrap` resolves, and its memo is populated (a repeat
/// evaluation is a cache hit).
#[test]
fn shallow_unwrap_resolves_and_memoizes() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let infer0 = interner.intern_infer(0);
    let extends = interner.intern_object(ObjectType {
        properties: vec![prop("v", infer0)],
        ..Default::default()
    });
    let template = interner.reserve_conditional();
    let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), infer0)]);
    interner.fill_conditional(
        template,
        ConditionalType {
            check: t,
            extends_ty: extends,
            true_branch: recur,
            false_branch: t,
            infer_count: 1,
            distributive: true,
            poisoned: false,
        },
    );
    // `{ v: { v: number } }`.
    let inner = interner.intern_object(ObjectType {
        properties: vec![prop("v", wk.number)],
        ..Default::default()
    });
    let outer = interner.intern_object(ObjectType {
        properties: vec![prop("v", inner)],
        ..Default::default()
    });
    let root = interner.intern_instantiation(template, vec![(TypeParamId(0), outer)]);

    let mut next_type_param: u32 = 1;
    let mut memo = FxHashMap::default();
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        DEFAULT_STEP_BUDGET,
    );
    assert_eq!(ev.evaluate(root), wk.number);
    assert!(memo.contains_key(&root), "the root evaluation is memoized");
}

/// A **poisoned** conditional (cross-binder nested infer — backlog 26 stopgap) NEVER
/// evaluates, even with a fully concrete check: the evaluator returns the node
/// as-is (both directly and through an instantiation of a poisoned template), so it
/// stays a deferred node under the conservative relation rules.
#[test]
fn poisoned_conditional_never_evaluates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let s_lit = interner.intern_literal(LiteralValue::String("s".into()));
    let n_lit = interner.intern_literal(LiteralValue::String("n".into()));

    // A concrete-check poisoned node: `string extends string ? "s" : "n"` — would
    // resolve to "s" if evaluation were allowed.
    let poisoned = interner.intern_conditional(ConditionalType {
        check: wk.string,
        extends_ty: wk.string,
        true_branch: s_lit,
        false_branch: n_lit,
        infer_count: 0,
        distributive: false,
        poisoned: true,
    });
    let mut next_type_param: u32 = 0;
    let mut memo = FxHashMap::default();
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        DEFAULT_STEP_BUDGET,
    );
    assert_eq!(
        ev.evaluate(poisoned),
        poisoned,
        "a poisoned conditional must be returned as-is, never evaluated"
    );
    assert!(!ev.exhausted);
    drop(ev);

    // Through an instantiation of a poisoned distributive template: the expansion
    // must NOT distribute (a poisoned base is treated as non-distributive) — the
    // result is the substituted, still-poisoned node, unevaluated.
    let t = interner.intern_type_param(TypeParamId(900), "T");
    let template = interner.intern_conditional(ConditionalType {
        check: t,
        extends_ty: wk.string,
        true_branch: s_lit,
        false_branch: n_lit,
        infer_count: 0,
        distributive: true,
        poisoned: true,
    });
    let union = interner.union(vec![wk.string, wk.number]);
    let root = interner.intern_instantiation(template, vec![(TypeParamId(900), union)]);
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        DEFAULT_STEP_BUDGET,
    );
    let result = ev.evaluate(root);
    drop(ev);
    let out = interner
        .store()
        .conditional_type(result)
        .copied()
        .expect("the instantiation must resolve to a (deferred) conditional node");
    assert!(out.poisoned, "the substituted node stays poisoned");
    assert_eq!(out.check, union, "substituted once, never distributed");
}

/// A genuinely-infinite alias (`type Inf<T> = T extends {} ? Inf<{ v: T }> : never`)
/// trips the step budget rather than looping, setting `exhausted`.
#[test]
fn runaway_growth_exhausts_the_budget() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let empty = interner.intern_object(ObjectType::default());
    let template = interner.reserve_conditional();
    // The true branch wraps the check type: `Inf<{ v: T }>`.
    let wrapped = interner.intern_object(ObjectType {
        properties: vec![prop("v", t)],
        ..Default::default()
    });
    let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), wrapped)]);
    interner.fill_conditional(
        template,
        ConditionalType {
            check: t,
            extends_ty: empty,
            true_branch: recur,
            false_branch: wk.never,
            infer_count: 0,
            distributive: true,
            poisoned: false,
        },
    );
    let root = interner.intern_instantiation(template, vec![(TypeParamId(0), empty)]);

    let mut next_type_param: u32 = 1;
    let mut memo = FxHashMap::default();
    let mut ev = ConditionalEvaluator::new(
        &mut interner,
        &mut next_type_param,
        &mut memo,
        DEFAULT_STEP_BUDGET,
    );
    let _ = ev.evaluate(root);
    assert!(ev.exhausted, "a runaway alias must exhaust the step budget");
}

/// M26 — a homomorphic identity mapped type `{ [K in keyof T]: T[K] }` over a
/// concrete source evaluates to the source's shape (per-property `T[K]` = the source
/// property's type), and its result is memoized.
fn eval(
    interner: &mut Interner,
    next: &mut u32,
    memo: &mut FxHashMap<TypeId, TypeId>,
    ty: TypeId,
) -> TypeId {
    let mut ev = ConditionalEvaluator::new(interner, next, memo, DEFAULT_STEP_BUDGET);
    ev.evaluate(ty)
}

#[test]
fn mapped_identity_evaluates_to_source_shape() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    // Concrete source `{ a: number; b: string }`.
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    let placeholder = interner.intern_mapped_value();
    let ident = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: source,
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, ident);
    assert_eq!(
        result, source,
        "an identity map over a concrete source yields the source shape"
    );
    assert!(
        memo.contains_key(&ident),
        "the mapped evaluation is memoized"
    );
}

/// M26 — modifier arithmetic: `readonly` (Add) sets every result property readonly;
/// `?` (Add) makes every property optional; a `MappedValue | null` template unions
/// `null` into each value.
#[test]
fn mapped_modifiers_and_value_transform_apply() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    let placeholder = interner.intern_mapped_value();
    // `{ readonly [K in keyof T]?: T[K] | null }`.
    let value_template = interner.union(vec![placeholder, wk.null]);
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: source,
        value_template,
        modifiers_source: None,
        optional_modifier: ModifierOp::Add,
        readonly_modifier: ModifierOp::Add,
    });
    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, mapped);

    let a = interner
        .store()
        .object_type(result)
        .and_then(|o| o.property("a"))
        .expect("property a survives")
        .clone();
    assert!(a.readonly, "readonly (Add) makes the property readonly");
    assert!(a.optional, "? (Add) makes the property optional");
    // Effective type is `number | null | undefined` (value `number | null`, plus the
    // optional `| undefined` baked in).
    let expected = interner.union(vec![wk.number, wk.null, wk.undefined]);
    assert_eq!(
        a.ty, expected,
        "value template `T[K] | null` + optional `| undefined`"
    );
}

/// M27 — template construction: all-literal holes **collapse** (`` `a-${"b"}` `` →
/// `"a-b"`), a union hole distributes to the cartesian-product union, `boolean`
/// expands to `"false" | "true"`, a `never` hole short-circuits to `never`, and a
/// number literal stringifies.
#[test]
fn template_construction_collapses_and_distributes() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut next = 0u32;
    let mut memo = FxHashMap::default();

    let s = |interner: &mut Interner, v: &str| {
        interner.intern_literal(LiteralValue::String(v.to_string()))
    };
    let template = |interner: &mut Interner, texts: &[&str], holes: Vec<TypeId>| {
        interner.intern_template(TemplateType {
            texts: texts.iter().map(|t| t.to_string()).collect(),
            holes,
        })
    };

    // `` `a-${"b"}` `` → "a-b".
    let b = s(&mut interner, "b");
    let one = template(&mut interner, &["a-", ""], vec![b]);
    let expect = s(&mut interner, "a-b");
    assert_eq!(eval(&mut interner, &mut next, &mut memo, one), expect);

    // `` `${"a"|"b"}-${"1"|"2"}` `` → "a-1" | "a-2" | "b-1" | "b-2".
    let a = s(&mut interner, "a");
    let b = s(&mut interner, "b");
    let d1 = s(&mut interner, "1");
    let d2 = s(&mut interner, "2");
    let ab = interner.union(vec![a, b]);
    let d12 = interner.union(vec![d1, d2]);
    let two = template(&mut interner, &["", "-", ""], vec![ab, d12]);
    let members: Vec<TypeId> = ["a-1", "a-2", "b-1", "b-2"]
        .into_iter()
        .map(|v| s(&mut interner, v))
        .collect();
    let expect = interner.union(members);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, two), expect);

    // `` `is:${boolean}` `` → "is:false" | "is:true".
    let bh = template(&mut interner, &["is:", ""], vec![wk.boolean]);
    let f = s(&mut interner, "is:false");
    let t = s(&mut interner, "is:true");
    let expect = interner.union(vec![f, t]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, bh), expect);

    // `` `x${never}` `` → never.
    let nh = template(&mut interner, &["x", ""], vec![wk.never]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, nh), wk.never);

    // `` `v${1|2}` `` → "v1" | "v2" (number stringify).
    let n1 = interner.intern_literal(LiteralValue::Number(1.0));
    let n2 = interner.intern_literal(LiteralValue::Number(2.0));
    let n12 = interner.union(vec![n1, n2]);
    let ver = template(&mut interner, &["v", ""], vec![n12]);
    let v1 = s(&mut interner, "v1");
    let v2 = s(&mut interner, "v2");
    let expect = interner.union(vec![v1, v2]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, ver), expect);
}

/// M27 — a template with a **non-literal** hole (a `string` intrinsic, or a free
/// declaration type parameter) stays a **symbolic** node; an **error-typed** hole
/// degrades the whole template to the error type (M22 cascade suppression).
#[test]
fn template_construction_keeps_symbolic_and_suppresses_error() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut next = 1u32;
    let mut memo = FxHashMap::default();

    let template = |interner: &mut Interner, hole: TypeId| {
        interner.intern_template(TemplateType {
            texts: vec!["tag:".to_string(), String::new()],
            holes: vec![hole],
        })
    };

    // `string` hole → symbolic pattern (unchanged). It memoizes to itself via the
    // SetMemo discipline (backlog 55) — idempotent, mirroring a conditional whose
    // concrete operands stay undecidable.
    let pattern = template(&mut interner, wk.string);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, pattern),
        pattern,
        "a `${{string}}` pattern stays symbolic"
    );
    assert_eq!(
        memo.get(&pattern).copied(),
        Some(pattern),
        "a symbolic template memoizes to itself (idempotent)"
    );

    // Free type parameter hole → deferred (symbolic).
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let deferred = template(&mut interner, t);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, deferred),
        deferred
    );

    // Error hole → error type (M22 cascade suppression).
    let err = template(&mut interner, wk.error);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, err),
        wk.error,
        "an error-typed hole degrades the template to the error type"
    );
    assert_eq!(
        memo.get(&err),
        Some(&wk.error),
        "an ordinary upstream error remains a cacheable cascade-suppression result"
    );
}

/// M26 — a mapped type over a **free** declaration type parameter stays deferred: the
/// evaluator returns the node unchanged (related conservatively by the M25 model),
/// and it is NOT memoized.
#[test]
fn deferred_mapped_over_free_param_is_returned_unchanged() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let placeholder = interner.intern_mapped_value();
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: t, // a free parameter → deferred
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut next = 1u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, mapped);
    assert_eq!(
        result, mapped,
        "a deferred mapped type is returned unchanged"
    );
    assert!(
        !memo.contains_key(&mapped),
        "a deferred mapped type is not memoized"
    );
}

/// M28 — a **deferred `keyof`** over a free type parameter is returned unchanged
/// (and not memoized); once its operand is concrete (an object) it resolves
/// through the SHARED keyof computation to the key-literal union; an error
/// operand degrades to the error type; a concrete-but-non-object operand (a
/// primitive after substitution) stays a deferred node — never a permissive
/// fallback.
#[test]
fn deferred_keyof_defers_and_resolves_via_shared_computation() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut next = 1u32;
    let mut memo = FxHashMap::default();

    // Free operand: unchanged, un-memoized.
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let keyof_t = interner.intern_keyof(t);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, keyof_t), keyof_t);
    assert!(
        !memo.contains_key(&keyof_t),
        "a deferred keyof is not memoized"
    );

    // Concrete object operand: the key-literal union (same as the eager path).
    let obj = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    let keyof_obj = interner.intern_keyof(obj);
    let a = interner.intern_literal(LiteralValue::String("a".into()));
    let b = interner.intern_literal(LiteralValue::String("b".into()));
    let expect = interner.union(vec![a, b]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, keyof_obj), expect);
    let eager = keyof_of_object(&mut interner, obj).expect("object operand keys");
    assert_eq!(
        eager, expect,
        "single source of truth: eager == deferred result"
    );

    // Error operand: the error type (M22 cascade suppression).
    let keyof_err = interner.intern_keyof(wk.error);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, keyof_err),
        wk.error
    );

    // Concrete non-object operand: stays deferred (conservative, not permissive).
    let keyof_num = interner.intern_keyof(wk.number);
    assert_eq!(
        eval(&mut interner, &mut next, &mut memo, keyof_num),
        keyof_num
    );
}

/// M28 string intrinsics: literals transform, unions distribute, and a
/// non-literal argument stays a symbolic instantiation.
#[test]
fn string_intrinsics_transform_distribute_and_stay_symbolic() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mut next = 1u32;
    let mut memo = FxHashMap::default();
    let s_param = TypeParamId(0);

    let lit = |interner: &mut Interner, v: &str| {
        interner.intern_literal(LiteralValue::String(v.to_string()))
    };
    let apply = |interner: &mut Interner,
                 next: &mut u32,
                 memo: &mut FxHashMap<TypeId, TypeId>,
                 base: TypeId,
                 arg: TypeId| {
        let inst = interner.intern_instantiation(base, vec![(s_param, arg)]);
        let mut ev = ConditionalEvaluator::new(interner, next, memo, DEFAULT_STEP_BUDGET);
        ev.evaluate(inst)
    };

    // Literal transforms — the four kinds.
    let abc = lit(&mut interner, "abc");
    let big = lit(&mut interner, "ABC");
    let cases = [
        (wk.uppercase, abc, "ABC"),
        (wk.lowercase, big, "abc"),
        (wk.capitalize, abc, "Abc"),
        (wk.uncapitalize, big, "aBC"),
    ];
    for (base, arg, expect) in cases {
        let expect = lit(&mut interner, expect);
        assert_eq!(
            apply(&mut interner, &mut next, &mut memo, base, arg),
            expect
        );
    }
    // The empty string is unchanged (no first char to map).
    let empty = lit(&mut interner, "");
    assert_eq!(
        apply(&mut interner, &mut next, &mut memo, wk.capitalize, empty),
        empty
    );

    // A union argument distributes per member.
    let a = lit(&mut interner, "a");
    let b = lit(&mut interner, "b");
    let ab = interner.union(vec![a, b]);
    let big_a = lit(&mut interner, "A");
    let big_b = lit(&mut interner, "B");
    let expect = interner.union(vec![big_a, big_b]);
    assert_eq!(
        apply(&mut interner, &mut next, &mut memo, wk.uppercase, ab),
        expect
    );

    // A non-literal argument stays the symbolic (identical, hash-consed) node.
    let sym = interner.intern_instantiation(wk.uppercase, vec![(s_param, wk.string)]);
    assert_eq!(eval(&mut interner, &mut next, &mut memo, sym), sym);
}

/// M28 — a non-homomorphic map with a **modifiers source** (the `Pick` shape)
/// resolves each key against the source object: the property's value type
/// replaces the placeholder and its `?` flag survives (with the M21
/// `| undefined` baked in); a key the source lacks keeps the M26 behavior
/// (error-typed value, flags absent).
#[test]
fn modifiers_source_preserves_values_and_flags() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let placeholder = interner.intern_mapped_value();

    // Source `{ a: number; b?: string }` (M21 stores b as `string | undefined`).
    let str_or_undef = interner.union(vec![wk.string, wk.undefined]);
    let mut b = prop("b", str_or_undef);
    b.optional = true;
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), b],
        ..Default::default()
    });

    // `{ [P in "a" | "b" | "q"]: T[P] }` with modifiers source = the object.
    let a_key = interner.intern_literal(LiteralValue::String("a".into()));
    let b_key = interner.intern_literal(LiteralValue::String("b".into()));
    let q_key = interner.intern_literal(LiteralValue::String("q".into()));
    let keys = interner.union(vec![a_key, b_key, q_key]);
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: false,
        key_source: keys,
        value_template: placeholder,
        modifiers_source: Some(source),
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, mapped);

    let props: Vec<PropertyType> = interner
        .store()
        .object_type(result)
        .expect("result is an object")
        .properties
        .clone();
    let get = |name: &str| {
        props
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("property {name} present"))
            .clone()
    };
    assert_eq!(get("a").ty, wk.number, "picked value type preserved");
    assert!(!get("a").optional);
    assert!(get("b").optional, "picked optionality preserved");
    assert_eq!(get("b").ty, str_or_undef);
    assert!(!get("q").optional, "a missing key keeps the M26 defaults");
    assert_eq!(get("q").ty, wk.error);
}

/// M26 — `-?` Required semantics (probed tsc 6.0.3, leader-arbitrated): over an
/// **optional** source member, `undefined` is stripped from the **evaluated** value
/// type — including a template-re-added `| undefined`; a result that is EXACTLY
/// `undefined` maps to `never`; a **non-optional** source member never strips
/// (template-added `undefined` is kept).
#[test]
fn required_strips_undefined_from_optional_source_values() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let placeholder = interner.intern_mapped_value();

    // Source `{ a: string | undefined; b?: string; u?: undefined }` — M21 stores an
    // optional member's effective type with `| undefined` baked in.
    let str_or_undef = interner.union(vec![wk.string, wk.undefined]);
    let mut b = prop("b", str_or_undef);
    b.optional = true;
    let mut u = prop("u", wk.undefined);
    u.optional = true;
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", str_or_undef), b, u],
        ..Default::default()
    });

    // `{ [K in keyof T]-?: T[K] | undefined }` — the template RE-ADDS `undefined`,
    // distinguishing a result-level strip from a source-level one.
    let template = interner.union(vec![placeholder, wk.undefined]);
    let req = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: source,
        value_template: template,
        modifiers_source: None,
        optional_modifier: ModifierOp::Remove,
        readonly_modifier: ModifierOp::Keep,
    });
    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    let result = eval(&mut interner, &mut next, &mut memo, req);

    let props: Vec<PropertyType> = interner
        .store()
        .object_type(result)
        .expect("result is an object")
        .properties
        .clone();
    let get = |name: &str| {
        props
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("property {name} present"))
            .clone()
    };
    // Optional source `b`: undefined stripped from the whole RESULT (even the
    // template-added one) → exactly `string`, and required.
    let b_out = get("b");
    assert!(!b_out.optional, "-? clears optionality");
    assert_eq!(
        b_out.ty, wk.string,
        "undefined stripped from the evaluated value"
    );
    // Exactly-undefined optional source `u`: maps to `never` (leader-arbitrated
    // tsc probe m26_arb.ts — filtering `undefined` by not-undefined leaves nothing).
    assert_eq!(
        get("u").ty,
        wk.never,
        "an exactly-undefined value maps to never"
    );
    // NON-optional source `a`: never strips — keeps `string | undefined`.
    assert_eq!(
        get("a").ty,
        str_or_undef,
        "a non-optional source member keeps its undefined"
    );
}

/// WU3 — `keyof` over index signatures (single source of truth [`keyof_of_object`]):
/// a **string** index sig covers numeric keys too (`string | number`); a **number**
/// index sig covers only `number`; a mixed object unions its named keys with both.
#[test]
fn keyof_over_index_signatures_covers_number_for_string_index() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let union_of = |interner: &mut Interner, members: Vec<TypeId>| interner.union(members);

    // `{ [k: string]: number }` → `string | number`.
    let string_dict = interner.intern_object(ObjectType {
        string_index: Some(wk.number),
        ..Default::default()
    });
    let expect_str = union_of(&mut interner, vec![wk.string, wk.number]);
    assert_eq!(
        keyof_of_object(&mut interner, string_dict).expect("keys"),
        expect_str,
        "keyof a string index signature is string | number"
    );

    // `{ [i: number]: string }` → `number`.
    let number_dict = interner.intern_object(ObjectType {
        number_index: Some(wk.string),
        ..Default::default()
    });
    assert_eq!(
        keyof_of_object(&mut interner, number_dict).expect("keys"),
        wk.number,
        "keyof a number index signature is number"
    );

    // `{ a: boolean; [k: string]: number }` → `"a" | string | number`.
    let mixed = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.boolean)],
        string_index: Some(wk.number),
        ..Default::default()
    });
    let a = interner.intern_literal(LiteralValue::String("a".into()));
    let expect_mixed = union_of(&mut interner, vec![a, wk.string, wk.number]);
    assert_eq!(
        keyof_of_object(&mut interner, mixed).expect("keys"),
        expect_mixed,
        "keyof unions named keys with string | number"
    );
}

/// WU3 — a mapped type whose value template is a genuinely **cyclic** object (a
/// recursive alias built via reserve/fill) evaluates WITHOUT overflowing the native
/// stack, preserves the recursive identity, and yields the same `TypeId` on repeated /
/// reordered evaluation (the `replace_mapped_value` cycle guard is deterministic and
/// never poisons the durable memo).
#[test]
fn recursive_mapped_value_terminates_with_stable_identity() {
    use crate::types::repr::{MappedType, ModifierOp};
    let mut interner = Interner::with_intrinsics();

    // `type Rec = { self: Rec }` — a real TypeId cycle via reserve/fill.
    let rec = interner.reserve_object();
    interner.fill_object(
        rec,
        ObjectType {
            properties: vec![prop("self", rec)],
            ..Default::default()
        },
    );

    // Source `{ a: 1 }` and the homomorphic map `{ [K in keyof source]: Rec }` — the
    // value template `Rec` carries no `T[K]` placeholder, so each key maps to `Rec`.
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", one)],
        ..Default::default()
    });
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: source,
        value_template: rec,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });

    let mut next = 0u32;
    let mut memo = FxHashMap::default();
    // Completing at all proves the recursion terminates (no native stack overflow).
    let first = eval(&mut interner, &mut next, &mut memo, mapped);

    // The result is `{ a: Rec }` — the cyclic value's identity is preserved.
    let result_obj = interner
        .store()
        .object_type(first)
        .expect("mapped result is an object");
    assert_eq!(result_obj.properties.len(), 1);
    assert_eq!(result_obj.properties[0].name, "a");
    assert_eq!(
        result_obj.properties[0].ty, rec,
        "the recursive value template keeps its identity"
    );

    // Repeated evaluation (fresh memo) yields the SAME id — stable and deterministic.
    let mut memo2 = FxHashMap::default();
    let second = eval(&mut interner, &mut next, &mut memo2, mapped);
    assert_eq!(first, second, "repeated recursive evaluation is stable");
}

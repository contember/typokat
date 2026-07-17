//! Acceptance spec for a per-`Substitution` completed-result memo.
//!
//! Activate from `substitute/mod.rs` only with the implementation. A memo key is
//! `(TypeId, canonical blocked ∩ substitution-map keys)` and lives for one
//! `Substitution` run. Every completed non-reentry `apply` result, including an
//! intrinsic or type-parameter leaf, is inserted by the wrapper-level memo rule.
//! A recursive re-entry taints every active frame whose start cycle epoch changed;
//! neither the reentry nor any of those completed ancestors may be memoized.

use super::*;
use crate::types::repr::{FunctionType, GenericTypeParam, ObjectType, ParameterType, PropertyType};

fn prop(name: impl Into<String>, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

#[test]
fn completed_memo_reuses_same_context_shared_dag() {
    const FANOUT: usize = 64;
    const FANOUT_U64: u64 = 64;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(96_000);
    let parameter = interner.intern_type_param(id, "T");
    let shared = interner.intern_object(ObjectType {
        properties: vec![prop("value", parameter)],
        ..Default::default()
    });
    let root = interner.intern_tuple(vec![shared; FANOUT]);
    let expected_shared = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let expected = interner.intern_tuple(vec![expected_shared; FANOUT]);
    let map = FxHashMap::from_iter([(id, wk.number)]);

    reset_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, root);
    let measure = substitution_measure();

    assert_eq!(
        result, expected,
        "memo reuse must preserve the interned result"
    );
    let tuple = interner
        .store()
        .tuple_type(result)
        .expect("the shared-DAG result must remain a tuple");
    assert!(tuple
        .elements
        .iter()
        .all(|&element| element == expected_shared));
    assert_eq!(measure.completed_memo_entries, 3);
    assert_eq!(measure.completed_memo_hits, FANOUT_U64 - 1);
    assert_eq!(
        measure.apply_visits,
        measure.completed_memo_entries + measure.completed_memo_hits,
        "each acyclic visit either completes one exact context or reuses it"
    );
    assert_eq!(measure.cycle_tainted_skips, 0);
    assert_eq!(measure.cycle_reentries, 0);
}

#[test]
fn completed_memo_canonicalizes_irrelevant_blockers_and_binder_order() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let mapped_id = TypeParamId(96_005);
    let irrelevant_u = TypeParamId(96_006);
    let irrelevant_v = TypeParamId(96_007);
    let mapped_blocker_a = TypeParamId(96_008);
    let mapped_blocker_b = TypeParamId(96_009);
    let parameter = interner.intern_type_param(mapped_id, "T");
    let shared = interner.intern_object(ObjectType {
        properties: vec![prop("value", parameter)],
        ..Default::default()
    });
    let wrapper = |type_params: Vec<TypeParamId>| FunctionType {
        type_params: type_params
            .into_iter()
            .map(|id| GenericTypeParam {
                id,
                constraint: None,
                default: None,
            })
            .collect(),
        receiver: None,
        params: vec![ParameterType::required("child", shared)],
        ret: wk.void,
    };
    let wrapper_u = interner.intern_function(wrapper(vec![irrelevant_u]));
    let wrapper_v = interner.intern_function(wrapper(vec![irrelevant_v]));
    let wrapper_ab = interner.intern_function(wrapper(vec![mapped_blocker_a, mapped_blocker_b]));
    let wrapper_ba = interner.intern_function(wrapper(vec![mapped_blocker_b, mapped_blocker_a]));
    // The shared child appears only below wrappers, so no empty-context occurrence
    // outside them can accidentally provide the required memo hit.
    let root = interner.intern_tuple(vec![wrapper_u, wrapper_v, wrapper_ab, wrapper_ba]);
    let expected_shared = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([
        (mapped_id, wk.number),
        (mapped_blocker_a, wk.string),
        (mapped_blocker_b, wk.boolean),
    ]);

    reset_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, root);
    let measure = substitution_measure();

    assert_ne!(result, root);
    let tuple = interner
        .store()
        .tuple_type(result)
        .expect("the irrelevant-blocker adversary must remain a tuple");
    assert_eq!(tuple.elements.len(), 4);
    for &element in &tuple.elements {
        let function = interner
            .store()
            .function_type(element)
            .expect("every tuple element remains a generic wrapper");
        assert_eq!(function.params[0].ty, expected_shared);
    }
    assert_eq!(
        measure.completed_memo_entries, 11,
        "U/V share the empty context while A/B and B/A share one sorted context"
    );
    assert_eq!(
        measure.completed_memo_hits, 4,
        "each equal-context wrapper pair reuses the shared child and void leaf"
    );
    assert_eq!(measure.apply_visits, 15);
    assert_eq!(
        measure.apply_visits,
        measure.completed_memo_entries + measure.completed_memo_hits,
    );
    assert_eq!(measure.blocked_type_param_hits, 0);
    assert_eq!(measure.cycle_tainted_skips, 0);
}

#[test]
fn completed_memo_keys_include_blocked_generic_binder_context() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(96_010);
    let parameter = interner.intern_type_param(id, "T");
    let shared = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: parameter,
    });
    let generic_wrapper = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("callback", shared)],
        ret: wk.void,
    });
    let root = interner.intern_tuple(vec![shared, generic_wrapper, shared]);
    let shared_number = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: wk.number,
    });
    let expected = interner.intern_tuple(vec![shared_number, generic_wrapper, shared_number]);
    let map = FxHashMap::from_iter([(id, wk.number)]);

    reset_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, root);
    let measure = substitution_measure();

    assert_eq!(result, expected);
    let tuple = interner
        .store()
        .tuple_type(result)
        .expect("the context adversary must remain a tuple");
    assert_eq!(tuple.elements[0], shared_number);
    assert_eq!(
        tuple.elements[1], generic_wrapper,
        "the wrapper-local T blocks the outer T → number substitution"
    );
    assert_eq!(tuple.elements[2], shared_number);
    assert!(measure.completed_memo_hits >= 1);
    assert!(measure.blocked_type_param_hits >= 1);
    assert!(measure.type_param_map_hits >= 1);
    assert!(
        measure.type_id_repeats > measure.exact_context_repeats,
        "one raw TypeId is visited under both empty and blocked contexts"
    );
    assert_eq!(measure.cycle_tainted_skips, 0);
}

#[test]
fn completed_memo_lifetime_is_one_substitution_run() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(96_015);
    let parameter = interner.intern_type_param(id, "T");
    let root = interner.intern_object(ObjectType {
        properties: vec![prop("value", parameter)],
        ..Default::default()
    });
    let expected_number = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let expected_string = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.string)],
        ..Default::default()
    });
    let number_map = FxHashMap::from_iter([(id, wk.number)]);
    let string_map = FxHashMap::from_iter([(id, wk.string)]);

    reset_substitution_measure();
    let number_result = Substitution::new(&number_map).apply(&mut interner, root);
    let after_number = substitution_measure();
    let string_result = Substitution::new(&string_map).apply(&mut interner, root);
    let after_string = substitution_measure();

    assert_eq!(number_result, expected_number);
    assert_eq!(string_result, expected_string);
    assert_ne!(number_result, string_result);
    assert_eq!(after_number.runs, 1);
    assert_eq!(after_number.completed_memo_entries, 2);
    assert_eq!(after_number.completed_memo_hits, 0);
    assert_eq!(after_string.runs, 2);
    assert_eq!(after_string.completed_memo_entries, 4);
    assert_eq!(
        after_string.completed_memo_hits, 0,
        "the second Substitution must not see the first run's root memo entry"
    );
}

#[test]
fn completed_memo_never_publishes_cross_context_cycle_ancestors() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(96_020);
    let parameter = interner.intern_type_param(id, "T");
    let recursive = interner.reserve_object();
    let cross_context_back_edge = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("self", recursive)],
        ret: parameter,
    });
    interner.fill_object(
        recursive,
        ObjectType {
            // Canonical property order deliberately visits the cross-context cycle
            // before the independently memoizable mapped leaf.
            properties: vec![
                prop("a_cycle", cross_context_back_edge),
                prop("z_mapped", parameter),
            ],
            ..Default::default()
        },
    );
    let recursive_array = interner.intern_array(recursive);
    let root = interner.intern_tuple(vec![recursive_array]);
    let map = FxHashMap::from_iter([(id, wk.number)]);

    reset_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let first = substitution.apply(&mut interner, root);
    let after_first = substitution_measure();

    assert_ne!(first, root, "the mapped acyclic sibling still rewrites");
    assert_eq!(after_first.cycle_reentries, 1);
    assert_eq!(
        after_first.cycle_tainted_skips, 4,
        "the generic function, recursive object, array, and root tuple are all tainted"
    );
    assert_eq!(
        after_first.completed_memo_entries, 2,
        "only T under the blocked and empty exact contexts may be memoized"
    );
    assert_eq!(after_first.completed_memo_hits, 0);

    let second = substitution.apply(&mut interner, root);
    let after_second = substitution_measure();
    assert_eq!(
        second, first,
        "re-entry must preserve the existing result shape"
    );
    assert_eq!(
        after_second.cycle_reentries, 2,
        "the differently-blocked back-edge must re-enter on the second apply"
    );
    assert_eq!(after_second.cycle_tainted_skips, 8);
    assert_eq!(
        after_second.completed_memo_entries, after_first.completed_memo_entries,
        "no tainted ancestor was published between top-level applies"
    );
    assert_eq!(
        after_second.completed_memo_hits, 2,
        "only the two safe T contexts are reusable on the second traversal"
    );
    assert!(after_second.apply_visits > after_first.apply_visits);

    let root_tuple = interner
        .store()
        .tuple_type(first)
        .expect("the cycle witness must remain wrapped in a tuple");
    let rewritten_recursive = interner
        .store()
        .array_type(root_tuple.elements[0])
        .expect("the cycle witness must remain wrapped in an array")
        .element;
    let rewritten = interner
        .store()
        .object_type(rewritten_recursive)
        .expect("the recursive child must remain an object");
    assert_eq!(
        rewritten.property("z_mapped").map(|property| property.ty),
        Some(wk.number)
    );
    assert_eq!(
        rewritten.property("a_cycle").map(|property| property.ty),
        Some(cross_context_back_edge),
        "the generic back-edge surface remains semantically unchanged"
    );
}

#[test]
fn completed_memo_bounds_message_event_target_fanout_by_unique_contexts() {
    const HANDLER_COUNT: usize = 64;
    const HANDLER_COUNT_U64: u64 = 64;
    const UNIQUE_EXACT_CONTEXTS: u64 = 9;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(96_030);
    let parameter = interner.intern_type_param(id, "T");
    let port_method = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: wk.void,
    });
    let message_port = interner.intern_object(ObjectType {
        properties: vec![prop("close", port_method), prop("start", port_method)],
        ..Default::default()
    });
    let message_ports = interner.intern_array(message_port);
    let message_event = interner.intern_object(ObjectType {
        properties: vec![prop("data", parameter), prop("ports", message_ports)],
        ..Default::default()
    });
    let listener = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("event", message_event)],
        ret: wk.void,
    });
    let registration = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("listener", listener)],
        ret: wk.void,
    });
    let mut target_properties = (0..HANDLER_COUNT)
        .map(|index| prop(format!("handler{index:02}"), registration))
        .collect::<Vec<_>>();
    target_properties.push(prop("port", message_port));
    let target = interner.intern_object(ObjectType {
        properties: target_properties,
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.string)]);

    reset_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, target);
    let measure = substitution_measure();

    assert_ne!(result, target);
    assert_eq!(measure.completed_memo_entries, UNIQUE_EXACT_CONTEXTS);
    assert!(
        measure.completed_memo_hits >= HANDLER_COUNT_U64,
        "registration fanout and the shared port surface must reuse completed contexts"
    );
    assert_eq!(
        measure.apply_visits,
        measure.completed_memo_entries + measure.completed_memo_hits,
        "acyclic work is bounded by unique contexts plus constant-time memo hits"
    );
    assert_eq!(measure.cycle_reentries, 0);
    assert_eq!(measure.cycle_tainted_skips, 0);

    let rewritten_target = interner
        .store()
        .object_type(result)
        .expect("the substituted target must remain an object");
    let first_handler = rewritten_target
        .property("handler00")
        .expect("the target keeps its first handler")
        .ty;
    assert!(
        (1..HANDLER_COUNT).all(|index| {
            rewritten_target
                .property(&format!("handler{index:02}"))
                .is_some_and(|property| property.ty == first_handler)
        }),
        "every repeated handler shares one rewritten registration surface"
    );
    let rewritten_registration = interner
        .store()
        .function_type(first_handler)
        .expect("a handler remains a registration function");
    let rewritten_listener = interner
        .store()
        .function_type(rewritten_registration.params[0].ty)
        .expect("the registration parameter remains a listener");
    let rewritten_event = interner
        .store()
        .object_type(rewritten_listener.params[0].ty)
        .expect("the listener parameter remains a message event");
    assert_eq!(
        rewritten_event.property("data").map(|property| property.ty),
        Some(wk.string),
        "MessageEvent<T>.data must still receive the substitution"
    );
}

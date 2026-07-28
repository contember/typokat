//! Acceptance spec for durable clean substitution results.
//!
//! This module is intentionally registered as a RED spec before implementation.
//!
//! A completed eager substitution is a derived fact of the current semantic
//! graph, the source `TypeId`, and the sorted argument vector for declaration
//! parameters free in that source. Paths that can retain or clone the mapper use
//! the whole sorted map instead. Any semantic graph mutation invalidates results.
//!
//! Results that observed a raw-id cycle guard are stack-dependent and must not
//! enter the durable cache. In particular, a child reached inside one live
//! recursive root can differ from the same child substituted as a fresh root.
//!
//! The tests use deterministic visit counters, never elapsed time.

use super::*;
use crate::types::repr::{
    ConditionalType, FunctionType, GenericTypeParam, MappedType, ModifierOp, ObjectType,
    ParameterType, PropertyType,
};
use std::sync::Arc;

fn prop(name: impl Into<String>, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn deep_generic_template(interner: &mut Interner, parameter: TypeId, depth: usize) -> TypeId {
    assert!(depth > 0, "the template must contain at least one layer");
    let mut current = parameter;
    for index in 0..depth {
        current = interner.intern_object(ObjectType {
            properties: vec![
                prop(format!("next{index:02}"), current),
                prop(format!("value{index:02}"), parameter),
            ],
            ..Default::default()
        });
    }
    current
}

#[test]
fn fresh_runs_reuse_one_clean_application_result_without_graph_work() {
    const DEPTH: usize = 48;
    const RUNS: u64 = 12;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_200);
    let parameter = interner.intern_type_param(id, "T");
    let template = deep_generic_template(&mut interner, parameter, DEPTH);
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let expected = substitute(&mut interner, template, &map);
    let after_first = substitution_measure().expect("the apply counter scope must remain enabled");
    assert_ne!(expected, template);
    assert!(
        after_first.apply_visits
            >= u64::try_from(DEPTH).expect("the test depth fits the visit counter"),
        "the cold application must traverse the generic template"
    );

    for _ in 1..RUNS {
        assert_eq!(substitute(&mut interner, template, &map), expected);
    }
    let after_reuse = substitution_measure().expect("the apply counter scope must remain enabled");

    assert_eq!(after_reuse.runs, RUNS);
    assert_eq!(
        after_reuse.apply_visits, after_first.apply_visits,
        "fresh substitutions with the same clean key must not reopen the apply graph"
    );
}

#[test]
fn irrelevant_map_entries_share_the_same_durable_application_key() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let relevant_id = TypeParamId(99_210);
    let irrelevant_id = TypeParamId(99_211);
    let parameter = interner.intern_type_param(relevant_id, "T");
    let template = deep_generic_template(&mut interner, parameter, 16);
    let minimal_map = FxHashMap::from_iter([(relevant_id, wk.number)]);
    let wider_map = FxHashMap::from_iter([(relevant_id, wk.number), (irrelevant_id, wk.string)]);

    let _scope = start_substitution_measure();
    let first = substitute(&mut interner, template, &minimal_map);
    let after_first = substitution_measure().expect("the apply counter scope must remain enabled");
    let second = substitute(&mut interner, template, &wider_map);
    let after_second = substitution_measure().expect("the apply counter scope must remain enabled");

    assert_eq!(second, first);
    assert_eq!(
        after_second.apply_visits, after_first.apply_visits,
        "map entries absent from the source free set must not split the durable key"
    );
}

#[test]
fn distinct_relevant_arguments_miss_once_then_reuse_their_own_result() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_220);
    let parameter = interner.intern_type_param(id, "T");
    let template = deep_generic_template(&mut interner, parameter, 16);
    let number_map = FxHashMap::from_iter([(id, wk.number)]);
    let string_map = FxHashMap::from_iter([(id, wk.string)]);

    let _scope = start_substitution_measure();
    let number_result = substitute(&mut interner, template, &number_map);
    let after_number = substitution_measure().expect("the apply counter scope must remain enabled");
    let string_result = substitute(&mut interner, template, &string_map);
    let after_string_miss =
        substitution_measure().expect("the apply counter scope must remain enabled");

    assert_ne!(number_result, string_result);
    assert!(
        after_string_miss.apply_visits > after_number.apply_visits,
        "a different argument for a free parameter must miss the first result"
    );

    assert_eq!(
        substitute(&mut interner, template, &string_map),
        string_result
    );
    let after_string_reuse =
        substitution_measure().expect("the apply counter scope must remain enabled");
    assert_eq!(
        after_string_reuse.apply_visits, after_string_miss.apply_visits,
        "the newly completed relevant argument vector must become reusable"
    );
}

#[test]
fn semantic_graph_mutation_invalidates_then_rebuilds_application_results() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_230);
    let parameter = interner.intern_type_param(id, "T");
    let reserved = interner.reserve_object();
    let template = interner.intern_object(ObjectType {
        properties: vec![prop("child", reserved), prop("marker", parameter)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let before_fill = substitute(&mut interner, template, &map);
    let after_first = substitution_measure().expect("the apply counter scope must remain enabled");
    let old_graph = Arc::clone(interner.store().semantic_graph_identity());

    interner.fill_object(
        reserved,
        ObjectType {
            properties: vec![prop("value", parameter)],
            ..Default::default()
        },
    );
    assert!(
        !Arc::ptr_eq(&old_graph, interner.store().semantic_graph_identity()),
        "reserved fill must rotate the semantic graph identity"
    );
    let expected_child = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let expected_after_fill = interner.intern_object(ObjectType {
        properties: vec![prop("child", expected_child), prop("marker", wk.number)],
        ..Default::default()
    });

    let after_fill = substitute(&mut interner, template, &map);
    let after_invalidation =
        substitution_measure().expect("the apply counter scope must remain enabled");
    assert_ne!(
        after_fill, before_fill,
        "the filled outgoing edge changes the application result"
    );
    assert_eq!(after_fill, expected_after_fill);
    assert!(
        after_invalidation.apply_visits > after_first.apply_visits,
        "a result from the old semantic graph must not survive mutation"
    );

    assert_eq!(
        substitute(&mut interner, template, &map),
        expected_after_fill
    );
    let after_reuse = substitution_measure().expect("the apply counter scope must remain enabled");
    assert_eq!(
        after_reuse.apply_visits, after_invalidation.apply_visits,
        "the first clean result in the new graph must become reusable"
    );
}

#[test]
fn binder_closed_recursive_graph_publishes_a_clean_durable_result() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_240);
    let parameter = interner.intern_type_param(id, "T");
    let recursive = interner.reserve_object();
    let binder_closed_back_edge = interner.intern_function(FunctionType {
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
            properties: vec![
                prop("back", binder_closed_back_edge),
                prop("value", parameter),
            ],
            ..Default::default()
        },
    );
    let expected = interner.intern_object(ObjectType {
        properties: vec![
            prop("back", binder_closed_back_edge),
            prop("value", wk.number),
        ],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let first = substitute_with_outcome(&mut interner, recursive, &map);
    let after_first = substitution_measure().expect("the apply counter scope must remain enabled");
    assert_eq!(first, SubstitutionOutcome::CycleClean(expected));
    assert_eq!(after_first.cycle_reentries, 0);

    let second = substitute_with_outcome(&mut interner, recursive, &map);
    let after_second = substitution_measure().expect("the apply counter scope must remain enabled");
    assert_eq!(second, first);
    assert_eq!(
        after_second.apply_visits, after_first.apply_visits,
        "a recursive declaration graph is cacheable when binder masking avoids re-entry"
    );
}

#[test]
fn stack_dependent_cycle_results_never_leak_between_fresh_runs() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_250);
    let parameter = interner.intern_type_param(id, "T");
    let a = interner.reserve_object();
    let b = interner.intern_object(ObjectType {
        properties: vec![prop("back", a), prop("value", parameter)],
        ..Default::default()
    });
    interner.fill_object(
        a,
        ObjectType {
            properties: vec![prop("child", b), prop("value", parameter)],
            ..Default::default()
        },
    );
    let b_inside_a = interner.intern_object(ObjectType {
        properties: vec![prop("back", a), prop("value", wk.number)],
        ..Default::default()
    });
    let expected_a = interner.intern_object(ObjectType {
        properties: vec![prop("child", b_inside_a), prop("value", wk.number)],
        ..Default::default()
    });
    let a_inside_b = interner.intern_object(ObjectType {
        properties: vec![prop("child", b), prop("value", wk.number)],
        ..Default::default()
    });
    let expected_standalone_b = interner.intern_object(ObjectType {
        properties: vec![prop("back", a_inside_b), prop("value", wk.number)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let first = substitute_with_outcome(&mut interner, a, &map);
    let after_first = substitution_measure().expect("the apply counter scope must remain enabled");
    let second = substitute_with_outcome(&mut interner, b, &map);
    let after_second = substitution_measure().expect("the apply counter scope must remain enabled");

    assert_eq!(first, SubstitutionOutcome::CycleTainted(expected_a));
    assert_eq!(
        second,
        SubstitutionOutcome::CycleTainted(expected_standalone_b),
        "the child result produced while `a` was live must not escape that stack"
    );
    assert_ne!(
        expected_standalone_b, b_inside_a,
        "the safety witness must distinguish the two live-stack contexts"
    );
    assert!(
        after_second.apply_visits > after_first.apply_visits,
        "a tainted child from the first run must be recomputed as a fresh root"
    );
    assert!(
        after_second.cycle_reentries > after_first.cycle_reentries,
        "the second run must observe its own raw-id cycle cut"
    );
}

#[test]
fn lazy_conditional_result_does_not_depend_on_the_first_retained_mapper() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let relevant_id = TypeParamId(99_260);
    let retained_id = TypeParamId(99_261);
    let relevant = interner.intern_type_param(relevant_id, "T");
    let union = interner.union(vec![wk.number, wk.string]);
    let source = interner.intern_conditional(ConditionalType {
        check: relevant,
        extends_ty: wk.unknown,
        true_branch: relevant,
        false_branch: wk.never,
        infer_count: 0,
        distributive: true,
        poisoned: false,
    });
    let number_map = FxHashMap::from_iter([(relevant_id, union), (retained_id, wk.number)]);
    let string_map = FxHashMap::from_iter([(relevant_id, union), (retained_id, wk.string)]);

    let number_result = substitute(&mut interner, source, &number_map);

    let mutation = interner.reserve_object();
    interner.fill_object(mutation, ObjectType::default());

    let string_result = substitute(&mut interner, source, &string_map);
    assert_ne!(
        string_result, number_result,
        "lazy instantiations retain mapper entries outside the source free set"
    );
    assert_eq!(
        substitute(&mut interner, source, &number_map),
        number_result,
        "a durable key must distinguish every mapper entry retained in the result"
    );
}

#[test]
fn lazy_conditional_descendant_promotes_the_root_to_a_full_mapper_key() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let relevant_id = TypeParamId(99_270);
    let retained_id = TypeParamId(99_271);
    let relevant = interner.intern_type_param(relevant_id, "T");
    let union = interner.union(vec![wk.number, wk.string]);
    let conditional = interner.intern_conditional(ConditionalType {
        check: relevant,
        extends_ty: wk.unknown,
        true_branch: relevant,
        false_branch: wk.never,
        infer_count: 0,
        distributive: true,
        poisoned: false,
    });
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("value", conditional)],
        ..Default::default()
    });
    let number_map = FxHashMap::from_iter([(relevant_id, union), (retained_id, wk.number)]);
    let string_map = FxHashMap::from_iter([(relevant_id, union), (retained_id, wk.string)]);

    let _scope = start_substitution_measure();
    let number_result = substitute(&mut interner, source, &number_map);
    let after_number = substitution_measure().expect("the apply counter scope must remain enabled");
    let string_result = substitute(&mut interner, source, &string_map);
    let after_string = substitution_measure().expect("the apply counter scope must remain enabled");

    assert_ne!(string_result, number_result);
    assert!(
        after_string.apply_visits > after_number.apply_visits,
        "a lazy descendant must make the root distinguish the retained mapper"
    );
    assert_eq!(
        substitute(&mut interner, source, &number_map),
        number_result
    );
    let after_reuse = substitution_measure().expect("the apply counter scope must remain enabled");
    assert_eq!(after_reuse.apply_visits, after_string.apply_visits);
}

#[test]
fn non_distributive_conditionals_remain_relevant_only() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let relevant_id = TypeParamId(99_275);
    let irrelevant_id = TypeParamId(99_276);
    let relevant = interner.intern_type_param(relevant_id, "T");
    let source = interner.intern_conditional(ConditionalType {
        check: relevant,
        extends_ty: wk.unknown,
        true_branch: relevant,
        false_branch: wk.never,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    let minimal_map = FxHashMap::from_iter([(relevant_id, wk.number)]);
    let wider_map = FxHashMap::from_iter([(relevant_id, wk.number), (irrelevant_id, wk.string)]);

    let _scope = start_substitution_measure();
    let first = substitute(&mut interner, source, &minimal_map);
    let after_first = substitution_measure().expect("the apply counter scope must remain enabled");
    assert_eq!(substitute(&mut interner, source, &wider_map), first);
    let after_reuse = substitution_measure().expect("the apply counter scope must remain enabled");
    assert_eq!(after_reuse.apply_visits, after_first.apply_visits);
}

#[test]
fn homomorphic_mapped_roots_conservatively_key_on_the_full_mapper() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let relevant_id = TypeParamId(99_280);
    let irrelevant_id = TypeParamId(99_281);
    let relevant = interner.intern_type_param(relevant_id, "T");
    let placeholder = interner.intern_mapped_value();
    let left = interner.intern_object(ObjectType {
        properties: vec![prop("left", wk.number)],
        ..Default::default()
    });
    let right = interner.intern_object(ObjectType {
        properties: vec![prop("right", wk.string)],
        ..Default::default()
    });
    let union = interner.union(vec![left, right]);
    let source = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: relevant,
        value_template: placeholder,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let minimal_map = FxHashMap::from_iter([(relevant_id, union)]);
    let wider_map = FxHashMap::from_iter([(relevant_id, union), (irrelevant_id, wk.string)]);

    let _scope = start_substitution_measure();
    let minimal_result = substitute(&mut interner, source, &minimal_map);
    let after_minimal =
        substitution_measure().expect("the apply counter scope must remain enabled");
    let wider_result = substitute(&mut interner, source, &wider_map);
    let after_wider = substitution_measure().expect("the apply counter scope must remain enabled");

    assert_eq!(wider_result, minimal_result);
    assert!(
        after_wider.apply_visits > after_minimal.apply_visits,
        "recursive mapper cloning must not reuse a narrower mapper key"
    );
    assert_eq!(substitute(&mut interner, source, &wider_map), wider_result);
    let after_reuse = substitution_measure().expect("the apply counter scope must remain enabled");
    assert_eq!(after_reuse.apply_visits, after_wider.apply_visits);
}

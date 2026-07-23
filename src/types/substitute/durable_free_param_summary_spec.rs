//! Acceptance spec for durable, map-independent free-parameter summaries.
//!
//! This module is intentionally registered as a RED spec before implementation.
//!
//! A fresh `Substitution` must not rebuild binder-aware free-parameter data for
//! an immutable `TypeId` graph. The durable summary belongs to the store's
//! current semantic graph identity, records declaration `TypeParamId`s rather
//! than one substitution map's bit positions, and is masked against each map at
//! query time. Therefore the first query may scan the graph, while later fresh
//! substitutions with different maps reuse the same summary without scanning.
//!
//! Summaries remain binder-precise inside recursive SCCs: two nodes in one SCC
//! can have distinct free sets when a function binder shadows an occurrence on
//! one entry path. Filling a reserved row replaces the store semantic graph
//! identity and must make every summary from the old graph unreachable before
//! the filled edge is queried. Large maps do not disable the closed-type fast
//! path; a durable empty summary has no per-map width limit.
//!
//! The tests use graph-scan counters, never elapsed time. Substitution results
//! remain byte-for-byte identical to the uncached semantics.

use super::*;
use crate::types::repr::{FunctionType, GenericTypeParam, ObjectType, PropertyType};
use std::sync::Arc;

fn prop(name: impl Into<String>, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn param_free_cyclic_diamond(interner: &mut Interner, levels: usize) -> TypeId {
    assert!(levels > 0, "the cyclic diamond needs at least one level");

    let recursive = interner.reserve_object();
    let wk = interner.well_known();
    let mut level = interner.intern_object(ObjectType {
        properties: vec![prop("back", recursive), prop("leaf", wk.string)],
        ..Default::default()
    });
    for _ in 1..levels {
        level = interner.intern_object(ObjectType {
            properties: vec![
                prop("left", level),
                prop("right", level),
                prop("back", recursive),
            ],
            ..Default::default()
        });
    }
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("next", level)],
            ..Default::default()
        },
    );
    recursive
}

#[test]
fn fresh_substitutions_build_one_param_free_summary_across_distinct_maps() {
    const LEVELS: usize = 64;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let root = param_free_cyclic_diamond(&mut interner, LEVELS);
    let first_map = FxHashMap::from_iter([(TypeParamId(99_000), wk.number)]);
    let second_map = FxHashMap::from_iter([(TypeParamId(99_001), wk.boolean)]);

    let _scope = start_substitution_measure();
    assert_eq!(substitute(&mut interner, root, &first_map), root);
    let after_first =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");
    let levels = u64::try_from(LEVELS).expect("test levels fit the counter");
    assert!(
        after_first.prefilter_graph_scans >= levels,
        "the cold query must prove that the large graph is parameter-free"
    );

    assert_eq!(substitute(&mut interner, root, &second_map), root);
    let after_second =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");

    assert_eq!(after_second.runs, after_first.runs + 1);
    assert_eq!(
        after_second.prefilter_skips,
        after_first.prefilter_skips + 1
    );
    assert_eq!(
        after_second.prefilter_graph_scans, after_first.prefilter_graph_scans,
        "a fresh run and a different map must reuse the store-owned closed summary"
    );
    assert_eq!(
        after_second.apply_visits, after_first.apply_visits,
        "a durable empty summary must answer before the substitution walker opens"
    );
}

#[test]
fn durable_summary_is_map_independent_for_open_type_parameters() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first_id = TypeParamId(99_010);
    let second_id = TypeParamId(99_011);
    let first_param = interner.intern_type_param(first_id, "T");
    let second_param = interner.intern_type_param(second_id, "U");
    let root = interner.intern_object(ObjectType {
        properties: vec![prop("first", first_param), prop("second", second_param)],
        ..Default::default()
    });
    let first_map = FxHashMap::from_iter([(first_id, wk.number)]);
    let second_map = FxHashMap::from_iter([(second_id, wk.string)]);

    let _scope = start_substitution_measure();
    let first_result = substitute(&mut interner, root, &first_map);
    let after_first =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");
    let first_object = interner
        .store()
        .object_type(first_result)
        .expect("substitution must preserve the object shape");
    assert_eq!(
        first_object.property("first").map(|property| property.ty),
        Some(wk.number)
    );
    assert_eq!(
        first_object.property("second").map(|property| property.ty),
        Some(second_param)
    );

    let second_result = substitute(&mut interner, root, &second_map);
    let after_second =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");
    let second_object = interner
        .store()
        .object_type(second_result)
        .expect("substitution must preserve the object shape");
    assert_eq!(
        second_object.property("first").map(|property| property.ty),
        Some(first_param)
    );
    assert_eq!(
        second_object.property("second").map(|property| property.ty),
        Some(wk.string)
    );
    assert_eq!(
        after_second.prefilter_graph_scans, after_first.prefilter_graph_scans,
        "the durable summary stores declaration ids, not the first map's bitmask"
    );
}

#[test]
fn recursive_scc_nodes_keep_distinct_binder_precise_free_sets() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_020);
    let parameter = interner.intern_type_param(id, "T");
    let object = interner.reserve_object();
    let function = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: Vec::new(),
        ret: object,
    });
    interner.fill_object(
        object,
        ObjectType {
            properties: vec![prop("method", function), prop("value", parameter)],
            ..Default::default()
        },
    );
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    assert_eq!(
        substitute(&mut interner, function, &map),
        function,
        "T is bound when the recursive SCC is entered through the function"
    );
    let after_function =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");
    assert_eq!(
        after_function.apply_visits, 0,
        "the binder-closed entry is answered by its summary"
    );

    let object_result = substitute(&mut interner, object, &map);
    let after_object =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");
    let rewritten = interner
        .store()
        .object_type(object_result)
        .expect("substitution must preserve the object shape");
    assert_ne!(
        object_result, object,
        "T is free when the same SCC is entered through the object"
    );
    assert_eq!(
        rewritten.property("value").map(|property| property.ty),
        Some(wk.number)
    );
    assert_eq!(
        rewritten.property("method").map(|property| property.ty),
        Some(function),
        "the nested function still binds its own T"
    );
    assert_eq!(
        after_object.prefilter_graph_scans, after_function.prefilter_graph_scans,
        "the first SCC analysis must publish each node's distinct exact summary"
    );
}

#[test]
fn reserved_fill_rotates_graph_identity_and_rebuilds_the_stale_summary_once() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_030);
    let parameter = interner.intern_type_param(id, "T");
    let reserved = interner.reserve_object();
    let map = FxHashMap::from_iter([(id, wk.number)]);
    let old_graph = Arc::clone(interner.store().semantic_graph_identity());

    let _scope = start_substitution_measure();
    assert_eq!(substitute(&mut interner, reserved, &map), reserved);
    let before_fill =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");

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
    let expected = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });

    assert_eq!(substitute(&mut interner, reserved, &map), expected);
    let after_rebuild =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");
    assert!(
        after_rebuild.prefilter_graph_scans > before_fill.prefilter_graph_scans,
        "the old empty summary must not survive the filled outgoing edge"
    );

    assert_eq!(substitute(&mut interner, reserved, &map), expected);
    let after_reuse =
        substitution_measure().expect("the graph-scan counter scope must remain enabled");
    assert_eq!(
        after_reuse.prefilter_graph_scans, after_rebuild.prefilter_graph_scans,
        "the rebuilt summary is durable for the new semantic graph identity"
    );
}

#[test]
fn maps_wider_than_machine_bitsets_keep_the_closed_type_fast_path() {
    const MAP_SIZE: u32 = 65;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let root = param_free_cyclic_diamond(&mut interner, 8);
    let map = (0_u32..MAP_SIZE)
        .map(|offset| (TypeParamId(99_100 + offset), wk.number))
        .collect::<FxHashMap<_, _>>();

    let store_len_before = interner.store().len();
    let _scope = start_substitution_measure();
    assert_eq!(substitute(&mut interner, root, &map), root);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(measure.apply_visits, 0);
    assert_eq!(measure.cycle_reentries, 0);
    assert_eq!(measure.prefilter_skips, 1);
    assert_eq!(
        interner.store().len(),
        store_len_before,
        "a large irrelevant map must not rebuild a closed nominal graph"
    );
}

//! Acceptance spec for durable deferred-`keyof` root summaries.
//!
//! `contains_deferred_keyof` is a pure fact about an immutable `TypeId` graph.
//! Repeated generic constraint checks must therefore scan a root only once.
//! Cached results belong to the current semantic graph identity: a reserved-row
//! fill invalidates them, while a sealed immutable prefix can share them with
//! isolated delta interners. The tests count graph work, never elapsed time.

use super::keyof::{contains_deferred_keyof, deferred_keyof_measure, start_deferred_keyof_measure};
use crate::types::repr::{FunctionType, ObjectType, ParameterType, PropertyType, TypeParamId};
use crate::types::Interner;
use std::sync::Arc;

fn prop(name: impl Into<String>, ty: crate::types::store::TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn object_chain(
    interner: &mut Interner,
    mut leaf: crate::types::store::TypeId,
    depth: usize,
) -> crate::types::store::TypeId {
    for index in 0..depth {
        leaf = interner.intern_object(ObjectType {
            properties: vec![prop(format!("level{index}"), leaf)],
            ..Default::default()
        });
    }
    leaf
}

#[test]
fn repeated_negative_root_scans_the_graph_once() {
    let mut interner = Interner::with_intrinsics();
    let string = interner.well_known().string;
    let root = object_chain(&mut interner, string, 64);

    let _scope = start_deferred_keyof_measure();
    assert!(!contains_deferred_keyof(&mut interner, root));
    let after_first =
        deferred_keyof_measure().expect("the graph counter scope must remain enabled");
    assert!(after_first.node_visits >= 64);

    for _ in 0..31 {
        assert!(!contains_deferred_keyof(&mut interner, root));
    }
    let after_repeats =
        deferred_keyof_measure().expect("the graph counter scope must remain enabled");
    assert_eq!(
        after_repeats.graph_scans, after_first.graph_scans,
        "a durable negative root summary must answer repeated constraint checks"
    );
    assert_eq!(
        after_repeats.node_visits, after_first.node_visits,
        "repeated absence proofs must not reopen the immutable graph"
    );
}

#[test]
fn repeated_positive_root_scans_the_graph_once() {
    let mut interner = Interner::with_intrinsics();
    let parameter = interner.intern_type_param(TypeParamId(99_700), "T");
    let deferred = interner.intern_keyof(parameter);
    let root = object_chain(&mut interner, deferred, 64);

    let _scope = start_deferred_keyof_measure();
    assert!(contains_deferred_keyof(&mut interner, root));
    let after_first =
        deferred_keyof_measure().expect("the graph counter scope must remain enabled");

    for _ in 0..31 {
        assert!(contains_deferred_keyof(&mut interner, root));
    }
    let after_repeats =
        deferred_keyof_measure().expect("the graph counter scope must remain enabled");
    assert_eq!(
        after_repeats.graph_scans, after_first.graph_scans,
        "a durable positive root summary must answer repeated constraint checks"
    );
    assert_eq!(
        after_repeats.node_visits, after_first.node_visits,
        "repeated positive queries must not search for the same deferred node"
    );
}

#[test]
fn reserved_fill_invalidates_the_old_root_summary_once() {
    let mut interner = Interner::with_intrinsics();
    let parameter = interner.intern_type_param(TypeParamId(99_701), "T");
    let deferred = interner.intern_keyof(parameter);
    let reserved = interner.reserve_object();
    let root = interner.intern_object(ObjectType {
        properties: vec![prop("child", reserved)],
        ..Default::default()
    });
    let old_graph = Arc::clone(interner.store().semantic_graph_identity());

    let _scope = start_deferred_keyof_measure();
    assert!(!contains_deferred_keyof(&mut interner, root));
    let before_fill =
        deferred_keyof_measure().expect("the graph counter scope must remain enabled");

    interner.fill_object(
        reserved,
        ObjectType {
            properties: vec![prop("key", deferred)],
            ..Default::default()
        },
    );
    assert!(
        !Arc::ptr_eq(&old_graph, interner.store().semantic_graph_identity()),
        "reserved fill must rotate the semantic graph identity"
    );

    assert!(contains_deferred_keyof(&mut interner, root));
    let after_rebuild =
        deferred_keyof_measure().expect("the graph counter scope must remain enabled");
    assert_eq!(after_rebuild.graph_scans, before_fill.graph_scans + 1);
    assert!(after_rebuild.node_visits > before_fill.node_visits);

    assert!(contains_deferred_keyof(&mut interner, root));
    let after_reuse =
        deferred_keyof_measure().expect("the graph counter scope must remain enabled");
    assert_eq!(
        after_reuse.graph_scans, after_rebuild.graph_scans,
        "the rebuilt result must become durable for the new graph identity"
    );
    assert_eq!(after_reuse.node_visits, after_rebuild.node_visits);
}

#[test]
fn sealed_prefix_results_are_shared_without_cross_fork_suffix_aliasing() {
    let mut base = Interner::with_intrinsics();
    let wk = base.well_known();
    let parameter = base.intern_type_param(TypeParamId(99_702), "T");
    let deferred = base.intern_keyof(parameter);
    let negative = object_chain(&mut base, wk.string, 8);
    let positive = object_chain(&mut base, deferred, 8);

    let _scope = start_deferred_keyof_measure();
    assert!(!contains_deferred_keyof(&mut base, negative));
    assert!(contains_deferred_keyof(&mut base, positive));
    let after_base = deferred_keyof_measure().expect("the graph counter scope must remain enabled");
    base.freeze_as_base().expect("complete interner seals");

    let mut first = base.fork_delta().expect("first suffix forks");
    let mut second = base.fork_delta().expect("second suffix forks");
    assert!(!contains_deferred_keyof(&mut first, negative));
    assert!(contains_deferred_keyof(&mut first, positive));
    assert!(!contains_deferred_keyof(&mut second, negative));
    assert!(contains_deferred_keyof(&mut second, positive));
    let after_prefix_reuse =
        deferred_keyof_measure().expect("the graph counter scope must remain enabled");
    assert_eq!(
        after_prefix_reuse.graph_scans, after_base.graph_scans,
        "sealed prefix summaries must remain reusable across delta identities"
    );
    assert_eq!(after_prefix_reuse.node_visits, after_base.node_visits);

    let first_local = first.intern_object(ObjectType {
        properties: vec![prop("local", deferred)],
        ..Default::default()
    });
    let second_local = second.intern_object(ObjectType {
        properties: vec![prop("local", wk.number)],
        ..Default::default()
    });
    assert_eq!(
        first_local, second_local,
        "isolated suffixes intentionally reuse dense local TypeIds"
    );
    assert!(contains_deferred_keyof(&mut first, first_local));
    assert!(!contains_deferred_keyof(&mut second, second_local));
}

#[test]
fn specialized_deferred_keyof_traversal_semantics_are_preserved() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter = interner.intern_type_param(TypeParamId(99_703), "T");
    let deferred = interner.intern_keyof(parameter);
    let concrete = interner.intern_keyof(wk.number);

    assert!(contains_deferred_keyof(&mut interner, deferred));
    assert!(!contains_deferred_keyof(&mut interner, concrete));

    let generic_base = interner.intern_object(ObjectType {
        properties: vec![prop("hidden", deferred)],
        ..Default::default()
    });
    let argument_only =
        interner.intern_instantiation(generic_base, vec![(TypeParamId(99_703), wk.string)]);
    assert!(
        !contains_deferred_keyof(&mut interner, argument_only),
        "instantiation bases are deliberately excluded from this specialized walk"
    );
    let deferred_argument =
        interner.intern_instantiation(wk.string, vec![(TypeParamId(99_703), deferred)]);
    assert!(
        contains_deferred_keyof(&mut interner, deferred_argument),
        "instantiation argument values remain part of the specialized walk"
    );

    let receiver = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(deferred),
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.void,
    });
    assert!(
        contains_deferred_keyof(&mut interner, receiver),
        "function receivers remain part of the specialized walk"
    );
}

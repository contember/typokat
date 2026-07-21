//! Acceptance spec for the param-relevant substitution prefilter and the
//! internal-cycle fold refinement of the cycle-scoped tainted memo.
//!
//! Activate from `substitute/mod.rs` only with the implementation.
//!
//! **Prefilter**: per run, `apply(ty)` returns `ty` immediately — no subtree
//! walk, no visit counted, no memo entry, no interning — whenever `ty`'s free
//! map-relevant parameter set, minus the currently blocked ids, is empty. The
//! free set of a node is the set of map-key `TypeParamId`s reachable along
//! exactly the edges the `apply_*` arms recurse into; a `Function` node removes
//! its own declared binder ids from everything reachable through it (matching
//! how `apply` blocks binders for the whole signature walk — constraints,
//! defaults, params, receiver, ret), an `Instantiation` node contributes only
//! its argument *values* (the base stays lazy), a `TypeParam` contributes its
//! own id iff it is a map key, and `Intrinsic`/`Literal`/`Infer`/`MappedValue`
//! are terminal-irrelevant. Cycles are plain graph reachability. Results are
//! byte-for-byte unchanged — a node with an empty effective free set already
//! substitutes to itself today; the prefilter only skips the proof-by-walking.
//! Skips count in the new `prefilter_skips` measure and never increment
//! `apply_visits`.
//!
//! **Internal-cycle fold**: re-entries to a node whose own frame opened *and*
//! closed within a subtree are self-reproducing — a fresh walk of that subtree
//! recreates the frame and re-enters it identically. So when the frame for id
//! X ends, X is removed from the folded/stored `reentered` sets (X lands in
//! the parent's `visited` instead, which already happens). Entries above a
//! fully-internal cycle then validate by the `visited` check alone — they
//! still reject when any recorded plainly-walked id, including X, is live at
//! reuse, so the stale-entry semantics of the cycle-scoped memo spec hold.

use super::*;
use crate::types::repr::{FunctionType, GenericTypeParam, ObjectType, ParameterType, PropertyType};

fn prop(name: impl Into<String>, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

#[test]
fn prefilter_returns_param_free_cyclic_diamond_untouched() {
    const LEVELS: usize = 8;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let unrelated = TypeParamId(98_000);
    let recursive = interner.reserve_object();
    // A cyclic diamond with no parameter anywhere: the effective free set of
    // every node is empty, so the prefilter answers without opening one frame.
    let mut level = interner.intern_object(ObjectType {
        properties: vec![prop("back", recursive)],
        ..Default::default()
    });
    for _ in 0..LEVELS - 1 {
        level = interner.intern_object(ObjectType {
            properties: vec![prop("a", level), prop("b", level), prop("back", recursive)],
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
    let map = FxHashMap::from_iter([(unrelated, wk.number)]);

    let store_len_before = interner.store().len();
    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, recursive);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(
        result, recursive,
        "a param-free graph substitutes to itself"
    );
    assert_eq!(
        interner.store().len(),
        store_len_before,
        "a prefiltered run must not intern anything"
    );
    assert_eq!(
        measure.apply_visits, 0,
        "prefiltered entries are not visits"
    );
    assert!(measure.prefilter_skips >= 1);
    assert_eq!(
        measure.cycle_reentries, 0,
        "no frame opens, so no back edge is ever taken"
    );
    assert_eq!(measure.completed_memo_entries, 0);
    assert_eq!(measure.tainted_memo_entries, 0);
}

#[test]
fn prefilter_respects_binder_shadowing_not_raw_param_ids() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(98_005);
    let parameter = interner.intern_type_param(id, "P");
    // The DOM shape: a method generic over P returning its own P — the map key
    // matches the raw id, but every occurrence is shadowed inside the binder.
    let method = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: Vec::new(),
        ret: parameter,
    });
    let object = interner.intern_object(ObjectType {
        properties: vec![prop("method", method)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, object);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(
        result, object,
        "the shadowed binder leaves the surface unchanged"
    );
    // Control (measure already snapshotted): the same raw id *free* at the
    // root still rewrites — the filter is shadowing-aware, not id-blind.
    let control = Substitution::new(&map).apply(&mut interner, parameter);
    assert_eq!(control, wk.number);
    assert_eq!(measure.apply_visits, 0);
    assert!(measure.prefilter_skips >= 1);
    assert_eq!(
        measure.blocked_type_param_hits, 0,
        "the signature walk never happens"
    );
}

#[test]
fn prefilter_does_not_leak_a_function_binder_through_a_recursive_scc() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(98_007);
    let parameter = interner.intern_type_param(id, "P");
    let root = interner.reserve_object();
    let inner = interner.intern_object(ObjectType {
        properties: vec![prop("back", root), prop("value", parameter)],
        ..Default::default()
    });
    let method = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("input", inner)],
        ret: wk.void,
    });
    interner.fill_object(
        root,
        ObjectType {
            properties: vec![prop("method", method)],
            ..Default::default()
        },
    );
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let store_len_before = interner.store().len();
    let _scope = start_substitution_measure();
    let result = Substitution::new(&map).apply(&mut interner, root);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(result, root);
    assert_eq!(interner.store().len(), store_len_before);
    assert_eq!(measure.apply_visits, 0);
    assert!(measure.prefilter_skips >= 1);
    assert_eq!(measure.cycle_reentries, 0);
    assert_eq!(measure.completed_memo_entries, 0);
    assert_eq!(measure.tainted_memo_entries, 0);

    // Consuming the outer binder makes the direct occurrence free, while the
    // back-edge method opens a fresh binder and remains unchanged.
    let instantiated = instantiate_function(&mut interner, method, &map);
    let instantiated = interner
        .store()
        .function_type(instantiated)
        .expect("instantiation remains a function");
    assert!(instantiated.type_params.is_empty());
    let rewritten_inner = interner
        .store()
        .object_type(instantiated.params[0].ty)
        .expect("the parameter remains an object");
    assert_eq!(
        rewritten_inner
            .property("value")
            .map(|property| property.ty),
        Some(wk.number)
    );
    assert_eq!(
        rewritten_inner.property("back").map(|property| property.ty),
        Some(root)
    );

    let free = interner.intern_object(ObjectType {
        properties: vec![prop("value", parameter)],
        ..Default::default()
    });
    let expected = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    assert_eq!(Substitution::new(&map).apply(&mut interner, free), expected);
}

#[test]
fn recursive_binder_prefilter_cost_is_independent_of_inner_diamond_fanout() {
    const LEVELS: usize = 12;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(98_008);
    let parameter = interner.intern_type_param(id, "P");
    let root = interner.reserve_object();
    let inner = interner.intern_object(ObjectType {
        properties: vec![prop("back", root), prop("value", parameter)],
        ..Default::default()
    });
    let mut payload = interner.intern_object(ObjectType {
        properties: vec![prop("back", root)],
        ..Default::default()
    });
    for _ in 0..LEVELS {
        payload = interner.intern_object(ObjectType {
            properties: vec![prop("left", payload), prop("right", payload)],
            ..Default::default()
        });
    }
    let method = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("input", inner)],
        ret: wk.void,
    });
    interner.fill_object(
        root,
        ObjectType {
            properties: vec![prop("method", method), prop("payload", payload)],
            ..Default::default()
        },
    );
    let map = FxHashMap::from_iter([(id, wk.string)]);

    let store_len_before = interner.store().len();
    let _scope = start_substitution_measure();
    let result = Substitution::new(&map).apply(&mut interner, root);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(result, root);
    assert_eq!(interner.store().len(), store_len_before);
    assert_eq!(measure.apply_visits, 0);
    assert!(measure.prefilter_skips >= 1);
    assert_eq!(measure.cycle_reentries, 0);
}

#[test]
fn prefilter_skips_only_the_param_free_branch_of_a_mixed_root() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let bound = TypeParamId(98_010);
    let mapped = TypeParamId(98_011);
    let bound_parameter = interner.intern_type_param(bound, "P");
    let mapped_parameter = interner.intern_type_param(mapped, "T");
    let method = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: bound,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: Vec::new(),
        ret: bound_parameter,
    });
    let shadowed = interner.intern_object(ObjectType {
        properties: vec![prop("method", method)],
        ..Default::default()
    });
    let root = interner.intern_object(ObjectType {
        properties: vec![prop("u", shadowed), prop("w", mapped_parameter)],
        ..Default::default()
    });
    let expected = interner.intern_object(ObjectType {
        properties: vec![prop("u", shadowed), prop("w", wk.number)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(mapped, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, root);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(result, expected);
    let rewritten = interner
        .store()
        .object_type(result)
        .expect("the mixed root must remain an object");
    assert_eq!(
        rewritten.property("u").map(|property| property.ty),
        Some(shadowed),
        "the param-free branch keeps its original id"
    );
    assert_eq!(
        rewritten.property("w").map(|property| property.ty),
        Some(wk.number)
    );
    assert!(
        measure.prefilter_skips >= 1,
        "the shadowed branch is skipped without a walk"
    );
    assert!(
        measure.apply_visits <= 4,
        "only the root and its mapped leaf are visited (saw {} visits)",
        measure.apply_visits
    );
}

#[test]
fn prefilter_keeps_lazy_instantiations_with_param_free_args_untouched() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(98_015);
    let parameter = interner.intern_type_param(id, "T");
    let base = interner.intern_object(ObjectType {
        properties: vec![prop("body", parameter)],
        ..Default::default()
    });
    // The base contains a free T, but substitution composes into argument
    // *values* only — and the stored value is already param-free.
    let instantiation = interner.intern_instantiation(base, vec![(id, wk.string)]);
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, instantiation);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(
        result, instantiation,
        "the base stays lazy, so a param-free argument list means identity"
    );
    assert_eq!(measure.apply_visits, 0);
    assert!(measure.prefilter_skips >= 1);
}

#[test]
fn prefilter_accounts_for_binders_blocked_at_the_lookup_position() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let blocked = TypeParamId(98_020);
    let mapped = TypeParamId(98_021);
    let blocked_parameter = interner.intern_type_param(blocked, "P");
    let mapped_parameter = interner.intern_type_param(mapped, "T");
    let inner = interner.intern_object(ObjectType {
        properties: vec![prop("v", blocked_parameter)],
        ..Default::default()
    });
    // The function's free set is {T} (its own binder P is removed), so the
    // root is walked — but at the `x` parameter P is blocked, emptying the
    // subtree's effective free set mid-walk.
    let function = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: blocked,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![
            ParameterType::required("x", inner),
            ParameterType::required("y", mapped_parameter),
        ],
        ret: wk.void,
    });
    let expected = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: blocked,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![
            ParameterType::required("x", inner),
            ParameterType::required("y", wk.string),
        ],
        ret: wk.void,
    });
    let map = FxHashMap::from_iter([(blocked, wk.number), (mapped, wk.string)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, function);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(result, expected);
    let rewritten = interner
        .store()
        .function_type(result)
        .expect("the blocked-position witness must remain a function");
    assert_eq!(
        rewritten.params[0].ty, inner,
        "the subtree mentioning only the blocked P keeps its original id"
    );
    assert_eq!(rewritten.params[1].ty, wk.string);
    // Control (measure already snapshotted): the same subtree outside the
    // binder rewrites — the skip is a property of the blocked context, not of
    // the subtree.
    let expected_standalone = interner.intern_object(ObjectType {
        properties: vec![prop("v", wk.number)],
        ..Default::default()
    });
    let control = Substitution::new(&map).apply(&mut interner, inner);
    assert_eq!(control, expected_standalone);
    assert!(
        measure.prefilter_skips >= 1,
        "`inner` is skipped while P is blocked"
    );
    assert!(
        measure.apply_visits <= 3,
        "the blocked subtree is never walked (saw {} visits)",
        measure.apply_visits
    );
    assert_eq!(
        measure.blocked_type_param_hits, 0,
        "no blocked leaf is ever reached"
    );
}

#[test]
fn internal_cycle_fold_collapses_diamond_ladder_above_self_contained_cycle() {
    const LEVELS: usize = 16;
    const LEVELS_U64: u64 = 16;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(98_030);
    let parameter = interner.intern_type_param(id, "T");
    // A self-contained cycle at the very bottom: `inner`'s frame opens and
    // closes entirely inside `bottom`'s subtree, so every ladder entry's
    // re-entry record reduces to the empty set once `inner` is folded into
    // `visited` — the entries then validate by the `visited` check alone.
    let inner = interner.reserve_object();
    interner.fill_object(
        inner,
        ObjectType {
            properties: vec![prop("self", inner), prop("v", parameter)],
            ..Default::default()
        },
    );
    let bottom = interner.intern_object(ObjectType {
        properties: vec![prop("c", inner)],
        ..Default::default()
    });
    let mut level = bottom;
    for _ in 0..LEVELS {
        level = interner.intern_object(ObjectType {
            properties: vec![prop("a", level), prop("b", level)],
            ..Default::default()
        });
    }
    let top = interner.intern_object(ObjectType {
        properties: vec![prop("l", level)],
        ..Default::default()
    });
    // NOTE: the ladder reaches T through `inner`, so the param-relevant
    // prefilter must NOT skip any of it — the two features compose here: the
    // whole ladder is walked, and the fold refinement bounds the walk.
    let expected_inner = interner.intern_object(ObjectType {
        properties: vec![prop("self", inner), prop("v", wk.number)],
        ..Default::default()
    });
    let mut expected_level = interner.intern_object(ObjectType {
        properties: vec![prop("c", expected_inner)],
        ..Default::default()
    });
    for _ in 0..LEVELS {
        expected_level = interner.intern_object(ObjectType {
            properties: vec![prop("a", expected_level), prop("b", expected_level)],
            ..Default::default()
        });
    }
    let expected = interner.intern_object(ObjectType {
        properties: vec![prop("l", expected_level)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, top);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(result, expected);
    let rewritten = interner
        .store()
        .object_type(result)
        .expect("the ladder root must remain an object");
    let first_level = interner
        .store()
        .object_type(
            rewritten
                .property("l")
                .expect("the root keeps its ladder edge")
                .ty,
        )
        .expect("the top ladder level must remain an object");
    assert_eq!(
        first_level.property("a").map(|property| property.ty),
        first_level.property("b").map(|property| property.ty),
        "both diamond edges of a level share one rewritten node"
    );
    assert!(
        measure.apply_visits <= 16 * LEVELS_U64,
        "the ladder must be walked linearly, not as a 2^N tree (saw {} visits)",
        measure.apply_visits
    );
    assert!(
        measure.tainted_memo_hits >= LEVELS_U64 - 1,
        "each level's second diamond edge reuses the first edge's entry"
    );
    assert!(
        measure.tainted_memo_stale_misses <= 32,
        "folded entries stay valid after the internal cycle closed (saw {} stale misses)",
        measure.tainted_memo_stale_misses
    );
}

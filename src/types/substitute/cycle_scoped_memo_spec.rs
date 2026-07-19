//! Acceptance spec for a cycle-scoped tainted-result memo in `Substitution`.
//!
//! Activate from `substitute/mod.rs` only with the implementation. Alongside the
//! run-wide `completed` memo, a run keeps a tainted memo with the identical key
//! `(TypeId, canonical blocked context)`. A frame whose subtree observed at least
//! one raw-id re-entry (or consumed a tainted entry) stores its result there
//! together with its **dependency record**: the set of ids it re-entered and the
//! set of ids its walk passed through while they were *not* in progress (both
//! closed over consumed memo entries). Lookup order is re-entry guard →
//! `completed` → tainted memo, and a tainted entry is reusable only when its
//! record still describes the current stack exactly: every re-entered id is
//! still in progress, and no id it walked plainly is in progress now. Anything
//! else is a stale miss and recomputes (a fresh walk would cut the graph at
//! different frames, so the stored result would be wrong). A tainted hit taints
//! the consumer like the recorded re-entries would (cycle epoch bumps; the
//! record folds into the consumer frame), so tainted results still never enter
//! `completed` — a *clean* result observed no re-entry, hence its closure is
//! acyclic and can never be cut by a live frame, which is why the run-wide
//! `completed` memo needs no such record. Every substitution result is
//! byte-for-byte the memo-less walk's result; only visit counts change.

use super::*;
use crate::types::repr::{ObjectType, PropertyType};

fn prop(name: impl Into<String>, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

#[test]
fn cycle_scoped_memo_collapses_diamond_fanout_inside_live_cycle() {
    const LEVELS: usize = 18;
    const LEVELS_U64: u64 = 18;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(97_000);
    let parameter = interner.intern_type_param(id, "T");
    let recursive = interner.reserve_object();
    // A diamond ladder under one raw-id cycle: with only the `completed` memo the
    // cycle taints every level, so the DAG is walked as a 2^LEVELS tree.
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
    let chain_head = level;
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("next", chain_head), prop("t", parameter)],
            ..Default::default()
        },
    );
    // Every back edge returns the original `recursive`, so no level rewrites;
    // only the cycle root's own `t` member changes.
    let expected = interner.intern_object(ObjectType {
        properties: vec![prop("next", chain_head), prop("t", wk.number)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, recursive);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_ne!(result, recursive);
    assert_eq!(result, expected);
    let rewritten = interner
        .store()
        .object_type(result)
        .expect("the cycle root must remain an object");
    assert_eq!(
        rewritten.property("next").map(|property| property.ty),
        Some(chain_head),
        "unchanged diamond levels keep their original ids"
    );
    assert_eq!(
        rewritten.property("t").map(|property| property.ty),
        Some(wk.number)
    );
    assert!(
        measure.apply_visits <= 16 * LEVELS_U64,
        "the diamond ladder must be walked linearly, not as a 2^N tree (saw {} visits)",
        measure.apply_visits
    );
    assert!(
        measure.tainted_memo_hits >= LEVELS_U64 - 1,
        "each level's second diamond edge reuses the first edge's tainted result"
    );
    assert!(
        measure.cycle_reentries <= 2 * LEVELS_U64,
        "with reuse, each level's back edge is walked a bounded number of times"
    );
}

#[test]
fn cycle_scoped_tainted_reuse_dies_with_its_cycle_root() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(97_005);
    let parameter = interner.intern_type_param(id, "T");
    let a = interner.reserve_object();
    let b = interner.intern_object(ObjectType {
        properties: vec![prop("back", a), prop("p", parameter)],
        ..Default::default()
    });
    interner.fill_object(
        a,
        ObjectType {
            properties: vec![prop("x", b), prop("t", parameter)],
            ..Default::default()
        },
    );
    let root = interner.intern_tuple(vec![a, b]);
    // Element 0 rewrites `b` while `a` is in progress; element 1 rewrites the
    // same `b` after `a`'s frame popped, where the correct result differs — the
    // standalone walk re-enters `b` itself instead of `a`.
    let expected_b_inside_a = interner.intern_object(ObjectType {
        properties: vec![prop("back", a), prop("p", wk.number)],
        ..Default::default()
    });
    let expected_a = interner.intern_object(ObjectType {
        properties: vec![prop("x", expected_b_inside_a), prop("t", wk.number)],
        ..Default::default()
    });
    let expected_a_inside_b = interner.intern_object(ObjectType {
        properties: vec![prop("x", b), prop("t", wk.number)],
        ..Default::default()
    });
    let expected_b = interner.intern_object(ObjectType {
        properties: vec![prop("back", expected_a_inside_b), prop("p", wk.number)],
        ..Default::default()
    });
    let expected = interner.intern_tuple(vec![expected_a, expected_b]);
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, root);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_ne!(
        expected_b, expected_b_inside_a,
        "the standalone rewrite of `b` must differ from its inside-the-cycle rewrite"
    );
    assert_eq!(
        result, expected,
        "a tainted entry must not be reused after its cycle root's frame popped"
    );
    let tuple = interner
        .store()
        .tuple_type(result)
        .expect("the eviction witness must remain a tuple");
    assert_eq!(tuple.elements[0], expected_a);
    assert_eq!(tuple.elements[1], expected_b);
    assert_eq!(
        measure.tainted_memo_hits, 0,
        "no live diamond exists here — any hit would be stale reuse"
    );
    assert!(
        measure.tainted_memo_stale_misses >= 1,
        "element 1 must reject the dead-cycle-root entries instead of reusing them"
    );
}

#[test]
fn cycle_scoped_memo_reuses_param_bearing_shared_node_across_diamond_paths() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(97_010);
    let parameter = interner.intern_type_param(id, "T");
    let recursive = interner.reserve_object();
    let shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", recursive), prop("v", parameter)],
        ..Default::default()
    });
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("a", shared), prop("b", shared)],
            ..Default::default()
        },
    );
    let expected_shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", recursive), prop("v", wk.number)],
        ..Default::default()
    });
    let expected = interner.intern_object(ObjectType {
        properties: vec![prop("a", expected_shared), prop("b", expected_shared)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, recursive);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(result, expected);
    let rewritten = interner
        .store()
        .object_type(result)
        .expect("the diamond root must remain an object");
    let first = rewritten.property("a").map(|property| property.ty);
    let second = rewritten.property("b").map(|property| property.ty);
    assert_eq!(first, Some(expected_shared));
    assert_eq!(first, second, "both diamond paths share one rewritten node");
    assert!(measure.tainted_memo_entries >= 1);
    assert_eq!(
        measure.tainted_memo_hits, 1,
        "the b path reuses the a path's tainted result while `recursive` is live"
    );
    assert!(
        measure.apply_visits <= 12,
        "the shared node must not be re-walked (saw {} visits)",
        measure.apply_visits
    );
}

#[test]
fn cycle_scoped_reuse_taints_the_consumer_frame() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(97_015);
    let parameter = interner.intern_type_param(id, "T");
    let recursive = interner.reserve_object();
    let shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", recursive), prop("v", parameter)],
        ..Default::default()
    });
    let wrapper = interner.intern_object(ObjectType {
        properties: vec![prop("s", shared)],
        ..Default::default()
    });
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("a", shared), prop("w", wrapper), prop("t", parameter)],
            ..Default::default()
        },
    );
    let root = interner.intern_tuple(vec![recursive, wrapper]);
    // Element 0: `wrapper` rewrites while `recursive` is in progress, consuming
    // the tainted `shared` result — it must stay out of the run-wide `completed`.
    let expected_shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", recursive), prop("v", wk.number)],
        ..Default::default()
    });
    let expected_wrapper = interner.intern_object(ObjectType {
        properties: vec![prop("s", expected_shared)],
        ..Default::default()
    });
    let expected_recursive = interner.intern_object(ObjectType {
        properties: vec![
            prop("a", expected_shared),
            prop("w", expected_wrapper),
            prop("t", wk.number),
        ],
        ..Default::default()
    });
    // Element 1: standalone, `recursive` is not on the stack — its members
    // re-enter the live `shared`/`wrapper` frames and return the originals.
    let expected_inner_recursive = interner.intern_object(ObjectType {
        properties: vec![prop("a", shared), prop("w", wrapper), prop("t", wk.number)],
        ..Default::default()
    });
    let expected_standalone_shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", expected_inner_recursive), prop("v", wk.number)],
        ..Default::default()
    });
    let expected_standalone_wrapper = interner.intern_object(ObjectType {
        properties: vec![prop("s", expected_standalone_shared)],
        ..Default::default()
    });
    let expected = interner.intern_tuple(vec![expected_recursive, expected_standalone_wrapper]);
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, root);

    assert_ne!(
        expected_standalone_wrapper, expected_wrapper,
        "publishing element 0's tainted-hit consumer would leak it into element 1"
    );
    assert_eq!(
        result, expected,
        "a tainted-memo hit must taint its consumer out of the `completed` memo"
    );
    let tuple = interner
        .store()
        .tuple_type(result)
        .expect("the taint-propagation witness must remain a tuple");
    assert_eq!(tuple.elements[0], expected_recursive);
    assert_eq!(tuple.elements[1], expected_standalone_wrapper);
}

#[test]
fn cycle_scoped_memo_handles_non_tracking_container_arms() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(97_020);
    let parameter = interner.intern_type_param(id, "T");
    let recursive = interner.reserve_object();
    let shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", recursive), prop("v", parameter)],
        ..Default::default()
    });
    // Arrays never enter `in_progress`, so the diamond reuse must work for
    // container arms that are visits without being cycle frames.
    let shared_list = interner.intern_array(shared);
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("a", shared_list), prop("b", shared_list)],
            ..Default::default()
        },
    );
    let expected_shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", recursive), prop("v", wk.number)],
        ..Default::default()
    });
    let expected_list = interner.intern_array(expected_shared);
    let expected = interner.intern_object(ObjectType {
        properties: vec![prop("a", expected_list), prop("b", expected_list)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, recursive);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(result, expected);
    let rewritten = interner
        .store()
        .object_type(result)
        .expect("the array-diamond root must remain an object");
    let first = rewritten.property("a").map(|property| property.ty);
    let second = rewritten.property("b").map(|property| property.ty);
    assert_eq!(first, Some(expected_list));
    assert_eq!(first, second);
    assert_eq!(
        interner
            .store()
            .array_type(expected_list)
            .map(|array| array.element),
        Some(expected_shared)
    );
    assert!(
        measure.tainted_memo_hits >= 1,
        "the second array occurrence (or the object beneath it) must reuse"
    );
    assert!(
        measure.apply_visits <= 14,
        "the array diamond must not be re-walked (saw {} visits)",
        measure.apply_visits
    );
}

#[test]
fn cycle_scoped_reuse_respects_newly_live_reachable_frames() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(97_030);
    let parameter = interner.intern_type_param(id, "T");
    let root = interner.reserve_object();
    let c = interner.reserve_object();
    let x = interner.intern_object(ObjectType {
        properties: vec![prop("back", root), prop("c", c), prop("p", parameter)],
        ..Default::default()
    });
    interner.fill_object(
        c,
        ObjectType {
            properties: vec![prop("back2", root), prop("q", parameter), prop("x2", x)],
            ..Default::default()
        },
    );
    interner.fill_object(
        root,
        ObjectType {
            properties: vec![prop("first", x), prop("second", c)],
            ..Default::default()
        },
    );
    // `first` computes `x` while only `root` is live: `c` is walked plainly and
    // rewrites. `second` then applies `c`, so `x`'s stored result is looked up
    // while `c` — a node `x` reaches — is *newly* in progress: a fresh walk
    // re-enters `c` and keeps it original, so the stored result is stale and
    // must be rejected even though its re-entered cycle roots are all still live.
    let expected_c_inside_first = interner.intern_object(ObjectType {
        properties: vec![prop("back2", root), prop("q", wk.number), prop("x2", x)],
        ..Default::default()
    });
    let expected_x_first = interner.intern_object(ObjectType {
        properties: vec![
            prop("back", root),
            prop("c", expected_c_inside_first),
            prop("p", wk.number),
        ],
        ..Default::default()
    });
    let expected_x_inside_second = interner.intern_object(ObjectType {
        properties: vec![prop("back", root), prop("c", c), prop("p", wk.number)],
        ..Default::default()
    });
    let expected_c_second = interner.intern_object(ObjectType {
        properties: vec![
            prop("back2", root),
            prop("q", wk.number),
            prop("x2", expected_x_inside_second),
        ],
        ..Default::default()
    });
    let expected = interner.intern_object(ObjectType {
        properties: vec![
            prop("first", expected_x_first),
            prop("second", expected_c_second),
        ],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let mut substitution = Substitution::new(&map);
    let result = substitution.apply(&mut interner, root);
    let measure = substitution_measure().expect("the counter scope must remain enabled");

    assert_ne!(
        expected_x_first, expected_x_inside_second,
        "the two occurrences of `x` must rewrite differently — reuse would conflate them"
    );
    assert_eq!(
        result, expected,
        "a tainted entry whose walked-plainly node is now in progress must recompute"
    );
    let rewritten = interner
        .store()
        .object_type(result)
        .expect("the newly-live-frame witness must remain an object");
    assert_eq!(
        rewritten.property("first").map(|property| property.ty),
        Some(expected_x_first)
    );
    assert_eq!(
        rewritten.property("second").map(|property| property.ty),
        Some(expected_c_second)
    );
    assert_eq!(
        measure.tainted_memo_hits, 0,
        "every stored entry here is invalid at its next lookup"
    );
}

#[test]
fn cycle_scoped_memo_lifetime_is_one_substitution_run() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(97_025);
    let parameter = interner.intern_type_param(id, "T");
    let recursive = interner.reserve_object();
    let shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", recursive), prop("v", parameter)],
        ..Default::default()
    });
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("a", shared), prop("b", shared)],
            ..Default::default()
        },
    );
    let expected_shared = interner.intern_object(ObjectType {
        properties: vec![prop("r", recursive), prop("v", wk.number)],
        ..Default::default()
    });
    let expected = interner.intern_object(ObjectType {
        properties: vec![prop("a", expected_shared), prop("b", expected_shared)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let _scope = start_substitution_measure();
    let first = Substitution::new(&map).apply(&mut interner, recursive);
    let after_first = substitution_measure().expect("the counter scope must remain enabled");
    let second = Substitution::new(&map).apply(&mut interner, recursive);
    let after_second = substitution_measure().expect("the counter scope must remain enabled");

    assert_eq!(first, expected);
    assert_eq!(second, first, "results must be identical across runs");
    assert_eq!(after_first.runs, 1);
    assert_eq!(after_first.tainted_memo_hits, 1);
    assert_eq!(after_second.runs, 2);
    assert_eq!(
        after_second.tainted_memo_hits, 2,
        "the second Substitution rebuilds its own tainted entries — nothing leaks across runs"
    );
    assert_eq!(
        after_second.tainted_memo_entries,
        after_first.tainted_memo_entries * 2,
        "each run stores the same tainted entries afresh"
    );
}

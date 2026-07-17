//! Disabled acceptance spec for default-off substitution measurement scopes.
//!
//! Activate from `substitute/mod.rs` only with the implementation. Test-only
//! current-collector state is `Option<Rc<RefCell<SubstitutionMeasure>>>` (or an
//! equivalent stable identity) and starts as `None` on every thread.
//! `start_substitution_measure()` installs a fresh collector and returns a
//! `SubstitutionMeasureScope` RAII guard that restores the prior collector on
//! drop. `substitution_measure()` clones a snapshot: `None` means disabled, while
//! `Some` means the current scope is enabled even when all counters are still zero.
//!
//! This boundary keeps the exact WU0B fresh-process probe free of substitution
//! telemetry unless that probe explicitly opts in.
//!
//! ## Binding implementation contract
//!
//! `Substitution::new` clones the current collector identity into that run and
//! increments `runs` there exactly once. Every instrumentation site writes the
//! captured collector, never whichever collector is currently installed in TLS;
//! a nested scope cannot redirect an outer-owned run. A disabled run branches on
//! its captured `None` before any measurement `LocalKey::with`, telemetry
//! exact-context `Vec` build/sort, or measurement set/map mutation. Behavioral
//! counters cannot prove this zero-work property, so activation review must
//! inspect every instrumentation site.
//!
//! Remove `reset_substitution_measure`; do not retain an ambient compatibility
//! shim. Every legacy counter test in `tests.rs` and `completed_memo_spec.rs` must
//! hold a `SubstitutionMeasureScope` and read an explicit `Some` snapshot. The
//! captured `Rc` collector or an explicit marker such as `PhantomData<Rc<()>>`
//! must make the guard structurally `!Send`, so its `Drop` cannot restore another
//! thread's TLS state. This crate has no direct negative-trait assertion
//! dependency, so activation review must confirm that property.

use super::*;
use crate::types::repr::{ObjectType, PropertyType};

fn prop(name: impl Into<String>, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn substitute_shared_fanout(fanout: usize) {
    assert!(fanout > 0, "the shared-DAG witness needs at least one edge");

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(97_000);
    let parameter = interner.intern_type_param(id, "T");
    let shared = interner.intern_object(ObjectType {
        properties: vec![prop("value", parameter)],
        ..Default::default()
    });
    let root = interner.intern_tuple(vec![shared; fanout]);
    let expected_shared = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let expected = interner.intern_tuple(vec![expected_shared; fanout]);
    let map = FxHashMap::from_iter([(id, wk.number)]);

    let result = Substitution::new(&map).apply(&mut interner, root);

    assert_eq!(result, expected, "measurement must not change substitution");
    let tuple = interner
        .store()
        .tuple_type(result)
        .expect("the shared-DAG result must remain a tuple");
    assert!(tuple
        .elements
        .iter()
        .all(|&element| element == expected_shared));
}

fn assert_exact_shared_fanout_measure(measure: &SubstitutionMeasure, fanout: usize) {
    let fanout = u64::try_from(fanout).expect("test fanout must fit the telemetry counters");

    assert_eq!(measure.runs, 1);
    assert_eq!(measure.apply_visits, fanout + 2);
    assert_eq!(measure.type_id_repeats, fanout - 1);
    assert_eq!(measure.exact_context_repeats, fanout - 1);
    assert_eq!(measure.type_param_map_hits, 1);
    assert_eq!(measure.blocked_type_param_hits, 0);
    assert_eq!(measure.cycle_reentries, 0);
    assert_eq!(
        measure.completed_memo_entries, 3,
        "the tuple, shared object, and mapped leaf are all completed memo entries"
    );
    assert_eq!(measure.completed_memo_hits, fanout - 1);
    assert_eq!(measure.cycle_tainted_skips, 0);
    assert_eq!(
        measure.apply_visits,
        measure.completed_memo_entries + measure.completed_memo_hits,
        "every acyclic visit either completes a context or reuses it"
    );
}

fn assert_exact_reused_run_measure(
    measure: &SubstitutionMeasure,
    fanout: usize,
    apply_count: usize,
) {
    assert!(apply_count > 0, "the run witness needs at least one apply");
    let fanout = u64::try_from(fanout).expect("test fanout must fit the telemetry counters");
    let apply_count =
        u64::try_from(apply_count).expect("test apply count must fit the telemetry counters");

    assert_eq!(measure.runs, 1);
    assert_eq!(measure.apply_visits, fanout + apply_count + 1);
    assert_eq!(measure.type_id_repeats, fanout + apply_count - 2);
    assert_eq!(measure.exact_context_repeats, fanout + apply_count - 2);
    assert_eq!(measure.type_param_map_hits, 1);
    assert_eq!(measure.blocked_type_param_hits, 0);
    assert_eq!(measure.cycle_reentries, 0);
    assert_eq!(measure.completed_memo_entries, 3);
    assert_eq!(measure.completed_memo_hits, fanout + apply_count - 2);
    assert_eq!(measure.cycle_tainted_skips, 0);
    assert_eq!(
        measure.apply_visits,
        measure.completed_memo_entries + measure.completed_memo_hits
    );
}

fn assert_shared_fanout_delta(
    before: &SubstitutionMeasure,
    after: &SubstitutionMeasure,
    fanout: usize,
) {
    let fanout = u64::try_from(fanout).expect("test fanout must fit the telemetry counters");

    assert_eq!(after.runs, before.runs + 1);
    assert_eq!(after.apply_visits, before.apply_visits + fanout + 2);
    assert_eq!(
        after.completed_memo_entries,
        before.completed_memo_entries + 3
    );
    assert_eq!(
        after.completed_memo_hits,
        before.completed_memo_hits + fanout - 1
    );
    assert_eq!(after.type_param_map_hits, before.type_param_map_hits + 1);
    assert_eq!(
        after.blocked_type_param_hits,
        before.blocked_type_param_hits
    );
    assert_eq!(after.cycle_reentries, before.cycle_reentries);
    assert_eq!(after.cycle_tainted_skips, before.cycle_tainted_skips);
    // Fresh interners may reuse numeric TypeIds, so raw repeat deltas are not stable here.
}

fn in_fresh_thread(body: impl FnOnce() + Send + 'static) {
    std::thread::spawn(body)
        .join()
        .expect("the measurement-scope witness thread must finish");
}

#[test]
fn fresh_thread_keeps_unscoped_substitution_measurement_disabled_for_wu0b() {
    const FANOUT: usize = 64;

    in_fresh_thread(|| {
        assert_eq!(substitution_measure(), None);
        substitute_shared_fanout(FANOUT);
        assert_eq!(
            substitution_measure(),
            None,
            "ordinary substitution must neither allocate nor enable TLS telemetry"
        );
    });
}

#[test]
fn scoped_substitution_measurement_preserves_exact_completed_memo_counters() {
    const FANOUT: usize = 64;

    in_fresh_thread(|| {
        let _scope = start_substitution_measure();
        assert_eq!(
            substitution_measure(),
            Some(SubstitutionMeasure::default()),
            "Some(default) distinguishes a fresh enabled scope from disabled None"
        );

        substitute_shared_fanout(FANOUT);

        let measure = substitution_measure().expect("the scope must remain enabled");
        assert_exact_shared_fanout_measure(&measure, FANOUT);
    });
}

#[test]
fn dropping_substitution_measurement_scope_restores_disabled_state() {
    const FANOUT: usize = 8;

    in_fresh_thread(|| {
        let scope = start_substitution_measure();
        substitute_shared_fanout(FANOUT);
        assert!(substitution_measure().is_some());

        drop(scope);

        assert_eq!(substitution_measure(), None);
        substitute_shared_fanout(FANOUT);
        assert_eq!(
            substitution_measure(),
            None,
            "work after scope drop must remain unmeasured"
        );
    });
}

#[test]
fn substitution_run_keeps_its_outer_collector_while_nested_scope_is_current() {
    const FANOUT: usize = 9;

    in_fresh_thread(|| {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let id = TypeParamId(97_010);
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

        let outer_scope = start_substitution_measure();
        let mut substitution = Substitution::new(&map);
        let mut constructed = SubstitutionMeasure::default();
        constructed.runs = 1;
        assert_eq!(substitution_measure(), Some(constructed));

        assert_eq!(substitution.apply(&mut interner, root), expected);
        let after_first = substitution_measure().expect("the outer scope must be enabled");
        assert_exact_reused_run_measure(&after_first, FANOUT, 1);

        {
            let _nested_scope = start_substitution_measure();
            assert_eq!(substitution_measure(), Some(SubstitutionMeasure::default()));
            assert_eq!(substitution.apply(&mut interner, root), expected);
            assert_eq!(
                substitution_measure(),
                Some(SubstitutionMeasure::default()),
                "an outer-owned run must not write the current nested collector"
            );
        }

        let after_nested = substitution_measure().expect("the outer collector must be restored");
        assert_exact_reused_run_measure(&after_nested, FANOUT, 2);

        assert_eq!(substitution.apply(&mut interner, root), expected);
        let after_third = substitution_measure().expect("the outer scope must remain enabled");
        assert_exact_reused_run_measure(&after_third, FANOUT, 3);

        drop(substitution);
        drop(outer_scope);
        assert_eq!(substitution_measure(), None);
    });
}

#[test]
fn nested_substitution_measurement_is_fresh_and_restores_outer_without_merging() {
    const OUTER_FANOUT: usize = 8;
    const INNER_FANOUT: usize = 17;
    const CONTINUATION_FANOUT: usize = 23;

    in_fresh_thread(|| {
        let outer_scope = start_substitution_measure();
        substitute_shared_fanout(OUTER_FANOUT);
        let outer_before = substitution_measure().expect("the outer scope must be enabled");
        assert_exact_shared_fanout_measure(&outer_before, OUTER_FANOUT);

        {
            let _inner_scope = start_substitution_measure();
            assert_eq!(
                substitution_measure(),
                Some(SubstitutionMeasure::default()),
                "a nested scope starts fresh instead of inheriting outer counters"
            );
            substitute_shared_fanout(INNER_FANOUT);
            let inner = substitution_measure().expect("the nested scope must be enabled");
            assert_exact_shared_fanout_measure(&inner, INNER_FANOUT);
            assert_ne!(inner, outer_before);
        }

        assert_eq!(
            substitution_measure().as_ref(),
            Some(&outer_before),
            "dropping the nested scope restores, rather than merges into, the outer measure"
        );
        substitute_shared_fanout(CONTINUATION_FANOUT);
        let outer_after = substitution_measure().expect("the restored outer scope must continue");
        assert_shared_fanout_delta(&outer_before, &outer_after, CONTINUATION_FANOUT);
        drop(outer_scope);
        assert_eq!(substitution_measure(), None);
    });
}

#[test]
fn substitution_measurement_scope_restores_outer_state_during_unwind() {
    const OUTER_FANOUT: usize = 5;
    const INNER_FANOUT: usize = 11;
    const CONTINUATION_FANOUT: usize = 19;

    in_fresh_thread(|| {
        let outer_scope = start_substitution_measure();
        substitute_shared_fanout(OUTER_FANOUT);
        let outer_before = substitution_measure().expect("the outer scope must be enabled");
        assert_exact_shared_fanout_measure(&outer_before, OUTER_FANOUT);

        let unwind = std::panic::catch_unwind(|| {
            let _inner_scope = start_substitution_measure();
            substitute_shared_fanout(INNER_FANOUT);
            let inner = substitution_measure().expect("the nested scope must be enabled");
            assert_exact_shared_fanout_measure(&inner, INNER_FANOUT);
            panic!("test-only unwind after nested measurement");
        });

        assert!(unwind.is_err());
        assert_eq!(
            substitution_measure().as_ref(),
            Some(&outer_before),
            "the RAII guard must restore the outer measure while unwinding"
        );
        substitute_shared_fanout(CONTINUATION_FANOUT);
        let outer_after = substitution_measure().expect("the unwound outer scope must continue");
        assert_shared_fanout_delta(&outer_before, &outer_after, CONTINUATION_FANOUT);
        drop(outer_scope);
        assert_eq!(substitution_measure(), None);
    });
}

#[test]
fn simultaneous_threads_keep_substitution_measurement_scopes_isolated() {
    const FIRST_FANOUT: usize = 7;
    const SECOND_FANOUT: usize = 13;

    struct ScopedObservations {
        initial: Option<SubstitutionMeasure>,
        populated: Option<SubstitutionMeasure>,
        while_peer_scoped: Option<SubstitutionMeasure>,
        after_drop: Option<SubstitutionMeasure>,
    }

    struct UnscopedThenScopedObservations {
        initial: Option<SubstitutionMeasure>,
        while_peer_scoped: Option<SubstitutionMeasure>,
        after_unscoped_work: Option<SubstitutionMeasure>,
        populated: Option<SubstitutionMeasure>,
        after_peer_drop: Option<SubstitutionMeasure>,
        after_drop: Option<SubstitutionMeasure>,
    }

    // Generations publish first scope, second scope, first snapshot, then first drop.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let first_barrier = std::sync::Arc::clone(&barrier);
    let first = std::thread::spawn(move || {
        let initial = substitution_measure();
        let scope = start_substitution_measure();
        substitute_shared_fanout(FIRST_FANOUT);
        let populated = substitution_measure();

        first_barrier.wait();
        first_barrier.wait();
        let while_peer_scoped = substitution_measure();
        first_barrier.wait();

        drop(scope);
        let after_drop = substitution_measure();
        first_barrier.wait();
        ScopedObservations {
            initial,
            populated,
            while_peer_scoped,
            after_drop,
        }
    });

    let second_barrier = std::sync::Arc::clone(&barrier);
    let second = std::thread::spawn(move || {
        let initial = substitution_measure();

        second_barrier.wait();
        let while_peer_scoped = substitution_measure();
        substitute_shared_fanout(SECOND_FANOUT);
        let after_unscoped_work = substitution_measure();

        let scope = start_substitution_measure();
        substitute_shared_fanout(SECOND_FANOUT);
        let populated = substitution_measure();
        second_barrier.wait();
        second_barrier.wait();

        second_barrier.wait();
        let after_peer_drop = substitution_measure();
        drop(scope);
        let after_drop = substitution_measure();
        UnscopedThenScopedObservations {
            initial,
            while_peer_scoped,
            after_unscoped_work,
            populated,
            after_peer_drop,
            after_drop,
        }
    });

    let first = first
        .join()
        .expect("the first isolation thread must finish");
    let second = second
        .join()
        .expect("the second isolation thread must finish");

    assert_eq!(first.initial, None);
    let first_populated = first
        .populated
        .expect("the first thread's scope must be enabled");
    assert_exact_shared_fanout_measure(&first_populated, FIRST_FANOUT);
    assert_eq!(first.while_peer_scoped, Some(first_populated));
    assert_eq!(first.after_drop, None);

    assert_eq!(second.initial, None);
    assert_eq!(second.while_peer_scoped, None);
    assert_eq!(second.after_unscoped_work, None);
    let second_populated = second
        .populated
        .expect("the second thread's scope must be enabled");
    assert_exact_shared_fanout_measure(&second_populated, SECOND_FANOUT);
    assert_eq!(second.after_peer_drop, Some(second_populated));
    assert_eq!(second.after_drop, None);
}

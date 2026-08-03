//! Disabled RED contract for the opt-in, per-`Pass` cycle-tainted application cache.
//! Clean gets first refusal; only completed whole-run `CycleTainted` outcomes publish.

use super::super::context::{
    cycle_tainted_application_cache_measure, eager_application_cache_measure,
    start_cycle_tainted_application_cache_baseline_measure,
    start_cycle_tainted_application_cache_measure, start_eager_application_cache_measure,
    CycleTaintedApplicationCacheMeasure, Pass,
};
use super::super::type_groups::{PublishedTypeEnvironment, TypeEnvironmentState};
use crate::binder::Binder;
use crate::check::test_support::{check_source, CheckOutput};
use crate::types::repr::{
    FunctionType, GenericTypeParam, MappedType, ModifierOp, ObjectType, ParameterType,
    PropertyType, TypeParamId,
};
use crate::types::store::TypeId;
use crate::types::{substitute_with_outcome, Interner, SubstitutionOutcome};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rustc_hash::FxHashMap;

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn empty_binder() -> Binder {
    let prelude_allocator = Allocator::default();
    let user_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let user = Parser::new(&user_allocator, "", SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty() && user.diagnostics.is_empty());
    crate::binder::bind_module_with_prelude(&prelude.program, &user.program)
}

fn type_source(relative: &str) -> String {
    std::fs::read_to_string(
        crate::test_support::repository_root()
            .join("crates/typokat-types/src/types")
            .join(relative),
    )
    .expect("type source")
}

fn pass<'a>(interner: &'a mut Interner, binder: &'a Binder) -> Pass<'a, 'static> {
    let mut pass = super::super::build_pass(
        interner,
        binder,
        Vec::new(),
        vec![None; binder.type_groups.len()],
        super::super::context::DeclTypes::new(binder.decl_count),
        0,
    );
    pass.type_environment = TypeEnvironmentState::Published(PublishedTypeEnvironment::empty());
    pass
}

fn apply(
    pass: &mut Pass<'_, 'static>,
    template: TypeId,
    parameters: &[TypeParamId],
    map: &FxHashMap<TypeParamId, TypeId>,
) -> SubstitutionOutcome {
    pass.substitute_ready_type_group_application_with_outcome_for_test(template, parameters, map)
}

fn recursive_object(interner: &mut Interner, param: TypeId, edge: &str) -> TypeId {
    let recursive = interner.reserve_object();
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop(edge, recursive), prop("value", param)],
            ..Default::default()
        },
    );
    recursive
}

fn assert_tainted(outcome: SubstitutionOutcome) -> TypeId {
    match outcome {
        SubstitutionOutcome::CycleTainted(result) => result,
        SubstitutionOutcome::CycleClean(_) => panic!("cycle taint was lost"),
    }
}

fn assert_arithmetic(measure: &CycleTaintedApplicationCacheMeasure) {
    assert_eq!(measure.lookups, measure.hits + measure.misses);
    assert_eq!(
        measure.misses,
        measure.insertions + measure.clean_skips + measure.aborted_runs
    );
}

fn assert_counts(measure: &CycleTaintedApplicationCacheMeasure, expected: (u64, u64, u64, u64)) {
    assert_eq!(
        (
            measure.lookups,
            measure.hits,
            measure.misses,
            measure.insertions
        ),
        expected
    );
}

fn check_current(source: &str) -> CheckOutput {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(!parsed.panicked && parsed.diagnostics.is_empty());
    let mut interner = Interner::with_intrinsics();
    let result = crate::check::checker::check_program(&mut interner, &parsed.program);
    CheckOutput {
        diagnostics: result.diagnostics,
        parse_errors: Vec::new(),
        incomplete: result.incomplete,
    }
}

fn compact(section: &str) -> String {
    section.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let tail = source.split_once(start).expect("section start").1;
    tail.split_once(end).expect("section end").0
}

#[test]
fn default_off_then_clean_cache_precedes_tainted_cache_without_mutating_it() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_100);
    let param = interner.intern_type_param(id, "T");
    let recursive = recursive_object(&mut interner, param, "self");
    let clean = interner.intern_object(ObjectType {
        properties: vec![prop("value", param)],
        ..Default::default()
    });
    let map = FxHashMap::from_iter([(id, wk.string)]);
    let partial = FxHashMap::default();
    let expected = assert_tainted(substitute_with_outcome(&mut interner, recursive, &map));
    {
        let mut default_off = pass(&mut interner, &binder);
        assert!(default_off.cycle_tainted_application_cache.is_none());
        for _ in 0..2 {
            assert_eq!(
                assert_tainted(apply(&mut default_off, recursive, &[id], &map)),
                expected
            );
        }
        assert!(default_off.cycle_tainted_application_cache.is_none());
    }
    assert_eq!(cycle_tainted_application_cache_measure(), None);
    {
        let _baseline_scope = start_cycle_tainted_application_cache_baseline_measure();
        let mut baseline = pass(&mut interner, &binder);
        assert!(baseline.cycle_tainted_application_cache.is_none());
        for _ in 0..2 {
            assert_eq!(
                assert_tainted(apply(&mut baseline, recursive, &[id], &map)),
                expected
            );
        }
        let measure = cycle_tainted_application_cache_measure().expect("baseline measure active");
        assert_eq!((measure.eligible_requests, measure.executed_runs), (2, 2));
        assert_eq!(
            (measure.tainted_cache_hits, measure.tainted_cache_entries),
            (0, 0)
        );
        assert_eq!(measure.tainted_outcomes, 2);
    }
    let _clean_scope = start_eager_application_cache_measure();
    let _tainted_scope = start_cycle_tainted_application_cache_measure();
    let mut pass = pass(&mut interner, &binder);

    let first = apply(&mut pass, recursive, &[id], &map);
    let clean_after_first = eager_application_cache_measure().expect("clean measure active");
    let second = apply(&mut pass, recursive, &[id], &map);
    assert_eq!(assert_tainted(first), assert_tainted(second));
    let clean_after_hit = eager_application_cache_measure().expect("clean measure active");
    assert_eq!(clean_after_hit.lookups, clean_after_first.lookups + 1);
    assert_eq!(clean_after_hit.misses, clean_after_first.misses + 1);
    assert_eq!(
        clean_after_hit.cycle_tainted_skips,
        clean_after_first.cycle_tainted_skips + 1
    );
    assert_eq!(pass.eager_application_cache.len(), 0);

    for _ in 0..2 {
        assert!(matches!(
            apply(&mut pass, clean, &[id], &map),
            SubstitutionOutcome::CycleClean(_)
        ));
    }
    for _ in 0..2 {
        assert_eq!(
            apply(&mut pass, recursive, &[id], &partial),
            SubstitutionOutcome::CycleClean(recursive)
        );
    }
    let clean_measure = eager_application_cache_measure().expect("clean measure active");
    assert_eq!(clean_measure.lookups, 4);
    assert_eq!(clean_measure.hits, 1);
    assert_eq!(clean_measure.misses, 3);
    assert_eq!(clean_measure.insertions, 1);
    assert_eq!(clean_measure.cycle_tainted_skips, 2);
    assert_eq!(clean_measure.unready_bypasses, 2);
    assert_eq!(pass.eager_application_cache.len(), 1);
    let tainted = cycle_tainted_application_cache_measure().expect("tainted measure active");
    assert_eq!((tainted.lookups, tainted.hits, tainted.misses), (3, 1, 2));
    assert_eq!((tainted.insertions, tainted.clean_skips), (1, 1));
    assert_arithmetic(&tainted);
}

#[test]
fn mutual_scc_publishes_each_requested_root_only_after_its_whole_run() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_120);
    let param = interner.intern_type_param(id, "T");
    let left = interner.reserve_object();
    let right = interner.reserve_object();
    interner.fill_object(
        left,
        ObjectType {
            properties: vec![prop("right", right), prop("value", param)],
            ..Default::default()
        },
    );
    interner.fill_object(
        right,
        ObjectType {
            properties: vec![prop("left", left), prop("value", param)],
            ..Default::default()
        },
    );
    let map = FxHashMap::from_iter([(id, wk.number)]);
    let right_oracle = substitute_with_outcome(&mut interner, right, &map);
    let _scope = start_cycle_tainted_application_cache_measure();
    let mut pass = pass(&mut interner, &binder);
    let left_first = apply(&mut pass, left, &[id], &map);
    let left_hit = apply(&mut pass, left, &[id], &map);
    let right_first = apply(&mut pass, right, &[id], &map);
    assert_eq!(assert_tainted(left_first), assert_tainted(left_hit));
    assert_eq!(assert_tainted(right_first), assert_tainted(right_oracle));
    let measure = cycle_tainted_application_cache_measure().expect("scope active");
    assert_counts(&measure, (3, 1, 2, 2));
    assert_arithmetic(&measure);
}

#[test]
fn generic_binder_cross_context_cycle_is_cached_with_fresh_oracle_parity() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_130);
    let free_id = TypeParamId(99_131);
    let parameter = interner.intern_type_param(id, "T");
    // A second mapped param that the binder does NOT shadow keeps the cycle
    // param-relevant, so the substitution prefilter cannot prove it identity.
    let free_parameter = interner.intern_type_param(free_id, "U");
    let recursive = interner.reserve_object();
    let back_edge = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![
            ParameterType::required("self", recursive),
            ParameterType::required("keep", free_parameter),
        ],
        ret: parameter,
    });
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("a_cycle", back_edge), prop("z_mapped", parameter)],
            ..Default::default()
        },
    );
    let array = interner.intern_array(recursive);
    let root = interner.intern_tuple(vec![array]);
    let map = FxHashMap::from_iter([(id, wk.number), (free_id, wk.boolean)]);
    let oracle = substitute_with_outcome(&mut interner, root, &map);
    let _scope = start_cycle_tainted_application_cache_measure();
    let mut pass = pass(&mut interner, &binder);
    let first = apply(&mut pass, root, &[id, free_id], &map);
    let hit = apply(&mut pass, root, &[id, free_id], &map);
    assert_eq!(assert_tainted(first), assert_tainted(oracle));
    assert_eq!(assert_tainted(hit), assert_tainted(oracle));
    let measure = cycle_tainted_application_cache_measure().expect("scope active");
    assert_eq!(
        (measure.lookups, measure.hits, measure.insertions),
        (2, 1, 1)
    );
    assert_arithmetic(&measure);
}

#[test]
fn nested_mapped_distribution_and_exact_complete_key_stay_whole_run_scoped() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let a = TypeParamId(99_140);
    let b = TypeParamId(99_141);
    let a_ty = interner.intern_type_param(a, "A");
    let recursive = recursive_object(&mut interner, a_ty, "self");
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: a_ty,
        value_template: recursive,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let root = interner.intern_object(ObjectType {
        properties: vec![prop("mapped", mapped)],
        ..Default::default()
    });
    let ab = FxHashMap::from_iter([
        (a, interner.union(vec![wk.string, wk.number])),
        (b, wk.boolean),
    ]);
    let reverse_insert = FxHashMap::from_iter([(b, wk.boolean), (a, ab[&a])]);
    let _scope = start_cycle_tainted_application_cache_measure();
    let mut pass = pass(&mut interner, &binder);
    for (template, params, map) in [
        (root, &[a, b][..], &ab),
        (root, &[a, b][..], &reverse_insert),
        (root, &[b, a][..], &ab),
    ] {
        assert_tainted(apply(&mut pass, template, params, map));
    }
    let measure = cycle_tainted_application_cache_measure().expect("scope active");
    assert_counts(&measure, (3, 1, 2, 2));
}

#[test]
fn checker_path_completes_defaults_and_validates_each_clean_occurrence() {
    const SOURCE: &str = "
interface Candidate<T = string> { self: Candidate<T>; value: T }
declare const invalid: Candidate<string, number>;
declare const omitted: Candidate;
declare const explicit: Candidate<string>;
declare const again: Candidate<string>;
let bad: Candidate<number> = explicit;
";
    let default_off = check_source(SOURCE);
    let _scope = start_cycle_tainted_application_cache_measure();
    let cached = check_current(SOURCE);
    assert_eq!(cached.diagnostics, default_off.diagnostics);
    assert_eq!(cached.incomplete, default_off.incomplete);
    assert!(!cached.diagnostics.is_empty());
    let measure = cycle_tainted_application_cache_measure().expect("scope active");
    assert!(measure.eligible_requests > 0 && measure.executed_runs > 0);
    assert!(measure.clean_skips > 0);
    assert_eq!((measure.insertions, measure.hits), (0, 0));
    assert_arithmetic(&measure);
}

#[test]
fn pass_and_universe_isolation_hold_even_when_numeric_ids_collide() {
    let binder = empty_binder();
    let _scope = start_cycle_tainted_application_cache_measure();
    let mut observations = Vec::new();
    for (edge, use_string) in [("left", true), ("right", false)] {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let id = TypeParamId(99_150);
        let parameter = interner.intern_type_param(id, "T");
        let template = recursive_object(&mut interner, parameter, edge);
        let argument = if use_string { wk.string } else { wk.number };
        let map = FxHashMap::from_iter([(id, argument)]);
        let result = {
            let mut first_pass = pass(&mut interner, &binder);
            let first = assert_tainted(apply(&mut first_pass, template, &[id], &map));
            assert_eq!(
                first,
                assert_tainted(apply(&mut first_pass, template, &[id], &map))
            );
            first
        };
        let mut fresh_pass = pass(&mut interner, &binder);
        assert_eq!(
            result,
            assert_tainted(apply(&mut fresh_pass, template, &[id], &map))
        );
        drop(fresh_pass);
        let object = interner.store().object_type(result).expect("result object");
        observations.push((
            template.0,
            result.0,
            object.properties[0].key.clone(),
            object.properties[1].ty,
        ));
    }
    assert_eq!(observations[0].0, observations[1].0);
    assert_eq!(observations[0].1, observations[1].1);
    assert_ne!(observations[0].2, observations[1].2);
    assert_ne!(observations[0].3, observations[1].3);
    let measure = cycle_tainted_application_cache_measure().expect("scope active");
    assert_counts(&measure, (6, 2, 4, 4));
}

#[test]
fn source_shape_keeps_semantic_boundaries_and_clean_lookup_first() {
    let resolve = include_str!("resolve.rs");
    let caller = compact(section(
        resolve,
        "fn instantiate_type_group_arguments(",
        "fn type_decl_defaults(",
    ));
    let helper = compact(section(
        resolve,
        "fn substitute_ready_type_group_application(",
        "fn instantiate_type_group_arguments(",
    ));
    let ready = caller.find("PublishedTypeGroupTerminal::Ready").unwrap();
    let constraints = caller.find("check_type_argument_constraints").unwrap();
    let defaults = caller.find("PublishedTypeParameterDefault::Ready").unwrap();
    let unfinished = caller.find("unfinished_interface").unwrap();
    let call = caller
        .rfind("substitute_ready_type_group_application")
        .expect("helper call");
    assert!(
        ready < constraints && constraints < defaults && defaults < unfinished && unfinished < call
    );
    let complete = helper.find("map.len()!=parameters.len()").unwrap();
    let lazy = helper.find("TypeTag::Conditional").unwrap();
    let clean = helper.find("eager_application_cache.get(&key)").unwrap();
    let option = helper
        .find("letSome(cycle_tainted_application_cache)=self.cycle_tainted_application_cache.as_mut()else")
        .unwrap();
    let tainted = helper
        .find("cycle_tainted_application_cache.get(&key)")
        .unwrap();
    let insertion = helper
        .find("cycle_tainted_application_cache.insert(key")
        .unwrap();
    assert!(complete < lazy && lazy < clean && clean < option);
    assert!(option < tainted && option < insertion);
    assert!(!type_source("substitute/mod.rs").contains("cycle_tainted_application_cache"));
}

#[test]
fn unwind_before_whole_run_publication_records_abort_and_publishes_no_entry() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let id = TypeParamId(99_160);
    let parameter = interner.intern_type_param(id, "T");
    let recursive = recursive_object(&mut interner, parameter, "self");
    let map = FxHashMap::from_iter([(id, wk.string)]);
    let _scope = start_cycle_tainted_application_cache_measure();
    let mut pass = pass(&mut interner, &binder);
    pass.panic_before_cycle_tainted_application_cache_publish_for_test();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = apply(&mut pass, recursive, &[id], &map);
    }));
    assert!(unwind.is_err());
    assert_eq!(
        pass.cycle_tainted_application_cache
            .as_ref()
            .expect("scope captured candidate state")
            .len(),
        0
    );
    assert_tainted(apply(&mut pass, recursive, &[id], &map));
    assert_tainted(apply(&mut pass, recursive, &[id], &map));
    let measure = cycle_tainted_application_cache_measure().expect("scope active");
    assert_eq!((measure.lookups, measure.hits, measure.misses), (3, 1, 2));
    assert_eq!((measure.insertions, measure.aborted_runs), (1, 1));
    assert_arithmetic(&measure);
}

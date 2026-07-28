//! Disabled RED acceptance spec for the per-`Pass` eager generic-application cache.
//!
//! Activate from `decls/mod.rs` only with the implementation. The production call
//! site is the final eager `substitute` arm of `instantiate_type_group_arguments`,
//! after every occurrence has independently completed arity validation, argument
//! lowering, default completion, and constraint checking.
//!
//! The cache key is exactly the immutable template `TypeId` plus the complete
//! `(TypeParamId, TypeId)` pairs in declaration order. It is owned by one `Pass`,
//! and therefore by one type universe. It contains only completed `TypeId` values:
//! there are no negative or in-flight entries. A local group is eligible only once
//! frozen; an inherited/current published group is eligible only through a `Ready`
//! terminal. Partial maps, pending/building/unfinished groups, and lazy template
//! families bypass lookup and insertion. Lazy families are `Conditional`, `Mapped`,
//! and `Instantiation`, plus the trusted string-intrinsic markers, `ThisType`, and
//! `OmitThisParameter`.
//!
//! `crate::types::substitute_with_outcome` is an internal boundary API and returns
//! `SubstitutionOutcome::{CycleClean, CycleTainted}`. The existing public
//! `substitute(...) -> TypeId` API stays unchanged. Nested substitutions, including
//! every per-member substitution created by homomorphic mapped distribution, must
//! propagate taint to their outer outcome. Only `CycleClean` results are inserted.
//!
//! Test-only measurement is opt-in and default-off. Its collector, scope, Pass
//! handle, and update sites remain behind `cfg(test)`, so production has no counter
//! field, TLS access, or counter branch. Counter arithmetic is exact:
//! `lookups == hits + misses` and
//! `misses == insertions + cycle_tainted_skips`. Bypasses are not lookups.
//!
//! The GDB samples that motivated this bounded cache repeatedly enter
//! `instantiate_type_group_arguments -> substitute`, but they are not attribution
//! for every slow traversal. In particular, this cache does not claim to solve the
//! separate roughly four-second cycle-tainted traversal storm.

use super::super::context::{
    eager_application_cache_measure, start_eager_application_cache_measure,
    EagerApplicationCacheMeasure, Pass, TypeDecl,
};
use super::super::type_groups::{
    PublishedTypeEnvironment, PublishedTypeGroup, PublishedTypeGroupSurface,
    PublishedTypeGroupTerminal, PublishedTypeGroupUnavailable, PublishedTypeParameterDefault,
    TypeEnvironmentState, TypeGroupUnavailableCause,
};
use crate::binder::declaration::TypeGroupId;
use crate::binder::Binder;
use crate::check::test_support::{check_source, CheckOutput};
use crate::diagnostics::DiagnosticCode;
use crate::span::Span;
use crate::types::repr::{
    ConditionalType, MappedType, ModifierOp, ObjectType, PropertyType, TypeParamId,
};
use crate::types::store::TypeId;
use crate::types::{substitute_with_outcome, Interner, SubstitutionOutcome};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rustc_hash::FxHashMap;

fn type_source(relative: &str) -> String {
    std::fs::read_to_string(
        crate::test_support::repository_root()
            .join("crates/typokat-types/src/types")
            .join(relative),
    )
    .expect("type source")
}

fn prop(name: impl Into<String>, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn binder_with_prelude(prelude_source: &str) -> Binder {
    let prelude_allocator = Allocator::default();
    let user_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, prelude_source, SourceType::ts()).parse();
    let user = Parser::new(&user_allocator, "", SourceType::ts()).parse();
    assert!(!prelude.panicked && prelude.diagnostics.is_empty());
    assert!(!user.panicked && user.diagnostics.is_empty());
    crate::binder::bind_module_with_prelude(&prelude.program, &user.program)
}

fn empty_binder() -> Binder {
    binder_with_prelude("")
}

fn empty_published_pass<'a>(interner: &'a mut Interner, binder: &'a Binder) -> Pass<'a, 'static> {
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

fn inherited_constructing_pass<'a>(
    interner: &'a mut Interner,
    binder: &'a Binder,
    param: TypeParamId,
    terminal: PublishedTypeGroupTerminal,
) -> Pass<'a, 'static> {
    assert_eq!(binder.prelude_type_group_count, 1);
    assert_eq!(binder.type_groups.len(), 1);
    let mut pass = super::super::build_pass(
        interner,
        binder,
        vec![TypeDecl::Resolved {
            params: vec![param],
        }],
        vec![None],
        super::super::context::DeclTypes::new(binder.decl_count),
        param.0 + 1,
    );
    pass.install_published_type_environment_base(
        PublishedTypeEnvironment::from_explicit_terminals_for_test(vec![terminal]),
    );
    pass
}

fn checked(source: &str) -> CheckOutput {
    let output = check_source(source);
    assert!(
        output.parse_errors.is_empty(),
        "unexpected parse errors: {:?}",
        output.parse_errors
    );
    output
}

fn checked_on_current_thread(source: &str) -> CheckOutput {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let parse_errors = parsed
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if parsed.panicked {
        return CheckOutput {
            diagnostics: Vec::new(),
            parse_errors,
            incomplete: Vec::new(),
        };
    }

    let mut interner = Interner::with_intrinsics();
    let result = crate::check::checker::check_program(&mut interner, &parsed.program);
    CheckOutput {
        diagnostics: result.diagnostics,
        parse_errors,
        incomplete: result.incomplete,
    }
}

fn span_text(source: &str, span: crate::span::Span) -> &str {
    let start = usize::try_from(span.start).expect("source span start fits usize");
    let end = usize::try_from(span.end).expect("source span end fits usize");
    &source[start..end]
}

fn measured(source: &str) -> (CheckOutput, EagerApplicationCacheMeasure) {
    assert_eq!(eager_application_cache_measure(), None);
    let scope = start_eager_application_cache_measure();
    let output = checked_on_current_thread(source);
    assert!(
        output.parse_errors.is_empty(),
        "unexpected parse errors: {:?}",
        output.parse_errors
    );
    let measure = eager_application_cache_measure().expect("measurement scope remains enabled");
    assert_cache_arithmetic(&measure);
    drop(scope);
    assert_eq!(eager_application_cache_measure(), None);
    (output, measure)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CacheCounters {
    lookups: u64,
    hits: u64,
    misses: u64,
    insertions: u64,
    cycle_tainted_skips: u64,
    unready_bypasses: u64,
    unfinished_bypasses: u64,
    lazy_bypasses: u64,
}

fn delta(
    after: &EagerApplicationCacheMeasure,
    before: &EagerApplicationCacheMeasure,
) -> CacheCounters {
    assert_cache_arithmetic(after);
    assert_cache_arithmetic(before);
    let delta = CacheCounters {
        lookups: after
            .lookups
            .checked_sub(before.lookups)
            .expect("scenario cannot remove baseline lookups"),
        hits: after
            .hits
            .checked_sub(before.hits)
            .expect("scenario cannot remove baseline hits"),
        misses: after
            .misses
            .checked_sub(before.misses)
            .expect("scenario cannot remove baseline misses"),
        insertions: after
            .insertions
            .checked_sub(before.insertions)
            .expect("scenario cannot remove baseline insertions"),
        cycle_tainted_skips: after
            .cycle_tainted_skips
            .checked_sub(before.cycle_tainted_skips)
            .expect("scenario cannot remove baseline taint skips"),
        unready_bypasses: after
            .unready_bypasses
            .checked_sub(before.unready_bypasses)
            .expect("scenario cannot remove baseline unready bypasses"),
        unfinished_bypasses: after
            .unfinished_bypasses
            .checked_sub(before.unfinished_bypasses)
            .expect("scenario cannot remove baseline unfinished bypasses"),
        lazy_bypasses: after
            .lazy_bypasses
            .checked_sub(before.lazy_bypasses)
            .expect("scenario cannot remove baseline lazy bypasses"),
    };
    assert_counter_arithmetic(&delta);
    delta
}

fn assert_cache_arithmetic(measure: &EagerApplicationCacheMeasure) {
    assert_eq!(measure.lookups, measure.hits + measure.misses);
    assert_eq!(
        measure.misses,
        measure.insertions + measure.cycle_tainted_skips
    );
}

fn assert_counter_arithmetic(counters: &CacheCounters) {
    assert_eq!(counters.lookups, counters.hits + counters.misses);
    assert_eq!(
        counters.misses,
        counters.insertions + counters.cycle_tainted_skips
    );
}

fn source_section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source
        .find(start)
        .unwrap_or_else(|| panic!("missing source-shape start marker: {start}"));
    let tail = &source[start..];
    let end = tail
        .find(end)
        .unwrap_or_else(|| panic!("missing source-shape end marker: {end}"));
    &tail[..end]
}

fn without_whitespace(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn normalized_substitute_reexport_entries(statement: &str) -> Vec<String> {
    let compact = without_whitespace(statement);
    let Some((_, entries)) = compact.rsplit_once("usesubstitute::") else {
        return Vec::new();
    };
    entries
        .trim_start_matches('{')
        .trim_end_matches('}')
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

fn delimiter_depths(lines: &[&str]) -> Vec<(i64, i64, i64)> {
    let mut result = Vec::with_capacity(lines.len() + 1);
    let (mut braces, mut parentheses, mut brackets) = (0_i64, 0_i64, 0_i64);
    for line in lines {
        result.push((braces, parentheses, brackets));
        braces += i64::try_from(line.matches('{').count()).expect("brace count fits i64");
        braces -= i64::try_from(line.matches('}').count()).expect("brace count fits i64");
        parentheses +=
            i64::try_from(line.matches('(').count()).expect("parenthesis count fits i64");
        parentheses -=
            i64::try_from(line.matches(')').count()).expect("parenthesis count fits i64");
        brackets += i64::try_from(line.matches('[').count()).expect("bracket count fits i64");
        brackets -= i64::try_from(line.matches(']').count()).expect("bracket count fits i64");
    }
    result.push((braces, parentheses, brackets));
    result
}

fn cfg_test_item_ranges(source: &str) -> Vec<(usize, usize)> {
    let lines = source.lines().collect::<Vec<_>>();
    let depths = delimiter_depths(&lines);
    let mut ranges = Vec::new();
    for guard in lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (line.trim() == "#[cfg(test)]").then_some(index))
    {
        let Some(target) = ((guard + 1)..lines.len()).find(|index| {
            let line = lines[*index].trim();
            !line.is_empty() && !line.starts_with("#[")
        }) else {
            continue;
        };
        let target_line = lines[target].trim_start();
        let braced_item = target_line.contains(" fn ")
            || target_line.starts_with("fn ")
            || target_line.contains(" struct ")
            || target_line.starts_with("struct ")
            || target_line.contains(" enum ")
            || target_line.starts_with("enum ")
            || target_line.contains(" impl")
            || target_line.starts_with("impl")
            || target_line.contains(" mod ")
            || target_line.starts_with("mod ")
            || target_line.contains(" trait ")
            || target_line.starts_with("trait ")
            || target_line.contains('!');
        let base = depths[guard];
        let end = if braced_item {
            let Some(header_end) = (target..lines.len()).find(|index| {
                lines[*index].contains('{')
                    || (depths[*index] == base && lines[*index].contains(';'))
            }) else {
                continue;
            };
            let brace = lines[header_end].find('{');
            let semicolon = lines[header_end].find(';');
            if semicolon.is_some_and(|semicolon| brace.is_none_or(|brace| semicolon < brace)) {
                header_end
            } else {
                (header_end..lines.len())
                    .find(|index| depths[index + 1].0 <= base.0)
                    .unwrap_or(lines.len() - 1)
            }
        } else {
            (target..lines.len())
                .find(|index| {
                    let trimmed = lines[*index].trim_end();
                    let at_item_depth = depths[*index].0 <= base.0
                        && depths[*index].1 <= base.1
                        && depths[*index].2 <= base.2;
                    (trimmed.ends_with(',') || trimmed.contains(';')) && at_item_depth
                        || depths[index + 1].0 < base.0
                })
                .unwrap_or(target)
        };
        ranges.push((guard, end));
    }
    ranges
}

fn unguarded_occurrence_lines(source: &str, needle: &str) -> Vec<usize> {
    let lines = source.lines().collect::<Vec<_>>();
    let ranges = cfg_test_item_ranges(source);
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            (line.contains(needle)
                && !ranges
                    .iter()
                    .any(|(start, end)| *start <= index && index <= *end))
            .then_some(index + 1)
        })
        .collect()
}

fn assert_test_guard_covers_every_occurrence(source: &str, needle: &str) {
    assert!(
        source.contains(needle),
        "missing cfg(test) subject: {needle}"
    );
    assert_eq!(
        unguarded_occurrence_lines(source, needle),
        Vec::<usize>::new(),
        "every {needle} declaration or update must be inside its own cfg(test) item"
    );
}

#[test]
fn cfg_test_guard_scanner_stops_at_item_statement_and_field_boundaries() {
    const SOURCE: &str = "#[cfg(test)]
fn guarded() {
    let eager_application_cache_measure = 1;
}
fn unguarded() {
    let eager_application_cache_measure = 2;
}
struct Fields {
    #[cfg(test)]
    guarded_eager_application_cache_measure: u64,
    unguarded_eager_application_cache_measure: u64,
}
#[cfg(test)]
const GUARDED_EAGER_APPLICATION_CACHE_MEASURE: u64 = 3;
const UNGUARDED_EAGER_APPLICATION_CACHE_MEASURE: u64 = 4;
";

    assert_eq!(
        unguarded_occurrence_lines(SOURCE, "eager_application_cache_measure"),
        [6, 11]
    );
    assert_eq!(
        unguarded_occurrence_lines(SOURCE, "EAGER_APPLICATION_CACHE_MEASURE"),
        [15]
    );
}

#[test]
fn ready_cache_misses_then_hits_and_uses_exact_declaration_order() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first_id = TypeParamId(98_000);
    let second_id = TypeParamId(98_001);
    let first = interner.intern_type_param(first_id, "First");
    let second = interner.intern_type_param(second_id, "Second");
    let template = interner.intern_object(ObjectType {
        properties: vec![prop("first", first), prop("second", second)],
        ..Default::default()
    });
    let other_template = interner.intern_object(ObjectType {
        properties: vec![prop("left", first), prop("right", second)],
        ..Default::default()
    });
    let parameters = [first_id, second_id];
    let reversed_parameters = [second_id, first_id];
    let first_map = FxHashMap::from_iter([(first_id, wk.string), (second_id, wk.number)]);
    let reverse_insertion_map =
        FxHashMap::from_iter([(second_id, wk.number), (first_id, wk.string)]);
    let different_arguments = FxHashMap::from_iter([(first_id, wk.number), (second_id, wk.string)]);

    let scope = start_eager_application_cache_measure();
    let mut pass = empty_published_pass(&mut interner, &binder);
    let first_result =
        pass.substitute_ready_type_group_application(template, &parameters, &first_map);
    let identical_result =
        pass.substitute_ready_type_group_application(template, &parameters, &reverse_insertion_map);
    let different_result =
        pass.substitute_ready_type_group_application(template, &parameters, &different_arguments);
    let reordered_key_result =
        pass.substitute_ready_type_group_application(template, &reversed_parameters, &first_map);
    let other_template_result =
        pass.substitute_ready_type_group_application(other_template, &parameters, &first_map);

    assert_eq!(first_result, identical_result);
    assert_ne!(first_result, different_result);
    assert_eq!(first_result, reordered_key_result);
    assert_ne!(first_result, other_template_result);

    let measure = eager_application_cache_measure().expect("measurement scope remains enabled");
    assert_eq!(measure.lookups, 5);
    assert_eq!(
        measure.hits, 1,
        "FxHashMap insertion order is not key order"
    );
    assert_eq!(measure.misses, 4);
    assert_eq!(measure.insertions, 4);
    assert_eq!(measure.cycle_tainted_skips, 0);
    assert_eq!(measure.unready_bypasses, 0);
    assert_cache_arithmetic(&measure);
    drop(pass);
    drop(scope);
}

#[test]
fn incomplete_parameter_pairs_bypass_without_publishing_a_partial_key() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first_id = TypeParamId(98_010);
    let second_id = TypeParamId(98_011);
    let first = interner.intern_type_param(first_id, "First");
    let second = interner.intern_type_param(second_id, "Second");
    let template = interner.intern_object(ObjectType {
        properties: vec![prop("first", first), prop("second", second)],
        ..Default::default()
    });
    let partial = FxHashMap::from_iter([(first_id, wk.string)]);

    let scope = start_eager_application_cache_measure();
    let mut pass = empty_published_pass(&mut interner, &binder);
    let first_result =
        pass.substitute_ready_type_group_application(template, &[first_id, second_id], &partial);
    let second_result =
        pass.substitute_ready_type_group_application(template, &[first_id, second_id], &partial);
    assert_eq!(first_result, second_result);

    let measure = eager_application_cache_measure().expect("measurement scope remains enabled");
    assert_eq!(measure.lookups, 0);
    assert_eq!(measure.hits, 0);
    assert_eq!(measure.misses, 0);
    assert_eq!(measure.insertions, 0);
    assert_eq!(measure.unready_bypasses, 2);
    assert_cache_arithmetic(&measure);
    drop(pass);
    drop(scope);
}

#[test]
fn cache_lifetime_is_one_pass_even_in_one_universe_and_never_crosses_universes() {
    let binder = empty_binder();
    let param_id = TypeParamId(98_020);

    let scope = start_eager_application_cache_measure();
    let mut first_interner = Interner::with_intrinsics();
    let first_wk = first_interner.well_known();
    let first_param = first_interner.intern_type_param(param_id, "T");
    let first_template = first_interner.intern_object(ObjectType {
        properties: vec![prop("value", first_param)],
        ..Default::default()
    });
    let first_map = FxHashMap::from_iter([(param_id, first_wk.string)]);
    {
        let mut first_pass = empty_published_pass(&mut first_interner, &binder);
        let first = first_pass.substitute_ready_type_group_application(
            first_template,
            &[param_id],
            &first_map,
        );
        let second = first_pass.substitute_ready_type_group_application(
            first_template,
            &[param_id],
            &first_map,
        );
        assert_eq!(first, second);
    }
    {
        let mut fresh_pass = empty_published_pass(&mut first_interner, &binder);
        let _ = fresh_pass.substitute_ready_type_group_application(
            first_template,
            &[param_id],
            &first_map,
        );
    }

    let mut second_interner = Interner::with_intrinsics();
    let second_wk = second_interner.well_known();
    let second_param = second_interner.intern_type_param(param_id, "T");
    let second_template = second_interner.intern_object(ObjectType {
        properties: vec![prop("value", second_param)],
        ..Default::default()
    });
    let second_map = FxHashMap::from_iter([(param_id, second_wk.string)]);
    assert_eq!(first_template.0, second_template.0);
    assert_eq!(first_param.0, second_param.0);
    assert_eq!(first_wk.string.0, second_wk.string.0);
    let mut second_universe_pass = empty_published_pass(&mut second_interner, &binder);
    let _ = second_universe_pass.substitute_ready_type_group_application(
        second_template,
        &[param_id],
        &second_map,
    );

    let measure = eager_application_cache_measure().expect("measurement scope remains enabled");
    assert_eq!(measure.lookups, 4);
    assert_eq!(measure.hits, 1);
    assert_eq!(measure.misses, 3);
    assert_eq!(measure.insertions, 3);
    assert_eq!(measure.cycle_tainted_skips, 0);
    assert_cache_arithmetic(&measure);
    drop(second_universe_pass);
    drop(scope);
}

#[test]
fn cycle_tainted_results_repeat_the_miss_and_never_publish() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let param_id = TypeParamId(98_030);
    let param = interner.intern_type_param(param_id, "T");
    let recursive = interner.reserve_object();
    interner.fill_object(
        recursive,
        ObjectType {
            properties: vec![prop("self", recursive), prop("value", param)],
            ..Default::default()
        },
    );
    let map = FxHashMap::from_iter([(param_id, wk.number)]);

    assert!(matches!(
        substitute_with_outcome(&mut interner, recursive, &map),
        SubstitutionOutcome::CycleTainted(_)
    ));

    let scope = start_eager_application_cache_measure();
    let mut pass = empty_published_pass(&mut interner, &binder);
    let first = pass.substitute_ready_type_group_application(recursive, &[param_id], &map);
    let second = pass.substitute_ready_type_group_application(recursive, &[param_id], &map);
    assert_eq!(first, second);

    let measure = eager_application_cache_measure().expect("measurement scope remains enabled");
    assert_eq!(measure.lookups, 2);
    assert_eq!(measure.hits, 0);
    assert_eq!(measure.misses, 2);
    assert_eq!(measure.insertions, 0);
    assert_eq!(measure.cycle_tainted_skips, 2);
    assert_cache_arithmetic(&measure);
    drop(pass);
    drop(scope);
}

#[test]
fn nested_mapped_distribution_propagates_cycle_taint_to_the_outer_application() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let param_id = TypeParamId(98_040);
    let param = interner.intern_type_param(param_id, "T");
    let recursive_value = interner.reserve_object();
    interner.fill_object(
        recursive_value,
        ObjectType {
            properties: vec![prop("self", recursive_value), prop("value", param)],
            ..Default::default()
        },
    );
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: param,
        value_template: recursive_value,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let eager_root = interner.intern_object(ObjectType {
        properties: vec![prop("mapped", mapped)],
        ..Default::default()
    });
    let left = interner.intern_object(ObjectType {
        properties: vec![prop("left", wk.string)],
        ..Default::default()
    });
    let right = interner.intern_object(ObjectType {
        properties: vec![prop("right", wk.number)],
        ..Default::default()
    });
    let union = interner.union(vec![left, right]);
    let map = FxHashMap::from_iter([(param_id, union)]);

    assert!(matches!(
        substitute_with_outcome(&mut interner, eager_root, &map),
        SubstitutionOutcome::CycleTainted(_)
    ));

    let scope = start_eager_application_cache_measure();
    let mut pass = empty_published_pass(&mut interner, &binder);
    let first = pass.substitute_ready_type_group_application(eager_root, &[param_id], &map);
    let second = pass.substitute_ready_type_group_application(eager_root, &[param_id], &map);
    assert_eq!(first, second);

    let measure = eager_application_cache_measure().expect("measurement scope remains enabled");
    assert_eq!(measure.lookups, 2);
    assert_eq!(measure.hits, 0);
    assert_eq!(measure.misses, 2);
    assert_eq!(measure.insertions, 0);
    assert_eq!(measure.cycle_tainted_skips, 2);
    assert_cache_arithmetic(&measure);
    drop(pass);
    drop(scope);
}

#[test]
fn constructed_lazy_tags_and_trusted_markers_bypass_before_lookup() {
    let binder = empty_binder();
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let param_id = TypeParamId(98_050);
    let param = interner.intern_type_param(param_id, "T");
    let conditional = interner.intern_conditional(ConditionalType {
        check: param,
        extends_ty: wk.string,
        true_branch: param,
        false_branch: wk.never,
        infer_count: 0,
        distributive: true,
        poisoned: false,
    });
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: true,
        key_source: param,
        value_template: param,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    });
    let eager_base = interner.intern_object(ObjectType {
        properties: vec![prop("value", param)],
        ..Default::default()
    });
    let instantiation = interner.intern_instantiation(eager_base, vec![(param_id, wk.string)]);
    let map = FxHashMap::from_iter([(param_id, wk.number)]);

    let scope = start_eager_application_cache_measure();
    let mut pass = empty_published_pass(&mut interner, &binder);
    for template in [
        conditional,
        mapped,
        instantiation,
        wk.uppercase,
        wk.this_type,
        wk.omit_this_parameter,
    ] {
        let _ = pass.substitute_ready_type_group_application(template, &[param_id], &map);
    }

    let measure = eager_application_cache_measure().expect("measurement scope remains enabled");
    assert_eq!(
        measure,
        EagerApplicationCacheMeasure {
            lazy_bypasses: 6,
            ..Default::default()
        }
    );
    assert_cache_arithmetic(&measure);
    drop(pass);
    drop(scope);
}

#[test]
fn default_completion_precedes_keying_and_distinct_completed_defaults_do_not_collide() {
    const BASE: &str = "\
type CacheDefaulted<A = string, B = A> = { first: A; second: B };
";
    const IDENTICAL: &str = "\
type CacheDefaulted<A = string, B = A> = { first: A; second: B };
declare const omitted: CacheDefaulted;
declare const oneExplicit: CacheDefaulted<string>;
declare const allExplicit: CacheDefaulted<string, string>;
";
    const DISTINCT: &str = "\
type CacheDefaulted<A = string, B = A> = { first: A; second: B };
declare const strings: CacheDefaulted;
declare const numbers: CacheDefaulted<number>;
";

    let (_, baseline) = measured(BASE);
    let (identical_output, identical) = measured(IDENTICAL);
    assert!(identical_output.diagnostics.is_empty());
    assert!(identical_output.incomplete.is_empty());
    assert_eq!(
        delta(&identical, &baseline),
        CacheCounters {
            lookups: 3,
            hits: 2,
            misses: 1,
            insertions: 1,
            ..Default::default()
        },
        "omitted and explicit spellings share the same completed ordered pairs"
    );

    let (distinct_output, distinct) = measured(DISTINCT);
    assert!(distinct_output.diagnostics.is_empty());
    assert!(distinct_output.incomplete.is_empty());
    assert_eq!(
        delta(&distinct, &baseline),
        CacheCounters {
            lookups: 2,
            misses: 2,
            insertions: 2,
            ..Default::default()
        },
        "different resolved defaults must remain different cache keys"
    );
}

#[test]
fn frozen_local_template_is_eligible_before_atomic_environment_publication() {
    const BASE: &str = "\
type CacheFrozen<T> = { value: T };
type CacheConsumer<T> = {};
";
    const SOURCE: &str = "\
type CacheFrozen<T> = { value: T };
type CacheConsumer<T> = {
  first: CacheFrozen<T>;
  second: CacheFrozen<T>;
};
";

    let (_, baseline) = measured(BASE);
    let (output, measure) = measured(SOURCE);
    assert!(output.diagnostics.is_empty());
    assert!(output.incomplete.is_empty());
    assert_eq!(
        delta(&measure, &baseline),
        CacheCounters {
            lookups: 2,
            hits: 1,
            misses: 1,
            insertions: 1,
            ..Default::default()
        },
        "the earlier local template is frozen before CacheConsumer lowers"
    );
}

#[test]
fn unfinished_and_every_lazy_template_family_bypass_the_cache() {
    const UNFINISHED: &str = "\
interface CacheRecursive<T> {
  first: CacheRecursive<T>;
  second: CacheRecursive<T>;
}
";
    const PENDING_FORWARD: &str = "\
interface CacheForwardConsumer<T> {
  first: CacheForwardTarget<T>;
  second: CacheForwardTarget<T>;
}
interface CacheForwardTarget<T> { value: T }
";
    const LAZY_BASE: &str = "\
type CacheConditional<T> = T extends string ? T : never;
type CacheMapped<T> = { [K in keyof T]: T[K] };
type CacheInstantiation<T> = CacheConditional<T>;
";
    const LAZY_APPLICATIONS: &str = "\
type CacheConditional<T> = T extends string ? T : never;
type CacheMapped<T> = { [K in keyof T]: T[K] };
type CacheInstantiation<T> = CacheConditional<T>;
declare const conditional: CacheConditional<string>;
declare const mapped: CacheMapped<{ value: string }>;
declare const instantiation: CacheInstantiation<string>;
declare const intrinsic: Uppercase<\"cache\">;
declare const thisType: ThisType<{ value: string }>;
declare const omitThis: OmitThisParameter<(this: { value: string }) => string>;
";

    let (_, empty_baseline) = measured("");
    let (unfinished_output, unfinished_measure) = measured(UNFINISHED);
    let unfinished = delta(&unfinished_measure, &empty_baseline);
    assert!(unfinished_output.diagnostics.is_empty());
    assert!(unfinished_output.incomplete.is_empty());
    assert_eq!(unfinished.lookups, 0);
    assert_eq!(unfinished.insertions, 0);
    assert_eq!(unfinished.unfinished_bypasses, 2);
    assert_eq!(
        unfinished,
        CacheCounters {
            unfinished_bypasses: 2,
            ..Default::default()
        }
    );

    let (pending_output, pending_measure) = measured(PENDING_FORWARD);
    let pending = delta(&pending_measure, &empty_baseline);
    assert!(pending_output.diagnostics.is_empty());
    assert!(pending_output.incomplete.is_empty());
    assert_eq!(
        pending,
        CacheCounters {
            unfinished_bypasses: 2,
            ..Default::default()
        },
        "forward Pending groups cannot publish a lookup or insertion"
    );

    let (_, lazy_baseline) = measured(LAZY_BASE);
    let (lazy_output, lazy) = measured(LAZY_APPLICATIONS);
    assert!(lazy_output.diagnostics.is_empty());
    assert!(lazy_output.incomplete.is_empty());
    assert_eq!(
        delta(&lazy, &lazy_baseline),
        CacheCounters {
            lazy_bypasses: 6,
            ..Default::default()
        }
    );
}

#[test]
fn published_non_ready_terminal_cannot_lookup_or_insert() {
    const BASE: &str = "\
interface CacheUnavailable<T> { first: T }
type CacheUnavailable<T> = { second: T };
";
    const SOURCE: &str = "\
interface CacheUnavailable<T> { first: T }
type CacheUnavailable<T> = { second: T };
declare const rejectedFirst: CacheUnavailable<string>;
declare const rejectedSecond: CacheUnavailable<string>;
";

    let (_, baseline) = measured(BASE);
    let (_, measure) = measured(SOURCE);
    assert_eq!(
        delta(&measure, &baseline),
        CacheCounters::default(),
        "Unavailable published terminals return before any cache operation"
    );
}

#[test]
fn inherited_explicit_ready_terminal_hits_and_unavailable_terminal_stays_silent() {
    let binder = binder_with_prelude("type InheritedCache<T> = { value: T };");
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let param_id = TypeParamId(98_060);
    let param = interner.intern_type_param(param_id, "T");
    let template = interner.intern_object(ObjectType {
        properties: vec![prop("value", param)],
        ..Default::default()
    });
    let ready = PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
        name: "InheritedCache".to_string(),
        surface: PublishedTypeGroupSurface::Template(template),
        parameters: vec![param_id],
        parameter_names: vec!["T".to_string()],
        parameter_defaults: vec![PublishedTypeParameterDefault::Absent],
        conflict_alternatives: Vec::new(),
    });
    let application_span = Span::new(40, 62);

    let ready_scope = start_eager_application_cache_measure();
    let mut ready_pass = inherited_constructing_pass(&mut interner, &binder, param_id, ready);
    let first = ready_pass.instantiate_type_group_arguments_for_test(
        binder.module,
        TypeGroupId(0),
        vec![(wk.string, Span::new(55, 61))],
        application_span,
    );
    let second = ready_pass.instantiate_type_group_arguments_for_test(
        binder.module,
        TypeGroupId(0),
        vec![(wk.string, Span::new(55, 61))],
        application_span,
    );
    assert_eq!(first, second);
    assert!(first.is_some());
    let ready_measure =
        eager_application_cache_measure().expect("ready measurement remains enabled");
    assert_eq!(
        ready_measure,
        EagerApplicationCacheMeasure {
            lookups: 2,
            hits: 1,
            misses: 1,
            insertions: 1,
            ..Default::default()
        }
    );
    assert_cache_arithmetic(&ready_measure);
    drop(ready_pass);
    drop(ready_scope);

    let unavailable = PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
        cause: TypeGroupUnavailableCause::UnsupportedComposition,
    });
    let unavailable_scope = start_eager_application_cache_measure();
    let mut unavailable_pass =
        inherited_constructing_pass(&mut interner, &binder, param_id, unavailable);
    let result = unavailable_pass.instantiate_type_group_arguments_for_test(
        binder.module,
        TypeGroupId(0),
        vec![(wk.string, Span::new(55, 61))],
        application_span,
    );
    assert_eq!(result, None);
    let unavailable_measure =
        eager_application_cache_measure().expect("unavailable measurement remains enabled");
    assert_eq!(unavailable_measure, EagerApplicationCacheMeasure::default());
    assert_cache_arithmetic(&unavailable_measure);
    drop(unavailable_pass);
    drop(unavailable_scope);
}

#[test]
fn inherited_ready_terminal_is_eager_eligible_while_non_ready_returns_early() {
    let application = without_whitespace(source_section(
        include_str!("resolve.rs"),
        "fn instantiate_type_group_arguments(",
        "fn type_decl_defaults(",
    ));
    let type_groups = without_whitespace(source_section(
        include_str!("../type_groups.rs"),
        "fn resolution_environment(&self)",
        "fn is_published(&self)",
    ));
    assert!(type_groups.contains("Self::Constructing{..}=>self.inherited()"));
    assert!(application.contains("letis_published=published.is_some()"));
    assert!(application.contains("letunfinished_interface=!is_published&&"));

    let ready = application
        .find("Some(PublishedTypeGroupTerminal::Ready(published))")
        .expect("inherited and current Ready terminals share the ready arm");
    let non_ready = application
        .find("Some(PublishedTypeGroupTerminal::Unavailable(_))=>returnNone")
        .expect("non-Ready published terminals return immediately");
    let cache = application
        .find("self.substitute_ready_type_group_application(")
        .expect("Ready eager templates reach the cache owner");
    assert!(ready < cache);
    assert!(non_ready < cache);
}

#[test]
fn cache_hits_never_own_or_deduplicate_occurrence_diagnostics() {
    const BASE: &str = "\
type CacheConstrained<T extends { kind: string }> = { value: T };
type CachePair<A, B> = { first: A; second: B };
type CacheBox<T> = { value: T };
";
    const SOURCE: &str = "\
type CacheConstrained<T extends { kind: string }> = { value: T };
type CachePair<A, B> = { first: A; second: B };
type CacheBox<T> = { value: T };
declare const badConstraintFirst: CacheConstrained<number>;
declare const badConstraintSecond: CacheConstrained<number>;
declare const badArityFirst: CachePair<string>;
declare const badAritySecond: CachePair<string>;
declare const memberFirst: CacheBox<string>;
memberFirst.missing;
declare const memberSecond: CacheBox<string>;
memberSecond.missing;
";

    let (_, baseline) = measured(BASE);
    let (output, measure) = measured(SOURCE);
    let rows = output
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, span_text(SOURCE, diagnostic.span)))
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            (DiagnosticCode::TK2344, "number"),
            (DiagnosticCode::TK2344, "number"),
            (DiagnosticCode::TK2314, "CachePair<string>"),
            (DiagnosticCode::TK2314, "CachePair<string>"),
            (DiagnosticCode::TK2339, "missing"),
            (DiagnosticCode::TK2339, "missing"),
        ],
        "each occurrence retains its source owner and source-order replay"
    );
    assert!(output.incomplete.is_empty());
    assert_eq!(
        delta(&measure, &baseline),
        CacheCounters {
            lookups: 4,
            hits: 2,
            misses: 2,
            insertions: 2,
            ..Default::default()
        },
        "constraint checks run before the hit; invalid arity never reaches the cache"
    );
}

#[test]
fn activation_shape_keeps_cache_at_the_ready_final_arm_and_telemetry_test_only() {
    let resolve = include_str!("resolve.rs");
    let context = include_str!("../context.rs");
    let type_groups = include_str!("../type_groups.rs");
    let types_module = type_source("mod.rs");
    let substitute_module = type_source("substitute/mod.rs");
    let substitute_apply = type_source("substitute/apply.rs");

    let application = source_section(
        resolve,
        "fn instantiate_type_group_arguments(",
        "fn type_decl_defaults(",
    );
    let application_signature = &application[..application
        .find('{')
        .expect("type-group application has a function body")];
    assert!(!application_signature.contains("Measure"));
    assert!(!application_signature.contains("collector"));
    let compact_application = without_whitespace(application);
    let constraints = compact_application
        .find("self.check_type_argument_constraints(")
        .expect("constraint checks remain occurrence-owned");
    let defaults = compact_application
        .find("PublishedTypeParameterDefault::Ready(")
        .expect("default completion remains occurrence-owned");
    let instantiation_tag = compact_application
        .find("TypeTag::Instantiation")
        .expect("lazy Instantiation templates have an explicit route");
    let cache_call = compact_application
        .find("self.substitute_ready_type_group_application(")
        .expect("the eager final arm calls the cache owner exactly once");
    assert!(constraints < cache_call);
    assert!(defaults < cache_call);
    assert!(instantiation_tag < cache_call);
    assert_eq!(
        compact_application
            .match_indices("self.substitute_ready_type_group_application(")
            .count(),
        1
    );
    assert!(compact_application[..cache_call]
        .contains("Some(PublishedTypeGroupTerminal::Ready(published))"));
    assert!(compact_application[..cache_call]
        .contains("Some(PublishedTypeGroupTerminal::Unavailable(_))=>returnNone"));
    assert!(compact_application[..cache_call]
        .contains(".resolution_environment().groups().get(decl_id).cloned()"));
    assert!(
        !compact_application.contains("Some(substitute(self.interner,template,&map))"),
        "the uncached eager final arm must be removed"
    );
    let final_some = compact_application
        .rfind("Some(")
        .expect("application has a successful final arm");
    assert!(
        compact_application[final_some..]
            .starts_with("Some(self.substitute_ready_type_group_application("),
        "no semantic branch follows the final cache call"
    );

    let resolution_environment = without_whitespace(source_section(
        type_groups,
        "fn resolution_environment(&self)",
        "fn is_published(&self)",
    ));
    assert!(resolution_environment.contains("Self::Constructing{..}=>self.inherited()"));
    assert!(resolution_environment.contains("Self::Published(_)=>self.published()"));

    let pass_fields = without_whitespace(source_section(
        context,
        "struct Pass<'a, 'ast",
        "impl<'ast, Ticket: Copy + PartialEq> Deref",
    ));
    assert!(pass_fields.contains("eager_application_cache:FxHashMap<"));
    assert!(!pass_fields.contains("Arc<"));
    assert_test_guard_covers_every_occurrence(context, "EagerApplicationCacheMeasure");
    assert_test_guard_covers_every_occurrence(context, "eager_application_cache_measure");
    assert_test_guard_covers_every_occurrence(resolve, "record_eager_application_cache_measure");
    assert_test_guard_covers_every_occurrence(type_groups, "from_explicit_terminals_for_test");
    assert_test_guard_covers_every_occurrence(resolve, "instantiate_type_group_arguments_for_test");

    let public_substitute_reexport = source_section(&types_module, "pub use substitute::", ";");
    let (_, exported_names) = public_substitute_reexport
        .split_once("::")
        .expect("substitute re-export names follow the module path");
    assert!(
        exported_names
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .any(|token| token == "substitute"),
        "crate::types::substitute remains a public re-export"
    );
    let mut boundary_substitute_entries = Vec::new();
    for statement in types_module.split(';') {
        let compact = without_whitespace(statement);
        if compact.contains("pubusesubstitute::") {
            boundary_substitute_entries.extend(normalized_substitute_reexport_entries(statement));
        }
    }
    assert!(boundary_substitute_entries
        .iter()
        .any(|entry| entry == "substitute_with_outcome"));
    assert!(boundary_substitute_entries
        .iter()
        .any(|entry| entry == "SubstitutionOutcome"));

    let substitute_api = without_whitespace(source_section(
        &substitute_module,
        "pub fn substitute(",
        "pub fn instantiate_function(",
    ));
    assert!(substitute_api.contains("pubfnsubstitute("));
    assert!(substitute_api.contains(")->TypeId"));
    let compact_substitute_module = without_whitespace(&substitute_module);
    assert!(compact_substitute_module.contains("pubfnsubstitute_with_outcome("));

    let mapped_apply = without_whitespace(source_section(
        &substitute_apply,
        "fn apply_mapped(",
        "fn apply_keyof(",
    ));
    assert!(mapped_apply.contains("substitute_with_outcome("));
    assert!(mapped_apply.contains("SubstitutionOutcome::CycleTainted"));
    assert!(!mapped_apply.contains("substitute(interner,ty,&member_map)"));
}

#[test]
fn cache_measurement_is_default_off() {
    assert_eq!(eager_application_cache_measure(), None);
    let output = checked("type CacheBox<T> = { value: T }; let value: CacheBox<string>;");
    assert!(output.diagnostics.is_empty());
    assert!(output.incomplete.is_empty());
    assert_eq!(eager_application_cache_measure(), None);
}

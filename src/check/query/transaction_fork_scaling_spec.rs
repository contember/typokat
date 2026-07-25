//! RED contract for enclosing semantic-query transactions over a growing program.

use super::{query_source_cold_measure, start_query_source_cold_measure, QuerySourceColdMeasure};
use crate::check::checker::check_program;
use crate::driver::CheckOutput;
use crate::relate::cache::{finish_relation_cache_depth_measure, start_relation_cache_depth_measure};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

const SMALL_BLOCKS: usize = 40;
const LARGE_BLOCKS: usize = 80;

/// Copy volume may exceed the transactions' own planning work only by this much.
const WORKING_SET_MULTIPLE: u64 = 4;
/// Doubling the program doubles the transactions; copy volume must not outpace that.
const GROWTH_MULTIPLE: u64 = 3;
const GROWTH_SLACK: u64 = 64;

struct MeasuredRun {
    blocks: usize,
    output: CheckOutput,
    work: QuerySourceColdMeasure,
}

impl MeasuredRun {
    /// Real per-transaction work: the planning each transaction actually performs.
    fn working_set(&self) -> u64 {
        self.work.planner_visits + self.work.semantic_query_transactions
    }

    fn record(&self) -> String {
        format!("{} blocks: {:#?}", self.blocks, self.work)
    }
}

fn check_measured(blocks: usize) -> MeasuredRun {
    assert_eq!(query_source_cold_measure(), None);
    let source = generic_call_site_source(blocks);
    let guard = start_query_source_cold_measure();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
    let parse_errors = parsed.diagnostics.iter().map(ToString::to_string).collect();
    let mut interner = crate::types::Interner::with_intrinsics();
    let checked = check_program(&mut interner, &parsed.program);
    let output = CheckOutput {
        diagnostics: checked.diagnostics,
        parse_errors,
        incomplete: checked.incomplete,
    };
    let work = query_source_cold_measure().expect("query measurement remains active");
    drop(guard);
    assert_eq!(query_source_cold_measure(), None);
    MeasuredRun {
        blocks,
        output,
        work,
    }
}

/// A scaled-down `tooling/bench/corpus/generics-*.ts`: every block is an
/// independent generic call site, so per-transaction work stays constant.
fn generic_call_site_source(blocks: usize) -> String {
    let mut source = String::from(
        "interface GenBox<T> { value: T; }\n\
         interface GenPair<A, B> { first: A; second: B; }\n\
         interface GenRecord<T> { current: T; next: GenBox<T>; }\n\
         function gen_id<T>(x: T): T { return x; }\n\
         function gen_box<T>(x: T): GenBox<T> { return { value: x }; }\n\
         function gen_unbox<T>(x: GenBox<T>): T { return x.value; }\n\
         function gen_pair<A, B>(a: A, b: B): GenPair<A, B> { return { first: a, second: b }; }\n\
         function gen_record<T>(x: T): GenRecord<T> { return { current: x, next: gen_box(x) }; }\n",
    );
    for block in 0..blocks {
        source.push_str(&format!(
            "const gen_num_{block}: number = gen_id({block});\n\
             const gen_str_{block}: string = gen_id(\"g_{block}\");\n\
             const gen_box_num_{block}: GenBox<number> = gen_box({block});\n\
             const gen_unboxed_{block}: number = gen_unbox(gen_box_num_{block});\n\
             const gen_pair_{block}: GenPair<number, string> = gen_pair({block}, \"p_{block}\");\n\
             const gen_nested_{block}: GenRecord<GenPair<number, string>> = gen_record(gen_pair({block}, \"n_{block}\"));\n\
             function gen_local_{block}<T>(x: T, b: GenBox<T>): T {{ return gen_id(gen_unbox(b)); }}\n\
             const gen_local_val_{block}: string = gen_local_{block}(\"x_{block}\", gen_box(\"x_{block}\"));\n"
        ));
    }
    source
}

fn assert_generic_call_site_semantics(run: &MeasuredRun) {
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert!(
        run.output.diagnostics.is_empty(),
        "{:#?}",
        run.output.diagnostics
    );
}

/// Every transaction pays for the whole accumulated program state, so total copy
/// volume must stay proportional to the transactions' own working set instead.
#[test]
fn semantic_query_transactions_copy_only_their_own_working_set() {
    let small = check_measured(SMALL_BLOCKS);
    let large = check_measured(LARGE_BLOCKS);
    assert_generic_call_site_semantics(&small);
    assert_generic_call_site_semantics(&large);

    // Premise: doubling the program doubles the transactions and the planning work.
    for (name, small_value, large_value) in [
        (
            "semantic query transactions",
            small.work.semantic_query_transactions,
            large.work.semantic_query_transactions,
        ),
        (
            "planner visits",
            small.work.planner_visits,
            large.work.planner_visits,
        ),
    ] {
        assert!(
            large_value <= 2 * small_value + GROWTH_SLACK,
            "{name} must grow linearly with the program: small={}, large={}",
            small.record(),
            large.record()
        );
    }

    let mut violations = Vec::new();
    for run in [&small, &large] {
        let bound = WORKING_SET_MULTIPLE * run.working_set();
        for (name, copied) in [
            ("fork-copied entries", run.work.semantic_state_fork_copied_entries),
            ("discarded entries", run.work.semantic_state_discarded_entries),
        ] {
            if copied > bound {
                violations.push(format!(
                    "{blocks} blocks: {name} {copied} exceeds {WORKING_SET_MULTIPLE}x working set {bound}",
                    blocks = run.blocks
                ));
            }
        }
    }
    for (name, small_value, large_value) in [
        (
            "fork-copied entries",
            small.work.semantic_state_fork_copied_entries,
            large.work.semantic_state_fork_copied_entries,
        ),
        (
            "discarded entries",
            small.work.semantic_state_discarded_entries,
            large.work.semantic_state_discarded_entries,
        ),
    ] {
        let bound = GROWTH_MULTIPLE * small_value + GROWTH_SLACK;
        if large_value > bound {
            violations.push(format!(
                "{name} grew from {small_value} to {large_value}, past the linear bound {bound}"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "semantic-query transaction copy volume is superlinear:\n{}\nsmall={}\nlarge={}",
        violations.join("\n"),
        small.record(),
        large.record()
    );
}

/// The layered relation cache trades a flat lookup for a walk down the open
/// savepoints, so the walk has to stay shallow rather than merely "bounded".
#[test]
fn layered_relation_cache_lookups_stay_shallow() {
    const MAX_DEPTH: u32 = 16;

    let source = generic_call_site_source(LARGE_BLOCKS);
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
    let mut interner = crate::types::Interner::with_intrinsics();
    start_relation_cache_depth_measure();
    let checked = check_program(&mut interner, &parsed.program);
    let depth = finish_relation_cache_depth_measure();

    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    assert!(depth.lookups > 0, "the corpus must exercise the cache");
    assert!(
        depth.max_depth <= MAX_DEPTH,
        "relation-cache layer walk must stay shallow: max={}, median={}, lookups={}",
        depth.max_depth,
        depth.median_depth(),
        depth.lookups
    );
}

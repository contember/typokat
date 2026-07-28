//! RED contract for the contextual argument re-walk under nested calls.

use super::{call_measure, reset_call_measure, CallMeasure};
use crate::check::checker::check_program;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Depths chosen so the doubling is visible while the RED run stays cheap. Walks per
/// phase are exactly `(3^d - 1)/2`, and two phases fire, so the total is `3^d - 1` —
/// 80 at depth 4 and 6560 at depth 8. Measured wall clock on these shapes: 2.4 ms at
/// depth 4, 20.5 ms at 8, 1352 ms at 12, and 11 984 ms at 14, the last on a 15-line
/// file. Memoizing the walk should leave one per level.
const SMALL_DEPTH: usize = 2;
const MID_DEPTH: usize = 4;
const LARGE_DEPTH: usize = 8;

/// How much the growth *factor* may itself grow from one doubling to the next.
///
/// A fixed bound on the factor cannot separate exponentials from polynomials: any
/// exponential looks polynomial once its base is small enough. Base 2 reaches
/// `walks(8)/walks(4) = 17`, which slips under a factor bound of 16 chosen to admit a
/// degree-4 polynomial — so such a bound goes green while the exponential remains.
///
/// Compare the *ratio of ratios* instead. For `walks ~ d^k` both doublings have the
/// same factor `2^k`, so the ratio of ratios is 1 whatever the degree; for `walks ~ b^d`
/// each doubling squares the factor, so it grows without bound. Today it is 4.1 and at
/// base 2 it is 1.7, while every polynomial sits at 1.
const FACTOR_GROWTH: u64 = 2;

/// Absolute headroom, so shapes whose counts are small enough for an additive constant
/// to dominate are not failed by rounding alone.
const GROWTH_SLACK: u64 = 64;

/// The control's depth. An argument that is neither a fresh literal nor an arrow makes
/// `context_can_shape_fresh_literal` return false, so the re-walk never runs and the
/// same nesting stays flat this far out — measured 3.1 ms at 4 and 2.3 ms at 20.
const CONTROL_DEPTH: usize = 20;

struct MeasuredRun {
    shape: &'static str,
    depth: usize,
    measure: CallMeasure,
}

impl MeasuredRun {
    fn contextual_walks(&self) -> u64 {
        self.measure.contextual_argument_walks()
    }

    fn record(&self) -> String {
        format!(
            "{} at depth {}: {} contextual argument walks ({} raw argument walks, \
             callbacks {:?}, fresh literals {:?})",
            self.shape,
            self.depth,
            self.contextual_walks(),
            self.measure.raw_call_argument_walks,
            self.measure.callback_rewalks,
            self.measure.fresh_literal_rewalks,
        )
    }
}

fn check_measured(shape: &'static str, depth: usize, source: fn(usize) -> String) -> MeasuredRun {
    let source = source(depth);
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();
    reset_call_measure();
    let checked = check_program(&mut interner, &parsed.program);
    assert!(checked.incomplete.is_empty(), "{:?}", checked.incomplete);
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    MeasuredRun {
        shape,
        depth,
        measure: call_measure(),
    }
}

/// Every level is a contextually typed arrow, the shape a nested callback chain has.
fn nested_arrows(depth: usize) -> String {
    let mut body = String::from("0");
    for index in (0..depth).rev() {
        body = format!("run(v{index} =>\n{body})");
    }
    format!("declare function run<T>(step: (value: number) => T): T;\nconst nested = {body};\n")
}

/// Every level is a fresh object literal passed through a generic call, the shape a
/// nested configuration literal has.
fn nested_object_literals(depth: usize) -> String {
    let mut body = String::from("0");
    for _ in 0..depth {
        body = format!("wrap({{ inner:\n{body} }})");
    }
    format!("declare function wrap<T>(value: {{ inner: T }}): T;\nconst nested = {body};\n")
}

/// The control: the argument is a plain call, so no contextual re-walk is possible.
fn nested_plain_calls(depth: usize) -> String {
    let mut body = String::from("0");
    for _ in 0..depth {
        body = format!("id(\n{body})");
    }
    format!("declare function id<T>(x: T): T;\nconst nested = {body};\n")
}

/// One nesting level costs the raw argument walk, the inference re-walk and the
/// committed re-walk — so an unmemoized re-walk makes each level branch three ways and
/// the whole check exponential in the nesting depth.
#[test]
fn nested_contextual_arguments_are_not_re_walked_exponentially() {
    let mut violations = Vec::new();
    for (shape, source) in [
        (
            "nested contextual arrows",
            nested_arrows as fn(usize) -> String,
        ),
        ("nested object literals", nested_object_literals),
    ] {
        let runs =
            [SMALL_DEPTH, MID_DEPTH, LARGE_DEPTH].map(|depth| check_measured(shape, depth, source));

        // Non-vacuity: the shape must actually reach the contextual walk at every depth,
        // so the bound below cannot be met by skipping the re-walk altogether.
        for run in &runs {
            let levels = u64::try_from(run.depth).expect("depth fits u64");
            assert!(
                run.contextual_walks() >= levels,
                "the corpus must contextually walk every nesting level: {}",
                run.record()
            );
        }

        let [small, mid, large] = &runs;
        // `large/mid <= FACTOR_GROWTH * (mid/small)`, cross-multiplied so the comparison
        // is exact rather than truncated by integer division.
        let observed = large.contextual_walks() * small.contextual_walks();
        let bound = FACTOR_GROWTH * mid.contextual_walks() * mid.contextual_walks() + GROWTH_SLACK;
        if observed > bound {
            violations.push(format!(
                "the growth factor is itself growing: depth {} to {} multiplied contextual \
                 walks by {}, then depth {} to {} multiplied them by {} — a polynomial \
                 repeats one factor, only an exponential squares it ({observed} > {bound}) \
                 — {}, then {}, then {}",
                small.depth,
                mid.depth,
                mid.contextual_walks() / small.contextual_walks().max(1),
                mid.depth,
                large.depth,
                large.contextual_walks() / mid.contextual_walks().max(1),
                small.record(),
                mid.record(),
                large.record(),
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "the contextual argument re-walk grows exponentially with nesting depth:\n{}",
        violations.join("\n"),
    );
}

/// The control that pins the exponent's source: the identical nesting, with an argument
/// the contextual walk declines to re-walk, stays flat far past the depth at which the
/// re-walking shapes hang.
#[test]
fn nested_plain_call_arguments_stay_flat_without_a_contextual_re_walk() {
    let run = check_measured(
        "nested plain generic calls",
        CONTROL_DEPTH,
        nested_plain_calls,
    );
    let levels = u64::try_from(CONTROL_DEPTH).expect("depth fits u64");
    assert_eq!(run.contextual_walks(), 0, "{}", run.record());
    assert_eq!(
        run.measure.raw_call_argument_walks,
        levels,
        "{}",
        run.record()
    );
}

//! A *failing* relation must be memoized, not re-derived per probe.
//!
//! Until [ADR-0016](../../../docs/decisions/0016-reason-free-relation-probes.md),
//! [`Relater::relate`] short-circuited a cached `true` but let a cached `false` fall
//! through to a full `relate_uncached` recompute, solely to rebuild the [`ReasonChain`]
//! the three-word cache does not store. Every rule that kept probing after a failure
//! therefore turned each failing probe into a branch point: cost was `arms^depth`, not
//! `arms * depth`. The dominant one was [`Relater::relate_union_target`];
//! `relate_signature_sets`, `relate_object_to_function`,
//! `relate_construct_signature_sets`, and the non-object branch of
//! `relate_source_members_to` had the same shape. A probe that discards its child's
//! reason now answers from the memo, and the guard below is what holds that.
//!
//! Measured on the release binary over exactly [`nested_discriminated_union_source`]
//! (nesting depth 9, 22-line files, only the union-arm count varying), before → after:
//!
//! | arms | 2 | 3 | 4 | 5 |
//! |---|---|---|---|---|
//! | before | 0.0032 s | 0.0181 s | 0.1781 s | 1.2259 s |
//! | after | 0.0023 s | 0.0027 s | 0.0025 s | 0.0027 s |
//!
//! The fitted exponent is the nesting depth. The identical shape with a *matching*
//! leaf ([`Leaf::Matching`]) is flat at the ~0.0025 s process floor for every arm
//! count and depth — so the exponent is the failure path, not the type graph. This is
//! not adversarial: it is the shape of a nested Redux-style discriminated union, where
//! `payload` sorts before the `type` discriminant so every arm walks the whole payload
//! before rejecting.
//!
//! The same split in relation frames, which is what the guard below asserts on
//! (2 arms, depth 4 → 8):
//!
//! | | frames | of those, rebuilds of a cached `false` |
//! |---|---|---|
//! | mismatched leaf, depth 4 | 77 | 63 |
//! | mismatched leaf, depth 8 | 1277 | 1251 |
//! | matching leaf, depth 4 | 9 | 0 |
//! | matching leaf, depth 8 | 17 | 0 |
//!
//! 98% of the pre-fix failing run's frames re-derived a verdict the cache already held,
//! and doubling the depth multiplied them by `ARMS^SMALL_DEPTH` instead of by ~2. The
//! post-fix counts are 14 and 26 frames with zero rebuilds — the same shape as the
//! always-flat matching-leaf control.
//!
//! The pins below fix the reporting behaviour the change had to preserve byte for byte.

use super::{
    relation_source_cold_measure, start_relation_source_cold_measure, RelationSourceColdMeasure,
};
use crate::check::checker::check_program;
use crate::diagnostics::{Diagnostic, DiagnosticCode};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Union-arm branch factor held fixed while the depth axis varies.
const ARMS: usize = 2;
/// The two depths the growth guard compares. Doubling the depth must not multiply
/// relation work by a factor that itself grows with depth.
const SMALL_DEPTH: usize = 4;
const LARGE_DEPTH: usize = 8;

/// Whether the innermost source payload matches its target — the only difference
/// between the exponential case and the flat control.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Leaf {
    Mismatched,
    Matching,
}

struct MeasuredRun {
    source: String,
    diagnostics: Vec<Diagnostic>,
    parse_errors: Vec<String>,
    work: RelationSourceColdMeasure,
}

fn check_measured(source: String) -> MeasuredRun {
    assert_eq!(relation_source_cold_measure(), None);
    let guard = start_relation_source_cold_measure();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
    let parse_errors = parsed.diagnostics.iter().map(ToString::to_string).collect();
    let mut interner = crate::types::Interner::with_intrinsics();
    let checked = check_program(&mut interner, &parsed.program);
    assert!(
        checked.incomplete.is_empty(),
        "{:?}",
        checked.incomplete.len()
    );
    let work = relation_source_cold_measure().expect("relation measurement remains active");
    drop(guard);
    assert_eq!(relation_source_cold_measure(), None);
    MeasuredRun {
        source,
        diagnostics: checked.diagnostics,
        parse_errors,
        work,
    }
}

/// A nested discriminated union: at every level the target is an `arms`-way union of
/// `{ payload; type }` objects sharing one payload, and the source picks arm `a0`. Only
/// the innermost `payload` decides the verdict, so a mismatch is discovered exactly
/// `depth` levels below every arm probe.
fn nested_discriminated_union_source(arms: usize, depth: usize, leaf: Leaf) -> String {
    let leaf_ty = match leaf {
        Leaf::Mismatched => "number",
        Leaf::Matching => "string",
    };
    let mut source = String::from("type T0 = { payload: string; type: \"t0\" };\n");
    source.push_str(&format!(
        "type S0 = {{ payload: {leaf_ty}; type: \"t0\" }};\n"
    ));
    for level in 1..=depth {
        let arms_text = (0..arms)
            .map(|arm| format!("{{ payload: T{}; type: \"a{arm}\" }}", level - 1))
            .collect::<Vec<_>>()
            .join(" | ");
        source.push_str(&format!("type T{level} = {arms_text};\n"));
        source.push_str(&format!(
            "type S{level} = {{ payload: S{}; type: \"a0\" }};\n",
            level - 1
        ));
    }
    source.push_str(&format!("declare const src: S{depth};\n"));
    source.push_str(&format!("const tgt: T{depth} = src;\n"));
    source
}

fn assert_single_assignment_failure(run: &MeasuredRun) {
    assert!(run.parse_errors.is_empty(), "{:?}", run.parse_errors);
    assert_eq!(run.diagnostics.len(), 1, "{:#?}", run.diagnostics);
    assert_eq!(run.diagnostics[0].code, DiagnosticCode::TK2322);
    assert_eq!(&run.source[run.diagnostics[0].span.range()], "tgt");
}

/// The guard. `uncached_relation_frames` counts `relate_uncached` entries, so it is the
/// direct measure of re-derivation; `pending_false_reason_rebuilds` names the mechanism
/// (a cached `false` that fell through inside one query transaction).
#[test]
fn failing_nested_union_relation_work_stays_polynomial_in_depth() {
    let small = check_measured(nested_discriminated_union_source(
        ARMS,
        SMALL_DEPTH,
        Leaf::Mismatched,
    ));
    let large = check_measured(nested_discriminated_union_source(
        ARMS,
        LARGE_DEPTH,
        Leaf::Mismatched,
    ));
    assert_single_assignment_failure(&small);
    assert_single_assignment_failure(&large);

    // Polynomial work is at most linear per level here (each distinct `(src, tgt)` pair
    // is decided once), so doubling the depth may at most double the frames plus a
    // fixed prelude offset. Exponential work multiplies them by `ARMS^SMALL_DEPTH`.
    assert!(
        large.work.uncached_relation_frames <= 4 * small.work.uncached_relation_frames,
        "doubling depth {SMALL_DEPTH}->{LARGE_DEPTH} at {ARMS} union arms must not \
         re-derive the failing subtree per probe: small={:#?}, large={:#?}",
        small.work,
        large.work
    );
    // The same bound on the mechanism itself: every one of these is a memo hit that
    // was thrown away and recomputed just to rebuild a reason the caller discards.
    assert!(
        large.work.pending_false_reason_rebuilds <= 4 * small.work.pending_false_reason_rebuilds,
        "a cached `false` must be a memo hit, not a re-derivation: small={:#?}, large={:#?}",
        small.work,
        large.work
    );
}

/// The control that isolates the exponent to the failure path: the identical type graph
/// with a matching leaf is already memoized flat today.
#[test]
fn succeeding_nested_union_relation_work_is_already_flat_in_depth() {
    let small = check_measured(nested_discriminated_union_source(
        ARMS,
        SMALL_DEPTH,
        Leaf::Matching,
    ));
    let large = check_measured(nested_discriminated_union_source(
        ARMS,
        LARGE_DEPTH,
        Leaf::Matching,
    ));
    assert!(small.diagnostics.is_empty(), "{:#?}", small.diagnostics);
    assert!(large.diagnostics.is_empty(), "{:#?}", large.diagnostics);

    assert!(
        large.work.uncached_relation_frames <= 4 * small.work.uncached_relation_frames,
        "a succeeding relation short-circuits on the cached `true`: small={:#?}, large={:#?}",
        small.work,
        large.work
    );
}

/// Reporting pin 1 — the union-target OR rule discards its arms' reasons and reports a
/// flat `NoUnionMember` headline with no elaboration. This is the exact text the
/// re-derivation currently buys nothing for, so a fix that suppresses arm reasons must
/// leave it byte-identical.
#[test]
fn nested_failing_union_target_keeps_its_flat_headline() {
    let run = check_measured(nested_discriminated_union_source(2, 2, Leaf::Mismatched));
    assert_single_assignment_failure(&run);
    assert_eq!(
        run.diagnostics[0].rendered_text(),
        "error[TK2322]: Type '{ payload: { payload: { payload: number; type: \"t0\" }; \
         type: \"a0\" }; type: \"a0\" }' is not assignable to type \
         '{ payload: { payload: { payload: string; type: \"t0\" }; type: \"a0\" } | \
         { payload: { payload: string; type: \"t0\" }; type: \"a1\" }; type: \"a0\" } | \
         { payload: { payload: { payload: string; type: \"t0\" }; type: \"a0\" } | \
         { payload: { payload: string; type: \"t0\" }; type: \"a1\" }; type: \"a1\" }'"
    );
}

/// Reporting pin 2 — the nested property cascade. A failure reached through plain
/// object properties keeps its full `because` chain; suppressing reasons on the OR
/// probes must not reach this path.
#[test]
fn nested_property_failure_keeps_its_full_reason_chain() {
    let run = check_measured(
        r#"interface Leaf { value: string }
interface Mid { leaf: Leaf }
interface Outer { mid: Mid }
declare const src: { mid: { leaf: { value: number } } };
const tgt: Outer = src;
"#
        .to_owned(),
    );
    assert_single_assignment_failure(&run);
    assert_eq!(
        run.diagnostics[0].rendered_text(),
        "error[TK2322]: Type '{ mid: { leaf: { value: number } } }' is not assignable to \
         type '{ mid: { leaf: { value: string } } }'\n  \
         Types of property 'mid' are incompatible.\n    \
         Types of property 'leaf' are incompatible.\n      \
         Types of property 'value' are incompatible.\n        \
         Type 'number' is not assignable to type 'string'."
    );
}

/// Reporting pin 3 — a missing target property surfaced through the nested cascade
/// (`TK2741`-shaped `MissingProperty` reason under a `Property` wrapper).
#[test]
fn nested_missing_property_failure_keeps_its_reason_chain() {
    let run = check_measured(
        r#"interface Leaf { value: string; extra: number }
interface Mid { leaf: Leaf }
declare const src: { mid: { leaf: { value: string } } };
const tgt: Mid = src.mid;
"#
        .to_owned(),
    );
    assert_single_assignment_failure(&run);
    assert_eq!(
        run.diagnostics[0].rendered_text(),
        "error[TK2322]: Type '{ leaf: { value: string } }' is not assignable to type \
         '{ leaf: { extra: number; value: string } }'\n  \
         Types of property 'leaf' are incompatible.\n    \
         Property 'extra' is missing in type '{ extra: number; value: string }'."
    );
}

/// Reporting pin 4 — `relate_source_members_to`'s non-object branch is the one OR probe
/// that *keeps* a failing candidate's reason (`last_child`). The headline elaboration
/// below names `boolean`, the **second** (last) intersection member, so any fix that
/// hands this probe a bare leaf changes this text.
#[test]
fn intersection_source_probe_keeps_its_last_candidate_reason() {
    let run = check_measured(
        r#"type Src = ((x: string) => number) & ((x: string) => boolean);
declare const s: Src;
const tgt: (x: string) => string = s;
"#
        .to_owned(),
    );
    assert_single_assignment_failure(&run);
    assert_eq!(
        run.diagnostics[0].rendered_text(),
        "error[TK2322]: Type '((x: string) => number) & ((x: string) => boolean)' is not \
         assignable to type '(x: string) => string'\n  \
         Call signature return types are incompatible.\n    \
         Type 'boolean' is not assignable to type 'string'."
    );
}

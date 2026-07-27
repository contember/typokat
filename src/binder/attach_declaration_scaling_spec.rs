//! Contract: attaching one declaration to a merged symbol costs the group's depth, not its size.
//!
//! A merge group's declaration list and its placement participant list are both append-only and
//! both already ordered — declarations arrive in source order. Re-deriving that order from
//! scratch on every attach, and scanning the whole list for a duplicate first, costs the group
//! once per member: quadratic in a heavily reopened name. The list it produces is byte-identical
//! either way, so no output assertion can see the difference; only a work counter can.
//!
//! The shapes measured here are the ones full `lib.d.ts` is made of — a reopened `interface`, a
//! reopened `namespace`, and a `declare function` overload set (backlog 88).

use super::bind::{
    bind_module_with_prelude, Binder, SymbolDeclarationAttachWorkForTest,
    SymbolDeclarationAttachWorkScopeForTest,
};
use super::declaration::{DeclId, DeclarationKind};
use super::namespace::{PlacementLookupWorkForTest, PlacementLookupWorkScopeForTest};
use super::symbol::Symbol;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// The two group sizes. Both bind one merge group of exactly that many declarations, so
/// "declarations" and "group size" are the same number and a cost that tracks their product
/// grows by the square of the step while a cost that tracks declarations grows by the step.
const SMALL: usize = 256;
const SCALED: usize = 1_024;

/// Per-declaration budget for both counters: one probe for the incoming declaration plus a
/// binary descent of its group, with room to spare. `32` covers groups of up to 2^30 members;
/// re-deriving the order per attach costs the whole group each time, which is already two
/// orders of magnitude past this at [`SCALED`].
const ROW_PROBES_PER_DECLARATION: u64 = 32;

/// One merge shape: a declaration reopened under one name, emitted one reopen per line.
struct MergeShape {
    what: &'static str,
    /// The name every reopen shares — the one merge group under measurement.
    merged_name: &'static str,
    /// The declaration kind each reopen contributes to that group.
    merged_kind: DeclarationKind,
    line: fn(usize) -> String,
}

const SHAPES: &[MergeShape] = &[
    MergeShape {
        what: "merged interface",
        merged_name: "Merged",
        merged_kind: DeclarationKind::Interface,
        line: |index| format!("interface Merged {{ member{index}: number }}\n"),
    },
    MergeShape {
        what: "reopened namespace",
        merged_name: "Reopened",
        merged_kind: DeclarationKind::Namespace,
        line: |index| format!("declare namespace Reopened {{ const value{index}: number; }}\n"),
    },
    MergeShape {
        what: "overload set",
        merged_name: "overloaded",
        merged_kind: DeclarationKind::Function,
        line: |index| format!("declare function overloaded(input{index}: number): void;\n"),
    },
];

impl MergeShape {
    fn source(&self, reopens: usize) -> String {
        (0..reopens).map(|index| (self.line)(index)).collect()
    }

    /// Bind the shape and hand back the binder plus everything the two counters saw.
    fn bind(&self, reopens: usize) -> BoundShape {
        let source = self.source(reopens);
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let parsed = Parser::new(&source_allocator, &source, SourceType::ts()).parse();
        assert!(!prelude.panicked, "the empty prelude parses");
        assert!(!parsed.panicked, "the {} fixture parses", self.what);

        let attach_scope = SymbolDeclarationAttachWorkScopeForTest::start();
        let placement_scope = PlacementLookupWorkScopeForTest::start();
        let binder = bind_module_with_prelude(&prelude.program, &parsed.program);
        let placement = placement_scope.finish();
        let attach = attach_scope.finish();
        BoundShape {
            binder,
            attach,
            placement,
        }
    }
}

struct BoundShape {
    binder: Binder,
    attach: SymbolDeclarationAttachWorkForTest,
    placement: PlacementLookupWorkForTest,
}

impl BoundShape {
    /// The one symbol every reopen merged into. Found by name over the whole table rather than
    /// by lookup, so a merge that silently split into two rows fails here instead of hiding
    /// behind whichever half a scope walk reaches first.
    fn merged_symbol(&self, shape: &MergeShape) -> &Symbol {
        let mut merged = None;
        for (_, symbol) in self.binder.symbols.iter() {
            if symbol.name != shape.merged_name || symbol.declarations.is_empty() {
                continue;
            }
            assert!(
                merged.is_none(),
                "the {} merged into more than one symbol",
                shape.what
            );
            merged = Some(symbol);
        }
        merged.unwrap_or_else(|| panic!("the {} publishes its merged symbol", shape.what))
    }

    /// The declarations of `kind`, in the order the source prewalk admitted them. The table is
    /// append-only and filled in source order, so this *is* source order.
    fn source_ordered(&self, kind: DeclarationKind) -> Vec<DeclId> {
        self.binder
            .declarations
            .iter()
            .filter(|declaration| declaration.kind == kind)
            .map(|declaration| declaration.id)
            .collect()
    }

    fn declarations(&self) -> u64 {
        u64::try_from(self.binder.declarations.len()).unwrap_or(u64::MAX)
    }
}

/// Merge order decides which declaration wins, and therefore the diagnostic text, so it is
/// pinned before anything is allowed to make ordering cheaper. Each shape's merged symbol must
/// list exactly its reopens, in source order, with no duplicates.
#[test]
fn a_merged_symbol_lists_its_declarations_in_source_order() {
    const REOPENS: usize = 64;

    for shape in SHAPES {
        let bound = shape.bind(REOPENS);
        let expected = bound.source_ordered(shape.merged_kind);
        assert_eq!(
            expected.len(),
            REOPENS,
            "the {} admits one declaration per reopen",
            shape.what
        );
        assert_eq!(
            bound.merged_symbol(shape).declarations,
            expected,
            "the {} merged its declarations out of source order",
            shape.what
        );
    }
}

/// The same pin under a group large enough that an ordering shortcut has room to go wrong:
/// an insert that lands one slot off, or a duplicate guard that stops catching a repeat, shows
/// up here and nowhere in the diagnostics.
#[test]
fn a_large_merge_group_keeps_source_order_and_no_duplicate() {
    for shape in SHAPES {
        let bound = shape.bind(SCALED);
        let declarations = &bound.merged_symbol(shape).declarations;
        assert_eq!(
            *declarations,
            bound.source_ordered(shape.merged_kind),
            "the {} merged {SCALED} declarations out of source order",
            shape.what
        );
        let mut seen = declarations.clone();
        seen.sort_by_key(|declaration| declaration.0);
        seen.dedup();
        assert_eq!(
            seen.len(),
            declarations.len(),
            "the {} attached a declaration twice",
            shape.what
        );
    }
}

/// Every shape x counter row is measured before anything is asserted, so one run reports the
/// whole table instead of stopping at whichever row happens to be first.
#[test]
fn attaching_a_merged_declaration_scales_with_declarations_not_with_group_size() {
    let step = u64::try_from(SCALED / SMALL).unwrap_or(u64::MAX);
    let mut measured = String::new();
    let mut over = Vec::new();

    for shape in SHAPES {
        let small = shape.bind(SMALL);
        let scaled = shape.bind(SCALED);

        // A silently degraded bind would attach nothing and probe nothing, so pin the work too.
        for (reopens, bound) in [(SMALL, &small), (SCALED, &scaled)] {
            assert_eq!(
                bound.merged_symbol(shape).declarations.len(),
                reopens,
                "the {} merged {reopens} declarations into one symbol",
                shape.what
            );
        }

        for (counter, small_probes, scaled_probes) in [
            (
                "symbol declaration",
                small.attach.row_probes,
                scaled.attach.row_probes,
            ),
            (
                "placement participant",
                small.placement.row_probes,
                scaled.placement.row_probes,
            ),
        ] {
            let growth = scaled_probes / small_probes.max(1);
            let small_budget = ROW_PROBES_PER_DECLARATION * small.declarations();
            let scaled_budget = ROW_PROBES_PER_DECLARATION * scaled.declarations();
            measured.push_str(&format!(
                "  {} / {counter}: {small_probes} -> {scaled_probes} probes ({growth}x for a \
                 {step}x group), budgets {small_budget} / {scaled_budget}\n",
                shape.what
            ));
            if growth > 2 * step || small_probes > small_budget || scaled_probes > scaled_budget {
                over.push(format!("{} / {counter}", shape.what));
            }
        }
    }

    assert!(
        over.is_empty(),
        "attach work grows with the merge group, not with the declarations it places — \
         {over:?} exceeded the budget:\n{measured}"
    );
}

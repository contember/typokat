---
id: 87
title: Reason chains have no depth cap and cost O(depth²) to render
blocked-by: []
---

# 87 — Reason chains have no depth cap and cost O(depth²) to render

**Summary.** `render_reason_chain` recurses without a depth limit and re-materializes the indent per
line, so one deeply-nested mismatch emits megabytes of stderr. `render_type` is carefully bounded;
the reason chain is not. Robustness, not benchmark — effort S.

## Problem

`src/diagnostics/reason.rs:66` builds `REASON_INDENT.repeat(depth)` inside `reason_lines`, which
recurses with `depth + 1` and no limit. The rendered result is stored in `Diagnostic::elaboration:
Vec<String>` for the whole run, so the cost is retained, not just streamed.

This is asymmetric with the neighbouring type renderer, which *is* bounded — `render_type` enforces
`DISPLAY_CHAR_LIMIT = 320` and `DISPLAY_DEPTH_LIMIT = 64`, breaks every loop on
`context.truncated`, and explicitly caps inspection of a single oversized identifier. There is also
no "…and N more" elision, which `tsc` does at roughly three levels.

Measured with `Ak = { p: A(k-1) }` vs `Bk = { p: B(k-1) }` producing exactly **one** diagnostic:

| depth | stderr bytes | lines | RSS |
|---|---|---|---|
| 100 | 14,783 | 103 | 8.0 MB |
| 400 | 178,283 | 403 | 11.6 MB |
| 1600 | **2,632,285** | 1603 | **32.0 MB** |

≈3.8× per doubling of depth → O(d²). At depth 1600 the deepest line is 3,253 characters of pure
indentation, and `lines == depth + 3` confirms nothing is ever elided.

It does not affect the benchmark — the `errors` corpus chains are depth 2, and the whole diagnostics
module measures 0.71% of a 100k-line run — so this is filed on robustness grounds: a single deeply
nested real-world mismatch produces unreadable, multi-megabyte output.

Related and deliberately kept separate: `render_tag_with_limit` (`src/diagnostics/render_type.rs:851-915`)
and `render_declared_recipe`'s `Array`/`Tuple`/`Readonly` arms (`:596-658`) recurse **without** going
through `render_type_inner`, so they bypass `DISPLAY_DEPTH_LIMIT` and carry no cycle guard. Believed
unreachable today because circular aliases are diagnosed as `TK2456` before a recipe is built — so the
missing guard is asymmetry, not a live bug — but it is cheap to restore and belongs in the same change.

## Approach / acceptance

Cap the chain at a fixed nesting depth in `render_reason_chain`, elide the remainder the way `tsc`
does, and clamp the indent so it cannot grow without bound. Thread the depth counter into the tag and
declared-recipe helpers so they honour the same `DISPLAY_DEPTH_LIMIT` as `render_type_inner`.

Acceptance: conformance fixtures pinning the elision text at the cap boundary; a deep-nesting fixture
whose output is bounded in both byte count and line width; **every existing diagnostic byte-identical**
(the cap must not fire below it). Note error-message text is part of the conformance corpus, so any
wording change needs its marker updated in the same commit.

## Touch points

`src/diagnostics/reason.rs` (`render_reason_chain`, `reason_lines`), `src/diagnostics/render_type.rs`
(`render_tag_with_limit`, `render_declared_recipe`), `tests/cases/` fixtures.

<!-- Origin: diagnostics complexity hunt, 2026-07-25 (findings 2 and 5). -->

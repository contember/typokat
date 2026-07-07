---
id: 54
title: Labeled statements are entirely unchecked
---

# 54 — Labeled statements are entirely unchecked

**Summary.** `foo: { const x: number = "s"; }` produces zero diagnostics — the whole
labeled body is invisible (2026-07-07 review, probe-verified; tsc TS2322).
`Statement::LabeledStatement` falls into the `_ => {}` catch-alls of both the statement
walk (`statements.rs:96-97`) and the flow builder (`flowgraph.rs:134-137`), and the gap
is not in the documented out-of-subset list (`for`/`for-of`/`do-while`/`try`).

## Problem

A silent unchecked-scope family, same class as backlog `21` (local classes): everything
inside a labeled statement — including a labeled `while` — is skipped without any
conservative fallback. Latent secondary issue for the fix: `break`/`continue` currently
ignore their label operand and target the innermost `flow_loops` frame
(`flowgraph.rs:114-133`), so labeled `break outer` needs label-aware edge targeting.

## Approach / acceptance

Walk the labeled body as its statement kind (label transparent for checking); wire
label-aware `break`/`continue` edges into the flow builder. Corpus: labeled block,
labeled `while` with `break label`/`continue label` narrowing shapes; cross-check tsc
6.0.3 --strict.

## Touch points

`src/check/checker/statements.rs` (statement walk), `src/check/checker/flowgraph.rs`
(labeled edges, `flow_loops` targeting).

<!-- Origin: cross-cutting soundness review 2026-07-07 (flow reviewer F5), leader-verified. -->

---
id: 130
title: Transition-oriented regression coverage across publication, copy and write boundaries
blocked-by: []
---

# 130 — Transition-oriented regression coverage across publication, copy and write boundaries

**Summary.** The as-const closure review found its one critical regression (an untyped binding
default silently dropping the leaf, so downstream writes were accepted) with hand-written probes,
not with the conformance corpus or the differential generator. Neither gate composes the
transitions that expose this class. Graduated from
[`checker-semantic-boundaries-and-maintainability.md`](../ideas/checker-semantic-boundaries-and-maintainability.md)
(findings S4 and unit U5). Effort M; non-blocking, but a coverage gap in the safe-direction
soundness gates. Filed 2026-09-09.

## Problem

- A fixture proves what it spells. `const x: "a" = value` can pass while `let copy = value` later
  has an incorrect writable type. Fixtures under `tests/cases/b44_as_const/` carry copy/write
  controls (`contextual_container_literal_modes.ts`, `binding_default_*.ts`), but only for the
  compositions someone wrote.
- The differential grammar (`tooling/differential/grammar.py`) already has a deterministic
  `generate_publication_transition` profile (satisfies → copy → write, behind
  `GenOptions.publication_transitions`), but it is exercised only by `test_differential.py`, not by
  the `fuzz` gate, and it has no producers for `as const`, object-binding patterns with defaults,
  class field initializers or enum keys — exactly the producers the regression lived in.
- A green generator run covers only the programs that generator produces (see the harness README's
  own motivation). Adding breadth must never be used to waive a known repro.

## Approach / acceptance

Extend the existing machinery; do not build a new harness.

1. **Wire the transition profile into the gate.** Make `fuzz` run the publication-transition
   profile alongside the random stream (a flag or a fixed share of the count), in regression,
   truth and self-consistency modes. Shrinking must preserve the transition that exposes a
   finding (the publish → copy → write chain), not reduce it to the immediate check.
2. **Add the missing producers**, each with a `let` copy and a valid same-value write plus an
   invalid different-value write: `as const` on scalars, objects, arrays, tuples and references;
   flat object-binding patterns with and without defaults; class field initializers; ready enum
   keys. Pin the R-B regression shape as a committed repro under `tooling/differential/repros/`
   and prove the negative control: the pre-repair binary (feature commit reconstructed without the
   R-B fix, or `main` at `27225f03` for the inherited shape) must diverge on it.
3. **Witness matrix for hand-written fixtures.** Add the minimum contrasts per axis below as
   conformance fixtures where the generator cannot reach them (display-hidden literal modes need
   direct store/protocol tests). A metamorphic copy test encodes a verified expected relationship;
   ordinary widening is sometimes the correct result, so keep the legitimate widening negative
   controls.

| Axis | Minimum contrasts |
|---|---|
| Literal contribution | ordinary vs explicitly preserved; same-value and different-value mixed modes |
| Context | absent; broad primitive; exact literal; literal with `undefined`/`null`; boolean; both union orders |
| Producer | scalar, object member, array element, tuple element, arrow expression body, single-return block, binding default, class field |
| Ownership | literal-owned nested shape vs identifier/member reference to an existing container |
| Later use | immediate annotation, `let` copy, member/element copy, valid same-value write, invalid different-value write |
| Callable contract | contextual target vs explicit return annotation vs inferred source return; nested callback |
| Transport | declaration publication, member access, substitution, generic argument/return where already admitted |
| Lifecycle | cold/repeated query, selected/rejected candidate, exhausted query |

Acceptance: the committed repro corpus grows by the pinned transition repros with `MATCH` or
ledgered `ALLOWED` status against pinned tsc 6.0.3; `repros --check` stays an identity ratchet;
the negative control is recorded in the harness README; no existing fixture, scoreboard or
divergence-ledger row changes without an owner. Check diagnostics, incompletes, cardinality and
ownership, not only type values.

Gates: `python3 differential.py fuzz --ref tsc` over the new profile, `repros --check`,
`cargo test`, ordinary exact conformance with the current receipt.

## Touch points

`tooling/differential/{grammar.py,differential.py,shrink.py,test_differential.py,README.md,repros/,scoreboard.txt}`,
`tests/cases/b44_as_const/`, `tests/cases/README.md`.

<!-- Origin: ideas/checker-semantic-boundaries-and-maintainability.md (S4/U5) and the archived as-const sprint closure review, 2026-09-09. -->

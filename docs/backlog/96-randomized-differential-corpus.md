---
id: 96
title: A randomized differential corpus, because the fixture corpus is blind
blocked-by: []
---

# 96 — A randomized differential corpus, because the fixture corpus is blind

**Summary.** An unsound change altered output on ~15 % of randomly generated nested-contextual
programs while showing **zero diff** across every gate this project has: 471 fixtures in two formats,
project mode in both file orders, eight bench corpora, the official-suite ratchet, and a
2,193-binding inferred-type probe. A throwaway fuzzer found it in minutes. Make that fuzzer
permanent. Effort M. **Highest-leverage item on the board.**

## Problem

`412f321` (reverted by `40540a1`) served a memoized walk performed with a contextual parameter
unbound to a later walk that ran with it bound. It dropped diagnostics, invented others, changed
overload verdicts, and rendered wrong types in messages.

Every existing gate passed it:

| gate | result on the unsound change |
|---|---|
| 471 `tests/cases` fixtures, rich + compact | 0 diff |
| every case directory as a project, both file orders | 0 diff |
| 8 bench corpora incl. `errors-100000`, `generics-100000` | 0 diff |
| official-suite ratchet | 0 regressions |
| 2,193 top-level bindings forced to render inferred types | 0 diff |
| **ad-hoc fuzzer, 400 files, depth 1–3** | **68 differ (17 %)** |
| **ad-hoc fuzzer, 700 files, depth 1–4, in a class method** | **102 differ (14.6 %)** |

The reason is not that the fixtures are weak. It is that the trigger shape — *an argument a
contextual re-walk can supersede (arrow / fresh object or array literal), nested inside a
contextually typed callback, whose value depends on that callback's contextually typed parameter* —
does not occur anywhere in the corpus. A hand-written corpus covers the cases someone thought of.

This is the second time this sprint that a gate was blind by construction: the conformance markers
match code multisets per line, which is why `2^d` duplicate diagnostics survived eleven days
(`92`, shipped as `243a878`), and the ≤2 % regression gate never measured a multi-file
corpus, which is how a 3× multi-file constant walked in ([`94`](./94-flat-per-file-regression-since-july-9.md)).

## Approach / acceptance

A generator plus a differential runner, in `tooling/` alongside the other harnesses (it is test
tooling, so it does **not** live under `docs/`). Shape it after `tooling/official-suite/`: a
committed scoreboard, `--check` exiting 1 on divergence, black-box against a prebuilt binary.

Requirements that matter:

- **Differential against a reference**, not against a baseline of our own output — otherwise it
  freezes current bugs. Two modes: against `tsc 6.0.3 --strict` (finds real divergences) and against
  a previous typokat binary (finds regressions in a change under review). The second is what a work
  unit runs; the first is what the project runs periodically.
- **Seeded and reproducible.** A failure must reduce to a committed minimal repro, not to "seed 42
  file 317".
- **Shrinking.** The review's finding reduced to three lines with no generics; that reduction is what
  made it actionable. Automate it or the reports will be unusable.
- **Grammar aimed at composition, not syntax coverage.** The bug lived in the interaction of nesting,
  contextual typing, and parameter binding. Generate deep compositions of a few constructs rather
  than broad shallow coverage of many.
- Start with the region that just failed — calls, contextual arguments, arrows, object/array
  literals, overloads, generics, `this`/class context — and grow.

Acceptance: the harness reproduces `412f321`'s divergence from a clean checkout; it runs in CI on a
bounded budget; and it is wired into `docs/reference/dev-method.md` §1 as a required gate for changes
touching inference or contextual typing, so the next work unit cannot report "zero diff" without it.

## Touch points

New `tooling/differential/` (generator, runner, shrinker, scoreboard),
`docs/reference/dev-method.md` §1, `.github/workflows/ci.yml`.

<!-- Origin: independent adversarial review of 412f321, 2026-07-26. The review's ad-hoc fuzzer found
     in minutes what five committed gates missed entirely. -->

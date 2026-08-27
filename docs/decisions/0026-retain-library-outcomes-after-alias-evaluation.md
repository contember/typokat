---
id: 0026
title: Retain library outcomes after complete recursive-alias evaluation
status: accepted
date: 2026-08-27
---

# 0026 — Retain library outcomes after complete recursive-alias evaluation

## Context

[ADR-0025](0025-rebaseline-library-records-after-immutable-aliases.md) interpreted a transient
`558 -> 411` library-record movement as the removal of 148 diagnostics caused by recursive-alias
recovery. It required a 410-row authority after fixing the additional `Awaited` `TK2456`.

That interpretation did not survive complete conditional-alias evaluation. The 410-row result
stopped at the immutable alias application instead of evaluating its conditional body. It therefore
also failed the existing M25 contracts for distributive aliases, non-distributive aliases, and
recursive conditional aliases. Once those semantics were restored, a named comparison showed that
none of the 148 old outcomes had disappeared:

- 19 `TK2344` rows moved only in rendered detail at identical codes and sites;
- five `TK2430` rows first became `TK2589` at identical sites because alias projection inherited
  the class-specific 128-expansion cap;
- one unmatched `TK2589` came from eagerly traversing an ordinary alias-backed union.

Giving aliases the existing semantic step budget and keeping ordinary union aliases one-layer lazy
removed all six budget regressions. The final live census is again 558 rows: 267 diagnostics and
291 incomplete surfaces. Its only named drift is the 19 remove/add rendering pairs at the same
`TK2344` sites in `lib.dom.d.ts` (`FileReader` and `XMLHttpRequestEventTarget` constraints).
TypeScript 6.0.3 checks the packaged library profile clean, but that establishes that the rows are
typokat model-gap debt; it does not prove that a typokat outcome disappeared.

## Decision

ADR-0026 supersedes ADR-0025 in full. We retain 558 library-owned outcomes as the current
[ADR-0018](0018-pin-library-owned-records-as-a-named-census.md) authority: 267 diagnostics and 291
incomplete surfaces.

We will re-pin only the 19 `TK2344` detail changes. Every replacement must keep the same code and
source site. No diagnostic or incomplete outcome may be added or removed by that rebaseline.

Recursive alias applications use a dedicated projection limit equal to the existing semantic step
budget, not the class-projection limit. A distributed conditional alias may recursively demand the
union it produced. An ordinary union alias remains one-layer lazy. Runaway recursive aliases still
terminate with `TK2589`.

The test-only `ForceAllRecordAudit` targets the resulting 558-row named authority. It must reproduce
current checker outcomes from isolated scratch semantics; it must not copy the fixture or call the
complete-source route.

## Consequences

- M25 distribution, legal `Awaited`-style recursion, direct-cycle `TK2456`, and runaway `TK2589`
  share one immutable alias model.
- The library census does not claim model-gap progress from an under-evaluated type.
- Count stability alone is insufficient. The 19 rendering replacements remain visible in the named
  fixture diff.
- A future reduction in library-owned outcomes still requires the named causal process in
  ADR-0018.
- WU4 force-all parity returns to 558 exact records, including their updated rendered detail.

## Alternatives considered

- **Keep the 410-row target and suppress the restored outcomes.** Rejected because it would encode
  under-evaluation as expected behavior and can hide user-code errors.
- **Pin the transient 559-row result.** Rejected because its six `TK2589` rows were bounded planner
  regressions, not stable checker outcomes.
- **Wrap every conditional alias body in a generic lazy instantiation.** Rejected because it fixed
  M25 but restored recovery-shaped library details and did not preserve the intended one-layer
  alias boundary.

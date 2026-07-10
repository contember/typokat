---
id: 0003
title: GO on the backlog 38 minimal ambient prelude — schedule a later spec-first sprint
status: accepted
date: 2026-07-10
---

# 0003 — GO on backlog `38` (minimal ambient prelude), scheduled for a later sprint

## Context

The sprint-2026-07-10 soundness-review WU7 requires an explicit go/defer gate for
backlog [`38`](../backlog/38-minimal-ambient-prelude.md) (a small, hand-curated
ambient prelude — `console`, a few `string`/`number` members, `Math`, simple `Array`
members — loaded via the M28 embedded-prelude mechanism). The gate rule: *approve a
later spec-first sprint only when the audited lead time to `14` (full `lib.d.ts`) is
long enough that `38`'s replaceable early real-world signal repays the temporary
semantic surface.*

The lead time to `14` is **long**. The TS 6.0.3 lib audit
([`../backlog/lib-audit-6.0.3.md`](../backlog/lib-audit-6.0.3.md)) confirms `14` is
blocked by the whole track-A model-completeness chain — `41` (generic methods), `43`
(namespaces + declaration merging), and the audit-discovered `70` (`this`-parameter
typing) — none of which has started, plus `42`/`44` for full model completeness.
Meanwhile `38` reuses an already-shipped mechanism (the M28 prelude compilation unit)
and is over-report-safe by construction: it curates its declarations around the model
gaps and makes **no** lib-fidelity promise.

## Decision

**GO — approve `38` as a later, separately-scheduled spec-first sprint**; keep the
backlog item alive. It carries **no implementation budget in the 2026-07-10 sprint**
and does not widen that sprint's WU8. It should be scheduled *after* the current
soundness sprint and interleaved per the backlog's recommended order (kill the
silent-FN C tail first); it is otherwise independent and can be picked up whenever the
real-world signal is wanted.

- **Prerequisites.** The M28 prelude mechanism (shipped) and M32/M33 signature shape +
  overloads (shipped). No new architecture. Generic-method-shaped ambient signatures
  are dodged/simplified until `41` lands.
- **Owner.** Unassigned (leader-scheduled); the spec-first sprint's WU0 writes the
  curated fixture corpus first.
- **Witness.** Fixtures using the curated names check correctly vs `tsc 6.0.3`; the
  official-suite `OOS:unresolved` bucket shrinks; the scoreboard `clean-kept` rate
  ratchets up. (No scoreboard rewrite outside that sprint's audited close.)
- **Replacement path.** Backlog `14`'s full loader **replaces** this slice when it
  starts — there is never a second ambient-loading path (already stated in items `14`
  and `38`).

## Consequences

- Early real-world contact and a wider regression net arrive long before the full
  track-A chain completes, at bounded, throwaway cost.
- A temporary, deliberately-low-fidelity ambient surface exists during track A. It is
  safe-direction (curated around gaps, no fidelity promise), so it does not weaken the
  soundness posture; it is deleted when `14` lands.
- This constrains future scheduling: `38` is a sanctioned pre-`14` slice, not a
  competing lib loader. Rejected alternative — **DEFER** — would have delayed all
  real-world signal until `41` + `43` + `70` + `14` all shipped (the entire model
  track), leaving lib-dependent regressions uncaught and the clean-file over-report
  rate unmeasured for that whole span; the only saving was avoiding a bounded,
  already-mechanised, throwaway surface. The long lead time makes that trade lose.

## Alternatives considered

- **DEFER `38` until `14`** (delete the item, fold the need into `14`). Rejected: the
  audited lead time to `14` is the full track-A chain; the early signal repays the
  temporary surface, which is the exact GO condition in the WU7 gate.
- **Implement `38` now, inside the 2026-07-10 sprint.** Rejected: that sprint is
  soundness-fixes-only and explicitly gives `38` no implementation budget; feature
  breadth follows the soundness tail.

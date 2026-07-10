---
id: 38
title: Minimal ambient prelude slice (early real-world signal)
---

# 38 — Minimal ambient prelude slice

**Decision: GO** ([ADR-0003](../decisions/0003-backlog-38-minimal-prelude-go.md), 2026-07-10).
The WU7 go/defer gate resolved **GO** — the audited lead time to `14` is the whole track-A
chain (`41` → `43` → `70` → `14`; see [`lib-audit-6.0.3.md`](lib-audit-6.0.3.md)), long enough
that this slice's replaceable early real-world signal repays its bounded, throwaway surface.
Scheduled as a **later, separate spec-first sprint** (no budget in the 2026-07-10 sprint),
after the silent-FN C tail per the recommended order; `14`'s full loader replaces it, never a
second ambient path. Prerequisites / owner / witness / cost are in the ADR.

**Summary.** The "earlier minimal ambient/prelude slice" that backlog `14` explicitly
permits, now approved: a deliberately small, hand-curated ambient set (`console`, a few
`string`/`number` members like `.length`, `Math`, simple non-generic `Array` members)
loaded as a frozen prelude — explicitly NOT pretending to be `lib.d.ts`.

## Problem

First real-world contact currently waits for full `lib.d.ts` (`14`), which is blocked by
the whole model-completeness track. A minimal slice (a) moves many `OOS:unresolved`
official-suite files into scope, widening the regression net cheaply; (b) exercises
ambient loading via the M28 prelude compilation-unit mechanism ahead of scale; (c) starts
paying down the clean-file over-report rate (scoreboard `clean-kept` 172/219 — safe, but a
usability ceiling worth an upward ratchet target).

## Approach / acceptance

Extend the M28 embedded-prelude mechanism (`src/prelude.ts`) with a small ambient
value/type set. Declarations must stay inside the implemented model — optional/default/rest
signature shape is available since M32 and overloads since M33, but generic-method-shaped
signatures still need to be dodged or simplified until `41` lands (the slice makes no
lib-fidelity promise).
Acceptance: fixtures using the curated names check
correctly vs tsc; the official-suite `unresolved` bucket shrinks; `clean-kept` ratchets
up. When `14` starts, the full loader **replaces** this slice — no second ambient-loading
path.

## Touch points

`src/prelude.ts` + the prelude compilation-unit path (`src/driver.rs`, binder seeding);
official-suite scoreboard re-save.

<!-- Origin: graduated from ideas/minimal-prelude-slice.md (2026-07-07); permitted by backlog 14. -->

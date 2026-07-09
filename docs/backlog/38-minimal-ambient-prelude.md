---
id: 38
title: Minimal ambient prelude slice (early real-world signal)
---

# 38 — Minimal ambient prelude slice

**Summary.** The "earlier minimal ambient/prelude slice" that backlog `14` explicitly
permits, now scheduled: a deliberately small, hand-curated ambient set (`console`, a few
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

---
id: 91
title: Missing required properties should be reported before value mismatches
blocked-by: []
---

# 91 — Missing required properties should be reported before value mismatches

**Summary.** `relate_objects` relates property values in name order, so a missing required property is
reported as `TK2322` on an earlier property whenever that property also fails. `tsc` checks presence
first and reports `TS2741`. Effort M.

## Problem

`crates/typokat-relate/src/relate/relation/objects.rs:170-261` walks the target's properties in canonical name order and
relates each value as it goes. `tsc` runs `getUnmatchedProperty` over all required target properties
**before** any value relation, so a genuinely missing property wins over a value mismatch on a
property that happens to sort earlier.

Ours therefore depends on property ordering and on which sub-relation fails first. Repro
(`tsc 6.0.3 --strict` reports `TS2741: Property 'p1' is missing in type 'N2' but required in type 'N0'`):

```ts
interface N0 { p0?: N2 & N0; p1: N0; }
interface N1 { p0: N1; p1: N2 | N0; }
interface N2 { p0: N1; }

declare const a: N1;
declare const b: N2;

const x: N0 = a;
const y: N0 = b;   // tsc: TS2741 · typokat: TK2322 "Types of property 'p0' are incompatible."
```

Surfaced while reviewing the reason-free relation probes ([ADR-0016](../decisions/0016-reason-free-relation-probes.md)):
before that change the cache-vs-cycle interaction happened to make `p0` succeed here, so `TK2741` fell
out by accident. It was never the result of a presence rule, and the ordering dependence was always
latent — the change only made it visible. The resulting divergence is recorded in
[`divergences.md`](../reference/divergences.md).

Severity is quality, not soundness: an error is still reported on the right line with the right span,
and the relation verdict (`No`) is correct. But `TK2741` names the actual defect and `TK2322` sends the
reader to an unrelated property.

## Approach / acceptance

Add a presence pass over all required target properties before any value relation in `relate_objects`,
mirroring `getUnmatchedProperty`. Decide explicitly what happens when a target has both a missing
required property and a genuine value mismatch — `tsc` reports the missing property; match that, and
say so in the fixture.

Acceptance: the repro above reports `TK2741` in **every** declaration ordering; conformance stays green;
the official-suite ratchet shows no regressions and preferably some progress; the
`divergences.md` entry is removed in the same commit. Watch the excess-property and
intersection-target paths, which also enumerate target properties.

## Touch points

`crates/typokat-relate/src/relate/relation/objects.rs` (`relate_objects`), `tests/cases/`,
`docs/reference/divergences.md`.

<!-- Origin: independent adversarial review of ADR-0016, 2026-07-25 (Finding 1, fix option 2). -->

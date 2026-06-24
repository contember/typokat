# Findings surfaced by the official-suite harness

Recorded for the checker-dev loop (HANDOFF.md). These came out of running
`tsofficial.py` against the official tsc conformance baselines, then triaged into
deferred-feature vs current-impl bug (see Classification). Fold into HANDOFF's
roadmap when convenient.

## F1 — Method / call signatures in object & interface types are dropped → `{}`

**The gap.** A method-shorthand or call-signature *member* in an object type or
interface (`{ f(x: number): void }`, `interface T { f(x): void }`) is not modeled:
typokat lowers the type as if the member weren't there, so it collapses to `{}`.

**Repro:**
```ts
interface T { f(x: number): void; }
declare var t: T;
const n: number = t.f;   // typokat: TK2339 "Property 'f' does not exist on type '{}'"
t = () => 1;             // tsc: TS2322 (fn not assignable to T); typokat diverges
```

**Evidence.** On `conformance/.../assignmentCompatWithCallSignatures2.ts` tsc's
baseline has **12** errors; typokat reports **4**, and renders the target interface
`T` as `{}`. It recurs across `assignmentCompatWithCallSignatures*`,
`assignmentCompatWithConstructSignatures*`, and `covariantCallbacks.ts` — a steady
source of false negatives in the in-scope diff.

**Why it matters.** This is *not* the same as class methods (M11, supported). It's
method/call/construct signatures as **members of object-type literals and
interfaces** — a distinct, well-isolated feature not covered by M0–M22. Candidate
for its own milestone. Likely also implicates a secondary divergence: a function
appears non-assignable to the (wrongly empty) `{}` target — verify whether
`fn → {}` itself is a real typokat bug when triaging.

## Classification — deferred vs current-impl bug (faithful binary, 874-test corpus)

Across the curated corpus (in-scope 494) the discrepancies split roughly:

- **False negatives (1473): ~98% deferred.** 75% are codes typokat can't even emit
  (definite-assignment `2454/2564`, duplicate-id `2300/2374`, interface-extends
  `2430`, override `2416`, comparison `2367`, no-overload, …); the rest are
  signatures-as-members (F1), flow narrowing (M23), ES `#private`, intersection
  `A&B`, generic constraints (M24). Only ~20 trace to current-impl gaps (F4/F5).
- **False positives (552): ~40% (~150) are ONE current-impl bug** — F3 below. The
  rest are deferred: dropped object/interface signatures (F1, ~201), can't-narrow
  over-reports (M23, ~118, safe direction), and `strictNullChecks` divergence
  (typokat is always strict; tsc's old tests default to non-strict).

> Caveat that bit this analysis: the prebuilt `target/release/typokat` was **stale**
> (pre-M22 — 0 of 18 `TK2304` on its own m22 fixtures). All numbers/probes here are
> from a freshly rebuilt binary. Rebuild before re-checking.

## F3 — Class member-collection drops parameter properties + inferred fields (REAL BUG)

**The bug.** Class members declared without an explicit type annotation aren't
collected, so every access reports a spurious `TK2339` (and the member's
access-control / type checks never run). Two common forms trip it:

- **parameter properties** — `constructor(public x: number, private y: string)`
- **initializer-inferred fields** — `f = () => 1`, `g = 2` (no annotation)

**Repro (faithful binary):**
```ts
class C { constructor(public x: number, private y: string) {} }
const c = new C(1, "a");
const n: number = c.x;   // typokat: TK2339 'x' does not exist  (tsc: ok)
c.y;                     // typokat: TK2339 'y' does not exist  (tsc: TS2341 private)

class D { f = () => 1; g = 2; }
new D().g;               // typokat: TK2339 'g' does not exist  (tsc: ok)
```

**Why it matters.** Unlike F1 this is a hole in *implemented* class support, and it
is **unsound in the false-positive direction** — typokat rejects valid, idiomatic
code. It is the single largest source of over-reports: ~150 spurious `TK2339`
across ~38 corpus files (6 parameter-property, ~32 initialized-field). The
hand-written corpus missed it because its class fixtures all use annotated fields
(`private x: string`), which collect fine. **Highest-value fix.**

## F4 — Accessibility not checked through a destructuring pattern (false negative)

`let { priv } = k` / `function f({ priv }: K)` doesn't run private/protected access
checks. tsc reports `TS2341`/`TS2445`; typokat is silent. Under-reports (safe-ish).
```ts
class K { private priv = 1; }
let { priv } = new K();   // typokat: silent   (tsc: TS2341)
```

## F5 — readonly / property not enforced through union member access (false negative)

Assigning to a member reached through a union of object types skips the `readonly`
(and union-property-existence) check.
```ts
type A = { readonly value: number }; type B = { readonly value: number };
declare const u: A | B;
u.value = 12;             // typokat: silent   (tsc: TS2540 read-only)
```

## F6 — readonly assignment suppresses the value-type cascade (minor)

`this.ro = 5` where `ro: 1` is `readonly` reports `TK2540` (read-only) but not the
`TK2322` (`5` not assignable to `1`) tsc also emits. The line *is* flagged, so it's
a code-precision nit, not a dropped error.

## Context — expected false negatives (NOT bugs, just out of current scope)

The harness's in-scope FN bucket is currently dominated by deferred features that
look in-scope syntactically and so aren't auto-gated. Expected to convert to
`matched` as the milestones land — don't chase them as bugs:

- **Flow narrowing (M23).** `typeof`/truthiness/equality narrowing across
  *unstructured* flow (early `return`, function scope) — typokat only narrows
  structured `if`/`else`/`switch`. Syntactically identical to the supported case,
  so the gate can't separate them; the `controlFlow/` and `typeGuards/typeof*`
  tests land here.
- **Type predicates / assertion signatures** (`x is T`, `asserts x`) — deferred;
  now gated out by syntax bucket.
- **Definite-assignment analysis** — `TS2454` (used before assigned) and `TS2564`
  (no initializer / not definitely assigned). `TS2564` isn't even in typokat's code
  set. Deferred (`tests/cases/README.md`).

## F2 — 2 tests hit typokat's own parser (`parse-error` bucket)

Two fetched tests are rejected by typokat's parser (oxc) rather than checked. Low
priority (likely exotic syntax), but worth a glance — `run` buckets them as
`parse-error`; list them with a quick report query if you want to look.

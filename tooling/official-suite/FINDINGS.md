# Findings surfaced by the official-suite harness

Recorded for the checker-dev loop (HANDOFF.md). These came out of the first runs
of `tsofficial.py` against the official tsc conformance baselines. Not yet triaged
— just logged so they aren't lost. Fold into HANDOFF's roadmap when convenient.

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

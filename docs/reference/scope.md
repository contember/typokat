# Diagnostic scope map

Which `tsc` diagnostics typokat is *meant* to cover, and which whole categories
are out of scope **by design**. This is a description of the checker's shape — the
boundary of the type model — not a schedule. The authoritative **live** coverage
(the codes actually emitted today) is the `DiagnosticCode` enum in
[`src/diagnostics.rs`](../../src/diagnostics.rs) and the coverage table in
[`README.md`](../../README.md); the roadmap is [`backlog/`](../backlog/README.md).

Codes use the `TK` prefix; the number mirrors `tsc` exactly (`TK2322` ≡ `TS2322`).

## The boundary in one line

typokat models TypeScript **types**. In scope: the semantic/type errors a strict
`tsc --noEmit --strict` reports about an already-parsed `.ts` file or, for the M29
slice, a local-relative `.ts` project with named imports/exports. Out of scope by
design (per [`CLAUDE.md`](../../CLAUDE.md)): **parsing** (oxc owns it), **emit**,
**JS runtime semantics**, full package/tsconfig **module resolution**, and
**compiler/CLI configuration**. Everything below is a consequence of that one line.

## Error codes by range

`tsc` has 1332 `Error`-category codes (plus ~765 non-error messages/suggestions,
all out of scope). They bucket by leading digits — and the bucket alone decides
most of in/out:

| Range | Count | Theme | In scope? |
|---|---:|---|---|
| `1xxx` | 398 | parse / grammar | **No** — oxc parses; we consume its AST and never re-emit these as `TK` codes |
| **`2xxx`** | **529** | **semantic / type** | **Yes** — the core (with a handful of module/emit strays called out below) |
| `4xxx` | 109 | declaration emit | **No** — typokat does not emit |
| `5xxx` | 65 | compiler options / tsconfig | **No** — no compiler-config surface |
| `6xxx` | 50 | CLI / file / driver messages | **No** — not type errors |
| `7xxx` | 45 | `noImplicitAny` & strict-lint | **Yes** — part of the strict type model |
| `8xxx` | 35 | `.js` / JSDoc / `checkJs` | **No** — typokat checks `.ts` only |
| `9xxx` | 34 | misc | mixed — case by case |
| `18xxx` | 67 | jsx-runtime / import-attributes / target-gating | **Mostly no** — emit/resolution; a few strict-null strays |

So the in-scope universe is essentially **`2xxx` + `7xxx`**, minus the `2xxx`
module/emit exceptions listed at the end.

## In-scope tiers

Tiered by **centrality to the type model**, not by schedule. Tier S is the reason
the project exists; Tier B is the long tail that the same machinery eventually
reaches. The exact `tsc` message text is shown so the wording is pinned.

### Tier S — structural core

The structural assignment-and-member engine. This *is* typokat; most of it is
already emitted.

**Assignability & calls**
- `TK2322` Type '{0}' is not assignable to type '{1}'.
- `TK2345` Argument of type '{0}' is not assignable to parameter of type '{1}'.
- `TK2554` Expected {0} arguments, but got {1}. · `TK2555` Expected at least {0} arguments, but got {1}.
- `TK2391` Function implementation is missing or not immediately following the declaration.
- `TK2394` Overload signature is not compatible with its implementation signature.
- `TK2769` No overload matches this call.
- `TK2741` Property '{0}' is missing in type '{1}' but required in type '{2}'. · `TK2739`/`TK2740` (missing **multiple** properties).
- `TK2353` Object literal may only specify known properties… (excess property).
- `TK2559` Type '{0}' has no properties in common with type '{1}'.

**Names & member access**
- `TK2304` Cannot find name '{0}'.
- `TK2339` Property '{0}' does not exist on type '{1}'. · `TK2551` …Did you mean '{2}'?
- `TK2693` '{0}' only refers to a type, but is being used as a value here (value/type symbol spaces).

**Classes (nominal + structural OO)**
- `TK2341` private · `TK2445` protected · `TK2540` assign to read-only · `TK2511` instantiate abstract.
- `TK2515` Non-abstract class does not implement inherited abstract member.
- `TK2416` Property in derived type is not assignable to the same property in base (override compat).
- `TK2417` Class static side incorrectly extends · `TK2420` Class incorrectly implements interface.
- `TK2451` Cannot redeclare block-scoped variable '{0}'.

### Tier A — strict type model & flow

The strict-mode and flow-sensitive layer: nullability, definite assignment,
operators, returns, generic constraints, implicit `any`.

**strict null / flow**
- `TK2531` Object is possibly 'null'. · `TK2532` …'undefined'. · `TK2533` …'null' or 'undefined'.
- `TK2571` Object is of type 'unknown'.
- `TK2454` Variable used before being assigned. · `TK2448` used before its declaration. · `TK2564` property has no initializer and is not definitely assigned.

**operators & narrowing**
- `TK2367` This comparison appears to be unintentional… have no overlap.
- `TK2365` Operator '{0}' cannot be applied to types '{1}' and '{2}'. · `TK2356`/`TK2362`/`TK2363` arithmetic operand must be number/bigint/enum.

**functions & returns**
- `TK2355` A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.
- `TK2366` Function lacks ending return statement… · `TK2378` A 'get' accessor must return a value. · `TK7030` Not all code paths return a value.
- `TK2349` This expression is not callable. · `TK2348` …Did you mean to include 'new'? · `TK2351` This expression is not constructable.

**generics**
- `TK2344` Type '{0}' does not satisfy the constraint '{1}'.
- `TK2558` Expected {0} type arguments, but got {1}.

**implicit any (`noImplicitAny`)**
- `TK7006` parameter · `TK7005` variable · `TK7008` member · `TK7031` binding element implicitly has an '{1}' type.
- `TK7053` Element implicitly has an 'any' type because expression of type '{0}' can't be used to index type '{1}'.

### Tier B — broader semantic surface

Same machinery, lower centrality / higher cost. Several of these are gated on the
type-level evaluation phase (mapped/conditional/template-literal types, `keyof`,
indexed access — see [ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)).

- Advanced type-level: `TK2536` Type '{0}' cannot be used to index type '{1}'., index-signature compat (`TK2411`), `keyof`/conditional/mapped/template-literal evaluation.
- `this` typing: `TK2683` 'this' implicitly has type 'any'…
- Iterables: `TK2488` Type must have a '[Symbol.iterator]()' method…
- Accessors: `TK2379`/`TK2380` get/set compatibility (incl. `exactOptionalPropertyTypes`).
- Enums, in-file namespaces, decorators, computed/symbol property names.
- Reachability lint: `TK7027` Unreachable code detected. · `TK2790` The operand of a 'delete' operator must be optional.

## Out of scope by design

These never get a `TK` code — they fall outside the type model, not merely "later".

**Whole ranges**
- **`1xxx` parse & grammar (398).** oxc is the parser; typokat consumes its AST.
  Syntactic and grammar diagnostics (`TK1005` `'{0}' expected.`, `TK1109`, `TK1128`,
  misplaced-modifier checks) are oxc's job and are not re-implemented under `TK`.
- **`4xxx` declaration emit (109).** No emit ⇒ no `.d.ts` privacy/portability errors
  (`TK4025` Exported variable has or is using private name…).
- **`5xxx` compiler options (65).** No tsconfig/flag surface (`TK5023` Unknown compiler
  option…, `TK5055`).
- **`6xxx` CLI / driver messages (50).** File-not-found, extensions, CLI plumbing
  (`TK6053`, `TK6054`) — not type errors.
- **`8xxx` TS-in-JS (35).** typokat checks `.ts`. `checkJs`, JSDoc inference, and
  "X can only be used in TypeScript files" (`TK8009`, `TK8010`) do not apply.
- **`18xxx` (67), mostly.** jsx-runtime, import-attributes, and target-version gating
  (`TK18045` `accessor` needs ES2015+) are emit/resolution concerns. A few strays are
  strict-null aliases (`TK18047` '{0}' is possibly 'null'.) and share their `2xxx`
  sibling's semantics if reached.

**`2xxx` codes that look in-scope but are not** — because they are really module
resolution or emit, not the type model:
- Module resolution: `TK2307` Cannot find module…, `TK2792` (moduleResolution hint),
  `TK2305` Module has no exported member…, `TK2459` declares locally but not exported.
- `isolatedModules` / emit-target-gated `2xxx` diagnostics.

## Why this is sound to bound this way

The out-of-scope categories are *orthogonal* to type correctness: a file can be
fully type-checked without resolving its imports, emitting `.d.ts`, or validating a
tsconfig. Where a missing capability *would* otherwise change a verdict (e.g. an
unresolved import feeding an expression), the **soundness > completeness** invariant
applies — typokat over-reports rather than silently passing. See
[`invariants.md`](invariants.md) and the divergence ledger in
[`divergences.md`](divergences.md).

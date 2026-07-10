# Diagnostic scope map

Which `tsc` diagnostics typokat is *meant* to cover, and which whole categories
are out of scope **by design**. This is a description of the checker's shape — the
boundary of the type model — not a schedule. The authoritative **live** coverage
(the codes actually emitted today) is the `DiagnosticCode` enum in
[`src/diagnostics/mod.rs`](../../src/diagnostics/mod.rs) and the coverage table in
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

The `2xxx` + `7xxx` ranges are the **candidate pool**, minus the module/emit
exceptions below. The marked Tier S/A/B families are the canonical 1.0 inventory;
an unlisted code is not implicitly promised. Backlog `75` owns the remaining
candidate-range census and either promotes each family here or records a sound OOS
disposition.

## In-scope tiers

Tiered by **centrality to the type model**, not by schedule. Tier S is the reason
the project exists; Tier B is the long tail that the same machinery eventually
reaches. The exact `tsc` message text is shown so the wording is pinned.

### Tier S — structural core

The structural assignment-and-member engine. This *is* typokat; most of it is
already emitted.

**Assignability & calls**
<!-- scope-family: s-assignability -->
- `TK2322` Type '{0}' is not assignable to type '{1}'.
<!-- scope-family: s-call-arguments -->
- `TK2345` Argument of type '{0}' is not assignable to parameter of type '{1}'.
<!-- scope-family: s-call-arity -->
- `TK2554` Expected {0} arguments, but got {1}. · `TK2555` Expected at least {0} arguments, but got {1}.
<!-- scope-family: s-overload-layout -->
- `TK2391` Function implementation is missing or not immediately following the declaration.
<!-- scope-family: s-overload-implementation -->
- `TK2394` Overload signature is not compatible with its implementation signature.
<!-- scope-family: s-overload-resolution -->
- `TK2769` No overload matches this call.
<!-- scope-family: s-missing-property -->
- `TK2741` Property '{0}' is missing in type '{1}' but required in type '{2}'.
<!-- scope-family: s-multiple-missing-properties -->
- `TK2739`/`TK2740` report multiple missing properties with tsc-compatible code selection.
<!-- scope-family: s-excess-property -->
- `TK2353` Object literal may only specify known properties… (excess property).
<!-- scope-family: s-weak-type -->
- `TK2559` Type '{0}' has no properties in common with type '{1}'.

**Names & member access**
<!-- scope-family: s-name-resolution -->
- `TK2304` Cannot find name '{0}'.
<!-- scope-family: s-member-access -->
- `TK2339` Property '{0}' does not exist on type '{1}'.
<!-- scope-family: s-member-suggestion -->
- `TK2551` …Did you mean '{2}'?
<!-- scope-family: s-value-type-space -->
- `TK2693` '{0}' only refers to a type, but is being used as a value here (value/type symbol spaces).
<!-- scope-family: s-declaration-hoisting -->
- Function declarations and `var` use TypeScript's hoisting/visibility rules so declaration order cannot hide checks.

**Classes (nominal + structural OO)**
<!-- scope-family: s-class-access -->
- `TK2341` private · `TK2445` protected · `TK2540` assign to read-only · `TK2511` instantiate abstract.
<!-- scope-family: s-abstract-completeness -->
- `TK2515` Non-abstract class does not implement inherited abstract member.
<!-- scope-family: s-class-override -->
- `TK2416` Property in derived type is not assignable to the same property in base (override compat).
<!-- scope-family: s-static-implements -->
- `TK2417` Class static side incorrectly extends · `TK2420` Class incorrectly implements interface.
<!-- scope-family: s-duplicate-declarations -->
- `TK2451` Cannot redeclare block-scoped variable '{0}'.

### Tier A — strict type model & flow

The strict-mode and flow-sensitive layer: nullability, definite assignment,
operators, returns, generic constraints, implicit `any`.

**strict null / flow**
<!-- scope-family: a-nullish-receivers -->
- `TK2531` Object is possibly 'null'. · `TK2532` …'undefined'. · `TK2533` …'null' or 'undefined'.
<!-- scope-family: a-unknown-receivers -->
- `TK2571` Object is of type 'unknown'.
<!-- scope-family: a-definite-assignment -->
- `TK2454` Variable used before being assigned. · `TK2448` used before its declaration. · `TK2564` property has no initializer and is not definitely assigned.

**operators & narrowing**
<!-- scope-family: a-comparison-overlap -->
- `TK2367` This comparison appears to be unintentional… have no overlap.
<!-- scope-family: a-operator-operands -->
- `TK2365` Operator '{0}' cannot be applied to types '{1}' and '{2}'. · `TK2356`/`TK2362`/`TK2363` arithmetic operand must be number/bigint/enum.
<!-- scope-family: a-type-predicates -->
- User-defined type predicates and assertion functions participate in the one flow model.
<!-- scope-family: a-narrowing-tail -->
- Remaining loop, member-path, closure, and `instanceof` narrowing forms preserve strict verdicts.

**functions & returns**
<!-- scope-family: a-missing-return -->
- `TK2355` A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.
<!-- scope-family: a-return-paths -->
- `TK2366` Function lacks ending return statement… · `TK2378` A 'get' accessor must return a value. · `TK7030` Not all code paths return a value.
<!-- scope-family: a-noncallable -->
- `TK2349` This expression is not callable.
<!-- scope-family: a-call-construct-parity -->
- `TK2348` …Did you mean to include 'new'? · `TK2351` This expression is not constructable.

**generics**
<!-- scope-family: a-generic-constraints -->
- `TK2344` Type '{0}' does not satisfy the constraint '{1}'.
<!-- scope-family: a-type-argument-arity -->
- `TK2558` Expected {0} type arguments, but got {1}.

**implicit any (`noImplicitAny`)**
<!-- scope-family: a-implicit-any-declarations -->
- `TK7006` parameter · `TK7005` variable · `TK7008` member · `TK7031` binding element implicitly has an '{1}' type.
<!-- scope-family: a-implicit-any-index -->
- `TK7053` Element implicitly has an 'any' type because expression of type '{0}' can't be used to index type '{1}'.

### Tier B — broader semantic surface

Same machinery, lower centrality / higher cost. Several of these are gated on the
type-level evaluation phase (mapped/conditional/template-literal types, `keyof`,
indexed access — see [ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)).

<!-- scope-family: b-indexed-access-diagnostics -->
- Indexed access and index-signature compatibility: `TK2536` Type '{0}' cannot be used to index type '{1}'. · `TK2411` property incompatible with index signature.
<!-- scope-family: b-type-level-tail -->
- Remaining `keyof`/conditional/mapped/template-literal evaluation, type-parameter defaults, optional tuples, and generic/deferred `T[K]` shapes.
<!-- scope-family: b-implicit-this -->
- `this` typing: `TK2683` 'this' implicitly has type 'any'…
<!-- scope-family: b-this-parameters -->
- Explicit `this` parameters and `ThisType<T>` preserve receiver/contextual types.
<!-- scope-family: b-iterability -->
- Iterables: `TK2488` Type must have a '[Symbol.iterator]()' method…
<!-- scope-family: b-accessor-compatibility -->
- Accessors: `TK2379`/`TK2380` get/set compatibility (incl. `exactOptionalPropertyTypes`).
<!-- scope-family: b-enums -->
- Enums.
<!-- scope-family: b-namespaces -->
- In-file namespaces and declaration merging.
<!-- scope-family: b-decorators-computed-members -->
- Decorators and computed/symbol property names.
<!-- scope-family: b-reachability -->
- `TK7027` Unreachable code detected.
<!-- scope-family: b-delete-operand -->
- `TK2790` The operand of a 'delete' operator must be optional.
<!-- scope-family: b-semantic-candidate-tail -->
- Remaining semantic candidates from the `2xxx`/`7xxx` pool receive an explicit implemented or OOS disposition before 1.0.

## Out of scope by design

The whole-range entries below never get a `TK` code. The final module-resolution subsection
separates today's supported slice, planned resolver capability, and diagnostics that remain OOS.

**Whole ranges**
- **`1xxx` parse & grammar (398).** oxc is the parser; typokat consumes its AST.
  Syntactic and grammar diagnostics (`TK1005` `'{0}' expected.`, `TK1109`, `TK1128`,
  misplaced-modifier checks) are oxc's job and are not re-implemented under `TK`.
- **`4xxx` declaration emit (109).** No emit ⇒ no `.d.ts` privacy/portability errors
  (`TK4025` Exported variable has or is using private name…).
- **`5xxx` compiler options (65).** Project discovery/resolution may consume a narrow,
  documented set of tsconfig fields (backlogs `72`/`15`), but option-validation
  diagnostics such as `TK5023`/`TK5055` remain outside the type model.
- **`6xxx` CLI / driver messages (50).** File-not-found, extensions, CLI plumbing
  (`TK6053`, `TK6054`) — not type errors.
- **`8xxx` TS-in-JS (35).** typokat checks `.ts`. `checkJs`, JSDoc inference, and
  "X can only be used in TypeScript files" (`TK8009`, `TK8010`) do not apply.
- **`18xxx` (67), mostly.** jsx-runtime, import-attributes, and target-version gating
  (`TK18045` `accessor` needs ES2015+) are emit/resolution concerns. A few strays are
  strict-null aliases (`TK18047` '{0}' is possibly 'null'.) and share their `2xxx`
  sibling's semantics if reached.

**`2xxx` module-resolution codes — partly in scope since M29.** The **local-relative `.ts`
slice** (M29, backlog `15` slice 1) resolves imports, so it **emits** `TK2307` *Cannot find
module…* and `TK2305` *Module has no exported member…* for that slice — both are live codes in
`src/diagnostics/mod.rs` and the README diagnostics list. What stays **out of scope** is
**currently unsupported but planned** is package/`node_modules`/tsconfig project resolution
(preview slice `72`, full breadth `15`). Resolver diagnostics for supported forms are in scope as
those slices land; `TK2792`, `TK2459`, unknown-option validation, and
`isolatedModules`/emit-target-gated diagnostics stay OOS unless deliberately promoted. The
divergence ledger's Modules section is the authoritative current boundary.

## Why this is sound to bound this way

The out-of-scope categories are *orthogonal* to type correctness: a file can be
fully type-checked without resolving its imports, emitting `.d.ts`, or validating a
tsconfig. Where a missing capability *would* otherwise change a verdict (e.g. an
unresolved import feeding an expression), the **soundness > completeness** invariant
applies — typokat over-reports rather than silently passing. See
[`invariants.md`](invariants.md) and the divergence ledger in
[`divergences.md`](divergences.md).

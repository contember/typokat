# Deliberate `tsc` divergences & deferred checks

The canonical ledger of every place typokat's output deliberately differs from
`tsc --noEmit --strict`. Two kinds of entry:

- **Over-report (divergence)** — typokat reports where tsc is clean, or reports the
  same verdict with a different message. Always the *safe* direction under
  **soundness > completeness** ([`invariants.md`](invariants.md)).
- **Deferred check** — a real tsc error typokat does **not** yet emit. A bounded,
  documented false-negative: the conformance fixture that hits it carries an
  explanatory comment and expects **0** errors, so the gap is recorded, not
  accidental. Each slots into a known later milestone / backlog item without rework.

**Where this fits.** [`README.md`](../../README.md) "Known limitations" is the
user-facing summary and [`scope.md`](scope.md) is the code-range boundary — both link
here for the detail. The conformance corpus ([`tests/cases/`](../../tests/cases/README.md))
pins every entry; the *how it's implemented* lives in
[`architecture.md`](architecture.md). When a backlog item ships a fix, the matching
entry here is deleted.

## Inline metadata (the machine layer)

This file is **both** the human ledger and the machine-checked divergence census
(manifest criterion `C-deferred-divergence-census`, backlog `75`). Every divergence
entry carries a one-line HTML-comment marker, validated by
[`tests/divergences.rs`](../../tests/divergences.rs). Grammar:

```
<!-- div: id=<stable-id> dir=<under|over|cosmetic> scope=<family|design-oos> owner=<owner> witness=<path> -->
```

- **`id`** — a stable `area/topic/detail` slug (`[a-z0-9-]` segments, `/`-separated),
  unique across the file, independent of the prose display text.
- **`dir`** — the report direction vs `tsc`: `under` (we drop a real error — a
  false-negative), `over` (we report where tsc is clean — the safe direction), or
  `cosmetic` (same verdict, different message/code/span).
- **`scope`** — the [`scope.md`](scope.md) Tier S/A/B family this touches, or
  `design-oos` when it is out of the type model by design.
- **`owner`** — a live `../backlog/NN-*.md` item that will resolve it, or `design-oos`
  when no fix is planned. **An `under` entry MUST name a live backlog owner** — an
  unowned false-negative is exactly the silent-FN the census exists to forbid.
- **`witness`** — a corpus/tooling path that pins the entry (a `tests/cases/**`
  fixture or directory, the disabled `sr_deferred_ledger` corpus, or the
  official-suite scoreboard).

The validator checks structure, links, and ownership — it does **not** run the
checker, so `dir` honesty rests on adversarial review (cross-check disputed entries
against `tsc --strict`), not on a green `cargo test`.

The validator rejects a divergence row missing its marker, a duplicate `id`, a bad
`dir`/`scope` enum, a dead/missing owner or witness, and any `under` without a live
owner. It flags an unmarked row whenever a list item under a divergence section
carries a divergence sentinel (`over-report`, `under-report`, `deferred`, `skipped`,
`out of scope`, `cosmetic`, …) but no marker; non-sentinel entries are marked by hand
and validated the same way.

## Cross-cutting conventions

- **Error type suppresses cascade.** An unresolved name (`TK2304`) gets the **error
  type** (`any`-like), which suppresses follow-on diagnostics on the same expression.
  Since **M22** this applies in **type position** too: an unresolved simple-identifier
  type reference (in any annotation — variable / parameter / return / interface or
  object-type member / type-alias body / union / array / tuple / generic name or
  argument / `keyof` operand) reports `TK2304` and degrades to the error type, so
  `const a: Foo = 5` is only `TK2304`, never also a `TK2322`. Top-level type
  declarations are **hoisted**, so a forward reference resolves (no false `TK2304`).
- **Deferred (silent) `TK2304` sub-cases.** `TK2304` usually fires only when the name
  resolves to *no* space. Not reported (documented divergences): a value used as a type
  (tsc `TS2749` — the name resolves in the value space); type arguments applied to a
  type parameter (tsc `TS2315`); a wrong **type-argument count** on a recognized type
  such as bare/over-applied `Array` (tsc `TS2314` — `Array` is a known built-in, not
  "cannot find name"); **qualified** type names (`A.B` — needs namespaces). M29
  temporarily maps a type-only import/export used as a value to `TK2304` instead of
  tsc's `TS2693`.
  <!-- div: id=names/value-used-as-type dir=under scope=s-value-type-space owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/m22_unresolved_type/positions.ts -->
  <!-- div: id=names/type-args-on-type-param dir=under scope=a-type-argument-arity owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/m24_generic_constraints/constraint_check_explicit.ts -->
  <!-- div: id=names/type-arg-count-on-builtin dir=under scope=a-type-argument-arity owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/m22_unresolved_type/generics.ts -->
  <!-- div: id=names/qualified-type-name dir=under scope=b-namespaces owner=../backlog/43-namespaces-declaration-merging.md witness=../../tests/cases/m22_unresolved_type/positions.ts -->
  <!-- div: id=names/type-only-import-as-value-code dir=cosmetic scope=s-value-type-space owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/sr_wu2_export_space/type_only_export_leak/a.ts -->
- **Multiple mismatched arguments (over-report).** On a call/`new` with several
  mismatched arguments, typokat reports a `TK2345` for **each**, whereas tsc stops at
  the first. Fixtures keep at most one mismatched argument per call so the corpus
  matches both.
  <!-- div: id=calls/multiple-mismatched-arguments dir=over scope=s-call-arguments owner=design-oos witness=../../tests/cases/m3_functions -->
- **Unannotated forward `var` value types are unresolved (under-report).** The
  declaration-hoisting sprint shipped name hoisting and explicit-annotation reservation, but a later
  unannotated initializer still does not type an earlier read/write. Moving value
  inference ahead of source-order initializer checking requires the exact lazy
  declaration/value queries owned by backlog `76`.
  <!-- div: id=hoisting/unannotated-forward-var-value dir=under scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/sr_deferred_ledger/b76_unannotated_forward_var.ts -->
- **Unannotated forward function returns are conservative (over-report).** The shipped
  declaration-hoisting path publishes the callable parameter/generic surface before
  the declaration, but uses
  `unknown` for a body-inferred return until source-position body checking replaces it.
  This preserves argument diagnostics and prevents a permissive false clean, but a valid
  result-consuming forward call can report `TK2322`. Exact demand-driven declaration
  types and TS7022/TS7023 cycles belong to backlog `76`.
  <!-- div: id=hoisting/unannotated-forward-return dir=over scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b74_declaration_hoisting/unannotated_forward_returns.ts -->
- **`undefined` in assignment-target position (cosmetic).** typokat resolves
  `undefined` as a value read but not as an assignment target, so `undefined = null`
  reports `TK2304` where tsc reports `TS2539` — same verdict, different code.
  Surfaced by WU1's nested-assignment checking; owner backlog `47`.
  <!-- div: id=names/undefined-assignment-target dir=cosmetic scope=s-name-resolution owner=../backlog/47-definite-assignment.md witness=../../tests/cases/sr_wu1_expressions/nested_assignments.ts -->
- **Assignment into an explicit `any` narrows it (over-report).** `let s: any;
  s = 5; s.foo` reports `TK2339` where tsc keeps `s` as `any` (assignment narrowing
  applies only to implicit evolving `any`). Safe direction; batched in backlog `63`.
  <!-- div: id=narrowing/explicit-any-narrowed dir=over scope=a-narrowing-tail owner=../backlog/63-review-parity-tail.md witness=../../tooling/official-suite/scoreboard.txt -->
- **`strictNullChecks` is on** (our default): `null`/`undefined` are distinct types,
  not assignable to others.

## Deferred checks — flow / binder (not yet emitted)

- `TK2355` *function must return a value* — needs control-flow reachability (with narrowing).
  <!-- div: id=flow/missing-return-value dir=under scope=a-missing-return owner=../backlog/46-return-path-analysis.md witness=../../tests/cases/m3_functions -->
- `TK2454` *used before assigned* — needs definite-assignment flow analysis.
  <!-- div: id=flow/used-before-assigned dir=under scope=a-definite-assignment owner=../backlog/47-definite-assignment.md witness=../../tests/cases/m1_binder_inference -->
- `TK2451` *cannot redeclare* — binder check, deferred; fixtures use unique names.
  <!-- div: id=binder/cannot-redeclare dir=under scope=s-duplicate-declarations owner=../backlog/18-duplicate-identifier-detection.md witness=../../tests/cases/m1_binder_inference -->
- Duplicate function implementations are not rejected with `TK2300`/`TK2393`; the
  existing M33 fallback can therefore expose the last implementation as the callable
  type and suppress an independent call diagnostic. This predates declaration hoisting
  and stays release-owned by backlog `18`.
  <!-- div: id=binder/duplicate-function-implementation-call dir=under scope=s-duplicate-declarations owner=../backlog/18-duplicate-identifier-detection.md witness=../../tests/cases/sr_deferred_ledger/b18_duplicate_function_implementations.ts -->

### Soundness-review deferred ledger (backlog `18`/`30`/`60`/`62`/`66`/`76`/`77`)

Known dropped-error (under-report) families from the 2026-07-07 cross-cutting review,
each an open backlog item, pinned by the **disabled** `sr_deferred_ledger/` corpus
(the fixtures assert tsc's verdict but stay `false` until the owning item ships):

- **`60`** — fresh object literals against UNION targets skip excess-property checking
  (`A | B`, `A | null`) and let an optional-member union member vacuously absorb a
  wrong-typed known property (missing `TK2353`/`TK2322`).
  <!-- div: id=ledger/fresh-literal-union-targets dir=under scope=s-excess-property owner=../backlog/60-fresh-literal-union-targets.md witness=../../tests/cases/sr_deferred_ledger/b60_fresh_literal_unions.ts -->
- **`62`** — a declared (interface/class) source is accepted against an index-signature
  target because there is no "source provides an index signature" rule (missing
  `TK2322`); anonymous sources correctly keep their implicit index signature.
  <!-- div: id=ledger/index-signature-source dir=under scope=b-indexed-access-diagnostics owner=../backlog/62-index-signature-relation-parity.md witness=../../tests/cases/sr_deferred_ledger/b62_index_signature.ts -->
- **`30`** (Template literal types), **`66`** (Classes), and **`77`** (Utility types)
  are documented in their own sections below; the corpus adds fixtures for them.

## Narrowing (M7 / M8 / M23)

Implemented: `typeof` / truthiness / `null`/`undefined` equality (M7); discriminated-union
(literal discriminant) / `in`-operator / `switch` narrowing + literal type annotations (M8);
narrowing through **unstructured flow** — early `return`/`throw`, `&&`/`||`/ternary,
assignment-in-flow, and `while` loop edges (back edge, exit edge, `break`/`continue`) — via the
flow-node CFG (M23), the single narrowing model.

- Declaration **initializers** are deliberately NOT narrowed (`let x: string | null = "a"`
  reads as `string | null` — over-report, safe direction); assignment narrowing starts at the
  first real assignment.
  <!-- div: id=narrowing/declaration-initializer dir=over scope=a-narrowing-tail owner=../backlog/51-narrowing-tail.md witness=../../tests/cases/m23_unstructured_narrowing -->
- **Deferred:** assertion functions / type predicates (`x is T`), `for`/`for-of`/`do-while`
  loop forms, and narrowing seen by a **closure** over a never-reassigned binding (tsc narrows;
  typokat keeps the function-boundary reset — over-report, safe direction). Member-path narrowing
  (`x.a`) — narrowing is symbol-keyed. (Backlog `50`/`51`.)
  <!-- div: id=narrowing/deferred-forms dir=over scope=a-narrowing-tail owner=../backlog/51-narrowing-tail.md witness=../../tests/cases/m23_unstructured_narrowing -->
- **Accepted official-suite over-reports** (safe direction, recorded in the scoreboard;
  independently audited — matched never drops, fn never rises): walking `while` bodies / ternary
  arms / logical RHS surfaces lib-shaped `TK2339` (`.length`/`.toString`/… on correctly-narrowed
  primitives — no `lib.d.ts`) in `controlFlowIteration*`, `typeGuardsIn{If,ConditionalExpression}`,
  `typeGuards{Redundancy,OnClassProperty}`, `…RightOperandOf{AndAnd,OrOr}Operator`; plus `TK2345`
  in `controlFlowIterationErrors` from the complex-RHS reset-to-declared rule on a loop back edge
  (tsc narrows `x = fn(x)` to the return type; typokat resets — wider, sound). Since the
  2026-07-10 statement-checking sprint (WU1) the same lib-shaped `TK2339` also surfaces in
  `for`/`do` **loop bodies** and in **assignments embedded in expressions** (comma / nested /
  initializer / `return` operands) — e.g. no-lib `Array.push` in
  `privateNameClassExpressionLoop.ts` and `typeInferenceWithTupleType.ts`, and
  `.length`/`.toString` on narrowed primitives in `controlFlowAssignmentExpression.ts`.
  <!-- div: id=narrowing/lib-shaped-member-access dir=over scope=b-type-level-tail owner=../backlog/14-libdts-loading.md witness=../../tooling/official-suite/scoreboard.txt -->

## Generics & constraints (M9 / M10 / M24)

Implemented: explicit type arguments + instantiation (M9); type-argument **inference** (M10);
**constraints** (`<T extends U>`: `TK2344` on explicit arguments, the constraint as the apparent
type, clamp-to-constraint inference reporting `TK2345`, `TK2313` for a circular constraint chain)
(M24).

- **Deferred / out of scope:** `K extends keyof T` (the generic-`keyof` deferral); full constraint-side
  excess-property checking for fresh literals (tsc `TS2353`) is still deferred, so a violating
  inference candidate that came only from a **fresh object/array literal** argument is exempt from
  the constraint clamp; typed values, primitives, structural values, and call-site contextual
  reshaping of fresh literal arguments clamp/check normally. Type-parameter defaults are deferred.
  <!-- div: id=constraints/generic-keyof dir=over scope=b-type-level-tail owner=../backlog/35-keyof-union-and-key-source-edges.md witness=../../tests/cases/m24_generic_constraints -->
  <!-- div: id=constraints/fresh-literal-excess-exempt dir=under scope=a-generic-constraints owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/m24_generic_constraints -->
  <!-- div: id=constraints/type-parameter-defaults dir=over scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/m24_generic_constraints -->
- **Representation note (deviation from architecture §3.1, not a tsc divergence):** type
  parameters keep a **named unique-id** representation, not de Bruijn indices; the constraint is a
  store-side column keyed by `TypeParamId`, not folded into the interned type's identity.
  [ADR-0002](../decisions/0002-de-bruijn-scoped-to-infer-binders.md) scopes de Bruijn indices to
  `infer` binders within conditional nodes.

## Classes (M11–M16 · b06 · b20)

Implemented: fields/constructor/methods/`this`/`new`/structural instances (M11); inheritance
(M12); access modifiers + `static` (M13); member-assignment + `readonly` (M14); getters/setters +
`abstract` (M15); generic classes (M16); override compatibility (`TK2416`) and abstract-member
completeness (`TK2515`/`TK2654`) (b06); private/protected constructor accessibility (`TK2673`/
`TK2674`) (b20).

- **Nominal typing is one-directional (matches tsc in verdict).** typokat enforces the
  foreign→private direction — a target with a `private`/`protected` member requires the *same*
  declaration, so a structurally-identical *other* type is rejected (`TK2322`), which matches tsc.
  Widening a private-bearing instance to a public structural shape is allowed by **both** typokat
  and tsc (verified vs tsc 6.0.3 across interface / object-type / empty-type / cross-class targets),
  so there is **no verdict divergence**. Message cosmetic only: the foreign→private rejection renders
  both sides structurally (`{ x: number }` not assignable to `{ x: number }`) where tsc explains
  "Types have separate declarations of a private property 'x'".
  <!-- div: id=classes/nominal-private-message dir=cosmetic scope=s-class-access owner=design-oos witness=../../tests/cases/m13_modifiers -->
- **Override compatibility (`TK2416`) is public↔public only.** A private/protected override is
  `TS2415` territory, deferred — this scope also skips a *genuine* tsc `TS2416` on an incompatible
  protected-over-protected override (a declared false negative — **dropped error**, backlog `66`;
  the nominal relation would otherwise also reject a *legal* protected redeclaration, which is why
  it was scoped out). Further deferrals:
  <!-- div: id=classes/override-public-only dir=under scope=s-class-override owner=../backlog/66-protected-override-compat.md witness=../../tests/cases/sr_deferred_ledger/b66_protected_override.ts -->
  - **Unequal raw-arity base-method overrides are skipped** on the bespoke method-bivariance path.
    Signature shape is modeled since M32, but override compatibility still keeps this narrow
    out-of-subset gate to avoid mixing tsc's bivariant method rule with represented rest/optional
    shapes without a dedicated override review. Over a base **field** the strict relation query
    still applies.
    <!-- div: id=classes/override-raw-arity dir=under scope=s-class-override owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/b06_class_completeness/override_kind_mixtures.ts -->
  - **Generic bases are skipped** — an override against a generic base / from within a generic
    class may carry a free type parameter (the generic-base composition deferral), where the
    relation would over-report. Relatedly, `TK2515`/`TK2654` render a generic direct base as its
    bare name (`Box`) where tsc renders the instantiation (`Box<string>`) — cosmetic.
    <!-- div: id=classes/override-generic-base dir=under scope=s-class-override owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/b06_class_completeness/override_incompatible.ts -->
    <!-- div: id=classes/abstract-completeness-generic-name dir=cosmetic scope=s-abstract-completeness owner=design-oos witness=../../tests/cases/b06_class_completeness -->
  - **`TS2425`/`TS2426`** (field↔method / accessor-vs-function kind-mismatch codes) are not
    emitted; typokat still reports the `TK2416` type incompatibility on those lines.
    <!-- div: id=classes/kind-mismatch-codes dir=cosmetic scope=s-class-override owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/b06_class_completeness/override_kind_mixtures.ts -->
  - **`TS2415`** (incorrectly-extends: visibility narrowing, private-member redeclaration) and
    **`TS2417`** (static-side override incompatibility) are deferred; fixtures avoid those shapes.
    <!-- div: id=classes/incorrectly-extends dir=under scope=s-static-implements owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/b06_class_completeness -->
- **Constructor accessibility.** On a class that is both `abstract` and inaccessibly-constructable,
  the accessibility error wins and `TK2511` is suppressed (tsc 6.0.3 behavior). Deferred:
  **`TS2675`** (`class D extends C` where `C`'s constructor is private) — a heritage-clause check,
  out of the direct-`new` scope. Cosmetic: a generic class renders the bare class name
  (`Constructor of class 'Box' is private…`) where tsc renders `'Box<T>'`; `new` through a
  parenthesized callees and non-generic one-step `const` aliases now retain the class-keyed
  checks; generic aliases remain the separately owned boundary below.
  <!-- div: id=classes/ts2675-heritage-private-ctor dir=under scope=s-class-access owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/b20_ctor_accessibility/private_ctor.ts -->
  <!-- div: id=classes/ctor-accessibility-generic-name dir=cosmetic scope=s-class-access owner=design-oos witness=../../tests/cases/b20_ctor_accessibility -->
- **Generic class value aliases remain deferred.** Parentheses around a direct generic
  class now preserve the direct path, but `const Alias = GenericClass; new Alias<T>()`
  still loses generic substitution and abstract/constructor-accessibility facts.
  <!-- div: id=classes/generic-new-alias dir=under scope=s-class-access owner=../backlog/78-generic-class-value-aliases.md witness=../../tests/cases/b78_generic_class_value_aliases/generic_aliases.ts -->

## Arrays & tuples (M17 / M18 / M30)

Implemented: `T[]` / `Array<T>`, array literals, element access, `length`, covariant assignability
(M17); tuples `[A, B]` (positional, indexed access, contextual typing) (M18); readonly
array/tuple syntax for relation, read/indexed access, and conditional-`infer` matching (b64);
contextual typing of fresh object/array/tuple literals against concrete declaration, assignment,
parameter, `new`/`super`, and declared-return targets (M30); tuple rest elements plus
function rest/optional/default signature shape (M32).

- **Deferred:** array METHODS (`push`/`map`/…) and the `ReadonlyArray` interface surface (need
  `lib.d.ts`); optional tuple elements (`[number?]`) remain deferred with the rest of M18's tuple
  gaps.
  <!-- div: id=arrays/array-methods-need-lib dir=over scope=design-oos owner=../backlog/14-libdts-loading.md witness=../../tests/cases/m17_arrays -->
  <!-- div: id=tuples/optional-elements dir=under scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/m18_tuples -->

## Index signatures & keyof (M19 / M20)

Implemented: index signatures (`{ [k: string]: T }`, `{ [i: number]: T }`) (M19); `keyof T` +
indexed-access types (`T[K]`) on concrete object types, plus `keyof (A | B)` for unions of concrete
object types as the common-key set (b34), evaluated eagerly (M20/M28).

- **Deferred:** generic `keyof` (over a type parameter) is a **deferred keyof node** since M28
  (see Utility types); `keyof` over intersections/non-objects stays out of subset; a
  generic/deferred `T[K]` outside a mapped value template remains the error type (silent, out of
  scope).
  <!-- div: id=keyof/generic-keyof dir=over scope=b-type-level-tail owner=../backlog/35-keyof-union-and-key-source-edges.md witness=../../tests/cases/m20_keyof -->
  <!-- div: id=keyof/keyof-intersection-nonobject dir=over scope=b-type-level-tail owner=../backlog/35-keyof-union-and-key-source-edges.md witness=../../tests/cases/m31_intersections -->
  <!-- div: id=keyof/generic-indexed-access dir=under scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/m20_keyof -->
- **Contextual generic callbacks over a multi-key `Events[K]` conservatively reject selected-key
  listeners that tsc accepts (over-report).** This was exposed by contextual callback typing; the
  underlying generic indexed-access model remains the backlog `75` deferral.
  <!-- div: id=keyof/generic-indexed-contextual-callback dir=over scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/sr_deferred_ledger/b75_generic_indexed_access.ts -->

## Mapped types (M26)

Implemented (M26): evaluation over concrete sources (keyof-derived and literal-union key sources),
value transformation, modifier arithmetic (`?`/`+?`/`-?` — `-?` strips `undefined` from the value,
an exactly-`undefined` optional member becomes `never`; `readonly`/`-readonly`), homomorphic
preservation of the source's `?`/`readonly`, distribution of homomorphic maps over union type
arguments (`Ident<A | B>` = `Ident<A> | Ident<B>`), mapped-of-mapped composition, generic-call
instantiation, and `TK2456` for a directly self-referential mapped alias. A mapped type over an
unresolved param stays deferred (M25 model); a non-iterable key source (index signatures,
primitives) also stays DEFERRED — never a permissive `{}`.

- **Documented divergences (over-report):** tsc's homomorphic-identity rule (`T` →
  `{ [K in keyof T]: T[K] }` is assignable in tsc; typokat conservatively rejects — pinned in
  `deferred_generics.ts`); index-signature sources (tsc resolves homomorphically; typokat defers —
  `evaluation_sites.ts`).
  <!-- div: id=mapped/homomorphic-identity dir=over scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m26_mapped_types/deferred_generics.ts -->
  <!-- div: id=mapped/index-signature-source dir=over scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m26_mapped_types/evaluation_sites.ts -->
- **Message divergence:** the secondary `TS2313` tsc adds on a self-referential mapped alias is
  omitted (`TK2456` carries the line).
  <!-- div: id=mapped/ts2313-secondary-omitted dir=cosmetic scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m26_mapped_types -->
- **Out of scope:** `as` key remapping and template-literal keys (backlog `11`).
  <!-- div: id=mapped/as-remapping-template-keys dir=over scope=design-oos owner=design-oos witness=../../tests/cases/m26_mapped_types -->
- **Aliased-`keyof` key source collapses (over-report):** a NON-homomorphic map whose `in`
  clause is an **alias of a `keyof`** (`type Keys = keyof Obj; { [K in Keys]: Obj[K] }`)
  evaluates the key source to `never` — and the alias `Keys` itself then resolves to `never`
  at its other use sites, rejecting valid code tsc accepts (`let k: Keys = "a"` → TK2322
  '"a"' not assignable to 'never'). The inline `keyof` (homomorphic) form is unaffected.
  Found during the 2026-07-10 completeness-accounting sprint (WU5).
  <!-- div: id=mapped/aliased-keyof-key-source dir=over scope=b-type-level-tail owner=../backlog/35-keyof-union-and-key-source-edges.md witness=../../tests/cases/sr_deferred_ledger/b35_aliased_keyof_mapped.ts -->

## Utility types (M28)

Implemented (M28): the standard aliases (Partial, Required, Readonly, Record, Pick, Omit, Exclude,
Extract, NonNullable, ReturnType) are BUILT-INS via a prelude compilation unit (`src/prelude.ts`),
each the ordinary mapped/conditional definition evaluated by the M25–M27 machinery. The same
canonical unit also supplies the deliberately bounded `console` (`log`/`warn`/`error`) and numeric
`Math` ambient values; it does not claim general `lib.d.ts` fidelity. A user redeclaration shadows
the matching prelude slot. `keyof <pending computation>` is a **deferred keyof** node
evaluated on demand (identical-node-only while deferred: rejects e.g. `x: T` against `keyof T`,
matching tsc). Uppercase/Lowercase/Capitalize/Uncapitalize are evaluator intrinsics on string
literals (distributing over unions; Rust char-wise case mapping — agrees with JS for the corpus,
including multi-char expansions like `ß` → `"SS"`).

- **Out of scope:** `Parameters`/`ConstructorParameters`,
  `InstanceType`, `Awaited`, `NoInfer`, and the `intrinsic` keyword outside the four (a
  user `= intrinsic` alias silently degrades to the error type).
  <!-- div: id=utility/unsupported-aliases dir=over scope=design-oos owner=design-oos witness=../../tests/cases/m28_utility_types -->
  <!-- div: id=utility/intrinsic-degradation dir=under scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/m28_utility_types -->
- **Deferred receiver utilities/context:** explicit `this` signature slots,
  `ThisType<T>`, `ThisParameterType<T>`, and `OmitThisParameter<T>` are owned by backlog `70`;
  dropping the receiver currently makes calls and relation silently permissive.
  <!-- div: id=utility/this-receiver-family dir=under scope=b-this-parameters owner=../backlog/70-this-parameter-typing.md witness=../../tests/cases/b70_this_parameter_typing/function_receivers.ts -->
- **Documented divergences:**
  - The prelude `ReturnType` uses a strict/sound `(...args: never[]) => infer R` match, so it handles
    non-nullary and rest functions without introducing the lib's permissive `any[]` constraint.
    Its modeled `(...args: never[]) => unknown` constraint is enforced through the shared alias
    constraint path, so non-callables report `TK2344` without introducing `any`.
  - Conditional `infer R` still fails to extract represented object call signatures: a callable
    object or overload set satisfies the `ReturnType` constraint but degrades to the error type,
    dropping wrong-result assignments (backlog `77`).
    <!-- div: id=utility/returntype-call-signature-infer dir=under scope=b-type-level-tail owner=../backlog/77-returntype-call-signature-infer.md witness=../../tests/cases/b77_returntype_call_signatures/single_call_signature.ts -->
  - A **symbolic** intrinsic application (`Uppercase<S>` over a pattern/`string`/free param)
    relates conservatively — assignable to `string` (and an identical node) only, nothing flows
    INTO it — rejecting values tsc's string-mapping algebra accepts (over-report; witnessed by the
    official suite's `stringMapping*` files).
    <!-- div: id=utility/symbolic-intrinsic-conservative dir=over scope=b-type-level-tail owner=design-oos witness=../../tooling/official-suite/scoreboard.txt -->
  - tsc's `TS2820` did-you-mean variant of 2322 is not produced.
    <!-- div: id=utility/ts2820-not-produced dir=cosmetic scope=s-assignability owner=design-oos witness=../../tests/cases/m28_utility_types -->
  - A constraint check is **skipped** only when the substituted CONSTRAINT still carries a deferred
    keyof (the canonical Omit idiom, `Pick<T, Exclude<keyof T, K>>` with `T` free — that check
    lands at concrete instantiation). Generic-call inference uses the same evaluate-then-gate
    discipline, so `K extends keyof T` rejects bad concrete keys while genuinely free wrappers stay
    deferred.
    <!-- div: id=utility/omit-idiom-constraint-skip dir=under scope=a-generic-constraints owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/m28_utility_types/constraint_arguments.ts -->
  - `TK2344` **argument** checks EVALUATE first, then always run: a decidable composition checks
    precisely (`Pick<P, Exclude<"a" | 1, "a">>` → `1` → `TK2344`, tsc-exact); a still-deferred
    argument checks conservatively — tsc-exact on unprovable shapes
    (`Uppercase<MyExclude<K, "a">>` errors in both), an over-report ONLY on provable shapes
    (`Uppercase<Extract<K, string>>` — tsc's constraint approximation proves the bound and stays
    clean; backlog `37`; `constraint_arguments.ts`).
    <!-- div: id=utility/constraint-approx-provable dir=over scope=a-generic-constraints owner=../backlog/37-constraint-approximation-deferred-args.md witness=../../tests/cases/m28_utility_types/constraint_arguments.ts -->
  - A conditional's top-level evaluable check/extends operands demand-evaluate before the extends
    test, and a `No` against an operand carrying an unevaluable deferred node (keyof / conditional
    / instantiation / mapped) at **any structural depth** — through object members, index-signature
    values, call/construct signatures, function parameters/returns, tuple/array elements, and union
    members; template patterns excluded (the M27 matching model decides them) — **never picks the
    false branch**: the whole conditional stays deferred (over-report; `conditional_positions.ts`).
    <!-- div: id=utility/conditional-deferred-operand dir=over scope=b-type-level-tail owner=../backlog/36-conditional-structural-operand-parity.md witness=../../tests/cases/m28_utility_types/conditional_positions.ts -->
  - Deferred nodes **nested inside composite operands** are NOT pre-evaluated — tsc's own
    resolution of those shapes is mixed (object-wrapped keyof / function-return keyof /
    tuple-wrapped intrinsics evaluate; object-wrapped and array-wrapped intrinsics eager-false), so
    the five tsc-clean lines in `conditional_positions.ts`'s nested block are documented
    sound-direction over-reports — exact parity is backlog `36`.
    <!-- div: id=utility/nested-composite-operand dir=over scope=b-type-level-tail owner=../backlog/36-conditional-structural-operand-parity.md witness=../../tests/cases/m28_utility_types/conditional_positions.ts -->
  - `Pick`/`Omit` now resolve concrete object-union operands through common keys, including named
    keys covered by a string index signature; **`K = never`** still stays DEFERRED (over-report; tsc
    computes the empty result).
    <!-- div: id=utility/pick-omit-never-key dir=over scope=b-type-level-tail owner=../backlog/35-keyof-union-and-key-source-edges.md witness=../../tests/cases/m28_utility_types -->
  - `Record` iterates **literal-union key sets only** — template-literal keys
    (`` Record<`k${string}`, V> ``) stay deferred (over-report; tsc produces the pattern index
    signature).
    <!-- div: id=utility/record-template-keys dir=over scope=b-type-level-tail owner=../backlog/35-keyof-union-and-key-source-edges.md witness=../../tests/cases/m28_utility_types -->
  - `DeepPartial`-style recursion over **primitive leaves** over-reports (no-lib
    `keyof <primitive>` is the M20 gap — the leaf stays a deferred map, rejecting values tsc
    accepts).
    <!-- div: id=utility/deeppartial-primitive-leaf dir=over scope=b-type-level-tail owner=../backlog/35-keyof-union-and-key-source-edges.md witness=../../tests/cases/m28_utility_types -->

## Template literal types (M27)

Implemented (M27): construction (all-literal holes collapse; union holes distribute as a cartesian
product; `boolean` expands; `never` short-circuits; numeric literals stringify), pattern
assignability (anchored segment matching for `${string}`/`${number}` holes; `string` does not match
a pattern; patterns flow into `string` and subsuming patterns), `infer` extraction for holes
separated by non-empty literal anchors (non-greedy on the first anchor), and deferred generics (M25
conservative model — a deferred template IS assignable to `string`).

- **Documented divergences:**
  - ADJACENT infer holes (no literal separator) **poison** the conditional — deferred, conservative
    (tsc resolves them: first hole takes one char).
    <!-- div: id=template/adjacent-infer-holes dir=over scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m27_template_literals -->
  - Numeric literal holes use ECMAScript `Number::toString`. The intrinsic `${number}`
    pattern remains a decimal-only subset: tsc also accepts parseable signed, exponent,
    hexadecimal, and redundant-zero spellings, while typokat rejects them conservatively
    (backlog `63(e)`).
    <!-- div: id=template/number-pattern-parse-only dir=over scope=b-type-level-tail owner=../backlog/63-review-parity-tail.md witness=../../tests/cases/m27_template_literals/pattern_assignability.ts -->
  - A hole that itself needs evaluation (a nested template, a conditional / alias instantiation)
    stays symbolic and relates conservatively — rejects strings tsc accepts (over-report, safe).
    <!-- div: id=template/evaluable-hole-conservative dir=over scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m27_template_literals -->
- `Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize` intrinsics (M28) compose with construction
  (an evaluable hole — an intrinsic application, a conditional, a keyof — is evaluated before the
  collapse).

## Conditional types (M25)

Implemented (M25): resolution through the relation engine; distribution over naked-param unions
(`never` → `never`, `boolean` expands to `true | false`); `infer` extraction (array element, object
property, fixed tuple positions, function param/return; same-name covariant candidates union; an
`infer` name used in the false branch is `TK2304`); deferred conditionals on an open check type
(assignable to itself and to any target BOTH branches satisfy; nothing else assignable into one);
`TK2456` for a directly self-referential alias; `TK2589` on runaway instantiation depth.

- **Documented divergences:**
  - `TK2589`'s span is the annotation that demanded evaluation (tsc points at the recursive
    reference inside the alias body).
    <!-- div: id=conditional/tk2589-span dir=cosmetic scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m25_conditional_types -->
  - Same-name `infer` in multiple contravariant positions resolves to a conservative `never`
    (over-report — rejecting values tsc accepts) where tsc **intersects** the candidates. `&` is in
    the model since M31 but this path is not yet wired to intersect (backlog `68`). Verified vs tsc
    6.0.3; the divergence only shows on *overlapping* candidates (disjoint candidates yield `never`
    in both). (An earlier note claiming this *unions* was corrected in the 2026-07-07 audit.)
    <!-- div: id=conditional/contravariant-infer-never dir=over scope=b-type-level-tail owner=../backlog/68-contravariant-infer-intersection.md witness=../../tests/cases/m25_conditional_types -->
  - `infer X extends C` (TS 4.7) is out of scope.
    <!-- div: id=conditional/infer-extends-constraint dir=over scope=design-oos owner=design-oos witness=../../tests/cases/m25_conditional_types -->
  - Rest-based conditional `infer` is implemented for fixed tuple/function rest patterns, but a
    variadic source tuple such as `Tail<[string, ...number[]]>` is still a safe-direction
    over-report (tracked in backlog `69`).
    <!-- div: id=conditional/variadic-source-tuple dir=over scope=b-type-level-tail owner=../backlog/69-signature-rest-parity-tail.md witness=../../tests/cases/b57_tuple_array_infer -->
  - A deferred conditional whose branches still contain its own `infer` binders is conservatively
    non-assignable (over-report, safe).
    <!-- div: id=conditional/deferred-branch-infer dir=over scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m25_conditional_types -->
  - A nested conditional referencing an OUTER conditional's `infer` binder is **poisoned at
    lowering** — never evaluated, conservatively related (tsc resolves it; over-report pinned in
    `nested_infer.ts` — proper de Bruijn shifting is backlog `26`).
    <!-- div: id=conditional/nested-outer-infer dir=over scope=b-type-level-tail owner=../backlog/26-cross-binder-nested-infer.md witness=../../tests/cases/m25_conditional_types/nested_infer.ts -->
  - A conditional buried inside a **named alias / interface / class body**
    (`type W = { foo: IsString<string> }`) is not yet evaluated — it stays deferred and relates
    conservatively (over-report; backlog `27`).
    <!-- div: id=conditional/buried-in-named-body dir=over scope=b-type-level-tail owner=../backlog/27-template-buried-conditional-evaluation.md witness=../../tests/cases/m25_conditional_types -->
  - An `infer` left unbound in a taken true branch resolves to `unknown` (matches tsc).

## Optional properties (M21)

Implemented (M21): optional properties (`a?: T`) on object types, interfaces, and class instance
fields — lowered as real members (so `keyof`/indexed-access include them and a declared optional no
longer trips excess), an optional may be **absent** in a value (no `TK2741`), reading one yields
`T | undefined`, and assignability treats an optional member's effective type as `T | undefined` (a
required source satisfies an optional target, but an optional source does not satisfy a required
target). Modeled with `exactOptionalPropertyTypes` **off** (the default): an explicit `undefined`
is assignable to an optional member. No new diagnostic code.

- **Deferred:** optional **methods**/accessors (`go?(): T` — calling needs the
  possibly-undefined-invocation check, tsc `TS2722`); the dedicated *object is possibly undefined*
  diagnostics (tsc `TS2532`/`TS18048`/`TS2722`); `exactOptionalPropertyTypes` semantics; and
  **narrowing of an optional through a member-access guard** (`if (x.b !== undefined) …` — needs
  the flow-node CFG, so a guarded optional read still over-reports `T | undefined`; safe direction).
  (Backlog `49`.)
  <!-- div: id=optional/deferred-methods-and-nullish dir=under scope=a-nullish-receivers owner=../backlog/49-possibly-undefined-family.md witness=../../tests/cases/m21_optional -->
- **Message-rendering nuance (verdict unchanged, so not a corpus divergence — optional object-target
  messages are asserted code-only):** where tsc renders a present-but-wrong optional property's
  target as the bare `T` (e.g. `{ b: 5 }` → "not assignable to type 'string'"), typokat relates
  against the effective `T | undefined` and may render that union instead.

## Modules / imports (M29)

Implemented (M29, the first correctness-first cross-file slice): local relative `./` / `../` imports
resolved to provided `.ts` files; named imports, `import type`, exported declarations, and simple
`export { x as y }` lists; one serial type universe.

- **Out of scope (deferred):** packages / `node_modules`, `tsconfig` resolver options, `.d.ts`,
  default imports, namespace imports, star imports/re-exports, re-export-from, CommonJS, ambient
  modules, cyclic module graphs, and parallel cross-file identity. The rest of **`lib.d.ts`
  globals** (`console`, string methods, `Promise`, …) are still out of scope — fixtures avoid the
  standard library otherwise. Since M33 preserves overload annotations instead of skipping them,
  official files such as `assignFromStringInterface2.ts` and `unionTypeCallSignatures2.ts` can
  self-gate as `OOS:unresolved` on missing lib names (`RegExp`, `RegExpMatchArray`, `Date`) that
  were previously hidden behind skipped overloads. Likewise, WU1's `throw`-operand and
  `for`-header checking (2026-07-10) evaluates lib globals (`Error`, `Number`) previously behind
  un-traversed code, so `controlFlowIIFE.ts`, `controlFlowInOperator.ts`,
  `booleanLiteralTypes1/2.ts`, `literalTypes3.ts`, and `numericLiteralTypes1/2.ts` self-gate as
  `OOS:unresolved` on a no-lib `TK2304` — lost measurement coverage, not a dropped error.
  (Backlog `14`, `15`, `38`, `43`, `52`.)
  <!-- div: id=modules/out-of-scope-resolution dir=over scope=design-oos owner=../backlog/15-modules-imports.md witness=../../tests/cases/m29_modules -->

## Intersection types (M31)

Implemented (M31): an interned, canonicalized member-set node — the dual of union. Target
intersection requires the value to satisfy **every** member; a source intersection relates through
its **merged apparent object**. Member access, excess-property checking (against the merged key
set), contextual fresh-literal shaping, and the M24 circular-constraint walk (`T extends T & X` →
`TK2313`) all see the merge.

- **Documented divergences (all safe / over-report):**
  - Disjoint primitives (`string & number`) are **not** reduced to `never` — the per-member relation
    yields the same *verdict* with a different message, so those fixtures assert **code-only**.
    <!-- div: id=intersection/disjoint-primitives-message dir=cosmetic scope=s-assignability owner=design-oos witness=../../tests/cases/m31_intersections -->
  - `&` is **not distributed** over unions (`(A | B) & C`).
    <!-- div: id=intersection/no-union-distribution dir=over scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m31_intersections -->
  - `keyof` / indexed-access **over an intersection** stay out of subset (the M20/M28
    keyof-of-non-object deferral).
    <!-- div: id=intersection/keyof-indexed-access dir=over scope=b-type-level-tail owner=../backlog/35-keyof-union-and-key-source-edges.md witness=../../tests/cases/m31_intersections -->
  - An **index-signature target** of a source intersection is conservatively rejected.
    <!-- div: id=intersection/index-signature-target dir=over scope=b-indexed-access-diagnostics owner=design-oos witness=../../tests/cases/m31_intersections -->
  - A **nested optional** target property contributed by a single member is checked more strictly
    than tsc (tsc is lenient only at the top level).
    <!-- div: id=intersection/nested-optional-strict dir=over scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m31_intersections -->
- **Out of scope:** function / call-signature intersection (overload intersection). M33 overload
  resolution is carried by explicit overload lists, not by synthesizing callable intersections.
  <!-- div: id=intersection/callable-intersection-oos dir=over scope=design-oos owner=design-oos witness=../../tests/cases/m31_intersections -->

## Object / interface signatures (F1 corpora)

Method signatures become function-typed properties; call/construct signatures make values
callable/constructable (F1, backlog `05`). Since M33, ordered overload lists are preserved; B41
adds persistent generic binders for free, class/interface/object method, call, and construct
signatures, including outer substitution, inference, constraints, and binder-aware relation.
Optional method signatures are still deferred (out of the WU1 subset on the sound side, so accessing
a dropped optional method member over-reports
instead of dropping a possibly-undefined call error). `tsc --strict` 6.0.3 reports `TS7010` for a
method signature whose return annotation is omitted; typokat is silent — a dropped error
(under-report, backlog `48`), pinned by the disabled ledger fixture.
<!-- div: id=signatures/optional-method dir=over scope=a-nullish-receivers owner=../backlog/49-possibly-undefined-family.md witness=../../tests/cases/f1_object_interface_methods -->
<!-- div: id=signatures/ts7010-omitted-return dir=under scope=a-implicit-any-declarations owner=../backlog/48-no-implicit-any.md witness=../../tests/cases/sr_deferred_ledger/b48_implicit_any_return.ts -->

- **Accepted official-suite over-reports** (safe direction, recorded in the scoreboard rather than
  dropped errors):
  <!-- div: id=signatures/official-overreports dir=over scope=design-oos owner=../backlog/14-libdts-loading.md witness=../../tooling/official-suite/scoreboard.txt -->
  - `objectTypeWithCallSignatureAppearsToBeFunctionType.ts` /
    `objectTypeWithConstructSignatureAppearsToBeFunctionType.ts` — `TK2339` on
    `.apply`/`.call`/`.bind`: typokat does not model `Function.prototype` members on callable /
    construct-signature objects.
  - `assignFromNumberInterface2.ts` — `TK2322`/`TK2741`: typokat does not model
    primitive-to/from-boxed interface assignability (`number` to/from `Number`).
  - `assignmentCompatWithCallSignatures2.ts` — `TK2322`: typokat does not model
    generic-function-to-specific-signature assignability.
  - M33 overload preservation moves several official overload files from "skipped shape" to
    conservative checking:
    `interfaceWithOverloadedCallAndConstructSignatures.ts`,
    `interfaceWithSpecializedCallAndConstructSignatures.ts`,
    `constructSignaturesWithIdenticalOverloads.ts`,
    `constructSignaturesWithOverloads.ts`, and `methodSignaturesWithOverloads2.ts` keep their
    pre-existing missing `TK2454` definite-assignment baseline errors, but now also report
    safe-direction `TK2554`/`TK2345`/`TK2769`/`TK2322` on represented overload calls or
    assignments.
    <!-- div: id=signatures/m33-overload-conservative dir=over scope=s-overload-resolution owner=design-oos witness=../../tooling/official-suite/scoreboard.txt -->
  - `typesWithSpecializedCallSignatures.ts` and
    `stringLiteralTypesInImplementationSignatures2.ts` — `TK2394`: tsc accepts specialized
    string-literal overload signatures against a broader implementation signature in these
    non-strict files; typokat conservatively checks them through the ordinary overload
    compatibility path and over-reports. The latter still has the pre-existing missing `TK2300`
    duplicate-name diagnostic.
    <!-- div: id=signatures/specialized-overload-overreport dir=over scope=s-overload-resolution owner=design-oos witness=../../tooling/official-suite/scoreboard.txt -->
    <!-- div: id=signatures/missing-tk2300-duplicate dir=under scope=s-duplicate-declarations owner=../backlog/18-duplicate-identifier-detection.md witness=../../tooling/official-suite/scoreboard.txt -->
- Construct signatures: ordinary function values are deliberately out of scope — `tsc --strict`
  6.0.3 does not treat a plain `(x: number) => Box` value as satisfying a construct signature, and
  `new makeBox(1)` reports `TS7009`. typokat does not model JavaScript runtime constructability.
  <!-- div: id=signatures/construct-from-function-value dir=over scope=design-oos owner=design-oos witness=../../tests/cases/f1_object_interface_construct -->

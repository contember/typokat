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
- **Multiple mismatched arguments (over-report).** On a call/`new` with several
  mismatched arguments, typokat reports a `TK2345` for **each**, whereas tsc stops at
  the first. Fixtures keep at most one mismatched argument per call so the corpus
  matches both.
- **`strictNullChecks` is on** (our default): `null`/`undefined` are distinct types,
  not assignable to others.

## Deferred checks — flow / binder (not yet emitted)

- `TK2355` *function must return a value* — needs control-flow reachability (with narrowing).
- `TK2454` *used before assigned* — needs definite-assignment flow analysis.
- `TK2451` *cannot redeclare* — binder check, deferred; fixtures use unique names.

## Narrowing (M7 / M8 / M23)

Implemented: `typeof` / truthiness / `null`/`undefined` equality (M7); discriminated-union
(literal discriminant) / `in`-operator / `switch` narrowing + literal type annotations (M8);
narrowing through **unstructured flow** — early `return`/`throw`, `&&`/`||`/ternary,
assignment-in-flow, and `while` loop edges (back edge, exit edge, `break`/`continue`) — via the
flow-node CFG (M23), the single narrowing model.

- Declaration **initializers** are deliberately NOT narrowed (`let x: string | null = "a"`
  reads as `string | null` — over-report, safe direction); assignment narrowing starts at the
  first real assignment.
- **Deferred:** assertion functions / type predicates (`x is T`), `for`/`for-of`/`do-while`
  loop forms, and narrowing seen by a **closure** over a never-reassigned binding (tsc narrows;
  typokat keeps the function-boundary reset — over-report, safe direction). Member-path narrowing
  (`x.a`) — narrowing is symbol-keyed. (Backlog `50`/`51`.)
- **Accepted official-suite over-reports** (safe direction, recorded in the scoreboard;
  independently audited — matched never drops, fn never rises): walking `while` bodies / ternary
  arms / logical RHS surfaces lib-shaped `TK2339` (`.length`/`.toString`/… on correctly-narrowed
  primitives — no `lib.d.ts`) in `controlFlowIteration*`, `typeGuardsIn{If,ConditionalExpression}`,
  `typeGuards{Redundancy,OnClassProperty}`, `…RightOperandOf{AndAnd,OrOr}Operator`; plus `TK2345`
  in `controlFlowIterationErrors` from the complex-RHS reset-to-declared rule on a loop back edge
  (tsc narrows `x = fn(x)` to the return type; typokat resets — wider, sound).

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
- **Override compatibility (`TK2416`) is public↔public only.** A private/protected override is
  `TS2415` territory, deferred — this scope also skips a *genuine* tsc `TS2416` on an incompatible
  protected-over-protected override (a declared false negative — **dropped error**, backlog `66`;
  the nominal relation would otherwise also reject a *legal* protected redeclaration, which is why
  it was scoped out). Further deferrals:
  - **Unequal raw-arity base-method overrides are skipped** on the bespoke method-bivariance path.
    Signature shape is modeled since M32, but override compatibility still keeps this narrow
    out-of-subset gate to avoid mixing tsc's bivariant method rule with represented rest/optional
    shapes without a dedicated override review. Over a base **field** the strict relation query
    still applies.
  - **Generic bases are skipped** — an override against a generic base / from within a generic
    class may carry a free type parameter (the generic-base composition deferral), where the
    relation would over-report. Relatedly, `TK2515`/`TK2654` render a generic direct base as its
    bare name (`Box`) where tsc renders the instantiation (`Box<string>`) — cosmetic.
  - **`TS2425`/`TS2426`** (field↔method / accessor-vs-function kind-mismatch codes) are not
    emitted; typokat still reports the `TK2416` type incompatibility on those lines.
  - **`TS2415`** (incorrectly-extends: visibility narrowing, private-member redeclaration) and
    **`TS2417`** (static-side override incompatibility) are deferred; fixtures avoid those shapes.
- **Constructor accessibility.** On a class that is both `abstract` and inaccessibly-constructable,
  the accessibility error wins and `TK2511` is suppressed (tsc 6.0.3 behavior). Deferred:
  **`TS2675`** (`class D extends C` where `C`'s constructor is private) — a heritage-clause check,
  out of the direct-`new` scope. Cosmetic: a generic class renders the bare class name
  (`Constructor of class 'Box' is private…`) where tsc renders `'Box<T>'`; `new` through a
  parenthesized callee (`new (Priv)()`) or a `const Alias = Priv` alias misses the class-keyed
  `new` checks (the same pre-existing boundary the `TK2511` abstract check has).

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

## Index signatures & keyof (M19 / M20)

Implemented: index signatures (`{ [k: string]: T }`, `{ [i: number]: T }`) (M19); `keyof T` +
indexed-access types (`T[K]`) on concrete object types, plus `keyof (A | B)` for unions of concrete
object types as the common-key set (b34), evaluated eagerly (M20/M28).

- **Deferred:** generic `keyof` (over a type parameter) is a **deferred keyof node** since M28
  (see Utility types); `keyof` over intersections/non-objects stays out of subset; a
  generic/deferred `T[K]` outside a mapped value template remains the error type (silent, out of
  scope).

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
- **Message divergence:** the secondary `TS2313` tsc adds on a self-referential mapped alias is
  omitted (`TK2456` carries the line).
- **Out of scope:** `as` key remapping and template-literal keys (backlog `11`).

## Utility types (M28)

Implemented (M28): the standard aliases (Partial, Required, Readonly, Record, Pick, Omit, Exclude,
Extract, NonNullable, ReturnType) are BUILT-INS via a prelude compilation unit (`src/prelude.ts`),
each the ordinary mapped/conditional definition evaluated by the M25–M27 machinery; a user
redeclaration shadows the prelude. `keyof <pending computation>` is a **deferred keyof** node
evaluated on demand (identical-node-only while deferred: rejects e.g. `x: T` against `keyof T`,
matching tsc). Uppercase/Lowercase/Capitalize/Uncapitalize are evaluator intrinsics on string
literals (distributing over unions; Rust char-wise case mapping — agrees with JS for the corpus,
including multi-char expansions like `ß` → `"SS"`).

- **Out of scope:** `Parameters`/`ConstructorParameters`,
  `InstanceType`/`ThisType`, `Awaited`, `NoInfer`, and the `intrinsic` keyword outside the four (a
  user `= intrinsic` alias silently degrades to the error type).
- **Documented divergences:**
  - The prelude `ReturnType` uses a strict/sound `(...args: never[]) => infer R` match, so it handles
    non-nullary and rest functions without introducing the lib's permissive `any[]` constraint.
    Its lib constraint is still dropped (`ReturnType<number>` is `never`, not `TS2344` — a
    documented under-report, **dropped error**, backlog `67`).
  - A **symbolic** intrinsic application (`Uppercase<S>` over a pattern/`string`/free param)
    relates conservatively — assignable to `string` (and an identical node) only, nothing flows
    INTO it — rejecting values tsc's string-mapping algebra accepts (over-report; witnessed by the
    official suite's `stringMapping*` files).
  - tsc's `TS2820` did-you-mean variant of 2322 is not produced.
  - A constraint check is **skipped** only when the substituted CONSTRAINT still carries a deferred
    keyof (the canonical Omit idiom, `Pick<T, Exclude<keyof T, K>>` with `T` free — that check
    lands at concrete instantiation). Generic-call inference uses the same evaluate-then-gate
    discipline, so `K extends keyof T` rejects bad concrete keys while genuinely free wrappers stay
    deferred.
  - `TK2344` **argument** checks EVALUATE first, then always run: a decidable composition checks
    precisely (`Pick<P, Exclude<"a" | 1, "a">>` → `1` → `TK2344`, tsc-exact); a still-deferred
    argument checks conservatively — tsc-exact on unprovable shapes
    (`Uppercase<MyExclude<K, "a">>` errors in both), an over-report ONLY on provable shapes
    (`Uppercase<Extract<K, string>>` — tsc's constraint approximation proves the bound and stays
    clean; backlog `37`; `constraint_arguments.ts`).
  - A conditional's top-level evaluable check/extends operands demand-evaluate before the extends
    test, and a `No` against an operand carrying an unevaluable deferred node (keyof / conditional
    / instantiation / mapped) at **any structural depth** — through object members, index-signature
    values, call/construct signatures, function parameters/returns, tuple/array elements, and union
    members; template patterns excluded (the M27 matching model decides them) — **never picks the
    false branch**: the whole conditional stays deferred (over-report; `conditional_positions.ts`).
  - Deferred nodes **nested inside composite operands** are NOT pre-evaluated — tsc's own
    resolution of those shapes is mixed (object-wrapped keyof / function-return keyof /
    tuple-wrapped intrinsics evaluate; object-wrapped and array-wrapped intrinsics eager-false), so
    the five tsc-clean lines in `conditional_positions.ts`'s nested block are documented
    sound-direction over-reports — exact parity is backlog `36`.
  - `Pick`/`Omit` now resolve concrete object-union operands through common keys, including named
    keys covered by a string index signature; **`K = never`** still stays DEFERRED (over-report; tsc
    computes the empty result).
  - `Record` iterates **literal-union key sets only** — template-literal keys
    (`` Record<`k${string}`, V> ``) stay deferred (over-report; tsc produces the pattern index
    signature).
  - `DeepPartial`-style recursion over **primitive leaves** over-reports (no-lib
    `keyof <primitive>` is the M20 gap — the leaf stays a deferred map, rejecting values tsc
    accepts).

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
  - Scientific/large-magnitude numeric stringification is a **known unsound gap — backlog `30`**:
    numeric holes and `${number}` segment validation stringify via Rust's shortest-form Display,
    not JS `String(n)` (`` `${1e21}` `` constructs `"1000000000000000000000"` where tsc's type is
    `"1e+21"`, so typokat accepts a string tsc rejects — an UNDER-report, not conservative).
  - A hole that itself needs evaluation (a nested template, a conditional / alias instantiation)
    stays symbolic and relates conservatively — rejects strings tsc accepts (over-report, safe).
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
  - Same-name `infer` in multiple contravariant positions resolves to a conservative `never`
    (over-report — rejecting values tsc accepts) where tsc **intersects** the candidates. `&` is in
    the model since M31 but this path is not yet wired to intersect (backlog `68`). Verified vs tsc
    6.0.3; the divergence only shows on *overlapping* candidates (disjoint candidates yield `never`
    in both). (An earlier note claiming this *unions* was corrected in the 2026-07-07 audit.)
  - `infer X extends C` (TS 4.7) is out of scope.
  - Rest-based conditional `infer` is implemented for fixed tuple/function rest patterns, but a
    variadic source tuple such as `Tail<[string, ...number[]]>` is still a safe-direction
    over-report (tracked in backlog `69`).
  - A deferred conditional whose branches still contain its own `infer` binders is conservatively
    non-assignable (over-report, safe).
  - A nested conditional referencing an OUTER conditional's `infer` binder is **poisoned at
    lowering** — never evaluated, conservatively related (tsc resolves it; over-report pinned in
    `nested_infer.ts` — proper de Bruijn shifting is backlog `26`).
  - A conditional buried inside a **named alias / interface / class body**
    (`type W = { foo: IsString<string> }`) is not yet evaluated — it stays deferred and relates
    conservatively (over-report; backlog `27`).
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
  standard library otherwise. (Backlog `14`, `15`, `38`, `43`, `52`.)

## Intersection types (M31)

Implemented (M31): an interned, canonicalized member-set node — the dual of union. Target
intersection requires the value to satisfy **every** member; a source intersection relates through
its **merged apparent object**. Member access, excess-property checking (against the merged key
set), contextual fresh-literal shaping, and the M24 circular-constraint walk (`T extends T & X` →
`TK2313`) all see the merge.

- **Documented divergences (all safe / over-report):**
  - Disjoint primitives (`string & number`) are **not** reduced to `never` — the per-member relation
    yields the same *verdict* with a different message, so those fixtures assert **code-only**.
  - `&` is **not distributed** over unions (`(A | B) & C`).
  - `keyof` / indexed-access **over an intersection** stay out of subset (the M20/M28
    keyof-of-non-object deferral).
  - An **index-signature target** of a source intersection is conservatively rejected.
  - A **nested optional** target property contributed by a single member is checked more strictly
    than tsc (tsc is lenient only at the top level).
- **Out of scope:** function / call-signature intersection (overload intersection), backlog `40`.

## Object / interface signatures (F1 corpora)

Method signatures become function-typed properties; single call/construct signatures make values
callable/constructable (F1, backlog `05`). Optional method signatures are deferred (out of the WU1
subset on the sound side, so accessing a dropped optional method member over-reports instead of
dropping a possibly-undefined call error). `tsc --strict` 6.0.3 reports `TS7010` for a method
signature whose return annotation is omitted — not part of the corpus.

- **Accepted official-suite over-reports** (safe direction, recorded in the scoreboard rather than
  dropped errors):
  - `objectTypeWithCallSignatureAppearsToBeFunctionType.ts` /
    `objectTypeWithConstructSignatureAppearsToBeFunctionType.ts` — `TK2339` on
    `.apply`/`.call`/`.bind`: typokat does not model `Function.prototype` members on callable /
    construct-signature objects.
  - `assignFromNumberInterface2.ts` — `TK2322`/`TK2741`: typokat does not model
    primitive-to/from-boxed interface assignability (`number` to/from `Number`).
  - `assignmentCompatWithCallSignatures2.ts` — `TK2322`: typokat does not model
    generic-function-to-specific-signature assignability.
- Construct signatures: ordinary function values are deliberately out of scope — `tsc --strict`
  6.0.3 does not treat a plain `(x: number) => Box` value as satisfying a construct signature, and
  `new makeBox(1)` reports `TS7009`. typokat does not model JavaScript runtime constructability.

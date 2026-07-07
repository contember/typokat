# Conformance fixtures

This is the spec corpus. Per the MVP plan (§6), correctness is the whole game — these files
define, milestone by milestone, exactly what typokat accepts and what it must reject. The
harness (`tests/harness.rs`, built in M0) runs the checker over each `.ts` file and diffs the
produced diagnostics against the inline markers below.

## Marker convention

Expectations live in trailing line-comments on the **same line** where the diagnostic's
**primary span starts**:

```ts
const x: number = "hello"; // error[TK2322]: Type 'string' is not assignable to type 'number'
```

Rules the harness enforces:

- For each source line, the multiset of `error[CODE]` markers must equal the multiset of
  diagnostics whose **primary span starts on that line**.
- A line with **no** marker must produce **zero** diagnostics starting on it.
- A file with **no** markers must check **clean** (0 diagnostics).
- The optional `: <substring>` after the code is a **case-sensitive `contains`** check against
  the **fully rendered** diagnostic text (including any nested reason chain, §6.4). Omit it
  (`// error[TK2322]`) to assert only the code + line — used where the rendered type strings
  aren't stable yet (object/union/alias targets, see "Type display").
- Multiple errors on one line: separate markers with ` | `.

Exact column spans are validated separately by dedicated M0 snapshot tests, not by every
fixture (keeps fixtures robust to author miscounting).

## Error codes (tsc-compatible numbers, `TK` prefix)

| Code | Meaning |
|---|---|
| `TK2304` | Cannot find name (unresolved identifier) |
| `TK2305` | Module has no exported member |
| `TK2307` | Cannot find module |
| `TK2313` | Type parameter has a circular constraint (`<T extends T>`) |
| `TK2322` | Type X is not assignable to type Y (annotation/reassignment/return/property) |
| `TK2339` | Property does not exist on type |
| `TK2341` | Property is private (accessed outside its declaring class) |
| `TK2344` | Type argument does not satisfy the type parameter's constraint |
| `TK2345` | Argument type not assignable to parameter type |
| `TK2445` | Property is protected (accessed outside the class and its subclasses) |
| `TK2456` | Type alias circularly references itself |
| `TK2416` | Property in derived type not assignable to the same property in base type (override compatibility) |
| `TK2511` | Cannot create an instance of an abstract class |
| `TK2515` | Non-abstract class does not implement inherited abstract member (exactly one missing) |
| `TK2654` | Non-abstract class is missing implementations for members (two or more missing, aggregated) |
| `TK2540` | Cannot assign to a read-only property |
| `TK2353` | Object literal may only specify known properties (excess property) |
| `TK2554` | Wrong number of arguments (arity) |
| `TK2589` | Type instantiation is excessively deep and possibly infinite |
| `TK2673` | Constructor of class is private (direct `new` outside the declaring class) |
| `TK2674` | Constructor of class is protected (direct `new` outside the declaring class/subclasses) |
| `TK2741` | Property is missing in type but required |

## Type display format (what the renderer must print)

Stable across runs → safe to assert in messages:

- intrinsics/keywords: `number string boolean null undefined void never unknown any`
- literals: `1`, `"x"`, `true`
- object: `{ a: number; b: string }` (members in **canonical name-sorted** order —
  see "union" below — `; ` separated; object-target messages in the corpus are
  asserted code-only, so the exact order is never matched against a fixed layout)
- function: `(x: number) => string` (param names included, always parenthesized)

**Not** stable → never assert in messages (use code-only markers):

- **union**: members are displayed in canonical order (sorted by `TypeId`, §3.3), which is
  intern-order dependent, *not* source order. So `string | number` may render either way.
- any object/alias type whose layout we may re-render during development.

Practical rule used in this corpus: assert full messages where both sides are
primitive/literal (stable); use code-only markers when a side is an object/union/alias.

## Deferred checks (intentionally NOT errors in the MVP)

These are real tsc errors that the MVP does **not** yet emit. Fixtures that hit them carry an
explanatory comment and expect **0** errors, so the gap is documented, not accidental. Each
slots into a known later phase without rework:

- `TK2355` *function must return a value* — needs control-flow reachability (with narrowing).
- `TK2454` *used before assigned* — needs definite-assignment flow analysis.
- `TK2451` *cannot redeclare* — binder check, deferred; fixtures use unique names.
- **narrowing**: `typeof` / truthiness / `null`/`undefined` equality (**M7**) and
  discriminated-union (literal discriminant) / `in`-operator / `switch` narrowing + literal type
  annotations (**M8**) are implemented, as is narrowing through **unstructured flow** — early
  `return`/`throw`, `&&`/`||`/ternary, assignment-in-flow, and `while` loop edges (back edge,
  exit edge, `break`/`continue`) — via the flow-node CFG (**M23**, `m23_unstructured_narrowing/`),
  the single narrowing model. Declaration **initializers** are deliberately NOT narrowed
  (`let x: string | null = "a"` reads as `string | null` — over-report, safe direction); assignment
  narrowing starts at the first real assignment. Still deferred: assertion functions / type
  predicates (`x is T`), `for`/`for-of`/`do-while` loop forms, and narrowing seen by a **closure**
  over a never-reassigned binding (tsc narrows; typokat keeps the function-boundary reset —
  over-report, safe direction).
  Accepted official-suite over-reports from M23 (safe direction, recorded in the scoreboard —
  independently audited: matched never drops, fn never rises): walking `while` bodies / ternary
  arms / logical RHS surfaces lib-shaped `TK2339` (`.length`/`.toString`/… on correctly-narrowed
  primitives — no `lib.d.ts`) in `controlFlowIteration*`, `typeGuardsIn{If,ConditionalExpression}`,
  `typeGuards{Redundancy,OnClassProperty}`, `…RightOperandOf{AndAnd,OrOr}Operator`; plus `TK2345`
  in `controlFlowIterationErrors` from the complex-RHS reset-to-declared rule on a loop back edge
  (tsc narrows `x = fn(x)` to the return type; typokat resets — wider, sound).
- **generics**: explicit type arguments + instantiation (**M9**), type-argument **inference**
  (**M10**), and **constraints** (`<T extends U>`: `TK2344` on explicit arguments, the constraint
  as the apparent type — governing member reads AND writes, element access, calls/`new`, and
  `T → constraint` assignability, clamp-to-constraint
  inference reporting `TK2345`, and `TK2313` for a circular constraint chain — followed through
  bare type-parameter constraints and the bare-parameter **members of union constraints**
  (`<T extends T | number>` is circular); a circular parameter records no constraint) are
  implemented (**M24**, corpus `m24_generic_constraints/`). Out of that scope (deferred):
  `K extends keyof T` (the generic-`keyof` deferral), contextual typing of object/array literals
  against a generic constraint (tsc `TS2353` / fresh-literal reshaping — so a violating inference
  candidate that came from a **fresh object/array literal** argument is exempt from the clamp; a
  typed value, primitive or structural, clamps normally), type-parameter defaults, and
  intersection types entirely (`T extends T & X` is tsc `TS2313`, but `&` is not in the type
  model — backlog `25`). The constraint is stored as a store-side column keyed by `TypeParamId`,
  not folded into the interned type's identity. Type parameters keep the named representation:
  ADR-0002 scopes de Bruijn indices to `infer` binders within conditional nodes.
- **classes**: fields/constructor/methods/`this`/`new`/structural instances (**M11**),
  inheritance (`extends`, `super`) (**M12**), and access modifiers (`private`/`protected` —
  access control + nominal typing) + `static` members (**M13**), member-assignment checking
  + `readonly` (**M14**), getters/setters + `abstract` classes (**M15**), and generic classes
  (**M16**) are implemented; method-override compatibility (`TK2416`) and
  abstract-member completeness (`TK2515` single / `TK2654` aggregated, per tsc 6.0.3) are
  implemented too (backlog `06`, corpus `b06_class_completeness/` — see its section below).
  Nominal typing currently constrains only the foreign→private direction (a private/protected
  type rejects a structurally-identical *other* type); widening a private-bearing instance to its
  public structural shape is not yet rejected (a one-directional divergence from tsc TS2322).
- **arrays**: `T[]` / `Array<T>`, array literals, element access, `length`, covariant
  assignability (**M17**) and **tuples** `[A, B]` (positional, indexed access, contextual typing)
  (**M18**) are implemented (built-in, no lib). Fresh object/array/tuple literals are
  contextually typed against concrete declaration, assignment, parameter, `new`/`super`, and
  declared-return targets (**M30**). Array METHODS (`push`/`map`/…) and `ReadonlyArray` follow.
- **index signatures** (`{ [k: string]: T }`, `{ [i: number]: T }`) land in **M19**; `keyof` +
  indexed-access types (`T[K]`) on concrete object types land in **M20** (evaluated eagerly).
  Generic `keyof` (over a type parameter) is a **deferred keyof node** since M28 (see the
  utility-types bullet); a generic/deferred `T[K]` outside a mapped value template remains the
  error type (silent, out of scope).
- **mapped types** (`{ [K in keyof T]: … }`): specced in `m26_mapped_types/`, lands with backlog
  `10` (M26). Scope: evaluation over concrete sources (keyof-derived and literal-union key
  sources), value transformation, modifier arithmetic (`?`/`+?`/`-?` — `-?` strips `undefined`
  from the value, an exactly-`undefined` optional member becomes `never`; `readonly`/
  `-readonly`), homomorphic preservation of the source's `?`/`readonly`, **distribution of
  homomorphic maps over union type arguments** (`Ident<A | B>` = `Ident<A> | Ident<B>`),
  mapped-of-mapped composition, generic-call instantiation, and `TK2456` for a directly
  self-referential mapped alias; a mapped type over an unresolved param stays deferred and
  relates conservatively (M25 model), and a non-iterable key source (index signatures,
  primitives) also stays DEFERRED — never a permissive `{}`. Documented divergences: tsc's
  homomorphic-identity rule (`T` → `{ [K in keyof T]: T[K] }` is assignable in tsc; typokat
  conservatively rejects — over-report, pinned in `deferred_generics.ts`); index-signature
  sources (tsc resolves homomorphically; typokat defers — over-report, `evaluation_sites.ts`);
  the secondary `TS2313` tsc adds on a self-referential mapped alias is omitted (`TK2456`
  carries the line); `as` key remapping and template-literal keys are out of scope (`11`).
- **utility types** (`Partial`, …): `m28_utility_types/` (M28, backlog `12`). The standard
  aliases (Partial, Required, Readonly, Record, Pick, Omit, Exclude, Extract, NonNullable,
  ReturnType) are BUILT-INS via a **prelude compilation unit** (`src/prelude.ts`, parsed +
  bound + resolved before every user program) — each is the ordinary mapped/conditional
  definition evaluated by the M25–M27 machinery; a user redeclaration shadows the prelude.
  `keyof <pending computation>` (a free type parameter, a deferred node) is a **deferred
  keyof** node evaluated on demand — previously the permissive error type, now conservative
  (identical-node-only while deferred: rejects e.g. `x: T` against `keyof T`, matching tsc);
  a non-homomorphic map whose value is `T[P]` on the mapped key carries `T` as its
  **modifiers source** (tsc modifiersType), so Pick/Omit preserve value types + `?`/`readonly`.
  Uppercase/Lowercase/Capitalize/Uncapitalize are evaluator intrinsics on string literals
  (distributing over unions; Rust char-wise case mapping — agrees with JS for the corpus,
  including multi-char expansions like `ß` → `"SS"`). Out of scope:
  `Parameters`/`ConstructorParameters` (need rest elements, backlog `24`),
  `InstanceType`/`ThisType`, `Awaited`, `NoInfer`, and the `intrinsic` keyword outside the
  four (a user `= intrinsic` alias silently degrades to the error type). Documented
  divergences: the prelude `ReturnType` matches a **zero-arity** `() => infer R` (rest
  parameters are out of the type model), so `ReturnType<(x: X) => R>` is `never` — over-report
  vs tsc — and its lib constraint is dropped (`ReturnType<number>` is `never`, not TS2344 —
  a documented under-report); a **symbolic** intrinsic application (`Uppercase<S>` over a
  pattern/`string`/free param) relates conservatively — assignable to `string` (and an
  identical node) only, nothing flows INTO it — rejecting values tsc's string-mapping algebra
  accepts (over-report, safe; witnessed by the official suite's `stringMapping*` files);
  tsc's TS2820 did-you-mean variant of 2322 is not produced; a constraint check is
  **skipped** only when the substituted CONSTRAINT still carries a deferred keyof (the
  canonical Omit idiom, `Pick<T, Exclude<keyof T, K>>` with `T` free — that check lands at
  concrete instantiation; conditional/mapped constraints keep their pre-M28 behavior);
  TK2344 **argument** checks EVALUATE first, then always run: a decidable composition
  checks precisely (`Pick<P, Exclude<"a" | 1, "a">>` → `1` → TK2344, tsc-exact), a
  still-deferred argument checks conservatively — tsc-exact on unprovable shapes
  (`Uppercase<MyExclude<K, "a">>` errors in both), an over-report ONLY on provable shapes
  (`Uppercase<Extract<K, string>>` — tsc's constraint approximation proves the bound and
  stays clean; backlog `37`; `constraint_arguments.ts`);
  a conditional's top-level evaluable check/extends operands demand-evaluate before the
  extends test, and a `No` against an operand carrying an unevaluable deferred node
  (keyof / conditional / instantiation / mapped) at **any structural depth** — through
  object members, index-signature values, call/construct signatures, function
  parameters/returns, tuple/array elements, and union members; template patterns excluded
  (the M27 matching model decides them) — **never picks the false branch**: the whole
  conditional stays deferred (over-report; tsc decides via its string-mapping/keyof
  algebra; `conditional_positions.ts`);
  deferred nodes **nested inside composite operands** are NOT pre-evaluated — tsc's own
  resolution of those shapes is mixed (object-wrapped keyof / function-return keyof /
  tuple-wrapped intrinsics evaluate; object-wrapped and array-wrapped intrinsics
  eager-false), so the five tsc-clean lines in `conditional_positions.ts`'s nested block
  are documented sound-direction over-reports — exact parity is backlog `36`;
  `Pick`/`Omit` stay DEFERRED over **union** operands and for **`K = never`** (a
  keyof-of-union / `never` key source is not iterable in this subset — over-report; tsc
  computes the common-key/empty results);
  `Record` iterates **literal-union key sets only** — template-literal keys
  (`` Record<`k${string}`, V> ``) stay deferred (over-report; tsc produces the pattern
  index signature);
  `DeepPartial`-style recursion over **primitive leaves** over-reports (no-lib
  `keyof <primitive>` is the existing M20 gap — the leaf stays a deferred map, rejecting
  values tsc accepts).
- **template literal types** (`` `a${T}` ``): specced in `m27_template_literals/`, lands with
  backlog `11` (M27). Scope: construction (all-literal holes collapse; union holes distribute as
  a cartesian product; `boolean` expands; `never` short-circuits; numeric literals stringify),
  pattern assignability (anchored segment matching for `${string}`/`${number}` holes; `string`
  does not match a pattern; patterns flow into `string` and subsuming patterns), `infer`
  extraction for holes separated by non-empty literal anchors (non-greedy on the first anchor),
  and deferred generics (M25/M26 conservative model — plus: a deferred template IS assignable to
  `string`). Documented divergences: ADJACENT infer holes (no literal separator) poison the
  conditional — deferred, conservative (tsc resolves them: first hole takes one char);
  `Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize` intrinsics land in M28 (see the
  utility-types bullet) and compose with construction (an evaluable hole — an intrinsic
  application, a conditional, a keyof — is evaluated before the collapse);
  scientific/large-magnitude numeric stringification is a **known unsound gap — backlog `30`**
  (numeric holes and `${number}` segment validation stringify via Rust's shortest-form Display,
  not JS `String(n)`: `` `${1e21}` `` constructs `"1000000000000000000000"` where tsc's type is
  `"1e+21"`, so typokat accepts a string tsc rejects — an UNDER-report, not conservative); a
  hole that itself needs evaluation (a nested template, a conditional / alias instantiation)
  stays symbolic and relates conservatively — rejects strings tsc accepts (over-report, safe).
- **conditional types** (`T extends U ? X : Y`): specced in `m25_conditional_types/`, lands with
  backlog `09` (M25). Scope: resolution through the relation engine; distribution over
  naked-param unions (`never` → `never`, `boolean` expands to `true | false`); `infer`
  extraction (array element, object property, fixed tuple positions, function param/return;
  same-name covariant candidates union; an `infer` name used in the false branch is `TK2304`);
  deferred conditionals on an open check type (assignable to itself and to any target BOTH
  branches satisfy; nothing else assignable into one); `TK2456` for a directly self-referential
  alias; `TK2589` on runaway instantiation depth. Documented divergences: `TK2589`'s span is the
  annotation that demanded evaluation (tsc points at the recursive reference inside the alias
  body); same-name `infer` in multiple contravariant positions unions where tsc intersects
  (`&` is not in the model — backlog `25`); `infer X extends C` (TS 4.7) is out of scope; rest
  elements are avoided throughout (backlog `24`); a deferred conditional whose branches still
  contain its own `infer` binders is conservatively non-assignable (over-report, safe); a nested
  conditional referencing an OUTER conditional's `infer` binder is **poisoned at lowering** —
  never evaluated, conservatively related (tsc resolves it; over-report divergence pinned in
  `nested_infer.ts` — proper de Bruijn shifting is backlog `26`); a conditional buried inside a
  **named alias / interface / class body** (`type W = { foo: IsString<string> }`) is not yet
  evaluated — it stays deferred and relates conservatively (over-report, safe; backlog `27`);
  an infer left unbound in a taken true branch resolves to `unknown` (matches tsc).
- **optional properties** (`a?: T`) on object types, interfaces, and class instance fields land in
  **M21**: lowered as real members (so `keyof`/indexed-access include them and a declared optional no
  longer trips excess), an optional may be **absent** in a value (no `TK2741`), reading one yields
  `T | undefined`, and assignability treats an optional member's effective type as `T | undefined`
  (a required source satisfies an optional target, but an optional source does **not** satisfy a
  required target). Modeled with `exactOptionalPropertyTypes` **off** (the default): an explicit
  `undefined` is assignable to an optional member. No new diagnostic code — a "possibly undefined"
  read flows through the existing `TK2322`/`TK2741`/`TK2353`. Still deferred: optional **methods**/
  accessors (`go?(): T` — calling needs the possibly-undefined-invocation check, tsc `TS2722`), the
  dedicated *object is possibly undefined* diagnostics (tsc `TS2532`/`TS18048`/`TS2722`),
  `exactOptionalPropertyTypes` semantics, and **narrowing of an optional through a member-access
  guard** (`if (x.b !== undefined) …` — needs the flow-node CFG, so a guarded optional read still
  over-reports `T | undefined`; safe direction). Optional **tuple** elements (`[number?]`) remain
  deferred with the rest of M18's tuple gaps.
  Deliberate message-rendering nuance (verdict unchanged, so not a corpus divergence — optional
  object-target messages are asserted code-only): where tsc renders a present-but-wrong optional
  property's target as the bare `T` (e.g. `{ b: 5 }` → "not assignable to type 'string'"), typokat
  relates against the effective `T | undefined` and may render that union instead.
- **modules/imports**: M29 implements the first correctness-first cross-file slice: local relative
  `./` / `../` imports resolved to provided `.ts` files; named imports, `import type`, exported
  declarations, and simple `export { x as y }` lists; one serial type universe. It deliberately does
  not implement packages / `node_modules`, `tsconfig` resolver options, `.d.ts`, default imports,
  namespace imports, star imports/re-exports, re-export-from, CommonJS, ambient modules, cyclic
  module graphs, or parallel cross-file identity. The rest of **`lib.d.ts` globals** (`console`,
  string methods, `Promise`, …) are still out of scope — fixtures avoid the standard library
  otherwise.

Other conventions: an unresolved name (`TK2304`) gets the **error type** (`any`-like), which
**suppresses cascade** diagnostics on the same expression. As of **M22** this applies in **type
position** too: an unresolved simple-identifier type reference (in any annotation — variable /
parameter / return / interface or object-type member / type-alias body / union / array / tuple /
generic name or argument / `keyof` operand) reports `TK2304` and degrades to the error type (so
`const a: Foo = 5` is only `TK2304`, never also a `TK2322`). Top-level type declarations are
**hoisted**, so a forward reference resolves (no false `TK2304`). Deferred (silent, documented
divergences — `TK2304` usually fires only when the name resolves to *no* space, plus M29's temporary
mapping of a type-only import/export used as a value to `TK2304` instead of tsc's `TS2693`): a value
used as a type (tsc `TS2749` — the name resolves in the value space), type arguments applied to a
type parameter (tsc `TS2315`), a wrong **type-argument count** on a recognized type such as
bare/over-applied `Array` (tsc `TS2314` — `Array` is a known built-in, not "cannot find name"), and
**qualified** type names (`A.B` — needs namespaces). `strictNullChecks` is
**on** (our default): `null`/`undefined` are distinct types, not assignable to others.

Known divergence (over-report, safe direction): on a call/`new` with **several** mismatched
arguments, typokat reports a `TK2345` for **each** mismatched argument, whereas tsc stops at the
first. Fixtures therefore keep at most one mismatched argument per call so the corpus matches both.

## Milestone index

| Dir | Milestone |
|---|---|
| `m0_assign_primitives/` | M0 — literal → primitive/intrinsic assignability (walking skeleton) |
| `m1_binder_inference/` | M1 — name resolution, inference, `const`/`let` widening, reassignment |
| `m2_objects/` | M2 — structural object assignability, member access, excess property |
| `m3_functions/` | M3 — function types, calls (arity + args), return-type checks |
| `m4_unions/` | M4 — union assignability, canonicalization, union member access |
| `m5_named_recursive/` | M5 — `type`/`interface`, mutually recursive types (cycle fixpoint) |
| `m6_reporting/` | M6 — nested reason chains in diagnostics |
| `m7_narrowing/` | M7 — control-flow narrowing (`typeof`, truthiness, `null`/`undefined` equality) |
| `m8_discriminated/` | M8 — literal types, discriminated-union / `in` / `switch` narrowing |
| `m9_generics/` | M9 — generic functions / interfaces / aliases, explicit type args, instantiation |
| `m10_inference/` | M10 — type-argument inference from call arguments |
| `m11_classes/` | M11 — class fields / constructor / methods / `this` / `new`, structural instances |
| `m12_inheritance/` | M12 — class inheritance (`extends`, `super`), inherited members + constructor |
| `m13_modifiers/` | M13 — access modifiers (`private`/`protected`: access control + nominal), `static` |
| `m14_readonly/` | M14 — member-assignment checking + `readonly` properties |
| `m15_accessors/` | M15 — getters/setters + `abstract` classes |
| `m16_generic_classes/` | M16 — generic classes (type params, `new C<T>`, inference, `C<T>` as a type) |
| `m17_arrays/` | M17 — array types (`T[]` / `Array<T>`, element access, `length`, covariance) |
| `m18_tuples/` | M18 — tuple types (`[A, B]`, positional assignability, indexed access) |
| `m19_index_sig/` | M19 — index signatures (`{ [k: string]: T }`, `{ [i: number]: T }`) |
| `m20_keyof/` | M20 — `keyof T` + indexed-access types (`T[K]`) on concrete object types |
| `m21_optional/` | M21 — optional properties (`a?: T`) on objects / interfaces / class instance fields |
| `m22_unresolved_type/` | M22 — `TK2304` for an unresolved type reference in type position |
| `m23_unstructured_narrowing/` | M23 — narrowing through unstructured flow (early exit, `&&`/`||`/ternary, assignment, loop edges) |
| `m24_generic_constraints/` | M24 — generic constraints (`TK2344`, apparent type, clamp-to-constraint inference, `TK2313` circularity) |
| `m25_conditional_types/` | M25 — conditional types (resolution, distribution, `infer`, deferred conditionals, `TK2456`/`TK2589`) |
| `m26_mapped_types/` | M26 — mapped types (`{ [K in keyof T]: … }`, modifiers, homomorphic preservation, deferred generics) |
| `m27_template_literals/` | M27 — template literal types (construction/distribution, patterns, `infer` extraction, deferred generics) |
| `m28_utility_types/` | M28 — built-in utility types (prelude aliases: Partial…Omit/ReturnType; Uppercase-family intrinsics) |
| `m29_modules/` | M29 — local relative modules / named imports + exports across files (project fixture subdirectories) |
| `m30_contextual_literals/` | M30 — contextual typing of fresh object / array / tuple literals |

## Project fixture convention (M29+)

M29 introduces multi-file project fixtures. A project fixture is a subdirectory
under `tests/cases/m29_modules/`; every `.ts` file in that subdirectory tree is
part of one project, and markers follow the same same-line convention as
single-file fixtures. Diagnostics are matched against the file that produced
them. Project fixtures stay registered `false` until the conformance harness grows
the project-check path.

The first M29 slice is deliberately narrow: local relative `./` / `../` imports
resolved to `.ts` files; named imports and named exports for values, types,
interfaces, and classes; simple `export { x as y }` lists; one serial type
universe. Packages, `tsconfig`, default/namespace/star imports, re-exports from
another module, CommonJS, ambient modules, `.d.ts`, cycles, and parallel
cross-universe identity are out of scope.

## Bug-fix / backlog corpora

Fixes to **already-shipped** milestones get their own dir rather than extending an
enabled milestone dir — so the spec can be committed on its own (per
`docs/reference/dev-method.md` §1) without changing test results, then enabled by
the commit that lands the fix. Each is registered in `MILESTONE_DIRS`
(`tests/conformance.rs`) as `false` until its fix ships. Named by the official-suite
finding ID (`fN_…`) or the backlog item ID (`bNN_…`).

| Dir | Finding / backlog | Fix |
|---|---|---|
| `f1_object_interface_methods/` | F1 / `05` | object/interface method signatures become function-typed properties |
| `f1_object_interface_call/` | F1 / `05` | single object/interface call signatures make values callable |
| `f1_object_interface_construct/` | F1 / `05` | single object/interface construct signatures make values constructable |
| `f3_class_member_collection/` | F3 / `01` | parameter properties + initializer-inferred class fields become real members |
| `f4_destructuring_access/` | F4 / `02` | private/protected access control runs through object-destructuring patterns |
| `f5_union_readonly/` | F5 / `03` | `readonly` enforced on object-type/interface members and through a union member-assignment |
| `b06_class_completeness/` | backlog `06` | override compatibility (`TK2416`) + abstract-member completeness (`TK2515`/`TK2654`) |
| `b20_ctor_accessibility/` | backlog `20` | private/protected constructor accessibility on direct `new C()` (`TK2673`/`TK2674`) |
| `b28_interface_extends/` | backlog `28` | interface `extends` composition (inherited members in assignability/keyof/mapped; own overrides) |
| `b29_alias_cycles/` | backlog `29` | surface alias cycles → `TK2456` (direct/mutual/through unions); legal member recursion resolves |
| `b30_negative_literals/` | backlog `30` | negative numeric literal types are literals, not `any` (annotations, unions, template holes, extends) |
| `b55_template_memo/` | backlog `55` | template evaluation must not memoize results computed under an exhausted TK2589 budget |
| `b58_project_scopes/` | backlog `58` | project-mode scope maps keyed per module — offset-aligned files must not collide |
| `b61_field_initializers/` | backlog `61` | class field initializers checked against the annotation (assignability, excess, contextual typing; instance + static) |
| `b53_cfg_assignments/` | backlog `53` | assignments survive `&&`/`||`/ternary, `switch` clauses, `while` tests, and sequence expressions in the flow graph |

`b58_project_scopes/` uses **project fixture subdirectories** (the m29 convention): every
`.ts` file in a subdirectory is one project checked via `check_project`. Each project's
two files carry byte-identical first-line headers so the tested constructs (function /
arrow / method / block) start at the **same byte offset** in both files — the collision
being specced keys on span starts, so the headers must stay identical when editing.
Shapes: the wrong-scope descent (spurious `TK2304` + dropped real error) for functions,
arrows, methods, and blocks; a genuine `TK2304` silently absorbed by the other file's
aligned scope; and a fully-correct project that must check clean (at the buggy HEAD it
false-errors). Markers pin the post-fix behavior, which equals each file's standalone
verdicts; cross-checked vs `tsc 6.0.3 --strict`.

`b55_template_memo/` pins the evaluator's memo discipline around budget exhaustion: a
template-literal hole that resolves to the error type only because the shared TK2589
budget was already exhausted must not be committed to the pass-wide memo (the fixed
behavior reports both the TK2589 and the later line's real TK2322). The other node
kinds (conditional / instantiation / mapped / eager keyof) already behave correctly and
are pinned as the regression net, as is the healthy cross-alias template-reuse path.
No new diagnostics; no tsc divergence expected (TK2589's span attribution divergence is
documented in the conditional-types bullet above).

`f1_object_interface_methods/` uses only existing diagnostics (`TK2322`, `TK2339`, `TK2345`).
No deliberate `tsc` divergence is expected for plain non-generic method signatures with explicit
return annotations. `tsc --strict` 6.0.3 reports `TS7010` for a method signature whose return
annotation is omitted, so that case is not part of this corpus.
Optional method signatures are deferred: they are out of the WU1 subset on the sound side, so
accessing a dropped optional method member over-reports instead of dropping a possibly-undefined
call error.

`f1_object_interface_call/` covers a single non-overloaded, non-generic call signature on interfaces
and object type literals, exact arity (`TK2554`), argument mismatch (`TK2345`), call-result
assignment (`TK2322`), function value assignability to a callable object/interface type with no
required named properties (including optional named properties), callable object/interface
assignability back to a matching function type, and missing required named properties (`TK2741`).
Overloads, generic signatures, optional/rest/default parameters, not-callable diagnostics, and
construct signatures remain out of scope.
Accepted official-suite over-reports (safe direction, recorded in the scoreboard rather than
dropped errors):
- `objectTypeWithCallSignatureAppearsToBeFunctionType.ts` — `TK2339` on `.apply`/`.call`/`.bind`:
  typokat does not model `Function.prototype` members on callable objects.
- `assignFromNumberInterface2.ts` — `TK2322`/`TK2741`: typokat does not model primitive-to/from-boxed
  interface assignability (`number` to/from `Number`).
- `assignmentCompatWithCallSignatures2.ts` — `TK2322`: typokat does not model
  generic-function-to-specific-signature assignability.

`f1_object_interface_construct/` covers a single non-overloaded, non-generic construct signature on
interfaces and object type literals, exact arity (`TK2554`), argument mismatch (`TK2345`), construct
result assignment (`TK2322`), class constructor and constructor-typed value assignability to a
construct-signature object/interface type, and construct-signature object/interface assignability
back to a matching `new (...) => T` type. Ordinary function values are deliberately out of scope:
`tsc --strict` 6.0.3 does not treat a plain `(x: number) => Box` value as satisfying a construct
signature, and `new makeBox(1)` reports `TS7009` because the target lacks a construct signature.
Typokat does not model JavaScript runtime constructability, so this corpus avoids fixtures that
depend on ordinary functions being constructable. Overloads, generic construct signatures,
optional/rest/default parameters, `abstract new`, `this` parameters, and constructor runtime
semantics remain out of scope.
Accepted official-suite over-report (safe direction, recorded in the scoreboard rather than a
dropped error):
- `objectTypeWithConstructSignatureAppearsToBeFunctionType.ts` — `TK2339` on
  `.apply`/`.call`/`.bind`: typokat does not model `Function.prototype` members on
  construct-signature objects.

`b06_class_completeness/` covers override compatibility (`TK2416` on the derived member name,
with the tsc message shape + reason chain) for methods, plain properties, and accessor overrides,
and abstract-member completeness on the class name: exactly one missing inherited abstract member
→ `TK2515` (member name unquoted, attributed to the **direct base**, not the declaring class);
two or more → one aggregated `TK2654` (names quoted, direct base's own abstract members first,
then inherited pending ones). A concrete accessor or an initializer-inferred field implements an
abstract member; an incompatible implementation is `TK2416`, not `TK2515`. Cross-checked against
`tsc 6.0.3 --strict`.

Override compatibility (`TK2416`) is checked **public↔public only** (a private/protected override
is `TS2415` territory, deferred — note this scope also skips a *genuine* tsc `TS2416` on an
incompatible protected-over-protected override, a declared false negative, not just an
over-report avoidance; the nominal relation would otherwise also reject a *legal* protected
redeclaration). It follows tsc's method/property variance split, keyed on the **base** (target)
member's declaration kind — probed against tsc 6.0.3, whose verdict depends only on the base
kind, never the derived one's. The kind is recovered from the AST (composed down the `extends`
chain, an inherited member keeping the kind of wherever it was last declared) since typokat
lowers methods and function-typed fields to the same function-typed properties: a base member
declared with **method syntax** (`m() {}`) compares parameters **bivariantly** and the return
type covariantly (with the `void`-return exception) regardless of the derived member's kind,
while a base function-typed **field** (`d: (a) => void`) / initializer / accessor is related by
the plain (strict-contravariant) assignability query — again regardless of the derived kind, so
a method-syntax override of a function-typed field is still strict. Documented divergences
(false-negative — safe only because `TK2416` is a *new* check, so nothing previously reported is
dropped):
- **Unequal-arity base-method overrides are skipped**: typokat models neither optional nor rest
  parameters, so an override of a base **method** that drops or adds a parameter (`m()` over
  `m(x)`, `m(x?)` over `m()`) is out of subset and not checked — tsc usually accepts these, so
  skipping avoids over-reporting, at the cost of missing a genuinely-incompatible arity-changing
  override. (Over a base **field** the strict query's arity rule still applies, so a derived
  member adding required parameters errors.)
- **Generic bases are skipped**: an override against a generic base / from within a generic class
  may carry a free type parameter (the existing generic-base composition deferral), where the
  relation would over-report — the check is skipped there. Relatedly, `TK2515`/`TK2654` render a
  generic direct base as its bare name (`Box`) where tsc renders the instantiation
  (`Box<string>`) — cosmetic, within the same generic-base deferral.
- **`TS2425`/`TS2426`** (field↔method / accessor-vs-function kind-mismatch codes) are not
  emitted; typokat still reports the `TK2416` type incompatibility on those lines per the
  base-kind rule.
- **`TS2415`** (incorrectly-extends: visibility narrowing, private-member redeclaration) and
  **`TS2417`** (static-side override incompatibility) are deferred; fixtures avoid those shapes.

`b20_ctor_accessibility/` covers direct `new C()` against a `private`/`protected` constructor
(`TK2673`/`TK2674` on the whole `new` expression, tsc message shape). `private` permits
construction only lexically inside the declaring class (instance methods and statics);
`protected` also inside subclasses — including constructing the *base* directly from a subclass.
A derived class with no own constructor inherits the base constructor's visibility and the
diagnostic names the **declaring** class; a derived class with its own public constructor is
freely constructable. Where construction is permitted, arity/argument checks run unchanged. On a
class that is both `abstract` and inaccessibly-constructable, the accessibility error wins and
`TK2511` is suppressed (tsc 6.0.3 behavior, probed). Deferred (documented false negative):
**`TS2675`** (`class D extends C` where `C`'s constructor is private) — a heritage-clause check,
out of this corpus's direct-`new` scope; fixtures avoid that shape.
Review-noted nuances (safe, cosmetic or pre-existing): for a generic class the message renders
the bare class name (`Constructor of class 'Box' is private…`) where tsc renders `'Box<T>'`;
`new` through a parenthesized callee (`new (Priv)()`) or a `const Alias = Priv` alias misses the
class-keyed `new` checks — the same pre-existing boundary the shipped `TK2511` abstract check
has (the class-value path is keyed on a direct identifier callee).

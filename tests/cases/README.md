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
| `TK2322` | Type X is not assignable to type Y (annotation/reassignment/return/property) |
| `TK2339` | Property does not exist on type |
| `TK2341` | Property is private (accessed outside its declaring class) |
| `TK2345` | Argument type not assignable to parameter type |
| `TK2445` | Property is protected (accessed outside the class and its subclasses) |
| `TK2511` | Cannot create an instance of an abstract class |
| `TK2540` | Cannot assign to a read-only property |
| `TK2353` | Object literal may only specify known properties (excess property) |
| `TK2554` | Wrong number of arguments (arity) |
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
  annotations (**M8**) are implemented. Still deferred: assertion functions / type predicates
  (`x is T`) and narrowing through unstructured flow (early `return`/`throw`, loops — needs the
  flow-node CFG).
- **generics**: explicit type arguments + instantiation (**M9**) and type-argument **inference**
  (**M10**) are implemented; **constraints** (`extends`, M11) follow. Type parameters use a named
  representation for now; de Bruijn indices (§3.1) are the pre-VM (Phase 3) target.
- **classes**: fields/constructor/methods/`this`/`new`/structural instances (**M11**),
  inheritance (`extends`, `super`) (**M12**), and access modifiers (`private`/`protected` —
  access control + nominal typing) + `static` members (**M13**), member-assignment checking
  + `readonly` (**M14**), getters/setters + `abstract` classes (**M15**), and generic classes
  (**M16**) are implemented; method-override compatibility (`TK2416`) and
  abstract-member-not-implemented (`TK2515`) remain deferred.
  Nominal typing currently constrains only the foreign→private direction (a private/protected
  type rejects a structurally-identical *other* type); widening a private-bearing instance to its
  public structural shape is not yet rejected (a one-directional divergence from tsc TS2322).
- **arrays**: `T[]` / `Array<T>`, array literals, element access, `length`, covariant
  assignability (**M17**) and **tuples** `[A, B]` (positional, indexed access, contextual typing)
  (**M18**) are implemented (built-in, no lib). Array METHODS (`push`/`map`/…) and `ReadonlyArray`
  follow. Tuple/array-literal **contextual typing** is currently declaration-position only — a
  tuple literal `return`ed or passed as an argument, and array-of-literal-type targets
  (`const a: 1[] = [1]`), are over-strict (false positive, safe direction); deferred.
- **index signatures** (`{ [k: string]: T }`, `{ [i: number]: T }`) land in **M19**; `keyof` +
  indexed-access types (`T[K]`) on concrete object types land in **M20** (evaluated eagerly).
  Generic/deferred `keyof`/`T[K]` (over a type parameter), mapped types, conditional types, and
  utility types (`Partial`, `Record`, …) remain deferred (they want the type-level VM, §7).
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
- **modules/imports** and the rest of **`lib.d.ts` globals** (`console`, string methods, `Promise`,
  …) are out of scope for now — fixtures avoid the standard library otherwise.

Other conventions: an unresolved name (`TK2304`) gets the **error type** (`any`-like), which
**suppresses cascade** diagnostics on the same expression. As of **M22** this applies in **type
position** too: an unresolved simple-identifier type reference (in any annotation — variable /
parameter / return / interface or object-type member / type-alias body / union / array / tuple /
generic name or argument / `keyof` operand) reports `TK2304` and degrades to the error type (so
`const a: Foo = 5` is only `TK2304`, never also a `TK2322`). Top-level type declarations are
**hoisted**, so a forward reference resolves (no false `TK2304`). Deferred (silent, documented
divergences — `TK2304` fires only when the name resolves to *no* space): a value used as a type
(tsc `TS2749` — the name resolves in the value space), type arguments applied to a type parameter
(tsc `TS2315`), a wrong **type-argument count** on a recognized type such as bare/over-applied
`Array` (tsc `TS2314` — `Array` is a known built-in, not "cannot find name"), and **qualified** type
names (`A.B` — needs namespaces). `strictNullChecks` is
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

## Bug-fix corpora (official-suite findings)

Fixes to **already-shipped** milestones get their own dir rather than extending an
enabled milestone dir — so the spec can be committed on its own (per
`docs/reference/dev-method.md` §1) without changing test results, then enabled by
the commit that lands the fix. Each is registered in `MILESTONE_DIRS`
(`tests/conformance.rs`) as `false` until its fix ships. Named by the official-suite
finding ID.

| Dir | Finding / backlog | Fix |
|---|---|---|
| `f3_class_member_collection/` | F3 / `01` | parameter properties + initializer-inferred class fields become real members |
| `f4_destructuring_access/` | F4 / `02` | private/protected access control runs through object-destructuring patterns |
| `f5_union_readonly/` | F5 / `03` | `readonly` enforced on object-type/interface members and through a union member-assignment |

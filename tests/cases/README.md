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
| `TK2345` | Argument type not assignable to parameter type |
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
- **classes**, **modules/imports**, and **`lib.d.ts` globals** (`Array`, `console`, string
  methods, …) are out of scope for now — fixtures avoid the standard library entirely.

Other conventions: an unresolved name (`TK2304`) gets the **error type** (`any`-like), which
**suppresses cascade** diagnostics on the same expression. `strictNullChecks` is **on** (our
default): `null`/`undefined` are distinct types, not assignable to others.

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

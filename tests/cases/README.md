# Conformance fixtures

This is the spec corpus. Per the MVP plan (§6), correctness is the whole game — these files
define, milestone by milestone, exactly what typokat accepts and what it must reject. The
harness (`tests/harness.rs`, built in M0) runs the checker over each `.ts` file and diffs the
produced diagnostics against the inline markers below.

This file is **test tooling**: how to write and read these fixtures. The substantive record of
**what typokat models and where it deliberately differs from `tsc`** lives in `docs/` —
[`docs/reference/scope.md`](../../docs/reference/scope.md) (the code-range boundary) and
[`docs/reference/divergences.md`](../../docs/reference/divergences.md) (the divergence + deferred-check
ledger). The public [`README.md`](../../README.md) "Known limitations" is the user-facing summary and
[`docs/backlog/`](../../docs/backlog/README.md) is the roadmap.

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
- **Malformed markers fail loudly.** A ` | `-separated comment segment that starts with
  `error[` but isn't a well-formed `error[TK<digits>]` (missing `]`, a non-`TK`/non-digit
  code, …) panics the harness with the file, line, and segment — a silently dropped marker
  would turn a typo like `error[TK2322` into "no expectation" and hide a real diagnostic.
  A second `error[` in a segment's residual (e.g. space-joined `error[TK2322] error[TK2345]`
  instead of ` | `-separated) is rejected the same way. Prose that merely mentions `error[`
  mid-text (not at the segment start) is still ignored.

Exact column spans are validated separately by dedicated M0 snapshot tests, not by every
fixture (keeps fixtures robust to author miscounting). Those tests live in `src/span.rs`
(`LineIndex` byte-column mapping) and `src/diagnostics/tests.rs` (compact/rich renderer
columns): exact start/end columns, same-line distinction, and multiline / tab / UTF-8 /
EOF spans.

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
| `TK2391` | Function implementation is missing or not immediately following overload declarations |
| `TK2394` | Overload signature is not compatible with its implementation signature |
| `TK2554` | Wrong number of arguments (arity) |
| `TK2555` | Too few arguments for a rest/min-arity call |
| `TK2589` | Type instantiation is excessively deep and possibly infinite |
| `TK2673` | Constructor of class is private (direct `new` outside the declaring class) |
| `TK2674` | Constructor of class is protected (direct `new` outside the declaring class/subclasses) |
| `TK2741` | Property is missing in type but required |
| `TK2769` | No overload matches this call |

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

## Deferred checks & `tsc` divergences

typokat deliberately diverges from `tsc --strict` in bounded, documented ways — it **over-reports**
(the safe direction) in some spots and does **not yet emit** some real tsc errors (deferred checks).
The full, feature-by-feature ledger — every over-report, every deferred check, and the reasons —
lives in [`docs/reference/divergences.md`](../../docs/reference/divergences.md). Do not restate it
here; add new divergences there and pin them with a fixture.

Two rules that govern the fixtures in this corpus:

- A fixture that hits a **deferred check** carries an explanatory comment and expects **0** errors
  on that line (the gap is recorded, not accidental).
- Where typokat reaches the same *verdict* as tsc but with a different **message** (e.g.
  disjoint-primitive intersections, a widened assignment literal, object/union/alias targets),
  assert **code-only** — never the substring.

Corpus-wide scope facts worth keeping in mind while writing fixtures (all detailed in the ledger):
an unresolved name gets the **error type**, which suppresses cascade diagnostics on the same
expression; `strictNullChecks` is **on** (our default); and on a call/`new` with several mismatched
arguments typokat reports one `TK2345` **per** argument where tsc stops at the first — so fixtures
keep at most one mismatched argument per call.

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
| `m31_intersections/` | M31 — intersection types (`A & B`): canonicalization, dual relation directions, merged member access + excess |
| `m32_signature_shape/` | M32 — signature shape: rest elements plus optional/default parameters |
| `m33_function_overloads/` | M33 — function overloads: ordered signatures, implementation compatibility, overload call resolution |

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
universe. (The full out-of-scope list is in the modules section of the divergence
ledger.)

## Bug-fix / backlog corpora

Fixes to **already-shipped** milestones get their own dir rather than extending an
enabled milestone dir — so the spec can be committed on its own (per
`docs/reference/dev-method.md` §1) without changing test results, then enabled by
the commit that lands the fix. Each is registered in `MILESTONE_DIRS`
(`tests/conformance.rs`) as `false` until its fix ships. Named by the official-suite
finding ID (`fN_…`) or the backlog item ID (`bNN_…`). Each corpus's **scope** and any
**accepted over-reports / documented divergences** are in
[`divergences.md`](../../docs/reference/divergences.md).

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
| `b57_tuple_array_infer/` | backlog `57` | Tuple↔Array inference pairings: `(infer U)[]` over a tuple binds the element union; tuple/array cross-kind call inference |
| `b64_readonly_infer_binder/` | backlog `64` | `infer` binders under `readonly` array/tuple syntax are collected and do not degrade aliases to error |
| `b34_fix_params_keyof/` | backlog `34` | `fix_params` evaluates substituted `keyof` constraints before deciding whether to gate them as deferred |
| `b33_as_cast_assignability/` | backlog `33` | `as` / angle-bracket assertions participate in normal assignment and call-argument assignability |
| `b54_labeled_statements/` | backlog `54` | labeled statements are checked and labeled `break` / `continue` participate in flow |
| `b59_modules_hygiene/` | backlog `59` | project-mode pending diagnostics are attributed to the owning module, and export lists validate local names |
| `b65_inference_candidate_policy/` | backlog `65` | call-site inference fixes same-parameter candidates before replaying each argument, rather than unioning incompatible candidates |

A few corpora need **construction** notes so they stay editable (the *why* of their marker choices):

`b57_tuple_array_infer/` pins the Tuple↔Array inference pairings. Type-level:
`T extends (infer U)[]` over a tuple binds `U` to the element union (non-widening —
literals stay literals; the empty tuple yields `never`), and an ARRAY source keeps
failing a TUPLE pattern (control). Call-site: a fresh array literal against a `[T, T]`
parameter and a tuple value against a `T[]` parameter both produce element candidates
(widened on fixing, so `T = number` for `[1, 2]`). The two `never`-target lines use the
target-only substring (`not assignable to type 'never'`, the m25/m27 convention): typokat
widens the assigned literal to `number` in the message where tsc keeps `1` — a pre-existing
assignment-message divergence (pinned at `m0_assign_primitives/intrinsics.ts:9`), orthogonal
to inference. Deliberately unpinned: a mixed-element literal against `[T, T]` (`h([1, "x"])`)
— tsc reports a per-element contextual error whose code/position rides on subtle
inference-priority choices.

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
No new diagnostics.

`b64_readonly_infer_binder/` pins the `readonly (infer U)[]` traversal gap. The
mutable `(infer U)[]` control already worked; the readonly array, readonly tuple,
and nested readonly-array forms must produce the same downstream assignment
errors as tsc instead of an alias-site `TK2304` plus an error-typed alias. The
regressions also pin the non-erasure boundaries: readonly sources must not match
mutable array/tuple infer patterns, user object shapes must not mimic the internal
readonly wrapper, and read/indexed access through readonly array/tuple types must
return the element type rather than the error type.

`b34_fix_params_keyof/` pins the M24/M28 asymmetry in generic call inference:
after `T` is inferred to a concrete object type, `K extends keyof T` must evaluate
to that object's key union before the candidate for `K` is accepted. The corpus
also pins the follow-on union-key edges that became observable once `keyof (A | B)`
is concrete: disjoint union keys reject as `never`, common keys stay valid,
`Pick` over a concrete union preserves common-key value types, and a named key
covered by a string index signature contributes the index value. The generic
`wrapper<T>` and intersection-wrapper controls stay clean because those `keyof`
constraints are still genuinely deferred/out of subset.

`b33_as_cast_assignability/` pins assertion expressions as normal expression
sources: the asserted type, not the untyped/out-of-subset fallback, must flow into
declaration initializers, reassignments, and call arguments; the asserted expression
is still walked so nested diagnostics such as `TK2304` are not lost. Cast validity
itself (`TS2352`) is deliberately out of scope, so every cast in this corpus is
valid and the expected errors come from the surrounding target relation.

`b54_labeled_statements/` pins labels as transparent statement wrappers for
ordinary checking, plus the flow-sensitive edges that make labels observable:
`break label` carries the current assignment state to the labeled statement's
exit, and `continue label` targets the labeled loop's back edge rather than the
innermost loop. The corpus intentionally stays on labeled blocks and `while`
loops; duplicate-label and invalid-label diagnostics are JavaScript semantic
checks outside typokat's current diagnostic surface.

`b59_modules_hygiene/` uses project fixture subdirectories. `override_attribution`
pins class-fill override checks to the derived module that owns the member span
(the buggy behavior drains the pending check into another module and renders at a
clamped nonsense location). `export_ghost` pins local export-list validation:
`export { ghost }` must report `TK2304` at the export site even when no importer
mentions the missing export.

`b65_inference_candidate_policy/` pins ordinary call-site inference for multiple
arguments that bind the same type parameter. tsc fixes a type parameter and then
checks every argument against the substituted signature; typokat must not infer a
too-wide union solely to make incompatible arguments fit. Markers are mostly
code-only because the exact fixed target can be literal-, primitive-, or
union-shaped depending on candidate priority and contextual use. The corpus keeps
at most one mismatched argument per call, per the general call-marker rule above.

## Soundness-review corpora (sprint 2026-07-10)

The `sr_*` dirs are the WU0 acceptance spec for the soundness-review-fixes sprint
(`docs/archive/sprint-2026-07-10-soundness-review-fixes.md`). Same mechanism as the
bug-fix corpora: each is committed `false` and flips `true` only in the commit that
lands its owning fix — except `sr_deferred_ledger/`, which stays `false` beyond this
sprint (its findings are open backlog items). Every fixture header records the
`tsc 6.0.3 --strict` verdict it pins.

| Dir | Owner | Findings |
|---|---|---|
| `sr_wu1_expressions/` | WU1 | nested assignment expressions (ternary / array-literal / condition / inner assignment), complete return inference (not first-return only), and `for`/`for-in`/`for-of`/`do` body + `throw`-operand checking |
| `sr_wu2_scope_overloads/` | WU2 | switch-local lexical scope (a case-clause `let`/`const` must not resolve after the switch — `TK2304`) and local (in-function) function overloads (no spurious `TK2391`; calls select declared overloads) |
| `sr_wu2_export_space/` | WU2 | **project-shaped** (registered in `PROJECT_DIRS`): `export type { x }` / `export { type x }` specifier forms must not leak the value slot; a non-type-only import using such a name as a value gets `TK2304` (the M29 stand-in for tsc's `TS1362`) |
| `sr_wu3_types_recursion/` | WU3 | `any & never` → `never` (assigning into it errors); source-intersection nominal origin (private member rejects); `keyof { [k: string]: T }` = `string \| number`; recursive mapped-value recursion guard; deep type-literal annotation depth guard (backlog 63k) |
| `sr_deferred_ledger/` | — (stays disabled) | known unfixtured under-reports: backlog `30` (JS-exact number stringify), `56` (silent instantiation cycles), `60` (fresh literals vs union targets), `62` (index-signature source parity), `66` (protected↔protected override compat), `67` (utility-alias constraint) |

Construction notes:

- **Switch scope (`sr_wu2_scope_overloads/switch_scope.ts`).** The witness is a
  MISSING `TK2304` on the after-switch use of a case-clause `let`. Per the sprint,
  this corpus pins ONLY the switch-local boundary; duplicate-declaration
  diagnostics (`TK2451`/`TK2300`, including cross-case) stay owned by backlog `18`
  and are not asserted. tsc's `TS2454` "used before being assigned" on the
  cross-clause read is a definite-assignment check typokat does not emit (deferred),
  so that control line expects zero typokat diagnostics.
- **`keyof` string index (`sr_wu3_types_recursion/keyof_string_index.ts`).** The
  missing `number` key flips a verdict in both directions: a MISSING `TK2322`
  (assignable-to `string`) and an OVER-report (the `fromNum` line — clean under tsc,
  errors at HEAD). Both are pinned.
- **Native-crash fixtures (`sr_wu3_types_recursion/recursive_mapped.ts` and
  `deep_annotation.ts`).** These STACK-OVERFLOW (SIGABRT) at HEAD — a mapped type
  whose value template is a recursive object, and a ~4000-deep nested type literal.
  They stay in the dir but must not be enabled until WU3's recursion/depth guards
  land, or `cargo test` aborts. `deep_annotation.ts` has no clean tsc oracle (tsc
  6.0.3 itself dies at ~3k); its pinned expectation is a bounded diagnostic
  (`TK2589` as the nearest existing code — WU3 owns the final choice) rather than a
  crash.
- **Deferred ledger (`sr_deferred_ledger/`).** Each fixture is a minimal pin of
  tsc's verdict for one dropped-error family plus a passing control; the dir stays
  disabled until its backlog item ships. See the deferred-check note in
  [`divergences.md`](../../docs/reference/divergences.md).

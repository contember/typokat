# Conformance fixtures

This is the spec corpus. Per the MVP plan (§6), correctness is the whole game — these files
define, milestone by milestone, exactly what typokat accepts and what it must reject. The
harness (`tests/conformance.rs`, built in M0) runs the checker over each `.ts` file and diffs the
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
- **Incomplete-surface markers** (`// incomplete[<stable-id>]`) pin an *in-scope*
  AST position the checker does not yet visit — an unsupported child slot, statement
  container, or annotation form that today exits **clean** while `tsc` rejects. The
  `<stable-id>` is the surface identity `role/surface/slot-or-variant` (see the
  surface-accounting corpus below and `tests/surface/README.md`). It mirrors the
  `error[TK…]` shape but asserts a *third outcome* — an incomplete record, not a
  diagnostic. The harness diffs these via `compare_incomplete_output` with the same
  per-line, exact-identity + optional-substring rules as `error[TK…]` (sprint
  2026-07-10 WU3). A malformed segment starting with `incomplete[` panics loudly, just
  like the `error[` parser; a valid `<stable-id>` is `[a-z0-9/-]+`. A corpus whose
  emissions have not yet landed stays registered `false` until its wiring ships.
- **Malformed markers fail loudly.** A ` | `-separated comment segment that starts with
  `error[` but isn't a well-formed `error[TK<digits>]` (missing `]`, a non-`TK`/non-digit
  code, …) panics the harness with the file, line, and segment — a silently dropped marker
  would turn a typo like `error[TK2322` into "no expectation" and hide a real diagnostic.
  A second `error[` in a segment's residual (e.g. space-joined `error[TK2322] error[TK2345]`
  instead of ` | `-separated) is rejected the same way. Prose that merely mentions `error[`
  mid-text (not at the segment start) is still ignored.

Exact column spans are validated separately by dedicated M0 snapshot tests, not by every
fixture (keeps fixtures robust to author miscounting). Those tests live in
`crates/typokat-core/src/span.rs`
(`LineIndex` byte-column mapping) and `crates/typokat-diagnostics/src/diagnostics/tests.rs` (compact/rich renderer
columns): exact start/end columns, same-line distinction, and multiline / tab / UTF-8 /
EOF spans.

## Error codes (tsc-compatible numbers, `TK` prefix)

| Code | Meaning |
|---|---|
| `TK1192` | Resolved module has no default export (narrow Bundler default-import exception) |
| `TK1314` | `export as namespace` appears outside a module |
| `TK1315` | `export as namespace` appears in a non-declaration source file |
| `TK2300` | Duplicate identifier across incompatible merged members |
| `TK2302` | Static members cannot reference class type parameters |
| `TK2304` | Cannot find name (unresolved identifier) |
| `TK2305` | Module has no exported member |
| `TK2307` | Cannot find module |
| `TK2310` | Type recursively references itself as a base type |
| `TK2313` | Type parameter has a circular constraint (`<T extends T>`) |
| `TK2314` | Generic type requires a different number of type arguments |
| `TK2315` | Type is not generic |
| `TK2320` | Interface cannot simultaneously extend incompatible base types |
| `TK2322` | Type X is not assignable to type Y (annotation/reassignment/return/property) |
| `TK2339` | Property does not exist on type |
| `TK2341` | Property is private (accessed outside its declaring class) |
| `TK2344` | Type argument does not satisfy the type parameter's constraint |
| `TK2345` | Argument type not assignable to parameter type |
| `TK2349` | Expression is not callable |
| `TK2351` | Expression is not constructable |
| `TK2445` | Property is protected (accessed outside the class and its subclasses) |
| `TK2448` | Block-scoped variable is used before its declaration |
| `TK2451` | Cannot redeclare a block-scoped variable |
| `TK2454` | Variable is used before being assigned |
| `TK2456` | Type alias circularly references itself |
| `TK2416` | Property in derived type not assignable to the same property in base type (override compatibility) |
| `TK2511` | Cannot create an instance of an abstract class |
| `TK2515` | Non-abstract class does not implement inherited abstract member (exactly one missing) |
| `TK2654` | Non-abstract class is missing implementations for members (two or more missing, aggregated) |
| `TK2540` | Cannot assign to a read-only property |
| `TK2353` | Object literal may only specify known properties (excess property) |
| `TK2362` | Left-hand side of an arithmetic operation must be `any`/`number`/`bigint`/enum |
| `TK2363` | Right-hand side of an arithmetic operation must be `any`/`number`/`bigint`/enum |
| `TK2365` | Operator cannot be applied to the two operand types (`+` general mismatch) |
| `TK2374` | Duplicate index signature |
| `TK2391` | Function implementation is missing or not immediately following overload declarations |
| `TK2394` | Overload signature is not compatible with its implementation signature |
| `TK2397` | Declaration name conflicts with built-in global identifier (shipped B14 diagnostic) |
| `TK2411` | Property is incompatible with a string index signature |
| `TK2413` | Numeric index type is not assignable to string index type |
| `TK2428` | Merged declarations must have identical type parameters |
| `TK2430` | Interface incorrectly extends a base interface |
| `TK2434` | Namespace declaration precedes the class or function it augments |
| `TK2554` | Wrong number of arguments (arity) |
| `TK2555` | Too few arguments for a rest/min-arity call |
| `TK2558` | Wrong number of type arguments |
| `TK2576` | Static class member is accessed on an instance |
| `TK2589` | Type instantiation is excessively deep and possibly infinite |
| `TK2631` | Cannot assign to a namespace binding |
| `TK2503` | Cannot find namespace |
| `TK2669` | Global augmentation is outside an external or ambient module |
| `TK2670` | Global augmentation is missing `declare` outside an ambient context |
| `TK2673` | Constructor of class is private (direct `new` outside the declaring class) |
| `TK2674` | Constructor of class is protected (direct `new` outside the declaring class/subclasses) |
| `TK2684` | The `this` context of a call is not assignable to the method's explicit receiver type |
| `TK2687` | Merged property declarations must have identical modifiers |
| `TK2694` | Namespace has no exported member |
| `TK2702` | A type-only name is used as a namespace |
| `TK2706` | Required type parameters may not follow optional type parameters |
| `TK2707` | Generic type requires between a minimum and maximum number of type arguments |
| `TK2708` | Cannot use a type-only namespace as a value |
| `TK2713` | A type-only path segment is accessed as a namespace |
| `TK2717` | Subsequent property declaration has an incompatible type |
| `TK2741` | Property is missing in type but required |
| `TK2744` | Type parameter defaults can only reference previously declared type parameters |
| `TK2749` | A value is used as a type |
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

All type displays share a hard bound: the complete rendered string is at most **320 Unicode scalar
values**, with the final three scalar values replaced by `...` when truncation is required. If
recursive rendering would descend beyond depth **64**, the entire display collapses to `...`.
Displays that fit both bounds are unchanged. Treat a truncated display like any other unstable
object/union/alias layout and assert its diagnostic code rather than its exact text.

## Reason-chain display (the nested elaboration)

The nested "…are incompatible." cascade below a headline carries the same kind of bound
(backlog `87`). It renders at most **16** nesting levels; beyond that the remaining wrappers
collapse into one line — `... <n> more nested levels omitted.` — followed by the innermost
(terminal) cause, so the reader still learns what actually mismatched. A chain whose first
level past the cap is *already* terminal has nothing to omit and renders unchanged. The
indent is clamped with the depth, so a rendered chain is at most 18 lines and its deepest
indent is 36 characters, however deep the reason tree is.

The deepest chain any fixture in this corpus produces is 6 levels, so the cap is invisible to
every existing marker. `b87_reason_chain_depth/` is the corpus that pins the boundary itself.

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
| `m28_utility_types/` | M28 — utility types (production default-library aliases: Partial…Omit/ReturnType; Uppercase-family intrinsics) |
| `m29_modules/` | M29 — local relative modules / named imports + exports across files (project fixture subdirectories) |
| `m30_contextual_literals/` | M30 — contextual typing of fresh object / array / tuple literals |
| `m31_intersections/` | M31 — intersection types (`A & B`): canonicalization, dual relation directions, merged member access + excess |
| `m32_signature_shape/` | M32 — signature shape: rest elements plus optional/default parameters |
| `m33_function_overloads/` | M33 — function overloads: ordered signatures, implementation compatibility, overload call resolution |
| `b72_bundler_project_tracer/` | Backlogs 15 + 72 — disabled Bundler project/CLI contract: root discovery, local `oxc_resolver` lookup, complete module-form accounting, deterministic summary |
| `b15_acyclic_source_reexports/` | Backlog 15 — permanently disabled raw corpus plus exact TypeScript 6.0.3 oracle for acyclic named source re-exports; black-box acceptance owns project semantics |

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

Flat `.d.ts`/`.d.mts`/`.d.cts` fixtures run through the same one-file project
path so their filename-derived declaration context is preserved.

### Bundler project tracer contract (backlogs 15 + 72)

`b72_bundler_project_tracer/` is a permanently disabled raw-conformance corpus. The raw fixture
route does not consume `tsconfig.json` or project summaries, so WU3 enables its dedicated black-box
integration contract instead. [`contract.json`](b72_bundler_project_tracer/contract.json) is the machine-readable WU1
acceptance contract. It pins `typokat check --project-summary json <directory|tsconfig.json>`, the
exact normalized summary shape, identity ordering, exit precedence, and the pinned `tsc 6.0.3`
Bundler oracle for every synthetic project.

The admitted slice is deliberately local and dependency-free: exact `strict`, `noEmit`, `module`,
and `moduleResolution` values, a non-empty project-relative `files` array, named imports,
declaration/local export forms, extensionless imports, and runtime-style
`.js` → `.ts` substitution. Roots are normalized, deduplicated, and path-sorted. Missing local
modules remain ordinary `TK2307` diagnostics. Missing/empty/invalid `files`, roots outside the
project, unsupported root extensions, `include`/`exclude`, `extends`, `references`, `lib` and other
unconsumed compiler options, bare
specifiers, default/namespace/side-effect/import-equals imports, default/export-assignment/
namespace/star exports, namespace-bearing named source re-exports, non-Bundler profiles, and module
cycles are explicit project-level non-clean identities. Acyclic local named source re-exports are
admitted by the separate B15 contract below. Unsupported projects stop semantic checking. No listed
form may disappear before accounting or fall through an error type.

Both directory and explicit-config invocations must produce the same normalized summary. Every
array is ordered by normalized relative path, then source position, then identity. The corpus is a
negative control at pre-change HEAD: direct project input is unavailable, `.js` substitution fails,
and unsupported forms are filtered before a project-level identity exists.

### Acyclic named source re-export contract (backlog 15)

`b15_acyclic_source_reexports/` is permanently disabled on the raw conformance route. Its
multi-file projects pin aliases, outer and inline type-only barriers, value/type class pairs,
two-barrel chains, missing targets and members, empty lists, duplicate output names, namespace
provenance, cycles, and deferred syntax. `contract.json` records the exact `tsc 6.0.3 --strict
--noEmit --module esnext --moduleResolution bundler` result. The dedicated black-box integration
test owns Bundler summaries and the separate explicit and legacy route baselines. The exact oracle
also corrects the planned empty-list rule: `export {} from` a missing module is clean and emits no
`TS2307` in TypeScript 6.0.3. The production Bundler route admits the namespace-free acyclic rows;
explicit and legacy routes remain frozen. Export attributes retain their exact exit-2 baseline.

### Default module slot contract (backlog 15)

`b15_default_module_slots/` is a permanently disabled raw-conformance corpus. Its machine contract
records 38 projects and both root orders against pinned `tsc 6.0.3`. The admitted rows cover named
and anonymous default classes and functions, literal/object/arrow expressions, namespace-free
identifier projection including a type-only interface slot, direct and type-only default imports,
lexical visibility, distinct
default/named slots, and named default functions that close adjacent overload groups without
replacing their implementation-signature default surfaces while named imports retain the overload
surface and mismatched exported/non-exported rows report `TK2383`, missing modules/defaults, and
both wrong-space barriers. Anonymous default subclasses retain abstract-completeness diagnostics
with the display name `default` (`TK2515` for one member and aggregated `TK2654` for several). The expression-owned
error row proves that a default expression is checked exactly once.

Deferred rows stay explicit non-clean outcomes: default interfaces; namespace-bearing producers,
including enum-backed identifiers whose qualified surface is not modeled;
local export-list and named-import spellings of `default`; source re-exports involving a default
slot; and mixed default imports. The mixed named form retains two named specifiers, including one
inline type-only member. Duplicate production uses five real publication paths — direct class,
direct function, default expression, local export-list default, and source export-to-default — with
all 15 unordered pairs plus the 10 reverse lexical orders. Exact `TS2528` and related duplicate
diagnostic parity is not required. Star/namespace forms, cycles, packages, and CommonJS keep their
existing exact identities in the B72 contract; this corpus references those controls instead of
copying them. The `post_wu6_*` production acceptance tests are enabled. Only the pinned-local-tsc
oracle and the obsolete pre-WU6 RED witness remain intentionally ignored.

## Fixture routing

Every enabled fixture runs through the same public production driver and the TypeScript 6.0.3
default library. `MILESTONE_DIRS` rows carry only the directory and its enablement flag.
`ENABLED_FIXTURES` and `ENABLED_PROJECT_FIXTURES` admit selected rows from mixed corpora without
introducing a second checker route. Keep every referenced directory registered in
`MILESTONE_DIRS`; the harness rejects an unregistered selected fixture. Mixed corpora stay
registered `false` until the whole directory is ready.

The backlog-14 flat and project corpora are enabled as whole directories and run through the same
public production driver. Residual model gaps remain explicit outcomes with independent owners.

`native_array_annotation_identity.ts` pins the annotation side of the native-array bridge:
`Array<T>` and `ReadonlyArray<T>` name the intrinsic array types themselves, so an annotation
resolving to either library declaration must carry the same identity as `T[]` / `readonly T[]`
in both relation directions and in every annotation position (variable, parameter, return,
alias, interface member, class member, nested type argument). The library interface bodies stay
the *member* surface `project_library_member_surface` projects — the fixture keeps `length`,
`map`, and the readonly `push` withholding as the non-permissive controls. The role is keyed on
the universe-local declaration identity selected from the library's compilation-global scope, so
`library_identity_shadowing.ts` (a module-local `interface Array<T>`) remains the negative
witness and is unaffected. Markers are code-only wherever a side is an array or alias layout.

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
| `b22_new_callee_forms/` | shipped backlog `22` | parenthesized and non-generic one-step const-aliased class construction retains abstract/private/protected checks |
| `b28_interface_extends/` | backlog `28` | interface `extends` composition (inherited members in assignability/keyof/mapped; own overrides) |
| `b29_alias_cycles/` | backlog `29` | surface alias cycles → `TK2456` (direct/mutual/through unions); legal member recursion resolves |
| `b30_negative_literals/` | shipped backlog `30` | negative numeric literal types are literals, not `any` (annotations, unions, template holes, extends) |
| `b30_numeric_stringify/` | shipped backlog `30` | ECMA number-to-string thresholds and `${number}` long-decimal regression control |
| `b32_eager_keyof_forward/` | backlog `32` | forward-referenced `keyof` operands retain their eventual keys in annotations and generic overload constraints |
| `b55_template_memo/` | backlog `55` | template evaluation must not memoize results computed under an exhausted TK2589 budget |
| `b56_instantiation_cycles/` | shipped backlog `56` | direct/mutual instantiation cycles emit TK2589 without poisoning terminating siblings or query order |
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
| `b45_operator_result_typing/` | backlog `45` | arithmetic/bitwise/shift operators produce `number` instead of the error type; arithmetic operand rule (`TK2362`/`TK2363`) and the `+` general mismatch (`TK2365`) |
| `b101_conditional_logical_values/` | backlog `101` | a ternary and a logical expression carry a real value type — the arm union, and `falsy`/`truthy`/`non-nullish`-part-of(left) joined with the right operand — instead of the error type, so everything downstream of them is checked again |
| `b104_excess_property_descent/` | backlog `104` | the excess-property walk descends through a ternary / logical into the operands the target contextually shapes, so a fresh literal in an arm keeps its freshness |
| `b87_reason_chain_depth/` | backlog `87` | the reason chain is bounded like the type renderer next to it: past 16 nesting levels it collapses to one elision line plus the retained innermost cause, and the indent stops growing |
| `b100_logical_condition_narrowing/` | shipped backlog `100` | composed conditions (`&&`, `\|\|`, `!` over them) narrow the branch they guard, in `if` / `while` / ternary / expression positions — with the complementary branch of every shape pinned, since a fix that narrows the wrong branch deletes diagnostics |
| `b67_utility_alias_constraint/` | shipped backlog `67` | the modeled `ReturnType` callable constraint rejects non-callable arguments while represented function shapes preserve their evaluated return types |
| `b70_this_parameter_typing/` | shipped backlog `70` | explicit non-positional receiver slots, receiver calls/relation, ThisParameterType/OmitThisParameter, and contextual ThisType |
| `b77_returntype_call_signatures/` | shipped backlog `77` | ReturnType extracts single and last-overload returns from represented object call signatures |
| `b66_protected_override_compat/` | backlog `66` (disabled) | acceptance target for protected↔protected TK2416 plus the nested protected-lineage architecture stop gate owned by `63(d)` |
| `b38_minimal_ambient_prelude/` | shipped backlog `38` | historical bounded `console` and numeric `Math` acceptance, now exercised through the production default library |
| `b38_prelude_lookup_boundaries/` | backlog `38` follow-up | project-shaped ambient-library boundaries: a value-bearing `import type` blocks ambient value fallback, export lists cannot inherit library names, and local type-only exports do not acquire ambient value slots |
| `b41_generic_methods/` | shipped B41 | generic method/call/construct signatures: persistent binders through outer substitution, calls, relation, overloads, inheritance, and cache order |
| `b74_declaration_hoisting/` | backlog `74` | forward ordinary/generic/overloaded function calls see hoisted callable types; `var` binds in its containing function/module scope |
| `b78_generic_class_value_aliases/` | backlog `78` (disabled) | one-step const aliases of generic classes retain substitution and abstract/private/protected construction facts |
| `b92_contextual_duplicate_diagnostics/` | shipped backlog `92` | one error nested inside contextually typed arguments is reported once, not `2^depth` times; the raw argument walk still reports wherever no committed contextual walk supersedes it |
| `b43_namespaces_declaration_merging/` | shipped namespace sprint (65 flat fixtures + 6 projects enabled) | namespace type/value containers, repeated interfaces, qualified names, legal cross-space merges, ambient/global boundaries, and explicitly owned deferred UMD/enum tails |
| `b14_full_lib_loading/` | shipped backlog `14` production corpus; archived after final WU7 PASS and exact-`d1aa6d4` remote CI | TypeScript 6.0.3 default-library globals, native-type bridges, intrinsic roles, identity-safe shadowing, and explicit unsupported outcomes |
| `b14_full_lib_loading_project/` | shipped backlog `14` production corpus; archived after final WU7 PASS and exact-`d1aa6d4` remote CI | fast external-module routing, collision/private-epoch order, global-object contributions, global augmentation/UMD forms, and unavailable-merge withholding |
| `b102_frozen_prefix_writes/` | shipped backlogs `102`/`103` regression net | fresh script globals reach a writable delta scope; library collisions route through private replay without mutating the shared base |
| `b102_frozen_prefix_writes_project/` | shipped backlog `102` project regression net | cross-file script globals in both input orders; module-scope declarations keep shadowing instead of publishing |
| `b103_library_merge_correctness/` | shipped backlog `103` correctness tier | successful library interface/namespace/function/cross-slot merges, illegal-collision controls, and unchanged shared-route behavior |
| `b103_library_merge_correctness_project/` | shipped backlog `103` project correctness tier | both input orders, whole-body `declare global` lexical scope, `globalThis`, UMD, destructuring, slot matrix, benchmark collision, classifier mutation input, controls, and cross-project isolation |
| `sr_semantic_duplication/` | shipped semantic-duplication/class-application cutover | class callable surfaces are lowered once; immutable recursive class applications publish complete SCC projections before demand, preserving diagnostics, overloads, parameter properties, structural relation, and nominal origin |
| `sr_semantic_duplication_project/` | shipped project-mode semantic-duplication gate | dependency-first class publication and heritage poison remain deterministic across module/input order |

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

`b102_frozen_prefix_writes/` and `b102_frozen_prefix_writes_project/` are the shipped backlog-`102`
corpora. The **publication** half is project-shaped: an ordinary `globals.d.ts`-shaped script file
declaring an interface, `declare var`, `declare function`, `declare namespace`, `class`, and a
`type` alias, consumed from a second file — once in declaration order and once with the consumer
fed first, because the hoist reservations are per-form and a value slot filled only at its own
module's execution would let the wrong-type witness vanish rather than error. Every consumption is
a clean read plus a wrong-type witness, so an `any`/error-type recovery cannot pass the corpus. Its
controls are the shapes that must NOT publish: an external module's `interface Array<T>` and
module-local `Date`/`class` shadow inside their own file and stay invisible to a sibling script
file, whose `Array`/`Date` surfaces must remain the library ones.

The flat library-collision fixtures route through the sparse private epoch shipped by backlog `103`. Legal
merges and overloads expose the user fragment alongside the library surface; illegal `var`,
lexical, type-alias, and class collisions preserve the library winner and the remaining missing
duplicate-declaration diagnostics stay ledgered in
[`divergences.md`](../../docs/reference/divergences.md). The binder's frozen-write ledger remains
an internal mutation witness, not a user-facing incomplete channel.

`b103_library_merge_correctness/` and its project sibling are the production acceptance contract.
They require the user fragment to be visible alongside the
unchanged library surface. Every positive read has a wrong-type or missing-member sibling, so an
error-type recovery cannot satisfy the fixture. The illegal `type Partial` and `class Date`
collisions deliberately keep the library declaration as the observable winner; typokat's missing
duplicate-identifier diagnostics remain owned by backlog `18`.

The project half pins Array and `declare global` merges in both file orders, legal
global-object/`globalThis` contributors, a UMD `export as namespace` route whose existing
backlog-15 incompletes remain visible, value/type/namespace slot coexistence, and a nested
destructuring leaf whose bound name—not its property keys—collides with `RegExp`. The
`classifier_false_negative_candidate` project is consumed by a direct mutation test; marker
semantics alone cannot prove that a deliberately omitted candidate fails closed before mutation.
`zz_private_shared_isolation` runs after the augmenting projects and proves that the process-wide
base did not acquire their members. Direct epoch receipts additionally own immutable-prefix
sharing, private graph/semantic identity, cache/event isolation, and zero full-source fallback.

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
valid and the expected errors come from the surrounding target relation. The separate
`b73_surface_accounting/assertion_compatibility.ts` fixture pins that missing validation with
exact `expr-infer/{as,type}-assertion/compatibility` outcomes owned by backlog `75`.
`assertion_compatibility_deferred_targets.ts` pins the conservative syntax-only fallback for
assertions nested in arrow, function, and class scopes on otherwise deferred assignment targets;
class expressions retain their existing independent owner.
`assignment_expression_nested_scope_target.ts` independently owns the same boundary across every
representable assignment target, including destructuring children, while retaining additive
assertion and class-expression records and exactly-once RHS traversal.
`update_expression_nested_scope_target.ts` pins the analogous target-wide prefix/postfix boundary
across every representable `SimpleAssignmentTarget` family without weakening ordinary update-
operand traversal.

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

`b45_operator_result_typing/` pins two halves of one defect. The **result** half:
every arithmetic (`- * / % **`), bitwise (`& | ^`) and shift (`<< >> >>>`) operator
produces `number`, so an ordinary annotation/argument/return mismatch downstream of
an operator is reported instead of being absorbed by the error type — including
through generic callback inference (`callback_inference.ts`, a compact witness in the unified
production default-library context). The **operand** half: `TK2362`/`TK2363` per
side, one diagnostic per bad side rather than a combined one, and `TK2365` for a `+`
whose operands satisfy none of its string/number/`any` rules. The result stays `number`
even when an operand is rejected, which is why most rows carry both an operand marker
and a `TK2322`. Three deliberate boundaries are pinned rather than fixed here: an
operand's `null`/`undefined` members are stripped before the numeric test (tsc's
`checkNonNullType`) and an `unknown` operand is exempt, so `arithmetic_operand_kinds.ts`
carries no operand marker on those rows — tsc's `TS18046`/`TS18047` are the separate
strict-null / unknown-receiver families typokat does not implement;
`arithmetic_boolean_bitwise.ts` records the one over-report, where tsc replaces the two
operand diagnostics with the single `TS2447` boolean-operator suggestion. Comparison
operators, `in`/`instanceof`, unary and update forms stay out of this corpus and remain
owned by backlog `45`.

`b101_conditional_logical_values/` is `b45`'s sibling for the two remaining
coarse-typed expression forms. `ternary_result_assignment.ts` and
`logical_result_assignment.ts` pin the value of each operator in annotation position;
`result_positions.ts` carries it into a call argument, a `return`, a nested
initializer, and the `b45` arithmetic operand rule; `nested_compositions.ts` composes
the forms; `narrowed_arms.ts` pins that each arm/operand reads its own flow edge,
including through a `&&`-composed test (backlog `100`); `contextual_arms.ts` pins that
the target contextually types the operands that carry the value (both ternary arms,
both `||`/`??` operands, `&&`'s right — tsc's `getContextualTypeForBinaryOperand`);
and `clean_controls.ts` is the negative set, including the tsc short-circuits (`??`
over a non-nullish left, `||` over an always-truthy left, `&&` over a never-truthy
left are just that left operand) and the `let` literal-union widening that keeps a
later reassignment legal. Rows whose result is a union are **code-only** — typokat
names the offending member where tsc names the whole union. `falsy_split_divergence.ts`
is the one accepted over-report, ledgered under `narrowing/logical-value-falsy-split`.
One deliberate gap is NOT pinned here: tsc reports a contextually typed `return`
ternary once per arm, where typokat reports one at the returned expression (noted in
`result_positions.ts`'s header). The excess property inside an arm, which this corpus
also left unpinned, is owned by `b104_excess_property_descent/` below.

`b104_excess_property_descent/` is the follow-on defect `b101` surfaced: the
excess-property walk is syntax-directed and recognised an object literal only
*directly* in the checked position, so it stopped at the first non-literal node. It
now descends through the same operands `context_can_shape_fresh_literal` does — both
ternary arms, both `||`/`??` operands, `&&`'s right — which is stated once in
`contextual_value_operands` (`crates/typokat-check/src/check/checker/expr.rs`) and consumed by the
contextual re-walk gate and both excess walks. `ternary_arms.ts` pins each arm
independently plus the array/tuple and nested-declared-target children;
`logical_operands.ts` pins each shaped operand; `nested_compositions.ts` pins that the
descent is recursive; `positions.ts` runs it through every
`check_excess_properties_for_target` call site (initializer, reassignment, array
element, object property, call argument, `return`), because a descent added at one
site only would leave the rest silent; `union_targets.ts` marks the boundary — `T |
undefined` unwraps and is checked, a multi-shape union is still backlog `60`'s dropped
error; and `clean_controls.ts` is the must-stay-silent set (a variable-reference arm,
an admitted extra key, an unannotated `const`, `&&`'s left, optional / index-signature
/ empty / `unknown` targets). Markers are code-only: tsc nests its excess elaboration
under one `TS2322` for the whole assignment where typokat emits the freestanding
`TK2353` at the key. Two count over-reports are ledgered rather than hidden —
`objects/excess-descent-count` (a key per arm where tsc stops at the first) and
`objects/excess-dead-logical-operand` (`||`/`??` with a fresh-literal left, whose right
operand tsc short-circuits away). tsc's `TS2872`/`TS2869` short-circuit warnings are
unimplemented families and carry no marker.

`absorbed_arm_divergence.ts` is the one accepted **false positive**, ledgered as
`objects/excess-absorbed-arm`. tsc's excess check runs on the relation of the
expression's *value*, and that value is a subtype-reduced union whose subtype test
itself performs the excess check — so an arm whose excess key is admitted by a sibling
arm's type is absorbed and never checked. A syntax-directed walk cannot see that. The
fixture pins the three absorbing shapes next to the two non-absorbing controls, so the
boundary is a fixture, not a footnote.

`b87_reason_chain_depth/` pins the **reason-chain depth cap** (see "Reason-chain display"
above) from both sides, because a cap is only correct if it fires at exactly one depth.
`property_chain_at_cap.ts` and `array_chain_at_cap.ts` are the *lower* bound: each is a
chain of exactly 16 levels, and each pins the line an early cap would be the first to
swallow — the `leaf` property wrapper at level 15, and the `string[]`/`number[]` array line
at level 15. `property_chain_first_elision.ts` is the *upper* bound: 18 levels un-elided, of
which exactly one wrapper (the `leaf` wrapper, level 17) is omitted — so a cap of 17 would
render the chain whole and emit no elision line at all, and a cap of 15 would omit two.
`property_chain_deep.ts` and `array_chain_elides.ts` then pin that the omitted
count is exact rather than approximate, and that the innermost cause survives the elision
on both the property and the array descent. `shallow_chains_unchanged.ts` is the
regression net for the acceptance bar — the deepest chain measured across this corpus, the
official-suite corpus, and the unit tests is **6**, so the cap must be invisible to every
diagnostic that exists today.

Markers cannot express the other half of the contract: a marker substring is trimmed, so it
cannot assert indentation, and the harness has no notion of a byte budget. The line count,
the total byte count, the clamped indent, and the collapse of a deep declared *recipe*
chain are therefore pinned directly in `crates/typokat-diagnostics/src/diagnostics/tests.rs`
(`reason_chain_*`, `type_display_bounds_a_deep_declared_*`), which mirrors the cap as its
own literal rather than importing the implementation's constant.

`b92_contextual_duplicate_diagnostics/` pins **occurrence counts**, not presence.
Each fixture nests one unresolved name — `undeclaredThing` — inside contextually typed
arguments at depths 1 through 8, one depth per line, and carries exactly one
`error[TK2304]` marker on each of those lines. No new marker syntax is involved: the
per-line rule at the top of this file is already a **multiset** equality, so a single
marker on a line that produces `2^depth` byte-identical copies fails as an ordinary code
mismatch. What hid the bug was that no fixture nested contextual arguments deeply enough
for the doubling to appear, not the marker contract.

The four fixtures are the measured discriminator matrix. `nested_arrows.ts` and
`nested_object_literals.ts` use signatures whose parameter type *structurally contains*
the type variable (`run<T>(step: (value: number) => T)`, `wrap<T>(value: { inner: T })`),
so candidate inference re-walks as well and each level costs three walks. `bare_type_variable.ts`
(`shapeOf<T>(shape: T)`, the shape of real `zod`) and `non_generic_callback.ts`
(`describe(fn: () => void)`, the shape of real `describe`/`it`) cost only two walks per
level — but both of those walks retain their effects, so all four shapes duplicate at the
same `2^depth`. The two cheap-to-walk shapes are the ones that hang in the wild, so they
are pinned independently rather than treated as covered by the generic ones.
`tsc 6.0.3 --strict --noEmit` reports exactly one `TS2304` on every line of all four
fixtures, at every depth. The full 1–12 depth sweep lives in
`crates/typokat-check/src/check/checker/calls/contextual_duplicate_diagnostics_spec.rs`, which reports the
observed count in its failure text; the fixtures stop at 8 to keep corpus runtime bounded.

`retained_raw_walks.ts` is the other half of the contract and the reason the fix is not
simply "stop reporting from the raw walk". Only the committed walk sees the instantiated
contextual target, so it is the walk that reports — but it does not always run. This
fixture pins every case where it declines and the raw walk is therefore the only walk:
a generic arrow and a two-call-signature context (both return before entering the arrow
body), a failed overload (the committed argument check never runs), a fresh object
literal against a primitive parameter, a `never` parameter that breaks the committed
loop before a later argument, two superseded arguments in one call, a skipped spread
that shifts the callback's argument index, and the `new` / `super(...)` paths. Dropping
the raw walk unconditionally deletes every diagnostic in it. Its `TK2554`/`TK2345`/
`incomplete[call/call-arguments/spread-argument]` surface on the spread line is the
spread-argument deferral (owner `71`), pinned only so the fixture is complete; tsc
reports `TS2556` there instead. Every other diagnostic matches `tsc 6.0.3 --strict` at
the same line and column, the sole gap being the unimplemented `TK7006`.

`nested_retained_raw_walks.ts` extends that net one level down, for backlog `95`. Since
`95` the raw argument walk of a re-walkable shape is memoized per call region, so a call
re-executed by a contextual re-walk of an *enclosing* argument has its raw walk served
from the memo and produces no records. Every shape in `retained_raw_walks.ts` is where
that would delete a diagnostic, so each is repeated here inside `run<T>(step: (value:
number) => T)` (and once inside `wrap<T>(value: { inner: T })`), which is what makes the
inner call run twice. A memo that is served and never recovered fails this fixture and
passes `retained_raw_walks.ts`, because at the top level nothing is ever re-executed.
Same `tsc 6.0.3 --strict` correspondence, same `TK7006` and spread-deferral gaps.

`b43_namespaces_declaration_merging/` contains 74 flat fixtures plus six project
fixtures with 12 source files (86 source files total) for the namespace/declaration-space
sprint. It covers merged property/method/call/construct/index and
heritage surfaces, overload precedence and query order, generic constraint/default compatibility,
recursive merge groups in opposite declaration orders, namespace syntax/reopening/visibility,
qualified lookup through value/type/namespace slots, all approved keep-pairs, representative
interface+constructor-variable coexistence, and ambient/global/UMD boundaries. The corpus pins
strict `tsc 6.0.3 --strict --lib es5 --module commonjs` results. Ordinary
namespace-before-function/class pairs report `TS2434`; only their ambient reverse-order forms are
clean. The approved WU0 addendum expands backlog `43` to full class+interface(+namespace)
composition in both class/interface orders. Its six focused fixtures require interface-added
required, method, generic, and recursive instance members; matching and conflicting generic
headers; property/method/heritage conflicts with non-permissive recovery; instance call/construct
signatures; namespace placement; and preservation of construction, capability separation, existing
and namespace-added statics, nested namespace types, and private nominal origin. Omitted versus
introduced constraints/defaults are legal and effective in either order; only conflicting supplied
names/constraints/defaults or arities reject. Every invalid merged generic group requires the
complete typed `tsc` recovery surface at use sites. Recovery may not use `any`, the error type,
`unknown`, synthesized `never`, a partial application vector, a dropped fragment, or whole-group
exhaustion; each class/interface member and each oracle-distinct fragment-local parameter position
must remain typed.
The WU2 spec addendum adds exact qualified-path topology and slot edges: alias/class roots used as
namespaces (`TK2702`), a value-only root (`TK2503`), missing/value-only intermediates and a
namespace-only leaf (`TK2694`), type/class-only intermediates (`TK2713`), and no parent fallback.
Its ambient export-list fixture pins ordinary and explicit type-only export-specifier aliases whose
targets occupy type space as public, value alias use as a type (`TK2749`), and both the original
pre-list name and a post-list declaration as hidden (`TK2694`). These began as WU2 classification
expectations; WU3 subsequently shipped successful qualified leaf lowering and generic application.
The second WU2 addendum pins checker-local qualified roots only after lexical namespace lookup finds
no namespace meaning. Callable and instance-class type parameters, an active conditional `infer`
binder, a mapped key binder, and the builtin `Array` type then report `TK2702`; a class type
parameter behind the static-member barrier reports `TK2503`. Clean same-name controls prove lexical
namespaces beat callable parameters, `infer`/mapped binders, and the builtin `Array`. The measured
independent `TK2694` routes are union, intersection, tuple, indexed access, conditional, mapped,
template literal, function/constructor type, type-literal call/construct/method signature, and class
constraint/field/method annotations. Named/default import coexistence is a project-shaped oracle
recorded in the sprint rather than a single-file fixture. Reopening-private helper lookup is already
pinned by `namespace_visibility.ts`; no duplicate fixture owns it. Three mixed-order controls require
a later missing-root `TK2503` to survive an earlier successful qualified endpoint or a
backlog-42-deferred enum endpoint.
The valid UMD `.d.ts` is the current `export =` form witness: it keeps both
`export as namespace` and `export =` explicitly incomplete under backlog `15`; local access to its
merged `Options` alone would not prove a global UMD export. `export as namespace` may also publish an
ordinary named-export external-module surface, which backlog `15` must cover differentially before
claiming UMD support. The
enum/function/namespace chimera keeps its enum surface explicitly incomplete under backlog `42`,
pins the namespace-order diagnostic, and requires receiver-use errors so an `any`/error recovery
cannot pass it. Exact `TS2567` ownership remains a WU0A/direct-test gate.
Direct merged-class identity/publication remains a WU0A test because marker fixtures observe only
downstream surfaces and diagnostics.
The WU4 addendum adds seven focused fixtures: type-only interface+namespace pairs in
both orders; ordinary, overloaded, reverse-order, and ambient function+namespace publication;
ordinary and ambient class+namespace publication in both orders; and the standalone-namespace
boundary, inferred function+namespace publication boundaries, namespace member validation, and the
explicit namespace-payload incomplete ledger. Function/class augmentation must
retain callable/construct, static/tag, and nested-type surfaces, with `TK2434` only on ordinary
reverse-order pairs. The standalone namespace's exported value now instantiates and publishes its
value receiver through WU6A, while exported interface/type members remain usable.
Forward-demand and inferred-return cycles remain precise backlog-76 incomplete records, while
post-body, annotated-recursive, and declared-overload surfaces require non-permissive witnesses.

The WU6 hard-stop addendum pins two residuals found by checking the authoritative TypeScript 6.0.3
`lib.es5.d.ts`. Ambient public members must resolve unqualified from interface bodies in the same
block and across reopenings in both source orders. Ordinary namespace fragments retain their own
private overlays while sharing only exported members. A locally declared global `Array<T>` must
also be a real heritage group: `interface X extends Array<string>` inherits its locally merged
members instead of taking the builtin-array opaque fallback. Semantic wrong-type witnesses keep
both fixtures non-permissive under error/`any` recovery.

The durable WU6 proof is
[`tests/fixtures/lib-es5-6.0.3/readiness.toml`](../fixtures/lib-es5-6.0.3/readiness.toml).
All 28 interface/value pairs, repeated `Date`/`Number`/`String` interfaces, local `Array` heritage,
and both `Intl` type and value paths are represented. The superseding proof at checker commit
`23bad42` established **GO for starting backlog 14**; the current gate has four `TK2430`s and 181
explicit incomplete outcomes: 173 owned by `75` and eight type predicates owned by `50`; no
namespace owner remains.
`deep.Intl.value` now contributes the same `TK2322` as tsc, so the synthetic suffix is exactly 66
`TK2322` diagnostics with no `TK2304` and no added incomplete. These counts are exact accounting,
not a broad allowlist. The namespace lifecycle closed that historical start gate. Production
standard-library loading subsequently shipped under the archived closure sprint after final WU7
PASS and exact-`d1aa6d4` remote CI; this proof does
not by itself claim that owners `50`/`75` or checker 1.0 are complete.

The WU6A addendum specifies standalone instantiated namespace values under ADR-0010. Five enabled
`wu6a_*.ts` flat fixtures pin ordinary and ambient/reopened roots as first-class aliases,
arguments, and returns; static and computed reads; function calls and class construction through
members; nested/dotted bottom-up values; private/missing members; namespace-body traversal; and the
exact `const`-readonly versus `let`/`var`/function/class/nested-property mutability matrix. Root
assignment and update forms require `TK2631`; a non-callable/non-constructable namespace root
requires `TK2349`/`TK2351`. Pure ordinary and ambient type-only
namespaces allocate no value storage or empty object: every alias/read/member/call/`new` value demand
pins exact `TK2708`, while qualified type use remains clean.

Two ordinary instantiated namespaces in `wu6a_first_class_values.ts` intentionally publish equal
structural shapes and clean member reads. The implementation direct gate must prove distinct
namespace-owned `ValueStorageId`s even when hash-consing deduplicates their structural `TypeId`.

`wu6a_unavailable_ledger.ts` keeps every admitted exported child that cannot publish on its precise
non-namespace boundary and
requires the complete parent value to remain terminal `Unavailable`: `typeof` type query → `52`
(`annotation-lower/type-query/typeof`), inferred initializer/function return → `76`, enum → `42`,
import-equals → `15`, duplicate value → `18`, TDZ/use-before-assignment → `47`, and the current
class/static dependency cycle → implementation or `76`. The stable payload incomplete ids are
pinned where they already exist. The duplicate row deliberately retains its backlog-18 incomplete
instead of pretending tsc's `TS2451` pair is implemented; the TDZ row pins future `TK2448` and
`TK2454` because no honest typed incomplete surface currently represents that semantic check. Root
aliases in the ledger are direct-state demands: they must observe no `Ready` value, partial prefix,
empty/error object, or fallback owner.

The class/static row is a real dependency cycle: the namespace root requires the exported class
value while the class's inferred static `root` initializer references that namespace value. It must
retain `decl/class-declaration/namespace-payload-static-cycle` at the class declaration under owner
`76` and withhold the whole namespace, never publish a partial class/root pair.

TypeScript 6.0.3 admits a plain private `using` declaration inside an ordinary namespace and the
namespace root remains a valid value, but rejects `export using` (`TS1491`), namespace `await using`
(`TS2852`), exported `await using` (`TS1495`), and ambient `using` (`TS1545`). The private form can
instantiate the group but contributes no public property. No WU6A marker or owner is invented: a
later direct gate must either support private `using` or assign it one concrete non-namespace
`Unavailable` owner before publishing `Ready`, and must prove the private binding cannot leak. Any
future admitted `await using` or exported using form requires the same owner decision first.

The enabled `wu6a_project_forward/` and `wu6a_project_reverse/` projects contain the same reopened
namespace declarations and semantic demands in opposite source/input order. The marker oracle pins
the same `TK2322`; direct gates additionally pin source-key-stable `ValueStorageId` allocation,
distinct storage for equal structural `TypeId`s, atomic terminal publication, and query-free
construction. The checker-wide EventStore suite independently pins the exact replay tuple
`(original_module_ordinal, source_start, event_ordinal, record_ordinal)`. Existing
`wu4_function_namespace_matrix.ts`,
`wu4_class_namespace_matrix.ts`, `keep_pairs_forward.ts`, and `keep_pairs_reverse.ts` remain the
function/class owner controls; WU6A does not duplicate them.
Across the six flat fixtures and both project orders, the pinned tsc oracle has exactly 33
diagnostics: `TS2322` x5, `TS2339` x2, `TS2345` x4, `TS2349`, `TS2351`, `TS2448`, `TS2451` x2,
`TS2454`, `TS2540` x2, `TS2631` x4, and `TS2708` x10.
The enabled five-flat/two-project subset owns 29 of them; the remaining four stay in the disabled
unavailable ledger.

The second WU6A adversarial addendum is the `wu6a_review_*` corpus. Six flat fixtures and both
forward/reverse two-file projects are enabled after their direct terminal gates passed. The
`wu6a_review_using_legality.ts` parser-boundary ledger remains disabled. Together, all seven flat
fixtures plus the two projects pin 23 strict TypeScript 6.0.3 diagnostics:

- `wu6a_review_root_provenance.ts` pins `TS2349` x3 and `TS2351` x3 through direct parentheses,
  aliases, and chained aliases. Explicit `const`/`let`/`var` annotations to `any` erase namespace
  provenance and remain callable/constructable controls.
- `wu6a_review_type_only_fallthrough.ts` pins `TS2349` and `TS2351` on an outer number shadowed only
  in type/namespace space. The bare read is clean and no value demand may report `TK2708`.
- `wu6a_review_nested_owner_merges.ts` pins `TS2322` x2 while nested legal
  function+namespace/class+namespace owners retain call/construct identity and mutable `.tag`
  properties.
- `wu6a_review_class_dependencies.ts` keeps annotated and lexically shadowed static initializers
  clean. Only the genuine unannotated root dependency carries
  `decl/class-declaration/namespace-payload-static-cycle` under owner `76`, alongside the class
  surface's legitimate `class/property-definition/initializer-inference`; a direct inspector proves
  the whole root terminal is `Unavailable` with no published storage type.
- `wu6a_review_ambient_export_alias.ts` proves the supported ambient value-alias surface:
  `TS2322` and `TS2339` pin the aliased public property type and private original spelling. Broader
  ambient external-module export semantics remain backlog `15`; an empty or prefix `Ready` object
  is never an alternative.
- `wu6a_review_using_legality.ts` pins private valid `using` as body-only/no-public-leak plus
  `TS2339`. Parsed namespace `await using` and ambient `using` are checker-owned as `TK2852` and
  `TK1545`; direct terminal tests cover both together with the valid private form. Oxc currently
  panics before producing a checker AST for `export using` and `export await using`, so `TK1491`
  and `TK1495` remain future parser-diagnostic translation gates and keep this combined fixture
  disabled. TypeScript also emits the ambient-initializer companion `TS1254`; that marker stays
  pinned in the disabled oracle rather than receiving an invented incomplete owner.
- `wu6a_review_qualified_missing_child.ts` pins `TS2694`; a direct inspector must prove that the
  surviving `good` member cannot publish a partial root or satisfy the structural downstream
  control.
- `wu6a_review_cross_space_forward/` and `wu6a_review_cross_space_reverse/` each pin `TS2322` x2.
  Cross-file interface+namespace and type-alias+namespace companions share one root in both input
  orders. The direct gate compares the root `SymbolId`, namespace-owned `ValueStorageId`, structural
  `TypeId`, and terminal state. It also inspects the actual post-finish `CheckerRecord` replay for
  four diagnostics, pinning exact tuples `(original_module_ordinal, source_start, event_ordinal,
  record_ordinal)` at both primary ordinal `0` and deferred ordinal `1`; the returned reports prove
  no replay suppression, deduplication, truncation, or hidden incomplete.

The second-review addendum adds five more disabled-at-authoring fixtures. Four are now enabled:

- `wu6a_review_alias_dependencies.ts` pins nested namespace and private callable export-alias
  dependencies, a writable ambient-const alias control matching tsc, and function-before-alias
  provenance with `TS2349`/`TS2351`.
- `wu6a_review_attached_const.ts` pins `TS2540` x2 for direct const properties attached to function
  and class owners, with mutable `let` controls.
- `wu6a_review_body_traversal.ts` pins `TS2304` plus `TS2322` x6 through block, loop, switch,
  try/finally, and expression-statement bodies inside a consumed standalone namespace fragment.
- `wu6a_review_static_cycle_events.ts` pins two exact static-cycle owners and their class-initializer
  records, plus an initializer-only ordinary-function/block-shadow control.

`wu6a_review_duplicate_legality.ts` remains disabled: typokat now withholds the root and emits its
exact backlog-18 incomplete for the third value declaration, but checker support for tsc's three
companion `TS2300` diagnostics is outside WU6A. No marker is hidden to enable it prematurely.

`wu6a_review_private_unsupported_declarations.ts` is the enabled soundness-review witness for
consumed standalone namespace bodies. Strict tsc keeps the private enum and `import =` alias off the
public value payload while the exported `ready` property remains usable. Typokat retains the exact
`decl/enum-declaration/self` and `decl/import-equals/self` completeness owners when it consumes that
fragment; publishing `Ready` may not silently discard either private declaration.

WU7 adds three enabled strict-tsc regression fixtures from the official-suite ratchet:

- `wu7_official_named_tuple.ts` erases fixed/rest tuple labels without losing element, rest, or
  call-argument checking; named optional members retain the existing owner-75 incomplete.
- `wu7_official_interface_validation.ts` pins method-owned `TK2411` and lets a complete own property
  reconcile inherited base/base conflicts without suppressing own-vs-base `TK2430` checks.
- `wu7_official_callable_relation.ts` pins fixed/rest contravariance in both directions, including
  target-rest absorption, aggregate moving prefix/suffix tails, and source-rest checks against every
  remaining target slot; two exact backlog-63 markers retain deliberate safe arity over-reports.

The project-order gate retains the existing module-local isolation and function/class owner controls
in `wu5_global_augmentation_forward/`, `wu5_global_augmentation_reverse/`,
`wu4_function_namespace_matrix.ts`, `wu4_class_namespace_matrix.ts`, `keep_pairs_forward.ts`, and
`keep_pairs_reverse.ts`; the adversarial addendum does not replace or weaken them. Ambient export
alias resolution, private-`using` non-leakage, unavailable-root publication, and event replay each
retain direct checker/binder inspection in addition to marker parity.

The directory contains 74 flat fixtures. Six older fixtures, the WU6A unavailable ledger,
`wu6a_review_using_legality.ts`, and `wu6a_review_duplicate_legality.ts` remain outside the admitted
slice, so the whole directory stays disabled; the conformance harness gates the other 65 flat fixtures explicitly through
`ENABLED_FIXTURES`, plus both two-file WU5 projects, both two-file WU6A projects, and both
`wu6a_review_cross_space_*` projects through `ENABLED_PROJECT_FIXTURES`. Together with all other
corpora, the harness currently covers 420 enabled source files. WU6 adds
`wu6_ambient_namespace_body_lookup.ts` and `wu6_local_array_heritage.ts`. WU5 adds
`global_augmentation.ts`, `global_missing_declare_negative.ts`,
`global_script_negative.ts`, `global_value_publication_deferred.ts`, `umd_export.d.ts`,
`umd_export_negatives.ts`, and `umd_export_nonmodule.ts`; its project fixtures are
`wu5_global_augmentation_forward/` and `wu5_global_augmentation_reverse/`. WU4 adds
`class_interface_conflicts.ts`, `class_interface_namespace_generics.ts`,
`class_interface_namespace_nominal.ts`, `class_interface_namespace_order_matrix.ts`,
`class_interface_namespace_orders.ts`, `class_interface_recursion.ts`, `degraded_chimera.ts`,
`interface_var_constructors.ts`, `keep_pairs_forward.ts`, `keep_pairs_reverse.ts`, and the seven
`wu4_*` focused fixtures above. The earlier WU3 slice remains:
`interface_conflicts.ts`, `interface_members.ts`, `interface_recursion.ts`, `namespace_forms.ts`,
`wu2_annotation_recovery.ts`, `wu2_checker_local_qualified_roots.ts`,
`wu2_interface_traversal.ts`, `wu2_qualified_contexts.ts`,
`wu3_cyclic_heritage_recovery.ts`, `wu3_deferred_heritage_identity.ts`,
`wu3_forward_reopening_qualified_leaf.ts`, `wu3_heritage_index_conflicts.ts`,
`wu3_independent_heritage_conflicts.ts`,
`wu3_merged_interface_recovery.ts`, `wu3_qualified_alias_chain.ts`,
`wu3_qualified_class_surfaces.ts`, `wu3_qualified_generic_leaves.ts`,
`wu3_sole_supplied_constraint_orders.ts`, and `wu3_unavailable_header_metadata.ts`. The new cyclic
heritage oracle pins `tsc 6.0.3 --strict --lib es5 --noEmit`: each interface binding owns one
source-ordered `TS2310`, an unresolved in-SCC heritage argument independently owns `TS2304`, and a
non-heritage generic recursive member remains clean. A singleton self-SCC diagnoses only its
self-edge fragments, while a multi-group mutual SCC diagnoses every reopening fragment. Direct,
chained, generic, and mutual alias-mediated heritage cycles follow the same alias-transparent rule
in both declaration orders; the generic self-cycle also retains its deterministic own/external
members. Other invalid-cycle downstream recovery is deliberately not an oracle because tsc exposes
partial members by declaration order. The selected
heritage/index oracle pins both base orders for inherited string/number indices, inherited
property-to-string-index and number-to-string-index constraints, own property/index overlays,
directional optional-required property overrides, compatible readonly/reverse-optional controls,
and legal accumulation of differing call signatures. Same-kind inherited and
own-overlay failures use header-owned `TS2430`; cross-family failures use `TS2411`/`TS2413` at the
derived header or exact own member occurrence, matching the tsc source site. The selected
deferred-identity oracle balances alpha-equivalent clean controls against unequal `TS2320`
controls for conditional, `keyof`, template, mapped, alias-instantiation, and indexed-access
generic method returns, plus duplicate-collapsing normalized intersection semantics. Per-base-pair
coalescing waits for the first semantic failure after an earlier raw-different/equal-normalized
property, in canonical property order. It also requires distinct same-shaped public classes to remain structurally
identical while private/protected declaring origins remain nominally unequal, and treats property
readonly/optional metadata as identity-bearing even when the read `TypeId` is equal. Cross-fragment
property conflicts likewise compare alias-normalized, alpha-generic, and distinct public-class
types semantically after publication, while a distinct private class origin owns `TS2717` at the
later property occurrence. The selected
generic-header oracle also requires alpha-equivalent nested generic binders in constraints and
defaults to remain clean, while a declaration pair with both unequal nested constraints and
defaults coalesces to one header-owned `TS2428` per declaration. The selected
WU2 fixtures contain successful qualified
leaves whose classification belongs to WU2 but whose lowering closes only with WU3; the gate also
retains their earlier diagnostics and incomplete records unchanged.

Two mixed WU2/WU3 fixtures remain outside this gate because their namespace surfaces cross into
WU5 ambient/global or broader qualified value/static publication: `qualified_diagnostics.ts` and
`wu2_topology_slot_edges.ts`. The ambient alias-list fixture is enabled with its export-specifier
unavailability records and omitted follow-on diagnostic cardinality explicitly owned by backlog
`63`. The selected type-only WU3
fixtures cover qualified generic-leaf arity/constraint checks and nested generic lowering,
qualified leaves through exported type-alias chains, and qualified forward-reopening leaf lowering.

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
| `sr_deferred_ledger/` | — (stays disabled) | pinned deferred under-reports: backlog `18` (duplicate function implementation recovery), `48` (implicit-any return), `51` (assignment-target evaluation order), `60` (fresh literals vs union targets), `62` (index-signature source parity), `66` (protected↔protected override compat), and `76` (unannotated forward `var` value type); plus the backlog `35` aliased-keyof mapped key-source and backlog `75` generic indexed-access **over**-reports |

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
  [`divergences.md`](../../docs/reference/divergences.md). The backlog-51 fixture additionally pins
  JavaScript assignment evaluation order: target-side flow effects precede the RHS, so strict tsc
  reports `TS2339` where typokat is currently silent.

## Rewrite-hotpath hardening corpus (sprint 2026-07-13)

`sr_rewrite_hotpath_wu0/` is the enabled WU0 acceptance corpus for
[`sprint-2026-07-13-rewrite-hotpath-hardening.md`](../../docs/archive/sprint-2026-07-13-rewrite-hotpath-hardening.md).
It pins computed-member receiver propagation, terminating recursive infer rewrites,
nested generic constraint/default rewriting, and rejected-overload depth parity.
WU1-WU3 jointly satisfied every fixture and enabled the directory; the overload probe
continues to guard its `tsc 6.0.3 --strict` parity while the call and evaluator paths change.

`sr_rewrite_hotpath_wu7/` is the enabled acceptance corpus for the deep-acyclic
traversal follow-up. Its shallow, topologically ordered alias chains prove that
generic constraint/default metadata reaches both auxiliary structural walkers
without relying on parser nesting. `InferRewrite` retains its private heap task/value stack and
fresh-binder/memo/SCC-taint policy; inference constraints now demand normalized types through the
semantic query coordinator. Direct arena tests carry the 10k+ host-stack regression because a
textual fixture would conflate the walker with parser/lowering depth and the CLI's
enlarged worker stack.

`sr_rewrite_hotpath_wu8/` is the enabled acceptance corpus for the mapped-value
rewrite hardening, archived with the
[`rewrite/hotpath sprint`](../../docs/archive/sprint-2026-07-13-rewrite-hotpath-hardening.md).
Its shallow, topologically ordered object aliases route `T[K]` through a concrete
mapped type without relying on parser nesting or generic function metadata. Its
second fixture pins `T[K]` in generic function constraints and defaults; WU8 fixed
the former stale-metadata false positive, matching strict `tsc 6.0.3`. Direct arena
tests carry the 10,005-deep acyclic public-evaluator spine because large source alias
chains also exercise generic substitution before the mapped rewrite starts.

## Semantic duplication corpus (sprint 2026-07-13)

`sr_semantic_duplication/` is the enabled acceptance corpus for
[`sprint-2026-07-13-semantic-duplication-layering.md`](../../docs/archive/sprint-2026-07-13-semantic-duplication-layering.md).
Its class-member fixture pins one-time reserved signature ownership: generic method
constraints/defaults and call-site instantiation; four static signature `TK2302`s plus
one body-local `TK2302`; one unresolved parameter/return/default diagnostic each;
first-incompatible method/constructor overload reporting with hidden implementation
signatures; and constructor parameter-property type reuse plus readonly enforcement.
The final pair deliberately preserves the current externally visible `void` type of an
unannotated class method. This over-reports when its numeric result is consumed and
under-reports when the result is assigned to `void`; both are ledgered to backlog `76`.
The directory stays enabled so member bodies must consume the reserved surfaces with
exact diagnostic cardinality.

`recursive_class_applications.ts` is the additional pre-WU1 architecture acceptance
fixture. It requires immutable class applications to survive inside object, array,
tuple, callback, union, intersection, deferred conditional/mapped, and indexed-access
positions; checks mutual class SCCs in both declaration orders; repeats the same demand
chain around clean and failing relations; directly relates regular, mutual, and
non-regular applications with wrong arguments in both source/target orders; and proves
`ExpandingBox<T[]>` recursion advances only one layer per demand. Private and protected
foreign-class pairs require `declaring_class` identity to survive projection instead of
flattening into public structural objects. Method constraint/default, constructor overload
and parameter-property paths share that graph. The final construction block pins one
unresolved parameter-property event, one overload failure, four distinct static-class-
parameter events, readonly enforcement, default-only method instantiation, callback
arguments, both conditional branches, and hidden constructor implementations, so
publication cannot hide or duplicate diagnostics. The remaining fixtures pin supported and
unsupported initializer construction, poison propagation, class application arity/defaults,
finite scheduling shapes, projection exhaustion, and overload-selection precedence. The enabled
`sr_semantic_duplication_project/` companion runs the cross-module opposite-order heritage case as
one project so dependency discovery cannot change ownership or output order.

## Surface-accounting corpus (sprint 2026-07-10)

`b73_surface_accounting/` is the WU0 acceptance spec for the completeness-accounting
sprint (backlog `73`, archived at `docs/archive/sprint-2026-07-10-completeness-accounting.md`).
Each fixture pins an **in-scope AST position the checker silently skips** — the child
slot / statement container / annotation form exits **clean** today while `tsc 6.0.3
--strict` rejects it. The skipped position carries an `incomplete[<role/surface/slot>]`
marker (the identity scheme is defined in [`tests/surface/README.md`](../surface/README.md));
a nearby **supported control** on a sibling line carries the ordinary `error[TK…]`
marker so the two paths are diffed side by side.

The corpus is a single **enabled** dir, filled milestone by milestone (WU3 expression
child slots, WU4 statement containers, WU5 annotation / signature / class-member
accounting): the harness diffs each fixture's `incomplete[<id>]` markers
(`compare_incomplete_output`) with the same per-line, exact-identity discipline as
`error[TK…]` — an unexpected record or a missing expected one fails. WU4 traverses the
try/catch/finally blocks through the existing block walker, so the try fixture now
carries the ordinary `error[TK…]` markers (with an `incomplete[…]` only for the
still-unmodeled catch parameter). WU5 records the incomplete surface for every
unsupported annotation/signature/class-member degradation before the error-type
fallback, so an unsupported annotation form is no longer false-clean. Every fixture
header records the exact `tsc 6.0.3 --strict` verdict and the `file:line` of the
wildcard/`None`/skip that drops the position.

| Fixture | WU | Skip site | Marker id | tsc verdict |
|---|---|---|---|---|
| `template_interpolation.ts` | WU3 | `infer_expr` `TemplateLiteral` arm records before `None` | `expr-infer/template-literal/interpolation` | TS2345 |
| `computed_key.ts` | WU3 | `infer_object_literal` records non-`static_name` keys | `expr-infer/object-literal/computed-key` | TS2345 |
| `array_spread.ts` | WU3 | array-element helper records spread/elision | `expr-infer/array-literal/spread-element` | TS2345 |
| `try_catch_finally.ts` | WU4 | `check_stmt` walks the blocks; catch param stays incomplete | `error[TK2322]` ×3 + `stmt-check/try-statement/catch-param` | TS2322 ×3 |
| `for_of_assign_target.ts` | WU4 | `declare_for_left` records the pre-declared target | `stmt-check/assignment-target/self` | TS2322 |
| `typeof_query.ts` | WU5 | `lower_annotation_inner` records before dropping `TSTypeQuery` | `annotation-lower/type-query/typeof` | TS2304 |
| `annotation_keywords.ts` | WU5 | remaining keyword/literal gaps record before `None`; `object` and `{}` retain distinct assignability | `annotation-lower/{symbol,bigint,intrinsic}-keyword/self`, `literal-type/bigint` | TS2304/TS2552 |
| `tuple_members.ts` | WU5/WU7 | named labels lower transparently; optional members remain recorded before withholding the tuple | `annotation-lower/tuple-optional-element/self` | (optional tuple unavailable) |
| `type_name_qualified.ts` | WU2/WU5 | successful public qualified type-group paths are supported; `this`/predicate type leaves remain recorded | `type-predicate/self`, `this-type/self` | clean qualified paths |
| `ambient_external_module.ts` | closure | a value-bearing string-literal ambient module stays explicitly deferred to backlog `15` | `decl/module-declaration/self` | clean |
| `signature_computed_key.ts` | WU5 | object/interface member collection records the computed key (property AND method signatures — WU7-E F1) | `signature/{property,method}-signature/computed-key` | TS1170/TS2304 |
| `class_members.ts` | WU5 | `collect_class_own_members` records static-block/accessor/index-sig/computed method + property keys (WU7-E F2) | `class/{static-block,accessor-property,class-index-signature}/self`, `class/{method,property}-definition/computed-key` | (member skip) |
| `class_heritage.ts` | WU5/WU7 | generic extends arguments are supported/traversed; nested unsupported syntax keeps its owner; implements remains record-only | `annotation-lower/type-query/typeof`, `class/implements-clause/self` | TS2304 |
| `assertion_compatibility.ts` | WU7 | assertions publish their asserted type but do not validate source/target overlap | `expr-infer/{as,type}-assertion/compatibility` | TS2352 ×2 |
| `assertion_compatibility_deferred_targets.ts` | WU7 review | deferred assignment targets find assertions across nested arrow/function/class syntax without entering those scopes semantically | `expr-infer/{as,type}-assertion/compatibility` ×6 | TS2352 ×3; valid controls clean |
| `assignment_expression_nested_scope_target.ts` | WU7 review | static/computed/private/wrapper and array/object destructuring targets containing an unreserved arrow/function/class keep one target-wide owner plus additive assertion/class owners and one RHS walk | `expr-infer/assignment-expression/nested-scope-target`, `expr-infer/{as,type}-assertion/compatibility`, `expr-infer/class-expression/self` | TS2304 ×2, TS2352; other controls clean |
| `update_expression_nested_scope_target.ts` | WU7 review | prefix/postfix updates across static/computed/private/wrapper targets keep one target-wide owner for an unreserved arrow/function/class plus additive assertion/class owners | `expr-infer/update-expression/nested-scope-target`, `expr-infer/{as,type}-assertion/compatibility`, `expr-infer/class-expression/self` | TS2352 ×4; other controls clean |
| `type_param_default.ts` | WU5 | record-only: type-parameter defaults never lowered (WU7-E F3; ledger `constraints/type-parameter-defaults`) | `annotation-lower/type-parameter-default/self` | TS2304 |
| `supported_annotations.ts` | WU5 (control) | keyof/mapped/conditional/template/readonly stay clean — no record | — | clean |

## Surface-accounting expression tail (sprint 2026-07-12)

`b73_expression_shape_tail/` is the disabled WU0 acceptance spec for closing backlog `73`.
It covers every expression variant that still reaches `infer_expr`'s wildcard at the planning
HEAD. Child-bearing wrappers require both their exact `incomplete[expr-infer/…/self]` identity and
any nested diagnostic reachable through an existing checker path. The update-expression fixture is
the deliberate exception: WU1 makes its operand traversal supported, so `Missing++` reports
`TK2304` while a routine numeric `for` update remains complete and clean. Semantic typing for all
wrappers remains with the backlog owners recorded in `tests/surface/inventory.toml`.

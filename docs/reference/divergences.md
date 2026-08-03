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
  "cannot find name"). Qualified namespace type paths are modeled; the remaining
  diagnosed ambient export-alias endpoint recovery is recorded separately below. M29
  temporarily maps a type-only import/export used as a value to `TK2304` instead of
  tsc's `TS2693`.
  <!-- div: id=names/value-used-as-type dir=under scope=s-value-type-space owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/m22_unresolved_type/positions.ts -->
  <!-- div: id=names/type-args-on-type-param dir=under scope=a-type-argument-arity owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/m24_generic_constraints/constraint_check_explicit.ts -->
  <!-- div: id=names/type-arg-count-on-builtin dir=under scope=a-type-argument-arity owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/m22_unresolved_type/generics.ts -->
  <!-- div: id=names/type-only-import-as-value-code dir=cosmetic scope=s-value-type-space owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/sr_wu2_export_space/type_only_export_leak/a.ts -->
- **An unsupported `typeof` query withholds its enclosing callable atomically
  (under-report).** This prevents partial signature publication, but the official
  `subtypingWithCallSignaturesA.ts` then loses tsc's downstream `TS2345` until value-side type
  queries are modeled.
  <!-- div: id=names/type-query-atomic-unavailable dir=under scope=s-value-type-space owner=../backlog/52-type-reference-tail.md witness=../../tooling/official-suite/scoreboard.txt -->
- **Multiple mismatched arguments (over-report).** On a call/`new` with several
  mismatched arguments, typokat reports a `TK2345` for **each**, whereas tsc stops at
  the first. Fixtures keep at most one mismatched argument per call so the corpus
  matches both.
  <!-- div: id=calls/multiple-mismatched-arguments dir=over scope=s-call-arguments owner=design-oos witness=../../tests/cases/m3_functions -->
- **A contextually typed argument is blamed at the argument (cosmetic).** When an argument that
  carries a contextual type — an arrow, or a fresh object/array literal — fails to match its
  parameter, typokat reports `TK2345` on the whole argument and names the inner mismatch in the
  reason chain; tsc 6.0.3 descends and reports `TS2322` on the offending sub-expression itself
  (an arrow's returned expression, a literal's property). Both reject the call. This is the
  dominant shape in the randomized differential corpus, where it is cancelled by an explicit
  allowlist rule rather than counted as a diff.
  <!-- div: id=calls/contextual-argument-blame-site dir=cosmetic scope=s-call-arguments owner=design-oos witness=../../tooling/differential/allowlist.txt -->
- **Spread call arguments remain explicitly unavailable (over-report / OOS).** The checker records
  `call/call-arguments/spread-argument` rather than dropping traversal or inventing an argument
  vector. Official `partiallyNamedTuples3.ts` therefore remains unsupported after its tuple label
  becomes transparent; backlog `71` owns spread traversal.
  <!-- div: id=calls/spread-argument-oos dir=over scope=b-iterability owner=../backlog/71-expression-inference-fn-tail.md witness=../../tooling/official-suite/scoreboard.txt -->
- **A fresh literal with both a wrong known property and an excess property
  over-reports.** typokat preserves the independent assignability and freshness
  diagnostics (`TK2322` plus `TK2353`), while tsc 6.0.3 gives the known-property
  mismatch precedence and reports only `TS2322`.
  <!-- div: id=objects/wrong-known-and-excess dir=over scope=s-excess-property owner=design-oos witness=../../tests/cases/m30_contextual_literals/excess_properties.ts -->
- **Excess keys reached through a ternary / logical over-report in count.** The
  excess-property walk descends into both ternary arms and both `||`/`??` operands
  (backlog `104`) and reports a `TK2353` for **every** excess key it finds, the same
  per-key rule a directly assigned literal already follows. tsc frames the whole
  expression as one `TS2322` whose elaboration stops at the first failing arm, so a
  ternary with an excess key in each arm is two diagnostics here and one there. Same
  verdict, same lines, more of them.
  <!-- div: id=objects/excess-descent-count dir=over scope=s-excess-property owner=design-oos witness=../../tests/cases/b104_excess_property_descent/ternary_arms.ts -->
- **A `||`/`??` right operand that tsc short-circuits away is still excess-checked
  (over-report).** An object literal is always truthy and never nullish, so when it is
  the *left* operand tsc drops the right one from the result type and never relates it
  — no excess check. typokat's excess walk is syntax-directed and has no operand types,
  so it checks both. Dead code only: the right operand of such an expression can never
  be evaluated. Making it precise means teaching the walk the short-circuit, which is
  the value model's job, not a second freshness rule.
  <!-- div: id=objects/excess-dead-logical-operand dir=over scope=s-excess-property owner=design-oos witness=../../tests/cases/b104_excess_property_descent/logical_operands.ts -->
- **A ternary/logical arm that tsc's union subtype reduction absorbs is still
  excess-checked (over-report).** tsc's excess check runs on the RELATION of the
  expression's value, and that value is a subtype-reduced union whose subtype test
  itself performs the excess check. So an arm whose excess key is admitted by a
  sibling arm's type (`flag ? { kind: "a", extra: 1 } : wide` with
  `wide: { kind: string; extra: number }`) is dropped from the union before anything
  is related, and tsc reports nothing; a sibling that does not admit the key absorbs
  nothing and both agree. typokat's excess walk is syntax-directed and has no operand
  types, so it reports either way. A false positive against tsc, in the over-reporting
  direction, on a literal that already names a key its annotation does not declare.
  Closing it means giving `Interner::union` tsc's subtype reduction — a change to the
  value model backlog `101` shipped, not to the freshness rule.
  <!-- div: id=objects/excess-absorbed-arm dir=over scope=s-excess-property owner=design-oos witness=../../tests/cases/b104_excess_property_descent/absorbed_arm_divergence.ts -->
- **Diagnosed ambient export-alias endpoints suppress qualified-use cascades
  (cosmetic).** After `TK2661` rejects an alias-only local name, typokat keeps the
  exported endpoint unavailable and omits tsc's follow-on `TS2694` at each use.
  <!-- div: id=namespaces/ambient-alias-use-cascade dir=cosmetic scope=b-namespaces owner=../backlog/63-review-parity-tail.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu2_ambient_export_alias_list.ts -->
- **The pinned ES5 `CallableFunction` and `NewableFunction` heritage edges
  over-report.** typokat emits one canonical `TK2430` for each interface where tsc is
  clean because apparent `Function` compatibility does not yet admit their generic
  overload surfaces. Backlog `14` owns the two compatibility results.
  <!-- div: id=lib-es5/callable-heritage-compatibility dir=over scope=s-assignability owner=../backlog/14-libdts-loading.md witness=../../tests/fixtures/lib-es5-6.0.3/readiness.toml -->
- **The same two ES5 heritage edges each emit one surplus `TK2430`
  (over-report).** The extra diagnostic per interface is a cardinality/parity issue
  distinct from the canonical compatibility result and remains backlog `63` work.
  <!-- div: id=lib-es5/callable-heritage-cardinality dir=over scope=s-assignability owner=../backlog/63-review-parity-tail.md witness=../../tests/fixtures/lib-es5-6.0.3/readiness.toml -->
- **The pinned ES5 annotation tail is explicitly incomplete (over-report).** The raw
  artifact records 173 backlog-`75` outcomes: polymorphic `this` ×164,
  `intrinsic` ×5, `symbol` ×3, and `bigint` ×1. These are non-permissive incomplete
  results rather than silent fallback, but tsc accepts the declarations.
  <!-- div: id=lib-es5/annotation-surface-tail dir=over scope=b-semantic-candidate-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/fixtures/lib-es5-6.0.3/readiness.toml -->
- **Newly traversed official surfaces expose missing standard-library globals
  (over-report / OOS movement).** Tuple-label and generic-heritage review now reaches
  `Error`, `Promise`, `Generator`, `AsyncGenerator`, `CloseEvent`, `Number`, `String`, `Object`,
  `Date`, and `Iterable` in files that tsc checks with a library. Typokat reports `TK2304` or an
  honest host-heritage incomplete; backlog `14` owns loading those declarations.
  <!-- div: id=lib/official-newly-reached-globals dir=over scope=design-oos owner=../backlog/14-libdts-loading.md witness=../../tooling/official-suite/scoreboard.txt -->
- **Library-local iterator constructor name is normalized (cosmetic).** In an external module,
  `IteratorObjectConstructor` is correctly absent from the global type space. TypeScript 6.0.3
  reports suggestion-bearing `TS2552`; typokat reports the ordinary unresolved-name `TK2304`.
  Both reject. This pins name visibility only and does not claim terminal iterator semantics.
  <!-- div: id=lib/iterator-object-constructor-name-code dir=cosmetic scope=s-value-type-space owner=../backlog/14-libdts-loading.md witness=../../tests/cases/b14_full_lib_loading/iterator_library_local_nonleak.ts -->
- **Absent `globalThis` property code is normalized (cosmetic).** In the cross-project isolation
  witness, TypeScript 6.0.3 reports `TS7017` for each unknown property on `typeof globalThis`;
  typokat uses its ordinary missing-member `TK2339`. Both reject the same leaked-property demand.
  <!-- div: id=lib/global-this-missing-property-code dir=cosmetic scope=s-value-type-space owner=../backlog/14-libdts-loading.md witness=../../tests/cases/b14_full_lib_loading_project/zz_shared_base_isolation/00_check.ts -->
- **Qualified enum endpoints remain unavailable (under-report).** Until enum types
  land, `E.Member` records
  `annotation-lower/type-name/qualified-enum` instead of guessing a type. Withholding
  the enclosing callable is non-permissive, but it suppresses tsc's downstream
  `TS2322` recovery diagnostic.
  <!-- div: id=enums/qualified-endpoint-unavailable dir=under scope=b-enums owner=../backlog/42-enums-type-side.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu2_annotation_recovery.ts -->
- **Enum/function/namespace diagnostics remain deferred (under-report).** The
  three-way recovery keeps the enum declaration explicitly incomplete and preserves
  namespace placement plus typed receiver-use errors, but does not yet emit tsc's
  `TS2567` at the enum/function declarations. Exact legality and recovery belong to
  backlog `42`.
  <!-- div: id=enums/function-namespace-ts2567 dir=under scope=b-enums owner=../backlog/42-enums-type-side.md witness=../../tests/cases/b43_namespaces_declaration_merging/degraded_chimera.ts -->
- **An exported enum in an attached namespace is unavailable (over-report).** The
  exact enum member records `decl/enum-declaration/namespace-payload-unavailable`
  and withholds the owner value where tsc publishes the member. Backlog `42` owns
  the enum value/type surface.
  <!-- div: id=enums/attached-namespace-payload dir=over scope=b-enums owner=../backlog/42-enums-type-side.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_namespace_payload_incomplete_ledger.ts -->
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
- **A forward call of an unannotated function/namespace merge is incomplete
  (over-report).** The merged callable cannot publish until its body-inferred return
  is final, so an earlier direct call records
  `expr-infer/call-expression/function-group-pending` where tsc resolves the return
  on demand and stays clean. Backlog `76` owns the lazy declaration query.
  <!-- div: id=namespaces/function-merge-forward-call dir=over scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_function_namespace_inferred_boundary.ts -->
- **An inferred-return cycle in a function/namespace merge lacks `TS7023`
  (under-report).** typokat records
  `decl/function-declaration/inferred-return-cycle` and withholds the callable instead
  of emitting the implicit-any cycle diagnostic. The incomplete outcome is
  non-permissive; exact demand resolution and the diagnostic remain backlog `76`/`48`
  work, with `76` owning this boundary.
  <!-- div: id=namespaces/function-merge-inferred-return-cycle dir=under scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_function_namespace_inferred_boundary.ts -->
- **A forward identifier demand of an unannotated function/namespace merge is
  incomplete (over-report).** A non-call reference before body completion records
  `expr-infer/identifier/function-group-pending`; tsc resolves the declaration type
  on demand and stays clean.
  <!-- div: id=namespaces/function-merge-forward-identifier dir=over scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_function_namespace_inferred_boundary.ts -->
- **An acyclic inferred-return dependency between function/namespace merges is
  incomplete (over-report).** The source declaration records
  `decl/function-declaration/inferred-return-dependency` instead of demanding the
  later declaration's return; tsc resolves the chain and stays clean.
  <!-- div: id=namespaces/function-merge-inferred-return-dependency dir=over scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_function_namespace_inferred_boundary.ts -->
- **Mutual inferred-return dependencies lack `TS7023` (under-report).** The same
  dependency record safely withholds both callable surfaces, but typokat does not yet
  emit tsc's implicit-any cycle diagnostic at either declaration.
  <!-- div: id=namespaces/function-merge-mutual-return-cycle dir=under scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_function_namespace_inferred_boundary.ts -->
- **An attached namespace variable with an inferred initializer is unavailable
  (over-report).** An exported variable whose initializer is outside the query-free
  literal subset records
  `decl/variable-declaration/namespace-payload-inferred-initializer`; tsc infers and
  publishes it. Lazy value resolution belongs to backlog `76`.
  <!-- div: id=namespaces/attached-inferred-initializer dir=over scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_namespace_payload_incomplete_ledger.ts -->
- **An attached namespace function with an inferred return is unavailable
  (over-report).** The exported function records
  `decl/function-declaration/namespace-payload-inferred-return`; tsc infers its body
  return and publishes it. Lazy value resolution belongs to backlog `76`.
  <!-- div: id=namespaces/attached-inferred-function-return dir=over scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_namespace_payload_incomplete_ledger.ts -->
- **An exported class in an attached namespace is unavailable (over-report).** Its
  exact declaration records `decl/class-declaration/namespace-payload-static-cycle`
  because the owner static surface cannot yet depend on the nested class publication;
  tsc publishes the class value. Lazy class/static dependency resolution remains backlog `76` work.
  <!-- div: id=namespaces/attached-class-static-cycle dir=over scope=b-namespaces owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_namespace_payload_incomplete_ledger.ts -->
- **An attached namespace import-equals value is unavailable (over-report).** The
  exact alias declaration records `decl/import-equals/namespace-payload-unavailable`
  and withholds the owner value where tsc publishes the forwarded member. Backlog
  `15` owns import/export semantics.
  <!-- div: id=namespaces/attached-import-equals-payload dir=over scope=b-namespaces owner=../backlog/15-modules-imports.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_namespace_payload_incomplete_ledger.ts -->
- **A later duplicate attached value lacks `TS2451` (under-report).** typokat attaches
  `decl/variable-declaration/namespace-payload-duplicate-value` to the exact later
  declaration and withholds the owner surface, but does not emit tsc's duplicate
  block-scoped-variable diagnostics. Backlog `18` owns them.
  <!-- div: id=namespaces/attached-duplicate-value-tk2451 dir=under scope=s-duplicate-declarations owner=../backlog/18-duplicate-identifier-detection.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_namespace_payload_incomplete_ledger.ts -->
- **A later attached function that duplicates another value lacks the duplicate-name
  diagnostics (under-report).** The exact function declaration records
  `decl/function-declaration/namespace-payload-duplicate-value` and withholds the owner
  surface, but typokat does not emit tsc's `TS2300`/`TS2451` family. Backlog `18`
  owns it.
  <!-- div: id=namespaces/attached-duplicate-function dir=under scope=s-duplicate-declarations owner=../backlog/18-duplicate-identifier-detection.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_namespace_payload_incomplete_ledger.ts -->
- **A later attached class that duplicates another value lacks the duplicate-name
  diagnostics (under-report).** The exact class declaration records
  `decl/class-declaration/namespace-payload-duplicate-value` and withholds the owner
  surface, but typokat does not emit tsc's `TS2300`/`TS2451` family. Backlog `18`
  owns it.
  <!-- div: id=namespaces/attached-duplicate-class dir=under scope=s-duplicate-declarations owner=../backlog/18-duplicate-identifier-detection.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu4_namespace_payload_incomplete_ledger.ts -->
- **A type depending on a deferred global value namespace is unavailable
  (cosmetic).** typokat records the exact backlog-82 global-augmentation incomplete
  outcome and withholds the dependent type instead of guessing from a same-named
  module-local namespace. Both downstream demands reject with `TK2339`; tsc reports
  `TS2322` for the known global property mismatch and `TS2339` for the module-local
  property, so only the first code differs.
  <!-- div: id=namespaces/deferred-global-value-dependency dir=cosmetic scope=b-namespaces owner=../backlog/82-declare-global-value-space.md witness=../../tests/cases/b43_namespaces_declaration_merging/global_value_publication_deferred.ts -->
- **`undefined` in assignment-target position (cosmetic).** typokat resolves
  `undefined` as a value read but not as an assignment target, so `undefined = null`
  reports `TK2304` where tsc reports `TS2539` — same verdict, different code.
  Surfaced by WU1's nested-assignment checking; owner backlog `47`.
  <!-- div: id=names/undefined-assignment-target dir=cosmetic scope=s-name-resolution owner=../backlog/47-definite-assignment.md witness=../../tests/cases/sr_wu1_expressions/nested_assignments.ts -->
- **Assignment targets containing a nested callable/class are explicitly incomplete
  (under-report).** Assignment-LHS reservation has no nested lexical owners, so typokat records
  `expr-infer/assignment-expression/nested-scope-target` instead of entering the scope with an
  incorrect binder or silently dropping member-write/destructuring semantics.
  <!-- div: id=assignments/nested-scope-target dir=under scope=s-assignability owner=../backlog/71-expression-inference-fn-tail.md witness=../../tests/cases/b73_surface_accounting/assignment_expression_nested_scope_target.ts -->
- **Update targets containing a nested callable/class are explicitly incomplete
  (over-report).** typokat records `expr-infer/update-expression/nested-scope-target` instead of
  entering an unreserved lexical owner. Numeric static/computed/private/wrapper prefix/postfix
  controls accepted by tsc therefore remain conservative, with assertion and class-expression
  outcomes retained independently.
  <!-- div: id=updates/nested-scope-target dir=over scope=b-semantic-candidate-tail owner=../backlog/71-expression-inference-fn-tail.md witness=../../tests/cases/b73_surface_accounting/update_expression_nested_scope_target.ts -->
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

### Frozen default-library prefix (backlog `102` / `103`)

On the `Library` base the library's binder tables are a sealed prefix a user delta may never
mutate ([ADR-0011](../decisions/0011-freeze-pinned-default-library-base.md)). Backlog `102` made
every attempted write visible to routing; backlog `103` now rebuilds the affected closure in a
private epoch. Fresh script-scope globals still publish through the ordinary delta. The remaining
rows below are duplicate-declaration diagnostics typokat does not yet emit, pinned by the
`b102_frozen_prefix_writes/` corpus and cross-checked against
`tsc 6.0.3 --strict --target es2025 --noEmit`.

- `TK2403` *subsequent variable declarations must have the same type* is not emitted when a
  script `var` redeclares a library global with a different type; the library declaration wins
  and only the resulting assignment errors are reported.
  <!-- div: id=library/var-redeclaration-type dir=under scope=s-duplicate-declarations owner=../backlog/103-library-merge-panics-and-routing.md witness=../../tests/cases/b102_frozen_prefix_writes/library_global_var_merge.ts -->
- `TK2451` *cannot redeclare block-scoped variable* is not emitted when a script `const`/`let`
  collides with a library value (tsc reports it on the library declarations too).
  <!-- div: id=library/const-redeclaration dir=under scope=s-duplicate-declarations owner=../backlog/103-library-merge-panics-and-routing.md witness=../../tests/cases/b102_frozen_prefix_writes/library_global_const_merge.ts -->
- `TK2300` *duplicate identifier* is not emitted when a script declaration collides with a
  library declaration in another declaration space.
  <!-- div: id=library/duplicate-identifier dir=under scope=s-duplicate-declarations owner=../backlog/103-library-merge-panics-and-routing.md witness=../../tests/cases/b102_frozen_prefix_writes/library_global_duplicate_identifier.ts -->
### Soundness-review deferred ledger (backlog `18`/`30`/`60`/`62`/`66`/`76`)

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
- **`30`** (Template literal types) and **`66`** (Classes)
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
- **Type-predicate and assertion signatures are explicitly incomplete
  (over-report).** The pinned ES5 artifact contains eight type-predicate annotations,
  each recorded as `annotation-lower/type-predicate/self` where tsc accepts the
  signature. Lowering predicate/assertion signature identity and its flow effect remains
  backlog `50`.
  <!-- div: id=narrowing/type-predicate-annotations dir=over scope=a-type-predicates owner=../backlog/50-type-predicates-assertions.md witness=../../tests/fixtures/lib-es5-6.0.3/readiness.toml -->
- **Type assertions do not validate source/target overlap (under-report).** Both `x as T` and
  `<T>x` publish `T` but currently skip tsc's `TS2352` compatibility check. The dedicated
  surface-accounting fixture prevents this from becoming a hidden false clean.
  <!-- div: id=assertions/source-target-overlap dir=under scope=b-semantic-candidate-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/b73_surface_accounting/assertion_compatibility.ts -->
- **Non-null assertion expressions remain explicitly unavailable (over-report / OOS).** The
  operand is still traversed, but `x!` records `expr-infer/non-null-assertion/self` rather than
  publishing a value with only `null | undefined` removed. Official `partiallyNamedTuples2.ts`
  exposes the same boundary after tuple-label support.
  <!-- div: id=assertions/non-null-expression-oos dir=over scope=a-nullish-receivers owner=../backlog/49-possibly-undefined-family.md witness=../../tooling/official-suite/scoreboard.txt -->
- **A logical expression's VALUE keeps a broad string or number left operand (over-report).**
  `a && b` is `falsy-part-of(a) | b` and `a || b` is `truthy-part-of(a) | b`, and both splits
  reuse the narrowing engine's `NarrowOp::Truthy`. Boolean now splits precisely to `true` or
  `false`; broad `string` and `number` deliberately remain unsplit. tsc's
  `extractDefinitelyFalsyTypes` reduces those to `""` and `0`, so typokat's result is a safe
  **superset**: identical verdict at any target admitting the whole primitive, over-reporting
  only at a target that admits just the falsy remainder (`const x: "" | number = s && n`).
  `||` has the mirror gap for a definitely-falsy string or number left operand, with no clean
  witness — tsc rejects those operands with its own `TS2873`. One truthiness model, not two;
  making it precise is the same change as making `if (s)` split `string` into `""`/`string`.
  <!-- div: id=narrowing/logical-value-falsy-split dir=over scope=a-narrowing-tail owner=../backlog/51-narrowing-tail.md witness=../../tests/cases/b101_conditional_logical_values/falsy_split_divergence.ts -->
- **Deferred:** `for`/`for-of`/`do-while` loop narrowing and narrowing seen by a
  **closure** over a never-reassigned binding (tsc narrows; typokat keeps the
  function-boundary reset — over-report, safe direction). Member-path narrowing
  (`x.a`) remains symbol-keyed. Backlog `51` owns these flow forms.
  <!-- div: id=narrowing/deferred-forms dir=over scope=a-narrowing-tail owner=../backlog/51-narrowing-tail.md witness=../../tests/cases/m23_unstructured_narrowing -->
- **A `for` body and the flow after a `do…while` read the declared type (over-report).**
  `build_flow_stmt` models no `for`/`for-in`/`for-of`/`do…while` edges, so a reference in
  those positions never reaches `reference_flow` — the guard in the loop test narrows
  nothing, composed or not. Same owner (`51`) as the deferred forms above; pinned
  separately because the composed-condition corpus carries the fixture that fails on it.
  <!-- div: id=narrowing/unmodeled-loop-condition-flow dir=over scope=a-narrowing-tail owner=../backlog/51-narrowing-tail.md witness=../../tests/cases/b100_logical_condition_narrowing/unmodeled_loop_flow_deferred.ts -->
- **A redundant guard over an unassigned variable loses tsc's member-error cascade
  (under-report).** tsc reports the owned `TS2454` definite-assignment errors and keeps the
  alternate wide enough to reject `x.toFixed`; typokat does not yet model definite assignment,
  narrows the alternate to `number`, and loses that follow-on `TS2339` too. Backlog `47` owns
  the root check and its recovery state.
  <!-- div: id=flow/unassigned-redundant-guard-cascade dir=under scope=a-definite-assignment owner=../backlog/47-definite-assignment.md witness=../../tests/cases/b100_logical_condition_narrowing/official_guard_parity_deferred.ts -->
- **An assignment inside a nested `||` can miss tsc's `never` alternate (under-report).** In
  the official assigned-operand shape, tsc narrows the final alternate to `never` and rejects
  its member access; typokat retains the assigned boolean path and stays silent. Backlog `51`
  owns the missing assignment-sensitive flow composition.
  <!-- div: id=narrowing/assigned-or-never-alternate dir=under scope=a-narrowing-tail owner=../backlog/51-narrowing-tail.md witness=../../tests/cases/b100_logical_condition_narrowing/official_guard_parity_deferred.ts -->
- **Accepted official-suite over-reports** (safe direction, recorded in the scoreboard;
  independently audited — matched never drops, fn never rises): walking `while` bodies / ternary
  arms / logical RHS surfaces lib-shaped `TK2339` (`.length`/`.toString`/… on correctly-narrowed
  primitives — no `lib.d.ts`) in `controlFlowIteration*`, `typeGuardsIn{If,ConditionalExpression}`,
  `typeGuardsOnClassProperty`, and `…RightOperandOfAndAndOperator`; plus `TK2345`
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

Implemented: fields/constructor/methods/`this`/`new`/structural instances (M11); inheritance,
including local plain-identifier generic heritage applications (M12/M16); access modifiers +
`static` (M13); member-assignment + `readonly` (M14); getters/setters + `abstract` (M15); generic
classes (M16); override compatibility (`TK2416`) and abstract-member
completeness (`TK2515`/`TK2654`) (b06); private/protected constructor accessibility (`TK2673`/
`TK2674`) (b20).

- **Unsupported unannotated class-field initializer inference (over-report).** The atomic class
  surface pass accepts a deliberately bounded pure-expression subset. Other unannotated
  initializers record an incomplete outcome and poison the class where tsc can infer their types.
  <!-- div: id=classes/unannotated-field-initializer-inference dir=over scope=b-semantic-candidate-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/sr_semantic_duplication/class_initializer_unsupported.ts -->
- **Source class type-parameter defaults (over-report).** A class application that needs a
  source-authored default records a typed incomplete outcome where tsc substitutes the default.
  Explicit complete type-argument vectors remain supported.
  <!-- div: id=classes/source-type-parameter-defaults dir=over scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/sr_semantic_duplication/class_application_contract.ts -->
- **Unavailable generic-constructor inference (over-report).** When constructor candidates cannot
  provide every class type argument, typokat records an incomplete outcome instead of constructing
  a partial or recovery-filled application; tsc accepts the corresponding unresolved inference.
  <!-- div: id=classes/generic-constructor-inference-unavailable dir=over scope=b-semantic-candidate-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/sr_semantic_duplication/class_application_contract.ts -->
- **Recursive class-projection frontier (over-report).** typokat conservatively rejects a semantic
  query after 128 distinct class applications and records its projection-budget outcome; tsc's
  recursive cutoff accepts some infinitely matching structural pairs.
  <!-- div: id=classes/recursive-projection-budget dir=over scope=s-assignability owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/sr_semantic_duplication/class_projection_exhaustion.ts -->
- **Class index signatures remain explicitly unavailable (over-report / OOS).** Interface and
  object-type index signatures are represented, but a class index signature records
  `class/class-index-signature/self`. The official numeric/string indexer files and the two
  `subtypesOfTypeParameterWithConstraints` files retain that boundary while preserving their
  independent diagnostic diff.
  <!-- div: id=classes/class-index-signature-oos dir=over scope=b-indexed-access-diagnostics owner=../backlog/75-scope-surface-tail.md witness=../../tooling/official-suite/scoreboard.txt -->
- **Non-object interface heritage topology is explicitly unavailable (over-report / OOS).** A
  local interface extending a tuple alias records `interface/heritage/topology` instead of
  publishing a partial base; `contextualTypeWithTuple.ts` is the exact official witness. The
  analogous implicit-`Array` case is a standard-library dependency owned by backlog `14`.
  <!-- div: id=interfaces/tuple-alias-heritage-topology dir=over scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tooling/official-suite/scoreboard.txt -->

- **Unannotated class-method returns are externally `void`.** Class fill publishes an omitted method
  return annotation as `void`; source-order body checking does not replace that public member type.
  Exact demand-driven value types belong to backlog `76`, and the reserved-surface refactor preserves
  this boundary:
  - Consuming a body-inferred numeric result as `number` reports `TK2322` where tsc is clean
    (over-report).
    <!-- div: id=classes/unannotated-method-return-number dir=over scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/sr_semantic_duplication/class_member_surfaces.ts -->
  - Assigning that result to `void` is clean where tsc reports `TS2322` (under-report).
    <!-- div: id=classes/unannotated-method-return-void dir=under scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tests/cases/sr_semantic_duplication/class_member_surfaces.ts -->
  - Official unannotated getter/static-method controls expose safe cascades: arrow/null getters
    report `TK2349`, and static methods returning `this` report `TK2351` at construction. Exact
    getter/method return publication belongs to backlog `76`; after that, backlog `49` owns the
    strict-null getter call's `TK2721`.
    <!-- div: id=classes/unannotated-accessor-static-return-cascade dir=over scope=s-declaration-hoisting owner=../backlog/76-lazy-value-type-resolution.md witness=../../tooling/official-suite/scoreboard.txt -->

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
  - **Generic-base override validation is skipped** — although represented generic heritage
    composition is supported, an override against a generic base / from within a generic class may
    carry a free type parameter where the bespoke override relation would over-report. Relatedly,
    `TK2515`/`TK2654` render a generic direct base as its
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
- **Tuple labels are transparent, but some valid rest containers are not provably array-like
  (over-report / OOS).** Conditional and mapped containers in official
  `partiallyNamedTuples{,2}.ts` retain `annotation-lower/tuple-rest-element/non-array`; the tuple
  is withheld rather than published with an invented rest shape.
  <!-- div: id=tuples/rest-container-proof-oos dir=over scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tooling/official-suite/scoreboard.txt -->
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
  listeners that tsc accepts (over-report).** This was exposed first by contextual callback typing
  and again when named event tuples became transparent in official
  `dependentDestructuredVariables.ts`; the callback still sees the whole tuple union. The
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

Implemented (M28): production resolves the standard aliases (Partial, Required, Readonly, Record,
Pick, Omit, Exclude, Extract, NonNullable, ReturnType, ThisParameterType, and
OmitThisParameter), `console`, and `Math` from the pinned TypeScript 6.0.3 default library.
`OmitThisParameter` uses a trusted intrinsic specialization to preserve represented function
parameter shape; the other aliases use the ordinary mapped/conditional machinery. A user
redeclaration follows the library merge/shadow rules. Raw checker unit tests use the separate
test-only `crates/typokat-check/src/check/test_support_prelude.ts`; it is not a production library
route. `keyof <pending computation>` is a **deferred keyof** node
evaluated on demand (identical-node-only while deferred: rejects e.g. `x: T` against `keyof T`,
matching tsc). Uppercase/Lowercase/Capitalize/Uncapitalize are evaluator intrinsics on string
literals (distributing over unions; Rust char-wise case mapping — agrees with JS for the corpus,
including multi-char expansions like `ß` → `"SS"`).

- **Out of scope:** `Parameters`/`ConstructorParameters`,
  `InstanceType`, `Awaited`, `NoInfer`, and the `intrinsic` keyword outside the four string aliases
  and `ThisType`/`OmitThisParameter` (a
  user `= intrinsic` alias silently degrades to the error type).
  <!-- div: id=utility/unsupported-aliases dir=over scope=design-oos owner=design-oos witness=../../tests/cases/m28_utility_types -->
  <!-- div: id=utility/intrinsic-degradation dir=under scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/m28_utility_types -->
- **Documented divergences:**
  - The test-support `ReturnType` uses a strict/sound `(...args: never[]) => infer R` match, so it handles
    non-nullary and rest functions without introducing the lib's permissive `any[]` constraint.
    Its modeled `(...args: never[]) => unknown` constraint is enforced through the shared alias
    constraint path, so non-callables report `TK2344` without introducing `any`.
  - A **symbolic** intrinsic application (`Uppercase<S>` over a pattern/`string`/free param)
    relates conservatively — assignable to `string` (and an identical node) only, nothing flows
    INTO it — rejecting values tsc's string-mapping algebra accepts (over-report; witnessed by the
    official suite's `stringMapping*` files).
    <!-- div: id=utility/symbolic-intrinsic-conservative dir=over scope=b-type-level-tail owner=design-oos witness=../../tooling/official-suite/scoreboard.txt -->
  - A standalone `ThisType<T>` marker is deliberately not structurally equivalent to `{}`:
    only a `ThisType<T>` member of a contextual intersection is transparent and supplies the
    object-literal method receiver. Standalone uses can therefore over-report versus tsc.
    <!-- div: id=utility/standalone-this-type-marker dir=over scope=b-this-parameters owner=design-oos witness=../../tests/cases/b70_this_parameter_typing/this_type_context.ts -->
  - For a preserved generic receiver signature, a bad bare `OmitThisParameter` call reports
    `TK2684` from receiver checking where tsc reports `TS2345` from its argument candidate.
    Both reject the call; this is diagnostic priority only.
    <!-- div: id=utility/omit-this-parameter-generic-bare-call-code dir=cosmetic scope=b-this-parameters owner=design-oos witness=../../tests/cases/b70_this_parameter_typing/this_utilities.ts -->
  - `OmitThisParameter` correctly retains a union when a generic member's effective receiver is
    `unknown`, but invoking that union is silent: callable-signature selection does not yet model
    unions, so typokat misses tsc's `TS2349`. The callability diagnostic and union support belong
    to backlog `19`.
    <!-- div: id=calls/union-callability-after-omit-this-parameter dir=under scope=a-noncallable owner=../backlog/19-call-of-non-callable-diagnostic.md witness=../../tests/cases/b70_this_parameter_typing/this_utilities.ts -->
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
  - `infer X extends C` (TS 4.7) is withheld at annotation lowering, including when
    `C` is array-like (backlog `75`).
    <!-- div: id=conditional/infer-extends-constraint dir=over scope=b-type-level-tail owner=../backlog/75-scope-surface-tail.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu7_official_named_tuple.ts -->
  - Rest-based conditional `infer` is implemented for fixed tuple/function rest patterns, but a
    variadic source tuple such as `Tail<[string, ...number[]]>` is still a safe-direction
    over-report (tracked in backlog `69`).
    <!-- div: id=conditional/variadic-source-tuple dir=over scope=b-type-level-tail owner=../backlog/69-signature-rest-parity-tail.md witness=../../tests/cases/b57_tuple_array_infer -->
  - A deferred conditional whose branches still contain its own `infer` binders is conservatively
    non-assignable (over-report, safe).
    <!-- div: id=conditional/deferred-branch-infer dir=over scope=b-type-level-tail owner=design-oos witness=../../tests/cases/m25_conditional_types -->
  - A nested conditional referencing an OUTER conditional's `infer` binder is **poisoned at
    lowering**. A definitive primitive-versus-`object` rejection may select its false branch through
    the guarded evaluator lifecycle; captured-infer true branches remain unavailable and are
    conservatively related (tsc resolves them; over-report pinned in `nested_infer.ts` — proper de
    Bruijn shifting is backlog `26`).
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
  (Backlog `14`, `15`, `38`, `52`.)
  <!-- div: id=modules/out-of-scope-resolution dir=over scope=design-oos owner=../backlog/15-modules-imports.md witness=../../tests/cases/m29_modules -->

The 1.0 plan narrows that deferred resolver surface to `moduleResolution: "bundler"` and delegates
physical resolution to `oxc_resolver` ([ADR-0007](../decisions/0007-bundler-resolution-via-oxc-resolver.md)).
Typokat retains source-root accounting, module graph/import/export semantics, `.d.ts` checking,
diagnostics, and determinism. Until backlog `15` lands, this is policy rather than implemented
coverage. NodeNext/alternate profiles and known dependency gaps such as simplified `typesVersions`
selection must remain explicit unsupported outcomes; they may not disappear behind the M29 local
resolver or an error-type fallback.

## Intersection types (M31)

Implemented (M31): an interned, canonicalized member-set node — the dual of union. Target
intersection requires the value to satisfy **every** member; a source intersection relates through
its **merged apparent object**. Member access, excess-property checking (against the merged key
set), contextual fresh-literal shaping, and the M24 circular-constraint walk (`T extends T & X` →
`TK2313`) all see the merge.

- **Documented divergences:**
  - `&` is **not generally distributed** over unions (`(A | B) & C`). The narrow,
    structurally decidable empty-domain proof for finite primitive/literal/`object`-keyword union
    members is implemented, including `(string | number) & boolean`. Broader distribution remains
    safe-direction out of scope.
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

## Object relation — which failure is reported

- **A missing required property loses to a value mismatch that sorts earlier, and the winner can
  depend on statement order.** `relate_objects` walks the target's properties in canonical name
  order and relates each value as it goes, so the first failing property becomes the headline.
  `tsc` runs `getUnmatchedProperty` over **all** required target properties before any value
  relation, so a genuinely missing property always wins. Repro (`tsc 6.0.3 --strict` reports
  `TS2741: Property 'p1' is missing in type 'N2' but required in type 'N0'`):

  ```ts
  interface N0 { p0?: N2 & N0; p1: N0; }
  interface N1 { p0: N1; p1: N2 | N0; }
  interface N2 { p0: N1; }
  declare const a: N1;
  declare const b: N2;
  const x: N0 = a;
  const y: N0 = b;   // tsc TS2741 · typokat TK2322 "Types of property 'p0' are incompatible."
  ```

  Deleting `const x`, or swapping the two declarations, restores `TK2741` — `const x` warms the
  relation cache with the `false` that makes `p0` fail for `const y`. The verdict never moves
  (`N2 <: N0` is false either way) and the span and line are correct; only the reported cause
  moves, and it moves because of a logically independent statement. The ordering dependence was
  always latent — before [ADR-0016](../decisions/0016-reason-free-relation-probes.md) made a
  cached `false` authoritative, the cache-vs-cycle interaction happened to let `p0` succeed here,
  so `TK2741` fell out by accident rather than from a presence rule. Backlog `91` adds the
  presence pass and deletes this entry.
  <!-- div: id=objects/missing-property-vs-value-order dir=cosmetic scope=s-assignability owner=../backlog/91-missing-property-presence-pass.md witness=../../tests/cases/b91_missing_property_presence -->

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

- **Mixed overload failure diagnostic (cosmetic).** When one call/construct candidate has a type
  mismatch and a later candidate fails arity, typokat reports `TK2769` on the whole invocation;
  tsc 6.0.3 reports `TS2345` on the mismatched argument. Both reject the program.
  <!-- div: id=signatures/mixed-arity-mismatch-diagnostic dir=cosmetic scope=s-overload-resolution owner=design-oos witness=../../tests/cases/sr_semantic_duplication/selector_precedence.ts -->
- **Explicit constraint failure selection (cosmetic).** If every call or construct overload rejects
  the same explicit type argument by constraint, typokat preserves the first candidate's `TK2344`
  text while tsc 6.0.3 renders the last overload's constraint. The code and rejection are identical.
  <!-- div: id=signatures/explicit-constraint-first-failure dir=cosmetic scope=s-overload-resolution owner=design-oos witness=../../tests/cases/sr_semantic_duplication/selector_precedence.ts -->
- **Two callable-rest arity boundaries remain conservative (over-report).** Unlike strict tsc
  6.0.3, typokat rejects a source with a moving required tuple suffix against a zero-parameter
  target, and rejects a required-prefix-plus-rest source against a single optional target. Both
  reject in the safe direction; backlog `63` owns their parity tail independently of WU7's
  dropped-error fixes for moving target/source suffixes.
  <!-- div: id=signatures/rest-arity-conservative dir=over scope=s-assignability owner=../backlog/63-review-parity-tail.md witness=../../tests/cases/b43_namespaces_declaration_merging/wu7_official_callable_relation.ts -->
- **Contextual generic signature instantiation is not modeled (over-report).** Alpha-aligned
  generic signatures are cache-safe and persistent, but official
  `callSignatureAssignabilityInInheritance4.ts` needs query-local contextual instantiation for
  members `a6`, `a11`, `a15`, and `a18`. Typokat emits four `TK2430` records; `a17` is the clean
  control. Backlog `83` owns the non-cacheable relation trial.
  <!-- div: id=signatures/contextual-generic-instantiation dir=over scope=s-assignability owner=../backlog/83-contextual-generic-signature-relation.md witness=../../tooling/official-suite/scoreboard.txt -->
- **Aggregate variadic-call diagnostics have two conservative identity/cardinality errors.** In
  official `variadicTuples2.ts`, harness line 66 reports `TK2555` instead of one aggregate
  `TK2345`, and line 71 reports the expected `TK2345` plus a duplicate. Both calls reject; backlog
  `69` owns exact tuple-rest call reporting independently of label lowering.
  <!-- div: id=signatures/variadic-call-diagnostic-cardinality dir=over scope=s-call-arguments owner=../backlog/69-signature-rest-parity-tail.md witness=../../tooling/official-suite/scoreboard.txt -->

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

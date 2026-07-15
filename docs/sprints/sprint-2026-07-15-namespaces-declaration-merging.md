# Sprint — namespace and declaration-space completion (2026-07-15)

**Goal.** Implement type-side namespaces and legal declaration merging with stable,
order-independent publication; close every checker surface owned by backlog `43`; and prove that
the pinned TypeScript 6.0.3 `lib.es5.d.ts` has no remaining namespace/declaration-merging blocker.

**Outcome boundary.** This sprint unblocks backlog `14`; it does not automatically load the
standard library. Full `lib.d.ts` loading, the frozen base plus per-file delta `Store`, and
parallelism Stage 1 remain the following sprint. The final `lib.es5.d.ts` gate checks the pinned
source as an explicit input through the existing pipeline and produces a GO/NO-GO handoff for
`14`, not a claim of ambient standard-library support.

**Why this scope.** Backlog `43` is the sole remaining audited `lib.es5.d.ts` model prerequisite.
Combining it with backlog `14` would couple two XL, independently reviewable risk domains:
declaration identity/publication and shared-prelude storage. Enums (`42`) and
`satisfies`/`as const` (`44`) remain release blockers but have zero uses in the pinned ES5 core and
do not belong on this critical path.

## Refs re-verified at HEAD (2026-07-15, `1836d37`)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `Symbol` already has distinct value/type/namespace slots, but the namespace slot remains
  unused — `src/binder/symbol.rs:30-51`.
- ✔ The scope graph has module/function/block scopes only; namespace scope ownership is not yet
  represented — `src/binder/scope.rs:20-38`.
- ⚠ Type binding allocates a fresh declaration for every interface but `declare_type` retains only
  the last same-name `DeclId`, so repeated interfaces overwrite instead of merge —
  `src/binder/bind.rs:305-348`, `src/binder/bind.rs:810-823`.
- ⚠ Type reservation asks the binder for the surviving same-name declaration and `place_type_decl`
  replaces that slot, so the earlier interface body is not published —
  `src/check/checker/decls/mod.rs:410-447`, `src/check/checker/decls/mod.rs:571-635`.
- ✔ Qualified type names currently emit the owned incomplete record and return no type —
  `src/check/checker/decls/resolve.rs:155-177`.
- ✔ Namespace/module, global augmentation, and UMD namespace-export statements currently emit the
  `43`-owned incomplete records — `src/check/checker/statements.rs:216-273`,
  `tests/surface/inventory.toml:108-115`, `tests/surface/inventory.toml:1301-1357`.
- ✔ The pinned TS 6.0.3 ES5 audit records 28 interface+var pairs, repeated
  `Date`/`Number`/`String` interfaces, and `declare namespace Intl`; enum and
  `satisfies`/`as const` uses remain zero — `docs/backlog/lib-audit-6.0.3.md:10-64`.
- ✔ The binding invariants still require complete immutable type/class publication, transactional
  semantic queries, and lexical event ownership; no `43` implementation may bypass them —
  `docs/reference/invariants.md:9-46`.

## Binding architecture and method

- Preserve the multi-slot symbol architecture: one symbol may carry independent value, type, and
  namespace meanings. Do not introduce a parallel namespace resolver or a special global-name path.
- The binder owns declaration identity, source order, merge groups, namespace scopes, and exported
  namespace membership. The checker publishes one complete merged semantic surface from that
  declaration graph.
- A published hash-consed `TypeId` is never mutated into a different identity. Recursive merge
  groups reserve identity and publish atomically; no lookup, relation, or cache may observe a
  partial surface.
- Preserve the semantic-query coordinator, typed exhaustion, immutable class publication, retained
  callable rows, and checker-wide lexical `EventStore` ordering from
  [`invariants.md`](../reference/invariants.md).
- Follow the mandatory spec-first loop from [`dev-method.md`](../reference/dev-method.md): commit
  the disabled acceptance corpus separately, delegate implementation, obtain an independent
  adversarial review against strict `tsc`, and let the leader verify and commit.
- Soundness wins over completeness. Any deliberate safe-direction difference is documented; a
  false negative is a stop condition.

## Scope matrix

| Surface | Required behavior | Explicit boundary |
|---|---|---|
| Repeated interfaces | One stable merged identity; members, heritage, call/construct/index signatures, and overload groups follow strict-tsc rules | Same-declaration duplicate-member families remain backlog `18`; cross-declaration conflicts belong here |
| Namespace declarations | Identifier, nested, dotted, reopened, and ambient `declare namespace` forms bind into namespace scopes | Runtime emit/transformation is out of scope |
| Qualified types | Resolve `A.B.C` segment by segment with correct shadowing, export visibility, type arguments, `TK2503`, and `TK2694` | String-literal external modules remain backlog `15` |
| Cross-space coexistence | Value/type/namespace slots resolve independently; all 28 ES5 interface+`declare var` constructor pairs see the final merged interface | Coexistence is not misreported as interface merging |
| Keep-pair merging | Interface+namespace, function+namespace, class+namespace, and the approved class+interface(+namespace) composition preserve their instance/type/static/container surfaces in every strict-tsc-legal order | Exact enum+namespace+function legality/recovery remains explicitly deferred to `42`; the interim receiver is typed incomplete and non-permissive |
| Namespace visibility | Exported members form the reopened public namespace surface; non-exported members remain local to their declaration block | No accidental sharing of private block members across reopenings |
| Global/UMD surface | In WU5, atomically link/publish legal `declare global` blocks into one compilation-global scope; implement `TK2669` and `TK1314`/`TK1315` context errors; keep valid `export as namespace` publication owned by `15`, with the current `export =` form as its WU0 witness | Keep WU1b global metadata disconnected, keep module locals isolated, and do not absorb valid UMD publication or general package/module loading; backlog `15` must cover both `export =` and ordinary named-export UMD surfaces |

**Approved scope addendum (2026-07-15).** Backlog `43` includes full
class+interface(+namespace) merging even though architecture §4.1's original keep-list did not name
class+interface. The class remains the identity/value owner: construction, existing statics,
private nominal origin, generic application, and atomic publication must survive while interface
instance members and namespace static/container members compose in either class/interface order.

## Work units

### WU0 — strict-tsc oracle corpus and baseline (effort L; spec first)

- **Verify first.** Pin `tsc 6.0.3`; measure the current focused conformance and official-suite
  namespace slice; inventory every `43`-owned incomplete record; and cross-check ambiguous merge,
  visibility, generic, conflict, and diagnostic cases against `tsc --strict`.
- **Scope.** Add a disabled acceptance corpus covering both declaration orders; interface property,
  method, call, construct, and index members; overload ordering; generic parameter
  constraint/default compatibility; recursive and mutually recursive merges; namespace reopening;
  nested and dotted syntax; exported versus non-exported members; qualified type arguments;
  shadowing across all three slots; every approved keep-pair; interface+var constructor patterns;
  ambient/global/UMD forms; missing root/member diagnostics; and explicitly backlog-42-deferred
  chimeras with typed incomplete, non-permissive interim behavior.
- **Acceptance.** The corpus is committed without production changes or registration. Expected
  codes, spans, order, and deliberate differences are recorded. The official-suite measurement
  names the exact candidate files and makes no unmeasured progress promise.
- **Review.** A read-only agent independent of production work hunts missing false-negative cases
  and repeats the strict-tsc probes before the spec commit.
- **Touch points.** A new `tests/cases/` directory, `tests/cases/README.md`, focused direct tests if
  marker output cannot prove identity/order, and this sprint run log only.

### WU0A — merged-declaration publication decision (effort M; architecture gate)

- **Problem.** The live binder has three symbol slots, but `declare_type` overwrites the previous
  type declaration and the checker reserves/fills the surviving interface declaration. Namespace
  scopes and member exports do not exist. Extending this with ad-hoc vectors or mutable completed
  objects would make identity and diagnostics order-dependent.
- **Scope.** Record the selected representation for ordered merge groups, namespace scope/public
  member identity, per-block private membership, type-parameter compatibility, reserve/fill
  ownership, complete publication, poison/exhaustion, and lexical event ownership. Distinguish
  symbol-slot coexistence from semantic composition. Decide the exact boundary between identifier
  namespaces, global augmentation, UMD namespace exports, and string-literal external modules.
- **Hard stop.** Do not edit production code until an architecture reviewer confirms the design
  fits the existing multi-slot symbols and immutable type/class/query invariants. If it requires a
  new module boundary, mutable interned identity, or shared-prelude storage, request approval.
- **Acceptance.** An ADR (or an explicit decision section here if no durable new decision is
  needed) is independently reviewed, indexed, and committed before WU1.

### WU1a — lexical identity and ordered type-group substrate (effort M)

- Give every source declaration a unique `DeclId` from one unified lexical identity space, exact AST
  site, and lexical event owner. Distinguish it from dedicated value-storage identity and from
  `TypeGroupId`.
- Build one stable dormant `TypeGroupId` containing every admitted fragment in source order; never
  retain only the last declaration or reconstruct order during checking.
- Land metadata only. Do not switch `Symbol.ty`, production lookup, reservation, publication, or any
  semantic consumer in WU1a.
- Add binder-level direct tests for non-aliasing lexical/value-storage/group identities, distinct
  declaration identity, shared group identity, exact AST ownership, source order, declaration/event
  ticket ownership, and unchanged production `Symbol.ty` behavior.

### WU1b — namespace binding and publication substrate (effort L)

- Activate the namespace slot with one stable `NamespaceId`, one shared public scope, and a distinct
  private block scope for each reopening, without resolving types in the binder.
- Bind nested, dotted, reopened, exported, and ambient namespace forms; preserve distinct local
  non-exported membership for each reopening block and ambient default exports.
- Classify legal merging, cross-slot coexistence, illegal redeclaration, global augmentation, UMD
  context errors, and deferred valid UMD/string-literal external-module forms according to WU0A.
- Reserve one compilation-global scope plus typed augmentation/context records, including
  `TK2669`, but keep them disconnected from production root lookup and publication until WU5. Do
  not introduce a second ambient resolver or `Store`.
- Add direct tests for namespace identity, public/private scope parentage, reopening isolation,
  dotted/nested equivalence, dormant cross-file global records, legal external/ambient versus
  illegal script context classification, both declaration orders, export visibility, and proof that
  production lookup cannot see the WU1b global scope. Production gates stay green without
  registering the full WU0 corpus.

### WU2 — namespace-qualified reservation and path-lookup substrate (effort L)

- Walk `A.B.C` recursively through namespace public member tables and classify the final member's
  slot without lowering or applying the successful leaf.
- Reserve namespace-contained type declarations before qualified references are lowered, so
  interfaces in namespaces and qualified heritage clauses do not depend on later publication.
- Preserve lexical shadowing before the namespace root is selected. Every later path segment uses
  the namespace public scope only, with no fallback to a parent namespace, module, global, value,
  or type slot. A value-only or private member must not satisfy a qualified type lookup.
- Only after lexical lookup finds no namespace meaning may root classification inspect checker-local
  type sources. Callable and instance-class type-parameter frames, active conditional `infer`
  binders, mapped key binders, and builtin type roots then report `TK2702`; a class type parameter
  behind the static-member barrier reports `TK2503`. Same-name lexical namespaces take precedence.
- When an admitted namespace slot coexists with a named or default import binding, root selection
  chooses that namespace slot before the unresolved import endpoint is deferred. Namespace imports
  and import-equals bindings remain rejected according to the WU1b import-form matrix.
- Independently visit the measured qualified paths in union, intersection, tuple, indexed access,
  conditional, mapped, template literal, function/constructor type, type-literal
  call/construct/method signature, and class constraint/field/method annotation routes. One failed
  child must not suppress its measured sibling. Reopening-private helper lexical lookup is already
  pinned by `namespace_visibility.ts` and needs no duplicate fixture.
- Emit strict-tsc-compatible topology/slot diagnostics `TK2503`, `TK2694`, `TK2702`, `TK2713`, and
  `TK2749`, with deterministic lexical event ownership. This includes missing/value-only roots,
  missing/value-only/private intermediates, type/class-only intermediate segments, namespace-only
  leaves, and explicit proof that a failed child lookup never falls back to its parent.
- Land public-path classification/reservation as the substrate. Successful type-bearing leaves do
  not select a first/last group fragment or adapt back to the legacy type slot; production type
  lowering closes only with WU3's atomic type-slot cutover. Successful qualified leaves, including
  forward reopening, and leaf diagnostics `TK2315`, `TK2314`, `TK2707`, and `TK2344` therefore remain
  WU3 work.

### WU3 — atomic production type-slot and merged-interface publication (effort XL)

- Atomically replace `Symbol.ty` and every type-space lookup/reservation/publication consumer with
  `TypeGroupId`. Adapt all single-fragment aliases/classes/interfaces in the same cut; delete the
  legacy winner path and forbid any `TypeGroupId -> DeclId` first/last adapter.
- Reserve one semantic interface identity per legal merged symbol and lower all declarations into
  one immutable source-order recovery surface, including namespace-contained interfaces and
  qualified member/heritage references enabled by WU2. No fragment owns an independently published
  object identity.
- Implement strict-tsc rules for generic parameters, heritage, properties, methods, overload
  groups, call/construct signatures, and index signatures across declaration blocks.
- Preallocate typed heritage/conflict/relation obligations and their lexical event owners during
  non-query construction. Freeze every surface, publish the whole dependency SCC atomically behind
  a final-state capability, and only then evaluate those obligations through
  `SemanticQueryCoordinator`; their outcomes must never mutate the surface.
- Preserve recursive and mutually recursive reserve/fill behavior; opposite declaration/check/SCC
  order must produce the same types and diagnostic event order.
- Report cross-declaration conflicts without suppressing independent diagnostics. Keep
  same-declaration duplicate detection owned by backlog `18`.
- Cut the old last-declaration publication path over atomically: it may not coexist with the ordered
  group path in production. Until WU4, class+interface(+namespace) groups return a directly tested,
  explicit typed unsupported/non-permissive outcome with a preallocated owner; they never take the
  old path or fabricate a surface. Stop on partial publication, construction-time query/relation
  work, validation before SCC publication, order-dependent `TypeId` structure, permissive conflict
  recovery, or mutation of an already-published hash-consed type.

### WU4 — atomic keep-pairs and static/value augmentation (effort XL)

- Implement interface+namespace type/container coexistence and composition.
- Implement the approved class+interface merge in both declaration orders, including interface-added
  required/method/generic/recursive instance members and strict-tsc generic-header/property-conflict
  diagnostics. Preserve the class constructor/value, existing statics, private nominal origin, and
  atomic class identity; the class owns the only `ClassId`/`ClassInstance`, and attached interface
  fragments have no independent object identity. Freeze one complete ordered effective group
  application/recovery frame on the class and map every class/interface binder into it;
  `ClassInstance` identity is `ClassId` plus every frame argument.
- For function+namespace, reserve ordinary/overload callable rows plus exported namespace value
  members in one draft, intern one immutable callable `ObjectType`, and publish it once to the symbol
  and every participating declaration slot. Body completion reuses reserved rows and cannot
  republish a bare `FunctionType`. Cover ordinary, overloaded, ambient, and reverse-order groups.
- Attach class+namespace exported members to the static surface while retaining class construction
  capability, nominal metadata, and atomic class publication.
- Prove interface+`declare var` cross-slot lookup against representative ES5 constructor patterns;
  the declared value annotation must resolve the final merged interface independently of order.
- Type-check only exported namespace members that augment an existing function/class value or
  static surface through existing member machinery. A standalone namespace remains a type
  container; do not add standalone namespace value/runtime semantics, JavaScript transformation,
  or emit.
- Join every admitted interface instance member and namespace value/static member to its existing
  class/function draft before the current freeze/SCC barrier. Cut all keep-pairs over atomically;
  never attach a fragment after publication or retain parallel old/new production paths.
- For admitted merged-group headers, non-query lowering makes a sole supplied constraint/default
  effective and may produce `Ready(TypeId)`. If a default is unavailable, retain typed
  `Unsupported(EventTicket)` plus the ADR-0008 declaration/default-use event protocol; do not query
  during construction or build a partial application. Directly prove that applications differing
  only in a fragment-local recovery argument have distinct application and projection identities.
- Keep rare three-way enum/function/namespace chimeras typed incomplete and incapable of producing a
  permissive receiver. `42` owns `TK2567` and exact three-way legality/recovery; this sprint owns
  `TK2434` and the non-permissive function+namespace surface. Do not claim a degraded implementation.

### WU5 — ambient/global surface closure (effort L)

- Implement `declare namespace` through the same namespace machinery.
- Atomically link and publish WU1b's disconnected global scope/records as the one production
  compilation global for every file. Implement cross-file `declare global` augmentation there,
  with module-local same-name declarations remaining isolated and opposite input order producing
  identical results; no partly linked state may land.
- Implement `TK2669` for global augmentation outside an external or ambient module. Direct gates
  cover legal external-module, legal ambient-module, and illegal script contexts plus exact lexical
  ownership.
- Implement `TK1314`/`TK1315` for invalid `export as namespace` contexts. Reassign
  `decl/namespace-export/self` to backlog `15`, with the valid `export =` form of
  `export as namespace` as its current witness; do not claim valid UMD global publication here.
  Backlog `15` owns the general external-module export surface and must differentially cover both
  this form and ordinary named exports.
- Keep string-literal ambient external modules, package discovery, and general import/export
  semantics with backlog `15`; reassign inventory ownership explicitly if WU0A proves necessary.
- Leave enum/function `TK2567` and exact enum+function+namespace legality with backlog `42`; retain
  this sprint's `TK2434` ownership and non-permissive function+namespace behavior.
- Audit every incomplete record owned by backlog `43`: each must be removed by supported behavior
  or retain a precise, machine-validated owner outside this criterion.

### WU6 — pinned `lib.es5.d.ts` readiness proof (effort M; no loader)

- Resolve the pinned TypeScript 6.0.3 `lib.es5.d.ts` exactly as recorded in
  [`lib-audit-6.0.3.md`](../backlog/lib-audit-6.0.3.md) and check it as an explicit input through the
  existing parse/bind/check pipeline.
- Require zero silently permissive or unsupported result owned by backlog `43`; classify every
  residual against a different concrete owner rather than broad allowlisting.
- Add semantic witnesses for all 28 interface+var pairs as a measured set and focused deep checks
  for `Array`, `Object`, `String`, `Number`, `Date`, repeated `Date`/`Number`/`String` interfaces,
  and the `Intl` namespace.
- Record command, pinned input digest, counts, remaining owners, and a GO/NO-GO conclusion for
  backlog `14`. This proves model readiness only; no standard-library auto-loading or Stage 1 store
  work may enter the diff.

### WU7 — independent adversarial review, official ratchet, and closure (effort L)

- A reviewer independent of WU1-WU6 starts from WU0 and attacks false negatives, declaration-order
  dependence, partial publication, recursive cycles, generic mismatches, slot collisions,
  exported/private reopening behavior, qualified-name errors, keep-pair statics, global
  augmentation, and ES5 constructor visibility. Every syntax probe is compared with strict
  `tsc 6.0.3`.
- Route every FAIL back to the owning implementation agent and repeat review after remediation.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build --release`, focused conformance, surface/manifest validation, and a fresh official
  suite `run --check`.
- Remove the official-suite namespace syntax prefilter only when the measured newly admitted set has
  zero regression. Newly exposed non-`43` gaps must stay explicitly OOS/owned; never change the
  scoreboard merely to hide a failure.
- On PASS, require `tests/surface/inventory.toml` and `completion-1.0.toml` validation with no stale
  backlog `43` owner: every reassigned surface must name a real active owner and witness, and the
  manifest dependency graph must remain machine-valid. Only then mark
  `A-namespaces-declaration-merging` complete, remove backlog `43`, update the roadmap and
  architecture/reference docs where behavior changed, stamp the outcome and commit map here,
  archive the sprint, and leave a precise active handoff for backlog `14`.

## Commit and agent topology

1. Plan commit.
2. WU0 spec-only commit.
3. WU0A decision commit.
4. WU1a identity-substrate commit.
5. WU1b namespace-binding-substrate commit.
6. WU2 qualified-reservation/path-lookup substrate commit.
7. WU3 atomic production type-slot/interface commit.
8. WU4 atomic keep-pair commit.
9. WU5 ambient/global and ownership commit.
10. WU6 readiness-proof commit.
11. Adversarial fixes as atomic owner-specific commits, followed by the official ratchet and closure
    commit.

Because WU1a-WU5 overlap binder/checker/type-store files, one implementation subagent owns the
serialized production path across those units. Read-only strict-tsc/official measurements may run
in parallel. A different subagent owns the final adversarial review; the leader verifies every gate
and makes all explicitly staged commits.

## Hard stop gates

- Stop before production until WU0 is separately committed and accepted ADR-0009 is committed.
- Stop and request architectural approval if stable merging cannot fit multi-slot symbols without
  new module/data-flow boundaries, mutable published identities, or shared-prelude storage.
- Stop on any false negative, source/check-order-dependent result, raw relation-boundary bypass,
  construction-time semantic query, partial/mutable publication, forbidden recovery type, or
  undocumented strict-tsc divergence. Last-declaration identity, one namespace scope, fragment
  union/intersection, validation-before-interface-SCC-publication, mutation by a semantic
  obligation, a winner adapter, special ambient/UMD paths, production global lookup before WU5,
  bare-function republication after callable-object publication, and old/new publication
  coexistence are rejected.
- Stop the ES5 claim unless every audited `43` shape has no silent fallback and every residual has a
  concrete different owner.
- Stop the official ratchet on any regression; progress is accepted only as measured evidence.
- Stop closure unless surface and manifest validation passes with no stale backlog `43` owner and
  every reassigned surface has a real active owner plus witness.
- Stop after backlog `43` closes. Do not opportunistically enter full library loading, shared Store,
  modules, parallelism, enums, `satisfies`, or unrelated soundness work.

## Out of scope

- Full `lib.d.ts` discovery/loading, frozen base plus delta `Store`, or parallelism Stage 1
  ([`14`](../backlog/14-libdts-loading.md)).
- General Bundler/package/module resolution or string-literal ambient external modules
  ([`15`](../backlog/15-modules-imports.md)).
- Namespace runtime emit, JavaScript transformation, or compiler output.
- Enums (`42`), `satisfies`/`as const` (`44`), and general duplicate diagnostics (`18`).
- Cross-file mutable export identity (`16`), incrementality (`17`), and unrelated parity/soundness
  backlog.

## Falsification and handoff rule

Reconsider combining a slice of backlog `14` only if WU6 proves that backlog `43` cannot be
meaningfully validated through the existing explicit-input pipeline and the indispensable loader
substrate is small, isolated, and does not introduce the frozen shared Store. If correct validation
requires the shared Store or ambient loader architecture, keep `14` separate as planned. Reconsider
adding `42` or `44` only if a reproducible pinned-library audit contradicts the committed ES5 audit.

## Run log

- **2026-07-15 — plan gate.** User approved option A (`43` plus an explicit-input ES5 readiness
  proof, excluding `14`). Three independent SOL reviews converged on the scope; one rejected the
  initial outline as insufficiently executable. Its required acceptance matrix, coexistence versus
  merge distinction, explicit readiness semantics, surface-owner audit, and stop gates are folded
  into this plan.
- **2026-07-15 — pre-commit review.** The reviewer required qualified reservation/resolution before
  complete interface publication, restricted value-side work to function/class augmentation, and
  made machine-valid surface/manifest ownership a closure gate. The plan now incorporates all three
  blockers.
- **2026-07-15 — WU0 oracle and baseline.** Added the disabled, unregistered
  `tests/cases/b43_namespaces_declaration_merging/` matrix (27 files after two WU2 spec addenda;
  23 files in the original WU0 commit). `tsc --version` is `6.0.3`; the original 23-file
  `tsc --strict --noEmit --pretty false --lib es5 --module commonjs` run produced 186 diagnostics:
  all 184 ordinary `TK` markers matched by code/line. The two additional `TS2567`s in the
  enum/function/namespace chimera are not claimed as backlog-43 marker parity: its enum declaration
  remains explicitly incomplete under backlog `42`, and exact duplicate-kind diagnostic ownership
  is a WU0A/direct-test gate. The valid UMD `.d.ts` is specifically the `export =` form and retains both
  `decl/namespace-export/self` and `decl/export-assignment/self` under backlog `15`; local merged
  member access is not accepted as proof of global UMD publication. `export as namespace` can also
  publish an external module's ordinary named-export surface, so backlog `15` must add differential
  coverage for both forms. Probes confirmed the non-obvious rules:
  later interface blocks do not win identical overload selection (the first method/call/construct
  literal return wins); ordinary namespace-before-function/class reports `TS2434`, but ambient
  reverse order is clean; private members do not cross reopened namespace blocks; ambient namespace
  members export by default; and legal `declare global` requires an external-module context.
  Two separate legal global-augmentation blocks merge while a module-local same-name interface
  stays isolated. The user-approved addendum promotes class+interface(+namespace) from a decision
  probe to required behavior: six fixtures pin both orders, composed instance/static/container
  surfaces, class construction and private origin, matching/conflicting generic headers, recursive
  members, namespace placement, instance call/construct signatures, capability separation, and
  non-permissive property/method/heritage conflict recovery. Omitted-vs-introduced constraints and
  defaults are legal in either order; only conflicting supplied names/constraints/defaults or
  arities reject. Invalid groups must retain the complete typed tsc-compatible recovery surface,
  including distinct fragment parameter positions after name/arity mismatch; they may never
  substitute `any`, error, `unknown`, synthesized `never`, partial vectors, dropped fragments, or
  whole-group exhaustion. Direct identity remains a WU0A test obligation.
  Conflict probes pin `TS2300/2320/2374/2411/2413/2428/2687/2717` plus downstream wrong-type demands so
  recovery cannot become `any`/error-permissive. Qualified probes pin
  `TS2315/2344/2503/2694/2702/2707/2749` across namespace, type-only, and value-only slots.
- **2026-07-15 — WU0 current-checker measurement (pre-addendum).** The existing debug binary over
  the original 23 focused files exited incomplete for 19 and diagnostic-only for 4, emitting 183
  diagnostics and 94 incomplete records; `cargo test conformance` remains green because the new
  corpus is deliberately unregistered. The inventory has exactly five backlog-43 records:
  `type-fill/module-declaration/self`, `annotation-lower/type-name/qualified-name`,
  `decl/global-declaration/self`, `decl/module-declaration/self`, and
  `decl/namespace-export/self`.
- **2026-07-15 — WU2 oracle addendum.** Added two disabled fixtures after independent WU1b review
  exposed path-slot and ambient export-list edges missing from the original matrix. Pinned global
  `tsc --version` reports `6.0.3` (the bench-local tool is `7.0.1-rc` and was not used as an oracle).
  `wu2_topology_slot_edges.ts` exits `2` with exactly nine diagnostics: `TS2702` x2 at the alias and
  class root spans, `TS2503` at the value-only root, `TS2694` x4 for missing/value-only
  intermediates, no-parent-fallback, and a namespace-only leaf, plus `TS2713` x2. The two `TS2713`
  diagnostics start on the terminal `Leaf` segments at lines/columns `23:47` and `24:49` and pin
  the complete `Cannot access 'X.Leaf' ... X["Leaf"]` messages. WU2 claims successful forward-path
  classification, but no successful leaf lowering or application. `wu2_ambient_export_alias_list.ts`
  exits `2` with exactly `TS2749` at `17:18` for the public value alias and `TS2694` x2 at `18:39`
  and `19:38` for the post-list declaration and original pre-list name; both an ordinary alias whose
  target occupies type space and an explicit type-only export-specifier alias check clean. The
  aggregate 25-file oracle exits `2` with 198 diagnostics: 196 ordinary marker-owned diagnostics
  plus the two already documented backlog-42 `TS2567`s. `cargo test conformance` remains the
  behavior-neutral gate because the corpus is still unregistered.
- **2026-07-15 — WU2 checker-root/context oracle addendum.** Added two more disabled topology-only
  fixtures. Pinned global `tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs`
  reports six `TS2702`s and one `TS2503` in `wu2_checker_local_qualified_roots.ts`. The `TS2702`
  roots are callable parameters at `2:58` and `5:22`, active conditional `infer InferT` at `8:67`, the
  mapped key binder at `11:30`, builtin `Array` at `14:20`, and an instance-class parameter at
  `17:19`; each exact message says the named root only refers to a type and is being used as a
  namespace. The static class-member contrast reports `TS2503` at `21:17`; its exact message is
  `Cannot find namespace 'StaticT'`. Four controls check clean: lexical namespaces named `T`, `U`,
  and `K` win over a same-name callable parameter, active `infer` binder, and mapped key binder, while
  `BuiltinHost.Array` wins over the builtin type root. The checker-local TypeOnlyRoot rule therefore
  applies only after lexical namespace lookup finds no namespace meaning.
  `wu2_qualified_contexts.ts` reports exactly 32 `TS2694`s,
  with terminal-member starts at `4:23,37`; `5:30,51`; `6:24,38`; `7:31,47`;
  `8:29,45,57,72`; `9:38,53`; `10:34,58`; `11:41,68`; `12:29,47`; `13:41,59`;
  `14:50,73`; `16:22,52,75`; `19:41`; `20:21`; and `21:31,58,78`. The markers pin each exact
  `Namespace 'N' has no exported member 'X'` message and prove independent visits only across the
  enumerated union, intersection, tuple, indexed access, conditional, mapped, template literal,
  function/constructor type, type-literal call/construct/method signature, and class annotation
  routes. The aggregate 27-file oracle exits `2` with 237 diagnostics: 235 ordinary marker-owned
  diagnostics plus the two existing backlog-42 `TS2567`s.
- **2026-07-15 — WU2 import-root oracle.** A scratch two-file project checked with the same pinned
  `tsc 6.0.3` command exits `0`: a named import plus same-name namespace exposes `NamedN.X`, and a
  default import plus same-name namespace exposes `DefaultN.X`. The dependency exports
  `const NamedN = 1` plus a default function; the consumer imports `DefaultN, { NamedN }`, declares
  both same-name namespaces with exported `X` interfaces, and types object literals through both
  qualified paths. These are root-selection witnesses, not import-endpoint or
  successful-leaf-lowering claims: WU2 must prefer the admitted namespace slot before deferring the
  import endpoint. The existing WU1b matrix remains authoritative for the rejected forms: namespace
  import plus namespace and import-equals plus namespace report `TS2440`. Reopening-private helper
  lexical behavior remains covered by `namespace_visibility.ts`; no new fixture duplicates it.
- **2026-07-15 — WU0 official-suite measurement.** At pinned TypeScript SHA
  `050880ce59e30b356b686bd3144efe24f875ebc8`, the committed scoreboard is 874 files, 339 IN / 535
  OOS; the latest existing-binary report is 340 / 534 with 69 unsaved progress entries. The coarse
  `syntax:module` bucket contains 117 files: 92 single-file identifier-namespace cases, one
  `declare global`, 22 export-only backlog-15 cases, and two regex false positives; one additional
  namespace case is multifile
  (`conformance/classes/members/instanceAndStaticMembers/superInStaticMembers1.ts`). Simulating
  removal of the broad gate yields only four newly-IN files—
  `conformance/expressions/typeGuards/typeGuardsInExternalModule.ts`,
  `conformance/types/intersection/intersectionMemberOfUnionNarrowsCorrectly.ts`,
  `conformance/types/literal/stringMappingReduction.ts`, and
  `conformance/types/tuple/contextualTypeTupleEnd.ts`—plus 70 unsupported results and secondary
  gates. This is not a backlog-43 progress promise: WU7 must structurally split identifier
  namespace/global/UMD syntax from external modules instead of deleting the broad regex.
- **2026-07-15 — WU0 direct-test handoff.** Marker fixtures cannot prove stable `TypeId` identity,
  same-span diagnostic ordering, heritage-event attribution, final-state capability isolation, or
  absence of construction-time queries. WU0A/WU1a-WU5 must add direct tests for those properties,
  including complete interface-SCC publication before coordinator validation, immutable outcomes,
  fragment-local recovery arguments in application/projection identity, all four function+namespace
  forms, the typed WU3-to-WU4 class+interface stop, disconnected WU1b global records, the atomic WU5
  global link, `TK2669` context/ownership, and a chimera receiver that cannot become permissive,
  before the corpus is registered.
- **2026-07-15 — WU0A accepted architecture gate.** Accepted
  [ADR-0009](../decisions/0009-ordered-declaration-groups-and-namespace-publication.md). Every source
  declaration owns a unique unified lexical `DeclId`, distinct from value-storage identity and
  stable ordered `TypeGroupId`; WU1a leaves `Symbol.ty` unchanged, and WU3 atomically switches every
  type-space consumer. Namespaces own one public scope plus private reopening scopes, and qualified
  lookup uses only the selected namespace's public path without fallback. Standalone interface
  groups publish immutable source-order recovery surfaces dependency-SCC-at-a-time before
  coordinator validation. Class+interface(+namespace) groups retain the class's sole `ClassId`,
  extend `ClassInstance` identity across the complete effective group recovery frame, and join
  interface/namespace fragments before the existing SCC/static freeze. Function+namespace groups
  publish one callable object. Generic-header recovery remains fully typed and oracle-compatible,
  with supported non-query defaults becoming effective. WU1b's global scope/records stay dormant;
  WU5 atomically links the one compilation global and owns `TK2669`. Valid UMD publication over
  either `export =` or named exports remains with `15` (the WU0 witness is the former), enum/function
  `TK2567` plus exact three-way legality remains with `42`, and `43` retains `TK1314`/`TK1315`,
  `TK2434`, and non-permissive function+namespace behavior. Production may now start at WU1a only
  after this ADR is committed; WU3, WU4, and WU5 remain atomic cutovers.

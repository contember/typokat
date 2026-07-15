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

## Refs re-verified at HEAD (2026-07-15, `ebd79ac`)

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
| Keep-pair merging | Interface+namespace, function+namespace, and class+namespace static/type surfaces work in either declaration order | Rare enum+namespace+function chimeras remain conservative and explicit |
| Namespace visibility | Exported members form the reopened public namespace surface; non-exported members remain local to their declaration block | No accidental sharing of private block members across reopenings |
| Global/UMD surface | Give the currently `43`-owned `declare global` and `export as namespace` inventory entries an implemented or precise non-`43` disposition | Do not absorb general package/module loading |

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
  ambient/global/UMD forms; missing root/member diagnostics; and explicit degraded chimeras.
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

### WU1 — namespace binding and ordered merge groups (effort L)

- Activate the namespace slot and introduce namespace scopes without resolving types in the binder.
- Retain every legal same-name declaration in source order instead of last-write replacement.
- Bind nested, dotted, reopened, exported, and ambient namespace forms; preserve distinct local
  non-exported membership for each reopening block.
- Classify legal merging, cross-slot coexistence, illegal redeclaration, global augmentation, UMD
  export, and string-literal external module forms according to WU0A.
- Add binder-level tests for identity, scope parentage, ordering, both declaration orders, and
  export visibility. Production gates stay green without registering the full WU0 corpus.

### WU2 — namespace-qualified type reservation and resolution (effort L)

- Resolve `A.B.C` recursively through namespace public member tables and the final member's type
  slot, including generic application at the leaf.
- Reserve namespace-contained type declarations before qualified references are lowered, so
  interfaces in namespaces and qualified heritage clauses do not depend on later publication.
- Preserve lexical shadowing and cross-space parent lookup. A value-only or private member must not
  satisfy a qualified type lookup.
- Emit strict-tsc-compatible `TK2503` for a missing namespace and `TK2694` for a missing exported
  member, with deterministic lexical event ownership.
- Close `annotation-lower/type-name/qualified-name` only after nested, dotted, reopened, shadowed,
  recursive, and unresolved controls pass.

### WU3 — stable merged-interface publication (effort XL)

- Reserve one semantic interface identity per legal merged symbol and lower all declarations into
  one complete surface, including namespace-contained interfaces and qualified member/heritage
  references enabled by WU2.
- Implement strict-tsc rules for generic parameters, heritage, properties, methods, overload
  groups, call/construct signatures, and index signatures across declaration blocks.
- Preserve recursive and mutually recursive reserve/fill behavior; opposite declaration/check
  order must produce the same types and diagnostic event order.
- Report cross-declaration conflicts without suppressing independent diagnostics. Keep
  same-declaration duplicate detection owned by backlog `18`.
- Stop on partial publication, order-dependent `TypeId` structure, permissive conflict recovery, or
  any need to mutate an already-published hash-consed type.

### WU4 — approved keep-pairs and static/value augmentation (effort XL)

- Implement interface+namespace type/container coexistence and composition.
- Attach function+namespace and class+namespace exported members to the value/static surface while
  retaining callable rows, overload visibility, class construction capability, nominal metadata,
  and atomic class publication.
- Prove interface+`declare var` cross-slot lookup against representative ES5 constructor patterns;
  the declared value annotation must resolve the final merged interface independently of order.
- Type-check only exported namespace members that augment an existing function/class value or
  static surface through existing member machinery. A standalone namespace remains a type
  container; do not add standalone namespace value/runtime semantics, JavaScript transformation,
  or emit.
- Keep rare three-way enum/function/namespace chimeras conservative, diagnosed/incomplete, and
  incapable of producing a permissive receiver.

### WU5 — ambient/global surface closure (effort L)

- Implement `declare namespace` through the same namespace machinery.
- Implement or precisely dispose `declare global` augmentation and `export as namespace` according
  to WU0A and their current surface-inventory ownership.
- Keep string-literal ambient external modules, package discovery, and general import/export
  semantics with backlog `15`; reassign inventory ownership explicitly if WU0A proves necessary.
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
4. WU1 binder boundary commit.
5. WU2 qualified-reservation/resolution commit.
6. WU3 merged-interface commit.
7. WU4 keep-pair commit.
8. WU5 ambient/global commit.
9. WU6 readiness-proof commit.
10. Adversarial fixes as atomic owner-specific commits, followed by the official ratchet and closure
    commit.

Because WU1-WU5 overlap binder/checker/type-store files, one implementation subagent owns the
serialized production path across those units. Read-only strict-tsc/official measurements may run
in parallel. A different subagent owns the final adversarial review; the leader verifies every gate
and makes all explicitly staged commits.

## Hard stop gates

- Stop before production until WU0 is separately committed and WU0A is approved.
- Stop and request architectural approval if stable merging cannot fit multi-slot symbols without
  new module/data-flow boundaries, mutable published identities, or shared-prelude storage.
- Stop on any false negative, source/check-order-dependent result, raw relation-boundary bypass,
  partial publication, forbidden recovery type, or undocumented strict-tsc divergence.
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

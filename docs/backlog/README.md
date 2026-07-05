# backlog

Decided work items ("issues") not yet scheduled into a sprint. One self-contained
file per item: `NN-<slug>.md` (zero-padded, **folder-local** sequence — don't
renumber, gaps are fine). Copy [`_template.md`](_template.md).

**No `status:` field** — an item is alive because it lives here. It leaves by being
**deleted** on ship (default; git holds the record) or moved to `../archive/` if it
documents something a future reader needs. Dependencies go in frontmatter:
`blocked-by: [./NN-other.md]`.

Add scope sub-folders (`security/`, `perf/`, …) only once the flat list gets
unwieldy; numbers stay folder-local.

This **is** the roadmap. Items `01`–`09` and `20` shipped — `01`–`04` (current-impl bugs), `05`
(object/interface signatures, F1), `06` + `20` (class completeness + constructor accessibility),
`07` (unstructured-flow narrowing, M23), `08` (generic constraints, M24), `09` (conditional
types, M25); see [`../archive/`](../archive/README.md). `10`–`17` are the milestone roadmap. Architecture §12 governs ordering — the relation
engine + narrowing come **before** type-level evaluation, whose speed lives in the tree-walker's
algorithms, not a VM (the bytecode VM is a deferred, profiling-gated refactor — see
[ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)). See
[`../reference/dev-method.md`](../reference/dev-method.md) for how each item is built.

## Items

**The type-level evaluation phase (tree-walked; bytecode VM deferred — ADR-0001)**
- [`10`](10-mapped-types.md) — mapped types (M26).
- [`11`](11-template-literal-types.md) — template literal types (M27).
- [`12`](12-utility-types.md) — utility types (M28) · blocked-by `10`.
- [`13`](13-bytecode-vm.md) — post-evaluator profiling gate: bytecode VM only if measured dispatch overhead remains after `10`–`12` · blocked-by `10`–`12`.

**Long-term: real-world scale + IDE**
- [`14`](14-libdts-loading.md) — full `lib.d.ts` loading · blocked-by `10` (an earlier minimal ambient/prelude slice is allowed when useful).
- [`15`](15-modules-imports.md) — modules / imports / module resolution, staged from correctness-first whole-repo checking to cross-file identity.
- [`16`](16-parallelism-type-universe.md) — parallelism: shared type universe hardening (Stages 1 & 2) · blocked-by `14`, `15`.
- [`17`](17-incrementality.md) — incrementality (Phase 5) · blocked-by `16`.
- [`18`](18-duplicate-identifier-detection.md) — duplicate identifier detection (`TK2300`) for duplicate object/interface members.
- [`19`](19-call-of-non-callable-diagnostic.md) — `TK2349` call-of-non-callable diagnostic once dropped callability is distinguishable.

**Small known gaps (review findings, sprint 2026-07-04)**
- [`21`](21-local-class-checking.md) — function-local classes are entirely unchecked (all class-keyed diagnostics silent).
- [`22`](22-new-callee-forms.md) — `new (C)()` / aliased `new` miss class-keyed checks (`TK2511`/`TK2673`/`TK2674`).
- [`23`](23-static-method-type-params.md) — `static of<U>(u: U)` raises spurious `TK2304`.
- [`24`](24-rest-elements-in-type-model.md) — tuple rest elements + rest parameters missing from the type model (silently permissive).
- [`25`](25-intersection-types.md) — intersection types (`A & B`) missing from the type model (silently permissive).
- [`26`](26-cross-binder-nested-infer.md) — cross-binder nested `infer` (de Bruijn shifting on embed); M25 ships the conservative poisoned-deferral stopgap.
- [`27`](27-template-buried-conditional-evaluation.md) — conditionals buried in named alias/interface/class bodies stay deferred (conservative); evaluate them.
- [`28`](28-interface-extends-composition.md) — interface `extends` members missing from the interface type (silent false negatives; pre-existing, found by the M26 review).
- [`29`](29-silent-alias-cycle-permissiveness.md) — alias-cycle re-entry silently error-types with no primary diagnostic (missing `TK2456` for plain circular aliases; legal member recursion degraded — pre-existing, M26 review).

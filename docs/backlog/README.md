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

This **is** the roadmap. Items `01`–`07` and `20` shipped — `01`–`04` (current-impl bugs), `05`
(object/interface signatures, F1), `06` + `20` (class completeness + constructor accessibility),
`07` (unstructured-flow narrowing, M23); see [`../archive/`](../archive/README.md).
`08`–`17` are the milestone roadmap. Architecture §12 governs ordering — the relation
engine + narrowing come **before** type-level evaluation, whose speed lives in the tree-walker's
algorithms, not a VM (the bytecode VM is a deferred, profiling-gated refactor — see
[ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)). See
[`../reference/dev-method.md`](../reference/dev-method.md) for how each item is built.

## Items

**Generics (precede type-level evaluation)**
- [`08`](08-generic-constraints.md) — generic constraints `<T extends U>` (M24).

**The type-level evaluation phase (tree-walked; bytecode VM deferred — ADR-0001)**
- [`09`](09-conditional-types.md) — conditional types (M25) · blocked-by `08`.
- [`10`](10-mapped-types.md) — mapped types (M26) · blocked-by `08`.
- [`11`](11-template-literal-types.md) — template literal types (M27).
- [`12`](12-utility-types.md) — utility types (M28) · blocked-by `09`, `10`.
- [`13`](13-bytecode-vm.md) — post-evaluator profiling gate: bytecode VM only if measured dispatch overhead remains after `09`–`12` · blocked-by `09`–`12`.

**Long-term: real-world scale + IDE**
- [`14`](14-libdts-loading.md) — full `lib.d.ts` loading · blocked-by `09`, `10` (an earlier minimal ambient/prelude slice is allowed when useful).
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

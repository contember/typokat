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

This **is** the roadmap. Item `05` is a gap surfaced by the official-suite harness (an `F*`
finding; `01`–`04` shipped — see [`../archive/`](../archive/README.md)); `06`–`17` are the
milestone roadmap. Architecture §12 governs ordering — the relation
engine + narrowing come **before** type-level evaluation, whose speed lives in the tree-walker's
algorithms, not a VM (the bytecode VM is a deferred, profiling-gated refactor — see
[ADR-0001](../decisions/0001-type-level-vm-is-a-deferred-evaluator-optimization.md)). See
[`../reference/dev-method.md`](../reference/dev-method.md) for how each item is built.

## Items

**Near-term completeness**
- [`05`](05-object-interface-signatures.md) — call/method/construct signatures in object & interface types (candidate milestone).
- [`06`](06-class-completeness-checks.md) — class-completeness checks `TK2416` + `TK2515` (good warm-up).

**Narrowing + generics (both precede type-level evaluation)**
- [`07`](07-unstructured-flow-narrowing.md) — unstructured-flow narrowing, the flow-node CFG (M23).
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

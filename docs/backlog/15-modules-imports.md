---
id: 15
title: Modules / imports / module resolution (whole-repo checking)
blocked-by: []
---

# 15 — Modules / imports / module resolution

**Summary.** Module-resolver **breadth** — whole-repo checking in a **serial** type universe;
also where I/O cost dominates. The correctness-first local-relative slice shipped as M29; the
remaining work here is full resolver breadth. **Ownership boundary (WU7):** this item owns
resolver breadth only. Parallel cross-file type identity (Stage 2) is owned solely by backlog
[`16`](./16-parallelism-type-universe.md) — do not duplicate it here. The pinned, narrow
real-project vertical slice is backlog [`72`](./72-real-project-preview-readiness.md); any resolver
pieces it lands become inputs here rather than a second resolver.

## Problem

M29 added a serial `check_project` path for provided local relative `.ts` files with named
imports/exports. Real projects still need package/tsconfig resolver breadth: `node_modules`
packages, `tsconfig` resolver options (`paths`/`baseUrl`/`moduleResolution`), `.d.ts`
consumption, default/namespace/star imports, re-exports, and (guarded) module cycles.
NodeNext also requires source substitution (`./foo.js` resolving to `foo.ts`/`foo.d.ts`) and
condition-aware package resolution through `exports`/`imports` plus `types`/`typesVersions`.
The CLI must discover a project config and its `files`/`include`/`exclude` roots instead of
requiring every source file to be enumerated manually.

## Approach / acceptance

Extend the serial `check_project` path with resolver breadth in one type universe:

- **Correctness-first whole-repo slice (shipped as M29):** local relative module resolution +
  import/export wiring runs serially. Acceptance: multi-file fixtures with imports/exports check
  correctly.
- **Preview vertical slice (owned by `72`):** project/tsconfig discovery, NodeNext local
  `.js` -> `.ts` substitution, and the package declaration paths exercised by one pinned real
  project. This proves integration early but does not close this item.
- **Resolver breadth (this item's remaining work):** generalize the same resolver to package
  `exports`/`imports` conditions, `types`/`typesVersions`, NodeNext source substitution,
  `node_modules` lookup, `.d.ts` consumption, `paths`/`baseUrl`, project-reference/root discovery,
  default/namespace/star imports, re-exports, and guarded cycles, still **serial**. Acceptance:
  fixtures cover each supported condition/layout and tsconfig root-selection rule against tsc,
  plus both the small preview project and the later pinned `contember/deptective` full-stack
  witness from `72` remain deterministic under the general path. The deptective witness pins its
  lockfile/tool versions and explicitly records which tsc version is the oracle; backlog `14`
  owns its ambient-library half.

This item consumes resolver-affecting compiler options; it does **not** own compiler-option
validation diagnostics. Unknown/incompatible-option `5xxx` errors, emit-only switches, and CLI
configuration policy remain out of scope per [`scope.md`](../reference/scope.md).

Crossing file boundaries under **parallel** execution — cross-file type identity via the stable
structural hash or a shared growing interner (§3.4 knot, architecture §8.2) — is **not** this
item; it is backlog `16` Stage 2. If resolver breadth ships while checking is still serial,
that is correct-but-serial by design (document it as such); the parallel type-universe strategy
lands with `16`.

## Touch points

Module resolution + import/export binding (NodeNext, package exports/types, tsconfig roots/options,
`.d.ts`); CLI project discovery; the serial `check_project` path. (Cross-file identity and
`driver::check_files` are backlog `16`.)

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->

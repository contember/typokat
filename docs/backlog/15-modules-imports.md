---
id: 15
title: Bundler modules / imports via oxc_resolver (whole-repo checking)
blocked-by: []
---

# 15 — Bundler modules / imports via `oxc_resolver`

**Summary.** Integrate `oxc_resolver` as the physical-resolution authority for typokat's supported
1.0 `moduleResolution: "bundler"` profile, then complete module graph and import/export semantics in
one **serial** type universe. The correctness-first local-relative slice shipped as M29. Per
[`ADR-0007`](../decisions/0007-bundler-resolution-via-oxc-resolver.md), this item does not reimplement
filesystem/package/tsconfig resolution and does not require NodeNext parity. Parallel cross-file
type identity (Stage 2) remains solely backlog [`16`](./16-parallelism-type-universe.md). The pinned,
narrow real-project vertical slice is backlog [`72`](./72-real-project-preview-readiness.md); its
integration work becomes an input here, not a second resolver.

## Problem

M29 added a serial `check_project` path for provided local relative `.ts` files with named
imports/exports. Real Bundler-profile projects also need declaration-aware extension substitution,
`node_modules`/`@types`, package `exports`/`imports` and `types`, path aliases, tsconfig inheritance
and references, and `.d.ts` consumption. Those physical lookup rules are already the domain of
[`oxc_resolver`](https://docs.rs/oxc_resolver/latest/oxc_resolver/): its
[`resolve_dts`](https://docs.rs/oxc_resolver/latest/oxc_resolver/struct.ResolverGeneric.html#method.resolve_dts)
API explicitly targets TypeScript Bundler resolution. Recreating the algorithms in `driver.rs`
would add a second authority without advancing typokat's type model.

Delegating lookup does not build a checker project. The CLI still needs to start from a directory or
config and enumerate configured `files`/`include`/`exclude` roots where the crate does not provide a
complete root set. Typokat must construct a deterministic module graph, load every resolved file,
wire default/namespace/star imports and re-exports across value/type/namespace spaces, handle guarded
cycles, parse/bind/check `.d.ts`, and report every unresolved or unsupported branch explicitly.

Valid UMD global publication is also part of this module boundary. In a declaration-file external
module, `export as namespace N` globally aliases/publishes that external module's export surface.
The surface may come from `export =` or ordinary named exports; local namespace/type lookup is not
sufficient. Backlog `43` owns the `TK1314`/`TK1315` context diagnostics, while this item owns the
valid publication semantics and both surface inventory entries. The current WU0 ownership witness
is specifically the `export =` form, not the complete differential matrix.

## Approach / acceptance

Extend the serial `check_project` path with a single explicit dependency boundary:

- **Correctness-first whole-repo slice (shipped as M29):** local relative module resolution +
  import/export wiring runs serially. Acceptance: multi-file fixtures with imports/exports check
  correctly.
- **Preview vertical slice (owned by `72`):** configure `oxc_resolver` for Bundler declaration
  resolution, discover and enumerate the witness project, and exercise only its import/export
  forms. This proves the boundary early but does not close this item.
- **Physical resolution (external authority):** use the pinned crate's tsconfig and `resolve_dts`
  facilities for extension substitution, `node_modules`/`@types`, package conditions and declaration
  metadata, `paths`/`baseUrl`, inheritance, and references. Do not add fallback filesystem probes or
  a local package resolver. Typokat owns only source-root enumeration that the crate does not expose,
  deterministic invocation/configuration, and conversion of results into project identities.
- **Module semantics (typokat-owned):** construct the graph; load and parse resolved `.ts`/`.d.ts`
  files; bind default/namespace/star/type-only imports, export lists and re-exports; handle guarded
  cycles; implement `export =` and valid `export as namespace` publication over the external
  module's export surface; and check all files in the serial type universe. Resolution success
  alone is not semantic import/export support.
- **Differential acceptance:** against pinned `tsc 6.0.3` with
  `moduleResolution: "bundler"`, fixtures cover local extension substitution, package and `@types`
  declarations, `exports`/`imports` plus `types`, path aliases, config inheritance/references, root
  selection, missing targets, symlinks, and every admitted import/export form. UMD fixtures must
  differentially cover `export as namespace` with both an `export =` surface and an ordinary named-
  export surface, proving the corresponding global alias and module-local isolation. Repeated runs
  are byte-identical and no configured, resolved, unresolved, or unsupported file/specifier
  disappears from project accounting.

The dependency's behavior is part of the compatibility boundary, not assumed parity. Today,
`resolve_dts` uses the tsconfig supplied through resolver options for `paths`, and
`typesVersions` selection is simplified relative to TypeScript's compiler-version range selection.
The spec-first corpus must pin these cases before integration. A mismatch is fixed upstream and
adopted through a pinned crate upgrade, or is an explicit unsupported project outcome with an owned
fixture; it is never silently approximated in typokat.

Both the small preview witness and the later full-stack witness from `72` must run under the
supported Bundler oracle profile. `contember/deptective` remains a candidate only if its pinned
witness can use a Bundler-compatible config without changing the program's type meaning; otherwise
select a replacement instead of claiming NodeNext support. The witness pins its lockfile/tool
versions and the checker-wide TypeScript oracle; backlog `14` owns its ambient-library half.

This item consumes resolver-affecting compiler options; it does **not** own compiler-option
validation diagnostics. Unknown/incompatible-option `5xxx` errors, emit-only switches, and CLI
configuration policy remain out of scope per [`scope.md`](../reference/scope.md).

The only required 1.0 profile is Bundler. `nodenext`, `node16`, classic Node, CommonJS-specific,
and other resolution profiles must produce an explicit unsupported-profile result rather than be
silently routed through Bundler. Adding one later requires its own decision and differential corpus.

Crossing file boundaries under **parallel** execution — cross-file type identity via the stable
structural hash or a shared growing interner (§3.4 knot, architecture §8.2) — is **not** this
item; it is backlog `16` Stage 2. If resolver breadth ships while checking is still serial,
that is correct-but-serial by design (document it as such); the parallel type-universe strategy
lands with `16`.

## Touch points

`oxc_resolver` integration and option mapping; project/config discovery and source-root enumeration;
deterministic module graph; import/export binding; `.d.ts` loading/checking; the serial
`check_project` path and project accounting. (Cross-file identity and `driver::check_files` are
backlog `16`.)

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->

---
id: 15
title: Bundler modules / imports via oxc_resolver (whole-repo checking)
blocked-by: []
---

# 15 — Bundler modules / imports via `oxc_resolver`

**Summary.** Expand the shipped bounded Bundler project route into complete module/import/export
coverage in one **serial** type universe. `oxc_resolver 11.24.2` is already the sole physical
authority for the admitted files-only project profile. The archived
[`2026-08-08 sprint`](../archive/sprint-2026-08-08-acyclic-source-reexports.md) shipped acyclic
local named source re-exports over the existing namespace-free value/type slots. The archived
[`default-slot sprint`](../archive/sprint-2026-08-08-default-module-slots.md) then shipped direct
default declarations/expressions and regular default imports through a structurally distinct slot.
Default bridges, namespace/star forms, cycles, and package/declaration breadth remain. Per
[`ADR-0007`](../decisions/0007-bundler-resolution-via-oxc-resolver.md), this item does not reimplement
filesystem/package/tsconfig resolution and does not require NodeNext parity. Parallel cross-file
type identity (Stage 2) remains solely backlog [`16`](./16-parallelism-type-universe.md). The pinned
public witness and mutation ratchet remain backlog [`72`](./72-real-project-preview-readiness.md).

## Problem

M29 added a serial `check_project` path for provided local relative `.ts` files with named
imports/exports. The bounded public project route now discovers an exact files-only
strict/noEmit/ESNext/Bundler config, resolves configured local `.ts` roots and extensionless or
`.js`→`.ts` named imports through `oxc_resolver 11.24.2`, inventories every module form before
filtering, and returns a deterministic complete summary. Unsupported forms are explicit non-clean
notices.

The six-candidate WU4 screen showed the practical gap before this slice: five projects first failed
on source re-exports or default exports, while the sixth used NodeNext-only explicit `.ts`
specifiers. Acyclic named source re-export publication is now shipped, including aliases,
outer/inline type-only forms, chains, and a class's existing value/type pair. Namespace-bearing
targets remain unsupported. Direct default declaration/expression export plus regular default-import
semantics are now shipped. Keep both source orders and exact missing/export-space diagnostics in
every later slice; do not pull package loading into an unrelated semantic change.

The follow-up immutable six-candidate re-screen at `659e30e` confirmed that no candidate yet meets
backlog `72`'s unchanged zero threshold. `lokicik/placetext` reached one route notice, a default
export at `src/index.ts:104:1`, before semantic checking started. The direct default-slot slice has
removed that specific blocker, but its transparent overlay still drops native target/library
options and equivalence is not proved. A fresh re-screen must establish the new first blocker; the
shipped slice is not evidence that `placetext` now qualifies. The other
candidates still stop on cycles or specifier policy, explicit `.ts` specifiers, or checker model and
diagnostic gaps. The exact exits and blockers are retained in the
[`re-screen archive`](../archive/sprint-2026-08-08-real-project-rescreen.md).

Later Bundler-profile breadth still needs declaration-aware extension substitution,
`node_modules`/`@types`, package `exports`/`imports` and `types`, path aliases, tsconfig inheritance
and references, and `.d.ts` consumption. Those physical lookup rules are already the domain of
[`oxc_resolver`](https://docs.rs/oxc_resolver/latest/oxc_resolver/): its
[`resolve_dts`](https://docs.rs/oxc_resolver/latest/oxc_resolver/struct.ResolverGeneric.html#method.resolve_dts)
API explicitly targets TypeScript Bundler resolution. Recreating the algorithms in
`crates/typokat-frontend/src/frontend.rs` would add a second authority without advancing typokat's
type model.

Delegating lookup does not build module semantics. Typokat must publish re-exported and default
symbols across value/type/namespace spaces, then broaden root selection and loading, construct the
general deterministic graph, load every resolved file, wire namespace/star forms, handle guarded
cycles, parse/bind/check `.d.ts`, and report every unresolved or unsupported branch explicitly.

Ambient external modules and valid UMD global publication are also part of this module boundary. In
a declaration-file external module, `export as namespace N` globally aliases/publishes that
external module's export surface.
The surface may come from `export =` or ordinary named exports; local namespace/type lookup is not
sufficient. The namespace sprint shipped the `TK1314`/`TK1315` context diagnostics; this item owns
string-literal ambient external modules, valid publication semantics, and both UMD surface
inventory entries. The current WU0 ownership witness
is specifically the `export =` form, not the complete differential matrix.

## Approach / acceptance

Extend the serial `check_project` path with a single explicit dependency boundary:

- **Shipped substrate:** local named imports/exports run serially; the public exact files-only
  Bundler route discovers roots, delegates physical lookup to `oxc_resolver 11.24.2`, accounts for
  every form, and fails closed on unsupported input.
- **Shipped breadth — acyclic named source re-exports:** the archived
  [`2026-08-08 sprint`](../archive/sprint-2026-08-08-acyclic-source-reexports.md) publishes
  `export { x } from` plus aliases, outer/inline type-only variants, and chains by directly
  projecting existing value/type slots. It preserves missing-export diagnostics, deterministic
  order, and the rule that the re-export does not create a local barrel binding.
  Namespace-bearing targets remain explicit unsupported input. The public summary classifies an
  admitted re-export as resolved only after both frontend and checker semantics run.
- **Shipped breadth — direct default slot:** the archived
  [`default-slot sprint`](../archive/sprint-2026-08-08-default-module-slots.md) publishes direct
  default classes/functions, expressions, and namespace-free identifiers through a separate slot.
  Direct and type-only default imports read only that slot; missing defaults report `TK1192`.
  Named/default bridges, mixed imports, namespace-bearing producers, and duplicate parity remain
  explicit non-clean input.
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

Both the small preview witness from `72` and a later full-stack witness must run under the supported
Bundler oracle profile. `contember/deptective` remains a candidate only if its pinned
witness can use a Bundler-compatible config without changing the program's type meaning; otherwise
select a replacement instead of claiming NodeNext support. The witness pins its lockfile/tool
versions and the checker-wide TypeScript oracle. The ambient-library half shipped with archived
backlog [`14`](../archive/backlog-14-libdts-loading.md); this item owns the remaining resolver,
module-graph, import/export, and declaration-package breadth.

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

Namespace/star and default-bridge source publication, `oxc_resolver` option mapping, broader
project/config root enumeration, deterministic module graph, package/declaration loading,
`.d.ts` checking, the serial `check_project` path, and project accounting. Cross-file identity and
`driver::check_files` are backlog `16`.

<!-- Origin: dev roadmap (was HANDOFF §3, long-term scale + IDE). -->

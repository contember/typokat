---
id: 72
title: Real-project preview readiness
blocked-by: []
---

# 72 — Real-project preview readiness

**Summary.** Deliver the first honest vertical slice where `typokat` can be pointed at a
real strict TypeScript project and produce useful, differential output rather than resolver,
ambient-library, or silent-skip noise. This is an early preview gate, not the full resolver
owned by backlog [`15`](./15-modules-imports.md). The preview uses the supported 1.0
`moduleResolution: "bundler"` profile and delegates physical resolution to `oxc_resolver` per
[`ADR-0007`](../decisions/0007-bundler-resolution-via-oxc-resolver.md).

## Problem

The checker can run over explicitly supplied local `.ts` files, but that is not yet the user
workflow. A normal project starts from a directory or `tsconfig.json`, may use Bundler-profile
extension substitution and package declarations, and depends on ambient library names. Running
today's checker on such a project mixes genuine checker diagnostics with resolution/prelude noise;
unsupported AST paths can also make a clean result untrustworthy.

The surface-accounting prerequisite shipped on 2026-07-12: every inventoried in-scope surface
now either produces an incomplete record or has a durable semantic owner. This item is therefore
unblocked; it owns the remaining project-level accounting and preview promise.

Without a pinned project-level witness, the model and resolver roadmaps can turn green without
proving the concrete preview promise: "point typokat at a project and understand the result."

## Approach / acceptance

Build one deliberately narrow, reusable vertical slice:

- expose and document a public CLI form for a project directory or `tsconfig.json`, discover the
  config and its root `.ts` files from `files`/`include`/`exclude`, and report the selected
  config/root set. Use `oxc_resolver` for config discovery/resolution and enumerate source roots in
  typokat where the crate does not expose a complete root set; every unconsumed config form is an
  explicit unsupported notice;
- configure `oxc_resolver::resolve_dts` for Bundler resolution and support the import forms used by
  the witness in one serial type universe. A dependency-free/local-only project is valid for this
  early gate; if WU0 selects one trivial package, only its pinned declaration path enters the slice.
  General package/`@types`/declaration-layout coverage stays in `15`, and no local fallback resolver
  is added;
- consume the minimal ambient declarations needed by the witness through the shipped canonical
  prelude path; do not add an unrelated hard-coded global-name shim;
- distinguish type diagnostics from explicit unsupported-surface notices, and never represent an
  unvisited in-scope AST form as a clean check (the shipped surface inventory enforces the
  systematic guarantee);
- emit a deterministic project summary suitable for differential comparison: roots checked,
  files checked/skipped, unsupported forms, unresolved modules, and diagnostics by code/file.

WU0 must select and pin a genuinely small, public strict-TypeScript project whose ambient and
language surface fits the shipped model plus the shipped bounded prelude. Record its repository URL,
commit, lockfile digest, install command, tsconfig, TypeScript oracle version, and every exercised
resolver/prelude feature before implementation. Reject candidates that require broad Node/Bun
declarations, a non-Bundler resolution profile, generic standard-library methods, or a large package
graph: the preview must not silently expand the bounded prelude into a partial `lib.d.ts` or a
project-specific shim.

`contember/deptective` commit `e953c79edc395f8933afaba3ad5b0c57c6afd676` remains a **later
full-stack witness candidate**, not the preview witness. Its repository config uses NodeNext,
package `.d.ts`, Node/Bun ambient declarations, `Promise`, `Map`, `Set`, and `JSON`; backlogs `14`
and `15` own making that project meaningful after full lib/resolver support. It qualifies only
through a Bundler-compatible witness config that preserves the program's type meaning; otherwise
replace it rather than claiming NodeNext support. Its witness uses the checker-wide pinned 6.0.3
oracle.

Acceptance requires all of the following:

1. every configured source root is deterministically accounted for as checked or explicitly
   unsupported; every admitted Bundler specifier resolves through `oxc_resolver`, and there are no
   unclassified package-resolution failures or local fallback probes;
2. the clean pinned-`tsc --noEmit` baseline produces no unclassified typokat type diagnostic; any
   deliberate over-report or unsupported notice is linked to a live backlog/divergence owner and
   appears separately from actionable type errors;
3. a committed mutation pack injects representative assignability, call-argument, and missing-
   member errors into the pinned checkout; typokat reports the expected `TK` identities at the
   mutated sites and the differential artifact records any remaining tsc misses;
4. repeated runs produce byte-identical normalized results, and the committed baseline is a
   no-regression ratchet by diagnostic identity, unresolved-module identity, and unsupported-form
   identity — aggregate counts alone are insufficient.

The spec-only commit that precedes implementation must record the clean baseline, mutation list,
the allowlist mapping every accepted mismatch/unsupported identity to its backlog or divergence,
and numeric preview thresholds (maximum actionable false positives, unresolved modules, skipped
files/forms, and missed seeded diagnostics). Implementation may improve those thresholds but may
not relax them. CI runs the public CLI through the same runner and checks the committed scoreboard.

This item may land the narrow `oxc_resolver` integration needed by the preview witness. Backlog `15`
remains the owner of general supported-Bundler breadth and typokat-owned semantics: all admitted
import/export forms, project enumeration/accounting, declaration consumption, module graphs, and
cycle behavior beyond the pinned slice.

## Touch points

`src/main.rs`, `crates/typokat-frontend/src/frontend.rs` for resolver/config/source discovery,
`crates/typokat-driver/src/driver.rs` for orchestration/reporting, deterministic module accounting,
the ambient prelude path, a checked-in real-project smoke runner/descriptor, and CI. Full Bundler
breadth and the later full-stack resolver witness are
backlog `15`; full `lib.d.ts` and the later ambient
witness are backlog `14`; cross-file parallel identity is backlog `16`.

<!-- Origin: post-sprint MVP-readiness audit, 2026-07-10. -->

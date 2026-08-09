---
id: 72
title: Real-project preview readiness
blocked-by: []
---

# 72 — Real-project preview readiness

**Summary.** Pin one qualifying public strict-TypeScript project, prove the shipped bounded Bundler
CLI against clean and mutated sources, and add a deterministic differential ratchet. Project
discovery, local physical resolution, complete module accounting, and the public summary are
already shipped. General module breadth remains backlog [`15`](./15-modules-imports.md).

## Problem

The public command now accepts a directory or `tsconfig.json`, but its exact supported profile is
deliberately small: files-only strict/noEmit/ESNext/Bundler, configured local `.ts` roots, local
named and regular direct default imports, acyclic local named source re-exports, and direct default
producers through a distinct slot.
`oxc_resolver 11.24.2` handles extensionless and `.js`→`.ts` physical resolution. The deterministic
JSON summary accounts for every root, resolution, unsupported form, parse error, incomplete surface,
and diagnostic. Unsupported input is non-clean rather than silently filtered.

Two bounded screening sprints failed the unchanged witness gate. The latest shipped the substrate
first, then screened six immutable public projects. None qualified: common first failures were
source re-exports and default exports; one used explicit `.ts` specifiers under NodeNext; others
required Node ambient types or non-equivalent config/test exclusions. Every candidate failed before
the mutation gate. The named source-re-export blocker is now removed for the shipped bounded form.
A follow-up immutable re-screen at `659e30e` still found no qualifier: `jabr` stopped on a cycle,
bare `..`, and a default export; `placetext` stopped on one default-export route notice before
semantic checking; `lite-fp`, `un-jinja`, and `south-african-id` reached checker model or diagnostic
failures; and `deco` retained explicit-`.ts` specifier incompatibility. The shipped direct
default-slot slice removes `placetext`'s historical route blocker, but qualification and the new
first blocker are unknown until the mandated fresh re-screen. The transparent
`un-jinja` and `south-african-id` production-subtree overlays exclude native tooling/tests that
consume `@types/node`, so they are not exact native programs. Exact evidence is retained in the
[`re-screen archive`](../archive/real-project-rescreen-2026-08-08/README.md). There is still no
pinned public baseline, mutation pack, ratchet, or CI promise.

## Approach / acceptance

Finish only the witness and durable gate:

- select a genuinely small, immutable public project whose complete production graph fits the
  supported route. Record repository, commit, license, lockfile digest, package-manager/tool
  versions, native config, exact witness config, roots, graph, module forms, and ambient names;
- require a clean pinned `tsc 6.0.3 --strict --noEmit` Bundler oracle and a zero-clean production
  summary: no actionable diagnostics, unresolved modules, skipped roots/forms, project notices,
  parse errors, or incomplete surfaces;
- reject the project if it needs a source edit, ambient shim, fixed-library change, non-Bundler
  profile, type-checking package, Node/Bun ambient dependency, unsupported form, or a config overlay
  that changes the program's type meaning;
- after the clean baseline only, apply isolated assignment (`2322`), call-argument (`2345`), and
  missing-member (`2339`) mutations. Both `tsc` and typokat must report the exact identities at the
  mutated sites, with full restoration between probes;
- commit an immutable descriptor, mutation manifest, normalized expected summary, and fresh-cache
  runner. Run it twice, seed runner faults, and add the identity-based ratchet to CI.

`contember/deptective` commit `e953c79edc395f8933afaba3ad5b0c57c6afd676` remains a **later
full-stack witness candidate**, not the preview witness. Its repository config uses NodeNext,
package `.d.ts`, Node/Bun ambient declarations, `Promise`, `Map`, `Set`, and `JSON`. Archived
backlog [`14`](../archive/backlog-14-libdts-loading.md) supplies the fixed default library; backlog
`15` owns the remaining resolver and ambient-package breadth needed to make that project meaningful.
It qualifies only
through a Bundler-compatible witness config that preserves the program's type meaning; otherwise
replace it rather than claiming NodeNext support. Its witness uses the checker-wide pinned 6.0.3
oracle.

Acceptance still requires all of the following:

1. every configured source root is checked; every admitted Bundler specifier resolves through
   `oxc_resolver`; all unsupported, unresolved, skipped, parse, incomplete, and diagnostic channels
   are empty on the clean project;
2. the pinned `tsc --noEmit` baseline is clean and the exact source/config meaning is preserved;
3. a committed mutation pack injects representative assignability, call-argument, and missing-
   member errors into the pinned checkout; typokat reports the expected `TK` identities at the
   mutated sites and the differential artifact records any remaining tsc misses;
4. repeated runs produce byte-identical normalized results, and the committed baseline is a
   no-regression ratchet by diagnostic identity, unresolved-module identity, and unsupported-form
   identity — aggregate counts alone are insufficient.

The descriptor/spec commit must precede the runner. Thresholds stay exactly zero; there is no
allowlist for a candidate-specific unsupported form. If no project fits after the next bounded
screening pass, stop and leave this item incomplete. Do not broaden production semantics inside the
witness sprint; land required general forms spec-first under backlog `15` first.

## Touch points

Checked-in project descriptor, mutation manifest, normalized expected result, fresh-cache
black-box runner, README, and CI. The production project substrate changes only through backlog
`15`. The shipped full `lib.d.ts` profile is fixed by archived backlog
[`14`](../archive/backlog-14-libdts-loading.md); cross-file parallel identity is backlog `16`.

<!-- Origin: post-sprint MVP-readiness audit, 2026-07-10. -->

+# Placetext enum-model re-screen evidence — 2026-08-13

This directory preserves the immutable WU6 re-screen required by the
[enum type/value sprint](../sprint-2026-08-12-enum-type-value-model.md) and backlog
[72](../../backlog/72-real-project-preview-readiness.md). The shipped enum model removes every
previous enum and module-recovery identity. Two general safe-direction cascades become visible, so
the project still does not meet the unchanged zero-clean gate. No source, configuration, library
profile, threshold, mutation, witness descriptor, or CI ratchet changed.

## Identities

- Reviewed repository HEAD: `41fe6c3cdfc26a7bd835627e0b35535383f8f87c`.
- Release binary SHA-256:
  `d38901f223971062ad1f45f5dbeb8dc25ed313b2ff25cfd578d1b9ee0b1d9102`.
- `library-info`: schema 2, 82 files, `production-complete-source-once` check route,
  `production-default-library` provider route, profile SHA-256
  `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`.
- Oracle: `tsc` from the active pinned toolchain, version `6.0.3`.
- Canonical project: `https://github.com/lokicik/placetext.git` at
  `faf233107146ceca63bf8a6fec8f07ad43ab17e2`.
- `LICENSE` SHA-256:
  `52578f8c669574581e8a046ee80ec13827c006dc173af8f28621449516a52633`.
- `package-lock.json` SHA-256:
  `6632ffc7fce92584a119fbce40647358703915a93f4ba382a8c90e61278642b0`.
- Native `tsconfig.json` SHA-256:
  `8bb71db4b3c601ab302873d436378906d226b08dbc4b717d6b588d0b8751c17a`.
- Files-only screening config SHA-256:
  `cbe3f9706b9c61005ed46792a64588c68a23228417a3eab68a729cd7a425dbfe`.
- Normalized nine-source manifest SHA-256:
  `c947d9a2b1530f650b7d8874f30a2396816ed1308167540871e45902d325a535`.

The checkout was detached at the pinned commit, clean, and had no Git alternates, worktree
redirect, shared object cache, or inherited repository-related Git environment. The native command
was `tsc -p repo/tsconfig.json --noEmit --pretty false`; the overlay command was
`tsc -p tsconfig.json --pretty false`. Both exited 0 with empty output. Two typokat runs each
exited 3 and produced byte-identical complete output with SHA-256
`1171e6e9e39d7abf15454c8521ddae5fe40476a48b85a712115b92ebb87cc7f3`.
The normalized JSON has SHA-256
`6b96dbd082f3cf92f266d2bb87078207d5c9369472dce4fa00d6bff302e5516e`.

## Result

The exact normalized output is [`placetext.json`](placetext.json).

| Channel | Result |
| --- | --- |
| Roots | 9 |
| Checked / skipped / excluded | 9 / 0 / 0 |
| Successful local resolutions | 13 |
| Project notices / parse errors | 0 / 0 |
| Incomplete records / diagnostics | 6 / 4 |

All four enum-owned identities from the
[2026-08-11 predecessor](../real-project-rescreen-2026-08-11/README.md) disappear:
`repo/src/core/types.ts:53:8 decl/enum-declaration/self`,
`repo/src/core/types.ts:68:12 TK2304`, and both `Corpus` `TK2305` records at
`repo/src/index.ts:8:10` and `:19:10`. No enum or module-recovery identity replaces them.

The six incomplete records are the five existing computed object keys and one template
interpolation. The two existing predicate-array `TK2339` records remain. Correctly materializing
`Record<Corpus, string[]>` exposes `repo/src/index.ts:23:7 TK2741`: backlog `75` deliberately
does not infer the five computed source keys, so the source object cannot prove the required
properties. Missing `Array.isArray` predicate narrowing exposes the downstream
`repo/src/index.ts:75:22 TK2322`; backlogs `50` and `107` own that predicate/apparent-array
surface. Both are safe-direction general cascades, not enum-model recovery.

The native config selects ES2020 while the screening config uses typokat's fixed full ES2025 host
library. Both programs are clean under tsc, but that does not prove identical type meaning. Backlog
`72` remains open at the unchanged zero threshold. No mutation, witness, or CI work started. An
independent reviewer rebuilt the current tree, reproduced the six/four identity set, checked the
two cascades against tsc 6.0.3, verified pending-enum fail-closed behavior and a two-file imported
interface control, and returned **PASS**.


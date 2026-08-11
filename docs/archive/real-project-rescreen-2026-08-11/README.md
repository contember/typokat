# Placetext optional-parameter re-screen evidence — 2026-08-11

This directory preserves the immutable WU3 re-screen required by the
[`optional-parameter sprint`](../sprint-2026-08-11-optional-parameter-undefined-arguments.md) and
backlog [`72`](../../backlog/72-real-project-preview-readiness.md). The shipped call-boundary rule
removes the measured optional-parameter diagnostic without changing the project route or witness
threshold. The project still does not meet the zero-clean gate, so no mutation, witness, or CI work
started.

## Identities

- Reviewed repository HEAD: `ab02d3dbf809fc66d1862e7fc659e8ff86b9449f`.
- Release binary SHA-256:
  `eaac0ad9a8697facd0f2096402c2094981adbb42fce12c9252e80cf746892be6`.
- `library-info`: schema 2, 82 files, `production-complete-source-once` check route,
  `production-default-library` provider route, profile SHA-256
  `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`.
- Oracle: `/run/user/1000/fnm_multishells/1002937_1784884227968/bin/tsc`, version `6.0.3`.
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
redirect, shared object cache, or inherited repository-related Git environment. The canonical
native oracle command was `tsc -p repo/tsconfig.json --noEmit --pretty false`; the overlay command
was `tsc -p tsconfig.json --pretty false`. Both exited 0 with empty output. The explicit
`--noEmit` is required for the native oracle: an unoverridden emit run reaches TypeScript 6.0's
`TS5011` `rootDir` migration check and is not the canonical comparison. Two typokat runs each
exited 3 and produced byte-identical JSON with SHA-256
`49c9b3f69d3a2608bba59acd68db5e6d8d41ea96d6a90fd0dc05ccbebe83a649`.

## Result

The exact normalized output is [`placetext.json`](placetext.json).

| Channel | Result |
| --- | --- |
| Roots | 9 |
| Checked / skipped / excluded | 9 / 0 / 0 |
| Successful local resolutions | 13 |
| Project notices / parse errors | 0 / 0 |
| Incomplete records / diagnostics | 7 / 5 |

The exact `repo/src/core/generator.ts:36:48 TK2345` identity from the
[`2026-08-10 re-screen`](../real-project-rescreen-2026-08-10/README.md) has zero survivors. All five
independent diagnostics remain unchanged: one `TK2304` and two `TK2305` records for `Corpus`, plus
two `TK2339` records on the predicate-bearing `Array.filter` / `Array.isArray` surface. No new
diagnostic appeared. The first remaining normalized diagnostic is the enum-owned
`repo/src/core/types.ts:68:12 TK2304`; backlog `42` remains its general model owner. The seven
incomplete records are also unchanged: one enum, five computed object keys, and one template
interpolation.

The native config selects target/library ES2020 while the screening config uses typokat's fixed
full ES2025 host library. Both programs are clean under `tsc`, but that does not prove identical
type meaning. Backlog `72` therefore remains open at the unchanged zero threshold. An independent
reviewer verified provenance, reproduced both typokat runs byte-for-byte, compared every old
diagnostic and incomplete identity, and returned **PASS**.

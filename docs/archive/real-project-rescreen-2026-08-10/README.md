# Placetext object-binding re-screen evidence — 2026-08-10

This directory preserves the immutable WU3 re-screen required by the
[`object-binding publication sprint`](../sprint-2026-08-09-object-binding-publication.md) and
backlog [`72`](../../backlog/72-real-project-preview-readiness.md). The shipped flat object-variable
binding slice removes the measured unresolved-name cascade without changing the project route or
the witness threshold. The project still does not meet the zero-clean gate, so no mutation,
witness, or CI work started.

## Identities

- Reviewed production source commit: `8638c4b725aea7ff68d39804d78a2a3db5d7ced3`.
- Screening repository HEAD: `066ec94311526159b6757331900db87e54d019d8`; the intervening commit
  changed only the official-suite scoreboard and divergence/backlog documentation.
- Release binary SHA-256:
  `628daee6cf1d55b6a96dd51e8aeb8cb8261f3661b545a72af552826fd72ace25`.
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

The checkout was detached at the pinned commit, clean, and had no Git alternates. Both native and
overlay `tsc` runs exited 0 with empty output. Two typokat runs each exited 3 and produced
byte-identical JSON with SHA-256
`ed646172491a6aef5ddc9b3ff7e6037dc93a06b557762b417cea956464ffd9cb`.

## Result

The exact normalized output is [`placetext.json`](placetext.json).

| Channel | Result |
| --- | --- |
| Roots | 9 |
| Checked / skipped / excluded | 9 / 0 / 0 |
| Successful local resolutions | 13 |
| Project notices / parse errors | 0 / 0 |
| Incomplete records / diagnostics | 7 / 6 |

All exact 22 object-binding `TK2304` identities from the
[`2026-08-09 re-screen`](../real-project-rescreen-2026-08-09/README.md) have zero survivors. Five
independent diagnostics remain unchanged: one `TK2304` and two `TK2305` records for `Corpus`, plus
two `TK2339` records on the predicate-bearing `Array.filter` / `Array.isArray` surface. The seven
incomplete records are also unchanged: one enum, five computed object keys, and one template
interpolation.

The newly visible first diagnostic is `TK2345` at `src/core/generator.ts:36:48`. The `seed` binding
is now correctly published as `number | undefined`, but call argument target construction checks it
against the raw `number` type of `createRandom(seed?: number)`. Backlog
[`109`](../../backlog/109-optional-parameter-undefined-argument.md) owns this general bug in optional
parameter acceptance.

The native config selects target/library ES2020 while the screening config uses typokat's fixed
full ES2025 host library. Both programs are clean under `tsc`, but that does not prove identical
type meaning. Backlog `72` therefore remains open. An independent reviewer verified provenance,
reproduced both typokat runs byte-for-byte, compared every old diagnostic identity, and returned
**PASS**.

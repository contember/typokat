# Placetext re-screen evidence — 2026-08-09

This directory preserves the exact post-default-slot read-only re-screen required by the
[`default-module-slot sprint`](../sprint-2026-08-08-default-module-slots.md) and backlog
[`72`](../../backlog/72-real-project-preview-readiness.md). The immutable project now passes the
bounded module route, but it does not meet the unchanged zero-clean witness gate. No source,
configuration, library, production behavior, threshold, mutation, witness descriptor, or CI
ratchet changed.

## Identities

- typokat HEAD: `ca13c30f3b9a558ef3c6a959a22f44750f5cbe2d`.
- Release binary SHA-256:
  `c2e7e7d00d31acbc3cacc3261c54dc99001d28d073a439e8df4c245dc73e0506`.
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

The checkout was detached at the pinned commit, clean, and had no Git alternates or shared object
cache. The first reproduction used a newly empty root. The fixed gate failed there, so the
procedure forbade a second cache.

## Commands and result

The command shapes were:

```sh
(cd "$ROOT/repo" && "$TSC" --pretty false --strict --noEmit -p tsconfig.json)
"$TSC" --pretty false -p "$ROOT/tsconfig.json"
"$BIN" check --project-summary json "$ROOT/tsconfig.json"
```

Both `tsc` commands exited 0 with empty stdout and stderr. Typokat exited 3. Its exact JSON is
[`placetext.json`](placetext.json), SHA-256
`3245508373f2bc54bd878e746fc56aaef1bb6024ee5e9c0f3721ba10f904c8d9`.

| Channel | Result |
| --- | --- |
| Roots | 9 |
| Checked / skipped / excluded | 9 / 0 / 0 |
| Successful local resolutions | 13 |
| Project notices / parse errors | 0 / 0 |
| Incomplete records / diagnostics | 7 / 27 |

The first rendered diagnostic is `TK2304` at `src/core/generator.ts:36:48` for `seed`. Object
destructuring does not publish its binding leaves; this causes 22 of the 23 `TK2304` records. It is
the first blocker, not the only blocker. The remaining independent families are:

- one enum incomplete plus one `TK2304` and two `TK2305` records for `Corpus`;
- five computed object-key incomplete records;
- one template-interpolation incomplete record;
- two `TK2339` records for `Array.filter` and `Array.isArray`, tied to the predicate-bearing array
  surface.

The native config selects target/library ES2020. The screening config selects typokat's fixed full
ES2025 host library. Both programs are clean under `tsc`, but that does not prove that the two
library universes preserve identical type meaning. Target/library equivalence therefore remains
open.

An independent reviewer reproduced byte-identical exit and output identities, verified every
channel and checkout property, audited the diagnostic causality, and returned **PASS** with no
HIGH, MEDIUM, or LOW finding. Because the first-cache zero-clean gate failed, mutation probes,
witness files, and CI work did not start.

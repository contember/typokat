# Sprint — real-project witness re-screen (2026-08-08)

**Goal.** Re-screen the same six immutable public candidates against the expanded production
Bundler route. If one candidate meets every unchanged zero threshold, pin it as backlog `72`'s
public witness and add the mutation and CI ratchet. Otherwise stop, archive this sprint incomplete,
and file the exact next general backlog-`15` blocker. This sprint does not broaden production
semantics.

**Baseline.** Planning starts at HEAD `1832d74`; the relevant production source-re-export cutover
is `daaad0c`. The route accepts files-only strict/noEmit/ESNext/Bundler projects with local named
imports and acyclic local named source re-exports over namespace-free value/type slots.

## Immutable candidate set

Use these exact repository commits and recorded integrity identities. The four immutable identities
are the canonical remote URL, commit, license file path plus SHA-256, and lockfile path plus
SHA-256. WU0 records the exact paths and canonical remote for every row; any later descriptor must
repeat them. Do not substitute a newer revision, edit sources, add ambient shims, or change the
fixed library.

| Candidate | Commit | License SHA-256 | Lock SHA-256 |
| --- | --- | --- | --- |
| `morkg/jabr` | `9415fdad8b98dc0f1aba09c8badc5fc209bc30ba` | `1256366f990b3fa2b0780d082cae641a126c50cd5fdbe77acff3b45acfe056c2` | `1825f799ba12dee085a2da8ef33768efe9fe98bb0827ab9be7af384cf87070a5` |
| `lokicik/placetext` | `faf233107146ceca63bf8a6fec8f07ad43ab17e2` | `52578f8c669574581e8a046ee80ec13827c006dc173af8f28621449516a52633` | `6632ffc7fce92584a119fbce40647358703915a93f4ba382a8c90e61278642b0` |
| `naoeosavio/lite-fp` | `09865973c3599928df272fc6f79c9daf9a955bc5` | `4bc6a360f7bab8b5c4b175bc24751c931f27bc5b4196a4cf8709fa9f624514d9` | `1a871ec1ddd676215fa2e125940c7d3cc9fc503fe03f8f68b475174c98d9e4d9` |
| `jacob-bennett/deco` | `daa5feaa886de0727807aa12ea6ff2f4d7841f60` | `7530c8d9c1f25c7b5b85bca3b75db0165c0f2893d2736b5ffa615ce1786bd290` | `09ec03451feff0df14edf3deab3b8929fbd37e97745d09aeea5dec0361112616` |
| `theetherGit/un-jinja` | `d43537ec4611e694528899dbfb97cbdc4b24b86c` | `3822d9bb8c5f39a4a07939371ff72adbbba20fade6c202523f21cfc7f3ef01b7` | `b9bdf32d064348ff28df2a23121bb6e7d0fae9d3b85df596fde8cec5322d4559` |
| `SiphoChris/south-african-id` | `4e8ab8ac4e6bd8109983a7db6adbf39a3c422a61` | `eaa832a918a94cc080c2d2edf5b7b83b64a44a74797e1e99635bb0f8d2c5b727` | `3c2469deb9494cfee4cd8fefc4a74b920de81ba6b4ff85b4334d49e44805cbdf` |

## Fixed gate

A candidate qualifies only if its complete, meaning-preserving Bundler program has both:

- a clean pinned `tsc 6.0.3 --strict --noEmit` oracle; and
- a zero-clean production JSON summary: no actionable diagnostics, unresolved modules, skipped
  roots or forms, project notices, parse errors, or incomplete surfaces.

It must contain at least two configured source files and one local named-import edge. Reject it if
the checked program depends on a type-checking package or on Node or Bun ambient declarations.

The threshold is exactly zero in every channel. Native configuration and any transparent screening
overlay remain separate evidence. An overlay that changes the program's type meaning cannot qualify
the candidate. A candidate-specific shim, allowlist, checker branch, library change, semantic
expansion, or threshold relaxation is forbidden.

Use the pinned oracle at
`/run/user/1000/fnm_multishells/1002937_1784884227968/bin/tsc`. A fresh-cache reproduction uses a
newly created empty candidate-local root. Checkout, dependency/install state, generated output, and
runner output may not be shared with any other reproduction. The two required reproductions use
two distinct empty roots; no Git object cache or other cache is shared between them. Every run
records the exact command, tool versions, canonical remote, commit, license and lockfile paths and
digests, roots, graph, module forms, ambient names, exit status, and normalized output. Build a
fresh release binary from the current commit before screening and record its commit and SHA-256;
candidate results from a stale or debug binary are invalid. Run CPU-heavy commands through
`cpu-lease run -n 2 -- ...`; serialize every Cargo invocation through
`flock -w 3600 /tmp/typokat-perf.lock -c '...'`. Any benchmark also uses `--no-smt`, though this
sprint has no performance gate.

## Work units

### WU0 — read-only six-candidate re-screen (effort S, hard stop)

- Verify all four immutable identities for each candidate before execution. Reproduce the archived
  native and overlay distinction, then run pinned `tsc 6.0.3` and the freshly built release binary
  from the current sprint HEAD from fresh caches. Record that HEAD and verify its production
  semantics still include `daaad0c`. Screen `morkg/jabr` first, followed by the other five rows in
  table order.
- Do not edit production code, candidate sources, configs, library inputs, tests, or thresholds.
  Record every failure by its first exact general unsupported/model surface and keep all non-clean
  channels visible.
- If no candidate qualifies, add the precise smallest general blocker to backlog `15`, record the
  six results, archive this sprint as incomplete, and leave backlog `72` open. Do not start WU1.
- If a candidate qualifies, record its clean oracle and production result twice from independent
  fresh caches, then continue to WU1.

### WU1 — commit the witness descriptor and mutation spec (effort S)

- Commit separately an immutable descriptor with canonical remote, commit, exact license and
  lockfile paths and digests, source/config digests, exact roots and graph, normalized zero-clean
  baseline, and a disabled mutation manifest for isolated assignment `2322`, call-argument `2345`,
  and missing-member `2339` probes.
- Cross-check each proposed mutation against `tsc 6.0.3`. Commit only the disabled runner contract
  and disabled fault cases for changed identity, dropped root, changed target, corrupt cache,
  unexpected exit, stale baseline, and unknown stderr. The runner does not exist yet, so forced RED
  execution and later green execution belong to WU2. No production behavior changes in this
  commit.

### WU2 — implement the fresh-cache runner and CI ratchet (effort M)

- Assign implementation to a subagent with ownership limited to the new project-preview tooling,
  its focused tests, documented command, and the CI invocation. A build error outside those files
  is another worker's in-progress change and must be reported, not fixed.
- Before enabling the contract, force every WU1 case and prove it is RED because the runner is
  absent. After implementation, enable it and prove the same contract is green.
- Fetch only the pinned checkout into a fresh candidate-local cache, verify all identities, restore
  the clean tree between mutations, run both checkers, and ratchet exact diagnostic, unresolved-
  module, unsupported-form, root, and resolver-target identities. Two normalized clean runs must be
  byte-identical.
- Negative controls must independently fire for missing root, changed resolver target, dropped or
  same-count-swapped mutation diagnostic, corrupt cache, commit/license/lock digest mismatch,
  unexpected exit, stale baseline, and unknown stderr. The corrupt-cache control modifies an
  isolated copy of the runner's downloaded candidate source inside its candidate-local root; the
  runner must reject its source or integrity digest before invoking either checker. A failed
  control blocks the work unit.

### WU3 — independent adversarial and fault review (effort M)

- A different subagent reviews the complete WU1-WU2 diff without relying on the implementing
  agent's conclusions. It must reproduce the clean oracle, all three mutations, both fresh runs,
  and every fault control.
- The review hunts false-clean root loss, config-meaning drift, hidden unsupported forms, resolver
  target drift, wildcard/count-only allowlisting, diagnostic identity swaps, incomplete cache
  invalidation, and any project-specific production path. Cross-check semantic results with pinned
  `tsc 6.0.3`.
- Any HIGH or MEDIUM finding returns to WU2 and requires a fresh independent review.

### WU4 — leader gates, public docs, and archive (effort S)

- The leader verifies the exact staged paths before each commit and runs focused runner tests, all
  negative controls, two fresh-cache reproductions, a fresh current-commit release build with its
  binary hash recorded, full workspace tests, formatting,
  `cargo clippy --all-targets -- -D warnings`, the official-suite ratchet, and docs lint.
- On success, update the public bounded claim, close backlog `72`, record the commit map and exact
  identities, and archive this sprint. If a late gate invalidates the witness, archive incomplete
  and leave backlog `72` open; do not weaken the gate.

## Ownership and sequence

WU0 is read-only and must finish before any witness files are created. WU1 is the separate spec
commit. WU2 starts only after WU1 lands. WU3 uses a different agent. The leader owns verification,
commits, sprint/backlog/index updates, and archival. The sequence is strictly WU0 → WU1 → WU2 →
WU3 → WU4; the no-qualifier branch is WU0 → WU4 incomplete closure.

## Out of scope

- New module syntax, package or ambient-package resolution, default/namespace/star forms, cycles,
  declaration-file breadth, NodeNext behavior, or checker/type-model fixes.
- Candidate substitutions, source/config edits, ambient shims, library changes, allowlists,
  production special cases, threshold changes, and performance claims.
- The later `contember/deptective` full-stack witness.

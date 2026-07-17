# TypeScript 6.0.3 full-library WU0B evidence

This directory is the RED acceptance contract for the measurement-only WU0B prototype selected by
ADR-0011. It is not evidence that the prototype exists, that the full library compiles, or that the
five-second/512-MiB gate passes. All manifests intentionally start as `PENDING`.

Ordinary tests in `tests/lib_es2025_full_readiness.rs` validate the helper contract, exact schemas,
cross-file binding, and the deliberately pending placeholder state; they do not require completed
evidence. Its separate final evidence test is ignored until an independently reviewed release run
has replaced every pending field. Running that ignored test today must fail; changing only a verdict
to `GO` must also fail because the test recomputes the verdict from the underlying rows.

The files have separate responsibilities:

- `readiness.toml` binds the profile and evidence files, records raw phase/cold/warm/private/fanout
  measurements plus the 1/2/32-check shared-base identity evidence, and carries the recomputed
  GO/NO-GO decision. Warm percentiles are derived from the raw arrays rather than trusted.
- `ledger.toml` is the complete parser/declaration/outcome census. Every file SHA and per-file count
  is checked against the pinned profile and an independent OXC source-occurrence census. Event keys
  use library file ordinals only; user `ModuleOrdinal` and `UnitSlot` fields are forbidden.
- `bridges.toml` inventories every approved bridge candidate, its exact declaration identity/span,
  universe-local identity role, all current consumers, shadowing/missing-member/shared/private
  hash-bound witness files, strict-tsc controls, rationale/falsifier, and
  required/not-required/deferred decision. The exact reviewed matrix must later be pinned in code.
- `routing.toml` records every committed conformance and official-suite invocation, collected roots,
  conservative preflight categories, expected/observed route, nonzero row timings, and matching
  observed/forced-private output artifacts. Official inputs are checked against the ignored pinned
  corpus, not merely the scoreboard names; source counts follow `tsofficial.py::parse_units`
  `@filename` grouping. COMPLETE-only raw outputs must be distinct regular, non-symlink TOML files
  under `raw-routing/`, with exact checker/input/invocation/mode bindings and semantic-output hashes.
  Their typed payload also binds source count, collected roots, categories, candidate count, actual
  route, and elapsed time. `projected_ms` is derived from the relevant raw execution; it is not an
  independently editable estimate. The domain-separated canonical evidence digest sorts routes by
  corpus/invocation and covers root metadata, corpus provenance, every decision field, and both raw
  files. On Unix, files are opened from a no-follow root through retained `openat` descriptors and
  read from the fstat-validated final handle with a fixed size cap; non-Unix COMPLETE fails closed.
- `conformance-invocations.toml` captures the exact single-file/project grouping and enabled universe
  produced by `tests/conformance.rs`. It binds the harness source and every grouped source by SHA;
  its canonical digest still requires an independent code approval.

The eventual evidence commit must be generated from one release build at the recorded checker
revision. It must retain raw runner output outside these summary manifests for review; these files
are a deterministic acceptance index, not a place to hand-enter plausible numbers. The benchmark
protocol requires five fresh release processes under GNU `/usr/bin/time -v`, followed by one
same-process warm run and bounded private/all-colliding runs. No threshold may be relaxed in these
manifests. Private-wall, benchmark-sample, fanout-size, projected/observed routing limits,
conformance grouping, official source provenance, and bridge-matrix approvals are deliberately
`None` constants in the validator. The canonical routing evidence manifest is likewise code-pinned
and initially `None`; its digest is also stored in the exact approval row. A later, separate
leader-review commit must pin them after measurement; until then GO is unreachable by construction.
Ordinary RED tests use injected official-source controls and do not require the gitignored corpus;
only COMPLETE final validation reads it.

Manual final gate:

```sh
cargo test --test lib_es2025_full_readiness \
  wu0b_completed_evidence_satisfies_adr_0011 -- --ignored --exact
```

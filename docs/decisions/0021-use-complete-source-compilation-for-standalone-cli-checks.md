---
id: 0021
title: Use complete-source compilation for standalone CLI checks
status: accepted
date: 2026-08-05
---

# 0021 — Use complete-source compilation for standalone CLI checks

## Context

ADR-0011 and ADR-0020 optimize repeated checks in one process. They compile the exact 82-file
TypeScript 6.0.3 profile once, freeze its semantic product, and fork user deltas or private sparse
collision epochs from that shared base. This is the right lifecycle for the official-suite batch
and library API consumers that submit more than one isolated project.

It is the wrong lifecycle for the standalone CLI. `typokat check` handles one project and exits.
The authoritative WU6 run at `2d3a73f` showed that the shared route is faster than the pinned native
comparator for non-colliding rows, but collision and fanout are only about `0.57x`: the process first
builds the shared base, then both private rows ultimately take the complete-source fallback. The
active sprint forbids removing or relabelling those rows.

A guarded non-authoritative probe at `4694880` measured a different lifecycle. It starts from fixed
intrinsics, parses and binds the exact packaged library plus the user sources, retains no replay
product, and publishes semantics once. The validated evidence had SHA-256
`a2f1f0be310ec56952e42476a095ac7956407faa1fedceabfba5a9060c22c5ad` and reported:

| Row | one-pass / tsgo median speedup | p95 speedup | bootstrap lower bound |
| --- | ---: | ---: | ---: |
| fast-clean | 1.1457 | 1.1308 | 1.1188 |
| fast-errors | 1.1484 | 1.1709 | 1.1311 |
| collision | 1.1591 | 1.1316 | 1.1166 |
| fanout | 1.1364 | 1.2030 | 1.1200 |

Median resident memory was 83,986–84,470 KiB, or 0.84–0.85 of tsgo. The collision and fanout rows
were about `1.99x` faster than the current production route. This evidence is intentionally not an
authoritative product claim. It is `PROMISING` evidence sufficient to specify a production
candidate without a snapshot or cache; only an authoritative WU6 rerun can prove that candidate.

The probe implementation itself is not production-correct. It appends user modules with empty
dependency lists. On `m29_modules/basic_named`, it reports three false `TK2304` diagnostics instead
of resolving the import and reporting the expected `TK2322` and `TK2345`. It also has a different
parse boundary and filters library-owned records. Promoting that example would therefore trade a
performance success for soundness and route drift.

## Decision

### Give the standalone CLI an explicit complete-source lifecycle

Ordinary `typokat check` uses a production complete-source project route through a distinct
`check_project_once` driver entry point. The existing `check_source`, `check_files`, and
`check_project` APIs remain provider-backed. The cold route:

1. uses the existing production parser, relative-import resolver, dependency ordering, and original
   report-order mapping;
2. compiles the exact packaged 82-file profile and the resolved user project in one semantic
   universe from fixed intrinsics;
3. publishes library and user semantics exactly once and retains no collision replay plan;
4. runs on the existing 256 MiB check-worker stack; and
5. completes before any result is exposed.

The route may bind the library first and carry a `LibraryBinderCheckpoint` into user binding within
the same run. Resolved user declarations must join that binder before semantic publication starts.
The checkpoint is phase-local staging. It is not a frozen semantic base, persistent cache,
snapshot, cross-project identity, or second semantic publisher.

Every user parser diagnostic or panic stops before binding or semantic checking and returns the
ordinary production parse output. The library event ledger must complete and drain through the
same record path before its payloads are dropped. The ADR-0018 named census remains a suite gate and
must cover the new compiler path. Production neither exposes those records as user output nor
silently suppresses them by file ordinal as the probe example does.

Route selection is explicit by lifecycle. It never depends on whether a process-wide singleton
happens to be initialized, so call order cannot change semantics or performance.

### Keep the shared provider for repeated same-process consumers

The official batch protocol and the existing repeated-call driver API retain the ADR-0011/
ADR-0020 provider-backed route. They compile the library base once and isolate every user project.
The batch must not invoke the cold route once per frame. Cross-route semantic parity is a binding
gate, including B102/B103 merges in both file orders, imports, parse failures, diagnostics, and
incomplete records.

The CLI route and shared provider route receive distinct, truthful attestations. `library-info`
must describe both lifecycle routes; it may not use one `production-default-library` string to
imply that they execute the same pipeline.

This narrowly supersedes ADR-0011's claim that every ordinary public CLI consumer first acquires the
shared base. It also narrowly supersedes ADR-0013 and ADR-0020's clauses that make complete-source
compilation ineligible for the required collision and fanout result, only for the standalone cold
CLI WU6 route. Complete-source fallback remains ineligible on the shared/provider lifecycle. Sparse
epochs remain the accepted shared-base collision design. ADR-0017 remains fully binding: every fresh
standalone CLI process compiles the vendored sources, and no shipped semantic artifact or cold-start
cache is introduced.

### Do not delete the shared collision machinery in this sprint

The frozen base, classifier, sparse epoch, replay plan, and complete-source containment fallback
remain live for repeated same-process consumers. Removing them would be a separate lifecycle/API
decision with its own measurements. This sprint only stops making that machinery pay for and route
the one-project CLI case.

## Acceptance and falsification

The decision is rejected or superseded if any of these gates fails:

1. The cold route differs from the shared route or `tsc 6.0.3 --strict` on accepted import,
   declaration-merge, global-object, parse, diagnostic, or incomplete witnesses.
2. A cold check acquires `LibraryBaseProvider`, constructs a collision replay plan, seals or reuses
   a standalone library-only semantic base, starts semantic publication before resolved user
   bindings join the combined binder, performs a second semantic publication, or compiles any
   packaged library source more than once.
3. A parser diagnostic or panic reaches binder or semantic work, the library event ledger does not
   complete, or the ADR-0018 named census drifts on the new compiler path.
4. Original input/report ordering or relative-import dependency ordering changes.
5. The official batch compiles the library more than once, leaks user state between frames, or uses
   the cold route per frame.
6. `library-info`, the benchmark contract, or the batch receipt cannot distinguish the two routes.
7. The authoritative WU6 rerun misses median, p95, bootstrap, or memory acceptance on any row.

The required pre-WU6 corpus includes ordinary imports in both CLI input orders, dependency cycles
and missing modules, every B102/B103 merge shape in both orders, recoverable parse diagnostics and
parser panics, a positive exit-3 incomplete case, mixed diagnostic/incomplete ordering, original
report order, provider non-initialization on cold checks, and one-base batch reuse without project
contamination.

## Consequences

- Standalone cold performance scales with one complete library-plus-project compile rather than a
  shared-base compile followed by collision replay.
- Repeated API and official-suite workloads retain library sharing.
- There are two production lifecycle routes, so parity and attestation are permanent gates.
- The current probe remains test-only and non-authoritative. Production reuses its measured
  semantic lifecycle, not its frontend or output shortcuts.
- General checker, multi-file, generic-heavy, and flow-heavy performance claims remain out of scope.

## Alternatives considered

### Route only collisions through complete-source compilation

Rejected. The current driver initializes the shared base before collision preflight. A colliding
standalone process would therefore compile the library twice and recreate the measured failure.
Moving a complete root index and classifier before base initialization adds another provenance
surface and still leaves two cold lifecycles where one is faster on all measured rows.

### Send every public and batch check through complete-source compilation

Rejected. The official batch runs hundreds of isolated projects in one process. Recompiling 82
library files per frame discards ADR-0011's legitimate same-process sharing benefit.

### Continue optimizing sparse replay as the standalone route

Rejected for this sprint. The authoritative result falsified its required collision and fanout
rows, while the complete-source probe passed every corresponding cold gate with lower memory. Sparse
replay remains available where its shared-base lifecycle is appropriate.

### Use lazy library loading, a semantic snapshot, or a persistent cold-start cache

Rejected. Those mechanisms change or precompute the fixed profile rather than making the ordinary
source-backed checker path fast. ADR-0017's product boundary remains binding.

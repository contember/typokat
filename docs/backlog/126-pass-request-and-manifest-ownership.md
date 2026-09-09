---
id: 126
title: Collapse the mirrored pass request and build the declaration manifest once
blocked-by: []
---

# 126 — Collapse the mirrored pass request and build the declaration manifest once

**Summary.** Two related ownership cleanups graduated from the maintainability audit in
[`checker-semantic-boundaries-and-maintainability.md`](../ideas/checker-semantic-boundaries-and-maintainability.md)
(findings M4 then M1). Effort S + M; non-blocking maintenance, behaviour-neutral. Do M4 first so the
new manifest ownership does not acquire another field mirror.

## Problem

- **M4** — `ProjectLocalUserPassRequest` (`project_local/project_local_pass_port.rs`) and
  `ProjectLocalUserPassPlan` (`checker/mod.rs`) share fourteen of fifteen fields; the adapter
  destructures one and reconstructs the other without validation or conversion. Every new input is
  defined and forwarded twice.
- **M1** — On the ordinary non-empty project route `SourceDeclarationManifest::build` runs for the
  same program slice three times (binding, dependency preparation, checking; `checker/mod.rs` and
  the pass port), and inventory authentication is repeated, despite the one-manifest/inventory
  contract of [ADR-0022](../decisions/0022-use-one-binder-authenticated-declaration-inventory.md).
  These are three executions of one canonical builder, not competing algorithms; no timing or
  diagnostic regression was demonstrated.

## Approach / acceptance

1. **M4 (S):** make the request contain `programs` plus the existing plan; keep ordinary/test
   wrappers and the cfg-gated fault field in its current test owner. Acceptance: identical ordinary
   behaviour, test fault activation and post-pass inspection; no new public compatibility layer.
2. **M1 (M):** retain binding manifests beside the AST-free `BoundProjectBinder` in a request-local
   execution payload; consume them into inventories once with the canonical authentication batch;
   borrow for dependencies and lexical reservations, then move into checking. No cache, registry,
   Binder AST lifetime or second state machine. Acceptance: exact source/module pairing, fail-closed
   authentication, canonical fragment order, lexical event order and outcomes unchanged, plus a
   construction-count witness on the real ordinary route.

Gates: `cargo test`, `cargo clippy --all-targets -- -D warnings`, ordinary exact conformance and the
repeated/standalone parity run (route lifecycle is touched).

## Touch points

`crates/typokat-check/src/check/checker/mod.rs`,
`crates/typokat-check/src/check/checker/project_local/{project_local_pass_port.rs,entry.rs}`.

<!-- Origin: ideas/checker-semantic-boundaries-and-maintainability.md (audit 2026-09-06), graduated 2026-09-09. -->

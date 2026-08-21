---
id: 120
title: Deduplicate same-tree and CI verification work
blocked-by: []
---

# 120 — Deduplicate same-tree and CI verification work

**Summary.** Remove provably subsumed test invocations within one checked tree and reuse one pinned
release artifact across PR consumers without replacing explicit, reproducible gate definitions.
Non-blocking development infrastructure, effort M.

## Problem

Sprint closure lists can run crate suites, then workspace `cargo test`, then conformance again for a
visible receipt even when the checked tree has not changed. Commands and expected inventory counts
are copied across living documents and can drift.

PR CI also builds the same release binary in the build, official-suite, and differential jobs.
Per-job Cargo caches do not share the final linked artifact. Cold caches or runner contention spend
capacity without validating a distinct binary.

A mutable canonical profile runner is not an acceptable replacement. Gate selection depends on the
semantic change, some gates need pre-change binaries, and archived sprint plans must retain the exact
commands and receipts that ran at closure.

## Approach / acceptance

Add a transparent same-tree verification helper that accepts an explicit resolved gate list. It
must print every command before execution, record the commit and dirty-tree identity, reject reuse
after the tree changes, and remove only targets proven subsumed within that invocation. Sprint plans
continue to freeze resolved commands, scope reasons, and expected receipts rather than a mutable
profile alias.

In CI, make the release-build job publish the pinned `target/release/typokat` artifact. Official-suite
and differential jobs download it and verify its commit/hash identity before running. Preserve the
same harness commands and failure semantics. The weekly tsc-truth job may keep an independent build
if its trigger/artifact lifetime requires it.

Acceptance includes helper self-tests for tree drift and subsumption, unchanged visible conformance
receipts, an archived-command reproducibility example, CI artifact identity checks, and no reduction
in the existing fmt, Clippy, unit, conformance, official-suite, differential, package, or full-parity
bug-class coverage.

## Touch points

A small repository-owned verification helper or xtask, sprint/dev-method command documentation,
`.github/workflows/ci.yml`, official-suite and differential binary paths, and CI artifact checks.

<!-- Origin: 2026-08-20 development-friction audit and adversarial gate-runner review. -->

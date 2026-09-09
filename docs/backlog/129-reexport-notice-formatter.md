---
id: 129
title: Format the re-export provenance notice in one place
blocked-by: []
---

# 129 — Format the re-export provenance notice in one place

**Summary.** The `unsupported-source-reexport-namespace-provenance` notice is rendered at one site
and reconstructed byte-for-byte at another so it can be removed; the wording is an implicit
identity key. Graduated from the maintainability audit in
[`checker-semantic-boundaries-and-maintainability.md`](../ideas/checker-semantic-boundaries-and-maintainability.md)
(finding M10). Effort S; non-blocking maintenance. Re-verified on `main` `81147d86` (2026-09-09).

## Problem

`crates/typokat-frontend/src/frontend.rs:5108` formats the notice; `frontend.rs:4037` rebuilds the
same string for declaration-wide removal. Wording, separators, location, specifier and member name
together form the key; editing one rendering leaves a stale blocking notice in the summary. The two
renderings currently agree; no false-clean result was demonstrated.

## Approach / acceptance

One private formatter used at both sites. Preserve declaration-wide removal, the shared evidence
collector, provenance blocking, sorting and every frozen string. The initial identity-first
redesign was rejected: suppressing a notice earlier can alter admission, and per-member suppression
does not preserve declaration-wide replacement.

Acceptance: `tests/b15_acyclic_source_reexports_cli.rs` and the frontend unit tests keep exact
notice strings for mixed/default declarations and separately collected evidence. Add one control
that a deliberately changed rendering at either site is caught by the shared formatter.

Gates: `cargo test`, `cargo clippy --all-targets -- -D warnings`, ordinary exact conformance with
the current receipt. No fixture or scoreboard movement is expected.

## Touch points

`crates/typokat-frontend/src/frontend.rs`.

<!-- Origin: ideas/checker-semantic-boundaries-and-maintainability.md (audit 2026-09-06), graduated 2026-09-09. -->

---
id: 112
title: Centralize the CLI report rendering transaction
blocked-by: []
---

# 112 — Centralize the CLI report rendering transaction

**Summary.** Remove the largest remaining live CLI duplication while preserving exact output,
buffering, and exit-status behavior (effort S).

## Problem

`src/main.rs` has three production paths that independently render project notices, parse errors,
diagnostics, and incomplete records, then recompute the same status precedence. The repeated regions
are `render_official_batch_reports`, `render_project_reports`, and the rendering half of
`check_paths_with_io`. Together they contain about 145 protocol lines and three copies of the
`incomplete > errors > clean` decision.

This is not cosmetic duplication. Drift can change output ordering, bounded-output behavior,
buffering atomicity, or process exit status. It therefore does not belong in the move-only
`library_compiler.rs` decomposition sprint.

## Approach / acceptance

Introduce one private report-rendering transaction that owns the four output channels and returns
the final check status. Keep all route wrappers and their existing I/O policy: official batch still
uses rich output through `BoundedOutput`, while ordinary and project routes still buffer before
write and flush. Do not change wording, channel order, format selection, size caps, or exit codes.

Static acceptance on `src/main.rs`:

- `diagnostics::render_to_writer_with_format` call sites: `3 -> 1`;
- `diagnostics::render_incomplete_to_writer_with_format` call sites: `3 -> 1`;
- `had_errors` occurrences: `9 -> 3`;
- `had_incomplete` occurrences: `9 -> 3`;
- exactly one report-channel loop and one status-precedence decision remain.

Behavioral acceptance must retain exact stdout, stderr, and exit witnesses in
`incomplete_outcome`, `wu7_official_batch`, both B72 Bundler tracer suites, and
`wu7_parse_routing`, followed by the ordinary conformance and official-suite ratchets.

## Touch points

`src/main.rs`; focused CLI integration tests only if verify-first review finds a missing channel or
route witness.

<!-- Origin: post-WU4 audit in ../archive/sprint-2026-08-14-maintenance-cleanup.md. -->

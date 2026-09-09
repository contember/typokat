---
id: 131
title: Conditional cleanups — member-context scope helper and driver error payloads
blocked-by: []
---

# 131 — Conditional cleanups: member-context scope helper and driver error payloads

**Summary.** Two audit findings that are only worth doing if the result is demonstrably smaller
than what it replaces. Graduated from
[`checker-semantic-boundaries-and-maintainability.md`](../ideas/checker-semantic-boundaries-and-maintainability.md)
(findings M9 and M11) so they are not re-proposed. Effort S–M; lowest-priority maintenance; each
half may be closed as "rejected" without shipping. Re-verified on `main` `81147d86` (2026-09-09).

## Problem

- **M9 — manual member-context restoration.** `crates/typokat-check/src/check/checker/classes/members.rs`
  saves and restores `current_this`, `current_body_this_environment` and `current_in_ctor` by hand
  at three paired sites (around lines 230/281, 311/316, 327/329). All inspected paths restore
  correctly; no leak was demonstrated. A new member-local field requires auditing every pair.
- **M11 — driver conversions erase error identity.** `crates/typokat-driver/src/driver.rs:241-258`
  converts typed session, inventory and assembly failures into `check_failure(String)`; ten
  assembly-error variants lose their structured identity while library-initialization errors keep a
  cause. Errors still propagate; no consumer requiring subtype recovery was demonstrated.

## Approach / acceptance

- **M9.** A private scope helper that saves/restores the three fields around the existing body
  work. Static, instance and constructor policies stay where they are; outer class access scope,
  lexical events and static type-parameter barriers keep their owners. **Drop it** if the borrow or
  closure plumbing is not smaller than the bookkeeping it replaces. Never snapshot the whole `Pass`.
  Witnesses: nested classes, static/instance bodies, constructor-only writes and early exits keep
  their behaviour under the existing class fixtures.
- **M11.** Retain the existing payloads at the driver boundary and render context in `Display`;
  no new taxonomy or recovery framework. Inspect API exposure before widening private error types.
  **Drop it** if payload preservation has no concrete consumer. Witnesses: exit classifications,
  messages and propagation stay byte-compatible across the CLI contract tests.

Each half is its own decision and its own commit. Do not schedule either alongside literal or
inference changes.

## Touch points

`crates/typokat-check/src/check/checker/classes/members.rs`, `crates/typokat-driver/src/driver.rs`.

<!-- Origin: ideas/checker-semantic-boundaries-and-maintainability.md (audit 2026-09-06), graduated 2026-09-09. -->

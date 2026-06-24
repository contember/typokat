---
id: 04
title: readonly assignment suppresses the value-type cascade
blocked-by: []
---

# 04 — readonly assignment suppresses the value-type cascade

**Summary.** Minor code-precision nit (the line *is* flagged — not a dropped error).

## Problem

`this.ro = 5` where `ro: 1` is `readonly` reports `TK2540` (read-only) but not the `TK2322` (`5` not
assignable to `1`) that tsc also emits. The line is flagged, so this is a code-precision gap, not a
soundness hole.

## Approach / acceptance

After emitting `TK2540` for a read-only assignment, still run the value-type assignability check so
the `TK2322` cascade also fires where tsc emits it. Acceptance: a fixture assigning an
out-of-type value to a `readonly` member reports **both** `TK2540` and `TK2322`.

## Touch points

Member-assignment checking (the `readonly` path) — don't short-circuit the value-type check.

<!-- Origin: official-suite finding F6. -->

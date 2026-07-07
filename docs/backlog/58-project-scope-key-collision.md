---
id: 58
title: Project mode — scope maps keyed by span start collide across files
---

# 58 — Cross-file span-start collision in project mode

**Summary.** `Binder.fn_scopes` / `block_scopes` are keyed by **node span start**
(`src/binder/bind.rs:546`, `:567` arrow, `:429` block), unique per file only.
`ProjectBinderBuilder` (`bind.rs:187-238`) shares one `BindState` across all modules, so
a function at the same byte offset in a later file **overwrites** the earlier file's
entry; the checker then descends into the wrong file's scope
(`calls.rs:740`/`:800`, `statements.rs:180`). Leader-verified 2026-07-07: two files whose
functions both start at offset 0 → **exit 0, zero diagnostics** where tsc reports TS2322
(silent when a same-named binding exists in the other file's chain; spurious TK2304
otherwise). **HIGH** — the invariants' "order-dependent dropped errors" class, keyed on
file layout; hits any ≥2-file project with offset-aligned functions/arrows/blocks, i.e.
practically every real project.

## Problem

The M29 serial project checker reuses per-file span-keyed maps in a shared universe.
`reference_flow` shares the keyspace too (`mod.rs:887`, `expr.rs:151`) but is rebuilt per
module immediately before its check — masked today, one refactor away from the same bug.

## Approach / acceptance

Key the maps by `(module id, span start)` or a global node id — including
`reference_flow` for safety. Acceptance: the collision probes (aligned functions with
same-named/absent bindings) report exactly tsc's diagnostics; an m29 corpus fixture pins
two offset-aligned files; no single-file regression.

## Touch points

`src/binder/bind.rs` (map keys, `ProjectBinderBuilder`), consumers in
`src/check/checker/calls.rs`, `statements.rs`, `mod.rs`/`expr.rs` (`reference_flow`),
m29 corpus.

<!-- Origin: cross-cutting soundness review 2026-07-07 (modules reviewer #1), leader-verified. -->

---
id: 84
title: A function-local interface panics the checker
blocked-by: []
---

# 84 — A function-local interface panics the checker

**Summary.** `function f() { interface I { x: number } }` aborts the check worker — a reachable
panic on valid TypeScript, which violates the no-reachable-panic invariant. Severity: HIGH
(the whole file, not just the declaration, produces no diagnostics).

## Problem

`interface_header_owner` (`crates/typokat-check/src/check/checker/decls/mod.rs:540`) unwraps
`lexical_events.interface_occurrence_owner(...)` with
`.expect("interface header has one exact preallocated owner")`. Interface occurrence owners are
preallocated only for interfaces the lexical-event pass reaches; one nested in a function body is
not among them, so the `Option` is `None` and the worker panics. `src/driver.rs:185` then
re-panics with `check worker panicked`.

Repro (reproduced on a pristine `933bfd5` build, so it predates the declaration-planner work):

```ts
function f() { interface I { x: number } }        // panics
const g = () => { interface I { x: number } };    // panics
```

Scoped by probe: **only `interface` is affected.** A function-local `type` alias, `enum`, `class`,
or `namespace` all check without panicking. The same panic is reached by
`tests/cases/b43_namespaces_declaration_merging/slot_shadowing.ts` and by two files of the
official corpus when they are run standalone, so it is currently masked only by how those run.

Related but distinct: `21-local-class-checking.md` is about local classes being silently
*unchecked*. This one is a hard abort.

## Approach / acceptance

Preallocate interface occurrence owners for interfaces nested in function bodies (the binder
already produces scoped declarations for them — verify), or, if that lexical-event slot is
genuinely unavailable there, make the checker degrade to the incomplete-outcome path instead of
unwrapping. Soundness over completeness: reporting the interface as unmodelled is acceptable, a
panic is not.

Acceptance: a fixture with a function-local interface used as an annotation checks to completion
and matches `tsc --strict` (assignability through the local interface, and a `TK2322` on a
mismatched member); no top-level regression; the existing `slot_shadowing.ts` fixture passes
standalone.

## Touch points

`crates/typokat-check/src/check/checker/decls/mod.rs` (`interface_header_owner`), the lexical-event preallocation pass,
`src/driver.rs` worker error path.

<!-- Origin: independent adversarial review of the declaration-surface planner, 2026-07-24
     (finding 6, incidental to that diff). -->

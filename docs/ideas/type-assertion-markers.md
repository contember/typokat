# Inline type-assertion markers for the conformance corpus

Proposal (2026-07-09, peer-reviewed as NOTE — see
[`phpstan-architecture-lessons.md`](phpstan-architecture-lessons.md), proposal C).

## Problem

The corpus can only pin *diagnostics*. A wrong-but-**compatible** inferred type is invisible
until it happens to suppress or produce a downstream error — backlog `65` (candidates unioned
instead of fixed-then-checked) was exactly this class, and several corpora fall back to
code-only markers because the inferred target's shape can't be asserted
(`tests/cases/README.md`, b65 construction note). Inference is otherwise pinned only by
`src/check/infer/tests.rs` unit tests (engine-level, not end-to-end).

## Design (the form that survived review)

A `// type: <annotation>` marker on a **variable-declaration line**:

```ts
const x = cond ? 1 : undefined; // type: number | undefined
```

The harness lowers the expected annotation **through the normal annotation-lowering path, in
the fixture's own scope/interner**, and compares interned `TypeId`s — an integer compare,
display-independent by construction (hash-consing does the work). No string comparison
anywhere.

**Explicitly rejected form:** comparing against the *displayed* type string (PHPStan's
`assertType('string', $x)` / `TypeInferenceTestCase`). Unsound here by declared policy:
union member order is intern-order dependent and object/alias rendering may change
(`tests/cases/README.md` "Type display"). Do not re-propose the string form.

## Constraints & open caveats (resolve before adopting)

- **Declaration lines only** — avoids expression addressing, and avoids PHPStan's
  contamination problem (wrapping `x` in a call changes contextual typing of the checked
  program). A marker asserts the declared binding's type after inference/widening.
- **Literal freshness:** the marker's annotation lowers as a plain type; the binding may hold
  a *fresh* literal/object type. Decide the comparison point (post-widening declared type is
  the natural one) and pin it.
- **Type parameters / generics in marker position:** an annotation naming an in-scope type
  param must resolve to the same named id; out-of-scope names are a fixture error.
- Harness: one new marker kind in `tests/conformance.rs`, same same-line convention;
  a line may carry both `// type:` and `// error[...]` markers.

## Value / priority

Cheap triangulation between engine unit tests and end-to-end error markers; would have made
`65`-class bugs directly speccable. Not worth scheduling ahead of the model-completeness
track — graduate to a backlog item when an inference-heavy milestone next needs it, or
delete if the official-suite scoreboard proves sufficient in practice.

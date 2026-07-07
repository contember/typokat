---
id: 61
title: Class field initializers are never checked against the field annotation
---

# 61 — Class field initializers unchecked

**Summary.** `class C { n: number = "not-number"; }` produces **zero diagnostics**
(leader-verified 2026-07-07; tsc TS2322) — likewise excess properties and tuple shapes in
field initializers. `check_class_member_bodies` (`classes.rs:918-931`) walks the
initializer via `infer_expr` only: no assignability obligation, no excess check, no
contextual typing. Pre-existing (not an M30 regression), but it is a contextual/excess
*declaration position* the M30 sweep missed, silent on a very common pattern, and absent
from the documented-deferral list. **HIGH-frequency FN.**

## Approach / acceptance

Check each field initializer against the declared annotation exactly like a variable
declaration initializer: assignability (TK2322), fresh-literal excess (TK2353),
contextual typing of object/array/tuple literals (M30 rules), `readonly` fields still
initializable in their declaration. Static fields too. Corpus first: annotated fields
(primitive/object/tuple/union), excess in field literals, contextual literal widening;
cross-check tsc 6.0.3 --strict.

## Touch points

`src/check/checker/classes.rs` (`check_class_member_bodies`), reuse the declaration
initializer path from `statements.rs`/`assignment.rs`, m11/m30 corpus extension.

<!-- Origin: cross-cutting soundness review 2026-07-07 (modules reviewer #4), leader-verified. -->

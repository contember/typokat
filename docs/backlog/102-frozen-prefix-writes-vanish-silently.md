---
id: 102
title: Binder writes into the frozen library prefix vanish silently
blocked-by: []
---

# 102 — Binder writes into the frozen library prefix vanish silently

**Summary.** Five binder call sites mutate a row with `if let Some(row) = table.get_mut(id)`. Against
a `FrozenLibraryBase` that lookup returns `None` for any id below the prefix boundary, so the
**user's declaration is dropped with no diagnostic and no failure**. An ordinary `globals.d.ts`
consumed from another file produces a spurious `TK2304`, and a library-shadowing declaration is
silently ignored where `tsc` reports a redeclaration error. Effort M. **This is not a collision
problem** — it fires on fresh names too, so the WU5 classifier will not catch it. Fix it first,
ahead of the routing work.

## Problem

```ts
// globals.d.ts
interface AppConfig { name: string }
declare var appConfig: AppConfig;

// main.ts
const n: string = appConfig.name;   // TK2304: Cannot find name 'appConfig'
```

`tsc 6.0.3 --strict` is clean. typokat on the library base reports `TK2304`. The name is *fresh* —
nothing in `lib.d.ts` is called `appConfig` — so no collision route would be involved.

Other measured shapes, all against real `tsc 6.0.3 --strict`:

| Input | typokat | tsc |
|---|---|---|
| `interface X` / `declare var v` in `a.ts`, used in `b.ts` | `TK2304` ×2 | clean |
| `declare var document: number` | accepted, library wins | `TS2403` |
| `const JSON = 1` | accepted, library wins | `TS2451` ×4 |
| `declare var isNaN: number` | accepted, no redeclaration error | `TS2300` ×2 |
| `declare function parseInt(a, b, c)` + a 3-arg call | `TK2554`/`TK2345`/`TK2322` | **clean** |

The last row is the worst: the user's overload is dropped, so their own call to their own signature
is rejected — a dropped write becoming three false positives at a distance.

## Root cause

Every frozen binder table is a `LayeredVec<T>`: an immutable `base: Arc<[T]>` plus a mutable
`local: Vec<T>` (`crates/typokat-types/src/types/layered.rs:288`). The only mutating accessor is `get_mut_local`
(`crates/typokat-types/src/types/layered.rs:343`):

```rust
pub(crate) fn get_mut_local(&mut self, index: usize) -> Option<&mut T> {
    self.local.get_mut(index.checked_sub(self.base.len())?)
}
```

Any id below the boundary is unwritable and yields `None`. The layering is doing its job —
[ADR-0011](../decisions/0011-freeze-pinned-default-library-base.md) requires that base rows are never
mutated by a delta. The defect is that these five callers treat the refusal as "nothing to do":

- `crates/typokat-binder/src/binder/scope.rs:147` — `ScopeGraph::declare`. Publishing a name into the frozen
  `compilation_global` scope silently does nothing. This is the `TK2304` mechanism.
- `crates/typokat-binder/src/binder/bind.rs:867` — `attach_symbol_declaration`. The declaration is never linked to the
  symbol, which is why the `TK2403`/`TK2451`-class diagnostics above are *structurally* unreachable
  rather than merely unimplemented.
- `crates/typokat-binder/src/binder/bind.rs:2111` — `declare_value`.
- `crates/typokat-binder/src/binder/bind.rs:2133` — `declare_function_value`.
- `crates/typokat-binder/src/binder/bind.rs:2293` — `bind_import`.

The sibling sites that hit the same boundary with `.expect(...)` panic instead
([`103`](./103-library-merge-panics-and-routing.md)). Same cause, opposite failure mode; a silent
drop is the worse of the two, because it looks like a model gap rather than a defect.

Nothing leaks into the shared base — the layering blocks the write rather than corrupting the `Arc`,
so there is no cross-project contamination.

## Approach / acceptance

Fail closed at every one of the five sites: a write targeting the frozen prefix must produce a typed
failure that reaches the caller, never a no-op. Whether the *correct* long-term behaviour is a
redeclaration diagnostic or a private rebuild is [`103`](./103-library-merge-panics-and-routing.md)'s
question; this item only requires that the outcome stop being silence.

Publishing a **fresh** script-scope name into the global scope is a separate matter and must simply
work — that is the `TK2304` case, and it needs a delta-side global scope the user run can write to,
not a refusal.

Corpus first per [`dev-method.md`](../reference/dev-method.md) §1, on the `Library` fixture base:
cross-file script type and value globals, `var`/`const`/`function` redeclaration of a library global,
an overload merged onto a library function, and the module-scope controls that must keep *shadowing*
rather than merging. Cross-check every marker against `tsc 6.0.3 --strict`.

## Touch points

`crates/typokat-binder/src/binder/scope.rs`,
`crates/typokat-binder/src/binder/bind.rs`,
`crates/typokat-types/src/types/layered.rs`,
`tests/cases/b14_full_lib_loading/`.

<!-- Origin: found 2026-07-26 by the family-1 diagnosis work unit while characterising the
     `declare global` panic; the silent half turned out to be the larger defect. Leader re-verified
     the cross-file `globals.d.ts` false positive against tsc 6.0.3 before filing. -->

# Project-local library index feasibility corpus

This disabled WU0 corpus is the acceptance specification for the bounded project-local default-
library index spike. It is intentionally not registered by the production crate yet. The
implementation work must consume these exact files and `locked-slice.toml`; it must not rewrite the
fixtures, weaken an oracle, or substitute handwritten library declarations.

## Oracle matrix

All projects use `strict: true`, `noEmit: true`, `module: ESNext`, and `moduleResolution: Bundler`.
Every multi-file project runs in both listed and reversed input order. The candidate additionally
runs the demand orders `Array,ReadonlyArray,Reflect` and `Reflect,ReadonlyArray,Array`.

| Project | Files | tsc 6.0.3 truth | Complete-source typokat oracle | Candidate obligation |
|---|---|---|---|---|
| `array-clean` | `array-clean.ts` | clean | clean | project-owned `Array`, `ReadonlyArray`, and `globalThis.Array` bridge |
| `array-cycle` | `array-cycle.ts` | one `TS2322` | one `TK2322` | atomically demand the `Array`/`ConcatArray` SCC |
| `direct-array-augmentation` | `direct-array-augment.ts`, `direct-array-consume.ts` | one `TS2322` | one `TK2322` | one merged `Array<T>` group containing native and user members |
| `declare-global-array-augmentation` | `declare-global-array-augment.ts`, `declare-global-array-consume.ts` | one `TS2322` | one `TK2322` | legal global overlay joins before publication; native members survive |
| `unique-global-type` | `unique-global-augment.ts`, `unique-global-consume.ts` | one `TS2322` | one `TK2322` | one project-global identity in both module orders |
| `script-global-object` | `script-global-object.ts` | one `TS2322` | two safe-direction `TK2339` records | exact current global-object oracle; no false-clean property |
| `reflect-namespace` | `reflect-namespace.ts` | one `TS2322` | clean (existing false-clean oracle) | single-fragment namespace/type group materializes without changing current semantics |
| `declare-global-value-tail` | `declare-global-value-augment.ts`, `declare-global-value-consume.ts` | clean | clean | preserve the current project-route result; do not invent an incomplete or diagnostic |
| `isolation-a` / `isolation-b` | one file per separate project | one suggestion-bearing `TS2551` each | one `TK2339` each | same shared index, both project creation orders, distinct local universes and no opposite member |

Normalized diagnostics, incompletes, source order, declaration-group terminal state, selected type
displays, global-object keys, and source-site identities must match the complete-source oracle. The
candidate must run isolation A then B and B then A through one shared symbolic index. Each project
must reject the other project's member, use a distinct universe-local `Array` identity, and retain
the same symbolic group identity. The candidate must not call collision preflight, fork a sparse
epoch, inherit a binder/store prefix, or retain an owner-site replay plan.

## Locked source slice

`locked-slice.toml` pins exact byte starts in source ordinal 0 (`lib.es5.d.ts`) and ordinal 23
(`lib.es2015.reflect.d.ts`). `Array`, `ArrayConstructor`, and `ConcatArray` are the required
recursive dependency witness, forced by `array-cycle.ts`. `Reflect` is the single-fragment
namespace/value group. This is explicitly a selected-source-fragment experiment: later same-name
full-profile fragments are excluded and inventoried, not silently treated as part of the slice.
The implementation may add a dependency only by recording its exact source site, edge, and reason
in the manifest before semantic code consumes it.

The shared symbolic artifact may retain stable profile/source/site/group facts. It may not retain
`TypeId`, `TypeParamId`, `DeclaredRecipeId`, `Store`, `Interner`, `LibraryBinderCheckpoint`, a
semantic terminal, relation/flow cache state, `CollisionReplayPlan`, or replay-owner rows.

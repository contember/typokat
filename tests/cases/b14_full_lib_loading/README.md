# Backlog 14 full-library acceptance corpus

Enabled shipped acceptance corpus for the exact TypeScript 6.0.3 default profile rooted at
`lib.es2025.full.d.ts`. Every ordinary error is cross-checked with
`tsc 6.0.3 --strict --target es2025 --noEmit`; unsupported language surfaces use the existing
stable incomplete identity instead of accepting an `any`/error fallback as clean.

Current mechanical inventory: 45 `.ts`/`.d.ts` units, 188 error markers, and 23 incomplete markers.
The corpus remains bridge-focused. Type-predicate and unsupported annotation/expression surfaces
retain backlog `50`/`75` ownership; `Awaited`/`NoInfer` remain design-OOS in the divergence ledger.
Iterator constructor-name coverage proves visibility only, while iterator/generator semantic tails
retain their recorded owners.

The corpus separates ordinary declaration lookup from syntax-native bridge behavior. In
particular, same-named module-local `Array`, `RegExp`, and intrinsic aliases must not acquire
library roles: `Array`, `RegExp`, `String`, `Object`, and `Function` controls prove bridges are
keyed by universe-local library declaration identity, never spelling. Only library-local iterator
name visibility has a focused negative witness. The
sibling project corpus owns preflight routing and same-universe collision rebuilding.

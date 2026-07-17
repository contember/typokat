# Backlog 14 full-library acceptance corpus

Disabled WU0A RED spec for the exact TypeScript 6.0.3 default profile rooted at
`lib.es2025.full.d.ts`. Every ordinary error is cross-checked with
`tsc 6.0.3 --strict --target es2025 --noEmit`; unsupported language surfaces use the existing
stable incomplete identity instead of accepting an `any`/error fallback as clean.

Inventory after the eager-application-cache RED spec: 12 `.ts` fixtures, 53 error markers, and six
incomplete markers.
The count is intentionally small and bridge-focused; iterator/generator/NoInfer/Awaited semantics
stay out until the WU0 census proves their library declarations terminal.

The corpus separates ordinary declaration lookup from syntax-native bridge behavior. In
particular, same-named module-local `Array`, `RegExp`, and intrinsic aliases must not acquire
library roles: `Array`, `RegExp`, `String`, `Object`, and `Function` controls prove bridges are
keyed by universe-local library declaration identity, never spelling. Iterator semantic acceptance
remains census-gated; only its library-local name visibility has a focused negative witness. The
sibling project corpus owns preflight routing and same-universe collision rebuilding.

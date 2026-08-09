# Acyclic source re-export oracle corpus

This is the permanently disabled raw-conformance corpus for the 2026-08-08 source re-export
sprint. Project behavior is owned by `tests/b15_acyclic_source_reexports_cli.rs`; the raw harness
does not consume `tsconfig.json`, resolver inventory, or project summaries.

`contract.json` records the exact TypeScript 6.0.3 oracle command and ordered diagnostics. The
same projects also act as the pre-change negative control: admitted named source re-exports remain
exit 3 until WU6, while every deferred form must keep its explicit project notice.

The oracle corrects one stale sprint assumption: `export {} from "./missing.js"` is clean in
TypeScript 6.0.3. It emits no `TS2307`, so the acceptance contract requires no `TK2307` and no
observable resolution edge for that empty projection.

The exact enabled pre-change freeze includes export attributes at their existing infrastructure
exit 2. They are a deferred form and do not move in this sprint.

WU6 unignores exactly three RED acceptances: admitted/empty summaries, namespace provenance, and
cycle accounting. The admitted contract compares the complete intended summary for both root
orders, including exact checked/skipped files, target-bearing resolution rows, grouped missing
owners, diagnostics, and all empty channels.

The admitted enum row pins the clean `df02287` public route. TypeScript 6.0.3 is clean, while
typokat still checks every root and reports its existing enum-model gap (`TK2305`, `TK2304`, and
`enum-declaration/self`). Default-slot candidate classification must not replace that result with a
namespace-provenance notice; enum modeling is outside the default-slot sprint.

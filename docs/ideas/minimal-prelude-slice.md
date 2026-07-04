# Minimal ambient prelude slice (pull real-world signal forward)

**Proposal.** Make the "earlier minimal ambient/prelude slice" that backlog `14` already
*permits* a concrete scheduled item around `07`/`08`, instead of a footnote: a deliberately
small, hand-curated ambient set (e.g. `console`, a few `string`/`number` members like
`.length`, `Math`, maybe non-generic `Array` methods) loaded as a frozen prelude — explicitly
NOT pretending to be `lib.d.ts`.

**Why.** Architecture §12 Phase 1's own goal is "a usable checker on a real repo as early as
possible — completability is decided here", but the current sequence defers first real-world
contact until after the whole type-level phase. A minimal prelude (a) moves many
`OOS:unresolved` official-suite files into scope, widening the regression net cheaply; (b)
exercises the declaration-merging machinery (§4.1) early, before full `lib.d.ts` forces it at
scale; (c) starts paying down the clean-file false-positive rate (scoreboard `clean-kept`
165/210 ≈ 21% of clean files get an over-report — safe, but a usability ceiling worth its own
upward ratchet target, not just a no-regression floor).

**Constraint carried from backlog 14.** When `14` starts, the full loader *replaces* this slice
— no second ambient-loading path.

<!-- Origin: architecture assessment, session 2026-07-04. Graduates to a backlog item if/when
     the roadmap owner schedules it. -->

---
id: 80
title: Differential resolution oracle vs pavouk/ts-morph on a real monorepo
blocked-by: [./79-resolution-query-surface.md]
---

# 80 — differential resolution oracle (pavouk ground truth)

**Summary.** Use `contember/pavouk`'s ts-morph-resolved code graph as a **ground-truth oracle for
resolution at scale**: ~138k real resolution assertions from real code, diffable edge by edge
against typokat. This is different from backlog [`72`](./72-real-project-preview-readiness.md),
which still needs a pinned diagnostic witness and mutation ratchet, and from the synthetic
resolution contracts shipped under backlog [`15`](./15-modules-imports.md): this diffs
*resolution* against a compiler-accurate graph across a whole
monorepo. Non-blocking for checker 1.0, but the strongest scale signal available for the D track.

## Problem

typokat's conformance corpus is excellent for the type model, but silent about whether resolution
holds up on a real repository. The bounded files-only Bundler route and synthetic accounting
contract are shipped. Its follow-up six-candidate screen still found no zero-clean public witness,
so backlog `72` remains incomplete. We also lack a *cheap, high-volume, real-world* correctness
signal.

`pavouk` (a sibling repo: compiler-accurate code graph for monorepos, ts-morph over the real TS
checker) already produces exactly that signal as a by-product. Indexed against the Contember
monorepo it holds:

| | count |
|---|---:|
| files / symbols / edges | 4,102 / 26,403 / **137,996** |
| `type_ref` | 50,561 |
| `calls` | 41,137 |
| `imports` / `reexports` | 20,075 / 12,942 |
| `contains` | 11,873 |
| `extends` / `implements` / `references` | 399 / 394 / 615 |

Each edge is a resolution assertion of the form *"the reference at (file, line) resolves to the
declaration at (file, line, name, kind)"* — produced by the real TypeScript checker via
`getSymbol()`/`getAliasedSymbol()`, and already validated against grep at 95–100% site coverage
with zero phantoms on tested symbols. That is a far denser correctness corpus than any fixture set
we would hand-write, and it exercises exactly the surfaces typokat has not yet met: barrel
re-export chains across ~140 tsconfigs, `.d.ts` package boundaries with declaration maps, and
DI-style member calls through typed constructor parameters.

The sub-population that matters most: **16,555 `calls` edges (≈40%) land on a *member*
declaration** — `MethodDeclaration` (9,263), `MethodSignature` (3,391), `PropertySignature`
(3,197), and the property tail. These are resolvable *only* through the receiver's type. They are
the reason a syntax-only tool cannot produce this graph, and they are the precise capability that
distinguishes typokat from `oxc_semantic` + a linker. 6,588 of them (`MethodSignature` +
`PropertySignature`) additionally require the interface-member side table from backlog `79`.

## Approach / acceptance

A **black-box differential harness**, in the shape of `tooling/official-suite/` (shells out to the
prebuilt binary, independent of the checker build, committed scoreboard, `--check` exits 1 on
regression).

- **Corpus.** pavouk exports its edge set as a stable, sorted, checked-in artifact — the oracle is
  a *file*, not a live dependency on pavouk or on ts-morph. Pin the repo commit, the pavouk
  version, and the TypeScript version that produced it, exactly as the lib audit is pinned.
- **Comparison.** Run typokat's resolution map (backlog `79`) over the same tree and classify every
  oracle edge: **agree** (same declaration site), **miss** (typokat resolved nothing), **phantom**
  (typokat resolved something the oracle did not), **disagree** (different declaration site), or
  **incomplete** (typokat explicitly reported an unsupported surface). Report per edge kind and per
  target kind — the interesting cell is `calls` × member-target.
- **Ratchet, not a gate.** A committed scoreboard with the counts; `--check` fails on regression.
  The absolute numbers may start bad because module and package breadth is incomplete, and that is
  fine — the point is the *derivative*. The full default library is the shipped baseline; as `15`
  lands, whole module-dependent cells should flip.
- **Disagreements are two-way evidence.** pavouk is ground truth for *resolution*, not scripture: a
  disagreement is a bug in typokat, a bug in pavouk, or a deliberate divergence. Each one gets a
  disposition, and typokat's genuine divergences land in
  [`divergences.md`](../reference/divergences.md) like any other.

**Acceptance.** The harness runs on the pinned corpus, emits the classified scoreboard, and
`--check` catches a seeded regression. Explicitly **not** a numeric pass threshold at this item —
the same policy as the official-suite ratchet.

**Sequencing.** Useful as soon as `79` exists, but its coverage is bounded by the module layer:
without `15` (`node_modules`, `.d.ts`, barrels, path aliases) most cross-package edges will
classify as `incomplete`. The shipped full default library lets receiver types that flow through
`Promise`/`Array` participate from the first run. Expect the honest first run to remain mostly
`incomplete` on module breadth, and treat that as the baseline to climb.

## Touch points

New `tooling/resolution-oracle/` (harness + committed scoreboard + pinned corpus artifact); depends
on the resolution map from backlog `79`. Coverage tracks `15` over the shipped default-library
baseline.

<!-- Origin: pavouk/typokat integration design session, 2026-07-14. The insight is that the
     dependency is worth running *backwards*: pavouk-on-ts-morph is more valuable to typokat as an
     oracle than typokat is to pavouk as an extractor (pavouk already works; typokat lacks scale
     evidence). -->

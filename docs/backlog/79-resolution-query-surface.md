---
id: 79
title: Resolution query surface (declaration-site provenance)
blocked-by: []
---

# 79 — resolution query surface

**Summary.** Make typokat's resolution results *addressable from outside*: given a span, which
declaration did it resolve to, and where does that declaration live. Today the checker knows this
transiently and throws it away — only diagnostics escape. Non-blocking for checker 1.0; it is the
prerequisite for every consumer that wants the resolver rather than the diagnostics
(backlog [`80`](./80-pavouk-resolution-oracle.md), [`81`](./81-resolve-only-driver-mode.md), and
eventually an LSP).

## Problem

`crates/typokat-driver/src/driver.rs` is a batch pipeline: parse → bind → check →
`Vec<Diagnostic>`. There is no way to ask *"what does this identifier/member access resolve to?"*.
Three concrete gaps:

1. **No span → declaration mapping.** The checker resolves a `PropertyAccessExpression` receiver to
   a type and looks the member up, but nothing records the outcome. A consumer cannot recover it.
2. **No `DeclId` → declaration site.** `binder::symbol::DeclId` indexes a declaration, but there is
   no exposed mapping to `(file, line, name, kind, exported)`. Consumers need to *name* the target,
   not just distinguish it.
3. **Members of interfaces and object literals have no declaration backref.**
   `types::repr::PropertyType` carries `declaring_class: Option<ClassId>`, which is `None` for every
   member that "did not come from a class declaration (object literals, interfaces, type ...)"
   (`repr.rs:294-299`). So an interface method's declaration site is currently unnameable.

⚠️ **The invariant this must not break.** `PropertyType` is hash-consed and identity-bearing
property metadata is folded into the type hash (architecture §3; `types/hash.rs`). Adding a
`decl: DeclId` field to `PropertyType` would make two **structurally identical** types declared in
different files unequal — destroying structural typing, the core of the checker. The declaration
backref therefore **must live in a side table**, keyed outside the hashed representation, never in
`repr`. Record this in [`invariants.md`](../reference/invariants.md) as part of this item.

A fourth gap is consumer-only and worth solving here because nowhere else will:

4. **Declaration maps (`.d.ts.map`) are not followed.** Backlog [`15`](./15-modules-imports.md) will
   parse and check `.d.ts`, which is all a *checker* needs — the type meaning is identical either
   way. But a consumer asking "where is this declared?" wants the **original source** declaration,
   not the generated `.d.ts` line. Following `X.d.ts` → `X.d.ts.map` → `sources[0]` → the source
   declaration of the same name is the general fix (it is what editors do, and what
   `contember/pavouk` had to implement by hand as `redirectDtsToSource`). Note that this is *not*
   solvable via `customConditions`: exports maps list `types` before `typescript`, and TS takes the
   first matching condition, so the map-based redirect is the only general answer.

## Approach / acceptance

Add a **resolution record** side channel, populated during the existing check pass and returned
alongside diagnostics. Do not fork a second traversal.

- A `ResolutionMap`: `Span -> Resolved { decl: DeclId, kind, via }`, where `via` distinguishes
  direct binding, import alias chain, member lookup on a receiver type, and heritage-inherited
  member. Populated at the points that already resolve (`check/checker/expr.rs` member access,
  `calls.rs`, the binder's identifier resolution).
- A `DeclTable`: `DeclId -> DeclSite { file, span, name, kind, exported }`. The binder already
  allocates `DeclId` per declaration site; this exposes it.
- A **property → declaration side table** for members that `declaring_class` cannot name
  (interfaces, object literals, type literals), keyed by the declaring type's identity + member
  name, held outside the hashed `PropertyType` (see the invariant above).
- **Declaration-map re-anchoring**: when a resolved `DeclSite` lands in a `.d.ts`, follow the
  adjacent `.d.ts.map` back to the source declaration and report *both* (the `.d.ts` site and the
  re-anchored source site). Never silently replace one with the other — a consumer may want either,
  and a missing/stale map must degrade to the `.d.ts` site rather than to nothing.
- **Incomplete surfaces stay honest.** A site the checker could not resolve must be recoverable as
  such (the existing `incomplete[<surface-id>]` machinery), not silently absent from the map.
  Silence and "unresolved" must be distinguishable — this is the whole difference between a graph
  with a known coverage number and a graph that is quietly wrong.

**Acceptance.** Fixtures asserting resolved declaration sites for: a local call, a call through an
import alias chain (barrel re-export), a class method on a typed receiver, an inherited method, an
interface method (exercises the side table), and a `.d.ts`-declared symbol with a declaration map
(asserts both sites). Plus a negative: an unresolvable member access appears as `incomplete`, not
as a missing entry. Adding the side table must leave `cargo test` green with **no change to any
existing type-identity assertion** — that is the regression witness for the hash invariant.

This item can start on the shipped M29 local-relative slice; it only becomes *useful at scale* once
backlog [`15`](./15-modules-imports.md) lands real module resolution and `.d.ts` consumption.

## Touch points

`src/binder/symbol.rs` (DeclId → site), `crates/typokat-check/src/check/checker/expr.rs` + `calls.rs` (record on
resolve), `src/types/repr.rs` (side table — **not** the hashed repr),
`crates/typokat-driver/src/driver.rs` (return the map), `docs/reference/invariants.md` (the
hash-consing invariant).

<!-- Origin: pavouk/typokat integration design session, 2026-07-14 — "does typokat already give us
     what we need for type resolution?" Answer: the type model yes, the query surface no. -->

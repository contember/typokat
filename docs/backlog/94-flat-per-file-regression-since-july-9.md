---
id: 94
title: A flat 3x per-file regression sits under the modules exponent
blocked-by: [./93-namespace-attachment-fill-per-module-scan.md]
---

# 94 — A flat 3× per-file regression sits under the modules exponent

**Summary.** Once [`93`](./93-namespace-attachment-fill-per-module-scan.md) removes the exponent,
`modules-100000` lands at ~0.98 s against typokat's own committed 0.3068 s of 2026-07-09 on the
identical corpus — **157 µs/file today against 49 µs/file then**, with peak RSS 372 MB against
159.9 MB. No single phase dominates, so this needs a **bisect**, not a hunt. Effort M–L.

## Problem

`93` is measured to be the whole superlinear term: skipping the attachment fill takes
`modules-100000` from 6.86 s to **0.977 s** and flattens the exponent to 1.03–1.12 out to 12,000
files. That 0.98 s is the honest projection of where `93` lands us — and it is still **3.2× slower
than typokat was on 2026-07-09**, which is not a projection but a committed measurement:

| | median | peak RSS | vs tsgo |
|---|---|---|---|
| 2026-07-09 (`report/raw/20260709-174055`) | **0.3068 s** | **159.9 MB** | tsgo 0.3741 s — **typokat won** |
| HEAD `0c2994c`, attachment fill skipped | 0.977 s | 372 MB | tsgo 0.307 s — 3.2× slower |

The memory figure is the useful independent signal: 2.3× more resident for the same corpus points at
representation or retention, not at an algorithm, and it moved in the same window.

Phase breakdown of the 0.98 s residual — the point is that there is no dominator:

| phase | time | scaling, 1,000 → 6,247 files (6.25×) |
|---|---|---|
| lexical_reserve | 0.146 s | 5.15× |
| `add_module` (bind, less classify/fill) | 0.128 s | 4.9× |
| check_statements + flow graph | 0.113 s | 5.82× |
| finish_event_effects (replay) | 0.091 s | 6.20× |
| parse | 0.090 s | 4.97× |
| `finalize_namespace_metadata` | 0.068 s | 6.63× |
| fill_type_decls_range | 0.057 s | 5.63× |
| imports + dependency order | 0.054 s | 4.9× |

Every row is linear. A ~3× constant spread evenly across eight unrelated phases is the signature of a
representation change or a per-declaration cost added everywhere, not of one bad loop — which is why
searching for a hotspot will fail and a bisect will not.

Note the related-but-distinct precedent: an earlier bisect this sprint pinned the *modules* blow-up to
`a7923b6` (2026-07-14, immutable class semantics cutover) and found it had also silently cost
assignability column precision ([`90`](./90-assignability-span-precision.md)). The window here starts
earlier — 2026-07-09 — so `a7923b6` is a candidate but must not be assumed.

## Approach / acceptance

Bisect `modules-100000` wall clock **and** peak RSS between the 2026-07-09 commit behind
`report/raw/20260709-174055` and HEAD. Two cautions learned the hard way in this sprint:

- **Prove build-input equivalence at each boundary** — the corpus must be md5-identical across the
  two commits being compared, or the bisect measures corpus drift.
- **Check diagnostics at the boundary too.** The `a7923b6` bisect found zero diagnostics gained or
  lost but one silent quality regression; a pure-perf conclusion needs the diff to say so.
- The `modules` corpus emits **no diagnostics and derives zero attachment targets**, so it can time a
  regression but can never witness a behavioural one. Pair it with a namespace-bearing corpus.

Acceptance: the responsible commit (or commits) named with build-input equivalence proven at the
boundary, the mechanism explained, and either a fix or a filed follow-up per cause. Target is the
2026-07-09 profile: ≤0.35 s and ≤200 MB on `modules-100000`, which is also what it takes to beat
`tsgo` on this family again.

## Touch points

Unknown by construction — that is the point of the bisect. Start from the phase table above;
`src/binder/`, `src/check/checker/`, and whatever changed in type representation between 07-09 and
HEAD are the likely surface.

<!-- Origin: complexity hunt of the modules knee, 2026-07-25. The hunt closed the exponent question
     (one term, = 93) and exposed this underneath it. Timing and RSS independently reproduced by the
     leader against the committed 07-09 report. -->

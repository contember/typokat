---
id: 43
title: Namespaces (type side) + declaration merging
---

# 43 — Namespaces (type side) + declaration merging

**Summary.** Namespaces and declaration merging are absent. Architecture §4/§4.1 makes
both **mandatory core**: `lib.d.ts` itself uses `namespace`+`interface` merges (`Symbol`,
`globalThis`) and qualified names `N.T`, and interface+interface merging is ubiquitous in
ambient code. A hard prerequisite of `14`.

## Problem

Per §4.1's keep-list: interface+interface merge, namespace(type container)+interface,
function+namespace, class+namespace (statics); qualified type references `A.B` (tsc
`TS2503`/`TS2694` when unresolved); nested namespaces. The three-way
`enum`+`namespace`+`function` chimeras stay degraded (§4.1). The binder's multi-slot
symbol design anticipated exactly this — the namespace slot exists but is unused.

## Approach / acceptance

Bind namespace declarations into the namespace space; resolve qualified names through it;
merge same-name interface declarations (member union, tsc's conflict rules) and the §4.1
keep-pairs. Corpus first: merge shapes, qualified names in type position, ambient-style
declarations; cross-check tsc 6.0.3 --strict. The official-suite `syntax:namespace` gate
opens after landing.

## Touch points

`src/binder/` (namespace slot activation, merging), `src/check/checker/annotations.rs`
(qualified names), `src/types/` (merged-interface identity).

<!-- Origin: completion-roadmap review (2026-07-07); architecture §4.1 requirement, prerequisite of 14. -->

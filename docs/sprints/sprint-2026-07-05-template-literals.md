<!--
On close, prepend an OUTCOME block here, then `git mv` this file to ../archive/.
-->

# Sprint — template literal types `` `a${T}` `` / M27 (2026-07-05)

**Goal.** Ship backlog [`11`](../backlog/11-template-literal-types.md): template literal types
are lowered to an interned node, **constructed** when all holes are concrete (literal collapse,
cartesian union distribution, `boolean` expansion, `never` short-circuit, numeric
stringification), act as **patterns** in the relation engine (anchored segment matching for
`${string}`/`${number}` holes, correct directionality), support **`infer` extraction** on
literal anchors (non-greedy), and stay **deferred** on unresolved params (conservative model +
the deferred-template → `string` allowance).

**Theme.** Third milestone of the type-level phase; completes the trio the utility milestone
(`12`) builds on. Today template literal types are silently permissive (probe: every m27
fixture passes with exit 0). Builds on the shared evaluator (work-stack/memo/budget), the M25
conditional/infer machinery (template patterns in extends), and the M25/M26 deferred-node
relation model.

## Refs re-verified at HEAD (2026-07-05, 6752f56)

- ✔ **Template literal types unchecked today** — probe: `const g: \`hello ${string}\` =
  "goodbye world"` → exit 0. The official-suite harness still GATES `template-lit-type`
  (`tsofficial.py`) — the leader lifts it at ratchet time.
- ✔ **Evaluator machinery** — `src/check/checker/eval.rs` (work-stack, memo with provisional
  discipline, budget); M25 conditional nodes + the non-widening infer collection mode
  (`infer_from_types_for_conditional`, `src/check/infer.rs`); M26 union-distribution guard in
  `src/types/substitute.rs` (`apply_mapped` — the M27 template analog distributes holes).
- ✔ **Deferred-node relation dispatch** — `src/relate/relation.rs` carries the M25/M26
  conservative rules; template patterns add a new relation arm (literal ↔ pattern, pattern ↔
  pattern) that must go through the same dispatch.
- ✔ **String literals** — `LiteralValue::String` interned with precomputed hashes; unions
  canonicalize (cartesian products must canonicalize like any union).
- ✔ **tsc 6.0.3 probes** (scratchpad `m27_p1`/`m27_p2`): all corpus behaviors, incl. pattern
  subsumption (`\`hello ${string}\`` → `\`h${string}\`` yes, → `\`x${string}\`` no), `string`
  NOT assignable to any pattern with literal text, boolean → `"is:false" | "is:true"` (display
  order tsc-side), infer-in-template resolving, adjacent-hole tsc semantics (first hole = one
  char — out of scope, poisoned).

## Work units

### WU1 — Repr + lowering + construction (effort M)

- **Scope.** Interned template-literal node: alternating literal text segments and hole
  `TypeId`s (canonical form: no empty interior text between adjacent holes is representable —
  adjacent holes are a lowering-time fact used by WU3's poison rule; leading/trailing empties
  normalized). Hash/eq/substitute (holes are ordinary types; substitution may make holes
  concrete). **Construction** (evaluator): when every hole is a string/number/boolean literal
  or a union thereof — collapse to a literal or the cartesian-product union (canonicalized,
  through the shared budget so combinatorial blowups hit TK2589, not memory); `never` hole →
  `never`; `boolean` expands to the two literals. Non-literal holes (`string`, `number`,
  unresolved params) keep the node symbolic. Demand sites: the M25/M26 set.
- **Acceptance / witness.** `construction.ts` + the `deferred_generics.ts` instantiation lines.
- **Touch points.** `src/types/repr.rs`/`hash.rs`/`intern.rs`/`substitute.rs`,
  `src/check/checker/annotations.rs` (lowering), `eval.rs` (construction), `diagnostics.rs`
  (display: backtick form).

### WU2 — Pattern relation rules (effort M)

- **Scope.** In the relation engine (existing dispatch, reason chains): (1) **string literal →
  pattern**: anchored segment matching — literal prefix/suffix/separator anchors must appear in
  order; `${string}` holes match any (possibly empty) segment; `${number}` holes match a
  numeric segment (decimal forms; scientific/edge forms may conservatively fail — over-report,
  document). (2) **`string` → pattern**: NO (unless the pattern is a single bare `${string}`
  hole with no literal text). (3) **pattern → `string`**: yes. (4) **pattern → pattern**:
  subsumption via anchor containment (the probed `h${string}` case); conservative `No` where
  unclear (over-report, document). (5) Deferred templates (free params in holes): identical
  node; deferred → `string` allowed; nothing else in.
- **Acceptance / witness.** `pattern_assignability.ts`, `deferred_generics.ts`.
- **Touch points.** `src/relate/relation.rs` (one new arm + the deferred rules).

### WU3 — `infer` in template patterns (effort M)

- **Scope.** Extend the conditional-evaluation matcher: an extends type that is a template
  pattern with infer holes matches a string-literal check type by **non-greedy anchored
  scanning** (first-anchor split; prefix/suffix/middle shapes per the corpus); matched segments
  become string-literal candidates through the existing non-widening collection mode;
  distribution composes (per-member extraction). **Adjacent infer holes (no literal separator
  between them) POISON the conditional at lowering** (the M25 cross-binder mechanism — never
  evaluates, conservative relations; documented divergence, tsc resolves with one-char-first
  semantics). No-match → false branch.
- **Acceptance / witness.** `infer_extraction.ts` (incl. the poison line and the m5-style
  regression guards in m25 corpora staying green).
- **Touch points.** `eval.rs` (matcher), `annotations.rs` (adjacent-hole poison at lowering),
  `src/check/infer.rs` (candidate positions for template holes).

### WU4 — Independent adversarial review + ratchet (effort M)

- **Scope.** Hunt false negatives, cross-check tsc: pattern-matching edges (empty segments,
  repeated anchors, unicode/multibyte text, `${number}` numeric forms), construction ×
  distribution interplay (cartesian canonicalization, memo soundness), infer edges (anchor at
  string start/end, empty matches, same-name multi-hole unification, template infer × M24
  constraints), deferred conservatism, poison boundaries (a healthy separator-ed pattern next
  to an adjacent-hole pattern), display stability. Then ratchet — LIFT the harness
  `template-lit-type` gate and audit every file entering scope.

## Out of scope (explicit)

- `Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize` intrinsics — backlog `12` (arith/string
  intrinsics per architecture §7.3).
- Adjacent infer holes (poisoned — documented; a future item may add tsc's one-char rule).
- `infer X extends C` in template holes; mapped `as` key remapping with templates (backlog
  `11`'s original `as` note moves with utility work).
- Scientific-notation/bigint numeric hole forms beyond simple decimals (conservative,
  documented as they arise).

## Decisions

- **Adjacent-hole poison** reuses the M25 poison mechanism — one flag, one discipline.
- **Cartesian construction runs under the shared step budget** (TK2589 on blowup) — no
  separate cap.

## Run log

<!-- Append as you work. -->

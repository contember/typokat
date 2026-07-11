# Sprint — JS-exact number stringification (2026-07-11)

**Goal.** Close backlog `30` by making numeric template-literal construction and
validation use ECMAScript `Number::toString` spellings without broadening the
template-pattern grammar.

**Theme.** A single shared formatter currently loses `TK2322` at JavaScript's
fixed/exponential thresholds. This sprint pins the boundary matrix first, replaces
only that formatter, and independently attacks both construction and pattern paths.

## Refs re-verified at HEAD (2026-07-11)

`✔` = confirmed live · `⚠` = drift/nuance caught.

- ✔ `number_to_string` is centralized in `src/types/repr.rs:670` and is reused by
  template construction, template-pattern relation, and template `infer` helpers.
- ✔ Rust `f64` display emits fixed digits for `1e21`, while TypeScript 6.0.3 types
  `` `${1e21}` `` as `"1e+21"`; the disabled backlog witness therefore demonstrates
  a real dropped `TK2322`.
- ✔ `dragonbox_ecma 0.1.12` is already present in `Cargo.lock` through OXC and
  exposes the ECMA formatting used by OXC's own JavaScript string conversion. A
  direct dependency edge adds no new resolved package.
- ⚠ `${number}` validation also has a conservative lexical gate. Expanding its
  accepted grammar is not necessary to remove the known under-report and remains
  outside this sprint.
- ✔ `C-numeric-stringification` is release-blocking and incomplete in
  `docs/backlog/completion-1.0.toml`; backlog `30` is its sole owner.

## Work units

### WU0 — focused boundary spec (effort S)

- **Problem.** The current deferred-ledger witness pins only `1e21` and one small
  control, so a formatter swap could still drift at the `1e20`/`1e21` and
  `1e-6`/`1e-7` boundaries or mishandle negative zero and shortest-round-trip values.
- **Verify first.** Cross-check every planned construction and `${number}` assignment
  against `tsc 6.0.3 --strict` before committing markers.
- **Scope.** Add a dedicated disabled `b30_numeric_stringify/` corpus covering both
  exponential thresholds, signs, `-0`, maximum finite, minimum subnormal, and a
  shortest-round-trip representative. Include pattern controls that prove the known
  non-canonical long form is rejected without claiming broader grammar parity.
- **Acceptance / witness.** At old HEAD, enabling only `b30_numeric_stringify` fails
  on the known dropped errors while all clean controls match tsc. Commit the disabled
  corpus independently.
- **Touch points.** `tests/cases/b30_numeric_stringify/`, `tests/cases/README.md`,
  `tests/conformance.rs`.

### WU1 — replace the shared formatter (effort S)

- **Problem.** `format!("{n}")` is shortest-round-trip but does not use JavaScript's
  fixed/exponential notation thresholds.
- **Verify first.** Unit-probe `dragonbox_ecma::Buffer::format` for every WU0 value,
  especially `-0`, non-finite values, and exponent sign/casing.
- **Scope.** Add a direct `dragonbox_ecma` dependency and route finite non-zero
  `number_to_string` values through it. Preserve explicit JavaScript spellings for
  zero, NaN, and infinities; do not hand-roll dtoa or change numeric-property parsing.
- **Acceptance / witness.** Enable `b30_numeric_stringify`; all construction and
  pattern diagnostics match the committed spec, existing M27 and negative-literal
  corpora remain green, and `Cargo.lock` gains no package/version churn.
- **Touch points.** `Cargo.toml`, `src/types/repr.rs`, focused unit tests,
  `tests/conformance.rs`.

### WU2 — independent adversarial review and closure (effort S/M)

- **Problem.** Exact formatting at the headline thresholds does not prove the matcher
  stopped accepting alternative spellings or that ordinary decimal rendering stayed
  stable.
- **Verify first.** A reviewer independent of WU1 reruns a tsc matrix around both
  thresholds, signed values, subnormal/max-finite values, round-trip traps, and
  non-canonical `${number}` strings.
- **Scope.** Fix only defects within the shared formatting slice. On PASS, run all
  quality gates and the official-suite identity ratchet; remove the shipped divergence,
  complete the manifest criterion, delete backlog `30`, and archive this sprint.
- **Acceptance / witness.** Independent PASS with no remaining construction or
  matcher under-report in the reviewed matrix; format, full tests, clippy, release
  build, and official-suite `run --check` all pass.
- **Touch points.** Read-only diff/probes, then backlog/divergence/manifest indexes and
  sprint archive docs.

## Out of scope (explicit)

- Broadening `${number}` lexical grammar for signed/scientific/uppercase spellings;
  conservative rejections remain safe-direction behavior unless a separate scoped item
  proves release relevance.
- Numeric property-name parsing and index-signature parity (backlog `62`).
- Decimal literal parsing precision, bigint formatting, arithmetic evaluation, or a
  new numeric representation.
- Backlog `13`: its profiling gate needs function-level profiler access, currently
  blocked by the host's `perf_event_paranoid=4` policy.

## Decisions

- Use `dragonbox_ecma 0.1.12`, already locked through OXC, rather than adding another
  dtoa implementation or writing one locally. This is a narrow implementation detail
  explicitly allowed by backlog `30`, not a new architecture decision.
- Preserve the matcher's existing lexical gate. The sprint closes the known
  stringification under-report, not every `${number}` parity edge.
- Follow the mandatory loop: leader-owned disabled spec commit, Terra subagent
  implementation, different-agent adversarial review, leader verification and closure.

## Sequencing

| Order | Unit | Gate |
| --- | --- | --- |
| 1 | WU0 | Disabled boundary corpus committed and tsc-cross-checked. |
| 2 | WU1 | Focused corpus enabled; dependency and formatter diff remain narrow. |
| 3 | WU2 | Independent PASS, full gates, audited closure/archive. |

## Run log

<!-- Append discoveries, deviations, and blockers here. Graduate durable findings to a
     decision/backlog/reference document; leave only transient execution notes here. -->

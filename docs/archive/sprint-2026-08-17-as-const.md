> **OUTCOME — shipped 2026-09-09.** Bounded `as const` is modeled: literal retention, readonly
> literal-owned properties and tuples, reference boundaries, literal-owned instance fields, flat
> binding defaults and ready enum keys. Feature commit `6b3cd507`, reviewed official scoreboard
> `634d1535`, spec re-plan `2911e105`; the earlier accepted M2 candidate (`dab3f745…`, `83b9aaaa`)
> was reconstructed byte-exactly on `main` and repaired in three bounded units (R-A dependency-ordered
> constraint fixing, R-B binding defaults + enum keys, R-C registration). Independent adversarial
> review: R-A PASS; R-B FAIL on one silent false-negative class (untyped binding default dropped
> the leaf), repaired and re-verified on every repro; one pre-existing candidate over-report
> (`<T, U = T>` contextual default) repaired in R-A. Verification on the unchanged final tree:
> full `cargo test` 1,923 passed / 0 failed, `cargo clippy --all-targets -- -D warnings`, `fmt`,
> ordinary conformance `660/776/660/0` (13.64 s), parity `660/776/660/660` (29.45 s), differential
> repros unchanged and three 400-program seeds against `main` with 20 message-only divergences
> (widened literal rendering, zero lost diagnostics), official suite 874 cases with 4 progress and
> 2 reviewed movements. Against pre-feature `main` over all 116 `b44_as_const` fixtures: 414 rows
> fixed, 136 shared gaps ledgered (owners 120, 69, 123, 124, 51/60/76), 11 accepted deviations
> (9 TK2339 over-reports, owner 69; one symmetric union-order pair, owner 120). Not delivered:
> ADR-0040 execution (moved to backlog 120 with its U1 patch retained under the evidence root),
> R2b row views (69), class member selection (123), ready collection projection (124).

# Sprint — AST-directed `as const` contexts (2026-08-17)

**Goal.** Finish the `as const` half of backlog 44 (closed with this sprint; see
`completion-1.0.toml` criterion `A-satisfies-as-const`): retain literal
values, make literal-owned object properties readonly, infer readonly tuples, and stop const
propagation at references. The [satisfies sprint](../archive/sprint-2026-08-17-satisfies.md) shipped.

**Standing at closure (2026-09-09).** Shipped; see OUTCOME. The accepted cumulative M2 candidate
(`dab3f745…`, recorded in `83b9aaaa`) is the delivery baseline. It was reconstructed byte-exactly on
current `main` from the frozen output manifest (2,088 entries; all code paths hash-verified) and
behaves identically to the frozen binary on all 116 `b44_as_const` fixtures. The registered-callable
U1 work-in-progress (ADR-0040) is **not** part of the delivery; its patch stays under the evidence
root and its execution moves to backlog [120](../backlog/120-semantic-constraint-construction-order.md).
No push, publish or deploy is authorized.

## Delivery boundary

The user authorized autonomous completion of the bounded feature: wider gaps get explicit backlog
ownership, but no required diagnostic may be lost relative to `main`, and no test may be bypassed.

| In scope | Outside this delivery (owner) |
|---|---|
| Primitive/enum literals; nested literal-owned objects and arrays; readonly properties/tuples; parentheses, shorthand and reference boundaries; suppression of outer context inside const syntax | Spread/elision: 71; computed keys, const-context object methods, invalid-operand TS1355: 75 |
| Literal-owned `as const` in instance fields, ordinary readonly modes, annotations and ready-context controls | General contextual class-initializer preparation, local-class member selection (ADR-0042): 123 |
| Flat binding defaults with correct authority/combination/widening; ready enum keys at a direct enum root | Ready collection reference projection in class initializers: 124 |
| Every regression the feature introduced relative to `main` | Cyclic-constraint completion order beyond dependency-ordered fixing; ADR-0040 execution: 120 |
| Documented over-reports in the safe direction | Correlated rest-parameter row views (ADR-0033): 69 |

## Measured classification (M2 candidate versus `main` `27225f03`)

Both binaries were run over all 116 `b44_as_const` fixtures and diffed per line against the
markers (`compare.py` under the scratch root; receipts `cmp-main-m2.json`; final receipts `cmp-main-final.json`).

| Class | Rows | Disposition |
|---|---|---|
| Feature progress (M2 right, `main` wrong) | 391 | Delivered |
| Shared gaps (`main` and M2 wrong the same way) | 152 | Explicit backlog owners below; fixtures stay registered as disabled rows |
| Shared gaps, different wrong answers | 7 | Same owners (rest correlation, enum key miss) |
| **Regressions** (`main` right, M2 wrong) | **81** | **Must be repaired before closure** |

The 81 regressions decompose into exactly three root causes:

1. **Cyclic-constraint fixed-point loop — 60 rows.** M1 wrapped both native fixers
   (`fix_signature_params_attempt`, `fix_present_signature_params_attempt`) in a `2n+1` fixed-point
   loop that substitutes each constraint through its own previous fallback. Cyclic constraints never
   converge, so clean calls report `TK2589` and invalid calls lose `TK2345`/`TK2322`. Restoring one
   pass removes all 60 rows (verified: `cmp-main-onepass.json`) but loses 12 rows the loop got right:
   reverse-dependent constraints (`<U extends T, T extends string>`) and later-parameter defaults
   (`<T extends { u: U }, U = T>`).
2. **Flat binding defaults — 7 rows** (`binding_default_widening.ts:4/5/7/8/12/13`,
   `binding_default_mode_combination.ts:9`). The default is checked against the projected source
   property as if annotated and its inferred type is discarded (`annotation.or(initializer)`).
3. **Ready enum keys — 3 rows** (`enum_key_and_widening.ts:9`, `enum_ready_key_boundaries.ts:9`,
   `enum_ready_key_result_identity.ts:3`). `Enum[readyKey] as const` reports
   `expr-infer/as-const/invalid-operand`; the operand validator accepts only syntactic keys.

The remaining official-suite deltas are `literalTypesAndTypeAssertions.ts:12/13` (cause 2) and eight
new `TK2339` over-reports in `dependentDestructuredVariables.ts`, a file already `OOS:unsupported`
with 46 false positives of the same class on `main` (owner 69, ADR-0033).

## Repairs (leader spec)

### R-A — dependency-ordered constraint fixing (owner: `check/infer/mod.rs`, `infer/tests.rs`)

Replace the fixed-point loop in both fixers with **demand-ordered, once-per-parameter fixing**:

- Fix each declared parameter at most once. Before fixing `P`, fix every not-yet-fixed declared
  parameter that `P`'s constraint or default mentions (`derived_free_params` from
  `typokat-types::substitute`), recursively. A parameter already being fixed on the active path is a
  cycle: skip it, so the substitution sees its raw candidate if any, otherwise the symbolic parameter.
- Substitute constraints and defaults through the working map exactly as today. No parameter's
  derived fallback is ever re-substituted into its own constraint; therefore no evaluation-budget
  exhaustion and no `TK2589` can originate from the fixers. Remove the `Exhaustion::EvaluationBudget`
  return; coordinator exhaustion of a constraint demand keeps its existing propagation.
- Preserve: raw-candidate map semantics for inherited binders, `fresh_exempt`, `constraint_violations`,
  M28 deferred-`keyof` gate, default-after-constraint order, `NeedsNativeIdentities` propagation, and
  declaration-ordered `ordered` output of the partial fixer.

Acceptance:

- `reversed<U extends T, T extends string>("child" as const, 1 as const)` reports exactly
  `TK2345` at `1` and `TK2322` at `base` (existing test
  `direct_const_candidates_revalidate_reverse_dependent_constraints`).
- Over the 116 `b44_as_const` fixtures: zero rows where `main` matches the marker and the candidate
  does not, for every file in the `constraint_*`, `recursive_*`, `source_construction_*` and
  `erased_*` families. Of the 12 loop-only rows, `recursive_constraint_seed_identity.ts:33/34`,
  `recursive_constraint_use_order_supplied.ts:22/23/30/31` and
  `recursive_default_preparation.ts:7/8/13` stay correct. `erased_constraint_supplied.ts:7/15` and
  `recursive_constraint_use_order_supplied.ts:16` may fall back to `main`'s false `TK2322` (cyclic
  supplied candidates; owner 120).
- No `TK2589` from the fixers anywhere in `tests/cases`; `cargo test -p typokat-check` green; the
  M2 native-scaling and P/R ownership tests unchanged.

### R-B — flat binding defaults and ready enum keys (owner: `checker/statements.rs`, `checker/decls/resolve.rs`, `checker/expr.rs`, `checker/classes/publication.rs`, `checker/tests/*`)

Implement groups 1 and 2 of `m3-current-preparation.md` (evidence root) exactly as contracted there.

Defaults (`check_declarator`, `publish_flat_object_binding_property`, `strip_binding_undefined`):

- An explicit annotation keeps its original default obligation and declared publication authority.
- Without an annotation, the leaf type is the combination of the source contribution minus
  `undefined` and the default inferred once through the existing contextual initializer path; no
  outer annotation assignment or excess-property obligation is added. Combine the mode-bearing IDs
  before `declared_from_init`: `const` retains modes; `let`/`var` widen only surviving widening
  contributions, after ADR-0031's same-value regular/widening collapse.
- Witnesses: `binding_default_widening.ts`, `binding_default_mode_combination.ts`,
  `binding_default_context_and_authority.ts` fully green (18 rows), `b48_object_binding_publication`
  unchanged, official `literalTypesAndTypeAssertions.ts` back to clean.

Enum keys (`enum_member_*_endpoint`, `infer_const_assertion`, `seed_const_enum_root`):

- Admit a ready singleton string identifier key (parentheses transparent) read through tracked
  declaration reads at a canonical **direct enum value root**; alias, shadow, namespace-collision and
  unavailable roots keep `invalid-operand`. A missing identifier key keeps the indexed-access
  incomplete (`expr-infer/element-access/implicit-any-index`), not `TK2339` or `invalid-operand`.
- Exactly one key walk; class fields seed the already-ready annotated key through the existing
  direct-root helper.
- Witnesses: `enum_key_and_widening.ts`, `enum_ready_key_boundaries.ts`,
  `enum_ready_key_result_identity.ts` — the four M3 enum rows green, every negative retained.

### R-C — registration, tag reconciliation and closure (leader-owned after R-A/R-B)

- Register `b44_as_const` as a mixed corpus: every fully green fixture through `ENABLED_FIXTURES`,
  every fixture with a remaining shared-gap row disabled with its owner listed in
  `tests/cases/README.md`; update the frozen inventory receipt and fingerprint through the harness.
- Reconcile `tests/type_tag_exhaustiveness.rs` for the `WideningLiteral` tag (frozen variant list
  and reviewed wildcard adjudication).
- Ledger every remaining shared gap in `divergences.md` with owner and witness; create backlog 124;
  extend 120, 69 and 123 with the exact rows.

## Closure gates (all executed on the unchanged final tree; results in OUTCOME)

1. Independent adversarial review of R-A and R-B with `tsc 6.0.3 --strict` cross-checks.
2. `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.
3. Ordinary exact conformance with the new receipt; repeated/standalone parity once.
4. Differential: `repros --check`; `fuzz --ref <main binary> --count 400` seeds 1–3 with every finding
   classified as intended `as const` progress or repaired; truth mode against `tsc` for the findings.
5. Official `run --check`: every movement classified; `--save` only after review, with the eight
   `dependentDestructuredVariables.ts` over-reports recorded under owner 69.
6. Integrate into primary (superseding the older WU1 edits), close backlog 44, archive this sprint
   with OUTCOME. Protected user WIP (`docs/INDEX.md`, `docs/ideas/*`, `claude-out.md`) is never staged
   with sprint work.

## Evidence

- Candidate tree and binaries: scratch root of the closing session (`m2-int`, `typokat-main-27225f03`,
  `typokat-m2-int`, `cmp-*.json`).
- Frozen M2 acceptance: `m2-fixed-arity-leader-closure.json`, `m2-fixed-arity-product-*` under
  `/tmp/opencode/typokat-b44-feasibility-evidence/`.
- Retained but undelivered: `registered-callable-u1-authority-*` (ADR-0040 U1 patch, non-compiling).
- Full history of earlier candidates, rejected repairs and gate receipts:
  `git show 27225f03:docs/sprints/sprint-2026-08-17-as-const.md`.

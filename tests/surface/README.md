# Surface-accounting fixtures (sprint 2026-07-10, WU0)

This directory is the **code-adjacent schema fixture data** for the executable
surface-accounting validator (`tests/surface.rs`) and the remaining semantic-disposition tail
(backlog [`75`](../../docs/backlog/75-scope-surface-tail.md)). The validator consumes the
inventory and fixtures under `cargo test --test surface`.

## Why a surface inventory exists

`typokat` traverses the `oxc` AST in **five independent layers**, each of which dispatches on
node kind with its own wildcard/`None`/skip fallback (see [`census.md`](./census.md)). A prose
list of "unsupported syntax" cannot detect a newly added `oxc` variant, a supported wrapper
whose child slot is skipped, or divergent coverage across layers. The inventory turns
completeness into an **executable, machine-validated** record keyed by a stable identity that
does not depend on `oxc`'s display names (so an `oxc` rename is caught as version drift, not
silently absorbed).

## Stable identity scheme

Every surface record is keyed by a stable id, independent of `oxc` enum spelling:

```
<id>    ::= <role> "/" <surface> "/" <slot-or-variant>
<role>  ::= bind | flow | type-fill | stmt-check | expr-infer
          | annotation-lower | call | decl | class | signature
<surface>        ::= kebab-case logical construct family (NOT the oxc variant name)
<slot-or-variant>::= kebab-case child slot, or `self` for the node's own semantics
```

- **`role`** is the *dispatcher layer* (which traversal owns the position), not the node. The
  same construct can appear under several roles with different coverage — that is the point.
- **`surface`** is an `oxc`-independent name for the construct family (`template-literal`,
  `object-literal`, `try-statement`, `type-query`). The `oxc` variant it maps to is recorded
  in a separate `oxc_variant` field so a rename shows up as a drift diff, not as an id change.
- **`slot-or-variant`** names the child position (`interpolation`, `computed-key`,
  `spread-element`, `handler`) or `self` when the record is about the wrapper's own semantics.

Three examples (all live in [`census.md`](./census.md)):

1. `expr-infer/template-literal/interpolation` — the `${…}` child slot of a template literal,
   as seen by the expression inference walker.
2. `stmt-check/try-statement/handler` — the `catch` block of a `try` statement, as seen by the
   statement checker.
3. `annotation-lower/type-query/typeof` — a `typeof X` type-query annotation, as seen by
   annotation lowering.

## Record schema (`[[surface]]`)

The manifest is a hand-parsed TOML subset, mirroring
[`docs/backlog/completion-1.0.toml`](../../docs/backlog/completion-1.0.toml) and the
`tests/manifest.rs` precedent (no serde/toml dependency). `#` comments and blank lines are
ignored; `[meta]` opens the singleton meta table; `[[surface]]` opens a record; values are a
quoted string or a bracketed array of quoted strings.

`[meta]` fields:

| Field | Meaning |
|---|---|
| `oxc_version` | Pinned `oxc` version. Must equal `Cargo.toml`'s `oxc_ast` pin; a mismatch is drift and the validator rejects it. |

`[[surface]]` fields:

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | The stable `role/surface/slot-or-variant` identity. Unique across the manifest. |
| `role` | yes | One of the `<role>` values above. |
| `surface` | yes | The `oxc`-independent construct family. |
| `slot` | yes | The child slot / variant, or `self`. |
| `oxc_variant` | yes | The `oxc` enum path this record maps to (drift witness), e.g. `Expression::TemplateLiteral`. |
| `disposition` | yes | `supported` \| `unsupported-in` \| `design-oos`. |
| `owner` | yes | For `unsupported-in`: a live backlog path (`../../docs/backlog/NN-*.md`). For `design-oos`: `by-design`. For `supported`: `shipped`. |
| `witness` | yes | A fixture path or descriptor proving the disposition (a `tests/cases/**` fixture for `supported`; an incomplete fixture or concise semantic boundary for `unsupported-in`). |
| `requires_slots` | when `supported` and the node has in-scope children | The child slots this wrapper MUST visit; each must have its own covering `[[surface]]` record. Enforces "a supported wrapper does not silently drop an in-scope child". |

## What the WU1 validator must reject (the fixtures below)

| Fixture | Category | Expected rejection |
|---|---|---|
| `fixtures/valid.surface.toml` | valid | none — validates cleanly |
| `fixtures/duplicate.surface.toml` | duplicate record | two `[[surface]]` records share an `id` |
| `fixtures/missing.surface.toml` | missing record | a `supported` wrapper's `requires_slots` names a child slot with no covering record |
| `fixtures/dependency_drift.toml` | dependency drift | a completion criterion's `deps` disagree with its owner's `blocked-by` frontmatter (the historical `14`/`70` drift) |
| `fixtures/malformed_divergence.md` | malformed divergence metadata | a `divergences.md`-style row with malformed / missing inline metadata (bad `dir`, missing `owner`/`witness`) |

Each fixture carries a header comment restating the exact rejection so WU1 can assert against it.

## Related WU0 artifacts in this directory

- [`census.md`](./census.md) — the recorded dispatcher-role / child-slot census, `oxc` variant
  counts, wildcard `file:line` map, and the **split-gate verdict**.
- [`probes.txt`](./probes.txt) — the exact CLI outcome snapshots for the five pinned probes
  (typokat exit code + output vs `tsc 6.0.3 --strict`), captured at HEAD as committed witness.

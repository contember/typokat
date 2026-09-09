---
id: 127
title: Replace partial Rust scanners and spelling assertions in the architecture test guards
blocked-by: []
---

# 127 — Replace partial Rust scanners and spelling assertions in the architecture test guards

**Summary.** Four architecture guards each maintain their own incomplete Rust grammar, and two of
them assert source spelling instead of behaviour. Graduated from the maintainability audit in
[`checker-semantic-boundaries-and-maintainability.md`](../ideas/checker-semantic-boundaries-and-maintainability.md)
(findings M2 then M3). Effort M; non-blocking maintenance. Re-verified on `main` `81147d86`
(2026-09-09).

## Problem

- **M2 — four partial parsers.** `sanitized` in `tests/module_accounting_structure_contract.rs:26`
  and `tests/type_tag_exhaustiveness.rs:95` are two near-identical ~90-line comment/string
  strippers (87 shared lines). `code_only` in `tests/cli_report_transaction_contract.rs:107` and the
  `delimiter_depths` brace counter in
  `crates/typokat-check/src/check/checker/decls/spec/eager_application_cache_spec.rs:282` are two
  more. Together the four guards carry ~680 lines of scanner machinery, each handling a different
  Rust subset. Textual call discovery is not name resolution; brace counting is not parsing. A
  future syntax change (raw strings, nested items, `cfg` attributes, macros) can break a guard or
  slip past it without any policy change.
- **M3 — spelling assertions.** `eager_application_cache_spec.rs:1093-1127` checks textual call
  order, the last `Some(` spelling and the count of `Arc<` across the whole `Pass` struct;
  `cli_report_transaction_contract.rs:263-319` requires an exact status expression and exact
  occurrence counts of each status flag. These reject equivalent rewrites but do not prove that a
  constraint call executes on every relevant path, and the cache-lifetime test constrains
  unrelated `Pass` fields.

## Approach / acceptance

1. **Share first.** Extract the near-identical root-test sanitizer into one root test helper if
   that alone is sufficient for the two root guards. Keep the guard *policies* independent; do not
   create a cross-crate test framework.
2. **Then parse.** Where a shared sanitizer is not enough, replace the scanner with a narrow
   `syn::parse_file` visitor owned by the guard (`syn` is a workspace dependency, `Cargo.toml:46`,
   with `full` + `visit`; it must be enabled as a direct dev-dependency of each owner). State the
   macro-token policy explicitly. The replacement must be smaller than the retired machinery; the
   ~680-line figure is a gross candidate surface, not a promised net deletion.
3. **Behaviour over spelling.** Keep structural checks for ownership, permitted callers,
   cfg-test telemetry and the cache field's own type. Move occurrence effects, cache
   lifetime/taint, status combinations and mixed-channel order to behaviour tests through the
   existing writer-injected CLI core and the existing cache behaviour tests.

Acceptance: every existing positive and mutation/decoy control still passes; add controls for
strings, comments, raw strings, character escapes, nested items and `cfg` attributes. Demonstrate
that the behaviour tests catch a deliberately misplaced constraint/effect and an incorrect status
priority before deleting any spelling assertion. Do not remove guard coverage merely to make a
refactor pass; complete CLI status/order coverage must be checked, not assumed.

Gates: `cargo test`, `cargo clippy --all-targets -- -D warnings`. No fixture or scoreboard
movement is expected.

## Touch points

`tests/module_accounting_structure_contract.rs`, `tests/type_tag_exhaustiveness.rs`,
`tests/cli_report_transaction_contract.rs`,
`crates/typokat-check/src/check/checker/decls/spec/eager_application_cache_spec.rs`, and the
dev-dependency tables of the owning packages.

<!-- Origin: ideas/checker-semantic-boundaries-and-maintainability.md (audit 2026-09-06), graduated 2026-09-09. -->

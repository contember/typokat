---
id: 59
title: M29 diagnostics hygiene — fill-phase drain attribution + export-list validation
---

# 59 — M29 diagnostics hygiene

**Summary.** Two same-area byproducts of the modules review (2026-07-07):

1. **Fill-phase override checks drain into the wrong module.** `check_project_programs`
   (`mod.rs:608-615`) drains only `pass.diagnostics` per module; `override_checks`
   collected during class fill (`classes.rs:1222`) accumulate globally and the first
   `emit_pending_checks` in the check loop (`mod.rs:620`, `:804-823`) drains them into
   `module_diagnostics[0]` — a cross-file `class D extends Base` override error renders
   at a nonsense position in the *base's* file (codespan clamps past-EOF spans; no
   panic). The verdict still fires (exit 1) — attribution only, but garbage spans.
2. **`export { ghost }` naming a nonexistent local is silent at the export site.**
   `collect_list_export` (`mod.rs:735-749`) inserts empty `ExportedSlots` without
   validating the local resolves; tsc reports TS2304 at the export site. If nothing
   imports the name, the project checks fully clean — a small FN.

## Approach / acceptance

Drain pending class-fill checks per module (or tag them with their module), and validate
export-list entries against the local scope at collection time. Acceptance: the
cross-file override probe renders in the derived file at tsc's position; `export
{ ghost }` reports at the export site with no importer needed; m29 corpus fixtures pin
both.

## Touch points

`src/check/checker/mod.rs` (`check_project_programs`, `emit_pending_checks`,
`collect_list_export`), m29 corpus.

<!-- Origin: cross-cutting soundness review 2026-07-07 (modules reviewer #2, #7), leader-verified. -->

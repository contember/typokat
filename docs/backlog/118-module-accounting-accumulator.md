---
id: 118
title: Represent project module accounting as one owned accumulator
blocked-by: []
---

# 118 — Represent project module accounting as one owned accumulator

**Summary.** Extract the owned scan/finalization state from `account_project_modules` while
preserving one AST scan and one frontend authority for module proof products. Non-blocking checker
maintenance, effort M.

## Problem

`crates/typokat-frontend/src/frontend.rs::account_project_modules` is about 570 lines and combines
syntax scanning, physical resolution, default candidates, unsupported notices, dependency graphs,
cycle analysis, namespace provenance, and final proof-product certification. Adding one admitted
module form requires editing a large loop whose one-pass and consistency requirements are implicit
in local variables.

Unlike the largest checker transactions, this function's mutable state consists of owned
collections. It does not need to retain a `Pass` that borrows its own fields, so it has a safe
decomposition seam.

## Approach / acceptance

Introduce a private owned `ModuleAccounting` accumulator (name not binding) initialized from the
canonical project paths and resolver context. It should scan each parsed program and each statement
exactly once, then consume itself to finalize source re-exports, default-module products, dependency
cycles, provenance, and unsupported notices into the existing typed result.

Do not split default, import, and re-export accounting into independent AST passes or authorities.
Preserve owner-start exclusions, physical-resolution calls, dependency ordering, deterministic
notices, route receipts, and all project-summary JSON. Add a scan-count witness and existing module
fixture parity before moving behavior.

Acceptance is a shorter linear facade, an accumulator with only owned fields, one scan per program,
unchanged project/conformance receipts, and no architecture or supported-module breadth change.

## Touch points

`crates/typokat-frontend/src/frontend.rs`, frontend module-accounting unit tests, Bundler project
fixtures, source/default re-export tests, and project-summary route tests.

<!-- Origin: 2026-08-20 development-friction audit; checker decomposition refutation. -->

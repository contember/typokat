---
id: 06
title: Class-completeness checks — method-override (TK2416) + abstract-not-implemented (TK2515)
blocked-by: []
---

# 06 — Class-completeness checks (TK2416 + TK2515)

**Summary.** Two small, well-isolated class-completeness checks deferred during the class phase. A
good lighter warm-up before (or instead of) the narrowing CFG (item 07).

## Problem

Two completeness checks are missing:

- **`TK2416`** — method-override compatibility: an overriding method whose signature isn't compatible
  with the base method's.
- **`TK2515`** — abstract-member-not-implemented: a non-abstract subclass that fails to implement an
  inherited `abstract` member.

## Approach / acceptance

Add both checks in the class-checking path (override compatibility via the relation engine; abstract
coverage by diffing declared abstract members against implemented ones). Acceptance: fixtures for an
incompatible override (`TK2416`) and an unimplemented abstract member (`TK2515`) matching tsc; no
regression on the existing class corpus.

## Touch points

Class checking — the inheritance/override path and the `abstract` handling.

<!-- Origin: dev roadmap (was HANDOFF §3, near-term). -->

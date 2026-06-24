# reference

How the system works **now** — architecture, conventions, runbooks. Flat files,
`kebab-case.md`.

Rules (see [`../CLAUDE.md`](../CLAUDE.md)): describe the current state only — no
status updates, no TODOs (file those in `../backlog/`), no design rationale (that's
a `../decisions/` ADR). Update reference in the **same change** that alters
behaviour.

- [`architecture.md`](architecture.md) — the full design (type store · binder · relation engine · checker; §12 phased plan).
- [`dev-method.md`](dev-method.md) — the mandatory milestone build loop (spec → implement → independent review) + the bug classes reviews keep catching.
- [`invariants.md`](invariants.md) — the binding soundness/architecture invariants + deliberate deferrals (precedence #1).

---
id: 0018
title: Pin the library's own records as a named census; retain none at runtime
status: accepted
date: 2026-07-27
---

# 0018 — Pin the library's own records as a named census; retain none at runtime

## Context

Compiling the pinned 82-file profile reports **875 records against the library itself** — 265
diagnostics and 610 incomplete surfaces. `LibraryEventLedger::finish` materializes them inside
`compile_owned_injected_frontend`, and on the route to a shared base they die when that function
returns. `OwnedLibraryRuntimeState` has no record store, and `FrozenLibraryBase` adds only root
names, prefixes and identity, so nothing that a process holds can answer what the library reports
about itself.

[ADR-0011](0011-freeze-pinned-default-library-base.md) requires the library-origin ledger to be
"preserved exactly". Today it is *computed* exactly and then discarded. That gap predates
[ADR-0017](0017-compile-the-default-library-from-source.md): while the snapshot existed, the
evidence blobs looked like retention but were a byte *projection* — they proved the records were
computed, never that they were kept — and they went with the snapshot. What survives at runtime is
the collision replay index's `baseline_records`, which is a per-owner record count plus a SHA-256
digest, not a record; and the shared base does not even carry that, because index assembly is
deferred to the run that collides.

Three facts decide the shape.

**The records are not user-facing errors.** They are typokat's own model gaps against a library the
real `tsc` checks clean — the oracle ADR-0011 already names for the library-origin ledger is
`tsc --strict --target es2025 --noEmit --noLib <the 82 manifest paths>`. So the requirement is not
"report them". It is that they must not be silently approximated away, and that their content stays
measurable, so that a regression in the model shows up as movement in that set.

**Retention is not free.** The project is under a live cross-tool performance and RSS contract —
ADR-0011's 5.00 s / 512 MiB release gates, and ADR-0017's obligation to restate a performance claim
the checker can actually support. 875 records with rendered messages, held in a process-wide `Arc`
that outlives every check, is memory spent on data no user reads.

**A digest is not a witness.** [Backlog `98`](../backlog/98-library-diagnostic-count-delta.md) is
the live example: the library's diagnostics went 273 → 265 and nobody could say which eight went,
because the pin was a count and a SHA-256 and it was 102 commits stale when the discrepancy
surfaced. A digest tells you something moved; it never tells you what. "Retain nothing" without a
named pin *is* the state that produced backlog 98.

## Decision

**We will retain no library-owned record in production, and pin the complete set in the suite as a
named `(code, site)` multiset.**

### Retention: none

`compile_owned_injected_base_profile` — the only route to a shared base — compiles with
`LibraryRecordRetention::Drop`. Nothing downstream of a compile keeps a record: not
`OwnedLibraryRuntimeState`, not `FrozenLibraryBase`, not the provider.

Draining the ledger stays. `LibraryEventLedger::finish` is the completeness gate that fails a
compile whose reservations were not all filled, which is ADR-0011's "parser/checker output exactly
matches the audited library ledger". Only *handing the drained records onward* is optional, and
`LibraryRecordRetention` is the seam that says so at each call site rather than by accident.

### Inspection: explicit

`typokat::library::LibraryRecordCensus::compile_packaged_profile()` compiles the profile from
source, names every record as `(kind, name, file:line:column, detail)` — a `TK` code for a
diagnostic, the stable surface id for an incomplete — and drops the semantic product it had to
build. It is a deliberate entry point, never a side effect of checking: no path that checks user
code reaches it.

### The pin: a named multiset, not a hash

`tests/fixtures/library-owned-records.txt` carries all 875 entries, one per line, sorted by site.
`tests/library_owned_records.rs` recompiles the census and diffs it against that file **as a
multiset**, failing with the specific entries added and removed:

```text
cargo test --test library_owned_records
TYPOKAT_BLESS_LIBRARY_RECORDS=1 cargo test --test library_owned_records   # re-pin deliberately
```

A `-` line is an outcome the pin carries that the checker no longer produces — the direction that
hides a dropped diagnostic. A `+`/`-` pair at the same code and site is a rendering change, which
is precisely the distinction backlog 98 could not make. The same target proves the other half on
the production-shaped path: `check_project_with_library` and `check_source_with_library` — the
entry points the WU7 cutover moves the CLI onto — report the user's own diagnostics and none of the
875.

### This narrows ADR-0011's "preserved exactly" clause

ADR-0011's requirement that the library-origin ledger be preserved exactly is narrowed to:
**preserved exactly in the pinned suite, not in the published base.** Two consequences of that
narrowing are stated here rather than left to be discovered:

- A process holds no baseline to compare against, so ADR-0011's private-rebuild rule that a
  library-origin outcome must be "either the exact pinned baseline or a deterministic record owned
  by an already-reserved user augmentation site" is asserted by the suite census, not by a runtime
  equality against retained records. The replay index's per-owner digests remain what private
  replay itself consumes; they are unchanged by this decision and are not a census.
- ADR-0011's "unexpected library-origin output is never suppressed" stays binding as written: no
  library-owned record is ever routed into user output, and none is silently dropped from the set
  the pin measures.

Every other ADR-0011 rule — the exact profile, the one shared library-global pipeline, the frozen
base/delta identity model, the conservative preflight, the private rebuild — is untouched.

## Consequences

- **Ordinary checks pay nothing.** No allocation, no retained bytes, no lifetime coupling between a
  user check and the library's own model gaps.
- **A moved integer becomes a named difference.** Adding or removing a library-owned outcome fails
  one test that names the entry by code and site. Backlog 98's failure mode cannot recur silently
  for drift *after* this pin; it does not retroactively attribute the eight records that already
  went, because the data to attribute them was never retained.
- **The pin is a maintenance surface, and deliberately so.** Any model improvement that closes a
  gap must re-bless it, and the diff is the review artifact. 875 lines is the current size of the
  model-gap debt against the standard library; it should shrink, and the pin makes that visible.
- **Inspection costs a full source compilation** — the census is not a query against a running
  process, and there is no cheaper answer as long as nothing is retained.
- **A shipped binary cannot answer "what does the library report about itself".** Only the
  repository can. That is accepted: the records are not user-facing, and a user who could see them
  would be reading typokat's internal gaps as if they were errors in their code.
- **Reversal is cheap.** `LibraryRecordRetention::Collect` already exists; a later product decision
  that needs in-process access changes one call site, not a design.

## Alternatives considered

**Retain the records in the base.** Backlog 99's option 1: honest, and the most direct reading of
ADR-0011's wording. Rejected because it costs memory in every process, for the entire life of the
shared base, for data no user reads and no code path consumes — under an RSS contract that is
already binding. The wording is better narrowed than satisfied literally.

**Retain a structured summary** — the `(code, file, span, id)` multiset without messages —
**and recompute the text on demand.** Backlog 99's option 2, and genuinely cheap. Rejected as
unnecessary rather than as wrong: the only consumer anyone has identified is regression detection,
and the suite is where regressions are detected. A summary in every process is still a cost with no
reader. This is the option to revisit if a production consumer ever appears — a WU7 CLI mode, or a
private-rebuild runtime comparison that wants more than per-owner digests.

**Retain nothing, and keep the digest pin.** Today's behaviour. Rejected: it is exactly what let
backlog 98's delta drift for 102 commits. The digest is retained only as a byte-identity check on
the canonical projection in `src/library/compiler.rs`; the census is the authority on *what*
drifted.

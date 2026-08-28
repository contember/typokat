---
id: 0029
title: Gate the library record census behind an opt-in feature
status: accepted
date: 2026-08-28
---

# 0029 — Gate the library record census behind an opt-in feature

## Context

[ADR-0028](0028-rebaseline-the-production-owned-library-census.md) makes the 709-row output of the
project-local production publisher the sole library-record authority. During WU4, that publisher
is still an inspection stack compiled by `typokat-check` only for tests or its `test-utils`
feature. `LibraryRecordCensus`, however, is currently an unconditional public API in
`typokat-library`.

Calling the project-local publisher from that unconditional API would require one of three
architecturally incorrect compromises: every production consumer would compile the complete test
inspection surface, non-test builds would retain the legacy producer, or WU4 would expose a new
production session seam before WU5 owns that cutover.

The census is deliberate verification tooling. Ordinary checks neither request nor retain its
library-owned records. Its temporary availability can therefore follow the inspection stack
without weakening ADR-0028's one-producer decision.

## Decision

`typokat-library` will add an opt-in `library-record-census` feature. The feature enables the
project-local test inspection stack required to build the production-owned force-all census.
`LibraryRecordCensus` and its related public census types are available only with that feature.

Default and production builds do not enable `library-record-census`, do not transitively enable
`typokat-check/test-utils`, and do not expose the census API. The root package and integration
tests enable `library-record-census` only through a dev dependency. The census has one producer:
one project-local session running test-only `force_all` over the immutable declaration product and
a benign project. No feature branch retains the complete-source or replay producer.

This is a temporary WU4 boundary. WU5 must reassess and normally remove the feature when the
project-local session becomes the production library lifecycle. At that point the census can use
the production session without carrying the test inspection stack; whether the public census API
should return is a separate API decision.

This decision implements ADR-0028's test-only producer and does not supersede ADR-0028. Its 709-row
authority, one-producer rule, and no-recovery rule remain binding.

## Consequences

- Verification and integration targets opt in explicitly and receive the project-local inspection
  contracts they need.
- Normal library and checker artifacts carry neither `test-utils` nor the census API.
- There is no cfg-dependent semantic producer: when the API exists, it uses the project-local
  publisher; when the feature is absent, the API does not exist.
- Downstream tooling that intentionally consumes `LibraryRecordCensus` must enable
  `library-record-census` until WU5 resolves the boundary.
- WU5 has an explicit deletion/reassessment obligation; the temporary feature must not become an
  accidental permanent architecture.

## Alternatives considered

**Enable `typokat-check/test-utils` unconditionally.** Rejected because every production consumer
would compile and expose a large inspection-only surface.

**Use the project-local producer in tests and retain the legacy producer elsewhere.** Rejected
because cfg-dependent producers recreate two authorities and directly violate ADR-0028.

**Expose a new production project-local session API during WU4.** Rejected because WU5 owns the
production lifecycle cutover. Adding that seam now would prematurely decide its module and API
boundary.

**Keep `LibraryRecordCensus` unconditional but fail at runtime without test utilities.** Rejected
because an API that is structurally unavailable should not compile successfully and then fail only
when invoked.

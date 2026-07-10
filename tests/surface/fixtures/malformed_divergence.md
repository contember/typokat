<!--
Surface-accounting fixture — MALFORMED DIVERGENCE METADATA (sprint 2026-07-10, WU0).

Expected WU6-validator outcome: REJECT the second row. WU6 makes divergences.md the
single human+machine source by attaching compact inline metadata to each divergence row:

    <!-- div: id=<stable-id> dir=under|over|cosmetic scope=<family|design-oos> owner=<path|shipped> witness=<path> -->

The validator rejects rows whose marker is missing, has an unknown `dir` enum, lacks an
`owner`/`witness`, or (for `dir=under`) has no scope disposition. This fixture pairs one
WELL-FORMED row with one MALFORMED row so WU6 can assert the rejection.
-->

# Divergence ledger (fixture excerpt)

## Deferred checks

- **Dropped call arguments (backlog 63g).** Call arguments beyond the collected
  `arg_types` are not related, so a bad trailing argument can go unreported.
  <!-- div: id=deferred/call/dropped-arguments dir=under scope=../../../docs/backlog/63g owner=../../../docs/backlog/71-expression-inference-fn-tail.md witness=shipped -->

- **Template interpolation not checked.** A bad expression inside a `${…}` hole exits
  clean.
  <!-- div: id=deferred/template/interpolation dir=sideways owner= -->
  <!-- MALFORMED: `dir=sideways` is not a valid direction enum (under|over|cosmetic),
       `owner` is empty, and there is no `witness` or `scope`. Must be rejected. -->

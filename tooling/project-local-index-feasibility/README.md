# Project-local library index feasibility gate

This WU0 gate authenticates the locked source slice and the exact TypeScript/complete-source
oracles before the experimental materializer exists.

```sh
python3 tooling/project-local-index-feasibility/spec.py oracle \
  --typokat target/release/typokat
```

The command requires installed `tsc` 6.0.3 and a release typokat binary from the same tree. It
checks every fixture in both file orders where applicable, validates diagnostic/incomplete counts,
and authenticates `locked-slice.toml` against the vendored source bytes.

Candidate work supplies a test-only runner and uses:

```sh
python3 tooling/project-local-index-feasibility/spec.py candidate \
  --runner path/to/project-local-index-runner
```

The runner reads one JSON batch request per process on stdin and writes exactly one JSON response
on stdout. It is not a production CLI route. Requests contain the locked profile identity, batch
ID, one or two ordered projects with source texts, library-source order, demand order, and
`force_all` flag. Responses use schema 1 and must include:

- exact normalized per-source diagnostic/incomplete records;
- one stable symbolic-index digest, symbolic group identities, one shared index owner, and zero
  shared semantic/AST/replay-owner rows;
- distinct project binder/store/global-object/universe and local group identities;
- zero collision-preflight, sparse-epoch, and complete-source-fallback invocations;
- exact `Ready` library and user-only group inventory, slots, symbolic/local identity, and
  library/user fragment sites;
- a complete publication transition trace, zero observations while resolving, selected
  global-object keys, demanded/published counts, semantic rows, and local cache entries;
- one exact final fragment-census digest per group, already present on its reservation event;
- exact exit status and normalized rendered output from the authenticated project-route oracle.

The gate runs ordinary and reversed user-file, library-source, and demand orders in lazy and
`force_all` modes. It also runs the paired isolation projects in both project-creation orders
through one index per batch. It
rejects a stale library-only `Array` terminal, missing native/user fragment, differing symbolic
identity, reused local universe identity, leaked opposite-project member, non-final terminal, or
output drift from the authenticated complete-source oracle.

The candidate mode deliberately fails until a runner is supplied. It is the RED implementation
contract; the oracle mode is the green standalone WU0 gate.

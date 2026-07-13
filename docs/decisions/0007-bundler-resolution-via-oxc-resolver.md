---
id: 0007
title: Use Bundler resolution through oxc_resolver for the 1.0 module profile
status: accepted
date: 2026-07-13
---

# 0007 — Use Bundler resolution through `oxc_resolver` for the 1.0 module profile

## Context

M29 proved cross-file semantics with a deliberately small resolver: local relative `.ts` files
supplied to one serial project check. Reaching real projects requires extension substitution,
package and declaration lookup, package `exports`/`imports`, `node_modules`/`@types`, path aliases,
and tsconfig discovery. Reimplementing those filesystem and package rules inside typokat would add
a large compatibility subsystem unrelated to its differentiating work: binding and checking the
TypeScript type model.

TypeScript requires `moduleResolution` to match the program's runtime or bundling host. Its
[`bundler` reference](https://www.typescriptlang.org/docs/handbook/modules/reference#bundler)
describes the modern host profile used by bundlers and runtimes such as Bun or tsx; Node.js
applications that run emitted files directly still need `nodenext`. Because typokat is a checker,
not an emitter or runtime, it needs one explicit 1.0 compatibility profile rather than pretending
that one filesystem algorithm represents every host.

The Rust [`oxc_resolver` crate](https://docs.rs/oxc_resolver/latest/oxc_resolver/) already owns this
physical-resolution domain. Its
[`resolve_dts`](https://docs.rs/oxc_resolver/latest/oxc_resolver/struct.ResolverGeneric.html#method.resolve_dts)
API explicitly targets parity with `ts.resolveModuleName` under `moduleResolution: "bundler"`,
including TypeScript declaration resolution and `"types"` package conditions. It also exposes
tsconfig discovery and parsing. There are still acceptance-relevant limits: `resolve_dts` consumes
the tsconfig supplied through its resolver options for `paths`, and its current `typesVersions`
selection is simpler than TypeScript's compiler-version range selection. Depending on an external
resolver therefore reduces implementation scope but does not remove the need for differential
evidence.

## Decision

`moduleResolution: "bundler"` is typokat's sole supported module-resolution profile for 1.0. We
will use `oxc_resolver` as the authority for physical module and declaration resolution and for the
tsconfig resolution facilities it exposes. Typokat will not grow a parallel implementation of
filesystem probing, extension substitution, package lookup, package conditions, path aliases, or
tsconfig inheritance/reference resolution. Implementation work will pin and audit a concrete crate
version.

The dependency boundary is explicit:

- `oxc_resolver` owns mapping an importing file and specifier to a physical source/declaration file,
  plus the tsconfig/package metadata used for that lookup;
- typokat owns project source-root enumeration where the crate does not expose the complete root
  set, deterministic module-graph construction, file loading, and dependency ordering;
- typokat owns TypeScript module semantics: value/type/namespace binding, default/namespace/star
  imports, export lists and re-exports, cycles, and diagnostic ownership;
- typokat owns parsing, binding, and checking resolved `.ts`/`.d.ts` files in one type universe,
  including ambient declarations and all type identity; and
- typokat owns deterministic project accounting and must never turn an unresolved or unsupported
  resolution branch into a clean result.

Resolver acceptance is differential against the repository's pinned `tsc` oracle. Fixtures must
cover every supported Bundler branch and package/config shape used by the public project witness.
Known dependency gaps, including `typesVersions` selection, must either be fixed upstream and
adopted through a pinned upgrade or be reported as explicit unsupported project outcomes. Typokat
must not silently approximate them or maintain a local resolver fork disguised as fallback logic.

`nodenext`, `node16`, classic Node, CommonJS-specific, and other alternate resolution profiles are
outside the required 1.0 scope. A project that requests one receives an explicit unsupported-profile
outcome; typokat does not silently reinterpret it as Bundler. Supporting another profile later
requires its own compatibility contract and differential corpus.

## Consequences

- The resolver backlog changes from implementing Node/package/tsconfig algorithms to integrating,
  configuring, and differentially validating `oxc_resolver`, while typokat concentrates on module
  graph and import/export type semantics.
- Bundler-oriented projects get a clear modern target without requiring NodeNext parity before 1.0.
  Node applications whose runtime contract is NodeNext remain unsupported even when an individual
  import would happen to resolve to the same file.
- Resolver behavior can change when the dependency changes. The crate version, option mapping, and
  differential fixtures therefore form one compatibility boundary and are reviewed together.
- Project discovery is not fully delegated: typokat must still enumerate configured source roots
  and account for every selected/skipped file when the crate only provides resolved config data.
- A crate limitation may narrow the accepted Bundler surface until it is fixed. That produces an
  explicit unsupported result, never an error type or false-clean check.
- This decision does not reduce the `.d.ts`, ambient-library, module-semantic, cycle, determinism,
  or cross-file type-identity work owned by typokat.

## Alternatives considered

### Implement NodeNext resolution in typokat

Rejected for 1.0. It duplicates a large, changing host-compatibility subsystem and delays type-model
work. NodeNext remains a valid future profile for direct Node.js hosts, but it needs a separate
contract rather than constraining the first supported project mode.

### Implement Bundler resolution in typokat

Rejected. Choosing a smaller profile does not justify maintaining our own filesystem/package/
tsconfig resolver when a Rust implementation with an explicit TypeScript Bundler API exists.

### Treat `oxc_resolver` output as best-effort and patch mismatches locally

Rejected. Layered fallback probes would split authority, make dependency upgrades unpredictable,
and risk silently resolving a package differently from `tsc`. Differentially exposed gaps are
either upstream work or explicit unsupported cases.

### Claim multiple resolution profiles through one approximation

Rejected. TypeScript resolution follows the host; matching common paths is not evidence of parity
for package conditions, extension rules, or declaration lookup. One honest profile is preferable
to several ambiguous ones.

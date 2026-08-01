---
id: 0012
title: Ship the canonical default-library semantic snapshot
status: superseded by 0017
date: 2026-07-22
---

# 0012 — Ship the canonical default-library semantic snapshot

## Context

[ADR-0011](0011-freeze-pinned-default-library-base.md) accepts one exact TypeScript 6.0.3
82-file profile, one source-backed `LibraryCompiler`, an immutable AST-free base, private deltas,
and a complete same-universe source rebuild for collisions. It assumed first initialization would
perform source semantic compilation and deferred serialization until cold-start evidence existed.

WU0B now supplies that evidence. Two isolated byte-reproducible release builds, two 17-pass/
4-ignored preflights, two byte-identical calibrated regenerations, and 45 fresh timing processes
produced an overall external p95 of 110.409 ms (window p95s 110.913 / 106.753 / 110.409 ms) with
57,836 KiB maximum RSS. The active
[archived sprint run log](../archive/sprint-2026-07-21-full-lib-performance-cutover.md) records the complete
optimization and failed-run ledger. This proves feasibility only: production still uses
`src/prelude.ts`, and the final base/delta, collision, package, CLI, and cross-tool 2× gates remain.

## Decision

### Narrowly supersede ADR-0011's initialization choice

Typokat will ship and eagerly decode the exact canonical semantic snapshot for its sole default
library. This supersedes only ADR-0011's runtime-source initialization and deferred-snapshot
clauses. All profile, compiler-authority, base/delta, collision, event, lifecycle, and failure
guarantees in ADR-0011 remain binding.

The snapshot transports the same `FrozenLibraryBase`; it is not a second type model. Exactly one
production `LibraryCompiler` consumes the embedded sources and remains authoritative for explicit
generation and private collision compilation. The decoder reconstructs typed rows; it never
synthesizes semantics, resolves declarations by name, or loads a reachable subset.

Generation is an explicit developer/CI command outside ordinary `cargo build` and timing. It uses
no `build.rs`, network, host TypeScript, working-directory discovery, or runtime regeneration. Two
clean isolated roots with the pinned toolchain, lockfile, sources, compiler, and canonical remaps
must produce byte-identical archives and equal source/decoded projections and clean/error
calibration. Semantic compiler or codec changes bump the schema epoch and regenerate atomically.

Packages retain the snapshot, all 82 exact sources and registry, TypeScript license, and third-party
notice. Sources remain required for private collision compilation. Package-list and extraction
tests pin every asset.

### Freeze the v1 archive

The internal platform-neutral big-endian header contains magic `typokat-semantic-snapshot`, version
1, profile and schema SHA-256 values, section count 10, body length/digest, then ten 52-byte rows
`(u16 tag, u16 zero, u64 absolute offset, u64 length, sha256 payload)`. Tags are exactly 1–10;
sections are nonempty and contiguous with no trailing bytes.

- profile: `ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d`;
- schema epoch: `a78ea0521c7c375669bfdb08f0929a5e4b1d0b0d6928de60fbfe09b222a8bc65`;
- artifact: 10,003,957 bytes,
  `af97017b22c9f8ff3726de9dbd49a3039cf70f2dd5a4fd9df9f71328be721dd0`;
- body: 10,003,300 bytes,
  `b8122f5e2c0d68d4b25920f2f4f31eaf88fe2b873e9df2845d47bdf94227bf00`.

| Tag | Section | Bytes | SHA-256 |
|---:|---|---:|---|
| 1 | store | 4,458,504 | `03abff50ce4cfc5d4753c8825474bc06ae362b2ebbfc45a9be7b40918c5f24a7` |
| 2 | interner | 523,847 | `305cc69166b266a593d2f0bad010eeccf5d7fbde0f19c02e3075d90b0758abbd` |
| 3 | binder | 1,180,632 | `910c59e3dd067e5cfdb8a9836abe1a93032af2c9692003769cc2c526599e29f3` |
| 4 | decl-types | 6,490 | `46394e3cefb4a04282c4b0852b8bd28741d999d07f4f1401ce41c75e37a971f6` |
| 5 | published-types | 189,925 | `091d0dc6d058309f64a2b0dfc66c6e9e4aeab65311dd36d61fb762c4c6fa3578` |
| 6 | namespace-terminals | 43 | `28598d80b34dafaa695d7603d23149ecd576b2a716063fbc8f287c515fceff9c` |
| 7 | class-metadata | 256 | `c81b208d7ae6b6af473fb869ac4315b724939ccc009b4a03d26bfbb5338adb9d` |
| 8 | semantic-identities | 3,558,489 | `a6b1810a831a3956254f1b2b97cc262d246313539b0e6d41969fa6799039e258` |
| 9 | root-name-index | 85,034 | `8aa936d8589231925e1fff5ae85485b7c2c7a05848ba3f1e5454d221f4f99b16` |
| 10 | next-ids | 80 | `60b19caafb728f764365c81e401889735d8af0343ecec3f74ad6fc24255d0208` |

Section 8 begins with nine families and 296,414 sorted typed references, followed by eight semantic
identity terminals and exactly 31 projection witnesses. Witnesses are audit metadata only. The
schema digest is a semantic epoch: changed meaning, identity assignment, order, terminal condition,
reference ownership, or interpretation requires a new epoch and artifact identity.

### Authenticate, typed-decode, then publish

The normal route owns embedded bytes and first checks the compile-time-pinned total length and
whole-file SHA. A self-consistent rehashed archive is rejected. The canonical path may omit
redundant body/section rehashing, but still validates header/directory structure, versions, ranges,
counts, IDs, order, root-index/binder agreement, every reference by streaming comparison, terminal
state, and next-ID prefixes. Generic adversarial tooling retains independent inner digest checks.

Only complete success publishes `Arc<FrozenLibraryBase>` through ADR-0011's cached
`OnceLock<Result<…>>`. Production must replace every prototype `assert!`, `panic!`, `expect`,
overflow, and thread-join failure with typed `LibraryInitError`; the CLI exits 2 without partial
output. There is no automatic source fallback or retry. Rollback is an explicit release/code
change and withdraws the performance claim.

ADR-0011's identity rules remain unchanged: immutable base prefixes, base-first private interning,
no base mutation or base→delta references, conservative collision preflight before mutation/cache/
events, pointer-identical success and cached failure. A collision binds the packaged library sources
and entire user project together in one isolated mutable universe before either is lowered or
observed. The WU0B continuation is only a certified non-collision witness, not this private route.

V1 chooses complete eager decode. Semantic lazy loading remains rejected; a later physical immutable
index would require separate evidence over the same authenticated complete identities. Initialization
may use WU0B's two bounded scoped joins for independent interner/binder decode and immutable reference
enumeration, with fixed interner-before-binder failure order and typed join failure. This is not
parallel semantic construction or user checking; mutable cross-file identity remains backlog 16.

## Consequences

- Ordinary startup can remove all default-library source work while preserving one source authority.
- Artifact/schema/package updates become one reviewed reproducible transaction.
- Exact admission is fast and fail-closed, but every legitimate semantic/wire change needs new
  identities and WU0B-equivalent evidence.
- WU2–WU7 must still promote typed production compiler/decoder/provider, delta, collision route,
  assets, and CLI atomically; the final 2× claim remains unproven.

## Alternatives considered

- **Compile sources on every startup:** rejected by the ~10.8 s complete-source result.
- **Resolve only reachable declarations:** rejected because it changes complete ambient semantics,
  grows shared state during checks, and games small workloads.
- **Accept any self-consistent v1 archive:** rejected; releases admit one reviewed whole-file identity.
- **Regenerate during build or fall back at runtime:** rejected as host-dependent and fail-open.
- **Use the shared snapshot for collisions:** rejected; collisions require one private combined
  library+project compilation before publication.

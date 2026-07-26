---
id: 97
title: Delete the wire serialization layer the snapshot left behind
blocked-by: []
---

# 97 — Delete the wire serialization layer the snapshot left behind

**Summary.** Retiring the shipped library snapshot ([ADR-0017](../decisions/0017-compile-the-default-library-from-source.md))
orphaned roughly **6,300 lines** of byte-level encode/decode. Nothing produces the format any more,
so the code is reachable only from its own round-trip tests. It was gated `#[cfg(test)]` rather than
deleted, because part of it still backs real reference-integrity assertions and separating the two
was out of that work unit's scope. Effort M. Mostly deletion.

## Problem

| file | lines | state |
|---|---|---|
| `src/binder/snapshot.rs` | 3,978 | `#[cfg(test)] pub(crate) mod snapshot` |
| `src/types/intern/snapshot.rs` | 2,108 | reachable, but verify what production still needs |
| `src/snapshot_codec.rs` | 251 | `SnapshotCodecError` and the reader/writer primitives |

These encode binder scopes, symbols, interner rows and store rows into a versioned byte format.
The only consumer of that format was the canonical archive, which no longer exists. What remains is
a large round-trip test corpus proving that a format nothing writes can be read back.

Deleting it is not purely mechanical, for two reasons:

- **Some of it backs live assertions.** `snapshot_reference_records_for_test` and
  `local_reference_records_for_test` are how `library_compiler.rs` checks reference integrity — the
  serializer is being used as a *traversal*, not as a format. Those assertions are worth keeping;
  they need a traversal that does not build bytes.
- **`into_snapshot_parts` / `from_snapshot_parts` are production and must not be swept up.** Seven
  types implement them and `freeze_library_runtime_product` calls
  `into_snapshot_parts` on the real path (`library_compiler.rs:135`). Despite the name they are the
  in-memory decomposition between compilation and freezing, not serialization. They should be
  **renamed**, not removed — the name is a leftover that will mislead the next reader into deleting
  live code.

## Approach / acceptance

1. Establish which assertions genuinely depend on the encoders. Replace those with a direct
   traversal that yields the same records without producing bytes.
2. Delete the three files and every test whose only subject is round-tripping the format.
3. Rename `*_snapshot_parts` to something that describes what it is (a frozen-product decomposition).
4. Re-check `Cargo.toml`: `sha2`'s comment still says "the shipped library snapshot boundary" and the
   dependency is now justified only by the profile identity and the replay manifest.

Acceptance: `cargo test` unchanged except for the deleted round-trip tests; reference-integrity
assertions in `library_compiler.rs` still fail when a reference leaks; clippy and fmt clean; no
production symbol keeps a `snapshot` name that does not serialize anything.

## Touch points

`src/binder/snapshot.rs`, `src/types/intern/snapshot.rs`, `src/snapshot_codec.rs`,
`src/check/checker/library_compiler.rs`, `src/binder/mod.rs`, `src/types/intern/mod.rs`,
`src/lib.rs`, `Cargo.toml`.

<!-- Origin: fallout of the snapshot removal, 2026-07-26. The implementing agent gated the orphans
     rather than deleting them and flagged the split as needing its own review. -->

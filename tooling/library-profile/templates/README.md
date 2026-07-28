# TypeScript 6.0.3 default-library profile

This directory is the single canonical copy of typokat's pinned TypeScript standard-library
profile. It contains the exact 82-file npm declaration closure rooted at
`lib.es2025.full.d.ts`, TypeScript's license and third-party notices, and a deterministic
`profile.toml` with per-file provenance and reference edges.

The artifacts come from the `typescript` npm package version 6.0.3 and are byte-matched to the
tracked `lib/` build artifacts at TypeScript commit
`050880ce59e30b356b686bd3144efe24f875ebc8`. The distinct upstream `src/lib/` inputs are not the
published package artifacts and are not treated as equivalent provenance.

Regenerate or verify this directory only through
[`tooling/library-profile/profile.py`](../../../tooling/library-profile/profile.py), using explicit
local npm-package and bare-Git inputs. No production reader is wired yet; `crates/typokat-check/src/prelude.ts` remains
the production library source until the planned cutover.

The copied Apache license and third-party notices apply to the vendored TypeScript artifacts. This
directory does not select typokat's own crate license.

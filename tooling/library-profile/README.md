# Pinned TypeScript library profile tool

`profile.py` generates and verifies typokat's sole 1.0 standard-library profile. Inputs are always
explicit: one local `typescript@6.0.3` npm package directory and one local bare Git repository that
contains commit `050880ce59e30b356b686bd3144efe24f875ebc8`. The tool never discovers an installed
`tsc`, fetches the network, or accepts a different version or revision.

The npm `lib/*.d.ts` files are authoritative package artifacts. They are compared byte-for-byte
with the pinned repository's tracked `lib/*.d.ts` build artifacts. The distinct TypeScript
`src/lib/*.d.ts` compiler inputs are not equivalent package artifacts and are deliberately not
recorded as interchangeable provenance.

Generate the checked-in subtree:

```sh
python3 tooling/library-profile/profile.py \
  --typescript-package /absolute/path/to/node_modules/typescript \
  --typescript-git-dir /absolute/path/to/typescript.git \
  --output crates/typokat-library/src/typescript-6.0.3
```

Verify it by regenerating into a temporary directory and byte-comparing every path:

```sh
python3 tooling/library-profile/profile.py \
  --typescript-package /absolute/path/to/node_modules/typescript \
  --typescript-git-dir /absolute/path/to/typescript.git \
  --output crates/typokat-library/src/typescript-6.0.3 \
  --check
```

The closure is reconstructed from raw `/// <reference lib>` edges rooted at
`lib.es2025.full.d.ts`. Ordering independently follows the explicit package's `libEntries` table and
is cross-checked against that package's own `tsc --strict --target es2025 --listFilesOnly`. The
output contains raw upstream declarations and notices plus a deterministic `profile.toml`; no host
tool or checkout is needed to inspect the committed result. This provenance tool does not wire a
production reader; `crates/typokat-check/src/prelude.ts` remains active until the planned cutover.

Run the standard-library-only tool tests with:

```sh
python3 -m unittest tooling/library-profile/test_profile.py -v
```

The copied Apache license and third-party notices license the vendored TypeScript artifacts. They
do not select or describe typokat's own crate license.

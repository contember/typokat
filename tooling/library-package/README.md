# Default-library package verification

`verify.py` is the release gate for the source compiler and canonical
snapshot. It refuses a dirty repository, clones the committed revision into two
distinct clean roots, generates one snapshot in each root, and requires the two
outputs to equal the pinned 10,003,957-byte artifact byte-for-byte.

Each clean root is then packaged independently. The coordinator validates
the source and normalized package manifests plus Cargo metadata, requires zero
custom-build targets, and compares the exact regular-file archive inventory with
`cargo package --list`. Duplicate archive paths, links, special files, and path
escapes are rejected. The gate validates the complete pinned profile manifest,
all 82 declaration sources and reference edges, the canonical snapshot, and all
upstream notices before byte-comparing the extracted assets with their inputs.
It then runs `cargo check --locked --offline --all-targets` against each crate.

Every Cargo or generation subprocess is enclosed by a no-follow inventory of the
whole source tree, including tracked, untracked, ignored, directory, file-mode,
size, and byte identities. Each clean root must also have empty porcelain output
including ignored paths. Cargo homes, targets, generated probes, package archives,
and extraction roots are the only exclusions, and all live outside the source
roots. A custom or conventional build script, extra profile asset, source
mutation, implicit regeneration, or missing check fails the run.

Run the cheap adversarial contract first:

```sh
python3 -m unittest tooling/library-package/test_verify.py -v
```

Run the complete release gate only from a clean committed tree:

```sh
python3 tooling/library-package/verify.py
```

CI invokes the same boundary through the ignored Rust integration test:

```sh
cargo test --test library_package_assets \
  clean_generation_and_cargo_package_are_reproducible_and_complete \
  -- --ignored --exact --nocapture
```

Generation is an explicit integration boundary. The coordinator invokes the
ignored release-libtest
`library::artifact::generate_packaged_snapshot_for_tooling` and supplies a new
absolute output path in `TYPOKAT_LIBRARY_SNAPSHOT_OUTPUT`. That probe must write
exactly that archive, must not rewrite the checked-in snapshot, and must run no
user check or packaging step. Ordinary `cargo build`, `cargo package`, and
`cargo check` receive no generation variable and must only consume packaged
bytes.

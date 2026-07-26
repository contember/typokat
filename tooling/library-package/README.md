# Default-library package verification

`verify.py` is the release gate for the default-library sources. It refuses a
dirty repository and clones the committed revision into two distinct clean
roots.

Each clean root is then packaged independently. The coordinator validates
the source and normalized package manifests plus Cargo metadata, requires zero
custom-build targets, and compares the exact regular-file archive inventory with
`cargo package --list`. Duplicate archive paths, links, special files, and path
escapes are rejected. The gate validates the complete pinned profile manifest,
all 82 declaration sources and reference edges, and all upstream notices before
byte-comparing the extracted assets with their inputs. It then runs
`cargo check --locked --offline --all-targets` against each crate.

Every Cargo subprocess is enclosed by a no-follow inventory of the whole source
tree, including tracked, untracked, ignored, directory, file-mode, size, and byte
identities. Each clean root must also have empty porcelain output including
ignored paths. Cargo homes, targets, package archives, and extraction roots are
the only exclusions, and all live outside the source roots. A custom or
conventional build script, extra profile asset, source mutation, or missing check
fails the run.

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
  cargo_package_ships_every_library_source_and_checks_clean \
  -- --ignored --exact --nocapture
```

# Workspace crate split

Proposal for turning the single `typokat` package into a Cargo workspace. All
numbers below are measured on the tree at `fb35185` (2026-07-27), not estimated.

**Verdict up front.** The split is worth doing, and it is *cheaper than it looks* —
the production dependency graph is already acyclic and the cross-layer API surface
is ~280 paths, not the ~1300 `pub(crate)` items a naive count suggests. But it does
**not** fix the build-time complaint, because 60 % of the code is one module
(`check`) and that is the module being edited. Do the split for **enforced
layering**, and treat build time as a separate problem with a different fix
(see §6).

## 1. Measured starting point

| Module | LOC | Files | Notes |
|---|---:|---:|---|
| `src/check/` | 93 343 | 102 | **60 % of the tree**; `checker/` alone is 80 601 |
| `src/binder/` | 22 669 | 10 | |
| `src/types/` | 15 317 | 20 | |
| `src/relate/` | 8 725 | 9 | |
| `src/library/` | 6 792 | 13 | + 3.1 MB of `typescript-6.0.3/lib` assets |
| `src/diagnostics/` | 3 657 | 6 | |
| `src/driver.rs` | 2 452 | 1 | |
| `src/class_semantics.rs` | 557 | 1 | depends on `types` only |
| `src/surface.rs` | 413 | 1 | oxc-only; compile-time drift tripwire |
| `src/{snapshot_codec,main,span,source,lib}.rs` | 796 | 5 | |

~29 600 of those lines live in `*_spec.rs` / `*test*.rs` files, plus inline
`#[cfg(test)]` blocks in 69 more files (`library_compiler.rs` alone has 180).

Incremental cost of touching `src/check/checker/expr.rs`:

```
cargo build --lib      9.1 s
cargo test --no-run   28.0 s
```

The ~19 s delta is one monolithic libtest binary.

## 2. The measured dependency graph

Production edges only (test-only edges listed separately in §4). Direction is
"depends on":

```
span · source · snapshot_codec        (no deps)
        ↓
      types  ← class_semantics
      ↙   ↘
 binder     relate
      ↘   ↙   ↓
    diagnostics
        ↓
      check  ⇄ library        ← tangle 1
              ⇄ driver        ← tangle 2
```

Notably `relate` does **not** depend on `binder` (only `types` + `class_semantics`),
and `diagnostics` sits above both. That is a cleaner shape than the architecture doc
implies, and it is what makes the split mechanical.

## 3. Proposed layout

Keep the **root package as a thin facade** rather than using a virtual manifest:

```
Cargo.toml              # [workspace] members = ["crates/*"]  AND  [package] name = "typokat"
src/lib.rs              # re-exports the pillars; keeps `test_repository_root()`
src/main.rs             # the bin (jemalloc lives here)
tests/                  # conformance corpus — unchanged
tooling/                # unchanged
crates/
  typokat-core/         span, source, snapshot_codec              ~ 0.5k
  typokat-types/        types/ + class_semantics                  ~15.9k
  typokat-binder/       binder/                                    22.7k
  typokat-relate/       relate/                                     8.7k
  typokat-diagnostics/  diagnostics/                                3.7k
  typokat-check/        check/                                     93.3k
  typokat-library/      library/ + the lib.d.ts snapshot assets     6.8k + 3.1 MB
  typokat-driver/       driver.rs                                   2.5k
  typokat-surface/      surface.rs                                  0.4k
```

**Why the facade and not a virtual manifest.** `src/lib.rs::test_repository_root()`
asserts the root manifest starts with `[package]\nname = "typokat"\n`; a virtual
manifest starts with `[workspace]` and breaks it. `tooling/official-suite/` shells
out to a prebuilt binary and `tests/cases/` is root-relative. Keeping the root
package makes `tests/` and `tooling/` a **zero-diff** part of the migration.

`class_semantics` folds into `typokat-types` (it depends on nothing else and is
consumed by `check` + `relate`). Alternative: its own 557-line crate — not worth it.

`surface.rs` gets its own leaf crate so an oxc bump recompiles the tripwire first
and alone. Alternative: fold into `typokat-driver` — acceptable, loses that property.

Workspace-level `[workspace.dependencies]` pins oxc once; `[workspace.lints]`
keeps `clippy --all-targets -- -D warnings` uniform across members.

## 4. The real cost, measured

Cross-boundary API surface — distinct paths referenced from **outside** each module
(braced `use crate::x::{A, B}` imports expanded, so this is the true number):

| Crate | distinct external paths | distinct top-level entry points |
|---|---:|---:|
| `types` | 74 | 15 (`TypeId`, `Interner`, `ClassId`, `WellKnown`, `store`, `substitute`, …) |
| `binder` | 86 | 9 (`Binder`, `bind`, `declaration`, `namespace`, `scope`, `symbol`, …) |
| `check` | 59 | 8 |
| `diagnostics` | 21 | 9 |
| `relate` | 19 | 9 (`Relater`, `Reason`, `ReasonChain`, `RelationOutcome`, …) |
| `class_semantics` | 19 | 10 |
| `source` | 8 | 7 |
| `library` | 4 | 4 |
| `span` | 4 | 2 |

**~294 `pub(crate)` → `pub` promotions total.** The 400+ `pub(super)` items are
intra-crate and unaffected. This is the honest cost line: a few hundred visibility
edits, mostly mechanical, and it does mean the layer internals become nominally
public — mitigate by re-exporting a curated surface from each crate's `lib.rs` and
keeping the rest behind private modules.

### Tangles that must be cut first

**1. `check` ⇄ `library`.** `library → check` is 33 refs (18 into
`check::checker::library_compiler`, 6 into `replay_index`, 3 into `events`).
`check → library` is only **2**: the `CollisionFreeUserDeltaCapability` token in a
signature at `library_compiler.rs:607`, and `library::profile::ExactLibraryProfile`
in a test at `library_compiler.rs:4883`. *Fix:* sink the capability token down into
`typokat-check`; the test ref is test-only. One type move.

**2. `library` ⇄ `driver`.** `library → driver` is `driver::FileInput` in
`provider.rs:155/208` and `base.rs:934` (plus one spec). `driver → library` is
`FrozenLibraryBase` + `LibraryBaseProvider`. *Fix:* `FileInput` is a plain input
record — sink it into `typokat-core`. One struct move.

**3. Three test-only wrong-direction edges** — end-to-end specs living in the wrong
layer:

- `relate/relation/failing_relation_scaling_spec.rs` → `check::checker::check_program`, `diagnostics::{Diagnostic, DiagnosticCode}`
- `types/intern/tests.rs` → `diagnostics::render_type`
- `check/**` → `crate::driver::check_source` (19 sites, all in `*_spec.rs` / `tests/`)

*Fix:* relocate them to the crate that owns the pipeline they drive (`typokat-check`
or the root facade's `tests/`). File moves.

### Migration hazard: source-introspecting specs

Several specs read sibling source with `include_str!`, some already crossing module
boundaries:

```
check/checker/decls/eager_application_cache_spec.rs:1060  include_str!("../../../types/mod.rs")
binder/exact_declaration_site_spec.rs:323                 include_str!("../check/checker/decls/mod.rs")
```

After the split these become `../../../typokat-types/src/mod.rs`-shaped paths across
crate roots — brittle. Either move each spec into the crate it introspects, or route
them through a shared helper that resolves paths from `test_repository_root()`
(which the facade preserves).

## 5. Staging

Each stage is independently green and independently committable.

- **Stage 0 — cut the tangles, still one crate.** Move `FileInput` and the capability
  token; relocate the three misplaced specs. Add a layering tripwire spec in the
  existing `include_str!`-introspection idiom that fails on a wrong-direction
  `crate::` import. *This stage delivers most of the architectural value on its own*
  and is a prerequisite for everything after.
- **Stage 1 — bottom half.** `typokat-core`, `-types`, `-binder`, `-relate`,
  `-diagnostics`, `-surface`. One crate per commit, bottom-up.
- **Stage 2 — top half.** `typokat-check`, `-library`, `-driver`; root becomes the
  facade. Fix the `include_str!` paths here.
- **Stage 3 — optional, profiling-gated.** Split `check` itself (§6).

## 6. What this does *not* buy — read before doing it for build times

Editing `check` is the common case and `check` is 93k lines; after the split that
crate still recompiles as one unit. **The 9.1 s / 28.0 s loop above is essentially
unchanged by Stages 1–2.** What does improve: per-crate test binaries, so an edit in
`check` stops rebuilding `binder`/`types`/`relate` test code, and `cargo test -p
typokat-relate` becomes real (today `cargo test <name>` still links the whole
libtest).

If build time is the actual goal, the levers, in value order:

1. **Move `library_compiler.rs` out of `check/checker/` into `typokat-library`.**
   7 971 lines with **180 `#[cfg(test)]` blocks** — the single biggest test-compile
   blob, and it is misfiled: it is the library subsystem's compiler sitting in the
   checker. This alone shrinks the hot crate by 8k lines. Highest value-per-risk
   move in this whole document.
2. **Split `check` internally.** Measured internal edges: `checker/` 80.6k,
   `query/` 8.3k, `infer/` 3.4k, `flow.rs` 1.0k. But `query → checker` (9),
   `infer → checker` (2), and `flow → query` (1) are back-edges, so a clean cut
   needs those resolved first. The `Checker` state in `checker/mod.rs` (3 910
   lines) + `context.rs` (1 875) is the god-object — this is a real refactor, not
   a mechanical move, and belongs behind the same profiling gate as ADR-0001.

## 7. Open questions

- Facade root package vs. virtual manifest — the facade is recommended (§3), but it
  means the root keeps a `src/` with a `lib.rs`/`main.rs` that are pure re-exports.
- Does `typokat-library` own the 3.1 MB `typescript-6.0.3/` asset tree, or does that
  become a separate data crate so a lib-snapshot bump doesn't touch checker code?
- Publish to crates.io eventually? If yes, the `pub` promotions in §4 become a real
  semver surface and the curated-re-export discipline stops being optional.

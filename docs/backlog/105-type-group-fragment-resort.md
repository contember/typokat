---
id: 105
title: Type-group fragments are re-sorted on every append, and production sorts them a second time
blocked-by: []
---

# 105 — Type-group fragments are re-sorted on every append, and production sorts them a second time

**Summary.** `declare_type` re-sorts a type group's entire `fragments` vector on every append — the
exact twin of the bug backlog `88` just fixed for declaration lists, and now the dominant remaining
cost of a merged interface. It also carries something `88` did not: a `#[cfg(not(test))]` second sort
that unconditionally overwrites the library-ordinal ordering the branch above it just computed, so
**test and production builds order library type-group fragments by different keys**. Effort M — the
perf half is mechanical, the ordering half needs a decision first.

## Problem

### The perf twin

`src/binder/bind.rs:2433-2467` runs a full `fragments.sort_by_key(...)` after every
`append_fragment`, so a group merged `k` times costs `O(k² log k)`. With backlog `88` fixed, this is
what is left: `iface_merge_4000` binds in **31 ms**, and ablating this sort as well takes the same
fixture to **~8 ms**. The fix is the same shape — the list is already almost sorted, so a binary
insert reproduces a stable sort of the appended list exactly.

`src/binder/namespace.rs:2393` and `:2405` are the same pattern for namespace fragments and belong
in the same change.

### The ordering divergence

The interesting half. `declare_type` picks a sort key by build:

```rust
if !state.library_module_ordinals.is_empty() {
    fragments.sort_by_key(|f| (library_ordinal(f), f.site.declaration_span.start, f.declaration.0));
} else {
    fragments.sort_by_key(|f| (f.source, f.site.declaration_span.start, f.declaration.0));
}
#[cfg(not(test))]
fragments.sort_by_key(|f| (f.source, f.site.declaration_span.start, f.declaration.0));
```

The third sort is **unconditional and outside the branch**. In a `cfg(test)` build the
library-ordinal ordering survives; in the shipped binary it is immediately overwritten by the source
key. Fragment order decides which declaration wins for overlapping members, and therefore diagnostic
text — so on the library path, `cargo test --lib` and the release binary are validating different
orderings.

**What was measured (2026-07-27), and what it does not settle.** Deleting the `cfg(not(test))` sort
in an isolated worktree at `8388308` changes **nothing we can see**: the 875-record library census is
identical, and `conformance`, `divergences`, `surface`, `manifest`, `incomplete_outcome` and
`lib_es2025_full_profile` — all integration targets, all built without `cfg(test)`, so the deletion
is live in them — stay green.

That is evidence, not exoneration. It says no gate this project has can observe the second sort's
effect, which leaves two readings open: either the two keys agree for the pinned 82-file profile and
the sort is dead code, or they disagree and nothing tests the difference. **Both readings are bad in
the same way** — this is the fourth time this sprint a behaviour turned out to be untested by
construction (`92`'s duplicate diagnostics, `94`'s multi-file constant, and the whole reason
`tooling/differential/` exists). Do not resolve it by deleting the line because the tests pass.

## Approach / acceptance

Two separable pieces; do the ordering one first, because the perf fix has to preserve whichever
order is correct.

*Ordering.* Determine whether the library-ordinal and source-unit keys actually order the pinned
profile's fragments differently — dump both orderings for every merged library type group and diff
them. If they agree, delete the redundant sort and pin the agreement with a test, so it cannot
silently stop agreeing. If they disagree, decide which is correct (the ordinal is the reference-closure
order the profile is defined by, so it is the likely answer), make both builds use it, and pin the
resulting order on a library-merged interface — `Array`, `String`, `Window` are the large groups.
Either way `cfg(test)` must stop changing binder output.

*Perf.* Binary-search insert, same as `88`. Acceptance mirrors it: a counter guard showing append
work grows with fragment count rather than fragments × group size at two group sizes, and
**fragment order byte-identical** — the 72,056-line merge-order dump `88` used covers type-group
fragment order already and is the right witness.

## Touch points

`src/binder/bind.rs` (`declare_type`), `src/binder/namespace.rs` (the two namespace fragment sorts).

<!-- Origin: found by the backlog 88 work unit, 2026-07-27, as the adjacent twin it deliberately
     left alone; the cfg divergence was confirmed by reading and the ablation measured by the leader. -->

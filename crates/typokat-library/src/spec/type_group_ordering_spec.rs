//! Disabled RED contract for backlog 105's pinned-profile ordering proof.
//!
//! The packaged profile happens to assign source keys in the same order as registry ordinals.
//! That agreement must be constructed and pinned directly; unchanged diagnostics cannot prove it.

use super::compiler::LibraryCompiler;
use super::profile::ExactLibraryProfile;
use std::collections::BTreeSet;

#[test]
fn every_merged_library_group_has_one_exact_order_under_both_keys() {
    let profile = ExactLibraryProfile::load_packaged().expect("exact packaged profile");
    let checkpoint = LibraryCompiler::new()
        .compile_binder_checkpoint(&profile)
        .expect("source-backed binder checkpoint");
    let ordering = checkpoint.type_group_fragment_ordering_for_test();

    assert_eq!(ordering.units.len(), 82);
    for (position, unit) in ordering.units.iter().enumerate() {
        assert_eq!(unit.library_ordinal.index(), position);
        assert_eq!(
            unit.source_key.0,
            u32::try_from(position + 1).expect("82 source keys fit u32")
        );
    }

    let merged = ordering
        .groups
        .iter()
        .filter(|group| group.fragments_by_library_ordinal.len() > 1)
        .collect::<Vec<_>>();
    assert!(!merged.is_empty(), "the full profile reopens type groups");
    for group in &merged {
        assert_eq!(
            group.fragments_by_library_ordinal, group.fragments_by_source_key,
            "{} orders differently under the two production keys",
            group.name
        );
        assert_eq!(
            group.fragments_in_binder, group.fragments_by_library_ordinal,
            "{} is not stored in canonical registry order",
            group.name
        );
    }

    let names = merged
        .iter()
        .map(|group| group.name.as_str())
        .collect::<BTreeSet<_>>();
    for witness in ["Array", "String", "Date"] {
        assert!(
            names.contains(witness),
            "{witness} is a merged profile witness; found {names:?}"
        );
    }
}

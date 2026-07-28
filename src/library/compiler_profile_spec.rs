//! Packaged-profile ownership witnesses for the source-backed checker compiler.

use super::profile::ExactLibraryProfile;
use crate::check::checker::library_compiler::{
    assert_exact_full_profile_owned_base_checks_caller_certified_suffix,
    assert_exact_profile_interner_has_no_pending_reservations,
    assert_exact_profile_replay_index_is_complete_and_deterministic,
    assert_exact_profile_selects_complete_native_bridge_identities, run_library_release_probe,
    InjectedLibrarySource,
};
use std::time::Instant;

fn injected_sources(profile: &ExactLibraryProfile) -> Vec<InjectedLibrarySource<'_>> {
    profile
        .sources()
        .iter()
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.ordinal(),
            name: source.name(),
            source: std::str::from_utf8(source.bytes())
                .expect("packaged library sources are UTF-8"),
        })
        .collect()
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "full-profile reservation closure is release-only"
)]
fn exact_profile_interner_has_no_pending_reservations() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged full-library profile");
    assert_exact_profile_interner_has_no_pending_reservations(&injected_sources(&profile));
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "full-profile replay-index generation is release-only"
)]
fn exact_profile_replay_index_is_complete_and_deterministic() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged full-library profile");
    assert_exact_profile_replay_index_is_complete_and_deterministic(&injected_sources(&profile));
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "full-profile semantic selection is release-only"
)]
fn exact_profile_selects_complete_native_bridge_identities() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged full-library profile");
    assert_exact_profile_selects_complete_native_bridge_identities(&injected_sources(&profile));
}

#[test]
#[ignore = "release-only cold-process library measurement"]
fn library_release_probe_once() {
    let total_started = Instant::now();
    let registry_started = Instant::now();
    let profile =
        ExactLibraryProfile::load_packaged().expect("packaged library registry validation");
    let registry_validation = registry_started.elapsed();
    run_library_release_probe(
        &injected_sources(&profile),
        registry_validation,
        total_started,
    );
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "full-profile owned-base semantic selection is release-only"
)]
fn exact_full_profile_owned_base_checks_caller_certified_suffix() {
    let profile = ExactLibraryProfile::load_packaged().expect("exact pinned full profile");
    assert_exact_full_profile_owned_base_checks_caller_certified_suffix(&injected_sources(
        &profile,
    ));
}

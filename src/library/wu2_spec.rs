//! Disabled RED contract for the production default-library compiler and artifact.
//!
//! Activate from `library/mod.rs` only after the production modules exist. These tests exercise
//! behavior through the library facade; checker drafts, ASTs, passes, and WU0 evidence types must
//! remain inaccessible here.

use super::artifact::{
    generate_canonical_snapshot, measure_generation_for_test, packaged_canonical_snapshot,
    verify_canonical_snapshot, SnapshotVerificationError, CANONICAL_SNAPSHOT_BYTES,
    CANONICAL_SNAPSHOT_SHA256,
};
use super::compiler::LibraryCompiler;
use super::profile::{ExactLibraryProfile, LibraryProfileError};
use std::collections::BTreeSet;

const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const SNAPSHOT_IDENTITY: &str = "af97017b22c9f8ff3726de9dbd49a3039cf70f2dd5a4fd9df9f71328be721dd0";
const WRONG_IDENTITY: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn assert_send_sync_static<T: Send + Sync + 'static>(_: &T) {}

#[test]
fn exact_profile_loads_all_packaged_sources_and_notices() {
    let profile = ExactLibraryProfile::load_packaged().expect("exact packaged profile");
    let sources = profile.sources();

    assert_eq!(profile.typescript_version(), "6.0.3");
    assert_eq!(profile.root_name(), "lib.es2025.full.d.ts");
    assert_eq!(profile.profile_identity(), PROFILE_IDENTITY);
    assert_eq!(sources.len(), 82);
    assert_eq!(
        sources
            .iter()
            .map(|source| source.ordinal().index())
            .collect::<Vec<_>>(),
        (0..82).collect::<Vec<_>>()
    );
    assert_eq!(
        sources
            .iter()
            .map(|source| source.name())
            .collect::<BTreeSet<_>>()
            .len(),
        82
    );
    assert!(sources.iter().all(|source| !source.bytes().is_empty()));
    assert_eq!(
        sources
            .iter()
            .map(|source| source.bytes().len())
            .sum::<usize>(),
        2_936_611
    );
    assert!(!profile.license_bytes().is_empty());
    assert!(!profile.third_party_notice_bytes().is_empty());
    profile
        .verify_packaged_assets()
        .expect("manifest binds every source and notice");
}

#[test]
fn library_compiler_separates_runtime_product_from_evidence() {
    let profile = ExactLibraryProfile::load_packaged().expect("exact packaged profile");
    let compiled = LibraryCompiler::new()
        .compile(&profile)
        .expect("complete source-backed library compilation");

    assert_send_sync_static(compiled.runtime_projection());
    assert_eq!(compiled.profile_identity(), PROFILE_IDENTITY);
    assert_eq!(compiled.report().parse_units, 82);
    assert_eq!(compiled.report().bind_units, 82);
    assert_eq!(compiled.report().statement_check_units, 82);
    assert_eq!(compiled.report().reserved_records, 13);
    assert_eq!(compiled.report().filled_records, 13);
    assert_eq!(compiled.report().publication_validations, 9);
    assert_eq!(compiled.evidence().diagnostics().record_count(), 2);
    assert_eq!(compiled.evidence().diagnostics().byte_len(), 91_453);
    assert_eq!(
        compiled.evidence().diagnostics().sha256(),
        "34cc5c2c11c7d4199dd1230f88691e97b5afb09f319153d9e56d1a45f3220c86"
    );
    assert_eq!(compiled.evidence().incompletes().record_count(), 3);
    assert_eq!(compiled.evidence().incompletes().byte_len(), 97_796);
    assert_eq!(
        compiled.evidence().incompletes().sha256(),
        "8c268088f8afd8048690584008c40a49cd3337b91f345b2e879d625525ccf6d8"
    );
    assert_eq!(compiled.evidence().library_ledger().record_count(), 5);
    assert_eq!(compiled.evidence().library_ledger().byte_len(), 189_218);
    assert_eq!(
        compiled.evidence().library_ledger().sha256(),
        "2837d5a3f65e9e8fa86459ff0784fd795069a7201d17793eecd4b6bc8c14e069"
    );
    assert_eq!(compiled.evidence().source_identities().len(), 82);
    assert!(compiled
        .evidence()
        .source_identities()
        .iter()
        .all(|source| source.is_library_owned()));
    assert_eq!(
        compiled.semantic_identity().runtime_projection(),
        compiled.runtime_projection().semantic_identity()
    );
    assert_eq!(
        compiled.semantic_identity().evidence(),
        compiled.evidence().semantic_identity()
    );
    assert_eq!(compiled.evidence().profile_identity(), PROFILE_IDENTITY);
}

#[test]
fn local_canonical_snapshot_generation_is_deterministic_and_exact() {
    let profile = ExactLibraryProfile::load_packaged().expect("exact packaged profile");
    let first = LibraryCompiler::new()
        .compile(&profile)
        .expect("first complete compilation");
    let second = LibraryCompiler::new()
        .compile(&profile)
        .expect("second complete compilation");
    let first_snapshot =
        generate_canonical_snapshot(&first).expect("first deterministic generation");
    let second_snapshot =
        generate_canonical_snapshot(&second).expect("second deterministic generation");
    let packaged = packaged_canonical_snapshot();

    assert_eq!(first.semantic_identity(), second.semantic_identity());
    assert_eq!(first_snapshot.bytes(), second_snapshot.bytes());
    assert_eq!(first_snapshot.bytes(), packaged.bytes());
    assert_eq!(first_snapshot.bytes().len(), CANONICAL_SNAPSHOT_BYTES);
    assert_eq!(first_snapshot.sha256(), SNAPSHOT_IDENTITY);
    assert_eq!(CANONICAL_SNAPSHOT_SHA256, SNAPSHOT_IDENTITY);

    let verified = verify_canonical_snapshot(first_snapshot.bytes(), packaged.binding())
        .expect("canonical archive binding");
    assert_eq!(verified.bytes(), CANONICAL_SNAPSHOT_BYTES);
    assert_eq!(verified.sha256(), SNAPSHOT_IDENTITY);
    assert_eq!(verified.binding(), packaged.binding());
}

#[test]
fn canonical_snapshot_binding_rejects_each_mutated_authority() {
    let packaged = packaged_canonical_snapshot();
    let binding = packaged.binding();

    assert!(matches!(
        verify_canonical_snapshot(
            packaged.bytes(),
            binding.with_profile_identity_for_test(WRONG_IDENTITY)
        ),
        Err(SnapshotVerificationError::ProfileIdentityMismatch { .. })
    ));
    assert!(matches!(
        verify_canonical_snapshot(
            packaged.bytes(),
            binding.with_compiler_schema_for_test(WRONG_IDENTITY)
        ),
        Err(SnapshotVerificationError::CompilerSchemaMismatch { .. })
    ));
    let error = ExactLibraryProfile::load_packaged_with_source_override_for_test(
        "lib.es5.d.ts",
        b"interface Changed {}\n",
    )
    .expect_err("changed packaged source must fail closed");
    assert!(matches!(
        error,
        LibraryProfileError::SourceIdentityMismatch { .. }
    ));

    let changed_input = ExactLibraryProfile::load_packaged()
        .expect("exact packaged profile")
        .test_input_with_appended_source("typokat.wu2.mutation.d.ts", b"interface Changed {}\n")
        .expect("valid isolated compiler mutation");
    let changed_compilation = LibraryCompiler::new()
        .compile_test_input(&changed_input)
        .expect("test input remains semantically compilable");
    let changed_snapshot =
        generate_canonical_snapshot(&changed_compilation).expect("changed generation");
    assert_ne!(changed_snapshot.bytes(), packaged.bytes());
    assert_ne!(
        changed_compilation.semantic_identity(),
        packaged.semantic_identity()
    );
    assert!(verify_canonical_snapshot(changed_snapshot.bytes(), binding).is_err());
}

#[test]
fn packaged_snapshot_access_performs_no_compiler_or_generation_work() {
    let measurement = measure_generation_for_test();
    let first = packaged_canonical_snapshot();
    let second = packaged_canonical_snapshot();
    let observed = measurement.finish();

    assert_eq!(first.bytes(), second.bytes());
    assert_eq!(first.sha256(), SNAPSHOT_IDENTITY);
    assert_eq!(observed.compiler_invocations, 0);
    assert_eq!(observed.generator_invocations, 0);
    assert_eq!(observed.source_bytes_read, 0);
}

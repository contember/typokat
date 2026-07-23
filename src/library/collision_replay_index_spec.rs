//! Disabled RED contract for WU5 authenticated replay-index admission.

use super::artifact::measure_generation_for_test;
use super::base::ReplayIndexMutationForTest;
use super::compiler::LibraryCompilerWorkScopeForTest;
use super::provider::{
    CollisionReplayIndexViolation, InitializationMeasurement, LibraryInitCause, LibraryInitStage,
    LibrarySnapshotViolation,
};
use super::snapshot::SnapshotWorkScopeForTest;
use super::{FrozenLibraryBase, LibraryBaseProvider};
use std::sync::Arc;

const PROFILE_IDENTITY: &str =
    "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const REPLAY_MANIFEST_IDENTITY: &str =
    "cc125e22a561b069f62f6707e5eb3f8187be0959bb75d8cbfb665266d21c2c95";
const SNAPSHOT_SECTIONS: [&str; 11] = [
    "store",
    "interner",
    "binder",
    "decl-types",
    "published-types",
    "namespace-terminals",
    "class-metadata",
    "semantic-identities",
    "root-name-index",
    "next-ids",
    "collision-replay-index",
];

fn acquire() -> Arc<FrozenLibraryBase> {
    LibraryBaseProvider::new()
        .get()
        .expect("canonical frozen library base")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn exact_struct_field_names(source: &str, declaration: &str) -> Vec<String> {
    let declaration_offset = source.find(declaration).expect("struct declaration");
    let body_offset = source[declaration_offset..]
        .find('{')
        .map(|offset| declaration_offset + offset + 1)
        .expect("struct body");
    let body_end = source[body_offset..]
        .find("\n}")
        .map(|offset| body_offset + offset)
        .expect("field-only struct body");
    let mut fields = Vec::new();
    let mut field = String::new();
    for line in source[body_offset..body_end].lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !field.is_empty() {
            field.push(' ');
        }
        field.push_str(line);
        if line.ends_with(',') {
            let (name, _) = field
                .strip_suffix(',')
                .and_then(|declaration| declaration.split_once(':'))
                .expect("named struct field");
            fields.push(name.trim().to_owned());
            field.clear();
        }
    }
    assert!(field.is_empty(), "unterminated struct field");
    fields
}

#[test]
fn admitted_replay_index_retains_decoded_rows_but_not_raw_manifest_bytes() {
    let source = include_str!("../check/checker/replay_index.rs");
    assert_eq!(
        exact_struct_field_names(source, "pub(crate) struct AdmittedCollisionReplayIndex"),
        [
            "schema",
            "owner_partition",
            "root_slots",
            "owner_sites",
            "reverse_edges",
            "root_slot_consumers",
            "scc_membership",
            "statement_owners",
            "baseline_records",
            "unowned_demand_count",
            "invalid_owner_site_count",
            "noncanonical_edge_count",
            "typed_reference_coverage_misses",
            "canonical_manifest_len",
            "canonical_manifest_sha256",
        ]
    );
    let declaration = source
        .split_once("pub(crate) struct AdmittedCollisionReplayIndex")
        .expect("admitted replay index declaration")
        .1
        .split_once("\n}")
        .expect("admitted replay index body")
        .0;
    assert!(!declaration.contains("Vec<u8>"));
    assert!(!declaration.contains("canonical_manifest_bytes"));
}

#[test]
fn canonical_replay_index_is_an_authenticated_eleventh_snapshot_section() {
    let compiler = LibraryCompilerWorkScopeForTest::start();
    let generation = measure_generation_for_test();
    let snapshot = SnapshotWorkScopeForTest::start();
    let base = acquire();
    let index = base.replay_index_for_test();

    assert_eq!(base.snapshot_section_inventory_for_test(), SNAPSHOT_SECTIONS);
    assert_eq!(index.schema, 1);
    assert_eq!(base.identity().profile_sha256(), PROFILE_IDENTITY);
    assert_eq!(index.canonical_manifest_len(), 10_996_257);
    assert_eq!(hex(&index.canonical_manifest_sha256), REPLAY_MANIFEST_IDENTITY);
    assert_eq!(
        hex(&base.pinned_replay_manifest_sha256_for_test()),
        REPLAY_MANIFEST_IDENTITY
    );
    assert_eq!(index.owner_partition.len(), 45_925);
    assert_eq!(index.root_slots.len(), 2_238);
    assert_eq!(index.owner_sites.len(), 47_253);
    assert_eq!(index.reverse_edges.len(), 9_922);
    assert_eq!(index.root_slot_consumers.len(), 6_940);
    assert_eq!(index.scc_membership.len(), 45_241);
    assert_eq!(index.statement_owners.len(), 42_496);
    assert_eq!(index.baseline_records.len(), 45_925);
    assert_eq!(index.unowned_demand_count, 0);
    assert_eq!(index.invalid_owner_site_count, 0);
    assert_eq!(index.noncanonical_edge_count, 0);
    assert_eq!(index.typed_reference_coverage_misses, 0);
    let snapshot_work = snapshot.finish();
    let generation_work = generation.finish();
    let compiler_work = compiler.finish();
    assert_eq!(compiler_work.compiles, 0);
    assert_eq!(compiler_work.parses, 0);
    assert_eq!(compiler_work.binds, 0);
    assert_eq!(compiler_work.checks, 0);
    assert_eq!(generation_work.compiler_invocations, 0);
    assert_eq!(generation_work.generator_invocations, 0);
    assert_eq!(generation_work.source_bytes_read, 0);
    assert_eq!(snapshot_work.validations, 1);
    assert_eq!(snapshot_work.decodes, 1);
}

#[test]
fn canonical_replay_index_is_source_reproducible_and_complete() {
    let base = acquire();
    let index = base.replay_index_for_test();
    let regenerated = FrozenLibraryBase::regenerate_replay_index_manifest_for_test()
        .expect("source compiler independently regenerates the replay index");

    assert_eq!(
        index.canonical_manifest_sha256,
        regenerated.canonical_manifest_sha256
    );
    assert_eq!(regenerated.library_source_compiles, 1);
    assert_eq!(regenerated.snapshot_decodes, 0);
    assert_eq!(index.owner_partition, regenerated.owner_partition);
    assert_eq!(index.root_slots, regenerated.root_slots);
    assert_eq!(index.owner_sites, regenerated.owner_sites);
    assert_eq!(index.reverse_edges, regenerated.reverse_edges);
    assert_eq!(
        index.root_slot_consumers,
        regenerated.root_slot_consumers
    );
    assert_eq!(index.scc_membership, regenerated.scc_membership);
    assert_eq!(index.statement_owners, regenerated.statement_owners);
    assert_eq!(index.baseline_records, regenerated.baseline_records);
}

fn assert_rejected_before_publication(
    mutation: ReplayIndexMutationForTest,
    expected_stage: LibraryInitStage,
    expected_cause: LibraryInitCause,
) {
    let compiler = LibraryCompilerWorkScopeForTest::start();
    let generation = measure_generation_for_test();
    let snapshot = SnapshotWorkScopeForTest::start();
    let mutated = super::snapshot::pre_admitted_replay_index_mutation_for_test(mutation);
    let provider = LibraryBaseProvider::with_pre_admitted_snapshot_for_test(mutated);
    let first = provider
        .get()
        .expect_err("corrupt replay index must fail before publication");
    let second = provider
        .get()
        .expect_err("repeated acquisition returns the cached failure");
    let snapshot_work = snapshot.finish();
    let generation_work = generation.finish();
    let compiler_work = compiler.finish();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(first.stage(), expected_stage);
    assert_eq!(first.cause(), &expected_cause);
    assert_eq!(
        provider.measurement_for_test(),
        InitializationMeasurement {
            attempts: 1,
            publications: 0,
        }
    );
    assert_eq!(compiler_work.compiles, 0);
    assert_eq!(compiler_work.parses, 0);
    assert_eq!(compiler_work.binds, 0);
    assert_eq!(compiler_work.checks, 0);
    assert_eq!(generation_work.compiler_invocations, 0);
    assert_eq!(generation_work.generator_invocations, 0);
    assert_eq!(generation_work.source_bytes_read, 0);
    assert_eq!(snapshot_work.validations, 0);
    assert_eq!(snapshot_work.decodes, 1);
}

#[test]
fn replay_section_envelope_corruption_fails_before_publication() {
    for (mutation, stage, cause) in [
        (
            ReplayIndexMutationForTest::MissingSection,
            LibraryInitStage::Header,
            LibraryInitCause::SnapshotRejected {
                violation: LibrarySnapshotViolation::MalformedHeader,
            },
        ),
        (
            ReplayIndexMutationForTest::WrongSectionTag,
            LibraryInitStage::Directory,
            LibraryInitCause::SnapshotRejected {
                violation: LibrarySnapshotViolation::MalformedDirectory,
            },
        ),
        (
            ReplayIndexMutationForTest::WrongSectionDigest,
            LibraryInitStage::Payload,
            LibraryInitCause::SnapshotRejected {
                violation: LibrarySnapshotViolation::InvalidPayload,
            },
        ),
        (
            ReplayIndexMutationForTest::TruncatedSection,
            LibraryInitStage::Payload,
            LibraryInitCause::SnapshotRejected {
                violation: LibrarySnapshotViolation::InvalidPayload,
            },
        ),
        (
            ReplayIndexMutationForTest::TrailingSectionBytes,
            LibraryInitStage::Directory,
            LibraryInitCause::SnapshotRejected {
                violation: LibrarySnapshotViolation::MalformedDirectory,
            },
        ),
    ] {
        assert_rejected_before_publication(mutation, stage, cause);
    }
}

#[test]
fn replay_index_structural_and_cross_section_corruption_fails_at_its_admission_boundary() {
    fn reject_family(
        mutations: impl IntoIterator<Item = ReplayIndexMutationForTest>,
        violation: CollisionReplayIndexViolation,
    ) {
        for mutation in mutations {
            assert_rejected_before_publication(
                mutation,
                LibraryInitStage::CollisionReplayIndexAdmission,
                LibraryInitCause::ReplayIndexRejected {
                    violation: violation.clone(),
                },
            );
        }
    }

    reject_family(
        [
            ReplayIndexMutationForTest::WrongManifestDomain,
            ReplayIndexMutationForTest::UnknownSchema,
            ReplayIndexMutationForTest::TruncatedInternalLength,
            ReplayIndexMutationForTest::InternalLengthOverflow,
            ReplayIndexMutationForTest::InvalidUtf8RootName,
            ReplayIndexMutationForTest::InvalidOptionalTag,
            ReplayIndexMutationForTest::InvalidBooleanTag,
            ReplayIndexMutationForTest::SemanticTrailingBytes,
            ReplayIndexMutationForTest::InvalidOwnerTag,
            ReplayIndexMutationForTest::SemanticallyEquivalentNoncanonicalEncoding,
        ],
        CollisionReplayIndexViolation::InvalidEncoding,
    );
    reject_family(
        [
            ReplayIndexMutationForTest::MissingOwner,
            ReplayIndexMutationForTest::MissingStaleButInRangeOwner,
            ReplayIndexMutationForTest::DuplicateOwner,
            ReplayIndexMutationForTest::ReorderedOwner,
            ReplayIndexMutationForTest::UnknownOwner,
            ReplayIndexMutationForTest::MissingGlobalObjectOwner,
            ReplayIndexMutationForTest::DuplicateGlobalObjectOwner,
        ],
        CollisionReplayIndexViolation::InvalidOwnerPartition,
    );
    reject_family(
        [
            ReplayIndexMutationForTest::DuplicateRoot,
            ReplayIndexMutationForTest::EmptyRootName,
            ReplayIndexMutationForTest::RootIdOutsidePrefix,
            ReplayIndexMutationForTest::PopulatedRootIndexMismatch,
        ],
        CollisionReplayIndexViolation::InvalidRootIndex,
    );
    reject_family(
        [
            ReplayIndexMutationForTest::DuplicateReverseEdge,
            ReplayIndexMutationForTest::SelfReverseEdge,
            ReplayIndexMutationForTest::ReorderedReverseEdge,
            ReplayIndexMutationForTest::UnknownReverseEdgeOwner,
            ReplayIndexMutationForTest::DuplicateRootConsumer,
            ReplayIndexMutationForTest::ReorderedRootConsumer,
            ReplayIndexMutationForTest::InvalidRootConsumerSlot,
            ReplayIndexMutationForTest::UnknownRootConsumerOwner,
            ReplayIndexMutationForTest::UnknownRootConsumerName,
        ],
        CollisionReplayIndexViolation::InvalidDependencyGraph,
    );
    reject_family(
        [
            ReplayIndexMutationForTest::MissingOwnerSite,
            ReplayIndexMutationForTest::DuplicateOwnerSite,
            ReplayIndexMutationForTest::InvalidOwnerSiteSpan,
            ReplayIndexMutationForTest::UnknownOwnerSiteOwner,
            ReplayIndexMutationForTest::OwnerSiteFileOutsideProfile,
        ],
        CollisionReplayIndexViolation::InvalidOwnerSites,
    );
    reject_family(
        [
            ReplayIndexMutationForTest::InvalidScc,
            ReplayIndexMutationForTest::MissingSccOwner,
            ReplayIndexMutationForTest::DuplicateSccOwner,
            ReplayIndexMutationForTest::ReorderedScc,
            ReplayIndexMutationForTest::WrongDependencyFirstScc,
        ],
        CollisionReplayIndexViolation::InvalidSccPartition,
    );
    reject_family(
        [
            ReplayIndexMutationForTest::WrongStatementOwner,
            ReplayIndexMutationForTest::DuplicateStatementOwner,
            ReplayIndexMutationForTest::MissingStatementOwner,
            ReplayIndexMutationForTest::StatementFileOutsideProfile,
            ReplayIndexMutationForTest::ReorderedStatementOwner,
        ],
        CollisionReplayIndexViolation::InvalidStatementPartition,
    );
    reject_family(
        [
            ReplayIndexMutationForTest::NonStatementBaselineCountNonzero,
            ReplayIndexMutationForTest::DuplicateBaseline,
            ReplayIndexMutationForTest::MissingBaseline,
        ],
        CollisionReplayIndexViolation::InvalidBaselinePartition,
    );
    reject_family(
        [
            ReplayIndexMutationForTest::NonzeroUnownedDemands,
            ReplayIndexMutationForTest::NonzeroInvalidOwnerSites,
            ReplayIndexMutationForTest::NonzeroNoncanonicalEdges,
            ReplayIndexMutationForTest::NonzeroTypedReferenceMisses,
        ],
        CollisionReplayIndexViolation::NonzeroGenerationHealthCounter,
    );
}

#[test]
fn source_only_completeness_is_enforced_by_the_independent_manifest_pin() {
    for mutation in [
        ReplayIndexMutationForTest::SelfConsistentMissingReverseEdge,
        ReplayIndexMutationForTest::SelfConsistentMissingRootConsumer,
        ReplayIndexMutationForTest::SelfConsistentMissingOwnerSite,
        ReplayIndexMutationForTest::SelfConsistentWrongBaseline,
        ReplayIndexMutationForTest::SelfConsistentWrongBaselineCount,
        ReplayIndexMutationForTest::SelfConsistentWrongRootProvenance,
        ReplayIndexMutationForTest::SelfConsistentCrossArtifactSection,
        ReplayIndexMutationForTest::SelfConsistentButUnpinned,
    ] {
        assert_rejected_before_publication(
            mutation,
            LibraryInitStage::CollisionReplayIndexAdmission,
            LibraryInitCause::ReplayIndexRejected {
                violation: CollisionReplayIndexViolation::ManifestIdentityMismatch,
            },
        );
    }
}

//! Disabled RED contract for WU5 authenticated replay-index admission.

use super::base::ReplayIndexMutationForTest;
use super::{FrozenLibraryBase, LibraryBaseProvider};
use std::sync::Arc;

fn acquire() -> Arc<FrozenLibraryBase> {
    LibraryBaseProvider::new()
        .get()
        .expect("canonical frozen library base")
}

#[test]
fn canonical_replay_index_is_authenticated_complete_and_source_reproducible() {
    let base = acquire();
    let index = base.replay_index_for_test();
    let regenerated = FrozenLibraryBase::regenerate_replay_index_manifest_for_test()
        .expect("source compiler regenerates the replay index independently");

    assert_eq!(index.schema, 1);
    assert_eq!(index.profile_identity, base.identity().profile_sha256);
    assert_eq!(
        index.canonical_manifest_bytes,
        regenerated.canonical_manifest_bytes
    );
    assert_eq!(
        index.canonical_manifest_sha256,
        regenerated.canonical_manifest_sha256
    );
    assert_eq!(
        index.canonical_manifest_sha256,
        index.pinned_manifest_sha256
    );
    assert_eq!(regenerated.library_source_compiles, 1);
    assert_eq!(regenerated.snapshot_decodes, 0);
    assert_eq!(index.owner_partition, regenerated.owner_partition);
    assert_eq!(index.root_slots, regenerated.root_slots);
    assert_eq!(index.owner_sites, regenerated.owner_sites);
    assert_eq!(index.reverse_edges, regenerated.reverse_edges);
    assert_eq!(index.scc_membership, regenerated.scc_membership);
    assert_eq!(index.statement_owners, regenerated.statement_owners);
    assert_eq!(index.baseline_records, regenerated.baseline_records);
    assert_eq!(index.unowned_demand_count, 0);
    assert_eq!(index.invalid_owner_site_count, 0);
    assert_eq!(index.noncanonical_edge_count, 0);
    assert_eq!(index.typed_reference_coverage_misses, 0);
}

#[test]
fn replay_index_corruption_fails_before_route_or_semantic_mutation() {
    for mutation in [
        ReplayIndexMutationForTest::Missing,
        ReplayIndexMutationForTest::UnknownOwner,
        ReplayIndexMutationForTest::MissingReverseEdge,
        ReplayIndexMutationForTest::ReorderedEdge,
        ReplayIndexMutationForTest::InvalidScc,
        ReplayIndexMutationForTest::MissingOwnerSite,
        ReplayIndexMutationForTest::WrongStatementOwner,
        ReplayIndexMutationForTest::WrongBaselineDigest,
        ReplayIndexMutationForTest::SelfConsistentButUnpinned,
    ] {
        let receipt = super::snapshot::decode_replay_index_mutation_for_test(mutation.clone());
        assert_eq!(receipt.applied_mutation, mutation);
        assert_eq!(receipt.error_stage, "collision-replay-index-admission");
        assert_eq!(receipt.error_kind, mutation.expected_error_kind());
        assert_eq!(receipt.published_bases, 0);
        assert_eq!(receipt.preflight_routes, 0);
        assert_eq!(receipt.user_delta_forks, 0);
        assert_eq!(receipt.candidate_semantic_work, 0);
        assert_eq!(receipt.oracle_semantic_work, 0);
        assert_eq!(receipt.source_fallbacks, 0);
    }
}

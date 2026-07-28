//! RED contract for ADR-0020's process-local collision plan.
//!
//! The plan is retained from the one ordinary source compilation. It is compact runtime data, not
//! a serialized artifact or a second full replay-index generation pass.

use super::LibraryBaseProvider;

#[test]
fn source_compile_retains_only_the_consumed_direct_plan() {
    let base = LibraryBaseProvider::new()
        .get()
        .expect("source-compiled frozen base");
    let plan = base.collision_plan_inspection_for_test();

    assert_eq!(plan.library_source_compiles, 1);
    assert_eq!(plan.second_source_censuses, 0);
    assert_eq!(plan.canonical_manifest_bytes, 0);
    assert_eq!(plan.rendered_record_digest_bytes, 0);
    assert_eq!(plan.transitive_terminal_owner_entries, 0);
    assert_eq!(plan.eager_all_owner_scc_memberships, 0);
    assert_eq!(plan.namespace_snapshot_rows, 0);
    assert_eq!(plan.runtime_snapshot_rows, 0);
    assert_eq!(plan.canonical_terminal_rows, 0);
    assert_eq!(plan.full_semantic_projection_rows, 0);
    assert!(plan.root_slot_seeds > 0);
    assert!(plan.owner_source_sites > 0);
    assert!(plan.direct_reverse_edges > 0);
    assert!(plan.statement_owner_sites > 0);
    assert!(plan.structured_record_fingerprints > 0);
    assert!(plan.structured_record_cardinalities > 0);
    assert_eq!(plan.retained_ast_nodes, 0);
    assert_eq!(plan.retained_drained_records, 0);
    assert_eq!(plan.retained_semantic_payload_rows, 0);
    assert_eq!(plan.retained_full_owner_products, 0);
    assert_eq!(plan.serialized_artifact_bytes, 0);
    assert_eq!(plan.prefix_boundaries.len(), 9);
    assert!(plan
        .prefix_boundaries
        .iter()
        .all(|boundary| boundary.exact && boundary.cardinality > 0));
    assert_eq!(plan.health.missing_active_owner_demands, 0);
    assert_eq!(plan.health.unmatched_owner_sites, 0);
    assert_eq!(plan.health.unowned_typed_references, 0);
    assert_eq!(plan.health.raw_semantic_accesses, 0);
}

#[test]
fn owner_site_capture_uses_dense_ticket_storage_without_losing_coverage() {
    const EXACT_OWNER_SITE_ROWS: usize = 46_758;

    let base = LibraryBaseProvider::new()
        .get()
        .expect("source-compiled frozen base");
    let plan = base.collision_plan_inspection_for_test();

    assert_eq!(plan.owner_source_sites, EXACT_OWNER_SITE_ROWS);
    assert_eq!(plan.owner_site_dense_slot_writes, EXACT_OWNER_SITE_ROWS);
    assert_eq!(plan.owner_site_ordered_map_inserts, 0);

    let broken = super::FrozenLibraryBase::force_ordered_owner_site_storage_for_test()
        .expect("known-broken owner-site storage finishes");
    assert_eq!(broken.owner_source_sites, EXACT_OWNER_SITE_ROWS);
    assert_eq!(broken.owner_site_dense_slot_writes, 0);
    assert_eq!(
        broken.owner_site_ordered_map_inserts,
        EXACT_OWNER_SITE_ROWS
    );
    assert!(!broken.admitted);
}

#[test]
fn compact_plan_matches_every_independent_coverage_projection() {
    let base = LibraryBaseProvider::new()
        .get()
        .expect("source-compiled frozen base");
    let comparison = base
        .compare_collision_plan_with_full_oracles_for_test()
        .expect("all plan projections compare");

    assert_eq!(comparison.compact_owner_sites, comparison.full_owner_sites);
    assert_eq!(
        comparison.compact_direct_edges,
        comparison.full_direct_edges
    );
    assert_eq!(
        comparison.compact_root_slot_consumers,
        comparison.full_root_slot_consumers
    );
    assert_eq!(
        comparison.compact_statement_owners,
        comparison.full_statement_owners
    );
    assert_eq!(
        comparison.compact_record_fingerprints,
        comparison.full_record_fingerprints
    );
    assert_eq!(
        comparison.compact_record_cardinalities,
        comparison.full_record_cardinalities
    );
    assert_eq!(
        comparison.compact_prefix_boundaries,
        comparison.full_prefix_boundaries
    );
    assert!(comparison.binder_source_census_complete);
    assert!(comparison.binder_provenance_complete);
    assert!(comparison.lexical_event_site_audit_complete);
    assert!(comparison.global_source_site_audit_complete);
    assert!(comparison.source_access_manifest_complete);
    assert!(comparison.injected_raw_bypass_rejected);
    assert!(comparison.forbidden_projection_callsite_audit_complete);
    assert!(comparison.typed_reference_coverage_complete);
    assert!(comparison.raw_semantic_access_guard_complete);
}

#[test]
fn every_plan_gate_has_a_known_broken_negative_control() {
    let base = LibraryBaseProvider::new()
        .get()
        .expect("source-compiled frozen base");
    for mutation in [
        "drop-direct-edge",
        "drop-owner-site",
        "drop-root-slot-consumer",
        "drop-statement-owner",
        "drop-record-fingerprint",
        "change-record-cardinality",
        "change-record-elaboration",
        "change-prefix-boundary",
        "drop-binder-provenance",
        "change-owner-site-kind",
        "change-owner-site-span-end",
        "duplicate-owner-and-drop-dense-id",
        "out-of-range-root-owner",
        "drop-typed-reference",
        "add-raw-semantic-access",
        "perform-forbidden-projection",
    ] {
        let rejected = base
            .admit_mutated_collision_plan_for_test(mutation)
            .expect("mutation harness finishes");
        assert!(!rejected.admitted, "{mutation} must be rejected");
        assert!(rejected.guard_fired, "{mutation} must exercise its guard");
    }
}

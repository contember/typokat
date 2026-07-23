//! RED contracts for WU5 authenticated replay admission and binder continuation.

use super::artifact::measure_generation_for_test;
use super::base::{
    CheckpointAuthenticationMutationForTest, ReplayIndexMutationForTest,
    UserDeltaProjectInputForTest,
};
use super::compiler::LibraryCompilerWorkScopeForTest;
use super::provider::{
    CheckpointAuthenticationViolation, CollisionReplayIndexViolation, InitializationMeasurement,
    LibraryInitCause, LibraryInitStage, LibrarySnapshotViolation,
};
use super::snapshot::SnapshotWorkScopeForTest;
use super::{FrozenLibraryBase, LibraryBaseProvider};
use crate::binder::declaration::TypeGroupId;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::checker::events::UserEventReservationScopeForTest;
use crate::check::checker::library_compiler::{
    UserDeltaForkScopeForTest, UserSourceWorkScopeForTest,
};
use crate::check::checker::replay_index::ReplayOwnerSite;
use crate::check::query::QueryCacheWriteScopeForTest;
use crate::relate::cache::RelationCacheWriteScopeForTest;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
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
            fields.push(
                name.trim()
                    .strip_prefix("pub(crate) ")
                    .unwrap_or(name.trim())
                    .to_owned(),
            );
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
fn singleton_scc_owner_rows_use_inline_storage_without_changing_the_wire_model() {
    let source = include_str!("../check/checker/replay_index.rs");
    let declaration = source
        .split_once("pub(crate) struct ReplayScc")
        .expect("replay SCC declaration")
        .1
        .split_once("\n}")
        .expect("replay SCC body")
        .0;
    let normalized = declaration.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        normalized.contains("owners: SmallVec<[ReplayOwner; 1]>,"),
        "the overwhelmingly singleton SCC partition must not allocate one owner Vec per row"
    );
    assert!(
        source.contains("pub(crate) scc_membership: Vec<ReplayScc>,"),
        "inline owner storage must preserve the ordered retained SCC row model"
    );
}

#[test]
fn canonical_replay_index_is_an_authenticated_eleventh_snapshot_section() {
    let compiler = LibraryCompilerWorkScopeForTest::start();
    let generation = measure_generation_for_test();
    let snapshot = SnapshotWorkScopeForTest::start();
    let base = acquire();
    let index = base.replay_index_for_test();

    assert_eq!(
        base.snapshot_section_inventory_for_test(),
        SNAPSHOT_SECTIONS
    );
    assert_eq!(index.schema, 1);
    assert_eq!(base.identity().profile_sha256(), PROFILE_IDENTITY);
    assert_eq!(index.canonical_manifest_len(), 10_996_257);
    assert_eq!(
        hex(&index.canonical_manifest_sha256),
        REPLAY_MANIFEST_IDENTITY
    );
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
    assert_eq!(index.root_slot_consumers, regenerated.root_slot_consumers);
    assert_eq!(index.scc_membership, regenerated.scc_membership);
    assert_eq!(index.statement_owners, regenerated.statement_owners);
    assert_eq!(index.baseline_records, regenerated.baseline_records);
}

struct CheckpointBoundaryScopes {
    library: LibraryCompilerWorkScopeForTest,
    snapshot: SnapshotWorkScopeForTest,
    user: UserSourceWorkScopeForTest,
    delta: UserDeltaForkScopeForTest,
    events: UserEventReservationScopeForTest,
    query: QueryCacheWriteScopeForTest,
    relation: RelationCacheWriteScopeForTest,
}

impl CheckpointBoundaryScopes {
    fn start() -> Self {
        Self {
            library: LibraryCompilerWorkScopeForTest::start(),
            snapshot: SnapshotWorkScopeForTest::start(),
            user: UserSourceWorkScopeForTest::start(),
            delta: UserDeltaForkScopeForTest::start(),
            events: UserEventReservationScopeForTest::start(),
            query: QueryCacheWriteScopeForTest::start(),
            relation: RelationCacheWriteScopeForTest::start(),
        }
    }

    fn assert_exact_library_only_authentication(self) {
        let library = self.library.finish();
        let snapshot = self.snapshot.finish();
        let user = self.user.finish();
        let query = self.query.finish();
        assert_eq!(
            (library.compiles, library.parses, library.binds),
            (1, 82, 82)
        );
        assert_eq!(library.checks, 0);
        assert_eq!(snapshot.decodes, 1);
        assert!(snapshot.validations <= 1);
        assert_eq!((user.binds, user.checks), (0, 0));
        assert_eq!(self.delta.finish(), 0);
        assert_eq!(self.events.finish(), 0);
        assert_eq!((query.projection, query.evaluator), (0, 0));
        assert_eq!(self.relation.finish(), 0);
    }
}

fn checkpoint_inputs() -> [UserDeltaProjectInputForTest<'static>; 2] {
    let inputs = [
        UserDeltaProjectInputForTest {
            path: "/project/00_augment.ts",
            source: "interface Array<T> { wu5Checkpoint(): T; }\n",
        },
        UserDeltaProjectInputForTest {
            path: "/project/99_consume.ts",
            source: "const value: number = [1, 2].wu5Checkpoint();\n",
        },
    ];
    inputs
}

fn assert_dense(actual: impl IntoIterator<Item = usize>, start: usize, end: usize) {
    assert_eq!(
        actual.into_iter().collect::<Vec<_>>(),
        (start..end).collect::<Vec<_>>()
    );
}

#[test]
fn exact_library_only_binder_checkpoint_authenticates_before_user_continuation() {
    let scopes = CheckpointBoundaryScopes::start();
    let inspection_reached = Cell::new(false);
    let checkpoint_array_symbol = Cell::new(None::<SymbolId>);
    let checkpoint_array_type_group = Cell::new(None::<TypeGroupId>);
    let inspected_library_modules = RefCell::new(Vec::<ScopeId>::new());
    let admitted_owner_sites = RefCell::new(Vec::<ReplayOwnerSite>::new());
    let continuation = LibraryBaseProvider::new()
        .continue_authenticated_library_binder_checkpoint_for_test(
            &checkpoint_inputs(),
            None,
            |checkpoint, admitted_snapshot, admitted_replay| {
                inspection_reached.set(true);
                scopes.assert_exact_library_only_authentication();
                assert_eq!(checkpoint.library_units.len(), 82);
                assert_eq!(checkpoint.library_units, admitted_snapshot.library_units());
                for (index, unit) in checkpoint.library_units.iter().enumerate() {
                    assert_eq!(unit.ordinal.index(), index);
                    assert_eq!(
                        usize::try_from(unit.source.0).expect("source key fits usize"),
                        index + 1
                    );
                }
                assert_eq!(
                    checkpoint.source_binder_encoding_sha256,
                    admitted_snapshot
                        .section_digest(3)
                        .expect("admitted binder section digest")
                );
                assert_eq!(
                    checkpoint.source_root_encoding_sha256,
                    admitted_snapshot
                        .section_digest(9)
                        .expect("admitted root section digest")
                );
                let prefixes = admitted_snapshot.library_prefixes();
                assert_eq!(checkpoint.ends.scopes, prefixes.scopes);
                assert_eq!(checkpoint.ends.symbols, prefixes.symbols);
                assert_eq!(checkpoint.ends.declarations, prefixes.declarations);
                assert_eq!(checkpoint.ends.type_groups, prefixes.type_groups);
                assert_eq!(checkpoint.ends.namespaces, prefixes.namespaces);
                assert_eq!(checkpoint.ends.value_storages, prefixes.value_storages);
                assert_eq!(checkpoint.ends.next_source, admitted_snapshot.next_source());
                assert!(checkpoint.array_symbol.index() < checkpoint.ends.symbols);
                assert!(checkpoint.array_type_group.index() < checkpoint.ends.type_groups);
                checkpoint_array_symbol.set(Some(checkpoint.array_symbol));
                checkpoint_array_type_group.set(Some(checkpoint.array_type_group));
                assert_eq!(admitted_replay.owner_sites.len(), 47_253);
                inspected_library_modules.replace(
                    checkpoint
                        .library_units
                        .iter()
                        .map(|unit| unit.module)
                        .collect(),
                );
                admitted_owner_sites.replace(admitted_replay.owner_sites.clone());
            },
        )
        .expect("authenticated binder checkpoint continues through the user files");

    assert!(inspection_reached.get());
    assert_eq!(
        checkpoint_array_symbol.get(),
        Some(continuation.array_symbol_before_augmentation)
    );
    assert_eq!(
        checkpoint_array_type_group.get(),
        Some(continuation.array_type_group_before_augmentation)
    );
    let inspected_library_modules = inspected_library_modules.borrow();
    let admitted_owner_sites = admitted_owner_sites.borrow();
    assert_eq!(
        continuation.mapped_owner_sites.len(),
        admitted_owner_sites.len()
    );
    for (mapped, admitted) in continuation
        .mapped_owner_sites
        .iter()
        .zip(admitted_owner_sites.iter())
    {
        assert_eq!(mapped.owner, admitted.owner);
        assert_eq!(mapped.file_ordinal, admitted.file_ordinal);
        assert_eq!(mapped.span, admitted.span);
        assert_eq!(
            mapped.syntax_module,
            inspected_library_modules[admitted.file_ordinal.index()]
        );
    }
    assert_eq!(
        continuation.array_symbol_after_augmentation,
        continuation.array_symbol_before_augmentation
    );
    assert_eq!(
        continuation.array_type_group_after_augmentation,
        continuation.array_type_group_before_augmentation
    );
    assert_eq!(
        continuation.consumer_array_type_group,
        continuation.array_type_group_before_augmentation
    );
    assert!(
        continuation.augmentation_declaration.index() >= continuation.checkpoint_ends.declarations
    );
    assert_dense(
        continuation.appended_scopes.iter().map(|id| id.index()),
        continuation.checkpoint_ends.scopes,
        continuation.ends.scopes,
    );
    assert_dense(
        continuation.appended_symbols.iter().map(|id| id.index()),
        continuation.checkpoint_ends.symbols,
        continuation.ends.symbols,
    );
    assert_dense(
        continuation
            .appended_declarations
            .iter()
            .map(|id| id.index()),
        continuation.checkpoint_ends.declarations,
        continuation.ends.declarations,
    );
    assert_dense(
        continuation
            .appended_type_groups
            .iter()
            .map(|id| id.index()),
        continuation.checkpoint_ends.type_groups,
        continuation.ends.type_groups,
    );
    assert_dense(
        continuation.appended_namespaces.iter().map(|id| id.index()),
        continuation.checkpoint_ends.namespaces,
        continuation.ends.namespaces,
    );
    assert_dense(
        continuation
            .appended_value_storages
            .iter()
            .map(|id| id.index()),
        continuation.checkpoint_ends.value_storages,
        continuation.ends.value_storages,
    );
    assert_eq!(continuation.appended_module_sources.len(), 2);
    assert_dense(
        continuation
            .appended_module_sources
            .iter()
            .map(|row| usize::try_from(row.source.0).expect("source key fits usize")),
        continuation.checkpoint_ends.next_source,
        continuation.ends.next_source,
    );
    assert!(continuation
        .appended_module_sources
        .iter()
        .all(|row| continuation.appended_scopes.contains(&row.module)));
}

#[test]
fn binder_root_and_prefix_mismatches_fail_before_checkpoint_inspection() {
    for (mutation, violation) in [
        (
            CheckpointAuthenticationMutationForTest::BinderDigest,
            CheckpointAuthenticationViolation::BinderDigestMismatch,
        ),
        (
            CheckpointAuthenticationMutationForTest::RootDigest,
            CheckpointAuthenticationViolation::RootDigestMismatch,
        ),
        (
            CheckpointAuthenticationMutationForTest::PrefixNextIds,
            CheckpointAuthenticationViolation::PrefixMismatch,
        ),
    ] {
        let scopes = CheckpointBoundaryScopes::start();
        let inspection_reached = Cell::new(false);
        let error = LibraryBaseProvider::new()
            .continue_authenticated_library_binder_checkpoint_for_test(
                &checkpoint_inputs(),
                Some(mutation),
                |_, _, _| inspection_reached.set(true),
            )
            .expect_err("checkpoint evidence mismatch must fail closed");
        assert!(!inspection_reached.get(), "{mutation:?}");
        assert_eq!(error.stage(), LibraryInitStage::ReferenceValidation);
        assert_eq!(
            error.cause(),
            &LibraryInitCause::CheckpointAuthenticationRejected { violation }
        );
        scopes.assert_exact_library_only_authentication();
    }
}

#[test]
fn binder_checkpoint_type_boundary_cannot_resume_raw_or_through_wu4() {
    let binder = include_str!("../binder/bind.rs");
    let private_compiler = include_str!("../check/checker/library_compiler.rs");
    let codec = include_str!("../check/checker/library_snapshot_codec/mod.rs");
    let unauthenticated_offset = binder
        .find("struct UnauthenticatedLibraryBinderCheckpoint {")
        .expect("private unauthenticated checkpoint");
    let unauthenticated_attributes = binder[..unauthenticated_offset]
        .rsplit_once("\n\n")
        .map_or(&binder[..unauthenticated_offset], |(_, attributes)| {
            attributes
        });
    let unauthenticated = binder
        .split_once("struct UnauthenticatedLibraryBinderCheckpoint {")
        .expect("private unauthenticated checkpoint")
        .1
        .split_once("\n}")
        .expect("unauthenticated checkpoint body")
        .0;
    assert!(!unauthenticated.contains("pub"));
    assert!(!unauthenticated_attributes.contains("Clone"));
    assert!(binder.contains("struct AuthenticatedLibraryBinderCheckpoint {"));
    assert!(binder.contains("checkpoint: UnauthenticatedLibraryBinderCheckpoint"));
    assert!(binder.contains("Result<AuthenticatedLibraryBinderCheckpoint"));
    assert!(codec.contains("struct AdmittedLibrarySnapshotEvidence {"));
    let continuation = private_compiler
        .split_once("fn continue_authenticated_library_binder_checkpoint(")
        .expect("authenticated continuation entrypoint")
        .1
        .split_once("\n}")
        .expect("authenticated continuation body")
        .0;
    assert!(continuation.contains("checkpoint: AuthenticatedLibraryBinderCheckpoint"));
    assert!(!continuation.contains("resume_frozen_library("));
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
    assert_eq!(first.stage(), expected_stage, "{mutation:?}");
    assert_eq!(first.cause(), &expected_cause, "{mutation:?}");
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
            ReplayIndexMutationForTest::RootNameLengthOverflow,
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
            ReplayIndexMutationForTest::MissingCanonicalPopulatedRoot,
            ReplayIndexMutationForTest::UnusedPlaceholderRoot,
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
            ReplayIndexMutationForTest::NonStatementBaselineDigestNoncanonical,
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

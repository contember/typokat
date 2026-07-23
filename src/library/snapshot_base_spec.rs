//! Disabled RED contract for strict canonical-snapshot decoding and atomic base publication.
//!
//! This spec ends at an immutable, pointer-identical `FrozenLibraryBase`. User deltas,
//! collision routing, semantic bridges, the process-wide singleton, driver/CLI cutover,
//! and user checking belong to later work and must not be pulled into this boundary.

use super::artifact::{
    measure_generation_for_test, CANONICAL_SNAPSHOT_BYTES, CANONICAL_SNAPSHOT_SHA256,
};
use super::base::FrozenLibraryBase;
use super::compiler::LibraryCompiler;
use super::profile::ExactLibraryProfile;
use super::provider::{
    InitializationMeasurement, LibraryBaseProvider, LibraryInitCause, LibraryInitError,
    LibraryInitStage,
};
use super::snapshot::test_support::{
    canonical_bytes_with_mutation_for_test, canonical_projection_from_compiled_for_test,
    pre_admitted_snapshot_case_for_test, ReferenceEndpoint, SnapshotTestMutation,
};
use std::sync::{Arc, Barrier};

const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const SCHEMA_IDENTITY: &str = "88fd84240ad5f574ddb1ee1bed1a631682d3ec15583882a5fbe4d9f9ca97e599";
const EXPECTED_COMPONENTS: [&str; 11] = [
    "store",
    "interner",
    "binder",
    "declaration-types",
    "published-types",
    "namespace-terminals",
    "class-metadata",
    "semantic-identities",
    "root-name-index",
    "id-prefixes",
    "collision-replay-index",
];

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

fn exact_production_struct_fields(source: &str, declaration: &str) -> Vec<String> {
    let declaration_offset = source.find(declaration).expect("struct declaration");
    let body_offset = source[declaration_offset..]
        .find('{')
        .map(|offset| declaration_offset + offset + 1)
        .expect("struct body");
    let body_end = source[body_offset..]
        .find("\n}")
        .map(|offset| body_offset + offset)
        .expect("simple private field block");

    let mut test_only = false;
    source[body_offset..body_end]
        .lines()
        .filter_map(|line| {
            if line.trim() == "#[cfg(test)]" {
                test_only = true;
                return None;
            }
            let field = line.strip_prefix("    ")?.strip_suffix(',')?;
            if !field.contains(':') {
                return None;
            }
            let retained = (!test_only).then(|| field.to_owned());
            test_only = false;
            retained
        })
        .collect()
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
    let mut declaration = String::new();
    for line in source[body_offset..body_end].lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !declaration.is_empty() {
            declaration.push(' ');
        }
        declaration.push_str(line);
        if line.ends_with(',') {
            let (name, _) = declaration
                .strip_suffix(',')
                .and_then(|field| field.split_once(':'))
                .expect("named struct field");
            fields.push(name.trim().to_owned());
            declaration.clear();
        }
    }
    assert!(declaration.is_empty(), "unterminated struct field");
    fields
}

fn acquire(
    provider: &LibraryBaseProvider,
) -> Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>> {
    provider.get()
}

fn concurrent_acquire(
    provider: Arc<LibraryBaseProvider>,
    callers: usize,
) -> Vec<Result<Arc<FrozenLibraryBase>, Arc<LibraryInitError>>> {
    let barrier = Arc::new(Barrier::new(callers));
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(callers);
        for _ in 0..callers {
            let provider = Arc::clone(&provider);
            let barrier = Arc::clone(&barrier);
            handles.push(scope.spawn(move || {
                barrier.wait();
                acquire(&provider)
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("test acquisition thread"))
            .collect()
    })
}

#[test]
fn canonical_snapshot_decodes_to_complete_frozen_library_base() {
    let provider = LibraryBaseProvider::new();
    let base = acquire(&provider).expect("canonical frozen library base");
    let identity = base.identity();
    let inventory = base.inventory_for_test();

    assert_send_sync_static::<FrozenLibraryBase>();
    assert_send_sync_static::<Arc<FrozenLibraryBase>>();
    assert_send_sync_static::<LibraryBaseProvider>();
    assert_send_sync_static::<LibraryInitError>();

    assert_eq!(identity.profile_sha256(), PROFILE_IDENTITY);
    assert_eq!(identity.schema_sha256(), SCHEMA_IDENTITY);
    assert_eq!(identity.artifact_sha256(), CANONICAL_SNAPSHOT_SHA256);
    assert_eq!(identity.artifact_bytes(), CANONICAL_SNAPSHOT_BYTES);
    assert_eq!(inventory.source_file_count(), 82);
    assert_eq!(inventory.reference_count(), 296_414);
    assert_eq!(inventory.runtime_family_count(), 11);
    assert_eq!(inventory.projection_subtable_count(), 31);
    assert_eq!(inventory.component_names(), EXPECTED_COMPONENTS);
    assert_eq!(
        inventory.root_name_count(),
        base.root_names_for_test().len()
    );
    assert_eq!(inventory.prefixes().types, base.type_count_for_test());
    assert!(inventory.prefixes().types > 0);
    assert!(inventory.prefixes().type_params > 0);
    assert!(inventory.prefixes().classes > 0);
    assert!(inventory.prefixes().scopes > 0);
    assert!(inventory.prefixes().symbols > 0);
    assert!(inventory.prefixes().declarations > 0);
    assert!(inventory.prefixes().type_groups > 0);
    assert!(inventory.prefixes().namespaces > 0);
    assert!(inventory.prefixes().value_storages > 0);
    assert_eq!(
        provider.measurement_for_test(),
        InitializationMeasurement {
            attempts: 1,
            publications: 1,
        }
    );
}

#[test]
fn source_compiled_and_decoded_bases_have_identical_canonical_projection() {
    let profile = ExactLibraryProfile::load_packaged().expect("exact packaged profile");
    let compiled = LibraryCompiler::new()
        .compile(&profile)
        .expect("complete source-backed compilation");
    let source_projection =
        canonical_projection_from_compiled_for_test(&compiled).expect("source runtime projection");
    let provider = LibraryBaseProvider::new();
    let decoded = acquire(&provider).expect("decoded canonical base");
    let decoded_projection = decoded
        .recompute_canonical_projection_for_test()
        .expect("projection recomputed from decoded typed tables");
    let timed_projection_sha256 = provider
        .typed_validation_sha256_for_test()
        .expect("typed projection identity is published with the base");

    assert_eq!(decoded_projection, source_projection);
    assert_eq!(
        timed_projection_sha256,
        decoded_projection.typed_validation_sha256()
    );
    assert_eq!(
        timed_projection_sha256,
        source_projection.typed_validation_sha256()
    );
    assert_eq!(source_projection.runtime_families().len(), 11);
    assert_eq!(source_projection.subtables().len(), 31);
    assert_eq!(
        source_projection
            .reference_family_counts()
            .iter()
            .sum::<u64>(),
        296_414
    );
    assert_eq!(
        source_projection.root_names(),
        decoded.root_names_for_test()
    );
    assert_eq!(source_projection.prefixes(), decoded.prefixes_for_test());
}

#[test]
fn provider_returns_one_pointer_identical_base_to_1_2_32_callers() {
    for callers in [1, 2, 32] {
        let provider = Arc::new(LibraryBaseProvider::new());
        let acquired = concurrent_acquire(Arc::clone(&provider), callers);
        let bases = acquired
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .expect("every caller receives the base");
        let first = bases.first().expect("at least one caller");
        assert!(bases.iter().all(|base| Arc::ptr_eq(first, base)));
        assert!(Arc::ptr_eq(
            first,
            &acquire(&provider).expect("subsequent cached acquisition")
        ));
        assert_eq!(
            provider.measurement_for_test(),
            InitializationMeasurement {
                attempts: 1,
                publications: 1,
            },
            "caller count {callers}"
        );
    }
}

#[test]
fn provider_caches_one_pointer_identical_typed_initialization_failure() {
    for callers in [1, 2, 32] {
        let snapshot =
            pre_admitted_snapshot_case_for_test(SnapshotTestMutation::DanglingReference {
                family: 0,
                endpoint: ReferenceEndpoint::First,
            })
            .expect("digest-valid dangling-reference fixture");
        let provider = Arc::new(LibraryBaseProvider::with_pre_admitted_snapshot_for_test(
            snapshot,
        ));
        let acquired = concurrent_acquire(Arc::clone(&provider), callers);
        let errors = acquired
            .into_iter()
            .map(|result| result.expect_err("corrupt base must not publish"))
            .collect::<Vec<_>>();
        let first = errors.first().expect("at least one caller");

        assert!(errors.iter().all(|error| Arc::ptr_eq(first, error)));
        assert_eq!(first.stage(), LibraryInitStage::ReferenceValidation);
        assert!(matches!(first.cause(), LibraryInitCause::InvalidId { .. }));
        let later = acquire(&provider).expect_err("cached failure must not retry");
        assert!(Arc::ptr_eq(first, &later));
        assert_eq!(
            provider.measurement_for_test(),
            InitializationMeasurement {
                attempts: 1,
                publications: 0,
            },
            "caller count {callers}"
        );
    }
}

#[test]
fn canonical_provider_rejects_every_changed_byte_at_artifact_admission() {
    for mutation in [
        SnapshotTestMutation::BadMagic,
        SnapshotTestMutation::UnknownVersion,
        SnapshotTestMutation::WrongProfileIdentity,
        SnapshotTestMutation::WrongSchemaIdentity,
        SnapshotTestMutation::WrongBodyDigest,
        SnapshotTestMutation::WrongSectionDigest,
        SnapshotTestMutation::UnknownSectionTag,
        SnapshotTestMutation::ReorderedSections,
        SnapshotTestMutation::TruncatedPayload,
        SnapshotTestMutation::TrailingBytes,
    ] {
        let bytes = canonical_bytes_with_mutation_for_test(mutation)
            .expect("independent canonical mutation fixture");
        let provider = LibraryBaseProvider::with_canonical_bytes_for_test(bytes);
        let error = acquire(&provider).expect_err("changed canonical bytes must fail closed");
        assert_eq!(
            error.stage(),
            LibraryInitStage::ArtifactAdmission,
            "mutation {mutation:?}"
        );
        assert!(matches!(
            error.cause(),
            LibraryInitCause::ArtifactIdentity { .. }
        ));
        assert_eq!(
            provider.measurement_for_test(),
            InitializationMeasurement {
                attempts: 1,
                publications: 0,
            },
            "mutation {mutation:?}"
        );
    }
}

#[test]
fn pre_admitted_decoder_rejects_deep_structural_corruption_before_publication() {
    let cases = [
        (
            SnapshotTestMutation::UnknownVersion,
            LibraryInitStage::Header,
        ),
        (
            SnapshotTestMutation::WrongProfileIdentity,
            LibraryInitStage::Header,
        ),
        (
            SnapshotTestMutation::WrongSchemaIdentity,
            LibraryInitStage::Header,
        ),
        (
            SnapshotTestMutation::WrongBodyDigest,
            LibraryInitStage::Payload,
        ),
        (
            SnapshotTestMutation::WrongSectionDigest,
            LibraryInitStage::Payload,
        ),
        (
            SnapshotTestMutation::UnknownSectionTag,
            LibraryInitStage::Directory,
        ),
        (
            SnapshotTestMutation::DuplicateSectionTag,
            LibraryInitStage::Directory,
        ),
        (
            SnapshotTestMutation::ReorderedSections,
            LibraryInitStage::Directory,
        ),
        (
            SnapshotTestMutation::NonZeroReservedDirectoryField,
            LibraryInitStage::Directory,
        ),
        (
            SnapshotTestMutation::OverlappingSection,
            LibraryInitStage::Directory,
        ),
        (
            SnapshotTestMutation::GappedSection,
            LibraryInitStage::Directory,
        ),
        (
            SnapshotTestMutation::LengthOverflow,
            LibraryInitStage::Directory,
        ),
        (
            SnapshotTestMutation::TruncatedPayload,
            LibraryInitStage::Payload,
        ),
        (
            SnapshotTestMutation::TrailingBytes,
            LibraryInitStage::Directory,
        ),
        (
            SnapshotTestMutation::NextIdMismatch,
            LibraryInitStage::ReferenceValidation,
        ),
        (
            SnapshotTestMutation::NextTypeParamIdMismatch,
            LibraryInitStage::ReferenceValidation,
        ),
        (
            SnapshotTestMutation::NextClassIdMismatch,
            LibraryInitStage::ReferenceValidation,
        ),
        (
            SnapshotTestMutation::InternerBucketMismatch,
            LibraryInitStage::ReferenceValidation,
        ),
        (
            SnapshotTestMutation::RootIndexBinderMismatch,
            LibraryInitStage::ReferenceValidation,
        ),
        (
            SnapshotTestMutation::NonTerminalPublication,
            LibraryInitStage::Publication,
        ),
    ];

    for (mutation, expected_stage) in cases {
        let snapshot = pre_admitted_snapshot_case_for_test(mutation)
            .expect("independently rehashed structural corruption fixture");
        let provider = LibraryBaseProvider::with_pre_admitted_snapshot_for_test(snapshot);
        let error = acquire(&provider).expect_err("structural corruption must fail closed");
        assert_eq!(error.stage(), expected_stage, "mutation {mutation:?}");
        assert_eq!(provider.measurement_for_test().publications, 0);
    }

    for family in 0..9 {
        for endpoint in [ReferenceEndpoint::First, ReferenceEndpoint::Last] {
            let mutation = SnapshotTestMutation::DanglingReference { family, endpoint };
            let snapshot = pre_admitted_snapshot_case_for_test(mutation)
                .expect("digest-valid per-family dangling reference");
            let provider = LibraryBaseProvider::with_pre_admitted_snapshot_for_test(snapshot);
            let error = acquire(&provider).expect_err("dangling reference must fail closed");
            assert_eq!(error.stage(), LibraryInitStage::ReferenceValidation);
            assert_eq!(provider.measurement_for_test().publications, 0);
        }
    }

    for mutation in [
        SnapshotTestMutation::InvalidReferenceOwner,
        SnapshotTestMutation::InvalidReferenceDomain,
        SnapshotTestMutation::InvalidReferenceField,
    ] {
        let snapshot = pre_admitted_snapshot_case_for_test(mutation)
            .expect("digest-valid reference discriminant corruption");
        let provider = LibraryBaseProvider::with_pre_admitted_snapshot_for_test(snapshot);
        let error = acquire(&provider).expect_err("invalid discriminant must fail closed");
        assert_eq!(error.stage(), LibraryInitStage::ReferenceValidation);
        assert_eq!(provider.measurement_for_test().publications, 0);
    }
}

#[test]
fn frozen_library_base_retains_only_ast_free_semantic_state() {
    let provider = LibraryBaseProvider::new();
    let base = acquire(&provider).expect("canonical frozen base");
    let inventory = base.inventory_for_test();
    let references = base
        .validate_frozen_reference_boundaries_for_test()
        .expect("every retained typed reference stays inside the frozen prefixes");

    assert_eq!(inventory.component_names(), EXPECTED_COMPONENTS);
    assert_eq!(references.checked, 296_414);
    assert_eq!(references.outside_frozen_prefix, 0);
    assert_eq!(references.base_to_delta, 0);
    assert_eq!(references.untyped_or_unowned, 0);
    assert_eq!(base.retained_source_bytes_for_test(), 0);
    assert_eq!(base.retained_archive_bytes_for_test(), 0);
    assert_eq!(base.retained_projection_witnesses_for_test(), 0);

    let source = include_str!("base.rs");
    assert!(
        source.contains("use crate::check::checker::library_compiler::OwnedLibraryRuntimeState;")
    );
    assert!(!source.contains("type OwnedLibraryRuntimeState"));
    assert!(source.contains(
        "#[cfg(test)]\n    structural_probe: Option<NonterminalStructuralTypeProbeForTest>"
    ));
    assert_eq!(
        exact_production_struct_fields(source, "pub struct FrozenLibraryBase"),
        [
            "runtime: OwnedLibraryRuntimeState",
            "root_names: BTreeSet<String>",
            "prefixes: FrozenLibraryPrefixes",
            "identity: FrozenLibraryIdentity",
        ]
    );
    let body = exact_production_struct_fields(source, "pub struct FrozenLibraryBase").join("\n");
    for forbidden in [
        "Allocator",
        "Program",
        "Ast",
        "SourceText",
        "Pass",
        "Draft",
        "EventStore",
        "Flow",
        "Query",
        "Evaluator",
        "Projection",
        "Relation",
        "ApplicationCache",
        "Vec<u8>",
    ] {
        assert!(
            !body.contains(forbidden),
            "retained forbidden field: {forbidden}"
        );
    }

    let compiler_source = include_str!("../check/checker/library_compiler.rs");
    assert_eq!(
        exact_struct_field_names(
            compiler_source,
            "pub(crate) struct OwnedLibraryRuntimeState"
        ),
        [
            "interner",
            "binder",
            "published_types",
            "decl_types",
            "semantic_identities",
            "runtime",
            "next_type_param",
            "next_class_id",
            "source_file_count",
            "replay_index",
        ]
    );
    let checker_source = include_str!("../check/checker/mod.rs");
    assert_eq!(
        exact_struct_field_names(
            checker_source,
            "pub(in crate::check::checker) struct FrozenCheckerRuntimeMetadata",
        ),
        [
            "class_application_parameters",
            "class_new_metadata",
            "class_parents",
            "class_value_aliases",
            "class_value_bindings",
            "standalone_namespace_value_aliases",
            "class_names",
            "namespace_terminals",
            "named_function_symbols",
        ]
    );
}

#[test]
fn provider_acquisition_performs_no_source_compilation_or_generation() {
    let measurement = measure_generation_for_test();
    let provider = LibraryBaseProvider::new();
    let first = acquire(&provider).expect("first acquisition");
    let second = acquire(&provider).expect("cached acquisition");
    let observed = measurement.finish();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(observed.compiler_invocations, 0);
    assert_eq!(observed.generator_invocations, 0);
    assert_eq!(observed.source_bytes_read, 0);
}

#[test]
#[ignore = "fresh-process release evidence probe for the production frozen base"]
fn frozen_library_base_release_probe_once() {
    let record = super::provider::frozen_library_base_release_probe_for_test()
        .expect("production frozen-base probe");

    assert_eq!(record.route, "production-frozen-library-base");
    assert_eq!(record.profile_sha256, PROFILE_IDENTITY);
    assert_eq!(record.schema_sha256, SCHEMA_IDENTITY);
    assert_eq!(record.artifact_sha256, CANONICAL_SNAPSHOT_SHA256);
    assert_eq!(record.artifact_bytes, CANONICAL_SNAPSHOT_BYTES);
    assert_eq!(record.initializations, 1);
    assert_eq!(record.publications, 1);
    assert_eq!(record.compiler_invocations, 0);
    assert_eq!(record.generator_invocations, 0);
    assert_eq!(record.source_bytes_read, 0);
    assert!(record.validation_us > 0);
    assert!(record.decode_us > 0);
    assert!(record.publication_us > 0);
    assert!(!record.typed_validation_sha256.is_empty());
    let framed = record.render();
    assert!(framed.starts_with("TYPOKAT_LIBRARY_BASE_PROBE={"));
    assert_eq!(framed.matches("TYPOKAT_LIBRARY_BASE_PROBE=").count(), 1);
    println!("{framed}");
}

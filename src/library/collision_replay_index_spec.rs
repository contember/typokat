//! RED contracts for WU5 authenticated replay admission and binder continuation.

use super::artifact::measure_generation_for_test;
use super::base::{
    CheckpointAuthenticationMutationForTest, ReplayIndexMutationForTest,
    UserDeltaProjectInputForTest,
};
use super::compiler::{LibraryCompiler, LibraryCompilerWorkScopeForTest};
use super::profile::ExactLibraryProfile;
use super::provider::{
    CheckpointAuthenticationViolation, CollisionReplayIndexViolation, InitializationMeasurement,
    LibraryInitCause, LibraryInitStage, LibrarySnapshotViolation,
};
use super::snapshot::SnapshotWorkScopeForTest;
use super::{FrozenLibraryBase, LibraryBaseProvider};
use crate::binder::declaration::DeclarationKind;
use crate::binder::declaration::TypeGroupId;
use crate::binder::namespace::{
    DeclarationOwner, NamespaceValueAttachmentDisposition, SourceFileKind,
};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::checker::events::UserEventReservationScopeForTest;
use crate::check::checker::library_compiler::{
    CanonicalLibraryFrontendWorkScopeForTest, UserDeltaForkScopeForTest, UserSourceWorkScopeForTest,
};
use crate::check::checker::replay_index::ReplayOwnerSite;
use crate::check::checker::AuthoritativeProjectBindingWorkScopeForTest;
use crate::check::query::QueryCacheWriteScopeForTest;
use crate::driver::FileInput;
use crate::relate::cache::RelationCacheWriteScopeForTest;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const REPLAY_MANIFEST_IDENTITY: &str =
    "cc125e22a561b069f62f6707e5eb3f8187be0959bb75d8cbfb665266d21c2c95";
const LEGACY_BINDER_V1_SCHEMA_IDENTITY: &str =
    "88fd84240ad5f574ddb1ee1bed1a631682d3ec15583882a5fbe4d9f9ca97e599";
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

fn decode_hex_32(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64);
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("hex digest");
    }
    bytes
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
    let wire_fields = [
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
    ];
    assert_eq!(
        exact_struct_field_names(source, "pub(crate) struct CollisionReplayIndex"),
        [
            wire_fields.as_slice(),
            &["canonical_manifest_bytes", "canonical_manifest_sha256",],
        ]
        .concat()
    );

    let admitted_fields =
        exact_struct_field_names(source, "pub(crate) struct AdmittedCollisionReplayIndex");
    assert_eq!(
        admitted_fields
            .iter()
            .filter(|field| wire_fields.contains(&field.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>(),
        wire_fields
    );
    let admitted_identity_fields = ["canonical_manifest_len", "canonical_manifest_sha256"];
    let required_compact_runtime_indexes = [
        "owner_to_scc",
        "scc_owner_ranges",
        "scc_owners",
        "reverse_scc_offsets",
        "reverse_scc_edges",
        "root_slot_lookup",
        "owner_site_ranges",
        "baseline_record_ranges",
    ];
    assert_eq!(
        admitted_fields
            .iter()
            .filter(|field| required_compact_runtime_indexes.contains(&field.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>(),
        required_compact_runtime_indexes
    );
    assert!(
        admitted_fields.iter().all(|field| {
            wire_fields.contains(&field.as_str())
                || admitted_identity_fields.contains(&field.as_str())
                || required_compact_runtime_indexes.contains(&field.as_str())
        }),
        "admission must retain only authenticated wire rows, identity evidence, and required compact runtime indexes: {admitted_fields:#?}"
    );
    assert_eq!(
        admitted_fields
            .iter()
            .filter(|field| admitted_identity_fields.contains(&field.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>(),
        admitted_identity_fields
    );
    let declaration = source
        .split_once("pub(crate) struct AdmittedCollisionReplayIndex")
        .expect("admitted replay index declaration")
        .1
        .split_once("\n}")
        .expect("admitted replay index body")
        .0;
    assert!(
        !declaration.contains("#[cfg(test)]"),
        "admitted runtime indexes must exist in production"
    );
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
fn binder_wire_v2_advances_the_top_level_schema_identity_and_rejects_v1() {
    const MAGIC: &[u8] = b"typokat-semantic-snapshot";
    const PROFILE_DIGEST_LEN: usize = 32;
    const SCHEMA_DIGEST_LEN: usize = 32;

    let packaged = super::artifact::packaged_canonical_snapshot();
    let schema_offset = MAGIC.len() + 4 + PROFILE_DIGEST_LEN;
    let schema_end = schema_offset + SCHEMA_DIGEST_LEN;
    let schema = hex(&packaged.bytes()[schema_offset..schema_end]);
    assert_ne!(
        schema, LEGACY_BINDER_V1_SCHEMA_IDENTITY,
        "adding retained binder maps changes the top-level wire authority"
    );

    for (label, authority) in [
        (
            "codec",
            include_str!("../check/checker/library_snapshot_codec/mod.rs"),
        ),
        ("provider", include_str!("snapshot.rs")),
        (
            "snapshot-tool",
            include_str!("../../tooling/full-lib-snapshot/contract.toml"),
        ),
        (
            "library-base-tool",
            include_str!("../../tooling/library-base/contract.toml"),
        ),
    ] {
        assert!(authority.contains(&schema), "{label} does not pin wire v2");
        assert!(
            !authority.contains(LEGACY_BINDER_V1_SCHEMA_IDENTITY),
            "{label} still admits the binder-v1 schema"
        );
    }

    let mut stale = packaged.bytes().to_vec();
    stale[schema_offset..schema_end]
        .copy_from_slice(&decode_hex_32(LEGACY_BINDER_V1_SCHEMA_IDENTITY));
    let error = match super::snapshot::admit_canonical_for_test(stale) {
        Ok(_) => panic!("the exact stale-v1 artifact must fail closed"),
        Err(error) => error,
    };
    assert_eq!(error.stage(), LibraryInitStage::ArtifactAdmission);
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

    fn assert_rejected_before_inspection_or_user_work(self) {
        let library = self.library.finish();
        let snapshot = self.snapshot.finish();
        let user = self.user.finish();
        let query = self.query.finish();
        assert_eq!(
            (
                library.compiles,
                library.parses,
                library.binds,
                library.checks
            ),
            (1, 82, 82, 0)
        );
        assert!(snapshot.decodes <= 1);
        assert!(snapshot.validations <= 1);
        assert_eq!((user.parses, user.binds, user.checks), (0, 0, 0));
        assert_eq!(self.delta.finish(), 0);
        assert_eq!(self.events.finish(), 0);
        assert_eq!((query.projection, query.evaluator), (0, 0));
        assert_eq!(self.relation.finish(), 0);
    }
}

fn checkpoint_inputs() -> [UserDeltaProjectInputForTest<'static>; 2] {
    [
        UserDeltaProjectInputForTest {
            path: "/project/00_augment.ts",
            source: "interface Array<T> { wu5Checkpoint(): T; }\n",
        },
        UserDeltaProjectInputForTest {
            path: "/project/99_consume.ts",
            source: "const value: number = [1, 2].wu5Checkpoint();\n",
        },
    ]
}

fn authoritative_project_inputs(reversed: bool) -> Vec<UserDeltaProjectInputForTest<'static>> {
    let mut inputs = vec![
        UserDeltaProjectInputForTest {
            path: "/project/nested/../00_exports.ts",
            source: r#"export class WU5ExportedClass {}
export function wu5ExportedFunction(): number { return 1; }
"#,
        },
        UserDeltaProjectInputForTest {
            path: "/project/10_imports.ts",
            source: r#"import { WU5ExportedClass, wu5ExportedFunction } from "./nested/../00_exports";
import { wu5Missing } from "./missing";
const instance: WU5ExportedClass = new WU5ExportedClass();
const importedValue: number = wu5ExportedFunction();
wu5Missing();
"#,
        },
        UserDeltaProjectInputForTest {
            path: "/project/20_ambient.d.ts",
            source: "declare namespace WU5Ambient { interface Entry {} }\n",
        },
        UserDeltaProjectInputForTest {
            path: "/project/25_kind_only_external.mts",
            source: "interface WU5MtsLocal { value: number }\n",
        },
        UserDeltaProjectInputForTest {
            path: "/project/30_script_use.ts",
            source: r#"const reservedBeforeDeclaration = WU5ScriptNamespace.value;
let mustNotSeeMtsLocal: WU5MtsLocal;
"#,
        },
        UserDeltaProjectInputForTest {
            path: "/project/40_script_declare.ts",
            source: r#"function WU5Function(): void {}
namespace WU5Function { export const attached = 1; }
class WU5Class {}
namespace WU5Class { export const attached = 2; }
namespace WU5Standalone { export const attached = 3; }
namespace WU5ScriptNamespace { export const value = 4; }
"#,
        },
    ];
    if reversed {
        inputs.reverse();
    }
    inputs
}

fn authoritative_driver_inputs(reversed: bool) -> Vec<FileInput> {
    authoritative_project_inputs(reversed)
        .into_iter()
        .map(|input| FileInput {
            name: input.path.to_owned(),
            source: input.source.to_owned(),
        })
        .collect()
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
fn production_frontend_products_feed_full_source_and_authenticated_project_routes() {
    let profile = ExactLibraryProfile::load_packaged().expect("pinned library profile");
    let compiler_work = LibraryCompilerWorkScopeForTest::start();
    let frontend_work = CanonicalLibraryFrontendWorkScopeForTest::start();
    let compiler = LibraryCompiler::new();

    let compiled = compiler
        .compile(&profile)
        .expect("production full-source compiler");
    let checkpoint = compiler
        .compile_binder_checkpoint(&profile)
        .expect("production opaque binder-checkpoint product");
    assert_eq!(checkpoint.library_unit_count(), 82);
    let provider = LibraryBaseProvider::new();
    let authenticated = provider
        .authenticate_library_binder_checkpoint(checkpoint)
        .expect("production checkpoint admission");
    assert_eq!(authenticated.library_unit_count(), 82);
    let continuation = provider
        .continue_authenticated_library_project_binder(
            authenticated,
            authoritative_driver_inputs(false),
        )
        .expect("production authenticated project continuation");
    let frontend_work = frontend_work.finish();
    let compiler_work = compiler_work.finish();

    assert_eq!(compiled.report().parse_units, 82);
    assert_eq!(compiled.report().bind_units, 82);
    assert_eq!(continuation.library_unit_count(), 82);
    assert_eq!(continuation.project_unit_count(), 6);
    assert_eq!(continuation.project_sources_for_test().len(), 6);
    assert_eq!(
        (
            compiler_work.compiles,
            compiler_work.parses,
            compiler_work.binds,
            compiler_work.checks
        ),
        (2, 164, 164, 82)
    );
    assert_eq!(frontend_work.entries, 2);
    assert_eq!(frontend_work.parse_units, 164);
    assert_eq!(frontend_work.bind_batches, 2);
    assert_eq!(frontend_work.bind_units, 164);
    assert_eq!(frontend_work.full_source_products_consumed, 1);
    assert_eq!(frontend_work.checkpoint_products_consumed, 1);
}

#[test]
fn initialized_provider_authenticates_a_checkpoint_without_a_second_snapshot_decode() {
    let provider = LibraryBaseProvider::new();
    let initialized = provider.get().expect("provider initializes once");
    let profile = ExactLibraryProfile::load_packaged().expect("pinned library profile");
    let checkpoint = LibraryCompiler::new()
        .compile_binder_checkpoint(&profile)
        .expect("production opaque binder-checkpoint product");

    let snapshot_work = SnapshotWorkScopeForTest::start();
    let authenticated = provider
        .authenticate_library_binder_checkpoint(checkpoint)
        .expect("initialized provider authenticates the checkpoint");
    let snapshot_work = snapshot_work.finish();

    assert_eq!(authenticated.library_unit_count(), 82);
    assert_eq!(
        (snapshot_work.validations, snapshot_work.decodes),
        (0, 0),
        "authentication must reuse the provider's admitted identity receipt"
    );
    assert!(Arc::ptr_eq(
        &initialized,
        &provider.get().expect("provider remains pointer-identical")
    ));
}

#[test]
fn ordinary_check_and_checkpoint_continuation_share_one_project_binding_core() {
    let profile = ExactLibraryProfile::load_packaged().expect("pinned library profile");
    let checkpoint = LibraryCompiler::new()
        .compile_binder_checkpoint(&profile)
        .expect("production opaque binder-checkpoint product");
    let provider = LibraryBaseProvider::new();
    let authenticated = provider
        .authenticate_library_binder_checkpoint(checkpoint)
        .expect("production checkpoint admission");

    let project_work = AuthoritativeProjectBindingWorkScopeForTest::start();
    let ordinary_reports = crate::driver::check_project(authoritative_driver_inputs(false));
    let continuation = provider
        .continue_authenticated_library_project_binder(
            authenticated,
            authoritative_driver_inputs(false),
        )
        .expect("authenticated project continuation");
    let project_work = project_work.finish();

    assert_eq!(ordinary_reports.len(), 6);
    assert!(ordinary_reports
        .iter()
        .all(|report| report.output.parse_errors.is_empty()));
    assert_eq!(project_work.entries, 2);
    assert_eq!(project_work.fresh_project_seed_entries, 1);
    assert_eq!(project_work.authenticated_checkpoint_seed_entries, 1);
    assert_eq!(project_work.bound_units, 12);
    assert_eq!(project_work.typed_products_produced, 2);
    assert_eq!(project_work.ordinary_check_products_consumed, 1);
    assert_eq!(project_work.continuation_route_products_consumed, 1);
    assert_eq!(project_work.fresh_project_products.len(), 1);
    assert_eq!(project_work.authenticated_checkpoint_products.len(), 1);
    assert_eq!(
        project_work.fresh_project_products[0].normalized_per_path_binding_shape,
        project_work.authenticated_checkpoint_products[0].normalized_per_path_binding_shape
    );
    assert_eq!(
        project_work.authenticated_checkpoint_products[0].normalized_per_path_binding_shape,
        continuation.normalized_per_path_binding_shape_for_test()
    );
    assert_eq!(
        project_work.fresh_project_products[0].normalized_import_export_shape,
        project_work.authenticated_checkpoint_products[0].normalized_import_export_shape
    );
    assert_eq!(
        project_work.authenticated_checkpoint_products[0].normalized_import_export_shape,
        continuation.normalized_import_export_shape_for_test()
    );
    assert_eq!(
        project_work.fresh_project_products[0].normalized_namespace_shape,
        project_work.authenticated_checkpoint_products[0].normalized_namespace_shape
    );
    assert_eq!(
        project_work.authenticated_checkpoint_products[0].normalized_namespace_shape,
        continuation.normalized_namespace_shape_for_test()
    );
}

#[test]
fn authoritative_multifile_project_binding_continues_from_the_checkpoint_in_path_order() {
    let bind = |reversed| {
        LibraryBaseProvider::new()
            .continue_authenticated_library_binder_checkpoint_for_test(
                &authoritative_project_inputs(reversed),
                None,
                |_, _, _| {},
            )
            .expect("authoritative project binder continuation")
    };
    let forward = bind(false);
    let reverse = bind(true);

    let expected_paths = [
        "/project/00_exports.ts",
        "/project/10_imports.ts",
        "/project/20_ambient.d.ts",
        "/project/25_kind_only_external.mts",
        "/project/30_script_use.ts",
        "/project/40_script_declare.ts",
    ];
    assert_eq!(
        forward.normalized_per_path_binding_shape_for_test(),
        reverse.normalized_per_path_binding_shape_for_test(),
        "opposite caller order preserves per-path binding shape without requiring equal numeric identities"
    );

    for (continuation, reversed) in [(&forward, false), (&reverse, true)] {
        let rows = continuation.project_sources_for_test();
        assert_eq!(
            rows.iter()
                .map(|row| row.normalized_path.as_str())
                .collect::<Vec<_>>(),
            expected_paths
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.source_file_kind)
                .collect::<Vec<_>>(),
            [
                SourceFileKind::ImplementationTs,
                SourceFileKind::ImplementationTs,
                SourceFileKind::DeclarationTs,
                SourceFileKind::ImplementationMts,
                SourceFileKind::ImplementationTs,
                SourceFileKind::ImplementationTs,
            ]
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.external_module)
                .collect::<Vec<_>>(),
            [true, true, false, true, false, false]
        );
        let expected_original_ordinals = if reversed {
            [5, 4, 3, 2, 1, 0]
        } else {
            [0, 1, 2, 3, 4, 5]
        };
        let expected_unit_slots = if reversed {
            [4, 5, 3, 2, 1, 0]
        } else {
            [0, 1, 2, 3, 4, 5]
        };
        assert_eq!(
            rows.iter()
                .map(|row| row.original_module_ordinal.index())
                .collect::<Vec<_>>(),
            expected_original_ordinals
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.unit_slot.index())
                .collect::<Vec<_>>(),
            expected_unit_slots
        );
        assert_dense(
            rows.iter()
                .map(|row| usize::try_from(row.source.0).expect("source key fits usize")),
            continuation.checkpoint_ends.next_source,
            continuation.ends.next_source,
        );
        assert!(rows
            .iter()
            .all(|row| continuation.appended_scopes.contains(&row.module)));

        let exported_class = continuation
            .lookup_binding_for_test("/project/00_exports.ts", "WU5ExportedClass")
            .expect("exported class binding");
        let imported_class = continuation
            .lookup_binding_for_test("/project/10_imports.ts", "WU5ExportedClass")
            .expect("imported class binding");
        assert_ne!(exported_class.symbol, imported_class.symbol);
        assert_eq!(exported_class.value, imported_class.value);
        assert_eq!(exported_class.type_group, imported_class.type_group);
        assert!(exported_class.value.is_some());
        assert!(exported_class.type_group.is_some());

        let exported_function = continuation
            .lookup_binding_for_test("/project/00_exports.ts", "wu5ExportedFunction")
            .expect("exported function binding");
        let imported_function = continuation
            .lookup_binding_for_test("/project/10_imports.ts", "wu5ExportedFunction")
            .expect("imported function binding");
        assert_ne!(exported_function.symbol, imported_function.symbol);
        assert_eq!(exported_function.value, imported_function.value);
        assert!(exported_function.value.is_some());

        let missing = continuation
            .lookup_binding_for_test("/project/10_imports.ts", "wu5Missing")
            .expect("missing import placeholder binding");
        assert!(missing.value.is_some());
        assert!(missing.blocks_type_lookup);
        assert_eq!(
            continuation.import_placeholders_for_test("/project/10_imports.ts"),
            missing.value.into_iter().collect::<Vec<_>>()
        );

        let mts_local = continuation
            .lookup_binding_for_test("/project/25_kind_only_external.mts", "WU5MtsLocal")
            .expect("kind-only external module local");
        assert!(mts_local.type_group.is_some());
        assert!(
            continuation
                .lookup_binding_for_test("/project/30_script_use.ts", "WU5MtsLocal")
                .is_none(),
            "the .mts-only external classification keeps its declaration module-local"
        );

        let reserved = continuation
            .script_namespace_root_reservation_for_test("WU5ScriptNamespace")
            .expect("script namespace root reservation");
        for path in ["/project/30_script_use.ts", "/project/40_script_declare.ts"] {
            let binding = continuation
                .lookup_binding_for_test(path, "WU5ScriptNamespace")
                .expect("reserved script namespace lookup");
            assert_eq!(binding.symbol, reserved);
            assert!(binding.namespace.is_some());
            assert!(binding.value.is_some());
        }
        let script_namespace_storage = continuation
            .standalone_namespace_value_storage_for_test(
                "/project/40_script_declare.ts",
                "WU5ScriptNamespace",
            )
            .expect("reserved script namespace value storage");
        assert_eq!(
            continuation
                .lookup_binding_for_test("/project/30_script_use.ts", "WU5ScriptNamespace")
                .and_then(|binding| binding.value),
            Some(script_namespace_storage)
        );

        for (name, disposition) in [
            (
                "WU5Function",
                NamespaceValueAttachmentDisposition::AdmittedFunction,
            ),
            (
                "WU5Class",
                NamespaceValueAttachmentDisposition::AdmittedClass,
            ),
        ] {
            let binding = continuation
                .lookup_binding_for_test("/project/40_script_declare.ts", name)
                .expect("script namespace value binding");
            assert_eq!(
                continuation.attached_namespace_value_disposition_for_test(
                    "/project/40_script_declare.ts",
                    name
                ),
                Some(disposition),
                "{name}"
            );
            assert!(binding.value.is_some(), "{name}");
            assert!(binding.namespace.is_some(), "{name}");
        }
        let standalone = continuation
            .lookup_binding_for_test("/project/40_script_declare.ts", "WU5Standalone")
            .expect("standalone namespace binding");
        let standalone_storage = continuation
            .standalone_namespace_value_storage_for_test(
                "/project/40_script_declare.ts",
                "WU5Standalone",
            )
            .expect("standalone namespace value storage");
        assert_eq!(standalone.value, Some(standalone_storage));
        assert!(standalone.namespace.is_some());
        assert!(
            continuation
                .lookup_binding_for_test("/project/20_ambient.d.ts", "WU5Ambient")
                .is_some_and(|binding| binding.namespace.is_some()),
            "declaration scripts participate in the shared script namespace root"
        );
    }
}

#[test]
fn colliding_declarations_keep_their_syntax_module_as_the_lexical_parent() {
    const PATH: &str = "/project/colliding-lexical-siblings.ts";
    const SOURCE: &str = r#"interface WU5LocalType { marker: number; }
declare const WU5LocalValue: WU5LocalType;
declare class WU5LocalBase { base: WU5LocalType; }
interface Array<T> extends WU5LocalType { local: WU5LocalType; }
declare function parseInt(value: WU5LocalType): WU5LocalType;
declare class AddEventListenerOptions extends WU5LocalBase { local: WU5LocalType; }
declare namespace Intl {
  const local: typeof WU5LocalValue;
  interface UsesLocal extends WU5LocalType {}
}
"#;
    let continuation = LibraryBaseProvider::new()
        .continue_authenticated_library_binder_checkpoint_for_test(
            &[UserDeltaProjectInputForTest {
                path: PATH,
                source: SOURCE,
            }],
            None,
            |_, _, _| {},
        )
        .expect("colliding declarations retain a usable lexical scope");
    let source_row = continuation
        .project_sources_for_test()
        .iter()
        .find(|row| row.normalized_path == PATH)
        .expect("continued source row");
    let module = source_row.module;
    let binder = &continuation.bound.binder;

    let declarations = [
        ("Array", "Array<T>", DeclarationKind::Interface),
        ("parseInt", "parseInt(value", DeclarationKind::Function),
        (
            "AddEventListenerOptions",
            "AddEventListenerOptions extends",
            DeclarationKind::Class,
        ),
        ("Intl", "Intl {", DeclarationKind::Namespace),
    ]
    .map(|(name, needle, kind)| {
        let start = u32::try_from(SOURCE.find(needle).expect("binding needle"))
            .expect("binding offset fits u32");
        let declaration = binder
            .exact_declaration_at(module, start, kind)
            .unwrap_or_else(|| panic!("exact {name} declaration"));
        assert_eq!(
            declaration.site.scope,
            Some(module),
            "{name} must resolve annotations and initializers through its syntax module"
        );
        let merge = binder
            .namespaces
            .merges()
            .find(|record| {
                record.owner == DeclarationOwner::CompilationGlobal && record.name == name
            })
            .unwrap_or_else(|| panic!("compilation-global {name} merge"));
        assert!(
            merge
                .declarations
                .iter()
                .any(|participant| participant.declaration == declaration.id),
            "{name} must publish the same lexical declaration into the global merge"
        );
        declaration.id
    });

    let array = continuation
        .lookup_binding_for_test(PATH, "Array")
        .expect("continued Array binding");
    assert_eq!(
        array.type_group,
        Some(continuation.array_type_group_before_augmentation),
        "the lexical Array fragment must extend the authenticated global identity"
    );

    let function_start = u32::try_from(
        SOURCE
            .find("declare function parseInt")
            .expect("function start"),
    )
    .expect("function offset fits u32");
    let function_scope = binder
        .fn_scopes
        .get(&(module, function_start))
        .copied()
        .expect("continued function scope");
    assert_eq!(
        binder
            .graph
            .get(function_scope)
            .and_then(|scope| scope.parent),
        Some(module),
        "the colliding function body must retain module-local value lookup"
    );

    let namespace_merge = binder
        .namespaces
        .merges()
        .find(|record| record.owner == DeclarationOwner::CompilationGlobal && record.name == "Intl")
        .expect("Intl compilation-global merge");
    let fragment = namespace_merge
        .declarations
        .iter()
        .find(|participant| participant.declaration == declarations[3])
        .and_then(|participant| participant.namespace_fragment)
        .and_then(|fragment| binder.namespaces.fragment(fragment))
        .expect("continued Intl fragment");
    assert_eq!(
        fragment.lexical_parent, module,
        "the global Intl identity must not replace its fragment's lexical parent"
    );
}

#[test]
fn nested_hoisted_collision_placements_match_the_preflight_binding_leaves() {
    const PATH: &str = "/project/nested-hoisted-collisions.ts";
    const SOURCE: &str = r#"declare const condition: boolean;
declare const values: unknown[];
declare const source: any;
{ var RegExp = 1; }
if (condition) { var Promise = 1; } else { var document = 1; }
switch (values.length) { case 0: var Map = 1; }
while (condition) { var Set = 1; break; }
do { var WeakMap = 1; } while (false);
for (var Date = 0; Date < 1; Date++) {}
for (var String in { value: 1 }) {}
for (var Number of values) {}
WU5Label: { var Boolean = 1; break WU5Label; }
try { var Error = 1; } catch { var Function = 1; } finally { var Object = 1; }
if (condition) {
  var { document: navigator, nested: [globalThis], ...Intl } = source;
}
"#;
    let inputs = [UserDeltaProjectInputForTest {
        path: PATH,
        source: SOURCE,
    }];
    let preflight = acquire()
        .preflight_user_project_for_test(&inputs)
        .expect("preflight census");
    let continuation = LibraryBaseProvider::new()
        .continue_authenticated_library_binder_checkpoint_for_test(&inputs, None, |_, _, _| {})
        .expect("nested collision continuation");
    let module = continuation.project_sources_for_test()[0].module;
    let binder = &continuation.bound.binder;
    let expected = [
        "RegExp",
        "Promise",
        "document",
        "Map",
        "Set",
        "WeakMap",
        "Date",
        "String",
        "Number",
        "Boolean",
        "Error",
        "Function",
        "Object",
        "navigator",
        "globalThis",
        "Intl",
    ];

    for name in expected {
        assert!(
            preflight
                .candidates
                .iter()
                .any(|candidate| { candidate.name == name && candidate.global_object_contributor }),
            "preflight omitted the hoisted binding leaf {name}"
        );
        let start = u32::try_from(SOURCE.find(name).expect("unique binding leaf"))
            .expect("binding offset fits u32");
        let declaration = binder
            .exact_declaration_at(module, start, DeclarationKind::Variable)
            .unwrap_or_else(|| panic!("exact hoisted declaration {name}"));
        let merge = binder
            .namespaces
            .merges()
            .find(|record| {
                record.owner == DeclarationOwner::CompilationGlobal && record.name == name
            })
            .unwrap_or_else(|| panic!("compilation-global placement for {name}"));
        assert!(
            merge
                .declarations
                .iter()
                .any(|participant| participant.declaration == declaration.id),
            "preflight leaf {name} did not become the exact global merge participant"
        );
    }
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
fn retained_function_and_block_scope_map_corruption_fails_before_user_work() {
    for (mutation, violation) in [
        (
            CheckpointAuthenticationMutationForTest::FunctionScopes,
            CheckpointAuthenticationViolation::RetainedScopeMapsMismatch,
        ),
        (
            CheckpointAuthenticationMutationForTest::FunctionDeclarationIds,
            CheckpointAuthenticationViolation::RetainedScopeMapsMismatch,
        ),
        (
            CheckpointAuthenticationMutationForTest::BlockScopes,
            CheckpointAuthenticationViolation::RetainedScopeMapsMismatch,
        ),
    ] {
        let scopes = CheckpointBoundaryScopes::start();
        let inspection_reached = Cell::new(false);
        let error = LibraryBaseProvider::new()
            .continue_authenticated_library_binder_checkpoint_for_test(
                &authoritative_project_inputs(false),
                Some(mutation),
                |_, _, _| inspection_reached.set(true),
            )
            .expect_err("retained scope-map corruption must fail closed");
        assert!(!inspection_reached.get(), "{mutation:?}");
        assert_eq!(error.stage(), LibraryInitStage::ReferenceValidation);
        assert_eq!(
            error.cause(),
            &LibraryInitCause::CheckpointAuthenticationRejected { violation }
        );
        scopes.assert_rejected_before_inspection_or_user_work();
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
        .split_once("fn continue_authenticated_library_project_binder(")
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

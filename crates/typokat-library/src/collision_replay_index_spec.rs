//! RED contracts for WU5 authenticated replay admission and binder continuation.

use super::base::{deferred_packaged_replay_index_for_test, UserDeltaProjectInputForTest};
use super::compiler::{LibraryCompiler, LibraryCompilerWorkScopeForTest, COMPILER_SCHEMA_SHA256};
use super::profile::ExactLibraryProfile;
use super::provider::{shared_library_base_provider_for_test, InitializationMeasurement};
use super::FrozenLibraryBase;
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
use crate::frontend::FileInput;
use crate::relate::cache::RelationCacheWriteScopeForTest;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const REPLAY_MANIFEST_IDENTITY: &str =
    "6460d58af3031dc687f714361ce9a6fe7f25ee05389afe8a16b781b67654f3d6";
/// The process-wide source-compiled base. Compiling all 82 packaged sources costs seconds, so
/// every spec in this binary shares one.
fn acquire() -> Arc<FrozenLibraryBase> {
    shared_library_base_provider_for_test()
        .get()
        .expect("source-compiled default library base")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn binder_source(relative: &str) -> String {
    std::fs::read_to_string(
        crate::test_support::repository_root()
            .join("crates/typokat-binder/src/binder")
            .join(relative),
    )
    .expect("binder source")
}

fn checker_source(relative: &str) -> String {
    std::fs::read_to_string(
        crate::test_support::repository_root()
            .join("crates/typokat-check/src/check/checker")
            .join(relative),
    )
    .expect("checker source")
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
                    .or_else(|| name.trim().strip_prefix("pub "))
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
    let source = checker_source("replay_index.rs");
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
        exact_struct_field_names(&source, "pub struct CollisionReplayIndex"),
        [
            wire_fields.as_slice(),
            &["canonical_manifest_bytes", "canonical_manifest_sha256",],
        ]
        .concat()
    );

    let admitted_fields =
        exact_struct_field_names(&source, "pub struct AdmittedCollisionReplayIndex");
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
        .split_once("pub struct AdmittedCollisionReplayIndex")
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
    let source = checker_source("replay_index.rs");
    let declaration = source
        .split_once("pub struct ReplayScc")
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
        source.contains("pub scc_membership: Vec<ReplayScc>,"),
        "inline owner storage must preserve the ordered retained SCC row model"
    );
}

#[test]
fn source_compiled_replay_index_matches_its_pinned_shape_and_identity() {
    let base = acquire();
    let index = deferred_packaged_replay_index_for_test();

    assert_eq!(index.schema, 1);
    assert_eq!(base.identity().profile_sha256(), PROFILE_IDENTITY);
    assert_eq!(base.identity().schema_sha256(), COMPILER_SCHEMA_SHA256);
    assert_eq!(index.canonical_manifest_len(), 10_991_777);
    // The regression canary for unattributed library-diagnostic drift: the manifest digests
    // every owner's rendered records, so a changed diagnostic moves it.
    assert_eq!(
        hex(&index.canonical_manifest_sha256),
        REPLAY_MANIFEST_IDENTITY
    );
    assert_eq!(index.owner_partition.len(), 45_925);
    assert_eq!(index.root_slots.len(), 2_238);
    assert_eq!(index.owner_sites.len(), 47_253);
    assert_eq!(index.reverse_edges.len(), 9_780);
    assert_eq!(index.root_slot_consumers.len(), 6_940);
    assert_eq!(index.scc_membership.len(), 45_236);
    assert_eq!(index.statement_owners.len(), 42_496);
    assert_eq!(index.baseline_records.len(), 45_925);
    assert_eq!(index.unowned_demand_count, 0);
    assert_eq!(index.invalid_owner_site_count, 0);
    assert_eq!(index.noncanonical_edge_count, 0);
    assert_eq!(index.typed_reference_coverage_misses, 0);
}

#[test]
fn canonical_replay_index_is_source_reproducible_and_complete() {
    let index = deferred_packaged_replay_index_for_test();
    let regenerated = FrozenLibraryBase::regenerate_replay_index_manifest_for_test()
        .expect("source compiler independently regenerates the replay index");

    assert_eq!(
        index.canonical_manifest_sha256,
        regenerated.canonical_manifest_sha256
    );
    assert_eq!(regenerated.library_source_compiles, 1);
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
            user: UserSourceWorkScopeForTest::start(),
            delta: UserDeltaForkScopeForTest::start(),
            events: UserEventReservationScopeForTest::start(),
            query: QueryCacheWriteScopeForTest::start(),
            relation: RelationCacheWriteScopeForTest::start(),
        }
    }

    fn assert_exact_library_only_frontend(self) {
        let library = self.library.finish();
        let user = self.user.finish();
        let query = self.query.finish();
        assert_eq!(
            (library.compiles, library.parses, library.binds),
            (1, 82, 82)
        );
        assert_eq!(library.checks, 0);
        assert_eq!((user.binds, user.checks), (0, 0));
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
fn exact_library_only_binder_checkpoint_precedes_every_user_continuation() {
    let scopes = CheckpointBoundaryScopes::start();
    let inspection_reached = Cell::new(false);
    let checkpoint_array_symbol = Cell::new(None::<SymbolId>);
    let checkpoint_array_type_group = Cell::new(None::<TypeGroupId>);
    let inspected_library_modules = RefCell::new(Vec::<ScopeId>::new());
    let base_owner_sites = RefCell::new(Vec::<ReplayOwnerSite>::new());
    let continuation = shared_library_base_provider_for_test()
        .continue_library_binder_checkpoint_for_test(
            &checkpoint_inputs(),
            |checkpoint, base_replay| {
                inspection_reached.set(true);
                scopes.assert_exact_library_only_frontend();
                assert_eq!(checkpoint.library_units.len(), 82);
                for (index, unit) in checkpoint.library_units.iter().enumerate() {
                    assert_eq!(unit.ordinal.index(), index);
                    assert_eq!(
                        usize::try_from(unit.source.0).expect("source key fits usize"),
                        index + 1
                    );
                }
                assert!(checkpoint.array_symbol.index() < checkpoint.ends.symbols);
                assert!(checkpoint.array_type_group.index() < checkpoint.ends.type_groups);
                checkpoint_array_symbol.set(Some(checkpoint.array_symbol));
                checkpoint_array_type_group.set(Some(checkpoint.array_type_group));
                assert_eq!(base_replay.owner_sites.len(), 47_253);
                inspected_library_modules.replace(
                    checkpoint
                        .library_units
                        .iter()
                        .map(|unit| unit.module)
                        .collect(),
                );
                base_owner_sites.replace(base_replay.owner_sites.clone());
            },
        )
        .expect("the library binder checkpoint continues through the user files");

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
    let base_owner_sites = base_owner_sites.borrow();
    assert_eq!(
        continuation.mapped_owner_sites.len(),
        base_owner_sites.len()
    );
    for (mapped, site) in continuation
        .mapped_owner_sites
        .iter()
        .zip(base_owner_sites.iter())
    {
        assert_eq!(mapped.owner, site.owner);
        assert_eq!(mapped.file_ordinal, site.file_ordinal);
        assert_eq!(mapped.span, site.span);
        assert_eq!(
            mapped.syntax_module,
            inspected_library_modules[site.file_ordinal.index()]
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
fn production_frontend_products_feed_full_source_and_checkpoint_project_routes() {
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
    let continuation = shared_library_base_provider_for_test()
        .continue_library_project_binder(checkpoint, authoritative_driver_inputs(false))
        .expect("production project continuation from the compiled checkpoint");
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
fn the_provider_publishes_one_pointer_identical_base_per_instance() {
    let provider = shared_library_base_provider_for_test();
    let first = provider.get().expect("provider initializes once");
    let second = provider.get().expect("provider remains pointer-identical");

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(
        provider.measurement_for_test(),
        InitializationMeasurement {
            attempts: 1,
            publications: 1,
        }
    );
}

#[test]
fn ordinary_check_and_checkpoint_continuation_share_one_project_binding_core() {
    let profile = ExactLibraryProfile::load_packaged().expect("pinned library profile");
    let checkpoint = LibraryCompiler::new()
        .compile_binder_checkpoint(&profile)
        .expect("production opaque binder-checkpoint product");
    let project_work = AuthoritativeProjectBindingWorkScopeForTest::start();
    let ordinary_reports =
        crate::check::test_support::check_project(authoritative_driver_inputs(false));
    let continuation = shared_library_base_provider_for_test()
        .continue_library_project_binder(checkpoint, authoritative_driver_inputs(false))
        .expect("checkpoint project continuation");
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
        shared_library_base_provider_for_test()
            .continue_library_binder_checkpoint_for_test(
                &authoritative_project_inputs(reversed),
                |_, _| {},
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
    let continuation = shared_library_base_provider_for_test()
        .continue_library_binder_checkpoint_for_test(
            &[UserDeltaProjectInputForTest {
                path: PATH,
                source: SOURCE,
            }],
            |_, _| {},
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
                record.owner == DeclarationOwner::CompilationGlobal && record.name.as_ref() == name
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
        .find(|record| {
            record.owner == DeclarationOwner::CompilationGlobal && record.name.as_ref() == "Intl"
        })
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
    let continuation = shared_library_base_provider_for_test()
        .continue_library_binder_checkpoint_for_test(&inputs, |_, _| {})
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
                record.owner == DeclarationOwner::CompilationGlobal && record.name.as_ref() == name
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
fn library_binder_checkpoint_is_unforgeable_and_resumes_only_through_the_continuation() {
    let binder = binder_source("bind.rs");
    let private_compiler = checker_source("library_compiler.rs");
    let library_compiler = include_str!("compiler.rs");
    let checkpoint_offset = binder
        .find("pub struct LibraryBinderCheckpoint {")
        .expect("opaque library binder checkpoint");
    // Attributes and doc lines attached to the declaration, back to the previous item.
    let attributes = binder[..checkpoint_offset]
        .rsplit_once("\n}\n")
        .map_or(&binder[..checkpoint_offset], |(_, attached)| attached);
    let body = binder
        .split_once("pub struct LibraryBinderCheckpoint {")
        .expect("opaque library binder checkpoint")
        .1
        .split_once("\n}")
        .expect("checkpoint body")
        .0;
    let checkpoint_impl = binder
        .split_once("impl LibraryBinderCheckpoint {")
        .expect("checkpoint impl")
        .1
        .split_once("\n}\n")
        .expect("checkpoint impl body")
        .0;
    // Provenance is structural: private fields, no Clone, and one boundary constructor.
    assert!(!body.contains("pub"));
    assert!(!attributes.contains("derive"));
    assert!(!binder.contains("AuthenticatedLibraryBinderCheckpoint"));
    assert!(checkpoint_impl.contains("pub fn new("));
    assert_eq!(
        private_compiler
            .matches("LibraryBinderCheckpoint::new(")
            .count(),
        1,
        "only the source compiler may build a checkpoint"
    );
    assert!(library_compiler.contains("compile_binder_checkpoint"));
    let continuation = private_compiler
        .split_once("fn continue_library_project_binder(")
        .expect("checkpoint continuation entrypoint")
        .1
        .split_once("\n}")
        .expect("checkpoint continuation body")
        .0;
    assert!(continuation.contains("checkpoint: LibraryBinderCheckpoint"));
    assert!(!continuation.contains("resume_frozen_library("));
}

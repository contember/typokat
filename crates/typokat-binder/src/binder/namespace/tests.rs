use super::layered::assert_idempotent_overlay_publication;
use super::*;
use crate::binder::bind::{
    Binder, ImportedSymbol, LibraryBinderCheckpoint, LibraryBinderUnit, ProjectBinderBuilder,
};
use crate::binder::declaration::{DeclarationKind, TypeFragmentKind, ValueStorageId};
use crate::binder::scope::{ScopeId, ScopeKind};
use crate::binder::symbol::{Symbol, SymbolId};
use crate::source::{CompilationOrigin, LibraryFileOrdinal, OriginalModuleOrdinal};
use crate::span::Span;
use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::Parser;
use oxc_span::SourceType;

#[test]
fn compilation_origin_index_preserves_layers_and_rejects_invalid_republication() {
    let inherited_source = SourceUnitKey(700_001);
    let inherited_origin = CompilationOrigin::Library(LibraryFileOrdinal::new(17));
    let mut base = NamespaceTable::default();
    base.record_compilation_origin(inherited_source, inherited_origin);
    base.freeze_as_base().expect("namespace origin base seals");

    let mut delta = base.fork_delta().expect("ordinary namespace delta forks");
    assert_eq!(
        delta.compilation_origin_for_source(inherited_source),
        Some(inherited_origin)
    );
    let fresh_source = SourceUnitKey(900_001);
    let fresh_origin = CompilationOrigin::User(OriginalModuleOrdinal::new(23));
    delta.record_compilation_origin(fresh_source, fresh_origin);
    assert_eq!(
        delta.compilation_origin_for_source(fresh_source),
        Some(fresh_origin)
    );
    assert_eq!(
        delta.compilation_origin_for_source(SourceUnitKey(u32::MAX)),
        None
    );
    delta.record_compilation_origin(inherited_source, inherited_origin);
    assert_eq!(
        delta.validate_compilation_origin_index(),
        Err(CompilationOriginIndexConflict::SealedPrefixWrite {
            source: inherited_source,
            attempted: inherited_origin,
        })
    );

    let mut sparse = base
        .fork_sparse_delta()
        .expect("sparse namespace delta forks");
    sparse.record_compilation_origin(inherited_source, inherited_origin);
    assert_eq!(sparse.validate_compilation_origin_index(), Ok(()));
    assert_eq!(
        sparse.local_family_row_counts_for_test(),
        [0, 1],
        "the sparse same-key override is epoch-owned index work"
    );
    assert_eq!(
        sparse.compilation_origin_for_source(inherited_source),
        Some(inherited_origin)
    );

    let attempted = CompilationOrigin::Library(LibraryFileOrdinal::new(18));
    sparse.record_compilation_origin(inherited_source, attempted);
    assert_eq!(
        sparse.validate_compilation_origin_index(),
        Err(CompilationOriginIndexConflict::ConflictingOrigin {
            source: inherited_source,
            retained: inherited_origin,
            attempted,
        })
    );
    assert_eq!(
        sparse.compilation_origin_for_source(inherited_source),
        Some(inherited_origin),
        "a rejected origin cannot replace the retained exact owner"
    );

    let mut conflicted_base = NamespaceTable::default();
    conflicted_base.record_compilation_origin(inherited_source, inherited_origin);
    conflicted_base.record_compilation_origin(inherited_source, attempted);
    assert!(conflicted_base.freeze_as_base().is_err());
    assert_eq!(
        conflicted_base.compilation_origin_for_source(inherited_source),
        Some(inherited_origin),
        "failed publication retains the first exact owner"
    );
}

#[test]
fn frozen_global_overlay_republication_accepts_only_the_same_symbol() {
    assert_idempotent_overlay_publication(None, SymbolId(7), "first publication is admitted");
    assert_idempotent_overlay_publication(
        Some(SymbolId(7)),
        SymbolId(7),
        "same-id continuation is idempotent",
    );
    let conflict = std::panic::catch_unwind(|| {
        assert_idempotent_overlay_publication(
            Some(SymbolId(8)),
            SymbolId(7),
            "different-id continuation is rejected",
        );
    });
    assert!(conflict.is_err());
}

#[test]
fn namespace_base_sharing_witness_covers_every_layered_field() {
    let source = include_str!("layered.rs");
    let fields = source
        .split_once("pub struct NamespaceTable {")
        .and_then(|(_, rest)| rest.split_once("\n}"))
        .map(|(body, _)| body)
        .expect("NamespaceTable declaration");
    let layered_fields = fields
        .lines()
        .filter(|line| line.contains("LayeredVec<") || line.contains("LayeredMap<"))
        .filter_map(|line| {
            let (name, _) = line.trim().split_once(':')?;
            Some(name.strip_prefix("pub(super) ").unwrap_or(name))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        layered_fields.len(),
        35,
        "layered namespace field inventory"
    );

    let witness = source
        .split_once("pub fn shares_base_storage_with(&self, other: &Self) -> bool {")
        .and_then(|(_, rest)| rest.split_once("\n    }"))
        .map(|(body, _)| body)
        .expect("namespace base-sharing witness")
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    for field in layered_fields {
        let required = format!("self.{field}.shares_base_with(&other.{field})");
        assert!(
            witness.contains(&required),
            "missing sharing witness for {field}"
        );
    }
}

#[test]
fn library_compilation_unit_retains_declaration_context_and_origin() {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "interface LibraryShape {}", SourceType::ts()).parse();
    assert!(!parsed.panicked);

    let file_ordinal = LibraryFileOrdinal::new(17);
    let unit = CompilationUnit::library(SourceUnitKey(82), file_ordinal, &parsed.program);
    assert_eq!(unit.origin, CompilationOrigin::Library(file_ordinal));
    assert!(unit.binding.declaration_file());
}

#[test]
fn statement_lexical_root_uses_sparse_source_keys_in_frozen_continuation(
) -> Result<(), &'static str> {
    let prelude_allocator = Allocator::default();
    let library_allocator = Allocator::default();
    let user_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let library = Parser::new(
        &library_allocator,
        "interface FrozenSparseMarker {}",
        SourceType::d_ts(),
    )
    .parse();
    let user = Parser::new(&user_allocator, "const local = 1;", SourceType::ts()).parse();
    assert!(!prelude.panicked && !library.panicked && !user.panicked);

    let mut base_builder = ProjectBinderBuilder::new(&prelude.program);
    let library_unit = CompilationUnit::library(
        SourceUnitKey(899),
        LibraryFileOrdinal::new(0),
        &library.program,
    );
    let library_modules = base_builder.add_library_modules(&[(&library.program, library_unit)]);
    let library_module = library_modules
        .first()
        .copied()
        .ok_or("library binder retains its module")?;
    let mut base = base_builder.finish(library_module);
    base.freeze_as_base()?;

    let delta = base.fork_sparse_collision_delta()?;
    let (mut builder, _) = ProjectBinderBuilder::resume_frozen_library(delta);
    let user_unit = CompilationUnit::implementation(SourceUnitKey(900), &user.program);
    builder.reserve_script_namespace_roots([(&user.program, user_unit)]);
    let (module, _) = builder.add_module(&user.program, &[], user_unit);
    let binder = builder.finish_frozen_library_continuation(Some(module))?;
    let expected = binder
        .graph
        .get(binder.script_namespace_root)
        .and_then(|scope| scope.parent)
        .ok_or("script namespace root retains its compilation parent")?;

    assert_eq!(binder.statement_lexical_root(module), expected);
    Ok(())
}

#[test]
fn direct_script_global_this_conflict_survives_frozen_library_continuation(
) -> Result<(), &'static str> {
    let prelude_allocator = Allocator::default();
    let library_allocator = Allocator::default();
    let user_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let library = Parser::new(
        &library_allocator,
        "declare var ExistingLibraryGlobal: number;",
        SourceType::d_ts(),
    )
    .parse();
    let source = "declare namespace globalThis { let rejected: number; }";
    let user = Parser::new(&user_allocator, source, SourceType::d_ts()).parse();
    assert!(!library.panicked && !user.panicked);

    let mut base_builder = ProjectBinderBuilder::new(&prelude.program);
    let library_unit = CompilationUnit::library(
        SourceUnitKey(1),
        LibraryFileOrdinal::new(0),
        &library.program,
    );
    let library_modules = base_builder.add_library_modules(&[(&library.program, library_unit)]);
    assert_eq!(library_modules.len(), 1);
    let mut base = base_builder.finish(library_modules[0]);
    assert!(base.freeze_as_base().is_ok());

    let delta = base.fork_sparse_collision_delta()?;
    let (mut builder, source_key) = ProjectBinderBuilder::resume_frozen_library(delta);
    let user_unit = CompilationUnit::implementation(source_key, &user.program);
    builder.reserve_script_namespace_roots([(&user.program, user_unit)]);
    let (module, _) = builder.add_module(&user.program, &[], user_unit);
    let binder = builder.finish_frozen_library_continuation(Some(module))?;
    let binding_start = source
        .find("globalThis")
        .ok_or("fixture contains the globalThis binding")?;
    let binding_start = u32::try_from(binding_start).map_err(|_| "binding offset fits u32")?;
    assert!(binder.direct_script_global_this_conflict(
        module,
        binding_start,
        DeclarationKind::Namespace,
    ));
    assert_eq!(
        binder.resolve_value_binding(module, "globalThis"),
        crate::binder::bind::ValueResolution::Missing
    );

    let mut checkpoint_builder = ProjectBinderBuilder::new(&prelude.program);
    let checkpoint_modules =
        checkpoint_builder.add_library_modules(&[(&library.program, library_unit)]);
    let checkpoint_module = checkpoint_modules
        .first()
        .copied()
        .ok_or("library checkpoint contains its module")?;
    let checkpoint_binder = checkpoint_builder.finish(checkpoint_module);
    let checkpoint = LibraryBinderCheckpoint::new(
        checkpoint_binder,
        vec![LibraryBinderUnit {
            ordinal: LibraryFileOrdinal::new(0),
            source: SourceUnitKey(1),
            module: checkpoint_module,
        }],
    );
    let (mut builder, _) = checkpoint.into_continuation();
    let user_unit = CompilationUnit::implementation(SourceUnitKey(2), &user.program);
    builder.reserve_script_namespace_roots([(&user.program, user_unit)]);
    let (module, _) = builder.add_module(&user.program, &[], user_unit);
    let binder = builder.finish_frozen_library_continuation(Some(module))?;
    assert!(binder.direct_script_global_this_conflict(
        module,
        binding_start,
        DeclarationKind::Namespace,
    ));
    assert_eq!(
        binder.resolve_value_binding(module, "globalThis"),
        crate::binder::bind::ValueResolution::Missing
    );
    Ok(())
}

fn bind(source: &str, declaration_file: bool) -> Binder {
    bind_unit(
        source,
        declaration_file,
        SourceUnitKey::SINGLE_SOURCE,
        OriginalModuleOrdinal::new(0),
    )
}

fn bind_unit(
    source: &str,
    declaration_file: bool,
    source_key: SourceUnitKey,
    original_module: OriginalModuleOrdinal,
) -> Binder {
    let prelude_allocator = Allocator::default();
    let source_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
    assert!(!parsed.panicked, "parse failed: {source}");
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let unit = CompilationUnit {
        source: source_key,
        origin: CompilationOrigin::User(original_module),
        binding: ModuleBindingContext::for_program(
            &parsed.program,
            if declaration_file {
                SourceFileKind::DeclarationTs
            } else {
                SourceFileKind::ImplementationTs
            },
        ),
    };
    let (module, _) = builder.add_module(&parsed.program, &[], unit);
    builder.finish(module)
}

#[test]
fn merge_disposition_lookup_is_exact_and_does_not_scan_merge_rows() {
    let binder = bind(
            "interface Shared { left: string }\ninterface Shared { right: number }\ntype Rejected = string;\ntype Rejected = number;\n",
            false,
        );
    let declarations = binder
        .namespaces
        .merges()
        .flat_map(|record| {
            record.declarations.iter().map(move |participant| {
                (participant.declaration, record.classification.disposition)
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 4);

    let work = NamespaceContinuationWorkScopeForTest::start();
    for (declaration, disposition) in declarations {
        assert_eq!(
            binder
                .namespaces
                .merge_disposition_for_declaration(declaration),
            Some(disposition)
        );
    }
    assert_eq!(work.finish().merge_rows, 0);
}

/// `DeclId` is dense, so locating the placement row of a single declaration must
/// cost a bounded number of participant probes — never a scan of the whole project.
const PLACEMENT_SYNTAX_PROBES_PER_DECLARATION: u64 = 4;

/// One group is a top-level declaration (which only writes syntax facts), a namespace
/// (whose fragment is back-patched onto the row), and a member of it (whose member row
/// reads the facts back) — every by-declaration placement lookup in one workload.
fn placement_syntax_probe_work(groups: u64) -> PlacementLookupWorkForTest {
    let source = (0..groups)
        .map(|index| {
            format!(
                "declare const value{index}: number;\n\
                     declare namespace Space{index} {{ const member{index}: number; }}\n"
            )
        })
        .collect::<String>();
    let scope = PlacementLookupWorkScopeForTest::start();
    let binder = bind(&source, false);
    let work = scope.finish();
    assert_eq!(
        u64::try_from(binder.namespaces.placements.local_len())
            .expect("placement bucket count fits u64"),
        3 * groups,
        "each group places a constant, a namespace, and a namespace member"
    );
    let last_member = format!("member{}", groups - 1);
    assert_eq!(
        binder
            .namespaces
            .members
            .local_iter()
            .find(|member| member.name.as_deref() == Some(last_member.as_str()))
            .map(|member| member.syntax),
        Some(DeclarationSyntaxFacts::Variable(VariableKind::Const)),
        "the member row carries the syntax facts read back from its placement"
    );
    assert!(
        binder
            .namespaces
            .placements
            .local_iter()
            .flat_map(|(_, participants)| participants.iter())
            .filter(|participant| participant.namespace_fragment.is_some())
            .count()
            == usize::try_from(groups).expect("group count fits usize"),
        "each namespace row keeps the fragment back-patched onto it"
    );
    work
}

#[test]
fn placement_syntax_probes_scale_with_declarations_not_with_every_placement() {
    const SMALL: u64 = 128;
    const SCALED: u64 = 1_024;

    let small = placement_syntax_probe_work(SMALL);
    let scaled = placement_syntax_probe_work(SCALED);

    let growth = scaled.row_probes / small.row_probes.max(1);
    assert!(
        growth <= 2 * SCALED / SMALL,
        "placement participant probes grew {growth}x while the declaration count grew {}x \
             ({small:?} -> {scaled:?}) — the lookup scans every placement per declaration",
        SCALED / SMALL
    );
    for (groups, work) in [(SMALL, small), (SCALED, scaled)] {
        let declarations = 3 * groups;
        let budget = PLACEMENT_SYNTAX_PROBES_PER_DECLARATION * declarations;
        assert!(
            work.row_probes <= budget,
            "{declarations} declarations spent {} placement participant probes, \
                 over the budget of {budget} ({work:?})",
            work.row_probes
        );
    }
}

fn assert_one_shot_placement_order_work(work: MergeSubstrateWorkForTest, groups: u64) {
    assert_eq!(
        work.placement_order_groups, groups,
        "the instance-state and canonical-order pass visited {} groups for {groups} changed \
             placement groups ({work:?})",
        work.placement_order_groups
    );
}

/// The three by-declaration lookups this guard covers all resolve through
/// `push_placement` now. Only the one-shot instance-state pass may still walk every changed
/// placement group. That pass also leaves each group in canonical order so the merge record
/// can share its vector, and the calibrated counter rejects a second pass.
#[test]
fn placement_rows_are_scanned_only_by_the_one_shot_instance_state_pass() {
    // The escaped needle never matches itself, so the split isolates production code.
    let production = include_str!("classification.rs")
        .split("#[cfg(test)]\nmod tests {")
        .next()
        .expect("production region precedes the test module");
    assert_eq!(
        production
            .matches("self.placements.changed_values_mut()")
            .count(),
        1,
        "the only surviving changed-group pass is the instance-state back-patch and ordering"
    );
    assert_eq!(
        production
            .matches("placements.changed_values_mut()")
            .count(),
        1,
        "no caller may add another scan over every changed placement group"
    );
    assert_eq!(
        production.matches("placements.local_values_mut()").count(),
        0,
        "the canonical-order pass must not bypass sparse prefix overrides"
    );
    assert_eq!(
        production.matches("placements.local_iter()").count(),
        0,
        "the by-declaration read resolves through the placement index"
    );

    let fixture = vec!["export {};\ninterface Merged { left: number; }\n\
             interface Merged { right: number; }\n"
        .to_string()];
    let work = merge_substrate_work(&fixture);
    assert_one_shot_placement_order_work(work, 1);

    let mut deliberately_repeated = work;
    deliberately_repeated.placement_order_groups += 1;
    let rejected = std::panic::catch_unwind(|| {
        assert_one_shot_placement_order_work(deliberately_repeated, 1);
    });
    assert!(
        rejected.is_err(),
        "the one-shot calibration admitted a deliberately repeated placement-group pass"
    );
}

/// One group is a top-level constant, a namespace, and a member of it — three placement
/// participants, one merge group each, and one namespace member row.
const FINALIZATION_PLACEMENTS_PER_GROUP: u64 = 3;

/// Groups the split deals out; the program is the same size at every split.
const FINALIZATION_GROUPS: usize = 192;

/// Total program size held constant while only the file split varies: the same groups are
/// dealt out evenly to `modules` files, so every difference between two calls is the split.
fn namespace_finalization_work(modules: usize) -> NamespaceFinalizationWorkForTest {
    const GROUPS: usize = FINALIZATION_GROUPS;
    assert_eq!(
        GROUPS % modules,
        0,
        "the split deals whole groups per module"
    );
    let per_module = GROUPS / modules;
    let sources = (0..modules)
        .map(|module| {
            (0..per_module)
                .map(|offset| {
                    let index = module * per_module + offset;
                    format!(
                        "declare const value{index}: number;\n\
                             declare namespace Space{index} {{ const member{index}: number; }}\n"
                    )
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let allocators = sources
        .iter()
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = sources
        .iter()
        .zip(&allocators)
        .map(|(source, allocator)| {
            let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
            assert!(!parsed.panicked, "parse failed for a split module");
            parsed
        })
        .collect::<Vec<_>>();

    let scope = NamespaceFinalizationWorkScopeForTest::start();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let mut last_module = None;
    for (index, parsed) in parsed.iter().enumerate() {
        let unit = CompilationUnit {
            source: SourceUnitKey(u32::try_from(index + 1).expect("source key fits u32")),
            origin: CompilationOrigin::User(OriginalModuleOrdinal::new(index)),
            binding: ModuleBindingContext::for_program(
                &parsed.program,
                SourceFileKind::ImplementationTs,
            ),
        };
        let (module, _) = builder.add_module(&parsed.program, &[], unit);
        last_module = Some(module);
    }
    let binder = builder.finish(last_module.expect("one project module"));
    let work = scope.finish();

    assert_eq!(
        u64::try_from(binder.namespaces.placements.local_len())
            .expect("placement bucket count fits u64"),
        FINALIZATION_PLACEMENTS_PER_GROUP * u64::try_from(GROUPS).expect("group count fits u64"),
        "the split changes only how the same program is filed, never its size"
    );
    work
}

/// `classify` rebuilds every canonical index over the whole accumulated project, so
/// running it once per module makes the same source text cost more the more files it is
/// split across. The program below is byte-for-byte the same work either way.
#[test]
fn namespace_finalization_reprocesses_the_project_once_per_project_not_once_per_module() {
    const FEW: usize = 8;
    const MANY: usize = 64;

    let few = namespace_finalization_work(FEW);
    let many = namespace_finalization_work(MANY);

    assert!(
        many.classifications <= few.classifications,
        "the same program was classified {} times at {MANY} modules against {} times at \
             {FEW} ({few:?} -> {many:?}) — finalization runs per module, not per project",
        many.classifications,
        few.classifications
    );
    assert!(
        many.merge_participant_rows <= few.merge_participant_rows,
        "the same program re-processed {} placement participants at {MANY} modules against \
             {} at {FEW} ({few:?} -> {many:?}) — the merges rebuild replays the whole project \
             once per module",
        many.merge_participant_rows,
        few.merge_participant_rows
    );
    assert!(
        many.merge_index_rows <= few.merge_index_rows,
        "the same program re-keyed {} merge rows at {MANY} modules against {} at {FEW} \
             ({few:?} -> {many:?}) — the merge index is rebuilt once per module",
        many.merge_index_rows,
        few.merge_index_rows
    );
    assert!(
        many.attachment_merge_rows <= few.attachment_merge_rows,
        "the same program re-scanned {} merge records for value attachments at {MANY} \
             modules against {} at {FEW} ({few:?} -> {many:?}) — the attachment fill walks the \
             whole project's merge set once per module",
        many.attachment_merge_rows,
        few.attachment_merge_rows
    );
    // Every merge belongs to the module(s) that declared it, so the fill visits each merge
    // for its own module and the total is the project's merge count at any split.
    let project_merges = FINALIZATION_PLACEMENTS_PER_GROUP
        * u64::try_from(FINALIZATION_GROUPS).expect("group count fits u64");
    assert_eq!(
        (few.attachment_merge_rows, many.attachment_merge_rows),
        (project_merges, project_merges),
        "the attachment fill scanned {} merge records at {FEW} modules and {} at {MANY} \
             ({few:?} -> {many:?}) — it must visit the project's {project_merges} merges once \
             in total, not re-derive the whole project's targets in every module's fill",
        few.attachment_merge_rows,
        many.attachment_merge_rows
    );

    // A counter is only as good as its recorder, so pin the shape too: this module walks
    // one module at a time and must never invoke the whole-project finalizer, which
    // belongs after the batch loop that `bind.rs` owns — once for the library batch and
    // once for the project batch. `finalize_namespace_metadata` is `pub(super)`, so
    // The binder member is the only place a call site can live.
    // The escaped needle never matches itself, so the split isolates production code.
    let namespace_production = include_str!("metadata.rs")
        .split("#[cfg(test)]\nmod tests {")
        .next()
        .expect("production region precedes the test module");
    assert_eq!(
        namespace_production
            .matches("finalize_namespace_metadata(")
            .count(),
        1,
        "per-module namespace collection declares the batch finalizer but must not call it"
    );
    let bind_production = include_str!("../bind.rs")
        .split("#[cfg(test)]\nmod tests {")
        .next()
        .expect("production region precedes the test module");
    assert!(
        bind_production
            .matches("finalize_namespace_metadata(")
            .count()
            >= 2,
        "the library batch and the project batch each finalize once, after their loop"
    );
}

/// Bind one external module per source, so every file owns a distinct declaration owner
/// and the same identifier declared in two files is two groups, never one merge.
fn merge_substrate_work(sources: &[String]) -> MergeSubstrateWorkForTest {
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let allocators = sources
        .iter()
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = sources
        .iter()
        .zip(&allocators)
        .map(|(source, allocator)| {
            let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
            assert!(
                !parsed.panicked,
                "parse failed for a merge-substrate module"
            );
            parsed
        })
        .collect::<Vec<_>>();

    let scope = MergeSubstrateWorkScopeForTest::start();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let mut last_module = None;
    for (index, parsed) in parsed.iter().enumerate() {
        let unit = CompilationUnit {
            source: SourceUnitKey(u32::try_from(index + 1).expect("source key fits u32")),
            origin: CompilationOrigin::User(OriginalModuleOrdinal::new(index)),
            binding: ModuleBindingContext::for_program(
                &parsed.program,
                SourceFileKind::ImplementationTs,
            ),
        };
        let (module, _) = builder.add_module(&parsed.program, &[], unit);
        last_module = Some(module);
    }
    let binder = builder.finish(last_module.expect("one project module"));
    let work = scope.finish();
    assert!(
        binder.namespaces.merges.local_len() > 0,
        "the project produced no merge groups, so the counters measure nothing"
    );
    work
}

/// Files in the sweep; every variant below deals the same declarations across the same
/// number of external modules, so only the axis named by the test differs.
const SUBSTRATE_MODULES: usize = 32;

/// Declarations per module — small enough to stay a fast unit test, large enough that a
/// per-declaration cost separates cleanly from the per-module scaffolding.
const SUBSTRATE_DECLARATIONS_PER_MODULE: usize = 16;

/// The same declaration count and the same group count either way; only the name text
/// repeats. `unique` gives every declaration in the project its own name.
fn non_merging_modules(unique: bool) -> Vec<String> {
    (0..SUBSTRATE_MODULES)
        .map(|module| {
            let mut source = String::from("export {};\n");
            for offset in 0..SUBSTRATE_DECLARATIONS_PER_MODULE {
                let index = if unique {
                    module * SUBSTRATE_DECLARATIONS_PER_MODULE + offset
                } else {
                    offset
                };
                source.push_str(&format!("const value_{index:06}: number = 0;\n"));
            }
            source
        })
        .collect()
}

/// A name the merge substrate has already stored must not be stored again — not for the
/// placement key, not for the merge record, not for the merge index, and not for a lookup.
/// The two projects below hold declaration count, group count and merge count constant and
/// vary only how many *distinct* names those declarations use, so every allocation beyond
/// the distinct-name count is a copy of a name the substrate already owns.
#[test]
fn merge_key_names_are_stored_once_per_distinct_name() {
    let declarations = SUBSTRATE_MODULES * SUBSTRATE_DECLARATIONS_PER_MODULE;
    let unique = merge_substrate_work(&non_merging_modules(true));
    let repeated = merge_substrate_work(&non_merging_modules(false));

    let repeated_budget =
        u64::try_from(SUBSTRATE_DECLARATIONS_PER_MODULE).expect("distinct name count fits u64");
    assert!(
        repeated.merge_key_name_allocations <= repeated_budget,
        "{declarations} declarations using only {SUBSTRATE_DECLARATIONS_PER_MODULE} distinct \
             names allocated {} owned merge-key names ({repeated:?}) against {} for the same \
             program with {declarations} distinct names ({unique:?}) — a name already stored \
             must cost nothing, so the budget is {repeated_budget}",
        repeated.merge_key_name_allocations,
        unique.merge_key_name_allocations
    );

    let unique_budget = u64::try_from(declarations).expect("declaration count fits u64");
    assert!(
        unique.merge_key_name_allocations <= unique_budget,
        "{declarations} declarations with {declarations} distinct names allocated {} owned \
             merge-key names ({unique:?}) — over the budget of one allocation per distinct name \
             ({unique_budget}); the substrate owns a `String` per key, per record and per lookup",
        unique.merge_key_name_allocations
    );

    // Same program size, same groups, same merges: with the name text shared, the bytes the
    // substrate owns must collapse to the distinct names, not stay proportional to the
    // declaration count.
    assert!(
        repeated.merge_key_name_bytes * 8 <= unique.merge_key_name_bytes,
        "shrinking the project's distinct names {}x moved owned merge-key name bytes only \
             from {} to {} ({repeated:?}) — the substrate stores the text per declaration, not \
             per name",
        declarations / SUBSTRATE_DECLARATIONS_PER_MODULE,
        unique.merge_key_name_bytes,
        repeated.merge_key_name_bytes
    );
}

/// Merge groups held constant across the sweep: each is two interfaces of one name, the
/// cheapest declaration pair that really merges.
const SUBSTRATE_MERGE_GROUPS: usize = 8;

/// One module of real merges, then `modules` modules of declarations that merge with
/// nothing. The merge set is identical at every point of the sweep; only the count of
/// non-merging declarations moves.
fn non_merging_sweep(modules: usize) -> Vec<String> {
    let mut merging = String::from("export {};\n");
    for group in 0..SUBSTRATE_MERGE_GROUPS {
        merging.push_str(&format!(
            "interface Merged_{group} {{ left: number; }}\n\
                 interface Merged_{group} {{ right: number; }}\n"
        ));
    }
    let mut sources = vec![merging];
    sources.extend((0..modules).map(|module| {
        let mut source = String::from("export {};\n");
        for offset in 0..SUBSTRATE_DECLARATIONS_PER_MODULE {
            let index = module * SUBSTRATE_DECLARATIONS_PER_MODULE + offset;
            source.push_str(&format!("const value_{index:06}: number = 0;\n"));
        }
        source
    }));
    sources
}

/// A declaration that merges with nothing must not be *copied* into merge substrate. The
/// two projects below carry the identical merge set — the same eight two-interface groups —
/// and differ only in how many declarations sit beside it that no other declaration shares
/// a name with, so every row the sweep adds is a row no merge can ever read twice.
///
/// One placement row per declaration is the floor: the group has to hold the declaration
/// somewhere. The merge record per single-declaration group is a **deliberate** deferral,
/// not a floor — backlog 94 keeps it because five production loops iterate the record set
/// (`namespace_values`, `library_compiler`, three in `bind`) and dropping records would
/// change what they see. Pinned here at exactly one apiece so it cannot grow unnoticed.
#[test]
fn declarations_that_merge_with_nothing_are_never_copied_into_merge_substrate() {
    const FEW: usize = 4;
    const MANY: usize = 32;

    let few = merge_substrate_work(&non_merging_sweep(FEW));
    let many = merge_substrate_work(&non_merging_sweep(MANY));

    for (modules, work) in [(FEW, few), (MANY, many)] {
        let declarations =
            u64::try_from(modules * SUBSTRATE_DECLARATIONS_PER_MODULE).expect("fits u64");
        assert_eq!(
            work.non_merging_participant_rows, declarations,
            "{declarations} declarations that merge with nothing cost {} retained \
                 participant rows ({work:?}) — one placement row each is the floor, so anything \
                 above it is the merge substrate keeping a second copy of a row it can share",
            work.non_merging_participant_rows
        );
        assert_eq!(
            work.non_merging_merge_records, declarations,
            "{declarations} declarations that merge with nothing built {} merge records \
                 ({work:?}) — the deferred one-per-group cost moved",
            work.non_merging_merge_records
        );
        assert_eq!(
            work.placement_sort_calls, 0,
            "already ordered placement groups still invoked {} sorts ({work:?})",
            work.placement_sort_calls
        );
        assert_eq!(
            work.placement_row_reorders, 0,
            "the canonical-order pass reordered {} placement groups ({work:?}) — rows are \
                 appended in ascending source order, and only that makes sorting them where \
                 they live invisible to everything that reads the placement map",
            work.placement_row_reorders
        );
    }
}

/// Bind script files as one project, returning each input file's module scope in input
/// order so a caller can name the file a declaration must belong to.
fn bind_cross_file_project(sources: &[(&str, SourceUnitKey)]) -> (Binder, Vec<ScopeId>) {
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let allocators = sources
        .iter()
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = sources
        .iter()
        .zip(&allocators)
        .map(|((source, _), allocator)| {
            let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
            assert!(!parsed.panicked, "parse failed: {source}");
            parsed
        })
        .collect::<Vec<_>>();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let mut modules = Vec::new();
    for (ordinal, ((_, source), parsed)) in sources.iter().zip(&parsed).enumerate() {
        let unit = CompilationUnit {
            source: *source,
            origin: CompilationOrigin::User(OriginalModuleOrdinal::new(ordinal)),
            binding: ModuleBindingContext::for_program(
                &parsed.program,
                SourceFileKind::ImplementationTs,
            ),
        };
        let (module, _) = builder.add_module(&parsed.program, &[], unit);
        modules.push(module);
    }
    let binder = builder.finish(*modules.last().expect("one project module"));
    (binder, modules)
}

/// Bind declaration files as one library batch, returning each input file's module scope
/// in input order. File ordinals follow the input, so swapping the inputs swaps the order
/// the batch binds and fills them in.
fn bind_cross_file_library(sources: &[&str]) -> (Binder, Vec<ScopeId>) {
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let allocators = sources
        .iter()
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = sources
        .iter()
        .zip(&allocators)
        .map(|(source, allocator)| {
            let parsed = Parser::new(allocator, source, SourceType::d_ts()).parse();
            assert!(!parsed.panicked, "parse failed: {source}");
            parsed
        })
        .collect::<Vec<_>>();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let units = parsed
        .iter()
        .enumerate()
        .map(|(ordinal, parsed)| {
            let key = u32::try_from(ordinal + 1).expect("source key fits u32");
            (
                &parsed.program,
                CompilationUnit::library(
                    SourceUnitKey(key),
                    LibraryFileOrdinal::new(ordinal),
                    &parsed.program,
                ),
            )
        })
        .collect::<Vec<_>>();
    let modules = builder.add_library_modules(&units);
    let binder = builder.finish(*modules.last().expect("one library module"));
    (binder, modules)
}

/// Everything the attachment fill writes for one namespace member: the lexical symbol it
/// binds in the member's own scope and the public symbol, both pointed at its storage.
/// A fill that never visits the member's module leaves both `None`, and nothing else in
/// the pipeline reports that — it is a silently dropped binding, not a diagnostic.
fn assert_member_attachment_filled(binder: &Binder, name: &str, module: ScopeId) {
    let member = binder
        .namespaces
        .members()
        .find(|member| member.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("namespace member {name} exists"));
    let declaration = member
        .declaration
        .and_then(|declaration| binder.declarations.get(declaration))
        .unwrap_or_else(|| panic!("{name} has a declaration row"));
    assert_eq!(
        declaration.site.module, module,
        "{name} is declared by the file whose fill must bind it"
    );
    let storage = declaration
        .value_storage
        .unwrap_or_else(|| panic!("{name} has a value storage"));
    let local = member
        .local_symbol
        .and_then(|symbol| binder.symbols.get(symbol))
        .unwrap_or_else(|| panic!("{name} lost its lexical local_symbol"));
    assert_eq!(
        local.value,
        Some(storage),
        "{name} lexical symbol points at its own storage"
    );
    let public = member
        .symbol
        .and_then(|symbol| binder.symbols.get(symbol))
        .unwrap_or_else(|| panic!("{name} has a public symbol"));
    assert_eq!(
        public.value,
        Some(storage),
        "{name} lost the public symbol.value the fill publishes"
    );
    assert_ne!(
        member.local_symbol, member.symbol,
        "{name} keeps its lexical symbol distinct from its exported one"
    );
}

/// A namespace continued in a second file leaves one merge record whose fragments live in
/// two different modules. The fill runs once per module, so an index that hands a fill
/// only "its own" merges has to be keyed on **every** participating module: today's
/// whole-project scan is only accidentally safe here, and the failure mode is a member
/// that silently keeps no `local_symbol` and no `symbol.value`.
#[test]
fn a_project_merge_spanning_two_files_is_filled_in_both_of_them() {
    const FIRST: (&str, SourceUnitKey) = (
        "namespace Shared { export const first = 1; }",
        SourceUnitKey(10),
    );
    const SECOND: (&str, SourceUnitKey) = (
        "namespace Shared { export const second = 2; }",
        SourceUnitKey(20),
    );

    let (binder, modules) = bind_cross_file_project(&[FIRST, SECOND]);
    assert_eq!(
        binder
            .namespaces
            .merges
            .iter()
            .filter(|record| record.name.as_ref() == "Shared")
            .count(),
        1,
        "the reopening shares one merge record across the two files"
    );
    assert_member_attachment_filled(&binder, "first", modules[0]);
    assert_member_attachment_filled(&binder, "second", modules[1]);

    // Merge order decides which participant leads the record, so the same program in the
    // other file order must bind exactly as much.
    let (binder, modules) = bind_cross_file_project(&[SECOND, FIRST]);
    assert_member_attachment_filled(&binder, "second", modules[0]);
    assert_member_attachment_filled(&binder, "first", modules[1]);
}

/// The library batch is where cross-file merges are the rule rather than the exception:
/// one shared-global name is continued in a second file, and a `function` in one file is
/// merged with a `namespace` in another. Both must fill the module that declares the
/// member, which is not the module that opened the merge.
#[test]
fn a_library_merge_spanning_two_files_is_filled_in_both_of_them() {
    const FIRST: &str = "declare namespace Shared { const first: number; }\n\
                             declare function paired(): void;";
    const SECOND: &str = "declare namespace Shared { const second: number; }\n\
                              declare namespace paired { const tag: number; }";

    let (binder, modules) = bind_cross_file_library(&[FIRST, SECOND]);
    for name in ["Shared", "paired"] {
        assert_eq!(
            binder
                .namespaces
                .merges
                .iter()
                .filter(|record| record.name.as_ref() == name)
                .count(),
            1,
            "{name} shares one merge record across the two library files"
        );
    }
    assert_member_attachment_filled(&binder, "first", modules[0]);
    assert_member_attachment_filled(&binder, "second", modules[1]);
    assert_member_attachment_filled(&binder, "tag", modules[1]);

    let (binder, modules) = bind_cross_file_library(&[SECOND, FIRST]);
    assert_member_attachment_filled(&binder, "second", modules[0]);
    assert_member_attachment_filled(&binder, "tag", modules[0]);
    assert_member_attachment_filled(&binder, "first", modules[1]);
}

/// The storage each `name` member carries and the shared exported symbol they publish to.
fn shared_member_storages(binder: &Binder, name: &str) -> (Vec<ValueStorageId>, Symbol) {
    let members = binder
        .namespaces
        .members()
        .filter(|member| member.name.as_deref() == Some(name))
        .collect::<Vec<_>>();
    assert_eq!(members.len(), 2, "one {name} member per file");
    let symbols = members
        .iter()
        .map(|member| member.symbol.expect("exported member symbol"))
        .collect::<Vec<_>>();
    assert_eq!(symbols[0], symbols[1], "both files publish one symbol");
    let storages = members
        .iter()
        .map(|member| {
            member
                .declaration
                .and_then(|declaration| binder.declarations.get(declaration))
                .and_then(|declaration| declaration.value_storage)
                .expect("member storage")
        })
        .collect::<Vec<_>>();
    let symbol = binder
        .symbols
        .get(symbols[0])
        .expect("shared exported symbol")
        .clone();
    (storages, symbol)
}

/// Two files declaring the same namespace member write the *same* exported symbol, so the
/// order the fills apply them in is observable. The two batch paths already disagree and
/// must keep disagreeing: a library batch always filled one module at a time, so its last
/// module wins, while a project batch re-applied the whole plan in every fill, so the
/// globally last declaration by span wins even when it belongs to the earlier file. The
/// overload list is in fill order on both, because a member's storage only exists from its
/// own module's fill onwards.
#[test]
fn a_symbol_shared_by_two_files_keeps_the_same_winning_storage() {
    // The padded file's member has the larger span start, so the project order (by span)
    // and the file order disagree — which is the only way to tell the two rules apart.
    const PADDED: &str = "declare namespace Over { /* pad pad pad pad */ \
                              function shared(a: string): void; }";
    const PLAIN: &str = "declare namespace Over { function shared(a: number): void; }";

    let (binder, _) =
        bind_cross_file_project(&[(PADDED, SourceUnitKey(10)), (PLAIN, SourceUnitKey(20))]);
    let (storages, symbol) = shared_member_storages(&binder, "shared");
    assert_eq!(symbol.function_values, storages, "overloads in file order");
    assert_eq!(
        symbol.value,
        Some(storages[0]),
        "the project batch keeps the globally last declaration by span, which is the \
             padded file's — here the first one"
    );

    let (binder, _) =
        bind_cross_file_project(&[(PLAIN, SourceUnitKey(10)), (PADDED, SourceUnitKey(20))]);
    let (storages, symbol) = shared_member_storages(&binder, "shared");
    assert_eq!(symbol.function_values, storages, "overloads in file order");
    assert_eq!(
        symbol.value,
        Some(storages[1]),
        "the same padded declaration still wins in the other file order"
    );

    let (binder, _) = bind_cross_file_library(&[PADDED, PLAIN]);
    let (storages, symbol) = shared_member_storages(&binder, "shared");
    assert_eq!(symbol.function_values, storages, "overloads in file order");
    assert_eq!(
        symbol.value,
        Some(storages[1]),
        "a library batch fills one module at a time, so its last file wins"
    );
}

fn merge<'a>(binder: &'a Binder, name: &str) -> &'a MergeRecord {
    binder
        .namespaces
        .merges
        .iter()
        .find(|record| record.name.as_ref() == name)
        .expect("merge record")
}

fn enum_group<'a>(binder: &'a Binder, name: &str) -> EnumGroupView<'a> {
    let record = merge(binder, name);
    let group = record
        .declarations
        .iter()
        .find(|participant| participant.kind == MergeDeclarationKind::Enum)
        .and_then(|participant| binder.declarations.get(participant.declaration))
        .and_then(|declaration| declaration.type_group)
        .expect("enum declaration owns a type group");
    binder.enum_group(group).expect("canonical enum group view")
}

fn issue_owner_kinds(record: &MergeRecord, issues: &[EnumGroupIssue]) -> Vec<MergeDeclarationKind> {
    issues
        .iter()
        .map(|issue| {
            record
                .declarations
                .iter()
                .find(|participant| participant.declaration == issue.owner)
                .expect("issue owner is a group participant")
                .kind
        })
        .collect()
}

fn placement_owner_kinds(record: &MergeRecord) -> Vec<MergeDeclarationKind> {
    record
        .placement_issues
        .iter()
        .map(|issue| {
            record
                .declarations
                .iter()
                .find(|participant| participant.declaration == issue.owner)
                .expect("placement owner is a group participant")
                .kind
        })
        .collect()
}

#[test]
fn enum_groups_use_real_type_group_keys_and_ordered_canonical_inputs() {
    let source = r#"
enum Root { First }
enum Root { Second }
namespace Root { export const tag = 1; }
export enum Exported { Value }
namespace Outer {
    export enum Nested { First }
    export enum Nested { Second }
    export namespace Nested { export const tag = 1; }
}
"#;
    let binder = bind(source, false);

    let enum_declarations = binder
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Enum)
        .collect::<Vec<_>>();
    assert_eq!(enum_declarations.len(), 5);
    assert!(enum_declarations
        .iter()
        .all(|declaration| declaration.type_group.is_some()));
    assert!(enum_declarations
        .iter()
        .all(|declaration| declaration.value_storage.is_some()));
    for declaration in &enum_declarations {
        let group = declaration.type_group.expect("real enum type group");
        let row = binder.type_groups.get(group).expect("type group row");
        assert!(row
            .fragments
            .iter()
            .any(|fragment| fragment.declaration == declaration.id
                && fragment.kind == TypeFragmentKind::Enum));
    }

    let root = enum_group(&binder, "Root");
    assert_eq!(root.name, "Root");
    assert_eq!(root.declarations.len(), 2);
    assert!(root.declarations.windows(2).all(|pair| {
        (pair[0].source, pair[0].span.start, pair[0].declaration.0)
            < (pair[1].source, pair[1].span.start, pair[1].declaration.0)
    }));
    assert!(root.namespace.is_some());
    assert_eq!(root.namespace_declarations.len(), 1);
    assert_eq!(root.buildability, EnumGroupBuildability::Buildable);
    assert!(root.issues.is_empty());
    assert!(root.placement_issues.is_empty());
    assert!(root.declarations.iter().all(|participant| {
        binder
            .declarations
            .get(participant.declaration)
            .is_some_and(|declaration| declaration.type_group == Some(root.group))
    }));

    let exported = enum_group(&binder, "Exported");
    assert_eq!(exported.declarations.len(), 1);
    assert_eq!(exported.namespace, None);
    assert_eq!(exported.buildability, EnumGroupBuildability::Buildable);

    let nested_record = binder
        .namespaces
        .merges
        .iter()
        .find(|record| {
            record.name.as_ref() == "Nested"
                && matches!(record.owner, DeclarationOwner::NamespacePublic(_))
        })
        .expect("nested enum merge record");
    let nested_group = nested_record
        .declarations
        .iter()
        .find(|participant| participant.kind == MergeDeclarationKind::Enum)
        .and_then(|participant| binder.declarations.get(participant.declaration))
        .and_then(|declaration| declaration.type_group)
        .expect("nested enum group");
    let nested = binder.enum_group(nested_group).expect("nested enum view");
    assert_eq!(nested.declarations.len(), 2);
    assert!(nested.namespace.is_some());
    assert_eq!(nested.namespace_declarations.len(), 1);
    assert_eq!(nested.buildability, EnumGroupBuildability::Buildable);
}

#[test]
fn nested_const_enums_reserve_checker_values_without_runtime_namespace_storage() {
    let source = r#"
namespace PrivateOuter { const enum Private { Value } }
namespace ExportedOuter { export const enum Exported { Value } }
namespace TypeOnlyOuter {
    export interface Marker {}
    export const enum TypeOnly { Value }
}
declare namespace AmbientOuter { const enum Ambient { Value } }
"#;
    let binder = bind(source, false);
    let enum_declarations = binder
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Enum)
        .collect::<Vec<_>>();
    assert_eq!(enum_declarations.len(), 4);
    let storages = enum_declarations
        .iter()
        .map(|declaration| {
            assert!(declaration.type_group.is_some());
            declaration
                .value_storage
                .expect("const enum has checker-visible value storage")
        })
        .collect::<rustc_hash::FxHashSet<_>>();
    assert_eq!(storages.len(), enum_declarations.len());
    assert_eq!(binder.decl_count, 4);

    for name in [
        "PrivateOuter",
        "ExportedOuter",
        "TypeOnlyOuter",
        "AmbientOuter",
    ] {
        let namespace = binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == name)
            .unwrap_or_else(|| panic!("{name} namespace"));
        assert_eq!(
            binder.namespaces.aggregate_instance_state(namespace.id),
            Some(NamespaceInstanceState::NonInstantiated),
            "{name}"
        );
        assert_eq!(
            binder.namespaces.standalone_value_storage(namespace.id),
            None,
            "{name}"
        );
    }
}

#[test]
fn enum_group_legality_and_issue_owners_match_typescript_matrix() {
    use MergeDeclarationKind::{Enum, Function, Namespace};

    struct Row {
        name: &'static str,
        source: &'static str,
        buildability: EnumGroupBuildability,
        issue_owners: &'static [MergeDeclarationKind],
        placement_owners: &'static [MergeDeclarationKind],
    }

    let rows = [
        Row {
            name: "EE",
            source: "enum X {}\nenum X {}\n",
            buildability: EnumGroupBuildability::Buildable,
            issue_owners: &[],
            placement_owners: &[],
        },
        Row {
            name: "CC",
            source: "const enum X {}\nconst enum X {}\n",
            buildability: EnumGroupBuildability::Buildable,
            issue_owners: &[],
            placement_owners: &[],
        },
        Row {
            name: "EN",
            source: "enum X {}\nnamespace X { export const tag = 1; }\n",
            buildability: EnumGroupBuildability::Buildable,
            issue_owners: &[],
            placement_owners: &[],
        },
        Row {
            name: "NE",
            source: "namespace X { export const tag = 1; }\nenum X {}\n",
            buildability: EnumGroupBuildability::Buildable,
            issue_owners: &[],
            placement_owners: &[],
        },
        Row {
            name: "CN",
            source: "const enum X {}\nnamespace X { export const tag = 1; }\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Enum, Namespace],
            placement_owners: &[],
        },
        Row {
            name: "NC",
            source: "namespace X { export const tag = 1; }\nconst enum X {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Namespace, Enum],
            placement_owners: &[],
        },
        Row {
            name: "CO",
            source: "const enum X {}\nenum X {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Enum, Enum],
            placement_owners: &[],
        },
        Row {
            name: "OC",
            source: "enum X {}\nconst enum X {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Enum, Enum],
            placement_owners: &[],
        },
        Row {
            name: "EF",
            source: "enum X {}\nfunction X() {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Enum, Function],
            placement_owners: &[],
        },
        Row {
            name: "FE",
            source: "function X() {}\nenum X {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Function, Enum],
            placement_owners: &[],
        },
        Row {
            name: "ENF",
            source: "enum X {}\nnamespace X { export const tag = 1; }\nfunction X() {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Enum, Function],
            placement_owners: &[Namespace],
        },
        Row {
            name: "EFN",
            source: "enum X {}\nfunction X() {}\nnamespace X { export const tag = 1; }\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Enum, Function],
            placement_owners: &[],
        },
        Row {
            name: "NEF",
            source: "namespace X { export const tag = 1; }\nenum X {}\nfunction X() {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Namespace, Enum, Function],
            placement_owners: &[Namespace],
        },
        Row {
            name: "NFE",
            source: "namespace X { export const tag = 1; }\nfunction X() {}\nenum X {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Namespace, Function, Enum],
            placement_owners: &[Namespace],
        },
        Row {
            name: "FEN",
            source: "function X() {}\nenum X {}\nnamespace X { export const tag = 1; }\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Function, Enum],
            placement_owners: &[],
        },
        Row {
            name: "FNE",
            source: "function X() {}\nnamespace X { export const tag = 1; }\nenum X {}\n",
            buildability: EnumGroupBuildability::RejectedFutureTk2567,
            issue_owners: &[Function, Namespace, Enum],
            placement_owners: &[],
        },
    ];

    for row in rows {
        let binder = bind(row.source, false);
        let record = merge(&binder, "X");
        assert!(
            record
                .declarations
                .iter()
                .any(|participant| participant.kind == MergeDeclarationKind::Enum),
            "{} merge record contains an enum",
            row.name
        );
        assert!(
            record
                .declarations
                .iter()
                .filter(|participant| participant.kind == MergeDeclarationKind::Enum)
                .all(|participant| binder
                    .declarations
                    .get(participant.declaration)
                    .is_some_and(|declaration| declaration.type_group.is_some())),
            "{} enum declarations own type groups",
            row.name
        );
        let view = enum_group(&binder, "X");
        assert_eq!(view.buildability, row.buildability, "{}", row.name);
        assert_eq!(
            record.classification.disposition,
            match row.buildability {
                EnumGroupBuildability::Buildable => MergeDisposition::Admitted,
                EnumGroupBuildability::RejectedFutureTk2567 => {
                    MergeDisposition::RejectedFutureTk2567
                }
            },
            "{}",
            row.name
        );
        assert_eq!(
            issue_owner_kinds(record, &view.issues),
            row.issue_owners,
            "{} TK2567 owners",
            row.name
        );
        assert!(view
            .issues
            .iter()
            .all(|issue| issue.kind == EnumGroupIssueKind::FutureTk2567));
        assert_eq!(
            placement_owner_kinds(record),
            row.placement_owners,
            "{} TK2434 owners",
            row.name
        );
        assert_eq!(
            view.placement_issues, record.placement_issues,
            "{}",
            row.name
        );
        assert!(view.issues.iter().all(|issue| {
            binder
                .declarations
                .get(issue.owner)
                .is_some_and(|declaration| declaration.site.binding_span == issue.span)
        }));
    }
}

#[test]
fn enum_group_normalization_is_independent_of_module_insertion_order() {
    const SOURCES: [(&str, SourceUnitKey); 3] = [
        ("declare enum X { First }", SourceUnitKey(10)),
        ("declare enum X { Second }", SourceUnitKey(15)),
        (
            "declare namespace X { const tag: number; }",
            SourceUnitKey(20),
        ),
    ];

    let bind_permutation = |order: [usize; 3]| {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let allocators = SOURCES
            .iter()
            .map(|_| Allocator::default())
            .collect::<Vec<_>>();
        let parsed = SOURCES
            .iter()
            .zip(&allocators)
            .map(|((source, _), allocator)| {
                let parsed = Parser::new(allocator, source, SourceType::d_ts()).parse();
                assert!(!parsed.panicked, "parse failed: {source}");
                parsed
            })
            .collect::<Vec<_>>();
        let units = order
            .iter()
            .map(|index| {
                let source = SOURCES[*index].1;
                let ordinal = LibraryFileOrdinal::new(*index);
                (
                    &parsed[*index].program,
                    CompilationUnit::library(source, ordinal, &parsed[*index].program),
                )
            })
            .collect::<Vec<_>>();
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let modules = builder.add_library_modules(&units);
        builder.finish(*modules.last().expect("one library module"))
    };

    let snapshot = |binder: &Binder| {
        let view = enum_group(binder, "X");
        (
            view.declarations
                .iter()
                .map(|participant| participant.source)
                .collect::<Vec<_>>(),
            view.namespace_declarations
                .iter()
                .map(|participant| participant.source)
                .collect::<Vec<_>>(),
            view.namespace.is_some(),
            view.buildability,
            view.issues
                .iter()
                .map(|issue| issue.source)
                .collect::<Vec<_>>(),
            view.placement_issues
                .iter()
                .map(|issue| issue.source)
                .collect::<Vec<_>>(),
        )
    };

    let forward = bind_permutation([0, 1, 2]);
    let reverse_insertion = bind_permutation([2, 1, 0]);
    assert_eq!(snapshot(&forward), snapshot(&reverse_insertion));
    assert_eq!(
        snapshot(&forward),
        (
            vec![SourceUnitKey(10), SourceUnitKey(15)],
            vec![SourceUnitKey(20)],
            true,
            EnumGroupBuildability::Buildable,
            Vec::new(),
            Vec::new(),
        )
    );
}

#[test]
fn attached_namespace_value_view_is_whole_group_ordered_and_storage_truthful() {
    let source = r#"
function F(): void {}
namespace F { export const first = 1; const hidden = 0; }
namespace F {
    export function make(functionParam: number): number { return functionParam; }
    export class Box { method(methodParam: string): void {} }
    export const second = 2;
}
class C {}
namespace C { export const classTag = 1; }
enum E { Member }
namespace E { export const enumTag = 1; }
namespace Standalone { export const dormant = 1; }
interface TypeOnly {}
namespace TypeOnly { export const dormant = 1; }
function Chimera(): void {}
namespace Chimera { export const tag = 1; }
enum Chimera { EnumOnly }
namespace Chimera {
    export function helper(value: number): number { return value; }
    export class Helper { method(value: string): void {} }
}
function ClassChimera(): void {}
class ClassChimera {}
namespace ClassChimera { export const rejected = 1; }
enum ClassChimera {}
"#;
    let binder = bind(source, false);

    let attachment = binder
        .namespace_value_attachment(binder.module, "F")
        .expect("function namespace attachment");
    assert_eq!(
        attachment.disposition,
        NamespaceValueAttachmentDisposition::AdmittedFunction
    );
    assert_eq!(attachment.owner, DeclarationOwner::Lexical(binder.module));
    assert_eq!(attachment.name, "F");
    assert_eq!(attachment.fragments.len(), 2);
    assert!(attachment
        .fragments
        .windows(2)
        .all(|pair| pair[0].source_start < pair[1].source_start));
    assert_eq!(
        attachment
            .members
            .iter()
            .map(|member| member.name)
            .collect::<Vec<_>>(),
        ["first", "make", "Box", "second"]
    );
    assert!(attachment.members.windows(2).all(|pair| {
        (
            pair[0].source,
            pair[0].site.declaration_span.start,
            pair[0].declaration.0,
        ) < (
            pair[1].source,
            pair[1].site.declaration_span.start,
            pair[1].declaration.0,
        )
    }));

    for attached in &attachment.members {
        let declaration = binder
            .declarations
            .get(attached.declaration)
            .expect("attached declaration");
        assert_eq!(attached.site, declaration.site);
        assert_eq!(
            attached.scope,
            declaration.site.scope.expect("lexical scope")
        );
        assert_eq!(attached.source, SourceUnitKey::SINGLE_SOURCE);
        let storage = attached.value_storage.expect("real value storage");
        assert_eq!(declaration.value_storage, Some(storage));

        let member = binder
            .namespaces
            .member(attached.member)
            .expect("namespace member");
        assert_eq!(member.declaration, Some(attached.declaration));
        assert_eq!(member.symbol, attached.symbol);
        let public_symbol = attached
            .symbol
            .and_then(|symbol| binder.symbols.get(symbol))
            .expect("public member symbol");
        assert_eq!(public_symbol.value, Some(storage));
        let local_symbol = member
            .local_symbol
            .and_then(|symbol| binder.symbols.get(symbol))
            .expect("private lexical symbol");
        assert_eq!(local_symbol.value, Some(storage));
        assert_ne!(member.local_symbol, member.symbol);
    }

    let make = attachment
        .members
        .iter()
        .find(|member| member.name == "make")
        .expect("make member");
    let make_symbol = make
        .symbol
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("make symbol");
    assert_eq!(
        make_symbol.function_values,
        [make.value_storage.expect("make storage")]
    );
    let make_scope = binder
        .fn_scopes
        .get(&(binder.module, make.site.declaration_span.start))
        .copied()
        .expect("attached function bound through the ordinary function binder");
    assert_eq!(
        binder.graph.get(make_scope).and_then(|scope| scope.parent),
        Some(make.scope)
    );
    let function_param = binder
        .declarations
        .iter()
        .find(|declaration| &source[declaration.site.binding_span.range()] == "functionParam")
        .expect("attached function parameter");
    assert_eq!(function_param.site.scope, Some(make_scope));
    assert!(function_param.value_storage.is_some());

    let box_member = attachment
        .members
        .iter()
        .find(|member| member.name == "Box")
        .expect("Box member");
    let method_param = binder
        .declarations
        .iter()
        .find(|declaration| &source[declaration.site.binding_span.range()] == "methodParam")
        .expect("attached class method parameter");
    let method_scope = method_param.site.scope.expect("method function scope");
    assert_eq!(
        binder
            .graph
            .get(method_scope)
            .and_then(|scope| scope.parent),
        Some(box_member.scope)
    );
    assert!(method_param.value_storage.is_some());

    let class_attachment = binder
        .namespace_value_attachment(binder.module, "C")
        .expect("class namespace attachment");
    assert_eq!(
        class_attachment.disposition,
        NamespaceValueAttachmentDisposition::AdmittedClass
    );
    assert_eq!(
        class_attachment
            .members
            .iter()
            .map(|member| member.name)
            .collect::<Vec<_>>(),
        ["classTag"]
    );
    assert!(class_attachment.members[0].value_storage.is_some());

    let enum_attachment = binder
        .namespace_value_attachment(binder.module, "E")
        .expect("enum namespace attachment");
    assert_eq!(
        enum_attachment.disposition,
        NamespaceValueAttachmentDisposition::AdmittedEnum
    );
    assert_eq!(
        enum_attachment
            .members
            .iter()
            .map(|member| member.name)
            .collect::<Vec<_>>(),
        ["enumTag"]
    );
    assert!(enum_attachment.members[0].value_storage.is_some());

    for name in ["Standalone", "TypeOnly"] {
        let namespace = binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == name)
            .expect("type-container namespace identity");
        let owner_scope = match namespace.owner {
            NamespaceOwner::Lexical(scope) => scope,
            _ => panic!("fixture root is lexical"),
        };
        let attachment = binder
            .namespace_value_attachment(owner_scope, name)
            .expect("type-container namespace");
        assert_eq!(
            attachment.disposition,
            NamespaceValueAttachmentDisposition::TypeContainerOnly,
            "{name}"
        );
        assert!(attachment.fragments.is_empty(), "{name}");
        assert!(attachment.members.is_empty(), "{name}");
        let owner_start = u32::try_from(source.find(name).expect("owner source start"))
            .expect("source offset fits u32");
        let dormant = binder
            .declarations
            .iter()
            .find(|declaration| {
                declaration.site.declaration_span.start > owner_start
                    && &source[declaration.site.binding_span.range()] == "dormant"
            })
            .expect("dormant value declaration");
        assert!(dormant.value_storage.is_some(), "{name}");
    }

    let chimera = binder
        .namespace_value_attachment(binder.module, "Chimera")
        .expect("deferred callable recovery");
    assert_eq!(
        chimera.disposition,
        NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42
    );
    assert_eq!(
        merge(&binder, "Chimera").classification.disposition,
        MergeDisposition::RejectedFutureTk2567
    );
    assert_eq!(chimera.fragments.len(), 2);
    assert!(chimera
        .fragments
        .windows(2)
        .all(|pair| pair[0].source_start < pair[1].source_start));
    assert_eq!(
        chimera
            .members
            .iter()
            .map(|member| (member.name, member.kind))
            .collect::<Vec<_>>(),
        [
            ("tag", MergeDeclarationKind::Variable),
            ("helper", MergeDeclarationKind::Function),
            ("Helper", MergeDeclarationKind::Class),
        ]
    );
    assert!(chimera.members.windows(2).all(|pair| {
        (
            pair[0].source,
            pair[0].site.declaration_span.start,
            pair[0].declaration.0,
        ) < (
            pair[1].source,
            pair[1].site.declaration_span.start,
            pair[1].declaration.0,
        )
    }));
    for attached in &chimera.members {
        let storage = attached
            .value_storage
            .expect("deferred namespace member storage");
        assert_eq!(
            binder
                .declarations
                .get(attached.declaration)
                .and_then(|declaration| declaration.value_storage),
            Some(storage)
        );
        let member = binder
            .namespaces
            .member(attached.member)
            .expect("deferred namespace member");
        assert_eq!(
            member
                .symbol
                .and_then(|symbol| binder.symbols.get(symbol))
                .and_then(|symbol| symbol.value),
            Some(storage)
        );
        assert_eq!(
            member
                .local_symbol
                .and_then(|symbol| binder.symbols.get(symbol))
                .and_then(|symbol| symbol.value),
            Some(storage)
        );
    }
    let tag = chimera.members.first().expect("exact deferred tag member");
    assert_eq!(&source[tag.site.binding_span.range()], "tag");

    let enum_participant = merge(&binder, "Chimera")
        .declarations
        .iter()
        .find(|participant| participant.kind == MergeDeclarationKind::Enum)
        .expect("enum participant remains in the rejected group");
    let enum_declaration = binder
        .declarations
        .get(enum_participant.declaration)
        .expect("enum declaration identity");
    assert_eq!(enum_declaration.kind, DeclarationKind::Enum);
    assert_eq!(enum_declaration.site.module, binder.module);
    assert!(enum_declaration.value_storage.is_some());
    assert!(enum_declaration.type_group.is_some());
    assert_eq!(
        merge(&binder, "Chimera").owner,
        DeclarationOwner::Lexical(binder.module)
    );
    assert!(chimera
        .members
        .iter()
        .all(|member| member.declaration != enum_participant.declaration));
    assert!(!chimera
        .members
        .iter()
        .any(|member| member.name == "EnumOnly" || member.kind == MergeDeclarationKind::Enum));

    let class_chimera = binder
        .namespace_value_attachment(binder.module, "ClassChimera")
        .expect("class backlog-42 group");
    assert_eq!(
        class_chimera.disposition,
        NamespaceValueAttachmentDisposition::Rejected(MergeDisposition::RejectedFutureTk2567)
    );
    assert!(merge(&binder, "ClassChimera")
        .classification
        .compositions
        .iter()
        .any(|composition| {
            composition.kind == MergeCompositionKind::FunctionNamespace
                && composition.disposition == MergeDisposition::Admitted
        }));
    assert!(class_chimera.fragments.is_empty());
    assert!(class_chimera.members.is_empty());
    let rejected = binder
        .declarations
        .iter()
        .find(|declaration| &source[declaration.site.binding_span.range()] == "rejected")
        .expect("class chimera namespace member");
    assert_eq!(rejected.value_storage, None);
}

#[test]
fn attached_namespace_private_values_bind_only_in_fragment_private_scopes() {
    let source = r#"
function F(): void {}
namespace F {
    export const functionTag = 1;
    const hiddenVariableF = 1;
    function hiddenFunctionF(functionParamF: number): number {
        const functionLocalF = functionParamF;
        return functionLocalF;
    }
    class HiddenClassF {
        method(methodParamF: number): number {
            const methodLocalF = methodParamF;
            return methodLocalF;
        }
    }
}

class C {}
namespace C {
    export const classTag = 1;
    const hiddenVariableC = 1;
    function hiddenFunctionC(functionParamC: number): number {
        const functionLocalC = functionParamC;
        return functionLocalC;
    }
    class HiddenClassC {
        method(methodParamC: number): number {
            const methodLocalC = methodParamC;
            return methodLocalC;
        }
    }
}
"#;
    let binder = bind(source, false);

    for (owner, tag, private_names, descendant_names) in [
        (
            "F",
            "functionTag",
            ["hiddenVariableF", "hiddenFunctionF", "HiddenClassF"],
            [
                "functionParamF",
                "functionLocalF",
                "methodParamF",
                "methodLocalF",
            ],
        ),
        (
            "C",
            "classTag",
            ["hiddenVariableC", "hiddenFunctionC", "HiddenClassC"],
            [
                "functionParamC",
                "functionLocalC",
                "methodParamC",
                "methodLocalC",
            ],
        ),
    ] {
        let attachment = binder
            .namespace_value_attachment(binder.module, owner)
            .expect("admitted attachment");
        assert_eq!(
            attachment
                .members
                .iter()
                .map(|member| member.name)
                .collect::<Vec<_>>(),
            [tag]
        );
        let fragment = attachment.fragments.first().expect("one fragment");
        let public_scope = binder
            .namespaces
            .get(fragment.namespace)
            .expect("namespace")
            .public_scope;

        for name in private_names {
            assert!(binder
                .graph
                .get(public_scope)
                .and_then(|scope| scope.lookup_local(name))
                .is_none());
            let private_symbol = binder
                .graph
                .get(fragment.private_scope)
                .and_then(|scope| scope.lookup_local(name))
                .expect("private local symbol");
            let storage = binder
                .symbols
                .get(private_symbol)
                .and_then(|symbol| symbol.value)
                .expect("private local storage");
            let declaration = binder
                .declarations
                .iter()
                .find(|declaration| &source[declaration.site.binding_span.range()] == name)
                .expect("private declaration");
            assert_eq!(declaration.site.scope, Some(fragment.private_scope));
            assert_eq!(declaration.value_storage, Some(storage));
            let member = fragment
                .members
                .iter()
                .filter_map(|member| binder.namespaces.member(*member))
                .find(|member| member.name.as_deref() == Some(name))
                .expect("private namespace member");
            assert_eq!(member.publication, NamespacePublication::Private);
            assert_eq!(member.symbol, Some(private_symbol));
            assert_eq!(member.local_symbol, Some(private_symbol));
        }

        for name in descendant_names {
            let declaration = binder
                .declarations
                .iter()
                .find(|declaration| &source[declaration.site.binding_span.range()] == name)
                .expect("private descendant declaration");
            let scope = declaration.site.scope.expect("private descendant scope");
            assert_eq!(
                binder.graph.get(scope).and_then(|scope| scope.parent),
                Some(fragment.private_scope)
            );
            assert!(declaration.value_storage.is_some());
        }
    }
}

#[test]
fn placement_view_retains_recovery_and_ambient_default_exports() {
    let source = r#"
namespace LateIssue {}
namespace F { export const first = 1; }
function F(): void {}
namespace C { export const second = 2; }
class C {}
namespace LateIssue { export const third = 3; }
function LateIssue(): void {}
declare namespace Ambient { const implicit: number; }
declare function Ambient(): void;
"#;
    let binder = bind_unit(
        source,
        false,
        SourceUnitKey(17),
        OriginalModuleOrdinal::new(5),
    );
    let issues = binder.namespaces.placement_issues().collect::<Vec<_>>();
    assert_eq!(issues.len(), 3);
    assert!(issues
        .iter()
        .all(|issue| issue.kind == PlacementIssueKind::FutureTk2434));
    assert!(issues.iter().all(|issue| {
        issue.source == SourceUnitKey(17)
            && issue.origin == CompilationOrigin::User(OriginalModuleOrdinal::new(5))
            && binder
                .declarations
                .get(issue.owner)
                .is_some_and(|declaration| declaration.site.binding_span == issue.span)
    }));
    assert_eq!(
        issues
            .iter()
            .map(|issue| &source[issue.span.range()])
            .collect::<Vec<_>>(),
        ["F", "C", "LateIssue"]
    );
    assert!(issues
        .windows(2)
        .all(|pair| pair[0].span.start < pair[1].span.start));

    for (name, disposition) in [
        ("F", NamespaceValueAttachmentDisposition::AdmittedFunction),
        ("C", NamespaceValueAttachmentDisposition::AdmittedClass),
        (
            "LateIssue",
            NamespaceValueAttachmentDisposition::AdmittedFunction,
        ),
        (
            "Ambient",
            NamespaceValueAttachmentDisposition::AdmittedFunction,
        ),
    ] {
        let attachment = binder
            .namespace_value_attachment(binder.module, name)
            .expect("recovery attachment");
        assert_eq!(attachment.disposition, disposition, "{name}");
        assert_eq!(attachment.members.len(), 1, "{name}");
        assert!(attachment.members[0].value_storage.is_some(), "{name}");
    }
    let ambient = binder
        .namespaces
        .members()
        .find(|member| member.name.as_deref() == Some("implicit"))
        .expect("ambient default member");
    assert_eq!(ambient.publication, NamespacePublication::AmbientDefault);
}

#[test]
fn placement_requires_instantiated_namespaces_to_follow_ordinary_value_sequences() {
    let source = r#"
function Interposed(value: number): number;
namespace Interposed { export const tag: string = "tag"; }
function Interposed(value: number): number { return value; }

function Consecutive(value: number): number;
function Consecutive(value: number): number { return value; }
namespace Consecutive { export const tag: string = "tag"; }

class ClassFirst {}
namespace ClassFirst { export const tag: string = "tag"; }

declare function Ambient(value: number): number;
declare namespace Ambient { const first: string; }
declare function Ambient(value: string): string;
declare namespace Ambient { const second: string; }

namespace Reverse { export const tag: string = "tag"; }
function Reverse(): void {}
"#;
    let binder = bind(source, false);

    let interposed = &merge(&binder, "Interposed").placement_issues;
    assert_eq!(interposed.len(), 1);
    assert_eq!(&source[interposed[0].span.range()], "Interposed");
    assert!(merge(&binder, "Consecutive").placement_issues.is_empty());
    assert!(merge(&binder, "ClassFirst").placement_issues.is_empty());
    assert!(merge(&binder, "Ambient").placement_issues.is_empty());

    let reverse = &merge(&binder, "Reverse").placement_issues;
    assert_eq!(reverse.len(), 1);
    assert_eq!(&source[reverse[0].span.range()], "Reverse");
}

#[test]
fn reopenings_share_one_public_scope_and_dormant_symbol_anchors() {
    let source = "namespace N { export const publicOne = 1; const privateOne = 1; } namespace N { const privateTwo = 2; } namespace N { const privateThree = 3; }";
    let binder = bind(source, false);
    let namespace = binder
        .namespaces
        .namespaces()
        .find(|namespace| namespace.name == "N")
        .expect("N namespace");
    assert_eq!(namespace.fragments.len(), 3);
    let public = binder
        .graph
        .get(namespace.public_scope)
        .expect("public scope");
    assert_eq!(public.kind, ScopeKind::NamespacePublic);
    assert_eq!(public.parent, None);
    assert_eq!(public.symbols.len(), 1);
    assert!(public.lookup_local("publicOne").is_some());

    let private_scopes: Vec<_> = namespace
        .fragments
        .iter()
        .map(|fragment| {
            let fragment = binder
                .namespaces
                .fragment(*fragment)
                .expect("namespace fragment");
            assert_eq!(fragment.public_scope, namespace.public_scope);
            assert_eq!(fragment.lexical_parent, binder.module);
            let scope = binder
                .graph
                .get(fragment.private_scope)
                .expect("private scope");
            assert_eq!(scope.kind, ScopeKind::NamespacePrivate);
            assert_eq!(scope.parent, Some(binder.module));
            assert!(!scope.symbols.is_empty());
            assert_eq!(
                binder.graph.var_scope(fragment.private_scope),
                Some(fragment.private_scope)
            );
            fragment.private_scope
        })
        .collect();
    assert_eq!(private_scopes.len(), 3);
    assert!(private_scopes.windows(2).all(|pair| pair[0] != pair[1]));

    assert_eq!(
        binder
            .graph
            .get(binder.script_namespace_root)
            .and_then(|scope| scope.lookup_local("N")),
        Some(namespace.symbol)
    );
    let root_symbol = binder
        .symbols
        .get(namespace.symbol)
        .expect("root namespace symbol");
    assert_eq!(root_symbol.ns, Some(namespace.id));
    assert_eq!(
        root_symbol.value,
        binder.namespaces.standalone_value_storage(namespace.id)
    );
    assert_eq!(root_symbol.ty, None);
    assert_eq!(root_symbol.declarations.len(), 3);
    assert_eq!(
        binder.resolve_value(binder.module, "N"),
        Some(namespace.symbol)
    );
    assert_eq!(binder.resolve_type(binder.module, "N"), None);

    assert_eq!(binder.decl_count, 5);
    assert_eq!(
        binder.namespaces.aggregate_instance_state(namespace.id),
        Some(NamespaceInstanceState::Instantiated)
    );
    assert_eq!(
        binder.namespaces.standalone_value_storage(namespace.id),
        Some(ValueStorageId(4))
    );
    assert!(binder.type_groups.is_empty());
    for name in ["publicOne", "privateOne", "privateTwo", "privateThree"] {
        let declaration = binder
            .declarations
            .iter()
            .find(|declaration| &source[declaration.site.binding_span.range()] == name)
            .expect("body declaration");
        assert!(declaration.value_storage.is_some());
        assert_eq!(declaration.type_group, None);
        assert!(private_scopes.contains(&declaration.site.scope.expect("private context")));
    }
    let publications: Vec<_> = binder
        .namespaces
        .members()
        .filter(|member| {
            matches!(member.owner, NamespaceMemberOwner::Fragment(_))
                && member
                    .name
                    .as_deref()
                    .is_some_and(|name| name.contains("One"))
        })
        .map(|member| member.publication)
        .collect();
    assert!(publications.contains(&NamespacePublication::Explicit));
    assert!(publications.contains(&NamespacePublication::Private));

    let global = binder
        .graph
        .get(binder.compilation_global)
        .expect("one compilation-global scope");
    assert_eq!(global.kind, ScopeKind::CompilationGlobal);
    assert_eq!(global.parent, Some(binder.prelude_module));
    assert_eq!(global.lookup_local("N"), None);
    let script_root = binder
        .graph
        .get(binder.script_namespace_root)
        .expect("one script namespace root scope");
    assert_eq!(script_root.kind, ScopeKind::ScriptNamespaceRoot);
    assert_eq!(script_root.parent, Some(binder.compilation_global));
    assert_eq!(script_root.lookup_local("N"), Some(namespace.symbol));
    assert_eq!(
        binder.graph.var_scope(binder.compilation_global),
        Some(binder.compilation_global)
    );
    assert_eq!(
        binder.graph.var_scope(binder.script_namespace_root),
        Some(binder.script_namespace_root)
    );
    assert_eq!(
        binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.parent),
        Some(binder.script_namespace_root)
    );
}

#[test]
fn standalone_namespace_storage_is_group_owned_dormant_and_exclusion_aware() {
    let source = r#"
const lexical = 0;
namespace Reopened { export interface Shape {} }
namespace Reopened { export const value: number = 1; }
namespace EqualLeft { export const value: number = 1; }
namespace EqualRight { export const value: number = 1; }
namespace TypeOnly { export interface Shape {} }
declare namespace Ambient { const value: number; }
function FunctionOwner(): void {}
namespace FunctionOwner { export const value: number = 1; }
class ClassOwner {}
namespace ClassOwner { export const value: number = 1; }
enum Rejected { Value }
namespace Rejected { export const value: number = 1; }
function Recovery(): void {}
namespace Recovery { export const value: number = 1; }
enum Recovery { Value }
namespace Parent {
    export namespace Child {
        export namespace Grandchild { export const value: number = 1; }
    }
}
export {};
declare global {
    namespace GlobalRoot {
        export const value: number;
        export namespace Nested { export const value: number; }
    }
}
"#;
    let binder = bind(source, false);
    let namespace = |name: &str| {
        binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == name)
            .unwrap_or_else(|| panic!("{name} namespace"))
    };

    let reopened = namespace("Reopened");
    assert_eq!(
        reopened
            .fragments
            .iter()
            .filter_map(|fragment| binder.namespaces.fragment(*fragment))
            .map(|fragment| fragment.instance_state)
            .collect::<Vec<_>>(),
        [
            NamespaceInstanceState::NonInstantiated,
            NamespaceInstanceState::Instantiated,
        ]
    );
    assert_eq!(
        binder.namespaces.aggregate_instance_state(reopened.id),
        Some(NamespaceInstanceState::Instantiated)
    );
    assert!(binder
        .namespaces
        .standalone_value_storage(reopened.id)
        .is_some());

    let equal_left = binder
        .namespaces
        .standalone_value_storage(namespace("EqualLeft").id)
        .expect("left namespace storage");
    let equal_right = binder
        .namespaces
        .standalone_value_storage(namespace("EqualRight").id)
        .expect("right namespace storage");
    assert_ne!(equal_left, equal_right);

    for name in ["Parent", "Child", "Grandchild", "Ambient"] {
        let namespace = namespace(name);
        assert_eq!(
            binder.namespaces.aggregate_instance_state(namespace.id),
            Some(NamespaceInstanceState::Instantiated),
            "{name} aggregate state"
        );
        assert!(
            binder
                .namespaces
                .standalone_value_storage(namespace.id)
                .is_some(),
            "{name} standalone storage"
        );
    }
    assert_eq!(
        binder
            .namespaces
            .aggregate_instance_state(namespace("TypeOnly").id),
        Some(NamespaceInstanceState::NonInstantiated)
    );
    for name in [
        "TypeOnly",
        "FunctionOwner",
        "ClassOwner",
        "Rejected",
        "Recovery",
        "GlobalRoot",
        "Nested",
    ] {
        assert_eq!(
            binder
                .namespaces
                .standalone_value_storage(namespace(name).id),
            None,
            "{name} must not own standalone storage"
        );
    }

    for name in ["FunctionOwner", "ClassOwner", "Recovery"] {
        let namespace = namespace(name);
        assert!(binder
            .symbols
            .get(namespace.symbol)
            .is_some_and(|symbol| symbol.value.is_some()));
    }
    for name in [
        "Reopened",
        "EqualLeft",
        "EqualRight",
        "Ambient",
        "Parent",
        "Child",
        "Grandchild",
    ] {
        let namespace = namespace(name);
        assert_eq!(
            binder
                .symbols
                .get(namespace.symbol)
                .and_then(|symbol| symbol.value),
            binder.namespaces.standalone_value_storage(namespace.id),
            "{name} root symbol uses its namespace-owned storage"
        );
    }
    assert!(binder
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Namespace)
        .all(|declaration| declaration.value_storage.is_none()));

    let lexical_max = binder
        .declarations
        .iter()
        .filter_map(|declaration| declaration.value_storage)
        .map(|storage| storage.0)
        .max()
        .expect("ordinary lexical storage");
    let dormant = binder
        .namespaces
        .namespaces()
        .filter_map(|namespace| binder.namespaces.standalone_value_storage(namespace.id))
        .collect::<Vec<_>>();
    assert_eq!(dormant.len(), 7);
    assert!(dormant.iter().all(|storage| storage.0 > lexical_max));
    assert_eq!(
        dormant
            .iter()
            .map(|storage| storage.0)
            .max()
            .map(|id| id + 1),
        Some(binder.decl_count)
    );
}

#[test]
fn standalone_namespace_storage_order_uses_stable_source_keys() {
    fn storage_map(reverse_input: bool) -> Vec<(String, ValueStorageId)> {
        let prelude_allocator = Allocator::default();
        let first_allocator = Allocator::default();
        let second_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let first = Parser::new(
            &first_allocator,
            "const firstLexical = 1; namespace First { export const value = 1; }",
            SourceType::ts(),
        )
        .parse();
        let second = Parser::new(
            &second_allocator,
            "const secondLexical = 1; namespace Second { export const value = 1; }",
            SourceType::ts(),
        )
        .parse();
        assert!(!first.panicked && !second.panicked);

        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let units = if reverse_input {
            [
                (&second.program, SourceUnitKey(20)),
                (&first.program, SourceUnitKey(10)),
            ]
        } else {
            [
                (&first.program, SourceUnitKey(10)),
                (&second.program, SourceUnitKey(20)),
            ]
        };
        let mut last_module = None;
        for (original_module, (program, source)) in units.into_iter().enumerate() {
            let unit = CompilationUnit {
                source,
                origin: CompilationOrigin::User(OriginalModuleOrdinal::new(original_module)),
                binding: ModuleBindingContext::for_program(
                    program,
                    SourceFileKind::ImplementationTs,
                ),
            };
            let (module, _) = builder.add_module(program, &[], unit);
            last_module = Some(module);
        }
        let binder = builder.finish(last_module.expect("one project module"));
        ["First", "Second"]
            .into_iter()
            .map(|name| {
                let namespace = binder
                    .namespaces
                    .namespaces()
                    .find(|namespace| namespace.name == name)
                    .expect("project namespace");
                (
                    name.to_string(),
                    binder
                        .namespaces
                        .standalone_value_storage(namespace.id)
                        .expect("standalone namespace storage"),
                )
            })
            .collect()
    }

    assert_eq!(storage_map(false), storage_map(true));
}

#[test]
fn script_namespace_root_shares_only_script_roots_and_isolates_other_owners() {
    fn bind_project(sources: &[(&str, SourceUnitKey)]) -> (Binder, Vec<ScopeId>) {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let allocators = sources
            .iter()
            .map(|_| Allocator::default())
            .collect::<Vec<_>>();
        let parsed = sources
            .iter()
            .zip(&allocators)
            .map(|((source, _), allocator)| {
                let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
                assert!(!parsed.panicked, "parse failed: {source}");
                parsed
            })
            .collect::<Vec<_>>();
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let mut modules = Vec::new();
        for (original_module, ((_, source), parsed)) in sources.iter().zip(&parsed).enumerate() {
            let unit = CompilationUnit {
                source: *source,
                origin: CompilationOrigin::User(OriginalModuleOrdinal::new(original_module)),
                binding: ModuleBindingContext::for_program(
                    &parsed.program,
                    SourceFileKind::ImplementationTs,
                ),
            };
            let (module, _) = builder.add_module(&parsed.program, &[], unit);
            modules.push(module);
        }
        let binder = builder.finish(*modules.last().expect("one project module"));
        (binder, modules)
    }

    fn shared_script_projection(reverse: bool) -> (ValueStorageId, Vec<SourceUnitKey>) {
        let first = (
            "namespace Shared { export const first: number = 1; }",
            SourceUnitKey(10),
        );
        let second = (
            "namespace Shared { export const second: string = \"second\"; }",
            SourceUnitKey(20),
        );
        let sources = if reverse {
            [second, first]
        } else {
            [first, second]
        };
        let (binder, _) = bind_project(&sources);
        let matches = binder
            .namespaces
            .namespaces()
            .filter(|namespace| namespace.name == "Shared")
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "script reopenings share one identity");
        let namespace = matches[0];
        assert_eq!(
            namespace.owner,
            NamespaceOwner::Lexical(binder.script_namespace_root)
        );
        let public = binder
            .graph
            .get(namespace.public_scope)
            .expect("shared script namespace surface");
        assert!(public.lookup_local("first").is_some());
        assert!(public.lookup_local("second").is_some());
        (
            binder
                .namespaces
                .standalone_value_storage(namespace.id)
                .expect("shared script root storage"),
            namespace
                .fragments
                .iter()
                .filter_map(|fragment| binder.namespaces.fragment(*fragment))
                .map(|fragment| fragment.source)
                .collect(),
        )
    }

    let forward = shared_script_projection(false);
    let reverse = shared_script_projection(true);
    assert_eq!(forward, reverse);
    assert_eq!(forward.1, [SourceUnitKey(10), SourceUnitKey(20)]);

    let sources = [
            (
                "namespace Shared { export const scriptOnly: number = 1; }",
                SourceUnitKey(10),
            ),
            (
                "export {}; namespace Shared { export const moduleOne: number = 1; }",
                SourceUnitKey(20),
            ),
            (
                "export {}; namespace Shared { export const moduleTwo: number = 2; }",
                SourceUnitKey(30),
            ),
            (
                "export {}; declare global { namespace Shared { interface GlobalOnly {} } }",
                SourceUnitKey(40),
            ),
            (
                "function FunctionPair(): void {} namespace FunctionPair { export const tag: number = 1; } class ClassPair {} namespace ClassPair { export const tag: number = 1; }",
                SourceUnitKey(50),
            ),
            (
                "export {}; declare global { namespace Blocked { const value: number; } }",
                SourceUnitKey(60),
            ),
        ];
    let (binder, modules) = bind_project(&sources);
    assert!(modules.iter().all(|module| binder
        .graph
        .get(*module)
        .is_some_and(|scope| scope.parent == Some(binder.script_namespace_root))));
    let script_root = binder
        .graph
        .get(binder.script_namespace_root)
        .expect("script namespace root");
    let compilation_global = binder
        .graph
        .get(binder.compilation_global)
        .expect("compilation global");
    let prelude = binder
        .graph
        .get(binder.prelude_module)
        .expect("prelude module");
    assert_eq!(script_root.kind, ScopeKind::ScriptNamespaceRoot);
    assert_eq!(script_root.parent, Some(binder.compilation_global));
    assert_eq!(compilation_global.parent, Some(binder.prelude_module));
    assert_eq!(prelude.parent, None);

    let script = binder
        .namespaces
        .namespaces()
        .find(|namespace| {
            namespace.name == "Shared"
                && namespace.owner == NamespaceOwner::Lexical(binder.script_namespace_root)
        })
        .expect("script Shared");
    let module_one = binder
        .namespaces
        .namespaces()
        .find(|namespace| {
            namespace.name == "Shared" && namespace.owner == NamespaceOwner::Lexical(modules[1])
        })
        .expect("first module-local Shared");
    let module_two = binder
        .namespaces
        .namespaces()
        .find(|namespace| {
            namespace.name == "Shared" && namespace.owner == NamespaceOwner::Lexical(modules[2])
        })
        .expect("second module-local Shared");
    let global = binder
        .namespaces
        .namespaces()
        .find(|namespace| {
            namespace.name == "Shared" && namespace.owner == NamespaceOwner::CompilationGlobal
        })
        .expect("declare-global Shared");
    let identities = [script.id, module_one.id, module_two.id, global.id];
    assert!(identities
        .iter()
        .enumerate()
        .all(|(index, identity)| identities[index + 1..]
            .iter()
            .all(|other| identity != other)));
    let storages = [script, module_one, module_two].map(|namespace| {
        binder
            .namespaces
            .standalone_value_storage(namespace.id)
            .expect("instantiated standalone owner")
    });
    assert_ne!(storages[0], storages[1]);
    assert_ne!(storages[0], storages[2]);
    assert_ne!(storages[1], storages[2]);
    assert_eq!(binder.namespaces.standalone_value_storage(global.id), None);
    assert_eq!(script_root.lookup_local("Shared"), Some(script.symbol));
    assert_eq!(
        compilation_global.lookup_local("Shared"),
        Some(global.symbol)
    );
    assert_eq!(
        binder
            .graph
            .get(modules[1])
            .and_then(|scope| scope.lookup_local("Shared")),
        Some(module_one.symbol)
    );
    assert_eq!(
        binder
            .graph
            .get(modules[2])
            .and_then(|scope| scope.lookup_local("Shared")),
        Some(module_two.symbol)
    );
    for (namespace, own, foreign) in [
        (script, "scriptOnly", "GlobalOnly"),
        (global, "GlobalOnly", "scriptOnly"),
    ] {
        let public = binder
            .graph
            .get(namespace.public_scope)
            .expect("isolated namespace surface");
        assert!(public.lookup_local(own).is_some());
        assert_eq!(public.lookup_local(foreign), None);
    }

    for name in ["FunctionPair", "ClassPair"] {
        let namespace = binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == name)
            .unwrap_or_else(|| panic!("{name} namespace"));
        assert_eq!(namespace.owner, NamespaceOwner::Lexical(modules[4]));
        assert_eq!(
            binder.namespaces.standalone_value_storage(namespace.id),
            None
        );
        assert_eq!(script_root.lookup_local(name), None);
    }

    assert_eq!(script_root.lookup_local("Blocked"), None);
    let blocked_symbol = compilation_global.lookup_local("Blocked");
    assert!(blocked_symbol.is_some());
    let blocked_global = binder
        .namespaces
        .globals()
        .find(|global| global.source == SourceUnitKey(60))
        .expect("blocked global augmentation");
    assert_eq!(
        binder
            .graph
            .get(blocked_global.overlay_scope)
            .and_then(|scope| scope.lookup_local("Blocked")),
        blocked_symbol
    );
    let blocker = blocked_symbol
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("global blocker");
    assert!(
        blocker.blocks_value_lookup
            && !blocker.blocks_type_lookup
            && !blocker.blocks_namespace_lookup
            && blocker.value.is_none()
            && blocker.ns.is_some()
    );
}

#[test]
fn namespace_private_lookup_orders_local_public_and_lexical_in_both_source_orders() {
    fn assert_lookup(source: &str) {
        let binder = bind(source, false);

        let namespace = binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == "N")
            .expect("N namespace");
        let public_shared = binder
            .graph
            .get(namespace.public_scope)
            .and_then(|scope| scope.lookup_local("Shared"))
            .expect("shared public type");
        let lexical_shared = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("Shared"))
            .expect("lexical shared symbol");
        assert_ne!(public_shared, lexical_shared);

        let provider_scope = namespace
            .fragments
            .iter()
            .filter_map(|fragment| binder.namespaces.fragment(*fragment))
            .find_map(|fragment| {
                binder
                    .graph
                    .get(fragment.private_scope)
                    .and_then(|scope| scope.lookup_local("OnlyProvider"))
                    .map(|_| fragment.private_scope)
            })
            .expect("provider private scope");
        let consumer_scope = namespace
            .fragments
            .iter()
            .filter_map(|fragment| binder.namespaces.fragment(*fragment))
            .find_map(|fragment| {
                binder
                    .graph
                    .get(fragment.private_scope)
                    .and_then(|scope| scope.lookup_local("ConsumerLocal"))
                    .map(|_| fragment.private_scope)
            })
            .expect("consumer private scope");
        assert_ne!(provider_scope, consumer_scope);
        for scope in [provider_scope, consumer_scope] {
            let scope = binder.graph.get(scope).expect("namespace private scope");
            assert_eq!(scope.parent, Some(binder.module));
            assert_eq!(scope.namespace_public, Some(namespace.public_scope));
        }

        assert_eq!(
            binder.resolve_type(provider_scope, "Shared"),
            Some(public_shared)
        );
        assert_eq!(
            binder.resolve_type(consumer_scope, "Shared"),
            Some(public_shared)
        );
        assert_eq!(
            binder.resolve_value(consumer_scope, "Shared"),
            Some(lexical_shared),
            "a public type-only symbol does not block lexical value lookup"
        );
        assert_eq!(
            binder.graph.resolve(consumer_scope, "Shared"),
            Some(public_shared)
        );

        let private_provider = binder
            .graph
            .get(provider_scope)
            .and_then(|scope| scope.lookup_local("OnlyProvider"))
            .expect("provider private symbol");
        let lexical_provider = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("OnlyProvider"))
            .expect("lexical provider fallback");
        assert_ne!(private_provider, lexical_provider);
        assert_eq!(
            binder.resolve_type(provider_scope, "OnlyProvider"),
            Some(private_provider)
        );
        assert_eq!(
            binder.resolve_type(consumer_scope, "OnlyProvider"),
            Some(lexical_provider),
            "another fragment's private symbol must remain invisible"
        );

        let shared_group = binder
            .symbols
            .get(public_shared)
            .and_then(|symbol| symbol.ty)
            .expect("shared public group");
        assert_eq!(
            binder.resolve_qualified_type_path(binder.module, &["N", "Shared"]),
            QualifiedTypePathResolution::TypeGroup(shared_group)
        );
        assert_eq!(
            binder.resolve_qualified_type_path(binder.module, &["N", "OnlyProvider"]),
            QualifiedTypePathResolution::MissingMember { segment: 1 },
            "qualified lookup cannot enter private scopes or fall back to the module"
        );

        let value_namespace = binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == "F")
            .expect("F namespace");
        let public_value = binder
            .graph
            .get(value_namespace.public_scope)
            .and_then(|scope| scope.lookup_local("SharedValue"))
            .expect("shared public value");
        let lexical_value_type = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("SharedValue"))
            .expect("lexical value type");
        let value_provider_scope = value_namespace
            .fragments
            .iter()
            .filter_map(|fragment| binder.namespaces.fragment(*fragment))
            .find_map(|fragment| {
                binder
                    .graph
                    .get(fragment.private_scope)
                    .and_then(|scope| scope.lookup_local("ValueFallback"))
                    .map(|_| fragment.private_scope)
            })
            .expect("value provider scope");
        let value_consumer_scope = value_namespace
            .fragments
            .iter()
            .filter_map(|fragment| binder.namespaces.fragment(*fragment))
            .find_map(|fragment| {
                binder
                    .graph
                    .get(fragment.private_scope)
                    .and_then(|scope| scope.lookup_local("ConsumerValue"))
                    .map(|_| fragment.private_scope)
            })
            .expect("value consumer scope");
        let provider_shared_value = binder
            .resolve_value(value_provider_scope, "SharedValue")
            .expect("provider-local exported value binding");
        assert_eq!(
            binder
                .symbols
                .get(provider_shared_value)
                .and_then(|symbol| symbol.value),
            binder
                .symbols
                .get(public_value)
                .and_then(|symbol| symbol.value),
            "the declaration fragment's local binding and public symbol share storage"
        );
        assert_eq!(
            binder.resolve_value(value_consumer_scope, "SharedValue"),
            Some(public_value)
        );
        assert_eq!(
            binder.resolve_type(value_consumer_scope, "SharedValue"),
            Some(lexical_value_type),
            "a public value-only symbol does not block lexical type lookup"
        );
        assert_eq!(
            binder.resolve_type(value_provider_scope, "SharedValue"),
            Some(lexical_value_type),
            "the provider's local value binding does not block lexical type lookup"
        );
        let private_value_fallback = binder
            .graph
            .get(value_provider_scope)
            .and_then(|scope| scope.lookup_local("ValueFallback"))
            .expect("private value fallback");
        let lexical_value_fallback = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("ValueFallback"))
            .expect("lexical value fallback");
        assert_eq!(
            binder.resolve_value(value_provider_scope, "ValueFallback"),
            Some(private_value_fallback)
        );
        assert_eq!(
            binder.resolve_value(value_consumer_scope, "ValueFallback"),
            Some(lexical_value_fallback)
        );

        let ambient = binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == "AmbientN")
            .expect("ambient namespace");
        let ambient_item = binder
            .graph
            .get(ambient.public_scope)
            .and_then(|scope| scope.lookup_local("Item"))
            .expect("ambient-default public item");
        let box_scope = binder
            .graph
            .get(ambient.public_scope)
            .and_then(|scope| scope.lookup_local("Box"))
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty)
            .and_then(|group| binder.type_groups.get(group))
            .and_then(|group| group.fragments.first())
            .map(|fragment| fragment.scope)
            .expect("ambient box fragment scope");
        assert_eq!(binder.resolve_type(box_scope, "Item"), Some(ambient_item));
    }

    let provider_first = r#"
interface Shared {}
const Shared = 0;
interface OnlyProvider {}
interface SharedValue {}
const ValueFallback = 0;
namespace N {
    export interface Shared { public: true }
    interface OnlyProvider { private: true }
}
namespace N {
    interface ConsumerLocal {}
    export interface Consumer { shared: Shared; fallback: OnlyProvider }
}
function F(): void {}
namespace F { export const SharedValue = 1; const ValueFallback = 1; }
namespace F { const ConsumerValue = 2; }
declare namespace AmbientN { interface Item { value: number } }
declare namespace AmbientN { interface Box { item: Item } }
"#;
    let consumer_first = r#"
interface Shared {}
const Shared = 0;
interface OnlyProvider {}
interface SharedValue {}
const ValueFallback = 0;
namespace N {
    interface ConsumerLocal {}
    export interface Consumer { shared: Shared; fallback: OnlyProvider }
}
namespace N {
    export interface Shared { public: true }
    interface OnlyProvider { private: true }
}
function F(): void {}
namespace F { const ConsumerValue = 2; }
namespace F { export const SharedValue = 1; const ValueFallback = 1; }
declare namespace AmbientN { interface Box { item: Item } }
declare namespace AmbientN { interface Item { value: number } }
"#;

    assert_lookup(provider_first);
    assert_lookup(consumer_first);
}

#[test]
fn namespace_type_groups_use_no_legacy_storage_and_preserve_public_private_identity() {
    let binder = bind(
            "interface Top {} namespace N { export interface Shared { first: number } interface Hidden { first: number } export type Alias = number; export class Box {} } namespace N { export interface Shared { second: string } interface Hidden { second: string } }",
            false,
        );
    assert_eq!(binder.type_groups.len(), 6);

    let top = binder
        .graph
        .get(binder.module)
        .and_then(|scope| scope.lookup_local("Top"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .and_then(|group| binder.type_groups.get(group))
        .expect("top-level group");
    assert_eq!(top.fragments.len(), 1);
    assert_eq!(top.fragments[0].source, SourceUnitKey::SINGLE_SOURCE);

    let namespace = binder
        .namespaces
        .namespaces()
        .find(|namespace| namespace.name == "N")
        .expect("N namespace");
    let public = binder
        .graph
        .get(namespace.public_scope)
        .expect("N public scope");
    let reopening_scopes = namespace
        .fragments
        .iter()
        .map(|fragment| {
            binder
                .namespaces
                .fragment(*fragment)
                .expect("namespace fragment")
                .private_scope
        })
        .collect::<Vec<_>>();
    let shared_group = public
        .lookup_local("Shared")
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .expect("shared public type group");
    let shared = binder
        .type_groups
        .get(shared_group)
        .expect("shared group row");
    assert_eq!(shared.fragments.len(), 2);
    assert_eq!(
        shared
            .fragments
            .iter()
            .map(|fragment| fragment.scope)
            .collect::<Vec<_>>(),
        reopening_scopes
    );
    assert!(shared.fragments.iter().all(|fragment| {
        fragment.source == SourceUnitKey::SINGLE_SOURCE
            && fragment.site.scope == Some(fragment.scope)
    }));
    assert!(shared.fragments.windows(2).all(|pair| {
        (
            pair[0].source,
            pair[0].site.declaration_span.start,
            pair[0].declaration.0,
        ) < (
            pair[1].source,
            pair[1].site.declaration_span.start,
            pair[1].declaration.0,
        )
    }));

    for name in ["Alias", "Box"] {
        let group = public
            .lookup_local(name)
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty)
            .and_then(|group| binder.type_groups.get(group))
            .expect("public namespace type group");
        assert_eq!(group.fragments.len(), 1);
        assert_eq!(group.fragments[0].scope, reopening_scopes[0]);
    }

    let hidden_groups = namespace
        .fragments
        .iter()
        .map(|fragment| {
            let fragment = binder
                .namespaces
                .fragment(*fragment)
                .expect("namespace fragment");
            let group = binder
                .graph
                .get(fragment.private_scope)
                .and_then(|scope| scope.lookup_local("Hidden"))
                .and_then(|symbol| binder.symbols.get(symbol))
                .and_then(|symbol| symbol.ty)
                .expect("fragment-private group");
            let row = binder.type_groups.get(group).expect("private group row");
            assert_eq!(row.fragments.len(), 1);
            assert_eq!(row.fragments[0].scope, fragment.private_scope);
            group
        })
        .collect::<Vec<_>>();
    assert_eq!(hidden_groups.len(), 2);
    assert_ne!(hidden_groups[0], hidden_groups[1]);
    assert_eq!(
        binder
            .declarations
            .iter()
            .filter(|declaration| { declaration.type_group.is_some() })
            .count(),
        7,
        "every exact type-bearing declaration points at its group, including both Shared fragments"
    );
}

#[test]
fn public_namespace_group_target_is_distinct_from_fragment_lexical_scopes() {
    let binder = bind(
            "namespace N { interface PrivateHelper { value: number } export interface Public { helper: PrivateHelper } } namespace N { export interface Public { reopened: true } }",
            false,
        );
    let namespace = binder
        .namespaces
        .namespaces()
        .find(|namespace| namespace.name == "N")
        .expect("N namespace");
    let private_scopes = namespace
        .fragments
        .iter()
        .map(|fragment| {
            binder
                .namespaces
                .fragment(*fragment)
                .expect("namespace fragment")
                .private_scope
        })
        .collect::<Vec<_>>();
    assert_eq!(private_scopes.len(), 2);
    assert_ne!(private_scopes[0], private_scopes[1]);

    let public_symbol = binder
        .graph
        .get(namespace.public_scope)
        .and_then(|scope| scope.lookup_local("Public"))
        .expect("Public is published in the namespace target scope");
    let public_group = binder
        .symbols
        .get(public_symbol)
        .and_then(|symbol| symbol.ty)
        .and_then(|group| binder.type_groups.get(group))
        .expect("Public group");
    assert_eq!(
        public_group
            .fragments
            .iter()
            .map(|fragment| fragment.scope)
            .collect::<Vec<_>>(),
        private_scopes
    );
    assert!(public_group
        .fragments
        .iter()
        .all(|fragment| { fragment.site.scope == Some(fragment.scope) }));
    assert!(private_scopes.iter().all(|scope| {
        binder
            .graph
            .get(*scope)
            .is_some_and(|scope| scope.lookup_local("Public").is_none())
    }));

    let helper_group = binder
        .graph
        .get(private_scopes[0])
        .and_then(|scope| scope.lookup_local("PrivateHelper"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .and_then(|group| binder.type_groups.get(group))
        .expect("private helper group");
    assert_eq!(helper_group.fragments.len(), 1);
    assert_eq!(helper_group.fragments[0].scope, private_scopes[0]);
    assert_eq!(
        helper_group.fragments[0].site.scope,
        Some(private_scopes[0])
    );
}

#[test]
fn namespace_public_type_groups_are_source_ordered_across_global_reopenings() {
    fn fragment_sources(reverse_input: bool) -> Vec<SourceUnitKey> {
        let prelude_allocator = Allocator::default();
        let first_allocator = Allocator::default();
        let second_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let first = Parser::new(
            &first_allocator,
            "export {}; declare global { namespace N { interface Shared { first: number } } }",
            SourceType::ts(),
        )
        .parse();
        let second = Parser::new(
            &second_allocator,
            "export {}; declare global { namespace N { interface Shared { second: string } } }",
            SourceType::ts(),
        )
        .parse();
        assert!(!first.panicked && !second.panicked);

        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let units = if reverse_input {
            [
                (&second.program, SourceUnitKey(20)),
                (&first.program, SourceUnitKey(10)),
            ]
        } else {
            [
                (&first.program, SourceUnitKey(10)),
                (&second.program, SourceUnitKey(20)),
            ]
        };
        let mut last_module = None;
        for (original_module, (program, source)) in units.into_iter().enumerate() {
            let unit = CompilationUnit {
                source,
                origin: CompilationOrigin::User(OriginalModuleOrdinal::new(original_module)),
                binding: ModuleBindingContext::for_program(
                    program,
                    SourceFileKind::ImplementationTs,
                ),
            };
            let (module, _) = builder.add_module(program, &[], unit);
            last_module = Some(module);
        }
        let binder = builder.finish(last_module.expect("one project module"));
        let namespace = binder
            .namespaces
            .namespaces()
            .find(|namespace| {
                namespace.owner == NamespaceOwner::CompilationGlobal && namespace.name == "N"
            })
            .expect("shared compilation-global namespace");
        let group = binder
            .graph
            .get(namespace.public_scope)
            .and_then(|scope| scope.lookup_local("Shared"))
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty)
            .and_then(|group| binder.type_groups.get(group))
            .expect("shared global namespace type group");
        group
            .fragments
            .iter()
            .map(|fragment| fragment.source)
            .collect()
    }

    assert_eq!(
        fragment_sources(false),
        [SourceUnitKey(10), SourceUnitKey(20)]
    );
    assert_eq!(
        fragment_sources(true),
        [SourceUnitKey(10), SourceUnitKey(20)]
    );
}

#[test]
fn qualified_type_path_resolver_classifies_topology_slots_and_aliases() {
    let source = r#"
type AliasRoot = { alias: true };
class ClassRoot {}
const ValueRoot = 1;
import RootImport = require("pkg");
enum RootEnum { Member }
namespace TopologyRoot {
  export const ValueMiddle = 1;
  export interface TypeMiddle { type: true }
  export class ClassMiddle {}
  export namespace NamespaceLeaf {}
  export interface ParentLeaf { parent: true }
  export namespace Child {}
  export namespace Nested { export interface Leaf { nested: true } }
}
let forward: ForwardRoot.Later;
declare namespace ForwardRoot { interface Earlier { earlier: true } }
declare namespace ForwardRoot { interface Later { later: true } }
namespace Dotted.Root { export interface Leaf { dotted: true } }
declare namespace AmbientAliasList {
  interface HiddenType { hidden: true }
  interface HiddenExplicitType { explicit: true }
  namespace HiddenChild { interface Leaf { child: true } }
  const HiddenValue: 1;
  export {
    HiddenType as PublicType,
    type HiddenExplicitType as ExplicitTypeOnly,
    HiddenChild as PublicChild,
    HiddenValue as PublicValue,
  };
  interface AfterList { after: true }
}
declare namespace DeferredRoot {
  export import External = require("pkg");
  export enum Enumeration { Member }
  export { Missing as Unresolved };
}
"#;
    let binder = bind(source, false);

    let success = |segments: &[&str]| {
        assert!(
            matches!(
                binder.resolve_qualified_type_path(binder.module, segments),
                QualifiedTypePathResolution::TypeGroup(_)
            ),
            "expected a type group for {segments:?}"
        );
    };
    for path in [
        &["TopologyRoot", "TypeMiddle"][..],
        &["TopologyRoot", "Nested", "Leaf"][..],
        &["ForwardRoot", "Later"][..],
        &["Dotted", "Root", "Leaf"][..],
        &["AmbientAliasList", "PublicType"][..],
        &["AmbientAliasList", "ExplicitTypeOnly"][..],
        &["AmbientAliasList", "PublicChild", "Leaf"][..],
        &["DeferredRoot", "Enumeration"][..],
    ] {
        success(path);
    }

    for (path, expected) in [
        (
            &["MissingRoot", "Member"][..],
            QualifiedTypePathResolution::MissingRoot { segment: 0 },
        ),
        (
            &["ValueRoot", "Member"][..],
            QualifiedTypePathResolution::MissingRoot { segment: 0 },
        ),
        (
            &["AliasRoot", "Member"][..],
            QualifiedTypePathResolution::TypeOnlyRoot { segment: 0 },
        ),
        (
            &["ClassRoot", "Member"][..],
            QualifiedTypePathResolution::TypeOnlyRoot { segment: 0 },
        ),
        (
            &["RootImport", "Member"][..],
            QualifiedTypePathResolution::Deferred {
                segment: 0,
                reason: QualifiedTypePathDeferredReason::Import,
            },
        ),
        (
            &["RootEnum", "Member"][..],
            QualifiedTypePathResolution::Deferred {
                segment: 0,
                reason: QualifiedTypePathDeferredReason::Enum,
            },
        ),
        (
            &["TopologyRoot", "Missing", "Leaf"][..],
            QualifiedTypePathResolution::MissingMember { segment: 1 },
        ),
        (
            &["TopologyRoot", "ValueMiddle", "Leaf"][..],
            QualifiedTypePathResolution::MissingMember { segment: 1 },
        ),
        (
            &["TopologyRoot", "TypeMiddle", "Leaf"][..],
            QualifiedTypePathResolution::TypeOnlyIntermediate { segment: 1 },
        ),
        (
            &["TopologyRoot", "ClassMiddle", "Leaf"][..],
            QualifiedTypePathResolution::TypeOnlyIntermediate { segment: 1 },
        ),
        (
            &["TopologyRoot", "Child", "ParentLeaf"][..],
            QualifiedTypePathResolution::MissingMember { segment: 2 },
        ),
        (
            &["TopologyRoot", "NamespaceLeaf"][..],
            QualifiedTypePathResolution::MissingMember { segment: 1 },
        ),
        (
            &["TopologyRoot", "ValueMiddle"][..],
            QualifiedTypePathResolution::ValueOnlyLeaf { segment: 1 },
        ),
        (
            &["AmbientAliasList", "PublicValue"][..],
            QualifiedTypePathResolution::ValueOnlyLeaf { segment: 1 },
        ),
        (
            &["AmbientAliasList", "HiddenType"][..],
            QualifiedTypePathResolution::MissingMember { segment: 1 },
        ),
        (
            &["AmbientAliasList", "AfterList"][..],
            QualifiedTypePathResolution::MissingMember { segment: 1 },
        ),
        (
            &["DeferredRoot", "External"][..],
            QualifiedTypePathResolution::Deferred {
                segment: 1,
                reason: QualifiedTypePathDeferredReason::Import,
            },
        ),
        (
            &["DeferredRoot", "Unresolved"][..],
            QualifiedTypePathResolution::Unavailable { segment: 1 },
        ),
    ] {
        assert_eq!(
            binder.resolve_qualified_type_path(binder.module, path),
            expected,
            "unexpected resolution for {path:?}"
        );
    }
}

#[test]
fn ambient_export_alias_outputs_are_not_local_bindings_but_namespace_aliases_keep_type_groups() {
    let source = r#"
declare namespace AliasOutputForward {
  interface Local { forward: true }
  export { Local as A };
  export { A as B };
}
declare namespace AliasOutputReverse {
  interface Local { reverse: true }
  export { A as B };
  export { Local as A };
}
declare namespace GenuineLocalControl {
  interface Local { aliasTarget: true }
  export { Local as A };
  export { A as B };
  interface A { genuineLocal: true }
}
declare namespace A {
  namespace N { export interface X {} }
  export { type N as TN };
}
"#;
    let binder = bind(source, false);

    let failures = binder.local_ambient_export_alias_failures();
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert_eq!(
        failures
            .iter()
            .map(|failure| (
                failure.local_name.as_str(),
                &source[failure.local_span.range()],
                failure.kind,
            ))
            .collect::<Vec<_>>(),
        [
            ("A", "A", LocalAmbientExportAliasFailureKind::NonLocal),
            ("A", "A", LocalAmbientExportAliasFailureKind::NonLocal),
        ],
        "both source orders must reject an alias-only local name"
    );

    for path in [
        &["AliasOutputForward", "B"][..],
        &["AliasOutputReverse", "B"][..],
    ] {
        assert_eq!(
            binder.resolve_qualified_type_path(binder.module, path),
            QualifiedTypePathResolution::Unavailable { segment: 1 },
            "a diagnosed alias output must not publish a qualified endpoint: {path:?}"
        );
    }

    for path in [&["GenuineLocalControl", "B"][..], &["A", "TN", "X"][..]] {
        assert!(
                matches!(
                    binder.resolve_qualified_type_path(binder.module, path),
                    QualifiedTypePathResolution::TypeGroup(_)
                ),
                "a genuine declaration or type-only namespace alias must preserve its type group: {path:?}"
            );
    }
}

#[test]
fn import_namespace_compositions_resolve_only_concrete_named_and_default_paths() {
    fn bind_project_source(source: &str, import_name: &str, type_only: bool) -> Binder {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "parse failed: {source}");
        let import_span = parsed
            .program
            .body
            .iter()
            .find_map(|statement| match statement {
                Statement::ImportDeclaration(import) => import
                    .specifiers
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .find(|specifier| specifier.local().name == import_name)
                    .map(|specifier| Span::from_oxc(specifier.local().span)),
                Statement::TSImportEqualsDeclaration(import) if import.id.name == import_name => {
                    Some(Span::from_oxc(import.id.span))
                }
                _ => None,
            })
            .expect("import binding span");
        let imported = if type_only {
            ImportedSymbol::placeholder_type(import_name.to_string(), import_span)
        } else {
            ImportedSymbol::placeholder_value_and_type(import_name.to_string(), import_span)
        };
        let imports = [imported];
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let unit = CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
        let (module, _) = builder.add_module(&parsed.program, &imports, unit);
        builder.finish(module)
    }

    for (source, name) in [
            (
                "import type { Remote as Named } from './dep'; namespace Named { export interface X {} }",
                "Named",
            ),
            (
                "namespace NamedReverse { export interface X {} } import type { Remote as NamedReverse } from './dep';",
                "NamedReverse",
            ),
            (
                "import type Defaulted from './dep'; namespace Defaulted { export interface X {} }",
                "Defaulted",
            ),
            (
                "namespace DefaultedReverse { export interface X {} } import type DefaultedReverse from './dep';",
                "DefaultedReverse",
            ),
        ] {
            let binder = bind_project_source(source, name, true);
            assert_eq!(
                merge(&binder, name).classification.disposition,
                MergeDisposition::Admitted,
                "{name} composition must be admitted"
            );
            let resolution = binder.resolve_qualified_type_path(binder.module, &[name, "X"]);
            assert!(
                matches!(resolution, QualifiedTypePathResolution::TypeGroup(_)),
                "{name}: {resolution:?}"
            );
        }

    for (source, name) in [
            (
                "import { Remote as NamedValue } from './dep'; namespace NamedValue { export interface X {} }",
                "NamedValue",
            ),
            (
                "namespace NamedValueReverse { export interface X {} } import { Remote as NamedValueReverse } from './dep';",
                "NamedValueReverse",
            ),
            (
                "import DefaultValue from './dep'; namespace DefaultValue { export interface X {} }",
                "DefaultValue",
            ),
            (
                "namespace DefaultValueReverse { export interface X {} } import DefaultValueReverse from './dep';",
                "DefaultValueReverse",
            ),
        ] {
            let binder = bind_project_source(source, name, false);
            assert_eq!(
                merge(&binder, name).classification.disposition,
                MergeDisposition::DeferredBacklog15,
                "{name} keeps its import-endpoint classification"
            );
            assert!(matches!(
                binder.resolve_qualified_type_path(binder.module, &[name, "X"]),
                QualifiedTypePathResolution::TypeGroup(_)
            ));
        }

    for (source, name, type_only) in [
        (
            "import type { Remote as PureTypeImport } from './dep';",
            "PureTypeImport",
            true,
        ),
        (
            "import { Remote as PureNamedImport } from './dep';",
            "PureNamedImport",
            false,
        ),
        (
            "import PureDefaultImport from './dep';",
            "PureDefaultImport",
            false,
        ),
    ] {
        let binder = bind_project_source(source, name, type_only);
        assert_eq!(
            binder.resolve_qualified_type_path(binder.module, &[name, "X"]),
            QualifiedTypePathResolution::Deferred {
                segment: 0,
                reason: QualifiedTypePathDeferredReason::Import,
            }
        );
    }

    for (source, name) in [
            (
                "import type * as NamespaceImport from './dep'; namespace NamespaceImport { export interface X {} }",
                "NamespaceImport",
            ),
            (
                "import EqualsImport = require('./dep'); namespace EqualsImport { export interface X {} }",
                "EqualsImport",
            ),
        ] {
            let binder = bind(source, false);
            assert_eq!(
                merge(&binder, name).classification.disposition,
                MergeDisposition::RejectedFutureTk2440
            );
            assert_eq!(
                binder.resolve_qualified_type_path(binder.module, &[name, "X"]),
                QualifiedTypePathResolution::Deferred {
                    segment: 0,
                    reason: QualifiedTypePathDeferredReason::Import,
                }
            );
        }
}

#[test]
fn qualified_type_path_root_lookup_is_slot_aware_and_private_scope_ordered() {
    let source = r#"
namespace SlotRoot { export interface Item { item: true } }
function valueShadow() { const SlotRoot = 1; }
function typeShadow() { interface SlotRoot { local: true } }
namespace NamespaceHost {
  export namespace SlotRoot { export interface ParentItem { parent: true } }
  export namespace Nested {
    namespace SlotRoot { export interface Inner { inner: true } }
  }
}
namespace Visibility {
  export namespace Shared { export interface Leaf { shared: true } }
  namespace FirstPrivate { export interface Leaf { first: true } }
}
namespace Visibility {
  namespace SecondPrivate { export interface Leaf { second: true } }
}
"#;
    let binder = bind(source, false);
    for function_name in ["valueShadow", "typeShadow"] {
        let start = u32::try_from(
            source
                .find(&format!("function {function_name}"))
                .expect("function source start"),
        )
        .expect("source span fits u32");
        let scope = binder
            .fn_scopes
            .get(&(binder.module, start))
            .copied()
            .expect("function scope");
        assert!(matches!(
            binder.resolve_qualified_type_path(scope, &["SlotRoot", "Item"]),
            QualifiedTypePathResolution::TypeGroup(_)
        ));
    }

    let host = binder
        .namespaces
        .namespaces()
        .find(|namespace| namespace.name == "NamespaceHost")
        .expect("NamespaceHost");
    let nested = binder
        .namespaces
        .namespaces()
        .find(|namespace| {
            namespace.name == "Nested"
                && namespace.owner == NamespaceOwner::NamespacePublic(host.id)
        })
        .expect("NamespaceHost.Nested");
    let nested_private = binder
        .namespaces
        .fragment(nested.fragments[0])
        .expect("Nested fragment")
        .private_scope;
    assert!(matches!(
        binder.resolve_qualified_type_path(nested_private, &["SlotRoot", "Inner"]),
        QualifiedTypePathResolution::TypeGroup(_)
    ));
    assert_eq!(
        binder.resolve_qualified_type_path(nested_private, &["SlotRoot", "ParentItem"]),
        QualifiedTypePathResolution::MissingMember { segment: 1 }
    );
    assert!(matches!(
        binder.resolve_qualified_type_path(
            nested_private,
            &["NamespaceHost", "SlotRoot", "ParentItem"]
        ),
        QualifiedTypePathResolution::TypeGroup(_)
    ));

    let visibility = binder
        .namespaces
        .namespaces()
        .find(|namespace| namespace.name == "Visibility")
        .expect("Visibility");
    let second_private = binder
        .namespaces
        .fragment(visibility.fragments[1])
        .expect("second Visibility reopening")
        .private_scope;
    assert!(matches!(
        binder.resolve_qualified_type_path(second_private, &["SecondPrivate", "Leaf"]),
        QualifiedTypePathResolution::TypeGroup(_)
    ));
    assert!(matches!(
        binder.resolve_qualified_type_path(second_private, &["Shared", "Leaf"]),
        QualifiedTypePathResolution::TypeGroup(_)
    ));
    assert_eq!(
        binder.resolve_qualified_type_path(second_private, &["FirstPrivate", "Leaf"]),
        QualifiedTypePathResolution::MissingRoot { segment: 0 }
    );
    assert!(matches!(
        binder.resolve_qualified_type_path(second_private, &["SlotRoot", "Item"]),
        QualifiedTypePathResolution::TypeGroup(_)
    ));
}

#[test]
fn dotted_and_explicit_nested_namespaces_share_typed_owner_identity() {
    let binder = bind(
        "namespace A.B.C {} namespace A { export namespace B { export namespace C {} } }",
        false,
    );
    assert_eq!(binder.namespaces.len(), 3);
    for name in ["A", "B", "C"] {
        let matches: Vec<_> = binder
            .namespaces
            .namespaces()
            .filter(|namespace| namespace.name == name)
            .collect();
        assert_eq!(matches.len(), 1, "one normalized namespace for {name}");
        assert_eq!(matches[0].fragments.len(), 2);
    }
    let dotted = binder
        .namespaces
        .fragments()
        .filter(|fragment| fragment.publication == NamespacePublication::DottedImplicit)
        .count();
    assert_eq!(dotted, 2);
    assert!(binder.namespaces.namespaces().all(|namespace| binder
        .graph
        .get(namespace.public_scope)
        .is_some_and(|scope| scope.parent.is_none())));
    assert!(binder
        .graph
        .get(binder.script_namespace_root)
        .is_some_and(|scope| {
            scope.lookup_local("A").is_some()
                && ["B", "C"]
                    .iter()
                    .all(|name| scope.lookup_local(name).is_none())
        }));
}

#[test]
fn publication_metadata_distinguishes_private_explicit_and_ambient_default() {
    let binder = bind(
            "namespace Plain { const hidden = 1; export const shown = 1; } declare namespace Ambient { const ambientMember: number; }",
            false,
        );
    let publication = |name: &str| {
        binder
            .namespaces
            .members()
            .find(|member| member.name.as_deref() == Some(name))
            .map(|member| member.publication)
            .expect("named namespace member")
    };
    assert_eq!(publication("hidden"), NamespacePublication::Private);
    assert_eq!(publication("shown"), NamespacePublication::Explicit);
    assert_eq!(
        publication("ambientMember"),
        NamespacePublication::AmbientDefault
    );
}

#[test]
fn ambient_export_lists_preserve_forward_alias_names_type_facts_and_anchors() {
    let source = r#"declare namespace N {
            export { Later as Renamed, Later as "wire-name" };
            export type { Later as TypeRenamed };
            const Later: number;
        }"#;
    let binder = bind(source, true);
    let namespace = binder
        .namespaces
        .namespaces()
        .find(|namespace| namespace.name == "N")
        .expect("N namespace");
    let fragment = binder
        .namespaces
        .fragment(namespace.fragments[0])
        .expect("fragment");
    let private = binder
        .graph
        .get(fragment.private_scope)
        .expect("private scope");
    let public = binder
        .graph
        .get(namespace.public_scope)
        .expect("public scope");
    let later = private.lookup_local("Later").expect("forward local anchor");
    assert_eq!(public.lookup_local("Later"), None);
    for name in ["Renamed", "wire-name", "TypeRenamed"] {
        let alias = binder
            .namespaces
            .members()
            .find(|member| member.name.as_deref() == Some(name))
            .expect("alias member");
        assert_eq!(alias.alias_context, Some(AliasContext::ValidAmbient));
        assert_eq!(alias.local_symbol, Some(later));
        assert_eq!(alias.publication, NamespacePublication::Explicit);
        assert!(alias.symbol.is_some());
        assert!(public.lookup_local(name).is_some());
    }
    let string_alias = binder
        .namespaces
        .members()
        .find(|member| member.name.as_deref() == Some("wire-name"))
        .expect("string alias");
    assert_eq!(
        string_alias.exported_name,
        Some(MetadataName::StringLiteral("wire-name".to_string()))
    );
    assert_eq!(
        string_alias.alias_space_intent,
        Some(AliasSpaceIntent::UnresolvedValueOrType)
    );
    assert_eq!(string_alias.spaces, DeclarationSpaces::NONE);
    assert!(string_alias.export_context.is_some());
    let type_alias = binder
        .namespaces
        .members()
        .find(|member| member.name.as_deref() == Some("TypeRenamed"))
        .expect("type alias");
    assert!(type_alias.outer_type_only);
    assert!(type_alias.spaces.r#type);
    assert!(!type_alias.spaces.value);
    assert_eq!(type_alias.alias_space_intent, Some(AliasSpaceIntent::Type));
    assert!(binder
        .namespaces
        .members()
        .find(|member| member.name.as_deref() == Some("Later") && member.declaration.is_some())
        .is_some_and(|member| member.publication == NamespacePublication::Private));
}

#[test]
fn namespace_export_syntax_and_resolution_match_strict_tsc_6_0_3_oracle() {
    struct Case {
        name: &'static str,
        source: &'static str,
        kind: ExportContextKind,
        syntax: ExportSyntaxDisposition,
        resolution: ExportResolutionDisposition,
        has_module_specifier: bool,
        alias: Option<AliasContext>,
    }

    // `tsc 6.0.3 --strict --noEmit`: wrapped declarations are valid; local named
    // lists require ambient namespaces; re-exports/export-all are TS1194; default
    // is TS1319; and export assignment is TS1063.
    let cases = [
        Case {
            name: "nonambient wrapped",
            source: "namespace N { export interface X {} }",
            kind: ExportContextKind::WrappedDeclaration,
            syntax: ExportSyntaxDisposition::Valid,
            resolution: ExportResolutionDisposition::NotRequired,
            has_module_specifier: false,
            alias: None,
        },
        Case {
            name: "ambient wrapped",
            source: "declare namespace N { export interface X {} }",
            kind: ExportContextKind::WrappedDeclaration,
            syntax: ExportSyntaxDisposition::Valid,
            resolution: ExportResolutionDisposition::NotRequired,
            has_module_specifier: false,
            alias: None,
        },
        Case {
            name: "nonambient local named list",
            source: "namespace N { const X = 1; export { X }; }",
            kind: ExportContextKind::NamedList,
            syntax: ExportSyntaxDisposition::FutureTk1194,
            resolution: ExportResolutionDisposition::NotRequired,
            has_module_specifier: false,
            alias: Some(AliasContext::InvalidFutureTk1194),
        },
        Case {
            name: "ambient local named list",
            source: "declare namespace N { const X: number; export { X }; }",
            kind: ExportContextKind::NamedList,
            syntax: ExportSyntaxDisposition::Valid,
            resolution: ExportResolutionDisposition::NotRequired,
            has_module_specifier: false,
            alias: Some(AliasContext::ValidAmbient),
        },
        Case {
            name: "nonambient named re-export",
            source: "namespace N { export { X } from 'pkg'; }",
            kind: ExportContextKind::NamedList,
            syntax: ExportSyntaxDisposition::FutureTk1194,
            resolution: ExportResolutionDisposition::DeferredBacklog15,
            has_module_specifier: true,
            alias: Some(AliasContext::InvalidFutureTk1194),
        },
        Case {
            name: "ambient named re-export",
            source: "declare namespace N { export { X } from 'pkg'; }",
            kind: ExportContextKind::NamedList,
            syntax: ExportSyntaxDisposition::FutureTk1194,
            resolution: ExportResolutionDisposition::DeferredBacklog15,
            has_module_specifier: true,
            alias: Some(AliasContext::InvalidFutureTk1194),
        },
        Case {
            name: "nonambient export all",
            source: "namespace N { export * from 'pkg'; }",
            kind: ExportContextKind::ExportAll,
            syntax: ExportSyntaxDisposition::FutureTk1194,
            resolution: ExportResolutionDisposition::DeferredBacklog15,
            has_module_specifier: true,
            alias: None,
        },
        Case {
            name: "ambient export all",
            source: "declare namespace N { export * from 'pkg'; }",
            kind: ExportContextKind::ExportAll,
            syntax: ExportSyntaxDisposition::FutureTk1194,
            resolution: ExportResolutionDisposition::DeferredBacklog15,
            has_module_specifier: true,
            alias: None,
        },
        Case {
            name: "nonambient default",
            source: "namespace N { export default function f() {} }",
            kind: ExportContextKind::ExportDefault,
            syntax: ExportSyntaxDisposition::FutureTk1319,
            resolution: ExportResolutionDisposition::NotRequired,
            has_module_specifier: false,
            alias: None,
        },
        Case {
            name: "ambient default",
            source: "declare namespace N { export default function f(): void; }",
            kind: ExportContextKind::ExportDefault,
            syntax: ExportSyntaxDisposition::FutureTk1319,
            resolution: ExportResolutionDisposition::NotRequired,
            has_module_specifier: false,
            alias: None,
        },
        Case {
            name: "nonambient export assignment",
            source: "namespace N { export = N; }",
            kind: ExportContextKind::ExportAssignment,
            syntax: ExportSyntaxDisposition::FutureTk1063,
            resolution: ExportResolutionDisposition::NotRequired,
            has_module_specifier: false,
            alias: None,
        },
        Case {
            name: "ambient export assignment",
            source: "declare namespace N { export = N; }",
            kind: ExportContextKind::ExportAssignment,
            syntax: ExportSyntaxDisposition::FutureTk1063,
            resolution: ExportResolutionDisposition::NotRequired,
            has_module_specifier: false,
            alias: None,
        },
    ];

    for case in cases {
        let binder = bind(case.source, false);
        let contexts: Vec<_> = binder.namespaces.export_contexts().collect();
        assert_eq!(contexts.len(), 1, "{}", case.name);
        let context = contexts[0];
        assert_eq!(context.kind, case.kind, "{}", case.name);
        assert_eq!(context.syntax, case.syntax, "{}", case.name);
        assert_eq!(context.resolution, case.resolution, "{}", case.name);
        assert_eq!(
            context.has_module_specifier, case.has_module_specifier,
            "{}",
            case.name
        );
        let aliases: Vec<_> = context
            .members
            .iter()
            .filter_map(|member| binder.namespaces.member(*member))
            .filter(|member| member.alias_context.is_some())
            .collect();
        if let Some(alias_context) = case.alias {
            assert_eq!(aliases.len(), 1, "{}", case.name);
            assert_eq!(
                aliases[0].alias_context,
                Some(alias_context),
                "{}",
                case.name
            );
            assert_eq!(
                aliases[0].alias_resolution,
                Some(case.resolution),
                "{}",
                case.name
            );
            assert_eq!(
                aliases[0].module_specifier.is_some(),
                case.has_module_specifier,
                "{}",
                case.name
            );
        } else {
            assert!(aliases.is_empty(), "{}", case.name);
        }
    }
}

#[test]
fn module_specifier_aliases_are_unreachable_from_qualified_identifier_paths() {
    struct Case {
        name: &'static str,
        source: &'static str,
        declaration_file: bool,
        path: &'static [&'static str],
        expected: QualifiedTypePathResolution,
    }

    let cases = [
        Case {
            name: "invalid identifier-namespace re-export",
            source: "namespace N { export { X } from 'pkg'; }",
            declaration_file: false,
            path: &["N", "X"],
            expected: QualifiedTypePathResolution::MissingMember { segment: 1 },
        },
        Case {
            name: "valid top-level re-export",
            source: "export { X as TopAlias } from 'pkg';",
            declaration_file: false,
            path: &["TopAlias", "Member"],
            expected: QualifiedTypePathResolution::MissingRoot { segment: 0 },
        },
        Case {
            name: "valid string ambient-module re-export",
            source: "declare module 'pkg' { export { X as StringAlias } from 'dep'; }",
            declaration_file: true,
            path: &["StringAlias", "Member"],
            expected: QualifiedTypePathResolution::MissingRoot { segment: 0 },
        },
    ];

    for case in cases {
        let binder = bind(case.source, case.declaration_file);
        assert_eq!(
            binder.resolve_qualified_type_path(binder.module, case.path),
            case.expected,
            "{}",
            case.name
        );
    }
}

#[test]
fn string_module_children_are_opaque_and_cannot_collide_with_root_namespaces() {
    let source = r#"declare module "pkg" {
            export namespace X { const hidden: number; }
            export { X as Alias };
        }
        namespace X {}"#;
    let binder = bind(source, true);
    assert_eq!(binder.namespaces.deferred_modules().count(), 1);
    assert_eq!(
        binder
            .namespaces
            .namespaces()
            .filter(|namespace| namespace.name == "X")
            .count(),
        1
    );
    let child = binder
        .namespaces
        .deferred_children()
        .find(|child| child.name.as_ref().is_some_and(|name| name.text() == "X"))
        .expect("opaque child namespace");
    assert_eq!(child.kind, DeferredChildKind::NamespaceDeclaration);
    let declaration = binder
        .declarations
        .get(child.declaration.expect("source declaration"))
        .expect("opaque declaration");
    assert_eq!(declaration.site.scope, None);
    assert_eq!(declaration.namespace, None);
    assert!(binder
        .namespaces
        .deferred_children()
        .any(|child| child.kind == DeferredChildKind::DeferredExport));
}

#[test]
fn placement_only_tracks_instantiated_nonambient_fragments_at_binding_spans() {
    let source = "namespace Outer {} namespace Empty {} function Empty() {} namespace Types { interface I {} } function Types() {} namespace PrivateValue { const x = 1; } function PrivateValue() {} namespace ExportedValue { export const x = 1; } function ExportedValue() {} namespace NestedValue { export namespace Child { const x = 1; } } function NestedValue() {} namespace PrivateImport { import Alias = Outer; } function PrivateImport() {} namespace ExportedImport { export import Alias = Outer; } function ExportedImport() {} namespace RegularEnum { enum E {} } function RegularEnum() {} namespace ConstEnum { const enum E {} } function ConstEnum() {} declare function Over(): void; namespace Over { export const x = 1; } function Over(): void {} function After(): void {} namespace After { const x = 1; }";
    let binder = bind(source, false);
    for name in ["Empty", "Types", "PrivateImport", "ConstEnum", "After"] {
        assert!(merge(&binder, name).placement_issues.is_empty(), "{name}");
    }
    for name in [
        "PrivateValue",
        "ExportedValue",
        "NestedValue",
        "ExportedImport",
        "RegularEnum",
        "Over",
    ] {
        let issues = &merge(&binder, name).placement_issues;
        assert_eq!(issues.len(), 1, "{name}");
        assert_eq!(&source[issues[0].span.range()], name);
    }
    let syntax = |namespace_name: &str, member_name: &str| {
        let namespace = binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == namespace_name)
            .expect("namespace");
        namespace
            .fragments
            .iter()
            .filter_map(|fragment| binder.namespaces.fragment(*fragment))
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| binder.namespaces.member(*member))
            .find(|member| member.name.as_deref() == Some(member_name))
            .map(|member| member.syntax)
            .expect("namespace member syntax")
    };
    assert!(matches!(
        syntax("PrivateImport", "Alias"),
        DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
            form: ImportBindingForm::ImportEquals,
            exported: false,
            ..
        })
    ));
    assert!(matches!(
        syntax("ExportedImport", "Alias"),
        DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
            form: ImportBindingForm::ImportEquals,
            exported: true,
            ..
        })
    ));
    assert_eq!(
        syntax("RegularEnum", "E"),
        DeclarationSyntaxFacts::Enum { constant: false }
    );
    assert_eq!(
        syntax("ConstEnum", "E"),
        DeclarationSyntaxFacts::Enum { constant: true }
    );
}

#[test]
fn external_indicators_file_kinds_and_context_precedence_match_ts() {
    fn context(source: &str, kind: SourceFileKind) -> ModuleBindingContext {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        ModuleBindingContext::for_program(&parsed.program, kind)
    }
    for source in [
        "import {} from 'x';",
        "export {};",
        "export = value;",
        "import value = require('x');",
        "import.meta;",
    ] {
        assert!(
            context(source, SourceFileKind::ImplementationTs).external_module,
            "{source}"
        );
    }
    for source in [
        "",
        "import value = Internal.value;",
        "export as namespace Umd;",
    ] {
        assert!(
            !context(source, SourceFileKind::ImplementationTs).external_module,
            "{source}"
        );
    }
    assert!(!context("", SourceFileKind::DeclarationTs).external_module);
    assert!(context("", SourceFileKind::ImplementationMts).external_module);
    assert!(context("", SourceFileKind::ImplementationCts).external_module);
    assert!(!context("", SourceFileKind::DeclarationMts).external_module);
    assert!(!context("", SourceFileKind::DeclarationCts).external_module);
    for kind in [
        SourceFileKind::DeclarationTs,
        SourceFileKind::DeclarationMts,
        SourceFileKind::DeclarationCts,
    ] {
        assert!(context("export {};", kind).external_module);
        assert!(context("import {} from 'x';", kind).external_module);
    }

    let missing_declare = bind("export {}; global { interface X {} }", false);
    let global = missing_declare.namespaces.globals().next().expect("global");
    assert_eq!(global.placement, GlobalPlacement::DirectExternalModule);
    assert_eq!(global.issues, vec![GlobalIssue::FutureTk2670]);
    let nested_umd = bind(
        "export {}; declare namespace N { export as namespace Nested; }",
        true,
    );
    assert_eq!(
        nested_umd
            .namespaces
            .umd_exports()
            .next()
            .expect("UMD record")
            .context,
        UmdContext::FutureTk1316Nested
    );
}

#[test]
fn rich_namespace_and_global_bodies_inventory_exact_declarations_and_binding_storage() {
    let source = "interface Existing {} namespace Existing { export const { a, nested: { b } } = value; export function f(param: number): void {} export class C {} export type T = number; export interface I {} export enum E {} export namespace Child {} export { a as alias }; } declare global { const [g]: number[]; function gf(arg: number): void; class GC {} type GT = number; interface GI {} enum GE {} namespace GN {} }";
    let binder = bind(source, false);
    assert_eq!(binder.decl_count, 7);
    assert_eq!(binder.type_groups.len(), 5);

    let existing_type_symbol = binder
        .graph
        .get(binder.module)
        .and_then(|scope| scope.lookup_local("Existing"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("same-file interface symbol");
    assert!(existing_type_symbol.ty.is_some());
    assert!(existing_type_symbol.ns.is_none());
    assert!(existing_type_symbol.value.is_none());
    assert_eq!(existing_type_symbol.declarations.len(), 1);

    let existing_namespace_symbol = binder
        .graph
        .get(binder.script_namespace_root)
        .and_then(|scope| scope.lookup_local("Existing"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .expect("script-root namespace symbol");
    assert!(existing_namespace_symbol.ns.is_some());
    assert_eq!(
        existing_namespace_symbol.value,
        existing_namespace_symbol
            .ns
            .and_then(|namespace| binder.namespaces.standalone_value_storage(namespace))
    );
    assert_eq!(existing_namespace_symbol.declarations.len(), 1);

    for declaration in binder.declarations.iter().filter(|declaration| {
        declaration.kind != DeclarationKind::Interface
            || &source[declaration.site.binding_span.range()] != "Existing"
    }) {
        let name = &source[declaration.site.binding_span.range()];
        if matches!(name, "a" | "b" | "f" | "C" | "E" | "param") {
            assert!(declaration.value_storage.is_some(), "{name}");
        } else {
            assert_eq!(declaration.value_storage, None, "{name}");
        }
        if matches!(name, "C" | "T" | "I" | "E") {
            assert!(declaration.type_group.is_some());
        } else {
            assert_eq!(declaration.type_group, None);
        }
        if name == "arg" {
            assert_eq!(declaration.site.scope, None);
        } else {
            assert!(declaration.site.scope.is_some());
        }
    }
    let namespace = existing_namespace_symbol
        .ns
        .and_then(|namespace| binder.namespaces.get(namespace))
        .expect("Existing namespace");
    let private_scope = namespace
        .fragments
        .first()
        .and_then(|fragment| binder.namespaces.fragment(*fragment))
        .map(|fragment| fragment.private_scope)
        .expect("Existing fragment");
    let mut binding_leaf_storages = Vec::new();
    for name in ["a", "b"] {
        let declaration = binder
            .declarations
            .iter()
            .find(|declaration| &source[declaration.site.binding_span.range()] == name)
            .expect("object binding leaf declaration");
        assert_eq!(declaration.kind, DeclarationKind::Variable);
        assert_eq!(declaration.site.scope, Some(private_scope));
        let storage = declaration
            .value_storage
            .expect("object binding leaf storage");
        let local_symbol_id = binder
            .graph
            .get(private_scope)
            .and_then(|scope| scope.lookup_local(name))
            .expect("private object binding symbol");
        let local_symbol = binder
            .symbols
            .get(local_symbol_id)
            .expect("private object binding symbol row");
        assert_eq!(local_symbol.value, Some(storage));
        assert_eq!(local_symbol.declarations, vec![declaration.id]);

        let member = binder
            .namespaces
            .members()
            .find(|member| member.declaration == Some(declaration.id))
            .expect("exported object binding member");
        assert_eq!(member.local_symbol, Some(local_symbol_id));
        let public_symbol_id = member.symbol.expect("public object binding symbol id");
        assert_ne!(public_symbol_id, local_symbol_id);
        assert_eq!(
            binder
                .graph
                .get(namespace.public_scope)
                .and_then(|scope| scope.lookup_local(name)),
            Some(public_symbol_id)
        );
        let public_symbol = binder
            .symbols
            .get(public_symbol_id)
            .expect("public object binding symbol");
        assert_eq!(public_symbol.value, Some(storage));
        assert_eq!(public_symbol.declarations, vec![declaration.id]);
        binding_leaf_storages.push(storage);
    }
    assert_ne!(binding_leaf_storages[0], binding_leaf_storages[1]);
    for name in ["a", "b", "f", "C", "T", "I", "E", "Child"] {
        let declaration = binder
            .declarations
            .iter()
            .find(|declaration| &source[declaration.site.binding_span.range()] == name)
            .expect("namespace body declaration");
        assert_eq!(declaration.site.scope, Some(private_scope));
    }
    for name in ["param", "arg"] {
        let declaration = binder
            .declarations
            .iter()
            .find(|declaration| &source[declaration.site.binding_span.range()] == name)
            .expect("descendant declaration");
        if name == "param" {
            assert!(declaration.site.scope.is_some());
        } else {
            assert_eq!(declaration.site.scope, None);
        }
    }
    for name in ["g", "gf", "GC", "GT", "GI", "GE", "GN"] {
        let declaration = binder
            .declarations
            .iter()
            .find(|declaration| &source[declaration.site.binding_span.range()] == name)
            .expect("global body declaration");
        let overlay = binder
            .namespaces
            .globals()
            .next()
            .expect("global augmentation")
            .overlay_scope;
        assert_eq!(declaration.site.scope, Some(overlay));
    }
    let alias = binder
        .namespaces
        .members()
        .find(|member| member.name.as_deref() == Some("alias"))
        .expect("export specifier metadata");
    assert_eq!(alias.declaration, None);
    assert_eq!(
        &source[alias.declaration_span.range()],
        "export { a as alias };"
    );
    assert_eq!(
        &source[alias.specifier_span.expect("specifier span").range()],
        "a as alias"
    );
    assert_eq!(&source[alias.binding_span.range()], "alias");
    assert_eq!(alias.alias_context, Some(AliasContext::InvalidFutureTk1194));
}

#[test]
fn merge_classifier_exposes_slots_compositions_and_overall_disposition() {
    let binder = bind(
            "interface IV {} declare var IV: { new(): IV }; type AliasFunction = {}; function AliasFunction() {} type AliasVariable = {}; var AliasVariable: number; type AliasClass = {}; class AliasClass {} function FIN(): void {} interface FIN {} namespace FIN { export const x = 1; } namespace Chimera {} function Chimera(): void {} enum Chimera {} enum E {} namespace E {} var RV: number; var RV: number; let RL: number; let RL: number; var VN: number; namespace VN {} var VBad: number; namespace VBad { export const x = 1; } import { value as NamedImport } from 'pkg'; namespace NamedImport {} namespace NamedImportRev {} import { value as NamedImportRev } from 'pkg'; import type { Type as NamedTypeImport } from 'pkg'; namespace NamedTypeImport {} namespace NamedTypeImportRev {} import type { Type as NamedTypeImportRev } from 'pkg'; import DefaultImport from 'pkg'; namespace DefaultImport {} namespace DefaultImportRev {} import DefaultImportRev from 'pkg'; import * as NamespaceImport from 'pkg'; namespace NamespaceImport {} namespace NamespaceImportRev {} import * as NamespaceImportRev from 'pkg'; import EqualsImport = require('pkg'); namespace EqualsImport {} namespace EqualsImportRev {} import EqualsImportRev = require('pkg');",
            false,
        );
    let disposition = |name| merge(&binder, name).classification.disposition;
    for name in [
        "IV",
        "AliasFunction",
        "AliasVariable",
        "FIN",
        "VN",
        "NamedTypeImport",
        "NamedTypeImportRev",
    ] {
        assert_eq!(disposition(name), MergeDisposition::Admitted, "{name}");
    }
    assert_eq!(
        disposition("AliasClass"),
        MergeDisposition::RejectedRedeclaration
    );
    assert_eq!(disposition("RV"), MergeDisposition::DeferredBacklog15);
    assert_eq!(disposition("RL"), MergeDisposition::RejectedRedeclaration);
    assert_eq!(
        disposition("VBad"),
        MergeDisposition::RejectedRuntimeNamespace
    );
    assert_eq!(disposition("E"), MergeDisposition::Admitted);
    assert_eq!(
        disposition("Chimera"),
        MergeDisposition::RejectedFutureTk2567
    );
    for name in [
        "NamedImport",
        "NamedImportRev",
        "DefaultImport",
        "DefaultImportRev",
    ] {
        assert_eq!(
            disposition(name),
            MergeDisposition::DeferredBacklog15,
            "{name}"
        );
    }
    for name in [
        "NamespaceImport",
        "NamespaceImportRev",
        "EqualsImport",
        "EqualsImportRev",
    ] {
        assert_eq!(
            disposition(name),
            MergeDisposition::RejectedFutureTk2440,
            "{name}"
        );
    }
    let fin = &merge(&binder, "FIN").classification;
    assert_eq!(fin.slots.value.declarations, 1);
    assert_eq!(fin.slots.r#type.declarations, 1);
    assert_eq!(fin.slots.namespace.declarations, 1);
    for kind in [
        MergeCompositionKind::FunctionNamespace,
        MergeCompositionKind::InterfaceNamespace,
    ] {
        assert!(fin.compositions.iter().any(|item| item.kind == kind));
    }
    assert!(merge(&binder, "AliasClass")
        .classification
        .compositions
        .iter()
        .any(|item| item.kind == MergeCompositionKind::ConflictingTypeDeclarations));
}

#[test]
fn globals_string_modules_and_umd_exports_record_additive_context() {
    let script = bind(
        "global { interface ScriptGlobal {} namespace ScriptSpace { interface Nested {} } }",
        false,
    );
    let script_global = script.namespaces.globals().next().expect("script global");
    assert_eq!(script_global.placement, GlobalPlacement::DirectScript);
    assert_eq!(
        script_global.issues,
        vec![GlobalIssue::FutureTk2669, GlobalIssue::FutureTk2670]
    );
    assert!(script
        .graph
        .get(script.compilation_global)
        .is_some_and(|scope| scope.lookup_local("ScriptGlobal").is_none()));
    assert!(script
        .graph
        .get(script_global.overlay_scope)
        .is_some_and(|scope| {
            scope.lookup_local("ScriptGlobal").is_none()
                && scope.lookup_local("ScriptSpace").is_some()
        }));
    assert!(script
        .graph
        .get(script.compilation_global)
        .is_some_and(|scope| scope.lookup_local("ScriptSpace").is_none()));
    let declared_script = bind("declare global { interface X {} }", false);
    assert_eq!(
        declared_script
            .namespaces
            .globals()
            .next()
            .expect("declared script global")
            .issues,
        vec![GlobalIssue::FutureTk2669]
    );
    let nested_ambient = bind(
        "declare namespace N { global { interface NestedAmbient {} } }",
        false,
    );
    assert_eq!(
        nested_ambient
            .namespaces
            .globals()
            .next()
            .expect("nested ambient global")
            .issues,
        vec![GlobalIssue::FutureTk2669]
    );

    let external = bind(
        "export {}; global { interface ExternalGlobal {} } export as namespace ImplUmd;",
        false,
    );
    let external_global = external
        .namespaces
        .globals()
        .next()
        .expect("external global");
    assert_eq!(
        external_global.placement,
        GlobalPlacement::DirectExternalModule
    );
    assert_eq!(external_global.issues, vec![GlobalIssue::FutureTk2670]);
    let declared_external = bind(
        "export {}; declare global { interface DeclaredExternal {} }",
        false,
    );
    assert!(declared_external
        .namespaces
        .globals()
        .next()
        .expect("declared external global")
        .issues
        .is_empty());
    assert_eq!(
        external
            .namespaces
            .umd_exports()
            .next()
            .expect("implementation UMD")
            .context,
        UmdContext::FutureTk1315Implementation
    );

    let declaration = bind(
        "export as namespace DeclUmd; export = DeclUmd; declare function DeclUmd(): void;",
        true,
    );
    assert_eq!(
        declaration
            .namespaces
            .umd_exports()
            .next()
            .expect("declaration UMD")
            .context,
        UmdContext::DeferredValidBacklog15
    );
    let ambient_module = bind(
        "declare module 'pkg' { global { interface AmbientGlobal {} } }",
        true,
    );
    assert_eq!(ambient_module.namespaces.deferred_modules().count(), 1);
    let module = ambient_module
        .namespaces
        .deferred_modules()
        .next()
        .expect("ambient external module");
    assert_eq!(module.kind, DeferredModuleKind::AmbientExternalModule);
    let ambient_global = ambient_module
        .namespaces
        .globals()
        .next()
        .expect("ambient module global");
    assert_eq!(
        ambient_global.placement,
        GlobalPlacement::DeferredAmbientModule
    );
    assert!(ambient_global.issues.is_empty());
    assert_eq!(
        ambient_module
            .declarations
            .get(ambient_global.declaration)
            .expect("global header")
            .site
            .scope,
        None
    );
    assert!(ambient_module
        .namespaces
        .namespaces()
        .all(|namespace| namespace.name != "pkg"));
    let string_module_declaration = ambient_module
        .namespaces
        .deferred_modules()
        .next()
        .expect("string module")
        .declaration;
    assert_eq!(
        ambient_module
            .declarations
            .get(string_module_declaration)
            .and_then(|declaration| declaration.namespace),
        None
    );

    // `tsc 6.0.3 --strict --noEmit`: a global block nested in a module
    // augmentation reports TS2669, unlike the ambient external-module case.
    let augmentation = bind(
        "export {}; declare module 'pkg' { global { interface AugmentedGlobal {} } }",
        false,
    );
    assert_eq!(
        augmentation
            .namespaces
            .deferred_modules()
            .next()
            .expect("module augmentation")
            .kind,
        DeferredModuleKind::ModuleAugmentation
    );
    assert_eq!(
        augmentation
            .namespaces
            .globals()
            .next()
            .expect("augmentation global")
            .issues,
        vec![GlobalIssue::FutureTk2669]
    );

    let deferred_umd_binder = bind(
        "declare module 'pkg' { export as namespace NestedUmd; }",
        true,
    );
    let deferred_umd = deferred_umd_binder
        .namespaces
        .umd_exports()
        .next()
        .expect("deferred UMD header");
    assert!(matches!(
        deferred_umd.owner,
        DeclarationOwner::DeferredAmbientModule(_)
    ));
    assert_eq!(deferred_umd.context, UmdContext::FutureTk1316Nested);
    assert_eq!(
        deferred_umd_binder
            .declarations
            .get(deferred_umd.declaration)
            .map(|declaration| declaration.site.scope),
        Some(None)
    );

    let non_external = bind("export as namespace ScriptUmd;", false);
    assert_eq!(
        non_external
            .namespaces
            .umd_exports()
            .next()
            .expect("script UMD")
            .context,
        UmdContext::FutureTk1314NonExternal
    );
}

#[test]
fn legal_global_overlays_publish_every_admitted_name_before_module_link() {
    let source = r#"
export {};
interface Captured { local: boolean }
namespace DeferredRoot { export interface DeferredLeaf { moduleOnly: number } }
declare global {
    const GlobalConst: { value: number };
    function GlobalFunction(value: number): string;
    class GlobalClass { value: number }
    interface Captured { global: number }
    interface UsesCaptured { value: Captured }
    type GlobalAlias = UsesCaptured;
    namespace TypeSpace { interface Item { value: string } }
    interface DeferredPair { typeOnly: number }
    class DeferredPair { value: number }
    namespace ValueSpace { interface Item { value: number } const value: number; }
    namespace DeferredRoot { export class DeferredLeaf { globalOnly: string } }
    interface UsesDeferredRoot { value: DeferredRoot.DeferredLeaf }
}
"#;
    let binder = bind(source, false);
    let global = binder.namespaces.globals().next().expect("legal global");
    assert!(global.issues.is_empty());

    let module = binder.graph.get(binder.module).expect("module scope");
    assert_eq!(module.parent, Some(binder.script_namespace_root));
    let script_root = binder
        .graph
        .get(binder.script_namespace_root)
        .expect("script namespace root");
    assert_eq!(script_root.kind, ScopeKind::ScriptNamespaceRoot);
    assert_eq!(script_root.parent, Some(binder.compilation_global));
    let compilation_global = binder
        .graph
        .get(binder.compilation_global)
        .expect("compilation global");
    assert_eq!(compilation_global.parent, Some(binder.prelude_module));
    let overlay = binder
        .graph
        .get(global.overlay_scope)
        .expect("global overlay");
    assert_eq!(overlay.kind, ScopeKind::GlobalOverlay);
    assert_eq!(overlay.parent, Some(binder.module));

    for name in [
        "Captured",
        "GlobalClass",
        "GlobalConst",
        "GlobalFunction",
        "UsesCaptured",
        "GlobalAlias",
        "TypeSpace",
        "DeferredPair",
        "ValueSpace",
        "DeferredRoot",
        "UsesDeferredRoot",
    ] {
        assert_eq!(script_root.lookup_local(name), None, "{name}");
        let canonical = compilation_global
            .lookup_local(name)
            .unwrap_or_else(|| panic!("published global {name}"));
        assert_eq!(overlay.lookup_local(name), Some(canonical));
    }
    for name in [
        "GlobalClass",
        "GlobalConst",
        "GlobalFunction",
        "DeferredPair",
    ] {
        let symbol = compilation_global
            .lookup_local(name)
            .and_then(|symbol| binder.symbols.get(symbol))
            .unwrap_or_else(|| panic!("published global value {name}"));
        assert!(!symbol.blocks_type_lookup, "{name}");
        assert!(!symbol.blocks_value_lookup, "{name}");
        assert!(!symbol.blocks_namespace_lookup, "{name}");
        assert!(symbol.value.is_some(), "{name}: {symbol:?}");
        if matches!(name, "GlobalClass" | "DeferredPair") {
            assert!(symbol.ty.is_some(), "{name}: {symbol:?}");
        }
    }
    for name in ["TypeSpace", "ValueSpace", "DeferredRoot"] {
        assert!(compilation_global
            .lookup_local(name)
            .and_then(|symbol| binder.symbols.get(symbol))
            .is_some_and(|symbol| {
                !symbol.blocks_type_lookup
                    && symbol.blocks_value_lookup
                    && !symbol.blocks_namespace_lookup
                    && symbol.value.is_none()
                    && symbol.ns.is_some()
            }));
    }

    let module_captured = module.lookup_local("Captured").expect("module local");
    let global_captured = compilation_global
        .lookup_local("Captured")
        .expect("global captured");
    assert_ne!(module_captured, global_captured);
    assert_eq!(
        binder.resolve_type(global.overlay_scope, "Captured"),
        Some(global_captured)
    );
    assert_eq!(
        binder.resolve_type(binder.module, "Captured"),
        Some(module_captured)
    );
    assert!(matches!(
        binder.resolve_qualified_type_path(global.overlay_scope, &["DeferredRoot", "DeferredLeaf"]),
        QualifiedTypePathResolution::TypeGroup(_)
    ));

    let uses_group = compilation_global
        .lookup_local("UsesCaptured")
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .and_then(|group| binder.type_groups.get(group))
        .expect("global interface group");
    assert!(uses_group
        .fragments
        .iter()
        .all(|fragment| fragment.scope == global.overlay_scope));
}

#[test]
fn augmentation_exports_have_context_records_without_emission() {
    let global_source =
        "export {}; declare global { export {}; export { X as Alias }; export interface X {} }";
    let global = bind(global_source, false);
    let contexts: Vec<_> = global.namespaces.export_contexts().collect();
    assert_eq!(contexts.len(), 3);
    assert!(contexts.iter().all(|context| {
        context.syntax == ExportSyntaxDisposition::FutureTk2666
            && context.resolution == ExportResolutionDisposition::NotRequired
            && matches!(context.owner, ExportContextOwner::GlobalAugmentation(_))
    }));
    assert!(contexts.iter().any(|context| {
        context.kind == ExportContextKind::NamedList && context.members.is_empty()
    }));
    assert!(contexts
        .iter()
        .any(|context| context.kind == ExportContextKind::WrappedDeclaration));
    let alias = global
        .namespaces
        .members()
        .find(|member| member.name.as_deref() == Some("Alias"))
        .expect("global alias record");
    assert_eq!(
        alias.alias_context,
        Some(AliasContext::InvalidAugmentationFutureTk2666)
    );
    assert!(alias.export_context.is_some());
    assert_eq!(alias.symbol, None);

    let module_augmentation = bind(
        "export {}; declare module 'pkg' { export {}; export interface X {} }",
        false,
    );
    assert_eq!(module_augmentation.namespaces.export_contexts().count(), 2);
    assert!(module_augmentation
        .namespaces
        .export_contexts()
        .all(|context| {
            context.syntax == ExportSyntaxDisposition::FutureTk2666
                && context.resolution == ExportResolutionDisposition::DeferredBacklog15
                && matches!(context.owner, ExportContextOwner::DeferredAmbientModule(_))
        }));

    let ambient_module = bind("declare module 'pkg' { export {}; }", true);
    assert_eq!(
        ambient_module
            .namespaces
            .export_contexts()
            .next()
            .expect("ambient module export")
            .syntax,
        ExportSyntaxDisposition::Valid
    );
    assert_eq!(
        ambient_module
            .namespaces
            .export_contexts()
            .next()
            .expect("ambient module export")
            .resolution,
        ExportResolutionDisposition::DeferredBacklog15
    );

    let assignment = bind("namespace N { export = N; }", false);
    let assignment = assignment
        .namespaces
        .export_contexts()
        .next()
        .expect("namespace export assignment");
    assert_eq!(assignment.kind, ExportContextKind::ExportAssignment);
    assert_eq!(assignment.syntax, ExportSyntaxDisposition::FutureTk1063);
}

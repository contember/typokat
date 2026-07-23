//! Statement-level checker.
//! This module wires parsing/binding output into the checker pass, flow pre-pass,
//! and relation phase. Keep durable design notes in `docs/reference/architecture.md`
//! (§5.2) and soundness rules in `docs/reference/invariants.md`.

use crate::binder::bind::{ImportPlaceholder, ImportedSymbol, ProjectBinderBuilder};
use crate::binder::bind_module_with_prelude;
#[cfg(test)]
use crate::binder::declaration::source_global_binding_census;
use crate::binder::declaration::{DeclarationKind, TypeGroupId, ValueStorageId};
use crate::binder::namespace::SourceUnitKey;
use crate::binder::namespace::{
    CompilationUnit, GlobalIssue, LocalAmbientExportAliasFailureKind, PlacementIssueKind,
    UmdContext,
};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::check::query::SemanticQueryCoordinator;
use crate::check::query::SemanticQueryState;
use crate::class_semantics::{DemandOutcome, Exhaustion};
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic, IncompleteSurface};
use crate::relate::RelationOutcome;
use crate::source::{
    CompilationOrigin, ModuleOrdinal, OriginalModuleOrdinal, SourceOrdinal, SourceUnit, UnitSlot,
};
use crate::span::Span;
use crate::types::layered::{LayeredMap, LayeredSet};
use crate::types::repr::ClassId;
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Declaration, ExportSpecifier, ImportOrExportKind, ModuleExportName, Program, Statement, TSType,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};

mod annotations;
mod assignment;
mod calls;
mod classes;
mod context;
#[cfg(test)]
mod declaration_surface_lazy_spec;
#[cfg(test)]
mod declaration_surface_measure;
mod decls;
pub(in crate::check) mod eval;
pub(crate) mod events;
pub(crate) mod events_library;
mod expr;
mod flowgraph;
mod function_groups;
mod indexed_access;
pub(crate) mod lexical_events;
pub(crate) mod lexical_events_library;
mod lexical_events_user;
pub(crate) mod library_compiler;
mod library_identities;
pub(crate) mod library_reporting;
pub(crate) mod library_snapshot_codec;
mod namespace_values;
mod narrowing;
pub(crate) mod replay_index;
pub(crate) mod reporting_record;
mod statements;
mod type_groups;

#[cfg(test)]
pub(crate) fn generate_library_snapshot_archive(
    product: &library_compiler::CompiledLibraryRuntimeProduct,
) -> Result<Vec<u8>, String> {
    library_snapshot_codec::encode_library_runtime_product(product)
        .map(|compiled| compiled.archive().as_bytes().to_vec())
        .map_err(|error| error.to_string())
}

use context::{
    AssertionCompatibilityObligation, AssignObligation, CheckerEffects, CheckerRecordBatch,
    ClassFillState, ConstructionDrafts, DeclTypes, DeferredRelationObligation,
    InterfaceRelationKind, InterfaceRelationObligation, InterfaceRelationReport, OverrideCheck,
    Pass, TemplateFillTable, TypeDecl, TypeDeclTable, TypeResolvedTable,
};
use decls::{reserve_type_decls, type_decl_id, walk_type_decls, TopTypeDecl};
use events::{user_record_ticket_key, CandidateEffects, EventStore, UserRecordTicket};
use lexical_events::{ClassBinding, LexicalOwnerPhase, LexicalReservations};
use replay_index::ReplayOwner;
use reporting_record::CheckerRecord;
use statements::{emit_exhausted_obligation, emit_obligation_failure};

struct PassReporting<Ticket: Copy> {
    source: SourceUnit,
    lexical_events: LexicalReservations<Ticket>,
    suppress_effects: bool,
}

struct PassReportingPlan<Ticket: Copy> {
    reporting: PassReporting<Ticket>,
    pending_tickets: Vec<Ticket>,
    ticket_key: fn(Ticket) -> (usize, usize),
}

struct UserReportingAdapter {
    event_store: EventStore,
}

fn user_original_module(origin: CompilationOrigin) -> Option<OriginalModuleOrdinal> {
    match origin {
        CompilationOrigin::User(original_module) => Some(original_module),
        CompilationOrigin::Library(_) => None,
    }
}

fn source_ordinal_from_origin(origin: CompilationOrigin) -> SourceOrdinal {
    match origin {
        CompilationOrigin::User(original_module) => {
            SourceOrdinal::User(ModuleOrdinal::new(original_module.index()))
        }
        CompilationOrigin::Library(file_ordinal) => SourceOrdinal::Library(file_ordinal),
    }
}

fn reserve_internal_reporting(
    program: &Program<'_>,
    module_ordinal: ModuleOrdinal,
    unit_slot: UnitSlot,
) -> (PassReporting<UserRecordTicket>, UserReportingAdapter) {
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    lexical_events
        .reserve_program(module_ordinal, unit_slot, program, &mut event_store)
        .expect("lexical event reservation must reference valid events");
    (
        PassReporting {
            source: SourceUnit::User {
                module_ordinal,
                unit_slot,
            },
            lexical_events,
            suppress_effects: false,
        },
        UserReportingAdapter { event_store },
    )
}

/// Trusted utility aliases and bounded ambient values, checked before user code.
pub(crate) const PRELUDE_SOURCE: &str = include_str!("../../prelude.ts");

const TRUSTED_PRELUDE_INTRINSICS: [&str; 6] = [
    "OmitThisParameter",
    "Uppercase",
    "Lowercase",
    "Capitalize",
    "Uncapitalize",
    "ThisType",
];

fn expected_trusted_prelude_incomplete(
    binder: &Binder,
    program: &Program<'_>,
) -> Option<Vec<IncompleteSurface>> {
    let mut valid = true;
    let mut seen = FxHashSet::default();
    let mut expected = Vec::with_capacity(TRUSTED_PRELUDE_INTRINSICS.len());
    walk_type_decls(
        binder,
        binder.prelude_module,
        program,
        &mut |scope, _, declaration| {
            let TopTypeDecl::Alias(alias) = declaration else {
                return;
            };
            let name = alias.id.name.as_str();
            if !TRUSTED_PRELUDE_INTRINSICS.contains(&name) {
                return;
            }
            let TSType::TSIntrinsicKeyword(keyword) = &alias.type_annotation else {
                valid = false;
                return;
            };
            if scope != binder.prelude_module || !seen.insert(name.to_string()) {
                valid = false;
                return;
            }
            expected.push(IncompleteSurface::new(
                "annotation-lower/intrinsic-keyword/self",
                Span::from_oxc(keyword.span),
                "intrinsic keyword type not modeled",
            ));
        },
    );
    if valid
        && TRUSTED_PRELUDE_INTRINSICS
            .iter()
            .all(|name| seen.contains(*name))
    {
        Some(expected)
    } else {
        None
    }
}

fn trusted_prelude_records_are_clean(
    binder: &Binder,
    program: &Program<'_>,
    records: &BTreeMap<ModuleOrdinal, (Vec<Diagnostic>, Vec<IncompleteSurface>)>,
) -> bool {
    let Some(expected_incomplete) = expected_trusted_prelude_incomplete(binder, program) else {
        return false;
    };
    let Some((diagnostics, incomplete)) = records.get(&ModuleOrdinal::new(0)) else {
        return false;
    };
    records.len() == 1 && diagnostics.is_empty() && incomplete == &expected_incomplete
}

/// Lifetime-free state handed from the trusted prelude pass to user checking.
struct TrustedPreludeHandoff {
    published_types: type_groups::PublishedTypeEnvironment,
    library_semantic_identities: Option<library_identities::LibrarySemanticIdentities>,
    lexical_array_alias: Option<TypeGroupId>,
    decl_types: DeclTypes,
    next_type_param: u32,
    next_class_id: u32,
}

#[derive(Default)]
pub(in crate::check::checker) struct FrozenCheckerRuntimeMetadata {
    class_application_parameters:
        LayeredMap<ClassId, Vec<classes::construction::DraftClassTypeParameter<()>>>,
    class_new_metadata: LayeredMap<ClassId, context::PublishedClassNewMetadata>,
    class_parents: LayeredMap<ClassId, ClassId>,
    class_value_aliases: LayeredMap<ValueStorageId, ValueStorageId>,
    class_value_bindings: LayeredMap<ValueStorageId, context::PublishedClassValueBinding>,
    standalone_namespace_value_aliases: LayeredMap<ValueStorageId, ValueStorageId>,
    class_names: LayeredMap<ClassId, String>,
    namespace_terminals: namespace_values::FrozenNamespaceValueTerminals,
    named_function_symbols: LayeredSet<SymbolId>,
}

pub(in crate::check::checker) struct FrozenCheckerRuntimeSnapshotParts {
    pub(in crate::check::checker) class_application_parameters: Vec<(
        ClassId,
        Vec<classes::construction::DraftClassTypeParameterSnapshot>,
    )>,
    pub(in crate::check::checker) class_new_metadata:
        Vec<(ClassId, context::PublishedClassNewMetadata)>,
    pub(in crate::check::checker) class_parents: Vec<(ClassId, ClassId)>,
    pub(in crate::check::checker) class_value_aliases: Vec<(ValueStorageId, ValueStorageId)>,
    pub(in crate::check::checker) class_value_bindings:
        Vec<(ValueStorageId, context::PublishedClassValueBinding)>,
    pub(in crate::check::checker) standalone_namespace_value_aliases:
        Vec<(ValueStorageId, ValueStorageId)>,
    pub(in crate::check::checker) class_names: Vec<(ClassId, String)>,
    pub(in crate::check::checker) namespace_terminals:
        namespace_values::FrozenNamespaceValueTerminalsSnapshotParts,
    pub(in crate::check::checker) named_function_symbols: Vec<SymbolId>,
}

impl FrozenCheckerRuntimeMetadata {
    pub(in crate::check::checker) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.class_application_parameters.freeze_as_base()?;
        self.class_new_metadata.freeze_as_base()?;
        self.class_parents.freeze_as_base()?;
        self.class_value_aliases.freeze_as_base()?;
        self.class_value_bindings.freeze_as_base()?;
        self.standalone_namespace_value_aliases.freeze_as_base()?;
        self.class_names.freeze_as_base()?;
        self.namespace_terminals.freeze_as_base()?;
        self.named_function_symbols.freeze_as_base()
    }

    pub(in crate::check::checker) fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            class_application_parameters: self.class_application_parameters.fork_delta()?,
            class_new_metadata: self.class_new_metadata.fork_delta()?,
            class_parents: self.class_parents.fork_delta()?,
            class_value_aliases: self.class_value_aliases.fork_delta()?,
            class_value_bindings: self.class_value_bindings.fork_delta()?,
            standalone_namespace_value_aliases: self
                .standalone_namespace_value_aliases
                .fork_delta()?,
            class_names: self.class_names.fork_delta()?,
            namespace_terminals: self.namespace_terminals.fork_delta()?,
            named_function_symbols: self.named_function_symbols.fork_delta()?,
        })
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn shares_base_with(&self, other: &Self) -> bool {
        self.class_application_parameters
            .shares_base_with(&other.class_application_parameters)
            && self
                .class_new_metadata
                .shares_base_with(&other.class_new_metadata)
            && self.class_parents.shares_base_with(&other.class_parents)
            && self
                .class_value_aliases
                .shares_base_with(&other.class_value_aliases)
            && self
                .class_value_bindings
                .shares_base_with(&other.class_value_bindings)
            && self
                .standalone_namespace_value_aliases
                .shares_base_with(&other.standalone_namespace_value_aliases)
            && self.class_names.shares_base_with(&other.class_names)
            && self
                .namespace_terminals
                .shares_base_with(&other.namespace_terminals)
            && self
                .named_function_symbols
                .shares_base_with(&other.named_function_symbols)
    }

    pub(in crate::check::checker) fn snapshot_parts(
        &self,
    ) -> Result<FrozenCheckerRuntimeSnapshotParts, &'static str> {
        let mut class_application_parameters = self
            .class_application_parameters
            .iter()
            .map(|(&class, parameters)| {
                (
                    class,
                    parameters
                        .iter()
                        .copied()
                        .map(classes::construction::DraftClassTypeParameter::snapshot_parts)
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        class_application_parameters.sort_by_key(|(class, _)| class.0);
        let mut class_new_metadata = self
            .class_new_metadata
            .iter()
            .map(|(&class, &metadata)| (class, metadata))
            .collect::<Vec<_>>();
        class_new_metadata.sort_by_key(|(class, _)| class.0);
        let mut class_parents = self
            .class_parents
            .iter()
            .map(|(&class, &parent)| (class, parent))
            .collect::<Vec<_>>();
        class_parents.sort_by_key(|(class, _)| class.0);
        let mut class_value_aliases = self
            .class_value_aliases
            .iter()
            .map(|(&alias, &target)| (alias, target))
            .collect::<Vec<_>>();
        class_value_aliases.sort_by_key(|(alias, _)| alias.0);
        let mut class_value_bindings = self
            .class_value_bindings
            .iter()
            .map(|(&storage, &binding)| (storage, binding))
            .collect::<Vec<_>>();
        class_value_bindings.sort_by_key(|(storage, _)| storage.0);
        let mut standalone_namespace_value_aliases = self
            .standalone_namespace_value_aliases
            .iter()
            .map(|(&alias, &target)| (alias, target))
            .collect::<Vec<_>>();
        standalone_namespace_value_aliases.sort_by_key(|(alias, _)| alias.0);
        let mut class_names = self
            .class_names
            .iter()
            .map(|(&class, name)| (class, name.clone()))
            .collect::<Vec<_>>();
        class_names.sort_by_key(|(class, _)| class.0);
        let mut named_function_symbols = self
            .named_function_symbols
            .iter()
            .copied()
            .collect::<Vec<_>>();
        named_function_symbols.sort_by_key(|symbol| symbol.0);
        Ok(FrozenCheckerRuntimeSnapshotParts {
            class_application_parameters,
            class_new_metadata,
            class_parents,
            class_value_aliases,
            class_value_bindings,
            standalone_namespace_value_aliases,
            class_names,
            namespace_terminals: self.namespace_terminals.snapshot_parts()?,
            named_function_symbols,
        })
    }

    pub(in crate::check::checker) fn from_snapshot_parts(
        parts: FrozenCheckerRuntimeSnapshotParts,
    ) -> Result<Self, &'static str> {
        fn strictly_ordered(mut keys: impl Iterator<Item = u32>) -> bool {
            let mut previous = None;
            keys.all(|key| {
                let ordered = previous.is_none_or(|previous| previous < key);
                previous = Some(key);
                ordered
            })
        }

        if !strictly_ordered(
            parts
                .class_application_parameters
                .iter()
                .map(|(class, _)| class.0),
        ) || !strictly_ordered(parts.class_new_metadata.iter().map(|(class, _)| class.0))
            || !strictly_ordered(parts.class_parents.iter().map(|(class, _)| class.0))
            || !strictly_ordered(parts.class_value_aliases.iter().map(|(id, _)| id.0))
            || !strictly_ordered(parts.class_value_bindings.iter().map(|(id, _)| id.0))
            || !strictly_ordered(
                parts
                    .standalone_namespace_value_aliases
                    .iter()
                    .map(|(id, _)| id.0),
            )
            || !strictly_ordered(parts.class_names.iter().map(|(class, _)| class.0))
            || !strictly_ordered(parts.named_function_symbols.iter().map(|symbol| symbol.0))
        {
            return Err("snapshot checker runtime rows are not strictly ordered");
        }
        let mut class_application_parameters = LayeredMap::default();
        for (class, parameters) in parts.class_application_parameters {
            let mut ids = BTreeSet::new();
            if parameters.iter().any(|parameter| !ids.insert(parameter.id)) {
                return Err("snapshot class application repeats a parameter id");
            }
            class_application_parameters.insert_local(
                class,
                parameters
                    .into_iter()
                    .map(classes::construction::DraftClassTypeParameter::from_snapshot_parts)
                    .collect(),
            )?;
        }
        Ok(Self {
            class_application_parameters,
            class_new_metadata: parts
                .class_new_metadata
                .into_iter()
                .collect::<FxHashMap<_, _>>()
                .into(),
            class_parents: parts
                .class_parents
                .into_iter()
                .collect::<FxHashMap<_, _>>()
                .into(),
            class_value_aliases: parts
                .class_value_aliases
                .into_iter()
                .collect::<FxHashMap<_, _>>()
                .into(),
            class_value_bindings: parts
                .class_value_bindings
                .into_iter()
                .collect::<FxHashMap<_, _>>()
                .into(),
            standalone_namespace_value_aliases: parts
                .standalone_namespace_value_aliases
                .into_iter()
                .collect::<FxHashMap<_, _>>()
                .into(),
            class_names: parts
                .class_names
                .into_iter()
                .collect::<FxHashMap<_, _>>()
                .into(),
            namespace_terminals:
                namespace_values::FrozenNamespaceValueTerminals::from_snapshot_parts(
                    parts.namespace_terminals,
                )?,
            named_function_symbols: parts
                .named_function_symbols
                .into_iter()
                .collect::<FxHashSet<_>>()
                .into(),
        })
    }
}

pub(in crate::check::checker) struct BoundUserBase {
    published_types: type_groups::PublishedTypeEnvironment,
    library_semantic_identities: Option<library_identities::LibrarySemanticIdentities>,
    lexical_array_alias: Option<TypeGroupId>,
    decl_types: DeclTypes,
    next_type_param: u32,
    next_class_id: u32,
    runtime: FrozenCheckerRuntimeMetadata,
}

/// Parse, bind, and check the trusted prelude in the caller's run-local type universe.
fn bootstrap_trusted_prelude(
    interner: &mut Interner,
    bind: impl FnOnce(&Program<'_>) -> Binder,
) -> (Binder, TrustedPreludeHandoff) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, PRELUDE_SOURCE, SourceType::ts()).parse();
    debug_assert!(
        !parsed.panicked && parsed.diagnostics.is_empty(),
        "the prelude must parse clean: {:?}",
        parsed.diagnostics
    );

    let binder = bind(&parsed.program);
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_resolved: TypeResolvedTable = vec![None; binder.type_groups.len()].into();
    let mut type_decls: TypeDeclTable<'_> = Vec::new().into();
    reserve_type_decls(
        interner,
        &binder,
        binder.prelude_module,
        &parsed.program,
        &mut next_type_param,
        &mut next_class_id,
        &mut type_decls,
        &mut type_resolved,
    );

    let prelude_ordinal = ModuleOrdinal::new(0);
    let prelude_slot = UnitSlot::new(0);
    // Capture into an isolated store so cleanliness is checked without leaking records.
    let (mut reporting, reporting_adapter) =
        reserve_internal_reporting(&parsed.program, prelude_ordinal, prelude_slot);
    attach_type_decl_owners(
        &mut reporting.lexical_events,
        SourceOrdinal::User(prelude_ordinal),
        &binder,
        binder.prelude_module,
        &parsed.program,
    );
    attach_class_bindings(
        &mut reporting.lexical_events,
        SourceOrdinal::User(prelude_ordinal),
        &binder,
        binder.prelude_module,
        &parsed.program,
        &type_decls,
    );
    let mut pass = build_pass_with_reporting(
        interner,
        &binder,
        type_decls,
        type_resolved,
        DeclTypes::new(binder.decl_count),
        next_type_param,
        reporting,
    );
    pass.current_module = binder.prelude_module;
    let intrinsic_markers = prelude_intrinsic_markers(pass.interner);
    seed_prelude_intrinsics(&binder, &mut pass.type_resolved, intrinsic_markers);
    pass.fill_type_decls(binder.prelude_module);
    pass.publish_class_surfaces();
    pass.fill_pending_interfaces_range(binder.prelude_module, 0, pass.type_decls.len());
    pass.freeze_seeded_type_groups();
    pass.publish_type_groups();
    pass.validate_published_class_surfaces();
    pass.build_flow_graph(binder.prelude_module, &parsed.program.body);
    pass.check_statements(binder.prelude_module, &parsed.program.body);

    let records = finish_event_effects(&mut pass, reporting_adapter);
    debug_assert!(
        trusted_prelude_records_are_clean(&binder, &parsed.program, &records),
        "the prelude must check clean: {records:?}"
    );

    let Pass {
        type_environment,
        decl_types,
        next_type_param,
        ..
    } = pass;
    let type_groups::TypeEnvironmentState::Published(published_types) = type_environment else {
        panic!("trusted prelude must hand off one published environment")
    };
    let selected = library_identities::LibrarySemanticIdentities::select_from_scope(
        &binder,
        binder.prelude_module,
        &published_types,
        interner.store(),
    );
    let lexical_array_alias = selected.array_group();
    let library_semantic_identities = selected.all_ready().then_some(selected);
    (
        binder,
        TrustedPreludeHandoff {
            published_types,
            library_semantic_identities,
            lexical_array_alias,
            decl_types,
            next_type_param,
            next_class_id,
        },
    )
}

/// The structured outcome of checking one module: type diagnostics plus the third
/// incomplete-surface channel (in-scope AST positions the checker skipped). An empty
/// `incomplete` is the normal case today — WU3–5 wire the emissions (sprint 2026-07-10).
pub struct CheckResult {
    pub(crate) module_ordinal: ModuleOrdinal,
    pub(crate) unit_slot: UnitSlot,
    pub diagnostics: Vec<Diagnostic>,
    pub incomplete: Vec<IncompleteSurface>,
}

/// Test-only emission hook (WU2 plumbing): if `TYPOKAT_TEST_EMIT_INCOMPLETE` is set,
/// record one incomplete surface per comma-separated id in its value (at span 0..0),
/// exercising the real `record_incomplete` API end to end. No real checker path emits
/// yet, so with the env var unset every run is unaffected. A blank/`1` value emits one
/// default id.
fn emit_test_incomplete(pass: &mut Pass<'_, '_>) {
    let Some(value) = std::env::var_os("TYPOKAT_TEST_EMIT_INCOMPLETE") else {
        return;
    };
    let value = value.to_string_lossy();
    let ids: Vec<&str> = match value.trim() {
        "" | "1" => vec!["test-only/plumbing/self"],
        other => other
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect(),
    };
    let source_start = pass
        .lexical_events
        .top_level()
        .iter()
        .find(|site| site.source.unit == pass.current_source)
        .map(|site| site.source.source_start);
    if let Some(source_start) = source_start {
        pass.with_lexical_effects(source_start, LexicalOwnerPhase::Incomplete, |pass| {
            for id in ids {
                pass.record_incomplete(
                    id,
                    Span::new(0, 0),
                    "test-only emission hook (WU2 plumbing)",
                );
            }
        });
    }
}

/// Check a parsed program and return the diagnostics plus incomplete surfaces it produces.
pub fn check_program<'ast>(interner: &mut Interner, program: &'ast Program<'ast>) -> CheckResult {
    check_program_inner(interner, program, |_, _, _, _, _| {})
}

#[cfg(test)]
fn check_program_with_publication_inspector<'ast, F>(
    interner: &mut Interner,
    program: &'ast Program<'ast>,
    inspect: F,
) -> CheckResult
where
    F: FnOnce(&Binder, &type_groups::PublishedTypeEnvironment, &Interner),
{
    check_program_inner(interner, program, |binder, published, interner, _, _| {
        inspect(binder, published, interner)
    })
}

#[cfg(test)]
fn check_program_with_namespace_value_inspector<'ast, F>(
    interner: &mut Interner,
    program: &'ast Program<'ast>,
    inspect: F,
) -> CheckResult
where
    F: FnOnce(&Binder, &namespace_values::NamespaceValueRegistry, &DeclTypes, &Interner),
{
    check_program_inner(
        interner,
        program,
        |binder, _, interner, decl_types, namespace_values| {
            inspect(binder, namespace_values, decl_types, interner)
        },
    )
}

fn check_program_inner<'ast, F>(
    interner: &mut Interner,
    program: &'ast Program<'ast>,
    inspect: F,
) -> CheckResult
where
    F: FnOnce(
        &Binder,
        &type_groups::PublishedTypeEnvironment,
        &Interner,
        &DeclTypes,
        &namespace_values::NamespaceValueRegistry,
    ),
{
    let (
        binder,
        TrustedPreludeHandoff {
            published_types,
            library_semantic_identities,
            lexical_array_alias,
            decl_types,
            next_type_param,
            next_class_id,
        },
    ) = bootstrap_trusted_prelude(interner, |prelude| {
        bind_module_with_prelude(prelude, program)
    });

    check_bound_user_program(
        interner,
        binder,
        program,
        BoundUserBase {
            published_types,
            library_semantic_identities,
            lexical_array_alias,
            decl_types,
            next_type_param,
            next_class_id,
            runtime: FrozenCheckerRuntimeMetadata::default(),
        },
        inspect,
    )
}

pub(in crate::check::checker) fn check_bound_user_program<'ast, F>(
    interner: &mut Interner,
    binder: Binder,
    program: &'ast Program<'ast>,
    base: BoundUserBase,
    inspect: F,
) -> CheckResult
where
    F: FnOnce(
        &Binder,
        &type_groups::PublishedTypeEnvironment,
        &Interner,
        &DeclTypes,
        &namespace_values::NamespaceValueRegistry,
    ),
{
    check_bound_user_program_inner(interner, binder, program, base, inspect, |_, _| {})
}

#[cfg(test)]
pub(in crate::check::checker) fn check_bound_user_program_with_final_identity_inspector<
    'ast,
    F,
    G,
>(
    interner: &mut Interner,
    binder: Binder,
    program: &'ast Program<'ast>,
    base: BoundUserBase,
    inspect: F,
    inspect_final: G,
) -> CheckResult
where
    F: FnOnce(
        &Binder,
        &type_groups::PublishedTypeEnvironment,
        &Interner,
        &DeclTypes,
        &namespace_values::NamespaceValueRegistry,
    ),
    G: FnOnce(&Pass<'_, 'ast>, u32),
{
    FINAL_IDENTITY_INSPECTOR_CALLS.with(|calls| calls.set(calls.get() + 1));
    check_bound_user_program_inner(interner, binder, program, base, inspect, inspect_final)
}

fn check_bound_user_program_inner<'ast, F, G>(
    interner: &mut Interner,
    binder: Binder,
    program: &'ast Program<'ast>,
    base: BoundUserBase,
    inspect: F,
    inspect_final: G,
) -> CheckResult
where
    F: FnOnce(
        &Binder,
        &type_groups::PublishedTypeEnvironment,
        &Interner,
        &DeclTypes,
        &namespace_values::NamespaceValueRegistry,
    ),
    G: FnOnce(&Pass<'_, 'ast>, u32),
{
    #[cfg(test)]
    BOUND_USER_CHECK_CALLS.with(|calls| calls.set(calls.get() + 1));
    let module_ordinal = ModuleOrdinal::new(0);
    let unit_slot = UnitSlot::new(0);
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    lexical_events
        .reserve_program(module_ordinal, unit_slot, program, &mut event_store)
        .expect("lexical event reservation must reference valid events");
    let BoundUserBase {
        published_types,
        library_semantic_identities,
        lexical_array_alias,
        mut decl_types,
        mut next_type_param,
        mut next_class_id,
        runtime,
    } = base;
    decl_types.resize(binder.decl_count);

    // User declarations append after prelude placeholders, preserving legacy storage indices.
    let (mut type_decls, mut type_resolved) = published_types.construction_prefix();
    type_resolved.resize(binder.type_groups.len(), None);
    reserve_type_decls(
        interner,
        &binder,
        binder.module,
        program,
        &mut next_type_param,
        &mut next_class_id,
        &mut type_decls,
        &mut type_resolved,
    );
    attach_type_decl_owners(
        &mut lexical_events,
        SourceOrdinal::User(module_ordinal),
        &binder,
        binder.module,
        program,
    );
    attach_class_bindings(
        &mut lexical_events,
        SourceOrdinal::User(module_ordinal),
        &binder,
        binder.module,
        program,
        &type_decls,
    );
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .expect("one callable binder reservation pass");
    let mut external_effects = BTreeMap::new();
    enqueue_local_ambient_export_alias_diagnostics(&binder, &lexical_events, &mut external_effects);
    enqueue_namespace_placement_diagnostics(&binder, &lexical_events, &mut external_effects);
    enqueue_ambient_context_diagnostics(&binder, &lexical_events, &mut external_effects);
    let mut pass = build_pass_with_reporting(
        interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReporting {
            source: SourceUnit::User {
                module_ordinal,
                unit_slot,
            },
            lexical_events,
            suppress_effects: false,
        },
    );
    pass.install_published_type_environment_base(published_types);
    pass.lexical_array_alias = lexical_array_alias;
    if let Some(identities) = library_semantic_identities {
        pass.install_library_semantic_identities(identities);
    }
    pass.class_application_parameters = runtime.class_application_parameters;
    pass.class_new_metadata = runtime.class_new_metadata;
    pass.class_parents = runtime.class_parents;
    pass.class_value_aliases = runtime.class_value_aliases;
    pass.class_value_bindings = runtime.class_value_bindings;
    pass.standalone_namespace_value_aliases = runtime.standalone_namespace_value_aliases;
    pass.class_names = runtime.class_names;
    pass.namespace_values
        .install_frozen_terminals(runtime.namespace_terminals);
    pass.named_function_symbols = runtime.named_function_symbols;
    for effects in external_effects.into_values() {
        pass.enqueue_effects(CheckerEffects::from_records(effects));
    }

    // Phase 0: fill named type declarations before walking values.
    pass.fill_type_decls(binder.module);
    pass.prepare_attached_namespace_values(binder.module, &program.body);
    pass.prepare_standalone_namespace_values(binder.module, &program.body);
    pass.publish_class_surfaces();
    pass.finalize_standalone_namespace_values();
    pass.precompute_standalone_namespace_value_aliases(&[(binder.module, program.body.as_slice())]);
    pass.fill_pending_interfaces_range(
        binder.module,
        pass.type_decls.published_len(),
        pass.type_decls.len(),
    );
    pass.publish_type_groups();
    pass.validate_published_class_surfaces();
    inspect(
        &binder,
        pass.type_environment.published(),
        pass.interner,
        &pass.decl_types,
        &pass.namespace_values,
    );

    // Phase 0.5: build complete flow graphs before narrowed reads are resolved.
    pass.build_flow_graph(binder.module, &program.body);

    // Phase 1: walk the module body and collect relation obligations.
    pass.check_statements(binder.module, &program.body);

    emit_test_incomplete(&mut pass);

    let mut records = finish_event_effects(&mut pass, UserReportingAdapter { event_store });
    let (diagnostics, incomplete) = records.remove(&module_ordinal).unwrap_or_default();
    inspect_final(&pass, next_class_id);

    CheckResult {
        module_ordinal,
        unit_slot,
        diagnostics,
        incomplete,
    }
}

#[cfg(test)]
thread_local! {
    static BOUND_USER_CHECK_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINAL_IDENTITY_INSPECTOR_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(in crate::check::checker) fn bound_user_check_calls_for_test() -> u64 {
    BOUND_USER_CHECK_CALLS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(in crate::check::checker) fn final_identity_inspector_calls_for_test() -> u64 {
    FINAL_IDENTITY_INSPECTOR_CALLS.with(std::cell::Cell::get)
}

/// One parsed project unit handed to the serial M29 project checker.
pub struct ProjectProgram<'ast> {
    pub(crate) module_ordinal: ModuleOrdinal,
    pub(crate) unit_slot: UnitSlot,
    pub(crate) normalized_path: String,
    pub program: &'ast Program<'ast>,
    pub(crate) compilation_unit: CompilationUnit,
    pub imports: Vec<ProjectImport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectSourceBindingRow {
    pub(crate) normalized_path: String,
    pub(crate) source_file_kind: crate::binder::namespace::SourceFileKind,
    pub(crate) external_module: bool,
    pub(crate) original_module_ordinal: OriginalModuleOrdinal,
    pub(crate) unit_slot: UnitSlot,
    pub(crate) source: SourceUnitKey,
    pub(crate) module: ScopeId,
}

pub(crate) struct BoundProjectBinder {
    pub(crate) binder: Binder,
    pub(crate) module_scopes: Vec<ScopeId>,
    pub(crate) module_placeholders: Vec<Vec<ImportPlaceholder>>,
    pub(crate) project_sources: Vec<ProjectSourceBindingRow>,
    #[cfg(test)]
    pub(crate) normalized: ProjectBindingProductForTest,
}

impl std::fmt::Debug for BoundProjectBinder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BoundProjectBinder")
            .field("module_count", &self.module_scopes.len())
            .field("project_sources", &self.project_sources)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectBindingProductForTest {
    pub(crate) normalized_per_path_binding_shape: Vec<String>,
    pub(crate) normalized_import_export_shape: Vec<String>,
    pub(crate) normalized_namespace_shape: Vec<String>,
}

#[cfg(test)]
thread_local! {
    static PROJECT_BINDING_ENTRIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PROJECT_BINDING_FRESH_SEEDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PROJECT_BINDING_CHECKPOINT_SEEDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PROJECT_BINDING_BOUND_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PROJECT_BINDING_PRODUCTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PROJECT_BINDING_ORDINARY_CONSUMERS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PROJECT_BINDING_CONTINUATION_CONSUMERS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PROJECT_BINDING_FRESH_PRODUCTS: std::cell::RefCell<Vec<ProjectBindingProductForTest>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static PROJECT_BINDING_CHECKPOINT_PRODUCTS: std::cell::RefCell<Vec<ProjectBindingProductForTest>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AuthoritativeProjectBindingWorkForTest {
    pub(crate) entries: u64,
    pub(crate) fresh_project_seed_entries: u64,
    pub(crate) authenticated_checkpoint_seed_entries: u64,
    pub(crate) bound_units: u64,
    pub(crate) typed_products_produced: u64,
    pub(crate) ordinary_check_products_consumed: u64,
    pub(crate) continuation_route_products_consumed: u64,
    pub(crate) fresh_project_products: Vec<ProjectBindingProductForTest>,
    pub(crate) authenticated_checkpoint_products: Vec<ProjectBindingProductForTest>,
}

#[cfg(test)]
pub(crate) struct AuthoritativeProjectBindingWorkScopeForTest {
    start: AuthoritativeProjectBindingWorkForTest,
    fresh_len: usize,
    checkpoint_len: usize,
}

#[cfg(test)]
impl AuthoritativeProjectBindingWorkScopeForTest {
    pub(crate) fn start() -> Self {
        Self {
            start: project_binding_work_for_test(),
            fresh_len: PROJECT_BINDING_FRESH_PRODUCTS.with(|products| products.borrow().len()),
            checkpoint_len: PROJECT_BINDING_CHECKPOINT_PRODUCTS
                .with(|products| products.borrow().len()),
        }
    }

    pub(crate) fn finish(self) -> AuthoritativeProjectBindingWorkForTest {
        let end = project_binding_work_for_test();
        AuthoritativeProjectBindingWorkForTest {
            entries: end.entries.saturating_sub(self.start.entries),
            fresh_project_seed_entries: end
                .fresh_project_seed_entries
                .saturating_sub(self.start.fresh_project_seed_entries),
            authenticated_checkpoint_seed_entries: end
                .authenticated_checkpoint_seed_entries
                .saturating_sub(self.start.authenticated_checkpoint_seed_entries),
            bound_units: end.bound_units.saturating_sub(self.start.bound_units),
            typed_products_produced: end
                .typed_products_produced
                .saturating_sub(self.start.typed_products_produced),
            ordinary_check_products_consumed: end
                .ordinary_check_products_consumed
                .saturating_sub(self.start.ordinary_check_products_consumed),
            continuation_route_products_consumed: end
                .continuation_route_products_consumed
                .saturating_sub(self.start.continuation_route_products_consumed),
            fresh_project_products: PROJECT_BINDING_FRESH_PRODUCTS
                .with(|products| products.borrow()[self.fresh_len..].to_vec()),
            authenticated_checkpoint_products: PROJECT_BINDING_CHECKPOINT_PRODUCTS
                .with(|products| products.borrow()[self.checkpoint_len..].to_vec()),
        }
    }
}

#[cfg(test)]
fn project_binding_work_for_test() -> AuthoritativeProjectBindingWorkForTest {
    AuthoritativeProjectBindingWorkForTest {
        entries: PROJECT_BINDING_ENTRIES.get(),
        fresh_project_seed_entries: PROJECT_BINDING_FRESH_SEEDS.get(),
        authenticated_checkpoint_seed_entries: PROJECT_BINDING_CHECKPOINT_SEEDS.get(),
        bound_units: PROJECT_BINDING_BOUND_UNITS.get(),
        typed_products_produced: PROJECT_BINDING_PRODUCTS.get(),
        ordinary_check_products_consumed: PROJECT_BINDING_ORDINARY_CONSUMERS.get(),
        continuation_route_products_consumed: PROJECT_BINDING_CONTINUATION_CONSUMERS.get(),
        fresh_project_products: Vec::new(),
        authenticated_checkpoint_products: Vec::new(),
    }
}

#[cfg(test)]
pub(crate) fn project_binding_thread_receipt_for_test() -> AuthoritativeProjectBindingWorkForTest {
    let mut receipt = project_binding_work_for_test();
    receipt.fresh_project_products =
        PROJECT_BINDING_FRESH_PRODUCTS.with(|products| products.borrow().clone());
    receipt.authenticated_checkpoint_products =
        PROJECT_BINDING_CHECKPOINT_PRODUCTS.with(|products| products.borrow().clone());
    receipt
}

#[cfg(test)]
pub(crate) fn merge_project_binding_thread_receipt_for_test(
    receipt: AuthoritativeProjectBindingWorkForTest,
) {
    PROJECT_BINDING_ENTRIES.set(
        PROJECT_BINDING_ENTRIES
            .get()
            .saturating_add(receipt.entries),
    );
    PROJECT_BINDING_FRESH_SEEDS.set(
        PROJECT_BINDING_FRESH_SEEDS
            .get()
            .saturating_add(receipt.fresh_project_seed_entries),
    );
    PROJECT_BINDING_CHECKPOINT_SEEDS.set(
        PROJECT_BINDING_CHECKPOINT_SEEDS
            .get()
            .saturating_add(receipt.authenticated_checkpoint_seed_entries),
    );
    PROJECT_BINDING_BOUND_UNITS.set(
        PROJECT_BINDING_BOUND_UNITS
            .get()
            .saturating_add(receipt.bound_units),
    );
    PROJECT_BINDING_PRODUCTS.set(
        PROJECT_BINDING_PRODUCTS
            .get()
            .saturating_add(receipt.typed_products_produced),
    );
    PROJECT_BINDING_ORDINARY_CONSUMERS.set(
        PROJECT_BINDING_ORDINARY_CONSUMERS
            .get()
            .saturating_add(receipt.ordinary_check_products_consumed),
    );
    PROJECT_BINDING_CONTINUATION_CONSUMERS.set(
        PROJECT_BINDING_CONTINUATION_CONSUMERS
            .get()
            .saturating_add(receipt.continuation_route_products_consumed),
    );
    PROJECT_BINDING_FRESH_PRODUCTS
        .with(|products| products.borrow_mut().extend(receipt.fresh_project_products));
    PROJECT_BINDING_CHECKPOINT_PRODUCTS.with(|products| {
        products
            .borrow_mut()
            .extend(receipt.authenticated_checkpoint_products)
    });
}

/// One named import after the driver has resolved its module specifier.
pub struct ProjectImport {
    pub local: String,
    pub imported: String,
    pub module: String,
    pub source: ProjectImportSource,
    pub type_only: bool,
    /// Exact local binding-name span used to attach binder identity.
    pub local_span: Span,
    /// Full import-specifier span used for diagnostics.
    pub span: Span,
    /// Owning import-declaration start reserved before project binding.
    pub owner_start: u32,
}

pub enum ProjectImportSource {
    Resolved(usize),
    Missing(String),
}

#[derive(Clone, Copy)]
struct ExportedSlots {
    value: Option<ValueStorageId>,
    ty: Option<TypeGroupId>,
    /// A type-only export hid a real local value slot. Imports must keep its
    /// runtime barrier even though no value declaration crosses the boundary.
    value_erased: bool,
    /// Missing imported/re-exported types hide ambient names without a fake group.
    type_unavailable: bool,
}

type ExportSurface = BTreeMap<String, ExportedSlots>;

/// Check a dependency-ordered project in one serial type universe. Returns one
/// [`CheckResult`] per unit, indexed like `units`.
pub fn check_project_programs<'ast>(
    interner: &mut Interner,
    units: &[ProjectProgram<'ast>],
) -> Vec<CheckResult> {
    check_project_programs_inner(
        interner,
        units,
        |_, _, _| {},
        |_, _, _, _, _, _, _| {},
        |_| {},
    )
}

#[cfg(test)]
pub(in crate::check::checker) fn check_project_programs_with_owned_library<'ast, F, G>(
    state: library_compiler::OwnedLibraryRuntimeState,
    units: &[ProjectProgram<'ast>],
    inspect_bindings: F,
    inspect_final: G,
) -> Vec<CheckResult>
where
    F: FnOnce(&Binder, &[ScopeId]),
    G: FnOnce(&Pass<'_, 'ast>, u32),
{
    if units.is_empty() {
        return Vec::new();
    }

    let (mut interner, binder, base) = state.into_user_project_base();
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    for (slot, unit) in units.iter().enumerate() {
        debug_assert_eq!(unit.unit_slot.index(), slot);
        lexical_events
            .reserve_program(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                &mut event_store,
            )
            .expect("lexical event reservation must reference valid events");
    }

    let BoundUserBase {
        published_types,
        library_semantic_identities,
        lexical_array_alias,
        mut decl_types,
        mut next_type_param,
        mut next_class_id,
        runtime,
    } = base;
    let mut module_scopes = Vec::with_capacity(units.len());
    let mut module_placeholders: Vec<Vec<ImportPlaceholder>> = Vec::with_capacity(units.len());
    let mut external_effects: BTreeMap<UserRecordTicket, CandidateEffects> = BTreeMap::new();
    let (mut builder, first_source) = ProjectBinderBuilder::resume_frozen_library(binder);
    builder.reserve_script_namespace_roots(units.iter().enumerate().map(|(index, unit)| {
        let source = SourceUnitKey(
            first_source
                .0
                .checked_add(u32::try_from(index).expect("project unit count fits u32"))
                .expect("project source key suffix fits u32"),
        );
        (
            unit.program,
            CompilationUnit {
                source,
                origin: unit.compilation_unit.origin,
                binding: unit.compilation_unit.binding,
            },
        )
    }));
    let mut exports: Vec<ExportSurface> = Vec::with_capacity(units.len());
    for (index, unit) in units.iter().enumerate() {
        let imports = imported_symbols(unit, &exports, &lexical_events, &mut external_effects);
        let source = SourceUnitKey(
            first_source
                .0
                .checked_add(u32::try_from(index).expect("project unit count fits u32"))
                .expect("project source key suffix fits u32"),
        );
        let compilation_unit = CompilationUnit {
            source,
            origin: unit.compilation_unit.origin,
            binding: unit.compilation_unit.binding,
        };
        let (scope, placeholders) = builder.add_module(unit.program, &imports, compilation_unit);
        let surface = collect_exports(
            &builder,
            scope,
            unit.program,
            unit.module_ordinal,
            &lexical_events,
            &mut external_effects,
        );
        module_scopes.push(scope);
        module_placeholders.push(placeholders);
        exports.push(surface);
    }
    let binder_module = module_scopes.last().copied().unwrap_or(ScopeId(0));
    let binder = builder
        .finish_frozen_library_continuation(binder_module)
        .expect("collision-free project continuation binds");
    library_compiler::record_user_source_binds_for_test(units.len());
    decl_types.resize(binder.decl_count);

    let (mut type_decls, mut type_resolved) = published_types.construction_prefix();
    let user_type_start = type_decls.len();
    type_resolved.resize(binder.type_groups.len(), None);
    let error = interner.well_known().error;
    for (scope, unit) in module_scopes.iter().copied().zip(units) {
        reserve_type_decls(
            &mut interner,
            &binder,
            scope,
            unit.program,
            &mut next_type_param,
            &mut next_class_id,
            &mut type_decls,
            &mut type_resolved,
        );
        attach_type_decl_owners(
            &mut lexical_events,
            SourceOrdinal::User(unit.module_ordinal),
            &binder,
            scope,
            unit.program,
        );
        attach_class_bindings(
            &mut lexical_events,
            SourceOrdinal::User(unit.module_ordinal),
            &binder,
            scope,
            unit.program,
            &type_decls,
        );
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .expect("one callable binder reservation pass");
    inspect_bindings(&binder, &module_scopes);
    enqueue_local_ambient_export_alias_diagnostics(&binder, &lexical_events, &mut external_effects);
    enqueue_namespace_placement_diagnostics(&binder, &lexical_events, &mut external_effects);
    enqueue_ambient_context_diagnostics(&binder, &lexical_events, &mut external_effects);
    for placeholders in &module_placeholders {
        for placeholder in placeholders {
            if let Some(decl_id) = placeholder.value {
                decl_types.set(decl_id, error);
            }
        }
    }

    let mut pass = build_pass_with_reporting(
        &mut interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReporting {
            source: SourceUnit::User {
                module_ordinal: units[0].module_ordinal,
                unit_slot: units[0].unit_slot,
            },
            lexical_events,
            suppress_effects: false,
        },
    );
    pass.install_published_type_environment_base(published_types);
    pass.lexical_array_alias = lexical_array_alias;
    if let Some(identities) = library_semantic_identities {
        pass.install_library_semantic_identities(identities);
    }
    pass.class_application_parameters = runtime.class_application_parameters;
    pass.class_new_metadata = runtime.class_new_metadata;
    pass.class_parents = runtime.class_parents;
    pass.class_value_aliases = runtime.class_value_aliases;
    pass.class_value_bindings = runtime.class_value_bindings;
    pass.standalone_namespace_value_aliases = runtime.standalone_namespace_value_aliases;
    pass.class_names = runtime.class_names;
    pass.namespace_values
        .install_frozen_terminals(runtime.namespace_terminals);
    pass.named_function_symbols = runtime.named_function_symbols;
    for effects in external_effects.into_values() {
        pass.enqueue_effects(CheckerEffects::from_records(effects));
    }

    pass.fill_type_decls_range(binder.module, user_type_start, pass.type_decls.len());
    let standalone_modules = module_scopes
        .iter()
        .copied()
        .zip(units.iter())
        .map(|(scope, unit)| (scope, unit.program.body.as_slice()))
        .collect::<Vec<_>>();
    pass.prepare_project_attached_namespace_values(&standalone_modules);
    pass.prepare_project_standalone_namespace_values(&standalone_modules);
    pass.publish_class_surfaces();
    pass.finalize_standalone_namespace_values();
    pass.precompute_standalone_namespace_value_aliases(&standalone_modules);
    pass.fill_pending_interfaces_range(binder.module, user_type_start, pass.type_decls.len());
    pass.publish_type_groups();
    pass.validate_published_class_surfaces();

    for (scope, unit) in module_scopes.iter().copied().zip(units) {
        pass.current_module = scope;
        pass.current_source = SourceUnit::User {
            module_ordinal: unit.module_ordinal,
            unit_slot: unit.unit_slot,
        };
        pass.build_flow_graph(scope, &unit.program.body);
        pass.check_statements(scope, &unit.program.body);
        emit_test_incomplete(&mut pass);
    }
    library_compiler::record_user_source_checks_for_test(units.len());

    let mut records = finish_event_effects(&mut pass, UserReportingAdapter { event_store });
    inspect_final(&pass, next_class_id);
    units
        .iter()
        .map(|unit| {
            let (diagnostics, incomplete) =
                records.remove(&unit.module_ordinal).unwrap_or_default();
            CheckResult {
                module_ordinal: unit.module_ordinal,
                unit_slot: unit.unit_slot,
                diagnostics,
                incomplete,
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn check_project_programs_with_binding_inspector<'ast, F>(
    interner: &mut Interner,
    units: &[ProjectProgram<'ast>],
    inspect: F,
) -> Vec<CheckResult>
where
    F: FnOnce(&Binder, &LexicalReservations, &[ScopeId]),
{
    check_project_programs_inner(interner, units, inspect, |_, _, _, _, _, _, _| {}, |_| {})
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProjectNamespaceRootInspection {
    pub(crate) name: String,
    pub(crate) symbol: SymbolId,
    pub(crate) terminal: &'static str,
    pub(crate) namespace_storage: Option<ValueStorageId>,
    pub(crate) terminal_storage: Option<ValueStorageId>,
    pub(crate) ty: Option<TypeId>,
    pub(crate) published: Option<TypeId>,
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProjectReplayRecordInspection {
    Diagnostic(String),
    Incomplete(String),
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProjectReplayInspection {
    pub(crate) key: events::EventKey,
    pub(crate) record: ProjectReplayRecordInspection,
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProjectNamespaceValueInspection {
    pub(crate) roots: Vec<ProjectNamespaceRootInspection>,
    pub(crate) replay: Vec<ProjectReplayInspection>,
}

#[cfg(test)]
pub(crate) fn check_project_programs_with_namespace_value_inspector<'ast, F>(
    interner: &mut Interner,
    units: &[ProjectProgram<'ast>],
    inspect: F,
) -> Vec<CheckResult>
where
    F: FnOnce(&ProjectNamespaceValueInspection),
{
    let roots = std::cell::RefCell::new(Vec::new());
    check_project_programs_inner(
        interner,
        units,
        |_, _, _| {},
        |binder, registry, decl_types, _, _, _, _| {
            let mut inspected = binder
                .namespaces
                .namespaces()
                .map(|namespace| {
                    let (terminal, storage, ty) = match registry.standalone_terminal(namespace.id) {
                        Some(namespace_values::StandaloneNamespaceTerminal::Planned) => {
                            ("planned", None, None)
                        }
                        Some(namespace_values::StandaloneNamespaceTerminal::Ready {
                            storage,
                            ty,
                        }) => ("ready", Some(storage), Some(ty)),
                        Some(namespace_values::StandaloneNamespaceTerminal::Unavailable {
                            ..
                        }) => ("unavailable", None, None),
                        None => ("absent", None, None),
                    };
                    ProjectNamespaceRootInspection {
                        name: namespace.name.clone(),
                        symbol: namespace.symbol,
                        terminal,
                        namespace_storage: binder.namespaces.standalone_value_storage(namespace.id),
                        terminal_storage: storage,
                        ty,
                        published: storage.and_then(|storage| decl_types.get(storage)),
                    }
                })
                .collect::<Vec<_>>();
            inspected.sort_by(|left, right| left.name.cmp(&right.name));
            *roots.borrow_mut() = inspected;
        },
        |records| {
            let replay = records
                .iter()
                .map(|(key, record)| ProjectReplayInspection {
                    key: *key,
                    record: match record {
                        CheckerRecord::Diagnostic(diagnostic) => {
                            ProjectReplayRecordInspection::Diagnostic(
                                diagnostic.code.as_str().to_owned(),
                            )
                        }
                        CheckerRecord::Incomplete(incomplete) => {
                            ProjectReplayRecordInspection::Incomplete(incomplete.id.clone())
                        }
                    },
                })
                .collect();
            inspect(&ProjectNamespaceValueInspection {
                roots: std::mem::take(&mut *roots.borrow_mut()),
                replay,
            });
        },
    )
}

pub(crate) fn bind_authenticated_project_programs(
    checkpoint: crate::binder::bind::AuthenticatedLibraryBinderCheckpoint,
    units: &[ProjectProgram<'_>],
) -> Result<BoundProjectBinder, String> {
    let checkpoint_ends = checkpoint.checkpoint_ends();
    let (builder, _) = checkpoint.into_continuation();
    let source_offset = u32::try_from(checkpoint_ends.next_source)
        .map_err(|_| "library source prefix exceeds u32")?
        .checked_sub(1)
        .ok_or_else(|| "library source prefix omits the prelude".to_owned())?;
    #[cfg(test)]
    {
        PROJECT_BINDING_CHECKPOINT_SEEDS
            .set(PROJECT_BINDING_CHECKPOINT_SEEDS.get().saturating_add(1));
    }
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    for (slot, unit) in units.iter().enumerate() {
        debug_assert_eq!(unit.unit_slot.index(), slot);
        lexical_events
            .reserve_program(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                &mut event_store,
            )
            .map_err(|error| format!("project lexical reservation failed: {error:?}"))?;
    }
    let mut external_effects = BTreeMap::new();
    let bound = bind_authoritative_project_core(
        builder,
        units,
        source_offset,
        &lexical_events,
        &mut external_effects,
    )?;
    #[cfg(test)]
    {
        PROJECT_BINDING_CHECKPOINT_PRODUCTS
            .with(|products| products.borrow_mut().push(bound.normalized.clone()));
    }
    Ok(bound)
}

fn bind_fresh_project_programs(
    prelude: &Program<'_>,
    units: &[ProjectProgram<'_>],
    lexical_events: &LexicalReservations,
    external_effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Result<BoundProjectBinder, String> {
    #[cfg(test)]
    PROJECT_BINDING_FRESH_SEEDS.set(PROJECT_BINDING_FRESH_SEEDS.get().saturating_add(1));
    let bound = bind_authoritative_project_core(
        ProjectBinderBuilder::new(prelude),
        units,
        0,
        lexical_events,
        external_effects,
    )?;
    #[cfg(test)]
    PROJECT_BINDING_FRESH_PRODUCTS
        .with(|products| products.borrow_mut().push(bound.normalized.clone()));
    Ok(bound)
}

fn bind_authoritative_project_core(
    mut builder: ProjectBinderBuilder,
    units: &[ProjectProgram<'_>],
    source_offset: u32,
    lexical_events: &LexicalReservations,
    external_effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Result<BoundProjectBinder, String> {
    #[cfg(test)]
    {
        PROJECT_BINDING_ENTRIES.set(PROJECT_BINDING_ENTRIES.get().saturating_add(1));
        PROJECT_BINDING_BOUND_UNITS.set(
            PROJECT_BINDING_BOUND_UNITS
                .get()
                .saturating_add(u64::try_from(units.len()).unwrap_or(u64::MAX)),
        );
    }
    let shifted_unit = |unit: &ProjectProgram<'_>| -> Result<CompilationUnit, String> {
        let source = SourceUnitKey(
            source_offset
                .checked_add(unit.compilation_unit.source.0)
                .ok_or_else(|| "project source key suffix exceeds u32".to_owned())?,
        );
        Ok(CompilationUnit {
            source,
            origin: unit.compilation_unit.origin,
            binding: unit.compilation_unit.binding,
        })
    };
    let reserved_units = units
        .iter()
        .map(|unit| shifted_unit(unit).map(|compilation| (unit.program, compilation)))
        .collect::<Result<Vec<_>, _>>()?;
    builder.reserve_script_namespace_roots(reserved_units.iter().copied());

    let mut module_scopes = Vec::with_capacity(units.len());
    let mut module_placeholders = Vec::with_capacity(units.len());
    let mut exports: Vec<ExportSurface> = Vec::with_capacity(units.len());
    for unit in units {
        let imports = imported_symbols(unit, &exports, lexical_events, external_effects);
        let compilation = shifted_unit(unit)?;
        let (scope, placeholders) = builder.add_module(unit.program, &imports, compilation);
        let surface = collect_exports(
            &builder,
            scope,
            unit.program,
            unit.module_ordinal,
            lexical_events,
            external_effects,
        );
        module_scopes.push(scope);
        module_placeholders.push(placeholders);
        exports.push(surface);
    }
    let final_module = module_scopes.last().copied().unwrap_or(ScopeId(0));
    let binder = builder.finish(final_module);
    let mut project_sources = units
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|(unit, module)| {
            let CompilationOrigin::User(original_module_ordinal) = unit.compilation_unit.origin
            else {
                return Err("project binding received a non-user compilation origin".to_owned());
            };
            Ok(ProjectSourceBindingRow {
                normalized_path: unit.normalized_path.clone(),
                source_file_kind: unit.compilation_unit.binding.source_file_kind,
                external_module: unit.compilation_unit.binding.external_module,
                original_module_ordinal,
                unit_slot: unit.unit_slot,
                source: shifted_unit(unit)?.source,
                module,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    project_sources.sort_by(|left, right| left.normalized_path.cmp(&right.normalized_path));
    #[cfg(test)]
    let normalized =
        normalized_project_binding_product(&binder, units, &module_scopes, &module_placeholders);
    #[cfg(test)]
    PROJECT_BINDING_PRODUCTS.set(PROJECT_BINDING_PRODUCTS.get().saturating_add(1));
    Ok(BoundProjectBinder {
        binder,
        module_scopes,
        module_placeholders,
        project_sources,
        #[cfg(test)]
        normalized,
    })
}

#[cfg(test)]
fn normalized_project_binding_product(
    binder: &Binder,
    units: &[ProjectProgram<'_>],
    module_scopes: &[ScopeId],
    module_placeholders: &[Vec<ImportPlaceholder>],
) -> ProjectBindingProductForTest {
    let mut rows = units
        .iter()
        .zip(module_scopes.iter().copied())
        .zip(module_placeholders)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.0 .0.normalized_path.cmp(&right.0 .0.normalized_path));
    let mut value_labels = BTreeMap::<u32, usize>::new();
    let mut type_labels = BTreeMap::<u32, usize>::new();
    let mut namespace_labels = BTreeMap::<u32, usize>::new();
    let mut per_path = Vec::new();
    let mut imports = Vec::new();
    let mut namespaces = Vec::new();
    for ((unit, module), placeholders) in rows {
        let mut names = binder
            .graph
            .get(module)
            .into_iter()
            .flat_map(|scope| scope.symbols.keys())
            .cloned()
            .collect::<Vec<_>>();
        if !unit.compilation_unit.binding.external_module {
            let census = source_global_binding_census(unit.program, unit.compilation_unit.binding);
            names.extend(census.candidates.into_keys());
            names.extend(census.uncertain_candidates.into_keys());
        }
        names.sort();
        names.dedup();
        for name in names {
            let Some(symbol_id) = binder.graph.resolve(module, &name) else {
                continue;
            };
            let Some(symbol) = binder.symbols.get(symbol_id) else {
                continue;
            };
            let next_value = value_labels.len();
            let value = symbol
                .value
                .map(|id| *value_labels.entry(id.0).or_insert(next_value));
            let next_type = type_labels.len();
            let ty = symbol
                .ty
                .map(|id| *type_labels.entry(id.0).or_insert(next_type));
            let next_namespace = namespace_labels.len();
            let namespace = symbol
                .ns
                .map(|id| *namespace_labels.entry(id.0).or_insert(next_namespace));
            per_path.push(format!(
                "{}|{}|v={value:?}|t={ty:?}|n={namespace:?}|bv={}|bt={}|bn={}",
                unit.normalized_path,
                name,
                symbol.blocks_value_lookup,
                symbol.blocks_type_lookup,
                symbol.blocks_namespace_lookup
            ));
            if let Some(attachment) = binder.namespace_value_attachment(module, &name) {
                namespaces.push(format!(
                    "{}|{}|attached={:?}|members={}",
                    unit.normalized_path,
                    name,
                    attachment.disposition,
                    attachment.members.len()
                ));
            } else if let Some(namespace) = symbol.ns {
                namespaces.push(format!(
                    "{}|{}|standalone={}",
                    unit.normalized_path,
                    name,
                    binder
                        .namespaces
                        .standalone_value_storage(namespace)
                        .is_some()
                ));
            }
        }
        for import in &unit.imports {
            let symbol = binder
                .graph
                .get(module)
                .and_then(|scope| scope.lookup_local(&import.local))
                .and_then(|id| binder.symbols.get(id));
            imports.push(format!(
                "{}|{}<-{}|missing={}|v={}|t={}|bv={}|bt={}",
                unit.normalized_path,
                import.local,
                import.imported,
                matches!(import.source, ProjectImportSource::Missing(_)),
                symbol.is_some_and(|symbol| symbol.value.is_some()),
                symbol.is_some_and(|symbol| symbol.ty.is_some()),
                symbol.is_some_and(|symbol| symbol.blocks_value_lookup),
                symbol.is_some_and(|symbol| symbol.blocks_type_lookup)
            ));
        }
        imports.push(format!(
            "{}|placeholder_values={}",
            unit.normalized_path,
            placeholders
                .iter()
                .filter(|placeholder| placeholder.value.is_some())
                .count()
        ));
    }
    ProjectBindingProductForTest {
        normalized_per_path_binding_shape: per_path,
        normalized_import_export_shape: imports,
        normalized_namespace_shape: namespaces,
    }
}

#[cfg(test)]
pub(crate) fn record_continuation_project_binding_consumed_for_test() {
    PROJECT_BINDING_CONTINUATION_CONSUMERS.set(
        PROJECT_BINDING_CONTINUATION_CONSUMERS
            .get()
            .saturating_add(1),
    );
}

fn check_project_programs_inner<'ast, F, G, H>(
    interner: &mut Interner,
    units: &[ProjectProgram<'ast>],
    inspect_bindings: F,
    inspect_namespace_values: G,
    inspect_replay: H,
) -> Vec<CheckResult>
where
    F: FnOnce(&Binder, &LexicalReservations, &[ScopeId]),
    G: FnOnce(
        &Binder,
        &namespace_values::NamespaceValueRegistry,
        &DeclTypes,
        &Interner,
        &LexicalReservations,
        &EventStore,
        &[ScopeId],
    ),
    H: FnOnce(&[(events::EventKey, CheckerRecord)]),
{
    if units.is_empty() {
        return Vec::new();
    }

    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    for (slot, unit) in units.iter().enumerate() {
        debug_assert_eq!(unit.unit_slot.index(), slot);
        lexical_events
            .reserve_program(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                &mut event_store,
            )
            .expect("lexical event reservation must reference valid events");
    }

    let mut module_scopes = Vec::with_capacity(units.len());
    let mut module_placeholders: Vec<Vec<ImportPlaceholder>> = Vec::with_capacity(units.len());
    let mut external_effects: BTreeMap<UserRecordTicket, CandidateEffects> = BTreeMap::new();
    let (
        binder,
        TrustedPreludeHandoff {
            published_types,
            library_semantic_identities,
            lexical_array_alias,
            mut decl_types,
            mut next_type_param,
            mut next_class_id,
        },
    ) = bootstrap_trusted_prelude(interner, |prelude| {
        let bound =
            bind_fresh_project_programs(prelude, units, &lexical_events, &mut external_effects)
                .expect("authoritative fresh project binding succeeds");
        module_scopes = bound.module_scopes;
        module_placeholders = bound.module_placeholders;
        bound.binder
    });
    #[cfg(test)]
    PROJECT_BINDING_ORDINARY_CONSUMERS
        .set(PROJECT_BINDING_ORDINARY_CONSUMERS.get().saturating_add(1));

    let (mut type_decls, mut type_resolved) = published_types.construction_prefix();
    let user_type_start = type_decls.len();
    type_resolved.resize(binder.type_groups.len(), None);
    let error = interner.well_known().error;
    for (scope, unit) in module_scopes.iter().copied().zip(units) {
        reserve_type_decls(
            interner,
            &binder,
            scope,
            unit.program,
            &mut next_type_param,
            &mut next_class_id,
            &mut type_decls,
            &mut type_resolved,
        );
        attach_type_decl_owners(
            &mut lexical_events,
            SourceOrdinal::User(unit.module_ordinal),
            &binder,
            scope,
            unit.program,
        );
        attach_class_bindings(
            &mut lexical_events,
            SourceOrdinal::User(unit.module_ordinal),
            &binder,
            scope,
            unit.program,
            &type_decls,
        );
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .expect("one callable binder reservation pass");
    let unreserved_user_groups: Vec<_> = type_decls
        .iter()
        .enumerate()
        .map(|(index, declaration)| (user_type_start + index, declaration))
        .filter(|(_, declaration)| matches!(declaration, TypeDecl::Resolved { .. }))
        .map(|(index, _)| {
            let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
            (group, binder.type_groups.get(group).cloned())
        })
        .collect();
    assert!(
        unreserved_user_groups.is_empty(),
        "every user type group has one construction draft: {unreserved_user_groups:?}"
    );
    inspect_bindings(&binder, &lexical_events, &module_scopes);
    enqueue_local_ambient_export_alias_diagnostics(&binder, &lexical_events, &mut external_effects);
    enqueue_namespace_placement_diagnostics(&binder, &lexical_events, &mut external_effects);
    enqueue_ambient_context_diagnostics(&binder, &lexical_events, &mut external_effects);

    for placeholders in &module_placeholders {
        for placeholder in placeholders {
            if let Some(decl_id) = placeholder.value {
                decl_types.set(decl_id, error);
            }
        }
    }
    let mut pass = build_pass_with_reporting(
        interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReporting {
            source: SourceUnit::User {
                module_ordinal: units[0].module_ordinal,
                unit_slot: units[0].unit_slot,
            },
            lexical_events,
            suppress_effects: false,
        },
    );
    pass.install_published_type_environment_base(published_types);
    pass.lexical_array_alias = lexical_array_alias;
    if let Some(identities) = library_semantic_identities {
        pass.install_library_semantic_identities(identities);
    }
    for effects in external_effects.into_values() {
        pass.enqueue_effects(CheckerEffects::from_records(effects));
    }

    pass.fill_type_decls_range(binder.module, user_type_start, pass.type_decls.len());

    let standalone_modules = module_scopes
        .iter()
        .copied()
        .zip(units.iter())
        .map(|(scope, unit)| (scope, unit.program.body.as_slice()))
        .collect::<Vec<_>>();
    pass.prepare_project_attached_namespace_values(&standalone_modules);
    pass.prepare_project_standalone_namespace_values(&standalone_modules);

    pass.publish_class_surfaces();
    pass.finalize_standalone_namespace_values();
    pass.precompute_standalone_namespace_value_aliases(&standalone_modules);

    pass.fill_pending_interfaces_range(binder.module, user_type_start, pass.type_decls.len());
    pass.publish_type_groups();
    pass.validate_published_class_surfaces();
    inspect_namespace_values(
        &binder,
        &pass.namespace_values,
        &pass.decl_types,
        pass.interner,
        &pass.lexical_events,
        &event_store,
        &module_scopes,
    );

    for (scope, unit) in module_scopes.iter().copied().zip(units) {
        pass.current_module = scope;
        pass.current_source = SourceUnit::User {
            module_ordinal: unit.module_ordinal,
            unit_slot: unit.unit_slot,
        };
        pass.build_flow_graph(scope, &unit.program.body);
        pass.check_statements(scope, &unit.program.body);
        emit_test_incomplete(&mut pass);
    }

    let mut records = finish_event_effects_with_inspector(
        &mut pass,
        UserReportingAdapter { event_store },
        inspect_replay,
    );
    units
        .iter()
        .map(|unit| {
            let (diagnostics, incomplete) =
                records.remove(&unit.module_ordinal).unwrap_or_default();
            CheckResult {
                module_ordinal: unit.module_ordinal,
                unit_slot: unit.unit_slot,
                diagnostics,
                incomplete,
            }
        })
        .collect()
}

fn imported_symbols(
    unit: &ProjectProgram<'_>,
    exports: &[ExportSurface],
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Vec<ImportedSymbol> {
    let mut imports = Vec::new();
    for import in &unit.imports {
        match &import.source {
            ProjectImportSource::Missing(module) => {
                enqueue_external_diagnostic(
                    reservations,
                    effects,
                    unit.module_ordinal,
                    import.owner_start,
                    Diagnostic::cannot_find_module(import.span, module),
                );
                imports.push(placeholder_import(import));
            }
            ProjectImportSource::Resolved(module_index) => {
                let slots = exports
                    .get(*module_index)
                    .and_then(|surface| surface.get(&import.imported))
                    .copied();
                match slots {
                    Some(slots) => {
                        let value_barrier =
                            slots.value_erased || (import.type_only && slots.value.is_some());
                        let value = if import.type_only { None } else { slots.value };
                        imports.push(ImportedSymbol::new(
                            import.local.clone(),
                            value,
                            slots.ty,
                            value_barrier,
                            slots.type_unavailable,
                            import.local_span,
                        ));
                    }
                    None => {
                        enqueue_external_diagnostic(
                            reservations,
                            effects,
                            unit.module_ordinal,
                            import.owner_start,
                            Diagnostic::no_exported_member(
                                import.span,
                                &import.module,
                                &import.imported,
                            ),
                        );
                        imports.push(placeholder_import(import));
                    }
                }
            }
        }
    }
    imports
}

fn placeholder_import(import: &ProjectImport) -> ImportedSymbol {
    if import.type_only {
        ImportedSymbol::placeholder_type(import.local.clone(), import.local_span)
    } else {
        ImportedSymbol::placeholder_value_and_type(import.local.clone(), import.local_span)
    }
}

fn collect_exports(
    builder: &ProjectBinderBuilder,
    scope: ScopeId,
    program: &Program<'_>,
    module_ordinal: ModuleOrdinal,
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> ExportSurface {
    let mut surface = ExportSurface::new();
    for stmt in &program.body {
        let Statement::ExportNamedDeclaration(export) = stmt else {
            continue;
        };
        if export.source.is_some() {
            continue;
        }
        if let Some(decl) = &export.declaration {
            collect_declaration_export(builder, scope, decl, &mut surface);
        } else {
            // `export type { x }` marks the whole statement type-only; mirror the
            // import side (driver.rs) where the outer kind ORs with each specifier.
            let outer_type_only = export.export_kind == ImportOrExportKind::Type;
            let mut context = ListExportContext {
                builder,
                scope,
                surface: &mut surface,
                module_ordinal,
                reservations,
                effects,
            };
            for specifier in &export.specifiers {
                collect_list_export(&mut context, specifier, outer_type_only, stmt.span().start);
            }
        }
    }
    surface
}

fn collect_declaration_export(
    builder: &ProjectBinderBuilder,
    scope: ScopeId,
    decl: &Declaration<'_>,
    surface: &mut ExportSurface,
) {
    match decl {
        Declaration::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                if let Some(name) = binding_name(&declarator.id) {
                    let (value, _) = builder.local_symbol_slots(scope, name);
                    surface.insert(
                        name.to_string(),
                        ExportedSlots {
                            value,
                            ty: None,
                            value_erased: false,
                            type_unavailable: false,
                        },
                    );
                }
            }
        }
        Declaration::FunctionDeclaration(func) => {
            if let Some(id) = &func.id {
                let (value, _) = builder.local_symbol_slots(scope, id.name.as_str());
                surface.insert(
                    id.name.to_string(),
                    ExportedSlots {
                        value,
                        ty: None,
                        value_erased: false,
                        type_unavailable: false,
                    },
                );
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                let (value, ty) = builder.local_symbol_slots(scope, id.name.as_str());
                surface.insert(
                    id.name.to_string(),
                    ExportedSlots {
                        value,
                        ty,
                        value_erased: false,
                        type_unavailable: false,
                    },
                );
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => {
            let (_, ty) = builder.local_symbol_slots(scope, alias.id.name.as_str());
            surface.insert(
                alias.id.name.to_string(),
                ExportedSlots {
                    value: None,
                    ty,
                    value_erased: false,
                    type_unavailable: false,
                },
            );
        }
        Declaration::TSInterfaceDeclaration(iface) => {
            let (_, ty) = builder.local_symbol_slots(scope, iface.id.name.as_str());
            surface.insert(
                iface.id.name.to_string(),
                ExportedSlots {
                    value: None,
                    ty,
                    value_erased: false,
                    type_unavailable: false,
                },
            );
        }
        _ => {}
    }
}

struct ListExportContext<'a> {
    builder: &'a ProjectBinderBuilder,
    scope: ScopeId,
    surface: &'a mut ExportSurface,
    module_ordinal: ModuleOrdinal,
    reservations: &'a LexicalReservations,
    effects: &'a mut BTreeMap<UserRecordTicket, CandidateEffects>,
}

fn collect_list_export(
    context: &mut ListExportContext<'_>,
    specifier: &ExportSpecifier<'_>,
    outer_type_only: bool,
    owner_start: u32,
) {
    let Some(local) = module_export_name(&specifier.local) else {
        return;
    };
    let Some(exported) = module_export_name(&specifier.exported) else {
        return;
    };
    let (mut value, ty) = context.builder.local_symbol_slots(context.scope, local);
    let local_value_barrier = context
        .builder
        .local_value_lookup_barrier(context.scope, local);
    let local_type_barrier = context
        .builder
        .local_type_lookup_barrier(context.scope, local);
    // Existence is judged against the real symbol; a value-only local is still a
    // valid `export type { x }` target (the error surfaces on the importer, below).
    if value.is_none() && ty.is_none() && !local_value_barrier && !local_type_barrier {
        enqueue_external_diagnostic(
            context.reservations,
            context.effects,
            context.module_ordinal,
            owner_start,
            Diagnostic::cannot_find_name(Span::from_oxc(specifier.local.span()), local),
        );
        return;
    }
    // A type-only specifier (`export type { x }` or `export { type x }`) must not
    // supply a runtime value — suppress the value slot so a non-type-only import
    // cannot use it as a value (tsc TS1362; M29 stand-in is TK2304 on the importer).
    let type_only = outer_type_only || specifier.export_kind == ImportOrExportKind::Type;
    let value_erased = local_value_barrier || (type_only && value.is_some());
    if type_only {
        value = None;
    }
    context.surface.insert(
        exported.to_string(),
        ExportedSlots {
            value,
            ty,
            value_erased,
            type_unavailable: local_type_barrier,
        },
    );
}

fn enqueue_external_diagnostic(
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
    module_ordinal: ModuleOrdinal,
    owner_start: u32,
    diagnostic: Diagnostic,
) {
    let owner = reservations
        .owner_at(
            SourceOrdinal::User(module_ordinal),
            owner_start,
            LexicalOwnerPhase::Immediate,
        )
        .expect("import/export diagnostic owner must be lexically reserved");
    effects
        .entry(owner.ticket)
        .or_insert_with(|| CandidateEffects::new(owner.ticket))
        .diagnostic(diagnostic);
}

fn enqueue_local_ambient_export_alias_diagnostics(
    binder: &Binder,
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) {
    for failure in binder.local_ambient_export_alias_failures() {
        let Some(original_module) = user_original_module(failure.origin) else {
            continue;
        };
        let module_ordinal = ModuleOrdinal::new(original_module.index());
        let owner = reservations
            .export_alias_owner(SourceOrdinal::User(module_ordinal), failure.local_span)
            .expect("local ambient export alias must have an exact lexical owner");
        effects
            .entry(owner.ticket)
            .or_insert_with(|| CandidateEffects::new(owner.ticket))
            .diagnostic(match failure.kind {
                LocalAmbientExportAliasFailureKind::Missing => {
                    Diagnostic::cannot_find_name(failure.local_span, &failure.local_name)
                }
                LocalAmbientExportAliasFailureKind::NonLocal => {
                    Diagnostic::cannot_export_non_local(failure.local_span, &failure.local_name)
                }
            });
    }
}

fn enqueue_namespace_placement_diagnostics(
    binder: &Binder,
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) {
    for issue in binder.namespaces.local_placement_issues() {
        let Some(original_module) = user_original_module(issue.origin) else {
            continue;
        };
        let owner = reservations
            .declaration_owner(issue.owner)
            .expect("namespace placement issue must keep its declaration owner");
        let source = reservations
            .declaration_source(issue.owner)
            .expect("namespace placement issue must keep its source site");
        let expected_module = ModuleOrdinal::new(original_module.index());
        assert_eq!(
            source.ordinal(),
            SourceOrdinal::User(expected_module),
            "namespace placement issue must remain in its original module"
        );
        let diagnostic = match issue.kind {
            PlacementIssueKind::FutureTk2434 => {
                Diagnostic::namespace_precedes_class_or_function(issue.span)
            }
        };
        effects
            .entry(owner.ticket)
            .or_insert_with(|| CandidateEffects::new(owner.ticket))
            .diagnostic(diagnostic);
    }
}

fn enqueue_ambient_context_diagnostics(
    binder: &Binder,
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) {
    for global in binder.namespaces.local_globals() {
        let Some(original_module) = user_original_module(global.origin) else {
            continue;
        };
        let owner = reservations
            .declaration_owner(global.declaration)
            .expect("global context issue must keep its declaration owner");
        let source = reservations
            .declaration_source(global.declaration)
            .expect("global context issue must keep its source site");
        let expected_module = ModuleOrdinal::new(original_module.index());
        assert_eq!(source.ordinal(), SourceOrdinal::User(expected_module));
        let candidate = effects
            .entry(owner.ticket)
            .or_insert_with(|| CandidateEffects::new(owner.ticket));
        for issue in &global.issues {
            candidate.diagnostic(match issue {
                GlobalIssue::FutureTk2669 => {
                    Diagnostic::global_augmentation_requires_module(global.diagnostic_span)
                }
                GlobalIssue::FutureTk2670 => {
                    Diagnostic::global_augmentation_requires_declare(global.diagnostic_span)
                }
            });
        }
    }

    for export in binder.namespaces.local_umd_exports() {
        let Some(original_module) = user_original_module(export.origin) else {
            continue;
        };
        let diagnostic = match export.context {
            UmdContext::FutureTk1314NonExternal => Some(
                Diagnostic::global_module_export_requires_module(export.span),
            ),
            UmdContext::FutureTk1315Implementation => Some(
                Diagnostic::global_module_export_requires_declaration_file(export.span),
            ),
            UmdContext::FutureTk1316Nested | UmdContext::DeferredValidBacklog15 => None,
        };
        let Some(diagnostic) = diagnostic else {
            continue;
        };
        let owner = reservations
            .declaration_owner(export.declaration)
            .expect("UMD context issue must keep its declaration owner");
        let source = reservations
            .declaration_source(export.declaration)
            .expect("UMD context issue must keep its source site");
        let expected_module = ModuleOrdinal::new(original_module.index());
        assert_eq!(source.ordinal(), SourceOrdinal::User(expected_module));
        effects
            .entry(owner.ticket)
            .or_insert_with(|| CandidateEffects::new(owner.ticket))
            .diagnostic(diagnostic);
    }
}

fn module_export_name<'ast>(name: &'ast ModuleExportName<'ast>) -> Option<&'ast str> {
    match name {
        ModuleExportName::IdentifierName(id) => Some(id.name.as_str()),
        ModuleExportName::IdentifierReference(id) => Some(id.name.as_str()),
        ModuleExportName::StringLiteral(_) => None,
    }
}

fn binding_name<'a>(pattern: &'a oxc_ast::ast::BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        oxc_ast::ast::BindingPattern::BindingIdentifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

fn prelude_intrinsic_markers(interner: &Interner) -> [TypeId; 6] {
    let wk = interner.well_known();
    [
        wk.uppercase,
        wk.lowercase,
        wk.capitalize,
        wk.uncapitalize,
        wk.this_type,
        wk.omit_this_parameter,
    ]
}

fn seed_prelude_intrinsics(
    binder: &Binder,
    type_resolved: &mut TypeResolvedTable,
    markers: [TypeId; 6],
) {
    for (name, marker) in [
        "Uppercase",
        "Lowercase",
        "Capitalize",
        "Uncapitalize",
        "ThisType",
        "OmitThisParameter",
    ]
    .into_iter()
    .zip(markers)
    {
        if let Some(decl_id) = type_decl_id(binder, binder.prelude_module, name) {
            if let Some(slot) = type_resolved.get_mut(decl_id.index()) {
                *slot = Some(marker);
            }
        }
    }
}

fn attach_type_decl_owners<Ticket: Copy + PartialEq>(
    reservations: &mut LexicalReservations<Ticket>,
    source_ordinal: SourceOrdinal,
    binder: &Binder,
    scope: ScopeId,
    program: &Program<'_>,
) {
    for declaration in binder
        .declarations
        .local_declarations()
        .filter(|declaration| declaration.site.module == scope)
    {
        reservations
            .attach_declaration_owner(
                declaration.id,
                source_ordinal,
                declaration.kind,
                declaration.site.declaration_span,
                declaration.site.binding_span,
            )
            .expect("source declaration must have its exact lexical event owner");
    }

    let _ = program;
}

fn attach_class_bindings<Ticket: Copy + PartialEq>(
    reservations: &mut LexicalReservations<Ticket>,
    source_ordinal: SourceOrdinal,
    binder: &Binder,
    scope: ScopeId,
    program: &Program<'_>,
    declarations: &TypeDeclTable<'_>,
) {
    let mut reserved_class_owners = BTreeMap::new();
    walk_type_decls(binder, scope, program, &mut |_, _, declaration| {
        let TopTypeDecl::Class(class) = declaration else {
            return;
        };
        let Some(binding) = class.id.as_ref() else {
            return;
        };
        let Some(site) = reservations.class_at(source_ordinal, class.span.start) else {
            return;
        };
        let Some(declaration) =
            binder.exact_declaration_at(scope, binding.span.start, DeclarationKind::Class)
        else {
            return;
        };
        let Some(type_decl) = declaration.type_group else {
            return;
        };
        let Some(type_declaration) = declarations.get(type_decl.index()) else {
            return;
        };
        let (class_id, lexical_params) = match type_declaration {
            TypeDecl::Class {
                class_id,
                class_params,
                ..
            } => (*class_id, class_params),
            _ => return,
        };
        let previous = reserved_class_owners.insert(class_id, type_decl);
        debug_assert!(
            previous.is_none(),
            "one source class owns each reserved nominal identity"
        );
        reservations
            .attach_class_binding(
                site,
                ClassBinding {
                    class_id,
                    type_decl,
                    value_decl: declaration.value_storage,
                    header_type_params: lexical_params.clone(),
                },
            )
            .expect("one class binding attachment per lexical class site");
    });
}

fn consume_relation_exhaustion<Ticket: Copy + PartialEq>(
    pass: &mut Pass<'_, '_, Ticket>,
    effects: CheckerEffects<Ticket>,
    exhaustion: Exhaustion,
    span: Span,
) -> CheckerEffects<Ticket> {
    pass.effect_stack.push(effects);
    let _: Option<TypeId> = pass.own_type_demand(DemandOutcome::Exhausted(exhaustion), span);
    pass.effect_stack
        .pop()
        .expect("relation exhaustion lexical effect frame")
}

fn consume_interface_relation_decision<Ticket: Copy + PartialEq>(
    pass: &mut Pass<'_, '_, Ticket>,
    effects: CheckerEffects<Ticket>,
    decision: Result<bool, Exhaustion>,
    span: Span,
) -> (CheckerEffects<Ticket>, Option<bool>) {
    match decision {
        Ok(failed) => (effects, Some(failed)),
        Err(exhaustion) => (
            consume_relation_exhaustion(pass, effects, exhaustion, span),
            None,
        ),
    }
}

fn finish_event_effects(
    pass: &mut Pass<'_, '_>,
    reporting: UserReportingAdapter,
) -> BTreeMap<ModuleOrdinal, (Vec<Diagnostic>, Vec<IncompleteSurface>)> {
    finish_event_effects_with_inspector(pass, reporting, |_| {})
}

fn finish_event_effects_with_inspector<F>(
    pass: &mut Pass<'_, '_>,
    reporting: UserReportingAdapter,
    inspect: F,
) -> BTreeMap<ModuleOrdinal, (Vec<Diagnostic>, Vec<IncompleteSurface>)>
where
    F: FnOnce(&[(events::EventKey, CheckerRecord)]),
{
    let batches = finish_semantic_effects(pass);
    reporting.finish(batches, inspect)
}

fn finish_semantic_effects<Ticket: Copy + PartialEq>(
    pass: &mut Pass<'_, '_, Ticket>,
) -> Vec<CheckerRecordBatch<Ticket>> {
    let pending = std::mem::take(&mut pass.pending_effects);
    let mut completed = Vec::with_capacity(pending.len());
    for mut effects in pending {
        let mut reported_heritage_pairs = BTreeSet::new();
        let mut reported_header_groups = BTreeSet::new();
        let (checks, check_owners) = effects.take_constraint_checks();
        for (index, check) in checks.into_iter().enumerate() {
            let owner = check_owners
                .as_ref()
                .and_then(|owners| owners.get(index))
                .copied()
                .flatten();
            let _replay_scope =
                owner.and_then(|owner| pass.replay_trace.as_ref().map(|trace| trace.scope(owner)));
            pass.effect_stack.push(effects);
            pass.check_constraint_arguments(&check.checks, &check.substitutions);
            effects = pass
                .effect_stack
                .pop()
                .expect("constraint obligation lexical effect frame");
        }
        let (relations, relation_owners) = effects.take_interface_relations();
        for (index, check) in relations.into_iter().enumerate() {
            let owner = relation_owners
                .as_ref()
                .and_then(|owners| owners.get(index))
                .copied()
                .flatten();
            let _replay_scope =
                owner.and_then(|owner| pass.replay_trace.as_ref().map(|trace| trace.scope(owner)));
            let decision = if matches!(
                &check.kind,
                InterfaceRelationKind::Heritage { .. }
                    | InterfaceRelationKind::MergedProperty { .. }
                    | InterfaceRelationKind::HeaderMetadata { .. }
            ) {
                match pass
                    .with_semantic_query(|query| query.is_identical(check.source, check.target))
                {
                    DemandOutcome::Ready(identical) => Ok(!identical),
                    DemandOutcome::Exhausted(exhaustion) => Err(exhaustion),
                }
            } else {
                match pass
                    .with_semantic_query(|query| query.is_assignable(check.source, check.target))
                {
                    RelationOutcome::Yes => Ok(false),
                    RelationOutcome::No(_) => Ok(true),
                    RelationOutcome::Exhausted(exhaustion) => Err(exhaustion),
                }
            };
            let consumed = consume_interface_relation_decision(pass, effects, decision, check.span);
            effects = consumed.0;
            let Some(failed) = consumed.1 else {
                continue;
            };
            let report_failure = failed
                && match check.report {
                    InterfaceRelationReport::Always => true,
                    InterfaceRelationReport::FirstFailedHeritagePair(pair) => {
                        reported_heritage_pairs.insert(pair)
                    }
                    InterfaceRelationReport::FirstFailedHeaderGroup(group) => {
                        reported_header_groups.insert(group)
                    }
                };
            if report_failure {
                let source = render_type(pass.interner.store(), check.source, false);
                let target = render_type(pass.interner.store(), check.target, false);
                let diagnostic = match check.kind {
                    InterfaceRelationKind::NumberIndex => {
                        Diagnostic::number_index_incompatible(check.span, &source, &target)
                    }
                    InterfaceRelationKind::PropertyStringIndex { name } => {
                        Diagnostic::property_incompatible_with_string_index(
                            check.span, &name, &source, &target,
                        )
                    }
                    InterfaceRelationKind::Heritage { left, right } => {
                        Diagnostic::incompatible_interface_heritage(check.span, &left, &right)
                    }
                    InterfaceRelationKind::MergedProperty { name } => {
                        Diagnostic::subsequent_property_type(check.span, &name)
                    }
                    InterfaceRelationKind::HeaderMetadata { name } => {
                        Diagnostic::merged_interface_type_parameters(check.span, &name)
                    }
                    InterfaceRelationKind::HeritageMember { derived, base }
                    | InterfaceRelationKind::HeritageIndex { derived, base } => {
                        Diagnostic::incorrectly_extends_interface(check.span, &derived, &base)
                    }
                };
                pass.append_effect_diagnostic(&mut effects, diagnostic);
            }
        }
        let (obligations, obligation_owners) = effects.take_obligations();
        for (index, obligation) in obligations.into_iter().enumerate() {
            let owner = obligation_owners
                .as_ref()
                .and_then(|owners| owners.get(index))
                .copied()
                .flatten();
            let _replay_scope =
                owner.and_then(|owner| pass.replay_trace.as_ref().map(|trace| trace.scope(owner)));
            match obligation {
                DeferredRelationObligation::Assign(obligation) => {
                    let outcome = pass.with_semantic_query(|query| {
                        query.is_assignable(obligation.src, obligation.tgt)
                    });
                    match outcome {
                        RelationOutcome::Yes => {}
                        RelationOutcome::No(chain) => {
                            let diagnostic = emit_obligation_failure(
                                pass.interner.store(),
                                &obligation,
                                chain.head(),
                            );
                            pass.append_effect_diagnostic(&mut effects, diagnostic);
                        }
                        RelationOutcome::Exhausted(Exhaustion::ClassProjectionBudget) => {
                            let diagnostic =
                                emit_exhausted_obligation(pass.interner.store(), &obligation);
                            pass.append_effect_diagnostic(&mut effects, diagnostic);
                            pass.append_effect_incomplete(
                                &mut effects,
                                IncompleteSurface::new(
                                    "relation/class-projection-budget",
                                    obligation.src_span,
                                    "class projection budget exhausted",
                                ),
                            );
                        }
                        RelationOutcome::Exhausted(Exhaustion::ClassNotPublished { .. })
                        | RelationOutcome::Exhausted(Exhaustion::ClassHeritagePoison { .. })
                        | RelationOutcome::Exhausted(Exhaustion::ClassInitializerPoison {
                            ..
                        })
                        | RelationOutcome::Exhausted(Exhaustion::ClassSurfacePoison { .. })
                        | RelationOutcome::Exhausted(Exhaustion::ClassApplicationArguments(_))
                        | RelationOutcome::Exhausted(Exhaustion::EvaluationBudget)
                        | RelationOutcome::Exhausted(Exhaustion::EvaluationCycle { .. }) => {}
                    }
                }
                DeferredRelationObligation::AssertionCompatibility(obligation) => {
                    let source_to_asserted = pass.with_semantic_query(|query| {
                        query.is_assignable(obligation.source, obligation.asserted)
                    });
                    if matches!(&source_to_asserted, RelationOutcome::Yes) {
                        continue;
                    }
                    let asserted_to_source = pass.with_semantic_query(|query| {
                        query.is_assignable(obligation.asserted, obligation.source)
                    });
                    if matches!(&asserted_to_source, RelationOutcome::Yes) {
                        continue;
                    }
                    let outcomes = [source_to_asserted, asserted_to_source];
                    if outcomes
                        .iter()
                        .all(|outcome| matches!(outcome, RelationOutcome::No(_)))
                    {
                        pass.append_effect_incomplete(
                            &mut effects,
                            IncompleteSurface::new(
                                obligation.syntax.incomplete_id(),
                                obligation.span,
                                "assertion source/target compatibility not validated",
                            ),
                        );
                        continue;
                    }
                    let mut exhaustions = Vec::new();
                    for outcome in outcomes {
                        let RelationOutcome::Exhausted(exhaustion) = outcome else {
                            continue;
                        };
                        if !exhaustions.contains(&exhaustion) {
                            exhaustions.push(exhaustion);
                        }
                    }
                    for exhaustion in exhaustions {
                        effects =
                            consume_relation_exhaustion(pass, effects, exhaustion, obligation.span);
                    }
                }
            }
        }
        let (override_checks, override_owners) = effects.take_override_checks();
        for (index, check) in override_checks.into_iter().enumerate() {
            let owner = override_owners
                .as_ref()
                .and_then(|owners| owners.get(index))
                .copied()
                .flatten();
            let _replay_scope =
                owner.and_then(|owner| pass.replay_trace.as_ref().map(|trace| trace.scope(owner)));
            if check.base_is_method {
                let strict = pass
                    .with_semantic_query(|query| query.is_assignable(check.own_ty, check.base_ty));
                let (compatible, exhaustion) = match strict {
                    RelationOutcome::Yes => (true, None),
                    RelationOutcome::No(_) => match pass.with_semantic_query(|query| {
                        query.overload_implementation_compatible(check.own_ty, check.base_ty)
                    }) {
                        RelationOutcome::Yes => (true, None),
                        RelationOutcome::No(_) => (false, None),
                        RelationOutcome::Exhausted(exhaustion) => (false, Some(exhaustion)),
                    },
                    RelationOutcome::Exhausted(exhaustion) => (false, Some(exhaustion)),
                };
                if matches!(exhaustion, Some(Exhaustion::ClassProjectionBudget)) {
                    pass.append_effect_incomplete(
                        &mut effects,
                        IncompleteSurface::new(
                            "relation/class-projection-budget",
                            check.span,
                            "class projection budget exhausted",
                        ),
                    );
                }
                if !compatible {
                    effects
                        .records
                        .diagnostic(Diagnostic::property_override_incompatible(
                            check.span,
                            &check.name,
                            &check.derived,
                            &check.base,
                        ));
                }
            } else {
                let outcome = pass
                    .with_semantic_query(|query| query.is_assignable(check.own_ty, check.base_ty));
                match outcome {
                    RelationOutcome::Yes => {}
                    RelationOutcome::No(chain) => {
                        let diagnostic = Diagnostic::property_override_incompatible(
                            check.span,
                            &check.name,
                            &check.derived,
                            &check.base,
                        )
                        .with_elaboration(render_reason_chain(pass.interner.store(), chain.head()));
                        pass.append_effect_diagnostic(&mut effects, diagnostic);
                    }
                    RelationOutcome::Exhausted(exhaustion) => {
                        if exhaustion == Exhaustion::ClassProjectionBudget {
                            pass.append_effect_incomplete(
                                &mut effects,
                                IncompleteSurface::new(
                                    "relation/class-projection-budget",
                                    check.span,
                                    "class projection budget exhausted",
                                ),
                            );
                        }
                        pass.append_effect_diagnostic(
                            &mut effects,
                            Diagnostic::property_override_incompatible(
                                check.span,
                                &check.name,
                                &check.derived,
                                &check.base,
                            ),
                        );
                    }
                }
            }
        }
        completed.push(effects.records);
    }

    completed
}

impl UserReportingAdapter {
    fn finish<F>(
        mut self,
        batches: Vec<CheckerRecordBatch<UserRecordTicket>>,
        inspect: F,
    ) -> BTreeMap<ModuleOrdinal, (Vec<Diagnostic>, Vec<IncompleteSurface>)>
    where
        F: FnOnce(&[(events::EventKey, CheckerRecord)]),
    {
        for batch in batches {
            let (owner, records) = batch.into_parts();
            self.event_store
                .complete(owner, records)
                .expect("each lexical record owner completes exactly once");
        }

        let mut by_module: BTreeMap<ModuleOrdinal, (Vec<Diagnostic>, Vec<IncompleteSurface>)> =
            BTreeMap::new();
        let records = self
            .event_store
            .finish()
            .expect("all lexically preallocated record owners must be completed");
        inspect(&records);
        for (key, record) in records {
            let channels = by_module.entry(key.module_ordinal).or_default();
            match record {
                CheckerRecord::Diagnostic(diagnostic) => channels.0.push(diagnostic),
                CheckerRecord::Incomplete(incomplete) => channels.1.push(incomplete),
            }
        }
        by_module
    }
}

impl<Ticket: Copy + PartialEq> Pass<'_, '_, Ticket> {
    pub(in crate::check::checker) fn current_replay_owner(&self) -> Option<ReplayOwner> {
        self.replay_trace
            .as_ref()
            .and_then(replay_index::ReplayDependencyTrace::current_owner)
    }

    pub(in crate::check::checker) fn with_replay_owner<R>(
        &mut self,
        owner: ReplayOwner,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let Some(trace) = self.replay_trace.clone() else {
            return produce(self);
        };
        trace.enter(owner);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| produce(self)));
        trace.leave(owner);
        match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    pub(in crate::check::checker) fn record_replay_demand(&self, owner: ReplayOwner) {
        if let Some(trace) = &self.replay_trace {
            trace.demand(owner);
        }
    }

    fn record_replay_demand_at(&self, owner: ReplayOwner, boundary: &'static str) {
        if let Some(trace) = &self.replay_trace {
            trace.demand_at(owner, boundary);
        }
    }

    fn record_replay_record_dependency(&self, ticket: Ticket) {
        let Some(trace) = &self.replay_trace else {
            return;
        };
        let Some(producer) = trace.current_owner() else {
            return;
        };
        trace.record_statement_dependency((self.pending_effect_key)(ticket), producer);
    }

    fn append_effect_diagnostic(
        &self,
        effects: &mut CheckerEffects<Ticket>,
        diagnostic: Diagnostic,
    ) {
        self.record_replay_record_dependency(effects.records.owner());
        effects.records.diagnostic(diagnostic);
    }

    fn append_effect_incomplete(
        &self,
        effects: &mut CheckerEffects<Ticket>,
        incomplete: IncompleteSurface,
    ) {
        self.record_replay_record_dependency(effects.records.owner());
        effects.records.incomplete(incomplete);
    }

    pub(in crate::check::checker) fn resolve_value_binding_replay(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> crate::binder::bind::ValueResolution {
        if self.replay_trace.is_none() {
            return self.binder.resolve_value_binding(scope, name);
        }
        let trace = self.replay_trace.clone();
        let _observation = trace
            .as_ref()
            .map(|trace| trace.observe_typed_demand("value-binding"));
        let resolution = self.binder.resolve_value_binding_traced(scope, name, || {
            if let Some(trace) = &trace {
                trace.demand_root_slot(name, replay_index::RootSlotKind::Value);
                if name == "globalThis" {
                    trace.demand(ReplayOwner::GlobalObject);
                }
            }
        });
        match resolution {
            crate::binder::bind::ValueResolution::Resolved { symbol, kind } => {
                if let Some(binding) = self.binder.symbols.get(symbol) {
                    if let Some(storage) = binding.value {
                        self.record_replay_demand_at(ReplayOwner::Value(storage), "value-binding");
                    }
                    if let Some(namespace) = binding.ns {
                        self.record_replay_demand_at(
                            ReplayOwner::Namespace(namespace),
                            "value-binding-namespace",
                        );
                    }
                }
                if let crate::binder::bind::ResolvedValueKind::StandaloneNamespace {
                    namespace,
                    storage,
                } = kind
                {
                    self.record_replay_demand_at(
                        ReplayOwner::Namespace(namespace),
                        "standalone-value-namespace",
                    );
                    self.record_replay_demand_at(
                        ReplayOwner::Value(storage),
                        "standalone-value-storage",
                    );
                }
            }
            crate::binder::bind::ValueResolution::TypeOnlyNamespace { namespace } => {
                self.record_replay_demand_at(
                    ReplayOwner::Namespace(namespace),
                    "type-only-namespace",
                );
            }
            crate::binder::bind::ValueResolution::Missing => {}
        }
        resolution
    }

    pub(in crate::check::checker) fn resolve_value_replay(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<SymbolId> {
        if self.replay_trace.is_none() {
            return self.binder.resolve_value(scope, name);
        }
        match self.resolve_value_binding_replay(scope, name) {
            crate::binder::bind::ValueResolution::Resolved { symbol, .. } => Some(symbol),
            crate::binder::bind::ValueResolution::TypeOnlyNamespace { .. }
            | crate::binder::bind::ValueResolution::Missing => None,
        }
    }

    pub(in crate::check::checker) fn resolve_type_replay(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<SymbolId> {
        if self.replay_trace.is_none() {
            return self.binder.resolve_type(scope, name);
        }
        let trace = self.replay_trace.clone();
        let _observation = trace
            .as_ref()
            .map(|trace| trace.observe_typed_demand("type-binding"));
        let symbol = self.binder.resolve_type_traced(scope, name, || {
            if let Some(trace) = &trace {
                trace.demand_root_slot(name, replay_index::RootSlotKind::Type);
            }
        });
        if let Some(group) = symbol
            .and_then(|symbol| self.binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty)
        {
            self.record_replay_demand_at(ReplayOwner::TypeGroup(group), "type-binding");
        }
        symbol
    }

    pub(in crate::check::checker) fn resolve_qualified_type_path_replay(
        &self,
        scope: ScopeId,
        segments: &[&str],
    ) -> crate::binder::namespace::QualifiedTypePathResolution {
        if self.replay_trace.is_none() {
            return self.binder.resolve_qualified_type_path(scope, segments);
        }
        let trace = self.replay_trace.clone();
        let _observation = trace
            .as_ref()
            .map(|trace| trace.observe_typed_demand("qualified-type-binding"));
        let resolution = self.binder.resolve_qualified_type_path_traced(
            scope,
            segments,
            || {
                if let (Some(trace), Some(root)) = (&trace, segments.first()) {
                    trace.demand_root_slot(root, replay_index::RootSlotKind::Namespace);
                }
            },
            |namespace| {
                if let Some(trace) = &trace {
                    trace.demand_at(
                        ReplayOwner::Namespace(namespace),
                        "qualified-type-namespace",
                    );
                }
            },
        );
        if let crate::binder::namespace::QualifiedTypePathResolution::TypeGroup(group) = resolution
        {
            self.record_replay_demand_at(ReplayOwner::TypeGroup(group), "qualified-type-terminal");
        }
        resolution
    }

    pub(in crate::check::checker) fn type_decl_id_replay(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<crate::binder::declaration::TypeGroupId> {
        self.resolve_type_replay(scope, name)
            .and_then(|symbol| self.binder.symbols.get(symbol))
            .and_then(|binding| binding.ty)
    }

    pub(in crate::check::checker) fn value_decl_id_replay(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<ValueStorageId> {
        self.resolve_value_replay(scope, name)
            .and_then(|symbol| self.binder.symbols.get(symbol))
            .and_then(|binding| binding.value)
    }

    pub(in crate::check::checker) fn published_class_replay(
        &self,
        class: crate::types::repr::ClassId,
    ) -> crate::class_semantics::DemandOutcome<&crate::class_semantics::PublishedClassSurface> {
        if let Some(trace) = &self.replay_trace {
            let _observation = trace.observe_typed_demand("class-terminal");
            trace.demand_at(ReplayOwner::Class(class), "class-terminal");
        }
        self.type_environment
            .published()
            .classes()
            .published_class(class)
    }

    pub(in crate::check::checker) fn decl_type_replay(
        &self,
        storage: ValueStorageId,
    ) -> Option<TypeId> {
        if self.replay_trace.is_none() {
            return self.decl_types.get(storage);
        }
        let _observation = self
            .replay_trace
            .as_ref()
            .map(|trace| trace.observe_typed_demand("decl-type"));
        self.record_replay_demand_at(ReplayOwner::Value(storage), "decl-type");
        self.decl_types.get(storage)
    }

    pub(in crate::check::checker) fn publish_copied_decl_type_replay(
        &mut self,
        storage: ValueStorageId,
        ty: TypeId,
    ) {
        let owner = ReplayOwner::Value(storage);
        self.with_replay_publication_owner(owner, |pass| pass.decl_types.set(storage, ty));
    }

    pub(in crate::check::checker) fn with_replay_publication_owner<R>(
        &mut self,
        owner: ReplayOwner,
        publish: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let producer = self.current_replay_owner();
        if producer == Some(owner) {
            return publish(self);
        }
        if producer.is_some() {
            self.record_replay_demand(owner);
        }
        self.with_replay_owner(owner, |pass| {
            if let Some(producer) = producer {
                pass.record_replay_demand(producer);
            }
            publish(pass)
        })
    }

    pub(in crate::check::checker) fn demand_function_group_replay(
        &mut self,
        symbol: SymbolId,
    ) -> function_groups::FunctionGroupDemand {
        if self.replay_trace.is_none() {
            return self.function_groups.demand(symbol);
        }
        let _observation = self
            .replay_trace
            .as_ref()
            .map(|trace| trace.observe_typed_demand("function-group"));
        if let Some(storage) = self
            .binder
            .symbols
            .get(symbol)
            .and_then(|binding| binding.value)
        {
            self.record_replay_demand_at(ReplayOwner::Value(storage), "function-group");
        }
        self.function_groups.demand(symbol)
    }

    pub(in crate::check::checker) fn standalone_namespace_terminal_replay(
        &self,
        namespace: crate::binder::namespace::NamespaceId,
    ) -> Option<namespace_values::StandaloneNamespaceTerminal> {
        if self.replay_trace.is_none() {
            return self.namespace_values.standalone_terminal(namespace);
        }
        let _observation = self
            .replay_trace
            .as_ref()
            .map(|trace| trace.observe_typed_demand("namespace-terminal"));
        self.record_replay_demand_at(ReplayOwner::Namespace(namespace), "namespace-terminal");
        let terminal = self.namespace_values.standalone_terminal(namespace);
        if let Some(namespace_values::StandaloneNamespaceTerminal::Ready { storage, .. }) = terminal
        {
            self.record_replay_demand_at(ReplayOwner::Value(storage), "namespace-terminal-storage");
        }
        terminal
    }

    pub(in crate::check::checker) fn with_semantic_query<R>(
        &mut self,
        run: impl FnOnce(&mut SemanticQueryCoordinator<'_, replay_index::ReplayClassLookup<'_>>) -> R,
    ) -> R {
        let lookup = replay_index::ReplayClassLookup::new(
            self.type_environment.published().classes(),
            self.replay_trace.clone(),
        );
        let mut coordinator = SemanticQueryCoordinator::new(
            self.interner,
            &lookup,
            &mut self.semantic_queries,
            &mut self.next_type_param,
        );
        run(&mut coordinator)
    }

    /// Run one lexically owned producer and retain its effects until deferred work
    /// has resolved. Nested lexical sites keep distinct preallocated owners.
    pub(in crate::check::checker) fn with_lexical_effects<R>(
        &mut self,
        source_start: u32,
        phase: LexicalOwnerPhase,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let owner = self
            .lexical_events
            .owner_at(
                lexical_events::source_ordinal(self.current_source),
                source_start,
                phase,
            )
            .expect("lexical owner must be preallocated before semantic execution");
        self.with_ticket_effects(owner.ticket, produce)
    }
}

impl<Ticket: Copy + PartialEq> Pass<'_, '_, Ticket> {
    /// Run a producer that already owns an exact preallocated ticket.
    pub(in crate::check::checker) fn with_ticket_effects<R>(
        &mut self,
        owner: Ticket,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let statement_owner = self.replay_trace.as_ref().and_then(|trace| {
            trace
                .current_owner()
                .is_none()
                .then(|| trace.statement_owner((self.pending_effect_key)(owner)))
                .flatten()
        });
        match statement_owner {
            Some(statement_owner) => self.with_replay_owner(statement_owner, |pass| {
                pass.with_ticket_effects_inner(owner, produce)
            }),
            None => self.with_ticket_effects_inner(owner, produce),
        }
    }

    fn with_ticket_effects_inner<R>(
        &mut self,
        owner: Ticket,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.effect_stack.push(CheckerEffects::new(owner));
        let result = produce(self);
        let effects = self.effect_stack.pop().expect("lexical effect frame");
        if self.suppress_effects {
            return result;
        }
        if let Some(parent) = self.effect_stack.last_mut() {
            if parent.records.owner() == effects.records.owner() {
                parent.merge(effects);
            } else {
                parent.nested.push(effects);
            }
        } else {
            self.enqueue_effects(effects);
        }
        result
    }

    /// Coalesce repeated phases for one lexical owner before its exactly-once
    /// completion, preserving producer order within the owner group.
    pub(in crate::check::checker) fn enqueue_effects(
        &mut self,
        mut effects: CheckerEffects<Ticket>,
    ) {
        let nested = std::mem::take(&mut effects.nested);
        let owner = effects.records.owner();
        let next = self.pending_effects.len();
        let (index, inserted) = self
            .pending_effect_slots
            .get_or_insert((self.pending_effect_key)(owner), || next);
        if inserted {
            debug_assert_eq!(index, next);
            self.pending_effects.push(effects);
        } else {
            let existing = self
                .pending_effects
                .get_mut(index)
                .expect("pending-effect slot indexes one existing batch");
            debug_assert!(existing.records.owner() == owner);
            existing.merge(effects);
        }
        for child in nested {
            self.enqueue_effects(child);
        }
    }

    pub(in crate::check::checker) fn enqueue_ticket_record(
        &mut self,
        owner: Ticket,
        record: CheckerRecord,
    ) {
        self.record_replay_record_dependency(owner);
        let mut effects = CheckerEffects::new(owner);
        effects.records.record(record);
        self.enqueue_effects(effects);
    }

    /// Isolate a speculative child under the enclosing lexical ticket.
    pub(in crate::check::checker) fn capture_candidate_effects<R>(
        &mut self,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> (R, CheckerEffects<Ticket>) {
        let owner = self
            .effect_stack
            .last()
            .map(|effects| effects.records.owner())
            .expect("speculation requires an enclosing lexical owner");
        self.effect_stack.push(CheckerEffects::new(owner));
        let result = produce(self);
        let effects = self.effect_stack.pop().expect("candidate effect frame");
        (result, effects)
    }

    pub(in crate::check::checker) fn merge_candidate_effects(
        &mut self,
        selected: CheckerEffects<Ticket>,
    ) {
        self.effect_stack
            .last_mut()
            .expect("selected candidate requires an enclosing lexical owner")
            .merge(selected);
    }

    pub(in crate::check::checker) fn emit_diagnostic(&mut self, diagnostic: Diagnostic) {
        let owner = self
            .effect_stack
            .last()
            .expect("diagnostic requires a lexical owner")
            .records
            .owner();
        self.record_replay_record_dependency(owner);
        self.effect_stack
            .last_mut()
            .expect("diagnostic requires a lexical owner")
            .records
            .diagnostic(diagnostic);
    }

    pub(in crate::check::checker) fn emit_incomplete(&mut self, incomplete: IncompleteSurface) {
        let owner = self
            .effect_stack
            .last()
            .expect("incomplete record requires a lexical owner")
            .records
            .owner();
        self.record_replay_record_dependency(owner);
        self.effect_stack
            .last_mut()
            .expect("incomplete record requires a lexical owner")
            .records
            .incomplete(incomplete);
    }

    pub(in crate::check::checker) fn schedule_obligation(&mut self, obligation: AssignObligation) {
        let owner = self.current_replay_owner();
        self.effect_stack
            .last_mut()
            .expect("obligation requires a lexical owner")
            .push_obligation(DeferredRelationObligation::Assign(obligation), owner);
    }

    pub(in crate::check::checker) fn schedule_assertion_compatibility(
        &mut self,
        obligation: AssertionCompatibilityObligation,
    ) {
        let owner = self.current_replay_owner();
        self.effect_stack
            .last_mut()
            .expect("assertion compatibility requires a lexical owner")
            .push_obligation(
                DeferredRelationObligation::AssertionCompatibility(obligation),
                owner,
            );
    }

    pub(in crate::check::checker) fn schedule_override(&mut self, check: OverrideCheck) {
        let owner = self.current_replay_owner();
        self.effect_stack
            .last_mut()
            .expect("override check requires a lexical owner")
            .push_override(check, owner);
    }

    pub(in crate::check::checker) fn schedule_interface_relation(
        &mut self,
        check: InterfaceRelationObligation,
    ) {
        let owner = self.current_replay_owner();
        self.effect_stack
            .last_mut()
            .expect("interface relation requires a lexical owner")
            .push_interface_relation(check, owner);
    }

    /// Record an in-scope AST position the walk skipped (WU2, sprint 2026-07-10). `id`
    /// is the stable `role/surface/slot-or-variant` identity; `span` is the skipped
    /// position. This is the single entry point the checker calls when WU3–5 wire real
    /// emissions — it is deliberately not a diagnostic and carries no `TK` code.
    pub(in crate::check::checker) fn record_incomplete(
        &mut self,
        id: &str,
        span: Span,
        context: &str,
    ) {
        self.emit_incomplete(IncompleteSurface::new(id, span, context));
    }
}

/// Construct a fresh phase-1 [`Pass`]. Fill states come from the decl kind:
/// classes/interfaces/template aliases start `Pending`; resolved placeholders and
/// other declarations start `Done`.
#[cfg(test)]
fn build_pass<'a, 'ast>(
    interner: &'a mut Interner,
    binder: &'a Binder,
    type_decls: Vec<TypeDecl<'ast>>,
    type_resolved: Vec<Option<TypeId>>,
    decl_types: DeclTypes,
    next_type_param: u32,
) -> Pass<'a, 'ast> {
    build_pass_with_reporting(
        interner,
        binder,
        type_decls.into(),
        type_resolved.into(),
        decl_types,
        next_type_param,
        PassReporting {
            source: SourceUnit::User {
                module_ordinal: ModuleOrdinal::new(0),
                unit_slot: UnitSlot::new(0),
            },
            lexical_events: LexicalReservations::default(),
            suppress_effects: false,
        },
    )
}

fn build_pass_with_reporting<'a, 'ast>(
    interner: &'a mut Interner,
    binder: &'a Binder,
    type_decls: TypeDeclTable<'ast>,
    type_resolved: TypeResolvedTable,
    decl_types: DeclTypes,
    next_type_param: u32,
    reporting: PassReporting<UserRecordTicket>,
) -> Pass<'a, 'ast, UserRecordTicket> {
    let pending_tickets = reporting.lexical_events.tickets();
    build_pass_with_tickets(
        interner,
        binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReportingPlan {
            reporting,
            pending_tickets,
            ticket_key: user_record_ticket_key,
        },
    )
}

fn build_pass_with_tickets<'a, 'ast, Ticket: Copy + PartialEq>(
    interner: &'a mut Interner,
    binder: &'a Binder,
    type_decls: TypeDeclTable<'ast>,
    type_resolved: TypeResolvedTable,
    decl_types: DeclTypes,
    next_type_param: u32,
    reporting_plan: PassReportingPlan<Ticket>,
) -> Pass<'a, 'ast, Ticket> {
    let PassReportingPlan {
        reporting,
        pending_tickets,
        ticket_key,
    } = reporting_plan;
    let mut pending_effects = Vec::with_capacity(pending_tickets.len());
    let mut pending_effect_slots = context::PendingEffectSlots::new();
    for ticket in pending_tickets {
        let index = pending_effects.len();
        let (existing, inserted) = pending_effect_slots.get_or_insert(ticket_key(ticket), || index);
        assert!(inserted, "pending lexical tickets must be unique");
        assert_eq!(existing, index);
        pending_effects.push(CheckerEffects::new(ticket));
    }
    let published_type_count = type_decls.published_len();
    let template_fill = type_decls
        .iter()
        .map(|decl| match decl {
            TypeDecl::Interface { .. } => ClassFillState::Pending,
            TypeDecl::Alias {
                object_template: Some(_),
                ..
            } => ClassFillState::Pending,
            _ => ClassFillState::Done,
        })
        .collect();
    let template_fill = TemplateFillTable::new(published_type_count, template_fill);
    #[cfg(test)]
    let cycle_tainted_application_cache_capture =
        context::capture_cycle_tainted_application_cache_measure();
    let mut pass = Pass {
        interner,
        binder,
        eager_application_cache: FxHashMap::default(),
        #[cfg(test)]
        eager_application_cache_measure: context::capture_eager_application_cache_measure(),
        #[cfg(test)]
        cycle_tainted_application_cache: cycle_tainted_application_cache_capture
            .as_ref()
            .and_then(|capture| capture.cache_enabled.then(FxHashMap::default)),
        #[cfg(test)]
        cycle_tainted_application_cache_measure: cycle_tainted_application_cache_capture
            .map(|capture| capture.collector),
        #[cfg(test)]
        panic_before_cycle_tainted_application_cache_publish: false,
        // Overwritten before each module's fill/flow/check phase; the user module is
        // the single-file default (backlog 58).
        current_module: binder.module,
        current_source: reporting.source,
        replay_trace: None,
        effect_stack: Vec::new(),
        pending_effects,
        pending_effect_slots,
        pending_effect_key: ticket_key,
        lexical_events: reporting.lexical_events,
        suppress_effects: reporting.suppress_effects,
        type_environment: type_groups::TypeEnvironmentState::constructing(ConstructionDrafts {
            staged_published_classes: None,
            type_group_construction: Some(type_groups::TypeGroupConstruction::new(
                type_decls.len(),
            )),
            type_decls,
            type_resolved,
            template_fill,
        }),
        semantic_queries: SemanticQueryState::default(),
        library_semantic_identities: None,
        lexical_array_alias: None,
        class_application_parameters: LayeredMap::default(),
        staged_class_validation: None,
        retained_class_callables: BTreeMap::new(),
        class_body_views: BTreeMap::new(),
        class_super_constructors: BTreeMap::new(),
        class_new_metadata: LayeredMap::default(),
        type_param_scopes: Vec::new(),
        static_class_type_param_barriers: Vec::new(),
        next_type_param,
        class_parents: LayeredMap::default(),
        class_value_aliases: LayeredMap::default(),
        class_value_bindings: LayeredMap::default(),
        standalone_namespace_value_aliases: LayeredMap::default(),
        class_names: LayeredMap::default(),
        decl_types,
        function_groups: function_groups::FunctionGroupRegistry::default(),
        named_function_symbols: LayeredSet::default(),
        class_namespace_payloads: BTreeMap::new(),
        namespace_values: namespace_values::NamespaceValueRegistry::default(),
        var_annotation_surfaces: FxHashMap::default(),
        var_value_type_states: FxHashMap::default(),
        // M23 flow-graph state. Slots 0/1 are the UNREACHABLE/START sentinels the
        // whole arena reserves (see `FlowNodeId::{UNREACHABLE,START}`).
        flow_nodes: vec![
            crate::check::flow::FlowNode::Unreachable,
            crate::check::flow::FlowNode::Start,
        ],
        flow_cursor: crate::check::flow::FlowNodeId::START,
        flow_loops: Vec::new(),
        break_targets: Vec::new(),
        label_targets: Vec::new(),
        reference_flow: FxHashMap::default(),
        flow_memo: FxHashMap::default(),
        flow_provisional: FxHashMap::default(),
        flow_loop_depth: 0,
        cond_frames: Vec::new(),
        building_template: false,
        resolving_conditional_alias: None,
        resolving_alias: None,
        resolving_alias_stack: Vec::new(),
        circular_aliases: FxHashSet::default(),
        alias_indirection_depth: 0,
        annotation_depth: 0,
        mapped_frames: Vec::new(),
        current_this: None,
        current_body_this_environment: None,
        current_class: None,
        enclosing_classes: Vec::new(),
        current_super_ctor: None,
        current_in_ctor: false,
    };
    let terminal_groups: Vec<TypeGroupId> = pass
        .type_decls
        .iter()
        .enumerate()
        .map(|(index, declaration)| (pass.type_decls.published_len() + index, declaration))
        .filter(|(_, declaration)| matches!(declaration, TypeDecl::Unavailable { .. }))
        .map(|(index, _)| TypeGroupId(u32::try_from(index).expect("type group index fits u32")))
        .collect();
    for group in terminal_groups {
        pass.begin_type_group_construction(group);
        pass.freeze_type_group(group);
    }
    pass
}

// ===========================================================================
// M5: named types — reserve-then-fill (mvp-plan M5, §3, §6.3).
// ===========================================================================

// ===========================================================================
// M9: type-parameter scoping — a name → TypeParam frame stack on `Pass`.
// ===========================================================================

#[cfg(test)]
mod tests;

#[cfg(test)]
mod layered_runtime_tests {
    use super::FrozenCheckerRuntimeMetadata;
    use crate::types::repr::ClassId;

    #[test]
    fn checker_runtime_metadata_forks_share_every_frozen_table() {
        let mut runtime = FrozenCheckerRuntimeMetadata::default();
        runtime.freeze_as_base().expect("runtime metadata seals");
        let mut first = runtime.fork_delta().expect("first runtime suffix");
        let second = runtime.fork_delta().expect("second runtime suffix");
        assert!(first.shares_base_with(&second));
        first
            .class_names
            .insert_local(ClassId(7), "Local".to_owned())
            .expect("local class metadata inserts");
        assert_eq!(
            first.class_names.get(&ClassId(7)).map(String::as_str),
            Some("Local")
        );
        assert!(second.class_names.get(&ClassId(7)).is_none());
    }
}

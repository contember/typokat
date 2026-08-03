//! Statement-level checker.
//! This module wires parsing/binding output into the checker pass, flow pre-pass,
//! and relation phase. Keep durable design notes in `docs/reference/architecture.md`
//! (§5.2) and soundness rules in `docs/reference/invariants.md`.

use crate::binder::bind::{ImportPlaceholder, ImportedSymbol, ProjectBinderBuilder};
#[cfg(any(test, feature = "test-utils"))]
use crate::binder::bind_module_with_prelude;
use crate::binder::declaration::source_global_binding_census;
use crate::binder::declaration::{DeclId, DeclarationKind, TypeGroupId, ValueStorageId};
use crate::binder::namespace::SourceUnitKey;
use crate::binder::namespace::{
    CompilationUnit, GlobalIssue, LocalAmbientExportAliasFailureKind, ModuleBindingContext,
    PlacementIssueKind, SourceFileKind, UmdContext,
};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::check::query::SemanticQueryCoordinator;
use crate::check::query::SemanticQueryState;
use crate::class_semantics::{DemandOutcome, Exhaustion};
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic, IncompleteSurface};
use crate::frontend::{ProjectImport, ProjectImportSource, ProjectProgram};
use crate::relate::RelationOutcome;
use crate::source::{
    CompilationOrigin, LibraryFileOrdinal, ModuleOrdinal, OriginalModuleOrdinal, SourceOrdinal,
    SourceUnit, UnitSlot,
};
use crate::span::Span;
use crate::types::layered::{LayeredMap, LayeredSet};
use crate::types::repr::{ClassId, ObjectType, PropertyType};
use crate::types::store::TypeId;
use crate::types::Interner;
#[cfg(any(test, feature = "test-utils"))]
use oxc_allocator::Allocator;
#[cfg(any(test, feature = "test-utils"))]
use oxc_ast::ast::TSType;
use oxc_ast::ast::{
    Declaration, ExportSpecifier, ImportOrExportKind, ModuleExportName, Program, Statement,
};
#[cfg(any(test, feature = "test-utils"))]
use oxc_parser::Parser;
use oxc_span::GetSpan;
#[cfg(any(test, feature = "test-utils"))]
use oxc_span::SourceType;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};

mod annotations;
mod assignment;
mod calls;
mod classes;
mod context;
#[cfg(test)]
mod declaration_owner_scaling_spec;
#[cfg(test)]
mod declaration_surface_lazy_spec;
#[cfg(test)]
pub(crate) mod declaration_surface_measure;
mod decls;
pub(in crate::check) mod eval;
pub mod events;
pub mod events_library;
#[cfg(test)]
mod exact_declaration_site_cutover_spec;
mod expr;
mod flowgraph;
mod function_groups;
mod indexed_access;
pub mod lexical_events;
pub mod lexical_events_library;
mod lexical_events_user;
pub mod library_compiler;
mod library_identities;
pub(crate) mod library_reporting;
mod namespace_values;
mod narrowing;
pub mod replay_index;
pub mod reporting_record;
mod statements;
#[cfg(test)]
mod surface_lowering_copy_spec;
mod type_groups;

use context::{
    AssertionCompatibilityObligation, AssignObligation, CheckerEffects, CheckerRecordBatch,
    ClassFillState, ConstructionDrafts, DeclTypes, DeferredRelationObligation,
    InterfaceRelationKind, InterfaceRelationObligation, InterfaceRelationReport, OverrideCheck,
    Pass, ProvisionalArgumentWalk, TemplateFillTable, TypeDecl, TypeDeclTable, TypeResolvedTable,
};
#[cfg(any(test, feature = "test-utils"))]
use decls::type_decl_id;
use decls::{
    reserve_type_decls, reserve_type_decls_for_combined_library, reserve_type_decls_selected,
    walk_type_decls, TopTypeDecl,
};
use events::{user_record_ticket_key, CandidateEffects, EventStore, UserRecordTicket};
use events_library::{library_record_ticket_key, LibraryEventLedger, LibraryRecordTicket};
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

fn direct_global_this_modeled_value_storage(
    binder: &Binder,
    module: ScopeId,
    program: &Program<'_>,
) -> Option<ValueStorageId> {
    program.body.iter().rev().find_map(|statement| {
        let declaration = statement.as_declaration().or_else(|| {
            let Statement::ExportNamedDeclaration(export) = statement else {
                return None;
            };
            export.declaration.as_ref()
        })?;
        let (binding_start, kind) = match declaration {
            Declaration::FunctionDeclaration(function) => {
                let identifier = function.id.as_ref()?;
                if identifier.name != "globalThis" {
                    return None;
                }
                (identifier.span.start, DeclarationKind::Function)
            }
            Declaration::ClassDeclaration(class) => {
                let identifier = class.id.as_ref()?;
                if identifier.name != "globalThis" {
                    return None;
                }
                (identifier.span.start, DeclarationKind::Class)
            }
            _ => return None,
        };
        binder
            .direct_script_global_this_conflict(module, binding_start, kind)
            .then(|| {
                binder
                    .exact_declaration_at(module, binding_start, kind)
                    .and_then(|declaration| declaration.value_storage)
            })
            .flatten()
    })
}

fn compose_global_this_value_surface(mut base: ObjectType, overlay: ObjectType) -> ObjectType {
    for property in overlay.properties {
        if !base
            .properties
            .iter()
            .any(|existing| existing.name == property.name)
        {
            base.properties.push(property);
        }
    }
    base.string_index = overlay.string_index.or(base.string_index);
    base.number_index = overlay.number_index.or(base.number_index);
    base.call_signatures.extend(overlay.call_signatures);
    base.construct_signatures
        .extend(overlay.construct_signatures);
    base
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PrivateCombinedRecordTicket {
    Library(LibraryRecordTicket),
    DisabledLibrary(LibraryRecordTicket),
    User(UserRecordTicket),
}

fn private_combined_record_ticket_key(ticket: PrivateCombinedRecordTicket) -> (usize, usize) {
    let (event, record) = match ticket {
        PrivateCombinedRecordTicket::Library(ticket) => {
            let (event, record) = library_record_ticket_key(ticket);
            (event.saturating_mul(2), record)
        }
        PrivateCombinedRecordTicket::DisabledLibrary(ticket) => {
            let (event, record) = library_record_ticket_key(ticket);
            (event.saturating_mul(2), record)
        }
        PrivateCombinedRecordTicket::User(ticket) => {
            let (event, record) = user_record_ticket_key(ticket);
            (event.saturating_mul(2).saturating_add(1), record)
        }
    };
    (event, record)
}

trait UserReportingOwner: Copy + PartialEq {
    type Error;

    fn user_ticket(self) -> Result<UserRecordTicket, Self::Error>;
}

impl UserReportingOwner for UserRecordTicket {
    type Error = std::convert::Infallible;

    fn user_ticket(self) -> Result<UserRecordTicket, Self::Error> {
        Ok(self)
    }
}

impl UserReportingOwner for PrivateCombinedRecordTicket {
    type Error = &'static str;

    fn user_ticket(self) -> Result<UserRecordTicket, Self::Error> {
        match self {
            Self::User(ticket) => Ok(ticket),
            Self::Library(_) | Self::DisabledLibrary(_) => {
                Err("user source resolved to a library reporting ticket")
            }
        }
    }
}

fn infallible<T>(result: Result<T, std::convert::Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
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

#[cfg(any(test, feature = "test-utils"))]
fn reserve_internal_reporting(
    program: &Program<'_>,
    module_ordinal: ModuleOrdinal,
    unit_slot: UnitSlot,
    context: Option<ModuleBindingContext>,
) -> (PassReporting<UserRecordTicket>, UserReportingAdapter) {
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    let reservation = match context {
        Some(context) => lexical_events.reserve_continuation_program(
            module_ordinal,
            unit_slot,
            program,
            context,
            &mut event_store,
        ),
        None => {
            lexical_events.reserve_program(module_ordinal, unit_slot, program, &mut event_store)
        }
    };
    reservation.expect("lexical event reservation must reference valid events");
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

fn reserve_continuation_reporting(
    program: &Program<'_>,
    module_ordinal: ModuleOrdinal,
    unit_slot: UnitSlot,
    context: crate::binder::namespace::ModuleBindingContext,
) -> Result<
    (PassReporting<UserRecordTicket>, UserReportingAdapter),
    lexical_events_user::ReservationError,
> {
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    lexical_events.reserve_continuation_program(
        module_ordinal,
        unit_slot,
        program,
        context,
        &mut event_store,
    )?;
    Ok((
        PassReporting {
            source: SourceUnit::User {
                module_ordinal,
                unit_slot,
            },
            lexical_events,
            suppress_effects: false,
        },
        UserReportingAdapter { event_store },
    ))
}

/// Test-only utility aliases and bounded ambient values for raw checker unit support.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) const TEST_AMBIENT_SOURCE: &str = include_str!("../test_support_prelude.ts");

#[cfg(any(test, feature = "test-utils"))]
const TRUSTED_PRELUDE_INTRINSICS: [&str; 6] = [
    "OmitThisParameter",
    "Uppercase",
    "Lowercase",
    "Capitalize",
    "Uncapitalize",
    "ThisType",
];

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
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
#[cfg(any(test, feature = "test-utils"))]
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
    global_object_type: Option<TypeId>,
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
    pub(in crate::check::checker) global_object_type: Option<TypeId>,
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
            global_object_type: self.global_object_type,
        })
    }

    pub(in crate::check::checker) fn fork_sparse_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            class_application_parameters: self.class_application_parameters.fork_sparse_delta()?,
            class_new_metadata: self.class_new_metadata.fork_sparse_delta()?,
            class_parents: self.class_parents.fork_sparse_delta()?,
            class_value_aliases: self.class_value_aliases.fork_sparse_delta()?,
            class_value_bindings: self.class_value_bindings.fork_sparse_delta()?,
            standalone_namespace_value_aliases: self
                .standalone_namespace_value_aliases
                .fork_sparse_delta()?,
            class_names: self.class_names.fork_sparse_delta()?,
            namespace_terminals: self.namespace_terminals.fork_sparse_delta()?,
            named_function_symbols: self.named_function_symbols.fork_delta()?,
            global_object_type: self.global_object_type,
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
            && self.global_object_type == other.global_object_type
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
            global_object_type: self.global_object_type,
        })
    }

    #[cfg(test)]
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
            global_object_type: parts.global_object_type,
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
    private_collision_epoch: Option<library_compiler::PrivateCollisionEpoch>,
    library_modules: std::sync::Arc<[ScopeId]>,
}

/// Parse, bind, and check the trusted prelude in the caller's run-local type universe.
#[cfg(any(test, feature = "test-utils"))]
fn bootstrap_test_support_prelude(
    interner: &mut Interner,
    bind: impl FnOnce(&Program<'_>) -> Binder,
) -> (Binder, TrustedPreludeHandoff) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, TEST_AMBIENT_SOURCE, SourceType::ts()).parse();
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
        reserve_internal_reporting(&parsed.program, prelude_ordinal, prelude_slot, None);
    let declaration_spans = ModuleDeclarationSpans::index(&binder);
    attach_type_decl_owners(
        &mut reporting.lexical_events,
        SourceOrdinal::User(prelude_ordinal),
        &binder,
        binder.prelude_module,
        &parsed.program,
        &declaration_spans,
    );
    attach_class_bindings(
        &mut reporting.lexical_events,
        SourceOrdinal::User(prelude_ordinal),
        &binder,
        binder.prelude_module,
        &parsed.program,
        &type_decls,
        None,
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
    let publication = pass.publish_type_groups();
    if publication.library_identity_selection_pending() {
        pass.suppress_effects = true;
    }
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
    pub module_ordinal: ModuleOrdinal,
    pub unit_slot: UnitSlot,
    pub diagnostics: Vec<Diagnostic>,
    pub incomplete: Vec<IncompleteSurface>,
}

/// Test-only emission hook (WU2 plumbing): if `TYPOKAT_TEST_EMIT_INCOMPLETE` is set,
/// record one incomplete surface per comma-separated id in its value (at span 0..0),
/// exercising the real `record_incomplete` API end to end. No real checker path emits
/// yet, so with the env var unset every run is unaffected. A blank/`1` value emits one
/// default id.
fn emit_test_incomplete<Ticket: Copy + PartialEq>(pass: &mut Pass<'_, '_, Ticket>) {
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
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn check_program<'ast>(
    interner: &mut Interner,
    program: &'ast Program<'ast>,
) -> CheckResult {
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

#[cfg(any(test, feature = "test-utils"))]
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
    ) = bootstrap_test_support_prelude(interner, |prelude| {
        bind_module_with_prelude(prelude, program)
    });

    let reporting = reserve_internal_reporting(
        program,
        ModuleOrdinal::new(0),
        UnitSlot::new(0),
        Some(ModuleBindingContext::for_program(
            program,
            SourceFileKind::ImplementationTs,
        )),
    );
    check_bound_user_program(
        interner,
        binder,
        program,
        reporting,
        BoundUserBase {
            published_types,
            library_semantic_identities,
            lexical_array_alias,
            decl_types,
            next_type_param,
            next_class_id,
            runtime: FrozenCheckerRuntimeMetadata::default(),
            private_collision_epoch: None,
            library_modules: std::sync::Arc::from([]),
        },
        inspect,
    )
}

const LIBRARY_IDENTITY_SELECTION_PENDING: &str =
    "library/publication/semantic-identity-selection-pending";

fn fail_closed_identity_selection(
    publication: type_groups::TypeGroupPublicationOutcome,
    diagnostics: &mut Vec<Diagnostic>,
    incomplete: &mut Vec<IncompleteSurface>,
) {
    if !publication.library_identity_selection_pending() {
        return;
    }
    diagnostics.clear();
    incomplete.clear();
    incomplete.push(IncompleteSurface::new(
        LIBRARY_IDENTITY_SELECTION_PENDING,
        Span::new(0, 0),
        "library semantic identities are incomplete after collision publication",
    ));
}

pub(in crate::check::checker) fn check_bound_user_program<'ast, F>(
    interner: &mut Interner,
    binder: Binder,
    program: &'ast Program<'ast>,
    reporting: (PassReporting<UserRecordTicket>, UserReportingAdapter),
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
    check_bound_user_program_inner(
        interner,
        binder,
        program,
        reporting,
        base,
        inspect,
        |_, _| {},
    )
}

#[cfg(any(test, feature = "test-utils"))]
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
) -> Result<CheckResult, &'static str>
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
    let continuation_binding = crate::binder::namespace::ModuleBindingContext::for_program(
        program,
        crate::binder::namespace::SourceFileKind::ImplementationTs,
    );
    let reporting = reserve_continuation_reporting(
        program,
        ModuleOrdinal::new(0),
        UnitSlot::new(0),
        continuation_binding,
    )
    .map_err(|_| "user lexical reservation failed")?;
    Ok(check_bound_user_program_inner(
        interner,
        binder,
        program,
        reporting,
        base,
        inspect,
        inspect_final,
    ))
}

fn check_bound_user_program_inner<'ast, F, G>(
    interner: &mut Interner,
    binder: Binder,
    program: &'ast Program<'ast>,
    reporting: (PassReporting<UserRecordTicket>, UserReportingAdapter),
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
    #[cfg(any(test, feature = "test-utils"))]
    BOUND_USER_CHECK_CALLS.with(|calls| calls.set(calls.get() + 1));
    let module_ordinal = ModuleOrdinal::new(0);
    let unit_slot = UnitSlot::new(0);
    let (mut reporting, reporting_adapter) = reporting;
    let BoundUserBase {
        published_types,
        library_semantic_identities,
        lexical_array_alias,
        mut decl_types,
        mut next_type_param,
        mut next_class_id,
        runtime,
        private_collision_epoch,
        library_modules: _,
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
        &mut reporting.lexical_events,
        SourceOrdinal::User(module_ordinal),
        &binder,
        binder.module,
        program,
        &ModuleDeclarationSpans::index(&binder),
    );
    attach_class_bindings(
        &mut reporting.lexical_events,
        SourceOrdinal::User(module_ordinal),
        &binder,
        binder.module,
        program,
        &type_decls,
        None,
    );
    reporting
        .lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .expect("one callable binder reservation pass");
    let mut external_effects = BTreeMap::new();
    infallible(enqueue_local_ambient_export_alias_diagnostics(
        &binder,
        &reporting.lexical_events,
        &mut external_effects,
    ));
    infallible(enqueue_namespace_placement_diagnostics(
        &binder,
        &reporting.lexical_events,
        &mut external_effects,
    ));
    infallible(enqueue_ambient_context_diagnostics(
        &binder,
        &reporting.lexical_events,
        &mut external_effects,
    ));
    let mut pass = build_pass_with_reporting(
        interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        reporting,
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
    pass.global_object_type = runtime.global_object_type;
    pass.install_private_collision_epoch(private_collision_epoch);
    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_private_replay_trace_for_test(|trace, _| {
        trace.sparse_candidate_execution_started = true;
    });
    for effects in external_effects.into_values() {
        pass.enqueue_effects(CheckerEffects::from_records(effects));
    }

    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_private_replay_trace_for_test(|trace, _| {
        trace.completion_or_semantic_query_steps =
            trace.completion_or_semantic_query_steps.saturating_add(1);
    });

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
    let publication = pass.publish_type_groups();
    if publication.library_identity_selection_pending() {
        pass.suppress_effects = true;
    }
    pass.validate_published_class_surfaces();
    let mut function_surfaces = pass.reserve_function_surfaces(binder.module, &program.body);
    pass.reserve_var_annotation_surfaces(binder.module, &program.body);
    pass.reserve_continuation_global_augmentation_surfaces(&program.body, &mut function_surfaces);
    let contributor_names = source_global_binding_census(
        program,
        ModuleBindingContext::for_program(program, SourceFileKind::ImplementationTs),
    )
    .candidates
    .into_iter()
    .filter_map(|(name, candidate)| {
        (name != "globalThis" && candidate.global_object_contributor).then_some(name)
    })
    .collect::<FxHashSet<_>>();
    pass.refresh_user_global_object([(
        binder.module,
        program,
        ModuleBindingContext::for_program(program, SourceFileKind::ImplementationTs),
    )]);
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
    let mut no_return = None;
    pass.check_statement_list_with_global_contributors(
        binder.module,
        &program.body,
        None,
        &mut no_return,
        &mut function_surfaces,
        &contributor_names,
    );

    emit_test_incomplete(&mut pass);

    let mut records = finish_event_effects(&mut pass, reporting_adapter);
    let (mut diagnostics, mut incomplete) = records.remove(&module_ordinal).unwrap_or_default();
    fail_closed_identity_selection(publication, &mut diagnostics, &mut incomplete);
    inspect_final(&pass, next_class_id);

    CheckResult {
        module_ordinal,
        unit_slot,
        diagnostics,
        incomplete,
    }
}

#[cfg(any(test, feature = "test-utils"))]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSourceBindingRow {
    pub normalized_path: String,
    pub source_file_kind: crate::binder::namespace::SourceFileKind,
    pub external_module: bool,
    pub original_module_ordinal: OriginalModuleOrdinal,
    pub unit_slot: UnitSlot,
    pub source: SourceUnitKey,
    pub module: ScopeId,
}

pub struct BoundProjectBinder {
    pub binder: Binder,
    pub module_scopes: Vec<ScopeId>,
    pub module_placeholders: Vec<Vec<ImportPlaceholder>>,
    pub project_sources: Vec<ProjectSourceBindingRow>,
    #[cfg(any(test, feature = "test-utils"))]
    pub normalized: ProjectBindingProductForTest,
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

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectBindingProductForTest {
    pub normalized_per_path_binding_shape: Vec<String>,
    pub normalized_import_export_shape: Vec<String>,
    pub normalized_namespace_shape: Vec<String>,
}

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthoritativeProjectBindingWorkForTest {
    pub entries: u64,
    pub fresh_project_seed_entries: u64,
    pub authenticated_checkpoint_seed_entries: u64,
    pub bound_units: u64,
    pub typed_products_produced: u64,
    pub ordinary_check_products_consumed: u64,
    pub continuation_route_products_consumed: u64,
    pub fresh_project_products: Vec<ProjectBindingProductForTest>,
    pub authenticated_checkpoint_products: Vec<ProjectBindingProductForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct AuthoritativeProjectBindingWorkScopeForTest {
    start: AuthoritativeProjectBindingWorkForTest,
    fresh_len: usize,
    checkpoint_len: usize,
}

#[cfg(any(test, feature = "test-utils"))]
impl AuthoritativeProjectBindingWorkScopeForTest {
    pub fn start() -> Self {
        Self {
            start: project_binding_work_for_test(),
            fresh_len: PROJECT_BINDING_FRESH_PRODUCTS.with(|products| products.borrow().len()),
            checkpoint_len: PROJECT_BINDING_CHECKPOINT_PRODUCTS
                .with(|products| products.borrow().len()),
        }
    }

    pub fn finish(self) -> AuthoritativeProjectBindingWorkForTest {
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

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
pub fn project_binding_thread_receipt_for_test() -> AuthoritativeProjectBindingWorkForTest {
    let mut receipt = project_binding_work_for_test();
    receipt.fresh_project_products =
        PROJECT_BINDING_FRESH_PRODUCTS.with(|products| products.borrow().clone());
    receipt.authenticated_checkpoint_products =
        PROJECT_BINDING_CHECKPOINT_PRODUCTS.with(|products| products.borrow().clone());
    receipt
}

#[cfg(any(test, feature = "test-utils"))]
pub fn merge_project_binding_thread_receipt_for_test(
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

fn selected_library_statement_lists<'ast>(
    program: &'ast Program<'ast>,
    sites: &[replay_index::CollisionReplayOwnerSite],
) -> Vec<&'ast [Statement<'ast>]> {
    let mut lists = Vec::new();
    let mut start = None;
    for (index, statement) in program.body.iter().enumerate() {
        let span = statement.span();
        let selected = sites.iter().any(|site| {
            !matches!(site.owner, ReplayOwner::GlobalObject)
                && span.start <= site.span.start
                && site.span.end <= span.end
        });
        match (start, selected) {
            (None, true) => start = Some(index),
            (Some(first), false) => {
                lists.push(&program.body[first..index]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        lists.push(&program.body[first..]);
    }
    lists
}

/// Check a dependency-ordered project in one serial type universe. Returns one
/// [`CheckResult`] per unit, indexed like `units`.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn check_project_programs<'ast>(
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

pub(in crate::check::checker) fn check_project_programs_with_owned_library<'ast, F, G>(
    state: library_compiler::OwnedLibraryRuntimeState,
    units: &[ProjectProgram<'ast>],
    inspect_bindings: F,
    inspect_final: G,
) -> Result<Vec<CheckResult>, &'static str>
where
    F: FnOnce(&Binder, &[ScopeId]),
    G: FnOnce(&Pass<'_, 'ast, PrivateCombinedRecordTicket>, u32),
{
    check_project_programs_with_owned_library_inner(
        state,
        &[],
        units,
        None,
        inspect_bindings,
        inspect_final,
        |_| {},
    )
}

fn check_project_programs_with_owned_library_inner<'ast, F, G, H>(
    state: library_compiler::OwnedLibraryRuntimeState,
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[ProjectProgram<'ast>],
    complete_binder_checkpoint: Option<crate::binder::bind::LibraryBinderCheckpoint>,
    inspect_bindings: F,
    inspect_final: G,
    inspect_records: H,
) -> Result<Vec<CheckResult>, &'static str>
where
    F: FnOnce(&Binder, &[ScopeId]),
    G: FnOnce(&Pass<'_, 'ast, PrivateCombinedRecordTicket>, u32),
    H: FnOnce(&[(events::EventKey, CheckerRecord)]),
{
    if units.is_empty() {
        return Ok(Vec::new());
    }

    let (mut interner, binder, base) = state.into_user_project_base();
    let mut event_store = EventStore::default();
    let mut library_event_ledger = LibraryEventLedger::default();
    let mut lexical_events: LexicalReservations<PrivateCombinedRecordTicket> =
        LexicalReservations::default();
    for (slot, unit) in units.iter().enumerate() {
        debug_assert_eq!(unit.unit_slot.index(), slot);
        lexical_events
            .reserve_private_continuation_program(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                unit.compilation_unit.binding,
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
        mut private_collision_epoch,
        library_modules,
    } = base;
    let has_library_semantic_identities = library_semantic_identities.is_some();
    let mut expected_library_records = Vec::new();
    let disabled_library_record_owner = None;
    #[cfg(any(test, feature = "test-utils"))]
    let mut disabled_library_record_owner = disabled_library_record_owner;
    let mut external_effects: BTreeMap<UserRecordTicket, CandidateEffects> = BTreeMap::new();
    let (binder, module_scopes, module_placeholders) = if let Some(checkpoint) =
        complete_binder_checkpoint
    {
        let checkpoint_ends = checkpoint.checkpoint_ends();
        let (builder, checkpoint_units) = checkpoint.into_continuation();
        if checkpoint_units.len() != library_programs.len()
            || checkpoint_units
                .iter()
                .zip(library_programs)
                .any(|(unit, program)| {
                    unit.ordinal.index() != program.source_ordinal
                        || unit.module
                            != library_modules
                                .get(program.source_ordinal)
                                .copied()
                                .unwrap_or(ScopeId(u32::MAX))
                })
        {
            return Err("complete-source binder checkpoint does not match library programs");
        }
        let source_offset = u32::try_from(checkpoint_ends.next_source)
            .map_err(|_| "library source prefix exceeds u32")?
            .checked_sub(1)
            .ok_or("library source prefix omits the prelude")?;
        let mut binding_event_store = EventStore::default();
        let mut binding_lexical_events = LexicalReservations::default();
        for unit in units {
            binding_lexical_events
                .reserve_program(
                    unit.module_ordinal,
                    unit.unit_slot,
                    unit.program,
                    &mut binding_event_store,
                )
                .map_err(|_| "complete-source binding reservation failed")?;
        }
        let mut bound = bind_authoritative_project_core(
            builder,
            units,
            source_offset,
            &binding_lexical_events,
            &mut external_effects,
            AuthoritativeProjectBinderFinish::Continuation,
        )
        .map_err(|_| "complete-source project binding failed")?;
        bound.binder.prelude_type_group_count = u32::try_from(published_types.groups().len())
            .map_err(|_| "complete-source type-group prefix exceeds u32")?;
        (bound.binder, bound.module_scopes, bound.module_placeholders)
    } else {
        let mut module_scopes = Vec::with_capacity(units.len());
        let mut module_placeholders: Vec<Vec<ImportPlaceholder>> = Vec::with_capacity(units.len());
        let (mut builder, first_source) = ProjectBinderBuilder::resume_frozen_library(binder);
        let continuation_units = units
            .iter()
            .enumerate()
            .map(|(index, unit)| {
                let offset = u32::try_from(index).map_err(|_| "project unit count exceeds u32")?;
                let source = SourceUnitKey(
                    first_source
                        .0
                        .checked_add(offset)
                        .ok_or("project source key suffix exceeds u32")?,
                );
                Ok(CompilationUnit {
                    source,
                    origin: unit.compilation_unit.origin,
                    binding: unit.compilation_unit.binding,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        builder.reserve_script_namespace_roots(
            units
                .iter()
                .zip(&continuation_units)
                .map(|(unit, compilation)| (unit.program, *compilation)),
        );
        let mut exports: Vec<ExportSurface> = Vec::with_capacity(units.len());
        for (unit, compilation_unit) in units.iter().zip(continuation_units) {
            let imports = imported_symbols(unit, &exports, &lexical_events, &mut external_effects)?;
            let (scope, placeholders) =
                builder.add_module(unit.program, &imports, compilation_unit);
            let surface = collect_exports(
                &builder,
                scope,
                unit.program,
                unit.module_ordinal,
                &lexical_events,
                &mut external_effects,
            )?;
            module_scopes.push(scope);
            module_placeholders.push(placeholders);
            exports.push(surface);
        }
        let binder_module = module_scopes.last().copied();
        // The single-source path already propagates this; the project path must not turn it
        // into a panic (backlog 103).
        let binder = builder.finish_frozen_library_continuation(binder_module)?;
        (binder, module_scopes, module_placeholders)
    };
    binder
        .namespaces
        .validate_compilation_origin_index()
        .map_err(|_| "binder source-origin index conflict")?;
    #[cfg(any(test, feature = "test-utils"))]
    if library_compiler::inject_checker_full_base_scan_for_test() {
        let _ = binder.module_sources().iter().count();
    }
    #[cfg(any(test, feature = "test-utils"))]
    if library_compiler::inject_checker_full_plan_scan_for_test() {
        let full_plan_scan_units = private_collision_epoch
            .as_ref()
            .map(|epoch| epoch.plan.measured_full_scan_for_test())
            .unwrap_or_default();
        debug_assert!(full_plan_scan_units > 0);
    }
    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_user_source_binds_for_test(units.len());
    if let Some(epoch) = private_collision_epoch.as_mut() {
        let mutation_owners = binder
            .sparse_prefix_mutation_owners()
            .into_iter()
            .map(|owner| match owner {
                crate::binder::roots::FrozenLibraryMutationOwner::TypeGroup(group) => {
                    ReplayOwner::TypeGroup(group)
                }
                crate::binder::roots::FrozenLibraryMutationOwner::Value(value) => {
                    ReplayOwner::Value(value)
                }
                crate::binder::roots::FrozenLibraryMutationOwner::Namespace(namespace) => {
                    ReplayOwner::Namespace(namespace)
                }
            })
            .collect::<BTreeSet<_>>();
        #[cfg(any(test, feature = "test-utils"))]
        let mut mutation_owners = mutation_owners;
        #[cfg(any(test, feature = "test-utils"))]
        {
            if library_compiler::private_replay_production_trace_active_for_test() {
                library_compiler::record_private_replay_trace_for_test(|trace, _| {
                    trace.bind_completed = true;
                });
                if library_compiler::private_replay_fault_for_test()
                    == library_compiler::PrivateReplayProductionFaultForTest::
                        InjectPostBindMutationOwnerAbsentFromPlan
                {
                    let owner = ReplayOwner::TypeGroup(
                        crate::binder::declaration::TypeGroupId(u32::MAX),
                    );
                    mutation_owners.insert(owner);
                    library_compiler::record_private_replay_trace_for_test(|trace, _| {
                        trace.injected_after_bind = true;
                        trace.injected_owner = Some(owner);
                    });
                }
                let plan = epoch.plan.full_oracle_snapshot_for_test();
                let mut plan_owners = plan
                    .baseline_records
                    .iter()
                    .map(|record| record.owner)
                    .collect::<BTreeSet<_>>();
                for root in &plan.root_slots {
                    plan_owners.extend(root.value.map(ReplayOwner::Value));
                    plan_owners.extend(root.ty.map(ReplayOwner::TypeGroup));
                    plan_owners.extend(root.namespace.map(ReplayOwner::Namespace));
                }
                plan_owners.extend(plan.owner_sites.iter().map(|site| site.owner));
                let recorded = mutation_owners.clone();
                library_compiler::record_private_replay_trace_for_test(|trace, event| {
                    trace.mutation_ledger_recorded = event;
                    trace.post_bind_mutation_owners = recorded;
                    trace.plan_owners = plan_owners;
                });
                library_compiler::record_private_replay_trace_for_test(|trace, event| {
                    trace.containment_validation_started = event;
                });
            }
        }
        let loaded_sources = library_programs
            .iter()
            .map(|source| LibraryFileOrdinal::new(source.source_ordinal))
            .collect::<BTreeSet<_>>();
        let owner_ends = [
            binder.type_groups.len(),
            usize::try_from(binder.decl_count).unwrap_or(usize::MAX),
            binder.namespaces.len(),
        ];
        let reclosed = epoch.reclose_after_binding(mutation_owners, owner_ends, &loaded_sources);
        #[cfg(any(test, feature = "test-utils"))]
        library_compiler::record_private_replay_trace_for_test(|trace, event| {
            trace.plan_owner_intersection_started = event;
        });
        if let Err(failure) = reclosed {
            #[cfg(any(test, feature = "test-utils"))]
            library_compiler::record_private_replay_trace_for_test(|trace, _| {
                trace.failure = Some(match failure {
                    library_compiler::PrivateReplayScheduleFailure::MutationOwnerOutsidePlan(
                        owner,
                    ) => library_compiler::PrivateReplayProductionFailureForTest::
                        MutationOwnerOutsidePlan(owner),
                    library_compiler::PrivateReplayScheduleFailure::RequiredSourceNotLoaded(
                        source,
                    ) => library_compiler::PrivateReplayProductionFailureForTest::
                        RequiredSourceNotLoaded(source),
                });
            });
            return Err(match failure {
                library_compiler::PrivateReplayScheduleFailure::MutationOwnerOutsidePlan(_) => {
                    "private binder mutation is absent from the authenticated replay plan"
                }
                library_compiler::PrivateReplayScheduleFailure::RequiredSourceNotLoaded(_) => {
                    "private replay closure expanded beyond provisionally loaded sources"
                }
            });
        }
        expected_library_records = epoch.library_record_baselines.clone();
        #[cfg(any(test, feature = "test-utils"))]
        {
            if library_compiler::private_replay_production_trace_active_for_test() {
                if library_compiler::private_replay_fault_for_test()
                    == library_compiler::PrivateReplayProductionFaultForTest::
                        DisableSealedExpectedBaselineOwnerBeforeCandidateReservation
                {
                    disabled_library_record_owner = expected_library_records.iter().find_map(
                        |record| match record.owner {
                            ReplayOwner::Statement(owner) => Some(owner),
                            _ => None,
                        },
                    );
                    let disabled = disabled_library_record_owner.map(ReplayOwner::Statement);
                    library_compiler::record_private_replay_trace_for_test(|trace, event| {
                        trace.fault_injected = event;
                        trace.disabled_baseline_owner = disabled;
                    });
                }
                let baselines = expected_library_records.clone();
                library_compiler::record_private_replay_trace_for_test(|trace, _| {
                    trace.epoch_library_record_baselines = baselines;
                });
                if let library_compiler::PrivateReplayProductionFaultForTest::OmitScheduledOwner(
                    owner,
                ) = library_compiler::private_replay_fault_for_test()
                {
                    let removed = epoch.affected_owners.remove(&owner);
                    if removed {
                        epoch.owner_sites.retain(|site| site.owner != owner);
                        epoch
                            .library_record_baselines
                            .retain(|record| record.owner != owner);
                    }
                    library_compiler::record_private_replay_trace_for_test(|trace, _| {
                        trace.schedule_omission_installed = removed;
                        trace.omitted_scheduled_owner = Some(owner);
                    });
                }
                let scheduled = epoch.affected_owners.clone();
                library_compiler::record_private_replay_trace_for_test(|trace, _| {
                    trace.scheduled_owners = scheduled;
                });
            }
        }
    }
    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_private_replay_trace_for_test(|trace, event| {
        trace.candidate_reservation_started = event;
    });
    for library in library_programs {
        lexical_events
            .reserve_private_library_program(
                LibraryFileOrdinal::new(library.source_ordinal),
                library.program,
                &mut library_event_ledger,
                disabled_library_record_owner,
            )
            .map_err(|_| "private library event reservation referenced an invalid event")?;
    }
    #[cfg(any(test, feature = "test-utils"))]
    {
        if library_compiler::private_replay_production_trace_active_for_test() {
            let reserved = library_event_ledger
                .reserved_record_keys()
                .into_iter()
                .map(ReplayOwner::Statement)
                .collect();
            library_compiler::record_private_replay_trace_for_test(|trace, _| {
                trace.candidate_reserved_library_record_owners = reserved;
            });
        }
    }
    decl_types.resize(binder.decl_count);

    let selected_library_units = if let Some(epoch) = private_collision_epoch.as_ref() {
        library_programs
            .iter()
            .map(|library| {
                let module = library_modules
                    .get(library.source_ordinal)
                    .copied()
                    .ok_or("scheduled private library source has no retained module")?;
                let statements = if epoch.complete_source_replay {
                    vec![library.program.body.as_slice()]
                } else {
                    let sites = epoch
                        .owner_sites
                        .iter()
                        .filter(|site| site.file_ordinal.index() == library.source_ordinal)
                        .cloned()
                        .collect::<Vec<_>>();
                    let statements = selected_library_statement_lists(library.program, &sites);
                    if statements.is_empty()
                        && (!sites.is_empty()
                            && sites
                                .iter()
                                .all(|site| matches!(site.owner, ReplayOwner::GlobalObject)))
                    {
                        Vec::new()
                    } else {
                        statements
                    }
                };
                if statements.is_empty()
                    && (epoch.complete_source_replay
                        || !epoch.owner_sites.iter().any(|site| {
                            site.file_ordinal.index() == library.source_ordinal
                                && matches!(site.owner, ReplayOwner::GlobalObject)
                        }))
                {
                    return Err("scheduled private library source has no selected statements");
                }
                Ok((library.source_ordinal, module, statements))
            })
            .collect::<Result<Vec<_>, &'static str>>()?
    } else if library_programs.is_empty() {
        Vec::new()
    } else {
        return Err("private library programs require a collision epoch");
    };

    let (mut type_decls, mut type_resolved) = published_types.construction_prefix();
    let user_type_start = type_decls.len();
    type_resolved.resize(binder.type_groups.len(), None);
    let error = interner.well_known().error;
    let declaration_spans = ModuleDeclarationSpans::index(&binder);
    if let Some(epoch) = private_collision_epoch.as_ref() {
        for library in library_programs {
            let module = library_modules
                .get(library.source_ordinal)
                .copied()
                .ok_or("scheduled private library source has no retained module")?;
            reserve_type_decls_selected(
                decls::TypeDeclReservationState {
                    interner: &mut interner,
                    binder: &binder,
                    next_type_param: &mut next_type_param,
                    next_class_id: &mut next_class_id,
                    decls: &mut type_decls,
                    resolved: &mut type_resolved,
                },
                module,
                library.program,
                &epoch.affected_owners,
            );
            attach_selected_library_decl_owners(
                &mut lexical_events,
                LibraryFileOrdinal::new(library.source_ordinal),
                &binder,
                &epoch.owner_sites,
            )?;
            attach_class_bindings(
                &mut lexical_events,
                SourceOrdinal::Library(LibraryFileOrdinal::new(library.source_ordinal)),
                &binder,
                module,
                library.program,
                &type_decls,
                Some(&epoch.affected_owners),
            );
        }
    } else if !library_programs.is_empty() {
        return Err("private library programs require a collision epoch");
    }
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
            &declaration_spans,
        );
        attach_class_bindings(
            &mut lexical_events,
            SourceOrdinal::User(unit.module_ordinal),
            &binder,
            scope,
            unit.program,
            &type_decls,
            None,
        );
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .expect("one callable binder reservation pass");
    inspect_bindings(&binder, &module_scopes);
    enqueue_local_ambient_export_alias_diagnostics(
        &binder,
        &lexical_events,
        &mut external_effects,
    )?;
    enqueue_namespace_placement_diagnostics(&binder, &lexical_events, &mut external_effects)?;
    enqueue_ambient_context_diagnostics(&binder, &lexical_events, &mut external_effects)?;
    for placeholders in &module_placeholders {
        for placeholder in placeholders {
            if let Some(decl_id) = placeholder.value {
                decl_types.set(decl_id, error);
            }
        }
    }

    let pending_tickets = lexical_events.tickets();
    let mut pass = build_pass_with_tickets(
        &mut interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReportingPlan {
            reporting: PassReporting {
                source: SourceUnit::User {
                    module_ordinal: units[0].module_ordinal,
                    unit_slot: units[0].unit_slot,
                },
                lexical_events,
                suppress_effects: false,
            },
            pending_tickets,
            ticket_key: private_combined_record_ticket_key,
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
    pass.global_object_type = runtime.global_object_type;
    let complete_source_replay = private_collision_epoch
        .as_ref()
        .is_some_and(|epoch| epoch.complete_source_replay);
    pass.install_private_collision_epoch(private_collision_epoch);
    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_private_replay_trace_for_test(|trace, _| {
        trace.sparse_candidate_execution_started = true;
        trace.completion_or_semantic_query_steps =
            trace.completion_or_semantic_query_steps.saturating_add(1);
    });
    #[cfg(any(test, feature = "test-utils"))]
    if let library_compiler::PrivateReplayProductionFaultForTest::OmitScheduledOwner(owner) =
        library_compiler::private_replay_fault_for_test()
    {
        if !pass.private_collision_affected.contains(&owner) {
            return Err("private replay execution detected an omitted scheduled owner");
        }
    }
    for effects in external_effects.into_values() {
        let (owner, records) = effects.into_parts();
        let mut combined = CheckerEffects::new(PrivateCombinedRecordTicket::User(owner));
        for record in records {
            combined.records.record(record);
        }
        pass.enqueue_effects(combined);
    }

    if let (true, Some((ordinal, module, _))) = (
        has_library_semantic_identities,
        selected_library_units.first(),
    ) {
        pass.current_module = *module;
        pass.current_source = SourceUnit::Library {
            file_ordinal: LibraryFileOrdinal::new(*ordinal),
        };
    }
    pass.fill_type_decls_range(binder.module, user_type_start, pass.type_decls.len());
    let mut standalone_modules = selected_library_units
        .iter()
        .flat_map(|(_, module, lists)| lists.iter().map(|statements| (*module, *statements)))
        .collect::<Vec<_>>();
    standalone_modules.extend(
        module_scopes
            .iter()
            .copied()
            .zip(units.iter())
            .map(|(scope, unit)| (scope, unit.program.body.as_slice())),
    );
    for (scope, unit) in module_scopes.iter().copied().zip(units) {
        pass.current_module = scope;
        pass.current_source = SourceUnit::User {
            module_ordinal: unit.module_ordinal,
            unit_slot: unit.unit_slot,
        };
        pass.reserve_local_type_annotation_surfaces(scope, &unit.program.body);
    }
    pass.prepare_project_attached_namespace_values(&standalone_modules);
    pass.prepare_project_standalone_namespace_values(&standalone_modules);
    if pass.namespace_terminal_planning_failed() {
        return Err("private namespace terminal planning escaped the sparse epoch");
    }
    pass.publish_class_surfaces();
    pass.fill_pending_interfaces_range(binder.module, user_type_start, pass.type_decls.len());
    let user_namespace_modules = module_scopes.iter().copied().collect::<FxHashSet<_>>();
    pass.refresh_colliding_standalone_variable_surfaces(
        &standalone_modules,
        &user_namespace_modules,
    );
    pass.finalize_standalone_namespace_values();
    pass.precompute_standalone_namespace_value_aliases(&standalone_modules);
    let publication = pass.publish_type_groups();
    if publication.library_identity_selection_pending() {
        pass.suppress_effects = true;
    }
    pass.validate_published_class_surfaces();

    // Script globals span the whole project here, so the hoist reservations every module
    // contributes must all be in place before the first module executes — otherwise a forward
    // `declare var`/`function` reads an unfilled slot and its errors vanish (backlog 102).
    pass.refresh_private_global_object(Vec::new());
    let mut library_surfaces = Vec::with_capacity(selected_library_units.len());
    for (ordinal, scope, statements) in &selected_library_units {
        pass.current_module = *scope;
        pass.current_source = SourceUnit::Library {
            file_ordinal: LibraryFileOrdinal::new(*ordinal),
        };
        let surfaces = pass.reserve_function_surfaces_for_lists(*scope, statements);
        for statements in statements {
            pass.reserve_var_annotation_surfaces(*scope, statements);
        }
        library_surfaces.push(surfaces);
    }
    let mut module_surfaces = Vec::with_capacity(units.len());
    for (scope, unit) in module_scopes.iter().copied().zip(units) {
        pass.current_module = scope;
        pass.current_source = SourceUnit::User {
            module_ordinal: unit.module_ordinal,
            unit_slot: unit.unit_slot,
        };
        let mut surfaces = pass.reserve_function_surfaces(scope, &unit.program.body);
        pass.reserve_var_annotation_surfaces(scope, &unit.program.body);
        pass.reserve_continuation_global_augmentation_surfaces(&unit.program.body, &mut surfaces);
        module_surfaces.push(surfaces);
    }
    let global_contributor_names = units
        .iter()
        .map(|unit| {
            source_global_binding_census(unit.program, unit.compilation_unit.binding)
                .candidates
                .into_iter()
                .filter_map(|(name, candidate)| {
                    (name != "globalThis" && candidate.global_object_contributor).then_some(name)
                })
                .collect::<FxHashSet<_>>()
        })
        .collect::<Vec<_>>();
    pass.refresh_user_global_object(
        module_scopes
            .iter()
            .copied()
            .zip(units)
            .map(|(scope, unit)| (scope, unit.program, unit.compilation_unit.binding)),
    );
    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_private_replay_trace_for_test(|trace, event| {
        trace.candidate_activation_started = event;
    });
    for ((ordinal, scope, statements), mut surfaces) in
        selected_library_units.iter().zip(library_surfaces)
    {
        pass.current_module = *scope;
        pass.current_source = SourceUnit::Library {
            file_ordinal: LibraryFileOrdinal::new(*ordinal),
        };
        for statements in statements {
            let mut no_return = None;
            pass.check_statement_list_with_surfaces(
                *scope,
                statements,
                None,
                &mut no_return,
                &mut surfaces,
            );
        }
    }
    for (((scope, unit), mut surfaces), contributor_names) in module_scopes
        .iter()
        .copied()
        .zip(units)
        .zip(module_surfaces)
        .zip(global_contributor_names)
    {
        pass.current_module = scope;
        pass.current_source = SourceUnit::User {
            module_ordinal: unit.module_ordinal,
            unit_slot: unit.unit_slot,
        };
        pass.build_flow_graph(scope, &unit.program.body);
        let mut no_return = None;
        pass.check_statement_list_with_global_contributors(
            scope,
            &unit.program.body,
            None,
            &mut no_return,
            &mut surfaces,
            &contributor_names,
        );
        emit_test_incomplete(&mut pass);
    }
    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_user_source_checks_for_test(units.len());

    let activated_library_record_owners = pass
        .pending_effects
        .iter()
        .filter(|effects| effects.is_activated())
        .filter_map(|effects| match effects.records.owner() {
            PrivateCombinedRecordTicket::Library(owner) => library_event_ledger
                .key(owner)
                .ok()
                .map(ReplayOwner::Statement),
            PrivateCombinedRecordTicket::DisabledLibrary(_)
            | PrivateCombinedRecordTicket::User(_) => None,
        })
        .collect::<BTreeSet<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    {
        if library_compiler::private_replay_production_trace_active_for_test() {
            let activated = activated_library_record_owners.clone();
            library_compiler::record_private_replay_trace_for_test(|trace, _| {
                trace.candidate_activated_library_record_owners = activated;
            });
        }
    }
    let batches = finish_semantic_effects(&mut pass);
    for batch in batches {
        let (owner, records) = batch.into_parts();
        match owner {
            PrivateCombinedRecordTicket::Library(owner) => library_event_ledger
                .complete(owner, records)
                .map_err(|_| "private library record completion failed")?,
            PrivateCombinedRecordTicket::DisabledLibrary(_) => {}
            PrivateCombinedRecordTicket::User(owner) => event_store
                .complete(owner, records)
                .map_err(|_| "private user record completion failed")?,
        }
    }
    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_private_replay_trace_for_test(|trace, _| {
        trace.completion_selection_started = true;
    });
    let reserved_library_record_owners = library_event_ledger
        .reserved_record_keys()
        .into_iter()
        .map(ReplayOwner::Statement)
        .collect::<BTreeSet<_>>();
    let library_output = library_event_ledger
        .finish_with_fingerprints()
        .map_err(|_| "private library record replay failed")?;
    #[cfg(any(test, feature = "test-utils"))]
    let mut library_output = library_output;
    #[cfg(any(test, feature = "test-utils"))]
    if library_compiler::private_replay_fault_for_test()
        == library_compiler::PrivateReplayProductionFaultForTest::
            OmitExpectedBaselineRecordDuringCompletion
    {
        if let Some(expected) = expected_library_records.first().cloned() {
            if let Some(index) = library_output.fingerprints.iter().position(|fingerprint| {
                ReplayOwner::Statement(fingerprint.key) == expected.owner
            }) {
                library_output.fingerprints.remove(index);
                library_compiler::record_private_replay_trace_for_test(|trace, _| {
                    trace.omitted_expected_baseline = Some(expected);
                });
            }
        }
    }
    let mut observed_library_records = library_output
        .fingerprints
        .into_iter()
        .map(|fingerprint| replay_index::ReplayBaselineRecord {
            owner: ReplayOwner::Statement(fingerprint.key),
            record_count: fingerprint.record_count,
            digest: fingerprint.digest,
        })
        .collect::<Vec<_>>();
    // Reserved but unqueried records retain their authenticated sealed result.
    observed_library_records.extend(
        expected_library_records
            .iter()
            .filter(|record| {
                reserved_library_record_owners.contains(&record.owner)
                    && !activated_library_record_owners.contains(&record.owner)
            })
            .cloned(),
    );
    observed_library_records.sort_by_key(|record| record.owner);
    #[cfg(any(test, feature = "test-utils"))]
    library_compiler::record_private_replay_trace_for_test(|trace, event| {
        trace.baseline_validation_started = event;
    });
    let baseline_validation = replay_index::validate_replay_baselines(
        &expected_library_records,
        &observed_library_records,
    );
    if !complete_source_replay && baseline_validation.is_err() {
        #[cfg(any(test, feature = "test-utils"))]
        library_compiler::record_private_replay_trace_for_test(|trace, _| {
            trace.failure =
                Some(library_compiler::PrivateReplayProductionFailureForTest::BaselineMissing);
        });
        return Err("private library record fingerprints differ from the sealed replay plan");
    }
    let _drained_library_records = library_output.records;
    let mut records = UserReportingAdapter { event_store }.finish(Vec::new(), inspect_records);
    inspect_final(&pass, next_class_id);
    Ok(units
        .iter()
        .map(|unit| {
            let (mut diagnostics, mut incomplete) =
                records.remove(&unit.module_ordinal).unwrap_or_default();
            fail_closed_identity_selection(publication, &mut diagnostics, &mut incomplete);
            CheckResult {
                module_ordinal: unit.module_ordinal,
                unit_slot: unit.unit_slot,
                diagnostics,
                incomplete,
            }
        })
        .collect())
}

/// Check a dependency-ordered project in a universe forked from a published default-library base.
///
/// The inspector-free face of [`check_project_programs_with_owned_library`], whose closure bounds
/// mention a checker-private type and so cannot cross the crate.
pub fn check_project_programs_with_library<'ast>(
    state: library_compiler::OwnedLibraryRuntimeState,
    units: &[ProjectProgram<'ast>],
) -> Result<Vec<CheckResult>, &'static str> {
    check_project_programs_with_owned_library(state, units, |_, _| {}, |_, _| {})
}

pub fn check_private_project_programs_with_library<'ast>(
    state: library_compiler::OwnedLibraryRuntimeState,
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[ProjectProgram<'ast>],
) -> Result<Vec<CheckResult>, &'static str> {
    check_project_programs_with_owned_library_inner(
        state,
        library_programs,
        units,
        None,
        |_, _| {},
        |_, _| {},
        |_| {},
    )
}

pub fn check_complete_source_project_programs_with_library<'ast>(
    state: library_compiler::OwnedLibraryRuntimeState,
    checkpoint: crate::binder::bind::LibraryBinderCheckpoint,
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[ProjectProgram<'ast>],
) -> Result<Vec<CheckResult>, &'static str> {
    check_project_programs_with_owned_library_inner(
        state,
        library_programs,
        units,
        Some(checkpoint),
        |_, _| {},
        |_, _| {},
        |_| {},
    )
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrivateProjectRootSlotsForTest {
    pub value: Option<ValueStorageId>,
    pub ty: Option<TypeGroupId>,
    pub namespace: Option<crate::binder::namespace::NamespaceId>,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct PrivateProjectSemanticEvidenceForTest {
    pub results: Vec<CheckResult>,
    pub normalized_records: Vec<(ModuleOrdinal, String)>,
    pub root_slots: BTreeMap<String, PrivateProjectRootSlotsForTest>,
    pub final_identity_ends: [usize; 9],
    pub normalized_root_projection: Vec<String>,
    pub normalized_semantic_identities: Vec<String>,
    pub affected_terminals_unavailable: BTreeSet<String>,
    pub affected_terminals_from_frozen_prefix: BTreeSet<String>,
    pub visible_root_members: BTreeSet<String>,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct PrivateProjectScaleEvidenceForTest {
    pub results: Vec<CheckResult>,
    pub visible_root_members: BTreeSet<String>,
}

#[cfg(any(test, feature = "test-utils"))]
fn private_project_visible_root_members_for_test(
    pass: &Pass<'_, '_, PrivateCombinedRecordTicket>,
    root_names: &[String],
) -> BTreeSet<String> {
    use self::type_groups::{PublishedTypeGroupSurface, PublishedTypeGroupTerminal};

    root_names
        .iter()
        .filter_map(|name| pass.type_decl_id_replay(pass.binder.compilation_global, name))
        .filter_map(|group| pass.type_environment.published().groups().get(group))
        .filter_map(|terminal| match terminal {
            PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                PublishedTypeGroupSurface::Template(template) => Some(template),
                PublishedTypeGroupSurface::Class(class) => match pass
                    .type_environment
                    .published()
                    .classes()
                    .published_class(class)
                {
                    crate::class_semantics::DemandOutcome::Ready(surface) => {
                        Some(surface.instance_template())
                    }
                    crate::class_semantics::DemandOutcome::Exhausted(_) => None,
                },
            },
            PublishedTypeGroupTerminal::Unavailable(_) => None,
        })
        .filter_map(|ty| pass.interner.store().object_type(ty))
        .flat_map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.name.clone())
        })
        .collect()
}

#[cfg(any(test, feature = "test-utils"))]
fn private_project_semantic_evidence_for_test(
    pass: &Pass<'_, '_, PrivateCombinedRecordTicket>,
    root_names: &[String],
    next_class_id: u32,
) -> PrivateProjectSemanticEvidenceForTest {
    use self::type_groups::{PublishedTypeGroupSurface, PublishedTypeGroupTerminal};

    let mut root_rows = crate::binder::roots::collect_root_rows(pass.binder).unwrap_or_default();
    let refused_global_names = pass
        .binder
        .namespaces
        .merges()
        .filter(|record| {
            record.owner == crate::binder::namespace::DeclarationOwner::CompilationGlobal
                && record.classification.disposition
                    != crate::binder::namespace::MergeDisposition::Admitted
        })
        .map(|record| record.name.as_ref())
        .collect::<BTreeSet<_>>();
    root_rows.retain(|row| !refused_global_names.contains(row.name.as_str()));
    let selected = root_names.iter().collect::<BTreeSet<_>>();
    let root_slots = root_rows
        .iter()
        .filter(|row| selected.contains(&row.name))
        .map(|row| {
            (
                row.name.clone(),
                PrivateProjectRootSlotsForTest {
                    value: row.value,
                    ty: row.ty,
                    namespace: row.namespace,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut normalized_root_projection = root_names
        .iter()
        .map(|name| {
            let slots = root_slots.get(name).copied().unwrap_or_default();
            let ty = slots
                .ty
                .and_then(|group| pass.type_environment.published().groups().get(group))
                .map(|terminal| match terminal {
                    PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                        PublishedTypeGroupSurface::Template(template) => {
                            render_type(pass.interner.store(), template, false)
                        }
                        PublishedTypeGroupSurface::Class(class) => match pass
                            .type_environment
                            .published()
                            .classes()
                            .published_class(class)
                        {
                            crate::class_semantics::DemandOutcome::Ready(surface) => render_type(
                                pass.interner.store(),
                                surface.instance_template(),
                                false,
                            ),
                            crate::class_semantics::DemandOutcome::Exhausted(_) => {
                                "class:unavailable".to_owned()
                            }
                        },
                    },
                    PublishedTypeGroupTerminal::Unavailable(cause) => {
                        format!("unavailable:{:?}", cause.cause)
                    }
                });
            format!(
                "{name}:value={}:type={}:namespace={}",
                slots.value.is_some(),
                ty.unwrap_or_else(|| "absent".to_owned()),
                slots.namespace.is_some()
            )
        })
        .collect::<Vec<_>>();
    normalized_root_projection.sort();

    let mut affected_terminals_unavailable = BTreeSet::new();
    let mut affected_terminals_from_frozen_prefix = BTreeSet::new();
    for (name, slots) in &root_slots {
        let Some(group_id) = pass
            .library_semantic_identities
            .as_ref()
            .and_then(|identities| identities.group_for_name_for_test(name))
            .or(slots.ty)
        else {
            continue;
        };
        let key = format!("type:{name}");
        match pass.type_environment.published().groups().get(group_id) {
            Some(PublishedTypeGroupTerminal::Unavailable(_)) => {
                affected_terminals_unavailable.insert(key);
            }
            Some(PublishedTypeGroupTerminal::Ready(_))
                if !pass
                    .type_environment
                    .published()
                    .groups()
                    .is_replaced_for_test(group_id) =>
            {
                affected_terminals_from_frozen_prefix.insert(key);
            }
            Some(PublishedTypeGroupTerminal::Ready(_)) => {}
            None => {}
        }
    }
    for owner in &pass.private_collision_affected {
        let ReplayOwner::TypeGroup(group) = owner else {
            continue;
        };
        if !matches!(
            pass.type_environment.published().groups().get(*group),
            Some(PublishedTypeGroupTerminal::Unavailable(_))
        ) {
            continue;
        }
        if let Some(name) = pass
            .binder
            .type_groups
            .get(*group)
            .map(|group| group.name.as_str())
        {
            affected_terminals_unavailable.insert(format!("type:{name}"));
        }
    }
    for group in &pass.private_collision_unavailable_type_groups {
        if let Some(name) = pass
            .binder
            .type_groups
            .get(*group)
            .map(|group| group.name.as_str())
        {
            affected_terminals_unavailable.insert(format!("type:{name}"));
        }
    }
    affected_terminals_from_frozen_prefix
        .retain(|key| !affected_terminals_unavailable.contains(key));
    let visible_root_members = root_slots
        .values()
        .filter_map(|slots| slots.ty)
        .filter_map(|group| pass.type_environment.published().groups().get(group))
        .filter_map(|terminal| match terminal {
            PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                PublishedTypeGroupSurface::Template(template) => Some(template),
                PublishedTypeGroupSurface::Class(class) => match pass
                    .type_environment
                    .published()
                    .classes()
                    .published_class(class)
                {
                    crate::class_semantics::DemandOutcome::Ready(surface) => {
                        Some(surface.instance_template())
                    }
                    crate::class_semantics::DemandOutcome::Exhausted(_) => None,
                },
            },
            PublishedTypeGroupTerminal::Unavailable(_) => None,
        })
        .filter_map(|ty| pass.interner.store().object_type(ty))
        .flat_map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.name.clone())
        })
        .collect();
    let normalized_semantic_identities = pass
        .library_semantic_identities
        .as_ref()
        .map_or_else(Vec::new, |identities| {
            identities.normalized_projection_for_test(pass.interner.store())
        });
    PrivateProjectSemanticEvidenceForTest {
        results: Vec::new(),
        normalized_records: Vec::new(),
        root_slots,
        final_identity_ends: [
            pass.interner.store().len(),
            usize::try_from(pass.next_type_param).unwrap_or(usize::MAX),
            usize::try_from(next_class_id).unwrap_or(usize::MAX),
            pass.binder.graph.len(),
            pass.binder.symbols.len(),
            pass.binder.declarations.len(),
            pass.binder.type_groups.len(),
            pass.binder.namespaces.len(),
            pass.decl_types.len(),
        ],
        normalized_root_projection,
        normalized_semantic_identities,
        affected_terminals_unavailable,
        affected_terminals_from_frozen_prefix,
        visible_root_members,
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_private_project_programs_with_library_evidence<'ast>(
    state: library_compiler::OwnedLibraryRuntimeState,
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[ProjectProgram<'ast>],
    root_names: &[String],
) -> Result<PrivateProjectSemanticEvidenceForTest, &'static str> {
    let evidence = std::cell::RefCell::new(None);
    let normalized_records = std::cell::RefCell::new(Vec::new());
    let results = check_project_programs_with_owned_library_inner(
        state,
        library_programs,
        units,
        None,
        |_, _| {},
        |pass, next_class_id| {
            evidence.replace(Some(private_project_semantic_evidence_for_test(
                pass,
                root_names,
                next_class_id,
            )));
        },
        |records| {
            normalized_records.replace(
                records
                    .iter()
                    .map(|(key, record)| {
                        let normalized = match record {
                            CheckerRecord::Diagnostic(diagnostic) => {
                                format!("{} {}", diagnostic.code.as_str(), diagnostic.message)
                            }
                            CheckerRecord::Incomplete(incomplete) => {
                                format!("incomplete {incomplete:?}")
                            }
                        };
                        (key.module_ordinal, normalized)
                    })
                    .collect(),
            );
        },
    )?;
    let Some(mut evidence) = evidence.into_inner() else {
        return Err("private project evidence was not observed");
    };
    evidence.results = results;
    evidence.normalized_records = normalized_records.into_inner();
    Ok(evidence)
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_private_project_programs_with_scale_evidence<'ast>(
    state: library_compiler::OwnedLibraryRuntimeState,
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[ProjectProgram<'ast>],
    root_names: &[String],
) -> Result<PrivateProjectScaleEvidenceForTest, &'static str> {
    let visible_root_members = std::cell::RefCell::new(None);
    let results = check_project_programs_with_owned_library_inner(
        state,
        library_programs,
        units,
        None,
        |_, _| {},
        |pass, _| {
            visible_root_members.replace(Some(private_project_visible_root_members_for_test(
                pass, root_names,
            )));
        },
        |_| {},
    )?;
    let Some(visible_root_members) = visible_root_members.into_inner() else {
        return Err("private project scale evidence was not observed");
    };
    Ok(PrivateProjectScaleEvidenceForTest {
        results,
        visible_root_members,
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_complete_library_project_programs_with_evidence<'ast>(
    state: library_compiler::OwnedLibraryRuntimeState,
    checkpoint: crate::binder::bind::LibraryBinderCheckpoint,
    library_programs: &[crate::frontend::AuxiliaryProgram<'ast>],
    units: &[ProjectProgram<'ast>],
    root_names: &[String],
) -> Result<PrivateProjectSemanticEvidenceForTest, &'static str> {
    let evidence = std::cell::RefCell::new(None);
    let normalized_records = std::cell::RefCell::new(Vec::new());
    let results = check_project_programs_with_owned_library_inner(
        state,
        library_programs,
        units,
        Some(checkpoint),
        |_, _| {},
        |pass, next_class_id| {
            evidence.replace(Some(private_project_semantic_evidence_for_test(
                pass,
                root_names,
                next_class_id,
            )));
        },
        |records| {
            normalized_records.replace(
                records
                    .iter()
                    .map(|(key, record)| {
                        let normalized = match record {
                            CheckerRecord::Diagnostic(diagnostic) => {
                                format!("{} {}", diagnostic.code.as_str(), diagnostic.message)
                            }
                            CheckerRecord::Incomplete(incomplete) => {
                                format!("incomplete {incomplete:?}")
                            }
                        };
                        (key.module_ordinal, normalized)
                    })
                    .collect(),
            );
        },
    )?;
    let Some(mut evidence) = evidence.into_inner() else {
        return Err("complete-source project evidence was not observed");
    };
    evidence.results = results;
    evidence.normalized_records = normalized_records.into_inner();
    Ok(evidence)
}

/// Check one user source in a universe forked from a published default-library base.
///
/// The single-source analogue of [`check_project_programs_with_owned_library`]: it resumes the
/// frozen library binder for exactly one appended suffix, so a flat fixture keeps the
/// single-file semantics [`check_program`] gives it on the prelude path. The existing
/// `library_compiler` single-source route is welded to its witness/counter machinery, so this is
/// the plain one.
pub fn check_program_with_owned_library<'ast>(
    state: library_compiler::OwnedLibraryRuntimeState,
    program: &'ast Program<'ast>,
) -> Result<CheckResult, &'static str> {
    let (mut interner, binder, base) = state.into_user_project_base();
    let (mut builder, source) = ProjectBinderBuilder::resume_frozen_library(binder);
    let unit = CompilationUnit::implementation(source, program);
    builder.reserve_script_namespace_roots([(program, unit)]);
    let (module, _) = builder.add_module(program, &[], unit);
    let binder = builder.finish_frozen_library_continuation(Some(module))?;
    binder
        .namespaces
        .validate_compilation_origin_index()
        .map_err(|_| "binder source-origin index conflict")?;
    let reporting = reserve_continuation_reporting(
        program,
        ModuleOrdinal::new(0),
        UnitSlot::new(0),
        unit.binding,
    )
    .map_err(|_| "user lexical reservation failed")?;
    Ok(check_bound_user_program(
        &mut interner,
        binder,
        program,
        reporting,
        base,
        |_, _, _, _, _| {},
    ))
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_project_programs_with_binding_inspector<'ast, F>(
    interner: &mut Interner,
    units: &[ProjectProgram<'ast>],
    inspect: F,
) -> Vec<CheckResult>
where
    F: FnOnce(&Binder, &LexicalReservations, &[ScopeId]),
{
    check_project_programs_inner(interner, units, inspect, |_, _, _, _, _, _, _| {}, |_| {})
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, PartialEq, Eq)]
pub struct ProjectNamespaceRootInspection {
    pub name: String,
    pub symbol: SymbolId,
    pub terminal: &'static str,
    pub namespace_storage: Option<ValueStorageId>,
    pub terminal_storage: Option<ValueStorageId>,
    pub ty: Option<TypeId>,
    pub published: Option<TypeId>,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, PartialEq, Eq)]
pub enum ProjectReplayRecordInspection {
    Diagnostic(String),
    Incomplete(String),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, PartialEq, Eq)]
pub struct ProjectReplayInspection {
    pub key: events::EventKey,
    pub record: ProjectReplayRecordInspection,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, PartialEq, Eq)]
pub struct ProjectNamespaceValueInspection {
    pub roots: Vec<ProjectNamespaceRootInspection>,
    pub replay: Vec<ProjectReplayInspection>,
}

#[cfg(any(test, feature = "test-utils"))]
pub fn check_project_programs_with_namespace_value_inspector<'ast, F>(
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

pub(crate) fn bind_library_checkpoint_project_programs(
    checkpoint: crate::binder::bind::LibraryBinderCheckpoint,
    units: &[ProjectProgram<'_>],
) -> Result<BoundProjectBinder, String> {
    let checkpoint_ends = checkpoint.checkpoint_ends();
    let (builder, _) = checkpoint.into_continuation();
    let source_offset = u32::try_from(checkpoint_ends.next_source)
        .map_err(|_| "library source prefix exceeds u32")?
        .checked_sub(1)
        .ok_or_else(|| "library source prefix omits the prelude".to_owned())?;
    #[cfg(any(test, feature = "test-utils"))]
    {
        PROJECT_BINDING_CHECKPOINT_SEEDS
            .set(PROJECT_BINDING_CHECKPOINT_SEEDS.get().saturating_add(1));
    }
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    for (slot, unit) in units.iter().enumerate() {
        debug_assert_eq!(unit.unit_slot.index(), slot);
        lexical_events
            .reserve_continuation_program(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                unit.compilation_unit.binding,
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
        AuthoritativeProjectBinderFinish::Continuation,
    )?;
    #[cfg(any(test, feature = "test-utils"))]
    {
        PROJECT_BINDING_CHECKPOINT_PRODUCTS
            .with(|products| products.borrow_mut().push(bound.normalized.clone()));
    }
    Ok(bound)
}

#[cfg(any(test, feature = "test-utils"))]
fn bind_fresh_project_programs(
    prelude: &Program<'_>,
    units: &[ProjectProgram<'_>],
    lexical_events: &LexicalReservations,
    external_effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Result<BoundProjectBinder, String> {
    #[cfg(any(test, feature = "test-utils"))]
    PROJECT_BINDING_FRESH_SEEDS.set(PROJECT_BINDING_FRESH_SEEDS.get().saturating_add(1));
    let bound = bind_authoritative_project_core(
        ProjectBinderBuilder::new(prelude),
        units,
        0,
        lexical_events,
        external_effects,
        AuthoritativeProjectBinderFinish::Fresh,
    )?;
    #[cfg(any(test, feature = "test-utils"))]
    PROJECT_BINDING_FRESH_PRODUCTS
        .with(|products| products.borrow_mut().push(bound.normalized.clone()));
    Ok(bound)
}

#[derive(Copy, Clone)]
enum AuthoritativeProjectBinderFinish {
    #[cfg(any(test, feature = "test-utils"))]
    Fresh,
    Continuation,
}

fn bind_authoritative_project_core<'ast>(
    mut builder: ProjectBinderBuilder<'ast>,
    units: &[ProjectProgram<'ast>],
    source_offset: u32,
    lexical_events: &LexicalReservations,
    external_effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
    finish: AuthoritativeProjectBinderFinish,
) -> Result<BoundProjectBinder, String> {
    #[cfg(any(test, feature = "test-utils"))]
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
        let imports = infallible(imported_symbols(
            unit,
            &exports,
            lexical_events,
            external_effects,
        ));
        let compilation = shifted_unit(unit)?;
        let (scope, placeholders) = builder.add_module(unit.program, &imports, compilation);
        let surface = infallible(collect_exports(
            &builder,
            scope,
            unit.program,
            unit.module_ordinal,
            lexical_events,
            external_effects,
        ));
        module_scopes.push(scope);
        module_placeholders.push(placeholders);
        exports.push(surface);
    }
    let final_module = module_scopes.last().copied();
    let binder = match finish {
        #[cfg(any(test, feature = "test-utils"))]
        AuthoritativeProjectBinderFinish::Fresh => {
            builder.finish(final_module.unwrap_or(ScopeId(0)))
        }
        AuthoritativeProjectBinderFinish::Continuation => builder
            .finish_frozen_library_continuation(final_module)
            .map_err(str::to_owned)?,
    };
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
    #[cfg(any(test, feature = "test-utils"))]
    let normalized =
        normalized_project_binding_product(&binder, units, &module_scopes, &module_placeholders);
    #[cfg(any(test, feature = "test-utils"))]
    PROJECT_BINDING_PRODUCTS.set(PROJECT_BINDING_PRODUCTS.get().saturating_add(1));
    Ok(BoundProjectBinder {
        binder,
        module_scopes,
        module_placeholders,
        project_sources,
        #[cfg(any(test, feature = "test-utils"))]
        normalized,
    })
}

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn record_continuation_project_binding_consumed_for_test() {
    PROJECT_BINDING_CONTINUATION_CONSUMERS.set(
        PROJECT_BINDING_CONTINUATION_CONSUMERS
            .get()
            .saturating_add(1),
    );
}

#[cfg(any(test, feature = "test-utils"))]
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
            .reserve_continuation_program(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                unit.compilation_unit.binding,
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
    ) = bootstrap_test_support_prelude(interner, |prelude| {
        let bound =
            bind_fresh_project_programs(prelude, units, &lexical_events, &mut external_effects)
                .expect("authoritative fresh project binding succeeds");
        module_scopes = bound.module_scopes;
        module_placeholders = bound.module_placeholders;
        bound.binder
    });
    #[cfg(any(test, feature = "test-utils"))]
    PROJECT_BINDING_ORDINARY_CONSUMERS
        .set(PROJECT_BINDING_ORDINARY_CONSUMERS.get().saturating_add(1));

    let (mut type_decls, mut type_resolved) = published_types.construction_prefix();
    let user_type_start = type_decls.len();
    type_resolved.resize(binder.type_groups.len(), None);
    let error = interner.well_known().error;
    let declaration_spans = ModuleDeclarationSpans::index(&binder);
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
            &declaration_spans,
        );
        attach_class_bindings(
            &mut lexical_events,
            SourceOrdinal::User(unit.module_ordinal),
            &binder,
            scope,
            unit.program,
            &type_decls,
            None,
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
    infallible(enqueue_local_ambient_export_alias_diagnostics(
        &binder,
        &lexical_events,
        &mut external_effects,
    ));
    infallible(enqueue_namespace_placement_diagnostics(
        &binder,
        &lexical_events,
        &mut external_effects,
    ));
    infallible(enqueue_ambient_context_diagnostics(
        &binder,
        &lexical_events,
        &mut external_effects,
    ));

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
    let publication = pass.publish_type_groups();
    if publication.library_identity_selection_pending() {
        pass.suppress_effects = true;
    }
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
            let (mut diagnostics, mut incomplete) =
                records.remove(&unit.module_ordinal).unwrap_or_default();
            fail_closed_identity_selection(publication, &mut diagnostics, &mut incomplete);
            CheckResult {
                module_ordinal: unit.module_ordinal,
                unit_slot: unit.unit_slot,
                diagnostics,
                incomplete,
            }
        })
        .collect()
}

fn imported_symbols<Ticket: UserReportingOwner>(
    unit: &ProjectProgram<'_>,
    exports: &[ExportSurface],
    reservations: &LexicalReservations<Ticket>,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Result<Vec<ImportedSymbol>, Ticket::Error> {
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
                )?;
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
                        )?;
                        imports.push(placeholder_import(import));
                    }
                }
            }
        }
    }
    Ok(imports)
}

fn placeholder_import(import: &ProjectImport) -> ImportedSymbol {
    if import.type_only {
        ImportedSymbol::placeholder_type(import.local.clone(), import.local_span)
    } else {
        ImportedSymbol::placeholder_value_and_type(import.local.clone(), import.local_span)
    }
}

fn collect_exports<Ticket: UserReportingOwner>(
    builder: &ProjectBinderBuilder<'_>,
    scope: ScopeId,
    program: &Program<'_>,
    module_ordinal: ModuleOrdinal,
    reservations: &LexicalReservations<Ticket>,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Result<ExportSurface, Ticket::Error> {
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
            // import side in `crates/typokat-frontend/src/frontend.rs`, where the
            // outer kind ORs with each specifier.
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
                collect_list_export(&mut context, specifier, outer_type_only, stmt.span().start)?;
            }
        }
    }
    Ok(surface)
}

fn collect_declaration_export(
    builder: &ProjectBinderBuilder<'_>,
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

struct ListExportContext<'a, Ticket: UserReportingOwner> {
    builder: &'a ProjectBinderBuilder<'a>,
    scope: ScopeId,
    surface: &'a mut ExportSurface,
    module_ordinal: ModuleOrdinal,
    reservations: &'a LexicalReservations<Ticket>,
    effects: &'a mut BTreeMap<UserRecordTicket, CandidateEffects>,
}

fn collect_list_export<Ticket: UserReportingOwner>(
    context: &mut ListExportContext<'_, Ticket>,
    specifier: &ExportSpecifier<'_>,
    outer_type_only: bool,
    owner_start: u32,
) -> Result<(), Ticket::Error> {
    let Some(local) = module_export_name(&specifier.local) else {
        return Ok(());
    };
    let Some(exported) = module_export_name(&specifier.exported) else {
        return Ok(());
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
        )?;
        return Ok(());
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
    Ok(())
}

fn enqueue_external_diagnostic<Ticket: UserReportingOwner>(
    reservations: &LexicalReservations<Ticket>,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
    module_ordinal: ModuleOrdinal,
    owner_start: u32,
    diagnostic: Diagnostic,
) -> Result<(), Ticket::Error> {
    let owner = reservations
        .owner_at(
            SourceOrdinal::User(module_ordinal),
            owner_start,
            LexicalOwnerPhase::Immediate,
        )
        .expect("import/export diagnostic owner must be lexically reserved");
    let owner = owner.ticket.user_ticket()?;
    effects
        .entry(owner)
        .or_insert_with(|| CandidateEffects::new(owner))
        .diagnostic(diagnostic);
    Ok(())
}

fn enqueue_local_ambient_export_alias_diagnostics<Ticket: UserReportingOwner>(
    binder: &Binder,
    reservations: &LexicalReservations<Ticket>,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Result<(), Ticket::Error> {
    for failure in binder.local_ambient_export_alias_failures() {
        let Some(original_module) = user_original_module(failure.origin) else {
            continue;
        };
        let module_ordinal = ModuleOrdinal::new(original_module.index());
        let owner = reservations
            .export_alias_owner(SourceOrdinal::User(module_ordinal), failure.local_span)
            .expect("local ambient export alias must have an exact lexical owner");
        let owner = owner.ticket.user_ticket()?;
        effects
            .entry(owner)
            .or_insert_with(|| CandidateEffects::new(owner))
            .diagnostic(match failure.kind {
                LocalAmbientExportAliasFailureKind::Missing => {
                    Diagnostic::cannot_find_name(failure.local_span, &failure.local_name)
                }
                LocalAmbientExportAliasFailureKind::NonLocal => {
                    Diagnostic::cannot_export_non_local(failure.local_span, &failure.local_name)
                }
            });
    }
    Ok(())
}

fn enqueue_namespace_placement_diagnostics<Ticket: UserReportingOwner>(
    binder: &Binder,
    reservations: &LexicalReservations<Ticket>,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Result<(), Ticket::Error> {
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
        let owner = owner.ticket.user_ticket()?;
        effects
            .entry(owner)
            .or_insert_with(|| CandidateEffects::new(owner))
            .diagnostic(diagnostic);
    }
    Ok(())
}

fn enqueue_ambient_context_diagnostics<Ticket: UserReportingOwner>(
    binder: &Binder,
    reservations: &LexicalReservations<Ticket>,
    effects: &mut BTreeMap<UserRecordTicket, CandidateEffects>,
) -> Result<(), Ticket::Error> {
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
        let owner = owner.ticket.user_ticket()?;
        let candidate = effects
            .entry(owner)
            .or_insert_with(|| CandidateEffects::new(owner));
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
        let owner = owner.ticket.user_ticket()?;
        effects
            .entry(owner)
            .or_insert_with(|| CandidateEffects::new(owner))
            .diagnostic(diagnostic);
    }
    Ok(())
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

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static DECLARATION_OWNER_SCAN_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(any(test, feature = "test-utils"))]
fn record_declaration_owner_scan_for_test(rows: u64) {
    DECLARATION_OWNER_SCAN_ROWS.set(DECLARATION_OWNER_SCAN_ROWS.get().saturating_add(rows));
}

/// Attributes the declaration rows owner attachment exposes, per module
/// (see `declaration_owner_scaling_spec`).
#[cfg(test)]
pub(crate) struct DeclarationOwnerScanScopeForTest(u64);

#[cfg(test)]
impl DeclarationOwnerScanScopeForTest {
    pub(crate) fn start() -> Self {
        Self(DECLARATION_OWNER_SCAN_ROWS.get())
    }

    pub(crate) fn finish(self) -> u64 {
        DECLARATION_OWNER_SCAN_ROWS.get().saturating_sub(self.0)
    }
}

/// Per-module `[start, end)` declaration spans over the user delta. Binding appends a module's
/// declarations together, so owner attachment walks its own span instead of filtering the whole
/// project once per module (backlog 89).
pub(crate) struct ModuleDeclarationSpans {
    spans: FxHashMap<ScopeId, std::ops::Range<u32>>,
}

impl ModuleDeclarationSpans {
    /// One pass over the delta for the whole compilation, not one per module.
    pub(crate) fn index(binder: &Binder) -> Self {
        let mut spans: FxHashMap<ScopeId, std::ops::Range<u32>> = FxHashMap::default();
        for declaration in binder.declarations.local_declarations() {
            let id = declaration.id.0;
            spans
                .entry(declaration.site.module)
                .and_modify(|span| {
                    span.start = span.start.min(id);
                    span.end = span.end.max(id.saturating_add(1));
                })
                .or_insert(id..id.saturating_add(1));
        }
        Self { spans }
    }

    fn module(&self, scope: ScopeId) -> std::ops::Range<u32> {
        self.spans.get(&scope).cloned().unwrap_or(0..0)
    }
}

fn attach_type_decl_owners<Ticket: Copy + PartialEq>(
    reservations: &mut LexicalReservations<Ticket>,
    source_ordinal: SourceOrdinal,
    binder: &Binder,
    scope: ScopeId,
    program: &Program<'_>,
    spans: &ModuleDeclarationSpans,
) {
    let span = spans.module(scope);
    #[cfg(any(test, feature = "test-utils"))]
    record_declaration_owner_scan_for_test(u64::from(span.end.saturating_sub(span.start)));
    for id in span {
        let Some(declaration) = binder.declarations.get(DeclId(id)) else {
            continue;
        };
        // Spans are dense per module today; the guard keeps attribution exact if that ever changes.
        if declaration.site.module != scope {
            continue;
        }
        reservations
            .attach_declaration_owner(
                declaration.id,
                source_ordinal,
                declaration.kind,
                declaration.site.declaration_span,
                declaration.site.binding_span,
            )
            .expect("source declaration must have its exact lexical event owner");
        let SourceOrdinal::Library(file_ordinal) = source_ordinal else {
            continue;
        };
        let declaration_scope = declaration.site.scope.unwrap_or(declaration.site.module);
        let binder_owner = binder
            .namespaces
            .declaration_owner_for_scope(declaration_scope);
        let containing_namespace = declaration_owner_namespace(binder, binder_owner);
        for owner in [
            declaration.type_group.map(ReplayOwner::TypeGroup),
            declaration.value_storage.map(ReplayOwner::Value),
            declaration
                .namespace
                .and_then(|namespace| binder.namespaces.standalone_value_storage(namespace))
                .map(ReplayOwner::Value),
            declaration.namespace.map(ReplayOwner::Namespace),
        ]
        .into_iter()
        .flatten()
        {
            reservations.attach_collision_declaration_site(
                owner,
                file_ordinal,
                declaration,
                binder_owner,
                containing_namespace,
            );
        }
    }

    let _ = program;
}

fn attach_selected_library_decl_owners<Ticket: Copy + PartialEq>(
    reservations: &mut LexicalReservations<Ticket>,
    file_ordinal: LibraryFileOrdinal,
    binder: &Binder,
    sites: &[replay_index::CollisionReplayOwnerSite],
) -> Result<(), &'static str> {
    let mut declarations = BTreeSet::new();
    for site in sites
        .iter()
        .filter(|site| site.file_ordinal == file_ordinal)
    {
        let replay_index::CollisionReplaySiteProvenance::Declaration {
            declaration, kind, ..
        } = site.provenance
        else {
            continue;
        };
        if !declarations.insert(declaration.0) {
            continue;
        }
        let Some(exact) = binder.declarations.get(declaration) else {
            return Err("selected library declaration is absent from the replay binder");
        };
        if exact.kind != kind {
            return Err("selected library declaration provenance changed kind");
        }
        reservations
            .attach_declaration_owner(
                exact.id,
                SourceOrdinal::Library(file_ordinal),
                exact.kind,
                exact.site.declaration_span,
                exact.site.binding_span,
            )
            .map_err(|_| "selected library declaration has no exact lexical owner")?;
    }
    Ok(())
}

fn attach_class_bindings<Ticket: Copy + PartialEq>(
    reservations: &mut LexicalReservations<Ticket>,
    source_ordinal: SourceOrdinal,
    binder: &Binder,
    scope: ScopeId,
    program: &Program<'_>,
    declarations: &TypeDeclTable<'_>,
    selected: Option<&BTreeSet<ReplayOwner>>,
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
                declaration: winning_declaration,
                class_id,
                class_params,
                ..
            } if *winning_declaration == declaration.id => (*class_id, class_params),
            _ => return,
        };
        if selected.is_some_and(|selected| {
            !selected.contains(&ReplayOwner::Class(class_id))
                && !selected.contains(&ReplayOwner::TypeGroup(type_decl))
        }) {
            return;
        }
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
        if let SourceOrdinal::Library(file_ordinal) = source_ordinal {
            let declaration_scope = declaration.site.scope.unwrap_or(declaration.site.module);
            let binder_owner = binder
                .namespaces
                .declaration_owner_for_scope(declaration_scope);
            reservations.attach_collision_declaration_site(
                ReplayOwner::Class(class_id),
                file_ordinal,
                declaration,
                binder_owner,
                declaration_owner_namespace(binder, binder_owner),
            );
        }
    });
}

fn declaration_owner_namespace(
    binder: &Binder,
    owner: crate::binder::namespace::DeclarationOwner,
) -> Option<crate::binder::namespace::NamespaceId> {
    match owner {
        crate::binder::namespace::DeclarationOwner::NamespacePublic(namespace) => Some(namespace),
        crate::binder::namespace::DeclarationOwner::NamespacePrivate(fragment) => binder
            .namespaces
            .fragment(fragment)
            .map(|row| row.namespace),
        _ => None,
    }
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
            if !self.compact_demand_capture.get() {
                if let Some(consumer) = trace.current_owner() {
                    self.compact_only_replay_edges.borrow_mut().remove(
                        &replay_index::ReplayReverseEdge {
                            dependency: owner,
                            consumer,
                        },
                    );
                }
            }
            let added = trace.demand_new(owner);
            if self.compact_demand_capture.get() && added {
                self.compact_demand_added.set(true);
            }
        }
    }

    fn record_replay_demand_at(&self, owner: ReplayOwner, boundary: &'static str) {
        if let Some(trace) = &self.replay_trace {
            if !self.compact_demand_capture.get() {
                if let Some(consumer) = trace.current_owner() {
                    self.compact_only_replay_edges.borrow_mut().remove(
                        &replay_index::ReplayReverseEdge {
                            dependency: owner,
                            consumer,
                        },
                    );
                }
            }
            let added = trace.demand_at_new(owner, boundary);
            if self.compact_demand_capture.get() && added {
                self.compact_demand_added.set(true);
            }
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
                if self.compact_demand_capture.get() {
                    trace.cover_typed_observations();
                } else {
                    trace.demand_root_slot(name, replay_index::RootSlotKind::Type);
                }
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

    pub(in crate::check::checker) fn compact_type_decl_id_replay(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<crate::binder::declaration::TypeGroupId> {
        let consumer = self
            .replay_trace
            .as_ref()
            .and_then(replay_index::ReplayDependencyTrace::current_owner);
        self.compact_demand_capture.set(true);
        self.compact_demand_added.set(false);
        let group = self.type_decl_id_replay(scope, name);
        self.compact_demand_capture.set(false);
        let added = self.compact_demand_added.replace(false);
        let group = group?;
        let Some(consumer) = consumer else {
            return Some(group);
        };
        let edge = replay_index::ReplayReverseEdge {
            dependency: ReplayOwner::TypeGroup(group),
            consumer,
        };
        if consumer != edge.dependency && added {
            self.compact_only_replay_edges.borrow_mut().insert(edge);
        }
        Some(group)
    }

    pub(in crate::check::checker) fn value_decl_id_replay(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<ValueStorageId> {
        let symbol = self.resolve_value_replay(scope, name)?;
        if let Some(winner) = self.private_collision_value_winners.get(&symbol).copied() {
            if self
                .private_collision_affected
                .contains(&ReplayOwner::Value(winner))
            {
                return Some(winner);
            }
        }
        self.binder
            .symbols
            .get(symbol)
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
        let mut effects = CheckerEffects::new(owner);
        effects.activate();
        self.effect_stack.push(effects);
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

    /// Run one call/`new` with a frame that holds its raw argument walks' effects.
    ///
    /// A re-walkable argument is walked twice — once before candidate selection for
    /// its type, once after it with the instantiated contextual target — and only one
    /// of the two may commit. The first walk's effects are held in this frame; the
    /// committed walk marks the ones it superseded, and whatever is left commits when
    /// the frame closes. Every exit path of the call goes through here, so an argument
    /// whose committed walk never runs (no candidate selected, no parameter target,
    /// typed exhaustion) still reports (backlog `92`).
    ///
    /// Each held batch is merged **at most once**, so no `UserRecordTicket` completes
    /// twice and the replay key stays unique (`docs/reference/invariants.md` §1). This is the
    /// sanctioned "candidate effects remain local until exactly one selected set
    /// commits", not deduplication or post-hoc suppression: nothing that has reached
    /// an owner's batch is ever removed.
    ///
    /// This frame is also the argument-walk memos' region (backlog `95`): a nested
    /// argument's walk is reused only within the outermost call it is nested in, and
    /// both tables — plus the declaration observation log that records what a walk
    /// read and wrote — are set up and torn down with it. A memo may not outlive the
    /// walk region whose ambient state its key was sampled from.
    ///
    /// `settle` runs after `produce` on every exit path and before anything is
    /// merged. It is where a raw walk this call served from the memo is re-run if
    /// nothing superseded it, so taking the memo can never be the reason a record does
    /// not exist. Making it a parameter rather than a step inside `produce` is what
    /// keeps that unskippable.
    pub(in crate::check::checker) fn with_provisional_argument_effects<R>(
        &mut self,
        produce: impl FnOnce(&mut Self) -> R,
        settle: impl FnOnce(&mut Self),
    ) -> R {
        self.provisional_argument_effects.push(Vec::new());
        self.argument_walk_depth += 1;
        if self.argument_walk_depth == 1 {
            DeclTypes::open_log();
        }
        let result = produce(self);
        settle(self);
        self.argument_walk_depth -= 1;
        if self.argument_walk_depth == 0 {
            DeclTypes::close_log();
            self.raw_argument_walk_memo.clear();
            self.contextual_walk_memo.clear();
        }
        let held = self
            .provisional_argument_effects
            .pop()
            .expect("provisional argument frame");
        for walk in held {
            match walk {
                ProvisionalArgumentWalk::Held(effects) => self.merge_candidate_effects(effects),
                // `Memoized` is unreachable after `settle`; treating it as nothing to
                // merge keeps the frame total rather than relying on that.
                ProvisionalArgumentWalk::Settled | ProvisionalArgumentWalk::Memoized { .. } => {}
            }
        }
        result
    }

    /// Park one raw argument walk, index-aligned with `arg_types`.
    pub(in crate::check::checker) fn hold_provisional_argument_effects(
        &mut self,
        walk: ProvisionalArgumentWalk<Ticket>,
    ) {
        self.provisional_argument_effects
            .last_mut()
            .expect("a call argument walk runs inside a provisional argument frame")
            .push(walk);
    }

    /// The committed contextual walk re-walked this argument, so the raw walk's
    /// records are the superseded copy and never commit. Out-of-range indices are
    /// argument positions that were never held, and are already committed.
    pub(in crate::check::checker) fn supersede_provisional_argument_effects(
        &mut self,
        index: usize,
    ) {
        let Some(frame) = self.provisional_argument_effects.last_mut() else {
            return;
        };
        let Some(slot) = frame.get_mut(index) else {
            return;
        };
        if let ProvisionalArgumentWalk::Held(effects) =
            std::mem::replace(slot, ProvisionalArgumentWalk::Settled)
        {
            effects.records.discard();
        }
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

    pub(in crate::check::checker) fn install_private_collision_epoch(
        &mut self,
        epoch: Option<library_compiler::PrivateCollisionEpoch>,
    ) {
        let Some(epoch) = epoch else {
            return;
        };
        #[cfg(any(test, feature = "test-utils"))]
        let mut epoch = epoch;
        #[cfg(any(test, feature = "test-utils"))]
        {
            self.private_collision_state_drop_witness = epoch.state_drop_witness.take();
        }
        for site in &epoch.owner_sites {
            let ReplayOwner::Value(owner) = site.owner else {
                continue;
            };
            let entry = self
                .private_library_value_owner_by_site
                .entry((site.file_ordinal, site.span.start))
                .or_insert(Some(owner));
            if *entry != Some(owner) {
                *entry = None;
            }
        }
        self.private_collision_affected = epoch.affected_owners;
        for (name, winner) in epoch.value_winners {
            if !self
                .private_collision_affected
                .contains(&ReplayOwner::Value(winner))
            {
                continue;
            }
            self.private_collision_value_winners_by_name
                .insert(name.clone(), winner);
            let Some(symbol) = self.resolve_value_replay(self.binder.compilation_global, &name)
            else {
                continue;
            };
            if self.binder.symbols.get(symbol).is_some_and(|binding| {
                binding.function_values.len() > 1 && binding.function_values.contains(&winner)
            }) {
                self.function_group_precedence_tails_by_name
                    .insert(name.clone(), winner);
            }
            self.private_collision_value_winners.insert(symbol, winner);
        }
    }

    fn refresh_user_global_object<'program>(
        &mut self,
        units: impl IntoIterator<Item = (ScopeId, &'program Program<'program>, ModuleBindingContext)>,
    ) {
        let mut contributors = Vec::new();
        for (scope, program, context) in units {
            if let Some(storage) =
                direct_global_this_modeled_value_storage(self.binder, scope, program)
            {
                contributors.push(("globalThis".to_string(), storage));
            }
            let census = source_global_binding_census(program, context);
            for (name, candidate) in census.candidates {
                if name == "globalThis" || !candidate.global_object_contributor {
                    continue;
                }
                if let Some(storage) = self.value_decl_id_replay(scope, &name) {
                    contributors.push((name, storage));
                }
            }
        }
        self.refresh_private_global_object(contributors);
    }

    pub(in crate::check::checker) fn refresh_source_global_object_contributor(
        &mut self,
        scope: ScopeId,
        name: &str,
    ) {
        self.with_replay_owner(ReplayOwner::GlobalObject, |pass| {
            let Some(storage) = pass.value_decl_id_replay(scope, name) else {
                return;
            };
            let Some(ty) = pass.decl_type_replay(storage) else {
                return;
            };
            let Some(mut object) = pass
                .global_object_type
                .and_then(|global| pass.interner.store().object_type(global).cloned())
            else {
                return;
            };
            if let Some(property) = object
                .properties
                .iter_mut()
                .find(|property| property.name == name)
            {
                if property.ty == ty {
                    return;
                }
                property.ty = ty;
            } else {
                object.properties.push(PropertyType::public(name, ty));
                object
                    .properties
                    .sort_by(|left, right| left.name.cmp(&right.name));
            }
            pass.global_object_type = Some(pass.interner.intern_object(object));
        });
    }

    fn refresh_private_global_object(&mut self, user_contributors: Vec<(String, ValueStorageId)>) {
        let direct_global_this = user_contributors
            .iter()
            .any(|(name, _)| name == "globalThis");
        if !direct_global_this
            && !self
                .private_collision_affected
                .contains(&ReplayOwner::GlobalObject)
        {
            return;
        }
        let mut object = self
            .global_object_type
            .and_then(|ty| self.interner.store().object_type(ty).cloned())
            .unwrap_or_default();
        let contributors = self
            .private_collision_value_winners_by_name
            .iter()
            .filter(|(name, _)| !direct_global_this || name.as_str() != "globalThis")
            .map(|(name, storage)| (name.clone(), *storage))
            .chain(user_contributors.into_iter().filter(|(name, _)| {
                name == "globalThis"
                    || !self
                        .private_collision_value_winners_by_name
                        .contains_key(name)
            }));
        for (name, storage) in contributors {
            let ty = self.decl_type_replay(storage).or_else(|| {
                let binding = self.class_value_bindings.get(&storage)?;
                match self.published_class_replay(binding.class_id) {
                    DemandOutcome::Ready(surface) => Some(surface.static_template()),
                    DemandOutcome::Exhausted(_) => None,
                }
            });
            let Some(ty) = ty else {
                continue;
            };
            if name == "globalThis" {
                if let Some(overlay) = self.interner.store().object_type(ty).cloned() {
                    object = compose_global_this_value_surface(object, overlay);
                } else if self.interner.store().function_type(ty).is_some() {
                    object.call_signatures.push(ty);
                }
                continue;
            }
            if let Some(property) = object
                .properties
                .iter_mut()
                .find(|property| property.name == name)
            {
                property.ty = ty;
            } else {
                object.properties.push(PropertyType::public(name, ty));
            }
        }
        object
            .properties
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.global_object_type = Some(self.interner.intern_object(object));
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
    let fill_state = |decl: &TypeDecl<'_>| match decl {
        TypeDecl::Interface { .. } => ClassFillState::Pending,
        TypeDecl::Alias {
            object_template: Some(_),
            ..
        } => ClassFillState::Pending,
        _ => ClassFillState::Done,
    };
    let template_fill = type_decls.iter().map(fill_state).collect();
    let mut template_fill = TemplateFillTable::new(published_type_count, template_fill);
    for (index, declaration) in type_decls.changed_entries() {
        if index < published_type_count {
            template_fill.install_replacement(index, fill_state(declaration));
        }
    }
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
        capture_compact_replay_dependencies: false,
        compact_only_replay_edges: std::cell::RefCell::new(BTreeSet::new()),
        compact_demand_capture: std::cell::Cell::new(false),
        compact_demand_added: std::cell::Cell::new(false),
        private_collision_affected: BTreeSet::new(),
        #[cfg(any(test, feature = "test-utils"))]
        private_collision_state_drop_witness: None,
        private_library_value_owner_by_site: BTreeMap::new(),
        private_collision_unavailable_type_groups: BTreeSet::new(),
        combined_user_library_type_groups: BTreeSet::new(),
        combined_user_source: false,
        private_collision_value_winners: FxHashMap::default(),
        private_collision_value_winners_by_name: FxHashMap::default(),
        global_object_type: None,
        effect_stack: Vec::new(),
        provisional_argument_effects: Vec::new(),
        argument_walk_depth: 0,
        raw_argument_walk_memo: FxHashMap::default(),
        contextual_walk_memo: FxHashMap::default(),
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
        function_group_precedence_tails_by_name: FxHashMap::default(),
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

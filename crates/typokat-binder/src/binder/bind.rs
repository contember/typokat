//! AST → scope graph + multi-slot symbols (architecture §4).
//! Declares value/type names, keeps lexical and storage identities separate, and records scopes
//! keyed by `(module scope, span start)` for the checker's reserve-then-fill pass.
//! The checker owns type construction and semantic diagnostics.

use crate::binder::declaration::{
    source_declaration_occurrences, source_global_binding_census_with_provenance, DeclId,
    DeclarationKind, DeclarationSite, DeclarationTable, LexicalDeclaration, TypeFragmentKind,
    TypeGroupFragment, TypeGroupId, TypeGroupTable, ValueStorageId,
};
use crate::binder::namespace::{
    allocate_dormant_namespace_value_storages, collect_project_namespace_metadata, CompilationUnit,
    DeclarationSyntaxFacts, NamespaceId, NamespaceInstanceState, NamespaceTable, SourceUnitKey,
};
use crate::binder::namespace::{
    collect_namespace_metadata, fill_namespace_value_attachments, finalize_namespace_metadata,
    plan_namespace_value_attachments, replay_namespace_value_attachments, NamespaceMetadataRoot,
};
use crate::binder::roots::{LibraryRootContributorSite, LibraryRootProjection};
use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{Symbol, SymbolId, SymbolTable};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use crate::span::Span;
use crate::types::layered::LayeredMap;
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, BlockStatement, Class, ClassElement, Declaration,
    Expression, ForStatement, ForStatementInit, ForStatementLeft, FormalParameters, Function,
    FunctionBody, FunctionType, Program, Statement, SwitchStatement, TSModuleDeclarationName,
    TryStatement, VariableDeclarationKind, VariableDeclarator,
};
use rustc_hash::FxHashMap;
use std::fmt;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static SYMBOL_DECLARATION_ROW_PROBES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TYPE_GROUP_FRAGMENT_ROW_PROBES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Declaration rows [`BindState::attach_symbol_declaration`] reads to place one declaration —
/// every row its duplicate guard compares and every row it orders. Ordering the whole list again
/// per attach makes this grow with declarations x group size, and the list it produces is
/// identical either way, so no output assertion can see the difference. Only a counter can.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SymbolDeclarationAttachWorkForTest {
    pub(crate) row_probes: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn symbol_declaration_attach_work_for_test() -> SymbolDeclarationAttachWorkForTest {
    SymbolDeclarationAttachWorkForTest {
        row_probes: SYMBOL_DECLARATION_ROW_PROBES.get(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_symbol_declaration_row_probe() {
    SYMBOL_DECLARATION_ROW_PROBES.set(SYMBOL_DECLARATION_ROW_PROBES.get() + 1);
}

#[cfg(any(test, feature = "test-utils"))]
pub struct SymbolDeclarationAttachWorkScopeForTest(SymbolDeclarationAttachWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl SymbolDeclarationAttachWorkScopeForTest {
    pub fn start() -> Self {
        Self(symbol_declaration_attach_work_for_test())
    }

    pub fn finish(self) -> SymbolDeclarationAttachWorkForTest {
        let end = symbol_declaration_attach_work_for_test();
        SymbolDeclarationAttachWorkForTest {
            row_probes: end.row_probes.saturating_sub(self.0.row_probes),
        }
    }
}

/// Fragment rows inspected while finding the insertion point for reopened type groups.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeGroupFragmentAttachWorkForTest {
    pub(crate) row_probes: u64,
}

#[cfg(any(test, feature = "test-utils"))]
fn type_group_fragment_attach_work_for_test() -> TypeGroupFragmentAttachWorkForTest {
    TypeGroupFragmentAttachWorkForTest {
        row_probes: TYPE_GROUP_FRAGMENT_ROW_PROBES.get(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn record_type_group_fragment_row_probe() {
    TYPE_GROUP_FRAGMENT_ROW_PROBES.set(TYPE_GROUP_FRAGMENT_ROW_PROBES.get().saturating_add(1));
}

#[cfg(any(test, feature = "test-utils"))]
pub struct TypeGroupFragmentAttachWorkScopeForTest(TypeGroupFragmentAttachWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl TypeGroupFragmentAttachWorkScopeForTest {
    pub fn start() -> Self {
        Self(type_group_fragment_attach_work_for_test())
    }

    pub fn finish(self) -> TypeGroupFragmentAttachWorkForTest {
        let end = type_group_fragment_attach_work_for_test();
        TypeGroupFragmentAttachWorkForTest {
            row_probes: end.row_probes.saturating_sub(self.0.row_probes),
        }
    }
}

/// Which key a symbol's declaration list is ordered by. Library binding orders by exact library
/// file ordinal and every other build by source unit key; the two can disagree, so a list written
/// under one and continued under the other is not sorted for the second and has to be derived
/// again rather than inserted into.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum DeclarationOrder {
    LibraryFileOrdinal,
    SourceUnit,
}

/// The total order a symbol's declaration list is kept in: owning source unit, then declaration
/// span, then binding span, then declaration id. The id makes it total, so no two declarations
/// compare equal and one binary descent finds the exact slot a full re-sort would have used.
type DeclarationOrderKey = (u64, u32, u32, u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryBinderError {
    EmptyBatch,
    NonEmptyPrelude,
    NonLibraryUnit {
        input_index: usize,
    },
    PreludeSourceKey {
        input_index: usize,
    },
    DuplicateSourceKey {
        input_index: usize,
    },
    DuplicateFileOrdinal {
        input_index: usize,
    },
    #[cfg(any(test, feature = "test-utils"))]
    RequiresPristineBuilder,
    AlreadyAdded,
}

impl fmt::Display for LibraryBinderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBatch => formatter.write_str("library binder requires at least one unit"),
            Self::NonEmptyPrelude => {
                formatter.write_str("library batch requires a fresh empty prelude")
            }
            Self::NonLibraryUnit { input_index } => {
                write!(
                    formatter,
                    "library unit {input_index} has a non-library origin"
                )
            }
            Self::PreludeSourceKey { input_index } => {
                write!(
                    formatter,
                    "library source key cannot be the prelude key (unit {input_index})"
                )
            }
            Self::DuplicateSourceKey { input_index } => {
                write!(
                    formatter,
                    "library source key is unique (repeated by unit {input_index})"
                )
            }
            Self::DuplicateFileOrdinal { input_index } => {
                write!(
                    formatter,
                    "library file ordinal is unique (repeated by unit {input_index})"
                )
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::RequiresPristineBuilder => {
                formatter.write_str("library batch requires a pristine project builder")
            }
            Self::AlreadyAdded => formatter.write_str("library batch is one-shot"),
        }
    }
}

impl std::error::Error for LibraryBinderError {}

/// The binder's output for one file: the scope graph, the symbol table, the
/// module scope id, source declarations, and the per-function scope map.
pub struct Binder {
    pub graph: ScopeGraph,
    pub symbols: SymbolTable,
    /// Every admitted source declaration in one unified lexical identity space.
    pub declarations: DeclarationTable,
    /// Ordered same-name type groups used by every production type-space lookup.
    pub type_groups: TypeGroupTable,
    /// Namespace/global/merge metadata and admitted attached value-member identities.
    pub namespaces: NamespaceTable,
    /// The last **user** module scope. Its parent is [`Binder::script_namespace_root`],
    /// then [`Binder::compilation_global`] and [`Binder::prelude_module`].
    pub module: ScopeId,
    /// The **prelude** root scope (M28) — the compilation unit holding the built-in
    /// utility aliases, bound BEFORE the user program. Its parent is `None`.
    pub prelude_module: ScopeId,
    /// The legal project-wide type-side global surface.
    pub compilation_global: ScopeId,
    /// Shared identity owner for direct top-level namespaces in script files.
    pub script_namespace_root: ScopeId,
    /// One-pass source/root projection retained by source-compiled library binders.
    pub library_root_projection: Option<LibraryRootProjection>,
    /// Number of value storage slots (`ValueStorageId`s run
    /// `0..decl_count`). Includes variable bindings, function declaration names,
    /// function parameters, and dormant standalone namespace slots.
    pub decl_count: u32,
    /// Number of type groups bound from the trusted prelude. User groups form the
    /// dense suffix, allowing two immutable publication epochs.
    pub prelude_type_group_count: u32,
    /// Maps a function/arrow node to its parameter scope. Keyed by `(module scope,
    /// span start)` because offsets are unique only within one file (backlog 58).
    pub fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// Maps function declarations to their value declaration id.
    pub fn_decl_ids: FxHashMap<(ScopeId, u32), ValueStorageId>,
    /// Maps a `{ … }` block to its lexical scope (M7), keyed like `fn_scopes` so
    /// branch-local declarations stay local and cross-file offsets do not collide.
    pub block_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// Stable source ownership retained across the frozen-library continuation seam.
    #[cfg_attr(not(test), allow(dead_code))]
    module_sources: LayeredMap<ScopeId, SourceUnitKey>,
    next_source_key: SourceUnitKey,
    /// Writes this build refused because their row lives in the frozen library prefix. The
    /// checker turns each into an incomplete outcome, so a dropped declaration is never silent
    /// (backlog 102).
    frozen_prefix_writes: Vec<FrozenPrefixWrite>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryBinderCheckpointEnds {
    pub scopes: usize,
    pub symbols: usize,
    pub declarations: usize,
    pub type_groups: usize,
    pub namespaces: usize,
    pub value_storages: usize,
    pub next_source: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryBinderUnit {
    pub ordinal: LibraryFileOrdinal,
    pub source: SourceUnitKey,
    pub module: ScopeId,
}

/// One compiled default-library binder, opaque outside the crate.
///
/// Provenance is structural: only `LibraryCompiler::compile_binder_checkpoint` can build one,
/// the fields are private, and it is neither `Clone` nor constructible by a caller.
#[doc(hidden)]
pub struct LibraryBinderCheckpoint {
    binder: Binder,
    library_units: Vec<LibraryBinderUnit>,
    ends: LibraryBinderCheckpointEnds,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct LibraryBinderCheckpointInspectionForTest<'checkpoint> {
    pub library_units: &'checkpoint [LibraryBinderUnit],
    pub ends: LibraryBinderCheckpointEnds,
    pub array_symbol: SymbolId,
    pub array_type_group: TypeGroupId,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct TypeGroupFragmentOrderingUnitForTest {
    pub library_ordinal: LibraryFileOrdinal,
    pub source_key: SourceUnitKey,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeGroupFragmentOrderingIdentityForTest {
    pub source_key: SourceUnitKey,
    pub declaration_start: u32,
    pub declaration: DeclId,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct TypeGroupFragmentOrderingGroupForTest {
    pub name: String,
    pub fragments_in_binder: Vec<TypeGroupFragmentOrderingIdentityForTest>,
    pub fragments_by_library_ordinal: Vec<TypeGroupFragmentOrderingIdentityForTest>,
    pub fragments_by_source_key: Vec<TypeGroupFragmentOrderingIdentityForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
pub struct TypeGroupFragmentOrderingForTest {
    pub units: Vec<TypeGroupFragmentOrderingUnitForTest>,
    pub groups: Vec<TypeGroupFragmentOrderingGroupForTest>,
}

#[cfg(any(test, feature = "test-utils"))]
fn type_group_fragment_ordering_identity_for_test(
    fragment: &TypeGroupFragment,
) -> TypeGroupFragmentOrderingIdentityForTest {
    TypeGroupFragmentOrderingIdentityForTest {
        source_key: fragment.source,
        declaration_start: fragment.site.declaration_span.start,
        declaration: fragment.declaration,
    }
}

impl LibraryBinderCheckpoint {
    pub fn new(binder: Binder, library_units: Vec<LibraryBinderUnit>) -> Self {
        let ends = binder.checkpoint_ends();
        Self {
            binder,
            library_units,
            ends,
        }
    }

    #[doc(hidden)]
    pub fn library_unit_count(&self) -> usize {
        self.library_units.len()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn inspection_for_test(&self) -> LibraryBinderCheckpointInspectionForTest<'_> {
        let binder = &self.binder;
        let array_symbol = binder
            .resolve_type(binder.compilation_global, "Array")
            .expect("canonical library checkpoint contains Array");
        let array_type_group = binder
            .symbols
            .get(array_symbol)
            .and_then(|symbol| symbol.ty)
            .expect("canonical Array owns one type group");
        LibraryBinderCheckpointInspectionForTest {
            library_units: &self.library_units,
            ends: self.ends,
            array_symbol,
            array_type_group,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn type_group_fragment_ordering_for_test(&self) -> TypeGroupFragmentOrderingForTest {
        let library_ordinals = self
            .library_units
            .iter()
            .map(|unit| (unit.module, unit.ordinal))
            .collect::<FxHashMap<_, _>>();
        let mut units = self
            .library_units
            .iter()
            .map(|unit| TypeGroupFragmentOrderingUnitForTest {
                library_ordinal: unit.ordinal,
                source_key: unit.source,
            })
            .collect::<Vec<_>>();
        units.sort_by_key(|unit| unit.library_ordinal);

        let groups = self
            .binder
            .type_groups
            .iter()
            .filter(|group| {
                group
                    .fragments
                    .iter()
                    .all(|fragment| library_ordinals.contains_key(&fragment.site.module))
            })
            .map(|group| {
                let fragments_in_binder = group
                    .fragments
                    .iter()
                    .map(type_group_fragment_ordering_identity_for_test)
                    .collect::<Vec<_>>();
                let mut fragments_by_library_ordinal = group.fragments.clone();
                fragments_by_library_ordinal.sort_by_key(|fragment| {
                    (
                        library_ordinals.get(&fragment.site.module).copied(),
                        fragment.site.declaration_span.start,
                        fragment.declaration.0,
                    )
                });
                let mut fragments_by_source_key = group.fragments.clone();
                fragments_by_source_key.sort_by_key(|fragment| {
                    (
                        fragment.source,
                        fragment.site.declaration_span.start,
                        fragment.declaration.0,
                    )
                });
                TypeGroupFragmentOrderingGroupForTest {
                    name: group.name.clone(),
                    fragments_in_binder,
                    fragments_by_library_ordinal: fragments_by_library_ordinal
                        .iter()
                        .map(type_group_fragment_ordering_identity_for_test)
                        .collect(),
                    fragments_by_source_key: fragments_by_source_key
                        .iter()
                        .map(type_group_fragment_ordering_identity_for_test)
                        .collect(),
                }
            })
            .collect();

        TypeGroupFragmentOrderingForTest { units, groups }
    }

    pub fn checkpoint_ends(&self) -> LibraryBinderCheckpointEnds {
        self.ends
    }

    pub fn into_continuation<'ast>(self) -> (ProjectBinderBuilder<'ast>, Vec<LibraryBinderUnit>) {
        let Self {
            binder,
            library_units,
            ..
        } = self;
        let type_group_library_ordinals = library_units
            .iter()
            .map(|unit| (unit.module, unit.ordinal))
            .collect();
        let Binder {
            graph,
            symbols,
            declarations,
            type_groups,
            namespaces,
            module,
            prelude_module,
            compilation_global,
            script_namespace_root,
            library_root_projection,
            decl_count,
            prelude_type_group_count,
            fn_scopes,
            fn_decl_ids,
            block_scopes,
            module_sources,
            next_source_key,
            frozen_prefix_writes,
        } = binder;
        (
            ProjectBinderBuilder {
                state: BindState {
                    graph,
                    symbols,
                    declarations,
                    type_groups,
                    namespaces,
                    module_sources,
                    library_module_ordinals: FxHashMap::default(),
                    type_group_library_ordinals,
                    fn_scopes,
                    fn_decl_ids,
                    block_scopes,
                    global_overlay_scopes: FxHashMap::default(),
                    placement_syntax: FxHashMap::default(),
                    symbol_declaration_orders: FxHashMap::default(),
                    current_module: module,
                    next_value_storage: decl_count,
                    next_source_key,
                    continuation_publication: None,
                    frozen_prefix_writes,
                },
                prelude_module,
                compilation_global,
                // Nothing is sealed on the checkpoint path, so it keeps its own routing.
                delta_compilation_global: None,
                script_namespace_root,
                library_root_projection,
                prelude_type_group_count,
                use_mode: BuilderUseMode::Continuation,
                empty_prelude: true,
                continuation_publication_plans: FxHashMap::default(),
                pending_namespace_modules: Vec::new(),
            },
            library_units,
        )
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ResolvedValueKind {
    Ordinary,
    StandaloneNamespace {
        namespace: NamespaceId,
        storage: ValueStorageId,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ValueResolution {
    Resolved {
        symbol: SymbolId,
        kind: ResolvedValueKind,
    },
    TypeOnlyNamespace {
        namespace: NamespaceId,
    },
    Missing,
}

impl Binder {
    pub fn source_for_module(&self, module: ScopeId) -> Option<SourceUnitKey> {
        self.module_sources.get(&module).copied()
    }

    /// Exact semantic owners whose frozen binder rows were replaced in a sparse epoch.
    pub fn sparse_prefix_mutation_owners(
        &self,
    ) -> std::collections::BTreeSet<crate::binder::roots::FrozenLibraryMutationOwner> {
        use crate::binder::roots::FrozenLibraryMutationOwner;

        let mut owners = self
            .type_groups
            .prefix_replacements()
            .map(FrozenLibraryMutationOwner::TypeGroup)
            .chain(
                self.namespaces
                    .prefix_namespace_replacements()
                    .map(FrozenLibraryMutationOwner::Namespace),
            )
            .collect::<std::collections::BTreeSet<_>>();
        for symbol in self.symbols.prefix_replacements() {
            owners.extend(symbol.value.map(FrozenLibraryMutationOwner::Value));
            owners.extend(symbol.ty.map(FrozenLibraryMutationOwner::TypeGroup));
            owners.extend(symbol.ns.map(FrozenLibraryMutationOwner::Namespace));
        }
        owners
    }

    pub fn take_library_root_projection(&mut self) -> Option<LibraryRootProjection> {
        self.library_root_projection.take()
    }

    pub fn checkpoint_ends(&self) -> LibraryBinderCheckpointEnds {
        LibraryBinderCheckpointEnds {
            scopes: self.graph.len(),
            symbols: self.symbols.len(),
            declarations: self.declarations.len(),
            type_groups: self.type_groups.len(),
            namespaces: self.namespaces.len(),
            value_storages: usize::try_from(self.decl_count)
                .expect("binder value-storage prefix fits usize"),
            next_source: usize::try_from(self.next_source_key.0)
                .expect("binder source prefix fits usize"),
        }
    }

    pub fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.graph.freeze_as_base()?;
        self.symbols.freeze_as_base()?;
        self.declarations.freeze_as_base()?;
        self.type_groups.freeze_as_base()?;
        self.namespaces.freeze_as_base()?;
        self.module_sources.freeze_as_base()?;
        Ok(())
    }

    pub fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            graph: self.graph.fork_delta()?,
            symbols: self.symbols.fork_delta()?,
            declarations: self.declarations.fork_delta()?,
            type_groups: self.type_groups.fork_delta()?,
            namespaces: self.namespaces.fork_delta()?,
            module: self.module,
            prelude_module: self.prelude_module,
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_root_projection: self.library_root_projection.clone(),
            decl_count: self.decl_count,
            prelude_type_group_count: self.prelude_type_group_count,
            fn_scopes: FxHashMap::default(),
            fn_decl_ids: FxHashMap::default(),
            block_scopes: FxHashMap::default(),
            module_sources: self.module_sources.fork_delta()?,
            next_source_key: self.next_source_key,
            frozen_prefix_writes: Vec::new(),
        })
    }

    pub fn fork_sparse_collision_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            graph: self.graph.fork_sparse_delta()?,
            symbols: self.symbols.fork_sparse_delta()?,
            declarations: self.declarations.fork_sparse_delta()?,
            type_groups: self.type_groups.fork_sparse_delta()?,
            namespaces: self.namespaces.fork_sparse_delta()?,
            module: self.module,
            prelude_module: self.prelude_module,
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_root_projection: self.library_root_projection.clone(),
            decl_count: self.decl_count,
            prelude_type_group_count: self.prelude_type_group_count,
            fn_scopes: FxHashMap::default(),
            fn_decl_ids: FxHashMap::default(),
            block_scopes: FxHashMap::default(),
            module_sources: self.module_sources.fork_delta()?,
            next_source_key: self.next_source_key,
            frozen_prefix_writes: Vec::new(),
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn shares_base_storage_with(&self, other: &Self) -> bool {
        self.graph.scopes_share_base_with(&other.graph)
            && self.symbols.symbols_share_base_with(&other.symbols)
            && self
                .declarations
                .shares_base_storage_with(&other.declarations)
            && self
                .type_groups
                .shares_base_storage_with(&other.type_groups)
            && self.namespaces.shares_base_storage_with(&other.namespaces)
            && self.module_sources.shares_base_with(&other.module_sources)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn base_family_sharing_with(&self, other: &Self) -> [bool; 8] {
        let declarations = self
            .declarations
            .base_family_sharing_with(&other.declarations);
        let namespaces = self.namespaces.base_family_sharing_with(&other.namespaces);
        [
            self.graph.scopes_share_base_with(&other.graph),
            self.symbols.symbols_share_base_with(&other.symbols),
            declarations[0],
            declarations[1],
            self.type_groups
                .shares_base_storage_with(&other.type_groups),
            namespaces[0],
            namespaces[1],
            self.module_sources.shares_base_with(&other.module_sources),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_family_row_counts_for_test(&self) -> [usize; 8] {
        let declarations = self.declarations.local_family_row_counts_for_test();
        let namespaces = self.namespaces.local_family_row_counts_for_test();
        [
            self.graph.local_scopes().count() + self.graph.replacement_row_count_for_test(),
            self.symbols.local_symbols().count() + self.symbols.replacement_row_count_for_test(),
            declarations[0],
            declarations[1],
            self.type_groups.local_row_count_for_test(),
            namespaces[0],
            namespaces[1],
            self.module_sources.local_len() + self.module_sources.replacement_len(),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn max_source_key(&self) -> SourceUnitKey {
        SourceUnitKey(
            self.next_source_key
                .0
                .checked_sub(1)
                .expect("binder retains at least the prelude source key"),
        )
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_names_for_test(&self) -> std::collections::BTreeSet<String> {
        self.graph
            .local_scopes()
            .flat_map(|(_, scope)| scope.symbols.keys().cloned())
            .collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_reference_records_for_test(&self) -> Vec<(u8, u32, u8, u32)> {
        super::references::local_reference_records_for_test(self)
            .into_iter()
            .map(|(owner_domain, target_domain, _, owner, target)| {
                (owner_domain, owner, target_domain, target)
            })
            .collect()
    }

    /// Every write this build had to refuse because its row lives in the frozen library prefix.
    pub fn frozen_prefix_writes(&self) -> &[FrozenPrefixWrite] {
        &self.frozen_prefix_writes
    }

    pub fn module_sources(&self) -> &LayeredMap<ScopeId, SourceUnitKey> {
        &self.module_sources
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[allow(clippy::too_many_arguments)]
    pub fn from_product_parts(
        graph: ScopeGraph,
        symbols: SymbolTable,
        declarations: DeclarationTable,
        type_groups: TypeGroupTable,
        namespaces: NamespaceTable,
        module: ScopeId,
        prelude_module: ScopeId,
        compilation_global: ScopeId,
        script_namespace_root: ScopeId,
        decl_count: u32,
        prelude_type_group_count: u32,
        module_sources: FxHashMap<ScopeId, SourceUnitKey>,
        fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
        fn_decl_ids: FxHashMap<(ScopeId, u32), ValueStorageId>,
        block_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    ) -> Self {
        let next_source_key = SourceUnitKey(
            module_sources
                .values()
                .map(|source| source.0)
                .max()
                .and_then(|source| source.checked_add(1))
                .unwrap_or(1),
        );
        Self {
            graph,
            symbols,
            declarations,
            type_groups,
            namespaces,
            module,
            prelude_module,
            compilation_global,
            script_namespace_root,
            library_root_projection: None,
            decl_count,
            prelude_type_group_count,
            fn_scopes,
            fn_decl_ids,
            block_scopes,
            module_sources: {
                let mut layered = LayeredMap::default();
                for (scope, source) in module_sources {
                    let _ = layered.insert_local(scope, source);
                }
                layered
            },
            next_source_key,
            frozen_prefix_writes: Vec::new(),
        }
    }

    /// Return the canonical semantically admitted declaration at one exact syntax site.
    pub fn exact_declaration_at(
        &self,
        syntax_module: ScopeId,
        binding_start: u32,
        kind: DeclarationKind,
    ) -> Option<&LexicalDeclaration> {
        self.declarations
            .declaration_at_site(syntax_module, binding_start, kind)
            .filter(|declaration| declaration.site.scope.is_some())
    }

    /// Resolve a value binding and its namespace provenance in one scope walk.
    pub fn resolve_value_binding(&self, scope: ScopeId, name: &str) -> ValueResolution {
        self.resolve_value_binding_traced(scope, name, || {})
    }

    pub fn resolve_value_binding_traced(
        &self,
        scope: ScopeId,
        name: &str,
        mut compilation_root_probe: impl FnMut(),
    ) -> ValueResolution {
        resolve_value_binding(
            &self.graph,
            &self.symbols,
            &self.namespaces,
            self.compilation_global,
            scope,
            name,
            &mut compilation_root_probe,
        )
    }

    /// Resolve only the ordinary symbol projection of [`Binder::resolve_value_binding`].
    pub fn resolve_value(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        match self.resolve_value_binding(scope, name) {
            ValueResolution::Resolved { symbol, .. } => Some(symbol),
            ValueResolution::TypeOnlyNamespace { .. } | ValueResolution::Missing => None,
        }
    }

    /// Resolve a type-space binding, skipping same-named value-only symbols while
    /// walking parents.
    pub fn resolve_type(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        self.resolve_type_traced(scope, name, || {})
    }

    pub fn resolve_type_traced(
        &self,
        scope: ScopeId,
        name: &str,
        mut compilation_root_probe: impl FnMut(),
    ) -> Option<SymbolId> {
        resolve_type_symbol(
            &self.graph,
            &self.symbols,
            self.compilation_global,
            scope,
            name,
            &mut compilation_root_probe,
        )
    }
}

fn resolve_value_binding(
    graph: &ScopeGraph,
    symbols: &SymbolTable,
    namespaces: &NamespaceTable,
    compilation_global: ScopeId,
    scope: ScopeId,
    name: &str,
    compilation_root_probe: &mut impl FnMut(),
) -> ValueResolution {
    let mut current = Some(scope);
    let mut type_only_namespace = None;
    while let Some(id) = current {
        if id == compilation_global {
            compilation_root_probe();
        }
        let Some(current_scope) = graph.get(id) else {
            return ValueResolution::Missing;
        };
        for lookup in [Some(id), current_scope.namespace_public] {
            let Some(symbol_id) = lookup
                .and_then(|scope| graph.get(scope))
                .and_then(|scope| scope.lookup_local(name))
            else {
                continue;
            };
            let Some(symbol) = symbols.get(symbol_id) else {
                return ValueResolution::Missing;
            };
            if let Some(storage) = symbol.value {
                let kind = symbol
                    .ns
                    .filter(|namespace| {
                        namespaces.standalone_value_storage(*namespace) == Some(storage)
                    })
                    .map_or(ResolvedValueKind::Ordinary, |namespace| {
                        ResolvedValueKind::StandaloneNamespace { namespace, storage }
                    });
                return ValueResolution::Resolved {
                    symbol: symbol_id,
                    kind,
                };
            }
            if symbol.blocks_value_lookup {
                if let Some(namespace) = symbol.ns.filter(|namespace| {
                    namespaces.aggregate_instance_state(*namespace)
                        == Some(NamespaceInstanceState::NonInstantiated)
                }) {
                    type_only_namespace.get_or_insert(namespace);
                    continue;
                }
                return ValueResolution::Missing;
            }
        }
        current = current_scope.parent;
    }
    type_only_namespace.map_or(ValueResolution::Missing, |namespace| {
        ValueResolution::TypeOnlyNamespace { namespace }
    })
}

fn resolve_type_symbol(
    graph: &ScopeGraph,
    symbols: &SymbolTable,
    compilation_global: ScopeId,
    scope: ScopeId,
    name: &str,
    compilation_root_probe: &mut impl FnMut(),
) -> Option<SymbolId> {
    let mut current = Some(scope);
    while let Some(id) = current {
        if id == compilation_global {
            compilation_root_probe();
        }
        let current_scope = graph.get(id)?;
        if let Some(symbol_id) = current_scope.lookup_local(name) {
            let symbol = symbols.get(symbol_id)?;
            if symbol.ty.is_some() || symbol.blocks_type_lookup {
                return Some(symbol_id);
            }
        }
        if let Some(public) = current_scope.namespace_public {
            let public_scope = graph.get(public)?;
            if let Some(symbol_id) = public_scope.lookup_local(name) {
                let symbol = symbols.get(symbol_id)?;
                if symbol.ty.is_some() || symbol.blocks_type_lookup {
                    return Some(symbol_id);
                }
            }
        }
        current = current_scope.parent;
    }
    None
}

/// Mutable binder state threaded through the recursive walk.
pub struct ImportedSymbol {
    name: String,
    value: Option<ImportedValueSlot>,
    ty: Option<ImportedTypeSlot>,
    value_barrier: bool,
    type_barrier: bool,
    site: Span,
}

impl ImportedSymbol {
    pub fn new(
        name: String,
        value: Option<ValueStorageId>,
        ty: Option<TypeGroupId>,
        value_barrier: bool,
        type_barrier: bool,
        site: Span,
    ) -> Self {
        ImportedSymbol {
            name,
            value: value.map(ImportedValueSlot::Existing),
            ty: ty.map(ImportedTypeSlot::Existing),
            value_barrier,
            type_barrier,
            site,
        }
    }

    pub fn placeholder_type(name: String, site: Span) -> Self {
        ImportedSymbol {
            name,
            value: None,
            ty: None,
            value_barrier: false,
            type_barrier: true,
            site,
        }
    }

    pub fn placeholder_value_and_type(name: String, site: Span) -> Self {
        ImportedSymbol {
            name,
            value: Some(ImportedValueSlot::Placeholder),
            ty: None,
            value_barrier: false,
            type_barrier: true,
            site,
        }
    }
}

pub(crate) enum ImportedValueSlot {
    Existing(ValueStorageId),
    Placeholder,
}

pub(crate) enum ImportedTypeSlot {
    Existing(TypeGroupId),
}

pub struct ImportPlaceholder {
    pub value: Option<ValueStorageId>,
}

pub(crate) struct BindState {
    pub(crate) graph: ScopeGraph,
    pub(crate) symbols: SymbolTable,
    pub(crate) declarations: DeclarationTable,
    pub(crate) type_groups: TypeGroupTable,
    pub(crate) namespaces: NamespaceTable,
    /// Stable source ownership for every module scope, including the prelude.
    module_sources: LayeredMap<ScopeId, SourceUnitKey>,
    next_source_key: SourceUnitKey,
    library_module_ordinals: FxHashMap<ScopeId, LibraryFileOrdinal>,
    /// Library provenance needed to keep fragment order canonical after binding mode changes.
    type_group_library_ordinals: FxHashMap<ScopeId, LibraryFileOrdinal>,
    fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    fn_decl_ids: FxHashMap<(ScopeId, u32), ValueStorageId>,
    /// Per-block lexical scopes (M7), keyed by `(module scope, block span start)`.
    block_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// Per-augmentation lexical overlays, shared by the declaration and metadata passes.
    global_overlay_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// Syntax facts of the placement row each declaration bound by *this* build owns.
    /// Build-local on purpose: the readers only ever ask about the local placement
    /// delta, so a sealed base contributes no entry — exactly as the scan it replaces.
    pub(super) placement_syntax: FxHashMap<DeclId, DeclarationSyntaxFacts>,
    /// The key each symbol's declaration list is currently ordered by. Build-local on purpose:
    /// a row this build has not attached to yet is derived once on its first attach, which is
    /// exactly what the fork and frozen-continuation seams need, and a build that
    /// switches key pays one derivation per row it touches afterwards.
    symbol_declaration_orders: FxHashMap<SymbolId, DeclarationOrder>,
    /// The module scope currently being bound — the disambiguating half of the
    /// scope-map keys (backlog 58). Set before each module's body is walked.
    pub(crate) current_module: ScopeId,
    /// Running checker storage counter for value declarations.
    pub(crate) next_value_storage: u32,
    continuation_publication: Option<ContinuationPublicationPlan>,
    /// Writes this build had to refuse because their row belongs to the frozen library prefix,
    /// in binder order. Recorded rather than dropped so the outcome stops being silence
    /// (backlog 102); making the merge itself work is backlog 103.
    frozen_prefix_writes: Vec<FrozenPrefixWrite>,
}

/// One binder write refused by the frozen library prefix (ADR-0011 forbids mutating a base row).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FrozenPrefixWrite {
    /// The user declaration whose effect was refused, when the site owns one.
    pub declaration: Option<DeclId>,
    pub site: FrozenPrefixWriteSite,
}

/// Which binder write a [`FrozenPrefixWrite`] refused.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FrozenPrefixWriteSite {
    /// Publishing a name into a frozen scope (`ScopeGraph::declare`).
    ScopeDeclaration,
    /// Appending a declaration to a frozen symbol's declaration list.
    SymbolDeclarationList,
    /// Rebinding a frozen symbol's value slot.
    ValueSlot,
    /// Rebinding a frozen symbol's function-value slots.
    FunctionValueSlot,
    /// Rebinding a frozen symbol's import slots.
    ImportSlot,
    /// Appending a source fragment to a frozen type group.
    TypeGroupFragment,
    /// Rebinding a frozen symbol's type slot.
    TypeSlot,
    /// Appending a fragment to a frozen namespace.
    NamespaceFragmentList,
}

impl BindState {
    pub(super) fn record_frozen_prefix_write(
        &mut self,
        declaration: Option<DeclId>,
        site: FrozenPrefixWriteSite,
    ) {
        self.frozen_prefix_writes
            .push(FrozenPrefixWrite { declaration, site });
    }

    fn continuation_publication_scope(
        &self,
        lexical_scope: ScopeId,
        binding_start: u32,
    ) -> ScopeId {
        let lexical_publication_scope = self
            .graph
            .get(lexical_scope)
            .filter(|scope| scope.kind == ScopeKind::GlobalOverlay)
            .and_then(|scope| scope.namespace_public)
            .unwrap_or(lexical_scope);
        self.continuation_publication
            .as_ref()
            .filter(|plan| plan.binding_sites.contains_key(&binding_start))
            .map_or(lexical_publication_scope, |plan| plan.compilation_global)
    }

    pub(super) fn global_augmentation_overlay_scope(
        &mut self,
        lexical_parent: ScopeId,
        binding_start: u32,
        publication_scope: ScopeId,
    ) -> ScopeId {
        let key = (self.current_module, binding_start);
        if let Some(scope) = self.global_overlay_scopes.get(&key) {
            return *scope;
        }
        let overlay = self.graph.push(
            Scope::new(ScopeKind::GlobalOverlay, Some(lexical_parent))
                .with_namespace_public(publication_scope),
        );
        self.global_overlay_scopes.insert(key, overlay);
        overlay
    }

    fn declaration_publication_scope(
        &self,
        lexical_scope: ScopeId,
        declaration: DeclId,
    ) -> ScopeId {
        let binding_start = self
            .declarations
            .get(declaration)
            .expect("publication declaration exists")
            .site
            .binding_span
            .start;
        self.continuation_publication_scope(lexical_scope, binding_start)
    }

    pub(super) fn continuation_compilation_global_for(
        &self,
        binding_start: u32,
    ) -> Option<ScopeId> {
        self.continuation_publication
            .as_ref()
            .filter(|plan| plan.binding_sites.contains_key(&binding_start))
            .map(|plan| plan.compilation_global)
    }

    pub(super) fn continuation_default_global_scope(&self, fallback: ScopeId) -> ScopeId {
        self.continuation_publication
            .as_ref()
            .map_or(fallback, |plan| plan.default_global_scope)
    }

    pub(super) fn continuation_active(&self) -> bool {
        self.continuation_publication.is_some()
    }

    pub(super) fn continuation_publication_sites(&self) -> Vec<(u32, String)> {
        self.continuation_publication
            .as_ref()
            .map(|plan| {
                plan.binding_sites
                    .iter()
                    .map(|(start, name)| (*start, name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn record_module_source(&mut self, module: ScopeId, source: SourceUnitKey) {
        self.module_sources
            .insert_local(module, source)
            .expect("module source ownership is unique");
        if source.0 >= self.next_source_key.0 {
            self.next_source_key =
                SourceUnitKey(source.0.checked_add(1).expect("source key suffix fits u32"));
        }
    }

    fn record_source_declarations(&mut self, program: &Program<'_>) {
        for occurrence in source_declaration_occurrences(program) {
            let site = DeclarationSite {
                module: self.current_module,
                scope: None,
                declaration_span: occurrence.declaration_span,
                binding_span: occurrence.binding_span,
            };
            self.declarations.push(occurrence.kind, site);
        }
    }

    pub(crate) fn source_decl_at(&self, span_start: u32, kind: DeclarationKind) -> Option<DeclId> {
        self.declarations
            .declaration_at_site(self.current_module, span_start, kind)
            .map(|declaration| declaration.id)
    }

    pub(crate) fn attach_declaration_scope(
        &mut self,
        span_start: u32,
        kind: DeclarationKind,
        scope: ScopeId,
    ) -> DeclId {
        let declaration = self
            .source_decl_at(span_start, kind)
            .expect("semantic binding attaches to a source-prewalk declaration");
        let site = &mut self
            .declarations
            .get_mut(declaration)
            .expect("source declaration exists")
            .site;
        match site.scope {
            Some(existing) => assert_eq!(existing, scope, "declaration scope is stable"),
            None => site.scope = Some(scope),
        }
        declaration
    }

    fn attach_pattern_scope(
        &mut self,
        pattern: &BindingPattern<'_>,
        kind: DeclarationKind,
        scope: ScopeId,
    ) {
        for identifier in pattern.get_binding_identifiers() {
            self.attach_declaration_scope(identifier.span.start, kind, scope);
        }
    }

    pub(super) fn fresh_value_storage(&mut self) -> ValueStorageId {
        let id = ValueStorageId(self.next_value_storage);
        self.next_value_storage += 1;
        id
    }

    fn attach_value_storage(&mut self, declaration: DeclId, storage: ValueStorageId) {
        self.declarations
            .get_mut(declaration)
            .expect("fresh lexical declaration exists")
            .value_storage = Some(storage);
    }

    /// Append `declaration` to `symbol`'s ordered declaration list.
    ///
    /// That list *is* the merge order — it decides which declaration wins and therefore the
    /// diagnostic text — so the vector this leaves behind must be exactly the one a full re-sort
    /// would leave. It already is sorted, so one binary descent puts the declaration where a
    /// stable sort of the appended list would have: after every row ordering at or before it
    /// (backlog 88). Only a row whose key changed under it is derived again.
    pub(crate) fn attach_symbol_declaration(&mut self, symbol: SymbolId, declaration: DeclId) {
        // A frozen symbol row cannot take the declaration; refuse rather than drop (backlog 102).
        if self.symbols.get_mut(symbol).is_none() {
            self.record_frozen_prefix_write(
                Some(declaration),
                FrozenPrefixWriteSite::SymbolDeclarationList,
            );
            return;
        }
        // The first declaration of a name needs no order at all, and most names never take a
        // second one, so the ordered path starts only once the row already holds something.
        let attached = self
            .symbols
            .get(symbol)
            .map_or(0, |row| row.declarations.len());
        if attached == 0 {
            if let Some(row) = self.symbols.get_mut(symbol) {
                row.declarations.push(declaration);
            }
            return;
        }
        let ordering = if self.library_module_ordinals.is_empty() {
            DeclarationOrder::SourceUnit
        } else {
            DeclarationOrder::LibraryFileOrdinal
        };
        // A one-row list is ordered under every key. A longer one is only insertable if *this*
        // key ordered it: a row restored across a fork seam, or one a library batch
        // left behind before the key changed, pays one derivation and is inserted into after.
        let sorted =
            attached == 1 || self.symbol_declaration_orders.get(&symbol).copied() == Some(ordering);
        let order_key = |id: DeclId| -> DeclarationOrderKey {
            #[cfg(any(test, feature = "test-utils"))]
            record_symbol_declaration_row_probe();
            self.declarations
                .get(id)
                .map(|declaration| {
                    let unit = match ordering {
                        DeclarationOrder::LibraryFileOrdinal => u64::try_from(
                            self.library_module_ordinals
                                .get(&declaration.site.module)
                                .copied()
                                .expect("library declaration has an exact file ordinal")
                                .index(),
                        )
                        .unwrap_or(u64::MAX),
                        DeclarationOrder::SourceUnit => u64::from(
                            self.module_sources
                                .get(&declaration.site.module)
                                .copied()
                                .unwrap_or(SourceUnitKey(u32::MAX))
                                .0,
                        ),
                    };
                    (
                        unit,
                        declaration.site.declaration_span.start,
                        declaration.site.binding_span.start,
                        declaration.id.0,
                    )
                })
                .unwrap_or((u64::MAX, u32::MAX, u32::MAX, u32::MAX))
        };
        let key = order_key(declaration);
        // Set only where the list below actually ends up ordered by `ordering`. A derivation the
        // duplicate guard skipped leaves the row under its previous key, exactly as before.
        let mut derived = attached == 1;
        if let Some(row) = self.symbols.get_mut(symbol) {
            if sorted {
                let at = row.declarations.partition_point(|id| order_key(*id) <= key);
                // Only the run sharing this key can already hold the declaration, and the id in
                // the key keeps that run one row long for every declaration the table still has.
                let mut run = at;
                let mut already_listed = false;
                while let Some(&candidate) = run
                    .checked_sub(1)
                    .and_then(|index| row.declarations.get(index))
                {
                    if order_key(candidate) != key {
                        break;
                    }
                    if candidate == declaration {
                        already_listed = true;
                        break;
                    }
                    run -= 1;
                }
                if !already_listed {
                    row.declarations.insert(at, declaration);
                }
            } else if !row.declarations.iter().any(|id| {
                #[cfg(any(test, feature = "test-utils"))]
                record_symbol_declaration_row_probe();
                *id == declaration
            }) {
                row.declarations.push(declaration);
                row.declarations.sort_by_key(|id| order_key(*id));
                derived = true;
            }
        }
        if derived {
            self.symbol_declaration_orders.insert(symbol, ordering);
        }
    }
}

/// Build the scope graph and symbol table for the **prelude + user** pair (M28).
/// The prelude binds first and becomes the user module's parent, giving normal
/// shadowing without duplicate-name diagnostics. Each unit still declares all
/// top-level type names before bodies for the checker's reserve-then-fill pass.
pub fn bind_module_with_prelude(prelude: &Program<'_>, program: &Program<'_>) -> Binder {
    let mut builder = ProjectBinderBuilder::new(prelude);
    let unit = CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, program);
    let (module, _) = builder.add_module(program, &[], unit);
    builder.finish(module)
}

/// Incremental binder for one serial project graph (M29 slice 1).
pub struct ProjectBinderBuilder<'ast> {
    state: BindState,
    prelude_module: ScopeId,
    compilation_global: ScopeId,
    /// Where a **script** file's top-level declarations publish while continuing a frozen
    /// library. The prefix `compilation_global` is sealed, so the user run needs its own
    /// writable global scope layered above it (backlog 102). `None` outside a continuation.
    delta_compilation_global: Option<ScopeId>,
    script_namespace_root: ScopeId,
    prelude_type_group_count: u32,
    use_mode: BuilderUseMode,
    empty_prelude: bool,
    continuation_publication_plans: FxHashMap<SourceUnitKey, ContinuationPublicationPlan>,
    /// Modules collected but not yet classified. Only `finish`/`finish_frozen_library_continuation`
    /// drain this, so a project cannot reach a `Binder` without exactly one classification pass.
    pending_namespace_modules: Vec<PendingNamespaceModule<'ast>>,
    library_root_projection: Option<LibraryRootProjection>,
}

/// One collected module awaiting the project-wide classification pass.
struct PendingNamespaceModule<'ast> {
    module: ScopeId,
    program: &'ast Program<'ast>,
    /// Restored for this module's fill so continuation publication stays per-module.
    publication: Option<ContinuationPublicationPlan>,
}

struct ContinuationPublicationPlan {
    compilation_global: ScopeId,
    default_global_scope: ScopeId,
    binding_sites: FxHashMap<u32, String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum BuilderUseMode {
    Pristine,
    #[cfg(any(test, feature = "test-utils"))]
    Project,
    Library,
    Continuation,
}

impl<'ast> ProjectBinderBuilder<'ast> {
    /// Bind the prelude first so its checker storage keeps the low id ranges.
    pub fn new(prelude: &Program<'_>) -> Self {
        let mut state = BindState {
            graph: ScopeGraph::new(),
            symbols: SymbolTable::new(),
            declarations: DeclarationTable::default(),
            type_groups: TypeGroupTable::default(),
            namespaces: NamespaceTable::default(),
            module_sources: LayeredMap::default(),
            library_module_ordinals: FxHashMap::default(),
            type_group_library_ordinals: FxHashMap::default(),
            fn_scopes: FxHashMap::default(),
            fn_decl_ids: FxHashMap::default(),
            block_scopes: FxHashMap::default(),
            global_overlay_scopes: FxHashMap::default(),
            placement_syntax: FxHashMap::default(),
            symbol_declaration_orders: FxHashMap::default(),
            current_module: ScopeId(0),
            next_value_storage: 0,
            next_source_key: SourceUnitKey(1),
            continuation_publication: None,
            frozen_prefix_writes: Vec::new(),
        };

        let prelude_module = state.graph.push(Scope::new(ScopeKind::Module, None));
        state.current_module = prelude_module;
        state.record_module_source(prelude_module, SourceUnitKey::PRELUDE);
        state.record_source_declarations(prelude);
        bind_statements(&mut state, prelude_module, &prelude.body);
        let prelude_type_group_count =
            u32::try_from(state.type_groups.len()).expect("prelude type group count fits u32");
        let compilation_global = state.graph.push(Scope::new(
            ScopeKind::CompilationGlobal,
            Some(prelude_module),
        ));
        let script_namespace_root = state.graph.push(Scope::new(
            ScopeKind::ScriptNamespaceRoot,
            Some(compilation_global),
        ));

        ProjectBinderBuilder {
            state,
            prelude_module,
            compilation_global,
            delta_compilation_global: None,
            script_namespace_root,
            prelude_type_group_count,
            use_mode: BuilderUseMode::Pristine,
            empty_prelude: prelude.body.is_empty(),
            continuation_publication_plans: FxHashMap::default(),
            pending_namespace_modules: Vec::new(),
            library_root_projection: None,
        }
    }

    pub fn reserve_script_namespace_roots<'a>(
        &mut self,
        units: impl IntoIterator<Item = (&'a Program<'a>, CompilationUnit)>,
    ) {
        #[cfg(any(test, feature = "test-utils"))]
        match self.use_mode {
            BuilderUseMode::Pristine | BuilderUseMode::Project => {
                self.use_mode = BuilderUseMode::Project;
            }
            BuilderUseMode::Library => {
                panic!("project modules cannot follow the one-shot library batch")
            }
            BuilderUseMode::Continuation => {}
        }
        let mut roots = Vec::new();
        for (program, unit) in units {
            if self.use_mode == BuilderUseMode::Continuation {
                let binding_sites =
                    source_global_binding_census_with_provenance(program, unit.binding)
                        .binding_sites
                        .into_iter()
                        .filter(|site| {
                            site.publishable
                                && (self
                                    .state
                                    .graph
                                    .get(self.compilation_global)
                                    .and_then(|scope| scope.lookup_local(&site.name))
                                    .is_some()
                                    || self
                                        .state
                                        .namespaces
                                        .is_admitted_compilation_global_name(&site.name)
                                    || site.name == "globalThis")
                        })
                        .map(|site| (site.span.start, site.name))
                        .collect();
                let default_global_scope = match self.delta_compilation_global {
                    Some(scope) => scope,
                    None => self.compilation_global,
                };
                let previous = self.continuation_publication_plans.insert(
                    unit.source,
                    ContinuationPublicationPlan {
                        compilation_global: self.compilation_global,
                        default_global_scope,
                        binding_sites,
                    },
                );
                assert!(previous.is_none(), "one continuation plan per source");
            }
            if unit.binding.external_module {
                continue;
            }
            let mut occupied_values = rustc_hash::FxHashSet::default();
            for statement in &program.body {
                match statement {
                    Statement::VariableDeclaration(declaration) => {
                        for declarator in &declaration.declarations {
                            for identifier in declarator.id.get_binding_identifiers() {
                                occupied_values.insert(identifier.name.to_string());
                            }
                        }
                    }
                    Statement::FunctionDeclaration(function) => {
                        if let Some(identifier) = &function.id {
                            occupied_values.insert(identifier.name.to_string());
                        }
                    }
                    Statement::ClassDeclaration(class) => {
                        if let Some(identifier) = &class.id {
                            occupied_values.insert(identifier.name.to_string());
                        }
                    }
                    _ => {}
                }
            }
            for statement in &program.body {
                let Statement::TSModuleDeclaration(namespace) = statement else {
                    continue;
                };
                let TSModuleDeclarationName::Identifier(identifier) = &namespace.id else {
                    continue;
                };
                if !occupied_values.contains(identifier.name.as_str()) {
                    roots.push((
                        unit.source,
                        identifier.name.to_string(),
                        identifier.span.start,
                    ));
                }
            }
        }
        roots.sort_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then(left.0.cmp(&right.0))
                .then(left.2.cmp(&right.2))
        });
        roots.dedup_by(|left, right| left.1 == right.1);
        for (source, name, binding_start) in roots {
            let reservation_scope = match self.continuation_publication_plans.get(&source) {
                Some(plan) if plan.binding_sites.contains_key(&binding_start) => continue,
                Some(plan) => plan.default_global_scope,
                None => self.script_namespace_root,
            };
            if self
                .state
                .graph
                .get(reservation_scope)
                .and_then(|scope| scope.lookup_local(&name))
                .is_some()
            {
                continue;
            }
            let symbol = self.state.symbols.push(Symbol::new(name.clone()));
            if self
                .state
                .graph
                .declare(reservation_scope, &name, symbol)
                .is_err()
            {
                self.state
                    .record_frozen_prefix_write(None, FrozenPrefixWriteSite::ScopeDeclaration);
            }
        }
    }

    /// Add one project module. Imported symbols are declared before local names so
    /// declarations in this file can reference imports during reserve/fill. Namespace
    /// classification is deferred to the finish that drains `pending_namespace_modules`.
    pub fn add_module(
        &mut self,
        program: &'ast Program<'ast>,
        imports: &[ImportedSymbol],
        unit: CompilationUnit,
    ) -> (ScopeId, Vec<ImportPlaceholder>) {
        #[cfg(any(test, feature = "test-utils"))]
        match self.use_mode {
            BuilderUseMode::Pristine | BuilderUseMode::Project => {
                self.use_mode = BuilderUseMode::Project;
            }
            BuilderUseMode::Library => {
                panic!("project modules cannot follow the one-shot library batch")
            }
            BuilderUseMode::Continuation => {}
        }
        // Every continuation module hangs off the delta global layer, so a script global one
        // file publishes is visible from the next file regardless of input order (backlog 102).
        let module_parent = match self.delta_compilation_global {
            Some(_) => self.script_namespace_root,
            None => self.prelude_module,
        };
        let module = self
            .state
            .graph
            .push(Scope::new(ScopeKind::Module, Some(module_parent)));
        self.state.current_module = module;
        self.state.record_module_source(module, unit.source);
        self.state.record_source_declarations(program);
        let mut placeholders = Vec::new();
        for import in imports {
            placeholders.push(declare_import(&mut self.state, module, import));
        }
        if self.use_mode == BuilderUseMode::Continuation {
            self.state.continuation_publication = Some(
                self.continuation_publication_plans
                    .remove(&unit.source)
                    .expect("continuation source has one publication plan"),
            );
            // A script file contributes to the project-wide global scope — the same rule the
            // library batch applies to its own script files, one layer up.
            let ordinary_scope = match self.delta_compilation_global {
                Some(delta) if !unit.binding.external_module => delta,
                _ => module,
            };
            bind_library_statements(
                &mut self.state,
                ordinary_scope,
                &program.body,
                unit,
                self.compilation_global,
            );
            collect_project_namespace_metadata(
                &mut self.state,
                module,
                program,
                unit,
                self.compilation_global,
                self.script_namespace_root,
                self.delta_compilation_global,
            );
        } else {
            bind_statements(&mut self.state, module, &program.body);
            collect_project_namespace_metadata(
                &mut self.state,
                module,
                program,
                unit,
                self.compilation_global,
                self.script_namespace_root,
                None,
            );
        }
        // The attachment fill publishes through the same plan, so it travels with the module
        // instead of being dropped at the end of this call.
        let publication = self.state.continuation_publication.take();
        self.pending_namespace_modules.push(PendingNamespaceModule {
            module,
            program,
            publication,
        });
        (module, placeholders)
    }

    /// Classify the accumulated project once, plan its value attachments once, then fill the
    /// ones each module declares — the shape `try_add_library_modules` already uses. Draining
    /// here is what makes a missed classification impossible: `Binder` is only reachable
    /// through a finish.
    fn finalize_pending_namespaces(&mut self) {
        if self.pending_namespace_modules.is_empty() {
            return;
        }
        finalize_namespace_metadata(&mut self.state);
        let plan = plan_namespace_value_attachments(&self.state);
        for pending in std::mem::take(&mut self.pending_namespace_modules) {
            self.state.current_module = pending.module;
            self.state.continuation_publication = pending.publication;
            fill_namespace_value_attachments(&mut self.state, pending.program, &plan);
            self.state.continuation_publication = None;
        }
        replay_namespace_value_attachments(&mut self.state, &plan);
    }

    /// Bind declaration-library files into one shared global identity domain.
    pub fn try_add_library_modules(
        &mut self,
        units: &[(&'ast Program<'ast>, CompilationUnit)],
    ) -> Result<Vec<ScopeId>, LibraryBinderError> {
        if units.is_empty() {
            return Err(LibraryBinderError::EmptyBatch);
        }
        if !self.empty_prelude {
            return Err(LibraryBinderError::NonEmptyPrelude);
        }
        let mut sources = rustc_hash::FxHashSet::default();
        let mut origins = rustc_hash::FxHashSet::default();
        let mut canonical_units = Vec::with_capacity(units.len());
        for (input_index, (program, unit)) in units.iter().enumerate() {
            let CompilationOrigin::Library(file_ordinal) = unit.origin else {
                return Err(LibraryBinderError::NonLibraryUnit { input_index });
            };
            if unit.source == SourceUnitKey::PRELUDE {
                return Err(LibraryBinderError::PreludeSourceKey { input_index });
            }
            if !sources.insert(unit.source) {
                return Err(LibraryBinderError::DuplicateSourceKey { input_index });
            }
            if !origins.insert(unit.origin) {
                return Err(LibraryBinderError::DuplicateFileOrdinal { input_index });
            }
            canonical_units.push((file_ordinal, input_index, *program, *unit));
        }
        canonical_units.sort_by_key(|(file_ordinal, _, _, _)| *file_ordinal);
        match self.use_mode {
            BuilderUseMode::Pristine => self.use_mode = BuilderUseMode::Library,
            #[cfg(any(test, feature = "test-utils"))]
            BuilderUseMode::Project => return Err(LibraryBinderError::RequiresPristineBuilder),
            BuilderUseMode::Library | BuilderUseMode::Continuation => {
                return Err(LibraryBinderError::AlreadyAdded);
            }
        }

        let mut bound_units = Vec::with_capacity(canonical_units.len());
        let projection = self
            .library_root_projection
            .get_or_insert_with(LibraryRootProjection::default);
        projection.canonical_unit_count = canonical_units.len();
        for (file_ordinal, input_index, program, unit) in canonical_units {
            let module = self
                .state
                .graph
                .push(Scope::new(ScopeKind::Module, Some(self.prelude_module)));
            self.state.current_module = module;
            self.state.record_module_source(module, unit.source);
            self.state
                .library_module_ordinals
                .insert(module, file_ordinal);
            self.state
                .type_group_library_ordinals
                .insert(module, file_ordinal);
            self.state.record_source_declarations(program);
            let provenance = source_global_binding_census_with_provenance(program, unit.binding);
            projection.source_census_count = projection.source_census_count.saturating_add(1);
            projection.uncertain_candidate_count = projection
                .uncertain_candidate_count
                .saturating_add(provenance.census.uncertain_candidates.len());
            projection.uncertain_relevant_syntax_count = projection
                .uncertain_relevant_syntax_count
                .saturating_add(usize::from(provenance.census.uncertain_relevant_syntax));
            projection.explicit_global_this |= provenance.census.explicit_global_this;
            for (name, candidate) in provenance.census.candidates {
                let aggregate = projection.candidates.entry(name).or_default();
                aggregate.slots.extend(candidate.slots);
                aggregate.global_object_contributor |= candidate.global_object_contributor;
            }
            projection
                .contributor_sites
                .extend(provenance.contributor_sites.into_iter().map(|site| {
                    LibraryRootContributorSite {
                        name: site.name,
                        kind: site.kind,
                        file_ordinal,
                        span: site.span,
                    }
                }));
            projection.explicit_global_this_sites.extend(
                provenance
                    .explicit_global_this_sites
                    .into_iter()
                    .map(|span| (file_ordinal, span)),
            );
            let ordinary_scope = if unit.binding.external_module {
                module
            } else {
                self.compilation_global
            };
            bind_library_statements(
                &mut self.state,
                ordinary_scope,
                &program.body,
                unit,
                self.compilation_global,
            );
            collect_namespace_metadata(
                &mut self.state,
                module,
                program,
                unit,
                self.compilation_global,
                self.script_namespace_root,
                NamespaceMetadataRoot::LibrarySharedGlobal,
            );
            bound_units.push((input_index, program, module));
        }
        finalize_namespace_metadata(&mut self.state);
        let plan = plan_namespace_value_attachments(&self.state);
        for (_, program, module) in &bound_units {
            self.state.current_module = *module;
            fill_namespace_value_attachments(&mut self.state, program, &plan);
        }
        replay_namespace_value_attachments(&mut self.state, &plan);
        bound_units.sort_by_key(|(input_index, _, _)| *input_index);
        Ok(bound_units
            .into_iter()
            .map(|(_, _, module)| module)
            .collect())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn add_library_modules(
        &mut self,
        units: &[(&'ast Program<'ast>, CompilationUnit)],
    ) -> Vec<ScopeId> {
        self.try_add_library_modules(units)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn finish(mut self, module: ScopeId) -> Binder {
        self.finalize_pending_namespaces();
        allocate_dormant_namespace_value_storages(&mut self.state);
        let root_rows = self.state.namespaces.finalize_global_scopes(
            &mut self.state.graph,
            &mut self.state.symbols,
            &mut self.state.frozen_prefix_writes,
            self.library_root_projection.as_mut(),
        );
        if let Some(projection) = self.library_root_projection.as_mut() {
            projection.root_rows = root_rows;
        }
        Binder {
            graph: self.state.graph,
            symbols: self.state.symbols,
            declarations: self.state.declarations,
            type_groups: self.state.type_groups,
            namespaces: self.state.namespaces,
            module,
            prelude_module: self.prelude_module,
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_root_projection: self.library_root_projection,
            decl_count: self.state.next_value_storage,
            prelude_type_group_count: self.prelude_type_group_count,
            fn_scopes: self.state.fn_scopes,
            fn_decl_ids: self.state.fn_decl_ids,
            block_scopes: self.state.block_scopes,
            module_sources: self.state.module_sources,
            next_source_key: self.state.next_source_key,
            frozen_prefix_writes: self.state.frozen_prefix_writes,
        }
    }

    /// Resume one AST-free library binder for a single user suffix.
    pub fn resume_frozen_library(binder: Binder) -> (Self, SourceUnitKey) {
        let next_source = binder.next_source_key;
        let Binder {
            mut graph,
            symbols,
            declarations,
            type_groups,
            namespaces,
            module,
            prelude_module,
            compilation_global,
            script_namespace_root,
            library_root_projection,
            decl_count,
            prelude_type_group_count: _,
            fn_scopes: _,
            fn_decl_ids: _,
            block_scopes: _,
            module_sources,
            next_source_key,
            frozen_prefix_writes,
        } = binder;
        // The frozen chain below is unwritable, so the user suffix gets its own global layer
        // above it; script files publish here instead of into the sealed prefix (backlog 102).
        let delta_compilation_global = graph.push(Scope::new(
            ScopeKind::CompilationGlobal,
            Some(script_namespace_root),
        ));
        let delta_script_namespace_root = graph.push(Scope::new(
            ScopeKind::ScriptNamespaceRoot,
            Some(delta_compilation_global),
        ));
        let prelude_type_group_count =
            u32::try_from(type_groups.len()).expect("frozen type group count fits u32");
        (
            Self {
                state: BindState {
                    graph,
                    symbols,
                    declarations,
                    type_groups,
                    namespaces,
                    module_sources,
                    library_module_ordinals: FxHashMap::default(),
                    type_group_library_ordinals: FxHashMap::default(),
                    fn_scopes: FxHashMap::default(),
                    fn_decl_ids: FxHashMap::default(),
                    block_scopes: FxHashMap::default(),
                    global_overlay_scopes: FxHashMap::default(),
                    placement_syntax: FxHashMap::default(),
                    symbol_declaration_orders: FxHashMap::default(),
                    current_module: module,
                    next_value_storage: decl_count,
                    next_source_key,
                    continuation_publication: None,
                    frozen_prefix_writes,
                },
                prelude_module,
                compilation_global,
                delta_compilation_global: Some(delta_compilation_global),
                script_namespace_root: delta_script_namespace_root,
                prelude_type_group_count,
                use_mode: BuilderUseMode::Continuation,
                empty_prelude: true,
                continuation_publication_plans: FxHashMap::default(),
                pending_namespace_modules: Vec::new(),
                library_root_projection,
            },
            next_source,
        )
    }

    /// Freeze only the appended user suffix; the library global prefix is already final.
    pub fn finish_frozen_library_continuation(
        mut self,
        module: ScopeId,
    ) -> Result<Binder, &'static str> {
        assert_eq!(self.use_mode, BuilderUseMode::Continuation);
        self.finalize_pending_namespaces();
        allocate_dormant_namespace_value_storages(&mut self.state);
        self.state
            .namespaces
            .publish_local_umd_export_assignment_aliases(
                &mut self.state.graph,
                self.script_namespace_root,
                &mut self.state.frozen_prefix_writes,
            );
        self.state
            .graph
            .get_mut(module)
            .expect("continuation module exists")
            .parent = Some(self.script_namespace_root);
        Ok(Binder {
            graph: self.state.graph,
            symbols: self.state.symbols,
            declarations: self.state.declarations,
            type_groups: self.state.type_groups,
            namespaces: self.state.namespaces,
            module,
            prelude_module: self.prelude_module,
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_root_projection: self.library_root_projection,
            decl_count: self.state.next_value_storage,
            prelude_type_group_count: self.prelude_type_group_count,
            fn_scopes: self.state.fn_scopes,
            fn_decl_ids: self.state.fn_decl_ids,
            block_scopes: self.state.block_scopes,
            module_sources: self.state.module_sources,
            next_source_key: self.state.next_source_key,
            frozen_prefix_writes: self.state.frozen_prefix_writes,
        })
    }

    /// Return only slots declared directly by this module, never inherited ones.
    /// Export lists use this to avoid leaking the ambient prelude across modules.
    pub fn local_symbol_slots(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> (Option<ValueStorageId>, Option<TypeGroupId>) {
        self.state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name))
            .and_then(|symbol_id| self.state.symbols.get(symbol_id))
            .map(|symbol| (symbol.value, symbol.ty))
            .unwrap_or((None, None))
    }

    /// Whether a local imported name blocks parent value lookup after its source
    /// erased a value export. Re-export lists preserve this provenance.
    pub fn local_value_lookup_barrier(&self, scope: ScopeId, name: &str) -> bool {
        self.state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name))
            .and_then(|symbol_id| self.state.symbols.get(symbol_id))
            .is_some_and(|symbol| symbol.blocks_value_lookup)
    }

    pub fn local_type_lookup_barrier(&self, scope: ScopeId, name: &str) -> bool {
        self.state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name))
            .and_then(|symbol_id| self.state.symbols.get(symbol_id))
            .is_some_and(|symbol| symbol.blocks_type_lookup)
    }
}

fn bind_library_statements(
    state: &mut BindState,
    scope: ScopeId,
    statements: &[Statement<'_>],
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    bind_type_declarations(state, scope, statements);
    for statement in statements {
        if let Statement::TSGlobalDeclaration(global) = statement {
            if unit.binding.external_module && (global.declare || unit.binding.declaration_file()) {
                let default_global_scope =
                    state.continuation_default_global_scope(compilation_global);
                let lexical_scope = if state.continuation_active() {
                    state.global_augmentation_overlay_scope(
                        scope,
                        global.global_span.start,
                        default_global_scope,
                    )
                } else {
                    default_global_scope
                };
                bind_statements(state, lexical_scope, &global.body.body);
            }
            continue;
        }
        bind_statement(state, scope, statement);
    }
}

/// Declare top-level type names before body walks so self/sibling references
/// resolve; the checker reserves each `TypeId` and fills it later.
fn bind_type_declarations(state: &mut BindState, scope: ScopeId, statements: &[Statement<'_>]) {
    for stmt in statements {
        bind_type_declaration_statement(state, scope, stmt);
    }
}

fn bind_type_declaration_statement(state: &mut BindState, scope: ScopeId, stmt: &Statement<'_>) {
    match stmt {
        Statement::TSTypeAliasDeclaration(alias) => {
            bind_source_type(
                state,
                scope,
                alias.id.name.as_str(),
                alias.id.span.start,
                DeclarationKind::TypeAlias,
                TypeFragmentKind::TypeAlias,
            );
        }
        Statement::TSInterfaceDeclaration(iface) => {
            bind_source_type(
                state,
                scope,
                iface.id.name.as_str(),
                iface.id.span.start,
                DeclarationKind::Interface,
                TypeFragmentKind::Interface,
            );
        }
        // Class type-side names are reserved up front so self/sibling type references resolve.
        Statement::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                bind_source_type(
                    state,
                    scope,
                    id.name.as_str(),
                    id.span.start,
                    DeclarationKind::Class,
                    TypeFragmentKind::Class,
                );
            }
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                bind_type_declaration(state, scope, decl);
            }
        }
        _ => {}
    }
}

fn bind_type_declaration(state: &mut BindState, scope: ScopeId, decl: &Declaration<'_>) {
    match decl {
        Declaration::TSTypeAliasDeclaration(alias) => {
            bind_source_type(
                state,
                scope,
                alias.id.name.as_str(),
                alias.id.span.start,
                DeclarationKind::TypeAlias,
                TypeFragmentKind::TypeAlias,
            );
        }
        Declaration::TSInterfaceDeclaration(iface) => {
            bind_source_type(
                state,
                scope,
                iface.id.name.as_str(),
                iface.id.span.start,
                DeclarationKind::Interface,
                TypeFragmentKind::Interface,
            );
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                bind_source_type(
                    state,
                    scope,
                    id.name.as_str(),
                    id.span.start,
                    DeclarationKind::Class,
                    TypeFragmentKind::Class,
                );
            }
        }
        _ => {}
    }
}

fn bind_source_type(
    state: &mut BindState,
    scope: ScopeId,
    name: &str,
    binding_start: u32,
    declaration_kind: DeclarationKind,
    fragment_kind: TypeFragmentKind,
) {
    let declaration = state.attach_declaration_scope(binding_start, declaration_kind, scope);
    let publication_scope = state.continuation_publication_scope(scope, binding_start);
    let source = state
        .module_sources
        .get(&state.current_module)
        .copied()
        .expect("current module has stable source ownership");
    declare_type(
        state,
        publication_scope,
        name,
        declaration,
        fragment_kind,
        source,
    );
}

/// Bind a list of statements into `scope`.
fn bind_statements(state: &mut BindState, scope: ScopeId, statements: &[Statement<'_>]) {
    bind_type_declarations(state, scope, statements);
    for stmt in statements {
        bind_statement(state, scope, stmt);
    }
}

/// Bind one statement into `scope` (declarations) and recurse into its
/// expressions/bodies for nested functions.
pub(super) fn bind_statement(state: &mut BindState, scope: ScopeId, stmt: &Statement<'_>) {
    match stmt {
        Statement::VariableDeclaration(decl) => {
            for declarator in &decl.declarations {
                bind_declarator(state, scope, decl.kind, declarator);
            }
        }
        Statement::FunctionDeclaration(func) => {
            bind_function_declaration(state, scope, func);
        }
        // Class value-side names live in the constructor slot; the body still needs scopes.
        Statement::ClassDeclaration(class) => {
            bind_class_declaration(state, scope, class);
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                bind_declaration(state, scope, decl);
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            bind_expression(state, scope, &expr_stmt.expression);
        }
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                bind_expression(state, scope, arg);
            }
        }
        // Bind tests/branches so nested functions and branch-local block scopes are visible.
        Statement::IfStatement(if_stmt) => {
            bind_expression(state, scope, &if_stmt.test);
            bind_statement(state, scope, &if_stmt.consequent);
            if let Some(alternate) = &if_stmt.alternate {
                bind_statement(state, scope, alternate);
            }
        }
        // Blocks always get lexical scopes; this keeps branch-local names local.
        Statement::BlockStatement(block) => {
            bind_block(state, scope, block);
        }
        // Switch clauses share the enclosing scope unless they contain an explicit block.
        Statement::SwitchStatement(switch) => {
            bind_switch(state, scope, switch);
        }
        // Loop conditions/bodies are walked so nested functions and body-local blocks bind.
        Statement::WhileStatement(while_stmt) => {
            bind_expression(state, scope, &while_stmt.test);
            bind_statement(state, scope, &while_stmt.body);
        }
        // A `do … while` has no head binding; walk the body and the condition.
        Statement::DoWhileStatement(do_stmt) => {
            bind_statement(state, scope, &do_stmt.body);
            bind_expression(state, scope, &do_stmt.test);
        }
        // C-style `for (init; test; update) body` — the init declaration lives in a
        // per-loop head scope shared by the test/update/body.
        Statement::ForStatement(for_stmt) => bind_for(state, scope, for_stmt),
        // `for-in`/`for-of` — the iteration variable lives in a per-loop head scope; the
        // source is evaluated in the enclosing scope.
        Statement::ForInStatement(for_in) => bind_for_in_of(
            state,
            scope,
            &for_in.left,
            &for_in.right,
            &for_in.body,
            for_in.span.start,
        ),
        Statement::ForOfStatement(for_of) => bind_for_in_of(
            state,
            scope,
            &for_of.left,
            &for_of.right,
            &for_of.body,
            for_of.span.start,
        ),
        // `label: <stmt>` — the label is not a binding; descend into the body so a
        // labeled loop gets its head scope and a labeled block binds normally.
        Statement::LabeledStatement(labeled) => {
            bind_statement(state, scope, &labeled.body);
        }
        // `try`/`catch`/`finally` — each block gets its own lexical scope so the
        // checker walks it (WU4); the catch parameter is declared in a dedicated
        // catch scope so references resolve (its type is left to the checker).
        Statement::TryStatement(try_stmt) => bind_try(state, scope, try_stmt),
        Statement::WithStatement(with_stmt) => {
            bind_expression(state, scope, &with_stmt.object);
            bind_statement(state, scope, &with_stmt.body);
        }
        // Other statements declare no names in the subset; their sub-expressions (if
        // any) are not in the subset either.
        _ => {}
    }
}

/// Bind a `try`/`catch`/`finally`. The try and finally blocks bind like ordinary
/// blocks. The catch clause gets a dedicated block scope holding the caught
/// parameter (so references inside the handler resolve), with the handler body
/// nested inside it as its own block.
fn bind_try(state: &mut BindState, parent: ScopeId, try_stmt: &TryStatement<'_>) {
    bind_block(state, parent, &try_stmt.block);
    if let Some(handler) = &try_stmt.handler {
        let catch_scope = state.graph.push(Scope::new(ScopeKind::Block, Some(parent)));
        state
            .block_scopes
            .insert((state.current_module, handler.span.start), catch_scope);
        if let Some(param) = &handler.param {
            state.attach_pattern_scope(
                &param.pattern,
                DeclarationKind::CatchParameter,
                catch_scope,
            );
            if let Some((name, binding_start)) = binding_name_and_start(&param.pattern) {
                let (declaration, storage) = bind_source_value(
                    state,
                    catch_scope,
                    name,
                    binding_start,
                    DeclarationKind::CatchParameter,
                );
                declare_value(state, catch_scope, name, storage, declaration);
            }
        }
        bind_block(state, catch_scope, &handler.body);
    }
    if let Some(finalizer) = &try_stmt.finalizer {
        bind_block(state, parent, finalizer);
    }
}

fn bind_declaration(state: &mut BindState, scope: ScopeId, decl: &Declaration<'_>) {
    match decl {
        Declaration::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                bind_declarator(state, scope, var.kind, declarator);
            }
        }
        Declaration::FunctionDeclaration(func) => {
            bind_function_declaration(state, scope, func);
        }
        Declaration::ClassDeclaration(class) => {
            bind_class_declaration(state, scope, class);
        }
        _ => {}
    }
}

/// Bind a `{ … }` block into its own [`ScopeKind::Block`] child scope under
/// `parent`, recording it under `(module scope, block span start)` so the checker
/// descends into the matching scope. The block's statements are bound inside it.
fn bind_block(state: &mut BindState, parent: ScopeId, block: &BlockStatement<'_>) {
    let block_scope = state.graph.push(Scope::new(ScopeKind::Block, Some(parent)));
    state
        .block_scopes
        .insert((state.current_module, block.span.start), block_scope);
    bind_statements(state, block_scope, &block.body);
}

/// Bind a `switch`: the whole case block is ONE lexical scope (per ECMAScript the
/// CaseBlock is a single block environment), keyed by the switch span so the
/// checker can enter it. The discriminant is evaluated in the enclosing scope
/// (before the case block); every clause's test and consequent binds into the
/// shared switch-local scope, so a block-scoped declaration in a case does not
/// leak past the switch, yet remains visible across clauses. Explicit nested
/// `{ }` blocks inside a clause still create their own child scope via `bind_block`.
fn bind_switch(state: &mut BindState, scope: ScopeId, switch: &SwitchStatement<'_>) {
    bind_expression(state, scope, &switch.discriminant);
    let switch_scope = state.graph.push(Scope::new(ScopeKind::Block, Some(scope)));
    state
        .block_scopes
        .insert((state.current_module, switch.span.start), switch_scope);
    for case in &switch.cases {
        bind_type_declarations(state, switch_scope, &case.consequent);
    }
    for case in &switch.cases {
        // Case tests resolve in the switch-local scope (tsc: a test can name an
        // earlier clause's `let`; it reports only the deferred TS2454, not TS2304).
        if let Some(test) = &case.test {
            bind_expression(state, switch_scope, test);
        }
        for statement in &case.consequent {
            bind_statement(state, switch_scope, statement);
        }
    }
}

/// Bind a C-style `for` head into a fresh [`ScopeKind::Block`] head scope (keyed by
/// the loop statement's span start, like [`bind_block`]) so a `for (let i…)`
/// initializer is scoped to the loop, then bind the test/update/body inside it.
fn bind_for(state: &mut BindState, parent: ScopeId, for_stmt: &ForStatement<'_>) {
    let head = state.graph.push(Scope::new(ScopeKind::Block, Some(parent)));
    state
        .block_scopes
        .insert((state.current_module, for_stmt.span.start), head);
    if let Some(init) = &for_stmt.init {
        match init {
            ForStatementInit::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    bind_declarator(state, head, decl.kind, declarator);
                }
            }
            other => {
                if let Some(expr) = other.as_expression() {
                    bind_expression(state, head, expr);
                }
            }
        }
    }
    if let Some(test) = &for_stmt.test {
        bind_expression(state, head, test);
    }
    if let Some(update) = &for_stmt.update {
        bind_expression(state, head, update);
    }
    bind_statement(state, head, &for_stmt.body);
}

/// Bind a `for-in`/`for-of` head: a fresh head scope holds the iteration variable,
/// the source is bound in the enclosing scope (it is evaluated there), and the body
/// is bound inside the head scope.
fn bind_for_in_of(
    state: &mut BindState,
    parent: ScopeId,
    left: &ForStatementLeft<'_>,
    right: &Expression<'_>,
    body: &Statement<'_>,
    span_start: u32,
) {
    let head = state.graph.push(Scope::new(ScopeKind::Block, Some(parent)));
    state
        .block_scopes
        .insert((state.current_module, span_start), head);
    bind_expression(state, parent, right);
    if let ForStatementLeft::VariableDeclaration(decl) = left {
        for declarator in &decl.declarations {
            bind_declarator(state, head, decl.kind, declarator);
        }
    }
    bind_statement(state, head, body);
}

/// Bind a variable declarator: a `var` name targets its nearest function/module,
/// while the initializer remains in the original lexical scope.
pub(super) fn bind_declarator(
    state: &mut BindState,
    scope: ScopeId,
    kind: VariableDeclarationKind,
    declarator: &VariableDeclarator<'_>,
) {
    let declaration_scope = if kind.is_var() {
        state.graph.var_scope(scope).unwrap_or(scope)
    } else {
        scope
    };
    state.attach_pattern_scope(&declarator.id, DeclarationKind::Variable, declaration_scope);
    if let Some((name, binding_start)) = binding_name_and_start(&declarator.id) {
        let (declaration, storage) = bind_source_value(
            state,
            declaration_scope,
            name,
            binding_start,
            DeclarationKind::Variable,
        );
        declare_value(state, declaration_scope, name, storage, declaration);
    }
    if let Some(init) = &declarator.init {
        bind_expression(state, scope, init);
    }
}

/// Bind a function declaration: declare its name (value space) in `scope`, then
/// bind the function itself (its own scope + parameters + body).
pub(super) fn bind_function_declaration(
    state: &mut BindState,
    scope: ScopeId,
    func: &Function<'_>,
) {
    if let Some(id) = &func.id {
        let (declaration, storage) = bind_source_value(
            state,
            scope,
            id.name.as_str(),
            id.span.start,
            DeclarationKind::Function,
        );
        state
            .fn_decl_ids
            .insert((state.current_module, func.span.start), storage);
        declare_function_value(state, scope, id.name.as_str(), storage, declaration);
    }
    bind_function(state, scope, func);
}

/// Bind a class declaration: declare the constructor-side value name, then bind
/// the body. Anonymous class bodies are still walked for nested scopes.
pub(super) fn bind_class_declaration(state: &mut BindState, scope: ScopeId, class: &Class<'_>) {
    if let Some(id) = &class.id {
        let declaration =
            state.attach_declaration_scope(id.span.start, DeclarationKind::Class, scope);
        let storage = state.fresh_value_storage();
        state.attach_value_storage(declaration, storage);
        declare_value(state, scope, id.name.as_str(), storage, declaration);
    }
    bind_class(state, scope, class);
}

/// Bind class-body scopes. The checker owns `extends`/`super`, abstract flags,
/// accessor merging, parameter properties, and deferred `implements` handling.
fn bind_class(state: &mut BindState, parent: ScopeId, class: &Class<'_>) {
    for element in &class.body.body {
        match element {
            // Method-like elements need a function scope even when the body is absent.
            ClassElement::MethodDefinition(method) => {
                bind_function(state, parent, &method.value);
            }
            // A field: walk its initializer for nested functions (the field's type
            // itself is an annotation, which holds no value bindings).
            ClassElement::PropertyDefinition(prop) => {
                if let Some(init) = &prop.value {
                    bind_expression(state, parent, init);
                }
            }
            // Static blocks, accessor properties, and index signatures are out of
            // the M11 subset — no value bindings.
            _ => {}
        }
    }
}

/// Bind a function/arrow scope, record it by `(module scope, span start)`, and
/// declare parameters with fresh value-storage ids for the checker to fill.
fn bind_function(state: &mut BindState, parent: ScopeId, func: &Function<'_>) {
    let fn_scope = state
        .graph
        .push(Scope::new(ScopeKind::Function, Some(parent)));
    state
        .fn_scopes
        .insert((state.current_module, func.span.start), fn_scope);
    if matches!(
        func.r#type,
        FunctionType::FunctionExpression | FunctionType::TSEmptyBodyFunctionExpression
    ) {
        if let Some(id) = &func.id {
            state.attach_declaration_scope(id.span.start, DeclarationKind::Function, fn_scope);
        }
    }

    bind_parameters(state, fn_scope, &func.params);

    for param in &func.params.items {
        if let Some(init) = &param.initializer {
            bind_expression(state, fn_scope, init);
        }
    }
    if let Some(body) = &func.body {
        bind_function_body(state, fn_scope, body);
    }
}

/// Bind an arrow's own scope, mirroring [`bind_function`]. An arrow always has a
/// body (an expression body or a block); the body is bound inside the arrow's
/// function scope.
fn bind_arrow(state: &mut BindState, parent: ScopeId, arrow: &ArrowFunctionExpression<'_>) {
    let fn_scope = state
        .graph
        .push(Scope::new(ScopeKind::Function, Some(parent)));
    state
        .fn_scopes
        .insert((state.current_module, arrow.span.start), fn_scope);

    bind_parameters(state, fn_scope, &arrow.params);

    for param in &arrow.params.items {
        if let Some(init) = &param.initializer {
            bind_expression(state, fn_scope, init);
        }
    }
    bind_function_body(state, fn_scope, &arrow.body);
}

fn bind_parameters(state: &mut BindState, fn_scope: ScopeId, params: &FormalParameters<'_>) {
    for param in &params.items {
        state.attach_pattern_scope(&param.pattern, DeclarationKind::Parameter, fn_scope);
        if let Some((name, binding_start)) = binding_name_and_start(&param.pattern) {
            let (declaration, storage) = bind_source_value(
                state,
                fn_scope,
                name,
                binding_start,
                DeclarationKind::Parameter,
            );
            declare_value(state, fn_scope, name, storage, declaration);
        }
    }
    if let Some(rest) = &params.rest {
        state.attach_pattern_scope(&rest.rest.argument, DeclarationKind::Parameter, fn_scope);
        if let Some((name, binding_start)) = binding_name_and_start(&rest.rest.argument) {
            let (declaration, storage) = bind_source_value(
                state,
                fn_scope,
                name,
                binding_start,
                DeclarationKind::Parameter,
            );
            declare_value(state, fn_scope, name, storage, declaration);
        }
    }
}

/// Bind a function body's statements into the function scope. An expression-body
/// arrow is parsed as a block holding a single `return <expr>`, so walking the
/// statements covers both forms.
fn bind_function_body(state: &mut BindState, fn_scope: ScopeId, body: &FunctionBody<'_>) {
    bind_statements(state, fn_scope, &body.statements);
}

/// Recurse into expression shapes that can contain nested scopes or initializers.
fn bind_expression(state: &mut BindState, scope: ScopeId, expr: &Expression<'_>) {
    match expr {
        Expression::FunctionExpression(func) => bind_function(state, scope, func),
        Expression::ArrowFunctionExpression(arrow) => bind_arrow(state, scope, arrow),
        // Class expressions still need method scopes even when their instance type is unnamed.
        Expression::ClassExpression(class) => bind_class(state, scope, class),
        // M11: `new C(args)` — bind the callee and each argument for nested
        // functions, mirroring the call-expression arm.
        Expression::NewExpression(new_expr) => {
            bind_expression(state, scope, &new_expr.callee);
            for arg in &new_expr.arguments {
                if let Some(arg_expr) = arg.as_expression() {
                    bind_expression(state, scope, arg_expr);
                }
            }
        }
        Expression::CallExpression(call) => {
            bind_expression(state, scope, &call.callee);
            for arg in &call.arguments {
                if let Some(arg_expr) = arg.as_expression() {
                    bind_expression(state, scope, arg_expr);
                }
            }
        }
        Expression::AssignmentExpression(assign) => {
            bind_expression(state, scope, &assign.right);
        }
        Expression::StaticMemberExpression(member) => {
            bind_expression(state, scope, &member.object);
        }
        Expression::ObjectExpression(obj) => {
            for member in &obj.properties {
                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) = member {
                    bind_expression(state, scope, &prop.value);
                }
            }
        }
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                if let Some(expression) = element.as_expression() {
                    bind_expression(state, scope, expression);
                }
            }
        }
        Expression::ParenthesizedExpression(paren) => {
            bind_expression(state, scope, &paren.expression);
        }
        Expression::TSAsExpression(assertion) => {
            bind_expression(state, scope, &assertion.expression);
        }
        Expression::TSTypeAssertion(assertion) => {
            bind_expression(state, scope, &assertion.expression);
        }
        // Literals, identifiers, and other expression shapes hold no nested
        // function in the M3 subset.
        _ => {}
    }
}

fn bind_source_value(
    state: &mut BindState,
    scope: ScopeId,
    _name: &str,
    binding_start: u32,
    kind: DeclarationKind,
) -> (DeclId, ValueStorageId) {
    let declaration = state.attach_declaration_scope(binding_start, kind, scope);
    let storage = state.fresh_value_storage();
    state.attach_value_storage(declaration, storage);
    (declaration, storage)
}

/// Declare a value-space binding `name` in `scope`, merging into an existing
/// symbol if the name is already present (so the multi-slot symbol carries the
/// value slot under the same id — architecture §4.1). Redeclaration in the same
/// space (`TK2451`) is deferred (mvp-plan); the later binding wins.
fn declare_value(
    state: &mut BindState,
    scope: ScopeId,
    name: &str,
    storage: ValueStorageId,
    declaration: DeclId,
) {
    let scope = state.declaration_publication_scope(scope, declaration);
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        match state.symbols.get_mut(existing) {
            Some(symbol) => symbol.value = Some(storage),
            None => state
                .record_frozen_prefix_write(Some(declaration), FrozenPrefixWriteSite::ValueSlot),
        }
        state.attach_symbol_declaration(existing, declaration);
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.value = Some(storage);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    if state.graph.declare(scope, name, symbol_id).is_err() {
        state
            .record_frozen_prefix_write(Some(declaration), FrozenPrefixWriteSite::ScopeDeclaration);
    }
    state.attach_symbol_declaration(symbol_id, declaration);
}

fn declare_function_value(
    state: &mut BindState,
    scope: ScopeId,
    name: &str,
    storage: ValueStorageId,
    declaration: DeclId,
) {
    let scope = state.declaration_publication_scope(scope, declaration);
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        match state.symbols.get_mut(existing) {
            Some(symbol) => {
                symbol.value = Some(storage);
                symbol.function_values.push(storage);
            }
            None => state.record_frozen_prefix_write(
                Some(declaration),
                FrozenPrefixWriteSite::FunctionValueSlot,
            ),
        }
        state.attach_symbol_declaration(existing, declaration);
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.value = Some(storage);
    symbol.function_values.push(storage);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    if state.graph.declare(scope, name, symbol_id).is_err() {
        state
            .record_frozen_prefix_write(Some(declaration), FrozenPrefixWriteSite::ScopeDeclaration);
    }
    state.attach_symbol_declaration(symbol_id, declaration);
}

type TypeGroupFragmentOrderKey = (bool, u64, u32, u32);

fn type_group_fragment_order_key(
    fragment: TypeGroupFragment,
    library_ordinals: &FxHashMap<ScopeId, LibraryFileOrdinal>,
) -> TypeGroupFragmentOrderKey {
    let (user_fragment, unit) = library_ordinals
        .get(&fragment.site.module)
        .map_or((true, u64::from(fragment.source.0)), |ordinal| {
            (false, u64::try_from(ordinal.index()).unwrap_or(u64::MAX))
        });
    (
        user_fragment,
        unit,
        fragment.site.declaration_span.start,
        fragment.declaration.0,
    )
}

/// Retain one source fragment in its stable production type group.
pub(super) fn declare_type(
    state: &mut BindState,
    target_scope: ScopeId,
    name: &str,
    declaration: DeclId,
    kind: TypeFragmentKind,
    source: SourceUnitKey,
) {
    let site = state
        .declarations
        .get(declaration)
        .expect("fresh type declaration exists")
        .site;
    let fragment_scope = site.scope.expect("type declaration has a lexical scope");
    let fragment = TypeGroupFragment {
        declaration,
        source,
        scope: fragment_scope,
        site,
        kind,
    };
    if let Some(existing) = state
        .graph
        .get(target_scope)
        .and_then(|s| s.lookup_local(name))
    {
        let owned = state
            .symbols
            .get(existing)
            .and_then(|symbol| symbol.owns_type_group.then_some(symbol.ty).flatten());
        let appended = state.type_groups.append_fragment(owned, name, fragment);
        let group = appended.group;
        if appended.frozen_merge_refused {
            state.record_frozen_prefix_write(
                Some(declaration),
                FrozenPrefixWriteSite::TypeGroupFragment,
            );
        }
        if let Some(fragments) = state
            .type_groups
            .get_mut(group)
            .map(|row| &mut row.fragments)
        {
            if let Some(appended) = fragments.last().copied() {
                let appended_index = fragments.len().saturating_sub(1);
                let key =
                    type_group_fragment_order_key(appended, &state.type_group_library_ordinals);
                let at = fragments[..appended_index].partition_point(|fragment| {
                    #[cfg(any(test, feature = "test-utils"))]
                    record_type_group_fragment_row_probe();
                    type_group_fragment_order_key(*fragment, &state.type_group_library_ordinals)
                        <= key
                });
                fragments[at..].rotate_right(1);
            }
        }
        let lexical = state
            .declarations
            .get_mut(declaration)
            .expect("fresh type declaration exists");
        lexical.type_group = Some(group);
        // A frozen symbol row cannot take the group either; refuse rather than panic (backlog 103).
        match state.symbols.get_mut(existing) {
            Some(symbol) => {
                symbol.ty = Some(group);
                symbol.owns_type_group = true;
                symbol.blocks_type_lookup = false;
            }
            None => {
                state
                    .record_frozen_prefix_write(Some(declaration), FrozenPrefixWriteSite::TypeSlot);
            }
        }
        state.attach_symbol_declaration(existing, declaration);
        return;
    }
    let group = state
        .type_groups
        .append_fragment(None, name, fragment)
        .group;
    let lexical = state
        .declarations
        .get_mut(declaration)
        .expect("fresh type declaration exists");
    lexical.type_group = Some(group);
    let mut symbol = Symbol::new(name);
    symbol.ty = Some(group);
    symbol.owns_type_group = true;
    let symbol_id: SymbolId = state.symbols.push(symbol);
    if state.graph.declare(target_scope, name, symbol_id).is_err() {
        state
            .record_frozen_prefix_write(Some(declaration), FrozenPrefixWriteSite::ScopeDeclaration);
    }
    state.attach_symbol_declaration(symbol_id, declaration);
}

fn declare_import(
    state: &mut BindState,
    scope: ScopeId,
    import: &ImportedSymbol,
) -> ImportPlaceholder {
    let (value_decl, value_placeholder) = match &import.value {
        Some(ImportedValueSlot::Existing(storage)) => (Some(*storage), None),
        Some(ImportedValueSlot::Placeholder) => {
            let storage = state.fresh_value_storage();
            (Some(storage), Some(storage))
        }
        None => (None, None),
    };
    let type_group = import
        .ty
        .as_ref()
        .map(|ImportedTypeSlot::Existing(group)| *group);
    let declaration =
        state.attach_declaration_scope(import.site.start, DeclarationKind::Import, scope);
    let lexical = state
        .declarations
        .get_mut(declaration)
        .expect("fresh import declaration exists");
    lexical.value_storage = value_decl;
    lexical.type_group = type_group;
    if let Some(existing) = state
        .graph
        .get(scope)
        .and_then(|s| s.lookup_local(&import.name))
    {
        match state.symbols.get_mut(existing) {
            Some(symbol) => {
                symbol.value = value_decl;
                symbol.ty = type_group;
                symbol.owns_type_group = false;
                symbol.blocks_value_lookup = import.value_barrier;
                symbol.blocks_type_lookup = import.type_barrier;
            }
            None => state
                .record_frozen_prefix_write(Some(declaration), FrozenPrefixWriteSite::ImportSlot),
        }
        state.attach_symbol_declaration(existing, declaration);
        return ImportPlaceholder {
            value: value_placeholder,
        };
    }
    let mut symbol = Symbol::new(&import.name);
    symbol.value = value_decl;
    symbol.ty = type_group;
    symbol.owns_type_group = false;
    symbol.blocks_value_lookup = import.value_barrier;
    symbol.blocks_type_lookup = import.type_barrier;
    let symbol_id: SymbolId = state.symbols.push(symbol);
    if state.graph.declare(scope, &import.name, symbol_id).is_err() {
        state
            .record_frozen_prefix_write(Some(declaration), FrozenPrefixWriteSite::ScopeDeclaration);
    }
    state.attach_symbol_declaration(symbol_id, declaration);
    ImportPlaceholder {
        value: value_placeholder,
    }
}

/// The bound name of a binding pattern, if it is a plain identifier. Returns
/// `None` for destructuring patterns (out of the M3 subset).
fn binding_name_and_start<'a>(pattern: &'a BindingPattern<'a>) -> Option<(&'a str, u32)> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some((ident.name.as_str(), ident.span.start)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::namespace::{
        DeclarationOwner, DeclarationSyntaxFacts, GlobalIssue, MergeDeclarationKind,
        MergeDisposition, NamespaceContinuationWorkForTest, NamespaceContinuationWorkScopeForTest,
        NamespaceOwner, NamespaceValueAttachmentDisposition, VariableKind,
    };
    use crate::source::{CompilationOrigin, LibraryFileOrdinal};
    use crate::types::layered::{BaseWorkLedgerForTest, BaseWorkScopeForTest};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::{GetSpan, SourceType};

    fn bind(src: &str) -> Binder {
        let prelude_alloc = Allocator::default();
        let alloc = Allocator::default();
        let prelude = Parser::new(&prelude_alloc, "", SourceType::ts()).parse();
        let parsed = Parser::new(&alloc, src, SourceType::ts()).parse();
        assert!(!parsed.panicked, "parse failed: {src}");
        bind_module_with_prelude(&prelude.program, &parsed.program)
    }

    fn bind_libraries<'ast>(
        programs: &[(&'ast Program<'ast>, SourceUnitKey, LibraryFileOrdinal)],
    ) -> (Binder, Vec<ScopeId>) {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        assert!(prelude.diagnostics.is_empty());
        let units = programs
            .iter()
            .map(|(program, source, file)| {
                (*program, CompilationUnit::library(*source, *file, program))
            })
            .collect::<Vec<_>>();
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let modules = builder.add_library_modules(&units);
        let last = modules.last().copied().expect("library batch is non-empty");
        (builder.finish(last), modules)
    }

    fn declaration_sources(binder: &Binder, symbol: SymbolId) -> Vec<SourceUnitKey> {
        binder
            .symbols
            .get(symbol)
            .expect("canonical symbol exists")
            .declarations
            .iter()
            .map(|declaration| {
                let module = binder
                    .declarations
                    .get(*declaration)
                    .expect("canonical declaration exists")
                    .site
                    .module;
                binder
                    .namespaces
                    .source_units()
                    .find(|unit| unit.module == module)
                    .map(|unit| unit.source)
                    .expect("canonical declaration module has source ownership")
            })
            .collect()
    }

    #[test]
    fn continuation_routes_each_global_collision_leaf_without_capturing_unique_siblings() {
        let prelude_allocator = Allocator::default();
        let library_allocator = Allocator::default();
        let user_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let library = Parser::new(
            &library_allocator,
            "interface Array<T> {} declare namespace Intl {} declare var document: unknown; declare var RegExp: unknown; declare function parseInt(value: string): number;",
            SourceType::d_ts(),
        )
        .parse();
        let user_source = r#"interface Array<T> { added: T }
interface WU5Unique {}
declare namespace Intl { interface Added {} }
declare namespace WU5UniqueNamespace {}
declare function parseInt(value: "one"): 1;
declare function wu5UniqueFunction(): void;
var { collision: Array, unique: uniqueLeaf } = source;
var document = 1, uniqueValue = 2;
with (source) { var RegExp = 1; }
"#;
        let user = Parser::new(&user_allocator, user_source, SourceType::ts()).parse();
        assert!(prelude.diagnostics.is_empty());
        assert!(library.diagnostics.is_empty());
        assert!(user.diagnostics.is_empty());

        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let library_unit = CompilationUnit::library(
            SourceUnitKey(1),
            LibraryFileOrdinal::new(0),
            &library.program,
        );
        builder.add_library_modules(&[(&library.program, library_unit)]);
        let array_symbol = builder
            .state
            .graph
            .get(builder.compilation_global)
            .and_then(|scope| scope.lookup_local("Array"))
            .expect("library Array symbol");
        let array_group = builder
            .state
            .symbols
            .get(array_symbol)
            .and_then(|symbol| symbol.ty)
            .expect("library Array type group");
        let intl_namespace = builder
            .state
            .graph
            .get(builder.compilation_global)
            .and_then(|scope| scope.lookup_local("Intl"))
            .and_then(|symbol| builder.state.symbols.get(symbol))
            .and_then(|symbol| symbol.ns)
            .expect("library Intl namespace");
        let document_symbol = builder
            .state
            .graph
            .get(builder.compilation_global)
            .and_then(|scope| scope.lookup_local("document"))
            .expect("library document symbol");
        let parse_int_symbol = builder
            .state
            .graph
            .get(builder.compilation_global)
            .and_then(|scope| scope.lookup_local("parseInt"))
            .expect("library parseInt symbol");

        builder.use_mode = BuilderUseMode::Continuation;
        builder.state.library_module_ordinals.clear();
        let user_unit = CompilationUnit::implementation(SourceUnitKey(2), &user.program);
        builder.reserve_script_namespace_roots([(&user.program, user_unit)]);
        let (module, _) = builder.add_module(&user.program, &[], user_unit);
        let binder = builder.finish(module);

        assert_eq!(binder.resolve_type(module, "Array"), Some(array_symbol));
        assert_eq!(
            binder
                .symbols
                .get(array_symbol)
                .and_then(|symbol| symbol.ty),
            Some(array_group)
        );
        assert_eq!(
            binder
                .symbols
                .get(document_symbol)
                .map(|symbol| symbol.declarations.len()),
            Some(2)
        );
        assert_eq!(
            binder.resolve_value(module, "parseInt"),
            Some(parse_int_symbol)
        );
        assert_eq!(
            binder
                .graph
                .resolve(module, "Intl")
                .and_then(|symbol| binder.symbols.get(symbol))
                .and_then(|symbol| symbol.ns),
            Some(intl_namespace)
        );
        for unique in ["WU5Unique", "wu5UniqueFunction", "uniqueValue"] {
            assert!(
                binder
                    .graph
                    .get(module)
                    .and_then(|scope| scope.lookup_local(unique))
                    .is_some(),
                "{unique} remains module-local"
            );
            assert!(
                binder
                    .graph
                    .get(binder.compilation_global)
                    .and_then(|scope| scope.lookup_local(unique))
                    .is_none(),
                "{unique} does not leak into the library global"
            );
        }
        let binding_scope = |name: &str, kind: DeclarationKind| {
            binder
                .declarations
                .iter()
                .find(|declaration| {
                    declaration.site.module == module
                        && declaration.kind == kind
                        && &user_source[declaration.site.binding_span.range()] == name
                })
                .and_then(|declaration| declaration.site.scope)
                .unwrap_or_else(|| panic!("binding scope for {name}"))
        };
        assert_eq!(binding_scope("Array", DeclarationKind::Interface), module);
        assert_eq!(binding_scope("Array", DeclarationKind::Variable), module);
        assert_eq!(binding_scope("document", DeclarationKind::Variable), module);
        assert_eq!(binding_scope("RegExp", DeclarationKind::Variable), module);
        assert_eq!(
            binding_scope("uniqueLeaf", DeclarationKind::Variable),
            module
        );
        assert_eq!(
            binding_scope("uniqueValue", DeclarationKind::Variable),
            module
        );
        assert!(binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local("WU5UniqueNamespace"))
            .is_some());
        for name in ["document", "RegExp"] {
            let user_participants = binder
                .namespaces
                .merges()
                .find(|record| {
                    record.owner == DeclarationOwner::CompilationGlobal
                        && record.name.as_ref() == name
                })
                .expect("collision has a compilation-global merge")
                .declarations
                .iter()
                .filter(|participant| {
                    participant.kind == MergeDeclarationKind::Variable
                        && participant.source == user_unit.source
                })
                .collect::<Vec<_>>();
            assert_eq!(
                user_participants.len(),
                1,
                "{name} publishes one exact user participant"
            );
            assert_eq!(
                user_participants[0].syntax,
                DeclarationSyntaxFacts::Variable(VariableKind::Var)
            );
        }
    }

    #[test]
    fn frozen_binder_forks_share_prefix_and_classify_isolated_namespace_suffixes() {
        let library_allocator = Allocator::default();
        let library = Parser::new(
            &library_allocator,
            "declare namespace BaseSpace { interface Shape {} } declare const baseValue: string;",
            SourceType::d_ts(),
        )
        .parse();
        assert!(library.diagnostics.is_empty());
        let (mut base, _) = bind_libraries(&[(
            &library.program,
            SourceUnitKey(1),
            LibraryFileOrdinal::new(0),
        )]);
        let base_scopes = base.graph.len();
        let base_symbols = base.symbols.len();
        let base_declarations = base.declarations.len();
        let base_groups = base.type_groups.len();
        let base_namespaces = base.namespaces.len();
        base.freeze_as_base().expect("binder base freezes once");

        let user_allocator = Allocator::default();
        let user = Parser::new(
            &user_allocator,
            "export namespace LocalSpace { export interface Shape {} export const value = 1; }",
            SourceType::ts(),
        )
        .parse();
        assert!(user.diagnostics.is_empty());

        let first_seed = base.fork_delta().expect("first private binder delta");
        let second_seed = base.fork_delta().expect("second private binder delta");
        assert!(base.shares_base_storage_with(&first_seed));
        assert!(first_seed.shares_base_storage_with(&second_seed));

        let (mut first_builder, first_source) =
            ProjectBinderBuilder::resume_frozen_library(first_seed);
        let first_unit = CompilationUnit::implementation(first_source, &user.program);
        first_builder.reserve_script_namespace_roots([(&user.program, first_unit)]);
        let (first_module, _) = first_builder.add_module(&user.program, &[], first_unit);
        let first = first_builder
            .finish_frozen_library_continuation(first_module)
            .expect("first namespace suffix freezes");

        assert!(first.graph.len() > base_scopes);
        assert!(first.symbols.len() > base_symbols);
        assert!(first.declarations.len() > base_declarations);
        assert!(first.type_groups.len() > base_groups);
        assert!(first.namespaces.len() > base_namespaces);
        assert!(first
            .graph
            .get(first.module)
            .and_then(|scope| scope.lookup_local("LocalSpace"))
            .is_some());
        assert!(first
            .graph
            .get(first.script_namespace_root)
            .and_then(|scope| scope.lookup_local("LocalSpace"))
            .is_none());
        assert!(first.namespaces.namespaces().any(|namespace| {
            namespace.name == "LocalSpace"
                && namespace.id.index() >= base_namespaces
                && namespace.owner == NamespaceOwner::Lexical(first.module)
        }));
        assert!(base.shares_base_storage_with(&first));

        assert!(second_seed
            .graph
            .all_scopes()
            .all(|scope| scope.lookup_local("LocalSpace").is_none()));
        assert!(!second_seed
            .namespaces
            .namespaces()
            .any(|namespace| namespace.name == "LocalSpace"));
        assert_eq!(base.graph.len(), base_scopes);
        assert_eq!(base.symbols.len(), base_symbols);
        assert_eq!(base.declarations.len(), base_declarations);
        assert_eq!(base.type_groups.len(), base_groups);
        assert_eq!(base.namespaces.len(), base_namespaces);
    }

    #[test]
    fn frozen_binder_plain_script_namespace_uses_a_reachable_delta_root() {
        let library_allocator = Allocator::default();
        let library = Parser::new(
            &library_allocator,
            "declare namespace BaseSpace { interface Shape {} }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(library.diagnostics.is_empty());
        let (mut base, _) = bind_libraries(&[(
            &library.program,
            SourceUnitKey(1),
            LibraryFileOrdinal::new(0),
        )]);
        let frozen_script_root = base.script_namespace_root;
        let frozen_scope_count = base.graph.len();
        base.freeze_as_base().expect("binder base freezes once");

        let user_allocator = Allocator::default();
        let user = Parser::new(
            &user_allocator,
            "namespace Local { export const x = 1; } const witness = Local.x;",
            SourceType::ts(),
        )
        .parse();
        assert!(user.diagnostics.is_empty());
        let delta = base.fork_delta().expect("private binder delta");
        let (mut builder, source) = ProjectBinderBuilder::resume_frozen_library(delta);
        let unit = CompilationUnit::implementation(source, &user.program);
        builder.reserve_script_namespace_roots([(&user.program, unit)]);
        let (module, _) = builder.add_module(&user.program, &[], unit);
        let binder = builder
            .finish_frozen_library_continuation(module)
            .expect("plain-script namespace suffix freezes");

        assert!(binder.script_namespace_root.index() >= frozen_scope_count);
        // The delta root hangs off the delta global layer, which in turn continues the frozen
        // chain: user script globals are writable without ever touching the sealed prefix.
        let delta_global = binder
            .graph
            .get(binder.script_namespace_root)
            .and_then(|scope| scope.parent)
            .expect("delta script root has a parent");
        assert!(delta_global.index() >= frozen_scope_count);
        assert_eq!(
            binder.graph.get(delta_global).map(|scope| scope.kind),
            Some(ScopeKind::CompilationGlobal)
        );
        assert_eq!(
            binder
                .graph
                .get(delta_global)
                .and_then(|scope| scope.parent),
            Some(frozen_script_root)
        );
        let local = binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == "Local")
            .expect("local script namespace");
        assert_eq!(local.owner, NamespaceOwner::Lexical(delta_global));
        let storage = binder
            .namespaces
            .standalone_value_storage(local.id)
            .expect("instantiated local namespace storage");
        assert_eq!(
            binder.resolve_value_binding(binder.module, "Local"),
            ValueResolution::Resolved {
                symbol: local.symbol,
                kind: ResolvedValueKind::StandaloneNamespace {
                    namespace: local.id,
                    storage,
                },
            }
        );
        let attachment = binder
            .local_standalone_namespace_value_attachments()
            .into_iter()
            .find(|attachment| attachment.namespace == local.id)
            .expect("local namespace value attachment");
        assert!(attachment
            .members
            .iter()
            .any(|member| { member.name == Some("x") && member.value_storage.is_some() }));
        assert!(base
            .graph
            .get(frozen_script_root)
            .and_then(|scope| scope.lookup_local("Local"))
            .is_none());
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ContinuationNamespaceWorkShape {
        local_namespaces: usize,
        local_merges: usize,
        local_standalone_attachments: usize,
        placement_issues: usize,
        globals: usize,
        umd_exports: usize,
        ambient_alias_failures: usize,
        value_storage_delta: u32,
        symbol_delta: usize,
        work: NamespaceContinuationWorkForTest,
    }

    const CONTINUATION_PROJECTION_SUBTABLES: [&str; 31] = [
        "store.rows",
        "store.payload-tables",
        "store.type-param-constraints",
        "store.frozen-type-params",
        "store.template-names",
        "interner.dedup-buckets",
        "interner.reserved-terminals",
        "interner.well-known",
        "binder.scopes",
        "binder.symbols",
        "binder.declarations",
        "binder.declaration-site-index",
        "binder.type-groups",
        "binder.namespaces",
        "binder.namespace-indexes",
        "binder.module-sources",
        "decl-types.slots",
        "published-types.groups",
        "published-types.classes",
        "namespace-terminals",
        "function-groups.symbols",
        "class.application-parameters",
        "class.parameter-defaults",
        "class.parents",
        "class.names",
        "class.new-metadata",
        "class.value-identities",
        "class.aliases",
        "semantic-identities",
        "root-name-index.entries",
        "next-ids",
    ];

    fn assert_no_unattributed_continuation_base_work(ledger: &BaseWorkLedgerForTest) {
        for operation in [
            &ledger.sequential_scans,
            &ledger.materializations,
            &ledger.clones,
            &ledger.remaps,
        ] {
            assert_eq!(operation.len(), CONTINUATION_PROJECTION_SUBTABLES.len());
            assert!(CONTINUATION_PROJECTION_SUBTABLES
                .iter()
                .all(|family| operation.contains_key(family)));
            assert!(!operation.contains_key("unattributed"));
        }
    }

    fn continuation_namespace_work_shape(
        frozen_namespace_count: usize,
    ) -> ContinuationNamespaceWorkShape {
        let library_source = (0..frozen_namespace_count)
            .map(|index| {
                format!(
                    "export as namespace FrozenUmd{index}; \
                     declare global {{ interface FrozenGlobal{index} {{}} }} \
                     declare namespace Frozen{index} {{ \
                         export const value: number; \
                         export {{ Missing as Alias }}; \
                     }}"
                )
            })
            .collect::<String>();
        let library_allocator = Allocator::default();
        let library = Parser::new(&library_allocator, &library_source, SourceType::d_ts()).parse();
        assert!(library.diagnostics.is_empty());
        let (mut base, _) = bind_libraries(&[(
            &library.program,
            SourceUnitKey(1),
            LibraryFileOrdinal::new(0),
        )]);
        let frozen_name = format!("Frozen{}", frozen_namespace_count - 1);
        let frozen_namespace = base
            .namespaces
            .namespaces()
            .find(|namespace| namespace.name == frozen_name)
            .map(|namespace| namespace.id)
            .expect("scaled frozen namespace exists");
        let frozen_storage = base
            .namespaces
            .standalone_value_storage(frozen_namespace)
            .expect("scaled frozen namespace has a value slot");
        let base_decl_count = base.decl_count;
        let base_symbol_count = base.symbols.len();
        base.freeze_as_base().expect("scaled binder base freezes");

        let user_allocator = Allocator::default();
        let user = Parser::new(
            &user_allocator,
            r#"
                export namespace LocalSpace {
                    export const value = 1;
                    export namespace Child { export const nested = 2; }
                }
                export function Attached(value: number): number { return value; }
                export namespace Attached { export const extra = 1; }
                export namespace Late { export const before = 1; }
                export function Late(): void {}
                export as namespace UserUmd;
                export declare namespace AmbientAlias {
                    export { Missing as Alias };
                }
                export default 1;
            "#,
            SourceType::ts(),
        )
        .parse();
        assert!(user.diagnostics.is_empty());
        let delta = base.fork_delta().expect("scaled private binder delta");
        let base_work_scope = BaseWorkScopeForTest::start(CONTINUATION_PROJECTION_SUBTABLES);
        let work_scope = NamespaceContinuationWorkScopeForTest::start();
        let (mut builder, source) = ProjectBinderBuilder::resume_frozen_library(delta);
        assert_eq!(source, SourceUnitKey(2));
        let unit = CompilationUnit::implementation(source, &user.program);
        builder.reserve_script_namespace_roots([(&user.program, unit)]);
        let (module, _) = builder.add_module(&user.program, &[], unit);
        let binder = builder
            .finish_frozen_library_continuation(module)
            .expect("scaled namespace suffix freezes");

        assert_eq!(
            binder.standalone_namespace_for_storage(frozen_storage),
            Some(frozen_namespace),
            "the frozen storage index remains queryable without a prefix scan"
        );
        let local = binder
            .namespaces
            .local_namespaces()
            .find(|(_, namespace)| namespace.name == "LocalSpace")
            .map(|(id, _)| id)
            .expect("local standalone namespace exists");
        let local_storage = binder
            .namespaces
            .standalone_value_storage(local)
            .expect("local standalone namespace has a value slot");
        assert_eq!(
            binder.standalone_namespace_for_storage(local_storage),
            Some(local)
        );
        assert_eq!(
            binder.standalone_namespace_for_storage(ValueStorageId(u32::MAX)),
            None
        );
        let local_fragment = binder
            .namespaces
            .get(local)
            .and_then(|namespace| namespace.fragments.first())
            .and_then(|fragment| binder.namespaces.fragment(*fragment))
            .expect("local namespace fragment exists");
        assert_eq!(
            binder.namespace_fragment_private_scope(module, local_fragment.source_start),
            Some(local_fragment.private_scope)
        );
        let attached = binder
            .namespace_value_attachment(module, "Attached")
            .expect("indexed attached merge resolves");
        assert_eq!(
            attached.disposition,
            NamespaceValueAttachmentDisposition::AdmittedFunction
        );
        assert!(attached.members.iter().any(|member| member.name == "extra"));

        let local_merges = binder.namespaces.local_merges().count();
        let local_standalone_attachments =
            binder.local_standalone_namespace_value_attachments().len();
        let placement_issues = binder.namespaces.local_placement_issues().count();
        let globals = binder.namespaces.local_globals().count();
        let local_umd_exports = binder.namespaces.local_umd_exports().collect::<Vec<_>>();
        let umd_exports = local_umd_exports.len();
        let umd = local_umd_exports.first().expect("local UMD export exists");
        assert!(!binder.umd_export_requires_incomplete(module, umd.span.start));
        assert_eq!(binder.global_augmentation_scope(module, u32::MAX), None);
        assert!(!binder.global_augmentation_requires_incomplete(module, u32::MAX));
        let export_default_start = user
            .program
            .body
            .iter()
            .find_map(|statement| {
                matches!(statement, Statement::ExportDefaultDeclaration(_))
                    .then_some(statement.span().start)
            })
            .expect("user export default exists");
        assert!(!binder.library_export_default_reporting_owns(module, export_default_start));
        assert!(!binder.library_module_reporting_owns(module, local_fragment.source_start));
        let ambient_alias_failures = binder.local_ambient_export_alias_failures().len();
        let work = work_scope.finish();
        let base_work = base_work_scope.finish();
        assert_no_unattributed_continuation_base_work(&base_work);

        ContinuationNamespaceWorkShape {
            local_namespaces: binder.namespaces.local_namespaces().count(),
            local_merges,
            local_standalone_attachments,
            placement_issues,
            globals,
            umd_exports,
            ambient_alias_failures,
            value_storage_delta: binder.decl_count - base_decl_count,
            symbol_delta: binder.symbols.len() - base_symbol_count,
            work,
        }
    }

    #[test]
    fn frozen_namespace_continuation_work_shape_is_independent_of_base_size() {
        let small = continuation_namespace_work_shape(2);
        let scaled = continuation_namespace_work_shape(1_024);
        assert_eq!(scaled, small);
        assert_eq!(
            scaled,
            ContinuationNamespaceWorkShape {
                local_namespaces: 5,
                local_merges: 9,
                local_standalone_attachments: 2,
                placement_issues: 1,
                globals: 0,
                umd_exports: 1,
                ambient_alias_failures: 1,
                value_storage_delta: 9,
                symbol_delta: 15,
                work: NamespaceContinuationWorkForTest {
                    merge_rows: 18,
                    instance_fragment_rows: 10,
                    child_fragment_lookups: 1,
                    allocation_namespace_rows: 10,
                    attachment_namespace_rows: 5,
                    fragment_scope_lookups: 1,
                    placement_merge_rows: 9,
                    global_rows: 0,
                    umd_rows: 2,
                    ambient_alias_member_rows: 6,
                    umd_statement_queries: 1,
                    global_statement_queries: 2,
                    library_source_lookups: 1,
                    library_reporting_lookups: 2,
                },
            }
        );
    }

    #[test]
    #[should_panic(expected = "project modules cannot follow the one-shot library batch")]
    fn library_batch_rejects_a_later_project_module() {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let source = Parser::new(
            &source_allocator,
            "interface LibraryOnly {}",
            SourceType::d_ts(),
        )
        .parse();
        let library = CompilationUnit::library(
            SourceUnitKey(10),
            LibraryFileOrdinal::new(10),
            &source.program,
        );
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        builder.add_library_modules(&[(&source.program, library)]);
        let project = CompilationUnit::implementation(SourceUnitKey(11), &source.program);
        builder.add_module(&source.program, &[], project);
    }

    #[test]
    #[should_panic(expected = "library batch requires a pristine project builder")]
    fn library_batch_rejects_reserved_script_namespace_roots() {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let source = Parser::new(
            &source_allocator,
            "declare namespace ReservedBeforeLibrary { export const value: number; }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(source.diagnostics.is_empty());
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let project = CompilationUnit::implementation(SourceUnitKey(11), &source.program);
        builder.reserve_script_namespace_roots([(&source.program, project)]);
        assert!(builder
            .state
            .graph
            .get(builder.script_namespace_root)
            .and_then(|scope| scope.lookup_local("ReservedBeforeLibrary"))
            .is_some());
        let library = CompilationUnit::library(
            SourceUnitKey(10),
            LibraryFileOrdinal::new(10),
            &source.program,
        );
        builder.add_library_modules(&[(&source.program, library)]);
    }

    #[test]
    #[should_panic(expected = "project modules cannot follow the one-shot library batch")]
    fn script_namespace_root_reservation_rejects_a_finished_library_batch() {
        let prelude_allocator = Allocator::default();
        let library_allocator = Allocator::default();
        let script_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let library = Parser::new(
            &library_allocator,
            "interface LibraryBeforeReservation {}",
            SourceType::d_ts(),
        )
        .parse();
        let script = Parser::new(
            &script_allocator,
            "declare namespace ReservedAfterLibrary { export const value: number; }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(library.diagnostics.is_empty());
        assert!(script.diagnostics.is_empty());
        let library_unit = CompilationUnit::library(
            SourceUnitKey(10),
            LibraryFileOrdinal::new(10),
            &library.program,
        );
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        builder.add_library_modules(&[(&library.program, library_unit)]);
        let project = CompilationUnit::implementation(SourceUnitKey(11), &script.program);
        builder.reserve_script_namespace_roots([(&script.program, project)]);
    }

    #[test]
    #[should_panic(expected = "library batch requires a pristine project builder")]
    fn library_batch_rejects_an_existing_project_module() {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let source = Parser::new(
            &source_allocator,
            "interface ProjectFirst {}",
            SourceType::d_ts(),
        )
        .parse();
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let project = CompilationUnit::implementation(SourceUnitKey(11), &source.program);
        builder.add_module(&source.program, &[], project);
        let library = CompilationUnit::library(
            SourceUnitKey(10),
            LibraryFileOrdinal::new(10),
            &source.program,
        );
        builder.add_library_modules(&[(&source.program, library)]);
    }

    #[test]
    #[should_panic(expected = "library batch is one-shot")]
    fn library_batch_rejects_a_second_batch() {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let source = Parser::new(
            &source_allocator,
            "interface OneShot {}",
            SourceType::d_ts(),
        )
        .parse();
        let unit = CompilationUnit::library(
            SourceUnitKey(10),
            LibraryFileOrdinal::new(10),
            &source.program,
        );
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        builder.add_library_modules(&[(&source.program, unit)]);
        builder.add_library_modules(&[(&source.program, unit)]);
    }

    #[test]
    #[should_panic(expected = "library batch requires a fresh empty prelude")]
    fn library_batch_rejects_a_non_empty_prelude() {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(
            &prelude_allocator,
            "interface PreludeFallback {}",
            SourceType::d_ts(),
        )
        .parse();
        let source = Parser::new(
            &source_allocator,
            "interface PreludeFallback {} type PreludeFallback = string;",
            SourceType::d_ts(),
        )
        .parse();
        let unit = CompilationUnit::library(
            SourceUnitKey(10),
            LibraryFileOrdinal::new(10),
            &source.program,
        );
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        builder.add_library_modules(&[(&source.program, unit)]);
    }

    #[test]
    #[should_panic(expected = "library binder requires at least one unit")]
    fn library_batch_rejects_an_empty_batch() {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        builder.add_library_modules(&[]);
    }

    #[test]
    #[should_panic(expected = "library source key cannot be the prelude key")]
    fn library_batch_rejects_the_prelude_source_key() {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let source = Parser::new(
            &source_allocator,
            "interface InvalidPreludeOwnedLibrary {}",
            SourceType::d_ts(),
        )
        .parse();
        let unit = CompilationUnit::library(
            SourceUnitKey::PRELUDE,
            LibraryFileOrdinal::new(0),
            &source.program,
        );
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        builder.add_library_modules(&[(&source.program, unit)]);
    }

    #[test]
    #[should_panic(expected = "library file ordinal is unique")]
    fn library_batch_rejects_duplicate_file_ordinals() {
        let prelude_allocator = Allocator::default();
        let first_allocator = Allocator::default();
        let second_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let first = Parser::new(
            &first_allocator,
            "interface FirstLibraryFile {}",
            SourceType::d_ts(),
        )
        .parse();
        let second = Parser::new(
            &second_allocator,
            "interface SecondLibraryFile {}",
            SourceType::d_ts(),
        )
        .parse();
        let units = [
            (
                &first.program,
                CompilationUnit::library(
                    SourceUnitKey(10),
                    LibraryFileOrdinal::new(10),
                    &first.program,
                ),
            ),
            (
                &second.program,
                CompilationUnit::library(
                    SourceUnitKey(11),
                    LibraryFileOrdinal::new(10),
                    &second.program,
                ),
            ),
        ];
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        builder.add_library_modules(&units);
    }

    #[test]
    #[should_panic(expected = "library source key is unique")]
    fn library_batch_rejects_duplicate_source_keys() {
        let prelude_allocator = Allocator::default();
        let first_allocator = Allocator::default();
        let second_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let first = Parser::new(
            &first_allocator,
            "interface FirstLibrarySource {}",
            SourceType::d_ts(),
        )
        .parse();
        let second = Parser::new(
            &second_allocator,
            "interface SecondLibrarySource {}",
            SourceType::d_ts(),
        )
        .parse();
        let units = [
            (
                &first.program,
                CompilationUnit::library(
                    SourceUnitKey(10),
                    LibraryFileOrdinal::new(10),
                    &first.program,
                ),
            ),
            (
                &second.program,
                CompilationUnit::library(
                    SourceUnitKey(10),
                    LibraryFileOrdinal::new(11),
                    &second.program,
                ),
            ),
        ];
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        builder.add_library_modules(&units);
    }

    #[test]
    fn library_scripts_reopen_one_global_interface_group_with_exact_provenance() {
        let first_allocator = Allocator::default();
        let second_allocator = Allocator::default();
        let first = Parser::new(
            &first_allocator,
            "interface SharedLibraryShape { first: number; }",
            SourceType::d_ts(),
        )
        .parse();
        let second = Parser::new(
            &second_allocator,
            "interface SharedLibraryShape { second: string; }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(first.diagnostics.is_empty());
        assert!(second.diagnostics.is_empty());
        let first_source = SourceUnitKey(20);
        let second_source = SourceUnitKey(21);
        let first_file = LibraryFileOrdinal::new(20);
        let second_file = LibraryFileOrdinal::new(21);
        let (binder, modules) = bind_libraries(&[
            (&first.program, first_source, first_file),
            (&second.program, second_source, second_file),
        ]);

        assert_ne!(modules[0], modules[1]);
        let first_symbol = binder
            .resolve_type(modules[0], "SharedLibraryShape")
            .expect("first script resolves shared interface");
        let second_symbol = binder
            .resolve_type(modules[1], "SharedLibraryShape")
            .expect("second script resolves shared interface");
        assert_eq!(first_symbol, second_symbol);
        let symbol = binder.symbols.get(first_symbol).expect("shared symbol");
        let group = symbol.ty.expect("shared type group");
        assert_eq!(symbol.declarations.len(), 2);
        let fragments = &binder
            .type_groups
            .get(group)
            .expect("shared type group row")
            .fragments;
        assert_eq!(fragments.len(), 2);
        assert_eq!(
            fragments
                .iter()
                .map(|fragment| (fragment.source, fragment.site.module, fragment.scope))
                .collect::<Vec<_>>(),
            [
                (first_source, modules[0], binder.compilation_global),
                (second_source, modules[1], binder.compilation_global),
            ]
        );
    }

    #[test]
    fn library_overloads_and_type_fragments_are_canonical_before_binding() {
        let first_allocator = Allocator::default();
        let second_allocator = Allocator::default();
        let first = Parser::new(
            &first_allocator,
            "declare function CanonicalOverload(value: number): number; interface CanonicalType { first: number; }",
            SourceType::d_ts(),
        )
        .parse();
        let second = Parser::new(
            &second_allocator,
            "declare function CanonicalOverload(value: string): string; declare namespace CanonicalOverload { export const tag: boolean; } interface CanonicalType { second: string; }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(first.diagnostics.is_empty());
        assert!(second.diagnostics.is_empty());
        let first_row = (
            &first.program,
            SourceUnitKey(900),
            LibraryFileOrdinal::new(70),
        );
        let second_row = (
            &second.program,
            SourceUnitKey(100),
            LibraryFileOrdinal::new(71),
        );
        let (forward, forward_modules) = bind_libraries(&[first_row, second_row]);
        let (reverse, reverse_modules) = bind_libraries(&[second_row, first_row]);
        assert_eq!(forward_modules[0], reverse_modules[1]);
        assert_eq!(forward_modules[1], reverse_modules[0]);

        let snapshot = |binder: &Binder| {
            let function_symbol = binder
                .graph
                .get(binder.compilation_global)
                .and_then(|scope| scope.lookup_local("CanonicalOverload"))
                .expect("canonical overload symbol");
            let function = binder
                .symbols
                .get(function_symbol)
                .expect("canonical overload row");
            let attachment = binder
                .namespace_value_attachment(binder.compilation_global, "CanonicalOverload")
                .expect("canonical overload namespace attachment");
            assert_eq!(attachment.symbol, function_symbol);
            assert_eq!(
                attachment.disposition,
                NamespaceValueAttachmentDisposition::AdmittedFunction
            );
            let type_symbol = binder
                .resolve_type(binder.compilation_global, "CanonicalType")
                .expect("canonical reopened interface");
            let type_group = binder
                .symbols
                .get(type_symbol)
                .and_then(|symbol| symbol.ty)
                .expect("canonical type group identity");
            let fragment_sources = binder
                .type_groups
                .get(type_group)
                .expect("canonical type group row")
                .fragments
                .iter()
                .map(|fragment| fragment.source)
                .collect::<Vec<_>>();
            let member_storage = attachment.members[0]
                .value_storage
                .expect("canonical namespace member storage");
            (
                function_symbol,
                function.value,
                function.function_values.clone(),
                declaration_sources(binder, function_symbol),
                attachment.symbol,
                member_storage,
                type_symbol,
                type_group,
                fragment_sources,
                binder
                    .namespaces
                    .source_units()
                    .map(|unit| (unit.source, unit.origin, unit.module))
                    .collect::<Vec<_>>(),
            )
        };
        let forward_snapshot = snapshot(&forward);
        let reverse_snapshot = snapshot(&reverse);
        assert_eq!(forward_snapshot, reverse_snapshot);
        assert_eq!(forward_snapshot.1, forward_snapshot.2.last().copied());
        assert_eq!(forward_snapshot.2.len(), 2);
        assert_eq!(
            forward_snapshot.3,
            [SourceUnitKey(900), SourceUnitKey(100), SourceUnitKey(100)]
        );
        assert_eq!(forward_snapshot.8, [SourceUnitKey(900), SourceUnitKey(100)]);
        assert_eq!(
            forward_snapshot.9,
            [
                (
                    SourceUnitKey(900),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(70)),
                    forward_modules[0],
                ),
                (
                    SourceUnitKey(100),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(71)),
                    forward_modules[1],
                ),
            ]
        );
    }

    fn assert_library_function_namespace(
        binder: &Binder,
        modules: &[ScopeId],
        name: &str,
        expected_sources: [SourceUnitKey; 2],
    ) {
        let symbols = modules
            .iter()
            .map(|module| {
                binder
                    .resolve_value(*module, name)
                    .expect("merged global value")
            })
            .collect::<Vec<_>>();
        assert!(symbols.windows(2).all(|pair| pair[0] == pair[1]));
        let symbol = binder.symbols.get(symbols[0]).expect("merged symbol");
        assert!(symbol.value.is_some());
        assert_eq!(symbol.function_values.len(), 1);
        assert_eq!(symbol.value, symbol.function_values.first().copied());
        assert!(symbol.ns.is_some());
        assert_eq!(symbol.declarations.len(), 2);
        let attachment = binder
            .namespace_value_attachment(binder.compilation_global, name)
            .expect("global function/namespace attachment");
        assert_eq!(
            attachment.disposition,
            NamespaceValueAttachmentDisposition::AdmittedFunction
        );
        assert_eq!(attachment.symbol, symbols[0]);
        assert_eq!(attachment.members.len(), 1);
        assert_eq!(attachment.members[0].name, "tag");
        assert!(attachment.members[0].value_storage.is_some());
        let merge = binder
            .namespaces
            .merges()
            .find(|record| {
                record.owner == crate::binder::namespace::DeclarationOwner::CompilationGlobal
                    && record.name.as_ref() == name
            })
            .expect("global merge record");
        assert_eq!(merge.classification.disposition, MergeDisposition::Admitted);
        assert_eq!(
            merge
                .declarations
                .iter()
                .map(|participant| participant.source)
                .collect::<Vec<_>>(),
            expected_sources
        );
    }

    #[test]
    fn library_function_namespace_merges_are_complete_in_both_source_and_input_orders() {
        let function_first_allocator = Allocator::default();
        let function_first_namespace_allocator = Allocator::default();
        let namespace_first_allocator = Allocator::default();
        let namespace_first_function_allocator = Allocator::default();
        let function_first = Parser::new(
            &function_first_allocator,
            "declare function FunctionFirst(value: number): string;",
            SourceType::d_ts(),
        )
        .parse();
        let function_first_namespace = Parser::new(
            &function_first_namespace_allocator,
            "declare namespace FunctionFirst { export const tag: number; }",
            SourceType::d_ts(),
        )
        .parse();
        let namespace_first = Parser::new(
            &namespace_first_allocator,
            "declare namespace NamespaceFirst { export const tag: string; }",
            SourceType::d_ts(),
        )
        .parse();
        let namespace_first_function = Parser::new(
            &namespace_first_function_allocator,
            "declare function NamespaceFirst(value: string): number;",
            SourceType::d_ts(),
        )
        .parse();
        for parsed in [
            &function_first,
            &function_first_namespace,
            &namespace_first,
            &namespace_first_function,
        ] {
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        }
        let programs = [
            (
                &function_first.program,
                SourceUnitKey(30),
                LibraryFileOrdinal::new(30),
            ),
            (
                &function_first_namespace.program,
                SourceUnitKey(31),
                LibraryFileOrdinal::new(31),
            ),
            (
                &namespace_first.program,
                SourceUnitKey(32),
                LibraryFileOrdinal::new(32),
            ),
            (
                &namespace_first_function.program,
                SourceUnitKey(33),
                LibraryFileOrdinal::new(33),
            ),
        ];
        let reversed = programs.iter().rev().copied().collect::<Vec<_>>();
        let (forward, forward_modules) = bind_libraries(&programs);
        let (reverse, reverse_modules) = bind_libraries(&reversed);
        assert_eq!(
            forward
                .namespaces
                .source_units()
                .map(|unit| (unit.source, unit.origin, unit.module))
                .collect::<Vec<_>>(),
            [
                (
                    SourceUnitKey(30),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(30)),
                    forward_modules[0],
                ),
                (
                    SourceUnitKey(31),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(31)),
                    forward_modules[1],
                ),
                (
                    SourceUnitKey(32),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(32)),
                    forward_modules[2],
                ),
                (
                    SourceUnitKey(33),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(33)),
                    forward_modules[3],
                ),
            ]
        );
        assert_eq!(
            reverse
                .namespaces
                .source_units()
                .map(|unit| (unit.source, unit.origin, unit.module))
                .collect::<Vec<_>>(),
            [
                (
                    SourceUnitKey(30),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(30)),
                    reverse_modules[3],
                ),
                (
                    SourceUnitKey(31),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(31)),
                    reverse_modules[2],
                ),
                (
                    SourceUnitKey(32),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(32)),
                    reverse_modules[1],
                ),
                (
                    SourceUnitKey(33),
                    CompilationOrigin::Library(LibraryFileOrdinal::new(33)),
                    reverse_modules[0],
                ),
            ]
        );
        for (binder, modules) in [(&forward, forward_modules), (&reverse, reverse_modules)] {
            assert_library_function_namespace(
                binder,
                &modules,
                "FunctionFirst",
                [SourceUnitKey(30), SourceUnitKey(31)],
            );
            assert_library_function_namespace(
                binder,
                &modules,
                "NamespaceFirst",
                [SourceUnitKey(32), SourceUnitKey(33)],
            );
        }
    }

    #[test]
    fn library_class_namespace_identity_is_canonical_in_both_input_orders() {
        let class_allocator = Allocator::default();
        let namespace_allocator = Allocator::default();
        let class = Parser::new(
            &class_allocator,
            "declare class CanonicalClass {}",
            SourceType::d_ts(),
        )
        .parse();
        let namespace = Parser::new(
            &namespace_allocator,
            "declare namespace CanonicalClass { export const member: number; }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(class.diagnostics.is_empty());
        assert!(namespace.diagnostics.is_empty());
        let class_row = (
            &class.program,
            SourceUnitKey(800),
            LibraryFileOrdinal::new(80),
        );
        let namespace_row = (
            &namespace.program,
            SourceUnitKey(200),
            LibraryFileOrdinal::new(81),
        );
        let (forward, forward_modules) = bind_libraries(&[class_row, namespace_row]);
        let (reverse, reverse_modules) = bind_libraries(&[namespace_row, class_row]);
        assert_eq!(forward_modules[0], reverse_modules[1]);
        assert_eq!(forward_modules[1], reverse_modules[0]);

        let snapshot = |binder: &Binder| {
            let symbol = binder
                .graph
                .get(binder.compilation_global)
                .and_then(|scope| scope.lookup_local("CanonicalClass"))
                .expect("canonical class symbol");
            let row = binder.symbols.get(symbol).expect("canonical class row");
            let attachment = binder
                .namespace_value_attachment(binder.compilation_global, "CanonicalClass")
                .expect("canonical class namespace attachment");
            assert_eq!(
                attachment.disposition,
                NamespaceValueAttachmentDisposition::AdmittedClass
            );
            assert_eq!(attachment.symbol, symbol);
            assert_eq!(attachment.members.len(), 1);
            (
                symbol,
                row.value,
                row.ty,
                row.ns,
                declaration_sources(binder, symbol),
                attachment.members[0].value_storage,
            )
        };
        let forward_snapshot = snapshot(&forward);
        let reverse_snapshot = snapshot(&reverse);
        assert_eq!(forward_snapshot, reverse_snapshot);
        assert!(forward_snapshot.1.is_some());
        assert!(forward_snapshot.2.is_some());
        assert!(forward_snapshot.3.is_some());
        assert_eq!(forward_snapshot.4, [SourceUnitKey(800), SourceUnitKey(200)]);
        assert!(forward_snapshot.5.is_some());
    }

    #[test]
    fn library_standalone_namespace_reopenings_keep_canonical_storage_and_members() {
        let first_allocator = Allocator::default();
        let second_allocator = Allocator::default();
        let first = Parser::new(
            &first_allocator,
            "declare namespace CanonicalStandalone { export const first: number; }",
            SourceType::d_ts(),
        )
        .parse();
        let second = Parser::new(
            &second_allocator,
            "declare namespace CanonicalStandalone { export const second: string; }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(first.diagnostics.is_empty());
        assert!(second.diagnostics.is_empty());
        let first_row = (
            &first.program,
            SourceUnitKey(9_000),
            LibraryFileOrdinal::new(90),
        );
        let second_row = (
            &second.program,
            SourceUnitKey(1_000),
            LibraryFileOrdinal::new(91),
        );
        let (forward, forward_modules) = bind_libraries(&[first_row, second_row]);
        let (reverse, reverse_modules) = bind_libraries(&[second_row, first_row]);
        assert_eq!(forward_modules[0], reverse_modules[1]);
        assert_eq!(forward_modules[1], reverse_modules[0]);

        let snapshot = |binder: &Binder| {
            let namespace = binder
                .namespaces
                .namespaces()
                .find(|namespace| namespace.name == "CanonicalStandalone")
                .expect("canonical standalone namespace");
            let storage = binder
                .namespaces
                .standalone_value_storage(namespace.id)
                .expect("canonical standalone storage");
            let root = binder
                .symbols
                .get(namespace.symbol)
                .expect("canonical standalone root");
            assert_eq!(root.value, Some(storage));
            let attachment = binder
                .local_standalone_namespace_value_attachments()
                .into_iter()
                .find(|attachment| attachment.namespace == namespace.id)
                .expect("canonical standalone attachment");
            let members = attachment
                .members
                .iter()
                .map(|member| {
                    (
                        member.source,
                        member.name.map(str::to_string),
                        member.value_storage.expect("standalone member storage"),
                    )
                })
                .collect::<Vec<_>>();
            (namespace.id, namespace.symbol, storage, members)
        };
        let forward_snapshot = snapshot(&forward);
        let reverse_snapshot = snapshot(&reverse);
        assert_eq!(forward_snapshot, reverse_snapshot);
        assert_eq!(
            forward_snapshot.3,
            [
                (
                    SourceUnitKey(9_000),
                    Some("first".to_string()),
                    forward_snapshot.3[0].2,
                ),
                (
                    SourceUnitKey(1_000),
                    Some("second".to_string()),
                    forward_snapshot.3[1].2,
                ),
            ]
        );
        assert_ne!(forward_snapshot.3[0].2, forward_snapshot.3[1].2);
    }

    #[test]
    fn library_external_privates_stay_local_and_legal_global_merges_without_duplicates() {
        let script_allocator = Allocator::default();
        let module_allocator = Allocator::default();
        let script = Parser::new(
            &script_allocator,
            "interface SharedAugmentedShape { script: number; }",
            SourceType::d_ts(),
        )
        .parse();
        let module = Parser::new(
            &module_allocator,
            "export {}; interface ModulePrivateShape { privateMember: boolean; } declare const ModulePrivateValue: number; declare class ModulePrivateClass {} declare namespace ModulePrivateNamespace { export const tag: number; } declare global { interface SharedAugmentedShape { augmentation: string; } }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(script.diagnostics.is_empty());
        assert!(module.diagnostics.is_empty());
        let script_source = SourceUnitKey(40);
        let module_source = SourceUnitKey(41);
        let script_file = LibraryFileOrdinal::new(40);
        let module_file = LibraryFileOrdinal::new(41);
        let (binder, modules) = bind_libraries(&[
            (&script.program, script_source, script_file),
            (&module.program, module_source, module_file),
        ]);

        assert!(binder
            .resolve_type(modules[0], "ModulePrivateShape")
            .is_none());
        assert!(binder
            .resolve_value(modules[0], "ModulePrivateValue")
            .is_none());
        assert!(binder
            .resolve_value(modules[0], "ModulePrivateClass")
            .is_none());
        assert!(binder
            .resolve_type(modules[0], "ModulePrivateClass")
            .is_none());
        assert!(binder
            .resolve_value(modules[0], "ModulePrivateNamespace")
            .is_none());
        assert!(binder
            .graph
            .resolve(modules[0], "ModulePrivateNamespace")
            .is_none());
        let private = binder
            .resolve_type(modules[1], "ModulePrivateShape")
            .expect("external private type remains local");
        let private_group = binder
            .symbols
            .get(private)
            .and_then(|symbol| symbol.ty)
            .and_then(|group| binder.type_groups.get(group))
            .expect("private type group");
        assert_eq!(private_group.fragments.len(), 1);
        assert_eq!(private_group.fragments[0].source, module_source);
        for name in [
            "ModulePrivateValue",
            "ModulePrivateClass",
            "ModulePrivateNamespace",
        ] {
            assert!(
                binder.resolve_value(modules[1], name).is_some(),
                "external module keeps private value {name}"
            );
        }
        assert!(binder
            .resolve_type(modules[1], "ModulePrivateClass")
            .is_some());

        let shared_script = binder
            .resolve_type(modules[0], "SharedAugmentedShape")
            .expect("script sees augmented global");
        let shared_module = binder
            .resolve_type(modules[1], "SharedAugmentedShape")
            .expect("external module sees augmented global");
        assert_eq!(shared_script, shared_module);
        let shared_group = binder
            .symbols
            .get(shared_script)
            .and_then(|symbol| symbol.ty)
            .and_then(|group| binder.type_groups.get(group))
            .expect("shared global type group");
        assert_eq!(shared_group.fragments.len(), 2);
        assert_eq!(
            shared_group
                .fragments
                .iter()
                .map(|fragment| fragment.source)
                .collect::<Vec<_>>(),
            [script_source, module_source]
        );
        let global = binder.namespaces.globals().next().expect("global metadata");
        assert!(global.issues.is_empty());
        assert_eq!(
            binder
                .graph
                .get(global.overlay_scope)
                .and_then(|scope| scope.lookup_local("SharedAugmentedShape")),
            Some(shared_script)
        );
        assert_eq!(
            binder
                .namespaces
                .source_units()
                .map(|unit| (unit.source, unit.origin, unit.module))
                .collect::<Vec<_>>(),
            [
                (
                    script_source,
                    CompilationOrigin::Library(script_file),
                    modules[0],
                ),
                (
                    module_source,
                    CompilationOrigin::Library(module_file),
                    modules[1],
                ),
            ]
        );
    }

    #[test]
    fn library_script_declare_global_is_fail_closed() {
        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            "declare global { interface InvalidScriptGlobal {} }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(parsed.diagnostics.is_empty());
        let (binder, modules) = bind_libraries(&[(
            &parsed.program,
            SourceUnitKey(50),
            LibraryFileOrdinal::new(50),
        )]);
        assert!(binder
            .resolve_type(modules[0], "InvalidScriptGlobal")
            .is_none());
        assert!(binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local("InvalidScriptGlobal"))
            .is_none());
        let global = binder
            .namespaces
            .globals()
            .next()
            .expect("invalid global metadata");
        assert_eq!(global.issues, [GlobalIssue::FutureTk2669]);
        assert!(binder.declarations.iter().all(|declaration| {
            declaration.site.binding_span.start
                != u32::try_from("declare global { interface ".len()).expect("test offset fits u32")
                || declaration.type_group.is_none()
        }));
    }

    #[test]
    fn library_global_freeze_removes_non_admitted_names_and_blocks_overlay_fallback() {
        let script_allocator = Allocator::default();
        let module_allocator = Allocator::default();
        let script = Parser::new(
            &script_allocator,
            "interface RejectedGlobal {} type RejectedGlobal = string; declare var DeferredGlobal: number; declare var DeferredGlobal: number;",
            SourceType::d_ts(),
        )
        .parse();
        let module = Parser::new(
            &module_allocator,
            "export {}; declare global { interface LegalOverlayWitness {} }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(script.diagnostics.is_empty());
        assert!(module.diagnostics.is_empty());
        let (binder, modules) = bind_libraries(&[
            (
                &script.program,
                SourceUnitKey(60),
                LibraryFileOrdinal::new(60),
            ),
            (
                &module.program,
                SourceUnitKey(61),
                LibraryFileOrdinal::new(61),
            ),
        ]);
        for name in ["RejectedGlobal", "DeferredGlobal"] {
            assert!(binder
                .graph
                .get(binder.compilation_global)
                .and_then(|scope| scope.lookup_local(name))
                .is_none());
            assert!(binder.resolve_type(modules[0], name).is_none());
            assert!(binder.resolve_value(modules[0], name).is_none());
        }
        let rejected = binder
            .namespaces
            .merges()
            .find(|record| record.name.as_ref() == "RejectedGlobal")
            .expect("rejected global merge");
        assert_eq!(
            rejected.classification.disposition,
            MergeDisposition::RejectedRedeclaration
        );
        let deferred = binder
            .namespaces
            .merges()
            .find(|record| record.name.as_ref() == "DeferredGlobal")
            .expect("deferred global merge");
        assert_eq!(
            deferred.classification.disposition,
            MergeDisposition::DeferredBacklog15
        );
        let overlay = binder
            .namespaces
            .globals()
            .find(|global| global.issues.is_empty())
            .map(|global| global.overlay_scope)
            .expect("legal global overlay");
        for name in ["RejectedGlobal", "DeferredGlobal"] {
            let blocker = binder
                .graph
                .get(overlay)
                .and_then(|scope| scope.lookup_local(name))
                .and_then(|symbol| binder.symbols.get(symbol))
                .expect("non-admitted global blocker");
            assert!(blocker.value.is_none());
            assert!(blocker.ty.is_none());
            assert!(blocker.ns.is_none());
            assert!(blocker.blocks_value_lookup);
            assert!(blocker.blocks_type_lookup);
            assert!(blocker.blocks_namespace_lookup);
        }
        assert!(binder
            .graph
            .get(overlay)
            .and_then(|scope| scope.lookup_local("LegalOverlayWitness"))
            .is_some());
    }

    #[test]
    fn value_resolution_reports_namespace_provenance_without_a_second_scope_walk() {
        let binder = bind(
            r#"
const Outer = 1;
namespace Container {
    namespace Outer { export interface Shape {} }
    const witness = Outer;
}
namespace OnlyType { export interface Shape {} }
namespace Standalone { export const value = 1; }
"#,
        );
        let namespace = |name: &str| {
            binder
                .namespaces
                .namespaces()
                .find(|namespace| namespace.name == name)
                .unwrap_or_else(|| panic!("{name} namespace"))
        };

        let standalone = namespace("Standalone");
        let storage = binder
            .namespaces
            .standalone_value_storage(standalone.id)
            .expect("instantiated namespace storage");
        assert_eq!(
            binder.resolve_value_binding(binder.module, "Standalone"),
            ValueResolution::Resolved {
                symbol: standalone.symbol,
                kind: ResolvedValueKind::StandaloneNamespace {
                    namespace: standalone.id,
                    storage,
                },
            }
        );

        let only_type = namespace("OnlyType");
        assert_eq!(
            binder.resolve_value_binding(binder.module, "OnlyType"),
            ValueResolution::TypeOnlyNamespace {
                namespace: only_type.id,
            }
        );
        assert_eq!(binder.resolve_value(binder.module, "OnlyType"), None);

        let outer_symbol = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("Outer"))
            .expect("outer value symbol");
        let container_scope = namespace("Container")
            .fragments
            .first()
            .and_then(|fragment| binder.namespaces.fragment(*fragment))
            .map(|fragment| fragment.private_scope)
            .expect("container private scope");
        assert_eq!(
            binder.resolve_value_binding(container_scope, "Outer"),
            ValueResolution::Resolved {
                symbol: outer_symbol,
                kind: ResolvedValueKind::Ordinary,
            },
            "an inner type-only namespace must not hide an outer value"
        );
        assert_eq!(
            binder.resolve_value_binding(container_scope, "Missing"),
            ValueResolution::Missing
        );
    }

    #[test]
    fn lexical_declarations_and_storage_identities_do_not_alias() {
        fn lexical(_: DeclId) {}
        fn value_storage(_: ValueStorageId) {}
        fn type_group(_: crate::binder::declaration::TypeGroupId) {}

        let source = "const value = 0; function f(param: number, ...rest: string[]) { try {} catch (caught) {} } type Alias = number; interface Shape {} class Both {}";
        let binder = bind(source);
        assert_eq!(binder.declarations.len(), 8);

        let declarations: Vec<_> = binder.declarations.iter().collect();
        for (index, declaration) in declarations.iter().enumerate() {
            assert_eq!(declaration.id.index(), index);
            assert_eq!(declaration.site.module, binder.module);
            assert!(
                declaration.site.declaration_span.start < declaration.site.declaration_span.end
            );
            assert!(declaration.site.binding_span.start < declaration.site.binding_span.end);
            lexical(declaration.id);
        }
        assert!(declarations.iter().all(|declaration| {
            (declaration.value_storage.is_none() && declaration.type_group.is_none())
                || declaration.site.scope.is_some()
        }));

        let value = declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Variable)
            .expect("variable declaration");
        assert_eq!(&source[value.site.declaration_span.range()], "value = 0");
        assert_eq!(&source[value.site.binding_span.range()], "value");
        value_storage(value.value_storage.expect("variable value storage"));

        let parameters: Vec<_> = declarations
            .iter()
            .filter(|declaration| declaration.kind == DeclarationKind::Parameter)
            .collect();
        assert_eq!(parameters.len(), 2);
        assert_eq!(
            parameters
                .iter()
                .map(|declaration| &source[declaration.site.binding_span.range()])
                .collect::<Vec<_>>(),
            vec!["param", "rest"]
        );

        let caught = declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::CatchParameter)
            .expect("catch declaration");
        assert_eq!(&source[caught.site.declaration_span.range()], "caught");
        assert_eq!(&source[caught.site.binding_span.range()], "caught");

        for kind in [
            DeclarationKind::TypeAlias,
            DeclarationKind::Interface,
            DeclarationKind::Class,
        ] {
            let declaration = declarations
                .iter()
                .find(|declaration| declaration.kind == kind)
                .expect("type declaration");
            type_group(declaration.type_group.expect("type group identity"));
        }

        let class = declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Class)
            .expect("class declaration");
        value_storage(class.value_storage.expect("class value storage"));
        type_group(class.type_group.expect("class type group identity"));
        let class_symbol = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("Both"))
            .and_then(|symbol| binder.symbols.get(symbol))
            .expect("class symbol");
        assert_eq!(class_symbol.declarations, vec![class.id]);
    }

    #[test]
    fn type_groups_retain_every_fragment_in_source_order_behind_legacy_boundary() {
        let source = "export interface M { first: number } export class M {} export interface M { last: string }";
        let binder = bind(source);
        let symbol_id = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("M"))
            .expect("merged symbol");
        let symbol = binder.symbols.get(symbol_id).expect("merged symbol row");
        let group_id = symbol.ty.expect("type group");
        let group = binder.type_groups.get(group_id).expect("type group row");

        assert_eq!(group.name, "M");
        assert_eq!(
            group
                .fragments
                .iter()
                .map(|fragment| fragment.kind)
                .collect::<Vec<_>>(),
            vec![
                TypeFragmentKind::Interface,
                TypeFragmentKind::Class,
                TypeFragmentKind::Interface,
            ]
        );
        assert!(group
            .fragments
            .windows(2)
            .all(|pair| pair[0].site.declaration_span.start < pair[1].site.declaration_span.start));
        assert!(group.fragments.iter().all(|fragment| {
            binder
                .declarations
                .get(fragment.declaration)
                .is_some_and(|declaration| {
                    declaration.site == fragment.site
                        && declaration.type_group == Some(group_id)
                        && fragment.scope == declaration.site.scope.expect("bound type scope")
                })
        }));

        assert!(group.fragments.iter().all(
            |fragment| fragment.site.declaration_span.start < fragment.site.binding_span.start
        ));
        assert_eq!(
            group
                .fragments
                .iter()
                .map(|fragment| &source[fragment.site.binding_span.range()])
                .collect::<Vec<_>>(),
            vec!["M", "M", "M"]
        );

        assert_eq!(symbol.ty, Some(group_id));

        let class = group
            .fragments
            .iter()
            .find(|fragment| fragment.kind == TypeFragmentKind::Class)
            .and_then(|fragment| binder.declarations.get(fragment.declaration))
            .expect("class fragment declaration");
        assert!(class.value_storage.is_some());
    }

    #[test]
    fn source_prewalk_records_imports_and_every_nested_binding_leaf() {
        let source = "import Default, * as NS from 'pkg'; import type { Remote as Local } from './dep'; const { a, nested: { b = 1 }, ...objectRest } = value; const [c, , [d], ...arrayRest] = value; function f({ p: [q] }, [r, ...s], t = 1) {} try {} catch ({ e: [caught], ...catchRest }) {}";
        let binder = bind(source);
        let declarations: Vec<_> = binder
            .declarations
            .iter()
            .filter(|declaration| declaration.site.module == binder.module)
            .collect();

        let binding_names: Vec<_> = declarations
            .iter()
            .map(|declaration| &source[declaration.site.binding_span.range()])
            .collect();
        assert_eq!(
            binding_names,
            vec![
                "Default",
                "NS",
                "Local",
                "a",
                "b",
                "objectRest",
                "c",
                "d",
                "arrayRest",
                "f",
                "q",
                "r",
                "s",
                "t",
                "caught",
                "catchRest",
            ]
        );

        let imports: Vec<_> = declarations
            .iter()
            .filter(|declaration| declaration.kind == DeclarationKind::Import)
            .collect();
        assert_eq!(imports.len(), 3);
        assert_eq!(
            imports
                .iter()
                .map(|declaration| &source[declaration.site.declaration_span.range()])
                .collect::<Vec<_>>(),
            vec![
                "import Default, * as NS from 'pkg';",
                "import Default, * as NS from 'pkg';",
                "import type { Remote as Local } from './dep';",
            ]
        );
        assert!(imports.iter().all(|declaration| {
            declaration.value_storage.is_none() && declaration.type_group.is_none()
        }));

        let a = declarations
            .iter()
            .find(|declaration| source[declaration.site.binding_span.range()] == *"a")
            .expect("nested object leaf");
        let b = declarations
            .iter()
            .find(|declaration| source[declaration.site.binding_span.range()] == *"b")
            .expect("nested assignment leaf");
        assert_eq!(a.site.declaration_span, b.site.declaration_span);
        assert_eq!(
            &source[a.site.declaration_span.range()],
            "{ a, nested: { b = 1 }, ...objectRest } = value"
        );
        assert!(a.value_storage.is_none());
        assert!(b.value_storage.is_none());

        let supported_parameter = declarations
            .iter()
            .find(|declaration| source[declaration.site.binding_span.range()] == *"t")
            .expect("simple parameter");
        assert!(supported_parameter.value_storage.is_some());
        for name in ["q", "r", "s", "caught", "catchRest"] {
            let declaration = declarations
                .iter()
                .find(|declaration| source[declaration.site.binding_span.range()] == *name)
                .expect("destructured binding leaf");
            assert!(declaration.value_storage.is_none());
        }
    }

    #[test]
    fn semantic_walk_attaches_truthful_scopes_without_fabricating_unsupported_ones() {
        let source = "const { top, nested: [topNested] } = value; { let [blockLeaf] = value; type BlockType = number; function nested({ paramLeaf }, ...restParam) { try {} catch ({ caughtLeaf }) {} } } namespace Unsupported { export const hidden = 1; } export {}; declare global { interface GlobalShape {} }";
        let binder = bind(source);
        let outer_block_start = u32::try_from(source.find("{ let").unwrap()).unwrap();
        let function_start = u32::try_from(source.find("function nested").unwrap()).unwrap();
        let catch_start = u32::try_from(source.find("catch").unwrap()).unwrap();
        let block_scope = binder
            .block_scopes
            .get(&(binder.module, outer_block_start))
            .copied()
            .expect("outer block scope");
        let function_scope = binder
            .fn_scopes
            .get(&(binder.module, function_start))
            .copied()
            .expect("nested function scope");
        let catch_scope = binder
            .block_scopes
            .get(&(binder.module, catch_start))
            .copied()
            .expect("catch scope");

        let declaration = |name: &str| {
            binder
                .declarations
                .iter()
                .find(|declaration| &source[declaration.site.binding_span.range()] == name)
                .expect("source declaration")
        };
        for name in ["top", "topNested"] {
            assert_eq!(declaration(name).site.scope, Some(binder.module));
        }
        for name in ["blockLeaf", "BlockType", "nested"] {
            assert_eq!(declaration(name).site.scope, Some(block_scope));
        }
        for name in ["paramLeaf", "restParam"] {
            assert_eq!(declaration(name).site.scope, Some(function_scope));
        }
        assert_eq!(declaration("caughtLeaf").site.scope, Some(catch_scope));

        let namespace = declaration("Unsupported")
            .namespace
            .and_then(|namespace| binder.namespaces.get(namespace))
            .expect("dormant namespace metadata");
        let fragment = namespace
            .fragments
            .first()
            .and_then(|fragment| binder.namespaces.fragment(*fragment))
            .expect("namespace fragment");
        assert_eq!(declaration("Unsupported").site.scope, Some(binder.module));
        assert_eq!(
            declaration("hidden").site.scope,
            Some(fragment.private_scope)
        );
        assert_eq!(declaration("global").site.scope, Some(binder.module));
        let global_overlay = binder
            .namespaces
            .globals()
            .next()
            .expect("global augmentation")
            .overlay_scope;
        assert_eq!(declaration("GlobalShape").site.scope, Some(global_overlay));
        assert_eq!(declaration("Unsupported").kind, DeclarationKind::Namespace);
        assert_eq!(declaration("global").kind, DeclarationKind::Global);
    }

    /// WU2: every `case`/`default` clause binds into ONE switch-local lexical
    /// scope, and that scope does not leak into the enclosing function.
    #[test]
    fn switch_clauses_share_one_switch_local_scope() {
        let binder = bind(
            "function f(x: number) { \
               switch (x) { \
                 case 1: let a = 1; break; \
                 case 2: let b = 2; break; \
               } \
             }",
        );

        // The switch introduces exactly one block scope (no explicit `{ }` blocks
        // in this fixture), shared by both clauses.
        assert_eq!(binder.block_scopes.len(), 1, "one switch-local scope");
        let switch_scope = *binder.block_scopes.values().next().unwrap();
        let scope = binder.graph.get(switch_scope).unwrap();
        assert_eq!(scope.kind, ScopeKind::Block);

        // Both clause-local `let`s live directly in that same scope, as distinct
        // symbols — proving the clauses share ONE ScopeId.
        let a = scope.lookup_local("a").expect("a in switch scope");
        let b = scope.lookup_local("b").expect("b in switch scope");
        assert_ne!(a, b);

        // The switch-local names do not leak up to the enclosing function scope.
        let parent = binder.graph.get(scope.parent.unwrap()).unwrap();
        assert_eq!(parent.kind, ScopeKind::Function);
        assert!(parent.lookup_local("a").is_none());
        assert!(parent.lookup_local("b").is_none());
    }

    /// An explicit `{ }` block inside a clause still gets its own nested scope,
    /// child of the switch-local scope — its declarations do not reach the switch.
    #[test]
    fn explicit_block_in_clause_keeps_its_own_scope() {
        let binder = bind(
            "function f(x: number) { \
               switch (x) { \
                 case 1: { let inner = 1; } break; \
               } \
             }",
        );

        // Two block scopes: the switch-local one and the explicit `{ }` inside it.
        assert_eq!(binder.block_scopes.len(), 2);
        let inner_scope = binder
            .block_scopes
            .values()
            .find(|id| {
                binder
                    .graph
                    .get(**id)
                    .unwrap()
                    .lookup_local("inner")
                    .is_some()
            })
            .copied()
            .expect("inner block scope");
        // Its parent is a switch-local block scope, and `inner` is not in the switch.
        let parent = binder.graph.get(inner_scope).unwrap().parent.unwrap();
        assert_eq!(binder.graph.get(parent).unwrap().kind, ScopeKind::Block);
        assert!(binder
            .graph
            .get(parent)
            .unwrap()
            .lookup_local("inner")
            .is_none());
    }

    #[test]
    fn var_bindings_target_the_nearest_function_or_module_scope() {
        let binder = bind(
            "{ var module_var = 1; let module_let = 1; } \
             function outer() { \
               if (true) { var from_if = 1; } \
               for (var from_for = 0; false;) {} \
               for (var from_in in { key: 1 }) {} \
               for (var from_of of [1]) {} \
               while (false) { var from_while = 1; } \
               switch (1) { case 1: var from_switch = 1; break; } \
               { let block_let = 1; const block_const = 2; } \
               function inner() { { var inner_only = 1; } } \
             }",
        );

        let module_scope = binder.graph.get(binder.module).expect("module scope");
        assert!(module_scope.lookup_local("module_var").is_some());
        assert!(module_scope.lookup_local("module_let").is_none());

        let outer_scope = binder
            .fn_scopes
            .values()
            .copied()
            .find(|scope| {
                binder
                    .graph
                    .get(*scope)
                    .is_some_and(|scope| scope.lookup_local("from_if").is_some())
            })
            .expect("outer function scope");
        let outer = binder.graph.get(outer_scope).expect("outer scope");
        for name in [
            "from_if",
            "from_for",
            "from_in",
            "from_of",
            "from_while",
            "from_switch",
        ] {
            assert!(outer.lookup_local(name).is_some(), "{name} in outer scope");
        }
        assert!(outer.lookup_local("block_let").is_none());
        assert!(outer.lookup_local("block_const").is_none());
        assert!(outer.lookup_local("inner_only").is_none());

        let inner_scope = binder
            .fn_scopes
            .values()
            .copied()
            .find(|scope| {
                binder
                    .graph
                    .get(*scope)
                    .is_some_and(|scope| scope.lookup_local("inner_only").is_some())
            })
            .expect("inner function scope");
        assert_ne!(outer_scope, inner_scope);
    }

    /// Backlog 102: a delta write aimed at a library-owned row is refused, not dropped. Every
    /// refusal reaches the caller as a typed [`FrozenPrefixWrite`] naming its own declaration.
    #[test]
    fn frozen_prefix_writes_are_recorded_instead_of_dropped() {
        let library_allocator = Allocator::default();
        let library = Parser::new(
            &library_allocator,
            "declare var libraryValue: string;\ndeclare function libraryFn(input: string): string;\n",
            SourceType::d_ts(),
        )
        .parse();
        assert!(library.diagnostics.is_empty());
        let (mut base, _) = bind_libraries(&[(
            &library.program,
            SourceUnitKey(1),
            LibraryFileOrdinal::new(0),
        )]);
        base.freeze_as_base().expect("binder base freezes once");

        let user_allocator = Allocator::default();
        let user = Parser::new(
            &user_allocator,
            "declare var libraryValue: number;\ndeclare function libraryFn(a: number, b: number): number;\n",
            SourceType::ts(),
        )
        .parse();
        assert!(user.diagnostics.is_empty());
        let delta = base.fork_delta().expect("private binder delta");
        let (mut builder, source) = ProjectBinderBuilder::resume_frozen_library(delta);
        let unit = CompilationUnit::implementation(source, &user.program);
        builder.reserve_script_namespace_roots([(&user.program, unit)]);
        let (module, _) = builder.add_module(&user.program, &[], unit);
        let binder = builder
            .finish_frozen_library_continuation(module)
            .expect("colliding suffix still binds");

        let sites = binder
            .frozen_prefix_writes()
            .iter()
            .map(|write| write.site)
            .collect::<Vec<_>>();
        assert_eq!(
            sites,
            [
                FrozenPrefixWriteSite::ValueSlot,
                FrozenPrefixWriteSite::SymbolDeclarationList,
                FrozenPrefixWriteSite::FunctionValueSlot,
                FrozenPrefixWriteSite::SymbolDeclarationList,
            ]
        );
        assert!(
            binder
                .frozen_prefix_writes()
                .iter()
                .all(|write| write.declaration.is_some()),
            "every refusal names the declaration whose effect was refused"
        );
        // The base rows themselves are untouched: the library declarations still win.
        let value = binder
            .resolve_value(binder.module, "libraryValue")
            .expect("library value survives the refused write");
        assert!(value.index() < base.symbols.len());
    }

    /// The complementary half: a fresh script global publishes into the delta layer and refuses
    /// nothing, so the ledger stays empty on the ordinary path.
    #[test]
    fn fresh_script_globals_publish_without_refusing_a_frozen_write() {
        let library_allocator = Allocator::default();
        let library = Parser::new(
            &library_allocator,
            "declare var libraryValue: string;\n",
            SourceType::d_ts(),
        )
        .parse();
        assert!(library.diagnostics.is_empty());
        let (mut base, _) = bind_libraries(&[(
            &library.program,
            SourceUnitKey(1),
            LibraryFileOrdinal::new(0),
        )]);
        base.freeze_as_base().expect("binder base freezes once");

        let declare_allocator = Allocator::default();
        let declare = Parser::new(
            &declare_allocator,
            "declare var freshGlobal: number;\ninterface FreshShape { name: string }\n",
            SourceType::ts(),
        )
        .parse();
        let consume_allocator = Allocator::default();
        let consume = Parser::new(
            &consume_allocator,
            "declare const witness: FreshShape;\nconst read = freshGlobal;\n",
            SourceType::ts(),
        )
        .parse();
        assert!(declare.diagnostics.is_empty() && consume.diagnostics.is_empty());

        let delta = base.fork_delta().expect("private binder delta");
        let (mut builder, source) = ProjectBinderBuilder::resume_frozen_library(delta);
        let declare_unit = CompilationUnit::implementation(source, &declare.program);
        let consume_unit =
            CompilationUnit::implementation(SourceUnitKey(source.0 + 1), &consume.program);
        builder.reserve_script_namespace_roots([
            (&declare.program, declare_unit),
            (&consume.program, consume_unit),
        ]);
        let (_declare_module, _) = builder.add_module(&declare.program, &[], declare_unit);
        let (consume_module, _) = builder.add_module(&consume.program, &[], consume_unit);
        let binder = builder
            .finish_frozen_library_continuation(consume_module)
            .expect("fresh-global suffix binds");

        assert_eq!(binder.frozen_prefix_writes(), &[]);
        assert!(
            binder
                .resolve_value(consume_module, "freshGlobal")
                .is_some(),
            "a fresh script value global is visible from another file"
        );
        assert!(
            binder.resolve_type(consume_module, "FreshShape").is_some(),
            "a fresh script type global is visible from another file"
        );
    }

    /// A frozen library base with a type group, a bare value name, and a namespace — the three
    /// rows backlog 103's panic shapes reach.
    fn frozen_merge_target_base() -> (Binder, Allocator) {
        let library_allocator = Allocator::default();
        let library = Parser::new(
            &library_allocator,
            "interface LibraryShape { first: string }\n\
             declare var libraryValue: string;\n\
             declare namespace LibrarySpace { const inner: number; }\n",
            SourceType::d_ts(),
        )
        .parse();
        assert!(library.diagnostics.is_empty());
        let (mut base, _) = bind_libraries(&[(
            &library.program,
            SourceUnitKey(1),
            LibraryFileOrdinal::new(0),
        )]);
        base.freeze_as_base().expect("binder base freezes once");
        (base, library_allocator)
    }

    /// Backlog 103, the guard tier: every merge into a library-owned name is a recorded refusal
    /// with its own declaration, not a panic. A marker fixture only sees the downstream surface,
    /// so the ledger sequence is pinned directly.
    #[test]
    fn frozen_library_merges_are_refused_instead_of_panicking() {
        let (base, _library_allocator) = frozen_merge_target_base();

        // Each user shape reached a different `.expect` before the guard. The flag is whether the
        // entry names a user declaration, which is what turns it into an incomplete record.
        type RefusalCase<'a> = (&'a str, &'a str, &'a [(FrozenPrefixWriteSite, bool)]);
        let cases: &[RefusalCase<'_>] = &[
            (
                "an interface merging into a frozen type group",
                "interface LibraryShape { second: number }\n",
                &[
                    (FrozenPrefixWriteSite::TypeGroupFragment, true),
                    (FrozenPrefixWriteSite::TypeSlot, true),
                    (FrozenPrefixWriteSite::SymbolDeclarationList, true),
                ],
            ),
            (
                "an interface taking the type slot of a frozen value symbol",
                "interface libraryValue { probe: number }\n",
                &[
                    (FrozenPrefixWriteSite::TypeSlot, true),
                    (FrozenPrefixWriteSite::SymbolDeclarationList, true),
                ],
            ),
            (
                "a namespace merging into a frozen namespace",
                "declare namespace LibrarySpace { const extra: number; }\n",
                // The two trailing scope refusals are the frozen global-scope finalization, which
                // owns no declaration of its own (backlog 102); the namespace declaration above
                // them is what carries this run's refusal to the user.
                &[
                    (FrozenPrefixWriteSite::SymbolDeclarationList, true),
                    (FrozenPrefixWriteSite::NamespaceFragmentList, true),
                    (FrozenPrefixWriteSite::ScopeDeclaration, false),
                    (FrozenPrefixWriteSite::ScopeDeclaration, false),
                ],
            ),
        ];

        for (label, source, expected) in cases {
            let user_allocator = Allocator::default();
            let user = Parser::new(&user_allocator, source, SourceType::ts()).parse();
            assert!(user.diagnostics.is_empty(), "{label}: fixture parses");

            let delta = base.fork_delta().expect("private binder delta");
            let (mut builder, key) = ProjectBinderBuilder::resume_frozen_library(delta);
            let unit = CompilationUnit::implementation(key, &user.program);
            builder.reserve_script_namespace_roots([(&user.program, unit)]);
            let (module, _) = builder.add_module(&user.program, &[], unit);
            let binder = builder
                .finish_frozen_library_continuation(module)
                .expect("a colliding suffix still binds");

            let refusals = binder
                .frozen_prefix_writes()
                .iter()
                .map(|write| (write.site, write.declaration.is_some()))
                .collect::<Vec<_>>();
            assert_eq!(refusals, *expected, "{label}: refusal sequence");
        }
    }

    #[test]
    fn frozen_library_continuation_admits_declare_global() {
        let (base, _library_allocator) = frozen_merge_target_base();

        for source in [
            "export {};\ndeclare global { interface LibraryShape { second: number } }\n",
            "export {};\ndeclare global { interface B103Brand { tag: string } }\n",
            "export {};\ndeclare global { var b103Counter: number; }\n",
        ] {
            let user_allocator = Allocator::default();
            let user = Parser::new(&user_allocator, source, SourceType::ts()).parse();
            assert!(user.diagnostics.is_empty(), "fixture parses: {source}");

            let delta = base.fork_delta().expect("private binder delta");
            let (mut builder, key) = ProjectBinderBuilder::resume_frozen_library(delta);
            let unit = CompilationUnit::implementation(key, &user.program);
            builder.reserve_script_namespace_roots([(&user.program, unit)]);
            let (module, _) = builder.add_module(&user.program, &[], unit);
            let result = builder.finish_frozen_library_continuation(module);
            assert!(
                result.is_ok(),
                "declare global must bind: {source}: {:?}",
                result.err()
            );
        }
    }
}

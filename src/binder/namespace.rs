//! Namespace, merge, global-augmentation, and UMD context metadata.
//!
//! Lexical namespace declarations remain independent of checker storage. Standalone groups keep
//! dormant owner slots in dense side columns, while admitted class/function augmentations reuse
//! the ordinary value binder for exported members.

use crate::binder::bind::{
    bind_class_declaration, bind_declarator, bind_function_declaration, declare_type, BindState,
    Binder,
};
use crate::binder::declaration::{
    DeclId, DeclarationKind, DeclarationSite, TypeFragmentKind, TypeGroupId, ValueStorageId,
};
use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{Symbol, SymbolId, SymbolTable};
#[cfg(test)]
use crate::source::LibraryFileOrdinal;
use crate::source::{CompilationOrigin, OriginalModuleOrdinal};
use crate::span::Span;
use crate::types::layered::{LayeredMap, LayeredVec};
use oxc_ast::ast::{
    Declaration, ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName, Program,
    Statement, TSModuleDeclaration, TSModuleDeclarationBody, TSModuleDeclarationName,
    TSModuleReference, VariableDeclarationKind,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

#[cfg(test)]
thread_local! {
    static CONTINUATION_MERGE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_INSTANCE_FRAGMENT_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_CHILD_FRAGMENT_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_ALLOCATION_NAMESPACE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_ATTACHMENT_NAMESPACE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_FRAGMENT_SCOPE_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_PLACEMENT_MERGE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_GLOBAL_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_UMD_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_AMBIENT_ALIAS_MEMBER_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_UMD_STATEMENT_QUERIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_GLOBAL_STATEMENT_QUERIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_LIBRARY_SOURCE_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CONTINUATION_LIBRARY_REPORTING_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NamespaceContinuationWorkForTest {
    pub(crate) merge_rows: u64,
    pub(crate) instance_fragment_rows: u64,
    pub(crate) child_fragment_lookups: u64,
    pub(crate) allocation_namespace_rows: u64,
    pub(crate) attachment_namespace_rows: u64,
    pub(crate) fragment_scope_lookups: u64,
    pub(crate) placement_merge_rows: u64,
    pub(crate) global_rows: u64,
    pub(crate) umd_rows: u64,
    pub(crate) ambient_alias_member_rows: u64,
    pub(crate) umd_statement_queries: u64,
    pub(crate) global_statement_queries: u64,
    pub(crate) library_source_lookups: u64,
    pub(crate) library_reporting_lookups: u64,
}

#[cfg(test)]
fn namespace_continuation_work_for_test() -> NamespaceContinuationWorkForTest {
    NamespaceContinuationWorkForTest {
        merge_rows: CONTINUATION_MERGE_ROWS.get(),
        instance_fragment_rows: CONTINUATION_INSTANCE_FRAGMENT_ROWS.get(),
        child_fragment_lookups: CONTINUATION_CHILD_FRAGMENT_LOOKUPS.get(),
        allocation_namespace_rows: CONTINUATION_ALLOCATION_NAMESPACE_ROWS.get(),
        attachment_namespace_rows: CONTINUATION_ATTACHMENT_NAMESPACE_ROWS.get(),
        fragment_scope_lookups: CONTINUATION_FRAGMENT_SCOPE_LOOKUPS.get(),
        placement_merge_rows: CONTINUATION_PLACEMENT_MERGE_ROWS.get(),
        global_rows: CONTINUATION_GLOBAL_ROWS.get(),
        umd_rows: CONTINUATION_UMD_ROWS.get(),
        ambient_alias_member_rows: CONTINUATION_AMBIENT_ALIAS_MEMBER_ROWS.get(),
        umd_statement_queries: CONTINUATION_UMD_STATEMENT_QUERIES.get(),
        global_statement_queries: CONTINUATION_GLOBAL_STATEMENT_QUERIES.get(),
        library_source_lookups: CONTINUATION_LIBRARY_SOURCE_LOOKUPS.get(),
        library_reporting_lookups: CONTINUATION_LIBRARY_REPORTING_LOOKUPS.get(),
    }
}

#[cfg(test)]
fn record_continuation_merge_row() {
    CONTINUATION_MERGE_ROWS.set(CONTINUATION_MERGE_ROWS.get() + 1);
}

#[cfg(test)]
fn record_continuation_instance_fragment_row() {
    CONTINUATION_INSTANCE_FRAGMENT_ROWS.set(CONTINUATION_INSTANCE_FRAGMENT_ROWS.get() + 1);
}

#[cfg(test)]
fn record_continuation_child_fragment_lookup() {
    CONTINUATION_CHILD_FRAGMENT_LOOKUPS.set(CONTINUATION_CHILD_FRAGMENT_LOOKUPS.get() + 1);
}

#[cfg(test)]
fn record_continuation_allocation_namespace_row() {
    CONTINUATION_ALLOCATION_NAMESPACE_ROWS.set(CONTINUATION_ALLOCATION_NAMESPACE_ROWS.get() + 1);
}

#[cfg(test)]
fn record_continuation_attachment_namespace_row() {
    CONTINUATION_ATTACHMENT_NAMESPACE_ROWS.set(CONTINUATION_ATTACHMENT_NAMESPACE_ROWS.get() + 1);
}

#[cfg(test)]
fn record_continuation_fragment_scope_lookup() {
    CONTINUATION_FRAGMENT_SCOPE_LOOKUPS.set(CONTINUATION_FRAGMENT_SCOPE_LOOKUPS.get() + 1);
}

#[cfg(test)]
fn record_continuation_placement_merge_row() {
    CONTINUATION_PLACEMENT_MERGE_ROWS.set(CONTINUATION_PLACEMENT_MERGE_ROWS.get() + 1);
}

#[cfg(test)]
fn record_continuation_global_row() {
    CONTINUATION_GLOBAL_ROWS.set(CONTINUATION_GLOBAL_ROWS.get() + 1);
}

#[cfg(test)]
fn record_continuation_umd_row() {
    CONTINUATION_UMD_ROWS.set(CONTINUATION_UMD_ROWS.get() + 1);
}

#[cfg(test)]
fn record_continuation_ambient_alias_member_row() {
    CONTINUATION_AMBIENT_ALIAS_MEMBER_ROWS.set(CONTINUATION_AMBIENT_ALIAS_MEMBER_ROWS.get() + 1);
}

#[cfg(test)]
fn record_continuation_umd_statement_query() {
    CONTINUATION_UMD_STATEMENT_QUERIES.set(CONTINUATION_UMD_STATEMENT_QUERIES.get() + 1);
}

#[cfg(test)]
fn record_continuation_global_statement_query() {
    CONTINUATION_GLOBAL_STATEMENT_QUERIES.set(CONTINUATION_GLOBAL_STATEMENT_QUERIES.get() + 1);
}

#[cfg(test)]
fn record_continuation_library_source_lookup() {
    CONTINUATION_LIBRARY_SOURCE_LOOKUPS.set(CONTINUATION_LIBRARY_SOURCE_LOOKUPS.get() + 1);
}

#[cfg(test)]
fn record_continuation_library_reporting_lookup() {
    CONTINUATION_LIBRARY_REPORTING_LOOKUPS.set(CONTINUATION_LIBRARY_REPORTING_LOOKUPS.get() + 1);
}

#[cfg(test)]
pub(crate) struct NamespaceContinuationWorkScopeForTest(NamespaceContinuationWorkForTest);

#[cfg(test)]
impl NamespaceContinuationWorkScopeForTest {
    pub(crate) fn start() -> Self {
        Self(namespace_continuation_work_for_test())
    }

    pub(crate) fn finish(self) -> NamespaceContinuationWorkForTest {
        let end = namespace_continuation_work_for_test();
        NamespaceContinuationWorkForTest {
            merge_rows: end.merge_rows.saturating_sub(self.0.merge_rows),
            instance_fragment_rows: end
                .instance_fragment_rows
                .saturating_sub(self.0.instance_fragment_rows),
            child_fragment_lookups: end
                .child_fragment_lookups
                .saturating_sub(self.0.child_fragment_lookups),
            allocation_namespace_rows: end
                .allocation_namespace_rows
                .saturating_sub(self.0.allocation_namespace_rows),
            attachment_namespace_rows: end
                .attachment_namespace_rows
                .saturating_sub(self.0.attachment_namespace_rows),
            fragment_scope_lookups: end
                .fragment_scope_lookups
                .saturating_sub(self.0.fragment_scope_lookups),
            placement_merge_rows: end
                .placement_merge_rows
                .saturating_sub(self.0.placement_merge_rows),
            global_rows: end.global_rows.saturating_sub(self.0.global_rows),
            umd_rows: end.umd_rows.saturating_sub(self.0.umd_rows),
            ambient_alias_member_rows: end
                .ambient_alias_member_rows
                .saturating_sub(self.0.ambient_alias_member_rows),
            umd_statement_queries: end
                .umd_statement_queries
                .saturating_sub(self.0.umd_statement_queries),
            global_statement_queries: end
                .global_statement_queries
                .saturating_sub(self.0.global_statement_queries),
            library_source_lookups: end
                .library_source_lookups
                .saturating_sub(self.0.library_source_lookups),
            library_reporting_lookups: end
                .library_reporting_lookups
                .saturating_sub(self.0.library_reporting_lookups),
        }
    }
}

#[cfg(test)]
thread_local! {
    static PLACEMENT_ROW_PROBES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Placement participant rows visited to resolve one declaration — the merge-group rows
/// `push_placement` compares, plus every by-declaration syntax read it answers.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PlacementLookupWorkForTest {
    row_probes: u64,
}

#[cfg(test)]
fn placement_lookup_work_for_test() -> PlacementLookupWorkForTest {
    PlacementLookupWorkForTest {
        row_probes: PLACEMENT_ROW_PROBES.get(),
    }
}

#[cfg(test)]
fn record_placement_row_probe() {
    PLACEMENT_ROW_PROBES.set(PLACEMENT_ROW_PROBES.get() + 1);
}

#[cfg(test)]
struct PlacementLookupWorkScopeForTest(PlacementLookupWorkForTest);

#[cfg(test)]
impl PlacementLookupWorkScopeForTest {
    fn start() -> Self {
        Self(placement_lookup_work_for_test())
    }

    fn finish(self) -> PlacementLookupWorkForTest {
        let end = placement_lookup_work_for_test();
        PlacementLookupWorkForTest {
            row_probes: end.row_probes.saturating_sub(self.0.row_probes),
        }
    }
}

#[cfg(test)]
thread_local! {
    static FINALIZATION_CLASSIFICATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINALIZATION_MERGE_PARTICIPANT_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINALIZATION_MERGE_INDEX_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINALIZATION_ATTACHMENT_MERGE_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FINALIZATION_ALIAS_MEMBER_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Project-wide rows re-processed by namespace finalization. Every field is derived from
/// the accumulated project, so a per-module finalization pass makes each of them grow with
/// the file split at constant program size.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NamespaceFinalizationWorkForTest {
    /// Times `classify` rebuilt the canonical indexes over the whole accumulated project.
    pub(crate) classifications: u64,
    /// Placement participants cloned, sorted and classified into the merges vector.
    pub(crate) merge_participant_rows: u64,
    /// Merge records re-keyed into `merge_indices`, each cloning its name.
    pub(crate) merge_index_rows: u64,
    /// Merge records scanned to fill namespace value attachments.
    pub(crate) attachment_merge_rows: u64,
    /// Member rows scanned to resolve local ambient export alias targets.
    pub(crate) alias_member_rows: u64,
}

#[cfg(test)]
fn namespace_finalization_work_for_test() -> NamespaceFinalizationWorkForTest {
    NamespaceFinalizationWorkForTest {
        classifications: FINALIZATION_CLASSIFICATIONS.get(),
        merge_participant_rows: FINALIZATION_MERGE_PARTICIPANT_ROWS.get(),
        merge_index_rows: FINALIZATION_MERGE_INDEX_ROWS.get(),
        attachment_merge_rows: FINALIZATION_ATTACHMENT_MERGE_ROWS.get(),
        alias_member_rows: FINALIZATION_ALIAS_MEMBER_ROWS.get(),
    }
}

#[cfg(test)]
fn record_finalization_classification() {
    FINALIZATION_CLASSIFICATIONS.set(FINALIZATION_CLASSIFICATIONS.get() + 1);
}

#[cfg(test)]
fn record_finalization_merge_participant_rows(rows: usize) {
    FINALIZATION_MERGE_PARTICIPANT_ROWS.set(
        FINALIZATION_MERGE_PARTICIPANT_ROWS
            .get()
            .saturating_add(u64::try_from(rows).unwrap_or(u64::MAX)),
    );
}

#[cfg(test)]
fn record_finalization_merge_index_row() {
    FINALIZATION_MERGE_INDEX_ROWS.set(FINALIZATION_MERGE_INDEX_ROWS.get() + 1);
}

#[cfg(test)]
fn record_finalization_attachment_merge_row() {
    FINALIZATION_ATTACHMENT_MERGE_ROWS.set(FINALIZATION_ATTACHMENT_MERGE_ROWS.get() + 1);
}

#[cfg(test)]
fn record_finalization_alias_member_rows(rows: usize) {
    FINALIZATION_ALIAS_MEMBER_ROWS.set(
        FINALIZATION_ALIAS_MEMBER_ROWS
            .get()
            .saturating_add(u64::try_from(rows).unwrap_or(u64::MAX)),
    );
}

#[cfg(test)]
pub(crate) struct NamespaceFinalizationWorkScopeForTest(NamespaceFinalizationWorkForTest);

#[cfg(test)]
impl NamespaceFinalizationWorkScopeForTest {
    pub(crate) fn start() -> Self {
        Self(namespace_finalization_work_for_test())
    }

    pub(crate) fn finish(self) -> NamespaceFinalizationWorkForTest {
        let end = namespace_finalization_work_for_test();
        NamespaceFinalizationWorkForTest {
            classifications: end.classifications.saturating_sub(self.0.classifications),
            merge_participant_rows: end
                .merge_participant_rows
                .saturating_sub(self.0.merge_participant_rows),
            merge_index_rows: end.merge_index_rows.saturating_sub(self.0.merge_index_rows),
            attachment_merge_rows: end
                .attachment_merge_rows
                .saturating_sub(self.0.attachment_merge_rows),
            alias_member_rows: end
                .alias_member_rows
                .saturating_sub(self.0.alias_member_rows),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NamespaceId(pub u32);

impl NamespaceId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("namespace id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NamespaceFragmentId(pub u32);

impl NamespaceFragmentId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("namespace fragment id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct NamespaceMemberId(pub(crate) u32);

impl NamespaceMemberId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("namespace member id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct GlobalAugmentationId(pub(crate) u32);

impl GlobalAugmentationId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("global augmentation id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DeferredModuleId(pub u32);

impl DeferredModuleId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("deferred module id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct ExportContextId(pub(crate) u32);

impl ExportContextId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("export context id fits usize")
    }
}

/// Run-stable project source ordering key. Project mode derives it from normalized paths.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct SourceUnitKey(pub(crate) u32);

impl SourceUnitKey {
    pub(crate) const PRELUDE: Self = Self(0);
    pub(crate) const SINGLE_SOURCE: Self = Self(1);
}

pub(crate) type ExactKey = SourceUnitKey;

pub(crate) const fn exact_key(index: u32) -> ExactKey {
    SourceUnitKey(index)
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum SourceFileKind {
    ImplementationTs,
    ImplementationMts,
    ImplementationCts,
    DeclarationTs,
    DeclarationMts,
    DeclarationCts,
}

impl SourceFileKind {
    pub fn is_declaration(self) -> bool {
        !matches!(
            self,
            Self::ImplementationTs | Self::ImplementationMts | Self::ImplementationCts
        )
    }

    fn implies_external_module(self) -> bool {
        matches!(self, Self::ImplementationMts | Self::ImplementationCts)
    }
}

/// Canonical filename classifier shared by parsing, binding, and source-only preflight.
pub(crate) fn source_file_kind(name: &str) -> SourceFileKind {
    if name.ends_with(".d.mts") {
        SourceFileKind::DeclarationMts
    } else if name.ends_with(".d.cts") {
        SourceFileKind::DeclarationCts
    } else if name.ends_with(".d.ts") {
        SourceFileKind::DeclarationTs
    } else if name.ends_with(".mts") {
        SourceFileKind::ImplementationMts
    } else if name.ends_with(".cts") {
        SourceFileKind::ImplementationCts
    } else {
        SourceFileKind::ImplementationTs
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ModuleBindingContext {
    pub source_file_kind: SourceFileKind,
    pub external_module: bool,
}

impl Default for ModuleBindingContext {
    fn default() -> Self {
        Self {
            source_file_kind: SourceFileKind::ImplementationTs,
            external_module: false,
        }
    }
}

impl ModuleBindingContext {
    pub fn for_program(program: &Program<'_>, source_file_kind: SourceFileKind) -> Self {
        Self {
            source_file_kind,
            external_module: source_file_kind.implies_external_module()
                || has_external_module_indicator(program),
        }
    }

    pub fn declaration_file(self) -> bool {
        self.source_file_kind.is_declaration()
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct CompilationUnit {
    pub(crate) source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub(crate) binding: ModuleBindingContext,
}

impl CompilationUnit {
    pub(crate) fn implementation(source: SourceUnitKey, program: &Program<'_>) -> Self {
        Self {
            source,
            origin: CompilationOrigin::User(OriginalModuleOrdinal::new(0)),
            binding: ModuleBindingContext::for_program(program, SourceFileKind::ImplementationTs),
        }
    }

    #[cfg(test)]
    pub(crate) fn library(
        source: SourceUnitKey,
        file_ordinal: LibraryFileOrdinal,
        program: &Program<'_>,
    ) -> Self {
        Self {
            source,
            origin: CompilationOrigin::Library(file_ordinal),
            binding: ModuleBindingContext::for_program(program, SourceFileKind::DeclarationTs),
        }
    }
}

#[derive(Default)]
struct ImportMetaVisitor {
    found: bool,
}

impl<'a> Visit<'a> for ImportMetaVisitor {
    fn visit_meta_property(&mut self, property: &oxc_ast::ast::MetaProperty<'a>) {
        if property.meta.name == "import" && property.property.name == "meta" {
            self.found = true;
        }
        walk::walk_meta_property(self, property);
    }
}

/// Structural external-module classification; parser source goals are intentionally ignored.
pub fn has_external_module_indicator(program: &Program<'_>) -> bool {
    let top_level_indicator = program.body.iter().any(|statement| match statement {
        Statement::ImportDeclaration(_)
        | Statement::ExportAllDeclaration(_)
        | Statement::ExportDefaultDeclaration(_)
        | Statement::ExportNamedDeclaration(_)
        | Statement::TSExportAssignment(_) => true,
        Statement::TSImportEqualsDeclaration(declaration) => matches!(
            declaration.module_reference,
            TSModuleReference::ExternalModuleReference(_)
        ),
        Statement::TSNamespaceExportDeclaration(_) => false,
        _ => false,
    });
    if top_level_indicator {
        return true;
    }
    let mut visitor = ImportMetaVisitor::default();
    visitor.visit_program(program);
    visitor.found
}

/// Typed identity owner; text paths and spans never participate in namespace identity.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NamespaceOwner {
    Lexical(ScopeId),
    NamespacePublic(NamespaceId),
    FragmentPrivate(NamespaceFragmentId),
    CompilationGlobal,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NamespacePublication {
    Private,
    Explicit,
    AmbientDefault,
    DottedImplicit,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Namespace {
    pub id: NamespaceId,
    pub owner: NamespaceOwner,
    pub name: String,
    pub public_scope: ScopeId,
    pub symbol: SymbolId,
    pub fragments: Vec<NamespaceFragmentId>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct NamespaceFragment {
    pub id: NamespaceFragmentId,
    pub namespace: NamespaceId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub source_start: u32,
    pub module: ScopeId,
    pub private_scope: ScopeId,
    pub lexical_parent: ScopeId,
    pub public_scope: ScopeId,
    pub ambient: bool,
    pub publication: NamespacePublication,
    pub instance_state: NamespaceInstanceState,
    pub members: Vec<NamespaceMemberId>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeclarationOwner {
    Lexical(ScopeId),
    NamespacePublic(NamespaceId),
    NamespacePrivate(NamespaceFragmentId),
    CompilationGlobal,
    DeferredAmbientModule(DeferredModuleId),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum NamespaceMemberOwner {
    Fragment(NamespaceFragmentId),
    GlobalAugmentation(GlobalAugmentationId),
    DeferredAmbientModule(DeferredModuleId),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum MetadataName {
    Identifier(String),
    StringLiteral(String),
}

impl MetadataName {
    pub fn text(&self) -> &str {
        match self {
            Self::Identifier(name) | Self::StringLiteral(name) => name,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AliasContext {
    ValidAmbient,
    InvalidFutureTk1194,
    InvalidAugmentationFutureTk2666,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AliasSpaceIntent {
    Type,
    UnresolvedValueOrType,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum QualifiedTypePathDeferredReason {
    Import,
    Enum,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum QualifiedTypePathResolution {
    TypeGroup(TypeGroupId),
    MissingRoot {
        segment: usize,
    },
    TypeOnlyRoot {
        segment: usize,
    },
    MissingMember {
        segment: usize,
    },
    TypeOnlyIntermediate {
        segment: usize,
    },
    ValueOnlyLeaf {
        segment: usize,
    },
    Unavailable {
        segment: usize,
    },
    Deferred {
        segment: usize,
        reason: QualifiedTypePathDeferredReason,
    },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct LocalAmbientExportAliasFailure {
    pub(crate) origin: CompilationOrigin,
    pub local_span: Span,
    pub local_name: String,
    pub kind: LocalAmbientExportAliasFailureKind,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum LocalAmbientExportAliasFailureKind {
    Missing,
    NonLocal,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NamespaceInstanceState {
    NonInstantiated,
    Instantiated,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum VariableKind {
    Var,
    Let,
    Const,
    Using,
    AwaitUsing,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ImportBindingForm {
    Named,
    Default,
    Namespace,
    ImportEquals,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ImportSyntaxFacts {
    pub form: ImportBindingForm,
    pub outer_type_only: bool,
    pub specifier_type_only: bool,
    pub external_reference: bool,
    pub exported: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeclarationSyntaxFacts {
    None,
    Variable(VariableKind),
    Import(ImportSyntaxFacts),
    Enum { constant: bool },
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeclarationSpaces {
    pub value: bool,
    pub r#type: bool,
    pub namespace: bool,
}

impl DeclarationSpaces {
    const NONE: Self = Self {
        value: false,
        r#type: false,
        namespace: false,
    };
    const VALUE: Self = Self {
        value: true,
        r#type: false,
        namespace: false,
    };
    const TYPE: Self = Self {
        value: false,
        r#type: true,
        namespace: false,
    };
    const VALUE_TYPE: Self = Self {
        value: true,
        r#type: true,
        namespace: false,
    };
    const ALIAS: Self = Self {
        value: true,
        r#type: true,
        namespace: false,
    };
    const NAMESPACE: Self = Self {
        value: false,
        r#type: false,
        namespace: true,
    };
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MergeDeclarationKind {
    Variable,
    Function,
    Class,
    TypeAlias,
    Interface,
    Enum,
    Namespace,
    ImportAlias,
    DeferredExport,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct NamespaceMember {
    pub id: NamespaceMemberId,
    pub owner: NamespaceMemberOwner,
    pub target: DeclarationOwner,
    pub declaration: Option<DeclId>,
    pub symbol: Option<SymbolId>,
    pub local_symbol: Option<SymbolId>,
    pub name: Option<String>,
    pub local_name: Option<MetadataName>,
    pub exported_name: Option<MetadataName>,
    pub declaration_span: Span,
    pub specifier_span: Option<Span>,
    pub binding_span: Span,
    pub local_span: Option<Span>,
    pub exported_span: Option<Span>,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub module_specifier: Option<MetadataName>,
    pub outer_type_only: bool,
    pub specifier_type_only: bool,
    pub alias_context: Option<AliasContext>,
    pub alias_resolution: Option<ExportResolutionDisposition>,
    pub alias_space_intent: Option<AliasSpaceIntent>,
    pub export_context: Option<ExportContextId>,
    pub syntax: DeclarationSyntaxFacts,
    pub spaces: DeclarationSpaces,
    pub kind: MergeDeclarationKind,
    pub publication: NamespacePublication,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GlobalOwner {
    Lexical(ScopeId),
    NamespaceFragment(NamespaceFragmentId),
    DeferredAmbientModule(DeferredModuleId),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GlobalPlacement {
    DirectExternalModule,
    DirectScript,
    DeferredAmbientModule,
    NestedNamespace,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum GlobalIssue {
    FutureTk2669,
    FutureTk2670,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct GlobalAugmentation {
    pub id: GlobalAugmentationId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub module: ScopeId,
    pub owner: GlobalOwner,
    pub body_span: Span,
    pub diagnostic_span: Span,
    pub target_scope: ScopeId,
    pub overlay_scope: ScopeId,
    pub placement: GlobalPlacement,
    pub issues: Vec<GlobalIssue>,
    pub declared: bool,
    pub members: Vec<NamespaceMemberId>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct DeferredAmbientModule {
    pub id: DeferredModuleId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub module: ScopeId,
    pub owner: DeclarationOwner,
    pub kind: DeferredModuleKind,
    pub specifier: String,
    pub span: Span,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeferredModuleKind {
    AmbientExternalModule,
    ModuleAugmentation,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeferredChildKind {
    OrdinaryDeclaration,
    ExportDeclaration,
    NamespaceDeclaration,
    DeferredExport,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct DeferredAmbientChild {
    pub module: DeferredModuleId,
    pub declaration: Option<DeclId>,
    pub kind: DeferredChildKind,
    pub name: Option<MetadataName>,
    pub span: Span,
    pub binding_span: Option<Span>,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ExportContextOwner {
    NamespaceFragment(NamespaceFragmentId),
    GlobalAugmentation(GlobalAugmentationId),
    DeferredAmbientModule(DeferredModuleId),
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ExportContextKind {
    NamedList,
    WrappedDeclaration,
    ExportAll,
    ExportDefault,
    ExportAssignment,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ExportSyntaxDisposition {
    Valid,
    FutureTk1194,
    FutureTk1319,
    FutureTk1063,
    FutureTk2666,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ExportResolutionDisposition {
    NotRequired,
    DeferredBacklog15,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ExportContext {
    pub id: ExportContextId,
    pub owner: ExportContextOwner,
    pub kind: ExportContextKind,
    pub syntax: ExportSyntaxDisposition,
    pub resolution: ExportResolutionDisposition,
    pub has_module_specifier: bool,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub span: Span,
    pub members: Vec<NamespaceMemberId>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum UmdContext {
    FutureTk1316Nested,
    FutureTk1314NonExternal,
    FutureTk1315Implementation,
    DeferredValidBacklog15,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct UmdNamespaceExport {
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub module: ScopeId,
    pub owner: DeclarationOwner,
    pub name: String,
    pub span: Span,
    pub context: UmdContext,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MergeSlotDisposition {
    Empty,
    Single,
    AdmittedMerge,
    Deferred,
    Rejected,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MergeSlotState {
    pub declarations: usize,
    pub disposition: MergeSlotDisposition,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MergeSlotSummary {
    pub value: MergeSlotState,
    pub r#type: MergeSlotState,
    pub namespace: MergeSlotState,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MergeCompositionKind {
    IndependentOccupiedSlots,
    InterfaceGroup,
    FunctionGroup,
    VariableGroup,
    NamespaceGroup,
    ClassInterface,
    FunctionNamespace,
    ClassNamespace,
    InterfaceNamespace,
    VariableNamespaceNonInstantiated,
    VariableNamespaceRuntime,
    ImportNamespace(ImportBindingForm),
    EnumComposition,
    ConflictingValueDeclarations,
    ConflictingTypeDeclarations,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MergeDisposition {
    Admitted,
    DeferredBacklog15,
    DeferredBacklog42,
    RejectedFutureTk2440,
    RejectedRedeclaration,
    RejectedRuntimeNamespace,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct MergeComposition {
    pub kind: MergeCompositionKind,
    pub disposition: MergeDisposition,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MergeClassification {
    pub slots: MergeSlotSummary,
    pub compositions: Vec<MergeComposition>,
    pub disposition: MergeDisposition,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PlacementIssueKind {
    FutureTk2434,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct PlacementIssue {
    pub kind: PlacementIssueKind,
    pub owner: DeclId,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub span: Span,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct MergeParticipant {
    pub declaration: DeclId,
    pub kind: MergeDeclarationKind,
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub span: Span,
    pub ambient: bool,
    pub spaces: DeclarationSpaces,
    pub syntax: DeclarationSyntaxFacts,
    pub namespace_fragment: Option<NamespaceFragmentId>,
    pub namespace_instance: Option<NamespaceInstanceState>,
    pub binding_span: Span,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MergeRecord {
    pub owner: DeclarationOwner,
    pub name: String,
    pub(crate) declarations: Vec<MergeParticipant>,
    pub classification: MergeClassification,
    pub(crate) placement_issues: Vec<PlacementIssue>,
}

/// Whole-group decision consumed by the class/function namespace lanes.
///
/// The decision is intentionally derived from the aggregate classification, never
/// from one apparently legal pair. The exact enum/function/namespace chimera keeps
/// its callable recovery separate from admitted publication.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum NamespaceValueAttachmentDisposition {
    AdmittedFunction,
    AdmittedClass,
    DeferredFunctionBacklog42,
    TypeContainerOnly,
    Rejected(MergeDisposition),
}

/// One exported namespace value declaration exposed to an owner draft or typed recovery.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct AttachedNamespaceValueMember<'a> {
    pub(crate) member: NamespaceMemberId,
    pub(crate) declaration: DeclId,
    pub(crate) name: &'a str,
    pub(crate) source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub(crate) scope: ScopeId,
    pub(crate) site: DeclarationSite,
    pub(crate) value_storage: Option<ValueStorageId>,
    pub(crate) symbol: Option<SymbolId>,
    pub(crate) kind: MergeDeclarationKind,
    pub(crate) variable_kind: Option<VariableKind>,
    pub(crate) publication: NamespacePublication,
    pub(crate) ambient: bool,
}

/// Frozen binder view of all namespace fragments attached to one same-name group.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct NamespaceValueAttachment<'a> {
    pub(crate) owner: DeclarationOwner,
    pub(crate) name: &'a str,
    pub(crate) symbol: SymbolId,
    pub(crate) disposition: NamespaceValueAttachmentDisposition,
    pub(crate) fragments: Vec<&'a NamespaceFragment>,
    pub(crate) members: Vec<AttachedNamespaceValueMember<'a>>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct StandaloneNamespaceValueMember<'a> {
    pub(crate) member: NamespaceMemberId,
    pub(crate) declaration: Option<DeclId>,
    pub(crate) name: Option<&'a str>,
    pub(crate) source: SourceUnitKey,
    pub(crate) site: Option<DeclarationSite>,
    pub(crate) declaration_span: Span,
    pub(crate) local_span: Option<Span>,
    pub(crate) origin: CompilationOrigin,
    pub(crate) value_storage: Option<ValueStorageId>,
    pub(crate) alias_target_storage: Option<ValueStorageId>,
    pub(crate) ambient: bool,
    pub(crate) child_namespace: Option<NamespaceId>,
    pub(crate) kind: MergeDeclarationKind,
    pub(crate) publication: NamespacePublication,
    pub(crate) spaces: DeclarationSpaces,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct StandaloneNamespaceValueAttachment<'a> {
    pub(crate) namespace: NamespaceId,
    pub(crate) storage: ValueStorageId,
    pub(crate) symbol: SymbolId,
    pub(crate) fragments: Vec<&'a NamespaceFragment>,
    pub(crate) members: Vec<StandaloneNamespaceValueMember<'a>>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct SourceUnitRecord {
    pub source: SourceUnitKey,
    pub(crate) origin: CompilationOrigin,
    pub module: ScopeId,
    pub context: ModuleBindingContext,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct NamespaceKey {
    owner: NamespaceOwner,
    name: String,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct MergeKey {
    owner: DeclarationOwner,
    name: String,
}

/// Canonical namespace, merge, ambient, and global publication metadata.
#[derive(Default)]
pub struct NamespaceTable {
    namespaces: LayeredVec<Namespace>,
    aggregate_instance_states: LayeredVec<NamespaceInstanceState>,
    standalone_value_storages: LayeredVec<Option<ValueStorageId>>,
    fragments: LayeredVec<NamespaceFragment>,
    members: LayeredVec<NamespaceMember>,
    namespace_keys: LayeredMap<NamespaceKey, NamespaceId>,
    canonical_namespaces: LayeredVec<NamespaceId>,
    placements: LayeredMap<MergeKey, Vec<MergeParticipant>>,
    merges: LayeredVec<MergeRecord>,
    merge_indices: LayeredMap<MergeKey, usize>,
    standalone_storage_namespaces: LayeredMap<ValueStorageId, NamespaceId>,
    declaration_owners_by_scope: LayeredMap<ScopeId, DeclarationOwner>,
    fragments_by_declaration: LayeredMap<DeclId, NamespaceFragmentId>,
    fragment_private_scopes_by_site: LayeredMap<(ScopeId, u32), ScopeId>,
    global_augmentations_by_site: LayeredMap<(ScopeId, u32), GlobalAugmentationId>,
    umd_exports_by_site: LayeredMap<(ScopeId, u32), usize>,
    source_keys_by_module: LayeredMap<ScopeId, SourceUnitKey>,
    library_export_default_sites: LayeredMap<(SourceUnitKey, u32), bool>,
    library_module_reporting_sites: LayeredMap<(ScopeId, u32), bool>,
    globals: LayeredVec<GlobalAugmentation>,
    deferred_modules: LayeredVec<DeferredAmbientModule>,
    deferred_children: LayeredVec<DeferredAmbientChild>,
    umd_exports: LayeredVec<UmdNamespaceExport>,
    export_contexts: LayeredVec<ExportContext>,
    source_units: LayeredVec<SourceUnitRecord>,
    canonical_source_units: LayeredVec<usize>,
    canonical_globals: LayeredVec<GlobalAugmentationId>,
    canonical_deferred_modules: LayeredVec<DeferredModuleId>,
    canonical_deferred_children: LayeredVec<usize>,
    canonical_umd_exports: LayeredVec<usize>,
    canonical_export_contexts: LayeredVec<ExportContextId>,
    compilation_global: Option<ScopeId>,
    script_namespace_root: Option<ScopeId>,
    library_shared_globals: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct NamespaceSnapshotPrimary {
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) standalone_value_storages: Vec<Option<ValueStorageId>>,
    pub(crate) fragments: Vec<NamespaceFragment>,
    pub(crate) members: Vec<NamespaceMember>,
    pub(crate) placements: Vec<(DeclarationOwner, String, Vec<MergeParticipant>)>,
    pub(crate) globals: Vec<GlobalAugmentation>,
    pub(crate) deferred_modules: Vec<DeferredAmbientModule>,
    pub(crate) deferred_children: Vec<DeferredAmbientChild>,
    pub(crate) umd_exports: Vec<UmdNamespaceExport>,
    pub(crate) export_contexts: Vec<ExportContext>,
    pub(crate) source_units: Vec<SourceUnitRecord>,
    pub(crate) compilation_global: Option<ScopeId>,
    pub(crate) script_namespace_root: Option<ScopeId>,
    pub(crate) library_shared_globals: bool,
}

#[derive(Copy, Clone, Default)]
pub(crate) struct NamespaceReferenceOffsets {
    pub(crate) placements: usize,
    pub(crate) deferred_children: usize,
    pub(crate) umd_exports: usize,
    pub(crate) canonical_namespaces: usize,
    pub(crate) canonical_source_units: usize,
    pub(crate) canonical_globals: usize,
    pub(crate) canonical_deferred_modules: usize,
    pub(crate) canonical_deferred_children: usize,
    pub(crate) canonical_umd_exports: usize,
    pub(crate) canonical_export_contexts: usize,
}

pub(crate) struct NamespaceReferenceRows {
    pub(crate) primary: NamespaceSnapshotPrimary,
    pub(crate) offsets: NamespaceReferenceOffsets,
    pub(crate) canonical_namespaces: Vec<u32>,
    pub(crate) canonical_source_units: Vec<u32>,
    pub(crate) canonical_globals: Vec<u32>,
    pub(crate) canonical_deferred_modules: Vec<u32>,
    pub(crate) canonical_deferred_children: Vec<u32>,
    pub(crate) canonical_umd_exports: Vec<u32>,
    pub(crate) canonical_export_contexts: Vec<u32>,
}

impl NamespaceTable {
    #[cfg(test)]
    pub(crate) fn global_augmentation_count(&self) -> usize {
        self.globals.len()
    }

    fn uses_library_shared_globals(&self) -> bool {
        self.library_shared_globals
    }

    pub fn get(&self, id: NamespaceId) -> Option<&Namespace> {
        self.namespaces.get(id.index())
    }

    pub(crate) fn fragment(&self, id: NamespaceFragmentId) -> Option<&NamespaceFragment> {
        self.fragments.get(id.index())
    }

    pub(crate) fn member(&self, id: NamespaceMemberId) -> Option<&NamespaceMember> {
        self.members.get(id.index())
    }

    pub fn namespaces(&self) -> impl Iterator<Item = &Namespace> {
        self.canonical_namespaces
            .iter()
            .filter_map(|id| self.namespaces.get(id.index()))
    }

    /// Whole-group instantiation after joining every reopening.
    pub(crate) fn aggregate_instance_state(
        &self,
        id: NamespaceId,
    ) -> Option<NamespaceInstanceState> {
        self.aggregate_instance_states.get(id.index()).copied()
    }

    /// Dormant owner slot for an admitted instantiated standalone namespace.
    pub(crate) fn standalone_value_storage(&self, id: NamespaceId) -> Option<ValueStorageId> {
        self.standalone_value_storages
            .get(id.index())
            .copied()
            .flatten()
    }

    #[cfg(test)]
    fn fragments(&self) -> impl Iterator<Item = &NamespaceFragment> {
        self.fragments.iter()
    }

    pub(crate) fn members(&self) -> impl Iterator<Item = &NamespaceMember> {
        self.members.iter()
    }

    pub fn merges(&self) -> impl Iterator<Item = &MergeRecord> {
        self.merges.iter().inspect(|_| {
            #[cfg(test)]
            record_continuation_merge_row();
        })
    }

    pub(crate) fn local_merges(&self) -> impl Iterator<Item = &MergeRecord> {
        self.merges.local_iter().inspect(|_| {
            #[cfg(test)]
            record_continuation_merge_row();
        })
    }

    pub(crate) fn is_admitted_compilation_global_name(&self, name: &str) -> bool {
        let key = MergeKey {
            owner: DeclarationOwner::CompilationGlobal,
            name: name.to_owned(),
        };
        self.merge_indices
            .get(&key)
            .and_then(|index| self.merges.get(*index))
            .is_some_and(|record| record.classification.disposition == MergeDisposition::Admitted)
    }

    /// Exact source-ordered placement outcomes ready for checker emission.
    pub(crate) fn placement_issues(&self) -> impl Iterator<Item = &PlacementIssue> {
        let mut issues = self
            .merges
            .iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_placement_merge_row();
            })
            .flat_map(|record| record.placement_issues.iter())
            .collect::<Vec<_>>();
        issues.sort_by_key(|issue| (issue.source, issue.origin, issue.span.start, issue.owner.0));
        issues.into_iter()
    }

    pub(crate) fn local_placement_issues(&self) -> impl Iterator<Item = &PlacementIssue> {
        let mut issues = self
            .merges
            .local_iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_placement_merge_row();
            })
            .flat_map(|record| record.placement_issues.iter())
            .collect::<Vec<_>>();
        issues.sort_by_key(|issue| (issue.source, issue.origin, issue.span.start, issue.owner.0));
        issues.into_iter()
    }

    #[cfg(test)]
    pub(crate) fn source_units(&self) -> impl Iterator<Item = &SourceUnitRecord> {
        self.canonical_source_units
            .iter()
            .filter_map(|index| self.source_units.get(*index))
    }

    #[cfg(test)]
    pub(crate) fn compilation_origin_for_source(
        &self,
        source: SourceUnitKey,
    ) -> Option<CompilationOrigin> {
        self.source_units
            .iter()
            .rev()
            .find(|unit| unit.source == source)
            .map(|unit| unit.origin)
    }

    pub(crate) fn globals(&self) -> impl Iterator<Item = &GlobalAugmentation> {
        self.canonical_globals
            .iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_global_row();
            })
            .filter_map(|id| self.globals.get(id.index()))
    }

    pub(crate) fn local_globals(&self) -> impl Iterator<Item = &GlobalAugmentation> {
        self.canonical_globals
            .local_iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_global_row();
            })
            .filter_map(|id| self.globals.get(id.index()))
    }

    /// Freeze legal global publication, then connect every user module in one cutover.
    pub(crate) fn finalize_global_scopes(&self, graph: &mut ScopeGraph, symbols: &mut SymbolTable) {
        let compilation_global = self
            .compilation_global
            .expect("compilation-global scope allocated");
        let script_namespace_root = self
            .script_namespace_root
            .expect("script namespace root scope allocated");
        let script_root = graph
            .get(script_namespace_root)
            .expect("script namespace root scope exists");
        assert_eq!(script_root.kind, ScopeKind::ScriptNamespaceRoot);
        assert_eq!(script_root.parent, Some(compilation_global));
        let mut unsafe_names = rustc_hash::FxHashSet::default();
        for record in self
            .merges
            .iter()
            .filter(|record| record.owner == DeclarationOwner::CompilationGlobal)
        {
            let safe = if self.uses_library_shared_globals() {
                record.classification.disposition == MergeDisposition::Admitted
            } else {
                record
                    .declarations
                    .iter()
                    .all(|participant| match participant.kind {
                        MergeDeclarationKind::Interface | MergeDeclarationKind::TypeAlias => true,
                        MergeDeclarationKind::Namespace => {
                            participant.namespace_instance
                                == Some(NamespaceInstanceState::NonInstantiated)
                        }
                        _ => false,
                    })
            };
            if !safe {
                unsafe_names.insert(record.name.clone());
            }
        }

        let (safe_symbols, blocked_names) = {
            let global = graph
                .get_mut(compilation_global)
                .expect("compilation-global scope exists");
            let mut blocked_names = Vec::new();
            for name in unsafe_names {
                if let Some(previous) = global.symbols.remove(&name) {
                    blocked_names.push((name, previous));
                }
            }
            (
                global
                    .symbols
                    .iter()
                    .map(|(name, symbol)| (name.clone(), *symbol))
                    .collect::<Vec<_>>(),
                blocked_names,
            )
        };

        // Prevent deferred globals from falling through to module-local names.
        let blocked_symbols = blocked_names
            .into_iter()
            .map(|(name, previous)| {
                let mut symbol = Symbol::new(name.clone());
                symbol.blocks_type_lookup = true;
                symbol.blocks_value_lookup = true;
                symbol.blocks_namespace_lookup = true;
                (name, previous, symbols.push(symbol))
            })
            .collect::<Vec<_>>();

        for global in self
            .globals
            .iter()
            .filter(|global| global.issues.is_empty())
        {
            for (name, symbol) in &safe_symbols {
                let replaced = graph.declare(global.overlay_scope, name.clone(), *symbol);
                assert_idempotent_overlay_publication(
                    replaced,
                    *symbol,
                    "global overlay cannot replace a different frozen symbol",
                );
            }
            for (name, previous, symbol) in &blocked_symbols {
                let replaced = graph.declare(global.overlay_scope, name.clone(), *symbol);
                assert!(
                    replaced.is_none_or(|existing| existing == *symbol || existing == *previous),
                    "global overlay blocker cannot replace an unrelated symbol"
                );
            }
        }

        for unit in &self.source_units {
            let module = graph
                .get_mut(unit.module)
                .expect("user module scope exists");
            assert_eq!(module.kind, ScopeKind::Module);
            module.parent = Some(script_namespace_root);
        }
    }

    #[cfg(test)]
    pub(crate) fn deferred_modules(&self) -> impl Iterator<Item = &DeferredAmbientModule> {
        self.canonical_deferred_modules
            .iter()
            .filter_map(|id| self.deferred_modules.get(id.index()))
    }

    #[cfg(test)]
    pub(crate) fn deferred_children(&self) -> impl Iterator<Item = &DeferredAmbientChild> {
        self.canonical_deferred_children
            .iter()
            .filter_map(|index| self.deferred_children.get(*index))
    }

    pub(crate) fn umd_exports(&self) -> impl Iterator<Item = &UmdNamespaceExport> {
        self.canonical_umd_exports
            .iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_umd_row();
            })
            .filter_map(|index| self.umd_exports.get(*index))
    }

    pub(crate) fn local_umd_exports(&self) -> impl Iterator<Item = &UmdNamespaceExport> {
        self.canonical_umd_exports
            .local_iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_umd_row();
            })
            .filter_map(|index| self.umd_exports.get(*index))
    }

    pub(crate) fn export_contexts(&self) -> impl Iterator<Item = &ExportContext> {
        self.canonical_export_contexts
            .iter()
            .filter_map(|id| self.export_contexts.get(id.index()))
    }

    pub fn len(&self) -> usize {
        self.namespaces.len()
    }

    #[cfg(test)]
    pub(crate) fn local_namespaces(&self) -> impl Iterator<Item = (NamespaceId, &Namespace)> {
        let base_len = self.namespaces.base_len();
        self.namespaces
            .local_iter()
            .enumerate()
            .map(move |(index, namespace)| {
                let id = u32::try_from(base_len + index).expect("namespace id fits u32");
                (NamespaceId(id), namespace)
            })
    }

    pub fn is_empty(&self) -> bool {
        self.namespaces.is_empty()
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.namespaces.freeze_as_base()?;
        self.aggregate_instance_states.freeze_as_base()?;
        self.standalone_value_storages.freeze_as_base()?;
        self.fragments.freeze_as_base()?;
        self.members.freeze_as_base()?;
        self.namespace_keys.freeze_as_base()?;
        self.canonical_namespaces.freeze_as_base()?;
        self.placements.freeze_as_base()?;
        self.merges.freeze_as_base()?;
        self.merge_indices.freeze_as_base()?;
        self.standalone_storage_namespaces.freeze_as_base()?;
        self.declaration_owners_by_scope.freeze_as_base()?;
        self.fragments_by_declaration.freeze_as_base()?;
        self.fragment_private_scopes_by_site.freeze_as_base()?;
        self.global_augmentations_by_site.freeze_as_base()?;
        self.umd_exports_by_site.freeze_as_base()?;
        self.source_keys_by_module.freeze_as_base()?;
        self.library_export_default_sites.freeze_as_base()?;
        self.library_module_reporting_sites.freeze_as_base()?;
        self.globals.freeze_as_base()?;
        self.deferred_modules.freeze_as_base()?;
        self.deferred_children.freeze_as_base()?;
        self.umd_exports.freeze_as_base()?;
        self.export_contexts.freeze_as_base()?;
        self.source_units.freeze_as_base()?;
        self.canonical_source_units.freeze_as_base()?;
        self.canonical_globals.freeze_as_base()?;
        self.canonical_deferred_modules.freeze_as_base()?;
        self.canonical_deferred_children.freeze_as_base()?;
        self.canonical_umd_exports.freeze_as_base()?;
        self.canonical_export_contexts.freeze_as_base()?;
        Ok(())
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            namespaces: self.namespaces.fork_delta()?,
            aggregate_instance_states: self.aggregate_instance_states.fork_delta()?,
            standalone_value_storages: self.standalone_value_storages.fork_delta()?,
            fragments: self.fragments.fork_delta()?,
            members: self.members.fork_delta()?,
            namespace_keys: self.namespace_keys.fork_delta()?,
            canonical_namespaces: self.canonical_namespaces.fork_delta()?,
            placements: self.placements.fork_delta()?,
            merges: self.merges.fork_delta()?,
            merge_indices: self.merge_indices.fork_delta()?,
            standalone_storage_namespaces: self.standalone_storage_namespaces.fork_delta()?,
            declaration_owners_by_scope: self.declaration_owners_by_scope.fork_delta()?,
            fragments_by_declaration: self.fragments_by_declaration.fork_delta()?,
            fragment_private_scopes_by_site: self.fragment_private_scopes_by_site.fork_delta()?,
            global_augmentations_by_site: self.global_augmentations_by_site.fork_delta()?,
            umd_exports_by_site: self.umd_exports_by_site.fork_delta()?,
            source_keys_by_module: self.source_keys_by_module.fork_delta()?,
            library_export_default_sites: self.library_export_default_sites.fork_delta()?,
            library_module_reporting_sites: self.library_module_reporting_sites.fork_delta()?,
            globals: self.globals.fork_delta()?,
            deferred_modules: self.deferred_modules.fork_delta()?,
            deferred_children: self.deferred_children.fork_delta()?,
            umd_exports: self.umd_exports.fork_delta()?,
            export_contexts: self.export_contexts.fork_delta()?,
            source_units: self.source_units.fork_delta()?,
            canonical_source_units: self.canonical_source_units.fork_delta()?,
            canonical_globals: self.canonical_globals.fork_delta()?,
            canonical_deferred_modules: self.canonical_deferred_modules.fork_delta()?,
            canonical_deferred_children: self.canonical_deferred_children.fork_delta()?,
            canonical_umd_exports: self.canonical_umd_exports.fork_delta()?,
            canonical_export_contexts: self.canonical_export_contexts.fork_delta()?,
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_shared_globals: self.library_shared_globals,
        })
    }

    #[cfg(test)]
    pub(crate) fn shares_base_storage_with(&self, other: &Self) -> bool {
        self.namespaces.shares_base_with(&other.namespaces)
            && self
                .aggregate_instance_states
                .shares_base_with(&other.aggregate_instance_states)
            && self
                .standalone_value_storages
                .shares_base_with(&other.standalone_value_storages)
            && self.fragments.shares_base_with(&other.fragments)
            && self.members.shares_base_with(&other.members)
            && self.namespace_keys.shares_base_with(&other.namespace_keys)
            && self
                .canonical_namespaces
                .shares_base_with(&other.canonical_namespaces)
            && self.placements.shares_base_with(&other.placements)
            && self.merges.shares_base_with(&other.merges)
            && self.merge_indices.shares_base_with(&other.merge_indices)
            && self
                .standalone_storage_namespaces
                .shares_base_with(&other.standalone_storage_namespaces)
            && self
                .declaration_owners_by_scope
                .shares_base_with(&other.declaration_owners_by_scope)
            && self
                .fragments_by_declaration
                .shares_base_with(&other.fragments_by_declaration)
            && self
                .fragment_private_scopes_by_site
                .shares_base_with(&other.fragment_private_scopes_by_site)
            && self
                .global_augmentations_by_site
                .shares_base_with(&other.global_augmentations_by_site)
            && self
                .umd_exports_by_site
                .shares_base_with(&other.umd_exports_by_site)
            && self
                .source_keys_by_module
                .shares_base_with(&other.source_keys_by_module)
            && self
                .library_export_default_sites
                .shares_base_with(&other.library_export_default_sites)
            && self
                .library_module_reporting_sites
                .shares_base_with(&other.library_module_reporting_sites)
            && self.globals.shares_base_with(&other.globals)
            && self
                .deferred_modules
                .shares_base_with(&other.deferred_modules)
            && self
                .deferred_children
                .shares_base_with(&other.deferred_children)
            && self.umd_exports.shares_base_with(&other.umd_exports)
            && self
                .export_contexts
                .shares_base_with(&other.export_contexts)
            && self.source_units.shares_base_with(&other.source_units)
            && self
                .canonical_source_units
                .shares_base_with(&other.canonical_source_units)
            && self
                .canonical_globals
                .shares_base_with(&other.canonical_globals)
            && self
                .canonical_deferred_modules
                .shares_base_with(&other.canonical_deferred_modules)
            && self
                .canonical_deferred_children
                .shares_base_with(&other.canonical_deferred_children)
            && self
                .canonical_umd_exports
                .shares_base_with(&other.canonical_umd_exports)
            && self
                .canonical_export_contexts
                .shares_base_with(&other.canonical_export_contexts)
    }

    #[cfg(test)]
    pub(crate) fn base_family_sharing_with(&self, other: &Self) -> [bool; 2] {
        let indexes = self
            .aggregate_instance_states
            .shares_base_with(&other.aggregate_instance_states)
            && self
                .standalone_value_storages
                .shares_base_with(&other.standalone_value_storages)
            && self.fragments.shares_base_with(&other.fragments)
            && self.members.shares_base_with(&other.members)
            && self.namespace_keys.shares_base_with(&other.namespace_keys)
            && self
                .canonical_namespaces
                .shares_base_with(&other.canonical_namespaces)
            && self.placements.shares_base_with(&other.placements)
            && self.merges.shares_base_with(&other.merges)
            && self.merge_indices.shares_base_with(&other.merge_indices)
            && self
                .standalone_storage_namespaces
                .shares_base_with(&other.standalone_storage_namespaces)
            && self
                .declaration_owners_by_scope
                .shares_base_with(&other.declaration_owners_by_scope)
            && self
                .fragments_by_declaration
                .shares_base_with(&other.fragments_by_declaration)
            && self
                .fragment_private_scopes_by_site
                .shares_base_with(&other.fragment_private_scopes_by_site)
            && self.globals.shares_base_with(&other.globals)
            && self
                .deferred_modules
                .shares_base_with(&other.deferred_modules)
            && self
                .deferred_children
                .shares_base_with(&other.deferred_children)
            && self.umd_exports.shares_base_with(&other.umd_exports)
            && self
                .export_contexts
                .shares_base_with(&other.export_contexts)
            && self.source_units.shares_base_with(&other.source_units)
            && self
                .canonical_source_units
                .shares_base_with(&other.canonical_source_units)
            && self
                .canonical_globals
                .shares_base_with(&other.canonical_globals)
            && self
                .canonical_deferred_modules
                .shares_base_with(&other.canonical_deferred_modules)
            && self
                .canonical_deferred_children
                .shares_base_with(&other.canonical_deferred_children)
            && self
                .canonical_umd_exports
                .shares_base_with(&other.canonical_umd_exports)
            && self
                .canonical_export_contexts
                .shares_base_with(&other.canonical_export_contexts)
            && self
                .global_augmentations_by_site
                .shares_base_with(&other.global_augmentations_by_site)
            && self
                .umd_exports_by_site
                .shares_base_with(&other.umd_exports_by_site)
            && self
                .source_keys_by_module
                .shares_base_with(&other.source_keys_by_module)
            && self
                .library_export_default_sites
                .shares_base_with(&other.library_export_default_sites)
            && self
                .library_module_reporting_sites
                .shares_base_with(&other.library_module_reporting_sites);
        [self.namespaces.shares_base_with(&other.namespaces), indexes]
    }

    #[cfg(test)]
    pub(crate) fn local_family_row_counts_for_test(&self) -> [usize; 2] {
        let indexes = self.aggregate_instance_states.local_len()
            + self.standalone_value_storages.local_len()
            + self.fragments.local_len()
            + self.members.local_len()
            + self.namespace_keys.local_len()
            + self.canonical_namespaces.local_len()
            + self.placements.local_len()
            + self.merges.local_len()
            + self.merge_indices.local_len()
            + self.standalone_storage_namespaces.local_len()
            + self.declaration_owners_by_scope.local_len()
            + self.fragments_by_declaration.local_len()
            + self.fragment_private_scopes_by_site.local_len()
            + self.globals.local_len()
            + self.deferred_modules.local_len()
            + self.deferred_children.local_len()
            + self.umd_exports.local_len()
            + self.export_contexts.local_len()
            + self.source_units.local_len()
            + self.canonical_source_units.local_len()
            + self.canonical_globals.local_len()
            + self.canonical_deferred_modules.local_len()
            + self.canonical_deferred_children.local_len()
            + self.canonical_umd_exports.local_len()
            + self.canonical_export_contexts.local_len()
            + self.global_augmentations_by_site.local_len()
            + self.umd_exports_by_site.local_len()
            + self.source_keys_by_module.local_len()
            + self.library_export_default_sites.local_len()
            + self.library_module_reporting_sites.local_len();
        [self.namespaces.local_len(), indexes]
    }

    pub(crate) fn snapshot_primary(&self) -> NamespaceSnapshotPrimary {
        NamespaceSnapshotPrimary {
            namespaces: self.namespaces.iter().cloned().collect(),
            standalone_value_storages: self.standalone_value_storages.iter().copied().collect(),
            fragments: self.fragments.iter().cloned().collect(),
            members: self.members.iter().cloned().collect(),
            placements: self
                .merges
                .iter()
                .map(|merge| (merge.owner, merge.name.clone(), merge.declarations.clone()))
                .collect(),
            globals: self.globals.iter().cloned().collect(),
            deferred_modules: self.deferred_modules.iter().cloned().collect(),
            deferred_children: self.deferred_children.iter().cloned().collect(),
            umd_exports: self.umd_exports.iter().cloned().collect(),
            export_contexts: self.export_contexts.iter().cloned().collect(),
            source_units: self.source_units.iter().cloned().collect(),
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_shared_globals: self.library_shared_globals,
        }
    }

    pub(crate) fn snapshot_reference_rows(&self, local_only: bool) -> NamespaceReferenceRows {
        if !local_only {
            return NamespaceReferenceRows {
                primary: self.snapshot_primary(),
                offsets: NamespaceReferenceOffsets::default(),
                canonical_namespaces: self
                    .canonical_namespaces
                    .iter()
                    .map(|namespace| namespace.0)
                    .collect(),
                canonical_source_units: self
                    .canonical_source_units
                    .iter()
                    .map(|index| self.source_units[*index].source.0)
                    .collect(),
                canonical_globals: self
                    .canonical_globals
                    .iter()
                    .map(|global| global.0)
                    .collect(),
                canonical_deferred_modules: self
                    .canonical_deferred_modules
                    .iter()
                    .map(|module| module.0)
                    .collect(),
                canonical_deferred_children: self
                    .canonical_deferred_children
                    .iter()
                    .map(|index| u32::try_from(*index).expect("deferred child index fits u32"))
                    .collect(),
                canonical_umd_exports: self
                    .canonical_umd_exports
                    .iter()
                    .map(|index| u32::try_from(*index).expect("UMD export index fits u32"))
                    .collect(),
                canonical_export_contexts: self
                    .canonical_export_contexts
                    .iter()
                    .map(|context| context.0)
                    .collect(),
            };
        }

        let primary = NamespaceSnapshotPrimary {
            namespaces: self.namespaces.local_iter().cloned().collect(),
            standalone_value_storages: self
                .standalone_value_storages
                .local_iter()
                .copied()
                .collect(),
            fragments: self.fragments.local_iter().cloned().collect(),
            members: self.members.local_iter().cloned().collect(),
            placements: self
                .merges
                .local_iter()
                .map(|merge| (merge.owner, merge.name.clone(), merge.declarations.clone()))
                .collect(),
            globals: self.globals.local_iter().cloned().collect(),
            deferred_modules: self.deferred_modules.local_iter().cloned().collect(),
            deferred_children: self.deferred_children.local_iter().cloned().collect(),
            umd_exports: self.umd_exports.local_iter().cloned().collect(),
            export_contexts: self.export_contexts.local_iter().cloned().collect(),
            source_units: self.source_units.local_iter().cloned().collect(),
            compilation_global: self.compilation_global,
            script_namespace_root: self.script_namespace_root,
            library_shared_globals: self.library_shared_globals,
        };
        NamespaceReferenceRows {
            primary,
            offsets: NamespaceReferenceOffsets {
                placements: self.merges.base_len(),
                deferred_children: self.deferred_children.base_len(),
                umd_exports: self.umd_exports.base_len(),
                canonical_namespaces: self.canonical_namespaces.base_len(),
                canonical_source_units: self.canonical_source_units.base_len(),
                canonical_globals: self.canonical_globals.base_len(),
                canonical_deferred_modules: self.canonical_deferred_modules.base_len(),
                canonical_deferred_children: self.canonical_deferred_children.base_len(),
                canonical_umd_exports: self.canonical_umd_exports.base_len(),
                canonical_export_contexts: self.canonical_export_contexts.base_len(),
            },
            canonical_namespaces: self
                .canonical_namespaces
                .local_iter()
                .map(|namespace| namespace.0)
                .collect(),
            canonical_source_units: self
                .canonical_source_units
                .local_iter()
                .map(|index| self.source_units[*index].source.0)
                .collect(),
            canonical_globals: self
                .canonical_globals
                .local_iter()
                .map(|global| global.0)
                .collect(),
            canonical_deferred_modules: self
                .canonical_deferred_modules
                .local_iter()
                .map(|module| module.0)
                .collect(),
            canonical_deferred_children: self
                .canonical_deferred_children
                .local_iter()
                .map(|index| u32::try_from(*index).expect("deferred child index fits u32"))
                .collect(),
            canonical_umd_exports: self
                .canonical_umd_exports
                .local_iter()
                .map(|index| u32::try_from(*index).expect("UMD export index fits u32"))
                .collect(),
            canonical_export_contexts: self
                .canonical_export_contexts
                .local_iter()
                .map(|context| context.0)
                .collect(),
        }
    }

    pub(crate) fn from_snapshot_primary(
        primary: NamespaceSnapshotPrimary,
    ) -> Result<Self, &'static str> {
        Self::validate_snapshot_primary_for_classification(&primary)?;
        let decoded_primary = primary.clone();
        let NamespaceSnapshotPrimary {
            namespaces,
            standalone_value_storages,
            fragments,
            members,
            placements: primary_placements,
            globals,
            deferred_modules,
            deferred_children,
            umd_exports,
            export_contexts,
            source_units,
            compilation_global,
            script_namespace_root,
            library_shared_globals,
        } = primary;
        if namespaces
            .iter()
            .enumerate()
            .any(|(index, namespace)| namespace.id.index() != index)
            || fragments
                .iter()
                .enumerate()
                .any(|(index, fragment)| fragment.id.index() != index)
            || members
                .iter()
                .enumerate()
                .any(|(index, member)| member.id.index() != index)
            || globals
                .iter()
                .enumerate()
                .any(|(index, global)| global.id.index() != index)
            || deferred_modules
                .iter()
                .enumerate()
                .any(|(index, module)| module.id.index() != index)
            || export_contexts
                .iter()
                .enumerate()
                .any(|(index, context)| context.id.index() != index)
        {
            return Err("snapshot namespace identities are not dense");
        }
        if standalone_value_storages.len() != namespaces.len() {
            return Err("snapshot namespace storage column has the wrong length");
        }
        let mut namespace_keys = FxHashMap::default();
        for namespace in &namespaces {
            let key = NamespaceKey {
                owner: namespace.owner,
                name: namespace.name.clone(),
            };
            if namespace_keys.insert(key, namespace.id).is_some() {
                return Err("snapshot namespace key index contains a duplicate");
            }
        }
        let mut placements = FxHashMap::default();
        for (owner, name, declarations) in primary_placements {
            if placements
                .insert(MergeKey { owner, name }, declarations)
                .is_some()
            {
                return Err("snapshot merge placement index contains a duplicate");
            }
        }
        let mut layered_namespace_keys = LayeredMap::default();
        for (key, id) in namespace_keys {
            layered_namespace_keys.insert_local(key, id)?;
        }
        let mut layered_placements = LayeredMap::default();
        for (key, declarations) in placements {
            layered_placements.insert_local(key, declarations)?;
        }
        let mut table = Self {
            namespaces: namespaces.into(),
            aggregate_instance_states: LayeredVec::default(),
            standalone_value_storages: standalone_value_storages.into(),
            fragments: fragments.into(),
            members: members.into(),
            namespace_keys: layered_namespace_keys,
            canonical_namespaces: LayeredVec::default(),
            placements: layered_placements,
            merges: LayeredVec::default(),
            merge_indices: LayeredMap::default(),
            standalone_storage_namespaces: LayeredMap::default(),
            declaration_owners_by_scope: LayeredMap::default(),
            fragments_by_declaration: LayeredMap::default(),
            fragment_private_scopes_by_site: LayeredMap::default(),
            global_augmentations_by_site: LayeredMap::default(),
            umd_exports_by_site: LayeredMap::default(),
            source_keys_by_module: LayeredMap::default(),
            library_export_default_sites: LayeredMap::default(),
            library_module_reporting_sites: LayeredMap::default(),
            globals: globals.into(),
            deferred_modules: deferred_modules.into(),
            deferred_children: deferred_children.into(),
            umd_exports: umd_exports.into(),
            export_contexts: export_contexts.into(),
            source_units: source_units.into(),
            canonical_source_units: LayeredVec::default(),
            canonical_globals: LayeredVec::default(),
            canonical_deferred_modules: LayeredVec::default(),
            canonical_deferred_children: LayeredVec::default(),
            canonical_umd_exports: LayeredVec::default(),
            canonical_export_contexts: LayeredVec::default(),
            compilation_global,
            script_namespace_root,
            library_shared_globals,
        };
        table.classify()?;
        table.rebuild_local_standalone_storage_index()?;
        if table.snapshot_primary() != decoded_primary {
            return Err("snapshot namespace derived state is not canonical");
        }
        Ok(table)
    }

    fn validate_snapshot_primary_for_classification(
        primary: &NamespaceSnapshotPrimary,
    ) -> Result<(), &'static str> {
        if primary
            .namespaces
            .iter()
            .enumerate()
            .any(|(index, namespace)| namespace.id.index() != index)
            || primary
                .fragments
                .iter()
                .enumerate()
                .any(|(index, fragment)| fragment.id.index() != index)
            || primary
                .members
                .iter()
                .enumerate()
                .any(|(index, member)| member.id.index() != index)
            || primary
                .globals
                .iter()
                .enumerate()
                .any(|(index, global)| global.id.index() != index)
            || primary
                .deferred_modules
                .iter()
                .enumerate()
                .any(|(index, module)| module.id.index() != index)
            || primary
                .export_contexts
                .iter()
                .enumerate()
                .any(|(index, context)| context.id.index() != index)
        {
            return Err("snapshot namespace identities are not dense");
        }
        if primary.standalone_value_storages.len() != primary.namespaces.len() {
            return Err("snapshot namespace storage column has the wrong length");
        }
        let fragment_len = primary.fragments.len();
        let member_len = primary.members.len();
        if primary.namespaces.iter().any(|namespace| {
            (primary.library_shared_globals && namespace.fragments.is_empty())
                || namespace
                    .fragments
                    .iter()
                    .any(|fragment| fragment.index() >= fragment_len)
        }) {
            return Err("snapshot namespace fragment reference is out of range");
        }
        if primary.fragments.iter().any(|fragment| {
            fragment.namespace.index() >= primary.namespaces.len()
                || fragment
                    .members
                    .iter()
                    .any(|member| member.index() >= member_len)
        }) {
            return Err("snapshot fragment reference is out of range");
        }
        if primary.globals.iter().any(|global| {
            global
                .members
                .iter()
                .any(|member| member.index() >= member_len)
        }) {
            return Err("snapshot global member reference is out of range");
        }
        if primary.export_contexts.iter().any(|context| {
            context
                .members
                .iter()
                .any(|member| member.index() >= member_len)
        }) {
            return Err("snapshot export-context member reference is out of range");
        }
        if primary.placements.iter().any(|(_, _, declarations)| {
            (primary.library_shared_globals && declarations.is_empty())
                || declarations.iter().any(|declaration| {
                    declaration
                        .namespace_fragment
                        .is_some_and(|fragment| fragment.index() >= fragment_len)
                })
        }) {
            return Err("snapshot merge placement reference is out of range");
        }
        Ok(())
    }

    pub(crate) fn validate_snapshot_canonical(&self) -> Result<(), &'static str> {
        let primary = self.snapshot_primary();
        let rebuilt = Self::from_snapshot_primary(primary.clone())?;
        (rebuilt.snapshot_primary() == primary)
            .then_some(())
            .ok_or("snapshot namespace derived ordering is not canonical")
    }

    fn classify(&mut self) -> Result<(), &'static str> {
        #[cfg(test)]
        record_finalization_classification();
        self.rebuild_local_fragment_declaration_index()?;
        self.rebuild_local_statement_site_indexes()?;
        self.compute_namespace_instance_states();
        let library_order = self.uses_library_shared_globals();
        for namespace in self.namespaces.local_iter_mut() {
            if library_order {
                namespace.fragments.sort_by_key(|fragment| {
                    let fragment = self
                        .fragments
                        .get(fragment.index())
                        .expect("canonical library namespace fragment exists");
                    (
                        fragment.origin,
                        fragment.source_start,
                        fragment.declaration.0,
                    )
                });
            } else {
                namespace.fragments.sort_by_key(|fragment| {
                    self.fragments
                        .get(fragment.index())
                        .map(|fragment| {
                            (
                                fragment.source,
                                fragment.source_start,
                                fragment.declaration.0,
                            )
                        })
                        .unwrap_or((SourceUnitKey(u32::MAX), u32::MAX, u32::MAX))
                });
            }
        }
        self.canonical_namespaces.clear_local();
        for id in (self.namespaces.base_len()..self.namespaces.len())
            .map(|index| NamespaceId(u32::try_from(index).expect("namespace count fits u32")))
        {
            self.canonical_namespaces.push_local(id);
        }
        if library_order {
            self.canonical_namespaces
                .local_slice_mut()
                .sort_by_key(|id| {
                    let namespace = &self.namespaces[id.index()];
                    let first = namespace
                        .fragments
                        .first()
                        .and_then(|fragment| self.fragments.get(fragment.index()))
                        .expect("canonical library namespace has a fragment");
                    (first.origin, first.source_start, namespace.name.clone())
                });
        } else {
            self.canonical_namespaces
                .local_slice_mut()
                .sort_by_key(|id| {
                    let namespace = &self.namespaces[id.index()];
                    let first = namespace
                        .fragments
                        .first()
                        .and_then(|fragment| self.fragments.get(fragment.index()));
                    (
                        first
                            .map(|fragment| fragment.source)
                            .unwrap_or(SourceUnitKey(u32::MAX)),
                        first
                            .map(|fragment| fragment.source_start)
                            .unwrap_or(u32::MAX),
                        namespace.name.clone(),
                    )
                });
        }
        self.merges.clear_local();
        let local_merges = self
            .placements
            .local_iter()
            .map(|(key, participants)| {
                #[cfg(test)]
                record_finalization_merge_participant_rows(participants.len());
                let mut declarations = participants.clone();
                if library_order {
                    declarations.sort_by_key(|participant| {
                        (
                            participant.origin,
                            participant.span.start,
                            participant.declaration.0,
                        )
                    });
                } else {
                    declarations.sort_by_key(|participant| {
                        (
                            participant.source,
                            participant.span.start,
                            participant.declaration.0,
                        )
                    });
                }
                let classification = classify_group(&declarations);
                let placement_issues = placement_issues(&declarations);
                MergeRecord {
                    owner: key.owner,
                    name: key.name.clone(),
                    declarations,
                    classification,
                    placement_issues,
                }
            })
            .collect::<Vec<_>>();
        for merge in local_merges {
            self.merges.push_local(merge);
        }
        if library_order {
            self.merges.local_slice_mut().sort_by(|left, right| {
                let left_key = left
                    .declarations
                    .first()
                    .map(|item| (item.origin, item.span.start))
                    .expect("canonical library merge has a declaration");
                let right_key = right
                    .declarations
                    .first()
                    .map(|item| (item.origin, item.span.start))
                    .expect("canonical library merge has a declaration");
                left_key
                    .cmp(&right_key)
                    .then_with(|| left.name.cmp(&right.name))
            });
        } else {
            self.merges.local_slice_mut().sort_by(|left, right| {
                let left_key = left
                    .declarations
                    .first()
                    .map(|item| (item.source, item.span.start))
                    .unwrap_or((SourceUnitKey(u32::MAX), u32::MAX));
                let right_key = right
                    .declarations
                    .first()
                    .map(|item| (item.source, item.span.start))
                    .unwrap_or((SourceUnitKey(u32::MAX), u32::MAX));
                left_key
                    .cmp(&right_key)
                    .then_with(|| left.name.cmp(&right.name))
            });
        }
        self.rebuild_local_merge_index()?;
        self.rebuild_local_declaration_owner_scope_index()?;
        self.canonical_globals.clear_local();
        for id in (self.globals.base_len()..self.globals.len())
            .map(|index| GlobalAugmentationId(u32::try_from(index).expect("global count fits u32")))
        {
            self.canonical_globals.push_local(id);
        }
        if library_order {
            self.canonical_globals.local_slice_mut().sort_by_key(|id| {
                let global = &self.globals[id.index()];
                (global.origin, global.diagnostic_span.start, global.source)
            });
        } else {
            self.canonical_globals.local_slice_mut().sort_by_key(|id| {
                let global = &self.globals[id.index()];
                (global.source, global.diagnostic_span.start, global.origin)
            });
        }
        self.canonical_deferred_modules.clear_local();
        for id in (self.deferred_modules.base_len()..self.deferred_modules.len())
            .map(|index| DeferredModuleId(u32::try_from(index).expect("module count fits u32")))
        {
            self.canonical_deferred_modules.push_local(id);
        }
        if library_order {
            self.canonical_deferred_modules
                .local_slice_mut()
                .sort_by_key(|id| {
                    let module = &self.deferred_modules[id.index()];
                    (module.origin, module.span.start, module.source)
                });
        } else {
            self.canonical_deferred_modules
                .local_slice_mut()
                .sort_by_key(|id| {
                    let module = &self.deferred_modules[id.index()];
                    (module.source, module.span.start, module.origin)
                });
        }
        self.canonical_source_units.clear_local();
        for index in self.source_units.base_len()..self.source_units.len() {
            self.canonical_source_units.push_local(index);
        }
        if library_order {
            self.canonical_source_units
                .local_slice_mut()
                .sort_by_key(|index| {
                    let unit = &self.source_units[*index];
                    (unit.origin, unit.source)
                });
        } else {
            self.canonical_source_units
                .local_slice_mut()
                .sort_by_key(|index| {
                    let unit = &self.source_units[*index];
                    (unit.source, unit.origin)
                });
        }
        self.canonical_deferred_children.clear_local();
        for index in self.deferred_children.base_len()..self.deferred_children.len() {
            self.canonical_deferred_children.push_local(index);
        }
        if library_order {
            self.canonical_deferred_children
                .local_slice_mut()
                .sort_by_key(|index| {
                    let child = &self.deferred_children[*index];
                    (child.origin, child.span.start, child.source)
                });
        } else {
            self.canonical_deferred_children
                .local_slice_mut()
                .sort_by_key(|index| {
                    let child = &self.deferred_children[*index];
                    (child.source, child.span.start, child.origin)
                });
        }
        self.canonical_umd_exports.clear_local();
        for index in self.umd_exports.base_len()..self.umd_exports.len() {
            self.canonical_umd_exports.push_local(index);
        }
        if library_order {
            self.canonical_umd_exports
                .local_slice_mut()
                .sort_by_key(|index| {
                    let export = &self.umd_exports[*index];
                    (export.origin, export.span.start, export.source)
                });
        } else {
            self.canonical_umd_exports
                .local_slice_mut()
                .sort_by_key(|index| {
                    let export = &self.umd_exports[*index];
                    (export.source, export.span.start, export.origin)
                });
        }
        self.canonical_export_contexts.clear_local();
        for id in (self.export_contexts.base_len()..self.export_contexts.len()).map(|index| {
            ExportContextId(u32::try_from(index).expect("export context count fits u32"))
        }) {
            self.canonical_export_contexts.push_local(id);
        }
        if library_order {
            self.canonical_export_contexts
                .local_slice_mut()
                .sort_by_key(|id| {
                    let context = &self.export_contexts[id.index()];
                    (context.origin, context.span.start, context.source)
                });
        } else {
            self.canonical_export_contexts
                .local_slice_mut()
                .sort_by_key(|id| {
                    let context = &self.export_contexts[id.index()];
                    (context.source, context.span.start, context.origin)
                });
        }
        Ok(())
    }

    fn rebuild_local_merge_index(&mut self) -> Result<(), &'static str> {
        self.merge_indices.clear_local();
        let base_len = self.merges.base_len();
        let rows = self
            .merges
            .local_iter()
            .enumerate()
            .map(|(offset, record)| {
                (
                    MergeKey {
                        owner: record.owner,
                        name: record.name.clone(),
                    },
                    base_len + offset,
                )
            })
            .collect::<Vec<_>>();
        for (key, index) in rows {
            #[cfg(test)]
            record_finalization_merge_index_row();
            self.merge_indices.insert_local(key, index)?;
        }
        Ok(())
    }

    fn rebuild_local_fragment_declaration_index(&mut self) -> Result<(), &'static str> {
        self.fragments_by_declaration.clear_local();
        self.fragment_private_scopes_by_site.clear_local();
        let rows = self
            .fragments
            .local_iter()
            .map(|fragment| {
                (
                    fragment.declaration,
                    fragment.id,
                    (fragment.module, fragment.source_start),
                    fragment.private_scope,
                )
            })
            .collect::<Vec<_>>();
        for (declaration, fragment, site, private_scope) in rows {
            if self
                .fragments_by_declaration
                .insert_local(declaration, fragment)
                .map_err(|_| "namespace fragment declaration index contains a duplicate")?
                .is_some()
            {
                return Err("namespace fragment declaration index contains a duplicate");
            }
            if self
                .fragment_private_scopes_by_site
                .insert_local(site, private_scope)
                .map_err(|_| "namespace fragment site index contains a duplicate")?
                .is_some()
            {
                return Err("namespace fragment site index contains a duplicate");
            }
        }
        Ok(())
    }

    fn rebuild_local_statement_site_indexes(&mut self) -> Result<(), &'static str> {
        self.global_augmentations_by_site.clear_local();
        self.umd_exports_by_site.clear_local();
        self.source_keys_by_module.clear_local();
        self.library_export_default_sites.clear_local();
        self.library_module_reporting_sites.clear_local();
        let globals = self
            .globals
            .local_iter()
            .map(|global| ((global.module, global.diagnostic_span.start), global.id))
            .collect::<Vec<_>>();
        for (site, global) in globals {
            if self
                .global_augmentations_by_site
                .insert_local(site, global)
                .map_err(|_| "global augmentation site index contains a duplicate")?
                .is_some()
            {
                return Err("global augmentation site index contains a duplicate");
            }
        }
        let umd_base = self.umd_exports.base_len();
        let exports = self
            .umd_exports
            .local_iter()
            .enumerate()
            .map(|(offset, export)| ((export.module, export.span.start), umd_base + offset))
            .collect::<Vec<_>>();
        for (site, export) in exports {
            if self
                .umd_exports_by_site
                .insert_local(site, export)
                .map_err(|_| "UMD export site index contains a duplicate")?
                .is_some()
            {
                return Err("UMD export site index contains a duplicate");
            }
        }
        let sources = self
            .source_units
            .local_iter()
            .map(|unit| (unit.module, unit.source))
            .collect::<Vec<_>>();
        for (module, source) in sources {
            if self
                .source_keys_by_module
                .insert_local(module, source)
                .map_err(|_| "source-module index contains a duplicate")?
                .is_some()
            {
                return Err("source-module index contains a duplicate");
            }
        }
        let default_sites = self
            .export_contexts
            .local_iter()
            .filter(|context| {
                context.kind == ExportContextKind::ExportDefault
                    && context.syntax == ExportSyntaxDisposition::FutureTk1319
                    && matches!(context.origin, CompilationOrigin::Library(_))
            })
            .map(|context| ((context.source, context.span.start), true))
            .collect::<Vec<_>>();
        for (site, owned) in default_sites {
            if self
                .library_export_default_sites
                .insert_local(site, owned)
                .map_err(|_| "library export-default reporting index contains a duplicate")?
                .is_some()
            {
                return Err("library export-default reporting index contains a duplicate");
            }
        }
        let module_sites = self
            .export_contexts
            .local_iter()
            .filter(|context| {
                context.syntax == ExportSyntaxDisposition::FutureTk1319
                    && matches!(context.origin, CompilationOrigin::Library(_))
            })
            .filter_map(|context| match context.owner {
                ExportContextOwner::NamespaceFragment(fragment) => self
                    .fragment(fragment)
                    .map(|fragment| ((fragment.module, fragment.source_start), true)),
                ExportContextOwner::GlobalAugmentation(_)
                | ExportContextOwner::DeferredAmbientModule(_) => None,
            })
            .collect::<Vec<_>>();
        for (site, owned) in module_sites {
            if self
                .library_module_reporting_sites
                .insert_local(site, owned)
                .map_err(|_| "library module-reporting index contains a duplicate")?
                .is_some()
            {
                return Err("library module-reporting index contains a duplicate");
            }
        }
        Ok(())
    }

    fn rebuild_local_standalone_storage_index(&mut self) -> Result<(), &'static str> {
        self.standalone_storage_namespaces.clear_local();
        let base_len = self.standalone_value_storages.base_len();
        let rows = self
            .standalone_value_storages
            .local_iter()
            .enumerate()
            .filter_map(|(offset, storage)| {
                let storage = (*storage)?;
                let index = base_len + offset;
                Some((
                    storage,
                    NamespaceId(u32::try_from(index).expect("namespace index fits u32")),
                ))
            })
            .collect::<Vec<_>>();
        for (storage, namespace) in rows {
            self.standalone_storage_namespaces
                .insert_local(storage, namespace)?;
        }
        Ok(())
    }

    fn rebuild_local_declaration_owner_scope_index(&mut self) -> Result<(), &'static str> {
        self.declaration_owners_by_scope.clear_local();
        if let Some(scope) = self.compilation_global {
            if !self.declaration_owners_by_scope.contains_key(&scope) {
                self.declaration_owners_by_scope
                    .insert_local(scope, DeclarationOwner::CompilationGlobal)?;
            }
        }
        let namespace_base = self.namespaces.base_len();
        let namespace_rows = self
            .namespaces
            .local_iter()
            .enumerate()
            .map(|(offset, namespace)| {
                let id = NamespaceId(
                    u32::try_from(namespace_base + offset).expect("namespace id fits u32"),
                );
                (
                    namespace.public_scope,
                    DeclarationOwner::NamespacePublic(id),
                )
            })
            .collect::<Vec<_>>();
        for (scope, owner) in namespace_rows {
            self.declaration_owners_by_scope
                .insert_local(scope, owner)?;
        }
        let fragment_base = self.fragments.base_len();
        let fragment_rows = self
            .fragments
            .local_iter()
            .enumerate()
            .map(|(offset, fragment)| {
                let id = NamespaceFragmentId(
                    u32::try_from(fragment_base + offset).expect("fragment id fits u32"),
                );
                (
                    fragment.private_scope,
                    DeclarationOwner::NamespacePrivate(id),
                )
            })
            .collect::<Vec<_>>();
        for (scope, owner) in fragment_rows {
            self.declaration_owners_by_scope
                .insert_local(scope, owner)?;
        }
        Ok(())
    }

    fn compute_namespace_instance_states(&mut self) {
        let fragment_base = self.fragments.base_len();
        let mut states = vec![NamespaceInstanceState::NonInstantiated; self.fragments.local_len()];
        for fragment in self.fragments.local_iter() {
            #[cfg(test)]
            record_continuation_instance_fragment_row();
            for member in fragment
                .members
                .iter()
                .filter_map(|member| self.members.get(member.index()))
            {
                let direct = match member.kind {
                    MergeDeclarationKind::Variable
                    | MergeDeclarationKind::Function
                    | MergeDeclarationKind::Class => NamespaceInstanceState::Instantiated,
                    MergeDeclarationKind::ImportAlias
                        if member.spaces.value
                            && matches!(
                                member.syntax,
                                DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
                                    exported: true,
                                    ..
                                })
                            ) =>
                    {
                        NamespaceInstanceState::Instantiated
                    }
                    MergeDeclarationKind::Enum
                        if member.syntax == (DeclarationSyntaxFacts::Enum { constant: false }) =>
                    {
                        NamespaceInstanceState::Instantiated
                    }
                    _ => NamespaceInstanceState::NonInstantiated,
                };
                let index = fragment.id.index() - fragment_base;
                states[index] = join_instance_state(states[index], direct);
            }
        }

        loop {
            let mut changed = false;
            for fragment in self.fragments.local_iter() {
                #[cfg(test)]
                record_continuation_instance_fragment_row();
                let fragment_index = fragment.id.index() - fragment_base;
                let mut state = states[fragment_index];
                for member in fragment
                    .members
                    .iter()
                    .filter_map(|member| self.members.get(member.index()))
                    .filter(|member| member.kind == MergeDeclarationKind::Namespace)
                {
                    #[cfg(test)]
                    record_continuation_child_fragment_lookup();
                    let child = member
                        .declaration
                        .and_then(|declaration| self.fragments_by_declaration.get(&declaration))
                        .and_then(|fragment| self.fragments.get(fragment.index()));
                    if let Some(child) = child.filter(|child| child.id.index() >= fragment_base) {
                        state =
                            join_instance_state(state, states[child.id.index() - fragment_base]);
                    }
                }
                if state != states[fragment_index] {
                    states[fragment_index] = state;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for fragment in self.fragments.local_iter_mut() {
            fragment.instance_state = states[fragment.id.index() - fragment_base];
        }
        self.aggregate_instance_states.clear_local();
        for _ in 0..self.namespaces.local_len() {
            self.aggregate_instance_states
                .push_local(NamespaceInstanceState::NonInstantiated);
        }
        for fragment in self.fragments.local_iter() {
            if let Some(aggregate) = self
                .aggregate_instance_states
                .get_mut_local(fragment.namespace.index())
            {
                *aggregate = join_instance_state(*aggregate, fragment.instance_state);
            }
        }
        for participants in self.placements.local_values_mut() {
            for participant in participants {
                participant.namespace_instance = participant
                    .namespace_fragment
                    .and_then(|fragment| fragment.index().checked_sub(fragment_base))
                    .and_then(|index| states.get(index).copied());
            }
        }
    }

    fn dormant_standalone_value_storage_candidates(&self) -> Vec<NamespaceId> {
        self.canonical_namespaces
            .local_iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_allocation_namespace_row();
            })
            .copied()
            .filter(|namespace| {
                self.aggregate_instance_state(*namespace)
                    == Some(NamespaceInstanceState::Instantiated)
                    && self.standalone_value_storage(*namespace).is_none()
                    && (self.uses_library_shared_globals()
                        || !self.has_compilation_global_ancestor(*namespace))
                    && self
                        .standalone_merge_record(*namespace)
                        .is_some_and(|record| {
                            record.classification.disposition == MergeDisposition::Admitted
                                && namespace_value_attachment_disposition(record)
                                    == Some(NamespaceValueAttachmentDisposition::TypeContainerOnly)
                        })
            })
            .collect()
    }

    fn is_admitted_instantiated_standalone(&self, record: &MergeRecord) -> bool {
        !matches!(record.owner, DeclarationOwner::DeferredAmbientModule(_))
            && (record.owner != DeclarationOwner::CompilationGlobal
                || self.uses_library_shared_globals())
            && record.classification.disposition == MergeDisposition::Admitted
            && namespace_value_attachment_disposition(record)
                == Some(NamespaceValueAttachmentDisposition::TypeContainerOnly)
            && record.declarations.iter().any(|participant| {
                participant.kind == MergeDeclarationKind::Namespace
                    && participant.namespace_instance == Some(NamespaceInstanceState::Instantiated)
            })
            && record
                .declarations
                .iter()
                .filter_map(|participant| participant.namespace_fragment)
                .filter_map(|fragment| self.fragment(fragment))
                .all(|fragment| {
                    self.uses_library_shared_globals()
                        || !self.has_compilation_global_ancestor(fragment.namespace)
                })
    }

    fn standalone_merge_record(&self, id: NamespaceId) -> Option<&MergeRecord> {
        let namespace = self.get(id)?;
        let owner = match namespace.owner {
            NamespaceOwner::Lexical(scope) => DeclarationOwner::Lexical(scope),
            NamespaceOwner::NamespacePublic(parent) => DeclarationOwner::NamespacePublic(parent),
            NamespaceOwner::FragmentPrivate(fragment) => {
                DeclarationOwner::NamespacePrivate(fragment)
            }
            NamespaceOwner::CompilationGlobal => DeclarationOwner::CompilationGlobal,
        };
        let key = MergeKey {
            owner,
            name: namespace.name.clone(),
        };
        self.merge_indices
            .get(&key)
            .and_then(|index| self.merges.get(*index))
    }

    fn has_compilation_global_ancestor(&self, id: NamespaceId) -> bool {
        let mut current = Some(id);
        let mut remaining = self.namespaces.len();
        while let Some(namespace) = current {
            if remaining == 0 {
                return true;
            }
            remaining -= 1;
            current = match self.get(namespace).map(|namespace| namespace.owner) {
                Some(NamespaceOwner::CompilationGlobal) => return true,
                Some(NamespaceOwner::NamespacePublic(parent)) => Some(parent),
                Some(NamespaceOwner::FragmentPrivate(fragment)) => {
                    self.fragment(fragment).map(|fragment| fragment.namespace)
                }
                Some(NamespaceOwner::Lexical(_)) | None => None,
            };
        }
        false
    }
}

fn assert_idempotent_overlay_publication(
    replaced: Option<SymbolId>,
    intended: SymbolId,
    message: &'static str,
) {
    assert!(
        replaced.is_none_or(|existing| existing == intended),
        "{message}"
    );
}

/// Append dormant namespace-owned slots after all lexical storage allocation.
pub(super) fn allocate_dormant_namespace_value_storages(state: &mut BindState) {
    let candidates = state
        .namespaces
        .dormant_standalone_value_storage_candidates();
    for namespace in candidates {
        let storage = state.fresh_value_storage();
        let slot = state
            .namespaces
            .standalone_value_storages
            .get_mut_local(namespace.index())
            .expect("namespace storage side column is dense");
        assert!(
            slot.replace(storage).is_none(),
            "namespace storage is stable"
        );
        state
            .namespaces
            .standalone_storage_namespaces
            .insert_local(storage, namespace)
            .expect("namespace storage cannot replace a frozen index entry");
        let symbol = state
            .namespaces
            .get(namespace)
            .expect("namespace storage owner exists")
            .symbol;
        let root = state
            .symbols
            .get_mut(symbol)
            .expect("namespace root symbol exists");
        assert!(
            root.value.replace(storage).is_none(),
            "standalone root is dormant"
        );
    }

    let type_only = state
        .namespaces
        .canonical_namespaces
        .local_iter()
        .inspect(|_| {
            #[cfg(test)]
            record_continuation_allocation_namespace_row();
        })
        .copied()
        .filter(|namespace| {
            state.namespaces.aggregate_instance_state(*namespace)
                == Some(NamespaceInstanceState::NonInstantiated)
                && state
                    .namespaces
                    .standalone_merge_record(*namespace)
                    .is_some_and(|record| {
                        record.classification.disposition == MergeDisposition::Admitted
                            && namespace_value_attachment_disposition(record)
                                == Some(NamespaceValueAttachmentDisposition::TypeContainerOnly)
                    })
        })
        .filter_map(|namespace| state.namespaces.get(namespace).map(|root| root.symbol))
        .collect::<Vec<_>>();
    for symbol in type_only {
        if let Some(symbol) = state.symbols.get_mut(symbol) {
            symbol.blocks_value_lookup = true;
        }
    }
}

#[derive(Copy, Clone, Default)]
struct QualifiedSymbolView {
    namespace: Option<NamespaceId>,
    type_group: Option<TypeGroupId>,
    value: bool,
    unavailable: bool,
    deferred: Option<QualifiedTypePathDeferredReason>,
}

impl Binder {
    pub(crate) fn namespace_fragment_private_scope(
        &self,
        module: ScopeId,
        source_start: u32,
    ) -> Option<ScopeId> {
        #[cfg(test)]
        record_continuation_fragment_scope_lookup();
        self.namespaces
            .fragment_private_scopes_by_site
            .get(&(module, source_start))
            .copied()
    }

    pub(crate) fn standalone_namespace_for_storage(
        &self,
        storage: ValueStorageId,
    ) -> Option<NamespaceId> {
        self.namespaces
            .standalone_storage_namespaces
            .get(&storage)
            .copied()
    }

    pub(crate) fn local_standalone_namespace_value_attachments(
        &self,
    ) -> Vec<StandaloneNamespaceValueAttachment<'_>> {
        self.namespaces
            .canonical_namespaces
            .local_iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_attachment_namespace_row();
            })
            .filter_map(|namespace| {
                let storage = self.namespaces.standalone_value_storage(*namespace)?;
                let root = self.namespaces.get(*namespace)?;
                let fragments = root
                    .fragments
                    .iter()
                    .filter_map(|fragment| self.namespaces.fragment(*fragment))
                    .collect::<Vec<_>>();
                let mut members = fragments
                    .iter()
                    .flat_map(|fragment| fragment.members.iter())
                    .filter_map(|member| self.namespaces.member(*member))
                    .map(|member| {
                        let declaration = member.declaration;
                        let lexical = declaration.and_then(|id| self.declarations.get(id));
                        let child_namespace = member
                            .symbol
                            .and_then(|symbol| self.symbols.get(symbol))
                            .and_then(|symbol| symbol.ns);
                        StandaloneNamespaceValueMember {
                            member: member.id,
                            declaration,
                            name: member.name.as_deref(),
                            source: member.source,
                            site: lexical.map(|declaration| declaration.site),
                            declaration_span: member.declaration_span,
                            local_span: member.local_span,
                            origin: member.origin,
                            value_storage: lexical
                                .and_then(|declaration| declaration.value_storage)
                                .or_else(|| {
                                    member
                                        .symbol
                                        .and_then(|symbol| self.symbols.get(symbol))
                                        .and_then(|symbol| symbol.value)
                                }),
                            alias_target_storage: member
                                .local_symbol
                                .and_then(|symbol| self.symbols.get(symbol))
                                .and_then(|symbol| symbol.value),
                            ambient: fragments.iter().any(|fragment| {
                                fragment.members.contains(&member.id) && fragment.ambient
                            }),
                            child_namespace,
                            kind: member.kind,
                            publication: member.publication,
                            spaces: member.spaces,
                        }
                    })
                    .collect::<Vec<_>>();
                if self.namespaces.uses_library_shared_globals() {
                    members.sort_by_key(|member| {
                        (
                            member.origin,
                            member
                                .site
                                .map_or(u32::MAX, |site| site.declaration_span.start),
                            member
                                .declaration
                                .map_or(u32::MAX, |declaration| declaration.0),
                            member.member.0,
                        )
                    });
                } else {
                    members.sort_by_key(|member| {
                        (
                            member.source,
                            member
                                .site
                                .map_or(u32::MAX, |site| site.declaration_span.start),
                            member
                                .declaration
                                .map_or(u32::MAX, |declaration| declaration.0),
                            member.member.0,
                        )
                    });
                }
                Some(StandaloneNamespaceValueAttachment {
                    namespace: *namespace,
                    storage,
                    symbol: root.symbol,
                    fragments,
                    members,
                })
            })
            .collect()
    }

    pub(crate) fn global_augmentation_scope(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Option<ScopeId> {
        #[cfg(test)]
        record_continuation_global_statement_query();
        self.namespaces
            .global_augmentations_by_site
            .get(&(module, binding_start))
            .and_then(|global| self.namespaces.globals.get(global.index()))
            .map(|global| global.overlay_scope)
    }

    pub(crate) fn global_augmentation_requires_incomplete(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> bool {
        #[cfg(test)]
        record_continuation_global_statement_query();
        let Some(global) = self
            .namespaces
            .global_augmentations_by_site
            .get(&(module, binding_start))
            .and_then(|global| self.namespaces.globals.get(global.index()))
        else {
            return false;
        };
        if !global.issues.is_empty() {
            return false;
        }
        global.members.iter().any(|member| {
            let Some(member) = self.namespaces.member(*member) else {
                return true;
            };
            match member.kind {
                MergeDeclarationKind::Interface | MergeDeclarationKind::TypeAlias => false,
                MergeDeclarationKind::Namespace => {
                    let fragment = member
                        .declaration
                        .and_then(|declaration| {
                            self.namespaces.fragments_by_declaration.get(&declaration)
                        })
                        .and_then(|fragment| self.namespaces.fragment(*fragment));
                    fragment.is_none_or(|fragment| {
                        fragment.instance_state == NamespaceInstanceState::Instantiated
                    })
                }
                _ => true,
            }
        })
    }

    pub(crate) fn umd_export_requires_incomplete(&self, module: ScopeId, span_start: u32) -> bool {
        #[cfg(test)]
        record_continuation_umd_statement_query();
        self.namespaces
            .umd_exports_by_site
            .get(&(module, span_start))
            .and_then(|export| self.namespaces.umd_exports.get(*export))
            .is_some_and(|export| export.context == UmdContext::DeferredValidBacklog15)
    }

    #[cfg(test)]
    pub(crate) fn library_export_default_reporting_owns(
        &self,
        module: ScopeId,
        span_start: u32,
    ) -> bool {
        record_continuation_library_source_lookup();
        let source = self.namespaces.source_keys_by_module.get(&module).copied();
        record_continuation_library_reporting_lookup();
        source.is_some_and(|source| {
            self.namespaces
                .library_export_default_sites
                .contains_key(&(source, span_start))
        })
    }

    #[cfg(test)]
    pub(crate) fn library_module_reporting_owns(&self, module: ScopeId, source_start: u32) -> bool {
        record_continuation_library_reporting_lookup();
        self.namespaces
            .library_module_reporting_sites
            .contains_key(&(module, source_start))
    }

    /// Return the frozen namespace-side input for one lexical value owner.
    /// Only admitted owners and the exact backlog-42 callable recovery expose members.
    pub(crate) fn namespace_value_attachment(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<NamespaceValueAttachment<'_>> {
        let owner = self
            .namespaces
            .declaration_owners_by_scope
            .get(&scope)
            .copied()
            .unwrap_or(DeclarationOwner::Lexical(scope));
        self.namespace_value_attachment_for_owner(owner, name)
    }

    pub(crate) fn namespace_value_attachment_for_owner(
        &self,
        owner: DeclarationOwner,
        name: &str,
    ) -> Option<NamespaceValueAttachment<'_>> {
        let key = MergeKey {
            owner,
            name: name.to_owned(),
        };
        let record = self
            .namespaces
            .merge_indices
            .get(&key)
            .and_then(|index| self.namespaces.merges.get(*index))?;
        self.namespace_value_attachment_from_record(record)
    }

    fn namespace_value_attachment_from_record<'a>(
        &'a self,
        record: &'a MergeRecord,
    ) -> Option<NamespaceValueAttachment<'a>> {
        let disposition = namespace_value_attachment_disposition(record)?;
        let namespace = record.declarations.iter().find_map(|participant| {
            participant
                .namespace_fragment
                .and_then(|fragment| self.namespaces.fragment(fragment))
                .map(|fragment| fragment.namespace)
        })?;
        let symbol = self.namespaces.get(namespace)?.symbol;
        let exposes_members = matches!(
            disposition,
            NamespaceValueAttachmentDisposition::AdmittedFunction
                | NamespaceValueAttachmentDisposition::AdmittedClass
                | NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42
        );
        let fragments = if exposes_members {
            record
                .declarations
                .iter()
                .filter_map(|participant| participant.namespace_fragment)
                .filter_map(|fragment| self.namespaces.fragment(fragment))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut members = fragments
            .iter()
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| self.namespaces.member(*member))
            .filter(|member| {
                member.spaces.value && !matches!(member.publication, NamespacePublication::Private)
            })
            .filter_map(|member| {
                let declaration = member.declaration?;
                let lexical = self.declarations.get(declaration)?;
                let name = member.name.as_deref()?;
                Some(AttachedNamespaceValueMember {
                    member: member.id,
                    declaration,
                    name,
                    source: member.source,
                    origin: member.origin,
                    scope: lexical.site.scope?,
                    site: lexical.site,
                    value_storage: lexical.value_storage,
                    symbol: member.symbol,
                    kind: member.kind,
                    variable_kind: match member.syntax {
                        DeclarationSyntaxFacts::Variable(kind) => Some(kind),
                        _ => None,
                    },
                    publication: member.publication,
                    ambient: fragments
                        .iter()
                        .any(|fragment| fragment.members.contains(&member.id) && fragment.ambient),
                })
            })
            .collect::<Vec<_>>();
        if self.namespaces.uses_library_shared_globals() {
            members.sort_by_key(|member| {
                (
                    member.origin,
                    member.site.declaration_span.start,
                    member.declaration.0,
                )
            });
        } else {
            members.sort_by_key(|member| {
                (
                    member.source,
                    member.site.declaration_span.start,
                    member.declaration.0,
                )
            });
        }
        Some(NamespaceValueAttachment {
            owner: record.owner,
            name: &record.name,
            symbol,
            disposition,
            fragments,
            members,
        })
    }

    pub(crate) fn local_ambient_export_alias_failures(
        &self,
    ) -> Vec<LocalAmbientExportAliasFailure> {
        self.namespaces
            .members
            .local_iter()
            .inspect(|_| {
                #[cfg(test)]
                record_continuation_ambient_alias_member_row();
            })
            .filter(|member| {
                matches!(member.owner, NamespaceMemberOwner::Fragment(_))
                    && member.kind == MergeDeclarationKind::DeferredExport
                    && member.declaration.is_none()
                    && member.alias_context == Some(AliasContext::ValidAmbient)
                    && member.module_specifier.is_none()
                    && member.local_symbol.is_none()
            })
            .filter_map(|member| {
                let local_name = member.local_name.as_ref()?.text();
                let kind = match member.owner {
                    NamespaceMemberOwner::Fragment(fragment) => {
                        let fragment = self.namespaces.fragment(fragment)?;
                        let public_scope = self.namespaces.get(fragment.namespace)?.public_scope;
                        let has_dormant_symbol = [fragment.private_scope, public_scope]
                            .into_iter()
                            .any(|scope| {
                                self.graph
                                    .get(scope)
                                    .and_then(|scope| scope.lookup_local(local_name))
                                    .is_some()
                            });
                        if has_dormant_symbol {
                            LocalAmbientExportAliasFailureKind::NonLocal
                        } else {
                            LocalAmbientExportAliasFailureKind::Missing
                        }
                    }
                    NamespaceMemberOwner::GlobalAugmentation(_)
                    | NamespaceMemberOwner::DeferredAmbientModule(_) => {
                        LocalAmbientExportAliasFailureKind::Missing
                    }
                };
                Some(LocalAmbientExportAliasFailure {
                    origin: member.origin,
                    local_span: member.local_span?,
                    local_name: local_name.to_string(),
                    kind,
                })
            })
            .collect()
    }

    pub(crate) fn resolve_qualified_type_path(
        &self,
        scope: ScopeId,
        segments: &[&str],
    ) -> QualifiedTypePathResolution {
        self.resolve_qualified_type_path_traced(scope, segments, || {}, |_| {})
    }

    pub(crate) fn resolve_qualified_type_path_traced(
        &self,
        scope: ScopeId,
        segments: &[&str],
        mut compilation_root_probe: impl FnMut(),
        mut namespace_visit: impl FnMut(NamespaceId),
    ) -> QualifiedTypePathResolution {
        if segments.len() < 2 {
            return QualifiedTypePathResolution::MissingRoot { segment: 0 };
        }

        let root_name = segments[0];
        let mut current_scope = Some(scope);
        let mut saw_type_root = false;
        let root_namespace = loop {
            let Some(candidate_scope) = current_scope else {
                return if saw_type_root {
                    QualifiedTypePathResolution::TypeOnlyRoot { segment: 0 }
                } else {
                    QualifiedTypePathResolution::MissingRoot { segment: 0 }
                };
            };
            let scope_record = match self.graph.get(candidate_scope) {
                Some(scope) => scope,
                None => return QualifiedTypePathResolution::MissingRoot { segment: 0 },
            };
            if candidate_scope == self.compilation_global {
                compilation_root_probe();
            }
            let owning_namespace = self.namespace_for_lookup_scope(candidate_scope);
            let root_merge = owning_namespace
                .is_none()
                .then(|| self.root_merge_record(candidate_scope, root_name))
                .flatten();
            let root_deferred = root_merge.and_then(Self::merge_deferred_reason);
            let root_symbol = scope_record.lookup_local(root_name);
            if let Some(symbol) = root_symbol {
                if self
                    .symbols
                    .get(symbol)
                    .is_some_and(|symbol| symbol.blocks_namespace_lookup)
                {
                    return QualifiedTypePathResolution::Unavailable { segment: 0 };
                }
                let view = self.qualified_symbol_view(owning_namespace, symbol, &mut Vec::new());
                if view.unavailable {
                    return QualifiedTypePathResolution::Unavailable { segment: 0 };
                }
                if let Some(reason) = view.deferred {
                    return QualifiedTypePathResolution::Deferred { segment: 0, reason };
                }
                if let Some(reason) = root_deferred {
                    let known_target = view.namespace.is_some() || view.type_group.is_some();
                    let admitted = root_merge.is_some_and(|record| {
                        record.classification.disposition == MergeDisposition::Admitted
                    });
                    let concrete_import_namespace = reason
                        == QualifiedTypePathDeferredReason::Import
                        && view.namespace.is_some()
                        && root_merge.is_some_and(Self::has_qualified_import_namespace_target);
                    if !known_target || (!admitted && !concrete_import_namespace) {
                        return QualifiedTypePathResolution::Deferred { segment: 0, reason };
                    }
                }
                if let Some(namespace) = view.namespace {
                    break namespace;
                }
                saw_type_root |= view.type_group.is_some()
                    || self
                        .symbols
                        .get(symbol)
                        .is_some_and(|symbol| symbol.ty.is_some());
            } else if let Some(reason) = root_deferred {
                return QualifiedTypePathResolution::Deferred { segment: 0, reason };
            }

            if scope_record.kind == ScopeKind::NamespacePrivate {
                if let Some(namespace) = owning_namespace {
                    let public_scope = self
                        .namespaces
                        .get(namespace)
                        .map(|namespace| namespace.public_scope);
                    if let Some(public_scope) = public_scope {
                        if let Some(symbol) = self
                            .graph
                            .get(public_scope)
                            .and_then(|scope| scope.lookup_local(root_name))
                        {
                            let view = self.qualified_symbol_view(
                                Some(namespace),
                                symbol,
                                &mut Vec::new(),
                            );
                            if view.unavailable {
                                return QualifiedTypePathResolution::Unavailable { segment: 0 };
                            }
                            if let Some(reason) = view.deferred {
                                return QualifiedTypePathResolution::Deferred {
                                    segment: 0,
                                    reason,
                                };
                            }
                            if let Some(namespace) = view.namespace {
                                break namespace;
                            }
                            saw_type_root |= view.type_group.is_some()
                                || self
                                    .symbols
                                    .get(symbol)
                                    .is_some_and(|symbol| symbol.ty.is_some());
                        }
                    }
                }
            }
            current_scope = scope_record.parent;
        };

        let mut namespace = root_namespace;
        namespace_visit(namespace);
        for (segment, name) in segments.iter().enumerate().skip(1) {
            let public_scope = match self.namespaces.get(namespace) {
                Some(namespace) => namespace.public_scope,
                None => return QualifiedTypePathResolution::MissingMember { segment },
            };
            let Some(symbol) = self
                .graph
                .get(public_scope)
                .and_then(|scope| scope.lookup_local(name))
            else {
                return QualifiedTypePathResolution::MissingMember { segment };
            };
            let view = self.qualified_symbol_view(Some(namespace), symbol, &mut Vec::new());
            if view.unavailable {
                return QualifiedTypePathResolution::Unavailable { segment };
            }
            if let Some(reason) = view.deferred {
                return QualifiedTypePathResolution::Deferred { segment, reason };
            }
            let leaf = segment + 1 == segments.len();
            if !leaf {
                if let Some(next) = view.namespace {
                    namespace = next;
                    namespace_visit(namespace);
                    continue;
                }
                return if view.type_group.is_some() {
                    QualifiedTypePathResolution::TypeOnlyIntermediate { segment }
                } else {
                    QualifiedTypePathResolution::MissingMember { segment }
                };
            }
            if let Some(group) = view.type_group {
                return QualifiedTypePathResolution::TypeGroup(group);
            }
            return if view.value {
                QualifiedTypePathResolution::ValueOnlyLeaf { segment }
            } else {
                QualifiedTypePathResolution::MissingMember { segment }
            };
        }

        QualifiedTypePathResolution::MissingMember {
            segment: segments.len() - 1,
        }
    }

    fn namespace_for_lookup_scope(&self, scope: ScopeId) -> Option<NamespaceId> {
        match self
            .namespaces
            .declaration_owners_by_scope
            .get(&scope)
            .copied()
        {
            Some(DeclarationOwner::NamespacePublic(namespace)) => Some(namespace),
            Some(DeclarationOwner::NamespacePrivate(fragment)) => self
                .namespaces
                .fragment(fragment)
                .map(|fragment| fragment.namespace),
            Some(DeclarationOwner::Lexical(_))
            | Some(DeclarationOwner::CompilationGlobal)
            | Some(DeclarationOwner::DeferredAmbientModule(_))
            | None => None,
        }
    }

    fn root_merge_record(&self, scope: ScopeId, name: &str) -> Option<&MergeRecord> {
        let key = MergeKey {
            owner: DeclarationOwner::Lexical(scope),
            name: name.to_owned(),
        };
        self.namespaces
            .merge_indices
            .get(&key)
            .and_then(|index| self.namespaces.merges.get(*index))
    }

    fn merge_deferred_reason(record: &MergeRecord) -> Option<QualifiedTypePathDeferredReason> {
        if record
            .declarations
            .iter()
            .any(|declaration| declaration.kind == MergeDeclarationKind::ImportAlias)
        {
            Some(QualifiedTypePathDeferredReason::Import)
        } else if record
            .declarations
            .iter()
            .any(|declaration| declaration.kind == MergeDeclarationKind::Enum)
        {
            Some(QualifiedTypePathDeferredReason::Enum)
        } else {
            None
        }
    }

    fn has_qualified_import_namespace_target(record: &MergeRecord) -> bool {
        record
            .classification
            .compositions
            .iter()
            .any(|composition| {
                matches!(
                    composition.kind,
                    MergeCompositionKind::ImportNamespace(
                        ImportBindingForm::Named | ImportBindingForm::Default
                    )
                ) && matches!(
                    composition.disposition,
                    MergeDisposition::Admitted | MergeDisposition::DeferredBacklog15
                )
            })
    }

    fn qualified_symbol_view(
        &self,
        owner: Option<NamespaceId>,
        symbol: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> QualifiedSymbolView {
        if visited.contains(&symbol) {
            return QualifiedSymbolView {
                unavailable: true,
                ..QualifiedSymbolView::default()
            };
        }
        visited.push(symbol);
        let mut view = self
            .symbols
            .get(symbol)
            .map(|symbol| QualifiedSymbolView {
                namespace: symbol.ns,
                type_group: symbol.ty,
                value: symbol.value.is_some(),
                unavailable: false,
                deferred: None,
            })
            .unwrap_or_default();

        if let Some(owner) = owner {
            for member in self.namespace_members_for_symbol(owner, symbol) {
                match member.kind {
                    MergeDeclarationKind::ImportAlias => {
                        view.deferred = Some(QualifiedTypePathDeferredReason::Import);
                        break;
                    }
                    MergeDeclarationKind::Enum => {
                        view.deferred = Some(QualifiedTypePathDeferredReason::Enum);
                        break;
                    }
                    MergeDeclarationKind::DeferredExport if member.declaration.is_none() => {
                        if member.module_specifier.is_some() {
                            view.deferred = Some(QualifiedTypePathDeferredReason::Import);
                            break;
                        }
                        let Some(target) = member.local_symbol else {
                            view = QualifiedSymbolView {
                                unavailable: true,
                                ..QualifiedSymbolView::default()
                            };
                            break;
                        };
                        let mut target_view =
                            self.qualified_symbol_view(Some(owner), target, visited);
                        if member.alias_space_intent == Some(AliasSpaceIntent::Type) {
                            target_view.value = false;
                        }
                        view = target_view;
                        break;
                    }
                    _ => {
                        view.value |= member.spaces.value;
                    }
                }
            }
        }
        visited.pop();
        view
    }

    fn namespace_members_for_symbol(
        &self,
        namespace: NamespaceId,
        symbol: SymbolId,
    ) -> Vec<&NamespaceMember> {
        self.namespaces
            .get(namespace)
            .into_iter()
            .flat_map(|namespace| namespace.fragments.iter())
            .filter_map(|fragment| self.namespaces.fragment(*fragment))
            .flat_map(|fragment| fragment.members.iter())
            .filter_map(|member| self.namespaces.member(*member))
            .filter(|member| member.symbol == Some(symbol))
            .collect()
    }
}

fn join_instance_state(
    left: NamespaceInstanceState,
    right: NamespaceInstanceState,
) -> NamespaceInstanceState {
    match (left, right) {
        (NamespaceInstanceState::Instantiated, _) | (_, NamespaceInstanceState::Instantiated) => {
            NamespaceInstanceState::Instantiated
        }
        _ => NamespaceInstanceState::NonInstantiated,
    }
}

fn namespace_value_attachment_disposition(
    record: &MergeRecord,
) -> Option<NamespaceValueAttachmentDisposition> {
    let has_namespace = record
        .declarations
        .iter()
        .any(|participant| participant.kind == MergeDeclarationKind::Namespace);
    if !has_namespace {
        return None;
    }
    let has_function = record
        .classification
        .compositions
        .iter()
        .any(|composition| {
            composition.kind == MergeCompositionKind::FunctionNamespace
                && composition.disposition == MergeDisposition::Admitted
        });
    let has_enum = record
        .declarations
        .iter()
        .any(|participant| participant.kind == MergeDeclarationKind::Enum);
    let only_enum_function_namespace = record.declarations.iter().all(|participant| {
        matches!(
            participant.kind,
            MergeDeclarationKind::Enum
                | MergeDeclarationKind::Function
                | MergeDeclarationKind::Namespace
        )
    });
    if record.classification.disposition == MergeDisposition::DeferredBacklog42
        && has_function
        && has_enum
        && only_enum_function_namespace
    {
        return Some(NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42);
    }
    if record.classification.disposition != MergeDisposition::Admitted {
        return Some(NamespaceValueAttachmentDisposition::Rejected(
            record.classification.disposition,
        ));
    }
    let has_class = record
        .classification
        .compositions
        .iter()
        .any(|composition| {
            composition.kind == MergeCompositionKind::ClassNamespace
                && composition.disposition == MergeDisposition::Admitted
        });
    match (has_function, has_class) {
        (true, false) => Some(NamespaceValueAttachmentDisposition::AdmittedFunction),
        (false, true) => Some(NamespaceValueAttachmentDisposition::AdmittedClass),
        (false, false) => Some(NamespaceValueAttachmentDisposition::TypeContainerOnly),
        (true, true) => Some(NamespaceValueAttachmentDisposition::Rejected(
            MergeDisposition::RejectedRedeclaration,
        )),
    }
}

fn classify_group(declarations: &[MergeParticipant]) -> MergeClassification {
    use MergeDeclarationKind::{
        Class, Enum, Function, ImportAlias, Interface, Namespace, Variable,
    };
    let count = |kind| declarations.iter().filter(|item| item.kind == kind).count();
    let namespaces = count(Namespace);
    let interfaces = count(Interface);
    let functions = count(Function);
    let classes = count(Class);
    let variables = count(Variable);
    let imports = count(ImportAlias);
    let total = declarations.len();
    let mut compositions = Vec::new();
    let mut push = |kind, disposition| {
        let composition = MergeComposition { kind, disposition };
        if !compositions.contains(&composition) {
            compositions.push(composition);
        }
    };

    let slot_state = |slot: fn(DeclarationSpaces) -> bool| MergeSlotState {
        declarations: declarations.iter().filter(|item| slot(item.spaces)).count(),
        disposition: MergeSlotDisposition::Single,
    };
    let mut value = slot_state(|spaces| spaces.value);
    let mut r#type = slot_state(|spaces| spaces.r#type);
    let mut namespace = slot_state(|spaces| spaces.namespace);
    for slot in [&mut value, &mut r#type, &mut namespace] {
        if slot.declarations == 0 {
            slot.disposition = MergeSlotDisposition::Empty;
        }
    }

    if value.declarations > 1 {
        if functions == value.declarations {
            value.disposition = MergeSlotDisposition::AdmittedMerge;
            push(
                MergeCompositionKind::FunctionGroup,
                MergeDisposition::Admitted,
            );
        } else if variables == value.declarations
            && declarations.iter().all(|item| {
                item.kind != Variable
                    || item.syntax == DeclarationSyntaxFacts::Variable(VariableKind::Var)
            })
        {
            value.disposition = MergeSlotDisposition::Deferred;
            push(
                MergeCompositionKind::VariableGroup,
                MergeDisposition::DeferredBacklog15,
            );
        } else if declarations.iter().any(|item| item.kind == Enum) {
            value.disposition = MergeSlotDisposition::Deferred;
        } else {
            value.disposition = MergeSlotDisposition::Rejected;
            push(
                MergeCompositionKind::ConflictingValueDeclarations,
                MergeDisposition::RejectedRedeclaration,
            );
        }
    }

    if r#type.declarations > 1 {
        if interfaces == r#type.declarations {
            r#type.disposition = MergeSlotDisposition::AdmittedMerge;
            push(
                MergeCompositionKind::InterfaceGroup,
                MergeDisposition::Admitted,
            );
        } else if classes == 1 && classes + interfaces == r#type.declarations {
            r#type.disposition = MergeSlotDisposition::AdmittedMerge;
            push(
                MergeCompositionKind::ClassInterface,
                MergeDisposition::Admitted,
            );
        } else if declarations.iter().any(|item| item.kind == Enum) {
            r#type.disposition = MergeSlotDisposition::Deferred;
        } else {
            r#type.disposition = MergeSlotDisposition::Rejected;
            push(
                MergeCompositionKind::ConflictingTypeDeclarations,
                MergeDisposition::RejectedRedeclaration,
            );
        }
    }

    if namespace.declarations > 1 {
        namespace.disposition = MergeSlotDisposition::AdmittedMerge;
        push(
            MergeCompositionKind::NamespaceGroup,
            MergeDisposition::Admitted,
        );
    }
    let occupied_slots = [&value, &r#type, &namespace]
        .iter()
        .filter(|slot| slot.declarations > 0)
        .count();
    if total > 1 && occupied_slots > 1 {
        push(
            MergeCompositionKind::IndependentOccupiedSlots,
            MergeDisposition::Admitted,
        );
    }
    if namespaces > 0 && functions > 0 {
        push(
            MergeCompositionKind::FunctionNamespace,
            MergeDisposition::Admitted,
        );
    }
    if namespaces > 0 && classes > 0 {
        push(
            MergeCompositionKind::ClassNamespace,
            MergeDisposition::Admitted,
        );
    }
    if namespaces > 0 && interfaces > 0 {
        push(
            MergeCompositionKind::InterfaceNamespace,
            MergeDisposition::Admitted,
        );
    }
    if namespaces > 0 && variables > 0 {
        if declarations.iter().any(|item| {
            item.kind == Namespace
                && item.namespace_instance == Some(NamespaceInstanceState::Instantiated)
        }) {
            push(
                MergeCompositionKind::VariableNamespaceRuntime,
                MergeDisposition::RejectedRuntimeNamespace,
            );
        } else {
            push(
                MergeCompositionKind::VariableNamespaceNonInstantiated,
                MergeDisposition::Admitted,
            );
        }
    }
    if namespaces > 0 && imports > 0 {
        for facts in declarations.iter().filter_map(|item| match item.syntax {
            DeclarationSyntaxFacts::Import(facts) => Some(facts),
            _ => None,
        }) {
            let type_only = facts.outer_type_only || facts.specifier_type_only;
            let disposition = match facts.form {
                ImportBindingForm::Namespace | ImportBindingForm::ImportEquals => {
                    MergeDisposition::RejectedFutureTk2440
                }
                ImportBindingForm::Named | ImportBindingForm::Default if type_only => {
                    MergeDisposition::Admitted
                }
                ImportBindingForm::Named | ImportBindingForm::Default => {
                    MergeDisposition::DeferredBacklog15
                }
            };
            push(
                MergeCompositionKind::ImportNamespace(facts.form),
                disposition,
            );
        }
    }
    let enum_composition = total > 1 && declarations.iter().any(|item| item.kind == Enum);
    if enum_composition {
        push(
            MergeCompositionKind::EnumComposition,
            MergeDisposition::DeferredBacklog42,
        );
    }

    let disposition = if enum_composition {
        MergeDisposition::DeferredBacklog42
    } else if compositions
        .iter()
        .any(|item| item.disposition == MergeDisposition::RejectedFutureTk2440)
    {
        MergeDisposition::RejectedFutureTk2440
    } else if compositions
        .iter()
        .any(|item| item.disposition == MergeDisposition::RejectedRuntimeNamespace)
    {
        MergeDisposition::RejectedRuntimeNamespace
    } else if compositions
        .iter()
        .any(|item| item.disposition == MergeDisposition::RejectedRedeclaration)
    {
        MergeDisposition::RejectedRedeclaration
    } else if compositions
        .iter()
        .any(|item| item.disposition == MergeDisposition::DeferredBacklog15)
    {
        MergeDisposition::DeferredBacklog15
    } else {
        MergeDisposition::Admitted
    };
    MergeClassification {
        slots: MergeSlotSummary {
            value,
            r#type,
            namespace,
        },
        compositions,
        disposition,
    }
}

fn placement_issues(declarations: &[MergeParticipant]) -> Vec<PlacementIssue> {
    let last_value = declarations.iter().rfind(|item| {
        matches!(
            item.kind,
            MergeDeclarationKind::Function | MergeDeclarationKind::Class
        ) && !item.ambient
    });
    let Some(last_value) = last_value else {
        return Vec::new();
    };
    declarations
        .iter()
        .filter(|item| {
            item.kind == MergeDeclarationKind::Namespace
                && !item.ambient
                && item.namespace_instance == Some(NamespaceInstanceState::Instantiated)
                && (item.source, item.span.start) < (last_value.source, last_value.span.start)
        })
        .map(|item| PlacementIssue {
            kind: PlacementIssueKind::FutureTk2434,
            owner: item.declaration,
            source: item.source,
            origin: item.origin,
            span: item.binding_span,
        })
        .collect()
}

#[derive(Copy, Clone)]
struct WalkContext {
    owner: DeclarationOwner,
    lexical_scope: ScopeId,
    namespace: Option<(NamespaceId, NamespaceFragmentId)>,
    global: Option<GlobalAugmentationId>,
    deferred_module: Option<DeferredModuleId>,
    ambient: bool,
    ambient_export_list_mode: bool,
    active_export_context: Option<ExportContextId>,
    direct_top_level: bool,
}

impl WalkContext {
    fn member_owner(self) -> Option<NamespaceMemberOwner> {
        if let Some((_, fragment)) = self.namespace {
            Some(NamespaceMemberOwner::Fragment(fragment))
        } else if let Some(global) = self.global {
            Some(NamespaceMemberOwner::GlobalAugmentation(global))
        } else {
            self.deferred_module
                .map(NamespaceMemberOwner::DeferredAmbientModule)
        }
    }

    fn publication(self, explicit: bool) -> NamespacePublication {
        if explicit {
            NamespacePublication::Explicit
        } else if self.ambient && !self.ambient_export_list_mode {
            NamespacePublication::AmbientDefault
        } else {
            NamespacePublication::Private
        }
    }

    fn declaration_owner(self, publication: NamespacePublication) -> DeclarationOwner {
        match self.namespace {
            Some((namespace, fragment)) => match publication {
                NamespacePublication::Explicit
                | NamespacePublication::AmbientDefault
                | NamespacePublication::DottedImplicit => {
                    DeclarationOwner::NamespacePublic(namespace)
                }
                NamespacePublication::Private => DeclarationOwner::NamespacePrivate(fragment),
            },
            None => self.owner,
        }
    }

    fn namespace_owner(self, publication: NamespacePublication) -> Option<NamespaceOwner> {
        match self.namespace {
            Some((namespace, fragment)) => Some(match publication {
                NamespacePublication::Explicit
                | NamespacePublication::AmbientDefault
                | NamespacePublication::DottedImplicit => {
                    NamespaceOwner::NamespacePublic(namespace)
                }
                NamespacePublication::Private => NamespaceOwner::FragmentPrivate(fragment),
            }),
            None => Some(match self.owner {
                DeclarationOwner::Lexical(scope) => NamespaceOwner::Lexical(scope),
                DeclarationOwner::NamespacePublic(namespace) => {
                    NamespaceOwner::NamespacePublic(namespace)
                }
                DeclarationOwner::NamespacePrivate(fragment) => {
                    NamespaceOwner::FragmentPrivate(fragment)
                }
                DeclarationOwner::CompilationGlobal => NamespaceOwner::CompilationGlobal,
                DeclarationOwner::DeferredAmbientModule(_) => return None,
            }),
        }
    }
}

#[derive(Copy, Clone)]
pub(super) enum NamespaceMetadataRoot {
    Module,
    LibrarySharedGlobal,
}

/// Collect namespace topology after ordinary declarations.
pub(super) fn collect_namespace_metadata(
    state: &mut BindState,
    module: ScopeId,
    program: &Program<'_>,
    unit: CompilationUnit,
    compilation_global: ScopeId,
    script_namespace_root: ScopeId,
    root: NamespaceMetadataRoot,
) {
    state.namespaces.source_units.push_local(SourceUnitRecord {
        source: unit.source,
        origin: unit.origin,
        module,
        context: unit.binding,
    });
    state.namespaces.compilation_global = Some(compilation_global);
    state.namespaces.script_namespace_root = Some(script_namespace_root);
    if matches!(root, NamespaceMetadataRoot::LibrarySharedGlobal) {
        state.namespaces.library_shared_globals = true;
    }
    let (owner, lexical_scope) = match root {
        NamespaceMetadataRoot::Module => (DeclarationOwner::Lexical(module), module),
        NamespaceMetadataRoot::LibrarySharedGlobal if unit.binding.external_module => {
            (DeclarationOwner::Lexical(module), module)
        }
        NamespaceMetadataRoot::LibrarySharedGlobal => {
            (DeclarationOwner::CompilationGlobal, compilation_global)
        }
    };
    let context = WalkContext {
        owner,
        lexical_scope,
        namespace: None,
        global: None,
        deferred_module: None,
        ambient: unit.binding.declaration_file(),
        ambient_export_list_mode: false,
        active_export_context: None,
        direct_top_level: true,
    };
    walk_statements(state, &program.body, context, unit, compilation_global);
}

pub(super) fn finalize_namespace_metadata(state: &mut BindState) {
    resolve_local_ambient_export_alias_targets(state);
    state
        .namespaces
        .classify()
        .unwrap_or_else(|error| panic!("namespace classification failed: {error}"));
}

pub(super) fn fill_namespace_value_attachments(
    state: &mut BindState,
    program: &Program<'_>,
    plan: &NamespaceValueAttachmentPlan,
) {
    bind_namespace_value_attachment_members(state, program, plan);
}

/// Everything one project module contributes on its own. Classification is a whole-project
/// pass, so the builder runs it once after the module loop instead of once per module.
pub(crate) fn collect_project_namespace_metadata(
    state: &mut BindState,
    module: ScopeId,
    program: &Program<'_>,
    unit: CompilationUnit,
    compilation_global: ScopeId,
    script_namespace_root: ScopeId,
) {
    collect_namespace_metadata(
        state,
        module,
        program,
        unit,
        compilation_global,
        script_namespace_root,
        NamespaceMetadataRoot::Module,
    );
    publish_continuation_hoisted_variables(state, unit);
}

fn publish_continuation_hoisted_variables(state: &mut BindState, unit: CompilationUnit) {
    for (binding_start, name) in state.continuation_publication_sites() {
        let Some(declaration) = state.source_decl_at(binding_start, DeclarationKind::Variable)
        else {
            continue;
        };
        let key = MergeKey {
            owner: DeclarationOwner::CompilationGlobal,
            name: name.clone(),
        };
        if state
            .namespaces
            .placements
            .get(&key)
            .is_some_and(|participants| {
                participants
                    .iter()
                    .any(|participant| participant.declaration == declaration)
            })
        {
            continue;
        }
        push_placement(
            state,
            DeclarationOwner::CompilationGlobal,
            &name,
            declaration,
            MergeDeclarationKind::Variable,
            DeclarationSpaces::VALUE,
            unit.binding.declaration_file(),
            unit,
            DeclarationSyntaxFacts::Variable(VariableKind::Var),
        );
    }
}

#[derive(Clone)]
struct NamespaceValueBindingTarget {
    member: NamespaceMemberId,
    declaration: DeclId,
    name: String,
    /// The module that declares the member — the one fill that must bind it.
    module: ScopeId,
    scope: ScopeId,
    kind: MergeDeclarationKind,
    public_symbol: Option<SymbolId>,
}

/// Every namespace value attachment the batch will bind, derived once and filed by the module
/// that declares each member.
pub(crate) struct NamespaceValueAttachmentPlan {
    /// Sorted and deduplicated exactly once, so the surviving target of a declaration that two
    /// merges reach is decided by one whole-project neighbourhood rather than by whatever
    /// subset a module happened to see.
    targets: Vec<NamespaceValueBindingTarget>,
    /// Indexes into `targets`, ascending, so each module's fill keeps the project order.
    by_module: FxHashMap<ScopeId, Vec<usize>>,
    /// Whether the batch owes a whole-project replay after its fills; see
    /// `replay_namespace_value_attachments`.
    replays_every_target: bool,
}

impl NamespaceValueAttachmentPlan {
    fn targets_for(&self, module: ScopeId) -> &[usize] {
        self.by_module
            .get(&module)
            .map_or(&[][..], |indexes| indexes.as_slice())
    }
}

/// Derive the batch's value attachments once, from the merge set classification just froze.
///
/// Classification runs once before the fill loop, so the merge set no longer changes while the
/// batch fills. One collection therefore produces byte-for-byte the list every fill used to
/// rebuild for itself, and one sort/dedup keeps the whole-project neighbourhood that decides
/// which of two targets for the same declaration wins — narrowing the *scan* per module would
/// have changed that neighbourhood, which is why the plan is built whole and only its
/// application is split.
pub(super) fn plan_namespace_value_attachments(state: &BindState) -> NamespaceValueAttachmentPlan {
    let mut targets = Vec::new();
    for record in state.namespaces.local_merges() {
        #[cfg(test)]
        record_finalization_attachment_merge_row();
        let attached = matches!(
            namespace_value_attachment_disposition(record),
            Some(
                NamespaceValueAttachmentDisposition::AdmittedFunction
                    | NamespaceValueAttachmentDisposition::AdmittedClass
                    | NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42
            )
        );
        let inaccessible_owner = matches!(record.owner, DeclarationOwner::DeferredAmbientModule(_))
            || (record.owner == DeclarationOwner::CompilationGlobal
                && !state.namespaces.uses_library_shared_globals());
        if (!attached && !state.namespaces.is_admitted_instantiated_standalone(record))
            || inaccessible_owner
        {
            continue;
        }
        for fragment in record
            .declarations
            .iter()
            .filter_map(|participant| participant.namespace_fragment)
            .filter_map(|fragment| state.namespaces.fragment(fragment))
        {
            for member in fragment
                .members
                .iter()
                .filter_map(|member| state.namespaces.member(*member))
                .filter(|member| {
                    member.spaces.value
                        && matches!(
                            member.kind,
                            MergeDeclarationKind::Variable
                                | MergeDeclarationKind::Function
                                | MergeDeclarationKind::Class
                        )
                })
            {
                let (Some(declaration), Some(name)) = (member.declaration, member.name.as_ref())
                else {
                    continue;
                };
                let public_symbol = if matches!(member.publication, NamespacePublication::Private) {
                    None
                } else {
                    let Some(symbol) = member.symbol else {
                        continue;
                    };
                    Some(symbol)
                };
                // A merge is not owned by one module: `namespace N` reopened in a second file
                // leaves one record whose fragments live in two modules, and in a library batch
                // a `function` in one file merges with a `namespace` in another (the project
                // path never merges those two — a top-level function is owned by its module
                // scope while the namespace is owned by the shared script root, so they land in
                // different records). Keying on the *member's* declaring module is what makes
                // every participating module fill its own members; keying on the record's owner
                // would drop the other file's.
                let Some((module, scope)) =
                    state.declarations.get(declaration).and_then(|declaration| {
                        declaration
                            .site
                            .scope
                            .map(|scope| (declaration.site.module, scope))
                    })
                else {
                    continue;
                };
                targets.push(NamespaceValueBindingTarget {
                    member: member.id,
                    declaration,
                    name: name.clone(),
                    module,
                    scope,
                    kind: member.kind,
                    public_symbol,
                });
            }
        }
    }
    targets.sort_by_key(|target| {
        state
            .declarations
            .get(target.declaration)
            .map(|declaration| {
                (
                    declaration.site.declaration_span.start,
                    target.declaration.0,
                )
            })
            .unwrap_or((u32::MAX, u32::MAX))
    });
    targets.dedup_by_key(|target| target.declaration);
    let mut by_module: FxHashMap<ScopeId, Vec<usize>> = FxHashMap::default();
    for (index, target) in targets.iter().enumerate() {
        by_module.entry(target.module).or_default().push(index);
    }
    NamespaceValueAttachmentPlan {
        targets,
        by_module,
        replays_every_target: !state.namespaces.uses_library_shared_globals(),
    }
}

fn bind_namespace_value_attachment_members(
    state: &mut BindState,
    program: &Program<'_>,
    plan: &NamespaceValueAttachmentPlan,
) {
    let indexes = plan.targets_for(state.current_module);
    // Only this module's declarations can be selected: `source_decl_at` resolves span starts
    // against `current_module`, so a foreign `DeclId` could never match this program anyway.
    let scopes = indexes
        .iter()
        .filter_map(|index| plan.targets.get(*index))
        .map(|target| (target.declaration, target.scope))
        .collect::<FxHashMap<_, _>>();
    bind_selected_namespace_value_statements(state, &program.body, &scopes);

    for target in indexes.iter().filter_map(|index| plan.targets.get(*index)) {
        apply_namespace_value_attachment(state, target);
    }
}

/// A library-shared-globals batch always filled one module's members at a time, but without
/// that flag every fill re-applied the *whole* project's target set — so the `symbol.value`
/// that survived for a name declared in two files is the globally last declaration's, not the
/// last file's. Replaying the whole plan once, in the same project order, reproduces exactly
/// that final state; the flagged path must not replay, because it never re-applied anything.
/// The replay is also what keeps a target whose module is outside this batch (a frozen library
/// module reached through a local merge) written, precisely as the repeated fills wrote it.
pub(super) fn replay_namespace_value_attachments(
    state: &mut BindState,
    plan: &NamespaceValueAttachmentPlan,
) {
    if !plan.replays_every_target {
        return;
    }
    for target in &plan.targets {
        apply_namespace_value_attachment(state, target);
    }
}

fn apply_namespace_value_attachment(state: &mut BindState, target: &NamespaceValueBindingTarget) {
    let storage = state
        .declarations
        .get(target.declaration)
        .and_then(|declaration| declaration.value_storage);
    let local_symbol = state
        .graph
        .get(target.scope)
        .and_then(|scope| scope.lookup_local(&target.name));
    if let Some(member) = state
        .namespaces
        .members
        .get_mut_local(target.member.index())
    {
        member.local_symbol = local_symbol;
    }
    let Some(symbol) = target.public_symbol else {
        return;
    };
    let Some(storage) = storage else {
        return;
    };
    if let Some(symbol) = state.symbols.get_mut(symbol) {
        symbol.value = Some(storage);
        if target.kind == MergeDeclarationKind::Function
            && !symbol.function_values.contains(&storage)
        {
            symbol.function_values.push(storage);
        }
    }
}

fn bind_selected_namespace_value_statements(
    state: &mut BindState,
    statements: &[Statement<'_>],
    scopes: &FxHashMap<DeclId, ScopeId>,
) {
    for statement in statements {
        match statement {
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    bind_selected_namespace_value_declaration(state, declaration, scopes);
                }
            }
            Statement::VariableDeclaration(declaration) => {
                bind_selected_namespace_variable(state, declaration, scopes)
            }
            Statement::FunctionDeclaration(function) => {
                let selected = function.id.as_ref().and_then(|identifier| {
                    state
                        .source_decl_at(identifier.span.start, DeclarationKind::Function)
                        .and_then(|declaration| {
                            scopes
                                .get(&declaration)
                                .copied()
                                .map(|scope| (declaration, scope))
                        })
                });
                if let Some((_, scope)) = selected {
                    bind_function_declaration(state, scope, function);
                }
            }
            Statement::ClassDeclaration(class) => {
                let selected = class.id.as_ref().and_then(|identifier| {
                    state
                        .source_decl_at(identifier.span.start, DeclarationKind::Class)
                        .and_then(|declaration| {
                            scopes
                                .get(&declaration)
                                .copied()
                                .map(|scope| (declaration, scope))
                        })
                });
                if let Some((_, scope)) = selected {
                    bind_class_declaration(state, scope, class);
                }
            }
            Statement::TSModuleDeclaration(declaration) => {
                bind_selected_namespace_module_body(state, declaration, scopes)
            }
            _ => {}
        }
    }
}

fn bind_selected_namespace_value_declaration(
    state: &mut BindState,
    declaration: &Declaration<'_>,
    scopes: &FxHashMap<DeclId, ScopeId>,
) {
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            bind_selected_namespace_variable(state, declaration, scopes)
        }
        Declaration::FunctionDeclaration(function) => {
            let scope = function.id.as_ref().and_then(|identifier| {
                state
                    .source_decl_at(identifier.span.start, DeclarationKind::Function)
                    .and_then(|declaration| scopes.get(&declaration).copied())
            });
            if let Some(scope) = scope {
                bind_function_declaration(state, scope, function);
            }
        }
        Declaration::ClassDeclaration(class) => {
            let scope = class.id.as_ref().and_then(|identifier| {
                state
                    .source_decl_at(identifier.span.start, DeclarationKind::Class)
                    .and_then(|declaration| scopes.get(&declaration).copied())
            });
            if let Some(scope) = scope {
                bind_class_declaration(state, scope, class);
            }
        }
        Declaration::TSModuleDeclaration(declaration) => {
            bind_selected_namespace_module_body(state, declaration, scopes)
        }
        _ => {}
    }
}

fn bind_selected_namespace_variable(
    state: &mut BindState,
    declaration: &oxc_ast::ast::VariableDeclaration<'_>,
    scopes: &FxHashMap<DeclId, ScopeId>,
) {
    for declarator in &declaration.declarations {
        let scope = declarator
            .id
            .get_binding_identifiers()
            .into_iter()
            .find_map(|identifier| {
                state
                    .source_decl_at(identifier.span.start, DeclarationKind::Variable)
                    .and_then(|declaration| scopes.get(&declaration).copied())
            });
        if let Some(scope) = scope {
            bind_declarator(state, scope, declaration.kind, declarator);
        }
    }
}

fn bind_selected_namespace_module_body(
    state: &mut BindState,
    declaration: &TSModuleDeclaration<'_>,
    scopes: &FxHashMap<DeclId, ScopeId>,
) {
    match &declaration.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            bind_selected_namespace_value_statements(state, &block.body, scopes)
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
            bind_selected_namespace_module_body(state, nested, scopes)
        }
        None => {}
    }
}

fn resolve_local_ambient_export_alias_targets(state: &mut BindState) {
    let member_base = state.namespaces.members.base_len();
    #[cfg(test)]
    record_finalization_alias_member_rows(state.namespaces.members.local_len());
    let candidates = state
        .namespaces
        .members
        .local_iter()
        .enumerate()
        .filter(|(_, member)| {
            matches!(member.owner, NamespaceMemberOwner::Fragment(_))
                && member.kind == MergeDeclarationKind::DeferredExport
                && member.declaration.is_none()
                && member.alias_context == Some(AliasContext::ValidAmbient)
                && member.module_specifier.is_none()
        })
        .filter_map(|(local_index, member)| {
            let NamespaceMemberOwner::Fragment(fragment) = member.owner else {
                return None;
            };
            Some((
                member_base + local_index,
                fragment,
                member.local_name.as_ref()?.text().to_string(),
            ))
        })
        .collect::<Vec<_>>();

    for (index, fragment, local_name) in candidates {
        let Some(fragment) = state.namespaces.fragment(fragment) else {
            continue;
        };
        let private_scope = fragment.private_scope;
        let public_scope = state
            .namespaces
            .get(fragment.namespace)
            .map(|namespace| namespace.public_scope);
        let local_symbol =
            local_declaration_symbol(state, private_scope, &local_name).or_else(|| {
                public_scope.and_then(|scope| local_declaration_symbol(state, scope, &local_name))
            });
        if let Some(member) = state.namespaces.members.get_mut_local(index) {
            member.local_symbol = local_symbol;
        }
    }
}

fn local_declaration_symbol(state: &BindState, scope: ScopeId, name: &str) -> Option<SymbolId> {
    state
        .graph
        .get(scope)
        .and_then(|scope| scope.lookup_local(name))
        .filter(|symbol| {
            state
                .symbols
                .get(*symbol)
                .is_some_and(|symbol| !symbol.declarations.is_empty())
        })
}

fn walk_statements(
    state: &mut BindState,
    statements: &[Statement<'_>],
    context: WalkContext,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    for statement in statements {
        walk_statement(state, statement, context, false, unit, compilation_global);
    }
}

fn walk_statement(
    state: &mut BindState,
    statement: &Statement<'_>,
    context: WalkContext,
    explicit: bool,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    match statement {
        Statement::ExportNamedDeclaration(export) => {
            let export_context = push_export_context(
                state,
                context,
                if export.declaration.is_some() {
                    ExportContextKind::WrappedDeclaration
                } else {
                    ExportContextKind::NamedList
                },
                export.source.is_some(),
                Span::from_oxc(export.span),
                unit,
            );
            let export_walk = WalkContext {
                active_export_context: export_context,
                ..context
            };
            if let Some(declaration) = &export.declaration {
                walk_declaration(
                    state,
                    declaration,
                    export_walk,
                    true,
                    unit,
                    compilation_global,
                );
            } else if context.member_owner().is_some() {
                for specifier in &export.specifiers {
                    push_export_alias_member(state, export_walk, export, specifier, unit);
                }
            }
        }
        Statement::ExportAllDeclaration(export) => {
            let context = context_with_export(
                state,
                context,
                ExportContextKind::ExportAll,
                true,
                Span::from_oxc(export.span),
                unit,
            );
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit)
        }
        Statement::ExportDefaultDeclaration(export) => {
            let context = context_with_export(
                state,
                context,
                ExportContextKind::ExportDefault,
                false,
                Span::from_oxc(export.span),
                unit,
            );
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit)
        }
        Statement::TSExportAssignment(export) => {
            let context = context_with_export(
                state,
                context,
                ExportContextKind::ExportAssignment,
                false,
                Span::from_oxc(export.span),
                unit,
            );
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit)
        }
        Statement::VariableDeclaration(declaration) => {
            bind_variable(state, declaration, context, explicit, unit)
        }
        Statement::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                bind_named_declaration(
                    state,
                    context,
                    explicit,
                    identifier.name.as_str(),
                    identifier.span.start,
                    DeclarationKind::Function,
                    MergeDeclarationKind::Function,
                    DeclarationSpaces::VALUE,
                    function.declare,
                    unit,
                );
            }
        }
        Statement::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                bind_named_declaration(
                    state,
                    context,
                    explicit,
                    identifier.name.as_str(),
                    identifier.span.start,
                    DeclarationKind::Class,
                    MergeDeclarationKind::Class,
                    DeclarationSpaces::VALUE_TYPE,
                    class.declare,
                    unit,
                );
            }
        }
        Statement::TSTypeAliasDeclaration(alias) => bind_named_declaration(
            state,
            context,
            explicit,
            alias.id.name.as_str(),
            alias.id.span.start,
            DeclarationKind::TypeAlias,
            MergeDeclarationKind::TypeAlias,
            DeclarationSpaces::TYPE,
            alias.declare,
            unit,
        ),
        Statement::TSInterfaceDeclaration(interface) => bind_named_declaration(
            state,
            context,
            explicit,
            interface.id.name.as_str(),
            interface.id.span.start,
            DeclarationKind::Interface,
            MergeDeclarationKind::Interface,
            DeclarationSpaces::TYPE,
            interface.declare,
            unit,
        ),
        Statement::TSEnumDeclaration(enumeration) => bind_named_declaration_with_syntax(
            state,
            context,
            explicit,
            enumeration.id.name.as_str(),
            enumeration.id.span.start,
            DeclarationKind::Enum,
            MergeDeclarationKind::Enum,
            DeclarationSpaces::VALUE_TYPE,
            enumeration.declare,
            unit,
            DeclarationSyntaxFacts::Enum {
                constant: enumeration.r#const,
            },
        ),
        Statement::TSModuleDeclaration(namespace) => bind_module_declaration(
            state,
            namespace,
            context,
            explicit,
            false,
            unit,
            compilation_global,
        ),
        Statement::TSGlobalDeclaration(global) => {
            bind_global(state, global, context, unit, compilation_global)
        }
        Statement::TSImportEqualsDeclaration(import) => {
            bind_named_declaration_with_syntax(
                state,
                context,
                explicit,
                import.id.name.as_str(),
                import.id.span.start,
                DeclarationKind::ImportEquals,
                MergeDeclarationKind::ImportAlias,
                if import.import_kind == ImportOrExportKind::Type {
                    DeclarationSpaces::TYPE
                } else {
                    DeclarationSpaces::ALIAS
                },
                context.ambient,
                unit,
                DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
                    form: ImportBindingForm::ImportEquals,
                    outer_type_only: import.import_kind == ImportOrExportKind::Type,
                    specifier_type_only: false,
                    external_reference: matches!(
                        import.module_reference,
                        TSModuleReference::ExternalModuleReference(_)
                    ),
                    exported: false,
                }),
            );
        }
        Statement::ImportDeclaration(import) => {
            if let Some(specifiers) = &import.specifiers {
                for specifier in specifiers {
                    let local = specifier.local();
                    let specifier_type_only = matches!(
                        specifier,
                        ImportDeclarationSpecifier::ImportSpecifier(named)
                            if named.import_kind == ImportOrExportKind::Type
                    );
                    let outer_type_only = import.import_kind == ImportOrExportKind::Type;
                    let form = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(_) => ImportBindingForm::Named,
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {
                            ImportBindingForm::Default
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => {
                            ImportBindingForm::Namespace
                        }
                    };
                    let spaces = if outer_type_only || specifier_type_only {
                        DeclarationSpaces::TYPE
                    } else {
                        DeclarationSpaces::ALIAS
                    };
                    bind_named_declaration_with_syntax(
                        state,
                        context,
                        explicit,
                        local.name.as_str(),
                        local.span.start,
                        DeclarationKind::Import,
                        MergeDeclarationKind::ImportAlias,
                        spaces,
                        context.ambient,
                        unit,
                        DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
                            form,
                            outer_type_only,
                            specifier_type_only,
                            external_reference: true,
                            exported: false,
                        }),
                    );
                }
            }
        }
        Statement::TSNamespaceExportDeclaration(export) => {
            let declaration = if context.deferred_module.is_some() {
                state
                    .source_decl_at(export.id.span.start, DeclarationKind::NamespaceExport)
                    .expect("namespace export source declaration exists")
            } else {
                state.attach_declaration_scope(
                    export.id.span.start,
                    DeclarationKind::NamespaceExport,
                    context.lexical_scope,
                )
            };
            let umd_context = if !context.direct_top_level {
                UmdContext::FutureTk1316Nested
            } else if !unit.binding.external_module {
                UmdContext::FutureTk1314NonExternal
            } else if !unit.binding.declaration_file() {
                UmdContext::FutureTk1315Implementation
            } else {
                UmdContext::DeferredValidBacklog15
            };
            state.namespaces.umd_exports.push_local(UmdNamespaceExport {
                declaration,
                source: unit.source,
                origin: unit.origin,
                module: state.current_module,
                owner: context.owner,
                name: export.id.name.to_string(),
                span: Span::from_oxc(export.span),
                context: umd_context,
            });
        }
        Statement::BlockStatement(_)
        | Statement::IfStatement(_)
        | Statement::SwitchStatement(_)
        | Statement::WhileStatement(_)
        | Statement::LabeledStatement(_)
        | Statement::ForStatement(_)
        | Statement::ForInStatement(_)
        | Statement::ForOfStatement(_)
        | Statement::DoWhileStatement(_)
        | Statement::ThrowStatement(_)
        | Statement::TryStatement(_)
        | Statement::ExpressionStatement(_)
            if context.namespace.is_some() =>
        {
            super::bind::bind_statement(state, context.lexical_scope, statement)
        }
        _ => {}
    }
}

fn walk_declaration(
    state: &mut BindState,
    declaration: &Declaration<'_>,
    context: WalkContext,
    explicit: bool,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            bind_variable(state, declaration, context, explicit, unit)
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                bind_named_declaration(
                    state,
                    context,
                    explicit,
                    identifier.name.as_str(),
                    identifier.span.start,
                    DeclarationKind::Function,
                    MergeDeclarationKind::Function,
                    DeclarationSpaces::VALUE,
                    function.declare,
                    unit,
                );
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                bind_named_declaration(
                    state,
                    context,
                    explicit,
                    identifier.name.as_str(),
                    identifier.span.start,
                    DeclarationKind::Class,
                    MergeDeclarationKind::Class,
                    DeclarationSpaces::VALUE_TYPE,
                    class.declare,
                    unit,
                );
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => bind_named_declaration(
            state,
            context,
            explicit,
            alias.id.name.as_str(),
            alias.id.span.start,
            DeclarationKind::TypeAlias,
            MergeDeclarationKind::TypeAlias,
            DeclarationSpaces::TYPE,
            alias.declare,
            unit,
        ),
        Declaration::TSInterfaceDeclaration(interface) => bind_named_declaration(
            state,
            context,
            explicit,
            interface.id.name.as_str(),
            interface.id.span.start,
            DeclarationKind::Interface,
            MergeDeclarationKind::Interface,
            DeclarationSpaces::TYPE,
            interface.declare,
            unit,
        ),
        Declaration::TSEnumDeclaration(enumeration) => bind_named_declaration_with_syntax(
            state,
            context,
            explicit,
            enumeration.id.name.as_str(),
            enumeration.id.span.start,
            DeclarationKind::Enum,
            MergeDeclarationKind::Enum,
            DeclarationSpaces::VALUE_TYPE,
            enumeration.declare,
            unit,
            DeclarationSyntaxFacts::Enum {
                constant: enumeration.r#const,
            },
        ),
        Declaration::TSModuleDeclaration(namespace) => bind_module_declaration(
            state,
            namespace,
            context,
            explicit,
            false,
            unit,
            compilation_global,
        ),
        Declaration::TSGlobalDeclaration(global) => {
            bind_global(state, global, context, unit, compilation_global)
        }
        Declaration::TSImportEqualsDeclaration(import) => {
            bind_named_declaration_with_syntax(
                state,
                context,
                explicit,
                import.id.name.as_str(),
                import.id.span.start,
                DeclarationKind::ImportEquals,
                MergeDeclarationKind::ImportAlias,
                if import.import_kind == ImportOrExportKind::Type {
                    DeclarationSpaces::TYPE
                } else {
                    DeclarationSpaces::ALIAS
                },
                context.ambient,
                unit,
                DeclarationSyntaxFacts::Import(ImportSyntaxFacts {
                    form: ImportBindingForm::ImportEquals,
                    outer_type_only: import.import_kind == ImportOrExportKind::Type,
                    specifier_type_only: false,
                    external_reference: matches!(
                        import.module_reference,
                        TSModuleReference::ExternalModuleReference(_)
                    ),
                    exported: false,
                }),
            );
        }
    }
}

fn bind_variable(
    state: &mut BindState,
    declaration: &oxc_ast::ast::VariableDeclaration<'_>,
    context: WalkContext,
    explicit: bool,
    unit: CompilationUnit,
) {
    let kind = match declaration.kind {
        VariableDeclarationKind::Var => VariableKind::Var,
        VariableDeclarationKind::Let => VariableKind::Let,
        VariableDeclarationKind::Const => VariableKind::Const,
        VariableDeclarationKind::Using => VariableKind::Using,
        VariableDeclarationKind::AwaitUsing => VariableKind::AwaitUsing,
    };
    for declarator in &declaration.declarations {
        for identifier in declarator.id.get_binding_identifiers() {
            bind_named_declaration_with_syntax(
                state,
                context,
                explicit,
                identifier.name.as_str(),
                identifier.span.start,
                DeclarationKind::Variable,
                MergeDeclarationKind::Variable,
                DeclarationSpaces::VALUE,
                declaration.declare,
                unit,
                DeclarationSyntaxFacts::Variable(kind),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn bind_named_declaration(
    state: &mut BindState,
    context: WalkContext,
    explicit: bool,
    name: &str,
    binding_start: u32,
    declaration_kind: DeclarationKind,
    merge_kind: MergeDeclarationKind,
    spaces: DeclarationSpaces,
    declared_ambient: bool,
    unit: CompilationUnit,
) {
    bind_named_declaration_with_syntax(
        state,
        context,
        explicit,
        name,
        binding_start,
        declaration_kind,
        merge_kind,
        spaces,
        declared_ambient,
        unit,
        DeclarationSyntaxFacts::None,
    );
}

#[allow(clippy::too_many_arguments)]
fn bind_named_declaration_with_syntax(
    state: &mut BindState,
    context: WalkContext,
    explicit: bool,
    name: &str,
    binding_start: u32,
    declaration_kind: DeclarationKind,
    merge_kind: MergeDeclarationKind,
    spaces: DeclarationSpaces,
    declared_ambient: bool,
    unit: CompilationUnit,
    syntax: DeclarationSyntaxFacts,
) {
    let continuation_global = state.continuation_compilation_global_for(binding_start);
    let declaration =
        state.attach_declaration_scope(binding_start, declaration_kind, context.lexical_scope);
    let publication = context.publication(explicit);
    let owner = continuation_global.map_or_else(
        || context.declaration_owner(publication),
        |_| DeclarationOwner::CompilationGlobal,
    );
    let ambient = context.ambient || declared_ambient;
    let syntax = match syntax {
        DeclarationSyntaxFacts::Import(mut facts) => {
            facts.exported = publication != NamespacePublication::Private;
            DeclarationSyntaxFacts::Import(facts)
        }
        syntax => syntax,
    };
    push_placement(
        state,
        owner,
        name,
        declaration,
        merge_kind,
        spaces,
        ambient,
        unit,
        syntax,
    );
    let legal_global_type = context.global.is_some()
        && owner == DeclarationOwner::CompilationGlobal
        && matches!(
            merge_kind,
            MergeDeclarationKind::TypeAlias | MergeDeclarationKind::Interface
        );
    let already_bound_type = state.namespaces.uses_library_shared_globals()
        && state
            .declarations
            .get(declaration)
            .is_some_and(|row| row.type_group.is_some());
    if (context.namespace.is_some() || legal_global_type) && !already_bound_type {
        let fragment_kind = match merge_kind {
            MergeDeclarationKind::TypeAlias => Some(TypeFragmentKind::TypeAlias),
            MergeDeclarationKind::Interface => Some(TypeFragmentKind::Interface),
            MergeDeclarationKind::Class => Some(TypeFragmentKind::Class),
            _ => None,
        };
        if let (Some(fragment_kind), Some(scope)) =
            (fragment_kind, declaration_owner_scope(state, owner))
        {
            declare_type(state, scope, name, declaration, fragment_kind, unit.source);
        }
    }
    if context.member_owner().is_some() {
        let site = state
            .declarations
            .get(declaration)
            .expect("metadata declaration exists")
            .site;
        push_member(
            state,
            context,
            owner,
            Some(declaration),
            Some(name.to_string()),
            site.declaration_span,
            site.binding_span,
            spaces,
            merge_kind,
            publication,
            unit,
        );
    }
}

fn declaration_owner_scope(state: &BindState, owner: DeclarationOwner) -> Option<ScopeId> {
    match owner {
        DeclarationOwner::Lexical(scope) => Some(scope),
        DeclarationOwner::NamespacePublic(namespace) => state
            .namespaces
            .get(namespace)
            .map(|namespace| namespace.public_scope),
        DeclarationOwner::NamespacePrivate(fragment) => state
            .namespaces
            .fragment(fragment)
            .map(|fragment| fragment.private_scope),
        DeclarationOwner::CompilationGlobal => state.namespaces.compilation_global,
        DeclarationOwner::DeferredAmbientModule(_) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn bind_module_declaration(
    state: &mut BindState,
    declaration: &TSModuleDeclaration<'_>,
    context: WalkContext,
    explicit: bool,
    dotted: bool,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    let TSModuleDeclarationName::Identifier(identifier) = &declaration.id else {
        bind_deferred_module(state, declaration, context, unit, compilation_global);
        return;
    };
    let publication = if dotted {
        NamespacePublication::DottedImplicit
    } else {
        context.publication(explicit)
    };
    let Some(mut owner) = context.namespace_owner(publication) else {
        return;
    };
    let continuation_global = state.continuation_compilation_global_for(identifier.span.start);
    if continuation_global.is_some() {
        owner = NamespaceOwner::CompilationGlobal;
    }
    if context.direct_top_level
        && !unit.binding.external_module
        && continuation_global.is_none()
        && matches!(owner, NamespaceOwner::Lexical(scope) if scope == context.lexical_scope)
    {
        let occupied_local = state
            .graph
            .get(context.lexical_scope)
            .and_then(|scope| scope.lookup_local(identifier.name.as_str()))
            .and_then(|symbol| state.symbols.get(symbol))
            .is_some_and(|symbol| symbol.value.is_some());
        if !occupied_local {
            owner = NamespaceOwner::Lexical(
                state
                    .namespaces
                    .script_namespace_root
                    .expect("script namespace root scope allocated"),
            );
        }
    }
    let declaration_id = state.attach_declaration_scope(
        identifier.span.start,
        DeclarationKind::Namespace,
        context.lexical_scope,
    );
    let namespace = namespace_for(state, owner, identifier.name.as_str());
    state
        .declarations
        .get_mut(declaration_id)
        .expect("namespace declaration exists")
        .namespace = Some(namespace);
    attach_namespace_symbol(state, namespace, declaration_id);
    let public_scope = state
        .namespaces
        .get(namespace)
        .expect("namespace exists")
        .public_scope;
    let private_scope = state.graph.push(
        Scope::new(ScopeKind::NamespacePrivate, Some(context.lexical_scope))
            .with_namespace_public(public_scope),
    );
    let fragment = NamespaceFragmentId(
        u32::try_from(state.namespaces.fragments.len()).expect("namespace fragment count fits u32"),
    );
    state.namespaces.fragments.push_local(NamespaceFragment {
        id: fragment,
        namespace,
        declaration: declaration_id,
        source: unit.source,
        origin: unit.origin,
        source_start: declaration.span.start,
        module: state.current_module,
        private_scope,
        lexical_parent: context.lexical_scope,
        public_scope,
        ambient: context.ambient || declaration.declare || unit.binding.declaration_file(),
        publication,
        instance_state: NamespaceInstanceState::NonInstantiated,
        members: Vec::new(),
    });
    state
        .namespaces
        .namespaces
        .get_mut_local(namespace.index())
        .expect("namespace exists")
        .fragments
        .push(fragment);

    let placement_owner = match owner {
        NamespaceOwner::Lexical(scope) => DeclarationOwner::Lexical(scope),
        NamespaceOwner::NamespacePublic(namespace) => DeclarationOwner::NamespacePublic(namespace),
        NamespaceOwner::FragmentPrivate(fragment) => DeclarationOwner::NamespacePrivate(fragment),
        NamespaceOwner::CompilationGlobal => DeclarationOwner::CompilationGlobal,
    };
    if let Some(participant) = push_placement(
        state,
        placement_owner,
        identifier.name.as_str(),
        declaration_id,
        MergeDeclarationKind::Namespace,
        DeclarationSpaces::NAMESPACE,
        context.ambient || declaration.declare || unit.binding.declaration_file(),
        unit,
        DeclarationSyntaxFacts::None,
    ) {
        participant.namespace_fragment = Some(fragment);
    }
    if context.member_owner().is_some() {
        let site = state
            .declarations
            .get(declaration_id)
            .expect("namespace declaration exists")
            .site;
        push_member(
            state,
            context,
            placement_owner,
            Some(declaration_id),
            Some(identifier.name.to_string()),
            site.declaration_span,
            site.binding_span,
            DeclarationSpaces::NAMESPACE,
            MergeDeclarationKind::Namespace,
            publication,
            unit,
        );
    }

    let nested_context = WalkContext {
        owner: DeclarationOwner::NamespacePrivate(fragment),
        lexical_scope: private_scope,
        namespace: Some((namespace, fragment)),
        global: None,
        deferred_module: None,
        ambient: context.ambient || declaration.declare || unit.binding.declaration_file(),
        ambient_export_list_mode: matches!(
            &declaration.body,
            Some(TSModuleDeclarationBody::TSModuleBlock(block))
                if (context.ambient || declaration.declare || unit.binding.declaration_file())
                    && block.body.iter().any(is_export_list_statement)
        ),
        active_export_context: None,
        direct_top_level: false,
    };
    match &declaration.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            reserve_member_headers(state, &block.body, nested_context);
            walk_statements(state, &block.body, nested_context, unit, compilation_global);
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => bind_module_declaration(
            state,
            nested,
            nested_context,
            true,
            true,
            unit,
            compilation_global,
        ),
        None => {}
    }
}

fn is_export_list_statement(statement: &Statement<'_>) -> bool {
    matches!(
        statement,
        Statement::ExportNamedDeclaration(export) if export.declaration.is_none()
    )
}

fn context_with_export(
    state: &mut BindState,
    context: WalkContext,
    kind: ExportContextKind,
    has_module_specifier: bool,
    span: Span,
    unit: CompilationUnit,
) -> WalkContext {
    WalkContext {
        active_export_context: push_export_context(
            state,
            context,
            kind,
            has_module_specifier,
            span,
            unit,
        ),
        ..context
    }
}

fn push_export_context(
    state: &mut BindState,
    context: WalkContext,
    kind: ExportContextKind,
    has_module_specifier: bool,
    span: Span,
    unit: CompilationUnit,
) -> Option<ExportContextId> {
    let owner = if let Some((_, fragment)) = context.namespace {
        ExportContextOwner::NamespaceFragment(fragment)
    } else if let Some(global) = context.global {
        ExportContextOwner::GlobalAugmentation(global)
    } else if let Some(module) = context.deferred_module {
        ExportContextOwner::DeferredAmbientModule(module)
    } else {
        return None;
    };
    let deferred_module_kind = match owner {
        ExportContextOwner::DeferredAmbientModule(module) => state
            .namespaces
            .deferred_modules
            .get(module.index())
            .map(|module| module.kind),
        ExportContextOwner::NamespaceFragment(_) | ExportContextOwner::GlobalAugmentation(_) => {
            None
        }
    };
    let syntax = match owner {
        ExportContextOwner::GlobalAugmentation(_) => ExportSyntaxDisposition::FutureTk2666,
        ExportContextOwner::DeferredAmbientModule(_)
            if deferred_module_kind == Some(DeferredModuleKind::ModuleAugmentation) =>
        {
            ExportSyntaxDisposition::FutureTk2666
        }
        ExportContextOwner::DeferredAmbientModule(_) => ExportSyntaxDisposition::Valid,
        ExportContextOwner::NamespaceFragment(_) => match kind {
            ExportContextKind::WrappedDeclaration => ExportSyntaxDisposition::Valid,
            ExportContextKind::NamedList if !has_module_specifier && context.ambient => {
                ExportSyntaxDisposition::Valid
            }
            ExportContextKind::NamedList | ExportContextKind::ExportAll => {
                ExportSyntaxDisposition::FutureTk1194
            }
            ExportContextKind::ExportDefault => ExportSyntaxDisposition::FutureTk1319,
            ExportContextKind::ExportAssignment => ExportSyntaxDisposition::FutureTk1063,
        },
    };
    let resolution =
        if matches!(owner, ExportContextOwner::DeferredAmbientModule(_)) || has_module_specifier {
            ExportResolutionDisposition::DeferredBacklog15
        } else {
            ExportResolutionDisposition::NotRequired
        };
    let id = ExportContextId(
        u32::try_from(state.namespaces.export_contexts.len())
            .expect("export context count fits u32"),
    );
    state.namespaces.export_contexts.push_local(ExportContext {
        id,
        owner,
        kind,
        syntax,
        resolution,
        has_module_specifier,
        source: unit.source,
        origin: unit.origin,
        span,
        members: Vec::new(),
    });
    Some(id)
}

fn reserve_member_headers(
    state: &mut BindState,
    statements: &[Statement<'_>],
    context: WalkContext,
) {
    for statement in statements {
        reserve_statement_header(state, statement, context, false);
    }
}

fn reserve_statement_header(
    state: &mut BindState,
    statement: &Statement<'_>,
    context: WalkContext,
    explicit: bool,
) {
    if let Statement::ExportNamedDeclaration(export) = statement {
        if let Some(declaration) = &export.declaration {
            reserve_declaration_header(state, declaration, context, true);
        }
        return;
    }
    let publication = context.publication(explicit);
    let owner = context.declaration_owner(publication);
    let mut reserve = |name: &str| {
        dormant_symbol_for_declaration_owner(state, owner, name);
    };
    match statement {
        Statement::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                for identifier in declarator.id.get_binding_identifiers() {
                    reserve(identifier.name.as_str());
                }
            }
        }
        Statement::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                reserve(identifier.name.as_str());
            }
        }
        Statement::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                reserve(identifier.name.as_str());
            }
        }
        Statement::TSTypeAliasDeclaration(alias) => reserve(alias.id.name.as_str()),
        Statement::TSInterfaceDeclaration(interface) => reserve(interface.id.name.as_str()),
        Statement::TSEnumDeclaration(enumeration) => reserve(enumeration.id.name.as_str()),
        Statement::TSModuleDeclaration(namespace) => {
            if let TSModuleDeclarationName::Identifier(identifier) = &namespace.id {
                reserve(identifier.name.as_str());
            }
        }
        Statement::TSImportEqualsDeclaration(import) => reserve(import.id.name.as_str()),
        Statement::ImportDeclaration(import) => {
            if let Some(specifiers) = &import.specifiers {
                for specifier in specifiers {
                    reserve(specifier.local().name.as_str());
                }
            }
        }
        _ => {}
    }
}

fn reserve_declaration_header(
    state: &mut BindState,
    declaration: &Declaration<'_>,
    context: WalkContext,
    explicit: bool,
) {
    let publication = context.publication(explicit);
    let owner = context.declaration_owner(publication);
    let mut reserve = |name: &str| {
        dormant_symbol_for_declaration_owner(state, owner, name);
    };
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                for identifier in declarator.id.get_binding_identifiers() {
                    reserve(identifier.name.as_str());
                }
            }
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                reserve(identifier.name.as_str());
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                reserve(identifier.name.as_str());
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => reserve(alias.id.name.as_str()),
        Declaration::TSInterfaceDeclaration(interface) => reserve(interface.id.name.as_str()),
        Declaration::TSEnumDeclaration(enumeration) => reserve(enumeration.id.name.as_str()),
        Declaration::TSModuleDeclaration(namespace) => {
            if let TSModuleDeclarationName::Identifier(identifier) = &namespace.id {
                reserve(identifier.name.as_str());
            }
        }
        Declaration::TSImportEqualsDeclaration(import) => reserve(import.id.name.as_str()),
        Declaration::TSGlobalDeclaration(_) => {}
    }
}

fn namespace_for(state: &mut BindState, owner: NamespaceOwner, name: &str) -> NamespaceId {
    let key = NamespaceKey {
        owner,
        name: name.to_string(),
    };
    if let Some(namespace) = state.namespaces.namespace_keys.get(&key) {
        return *namespace;
    }
    let public_scope = state
        .graph
        .push(Scope::new(ScopeKind::NamespacePublic, None));
    let symbol = dormant_symbol_for_namespace_owner(state, owner, name);
    let id = NamespaceId(
        u32::try_from(state.namespaces.namespaces.len()).expect("namespace count fits u32"),
    );
    state.namespaces.namespaces.push_local(Namespace {
        id,
        owner,
        name: name.to_string(),
        public_scope,
        symbol,
        fragments: Vec::new(),
    });
    state
        .namespaces
        .aggregate_instance_states
        .push_local(NamespaceInstanceState::NonInstantiated);
    state.namespaces.standalone_value_storages.push_local(None);
    let _ = state.namespaces.namespace_keys.insert_local(key, id);
    id
}

fn attach_namespace_symbol(state: &mut BindState, namespace: NamespaceId, declaration: DeclId) {
    let symbol_id = state
        .namespaces
        .get(namespace)
        .expect("namespace exists")
        .symbol;
    if let Some(symbol) = state.symbols.get_mut(symbol_id) {
        symbol.ns = Some(namespace);
    }
    state.attach_symbol_declaration(symbol_id, declaration);
}

fn dormant_symbol_for_namespace_owner(
    state: &mut BindState,
    owner: NamespaceOwner,
    name: &str,
) -> SymbolId {
    match owner {
        NamespaceOwner::Lexical(scope) => dormant_symbol_in_scope(state, scope, name),
        NamespaceOwner::NamespacePublic(namespace) => {
            let scope = state
                .namespaces
                .get(namespace)
                .expect("parent namespace exists")
                .public_scope;
            dormant_symbol_in_scope(state, scope, name)
        }
        NamespaceOwner::FragmentPrivate(fragment) => {
            let scope = state
                .namespaces
                .fragment(fragment)
                .expect("parent namespace fragment exists")
                .private_scope;
            dormant_symbol_in_scope(state, scope, name)
        }
        NamespaceOwner::CompilationGlobal => {
            let scope = state
                .namespaces
                .compilation_global
                .expect("compilation-global scope allocated");
            dormant_symbol_in_scope(state, scope, name)
        }
    }
}

fn dormant_symbol_for_declaration_owner(
    state: &mut BindState,
    owner: DeclarationOwner,
    name: &str,
) -> Option<SymbolId> {
    match owner {
        DeclarationOwner::Lexical(scope) => state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name)),
        DeclarationOwner::NamespacePublic(namespace) => {
            let scope = state.namespaces.get(namespace)?.public_scope;
            Some(dormant_symbol_in_scope(state, scope, name))
        }
        DeclarationOwner::NamespacePrivate(fragment) => {
            let scope = state.namespaces.fragment(fragment)?.private_scope;
            Some(dormant_symbol_in_scope(state, scope, name))
        }
        DeclarationOwner::CompilationGlobal => {
            let scope = state.namespaces.compilation_global?;
            Some(dormant_symbol_in_scope(state, scope, name))
        }
        DeclarationOwner::DeferredAmbientModule(_) => None,
    }
}

fn dormant_symbol_in_scope(state: &mut BindState, scope: ScopeId, name: &str) -> SymbolId {
    if let Some(symbol) = state
        .graph
        .get(scope)
        .and_then(|scope| scope.lookup_local(name))
    {
        return symbol;
    }
    let symbol = state.symbols.push(Symbol::new(name));
    state.graph.declare(scope, name, symbol);
    symbol
}

fn bind_deferred_module(
    state: &mut BindState,
    declaration: &TSModuleDeclaration<'_>,
    context: WalkContext,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    let TSModuleDeclarationName::StringLiteral(specifier) = &declaration.id else {
        return;
    };
    let declaration_id = state.attach_declaration_scope(
        specifier.span.start,
        DeclarationKind::Namespace,
        context.lexical_scope,
    );
    let id = DeferredModuleId(
        u32::try_from(state.namespaces.deferred_modules.len())
            .expect("deferred module count fits u32"),
    );
    state
        .namespaces
        .deferred_modules
        .push_local(DeferredAmbientModule {
            id,
            declaration: declaration_id,
            source: unit.source,
            origin: unit.origin,
            module: state.current_module,
            owner: context.owner,
            kind: if unit.binding.external_module {
                DeferredModuleKind::ModuleAugmentation
            } else {
                DeferredModuleKind::AmbientExternalModule
            },
            specifier: specifier.value.to_string(),
            span: Span::from_oxc(declaration.span),
        });
    let deferred_context = WalkContext {
        owner: DeclarationOwner::DeferredAmbientModule(id),
        lexical_scope: context.lexical_scope,
        namespace: None,
        global: None,
        deferred_module: Some(id),
        ambient: true,
        ambient_export_list_mode: false,
        active_export_context: None,
        direct_top_level: false,
    };
    if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &declaration.body {
        for statement in &block.body {
            match statement {
                Statement::TSGlobalDeclaration(_) | Statement::TSNamespaceExportDeclaration(_) => {
                    walk_statement(
                        state,
                        statement,
                        deferred_context,
                        false,
                        unit,
                        compilation_global,
                    )
                }
                _ => record_deferred_statement(state, id, statement, deferred_context, unit),
            }
        }
    }
}

fn record_deferred_statement(
    state: &mut BindState,
    module: DeferredModuleId,
    statement: &Statement<'_>,
    context: WalkContext,
    unit: CompilationUnit,
) {
    if let Statement::ExportNamedDeclaration(export) = statement {
        push_export_context(
            state,
            context,
            if export.declaration.is_some() {
                ExportContextKind::WrappedDeclaration
            } else {
                ExportContextKind::NamedList
            },
            export.source.is_some(),
            Span::from_oxc(export.span),
            unit,
        );
        if let Some(declaration) = &export.declaration {
            record_deferred_declaration(state, module, declaration, true, unit);
        } else {
            state
                .namespaces
                .deferred_children
                .push_local(DeferredAmbientChild {
                    module,
                    declaration: None,
                    kind: DeferredChildKind::DeferredExport,
                    name: None,
                    span: Span::from_oxc(export.span),
                    binding_span: None,
                    source: unit.source,
                    origin: unit.origin,
                });
        }
        return;
    }
    match statement {
        Statement::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                for identifier in declarator.id.get_binding_identifiers() {
                    record_deferred_binding(
                        state,
                        module,
                        identifier.span.start,
                        DeclarationKind::Variable,
                        DeferredChildKind::OrdinaryDeclaration,
                        identifier.name.as_str(),
                        unit,
                    );
                }
            }
        }
        Statement::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Function,
                    DeferredChildKind::OrdinaryDeclaration,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Statement::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Class,
                    DeferredChildKind::OrdinaryDeclaration,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Statement::TSTypeAliasDeclaration(alias) => record_deferred_binding(
            state,
            module,
            alias.id.span.start,
            DeclarationKind::TypeAlias,
            DeferredChildKind::OrdinaryDeclaration,
            alias.id.name.as_str(),
            unit,
        ),
        Statement::TSInterfaceDeclaration(interface) => record_deferred_binding(
            state,
            module,
            interface.id.span.start,
            DeclarationKind::Interface,
            DeferredChildKind::OrdinaryDeclaration,
            interface.id.name.as_str(),
            unit,
        ),
        Statement::TSEnumDeclaration(enumeration) => record_deferred_binding(
            state,
            module,
            enumeration.id.span.start,
            DeclarationKind::Enum,
            DeferredChildKind::OrdinaryDeclaration,
            enumeration.id.name.as_str(),
            unit,
        ),
        Statement::TSModuleDeclaration(namespace) => {
            if let TSModuleDeclarationName::Identifier(identifier) = &namespace.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Namespace,
                    DeferredChildKind::NamespaceDeclaration,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Statement::TSImportEqualsDeclaration(import) => record_deferred_binding(
            state,
            module,
            import.id.span.start,
            DeclarationKind::ImportEquals,
            DeferredChildKind::OrdinaryDeclaration,
            import.id.name.as_str(),
            unit,
        ),
        Statement::ImportDeclaration(import) => {
            if let Some(specifiers) = &import.specifiers {
                for specifier in specifiers {
                    let local = specifier.local();
                    record_deferred_binding(
                        state,
                        module,
                        local.span.start,
                        DeclarationKind::Import,
                        DeferredChildKind::OrdinaryDeclaration,
                        local.name.as_str(),
                        unit,
                    );
                }
            }
        }
        Statement::ExportAllDeclaration(export) => {
            push_export_context(
                state,
                context,
                ExportContextKind::ExportAll,
                true,
                Span::from_oxc(export.span),
                unit,
            );
            record_deferred_export(state, module, Span::from_oxc(export.span), unit)
        }
        Statement::ExportDefaultDeclaration(export) => {
            push_export_context(
                state,
                context,
                ExportContextKind::ExportDefault,
                false,
                Span::from_oxc(export.span),
                unit,
            );
            record_deferred_export(state, module, Span::from_oxc(export.span), unit)
        }
        Statement::TSExportAssignment(export) => {
            push_export_context(
                state,
                context,
                ExportContextKind::ExportAssignment,
                false,
                Span::from_oxc(export.span),
                unit,
            );
            record_deferred_export(state, module, Span::from_oxc(export.span), unit)
        }
        _ => {}
    }
}

fn record_deferred_declaration(
    state: &mut BindState,
    module: DeferredModuleId,
    declaration: &Declaration<'_>,
    exported: bool,
    unit: CompilationUnit,
) {
    let kind = if exported {
        DeferredChildKind::ExportDeclaration
    } else {
        DeferredChildKind::OrdinaryDeclaration
    };
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            for declarator in &declaration.declarations {
                for identifier in declarator.id.get_binding_identifiers() {
                    record_deferred_binding(
                        state,
                        module,
                        identifier.span.start,
                        DeclarationKind::Variable,
                        kind,
                        identifier.name.as_str(),
                        unit,
                    );
                }
            }
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(identifier) = &function.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Function,
                    kind,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(identifier) = &class.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Class,
                    kind,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => record_deferred_binding(
            state,
            module,
            alias.id.span.start,
            DeclarationKind::TypeAlias,
            kind,
            alias.id.name.as_str(),
            unit,
        ),
        Declaration::TSInterfaceDeclaration(interface) => record_deferred_binding(
            state,
            module,
            interface.id.span.start,
            DeclarationKind::Interface,
            kind,
            interface.id.name.as_str(),
            unit,
        ),
        Declaration::TSEnumDeclaration(enumeration) => record_deferred_binding(
            state,
            module,
            enumeration.id.span.start,
            DeclarationKind::Enum,
            kind,
            enumeration.id.name.as_str(),
            unit,
        ),
        Declaration::TSModuleDeclaration(namespace) => {
            if let TSModuleDeclarationName::Identifier(identifier) = &namespace.id {
                record_deferred_binding(
                    state,
                    module,
                    identifier.span.start,
                    DeclarationKind::Namespace,
                    DeferredChildKind::NamespaceDeclaration,
                    identifier.name.as_str(),
                    unit,
                );
            }
        }
        Declaration::TSImportEqualsDeclaration(import) => record_deferred_binding(
            state,
            module,
            import.id.span.start,
            DeclarationKind::ImportEquals,
            kind,
            import.id.name.as_str(),
            unit,
        ),
        Declaration::TSGlobalDeclaration(_) => {}
    }
}

fn record_deferred_binding(
    state: &mut BindState,
    module: DeferredModuleId,
    binding_start: u32,
    declaration_kind: DeclarationKind,
    child_kind: DeferredChildKind,
    name: &str,
    unit: CompilationUnit,
) {
    let declaration = state.source_decl_at(binding_start, declaration_kind);
    let (span, binding_span) = declaration
        .and_then(|declaration| state.declarations.get(declaration))
        .map(|declaration| {
            (
                declaration.site.declaration_span,
                Some(declaration.site.binding_span),
            )
        })
        .unwrap_or((Span::new(binding_start, binding_start), None));
    state
        .namespaces
        .deferred_children
        .push_local(DeferredAmbientChild {
            module,
            declaration,
            kind: child_kind,
            name: Some(MetadataName::Identifier(name.to_string())),
            span,
            binding_span,
            source: unit.source,
            origin: unit.origin,
        });
}

fn record_deferred_export(
    state: &mut BindState,
    module: DeferredModuleId,
    span: Span,
    unit: CompilationUnit,
) {
    state
        .namespaces
        .deferred_children
        .push_local(DeferredAmbientChild {
            module,
            declaration: None,
            kind: DeferredChildKind::DeferredExport,
            name: None,
            span,
            binding_span: None,
            source: unit.source,
            origin: unit.origin,
        });
}

fn bind_global(
    state: &mut BindState,
    declaration: &oxc_ast::ast::TSGlobalDeclaration<'_>,
    context: WalkContext,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    let declaration_id = if context.deferred_module.is_some() {
        state
            .source_decl_at(declaration.global_span.start, DeclarationKind::Global)
            .expect("global source declaration exists")
    } else {
        state.attach_declaration_scope(
            declaration.global_span.start,
            DeclarationKind::Global,
            context.lexical_scope,
        )
    };
    let owner = if let Some(module) = context.deferred_module {
        GlobalOwner::DeferredAmbientModule(module)
    } else if let Some((_, fragment)) = context.namespace {
        GlobalOwner::NamespaceFragment(fragment)
    } else {
        GlobalOwner::Lexical(context.lexical_scope)
    };
    let placement = if context.deferred_module.is_some() {
        GlobalPlacement::DeferredAmbientModule
    } else if context.direct_top_level && unit.binding.external_module {
        GlobalPlacement::DirectExternalModule
    } else if context.direct_top_level {
        GlobalPlacement::DirectScript
    } else {
        GlobalPlacement::NestedNamespace
    };
    let in_module_augmentation = context.deferred_module.is_some_and(|module| {
        state
            .namespaces
            .deferred_modules
            .get(module.index())
            .is_some_and(|module| module.kind == DeferredModuleKind::ModuleAugmentation)
    });
    let mut issues = Vec::new();
    if matches!(
        placement,
        GlobalPlacement::DirectScript | GlobalPlacement::NestedNamespace
    ) || in_module_augmentation
    {
        issues.push(GlobalIssue::FutureTk2669);
    }
    if !declaration.declare && !context.ambient {
        issues.push(GlobalIssue::FutureTk2670);
    }
    let overlay_scope = state.graph.push(Scope::new(
        ScopeKind::GlobalOverlay,
        Some(context.lexical_scope),
    ));
    let legal = issues.is_empty();
    let id = GlobalAugmentationId(
        u32::try_from(state.namespaces.globals.len()).expect("global count fits u32"),
    );
    state.namespaces.globals.push_local(GlobalAugmentation {
        id,
        declaration: declaration_id,
        source: unit.source,
        origin: unit.origin,
        module: state.current_module,
        owner,
        body_span: Span::from_oxc(declaration.body.span),
        diagnostic_span: Span::from_oxc(declaration.global_span),
        target_scope: if legal {
            compilation_global
        } else {
            overlay_scope
        },
        overlay_scope,
        placement,
        issues,
        declared: declaration.declare,
        members: Vec::new(),
    });
    let global_lexical_scope = if legal && state.namespaces.uses_library_shared_globals() {
        compilation_global
    } else {
        overlay_scope
    };
    let global_body = WalkContext {
        owner: if legal {
            DeclarationOwner::CompilationGlobal
        } else {
            DeclarationOwner::Lexical(overlay_scope)
        },
        lexical_scope: global_lexical_scope,
        namespace: None,
        global: Some(id),
        deferred_module: None,
        ambient: true,
        ambient_export_list_mode: declaration.body.body.iter().any(is_export_list_statement),
        active_export_context: None,
        direct_top_level: false,
    };
    reserve_member_headers(state, &declaration.body.body, global_body);
    walk_statements(
        state,
        &declaration.body.body,
        global_body,
        unit,
        compilation_global,
    );
}

#[allow(clippy::too_many_arguments)]
/// Places `declaration` under `owner`/`name` and returns its row, so a caller that has
/// more to record reaches it through this one merge-key lookup instead of hunting for
/// the row it just wrote across every placement in the project.
fn push_placement<'state>(
    state: &'state mut BindState,
    owner: DeclarationOwner,
    name: &str,
    declaration: DeclId,
    kind: MergeDeclarationKind,
    spaces: DeclarationSpaces,
    ambient: bool,
    unit: CompilationUnit,
    syntax: DeclarationSyntaxFacts,
) -> Option<&'state mut MergeParticipant> {
    let span = state
        .declarations
        .get(declaration)
        .expect("placement declaration exists")
        .site
        .declaration_span;
    let binding_span = state
        .declarations
        .get(declaration)
        .expect("placement declaration exists")
        .site
        .binding_span;
    let participant = MergeParticipant {
        declaration,
        kind,
        source: unit.source,
        origin: unit.origin,
        span,
        binding_span,
        ambient,
        spaces,
        syntax,
        namespace_fragment: None,
        namespace_instance: None,
    };
    let Ok(entries) = state.namespaces.placements.get_or_insert_local_with(
        MergeKey {
            owner,
            name: name.to_string(),
        },
        Vec::new,
    ) else {
        return None;
    };
    // A merge group holds one row per declaration sharing the name, never the project.
    let index = match entries.iter().position(|entry| {
        #[cfg(test)]
        record_placement_row_probe();
        entry.declaration == declaration
    }) {
        Some(index) => index,
        None => {
            entries.push(participant);
            entries.len() - 1
        }
    };
    let row = entries.get_mut(index)?;
    row.syntax = syntax;
    // Mirror the row's facts so the by-declaration read is a lookup, not a scan. Only a
    // row this build actually placed contributes, so a sealed base still answers `None`.
    if syntax == DeclarationSyntaxFacts::None {
        state.placement_syntax.remove(&declaration);
    } else {
        state.placement_syntax.insert(declaration, syntax);
    }
    Some(row)
}

fn placement_syntax(state: &BindState, declaration: DeclId) -> Option<DeclarationSyntaxFacts> {
    let syntax = state.placement_syntax.get(&declaration).copied();
    #[cfg(test)]
    if syntax.is_some() {
        record_placement_row_probe();
    }
    syntax
}

#[allow(clippy::too_many_arguments)]
fn push_member(
    state: &mut BindState,
    context: WalkContext,
    target: DeclarationOwner,
    declaration: Option<DeclId>,
    name: Option<String>,
    declaration_span: Span,
    binding_span: Span,
    spaces: DeclarationSpaces,
    kind: MergeDeclarationKind,
    publication: NamespacePublication,
    unit: CompilationUnit,
) {
    let Some(owner) = context.member_owner() else {
        return;
    };
    let id = NamespaceMemberId(
        u32::try_from(state.namespaces.members.len()).expect("namespace member count fits u32"),
    );
    let symbol = name
        .as_deref()
        .and_then(|name| dormant_symbol_for_declaration_owner(state, target, name));
    if let (Some(symbol), Some(declaration)) = (symbol, declaration) {
        state.attach_symbol_declaration(symbol, declaration);
    }
    let local_name = name.clone().map(MetadataName::Identifier);
    let exported_name = (!matches!(publication, NamespacePublication::Private))
        .then(|| name.clone().map(MetadataName::Identifier))
        .flatten();
    let exported_span = exported_name.as_ref().map(|_| binding_span);
    let syntax = declaration
        .and_then(|declaration| placement_syntax(state, declaration))
        .unwrap_or(DeclarationSyntaxFacts::None);
    state.namespaces.members.push_local(NamespaceMember {
        id,
        owner,
        target,
        declaration,
        symbol,
        local_symbol: symbol,
        name,
        local_name,
        exported_name,
        declaration_span,
        specifier_span: None,
        binding_span,
        local_span: Some(binding_span),
        exported_span,
        source: unit.source,
        origin: unit.origin,
        module_specifier: None,
        outer_type_only: false,
        specifier_type_only: false,
        alias_context: None,
        alias_resolution: None,
        alias_space_intent: None,
        export_context: context.active_export_context,
        syntax,
        spaces,
        kind,
        publication,
    });
    match owner {
        NamespaceMemberOwner::Fragment(fragment) => state
            .namespaces
            .fragments
            .get_mut_local(fragment.index())
            .expect("namespace fragment exists")
            .members
            .push(id),
        NamespaceMemberOwner::GlobalAugmentation(global) => state
            .namespaces
            .globals
            .get_mut_local(global.index())
            .expect("global augmentation exists")
            .members
            .push(id),
        NamespaceMemberOwner::DeferredAmbientModule(_) => {}
    }
    attach_export_member(state, context.active_export_context, id);
}

fn attach_export_member(
    state: &mut BindState,
    context: Option<ExportContextId>,
    member: NamespaceMemberId,
) {
    let Some(context) = context else {
        return;
    };
    if let Some(context) = state
        .namespaces
        .export_contexts
        .get_mut_local(context.index())
    {
        if !context.members.contains(&member) {
            context.members.push(member);
        }
    }
}

fn push_deferred_export_member(
    state: &mut BindState,
    context: WalkContext,
    span: Span,
    unit: CompilationUnit,
) {
    push_member(
        state,
        context,
        context.declaration_owner(NamespacePublication::Explicit),
        None,
        None,
        span,
        span,
        DeclarationSpaces::ALIAS,
        MergeDeclarationKind::DeferredExport,
        NamespacePublication::Explicit,
        unit,
    );
}

fn push_export_alias_member(
    state: &mut BindState,
    context: WalkContext,
    export: &oxc_ast::ast::ExportNamedDeclaration<'_>,
    specifier: &oxc_ast::ast::ExportSpecifier<'_>,
    unit: CompilationUnit,
) {
    let Some(owner) = context.member_owner() else {
        return;
    };
    let local_name = metadata_name(&specifier.local);
    let exported_name = metadata_name(&specifier.exported);
    let local_owner = context.declaration_owner(NamespacePublication::Private);
    let local_symbol = lookup_dormant_symbol(state, local_owner, local_name.text()).or_else(|| {
        context.namespace.and_then(|(namespace, _)| {
            lookup_dormant_symbol(
                state,
                DeclarationOwner::NamespacePublic(namespace),
                local_name.text(),
            )
        })
    });
    let Some(export_context) = context
        .active_export_context
        .and_then(|id| state.namespaces.export_contexts.get(id.index()))
    else {
        return;
    };
    let alias_context = match export_context.syntax {
        ExportSyntaxDisposition::Valid => AliasContext::ValidAmbient,
        ExportSyntaxDisposition::FutureTk1194 => AliasContext::InvalidFutureTk1194,
        ExportSyntaxDisposition::FutureTk2666 => AliasContext::InvalidAugmentationFutureTk2666,
        ExportSyntaxDisposition::FutureTk1319 | ExportSyntaxDisposition::FutureTk1063 => {
            unreachable!("named export aliases cannot have default or assignment syntax")
        }
    };
    let alias_resolution = export_context.resolution;
    let (target, publication, symbol) = if alias_context == AliasContext::ValidAmbient {
        let target = context.declaration_owner(NamespacePublication::Explicit);
        let symbol = dormant_symbol_for_declaration_owner(state, target, exported_name.text());
        (target, NamespacePublication::Explicit, symbol)
    } else {
        (local_owner, NamespacePublication::Private, None)
    };
    let outer_type_only = export.export_kind == ImportOrExportKind::Type;
    let specifier_type_only = specifier.export_kind == ImportOrExportKind::Type;
    let spaces = if outer_type_only || specifier_type_only {
        DeclarationSpaces::TYPE
    } else {
        DeclarationSpaces::NONE
    };
    let alias_space_intent = if outer_type_only || specifier_type_only {
        AliasSpaceIntent::Type
    } else {
        AliasSpaceIntent::UnresolvedValueOrType
    };
    let id = NamespaceMemberId(
        u32::try_from(state.namespaces.members.len()).expect("namespace member count fits u32"),
    );
    state.namespaces.members.push_local(NamespaceMember {
        id,
        owner,
        target,
        declaration: None,
        symbol,
        local_symbol,
        name: Some(exported_name.text().to_string()),
        local_name: Some(local_name),
        exported_name: Some(exported_name),
        declaration_span: Span::from_oxc(export.span),
        specifier_span: Some(Span::from_oxc(specifier.span)),
        binding_span: Span::from_oxc(specifier.exported.span()),
        local_span: Some(Span::from_oxc(specifier.local.span())),
        exported_span: Some(Span::from_oxc(specifier.exported.span())),
        source: unit.source,
        origin: unit.origin,
        module_specifier: export
            .source
            .as_ref()
            .map(|source| MetadataName::StringLiteral(source.value.to_string())),
        outer_type_only,
        specifier_type_only,
        alias_context: Some(alias_context),
        alias_resolution: Some(alias_resolution),
        alias_space_intent: Some(alias_space_intent),
        export_context: context.active_export_context,
        syntax: DeclarationSyntaxFacts::None,
        spaces,
        kind: MergeDeclarationKind::DeferredExport,
        publication,
    });
    match owner {
        NamespaceMemberOwner::Fragment(fragment) => {
            if let Some(fragment) = state.namespaces.fragments.get_mut_local(fragment.index()) {
                fragment.members.push(id);
            }
        }
        NamespaceMemberOwner::GlobalAugmentation(global) => {
            if let Some(global) = state.namespaces.globals.get_mut_local(global.index()) {
                global.members.push(id);
            }
        }
        NamespaceMemberOwner::DeferredAmbientModule(_) => {}
    }
    attach_export_member(state, context.active_export_context, id);
}

fn lookup_dormant_symbol(
    state: &BindState,
    owner: DeclarationOwner,
    name: &str,
) -> Option<SymbolId> {
    let scope = match owner {
        DeclarationOwner::Lexical(scope) => scope,
        DeclarationOwner::NamespacePublic(namespace) => {
            state.namespaces.get(namespace)?.public_scope
        }
        DeclarationOwner::NamespacePrivate(fragment) => {
            state.namespaces.fragment(fragment)?.private_scope
        }
        DeclarationOwner::CompilationGlobal => state.namespaces.compilation_global?,
        DeclarationOwner::DeferredAmbientModule(_) => return None,
    };
    state.graph.get(scope)?.lookup_local(name)
}

fn metadata_name(name: &ModuleExportName<'_>) -> MetadataName {
    match name {
        ModuleExportName::IdentifierName(identifier) => {
            MetadataName::Identifier(identifier.name.to_string())
        }
        ModuleExportName::IdentifierReference(identifier) => {
            MetadataName::Identifier(identifier.name.to_string())
        }
        ModuleExportName::StringLiteral(literal) => {
            MetadataName::StringLiteral(literal.value.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::bind::{Binder, ImportedSymbol, ProjectBinderBuilder};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

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
        let source = include_str!("namespace.rs");
        let fields = source
            .split_once("pub struct NamespaceTable {")
            .and_then(|(_, rest)| rest.split_once("\n}"))
            .map(|(body, _)| body)
            .expect("NamespaceTable declaration");
        let layered_fields = fields
            .lines()
            .filter(|line| line.contains("LayeredVec<") || line.contains("LayeredMap<"))
            .filter_map(|line| line.trim().split_once(':').map(|(name, _)| name))
            .collect::<Vec<_>>();
        assert_eq!(
            layered_fields.len(),
            31,
            "layered namespace field inventory"
        );

        let witness = source
            .split_once("pub(crate) fn shares_base_storage_with(&self, other: &Self) -> bool {")
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
                .flat_map(|(_, participants)| participants)
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

    /// The three by-declaration lookups this guard covers all resolve through
    /// `push_placement` now. Only the one-shot instance-state pass may still walk every
    /// placement row, so a reintroduced per-declaration scan cannot slip past the counter.
    #[test]
    fn placement_rows_are_scanned_only_by_the_one_shot_instance_state_pass() {
        // The escaped needle never matches itself, so the split isolates production code.
        let production = include_str!("namespace.rs")
            .split("#[cfg(test)]\nmod tests {")
            .next()
            .expect("production region precedes the test module");
        assert_eq!(
            production
                .matches("self.placements.local_values_mut()")
                .count(),
            1,
            "the only surviving full pass is the instance-state back-patch"
        );
        assert_eq!(
            production.matches("placements.local_values_mut()").count(),
            1,
            "no caller may reintroduce a per-declaration scan over every placement"
        );
        assert_eq!(
            production.matches("placements.local_iter()").count(),
            0,
            "the by-declaration read resolves through the placement index"
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
            FINALIZATION_PLACEMENTS_PER_GROUP
                * u64::try_from(GROUPS).expect("group count fits u64"),
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
        // `src/binder/` is the only place a call site can live.
        // The escaped needle never matches itself, so the split isolates production code.
        let namespace_production = include_str!("namespace.rs")
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
        let bind_production = include_str!("bind.rs")
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
                .filter(|record| record.name == "Shared")
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
                    .filter(|record| record.name == name)
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

    fn bind_snapshot_validation_library() -> Binder {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let source = Parser::new(
            &source_allocator,
            r#"
                export {};
                export as namespace FirstUmd;
                export as namespace SecondUmd;
                declare global { interface FirstGlobal {} }
                declare global { interface SecondGlobal {} }
                declare namespace FirstNamespace {
                    export default function first(): void;
                }
                declare namespace SecondNamespace {
                    export default function second(): void;
                }
            "#,
            SourceType::d_ts(),
        )
        .parse();
        assert!(prelude.diagnostics.is_empty());
        assert!(!source.panicked);
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let unit = CompilationUnit::library(
            SourceUnitKey(1),
            LibraryFileOrdinal::new(0),
            &source.program,
        );
        let module = builder.add_library_modules(&[(&source.program, unit)])[0];
        builder.finish(module)
    }

    fn assert_snapshot_corruption(
        primary: &NamespaceSnapshotPrimary,
        corrupt: impl FnOnce(&mut NamespaceSnapshotPrimary),
        expected: &'static str,
    ) {
        let mut corrupt_primary = primary.clone();
        corrupt(&mut corrupt_primary);
        let actual = NamespaceTable::from_snapshot_primary(corrupt_primary)
            .err()
            .expect("corrupt snapshot must be rejected");
        assert_eq!(actual, expected);
    }

    #[test]
    fn snapshot_decode_rejects_duplicate_fragment_derived_keys() {
        let primary = bind_snapshot_validation_library()
            .namespaces
            .snapshot_primary();
        assert!(primary.fragments.len() >= 2);
        assert!(NamespaceTable::from_snapshot_primary(primary.clone()).is_ok());

        assert_snapshot_corruption(
            &primary,
            |corrupt| corrupt.fragments[1].declaration = corrupt.fragments[0].declaration,
            "namespace fragment declaration index contains a duplicate",
        );
        assert_snapshot_corruption(
            &primary,
            |corrupt| {
                corrupt.fragments[1].module = corrupt.fragments[0].module;
                corrupt.fragments[1].source_start = corrupt.fragments[0].source_start;
            },
            "namespace fragment site index contains a duplicate",
        );
    }

    #[test]
    fn snapshot_decode_rejects_duplicate_statement_and_reporting_keys() {
        let primary = bind_snapshot_validation_library()
            .namespaces
            .snapshot_primary();
        assert!(primary.globals.len() >= 2);
        assert!(primary.umd_exports.len() >= 2);
        assert!(primary.export_contexts.len() >= 2);
        assert!(NamespaceTable::from_snapshot_primary(primary.clone()).is_ok());

        assert_snapshot_corruption(
            &primary,
            |corrupt| {
                corrupt.globals[1].module = corrupt.globals[0].module;
                corrupt.globals[1].diagnostic_span.start = corrupt.globals[0].diagnostic_span.start;
            },
            "global augmentation site index contains a duplicate",
        );
        assert_snapshot_corruption(
            &primary,
            |corrupt| {
                corrupt.umd_exports[1].module = corrupt.umd_exports[0].module;
                corrupt.umd_exports[1].span.start = corrupt.umd_exports[0].span.start;
            },
            "UMD export site index contains a duplicate",
        );
        assert_snapshot_corruption(
            &primary,
            |corrupt| corrupt.source_units.push(corrupt.source_units[0].clone()),
            "source-module index contains a duplicate",
        );
        assert_snapshot_corruption(
            &primary,
            |corrupt| {
                corrupt.export_contexts[1].source = corrupt.export_contexts[0].source;
                corrupt.export_contexts[1].span.start = corrupt.export_contexts[0].span.start;
            },
            "library export-default reporting index contains a duplicate",
        );
        assert_snapshot_corruption(
            &primary,
            |corrupt| corrupt.export_contexts[1].owner = corrupt.export_contexts[0].owner,
            "library module-reporting index contains a duplicate",
        );
    }

    fn merge<'a>(binder: &'a Binder, name: &str) -> &'a MergeRecord {
        binder
            .namespaces
            .merges
            .iter()
            .find(|record| record.name == name)
            .expect("merge record")
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
            MergeDisposition::DeferredBacklog42
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
            .expect("enum participant remains owned by backlog 42");
        let enum_declaration = binder
            .declarations
            .get(enum_participant.declaration)
            .expect("enum declaration identity");
        assert_eq!(enum_declaration.kind, DeclarationKind::Enum);
        assert_eq!(enum_declaration.site.module, binder.module);
        assert_eq!(enum_declaration.value_storage, None);
        assert_eq!(enum_declaration.type_group, None);
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
            NamespaceValueAttachmentDisposition::Rejected(MergeDisposition::DeferredBacklog42)
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
            for (original_module, ((_, source), parsed)) in sources.iter().zip(&parsed).enumerate()
            {
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
        assert_eq!(compilation_global.lookup_local("Blocked"), None);
        let blocked_global = binder
            .namespaces
            .globals()
            .find(|global| global.source == SourceUnitKey(60))
            .expect("blocked global augmentation");
        let blocker = binder
            .graph
            .get(blocked_global.overlay_scope)
            .and_then(|scope| scope.lookup_local("Blocked"))
            .and_then(|symbol| binder.symbols.get(symbol))
            .expect("global blocker");
        assert!(
            blocker.blocks_value_lookup
                && blocker.blocks_type_lookup
                && blocker.blocks_namespace_lookup
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
                &["DeferredRoot", "Enumeration"][..],
                QualifiedTypePathResolution::Deferred {
                    segment: 1,
                    reason: QualifiedTypePathDeferredReason::Enum,
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
    fn ambient_export_alias_outputs_are_not_local_bindings_but_namespace_aliases_keep_type_groups()
    {
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
                    Statement::TSImportEqualsDeclaration(import)
                        if import.id.name == import_name =>
                    {
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
            let unit =
                CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
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
            assert!(matches!(
                binder.resolve_qualified_type_path(binder.module, &[name, "X"]),
                QualifiedTypePathResolution::TypeGroup(_)
            ));
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
    fn rich_namespace_and_global_bodies_inventory_exact_declarations_without_storage() {
        let source = "interface Existing {} namespace Existing { export const { a, nested: { b } } = value; export function f(param: number): void {} export class C {} export type T = number; export interface I {} export enum E {} export namespace Child {} export { a as alias }; } declare global { const [g]: number[]; function gf(arg: number): void; class GC {} type GT = number; interface GI {} enum GE {} namespace GN {} }";
        let binder = bind(source, false);
        assert_eq!(binder.decl_count, 4);
        assert_eq!(binder.type_groups.len(), 4);

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
            if matches!(name, "f" | "C" | "param") {
                assert!(declaration.value_storage.is_some(), "{name}");
            } else {
                assert_eq!(declaration.value_storage, None, "{name}");
            }
            if matches!(name, "C" | "T" | "I") {
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
        for name in ["E", "Chimera"] {
            assert_eq!(
                disposition(name),
                MergeDisposition::DeferredBacklog42,
                "{name}"
            );
        }
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
    fn legal_global_overlays_publish_only_complete_type_side_names_before_module_link() {
        let source = r#"
export {};
interface Captured { local: boolean }
namespace DeferredRoot { export interface DeferredLeaf { moduleOnly: number } }
declare global {
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
            "UsesCaptured",
            "GlobalAlias",
            "TypeSpace",
            "UsesDeferredRoot",
        ] {
            assert_eq!(script_root.lookup_local(name), None, "{name}");
            let canonical = compilation_global
                .lookup_local(name)
                .unwrap_or_else(|| panic!("published global {name}"));
            assert_eq!(overlay.lookup_local(name), Some(canonical));
        }
        for name in ["DeferredPair", "ValueSpace", "DeferredRoot"] {
            assert_eq!(compilation_global.lookup_local(name), None, "{name}");
            let blocker = overlay
                .lookup_local(name)
                .and_then(|symbol| binder.symbols.get(symbol))
                .unwrap_or_else(|| panic!("blocked global {name}"));
            assert!(
                blocker.blocks_type_lookup
                    && blocker.blocks_value_lookup
                    && blocker.blocks_namespace_lookup
            );
            assert!(blocker.ty.is_none() && blocker.value.is_none() && blocker.ns.is_none());
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
        assert_eq!(
            binder.resolve_qualified_type_path(
                global.overlay_scope,
                &["DeferredRoot", "DeferredLeaf"]
            ),
            QualifiedTypePathResolution::Unavailable { segment: 0 }
        );

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
}

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
use crate::span::Span;
use oxc_ast::ast::{
    Declaration, ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName, Program,
    Statement, TSModuleDeclaration, TSModuleDeclarationBody, TSModuleDeclarationName,
    TSModuleReference, VariableDeclarationKind,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

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
pub struct NamespaceMemberId(pub u32);

impl NamespaceMemberId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("namespace member id fits usize")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GlobalAugmentationId(pub u32);

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
pub struct ExportContextId(pub u32);

impl ExportContextId {
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("export context id fits usize")
    }
}

/// Run-stable project source ordering key. Project mode derives it from normalized paths.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SourceUnitKey(pub u32);

impl SourceUnitKey {
    pub const PRELUDE: Self = Self(0);
    pub const SINGLE_SOURCE: Self = Self(1);
}

/// Binder-layer copy of original input ownership, separate from dependency slots and checker events.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct OriginalModuleOrdinal(pub u32);

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
pub struct CompilationUnit {
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
    pub binding: ModuleBindingContext,
}

impl CompilationUnit {
    pub fn implementation(source: SourceUnitKey, program: &Program<'_>) -> Self {
        Self {
            source,
            original_module: OriginalModuleOrdinal(0),
            binding: ModuleBindingContext::for_program(program, SourceFileKind::ImplementationTs),
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
pub struct NamespaceFragment {
    pub id: NamespaceFragmentId,
    pub namespace: NamespaceId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
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
pub enum NamespaceMemberOwner {
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
    pub original_module: OriginalModuleOrdinal,
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
pub struct NamespaceMember {
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
    pub original_module: OriginalModuleOrdinal,
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
pub struct GlobalAugmentation {
    pub id: GlobalAugmentationId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
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
pub struct DeferredAmbientModule {
    pub id: DeferredModuleId,
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
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
pub struct DeferredAmbientChild {
    pub module: DeferredModuleId,
    pub declaration: Option<DeclId>,
    pub kind: DeferredChildKind,
    pub name: Option<MetadataName>,
    pub span: Span,
    pub binding_span: Option<Span>,
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ExportContextOwner {
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
pub struct ExportContext {
    pub id: ExportContextId,
    pub owner: ExportContextOwner,
    pub kind: ExportContextKind,
    pub syntax: ExportSyntaxDisposition,
    pub resolution: ExportResolutionDisposition,
    pub has_module_specifier: bool,
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
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
pub struct UmdNamespaceExport {
    pub declaration: DeclId,
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
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
pub struct PlacementIssue {
    pub kind: PlacementIssueKind,
    pub owner: DeclId,
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
    pub span: Span,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MergeParticipant {
    pub declaration: DeclId,
    pub kind: MergeDeclarationKind,
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
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
    pub declarations: Vec<MergeParticipant>,
    pub classification: MergeClassification,
    pub placement_issues: Vec<PlacementIssue>,
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
    pub(crate) scope: ScopeId,
    pub(crate) site: DeclarationSite,
    pub(crate) value_storage: Option<ValueStorageId>,
    pub(crate) symbol: Option<SymbolId>,
    pub(crate) kind: MergeDeclarationKind,
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

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceUnitRecord {
    pub source: SourceUnitKey,
    pub original_module: OriginalModuleOrdinal,
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
    namespaces: Vec<Namespace>,
    aggregate_instance_states: Vec<NamespaceInstanceState>,
    standalone_value_storages: Vec<Option<ValueStorageId>>,
    fragments: Vec<NamespaceFragment>,
    members: Vec<NamespaceMember>,
    namespace_keys: FxHashMap<NamespaceKey, NamespaceId>,
    canonical_namespaces: Vec<NamespaceId>,
    placements: FxHashMap<MergeKey, Vec<MergeParticipant>>,
    merges: Vec<MergeRecord>,
    globals: Vec<GlobalAugmentation>,
    deferred_modules: Vec<DeferredAmbientModule>,
    deferred_children: Vec<DeferredAmbientChild>,
    umd_exports: Vec<UmdNamespaceExport>,
    export_contexts: Vec<ExportContext>,
    source_units: Vec<SourceUnitRecord>,
    canonical_source_units: Vec<usize>,
    canonical_globals: Vec<GlobalAugmentationId>,
    canonical_deferred_modules: Vec<DeferredModuleId>,
    canonical_deferred_children: Vec<usize>,
    canonical_umd_exports: Vec<usize>,
    canonical_export_contexts: Vec<ExportContextId>,
    compilation_global: Option<ScopeId>,
}

impl NamespaceTable {
    pub fn get(&self, id: NamespaceId) -> Option<&Namespace> {
        self.namespaces.get(id.index())
    }

    pub fn fragment(&self, id: NamespaceFragmentId) -> Option<&NamespaceFragment> {
        self.fragments.get(id.index())
    }

    pub fn member(&self, id: NamespaceMemberId) -> Option<&NamespaceMember> {
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

    #[cfg(test)]
    fn members(&self) -> impl Iterator<Item = &NamespaceMember> {
        self.members.iter()
    }

    pub fn merges(&self) -> impl Iterator<Item = &MergeRecord> {
        self.merges.iter()
    }

    /// Exact source-ordered placement outcomes ready for checker emission.
    pub(crate) fn placement_issues(&self) -> impl Iterator<Item = &PlacementIssue> {
        let mut issues = self
            .merges
            .iter()
            .flat_map(|record| record.placement_issues.iter())
            .collect::<Vec<_>>();
        issues.sort_by_key(|issue| {
            (
                issue.source,
                issue.original_module,
                issue.span.start,
                issue.owner.0,
            )
        });
        issues.into_iter()
    }

    pub fn source_units(&self) -> impl Iterator<Item = &SourceUnitRecord> {
        self.canonical_source_units
            .iter()
            .filter_map(|index| self.source_units.get(*index))
    }

    pub fn globals(&self) -> impl Iterator<Item = &GlobalAugmentation> {
        self.canonical_globals
            .iter()
            .filter_map(|id| self.globals.get(id.index()))
    }

    /// Freeze legal global publication, then connect every user module in one cutover.
    pub(crate) fn finalize_global_scopes(&self, graph: &mut ScopeGraph, symbols: &mut SymbolTable) {
        let compilation_global = self
            .compilation_global
            .expect("compilation-global scope allocated");
        let mut unsafe_names = rustc_hash::FxHashSet::default();
        for record in self
            .merges
            .iter()
            .filter(|record| record.owner == DeclarationOwner::CompilationGlobal)
        {
            let safe = record
                .declarations
                .iter()
                .all(|participant| match participant.kind {
                    MergeDeclarationKind::Interface | MergeDeclarationKind::TypeAlias => true,
                    MergeDeclarationKind::Namespace => {
                        participant.namespace_instance
                            == Some(NamespaceInstanceState::NonInstantiated)
                    }
                    _ => false,
                });
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
                if global.symbols.remove(&name).is_some() {
                    blocked_names.push(name);
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
            .map(|name| {
                let mut symbol = Symbol::new(name.clone());
                symbol.blocks_type_lookup = true;
                symbol.blocks_value_lookup = true;
                symbol.blocks_namespace_lookup = true;
                (name, symbols.push(symbol))
            })
            .collect::<Vec<_>>();

        for global in self
            .globals
            .iter()
            .filter(|global| global.issues.is_empty())
        {
            for (name, symbol) in &safe_symbols {
                let replaced = graph.declare(global.overlay_scope, name.clone(), *symbol);
                assert!(
                    replaced.is_none(),
                    "global overlay is populated only at freeze"
                );
            }
            for (name, symbol) in &blocked_symbols {
                let replaced = graph.declare(global.overlay_scope, name.clone(), *symbol);
                assert!(
                    replaced.is_none(),
                    "global overlay blockers are frozen once"
                );
            }
        }

        for unit in &self.source_units {
            let module = graph
                .get_mut(unit.module)
                .expect("user module scope exists");
            assert_eq!(module.kind, ScopeKind::Module);
            module.parent = Some(compilation_global);
        }
    }

    pub fn deferred_modules(&self) -> impl Iterator<Item = &DeferredAmbientModule> {
        self.canonical_deferred_modules
            .iter()
            .filter_map(|id| self.deferred_modules.get(id.index()))
    }

    pub fn deferred_children(&self) -> impl Iterator<Item = &DeferredAmbientChild> {
        self.canonical_deferred_children
            .iter()
            .filter_map(|index| self.deferred_children.get(*index))
    }

    pub fn umd_exports(&self) -> impl Iterator<Item = &UmdNamespaceExport> {
        self.canonical_umd_exports
            .iter()
            .filter_map(|index| self.umd_exports.get(*index))
    }

    pub fn export_contexts(&self) -> impl Iterator<Item = &ExportContext> {
        self.canonical_export_contexts
            .iter()
            .filter_map(|id| self.export_contexts.get(id.index()))
    }

    pub fn len(&self) -> usize {
        self.namespaces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.namespaces.is_empty()
    }

    fn classify(&mut self) {
        self.compute_namespace_instance_states();
        for namespace in &mut self.namespaces {
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
        self.canonical_namespaces = (0..self.namespaces.len())
            .map(|index| NamespaceId(u32::try_from(index).expect("namespace count fits u32")))
            .collect();
        self.canonical_namespaces.sort_by_key(|id| {
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
        self.merges = self
            .placements
            .iter()
            .map(|(key, participants)| {
                let mut declarations = participants.clone();
                declarations.sort_by_key(|participant| {
                    (
                        participant.source,
                        participant.span.start,
                        participant.declaration.0,
                    )
                });
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
            .collect();
        self.merges.sort_by(|left, right| {
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
        self.canonical_globals = (0..self.globals.len())
            .map(|index| GlobalAugmentationId(u32::try_from(index).expect("global count fits u32")))
            .collect();
        self.canonical_globals.sort_by_key(|id| {
            let global = &self.globals[id.index()];
            (
                global.source,
                global.diagnostic_span.start,
                global.original_module,
            )
        });
        self.canonical_deferred_modules = (0..self.deferred_modules.len())
            .map(|index| DeferredModuleId(u32::try_from(index).expect("module count fits u32")))
            .collect();
        self.canonical_deferred_modules.sort_by_key(|id| {
            let module = &self.deferred_modules[id.index()];
            (module.source, module.span.start, module.original_module)
        });
        self.canonical_source_units = (0..self.source_units.len()).collect();
        self.canonical_source_units.sort_by_key(|index| {
            let unit = &self.source_units[*index];
            (unit.source, unit.original_module)
        });
        self.canonical_deferred_children = (0..self.deferred_children.len()).collect();
        self.canonical_deferred_children.sort_by_key(|index| {
            let child = &self.deferred_children[*index];
            (child.source, child.span.start, child.original_module)
        });
        self.canonical_umd_exports = (0..self.umd_exports.len()).collect();
        self.canonical_umd_exports.sort_by_key(|index| {
            let export = &self.umd_exports[*index];
            (export.source, export.span.start, export.original_module)
        });
        self.canonical_export_contexts = (0..self.export_contexts.len())
            .map(|index| {
                ExportContextId(u32::try_from(index).expect("export context count fits u32"))
            })
            .collect();
        self.canonical_export_contexts.sort_by_key(|id| {
            let context = &self.export_contexts[id.index()];
            (context.source, context.span.start, context.original_module)
        });
    }

    fn compute_namespace_instance_states(&mut self) {
        let mut states = vec![NamespaceInstanceState::NonInstantiated; self.fragments.len()];
        for fragment in &self.fragments {
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
                states[fragment.id.index()] =
                    join_instance_state(states[fragment.id.index()], direct);
            }
        }

        loop {
            let mut changed = false;
            for fragment in &self.fragments {
                let mut state = states[fragment.id.index()];
                for member in fragment
                    .members
                    .iter()
                    .filter_map(|member| self.members.get(member.index()))
                    .filter(|member| member.kind == MergeDeclarationKind::Namespace)
                {
                    let child = member.declaration.and_then(|declaration| {
                        self.fragments
                            .iter()
                            .find(|candidate| candidate.declaration == declaration)
                    });
                    if let Some(child) = child {
                        state = join_instance_state(state, states[child.id.index()]);
                    }
                }
                if state != states[fragment.id.index()] {
                    states[fragment.id.index()] = state;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for fragment in &mut self.fragments {
            fragment.instance_state = states[fragment.id.index()];
        }
        self.aggregate_instance_states.resize(
            self.namespaces.len(),
            NamespaceInstanceState::NonInstantiated,
        );
        self.aggregate_instance_states
            .fill(NamespaceInstanceState::NonInstantiated);
        for fragment in &self.fragments {
            let aggregate = &mut self.aggregate_instance_states[fragment.namespace.index()];
            *aggregate = join_instance_state(*aggregate, fragment.instance_state);
        }
        for participants in self.placements.values_mut() {
            for participant in participants {
                participant.namespace_instance = participant
                    .namespace_fragment
                    .map(|fragment| states[fragment.index()]);
            }
        }
    }

    fn dormant_standalone_value_storage_candidates(&self) -> Vec<NamespaceId> {
        self.canonical_namespaces
            .iter()
            .copied()
            .filter(|namespace| {
                self.aggregate_instance_state(*namespace)
                    == Some(NamespaceInstanceState::Instantiated)
                    && self.standalone_value_storage(*namespace).is_none()
                    && !self.has_compilation_global_ancestor(*namespace)
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
        self.merges
            .iter()
            .find(|record| record.owner == owner && record.name == namespace.name)
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
            .get_mut(namespace.index())
            .expect("namespace storage side column is dense");
        assert!(
            slot.replace(storage).is_none(),
            "namespace storage is stable"
        );
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
    pub(crate) fn global_augmentation_scope(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> Option<ScopeId> {
        self.namespaces
            .globals
            .iter()
            .find(|global| global.module == module && global.diagnostic_span.start == binding_start)
            .map(|global| global.overlay_scope)
    }

    pub(crate) fn global_augmentation_requires_incomplete(
        &self,
        module: ScopeId,
        binding_start: u32,
    ) -> bool {
        let Some(global) = self.namespaces.globals.iter().find(|global| {
            global.module == module && global.diagnostic_span.start == binding_start
        }) else {
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
                            self.namespaces
                                .merges
                                .iter()
                                .flat_map(|record| &record.declarations)
                                .find(|participant| participant.declaration == declaration)
                        })
                        .and_then(|participant| participant.namespace_fragment)
                        .and_then(|fragment| self.namespaces.fragment(fragment));
                    fragment.is_none_or(|fragment| {
                        fragment.instance_state == NamespaceInstanceState::Instantiated
                    })
                }
                _ => true,
            }
        })
    }

    pub(crate) fn umd_export_requires_incomplete(&self, module: ScopeId, span_start: u32) -> bool {
        self.namespaces.umd_exports.iter().any(|export| {
            export.module == module
                && export.span.start == span_start
                && export.context == UmdContext::DeferredValidBacklog15
        })
    }

    /// Return the frozen namespace-side input for one lexical value owner.
    /// Only admitted owners and the exact backlog-42 callable recovery expose members.
    pub(crate) fn namespace_value_attachment(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<NamespaceValueAttachment<'_>> {
        let record = self.namespaces.merges.iter().find(|record| {
            record.name == name && self.declaration_owner_scope(record.owner) == Some(scope)
        })?;
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
                    scope: lexical.site.scope?,
                    site: lexical.site,
                    value_storage: lexical.value_storage,
                    symbol: member.symbol,
                    kind: member.kind,
                })
            })
            .collect::<Vec<_>>();
        members.sort_by_key(|member| {
            (
                member.source,
                member.site.declaration_span.start,
                member.declaration.0,
            )
        });
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
            .iter()
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
                    original_module: member.original_module,
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
        self.namespaces.namespaces.iter().find_map(|namespace| {
            if namespace.public_scope == scope {
                return Some(namespace.id);
            }
            namespace.fragments.iter().find_map(|fragment| {
                self.namespaces
                    .fragments
                    .get(fragment.index())
                    .filter(|fragment| fragment.private_scope == scope)
                    .map(|_| namespace.id)
            })
        })
    }

    fn declaration_owner_scope(&self, owner: DeclarationOwner) -> Option<ScopeId> {
        match owner {
            DeclarationOwner::Lexical(scope) => Some(scope),
            DeclarationOwner::NamespacePublic(namespace) => self
                .namespaces
                .get(namespace)
                .map(|namespace| namespace.public_scope),
            DeclarationOwner::NamespacePrivate(fragment) => self
                .namespaces
                .fragment(fragment)
                .map(|fragment| fragment.private_scope),
            DeclarationOwner::CompilationGlobal => Some(self.compilation_global),
            DeclarationOwner::DeferredAmbientModule(_) => None,
        }
    }

    fn root_merge_record(&self, scope: ScopeId, name: &str) -> Option<&MergeRecord> {
        self.namespaces
            .merges
            .iter()
            .find(|record| record.owner == DeclarationOwner::Lexical(scope) && record.name == name)
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
            original_module: item.original_module,
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

/// Bind namespace topology after ordinary declarations, then activate only value
/// members that can attach to an admitted class/function draft.
pub(crate) fn bind_namespace_metadata(
    state: &mut BindState,
    module: ScopeId,
    program: &Program<'_>,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    state.namespaces.source_units.push(SourceUnitRecord {
        source: unit.source,
        original_module: unit.original_module,
        module,
        context: unit.binding,
    });
    state.namespaces.compilation_global = Some(compilation_global);
    let context = WalkContext {
        owner: DeclarationOwner::Lexical(module),
        lexical_scope: module,
        namespace: None,
        global: None,
        deferred_module: None,
        ambient: unit.binding.declaration_file(),
        ambient_export_list_mode: false,
        active_export_context: None,
        direct_top_level: true,
    };
    walk_statements(state, &program.body, context, unit, compilation_global);
    resolve_local_ambient_export_alias_targets(state);
    state.namespaces.classify();
    bind_namespace_value_attachment_members(state, program);
}

#[derive(Clone)]
struct NamespaceValueBindingTarget {
    member: NamespaceMemberId,
    declaration: DeclId,
    name: String,
    scope: ScopeId,
    kind: MergeDeclarationKind,
    public_symbol: Option<SymbolId>,
}

fn bind_namespace_value_attachment_members(state: &mut BindState, program: &Program<'_>) {
    let mut targets = Vec::new();
    for record in &state.namespaces.merges {
        if !matches!(
            namespace_value_attachment_disposition(record),
            Some(
                NamespaceValueAttachmentDisposition::AdmittedFunction
                    | NamespaceValueAttachmentDisposition::AdmittedClass
                    | NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42
            )
        ) || matches!(
            record.owner,
            DeclarationOwner::CompilationGlobal | DeclarationOwner::DeferredAmbientModule(_)
        ) {
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
                let Some(scope) = state
                    .declarations
                    .get(declaration)
                    .and_then(|declaration| declaration.site.scope)
                else {
                    continue;
                };
                targets.push(NamespaceValueBindingTarget {
                    member: member.id,
                    declaration,
                    name: name.clone(),
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
    let scopes = targets
        .iter()
        .map(|target| (target.declaration, target.scope))
        .collect::<FxHashMap<_, _>>();
    bind_selected_namespace_value_statements(state, &program.body, &scopes);

    for target in targets {
        let storage = state
            .declarations
            .get(target.declaration)
            .and_then(|declaration| declaration.value_storage);
        let local_symbol = state
            .graph
            .get(target.scope)
            .and_then(|scope| scope.lookup_local(&target.name));
        if let Some(member) = state.namespaces.members.get_mut(target.member.index()) {
            member.local_symbol = local_symbol;
        }
        let Some(symbol) = target.public_symbol else {
            continue;
        };
        let Some(storage) = storage else {
            continue;
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
    let candidates = state
        .namespaces
        .members
        .iter()
        .enumerate()
        .filter(|(_, member)| {
            matches!(member.owner, NamespaceMemberOwner::Fragment(_))
                && member.kind == MergeDeclarationKind::DeferredExport
                && member.declaration.is_none()
                && member.alias_context == Some(AliasContext::ValidAmbient)
                && member.module_specifier.is_none()
        })
        .filter_map(|(index, member)| {
            let NamespaceMemberOwner::Fragment(fragment) = member.owner else {
                return None;
            };
            Some((
                index,
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
        state.namespaces.members[index].local_symbol = local_symbol;
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
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit.source)
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
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit.source)
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
            push_deferred_export_member(state, context, Span::from_oxc(export.span), unit.source)
        }
        Statement::VariableDeclaration(declaration) => {
            bind_variable(state, declaration, context, explicit, unit.source)
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
                    unit.source,
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
                    unit.source,
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
            unit.source,
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
            unit.source,
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
            unit.source,
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
                unit.source,
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
                        unit.source,
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
            state.namespaces.umd_exports.push(UmdNamespaceExport {
                declaration,
                source: unit.source,
                original_module: unit.original_module,
                module: state.current_module,
                owner: context.owner,
                name: export.id.name.to_string(),
                span: Span::from_oxc(export.span),
                context: umd_context,
            });
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
            bind_variable(state, declaration, context, explicit, unit.source)
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
                    unit.source,
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
                    unit.source,
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
            unit.source,
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
            unit.source,
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
            unit.source,
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
                unit.source,
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
    source: SourceUnitKey,
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
                source,
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
    source: SourceUnitKey,
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
        source,
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
    source: SourceUnitKey,
    syntax: DeclarationSyntaxFacts,
) {
    let declaration =
        state.attach_declaration_scope(binding_start, declaration_kind, context.lexical_scope);
    let publication = context.publication(explicit);
    let owner = context.declaration_owner(publication);
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
        source,
    );
    set_placement_syntax(state, declaration, syntax);
    let legal_global_type = context.global.is_some()
        && owner == DeclarationOwner::CompilationGlobal
        && matches!(
            merge_kind,
            MergeDeclarationKind::TypeAlias | MergeDeclarationKind::Interface
        );
    if context.namespace.is_some() || legal_global_type {
        let fragment_kind = match merge_kind {
            MergeDeclarationKind::TypeAlias => Some(TypeFragmentKind::TypeAlias),
            MergeDeclarationKind::Interface => Some(TypeFragmentKind::Interface),
            MergeDeclarationKind::Class => Some(TypeFragmentKind::Class),
            _ => None,
        };
        if let (Some(fragment_kind), Some(scope)) =
            (fragment_kind, declaration_owner_scope(state, owner))
        {
            declare_type(state, scope, name, declaration, fragment_kind, source);
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
            source,
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
    let Some(owner) = context.namespace_owner(publication) else {
        return;
    };
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
    state.namespaces.fragments.push(NamespaceFragment {
        id: fragment,
        namespace,
        declaration: declaration_id,
        source: unit.source,
        original_module: unit.original_module,
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
        .get_mut(namespace.index())
        .expect("namespace exists")
        .fragments
        .push(fragment);

    let placement_owner = match owner {
        NamespaceOwner::Lexical(scope) => DeclarationOwner::Lexical(scope),
        NamespaceOwner::NamespacePublic(namespace) => DeclarationOwner::NamespacePublic(namespace),
        NamespaceOwner::FragmentPrivate(fragment) => DeclarationOwner::NamespacePrivate(fragment),
        NamespaceOwner::CompilationGlobal => DeclarationOwner::CompilationGlobal,
    };
    push_placement(
        state,
        placement_owner,
        identifier.name.as_str(),
        declaration_id,
        MergeDeclarationKind::Namespace,
        DeclarationSpaces::NAMESPACE,
        context.ambient || declaration.declare || unit.binding.declaration_file(),
        unit.source,
    );
    for participants in state.namespaces.placements.values_mut() {
        if let Some(participant) = participants
            .iter_mut()
            .find(|participant| participant.declaration == declaration_id)
        {
            participant.namespace_fragment = Some(fragment);
            break;
        }
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
            unit.source,
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
    state.namespaces.export_contexts.push(ExportContext {
        id,
        owner,
        kind,
        syntax,
        resolution,
        has_module_specifier,
        source: unit.source,
        original_module: unit.original_module,
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
    state.namespaces.namespaces.push(Namespace {
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
        .push(NamespaceInstanceState::NonInstantiated);
    state.namespaces.standalone_value_storages.push(None);
    state.namespaces.namespace_keys.insert(key, id);
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
        .push(DeferredAmbientModule {
            id,
            declaration: declaration_id,
            source: unit.source,
            original_module: unit.original_module,
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
                .push(DeferredAmbientChild {
                    module,
                    declaration: None,
                    kind: DeferredChildKind::DeferredExport,
                    name: None,
                    span: Span::from_oxc(export.span),
                    binding_span: None,
                    source: unit.source,
                    original_module: unit.original_module,
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
        .push(DeferredAmbientChild {
            module,
            declaration,
            kind: child_kind,
            name: Some(MetadataName::Identifier(name.to_string())),
            span,
            binding_span,
            source: unit.source,
            original_module: unit.original_module,
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
        .push(DeferredAmbientChild {
            module,
            declaration: None,
            kind: DeferredChildKind::DeferredExport,
            name: None,
            span,
            binding_span: None,
            source: unit.source,
            original_module: unit.original_module,
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
    state.namespaces.globals.push(GlobalAugmentation {
        id,
        declaration: declaration_id,
        source: unit.source,
        original_module: unit.original_module,
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
    let global_body = WalkContext {
        owner: if legal {
            DeclarationOwner::CompilationGlobal
        } else {
            DeclarationOwner::Lexical(overlay_scope)
        },
        lexical_scope: overlay_scope,
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
fn push_placement(
    state: &mut BindState,
    owner: DeclarationOwner,
    name: &str,
    declaration: DeclId,
    kind: MergeDeclarationKind,
    spaces: DeclarationSpaces,
    ambient: bool,
    source: SourceUnitKey,
) {
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
        source,
        original_module: original_module_for_source(state, source),
        span,
        binding_span,
        ambient,
        spaces,
        syntax: DeclarationSyntaxFacts::None,
        namespace_fragment: None,
        namespace_instance: None,
    };
    let entries = state
        .namespaces
        .placements
        .entry(MergeKey {
            owner,
            name: name.to_string(),
        })
        .or_default();
    if !entries.iter().any(|entry| entry.declaration == declaration) {
        entries.push(participant);
    }
}

fn set_placement_syntax(
    state: &mut BindState,
    declaration: DeclId,
    syntax: DeclarationSyntaxFacts,
) {
    for participants in state.namespaces.placements.values_mut() {
        if let Some(participant) = participants
            .iter_mut()
            .find(|participant| participant.declaration == declaration)
        {
            participant.syntax = syntax;
            return;
        }
    }
}

fn placement_syntax(state: &BindState, declaration: DeclId) -> Option<DeclarationSyntaxFacts> {
    state
        .namespaces
        .placements
        .values()
        .flatten()
        .find(|participant| participant.declaration == declaration)
        .map(|participant| participant.syntax)
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
    source: SourceUnitKey,
) {
    let Some(owner) = context.member_owner() else {
        return;
    };
    let id = NamespaceMemberId(
        u32::try_from(state.namespaces.members.len()).expect("namespace member count fits u32"),
    );
    let original_module = original_module_for_source(state, source);
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
    state.namespaces.members.push(NamespaceMember {
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
        source,
        original_module,
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
            .get_mut(fragment.index())
            .expect("namespace fragment exists")
            .members
            .push(id),
        NamespaceMemberOwner::GlobalAugmentation(global) => state
            .namespaces
            .globals
            .get_mut(global.index())
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
    let members = &mut state.namespaces.export_contexts[context.index()].members;
    if !members.contains(&member) {
        members.push(member);
    }
}

fn original_module_for_source(state: &BindState, source: SourceUnitKey) -> OriginalModuleOrdinal {
    state
        .namespaces
        .source_units
        .iter()
        .rev()
        .find(|unit| unit.source == source)
        .map(|unit| unit.original_module)
        .unwrap_or(OriginalModuleOrdinal(0))
}

fn push_deferred_export_member(
    state: &mut BindState,
    context: WalkContext,
    span: Span,
    source: SourceUnitKey,
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
        source,
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
    state.namespaces.members.push(NamespaceMember {
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
        original_module: unit.original_module,
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
        NamespaceMemberOwner::Fragment(fragment) => state.namespaces.fragments[fragment.index()]
            .members
            .push(id),
        NamespaceMemberOwner::GlobalAugmentation(global) => {
            state.namespaces.globals[global.index()].members.push(id)
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

    fn bind(source: &str, declaration_file: bool) -> Binder {
        bind_unit(
            source,
            declaration_file,
            SourceUnitKey::SINGLE_SOURCE,
            OriginalModuleOrdinal(0),
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
            original_module,
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
            let attachment = binder
                .namespace_value_attachment(binder.module, name)
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
            assert_eq!(dormant.value_storage, None, "{name}");
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
        let binder = bind_unit(source, false, SourceUnitKey(17), OriginalModuleOrdinal(5));
        let issues = binder.namespaces.placement_issues().collect::<Vec<_>>();
        assert_eq!(issues.len(), 3);
        assert!(issues
            .iter()
            .all(|issue| issue.kind == PlacementIssueKind::FutureTk2434));
        assert!(issues.iter().all(|issue| {
            issue.source == SourceUnitKey(17)
                && issue.original_module == OriginalModuleOrdinal(5)
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
                assert_eq!(scope.symbols.len(), 1);
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
                .get(binder.module)
                .and_then(|scope| scope.lookup_local("N")),
            Some(namespace.symbol)
        );
        let root_symbol = binder
            .symbols
            .get(namespace.symbol)
            .expect("root namespace symbol");
        assert_eq!(root_symbol.ns, Some(namespace.id));
        assert_eq!(root_symbol.value, None);
        assert_eq!(root_symbol.ty, None);
        assert_eq!(root_symbol.declarations.len(), 3);
        assert_eq!(binder.resolve_value(binder.module, "N"), None);
        assert_eq!(binder.resolve_type(binder.module, "N"), None);

        assert_eq!(binder.decl_count, 1);
        assert_eq!(
            binder.namespaces.aggregate_instance_state(namespace.id),
            Some(NamespaceInstanceState::Instantiated)
        );
        assert_eq!(
            binder.namespaces.standalone_value_storage(namespace.id),
            Some(ValueStorageId(0))
        );
        assert!(binder.type_groups.is_empty());
        for name in ["publicOne", "privateOne", "privateTwo", "privateThree"] {
            let declaration = binder
                .declarations
                .iter()
                .find(|declaration| &source[declaration.site.binding_span.range()] == name)
                .expect("body declaration");
            assert_eq!(declaration.value_storage, None);
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
        assert!(global.symbols.is_empty());
        assert_eq!(
            binder.graph.var_scope(binder.compilation_global),
            Some(binder.compilation_global)
        );
        assert_eq!(
            binder
                .graph
                .get(binder.module)
                .and_then(|scope| scope.parent),
            Some(binder.compilation_global)
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
                None,
                "{name} root symbol stays dormant"
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
                    original_module: OriginalModuleOrdinal(
                        u32::try_from(original_module).expect("two modules fit u32"),
                    ),
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
                    original_module: OriginalModuleOrdinal(
                        u32::try_from(original_module).expect("two modules fit u32"),
                    ),
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
        assert!(binder.graph.get(binder.module).is_some_and(|scope| {
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
        assert_eq!(binder.decl_count, 1);
        assert_eq!(binder.type_groups.len(), 4);

        let existing_symbol = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("Existing"))
            .and_then(|symbol| binder.symbols.get(symbol))
            .expect("interface plus namespace symbol");
        assert!(existing_symbol.ty.is_some());
        assert!(existing_symbol.ns.is_some());
        assert_eq!(existing_symbol.value, None);
        assert_eq!(existing_symbol.declarations.len(), 2);
        assert!(existing_symbol.declarations.windows(2).all(|pair| binder
            .declarations
            .get(pair[0])
            .zip(binder.declarations.get(pair[1]))
            .is_some_and(|(left, right)| left.site.declaration_span.start
                < right.site.declaration_span.start)));

        for declaration in binder.declarations.iter().filter(|declaration| {
            declaration.kind != DeclarationKind::Interface
                || &source[declaration.site.binding_span.range()] != "Existing"
        }) {
            assert_eq!(declaration.value_storage, None);
            let name = &source[declaration.site.binding_span.range()];
            if matches!(name, "C" | "T" | "I") {
                assert!(declaration.type_group.is_some());
            } else {
                assert_eq!(declaration.type_group, None);
            }
            if matches!(name, "param" | "arg") {
                assert_eq!(declaration.site.scope, None);
            } else {
                assert!(declaration.site.scope.is_some());
            }
        }
        let namespace = existing_symbol
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
            assert_eq!(declaration.site.scope, None);
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
        assert_eq!(module.parent, Some(binder.compilation_global));
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

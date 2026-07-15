//! Dormant namespace, merge, global-augmentation, and UMD context metadata.
//!
//! WU1b deliberately inventories source topology without binding namespace/global
//! bodies into production scopes or allocating checker storage.

use crate::binder::bind::BindState;
use crate::binder::declaration::{DeclId, DeclarationKind};
use crate::binder::scope::{Scope, ScopeId, ScopeKind};
use crate::binder::symbol::{Symbol, SymbolId};
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
        matches!(
            self,
            Self::ImplementationMts
                | Self::ImplementationCts
                | Self::DeclarationMts
                | Self::DeclarationCts
        )
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

/// All WU1b data. No production resolver or checker reads this table.
#[derive(Default)]
pub struct NamespaceTable {
    namespaces: Vec<Namespace>,
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
        for participants in self.placements.values_mut() {
            for participant in participants {
                participant.namespace_instance = participant
                    .namespace_fragment
                    .map(|fragment| states[fragment.index()]);
            }
        }
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
    let first_value = declarations.iter().find(|item| {
        matches!(
            item.kind,
            MergeDeclarationKind::Function | MergeDeclarationKind::Class
        ) && !item.ambient
    });
    let Some(first_value) = first_value else {
        return Vec::new();
    };
    declarations
        .iter()
        .filter(|item| {
            item.kind == MergeDeclarationKind::Namespace
                && !item.ambient
                && item.namespace_instance == Some(NamespaceInstanceState::Instantiated)
                && (item.source, item.span.start) < (first_value.source, first_value.span.start)
        })
        .map(|item| PlacementIssue {
            kind: PlacementIssueKind::FutureTk2434,
            owner: item.declaration,
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

/// Run the WU1b metadata inventory after ordinary binding and prove storage dormancy.
pub(crate) fn bind_namespace_metadata(
    state: &mut BindState,
    module: ScopeId,
    program: &Program<'_>,
    unit: CompilationUnit,
    compilation_global: ScopeId,
) {
    let storage_snapshot = (
        state.next_value_storage,
        state.next_legacy_type_storage,
        state.type_groups.len(),
    );
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
    state.namespaces.classify();
    assert_eq!(
        storage_snapshot,
        (
            state.next_value_storage,
            state.next_legacy_type_storage,
            state.type_groups.len(),
        ),
        "WU1b metadata must not allocate checker storage or type groups"
    );
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
    let private_scope = state.graph.push(Scope::new(
        ScopeKind::NamespacePrivate,
        Some(context.lexical_scope),
    ));
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
        NamespaceOwner::Lexical(scope) => state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name))
            .unwrap_or_else(|| state.symbols.push(Symbol::new(name))),
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
    if !declaration.declare
        && !unit.binding.declaration_file()
        && placement != GlobalPlacement::DeferredAmbientModule
    {
        issues.push(GlobalIssue::FutureTk2670);
    }
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
        target_scope: compilation_global,
        placement,
        issues,
        declared: declaration.declare,
        members: Vec::new(),
    });
    let global_body = WalkContext {
        owner: DeclarationOwner::CompilationGlobal,
        lexical_scope: compilation_global,
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
    use crate::binder::bind::{Binder, ProjectBinderBuilder};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn bind(source: &str, declaration_file: bool) -> Binder {
        let prelude_allocator = Allocator::default();
        let source_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let parsed = Parser::new(&source_allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "parse failed: {source}");
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let unit = CompilationUnit {
            source: SourceUnitKey::SINGLE_SOURCE,
            original_module: OriginalModuleOrdinal(0),
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
            None
        );
        let root_symbol = binder
            .symbols
            .get(namespace.symbol)
            .expect("root namespace symbol");
        assert_eq!(root_symbol.ns, Some(namespace.id));
        assert_eq!(root_symbol.value, None);
        assert_eq!(root_symbol.ty, None);
        assert_eq!(root_symbol.type_group, None);
        assert_eq!(root_symbol.declarations.len(), 3);
        assert_eq!(binder.resolve_value(binder.module, "N"), None);
        assert_eq!(binder.resolve_type(binder.module, "N"), None);

        assert_eq!(binder.decl_count, 0);
        assert_eq!(binder.type_decl_count, 0);
        assert!(binder.type_groups.is_empty());
        for name in ["publicOne", "privateOne", "privateTwo", "privateThree"] {
            let declaration = binder
                .declarations
                .iter()
                .find(|declaration| &source[declaration.site.binding_span.range()] == name)
                .expect("body declaration");
            assert_eq!(declaration.value_storage, None);
            assert_eq!(declaration.legacy_type_storage, None);
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
            .expect("one dormant global scope");
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
            Some(binder.prelude_module)
        );
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
            ["A", "B", "C"]
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
        assert!(context("", SourceFileKind::DeclarationMts).external_module);
        assert!(context("", SourceFileKind::DeclarationCts).external_module);

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
        assert_eq!(binder.decl_count, 0);
        assert_eq!(binder.type_decl_count, 1);
        assert_eq!(binder.type_groups.len(), 1);

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
            assert_eq!(declaration.legacy_type_storage, None);
            assert_eq!(declaration.type_group, None);
            let name = &source[declaration.site.binding_span.range()];
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
            assert_eq!(declaration.site.scope, Some(binder.compilation_global));
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
        let script = bind("global { interface ScriptGlobal {} }", false);
        let script_global = script.namespaces.globals().next().expect("script global");
        assert_eq!(script_global.placement, GlobalPlacement::DirectScript);
        assert_eq!(
            script_global.issues,
            vec![GlobalIssue::FutureTk2669, GlobalIssue::FutureTk2670]
        );
        assert!(script
            .graph
            .get(script.compilation_global)
            .is_some_and(|scope| scope.lookup_local("ScriptGlobal").is_some()));
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

//! Parser and local-project frontend shared by the checker and driver.

use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::source::{CompilationOrigin, ModuleOrdinal, OriginalModuleOrdinal, UnitSlot};
use crate::span::Span;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, ImportDeclarationSpecifier,
    ImportOrExportKind, ModuleExportName, Program, Statement, TSModuleDeclarationName,
    TSModuleReference,
};
use oxc_parser::Parser;
use oxc_resolver::{ResolveError, Resolver};
use oxc_span::{GetSpan, SourceType};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub use crate::project::{
    discover_project, DiscoveredProject, ProjectDiscoveryError, ProjectNotice, ProjectRoot,
};

/// One owned source handed to the frontend.
#[derive(Clone)]
pub struct FileInput {
    pub name: String,
    pub source: String,
}

/// Module-resolution policy selected before the single project parse/check lifecycle.
#[derive(Clone, Debug)]
pub enum ProjectResolutionMode {
    /// Preserve the existing explicit-file behavior for admitted named imports.
    ExplicitFileList,
    /// Resolve admitted named imports with the pinned Bundler resolver, then require
    /// every result to be one of the exact configured roots.
    BundlerProject {
        project_directory: PathBuf,
        roots: Vec<ProjectRoot>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectMissingModuleLocation {
    pub file: String,
    pub diagnostic_span: Span,
    pub summary_start: u32,
}

/// Deterministic evidence produced before any semantic work starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectModuleInventory {
    pub resolutions: Vec<String>,
    pub notices: Vec<String>,
    pub parse_errors: Vec<String>,
    pub missing_module_locations: Vec<ProjectMissingModuleLocation>,
}

impl ProjectModuleInventory {
    #[must_use]
    pub fn blocks_semantics(&self) -> bool {
        !self.parse_errors.is_empty() || !self.notices.is_empty()
    }
}

/// One frontend lifecycle product plus its independently reportable inventory.
pub struct AccountedProjectProduct<Product> {
    pub inventory: ProjectModuleInventory,
    pub product: Option<Product>,
}

/// Failures that cannot be represented by the frozen B72 project identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectInventoryError {
    detail: String,
}

impl ProjectInventoryError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for ProjectInventoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for ProjectInventoryError {}

pub enum DeferredProjectFrontendError<AuxiliaryError> {
    Inventory(ProjectInventoryError),
    Auxiliary(AuxiliaryError),
}

/// One owned auxiliary source parsed alongside a user project.
#[derive(Clone)]
pub struct AuxiliarySourceInput {
    pub source_ordinal: usize,
    pub name: String,
    pub source: String,
}

/// One parsed auxiliary unit handed to the project checker.
pub struct AuxiliaryProgram<'ast> {
    pub source_ordinal: usize,
    pub name: &'ast str,
    pub program: &'ast Program<'ast>,
    pub parser_diagnostics: Vec<AuxiliaryParserDiagnostic>,
    pub parser_panicked: bool,
}

/// Test-only work receipt produced at the real auxiliary parser entry point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuxiliaryParseWork {
    #[cfg(any(test, feature = "test-utils"))]
    pub parser_invocations: u64,
    #[cfg(any(test, feature = "test-utils"))]
    pub source_reparses: u64,
}

/// Parser evidence retained for authoritative auxiliary-source validation.
pub struct AuxiliaryParserDiagnostic {
    pub scope: Option<String>,
    pub number: Option<String>,
    pub labels: Vec<Span>,
    pub rendered: String,
}

/// One parsed project unit handed to the serial project checker.
pub struct ProjectProgram<'ast> {
    pub module_ordinal: ModuleOrdinal,
    pub unit_slot: UnitSlot,
    pub normalized_path: String,
    pub program: &'ast Program<'ast>,
    pub compilation_unit: CompilationUnit,
    pub imports: Vec<ProjectImport>,
}

/// One named import after the frontend has resolved its module specifier.
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

/// Structural module-member identity retained before checker integration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleMemberIdentity {
    Default,
    Named(String),
    Namespace,
}

/// Candidate disposition for one import declaration involving the default slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultImportCandidateDisposition {
    Admitted,
    NamedDefaultSyntax,
    MixedDefaultNamed,
    MixedDefaultNamespace,
}

/// Resolved source retained by the candidate default-import product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultImportCandidateSource {
    Resolved(usize),
    Missing,
    UnsupportedTarget(Option<String>),
}

/// AST form retained separately from the member identity it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultImportSpecifierSyntax {
    DirectDefault,
    Named,
    NamedDefault,
    Namespace,
}

/// One import specifier retained without collapsing default into the named map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultImportCandidateSpecifier {
    identity: ModuleMemberIdentity,
    syntax: DefaultImportSpecifierSyntax,
    local: String,
    type_only: bool,
    inline_type_only: bool,
    local_span: Span,
    span: Span,
}

impl DefaultImportCandidateSpecifier {
    #[must_use]
    pub const fn identity(&self) -> &ModuleMemberIdentity {
        &self.identity
    }

    #[must_use]
    pub const fn syntax(&self) -> DefaultImportSpecifierSyntax {
        self.syntax
    }

    #[must_use]
    pub fn local(&self) -> &str {
        &self.local
    }

    #[must_use]
    pub const fn is_type_only(&self) -> bool {
        self.type_only
    }

    #[must_use]
    pub const fn is_inline_type_only(&self) -> bool {
        self.inline_type_only
    }

    #[must_use]
    pub const fn local_span(&self) -> Span {
        self.local_span
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// One frontend-classified import declaration involving the default slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultImportCandidate {
    owner_module: usize,
    source: DefaultImportCandidateSource,
    module_specifier: String,
    declaration_span: Span,
    source_span: Span,
    owner_start: u32,
    declaration_type_only: bool,
    disposition: DefaultImportCandidateDisposition,
    specifiers: Vec<DefaultImportCandidateSpecifier>,
}

impl DefaultImportCandidate {
    #[must_use]
    pub const fn owner_module(&self) -> usize {
        self.owner_module
    }

    #[must_use]
    pub const fn source(&self) -> &DefaultImportCandidateSource {
        &self.source
    }

    #[must_use]
    pub fn module_specifier(&self) -> &str {
        &self.module_specifier
    }

    #[must_use]
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }

    #[must_use]
    pub const fn source_span(&self) -> Span {
        self.source_span
    }

    #[must_use]
    pub const fn owner_start(&self) -> u32 {
        self.owner_start
    }

    #[must_use]
    pub const fn is_declaration_type_only(&self) -> bool {
        self.declaration_type_only
    }

    #[must_use]
    pub const fn disposition(&self) -> DefaultImportCandidateDisposition {
        self.disposition
    }

    #[must_use]
    pub fn specifiers(&self) -> &[DefaultImportCandidateSpecifier] {
        &self.specifiers
    }
}

/// Syntactic origin of one occurrence that reads or produces a default slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultExportOccurrenceKind {
    DirectClass,
    DirectFunction,
    DefaultExpression,
    DefaultInterface,
    LocalExportListDefault,
    SourceExportToDefault,
    SourceDefaultReexport,
}

/// Why a module's candidate default surface cannot be published in this slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultExportCandidateBlock {
    DefaultInterface,
    DirectNamedNamespaceMerge,
    IdentifierNamespaceProvenance,
    LocalExportListDefault,
    SourceDefaultReexport,
    DuplicateDefault,
}

/// Root-order-stable identity for one retained default export occurrence.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefaultExportOccurrenceId {
    owner_source_key: SourceUnitKey,
    normalized_owner_path: String,
    declaration_start: u32,
    occurrence_ordinal: u32,
}

impl DefaultExportOccurrenceId {
    #[must_use]
    pub const fn owner_source_key(&self) -> SourceUnitKey {
        self.owner_source_key
    }

    #[must_use]
    pub fn normalized_owner_path(&self) -> &str {
        &self.normalized_owner_path
    }

    #[must_use]
    pub const fn declaration_start(&self) -> u32 {
        self.declaration_start
    }

    #[must_use]
    pub const fn occurrence_ordinal(&self) -> u32 {
        self.occurrence_ordinal
    }
}

/// One frontend-certified default export occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultExportOccurrence {
    id: DefaultExportOccurrenceId,
    kind: DefaultExportOccurrenceKind,
    declaration_span: Span,
    subject_span: Span,
    lexical_name: Option<String>,
    lexical_name_span: Option<Span>,
    declaration_anonymous: Option<bool>,
    type_only: bool,
    identifier_namespace_provenance: Option<NamespaceProvenance>,
    imported: Option<ModuleMemberIdentity>,
    exported: ModuleMemberIdentity,
    produces_default: bool,
}

impl DefaultExportOccurrence {
    #[must_use]
    pub const fn id(&self) -> &DefaultExportOccurrenceId {
        &self.id
    }

    #[must_use]
    pub const fn kind(&self) -> DefaultExportOccurrenceKind {
        self.kind
    }

    #[must_use]
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }

    #[must_use]
    pub const fn subject_span(&self) -> Span {
        self.subject_span
    }

    #[must_use]
    pub fn lexical_name(&self) -> Option<&str> {
        self.lexical_name.as_deref()
    }

    #[must_use]
    pub const fn lexical_name_span(&self) -> Option<Span> {
        self.lexical_name_span
    }

    #[must_use]
    pub const fn declaration_anonymous(&self) -> Option<bool> {
        self.declaration_anonymous
    }

    #[must_use]
    pub const fn is_type_only(&self) -> bool {
        self.type_only
    }

    #[must_use]
    pub const fn identifier_namespace_provenance(&self) -> Option<NamespaceProvenance> {
        self.identifier_namespace_provenance
    }

    #[must_use]
    pub const fn imported(&self) -> Option<&ModuleMemberIdentity> {
        self.imported.as_ref()
    }

    #[must_use]
    pub const fn exported(&self) -> &ModuleMemberIdentity {
        &self.exported
    }

    #[must_use]
    pub const fn produces_default(&self) -> bool {
        self.produces_default
    }
}

/// Candidate default-export surface for one dependency-ordered module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultExportCandidateModule {
    owner_module: usize,
    normalized_path: String,
    producer_count: usize,
    occurrences: Vec<DefaultExportOccurrence>,
    blocks: Vec<DefaultExportCandidateBlock>,
}

/// One dependency edge after root-order-independent module ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefaultModuleDependencyEdge {
    owner_module: usize,
    target_module: usize,
}

impl DefaultModuleDependencyEdge {
    #[must_use]
    pub const fn owner_module(&self) -> usize {
        self.owner_module
    }

    #[must_use]
    pub const fn target_module(&self) -> usize {
        self.target_module
    }
}

impl DefaultExportCandidateModule {
    #[must_use]
    pub const fn owner_module(&self) -> usize {
        self.owner_module
    }

    #[must_use]
    pub fn normalized_path(&self) -> &str {
        &self.normalized_path
    }

    #[must_use]
    pub const fn producer_count(&self) -> usize {
        self.producer_count
    }

    #[must_use]
    pub fn occurrences(&self) -> &[DefaultExportOccurrence] {
        &self.occurrences
    }

    #[must_use]
    pub fn blocks(&self) -> &[DefaultExportCandidateBlock] {
        &self.blocks
    }

    #[must_use]
    pub fn is_admitted(&self) -> bool {
        self.producer_count == 1 && self.blocks.is_empty()
    }
}

/// Opaque frontend proof for the future default-slot project route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultModuleCandidates {
    imports: Vec<DefaultImportCandidate>,
    exports: Vec<DefaultExportCandidateModule>,
    dependency_order: Vec<String>,
    dependency_edges: Vec<DefaultModuleDependencyEdge>,
    first_cycle: Option<Vec<String>>,
}

impl DefaultModuleCandidates {
    #[must_use]
    pub fn imports(&self) -> &[DefaultImportCandidate] {
        &self.imports
    }

    #[must_use]
    pub fn exports(&self) -> &[DefaultExportCandidateModule] {
        &self.exports
    }

    #[must_use]
    pub fn dependency_order(&self) -> &[String] {
        &self.dependency_order
    }

    #[must_use]
    pub const fn dependency_edge_count(&self) -> usize {
        self.dependency_edges.len()
    }

    #[must_use]
    pub fn dependency_edges(&self) -> &[DefaultModuleDependencyEdge] {
        &self.dependency_edges
    }

    #[must_use]
    pub fn first_cycle(&self) -> Option<&[String]> {
        self.first_cycle.as_deref()
    }
}

/// Root-order-stable declaration identity owned by one normalized source path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceReexportDeclarationId {
    owner_source_key: SourceUnitKey,
    normalized_owner_path: String,
    declaration_start: u32,
    source_ordinal: u32,
}

impl SourceReexportDeclarationId {
    #[must_use]
    pub const fn owner_source_key(&self) -> SourceUnitKey {
        self.owner_source_key
    }

    #[must_use]
    pub fn normalized_owner_path(&self) -> &str {
        &self.normalized_owner_path
    }

    #[must_use]
    pub const fn declaration_start(&self) -> u32 {
        self.declaration_start
    }

    #[must_use]
    pub const fn source_ordinal(&self) -> u32 {
        self.source_ordinal
    }
}

/// Frontend proof that projecting a named export cannot discard namespace meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamespaceProvenance {
    ProvenAbsent,
    PresentOrUnknown,
}

/// Resolved or missing source owned by one admitted declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmittedSourceReexportSource {
    Resolved(usize),
    Missing,
}

/// One source re-export member retained without creating a local binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedSourceReexportMember {
    imported: String,
    exported: String,
    span: Span,
    type_only: bool,
    namespace_provenance: Option<NamespaceProvenance>,
}

impl AdmittedSourceReexportMember {
    #[must_use]
    pub fn imported(&self) -> &str {
        &self.imported
    }

    #[must_use]
    pub fn exported(&self) -> &str {
        &self.exported
    }

    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    #[must_use]
    pub const fn is_type_only(&self) -> bool {
        self.type_only
    }

    /// Missing declarations bypass the namespace census and return `None`.
    #[must_use]
    pub const fn namespace_provenance(&self) -> Option<NamespaceProvenance> {
        self.namespace_provenance
    }
}

/// One declaration in the opaque admitted source-re-export product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedSourceReexportDeclaration {
    id: SourceReexportDeclarationId,
    owner_module: usize,
    source: AdmittedSourceReexportSource,
    module_specifier: String,
    declaration_span: Span,
    source_span: Span,
    owner_start: u32,
    members: Vec<AdmittedSourceReexportMember>,
}

impl AdmittedSourceReexportDeclaration {
    #[must_use]
    pub const fn id(&self) -> &SourceReexportDeclarationId {
        &self.id
    }

    #[must_use]
    pub const fn owner_module(&self) -> usize {
        self.owner_module
    }

    #[must_use]
    pub const fn source(&self) -> AdmittedSourceReexportSource {
        self.source
    }

    #[must_use]
    pub fn module_specifier(&self) -> &str {
        &self.module_specifier
    }

    #[must_use]
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }

    #[must_use]
    pub const fn source_span(&self) -> Span {
        self.source_span
    }

    #[must_use]
    pub const fn owner_start(&self) -> u32 {
        self.owner_start
    }

    #[must_use]
    pub fn members(&self) -> &[AdmittedSourceReexportMember] {
        &self.members
    }
}

/// Opaque frontend-owned proof product consumed by later checker integration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedSourceReexports {
    declarations: Vec<AdmittedSourceReexportDeclaration>,
}

impl AdmittedSourceReexports {
    #[must_use]
    pub fn declarations(&self) -> &[AdmittedSourceReexportDeclaration] {
        &self.declarations
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }
}

/// Read-only receipt for the frontend source re-export evidence tests.
#[cfg(any(test, feature = "test-utils"))]
pub struct SourceReexportCollectionForTest {
    admitted: AdmittedSourceReexports,
    dependency_order: Vec<String>,
    dependency_edge_count: usize,
    resolutions: Vec<String>,
    blocked_notices: Vec<String>,
    first_cycle: Option<Vec<String>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl SourceReexportCollectionForTest {
    #[must_use]
    pub fn admitted(&self) -> &AdmittedSourceReexports {
        &self.admitted
    }

    #[must_use]
    pub fn dependency_order(&self) -> &[String] {
        &self.dependency_order
    }

    #[must_use]
    pub const fn dependency_edge_count(&self) -> usize {
        self.dependency_edge_count
    }

    #[must_use]
    pub fn resolutions(&self) -> &[String] {
        &self.resolutions
    }

    #[must_use]
    pub fn blocked_notices(&self) -> &[String] {
        &self.blocked_notices
    }

    #[must_use]
    pub fn first_cycle(&self) -> Option<&[String]> {
        self.first_cycle.as_deref()
    }

    /// Corrupt one resolved proof for checker invariant-path testing.
    pub fn invalidate_first_resolved_provenance_for_test(
        &mut self,
    ) -> Result<(), ProjectInventoryError> {
        let member = self
            .admitted
            .declarations
            .iter_mut()
            .find_map(|declaration| match declaration.source {
                AdmittedSourceReexportSource::Resolved(_) => {
                    declaration.members.iter_mut().find(|member| {
                        member.namespace_provenance == Some(NamespaceProvenance::ProvenAbsent)
                    })
                }
                AdmittedSourceReexportSource::Missing => None,
            })
            .ok_or_else(|| {
                ProjectInventoryError::new(
                    "source re-export test corruption requires one resolved member",
                )
            })?;
        member.namespace_provenance = Some(NamespaceProvenance::PresentOrUnknown);
        Ok(())
    }
}

pub struct SourceFrontendRun<Product> {
    pub parse_errors: Vec<String>,
    pub product: Option<Product>,
}

/// Parse one source without exposing a recovered AST to semantic consumers.
pub fn parse_source_errors(source: &str) -> Vec<String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    parse_errors(&parsed)
}

fn parse_errors(parsed: &oxc_parser::ParserReturn<'_>) -> Vec<String> {
    let mut errors = parsed
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if parsed.panicked && errors.is_empty() {
        errors.push("parser panicked without a diagnostic".to_owned());
    }
    errors
}

#[cfg(any(test, feature = "test-utils"))]
fn parse_auxiliary_source<'ast>(
    allocator: &'ast Allocator,
    input: &'ast AuxiliarySourceInput,
    invocations: &mut BTreeMap<(usize, String), u64>,
) -> oxc_parser::ParserReturn<'ast> {
    let count = invocations
        .entry((input.source_ordinal, input.name.clone()))
        .or_default();
    *count = count.saturating_add(1);
    let source_type = if source_file_kind(&input.name).is_declaration() {
        SourceType::d_ts()
    } else {
        SourceType::ts()
    };
    Parser::new(allocator, &input.source, source_type).parse()
}

#[cfg(not(any(test, feature = "test-utils")))]
fn parse_auxiliary_source<'ast>(
    allocator: &'ast Allocator,
    input: &'ast AuxiliarySourceInput,
) -> oxc_parser::ParserReturn<'ast> {
    let source_type = if source_file_kind(&input.name).is_declaration() {
        SourceType::d_ts()
    } else {
        SourceType::ts()
    };
    Parser::new(allocator, &input.source, source_type).parse()
}

#[cfg(any(test, feature = "test-utils"))]
fn auxiliary_parse_work(invocations: &BTreeMap<(usize, String), u64>) -> AuxiliaryParseWork {
    AuxiliaryParseWork {
        parser_invocations: invocations.values().copied().sum(),
        source_reparses: invocations
            .values()
            .copied()
            .map(|count| count.saturating_sub(1))
            .sum(),
    }
}

/// Parse one TypeScript source and keep its borrowed AST inside `consume`.
pub fn run_source_frontend<Product>(
    source: &str,
    consume: impl for<'ast> FnOnce(&Program<'ast>) -> Product,
) -> SourceFrontendRun<Product> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let parse_errors = parsed.diagnostics.iter().map(ToString::to_string).collect();
    let product = (!parsed.panicked).then(|| consume(&parsed.program));
    SourceFrontendRun {
        parse_errors,
        product,
    }
}

pub struct ProjectFrontendRun<Product> {
    pub inputs: Vec<FileInput>,
    pub parse_errors: Vec<Vec<String>>,
    pub product: Product,
}

pub struct ParseOnlyProjectRun {
    pub inputs: Vec<FileInput>,
    pub parse_errors: Vec<Vec<String>>,
}

/// Parse a project without resolving imports or exposing recovered ASTs.
pub fn run_project_parse_only(inputs: Vec<FileInput>) -> ParseOnlyProjectRun {
    let allocators = (0..inputs.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parse_errors = inputs
        .iter()
        .zip(&allocators)
        .map(|(input, allocator)| {
            let parsed = Parser::new(allocator, &input.source, SourceType::ts()).parse();
            parse_errors(&parsed)
        })
        .collect();
    ParseOnlyProjectRun {
        inputs,
        parse_errors,
    }
}

impl<Product> ProjectFrontendRun<Product> {
    pub fn into_product(self) -> Product {
        self.product
    }
}

/// Parse, resolve, and dependency-order a local relative-module project.
pub fn run_project_frontend<Product>(
    inputs: Vec<FileInput>,
    consume: impl for<'ast> FnOnce(&mut Interner, &[ProjectProgram<'ast>]) -> Product,
) -> ProjectFrontendRun<Product> {
    run_project_frontend_with_auxiliary(inputs, Vec::new(), |interner, _, units| {
        consume(interner, units)
    })
}

/// Parse auxiliary sources and a local project under one frontend-owned lifetime.
///
/// Auxiliary programs retain their input order. They do not participate in user
/// import resolution or dependency ordering.
pub fn run_project_frontend_with_auxiliary<Product>(
    inputs: Vec<FileInput>,
    auxiliary: Vec<AuxiliarySourceInput>,
    consume: impl for<'ast> FnOnce(
        &mut Interner,
        &[AuxiliaryProgram<'ast>],
        &[ProjectProgram<'ast>],
    ) -> Product,
) -> ProjectFrontendRun<Product> {
    run_project_frontend_with_auxiliary_control(
        inputs,
        auxiliary,
        |_, interner, auxiliary_units, project_units| {
            consume(interner, auxiliary_units, project_units)
        },
    )
}

/// Parse a project once and expose semantics only when every user source parsed cleanly.
pub fn run_clean_project_frontend_with_auxiliary<Product>(
    inputs: Vec<FileInput>,
    auxiliary: Vec<AuxiliarySourceInput>,
    consume: impl for<'ast> FnOnce(
        &mut Interner,
        &[AuxiliaryProgram<'ast>],
        &[ProjectProgram<'ast>],
    ) -> Product,
) -> ProjectFrontendRun<Option<Product>> {
    run_project_frontend_with_auxiliary_control(
        inputs,
        auxiliary,
        |user_parse_rejected, interner, auxiliary_units, project_units| {
            if user_parse_rejected {
                None
            } else {
                Some(consume(interner, auxiliary_units, project_units))
            }
        },
    )
}

/// Parse user sources first and load auxiliary sources only for a clean project.
pub fn run_clean_project_frontend_with_deferred_auxiliary<Product, Error>(
    inputs: Vec<FileInput>,
    resolution_mode: ProjectResolutionMode,
    load_auxiliary: impl FnOnce() -> Result<Vec<AuxiliarySourceInput>, Error>,
    consume: impl for<'ast> FnOnce(
        &mut Interner,
        &[AuxiliarySourceInput],
        &[AuxiliaryProgram<'ast>],
        &[ProjectProgram<'ast>],
        AuxiliaryParseWork,
    ) -> Product,
) -> ProjectFrontendRun<Result<AccountedProjectProduct<Product>, DeferredProjectFrontendError<Error>>>
{
    run_clean_project_frontend_with_deferred_auxiliary_control(
        inputs,
        resolution_mode,
        SourceReexportAccounting::FrozenUnsupported,
        DefaultCandidateAccounting::FrozenUnsupported,
        load_auxiliary,
        move |interner,
              source_specs,
              auxiliary_units,
              project_units,
              parse_work,
              _source_reexports,
              _default_modules| {
            Ok(consume(
                interner,
                source_specs,
                auxiliary_units,
                project_units,
                parse_work,
            ))
        },
    )
}

/// Parse and account one Bundler project, then expose its admitted named source re-exports.
pub fn run_clean_bundler_project_frontend_with_deferred_auxiliary<Product, Error>(
    inputs: Vec<FileInput>,
    project_directory: PathBuf,
    roots: Vec<ProjectRoot>,
    load_auxiliary: impl FnOnce() -> Result<Vec<AuxiliarySourceInput>, Error>,
    consume: impl for<'ast> FnOnce(
        &mut Interner,
        &[AuxiliarySourceInput],
        &[AuxiliaryProgram<'ast>],
        &[ProjectProgram<'ast>],
        &AdmittedSourceReexports,
        AuxiliaryParseWork,
    ) -> Product,
) -> ProjectFrontendRun<Result<AccountedProjectProduct<Product>, DeferredProjectFrontendError<Error>>>
{
    run_clean_project_frontend_with_deferred_auxiliary_control(
        inputs,
        ProjectResolutionMode::BundlerProject {
            project_directory,
            roots,
        },
        SourceReexportAccounting::AdmitBundler,
        DefaultCandidateAccounting::FrozenUnsupported,
        load_auxiliary,
        move |interner,
              source_specs,
              auxiliary_units,
              project_units,
              parse_work,
              source_reexports,
              _default_modules| {
            let source_reexports = source_reexports.ok_or_else(|| {
                ProjectInventoryError::new(
                    "Bundler source re-export accounting lost its admitted product",
                )
            })?;
            Ok(consume(
                interner,
                source_specs,
                auxiliary_units,
                project_units,
                source_reexports,
                parse_work,
            ))
        },
    )
}

/// Parse and account one Bundler project through the certified default-slot product.
pub fn run_clean_bundler_project_frontend_with_default_modules<Product, Error>(
    inputs: Vec<FileInput>,
    project_directory: PathBuf,
    roots: Vec<ProjectRoot>,
    load_auxiliary: impl FnOnce() -> Result<Vec<AuxiliarySourceInput>, Error>,
    consume: impl for<'ast> FnOnce(
        &mut Interner,
        &[AuxiliarySourceInput],
        &[AuxiliaryProgram<'ast>],
        &[ProjectProgram<'ast>],
        &AdmittedSourceReexports,
        &DefaultModuleCandidates,
        AuxiliaryParseWork,
    ) -> Product,
) -> ProjectFrontendRun<Result<AccountedProjectProduct<Product>, DeferredProjectFrontendError<Error>>>
{
    run_clean_project_frontend_with_deferred_auxiliary_control(
        inputs,
        ProjectResolutionMode::BundlerProject {
            project_directory,
            roots,
        },
        SourceReexportAccounting::AdmitBundler,
        DefaultCandidateAccounting::CollectCandidate,
        load_auxiliary,
        move |interner,
              source_specs,
              auxiliary_units,
              project_units,
              parse_work,
              source_reexports,
              default_modules| {
            let source_reexports = source_reexports.ok_or_else(|| {
                ProjectInventoryError::new(
                    "Bundler source re-export accounting lost its admitted product",
                )
            })?;
            let default_modules = default_modules.ok_or_else(|| {
                ProjectInventoryError::new(
                    "Bundler default-module accounting lost its certified product",
                )
            })?;
            Ok(consume(
                interner,
                source_specs,
                auxiliary_units,
                project_units,
                source_reexports,
                default_modules,
                parse_work,
            ))
        },
    )
}

fn run_clean_project_frontend_with_deferred_auxiliary_control<Product, Error>(
    inputs: Vec<FileInput>,
    resolution_mode: ProjectResolutionMode,
    source_reexport_accounting: SourceReexportAccounting,
    default_candidate_accounting: DefaultCandidateAccounting,
    load_auxiliary: impl FnOnce() -> Result<Vec<AuxiliarySourceInput>, Error>,
    consume: impl for<'ast> FnOnce(
        &mut Interner,
        &[AuxiliarySourceInput],
        &[AuxiliaryProgram<'ast>],
        &[ProjectProgram<'ast>],
        AuxiliaryParseWork,
        Option<&AdmittedSourceReexports>,
        Option<&DefaultModuleCandidates>,
    ) -> Result<Product, ProjectInventoryError>,
) -> ProjectFrontendRun<Result<AccountedProjectProduct<Product>, DeferredProjectFrontendError<Error>>>
{
    let user_allocators = (0..inputs.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = inputs
        .iter()
        .zip(&user_allocators)
        .map(|(input, allocator)| Parser::new(allocator, &input.source, SourceType::ts()).parse())
        .collect::<Vec<_>>();
    let parse_errors = parsed.iter().map(parse_errors).collect::<Vec<_>>();
    let user_parse_rejected = parsed
        .iter()
        .any(|parsed| parsed.panicked || !parsed.diagnostics.is_empty());
    if user_parse_rejected {
        let parse_error_identities = structured_parse_error_identities(&inputs, &parsed);
        return ProjectFrontendRun {
            inputs,
            parse_errors,
            product: Ok(AccountedProjectProduct {
                inventory: ProjectModuleInventory {
                    parse_errors: parse_error_identities,
                    ..ProjectModuleInventory::default()
                },
                product: None,
            }),
        };
    }

    let programs = parsed
        .iter()
        .map(|parsed| &parsed.program)
        .collect::<Vec<_>>();
    let mut module_plan = match account_project_modules(
        &inputs,
        &programs,
        &resolution_mode,
        source_reexport_accounting,
        default_candidate_accounting,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return ProjectFrontendRun {
                inputs,
                parse_errors,
                product: Err(DeferredProjectFrontendError::Inventory(error)),
            };
        }
    };
    let default_modules = if matches!(
        default_candidate_accounting,
        DefaultCandidateAccounting::CollectCandidate
    ) {
        match certified_default_module_candidates(&inputs, &module_plan) {
            Ok(candidates) => {
                if let Err(error) = activate_default_module_inventory(
                    &inputs,
                    &module_plan.source_reexports,
                    &candidates,
                    &mut module_plan.inventory,
                ) {
                    return ProjectFrontendRun {
                        inputs,
                        parse_errors,
                        product: Err(DeferredProjectFrontendError::Inventory(error)),
                    };
                }
                Some(candidates)
            }
            Err(error) => {
                return ProjectFrontendRun {
                    inputs,
                    parse_errors,
                    product: Err(DeferredProjectFrontendError::Inventory(error)),
                };
            }
        }
    } else {
        None
    };
    if matches!(
        resolution_mode,
        ProjectResolutionMode::BundlerProject { .. }
    ) && module_plan.inventory.blocks_semantics()
    {
        return ProjectFrontendRun {
            inputs,
            parse_errors,
            product: Ok(AccountedProjectProduct {
                inventory: module_plan.inventory,
                product: None,
            }),
        };
    }

    let auxiliary = match load_auxiliary() {
        Ok(auxiliary) => auxiliary,
        Err(error) => {
            return ProjectFrontendRun {
                inputs,
                parse_errors,
                product: Err(DeferredProjectFrontendError::Auxiliary(error)),
            };
        }
    };
    let auxiliary_allocators = (0..auxiliary.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let mut auxiliary_parse_invocations = BTreeMap::new();
    let auxiliary_parsed = auxiliary
        .iter()
        .zip(&auxiliary_allocators)
        .map(|(input, allocator)| {
            #[cfg(any(test, feature = "test-utils"))]
            {
                parse_auxiliary_source(allocator, input, &mut auxiliary_parse_invocations)
            }
            #[cfg(not(any(test, feature = "test-utils")))]
            {
                parse_auxiliary_source(allocator, input)
            }
        })
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "test-utils"))]
    let auxiliary_parse_work = auxiliary_parse_work(&auxiliary_parse_invocations);
    #[cfg(not(any(test, feature = "test-utils")))]
    let auxiliary_parse_work = AuxiliaryParseWork::default();
    let default_dependency_order = default_modules
        .as_ref()
        .map(|candidates| candidate_original_order(&inputs, candidates))
        .transpose();
    let default_dependency_order = match default_dependency_order {
        Ok(order) => order,
        Err(error) => {
            return ProjectFrontendRun {
                inputs,
                parse_errors,
                product: Err(DeferredProjectFrontendError::Inventory(error)),
            };
        }
    };
    let AccountedModulePlan {
        inventory,
        paths,
        raw_imports,
        source_reexports,
        admitted_source_reexports,
        default_module_authority: _,
    } = module_plan;
    let (admitted_source_reexports, admitted_dependency_order) = admitted_source_reexports
        .map(|product| {
            (
                Some(AdmittedSourceReexports {
                    declarations: product.declarations,
                }),
                Some(product.dependency_order),
            )
        })
        .unwrap_or((None, None));
    let mut project_units = match default_dependency_order.or(admitted_dependency_order) {
        Some(order) => project_programs_from_accounted_imports_in_order(
            &inputs,
            &programs,
            paths,
            raw_imports,
            &order,
        ),
        None => project_programs_from_accounted_imports_with_reexports(
            &inputs,
            &programs,
            paths,
            raw_imports,
            &source_reexports.dependency_edges,
        ),
    };
    if let Some(candidates) = &default_modules {
        for (unit, path) in project_units.iter_mut().zip(&candidates.dependency_order) {
            unit.normalized_path.clone_from(path);
        }
    }
    let auxiliary_units = auxiliary
        .iter()
        .zip(&auxiliary_parsed)
        .map(|(input, parsed)| AuxiliaryProgram {
            source_ordinal: input.source_ordinal,
            name: &input.name,
            program: &parsed.program,
            parser_diagnostics: parsed
                .diagnostics
                .iter()
                .map(|diagnostic| AuxiliaryParserDiagnostic {
                    scope: diagnostic.code.scope.as_deref().map(str::to_owned),
                    number: diagnostic.code.number.as_deref().map(str::to_owned),
                    labels: diagnostic
                        .labels
                        .iter()
                        .map(|label| {
                            Span::new(label.offset(), label.offset().saturating_add(label.len()))
                        })
                        .collect(),
                    rendered: diagnostic.to_string(),
                })
                .collect(),
            parser_panicked: parsed.panicked,
        })
        .collect::<Vec<_>>();
    let mut interner = Interner::with_intrinsics();
    let product = match consume(
        &mut interner,
        &auxiliary,
        &auxiliary_units,
        &project_units,
        auxiliary_parse_work,
        admitted_source_reexports.as_ref(),
        default_modules.as_ref(),
    ) {
        Ok(product) => product,
        Err(error) => {
            return ProjectFrontendRun {
                inputs,
                parse_errors,
                product: Err(DeferredProjectFrontendError::Inventory(error)),
            };
        }
    };
    ProjectFrontendRun {
        inputs,
        parse_errors,
        product: Ok(AccountedProjectProduct {
            inventory,
            product: Some(product),
        }),
    }
}

fn run_project_frontend_with_auxiliary_control<Product>(
    inputs: Vec<FileInput>,
    auxiliary: Vec<AuxiliarySourceInput>,
    consume: impl for<'ast> FnOnce(
        bool,
        &mut Interner,
        &[AuxiliaryProgram<'ast>],
        &[ProjectProgram<'ast>],
    ) -> Product,
) -> ProjectFrontendRun<Product> {
    let source_count = inputs.len() + auxiliary.len();
    let allocators: Vec<Allocator> = (0..source_count).map(|_| Allocator::default()).collect();
    let parsed: Vec<_> = inputs
        .iter()
        .map(|input| (input.source.as_str(), SourceType::ts()))
        .chain(auxiliary.iter().map(|input| {
            let source_type = if source_file_kind(&input.name).is_declaration() {
                SourceType::d_ts()
            } else {
                SourceType::ts()
            };
            (input.source.as_str(), source_type)
        }))
        .zip(&allocators)
        .map(|((source, source_type), allocator)| {
            Parser::new(allocator, source, source_type).parse()
        })
        .collect();
    let (parsed, auxiliary_parsed) = parsed.split_at(inputs.len());

    let parse_errors = parsed.iter().map(parse_errors).collect::<Vec<_>>();
    let user_parse_rejected = parsed
        .iter()
        .any(|parsed| parsed.panicked || !parsed.diagnostics.is_empty());

    let programs = parsed
        .iter()
        .map(|parsed| &parsed.program)
        .collect::<Vec<_>>();
    let project_units = resolved_project_programs(&inputs, &programs);
    let auxiliary_units: Vec<AuxiliaryProgram<'_>> = auxiliary
        .iter()
        .zip(auxiliary_parsed)
        .map(|(input, parsed)| AuxiliaryProgram {
            source_ordinal: input.source_ordinal,
            name: &input.name,
            program: &parsed.program,
            parser_diagnostics: parsed
                .diagnostics
                .iter()
                .map(|diagnostic| AuxiliaryParserDiagnostic {
                    scope: diagnostic.code.scope.as_deref().map(str::to_owned),
                    number: diagnostic.code.number.as_deref().map(str::to_owned),
                    labels: diagnostic
                        .labels
                        .iter()
                        .map(|label| {
                            Span::new(label.offset(), label.offset().saturating_add(label.len()))
                        })
                        .collect(),
                    rendered: diagnostic.to_string(),
                })
                .collect(),
            parser_panicked: parsed.panicked,
        })
        .collect();

    let mut interner = Interner::with_intrinsics();
    let product = consume(
        user_parse_rejected,
        &mut interner,
        &auxiliary_units,
        &project_units,
    );
    ProjectFrontendRun {
        inputs,
        parse_errors,
        product,
    }
}

fn resolved_project_programs<'ast>(
    inputs: &[FileInput],
    programs: &[&'ast Program<'ast>],
) -> Vec<ProjectProgram<'ast>> {
    let paths = normalized_input_paths(inputs);
    let path_to_index: BTreeMap<PathBuf, usize> = paths
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, path)| (path, index))
        .collect();
    let raw_imports = programs
        .iter()
        .zip(&paths)
        .map(|(program, path)| scan_imports(program, path, &path_to_index))
        .collect::<Vec<_>>();
    project_programs_from_accounted_imports(inputs, programs, paths, raw_imports)
}

fn project_programs_from_accounted_imports<'ast>(
    inputs: &[FileInput],
    programs: &[&'ast Program<'ast>],
    paths: Vec<PathBuf>,
    raw_imports: Vec<Vec<RawImport>>,
) -> Vec<ProjectProgram<'ast>> {
    project_programs_from_accounted_imports_with_reexports(
        inputs,
        programs,
        paths,
        raw_imports,
        &[],
    )
}

fn project_programs_from_accounted_imports_with_reexports<'ast>(
    inputs: &[FileInput],
    programs: &[&'ast Program<'ast>],
    paths: Vec<PathBuf>,
    raw_imports: Vec<Vec<RawImport>>,
    reexport_edges: &[BTreeSet<usize>],
) -> Vec<ProjectProgram<'ast>> {
    let order = dependency_order_with_reexports(&raw_imports, reexport_edges);
    project_programs_from_accounted_imports_in_order(inputs, programs, paths, raw_imports, &order)
}

fn project_programs_from_accounted_imports_in_order<'ast>(
    inputs: &[FileInput],
    programs: &[&'ast Program<'ast>],
    paths: Vec<PathBuf>,
    raw_imports: Vec<Vec<RawImport>>,
    order: &[usize],
) -> Vec<ProjectProgram<'ast>> {
    let source_keys = stable_source_keys(&paths);
    let mut ordered_index = vec![0usize; inputs.len()];
    for (position, &original) in order.iter().enumerate() {
        if let Some(slot) = ordered_index.get_mut(original) {
            *slot = position;
        }
    }

    order
        .iter()
        .enumerate()
        .map(|(unit_slot, &original)| ProjectProgram {
            module_ordinal: ModuleOrdinal::new(original),
            unit_slot: UnitSlot::new(unit_slot),
            normalized_path: paths[original].to_string_lossy().into_owned(),
            program: programs[original],
            compilation_unit: CompilationUnit {
                source: source_keys[original],
                origin: CompilationOrigin::User(OriginalModuleOrdinal::new(original)),
                binding: ModuleBindingContext::for_program(
                    programs[original],
                    source_file_kind(&inputs[original].name),
                ),
            },
            imports: raw_imports[original]
                .iter()
                .map(|import| ProjectImport {
                    local: import.local.clone(),
                    imported: import.imported.clone(),
                    module: import.specifier.clone(),
                    source: match import.source {
                        RawImportSource::Resolved(target) => {
                            ProjectImportSource::Resolved(ordered_index[target])
                        }
                        RawImportSource::Missing => {
                            ProjectImportSource::Missing(import.specifier.clone())
                        }
                    },
                    type_only: import.type_only,
                    local_span: import.local_span,
                    span: import.span,
                    owner_start: import.owner_start,
                })
                .collect(),
        })
        .collect()
}

struct AccountedModulePlan {
    inventory: ProjectModuleInventory,
    paths: Vec<PathBuf>,
    raw_imports: Vec<Vec<RawImport>>,
    source_reexports: PendingSourceReexports,
    admitted_source_reexports: Option<AdmittedSourceReexportProduct>,
    default_module_authority: Option<DefaultModuleAuthority>,
}

struct AdmittedSourceReexportProduct {
    declarations: Vec<AdmittedSourceReexportDeclaration>,
    dependency_order: Vec<usize>,
}

#[derive(Clone, Copy)]
enum SourceReexportAccounting {
    FrozenUnsupported,
    AdmitBundler,
    #[cfg(any(test, feature = "test-utils"))]
    CollectEvidence,
}

impl SourceReexportAccounting {
    fn collects_evidence(self) -> bool {
        #[cfg(any(test, feature = "test-utils"))]
        {
            matches!(self, Self::AdmitBundler | Self::CollectEvidence)
        }
        #[cfg(not(any(test, feature = "test-utils")))]
        {
            matches!(self, Self::AdmitBundler)
        }
    }

    fn admits_product(self) -> bool {
        matches!(self, Self::AdmitBundler)
    }
}

#[derive(Clone, Copy)]
enum DefaultCandidateAccounting {
    FrozenUnsupported,
    CollectCandidate,
}

struct DefaultModuleAuthority {
    imports: Vec<RawDefaultImportCandidate>,
    exports: Vec<RawDefaultExportCandidateModule>,
    admitted_default_edges: Vec<BTreeSet<usize>>,
}

#[derive(Clone)]
struct RawSourceReexportMember {
    imported: String,
    exported: String,
    span: Span,
    type_only: bool,
}

#[derive(Clone)]
struct RawSourceReexportDeclaration {
    owner_module: usize,
    source: RawImportSource,
    module_specifier: String,
    declaration_span: Span,
    source_span: Span,
    owner_start: u32,
    members: Vec<RawSourceReexportMember>,
}

struct PendingSourceReexports {
    declarations: Vec<RawSourceReexportDeclaration>,
    dependency_edges: Vec<BTreeSet<usize>>,
    namespace_provenance: BTreeMap<(usize, String), NamespaceProvenance>,
    first_cycle: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawDefaultImportCandidate {
    owner_module: usize,
    source: RawDefaultImportCandidateSource,
    module_specifier: String,
    declaration_span: Span,
    source_span: Span,
    owner_start: u32,
    declaration_type_only: bool,
    disposition: DefaultImportCandidateDisposition,
    specifiers: Vec<DefaultImportCandidateSpecifier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RawDefaultImportCandidateSource {
    Resolved(usize),
    Missing,
    UnsupportedTarget(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawDefaultExportCandidateModule {
    owner_module: usize,
    producer_count: usize,
    occurrences: Vec<DefaultExportOccurrence>,
    blocks: Vec<DefaultExportCandidateBlock>,
}

struct DefaultImportShape {
    specifiers: Vec<DefaultImportCandidateSpecifier>,
    direct_default_count: usize,
    named_default_count: usize,
    named_count: usize,
    namespace_count: usize,
}

struct DefaultCandidateResolution<'a> {
    mode: &'a ProjectResolutionMode,
    resolver: &'a Resolver,
    importer_path: &'a Path,
    explicit_path_to_index: &'a BTreeMap<PathBuf, usize>,
    configured_roots: &'a BTreeMap<PathBuf, usize>,
    canonical_project: Option<&'a Path>,
}

impl PendingSourceReexports {
    fn validate(&self, module_count: usize) -> Result<(), ProjectInventoryError> {
        let dependencies_valid = self.dependency_edges.len() == module_count
            && self
                .dependency_edges
                .iter()
                .all(|targets| targets.iter().all(|target| *target < module_count));
        let declarations_valid = self.declarations.iter().all(|declaration| {
            let resolved_valid = match declaration.source {
                RawImportSource::Missing => true,
                RawImportSource::Resolved(target) => {
                    self.dependency_edges
                        .get(declaration.owner_module)
                        .is_some_and(|edges| edges.contains(&target))
                        && declaration.members.iter().all(|member| {
                            self.namespace_provenance
                                .contains_key(&(target, member.imported.clone()))
                        })
                }
            };
            declaration.owner_module < module_count
                && !declaration.module_specifier.is_empty()
                && !declaration.members.is_empty()
                && declaration.owner_start == declaration.declaration_span.start
                && declaration.source_span.start >= declaration.declaration_span.start
                && declaration.source_span.end <= declaration.declaration_span.end
                && resolved_valid
                && declaration.members.iter().all(|member| {
                    let _retained_type_only = member.type_only;
                    !member.imported.is_empty()
                        && !member.exported.is_empty()
                        && member.span.start >= declaration.declaration_span.start
                        && member.span.end <= declaration.declaration_span.end
                })
        });
        let cycle_valid = self
            .first_cycle
            .as_ref()
            .is_none_or(|cycle| cycle.len() >= 2);
        if !(dependencies_valid && declarations_valid && cycle_valid) {
            return Err(ProjectInventoryError::new(
                "source re-export metadata changed during accounting",
            ));
        }
        Ok(())
    }
}

struct ResolutionPaths {
    paths: Vec<PathBuf>,
    configured_roots: BTreeMap<PathBuf, usize>,
    canonical_project: Option<PathBuf>,
}

#[derive(Clone)]
struct LocatedIdentity {
    path: String,
    start: u32,
    identity: String,
}

fn structured_parse_error_identities(
    inputs: &[FileInput],
    parsed: &[oxc_parser::ParserReturn<'_>],
) -> Vec<String> {
    let mut identities = Vec::new();
    for (input, parsed) in inputs.iter().zip(parsed) {
        let line_index = crate::span::LineIndex::new(&input.source);
        for diagnostic in &parsed.diagnostics {
            let offset = diagnostic.labels.first().map_or(0, |label| label.offset());
            let position = line_index.line_col(offset);
            identities.push(LocatedIdentity {
                path: normalized_display_name(&input.name),
                start: offset,
                identity: format!(
                    "{}:{}:{} parser/unexpected-token",
                    normalized_display_name(&input.name),
                    position.line,
                    position.column
                ),
            });
        }
        if parsed.panicked && parsed.diagnostics.is_empty() {
            identities.push(LocatedIdentity {
                path: normalized_display_name(&input.name),
                start: 0,
                identity: format!("{}:1:1 parser/panic", normalized_display_name(&input.name)),
            });
        }
    }
    sorted_identities(identities)
}

fn account_project_modules(
    inputs: &[FileInput],
    programs: &[&Program<'_>],
    mode: &ProjectResolutionMode,
    source_reexport_accounting: SourceReexportAccounting,
    default_candidate_accounting: DefaultCandidateAccounting,
) -> Result<AccountedModulePlan, ProjectInventoryError> {
    let ResolutionPaths {
        paths,
        configured_roots,
        canonical_project,
    } = resolution_paths(inputs, mode)?;
    let explicit_path_to_index = paths
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, path)| (path, index))
        .collect::<BTreeMap<_, _>>();
    let resolver = Resolver::default();
    let mut raw_imports = vec![Vec::new(); inputs.len()];
    let cycle_path_to_index = paths
        .iter()
        .enumerate()
        .filter_map(|(index, path)| fs::canonicalize(path).ok().map(|path| (path, index)))
        .collect::<BTreeMap<_, _>>();
    let mut cycle_edges = vec![BTreeSet::new(); inputs.len()];
    let mut reexport_edges = vec![BTreeSet::new(); inputs.len()];
    let mut raw_source_reexports = Vec::new();
    let mut resolution_identities = Vec::new();
    let mut notice_identities = Vec::new();
    let mut missing_module_locations = Vec::new();
    let default_source_keys = matches!(
        default_candidate_accounting,
        DefaultCandidateAccounting::CollectCandidate
    )
    .then(|| stable_source_keys(&paths));
    let mut default_module_authority = matches!(
        default_candidate_accounting,
        DefaultCandidateAccounting::CollectCandidate
    )
    .then(|| DefaultModuleAuthority {
        imports: Vec::new(),
        exports: Vec::new(),
        admitted_default_edges: vec![BTreeSet::new(); inputs.len()],
    });

    for (index, ((input, program), importer_path)) in
        inputs.iter().zip(programs).zip(&paths).enumerate()
    {
        let line_index = crate::span::LineIndex::new(&input.source);
        let mut default_candidate_owner_starts = BTreeSet::new();
        if let Some(authority) = &mut default_module_authority {
            let imports = scan_default_import_candidates(
                index,
                program,
                &DefaultCandidateResolution {
                    mode,
                    resolver: &resolver,
                    importer_path,
                    explicit_path_to_index: &explicit_path_to_index,
                    configured_roots: &configured_roots,
                    canonical_project: canonical_project.as_deref(),
                },
            )?;
            validate_raw_default_imports(program, &imports)?;
            for import in &imports {
                default_candidate_owner_starts.insert(import.owner_start);
                if import.disposition == DefaultImportCandidateDisposition::Admitted {
                    if let RawDefaultImportCandidateSource::Resolved(target) = &import.source {
                        let edges =
                            authority
                                .admitted_default_edges
                                .get_mut(index)
                                .ok_or_else(|| {
                                    ProjectInventoryError::new(
                                        "default import owner escaped authority graph",
                                    )
                                })?;
                        edges.insert(*target);
                    }
                }
            }
            authority.imports.extend(imports);

            let owner_source_key = default_source_keys
                .as_ref()
                .and_then(|keys| keys.get(index))
                .copied()
                .ok_or_else(|| {
                    ProjectInventoryError::new(
                        "default export owner lost its stable source identity",
                    )
                })?;
            let normalized_owner_path = normalized_display_name(&input.name);
            let exports = scan_default_export_candidates(
                index,
                owner_source_key,
                &normalized_owner_path,
                program,
            );
            validate_raw_default_exports(
                program,
                owner_source_key,
                &normalized_owner_path,
                &exports,
            )?;
            if !exports.occurrences.is_empty() {
                authority.exports.push(exports);
            }
        }
        for statement in &program.body {
            match statement {
                Statement::ImportDeclaration(import) => {
                    if default_candidate_owner_starts.contains(&import.span.start) {
                        continue;
                    }
                    if import.phase.is_some() {
                        return Err(ProjectInventoryError::new(format!(
                            "unfrozen import phase surface at {}",
                            source_location(input, &line_index, import.span.start)
                        )));
                    }
                    let specifier = import.source.value.as_str();
                    if import.with_clause.is_some() {
                        record_unsupported_module_form(
                            input,
                            &line_index,
                            import.span.start,
                            "import-attributes",
                            Some(specifier),
                            &mut resolution_identities,
                            &mut notice_identities,
                        );
                        continue;
                    }
                    let form = classify_import_declaration(import)?;
                    if form != "named-import" {
                        record_unsupported_module_form(
                            input,
                            &line_index,
                            import.span.start,
                            form,
                            Some(specifier),
                            &mut resolution_identities,
                            &mut notice_identities,
                        );
                        continue;
                    }
                    if import.specifiers.as_ref().is_some_and(|specifiers| {
                        specifiers.iter().any(|item| {
                            matches!(
                                item,
                                ImportDeclarationSpecifier::ImportSpecifier(named)
                                    if matches!(named.imported, ModuleExportName::StringLiteral(_))
                            )
                        })
                    }) {
                        record_unsupported_module_form(
                            input,
                            &line_index,
                            import.span.start,
                            "string-literal-import-name",
                            Some(specifier),
                            &mut resolution_identities,
                            &mut notice_identities,
                        );
                        continue;
                    }
                    if is_local_relative(specifier) {
                        let unsupported_reason = if specifier.contains(['?', '#']) {
                            Some("query-or-fragment")
                        } else if specifier.ends_with(".ts") {
                            Some("explicit-ts-extension")
                        } else {
                            None
                        };
                        if let Some(reason) = unsupported_reason {
                            record_unsupported_module_specifier(
                                input,
                                &line_index,
                                import.span.start,
                                specifier,
                                reason,
                                &mut resolution_identities,
                                &mut notice_identities,
                            );
                            continue;
                        }
                    }
                    if !is_local_relative(specifier) {
                        let reason = if specifier.starts_with('#') {
                            "package-import"
                        } else if specifier.contains("://") {
                            "uri"
                        } else if Path::new(specifier).is_absolute() {
                            "absolute"
                        } else {
                            "bare"
                        };
                        record_unsupported_module_specifier(
                            input,
                            &line_index,
                            import.span.start,
                            specifier,
                            reason,
                            &mut resolution_identities,
                            &mut notice_identities,
                        );
                        continue;
                    }
                    let resolution = resolve_named_import(
                        mode,
                        &resolver,
                        importer_path,
                        specifier,
                        &explicit_path_to_index,
                        &configured_roots,
                        canonical_project.as_deref(),
                    )?;
                    let source = match resolution {
                        NamedImportOutcome::Source(source) => source,
                        NamedImportOutcome::UnsupportedTarget(target) => {
                            resolution_identities.push(located_identity(
                                input,
                                &line_index,
                                import.span.start,
                                format!("named-import {specifier} -> unsupported"),
                            ));
                            notice_identities.push(LocatedIdentity {
                                path: normalized_display_name(&input.name),
                                start: import.span.start,
                                identity: format!(
                                    "unsupported-module-target unconfigured {} {specifier} -> {}",
                                    source_location(input, &line_index, import.span.start),
                                    target.as_deref().unwrap_or("outside-project")
                                ),
                            });
                            continue;
                        }
                    };
                    let cycle_source = match (mode, source) {
                        (ProjectResolutionMode::ExplicitFileList, RawImportSource::Missing) => {
                            resolver
                                .resolve_dts(importer_path, specifier)
                                .ok()
                                .and_then(|resolution| fs::canonicalize(resolution.path()).ok())
                                .and_then(|path| cycle_path_to_index.get(&path).copied())
                                .map_or(RawImportSource::Missing, RawImportSource::Resolved)
                        }
                        _ => source,
                    };
                    if let RawImportSource::Resolved(target) = cycle_source {
                        cycle_edges[index].insert(target);
                    }
                    let outcome = match source {
                        RawImportSource::Resolved(target) => normalized_display_name(
                            inputs
                                .get(target)
                                .map_or("<invalid-root>", |target| target.name.as_str()),
                        ),
                        RawImportSource::Missing => "unresolved".to_owned(),
                    };
                    resolution_identities.push(located_identity(
                        input,
                        &line_index,
                        import.span.start,
                        format!("named-import {specifier} -> {outcome}"),
                    ));
                    let outer_type_only = import.import_kind == ImportOrExportKind::Type;
                    let specifiers = import.specifiers.as_ref().ok_or_else(|| {
                        ProjectInventoryError::new("named import lost its specifier list")
                    })?;
                    for item in specifiers {
                        let ImportDeclarationSpecifier::ImportSpecifier(named) = item else {
                            return Err(ProjectInventoryError::new(
                                "named import classification changed during inventory",
                            ));
                        };
                        let Some(imported) = module_export_name(&named.imported) else {
                            return Err(ProjectInventoryError::new(
                                "string-literal import escaped its inventory classification",
                            ));
                        };
                        if matches!(source, RawImportSource::Missing) {
                            missing_module_locations.push(ProjectMissingModuleLocation {
                                file: normalized_display_name(&input.name),
                                diagnostic_span: Span::from_oxc(named.local.span),
                                summary_start: import.source.span.start,
                            });
                        }
                        raw_imports[index].push(RawImport {
                            local: named.local.name.to_string(),
                            imported: imported.to_owned(),
                            specifier: specifier.to_owned(),
                            source,
                            type_only: outer_type_only
                                || named.import_kind == ImportOrExportKind::Type,
                            local_span: Span::from_oxc(named.local.span),
                            span: Span::from_oxc(named.span),
                            owner_start: import.span.start,
                        });
                    }
                }
                Statement::ExportNamedDeclaration(export) if export.source.is_some() => {
                    if export.with_clause.is_some() {
                        return Err(ProjectInventoryError::new(format!(
                            "unfrozen export attribute surface at {}",
                            source_location(input, &line_index, export.span.start)
                        )));
                    }
                    let source_literal = export.source.as_ref().ok_or_else(|| {
                        ProjectInventoryError::new("source re-export lost source")
                    })?;
                    let specifier = source_literal.value.as_str();
                    if export.specifiers.is_empty() {
                        if source_reexport_accounting.collects_evidence() {
                            continue;
                        }
                        record_unsupported_module_form(
                            input,
                            &line_index,
                            export.span.start,
                            "source-reexport",
                            Some(specifier),
                            &mut resolution_identities,
                            &mut notice_identities,
                        );
                        continue;
                    }
                    if source_reexport_accounting.collects_evidence()
                        && is_admitted_source_reexport_specifier(specifier)
                        && export.specifiers.iter().all(|member| {
                            module_export_name(&member.local).is_some()
                                && module_export_name(&member.exported).is_some()
                        })
                    {
                        let resolution = resolve_named_import(
                            mode,
                            &resolver,
                            importer_path,
                            specifier,
                            &explicit_path_to_index,
                            &configured_roots,
                            canonical_project.as_deref(),
                        )?;
                        if let NamedImportOutcome::Source(source) = resolution {
                            if let RawImportSource::Resolved(target) = source {
                                reexport_edges[index].insert(target);
                            }
                            let outer_type_only = export.export_kind == ImportOrExportKind::Type;
                            let members = export
                                .specifiers
                                .iter()
                                .map(|member| {
                                    let imported = module_export_name(&member.local).ok_or_else(|| {
                                        ProjectInventoryError::new(
                                            "classified source re-export lost its local name",
                                        )
                                    })?;
                                    let exported =
                                        module_export_name(&member.exported).ok_or_else(|| {
                                            ProjectInventoryError::new(
                                                "classified source re-export lost its exported name",
                                            )
                                        })?;
                                    Ok(RawSourceReexportMember {
                                        imported: imported.to_owned(),
                                        exported: exported.to_owned(),
                                        span: Span::from_oxc(member.span),
                                        type_only: outer_type_only
                                            || member.export_kind == ImportOrExportKind::Type,
                                    })
                                })
                                .collect::<Result<Vec<_>, ProjectInventoryError>>()?;
                            raw_source_reexports.push(RawSourceReexportDeclaration {
                                owner_module: index,
                                source,
                                module_specifier: specifier.to_owned(),
                                declaration_span: Span::from_oxc(export.span),
                                source_span: Span::from_oxc(source_literal.span),
                                owner_start: export.span.start,
                                members,
                            });
                            if source_reexport_accounting.admits_product() {
                                continue;
                            }
                        }
                    }
                    record_unsupported_module_form(
                        input,
                        &line_index,
                        export.span.start,
                        "source-reexport",
                        Some(specifier),
                        &mut resolution_identities,
                        &mut notice_identities,
                    );
                }
                Statement::ExportAllDeclaration(export) => {
                    if export.with_clause.is_some() {
                        return Err(ProjectInventoryError::new(format!(
                            "unfrozen export attribute surface at {}",
                            source_location(input, &line_index, export.span.start)
                        )));
                    }
                    let form = if export.exported.is_some() {
                        "namespace-reexport"
                    } else {
                        "star-reexport"
                    };
                    record_unsupported_module_form(
                        input,
                        &line_index,
                        export.span.start,
                        form,
                        Some(export.source.value.as_str()),
                        &mut resolution_identities,
                        &mut notice_identities,
                    );
                }
                Statement::ExportDefaultDeclaration(export) => {
                    if matches!(
                        default_candidate_accounting,
                        DefaultCandidateAccounting::FrozenUnsupported
                    ) {
                        record_unsupported_module_form(
                            input,
                            &line_index,
                            export.span.start,
                            "default-export",
                            None,
                            &mut resolution_identities,
                            &mut notice_identities,
                        );
                    }
                }
                Statement::TSImportEqualsDeclaration(import) => {
                    let specifier = match &import.module_reference {
                        TSModuleReference::ExternalModuleReference(reference) => {
                            Some(reference.expression.value.as_str())
                        }
                        TSModuleReference::IdentifierReference(_)
                        | TSModuleReference::QualifiedName(_) => None,
                    };
                    record_unsupported_module_form(
                        input,
                        &line_index,
                        import.span.start,
                        "import-equals",
                        specifier,
                        &mut resolution_identities,
                        &mut notice_identities,
                    );
                }
                Statement::TSExportAssignment(export) => {
                    record_unsupported_module_form(
                        input,
                        &line_index,
                        export.span.start,
                        "export-assignment",
                        None,
                        &mut resolution_identities,
                        &mut notice_identities,
                    );
                }
                Statement::TSNamespaceExportDeclaration(export) => {
                    record_unsupported_module_form(
                        input,
                        &line_index,
                        export.span.start,
                        "export-as-namespace",
                        None,
                        &mut resolution_identities,
                        &mut notice_identities,
                    );
                }
                _ => {}
            }
        }
    }

    let import_cycle = first_module_cycle(inputs, &cycle_edges);
    if let Some(cycle) = &import_cycle {
        notice_identities.push(LocatedIdentity {
            path: cycle.first().cloned().unwrap_or_default(),
            start: 0,
            identity: format!("unsupported-module-cycle {}", cycle.join(" -> ")),
        });
    }

    let source_reexports = if source_reexport_accounting.collects_evidence() {
        let mut combined_edges = raw_imports
            .iter()
            .map(|imports| {
                imports
                    .iter()
                    .filter_map(|import| match import.source {
                        RawImportSource::Resolved(target) => Some(target),
                        RawImportSource::Missing => None,
                    })
                    .collect::<BTreeSet<_>>()
            })
            .collect::<Vec<_>>();
        for (edges, reexports) in combined_edges.iter_mut().zip(&reexport_edges) {
            edges.extend(reexports);
        }
        let first_cycle = first_module_cycle(inputs, &combined_edges);
        let namespace_provenance = source_reexport_namespace_provenance(
            programs,
            &raw_imports,
            &raw_source_reexports,
            &reexport_edges,
            first_cycle.is_some(),
        );
        PendingSourceReexports {
            declarations: raw_source_reexports,
            dependency_edges: reexport_edges,
            namespace_provenance,
            first_cycle,
        }
    } else {
        PendingSourceReexports {
            declarations: Vec::new(),
            dependency_edges: vec![BTreeSet::new(); inputs.len()],
            namespace_provenance: BTreeMap::new(),
            first_cycle: None,
        }
    };
    source_reexports.validate(inputs.len())?;

    let admitted_source_reexports = if source_reexport_accounting.admits_product() {
        let finalized =
            finalize_admitted_source_reexports(inputs, &paths, &raw_imports, &source_reexports)?;
        let FinalizedSourceReexports {
            admitted,
            order,
            resolutions,
            blocked_notices,
            first_cycle,
        } = finalized;
        resolution_identities.extend(resolutions);
        notice_identities.extend(blocked_notices);
        if import_cycle.is_none() {
            if let Some(cycle) = &first_cycle {
                notice_identities.push(LocatedIdentity {
                    path: cycle.first().cloned().unwrap_or_default(),
                    start: 0,
                    identity: format!("unsupported-module-cycle {}", cycle.join(" -> ")),
                });
            }
        }
        Some(AdmittedSourceReexportProduct {
            declarations: admitted,
            dependency_order: order,
        })
    } else {
        None
    };

    Ok(AccountedModulePlan {
        inventory: ProjectModuleInventory {
            resolutions: sorted_identities(resolution_identities),
            notices: sorted_identities(notice_identities),
            parse_errors: Vec::new(),
            missing_module_locations,
        },
        paths,
        raw_imports,
        source_reexports,
        admitted_source_reexports,
        default_module_authority,
    })
}

/// Collect WU2 evidence without changing any production project route.
#[cfg(any(test, feature = "test-utils"))]
pub fn collect_admitted_source_reexports_for_test(
    inputs: Vec<FileInput>,
    mode: ProjectResolutionMode,
) -> Result<SourceReexportCollectionForTest, ProjectInventoryError> {
    let allocators = (0..inputs.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = inputs
        .iter()
        .zip(&allocators)
        .map(|(input, allocator)| Parser::new(allocator, &input.source, SourceType::ts()).parse())
        .collect::<Vec<_>>();
    if parsed
        .iter()
        .any(|result| result.panicked || !result.diagnostics.is_empty())
    {
        return Err(ProjectInventoryError::new(
            "source re-export test collector requires clean TypeScript input",
        ));
    }
    let programs = parsed
        .iter()
        .map(|result| &result.program)
        .collect::<Vec<_>>();
    let AccountedModulePlan {
        paths,
        raw_imports,
        source_reexports,
        ..
    } = account_project_modules(
        &inputs,
        &programs,
        &mode,
        SourceReexportAccounting::CollectEvidence,
        DefaultCandidateAccounting::FrozenUnsupported,
    )?;
    let finalized =
        finalize_admitted_source_reexports(&inputs, &paths, &raw_imports, &source_reexports)?;
    let dependency_edge_count = source_reexports
        .declarations
        .iter()
        .filter(|declaration| matches!(declaration.source, RawImportSource::Resolved(_)))
        .count();
    let dependency_order = finalized
        .order
        .iter()
        .map(|original| normalized_display_name(&inputs[*original].name))
        .collect();
    Ok(SourceReexportCollectionForTest {
        admitted: AdmittedSourceReexports {
            declarations: finalized.admitted,
        },
        dependency_order,
        dependency_edge_count,
        resolutions: sorted_identities(finalized.resolutions),
        blocked_notices: sorted_identities(finalized.blocked_notices),
        first_cycle: finalized.first_cycle,
    })
}

/// Build the future default-slot proof without selecting it for a production route.
#[cfg(any(test, feature = "test-utils"))]
pub fn collect_default_module_candidates(
    inputs: Vec<FileInput>,
    mode: ProjectResolutionMode,
) -> Result<DefaultModuleCandidates, ProjectInventoryError> {
    collect_default_module_candidates_with_mutation(inputs, mode, |_| Ok(()))
}

#[cfg(any(test, feature = "test-utils"))]
fn collect_default_module_candidates_with_mutation(
    inputs: Vec<FileInput>,
    mode: ProjectResolutionMode,
    mutate: impl FnOnce(&mut DefaultModuleCandidates) -> Result<(), ProjectInventoryError>,
) -> Result<DefaultModuleCandidates, ProjectInventoryError> {
    let allocators = inputs
        .iter()
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = inputs
        .iter()
        .zip(&allocators)
        .map(|(input, allocator)| Parser::new(allocator, &input.source, SourceType::ts()).parse())
        .collect::<Vec<_>>();
    if parsed
        .iter()
        .any(|result| result.panicked || !result.diagnostics.is_empty())
    {
        return Err(ProjectInventoryError::new(
            "default-module test collector requires clean TypeScript input",
        ));
    }
    let programs = parsed
        .iter()
        .map(|result| &result.program)
        .collect::<Vec<_>>();
    certify_default_module_candidates_from_parsed_programs_with_mutation(
        &inputs, &programs, &mode, mutate,
    )
}

/// Certify candidate default evidence from an existing clean project parse.
#[doc(hidden)]
pub fn certify_default_module_candidates_from_parsed_programs(
    inputs: &[FileInput],
    programs: &[&Program<'_>],
    mode: &ProjectResolutionMode,
) -> Result<DefaultModuleCandidates, ProjectInventoryError> {
    validate_default_candidate_parsed_programs(inputs, programs)?;
    let plan = account_project_modules(
        inputs,
        programs,
        mode,
        SourceReexportAccounting::AdmitBundler,
        DefaultCandidateAccounting::CollectCandidate,
    )?;
    finalize_default_module_candidates(inputs, plan)
}

#[cfg(any(test, feature = "test-utils"))]
fn certify_default_module_candidates_from_parsed_programs_with_mutation(
    inputs: &[FileInput],
    programs: &[&Program<'_>],
    mode: &ProjectResolutionMode,
    mutate: impl FnOnce(&mut DefaultModuleCandidates) -> Result<(), ProjectInventoryError>,
) -> Result<DefaultModuleCandidates, ProjectInventoryError> {
    validate_default_candidate_parsed_programs(inputs, programs)?;
    let plan = account_project_modules(
        inputs,
        programs,
        mode,
        SourceReexportAccounting::AdmitBundler,
        DefaultCandidateAccounting::CollectCandidate,
    )?;
    finalize_default_module_candidates_with_mutation(inputs, plan, mutate)
}

fn validate_default_candidate_parsed_programs(
    inputs: &[FileInput],
    programs: &[&Program<'_>],
) -> Result<(), ProjectInventoryError> {
    if inputs.len() != programs.len() {
        return Err(ProjectInventoryError::new(
            "default-module parsed-program coverage changed before accounting",
        ));
    }
    for (input, program) in inputs.iter().zip(programs) {
        if input.source != program.source_text {
            return Err(ProjectInventoryError::new(format!(
                "default-module parsed source changed for {}",
                normalized_display_name(&input.name)
            )));
        }
    }
    Ok(())
}

fn finalize_default_module_candidates(
    inputs: &[FileInput],
    plan: AccountedModulePlan,
) -> Result<DefaultModuleCandidates, ProjectInventoryError> {
    certified_default_module_candidates(inputs, &plan)
}

#[cfg(any(test, feature = "test-utils"))]
fn finalize_default_module_candidates_with_mutation(
    inputs: &[FileInput],
    plan: AccountedModulePlan,
    mutate: impl FnOnce(&mut DefaultModuleCandidates) -> Result<(), ProjectInventoryError>,
) -> Result<DefaultModuleCandidates, ProjectInventoryError> {
    let authority = plan.default_module_authority.as_ref().ok_or_else(|| {
        ProjectInventoryError::new("default-module accounting lost its raw authority")
    })?;
    let graph =
        default_candidate_graph(inputs, &plan.raw_imports, &plan.source_reexports, authority)?;
    let mut product = derive_default_module_candidates(inputs, authority, &graph)?;
    mutate(&mut product)?;
    validate_default_module_candidates(
        inputs,
        &plan.raw_imports,
        &plan.source_reexports,
        authority,
        &product,
    )?;
    Ok(product)
}

fn certified_default_module_candidates(
    inputs: &[FileInput],
    plan: &AccountedModulePlan,
) -> Result<DefaultModuleCandidates, ProjectInventoryError> {
    let authority = plan.default_module_authority.as_ref().ok_or_else(|| {
        ProjectInventoryError::new("default-module accounting lost its raw authority")
    })?;
    let graph =
        default_candidate_graph(inputs, &plan.raw_imports, &plan.source_reexports, authority)?;
    let product = derive_default_module_candidates(inputs, authority, &graph)?;
    validate_default_module_candidates(
        inputs,
        &plan.raw_imports,
        &plan.source_reexports,
        authority,
        &product,
    )?;
    Ok(product)
}

struct DefaultCandidateGraph {
    edges: Vec<BTreeSet<usize>>,
    first_cycle: Option<Vec<String>>,
    order: Vec<usize>,
    ordered_index: Vec<usize>,
}

fn default_candidate_graph(
    inputs: &[FileInput],
    raw_imports: &[Vec<RawImport>],
    source_reexports: &PendingSourceReexports,
    authority: &DefaultModuleAuthority,
) -> Result<DefaultCandidateGraph, ProjectInventoryError> {
    if raw_imports.len() != inputs.len() || authority.admitted_default_edges.len() != inputs.len() {
        return Err(ProjectInventoryError::new(
            "default-module dependency graph changed before derivation",
        ));
    }
    let mut edges = vec![BTreeSet::new(); inputs.len()];
    for declaration in &source_reexports.declarations {
        let involves_default = declaration
            .members
            .iter()
            .any(|member| member.imported == "default" || member.exported == "default");
        if involves_default {
            continue;
        }
        if let RawImportSource::Resolved(target) = declaration.source {
            let Some(owner_edges) = edges.get_mut(declaration.owner_module) else {
                return Err(ProjectInventoryError::new(
                    "source re-export owner escaped default dependency graph",
                ));
            };
            owner_edges.insert(target);
        }
    }
    for (owner_edges, default_edges) in edges.iter_mut().zip(&authority.admitted_default_edges) {
        owner_edges.extend(default_edges);
    }

    let deferred_import_declarations = authority
        .imports
        .iter()
        .filter(|import| import.disposition != DefaultImportCandidateDisposition::Admitted)
        .map(|import| (import.owner_module, import.owner_start))
        .collect::<BTreeSet<_>>();
    for (owner, imports) in raw_imports.iter().enumerate() {
        let Some(owner_edges) = edges.get_mut(owner) else {
            return Err(ProjectInventoryError::new(
                "named import owner escaped default dependency graph",
            ));
        };
        for import in imports {
            if deferred_import_declarations.contains(&(owner, import.owner_start)) {
                continue;
            }
            if let RawImportSource::Resolved(target) = import.source {
                owner_edges.insert(target);
            }
        }
    }
    let first_cycle = first_module_cycle(inputs, &edges);
    let order = stable_dependency_order(inputs, &edges);
    let mut ordered_index = vec![0usize; inputs.len()];
    for (position, original) in order.iter().copied().enumerate() {
        let Some(slot) = ordered_index.get_mut(original) else {
            return Err(ProjectInventoryError::new(
                "default-module order escaped configured roots",
            ));
        };
        *slot = position;
    }
    Ok(DefaultCandidateGraph {
        edges,
        first_cycle,
        order,
        ordered_index,
    })
}

fn derive_default_module_candidates(
    inputs: &[FileInput],
    authority: &DefaultModuleAuthority,
    graph: &DefaultCandidateGraph,
) -> Result<DefaultModuleCandidates, ProjectInventoryError> {
    let mut imports = authority
        .imports
        .iter()
        .map(|candidate| {
            let owner = inputs.get(candidate.owner_module).ok_or_else(|| {
                ProjectInventoryError::new("default import authority escaped configured roots")
            })?;
            let owner_module = graph
                .ordered_index
                .get(candidate.owner_module)
                .copied()
                .ok_or_else(|| {
                    ProjectInventoryError::new("default import owner escaped dependency order")
                })?;
            let source = match candidate.source {
                RawDefaultImportCandidateSource::Resolved(target) => {
                    DefaultImportCandidateSource::Resolved(
                        graph.ordered_index.get(target).copied().ok_or_else(|| {
                            ProjectInventoryError::new(
                                "default import target escaped dependency order",
                            )
                        })?,
                    )
                }
                RawDefaultImportCandidateSource::Missing => DefaultImportCandidateSource::Missing,
                RawDefaultImportCandidateSource::UnsupportedTarget(ref target) => {
                    DefaultImportCandidateSource::UnsupportedTarget(target.clone())
                }
            };
            Ok((
                normalized_display_name(&owner.name),
                DefaultImportCandidate {
                    owner_module,
                    source,
                    module_specifier: candidate.module_specifier.clone(),
                    declaration_span: candidate.declaration_span,
                    source_span: candidate.source_span,
                    owner_start: candidate.owner_start,
                    declaration_type_only: candidate.declaration_type_only,
                    disposition: candidate.disposition,
                    specifiers: candidate.specifiers.clone(),
                },
            ))
        })
        .collect::<Result<Vec<_>, ProjectInventoryError>>()?;
    imports
        .sort_by(|left, right| (&left.0, left.1.owner_start).cmp(&(&right.0, right.1.owner_start)));
    let imports: Vec<DefaultImportCandidate> =
        imports.into_iter().map(|(_, import)| import).collect();

    let mut exports = authority
        .exports
        .iter()
        .map(|candidate| {
            let owner = inputs.get(candidate.owner_module).ok_or_else(|| {
                ProjectInventoryError::new("default export authority escaped configured roots")
            })?;
            let normalized_path = normalized_display_name(&owner.name);
            let owner_module = graph
                .ordered_index
                .get(candidate.owner_module)
                .copied()
                .ok_or_else(|| {
                    ProjectInventoryError::new("default export owner escaped dependency order")
                })?;
            Ok(DefaultExportCandidateModule {
                owner_module,
                normalized_path,
                producer_count: candidate.producer_count,
                occurrences: candidate.occurrences.clone(),
                blocks: candidate.blocks.clone(),
            })
        })
        .collect::<Result<Vec<_>, ProjectInventoryError>>()?;
    exports.sort_by(|left, right| left.normalized_path.cmp(&right.normalized_path));

    let dependency_order = graph
        .order
        .iter()
        .map(|original| normalized_display_name(&inputs[*original].name))
        .collect::<Vec<_>>();
    let mut dependency_edges = Vec::new();
    for (owner, targets) in graph.edges.iter().enumerate() {
        for target in targets {
            let owner_module = graph.ordered_index.get(owner).copied().ok_or_else(|| {
                ProjectInventoryError::new("dependency owner escaped default-module order")
            })?;
            let target_module = graph.ordered_index.get(*target).copied().ok_or_else(|| {
                ProjectInventoryError::new("dependency target escaped default-module order")
            })?;
            dependency_edges.push(DefaultModuleDependencyEdge {
                owner_module,
                target_module,
            });
        }
    }
    dependency_edges.sort();
    Ok(DefaultModuleCandidates {
        imports,
        exports,
        dependency_order,
        dependency_edges,
        first_cycle: graph.first_cycle.clone(),
    })
}

fn candidate_original_order(
    inputs: &[FileInput],
    candidates: &DefaultModuleCandidates,
) -> Result<Vec<usize>, ProjectInventoryError> {
    let by_path = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| (normalized_display_name(&input.name), index))
        .collect::<BTreeMap<_, _>>();
    let order = candidates
        .dependency_order
        .iter()
        .map(|path| {
            by_path.get(path).copied().ok_or_else(|| {
                ProjectInventoryError::new(
                    "default-module dependency order names an unknown project root",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if order.len() != inputs.len()
        || order.iter().copied().collect::<BTreeSet<_>>().len() != inputs.len()
    {
        return Err(ProjectInventoryError::new(
            "default-module dependency order does not cover the project exactly",
        ));
    }
    Ok(order)
}

fn default_import_specifier_inventory(import: &DefaultImportCandidate) -> String {
    import
        .specifiers
        .iter()
        .map(|specifier| {
            let kind = match specifier.identity {
                ModuleMemberIdentity::Default => "default",
                ModuleMemberIdentity::Named(_) if specifier.type_only => "type",
                ModuleMemberIdentity::Named(_) => "named",
                ModuleMemberIdentity::Namespace => "namespace",
            };
            format!("{kind}:{}", specifier.local)
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn default_member_name(identity: &ModuleMemberIdentity) -> &str {
    match identity {
        ModuleMemberIdentity::Default => "default",
        ModuleMemberIdentity::Named(name) => name,
        ModuleMemberIdentity::Namespace => "namespace",
    }
}

fn default_producer_name(kind: DefaultExportOccurrenceKind) -> &'static str {
    match kind {
        DefaultExportOccurrenceKind::DirectClass => "direct-class",
        DefaultExportOccurrenceKind::DirectFunction => "direct-function",
        DefaultExportOccurrenceKind::DefaultExpression => "default-expression",
        DefaultExportOccurrenceKind::DefaultInterface => "default-interface",
        DefaultExportOccurrenceKind::LocalExportListDefault => "local-export-list-default",
        DefaultExportOccurrenceKind::SourceExportToDefault => "source-export-to-default",
        DefaultExportOccurrenceKind::SourceDefaultReexport => "source-default-reexport",
    }
}

fn activate_default_module_inventory(
    inputs: &[FileInput],
    source_reexports: &PendingSourceReexports,
    candidates: &DefaultModuleCandidates,
    inventory: &mut ProjectModuleInventory,
) -> Result<(), ProjectInventoryError> {
    let original_order = candidate_original_order(inputs, candidates)?;
    let input_for_module = |module: usize| {
        original_order
            .get(module)
            .and_then(|original| inputs.get(*original).map(|input| (*original, input)))
            .ok_or_else(|| {
                ProjectInventoryError::new("default-module owner escaped the certified order")
            })
    };
    let mut resolutions = Vec::new();
    let mut notices = Vec::new();

    for import in &candidates.imports {
        let (_, owner) = input_for_module(import.owner_module)?;
        let line_index = crate::span::LineIndex::new(&owner.source);
        let location = source_location(owner, &line_index, import.owner_start);
        let outcome = match &import.source {
            DefaultImportCandidateSource::Resolved(target) => candidates
                .dependency_order
                .get(*target)
                .cloned()
                .ok_or_else(|| {
                    ProjectInventoryError::new(
                        "default import target escaped the certified dependency order",
                    )
                })?,
            DefaultImportCandidateSource::Missing => "unresolved".to_owned(),
            DefaultImportCandidateSource::UnsupportedTarget(_) => "unsupported".to_owned(),
        };
        let (form, inventory_suffix) = match import.disposition {
            DefaultImportCandidateDisposition::Admitted => ("default-import", String::new()),
            DefaultImportCandidateDisposition::NamedDefaultSyntax => {
                ("named-default-import", String::new())
            }
            DefaultImportCandidateDisposition::MixedDefaultNamed => (
                "mixed-default-named-import",
                format!(" [{}]", default_import_specifier_inventory(import)),
            ),
            DefaultImportCandidateDisposition::MixedDefaultNamespace => (
                "mixed-default-namespace-import",
                format!(" [{}]", default_import_specifier_inventory(import)),
            ),
        };
        resolutions.push(LocatedIdentity {
            path: normalized_display_name(&owner.name),
            start: import.owner_start,
            identity: format!(
                "{location} {form} {} -> {outcome}{inventory_suffix}",
                import.module_specifier
            ),
        });
        match import.disposition {
            DefaultImportCandidateDisposition::Admitted => match &import.source {
                DefaultImportCandidateSource::Missing => {
                    let specifier = import.specifiers.first().ok_or_else(|| {
                        ProjectInventoryError::new("admitted default import lost its specifier")
                    })?;
                    inventory
                        .missing_module_locations
                        .push(ProjectMissingModuleLocation {
                            file: normalized_display_name(&owner.name),
                            diagnostic_span: specifier.local_span,
                            summary_start: import.source_span.start,
                        });
                }
                DefaultImportCandidateSource::UnsupportedTarget(target) => {
                    notices.push(LocatedIdentity {
                        path: normalized_display_name(&owner.name),
                        start: import.owner_start,
                        identity: match target {
                            Some(target) => format!(
                                "unsupported-module-target unconfigured {location} {} -> {target}",
                                import.module_specifier
                            ),
                            None => format!(
                                "unsupported-module-form default-import {location} {}",
                                import.module_specifier
                            ),
                        },
                    });
                }
                DefaultImportCandidateSource::Resolved(_) => {}
            },
            DefaultImportCandidateDisposition::NamedDefaultSyntax
            | DefaultImportCandidateDisposition::MixedDefaultNamed
            | DefaultImportCandidateDisposition::MixedDefaultNamespace => {
                notices.push(LocatedIdentity {
                    path: normalized_display_name(&owner.name),
                    start: import.owner_start,
                    identity: format!(
                        "unsupported-module-form {form} {location} {}{inventory_suffix}",
                        import.module_specifier
                    ),
                });
            }
        }
    }

    for module in &candidates.exports {
        let (original_owner, owner) = input_for_module(module.owner_module)?;
        if module.producer_count > 1 {
            let producers = module
                .occurrences
                .iter()
                .filter(|occurrence| occurrence.produces_default)
                .map(|occurrence| default_producer_name(occurrence.kind))
                .collect::<Vec<_>>()
                .join(",");
            notices.push(LocatedIdentity {
                path: module.normalized_path.clone(),
                start: 0,
                identity: format!(
                    "unsupported-duplicate-default-export {} producers={producers}",
                    module.normalized_path
                ),
            });
            continue;
        }
        for block in &module.blocks {
            let occurrence = module.occurrences.first().ok_or_else(|| {
                ProjectInventoryError::new("blocked default export lost its occurrence")
            })?;
            let line_index = crate::span::LineIndex::new(&owner.source);
            let location = source_location(owner, &line_index, occurrence.declaration_span.start);
            let identity = match block {
                DefaultExportCandidateBlock::DefaultInterface => {
                    format!("unsupported-module-form default-interface {location}")
                }
                DefaultExportCandidateBlock::DirectNamedNamespaceMerge
                | DefaultExportCandidateBlock::IdentifierNamespaceProvenance => format!(
                    "unsupported-default-export-namespace-provenance {location} {}",
                    occurrence.lexical_name.as_deref().unwrap_or("<unknown>")
                ),
                DefaultExportCandidateBlock::LocalExportListDefault => {
                    format!("unsupported-module-form local-default-export {location}")
                }
                DefaultExportCandidateBlock::SourceDefaultReexport => {
                    let declaration = source_reexports
                        .declarations
                        .iter()
                        .find(|declaration| {
                            declaration.owner_module == original_owner
                                && declaration.owner_start == occurrence.declaration_span.start
                        })
                        .ok_or_else(|| {
                            ProjectInventoryError::new(
                                "source default re-export lost its resolver evidence",
                            )
                        })?;
                    let imported = occurrence.imported.as_ref().ok_or_else(|| {
                        ProjectInventoryError::new(
                            "source default re-export lost its imported identity",
                        )
                    })?;
                    format!(
                        "unsupported-source-reexport-default-slot {location} {} {}->{}",
                        declaration.module_specifier,
                        default_member_name(imported),
                        default_member_name(&occurrence.exported)
                    )
                }
                DefaultExportCandidateBlock::DuplicateDefault => continue,
            };
            notices.push(LocatedIdentity {
                path: module.normalized_path.clone(),
                start: occurrence.declaration_span.start,
                identity,
            });
        }
    }

    for declaration in &source_reexports.declarations {
        if !declaration
            .members
            .iter()
            .any(|member| member.imported == "default" || member.exported == "default")
        {
            continue;
        }
        let Some(owner) = inputs.get(declaration.owner_module) else {
            continue;
        };
        let line_index = crate::span::LineIndex::new(&owner.source);
        let location = source_location(owner, &line_index, declaration.owner_start);
        for member in &declaration.members {
            let stale = format!(
                "unsupported-source-reexport-namespace-provenance {location} {} {}",
                declaration.module_specifier, member.exported
            );
            inventory.notices.retain(|notice| notice != &stale);
        }
    }
    if let Some(cycle) = &candidates.first_cycle {
        let identity = format!("unsupported-module-cycle {}", cycle.join(" -> "));
        if !inventory.notices.contains(&identity) {
            notices.push(LocatedIdentity {
                path: cycle.first().cloned().unwrap_or_default(),
                start: 0,
                identity,
            });
        }
    }

    inventory.resolutions.extend(sorted_identities(resolutions));
    inventory.resolutions.sort();
    inventory.notices.extend(sorted_identities(notices));
    inventory.notices.sort();
    inventory.missing_module_locations.sort_by(|left, right| {
        (&left.file, left.summary_start).cmp(&(&right.file, right.summary_start))
    });
    Ok(())
}

fn scan_default_import_candidates(
    owner_module: usize,
    program: &Program<'_>,
    resolution: &DefaultCandidateResolution<'_>,
) -> Result<Vec<RawDefaultImportCandidate>, ProjectInventoryError> {
    let mut imports = Vec::new();
    for statement in &program.body {
        let Statement::ImportDeclaration(import) = statement else {
            continue;
        };
        if import.phase.is_some() || import.with_clause.is_some() {
            continue;
        }
        let Some(_) = &import.specifiers else {
            continue;
        };
        let DefaultImportShape {
            specifiers,
            direct_default_count,
            named_default_count,
            named_count,
            namespace_count,
        } = default_import_shape(import);
        if direct_default_count == 0 && named_default_count == 0 {
            continue;
        }
        let disposition = if direct_default_count == 1
            && named_default_count == 0
            && named_count == 0
            && namespace_count == 0
            && specifiers.len() == 1
        {
            DefaultImportCandidateDisposition::Admitted
        } else if direct_default_count > 0 && namespace_count > 0 {
            DefaultImportCandidateDisposition::MixedDefaultNamespace
        } else if direct_default_count > 0 {
            DefaultImportCandidateDisposition::MixedDefaultNamed
        } else {
            DefaultImportCandidateDisposition::NamedDefaultSyntax
        };
        let module_specifier = import.source.value.as_str();
        let source = if is_admitted_source_reexport_specifier(module_specifier) {
            match resolve_named_import(
                resolution.mode,
                resolution.resolver,
                resolution.importer_path,
                module_specifier,
                resolution.explicit_path_to_index,
                resolution.configured_roots,
                resolution.canonical_project,
            )? {
                NamedImportOutcome::Source(RawImportSource::Resolved(target)) => {
                    RawDefaultImportCandidateSource::Resolved(target)
                }
                NamedImportOutcome::Source(RawImportSource::Missing) => {
                    RawDefaultImportCandidateSource::Missing
                }
                NamedImportOutcome::UnsupportedTarget(target) => {
                    RawDefaultImportCandidateSource::UnsupportedTarget(target)
                }
            }
        } else {
            RawDefaultImportCandidateSource::UnsupportedTarget(None)
        };
        imports.push(RawDefaultImportCandidate {
            owner_module,
            source,
            module_specifier: module_specifier.to_owned(),
            declaration_span: Span::from_oxc(import.span),
            source_span: Span::from_oxc(import.source.span),
            owner_start: import.span.start,
            declaration_type_only: import.import_kind == ImportOrExportKind::Type,
            disposition,
            specifiers,
        });
    }
    Ok(imports)
}

fn default_import_shape(import: &oxc_ast::ast::ImportDeclaration<'_>) -> DefaultImportShape {
    let outer_type_only = import.import_kind == ImportOrExportKind::Type;
    let mut specifiers = Vec::new();
    let mut direct_default_count = 0usize;
    let mut named_default_count = 0usize;
    let mut named_count = 0usize;
    let mut namespace_count = 0usize;
    for specifier in import.specifiers.iter().flatten() {
        match specifier {
            ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                direct_default_count = direct_default_count.saturating_add(1);
                specifiers.push(DefaultImportCandidateSpecifier {
                    identity: ModuleMemberIdentity::Default,
                    syntax: DefaultImportSpecifierSyntax::DirectDefault,
                    local: default.local.name.to_string(),
                    type_only: outer_type_only,
                    inline_type_only: false,
                    local_span: Span::from_oxc(default.local.span),
                    span: Span::from_oxc(default.span),
                });
            }
            ImportDeclarationSpecifier::ImportSpecifier(named) => {
                let (identity, syntax) = match module_export_name(&named.imported) {
                    Some("default") => {
                        named_default_count = named_default_count.saturating_add(1);
                        (
                            ModuleMemberIdentity::Default,
                            DefaultImportSpecifierSyntax::NamedDefault,
                        )
                    }
                    Some(name) => {
                        named_count = named_count.saturating_add(1);
                        (
                            ModuleMemberIdentity::Named(name.to_owned()),
                            DefaultImportSpecifierSyntax::Named,
                        )
                    }
                    None => {
                        named_count = named_count.saturating_add(1);
                        (
                            ModuleMemberIdentity::Named(named.imported.to_string()),
                            DefaultImportSpecifierSyntax::Named,
                        )
                    }
                };
                specifiers.push(DefaultImportCandidateSpecifier {
                    identity,
                    syntax,
                    local: named.local.name.to_string(),
                    type_only: outer_type_only || named.import_kind == ImportOrExportKind::Type,
                    inline_type_only: named.import_kind == ImportOrExportKind::Type,
                    local_span: Span::from_oxc(named.local.span),
                    span: Span::from_oxc(named.span),
                });
            }
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                namespace_count = namespace_count.saturating_add(1);
                specifiers.push(DefaultImportCandidateSpecifier {
                    identity: ModuleMemberIdentity::Namespace,
                    syntax: DefaultImportSpecifierSyntax::Namespace,
                    local: namespace.local.name.to_string(),
                    type_only: outer_type_only,
                    inline_type_only: false,
                    local_span: Span::from_oxc(namespace.local.span),
                    span: Span::from_oxc(namespace.span),
                });
            }
        }
    }
    DefaultImportShape {
        specifiers,
        direct_default_count,
        named_default_count,
        named_count,
        namespace_count,
    }
}

fn scan_default_export_candidates(
    owner_module: usize,
    owner_source_key: SourceUnitKey,
    normalized_owner_path: &str,
    program: &Program<'_>,
) -> RawDefaultExportCandidateModule {
    let local_names = default_candidate_local_namespace_provenance(program);
    let mut occurrences = Vec::new();
    for statement in &program.body {
        match statement {
            Statement::ExportDefaultDeclaration(export) => {
                let declaration_span = Span::from_oxc(export.span);
                let id = default_export_occurrence_id(
                    owner_source_key,
                    normalized_owner_path,
                    declaration_span.start,
                    occurrences.len(),
                );
                let occurrence = match &export.declaration {
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                        let lexical_name = class.id.as_ref().map(|id| id.name.to_string());
                        let provenance = lexical_name.as_ref().map(|name| {
                            local_names
                                .get(name)
                                .copied()
                                .unwrap_or(NamespaceProvenance::PresentOrUnknown)
                        });
                        DefaultExportOccurrence {
                            id,
                            kind: DefaultExportOccurrenceKind::DirectClass,
                            declaration_span,
                            subject_span: Span::from_oxc(class.span),
                            lexical_name,
                            lexical_name_span: class.id.as_ref().map(|id| Span::from_oxc(id.span)),
                            declaration_anonymous: Some(class.id.is_none()),
                            type_only: false,
                            identifier_namespace_provenance: provenance,
                            imported: None,
                            exported: ModuleMemberIdentity::Default,
                            produces_default: true,
                        }
                    }
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        let lexical_name = function.id.as_ref().map(|id| id.name.to_string());
                        let provenance = lexical_name.as_ref().map(|name| {
                            local_names
                                .get(name)
                                .copied()
                                .unwrap_or(NamespaceProvenance::PresentOrUnknown)
                        });
                        DefaultExportOccurrence {
                            id,
                            kind: DefaultExportOccurrenceKind::DirectFunction,
                            declaration_span,
                            subject_span: Span::from_oxc(function.span),
                            lexical_name,
                            lexical_name_span: function
                                .id
                                .as_ref()
                                .map(|id| Span::from_oxc(id.span)),
                            declaration_anonymous: Some(function.id.is_none()),
                            type_only: false,
                            identifier_namespace_provenance: provenance,
                            imported: None,
                            exported: ModuleMemberIdentity::Default,
                            produces_default: true,
                        }
                    }
                    ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                        DefaultExportOccurrence {
                            id,
                            kind: DefaultExportOccurrenceKind::DefaultInterface,
                            declaration_span,
                            subject_span: Span::from_oxc(interface.span),
                            lexical_name: Some(interface.id.name.to_string()),
                            lexical_name_span: Some(Span::from_oxc(interface.id.span)),
                            declaration_anonymous: Some(false),
                            type_only: true,
                            identifier_namespace_provenance: Some(
                                NamespaceProvenance::ProvenAbsent,
                            ),
                            imported: None,
                            exported: ModuleMemberIdentity::Default,
                            produces_default: true,
                        }
                    }
                    expression => {
                        let (lexical_name, lexical_name_span, provenance) =
                            match expression.as_expression() {
                                Some(oxc_ast::ast::Expression::Identifier(identifier)) => {
                                    let name = identifier.name.to_string();
                                    let provenance = local_names
                                        .get(&name)
                                        .copied()
                                        .unwrap_or(NamespaceProvenance::PresentOrUnknown);
                                    (
                                        Some(name),
                                        Some(Span::from_oxc(identifier.span)),
                                        Some(provenance),
                                    )
                                }
                                _ => (None, None, None),
                            };
                        DefaultExportOccurrence {
                            id,
                            kind: DefaultExportOccurrenceKind::DefaultExpression,
                            declaration_span,
                            subject_span: Span::from_oxc(expression.span()),
                            lexical_name,
                            lexical_name_span,
                            declaration_anonymous: None,
                            type_only: false,
                            identifier_namespace_provenance: provenance,
                            imported: None,
                            exported: ModuleMemberIdentity::Default,
                            produces_default: true,
                        }
                    }
                };
                occurrences.push(occurrence);
            }
            Statement::ExportNamedDeclaration(export) => {
                for specifier in &export.specifiers {
                    let imported = member_identity(&specifier.local);
                    let exported = member_identity(&specifier.exported);
                    let involves_default = imported == ModuleMemberIdentity::Default
                        || exported == ModuleMemberIdentity::Default;
                    if !involves_default {
                        continue;
                    }
                    let type_only = export.export_kind == ImportOrExportKind::Type
                        || specifier.export_kind == ImportOrExportKind::Type;
                    let lexical_name = if export.source.is_none() {
                        member_identity_name(&imported).map(str::to_owned)
                    } else {
                        None
                    };
                    let kind = if export.source.is_none() {
                        DefaultExportOccurrenceKind::LocalExportListDefault
                    } else if exported == ModuleMemberIdentity::Default {
                        DefaultExportOccurrenceKind::SourceExportToDefault
                    } else {
                        DefaultExportOccurrenceKind::SourceDefaultReexport
                    };
                    occurrences.push(DefaultExportOccurrence {
                        id: default_export_occurrence_id(
                            owner_source_key,
                            normalized_owner_path,
                            export.span.start,
                            occurrences.len(),
                        ),
                        kind,
                        declaration_span: Span::from_oxc(export.span),
                        subject_span: Span::from_oxc(specifier.span),
                        lexical_name,
                        lexical_name_span: if export.source.is_none() {
                            Some(Span::from_oxc(specifier.local.span()))
                        } else {
                            None
                        },
                        declaration_anonymous: None,
                        type_only,
                        identifier_namespace_provenance: None,
                        imported: Some(imported),
                        produces_default: exported == ModuleMemberIdentity::Default,
                        exported,
                    });
                }
            }
            _ => {}
        }
    }
    let producer_count = occurrences
        .iter()
        .filter(|occurrence| occurrence.produces_default)
        .count();
    let blocks = default_export_blocks(&occurrences, producer_count);
    RawDefaultExportCandidateModule {
        owner_module,
        producer_count,
        occurrences,
        blocks,
    }
}

fn default_export_occurrence_id(
    owner_source_key: SourceUnitKey,
    normalized_owner_path: &str,
    declaration_start: u32,
    occurrence_index: usize,
) -> DefaultExportOccurrenceId {
    let occurrence_ordinal = u32::try_from(occurrence_index).unwrap_or(u32::MAX);
    DefaultExportOccurrenceId {
        owner_source_key,
        normalized_owner_path: normalized_owner_path.to_owned(),
        declaration_start,
        occurrence_ordinal,
    }
}

fn default_export_blocks(
    occurrences: &[DefaultExportOccurrence],
    producer_count: usize,
) -> Vec<DefaultExportCandidateBlock> {
    let mut blocks = Vec::new();
    for occurrence in occurrences {
        let block = match occurrence.kind {
            DefaultExportOccurrenceKind::DefaultInterface => {
                Some(DefaultExportCandidateBlock::DefaultInterface)
            }
            DefaultExportOccurrenceKind::DirectClass
            | DefaultExportOccurrenceKind::DirectFunction
                if occurrence.identifier_namespace_provenance
                    == Some(NamespaceProvenance::PresentOrUnknown) =>
            {
                Some(DefaultExportCandidateBlock::DirectNamedNamespaceMerge)
            }
            DefaultExportOccurrenceKind::DefaultExpression
                if occurrence.lexical_name.is_some()
                    && occurrence.identifier_namespace_provenance
                        != Some(NamespaceProvenance::ProvenAbsent) =>
            {
                Some(DefaultExportCandidateBlock::IdentifierNamespaceProvenance)
            }
            DefaultExportOccurrenceKind::LocalExportListDefault => {
                Some(DefaultExportCandidateBlock::LocalExportListDefault)
            }
            DefaultExportOccurrenceKind::SourceExportToDefault
            | DefaultExportOccurrenceKind::SourceDefaultReexport => {
                Some(DefaultExportCandidateBlock::SourceDefaultReexport)
            }
            _ => None,
        };
        if let Some(block) = block {
            if !blocks.contains(&block) {
                blocks.push(block);
            }
        }
    }
    if producer_count > 1 {
        blocks.push(DefaultExportCandidateBlock::DuplicateDefault);
    }
    blocks
}

fn default_candidate_local_namespace_provenance(
    program: &Program<'_>,
) -> BTreeMap<String, NamespaceProvenance> {
    let mut names = BTreeMap::new();
    for statement in &program.body {
        if let Some(declaration) = statement.as_declaration() {
            default_candidate_declaration_namespace_names(declaration, |name, provenance| {
                join_namespace(&mut names, name, provenance);
            });
        }
        match statement {
            Statement::ImportDeclaration(import) => {
                for specifier in import.specifiers.iter().flatten() {
                    let local = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(named) => &named.local,
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                            &default.local
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                            &namespace.local
                        }
                    };
                    join_namespace(
                        &mut names,
                        local.name.as_str(),
                        NamespaceProvenance::PresentOrUnknown,
                    );
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    default_candidate_declaration_namespace_names(
                        declaration,
                        |name, provenance| {
                            join_namespace(&mut names, name, provenance);
                        },
                    );
                }
            }
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    if let Some(id) = &class.id {
                        join_namespace(
                            &mut names,
                            id.name.as_str(),
                            NamespaceProvenance::ProvenAbsent,
                        );
                    }
                }
                ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                    if let Some(id) = &function.id {
                        join_namespace(
                            &mut names,
                            id.name.as_str(),
                            NamespaceProvenance::ProvenAbsent,
                        );
                    }
                }
                ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                    join_namespace(
                        &mut names,
                        interface.id.name.as_str(),
                        NamespaceProvenance::ProvenAbsent,
                    );
                }
                _ => {}
            },
            _ => {}
        }
    }
    names
}

fn member_identity(name: &ModuleExportName<'_>) -> ModuleMemberIdentity {
    match module_export_name(name) {
        Some("default") => ModuleMemberIdentity::Default,
        Some(name) => ModuleMemberIdentity::Named(name.to_owned()),
        None => ModuleMemberIdentity::Named(name.to_string()),
    }
}

fn member_identity_name(identity: &ModuleMemberIdentity) -> Option<&str> {
    match identity {
        ModuleMemberIdentity::Named(name) => Some(name),
        ModuleMemberIdentity::Default | ModuleMemberIdentity::Namespace => None,
    }
}

fn validate_raw_default_imports(
    program: &Program<'_>,
    imports: &[RawDefaultImportCandidate],
) -> Result<(), ProjectInventoryError> {
    let valid = imports.iter().all(|candidate| {
        let Some(import) = program.body.iter().find_map(|statement| {
            let Statement::ImportDeclaration(import) = statement else {
                return None;
            };
            (import.span.start == candidate.owner_start).then_some(import)
        }) else {
            return false;
        };
        let expected = default_import_shape(import).specifiers;
        candidate.owner_start == candidate.declaration_span.start
            && candidate.declaration_span == Span::from_oxc(import.span)
            && candidate.source_span == Span::from_oxc(import.source.span)
            && candidate.module_specifier == import.source.value.as_str()
            && candidate.declaration_type_only == (import.import_kind == ImportOrExportKind::Type)
            && candidate.specifiers == expected
            && candidate.source_span.start >= candidate.declaration_span.start
            && candidate.source_span.end <= candidate.declaration_span.end
            && !candidate.module_specifier.is_empty()
            && !candidate.specifiers.is_empty()
            && candidate.specifiers.iter().all(|specifier| {
                !specifier.local.is_empty()
                    && specifier.local_span.start >= specifier.span.start
                    && specifier.local_span.end <= specifier.span.end
                    && specifier.span.start >= candidate.declaration_span.start
                    && specifier.span.end <= candidate.declaration_span.end
                    && !matches!(
                        &specifier.identity,
                        ModuleMemberIdentity::Named(name) if name == "default"
                    )
                    && matches!(
                        (&specifier.identity, specifier.syntax),
                        (
                            ModuleMemberIdentity::Default,
                            DefaultImportSpecifierSyntax::DirectDefault
                                | DefaultImportSpecifierSyntax::NamedDefault
                        ) | (
                            ModuleMemberIdentity::Named(_),
                            DefaultImportSpecifierSyntax::Named
                        ) | (
                            ModuleMemberIdentity::Namespace,
                            DefaultImportSpecifierSyntax::Namespace
                        )
                    )
                    && specifier.type_only
                        == (candidate.declaration_type_only || specifier.inline_type_only)
                    && (!specifier.inline_type_only
                        || specifier.syntax == DefaultImportSpecifierSyntax::Named)
            })
    });
    if !valid {
        return Err(ProjectInventoryError::new(
            "default import AST evidence changed during collection",
        ));
    }
    Ok(())
}

fn validate_raw_default_exports(
    program: &Program<'_>,
    owner_source_key: SourceUnitKey,
    normalized_owner_path: &str,
    exports: &RawDefaultExportCandidateModule,
) -> Result<(), ProjectInventoryError> {
    let rescanned = scan_default_export_candidates(
        exports.owner_module,
        owner_source_key,
        normalized_owner_path,
        program,
    );
    if &rescanned != exports {
        return Err(ProjectInventoryError::new(
            "default export AST evidence changed during collection",
        ));
    }
    Ok(())
}

fn validate_default_module_candidates(
    inputs: &[FileInput],
    raw_imports: &[Vec<RawImport>],
    source_reexports: &PendingSourceReexports,
    authority: &DefaultModuleAuthority,
    candidates: &DefaultModuleCandidates,
) -> Result<(), ProjectInventoryError> {
    if authority
        .imports
        .iter()
        .any(|candidate| candidate.owner_module >= inputs.len())
        || authority
            .exports
            .iter()
            .any(|candidate| candidate.owner_module >= inputs.len())
    {
        return Err(ProjectInventoryError::new(
            "default-module authority escaped configured roots",
        ));
    }

    let mut admitted_default_edges = vec![BTreeSet::new(); inputs.len()];
    for import in &authority.imports {
        if import.disposition == DefaultImportCandidateDisposition::Admitted {
            if let RawDefaultImportCandidateSource::Resolved(target) = &import.source {
                let edges = admitted_default_edges
                    .get_mut(import.owner_module)
                    .ok_or_else(|| {
                        ProjectInventoryError::new(
                            "default import authority escaped configured roots",
                        )
                    })?;
                edges.insert(*target);
            }
        }
    }
    if admitted_default_edges != authority.admitted_default_edges {
        return Err(ProjectInventoryError::new(
            "default edge authority diverged from exact import evidence",
        ));
    }

    let graph = default_candidate_graph(inputs, raw_imports, source_reexports, authority)?;
    let expected_order = graph
        .order
        .iter()
        .map(|original| normalized_display_name(&inputs[*original].name))
        .collect::<Vec<_>>();
    let mut expected_edges = Vec::new();
    for (owner, targets) in graph.edges.iter().enumerate() {
        for target in targets {
            let owner_module = graph.ordered_index.get(owner).copied().ok_or_else(|| {
                ProjectInventoryError::new("dependency owner escaped validation order")
            })?;
            let target_module = graph.ordered_index.get(*target).copied().ok_or_else(|| {
                ProjectInventoryError::new("dependency target escaped validation order")
            })?;
            expected_edges.push(DefaultModuleDependencyEdge {
                owner_module,
                target_module,
            });
        }
    }
    expected_edges.sort();
    if candidates.dependency_order != expected_order
        || candidates.dependency_edges != expected_edges
        || candidates.first_cycle != graph.first_cycle
    {
        return Err(ProjectInventoryError::new(
            "default-module graph diverged from raw accounting evidence",
        ));
    }

    let mut expected_imports = authority.imports.iter().collect::<Vec<_>>();
    expected_imports.sort_by(|left, right| {
        (
            normalized_display_name(&inputs[left.owner_module].name),
            left.owner_start,
        )
            .cmp(&(
                normalized_display_name(&inputs[right.owner_module].name),
                right.owner_start,
            ))
    });
    let imports_match = candidates.imports.len() == expected_imports.len()
        && candidates
            .imports
            .iter()
            .zip(expected_imports)
            .all(|(candidate, raw)| {
                let source_matches = match (&candidate.source, &raw.source) {
                    (
                        DefaultImportCandidateSource::Resolved(actual),
                        RawDefaultImportCandidateSource::Resolved(original),
                    ) => graph
                        .ordered_index
                        .get(*original)
                        .is_some_and(|expected| actual == expected),
                    (
                        DefaultImportCandidateSource::Missing,
                        RawDefaultImportCandidateSource::Missing,
                    ) => true,
                    (
                        DefaultImportCandidateSource::UnsupportedTarget(actual),
                        RawDefaultImportCandidateSource::UnsupportedTarget(original),
                    ) => actual == original,
                    _ => false,
                };
                source_matches
                    && graph
                        .ordered_index
                        .get(raw.owner_module)
                        .is_some_and(|owner| candidate.owner_module == *owner)
                    && candidate.module_specifier == raw.module_specifier
                    && candidate.declaration_span == raw.declaration_span
                    && candidate.source_span == raw.source_span
                    && candidate.owner_start == raw.owner_start
                    && candidate.declaration_type_only == raw.declaration_type_only
                    && candidate.disposition == raw.disposition
                    && candidate.specifiers == raw.specifiers
            });
    if !imports_match {
        return Err(ProjectInventoryError::new(
            "default imports diverged from exact AST and resolver evidence",
        ));
    }

    let mut expected_exports = authority.exports.iter().collect::<Vec<_>>();
    expected_exports
        .sort_by_key(|candidate| normalized_display_name(&inputs[candidate.owner_module].name));
    let exports_match = candidates.exports.len() == expected_exports.len()
        && candidates
            .exports
            .iter()
            .zip(expected_exports)
            .all(|(candidate, raw)| {
                let normalized_path = normalized_display_name(&inputs[raw.owner_module].name);
                graph
                    .ordered_index
                    .get(raw.owner_module)
                    .is_some_and(|owner| candidate.owner_module == *owner)
                    && candidate.normalized_path == normalized_path
                    && candidate.producer_count == raw.producer_count
                    && candidate.occurrences == raw.occurrences
                    && candidate.blocks == raw.blocks
            });
    if !exports_match {
        return Err(ProjectInventoryError::new(
            "default exports diverged from exact AST evidence",
        ));
    }
    Ok(())
}

struct FinalizedSourceReexports {
    admitted: Vec<AdmittedSourceReexportDeclaration>,
    order: Vec<usize>,
    resolutions: Vec<LocatedIdentity>,
    blocked_notices: Vec<LocatedIdentity>,
    first_cycle: Option<Vec<String>>,
}

fn finalize_admitted_source_reexports(
    inputs: &[FileInput],
    paths: &[PathBuf],
    raw_imports: &[Vec<RawImport>],
    source_reexports: &PendingSourceReexports,
) -> Result<FinalizedSourceReexports, ProjectInventoryError> {
    let order = dependency_order_with_reexports(raw_imports, &source_reexports.dependency_edges);
    let source_keys = stable_source_keys(paths);
    let mut ordered_index = vec![0usize; inputs.len()];
    for (position, original) in order.iter().copied().enumerate() {
        let Some(slot) = ordered_index.get_mut(original) else {
            return Err(ProjectInventoryError::new(
                "source re-export order escaped configured roots",
            ));
        };
        *slot = position;
    }

    let mut declaration_ordinals = BTreeMap::<usize, u32>::new();
    let mut admitted = Vec::new();
    let mut resolutions = Vec::new();
    let mut blocked_notices = Vec::new();
    let cycle_blocks_product = source_reexports.first_cycle.is_some();
    for declaration in &source_reexports.declarations {
        let ordinal = declaration_ordinals
            .entry(declaration.owner_module)
            .or_default();
        let owner = inputs.get(declaration.owner_module).ok_or_else(|| {
            ProjectInventoryError::new("source re-export owner escaped configured roots")
        })?;
        let id = SourceReexportDeclarationId {
            owner_source_key: source_keys
                .get(declaration.owner_module)
                .copied()
                .ok_or_else(|| {
                    ProjectInventoryError::new(
                        "source re-export owner lost its stable source identity",
                    )
                })?,
            normalized_owner_path: normalized_display_name(&owner.name),
            declaration_start: declaration.owner_start,
            source_ordinal: *ordinal,
        };
        *ordinal = ordinal.checked_add(1).ok_or_else(|| {
            ProjectInventoryError::new("source re-export declaration identity overflow")
        })?;
        let line_index = crate::span::LineIndex::new(&owner.source);
        let source = match declaration.source {
            RawImportSource::Missing => {
                resolutions.push(LocatedIdentity {
                    path: normalized_display_name(&owner.name),
                    start: declaration.owner_start,
                    identity: format!(
                        "{} source-reexport {} -> unresolved",
                        source_location(owner, &line_index, declaration.owner_start),
                        declaration.module_specifier
                    ),
                });
                AdmittedSourceReexportSource::Missing
            }
            RawImportSource::Resolved(target) => {
                let target_name = inputs
                    .get(target)
                    .map_or("<invalid-root>", |input| input.name.as_str());
                resolutions.push(LocatedIdentity {
                    path: normalized_display_name(&owner.name),
                    start: declaration.owner_start,
                    identity: format!(
                        "{} source-reexport {} -> {}",
                        source_location(owner, &line_index, declaration.owner_start),
                        declaration.module_specifier,
                        normalized_display_name(target_name)
                    ),
                });
                let mut blocked = false;
                if !cycle_blocks_product {
                    for member in &declaration.members {
                        let provenance = source_reexports
                            .namespace_provenance
                            .get(&(target, member.imported.clone()))
                            .copied()
                            .unwrap_or(NamespaceProvenance::PresentOrUnknown);
                        if provenance == NamespaceProvenance::PresentOrUnknown {
                            blocked = true;
                            blocked_notices.push(LocatedIdentity {
                                path: normalized_display_name(&owner.name),
                                start: declaration.owner_start,
                                identity: format!(
                                    "unsupported-source-reexport-namespace-provenance {} {} {}",
                                    source_location(owner, &line_index, declaration.owner_start),
                                    declaration.module_specifier,
                                    member.exported
                                ),
                            });
                        }
                    }
                }
                if blocked {
                    continue;
                }
                let Some(target) = ordered_index.get(target).copied() else {
                    return Err(ProjectInventoryError::new(
                        "source re-export target escaped dependency order",
                    ));
                };
                AdmittedSourceReexportSource::Resolved(target)
            }
        };
        if cycle_blocks_product {
            continue;
        }
        let owner_module = ordered_index
            .get(declaration.owner_module)
            .copied()
            .ok_or_else(|| {
                ProjectInventoryError::new("source re-export owner escaped dependency order")
            })?;
        let members = declaration
            .members
            .iter()
            .map(|member| AdmittedSourceReexportMember {
                imported: member.imported.clone(),
                exported: member.exported.clone(),
                span: member.span,
                type_only: member.type_only,
                namespace_provenance: match source {
                    AdmittedSourceReexportSource::Resolved(_) => {
                        Some(NamespaceProvenance::ProvenAbsent)
                    }
                    AdmittedSourceReexportSource::Missing => None,
                },
            })
            .collect();
        admitted.push((
            normalized_display_name(&owner.name),
            AdmittedSourceReexportDeclaration {
                id,
                owner_module,
                source,
                module_specifier: declaration.module_specifier.clone(),
                declaration_span: declaration.declaration_span,
                source_span: declaration.source_span,
                owner_start: declaration.owner_start,
                members,
            },
        ));
    }

    admitted
        .sort_by(|left, right| (&left.0, left.1.owner_start).cmp(&(&right.0, right.1.owner_start)));
    let admitted = admitted
        .into_iter()
        .map(|(_, declaration)| declaration)
        .collect();
    Ok(FinalizedSourceReexports {
        admitted,
        order,
        resolutions,
        blocked_notices,
        first_cycle: source_reexports.first_cycle.clone(),
    })
}

fn is_admitted_source_reexport_specifier(specifier: &str) -> bool {
    is_local_relative(specifier) && !specifier.contains(['?', '#']) && !specifier.ends_with(".ts")
}

fn source_reexport_namespace_provenance(
    programs: &[&Program<'_>],
    raw_imports: &[Vec<RawImport>],
    source_reexports: &[RawSourceReexportDeclaration],
    reexport_edges: &[BTreeSet<usize>],
    has_cycle: bool,
) -> BTreeMap<(usize, String), NamespaceProvenance> {
    let requested = source_reexports
        .iter()
        .flat_map(|declaration| {
            declaration.members.iter().filter_map(move |member| {
                let RawImportSource::Resolved(target) = declaration.source else {
                    return None;
                };
                Some((target, member.imported.clone()))
            })
        })
        .collect::<Vec<_>>();
    let globally_open = has_cycle
        || programs.iter().any(|program| {
            program.body.iter().any(|statement| {
                matches!(
                    statement,
                    Statement::TSModuleDeclaration(declaration)
                        if matches!(declaration.id, TSModuleDeclarationName::StringLiteral(_))
                ) || matches!(
                    statement,
                    Statement::ExportNamedDeclaration(export)
                        if matches!(
                            export.declaration,
                            Some(Declaration::TSModuleDeclaration(ref declaration))
                                if matches!(
                                    declaration.id,
                                    TSModuleDeclarationName::StringLiteral(_)
                                )
                        )
                )
            })
        });
    if globally_open {
        return requested
            .into_iter()
            .map(|key| (key, NamespaceProvenance::PresentOrUnknown))
            .collect();
    }

    let order = dependency_order_with_reexports(raw_imports, reexport_edges);
    let mut module_exports = vec![BTreeMap::<String, NamespaceProvenance>::new(); programs.len()];
    let mut module_complete = vec![true; programs.len()];
    for module in order {
        let Some(program) = programs.get(module) else {
            continue;
        };
        let mut locals = BTreeMap::new();
        for import in raw_imports.get(module).into_iter().flatten() {
            let provenance = namespace_from_dependency(
                import.source,
                &import.imported,
                &module_exports,
                &module_complete,
            );
            join_namespace(&mut locals, &import.local, provenance);
        }
        for statement in &program.body {
            if let Some(declaration) = statement.as_declaration() {
                declaration_namespace_names(declaration, |name, provenance| {
                    join_namespace(&mut locals, name, provenance);
                });
            }
            if let Statement::ExportNamedDeclaration(export) = statement {
                if let Some(declaration) = &export.declaration {
                    declaration_namespace_names(declaration, |name, provenance| {
                        join_namespace(&mut locals, name, provenance);
                    });
                }
            }
        }

        let mut exports = BTreeMap::new();
        for statement in &program.body {
            match statement {
                Statement::ExportNamedDeclaration(export) => {
                    if let Some(declaration) = &export.declaration {
                        let enumerable = declaration_namespace_names(declaration, |name, _| {
                            let provenance = locals
                                .get(name)
                                .copied()
                                .unwrap_or(NamespaceProvenance::PresentOrUnknown);
                            join_namespace(&mut exports, name, provenance);
                        });
                        module_complete[module] &= enumerable;
                    } else if export.source.is_none() {
                        for specifier in &export.specifiers {
                            let (Some(local), Some(exported)) = (
                                module_export_name(&specifier.local),
                                module_export_name(&specifier.exported),
                            ) else {
                                module_complete[module] = false;
                                continue;
                            };
                            let provenance = locals
                                .get(local)
                                .copied()
                                .unwrap_or(NamespaceProvenance::PresentOrUnknown);
                            join_namespace(&mut exports, exported, provenance);
                        }
                    } else if !export.specifiers.is_empty()
                        && !source_reexports.iter().any(|declaration| {
                            declaration.owner_module == module
                                && declaration.owner_start == export.span.start
                        })
                    {
                        module_complete[module] = false;
                    }
                }
                Statement::ExportAllDeclaration(_)
                | Statement::ExportDefaultDeclaration(_)
                | Statement::TSExportAssignment(_)
                | Statement::TSNamespaceExportDeclaration(_) => {
                    module_complete[module] = false;
                }
                _ => {}
            }
        }
        for declaration in source_reexports
            .iter()
            .filter(|declaration| declaration.owner_module == module)
        {
            for member in &declaration.members {
                let provenance = namespace_from_dependency(
                    declaration.source,
                    &member.imported,
                    &module_exports,
                    &module_complete,
                );
                join_namespace(&mut exports, &member.exported, provenance);
            }
        }
        if !module_complete[module] {
            for provenance in exports.values_mut() {
                *provenance = NamespaceProvenance::PresentOrUnknown;
            }
        }
        module_exports[module] = exports;
    }

    requested
        .into_iter()
        .map(|(module, name)| {
            let provenance = module_exports
                .get(module)
                .and_then(|exports| exports.get(&name))
                .copied()
                .unwrap_or_else(|| {
                    if module_complete.get(module).copied().unwrap_or(false) {
                        NamespaceProvenance::ProvenAbsent
                    } else {
                        NamespaceProvenance::PresentOrUnknown
                    }
                });
            ((module, name), provenance)
        })
        .collect()
}

fn namespace_from_dependency(
    source: RawImportSource,
    name: &str,
    module_exports: &[BTreeMap<String, NamespaceProvenance>],
    module_complete: &[bool],
) -> NamespaceProvenance {
    let RawImportSource::Resolved(target) = source else {
        return NamespaceProvenance::PresentOrUnknown;
    };
    module_exports
        .get(target)
        .and_then(|exports| exports.get(name))
        .copied()
        .unwrap_or_else(|| {
            if module_complete.get(target).copied().unwrap_or(false) {
                NamespaceProvenance::ProvenAbsent
            } else {
                NamespaceProvenance::PresentOrUnknown
            }
        })
}

fn join_namespace(
    names: &mut BTreeMap<String, NamespaceProvenance>,
    name: &str,
    incoming: NamespaceProvenance,
) {
    names
        .entry(name.to_owned())
        .and_modify(|current| {
            if incoming == NamespaceProvenance::PresentOrUnknown {
                *current = NamespaceProvenance::PresentOrUnknown;
            }
        })
        .or_insert(incoming);
}

fn declaration_namespace_names(
    declaration: &Declaration<'_>,
    mut record: impl FnMut(&str, NamespaceProvenance),
) -> bool {
    match declaration {
        Declaration::VariableDeclaration(declaration) => {
            let mut enumerable = true;
            for declarator in &declaration.declarations {
                if let BindingPattern::BindingIdentifier(identifier) = &declarator.id {
                    record(identifier.name.as_str(), NamespaceProvenance::ProvenAbsent);
                } else {
                    enumerable = false;
                }
            }
            enumerable
        }
        Declaration::FunctionDeclaration(declaration) => {
            declaration.id.as_ref().is_some_and(|id| {
                record(id.name.as_str(), NamespaceProvenance::ProvenAbsent);
                true
            })
        }
        Declaration::ClassDeclaration(declaration) => declaration.id.as_ref().is_some_and(|id| {
            record(id.name.as_str(), NamespaceProvenance::ProvenAbsent);
            true
        }),
        Declaration::TSTypeAliasDeclaration(declaration) => {
            record(
                declaration.id.name.as_str(),
                NamespaceProvenance::ProvenAbsent,
            );
            true
        }
        Declaration::TSInterfaceDeclaration(declaration) => {
            record(
                declaration.id.name.as_str(),
                NamespaceProvenance::ProvenAbsent,
            );
            true
        }
        Declaration::TSEnumDeclaration(declaration) => {
            record(
                declaration.id.name.as_str(),
                NamespaceProvenance::ProvenAbsent,
            );
            true
        }
        Declaration::TSModuleDeclaration(declaration) => {
            let TSModuleDeclarationName::Identifier(identifier) = &declaration.id else {
                return false;
            };
            record(
                identifier.name.as_str(),
                NamespaceProvenance::PresentOrUnknown,
            );
            true
        }
        Declaration::TSGlobalDeclaration(_) | Declaration::TSImportEqualsDeclaration(_) => false,
    }
}

fn default_candidate_declaration_namespace_names(
    declaration: &Declaration<'_>,
    mut record: impl FnMut(&str, NamespaceProvenance),
) -> bool {
    if let Declaration::TSEnumDeclaration(declaration) = declaration {
        record(
            declaration.id.name.as_str(),
            NamespaceProvenance::PresentOrUnknown,
        );
        return true;
    }
    declaration_namespace_names(declaration, record)
}

fn classify_import_declaration(
    import: &oxc_ast::ast::ImportDeclaration<'_>,
) -> Result<&'static str, ProjectInventoryError> {
    let Some(specifiers) = &import.specifiers else {
        return Ok("side-effect-import");
    };
    if specifiers.is_empty() {
        return Ok("empty-named-import");
    }
    if specifiers.iter().any(|specifier| {
        matches!(
            specifier,
            ImportDeclarationSpecifier::ImportDefaultSpecifier(_)
        )
    }) {
        return Ok("default-import");
    }
    if specifiers.iter().any(|specifier| {
        matches!(
            specifier,
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(_)
        )
    }) {
        return Ok("namespace-import");
    }
    if specifiers
        .iter()
        .all(|specifier| matches!(specifier, ImportDeclarationSpecifier::ImportSpecifier(_)))
    {
        return Ok("named-import");
    }
    Err(ProjectInventoryError::new(
        "import declaration has an unclassified specifier shape",
    ))
}

fn resolution_paths(
    inputs: &[FileInput],
    mode: &ProjectResolutionMode,
) -> Result<ResolutionPaths, ProjectInventoryError> {
    match mode {
        ProjectResolutionMode::ExplicitFileList => Ok(ResolutionPaths {
            paths: normalized_input_paths(inputs),
            configured_roots: BTreeMap::new(),
            canonical_project: None,
        }),
        ProjectResolutionMode::BundlerProject {
            project_directory,
            roots,
        } => {
            if roots.len() != inputs.len() {
                return Err(ProjectInventoryError::new(
                    "configured root/input coverage changed before module inventory",
                ));
            }
            let mut configured = BTreeMap::new();
            let mut paths = Vec::with_capacity(roots.len());
            for (index, (root, input)) in roots.iter().zip(inputs).enumerate() {
                if root.identity != input.name || !root.exists {
                    return Err(ProjectInventoryError::new(format!(
                        "configured root/input identity changed for {}",
                        root.identity
                    )));
                }
                if root
                    .path
                    .extension()
                    .is_none_or(|extension| extension != "ts")
                    || root.identity.ends_with(".d.ts")
                {
                    return Err(ProjectInventoryError::new(format!(
                        "configured root escaped the admitted .ts surface: {}",
                        root.identity
                    )));
                }
                let canonical = fs::canonicalize(&root.path).map_err(|error| {
                    ProjectInventoryError::new(format!(
                        "configured root {} became unreadable: {error}",
                        root.identity
                    ))
                })?;
                if configured.insert(canonical.clone(), index).is_some() {
                    return Err(ProjectInventoryError::new(
                        "configured roots collapse to one canonical file",
                    ));
                }
                paths.push(canonical);
            }
            let canonical_project = fs::canonicalize(project_directory).map_err(|error| {
                ProjectInventoryError::new(format!(
                    "project directory became unreadable before resolution: {error}"
                ))
            })?;
            Ok(ResolutionPaths {
                paths,
                configured_roots: configured,
                canonical_project: Some(canonical_project),
            })
        }
    }
}

fn resolve_named_import(
    mode: &ProjectResolutionMode,
    resolver: &Resolver,
    importer_path: &Path,
    specifier: &str,
    explicit_path_to_index: &BTreeMap<PathBuf, usize>,
    configured_roots: &BTreeMap<PathBuf, usize>,
    canonical_project: Option<&Path>,
) -> Result<NamedImportOutcome, ProjectInventoryError> {
    if !is_local_relative(specifier) {
        return Err(ProjectInventoryError::new(format!(
            "unsupported named-import specifier must be classified before resolution: {specifier}"
        )));
    }
    if specifier.contains(['?', '#']) || specifier.ends_with(".ts") {
        return Err(ProjectInventoryError::new(format!(
            "specifier is outside the frozen Bundler oracle: {specifier}"
        )));
    }
    match mode {
        ProjectResolutionMode::ExplicitFileList => Ok(NamedImportOutcome::Source(
            resolve_local_import(importer_path, specifier)
                .and_then(|path| explicit_path_to_index.get(&path).copied())
                .map_or(RawImportSource::Missing, RawImportSource::Resolved),
        )),
        ProjectResolutionMode::BundlerProject { .. } => {
            match resolver.resolve_dts(importer_path, specifier) {
                Ok(resolution) => {
                    let canonical = fs::canonicalize(resolution.path()).map_err(|error| {
                        ProjectInventoryError::new(format!(
                            "resolved target {} cannot be canonicalized: {error}",
                            resolution.path().display()
                        ))
                    })?;
                    if let Some(target) = configured_roots.get(&canonical).copied() {
                        Ok(NamedImportOutcome::Source(RawImportSource::Resolved(
                            target,
                        )))
                    } else {
                        let target = canonical_project
                            .and_then(|project| canonical.strip_prefix(project).ok())
                            .map(|relative| relative.to_string_lossy().replace('\\', "/"));
                        Ok(NamedImportOutcome::UnsupportedTarget(target))
                    }
                }
                Err(ResolveError::NotFound(missing)) if missing == specifier => {
                    Ok(NamedImportOutcome::Source(RawImportSource::Missing))
                }
                Err(error) => Err(ProjectInventoryError::new(format!(
                    "Bundler resolver failed for {specifier}: {error}"
                ))),
            }
        }
    }
}

fn record_unsupported_module_form(
    input: &FileInput,
    line_index: &crate::span::LineIndex,
    start: u32,
    form: &str,
    specifier: Option<&str>,
    resolutions: &mut Vec<LocatedIdentity>,
    notices: &mut Vec<LocatedIdentity>,
) {
    let suffix = specifier.map_or_else(String::new, |value| format!(" {value}"));
    if let Some(specifier) = specifier {
        resolutions.push(located_identity(
            input,
            line_index,
            start,
            format!("{form} {specifier} -> unsupported"),
        ));
    }
    notices.push(LocatedIdentity {
        path: normalized_display_name(&input.name),
        start,
        identity: format!(
            "unsupported-module-form {form} {}{suffix}",
            source_location(input, line_index, start)
        ),
    });
}

fn record_unsupported_module_specifier(
    input: &FileInput,
    line_index: &crate::span::LineIndex,
    start: u32,
    specifier: &str,
    reason: &str,
    resolutions: &mut Vec<LocatedIdentity>,
    notices: &mut Vec<LocatedIdentity>,
) {
    resolutions.push(located_identity(
        input,
        line_index,
        start,
        format!("named-import {specifier} -> unsupported"),
    ));
    notices.push(LocatedIdentity {
        path: normalized_display_name(&input.name),
        start,
        identity: format!(
            "unsupported-module-specifier {reason} {} {specifier}",
            source_location(input, line_index, start)
        ),
    });
}

fn located_identity(
    input: &FileInput,
    line_index: &crate::span::LineIndex,
    start: u32,
    tail: String,
) -> LocatedIdentity {
    let path = normalized_display_name(&input.name);
    LocatedIdentity {
        path: path.clone(),
        start,
        identity: format!("{} {tail}", source_location(input, line_index, start)),
    }
}

fn source_location(input: &FileInput, line_index: &crate::span::LineIndex, start: u32) -> String {
    let position = line_index.line_col(start);
    format!(
        "{}:{}:{}",
        normalized_display_name(&input.name),
        position.line,
        position.column
    )
}

fn normalized_display_name(name: &str) -> String {
    name.replace('\\', "/")
}

fn sorted_identities(mut identities: Vec<LocatedIdentity>) -> Vec<String> {
    identities.sort_by(|left, right| {
        (&left.path, left.start, &left.identity).cmp(&(&right.path, right.start, &right.identity))
    });
    identities
        .into_iter()
        .map(|identity| identity.identity)
        .collect()
}

fn first_module_cycle(
    inputs: &[FileInput],
    cycle_edges: &[BTreeSet<usize>],
) -> Option<Vec<String>> {
    fn visit_cycle(
        index: usize,
        inputs: &[FileInput],
        edges: &[Vec<usize>],
        state: &mut [VisitState],
        stack: &mut Vec<usize>,
    ) -> Option<Vec<String>> {
        match state.get(index).copied()? {
            VisitState::Done => return None,
            VisitState::Visiting => {
                let start = stack.iter().position(|candidate| *candidate == index)?;
                let mut cycle = stack[start..]
                    .iter()
                    .map(|item| normalized_display_name(&inputs[*item].name))
                    .collect::<Vec<_>>();
                cycle.push(normalized_display_name(&inputs[index].name));
                return Some(cycle);
            }
            VisitState::Unseen => {}
        }
        state[index] = VisitState::Visiting;
        stack.push(index);
        for target in &edges[index] {
            if let Some(cycle) = visit_cycle(*target, inputs, edges, state, stack) {
                return Some(cycle);
            }
        }
        stack.pop();
        state[index] = VisitState::Done;
        None
    }

    let mut edges = cycle_edges
        .iter()
        .map(|targets| targets.iter().copied().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    for module_edges in &mut edges {
        module_edges.sort_by_key(|index| normalized_display_name(&inputs[*index].name));
    }
    let mut roots = (0..inputs.len()).collect::<Vec<_>>();
    roots.sort_by_key(|index| normalized_display_name(&inputs[*index].name));
    let mut state = vec![VisitState::Unseen; inputs.len()];
    let mut stack = Vec::new();
    for root in roots {
        if let Some(cycle) = visit_cycle(root, inputs, &edges, &mut state, &mut stack) {
            return Some(cycle);
        }
    }
    None
}

fn stable_dependency_order(inputs: &[FileInput], edges: &[BTreeSet<usize>]) -> Vec<usize> {
    fn visit_stable(
        index: usize,
        inputs: &[FileInput],
        edges: &[BTreeSet<usize>],
        state: &mut [VisitState],
        order: &mut Vec<usize>,
    ) {
        match state.get(index).copied() {
            Some(VisitState::Done | VisitState::Visiting) | None => return,
            Some(VisitState::Unseen) => {}
        }
        state[index] = VisitState::Visiting;
        let mut targets = edges
            .get(index)
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        targets.sort_by_key(|target| normalized_display_name(&inputs[*target].name));
        for target in targets {
            visit_stable(target, inputs, edges, state, order);
        }
        state[index] = VisitState::Done;
        order.push(index);
    }

    let mut roots = (0..inputs.len()).collect::<Vec<_>>();
    roots.sort_by_key(|index| normalized_display_name(&inputs[*index].name));
    let mut state = vec![VisitState::Unseen; inputs.len()];
    let mut order = Vec::with_capacity(inputs.len());
    for root in roots {
        visit_stable(root, inputs, edges, &mut state, &mut order);
    }
    order
}

#[derive(Clone)]
struct RawImport {
    local: String,
    imported: String,
    specifier: String,
    source: RawImportSource,
    type_only: bool,
    local_span: Span,
    span: Span,
    owner_start: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RawImportSource {
    Resolved(usize),
    Missing,
}

enum NamedImportOutcome {
    Source(RawImportSource),
    UnsupportedTarget(Option<String>),
}

fn scan_imports(
    program: &Program<'_>,
    importer_path: &Path,
    path_to_index: &BTreeMap<PathBuf, usize>,
) -> Vec<RawImport> {
    let mut imports = Vec::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else {
            continue;
        };
        let specifier = import.source.value.as_str().to_string();
        if !is_local_relative(&specifier) {
            continue;
        }
        let source = resolve_local_import(importer_path, &specifier)
            .and_then(|path| path_to_index.get(&path).copied())
            .map(RawImportSource::Resolved)
            .unwrap_or(RawImportSource::Missing);
        let outer_type_only = import.import_kind == ImportOrExportKind::Type;
        if let Some(specifiers) = &import.specifiers {
            for spec in specifiers {
                let ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                    continue;
                };
                let Some(imported) = module_export_name(&named.imported) else {
                    continue;
                };
                let type_only = outer_type_only || named.import_kind == ImportOrExportKind::Type;
                imports.push(RawImport {
                    local: named.local.name.to_string(),
                    imported: imported.to_string(),
                    specifier: specifier.clone(),
                    source,
                    type_only,
                    local_span: Span::from_oxc(named.local.span),
                    span: Span::from_oxc(named.span),
                    owner_start: import.span.start,
                });
            }
        }
    }
    imports
}

/// Count source-level relative import declarations without performing resolution.
pub fn relative_import_edge_count(program: &Program<'_>) -> usize {
    program
        .body
        .iter()
        .filter(|statement| {
            matches!(
                statement,
                Statement::ImportDeclaration(import)
                    if is_local_relative(import.source.value.as_str())
            )
        })
        .count()
}

fn module_export_name<'ast>(name: &'ast ModuleExportName<'ast>) -> Option<&'ast str> {
    match name {
        ModuleExportName::IdentifierName(id) => Some(id.name.as_str()),
        ModuleExportName::IdentifierReference(id) => Some(id.name.as_str()),
        ModuleExportName::StringLiteral(_) => None,
    }
}

fn dependency_order_with_reexports(
    imports: &[Vec<RawImport>],
    reexport_edges: &[BTreeSet<usize>],
) -> Vec<usize> {
    let mut state = vec![VisitState::Unseen; imports.len()];
    let mut order = Vec::with_capacity(imports.len());
    for index in 0..imports.len() {
        visit(index, imports, reexport_edges, &mut state, &mut order);
    }
    order
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unseen,
    Visiting,
    Done,
}

fn visit(
    index: usize,
    imports: &[Vec<RawImport>],
    reexport_edges: &[BTreeSet<usize>],
    state: &mut [VisitState],
    order: &mut Vec<usize>,
) {
    match state.get(index).copied() {
        Some(VisitState::Done | VisitState::Visiting) | None => return,
        Some(VisitState::Unseen) => {}
    }
    if let Some(slot) = state.get_mut(index) {
        *slot = VisitState::Visiting;
    }
    if let Some(module_imports) = imports.get(index) {
        for import in module_imports {
            if let RawImportSource::Resolved(dep) = import.source {
                visit(dep, imports, reexport_edges, state, order);
            }
        }
    }
    if let Some(targets) = reexport_edges.get(index) {
        for target in targets {
            visit(*target, imports, reexport_edges, state, order);
        }
    }
    if let Some(slot) = state.get_mut(index) {
        *slot = VisitState::Done;
    }
    order.push(index);
}

fn normalized_input_paths(inputs: &[FileInput]) -> Vec<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    inputs
        .iter()
        .map(|input| {
            let path = Path::new(&input.name);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            normalize_path(&absolute)
        })
        .collect()
}

fn stable_source_keys(paths: &[PathBuf]) -> Vec<SourceUnitKey> {
    let mut ranked: Vec<_> = paths.iter().enumerate().collect();
    ranked.sort_by_key(|(_, path)| (*path).clone());
    let mut keys = vec![SourceUnitKey::SINGLE_SOURCE; paths.len()];
    for (rank, (original, _)) in ranked.into_iter().enumerate() {
        if let Some(key) = keys.get_mut(original) {
            *key =
                SourceUnitKey(u32::try_from(rank + 1).expect("project source path rank fits u32"));
        }
    }
    keys
}

fn resolve_local_import(importer_path: &Path, specifier: &str) -> Option<PathBuf> {
    if !is_local_relative(specifier) {
        return None;
    }
    let base = importer_path.parent()?;
    let mut path = normalize_path(&base.join(specifier));
    if path.extension().is_none() {
        path.set_extension("ts");
    }
    Some(path)
}

fn is_local_relative(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../")
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod auxiliary_parse_work_tests {
    use super::*;

    #[test]
    fn duplicate_exact_source_parse_changes_invocation_and_reparse_receipts() {
        let allocator = Allocator::default();
        let source = AuxiliarySourceInput {
            source_ordinal: 7,
            name: "lib.same.d.ts".to_owned(),
            source: "interface Same {}".to_owned(),
        };
        let distinct_name = AuxiliarySourceInput {
            source_ordinal: 7,
            name: "lib.distinct.d.ts".to_owned(),
            source: "interface Distinct {}".to_owned(),
        };
        let mut invocations = BTreeMap::new();

        let _first = parse_auxiliary_source(&allocator, &source, &mut invocations);
        let _reparse = parse_auxiliary_source(&allocator, &source, &mut invocations);
        let _distinct = parse_auxiliary_source(&allocator, &distinct_name, &mut invocations);

        assert_eq!(
            auxiliary_parse_work(&invocations),
            AuxiliaryParseWork {
                parser_invocations: 3,
                source_reparses: 1,
            }
        );
    }
}

#[cfg(test)]
mod source_reexport_collection_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);

    struct TestProject {
        directory: PathBuf,
    }

    impl TestProject {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let sequence = NEXT_PROJECT.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "typokat-frontend-source-reexports-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&directory)?;
            Ok(Self { directory })
        }

        fn write(&self, name: &str, source: &str) -> Result<PathBuf, std::io::Error> {
            let path = self.directory.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, source)?;
            Ok(path)
        }

        fn collect(
            files: &[(&str, &str)],
        ) -> Result<SourceReexportCollectionForTest, Box<dyn std::error::Error>> {
            let physical = files
                .iter()
                .map(|(name, source)| (*name, *name, *source))
                .collect::<Vec<_>>();
            Self::collect_physical(&physical)
        }

        fn collect_physical(
            files: &[(&str, &str, &str)],
        ) -> Result<SourceReexportCollectionForTest, Box<dyn std::error::Error>> {
            let project = Self::new()?;
            let mut inputs = Vec::new();
            let mut roots = Vec::new();
            for (identity, physical_name, source) in files {
                let path = project.write(physical_name, source)?;
                inputs.push(FileInput {
                    name: (*identity).to_owned(),
                    source: (*source).to_owned(),
                });
                roots.push(ProjectRoot {
                    identity: (*identity).to_owned(),
                    path,
                    exists: true,
                });
            }
            let result = collect_admitted_source_reexports_for_test(
                inputs,
                ProjectResolutionMode::BundlerProject {
                    project_directory: project.directory.clone(),
                    roots,
                },
            )?;
            drop(project);
            Ok(result)
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn source_slice(source: &str, span: Span) -> Option<&str> {
        let start = usize::try_from(span.start).ok()?;
        let end = usize::try_from(span.end).ok()?;
        source.get(start..end)
    }

    #[test]
    fn test_hook_invalidates_only_one_resolved_provenance() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut collection = TestProject::collect(&[
            (
                "barrel.ts",
                "export { first } from \"./source.js\";\nexport { second } from \"./source.js\";\n",
            ),
            (
                "source.ts",
                "export const first = 1;\nexport const second = 2;\n",
            ),
        ])?;
        let mut expected = collection.admitted.clone();
        let expected_member = expected
            .declarations
            .first_mut()
            .and_then(|declaration| declaration.members.first_mut())
            .ok_or_else(|| std::io::Error::other("resolved test member was not collected"))?;
        expected_member.namespace_provenance = Some(NamespaceProvenance::PresentOrUnknown);
        let untouched = expected
            .declarations
            .get(1)
            .and_then(|declaration| declaration.members.first())
            .ok_or_else(|| {
                std::io::Error::other("second resolved test member was not collected")
            })?;
        assert_eq!(
            untouched.namespace_provenance,
            Some(NamespaceProvenance::ProvenAbsent)
        );
        let dependency_order = collection.dependency_order.clone();
        let dependency_edge_count = collection.dependency_edge_count;
        let resolutions = collection.resolutions.clone();
        let blocked_notices = collection.blocked_notices.clone();
        let first_cycle = collection.first_cycle.clone();

        collection.invalidate_first_resolved_provenance_for_test()?;

        assert_eq!(collection.admitted, expected);
        assert_eq!(collection.dependency_order, dependency_order);
        assert_eq!(collection.dependency_edge_count, dependency_edge_count);
        assert_eq!(collection.resolutions, resolutions);
        assert_eq!(collection.blocked_notices, blocked_notices);
        assert_eq!(collection.first_cycle, first_cycle);

        let mut missing =
            TestProject::collect(&[("barrel.ts", "export { value } from \"./missing.js\";\n")])?;
        let missing_admitted = missing.admitted.clone();
        let missing_dependency_order = missing.dependency_order.clone();
        let missing_dependency_edge_count = missing.dependency_edge_count;
        let missing_resolutions = missing.resolutions.clone();
        let missing_blocked_notices = missing.blocked_notices.clone();
        let missing_first_cycle = missing.first_cycle.clone();
        assert!(missing
            .invalidate_first_resolved_provenance_for_test()
            .is_err());
        assert_eq!(missing.admitted, missing_admitted);
        assert_eq!(missing.dependency_order, missing_dependency_order);
        assert_eq!(missing.dependency_edge_count, missing_dependency_edge_count);
        assert_eq!(missing.resolutions, missing_resolutions);
        assert_eq!(missing.blocked_notices, missing_blocked_notices);
        assert_eq!(missing.first_cycle, missing_first_cycle);
        Ok(())
    }

    #[test]
    fn retains_alias_type_only_spans_and_stable_declaration_identity(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = "export const value = 1;\nexport interface Shape {}\n";
        let barrel = "export { value as renamed, type Shape as InlineShape } from \"./source.js\";\nexport type { Shape as OuterShape } from \"./source.js\";\n";
        let normal = TestProject::collect(&[("barrel.ts", barrel), ("source.ts", source)])?;
        let reverse = TestProject::collect(&[("source.ts", source), ("barrel.ts", barrel)])?;

        assert_eq!(normal.admitted(), reverse.admitted());
        assert_eq!(normal.dependency_order(), ["source.ts", "barrel.ts"]);
        assert_eq!(normal.dependency_edge_count(), 2);
        assert_eq!(normal.admitted().declarations().len(), 2);
        let first = &normal.admitted().declarations()[0];
        let second = &normal.admitted().declarations()[1];
        assert_eq!(first.id().normalized_owner_path(), "barrel.ts");
        assert_eq!(first.id().source_ordinal(), 0);
        assert_eq!(second.id().source_ordinal(), 1);
        assert_eq!(first.owner_start(), 0);
        assert_eq!(
            source_slice(barrel, first.source_span()),
            Some("\"./source.js\"")
        );
        assert_eq!(
            source_slice(barrel, first.declaration_span()),
            barrel.lines().next()
        );
        assert_eq!(first.members().len(), 2);
        assert_eq!(first.members()[0].imported(), "value");
        assert_eq!(first.members()[0].exported(), "renamed");
        assert!(!first.members()[0].is_type_only());
        assert_eq!(first.members()[1].imported(), "Shape");
        assert_eq!(first.members()[1].exported(), "InlineShape");
        assert!(first.members()[1].is_type_only());
        assert!(second.members()[0].is_type_only());
        assert_eq!(
            first.members()[0].namespace_provenance(),
            Some(NamespaceProvenance::ProvenAbsent)
        );
        assert_eq!(
            source_slice(barrel, first.members()[0].span()),
            Some("value as renamed")
        );

        let left = "export { value } from \"./left-missing.js\";\n";
        let right = "export { value } from \"./right-missing.js\";\n";
        let duplicate_names = TestProject::collect_physical(&[
            ("same.ts", "left/same.ts", left),
            ("same.ts", "right/same.ts", right),
        ])?;
        let duplicate_names_reversed = TestProject::collect_physical(&[
            ("same.ts", "right/same.ts", right),
            ("same.ts", "left/same.ts", left),
        ])?;
        let ids_by_specifier = |collection: &SourceReexportCollectionForTest| {
            collection
                .admitted()
                .declarations()
                .iter()
                .map(|declaration| {
                    (
                        declaration.module_specifier().to_owned(),
                        declaration.id().clone(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        let ids = ids_by_specifier(&duplicate_names);
        assert_eq!(ids, ids_by_specifier(&duplicate_names_reversed));
        assert_ne!(ids.get("./left-missing.js"), ids.get("./right-missing.js"));

        #[cfg(unix)]
        {
            let backslash = "export { value } from \"./backslash-missing.js\";\n";
            let slash = "export { value } from \"./slash-missing.js\";\n";
            let display_collision = TestProject::collect_physical(&[
                ("a\\b.ts", "a\\b.ts", backslash),
                ("a/b.ts", "a/b.ts", slash),
            ])?;
            let declarations = display_collision.admitted().declarations();
            assert_eq!(declarations.len(), 2);
            assert_eq!(
                declarations[0].id().normalized_owner_path(),
                declarations[1].id().normalized_owner_path()
            );
            assert_ne!(
                declarations[0].id().owner_source_key(),
                declarations[1].id().owner_source_key()
            );
        }
        Ok(())
    }

    #[test]
    fn closed_absence_missing_and_empty_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
        let closed = TestProject::collect(&[
            ("barrel.ts", "export { absent } from \"./source.js\";\n"),
            ("source.ts", "export const other = 1;\n"),
        ])?;
        assert_eq!(closed.admitted().declarations().len(), 1);
        assert_eq!(
            closed.admitted().declarations()[0].members()[0].namespace_provenance(),
            Some(NamespaceProvenance::ProvenAbsent)
        );

        let barrel = "export { alpha, beta as renamed } from \"./missing.js\";\n";
        let result = TestProject::collect(&[("barrel.ts", barrel)])?;

        assert_eq!(result.dependency_edge_count(), 0);
        assert_eq!(result.resolutions().len(), 1);
        assert!(result.resolutions()[0].ends_with("source-reexport ./missing.js -> unresolved"));
        let declarations = result.admitted().declarations();
        assert_eq!(declarations.len(), 1);
        assert_eq!(
            declarations[0].source(),
            AdmittedSourceReexportSource::Missing
        );
        assert_eq!(declarations[0].members().len(), 2);
        assert!(declarations[0]
            .members()
            .iter()
            .all(|member| member.namespace_provenance().is_none()));

        let source = "export const present = 1;\n";
        let barrel = "export {} from \"./source.js\";\nexport type {} from \"./missing.js\";\n";
        let empty = TestProject::collect(&[("barrel.ts", barrel), ("source.ts", source)])?;

        assert!(empty.admitted().is_empty());
        assert!(empty.resolutions().is_empty());
        assert_eq!(empty.dependency_edge_count(), 0);
        assert!(empty.first_cycle().is_none());

        let malformed = TestProject::new()?;
        let barrel_source = "export { value } from \"./pkg\";\n";
        let barrel_path = malformed.write("barrel.ts", barrel_source)?;
        malformed.write("pkg/package.json", "{")?;
        let inputs = vec![FileInput {
            name: "barrel.ts".to_owned(),
            source: barrel_source.to_owned(),
        }];
        let mode = ProjectResolutionMode::BundlerProject {
            project_directory: malformed.directory.clone(),
            roots: vec![ProjectRoot {
                identity: "barrel.ts".to_owned(),
                path: barrel_path,
                exists: true,
            }],
        };
        let frozen = run_clean_project_frontend_with_deferred_auxiliary(
            inputs.clone(),
            mode.clone(),
            || Ok::<_, std::convert::Infallible>(Vec::new()),
            |_, _, _, _, _| (),
        );
        let accounted = match frozen.product {
            Ok(accounted) => accounted,
            Err(_) => {
                return Err(std::io::Error::other(
                    "frozen source re-export route invoked the resolver",
                )
                .into());
            }
        };
        assert_eq!(
            accounted.inventory.resolutions,
            ["barrel.ts:1:1 source-reexport ./pkg -> unsupported"]
        );
        assert_eq!(
            accounted.inventory.notices,
            ["unsupported-module-form source-reexport barrel.ts:1:1 ./pkg"]
        );
        assert!(accounted.product.is_none());
        let evidence_error = match collect_admitted_source_reexports_for_test(inputs, mode) {
            Ok(_) => {
                return Err(std::io::Error::other(
                    "evidence collector accepted malformed package metadata",
                )
                .into());
            }
            Err(error) => error,
        };
        assert!(evidence_error
            .to_string()
            .contains("Bundler resolver failed for ./pkg"));
        Ok(())
    }

    #[test]
    fn admits_only_proven_namespace_absence_and_propagates_unknown_chains(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = "export class Plain {}\nexport class Merged {}\nexport namespace Merged {}\nfunction callable() {}\nnamespace callable {}\nexport { callable };\n";
        let first = "export { Plain } from \"./source.js\";\nexport { Merged } from \"./source.js\";\nexport { callable as First } from \"./source.js\";\n";
        let second = "export { First as Final } from \"./first.js\";\nexport { absent } from \"./source.js\";\n";
        let result = TestProject::collect(&[
            ("second.ts", second),
            ("first.ts", first),
            ("source.ts", source),
        ])?;

        let admitted = result.admitted().declarations();
        assert_eq!(admitted.len(), 2);
        assert_eq!(admitted[0].members()[0].exported(), "Plain");
        assert_eq!(admitted[1].members()[0].exported(), "absent");
        assert_eq!(result.blocked_notices().len(), 3);
        assert!(result
            .blocked_notices()
            .iter()
            .any(|notice| notice.ends_with("./source.js Merged")));
        assert!(result
            .blocked_notices()
            .iter()
            .any(|notice| notice.ends_with("./source.js First")));
        assert!(result
            .blocked_notices()
            .iter()
            .any(|notice| notice.ends_with("./first.js Final")));

        let augmented = TestProject::collect(&[
            ("barrel.ts", "export { C } from \"./source.js\";\n"),
            ("source.ts", "export class C {}\n"),
            (
                "augment.ts",
                "export {};\ndeclare module \"./source\" { namespace C {} }\n",
            ),
        ])?;
        assert!(augmented.admitted().is_empty());
        assert_eq!(augmented.blocked_notices().len(), 1);
        Ok(())
    }

    #[test]
    fn combined_graph_accounts_for_reexport_only_and_mixed_cycles(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chain = TestProject::collect(&[
            ("b.ts", "export { middle as final } from \"./a.js\";\n"),
            ("source.ts", "export const value = 1;\n"),
            ("a.ts", "export { value as middle } from \"./source.js\";\n"),
        ])?;
        assert_eq!(chain.dependency_order(), ["source.ts", "a.ts", "b.ts"]);
        assert_eq!(chain.dependency_edge_count(), 2);

        let reexport_cycle = TestProject::collect(&[
            (
                "a.ts",
                "export const x = 1;\nexport { y } from \"./b.js\";\n",
            ),
            ("b.ts", "export { x as y } from \"./a.js\";\n"),
        ])?;
        let cycle = reexport_cycle.first_cycle().map(|items| items.join(" -> "));
        assert_eq!(cycle.as_deref(), Some("a.ts -> b.ts -> a.ts"));
        assert_eq!(reexport_cycle.dependency_edge_count(), 2);
        assert!(reexport_cycle.admitted().is_empty());

        let mixed_cycle = TestProject::collect(&[
            (
                "a.ts",
                "import { b } from \"./b.js\";\nexport const a = b;\n",
            ),
            ("b.ts", "export { a as b } from \"./a.js\";\n"),
        ])?;
        let cycle = mixed_cycle.first_cycle().map(|items| items.join(" -> "));
        assert_eq!(cycle.as_deref(), Some("a.ts -> b.ts -> a.ts"));
        assert!(mixed_cycle.admitted().is_empty());
        Ok(())
    }
}

#[cfg(test)]
mod default_module_candidate_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);

    struct TestProject {
        directory: PathBuf,
    }

    impl TestProject {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let sequence = NEXT_PROJECT.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "typokat-frontend-default-modules-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&directory)?;
            Ok(Self { directory })
        }

        fn collect(
            files: &[(&str, &str)],
        ) -> Result<DefaultModuleCandidates, Box<dyn std::error::Error>> {
            Self::collect_with_mutation(files, |_| Ok(()))
        }

        fn collect_with_mutation(
            files: &[(&str, &str)],
            mutate: impl FnOnce(&mut DefaultModuleCandidates) -> Result<(), ProjectInventoryError>,
        ) -> Result<DefaultModuleCandidates, Box<dyn std::error::Error>> {
            let project = Self::new()?;
            let mut inputs = Vec::new();
            let mut roots = Vec::new();
            for (name, source) in files {
                let path = project.directory.join(name);
                fs::write(&path, source)?;
                inputs.push(FileInput {
                    name: (*name).to_owned(),
                    source: (*source).to_owned(),
                });
                roots.push(ProjectRoot {
                    identity: (*name).to_owned(),
                    path,
                    exists: true,
                });
            }
            let result = collect_default_module_candidates_with_mutation(
                inputs,
                ProjectResolutionMode::BundlerProject {
                    project_directory: project.directory.clone(),
                    roots,
                },
                mutate,
            )?;
            drop(project);
            Ok(result)
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn source_slice(source: &str, span: Span) -> Option<&str> {
        let start = usize::try_from(span.start).ok()?;
        let end = usize::try_from(span.end).ok()?;
        source.get(start..end)
    }

    fn assert_candidate_rejected(
        result: Result<DefaultModuleCandidates, Box<dyn std::error::Error>>,
        expected: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match result {
            Ok(_) => Err(std::io::Error::other("candidate mutation passed validation").into()),
            Err(error) => {
                assert_eq!(error.to_string(), expected);
                Ok(())
            }
        }
    }

    #[test]
    fn retains_every_admitted_producer_and_default_import_in_both_root_orders(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cases = [
            (
                "export default class Named {}\n",
                DefaultExportOccurrenceKind::DirectClass,
                Some(false),
                Some("Named"),
            ),
            (
                "export default class {}\n",
                DefaultExportOccurrenceKind::DirectClass,
                Some(true),
                None,
            ),
            (
                "export default function named() {}\n",
                DefaultExportOccurrenceKind::DirectFunction,
                Some(false),
                Some("named"),
            ),
            (
                "export default function() {}\n",
                DefaultExportOccurrenceKind::DirectFunction,
                Some(true),
                None,
            ),
            (
                "export default 1;\n",
                DefaultExportOccurrenceKind::DefaultExpression,
                None,
                None,
            ),
            (
                "export default { value: 1 };\n",
                DefaultExportOccurrenceKind::DefaultExpression,
                None,
                None,
            ),
            (
                "export default () => 1;\n",
                DefaultExportOccurrenceKind::DefaultExpression,
                None,
                None,
            ),
            (
                "const local = 1;\nexport default local;\n",
                DefaultExportOccurrenceKind::DefaultExpression,
                None,
                Some("local"),
            ),
            (
                "class Local {}\nexport default Local;\n",
                DefaultExportOccurrenceKind::DefaultExpression,
                None,
                Some("Local"),
            ),
            (
                "function local() {}\nexport default local;\n",
                DefaultExportOccurrenceKind::DefaultExpression,
                None,
                Some("local"),
            ),
        ];
        let consumer = "import Value from \"./source.js\";\nexport const retained = Value;\n";
        for (source, kind, anonymous, lexical_name) in cases {
            let normal = TestProject::collect(&[("consumer.ts", consumer), ("source.ts", source)])?;
            let reverse =
                TestProject::collect(&[("source.ts", source), ("consumer.ts", consumer)])?;
            assert_eq!(normal, reverse);
            assert_eq!(normal.dependency_order(), ["source.ts", "consumer.ts"]);
            assert_eq!(normal.dependency_edge_count(), 1);
            assert!(normal.first_cycle().is_none());

            let import = normal
                .imports()
                .first()
                .ok_or_else(|| std::io::Error::other("default import evidence was not retained"))?;
            assert_eq!(import.owner_module(), 1);
            assert_eq!(import.source(), &DefaultImportCandidateSource::Resolved(0));
            assert_eq!(
                import.disposition(),
                DefaultImportCandidateDisposition::Admitted
            );
            assert_eq!(import.specifiers().len(), 1);
            assert_eq!(
                import.specifiers()[0].identity(),
                &ModuleMemberIdentity::Default
            );
            assert_eq!(
                import.specifiers()[0].syntax(),
                DefaultImportSpecifierSyntax::DirectDefault
            );
            assert_eq!(import.specifiers()[0].local(), "Value");
            assert_eq!(
                source_slice(consumer, import.specifiers()[0].local_span()),
                Some("Value")
            );
            assert_eq!(
                source_slice(consumer, import.source_span()),
                Some("\"./source.js\"")
            );

            let module = normal
                .exports()
                .first()
                .ok_or_else(|| std::io::Error::other("default export evidence was not retained"))?;
            assert_eq!(module.owner_module(), 0);
            assert_eq!(module.normalized_path(), "source.ts");
            assert_eq!(module.producer_count(), 1);
            assert!(module.is_admitted());
            assert_eq!(module.occurrences().len(), 1);
            let occurrence = &module.occurrences()[0];
            assert_eq!(occurrence.id().normalized_owner_path(), "source.ts");
            assert_eq!(
                occurrence.id().declaration_start(),
                occurrence.declaration_span().start
            );
            assert_eq!(occurrence.id().occurrence_ordinal(), 0);
            assert_eq!(occurrence.kind(), kind);
            assert_eq!(occurrence.declaration_anonymous(), anonymous);
            assert_eq!(occurrence.lexical_name(), lexical_name);
            assert_eq!(occurrence.exported(), &ModuleMemberIdentity::Default);
            assert!(occurrence.produces_default());
            if lexical_name.is_some() {
                assert_eq!(
                    occurrence.identifier_namespace_provenance(),
                    Some(NamespaceProvenance::ProvenAbsent)
                );
            }
        }
        Ok(())
    }

    #[test]
    fn retains_type_only_and_every_deferred_import_specifier(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = "export default class Shape {}\nexport const named = 1;\nexport interface TypeOnly {}\n";
        let admitted = TestProject::collect(&[
            (
                "consumer.ts",
                "import type Shape from \"./source.js\";\nlet value: Shape;\n",
            ),
            ("source.ts", source),
        ])?;
        let specifier = &admitted.imports()[0].specifiers()[0];
        assert!(specifier.is_type_only());
        assert_eq!(
            source_slice(
                "import type Shape from \"./source.js\";\nlet value: Shape;\n",
                specifier.local_span()
            ),
            Some("Shape")
        );

        let extensionless = TestProject::collect(&[
            ("consumer.ts", "import Shape from \"./source\";\n"),
            ("source.ts", source),
        ])?;
        assert_eq!(
            extensionless.imports()[0].source(),
            &DefaultImportCandidateSource::Resolved(0)
        );

        let missing =
            TestProject::collect(&[("consumer.ts", "import Missing from \"./missing.js\";\n")])?;
        assert_eq!(
            missing.imports()[0].source(),
            &DefaultImportCandidateSource::Missing
        );
        assert_eq!(
            source_slice(
                "import Missing from \"./missing.js\";\n",
                missing.imports()[0].specifiers()[0].local_span()
            ),
            Some("Missing")
        );

        let unconfigured = TestProject::new()?;
        let consumer_path = unconfigured.directory.join("consumer.ts");
        fs::write(&consumer_path, "import Hidden from \"./hidden.js\";\n")?;
        fs::write(
            unconfigured.directory.join("hidden.ts"),
            "export default 1;\n",
        )?;
        let unsupported = collect_default_module_candidates(
            vec![FileInput {
                name: "consumer.ts".to_owned(),
                source: "import Hidden from \"./hidden.js\";\n".to_owned(),
            }],
            ProjectResolutionMode::BundlerProject {
                project_directory: unconfigured.directory.clone(),
                roots: vec![ProjectRoot {
                    identity: "consumer.ts".to_owned(),
                    path: consumer_path,
                    exists: true,
                }],
            },
        )?;
        assert_eq!(
            unsupported.imports()[0].source(),
            &DefaultImportCandidateSource::UnsupportedTarget(Some("hidden.ts".to_owned()))
        );

        let named_default = TestProject::collect(&[
            (
                "consumer.ts",
                "import { default as Alias } from \"./source.js\";\n",
            ),
            ("source.ts", source),
        ])?;
        assert_eq!(
            named_default.imports()[0].disposition(),
            DefaultImportCandidateDisposition::NamedDefaultSyntax
        );
        assert_eq!(
            named_default.imports()[0].specifiers()[0].syntax(),
            DefaultImportSpecifierSyntax::NamedDefault
        );
        assert_eq!(
            named_default.imports()[0].specifiers()[0].identity(),
            &ModuleMemberIdentity::Default
        );

        let mixed_named = TestProject::collect(&[
            (
                "consumer.ts",
                "import Value, { named, type TypeOnly } from \"./source.js\";\n",
            ),
            ("source.ts", source),
        ])?;
        let mixed_named_reverse = TestProject::collect(&[
            ("source.ts", source),
            (
                "consumer.ts",
                "import Value, { named, type TypeOnly } from \"./source.js\";\n",
            ),
        ])?;
        assert_eq!(mixed_named, mixed_named_reverse);
        let import = &mixed_named.imports()[0];
        assert_eq!(
            import.disposition(),
            DefaultImportCandidateDisposition::MixedDefaultNamed
        );
        assert_eq!(import.specifiers().len(), 3);
        assert_eq!(import.specifiers()[0].local(), "Value");
        assert_eq!(import.specifiers()[1].local(), "named");
        assert_eq!(import.specifiers()[2].local(), "TypeOnly");
        assert!(import.specifiers()[2].is_type_only());

        let mixed_namespace = TestProject::collect(&[
            (
                "consumer.ts",
                "import Value, * as all from \"./source.js\";\n",
            ),
            ("source.ts", source),
        ])?;
        let import = &mixed_namespace.imports()[0];
        assert_eq!(
            import.disposition(),
            DefaultImportCandidateDisposition::MixedDefaultNamespace
        );
        assert_eq!(import.specifiers().len(), 2);
        assert_eq!(
            import.specifiers()[1].identity(),
            &ModuleMemberIdentity::Namespace
        );
        Ok(())
    }

    #[test]
    fn excludes_every_member_of_a_deferred_import_from_cycle_evidence(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let a =
            "import { default as Deferred, named } from \"./b.js\";\nexport const fromA = named;\n";
        let b = "import FromA from \"./a.js\";\nexport default FromA;\nexport const named = 1;\n";
        let normal = TestProject::collect(&[("a.ts", a), ("b.ts", b)])?;
        let reverse = TestProject::collect(&[("b.ts", b), ("a.ts", a)])?;
        assert_eq!(normal, reverse);
        assert!(normal.first_cycle().is_none());
        assert_eq!(normal.dependency_edge_count(), 1);
        let edge = normal
            .dependency_edges()
            .first()
            .ok_or_else(|| std::io::Error::other("admitted opposite edge was not retained"))?;
        assert_eq!(normal.dependency_order()[edge.owner_module()], "b.ts");
        assert_eq!(normal.dependency_order()[edge.target_module()], "a.ts");
        let deferred = normal
            .imports()
            .iter()
            .find(|import| {
                import.disposition() == DefaultImportCandidateDisposition::NamedDefaultSyntax
            })
            .ok_or_else(|| std::io::Error::other("deferred import was not retained"))?;
        assert_eq!(deferred.specifiers().len(), 2);
        Ok(())
    }

    #[test]
    fn candidate_declarations_bypass_legacy_accounting_without_changing_frozen_mode(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let project = TestProject::new()?;
        let sources = [
            (
                "consumer.ts",
                "import { default as Alias, named } from \"./source.js\";\nimport Direct from \"./other.js\";\n",
            ),
            ("source.ts", "export default 1;\nexport const named = 1;\n"),
            ("other.ts", "export default 2;\n"),
        ];
        let mut inputs = Vec::new();
        let mut roots = Vec::new();
        for (name, source) in sources {
            let path = project.directory.join(name);
            fs::write(&path, source)?;
            inputs.push(FileInput {
                name: name.to_owned(),
                source: source.to_owned(),
            });
            roots.push(ProjectRoot {
                identity: name.to_owned(),
                path,
                exists: true,
            });
        }
        let mode = ProjectResolutionMode::BundlerProject {
            project_directory: project.directory.clone(),
            roots,
        };
        let allocators = inputs
            .iter()
            .map(|_| Allocator::default())
            .collect::<Vec<_>>();
        let parsed = inputs
            .iter()
            .zip(&allocators)
            .map(|(input, allocator)| {
                Parser::new(allocator, &input.source, SourceType::ts()).parse()
            })
            .collect::<Vec<_>>();
        let programs = parsed
            .iter()
            .map(|result| &result.program)
            .collect::<Vec<_>>();

        let frozen = account_project_modules(
            &inputs,
            &programs,
            &mode,
            SourceReexportAccounting::AdmitBundler,
            DefaultCandidateAccounting::FrozenUnsupported,
        )?;
        assert!(frozen.default_module_authority.is_none());
        assert_eq!(frozen.raw_imports[0].len(), 2);

        let candidate = account_project_modules(
            &inputs,
            &programs,
            &mode,
            SourceReexportAccounting::AdmitBundler,
            DefaultCandidateAccounting::CollectCandidate,
        )?;
        assert!(candidate.raw_imports[0].is_empty());
        let authority = candidate
            .default_module_authority
            .as_ref()
            .ok_or_else(|| std::io::Error::other("candidate authority was not retained"))?;
        assert_eq!(authority.imports.len(), 2);
        let product = finalize_default_module_candidates(&inputs, candidate)?;
        assert_eq!(product.imports().len(), 2);
        Ok(())
    }

    #[test]
    fn phase_and_attribute_imports_remain_legacy_fail_closed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let attributes_source = "import Value from \"./source.js\" with { type: \"json\" };\n";
        let attributes_input = FileInput {
            name: "consumer.ts".to_owned(),
            source: attributes_source.to_owned(),
        };
        let attributes_allocator = Allocator::default();
        let attributes_parsed =
            Parser::new(&attributes_allocator, attributes_source, SourceType::ts()).parse();
        assert!(attributes_parsed.diagnostics.is_empty());
        let attributes_programs = [&attributes_parsed.program];
        let frozen_attributes = account_project_modules(
            std::slice::from_ref(&attributes_input),
            &attributes_programs,
            &ProjectResolutionMode::ExplicitFileList,
            SourceReexportAccounting::AdmitBundler,
            DefaultCandidateAccounting::FrozenUnsupported,
        )?;
        let candidate_attributes = account_project_modules(
            std::slice::from_ref(&attributes_input),
            &attributes_programs,
            &ProjectResolutionMode::ExplicitFileList,
            SourceReexportAccounting::AdmitBundler,
            DefaultCandidateAccounting::CollectCandidate,
        )?;
        assert_eq!(frozen_attributes.inventory, candidate_attributes.inventory);
        assert_eq!(
            candidate_attributes.inventory.resolutions,
            ["consumer.ts:1:1 import-attributes ./source.js -> unsupported"]
        );
        assert_eq!(
            candidate_attributes.inventory.notices,
            ["unsupported-module-form import-attributes consumer.ts:1:1 ./source.js"]
        );
        assert!(candidate_attributes
            .default_module_authority
            .as_ref()
            .is_some_and(|authority| authority.imports.is_empty()));

        let phase_source = "import source Value from \"./source.js\";\n";
        let phase_input = FileInput {
            name: "consumer.ts".to_owned(),
            source: phase_source.to_owned(),
        };
        let phase_allocator = Allocator::default();
        let phase_parsed = Parser::new(&phase_allocator, phase_source, SourceType::ts()).parse();
        assert!(phase_parsed.diagnostics.is_empty());
        let phase_programs = [&phase_parsed.program];
        let frozen_phase = account_project_modules(
            std::slice::from_ref(&phase_input),
            &phase_programs,
            &ProjectResolutionMode::ExplicitFileList,
            SourceReexportAccounting::AdmitBundler,
            DefaultCandidateAccounting::FrozenUnsupported,
        );
        let candidate_phase = account_project_modules(
            std::slice::from_ref(&phase_input),
            &phase_programs,
            &ProjectResolutionMode::ExplicitFileList,
            SourceReexportAccounting::AdmitBundler,
            DefaultCandidateAccounting::CollectCandidate,
        );
        let exact_phase_error = "unfrozen import phase surface at consumer.ts:1:1";
        match (frozen_phase, candidate_phase) {
            (Err(frozen), Err(candidate)) => {
                assert_eq!(frozen.to_string(), exact_phase_error);
                assert_eq!(candidate.to_string(), exact_phase_error);
            }
            _ => {
                return Err(std::io::Error::other(
                    "phase import did not preserve its legacy fail-closed error",
                )
                .into());
            }
        }
        Ok(())
    }

    #[test]
    fn parsed_program_hook_rejects_truncation_and_source_mismatch(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let inputs = [
            FileInput {
                name: "a.ts".to_owned(),
                source: "export default 1;\n".to_owned(),
            },
            FileInput {
                name: "b.ts".to_owned(),
                source: "export default 2;\n".to_owned(),
            },
        ];
        let allocators = [Allocator::default(), Allocator::default()];
        let parsed = inputs
            .iter()
            .zip(&allocators)
            .map(|(input, allocator)| {
                Parser::new(allocator, &input.source, SourceType::ts()).parse()
            })
            .collect::<Vec<_>>();
        let programs = [&parsed[0].program, &parsed[1].program];
        let coverage_error = "default-module parsed-program coverage changed before accounting";

        let shorter = certify_default_module_candidates_from_parsed_programs(
            &inputs,
            &programs[..1],
            &ProjectResolutionMode::ExplicitFileList,
        );
        match shorter {
            Err(error) => assert_eq!(error.to_string(), coverage_error),
            Ok(_) => {
                return Err(std::io::Error::other("short parsed coverage was accepted").into());
            }
        }

        let longer = certify_default_module_candidates_from_parsed_programs(
            &inputs[..1],
            &programs,
            &ProjectResolutionMode::ExplicitFileList,
        );
        match longer {
            Err(error) => assert_eq!(error.to_string(), coverage_error),
            Ok(_) => {
                return Err(std::io::Error::other("long parsed coverage was accepted").into());
            }
        }

        let swapped = certify_default_module_candidates_from_parsed_programs(
            &inputs,
            &[programs[1], programs[0]],
            &ProjectResolutionMode::ExplicitFileList,
        );
        match swapped {
            Err(error) => assert_eq!(
                error.to_string(),
                "default-module parsed source changed for a.ts"
            ),
            Ok(_) => {
                return Err(std::io::Error::other("swapped parsed sources were accepted").into());
            }
        }
        Ok(())
    }

    #[test]
    fn classifies_namespace_bridges_and_duplicate_producers_fail_closed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = "export const named = 1;\n";
        let direct_merge = "export default class Merged {}\nnamespace Merged {}\n";
        let identifier_merge =
            "class Identifier {}\nnamespace Identifier {}\nexport default Identifier;\n";
        let local = "const local = 1;\nexport { local as default };\n";
        let interface = "export default interface Unsupported {}\n";
        let source_bridge = "export { named as default } from \"./source.js\";\nexport { default as alias } from \"./missing.js\";\n";
        let result = TestProject::collect(&[
            ("direct.ts", direct_merge),
            ("identifier.ts", identifier_merge),
            ("interface.ts", interface),
            ("local.ts", local),
            ("source.ts", source),
            ("bridge.ts", source_bridge),
        ])?;
        let reverse = TestProject::collect(&[
            ("bridge.ts", source_bridge),
            ("source.ts", source),
            ("local.ts", local),
            ("interface.ts", interface),
            ("identifier.ts", identifier_merge),
            ("direct.ts", direct_merge),
        ])?;
        assert_eq!(result, reverse);
        let module = |path: &str| {
            result
                .exports()
                .iter()
                .find(|module| module.normalized_path() == path)
        };
        assert!(module("direct.ts")
            .is_some_and(|module| module.blocks()
                == [DefaultExportCandidateBlock::DirectNamedNamespaceMerge]));
        assert!(module("identifier.ts").is_some_and(|module| module.blocks()
            == [DefaultExportCandidateBlock::IdentifierNamespaceProvenance]));
        assert!(module("interface.ts").is_some_and(
            |module| module.blocks() == [DefaultExportCandidateBlock::DefaultInterface]
        ));
        assert!(module("local.ts").is_some_and(
            |module| module.blocks() == [DefaultExportCandidateBlock::LocalExportListDefault]
        ));
        let bridge = module("bridge.ts").ok_or_else(|| {
            std::io::Error::other("source default bridge evidence was not retained")
        })?;
        assert_eq!(bridge.occurrences().len(), 2);
        assert_eq!(bridge.producer_count(), 1);
        assert_eq!(
            bridge.blocks(),
            [DefaultExportCandidateBlock::SourceDefaultReexport]
        );
        assert_eq!(
            bridge.occurrences()[0].kind(),
            DefaultExportOccurrenceKind::SourceExportToDefault
        );
        assert_eq!(
            bridge.occurrences()[1].kind(),
            DefaultExportOccurrenceKind::SourceDefaultReexport
        );

        let duplicate = TestProject::collect(&[
            (
                "duplicate.ts",
                "const local = 1;\nexport default class C {}\nexport default function f() {}\nexport default 1;\nexport { local as default };\nexport { named as default } from \"./source.js\";\n",
            ),
            ("source.ts", source),
        ])?;
        let duplicate = duplicate
            .exports()
            .iter()
            .find(|module| module.normalized_path() == "duplicate.ts")
            .ok_or_else(|| std::io::Error::other("duplicate evidence was not retained"))?;
        assert_eq!(duplicate.producer_count(), 5);
        assert_eq!(
            duplicate
                .occurrences()
                .iter()
                .map(DefaultExportOccurrence::kind)
                .collect::<Vec<_>>(),
            [
                DefaultExportOccurrenceKind::DirectClass,
                DefaultExportOccurrenceKind::DirectFunction,
                DefaultExportOccurrenceKind::DefaultExpression,
                DefaultExportOccurrenceKind::LocalExportListDefault,
                DefaultExportOccurrenceKind::SourceExportToDefault,
            ]
        );
        assert!(duplicate
            .blocks()
            .contains(&DefaultExportCandidateBlock::DuplicateDefault));
        Ok(())
    }

    #[test]
    fn corruption_controls_reject_every_certified_dimension(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = "export default class Value {}\nexport const named = 1;\nexport const other = 2;\nexport interface TypeOnly {}\n";
        let consumer = "import Value, { named, other, type TypeOnly } from \"./source.js\";\n";
        let admitted_files = [
            (
                "consumer.ts",
                "import Value from \"./source.js\";\nimport { other } from \"./other.js\";\nexport const retained = Value;\n",
            ),
            ("source.ts", "export default 1;\n"),
            ("other.ts", "export const other = 1;\n"),
        ];
        let invalid_target = TestProject::collect_with_mutation(&admitted_files, |candidate| {
            let import = candidate.imports.first().ok_or_else(|| {
                ProjectInventoryError::new("admitted target control lost its import")
            })?;
            let correct_target = match &import.source {
                DefaultImportCandidateSource::Resolved(target) => *target,
                DefaultImportCandidateSource::Missing
                | DefaultImportCandidateSource::UnsupportedTarget(_) => {
                    return Err(ProjectInventoryError::new(
                        "admitted target control lost its resolved target",
                    ));
                }
            };
            let wrong_target = candidate
                .dependency_edges
                .iter()
                .find(|edge| {
                    edge.owner_module == import.owner_module && edge.target_module != correct_target
                })
                .map(|edge| edge.target_module)
                .ok_or_else(|| {
                    ProjectInventoryError::new(
                        "target control needs an existing edge to a second target",
                    )
                })?;
            candidate.imports[0].source = DefaultImportCandidateSource::Resolved(wrong_target);
            Ok(())
        });
        assert_candidate_rejected(
            invalid_target,
            "default imports diverged from exact AST and resolver evidence",
        )?;

        let invalid_module_specifier =
            TestProject::collect_with_mutation(&admitted_files, |candidate| {
                candidate.imports[0].module_specifier = "./other.js".to_owned();
                Ok(())
            });
        assert_candidate_rejected(
            invalid_module_specifier,
            "default imports diverged from exact AST and resolver evidence",
        )?;

        let mixed_files = [("consumer.ts", consumer), ("source.ts", source)];
        let invalid_kind = TestProject::collect_with_mutation(&mixed_files, |candidate| {
            candidate.imports[0].specifiers[0].syntax = DefaultImportSpecifierSyntax::Named;
            Ok(())
        });
        assert_candidate_rejected(
            invalid_kind,
            "default imports diverged from exact AST and resolver evidence",
        )?;

        let invalid_type_only = TestProject::collect_with_mutation(&mixed_files, |candidate| {
            candidate.imports[0].specifiers[0].type_only = true;
            Ok(())
        });
        assert_candidate_rejected(
            invalid_type_only,
            "default imports diverged from exact AST and resolver evidence",
        )?;

        let invalid_inline_type_only =
            TestProject::collect_with_mutation(&mixed_files, |candidate| {
                candidate.imports[0].specifiers[3].inline_type_only = false;
                Ok(())
            });
        assert_candidate_rejected(
            invalid_inline_type_only,
            "default imports diverged from exact AST and resolver evidence",
        )?;

        let dropped_mixed_member = TestProject::collect_with_mutation(&mixed_files, |candidate| {
            candidate.imports[0].specifiers.remove(2);
            Ok(())
        });
        assert_candidate_rejected(
            dropped_mixed_member,
            "default imports diverged from exact AST and resolver evidence",
        )?;

        let named_default_map = TestProject::collect_with_mutation(&mixed_files, |candidate| {
            candidate.imports[0].specifiers[0].identity =
                ModuleMemberIdentity::Named("default".to_owned());
            Ok(())
        });
        assert_candidate_rejected(
            named_default_map,
            "default imports diverged from exact AST and resolver evidence",
        )?;

        let invalid_order = TestProject::collect_with_mutation(&mixed_files, |candidate| {
            candidate.dependency_order.reverse();
            Ok(())
        });
        assert_candidate_rejected(
            invalid_order,
            "default-module graph diverged from raw accounting evidence",
        )?;

        let invalid_edges = TestProject::collect_with_mutation(&admitted_files, |candidate| {
            candidate.dependency_edges.clear();
            Ok(())
        });
        assert_candidate_rejected(
            invalid_edges,
            "default-module graph diverged from raw accounting evidence",
        )?;

        let invalid_cycle = TestProject::collect_with_mutation(&mixed_files, |candidate| {
            candidate.first_cycle = Some(vec!["invented.ts".to_owned()]);
            Ok(())
        });
        assert_candidate_rejected(
            invalid_cycle,
            "default-module graph diverged from raw accounting evidence",
        )?;

        let duplicate_files = [(
            "duplicate.ts",
            "export default class First {}\nexport default class Second {}\n",
        )];
        let invalid_count = TestProject::collect_with_mutation(&duplicate_files, |candidate| {
            candidate.exports[0].producer_count = 1;
            Ok(())
        });
        assert_candidate_rejected(
            invalid_count,
            "default exports diverged from exact AST evidence",
        )?;
        let lost_duplicate = TestProject::collect_with_mutation(&duplicate_files, |candidate| {
            candidate.exports[0]
                .blocks
                .retain(|block| *block != DefaultExportCandidateBlock::DuplicateDefault);
            Ok(())
        });
        assert_candidate_rejected(
            lost_duplicate,
            "default exports diverged from exact AST evidence",
        )?;

        let provenance_files = [(
            "source.ts",
            "class Merged {}\nnamespace Merged {}\nexport default Merged;\n",
        )];
        let invalid_provenance =
            TestProject::collect_with_mutation(&provenance_files, |candidate| {
                candidate.exports[0].occurrences[0].identifier_namespace_provenance =
                    Some(NamespaceProvenance::ProvenAbsent);
                Ok(())
            });
        assert_candidate_rejected(
            invalid_provenance,
            "default exports diverged from exact AST evidence",
        )?;

        let synchronized_provenance =
            TestProject::collect_with_mutation(&provenance_files, |candidate| {
                candidate.exports[0].occurrences[0].identifier_namespace_provenance =
                    Some(NamespaceProvenance::ProvenAbsent);
                candidate.exports[0].blocks.retain(|block| {
                    *block != DefaultExportCandidateBlock::IdentifierNamespaceProvenance
                });
                Ok(())
            });
        assert_candidate_rejected(
            synchronized_provenance,
            "default exports diverged from exact AST evidence",
        )?;

        let invalid_occurrence_ordinal =
            TestProject::collect_with_mutation(&provenance_files, |candidate| {
                candidate.exports[0].occurrences[0].id.occurrence_ordinal = 1;
                Ok(())
            });
        assert_candidate_rejected(
            invalid_occurrence_ordinal,
            "default exports diverged from exact AST evidence",
        )?;

        let invalid_source_key =
            TestProject::collect_with_mutation(&provenance_files, |candidate| {
                let source_key = candidate.exports[0].occurrences[0].id.owner_source_key;
                candidate.exports[0].occurrences[0].id.owner_source_key =
                    SourceUnitKey(source_key.0.wrapping_add(1));
                Ok(())
            });
        assert_candidate_rejected(
            invalid_source_key,
            "default exports diverged from exact AST evidence",
        )?;

        let invalid_source_path =
            TestProject::collect_with_mutation(&provenance_files, |candidate| {
                candidate.exports[0].occurrences[0].id.normalized_owner_path =
                    "other.ts".to_owned();
                Ok(())
            });
        assert_candidate_rejected(
            invalid_source_path,
            "default exports diverged from exact AST evidence",
        )?;

        let invalid_declaration_span =
            TestProject::collect_with_mutation(&provenance_files, |candidate| {
                candidate.exports[0].occurrences[0].declaration_span.end += 1;
                Ok(())
            });
        assert_candidate_rejected(
            invalid_declaration_span,
            "default exports diverged from exact AST evidence",
        )?;

        let invalid_subject_span =
            TestProject::collect_with_mutation(&provenance_files, |candidate| {
                candidate.exports[0].occurrences[0].subject_span.start += 1;
                Ok(())
            });
        assert_candidate_rejected(
            invalid_subject_span,
            "default exports diverged from exact AST evidence",
        )?;

        let invalid_lexical_name_span =
            TestProject::collect_with_mutation(&provenance_files, |candidate| {
                let lexical_name_span = candidate.exports[0].occurrences[0]
                    .lexical_name_span
                    .as_mut()
                    .ok_or_else(|| {
                        ProjectInventoryError::new("identifier control lost its name span")
                    })?;
                lexical_name_span.start += 1;
                Ok(())
            });
        assert_candidate_rejected(
            invalid_lexical_name_span,
            "default exports diverged from exact AST evidence",
        )?;

        let allocator = Allocator::default();
        let parsed_import = Parser::new(
            &allocator,
            "import type Exact from \"./source.js\";\n",
            SourceType::ts(),
        )
        .parse();
        let mode = ProjectResolutionMode::ExplicitFileList;
        let resolver = Resolver::default();
        let importer_path = Path::new("/tmp/typokat-default-control/consumer.ts");
        let explicit_paths = BTreeMap::new();
        let configured_roots = BTreeMap::new();
        let mut raw_import = scan_default_import_candidates(
            0,
            &parsed_import.program,
            &DefaultCandidateResolution {
                mode: &mode,
                resolver: &resolver,
                importer_path,
                explicit_path_to_index: &explicit_paths,
                configured_roots: &configured_roots,
                canonical_project: None,
            },
        )?;
        raw_import[0].declaration_type_only = false;
        assert!(validate_raw_default_imports(&parsed_import.program, &raw_import).is_err());

        let parsed = Parser::new(
            &allocator,
            "export default class Exact {}\n",
            SourceType::ts(),
        )
        .parse();
        let mut raw = scan_default_export_candidates(
            0,
            SourceUnitKey::SINGLE_SOURCE,
            "source.ts",
            &parsed.program,
        );
        raw.occurrences[0].kind = DefaultExportOccurrenceKind::DirectFunction;
        assert!(validate_raw_default_exports(
            &parsed.program,
            SourceUnitKey::SINGLE_SOURCE,
            "source.ts",
            &raw
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn candidate_collection_does_not_change_frozen_public_inventory(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let project = TestProject::new()?;
        let consumer = "import Value from \"./source.js\";\nexport const value = Value;\n";
        let source = "export default 1;\n";
        let consumer_path = project.directory.join("consumer.ts");
        let source_path = project.directory.join("source.ts");
        fs::write(&consumer_path, consumer)?;
        fs::write(&source_path, source)?;
        let inputs = vec![
            FileInput {
                name: "consumer.ts".to_owned(),
                source: consumer.to_owned(),
            },
            FileInput {
                name: "source.ts".to_owned(),
                source: source.to_owned(),
            },
        ];
        let roots = vec![
            ProjectRoot {
                identity: "consumer.ts".to_owned(),
                path: consumer_path,
                exists: true,
            },
            ProjectRoot {
                identity: "source.ts".to_owned(),
                path: source_path,
                exists: true,
            },
        ];
        let mode = ProjectResolutionMode::BundlerProject {
            project_directory: project.directory.clone(),
            roots: roots.clone(),
        };
        let frozen = run_clean_bundler_project_frontend_with_deferred_auxiliary(
            inputs.clone(),
            project.directory.clone(),
            roots.clone(),
            || Ok::<_, std::convert::Infallible>(Vec::new()),
            |_, _, _, _, _, _| (),
        );
        let before = match frozen.product {
            Ok(accounted) => accounted.inventory,
            Err(_) => {
                return Err(std::io::Error::other("frozen route failed inventory").into());
            }
        };
        assert_eq!(
            before.resolutions,
            ["consumer.ts:1:1 default-import ./source.js -> unsupported"]
        );
        assert_eq!(
            before.notices,
            [
                "unsupported-module-form default-import consumer.ts:1:1 ./source.js",
                "unsupported-module-form default-export source.ts:1:1",
            ]
        );
        let candidate = collect_default_module_candidates(inputs.clone(), mode.clone())?;
        assert_eq!(candidate.imports().len(), 1);
        let frozen = run_clean_bundler_project_frontend_with_deferred_auxiliary(
            inputs,
            project.directory.clone(),
            roots,
            || Ok::<_, std::convert::Infallible>(Vec::new()),
            |_, _, _, _, _, _| (),
        );
        let after = match frozen.product {
            Ok(accounted) => accounted.inventory,
            Err(_) => {
                return Err(std::io::Error::other("frozen route failed after candidate").into());
            }
        };
        assert_eq!(before, after);
        Ok(())
    }
}

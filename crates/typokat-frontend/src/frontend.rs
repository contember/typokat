//! Parser and local-project frontend shared by the checker and driver.

use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::source::{CompilationOrigin, ModuleOrdinal, OriginalModuleOrdinal, UnitSlot};
use crate::span::Span;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Declaration, ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName,
    Program, Statement, TSModuleDeclarationName, TSModuleReference,
};
use oxc_parser::Parser;
use oxc_resolver::{ResolveError, Resolver};
use oxc_span::SourceType;
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

/// Read-only WU2 collector receipt. Production routes cannot construct this product yet.
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
    let module_plan = match account_project_modules(
        &inputs,
        &programs,
        &resolution_mode,
        SourceReexportAccounting::FrozenUnsupported,
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
    let project_units = project_programs_from_accounted_imports(
        &inputs,
        &programs,
        module_plan.paths,
        module_plan.raw_imports,
    );
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
    let product = consume(
        &mut interner,
        &auxiliary,
        &auxiliary_units,
        &project_units,
        auxiliary_parse_work,
    );
    ProjectFrontendRun {
        inputs,
        parse_errors,
        product: Ok(AccountedProjectProduct {
            inventory: module_plan.inventory,
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
    let source_keys = stable_source_keys(&paths);
    let order = dependency_order(&raw_imports);
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
    _source_reexports: PendingSourceReexports,
}

#[derive(Clone, Copy)]
enum SourceReexportAccounting {
    FrozenUnsupported,
    #[cfg(any(test, feature = "test-utils"))]
    CollectEvidence,
}

impl SourceReexportAccounting {
    fn collects_evidence(self) -> bool {
        #[cfg(any(test, feature = "test-utils"))]
        {
            matches!(self, Self::CollectEvidence)
        }
        #[cfg(not(any(test, feature = "test-utils")))]
        {
            false
        }
    }
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

    for (index, ((input, program), importer_path)) in
        inputs.iter().zip(programs).zip(&paths).enumerate()
    {
        let line_index = crate::span::LineIndex::new(&input.source);
        for statement in &program.body {
            match statement {
                Statement::ImportDeclaration(import) => {
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

    if let Some(cycle) = first_module_cycle(inputs, &cycle_edges) {
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

    Ok(AccountedModulePlan {
        inventory: ProjectModuleInventory {
            resolutions: sorted_identities(resolution_identities),
            notices: sorted_identities(notice_identities),
            parse_errors: Vec::new(),
            missing_module_locations,
        },
        paths,
        raw_imports,
        _source_reexports: source_reexports,
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
        _source_reexports: source_reexports,
        ..
    } = account_project_modules(
        &inputs,
        &programs,
        &mode,
        SourceReexportAccounting::CollectEvidence,
    )?;
    let order = dependency_order_with_reexports(&raw_imports, &source_reexports.dependency_edges);
    let source_keys = stable_source_keys(&paths);
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
    let dependency_order = order
        .iter()
        .map(|original| normalized_display_name(&inputs[*original].name))
        .collect();
    let dependency_edge_count = source_reexports
        .declarations
        .iter()
        .filter(|declaration| matches!(declaration.source, RawImportSource::Resolved(_)))
        .count();
    Ok(SourceReexportCollectionForTest {
        admitted: AdmittedSourceReexports {
            declarations: admitted,
        },
        dependency_order,
        dependency_edge_count,
        resolutions: sorted_identities(resolutions),
        blocked_notices: sorted_identities(blocked_notices),
        first_cycle: source_reexports.first_cycle,
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

#[derive(Clone, Copy)]
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

fn dependency_order(imports: &[Vec<RawImport>]) -> Vec<usize> {
    dependency_order_with_reexports(imports, &[])
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

//! Parser and local-project frontend shared by the checker and driver.

use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::source::{CompilationOrigin, ModuleOrdinal, OriginalModuleOrdinal, UnitSlot};
use crate::span::Span;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName, Program, Statement,
    TSModuleReference,
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
    let module_plan = match account_project_modules(&inputs, &programs, &resolution_mode) {
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
                    let specifier = export
                        .source
                        .as_ref()
                        .map(|source| source.value.as_str())
                        .ok_or_else(|| {
                            ProjectInventoryError::new("source re-export lost source")
                        })?;
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

    Ok(AccountedModulePlan {
        inventory: ProjectModuleInventory {
            resolutions: sorted_identities(resolution_identities),
            notices: sorted_identities(notice_identities),
            parse_errors: Vec::new(),
            missing_module_locations,
        },
        paths,
        raw_imports,
    })
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
    let mut state = vec![VisitState::Unseen; imports.len()];
    let mut order = Vec::with_capacity(imports.len());
    for index in 0..imports.len() {
        visit(index, imports, &mut state, &mut order);
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
                visit(dep, imports, state, order);
            }
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

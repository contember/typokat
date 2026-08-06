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
};
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// One owned source handed to the frontend.
#[derive(Clone)]
pub struct FileInput {
    pub name: String,
    pub source: String,
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
    load_auxiliary: impl FnOnce() -> Result<Vec<AuxiliarySourceInput>, Error>,
    consume: impl for<'ast> FnOnce(
        &mut Interner,
        &[AuxiliarySourceInput],
        &[AuxiliaryProgram<'ast>],
        &[ProjectProgram<'ast>],
        AuxiliaryParseWork,
    ) -> Product,
) -> ProjectFrontendRun<Result<Option<Product>, Error>> {
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
        return ProjectFrontendRun {
            inputs,
            parse_errors,
            product: Ok(None),
        };
    }

    let auxiliary = match load_auxiliary() {
        Ok(auxiliary) => auxiliary,
        Err(error) => {
            return ProjectFrontendRun {
                inputs,
                parse_errors,
                product: Err(error),
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
    let programs = parsed
        .iter()
        .map(|parsed| &parsed.program)
        .collect::<Vec<_>>();
    let project_units = resolved_project_programs(&inputs, &programs);
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
        product: Ok(Some(product)),
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
    let source_keys = stable_source_keys(&paths);
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

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
pub struct FileInput {
    pub name: String,
    pub source: String,
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
    let allocators: Vec<Allocator> = (0..inputs.len()).map(|_| Allocator::default()).collect();
    let parsed: Vec<_> = inputs
        .iter()
        .zip(&allocators)
        .map(|(input, allocator)| Parser::new(allocator, &input.source, SourceType::ts()).parse())
        .collect();

    let parse_errors: Vec<Vec<String>> = parsed
        .iter()
        .map(|parsed| parsed.diagnostics.iter().map(ToString::to_string).collect())
        .collect();

    let paths = normalized_input_paths(&inputs);
    let source_keys = stable_source_keys(&paths);
    let path_to_index: BTreeMap<PathBuf, usize> = paths
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, path)| (path, index))
        .collect();
    let raw_imports: Vec<Vec<RawImport>> = parsed
        .iter()
        .zip(&paths)
        .map(|(parsed, path)| scan_imports(&parsed.program, path, &path_to_index))
        .collect();
    let order = dependency_order(&raw_imports);
    let mut ordered_index = vec![0usize; inputs.len()];
    for (position, &original) in order.iter().enumerate() {
        if let Some(slot) = ordered_index.get_mut(original) {
            *slot = position;
        }
    }

    let project_units: Vec<ProjectProgram<'_>> = order
        .iter()
        .enumerate()
        .map(|(unit_slot, &original)| ProjectProgram {
            module_ordinal: ModuleOrdinal::new(original),
            unit_slot: UnitSlot::new(unit_slot),
            normalized_path: paths[original].to_string_lossy().into_owned(),
            program: &parsed[original].program,
            compilation_unit: CompilationUnit {
                source: source_keys[original],
                origin: CompilationOrigin::User(OriginalModuleOrdinal::new(original)),
                binding: ModuleBindingContext::for_program(
                    &parsed[original].program,
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
        .collect();

    let mut interner = Interner::with_intrinsics();
    let product = consume(&mut interner, &project_units);
    ProjectFrontendRun {
        inputs,
        parse_errors,
        product,
    }
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

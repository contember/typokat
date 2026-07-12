//! Statement-level checker.
//! This module wires parsing/binding output into the checker pass, flow pre-pass,
//! and relation phase. Keep durable design notes in `docs/reference/architecture.md`
//! (§5.2) and soundness rules in `docs/reference/invariants.md`.

use crate::binder::bind::{ImportPlaceholder, ImportedSymbol, ProjectBinderBuilder};
use crate::binder::bind_module_with_prelude;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::binder::Binder;
use crate::diagnostics::{Diagnostic, IncompleteSurface};
use crate::relate::{Relater, Relation};
use crate::span::Span;
use crate::types::repr::TypeParamId;
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Declaration, ExportSpecifier, ImportOrExportKind, ModuleExportName, Program, Statement,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;

mod annotations;
mod assignment;
mod calls;
mod classes;
mod context;
mod decls;
pub(in crate::check) mod eval;
mod expr;
mod flowgraph;
mod narrowing;
mod statements;

use context::{ClassFillState, DeclTypes, Pass, TypeDecl};
use decls::{reserve_type_decls, type_decl_id};
use statements::{emit_obligation_failure, emit_override_failures};

/// Trusted utility aliases and bounded ambient values, checked before user code.
pub(crate) const PRELUDE_SOURCE: &str = include_str!("../../prelude.ts");

/// The structured outcome of checking one module: type diagnostics plus the third
/// incomplete-surface channel (in-scope AST positions the checker skipped). An empty
/// `incomplete` is the normal case today — WU3–5 wire the emissions (sprint 2026-07-10).
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub incomplete: Vec<IncompleteSurface>,
}

/// Test-only emission hook (WU2 plumbing): if `TYPOKAT_TEST_EMIT_INCOMPLETE` is set,
/// record one incomplete surface per comma-separated id in its value (at span 0..0),
/// exercising the real `record_incomplete` API end to end. No real checker path emits
/// yet, so with the env var unset every run is unaffected. A blank/`1` value emits one
/// default id.
fn emit_test_incomplete(pass: &mut Pass<'_, '_>) {
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
    for id in ids {
        pass.record_incomplete(
            id,
            Span::new(0, 0),
            "test-only emission hook (WU2 plumbing)",
        );
    }
}

/// Check a parsed program and return the diagnostics plus incomplete surfaces it produces.
pub fn check_program<'ast>(interner: &mut Interner, program: &'ast Program<'ast>) -> CheckResult {
    // Build the prelude in the same type universe; user code keeps only lifetime-free
    // resolved placeholders for its declarations.
    let prelude_allocator = Allocator::default();
    let prelude_parsed = Parser::new(&prelude_allocator, PRELUDE_SOURCE, SourceType::ts()).parse();
    debug_assert!(
        !prelude_parsed.panicked && prelude_parsed.diagnostics.is_empty(),
        "the prelude must parse clean: {:?}",
        prelude_parsed.diagnostics
    );

    let binder = bind_module_with_prelude(&prelude_parsed.program, program);

    // Reserve type ids before lowering bodies so recursive declarations store ids, not
    // expanded structures.
    let mut next_type_param: u32 = 0;
    // Stable class ids are allocated in declaration order.
    let mut next_class_id: u32 = 0;
    let total_type_decls = binder.type_decl_count as usize;
    let mut type_resolved: Vec<Option<TypeId>> = vec![None; total_type_decls];

    // Fill prelude declarations in the prelude scope so user shadows cannot capture
    // their internal references.
    let mut prelude_decls: Vec<TypeDecl> = Vec::new();
    reserve_type_decls(
        interner,
        &binder,
        binder.prelude_module,
        &prelude_parsed.program,
        &mut next_type_param,
        &mut next_class_id,
        &mut prelude_decls,
        &mut type_resolved,
    );
    let mut prelude_pass = build_pass(
        interner,
        &binder,
        prelude_decls,
        type_resolved,
        DeclTypes::new(binder.decl_count),
        next_type_param,
    );
    prelude_pass.current_module = binder.prelude_module;
    prelude_pass.fill_type_decls(binder.prelude_module);
    prelude_pass.build_flow_graph(binder.prelude_module, &prelude_parsed.program.body);
    prelude_pass.check_statements(binder.prelude_module, &prelude_parsed.program.body);
    // Prelude diagnostics are never surfaced to users; debug builds still assert it is clean.
    debug_assert!(
        prelude_pass.diagnostics.is_empty(),
        "the prelude must check clean: {:?}",
        prelude_pass.diagnostics
    );
    // Extract lifetime-free outputs before the prelude AST borrow goes away.
    let prelude_params: Vec<Vec<TypeParamId>> = prelude_pass
        .type_decls
        .iter()
        .map(|decl| match decl {
            TypeDecl::Interface { params, .. }
            | TypeDecl::Alias { params, .. }
            | TypeDecl::Class { params, .. }
            | TypeDecl::Resolved { params } => params.clone(),
        })
        .collect();
    let Pass {
        mut type_resolved,
        decl_types,
        next_type_param: prelude_next_type_param,
        ..
    } = prelude_pass;
    next_type_param = prelude_next_type_param;

    // Seed string intrinsic aliases after their constraints have been recorded normally.
    {
        let wk = interner.well_known();
        for (name, marker) in [
            ("Uppercase", wk.uppercase),
            ("Lowercase", wk.lowercase),
            ("Capitalize", wk.capitalize),
            ("Uncapitalize", wk.uncapitalize),
        ] {
            if let Some(decl_id) = type_decl_id(&binder, binder.prelude_module, name) {
                if let Some(slot) = type_resolved.get_mut(decl_id.index()) {
                    *slot = Some(marker);
                }
            }
        }
    }

    // User declarations append after prelude placeholders, preserving binder DeclId indices.
    let mut type_decls: Vec<TypeDecl<'ast>> = prelude_params
        .into_iter()
        .map(|params| TypeDecl::Resolved { params })
        .collect();
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
    let mut pass = build_pass(
        interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
    );

    // Phase 0: fill named type declarations before walking values.
    pass.fill_type_decls(binder.module);

    // Phase 0.5: build complete flow graphs before narrowed reads are resolved.
    pass.build_flow_graph(binder.module, &program.body);

    // Phase 1: walk the module body and collect relation obligations.
    pass.check_statements(binder.module, &program.body);

    emit_test_incomplete(&mut pass);

    // Move the working set out before borrowing the store immutably for phase 2.
    let Pass {
        interner,
        obligations,
        override_checks,
        mut diagnostics,
        incomplete,
        ..
    } = pass;

    // Phase 2: relate obligations after mutable interning is done.
    let well_known = interner.well_known();
    let store = interner.store();
    let mut relater = Relater::new(store, well_known);

    for ob in &obligations {
        if let Relation::No(chain) = relater.is_assignable(ob.src, ob.tgt) {
            emit_obligation_failure(store, ob, chain.head(), &mut diagnostics);
        }
    }

    // Override checks share this relater with normal obligations.
    emit_override_failures(
        store,
        well_known,
        &mut relater,
        &override_checks,
        &mut diagnostics,
    );

    CheckResult {
        diagnostics,
        incomplete,
    }
}

/// One parsed project unit handed to the serial M29 project checker.
pub struct ProjectProgram<'ast> {
    pub program: &'ast Program<'ast>,
    pub imports: Vec<ProjectImport>,
}

/// One named import after the driver has resolved its module specifier.
pub struct ProjectImport {
    pub local: String,
    pub imported: String,
    pub module: String,
    pub source: ProjectImportSource,
    pub type_only: bool,
    pub span: Span,
}

pub enum ProjectImportSource {
    Resolved(usize),
    Missing(String),
}

#[derive(Clone, Copy)]
struct ExportedSlots {
    value: Option<DeclId>,
    ty: Option<DeclId>,
    /// A type-only export hid a real local value slot. Imports must keep its
    /// runtime barrier even though no value declaration crosses the boundary.
    value_erased: bool,
}

type ExportSurface = BTreeMap<String, ExportedSlots>;

/// Check a dependency-ordered project in one serial type universe. Returns one
/// [`CheckResult`] per unit, indexed like `units`.
pub fn check_project_programs<'ast>(
    interner: &mut Interner,
    units: &[ProjectProgram<'ast>],
) -> Vec<CheckResult> {
    let prelude_allocator = Allocator::default();
    let prelude_parsed = Parser::new(&prelude_allocator, PRELUDE_SOURCE, SourceType::ts()).parse();
    debug_assert!(
        !prelude_parsed.panicked && prelude_parsed.diagnostics.is_empty(),
        "the prelude must parse clean: {:?}",
        prelude_parsed.diagnostics
    );

    let mut builder = ProjectBinderBuilder::new(&prelude_parsed.program);
    let mut module_scopes = Vec::with_capacity(units.len());
    let mut module_placeholders: Vec<Vec<ImportPlaceholder>> = Vec::with_capacity(units.len());
    let mut module_diagnostics: Vec<Vec<Diagnostic>> =
        (0..units.len()).map(|_| Vec::new()).collect();
    let mut module_incomplete: Vec<Vec<IncompleteSurface>> =
        (0..units.len()).map(|_| Vec::new()).collect();
    let mut exports: Vec<ExportSurface> = Vec::with_capacity(units.len());

    for (index, unit) in units.iter().enumerate() {
        let imports = imported_symbols(unit, &exports, &mut module_diagnostics[index]);
        let (scope, placeholders) = builder.add_module(unit.program, &imports);
        let surface = collect_exports(
            &builder,
            scope,
            unit.program,
            &mut module_diagnostics[index],
        );
        module_scopes.push(scope);
        module_placeholders.push(placeholders);
        exports.push(surface);
    }

    let binder_module = module_scopes.last().copied().unwrap_or(ScopeId(0));
    let binder = builder.finish(binder_module);

    let mut next_type_param: u32 = 0;
    let mut next_class_id: u32 = 0;
    let total_type_decls = binder.type_decl_count as usize;
    let mut type_resolved: Vec<Option<TypeId>> = vec![None; total_type_decls];

    let mut prelude_decls: Vec<TypeDecl> = Vec::new();
    reserve_type_decls(
        interner,
        &binder,
        binder.prelude_module,
        &prelude_parsed.program,
        &mut next_type_param,
        &mut next_class_id,
        &mut prelude_decls,
        &mut type_resolved,
    );
    let mut prelude_pass = build_pass(
        interner,
        &binder,
        prelude_decls,
        type_resolved,
        DeclTypes::new(binder.decl_count),
        next_type_param,
    );
    prelude_pass.current_module = binder.prelude_module;
    prelude_pass.fill_type_decls(binder.prelude_module);
    prelude_pass.build_flow_graph(binder.prelude_module, &prelude_parsed.program.body);
    prelude_pass.check_statements(binder.prelude_module, &prelude_parsed.program.body);
    debug_assert!(
        prelude_pass.diagnostics.is_empty(),
        "the prelude must check clean: {:?}",
        prelude_pass.diagnostics
    );
    let prelude_params: Vec<Vec<TypeParamId>> = prelude_pass
        .type_decls
        .iter()
        .map(|decl| match decl {
            TypeDecl::Interface { params, .. }
            | TypeDecl::Alias { params, .. }
            | TypeDecl::Class { params, .. }
            | TypeDecl::Resolved { params } => params.clone(),
        })
        .collect();
    let Pass {
        mut type_resolved,
        mut decl_types,
        next_type_param: prelude_next_type_param,
        ..
    } = prelude_pass;
    next_type_param = prelude_next_type_param;

    seed_string_intrinsics(interner, &binder, &mut type_resolved);

    let mut type_decls: Vec<TypeDecl<'ast>> = prelude_params
        .into_iter()
        .map(|params| TypeDecl::Resolved { params })
        .collect();
    let error = interner.well_known().error;
    let mut type_decl_ranges = Vec::with_capacity(units.len());
    for (scope, unit) in module_scopes.iter().copied().zip(units) {
        let start = type_decls.len();
        if let Some(placeholders) = module_placeholders.get(type_decl_ranges.len()) {
            for placeholder in placeholders {
                if let Some(decl_id) = placeholder.ty {
                    seed_resolved_type(&mut type_decls, &mut type_resolved, decl_id, error);
                }
            }
        }
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
        type_decl_ranges.push((start, type_decls.len()));
    }

    for placeholders in &module_placeholders {
        for placeholder in placeholders {
            if let Some(decl_id) = placeholder.value {
                decl_types.set(decl_id, error);
            }
        }
    }
    let mut pass = build_pass(
        interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
    );

    for (index, scope) in module_scopes.iter().copied().enumerate() {
        let (start, end) = type_decl_ranges
            .get(index)
            .copied()
            .unwrap_or((0, pass.type_decls.len()));
        pass.current_module = scope;
        pass.fill_type_decls_range(scope, start, end);
        emit_pending_checks(&mut pass);
        module_diagnostics[index].append(&mut pass.diagnostics);
        module_incomplete[index].append(&mut pass.incomplete);
    }

    for (index, (scope, unit)) in module_scopes.iter().copied().zip(units).enumerate() {
        pass.current_module = scope;
        pass.build_flow_graph(scope, &unit.program.body);
        pass.check_statements(scope, &unit.program.body);
        emit_pending_checks(&mut pass);
        emit_test_incomplete(&mut pass);
        module_diagnostics[index].append(&mut pass.diagnostics);
        module_incomplete[index].append(&mut pass.incomplete);
    }

    module_diagnostics
        .into_iter()
        .zip(module_incomplete)
        .map(|(diagnostics, incomplete)| CheckResult {
            diagnostics,
            incomplete,
        })
        .collect()
}

fn imported_symbols(
    unit: &ProjectProgram<'_>,
    exports: &[ExportSurface],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ImportedSymbol> {
    let mut imports = Vec::new();
    for import in &unit.imports {
        match &import.source {
            ProjectImportSource::Missing(module) => {
                diagnostics.push(Diagnostic::cannot_find_module(import.span, module));
                imports.push(placeholder_import(&import.local, import.type_only));
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
                        if value_barrier {
                            imports.push(ImportedSymbol::value_lookup_barrier(
                                import.local.clone(),
                                slots.ty,
                            ));
                        } else {
                            let value = if import.type_only { None } else { slots.value };
                            imports.push(ImportedSymbol::new(
                                import.local.clone(),
                                value,
                                slots.ty,
                            ));
                        }
                    }
                    None => {
                        diagnostics.push(Diagnostic::no_exported_member(
                            import.span,
                            &import.module,
                            &import.imported,
                        ));
                        imports.push(placeholder_import(&import.local, import.type_only));
                    }
                }
            }
        }
    }
    imports
}

fn placeholder_import(local: &str, type_only: bool) -> ImportedSymbol {
    if type_only {
        ImportedSymbol::placeholder_type(local.to_string())
    } else {
        ImportedSymbol::placeholder_value_and_type(local.to_string())
    }
}

fn collect_exports(
    builder: &ProjectBinderBuilder,
    scope: ScopeId,
    program: &Program<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> ExportSurface {
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
            // import side (driver.rs) where the outer kind ORs with each specifier.
            let outer_type_only = export.export_kind == ImportOrExportKind::Type;
            for specifier in &export.specifiers {
                collect_list_export(
                    builder,
                    scope,
                    specifier,
                    outer_type_only,
                    &mut surface,
                    diagnostics,
                );
            }
        }
    }
    surface
}

fn collect_declaration_export(
    builder: &ProjectBinderBuilder,
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
                },
            );
        }
        _ => {}
    }
}

fn collect_list_export(
    builder: &ProjectBinderBuilder,
    scope: ScopeId,
    specifier: &ExportSpecifier<'_>,
    outer_type_only: bool,
    surface: &mut ExportSurface,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(local) = module_export_name(&specifier.local) else {
        return;
    };
    let Some(exported) = module_export_name(&specifier.exported) else {
        return;
    };
    let (mut value, ty) = builder.local_symbol_slots(scope, local);
    let local_value_barrier = builder.local_value_lookup_barrier(scope, local);
    // Existence is judged against the real symbol; a value-only local is still a
    // valid `export type { x }` target (the error surfaces on the importer, below).
    if value.is_none() && ty.is_none() && !local_value_barrier {
        diagnostics.push(Diagnostic::cannot_find_name(
            Span::from_oxc(specifier.local.span()),
            local,
        ));
        return;
    }
    // A type-only specifier (`export type { x }` or `export { type x }`) must not
    // supply a runtime value — suppress the value slot so a non-type-only import
    // cannot use it as a value (tsc TS1362; M29 stand-in is TK2304 on the importer).
    let type_only = outer_type_only || specifier.export_kind == ImportOrExportKind::Type;
    let value_erased = local_value_barrier || (type_only && value.is_some());
    if type_only {
        value = None;
    }
    surface.insert(
        exported.to_string(),
        ExportedSlots {
            value,
            ty,
            value_erased,
        },
    );
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

fn seed_resolved_type<'ast>(
    type_decls: &mut Vec<TypeDecl<'ast>>,
    type_resolved: &mut [Option<TypeId>],
    decl_id: DeclId,
    ty: TypeId,
) {
    let index = decl_id.index();
    while type_decls.len() < index {
        type_decls.push(TypeDecl::Resolved { params: Vec::new() });
    }
    if type_decls.len() == index {
        type_decls.push(TypeDecl::Resolved { params: Vec::new() });
    }
    if let Some(slot) = type_resolved.get_mut(index) {
        *slot = Some(ty);
    }
}

fn seed_string_intrinsics(
    interner: &Interner,
    binder: &Binder,
    type_resolved: &mut [Option<TypeId>],
) {
    let wk = interner.well_known();
    for (name, marker) in [
        ("Uppercase", wk.uppercase),
        ("Lowercase", wk.lowercase),
        ("Capitalize", wk.capitalize),
        ("Uncapitalize", wk.uncapitalize),
    ] {
        if let Some(decl_id) = type_decl_id(binder, binder.prelude_module, name) {
            if let Some(slot) = type_resolved.get_mut(decl_id.index()) {
                *slot = Some(marker);
            }
        }
    }
}

fn emit_pending_checks(pass: &mut Pass<'_, '_>) {
    let obligations = std::mem::take(&mut pass.obligations);
    let override_checks = std::mem::take(&mut pass.override_checks);
    let well_known = pass.interner.well_known();
    let store = pass.interner.store();
    let mut relater = Relater::new(store, well_known);

    for ob in &obligations {
        if let Relation::No(chain) = relater.is_assignable(ob.src, ob.tgt) {
            emit_obligation_failure(store, ob, chain.head(), &mut pass.diagnostics);
        }
    }
    emit_override_failures(
        store,
        well_known,
        &mut relater,
        &override_checks,
        &mut pass.diagnostics,
    );
}

impl Pass<'_, '_> {
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
        // Canonical "exactly once per site": an id+span is one skipped position even if
        // the walk revisits it (e.g. a signature return type lowered in both fill and
        // body-check). The renderer dedups too; keeping the vector unique also makes it
        // the authority the conformance harness diffs.
        if self
            .incomplete
            .iter()
            .any(|rec| rec.id == id && rec.span == span)
        {
            return;
        }
        self.incomplete
            .push(IncompleteSurface::new(id, span, context));
    }
}

/// Construct a fresh phase-1 [`Pass`]. Fill states come from the decl kind:
/// classes/interfaces/template aliases start `Pending`; resolved placeholders and
/// other declarations start `Done`.
fn build_pass<'a, 'ast>(
    interner: &'a mut Interner,
    binder: &'a Binder,
    type_decls: Vec<TypeDecl<'ast>>,
    type_resolved: Vec<Option<TypeId>>,
    decl_types: DeclTypes,
    next_type_param: u32,
) -> Pass<'a, 'ast> {
    let class_fill: Vec<ClassFillState> = type_decls
        .iter()
        .map(|decl| match decl {
            TypeDecl::Class { .. } => ClassFillState::Pending,
            _ => ClassFillState::Done,
        })
        .collect();
    let template_fill: Vec<ClassFillState> = type_decls
        .iter()
        .map(|decl| match decl {
            TypeDecl::Interface { .. } => ClassFillState::Pending,
            TypeDecl::Alias {
                object_template: Some(_),
                ..
            } => ClassFillState::Pending,
            _ => ClassFillState::Done,
        })
        .collect();

    Pass {
        interner,
        binder,
        // Overwritten before each module's fill/flow/check phase; the user module is
        // the single-file default (backlog 58).
        current_module: binder.module,
        type_decls,
        type_resolved,
        type_param_scopes: Vec::new(),
        static_class_type_param_barriers: Vec::new(),
        next_type_param,
        class_parents: FxHashMap::default(),
        class_ctors: FxHashMap::default(),
        class_value_aliases: FxHashMap::default(),
        class_ctor_overloads: FxHashMap::default(),
        class_type_params: FxHashMap::default(),
        class_pending_abstract: FxHashMap::default(),
        class_member_kinds: FxHashMap::default(),
        class_names: FxHashMap::default(),
        class_fill,
        template_fill,
        decl_types,
        var_annotation_surfaces: FxHashMap::default(),
        var_value_type_states: FxHashMap::default(),
        obligations: Vec::new(),
        override_checks: Vec::new(),
        diagnostics: Vec::new(),
        incomplete: Vec::new(),
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
        cond_memo: FxHashMap::default(),
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
        current_class: None,
        current_super_ctor: None,
        current_in_ctor: false,
    }
}

// ===========================================================================
// M5: named types — reserve-then-fill (mvp-plan M5, §3, §6.3).
// ===========================================================================

// ===========================================================================
// M9: type-parameter scoping — a name → TypeParam frame stack on `Pass`.
// ===========================================================================

#[cfg(test)]
mod tests;

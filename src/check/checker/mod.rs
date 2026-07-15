//! Statement-level checker.
//! This module wires parsing/binding output into the checker pass, flow pre-pass,
//! and relation phase. Keep durable design notes in `docs/reference/architecture.md`
//! (§5.2) and soundness rules in `docs/reference/invariants.md`.

use crate::binder::bind::{ImportPlaceholder, ImportedSymbol, ProjectBinderBuilder};
use crate::binder::bind_module_with_prelude;
use crate::binder::declaration::{LegacyTypeStorageId, ValueStorageId};
use crate::binder::namespace::{CompilationUnit, LocalAmbientExportAliasFailureKind};
use crate::binder::scope::ScopeId;
use crate::binder::Binder;
use crate::check::query::SemanticQueryCoordinator;
use crate::check::query::SemanticQueryState;
use crate::class_semantics::{Exhaustion, PublishedClasses};
use crate::diagnostics::{render_reason_chain, Diagnostic, IncompleteSurface};
use crate::relate::RelationOutcome;
use crate::span::Span;
use crate::types::repr::TypeParamId;
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Declaration, ExportSpecifier, ImportOrExportKind, ModuleExportName, Program, Statement, TSType,
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
pub(crate) mod events;
mod expr;
mod flowgraph;
mod indexed_access;
pub(crate) mod lexical_events;
mod narrowing;
mod statements;

use context::{
    AssignObligation, CheckerEffects, ClassFillState, DeclTypes, OverrideCheck, Pass, TypeDecl,
};
use decls::{reserve_type_decls, type_decl_id, value_decl_id, walk_type_decls, TopTypeDecl};
use events::{CandidateEffects, EventStore, ModuleOrdinal, RecordTicket, UnitSlot};
use lexical_events::{ClassBinding, LexicalOwnerPhase, LexicalReservations};
use statements::{emit_exhausted_obligation, emit_obligation_failure};

struct PassReporting {
    module_ordinal: ModuleOrdinal,
    unit_slot: UnitSlot,
    event_store: EventStore,
    lexical_events: LexicalReservations,
    suppress_effects: bool,
}

fn reserve_internal_reporting(
    program: &Program<'_>,
    module_ordinal: ModuleOrdinal,
    unit_slot: UnitSlot,
) -> PassReporting {
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    lexical_events
        .reserve_program(module_ordinal, unit_slot, program, &mut event_store)
        .expect("lexical event reservation must reference valid events");
    PassReporting {
        module_ordinal,
        unit_slot,
        event_store,
        lexical_events,
        suppress_effects: false,
    }
}

/// Trusted utility aliases and bounded ambient values, checked before user code.
pub(crate) const PRELUDE_SOURCE: &str = include_str!("../../prelude.ts");

const TRUSTED_PRELUDE_INTRINSICS: [&str; 6] = [
    "OmitThisParameter",
    "Uppercase",
    "Lowercase",
    "Capitalize",
    "Uncapitalize",
    "ThisType",
];

fn expected_trusted_prelude_incomplete(
    binder: &Binder,
    program: &Program<'_>,
) -> Option<Vec<IncompleteSurface>> {
    let mut valid = true;
    let mut seen = FxHashSet::default();
    let mut expected = Vec::with_capacity(TRUSTED_PRELUDE_INTRINSICS.len());
    walk_type_decls(
        binder,
        binder.prelude_module,
        program,
        &mut |scope, _, declaration| {
            let TopTypeDecl::Alias(alias) = declaration else {
                return;
            };
            let name = alias.id.name.as_str();
            if !TRUSTED_PRELUDE_INTRINSICS.contains(&name) {
                return;
            }
            let TSType::TSIntrinsicKeyword(keyword) = &alias.type_annotation else {
                valid = false;
                return;
            };
            if scope != binder.prelude_module || !seen.insert(name.to_string()) {
                valid = false;
                return;
            }
            expected.push(IncompleteSurface::new(
                "annotation-lower/intrinsic-keyword/self",
                Span::from_oxc(keyword.span),
                "intrinsic keyword type not modeled",
            ));
        },
    );
    if valid
        && TRUSTED_PRELUDE_INTRINSICS
            .iter()
            .all(|name| seen.contains(*name))
    {
        Some(expected)
    } else {
        None
    }
}

fn trusted_prelude_records_are_clean(
    binder: &Binder,
    program: &Program<'_>,
    records: &BTreeMap<ModuleOrdinal, (Vec<Diagnostic>, Vec<IncompleteSurface>)>,
) -> bool {
    let Some(expected_incomplete) = expected_trusted_prelude_incomplete(binder, program) else {
        return false;
    };
    let Some((diagnostics, incomplete)) = records.get(&ModuleOrdinal::new(0)) else {
        return false;
    };
    records.len() == 1 && diagnostics.is_empty() && incomplete == &expected_incomplete
}

/// Lifetime-free state handed from the trusted prelude pass to user checking.
struct TrustedPreludeHandoff {
    /// Ordered by prelude legacy type-storage id, preserving the user table prefix.
    type_decl_params: Vec<Vec<TypeParamId>>,
    type_resolved: Vec<Option<TypeId>>,
    decl_types: DeclTypes,
    next_type_param: u32,
    next_class_id: u32,
}

/// Parse, bind, and check the trusted prelude in the caller's run-local type universe.
fn bootstrap_trusted_prelude(
    interner: &mut Interner,
    bind: impl FnOnce(&Program<'_>) -> Binder,
) -> (Binder, TrustedPreludeHandoff) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, PRELUDE_SOURCE, SourceType::ts()).parse();
    debug_assert!(
        !parsed.panicked && parsed.diagnostics.is_empty(),
        "the prelude must parse clean: {:?}",
        parsed.diagnostics
    );

    let binder = bind(&parsed.program);
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_resolved = vec![None; binder.type_decl_count as usize];
    let mut type_decls = Vec::new();
    reserve_type_decls(
        interner,
        &binder,
        binder.prelude_module,
        &parsed.program,
        &mut next_type_param,
        &mut next_class_id,
        &mut type_decls,
        &mut type_resolved,
    );

    let prelude_ordinal = ModuleOrdinal::new(0);
    let prelude_slot = UnitSlot::new(0);
    // Capture into an isolated store so cleanliness is checked without leaking records.
    let mut reporting = reserve_internal_reporting(&parsed.program, prelude_ordinal, prelude_slot);
    attach_type_decl_owners(
        &mut reporting.lexical_events,
        prelude_ordinal,
        &binder,
        binder.prelude_module,
        &parsed.program,
    );
    let mut pass = build_pass_with_reporting(
        interner,
        &binder,
        type_decls,
        type_resolved,
        DeclTypes::new(binder.decl_count),
        next_type_param,
        reporting,
    );
    pass.current_module = binder.prelude_module;
    pass.fill_type_decls(binder.prelude_module);
    pass.build_flow_graph(binder.prelude_module, &parsed.program.body);
    pass.check_statements(binder.prelude_module, &parsed.program.body);

    let records = finish_event_effects(&mut pass);
    debug_assert!(
        trusted_prelude_records_are_clean(&binder, &parsed.program, &records),
        "the prelude must check clean: {records:?}"
    );

    let type_decl_params = pass
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
        next_type_param,
        ..
    } = pass;
    seed_prelude_intrinsics(interner, &binder, &mut type_resolved);

    (
        binder,
        TrustedPreludeHandoff {
            type_decl_params,
            type_resolved,
            decl_types,
            next_type_param,
            next_class_id,
        },
    )
}

/// The structured outcome of checking one module: type diagnostics plus the third
/// incomplete-surface channel (in-scope AST positions the checker skipped). An empty
/// `incomplete` is the normal case today — WU3–5 wire the emissions (sprint 2026-07-10).
pub struct CheckResult {
    pub(crate) module_ordinal: ModuleOrdinal,
    pub(crate) unit_slot: UnitSlot,
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
    let source_start = pass
        .lexical_events
        .top_level()
        .iter()
        .find(|site| site.source.module_ordinal == pass.current_module_ordinal)
        .map(|site| site.source.source_start);
    if let Some(source_start) = source_start {
        pass.with_lexical_effects(source_start, LexicalOwnerPhase::Incomplete, |pass| {
            for id in ids {
                pass.record_incomplete(
                    id,
                    Span::new(0, 0),
                    "test-only emission hook (WU2 plumbing)",
                );
            }
        });
    }
}

/// Check a parsed program and return the diagnostics plus incomplete surfaces it produces.
pub fn check_program<'ast>(interner: &mut Interner, program: &'ast Program<'ast>) -> CheckResult {
    let module_ordinal = ModuleOrdinal::new(0);
    let unit_slot = UnitSlot::new(0);
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    lexical_events
        .reserve_program(module_ordinal, unit_slot, program, &mut event_store)
        .expect("lexical event reservation must reference valid events");

    let (
        binder,
        TrustedPreludeHandoff {
            type_decl_params,
            mut type_resolved,
            decl_types,
            mut next_type_param,
            mut next_class_id,
        },
    ) = bootstrap_trusted_prelude(interner, |prelude| {
        bind_module_with_prelude(prelude, program)
    });

    // User declarations append after prelude placeholders, preserving legacy storage indices.
    let mut type_decls: Vec<TypeDecl<'ast>> = type_decl_params
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
    attach_type_decl_owners(
        &mut lexical_events,
        module_ordinal,
        &binder,
        binder.module,
        program,
    );
    attach_class_bindings(
        &mut lexical_events,
        module_ordinal,
        &binder,
        binder.module,
        program,
        &type_decls,
    );
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .expect("one callable binder reservation pass");
    let mut external_effects = BTreeMap::new();
    enqueue_local_ambient_export_alias_diagnostics(&binder, &lexical_events, &mut external_effects);
    let mut pass = build_pass_with_reporting(
        interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReporting {
            module_ordinal,
            unit_slot,
            event_store,
            lexical_events,
            suppress_effects: false,
        },
    );
    for effects in external_effects.into_values() {
        pass.enqueue_effects(CheckerEffects::from_records(effects));
    }

    // Phase 0: fill named type declarations before walking values.
    pass.fill_type_decls(binder.module);
    pass.publish_class_surfaces(&[(module_ordinal, binder.module)]);
    pass.fill_pending_interfaces_range(binder.module, 0, pass.type_decls.len());

    // Phase 0.5: build complete flow graphs before narrowed reads are resolved.
    pass.build_flow_graph(binder.module, &program.body);

    // Phase 1: walk the module body and collect relation obligations.
    pass.check_statements(binder.module, &program.body);

    emit_test_incomplete(&mut pass);

    let mut records = finish_event_effects(&mut pass);
    let (diagnostics, incomplete) = records.remove(&module_ordinal).unwrap_or_default();

    CheckResult {
        module_ordinal,
        unit_slot,
        diagnostics,
        incomplete,
    }
}

/// One parsed project unit handed to the serial M29 project checker.
pub struct ProjectProgram<'ast> {
    pub(crate) module_ordinal: ModuleOrdinal,
    pub(crate) unit_slot: UnitSlot,
    pub program: &'ast Program<'ast>,
    pub compilation_unit: CompilationUnit,
    pub imports: Vec<ProjectImport>,
}

/// One named import after the driver has resolved its module specifier.
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

#[derive(Clone, Copy)]
struct ExportedSlots {
    value: Option<ValueStorageId>,
    ty: Option<LegacyTypeStorageId>,
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
    check_project_programs_inner(interner, units, |_, _, _| {})
}

#[cfg(test)]
pub(crate) fn check_project_programs_with_binding_inspector<'ast, F>(
    interner: &mut Interner,
    units: &[ProjectProgram<'ast>],
    inspect: F,
) -> Vec<CheckResult>
where
    F: FnOnce(&Binder, &LexicalReservations, &[ScopeId]),
{
    check_project_programs_inner(interner, units, inspect)
}

fn check_project_programs_inner<'ast, F>(
    interner: &mut Interner,
    units: &[ProjectProgram<'ast>],
    inspect: F,
) -> Vec<CheckResult>
where
    F: FnOnce(&Binder, &LexicalReservations, &[ScopeId]),
{
    let mut event_store = EventStore::default();
    let mut lexical_events = LexicalReservations::default();
    for (slot, unit) in units.iter().enumerate() {
        debug_assert_eq!(unit.unit_slot.index(), slot);
        lexical_events
            .reserve_program(
                unit.module_ordinal,
                unit.unit_slot,
                unit.program,
                &mut event_store,
            )
            .expect("lexical event reservation must reference valid events");
    }

    let mut module_scopes = Vec::with_capacity(units.len());
    let mut module_placeholders: Vec<Vec<ImportPlaceholder>> = Vec::with_capacity(units.len());
    let mut external_effects: BTreeMap<RecordTicket, CandidateEffects> = BTreeMap::new();
    let (
        binder,
        TrustedPreludeHandoff {
            type_decl_params,
            mut type_resolved,
            mut decl_types,
            mut next_type_param,
            mut next_class_id,
        },
    ) = bootstrap_trusted_prelude(interner, |prelude| {
        let mut builder = ProjectBinderBuilder::new(prelude);
        let mut exports: Vec<ExportSurface> = Vec::with_capacity(units.len());
        for unit in units {
            let imports = imported_symbols(unit, &exports, &lexical_events, &mut external_effects);
            let (scope, placeholders) =
                builder.add_module(unit.program, &imports, unit.compilation_unit);
            let surface = collect_exports(
                &builder,
                scope,
                unit.program,
                unit.module_ordinal,
                &lexical_events,
                &mut external_effects,
            );
            module_scopes.push(scope);
            module_placeholders.push(placeholders);
            exports.push(surface);
        }
        let binder_module = module_scopes.last().copied().unwrap_or(ScopeId(0));
        builder.finish(binder_module)
    });

    let mut type_decls: Vec<TypeDecl<'ast>> = type_decl_params
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
        attach_type_decl_owners(
            &mut lexical_events,
            unit.module_ordinal,
            &binder,
            scope,
            unit.program,
        );
        attach_class_bindings(
            &mut lexical_events,
            unit.module_ordinal,
            &binder,
            scope,
            unit.program,
            &type_decls,
        );
        type_decl_ranges.push((start, type_decls.len()));
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .expect("one callable binder reservation pass");
    inspect(&binder, &lexical_events, &module_scopes);
    enqueue_local_ambient_export_alias_diagnostics(&binder, &lexical_events, &mut external_effects);

    for placeholders in &module_placeholders {
        for placeholder in placeholders {
            if let Some(decl_id) = placeholder.value {
                decl_types.set(decl_id, error);
            }
        }
    }
    let mut pass = build_pass_with_reporting(
        interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReporting {
            module_ordinal: units
                .first()
                .map(|unit| unit.module_ordinal)
                .unwrap_or(ModuleOrdinal::new(0)),
            unit_slot: units
                .first()
                .map(|unit| unit.unit_slot)
                .unwrap_or(UnitSlot::new(0)),
            event_store,
            lexical_events,
            suppress_effects: false,
        },
    );
    for effects in external_effects.into_values() {
        pass.enqueue_effects(CheckerEffects::from_records(effects));
    }

    for (index, scope) in module_scopes.iter().copied().enumerate() {
        let (start, end) = type_decl_ranges
            .get(index)
            .copied()
            .unwrap_or((0, pass.type_decls.len()));
        pass.current_module = scope;
        if let Some(unit) = units.get(index) {
            pass.current_module_ordinal = unit.module_ordinal;
            pass.current_unit_slot = unit.unit_slot;
        }
        pass.fill_type_decls_range(scope, start, end);
    }

    let publication_scopes: Vec<(ModuleOrdinal, ScopeId)> = units
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|(unit, scope)| (unit.module_ordinal, scope))
        .collect();
    pass.publish_class_surfaces(&publication_scopes);

    for (index, scope) in module_scopes.iter().copied().enumerate() {
        let (start, end) = type_decl_ranges
            .get(index)
            .copied()
            .unwrap_or((0, pass.type_decls.len()));
        pass.current_module = scope;
        if let Some(unit) = units.get(index) {
            pass.current_module_ordinal = unit.module_ordinal;
            pass.current_unit_slot = unit.unit_slot;
        }
        pass.fill_pending_interfaces_range(scope, start, end);
    }

    for (scope, unit) in module_scopes.iter().copied().zip(units) {
        pass.current_module = scope;
        pass.current_module_ordinal = unit.module_ordinal;
        pass.current_unit_slot = unit.unit_slot;
        pass.build_flow_graph(scope, &unit.program.body);
        pass.check_statements(scope, &unit.program.body);
        emit_test_incomplete(&mut pass);
    }

    let mut records = finish_event_effects(&mut pass);
    units
        .iter()
        .map(|unit| {
            let (diagnostics, incomplete) =
                records.remove(&unit.module_ordinal).unwrap_or_default();
            CheckResult {
                module_ordinal: unit.module_ordinal,
                unit_slot: unit.unit_slot,
                diagnostics,
                incomplete,
            }
        })
        .collect()
}

fn imported_symbols(
    unit: &ProjectProgram<'_>,
    exports: &[ExportSurface],
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<RecordTicket, CandidateEffects>,
) -> Vec<ImportedSymbol> {
    let mut imports = Vec::new();
    for import in &unit.imports {
        match &import.source {
            ProjectImportSource::Missing(module) => {
                enqueue_external_diagnostic(
                    reservations,
                    effects,
                    unit.module_ordinal,
                    import.owner_start,
                    Diagnostic::cannot_find_module(import.span, module),
                );
                imports.push(placeholder_import(import));
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
                                import.local_span,
                            ));
                        } else {
                            let value = if import.type_only { None } else { slots.value };
                            imports.push(ImportedSymbol::new(
                                import.local.clone(),
                                value,
                                slots.ty,
                                import.local_span,
                            ));
                        }
                    }
                    None => {
                        enqueue_external_diagnostic(
                            reservations,
                            effects,
                            unit.module_ordinal,
                            import.owner_start,
                            Diagnostic::no_exported_member(
                                import.span,
                                &import.module,
                                &import.imported,
                            ),
                        );
                        imports.push(placeholder_import(import));
                    }
                }
            }
        }
    }
    imports
}

fn placeholder_import(import: &ProjectImport) -> ImportedSymbol {
    if import.type_only {
        ImportedSymbol::placeholder_type(import.local.clone(), import.local_span)
    } else {
        ImportedSymbol::placeholder_value_and_type(import.local.clone(), import.local_span)
    }
}

fn collect_exports(
    builder: &ProjectBinderBuilder,
    scope: ScopeId,
    program: &Program<'_>,
    module_ordinal: ModuleOrdinal,
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<RecordTicket, CandidateEffects>,
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
            let mut context = ListExportContext {
                builder,
                scope,
                surface: &mut surface,
                module_ordinal,
                reservations,
                effects,
            };
            for specifier in &export.specifiers {
                collect_list_export(&mut context, specifier, outer_type_only, stmt.span().start);
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

struct ListExportContext<'a> {
    builder: &'a ProjectBinderBuilder,
    scope: ScopeId,
    surface: &'a mut ExportSurface,
    module_ordinal: ModuleOrdinal,
    reservations: &'a LexicalReservations,
    effects: &'a mut BTreeMap<RecordTicket, CandidateEffects>,
}

fn collect_list_export(
    context: &mut ListExportContext<'_>,
    specifier: &ExportSpecifier<'_>,
    outer_type_only: bool,
    owner_start: u32,
) {
    let Some(local) = module_export_name(&specifier.local) else {
        return;
    };
    let Some(exported) = module_export_name(&specifier.exported) else {
        return;
    };
    let (mut value, ty) = context.builder.local_symbol_slots(context.scope, local);
    let local_value_barrier = context
        .builder
        .local_value_lookup_barrier(context.scope, local);
    // Existence is judged against the real symbol; a value-only local is still a
    // valid `export type { x }` target (the error surfaces on the importer, below).
    if value.is_none() && ty.is_none() && !local_value_barrier {
        enqueue_external_diagnostic(
            context.reservations,
            context.effects,
            context.module_ordinal,
            owner_start,
            Diagnostic::cannot_find_name(Span::from_oxc(specifier.local.span()), local),
        );
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
    context.surface.insert(
        exported.to_string(),
        ExportedSlots {
            value,
            ty,
            value_erased,
        },
    );
}

fn enqueue_external_diagnostic(
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<RecordTicket, CandidateEffects>,
    module_ordinal: ModuleOrdinal,
    owner_start: u32,
    diagnostic: Diagnostic,
) {
    let owner = reservations
        .owner_at(module_ordinal, owner_start, LexicalOwnerPhase::Immediate)
        .expect("import/export diagnostic owner must be lexically reserved");
    effects
        .entry(owner.ticket)
        .or_insert_with(|| CandidateEffects::new(owner.ticket))
        .diagnostic(diagnostic);
}

fn enqueue_local_ambient_export_alias_diagnostics(
    binder: &Binder,
    reservations: &LexicalReservations,
    effects: &mut BTreeMap<RecordTicket, CandidateEffects>,
) {
    for failure in binder.local_ambient_export_alias_failures() {
        let module_ordinal = ModuleOrdinal::new(
            usize::try_from(failure.original_module.0)
                .expect("original module ordinal fits checker ownership"),
        );
        let owner = reservations
            .export_alias_owner(module_ordinal, failure.local_span)
            .expect("local ambient export alias must have an exact lexical owner");
        effects
            .entry(owner.ticket)
            .or_insert_with(|| CandidateEffects::new(owner.ticket))
            .diagnostic(match failure.kind {
                LocalAmbientExportAliasFailureKind::Missing => {
                    Diagnostic::cannot_find_name(failure.local_span, &failure.local_name)
                }
                LocalAmbientExportAliasFailureKind::NonLocal => {
                    Diagnostic::cannot_export_non_local(failure.local_span, &failure.local_name)
                }
            });
    }
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
    decl_id: LegacyTypeStorageId,
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

fn seed_prelude_intrinsics(
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
        ("ThisType", wk.this_type),
        ("OmitThisParameter", wk.omit_this_parameter),
    ] {
        if let Some(decl_id) = type_decl_id(binder, binder.prelude_module, name) {
            if let Some(slot) = type_resolved.get_mut(decl_id.index()) {
                *slot = Some(marker);
            }
        }
    }
}

fn attach_type_decl_owners(
    reservations: &mut LexicalReservations,
    module_ordinal: ModuleOrdinal,
    binder: &Binder,
    scope: ScopeId,
    program: &Program<'_>,
) {
    for declaration in binder
        .declarations
        .iter()
        .filter(|declaration| declaration.site.module == scope)
    {
        reservations
            .attach_declaration_owner(
                declaration.id,
                module_ordinal,
                declaration.kind,
                declaration.site.declaration_span,
                declaration.site.binding_span,
            )
            .expect("source declaration must have its exact lexical event owner");
    }

    walk_type_decls(
        binder,
        scope,
        program,
        &mut |declaration_scope, owner_start, declaration| {
            let name = match declaration {
                TopTypeDecl::Interface(declaration) => Some(declaration.id.name.as_str()),
                TopTypeDecl::Alias(declaration) => Some(declaration.id.name.as_str()),
                TopTypeDecl::Class(declaration) => {
                    declaration.id.as_ref().map(|id| id.name.as_str())
                }
            };
            let Some(decl_id) = name.and_then(|name| type_decl_id(binder, declaration_scope, name))
            else {
                return;
            };
            reservations
                .attach_type_decl_owner(decl_id, module_ordinal, owner_start)
                .expect("named type declaration must have a lexical owner");
        },
    );
}

fn attach_class_bindings(
    reservations: &mut LexicalReservations,
    module_ordinal: ModuleOrdinal,
    binder: &Binder,
    scope: ScopeId,
    program: &Program<'_>,
    declarations: &[TypeDecl<'_>],
) {
    walk_type_decls(
        binder,
        scope,
        program,
        &mut |declaration_scope, _, declaration| {
            let TopTypeDecl::Class(class) = declaration else {
                return;
            };
            let Some(name) = class.id.as_ref().map(|id| id.name.as_str()) else {
                return;
            };
            let Some(site) = reservations.class_at(module_ordinal, class.span.start) else {
                return;
            };
            let Some(type_decl) = type_decl_id(binder, declaration_scope, name) else {
                return;
            };
            let Some(value_decl) = value_decl_id(binder, declaration_scope, name) else {
                return;
            };
            let Some(TypeDecl::Class {
                class_id, params, ..
            }) = declarations.get(type_decl.index())
            else {
                return;
            };
            reservations
                .attach_class_binding(
                    site,
                    ClassBinding {
                        class_id: *class_id,
                        type_decl,
                        value_decl,
                        header_type_params: params.clone(),
                    },
                )
                .expect("one class binding attachment per lexical class site");
        },
    );
}

fn finish_event_effects(
    pass: &mut Pass<'_, '_>,
) -> BTreeMap<ModuleOrdinal, (Vec<Diagnostic>, Vec<IncompleteSurface>)> {
    let pending = std::mem::take(&mut pass.pending_effects);
    for mut effects in pending {
        for obligation in std::mem::take(&mut effects.obligations) {
            let outcome = SemanticQueryCoordinator::new(
                pass.interner,
                &pass.published_classes,
                &mut pass.semantic_queries,
                &mut pass.next_type_param,
            )
            .is_assignable(obligation.src, obligation.tgt);
            match outcome {
                RelationOutcome::Yes => {}
                RelationOutcome::No(chain) => {
                    effects.records.diagnostic(emit_obligation_failure(
                        pass.interner.store(),
                        &obligation,
                        chain.head(),
                    ));
                }
                RelationOutcome::Exhausted(Exhaustion::ClassProjectionBudget) => {
                    effects.records.diagnostic(emit_exhausted_obligation(
                        pass.interner.store(),
                        &obligation,
                    ));
                    effects.records.incomplete(IncompleteSurface::new(
                        "relation/class-projection-budget",
                        obligation.src_span,
                        "class projection budget exhausted",
                    ));
                }
                RelationOutcome::Exhausted(Exhaustion::ClassNotPublished { .. })
                | RelationOutcome::Exhausted(Exhaustion::ClassHeritagePoison { .. })
                | RelationOutcome::Exhausted(Exhaustion::ClassInitializerPoison { .. })
                | RelationOutcome::Exhausted(Exhaustion::ClassSurfacePoison { .. })
                | RelationOutcome::Exhausted(Exhaustion::ClassApplicationArguments(_))
                | RelationOutcome::Exhausted(Exhaustion::EvaluationBudget)
                | RelationOutcome::Exhausted(Exhaustion::EvaluationCycle { .. }) => {}
            }
        }
        for check in std::mem::take(&mut effects.override_checks) {
            if check.base_is_method {
                let strict = SemanticQueryCoordinator::new(
                    pass.interner,
                    &pass.published_classes,
                    &mut pass.semantic_queries,
                    &mut pass.next_type_param,
                )
                .is_assignable(check.own_ty, check.base_ty);
                let (compatible, exhaustion) = match strict {
                    RelationOutcome::Yes => (true, None),
                    RelationOutcome::No(_) => match SemanticQueryCoordinator::new(
                        pass.interner,
                        &pass.published_classes,
                        &mut pass.semantic_queries,
                        &mut pass.next_type_param,
                    )
                    .overload_implementation_compatible(check.own_ty, check.base_ty)
                    {
                        RelationOutcome::Yes => (true, None),
                        RelationOutcome::No(_) => (false, None),
                        RelationOutcome::Exhausted(exhaustion) => (false, Some(exhaustion)),
                    },
                    RelationOutcome::Exhausted(exhaustion) => (false, Some(exhaustion)),
                };
                if matches!(exhaustion, Some(Exhaustion::ClassProjectionBudget)) {
                    effects.records.incomplete(IncompleteSurface::new(
                        "relation/class-projection-budget",
                        check.span,
                        "class projection budget exhausted",
                    ));
                }
                if !compatible {
                    effects
                        .records
                        .diagnostic(Diagnostic::property_override_incompatible(
                            check.span,
                            &check.name,
                            &check.derived,
                            &check.base,
                        ));
                }
            } else {
                let outcome = SemanticQueryCoordinator::new(
                    pass.interner,
                    &pass.published_classes,
                    &mut pass.semantic_queries,
                    &mut pass.next_type_param,
                )
                .is_assignable(check.own_ty, check.base_ty);
                match outcome {
                    RelationOutcome::Yes => {}
                    RelationOutcome::No(chain) => effects.records.diagnostic(
                        Diagnostic::property_override_incompatible(
                            check.span,
                            &check.name,
                            &check.derived,
                            &check.base,
                        )
                        .with_elaboration(render_reason_chain(pass.interner.store(), chain.head())),
                    ),
                    RelationOutcome::Exhausted(exhaustion) => {
                        if exhaustion == Exhaustion::ClassProjectionBudget {
                            effects.records.incomplete(IncompleteSurface::new(
                                "relation/class-projection-budget",
                                check.span,
                                "class projection budget exhausted",
                            ));
                        }
                        effects
                            .records
                            .diagnostic(Diagnostic::property_override_incompatible(
                                check.span,
                                &check.name,
                                &check.derived,
                                &check.base,
                            ));
                    }
                }
            }
        }
        pass.event_store
            .commit(effects.records)
            .expect("each lexical record owner completes exactly once");
    }

    let mut by_module: BTreeMap<ModuleOrdinal, (Vec<Diagnostic>, Vec<IncompleteSurface>)> =
        BTreeMap::new();
    let records = std::mem::take(&mut pass.event_store)
        .finish()
        .expect("all lexically preallocated record owners must be completed");
    for (key, record) in records {
        let channels = by_module.entry(key.module_ordinal).or_default();
        match record {
            events::CheckerRecord::Diagnostic(diagnostic) => channels.0.push(diagnostic),
            events::CheckerRecord::Incomplete(incomplete) => channels.1.push(incomplete),
        }
    }
    by_module
}

impl Pass<'_, '_> {
    /// Run one lexically owned producer and retain its effects until deferred work
    /// has resolved. Nested lexical sites keep distinct preallocated owners.
    pub(in crate::check::checker) fn with_lexical_effects<R>(
        &mut self,
        source_start: u32,
        phase: LexicalOwnerPhase,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let owner = self
            .lexical_events
            .owner_at(self.current_module_ordinal, source_start, phase)
            .expect("lexical owner must be preallocated before semantic execution");
        self.with_ticket_effects(owner.ticket, produce)
    }

    /// Run a producer that already owns an exact preallocated ticket.
    pub(in crate::check::checker) fn with_ticket_effects<R>(
        &mut self,
        owner: events::RecordTicket,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved_event = self.current_event.replace(owner.event);
        self.effect_stack.push(CheckerEffects::new(owner));
        let result = produce(self);
        let effects = self.effect_stack.pop().expect("lexical effect frame");
        self.current_event = saved_event;
        if self.suppress_effects {
            return result;
        }
        if let Some(parent) = self.effect_stack.last_mut() {
            if parent.records.owner() == effects.records.owner() {
                parent.merge(effects);
            } else {
                parent.nested.push(effects);
            }
        } else {
            self.enqueue_effects(effects);
        }
        result
    }

    /// Coalesce repeated phases for one lexical owner before its exactly-once
    /// completion, preserving producer order within the owner group.
    pub(in crate::check::checker) fn enqueue_effects(&mut self, mut effects: CheckerEffects) {
        let nested = std::mem::take(&mut effects.nested);
        if let Some(existing) = self
            .pending_effects
            .iter_mut()
            .find(|existing| existing.records.owner() == effects.records.owner())
        {
            existing.merge(effects);
        } else {
            self.pending_effects.push(effects);
        }
        for child in nested {
            self.enqueue_effects(child);
        }
    }

    pub(in crate::check::checker) fn enqueue_ticket_record(
        &mut self,
        owner: events::RecordTicket,
        record: events::CheckerRecord,
    ) {
        let mut effects = CheckerEffects::new(owner);
        effects.records.record(record);
        self.enqueue_effects(effects);
    }

    /// Isolate a speculative child under the enclosing lexical ticket.
    pub(in crate::check::checker) fn capture_candidate_effects<R>(
        &mut self,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> (R, CheckerEffects) {
        let owner = self
            .effect_stack
            .last()
            .map(|effects| effects.records.owner())
            .expect("speculation requires an enclosing lexical owner");
        self.effect_stack.push(CheckerEffects::new(owner));
        let result = produce(self);
        let effects = self.effect_stack.pop().expect("candidate effect frame");
        (result, effects)
    }

    pub(in crate::check::checker) fn merge_candidate_effects(&mut self, selected: CheckerEffects) {
        self.effect_stack
            .last_mut()
            .expect("selected candidate requires an enclosing lexical owner")
            .merge(selected);
    }

    pub(in crate::check::checker) fn emit_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.effect_stack
            .last_mut()
            .expect("diagnostic requires a lexical owner")
            .records
            .diagnostic(diagnostic);
    }

    pub(in crate::check::checker) fn emit_incomplete(&mut self, incomplete: IncompleteSurface) {
        self.effect_stack
            .last_mut()
            .expect("incomplete record requires a lexical owner")
            .records
            .incomplete(incomplete);
    }

    pub(in crate::check::checker) fn schedule_obligation(&mut self, obligation: AssignObligation) {
        self.effect_stack
            .last_mut()
            .expect("obligation requires a lexical owner")
            .obligations
            .push(obligation);
    }

    pub(in crate::check::checker) fn schedule_override(&mut self, check: OverrideCheck) {
        self.effect_stack
            .last_mut()
            .expect("override check requires a lexical owner")
            .override_checks
            .push(check);
    }

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
        self.emit_incomplete(IncompleteSurface::new(id, span, context));
    }
}

/// Construct a fresh phase-1 [`Pass`]. Fill states come from the decl kind:
/// classes/interfaces/template aliases start `Pending`; resolved placeholders and
/// other declarations start `Done`.
#[cfg(test)]
fn build_pass<'a, 'ast>(
    interner: &'a mut Interner,
    binder: &'a Binder,
    type_decls: Vec<TypeDecl<'ast>>,
    type_resolved: Vec<Option<TypeId>>,
    decl_types: DeclTypes,
    next_type_param: u32,
) -> Pass<'a, 'ast> {
    build_pass_with_reporting(
        interner,
        binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
        PassReporting {
            module_ordinal: ModuleOrdinal::new(0),
            unit_slot: UnitSlot::new(0),
            event_store: EventStore::default(),
            lexical_events: LexicalReservations::default(),
            suppress_effects: false,
        },
    )
}

fn build_pass_with_reporting<'a, 'ast>(
    interner: &'a mut Interner,
    binder: &'a Binder,
    type_decls: Vec<TypeDecl<'ast>>,
    type_resolved: Vec<Option<TypeId>>,
    decl_types: DeclTypes,
    next_type_param: u32,
    reporting: PassReporting,
) -> Pass<'a, 'ast> {
    let pending_effects = reporting
        .lexical_events
        .tickets()
        .into_iter()
        .map(CheckerEffects::new)
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
        current_module_ordinal: reporting.module_ordinal,
        current_unit_slot: reporting.unit_slot,
        event_store: reporting.event_store,
        current_event: None,
        effect_stack: Vec::new(),
        pending_effects,
        lexical_events: reporting.lexical_events,
        suppress_effects: reporting.suppress_effects,
        published_classes: PublishedClasses::empty(),
        semantic_queries: SemanticQueryState::default(),
        class_application_parameters: BTreeMap::new(),
        class_publication_complete: false,
        retained_class_callables: BTreeMap::new(),
        class_body_views: BTreeMap::new(),
        class_super_constructors: BTreeMap::new(),
        type_decls,
        type_resolved,
        type_param_scopes: Vec::new(),
        static_class_type_param_barriers: Vec::new(),
        next_type_param,
        class_parents: FxHashMap::default(),
        class_value_aliases: FxHashMap::default(),
        class_names: FxHashMap::default(),
        template_fill,
        decl_types,
        var_annotation_surfaces: FxHashMap::default(),
        var_value_type_states: FxHashMap::default(),
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
        current_body_this_environment: None,
        current_class: None,
        enclosing_classes: Vec::new(),
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

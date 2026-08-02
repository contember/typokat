//! Statement checking.

use super::assignment::binding_decl_id;
use super::assignment::declared_from_init;
use super::calls::{widen, FunctionReservation};
use super::context::*;
use super::function_groups::{
    FunctionGroupBodyCompletion, FunctionGroupIdentity, FunctionGroupUnavailableCause,
    FunctionNamespacePayload,
};
use super::lexical_events::LexicalOwnerPhase;
use super::replay_index::ReplayOwner;
use crate::binder::declaration::{DeclarationKind, ValueStorageId};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::class_semantics::DemandOutcome;
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic};
use crate::relate::{Reason, RelationOutcome};
use crate::span::Span;
use crate::types::repr::{FunctionType, ObjectType};
use crate::types::store::{Store, TypeId};
use oxc_ast::ast::{
    BindingPattern, BlockStatement, Declaration, Expression, ForInStatement, ForOfStatement,
    ForStatement, ForStatementInit, ForStatementLeft, Function, ObjectPropertyKind, Statement,
    TSModuleDeclaration, TSModuleDeclarationBody, TSType, TSTypeName, TryStatement,
    VariableDeclaration, VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

#[cfg(test)]
thread_local! {
    static NAMESPACE_ALIAS_SITE_LOOKUPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
struct NamespaceAliasLookupScope(u64);

#[cfg(test)]
impl NamespaceAliasLookupScope {
    fn start() -> Self {
        Self(NAMESPACE_ALIAS_SITE_LOOKUPS.get())
    }

    fn finish(self) -> u64 {
        NAMESPACE_ALIAS_SITE_LOOKUPS.get().saturating_sub(self.0)
    }
}

fn parenthesized_identifier<'expr, 'ast>(
    expression: &'expr Expression<'ast>,
) -> Option<&'expr oxc_ast::ast::IdentifierReference<'ast>> {
    match expression {
        Expression::Identifier(identifier) => Some(identifier),
        Expression::ParenthesizedExpression(parenthesized) => {
            parenthesized_identifier(&parenthesized.expression)
        }
        _ => None,
    }
}

fn binding_initializer_member_spans(
    pattern: &BindingPattern<'_>,
    initializer: &Expression<'_>,
) -> Vec<(AssignSourceMember, Span)> {
    let initializer = match initializer {
        Expression::ParenthesizedExpression(parenthesized) => &parenthesized.expression,
        other => other,
    };
    match (pattern, initializer) {
        (BindingPattern::ObjectPattern(_), Expression::ObjectExpression(object)) => object
            .properties
            .iter()
            .filter_map(|member| {
                let ObjectPropertyKind::ObjectProperty(property) = member else {
                    return None;
                };
                let name = property.key.static_name()?.into_owned();
                Some((
                    AssignSourceMember::Property(name),
                    Span::from_oxc(property.span),
                ))
            })
            .collect(),
        (BindingPattern::ArrayPattern(_), Expression::ArrayExpression(array)) => array
            .elements
            .iter()
            .enumerate()
            .filter_map(|(index, element)| {
                element.as_expression().map(|expression| {
                    (
                        AssignSourceMember::Element(index),
                        Span::from_oxc(expression.span()),
                    )
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

struct NamespaceAliasCandidateCollector<'a> {
    binder: &'a crate::binder::Binder,
    module: ScopeId,
    candidates: Vec<(ValueStorageId, ScopeId, String)>,
}

impl<'ast> Visit<'ast> for NamespaceAliasCandidateCollector<'_> {
    fn visit_variable_declaration(&mut self, declaration: &VariableDeclaration<'ast>) {
        if declaration.kind.is_const() {
            for declarator in &declaration.declarations {
                if declarator.type_annotation.is_some() {
                    continue;
                }
                let BindingPattern::BindingIdentifier(alias) = &declarator.id else {
                    continue;
                };
                let Some(source) = declarator.init.as_ref().and_then(parenthesized_identifier)
                else {
                    continue;
                };
                #[cfg(test)]
                NAMESPACE_ALIAS_SITE_LOOKUPS.set(NAMESPACE_ALIAS_SITE_LOOKUPS.get() + 1);
                let Some(alias_declaration) = self.binder.declarations.declaration_at_site(
                    self.module,
                    alias.span.start,
                    DeclarationKind::Variable,
                ) else {
                    continue;
                };
                let (Some(alias_storage), Some(scope)) = (
                    alias_declaration.value_storage,
                    alias_declaration.site.scope,
                ) else {
                    continue;
                };
                self.candidates
                    .push((alias_storage, scope, source.name.to_string()));
            }
        }
        walk::walk_variable_declaration(self, declaration);
    }
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    fn continuation_global_augmentation<'statement, 'program>(
        &self,
        statement: &'statement Statement<'program>,
    ) -> Option<(u32, &'statement [Statement<'program>])> {
        let global = match statement {
            Statement::TSGlobalDeclaration(global) => global,
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(Declaration::TSGlobalDeclaration(global)) => global,
                _ => return None,
            },
            _ => return None,
        };
        self.binder
            .continuation_global_augmentation_body_scope(
                self.current_module,
                global.global_span.start,
            )
            .map(|_| (global.global_span.start, global.body.body.as_slice()))
    }

    fn check_continuation_global_augmentation_body(
        &mut self,
        binding_start: u32,
        body: &[Statement<'_>],
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
    ) -> bool {
        let Some(scope) = self
            .binder
            .continuation_global_augmentation_body_scope(self.current_module, binding_start)
        else {
            return false;
        };
        let mut no_return = None;
        self.check_statement_list_with_surfaces_mode(
            scope,
            body,
            None,
            &mut no_return,
            surfaces,
            true,
        );
        true
    }

    pub(in crate::check::checker) fn reserve_continuation_global_augmentation_surfaces(
        &mut self,
        statements: &[Statement<'_>],
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
    ) {
        for statement in statements {
            let Some((binding_start, body)) = self.continuation_global_augmentation(statement)
            else {
                continue;
            };
            let Some(scope) = self
                .binder
                .continuation_global_augmentation_body_scope(self.current_module, binding_start)
            else {
                continue;
            };
            self.reserve_function_surfaces_into(scope, body, surfaces);
            self.reserve_var_annotation_surfaces(scope, body);
        }
    }

    fn selected_global_augmentation_requires_incomplete(&self, binding_start: u32) -> bool {
        if !self
            .binder
            .global_augmentation_requires_incomplete(self.current_module, binding_start)
        {
            return false;
        }
        self.private_collision_affected.is_empty()
            || self
                .binder
                .global_augmentation_value_storages(self.current_module, binding_start)
                .into_iter()
                .any(|storage| {
                    self.private_collision_affected
                        .contains(&ReplayOwner::Value(storage))
                })
    }

    pub(in crate::check::checker) fn precompute_standalone_namespace_value_aliases(
        &mut self,
        modules: &[(ScopeId, &'ast [Statement<'ast>])],
    ) {
        let mut candidates = Vec::new();
        for (module, statements) in modules {
            let mut collector = NamespaceAliasCandidateCollector {
                binder: self.binder,
                module: *module,
                candidates: Vec::new(),
            };
            for statement in *statements {
                collector.visit_statement(statement);
            }
            candidates.extend(collector.candidates);
        }

        let mut remaining = candidates
            .into_iter()
            .filter_map(|(alias, scope, source)| {
                self.resolve_value_replay(scope, &source)
                    .and_then(|symbol| self.binder.symbols.get(symbol))
                    .and_then(|symbol| symbol.value)
                    .map(|source| (alias, source))
            })
            .collect::<Vec<_>>();
        loop {
            let mut next = Vec::new();
            let mut progressed = false;
            for (alias, source) in remaining {
                let root = self
                    .binder
                    .standalone_namespace_for_storage(source)
                    .map(|_| source)
                    .or_else(|| {
                        self.standalone_namespace_value_aliases
                            .get(&source)
                            .copied()
                    });
                let Some(root) = root else {
                    next.push((alias, source));
                    continue;
                };
                let Some(namespace) = self.binder.standalone_namespace_for_storage(root) else {
                    continue;
                };
                if matches!(
                    self.standalone_namespace_terminal_replay(namespace),
                    Some(super::namespace_values::StandaloneNamespaceTerminal::Ready {
                        storage,
                        ..
                    }) if storage == root
                ) {
                    self.standalone_namespace_value_aliases
                        .insert_local(alias, root)
                        .expect("namespace aliases cannot replace a frozen base row");
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
            remaining = next;
        }
    }

    /// Install the namespace-side half of an admitted function merge before its
    /// statement list reserves callable rows. The namespace lane owns lowering;
    /// this API accepts only its frozen, identity-neutral value payload.
    pub(in crate::check::checker) fn install_function_namespace_payload(
        &mut self,
        scope: ScopeId,
        name: &str,
        payload: FunctionNamespacePayload<Ticket>,
    ) -> bool {
        let Some(identity) =
            super::function_groups::FunctionGroupRegistry::<Ticket>::function_namespace_identity(
                self.binder,
                scope,
                name,
            )
        else {
            return false;
        };
        let symbol = identity.symbol;
        self.function_groups.register(identity);
        self.function_groups
            .install_namespace_payload(symbol, payload);
        self.publish_ready_function_group(symbol);
        true
    }

    /// Check a list of statements in `scope` at the **module top level** (no enclosing
    /// function, so no return context). Each statement flows through the unified
    /// statement walker with an empty return context.
    pub(in crate::check::checker) fn check_statements(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
    ) {
        let mut no_return: Option<TypeId> = None;
        self.check_statement_list(scope, statements, None, &mut no_return);
    }

    /// Walk a statement list, threading the return context, and route consecutive
    /// same-named function declarations through the overload grouping machinery
    /// (M33). Shared by every statement-list context — module top level, blocks,
    /// switch clauses, loop bodies — so a *local* overload set is grouped exactly
    /// like a top-level one (no spurious TK2391; calls select the declared
    /// signatures, not the implementation signature).
    pub(in crate::check::checker) fn check_statement_list(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        // The binder has already made every function name visible in this scope. Reserve
        // its callable signature before any executable statement can resolve that name;
        // bodies still fill at their source declaration, preserving outer type timing.
        let mut surfaces = self.reserve_function_surfaces(scope, statements);
        // Explicit `var` annotations are visible throughout their containing
        // function/module. The reservation does not inspect initializers or add flow.
        self.reserve_var_annotation_surfaces(scope, statements);
        self.reserve_continuation_global_augmentation_surfaces(statements, &mut surfaces);
        self.check_statement_list_with_surfaces(
            scope,
            statements,
            declared_ret,
            inferred,
            &mut surfaces,
        );
    }

    /// Check one statement list using callable surfaces reserved across its whole
    /// lexical container. Switch clauses share one such set while retaining their
    /// own source-local overload grouping.
    pub(in crate::check::checker) fn check_statement_list_with_surfaces(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
    ) {
        self.check_statement_list_with_surfaces_mode(
            scope,
            statements,
            declared_ret,
            inferred,
            surfaces,
            false,
        );
    }

    fn check_statement_list_with_surfaces_mode(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
        ambient: bool,
    ) {
        let mut index = 0;
        while index < statements.len() {
            if let Some((binding_start, body)) =
                self.continuation_global_augmentation(&statements[index])
            {
                self.check_continuation_global_augmentation_body(binding_start, body, surfaces);
                index += 1;
                continue;
            }
            if let Some((name, end)) = function_overload_group(statements, index) {
                self.finalize_function_declaration_group_with_publication(
                    scope,
                    &statements[index..end],
                    name,
                    surfaces,
                    true,
                    ambient,
                );
                index = end;
                continue;
            }
            if let Some(func) = function_decl_from_statement(&statements[index]) {
                self.finalize_function_declaration_with_ambient(scope, func, surfaces, ambient);
                index += 1;
                continue;
            }
            self.check_stmt(scope, &statements[index], declared_ret, inferred);
            index += 1;
        }
    }

    /// Check one statement, threading return context. Narrowing comes from the
    /// pre-built flow graph; `inferred` records the first value return when needed.
    pub(in crate::check::checker) fn check_stmt(
        &mut self,
        scope: ScopeId,
        stmt: &Statement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        match stmt {
            Statement::FunctionDeclaration(func) => {
                self.check_function_declaration(scope, func);
                return;
            }
            Statement::ClassDeclaration(class) => {
                self.check_class(scope, class);
                return;
            }
            _ => {}
        }
        self.with_lexical_effects(stmt.span().start, LexicalOwnerPhase::Immediate, |pass| {
            pass.check_stmt_immediate(scope, stmt, declared_ret, inferred)
        });
    }

    fn check_stmt_immediate(
        &mut self,
        scope: ScopeId,
        stmt: &Statement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    self.check_owned_declarator(scope, decl.kind, declarator);
                }
            }
            Statement::FunctionDeclaration(func) => {
                self.check_function_declaration(scope, func);
            }
            // M11: a `class` body — check each method/constructor body with `this` bound
            // to the instance type (its types are already built in phase 0).
            Statement::ClassDeclaration(class) => {
                self.check_class(scope, class);
            }
            Statement::ExpressionStatement(expr_stmt) => {
                if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                    self.check_assignment(scope, assign);
                } else {
                    // Other expression statements are still inferred so nested calls /
                    // functions inside them are checked (e.g. a bare `f(1)`).
                    self.infer_expr(scope, &expr_stmt.expression);
                }
            }
            Statement::ReturnStatement(ret) => {
                self.check_return(scope, ret, declared_ret, inferred);
            }
            // M7/M23: narrowing lives in the flow graph; the checker just walks the
            // condition + branches (each reference resolves against its flow node).
            Statement::IfStatement(if_stmt) => {
                self.check_if(scope, if_stmt, declared_ret, inferred);
            }
            // A `{ … }` block runs its statements in its own (binder-created) block scope.
            Statement::BlockStatement(block) => {
                self.check_block(scope, block, declared_ret, inferred);
            }
            // M8: `switch` — walk the discriminant + clause bodies.
            Statement::SwitchStatement(switch) => {
                self.check_switch(scope, switch, declared_ret, inferred);
            }
            // M23: a `while` loop — walk the condition + body (the flow graph carries
            // the loop-edge narrowing).
            Statement::WhileStatement(while_stmt) => {
                self.check_while(scope, while_stmt, declared_ret, inferred);
            }
            // A label is transparent to the type-check walk; any block inside it still
            // enters its own binder-created block scope through the usual block case.
            Statement::LabeledStatement(labeled) => {
                self.check_stmt(scope, &labeled.body, declared_ret, inferred);
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(decl) = &export.declaration {
                    self.check_declaration(scope, decl);
                }
            }
            // Loop forms: walk their head (init/condition/incrementor or iteration
            // target/source) and body so nested declarations/assignments/calls are
            // checked. Precise per-iteration narrowing stays deferred to backlog 51 —
            // body references fall back to the declared/START-flow type (sound).
            Statement::ForStatement(for_stmt) => {
                self.check_for(scope, for_stmt, declared_ret, inferred);
            }
            Statement::ForInStatement(for_in) => {
                self.check_for_in(scope, for_in, declared_ret, inferred);
            }
            Statement::ForOfStatement(for_of) => {
                self.check_for_of(scope, for_of, declared_ret, inferred);
            }
            Statement::DoWhileStatement(do_stmt) => {
                self.check_do(scope, do_stmt, declared_ret, inferred);
            }
            // A `throw expr` — type-check the operand so nested calls/assignments inside
            // it are checked (its own value type is irrelevant; the throw is a diverge).
            Statement::ThrowStatement(throw) => {
                self.infer_expr(scope, &throw.argument);
            }
            // `try`/`catch`/`finally`: the three blocks are ordinary statement lists,
            // walked through the existing block walker so nested errors surface (WU4).
            // The catch parameter's type stays unmodeled (recorded incomplete inside).
            Statement::TryStatement(try_stmt) => {
                self.check_try(scope, try_stmt, declared_ret, inferred);
            }
            // Declaration statements the statement checker does not model. Their type/
            // value side is skipped, so record the incomplete surface before dropping
            // (owners on each backlog) instead of exiting clean (WU4). `import`,
            // `export {…}`, type-alias, and interface are handled elsewhere (binder /
            // type-fill / the ExportNamedDeclaration arm) and skip silently.
            Statement::TSEnumDeclaration(_) => {
                self.record_incomplete(
                    "decl/enum-declaration/self",
                    Span::from_oxc(stmt.span()),
                    "enum declaration not modeled",
                );
            }
            Statement::TSModuleDeclaration(declaration)
                if !self.check_prepared_namespace_declaration(declaration)
                    && !module_declaration_is_type_only(declaration) =>
            {
                if self
                    .binder
                    .library_module_reporting_owns(self.current_module, declaration.span.start)
                {
                    return;
                }
                self.record_incomplete(
                    "decl/module-declaration/self",
                    Span::from_oxc(stmt.span()),
                    "namespace/module declaration has an unmodeled value surface",
                );
            }
            Statement::TSModuleDeclaration(_) => {}
            Statement::TSGlobalDeclaration(global)
                if self
                    .selected_global_augmentation_requires_incomplete(global.global_span.start) =>
            {
                self.record_incomplete(
                    "decl/global-declaration/self",
                    Span::from_oxc(stmt.span()),
                    "global augmentation value publication not modeled",
                );
            }
            Statement::TSGlobalDeclaration(_) => {}
            Statement::TSImportEqualsDeclaration(_) => {
                self.record_incomplete(
                    "decl/import-equals/self",
                    Span::from_oxc(stmt.span()),
                    "import = declaration not modeled",
                );
            }
            Statement::ExportAllDeclaration(_) => {
                self.record_incomplete(
                    "decl/export-all/self",
                    Span::from_oxc(stmt.span()),
                    "export * not modeled",
                );
            }
            Statement::ExportDefaultDeclaration(_) => {
                if self
                    .binder
                    .library_export_default_reporting_owns(self.current_module, stmt.span().start)
                {
                    return;
                }
                self.record_incomplete(
                    "decl/export-default/self",
                    Span::from_oxc(stmt.span()),
                    "export default not modeled",
                );
            }
            Statement::TSExportAssignment(_) => {
                self.record_incomplete(
                    "decl/export-assignment/self",
                    Span::from_oxc(stmt.span()),
                    "export = not modeled",
                );
            }
            Statement::TSNamespaceExportDeclaration(export)
                if self
                    .binder
                    .umd_export_requires_incomplete(self.current_module, export.span.start) =>
            {
                self.record_incomplete(
                    "decl/namespace-export/self",
                    Span::from_oxc(stmt.span()),
                    "export as namespace not modeled",
                );
            }
            Statement::TSNamespaceExportDeclaration(_) => {}
            // Remaining forms carry no in-scope child the statement checker hides:
            // supported-elsewhere (`import`, type-alias, interface), design-OOS
            // (`with`/`debugger`/`;`), and the flow-owned `break`/`continue`.
            _ => {}
        }
    }

    /// Check a `try`/`catch`/`finally` (WU4). Each block is walked through the
    /// existing block walker, so nested TK diagnostics surface exactly as in a plain
    /// block. Flow narrowing does not cross into try blocks (the pre-pass leaves them
    /// conservative — see `flowgraph`); the catch variable is bound by the binder but
    /// its type is not modeled (tsc types it `unknown`), so record the incomplete
    /// catch-parameter surface rather than under-reporting silently.
    fn check_try(
        &mut self,
        scope: ScopeId,
        try_stmt: &TryStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        self.check_block(scope, &try_stmt.block, declared_ret, inferred);
        if let Some(handler) = &try_stmt.handler {
            if let Some(param) = &handler.param {
                self.record_incomplete(
                    "stmt-check/try-statement/catch-param",
                    Span::from_oxc(param.span),
                    "catch parameter type not modeled (tsc types it unknown)",
                );
            }
            self.check_block(scope, &handler.body, declared_ret, inferred);
        }
        if let Some(finalizer) = &try_stmt.finalizer {
            self.check_block(scope, finalizer, declared_ret, inferred);
        }
    }

    fn check_declaration(&mut self, scope: ScopeId, decl: &Declaration<'_>) {
        match decl {
            Declaration::VariableDeclaration(var) => {
                for declarator in &var.declarations {
                    self.check_owned_declarator(scope, var.kind, declarator);
                }
            }
            Declaration::FunctionDeclaration(func) => {
                self.check_function_declaration(scope, func);
            }
            Declaration::ClassDeclaration(class) => {
                self.check_class(scope, class);
            }
            // Exported declaration forms the statement checker does not model — account
            // for them before dropping (WU4). Type-alias / interface are handled by
            // type-fill and skip silently.
            Declaration::TSEnumDeclaration(_) => {
                self.record_incomplete(
                    "decl/enum-declaration/self",
                    Span::from_oxc(decl.span()),
                    "enum declaration not modeled",
                );
            }
            Declaration::TSModuleDeclaration(declaration)
                if !self.check_prepared_namespace_declaration(declaration)
                    && !module_declaration_is_type_only(declaration) =>
            {
                if self
                    .binder
                    .library_module_reporting_owns(self.current_module, declaration.span.start)
                {
                    return;
                }
                self.record_incomplete(
                    "decl/module-declaration/self",
                    Span::from_oxc(decl.span()),
                    "namespace/module declaration has an unmodeled value surface",
                );
            }
            Declaration::TSModuleDeclaration(_) => {}
            Declaration::TSGlobalDeclaration(global)
                if self
                    .selected_global_augmentation_requires_incomplete(global.global_span.start) =>
            {
                self.record_incomplete(
                    "decl/global-declaration/self",
                    Span::from_oxc(decl.span()),
                    "global augmentation value publication not modeled",
                );
            }
            Declaration::TSGlobalDeclaration(_) => {}
            Declaration::TSImportEqualsDeclaration(_) => {
                self.record_incomplete(
                    "decl/import-equals/self",
                    Span::from_oxc(decl.span()),
                    "import = declaration not modeled",
                );
            }
            _ => {}
        }
    }

    /// Check `return expr` against the declared return type, or fold the value return's
    /// widened type into the running inferred-return union. Bare `return;` contributes none.
    fn check_return(
        &mut self,
        scope: ScopeId,
        ret: &oxc_ast::ast::ReturnStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        let Some(arg) = &ret.argument else {
            return;
        };
        match declared_ret {
            // Declared return type: check the returned expression against it.
            Some(tgt) => {
                let Some((src, src_span)) = self.infer_initializer(scope, arg, Some(tgt)) else {
                    return;
                };
                match self.check_excess_properties_for_target(arg, tgt) {
                    DemandOutcome::Ready(diagnostics) => {
                        for diagnostic in diagnostics {
                            self.emit_diagnostic(diagnostic);
                        }
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(DemandOutcome::Exhausted(exhaustion), src_span);
                        return;
                    }
                }
                self.schedule_obligation(AssignObligation {
                    src,
                    tgt,
                    src_span,
                    source_member_spans: Vec::new(),
                    kind: ObligationKind::Assignment,
                });
            }
            // No annotation: the inferred return type is the union of every value
            // return's widened type, independent of visitation order (`union` canonicalizes
            // its member set). Stopping at the first return would drop `string | number`
            // down to whichever return was seen first.
            None => {
                let Some((src, _)) = self.infer_expr(scope, arg) else {
                    return;
                };
                let widened = widen(self.interner, src);
                *inferred = Some(match *inferred {
                    Some(existing) => self.interner.union(vec![existing, widened]),
                    None => widened,
                });
            }
        }
    }

    /// Check a `{ … }` block (M7): descend into its own lexical block scope (created by
    /// the binder, keyed by span start) and run its statements there with the current
    /// return context and narrowing environment. A block that the binder did not record
    /// (defensive — never expected) falls back to the enclosing scope.
    fn check_block(
        &mut self,
        scope: ScopeId,
        block: &BlockStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        let block_scope = self.block_scope(scope, block.span.start);
        self.check_statement_list(block_scope, &block.body, declared_ret, inferred);
    }

    /// The lexical scope for a block-like container (an explicit block or switch).
    /// A missing binder entry is defensive and preserves the enclosing scope.
    fn block_scope(&self, scope: ScopeId, span_start: u32) -> ScopeId {
        self.binder
            .block_scopes
            .get(&(self.current_module, span_start))
            .copied()
            .unwrap_or(scope)
    }

    /// The lexical scope the binder created for a loop's head (holding a `for (let i…)`
    /// initializer or a `for-in`/`for-of` iteration variable), keyed by the loop
    /// statement's span start. Falls back to the enclosing scope defensively.
    fn loop_head_scope(&self, scope: ScopeId, span_start: u32) -> ScopeId {
        self.binder
            .block_scopes
            .get(&(self.current_module, span_start))
            .copied()
            .unwrap_or(scope)
    }

    /// Check a C-style `for (init; test; update) body`. The head scope holds a
    /// `let`/`const` initializer so it is in scope for test/update/body; each part is
    /// walked and the body checked.
    fn check_for(
        &mut self,
        scope: ScopeId,
        for_stmt: &ForStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        let head = self.loop_head_scope(scope, for_stmt.span.start);
        if let Some(init) = &for_stmt.init {
            match init {
                ForStatementInit::VariableDeclaration(decl) => {
                    for declarator in &decl.declarations {
                        self.check_owned_declarator(head, decl.kind, declarator);
                    }
                }
                other => {
                    if let Some(expr) = other.as_expression() {
                        self.infer_expr(head, expr);
                    }
                }
            }
        }
        if let Some(test) = &for_stmt.test {
            self.infer_expr(head, test);
        }
        if let Some(update) = &for_stmt.update {
            self.infer_expr(head, update);
        }
        self.check_stmt(head, &for_stmt.body, declared_ret, inferred);
    }

    /// Check `for (left in right) body`. The iteration variable is always typed
    /// `string` (for-in enumerates string keys); the source object is walked in the
    /// enclosing scope.
    fn check_for_in(
        &mut self,
        scope: ScopeId,
        for_in: &ForInStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        let head = self.loop_head_scope(scope, for_in.span.start);
        self.infer_expr(scope, &for_in.right);
        let key_ty = self.interner.well_known().string;
        self.declare_for_left(head, &for_in.left, key_ty);
        self.check_stmt(head, &for_in.body, declared_ret, inferred);
    }

    /// Check `for (left of right) body`. The iteration variable is typed as the
    /// source's element type (array element / tuple-element union); the source is
    /// walked in the enclosing scope.
    fn check_for_of(
        &mut self,
        scope: ScopeId,
        for_of: &ForOfStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        let head = self.loop_head_scope(scope, for_of.span.start);
        let element_ty = match self.infer_expr(scope, &for_of.right) {
            Some((source, _)) => self.iterated_element_type(source),
            None => self.interner.well_known().error,
        };
        self.declare_for_left(head, &for_of.left, element_ty);
        self.check_stmt(head, &for_of.body, declared_ret, inferred);
    }

    /// Check `do body while (test)`: the body runs first (its own block scope), then
    /// the condition is walked.
    fn check_do(
        &mut self,
        scope: ScopeId,
        do_stmt: &oxc_ast::ast::DoWhileStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        self.check_stmt(scope, &do_stmt.body, declared_ret, inferred);
        self.infer_expr(scope, &do_stmt.test);
    }

    /// Record the declared type of a `for-in`/`for-of` iteration variable. Only a
    /// `let`/`const` binding-identifier target is typed; a pre-declared assignment
    /// target (`for (x of xs)`) reassigns an existing binding and its per-iteration
    /// assignability is deferred (backlog 51).
    fn declare_for_left(
        &mut self,
        scope: ScopeId,
        left: &ForStatementLeft<'_>,
        element_ty: TypeId,
    ) {
        let ForStatementLeft::VariableDeclaration(decl) = left else {
            // A pre-declared assignment target (`for (s of xs)`): the per-iteration
            // assignability of the element to the existing binding is not checked
            // (WU4 accounting; `let s: string; for (s of [1,2,3])` should error).
            self.record_incomplete(
                "stmt-check/assignment-target/self",
                Span::from_oxc(left.span()),
                "for-in/of assignment target not typed",
            );
            return;
        };
        for declarator in &decl.declarations {
            if let Some(decl_id) =
                variable_declaration_decl_id(self.binder, scope, decl.kind, &declarator.id)
            {
                let declared = match self.take_var_annotation_surface(decl.kind, declarator) {
                    Some(annotation) => annotation.unwrap_or(element_ty),
                    None => element_ty,
                };
                self.publish_variable_decl_type(
                    scope,
                    decl.kind,
                    &declarator.id,
                    decl_id,
                    declared,
                );
            }
        }
    }

    /// The element type yielded by iterating `ty` in a `for-of`: an array's element or
    /// a tuple's element union. Other iterables (strings, iterators) need `lib.d.ts`
    /// and are out of subset → the error type (no diagnostic, no cascade).
    fn iterated_element_type(&mut self, ty: TypeId) -> TypeId {
        let ty = self.apparent_type(ty);
        let ty = self.interner.store().readonly_operand(ty).unwrap_or(ty);
        let store = self.interner.store();
        if let Some(array) = store.array_type(ty) {
            return array.element;
        }
        if store.tag(ty) == crate::types::repr::TypeTag::Tuple {
            let elements = store
                .tuple_type(ty)
                .map(|t| t.elements.clone())
                .unwrap_or_default();
            return self.interner.union(elements);
        }
        self.interner.well_known().error
    }

    /// Check an initializer against an optional annotation using the declaration
    /// path: contextual typing, assignability, and excess-property checks. Shared
    /// by variables and class fields so they cannot drift.
    pub(in crate::check::checker) fn check_annotated_initializer(
        &mut self,
        scope: ScopeId,
        annotation: Option<TypeId>,
        init: &Expression<'_>,
        diagnostic_span: Span,
    ) -> Option<(TypeId, Span)> {
        self.check_annotated_initializer_with_member_spans(
            scope,
            annotation,
            init,
            diagnostic_span,
            Vec::new(),
        )
    }

    pub(in crate::check::checker) fn check_pattern_annotated_initializer(
        &mut self,
        scope: ScopeId,
        annotation: Option<TypeId>,
        pattern: &BindingPattern<'_>,
        init: &Expression<'_>,
    ) -> Option<(TypeId, Span)> {
        self.check_annotated_initializer_with_member_spans(
            scope,
            annotation,
            init,
            Span::from_oxc(pattern.span()),
            binding_initializer_member_spans(pattern, init),
        )
    }

    fn check_annotated_initializer_with_member_spans(
        &mut self,
        scope: ScopeId,
        annotation: Option<TypeId>,
        init: &Expression<'_>,
        diagnostic_span: Span,
        source_member_spans: Vec<(AssignSourceMember, Span)>,
    ) -> Option<(TypeId, Span)> {
        let initializer = self.infer_initializer(scope, init, annotation);

        // Both sides present: assignability reports at the binding/name fallback or an
        // offending destructured initializer member; fresh literals also check excess keys.
        if let (Some(ann), Some((init_ty, init_span))) = (annotation, initializer) {
            match self.check_excess_properties_for_target(init, ann) {
                DemandOutcome::Ready(diagnostics) => {
                    for diagnostic in diagnostics {
                        self.emit_diagnostic(diagnostic);
                    }
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), init_span);
                }
            }
            let obligation = AssignObligation {
                src: init_ty,
                tgt: ann,
                src_span: diagnostic_span,
                source_member_spans,
                kind: ObligationKind::Assignment,
            };
            if let Some(owner) = self
                .lexical_events
                .initializer_owner_at(self.current_source, init_span.start)
            {
                self.with_ticket_effects(owner, move |pass| {
                    pass.schedule_obligation(obligation);
                });
            } else {
                self.schedule_obligation(obligation);
            }
        }

        initializer
    }

    /// Check one variable declarator and record its declared/inferred type. M18
    /// tuple-context array literals are typed positionally as tuples; otherwise
    /// array literals keep the M17 array inference path.
    fn check_declarator(
        &mut self,
        scope: ScopeId,
        kind: VariableDeclarationKind,
        declarator: &VariableDeclarator<'_>,
    ) {
        let decl_id = variable_declaration_decl_id(self.binder, scope, kind, &declarator.id);

        // Lower the annotation first (independent of the initializer; emits no
        // initializer-dependent diagnostics) so it can provide a **tuple context** for an
        // array-literal initializer (M18 contextual typing).
        let annotation = match self.take_var_annotation_surface(kind, declarator) {
            Some(annotation) => annotation,
            None => match declarator.type_annotation.as_ref() {
                Some(ann) => self.lower_annotation(scope, &ann.type_annotation),
                None => None,
            },
        };

        // Infer/check the initializer against the annotation, including M18 tuple
        // contextual typing for array literals.
        let initializer = declarator.init.as_ref().and_then(|init| {
            self.check_pattern_annotated_initializer(scope, annotation, &declarator.id, init)
        });

        self.record_class_value_alias(scope, kind, declarator, decl_id);

        // F4: object destructuring bindings run M13 access checks against the
        // initializer type; binding the destructured names' types is deferred.
        if let BindingPattern::ObjectPattern(object) = &declarator.id {
            if let Some((source, _)) = &initializer {
                match self.demand_apparent_type(*source) {
                    DemandOutcome::Ready(source) => {
                        self.check_object_pattern_access(object, source);
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(
                            DemandOutcome::Exhausted(exhaustion),
                            Span::from_oxc(object.span),
                        );
                    }
                }
            }
        }

        // The declared type the symbol resolves to: annotation wins; otherwise the
        // (possibly widened) initializer type.
        let declared = match (annotation, &initializer) {
            (Some(ann), _) => Some(ann),
            (None, Some((init_ty, _))) => Some(declared_from_init(self.interner, kind, *init_ty)),
            (None, None) => None,
        };
        if let (Some(decl_id), Some(ty)) = (decl_id, declared) {
            self.publish_variable_decl_type(scope, kind, &declarator.id, decl_id, ty);
        }
    }

    fn check_owned_declarator(
        &mut self,
        scope: ScopeId,
        kind: VariableDeclarationKind,
        declarator: &VariableDeclarator<'_>,
    ) {
        let replay_owner = self.replay_trace.as_ref().and_then(|_| {
            variable_declaration_decl_id(self.binder, scope, kind, &declarator.id)
                .map(super::replay_index::ReplayOwner::Value)
        });
        self.with_lexical_effects(declarator.span.start, LexicalOwnerPhase::Deferred, |pass| {
            match replay_owner {
                Some(owner) => pass.with_replay_owner(owner, |pass| {
                    pass.check_declarator(scope, kind, declarator)
                }),
                None => pass.check_declarator(scope, kind, declarator),
            }
        });
    }

    /// Preserve class-only `new` facts through exactly one `const Alias = Class`
    /// declaration. Chained aliases and non-identifier initializers stay outside this
    /// narrow syntactic slice.
    fn record_class_value_alias(
        &mut self,
        scope: ScopeId,
        kind: VariableDeclarationKind,
        declarator: &VariableDeclarator<'_>,
        alias_decl: Option<ValueStorageId>,
    ) {
        if !kind.is_const() {
            return;
        }
        let Some(alias_decl) = alias_decl else {
            return;
        };
        let Some(Expression::Identifier(source)) = declarator.init.as_ref() else {
            return;
        };
        let Some(class_decl) = self.value_decl_id_replay(scope, source.name.as_str()) else {
            return;
        };
        let published = self
            .class_value_bindings
            .get(&class_decl)
            .is_some_and(|binding| {
                matches!(
                    self.published_class_replay(binding.class_id),
                    crate::class_semantics::DemandOutcome::Ready(_)
                )
            });
        if published {
            self.class_value_aliases
                .insert_local(alias_decl, class_decl)
                .expect("class aliases cannot replace a frozen base row");
        }
    }

    /// Reserve explicit `var` annotations from the complete lexical subtree of one
    /// function/module. Nested functions and classes establish their own boundaries.
    pub(in crate::check::checker) fn reserve_var_annotation_surfaces(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
    ) {
        for statement in statements {
            self.reserve_var_annotation_statement(scope, statement);
        }
    }

    pub(in crate::check::checker) fn reserve_local_type_annotation_surfaces(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
    ) {
        for statement in statements {
            let declaration = match statement {
                Statement::VariableDeclaration(declaration) => declaration,
                Statement::ExportNamedDeclaration(export) => {
                    let Some(Declaration::VariableDeclaration(declaration)) = &export.declaration
                    else {
                        continue;
                    };
                    declaration
                }
                _ => continue,
            };
            for declarator in &declaration.declarations {
                let Some(type_annotation) = &declarator.type_annotation else {
                    continue;
                };
                let TSType::TSTypeReference(reference) = &type_annotation.type_annotation else {
                    continue;
                };
                let TSTypeName::IdentifierReference(identifier) = &reference.type_name else {
                    continue;
                };
                let group = if self.capture_compact_replay_dependencies {
                    self.compact_type_decl_id_replay(scope, identifier.name.as_str())
                } else {
                    self.type_decl_id_replay(scope, identifier.name.as_str())
                };
                let Some(group) = group else {
                    continue;
                };
                let Some(TypeDecl::Interface {
                    reserved, params, ..
                }) = self.type_decls.get(group.index())
                else {
                    continue;
                };
                if group.index() < self.type_decls.published_len() || !params.is_empty() {
                    continue;
                }
                let Some(decl_id) = variable_declaration_decl_id(
                    self.binder,
                    scope,
                    declaration.kind,
                    &declarator.id,
                ) else {
                    continue;
                };
                let surface_key = (self.current_module, declarator.span.start);
                if self.var_annotation_surfaces.contains_key(&surface_key) {
                    continue;
                }
                let reserved = *reserved;
                self.reserve_variable_annotation_type(
                    scope,
                    declaration.kind,
                    &declarator.id,
                    decl_id,
                    reserved,
                );
                self.var_annotation_surfaces.insert(
                    surface_key,
                    VarAnnotationSurface {
                        annotation: Some(reserved),
                    },
                );
            }
        }
    }

    fn reserve_var_annotation_statement(&mut self, scope: ScopeId, statement: &Statement<'_>) {
        match statement {
            Statement::VariableDeclaration(decl) => {
                self.reserve_var_annotation_declaration(scope, decl);
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::VariableDeclaration(decl)) = &export.declaration {
                    self.reserve_var_annotation_declaration(scope, decl);
                }
            }
            Statement::BlockStatement(block) => {
                let block_scope = self.block_scope(scope, block.span.start);
                self.reserve_var_annotation_surfaces(block_scope, &block.body);
            }
            Statement::IfStatement(if_stmt) => {
                self.reserve_var_annotation_statement(scope, &if_stmt.consequent);
                if let Some(alternate) = &if_stmt.alternate {
                    self.reserve_var_annotation_statement(scope, alternate);
                }
            }
            Statement::SwitchStatement(switch) => {
                let switch_scope = self.block_scope(scope, switch.span.start);
                for case in &switch.cases {
                    self.reserve_var_annotation_surfaces(switch_scope, &case.consequent);
                }
            }
            Statement::ForStatement(for_stmt) => {
                let head = self.loop_head_scope(scope, for_stmt.span.start);
                if let Some(ForStatementInit::VariableDeclaration(decl)) = &for_stmt.init {
                    self.reserve_var_annotation_declaration(head, decl);
                }
                self.reserve_var_annotation_statement(head, &for_stmt.body);
            }
            Statement::ForInStatement(for_in) => {
                let head = self.loop_head_scope(scope, for_in.span.start);
                if let ForStatementLeft::VariableDeclaration(decl) = &for_in.left {
                    self.reserve_var_annotation_declaration(head, decl);
                }
                self.reserve_var_annotation_statement(head, &for_in.body);
            }
            Statement::ForOfStatement(for_of) => {
                let head = self.loop_head_scope(scope, for_of.span.start);
                if let ForStatementLeft::VariableDeclaration(decl) = &for_of.left {
                    self.reserve_var_annotation_declaration(head, decl);
                }
                self.reserve_var_annotation_statement(head, &for_of.body);
            }
            Statement::WhileStatement(while_stmt) => {
                self.reserve_var_annotation_statement(scope, &while_stmt.body);
            }
            Statement::DoWhileStatement(do_stmt) => {
                self.reserve_var_annotation_statement(scope, &do_stmt.body);
            }
            Statement::LabeledStatement(labeled) => {
                self.reserve_var_annotation_statement(scope, &labeled.body);
            }
            Statement::TryStatement(try_stmt) => {
                let try_scope = self.block_scope(scope, try_stmt.block.span.start);
                self.reserve_var_annotation_surfaces(try_scope, &try_stmt.block.body);
                if let Some(handler) = &try_stmt.handler {
                    let handler_scope = self.block_scope(scope, handler.body.span.start);
                    self.reserve_var_annotation_surfaces(handler_scope, &handler.body.body);
                }
                if let Some(finalizer) = &try_stmt.finalizer {
                    let finalizer_scope = self.block_scope(scope, finalizer.span.start);
                    self.reserve_var_annotation_surfaces(finalizer_scope, &finalizer.body);
                }
            }
            // Function and class bodies own independent hoist containers.
            Statement::FunctionDeclaration(_) | Statement::ClassDeclaration(_) => {}
            _ => {}
        }
    }

    fn reserve_var_annotation_declaration(
        &mut self,
        scope: ScopeId,
        declaration: &VariableDeclaration<'_>,
    ) {
        for declarator in &declaration.declarations {
            let Some(type_annotation) = &declarator.type_annotation else {
                continue;
            };
            let Some(decl_id) =
                variable_declaration_decl_id(self.binder, scope, declaration.kind, &declarator.id)
            else {
                continue;
            };
            let surface_key = (self.current_module, declarator.span.start);
            if self.var_annotation_surfaces.contains_key(&surface_key) {
                continue;
            }
            let annotation =
                self.with_replay_owner(super::replay_index::ReplayOwner::Value(decl_id), |pass| {
                    let annotation = pass.with_lexical_effects(
                        declarator.span.start,
                        LexicalOwnerPhase::Immediate,
                        |pass| pass.lower_annotation(scope, &type_annotation.type_annotation),
                    );
                    if let Some(annotation) = annotation {
                        pass.reserve_variable_annotation_type(
                            scope,
                            declaration.kind,
                            &declarator.id,
                            decl_id,
                            annotation,
                        );
                    }
                    annotation
                });
            self.var_annotation_surfaces
                .insert(surface_key, VarAnnotationSurface { annotation });
        }
    }

    /// Return the already-lowered explicit `var` annotation at source execution.
    fn take_var_annotation_surface(
        &mut self,
        _kind: VariableDeclarationKind,
        declarator: &VariableDeclarator<'_>,
    ) -> Option<Option<TypeId>> {
        let surface = self
            .var_annotation_surfaces
            .remove(&(self.current_module, declarator.span.start))?;
        Some(surface.annotation)
    }

    /// Publish a variable's type and discard only that symbol's stale flow reads.
    /// An unannotated `var` changes from the defensive error type at its source
    /// declaration, so a pre-declaration read must not survive in `flow_memo`.
    fn publish_variable_decl_type(
        &mut self,
        scope: ScopeId,
        kind: VariableDeclarationKind,
        pattern: &BindingPattern<'_>,
        decl_id: ValueStorageId,
        ty: TypeId,
    ) {
        let replay_decl_id = match (kind.is_var(), pattern) {
            (true, BindingPattern::BindingIdentifier(identifier)) => self
                .private_collision_value_winners_by_name
                .get(identifier.name.as_str())
                .copied()
                .filter(|winner| {
                    self.private_collision_affected
                        .contains(&ReplayOwner::Value(*winner))
                })
                .unwrap_or(decl_id),
            _ => decl_id,
        };
        if kind.is_var() {
            if let Some(function_ty) = self.function_value_type_for_var(scope, kind, pattern) {
                self.set_variable_decl_type(scope, kind, pattern, replay_decl_id, function_ty);
                self.var_value_type_states
                    .insert(replay_decl_id, VarValueTypeState::Existing);
                return;
            }
            match self.var_value_type_states.get(&replay_decl_id) {
                Some(VarValueTypeState::Source) | Some(VarValueTypeState::Existing) => {
                    return;
                }
                Some(VarValueTypeState::Provisional) => {}
                None if self.decl_type_replay(replay_decl_id).is_some()
                    && !self
                        .private_collision_affected
                        .contains(&ReplayOwner::Value(replay_decl_id)) =>
                {
                    self.var_value_type_states
                        .insert(replay_decl_id, VarValueTypeState::Existing);
                    return;
                }
                None => {}
            }
        }
        self.set_variable_decl_type(scope, kind, pattern, replay_decl_id, ty);
        if kind.is_var() {
            self.var_value_type_states
                .insert(replay_decl_id, VarValueTypeState::Source);
        }
    }

    /// Make an explicit `var` annotation available before execution without making
    /// it the shared value type until its own source declarator is checked.
    fn reserve_variable_annotation_type(
        &mut self,
        scope: ScopeId,
        kind: VariableDeclarationKind,
        pattern: &BindingPattern<'_>,
        decl_id: ValueStorageId,
        ty: TypeId,
    ) {
        let replay_decl_id = match (kind.is_var(), pattern) {
            (true, BindingPattern::BindingIdentifier(identifier)) => self
                .private_collision_value_winners_by_name
                .get(identifier.name.as_str())
                .copied()
                .filter(|winner| {
                    self.private_collision_affected
                        .contains(&ReplayOwner::Value(*winner))
                })
                .unwrap_or(decl_id),
            _ => decl_id,
        };
        if let Some(function_ty) = self.function_value_type_for_var(scope, kind, pattern) {
            self.set_variable_decl_type(scope, kind, pattern, replay_decl_id, function_ty);
            self.var_value_type_states
                .insert(replay_decl_id, VarValueTypeState::Existing);
            return;
        }
        match self.var_value_type_states.get(&replay_decl_id) {
            Some(VarValueTypeState::Source) | Some(VarValueTypeState::Existing) => return,
            Some(VarValueTypeState::Provisional) => return,
            None if self.decl_type_replay(replay_decl_id).is_some()
                && !self
                    .private_collision_affected
                    .contains(&ReplayOwner::Value(replay_decl_id)) =>
            {
                self.var_value_type_states
                    .insert(replay_decl_id, VarValueTypeState::Existing);
                return;
            }
            None => {}
        }
        self.set_variable_decl_type(scope, kind, pattern, replay_decl_id, ty);
        self.var_value_type_states
            .insert(replay_decl_id, VarValueTypeState::Provisional);
    }

    /// A `var` merged with a function keeps the callable surface in the value slot.
    fn function_value_type_for_var(
        &self,
        scope: ScopeId,
        kind: VariableDeclarationKind,
        pattern: &BindingPattern<'_>,
    ) -> Option<TypeId> {
        if !kind.is_var() {
            return None;
        }
        let symbol_id = variable_declaration_symbol_id(self.binder, scope, kind, pattern)?;
        let symbol = self.binder.symbols.get(symbol_id)?;
        symbol
            .function_values
            .iter()
            .rev()
            .find_map(|decl_id| self.decl_type_replay(*decl_id))
    }

    fn set_variable_decl_type(
        &mut self,
        scope: ScopeId,
        kind: VariableDeclarationKind,
        pattern: &BindingPattern<'_>,
        decl_id: ValueStorageId,
        ty: TypeId,
    ) {
        let previous = self.decl_types.get(decl_id);
        self.publish_copied_decl_type_replay(decl_id, ty);
        if previous == Some(ty) {
            return;
        }
        let Some(symbol_id) = variable_declaration_symbol_id(self.binder, scope, kind, pattern)
        else {
            return;
        };
        self.flow_memo
            .retain(|(_, memo_symbol), _| *memo_symbol != symbol_id);
    }

    /// Reserve every direct function declaration in a statement list before any of
    /// its executable statements run. Consecutive M33 groups publish their visible
    /// overload object immediately; ordinary functions publish their own signature.
    pub(in crate::check::checker) fn reserve_function_surfaces(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
    ) -> FxHashMap<u32, FunctionReservation<Ticket>> {
        let mut surfaces = FxHashMap::default();
        self.reserve_function_surfaces_into(scope, statements, &mut surfaces);
        surfaces
    }

    /// Reserve direct functions from several statement lists that share one lexical
    /// scope, without treating declarations across list boundaries as consecutive.
    pub(in crate::check::checker) fn reserve_function_surfaces_for_lists(
        &mut self,
        scope: ScopeId,
        statement_lists: &[&[Statement<'_>]],
    ) -> FxHashMap<u32, FunctionReservation<Ticket>> {
        let mut surfaces = FxHashMap::default();
        for statements in statement_lists {
            self.reserve_function_surfaces_into(scope, statements, &mut surfaces);
        }
        surfaces
    }

    fn reserve_function_surfaces_into(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
    ) {
        let mut index = 0;
        while index < statements.len() {
            if let Some((name, end)) = function_overload_group(statements, index) {
                let group = self.register_function_group(scope, name);
                for stmt in &statements[index..end] {
                    let Some(func) = function_decl_from_statement(stmt) else {
                        continue;
                    };
                    self.reserve_function_surface(scope, func, surfaces, false);
                }
                if let Some(group) = group {
                    let symbol = group.symbol;
                    self.reserve_function_group_overload_rows(
                        group,
                        &statements[index..end],
                        surfaces,
                    );
                    self.publish_ready_function_group(symbol);
                } else {
                    self.publish_reserved_overload_group(
                        scope,
                        &statements[index..end],
                        name,
                        surfaces,
                    );
                }
                index = end;
                continue;
            }
            if let Some(func) = function_decl_from_statement(&statements[index]) {
                let group = func
                    .id
                    .as_ref()
                    .and_then(|id| self.register_function_group(scope, id.name.as_str()));
                self.reserve_function_surface(scope, func, surfaces, group.is_none());
                if let Some(group) = group {
                    self.reserve_ordinary_function_group_row(group, func, surfaces);
                }
            }
            index += 1;
        }
    }

    fn register_function_group(
        &mut self,
        scope: ScopeId,
        name: &str,
    ) -> Option<FunctionGroupIdentity> {
        let registered = self
            .binder
            .resolve_value(scope, name)
            .and_then(|symbol| self.function_groups.registered_identity(symbol));
        let mut identity = registered.or_else(|| {
            super::function_groups::FunctionGroupRegistry::<Ticket>::function_namespace_identity(
                self.binder,
                scope,
                name,
            )
        })?;
        if let Some(tail) = self
            .function_group_precedence_tails_by_name
            .get(name)
            .copied()
        {
            identity
                .participants
                .sort_by_key(|participant| *participant == tail);
        }
        let inherited_winner =
            matches!(self.current_source, crate::source::SourceUnit::User { .. })
                .then(|| {
                    self.private_collision_value_winners_by_name
                        .get(name)
                        .copied()
                })
                .flatten();
        let inherited_participants = if inherited_winner.is_some() {
            identity
                .participants
                .iter()
                .copied()
                .filter(|declaration| self.decl_type_replay(*declaration).is_some())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let inherited_call_signatures = inherited_winner
            .and_then(|declaration| self.decl_type_replay(declaration))
            .and_then(|ty| {
                self.interner
                    .store()
                    .object_type(ty)
                    .map(|object| object.call_signatures.clone())
                    .or_else(|| self.interner.store().function_type(ty).map(|_| vec![ty]))
            })
            .unwrap_or_default();
        self.function_groups.register(identity.clone());
        self.function_groups.seed_inherited_publication(
            identity.symbol,
            &inherited_participants,
            inherited_call_signatures,
        );
        Some(identity)
    }

    fn reserve_ordinary_function_group_row(
        &mut self,
        group: FunctionGroupIdentity,
        func: &Function<'_>,
        surfaces: &FxHashMap<u32, FunctionReservation<Ticket>>,
    ) {
        let Some(declaration) = self.function_decl_id(func) else {
            return;
        };
        match surfaces.get(&func.span.start) {
            Some(FunctionReservation::Ready(surface))
                if func.body.is_some() && surface.declared_return.is_none() =>
            {
                self.function_groups
                    .wait_for_body(group.symbol, declaration);
            }
            Some(FunctionReservation::Ready(surface)) => {
                self.function_groups.reserve_public_row(
                    group.symbol,
                    declaration,
                    surface.function_ty,
                );
                self.publish_ready_function_group(group.symbol);
            }
            Some(FunctionReservation::Unavailable(surface)) => {
                self.function_groups.mark_unavailable(
                    group.symbol,
                    FunctionGroupUnavailableCause::Signature,
                    surface.tickets.map(|tickets| tickets.incomplete),
                );
            }
            None => {}
        }
    }

    fn reserve_function_group_overload_rows(
        &mut self,
        group: FunctionGroupIdentity,
        statements: &[Statement<'_>],
        surfaces: &FxHashMap<u32, FunctionReservation<Ticket>>,
    ) {
        for func in statements.iter().filter_map(function_decl_from_statement) {
            let Some(declaration) = self.function_decl_id(func) else {
                continue;
            };
            match surfaces.get(&func.span.start) {
                Some(FunctionReservation::Ready(surface)) if func.body.is_none() => {
                    self.function_groups.reserve_public_row(
                        group.symbol,
                        declaration,
                        surface.function_ty,
                    );
                }
                Some(FunctionReservation::Ready(_)) => {
                    // The implementation row is validation-only.
                    self.function_groups
                        .reserve_validation_only(group.symbol, declaration);
                }
                Some(FunctionReservation::Unavailable(_)) if func.body.is_some() => {
                    // An overload implementation never contributes a public row.
                    self.function_groups
                        .reserve_validation_only(group.symbol, declaration);
                }
                Some(FunctionReservation::Unavailable(surface)) => {
                    self.function_groups.mark_unavailable(
                        group.symbol,
                        FunctionGroupUnavailableCause::Signature,
                        surface.tickets.map(|tickets| tickets.incomplete),
                    );
                    return;
                }
                None => return,
            }
        }
    }

    fn reserve_function_surface(
        &mut self,
        scope: ScopeId,
        func: &Function<'_>,
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
        publish_value: bool,
    ) {
        let replay_owner = self.replay_trace.as_ref().and_then(|_| {
            self.function_decl_id(func)
                .map(super::replay_index::ReplayOwner::Value)
        });
        match replay_owner {
            Some(owner) => self.with_replay_owner(owner, |pass| {
                pass.reserve_function_surface_inner(scope, func, surfaces, publish_value)
            }),
            None => self.reserve_function_surface_inner(scope, func, surfaces, publish_value),
        }
    }

    fn reserve_function_surface_inner(
        &mut self,
        scope: ScopeId,
        func: &Function<'_>,
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
        publish_value: bool,
    ) {
        let tickets = self
            .lexical_events
            .callable_at(
                super::lexical_events::source_ordinal(self.current_source),
                func.span.start,
            )
            .and_then(|site| self.lexical_events.callable(site))
            .map(|callable| callable.tickets);
        let surface = match tickets {
            Some(tickets) => self
                .with_ticket_effects(tickets.signature, |pass| pass.reserve_function(scope, func)),
            None => self.reserve_function(scope, func),
        };
        if publish_value {
            if let FunctionReservation::Ready(surface) = &surface {
                self.publish_function_type(scope, func, surface.function_ty);
            }
        }
        surfaces.insert(func.span.start, surface);
    }

    fn fill_reserved_function_body(
        &mut self,
        scope: ScopeId,
        func: &Function<'_>,
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
        publish_value: bool,
    ) -> bool {
        let replay_owner = self.replay_trace.as_ref().and_then(|_| {
            self.function_decl_id(func)
                .map(super::replay_index::ReplayOwner::Value)
        });
        match replay_owner {
            Some(owner) => self.with_replay_owner(owner, |pass| {
                pass.fill_reserved_function_body_inner(scope, func, surfaces, publish_value)
            }),
            None => self.fill_reserved_function_body_inner(scope, func, surfaces, publish_value),
        }
    }

    fn fill_reserved_function_body_inner(
        &mut self,
        scope: ScopeId,
        func: &Function<'_>,
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
        publish_value: bool,
    ) -> bool {
        let declaration = self.function_decl_id(func);
        let function_group =
            declaration.and_then(|declaration| self.function_groups.symbol_for_value(declaration));
        let Some(surface) = surfaces.remove(&func.span.start) else {
            // Defensive fallback for an AST shape outside the shared statement-list
            // walker (for example a labeled declaration).
            self.check_function_declaration(scope, func);
            return false;
        };
        match surface {
            FunctionReservation::Ready(mut surface) => {
                if let (Some(symbol), Some(declaration)) = (function_group, declaration) {
                    if self.function_groups.is_waiting_for(symbol, declaration) {
                        let private_self = pure_self_recursive_name(func).map(|_| {
                            self.interner.intern_function(FunctionType {
                                type_params: surface.generic_params.clone(),
                                receiver: surface.receiver,
                                params: surface.params.clone(),
                                ret: self.interner.well_known().never,
                            })
                        });
                        let owner = surface.tickets.map(|tickets| tickets.incomplete);
                        self.function_groups
                            .begin_body(symbol, declaration, owner, private_self);
                        let function_ty = self.fill_reserved_function(scope, func, &surface);
                        surface.function_ty = function_ty;
                        match self
                            .function_groups
                            .finish_body(symbol, declaration, function_ty)
                        {
                            FunctionGroupBodyCompletion::Ready => {
                                self.publish_ready_function_group(symbol);
                            }
                            FunctionGroupBodyCompletion::Unavailable { cause, owner } => {
                                let id = match cause {
                                    FunctionGroupUnavailableCause::InferredReturnCycle => {
                                        "decl/function-declaration/inferred-return-cycle"
                                    }
                                    FunctionGroupUnavailableCause::InferredReturnDependency => {
                                        "decl/function-declaration/inferred-return-dependency"
                                    }
                                    FunctionGroupUnavailableCause::Signature
                                    | FunctionGroupUnavailableCause::NamespacePayload => {
                                        unreachable!("body completion owns only inference failures")
                                    }
                                };
                                let emit = |pass: &mut Self| {
                                    pass.record_incomplete(
                                        id,
                                        Span::from_oxc(func.span),
                                        "merged function return inference did not reach a final callable surface",
                                    );
                                };
                                match owner {
                                    Some(owner) => self.with_ticket_effects(owner, emit),
                                    None => emit(self),
                                }
                            }
                        }
                    } else {
                        // Annotated public rows and overload implementations are
                        // already terminal at reservation. Their bodies validate only.
                        surface.function_ty = self.fill_reserved_function(scope, func, &surface);
                    }
                    surfaces.insert(func.span.start, FunctionReservation::Ready(surface));
                    return true;
                }
                let function_ty = self.fill_reserved_function(scope, func, &surface);
                surface.function_ty = function_ty;
                if publish_value {
                    self.publish_function_type(scope, func, function_ty);
                }
                surfaces.insert(func.span.start, FunctionReservation::Ready(surface));
            }
            FunctionReservation::Unavailable(surface) => {
                self.check_retained_function_body(scope, func, &surface);
                surfaces.insert(func.span.start, FunctionReservation::Unavailable(surface));
            }
        }
        true
    }

    fn publish_function_type(&mut self, scope: ScopeId, func: &Function<'_>, function_ty: TypeId) {
        let root_winner = func.id.as_ref().and_then(|id| {
            self.private_collision_value_winners_by_name
                .get(id.name.as_str())
                .copied()
        });
        if matches!(
            self.current_source,
            crate::source::SourceUnit::Library { .. }
        ) {
            if let Some(winner) = root_winner {
                self.publish_copied_decl_type_replay(winner, function_ty);
            }
        }
        if let Some(decl_id) = self.function_decl_id(func) {
            let inherited = matches!(self.current_source, crate::source::SourceUnit::User { .. })
                .then_some(root_winner)
                .flatten()
                .and_then(|winner| {
                    self.decl_type_replay(winner)
                        .map(|inherited| (winner, inherited))
                });
            let function_ty = inherited
                .map(|(winner, inherited)| {
                    let merged = if let Some(mut object) =
                        self.interner.store().object_type(inherited).cloned()
                    {
                        object.call_signatures.push(function_ty);
                        self.interner.intern_object(object)
                    } else if self.interner.store().function_type(inherited).is_some() {
                        self.interner.intern_object(ObjectType {
                            call_signatures: vec![inherited, function_ty],
                            ..Default::default()
                        })
                    } else {
                        function_ty
                    };
                    self.publish_copied_decl_type_replay(winner, merged);
                    merged
                })
                .unwrap_or(function_ty);
            if let Some(symbol_id) = func
                .id
                .as_ref()
                .and_then(|id| self.binder.resolve_value(scope, id.name.as_str()))
            {
                self.publish_symbol_value_type(decl_id, function_ty, symbol_id);
                if let Some(merged_decl) = self
                    .binder
                    .symbols
                    .get(symbol_id)
                    .and_then(|symbol| symbol.value)
                {
                    self.publish_symbol_value_type(merged_decl, function_ty, symbol_id);
                }
            } else {
                self.publish_copied_decl_type_replay(decl_id, function_ty);
            }
        }
    }

    fn publish_ready_function_group(&mut self, symbol: SymbolId) {
        let replay_owner = self.replay_trace.as_ref().and_then(|_| {
            self.binder
                .symbols
                .get(symbol)
                .and_then(|binding| binding.value)
                .map(super::replay_index::ReplayOwner::Value)
        });
        match replay_owner {
            Some(owner) => self.with_replay_owner(owner, |pass| {
                pass.publish_ready_function_group_inner(symbol)
            }),
            None => self.publish_ready_function_group_inner(symbol),
        }
    }

    fn publish_ready_function_group_inner(&mut self, symbol: SymbolId) {
        let Some(publication) = self.function_groups.publication_plan(symbol) else {
            return;
        };
        assert_eq!(publication.symbol, symbol);
        let canonical_owner = publication
            .participants
            .first()
            .copied()
            .map(super::replay_index::ReplayOwner::Value);
        if let Some(canonical_owner) = canonical_owner {
            self.with_replay_owner(canonical_owner, |pass| {
                for declaration in &publication.participants {
                    let participant = super::replay_index::ReplayOwner::Value(*declaration);
                    if participant != canonical_owner {
                        pass.record_replay_demand(participant);
                    }
                }
            });
        }
        for declaration in &publication.participants {
            if publication.prepublished_participants.contains(declaration) {
                continue;
            }
            assert!(
                self.decl_type_replay(*declaration).is_none()
                    || (self
                        .private_collision_affected
                        .contains(&super::replay_index::ReplayOwner::Value(*declaration))
                        && self.decl_types.is_unoverridden_frozen_slot(*declaration)),
                "function group participant was published before the atomic object"
            );
        }
        let ty = self.interner.intern_object(ObjectType {
            properties: publication.properties,
            call_signatures: publication.call_signatures,
            ..Default::default()
        });
        for declaration in publication.participants {
            let participant = super::replay_index::ReplayOwner::Value(declaration);
            self.with_replay_owner(participant, |pass| {
                if canonical_owner.is_some_and(|canonical| canonical != participant) {
                    pass.record_replay_demand(canonical_owner.expect("canonical participant"));
                }
                pass.publish_copied_decl_type_replay(declaration, ty);
            });
        }
        self.flow_memo
            .retain(|(_, memo_symbol), _| *memo_symbol != symbol);
        self.function_groups.mark_published(symbol, ty);
    }

    /// Publish a callable type into one declaration sharing `symbol_id`, evicting
    /// only that symbol's stale flow results when its visible type changes.
    fn publish_symbol_value_type(
        &mut self,
        decl_id: ValueStorageId,
        ty: TypeId,
        symbol_id: SymbolId,
    ) {
        let previous = self.decl_types.get(decl_id);
        self.publish_copied_decl_type_replay(decl_id, ty);
        if previous != Some(ty) {
            self.flow_memo
                .retain(|(_, memo_symbol), _| *memo_symbol != symbol_id);
        }
    }

    fn function_decl_id(&self, func: &Function<'_>) -> Option<ValueStorageId> {
        self.binder
            .fn_decl_ids
            .get(&(self.current_module, func.span.start))
            .copied()
            .or_else(|| {
                let crate::source::SourceUnit::Library { file_ordinal } = self.current_source
                else {
                    return None;
                };
                self.private_library_value_owner_by_site
                    .get(&(file_ordinal, func.span.start))
                    .copied()
                    .flatten()
            })
    }

    fn publish_reserved_overload_group(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        name: &str,
        surfaces: &FxHashMap<u32, FunctionReservation<Ticket>>,
    ) {
        let mut signatures = Vec::new();
        for func in statements.iter().filter_map(function_decl_from_statement) {
            let Some(surface) = surfaces.get(&func.span.start) else {
                return;
            };
            let FunctionReservation::Ready(surface) = surface else {
                return;
            };
            if func.body.is_none() {
                signatures.push(surface.function_ty);
            }
        }
        if signatures.is_empty() {
            return;
        }
        let overload_ty = self.interner.intern_object(ObjectType {
            call_signatures: signatures,
            ..Default::default()
        });
        let implementation_decl = statements
            .iter()
            .filter_map(function_decl_from_statement)
            .filter(|func| func.body.is_some())
            .filter_map(|func| self.function_decl_id(func))
            .next_back();
        self.expose_overload_value(scope, name, implementation_decl, overload_ty);
    }

    /// Check a function declaration outside the shared statement-list prepass and
    /// bind its completed function type. This keeps uncommon direct walker paths
    /// behaviorally equivalent without introducing a second declaration model.
    fn check_function_declaration(&mut self, scope: ScopeId, func: &Function<'_>) {
        let mut surfaces = FxHashMap::default();
        let group = func
            .id
            .as_ref()
            .and_then(|id| self.register_function_group(scope, id.name.as_str()));
        self.reserve_function_surface(scope, func, &mut surfaces, group.is_none());
        if let Some(group) = group {
            self.reserve_ordinary_function_group_row(group, func, &surfaces);
        }
        if !self.fill_reserved_function_body(scope, func, &mut surfaces, true) {
            return;
        }
        if func.body.is_none() && !func.declare {
            let ticket = surfaces
                .get(&func.span.start)
                .and_then(|surface| match surface {
                    FunctionReservation::Ready(surface) => surface.tickets,
                    FunctionReservation::Unavailable(surface) => surface.tickets,
                })
                .map(|tickets| tickets.deferred);
            match ticket {
                Some(ticket) => self.with_ticket_effects(ticket, |pass| {
                    pass.emit_diagnostic(Diagnostic::overload_missing_implementation(
                        Span::from_oxc(func.span),
                    ));
                }),
                None => self.emit_diagnostic(Diagnostic::overload_missing_implementation(
                    Span::from_oxc(func.span),
                )),
            }
        }
    }

    fn finalize_function_declaration_with_ambient(
        &mut self,
        scope: ScopeId,
        func: &Function<'_>,
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
        ambient: bool,
    ) {
        if !self.fill_reserved_function_body(scope, func, surfaces, true) {
            return;
        }
        if func.body.is_none() && !func.declare && !ambient {
            let ticket = surfaces
                .get(&func.span.start)
                .and_then(|surface| match surface {
                    FunctionReservation::Ready(surface) => surface.tickets,
                    FunctionReservation::Unavailable(surface) => surface.tickets,
                })
                .map(|tickets| tickets.deferred);
            match ticket {
                Some(ticket) => self.with_ticket_effects(ticket, |pass| {
                    pass.emit_diagnostic(Diagnostic::overload_missing_implementation(
                        Span::from_oxc(func.span),
                    ));
                }),
                None => self.emit_diagnostic(Diagnostic::overload_missing_implementation(
                    Span::from_oxc(func.span),
                )),
            }
        }
    }

    /// Validate already-reserved namespace function rows without republishing them.
    pub(in crate::check::checker) fn validate_reserved_namespace_function_group(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
        ambient: bool,
    ) -> bool {
        let Some(first) = statements.first().and_then(function_decl_from_statement) else {
            return false;
        };
        let Some(name) = first.id.as_ref().map(|id| id.name.as_str()) else {
            return false;
        };
        if statements.iter().any(|statement| {
            function_decl_from_statement(statement)
                .and_then(|function| function.id.as_ref())
                .map(|id| id.name.as_str())
                != Some(name)
        }) {
            return false;
        }
        self.finalize_function_declaration_group_with_publication(
            scope, statements, name, surfaces, false, ambient,
        );
        true
    }

    fn finalize_function_declaration_group_with_publication(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        name: &str,
        surfaces: &mut FxHashMap<u32, FunctionReservation<Ticket>>,
        publish_value: bool,
        ambient: bool,
    ) {
        let function_group = self
            .binder
            .resolve_value(scope, name)
            .filter(|symbol| self.function_groups.contains_symbol(*symbol));
        let mut signatures = Vec::new();
        let mut implementation: Option<(TypeId, ValueStorageId)> = None;
        let mut unavailable = false;
        for stmt in statements {
            let Some(func) = function_decl_from_statement(stmt) else {
                continue;
            };
            self.fill_reserved_function_body(scope, func, surfaces, false);
            let Some(decl_id) = self.function_decl_id(func) else {
                continue;
            };
            if let Some(surface) = surfaces.get(&func.span.start) {
                match surface {
                    FunctionReservation::Ready(surface) => {
                        if func.body.is_some() {
                            implementation = Some((surface.function_ty, decl_id));
                        } else {
                            signatures.push((
                                surface.function_ty,
                                Span::from_oxc(func.span),
                                surface.tickets.map(|tickets| tickets.deferred),
                            ));
                        }
                    }
                    FunctionReservation::Unavailable(_) => unavailable = true,
                }
            }
        }

        // Compatibility diagnostics belong to each ready overload row, even when a
        // different row is unavailable and therefore withholds the whole group.
        if let Some((implementation_ty, _)) = implementation {
            if !signatures.is_empty() {
                self.check_overload_implementation_compatibility(implementation_ty, &signatures);
            }
        }

        if !publish_value {
            if !ambient
                && implementation.is_none()
                && statements
                    .iter()
                    .filter_map(function_decl_from_statement)
                    .any(|func| !func.declare)
            {
                if let Some((_, span, ticket)) = signatures.last() {
                    match ticket {
                        Some(ticket) => self.with_ticket_effects(*ticket, |pass| {
                            pass.emit_diagnostic(Diagnostic::overload_missing_implementation(
                                *span,
                            ));
                        }),
                        None => {
                            self.emit_diagnostic(Diagnostic::overload_missing_implementation(*span))
                        }
                    }
                }
            }
            return;
        }

        if function_group.is_some() {
            if !ambient
                && implementation.is_none()
                && statements
                    .iter()
                    .filter_map(function_decl_from_statement)
                    .any(|func| !func.declare)
            {
                if let Some((_, span, ticket)) = signatures.last() {
                    match ticket {
                        Some(ticket) => self.with_ticket_effects(*ticket, |pass| {
                            pass.emit_diagnostic(Diagnostic::overload_missing_implementation(
                                *span,
                            ));
                        }),
                        None => {
                            self.emit_diagnostic(Diagnostic::overload_missing_implementation(*span))
                        }
                    }
                }
            }
            return;
        }

        if unavailable {
            return;
        }

        let overload_ty = if signatures.is_empty() {
            None
        } else {
            Some(self.interner.intern_object(ObjectType {
                call_signatures: signatures.iter().map(|(ty, _, _)| *ty).collect(),
                ..Default::default()
            }))
        };

        let Some((implementation_ty, implementation_decl)) = implementation else {
            if let Some(overload_ty) = overload_ty {
                if let Some((_, span, ticket)) = signatures.last() {
                    if !ambient
                        && statements
                            .iter()
                            .filter_map(function_decl_from_statement)
                            .any(|func| !func.declare)
                    {
                        match ticket {
                            Some(ticket) => self.with_ticket_effects(*ticket, |pass| {
                                pass.emit_diagnostic(Diagnostic::overload_missing_implementation(
                                    *span,
                                ));
                            }),
                            None => self.emit_diagnostic(
                                Diagnostic::overload_missing_implementation(*span),
                            ),
                        }
                    }
                }
                self.expose_overload_value(scope, name, None, overload_ty);
            }
            return;
        };
        if signatures.is_empty() {
            self.publish_copied_decl_type_replay(implementation_decl, implementation_ty);
            return;
        }

        let Some(overload_ty) = overload_ty else {
            return;
        };
        self.expose_overload_value(scope, name, Some(implementation_decl), overload_ty);
    }

    fn expose_overload_value(
        &mut self,
        scope: ScopeId,
        name: &str,
        implementation_decl: Option<ValueStorageId>,
        overload_ty: TypeId,
    ) {
        if let Some(symbol_id) = self.binder.resolve_value(scope, name) {
            if let Some(symbol) = self.binder.symbols.get(symbol_id) {
                let function_values = symbol.function_values.clone();
                let merged_decl = symbol.value;
                let canonical = function_values
                    .first()
                    .copied()
                    .or(implementation_decl)
                    .or(merged_decl);
                if let Some(canonical) = canonical {
                    self.with_replay_owner(
                        super::replay_index::ReplayOwner::Value(canonical),
                        |pass| {
                            for participant in function_values
                                .iter()
                                .copied()
                                .chain(implementation_decl)
                                .chain(merged_decl)
                            {
                                if participant != canonical {
                                    pass.record_replay_demand(
                                        super::replay_index::ReplayOwner::Value(participant),
                                    );
                                }
                            }
                        },
                    );
                }
                let mut published = Vec::new();
                for decl_id in implementation_decl
                    .into_iter()
                    .chain(function_values)
                    .chain(merged_decl)
                {
                    if published.contains(&decl_id) {
                        continue;
                    }
                    published.push(decl_id);
                    let participant = super::replay_index::ReplayOwner::Value(decl_id);
                    self.with_replay_owner(participant, |pass| {
                        if canonical.is_some_and(|canonical| canonical != decl_id) {
                            pass.record_replay_demand(super::replay_index::ReplayOwner::Value(
                                canonical.expect("canonical overload participant"),
                            ));
                        }
                        pass.publish_symbol_value_type(decl_id, overload_ty, symbol_id);
                    });
                }
            }
        } else if let Some(implementation_decl) = implementation_decl {
            self.publish_copied_decl_type_replay(implementation_decl, overload_ty);
        }
    }

    fn check_overload_implementation_compatibility(
        &mut self,
        implementation_ty: TypeId,
        signatures: &[(TypeId, Span, Option<Ticket>)],
    ) {
        for (signature_ty, span, ticket) in signatures {
            let check = |pass: &mut Self| {
                let outcome = pass.with_semantic_query(|query| {
                    query.overload_implementation_compatible(*signature_ty, implementation_ty)
                });
                match outcome {
                    RelationOutcome::Yes => Some(true),
                    RelationOutcome::No(_) => Some(false),
                    RelationOutcome::Exhausted(exhaustion) => {
                        pass.own_type_demand(DemandOutcome::Exhausted(exhaustion), *span);
                        None
                    }
                }
            };
            let compatible = match ticket {
                Some(ticket) => self.with_ticket_effects(*ticket, check),
                None => check(self),
            };
            let Some(compatible) = compatible else {
                break;
            };
            if !compatible {
                match ticket {
                    Some(ticket) => self.with_ticket_effects(*ticket, |pass| {
                        pass.emit_diagnostic(Diagnostic::overload_incompatible(*span));
                    }),
                    None => self.emit_diagnostic(Diagnostic::overload_incompatible(*span)),
                }
                break;
            }
        }
    }
}

fn module_declaration_is_type_only(declaration: &TSModuleDeclaration<'_>) -> bool {
    match &declaration.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            block.body.iter().all(module_statement_is_type_only)
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
            module_declaration_is_type_only(nested)
        }
        None => true,
    }
}

fn module_statement_is_type_only(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::TSInterfaceDeclaration(_) | Statement::TSTypeAliasDeclaration(_) => true,
        Statement::TSModuleDeclaration(declaration) => module_declaration_is_type_only(declaration),
        Statement::ExportNamedDeclaration(export) => match &export.declaration {
            Some(Declaration::TSInterfaceDeclaration(_))
            | Some(Declaration::TSTypeAliasDeclaration(_)) => true,
            Some(Declaration::TSModuleDeclaration(declaration)) => {
                module_declaration_is_type_only(declaration)
            }
            None => true,
            _ => false,
        },
        _ => false,
    }
}

/// Resolve a variable declarator's symbol from its declaration-owning scope. A
/// lexical shadow may affect initializer lookup, but never the identity of a `var`.
fn variable_declaration_decl_id(
    binder: &crate::binder::Binder,
    scope: ScopeId,
    kind: VariableDeclarationKind,
    pattern: &BindingPattern<'_>,
) -> Option<ValueStorageId> {
    let declaration_scope = if kind.is_var() {
        binder.graph.var_scope(scope).unwrap_or(scope)
    } else {
        scope
    };
    binding_decl_id(binder, declaration_scope, pattern)
}

fn variable_declaration_symbol_id(
    binder: &crate::binder::Binder,
    scope: ScopeId,
    kind: VariableDeclarationKind,
    pattern: &BindingPattern<'_>,
) -> Option<crate::binder::symbol::SymbolId> {
    let BindingPattern::BindingIdentifier(identifier) = pattern else {
        return None;
    };
    let declaration_scope = if kind.is_var() {
        binder.graph.var_scope(scope).unwrap_or(scope)
    } else {
        scope
    };
    binder.resolve_value(declaration_scope, identifier.name.as_str())
}

pub(in crate::check::checker) fn function_overload_group<'stmt>(
    statements: &'stmt [Statement<'_>],
    index: usize,
) -> Option<(&'stmt str, usize)> {
    let first = function_decl_from_statement(statements.get(index)?)?;
    let name = first.id.as_ref()?.name.as_str();
    let mut end = index + 1;
    while let Some(next) = statements.get(end).and_then(function_decl_from_statement) {
        if next.id.as_ref().map(|id| id.name.as_str()) != Some(name) {
            break;
        }
        end += 1;
    }
    if end - index > 1 {
        Some((name, end))
    } else {
        None
    }
}

pub(in crate::check::checker) fn function_decl_from_statement<'stmt, 'ast>(
    stmt: &'stmt Statement<'ast>,
) -> Option<&'stmt Function<'ast>> {
    match stmt {
        Statement::FunctionDeclaration(func) => Some(func),
        Statement::ExportNamedDeclaration(export) => match export.declaration.as_ref()? {
            Declaration::FunctionDeclaration(func) => Some(func),
            _ => None,
        },
        _ => None,
    }
}

/// Recognize only the exact unconditional recursion that TypeScript settles as
/// `never`. Broader recursive control flow remains a terminal inference cycle.
fn pure_self_recursive_name<'ast>(func: &Function<'ast>) -> Option<&'ast str> {
    let name = func.id.as_ref()?.name.as_str();
    let body = func.body.as_ref()?;
    let [Statement::ReturnStatement(ret)] = body.statements.as_slice() else {
        return None;
    };
    let mut returned = ret.argument.as_ref()?;
    while let Expression::ParenthesizedExpression(paren) = returned {
        returned = &paren.expression;
    }
    let Expression::CallExpression(call) = returned else {
        return None;
    };
    let mut callee = &call.callee;
    while let Expression::ParenthesizedExpression(paren) = callee {
        callee = &paren.expression;
    }
    let Expression::Identifier(identifier) = callee else {
        return None;
    };
    (identifier.name.as_str() == name).then_some(name)
}

/// Map a relation failure to `TK2741`/`TK2322`/`TK2345`. M6 keeps a flat headline
/// and renders nested reasons as elaboration; simple heads produce no elaboration.
pub(in crate::check::checker) fn emit_obligation_failure(
    store: &Store,
    ob: &AssignObligation,
    head: &Reason,
) -> Diagnostic {
    // The nested "because…" cascade shown below the headline. Empty for a head the
    // headline already expresses in full (e.g. a scalar `Leaf`).
    let elaboration = render_reason_chain(store, head);
    let diagnostic_span = obligation_source_span(ob, head);

    match ob.kind {
        ObligationKind::Assignment => match head {
            Reason::MissingProperty { name, tgt, .. } => {
                let tgt = render_type(store, *tgt, /* widen */ false);
                Diagnostic::property_missing(diagnostic_span, name, &tgt)
            }
            Reason::Leaf { .. }
            | Reason::Property { .. }
            | Reason::ParameterCount { .. }
            | Reason::Parameter { .. }
            | Reason::ReturnType { .. }
            | Reason::UnionSourceMember { .. }
            | Reason::NoUnionMember { .. }
            // M17: an array-element mismatch (`S[]` not assignable to `T[]`) is a
            // `TK2322`; the headline states the two array types, the element's cause
            // nests below it.
            | Reason::ArrayElement { .. }
            // M18: a tuple length mismatch or a positional element mismatch is a
            // `TK2322`; the headline states the two tuple types, with any element
            // cause nested below it.
            | Reason::TupleLength { .. }
            | Reason::TupleElement { .. }
            // M19: a value not fitting the target's index signature is a `TK2322`;
            // the headline states the two object types, the value's cause nested below.
            | Reason::IndexSignature { .. } => {
                // Widen source literals for non-literal targets; keep unit/literal
                // source forms and union offending members in the headline.
                let widen = !is_literal_target(store, ob.tgt);
                let src = render_type(store, headline_src(ob, head), widen);
                let tgt = render_type(store, ob.tgt, /* widen */ false);
                let message = format!("Type '{src}' is not assignable to type '{tgt}'");
                Diagnostic::not_assignable(diagnostic_span, message).with_elaboration(elaboration)
            }
        },
        ObligationKind::Argument => {
            let widen = !is_literal_target(store, ob.tgt);
            let src = render_type(store, headline_src(ob, head), widen);
            let tgt = render_type(store, ob.tgt, /* widen */ false);
            Diagnostic::argument_not_assignable(diagnostic_span, &src, &tgt)
                .with_elaboration(elaboration)
        }
        ObligationKind::FreshArgument => match head {
            Reason::MissingProperty { .. } | Reason::TupleLength { .. } => {
                let widen = !is_literal_target(store, ob.tgt);
                let src = render_type(store, headline_src(ob, head), widen);
                let tgt = render_type(store, ob.tgt, /* widen */ false);
                Diagnostic::argument_not_assignable(diagnostic_span, &src, &tgt)
                    .with_elaboration(elaboration)
            }
            Reason::Leaf { .. }
            | Reason::Property { .. }
            | Reason::ParameterCount { .. }
            | Reason::Parameter { .. }
            | Reason::ReturnType { .. }
            | Reason::UnionSourceMember { .. }
            | Reason::NoUnionMember { .. }
            | Reason::ArrayElement { .. }
            | Reason::TupleElement { .. }
            | Reason::IndexSignature { .. } => {
                let widen = !is_literal_target(store, ob.tgt);
                let src = render_type(store, headline_src(ob, head), widen);
                let tgt = render_type(store, ob.tgt, /* widen */ false);
                let message = format!("Type '{src}' is not assignable to type '{tgt}'");
                Diagnostic::not_assignable(diagnostic_span, message).with_elaboration(elaboration)
            }
        },
    }
}

fn obligation_source_span(ob: &AssignObligation, reason: &Reason) -> Span {
    let member = match reason {
        Reason::Property { name, .. } => Some(AssignSourceMember::Property(name.clone())),
        Reason::TupleElement { index, .. } => Some(AssignSourceMember::Element(*index)),
        Reason::UnionSourceMember { because, .. }
        | Reason::ArrayElement { because, .. }
        | Reason::IndexSignature { because, .. } => {
            return obligation_source_span(ob, because);
        }
        _ => None,
    };
    member
        .and_then(|member| {
            ob.source_member_spans
                .iter()
                .find_map(|(candidate, span)| (candidate == &member).then_some(*span))
        })
        .unwrap_or(ob.src_span)
}

pub(in crate::check::checker) fn emit_exhausted_obligation(
    store: &Store,
    ob: &AssignObligation,
) -> Diagnostic {
    let src = render_type(store, ob.src, false);
    let tgt = render_type(store, ob.tgt, false);
    match ob.kind {
        ObligationKind::Assignment | ObligationKind::FreshArgument => Diagnostic::not_assignable(
            ob.src_span,
            format!("Type '{src}' is not assignable to type '{tgt}'"),
        ),
        ObligationKind::Argument => Diagnostic::argument_not_assignable(ob.src_span, &src, &tgt),
    }
}

/// The source type for the headline; union-source failures use the specific
/// offending member and leave the whole union for the nested reason chain.
fn headline_src(ob: &AssignObligation, head: &Reason) -> TypeId {
    match head {
        Reason::UnionSourceMember { member, .. } => *member,
        Reason::Leaf { src, .. } => *src,
        _ => ob.src,
    }
}

/// Whether the message's **target** is a literal / unit type (M25). When it is, the
/// source literal is shown as-is rather than widened to its base intrinsic — tsc keeps
/// `'false'` / `'2'` against a `true` / `1` target, and only widens (`"hello"` →
/// `string`) against a non-literal target.
fn is_literal_target(store: &Store, tgt: TypeId) -> bool {
    store.literal_value(tgt).is_some()
}

#[cfg(test)]
mod namespace_alias_tests {
    use super::NamespaceAliasLookupScope;
    use crate::types::Interner;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn check_alias_with_unrelated_declarations(count: usize) -> u64 {
        let unrelated = (0..count)
            .map(|index| format!("const unrelated{index} = {index};"))
            .collect::<String>();
        let source = format!(
            r#"
                namespace AliasRoot {{ export const value: number = 1; }}
                {unrelated}
                const FirstAlias = AliasRoot;
                const SecondAlias = FirstAlias;
                const observed: number = SecondAlias.value;
            "#
        );
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
        assert!(!parsed.panicked);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut interner = Interner::with_intrinsics();
        let scope = NamespaceAliasLookupScope::start();
        let output = super::super::check_program(&mut interner, &parsed.program);
        let lookups = scope.finish();
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        lookups
    }

    #[test]
    fn namespace_alias_declaration_lookup_is_indexed_and_table_size_independent() {
        assert_eq!(check_alias_with_unrelated_declarations(2), 2);
        assert_eq!(check_alias_with_unrelated_declarations(1_024), 2);
    }
}

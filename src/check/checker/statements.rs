//! Statement checking.

use super::assignment::binding_decl_id;
use super::assignment::check_excess_properties;
use super::assignment::declared_from_init;
use super::calls::widen;
use super::context::*;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic};
use crate::relate::{Reason, ReasonChain, Relater, Relation};
use crate::span::Span;
use crate::types::repr::{ObjectType, ParameterType};
use crate::types::store::{Store, TypeId};
use crate::types::WellKnown;
use oxc_ast::ast::{
    BindingPattern, BlockStatement, Declaration, Expression, ForInStatement, ForOfStatement,
    ForStatement, ForStatementInit, ForStatementLeft, Function, Statement, VariableDeclarationKind,
    VariableDeclarator,
};

impl<'a, 'ast> Pass<'a, 'ast> {
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
        let mut index = 0;
        while index < statements.len() {
            if let Some((name, end)) = function_overload_group(statements, index) {
                self.check_function_declaration_group(scope, &statements[index..end], name);
                index = end;
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
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    self.check_declarator(scope, decl.kind, declarator);
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
            // Other statements are out of the subset.
            _ => {}
        }
    }

    fn check_declaration(&mut self, scope: ScopeId, decl: &Declaration<'_>) {
        match decl {
            Declaration::VariableDeclaration(var) => {
                for declarator in &var.declarations {
                    self.check_declarator(scope, var.kind, declarator);
                }
            }
            Declaration::FunctionDeclaration(func) => {
                self.check_function_declaration(scope, func);
            }
            Declaration::ClassDeclaration(class) => {
                self.check_class(scope, class);
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
                check_excess_properties(self.interner.store(), arg, tgt, &mut self.diagnostics);
                self.obligations.push(AssignObligation {
                    src,
                    tgt,
                    src_span,
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
        let block_scope = self
            .binder
            .block_scopes
            .get(&(self.current_module, block.span.start))
            .copied()
            .unwrap_or(scope);
        self.check_statement_list(block_scope, &block.body, declared_ret, inferred);
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
                        self.check_declarator(head, decl.kind, declarator);
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
            return;
        };
        for declarator in &decl.declarations {
            if let Some(decl_id) = binding_decl_id(self.binder, scope, &declarator.id) {
                self.decl_types.set(decl_id, element_ty);
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
    ) -> Option<(TypeId, Span)> {
        let initializer = self.infer_initializer(scope, init, annotation);

        // Both sides present: the initializer must be assignable to the annotation (primary
        // span = the initializer), and a fresh object literal gets an excess-property check.
        if let (Some(ann), Some((init_ty, init_span))) = (annotation, initializer) {
            self.obligations.push(AssignObligation {
                src: init_ty,
                tgt: ann,
                src_span: init_span,
                kind: ObligationKind::Assignment,
            });
            check_excess_properties(self.interner.store(), init, ann, &mut self.diagnostics);
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
        let decl_id = binding_decl_id(self.binder, scope, &declarator.id);

        // Lower the annotation first (independent of the initializer; emits no
        // initializer-dependent diagnostics) so it can provide a **tuple context** for an
        // array-literal initializer (M18 contextual typing).
        let annotation = match declarator.type_annotation.as_ref() {
            Some(ann) => self.lower_annotation(scope, &ann.type_annotation),
            None => None,
        };

        // Infer/check the initializer against the annotation, including M18 tuple
        // contextual typing for array literals.
        let initializer = declarator
            .init
            .as_ref()
            .and_then(|init| self.check_annotated_initializer(scope, annotation, init));

        // F4: object destructuring bindings run M13 access checks against the
        // initializer type; binding the destructured names' types is deferred.
        if let BindingPattern::ObjectPattern(object) = &declarator.id {
            if let Some((source, _)) = &initializer {
                self.check_object_pattern_access(object, *source);
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
            self.decl_types.set(decl_id, ty);
        }
    }

    /// Check a function declaration and bind its function type. Generic functions
    /// also record the template plus type-parameter ids for call-site instantiation.
    fn check_function_declaration(&mut self, scope: ScopeId, func: &Function<'_>) {
        let (fn_ty, params) = self.infer_function(scope, func);
        if let Some(id) = &func.id {
            if let Some(decl_id) = self
                .binder
                .graph
                .resolve(scope, id.name.as_str())
                .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                .and_then(|s| s.value)
            {
                self.decl_types.set(decl_id, fn_ty);
                if !params.is_empty() {
                    self.generic_sig_params.insert(fn_ty, params);
                }
                if func.body.is_none() && !func.declare {
                    self.diagnostics.push(Diagnostic::overload_missing_implementation(
                        Span::from_oxc(func.span),
                    ));
                }
            }
        }
    }

    fn check_function_declaration_group(
        &mut self,
        scope: ScopeId,
        statements: &[Statement<'_>],
        name: &str,
    ) {
        let mut signatures: Vec<(TypeId, Span)> = Vec::new();
        let mut implementation: Option<(TypeId, DeclId)> = None;
        for stmt in statements {
            let Some(func) = function_decl_from_statement(stmt) else {
                continue;
            };
            let decl_id = self
                .binder
                .fn_decl_ids
                .get(&(self.current_module, func.span.start))
                .copied();
            let (fn_ty, params) = self.infer_function(scope, func);
            if let Some(decl_id) = decl_id {
                self.decl_types.set(decl_id, fn_ty);
                if !params.is_empty() {
                    self.generic_sig_params.insert(fn_ty, params);
                }
                if func.body.is_some() {
                    implementation = Some((fn_ty, decl_id));
                } else {
                    signatures.push((fn_ty, Span::from_oxc(func.span)));
                }
            }
        }

        let overload_ty = if signatures.is_empty() {
            None
        } else {
            Some(self.interner.intern_object(ObjectType {
                call_signatures: signatures.iter().map(|(ty, _)| *ty).collect(),
                ..Default::default()
            }))
        };

        let Some((implementation_ty, implementation_decl)) = implementation else {
            if let Some(overload_ty) = overload_ty {
                if let Some((_, span)) = signatures.first() {
                    if statements
                        .iter()
                        .filter_map(function_decl_from_statement)
                        .any(|func| !func.declare)
                    {
                        self.diagnostics
                            .push(Diagnostic::overload_missing_implementation(*span));
                    }
                }
                self.expose_overload_value(scope, name, None, overload_ty);
            }
            return;
        };
        if signatures.is_empty() {
            self.decl_types.set(implementation_decl, implementation_ty);
            return;
        }

        self.check_overload_implementation_compatibility(
            implementation_ty,
            &signatures,
        );
        let Some(overload_ty) = overload_ty else {
            return;
        };
        self.expose_overload_value(scope, name, Some(implementation_decl), overload_ty);
    }

    fn expose_overload_value(
        &mut self,
        scope: ScopeId,
        name: &str,
        implementation_decl: Option<DeclId>,
        overload_ty: TypeId,
    ) {
        if let Some(implementation_decl) = implementation_decl {
            self.decl_types.set(implementation_decl, overload_ty);
        }
        if let Some(symbol_id) = self.binder.graph.resolve(scope, name) {
            if let Some(symbol) = self.binder.symbols.get(symbol_id) {
                for decl_id in &symbol.function_values {
                    if Some(*decl_id) == implementation_decl {
                        continue;
                    }
                    self.decl_types.set(*decl_id, overload_ty);
                }
            }
        }
    }

    fn check_overload_implementation_compatibility(
        &mut self,
        implementation_ty: TypeId,
        signatures: &[(TypeId, Span)],
    ) {
        for (signature_ty, span) in signatures {
            let signature_ty =
                self.align_overload_type_params(*signature_ty, implementation_ty);
            let wk = self.interner.well_known();
            let store = self.interner.store();
            let mut relater = Relater::new(store, wk);
            if !overload_implementation_compatible(
                store,
                &mut relater,
                signature_ty,
                implementation_ty,
            ) {
                self.diagnostics
                    .push(Diagnostic::overload_incompatible(*span));
                break;
            }
        }
    }

    fn align_overload_type_params(
        &mut self,
        overload_ty: TypeId,
        implementation_ty: TypeId,
    ) -> TypeId {
        let Some(overload_params) = self.generic_sig_params.get(&overload_ty).cloned() else {
            return overload_ty;
        };
        let Some(implementation_params) =
            self.generic_sig_params.get(&implementation_ty).cloned()
        else {
            return overload_ty;
        };
        if overload_params.len() != implementation_params.len() {
            return overload_ty;
        }
        let mut map = rustc_hash::FxHashMap::default();
        for (overload_param, implementation_param) in
            overload_params.into_iter().zip(implementation_params)
        {
            let target = self
                .interner
                .intern_type_param(implementation_param, "T");
            map.insert(overload_param, target);
        }
        crate::types::substitute(self.interner, overload_ty, &map)
    }
}

pub(in crate::check::checker) fn overload_implementation_compatible(
    store: &Store,
    relater: &mut Relater<'_>,
    overload_ty: TypeId,
    implementation_ty: TypeId,
) -> bool {
    let Some(overload) = store.function_type(overload_ty) else {
        return false;
    };
    let Some(implementation) = store.function_type(implementation_ty) else {
        return false;
    };
    if overload.params.len() > implementation.params.len() {
        return false;
    }
    for (overload_param, implementation_param) in overload.params.iter().zip(&implementation.params)
    {
        if !parameter_compatible(relater, overload_param, implementation_param) {
            return false;
        }
    }
    matches!(relater.is_assignable(overload.ret, implementation.ret), Relation::Yes)
}

fn parameter_compatible(
    relater: &mut Relater<'_>,
    overload: &ParameterType,
    implementation: &ParameterType,
) -> bool {
    if overload.rest != implementation.rest {
        return false;
    }
    matches!(
        relater.is_assignable(overload.ty, implementation.ty),
        Relation::Yes
    )
}

fn function_overload_group<'stmt>(
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

fn function_decl_from_statement<'stmt, 'ast>(
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

/// Map a relation failure to `TK2741`/`TK2322`/`TK2345`. M6 keeps a flat headline
/// and renders nested reasons as elaboration; simple heads produce no elaboration.
pub(in crate::check::checker) fn emit_obligation_failure(
    store: &Store,
    ob: &AssignObligation,
    head: &Reason,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // The nested "because…" cascade shown below the headline. Empty for a head the
    // headline already expresses in full (e.g. a scalar `Leaf`).
    let elaboration = render_reason_chain(store, head);

    match ob.kind {
        ObligationKind::Assignment => match head {
            Reason::MissingProperty { name, tgt, .. } => {
                let tgt = render_type(store, *tgt, /* widen */ false);
                diagnostics.push(Diagnostic::property_missing(ob.src_span, name, &tgt));
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
                diagnostics
                    .push(Diagnostic::not_assignable(ob.src_span, message).with_elaboration(elaboration));
            }
        },
        ObligationKind::Argument => {
            let widen = !is_literal_target(store, ob.tgt);
            let src = render_type(store, headline_src(ob, head), widen);
            let tgt = render_type(store, ob.tgt, /* widen */ false);
            diagnostics.push(
                Diagnostic::argument_not_assignable(ob.src_span, &src, &tgt)
                    .with_elaboration(elaboration),
            );
        }
        ObligationKind::FreshArgument => match head {
            Reason::MissingProperty { .. } | Reason::TupleLength { .. } => {
                let widen = !is_literal_target(store, ob.tgt);
                let src = render_type(store, headline_src(ob, head), widen);
                let tgt = render_type(store, ob.tgt, /* widen */ false);
                diagnostics.push(
                    Diagnostic::argument_not_assignable(ob.src_span, &src, &tgt)
                        .with_elaboration(elaboration),
                );
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
                diagnostics.push(
                    Diagnostic::not_assignable(ob.src_span, message).with_elaboration(elaboration),
                );
            }
        },
    }
}

/// Emit `TK2416` override failures. The base member kind decides variance: base
/// methods use tsc's bivariant-parameter/covariant-return rule, while fields,
/// accessors, and data properties use one strict `own → base` relation query.
/// Unequal raw-arity base methods stay deferred until this bespoke bivariant path is
/// reviewed against represented optional/rest signature shape.
pub(in crate::check::checker) fn emit_override_failures(
    store: &Store,
    well_known: WellKnown,
    relater: &mut Relater,
    checks: &[OverrideCheck],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for check in checks {
        if let Some(chain) = override_failure_reason(
            store,
            well_known,
            relater,
            check.own_ty,
            check.base_ty,
            check.base_is_method,
        ) {
            let elaboration = render_reason_chain(store, chain.head());
            diagnostics.push(
                Diagnostic::property_override_incompatible(
                    check.span,
                    &check.name,
                    &check.derived,
                    &check.base,
                )
                .with_elaboration(elaboration),
            );
        }
    }
}

/// The reason chain for an incompatible override, or `None` when compatible / out of
/// subset. See [`emit_override_failures`] for the base-method (bivariant-param /
/// covariant-return) vs. base-field (strict) rule.
fn override_failure_reason(
    store: &Store,
    well_known: WellKnown,
    relater: &mut Relater,
    own_ty: TypeId,
    base_ty: TypeId,
    base_is_method: bool,
) -> Option<ReasonChain> {
    // Base method syntax plus function types triggers tsc's method rule; base
    // function-typed fields fall through to the strict query below.
    if let (true, Some(own_fn), Some(base_fn)) = (
        base_is_method,
        store.function_type(own_ty),
        store.function_type(base_ty),
    ) {
        // Differing raw arity is still out of subset for the method-bivariance path.
        if own_fn.params.len() != base_fn.params.len() {
            return None;
        }
        // Parameters: bivariant — compatible if assignable in EITHER direction.
        for (own_param, base_param) in own_fn.params.iter().zip(base_fn.params.iter()) {
            if relater.is_assignable(own_param.ty, base_param.ty).is_yes()
                || relater.is_assignable(base_param.ty, own_param.ty).is_yes()
            {
                continue;
            }
            // Neither direction holds: report the derived→base (own→base) failure.
            if let Relation::No(chain) = relater.is_assignable(own_param.ty, base_param.ty) {
                return Some(chain);
            }
        }
        // Return type: covariant (own → base), with the void-return exception (a void
        // target return accepts any source return) — mirroring `relate_functions` so a
        // value-returning method/field over a `void` method stays clean.
        if base_fn.ret != well_known.void {
            if let Relation::No(chain) = relater.is_assignable(own_fn.ret, base_fn.ret) {
                return Some(chain);
            }
        }
        return None;
    }
    // Base is a data property / function-typed field / accessor (or a non-function
    // shape): a single strict own → base assignability query, exactly like `TK2322`.
    match relater.is_assignable(own_ty, base_ty) {
        Relation::No(chain) => Some(chain),
        Relation::Yes => None,
    }
}

/// The source type for the headline; union-source failures use the specific
/// offending member and leave the whole union for the nested reason chain.
fn headline_src(ob: &AssignObligation, head: &Reason) -> TypeId {
    match head {
        Reason::UnionSourceMember { member, .. } => *member,
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

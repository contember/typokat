//! Statement checking.

use super::assignment::binding_decl_id;
use super::assignment::check_excess_properties;
use super::assignment::declared_from_init;
use super::calls::widen;
use super::context::*;
use crate::binder::scope::ScopeId;
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic};
use crate::relate::{Reason, ReasonChain, Relater, Relation};
use crate::span::Span;
use crate::types::store::{Store, TypeId};
use crate::types::WellKnown;
use oxc_ast::ast::{
    BindingPattern, BlockStatement, Declaration, Expression, Function, Statement,
    VariableDeclarationKind, VariableDeclarator,
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
        for stmt in statements {
            self.check_stmt(scope, stmt, None, &mut no_return);
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

    /// Check `return expr` against the declared return type, or record the first
    /// value return's widened type for inference. Bare `return;` contributes none.
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
            // No annotation: infer from the first value return, widened.
            None => {
                let Some((src, _)) = self.infer_expr(scope, arg) else {
                    return;
                };
                if inferred.is_none() {
                    *inferred = Some(widen(self.interner, src));
                }
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
        for stmt in &block.body {
            self.check_stmt(block_scope, stmt, declared_ret, inferred);
        }
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
                    self.generic_fns
                        .insert(decl_id, GenericSig { params, fn_ty });
                }
            }
        }
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

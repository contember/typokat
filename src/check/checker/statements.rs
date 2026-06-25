//! statements module (extracted from checker/mod.rs).

use crate::binder::scope::ScopeId;
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic};
use crate::relate::Reason;
use crate::types::store::{Store, TypeId};
use oxc_ast::ast::{
    BindingPattern, BlockStatement, Expression,
    Function, Statement, VariableDeclarationKind, VariableDeclarator,
};
use super::context::*;
use super::assignment::binding_decl_id;
use super::assignment::check_excess_properties;
use super::assignment::declared_from_init;
use super::calls::widen;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Check a list of statements in `scope` at the **module top level** (no enclosing
    /// function, so no return context). Each statement flows through the unified
    /// statement walker with an empty return context.
    pub(in crate::check::checker) fn check_statements(&mut self, scope: ScopeId, statements: &[Statement<'_>]) {
        let mut no_return: Option<TypeId> = None;
        for stmt in statements {
            self.check_stmt(scope, stmt, None, &mut no_return);
        }
    }

    /// Check one statement in `scope` — the **unified, flow-sensitive** statement
    /// walker (M7). It handles every statement kind in the subset, threading an
    /// optional return context (`declared_ret` + the accumulating inferred return
    /// `inferred`) so the same code serves both the module top level (empty context)
    /// and a function body. It is the structured-flow driver of the §5 interpreter:
    /// `if`/`else` is the only construct that touches the narrowing environment, via
    /// [`check_if`]'s fork-and-restore.
    ///
    /// `inferred` accumulates the first value-return's widened type when no return is
    /// declared (used only by [`check_function_body`]); at module level it is a
    /// throwaway that no `return` ever writes (a top-level `return` is illegal TS).
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
            // M7: control-flow narrowing happens here (the fork-and-restore).
            Statement::IfStatement(if_stmt) => {
                self.check_if(scope, if_stmt, declared_ret, inferred);
            }
            // A `{ … }` block runs its statements in its own (binder-created) block
            // scope, inheriting the current narrowing environment.
            Statement::BlockStatement(block) => {
                self.check_block(scope, block, declared_ret, inferred);
            }
            // M8: `switch` narrows the discriminant per `case` (fork-and-restore).
            Statement::SwitchStatement(switch) => {
                self.check_switch(scope, switch, declared_ret, inferred);
            }
            // Other statements are out of the subset.
            _ => {}
        }
    }

    /// Check a `return <expr>?` statement against the enclosing function's return
    /// context (extracted from the M3 body walk; behaviour unchanged). With a declared
    /// return type, the returned expression is an assignability obligation
    /// (primary span = the expression); without one, the first value-return's widened
    /// type is recorded as the inferred return. A bare `return;` is handled by the
    /// `void` rule in phase 2 and contributes no inferred type.
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
        let Some((src, src_span)) = self.infer_expr(scope, arg) else {
            return;
        };
        match declared_ret {
            // Declared return type: check the returned expression against it.
            Some(tgt) => {
                self.obligations.push(AssignObligation {
                    src,
                    tgt,
                    src_span,
                    kind: ObligationKind::Assignment,
                });
            }
            // No annotation: infer from the first value return, widened.
            None => {
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
            .get(&block.span.start)
            .copied()
            .unwrap_or(scope);
        for stmt in &block.body {
            self.check_stmt(block_scope, stmt, declared_ret, inferred);
        }
    }

    /// Check one variable declarator and record its declared/inferred type.
    ///
    /// The declared type is the annotation if present, otherwise the (possibly
    /// widened) initializer type. When both are present, an assignability obligation
    /// is collected and a fresh object literal gets an excess-property check.
    ///
    /// M18 — **contextual typing** for an array literal in a **tuple** position: when
    /// the (resolved) annotation is a tuple and the initializer is an array literal, the
    /// literal is typed **positionally as a tuple** (element *i* = the type of literal
    /// element *i*) rather than as the M17 array. Relating that tuple to the annotation
    /// tuple then checks position-by-position (`[1, "x"]` <: `[number, string]` ok;
    /// `["x", 1]` → one `TK2322`) and catches length mismatches (`[1]`, `[1, "x", 2]` →
    /// one `TK2322`). With **no** tuple context an array literal still infers an array
    /// (M17 unchanged). See [`infer_array_literal_as_tuple`].
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

        // Infer the initializer (it may resolve references / emit TK2304 and descends
        // into any nested function body). M18: in a **tuple** context an array literal is
        // typed positionally as a tuple; otherwise it infers an array (M17) — and every
        // other expression is inferred exactly as before.
        let initializer = declarator
            .init
            .as_ref()
            .and_then(|init| self.infer_initializer(scope, init, annotation));

        // F4 — access control through an **object** destructuring binding
        // (`let { priv } = k;`). Run M13's private/protected check for each destructured
        // member against the source type — the initializer's inferred type (binding the
        // names' types is deferred; only the access check runs). Other pattern kinds
        // (identifier, array, nested, rest, defaults) are out of scope and skipped.
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

        // When both sides are present, the initializer must be assignable to the
        // annotation (primary span = the initializer).
        if let (Some(ann), Some((init_ty, init_span))) = (annotation, initializer) {
            self.obligations.push(AssignObligation {
                src: init_ty,
                tgt: ann,
                src_span: init_span,
                kind: ObligationKind::Assignment,
            });

            // Excess-property check (freshness) for a fresh object literal target.
            if let Some(init_expr) = declarator.init.as_ref() {
                check_excess_properties(self.interner.store(), init_expr, ann, &mut self.diagnostics);
            }
        }
    }

    /// Check a function declaration: compute its function type, bind it into the
    /// value slot (so a call resolves), and descend into its body.
    ///
    /// M9: a **generic** function (`function f<T>(…)`) additionally records its generic
    /// signature (the type-parameter ids + the template function type) under its value
    /// `DeclId`, so a call `f<number>(…)` can instantiate it. Its bound `decl_types`
    /// id is the template (signature with the parameter types embedded); a *non-generic*
    /// call site would see those parameter types unresolved, but the fixtures only call
    /// generic functions with explicit type arguments (inference is M10).
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
                    self.generic_fns.insert(decl_id, GenericSig { params, fn_ty });
                }
            }
        }
    }

}

/// Map a relation failure to a diagnostic according to the obligation's kind.
///
/// `Assignment`: a required-target-property absence → `TK2741`; everything else
/// (primitive mismatch, a present-but-wrong property, or any function-shaped
/// mismatch — possibly nested) → `TK2322`. The error type never reaches here (it
/// is `any`-like, so its obligations resolve to `Yes`). `Argument`: any failure →
/// `TK2345`.
///
/// M6 (§6.4): the **headline** keeps its flat top-level form, and the nested
/// reason chain is rendered below it as the diagnostic's elaboration via
/// [`render_reason_chain`]. A single-`Leaf`/missing-property/arity head produces
/// an **empty** elaboration (the headline already states it in full), so scalar
/// mismatches render exactly one line — no earlier-milestone regression.
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
                // Source widened (literal → base), target as-is (mvp-plan
                // M0/M1 message spec). For a union source the headline names the
                // specific failing member, not the whole union (matching tsc:
                // `number | string` → `number` reports `'string'`).
                let src = render_type(store, headline_src(ob, head), /* widen */ true);
                let tgt = render_type(store, ob.tgt, /* widen */ false);
                let message = format!("Type '{src}' is not assignable to type '{tgt}'");
                diagnostics
                    .push(Diagnostic::not_assignable(ob.src_span, message).with_elaboration(elaboration));
            }
        },
        ObligationKind::Argument => {
            let src = render_type(store, headline_src(ob, head), /* widen */ true);
            let tgt = render_type(store, ob.tgt, /* widen */ false);
            diagnostics.push(
                Diagnostic::argument_not_assignable(ob.src_span, &src, &tgt)
                    .with_elaboration(elaboration),
            );
        }
    }
}

/// The source type to put in the headline message. Normally the obligation's
/// source, but for a **union source** failure it is the specific offending member
/// (`number | string` not assignable to `number` reports the failing `string`,
/// matching tsc) — the whole-union form is reserved for the nested reason chain
/// (M6).
fn headline_src(ob: &AssignObligation, head: &Reason) -> TypeId {
    match head {
        Reason::UnionSourceMember { member, .. } => *member,
        _ => ob.src,
    }
}


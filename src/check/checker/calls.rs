//! calls module (extracted from checker/mod.rs).

use super::assignment::check_excess_properties;
use super::context::*;
use super::decls::alloc_type_param_ids;
use super::decls::value_decl_id;
use super::eval::{contains_deferred_argument, contains_deferred_keyof};
use super::expr::contextual_literal_target;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::check::infer;
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic};
use crate::relate::{Relater, Relation};
use crate::span::Span;
use crate::types::repr::{FunctionType, IntrinsicKind, ParameterType, TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::{substitute, Interner, WellKnown};
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, CallExpression, Expression, FormalParameters,
    Function, FunctionBody, NewExpression,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// M24: check explicit type arguments against substituted constraints. The
    /// shared relation engine supplies `TK2344` reason chains; failed arguments
    /// still instantiate, matching tsc and avoiding cascades.
    pub(in crate::check::checker) fn check_type_argument_constraints(
        &mut self,
        type_params: &[TypeParamId],
        args: &[(TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) {
        // Build the (argument, substituted-constraint, span) checks up front — this needs
        // `&mut Interner` (substitution may intern new types), which cannot overlap the
        // relation engine's immutable store borrow below.
        let mut checks: Vec<(TypeId, TypeId, TypeId, Span)> = Vec::new();
        for (&param, &(arg, span)) in type_params.iter().zip(args) {
            let Some(constraint) = self.interner.store().type_param_constraint(param) else {
                continue;
            };
            let substituted = substitute(self.interner, constraint, map);
            // M28: a substituted constraint may be a pending computation (`K extends
            // keyof T` at `Pick<P, "q">` → `keyof P`) — resolve it through the shared
            // evaluator before relating, so the check runs against the VALUE
            // (`"a" | "b"`), driving the fixture's TK2344.
            let evaluated = self.evaluate_type(substituted, span);
            // M28: a substituted constraint still carrying deferred `keyof` cannot
            // be decided here; tsc lands that check at concrete instantiation.
            // Keyof only, so conditional/mapped constraints keep prior behavior.
            if contains_deferred_keyof(self.interner.store(), evaluated) {
                continue;
            }
            // M28: always evaluate the argument before checking. Decidable
            // compositions check precisely; still-deferred results check
            // conservatively (documented over-report for backlog 37 shapes).
            let evaluated_arg = self.evaluate_type(arg, span);
            checks.push((evaluated_arg, arg, evaluated, span));
        }
        if checks.is_empty() {
            return;
        }

        // Relate each argument to its constraint and render the failures under a single
        // immutable store borrow; push the diagnostics after it ends.
        let wk = self.interner.well_known();
        let mut failures: Vec<(String, String, Span, Vec<String>)> = Vec::new();
        {
            let store = self.interner.store();
            let mut relater = Relater::new(store, wk);
            for (evaluated_arg, written_arg, constraint, span) in checks {
                if let Relation::No(chain) = relater.is_assignable(evaluated_arg, constraint) {
                    // Render the written argument when evaluation remains deferred;
                    // otherwise render the evaluated value, matching tsc-like output.
                    let render_id = if contains_deferred_argument(store, evaluated_arg) {
                        written_arg
                    } else {
                        evaluated_arg
                    };
                    let src = render_type(store, render_id, /* widen */ false);
                    let tgt = render_type(store, constraint, /* widen */ false);
                    let elaboration = render_reason_chain(store, chain.head());
                    failures.push((src, tgt, span, elaboration));
                }
            }
        }
        for (src, tgt, span, elaboration) in failures {
            self.diagnostics.push(
                Diagnostic::constraint_not_satisfied(span, &src, &tgt).with_elaboration(elaboration),
            );
        }
    }

    /// Instantiate an M9 generic call with explicit type arguments. The callee must
    /// be a registered generic function identifier; wrong type-argument arity zips
    /// gracefully because `TK2558` is out of scope.
    fn instantiate_generic_callee(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
    ) -> Option<TypeId> {
        let args = call.type_arguments.as_deref()?;

        // The callee must be a plain identifier naming a generic function declaration.
        let Expression::Identifier(ident) = &call.callee else {
            return None;
        };
        let decl_id = self
            .binder
            .graph
            .resolve(scope, ident.name.as_str())
            .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
            .and_then(|s| s.value)?;
        let sig = self.generic_fns.get(&decl_id)?.clone();

        // Lower the type arguments (in the call's scope), keeping each one's span for a
        // constraint diagnostic. A non-lowerable argument aborts → fall back to the
        // inferred callee path.
        let mut arg_infos: Vec<(TypeId, Span)> = Vec::with_capacity(args.params.len());
        for arg in &args.params {
            arg_infos.push((self.lower_annotation(scope, arg)?, Span::from_oxc(arg.span())));
        }

        // Substitute the generic's parameters with the arguments (graceful on an arity
        // mismatch: zip to the shorter list) and return the instantiated signature.
        let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
        for (&param, &(arg, _)) in sig.params.iter().zip(&arg_infos) {
            map.insert(param, arg);
        }
        // M24: each explicit type argument must satisfy its parameter's constraint.
        self.check_type_argument_constraints(&sig.params, &arg_infos, &map);
        Some(substitute(self.interner, sig.fn_ty, &map))
    }

    /// Instantiate an M10 generic call without explicit type arguments by inferring
    /// from already-inferred arguments. Inference only chooses type arguments; the
    /// instantiated signature is still checked by the relation engine.
    fn infer_generic_callee(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
        arg_types: &[(TypeId, Span)],
        arg_fresh: &[bool],
    ) -> Option<TypeId> {
        // The callee must be a plain identifier naming a generic function declaration.
        let Expression::Identifier(ident) = &call.callee else {
            return None;
        };
        let decl_id = self
            .binder
            .graph
            .resolve(scope, ident.name.as_str())
            .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
            .and_then(|s| s.value)?;
        let sig = self.generic_fns.get(&decl_id)?.clone();

        // The template signature's parameter types — the inference targets (they carry
        // the type parameters to be solved). Snapshot before the mutable inference call.
        let param_types: Vec<TypeId> = match self.interner.store().function_type(sig.fn_ty) {
            Some(func) => func.params.iter().map(|p| p.ty).collect(),
            None => return None,
        };
        let args: Vec<TypeId> = arg_types.iter().map(|(ty, _)| *ty).collect();

        // Run the generative inference engine to fix the type arguments, then
        // instantiate the template by the same M9 substitution. `arg_fresh` feeds the
        // M24 clamp exemption for fresh object/array literal arguments.
        let map = infer::infer_type_arguments(
            self.interner,
            &mut self.next_type_param,
            &sig.params,
            &param_types,
            &args,
            arg_fresh,
        );
        Some(substitute(self.interner, sig.fn_ty, &map))
    }

    /// Infer and check a call. Callable callees are function types or objects with
    /// one call signature; non-callables still yield the error type silently until
    /// the dedicated diagnostic can account for dropped callability and overloads.
    pub(in crate::check::checker) fn infer_call(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();
        let call_span = Span::from_oxc(call.span);

        // M12: a `super(args)` call (callee is `Super`) — check against the **base
        // constructor** signature in scope, not as an ordinary call. Handled before the
        // generic/callee machinery (`Super` is not an identifier and has no callee type).
        if matches!(call.callee, Expression::Super(_)) {
            return self.infer_super_call(scope, call, call_span);
        }

        // M9: an **explicit-type-argument generic call** (`identity<number>(5)`) is
        // instantiated by substitution *before* the usual checks. When the callee is a
        // registered generic function and type arguments are present, the instantiated
        // signature replaces the template; otherwise the callee is inferred normally.
        let instantiated_callee = self.instantiate_generic_callee(scope, call);

        // Always infer the callee expression for its side effects (resolving its name /
        // emitting TK2304, descending into a callee expression). Its inferred type is
        // used only when there was no explicit-args instantiation above.
        let inferred_callee = self.infer_expr(scope, &call.callee);

        // Infer arguments up front and build `arg_fresh` in the same loop so M24
        // clamp provenance stays index-aligned with skipped out-of-subset args.
        let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(call.arguments.len());
        let mut arg_fresh: Vec<bool> = Vec::with_capacity(call.arguments.len());
        let mut arg_exprs: Vec<&Expression<'_>> = Vec::with_capacity(call.arguments.len());
        for arg in &call.arguments {
            if let Some(arg_expr) = arg.as_expression() {
                if let Some(inferred) = self.infer_expr(scope, arg_expr) {
                    arg_types.push(inferred);
                    arg_fresh.push(is_fresh_literal(arg_expr));
                    arg_exprs.push(arg_expr);
                }
            }
            // A spread or an out-of-subset argument is not paired against a parameter.
        }

        // M10 inference runs only when M9 explicit instantiation did not; a
        // non-generic call falls through to the inferred callee unchanged.
        let inferred_generic_callee = if instantiated_callee.is_none() {
            self.infer_generic_callee(scope, call, &arg_types, &arg_fresh)
        } else {
            None
        };

        // Precedence: the M9 explicit instantiation, then the M10 inferred
        // instantiation, then the plainly-inferred callee type.
        let callee_ty = match instantiated_callee
            .or(inferred_generic_callee)
            .or(inferred_callee.map(|(ty, _)| ty))
        {
            Some(ty) => ty,
            None => return Some((wk.error, call_span)),
        };

        let Some(signature_ty) = self.callable_signature(callee_ty) else {
            return Some((wk.error, call_span));
        };

        // Snapshot the callee's parameter types + return type so the immutable store
        // borrow does not overlap pushing obligations / diagnostics below.
        let Some(func) = self.interner.store().function_type(signature_ty) else {
            return Some((wk.error, call_span));
        };
        let param_types: Vec<TypeId> = func.params.iter().map(|p| p.ty).collect();
        let ret = func.ret;

        // M25: a substituted PARAMETER type may be a now-concrete conditional too
        // (`g2<T>(t: T, c: T extends string ? "yes" : "no")`). Evaluate each (the same
        // demand as the return below) so valid calls pass and a mismatch reports against
        // the RESOLVED type — an unevaluated deferred node would reject every argument.
        let param_types: Vec<TypeId> = param_types
            .into_iter()
            .map(|param| self.evaluate_type(param, call_span))
            .collect();

        // Arity (TK2554) + per-argument assignability (TK2345), shared with `new`.
        self.check_call_arguments(scope, &param_types, &arg_types, &arg_exprs, call_span);

        // M25: a generic call's return type may be a conditional instantiated by the
        // inferred/explicit type arguments (`m("abc")` → `"abc" extends string ? … : …`).
        // Now that its check type is concrete, evaluate it (a no-op for any other type).
        let ret = self.evaluate_type(ret, call_span);

        Some((ret, call_span))
    }

    /// Callable signature after apparent-type resolution: a function type or an
    /// object's single call signature.
    fn callable_signature(&self, callee_ty: TypeId) -> Option<TypeId> {
        let callee_ty = self.apparent_type(callee_ty);
        match self.interner.store().tag(callee_ty) {
            TypeTag::Function => Some(callee_ty),
            TypeTag::Object => {
                let object = self.interner.store().object_type(callee_ty)?;
                let [signature] = object.call_signatures.as_slice() else {
                    return None;
                };
                Some(*signature)
            }
            _ => None,
        }
    }

    /// Check `super(args)` against the base constructor with the shared call
    /// machine. Arguments are always walked; missing base signatures collect no
    /// obligation and emit no `super`-specific diagnostic.
    fn infer_super_call(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
        call_span: Span,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();

        // Infer every argument up front (skipping spreads — out of subset); this descends
        // into nested calls/`new`/functions inside the arguments.
        let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(call.arguments.len());
        let mut arg_exprs: Vec<&Expression<'_>> = Vec::with_capacity(call.arguments.len());
        for arg in &call.arguments {
            if let Some(arg_expr) = arg.as_expression() {
                if let Some(inferred) = self.infer_expr(scope, arg_expr) {
                    arg_types.push(inferred);
                    arg_exprs.push(arg_expr);
                }
            }
        }

        // The base constructor signature in scope. Absent → no obligation, no diagnostic.
        let Some(super_ctor) = self.current_super_ctor else {
            return Some((wk.error, call_span));
        };
        let param_types: Vec<TypeId> = match self.interner.store().function_type(super_ctor) {
            Some(func) => func.params.iter().map(|p| p.ty).collect(),
            // Defensive: the constructor is always interned as a function in `fill_class`.
            None => return Some((wk.error, call_span)),
        };

        // Reuse the shared call-checking path: arity (TK2554) + argument assignability
        // (TK2345). The `super(...)` expression's value type is unused.
        self.check_call_arguments(scope, &param_types, &arg_types, &arg_exprs, call_span);

        Some((wk.error, call_span))
    }

    /// Shared M3 call/`new` argument checking: exact arity plus per-argument
    /// assignability. Fresh object/tuple literals use assignment-style diagnostics,
    /// matching tsc's literal-member reporting.
    fn check_call_arguments(
        &mut self,
        scope: ScopeId,
        param_types: &[TypeId],
        arg_types: &[(TypeId, Span)],
        arg_exprs: &[&Expression<'_>],
        call_span: Span,
    ) {
        if arg_types.len() != param_types.len() {
            self.diagnostics.push(Diagnostic::wrong_argument_count(
                call_span,
                param_types.len(),
                arg_types.len(),
            ));
        }

        for (((arg_ty, arg_span), arg_expr), param_ty) in
            arg_types.iter().zip(arg_exprs).zip(param_types)
        {
            let (src, src_span) = self.infer_contextual_source_after_walked(
                scope,
                arg_expr,
                *param_ty,
                (*arg_ty, *arg_span),
            );
            check_excess_properties(
                self.interner.store(),
                arg_expr,
                *param_ty,
                &mut self.diagnostics,
            );
            self.obligations.push(AssignObligation {
                src,
                tgt: *param_ty,
                src_span,
                kind: self.call_argument_obligation_kind(arg_expr, *param_ty),
            });
        }
    }

    fn call_argument_obligation_kind(
        &self,
        arg_expr: &Expression<'_>,
        param_ty: TypeId,
    ) -> ObligationKind {
        let context = contextual_literal_target(self.interner.store(), param_ty);
        match arg_expr {
            Expression::ParenthesizedExpression(paren) => {
                self.call_argument_obligation_kind(&paren.expression, context)
            }
            Expression::ObjectExpression(_)
                if self.interner.store().object_type(context).is_some() =>
            {
                ObligationKind::Assignment
            }
            Expression::ArrayExpression(_)
                if self.interner.store().tag(context) == TypeTag::Tuple =>
            {
                ObligationKind::Assignment
            }
            _ => ObligationKind::Argument,
        }
    }

    /// Infer/check `new ClassName(args)` and return the instance type. Direct class
    /// constructors use shared call checks; generic classes instantiate constructor
    /// and instance types first. Non-class callees are walked but yield the error
    /// type without a `new`-specific diagnostic.
    pub(in crate::check::checker) fn infer_new(
        &mut self,
        scope: ScopeId,
        new_expr: &NewExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();
        let new_span = Span::from_oxc(new_expr.span);

        // Resolve direct class identifiers before callee inference so unresolved
        // names still emit exactly one `TK2304`; keep `DeclId` for M16 generics.
        let class_resolved: Option<(DeclId, ClassInfo)> = match &new_expr.callee {
            Expression::Identifier(ident) => value_decl_id(self.binder, scope, ident.name.as_str())
                .and_then(|decl_id| {
                    self.class_ctors
                        .get(&decl_id)
                        .copied()
                        .map(|info| (decl_id, info))
                }),
            _ => None,
        };

        // Always infer the callee for its side effects (resolving its name / emitting
        // `TK2304`, descending into a callee expression). For non-class callees the
        // inferred type is also used to find an object construct signature.
        let inferred_callee = self.infer_expr(scope, &new_expr.callee);

        // Infer every argument up front (skipping spreads — out of subset); this descends
        // into nested calls/`new`/functions inside the arguments. `arg_fresh` mirrors
        // `infer_call`'s: which arguments are fresh object/array literals (M24 clamp
        // exemption), built in the same loop so the vecs stay index-aligned.
        let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(new_expr.arguments.len());
        let mut arg_fresh: Vec<bool> = Vec::with_capacity(new_expr.arguments.len());
        let mut arg_exprs: Vec<&Expression<'_>> = Vec::with_capacity(new_expr.arguments.len());
        for arg in &new_expr.arguments {
            if let Some(arg_expr) = arg.as_expression() {
                if let Some(inferred) = self.infer_expr(scope, arg_expr) {
                    arg_types.push(inferred);
                    arg_fresh.push(is_fresh_literal(arg_expr));
                    arg_exprs.push(arg_expr);
                }
            }
        }

        // Not a known class: WU3 falls through to a single object construct
        // signature. If the callee is not constructable in the represented subset,
        // preserve the previous no-diagnostic/error-type behavior.
        let Some((decl_id, info)) = class_resolved else {
            if let Some((callee_ty, _)) = inferred_callee {
                if let Some(signature_ty) = self.construct_signature(callee_ty) {
                    let Some(func) = self.interner.store().function_type(signature_ty) else {
                        return Some((wk.error, new_span));
                    };
                    let param_types: Vec<TypeId> = func.params.iter().map(|p| p.ty).collect();
                    let ret = func.ret;
                    self.check_call_arguments(
                        scope,
                        &param_types,
                        &arg_types,
                        &arg_exprs,
                        new_span,
                    );
                    return Some((ret, new_span));
                }
            }
            return Some((wk.error, new_span));
        };

        // Backlog 20: constructor accessibility on a direct `new C(...)`. A
        // `private`/`protected` constructor reachable only from inside its declaring
        // class (and, for `protected`, its subclasses) emits `TK2673`/`TK2674` on the
        // whole `new` span; returns whether the constructor was inaccessible.
        let ctor_inaccessible = self.check_new_accessibility(&info, new_span);

        // M15: only the directly named class's abstract flag matters. Still run
        // argument checks; suppress when constructor accessibility already reported,
        // matching tsc's single accessibility error in that combination.
        if info.is_abstract && !ctor_inaccessible {
            self.diagnostics
                .push(Diagnostic::abstract_instantiation(new_span));
        }

        // M16: instantiate a generic class's constructor + instance before the argument
        // checks. For a non-generic class this is the identity (`ctor`/`instance` unchanged),
        // so M11 behaviour is preserved. Explicit type arguments substitute directly; no type
        // arguments infer the parameters from the constructor argument types (M10 engine).
        let (ctor, instance) =
            self.new_class_substitution(scope, decl_id, &info, new_expr, &arg_types, &arg_fresh);

        // The (instantiated) constructor signature's parameter types (zero for an implicit
        // constructor).
        let param_types: Vec<TypeId> = match self.interner.store().function_type(ctor) {
            Some(func) => func.params.iter().map(|p| p.ty).collect(),
            // Defensive: the constructor is always interned as a function in `fill_class`.
            None => Vec::new(),
        };

        // Reuse the M3 call-checking path: arity (TK2554) + argument assignability
        // (TK2345). The `new` expression's type is the (instantiated) instance type.
        self.check_call_arguments(scope, &param_types, &arg_types, &arg_exprs, new_span);

        Some((instance, new_span))
    }

    /// Construct signature after apparent-type resolution: an object's single
    /// construct signature.
    fn construct_signature(&self, callee_ty: TypeId) -> Option<TypeId> {
        let callee_ty = self.apparent_type(callee_ty);
        if self.interner.store().tag(callee_ty) != TypeTag::Object {
            return None;
        }
        let object = self.interner.store().object_type(callee_ty)?;
        let [signature] = object.construct_signatures.as_slice() else {
            return None;
        };
        Some(*signature)
    }

    /// M16 generic-class substitution for `new`: explicit type args or M10
    /// constructor-argument inference build one map, then both constructor and
    /// instance type are substituted. Empty/non-generic maps are the M11 identity.
    fn new_class_substitution(
        &mut self,
        scope: ScopeId,
        decl_id: DeclId,
        info: &ClassInfo,
        new_expr: &NewExpression<'_>,
        arg_types: &[(TypeId, Span)],
        arg_fresh: &[bool],
    ) -> (TypeId, TypeId) {
        // Non-generic class: no parameters to substitute — the M11 identity.
        let Some(type_params) = self.class_type_params.get(&decl_id).cloned() else {
            return (info.ctor, info.instance);
        };

        let map: FxHashMap<TypeParamId, TypeId> = match new_expr.type_arguments.as_deref() {
            // Explicit type arguments: lower each and zip to the class's parameters.
            Some(args) => {
                let mut map = FxHashMap::default();
                // Kept aligned so the constraint check pairs each param with its own
                // argument even when an earlier one is unlowerable (out of subset).
                let mut checked_params: Vec<TypeParamId> = Vec::with_capacity(args.params.len());
                let mut arg_infos: Vec<(TypeId, Span)> = Vec::with_capacity(args.params.len());
                for (&param, arg) in type_params.iter().zip(&args.params) {
                    if let Some(lowered) = self.lower_annotation(scope, arg) {
                        map.insert(param, lowered);
                        checked_params.push(param);
                        arg_infos.push((lowered, Span::from_oxc(arg.span())));
                    }
                }
                // M24: each explicit type argument must satisfy its parameter's constraint.
                self.check_type_argument_constraints(&checked_params, &arg_infos, &map);
                map
            }
            // No type arguments: infer from the constructor argument types. The inference
            // targets are the *uninstantiated* constructor parameter types (they carry the
            // type parameters). Snapshot them before the mutable inference call.
            None => {
                let param_types: Vec<TypeId> = match self.interner.store().function_type(info.ctor)
                {
                    Some(func) => func.params.iter().map(|p| p.ty).collect(),
                    None => Vec::new(),
                };
                let args: Vec<TypeId> = arg_types.iter().map(|(ty, _)| *ty).collect();
                infer::infer_type_arguments(
                    self.interner,
                    &mut self.next_type_param,
                    &type_params,
                    &param_types,
                    &args,
                    arg_fresh,
                )
            }
        };

        // Apply the substitution to both the constructor signature and the instance type.
        // An empty map is the identity (`substitute` short-circuits), so this is safe even
        // when no parameter resolved.
        let ctor = substitute(self.interner, info.ctor, &map);
        let instance = substitute(self.interner, info.instance, &map);
        (ctor, instance)
    }

    /// Infer a function type and check its body. Generic functions push their type
    /// parameters for the whole signature/body and return the template plus ids for
    /// call-site instantiation.
    pub(in crate::check::checker) fn infer_function(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
    ) -> (TypeId, Vec<TypeParamId>) {
        let param_ids =
            alloc_type_param_ids(func.type_parameters.as_deref(), &mut self.next_type_param);
        let frame = self.build_type_param_frame(func.type_parameters.as_deref(), &param_ids);

        let fn_ty = self.with_type_params(frame, |pass| {
            // M24: lower the parameters' `extends` constraints with the frame active.
            pass.lower_type_param_constraints(enclosing, func.type_parameters.as_deref(), &param_ids);
            let fn_scope = pass
                .binder
                .fn_scopes
                .get(&(pass.current_module, func.span.start))
                .copied();
            let params = pass.lower_parameters(enclosing, fn_scope, &func.params);

            // Declared return type from the annotation, if any. Type references in the
            // signature resolve from the enclosing scope (where the type names live);
            // type parameters resolve through the pushed frame.
            let declared_ret = match func.return_type.as_ref() {
                Some(ann) => pass.lower_annotation(enclosing, &ann.type_annotation),
                None => None,
            };

            // Descend into the body (in the function scope) to check returns against a
            // declared return type and/or infer the return type from `return` statements.
            let body_scope = fn_scope.unwrap_or(enclosing);
            let inferred_ret = func
                .body
                .as_ref()
                .map(|body| pass.check_function_body(body_scope, body, declared_ret));

            let ret = resolve_return_type(pass.interner, declared_ret, inferred_ret);
            pass.interner.intern_function(FunctionType { params, ret })
        });

        (fn_ty, param_ids)
    }

    /// Infer an arrow's type and check its body. Generic arrow type parameters are
    /// scoped to the signature/body only; they are not registered for explicit
    /// call-site type arguments.
    pub(in crate::check::checker) fn infer_arrow(
        &mut self,
        enclosing: ScopeId,
        arrow: &ArrowFunctionExpression<'_>,
    ) -> TypeId {
        let param_ids =
            alloc_type_param_ids(arrow.type_parameters.as_deref(), &mut self.next_type_param);
        let frame = self.build_type_param_frame(arrow.type_parameters.as_deref(), &param_ids);
        self.with_type_params(frame, |pass| {
            // M24: lower the parameters' `extends` constraints with the frame active.
            pass.lower_type_param_constraints(
                enclosing,
                arrow.type_parameters.as_deref(),
                &param_ids,
            );
            pass.infer_arrow_inner(enclosing, arrow)
        })
    }

    /// The body of [`infer_arrow`], run with any type-parameter frame already pushed.
    fn infer_arrow_inner(
        &mut self,
        enclosing: ScopeId,
        arrow: &ArrowFunctionExpression<'_>,
    ) -> TypeId {
        let fn_scope = self
            .binder
            .fn_scopes
            .get(&(self.current_module, arrow.span.start))
            .copied();
        let params = self.lower_parameters(enclosing, fn_scope, &arrow.params);

        let declared_ret = match arrow.return_type.as_ref() {
            Some(ann) => self.lower_annotation(enclosing, &ann.type_annotation),
            None => None,
        };

        let body_scope = fn_scope.unwrap_or(enclosing);

        let inferred_ret = if let Some(body_expr) = arrow.get_expression() {
            // Expression body `() => expr`: the return value is the expression.
            let value = match declared_ret {
                Some(ret) => self.infer_initializer(body_scope, body_expr, Some(ret)),
                None => self.infer_expr(body_scope, body_expr),
            };
            match (declared_ret, value) {
                // With a declared return type, the body expression is checked against
                // it (primary span = the expression), like a `return <expr>`.
                (Some(ret), Some((src, src_span))) => {
                    check_excess_properties(
                        self.interner.store(),
                        body_expr,
                        ret,
                        &mut self.diagnostics,
                    );
                    self.obligations.push(AssignObligation {
                        src,
                        tgt: ret,
                        src_span,
                        kind: ObligationKind::Assignment,
                    });
                    None
                }
                // No annotation: infer the return type from the body, widened.
                (None, Some((value_ty, _))) => Some(widen(self.interner, value_ty)),
                _ => None,
            }
        } else {
            // Block body `() => { ... }`: same as a function body.
            Some(self.check_function_body(body_scope, &arrow.body, declared_ret))
        };

        let ret = resolve_return_type(self.interner, declared_ret, inferred_ret);
        self.interner.intern_function(FunctionType { params, ret })
    }

    /// Lower a function's/arrow's parameters to `ParameterType`s and, when a function
    /// scope is known, record each parameter's type in `decl_types` so the body can
    /// resolve it. An un-annotated parameter is out of the MVP subset → the error
    /// type (no diagnostic), matching M0/M1 leniency. Parameters are positional.
    fn lower_parameters(
        &mut self,
        enclosing: ScopeId,
        fn_scope: Option<ScopeId>,
        params: &FormalParameters<'_>,
    ) -> Vec<ParameterType> {
        let error_ty = self.interner.well_known().error;
        let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
        for param in &params.items {
            let name = parameter_name(&param.pattern).unwrap_or_default();
            // Annotated type, or the error type for an un-annotated parameter. Type
            // references in the annotation resolve from the enclosing scope.
            let ty = match param.type_annotation.as_ref() {
                Some(ann) => self
                    .lower_annotation(enclosing, &ann.type_annotation)
                    .unwrap_or(error_ty),
                None => error_ty,
            };

            // F4: object destructuring parameters run M13 access checks against the
            // annotation type only; binding destructured names is deferred. The
            // annotation resolves in the enclosing class context.
            if let BindingPattern::ObjectPattern(object) = &param.pattern {
                if param.type_annotation.is_some() {
                    self.check_object_pattern_access(object, ty);
                }
            }

            // Bind the parameter's type into the function scope so the body resolves
            // it (the binder declared the parameter symbol + DeclId).
            if let Some(scope) = fn_scope {
                if let Some(decl_id) = parameter_name(&param.pattern)
                    .and_then(|n| self.binder.graph.resolve(scope, &n))
                    .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                    .and_then(|s| s.value)
                {
                    self.decl_types.set(decl_id, ty);
                }
            }

            lowered.push(ParameterType {
                name,
                ty,
                optional: false,
            });
        }
        lowered
    }

    /// Walk a function body, checking returns against a declared type or inferring
    /// the first value return's widened type. Missing-return analysis (`TK2355`)
    /// remains deferred.
    fn check_function_body(
        &mut self,
        scope: ScopeId,
        body: &FunctionBody<'_>,
        declared_ret: Option<TypeId>,
    ) -> TypeId {
        let void_ty = self.interner.well_known().void;
        let mut inferred: Option<TypeId> = None;

        // M23: the function-boundary narrowing reset lives in the flow pre-pass (each
        // body is built at its own `START`, so a reference never sees the caller's
        // narrowing — the documented closure divergence). The check walk here just
        // descends into the body.
        for stmt in &body.statements {
            self.check_stmt(scope, stmt, declared_ret, &mut inferred);
        }

        inferred.unwrap_or(void_ty)
    }
}

/// The function's return type: a declared annotation always wins; otherwise the
/// inferred type; otherwise `void` (a function with no body and no annotation,
/// which is out of the subset but handled defensively).
fn resolve_return_type(
    interner: &mut Interner,
    declared: Option<TypeId>,
    inferred: Option<TypeId>,
) -> TypeId {
    declared
        .or(inferred)
        .unwrap_or_else(|| interner.well_known().void)
}

/// Whether a call/`new` argument is a fresh object/array literal for the M24
/// clamp-to-constraint exemption. Freshness is syntactic; parentheses are
/// transparent.
fn is_fresh_literal(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => true,
        Expression::ParenthesizedExpression(paren) => is_fresh_literal(&paren.expression),
        _ => false,
    }
}

/// The parameter name of a binding pattern, if it is a plain identifier. `None`
/// for destructuring patterns (out of the M3 subset).
pub(in crate::check::checker) fn parameter_name(pattern: &BindingPattern<'_>) -> Option<String> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.to_string()),
        _ => None,
    }
}

/// Widen a type: a literal widens to its base intrinsic (`1` → `number`); every
/// other type passes through unchanged.
pub(in crate::check::checker) fn widen(interner: &mut Interner, ty: TypeId) -> TypeId {
    match interner.store().literal_value(ty) {
        Some(lit) => intrinsic_id(interner.well_known(), lit.base_kind()),
        None => ty,
    }
}

/// Well-known id for an intrinsic kind (small helper mirroring the relater's).
pub(in crate::check::checker) fn intrinsic_id(wk: WellKnown, kind: IntrinsicKind) -> TypeId {
    match kind {
        IntrinsicKind::Error => wk.error,
        IntrinsicKind::Any => wk.any,
        IntrinsicKind::Unknown => wk.unknown,
        IntrinsicKind::Never => wk.never,
        IntrinsicKind::Void => wk.void,
        IntrinsicKind::Null => wk.null,
        IntrinsicKind::Undefined => wk.undefined,
        IntrinsicKind::Boolean => wk.boolean,
        IntrinsicKind::Number => wk.number,
        IntrinsicKind::String => wk.string,
        // M28 string-intrinsic markers.
        IntrinsicKind::Uppercase => wk.uppercase,
        IntrinsicKind::Lowercase => wk.lowercase,
        IntrinsicKind::Capitalize => wk.capitalize,
        IntrinsicKind::Uncapitalize => wk.uncapitalize,
    }
}

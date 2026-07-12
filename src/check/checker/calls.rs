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
use crate::types::repr::{
    FunctionType, GenericTypeParam, IntrinsicKind, ParameterType, TupleType, TypeParamId, TypeTag,
};
use crate::types::store::TypeId;
use crate::types::{instantiate_function, substitute, Interner, WellKnown};
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, CallExpression, Expression, FormalParameters,
    Function, FunctionBody, NewExpression, TSTypeParameterInstantiation,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Record the incomplete surface for a skipped spread call/`new` argument
    /// (`f(...xs)` / `new C(...xs)`, owner 71) — the argument collectors share this so
    /// no in-scope argument is silently dropped before arity/assignability checking.
    fn record_spread_argument_skip(&mut self, arg: &oxc_ast::ast::Argument<'_>) {
        self.record_incomplete(
            "call/call-arguments/spread-argument",
            Span::from_oxc(arg.span()),
            "spread call argument not visited",
        );
    }

    /// M24: check explicit type arguments against substituted constraints. The
    /// shared relation engine supplies `TK2344` reason chains; failed arguments
    /// still instantiate, matching tsc and avoiding cascades.
    pub(in crate::check::checker) fn check_type_argument_constraints(
        &mut self,
        type_params: &[TypeParamId],
        args: &[(TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) {
        let checks: Vec<(Option<TypeId>, TypeId, Span)> = type_params
            .iter()
            .zip(args)
            .map(|(&param, &(arg, span))| {
                (
                    self.interner.store().type_param_constraint(param),
                    arg,
                    span,
                )
            })
            .collect();
        self.check_constraint_arguments(&checks, map);
    }

    /// Check explicit function-signature arguments against their persistent
    /// descriptors. The descriptor can already have been rewritten by an outer
    /// class/interface substitution, unlike the declaration-side store column.
    fn check_signature_type_argument_constraints(
        &mut self,
        type_params: &[GenericTypeParam],
        args: &[(TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) {
        let checks: Vec<(Option<TypeId>, TypeId, Span)> = type_params
            .iter()
            .zip(args)
            .map(|(param, &(arg, span))| (param.constraint, arg, span))
            .collect();
        self.check_constraint_arguments(&checks, map);
    }

    /// Check already-lowered type arguments against constraint sources. Signature
    /// default validation shares this with call-site explicit arguments.
    pub(in crate::check::checker) fn check_constraint_arguments(
        &mut self,
        args: &[(Option<TypeId>, TypeId, Span)],
        map: &FxHashMap<TypeParamId, TypeId>,
    ) {
        // Build the (argument, substituted-constraint, span) checks up front — this needs
        // `&mut Interner` (substitution may intern new types), which cannot overlap the
        // relation engine's immutable store borrow below.
        let mut checks: Vec<(TypeId, TypeId, TypeId, Span)> = Vec::new();
        for &(raw_constraint, arg, span) in args {
            let Some(constraint) = raw_constraint else {
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
                Diagnostic::constraint_not_satisfied(span, &src, &tgt)
                    .with_elaboration(elaboration),
            );
        }
    }

    fn contextual_inference_args(
        &mut self,
        scope: ScopeId,
        params: &[ParameterType],
        arg_types: &[(TypeId, Span)],
        arg_exprs: &[&Expression<'_>],
    ) -> Vec<TypeId> {
        let targets = self.call_argument_targets(params, arg_types.len());
        arg_types
            .iter()
            .zip(arg_exprs)
            .zip(targets)
            .map(|(((arg_ty, arg_span), arg_expr), target)| {
                let Some(target) = target else {
                    return *arg_ty;
                };
                if self.should_keep_raw_array_inference_source(arg_expr, *arg_ty, target) {
                    return *arg_ty;
                }
                self.infer_contextual_source_after_walked(
                    scope,
                    arg_expr,
                    target,
                    (*arg_ty, *arg_span),
                    true,
                    false,
                )
                .0
            })
            .collect()
    }

    fn should_keep_raw_array_inference_source(
        &self,
        arg_expr: &Expression<'_>,
        arg_ty: TypeId,
        target: TypeId,
    ) -> bool {
        if !matches!(arg_expr, Expression::ArrayExpression(_)) {
            return false;
        }
        let Some(target_array) = self.interner.store().array_type(target) else {
            return false;
        };
        if self.interner.store().tag(target_array.element) != TypeTag::TypeParam {
            return false;
        }
        self.interner.store().array_type(arg_ty).is_some()
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

        // Always infer the callee expression for its side effects (resolving its name /
        // emitting TK2304, descending into a callee expression).
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
            } else {
                // A spread argument `f(...xs)` is not paired against a parameter (owner 71).
                self.record_spread_argument_skip(arg);
            }
        }

        let Some((callee_ty, _)) = inferred_callee else {
            return Some((wk.error, call_span));
        };
        let signatures = self.callable_signatures(callee_ty);
        if signatures.is_empty() {
            return Some((wk.error, call_span));
        }

        let Some(candidate) = self.select_call_candidate(
            scope,
            call,
            &signatures,
            PreparedCallArgs {
                types: &arg_types,
                fresh: &arg_fresh,
                exprs: &arg_exprs,
            },
            call_span,
        ) else {
            return Some((wk.error, call_span));
        };
        self.check_call_arguments(scope, &candidate.params, &arg_types, &arg_exprs, call_span);
        let ret = self.evaluate_type(candidate.ret, call_span);

        Some((ret, call_span))
    }

    /// Callable signatures after apparent-type resolution: a function type or an
    /// object's ordered call-signature list.
    fn callable_signatures(&self, callee_ty: TypeId) -> Vec<TypeId> {
        let callee_ty = self.apparent_type(callee_ty);
        match self.interner.store().tag(callee_ty) {
            TypeTag::Function => vec![callee_ty],
            TypeTag::Object => {
                let Some(object) = self.interner.store().object_type(callee_ty) else {
                    return Vec::new();
                };
                object.call_signatures.clone()
            }
            _ => Vec::new(),
        }
    }

    fn select_call_candidate(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
        signatures: &[TypeId],
        args: PreparedCallArgs<'_, '_>,
        call_span: Span,
    ) -> Option<CallCandidate> {
        let overload = signatures.len() > 1;
        if !overload {
            let signature = signatures.first().copied()?;
            return match self
                .instantiate_call_candidate(scope, call, signature, args, call_span, true)
            {
                Ok(candidate) => Some(candidate),
                Err(CandidateBuildFailure::Constraint(_))
                | Err(CandidateBuildFailure::Unavailable) => None,
            };
        }

        let mut arity_failures: Vec<CallArity> = Vec::new();
        let mut saw_non_arity_failure = false;
        let mut first_constraint_failure: Option<Vec<Diagnostic>> = None;

        for signature in signatures {
            let candidate = match self
                .instantiate_call_candidate(scope, call, *signature, args, call_span, false)
            {
                Ok(candidate) => candidate,
                Err(CandidateBuildFailure::Constraint(diagnostics)) => {
                    if first_constraint_failure.is_none() {
                        first_constraint_failure = Some(diagnostics);
                    }
                    saw_non_arity_failure = true;
                    continue;
                }
                Err(CandidateBuildFailure::Unavailable) => {
                    saw_non_arity_failure = true;
                    continue;
                }
            };
            match self.try_call_candidate(
                scope,
                &candidate.params,
                args.types,
                args.exprs,
                call_span,
            ) {
                CandidateTrial::Match => {
                    let committed = match self
                        .instantiate_call_candidate(scope, call, *signature, args, call_span, true)
                    {
                        Ok(candidate) => candidate,
                        Err(CandidateBuildFailure::Constraint(_))
                        | Err(CandidateBuildFailure::Unavailable) => return None,
                    };
                    return Some(committed);
                }
                CandidateTrial::Arity(arity) => arity_failures.push(arity),
                CandidateTrial::Mismatch => saw_non_arity_failure = true,
            }
        }

        if let Some(diagnostics) = first_constraint_failure {
            self.diagnostics.extend(diagnostics);
        } else if !arity_failures.is_empty() && !saw_non_arity_failure {
            self.emit_overload_arity_failure(&arity_failures, args.types.len(), call_span);
        } else {
            self.diagnostics
                .push(Diagnostic::no_overload_matches(call_span));
        }
        None
    }

    fn instantiate_call_candidate(
        &mut self,
        scope: ScopeId,
        call: &CallExpression<'_>,
        signature_ty: TypeId,
        args: PreparedCallArgs<'_, '_>,
        call_span: Span,
        commit_constraints: bool,
    ) -> Result<CallCandidate, CandidateBuildFailure> {
        self.instantiate_signature_candidate(
            scope,
            signature_ty,
            call.type_arguments.as_deref(),
            args,
            call_span,
            commit_constraints,
        )
    }

    /// Build one callable or constructable signature candidate from its persistent
    /// generic descriptors. Both `f<T>(...)` and `new C<T>(...)` share this path so
    /// outer-substituted constraints/defaults cannot fall back to stale store state.
    fn instantiate_signature_candidate(
        &mut self,
        scope: ScopeId,
        signature_ty: TypeId,
        type_arguments: Option<&TSTypeParameterInstantiation<'_>>,
        args: PreparedCallArgs<'_, '_>,
        call_span: Span,
        commit_constraints: bool,
    ) -> Result<CallCandidate, CandidateBuildFailure> {
        let generic_params = self
            .interner
            .store()
            .function_type(signature_ty)
            .map(|function| function.type_params.clone())
            .ok_or(CandidateBuildFailure::Unavailable)?;
        let instantiated = if generic_params.is_empty() {
            signature_ty
        } else {
            let map = match type_arguments {
                Some(type_arguments) => {
                    let mut arg_infos: Vec<(TypeId, Span)> =
                        Vec::with_capacity(type_arguments.params.len());
                    for arg in &type_arguments.params {
                        arg_infos.push((
                            self.lower_annotation(scope, arg)
                                .ok_or(CandidateBuildFailure::Unavailable)?,
                            Span::from_oxc(arg.span()),
                        ));
                    }
                    let min = generic_params
                        .iter()
                        .filter(|param| param.default.is_none())
                        .count();
                    let max = generic_params.len();
                    if arg_infos.len() < min || arg_infos.len() > max {
                        let diagnostic = Diagnostic::wrong_type_argument_count(
                            Span::from_oxc(type_arguments.span),
                            min,
                            max,
                            arg_infos.len(),
                        );
                        return Err(
                            self.candidate_constraint_failure(commit_constraints, diagnostic)
                        );
                    }
                    let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
                    for (param, &(arg, _)) in generic_params.iter().zip(&arg_infos) {
                        map.insert(param.id, arg);
                    }
                    let map = self.complete_signature_type_arguments(&generic_params, map);
                    if commit_constraints {
                        self.check_signature_type_argument_constraints(
                            &generic_params,
                            &arg_infos,
                            &map,
                        );
                    } else {
                        let diagnostics_len = self.diagnostics.len();
                        self.check_signature_type_argument_constraints(
                            &generic_params,
                            &arg_infos,
                            &map,
                        );
                        if self.diagnostics.len() != diagnostics_len {
                            let diagnostics = self.diagnostics[diagnostics_len..].to_vec();
                            self.diagnostics.truncate(diagnostics_len);
                            return Err(CandidateBuildFailure::Constraint(diagnostics));
                        }
                    }
                    map
                }
                None => {
                    let params: Vec<ParameterType> = self
                        .interner
                        .store()
                        .function_type(signature_ty)
                        .ok_or(CandidateBuildFailure::Unavailable)?
                        .params
                        .clone();
                    let inference_args =
                        self.contextual_inference_args(scope, &params, args.types, args.exprs);
                    infer::infer_signature_type_arguments_from_params(
                        self.interner,
                        &mut self.next_type_param,
                        &generic_params,
                        &params,
                        &inference_args,
                        args.fresh,
                    )
                }
            };
            instantiate_function(self.interner, signature_ty, &map)
        };

        let func = self
            .interner
            .store()
            .function_type(instantiated)
            .ok_or(CandidateBuildFailure::Unavailable)?;
        let params = func.params.clone();
        let ret = func.ret;
        let params = self.evaluate_parameters(params, call_span);
        Ok(CallCandidate { params, ret })
    }

    /// Fill omitted function binders in declaration order. Explicit arguments are
    /// retained verbatim (so their constraint error is reported as `TK2344`), while
    /// defaults observe the already-completed earlier bindings.
    fn complete_signature_type_arguments(
        &mut self,
        type_params: &[GenericTypeParam],
        mut map: FxHashMap<TypeParamId, TypeId>,
    ) -> FxHashMap<TypeParamId, TypeId> {
        let unknown = self.interner.well_known().unknown;
        for type_param in type_params {
            if map.contains_key(&type_param.id) {
                continue;
            }
            let value = type_param
                .default
                .map(|default| substitute(self.interner, default, &map))
                .or_else(|| {
                    type_param
                        .constraint
                        .map(|constraint| substitute(self.interner, constraint, &map))
                })
                .unwrap_or(unknown);
            map.insert(type_param.id, value);
        }
        map
    }

    fn candidate_constraint_failure(
        &mut self,
        commit: bool,
        diagnostic: Diagnostic,
    ) -> CandidateBuildFailure {
        if commit {
            self.diagnostics.push(diagnostic.clone());
        }
        CandidateBuildFailure::Constraint(vec![diagnostic])
    }

    fn try_call_candidate(
        &mut self,
        scope: ScopeId,
        params: &[ParameterType],
        arg_types: &[(TypeId, Span)],
        arg_exprs: &[&Expression<'_>],
        _call_span: Span,
    ) -> CandidateTrial {
        let arity = self.call_arity(params);
        if !self.call_arity_accepts(&arity, arg_types.len()) {
            return CandidateTrial::Arity(arity);
        }

        let targets = self.call_argument_targets(params, arg_types.len());
        let wk = self.interner.well_known();
        for (((arg_ty, arg_span), arg_expr), param_ty) in
            arg_types.iter().zip(arg_exprs).zip(targets)
        {
            let Some(param_ty) = param_ty else {
                continue;
            };
            let (src, _src_span) = self.infer_contextual_source_after_walked(
                scope,
                arg_expr,
                param_ty,
                (*arg_ty, *arg_span),
                true,
                false,
            );
            let diagnostics_len = self.diagnostics.len();
            check_excess_properties(
                self.interner.store(),
                arg_expr,
                param_ty,
                &mut self.diagnostics,
            );
            if self.diagnostics.len() != diagnostics_len {
                self.diagnostics.truncate(diagnostics_len);
                return CandidateTrial::Mismatch;
            }
            let store = self.interner.store();
            let mut relater = Relater::new(store, wk);
            if let Relation::No(_) = relater.is_assignable(src, param_ty) {
                return CandidateTrial::Mismatch;
            }
        }
        CandidateTrial::Match
    }

    fn call_arity_accepts(&self, arity: &CallArity, got: usize) -> bool {
        if got < arity.min {
            return false;
        }
        arity.max.is_none_or(|max| got <= max)
    }

    fn emit_overload_arity_failure(&mut self, arities: &[CallArity], got: usize, span: Span) {
        let min = arities.iter().map(|arity| arity.min).min().unwrap_or(0);
        let max = if arities.iter().any(|arity| arity.max.is_none()) {
            None
        } else {
            arities.iter().filter_map(|arity| arity.max).max()
        };
        let unbounded_rest = arities.iter().any(|arity| arity.unbounded_rest);
        let diagnostic = if got < min && unbounded_rest {
            Diagnostic::wrong_min_argument_count(span, min, got)
        } else {
            self.wrong_bounded_argument_count(span, min, max.unwrap_or(min), got)
        };
        self.diagnostics.push(diagnostic);
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
            } else {
                self.record_spread_argument_skip(arg);
            }
        }

        // The base constructor signature in scope. Absent → no obligation, no diagnostic.
        let Some(super_ctor) = self.current_super_ctor else {
            return Some((wk.error, call_span));
        };
        let params: Vec<ParameterType> = match self.interner.store().function_type(super_ctor) {
            Some(func) => func.params.clone(),
            // Defensive: the constructor is always interned as a function in `fill_class`.
            None => return Some((wk.error, call_span)),
        };
        let params = self.evaluate_parameters(params, call_span);

        // Reuse the shared call-checking path: arity (TK2554) + argument assignability
        // (TK2345). The `super(...)` expression's value type is unused.
        self.check_call_arguments(scope, &params, &arg_types, &arg_exprs, call_span);

        Some((wk.error, call_span))
    }

    /// Shared M3 call/`new` argument checking: arity plus per-argument
    /// assignability. Fresh object/tuple literals use assignment-style diagnostics,
    /// matching tsc's literal-member reporting.
    fn check_call_arguments(
        &mut self,
        scope: ScopeId,
        params: &[ParameterType],
        arg_types: &[(TypeId, Span)],
        arg_exprs: &[&Expression<'_>],
        call_span: Span,
    ) {
        self.check_call_arity(params, arg_types.len(), call_span);

        let targets = self.call_argument_targets(params, arg_types.len());
        for (((arg_ty, arg_span), arg_expr), param_ty) in
            arg_types.iter().zip(arg_exprs).zip(targets)
        {
            let Some(param_ty) = param_ty else {
                continue;
            };
            let (src, src_span) = self.infer_contextual_source_after_walked(
                scope,
                arg_expr,
                param_ty,
                (*arg_ty, *arg_span),
                true,
                true,
            );
            check_excess_properties(
                self.interner.store(),
                arg_expr,
                param_ty,
                &mut self.diagnostics,
            );
            self.obligations.push(AssignObligation {
                src,
                tgt: param_ty,
                src_span,
                kind: self.call_argument_obligation_kind(arg_expr, param_ty),
            });
        }
    }

    fn evaluate_parameters(
        &mut self,
        params: Vec<ParameterType>,
        span: Span,
    ) -> Vec<ParameterType> {
        params
            .into_iter()
            .map(|mut param| {
                param.ty = self.evaluate_type(param.ty, span);
                param
            })
            .collect()
    }

    fn check_call_arity(&mut self, params: &[ParameterType], got: usize, span: Span) {
        let arity = self.call_arity(params);
        if got < arity.min {
            let diagnostic = if arity.unbounded_rest {
                Diagnostic::wrong_min_argument_count(span, arity.min, got)
            } else {
                self.wrong_bounded_argument_count(
                    span,
                    arity.min,
                    arity.max.unwrap_or(arity.min),
                    got,
                )
            };
            self.diagnostics.push(diagnostic);
            return;
        }
        if let Some(max) = arity.max {
            if got > max {
                let diagnostic = self.wrong_bounded_argument_count(span, arity.min, max, got);
                self.diagnostics.push(diagnostic);
            }
        }
    }

    fn wrong_bounded_argument_count(
        &self,
        span: Span,
        min: usize,
        max: usize,
        got: usize,
    ) -> Diagnostic {
        if min == max {
            Diagnostic::wrong_argument_count(span, min, got)
        } else {
            Diagnostic::wrong_argument_count_range(span, min, max, got)
        }
    }

    fn call_arity(&self, params: &[ParameterType]) -> CallArity {
        let function = FunctionType {
            type_params: Vec::new(),
            params: params.to_vec(),
            ret: self.interner.well_known().void,
        };
        let fixed = function.total_fixed_param_count();
        let mut min = function.required_param_count();
        let mut max = Some(fixed);
        let mut unbounded_rest = false;
        if let Some(rest) = function.rest_param() {
            let rest_arity = self.rest_parameter_arity(rest.ty);
            min += rest_arity.min;
            max = rest_arity.max.map(|rest_max| fixed + rest_max);
            unbounded_rest = rest_arity.max.is_none();
        }
        CallArity {
            min,
            max,
            unbounded_rest,
        }
    }

    fn rest_parameter_arity(&self, rest_ty: TypeId) -> RestArity {
        if let Some(shape) = self.rest_call_shape(rest_ty) {
            return RestArity {
                min: shape.min_len(),
                max: shape.max_len(),
            };
        }
        RestArity { min: 0, max: None }
    }

    fn call_argument_targets(
        &self,
        params: &[ParameterType],
        arg_count: usize,
    ) -> Vec<Option<TypeId>> {
        let fixed: Vec<&ParameterType> = params.iter().filter(|param| param.is_fixed()).collect();
        let rest = params.iter().find(|param| param.rest);
        let total_rest_args = arg_count.saturating_sub(fixed.len());
        (0..arg_count)
            .map(|index| {
                if let Some(param) = fixed.get(index) {
                    return Some(param.ty);
                }
                let rest = rest?;
                self.rest_argument_target(rest.ty, index - fixed.len(), total_rest_args)
            })
            .collect()
    }

    fn rest_argument_target(
        &self,
        rest_ty: TypeId,
        offset: usize,
        total_rest_args: usize,
    ) -> Option<TypeId> {
        self.rest_call_shape(rest_ty)?
            .element_at(offset, total_rest_args)
    }

    fn rest_call_shape(&self, rest_ty: TypeId) -> Option<RestCallShape> {
        let rest_ty = self
            .interner
            .store()
            .readonly_operand(rest_ty)
            .unwrap_or(rest_ty);
        if let Some(array) = self.interner.store().array_type(rest_ty) {
            return Some(RestCallShape {
                prefix: Vec::new(),
                variadic: Some(array.element),
                suffix: Vec::new(),
            });
        }
        if let Some(tuple) = self.interner.store().tuple_type(rest_ty) {
            return self.tuple_call_shape(tuple);
        }
        Some(RestCallShape {
            prefix: Vec::new(),
            variadic: Some(rest_ty),
            suffix: Vec::new(),
        })
    }

    fn tuple_call_shape(&self, tuple: &TupleType) -> Option<RestCallShape> {
        let Some(rest) = tuple.rest else {
            return Some(RestCallShape {
                prefix: tuple.elements.clone(),
                variadic: None,
                suffix: Vec::new(),
            });
        };
        if rest.position > tuple.elements.len() {
            return None;
        }
        let mut prefix = tuple.elements[..rest.position].to_vec();
        let suffix = tuple.elements[rest.position..].to_vec();
        let rest_shape = self.rest_call_shape(rest.ty)?;
        prefix.extend(rest_shape.prefix);
        let mut combined_suffix = rest_shape.suffix;
        combined_suffix.extend(suffix);
        Some(RestCallShape {
            prefix,
            variadic: rest_shape.variadic,
            suffix: combined_suffix,
        })
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
                ObligationKind::FreshArgument
            }
            Expression::ArrayExpression(_)
                if self.interner.store().tag(context) == TypeTag::Tuple =>
            {
                ObligationKind::FreshArgument
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
            } else {
                self.record_spread_argument_skip(arg);
            }
        }

        // Not a known class: WU3 falls through to a single object construct
        // signature. If the callee is not constructable in the represented subset,
        // preserve the previous no-diagnostic/error-type behavior.
        let Some((decl_id, info)) = class_resolved else {
            if let Some((callee_ty, _)) = inferred_callee {
                let signatures = self.construct_signatures(callee_ty);
                if !signatures.is_empty() {
                    if let Some(candidate) = self.select_construct_candidate(
                        scope,
                        &signatures,
                        new_expr.type_arguments.as_deref(),
                        PreparedCallArgs {
                            types: &arg_types,
                            fresh: &arg_fresh,
                            exprs: &arg_exprs,
                        },
                        new_span,
                    ) {
                        self.check_call_arguments(
                            scope,
                            &candidate.params,
                            &arg_types,
                            &arg_exprs,
                            new_span,
                        );
                        return Some((candidate.ret, new_span));
                    }
                    return Some((wk.error, new_span));
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

        if let Some(overloads) = self.class_ctor_overloads.get(&decl_id).cloned() {
            if let Some(candidate) = self.select_construct_candidate(
                scope,
                &overloads,
                new_expr.type_arguments.as_deref(),
                PreparedCallArgs {
                    types: &arg_types,
                    fresh: &arg_fresh,
                    exprs: &arg_exprs,
                },
                new_span,
            ) {
                self.check_call_arguments(
                    scope,
                    &candidate.params,
                    &arg_types,
                    &arg_exprs,
                    new_span,
                );
                return Some((info.instance, new_span));
            }
            return Some((wk.error, new_span));
        }

        // M16: instantiate a generic class's constructor + instance before the argument
        // checks. For a non-generic class this is the identity (`ctor`/`instance` unchanged),
        // so M11 behaviour is preserved. Explicit type arguments substitute directly; no type
        // arguments infer the parameters from the constructor argument types (M10 engine).
        let (ctor, instance) = self.new_class_substitution(
            scope,
            decl_id,
            &info,
            new_expr,
            (&arg_types, &arg_fresh, &arg_exprs),
        );

        // The (instantiated) constructor signature's parameter types (zero for an implicit
        // constructor).
        let params: Vec<ParameterType> = match self.interner.store().function_type(ctor) {
            Some(func) => func.params.clone(),
            // Defensive: the constructor is always interned as a function in `fill_class`.
            None => Vec::new(),
        };
        let params = self.evaluate_parameters(params, new_span);

        // Reuse the M3 call-checking path: arity (TK2554) + argument assignability
        // (TK2345). The `new` expression's type is the (instantiated) instance type.
        self.check_call_arguments(scope, &params, &arg_types, &arg_exprs, new_span);

        Some((instance, new_span))
    }

    /// Construct signatures after apparent-type resolution.
    fn construct_signatures(&self, callee_ty: TypeId) -> Vec<TypeId> {
        let callee_ty = self.apparent_type(callee_ty);
        if self.interner.store().tag(callee_ty) != TypeTag::Object {
            return Vec::new();
        }
        let Some(object) = self.interner.store().object_type(callee_ty) else {
            return Vec::new();
        };
        object.construct_signatures.clone()
    }

    fn select_construct_candidate(
        &mut self,
        scope: ScopeId,
        signatures: &[TypeId],
        type_arguments: Option<&TSTypeParameterInstantiation<'_>>,
        args: PreparedCallArgs<'_, '_>,
        span: Span,
    ) -> Option<CallCandidate> {
        let overload = signatures.len() > 1;
        if !overload {
            let signature = signatures.first().copied()?;
            return match self.instantiate_signature_candidate(
                scope,
                signature,
                type_arguments,
                args,
                span,
                true,
            ) {
                Ok(candidate) => Some(candidate),
                Err(CandidateBuildFailure::Constraint(_))
                | Err(CandidateBuildFailure::Unavailable) => None,
            };
        }

        let mut arity_failures: Vec<CallArity> = Vec::new();
        let mut saw_non_arity_failure = false;
        let mut first_constraint_failure: Option<Vec<Diagnostic>> = None;

        for signature in signatures {
            let candidate = match self.instantiate_signature_candidate(
                scope,
                *signature,
                type_arguments,
                args,
                span,
                false,
            ) {
                Ok(candidate) => candidate,
                Err(CandidateBuildFailure::Constraint(diagnostics)) => {
                    if first_constraint_failure.is_none() {
                        first_constraint_failure = Some(diagnostics);
                    }
                    saw_non_arity_failure = true;
                    continue;
                }
                Err(CandidateBuildFailure::Unavailable) => {
                    saw_non_arity_failure = true;
                    continue;
                }
            };
            match self.try_call_candidate(scope, &candidate.params, args.types, args.exprs, span) {
                CandidateTrial::Match => {
                    let committed = match self.instantiate_signature_candidate(
                        scope,
                        *signature,
                        type_arguments,
                        args,
                        span,
                        true,
                    ) {
                        Ok(candidate) => candidate,
                        Err(CandidateBuildFailure::Constraint(_))
                        | Err(CandidateBuildFailure::Unavailable) => return None,
                    };
                    return Some(committed);
                }
                CandidateTrial::Arity(arity) => arity_failures.push(arity),
                CandidateTrial::Mismatch => saw_non_arity_failure = true,
            }
        }

        if let Some(diagnostics) = first_constraint_failure {
            self.diagnostics.extend(diagnostics);
        } else if !arity_failures.is_empty() && !saw_non_arity_failure {
            self.emit_overload_arity_failure(&arity_failures, args.types.len(), span);
        } else {
            self.diagnostics.push(Diagnostic::no_overload_matches(span));
        }
        None
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
        args: (&[(TypeId, Span)], &[bool], &[&Expression<'_>]),
    ) -> (TypeId, TypeId) {
        let (arg_types, arg_fresh, arg_exprs) = args;
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
                let params: Vec<ParameterType> =
                    match self.interner.store().function_type(info.ctor) {
                        Some(func) => func.params.clone(),
                        None => Vec::new(),
                    };
                let args = self.contextual_inference_args(scope, &params, arg_types, arg_exprs);
                infer::infer_type_arguments_from_params(
                    self.interner,
                    &mut self.next_type_param,
                    &type_params,
                    &params,
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

    /// Reserve a function declaration's callable signature before its body is checked.
    /// Generic ids, constraints, parameters, and a declared return are established
    /// exactly once so callers can use the surface during the body-fill phase.
    pub(in crate::check::checker) fn reserve_function(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
    ) -> FunctionSurface {
        let diagnostics_start = self.diagnostics.len();
        let incomplete_start = self.incomplete.len();
        let type_params =
            alloc_type_param_ids(func.type_parameters.as_deref(), &mut self.next_type_param);
        let type_param_frame =
            self.build_type_param_frame(func.type_parameters.as_deref(), &type_params);
        let (generic_params, params, declared_return) =
            self.with_type_params(type_param_frame.clone(), |pass| {
                let generic_params = pass.lower_signature_type_params(
                    enclosing,
                    func.type_parameters.as_deref(),
                    &type_params,
                );
                let fn_scope = pass
                    .binder
                    .fn_scopes
                    .get(&(pass.current_module, func.span.start))
                    .copied();
                let params = pass.lower_parameters(enclosing, fn_scope, &func.params, false);
                // Type references in the signature resolve from the enclosing scope,
                // while declared type parameters resolve through the pushed frame.
                let declared_return = func
                    .return_type
                    .as_ref()
                    .and_then(|ann| pass.lower_annotation(enclosing, &ann.type_annotation));
                (generic_params, params, declared_return)
            });
        let ret = declared_return.unwrap_or_else(|| {
            let well_known = self.interner.well_known();
            if func.body.is_some() {
                // Backlog 76 owns pre-body return inference. `unknown` keeps forward
                // reads conservative without pretending the function returns `void`.
                well_known.unknown
            } else {
                well_known.void
            }
        });
        let function_ty = self.interner.intern_function(FunctionType {
            type_params: generic_params.clone(),
            params: params.clone(),
            ret,
        });
        let diagnostics = self.diagnostics.split_off(diagnostics_start);
        let incomplete = self.incomplete.split_off(incomplete_start);
        FunctionSurface {
            params,
            generic_params,
            type_param_frame,
            declared_return,
            function_ty,
            diagnostics,
            incomplete,
        }
    }

    /// Replay eager signature-lowering records at the declaration's source position.
    /// Function expressions and methods call this immediately; declarations defer it
    /// until their source walk reaches the reserved surface.
    pub(in crate::check::checker) fn replay_function_surface_records(
        &mut self,
        surface: &mut FunctionSurface,
    ) {
        self.diagnostics.append(&mut surface.diagnostics);
        for record in std::mem::take(&mut surface.incomplete) {
            if self
                .incomplete
                .iter()
                .any(|existing| existing.id == record.id && existing.span == record.span)
            {
                continue;
            }
            self.incomplete.push(record);
        }
    }

    /// Check a previously reserved function body and return its completed callable
    /// type. Reservation has already installed parameters and constraints, so this
    /// pass visits only the body and cannot duplicate signature diagnostics.
    pub(in crate::check::checker) fn fill_reserved_function(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
        surface: &FunctionSurface,
    ) -> TypeId {
        let params = surface.params.clone();
        let generic_params = surface.generic_params.clone();
        self.with_type_params(surface.type_param_frame.clone(), |pass| {
            let fn_scope = pass
                .binder
                .fn_scopes
                .get(&(pass.current_module, func.span.start))
                .copied();
            let body_scope = fn_scope.unwrap_or(enclosing);
            pass.check_reserved_parameter_initializers(body_scope, &func.params, &surface.params);
            let inferred_return = func
                .body
                .as_ref()
                .map(|body| pass.check_function_body(body_scope, body, surface.declared_return));
            let ret = resolve_return_type(pass.interner, surface.declared_return, inferred_return);
            pass.interner.intern_function(FunctionType {
                type_params: generic_params,
                params,
                ret,
            })
        })
    }

    /// Infer a function expression or class member type and check its body. Function
    /// declarations use the reserve/fill split above so their callable surfaces can
    /// be published to forward calls before executable statements are checked.
    pub(in crate::check::checker) fn infer_function(
        &mut self,
        enclosing: ScopeId,
        func: &Function<'_>,
    ) -> TypeId {
        let mut surface = self.reserve_function(enclosing, func);
        self.replay_function_surface_records(&mut surface);
        self.fill_reserved_function(enclosing, func, &surface)
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
        let params = self.lower_parameters(enclosing, fn_scope, &arrow.params, true);

        let declared_ret = match arrow.return_type.as_ref() {
            Some(ann) => self.lower_annotation(enclosing, &ann.type_annotation),
            None => None,
        };

        self.finish_arrow_inference(enclosing, arrow, fn_scope, params, declared_ret)
    }

    /// Re-infer a non-generic arrow against a function parameter's shape. This is
    /// intentionally limited to call-site contextual typing: the first uncontextual
    /// walk has already handled side effects, while this pass supplies the callback
    /// parameter types needed for generic method inference and checking.
    pub(in crate::check::checker) fn infer_contextual_arrow(
        &mut self,
        enclosing: ScopeId,
        arrow: &ArrowFunctionExpression<'_>,
        context: TypeId,
    ) -> Option<TypeId> {
        if arrow.type_parameters.is_some() {
            return None;
        }
        let context = self.interner.store().function_type(context).cloned()?;
        let fn_scope = self
            .binder
            .fn_scopes
            .get(&(self.current_module, arrow.span.start))
            .copied();
        let params = self.lower_contextual_arrow_parameters(
            enclosing,
            fn_scope,
            &arrow.params,
            &context.params,
        );
        let declared_ret = match arrow.return_type.as_ref() {
            Some(ann) => self.lower_annotation(enclosing, &ann.type_annotation),
            // An unresolved generic return is the inference variable itself. Infer
            // the callback return normally so that variable can receive a candidate;
            // a concrete expected return is checked in the arrow body instead.
            None if self.interner.store().tag(context.ret) == TypeTag::TypeParam => None,
            None => Some(context.ret),
        };
        Some(self.finish_arrow_inference(enclosing, arrow, fn_scope, params, declared_ret))
    }

    fn lower_contextual_arrow_parameters(
        &mut self,
        enclosing: ScopeId,
        fn_scope: Option<ScopeId>,
        params: &FormalParameters<'_>,
        context: &[ParameterType],
    ) -> Vec<ParameterType> {
        let error_ty = self.interner.well_known().error;
        let mut lowered =
            Vec::with_capacity(params.items.len() + usize::from(params.rest.is_some()));
        for (index, param) in params.items.iter().enumerate() {
            let name = parameter_name(&param.pattern).unwrap_or_default();
            let annotation_ty = param
                .type_annotation
                .as_ref()
                .and_then(|ann| self.lower_annotation(enclosing, &ann.type_annotation));
            let ty = annotation_ty.unwrap_or_else(|| {
                context
                    .get(index)
                    .map(|parameter| parameter.ty)
                    .unwrap_or(error_ty)
            });
            if let Some(scope) = fn_scope {
                if let Some(decl_id) = parameter_name(&param.pattern)
                    .and_then(|name| self.binder.resolve_value(scope, &name))
                    .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                    .and_then(|symbol| symbol.value)
                {
                    self.decl_types.set(decl_id, ty);
                }
            }
            lowered.push(parameter_from_shape(
                name,
                ty,
                param.optional,
                param.initializer.is_some(),
            ));
        }
        if let Some(rest) = &params.rest {
            let name = parameter_name(&rest.rest.argument).unwrap_or_default();
            let annotation_ty = rest
                .type_annotation
                .as_ref()
                .and_then(|ann| self.lower_annotation(enclosing, &ann.type_annotation));
            let ty = annotation_ty.unwrap_or_else(|| {
                context
                    .iter()
                    .find(|parameter| parameter.rest)
                    .map(|parameter| parameter.ty)
                    .unwrap_or(error_ty)
            });
            if let Some(scope) = fn_scope {
                if let Some(decl_id) = parameter_name(&rest.rest.argument)
                    .and_then(|name| self.binder.resolve_value(scope, &name))
                    .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                    .and_then(|symbol| symbol.value)
                {
                    self.decl_types.set(decl_id, ty);
                }
            }
            lowered.push(ParameterType::rest(name, ty));
        }
        lowered
    }

    fn finish_arrow_inference(
        &mut self,
        enclosing: ScopeId,
        arrow: &ArrowFunctionExpression<'_>,
        fn_scope: Option<ScopeId>,
        params: Vec<ParameterType>,
        declared_ret: Option<TypeId>,
    ) -> TypeId {
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
        self.interner.intern_function(FunctionType {
            type_params: Vec::new(),
            params,
            ret,
        })
    }

    /// Lower a function's/arrow's parameters to `ParameterType`s and, when a function
    /// scope is known, record each parameter's type in `decl_types` so the body can
    /// resolve it. An un-annotated parameter is out of the MVP subset → the error
    /// type (no diagnostic), matching M0/M1 leniency.
    fn lower_parameters(
        &mut self,
        enclosing: ScopeId,
        fn_scope: Option<ScopeId>,
        params: &FormalParameters<'_>,
        check_initializers: bool,
    ) -> Vec<ParameterType> {
        let error_ty = self.interner.well_known().error;
        let mut lowered: Vec<ParameterType> =
            Vec::with_capacity(params.items.len() + usize::from(params.rest.is_some()));
        let parameter_scope = fn_scope.unwrap_or(enclosing);
        for param in &params.items {
            let name = parameter_name(&param.pattern).unwrap_or_default();
            // Annotated type, or the error type for an un-annotated parameter. Type
            // references in the annotation resolve from the enclosing scope.
            let annotation_ty = param
                .type_annotation
                .as_ref()
                .and_then(|ann| self.lower_annotation(enclosing, &ann.type_annotation));
            let ty = if param.type_annotation.is_some() {
                annotation_ty.unwrap_or(error_ty)
            } else {
                error_ty
            };

            // F4: object destructuring parameters run M13 access checks against the
            // annotation type only; binding destructured names is deferred. The
            // annotation resolves in the enclosing class context.
            if let BindingPattern::ObjectPattern(object) = &param.pattern {
                if param.type_annotation.is_some() {
                    self.check_object_pattern_access(object, ty);
                }
            }

            if check_initializers {
                if let (Some(init), Some(annotation_ty)) = (&param.initializer, annotation_ty) {
                    self.check_annotated_initializer(parameter_scope, Some(annotation_ty), init);
                }
            }

            // Bind the parameter's type into the function scope so the body resolves
            // it (the binder declared the parameter symbol + DeclId).
            if let Some(scope) = fn_scope {
                if let Some(decl_id) = parameter_name(&param.pattern)
                    .and_then(|n| self.binder.resolve_value(scope, &n))
                    .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                    .and_then(|s| s.value)
                {
                    self.decl_types.set(decl_id, ty);
                }
            }

            lowered.push(parameter_from_shape(
                name,
                ty,
                param.optional,
                param.initializer.is_some(),
            ));
        }
        if let Some(rest) = &params.rest {
            let name = parameter_name(&rest.rest.argument).unwrap_or_default();
            let ty = match rest.type_annotation.as_ref() {
                Some(ann) => self
                    .lower_annotation(enclosing, &ann.type_annotation)
                    .unwrap_or(error_ty),
                None => error_ty,
            };
            if let Some(scope) = fn_scope {
                if let Some(decl_id) = parameter_name(&rest.rest.argument)
                    .and_then(|n| self.binder.resolve_value(scope, &n))
                    .and_then(|symbol_id| self.binder.symbols.get(symbol_id))
                    .and_then(|s| s.value)
                {
                    self.decl_types.set(decl_id, ty);
                }
            }
            lowered.push(ParameterType::rest(name, ty));
        }
        lowered
    }

    /// Parameter defaults are executable expressions, so declaration pre-reservation
    /// leaves them until the original function source position. Their parameter types
    /// were lowered once into the reserved surface.
    fn check_reserved_parameter_initializers(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        lowered: &[ParameterType],
    ) {
        for (param, parameter) in params.items.iter().zip(lowered) {
            if param.type_annotation.is_some() {
                if let Some(init) = &param.initializer {
                    self.check_annotated_initializer(scope, Some(parameter.ty), init);
                }
            }
        }
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
        // descends into the body, via the shared list walker so a *local* overload
        // set is grouped exactly like a top-level one (M33).
        self.check_statement_list(scope, &body.statements, declared_ret, &mut inferred);

        inferred.unwrap_or(void_ty)
    }
}

struct CallCandidate {
    params: Vec<ParameterType>,
    ret: TypeId,
}

#[derive(Copy, Clone)]
struct PreparedCallArgs<'a, 'ast> {
    types: &'a [(TypeId, Span)],
    fresh: &'a [bool],
    exprs: &'a [&'a Expression<'ast>],
}

enum CandidateTrial {
    Match,
    Arity(CallArity),
    Mismatch,
}

enum CandidateBuildFailure {
    Constraint(Vec<Diagnostic>),
    Unavailable,
}

#[derive(Clone)]
struct CallArity {
    min: usize,
    max: Option<usize>,
    unbounded_rest: bool,
}

struct RestArity {
    min: usize,
    max: Option<usize>,
}

struct RestCallShape {
    prefix: Vec<TypeId>,
    variadic: Option<TypeId>,
    suffix: Vec<TypeId>,
}

impl RestCallShape {
    fn min_len(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    fn max_len(&self) -> Option<usize> {
        if self.variadic.is_some() {
            None
        } else {
            Some(self.min_len())
        }
    }

    fn accepts_len(&self, len: usize) -> bool {
        if len < self.min_len() {
            return false;
        }
        self.variadic.is_some() || len == self.min_len()
    }

    fn element_at(&self, index: usize, len: usize) -> Option<TypeId> {
        if !self.accepts_len(len) || index >= len {
            return None;
        }
        if index < self.prefix.len() {
            return self.prefix.get(index).copied();
        }
        let suffix_start = len.saturating_sub(self.suffix.len());
        if index >= suffix_start {
            return self.suffix.get(index - suffix_start).copied();
        }
        self.variadic
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

fn parameter_from_shape(
    name: impl Into<String>,
    ty: TypeId,
    optional: bool,
    has_default: bool,
) -> ParameterType {
    if has_default {
        ParameterType::defaulted(name, ty)
    } else if optional {
        ParameterType::optional(name, ty)
    } else {
        ParameterType::required(name, ty)
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

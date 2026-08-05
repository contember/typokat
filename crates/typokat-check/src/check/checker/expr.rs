//! expr module (extracted from checker/mod.rs).

use super::calls::widen;
use super::classes::body::BodyMemberLookup;
use super::context::*;
use super::function_groups::FunctionGroupDemand;
use super::library_identities::{LibraryComposedMember, LibraryMemberProjection};
use crate::binder::bind::{ResolvedValueKind, ValueResolution};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::flow::{narrow_by_truthiness, FlowNodeId};
use crate::class_semantics::DemandOutcome;
use crate::diagnostics::{render_type, Diagnostic};
use crate::relate::RelationOutcome;
use crate::span::Span;
use crate::types::repr::{
    ClassId, FunctionType, IntrinsicKind, LiteralValue, ObjectType, PropertyKey, PropertyType,
    TypeParamId, TypeTag, Visibility,
};
use crate::types::store::{Store, TypeId};
use crate::types::{Interner, WellKnown};
use oxc_ast::ast::{
    ArrayExpression, ArrayExpressionElement, BinaryExpression, BinaryOperator, ChainElement,
    ComputedMemberExpression, ConditionalExpression, Expression, LogicalExpression,
    LogicalOperator, ObjectExpression, ObjectPropertyKind, SimpleAssignmentTarget,
    StaticMemberExpression, TSType, TSTypeName, UnaryExpression, UnaryOperator,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};

/// Whether a contextual re-walk actually walked the expression again.
///
/// A call argument that a committed contextual walk re-walks is walked twice, and
/// only one of the two walks may report. `Rewalked` is the only answer that lets a
/// caller treat the earlier raw walk as superseded; `KeptRaw` means this walk never
/// entered the expression, so the raw walk is still its only walk (backlog `92`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ContextualRewalk {
    Rewalked,
    KeptRaw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ElementAccessLookup {
    Found(TypeId),
    MissingObjectKey,
    NullishReceiver,
    UnsupportedReceiver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ExactSymbolReceiverGuard {
    Continue,
    Recovery(TypeId),
    Unknown,
    Nullish,
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Infer the type of an expression in `scope`, returning `(TypeId, span)`. The
    /// span is the expression's own span — the primary span for any diagnostic on it.
    /// Returns `None` for expression shapes outside the subset (those positions are
    /// simply not checked, matching M0 leniency).
    pub(in crate::check::checker) fn infer_expr(
        &mut self,
        scope: ScopeId,
        expr: &Expression<'_>,
    ) -> Option<(TypeId, Span)> {
        let well_known = self.interner.well_known();
        match expr {
            Expression::NumericLiteral(lit) => {
                let id = self
                    .interner
                    .intern_literal(LiteralValue::Number(lit.value));
                Some((id, Span::from_oxc(lit.span)))
            }
            Expression::StringLiteral(lit) => {
                let id = self
                    .interner
                    .intern_literal(LiteralValue::String(lit.value.to_string()));
                Some((id, Span::from_oxc(lit.span)))
            }
            Expression::BooleanLiteral(lit) => {
                let id = self
                    .interner
                    .intern_literal(LiteralValue::Boolean(lit.value));
                Some((id, Span::from_oxc(lit.span)))
            }
            Expression::NullLiteral(lit) => Some((well_known.null, Span::from_oxc(lit.span))),
            Expression::ObjectExpression(obj) => {
                let id = self.infer_object_literal(scope, obj);
                Some((id, Span::from_oxc(obj.span)))
            }
            Expression::StaticMemberExpression(member) => self.infer_member_access(scope, member),
            Expression::CallExpression(call) => self.infer_call(scope, call),
            // M11: `new ClassName(args)` — check the constructor signature and yield the
            // instance type.
            Expression::NewExpression(new_expr) => self.infer_new(scope, new_expr),
            // M11: `this` resolves to the current class member's instance type, set while
            // checking a class member body ([`check_class`]). Outside any class member
            // `current_this` is `None` → the error type (out of scope; no narrowing, no
            // crash, and the error type suppresses cascade).
            Expression::ThisExpression(this_expr) => {
                let span = Span::from_oxc(this_expr.span);
                Some((self.current_this.unwrap_or(well_known.error), span))
            }
            Expression::FunctionExpression(func) => {
                // Generic function expressions scope type params to the body, but
                // are not registered for explicit call-site type arguments.
                let id = self.infer_function(scope, func);
                Some((id, Span::from_oxc(func.span)))
            }
            Expression::ArrowFunctionExpression(arrow) => {
                let id = self.infer_arrow(scope, arrow);
                Some((id, Span::from_oxc(arrow.span)))
            }
            Expression::ParenthesizedExpression(paren) => self.infer_expr(scope, &paren.expression),
            // An assignment expression can appear anywhere an expression can — a ternary
            // arm, an array element, a condition, or another assignment's RHS. Reuse the
            // one assignment checker (so the nested target is checked); its value type is
            // the RHS (TS assignment-expression semantics).
            Expression::AssignmentExpression(assign) => self.check_assignment(scope, assign),
            Expression::TSAsExpression(assertion) => self.infer_assertion(
                scope,
                &assertion.expression,
                &assertion.type_annotation,
                Span::from_oxc(assertion.span),
                AssertionSyntax::As,
            ),
            Expression::TSTypeAssertion(assertion) => self.infer_assertion(
                scope,
                &assertion.expression,
                &assertion.type_annotation,
                Span::from_oxc(assertion.span),
                AssertionSyntax::Angle,
            ),
            Expression::Identifier(ident) => {
                let span = Span::from_oxc(ident.span);
                let name = ident.name.as_str();
                match self.resolve_value_binding_replay(scope, name) {
                    ValueResolution::TypeOnlyNamespace { .. } => {
                        self.emit_diagnostic(Diagnostic::cannot_use_namespace_as_value(
                            span,
                            ident.name.as_str(),
                        ));
                        Some((well_known.error, span))
                    }
                    ValueResolution::Resolved {
                        symbol: _,
                        kind: ResolvedValueKind::StandaloneNamespace { namespace, storage },
                    } => match self.standalone_namespace_terminal_replay(namespace) {
                        Some(super::namespace_values::StandaloneNamespaceTerminal::Ready {
                            storage: terminal_storage,
                            ty,
                        }) if terminal_storage == storage => Some((ty, span)),
                        Some(
                            super::namespace_values::StandaloneNamespaceTerminal::Unavailable {
                                ..
                            },
                        ) => Some((well_known.error, span)),
                        Some(
                            super::namespace_values::StandaloneNamespaceTerminal::Planned
                            | super::namespace_values::StandaloneNamespaceTerminal::Ready { .. },
                        )
                        | None => {
                            self.record_incomplete(
                                "expr-infer/identifier/standalone-namespace-unavailable",
                                span,
                                "standalone namespace value terminal is unavailable",
                            );
                            Some((well_known.error, span))
                        }
                    },
                    ValueResolution::Resolved {
                        symbol: symbol_id,
                        kind: ResolvedValueKind::Ordinary,
                    } => {
                        if name == "globalThis"
                            && self.binder.direct_global_this_value_conflict(symbol_id)
                        {
                            if let Some(global_object) = self.global_object_type {
                                return Some((global_object, span));
                            }
                        }
                        match self.demand_function_group_replay(symbol_id) {
                            FunctionGroupDemand::Ready(ty)
                            | FunctionGroupDemand::PrivateSelf(ty) => Some((ty, span)),
                            FunctionGroupDemand::Pending { report_use } => {
                                if report_use {
                                    self.record_incomplete(
                                        "expr-infer/identifier/function-group-pending",
                                        span,
                                        "merged function value waits for body return inference",
                                    );
                                }
                                None
                            }
                            FunctionGroupDemand::Unavailable => None,
                            FunctionGroupDemand::NotGroup => Some((
                                self.resolve_identifier_type(symbol_id, ident.span.start),
                                span,
                            )),
                        }
                    }
                    ValueResolution::Missing => {
                        // These built-ins are synthetic globals, after lexical lookup.
                        if name == "undefined" {
                            return Some((well_known.undefined, span));
                        }
                        if name == "globalThis" {
                            if let Some(global_object) = self.global_object_type {
                                return Some((global_object, span));
                            }
                        }
                        self.emit_diagnostic(Diagnostic::cannot_find_name(span, name));
                        Some((well_known.error, span))
                    }
                }
            }
            // M7: condition shapes (`typeof x`, `!x`). They are walked for their
            // operands' side effects (resolving references / descending into nested
            // constructs); their *value* type is only ever a condition, never an
            // assignment source in the subset, so a coarse result type is sufficient.
            Expression::UnaryExpression(unary) => Some(self.infer_unary(scope, unary)),
            Expression::BinaryExpression(binary) => Some(self.infer_binary(scope, binary)),
            // Backlog 101: `&&`/`||`/`??` and a ternary carry real value types, so this
            // position is context-free but no longer coarse.
            Expression::LogicalExpression(logical) => self.infer_logical(scope, logical, None),
            Expression::ConditionalExpression(cond) => self.infer_conditional(scope, cond, None),
            // A sequence `(a, b, …, z)` (backlog 53): walk every operand for its side
            // effects (references, nested checks), and take the value/type of the
            // **last** operand — the comma operator's result.
            Expression::SequenceExpression(seq) => {
                let mut result = None;
                for operand in &seq.expressions {
                    result = self.infer_expr(scope, operand);
                }
                result
            }
            // M17: an array literal `[e1, e2, …]` infers `(<elem>)[]` where the element
            // type is the union of the (widened) element types (`[1,2,3]` → `number[]`,
            // `[1,"x"]` → `(number | string)[]`); `[]` → `never[]`.
            Expression::ArrayExpression(array) => {
                let id = self.infer_array_literal(scope, array);
                Some((id, Span::from_oxc(array.span)))
            }
            // M17: element access `a[i]`. If `a` is an array, the result is its element
            // type (any index yields the element type — M17 does not strict-check the
            // index). A non-array base is out of M17 scope (no diagnostic, error type).
            Expression::ComputedMemberExpression(member) => {
                self.infer_element_access(scope, member)
            }
            // A template literal `` `x${e}y` `` is not typed (owner 71). Record the
            // skipped child slot before dropping so the interpolation's nested errors
            // are accounted for: `interpolation` when it has holes, else `self`.
            Expression::TemplateLiteral(tpl) => {
                let (id, context) = if tpl.expressions.is_empty() {
                    (
                        "expr-infer/template-literal/self",
                        "template literal not typed",
                    )
                } else {
                    (
                        "expr-infer/template-literal/interpolation",
                        "template interpolation not visited",
                    )
                };
                self.record_incomplete(id, Span::from_oxc(tpl.span), context);
                None
            }
            Expression::AwaitExpression(await_expr) => {
                self.infer_expr(scope, &await_expr.argument);
                self.record_incomplete(
                    "expr-infer/await-expression/self",
                    Span::from_oxc(await_expr.span),
                    "await result typing not modeled",
                );
                None
            }
            Expression::BigIntLiteral(lit) => {
                self.record_incomplete(
                    "expr-infer/bigint-literal/self",
                    Span::from_oxc(lit.span),
                    "bigint literal value has no type model",
                );
                None
            }
            // Class expressions need dedicated local binding and value/type duality semantics.
            Expression::ClassExpression(class) => {
                self.record_incomplete(
                    "expr-infer/class-expression/self",
                    Span::from_oxc(class.span),
                    "class expression not modeled",
                );
                None
            }
            Expression::ChainExpression(chain) => {
                self.infer_chain_element(scope, &chain.expression);
                self.record_incomplete(
                    "expr-infer/optional-chain/self",
                    Span::from_oxc(chain.span),
                    "optional chain not inferred",
                );
                None
            }
            Expression::ImportExpression(import) => {
                self.infer_expr(scope, &import.source);
                if let Some(options) = &import.options {
                    self.infer_expr(scope, options);
                }
                self.record_incomplete(
                    "expr-infer/import-expression/self",
                    Span::from_oxc(import.span),
                    "dynamic import not modeled",
                );
                None
            }
            Expression::TaggedTemplateExpression(tagged) => {
                self.infer_expr(scope, &tagged.tag);
                if let Some(type_arguments) = &tagged.type_arguments {
                    self.lower_type_arguments(scope, type_arguments);
                }
                for expression in &tagged.quasi.expressions {
                    self.infer_expr(scope, expression);
                }
                self.record_incomplete(
                    "expr-infer/tagged-template/self",
                    Span::from_oxc(tagged.span),
                    "tagged template not inferred",
                );
                None
            }
            Expression::UpdateExpression(update) => {
                self.infer_update_target(scope, &update.argument);
                Some((well_known.error, Span::from_oxc(update.span)))
            }
            Expression::YieldExpression(yield_expr) => {
                if let Some(argument) = &yield_expr.argument {
                    self.infer_expr(scope, argument);
                }
                self.record_incomplete(
                    "expr-infer/yield-expression/self",
                    Span::from_oxc(yield_expr.span),
                    "generator yield result not modeled",
                );
                None
            }
            Expression::PrivateInExpression(private_in) => {
                self.infer_expr(scope, &private_in.right);
                self.record_incomplete(
                    "expr-infer/private-in-expression/self",
                    Span::from_oxc(private_in.span),
                    "private-in expression not modeled",
                );
                None
            }
            Expression::TSSatisfiesExpression(satisfies) => {
                self.infer_expr(scope, &satisfies.expression);
                self.lower_annotation(scope, &satisfies.type_annotation);
                self.record_incomplete(
                    "expr-infer/satisfies-expression/self",
                    Span::from_oxc(satisfies.span),
                    "satisfies expression not modeled",
                );
                None
            }
            Expression::TSNonNullExpression(non_null) => {
                self.infer_expr(scope, &non_null.expression);
                self.record_incomplete(
                    "expr-infer/non-null-assertion/self",
                    Span::from_oxc(non_null.span),
                    "non-null assertion not modeled",
                );
                None
            }
            Expression::TSInstantiationExpression(instantiation) => {
                self.infer_expr(scope, &instantiation.expression);
                self.lower_type_arguments(scope, &instantiation.type_arguments);
                self.record_incomplete(
                    "expr-infer/instantiation-expression/self",
                    Span::from_oxc(instantiation.span),
                    "instantiation expression not modeled",
                );
                None
            }
            Expression::PrivateFieldExpression(private_field) => {
                self.record_incomplete(
                    "expr-infer/private-field-access/self",
                    Span::from_oxc(private_field.span),
                    "private field access not inferred",
                );
                None
            }
            Expression::RegExpLiteral(lit) => {
                let span = Span::from_oxc(lit.span);
                if let Some(ty) = self.regexp_literal_type(span) {
                    return Some((ty, span));
                }
                self.record_incomplete(
                    "expr-infer/regexp-literal/self",
                    span,
                    "RegExp literal value has no type model",
                );
                None
            }
            _ => None,
        }
    }

    /// Walk a complete optional chain structurally; its enclosing record owns the semantics.
    fn infer_chain_element(&mut self, scope: ScopeId, element: &ChainElement<'_>) {
        match element {
            ChainElement::CallExpression(call) => {
                self.infer_chain_call_children(scope, call);
            }
            ChainElement::StaticMemberExpression(member) => {
                self.infer_chain_expression(scope, &member.object);
            }
            ChainElement::ComputedMemberExpression(member) => {
                self.infer_chain_expression(scope, &member.object);
                self.infer_expr(scope, &member.expression);
            }
            ChainElement::PrivateFieldExpression(_) => {}
            ChainElement::TSNonNullExpression(non_null) => {
                self.infer_chain_expression(scope, &non_null.expression);
            }
        }
    }

    /// Visit only an update expression's target; operator compatibility stays deferred.
    fn infer_update_target(&mut self, scope: ScopeId, target: &SimpleAssignmentTarget<'_>) {
        if super::assignment::update_target_contains_nested_scope(target) {
            self.record_incomplete(
                "expr-infer/update-expression/nested-scope-target",
                Span::from_oxc(target.span()),
                "update target has an unreserved nested scope",
            );
            self.walk_update_target_assertions_syntax_only(scope, target);
            return;
        }
        match target {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(ident) => {
                if ident.name.as_str() == "undefined" {
                    return;
                }
                match self.resolve_value_binding_replay(scope, ident.name.as_str()) {
                    ValueResolution::Resolved {
                        kind: ResolvedValueKind::StandaloneNamespace { .. },
                        ..
                    } => {
                        self.emit_diagnostic(Diagnostic::cannot_assign_namespace(
                            Span::from_oxc(ident.span),
                            ident.name.as_str(),
                        ));
                    }
                    ValueResolution::Resolved { .. } => {}
                    ValueResolution::TypeOnlyNamespace { .. } | ValueResolution::Missing => {
                        self.emit_diagnostic(Diagnostic::cannot_find_name(
                            Span::from_oxc(ident.span),
                            ident.name.as_str(),
                        ));
                    }
                }
            }
            SimpleAssignmentTarget::StaticMemberExpression(member) => {
                self.infer_member_access(scope, member);
            }
            SimpleAssignmentTarget::ComputedMemberExpression(member) => {
                self.infer_element_access(scope, member);
            }
            SimpleAssignmentTarget::PrivateFieldExpression(member) => {
                self.record_incomplete(
                    "expr-infer/private-field-access/self",
                    Span::from_oxc(member.span),
                    "private field access not inferred",
                );
            }
            SimpleAssignmentTarget::TSAsExpression(assertion) => {
                self.infer_assertion(
                    scope,
                    &assertion.expression,
                    &assertion.type_annotation,
                    Span::from_oxc(assertion.span),
                    AssertionSyntax::As,
                );
            }
            SimpleAssignmentTarget::TSSatisfiesExpression(satisfies) => {
                self.infer_expr(scope, &satisfies.expression);
                self.lower_annotation(scope, &satisfies.type_annotation);
                self.record_incomplete(
                    "expr-infer/satisfies-expression/self",
                    Span::from_oxc(satisfies.span),
                    "satisfies expression not modeled",
                );
            }
            SimpleAssignmentTarget::TSNonNullExpression(non_null) => {
                self.infer_expr(scope, &non_null.expression);
                self.record_incomplete(
                    "expr-infer/non-null-assertion/self",
                    Span::from_oxc(non_null.span),
                    "non-null assertion not modeled",
                );
            }
            SimpleAssignmentTarget::TSTypeAssertion(assertion) => {
                self.infer_assertion(
                    scope,
                    &assertion.expression,
                    &assertion.type_annotation,
                    Span::from_oxc(assertion.span),
                    AssertionSyntax::Angle,
                );
            }
        }
    }

    /// Walk a chain expression without resolving calls or member access on its spine.
    fn infer_chain_expression(&mut self, scope: ScopeId, expression: &Expression<'_>) {
        match expression {
            Expression::CallExpression(call) => self.infer_chain_call_children(scope, call),
            Expression::StaticMemberExpression(member) => {
                self.infer_chain_expression(scope, &member.object);
            }
            Expression::ComputedMemberExpression(member) => {
                self.infer_chain_expression(scope, &member.object);
                self.infer_expr(scope, &member.expression);
            }
            Expression::PrivateFieldExpression(_) => {}
            Expression::ParenthesizedExpression(paren) => {
                self.infer_chain_expression(scope, &paren.expression);
            }
            Expression::TSAsExpression(assertion) => {
                self.infer_assertion(
                    scope,
                    &assertion.expression,
                    &assertion.type_annotation,
                    Span::from_oxc(assertion.span),
                    AssertionSyntax::As,
                );
            }
            Expression::TSTypeAssertion(assertion) => {
                self.infer_assertion(
                    scope,
                    &assertion.expression,
                    &assertion.type_annotation,
                    Span::from_oxc(assertion.span),
                    AssertionSyntax::Angle,
                );
            }
            Expression::TSSatisfiesExpression(satisfies) => {
                self.infer_chain_expression(scope, &satisfies.expression);
                self.lower_annotation(scope, &satisfies.type_annotation);
            }
            Expression::TSNonNullExpression(non_null) => {
                self.infer_chain_expression(scope, &non_null.expression);
            }
            Expression::TSInstantiationExpression(instantiation) => {
                self.infer_chain_expression(scope, &instantiation.expression);
                self.lower_type_arguments(scope, &instantiation.type_arguments);
            }
            _ => {
                self.infer_expr(scope, expression);
            }
        }
    }

    /// Walk a chain call's children without applying ordinary callability rules.
    fn infer_chain_call_children(
        &mut self,
        scope: ScopeId,
        call: &oxc_ast::ast::CallExpression<'_>,
    ) {
        self.infer_chain_expression(scope, &call.callee);
        if let Some(type_arguments) = &call.type_arguments {
            self.lower_type_arguments(scope, type_arguments);
        }
        for argument in &call.arguments {
            if let Some(expression) = argument.as_expression() {
                self.infer_expr(scope, expression);
            } else {
                self.record_incomplete(
                    "call/call-arguments/spread-argument",
                    Span::from_oxc(argument.span()),
                    "spread call argument not visited",
                );
            }
        }
    }

    /// Lower explicit type arguments solely to retain annotation diagnostics.
    fn lower_type_arguments(
        &mut self,
        scope: ScopeId,
        type_arguments: &oxc_ast::ast::TSTypeParameterInstantiation<'_>,
    ) {
        for argument in &type_arguments.params {
            self.lower_annotation(scope, argument);
        }
    }

    /// The expression of an array element, or `None` after recording the incomplete
    /// surface for a spread/elision child slot (owner 71). Every array-literal
    /// walker routes through this so no in-scope child is silently dropped.
    fn array_element_expr<'e, 'x>(
        &mut self,
        element: &'e ArrayExpressionElement<'x>,
    ) -> Option<&'e Expression<'x>> {
        match element {
            ArrayExpressionElement::SpreadElement(spread) => {
                self.record_incomplete(
                    "expr-infer/array-literal/spread-element",
                    Span::from_oxc(spread.span),
                    "array spread element not visited",
                );
                None
            }
            ArrayExpressionElement::Elision(elision) => {
                self.record_incomplete(
                    "expr-infer/array-literal/elision",
                    Span::from_oxc(elision.span),
                    "array hole (elision) not visited",
                );
                None
            }
            _ => element.as_expression(),
        }
    }

    /// Record the incomplete surface for a skipped object-literal spread member
    /// (`{ ...x }`, owner 71), keeping every object walker's `continue` accounted for.
    fn record_object_spread_skip(&mut self, member: &ObjectPropertyKind<'_>) {
        self.record_incomplete(
            "expr-infer/object-literal/spread-element",
            Span::from_oxc(member.span()),
            "object spread element not visited",
        );
    }

    /// Record the incomplete surface for a skipped object-literal computed key
    /// (`{ [e]: v }`, owner 75) at the key's span.
    fn record_object_computed_key_skip(&mut self, key: &oxc_ast::ast::PropertyKey<'_>) {
        self.record_incomplete(
            "expr-infer/object-literal/computed-key",
            Span::from_oxc(key.span()),
            "computed object key not visited",
        );
    }

    /// Infer a TypeScript assertion (`expr as T` / `<T>expr`). The source expression is
    /// still walked for nested diagnostics; the value type is the asserted type.
    pub(in crate::check::checker) fn infer_assertion(
        &mut self,
        scope: ScopeId,
        expression: &Expression<'_>,
        type_annotation: &TSType<'_>,
        span: Span,
        syntax: AssertionSyntax,
    ) -> Option<(TypeId, Span)> {
        let source = self.infer_expr(scope, expression);
        if is_const_assertion_type(type_annotation) {
            return source;
        }
        let asserted = self
            .lower_annotation(scope, type_annotation)
            .unwrap_or_else(|| self.interner.well_known().error);
        if let Some((source, _)) = source {
            let error = self.interner.well_known().error;
            if source != error && asserted != error {
                self.schedule_assertion_compatibility(AssertionCompatibilityObligation {
                    source,
                    asserted,
                    span,
                    syntax,
                });
            }
        }
        Some((asserted, span))
    }

    /// Resolve a value symbol's narrowed type at a reference. Missing pre-pass
    /// coverage defaults to START (the declared type), and facts key by `SymbolId`
    /// so narrowing applies only to the guarded binding.
    pub(in crate::check::checker) fn resolve_identifier_type(
        &mut self,
        symbol_id: SymbolId,
        ref_start: u32,
    ) -> TypeId {
        let flow = self
            .reference_flow
            .get(&(self.current_module, ref_start))
            .copied()
            .unwrap_or(FlowNodeId::START);
        self.resolve_narrowed_type(flow, symbol_id)
    }

    /// Infer a unary expression for M7 condition support: walk the operand, then
    /// return `string` for `typeof`, `boolean` for `!`, or the error type for
    /// out-of-subset unary operators.
    fn infer_unary(&mut self, scope: ScopeId, unary: &UnaryExpression<'_>) -> (TypeId, Span) {
        let wk = self.interner.well_known();
        let span = Span::from_oxc(unary.span);
        // `-<numeric literal>` is a fresh negative number literal (mirrors the plain
        // numeric-literal case; tsc collapses `-0` to `0`).
        if unary.operator == UnaryOperator::UnaryNegation {
            if let Expression::NumericLiteral(lit) = &unary.argument {
                let negated = -lit.value;
                let value = if negated == 0.0 { 0.0 } else { negated };
                let id = self.interner.intern_literal(LiteralValue::Number(value));
                return (id, span);
            }
        }
        // Walk the operand so references inside the condition resolve (and nested
        // functions are checked).
        self.infer_expr(scope, &unary.argument);
        let ty = match unary.operator {
            UnaryOperator::Typeof => wk.string,
            UnaryOperator::LogicalNot => wk.boolean,
            _ => wk.error,
        };
        (ty, span)
    }

    /// Infer a binary expression. Comparisons return `boolean`; numeric/string `+`
    /// retains its primitive result so contextual callback inference can consume it;
    /// every arithmetic, bitwise, and shift operator returns `number` and checks its
    /// operands (backlog `45`). `in`/`instanceof` remain out of subset.
    fn infer_binary(&mut self, scope: ScopeId, binary: &BinaryExpression<'_>) -> (TypeId, Span) {
        let wk = self.interner.well_known();
        let span = Span::from_oxc(binary.span);
        let left = self
            .infer_expr(scope, &binary.left)
            .map(|(ty, _)| ty)
            .unwrap_or(wk.error);
        let right = self
            .infer_expr(scope, &binary.right)
            .map(|(ty, _)| ty)
            .unwrap_or(wk.error);
        let ty = if is_comparison_operator(binary.operator) {
            wk.boolean
        } else if binary.operator == BinaryOperator::Addition {
            self.addition_result(binary, left, right)
        } else if is_arithmetic_operator(binary.operator) {
            self.arithmetic_result(binary, left, right)
        } else {
            wk.error
        };
        (ty, span)
    }

    /// Type an arithmetic / bitwise / shift operator. Both operands must be numeric
    /// (`TK2362` / `TK2363` per side, one diagnostic each — mirroring tsc, which does
    /// NOT collapse a two-sided violation into one `TK2365`). The result is `number`
    /// regardless, so a downstream assignment still reports its own mismatch.
    fn arithmetic_result(
        &mut self,
        binary: &BinaryExpression<'_>,
        left: TypeId,
        right: TypeId,
    ) -> TypeId {
        let wk = self.interner.well_known();
        // The error type suppresses cascade diagnostics on the same expression (the
        // corpus-wide rule) and is also what an unmodelled `bigint` operand produces,
        // where `number` would be the wrong answer.
        if left == wk.error || right == wk.error {
            return wk.error;
        }
        if self.numeric_operand_verdict(left) == OperandVerdict::Rejected {
            let span = Span::from_oxc(binary.left.span());
            self.emit_diagnostic(Diagnostic::arithmetic_left_operand(span));
        }
        if self.numeric_operand_verdict(right) == OperandVerdict::Rejected {
            let span = Span::from_oxc(binary.right.span());
            self.emit_diagnostic(Diagnostic::arithmetic_right_operand(span));
        }
        wk.number
    }

    /// Type a `+`. The primitive string/number pairings keep their existing fast
    /// classification; anything they do not cover resolves through the relation
    /// engine in tsc's order (both number-like → `number`, either string-like →
    /// `string`), and a pair matching neither reports `TK2365`.
    fn addition_result(
        &mut self,
        binary: &BinaryExpression<'_>,
        left: TypeId,
        right: TypeId,
    ) -> TypeId {
        let wk = self.interner.well_known();
        match (
            addition_operand_kind(self.interner.store(), left),
            addition_operand_kind(self.interner.store(), right),
        ) {
            (AdditionOperandKind::Error, _) | (_, AdditionOperandKind::Error) => wk.error,
            (AdditionOperandKind::Any, _) | (_, AdditionOperandKind::Any) => wk.any,
            (
                AdditionOperandKind::String,
                AdditionOperandKind::String | AdditionOperandKind::Number,
            )
            | (AdditionOperandKind::Number, AdditionOperandKind::String) => wk.string,
            (AdditionOperandKind::Number, AdditionOperandKind::Number) => wk.number,
            _ => self.addition_general_result(binary, left, right),
        }
    }

    /// The `+` rules the primitive fast path does not cover. tsc tests number-like
    /// first (so `never + 1` is `number`, not `string`), then string-like on either
    /// side. An operand the model cannot decide keeps the previous conservative error
    /// result and reports nothing.
    fn addition_general_result(
        &mut self,
        binary: &BinaryExpression<'_>,
        left: TypeId,
        right: TypeId,
    ) -> TypeId {
        let wk = self.interner.well_known();
        let left_numeric = self.numeric_operand_verdict(left);
        let right_numeric = self.numeric_operand_verdict(right);
        if left_numeric == OperandVerdict::Satisfied && right_numeric == OperandVerdict::Satisfied {
            return wk.number;
        }
        let left_string = self.stringlike_operand_verdict(left);
        let right_string = self.stringlike_operand_verdict(right);
        if left_string == OperandVerdict::Satisfied || right_string == OperandVerdict::Satisfied {
            return wk.string;
        }
        let undecided = [left_numeric, right_numeric, left_string, right_string]
            .contains(&OperandVerdict::Deferred);
        if undecided {
            return wk.error;
        }
        let store = self.interner.store();
        let left_display = render_type(store, left, /* widen */ false);
        let right_display = render_type(store, right, /* widen */ false);
        let span = Span::from_oxc(binary.span);
        self.emit_diagnostic(Diagnostic::operator_not_applicable(
            span,
            binary.operator.as_str(),
            &left_display,
            &right_display,
        ));
        wk.any
    }

    /// Whether an operand satisfies the arithmetic operand rule. `null`/`undefined`
    /// members are stripped first — the shape of tsc's `checkNonNullType`, whose own
    /// diagnostic belongs to the (unimplemented) strict-null receiver family — and an
    /// `unknown` operand is likewise left to the unknown-receiver family.
    fn numeric_operand_verdict(&mut self, ty: TypeId) -> OperandVerdict {
        let wk = self.interner.well_known();
        if ty == wk.unknown {
            return OperandVerdict::Deferred;
        }
        let stripped = self.strip_nullish(ty);
        self.operand_verdict_against(stripped, wk.number)
    }

    /// Whether an operand is string-like for `+`. No nullish stripping: tsc tests the
    /// original operand for string-likeness before it ever reaches `checkNonNullType`.
    fn stringlike_operand_verdict(&mut self, ty: TypeId) -> OperandVerdict {
        let wk = self.interner.well_known();
        if ty == wk.unknown {
            return OperandVerdict::Deferred;
        }
        self.operand_verdict_against(ty, wk.string)
    }

    /// Decide one operand against one primitive target. A type-level node the relation
    /// engine can only relate identically (a deferred conditional / instantiation /
    /// mapped / `keyof` / indexed access) and an exhausted query are `Deferred`: they
    /// report nothing, because a `No` there means "not decided", not "not numeric".
    fn operand_verdict_against(&mut self, ty: TypeId, target: TypeId) -> OperandVerdict {
        let wk = self.interner.well_known();
        if ty == target || ty == wk.any || ty == wk.never {
            return OperandVerdict::Satisfied;
        }
        let apparent = self.apparent_type(ty);
        if is_undecided_operand_shape(self.interner.store(), apparent, UNDECIDED_SHAPE_DEPTH) {
            return OperandVerdict::Deferred;
        }
        match self.with_semantic_query(|query| query.is_assignable(ty, target)) {
            RelationOutcome::Yes => OperandVerdict::Satisfied,
            RelationOutcome::No(_) => OperandVerdict::Rejected,
            RelationOutcome::Exhausted(_) => OperandVerdict::Deferred,
        }
    }

    /// Drop `null`/`undefined` from a union operand (`null`/`undefined` alone become
    /// `never`, which every primitive accepts — exactly tsc's `getNonNullableType`).
    fn strip_nullish(&mut self, ty: TypeId) -> TypeId {
        let wk = self.interner.well_known();
        if ty == wk.null || ty == wk.undefined {
            return wk.never;
        }
        let kept = {
            let store = self.interner.store();
            let is_nullish = |member: &TypeId| *member == wk.null || *member == wk.undefined;
            match store.union_members(ty) {
                Some(members) if members.iter().any(is_nullish) => Some(
                    members
                        .iter()
                        .copied()
                        .filter(|member| !is_nullish(member))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            }
        };
        match kept {
            Some(members) => self.interner.union(members),
            None => ty,
        }
    }

    /// Infer a ternary `test ? a : b` (backlog `101`): the union of the two arm types.
    /// Each arm is walked at the flow edge `build_flow_conditional` built for it, so a
    /// guarded arm reads its branch's narrow; `context` (the assignment target) reaches
    /// both arms through the ordinary contextual-initializer path.
    fn infer_conditional(
        &mut self,
        scope: ScopeId,
        cond: &ConditionalExpression<'_>,
        context: Option<TypeId>,
    ) -> Option<(TypeId, Span)> {
        self.infer_expr(scope, &cond.test);
        let consequent = self.infer_initializer(scope, &cond.consequent, context);
        let alternate = self.infer_initializer(scope, &cond.alternate, context);
        // An arm outside the expression subset contributes no member; a smaller union
        // only ever under-reports, never invents a mismatch.
        let ty = match (consequent, alternate) {
            (Some((left, _)), Some((right, _))) => {
                if self.conditional_undefined_arity_subtype(left, right) {
                    right
                } else if self.conditional_undefined_arity_subtype(right, left) {
                    left
                } else {
                    self.interner.union(vec![left, right])
                }
            }
            (Some((ty, _)), None) | (None, Some((ty, _))) => ty,
            (None, None) => self.interner.well_known().never,
        };
        Some((ty, Span::from_oxc(cond.span)))
    }

    /// The bounded part of tsc's subtype-reducing conditional union needed for
    /// required `T | undefined` versus optional callable parameters. Explicit unions
    /// keep their own union-call arity; only conditional arm selection uses this probe.
    fn conditional_undefined_arity_subtype(&self, source: TypeId, target: TypeId) -> bool {
        conditional_undefined_arity_subtype(self.interner.store(), source, target)
    }

    /// Infer a logical expression (`&&`/`||`/`??`, backlog `101`): the surviving part of
    /// the left operand joined with the right. The split reuses the narrowing engine —
    /// `NarrowOp::Truthy`'s [`narrow_by_truthiness`] for `&&`/`||` and [`Self::strip_nullish`]
    /// (tsc's `getNonNullableType`) for `??` — so there is one truthiness model, not two.
    /// A left operand that can never take the other branch short-circuits to itself,
    /// mirroring `checkBinaryLikeExpression`.
    fn infer_logical(
        &mut self,
        scope: ScopeId,
        logical: &LogicalExpression<'_>,
        context: Option<TypeId>,
    ) -> Option<(TypeId, Span)> {
        let span = Span::from_oxc(logical.span);
        // tsc's `getContextualTypeForBinaryOperand`: `||`/`??` shape both operands,
        // `&&` only its right one (its left is a condition).
        let left_context = match logical.operator {
            LogicalOperator::And => None,
            LogicalOperator::Or | LogicalOperator::Coalesce => context,
        };
        let left = self
            .infer_initializer(scope, &logical.left, left_context)
            .map(|(ty, _)| ty);
        let right = self
            .infer_initializer(scope, &logical.right, context)
            .map(|(ty, _)| ty);
        let Some(left) = left else {
            let members = right.into_iter().collect();
            return Some((self.interner.union(members), span));
        };
        let never = self.interner.well_known().never;
        let (kept, taken) = match logical.operator {
            LogicalOperator::And => (
                narrow_by_truthiness(self.interner, left, false),
                narrow_by_truthiness(self.interner, left, true),
            ),
            LogicalOperator::Or => (
                narrow_by_truthiness(self.interner, left, true),
                narrow_by_truthiness(self.interner, left, false),
            ),
            LogicalOperator::Coalesce => {
                let non_nullish = self.strip_nullish(left);
                // `??` takes its right operand exactly when the left had a nullish member.
                let taken = if non_nullish == left { never } else { left };
                (non_nullish, taken)
            }
        };
        // The right operand is unreachable, so the value is the whole left operand.
        if taken == never {
            return Some((left, span));
        }
        let members = std::iter::once(kept).chain(right).collect();
        Some((self.interner.union(members), span))
    }

    /// Infer the type of an object literal in `scope`. Unchanged from M2: member
    /// types are widened (`{ a: 1 }` → `{ a: number }`).
    fn infer_object_literal(&mut self, scope: ScopeId, obj: &ObjectExpression<'_>) -> TypeId {
        let mut properties: Vec<PropertyType> = Vec::with_capacity(obj.properties.len());
        for member in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(prop) = member else {
                self.record_object_spread_skip(member);
                continue;
            };
            let Some(name) = prop.key.static_name() else {
                self.record_object_computed_key_skip(&prop.key);
                continue;
            };
            let Some((value_ty, _)) = self.infer_expr(scope, &prop.value) else {
                continue;
            };
            let widened = widen(self.interner, value_ty);
            properties.push(PropertyType::public(name.into_owned(), widened));
        }
        // M19: an object literal never declares an index signature (it is a set of
        // named members); the index slots stay `None`.
        self.interner.intern_object(ObjectType {
            properties,
            ..Default::default()
        })
    }

    /// Infer an M17 array literal as an array of the union of widened element types.
    /// Empty/all-skipped literals become `never[]`; spread/elision elements are out
    /// of subset and contribute no element type.
    fn infer_array_literal(&mut self, scope: ScopeId, array: &ArrayExpression<'_>) -> TypeId {
        let mut element_types: Vec<TypeId> = Vec::with_capacity(array.elements.len());
        for element in &array.elements {
            // Spread (`...xs`) / elision (a hole) are out of subset — the helper records
            // the skipped child slot. Only a plain expression contributes an element type.
            let Some(expr) = self.array_element_expr(element) else {
                continue;
            };
            let Some((elem_ty, _)) = self.infer_expr(scope, expr) else {
                continue;
            };
            element_types.push(widen(self.interner, elem_ty));
        }
        // Empty (or all-skipped) → empty union → `never`, giving `never[]`.
        let element = self.interner.union(element_types);
        self.interner.intern_array(element)
    }

    /// Infer an initializer/source expression with a known target type. Only fresh
    /// object/array literals are contextually shaped; every other expression remains
    /// context-free through [`infer_expr`].
    pub(in crate::check::checker) fn infer_initializer(
        &mut self,
        scope: ScopeId,
        init: &Expression<'_>,
        context: Option<TypeId>,
    ) -> Option<(TypeId, Span)> {
        // A ternary / logical is not itself a fresh literal — the value lives in its
        // operands, so the raw target is pushed down to each of them before any
        // literal-shaping transform runs on it (backlog `101`).
        match init {
            Expression::ConditionalExpression(cond) => {
                return self.infer_conditional(scope, cond, context);
            }
            Expression::LogicalExpression(logical) => {
                return self.infer_logical(scope, logical, context);
            }
            _ => {}
        }
        let (context, contextual_this) = match context {
            Some(context) => match self.with_semantic_query_transaction(|pass| {
                let context = match pass.demand_composite_apparent_type(context) {
                    DemandOutcome::Ready(context) => {
                        contextual_literal_target(pass.interner.store(), context)
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                };
                // `ThisType` markers live on the original intersection. Read them
                // before merging the visible contextual object surface.
                let contextual_this = pass.contextual_this_type(context);
                let context = match pass.intersection_apparent_object(context) {
                    DemandOutcome::Ready(Some(apparent)) => apparent,
                    DemandOutcome::Ready(None) => context,
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                };
                DemandOutcome::Ready((context, contextual_this))
            }) {
                DemandOutcome::Ready((context, contextual_this)) => {
                    (Some(context), contextual_this)
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(
                        DemandOutcome::Exhausted(exhaustion),
                        Span::from_oxc(init.span()),
                    );
                    return None;
                }
            },
            None => (None, None),
        };
        match (init, context) {
            (Expression::ParenthesizedExpression(paren), ctx) => {
                self.infer_initializer(scope, &paren.expression, ctx)
            }
            (Expression::ObjectExpression(obj), Some(ctx))
                if self.interner.store().object_type(ctx).is_some() =>
            {
                let id = self.infer_object_literal_with_context(scope, obj, ctx, contextual_this);
                Some((id, Span::from_oxc(obj.span)))
            }
            (Expression::ArrayExpression(array), Some(ctx))
                if self.interner.store().tag(ctx) == TypeTag::Tuple =>
            {
                let id = self.infer_array_literal_as_tuple(scope, array, ctx);
                Some((id, Span::from_oxc(array.span)))
            }
            (Expression::ArrayExpression(array), Some(ctx)) => {
                if let Some(element_context) =
                    self.interner.store().array_type(ctx).map(|a| a.element)
                {
                    let id = self.infer_array_literal_as_array(scope, array, element_context);
                    Some((id, Span::from_oxc(array.span)))
                } else {
                    self.infer_expr(scope, init)
                }
            }
            _ => self.infer_expr(scope, init),
        }
    }

    /// Re-infer an already-walked fresh literal against a known target without
    /// duplicating diagnostics/obligations from nested expressions.
    pub(in crate::check::checker) fn infer_contextual_source_after_walked(
        &mut self,
        scope: ScopeId,
        expr: &Expression<'_>,
        context: TypeId,
        raw: (TypeId, Span),
        use_contextual_arrow: bool,
        retain_contextual_arrow_checks: bool,
    ) -> (TypeId, Span) {
        self.infer_contextual_source_after_walked_reporting(
            scope,
            expr,
            context,
            raw,
            use_contextual_arrow,
            retain_contextual_arrow_checks,
        )
        .0
    }

    /// [`Self::infer_contextual_source_after_walked`], also reporting whether the
    /// expression was actually re-walked.
    ///
    /// Only a [`ContextualRewalk::Rewalked`] answer means this walk saw the
    /// expression, so only then may a caller treat the earlier raw walk as
    /// superseded. The re-walk is declined for a target that cannot shape the
    /// literal, and — for arrows — for a generic arrow or a context that is not one
    /// call signature, both of which return before the body is entered.
    pub(in crate::check::checker) fn infer_contextual_source_after_walked_reporting(
        &mut self,
        scope: ScopeId,
        expr: &Expression<'_>,
        context: TypeId,
        raw: (TypeId, Span),
        use_contextual_arrow: bool,
        retain_contextual_arrow_checks: bool,
    ) -> ((TypeId, Span), ContextualRewalk) {
        let context = match self.demand_composite_apparent_type(context) {
            DemandOutcome::Ready(context) => context,
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), raw.1);
                return (raw, ContextualRewalk::KeptRaw);
            }
        };
        // Backlog 95: only the effect-discarding mode is memoized. There the walk's
        // whole `CheckerEffects` is dropped, so the answer returned here plus the
        // declaration bindings it leaves behind are the whole of its observable
        // output, and replaying those is indistinguishable from re-walking. The
        // retaining mode is the one that reports (backlog 92) and must always run.
        let memo_key = if retain_contextual_arrow_checks {
            None
        } else {
            self.contextual_walk_key(scope, raw, context, use_contextual_arrow)
        };
        if let Some(key) = &memo_key {
            if let Some(memoized) = self.memoized_contextual_walk(key) {
                return memoized;
            }
        }
        let mark = DeclTypes::log_mark();
        if let (true, Expression::ArrowFunctionExpression(arrow)) = (use_contextual_arrow, expr) {
            #[cfg(test)]
            super::calls::measure_contextual_rewalk(super::calls::contextual_measure_phase(), true);
            let decl_types = (!retain_contextual_arrow_checks).then(|| self.decl_types.clone());
            let contextual = if retain_contextual_arrow_checks {
                self.infer_contextual_arrow_with_return_context(scope, arrow, context, false)
            } else {
                let (contextual, effects) = self.capture_candidate_effects(|pass| {
                    pass.infer_contextual_arrow_with_return_context(scope, arrow, context, false)
                });
                #[cfg(test)]
                let discarded_records = effects.records.len();
                effects.records.discard();
                #[cfg(test)]
                super::calls::measure_contextual_rollback(discarded_records);
                if let Some(decl_types) = decl_types {
                    self.decl_types.restore(decl_types, mark);
                }
                contextual
            };
            let resolved = match contextual {
                Some(ty) => ((ty, raw.1), ContextualRewalk::Rewalked),
                None => (raw, ContextualRewalk::KeptRaw),
            };
            if let Some(key) = memo_key {
                self.memoize_contextual_walk(key, mark, resolved);
            }
            return resolved;
        }

        if !self.context_can_shape_fresh_literal(expr, context) {
            return (raw, ContextualRewalk::KeptRaw);
        }

        #[cfg(test)]
        super::calls::measure_contextual_rewalk(super::calls::contextual_measure_phase(), false);
        let contextual = if retain_contextual_arrow_checks {
            self.infer_initializer(scope, expr, Some(context))
        } else {
            let (contextual, effects) = self.capture_candidate_effects(|pass| {
                pass.infer_initializer(scope, expr, Some(context))
            });
            #[cfg(test)]
            let discarded_records = effects.records.len();
            effects.records.discard();
            #[cfg(test)]
            super::calls::measure_contextual_rollback(discarded_records);
            contextual
        };
        let resolved = match contextual {
            Some(source) => (source, ContextualRewalk::Rewalked),
            None => (raw, ContextualRewalk::KeptRaw),
        };
        if let Some(key) = memo_key {
            self.memoize_contextual_walk(key, mark, resolved);
        }
        resolved
    }

    fn context_can_shape_fresh_literal(&self, expr: &Expression<'_>, context: TypeId) -> bool {
        let context = contextual_literal_target(self.interner.store(), context);
        match expr {
            Expression::ParenthesizedExpression(paren) => {
                self.context_can_shape_fresh_literal(&paren.expression, context)
            }
            // M31: an intersection whose members include an object type can shape a fresh
            // object literal (via its merged apparent object — resolved in `infer_initializer`).
            Expression::ObjectExpression(_) => {
                let store = self.interner.store();
                store.object_type(context).is_some()
                    || store
                        .intersection_members(context)
                        .is_some_and(|ms| ms.iter().any(|&m| store.object_type(m).is_some()))
            }
            Expression::ArrayExpression(_) => {
                self.interner.store().tag(context) == TypeTag::Tuple
                    || self.interner.store().array_type(context).is_some()
            }
            // Backlog 101: a ternary / logical passes the target on to the operands that
            // carry the value, so it is re-walkable exactly when one of them is.
            _ => contextual_value_operands(expr).is_some_and(|operands| {
                operands
                    .into_iter()
                    .flatten()
                    .any(|operand| self.context_can_shape_fresh_literal(operand, context))
            }),
        }
    }

    /// Infer an object literal against a concrete object target. Matched members use
    /// the target member/index value as recursive context; unmatched members keep the
    /// ordinary no-context widening behavior.
    fn infer_object_literal_with_context(
        &mut self,
        scope: ScopeId,
        obj: &ObjectExpression<'_>,
        context: TypeId,
        contextual_this: Option<TypeId>,
    ) -> TypeId {
        let saved_this = self.current_this;
        if let Some(contextual_this) = contextual_this {
            self.current_this = Some(contextual_this);
        }
        let mut properties: Vec<PropertyType> = Vec::with_capacity(obj.properties.len());
        for member in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(prop) = member else {
                self.record_object_spread_skip(member);
                continue;
            };
            let Some(name) = prop.key.static_name() else {
                self.record_object_computed_key_skip(&prop.key);
                continue;
            };
            let value_context = self.object_literal_property_context(context, &name);
            let value = match value_context {
                Some(ctx) => self.infer_initializer(scope, &prop.value, Some(ctx)),
                None => self.infer_expr(scope, &prop.value),
            };
            let Some((value_ty, _)) = value else {
                continue;
            };
            let ty = if value_context.is_some() {
                value_ty
            } else {
                widen(self.interner, value_ty)
            };
            properties.push(PropertyType::public(name.into_owned(), ty));
        }

        self.current_this = saved_this;
        self.interner.intern_object(ObjectType {
            properties,
            ..Default::default()
        })
    }

    /// Find the one retained `ThisType<T>` marker in a contextual intersection.
    /// Source lowering normalizes marker precedence before canonical intersection
    /// interning, so the member order here is immaterial.
    fn contextual_this_type(&self, context: TypeId) -> Option<TypeId> {
        let store = self.interner.store();
        let members = store.intersection_members(context)?;
        members
            .iter()
            .find_map(|&member| self.interner.well_known().this_type_operand(store, member))
    }

    fn object_literal_property_context(&self, context: TypeId, name: &str) -> Option<TypeId> {
        let store = self.interner.store();
        let context = contextual_literal_target(store, context);
        let target = self.interner.store().object_type(context)?;
        if let Some(prop) = target.property(name) {
            return Some(contextual_literal_target(store, prop.ty));
        }
        let index_value = if is_numeric_property_name(name) {
            target.number_index.or(target.string_index)
        } else {
            target.string_index
        };
        index_value.map(|ty| contextual_literal_target(store, ty))
    }

    /// Infer an array literal against a concrete array target. Element values are kept
    /// narrow for the final obligation instead of being widened before the source array
    /// type is formed.
    fn infer_array_literal_as_array(
        &mut self,
        scope: ScopeId,
        array: &ArrayExpression<'_>,
        element_context: TypeId,
    ) -> TypeId {
        let mut element_types: Vec<TypeId> = Vec::with_capacity(array.elements.len());
        for element in &array.elements {
            let Some(expr) = self.array_element_expr(element) else {
                continue;
            };
            let Some((elem_ty, _)) = self.infer_initializer(scope, expr, Some(element_context))
            else {
                continue;
            };
            element_types.push(elem_ty);
        }
        let element = self.interner.union(element_types);
        self.interner.intern_array(element)
    }

    /// Type an array literal positionally as an M18 tuple context. Element contexts
    /// recurse, surplus elements infer normally for the later length error, and
    /// element types stay un-widened so literal tuple targets remain precise.
    fn infer_array_literal_as_tuple(
        &mut self,
        scope: ScopeId,
        array: &ArrayExpression<'_>,
        context: TypeId,
    ) -> TypeId {
        // Snapshot the target tuple up front (immutable borrow) so the recursive
        // contextual inference below can take `&mut self`. The shared tuple call-shape
        // helper accounts for a represented rest segment and a trailing fixed suffix.
        let context_tuple = self.interner.store().tuple_type(context).cloned();

        let mut elements: Vec<TypeId> = Vec::with_capacity(array.elements.len());
        for (index, element) in array.elements.iter().enumerate() {
            // Spread (`...xs`) / elision (a hole) are out of subset — the helper records
            // the skipped child slot; only a plain expression contributes a position.
            let Some(expr) = self.array_element_expr(element) else {
                continue;
            };
            // Contextually type this position against its tuple element. In particular,
            // direct callback elements need their function parameter bindings before the
            // arrow body is checked.
            let elem_context = context_tuple
                .as_ref()
                .and_then(|tuple| self.tuple_context_element(tuple, index, array.elements.len()));
            let inferred = match (expr, elem_context) {
                (Expression::ArrowFunctionExpression(arrow), Some(ctx))
                    if self.interner.store().function_type(ctx).is_some() =>
                {
                    self.infer_contextual_arrow(scope, arrow, ctx)
                        .map(|ty| (ty, Span::from_oxc(arrow.span)))
                }
                _ => self.infer_initializer(scope, expr, elem_context),
            };
            let Some((elem_ty, _)) = inferred else {
                continue;
            };
            // Do NOT widen: the literal element type relates correctly to BOTH a literal
            // target (`1` <: `1`) and a base target (`1` <: `number`) via the relation, so
            // keeping it is what makes a literal-type tuple target work without breaking
            // the base-type case.
            elements.push(elem_ty);
        }
        self.interner.intern_tuple(elements)
    }

    /// Infer `a[i]`: arrays yield their element type for any index, tuples yield the
    /// literal-indexed element, and object bases use M19 index rules. Out-of-subset
    /// cases return the error type without diagnostics.
    fn infer_element_access(
        &mut self,
        scope: ScopeId,
        member: &ComputedMemberExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let (base_ty, _) = self.infer_expr(scope, &member.object)?;
        self.infer_element_access_from_base(scope, base_ty, member)
    }

    /// Infer `a[i]` after its base has already been evaluated by a call expression.
    pub(in crate::check::checker) fn infer_element_access_from_base(
        &mut self,
        scope: ScopeId,
        base_ty: TypeId,
        member: &ComputedMemberExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();
        let span = Span::from_oxc(member.span);

        // Authenticated well-known symbols are exact key literals. Certification
        // resolves the `Symbol` binding itself, so the generic member walker must not
        // reinterpret the trusted key as an ordinary static-property access.
        let authenticated_symbol = self
            .authenticated_well_known_symbol_expression_key(scope, &member.expression)
            .and_then(|key| key.as_well_known_symbol());
        // Other keys still walk normally for side effects, reference resolution, and
        // nested diagnostics.
        let key_ty = match authenticated_symbol {
            Some(symbol) => Some(
                self.interner
                    .intern_literal(LiteralValue::WellKnownSymbol(symbol)),
            ),
            None => self.infer_expr(scope, &member.expression).map(|(ty, _)| ty),
        };

        if base_ty == wk.any || base_ty == wk.error {
            return Some((wk.error, span));
        }

        // M24 (review F2): element access resolves through the base's **apparent type**
        // — `t["x"]` with `T extends { x: number }` and `t[0]` with `T extends number[]`
        // read through the constraint. For a non-parameter base this is the identity.
        let base_ty = match self.demand_composite_apparent_type(base_ty) {
            DemandOutcome::Ready(base_ty) => base_ty,
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), span);
                return None;
            }
        };
        let base_ty = self
            .interner
            .store()
            .readonly_operand(base_ty)
            .unwrap_or(base_ty);

        if key_ty == Some(wk.symbol) {
            let incomplete = if base_ty == wk.unknown {
                Some((
                    "expr-infer/element-access/unknown-receiver",
                    "broad symbol index has an unknown receiver",
                ))
            } else if base_ty == wk.null || base_ty == wk.undefined {
                Some((
                    "expr-infer/element-access/nullish-receiver",
                    "broad symbol index has a nullish receiver",
                ))
            } else if base_ty == wk.never {
                None
            } else {
                Some((
                    "expr-infer/element-access/implicit-any-index",
                    "broad symbol index requires implicit-any index checking",
                ))
            };
            if let Some((id, context)) = incomplete {
                self.record_incomplete(id, span, context);
                return None;
            }
        }

        if authenticated_symbol.is_some() {
            match exact_symbol_receiver_guard(base_ty, &wk) {
                ExactSymbolReceiverGuard::Continue => {}
                ExactSymbolReceiverGuard::Recovery(result) => return Some((result, span)),
                ExactSymbolReceiverGuard::Unknown => {
                    self.record_incomplete(
                        "expr-infer/element-access/unknown-receiver",
                        span,
                        "exact well-known symbol index has an unknown receiver",
                    );
                    return None;
                }
                ExactSymbolReceiverGuard::Nullish => {
                    self.record_incomplete(
                        "expr-infer/element-access/nullish-receiver",
                        span,
                        "exact well-known symbol index has a nullish receiver",
                    );
                    return None;
                }
            }
            let result = if self.interner.store().tag(base_ty) == TypeTag::Union {
                match self.union_exact_symbol_element_access(base_ty, key_ty) {
                    DemandOutcome::Ready(result) => result,
                    DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(DemandOutcome::Exhausted(exhaustion), span);
                        return None;
                    }
                }
            } else if self.interner.store().tag(base_ty) == TypeTag::Object {
                object_element_access(self.interner.store(), base_ty, key_ty)
            } else {
                ElementAccessLookup::UnsupportedReceiver
            };
            return match result {
                ElementAccessLookup::Found(result) => Some((result, span)),
                ElementAccessLookup::MissingObjectKey => {
                    self.record_incomplete(
                        "expr-infer/element-access/missing-symbol-key",
                        span,
                        "exact well-known symbol key is absent from the receiver",
                    );
                    None
                }
                ElementAccessLookup::NullishReceiver => {
                    self.record_incomplete(
                        "expr-infer/element-access/nullish-receiver",
                        span,
                        "exact well-known symbol index has a nullable union receiver",
                    );
                    None
                }
                ElementAccessLookup::UnsupportedReceiver => {
                    self.record_incomplete(
                        "expr-infer/element-access/unsupported-symbol-receiver",
                        span,
                        "exact well-known symbol lookup is unsupported for this receiver",
                    );
                    None
                }
            };
        }

        if self.interner.store().tag(base_ty) == TypeTag::Union {
            let result = match self.union_element_access(base_ty, &member.expression, key_ty) {
                DemandOutcome::Ready(result) => result,
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), span);
                    return None;
                }
            };
            return Some((result, span));
        }

        // Array base (M17): the result is the element type (any index).
        if let Some(array) = self.interner.store().array_type(base_ty) {
            return Some((array.element, span));
        }

        // Tuple base (M18): index by the **literal** numeric index. A non-literal index
        // or one out of range is out of subset → error type (no diagnostic, no crash).
        if self.interner.store().tag(base_ty) == TypeTag::Tuple {
            let element = literal_index(&member.expression).and_then(|i| {
                self.interner
                    .store()
                    .tuple_type(base_ty)?
                    .elements
                    .get(i)
                    .copied()
            });
            return Some((element.unwrap_or(wk.error), span));
        }

        // Object base (M19): resolve `obj[key]` through named properties / index sigs.
        if self.interner.store().tag(base_ty) == TypeTag::Object {
            let result = match object_element_access(self.interner.store(), base_ty, key_ty) {
                ElementAccessLookup::Found(result) => result,
                ElementAccessLookup::MissingObjectKey
                | ElementAccessLookup::NullishReceiver
                | ElementAccessLookup::UnsupportedReceiver => wk.error,
            };
            return Some((result, span));
        }

        // Non-array/non-tuple/non-object base: out of scope (no diagnostic, no crash) →
        // error type.
        Some((wk.error, span))
    }

    /// Resolve an element access across every union constituent. Class and
    /// constrained-parameter members are demanded before lookup; an unsupported
    /// constituent yields `unknown`, preserving downstream checking without using
    /// the error type as a successful operand.
    fn union_element_access(
        &mut self,
        union_ty: TypeId,
        key_expr: &Expression<'_>,
        key_ty: Option<TypeId>,
    ) -> DemandOutcome<TypeId> {
        let Some(members) = self.interner.store().union_members(union_ty) else {
            return DemandOutcome::Ready(self.interner.well_known().unknown);
        };
        let members = members.to_vec();
        let mut results = Vec::with_capacity(members.len());
        for member in members {
            let member = match self.demand_apparent_type(member) {
                DemandOutcome::Ready(member) => member,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
            let member = self
                .interner
                .store()
                .readonly_operand(member)
                .unwrap_or(member);
            let result = if let Some(array) = self.interner.store().array_type(member) {
                Some(array.element)
            } else if let Some(tuple) = self.interner.store().tuple_type(member) {
                literal_index(key_expr).and_then(|index| tuple.elements.get(index).copied())
            } else if self.interner.store().tag(member) == TypeTag::Object {
                match object_element_access(self.interner.store(), member, key_ty) {
                    ElementAccessLookup::Found(result) => Some(result),
                    ElementAccessLookup::MissingObjectKey
                    | ElementAccessLookup::NullishReceiver
                    | ElementAccessLookup::UnsupportedReceiver => None,
                }
            } else {
                None
            };
            results.push(result.unwrap_or(self.interner.well_known().unknown));
        }
        DemandOutcome::Ready(self.interner.union(results))
    }

    fn union_exact_symbol_element_access(
        &mut self,
        union_ty: TypeId,
        key_ty: Option<TypeId>,
    ) -> DemandOutcome<ElementAccessLookup> {
        let Some(members) = self.interner.store().union_members(union_ty) else {
            return DemandOutcome::Ready(ElementAccessLookup::UnsupportedReceiver);
        };
        let members = members.to_vec();
        let mut apparent_members = Vec::with_capacity(members.len());
        for member in members {
            let member = match self.demand_apparent_type(member) {
                DemandOutcome::Ready(member) => member,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
            let member = self
                .interner
                .store()
                .readonly_operand(member)
                .unwrap_or(member);
            apparent_members.push(member);
        }
        DemandOutcome::Ready(combine_exact_symbol_member_lookups(
            self.interner,
            &apparent_members,
            key_ty,
        ))
    }

    /// Resolve the M24 apparent type of constrained type parameters transitively.
    /// The visited set terminates constraint cycles in the safe over-report direction.
    /// All structural consumers route through this shared resolver.
    pub(in crate::check::checker) fn apparent_type(&self, ty: TypeId) -> TypeId {
        let store = self.interner.store();
        let mut current = ty;
        let mut seen: FxHashSet<TypeParamId> = FxHashSet::default();
        while store.tag(current) == TypeTag::TypeParam {
            let Some(id) = store.type_param(current).map(|p| p.id) else {
                break;
            };
            // A repeated parameter id means a constraint cycle — stop (terminate).
            if !seen.insert(id) {
                break;
            }
            match store.type_param_constraint(id) {
                Some(constraint) => current = constraint,
                None => break,
            }
        }
        current
    }

    pub(in crate::check::checker) fn demand_apparent_type(
        &mut self,
        ty: TypeId,
    ) -> DemandOutcome<TypeId> {
        let mut current = ty;
        let mut seen = FxHashSet::default();
        loop {
            if !seen.insert(current) {
                return DemandOutcome::Ready(current);
            }
            current = match self.evaluate_type(current) {
                DemandOutcome::Ready(demanded) => demanded,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
            let apparent = self.apparent_type(current);
            if apparent == current {
                return DemandOutcome::Ready(current);
            }
            current = apparent;
        }
    }

    /// Demand the outer type and every immediate union constituent as one atomic
    /// semantic query. This is the structural-consumer boundary for class roots
    /// hidden inside composite types.
    pub(in crate::check::checker) fn demand_composite_apparent_type(
        &mut self,
        ty: TypeId,
    ) -> DemandOutcome<TypeId> {
        self.with_semantic_query_transaction(|pass| pass.demand_composite_apparent_type_inner(ty))
    }

    fn demand_composite_apparent_type_inner(&mut self, ty: TypeId) -> DemandOutcome<TypeId> {
        let ty = match self.demand_apparent_type(ty) {
            DemandOutcome::Ready(ty) => ty,
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        };
        let Some(members) = self.interner.store().union_members(ty) else {
            return DemandOutcome::Ready(ty);
        };
        let members = members.to_vec();
        let mut apparent = Vec::with_capacity(members.len());
        for member in members {
            match self.demand_apparent_type(member) {
                DemandOutcome::Ready(member) => apparent.push(member),
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            }
        }
        DemandOutcome::Ready(self.interner.union(apparent))
    }

    /// Demand a composite and merge an intersection's visible object surface in
    /// the same transaction, so a later exhausted constituent promotes nothing.
    pub(in crate::check::checker) fn demand_structural_apparent_type(
        &mut self,
        ty: TypeId,
    ) -> DemandOutcome<TypeId> {
        self.with_semantic_query_transaction(|pass| pass.demand_structural_apparent_type_inner(ty))
    }

    pub(in crate::check::checker) fn demand_structural_apparent_type_inner(
        &mut self,
        ty: TypeId,
    ) -> DemandOutcome<TypeId> {
        let ty = match self.demand_composite_apparent_type_inner(ty) {
            DemandOutcome::Ready(ty) => ty,
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        };
        match self.intersection_apparent_object_inner(ty) {
            DemandOutcome::Ready(Some(apparent)) => DemandOutcome::Ready(apparent),
            DemandOutcome::Ready(None) => DemandOutcome::Ready(ty),
            DemandOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
        }
    }

    /// Promote semantic-query writes only when the whole consumer completes.
    pub(in crate::check::checker) fn with_semantic_query_transaction<R>(
        &mut self,
        produce: impl FnOnce(&mut Self) -> DemandOutcome<R>,
    ) -> DemandOutcome<R> {
        #[cfg(test)]
        crate::check::query::record_semantic_query_transaction();
        self.semantic_queries.savepoint();
        let outcome = produce(self);
        if matches!(outcome, DemandOutcome::Ready(_)) {
            self.semantic_queries.commit();
        } else {
            self.semantic_queries.rollback();
        }
        outcome
    }

    /// Intern the merged apparent object for an intersection (M31). Properties are
    /// unioned by name, duplicate property/index types are intersected, optional
    /// requires all contributors, and readonly/accessor wins if any contributor has it.
    pub(in crate::check::checker) fn intersection_apparent_object(
        &mut self,
        ty: TypeId,
    ) -> DemandOutcome<Option<TypeId>> {
        self.with_semantic_query_transaction(|pass| pass.intersection_apparent_object_inner(ty))
    }

    fn intersection_apparent_object_inner(&mut self, ty: TypeId) -> DemandOutcome<Option<TypeId>> {
        if self.interner.store().tag(ty) != TypeTag::Intersection {
            return DemandOutcome::Ready(None);
        }
        let Some(members) = self.interner.store().intersection_members(ty) else {
            return DemandOutcome::Ready(None);
        };
        let members: Vec<TypeId> = members.to_vec();

        // Snapshot each object member's properties + index signatures (owned) before any
        // mutable interning borrow. A constrained-param member resolves through its
        // apparent type first, mirroring the union member-access handling.
        let mut snapshots: Vec<(Vec<PropertyType>, Option<TypeId>, Option<TypeId>)> = Vec::new();
        for &member in &members {
            let member = match self.demand_apparent_type(member) {
                DemandOutcome::Ready(member) => member,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
            if let Some(obj) = self.interner.store().object_type(member) {
                snapshots.push((obj.properties.clone(), obj.string_index, obj.number_index));
            }
        }
        if snapshots.is_empty() {
            return DemandOutcome::Ready(None);
        }

        // Accumulate per property name: the base member (first seen, for its
        // visibility/origin), the contributing value types, and the merged flags.
        struct Acc {
            base: PropertyType,
            tys: Vec<TypeId>,
            write_tys: Vec<TypeId>,
            has_write_ty: bool,
            all_optional: bool,
            any_readonly: bool,
            any_accessor: bool,
        }
        let mut order: Vec<PropertyKey> = Vec::new();
        let mut props: FxHashMap<PropertyKey, Acc> = FxHashMap::default();
        let mut string_index_values: Vec<TypeId> = Vec::new();
        let mut number_index_values: Vec<TypeId> = Vec::new();

        for (member_props, string_index, number_index) in snapshots {
            for prop in member_props {
                let write_ty = prop.write_ty.unwrap_or(prop.ty);
                match props.get_mut(&prop.key) {
                    Some(acc) => {
                        acc.tys.push(prop.ty);
                        acc.write_tys.push(write_ty);
                        acc.has_write_ty |= prop.write_ty.is_some();
                        acc.all_optional &= prop.optional;
                        acc.any_readonly |= prop.readonly;
                        acc.any_accessor |= prop.is_accessor;
                    }
                    None => {
                        order.push(prop.key.clone());
                        props.insert(
                            prop.key.clone(),
                            Acc {
                                all_optional: prop.optional,
                                any_readonly: prop.readonly,
                                any_accessor: prop.is_accessor,
                                tys: vec![prop.ty],
                                write_tys: vec![write_ty],
                                has_write_ty: prop.write_ty.is_some(),
                                base: prop,
                            },
                        );
                    }
                }
            }
            string_index_values.extend(string_index);
            number_index_values.extend(number_index);
        }

        // Build the merged property list (each value type is the intersection of its
        // contributors — a single contributor collapses to itself).
        let mut properties: Vec<PropertyType> = Vec::with_capacity(order.len());
        for name in order {
            let Some(acc) = props.remove(&name) else {
                continue;
            };
            let ty = self.interner.intersection(acc.tys);
            let write_ty = acc
                .has_write_ty
                .then(|| self.interner.intersection(acc.write_tys));
            properties.push(PropertyType {
                ty,
                write_ty,
                optional: acc.all_optional,
                readonly: acc.any_readonly,
                is_accessor: acc.any_accessor,
                ..acc.base
            });
        }
        let string_index = (!string_index_values.is_empty())
            .then(|| self.interner.intersection(string_index_values));
        let number_index = (!number_index_values.is_empty())
            .then(|| self.interner.intersection(number_index_values));

        DemandOutcome::Ready(Some(self.interner.intern_object(ObjectType {
            properties,
            string_index,
            number_index,
            ..Default::default()
        })))
    }

    /// Infer `obj.prop`. Missing properties emit `TK2339`; union bases require the
    /// property on every member and union the member property types.
    fn infer_member_access(
        &mut self,
        scope: ScopeId,
        member: &StaticMemberExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let (base_ty, _) = self.infer_expr(scope, &member.object)?;
        if let Some(DemandOutcome::Exhausted(exhaustion)) =
            self.demand_class_value_surface(scope, &member.object)
        {
            self.own_type_demand(
                DemandOutcome::Exhausted(exhaustion),
                Span::from_oxc(member.property.span),
            );
            return None;
        }
        self.infer_member_access_from_base(scope, base_ty, member)
    }

    /// Demand publication for an explicit class value before consuming its static
    /// object. Static templates are ordinary objects, so poison must enter through
    /// the class binding rather than being mistaken for a missing property.
    pub(in crate::check::checker) fn demand_class_value_surface(
        &self,
        scope: ScopeId,
        expression: &Expression<'_>,
    ) -> Option<DemandOutcome<()>> {
        let identifier = match expression {
            Expression::ParenthesizedExpression(parenthesized) => {
                return self.demand_class_value_surface(scope, &parenthesized.expression);
            }
            Expression::Identifier(identifier) => identifier,
            _ => return None,
        };
        let value_decl = self.value_decl_id_replay(scope, identifier.name.as_str())?;
        let class_decl = self
            .class_value_aliases
            .get(&value_decl)
            .copied()
            .unwrap_or(value_decl);
        let binding = self.class_value_bindings.get(&class_decl).copied()?;
        if value_decl != class_decl && binding.has_header_type_params {
            return None;
        }
        let class_id = binding.class_id;
        Some(match self.published_class_replay(class_id) {
            DemandOutcome::Ready(_) => DemandOutcome::Ready(()),
            DemandOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
        })
    }

    /// Resolve a member after its object has already been inferred. Calls use this
    /// path to preserve the object type as the explicit receiver source.
    pub(in crate::check::checker) fn infer_member_access_from_base(
        &mut self,
        scope: ScopeId,
        base_ty: TypeId,
        member: &StaticMemberExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();
        let prop_name = member.property.name.as_str();
        let prop_span = Span::from_oxc(member.property.span);

        if let Some(lookup) = self.body_this_member_lookup(&member.object, prop_name) {
            match lookup {
                BodyMemberLookup::Known { ty, metadata, .. } => {
                    self.check_member_access_control(
                        prop_name,
                        prop_span,
                        metadata.visibility,
                        metadata.declaring_class,
                    );
                    return Some((ty, prop_span));
                }
                BodyMemberLookup::Unavailable(metadata) => {
                    self.check_member_access_control(
                        prop_name,
                        prop_span,
                        metadata.visibility,
                        metadata.declaring_class,
                    );
                    return None;
                }
                BodyMemberLookup::Missing { definite: false } => return None,
                BodyMemberLookup::Missing { definite: true } => {
                    let target = self
                        .current_class
                        .and_then(|class| self.class_names.get(&class))
                        .cloned()
                        .unwrap_or_else(|| "class body".to_string());
                    self.emit_diagnostic(Diagnostic::property_does_not_exist(
                        prop_span, prop_name, &target,
                    ));
                    return Some((wk.error, prop_span));
                }
            }
        }

        if base_ty == wk.any || base_ty == wk.error {
            return Some((wk.error, prop_span));
        }

        match self.lookup_library_composed_member(base_ty, prop_name, prop_span) {
            LibraryComposedMember::Ready { ty, access } => {
                if let Some((visibility, declaring_class)) = access
                    .into_iter()
                    .find(|constraint| !self.member_access_constraint_allowed(*constraint))
                {
                    self.check_member_access_control(
                        prop_name,
                        prop_span,
                        visibility,
                        declaring_class,
                    );
                }
                return Some((ty, prop_span));
            }
            LibraryComposedMember::Missing => {
                return self.missing_member_access(scope, base_ty, member, prop_name, prop_span);
            }
            LibraryComposedMember::Unavailable => return Some((wk.error, prop_span)),
            LibraryComposedMember::NotInstalled => {}
        }

        // Resolve members through apparent/merged types, but keep `base_ty` for
        // `TK2339` rendering so missing constrained-parameter members still name `T`.
        let lookup_ty = match self.demand_structural_apparent_type(base_ty) {
            DemandOutcome::Ready(lookup_ty) => lookup_ty,
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), prop_span);
                return None;
            }
        };
        if prop_name == "length" {
            if let Some(length) = self.native_member_length(lookup_ty) {
                return Some((length, prop_span));
            }
        }
        let lookup_ty = match self.project_library_member_surface(lookup_ty, prop_span) {
            LibraryMemberProjection::NotApplicable => lookup_ty,
            LibraryMemberProjection::Ready(projected) => projected,
            LibraryMemberProjection::Unavailable => return Some((wk.error, prop_span)),
        };
        // Union base (M4): the property must exist on every member; its type is the
        // union of the per-member property types.
        if self.interner.store().tag(lookup_ty) == TypeTag::Union {
            return Some((
                self.union_member_access(lookup_ty, prop_name, prop_span),
                prop_span,
            ));
        }

        // Array base (M17): only `length` is synthesized without a full library. Every other array
        // member (`push`, `map`, `filter`, …) needs `lib.d.ts`, so it is deferred →
        // `TK2339` (property does not exist), with the array type rendered in the message
        // (`number[]`). The access yields the error type on the missing path (no cascade).
        if self.interner.store().tag(lookup_ty) == TypeTag::Array {
            let tgt = render_type(self.interner.store(), lookup_ty, /* widen */ false);
            self.emit_diagnostic(Diagnostic::property_does_not_exist(
                prop_span, prop_name, &tgt,
            ));
            return Some((wk.error, prop_span));
        }

        // Snapshot the looked-up property's type + visibility + origin before any
        // mutable borrow (a diagnostic needs `&mut pass`). `None` = the property is not
        // on this object type. Looked up on the apparent type (M24).
        let found = self
            .interner
            .store()
            .object_type(lookup_ty)
            .and_then(|obj| obj.property(prop_name))
            .map(|prop| (prop.ty, prop.visibility, prop.declaring_class));

        // M19: a property access `obj.prop` resolves through a **string** index
        // signature when there is no named property of that name — `dict.a` on
        // `{ [k: string]: number }` is `number` (a string-keyed access), not `TK2339`.
        // Snapshot it before any diagnostic borrow.
        let string_index_value = self
            .interner
            .store()
            .object_type(lookup_ty)
            .and_then(|obj| obj.string_index);

        match found {
            Some((prop_ty, visibility, declaring_class)) => {
                // M13: access violations are `TK2341`/`TK2445`, not missing-property
                // errors, and still yield the real member type to avoid cascades.
                self.check_member_access_control(prop_name, prop_span, visibility, declaring_class);
                Some((prop_ty, prop_span))
            }
            None => {
                // M19: a string index signature accepts any property name — the access
                // yields its value type rather than `TK2339`.
                if let Some(value) = string_index_value {
                    return Some((value, prop_span));
                }
                self.missing_member_access(scope, base_ty, member, prop_name, prop_span)
            }
        }
    }

    fn member_access_constraint_allowed(
        &self,
        (visibility, declaring_class): (Visibility, Option<ClassId>),
    ) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Private => {
                declaring_class.is_some_and(|owner| self.has_exact_class_access_context(owner))
            }
            Visibility::Protected => {
                declaring_class.is_some_and(|owner| self.has_derived_class_access_context(owner))
            }
        }
    }

    fn missing_member_access(
        &mut self,
        scope: ScopeId,
        base_ty: TypeId,
        member: &StaticMemberExpression<'_>,
        prop_name: &str,
        prop_span: Span,
    ) -> Option<(TypeId, Span)> {
        let error = self.interner.well_known().error;
        match self.class_instance_static_member_owner(base_ty, prop_name) {
            DemandOutcome::Ready(Some(class)) => {
                self.emit_diagnostic(Diagnostic::static_property_accessed_on_instance(
                    prop_span, prop_name, &class,
                ));
                return Some((error, prop_span));
            }
            DemandOutcome::Ready(None) => {}
            DemandOutcome::Exhausted(exhaustion) => {
                self.own_type_demand(DemandOutcome::Exhausted(exhaustion), prop_span);
                return None;
            }
        }
        if let Some(class) = self.class_instance_name(base_ty) {
            self.emit_diagnostic(Diagnostic::property_does_not_exist_on_named_type(
                prop_span, prop_name, &class,
            ));
        } else if let Some(value) = self.named_class_or_function_value(scope, &member.object) {
            self.emit_diagnostic(Diagnostic::property_does_not_exist_on_named_value(
                prop_span, prop_name, &value,
            ));
        } else {
            let target = render_type(self.interner.store(), base_ty, false);
            self.emit_diagnostic(Diagnostic::property_does_not_exist(
                prop_span, prop_name, &target,
            ));
        }
        Some((error, prop_span))
    }

    fn class_instance_name(&self, ty: TypeId) -> Option<String> {
        let application = self.interner.store().class_instance_type(ty)?;
        self.class_names.get(&application.class).cloned()
    }

    /// Return the source receiver name only after proving that it denotes a
    /// published class value or one of the function/namespace drafts.
    fn named_class_or_function_value(
        &self,
        scope: ScopeId,
        expression: &Expression<'_>,
    ) -> Option<String> {
        let identifier = match expression {
            Expression::ParenthesizedExpression(parenthesized) => {
                return self.named_class_or_function_value(scope, &parenthesized.expression);
            }
            Expression::Identifier(identifier) => identifier,
            _ => return None,
        };
        let symbol = self.resolve_value_replay(scope, identifier.name.as_str())?;
        if self.function_groups.contains_symbol(symbol)
            || self.named_function_symbols.contains(&symbol)
        {
            return Some(identifier.name.to_string());
        }
        let value_decl = self.binder.symbols.get(symbol)?.value?;
        let class_decl = self
            .class_value_aliases
            .get(&value_decl)
            .copied()
            .unwrap_or(value_decl);
        let binding = self.class_value_bindings.get(&class_decl)?;
        if value_decl != class_decl && binding.has_header_type_params {
            return None;
        }
        Some(identifier.name.to_string())
    }

    /// Recognize the one cross-side lookup with a dedicated tsc diagnostic. The
    /// check is deliberately limited to the same class's published static surface.
    fn class_instance_static_member_owner(
        &self,
        base_ty: TypeId,
        property: &str,
    ) -> DemandOutcome<Option<String>> {
        let Some(application) = self.interner.store().class_instance_type(base_ty) else {
            return DemandOutcome::Ready(None);
        };
        let surface = match self.published_class_replay(application.class) {
            DemandOutcome::Ready(surface) => surface,
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        };
        let has_static_property = self
            .interner
            .store()
            .object_type(surface.static_template())
            .and_then(|object| object.property(property))
            .is_some();
        if !has_static_property {
            return DemandOutcome::Ready(None);
        }
        let class = self
            .class_names
            .get(&application.class)
            .cloned()
            .unwrap_or_else(|| render_type(self.interner.store(), base_ty, false));
        DemandOutcome::Ready(Some(class))
    }

    /// Resolve `union.prop` by requiring the property on every member and unioning
    /// member property types. Non-object members count as missing; `any`/error
    /// contributes the error type.
    fn union_member_access(
        &mut self,
        union_ty: TypeId,
        prop_name: &str,
        prop_span: Span,
    ) -> TypeId {
        let wk = self.interner.well_known();

        // Snapshot the member ids: the per-member lookups below are immutable, but
        // interning the result union needs `&mut`, so the borrow must not be held.
        let Some(members) = self.interner.store().union_members(union_ty) else {
            return wk.error;
        };
        let members: Vec<TypeId> = members.to_vec();

        let mut member_prop_types: Vec<TypeId> = Vec::with_capacity(members.len());
        for member in members {
            if member == wk.any || member == wk.error {
                member_prop_types.push(wk.error);
                continue;
            }
            // M24 (audit): a union MEMBER that is a constrained type parameter resolves
            // through its apparent type too (`(T | U).x` with both constrained to
            // `HasX`), else the lookup would falsely report `TK2339`.
            let member = self.apparent_type(member);
            match self
                .interner
                .store()
                .object_type(member)
                .and_then(|o| o.property(prop_name))
            {
                Some(prop) => member_prop_types.push(prop.ty),
                // Missing on this member: the property does not exist on the union.
                None => {
                    let tgt = render_type(self.interner.store(), union_ty, /* widen */ false);
                    self.emit_diagnostic(Diagnostic::property_does_not_exist(
                        prop_span, prop_name, &tgt,
                    ));
                    return wk.error;
                }
            }
        }

        // Present on every member: the result is the union of the per-member types.
        self.interner.union(member_prop_types)
    }
}

/// How one binary-operator operand answered the rule it was asked (backlog `45`) —
/// numeric for the arithmetic family, string-like for `+`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum OperandVerdict {
    Satisfied,
    /// Provably does not satisfy the rule that was asked.
    Rejected,
    /// Not decided here: an `unknown` operand (the unknown-receiver family owns it),
    /// a deferred type-level node, or an exhausted relation query. Never reports.
    Deferred,
}

/// Recursion budget for [`is_undecided_operand_shape`]. Unions/intersections are
/// flattened by the interner, so a legitimate shape needs one or two levels; anything
/// deeper is treated as undecided rather than risking unbounded traversal.
const UNDECIDED_SHAPE_DEPTH: u32 = 8;

/// Whether the relation engine can only relate this type identically, which makes a
/// `No` answer uninformative about the operand rule (a deferred conditional such as
/// `T extends string ? number : number` IS a valid numeric operand for tsc).
fn is_undecided_operand_shape(store: &Store, ty: TypeId, depth: u32) -> bool {
    if depth == 0 {
        return true;
    }
    match store.tag(ty) {
        TypeTag::Conditional
        | TypeTag::Instantiation
        | TypeTag::Infer
        | TypeTag::Mapped
        | TypeTag::MappedValue
        | TypeTag::Keyof
        | TypeTag::DeferredIndexedAccess
        | TypeTag::Declared => true,
        TypeTag::Union => store
            .union_members(ty)
            .is_some_and(|members| members_undecided(store, members, depth)),
        TypeTag::Intersection => store
            .intersection_members(ty)
            .is_some_and(|members| members_undecided(store, members, depth)),
        _ => false,
    }
}

fn members_undecided(store: &Store, members: &[TypeId], depth: u32) -> bool {
    members
        .iter()
        .any(|member| is_undecided_operand_shape(store, *member, depth - 1))
}

fn conditional_undefined_arity_subtype(store: &Store, source: TypeId, target: TypeId) -> bool {
    match (store.tag(source), store.tag(target)) {
        (TypeTag::Function, TypeTag::Function) => store
            .function_type(source)
            .zip(store.function_type(target))
            .is_some_and(|(source, target)| {
                conditional_undefined_arity_function_subtype(store, source, target)
            }),
        (TypeTag::Object, TypeTag::Object) => store
            .object_type(source)
            .zip(store.object_type(target))
            .is_some_and(|(source, target)| {
                conditional_undefined_arity_object_subtype(store, source, target)
            }),
        _ => false,
    }
}

fn conditional_undefined_arity_object_subtype(
    store: &Store,
    source: &ObjectType,
    target: &ObjectType,
) -> bool {
    if source.properties.len() != target.properties.len()
        || source.string_index != target.string_index
        || source.number_index != target.number_index
        || source.call_signatures != target.call_signatures
        || source.construct_signatures != target.construct_signatures
    {
        return false;
    }
    source
        .properties
        .iter()
        .zip(&target.properties)
        .try_fold(false, |changed, (source, target)| {
            let same_metadata = source.key == target.key
                && source.write_ty == target.write_ty
                && source.optional == target.optional
                && source.visibility == target.visibility
                && source.declaring_class == target.declaring_class
                && source.readonly == target.readonly
                && source.is_accessor == target.is_accessor;
            if !same_metadata {
                return None;
            }
            if source.ty == target.ty {
                return Some(changed);
            }
            store
                .function_type(source.ty)
                .zip(store.function_type(target.ty))
                .filter(|(source, target)| {
                    conditional_undefined_arity_function_subtype(store, source, target)
                })
                .map(|_| true)
        })
        == Some(true)
}

fn conditional_undefined_arity_function_subtype(
    store: &Store,
    source: &FunctionType,
    target: &FunctionType,
) -> bool {
    if !source.type_params.is_empty()
        || !target.type_params.is_empty()
        || source.receiver != target.receiver
        || source.ret != target.ret
        || source.params.len() != target.params.len()
    {
        return false;
    }
    let mut changed_arity = false;
    for (source, target) in source.params.iter().zip(&target.params) {
        let unchanged = source.ty == target.ty
            && source.optional == target.optional
            && source.has_default == target.has_default
            && source.rest == target.rest;
        if unchanged {
            continue;
        }
        let required_undefined_to_optional = !source.rest
            && !source.optional
            && !source.has_default
            && target.optional
            && !target.rest
            && conditional_parameter_type_within(store, target.ty, source.ty);
        if !required_undefined_to_optional {
            return false;
        }
        changed_arity = true;
    }
    changed_arity
}

fn conditional_parameter_type_within(store: &Store, candidate: TypeId, container: TypeId) -> bool {
    let Some(container_members) = store.union_members(container) else {
        return false;
    };
    if !container_members
        .iter()
        .any(|member| store.intrinsic_kind(*member) == Some(IntrinsicKind::Undefined))
    {
        return false;
    }
    let within = |candidate: TypeId| {
        container_members.iter().any(|container| {
            candidate == *container
                || store.literal_value(candidate).is_some_and(|literal| {
                    store.intrinsic_kind(*container) == Some(literal.base_kind())
                })
        })
    };
    store.union_members(candidate).map_or_else(
        || within(candidate),
        |members| members.iter().copied().all(within),
    )
}

/// Whether a binary operator is an arithmetic, bitwise, or shift operator — the family
/// whose result is `number` and whose operands must be numeric. `+` has its own
/// string/number rules and is handled separately; `in`/`instanceof` stay out of subset.
fn is_arithmetic_operator(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Subtraction
            | BinaryOperator::Multiplication
            | BinaryOperator::Division
            | BinaryOperator::Remainder
            | BinaryOperator::Exponential
            | BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseOR
            | BinaryOperator::BitwiseXOR
            | BinaryOperator::ShiftLeft
            | BinaryOperator::ShiftRight
            | BinaryOperator::ShiftRightZeroFill
    )
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum AdditionOperandKind {
    Error,
    Any,
    String,
    Number,
    Unsupported,
}

fn addition_operand_kind(store: &Store, ty: TypeId) -> AdditionOperandKind {
    let scalar = store
        .literal_value(ty)
        .map(LiteralValue::base_kind)
        .or_else(|| store.intrinsic_kind(ty));
    if let Some(kind) = scalar {
        return match kind {
            IntrinsicKind::Error => AdditionOperandKind::Error,
            IntrinsicKind::Any => AdditionOperandKind::Any,
            IntrinsicKind::String => AdditionOperandKind::String,
            IntrinsicKind::Number => AdditionOperandKind::Number,
            _ => AdditionOperandKind::Unsupported,
        };
    }
    let Some(members) = store.union_members(ty) else {
        return AdditionOperandKind::Unsupported;
    };
    let mut kinds = members
        .iter()
        .map(|member| addition_operand_kind(store, *member));
    let Some(first) = kinds.next() else {
        return AdditionOperandKind::Unsupported;
    };
    if kinds.all(|kind| {
        kind == first
            && matches!(
                kind,
                AdditionOperandKind::Number | AdditionOperandKind::String
            )
    }) {
        first
    } else {
        AdditionOperandKind::Unsupported
    }
}

/// Whether a binary operator is a comparison/equality operator (its result is
/// `boolean`). Equality operators (`==`/`!=`/`===`/`!==`) and the relational
/// operators all qualify; arithmetic/bitwise/`in`/`instanceof` do not (the latter
/// two are out of the M7 subset).
fn is_comparison_operator(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Equality
            | BinaryOperator::Inequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
            | BinaryOperator::LessThan
            | BinaryOperator::LessEqualThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterEqualThan
    )
}

fn is_const_assertion_type(ts_type: &TSType<'_>) -> bool {
    let TSType::TSTypeReference(reference) = ts_type else {
        return false;
    };
    if reference.type_arguments.is_some() {
        return false;
    }
    let TSTypeName::IdentifierReference(ident) = &reference.type_name else {
        return false;
    };
    ident.name.as_str() == "const"
}

fn property_key_from_type(store: &Store, ty: TypeId) -> Option<PropertyKey> {
    match store.literal_value(ty) {
        Some(LiteralValue::String(name)) => Some(PropertyKey::String(name.clone())),
        Some(LiteralValue::WellKnownSymbol(symbol)) => Some(PropertyKey::WellKnownSymbol(*symbol)),
        _ => None,
    }
}

/// Resolve object element access without using the recovery type as a missing-key sentinel.
pub(in crate::check::checker) fn object_element_access(
    store: &Store,
    base_ty: TypeId,
    key_ty: Option<TypeId>,
) -> ElementAccessLookup {
    let Some(obj) = store.object_type(base_ty) else {
        return ElementAccessLookup::UnsupportedReceiver;
    };

    if let Some(key) = key_ty.and_then(|key| property_key_from_type(store, key)) {
        if let Some(prop) = obj.property_by_key(&key) {
            return ElementAccessLookup::Found(prop.ty);
        }
        if matches!(key, PropertyKey::WellKnownSymbol(_)) {
            return ElementAccessLookup::MissingObjectKey;
        }
    }

    if key_ty.is_some_and(|key| is_number_keyed(store, key)) {
        if let Some(value) = obj.number_index {
            return ElementAccessLookup::Found(value);
        }
    }

    if let Some(value) = obj.string_index {
        return ElementAccessLookup::Found(value);
    }

    ElementAccessLookup::MissingObjectKey
}

pub(in crate::check::checker) fn exact_symbol_receiver_guard(
    base_ty: TypeId,
    well_known: &WellKnown,
) -> ExactSymbolReceiverGuard {
    if base_ty == well_known.any || base_ty == well_known.error || base_ty == well_known.never {
        ExactSymbolReceiverGuard::Recovery(well_known.error)
    } else if base_ty == well_known.unknown {
        ExactSymbolReceiverGuard::Unknown
    } else if base_ty == well_known.null || base_ty == well_known.undefined {
        ExactSymbolReceiverGuard::Nullish
    } else {
        ExactSymbolReceiverGuard::Continue
    }
}

pub(in crate::check::checker) fn combine_exact_symbol_member_lookups(
    interner: &mut Interner,
    members: &[TypeId],
    key_ty: Option<TypeId>,
) -> ElementAccessLookup {
    let mut results = Vec::with_capacity(members.len());
    let mut missing_object_key = false;
    let mut nullish_receiver = false;
    let mut unsupported_receiver = false;
    for member in members {
        if *member == interner.well_known().never {
            continue;
        }
        match exact_symbol_receiver_guard(*member, &interner.well_known()) {
            ExactSymbolReceiverGuard::Recovery(result) => {
                results.push(result);
                continue;
            }
            ExactSymbolReceiverGuard::Nullish => {
                nullish_receiver = true;
                continue;
            }
            ExactSymbolReceiverGuard::Unknown => {
                unsupported_receiver = true;
                continue;
            }
            ExactSymbolReceiverGuard::Continue => {}
        }
        match object_element_access(interner.store(), *member, key_ty) {
            ElementAccessLookup::Found(result) => results.push(result),
            ElementAccessLookup::MissingObjectKey => missing_object_key = true,
            ElementAccessLookup::NullishReceiver => nullish_receiver = true,
            ElementAccessLookup::UnsupportedReceiver => unsupported_receiver = true,
        }
    }
    if missing_object_key {
        ElementAccessLookup::MissingObjectKey
    } else if unsupported_receiver {
        ElementAccessLookup::UnsupportedReceiver
    } else if nullish_receiver {
        ElementAccessLookup::NullishReceiver
    } else {
        ElementAccessLookup::Found(interner.union(results))
    }
}

/// Whether a key type is **number-keyed** (M19) — a numeric literal type or the
/// `number` intrinsic. A number index signature is selected for such a key. (A
/// numeric-literal key like `nums[0]` has a literal type whose base is `number`; a
/// `number`-typed variable matches directly.)
fn is_number_keyed(store: &Store, key: TypeId) -> bool {
    if let Some(lit) = store.literal_value(key) {
        return matches!(lit.base_kind(), IntrinsicKind::Number);
    }
    store.intrinsic_kind(key) == Some(IntrinsicKind::Number)
}

pub(in crate::check::checker) fn contextual_literal_target(store: &Store, ty: TypeId) -> TypeId {
    if let Some(operand) = store.readonly_operand(ty) {
        if is_contextual_literal_shape(store, operand) {
            return operand;
        }
    }
    let Some(members) = store.union_members(ty) else {
        return ty;
    };
    let mut shape = None;
    for &member in members {
        let member = store.readonly_operand(member).unwrap_or(member);
        if is_contextual_literal_shape(store, member) {
            if shape.replace(member).is_some() {
                return ty;
            }
        } else if !matches!(
            store.intrinsic_kind(member),
            Some(IntrinsicKind::Null | IntrinsicKind::Undefined)
        ) {
            return ty;
        }
    }
    shape.unwrap_or(ty)
}

fn is_contextual_literal_shape(store: &Store, ty: TypeId) -> bool {
    matches!(
        store.tag(ty),
        TypeTag::Object | TypeTag::Array | TypeTag::Tuple
    )
}

/// The sub-expressions a ternary / logical expression passes its contextual target
/// down to — the ones that carry the value, per tsc's
/// `getContextualTypeForBinaryOperand`: both ternary arms, both `||` / `??`
/// operands, and only `&&`'s right (its left is a condition, not a shaped value).
/// `None` for every other shape. The single statement of that rule: the value
/// inference ([`Pass::infer_conditional`] / [`Pass::infer_logical`]), the
/// contextual re-walk gate, and the excess-property walk must agree on it.
pub(in crate::check::checker) fn contextual_value_operands<'e, 'ast>(
    expr: &'e Expression<'ast>,
) -> Option<[Option<&'e Expression<'ast>>; 2]> {
    match expr {
        Expression::ConditionalExpression(cond) => {
            Some([Some(&cond.consequent), Some(&cond.alternate)])
        }
        Expression::LogicalExpression(logical) => Some([
            (logical.operator != LogicalOperator::And).then_some(&logical.left),
            Some(&logical.right),
        ]),
        _ => None,
    }
}

fn is_numeric_property_name(name: &str) -> bool {
    name.parse::<f64>().map(|n| n.is_finite()).unwrap_or(false)
}

/// Read a non-negative integer literal tuple index, or `None` for non-literal,
/// fractional, negative, non-finite, or out-of-`usize` indices.
fn literal_index(expr: &Expression<'_>) -> Option<usize> {
    let Expression::NumericLiteral(lit) = expr else {
        return None;
    };
    let value = lit.value;
    // Must be a finite, whole, non-negative number that fits a `usize` index.
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
        return None;
    }
    // `usize::MAX as f64` is exact enough as an upper bound; a tuple long enough to
    // matter is impossible in practice, so this only rejects absurd indices.
    if value > usize::MAX as f64 {
        return None;
    }
    Some(value as usize)
}

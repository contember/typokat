//! expr module (extracted from checker/mod.rs).

use super::calls::widen;
use super::context::*;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::flow::FlowNodeId;
use crate::diagnostics::{render_type, Diagnostic};
use crate::span::Span;
use crate::types::repr::{
    IntrinsicKind, LiteralValue, ObjectType, PropertyType, TypeParamId, TypeTag,
};
use crate::types::store::{Store, TypeId};
use oxc_ast::ast::{
    ArrayExpression, ArrayExpressionElement, BinaryExpression, BinaryOperator, ChainElement,
    ComputedMemberExpression, Expression, LogicalExpression, ObjectExpression, ObjectPropertyKind,
    SimpleAssignmentTarget, StaticMemberExpression, TSType, TSTypeName, UnaryExpression,
    UnaryOperator,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, 'ast> Pass<'a, 'ast> {
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
            ),
            Expression::TSTypeAssertion(assertion) => self.infer_assertion(
                scope,
                &assertion.expression,
                &assertion.type_annotation,
                Span::from_oxc(assertion.span),
            ),
            Expression::Identifier(ident) => {
                let span = Span::from_oxc(ident.span);
                // The `undefined` keyword parses as an identifier reference.
                if ident.name.as_str() == "undefined" {
                    return Some((well_known.undefined, span));
                }
                match self.binder.resolve_value(scope, ident.name.as_str()) {
                    Some(symbol_id) => Some((
                        self.resolve_identifier_type(symbol_id, ident.span.start),
                        span,
                    )),
                    None => {
                        self.diagnostics
                            .push(Diagnostic::cannot_find_name(span, ident.name.as_str()));
                        Some((well_known.error, span))
                    }
                }
            }
            // M7: condition shapes (`typeof x`, `!x`, `x === null`, …). They are walked
            // for their operands' side effects (resolving references / descending into
            // nested constructs); their *value* type is only ever a condition, never an
            // assignment source in the subset, so a coarse result type is sufficient.
            Expression::UnaryExpression(unary) => Some(self.infer_unary(scope, unary)),
            Expression::BinaryExpression(binary) => Some(self.infer_binary(scope, binary)),
            Expression::LogicalExpression(logical) => Some(self.infer_logical(scope, logical)),
            // M23: a ternary `test ? a : b` — walk all three parts for their side
            // effects (references resolve against the flow node the pre-pass recorded,
            // so a guarded arm is narrowed). Its value type is coarse (never an
            // assignment source in the subset), like a logical expression.
            Expression::ConditionalExpression(cond) => {
                self.infer_expr(scope, &cond.test);
                self.infer_expr(scope, &cond.consequent);
                self.infer_expr(scope, &cond.alternate);
                Some((well_known.error, Span::from_oxc(cond.span)))
            }
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
                self.record_incomplete(
                    "expr-infer/regexp-literal/self",
                    Span::from_oxc(lit.span),
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
        match target {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(ident) => {
                if ident.name.as_str() == "undefined" {
                    return;
                }
                if self
                    .binder
                    .resolve_value(scope, ident.name.as_str())
                    .is_none()
                {
                    self.diagnostics.push(Diagnostic::cannot_find_name(
                        Span::from_oxc(ident.span),
                        ident.name.as_str(),
                    ));
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
                self.infer_expr(scope, &assertion.expression);
                self.lower_annotation(scope, &assertion.type_annotation);
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
                self.infer_expr(scope, &assertion.expression);
                self.lower_annotation(scope, &assertion.type_annotation);
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
                self.infer_chain_expression(scope, &assertion.expression);
                self.lower_annotation(scope, &assertion.type_annotation);
            }
            Expression::TSTypeAssertion(assertion) => {
                self.infer_chain_expression(scope, &assertion.expression);
                self.lower_annotation(scope, &assertion.type_annotation);
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
    fn infer_assertion(
        &mut self,
        scope: ScopeId,
        expression: &Expression<'_>,
        type_annotation: &TSType<'_>,
        span: Span,
    ) -> Option<(TypeId, Span)> {
        let source = self.infer_expr(scope, expression);
        if is_const_assertion_type(type_annotation) {
            return source;
        }
        let asserted = self
            .lower_annotation(scope, type_annotation)
            .unwrap_or_else(|| self.interner.well_known().error);
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

    /// Infer a binary expression (M7 condition support). Descends into both operands
    /// for their side effects, then returns `boolean` for a comparison/equality
    /// operator (`===`, `!==`, `<`, …) and the error type for any other operator
    /// (arithmetic/bitwise are out of the subset; never an assignment source here).
    fn infer_binary(&mut self, scope: ScopeId, binary: &BinaryExpression<'_>) -> (TypeId, Span) {
        let wk = self.interner.well_known();
        let span = Span::from_oxc(binary.span);
        self.infer_expr(scope, &binary.left);
        self.infer_expr(scope, &binary.right);
        let ty = if is_comparison_operator(binary.operator) {
            wk.boolean
        } else {
            wk.error
        };
        (ty, span)
    }

    /// Infer a logical expression (`&&`/`||`/`??`, M7 condition support). Both operands
    /// are walked for side effects; the result type is the error type — `&&`/`||`
    /// condition narrowing is deferred (mvp-plan, README "Deferred checks"), so a
    /// logical expression is treated as an unrecognized guard (it narrows nothing).
    fn infer_logical(&mut self, scope: ScopeId, logical: &LogicalExpression<'_>) -> (TypeId, Span) {
        let wk = self.interner.well_known();
        let span = Span::from_oxc(logical.span);
        self.infer_expr(scope, &logical.left);
        self.infer_expr(scope, &logical.right);
        (wk.error, span)
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
        let context = context.map(|ctx| contextual_literal_target(self.interner.store(), ctx));
        // M31: peel an intersection context to its merged apparent object so fresh
        // literals shape against the merged member set, including nested cases.
        let context = context.map(|ctx| self.intersection_apparent_object(ctx).unwrap_or(ctx));
        match (init, context) {
            (Expression::ParenthesizedExpression(paren), ctx) => {
                self.infer_initializer(scope, &paren.expression, ctx)
            }
            (Expression::ObjectExpression(obj), Some(ctx))
                if self.interner.store().object_type(ctx).is_some() =>
            {
                let id = self.infer_object_literal_with_context(scope, obj, ctx);
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
        if let (true, Expression::ArrowFunctionExpression(arrow)) = (use_contextual_arrow, expr) {
            let diagnostics_len = self.diagnostics.len();
            let obligation_len = self.obligations.len();
            let override_len = self.override_checks.len();
            let decl_types = (!retain_contextual_arrow_checks).then(|| self.decl_types.clone());
            let contextual = self.infer_contextual_arrow(scope, arrow, context);
            if !retain_contextual_arrow_checks {
                self.diagnostics.truncate(diagnostics_len);
                self.obligations.truncate(obligation_len);
                self.override_checks.truncate(override_len);
                if let Some(decl_types) = decl_types {
                    self.decl_types = decl_types;
                }
            }
            return contextual.map(|ty| (ty, raw.1)).unwrap_or(raw);
        }

        if !self.context_can_shape_fresh_literal(expr, context) {
            return raw;
        }

        let diagnostic_len = self.diagnostics.len();
        let obligation_len = self.obligations.len();
        let override_len = self.override_checks.len();
        let contextual = self.infer_initializer(scope, expr, Some(context));
        self.diagnostics.truncate(diagnostic_len);
        self.obligations.truncate(obligation_len);
        self.override_checks.truncate(override_len);
        contextual.unwrap_or(raw)
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
            _ => false,
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
    ) -> TypeId {
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

        self.interner.intern_object(ObjectType {
            properties,
            ..Default::default()
        })
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
        // Snapshot the target tuple's element types up front (immutable borrow) so the
        // recursive `infer_initializer` below can take `&mut pass`. Element *i*'s context
        // is the target tuple's element *i*, when present.
        let context_elements: Vec<TypeId> = self
            .interner
            .store()
            .tuple_type(context)
            .map(|t| t.elements.clone())
            .unwrap_or_default();

        let mut elements: Vec<TypeId> = Vec::with_capacity(array.elements.len());
        for (index, element) in array.elements.iter().enumerate() {
            // Spread (`...xs`) / elision (a hole) are out of subset — the helper records
            // the skipped child slot; only a plain expression contributes a position.
            let Some(expr) = self.array_element_expr(element) else {
                continue;
            };
            // Contextually type this position against the target tuple's element *i* (if
            // any), so a nested array-literal-in-tuple-position becomes a tuple too.
            let elem_context = context_elements.get(index).copied();
            let Some((elem_ty, _)) = self.infer_initializer(scope, expr, elem_context) else {
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
        let wk = self.interner.well_known();
        let (base_ty, _) = self.infer_expr(scope, &member.object)?;
        let span = Span::from_oxc(member.span);

        // Walk the index expression for its side effects (reference resolution, nested
        // diagnostics), capturing its type for index-signature resolution (M19).
        let key_ty = self.infer_expr(scope, &member.expression).map(|(ty, _)| ty);

        if base_ty == wk.any || base_ty == wk.error {
            return Some((wk.error, span));
        }

        // M24 (review F2): element access resolves through the base's **apparent type**
        // — `t["x"]` with `T extends { x: number }` and `t[0]` with `T extends number[]`
        // read through the constraint. For a non-parameter base this is the identity.
        let base_ty = self.apparent_type(base_ty);
        let base_ty = self
            .interner
            .store()
            .readonly_operand(base_ty)
            .unwrap_or(base_ty);

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
            return Some((
                self.object_element_access(base_ty, &member.expression, key_ty),
                span,
            ));
        }

        // Non-array/non-tuple/non-object base: out of scope (no diagnostic, no crash) →
        // error type.
        Some((wk.error, span))
    }

    /// Resolve M19 object element access: named string-literal property, then number
    /// index, then string index, else the error type without diagnostics.
    fn object_element_access(
        &mut self,
        base_ty: TypeId,
        key_expr: &Expression<'_>,
        key_ty: Option<TypeId>,
    ) -> TypeId {
        let wk = self.interner.well_known();
        let store = self.interner.store();
        let Some(obj) = store.object_type(base_ty) else {
            return wk.error;
        };

        // 1. A string-literal key naming a known property → that property's type.
        if let Some(name) = string_literal_key(key_expr) {
            if let Some(prop) = obj.property(&name) {
                return prop.ty;
            }
        }

        // 2. A number-typed key + a number index signature → the number value type.
        let key_is_number = key_ty.is_some_and(|k| is_number_keyed(store, k));
        if key_is_number {
            if let Some(value) = obj.number_index {
                return value;
            }
        }

        // 3. Otherwise a string index signature accepts the key → the string value type.
        if let Some(value) = obj.string_index {
            return value;
        }

        // 4. Out of subset → error type (no diagnostic, no crash).
        wk.error
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

    /// Intern the merged apparent object for an intersection (M31). Properties are
    /// unioned by name, duplicate property/index types are intersected, optional
    /// requires all contributors, and readonly/accessor wins if any contributor has it.
    pub(in crate::check::checker) fn intersection_apparent_object(
        &mut self,
        ty: TypeId,
    ) -> Option<TypeId> {
        if self.interner.store().tag(ty) != TypeTag::Intersection {
            return None;
        }
        let members: Vec<TypeId> = self.interner.store().intersection_members(ty)?.to_vec();

        // Snapshot each object member's properties + index signatures (owned) before any
        // mutable interning borrow. A constrained-param member resolves through its
        // apparent type first, mirroring the union member-access handling.
        let mut snapshots: Vec<(Vec<PropertyType>, Option<TypeId>, Option<TypeId>)> = Vec::new();
        for &member in &members {
            let member = self.apparent_type(member);
            if let Some(obj) = self.interner.store().object_type(member) {
                snapshots.push((obj.properties.clone(), obj.string_index, obj.number_index));
            }
        }
        if snapshots.is_empty() {
            return None;
        }

        // Accumulate per property name: the base member (first seen, for its
        // visibility/origin), the contributing value types, and the merged flags.
        struct Acc {
            base: PropertyType,
            tys: Vec<TypeId>,
            all_optional: bool,
            any_readonly: bool,
            any_accessor: bool,
        }
        let mut order: Vec<String> = Vec::new();
        let mut props: FxHashMap<String, Acc> = FxHashMap::default();
        let mut string_index_values: Vec<TypeId> = Vec::new();
        let mut number_index_values: Vec<TypeId> = Vec::new();

        for (member_props, string_index, number_index) in snapshots {
            for prop in member_props {
                match props.get_mut(&prop.name) {
                    Some(acc) => {
                        acc.tys.push(prop.ty);
                        acc.all_optional &= prop.optional;
                        acc.any_readonly |= prop.readonly;
                        acc.any_accessor |= prop.is_accessor;
                    }
                    None => {
                        order.push(prop.name.clone());
                        props.insert(
                            prop.name.clone(),
                            Acc {
                                all_optional: prop.optional,
                                any_readonly: prop.readonly,
                                any_accessor: prop.is_accessor,
                                tys: vec![prop.ty],
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
            properties.push(PropertyType {
                ty,
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

        Some(self.interner.intern_object(ObjectType {
            properties,
            string_index,
            number_index,
            ..Default::default()
        }))
    }

    /// Infer `obj.prop`. Missing properties emit `TK2339`; union bases require the
    /// property on every member and union the member property types.
    fn infer_member_access(
        &mut self,
        scope: ScopeId,
        member: &StaticMemberExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();
        let (base_ty, _) = self.infer_expr(scope, &member.object)?;
        let prop_name = member.property.name.as_str();
        let prop_span = Span::from_oxc(member.property.span);

        if base_ty == wk.any || base_ty == wk.error {
            return Some((wk.error, prop_span));
        }

        // Resolve members through apparent/merged types, but keep `base_ty` for
        // `TK2339` rendering so missing constrained-parameter members still name `T`.
        let lookup_ty = self.apparent_type(base_ty);
        let lookup_ty = self
            .intersection_apparent_object(lookup_ty)
            .unwrap_or(lookup_ty);

        // Union base (M4): the property must exist on every member; its type is the
        // union of the per-member property types.
        if self.interner.store().tag(lookup_ty) == TypeTag::Union {
            return Some((
                self.union_member_access(lookup_ty, prop_name, prop_span),
                prop_span,
            ));
        }

        // Array base (M17): only `length` is synthesized (→ `number`). Every other array
        // member (`push`, `map`, `filter`, …) needs `lib.d.ts`, so it is deferred →
        // `TK2339` (property does not exist), with the array type rendered in the message
        // (`number[]`). The access yields the error type on the missing path (no cascade).
        if self.interner.store().tag(lookup_ty) == TypeTag::Array {
            if prop_name == "length" {
                return Some((wk.number, prop_span));
            }
            let tgt = render_type(self.interner.store(), lookup_ty, /* widen */ false);
            self.diagnostics.push(Diagnostic::property_does_not_exist(
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
                // Not on the type. For a class value base, this also covers
                // `C.instanceMember`; for an instance base, `instance.staticMember` —
                // both are `TK2339`, since each member lives on the other side.
                let tgt = render_type(self.interner.store(), base_ty, /* widen */ false);
                self.diagnostics.push(Diagnostic::property_does_not_exist(
                    prop_span, prop_name, &tgt,
                ));
                Some((wk.error, prop_span))
            }
        }
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
            let store = self.interner.store();
            if member == wk.any || member == wk.error {
                member_prop_types.push(wk.error);
                continue;
            }
            // M24 (audit): a union MEMBER that is a constrained type parameter resolves
            // through its apparent type too (`(T | U).x` with both constrained to
            // `HasX`), else the lookup would falsely report `TK2339`.
            let member = self.apparent_type(member);
            match store
                .object_type(member)
                .and_then(|o| o.property(prop_name))
            {
                Some(prop) => member_prop_types.push(prop.ty),
                // Missing on this member: the property does not exist on the union.
                None => {
                    let tgt = render_type(self.interner.store(), union_ty, /* widen */ false);
                    self.diagnostics.push(Diagnostic::property_does_not_exist(
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

/// Read a **string-literal** key from an element-access index expression
/// (`obj["a"]`), or `None` for any non-string-literal index. Used by object element
/// access (M19) to resolve a literal key to a named property.
fn string_literal_key(expr: &Expression<'_>) -> Option<String> {
    match expr {
        Expression::StringLiteral(lit) => Some(lit.value.to_string()),
        _ => None,
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
        if is_contextual_literal_shape(store, member) {
            if shape.replace(member).is_some() {
                return ty;
            }
        } else if store.intrinsic_kind(member) != Some(IntrinsicKind::Undefined) {
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

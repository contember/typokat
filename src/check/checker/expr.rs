//! expr module (extracted from checker/mod.rs).

use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::flow::FlowNodeId;
use crate::diagnostics::{render_type, Diagnostic};
use crate::span::Span;
use crate::types::repr::{
    IntrinsicKind, LiteralValue, ObjectType, PropertyType, TypeParamId, TypeTag,
};
use crate::types::store::{Store, TypeId};
use rustc_hash::FxHashSet;
use oxc_ast::ast::{
    ArrayExpression, BinaryExpression, BinaryOperator, ComputedMemberExpression, Expression, LogicalExpression, ObjectExpression, ObjectPropertyKind,
    StaticMemberExpression, UnaryExpression,
    UnaryOperator,
};
use super::context::*;
use super::calls::widen;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Infer the type of an expression in `scope`, returning `(TypeId, span)`. The
    /// span is the expression's own span — the primary span for any diagnostic on it.
    /// Returns `None` for expression shapes outside the subset (those positions are
    /// simply not checked, matching M0 leniency).
    pub(in crate::check::checker) fn infer_expr(&mut self, scope: ScopeId, expr: &Expression<'_>) -> Option<(TypeId, Span)> {
        let well_known = self.interner.well_known();
        match expr {
            Expression::NumericLiteral(lit) => {
                let id = self.interner.intern_literal(LiteralValue::Number(lit.value));
                Some((id, Span::from_oxc(lit.span)))
            }
            Expression::StringLiteral(lit) => {
                let id = self
                    .interner
                    .intern_literal(LiteralValue::String(lit.value.to_string()));
                Some((id, Span::from_oxc(lit.span)))
            }
            Expression::BooleanLiteral(lit) => {
                let id = self.interner.intern_literal(LiteralValue::Boolean(lit.value));
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
                // A generic function *expression*'s type parameters are scoped to its
                // body (handled inside `infer_function`); the param ids are not
                // registered for a call site (only a named generic `function`
                // declaration is callable with explicit type args in the M9 subset —
                // inference is M10).
                let (id, _params) = self.infer_function(scope, func);
                Some((id, Span::from_oxc(func.span)))
            }
            Expression::ArrowFunctionExpression(arrow) => {
                let id = self.infer_arrow(scope, arrow);
                Some((id, Span::from_oxc(arrow.span)))
            }
            Expression::ParenthesizedExpression(paren) => self.infer_expr(scope, &paren.expression),
            Expression::Identifier(ident) => {
                let span = Span::from_oxc(ident.span);
                // The `undefined` keyword parses as an identifier reference.
                if ident.name.as_str() == "undefined" {
                    return Some((well_known.undefined, span));
                }
                match self.binder.graph.resolve(scope, ident.name.as_str()) {
                    Some(symbol_id) => match self.binder.symbols.get(symbol_id) {
                        Some(symbol) if symbol.value.is_some() => {
                            Some((self.resolve_identifier_type(symbol_id, ident.span.start), span))
                        }
                        _ => {
                            self.diagnostics
                                .push(Diagnostic::cannot_find_name(span, ident.name.as_str()));
                            Some((well_known.error, span))
                        }
                    },
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
            _ => None,
        }
    }

    /// Resolve a value symbol's *narrowed* type at a reference (M23). The reference's
    /// flow node (recorded by the pre-pass, keyed by its span start) drives a backward
    /// walk over the flow-node CFG ([`resolve_narrowed_type`](Pass::resolve_narrowed_type));
    /// a reference the pre-pass did not cover defaults to [`FlowNodeId::START`] — the
    /// **declared** type (the sound over-report), so partial coverage never
    /// under-reports.
    ///
    /// This single seam is where control-flow narrowing takes effect: every identifier
    /// reference — assignment sources, member-access bases, returned expressions, call
    /// arguments — resolves through [`infer_expr`], which calls this. Keying on the
    /// `SymbolId` (not the name, not the `DeclId` of an unrelated binding) is the
    /// soundness guarantee that narrowing applies to exactly the guarded binding.
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

    /// Infer a unary expression (M7 condition support). Descends into the operand for
    /// its side effects, then returns a coarse result type by operator:
    ///
    ///  - `typeof x` → `string` (the runtime tag string),
    ///  - `!x` → `boolean`,
    ///  - everything else (`+`/`-`/`~`/`void`/`delete`) is out of the subset → the
    ///    error type (no diagnostic; never an assignment source in the corpus).
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
                continue;
            };
            let Some(name) = prop.key.static_name() else {
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

    /// Infer the type of an array literal `[e1, e2, …]` (M17): `(<elem>)[]` where the
    /// element type is the `union(...)` of the **widened** per-element types
    /// (`[1, 2, 3]` → `number[]`, `[1, "x"]` → `(number | string)[]`). An **empty**
    /// literal `[]` has an empty element union → `never`, giving `never[]` — which is
    /// assignable to any `T[]` (the bottom element under covariance), exactly as a fresh
    /// empty array should be.
    ///
    /// Elements are **widened** (a literal `1` → `number`) before the union, matching the
    /// object-literal member inference and tsc's array-literal element typing; a literal
    /// element type would otherwise make `[1, 2, 3]` infer `(1 | 2 | 3)[]` and reject the
    /// `number[]` annotation. A **spread** or **elision** element is out of the M17 subset
    /// — it is skipped (it contributes no element type), so a literal containing one is
    /// not mis-typed; spread support lands with the array-methods milestone.
    fn infer_array_literal(&mut self, scope: ScopeId, array: &ArrayExpression<'_>) -> TypeId {
        let mut element_types: Vec<TypeId> = Vec::with_capacity(array.elements.len());
        for element in &array.elements {
            // Spread (`...xs`) / elision (a hole) are out of subset — skip them. Only a
            // plain expression element contributes to the element type.
            let Some(expr) = element.as_expression() else {
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
    ) -> (TypeId, Span) {
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
            Expression::ObjectExpression(_) => self.interner.store().object_type(context).is_some(),
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
                continue;
            };
            let Some(name) = prop.key.static_name() else {
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
            let Some(expr) = element.as_expression() else {
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

    /// Type an array literal **positionally as a tuple** for an M18 tuple context
    /// `context` (the target tuple): the result is `[T0, T1, …]` where `Ti` is the
    /// (widened) type of the literal's *i*-th element. This is what makes `const t:
    /// [number, string] = [1, "x"]` check position-by-position against the target tuple
    /// (and what makes a wrong-length literal — `[1]`, `[1, "x", 2]` — a length mismatch
    /// rather than an array-vs-tuple mismatch).
    ///
    /// Contextual typing is **recursive**: element *i* is itself typed against the target
    /// tuple's element type *i* (via [`infer_initializer`]), so a **nested** array literal
    /// in a nested tuple position is typed as a tuple too (`[[number], string]` accepts
    /// `[[1], "x"]`). When the literal is longer than the target tuple (a length error
    /// reported by the relation), the surplus elements have no contextual type and infer
    /// normally — they cannot pass the length check anyway.
    ///
    /// Elements are kept at their **inferred (un-widened) type** — a literal `1` stays
    /// `1`, not `number`. This is the difference from the M17 *array*-literal inference
    /// (which widens before the union): here the tuple is used **only** for the
    /// assignability check against the target tuple, and the existing relation widens
    /// literal→base at each position *when the target permits it* (`1` relates to both `1`
    /// and `number`), so keeping the literal type is what makes a **literal-type** tuple
    /// target accept a matching literal (`[1, 2] = [1, 2]` ok) while still rejecting a
    /// non-matching one (`[1, 2] = [1, 3]` → `3` not assignable to `2`) and a base-type
    /// target (`[number, string] = [1, "x"]` ok via the relation's literal→base
    /// widening). The variable still takes the **annotation's** type, so the un-widened
    /// source tuple is never observed elsewhere. A **spread** or **elision** element is
    /// out of the M18 subset; it is skipped (contributes no element), so a literal
    /// containing one is not mis-shaped — exactly as the M17 array-literal inference
    /// treats it.
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
            // Spread (`...xs`) / elision (a hole) are out of subset — skip (matching
            // `infer_array_literal`); only a plain expression contributes a position.
            let Some(expr) = element.as_expression() else {
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

    /// Infer the type of an element access `a[i]` (M17/M18). When the base `a` is an
    /// **array** (M17), the result is its **element type** — M17 does not strict-check
    /// the index, so **any** index expression yields the element type (`nums[0]`,
    /// `nums[i]` alike). When the base is a **tuple** (M18), the result is the element at
    /// the **literal** numeric index (`t[0]` → position 0's type, `t[1]` → position 1's),
    /// read off the constant index. The index is still inferred for its side effects
    /// (resolving references / emitting any `TK2304` inside it).
    ///
    /// A base that is `any`/error yields the error type (suppressing cascade). For a
    /// tuple base, an **out-of-range** literal index or a **non-literal** index is out of
    /// the M18 subset → the error type (no diagnostic, no crash; the fixtures use only
    /// in-range literal indices). A base that is **not** an array or tuple is out of
    /// scope: no diagnostic is emitted and the result is the error type, so nothing
    /// downstream over-reports and the checker never crashes.
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

        // Array base (M17): the result is the element type (any index).
        if let Some(array) = self.interner.store().array_type(base_ty) {
            return Some((array.element, span));
        }

        // Tuple base (M18): index by the **literal** numeric index. A non-literal index
        // or one out of range is out of subset → error type (no diagnostic, no crash).
        if self.interner.store().tag(base_ty) == TypeTag::Tuple {
            let element = literal_index(&member.expression)
                .and_then(|i| self.interner.store().tuple_type(base_ty)?.elements.get(i).copied());
            return Some((element.unwrap_or(wk.error), span));
        }

        // Object base (M19): resolve `obj[key]` through named properties / index sigs.
        if self.interner.store().tag(base_ty) == TypeTag::Object {
            return Some((self.object_element_access(base_ty, &member.expression, key_ty), span));
        }

        // Non-array/non-tuple/non-object base: out of scope (no diagnostic, no crash) →
        // error type.
        Some((wk.error, span))
    }

    /// Resolve the result type of an element access `obj[key]` on an **object** base
    /// (M19). In order:
    ///
    ///  1. a **string-literal** key that names a known property → that property's type
    ///     (`dict["a"]` where `a` is declared);
    ///  2. a **`number`-typed** key (a numeric literal, or a `number`-typed variable)
    ///     when the object has a number index signature → the number value type;
    ///  3. otherwise, when the object has a **string** index signature → the string
    ///     value type (covers a dynamic string key `dict[key]` and any other key under a
    ///     string index);
    ///  4. otherwise the error type (no diagnostic — element access on an object outside
    ///     these cases is out of the M19 subset, matching the array/tuple leniency).
    ///
    /// `key_ty` is the (already-inferred) type of the index expression, or `None` if it
    /// was out of subset.
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

    /// The **apparent type** of `ty` (M24): for a **constrained type parameter**, its
    /// `extends` constraint — resolved **transitively** through a constraint chain
    /// (`U extends T`, `T extends HasX` → `HasX`) so a structural operand sees the
    /// ultimate concrete bound. Any other type is its own apparent type (returned
    /// unchanged), so non-generic operands are untouched. A **cycle** (`T extends T`, or
    /// a mutual `T extends U` / `U extends T`) terminates: the visited-set guard stops
    /// at the first repeat, returning the last parameter in the chain (member access on
    /// it then reports `TK2339`, the safe over-report). An **unconstrained** type
    /// parameter is also returned unchanged, so `t.x` on a bare `T` stays `TK2339` on
    /// `'T'`.
    ///
    /// This is the **shared Pass-level resolver** (review F1–F3): EVERY structural
    /// consumer of an operand type routes through it — member reads
    /// ([`infer_member_access`]), member writes ([`check_member_assignment`]), element
    /// access ([`infer_element_access`]), and callable/construct signature lookup
    /// ([`callable_signature`]/[`construct_signature`]) — so a constrained `T` behaves
    /// as its constraint everywhere, not just on reads.
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

    /// Infer the type of a member access `obj.prop` in `scope`. A missing property is
    /// `TK2339` and yields the error type (no cascade); an `any`/error base yields the
    /// error type.
    ///
    /// M4 adds the **union** base: `u.p` where `u` is a union requires `p` on *every*
    /// member; the result type is the `union(...)` of the per-member property types.
    /// If any member lacks the property, it is `TK2339` on the union as a whole (and
    /// the result is the error type, suppressing cascade).
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

        // M24 — apparent type: a **constrained type parameter** exposes its constraint's
        // members, resolving transitively through a constraint chain (`U extends T`,
        // `T extends HasX`) and terminating on a cycle ([`apparent_type`]). The member is
        // resolved against this apparent type, but `base_ty` is kept for the `TK2339`
        // render so a missing member still names the parameter (`'T'`). For a non-parameter
        // base `lookup_ty == base_ty`, so M0–M23 behaviour is unchanged.
        let lookup_ty = self.apparent_type(base_ty);

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
            self.diagnostics
                .push(Diagnostic::property_does_not_exist(prop_span, prop_name, &tgt));
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
                // M13 access control: a `private`/`protected` member is reachable only
                // from the right class context (`current_class`). The member is present
                // on the type, so an access violation is `TK2341`/`TK2445` (NOT a
                // property-does-not-exist), and the access still yields the member's real
                // type (matching tsc — access control does not change the type, so there
                // is no cascade).
                self.check_member_access_control(prop_name,
                    prop_span,
                    visibility,
                    declaring_class,
                );
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

    /// Resolve `union.prop` (M4): collect each member's type for `prop`, requiring it
    /// on **every** member. The result is the `union(...)` of those per-member types
    /// (canonicalized by the interner). If any member lacks the property, emit a
    /// single `TK2339` against the whole union and return the error type.
    ///
    /// A member that is itself `any`/error contributes the error type (its `prop` is
    /// assumed to exist). A member that is neither an object nor `any`/error has no
    /// known property in the MVP subset, so it counts as "missing" → `TK2339`.
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
            match store.object_type(member).and_then(|o| o.property(prop_name)) {
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
    matches!(store.tag(ty), TypeTag::Object | TypeTag::Array | TypeTag::Tuple)
}

fn is_numeric_property_name(name: &str) -> bool {
    name.parse::<f64>().map(|n| n.is_finite()).unwrap_or(false)
}

/// Read a **non-negative integer literal** index from an element-access index
/// expression (`t[0]`, `t[2]`), as a `usize` array offset, or `None` for any
/// non-literal / non-integer / negative / out-of-`usize` index. Used by tuple
/// element access (M18) to resolve `t[k]` to the *k*-th element type. A `NumericLiteral`
/// whose value is a whole, finite, in-range, non-negative number maps to that index;
/// everything else (a variable, a fractional/negative literal, `NaN`/∞) is `None`
/// (out of subset).
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

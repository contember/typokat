//! Flow-graph construction for expressions, assignments, and function boundaries
//! (extracted from flowgraph.rs).

use crate::binder::bind::{ResolvedValueKind, ValueResolution};
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::flow::{FlowNode, FlowNodeId};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, Class, ClassElement,
    ConditionalExpression, Expression, Function, LogicalExpression, LogicalOperator,
    ObjectPropertyKind,
};

use super::super::context::*;

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Build the flow for an expression: record each identifier reference at the
    /// current cursor, apply `&&`/`||`/ternary branch narrowing to their RHS/arms,
    /// handle an inline assignment, and recurse into nested constructs (calls, member
    /// accesses, literals, and nested function/arrow bodies at their own `START`).
    pub(super) fn build_flow_expr(&mut self, scope: ScopeId, expr: &Expression<'_>) {
        match expr {
            Expression::Identifier(ident) => {
                self.reference_flow
                    .insert((self.current_module, ident.span.start), self.flow_cursor);
            }
            Expression::ParenthesizedExpression(paren) => {
                self.build_flow_expr(scope, &paren.expression)
            }
            Expression::TSAsExpression(assertion) => {
                self.build_flow_expr(scope, &assertion.expression)
            }
            Expression::TSTypeAssertion(assertion) => {
                self.build_flow_expr(scope, &assertion.expression)
            }
            Expression::LogicalExpression(logical) => self.build_flow_logical(scope, logical),
            Expression::ConditionalExpression(cond) => self.build_flow_conditional(scope, cond),
            Expression::AssignmentExpression(assign) => self.build_flow_assignment(scope, assign),
            Expression::UnaryExpression(unary) => self.build_flow_expr(scope, &unary.argument),
            Expression::BinaryExpression(binary) => {
                self.build_flow_expr(scope, &binary.left);
                self.build_flow_expr(scope, &binary.right);
            }
            Expression::CallExpression(call) => {
                self.build_flow_expr(scope, &call.callee);
                for arg in &call.arguments {
                    if let Some(arg_expr) = arg.as_expression() {
                        self.build_flow_expr(scope, arg_expr);
                    }
                }
            }
            Expression::NewExpression(new_expr) => {
                self.build_flow_expr(scope, &new_expr.callee);
                for arg in &new_expr.arguments {
                    if let Some(arg_expr) = arg.as_expression() {
                        self.build_flow_expr(scope, arg_expr);
                    }
                }
            }
            Expression::StaticMemberExpression(member) => {
                self.build_flow_expr(scope, &member.object)
            }
            Expression::ComputedMemberExpression(member) => {
                self.build_flow_expr(scope, &member.object);
                self.build_flow_expr(scope, &member.expression);
            }
            Expression::ObjectExpression(obj) => {
                for member in &obj.properties {
                    if let ObjectPropertyKind::ObjectProperty(prop) = member {
                        self.build_flow_expr(scope, &prop.value);
                    }
                }
            }
            Expression::ArrayExpression(array) => {
                for element in &array.elements {
                    if let Some(elem) = element.as_expression() {
                        self.build_flow_expr(scope, elem);
                    }
                }
            }
            // Nested function/arrow: a fresh flow context (function boundary).
            Expression::FunctionExpression(func) => {
                self.build_flow_fn_body(scope, func);
            }
            Expression::ArrowFunctionExpression(arrow) => {
                self.build_flow_arrow_body(scope, arrow);
            }
            // A sequence `(a, b, c)` (backlog 53): walk operands left-to-right so an
            // inner assignment advances the cursor and survives the expression.
            Expression::SequenceExpression(seq) => {
                for operand in &seq.expressions {
                    self.build_flow_expr(scope, operand);
                }
            }
            // Literals, `this`, and other shapes carry no narrowable reference.
            _ => {}
        }
    }

    /// Build `&&`/`||` flow: RHS runs under the appropriate left-guard branch, then
    /// joins with the skipped branch so RHS assignments survive but guard narrowing
    /// does not persist past the expression.
    fn build_flow_logical(&mut self, scope: ScopeId, logical: &LogicalExpression<'_>) {
        self.build_flow_expr(scope, &logical.left);
        let fact = self.analyze_guard(scope, &logical.left);
        let pre = self.flow_cursor;
        let (rhs_flow, skip_flow) = match logical.operator {
            LogicalOperator::And => (
                self.flow_condition(pre, &fact, true),
                self.flow_condition(pre, &fact, false),
            ),
            LogicalOperator::Or => (
                self.flow_condition(pre, &fact, false),
                self.flow_condition(pre, &fact, true),
            ),
            // `??` narrows on nullishness only — out of the recognized-guard subset.
            LogicalOperator::Coalesce => (pre, pre),
        };
        self.flow_cursor = rhs_flow;
        self.build_flow_expr(scope, &logical.right);
        let rhs_end = self.flow_cursor;
        self.flow_cursor = self.flow_join(vec![skip_flow, rhs_end]);
    }

    /// A ternary `test ? a : b` (M23): the consequent under the test's true branch,
    /// the alternate under its false branch. The post-expression cursor **joins** the
    /// two arm ends, so an assignment in either arm survives (backlog 53); the arm
    /// narrowing does not persist (the join carries both senses of the test).
    fn build_flow_conditional(&mut self, scope: ScopeId, cond: &ConditionalExpression<'_>) {
        self.build_flow_expr(scope, &cond.test);
        let fact = self.analyze_guard(scope, &cond.test);
        let pre = self.flow_cursor;
        let then_flow = self.flow_condition(pre, &fact, true);
        let else_flow = self.flow_condition(pre, &fact, false);

        self.flow_cursor = then_flow;
        self.build_flow_expr(scope, &cond.consequent);
        let then_end = self.flow_cursor;

        self.flow_cursor = else_flow;
        self.build_flow_expr(scope, &cond.alternate);
        let else_end = self.flow_cursor;

        self.flow_cursor = self.flow_join(vec![then_end, else_end]);
    }

    /// Build assignment flow. RHS sees pre-assignment flow; simple identifier `=`
    /// narrows to the assigned value, while compound/complex/destructuring targets
    /// reset bound identifiers to declared types. Member targets bind no symbol.
    fn build_flow_assignment(&mut self, scope: ScopeId, assign: &AssignmentExpression<'_>) {
        self.build_flow_expr(scope, &assign.right);
        if self.flow_cursor == FlowNodeId::UNREACHABLE {
            return;
        }

        let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
            // Array/object pattern: reset each bound identifier. A member target
            // resets nothing (no symbol is bound — member paths are not narrowed).
            self.reset_pattern_targets(scope, &assign.left);
            return;
        };
        let Some(symbol) = self.assignable_symbol(scope, target.name.as_str()) else {
            return;
        };

        let assigned = if assign.operator == AssignmentOperator::Assign {
            self.flow_assigned(&assign.right)
        } else {
            // Compound (`+=`, `||=`, …): the result type is out of the value subset —
            // reset to the declared type so a later reference is not over-narrowed.
            None
        };
        let node = self.new_flow(FlowNode::Assignment {
            symbol,
            assigned,
            antecedent: self.flow_cursor,
        });
        self.flow_cursor = node;
    }

    /// Resolve an assignment-target name to its narrowable value symbol, or `None`
    /// for an unresolvable / non-value binding.
    fn assignable_symbol(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        match self.binder.resolve_value_binding(scope, name) {
            ValueResolution::Resolved {
                symbol,
                kind: ResolvedValueKind::Ordinary,
            } => Some(symbol),
            ValueResolution::Resolved {
                kind: ResolvedValueKind::StandaloneNamespace { .. },
                ..
            }
            | ValueResolution::TypeOnlyNamespace { .. }
            | ValueResolution::Missing => None,
        }
    }

    /// Emit a reset-to-declared assignment node for `symbol` (the value changed in a
    /// way the subset cannot type — the sound over-report).
    fn reset_symbol(&mut self, symbol: SymbolId) {
        let node = self.new_flow(FlowNode::Assignment {
            symbol,
            assigned: None,
            antecedent: self.flow_cursor,
        });
        self.flow_cursor = node;
    }

    /// Reset every identifier bound by a destructuring assignment target; nested
    /// patterns recurse, member targets bind no symbol, and TS-wrapper targets are
    /// out of subset.
    fn reset_pattern_targets(&mut self, scope: ScopeId, target: &AssignmentTarget<'_>) {
        match target {
            AssignmentTarget::AssignmentTargetIdentifier(ident) => {
                if let Some(symbol) = self.assignable_symbol(scope, ident.name.as_str()) {
                    self.reset_symbol(symbol);
                }
            }
            AssignmentTarget::ArrayAssignmentTarget(array) => {
                for element in array.elements.iter().flatten() {
                    self.reset_maybe_default(scope, element);
                }
                if let Some(rest) = &array.rest {
                    self.reset_pattern_targets(scope, &rest.target);
                }
            }
            AssignmentTarget::ObjectAssignmentTarget(object) => {
                for property in &object.properties {
                    match property {
                        // Shorthand `({ x } = …)` / with default `({ x = d } = …)`.
                        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(prop) => {
                            if let Some(symbol) =
                                self.assignable_symbol(scope, prop.binding.name.as_str())
                            {
                                self.reset_symbol(symbol);
                            }
                        }
                        // Renamed `({ a: x } = …)` — the *binding* side is assigned.
                        AssignmentTargetProperty::AssignmentTargetPropertyProperty(prop) => {
                            self.reset_maybe_default(scope, &prop.binding);
                        }
                    }
                }
                if let Some(rest) = &object.rest {
                    self.reset_pattern_targets(scope, &rest.target);
                }
            }
            // Member targets: no symbol binds. TS wrappers: out of subset (skipped).
            _ => {}
        }
    }

    /// Unwrap a pattern element's optional default (`[x = d] = …`) and reset its
    /// bound target(s).
    fn reset_maybe_default(&mut self, scope: ScopeId, element: &AssignmentTargetMaybeDefault<'_>) {
        match element {
            AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(with_default) => {
                self.reset_pattern_targets(scope, &with_default.binding);
            }
            _ => {
                if let Some(target) = element.as_assignment_target() {
                    self.reset_pattern_targets(scope, target);
                }
            }
        }
    }

    pub(super) fn add_labeled_break(&mut self, name: &str, cursor: FlowNodeId) {
        if let Some(target) = self
            .label_targets
            .iter_mut()
            .rev()
            .find(|target| target.name == name)
        {
            target.breaks.push(cursor);
        }
    }

    pub(super) fn labeled_continue_target(&self, name: &str) -> Option<FlowNodeId> {
        self.label_targets
            .iter()
            .rev()
            .find(|target| target.name == name)
            .and_then(|target| target.continue_target)
    }

    /// The narrowed type a `x = <rhs>` assignment installs: a literal's **widened**
    /// base (`"s"` → `string`, `5` → `number`), `null`/`undefined` as-is, or `None`
    /// (reset to the declared type) for any richer RHS — the sound over-report, and
    /// enough for the corpus. Uses only well-known ids (no interning).
    fn flow_assigned(&self, rhs: &Expression<'_>) -> Option<TypeId> {
        let wk = self.interner.well_known();
        match rhs {
            Expression::StringLiteral(_) => Some(wk.string),
            Expression::NumericLiteral(_) => Some(wk.number),
            Expression::BooleanLiteral(_) => Some(wk.boolean),
            Expression::NullLiteral(_) => Some(wk.null),
            Expression::Identifier(ident) if ident.name.as_str() == "undefined" => {
                Some(wk.undefined)
            }
            Expression::ParenthesizedExpression(paren) => self.flow_assigned(&paren.expression),
            _ => None,
        }
    }

    /// Build a top-level `function` declaration's body at a fresh `START` (a function
    /// boundary; the narrowing does not cross it).
    pub(super) fn build_flow_function(&mut self, scope: ScopeId, func: &Function<'_>) {
        self.build_flow_fn_body(scope, func);
    }

    fn build_flow_fn_body(&mut self, enclosing: ScopeId, func: &Function<'_>) {
        let Some(body) = &func.body else {
            return;
        };
        let fn_scope = self
            .binder
            .fn_scopes
            .get(&(self.current_module, func.span.start))
            .copied()
            .unwrap_or(enclosing);
        self.build_flow_boundary(|pass| pass.build_flow_stmts(fn_scope, &body.statements));
    }

    fn build_flow_arrow_body(&mut self, enclosing: ScopeId, arrow: &ArrowFunctionExpression<'_>) {
        let fn_scope = self
            .binder
            .fn_scopes
            .get(&(self.current_module, arrow.span.start))
            .copied()
            .unwrap_or(enclosing);
        self.build_flow_boundary(|pass| {
            if let Some(body_expr) = arrow.get_expression() {
                pass.build_flow_expr(fn_scope, body_expr);
            } else {
                pass.build_flow_stmts(fn_scope, &arrow.body.statements);
            }
        });
    }

    /// Build the flow for a `class`'s member bodies (each method) and field
    /// initializers, mirroring `check_class_member_bodies`. Each is its own flow
    /// boundary.
    pub(super) fn build_flow_class(&mut self, scope: ScopeId, class: &Class<'_>) {
        for element in &class.body.body {
            match element {
                ClassElement::MethodDefinition(method) => {
                    self.build_flow_fn_body(scope, &method.value);
                }
                ClassElement::PropertyDefinition(prop) => {
                    if let Some(init) = &prop.value {
                        self.build_flow_boundary(|pass| pass.build_flow_expr(scope, init));
                    }
                }
                _ => {}
            }
        }
    }

    /// Run `build` at a fresh `START` with an empty loop stack, restoring the cursor +
    /// loop stack afterward — a function/initializer boundary (narrowing does not
    /// cross it, the documented closure divergence).
    fn build_flow_boundary(&mut self, build: impl FnOnce(&mut Self)) {
        let saved_cursor = self.flow_cursor;
        let saved_loops = std::mem::take(&mut self.flow_loops);
        let saved_breaks = std::mem::take(&mut self.break_targets);
        let saved_labels = std::mem::take(&mut self.label_targets);
        self.flow_cursor = FlowNodeId::START;
        build(self);
        self.flow_cursor = saved_cursor;
        self.flow_loops = saved_loops;
        self.break_targets = saved_breaks;
        self.label_targets = saved_labels;
    }
}

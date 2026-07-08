//! The flow-node CFG: construction (a pre-pass) + resolution (the backward walk).
//! This is the **single** narrowing model since M23 (architecture §5, backlog 07).
//!
//! ## Construction (the pre-pass)
//!
//! [`Pass::build_flow_graph`] walks the whole module — every function/method body
//! and the top level — **before** the check walk, maintaining a `flow_cursor` (the
//! flow node currently in effect) and recording, for each identifier reference, the
//! cursor at that point ([`Pass::reference_flow`], keyed by `(module scope, span)`).
//! It creates flow
//! nodes for assignments, condition branches (`if`/`switch`/`while`/`&&`/`||`/
//! ternary), branch joins, and loop labels. Running it as a pre-pass is what makes
//! **loop back edges complete** before any type is resolved (the check walk resolves
//! against the finished graph) — the ordering the backward walk needs. A function
//! body starts at [`FlowNodeId::START`] (function boundaries stay narrowing barriers,
//! the documented closure divergence). Declarations create **no** flow node (a
//! reference reads its declared type until a real `=` reassigns it — the M0–M22
//! behavior, preserved); only `x = …` assignments narrow.
//!
//! ## Resolution (the backward walk)
//!
//! [`Pass::resolve_narrowed_type`] walks backward from a reference's flow node
//! applying the flow-model-agnostic [`narrow`](crate::check::flow::narrow) ops,
//! memoized per `(flow node, symbol)`. It is **iterative** (an explicit stack) for
//! the non-loop graph so a deep `if`/statement nest cannot overflow the host stack;
//! a loop label is a bounded-recursion single-unroll fixpoint. The provisional loop
//! seed is **never** durably memoized (invariants §1 — a pre-loop narrow state cached
//! across a back edge is a dropped-error false negative).

use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::flow::{narrow, FlowNode, FlowNodeId};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, Class, ClassElement,
    ConditionalExpression, Declaration, Expression, Function, IfStatement, LogicalExpression,
    LogicalOperator, ObjectPropertyKind, Statement, SwitchStatement, WhileStatement,
};
use rustc_hash::FxHashMap;

use super::context::*;
use super::narrowing::{clause_terminates, GuardFact};

impl<'a, 'ast> Pass<'a, 'ast> {
    // =======================================================================
    // Construction — the flow-graph pre-pass.
    // =======================================================================

    /// Build the whole module's control-flow graph (the M23 pre-pass). Entry point:
    /// walk the top-level statements at [`FlowNodeId::START`], recursing into every
    /// function/method body (each reset to its own `START`). Populates the flow-node
    /// arena and [`Pass::reference_flow`].
    pub(in crate::check::checker) fn build_flow_graph(
        &mut self,
        module_scope: ScopeId,
        statements: &[Statement<'_>],
    ) {
        self.flow_cursor = FlowNodeId::START;
        self.build_flow_stmts(module_scope, statements);
    }

    fn build_flow_stmts(&mut self, scope: ScopeId, statements: &[Statement<'_>]) {
        for stmt in statements {
            self.build_flow_stmt(scope, stmt);
        }
    }

    fn build_flow_stmt(&mut self, scope: ScopeId, stmt: &Statement<'_>) {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                // A declaration does not narrow (the M0–M22 behavior): a reference
                // reads its declared type until a real `=` reassigns it. Only the
                // initializer's references / nested constructs are recorded.
                for declarator in &decl.declarations {
                    if let Some(init) = &declarator.init {
                        self.build_flow_expr(scope, init);
                    }
                }
            }
            Statement::FunctionDeclaration(func) => self.build_flow_function(scope, func),
            Statement::ClassDeclaration(class) => self.build_flow_class(scope, class),
            Statement::ExpressionStatement(expr_stmt) => {
                self.build_flow_expr(scope, &expr_stmt.expression);
            }
            Statement::ReturnStatement(ret) => {
                if let Some(arg) = &ret.argument {
                    self.build_flow_expr(scope, arg);
                }
                // Everything after an unconditional `return` is unreachable — an
                // all-unreachable join is what makes early-exit narrowing work.
                self.flow_cursor = FlowNodeId::UNREACHABLE;
            }
            Statement::ThrowStatement(throw) => {
                self.build_flow_expr(scope, &throw.argument);
                self.flow_cursor = FlowNodeId::UNREACHABLE;
            }
            Statement::IfStatement(if_stmt) => self.build_flow_if(scope, if_stmt),
            Statement::BlockStatement(block) => {
                let block_scope = self
                    .binder
                    .block_scopes
                    .get(&(self.current_module, block.span.start))
                    .copied()
                    .unwrap_or(scope);
                self.build_flow_stmts(block_scope, &block.body);
            }
            Statement::SwitchStatement(switch) => self.build_flow_switch(scope, switch),
            Statement::WhileStatement(while_stmt) => self.build_flow_while(scope, while_stmt),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(decl) = &export.declaration {
                    self.build_flow_declaration(scope, decl);
                }
            }
            Statement::BreakStatement(_) => {
                // The break carries the current (narrowed) state out to the nearest
                // breakable's exit join (loop or `switch`) — what un-narrows the
                // after-loop state (`breakTrap`) and carries a clause's assignments
                // out of a `switch` (backlog 53).
                let cursor = self.flow_cursor;
                if let Some(target) = self.break_targets.last_mut() {
                    target.push(cursor);
                }
                self.flow_cursor = FlowNodeId::UNREACHABLE;
            }
            Statement::ContinueStatement(_) => {
                // A `continue` is a back edge to the loop label (re-checks the
                // condition), not an exit edge.
                let cursor = self.flow_cursor;
                let label = self.flow_loops.last().map(|f| f.label);
                if let Some(label) = label {
                    self.add_back_edge(label, cursor);
                }
                self.flow_cursor = FlowNodeId::UNREACHABLE;
            }
            // Out of subset (`for`/`for-of`/`do-while`/`try`, …): not walked by the
            // check pass either, so no references are resolved inside them — a miss in
            // `reference_flow` falls back to the declared type (sound). Left un-built.
            _ => {}
        }
    }

    fn build_flow_declaration(&mut self, scope: ScopeId, decl: &Declaration<'_>) {
        match decl {
            Declaration::VariableDeclaration(var) => {
                for declarator in &var.declarations {
                    if let Some(init) = &declarator.init {
                        self.build_flow_expr(scope, init);
                    }
                }
            }
            Declaration::FunctionDeclaration(func) => self.build_flow_function(scope, func),
            Declaration::ClassDeclaration(class) => self.build_flow_class(scope, class),
            _ => {}
        }
    }

    /// Build the flow for an `if`/`else`: two condition nodes (the guard's positive /
    /// complementary sense), each branch under its node, then a join of the branch
    /// ends (unreachable ones excluded — an all-returning `if` collapses to its
    /// complement).
    fn build_flow_if(&mut self, scope: ScopeId, if_stmt: &IfStatement<'_>) {
        self.build_flow_expr(scope, &if_stmt.test);
        let fact = self.analyze_guard(scope, &if_stmt.test);
        let pre = self.flow_cursor;

        let cond_true = self.flow_condition(pre, &fact, true);
        let cond_false = self.flow_condition(pre, &fact, false);

        self.flow_cursor = cond_true;
        self.build_flow_stmt(scope, &if_stmt.consequent);
        let then_end = self.flow_cursor;

        self.flow_cursor = cond_false;
        let else_end = if let Some(alternate) = &if_stmt.alternate {
            self.build_flow_stmt(scope, alternate);
            self.flow_cursor
        } else {
            cond_false
        };

        self.flow_cursor = self.flow_join(vec![then_end, else_end]);
    }

    /// Build the flow for a `switch` (backlog 53): each clause body starts from its
    /// per-case discriminant narrowing (`x.prop === label`) joined with the previous
    /// clause's fall-through end when it did not terminate; `default:` narrows by the
    /// complement of all labels. Post-switch flow **joins** every clause's exit — the
    /// last clause's fall-through end, every `break` edge, and (when there is no
    /// `default`) the pre-switch no-match path — so an assignment in a clause reaches
    /// the code after the `switch`. A `return`/`throw`-terminated clause contributes
    /// [`FlowNodeId::UNREACHABLE`], filtered out by [`flow_join`].
    fn build_flow_switch(&mut self, scope: ScopeId, switch: &SwitchStatement<'_>) {
        self.build_flow_expr(scope, &switch.discriminant);
        let discriminant = self.member_discriminant(scope, &switch.discriminant);
        let pre = self.flow_cursor;

        // Intern each case's literal label up front (a non-literal / `default` is `None`).
        let labels: Vec<Option<TypeId>> = switch
            .cases
            .iter()
            .map(|case| case.test.as_ref().and_then(|test| self.literal_expr_type(test)))
            .collect();
        // The labeled cases' literals, for the `default` complement.
        let case_labels: Vec<TypeId> = switch
            .cases
            .iter()
            .zip(&labels)
            .filter_map(|(case, label)| case.test.as_ref().and(*label))
            .collect();
        let has_default = switch.cases.iter().any(|case| case.test.is_none());

        // A `break` in a clause exits to the switch's own join, not the enclosing loop.
        self.break_targets.push(Vec::new());
        // The previous clause's fall-through edge, if it did not terminate.
        let mut fell_through: Option<FlowNodeId> = None;
        for (case, label) in switch.cases.iter().zip(&labels) {
            let direct =
                self.switch_case_entry(pre, &discriminant, case.test.is_some(), *label, &case_labels);
            // Reachable by a direct label match OR by falling through the prior clause.
            let entry = match fell_through {
                Some(prev_end) => self.flow_join(vec![prev_end, direct]),
                None => direct,
            };
            self.flow_cursor = entry;
            self.build_flow_stmts(scope, &case.consequent);
            fell_through = if clause_terminates(&case.consequent) {
                None
            } else {
                Some(self.flow_cursor)
            };
        }
        // The last clause's fall-through end (UNREACHABLE if it terminated).
        let last_end = self.flow_cursor;
        let breaks = self.break_targets.pop().unwrap_or_default();

        let mut post = vec![last_end];
        post.extend(breaks);
        // Without a `default`, the discriminant can match nothing: the pre-switch
        // state flows straight to the post-switch join.
        if !has_default {
            post.push(pre);
        }
        self.flow_cursor = self.flow_join(post);
    }

    /// The flow node a `switch` clause body starts from, given the discriminant
    /// `x.prop`: a labeled `case <lit>:` narrows `x` by `x.prop === lit`; a `default:`
    /// narrows by the complement of all labels; an unrecognized discriminant or a
    /// non-literal case label yields no narrowing (the wide `pre`).
    fn switch_case_entry(
        &mut self,
        pre: FlowNodeId,
        discriminant: &Option<(SymbolId, String)>,
        has_test: bool,
        label: Option<TypeId>,
        case_labels: &[TypeId],
    ) -> FlowNodeId {
        let Some((symbol, property)) = discriminant else {
            return pre;
        };
        match (has_test, label) {
            // `case <literal>:` — narrow to the matching members.
            (true, Some(lit)) => self.new_flow(FlowNode::Condition {
                symbol: *symbol,
                op: crate::check::flow::NarrowOp::Discriminant {
                    property: property.clone(),
                    literal: lit,
                },
                positive: true,
                antecedent: pre,
            }),
            // `default:` — remove each label's member in turn (the complement chain).
            (false, _) => {
                let mut node = pre;
                for &lit in case_labels {
                    node = self.new_flow(FlowNode::Condition {
                        symbol: *symbol,
                        op: crate::check::flow::NarrowOp::Discriminant {
                            property: property.clone(),
                            literal: lit,
                        },
                        positive: false,
                        antecedent: node,
                    });
                }
                node
            }
            // A non-literal `case` test: no narrowing.
            (true, None) => pre,
        }
    }

    /// Build the flow for a `while` loop: a loop label joins the pre-loop edge with
    /// the back edges (body fall-through end + every `continue`); the condition
    /// narrows the body (the true branch) and the code after the loop (the false
    /// branch joined with any `break` edges). The condition is evaluated *at the loop
    /// label* so its references see the back-edge fixpoint (each iteration).
    fn build_flow_while(&mut self, scope: ScopeId, while_stmt: &WhileStatement<'_>) {
        let pre = self.flow_cursor;
        let label = self.new_flow(FlowNode::LoopLabel {
            pre: vec![pre],
            back: Vec::new(),
        });
        self.flow_cursor = label;

        // The test may create Assignment nodes (`while (x = next())`, backlog 53); the
        // condition branches antecede the **post-test** cursor, not the bare label, so
        // those assignments are not orphaned. Its antecedent chain still reaches the
        // label, so the back-edge fixpoint (invariants §1) is preserved.
        self.build_flow_expr(scope, &while_stmt.test);
        let post_test = self.flow_cursor;
        let fact = self.analyze_guard(scope, &while_stmt.test);
        let cond_true = self.flow_condition(post_test, &fact, true);
        let cond_false = self.flow_condition(post_test, &fact, false);

        self.flow_loops.push(FlowLoopFrame { label });
        self.break_targets.push(Vec::new());
        self.flow_cursor = cond_true;
        self.build_flow_stmt(scope, &while_stmt.body);
        let body_end = self.flow_cursor;
        self.add_back_edge(label, body_end);
        self.flow_loops.pop();
        let breaks = self.break_targets.pop().unwrap_or_default();

        let mut post = vec![cond_false];
        post.extend(breaks);
        self.flow_cursor = self.flow_join(post);
    }

    /// Build the flow for an expression: record each identifier reference at the
    /// current cursor, apply `&&`/`||`/ternary branch narrowing to their RHS/arms,
    /// handle an inline assignment, and recurse into nested constructs (calls, member
    /// accesses, literals, and nested function/arrow bodies at their own `START`).
    fn build_flow_expr(&mut self, scope: ScopeId, expr: &Expression<'_>) {
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

    /// `&&` / `||` (M23): the right operand is evaluated under the left operand's
    /// guard — `&&` when it holds (true branch), `||` when it fails (false branch).
    /// The post-expression cursor **joins** the RHS-evaluated end with the RHS-skipped
    /// branch (`&&` → the false branch, `||` → the true branch), so an assignment in
    /// the RHS survives (backlog 53). Condition narrowing still does not persist past
    /// the expression: both join inputs carry the condition's opposing senses, so it
    /// widens back out naturally.
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

    /// An assignment `x = <rhs>` (M23). The RHS is built first (it sees the
    /// pre-assignment flow — `x = x` reads the old `x`). A simple `=` to an identifier
    /// narrows `x` to the **assigned value's** (widened) type in the straight-line
    /// flow after; a compound (`+=`) or complex RHS resets to the declared type (over-
    /// report, sound). A **destructuring** target (`[x] = …`, `({ x } = …)`) resets
    /// every identifier the pattern binds to its declared type — a stale narrow past
    /// the reassignment is a dropped error (review round 1, `assignment_patterns.ts`).
    /// A member target binds no symbol (member paths are not narrowed).
    fn build_flow_assignment(&mut self, scope: ScopeId, assign: &AssignmentExpression<'_>) {
        self.build_flow_expr(scope, &assign.right);

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
        self.binder.graph.resolve(scope, name).filter(|&s| {
            self.binder
                .symbols
                .get(s)
                .and_then(|sym| sym.value)
                .is_some()
        })
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

    /// Reset every identifier a destructuring assignment target binds (review round
    /// 1): array patterns (elements + rest), object patterns (shorthand + renamed
    /// properties + rest), defaults, and nested patterns all recurse here. A member
    /// target binds no symbol (member paths are not narrowed — nothing to reset).
    /// TS-wrapper targets (`x as T`, `x!` — `TSAsExpression` and friends) are out of
    /// the subset and skipped.
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
    fn build_flow_function(&mut self, scope: ScopeId, func: &Function<'_>) {
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
    fn build_flow_class(&mut self, scope: ScopeId, class: &Class<'_>) {
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
        self.flow_cursor = FlowNodeId::START;
        build(self);
        self.flow_cursor = saved_cursor;
        self.flow_loops = saved_loops;
        self.break_targets = saved_breaks;
    }

    /// Append a flow node, returning its id.
    fn new_flow(&mut self, node: FlowNode) -> FlowNodeId {
        let id = FlowNodeId(self.flow_nodes.len() as u32);
        self.flow_nodes.push(node);
        id
    }

    /// A condition node for one branch of `fact` from `antecedent`, with the branch
    /// polarity already folded (`then_branch` applies the fact as written, else its
    /// negation). An unrecognized guard (`None`) narrows nothing — both branches
    /// share the antecedent.
    fn flow_condition(
        &mut self,
        antecedent: FlowNodeId,
        fact: &Option<GuardFact>,
        then_branch: bool,
    ) -> FlowNodeId {
        match fact {
            Some(fact) => {
                let positive = if then_branch {
                    fact.then_positive
                } else {
                    !fact.then_positive
                };
                self.new_flow(FlowNode::Condition {
                    symbol: fact.symbol,
                    op: fact.op.clone(),
                    positive,
                    antecedent,
                })
            }
            None => antecedent,
        }
    }

    /// Join branch antecedents, excluding the unreachable ones. Empty → unreachable;
    /// one → that node (no label); more → a fresh branch label.
    fn flow_join(&mut self, antecedents: Vec<FlowNodeId>) -> FlowNodeId {
        let mut reachable: Vec<FlowNodeId> = antecedents
            .into_iter()
            .filter(|&a| a != FlowNodeId::UNREACHABLE)
            .collect();
        reachable.sort_by_key(|id| id.0);
        reachable.dedup();
        match reachable.as_slice() {
            [] => FlowNodeId::UNREACHABLE,
            [only] => *only,
            _ => self.new_flow(FlowNode::BranchLabel {
                antecedents: reachable,
            }),
        }
    }

    /// Append `edge` as a back edge of a loop `label` (unless the edge is
    /// unreachable, e.g. a body ending in `return`/`break`).
    fn add_back_edge(&mut self, label: FlowNodeId, edge: FlowNodeId) {
        if edge == FlowNodeId::UNREACHABLE {
            return;
        }
        if let Some(FlowNode::LoopLabel { back, .. }) = self.flow_nodes.get_mut(label.0 as usize) {
            back.push(edge);
        }
    }

    // =======================================================================
    // Resolution — the backward walk.
    // =======================================================================

    /// A value symbol's **declared** type — the backward walk's base case (at
    /// `START`) and the reset target for a `None` assignment. Mirrors the pre-M23
    /// `resolve_identifier_type` fallback.
    pub(in crate::check::checker) fn declared_type(&self, symbol: SymbolId) -> TypeId {
        self.binder
            .symbols
            .get(symbol)
            .and_then(|s| s.value)
            .and_then(|decl_id| self.decl_types.get(decl_id))
            .unwrap_or_else(|| self.interner.well_known().error)
    }

    /// The already-known type at a flow node for `symbol`, if any: the `START`/
    /// `UNREACHABLE` sentinels resolve to the declared type; a provisional loop seed
    /// or a durable memo entry short-circuits. `None` means "must walk the node".
    fn flow_lookup_cached(&self, node: FlowNodeId, symbol: SymbolId) -> Option<TypeId> {
        if node == FlowNodeId::START || node == FlowNodeId::UNREACHABLE {
            return Some(self.declared_type(symbol));
        }
        if let Some(&t) = self.flow_provisional.get(&(node, symbol)) {
            return Some(t);
        }
        self.flow_memo.get(&(node, symbol)).copied()
    }

    /// Resolve `symbol`'s narrowed type at flow node `flow` — the memoized backward
    /// walk (architecture §5). Iterative (explicit stack) for the non-loop graph so a
    /// deep nest cannot overflow the host stack; a loop label recurses through
    /// [`resolve_loop_label`] (bounded by loop-nesting depth).
    pub(in crate::check::checker) fn resolve_narrowed_type(
        &mut self,
        flow: FlowNodeId,
        symbol: SymbolId,
    ) -> TypeId {
        if let Some(t) = self.flow_lookup_cached(flow, symbol) {
            return t;
        }
        // Per-call scratch: within one walk every node is computed once (bounds the
        // work and keeps the explicit stack from re-descending shared antecedents).
        let mut scratch: FxHashMap<FlowNodeId, TypeId> = FxHashMap::default();
        let mut stack: Vec<FlowNodeId> = vec![flow];
        while let Some(&node) = stack.last() {
            if scratch.contains_key(&node) {
                stack.pop();
                continue;
            }
            if let Some(t) = self.flow_lookup_cached(node, symbol) {
                scratch.insert(node, t);
                stack.pop();
                continue;
            }
            match self.flow_step(node) {
                FlowStep::Terminal(t) => {
                    self.flow_set(&mut scratch, node, symbol, t);
                    stack.pop();
                }
                FlowStep::Assignment {
                    target,
                    assigned,
                    antecedent,
                } => {
                    if target == symbol {
                        let t = assigned.unwrap_or_else(|| self.declared_type(symbol));
                        self.flow_set(&mut scratch, node, symbol, t);
                        stack.pop();
                    } else if let Some(base) = self.scratch_or_cached(&scratch, antecedent, symbol) {
                        self.flow_set(&mut scratch, node, symbol, base);
                        stack.pop();
                    } else {
                        stack.push(antecedent);
                    }
                }
                FlowStep::Condition {
                    guard_symbol,
                    op,
                    positive,
                    antecedent,
                } => {
                    if let Some(base) = self.scratch_or_cached(&scratch, antecedent, symbol) {
                        let t = if guard_symbol == symbol {
                            narrow(self.interner, base, &op, positive)
                        } else {
                            base
                        };
                        self.flow_set(&mut scratch, node, symbol, t);
                        stack.pop();
                    } else {
                        stack.push(antecedent);
                    }
                }
                FlowStep::Branch { antecedents } => {
                    let missing = antecedents
                        .iter()
                        .copied()
                        .find(|&a| self.scratch_or_cached(&scratch, a, symbol).is_none());
                    if let Some(a) = missing {
                        stack.push(a);
                    } else {
                        let parts: Vec<TypeId> = antecedents
                            .iter()
                            .map(|&a| {
                                self.scratch_or_cached(&scratch, a, symbol)
                                    .unwrap_or_else(|| self.declared_type(symbol))
                            })
                            .collect();
                        let t = self.interner.union(parts);
                        self.flow_set(&mut scratch, node, symbol, t);
                        stack.pop();
                    }
                }
                FlowStep::Loop => {
                    let t = self.resolve_loop_label(node, symbol);
                    // `resolve_loop_label` handles the durable memo itself.
                    scratch.insert(node, t);
                    stack.pop();
                }
            }
        }
        scratch
            .get(&flow)
            .copied()
            .unwrap_or_else(|| self.declared_type(symbol))
    }

    /// Resolve a `while` loop label (M23): a **single-unroll fixpoint** — the union of
    /// the pre-loop seed with the back-edge types, the seed made visible for re-entry
    /// via [`Pass::flow_provisional`]. The provisional seed is **never** promoted to
    /// the durable memo (invariants §1); durable writes are suppressed while any loop
    /// fixpoint is in progress ([`Pass::flow_loop_depth`]).
    fn resolve_loop_label(&mut self, label: FlowNodeId, symbol: SymbolId) -> TypeId {
        if let Some(&t) = self.flow_provisional.get(&(label, symbol)) {
            return t;
        }
        if self.flow_loop_depth == 0 {
            if let Some(&t) = self.flow_memo.get(&(label, symbol)) {
                return t;
            }
        }
        let (pre, back) = match self.flow_nodes.get(label.0 as usize) {
            Some(FlowNode::LoopLabel { pre, back }) => (pre.clone(), back.clone()),
            _ => return self.declared_type(symbol),
        };

        self.flow_loop_depth += 1;
        // Guard re-entry during pre/seed resolution with the declared type (sound;
        // pre edges do not normally reach the label, but a malformed graph must still
        // terminate).
        let declared = self.declared_type(symbol);
        self.flow_provisional.insert((label, symbol), declared);

        let mut pre_parts: Vec<TypeId> = Vec::with_capacity(pre.len());
        for antecedent in &pre {
            let t = self.resolve_narrowed_type(*antecedent, symbol);
            pre_parts.push(t);
        }
        let seed = self.interner.union(pre_parts);
        // The seed is now the value a re-entrant back-edge walk sees.
        self.flow_provisional.insert((label, symbol), seed);

        let mut parts: Vec<TypeId> = vec![seed];
        for antecedent in &back {
            let t = self.resolve_narrowed_type(*antecedent, symbol);
            parts.push(t);
        }
        self.flow_provisional.remove(&(label, symbol));
        self.flow_loop_depth -= 1;

        let result = self.interner.union(parts);
        if self.flow_loop_depth == 0 {
            self.flow_memo.insert((label, symbol), result);
        }
        result
    }

    /// Look a node's type up in the per-call scratch, then the cross-call caches.
    fn scratch_or_cached(
        &self,
        scratch: &FxHashMap<FlowNodeId, TypeId>,
        node: FlowNodeId,
        symbol: SymbolId,
    ) -> Option<TypeId> {
        scratch
            .get(&node)
            .copied()
            .or_else(|| self.flow_lookup_cached(node, symbol))
    }

    /// Record a resolved node type in the per-call scratch and — unless a loop
    /// fixpoint is in progress (its value could depend on a provisional seed) — the
    /// durable memo.
    fn flow_set(
        &mut self,
        scratch: &mut FxHashMap<FlowNodeId, TypeId>,
        node: FlowNodeId,
        symbol: SymbolId,
        ty: TypeId,
    ) {
        scratch.insert(node, ty);
        if self.flow_loop_depth == 0 {
            self.flow_memo.insert((node, symbol), ty);
        }
    }

    /// Snapshot a flow node's resolution-relevant data (cloning the small owned bits
    /// so the `&mut Interner` narrow/union calls do not conflict with the arena
    /// borrow). `START`/`UNREACHABLE` are handled by the caller before this, so the
    /// defensive fallthrough (`Terminal(error)`) is unreachable.
    fn flow_step(&self, node: FlowNodeId) -> FlowStep {
        match self.flow_nodes.get(node.0 as usize) {
            Some(FlowNode::Assignment {
                symbol,
                assigned,
                antecedent,
            }) => FlowStep::Assignment {
                target: *symbol,
                assigned: *assigned,
                antecedent: *antecedent,
            },
            Some(FlowNode::Condition {
                symbol,
                op,
                positive,
                antecedent,
            }) => FlowStep::Condition {
                guard_symbol: *symbol,
                op: op.clone(),
                positive: *positive,
                antecedent: *antecedent,
            },
            Some(FlowNode::BranchLabel { antecedents }) => FlowStep::Branch {
                antecedents: antecedents.clone(),
            },
            Some(FlowNode::LoopLabel { .. }) => FlowStep::Loop,
            _ => FlowStep::Terminal(self.interner.well_known().error),
        }
    }
}

/// One backward-walk step, cloned out of the flow arena so the resolver can mutate
/// the interner (narrow/union) without holding the arena borrow.
enum FlowStep {
    /// A sentinel node's fallback (defensive — `START`/`UNREACHABLE` are handled
    /// before this).
    Terminal(TypeId),
    Assignment {
        target: SymbolId,
        assigned: Option<TypeId>,
        antecedent: FlowNodeId,
    },
    Condition {
        guard_symbol: SymbolId,
        op: crate::check::flow::NarrowOp,
        positive: bool,
        antecedent: FlowNodeId,
    },
    Branch {
        antecedents: Vec<FlowNodeId>,
    },
    Loop,
}

//! Flow-node CFG construction and backward resolution (architecture §5).
//! The pre-pass records each reference's flow node before checking, with loop back
//! edges complete up front. Resolution memoizes by `(flow node, symbol)`; loop
//! labels use a single-unroll fixpoint whose seed is never durably memoized.

mod exprs;
mod nodes;

use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::flow::{FlowNode, FlowNodeId};
use crate::types::store::TypeId;
use oxc_ast::ast::{
    BreakStatement, ContinueStatement, Declaration, IfStatement, LabeledStatement, Statement,
    SwitchStatement, WhileStatement,
};

use super::context::*;
use super::narrowing::clause_terminates;

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

    pub(super) fn build_flow_stmts(&mut self, scope: ScopeId, statements: &[Statement<'_>]) {
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
            Statement::LabeledStatement(labeled) => self.build_flow_labeled(scope, labeled),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(decl) = &export.declaration {
                    self.build_flow_declaration(scope, decl);
                }
            }
            Statement::BreakStatement(break_stmt) => {
                // The break carries the current (narrowed) state out to the nearest
                // breakable's exit join (loop or `switch`) — what un-narrows the
                // after-loop state (`breakTrap`) and carries a clause's assignments
                // out of a `switch` (backlog 53).
                self.build_flow_break(break_stmt);
            }
            Statement::ContinueStatement(continue_stmt) => {
                // A `continue` is a back edge to the loop label (re-checks the
                // condition), not an exit edge.
                self.build_flow_continue(continue_stmt);
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

    fn build_flow_labeled(&mut self, scope: ScopeId, labeled: &LabeledStatement<'_>) {
        let allows_continue = labeled_statement_allows_continue(&labeled.body);
        self.label_targets.push(FlowLabelFrame {
            name: labeled.label.name.as_str().to_owned(),
            breaks: Vec::new(),
            continue_target: None,
            allows_continue,
        });

        self.build_flow_stmt(scope, &labeled.body);
        let body_end = self.flow_cursor;
        let label = self.label_targets.pop().unwrap();
        let mut post = vec![body_end];
        post.extend(label.breaks);
        self.flow_cursor = self.flow_join(post);
    }

    fn build_flow_break(&mut self, break_stmt: &BreakStatement<'_>) {
        let cursor = self.flow_cursor;
        if let Some(label) = &break_stmt.label {
            self.add_labeled_break(label.name.as_str(), cursor);
        } else if let Some(target) = self.break_targets.last_mut() {
            target.push(cursor);
        }
        self.flow_cursor = FlowNodeId::UNREACHABLE;
    }

    fn build_flow_continue(&mut self, continue_stmt: &ContinueStatement<'_>) {
        let cursor = self.flow_cursor;
        let label = match &continue_stmt.label {
            Some(label) => self.labeled_continue_target(label.name.as_str()),
            None => self.flow_loops.last().map(|frame| frame.label),
        };
        if let Some(label) = label {
            self.add_back_edge(label, cursor);
        }
        self.flow_cursor = FlowNodeId::UNREACHABLE;
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

    /// Build `switch` flow: case/default discriminant narrows, fallthrough joins
    /// into the next clause, and post-switch flow joins fallthrough, breaks, and
    /// the no-match path when there is no default.
    fn build_flow_switch(&mut self, scope: ScopeId, switch: &SwitchStatement<'_>) {
        self.build_flow_expr(scope, &switch.discriminant);
        let discriminant = self.member_discriminant(scope, &switch.discriminant);
        let pre = self.flow_cursor;

        // Intern each case's literal label up front (a non-literal / `default` is `None`).
        let labels: Vec<Option<TypeId>> = switch
            .cases
            .iter()
            .map(|case| {
                case.test
                    .as_ref()
                    .and_then(|test| self.literal_expr_type(test))
            })
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
            let direct = self.switch_case_entry(
                pre,
                &discriminant,
                case.test.is_some(),
                *label,
                &case_labels,
            );
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
        if pre == FlowNodeId::UNREACHABLE {
            return FlowNodeId::UNREACHABLE;
        }
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

    /// Build `while` flow with a loop label for pre-loop plus back edges. The
    /// condition is evaluated at the label so references see the fixpoint.
    fn build_flow_while(&mut self, scope: ScopeId, while_stmt: &WhileStatement<'_>) {
        let pre = self.flow_cursor;
        if pre == FlowNodeId::UNREACHABLE {
            return;
        }
        let label = self.new_flow(FlowNode::LoopLabel {
            pre: vec![pre],
            back: Vec::new(),
        });
        for target in self.label_targets.iter_mut().rev() {
            if !target.allows_continue {
                break;
            }
            if target.continue_target.is_none() {
                target.continue_target = Some(label);
            }
        }
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
}

fn labeled_statement_allows_continue(stmt: &Statement<'_>) -> bool {
    match stmt {
        Statement::WhileStatement(_) => true,
        Statement::LabeledStatement(labeled) => labeled_statement_allows_continue(&labeled.body),
        _ => false,
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

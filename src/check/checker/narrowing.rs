//! Guard analysis plus structural statement walkers.
//! The flow-node CFG is the single narrowing model; these walkers just descend
//! into expressions while guard analysis feeds flow condition nodes.

use super::context::*;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::flow::{NarrowOp, TypeofTag};
use crate::types::repr::LiteralValue;
use crate::types::store::TypeId;
use oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, BinaryExpression, BinaryOperator, Expression,
    IfStatement, Statement, SwitchStatement, UnaryOperator, WhileStatement,
};

/// A recognized guard fact: the specific symbol, operation, and then-branch
/// polarity. Symbol-keying here keeps a narrowing of `x` from touching any other
/// binding.
pub(in crate::check::checker) struct GuardFact {
    /// The value symbol the guard refines (resolved from the condition's operand
    /// through the scope graph, so it is the exact binding in scope).
    pub(in crate::check::checker) symbol: SymbolId,
    /// The narrowing operation to apply.
    pub(in crate::check::checker) op: NarrowOp,
    /// The polarity for the **then**-branch (`true` = apply the op as written). The
    /// else-branch applies the negation. A leading `!` on the condition flips this
    /// at analysis time so the builder is polarity-agnostic.
    pub(in crate::check::checker) then_positive: bool,
}

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Check an `if`/`else` statement (M23: the narrowing itself lives in the flow
    /// graph). Walk the condition (so its operands resolve and nested constructs are
    /// checked) and both branches; a reference inside a branch resolves its narrowed
    /// type against the flow node the pre-pass recorded for it.
    pub(in crate::check::checker) fn check_if(
        &mut self,
        scope: ScopeId,
        if_stmt: &IfStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        self.infer_expr(scope, &if_stmt.test);
        self.check_stmt(scope, &if_stmt.consequent, declared_ret, inferred);
        if let Some(alternate) = &if_stmt.alternate {
            self.check_stmt(scope, alternate, declared_ret, inferred);
        }
    }

    /// Check a `switch` statement (M23: per-case narrowing lives in the flow graph).
    /// Walk the discriminant and every clause body; each reference inside a clause
    /// resolves against its recorded flow node (the flow builder installed the
    /// per-case discriminant narrowing, including the conservative fallthrough rule).
    pub(in crate::check::checker) fn check_switch(
        &mut self,
        scope: ScopeId,
        switch: &SwitchStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        // The discriminant is evaluated in the enclosing scope; the case block is one
        // switch-local lexical scope (binder-created, keyed by span) that the clauses
        // share so their block-scoped declarations do not leak past the switch.
        self.infer_expr(scope, &switch.discriminant);
        let switch_scope = self
            .binder
            .block_scopes
            .get(&(self.current_module, switch.span.start))
            .copied()
            .unwrap_or(scope);
        let consequents: Vec<&[Statement<'_>]> = switch
            .cases
            .iter()
            .map(|case| case.consequent.as_slice())
            .collect();
        let mut surfaces = self.reserve_function_surfaces_for_lists(switch_scope, &consequents);
        for case in &switch.cases {
            if let Some(test) = &case.test {
                self.infer_expr(switch_scope, test);
            }
            // Every clause shares one lexical scope, but consecutive overload grouping
            // remains local to the clause's own statement list.
            self.check_statement_list_with_surfaces(
                switch_scope,
                &case.consequent,
                declared_ret,
                inferred,
                &mut surfaces,
            );
        }
    }

    /// Check a `while` statement (M23). Walk the condition and body; the body's
    /// references resolve against the flow graph's loop label (the back-edge
    /// fixpoint), and code after the loop against the exit/`break` join.
    pub(in crate::check::checker) fn check_while(
        &mut self,
        scope: ScopeId,
        while_stmt: &WhileStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        self.infer_expr(scope, &while_stmt.test);
        self.check_stmt(scope, &while_stmt.body, declared_ret, inferred);
    }

    /// Analyze a condition into a guard fact, or `None` for unrecognized shapes
    /// (unknown guards must never narrow). Recognized forms cover `typeof`,
    /// truthiness, nullish equality, literal discriminants, and `"prop" in x`;
    /// leading `!` flips the then-branch polarity.
    pub(in crate::check::checker) fn analyze_guard(
        &mut self,
        scope: ScopeId,
        test: &Expression<'_>,
    ) -> Option<GuardFact> {
        match test {
            // `!cond` — recurse and flip the then-branch polarity.
            Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
                let mut inner = self.analyze_guard(scope, &unary.argument)?;
                inner.then_positive = !inner.then_positive;
                Some(inner)
            }
            // A parenthesized condition is transparent.
            Expression::ParenthesizedExpression(paren) => {
                self.analyze_guard(scope, &paren.expression)
            }
            // A plain assignment used as a condition (`while (x = e)`, backlog 53): the
            // value is the assigned expression, so its truthiness narrows `x`. The
            // Assignment node itself is created by the flow builder; here we only
            // recognize the truthy guard on the target symbol.
            Expression::AssignmentExpression(assign)
                if assign.operator == AssignmentOperator::Assign =>
            {
                let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
                    return None;
                };
                let symbol = self.binder.resolve_value(scope, target.name.as_str())?;
                Some(GuardFact {
                    symbol,
                    op: NarrowOp::Truthy,
                    then_positive: true,
                })
            }
            // Bare truthiness `if (x)`.
            Expression::Identifier(_) => {
                let symbol = self.condition_symbol(scope, test)?;
                Some(GuardFact {
                    symbol,
                    op: NarrowOp::Truthy,
                    then_positive: true,
                })
            }
            // A binary comparison: `typeof x === "tag"`, `x === null`, `x.kind === "c"`,
            // or the `in` operator `"prop" in x`.
            Expression::BinaryExpression(binary) => {
                if binary.operator == BinaryOperator::In {
                    self.in_guard(scope, binary)
                } else {
                    self.analyze_equality_guard(scope, binary)
                }
            }
            _ => None,
        }
    }

    /// Analyze equality guards. Strict equality handles typeof/nullish forms; loose
    /// equality is accepted only for `typeof`, and operands are tried in both orders.
    fn analyze_equality_guard(
        &mut self,
        scope: ScopeId,
        binary: &BinaryExpression<'_>,
    ) -> Option<GuardFact> {
        // The positive sense of the comparison: `===`/`==` keep the matching branch as
        // the then-branch; `!==`/`!=` invert it. Non-equality operators are not guards.
        let eq_positive = match binary.operator {
            BinaryOperator::StrictEquality | BinaryOperator::Equality => true,
            BinaryOperator::StrictInequality | BinaryOperator::Inequality => false,
            _ => return None,
        };
        let strict = matches!(
            binary.operator,
            BinaryOperator::StrictEquality | BinaryOperator::StrictInequality
        );

        let left = &binary.left;
        let right = &binary.right;

        // typeof form: `typeof x === "tag"` (either operand order). Loose `==`/`!=` is
        // fine here because `typeof` always yields a string.
        if let Some(fact) = self
            .typeof_guard(scope, left, right, eq_positive)
            .or_else(|| self.typeof_guard(scope, right, left, eq_positive))
        {
            return Some(fact);
        }

        // null/undefined form: `x === null` (either operand order). Strict only — loose
        // `== null` also matches `undefined`, a different (deferred) rule, so a loose
        // null/undefined comparison is treated as an unrecognized guard (narrows
        // nothing — sound).
        if strict {
            if let Some(fact) = self
                .nullish_guard(scope, left, right, eq_positive)
                .or_else(|| self.nullish_guard(scope, right, left, eq_positive))
            {
                return Some(fact);
            }
        }

        // Literal-discriminant form (M8): `x.prop === <literal>` (either operand order).
        // **Strict only**: loose `==` coerces, which complicates "could equal", so a
        // loose discriminant comparison narrows nothing (sound). Interns the literal, so
        // this needs `&mut pass`.
        if strict {
            if let Some(fact) = self.discriminant_guard(scope, left, right, eq_positive) {
                return Some(fact);
            }
            if let Some(fact) = self.discriminant_guard(scope, right, left, eq_positive) {
                return Some(fact);
            }
        }

        None
    }

    /// Try to read `typeof_side` as `typeof <ident>` and `tag_side` as a recognized
    /// `typeof` tag string literal, producing a typeof guard. `eq_positive` is the
    /// comparison's positive sense (`===`/`==` → `true`); it becomes the then-branch
    /// polarity for "keep the tag".
    fn typeof_guard(
        &self,
        scope: ScopeId,
        typeof_side: &Expression<'_>,
        tag_side: &Expression<'_>,
        eq_positive: bool,
    ) -> Option<GuardFact> {
        let Expression::UnaryExpression(unary) = typeof_side else {
            return None;
        };
        if unary.operator != UnaryOperator::Typeof {
            return None;
        }
        let symbol = self.condition_symbol(scope, &unary.argument)?;
        let Expression::StringLiteral(lit) = tag_side else {
            return None;
        };
        let tag = TypeofTag::from_tag_literal(lit.value.as_str())?;
        Some(GuardFact {
            symbol,
            op: NarrowOp::Typeof(tag),
            // `typeof x === "string"` then-branch keeps the tag; `!==` flips it.
            then_positive: eq_positive,
        })
    }

    /// Try to read `ident_side` as a plain identifier and `nullish_side` as the `null`
    /// or `undefined` literal, producing a null/undefined-equality guard. `eq_positive`
    /// is the comparison's positive sense; for `x === null` the then-branch keeps only
    /// `null`, so `then_positive == eq_positive`.
    fn nullish_guard(
        &self,
        scope: ScopeId,
        ident_side: &Expression<'_>,
        nullish_side: &Expression<'_>,
        eq_positive: bool,
    ) -> Option<GuardFact> {
        let is_undefined = match nullish_side {
            Expression::NullLiteral(_) => false,
            // The `undefined` keyword parses as an identifier reference; it is a value
            // operand here, never a narrowing *target*, so match it by name.
            Expression::Identifier(ident) if ident.name.as_str() == "undefined" => true,
            _ => return None,
        };
        let symbol = self.condition_symbol(scope, ident_side)?;
        Some(GuardFact {
            symbol,
            op: NarrowOp::EqNullish { is_undefined },
            then_positive: eq_positive,
        })
    }

    /// Read `x.prop === <literal>` as an M8 literal-discriminant guard targeting
    /// `x`; the comparison literal is interned.
    fn discriminant_guard(
        &mut self,
        scope: ScopeId,
        member_side: &Expression<'_>,
        literal_side: &Expression<'_>,
        eq_positive: bool,
    ) -> Option<GuardFact> {
        let (symbol, property) = self.member_discriminant(scope, member_side)?;
        let literal = self.literal_expr_type(literal_side)?;
        Some(GuardFact {
            symbol,
            op: NarrowOp::Discriminant { property, literal },
            then_positive: eq_positive,
        })
    }

    /// Read `x.prop` as a discriminant member access. Only non-optional static
    /// access on a narrowable identifier qualifies; keying on the base symbol keeps
    /// the narrowing on `x`.
    pub(in crate::check::checker) fn member_discriminant(
        &self,
        scope: ScopeId,
        expr: &Expression<'_>,
    ) -> Option<(SymbolId, String)> {
        let Expression::StaticMemberExpression(member) = expr else {
            return None;
        };
        // Optional chaining (`x?.kind`) changes the value (it can be `undefined`); keep
        // it out of the recognized discriminant form.
        if member.optional {
            return None;
        }
        let symbol = self.condition_symbol(scope, &member.object)?;
        Some((symbol, member.property.name.to_string()))
    }

    /// Intern a literal **value** expression (`"circle"`, `42`, `true`) to its literal
    /// `TypeId`, or `None` if it is not a plain literal (the discriminant form only
    /// narrows against a literal). Mirrors the literal arms of [`infer_expr`].
    pub(in crate::check::checker) fn literal_expr_type(
        &mut self,
        expr: &Expression<'_>,
    ) -> Option<TypeId> {
        let value = match expr {
            Expression::StringLiteral(s) => LiteralValue::String(s.value.to_string()),
            Expression::NumericLiteral(n) => LiteralValue::Number(n.value),
            Expression::BooleanLiteral(b) => LiteralValue::Boolean(b.value),
            _ => return None,
        };
        Some(self.interner.intern_literal(value))
    }

    /// Resolve a condition operand to the value `SymbolId` it narrows, or `None` if it
    /// is not a narrowable plain identifier in scope. A non-identifier operand (member
    /// access, call, literal, the `undefined` keyword, …) is not narrowable — returning
    /// `None` keeps narrowing keyed strictly to a real local/parameter binding.
    fn condition_symbol(&self, scope: ScopeId, expr: &Expression<'_>) -> Option<SymbolId> {
        let Expression::Identifier(ident) = expr else {
            return None;
        };
        // The `undefined` keyword is an identifier reference but not a narrowable
        // binding; exclude it so `x === undefined` does not treat `undefined` as the
        // narrowed symbol.
        if ident.name.as_str() == "undefined" {
            return None;
        }
        self.binder.resolve_value(scope, ident.name.as_str())
    }

    /// Analyze `"prop" in x` as an M8 guard. The left side must be a string literal
    /// and the right side a narrowable identifier; every other shape narrows nothing.
    fn in_guard(&self, scope: ScopeId, binary: &BinaryExpression<'_>) -> Option<GuardFact> {
        // The property name must be a static string literal: `"a" in x`.
        let Expression::StringLiteral(name) = &binary.left else {
            return None;
        };
        let symbol = self.condition_symbol(scope, &binary.right)?;
        Some(GuardFact {
            symbol,
            op: NarrowOp::In {
                property: name.value.to_string(),
            },
            then_positive: true,
        })
    }
}

/// Whether a `switch` clause definitely terminates. Conservative false means the
/// next clause cannot assume only its own label, avoiding over-narrowing.
pub(in crate::check::checker) fn clause_terminates(consequent: &[Statement<'_>]) -> bool {
    match consequent.last() {
        Some(stmt) => statement_terminates(stmt),
        None => false,
    }
}

/// Whether one statement terminates a switch clause for fallthrough purposes.
/// `continue` is intentionally not a terminator here.
fn statement_terminates(stmt: &Statement<'_>) -> bool {
    match stmt {
        Statement::ReturnStatement(_)
        | Statement::BreakStatement(_)
        | Statement::ThrowStatement(_) => true,
        // A block terminates iff its last statement does (so `case x: { …; break; }`
        // is recognized as terminating).
        Statement::BlockStatement(block) => clause_terminates(&block.body),
        _ => false,
    }
}

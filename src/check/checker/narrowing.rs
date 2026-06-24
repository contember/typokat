//! narrowing module (extracted from checker/mod.rs).

use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::flow::{narrow, NarrowOp, TypeofTag};
use crate::types::repr::LiteralValue;
use crate::types::store::TypeId;
use oxc_ast::ast::{
    BinaryExpression, BinaryOperator, Expression, IfStatement, Statement, SwitchStatement,
    UnaryOperator,
};
use super::context::*;

/// A recognized guard fact: the **specific symbol** being narrowed plus the
/// narrowing operation (and the polarity already folded so the then-branch applies
/// it as written). Pairing a [`NarrowOp`] with a `SymbolId` here — in the checker,
/// not in the flow operations — is the symbol-keying that keeps a narrowing of `x`
/// from ever touching another symbol.
pub(in crate::check::checker) struct GuardFact {
    /// The value symbol the guard refines (resolved from the condition's operand
    /// through the scope graph, so it is the exact binding in scope).
    symbol: SymbolId,
    /// The narrowing operation to apply.
    op: NarrowOp,
    /// The polarity for the **then**-branch (`true` = apply the op as written). The
    /// else-branch applies the negation. A leading `!` on the condition flips this
    /// at analysis time so the driver is polarity-agnostic.
    then_positive: bool,
}

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Check an `if`/`else` statement with control-flow narrowing (M7, architecture
    /// §5) — the structured-flow **fork-and-restore**.
    ///
    /// The condition is first inferred under the *pre-`if`* (current) environment so
    /// its operands' references resolve and any nested constructs are checked. It is
    /// then analyzed into an optional `GuardFact`. The two branches are checked under
    /// **forked** copies of the narrowing environment:
    ///
    ///  - the **then**-branch under the env with the positive fact applied,
    ///  - the **else**-branch under the env with the negative (complement) fact.
    ///
    /// After both branches the pre-`if` environment is **restored**, so no
    /// branch-local narrowing escapes the `if` (this is exactly what makes a
    /// post-branch reference see the wide type again — the `const after` re-error in
    /// the fixtures). An **unrecognized** condition yields no fact, so both branches
    /// run under the unchanged env (no spurious narrowing → no false negatives). A
    /// conservative join is used (the post-`if` env is the pre-`if` env); precise joins
    /// — e.g. both-branches-return — are deferred to the flow-node CFG (M8+).
    pub(in crate::check::checker) fn check_if(
        &mut self,
        scope: ScopeId,
        if_stmt: &IfStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        // Evaluate the condition under the current env (before forking) so references
        // resolve and nested functions/calls inside it are checked exactly once.
        self.infer_expr(scope, &if_stmt.test);
        let fact = self.analyze_guard(scope, &if_stmt.test);

        // Snapshot the pre-`if` environment so each branch starts from it and it is
        // restored afterwards. The map holds only currently-narrowed symbols, so the
        // clone is small; cloning makes the restore unconditionally correct.
        let saved = self.narrowed.clone();

        // --- then-branch: apply the positive fact, walk, restore. ---
        if let Some(fact) = &fact {
            self.apply_guard(fact, fact.then_positive);
        }
        self.check_stmt(scope, &if_stmt.consequent, declared_ret, inferred);
        self.narrowed = saved.clone();

        // --- else-branch: apply the negative (complement) fact, walk, restore. ---
        if let Some(alternate) = &if_stmt.alternate {
            if let Some(fact) = &fact {
                self.apply_guard(fact, !fact.then_positive);
            }
            self.check_stmt(scope, alternate, declared_ret, inferred);
        }
        // Restore the pre-`if` env unconditionally: narrowing must not escape the `if`.
        self.narrowed = saved;
    }

    /// Check a `switch (x.prop) { case "lit": … }` statement with per-case
    /// discriminated-union narrowing (M8, architecture §5) — a fork-and-restore
    /// mirroring [`check_if`], one fork per `case`/`default` clause.
    ///
    /// The discriminant is inferred under the current env (so its operands resolve),
    /// then read as a member-access discriminant `x.prop`. Each clause is checked under
    /// a **forked** narrowing env:
    ///
    ///  - a `case "lit":` narrows `x` by `x.prop === "lit"` (the discriminant op), and
    ///  - `default:` narrows `x` by the **complement** of all the case labels
    ///    (`x.prop !== lit1 && … && x.prop !== litN`).
    ///
    /// After each clause the pre-`switch` env is restored, so no per-case narrowing
    /// escapes the `switch`. An **unrecognized** discriminant (not `x.prop`), or a
    /// non-literal `case` test, yields no narrowing for that clause (sound — narrows
    /// nothing). **Fallthrough is handled conservatively**: a clause whose body does
    /// not definitely terminate (no trailing `return`/`break`/`throw`) falls into the
    /// next clause, so the next clause's body could see *this* clause's discriminant
    /// value too; in that case the next clause is checked **without** narrowing (the
    /// wide type — sound). The common no-fallthrough pattern (each `case` ends in
    /// `return`/`break`) gets the precise per-case narrowing.
    pub(in crate::check::checker) fn check_switch(
        &mut self,
        scope: ScopeId,
        switch: &SwitchStatement<'_>,
        declared_ret: Option<TypeId>,
        inferred: &mut Option<TypeId>,
    ) {
        // Evaluate the discriminant under the current env (before forking) so its
        // references resolve and nested constructs inside it are checked once.
        self.infer_expr(scope, &switch.discriminant);
        let discriminant = self.member_discriminant(scope, &switch.discriminant);

        // Intern every `case`'s literal label up front (mutable). A `default` clause has
        // no test (`None`); a non-literal `case` test also yields `None` (it cannot be
        // narrowed). The labels are reused to build the `default` complement.
        let labels: Vec<Option<TypeId>> = switch
            .cases
            .iter()
            .map(|case| {
                case.test
                    .as_ref()
                    .and_then(|test| self.literal_expr_type(test))
            })
            .collect();

        // The non-`default` labels, for the `default` clause's complement.
        let case_labels: Vec<TypeId> = switch
            .cases
            .iter()
            .zip(&labels)
            .filter_map(|(case, label)| case.test.as_ref().and(*label))
            .collect();

        let saved = self.narrowed.clone();

        // Whether the *previous* clause fell through into this one (so this clause's
        // body could also see the previous clause's discriminant value → be conservative
        // and apply no narrowing for it).
        let mut prev_fell_through = false;

        for (case, label) in switch.cases.iter().zip(&labels) {
            // Apply this clause's narrowing only when no preceding clause falls through
            // into it (otherwise the value reaching this body is not pinned to this
            // clause's label → narrowing would be unsound).
            if !prev_fell_through {
                if let Some((symbol, property)) = &discriminant {
                    self.apply_case_narrowing(*symbol, property, case, *label, &case_labels);
                }
            }

            // Check the clause body in the current scope (a block-bodied case opens its
            // own scope via the `BlockStatement` arm of `check_stmt`).
            for stmt in &case.consequent {
                self.check_stmt(scope, stmt, declared_ret, inferred);
            }

            // Restore the pre-`switch` env: narrowing must not escape a clause.
            self.narrowed = saved.clone();

            // Does this clause fall through into the next? (Conservative — empty or
            // non-terminating bodies fall through.)
            prev_fell_through = !clause_terminates(&case.consequent);
        }

        // Restore unconditionally (covers an empty `switch`).
        self.narrowed = saved;
    }

    /// Install the narrowing for one `switch` clause into the environment: a `case`
    /// with a literal label narrows `symbol` by `symbol.prop === label` (then-sense);
    /// a `default` (no label) narrows by the complement of every case label
    /// (`symbol.prop !== label` applied for each). A `case` whose test was not a
    /// literal (`label == None` but it *is* a `case`) installs no narrowing (sound).
    fn apply_case_narrowing(
        &mut self,
        symbol: SymbolId,
        property: &str,
        case: &oxc_ast::ast::SwitchCase<'_>,
        label: Option<TypeId>,
        case_labels: &[TypeId],
    ) {
        match (&case.test, label) {
            // `case <literal>:` — narrow to the matching members.
            (Some(_), Some(lit)) => {
                let op = NarrowOp::Discriminant {
                    property: property.to_string(),
                    literal: lit,
                };
                let current = self.resolve_identifier_type(symbol);
                let narrowed = narrow(self.interner, current, &op, /* positive */ true);
                self.narrowed.insert(symbol, narrowed);
            }
            // `default:` — narrow to the complement of all case labels by removing each
            // label's member in turn (`prop !== label1 && … && prop !== labelN`).
            (None, _) => {
                let mut current = self.resolve_identifier_type(symbol);
                for &lit in case_labels {
                    let op = NarrowOp::Discriminant {
                        property: property.to_string(),
                        literal: lit,
                    };
                    current = narrow(self.interner, current, &op, /* positive */ false);
                }
                self.narrowed.insert(symbol, current);
            }
            // A non-literal `case` test: cannot narrow → leave the env unchanged.
            (Some(_), None) => {}
        }
    }

    /// Apply a guard fact to the narrowing environment for one branch: narrow the
    /// guarded symbol's **current** type (its already-narrowed type if an enclosing
    /// `if` narrowed it, else its declared type — so nested `if`s compose) by the
    /// fact's operation under `positive`, and install the result.
    ///
    /// `positive` is the branch polarity (the then-branch passes `then_positive`, the
    /// else-branch its negation). A symbol with no resolvable current type (out of
    /// subset) is left untouched.
    fn apply_guard(&mut self, fact: &GuardFact, positive: bool) {
        let current = self.resolve_identifier_type(fact.symbol);
        let narrowed = narrow(self.interner, current, &fact.op, positive);
        self.narrowed.insert(fact.symbol, narrowed);
    }

    /// Analyze a condition expression into a `GuardFact`, or `None` if it is not a
    /// recognized guard (in which case nothing is narrowed — soundness: an unknown
    /// guard must never narrow). Recognizes, over a plain identifier operand:
    ///
    ///  - **typeof**: `typeof x === "string" | "number" | "boolean"` and `!==`/`==`/
    ///    `!=`, with the `typeof …` on either side of the comparison;
    ///  - **truthiness**: bare `x`, and `!x` (which flips the polarity);
    ///  - **null/undefined equality**: `x === null` / `x === undefined` and `!==`,
    ///    with the literal on either side;
    ///  - **literal discriminant** (M8): `x.prop === <literal>` / `!==` (strict only,
    ///    literal on either side), narrowing the symbol `x`;
    ///  - **`in` operator** (M8): `"prop" in x`, narrowing the symbol `x`.
    ///
    /// A leading `!` flips `then_positive`. Anything else (an unrecognized tag, a
    /// non-identifier operand, a `&&`/`||`, a member-access operand in a position other
    /// than the recognized discriminant, …) returns `None`. Takes `&mut Pass` because
    /// the discriminant form interns the comparison literal.
    fn analyze_guard(&mut self, scope: ScopeId, test: &Expression<'_>) -> Option<GuardFact> {
        match test {
            // `!cond` — recurse and flip the then-branch polarity.
            Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
                let mut inner = self.analyze_guard(scope, &unary.argument)?;
                inner.then_positive = !inner.then_positive;
                Some(inner)
            }
            // A parenthesized condition is transparent.
            Expression::ParenthesizedExpression(paren) => self.analyze_guard(scope, &paren.expression),
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

    /// Analyze an equality `BinaryExpression` into a guard fact (M7). Handles strict
    /// (`===`/`!==`) equality for both the typeof and null/undefined forms; for the
    /// typeof form, where `typeof x` is always a string, loose `==`/`!=` behave
    /// identically and are accepted too. The two operands are tried in both orders so
    /// `typeof x === "s"` and `"s" === typeof x` (and `x === null` / `null === x`) are
    /// both recognized.
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
        // fine here because `typeof` always yields a string. These borrow `pass`
        // immutably and return an owned fact, so the discriminant form below can take
        // `&mut pass` afterwards.
        if let Some(fact) = self.typeof_guard(scope, left, right, eq_positive)
            .or_else(|| self.typeof_guard(scope, right, left, eq_positive))
        {
            return Some(fact);
        }

        // null/undefined form: `x === null` (either operand order). Strict only — loose
        // `== null` also matches `undefined`, a different (deferred) rule, so a loose
        // null/undefined comparison is treated as an unrecognized guard (narrows
        // nothing — sound).
        if strict {
            if let Some(fact) = self.nullish_guard(scope, left, right, eq_positive)
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

    /// Try to read `member_side` as a discriminant member access `x.prop` (on a
    /// narrowable identifier `x`) and `literal_side` as a literal expression, producing
    /// a literal-discriminant guard fact targeting `x` (M8). `eq_positive` is the
    /// comparison's positive sense (`===` → `true`); it becomes the then-branch
    /// polarity (`kind === "circle"` keeps the matching members in the then-branch).
    /// The comparison literal is interned (hence `&mut pass`).
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

    /// Read an expression as a **discriminant member access** `x.prop`: a non-optional
    /// static member access whose object is a narrowable identifier `x`. Returns the
    /// `(SymbolId, property name)` to narrow, or `None` if the shape is not recognized
    /// (a computed/optional member, a non-identifier base, a nested member like
    /// `x.a.b`, …) — in which case nothing is narrowed (sound). Keying on the base
    /// **symbol** is what guarantees `x.prop === lit` narrows `x` and never `prop` or
    /// another symbol.
    fn member_discriminant(
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
    fn literal_expr_type(&mut self, expr: &Expression<'_>) -> Option<TypeId> {
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
        let symbol_id = self.binder.graph.resolve(scope, ident.name.as_str())?;
        // Only a value binding (a local/parameter) is narrowable.
        self.binder.symbols.get(symbol_id)?.value?;
        Some(symbol_id)
    }

    /// Analyze a `"prop" in x` expression into an `in`-operator guard fact targeting
    /// `x` (M8). The left operand must be a **string-literal** property name and the
    /// right operand a narrowable identifier `x`. The then-branch (`in` holds) keeps
    /// the members that have the property; an enclosing `!` flips it. Anything else
    /// (a non-literal left, a computed/private `in`, a non-identifier right) narrows
    /// nothing.
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

/// Whether a `switch` clause body **definitely terminates** (does not fall through
/// to the next clause). Conservative: a clause terminates only when its last
/// statement is a `return`/`break`/`throw`, or a block whose last statement is one
/// of those. An empty body, or any other trailing statement, is treated as
/// falling through (so the next clause is checked without this clause's narrowing —
/// sound). This is intentionally simple — full reachability is the flow-node CFG's
/// job (M9+); here it only gates whether the *next* clause may assume its own label.
fn clause_terminates(consequent: &[Statement<'_>]) -> bool {
    match consequent.last() {
        Some(stmt) => statement_terminates(stmt),
        None => false,
    }
}

/// Whether a single statement is a control-flow terminator for the purposes of the
/// conservative fallthrough check: a `return`/`break`/`throw`, or a block ending in
/// one. `continue` is intentionally **not** treated as a terminator (it only
/// applies inside a loop, never to a `switch` clause's fallthrough). Anything else
/// (an expression, declaration, `if`, nested `switch`, …) is treated as
/// non-terminating — conservative, so a clause that might fall through never lets
/// the next clause over-narrow.
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


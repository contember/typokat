//! assignment module (extracted from checker/mod.rs).

use super::classes::body::BodyMemberLookup;
use super::context::*;
use super::expr::{contextual_literal_target, contextual_value_operands};
use super::function_groups::FunctionGroupDemand;
use crate::binder::bind::{ResolvedValueKind, ValueResolution};
use crate::binder::declaration::ValueStorageId;
use crate::binder::scope::ScopeId;
use crate::binder::Binder;
use crate::class_semantics::DemandOutcome;
use crate::diagnostics::{render_type, Diagnostic};
use crate::span::Span;
use crate::types::repr::{ClassId, IntrinsicKind, LiteralValue, TypeTag, Visibility};
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentOperator, AssignmentTarget, BindingPattern,
    ComputedMemberExpression, Expression, ObjectPropertyKind, SimpleAssignmentTarget,
    StaticMemberExpression, TSAsExpression, TSTypeAssertion, VariableDeclarationKind,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy)]
struct MemberAssignmentTarget {
    ty: TypeId,
    visibility: Visibility,
    readonly: bool,
    is_accessor: bool,
    declaring_class: Option<ClassId>,
}

/// Find only assertion expressions embedded in an otherwise deferred assignment target.
struct AssignmentTargetAssertionWalker<'pass, 'a, 'ast, Ticket: Copy + PartialEq> {
    pass: &'pass mut Pass<'a, 'ast, Ticket>,
    scope: ScopeId,
    syntax_only_depth: u32,
}

#[derive(Default)]
struct NestedAssignmentTargetScopeFinder {
    found: bool,
}

impl<'node> Visit<'node> for NestedAssignmentTargetScopeFinder {
    fn visit_expression(&mut self, expression: &Expression<'node>) {
        if matches!(
            expression,
            Expression::ArrowFunctionExpression(_)
                | Expression::ClassExpression(_)
                | Expression::FunctionExpression(_)
        ) {
            self.found = true;
            return;
        }
        walk::walk_expression(self, expression);
    }
}

#[derive(Copy, Clone)]
enum AssertionTargetWalkMode {
    Semantic,
    SyntaxOnly,
}

fn assignment_target_contains_nested_scope(target: &AssignmentTarget<'_>) -> bool {
    let mut finder = NestedAssignmentTargetScopeFinder::default();
    finder.visit_assignment_target(target);
    finder.found
}

pub(in crate::check::checker) fn update_target_contains_nested_scope(
    target: &SimpleAssignmentTarget<'_>,
) -> bool {
    let mut finder = NestedAssignmentTargetScopeFinder::default();
    finder.visit_simple_assignment_target(target);
    finder.found
}

impl<'node, 'a, 'ast, Ticket: Copy + PartialEq> Visit<'node>
    for AssignmentTargetAssertionWalker<'_, 'a, 'ast, Ticket>
{
    fn visit_expression(&mut self, expression: &Expression<'node>) {
        if self.syntax_only_depth > 0 {
            if let Expression::ClassExpression(class) = expression {
                self.pass.record_incomplete(
                    "expr-infer/class-expression/self",
                    Span::from_oxc(class.span),
                    "class expression not modeled",
                );
            }
            walk::walk_expression(self, expression);
            return;
        }
        match expression {
            // Assignment LHS prewalk reserves neither callable owners nor child binder scopes.
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                self.syntax_only_depth += 1;
                walk::walk_expression(self, expression);
                self.syntax_only_depth -= 1;
            }
            Expression::ClassExpression(class) => {
                self.pass.record_incomplete(
                    "expr-infer/class-expression/self",
                    Span::from_oxc(class.span),
                    "class expression not modeled",
                );
                self.syntax_only_depth += 1;
                walk::walk_expression(self, expression);
                self.syntax_only_depth -= 1;
            }
            _ => walk::walk_expression(self, expression),
        }
    }

    fn visit_assignment_target(&mut self, target: &AssignmentTarget<'node>) {
        match target {
            AssignmentTarget::AssignmentTargetIdentifier(_)
            | AssignmentTarget::TSAsExpression(_)
            | AssignmentTarget::TSSatisfiesExpression(_)
            | AssignmentTarget::TSNonNullExpression(_)
            | AssignmentTarget::TSTypeAssertion(_)
            | AssignmentTarget::ComputedMemberExpression(_)
            | AssignmentTarget::StaticMemberExpression(_)
            | AssignmentTarget::PrivateFieldExpression(_) => {
                self.visit_simple_assignment_target(target.to_simple_assignment_target());
            }
            AssignmentTarget::ArrayAssignmentTarget(array) => {
                walk::walk_array_assignment_target(self, array);
            }
            AssignmentTarget::ObjectAssignmentTarget(object) => {
                walk::walk_object_assignment_target(self, object);
            }
        }
    }

    fn visit_simple_assignment_target(&mut self, target: &SimpleAssignmentTarget<'node>) {
        match target {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(_) => {}
            SimpleAssignmentTarget::TSAsExpression(assertion) => {
                self.visit_ts_as_expression(assertion);
            }
            SimpleAssignmentTarget::TSSatisfiesExpression(satisfies) => {
                self.visit_expression(&satisfies.expression);
            }
            SimpleAssignmentTarget::TSNonNullExpression(non_null) => {
                self.visit_expression(&non_null.expression);
            }
            SimpleAssignmentTarget::TSTypeAssertion(assertion) => {
                self.visit_ts_type_assertion(assertion);
            }
            SimpleAssignmentTarget::ComputedMemberExpression(member) => {
                self.visit_expression(&member.object);
                self.visit_expression(&member.expression);
            }
            SimpleAssignmentTarget::StaticMemberExpression(member) => {
                self.visit_expression(&member.object);
            }
            SimpleAssignmentTarget::PrivateFieldExpression(member) => {
                self.visit_expression(&member.object);
            }
        }
    }

    fn visit_ts_as_expression(&mut self, assertion: &TSAsExpression<'node>) {
        if self.syntax_only_depth > 0 {
            self.visit_expression(&assertion.expression);
            self.pass.record_incomplete(
                AssertionSyntax::As.incomplete_id(),
                Span::from_oxc(assertion.span),
                "assertion compatibility deferred with assignment-target nested scope",
            );
            return;
        }
        self.pass.infer_assertion(
            self.scope,
            &assertion.expression,
            &assertion.type_annotation,
            Span::from_oxc(assertion.span),
            AssertionSyntax::As,
        );
    }

    fn visit_ts_type_assertion(&mut self, assertion: &TSTypeAssertion<'node>) {
        if self.syntax_only_depth > 0 {
            self.visit_expression(&assertion.expression);
            self.pass.record_incomplete(
                AssertionSyntax::Angle.incomplete_id(),
                Span::from_oxc(assertion.span),
                "assertion compatibility deferred with assignment-target nested scope",
            );
            return;
        }
        self.pass.infer_assertion(
            self.scope,
            &assertion.expression,
            &assertion.type_annotation,
            Span::from_oxc(assertion.span),
            AssertionSyntax::Angle,
        );
    }
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Check `NAME = expr` against the target's declared type, returning the
    /// assignment expression's **value** (the RHS type + the assignment span) so a
    /// nested assignment (`cond ? (a = e) : …`, `[a = e]`, `s = (a = e)`) is both
    /// checked and usable as an outer source. M23 narrowing effects live only in the
    /// flow graph; this routine just walks the RHS and records assignability.
    pub(in crate::check::checker) fn check_assignment(
        &mut self,
        scope: ScopeId,
        assign: &AssignmentExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let assign_span = Span::from_oxc(assign.span);
        if assignment_target_contains_nested_scope(&assign.left) {
            self.record_incomplete(
                "expr-infer/assignment-expression/nested-scope-target",
                Span::from_oxc(assign.left.span()),
                "assignment target has an unreserved nested scope",
            );
            self.walk_assignment_target_assertions(
                scope,
                &assign.left,
                AssertionTargetWalkMode::SyntaxOnly,
            );
            let rhs = self.infer_expr(scope, &assign.right);
            return rhs.map(|(ty, _)| (ty, assign_span));
        }
        let target = match &assign.left {
            AssignmentTarget::AssignmentTargetIdentifier(target) => target,
            // M14: a **static member** target (`this.prop = …`, `obj.prop = …`) is now
            // type-checked — the RHS must be assignable to the property's type
            // (`TK2322`), and a `readonly` member may not be assigned outside the
            // declaring class's constructor (`TK2540`). Handled in its own routine.
            AssignmentTarget::StaticMemberExpression(member) => {
                return self.check_member_assignment(scope, member, assign);
            }
            AssignmentTarget::ComputedMemberExpression(member) => {
                return self.check_computed_member_assignment(scope, member, assign);
            }
            // These targets remain otherwise deferred; visit only embedded assertions, then walk
            // the RHS once. Identifier/static-member targets retain their dedicated diagnostics.
            AssignmentTarget::TSAsExpression(_)
            | AssignmentTarget::TSTypeAssertion(_)
            | AssignmentTarget::PrivateFieldExpression(_)
            | AssignmentTarget::TSSatisfiesExpression(_)
            | AssignmentTarget::TSNonNullExpression(_)
            | AssignmentTarget::ArrayAssignmentTarget(_)
            | AssignmentTarget::ObjectAssignmentTarget(_) => {
                self.walk_assignment_target_assertions(
                    scope,
                    &assign.left,
                    AssertionTargetWalkMode::Semantic,
                );
                let rhs = self.infer_expr(scope, &assign.right);
                return rhs.map(|(ty, _)| (ty, assign_span));
            }
        };

        let arithmetic = !assign.operator.is_assign() && !assign.operator.is_logical();
        let operation_left = if arithmetic {
            match self.resolve_value_binding(scope, target.name.as_str()) {
                ValueResolution::Resolved {
                    symbol,
                    kind: ResolvedValueKind::Ordinary,
                } => {
                    let span = Span::from_oxc(target.span);
                    match self.demand_function_group(symbol) {
                        FunctionGroupDemand::Ready(ty) | FunctionGroupDemand::PrivateSelf(ty) => {
                            Some(ty)
                        }
                        FunctionGroupDemand::NotGroup => self.resolve_identifier_type(symbol, span),
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
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        let checked_rhs = self.infer_checked_expr(scope, &assign.right);
        let rhs = checked_rhs.as_ref().map(CheckedExpr::tuple);
        // The assignment expression's value is the RHS (TS semantics), regardless of
        // whether a type obligation is collected below.
        let value = rhs.map(|(ty, _)| (ty, assign_span));

        let symbol_id = match self.resolve_value_binding(scope, target.name.as_str()) {
            ValueResolution::Resolved {
                kind: ResolvedValueKind::StandaloneNamespace { .. },
                ..
            } => {
                self.emit_diagnostic(Diagnostic::cannot_assign_namespace(
                    Span::from_oxc(target.span),
                    target.name.as_str(),
                ));
                return value;
            }
            ValueResolution::Resolved { symbol, .. } => symbol,
            ValueResolution::TypeOnlyNamespace { .. } | ValueResolution::Missing => {
                self.emit_diagnostic(Diagnostic::cannot_find_name(
                    Span::from_oxc(target.span),
                    target.name.as_str(),
                ));
                return value;
            }
        };

        if arithmetic {
            let left = operation_left?;
            let (right, _) = rhs?;
            let wk = self.interner.well_known();
            let result = if assign.operator == AssignmentOperator::Addition {
                self.addition_type(left, right).unwrap_or(wk.any)
            } else if left == wk.error || right == wk.error {
                wk.error
            } else {
                wk.number
            };
            return Some((result, assign_span));
        }
        // Logical assignment retains the existing endpoint and deferred obligation policy.
        if assign.operator != AssignmentOperator::Assign {
            return value;
        }

        // The target type is always the symbol's *declared* type (you may assign
        // anything assignable to the declaration, regardless of the current narrowing).
        let target_ty = self
            .binder
            .symbols
            .get(symbol_id)
            .and_then(|s| s.value)
            .and_then(|decl_id| self.decl_type(decl_id));

        if let (Some(tgt), Some(raw)) = (target_ty, checked_rhs) {
            let (checked, rewalk) = self.infer_contextual_checked_source_after_walked_reporting(
                scope,
                &assign.right,
                tgt,
                raw,
                false,
                false,
            );
            if checked.literal.is_none() {
                self.schedule_obligation(AssignObligation {
                    src: checked.ty,
                    tgt,
                    src_span: checked.span,
                    source_sites: Vec::new(),
                    kind: ObligationKind::Assignment,
                });
                return value;
            }
            match rewalk {
                super::expr::ContextualRewalk::Failed
                | super::expr::ContextualRewalk::RewalkedFailed => {}
                super::expr::ContextualRewalk::KeptRaw
                | super::expr::ContextualRewalk::Rewalked => {
                    let group = self.prepare_literal_assignment(&assign.right, &checked, tgt);
                    self.schedule_literal_assignment(group);
                }
            }
        }

        value
    }

    pub(in crate::check::checker) fn prepare_literal_assignment(
        &mut self,
        expr: &Expression<'_>,
        checked: &CheckedExpr,
        target: TypeId,
    ) -> PreparedLiteralAssignment {
        self.semantic_queries.savepoint();
        let mut unavailable_sites = Vec::new();
        let root = self.prepare_literal_site(
            Some(expr),
            checked,
            target,
            checked.span,
            None,
            &mut unavailable_sites,
        );
        if unavailable_sites.is_empty() {
            self.semantic_queries.commit();
        } else {
            self.semantic_queries.rollback();
        }
        PreparedLiteralAssignment {
            root,
            unavailable_sites,
        }
    }

    fn prepare_literal_site(
        &mut self,
        expr: Option<&Expression<'_>>,
        checked: &CheckedExpr,
        target: TypeId,
        span: Span,
        filter: Option<LiteralValueFilter>,
        unavailable_sites: &mut Vec<UnavailableLiteralSite>,
    ) -> LiteralAssignmentSite {
        let mut site = LiteralAssignmentSite {
            residual: AssignObligation {
                src: checked.ty,
                tgt: target,
                src_span: span,
                source_sites: Vec::new(),
                kind: ObligationKind::Assignment,
            },
            children: Vec::new(),
            excess: Vec::new(),
            decomposed: false,
            unavailable: false,
            preparation_failure: None,
            missing_required: false,
        };
        let Some(literal) = &checked.literal else {
            return site;
        };
        debug_assert_eq!(literal.module, self.current_module);
        debug_assert!(self.binder.graph.get(literal.scope).is_some());
        let represented = contextual_literal_target(self.interner.store(), target);
        let shape = if matches!(
            literal.kind,
            LiteralKind::Parenthesized | LiteralKind::Conditional | LiteralKind::Logical
        ) || is_excess_literal_shape(self.interner.store(), represented)
        {
            represented
        } else {
            match self.demand_structural_apparent_type(target) {
                DemandOutcome::Ready(shape) => {
                    contextual_literal_target(self.interner.store(), shape)
                }
                DemandOutcome::Exhausted(exhaustion) => {
                    site.preparation_failure = Some(exhaustion.clone());
                    unavailable_sites.push(UnavailableLiteralSite {
                        span,
                        target,
                        evidence: LiteralUnavailableEvidence::Demand(exhaustion),
                    });
                    site.unavailable = true;
                    return site;
                }
            }
        };
        let object = self.interner.store().object_type(shape).cloned();
        let tuple = self.interner.store().tuple_type(shape).cloned();
        let array = self
            .interner
            .store()
            .array_type(shape)
            .map(|array| array.element);
        site.decomposed = match literal.kind {
            LiteralKind::Object => object.is_some(),
            LiteralKind::Array => tuple.is_some() || array.is_some(),
            LiteralKind::Parenthesized | LiteralKind::Conditional | LiteralKind::Logical => true,
        };
        if !site.decomposed {
            if let Some(expr) = expr {
                match self.check_excess_properties_for_target(expr, target) {
                    DemandOutcome::Ready(excess) => site.excess = excess,
                    DemandOutcome::Exhausted(exhaustion) => {
                        site.preparation_failure = Some(exhaustion.clone());
                        unavailable_sites.push(UnavailableLiteralSite {
                            span,
                            target,
                            evidence: LiteralUnavailableEvidence::Demand(exhaustion),
                        });
                        site.unavailable = true;
                    }
                }
            }
            return site;
        }
        if let (Some(target), Some(source)) =
            (&object, self.interner.store().object_type(checked.ty))
        {
            site.missing_required = target.call_signatures.is_empty()
                && target.construct_signatures.is_empty()
                && target.properties.iter().any(|property| {
                    !property.optional && source.property_by_key(&property.key).is_none()
                });
        }
        for child in literal.children.iter() {
            if !child.required {
                continue;
            }
            let mut child_span = child.full_span;
            let child_target = match &child.edge {
                LiteralEdge::Property {
                    name: Some(name),
                    key_span,
                    ..
                } => {
                    child_span = *key_span;
                    let Some(object) = &object else {
                        continue;
                    };
                    let target = object.property(name).map(|p| p.ty).or_else(|| {
                        if is_numeric_property_name(name) {
                            object.number_index.or(object.string_index)
                        } else {
                            object.string_index
                        }
                    });
                    if target.is_none()
                        && (!object.properties.is_empty()
                            || !object.call_signatures.is_empty()
                            || !object.construct_signatures.is_empty())
                    {
                        site.excess.push(Diagnostic::excess_property(
                            *key_span,
                            name,
                            &render_type(self.interner.store(), shape, false),
                        ));
                    }
                    target
                }
                LiteralEdge::Property { name: None, .. } => None,
                LiteralEdge::Element(index) => tuple
                    .as_ref()
                    .and_then(|tuple| {
                        if tuple
                            .rest
                            .as_ref()
                            .is_none_or(|rest| *index < rest.position)
                        {
                            return tuple.elements.get(*index).copied();
                        }
                        match expr {
                            Some(Expression::ArrayExpression(array)) => {
                                self.tuple_context_element(tuple, *index, array.elements.len())
                            }
                            _ => None,
                        }
                    })
                    .or(array),
                LiteralEdge::Parenthesized
                | LiteralEdge::Consequent
                | LiteralEdge::Alternate
                | LiteralEdge::LogicalLeft(_)
                | LiteralEdge::LogicalRight => Some(target),
            };
            let Some(child_target) = child_target else {
                continue;
            };
            let Some(source) = &child.checked else {
                unavailable_sites.push(UnavailableLiteralSite {
                    span: child.full_span,
                    target: child_target,
                    evidence: LiteralUnavailableEvidence::SourceWalk,
                });
                site.unavailable = true;
                continue;
            };
            let child_filter = match child.edge {
                LiteralEdge::Property { .. } | LiteralEdge::Element(_) => None,
                LiteralEdge::LogicalLeft(local) => Some(match (filter, local) {
                    (Some(LiteralValueFilter::Truthy), _) | (_, LiteralValueFilter::Truthy) => {
                        LiteralValueFilter::Truthy
                    }
                    _ => LiteralValueFilter::NonNullish,
                }),
                _ => filter,
            };
            let mut ty = if matches!(child.edge, LiteralEdge::LogicalLeft(_)) {
                child.contribution.unwrap_or(source.ty)
            } else {
                source.ty
            };
            if let Some(filter) = child_filter {
                ty = match filter {
                    LiteralValueFilter::Truthy => {
                        crate::check::flow::narrow_by_truthiness(self.interner, ty, true)
                    }
                    LiteralValueFilter::NonNullish => self.strip_nullish(ty),
                };
                if ty == self.interner.well_known().never {
                    continue;
                }
            }
            let contribution = CheckedExpr {
                ty,
                span: source.span,
                literal: source.literal.clone(),
            };
            let child_expr = expr.and_then(|expr| literal_child_expression(expr, &child.edge));
            site.children.push(self.prepare_literal_site(
                child_expr,
                &contribution,
                child_target,
                child_span,
                child_filter,
                unavailable_sites,
            ));
        }
        site
    }

    /// Preserve deferred computed assignments, except statically known readonly
    /// targets report `TK2540`.
    fn check_computed_member_assignment(
        &mut self,
        scope: ScopeId,
        member: &ComputedMemberExpression<'_>,
        assign: &AssignmentExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let base = self.infer_expr(scope, &member.object);
        let key = self.infer_expr(scope, &member.expression);
        let rhs = self.infer_expr(scope, &assign.right);
        let value = rhs.map(|(ty, _)| (ty, Span::from_oxc(assign.span)));
        if assign.operator != AssignmentOperator::Assign {
            return value;
        }

        let Some((base_ty, _)) = base else {
            return value;
        };
        let apparent = self.apparent_type(base_ty);
        let readonly_collection = readonly_collection_target(
            self.interner.store(),
            apparent,
            key.map(|(ty, _)| ty),
            member.static_property_name().as_deref(),
        );
        let static_name = member
            .static_property_name()
            .map(|name| name.to_string())
            .or_else(|| key.and_then(|(ty, _)| literal_property_name(self.interner.store(), ty)));
        let property_span = Span::from_oxc(member.expression.span());
        let mut found = None;
        if let Some(name) = static_name.as_deref() {
            let wk = self.interner.well_known();
            if base_ty != wk.error && base_ty != wk.any {
                if let Some(DemandOutcome::Exhausted(exhaustion)) =
                    self.demand_class_value_surface(scope, &member.object)
                {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), property_span);
                    return value;
                }
                let structural = match self.demand_structural_apparent_type(base_ty) {
                    DemandOutcome::Ready(structural) => self
                        .interner
                        .store()
                        .readonly_operand(structural)
                        .unwrap_or(structural),
                    DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(DemandOutcome::Exhausted(exhaustion), property_span);
                        return value;
                    }
                };
                found = match self.structural_member_assignment_target(structural, name) {
                    DemandOutcome::Ready(found) => found,
                    DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(DemandOutcome::Exhausted(exhaustion), property_span);
                        return value;
                    }
                };
                if let Some(target) = found {
                    self.check_member_access_control(
                        name,
                        property_span,
                        target.visibility,
                        target.declaring_class,
                    );
                }
            }
        }
        let readonly_property = found.is_some_and(|target| target.readonly);
        if !readonly_collection && !readonly_property {
            return value;
        }
        if !readonly_collection
            && found.is_some_and(|target| {
                self.readonly_member_assignment_allowed(&member.object, target)
            })
        {
            return value;
        }

        let name = static_name.unwrap_or_else(|| "computed".to_owned());
        self.emit_diagnostic(Diagnostic::readonly_assignment(property_span, &name));
        value
    }

    fn walk_assignment_target_assertions(
        &mut self,
        scope: ScopeId,
        target: &AssignmentTarget<'_>,
        mode: AssertionTargetWalkMode,
    ) {
        let mut walker = AssignmentTargetAssertionWalker {
            pass: self,
            scope,
            syntax_only_depth: match mode {
                AssertionTargetWalkMode::Semantic => 0,
                AssertionTargetWalkMode::SyntaxOnly => 1,
            },
        };
        walker.visit_assignment_target(target);
    }

    pub(in crate::check::checker) fn walk_update_target_assertions_syntax_only(
        &mut self,
        scope: ScopeId,
        target: &SimpleAssignmentTarget<'_>,
    ) {
        let mut walker = AssignmentTargetAssertionWalker {
            pass: self,
            scope,
            syntax_only_depth: 1,
        };
        walker.visit_simple_assignment_target(target);
    }

    /// Check M14 member assignment, returning the assignment's value (the RHS type +
    /// the assignment span) for use as an outer source. Base/RHS are walked first;
    /// local lookup avoids read-side diagnostics, and compound member assignments
    /// remain deferred.
    fn check_member_assignment(
        &mut self,
        scope: ScopeId,
        member: &StaticMemberExpression<'_>,
        assign: &AssignmentExpression<'_>,
    ) -> Option<(TypeId, Span)> {
        let wk = self.interner.well_known();
        let assign_span = Span::from_oxc(assign.span);

        // Walk the base (so its references resolve / nested constructs are checked) and the
        // RHS (so an unresolved name in it emits `TK2304`, and nested calls/functions are
        // checked), regardless of operator — both must be walked even when no obligation is
        // collected below.
        let base = self.infer_expr(scope, &member.object);
        let rhs = self.infer_expr(scope, &assign.right);
        let value = rhs.map(|(ty, _)| (ty, assign_span));

        // Resolve the base type. Absent → nothing to check.
        let Some((base_ty, _)) = base else {
            return value;
        };

        let prop_name = member.property.name.as_str();
        let property_span = Span::from_oxc(member.property.span);
        let body_this_found = match self.body_this_member_lookup(&member.object, prop_name) {
            Some(BodyMemberLookup::Known {
                ty,
                write_ty,
                metadata,
            }) => Some(MemberAssignmentTarget {
                ty: write_ty.unwrap_or(ty),
                visibility: metadata.visibility,
                readonly: metadata.readonly,
                is_accessor: metadata.is_accessor,
                declaring_class: metadata.declaring_class,
            }),
            Some(BodyMemberLookup::Unavailable(metadata)) => {
                self.check_member_access_control(
                    prop_name,
                    property_span,
                    metadata.visibility,
                    metadata.declaring_class,
                );
                return value;
            }
            Some(BodyMemberLookup::Missing { .. }) => return value,
            None => None,
        };

        // Skip when the base is the error/`any` type (an unresolved base, or `any`): there is
        // no concrete property to check against, and the error type suppresses cascades.
        if (base_ty == wk.error || base_ty == wk.any) && body_this_found.is_none() {
            return value;
        }

        // Member writes use the same apparent/merged base type as reads, so
        // constrained parameters and intersections expose their real property type.
        if let Some(DemandOutcome::Exhausted(exhaustion)) =
            self.demand_class_value_surface(scope, &member.object)
        {
            self.own_type_demand(DemandOutcome::Exhausted(exhaustion), property_span);
            return None;
        }
        let base_ty = if body_this_found.is_some() {
            base_ty
        } else {
            match self.demand_structural_apparent_type(base_ty) {
                DemandOutcome::Ready(base_ty) => base_ty,
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), property_span);
                    return None;
                }
            }
        };
        // Look up the property on the base **object** type. Snapshot the property's
        // type + `readonly` + `is_accessor` + origin before any `&mut` borrow for a
        // diagnostic.
        let found = match body_this_found {
            Some(found) => Some(found),
            None => match self.structural_member_assignment_target(base_ty, prop_name) {
                DemandOutcome::Ready(found) => found,
                DemandOutcome::Exhausted(exhaustion) => {
                    self.own_type_demand(DemandOutcome::Exhausted(exhaustion), property_span);
                    return None;
                }
            },
        };

        // Property not on the type (or missing on some union member) → deferred (no
        // `TK2339` on an assignment target).
        let Some(MemberAssignmentTarget {
            ty: prop_ty,
            visibility,
            readonly,
            is_accessor,
            declaring_class,
        }) = found
        else {
            return value;
        };

        self.check_member_access_control(prop_name, property_span, visibility, declaring_class);

        // Compound assignment operators remain outside the assignability/readonly subset,
        // but access control applies to their target.
        if assign.operator != AssignmentOperator::Assign {
            return value;
        }

        // A property whose type is the error type carries no real obligation.
        if prop_ty == wk.error {
            return value;
        }

        // `readonly` data fields may be assigned only as `this.prop` in the declaring
        // constructor. Get-only accessors are read-only everywhere, so the carve-out
        // is gated on `!is_accessor`.
        if readonly
            && !self.readonly_member_assignment_allowed(
                &member.object,
                MemberAssignmentTarget {
                    ty: prop_ty,
                    visibility,
                    readonly,
                    is_accessor,
                    declaring_class,
                },
            )
        {
            self.emit_diagnostic(Diagnostic::readonly_assignment(
                Span::from_oxc(member.property.span),
                member.property.name.as_str(),
            ));
            // `TK2540` is the assignment error for this target; do not also collect a
            // type-assignability obligation (the fixtures' readonly RHS types already
            // match, and tsc surfaces the read-only violation as the assignment error).
            return value;
        }
        // Allowed (declaring constructor): fall through to the normal type check, so a
        // *type-wrong* `this.readonly = …` in the constructor is still `TK2322`.

        // Type-assignability obligation: the RHS must be assignable to the property's type.
        // Skipped when the RHS type is the error type (incomplete inference — see the doc
        // comment); the primary span is the RHS.
        if let Some(raw) = rhs {
            let (src, src_span) = self.infer_contextual_source_after_walked(
                scope,
                &assign.right,
                prop_ty,
                raw,
                false,
                false,
            );
            if src != wk.error {
                match self.check_excess_properties_for_target(&assign.right, prop_ty) {
                    DemandOutcome::Ready(diagnostics) => {
                        for diagnostic in diagnostics {
                            self.emit_diagnostic(diagnostic);
                        }
                    }
                    DemandOutcome::Exhausted(exhaustion) => {
                        self.own_type_demand(DemandOutcome::Exhausted(exhaustion), src_span);
                        return value;
                    }
                }
                self.schedule_obligation(AssignObligation {
                    src,
                    tgt: prop_ty,
                    src_span,
                    source_sites: Vec::new(),
                    kind: ObligationKind::Assignment,
                });
            }
        }

        value
    }

    fn readonly_member_assignment_allowed(
        &self,
        object: &Expression<'_>,
        target: MemberAssignmentTarget,
    ) -> bool {
        !target.is_accessor
            && self.current_in_ctor
            && base_is_this(object)
            && matches!(
                (self.current_class, target.declaring_class),
                (Some(context), Some(owner)) if context == owner
            )
    }

    fn structural_member_assignment_target(
        &mut self,
        base_ty: TypeId,
        prop_name: &str,
    ) -> DemandOutcome<Option<MemberAssignmentTarget>> {
        let object_found = self
            .interner
            .store()
            .object_type(base_ty)
            .and_then(|object| object.property(prop_name))
            .map(|property| MemberAssignmentTarget {
                ty: property.write_ty.unwrap_or(property.ty),
                visibility: property.visibility,
                readonly: property.readonly,
                is_accessor: property.is_accessor,
                declaring_class: property.declaring_class,
            });
        match object_found {
            Some(found) => DemandOutcome::Ready(Some(found)),
            None => self.union_member_assignment_target(base_ty, prop_name),
        }
    }

    /// Resolve a union assignment target. The property must exist on every member;
    /// the write type is the union of member property types, and readonly is true if
    /// any constituent is readonly. A union base is never `this`, so constructor
    /// carve-outs do not apply.
    fn union_member_assignment_target(
        &mut self,
        base_ty: TypeId,
        prop_name: &str,
    ) -> DemandOutcome<Option<MemberAssignmentTarget>> {
        self.with_semantic_query_transaction(|pass| {
            pass.union_member_assignment_target_inner(base_ty, prop_name)
        })
    }

    fn union_member_assignment_target_inner(
        &mut self,
        base_ty: TypeId,
        prop_name: &str,
    ) -> DemandOutcome<Option<MemberAssignmentTarget>> {
        // Snapshot the member ids: the per-member lookups are immutable, but interning the
        // result union below needs `&mut`, so the borrow must not be held across it.
        let Some(members) = self.interner.store().union_members(base_ty) else {
            return DemandOutcome::Ready(None);
        };
        let members = members.to_vec();

        let mut member_prop_types: Vec<TypeId> = Vec::with_capacity(members.len());
        let mut any_readonly = false;
        let mut any_accessor = false;
        let mut access = None;
        for member in members {
            // M24 (audit): a union member that is a constrained type parameter resolves
            // through its apparent type, mirroring the read side.
            let member = match self.demand_apparent_type(member) {
                DemandOutcome::Ready(member) => member,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
            let Some((ty, visibility, declaring_class, readonly, is_accessor)) = self
                .interner
                .store()
                .object_type(member)
                .and_then(|object| object.property(prop_name))
                .map(|property| {
                    (
                        property.write_ty.unwrap_or(property.ty),
                        property.visibility,
                        property.declaring_class,
                        property.readonly,
                        property.is_accessor,
                    )
                })
            else {
                return DemandOutcome::Ready(None);
            };
            member_prop_types.push(ty);
            any_readonly |= readonly;
            any_accessor |= is_accessor;
            if access.is_none() || matches!(visibility, Visibility::Private | Visibility::Protected)
            {
                access = Some((visibility, declaring_class));
            }
        }

        let prop_ty = self.interner.union(member_prop_types);
        let (visibility, declaring_class) = access.unwrap_or((Visibility::Public, None));
        DemandOutcome::Ready(Some(MemberAssignmentTarget {
            ty: prop_ty,
            visibility,
            readonly: any_readonly,
            is_accessor: any_accessor,
            declaring_class,
        }))
    }

    /// Demand a contextual/excess-property target before the immutable literal
    /// walker consumes it. Composite class constituents promote atomically.
    pub(in crate::check::checker) fn check_excess_properties_for_target(
        &mut self,
        expr: &Expression<'_>,
        target_ty: TypeId,
    ) -> DemandOutcome<Vec<Diagnostic>> {
        self.with_semantic_query_transaction(|pass| {
            let mut normalized = FxHashMap::default();
            let demanded = match pass.demand_structural_apparent_type_inner(target_ty) {
                DemandOutcome::Ready(demanded) => demanded,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            };
            normalized.insert(target_ty, demanded);
            normalized.insert(demanded, demanded);
            match pass.prepare_declared_excess_property_targets(expr, demanded, &mut normalized) {
                DemandOutcome::Ready(()) => DemandOutcome::Ready(check_excess_properties(
                    pass.interner.store(),
                    expr,
                    target_ty,
                    &normalized,
                )),
                DemandOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
            }
        })
    }

    fn prepare_declared_excess_property_targets(
        &mut self,
        expr: &Expression<'_>,
        target_ty: TypeId,
        normalized: &mut FxHashMap<TypeId, TypeId>,
    ) -> DemandOutcome<()> {
        let demanded = match self.demand_declared_literal_shape(target_ty, normalized) {
            DemandOutcome::Ready(demanded) => demanded,
            DemandOutcome::Exhausted(exhaustion) => {
                return DemandOutcome::Exhausted(exhaustion);
            }
        };
        let target_ty = contextual_literal_target(self.interner.store(), demanded);

        match expr {
            Expression::ParenthesizedExpression(parenthesized) => self
                .prepare_declared_excess_property_targets(
                    &parenthesized.expression,
                    target_ty,
                    normalized,
                ),
            Expression::ObjectExpression(literal) => {
                let Some(target) = self.interner.store().object_type(target_ty).cloned() else {
                    return DemandOutcome::Ready(());
                };
                for member in &literal.properties {
                    let ObjectPropertyKind::ObjectProperty(property) = member else {
                        continue;
                    };
                    let Some(name) = property.key.static_name() else {
                        continue;
                    };
                    let child = match target.property(&name) {
                        Some(target_property) => Some(target_property.ty),
                        None if is_numeric_property_name(&name) => {
                            target.number_index.or(target.string_index)
                        }
                        None => target.string_index,
                    };
                    let Some(child) = child else {
                        continue;
                    };
                    match self.prepare_declared_excess_property_targets(
                        &property.value,
                        child,
                        normalized,
                    ) {
                        DemandOutcome::Ready(()) => {}
                        DemandOutcome::Exhausted(exhaustion) => {
                            return DemandOutcome::Exhausted(exhaustion);
                        }
                    }
                }
                DemandOutcome::Ready(())
            }
            Expression::ArrayExpression(array) => {
                let children = if let Some(tuple) = self.interner.store().tuple_type(target_ty) {
                    array
                        .elements
                        .iter()
                        .enumerate()
                        .filter_map(|(index, element)| {
                            Some((element.as_expression()?, *tuple.elements.get(index)?))
                        })
                        .collect::<Vec<_>>()
                } else if let Some(array_type) = self.interner.store().array_type(target_ty) {
                    array
                        .elements
                        .iter()
                        .filter_map(|element| {
                            element
                                .as_expression()
                                .map(|expression| (expression, array_type.element))
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                for (expression, child) in children {
                    match self
                        .prepare_declared_excess_property_targets(expression, child, normalized)
                    {
                        DemandOutcome::Ready(()) => {}
                        DemandOutcome::Exhausted(exhaustion) => {
                            return DemandOutcome::Exhausted(exhaustion);
                        }
                    }
                }
                DemandOutcome::Ready(())
            }
            // Backlog 104: a ternary / logical is not itself a literal, but its shaped
            // operands are — the same target has to reach them here too, or the walk
            // below finds an unresolved declared child target.
            expr => {
                let Some(operands) = contextual_value_operands(expr) else {
                    return DemandOutcome::Ready(());
                };
                for operand in operands.into_iter().flatten() {
                    match self
                        .prepare_declared_excess_property_targets(operand, target_ty, normalized)
                    {
                        DemandOutcome::Ready(()) => {}
                        DemandOutcome::Exhausted(exhaustion) => {
                            return DemandOutcome::Exhausted(exhaustion);
                        }
                    }
                }
                DemandOutcome::Ready(())
            }
        }
    }

    fn demand_declared_literal_shape(
        &mut self,
        target_ty: TypeId,
        normalized: &mut FxHashMap<TypeId, TypeId>,
    ) -> DemandOutcome<TypeId> {
        let tag = self.interner.store().tag(target_ty);
        let alias_application = tag == TypeTag::Instantiation
            && self
                .interner
                .store()
                .instantiation_type(target_ty)
                .is_some_and(|application| {
                    self.interner.store().tag(application.base) == TypeTag::AliasTemplate
                });
        let demanded = match tag {
            TypeTag::Declared => match self.demand_apparent_type(target_ty) {
                DemandOutcome::Ready(demanded) => demanded,
                DemandOutcome::Exhausted(exhaustion) => {
                    return DemandOutcome::Exhausted(exhaustion);
                }
            },
            TypeTag::Instantiation if alias_application => {
                match self.demand_apparent_type(target_ty) {
                    DemandOutcome::Ready(demanded) => demanded,
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                }
            }
            TypeTag::Readonly => {
                let Some(operand) = self.interner.store().readonly_operand(target_ty) else {
                    return DemandOutcome::Ready(target_ty);
                };
                let operand = match self.demand_declared_literal_shape(operand, normalized) {
                    DemandOutcome::Ready(operand) => operand,
                    DemandOutcome::Exhausted(exhaustion) => {
                        return DemandOutcome::Exhausted(exhaustion);
                    }
                };
                if is_excess_literal_shape(self.interner.store(), operand) {
                    operand
                } else {
                    target_ty
                }
            }
            TypeTag::Union => {
                let Some(members) = self.interner.store().union_members(target_ty) else {
                    return DemandOutcome::Ready(target_ty);
                };
                let members = members.to_vec();
                let mut shape = None;
                for member in members {
                    if self.interner.store().intrinsic_kind(member)
                        == Some(crate::types::repr::IntrinsicKind::Undefined)
                    {
                        continue;
                    }
                    if shape.is_some() {
                        return DemandOutcome::Ready(target_ty);
                    }
                    let member = match self.demand_declared_literal_shape(member, normalized) {
                        DemandOutcome::Ready(member) => member,
                        DemandOutcome::Exhausted(exhaustion) => {
                            return DemandOutcome::Exhausted(exhaustion);
                        }
                    };
                    if !is_excess_literal_shape(self.interner.store(), member) {
                        return DemandOutcome::Ready(target_ty);
                    }
                    shape = Some(member);
                }
                shape.unwrap_or(target_ty)
            }
            _ => target_ty,
        };
        if demanded != target_ty {
            normalized.insert(target_ty, demanded);
            normalized.insert(demanded, demanded);
        }
        DemandOutcome::Ready(demanded)
    }
}

fn is_excess_literal_shape(store: &Store, ty: TypeId) -> bool {
    matches!(
        store.tag(ty),
        TypeTag::Object | TypeTag::Array | TypeTag::Tuple
    )
}

fn literal_child_expression<'e, 'ast>(
    expr: &'e Expression<'ast>,
    edge: &LiteralEdge,
) -> Option<&'e Expression<'ast>> {
    match (expr, edge) {
        (Expression::ObjectExpression(object), LiteralEdge::Property { ordinal, .. }) => {
            match object.properties.get(*ordinal)? {
                ObjectPropertyKind::ObjectProperty(property) => Some(&property.value),
                _ => None,
            }
        }
        (Expression::ArrayExpression(array), LiteralEdge::Element(index)) => {
            array.elements.get(*index)?.as_expression()
        }
        (Expression::ParenthesizedExpression(paren), LiteralEdge::Parenthesized) => {
            Some(&paren.expression)
        }
        (Expression::ConditionalExpression(cond), LiteralEdge::Consequent) => {
            Some(&cond.consequent)
        }
        (Expression::ConditionalExpression(cond), LiteralEdge::Alternate) => Some(&cond.alternate),
        (Expression::LogicalExpression(logical), LiteralEdge::LogicalLeft(_)) => {
            Some(&logical.left)
        }
        (Expression::LogicalExpression(logical), LiteralEdge::LogicalRight) => Some(&logical.right),
        _ => None,
    }
}

/// Recursive excess-property check. M19 index signatures suppress unknown keys
/// only on the target that owns the signature; plain object targets still run the
/// M2 freshness check, including at nested levels.
pub(in crate::check::checker) fn check_excess_properties(
    store: &Store,
    expr: &Expression<'_>,
    target_ty: TypeId,
    normalized: &FxHashMap<TypeId, TypeId>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    collect_excess_properties(store, expr, target_ty, normalized, &mut diagnostics);
    diagnostics
}

fn collect_excess_properties(
    store: &Store,
    expr: &Expression<'_>,
    target_ty: TypeId,
    normalized: &FxHashMap<TypeId, TypeId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let target_ty = normalized.get(&target_ty).copied().unwrap_or(target_ty);
    let target_ty = contextual_literal_target(store, target_ty);
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            collect_excess_properties(store, &paren.expression, target_ty, normalized, diagnostics);
        }
        Expression::ObjectExpression(literal) => {
            // M31: a fresh literal against an intersection target is excess-checked against
            // the MERGED key set (a single check, never per-member — see the function).
            if store.tag(target_ty) == TypeTag::Intersection {
                check_intersection_object_excess(
                    store,
                    literal,
                    target_ty,
                    normalized,
                    diagnostics,
                );
            } else {
                check_object_excess_properties(store, literal, target_ty, normalized, diagnostics);
            }
        }
        Expression::ArrayExpression(array) => {
            check_array_excess_properties(store, array, target_ty, normalized, diagnostics);
        }
        // Backlog 104: freshness survives a ternary / logical — tsc checks each shaped
        // operand against the same contextual target, so the walk descends instead of
        // stopping at the first non-literal node.
        expr => {
            let Some(operands) = contextual_value_operands(expr) else {
                return;
            };
            for operand in operands.into_iter().flatten() {
                collect_excess_properties(store, operand, target_ty, normalized, diagnostics);
            }
        }
    }
}

fn check_object_excess_properties(
    store: &Store,
    literal: &oxc_ast::ast::ObjectExpression<'_>,
    target_ty: TypeId,
    normalized: &FxHashMap<TypeId, TypeId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(target_obj) = store.object_type(target_ty) else {
        return;
    };
    if target_obj.properties.is_empty()
        && target_obj.string_index.is_none()
        && target_obj.number_index.is_none()
        && target_obj.call_signatures.is_empty()
        && target_obj.construct_signatures.is_empty()
    {
        return;
    }
    // Snapshot index value types: string indexes accept any key, number indexes
    // accept numeric-named keys, and fresh index values still get checked.
    let string_index = target_obj.string_index;
    let number_index = target_obj.number_index;
    let target_rendered = render_type(store, target_ty, /* widen */ false);

    for member in &literal.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = member else {
            continue;
        };
        let Some(name) = prop.key.static_name() else {
            continue;
        };

        match target_obj.property(&name) {
            Some(target_prop) => {
                // Optional members are `T | undefined`; recurse through the object
                // part instead of bailing on the union container.
                let recurse_ty = excess_recursion_target(store, target_prop.ty);
                collect_excess_properties(store, &prop.value, recurse_ty, normalized, diagnostics);
            }
            None => {
                // M19: index signatures suppress accepted keys; numeric keys prefer
                // the number index, otherwise the string index covers them.
                let index_value = if is_numeric_property_name(&name) {
                    number_index.or(string_index)
                } else {
                    string_index
                };
                match index_value {
                    Some(value_ty) => {
                        // The key is accepted, BUT a fresh object literal used as the
                        // value still gets its own excess check against the index value
                        // type (the helper self-guards: a no-op unless `prop.value` is a
                        // fresh object literal and `value_ty` is an object type).
                        collect_excess_properties(
                            store,
                            &prop.value,
                            value_ty,
                            normalized,
                            diagnostics,
                        );
                    }
                    None => {
                        diagnostics.push(Diagnostic::excess_property(
                            Span::from_oxc(prop.key.span()),
                            &name,
                            &target_rendered,
                        ));
                    }
                }
            }
        }
    }
}

/// Excess-check an intersection target against the merged key set (M31), never
/// per member. Per-member checks would spuriously reject keys supplied by a
/// different constituent.
fn check_intersection_object_excess(
    store: &Store,
    literal: &oxc_ast::ast::ObjectExpression<'_>,
    ty: TypeId,
    normalized: &FxHashMap<TypeId, TypeId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(members) = store.intersection_members(ty) else {
        return;
    };
    check_object_excess_against_members(store, literal, members, normalized, diagnostics);
}

/// Excess-check a fresh object literal against a merged member set. Matched keys
/// recurse through all contributing members so later constituents are not missed.
fn check_object_excess_against_members(
    store: &Store,
    literal: &oxc_ast::ast::ObjectExpression<'_>,
    members: &[TypeId],
    normalized: &FxHashMap<TypeId, TypeId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // The rendered target joins the members with ` & ` (matches the intersection's own
    // render; excess targets are asserted code-only in the corpus).
    let target_rendered = members
        .iter()
        .map(|&m| render_type(store, m, /* widen */ false))
        .collect::<Vec<_>>()
        .join(" & ");

    for member in &literal.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = member else {
            continue;
        };
        let Some(name) = prop.key.static_name() else {
            continue;
        };

        // Every member that names this key contributes its property type (the merged
        // source value is their intersection). A key named by ≥ 1 member is allowed —
        // recurse against the WHOLE contributing set for nested freshness.
        let contributors: Vec<TypeId> = members
            .iter()
            .filter_map(|&m| {
                store
                    .object_type(m)
                    .and_then(|o| o.property(&name))
                    .map(|p| p.ty)
            })
            .collect();
        if !contributors.is_empty() {
            check_excess_against_members(
                store,
                &prop.value,
                &contributors,
                normalized,
                diagnostics,
            );
            continue;
        }

        // Not a named member — an index signature on some member may accept it (a string
        // index for any key, a number index for a numeric-named key).
        let numeric = is_numeric_property_name(&name);
        let index_value = members.iter().find_map(|&m| {
            let obj = store.object_type(m)?;
            if numeric {
                obj.number_index.or(obj.string_index)
            } else {
                obj.string_index
            }
        });
        match index_value {
            Some(value_ty) => {
                check_excess_against_members(
                    store,
                    &prop.value,
                    &[value_ty],
                    normalized,
                    diagnostics,
                );
            }
            None => {
                diagnostics.push(Diagnostic::excess_property(
                    Span::from_oxc(prop.key.span()),
                    &name,
                    &target_rendered,
                ));
            }
        }
    }
}

/// Recurse excess checks against a merged member set. Singletons delegate to the
/// ordinary path; multi-member non-object values are out of the M31 subset.
fn check_excess_against_members(
    store: &Store,
    expr: &Expression<'_>,
    members: &[TypeId],
    normalized: &FxHashMap<TypeId, TypeId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match members {
        [] => {}
        [single] => collect_excess_properties(store, expr, *single, normalized, diagnostics),
        _ => match expr {
            Expression::ParenthesizedExpression(paren) => {
                check_excess_against_members(
                    store,
                    &paren.expression,
                    members,
                    normalized,
                    diagnostics,
                );
            }
            Expression::ObjectExpression(literal) => {
                check_object_excess_against_members(
                    store,
                    literal,
                    members,
                    normalized,
                    diagnostics,
                );
            }
            _ => {}
        },
    }
}

fn check_array_excess_properties(
    store: &Store,
    array: &oxc_ast::ast::ArrayExpression<'_>,
    target_ty: TypeId,
    normalized: &FxHashMap<TypeId, TypeId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(tuple) = store.tuple_type(target_ty) {
        for (index, element) in array.elements.iter().enumerate() {
            let Some(expr) = element.as_expression() else {
                continue;
            };
            let Some(target) = tuple.elements.get(index).copied() else {
                continue;
            };
            let target = excess_recursion_target(store, target);
            collect_excess_properties(store, expr, target, normalized, diagnostics);
        }
        return;
    }

    let Some(array_ty) = store.array_type(target_ty) else {
        return;
    };
    let target = excess_recursion_target(store, array_ty.element);
    for element in &array.elements {
        let Some(expr) = element.as_expression() else {
            continue;
        };
        collect_excess_properties(store, expr, target, normalized, diagnostics);
    }
}

/// The concrete target to descend into for a freshness (excess) check on a matched
/// member or array element. Optional-style unions (`T | undefined`) recurse through
/// their single object/array/tuple member; every other union stays unchanged.
fn excess_recursion_target(store: &Store, ty: TypeId) -> TypeId {
    contextual_literal_target(store, ty)
}

/// Whether a property name is numeric-keyed for M19 number index signatures,
/// matching the relation engine's suppression rule.
fn is_numeric_property_name(name: &str) -> bool {
    name.parse::<f64>().map(|n| n.is_finite()).unwrap_or(false)
}

fn readonly_collection_target(
    store: &Store,
    ty: TypeId,
    key_ty: Option<TypeId>,
    static_name: Option<&str>,
) -> bool {
    if let Some(operand) = store.readonly_operand(ty) {
        if let Some(tuple) = store.tuple_type(operand) {
            return collection_index(store, key_ty, static_name)
                .is_some_and(|index| index.is_none_or(|index| index < tuple.elements.len()));
        }
        if store.array_type(operand).is_some() {
            return collection_index(store, key_ty, static_name).is_some();
        }
        return false;
    }
    store.union_members(ty).is_some_and(|members| {
        members
            .iter()
            .any(|member| readonly_collection_target(store, *member, key_ty, static_name))
    })
}

fn collection_index(
    store: &Store,
    key_ty: Option<TypeId>,
    static_name: Option<&str>,
) -> Option<Option<usize>> {
    if let Some(name) = static_name {
        let index = name.parse::<usize>().ok()?;
        return (index.to_string() == name).then_some(Some(index));
    }
    let key_ty = key_ty?;
    match store.literal_value(key_ty) {
        Some(LiteralValue::String(name)) => {
            let index = name.parse::<usize>().ok()?;
            (index.to_string() == *name).then_some(Some(index))
        }
        Some(LiteralValue::Number(value)) if *value >= 0.0 && value.fract() == 0.0 => {
            let value = if *value == 0.0 {
                "0".to_string()
            } else {
                value.to_string()
            };
            value.parse::<usize>().ok().map(Some)
        }
        _ if store.intrinsic_kind(key_ty) == Some(IntrinsicKind::Number) => Some(None),
        _ => None,
    }
}

fn literal_property_name(store: &Store, ty: TypeId) -> Option<String> {
    match store.literal_value(ty) {
        Some(LiteralValue::String(name)) => Some(name.clone()),
        Some(LiteralValue::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

/// Whether an assignment-target base expression is the `this` keyword (M14), unwrapping
/// redundant parentheses (`(this).prop`). Used to gate the `readonly` constructor
/// allowance, which applies only to `this.prop` — not to `obj.prop` on some other
/// instance value.
fn base_is_this(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::ThisExpression(_) => true,
        Expression::ParenthesizedExpression(paren) => base_is_this(&paren.expression),
        _ => false,
    }
}

/// The declared type for an initializer-only declaration, applying widening:
/// `let`/`var` widen a literal init to its base intrinsic; `const`/`using` keep
/// the literal. Non-literal inits (objects, functions) pass through unchanged.
pub(in crate::check::checker) fn declared_from_init(
    interner: &mut Interner,
    kind: VariableDeclarationKind,
    init_ty: TypeId,
) -> TypeId {
    if kind.is_const() {
        return init_ty;
    }
    interner.widen_literal_type(init_ty)
}

/// The [`ValueStorageId`] of a declarator's binding, resolved through the scope graph by
/// name from `scope`. `None` for non-identifier bindings (out of subset).
pub(in crate::check::checker) fn binding_decl_id(
    binder: &Binder,
    scope: ScopeId,
    pattern: &BindingPattern<'_>,
) -> Option<ValueStorageId> {
    let name = match pattern {
        BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
        _ => return None,
    };
    let symbol_id = binder.resolve_value(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}

#[cfg(test)]
mod enum_widen_tests {
    use super::*;
    use crate::check::checker::calls::widen;
    use crate::types::repr::{
        EnumId, EnumMemberFact, EnumMemberId, EnumNumber, EnumType, EnumValue,
    };

    #[test]
    fn mutable_bindings_and_expression_widening_use_the_owning_enum() {
        let mut interner = Interner::with_intrinsics();
        let enum_id = EnumId(90_001);
        let member_id = EnumMemberId(0);
        let whole = interner.intern_enum(EnumType {
            id: enum_id,
            name: "Mode".to_owned(),
            members: vec![EnumMemberFact {
                id: member_id,
                name: "Read".to_owned(),
                value: EnumValue::Number(EnumNumber::new(0.0).expect("finite enum constant")),
            }],
            numeric_open: false,
        });
        let member = interner
            .store()
            .enum_member_type_id(enum_id, member_id)
            .expect("interning the family publishes its exact member");
        let regular = member;
        let member = interner
            .intern_widening_literal(regular)
            .expect("ordinary enum initializer");

        assert_eq!(
            declared_from_init(&mut interner, VariableDeclarationKind::Let, member),
            whole
        );
        assert_eq!(
            declared_from_init(&mut interner, VariableDeclarationKind::Const, member),
            member
        );
        assert_eq!(widen(&mut interner, member), whole);
        assert_eq!(
            declared_from_init(&mut interner, VariableDeclarationKind::Let, regular),
            regular
        );
        assert_eq!(widen(&mut interner, regular), regular);
    }
}

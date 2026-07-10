//! assignment module (extracted from checker/mod.rs).

use super::calls::intrinsic_id;
use super::context::*;
use super::expr::contextual_literal_target;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::binder::Binder;
use crate::diagnostics::{render_type, Diagnostic};
use crate::span::Span;
use crate::types::repr::TypeTag;
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentOperator, AssignmentTarget, BindingPattern, Expression,
    ObjectPropertyKind, StaticMemberExpression, VariableDeclarationKind,
};
use oxc_span::GetSpan;

impl<'a, 'ast> Pass<'a, 'ast> {
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
        let target = match &assign.left {
            AssignmentTarget::AssignmentTargetIdentifier(target) => target,
            // M14: a **static member** target (`this.prop = …`, `obj.prop = …`) is now
            // type-checked — the RHS must be assignable to the property's type
            // (`TK2322`), and a `readonly` member may not be assigned outside the
            // declaring class's constructor (`TK2540`). Handled in its own routine.
            AssignmentTarget::StaticMemberExpression(member) => {
                return self.check_member_assignment(scope, member, assign);
            }
            // Computed/destructuring targets stay deferred. Still walk the RHS so
            // nested checks and unresolved-name diagnostics fire, but collect no
            // obligation.
            _ => {
                let rhs = self.infer_expr(scope, &assign.right);
                return rhs.map(|(ty, _)| (ty, assign_span));
            }
        };

        // Infer the RHS before the target so it sees pre-assignment narrowing and
        // compound assignments still walk their RHS.
        let rhs = self.infer_expr(scope, &assign.right);
        // The assignment expression's value is the RHS (TS semantics), regardless of
        // whether a type obligation is collected below.
        let value = rhs.map(|(ty, _)| (ty, assign_span));

        let symbol_id = match self.binder.graph.resolve(scope, target.name.as_str()) {
            Some(symbol_id) => symbol_id,
            None => {
                self.diagnostics.push(Diagnostic::cannot_find_name(
                    Span::from_oxc(target.span),
                    target.name.as_str(),
                ));
                return value;
            }
        };

        // Compound assignment (`+=`, `||=`, …): assignability is unchecked baseline-wide
        // (out of the M7 subset). The flow graph already reset this symbol's narrowing
        // (an assignment node), so it is sound to stop here without an obligation.
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
            .and_then(|decl_id| self.decl_types.get(decl_id));

        if let (Some(tgt), Some(raw)) = (target_ty, rhs) {
            let (src, src_span) =
                self.infer_contextual_source_after_walked(scope, &assign.right, tgt, raw);
            check_excess_properties(
                self.interner.store(),
                &assign.right,
                tgt,
                &mut self.diagnostics,
            );
            self.obligations.push(AssignObligation {
                src,
                tgt,
                src_span,
                kind: ObligationKind::Assignment,
            });
        }

        value
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

        // Compound assignment (`+=`, `||=`, …): assignability is unchecked baseline-wide and a
        // `readonly` member-compound-assignment is out of the M14 subset. Stop after walking.
        if assign.operator != AssignmentOperator::Assign {
            return value;
        }

        // Resolve the base type. Absent → nothing to check.
        let Some((base_ty, _)) = base else {
            return value;
        };

        // Skip when the base is the error/`any` type (an unresolved base, or `any`): there is
        // no concrete property to check against, and the error type suppresses cascades.
        if base_ty == wk.error || base_ty == wk.any {
            return value;
        }

        // Member writes use the same apparent/merged base type as reads, so
        // constrained parameters and intersections expose their real property type.
        let base_ty = self.apparent_type(base_ty);
        let base_ty = self
            .intersection_apparent_object(base_ty)
            .unwrap_or(base_ty);

        // Look up the property on the base **object** type. Snapshot the property's
        // type + `readonly` + `is_accessor` + origin before any `&mut` borrow for a
        // diagnostic.
        let prop_name = member.property.name.as_str();
        let object_found = self
            .interner
            .store()
            .object_type(base_ty)
            .and_then(|obj| obj.property(prop_name))
            .map(|prop| {
                (
                    prop.ty,
                    prop.readonly,
                    prop.is_accessor,
                    prop.declaring_class,
                )
            });

        // Union bases have no single object; fall back to the read-side union member
        // model so readonly and type checks are not skipped.
        let found = match object_found {
            Some(found) => Some(found),
            None => self.union_member_assignment_target(base_ty, prop_name),
        };

        // Property not on the type (or missing on some union member) → deferred (no
        // `TK2339` on an assignment target).
        let Some((prop_ty, readonly, is_accessor, declaring_class)) = found else {
            return value;
        };

        // A property whose type is the error type carries no real obligation.
        if prop_ty == wk.error {
            return value;
        }

        // `readonly` data fields may be assigned only as `this.prop` in the declaring
        // constructor. Get-only accessors are read-only everywhere, so the carve-out
        // is gated on `!is_accessor`.
        if readonly {
            let in_declaring_ctor = !is_accessor
                && self.current_in_ctor
                && matches!(
                    (self.current_class, declaring_class),
                    (Some(ctx), Some(owner)) if ctx == owner
                );
            if !(base_is_this(&member.object) && in_declaring_ctor) {
                self.diagnostics.push(Diagnostic::readonly_assignment(
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
        }

        // Type-assignability obligation: the RHS must be assignable to the property's type.
        // Skipped when the RHS type is the error type (incomplete inference — see the doc
        // comment); the primary span is the RHS.
        if let Some(raw) = rhs {
            let (src, src_span) =
                self.infer_contextual_source_after_walked(scope, &assign.right, prop_ty, raw);
            if src != wk.error {
                check_excess_properties(
                    self.interner.store(),
                    &assign.right,
                    prop_ty,
                    &mut self.diagnostics,
                );
                self.obligations.push(AssignObligation {
                    src,
                    tgt: prop_ty,
                    src_span,
                    kind: ObligationKind::Assignment,
                });
            }
        }

        value
    }

    /// Resolve a union assignment target. The property must exist on every member;
    /// the write type is the union of member property types, and readonly is true if
    /// any constituent is readonly. A union base is never `this`, so constructor
    /// carve-outs do not apply.
    fn union_member_assignment_target(
        &mut self,
        base_ty: TypeId,
        prop_name: &str,
    ) -> Option<(TypeId, bool, bool, Option<crate::types::repr::ClassId>)> {
        // Snapshot the member ids: the per-member lookups are immutable, but interning the
        // result union below needs `&mut`, so the borrow must not be held across it.
        let members: Vec<TypeId> = self.interner.store().union_members(base_ty)?.to_vec();

        let mut member_prop_types: Vec<TypeId> = Vec::with_capacity(members.len());
        let mut any_readonly = false;
        for member in members {
            // M24 (audit): a union member that is a constrained type parameter resolves
            // through its apparent type, mirroring the read side.
            let member = self.apparent_type(member);
            let prop = self
                .interner
                .store()
                .object_type(member)
                .and_then(|o| o.property(prop_name))?;
            member_prop_types.push(prop.ty);
            any_readonly |= prop.readonly;
        }

        let prop_ty = self.interner.union(member_prop_types);
        Some((prop_ty, any_readonly, false, None))
    }
}

/// Recursive excess-property check. M19 index signatures suppress unknown keys
/// only on the target that owns the signature; plain object targets still run the
/// M2 freshness check, including at nested levels.
pub(in crate::check::checker) fn check_excess_properties(
    store: &Store,
    expr: &Expression<'_>,
    target_ty: TypeId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let target_ty = contextual_literal_target(store, target_ty);
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            check_excess_properties(store, &paren.expression, target_ty, diagnostics);
        }
        Expression::ObjectExpression(literal) => {
            // M31: a fresh literal against an intersection target is excess-checked against
            // the MERGED key set (a single check, never per-member — see the function).
            if store.tag(target_ty) == TypeTag::Intersection {
                check_intersection_object_excess(store, literal, target_ty, diagnostics);
            } else {
                check_object_excess_properties(store, literal, target_ty, diagnostics);
            }
        }
        Expression::ArrayExpression(array) => {
            check_array_excess_properties(store, array, target_ty, diagnostics);
        }
        _ => {}
    }
}

fn check_object_excess_properties(
    store: &Store,
    literal: &oxc_ast::ast::ObjectExpression<'_>,
    target_ty: TypeId,
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
                check_excess_properties(store, &prop.value, recurse_ty, diagnostics);
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
                        check_excess_properties(store, &prop.value, value_ty, diagnostics);
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
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(members) = store.intersection_members(ty) else {
        return;
    };
    check_object_excess_against_members(store, literal, members, diagnostics);
}

/// Excess-check a fresh object literal against a merged member set. Matched keys
/// recurse through all contributing members so later constituents are not missed.
fn check_object_excess_against_members(
    store: &Store,
    literal: &oxc_ast::ast::ObjectExpression<'_>,
    members: &[TypeId],
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
            check_excess_against_members(store, &prop.value, &contributors, diagnostics);
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
                check_excess_against_members(store, &prop.value, &[value_ty], diagnostics);
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
    diagnostics: &mut Vec<Diagnostic>,
) {
    match members {
        [] => {}
        [single] => check_excess_properties(store, expr, *single, diagnostics),
        _ => match expr {
            Expression::ParenthesizedExpression(paren) => {
                check_excess_against_members(store, &paren.expression, members, diagnostics);
            }
            Expression::ObjectExpression(literal) => {
                check_object_excess_against_members(store, literal, members, diagnostics);
            }
            _ => {}
        },
    }
}

fn check_array_excess_properties(
    store: &Store,
    array: &oxc_ast::ast::ArrayExpression<'_>,
    target_ty: TypeId,
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
            check_excess_properties(store, expr, target, diagnostics);
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
        check_excess_properties(store, expr, target, diagnostics);
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
    match interner.store().literal_value(init_ty) {
        Some(lit) => intrinsic_id(interner.well_known(), lit.base_kind()),
        None => init_ty,
    }
}

/// The `DeclId` of a declarator's binding, resolved through the scope graph by
/// name from `scope`. `None` for non-identifier bindings (out of subset).
pub(in crate::check::checker) fn binding_decl_id(
    binder: &Binder,
    scope: ScopeId,
    pattern: &BindingPattern<'_>,
) -> Option<DeclId> {
    let name = match pattern {
        BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
        _ => return None,
    };
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}

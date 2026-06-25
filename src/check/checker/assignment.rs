//! assignment module (extracted from checker/mod.rs).

use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::binder::Binder;
use crate::diagnostics::{render_type, Diagnostic};
use crate::span::Span;
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use oxc_ast::ast::{
    AssignmentExpression, AssignmentOperator,
    AssignmentTarget, BindingPattern, Expression, ObjectPropertyKind,
    StaticMemberExpression, VariableDeclarationKind,
};
use oxc_span::GetSpan;
use super::context::*;
use super::calls::intrinsic_id;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Check a reassignment `NAME = <expr>` (a simple `=` to an identifier target) in
    /// `scope`. The RHS must be assignable to the target's declared type → `TK2322`
    /// with the RHS as the primary span. An unresolved target is `TK2304`.
    ///
    /// M7 soundness — **any assignment to a narrowed symbol resets its narrowing.**
    /// Assigning to a narrowed symbol drops its narrowing entry (resetting it to the
    /// declared type) so a stale narrowing is never read after the value changed.
    /// (Conservatively resetting to the declared type, rather than re-narrowing to the
    /// assigned value's type, is sound: the declared type is the widest the symbol can
    /// hold, so it can only over-report.) The reset runs for **every** assignment to a
    /// resolvable identifier target — simple (`=`) *and* compound (`+=`, `||=`, …) —
    /// **before** the compound-operator early-return, so a compound assignment to a
    /// narrowed variable cannot leave a stale narrowing in place. (Compound-assignment
    /// *assignability* is unchecked baseline-wide, so the obligation/`TK2322` path stays
    /// gated on a simple `=`; only the narrowing reset is hoisted.) A non-identifier or
    /// unresolvable target has no symbol to reset, so it narrows nothing and never
    /// panics.
    pub(in crate::check::checker) fn check_assignment(&mut self, scope: ScopeId, assign: &AssignmentExpression<'_>) {
        let target = match &assign.left {
            AssignmentTarget::AssignmentTargetIdentifier(target) => target,
            // M14: a **static member** target (`this.prop = …`, `obj.prop = …`) is now
            // type-checked — the RHS must be assignable to the property's type
            // (`TK2322`), and a `readonly` member may not be assigned outside the
            // declaring class's constructor (`TK2540`). Handled in its own routine.
            AssignmentTarget::StaticMemberExpression(member) => {
                self.check_member_assignment(scope, member, assign);
                return;
            }
            // Other targets (computed/element access `obj[k] = …`, destructuring) stay
            // **deferred**: there is no symbol to reset and member-assignment checking of
            // these is out of the M14 subset. Still infer the RHS so it is walked — a
            // nested call/`new`/function inside it is checked, an unresolved name in it
            // emits `TK2304` — but collect **no** obligation (so no false negative or
            // spurious error).
            _ => {
                self.infer_expr(scope, &assign.right);
                return;
            }
        };

        // Infer the RHS first so any reference inside it resolves (and emits TK2304
        // before we look at the target), and any nested function body is checked. The
        // RHS is evaluated *before* the assignment, so it still sees the target's
        // pre-assignment narrowing (e.g. `x = x` reads the narrowed `x`). Done for both
        // simple and compound forms so the RHS of a compound assignment is still walked.
        let rhs = self.infer_expr(scope, &assign.right);

        let symbol_id = match self.binder.graph.resolve(scope, target.name.as_str()) {
            Some(symbol_id) => symbol_id,
            None => {
                self.diagnostics.push(Diagnostic::cannot_find_name(
                    Span::from_oxc(target.span),
                    target.name.as_str(),
                ));
                return;
            }
        };

        // Reset any narrowing on the reassigned symbol FIRST — for every operator
        // (simple or compound). The value changed, so a prior narrowing is now stale
        // and must not be read by a later reference. Hoisted above the compound-operator
        // early-return below so `x += …` / `x ||= …` cannot leave a stale narrowing.
        self.narrowed.remove(&symbol_id);

        // Compound assignment (`+=`, `||=`, …): assignability is unchecked baseline-wide
        // (out of the M7 subset). The narrowing reset above already ran, so it is sound
        // to stop here without collecting an obligation.
        if assign.operator != AssignmentOperator::Assign {
            return;
        }

        // The target type is always the symbol's *declared* type (you may assign
        // anything assignable to the declaration, regardless of the current narrowing).
        let target_ty = self
            .binder
            .symbols
            .get(symbol_id)
            .and_then(|s| s.value)
            .and_then(|decl_id| self.decl_types.get(decl_id));

        if let (Some(tgt), Some((src, src_span))) = (target_ty, rhs) {
            self.obligations.push(AssignObligation {
                src,
                tgt,
                src_span,
                kind: ObligationKind::Assignment,
            });
        }
    }

    /// Check a **member-assignment target** `obj.prop = expr` / `this.prop = expr` (M14,
    /// filling the gap deferred since M11). The right-hand side must be assignable to the
    /// resolved property's type (`TK2322`, primary span = the RHS), and a `readonly`
    /// property may be assigned **only** inside its declaring class's constructor via
    /// `this.prop` (otherwise `TK2540`).
    ///
    /// **Base / property resolution.** The base (`member.object`) and the RHS are inferred
    /// first (so references inside them resolve, nested calls/functions are checked, and an
    /// unresolved name emits `TK2304`). The property is then looked up by name on the base
    /// **object** type — the same lookup `infer_member_access` uses, but kept local so an
    /// assignment target does not also emit read-side `TK2339`/access-control diagnostics
    /// (those stay on the read path; reporting a missing/illegal *assignment* target is out
    /// of the M14 subset). `this.prop` falls out for free: a `ThisExpression` base infers to
    /// the current instance type (or the static side in a static body), so its members
    /// resolve exactly as any object base.
    ///
    /// **The error-typed skips (no regression / no false positive).** The assignability
    /// obligation is collected **only** when the base, the property, *and* the RHS are all
    /// well-typed — it is skipped when the base type is the error/`any` type, the base is not
    /// an object type (a union / intrinsic — element-access and union targets are deferred),
    /// the property is not found, the property's type is the error type, **or the RHS type is
    /// the error type**. The RHS skip mirrors how the M3/M11 return path tolerates
    /// incomplete expression inference: an arithmetic RHS like `this.count + by` infers to the
    /// error type (arithmetic is out of the value subset), so collecting an obligation against
    /// it would be meaningless — skipping keeps the existing M11–M13 constructor/method
    /// assignments clean. (Such an RHS is already assignable to anything, so the skip changes
    /// no verdict; it is belt-and-suspenders against any future over-report.)
    ///
    /// **Compound assignment** (`+=`, …) on a member target stays **deferred** — the base/RHS
    /// are still walked, but no obligation and no `readonly` check are collected (matching the
    /// identifier-target baseline, where compound assignability is unchecked).
    fn check_member_assignment(
        &mut self,
        scope: ScopeId,
        member: &StaticMemberExpression<'_>,
        assign: &AssignmentExpression<'_>,
    ) {
        let wk = self.interner.well_known();

        // Walk the base (so its references resolve / nested constructs are checked) and the
        // RHS (so an unresolved name in it emits `TK2304`, and nested calls/functions are
        // checked), regardless of operator — both must be walked even when no obligation is
        // collected below.
        let base = self.infer_expr(scope, &member.object);
        let rhs = self.infer_expr(scope, &assign.right);

        // Compound assignment (`+=`, `||=`, …): assignability is unchecked baseline-wide and a
        // `readonly` member-compound-assignment is out of the M14 subset. Stop after walking.
        if assign.operator != AssignmentOperator::Assign {
            return;
        }

        // Resolve the base type. Absent → nothing to check.
        let Some((base_ty, _)) = base else {
            return;
        };

        // Skip when the base is the error/`any` type (an unresolved base, or `any`): there is
        // no concrete property to check against, and the error type suppresses cascades.
        if base_ty == wk.error || base_ty == wk.any {
            return;
        }

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

        // F5/backlog-03 (part b): a **union** base has no `object_type`, so the object
        // lookup above misses it and the assignment was silently skipped (no readonly
        // check, no type check). Resolve the property across the union members instead —
        // mirroring the read-side `union_member_access` model — when the object path found
        // nothing.
        let found = match object_found {
            Some(found) => Some(found),
            None => self.union_member_assignment_target(base_ty, prop_name),
        };

        // Property not on the type (or missing on some union member) → deferred (no
        // `TK2339` on an assignment target).
        let Some((prop_ty, readonly, is_accessor, declaring_class)) = found else {
            return;
        };

        // A property whose type is the error type carries no real obligation.
        if prop_ty == wk.error {
            return;
        }

        // M14/M15 — `readonly` gate: a `readonly` member may be assigned **only** as
        // `this.prop` inside the **declaring class's constructor**. Anywhere else (another
        // method, a static body, external code, or via a non-`this` base) it is `TK2540`.
        //
        // M15: that constructor carve-out applies to a `readonly` **data field** only — a
        // get-only **accessor** is read-only *everywhere*, including its own constructor
        // (tsc `TS2540`). So the carve-out is additionally gated on `!is_accessor`: a
        // get-only accessor is `TK2540` regardless of constructor context.
        if readonly {
            let in_declaring_ctor = !is_accessor
                && self.current_in_ctor
                && matches!(
                    (self.current_class, declaring_class),
                    (Some(ctx), Some(owner)) if ctx == owner
                );
            if !(base_is_this(&member.object) && in_declaring_ctor) {
                self.diagnostics.push(Diagnostic::readonly_assignment(
                    Span::from_oxc(member.span),
                    member.property.name.as_str(),
                ));
                // `TK2540` is the assignment error for this target; do not also collect a
                // type-assignability obligation (the fixtures' readonly RHS types already
                // match, and tsc surfaces the read-only violation as the assignment error).
                return;
            }
            // Allowed (declaring constructor): fall through to the normal type check, so a
            // *type-wrong* `this.readonly = …` in the constructor is still `TK2322`.
        }

        // Type-assignability obligation: the RHS must be assignable to the property's type.
        // Skipped when the RHS type is the error type (incomplete inference — see the doc
        // comment); the primary span is the RHS.
        if let Some((src, src_span)) = rhs {
            if src != wk.error {
                self.obligations.push(AssignObligation {
                    src,
                    tgt: prop_ty,
                    src_span,
                    kind: ObligationKind::Assignment,
                });
            }
        }
    }

    /// Resolve a property assignment target across a **union** base (F5/backlog-03 part b),
    /// mirroring the read-side [`union_member_access`](super::Pass::union_member_access)
    /// model. The property must be present on **every** member (else the target is deferred
    /// — `None`, no `TK2339` on an assignment target, matching the object path). When it is:
    ///
    /// - the **readonly** flag for the target is `true` if the property is `readonly` on
    ///   **any** member (matching tsc — a union property is read-only if any constituent
    ///   makes it so), so assigning to it is `TK2540`;
    /// - the target **type** is the union of the members' property types (interned), so a
    ///   wrong-typed RHS is `TK2322`.
    ///
    /// `is_accessor`/`declaring_class` are reported as `false`/`None`: the `readonly`
    /// gate's constructor carve-out only applies to a `this.prop` base, and a union base is
    /// never `this`, so these never affect the verdict — the gate emits `TK2540` whenever
    /// `readonly` holds. Returns the same `(ty, readonly, is_accessor, declaring_class)`
    /// snapshot the object path produces.
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

/// Recursive excess-property (freshness) check (mvp-plan §6, README
/// `excess_property.ts`).
///
/// M19 — **index-signature suppression**: an index signature accepts arbitrary keys,
/// so when the target object type has a **string** index signature a property the
/// target does not name is NOT excess — the `TK2353` is suppressed (otherwise
/// `{ a: 1, b: 2 }` against `{ [k: string]: number }` would wrongly report `b` as
/// excess). The suppression is scoped to the **target that actually has the index
/// signature**: a plain object target (no index signature) still gets the M2 excess
/// check exactly as before, including at nested levels.
pub(in crate::check::checker) fn check_excess_properties(
    store: &Store,
    expr: &Expression<'_>,
    target_ty: TypeId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Expression::ObjectExpression(literal) = expr else {
        return;
    };
    let Some(target_obj) = store.object_type(target_ty) else {
        return;
    };
    // M19: an index signature accepts arbitrary keys, so a property the target does
    // not name is not excess. A **string** index accepts any key; a **number** index
    // accepts only **numeric-named** keys (`{ 0: "a" }` against `{ [i: number]: T }`).
    // The index **value** types are snapshotted so the excess check still descends
    // into a fresh object literal used as an index value (its own freshness check).
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
                // M21: an *optional* member's type is `T | undefined` (a union), but
                // freshness must still recurse THROUGH that container (the M19
                // "freshness recurses through a new container" rule), so descend into
                // the union's object part rather than bail on the union.
                let recurse_ty = excess_recursion_target(store, target_prop.ty);
                check_excess_properties(store, &prop.value, recurse_ty, diagnostics);
            }
            None => {
                // M19: the key itself is suppressed when an index signature accepts it
                // — a string index for any key, a number index for a numeric-named key.
                // A plain object target (no index signature) still reports excess (M2).
                // The index value type that governs this key: a numeric key prefers
                // the number index, else the string index covers it.
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

/// The object type to descend into for a freshness (excess) check on a matched
/// member. An *optional* member's type is `T | undefined` (a union); freshness must
/// still recurse through that container (the M19 "freshness recurses through a new
/// container" rule), so return the union's lone object member. A non-union (or a
/// union without exactly one object member — e.g. a primitive, or the pre-existing
/// multi-object-union gap) is returned unchanged, and the recursive call simply
/// finds no object and stops.
fn excess_recursion_target(store: &Store, ty: TypeId) -> TypeId {
    let Some(members) = store.union_members(ty) else {
        return ty;
    };
    let mut objects = members
        .iter()
        .copied()
        .filter(|&m| store.object_type(m).is_some());
    match (objects.next(), objects.next()) {
        (Some(obj), None) => obj,
        _ => ty,
    }
}

/// Whether a property name is **numeric-keyed** (M19) — governed by a number index
/// signature. A name that parses as a finite number (`"0"`, `"1"`, …) is numeric;
/// an ordinary identifier (`"a"`) is not. Mirrors the relation engine's rule for the
/// excess-property suppression so a numeric-named member is accepted by a number
/// index signature (matching tsc).
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
pub(in crate::check::checker) fn binding_decl_id(binder: &Binder, scope: ScopeId, pattern: &BindingPattern<'_>) -> Option<DeclId> {
    let name = match pattern {
        BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
        _ => return None,
    };
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}


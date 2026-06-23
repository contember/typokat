//! The statement-level checker (architecture §5, mvp-plan §5 — M0/M1/M2 rows).
//!
//! M2 scope, on top of M0/M1:
//!
//!  - **Object type annotations.** `{ a: number; b: string }` lowers to an
//!    interned object `TypeId` (recursing into nested object types).
//!  - **Object literal inference.** `{ a: 1, b: "x" }` infers an object type with
//!    its member types **widened** (`{ a: number; b: string }`) regardless of
//!    `const`/`let` (standard TS), recursing into nested literals.
//!  - **Member access.** `obj.prop` resolves the property type off the object
//!    type; a missing property is `TK2339` (primary span = the property name). An
//!    `any`/error base yields the error type (no cascade).
//!  - **Structural assignability.** The relation engine compares object types
//!    property-wise (width + depth). The checker maps the failure's outermost
//!    reason to a code: a **missing required property** → `TK2741` (primary span =
//!    the source literal/expression); a **property-type mismatch** → `TK2322`.
//!  - **Excess-property check (freshness).** A **fresh** object literal assigned
//!    directly to an object-typed target reports each property not in the target
//!    as `TK2353` (primary span = the offending property). A literal reaching the
//!    target *through a variable* is not fresh → no excess check. Freshness
//!    recurses into nested fresh literals. This is separate from the structural
//!    verdict (a literal `{a,x}` is width-assignable to `{a}`, but freshness
//!    makes the excess `x` an error).
//!
//! M1 scope, on top of M0's annotation-vs-literal check:
//!
//!  - **Binding & resolution.** The program is bound (`binder::bind_module`) into
//!    a scope graph + multi-slot symbols; identifier references resolve through
//!    the scope chain rather than being matched syntactically.
//!  - **Inference from initializer.** `const NAME = <init>` takes the init's
//!    type (a literal stays a literal); `let`/`var NAME = <init>` *widens* a
//!    literal init to its base type (`1` → `number`).
//!  - **Variable references.** An identifier expression resolves to its symbol's
//!    declared/inferred type, usable as an assignment source.
//!  - **Unresolved names.** An unknown identifier is `TK2304` and gets the
//!    **error type**, which relates to everything (`is_any_like`) and so
//!    suppresses any cascade `TK2322` on the same expression.
//!  - **Reassignment.** `NAME = <expr>` checks the RHS is assignable to the
//!    variable's declared type → `TK2322` (primary span = the RHS).
//!
//! The two-phase shape from M0 is kept: phase 1 interns types, resolves names,
//! and emits `TK2304`; phase 2 runs the relation engine over an immutable store
//! and emits `TK2322`. Phases are split this way because interning needs
//! `&mut Interner` while the relater borrows the store immutably — they cannot
//! be held at once. No `unwrap`/`panic` on any path.

use crate::binder::symbol::DeclId;
use crate::binder::{bind_module, Binder};
use crate::diagnostics::{render_type, Diagnostic};
use crate::relate::{Reason, Relater, Relation};
use crate::span::Span;
use crate::types::repr::{IntrinsicKind, LiteralValue, ObjectType, PropertyType};
use crate::types::store::{Store, TypeId};
use crate::types::{Interner, WellKnown};
use oxc_ast::ast::{
    AssignmentExpression, AssignmentOperator, AssignmentTarget, BindingPattern, Expression,
    ObjectExpression, ObjectPropertyKind, Program, Statement, StaticMemberExpression, TSSignature,
    TSType, VariableDeclarationKind, VariableDeclarator,
};
use oxc_span::GetSpan;

/// One assignability obligation: `src` must be assignable to `tgt`, and the
/// resulting diagnostic's primary span is `src_span`. Covers both
/// annotation-vs-initializer (declaration) and RHS-vs-declared-type
/// (reassignment).
struct AssignObligation {
    src: TypeId,
    tgt: TypeId,
    src_span: Span,
}

/// Per-declaration computed types, indexed by `DeclId`. `None` means a
/// declaration whose type could not be computed (out of the M1 subset); a
/// reference to it resolves to the error type defensively.
struct DeclTypes {
    types: Vec<Option<TypeId>>,
}

impl DeclTypes {
    fn new(count: u32) -> Self {
        DeclTypes {
            types: vec![None; count as usize],
        }
    }

    fn set(&mut self, id: DeclId, ty: TypeId) {
        if let Some(slot) = self.types.get_mut(id.index()) {
            *slot = Some(ty);
        }
    }

    fn get(&self, id: DeclId) -> Option<TypeId> {
        self.types.get(id.index()).copied().flatten()
    }
}

/// Check a parsed program and return the diagnostics it produces.
pub fn check_program(interner: &mut Interner, program: &Program<'_>) -> Vec<Diagnostic> {
    let binder = bind_module(program);
    let mut decl_types = DeclTypes::new(binder.decl_count);

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // --- Phase 1: bind-resolved walk. Interns types, records each declaration's
    //     type, resolves references, and emits TK2304 for unresolved names. ---
    let mut obligations: Vec<AssignObligation> = Vec::new();
    for stmt in &program.body {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    check_declarator(
                        interner,
                        &binder,
                        &mut decl_types,
                        decl.kind,
                        declarator,
                        &mut obligations,
                        &mut diagnostics,
                    );
                }
            }
            Statement::ExpressionStatement(expr_stmt) => {
                if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                    check_assignment(
                        interner,
                        &binder,
                        &decl_types,
                        assign,
                        &mut obligations,
                        &mut diagnostics,
                    );
                }
                // Other expression statements are out of M1 scope.
            }
            // Any other statement is out of M1 scope and ignored (no diagnostic).
            _ => {}
        }
    }

    // --- Phase 2: relate + render obligations (immutable store borrow). ---
    let well_known = interner.well_known();
    let store = interner.store();
    let mut relater = Relater::new(store, well_known);

    for ob in &obligations {
        if let Relation::No(chain) = relater.is_assignable(ob.src, ob.tgt) {
            // Map the failure's outermost reason to a code (mvp-plan §6 "code
            // mapping"). The error type never reaches here: it is `any`-like, so
            // its obligations resolve to `Yes` and the cascade is suppressed.
            match chain.head() {
                // A required target property is absent in the source → TK2741,
                // primary span = the source literal/expression.
                Reason::MissingProperty { name, tgt, .. } => {
                    let tgt = render_type(store, *tgt, /* widen */ false);
                    diagnostics.push(Diagnostic::property_missing(ob.src_span, name, &tgt));
                }
                // Everything else (primitive mismatch, or a present-but-wrong
                // property — possibly nested) → TK2322. The nested reason is built
                // for M6; M2 renders the flat top-level message. Object targets
                // are asserted code-only in the corpus, so the rendered object
                // string need not match a fixed layout.
                Reason::Leaf { .. } | Reason::Property { .. } => {
                    // Source widened (literal → base), target as-is (mvp-plan
                    // M0/M1 message spec).
                    let src = render_type(store, ob.src, /* widen */ true);
                    let tgt = render_type(store, ob.tgt, /* widen */ false);
                    let message = format!("Type '{src}' is not assignable to type '{tgt}'");
                    diagnostics.push(Diagnostic::not_assignable(ob.src_span, message));
                }
            }
        }
    }

    diagnostics
}

/// Check one variable declarator and record its declared/inferred type.
///
/// The declared type is the annotation if present, otherwise inferred from the
/// initializer (with `let`/`var` widening). When both an annotation and an
/// initializer are present, an assignability obligation is collected (the M0
/// check). The type is stored under the declarator's `DeclId` so later
/// references resolve to it.
fn check_declarator(
    interner: &mut Interner,
    binder: &Binder,
    decl_types: &mut DeclTypes,
    kind: VariableDeclarationKind,
    declarator: &VariableDeclarator<'_>,
    obligations: &mut Vec<AssignObligation>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(decl_id) = binding_decl_id(binder, &declarator.id) else {
        // Non-identifier binding (destructuring) — out of M1 scope.
        return;
    };

    // Infer the initializer's type first (it may resolve references / emit
    // TK2304), independent of whether an annotation is present.
    let initializer = declarator
        .init
        .as_ref()
        .and_then(|init| infer_expr(interner, binder, decl_types, init, diagnostics));

    let annotation = declarator
        .type_annotation
        .as_ref()
        .and_then(|ann| lower_annotation(interner, &ann.type_annotation));

    // The declared type the symbol resolves to: the annotation wins; otherwise
    // the (possibly widened) initializer type.
    let declared = match (annotation, &initializer) {
        (Some(ann), _) => Some(ann),
        (None, Some((init_ty, _))) => Some(declared_from_init(interner, kind, *init_ty)),
        (None, None) => None,
    };
    if let Some(ty) = declared {
        decl_types.set(decl_id, ty);
    }

    // When both sides are present, the initializer must be assignable to the
    // annotation (M0's rule; primary span = the initializer).
    if let (Some(ann), Some((init_ty, init_span))) = (annotation, initializer) {
        obligations.push(AssignObligation {
            src: init_ty,
            tgt: ann,
            src_span: init_span,
        });

        // Excess-property check (freshness): a fresh object literal assigned
        // directly to an object-typed target reports properties absent in the
        // target as TK2353. This is separate from — and runs alongside — the
        // structural assignability obligation above (a fresh `{a,x}` is
        // width-assignable to `{a}`, yet `x` is still an excess error). Reads the
        // now-complete store immutably; all relevant types are already interned.
        if let Some(init_expr) = declarator.init.as_ref() {
            check_excess_properties(interner.store(), init_expr, ann, diagnostics);
        }
    }
}

/// Recursive excess-property (freshness) check (mvp-plan §6, README
/// `excess_property.ts`).
///
/// Fires **only** when `expr` is a *fresh* object literal (a syntactic
/// `ObjectExpression`) assigned to an object-typed target — a literal that
/// reached the target through a variable is an `Identifier`, not an
/// `ObjectExpression`, so it is not fresh and is skipped here. Each literal
/// property whose name is absent in `target_ty` is reported as `TK2353` (primary
/// span = the offending property). Freshness recurses: a property whose value is
/// itself a fresh object literal is checked against the corresponding nested
/// target object type.
fn check_excess_properties(
    store: &Store,
    expr: &Expression<'_>,
    target_ty: TypeId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Not a fresh literal, or the target is not an object type → no check.
    let Expression::ObjectExpression(literal) = expr else {
        return;
    };
    let Some(target_obj) = store.object_type(target_ty) else {
        return;
    };
    let target_rendered = render_type(store, target_ty, /* widen */ false);

    for member in &literal.properties {
        // Only plain `key: value` members with a static name are in the subset;
        // spreads / computed / accessor members are skipped (not in M2 fixtures).
        let ObjectPropertyKind::ObjectProperty(prop) = member else {
            continue;
        };
        let Some(name) = prop.key.static_name() else {
            continue;
        };

        match target_obj.property(&name) {
            // Present in the target: recurse into a nested fresh literal so a
            // deeper excess (`{ a: { b, c } }` vs `{ a: { b } }`) is caught.
            Some(target_prop) => {
                check_excess_properties(store, &prop.value, target_prop.ty, diagnostics);
            }
            // Absent in the target → excess property.
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

/// Check a reassignment `NAME = <expr>` (a simple `=` to an identifier target).
///
/// The RHS must be assignable to the target variable's declared type → `TK2322`
/// with the RHS as the primary span. An unresolved target is `TK2304`; other
/// (compound `+=`, member, destructuring) targets are out of M1 scope.
fn check_assignment(
    interner: &mut Interner,
    binder: &Binder,
    decl_types: &DeclTypes,
    assign: &AssignmentExpression<'_>,
    obligations: &mut Vec<AssignObligation>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // M1: only the plain `=` operator (no `+=`, `&&=`, …).
    if assign.operator != AssignmentOperator::Assign {
        return;
    }
    // M1: only an identifier target (no member / destructuring targets).
    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
        return;
    };

    // Infer the RHS first so any reference inside it resolves (and emits TK2304
    // before we look at the target).
    let rhs = infer_expr(interner, binder, decl_types, &assign.right, diagnostics);

    // Resolve the target variable's declared type.
    let target_ty = match binder.graph.resolve(binder.module, target.name.as_str()) {
        Some(symbol_id) => binder
            .symbols
            .get(symbol_id)
            .and_then(|s| s.value)
            .and_then(|decl_id| decl_types.get(decl_id)),
        None => {
            diagnostics.push(Diagnostic::cannot_find_name(
                Span::from_oxc(target.span),
                target.name.as_str(),
            ));
            return;
        }
    };

    // With both a typed target and a typed RHS, check assignability.
    if let (Some(tgt), Some((src, src_span))) = (target_ty, rhs) {
        obligations.push(AssignObligation {
            src,
            tgt,
            src_span,
        });
    }
}

/// The declared type for an initializer-only declaration, applying widening:
/// `let`/`var` widen a literal init to its base intrinsic (`1` → `number`);
/// `const` (and `using`) keep the literal type. Non-literal inits are unchanged.
fn declared_from_init(
    interner: &mut Interner,
    kind: VariableDeclarationKind,
    init_ty: TypeId,
) -> TypeId {
    if kind.is_const() {
        return init_ty;
    }
    // `let` / `var`: widen a literal to its base intrinsic. Non-literals pass
    // through unchanged.
    match interner.store().literal_value(init_ty) {
        Some(lit) => intrinsic_id(interner.well_known(), lit.base_kind()),
        None => init_ty,
    }
}

/// The `DeclId` of a declarator's binding, resolved through the scope graph by
/// name. `None` for non-identifier bindings (out of M1 scope).
fn binding_decl_id(binder: &Binder, pattern: &BindingPattern<'_>) -> Option<DeclId> {
    let name = match pattern {
        BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
        _ => return None,
    };
    let symbol_id = binder.graph.resolve(binder.module, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}

/// Lower an annotation type to its `TypeId`. M2 supports the intrinsic keyword
/// types and object type literals (`{ a: number; b: string }`, recursing into
/// nested object types); union/reference/function annotations land in later
/// milestones and leave the declarator's annotation side absent.
///
/// Takes `&mut Interner` because object lowering interns new object types. The
/// keyword cases only read the well-known table.
fn lower_annotation(interner: &mut Interner, ts_type: &TSType<'_>) -> Option<TypeId> {
    let wk = interner.well_known();
    let id = match ts_type {
        TSType::TSAnyKeyword(_) => wk.any,
        TSType::TSUnknownKeyword(_) => wk.unknown,
        TSType::TSNeverKeyword(_) => wk.never,
        TSType::TSVoidKeyword(_) => wk.void,
        TSType::TSNullKeyword(_) => wk.null,
        TSType::TSUndefinedKeyword(_) => wk.undefined,
        TSType::TSBooleanKeyword(_) => wk.boolean,
        TSType::TSNumberKeyword(_) => wk.number,
        TSType::TSStringKeyword(_) => wk.string,
        TSType::TSTypeLiteral(lit) => return lower_object_annotation(interner, &lit.members),
        // TODO(M3+): union/reference/function annotations.
        _ => return None,
    };
    Some(id)
}

/// Lower an object type literal's members to an interned object `TypeId`.
///
/// Each property signature contributes a **required** member (M2 has no optional
/// types). A member whose type cannot be lowered (out of subset) makes the whole
/// object annotation un-lowerable (`None`) rather than silently dropping the
/// member — keeping the annotation honest. Index/call/method/construct
/// signatures are out of the subset and likewise abort the lowering.
fn lower_object_annotation(
    interner: &mut Interner,
    members: &[TSSignature<'_>],
) -> Option<TypeId> {
    let mut properties: Vec<PropertyType> = Vec::with_capacity(members.len());
    for member in members {
        let TSSignature::TSPropertySignature(sig) = member else {
            // Index/call/method/construct signature — out of the M2 subset.
            return None;
        };
        // Optional members are out of the subset; a `?` would change the relation
        // rule, so abort rather than treat it as required.
        if sig.optional {
            return None;
        }
        let name = sig.key.static_name()?;
        let annotation = sig.type_annotation.as_ref()?;
        let ty = lower_annotation(interner, &annotation.type_annotation)?;
        properties.push(PropertyType {
            name: name.into_owned(),
            ty,
            optional: false,
        });
    }
    Some(interner.intern_object(ObjectType { properties }))
}

/// Infer the type of an expression, returning `(TypeId, span)`. The span is the
/// expression's own span — the primary span for any diagnostic on it.
///
/// Literals are interned as their *literal* type (widening is applied later,
/// only where a `let`/`var` declaration calls for it). `null` and the
/// `undefined` keyword map to their intrinsics. An object literal infers an
/// object type with **widened** member types (M2). Member access resolves the
/// property type (or `TK2339`). An identifier reference resolves through the
/// scope graph: a resolved name yields its declared type; an unresolved name
/// emits `TK2304` and yields the **error type** so no cascade `TK2322` follows.
/// Returns `None` for expression shapes outside the subset (those positions are
/// simply not checked, matching M0 leniency).
fn infer_expr(
    interner: &mut Interner,
    binder: &Binder,
    decl_types: &DeclTypes,
    expr: &Expression<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(TypeId, Span)> {
    let well_known = interner.well_known();
    match expr {
        Expression::NumericLiteral(lit) => {
            let id = interner.intern_literal(LiteralValue::Number(lit.value));
            Some((id, Span::from_oxc(lit.span)))
        }
        Expression::StringLiteral(lit) => {
            let id = interner.intern_literal(LiteralValue::String(lit.value.to_string()));
            Some((id, Span::from_oxc(lit.span)))
        }
        Expression::BooleanLiteral(lit) => {
            let id = interner.intern_literal(LiteralValue::Boolean(lit.value));
            Some((id, Span::from_oxc(lit.span)))
        }
        Expression::NullLiteral(lit) => Some((well_known.null, Span::from_oxc(lit.span))),
        Expression::ObjectExpression(obj) => {
            let id = infer_object_literal(interner, binder, decl_types, obj, diagnostics);
            Some((id, Span::from_oxc(obj.span)))
        }
        Expression::StaticMemberExpression(member) => {
            infer_member_access(interner, binder, decl_types, member, diagnostics)
        }
        Expression::Identifier(ident) => {
            let span = Span::from_oxc(ident.span);
            // The `undefined` keyword parses as an identifier reference; treat it
            // as the `undefined` type (it has no declaration to resolve).
            if ident.name.as_str() == "undefined" {
                return Some((well_known.undefined, span));
            }
            match binder.graph.resolve(binder.module, ident.name.as_str()) {
                Some(symbol_id) => {
                    let ty = binder
                        .symbols
                        .get(symbol_id)
                        .and_then(|s| s.value)
                        .and_then(|decl_id| decl_types.get(decl_id))
                        // A resolved symbol with no computed type yet (out of
                        // subset) falls back to the error type — no cascade.
                        .unwrap_or(well_known.error);
                    Some((ty, span))
                }
                None => {
                    diagnostics.push(Diagnostic::cannot_find_name(span, ident.name.as_str()));
                    // The error type suppresses cascade diagnostics.
                    Some((well_known.error, span))
                }
            }
        }
        // TODO(M3+): array literals, calls, etc.
        _ => None,
    }
}

/// Infer the type of an object literal `{ a: 1, b: "x" }` → an interned object
/// type whose member types are **widened** (`{ a: number; b: string }`).
///
/// Object-literal members widen regardless of `const`/`let` (standard TS: a
/// `const obj = { a: 1 }` still has type `{ a: number }`), so widening is applied
/// here at member level — `declared_from_init` then leaves the object type alone.
/// Member inference recurses (`infer_expr`), so a nested object literal yields a
/// nested object type whose own members are already widened; widening such a
/// member is a no-op. A member whose value type cannot be inferred (out of
/// subset) is skipped — the object simply omits it, matching M0/M1 leniency.
/// Spread / computed / accessor members are likewise skipped (not in the subset).
fn infer_object_literal(
    interner: &mut Interner,
    binder: &Binder,
    decl_types: &DeclTypes,
    obj: &ObjectExpression<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> TypeId {
    let mut properties: Vec<PropertyType> = Vec::with_capacity(obj.properties.len());
    for member in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = member else {
            // Spread element — out of the M2 subset.
            continue;
        };
        let Some(name) = prop.key.static_name() else {
            // Computed / non-static key — out of the M2 subset.
            continue;
        };
        let Some((value_ty, _)) = infer_expr(interner, binder, decl_types, &prop.value, diagnostics)
        else {
            // Value type out of subset — omit this member.
            continue;
        };
        let widened = widen(interner, value_ty);
        properties.push(PropertyType {
            name: name.into_owned(),
            ty: widened,
            optional: false,
        });
    }
    interner.intern_object(ObjectType { properties })
}

/// Infer the type of a member access `obj.prop` (`StaticMemberExpression`).
///
/// The base is inferred first; the result is the property's type looked up off
/// the base object type. A **missing** property is `TK2339` with the property
/// name as the primary span, and the access yields the **error type** so nothing
/// cascades. An `any`/error base yields the error type directly (no `TK2339`). A
/// base that is neither an object nor `any`-like is out of the M2 subset (e.g.
/// member access on a primitive) and yields the error type without a diagnostic,
/// matching M0/M1 leniency. Returns `None` only when the base itself is out of
/// subset (`infer_expr` returned `None`).
fn infer_member_access(
    interner: &mut Interner,
    binder: &Binder,
    decl_types: &DeclTypes,
    member: &StaticMemberExpression<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(TypeId, Span)> {
    let wk = interner.well_known();
    let (base_ty, _) = infer_expr(interner, binder, decl_types, &member.object, diagnostics)?;
    let prop_name = member.property.name.as_str();
    let prop_span = Span::from_oxc(member.property.span);

    // An `any`/error base relates to everything and suppresses cascades; member
    // access off it is the error type, no `TK2339`.
    if base_ty == wk.any || base_ty == wk.error {
        return Some((wk.error, prop_span));
    }

    match interner.store().object_type(base_ty) {
        Some(obj) => match obj.property(prop_name) {
            // Resolved: the access has the property's type. Its span is the
            // property name (the primary span for any downstream diagnostic).
            Some(prop) => Some((prop.ty, prop_span)),
            // Missing property → TK2339; the access yields the error type so no
            // cascade `TK2322` follows on this expression.
            None => {
                let tgt = render_type(interner.store(), base_ty, /* widen */ false);
                diagnostics.push(Diagnostic::property_does_not_exist(
                    prop_span, prop_name, &tgt,
                ));
                Some((wk.error, prop_span))
            }
        },
        // Base is not an object type and not `any`-like — out of the M2 subset
        // (e.g. accessing a member of a primitive). Yield the error type to avoid
        // a spurious cascade, without emitting a diagnostic.
        None => Some((wk.error, prop_span)),
    }
}

/// Widen a type for object-literal member inference: a literal widens to its base
/// intrinsic (`1` → `number`); every other type (including object types) passes
/// through unchanged.
fn widen(interner: &mut Interner, ty: TypeId) -> TypeId {
    match interner.store().literal_value(ty) {
        Some(lit) => intrinsic_id(interner.well_known(), lit.base_kind()),
        None => ty,
    }
}

/// Well-known id for an intrinsic kind (small helper mirroring the relater's).
fn intrinsic_id(wk: WellKnown, kind: IntrinsicKind) -> TypeId {
    match kind {
        IntrinsicKind::Error => wk.error,
        IntrinsicKind::Any => wk.any,
        IntrinsicKind::Unknown => wk.unknown,
        IntrinsicKind::Never => wk.never,
        IntrinsicKind::Void => wk.void,
        IntrinsicKind::Null => wk.null,
        IntrinsicKind::Undefined => wk.undefined,
        IntrinsicKind::Boolean => wk.boolean,
        IntrinsicKind::Number => wk.number,
        IntrinsicKind::String => wk.string,
    }
}

//! The statement-level checker (architecture §5, mvp-plan §5 — M0/M1 rows).
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
use crate::relate::{Relater, Relation};
use crate::span::Span;
use crate::types::repr::{IntrinsicKind, LiteralValue};
use crate::types::store::TypeId;
use crate::types::{Interner, WellKnown};
use oxc_ast::ast::{
    AssignmentExpression, AssignmentOperator, AssignmentTarget, BindingPattern, Expression,
    Program, Statement, TSType, VariableDeclarationKind, VariableDeclarator,
};

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
        if let Relation::No(_chain) = relater.is_assignable(ob.src, ob.tgt) {
            // Source widened (literal → base), target as-is (mvp-plan M0/M1
            // message spec). The error type never reaches here: it is `any`-like,
            // so its obligations resolve to `Yes` and the cascade is suppressed.
            let src = render_type(store, ob.src, /* widen */ true);
            let tgt = render_type(store, ob.tgt, /* widen */ false);
            let message = format!("Type '{src}' is not assignable to type '{tgt}'");
            diagnostics.push(Diagnostic::not_assignable(ob.src_span, message));
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
        .and_then(|ann| lower_annotation(interner.well_known(), &ann.type_annotation));

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

/// Lower an annotation type to its `TypeId`. M1 still supports only the
/// intrinsic keyword types; object/union/reference/function annotations land in
/// later milestones and leave the declarator's annotation side absent.
fn lower_annotation(wk: WellKnown, ts_type: &TSType<'_>) -> Option<TypeId> {
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
        // TODO(M2+): object/union/reference/function annotations.
        _ => return None,
    };
    Some(id)
}

/// Infer the type of an expression, returning `(TypeId, span)`. The span is the
/// expression's own span — the primary span for any diagnostic on it.
///
/// Literals are interned as their *literal* type (widening is applied later,
/// only where a `let`/`var` declaration calls for it). `null` and the
/// `undefined` keyword map to their intrinsics. An identifier reference resolves
/// through the scope graph: a resolved name yields its declared type; an
/// unresolved name emits `TK2304` and yields the **error type** so no cascade
/// `TK2322` follows. Returns `None` for expression shapes outside the M1 subset
/// (those positions are simply not checked, matching M0 leniency).
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
        // TODO(M2+): object/array literals, member access, calls, etc.
        _ => None,
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

//! The statement-level checker (architecture §5, mvp-plan §5 — M0 row).
//!
//! M0 scope: a single construct — a top-level variable declaration with a type
//! annotation and a literal initializer, `const NAME: <T> = <literal>;` (also
//! `let`/`var`, treated identically in M0). For each, check the initializer type
//! is assignable to the annotation; on failure emit `TK2322` whose primary span
//! is the **initializer expression**.
//!
//! Annotation types in M0: the primitive/intrinsic keyword types only.
//! Initializers in M0: number/string/boolean literals, `null`, and the
//! `undefined` identifier.
//!
//! The structure (collect typed facts, then relate) is deliberately the seam the
//! later milestones grow into: M1 adds inference + name resolution, M2+ add the
//! object/function/union cases inside `lower_annotation` / `infer_initializer`.

use crate::diagnostics::{render_type, Diagnostic};
use crate::relate::{Relater, Relation};
use crate::span::Span;
use crate::types::repr::{IntrinsicKind, LiteralValue};
use crate::types::store::TypeId;
use crate::types::{Interner, WellKnown};
use oxc_ast::ast::{Expression, Program, Statement, TSType, VariableDeclarator};

/// One annotation-vs-initializer obligation collected from a declarator: the
/// annotation type, the initializer type, and the initializer's span (the
/// primary span of any resulting diagnostic).
struct AssignObligation {
    annotation: TypeId,
    initializer: TypeId,
    init_span: Span,
}

/// Check a parsed program and return the diagnostics it produces.
///
/// Two phases to respect the borrow checker cleanly: phase 1 interns the
/// annotation/initializer types (needs `&mut Interner`); phase 2 relates and
/// renders them (needs only `&Store`). No `unwrap`/`panic` on any path.
pub fn check_program(interner: &mut Interner, program: &Program<'_>) -> Vec<Diagnostic> {
    // --- Phase 1: collect typed obligations (mutable interner). ---
    let mut obligations: Vec<AssignObligation> = Vec::new();
    for stmt in &program.body {
        if let Statement::VariableDeclaration(decl) = stmt {
            for declarator in &decl.declarations {
                if let Some(obligation) = obligation_for(interner, declarator) {
                    obligations.push(obligation);
                }
            }
        }
        // Any other statement is out of M0 scope and ignored (no diagnostic).
    }

    // --- Phase 2: relate + render (immutable store borrow). ---
    let well_known = interner.well_known();
    let store = interner.store();
    let mut relater = Relater::new(store, well_known);

    let mut diagnostics = Vec::new();
    for ob in &obligations {
        if let Relation::No(_chain) = relater.is_assignable(ob.initializer, ob.annotation) {
            // Message renders the SOURCE widened (literal → base) and the TARGET
            // as-is (mvp-plan M0 spec). The reason chain is available for M6's
            // nested rendering; M0 uses the flat message.
            let src = render_type(store, ob.initializer, /* widen */ true);
            let tgt = render_type(store, ob.annotation, /* widen */ false);
            let message = format!("Type '{src}' is not assignable to type '{tgt}'");
            diagnostics.push(Diagnostic::not_assignable(ob.init_span, message));
        }
    }
    diagnostics
}

/// Build an obligation from a declarator, or `None` if it is out of M0 scope
/// (no annotation, no initializer, or a non-M0 annotation/initializer shape).
fn obligation_for(
    interner: &mut Interner,
    declarator: &VariableDeclarator<'_>,
) -> Option<AssignObligation> {
    // Need both an annotation and an initializer for the M0 rule.
    let annotation_node = declarator.type_annotation.as_ref()?;
    let init = declarator.init.as_ref()?;

    let well_known = interner.well_known();
    let annotation = lower_annotation(well_known, &annotation_node.type_annotation)?;
    let (initializer, init_span) = infer_initializer(interner, init)?;

    Some(AssignObligation {
        annotation,
        initializer,
        init_span,
    })
}

/// Lower an M0 annotation (an intrinsic keyword type) to its well-known
/// `TypeId`. Returns `None` for annotations outside M0 scope (objects, unions,
/// type references, …) — those declarators are simply not checked in M0.
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

/// Infer the type of an M0 initializer expression, returning `(TypeId, span)`.
/// The span is the initializer's own span — the primary span for diagnostics.
/// Returns `None` for initializers outside M0 scope.
///
/// Literals are interned as their *literal* type (so assignability logic can use
/// the literal); the diagnostic renderer widens them for display. `null` is the
/// `null` intrinsic; the `undefined` identifier is the `undefined` intrinsic.
fn infer_initializer(
    interner: &mut Interner,
    expr: &Expression<'_>,
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
        // The `undefined` keyword parses as an identifier reference; M0 treats
        // the literal identifier `undefined` as the `undefined` type. Any other
        // identifier is a name reference, which is out of M0 scope (M1: TK2304).
        Expression::Identifier(ident) if ident.name.as_str() == "undefined" => {
            Some((intrinsic_id(well_known, IntrinsicKind::Undefined), Span::from_oxc(ident.span)))
        }
        // TODO(M1+): other identifiers (name resolution), object/array literals,
        // calls, member access, etc.
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

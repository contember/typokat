//! The statement-level checker (architecture §5, mvp-plan §5 — M0–M4 rows).
//!
//! M4 scope, on top of M0–M3:
//!
//!  - **Union type annotations.** `A | B` lowers each member (recursing) and
//!    interns the result through [`Interner::union`], which flattens, sorts,
//!    dedups, drops `never`, and collapses degenerate unions (mvp-plan §3.3).
//!  - **Union member access.** `u.p` where `u` is a union requires `p` on *every*
//!    member; the result type is the `union(...)` of the per-member property
//!    types. A member missing `p` is `TK2339` on the union as a whole.
//!  - **Union assignability** is decided by the relation engine (a union source
//!    requires every member to relate; a union target requires some member to);
//!    the headline of a union-source failure names the specific failing member.
//!
//! M3 scope, on top of M0/M1/M2:
//!
//!  - **Function type annotations.** `(x: number) => string` lowers to an
//!    interned function `TypeId` (recursing into parameter and return types).
//!  - **Function inference.** A `function` expression, an arrow, and a named
//!    function **declaration** each infer a function type: every parameter type
//!    comes from its annotation (an un-annotated parameter is out of the MVP
//!    subset → the error type, no diagnostic); the **return type** is the
//!    annotation if present, otherwise inferred from the body (an expression-body
//!    arrow → the body expression's type, *widened*; a block body → the first
//!    `return <expr>`'s type, *widened*; no value return → `void`). A function
//!    declaration's type is bound into its value slot so a call resolves.
//!  - **Function bodies.** The checker descends into each body in the function's
//!    [`ScopeKind::Function`] scope (built by the binder) with the parameters
//!    bound, so `return x` resolves the parameter. When the function has a return
//!    **annotation**, each `return <expr>` is checked assignable to it → `TK2322`
//!    (primary span = the returned expression); a bare `return;` is fine under a
//!    `void` return. With no annotation, the return type is inferred and nothing
//!    is checked. **Missing-return analysis (`TK2355`) is deferred** (needs
//!    reachability), so a non-void function with no `return` is not an error.
//!  - **Calls.** A `CallExpression` whose callee is a function type is checked for
//!    **arity** (no optional params in M3: too many or too few arguments →
//!    `TK2554`, primary span = the call) and **argument assignability** (each
//!    argument assignable to the corresponding parameter → `TK2345`, primary span
//!    = the argument). The call's type is the function's return type. A
//!    non-function callee is out of scope (no diagnostic; the error type).
//!
//! M2 scope: object type annotations, object-literal inference (widened members),
//! member access (`TK2339`), structural assignability (`TK2741`/`TK2322`),
//! excess-property freshness (`TK2353`).
//!
//! M1 scope: binding & scope resolution, inference from initializer (`const`
//! keeps literals, `let`/`var` widen), variable references, unresolved names
//! (`TK2304` → error type, suppresses cascade), reassignment (`TK2322`).
//!
//! The two-phase shape from M0 is kept: phase 1 interns types, resolves names,
//! emits the name/arity/excess diagnostics, and **collects assignability
//! obligations**; phase 2 runs the relation engine over an immutable store and
//! emits the assignability diagnostics. Phases are split because interning needs
//! `&mut Interner` while the relater borrows the store immutably — they cannot be
//! held at once. No `unwrap`/`panic` on any path.

use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::binder::{bind_module, Binder};
use crate::diagnostics::{render_type, Diagnostic};
use crate::relate::{Reason, Relater, Relation};
use crate::span::Span;
use crate::types::repr::{
    FunctionType, IntrinsicKind, LiteralValue, ObjectType, ParameterType, PropertyType, TypeTag,
};
use crate::types::store::{Store, TypeId};
use crate::types::{Interner, WellKnown};
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AssignmentTarget,
    BindingPattern, CallExpression, Expression, FormalParameters, Function, FunctionBody,
    ObjectExpression, ObjectPropertyKind, Program, Statement, StaticMemberExpression, TSSignature,
    TSType, VariableDeclarationKind, VariableDeclarator,
};
use oxc_span::GetSpan;

/// Which diagnostic an assignability obligation produces on failure. The
/// structural verdict is the same relation query; only the code/message mapping
/// differs (mvp-plan §6 "code mapping").
#[derive(Copy, Clone, PartialEq, Eq)]
enum ObligationKind {
    /// Annotation-vs-initializer, reassignment, or `return`-vs-declared-return.
    /// Maps a missing-property reason to `TK2741`, everything else to `TK2322`.
    Assignment,
    /// A call argument vs its parameter. Any failure maps to `TK2345`.
    Argument,
}

/// One assignability obligation: `src` must be assignable to `tgt`, with the
/// resulting diagnostic's primary span at `src_span` and its code determined by
/// `kind`.
struct AssignObligation {
    src: TypeId,
    tgt: TypeId,
    src_span: Span,
    kind: ObligationKind,
}

/// Per-declaration computed types, indexed by `DeclId`. `None` means a
/// declaration whose type could not be computed (out of subset); a reference to
/// it resolves to the error type defensively.
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

/// The phase-1 working set threaded through the walk: everything the inference
/// pass writes to. Bundled into one struct so the many recursive `infer_*`
/// helpers take a single `&mut` rather than a long, churn-prone argument list.
struct Pass<'a> {
    interner: &'a mut Interner,
    binder: &'a Binder,
    decl_types: DeclTypes,
    obligations: Vec<AssignObligation>,
    diagnostics: Vec<Diagnostic>,
}

/// Check a parsed program and return the diagnostics it produces.
pub fn check_program(interner: &mut Interner, program: &Program<'_>) -> Vec<Diagnostic> {
    let binder = bind_module(program);
    let decl_types = DeclTypes::new(binder.decl_count);

    let mut pass = Pass {
        interner,
        binder: &binder,
        decl_types,
        obligations: Vec::new(),
        diagnostics: Vec::new(),
    };

    // --- Phase 1: bind-resolved walk over the module body. ---
    check_statements(&mut pass, binder.module, &program.body);

    // Move the working set out before borrowing the store immutably for phase 2.
    let Pass {
        interner,
        obligations,
        mut diagnostics,
        ..
    } = pass;

    // --- Phase 2: relate + render obligations (immutable store borrow). ---
    let well_known = interner.well_known();
    let store = interner.store();
    let mut relater = Relater::new(store, well_known);

    for ob in &obligations {
        if let Relation::No(chain) = relater.is_assignable(ob.src, ob.tgt) {
            emit_obligation_failure(store, ob, chain.head(), &mut diagnostics);
        }
    }

    diagnostics
}

/// Map a relation failure to a diagnostic according to the obligation's kind.
///
/// `Assignment`: a required-target-property absence → `TK2741`; everything else
/// (primitive mismatch, a present-but-wrong property, or any function-shaped
/// mismatch — possibly nested) → `TK2322`. The error type never reaches here (it
/// is `any`-like, so its obligations resolve to `Yes`). `Argument`: any failure →
/// `TK2345`. Nested reasons are built for M6; the flat top-level message is
/// rendered here.
fn emit_obligation_failure(
    store: &Store,
    ob: &AssignObligation,
    head: &Reason,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match ob.kind {
        ObligationKind::Assignment => match head {
            Reason::MissingProperty { name, tgt, .. } => {
                let tgt = render_type(store, *tgt, /* widen */ false);
                diagnostics.push(Diagnostic::property_missing(ob.src_span, name, &tgt));
            }
            Reason::Leaf { .. }
            | Reason::Property { .. }
            | Reason::ParameterCount { .. }
            | Reason::Parameter { .. }
            | Reason::ReturnType { .. }
            | Reason::UnionSourceMember { .. }
            | Reason::NoUnionMember { .. } => {
                // Source widened (literal → base), target as-is (mvp-plan
                // M0/M1 message spec). For a union source the headline names the
                // specific failing member, not the whole union (matching tsc:
                // `number | string` → `number` reports `'string'`).
                let src = render_type(store, headline_src(ob, head), /* widen */ true);
                let tgt = render_type(store, ob.tgt, /* widen */ false);
                let message = format!("Type '{src}' is not assignable to type '{tgt}'");
                diagnostics.push(Diagnostic::not_assignable(ob.src_span, message));
            }
        },
        ObligationKind::Argument => {
            let src = render_type(store, headline_src(ob, head), /* widen */ true);
            let tgt = render_type(store, ob.tgt, /* widen */ false);
            diagnostics.push(Diagnostic::argument_not_assignable(ob.src_span, &src, &tgt));
        }
    }
}

/// The source type to put in the headline message. Normally the obligation's
/// source, but for a **union source** failure it is the specific offending member
/// (`number | string` not assignable to `number` reports the failing `string`,
/// matching tsc) — the whole-union form is reserved for the nested reason chain
/// (M6).
fn headline_src(ob: &AssignObligation, head: &Reason) -> TypeId {
    match head {
        Reason::UnionSourceMember { member, .. } => *member,
        _ => ob.src,
    }
}

/// Check a list of statements in `scope`.
fn check_statements(pass: &mut Pass, scope: ScopeId, statements: &[Statement<'_>]) {
    for stmt in statements {
        check_statement(pass, scope, stmt);
    }
}

/// Check one statement in `scope`.
fn check_statement(pass: &mut Pass, scope: ScopeId, stmt: &Statement<'_>) {
    match stmt {
        Statement::VariableDeclaration(decl) => {
            for declarator in &decl.declarations {
                check_declarator(pass, scope, decl.kind, declarator);
            }
        }
        Statement::FunctionDeclaration(func) => {
            check_function_declaration(pass, scope, func);
        }
        Statement::ExpressionStatement(expr_stmt) => {
            if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                check_assignment(pass, scope, assign);
            } else {
                // Other expression statements are still inferred so nested calls /
                // functions inside them are checked (e.g. a bare `f(1)`).
                infer_expr(pass, scope, &expr_stmt.expression);
            }
        }
        // Other statements are out of the M3 subset. (`return` is handled inside
        // the function-body walk, not at the module level.)
        _ => {}
    }
}

/// Check one variable declarator and record its declared/inferred type.
///
/// The declared type is the annotation if present, otherwise the (possibly
/// widened) initializer type. When both are present, an assignability obligation
/// is collected and a fresh object literal gets an excess-property check.
fn check_declarator(
    pass: &mut Pass,
    scope: ScopeId,
    kind: VariableDeclarationKind,
    declarator: &VariableDeclarator<'_>,
) {
    let decl_id = binding_decl_id(pass.binder, scope, &declarator.id);

    // Infer the initializer first (it may resolve references / emit TK2304 and
    // descends into any nested function body), independent of the annotation.
    let initializer = declarator
        .init
        .as_ref()
        .and_then(|init| infer_expr(pass, scope, init));

    let annotation = declarator
        .type_annotation
        .as_ref()
        .and_then(|ann| lower_annotation(pass.interner, &ann.type_annotation));

    // The declared type the symbol resolves to: annotation wins; otherwise the
    // (possibly widened) initializer type.
    let declared = match (annotation, &initializer) {
        (Some(ann), _) => Some(ann),
        (None, Some((init_ty, _))) => Some(declared_from_init(pass.interner, kind, *init_ty)),
        (None, None) => None,
    };
    if let (Some(decl_id), Some(ty)) = (decl_id, declared) {
        pass.decl_types.set(decl_id, ty);
    }

    // When both sides are present, the initializer must be assignable to the
    // annotation (primary span = the initializer).
    if let (Some(ann), Some((init_ty, init_span))) = (annotation, initializer) {
        pass.obligations.push(AssignObligation {
            src: init_ty,
            tgt: ann,
            src_span: init_span,
            kind: ObligationKind::Assignment,
        });

        // Excess-property check (freshness) for a fresh object literal target.
        if let Some(init_expr) = declarator.init.as_ref() {
            check_excess_properties(pass.interner.store(), init_expr, ann, &mut pass.diagnostics);
        }
    }
}

/// Check a function declaration: compute its function type, bind it into the
/// value slot (so a call resolves), and descend into its body.
fn check_function_declaration(pass: &mut Pass, scope: ScopeId, func: &Function<'_>) {
    let fn_ty = infer_function(pass, scope, func);
    if let Some(id) = &func.id {
        if let Some(decl_id) = pass
            .binder
            .graph
            .resolve(scope, id.name.as_str())
            .and_then(|symbol_id| pass.binder.symbols.get(symbol_id))
            .and_then(|s| s.value)
        {
            pass.decl_types.set(decl_id, fn_ty);
        }
    }
}

/// Recursive excess-property (freshness) check (mvp-plan §6, README
/// `excess_property.ts`). Unchanged from M2.
fn check_excess_properties(
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
                check_excess_properties(store, &prop.value, target_prop.ty, diagnostics);
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

/// Check a reassignment `NAME = <expr>` (a simple `=` to an identifier target) in
/// `scope`. The RHS must be assignable to the target's declared type → `TK2322`
/// with the RHS as the primary span. An unresolved target is `TK2304`.
fn check_assignment(pass: &mut Pass, scope: ScopeId, assign: &AssignmentExpression<'_>) {
    if assign.operator != AssignmentOperator::Assign {
        return;
    }
    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
        return;
    };

    // Infer the RHS first so any reference inside it resolves (and emits TK2304
    // before we look at the target), and any nested function body is checked.
    let rhs = infer_expr(pass, scope, &assign.right);

    let target_ty = match pass.binder.graph.resolve(scope, target.name.as_str()) {
        Some(symbol_id) => pass
            .binder
            .symbols
            .get(symbol_id)
            .and_then(|s| s.value)
            .and_then(|decl_id| pass.decl_types.get(decl_id)),
        None => {
            pass.diagnostics.push(Diagnostic::cannot_find_name(
                Span::from_oxc(target.span),
                target.name.as_str(),
            ));
            return;
        }
    };

    if let (Some(tgt), Some((src, src_span))) = (target_ty, rhs) {
        pass.obligations.push(AssignObligation {
            src,
            tgt,
            src_span,
            kind: ObligationKind::Assignment,
        });
    }
}

/// The declared type for an initializer-only declaration, applying widening:
/// `let`/`var` widen a literal init to its base intrinsic; `const`/`using` keep
/// the literal. Non-literal inits (objects, functions) pass through unchanged.
fn declared_from_init(
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
fn binding_decl_id(binder: &Binder, scope: ScopeId, pattern: &BindingPattern<'_>) -> Option<DeclId> {
    let name = match pattern {
        BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
        _ => return None,
    };
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}

/// Lower an annotation type to its `TypeId`. M3 supports intrinsic keywords,
/// object type literals, and function types (`(x: number) => string`); union /
/// reference annotations land in later milestones and leave the annotation side
/// absent.
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
        TSType::TSFunctionType(func) => {
            return lower_function_annotation(
                interner,
                &func.params,
                &func.return_type.type_annotation,
            );
        }
        TSType::TSUnionType(union) => return lower_union_annotation(interner, &union.types),
        TSType::TSParenthesizedType(paren) => {
            return lower_annotation(interner, &paren.type_annotation);
        }
        // TODO(M5+): reference annotations (type aliases / interfaces).
        _ => return None,
    };
    Some(id)
}

/// Lower a union type annotation `A | B | …` to a canonical interned `TypeId`
/// (M4). Each member is lowered recursively, then `Interner::union` flattens,
/// sorts, dedups, drops `never`, and collapses degenerate unions (mvp-plan
/// §3.3). A member whose type cannot be lowered (out of subset) aborts the whole
/// annotation (`None`), matching the object/function lowering — dropping a member
/// silently would mis-state the union.
fn lower_union_annotation(interner: &mut Interner, members: &[TSType<'_>]) -> Option<TypeId> {
    let mut lowered: Vec<TypeId> = Vec::with_capacity(members.len());
    for member in members {
        lowered.push(lower_annotation(interner, member)?);
    }
    Some(interner.union(lowered))
}

/// Lower an object type literal's members to an interned object `TypeId`.
/// Unchanged from M2. A member whose type cannot be lowered (or an
/// optional/index/call/method/construct signature) aborts the lowering (`None`).
fn lower_object_annotation(interner: &mut Interner, members: &[TSSignature<'_>]) -> Option<TypeId> {
    let mut properties: Vec<PropertyType> = Vec::with_capacity(members.len());
    for member in members {
        let TSSignature::TSPropertySignature(sig) = member else {
            return None;
        };
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

/// Lower a function type annotation's parameters and return type to an interned
/// function `TypeId`. Parameters are kept **positional** (never sorted). A
/// parameter without a type annotation, or one whose type cannot be lowered, or
/// any optional/rest parameter, aborts the lowering (`None`) — these are out of
/// the M3 subset and dropping them silently would mis-state the type.
fn lower_function_annotation(
    interner: &mut Interner,
    params: &FormalParameters<'_>,
    return_type: &TSType<'_>,
) -> Option<TypeId> {
    // Rest parameters are out of the M3 subset.
    if params.rest.is_some() {
        return None;
    }
    let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
    for param in &params.items {
        // Optional parameters change the relation rule; abort rather than treat
        // as required.
        if param.optional {
            return None;
        }
        let name = parameter_name(&param.pattern)?;
        let annotation = param.type_annotation.as_ref()?;
        let ty = lower_annotation(interner, &annotation.type_annotation)?;
        lowered.push(ParameterType {
            name,
            ty,
            optional: false,
        });
    }
    let ret = lower_annotation(interner, return_type)?;
    Some(interner.intern_function(FunctionType {
        params: lowered,
        ret,
    }))
}

/// Infer the type of an expression in `scope`, returning `(TypeId, span)`. The
/// span is the expression's own span — the primary span for any diagnostic on it.
/// Returns `None` for expression shapes outside the subset (those positions are
/// simply not checked, matching M0 leniency).
fn infer_expr(pass: &mut Pass, scope: ScopeId, expr: &Expression<'_>) -> Option<(TypeId, Span)> {
    let well_known = pass.interner.well_known();
    match expr {
        Expression::NumericLiteral(lit) => {
            let id = pass.interner.intern_literal(LiteralValue::Number(lit.value));
            Some((id, Span::from_oxc(lit.span)))
        }
        Expression::StringLiteral(lit) => {
            let id = pass
                .interner
                .intern_literal(LiteralValue::String(lit.value.to_string()));
            Some((id, Span::from_oxc(lit.span)))
        }
        Expression::BooleanLiteral(lit) => {
            let id = pass.interner.intern_literal(LiteralValue::Boolean(lit.value));
            Some((id, Span::from_oxc(lit.span)))
        }
        Expression::NullLiteral(lit) => Some((well_known.null, Span::from_oxc(lit.span))),
        Expression::ObjectExpression(obj) => {
            let id = infer_object_literal(pass, scope, obj);
            Some((id, Span::from_oxc(obj.span)))
        }
        Expression::StaticMemberExpression(member) => infer_member_access(pass, scope, member),
        Expression::CallExpression(call) => infer_call(pass, scope, call),
        Expression::FunctionExpression(func) => {
            let id = infer_function(pass, scope, func);
            Some((id, Span::from_oxc(func.span)))
        }
        Expression::ArrowFunctionExpression(arrow) => {
            let id = infer_arrow(pass, scope, arrow);
            Some((id, Span::from_oxc(arrow.span)))
        }
        Expression::ParenthesizedExpression(paren) => infer_expr(pass, scope, &paren.expression),
        Expression::Identifier(ident) => {
            let span = Span::from_oxc(ident.span);
            // The `undefined` keyword parses as an identifier reference.
            if ident.name.as_str() == "undefined" {
                return Some((well_known.undefined, span));
            }
            match pass.binder.graph.resolve(scope, ident.name.as_str()) {
                Some(symbol_id) => {
                    let ty = pass
                        .binder
                        .symbols
                        .get(symbol_id)
                        .and_then(|s| s.value)
                        .and_then(|decl_id| pass.decl_types.get(decl_id))
                        // A resolved symbol with no computed type yet (out of
                        // subset) falls back to the error type — no cascade.
                        .unwrap_or(well_known.error);
                    Some((ty, span))
                }
                None => {
                    pass.diagnostics
                        .push(Diagnostic::cannot_find_name(span, ident.name.as_str()));
                    Some((well_known.error, span))
                }
            }
        }
        // TODO(M4+): array literals, etc.
        _ => None,
    }
}

/// Infer the type of an object literal in `scope`. Unchanged from M2: member
/// types are widened (`{ a: 1 }` → `{ a: number }`).
fn infer_object_literal(pass: &mut Pass, scope: ScopeId, obj: &ObjectExpression<'_>) -> TypeId {
    let mut properties: Vec<PropertyType> = Vec::with_capacity(obj.properties.len());
    for member in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = member else {
            continue;
        };
        let Some(name) = prop.key.static_name() else {
            continue;
        };
        let Some((value_ty, _)) = infer_expr(pass, scope, &prop.value) else {
            continue;
        };
        let widened = widen(pass.interner, value_ty);
        properties.push(PropertyType {
            name: name.into_owned(),
            ty: widened,
            optional: false,
        });
    }
    pass.interner.intern_object(ObjectType { properties })
}

/// Infer the type of a member access `obj.prop` in `scope`. A missing property is
/// `TK2339` and yields the error type (no cascade); an `any`/error base yields the
/// error type.
///
/// M4 adds the **union** base: `u.p` where `u` is a union requires `p` on *every*
/// member; the result type is the `union(...)` of the per-member property types.
/// If any member lacks the property, it is `TK2339` on the union as a whole (and
/// the result is the error type, suppressing cascade).
fn infer_member_access(
    pass: &mut Pass,
    scope: ScopeId,
    member: &StaticMemberExpression<'_>,
) -> Option<(TypeId, Span)> {
    let wk = pass.interner.well_known();
    let (base_ty, _) = infer_expr(pass, scope, &member.object)?;
    let prop_name = member.property.name.as_str();
    let prop_span = Span::from_oxc(member.property.span);

    if base_ty == wk.any || base_ty == wk.error {
        return Some((wk.error, prop_span));
    }

    // Union base (M4): the property must exist on every member; its type is the
    // union of the per-member property types.
    if pass.interner.store().tag(base_ty) == TypeTag::Union {
        return Some((
            union_member_access(pass, base_ty, prop_name, prop_span),
            prop_span,
        ));
    }

    match pass.interner.store().object_type(base_ty) {
        Some(obj) => match obj.property(prop_name) {
            Some(prop) => Some((prop.ty, prop_span)),
            None => {
                let tgt = render_type(pass.interner.store(), base_ty, /* widen */ false);
                pass.diagnostics.push(Diagnostic::property_does_not_exist(
                    prop_span, prop_name, &tgt,
                ));
                Some((wk.error, prop_span))
            }
        },
        None => Some((wk.error, prop_span)),
    }
}

/// Resolve `union.prop` (M4): collect each member's type for `prop`, requiring it
/// on **every** member. The result is the `union(...)` of those per-member types
/// (canonicalized by the interner). If any member lacks the property, emit a
/// single `TK2339` against the whole union and return the error type.
///
/// A member that is itself `any`/error contributes the error type (its `prop` is
/// assumed to exist). A member that is neither an object nor `any`/error has no
/// known property in the MVP subset, so it counts as "missing" → `TK2339`.
fn union_member_access(
    pass: &mut Pass,
    union_ty: TypeId,
    prop_name: &str,
    prop_span: Span,
) -> TypeId {
    let wk = pass.interner.well_known();

    // Snapshot the member ids: the per-member lookups below are immutable, but
    // interning the result union needs `&mut`, so the borrow must not be held.
    let Some(members) = pass.interner.store().union_members(union_ty) else {
        return wk.error;
    };
    let members: Vec<TypeId> = members.to_vec();

    let mut member_prop_types: Vec<TypeId> = Vec::with_capacity(members.len());
    for member in members {
        let store = pass.interner.store();
        if member == wk.any || member == wk.error {
            member_prop_types.push(wk.error);
            continue;
        }
        match store.object_type(member).and_then(|o| o.property(prop_name)) {
            Some(prop) => member_prop_types.push(prop.ty),
            // Missing on this member: the property does not exist on the union.
            None => {
                let tgt = render_type(pass.interner.store(), union_ty, /* widen */ false);
                pass.diagnostics.push(Diagnostic::property_does_not_exist(
                    prop_span, prop_name, &tgt,
                ));
                return wk.error;
            }
        }
    }

    // Present on every member: the result is the union of the per-member types.
    pass.interner.union(member_prop_types)
}

/// Infer the type of a call expression in `scope` and check it.
///
/// The callee is inferred first (resolving its name / descending into a callee
/// expression). When it is a function type:
///
///  - **arity** (no optional/rest params in M3): too many or too few arguments →
///    `TK2554` (primary span = the call), and
///  - each **argument** is collected as an assignability obligation against the
///    corresponding parameter → `TK2345` (primary span = the argument), paired up
///    to the lesser of the two counts.
///
/// The call's type is the function's return type. A non-function callee is out of
/// scope (no diagnostic) and yields the error type. Arguments are always inferred
/// (so nested calls/functions inside them are checked) even when the callee is
/// not a function.
fn infer_call(
    pass: &mut Pass,
    scope: ScopeId,
    call: &CallExpression<'_>,
) -> Option<(TypeId, Span)> {
    let wk = pass.interner.well_known();
    let call_span = Span::from_oxc(call.span);

    let callee = infer_expr(pass, scope, &call.callee);

    // Infer every argument up front (skipping spreads — out of the M3 subset);
    // this also descends into nested calls/functions inside the arguments.
    let mut arg_types: Vec<(TypeId, Span)> = Vec::with_capacity(call.arguments.len());
    for arg in &call.arguments {
        if let Some(arg_expr) = arg.as_expression() {
            if let Some(inferred) = infer_expr(pass, scope, arg_expr) {
                arg_types.push(inferred);
            }
        }
        // A spread or an out-of-subset argument is not paired against a parameter.
    }

    let Some((callee_ty, _)) = callee else {
        return Some((wk.error, call_span));
    };

    // Snapshot the callee's parameter types + return type so the immutable store
    // borrow does not overlap pushing obligations / diagnostics below.
    let Some(func) = pass.interner.store().function_type(callee_ty) else {
        // Non-function callee — out of scope. No diagnostic; error-typed result.
        return Some((wk.error, call_span));
    };
    let param_types: Vec<TypeId> = func.params.iter().map(|p| p.ty).collect();
    let ret = func.ret;

    // Arity: no optional/rest params in M3, so the counts must match exactly.
    if arg_types.len() != param_types.len() {
        pass.diagnostics.push(Diagnostic::wrong_argument_count(
            call_span,
            param_types.len(),
            arg_types.len(),
        ));
    }

    // Argument assignability: pair each argument with its parameter up to the
    // lesser count (the surplus on either side is already reported as an arity
    // error above). Each pairing is an obligation resolved in phase 2.
    for ((arg_ty, arg_span), param_ty) in arg_types.iter().zip(&param_types) {
        pass.obligations.push(AssignObligation {
            src: *arg_ty,
            tgt: *param_ty,
            src_span: *arg_span,
            kind: ObligationKind::Argument,
        });
    }

    Some((ret, call_span))
}

/// Infer a `function` declaration/expression's type and check its body.
fn infer_function(pass: &mut Pass, enclosing: ScopeId, func: &Function<'_>) -> TypeId {
    let fn_scope = pass.binder.fn_scopes.get(&func.span.start).copied();
    let params = lower_parameters(pass, fn_scope, &func.params);

    // Declared return type from the annotation, if any.
    let declared_ret = func
        .return_type
        .as_ref()
        .and_then(|ann| lower_annotation(pass.interner, &ann.type_annotation));

    // Descend into the body (in the function scope) to check returns against a
    // declared return type and/or infer the return type from `return` statements.
    let body_scope = fn_scope.unwrap_or(enclosing);
    let inferred_ret = func
        .body
        .as_ref()
        .map(|body| check_function_body(pass, body_scope, body, declared_ret));

    let ret = resolve_return_type(pass.interner, declared_ret, inferred_ret);
    pass.interner
        .intern_function(FunctionType { params, ret })
}

/// Infer an arrow's type and check its body. An expression-body arrow's return
/// type is the body expression's type (widened when not annotated).
fn infer_arrow(
    pass: &mut Pass,
    enclosing: ScopeId,
    arrow: &ArrowFunctionExpression<'_>,
) -> TypeId {
    let fn_scope = pass.binder.fn_scopes.get(&arrow.span.start).copied();
    let params = lower_parameters(pass, fn_scope, &arrow.params);

    let declared_ret = arrow
        .return_type
        .as_ref()
        .and_then(|ann| lower_annotation(pass.interner, &ann.type_annotation));

    let body_scope = fn_scope.unwrap_or(enclosing);

    let inferred_ret = if let Some(body_expr) = arrow.get_expression() {
        // Expression body `() => expr`: the return value is the expression.
        let value = infer_expr(pass, body_scope, body_expr);
        match (declared_ret, value) {
            // With a declared return type, the body expression is checked against
            // it (primary span = the expression), like a `return <expr>`.
            (Some(ret), Some((src, src_span))) => {
                pass.obligations.push(AssignObligation {
                    src,
                    tgt: ret,
                    src_span,
                    kind: ObligationKind::Assignment,
                });
                None
            }
            // No annotation: infer the return type from the body, widened.
            (None, Some((value_ty, _))) => Some(widen(pass.interner, value_ty)),
            _ => None,
        }
    } else {
        // Block body `() => { ... }`: same as a function body.
        Some(check_function_body(pass, body_scope, &arrow.body, declared_ret))
    };

    let ret = resolve_return_type(pass.interner, declared_ret, inferred_ret);
    pass.interner
        .intern_function(FunctionType { params, ret })
}

/// Lower a function's/arrow's parameters to `ParameterType`s and, when a function
/// scope is known, record each parameter's type in `decl_types` so the body can
/// resolve it. An un-annotated parameter is out of the MVP subset → the error
/// type (no diagnostic), matching M0/M1 leniency. Parameters are positional.
fn lower_parameters(
    pass: &mut Pass,
    fn_scope: Option<ScopeId>,
    params: &FormalParameters<'_>,
) -> Vec<ParameterType> {
    let error_ty = pass.interner.well_known().error;
    let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
    for param in &params.items {
        let name = parameter_name(&param.pattern).unwrap_or_default();
        // Annotated type, or the error type for an un-annotated parameter.
        let ty = param
            .type_annotation
            .as_ref()
            .and_then(|ann| lower_annotation(pass.interner, &ann.type_annotation))
            .unwrap_or(error_ty);

        // Bind the parameter's type into the function scope so the body resolves
        // it (the binder declared the parameter symbol + DeclId).
        if let Some(scope) = fn_scope {
            if let Some(decl_id) = parameter_name(&param.pattern)
                .and_then(|n| pass.binder.graph.resolve(scope, &n))
                .and_then(|symbol_id| pass.binder.symbols.get(symbol_id))
                .and_then(|s| s.value)
            {
                pass.decl_types.set(decl_id, ty);
            }
        }

        lowered.push(ParameterType {
            name,
            ty,
            optional: false,
        });
    }
    lowered
}

/// Walk a function body in `scope`, checking each `return <expr>` against a
/// declared return type and inferring a return type when none is declared.
///
/// Returns the **inferred** return type (used only when there is no declared
/// return type): the first `return <expr>`'s widened type, or `void` if no value
/// return is found (a bare `return;` or no `return` at all). When a return type
/// *is* declared, each `return <expr>` is collected as an assignability
/// obligation (primary span = the expression); a bare `return;` is left to the
/// `void` rule in phase 2. Missing-return analysis (`TK2355`) is deferred, so an
/// empty body under a non-void declared return is **not** an error.
fn check_function_body(
    pass: &mut Pass,
    scope: ScopeId,
    body: &FunctionBody<'_>,
    declared_ret: Option<TypeId>,
) -> TypeId {
    let void_ty = pass.interner.well_known().void;
    let mut inferred: Option<TypeId> = None;

    for stmt in &body.statements {
        check_return_in_statement(pass, scope, stmt, declared_ret, &mut inferred);
    }

    inferred.unwrap_or(void_ty)
}

/// Check `return` statements within a body statement (and descend into nested
/// expressions for nested function bodies). Nested functions introduce their own
/// `return` scope and are handled when their own body is walked, so a `return`
/// inside a nested function does not bind to the outer function here.
fn check_return_in_statement(
    pass: &mut Pass,
    scope: ScopeId,
    stmt: &Statement<'_>,
    declared_ret: Option<TypeId>,
    inferred: &mut Option<TypeId>,
) {
    match stmt {
        Statement::ReturnStatement(ret) => {
            // A bare `return;` is fine under a `void` return (handled by the
            // `void` rule in phase 2) and contributes no inferred type, so only a
            // value return (`return <expr>;`) is processed here.
            if let Some(arg) = &ret.argument {
                if let Some((src, src_span)) = infer_expr(pass, scope, arg) {
                    match declared_ret {
                        // Declared return type: check the returned expression
                        // against it (primary span = the expression).
                        Some(tgt) => {
                            pass.obligations.push(AssignObligation {
                                src,
                                tgt,
                                src_span,
                                kind: ObligationKind::Assignment,
                            });
                        }
                        // No annotation: infer from the first value return,
                        // widened (`return 1` → `number`).
                        None => {
                            if inferred.is_none() {
                                *inferred = Some(widen(pass.interner, src));
                            }
                        }
                    }
                }
            }
        }
        // Recurse into other statements so a nested function/call inside the body
        // is still checked, but `return`s inside a nested function bind to that
        // function (handled when its body is walked), not to this one.
        Statement::ExpressionStatement(expr_stmt) => {
            if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                check_assignment(pass, scope, assign);
            } else {
                infer_expr(pass, scope, &expr_stmt.expression);
            }
        }
        Statement::VariableDeclaration(decl) => {
            for declarator in &decl.declarations {
                check_declarator(pass, scope, decl.kind, declarator);
            }
        }
        Statement::FunctionDeclaration(func) => {
            check_function_declaration(pass, scope, func);
        }
        _ => {}
    }
}

/// The function's return type: a declared annotation always wins; otherwise the
/// inferred type; otherwise `void` (a function with no body and no annotation,
/// which is out of the subset but handled defensively).
fn resolve_return_type(
    interner: &mut Interner,
    declared: Option<TypeId>,
    inferred: Option<TypeId>,
) -> TypeId {
    declared
        .or(inferred)
        .unwrap_or_else(|| interner.well_known().void)
}

/// The parameter name of a binding pattern, if it is a plain identifier. `None`
/// for destructuring patterns (out of the M3 subset).
fn parameter_name(pattern: &BindingPattern<'_>) -> Option<String> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.to_string()),
        _ => None,
    }
}

/// Widen a type: a literal widens to its base intrinsic (`1` → `number`); every
/// other type passes through unchanged.
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

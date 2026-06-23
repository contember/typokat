//! The statement-level checker (architecture §5, mvp-plan §5 — M0–M6 rows, plus
//! the post-MVP M7 narrowing slice).
//!
//! M7 scope (post-MVP — control-flow narrowing), on top of M0–M6:
//!
//!  - **Control-flow narrowing of a union-typed local/parameter** inside `if`/
//!    `else` branches, for three guard families: `typeof x === "string" | "number"
//!    | "boolean"` (and `!==`/`==`/`!=`, either operand order), **truthiness**
//!    (`if (x)` / `if (!x)`), and **`null`/`undefined` equality** (`x === null` /
//!    `x !== undefined`, either operand order). A guarded reference uses the
//!    narrowed type wherever the checker resolves an identifier
//!    (`resolve_identifier_type`) — assignment sources, member-access bases,
//!    returned expressions — so the same `TK2322`/`TK2339` that fires on the wide
//!    union is clean inside the branch.
//!  - This is the first **structured-control-flow** slice of the §5 flow
//!    interpreter: a `SymbolId → TypeId` **narrowing environment** (`Pass::narrowed`)
//!    layered on the in-order statement walk, with an `if`/`else`
//!    **fork-and-restore** (`check_if`) that checks the then-branch under the
//!    positive guard fact, the else-branch under its complement, and **restores**
//!    the pre-`if` environment afterwards (narrowing never escapes the `if`). The
//!    reusable narrowing *operations* live in `flow.rs`; only the env/driver and the
//!    guard analysis (`analyze_guard`) live here. Unstructured-flow narrowing
//!    (early `return`/`throw` join, loops, `switch`), `in`-operator, discriminated
//!    unions, and assertion functions are deferred to the flow-node CFG (M8+).
//!  - **Soundness** is keyed on the **specific `SymbolId`** (narrowing `x` never
//!    touches `y`, a property access, or a shadowed binding), narrowing **resets on
//!    reassignment**, an **unrecognized guard narrows nothing** (no false
//!    negatives), and a **function boundary resets** the environment.
//!  - The binder gives every `{ … }` block its own `ScopeKind::Block` scope (so a
//!    branch-local `let`/`const` does not leak), and the unified statement walker
//!    (`check_stmt`) handles `if`/block in both the module top level and function
//!    bodies.
//!
//! M5 scope, on top of M0–M4:
//!
//!  - **`type` aliases** (`TSTypeAliasDeclaration`) are **transparent**: a
//!    reference to `type Num = number` resolves to `number`; `type Pair = { … }`
//!    resolves to the (structurally interned) object type. The alias name lives in
//!    the binder's type slot; the checker maps its type `DeclId` to the resolved
//!    target `TypeId`.
//!  - **`interface` declarations** (`TSInterfaceDeclaration`, single declaration —
//!    no merging) are **nominal** named object types: each gets its own object id
//!    (never structurally hash-consed), so member access and the structural
//!    relation reuse the M2 object rules.
//!  - **Type references** in annotations (`TSTypeReference` — `Point`, `Num`,
//!    `List`) resolve through the binder's type slot to the referenced `TypeId`.
//!  - **Recursive & mutually-recursive types** are made lowerable by a **two-phase
//!    reserve-then-fill** of the type environment ([`build_type_env`]): every type
//!    declaration's `TypeId` is reserved *before* any body is resolved, so a body
//!    can reference itself (`interface List { tail: List | null }`) or a sibling
//!    (`Ping`/`Pong`). A named reference is stored as the referenced **id** — never
//!    expanded inline — so lowering terminates, and the relation engine's
//!    assume-true cycle stack (§6.3) makes relating recursive types terminate too.
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
use crate::binder::symbol::{DeclId, SymbolId};
use crate::binder::{bind_module, Binder};
use crate::check::flow::{narrow, NarrowOp, TypeofTag};
use crate::diagnostics::{render_reason_chain, render_type, Diagnostic};
use crate::relate::{Reason, Relater, Relation};
use crate::span::Span;
use crate::types::repr::{
    FunctionType, IntrinsicKind, LiteralValue, ObjectType, ParameterType, PropertyType, TypeTag,
};
use crate::types::store::{Store, TypeId};
use crate::types::{Interner, WellKnown};
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AssignmentTarget,
    BinaryExpression, BinaryOperator, BindingPattern, BlockStatement, CallExpression, Expression,
    FormalParameters, Function, FunctionBody, IfStatement, LogicalExpression, ObjectExpression,
    ObjectPropertyKind, Program, Statement, StaticMemberExpression, TSSignature, TSType, TSTypeName,
    UnaryExpression, UnaryOperator, VariableDeclarationKind, VariableDeclarator,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;

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

/// How a top-level type declaration lowers (M5), indexed by its type-space
/// `DeclId`. Collected up front so every declaration's `TypeId` is reserved before
/// any body is resolved — the reserve-then-fill that makes recursive/mutual
/// references lowerable (mvp-plan M5, §6.3). The `'ast` borrows the AST bodies.
enum TypeDecl<'ast> {
    /// An `interface` — a **nominal** named object type. `reserved` is its id
    /// (allocated empty via `Interner::reserve_object`, filled once the members are
    /// lowered); `members` is the interface body, lowered in the fill step.
    Interface {
        reserved: TypeId,
        members: &'ast [TSSignature<'ast>],
    },
    /// A `type` alias — **transparent**. `annotation` is the aliased type, lowered
    /// on demand to the target id; `resolving` guards a recursive alias (out of the
    /// M5 subset — broken by yielding the error type rather than looping).
    Alias {
        annotation: &'ast TSType<'ast>,
        resolving: bool,
    },
}

/// The phase-1 working set threaded through the walk: everything the inference
/// pass writes to. Bundled into one struct so the many recursive `infer_*`/
/// `lower_*` helpers take a single `&mut` rather than a long, churn-prone argument
/// list.
struct Pass<'a, 'ast> {
    interner: &'a mut Interner,
    binder: &'a Binder,
    /// Named-type declarations (M5), indexed by type-space `DeclId`. Reserve-then-
    /// fill populates this in phase 0; a `TSTypeReference` resolves through the
    /// binder's type slot to a `DeclId`, then to a `TypeId` via `type_resolved`.
    type_decls: Vec<TypeDecl<'ast>>,
    /// Resolved named types (M5), indexed by type-space `DeclId`. An interface's
    /// entry is its reserved id; an alias's is filled on first resolution. `None`
    /// means unresolved (out of subset / recursive alias → error type). A
    /// `TSTypeReference` reads a stored id here — **never** an inlined copy of the
    /// referenced structure, which is what keeps lowering terminating.
    type_resolved: Vec<Option<TypeId>>,
    decl_types: DeclTypes,
    obligations: Vec<AssignObligation>,
    diagnostics: Vec<Diagnostic>,
    /// **Narrowing environment** (M7, architecture §5): the current control-flow
    /// narrowing overlay, mapping a **`SymbolId`** to its narrowed `TypeId`. It is
    /// consulted *first* when resolving an identifier's type (see
    /// [`resolve_identifier_type`]); an absent entry falls back to the declared/
    /// inferred type. This is the structured-flow slice of the §5 flow interpreter:
    /// the entries are installed by the `if`/`else` fork-and-restore around branch
    /// walks ([`check_if`]) and never escape an `if` (the pre-`if` map is restored
    /// after both branches). Keying on `SymbolId` — the specific binding — is what
    /// guarantees narrowing `x` never affects another symbol `y`, a property access,
    /// or a shadowed binding in a different scope.
    narrowed: FxHashMap<SymbolId, TypeId>,
}

/// Check a parsed program and return the diagnostics it produces.
pub fn check_program<'ast>(
    interner: &mut Interner,
    program: &'ast Program<'ast>,
) -> Vec<Diagnostic> {
    let binder = bind_module(program);
    let decl_types = DeclTypes::new(binder.decl_count);

    // Reserve a nominal id for every interface and record every type-declaration
    // body, indexed by its type-space `DeclId`, BEFORE any body is lowered (the
    // reserve half of reserve-then-fill — mvp-plan M5, §6.3). Aliases are resolved
    // lazily; interface ids are available immediately.
    let (type_decls, type_resolved) = reserve_type_decls(interner, &binder, program);

    let mut pass = Pass {
        interner,
        binder: &binder,
        type_decls,
        type_resolved,
        decl_types,
        obligations: Vec::new(),
        diagnostics: Vec::new(),
        narrowed: FxHashMap::default(),
    };

    // --- Phase 0 (fill): resolve every alias and fill every interface body. From
    // here on `type_resolved` is complete, so a `TSTypeReference` in the walk is a
    // plain id lookup. ---
    fill_type_decls(&mut pass, binder.module);

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

// ===========================================================================
// M5: named types — reserve-then-fill (mvp-plan M5, §3, §6.3).
// ===========================================================================

/// Phase 0a — **reserve**. Walk the top-level type declarations and, indexed by
/// the binder's type-space `DeclId`, record each one's lowering plan. Every
/// `interface` gets a fresh nominal object id reserved up front (empty body); each
/// `type` alias records its annotation for lazy resolution.
///
/// Reserving the interface ids *before* any body is resolved is what lets a body
/// reference itself or a sibling: `interface List { tail: List | null }` and the
/// mutual `Ping`/`Pong` lower because `List`/`Pong` already have ids by the time
/// their members are lowered. Returns the per-`DeclId` decl table and a parallel
/// `resolved` table (interfaces pre-seeded with their reserved id; aliases `None`).
fn reserve_type_decls<'ast>(
    interner: &mut Interner,
    binder: &Binder,
    program: &'ast Program<'ast>,
) -> (Vec<TypeDecl<'ast>>, Vec<Option<TypeId>>) {
    let count = binder.type_decl_count as usize;
    // Placeholders so the tables are indexable by every type `DeclId`; a
    // declaration the binder counted but we don't recognise stays an unresolved
    // alias of a never-resolving annotation (defensive — not expected).
    let mut decls: Vec<TypeDecl<'ast>> = Vec::with_capacity(count);
    let mut resolved: Vec<Option<TypeId>> = vec![None; count];

    // Build by walking declarations in source order; the binder assigned type
    // `DeclId`s in that same order (`bind_type_declarations`), so pushing in order
    // keeps the decl table index-aligned with the `DeclId`s.
    for stmt in &program.body {
        match stmt {
            Statement::TSInterfaceDeclaration(iface) => {
                let reserved = interner.reserve_object();
                if let Some(decl_id) = type_decl_id(binder, binder.module, iface.id.name.as_str()) {
                    if let Some(slot) = resolved.get_mut(decl_id.index()) {
                        *slot = Some(reserved);
                    }
                }
                decls.push(TypeDecl::Interface {
                    reserved,
                    members: &iface.body.body,
                });
            }
            Statement::TSTypeAliasDeclaration(alias) => {
                decls.push(TypeDecl::Alias {
                    annotation: &alias.type_annotation,
                    resolving: false,
                });
            }
            _ => {}
        }
    }

    (decls, resolved)
}

/// Phase 0b — **fill**. Force every alias to resolve and fill every interface body
/// in place. After this returns, `pass.type_resolved` is complete, so every
/// `TSTypeReference` encountered in the obligation walk is a plain id lookup.
fn fill_type_decls(pass: &mut Pass, scope: ScopeId) {
    let count = pass.type_decls.len();

    // Resolve aliases first so an interface body referencing an alias sees a
    // resolved id (interface ids are already reserved). Resolution is on-demand
    // and idempotent, so touching every alias resolves the whole alias DAG.
    for index in 0..count {
        if matches!(pass.type_decls[index], TypeDecl::Alias { .. }) {
            resolve_type_decl(pass, scope, DeclId(index as u32));
        }
    }

    // Fill each interface's reserved id with its lowered members. Members are
    // lowered with the full resolver available; a self/sibling reference resolves
    // to a reserved/resolved id (stored, never inlined).
    for index in 0..count {
        let TypeDecl::Interface { reserved, members } = pass.type_decls[index] else {
            continue;
        };
        let object = lower_interface_members(pass, scope, members);
        pass.interner.fill_object(reserved, object);
    }
}

/// Resolve a single type declaration to its `TypeId`, memoizing the result in
/// `pass.type_resolved`. Interfaces are already seeded (their reserved id);
/// aliases are lowered on first request. A recursive alias (an alias whose body,
/// directly or transitively, references itself) is detected by the `resolving`
/// flag and broken by yielding the error type — recursive *aliases* are out of the
/// M5 subset (recursion comes via interfaces), so this never loops.
fn resolve_type_decl(pass: &mut Pass, scope: ScopeId, decl_id: DeclId) -> TypeId {
    let error_ty = pass.interner.well_known().error;

    // Already resolved (interface reserved id, or a previously-resolved alias).
    if let Some(existing) = pass.type_resolved.get(decl_id.index()).copied().flatten() {
        return existing;
    }

    let index = decl_id.index();
    let annotation = match pass.type_decls.get(index) {
        Some(TypeDecl::Alias { annotation, resolving: false }) => *annotation,
        // A reference re-entered while this alias is mid-resolution: a recursive
        // alias (out of subset). Break the cycle with the error type.
        Some(TypeDecl::Alias { resolving: true, .. }) => return error_ty,
        // An interface with no seeded id, or an out-of-range id: defensive.
        _ => return error_ty,
    };

    // Mark in-progress so a transitive self-reference is caught above.
    if let Some(TypeDecl::Alias { resolving, .. }) = pass.type_decls.get_mut(index) {
        *resolving = true;
    }

    let target = lower_annotation(pass, scope, annotation).unwrap_or(error_ty);

    if let Some(TypeDecl::Alias { resolving, .. }) = pass.type_decls.get_mut(index) {
        *resolving = false;
    }
    if let Some(slot) = pass.type_resolved.get_mut(index) {
        *slot = Some(target);
    }
    target
}

/// Resolve a `TSTypeReference`'s name through the binder's type slot to a
/// `TypeId`. A name that resolves to a type declaration yields its (reserved or
/// lazily-resolved) id; an unresolved type name is out of the M5 subset and yields
/// `None` (the caller aborts the enclosing lowering, matching object/union/
/// function lowering). Qualified names (`A.B`) and type arguments (`List<T>`) are
/// out of the M5 subset → `None`.
fn resolve_type_reference(
    pass: &mut Pass,
    scope: ScopeId,
    type_name: &TSTypeName<'_>,
    has_type_arguments: bool,
) -> Option<TypeId> {
    if has_type_arguments {
        return None;
    }
    let TSTypeName::IdentifierReference(ident) = type_name else {
        return None;
    };
    let decl_id = type_decl_id(pass.binder, scope, ident.name.as_str())?;
    Some(resolve_type_decl(pass, scope, decl_id))
}

/// The type-space `DeclId` a name resolves to from `scope` (binder type slot), if
/// any. Walks the scope graph like value resolution, then reads the `ty` slot.
fn type_decl_id(binder: &Binder, scope: ScopeId, name: &str) -> Option<DeclId> {
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.ty)
}

/// Lower an interface body's members to a nominal `ObjectType` (M5). Mirrors
/// [`lower_object_annotation`] but returns the `ObjectType` (the caller fills the
/// reserved nominal id rather than interning a fresh structural object). A member
/// that is not a plain required property signature, or whose type cannot be
/// lowered, is skipped — the interface keeps the members it can express (a partial
/// interface is more useful than none, and the unsupported members are out of the
/// M5 subset).
fn lower_interface_members(
    pass: &mut Pass,
    scope: ScopeId,
    members: &[TSSignature<'_>],
) -> ObjectType {
    let mut properties: Vec<PropertyType> = Vec::with_capacity(members.len());
    for member in members {
        let TSSignature::TSPropertySignature(sig) = member else {
            continue;
        };
        if sig.optional {
            continue;
        }
        let Some(name) = sig.key.static_name() else {
            continue;
        };
        let Some(annotation) = sig.type_annotation.as_ref() else {
            continue;
        };
        let Some(ty) = lower_annotation(pass, scope, &annotation.type_annotation) else {
            continue;
        };
        properties.push(PropertyType {
            name: name.into_owned(),
            ty,
            optional: false,
        });
    }
    ObjectType { properties }
}

/// Map a relation failure to a diagnostic according to the obligation's kind.
///
/// `Assignment`: a required-target-property absence → `TK2741`; everything else
/// (primitive mismatch, a present-but-wrong property, or any function-shaped
/// mismatch — possibly nested) → `TK2322`. The error type never reaches here (it
/// is `any`-like, so its obligations resolve to `Yes`). `Argument`: any failure →
/// `TK2345`.
///
/// M6 (§6.4): the **headline** keeps its flat top-level form, and the nested
/// reason chain is rendered below it as the diagnostic's elaboration via
/// [`render_reason_chain`]. A single-`Leaf`/missing-property/arity head produces
/// an **empty** elaboration (the headline already states it in full), so scalar
/// mismatches render exactly one line — no earlier-milestone regression.
fn emit_obligation_failure(
    store: &Store,
    ob: &AssignObligation,
    head: &Reason,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // The nested "because…" cascade shown below the headline. Empty for a head the
    // headline already expresses in full (e.g. a scalar `Leaf`).
    let elaboration = render_reason_chain(store, head);

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
                diagnostics
                    .push(Diagnostic::not_assignable(ob.src_span, message).with_elaboration(elaboration));
            }
        },
        ObligationKind::Argument => {
            let src = render_type(store, headline_src(ob, head), /* widen */ true);
            let tgt = render_type(store, ob.tgt, /* widen */ false);
            diagnostics.push(
                Diagnostic::argument_not_assignable(ob.src_span, &src, &tgt)
                    .with_elaboration(elaboration),
            );
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

/// Check a list of statements in `scope` at the **module top level** (no enclosing
/// function, so no return context). Each statement flows through the unified
/// statement walker with an empty return context.
fn check_statements(pass: &mut Pass, scope: ScopeId, statements: &[Statement<'_>]) {
    let mut no_return: Option<TypeId> = None;
    for stmt in statements {
        check_stmt(pass, scope, stmt, None, &mut no_return);
    }
}

/// Check one statement in `scope` — the **unified, flow-sensitive** statement
/// walker (M7). It handles every statement kind in the subset, threading an
/// optional return context (`declared_ret` + the accumulating inferred return
/// `inferred`) so the same code serves both the module top level (empty context)
/// and a function body. It is the structured-flow driver of the §5 interpreter:
/// `if`/`else` is the only construct that touches the narrowing environment, via
/// [`check_if`]'s fork-and-restore.
///
/// `inferred` accumulates the first value-return's widened type when no return is
/// declared (used only by [`check_function_body`]); at module level it is a
/// throwaway that no `return` ever writes (a top-level `return` is illegal TS).
fn check_stmt(
    pass: &mut Pass,
    scope: ScopeId,
    stmt: &Statement<'_>,
    declared_ret: Option<TypeId>,
    inferred: &mut Option<TypeId>,
) {
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
        Statement::ReturnStatement(ret) => {
            check_return(pass, scope, ret, declared_ret, inferred);
        }
        // M7: control-flow narrowing happens here (the fork-and-restore).
        Statement::IfStatement(if_stmt) => {
            check_if(pass, scope, if_stmt, declared_ret, inferred);
        }
        // A `{ … }` block runs its statements in its own (binder-created) block
        // scope, inheriting the current narrowing environment.
        Statement::BlockStatement(block) => {
            check_block(pass, scope, block, declared_ret, inferred);
        }
        // Other statements are out of the M7 subset.
        _ => {}
    }
}

/// Check a `return <expr>?` statement against the enclosing function's return
/// context (extracted from the M3 body walk; behaviour unchanged). With a declared
/// return type, the returned expression is an assignability obligation
/// (primary span = the expression); without one, the first value-return's widened
/// type is recorded as the inferred return. A bare `return;` is handled by the
/// `void` rule in phase 2 and contributes no inferred type.
fn check_return(
    pass: &mut Pass,
    scope: ScopeId,
    ret: &oxc_ast::ast::ReturnStatement<'_>,
    declared_ret: Option<TypeId>,
    inferred: &mut Option<TypeId>,
) {
    let Some(arg) = &ret.argument else {
        return;
    };
    let Some((src, src_span)) = infer_expr(pass, scope, arg) else {
        return;
    };
    match declared_ret {
        // Declared return type: check the returned expression against it.
        Some(tgt) => {
            pass.obligations.push(AssignObligation {
                src,
                tgt,
                src_span,
                kind: ObligationKind::Assignment,
            });
        }
        // No annotation: infer from the first value return, widened.
        None => {
            if inferred.is_none() {
                *inferred = Some(widen(pass.interner, src));
            }
        }
    }
}

/// Check a `{ … }` block (M7): descend into its own lexical block scope (created by
/// the binder, keyed by span start) and run its statements there with the current
/// return context and narrowing environment. A block that the binder did not record
/// (defensive — never expected) falls back to the enclosing scope.
fn check_block(
    pass: &mut Pass,
    scope: ScopeId,
    block: &BlockStatement<'_>,
    declared_ret: Option<TypeId>,
    inferred: &mut Option<TypeId>,
) {
    let block_scope = pass
        .binder
        .block_scopes
        .get(&block.span.start)
        .copied()
        .unwrap_or(scope);
    for stmt in &block.body {
        check_stmt(pass, block_scope, stmt, declared_ret, inferred);
    }
}

// ===========================================================================
// M7: control-flow narrowing — guard analysis + if/else fork-and-restore.
// ===========================================================================

/// A recognized guard fact: the **specific symbol** being narrowed plus the
/// narrowing operation (and the polarity already folded so the then-branch applies
/// it as written). Pairing a [`NarrowOp`] with a `SymbolId` here — in the checker,
/// not in the flow operations — is the symbol-keying that keeps a narrowing of `x`
/// from ever touching another symbol.
struct GuardFact {
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
fn check_if(
    pass: &mut Pass,
    scope: ScopeId,
    if_stmt: &IfStatement<'_>,
    declared_ret: Option<TypeId>,
    inferred: &mut Option<TypeId>,
) {
    // Evaluate the condition under the current env (before forking) so references
    // resolve and nested functions/calls inside it are checked exactly once.
    infer_expr(pass, scope, &if_stmt.test);
    let fact = analyze_guard(pass, scope, &if_stmt.test);

    // Snapshot the pre-`if` environment so each branch starts from it and it is
    // restored afterwards. The map holds only currently-narrowed symbols, so the
    // clone is small; cloning makes the restore unconditionally correct.
    let saved = pass.narrowed.clone();

    // --- then-branch: apply the positive fact, walk, restore. ---
    if let Some(fact) = &fact {
        apply_guard(pass, fact, fact.then_positive);
    }
    check_stmt(pass, scope, &if_stmt.consequent, declared_ret, inferred);
    pass.narrowed = saved.clone();

    // --- else-branch: apply the negative (complement) fact, walk, restore. ---
    if let Some(alternate) = &if_stmt.alternate {
        if let Some(fact) = &fact {
            apply_guard(pass, fact, !fact.then_positive);
        }
        check_stmt(pass, scope, alternate, declared_ret, inferred);
    }
    // Restore the pre-`if` env unconditionally: narrowing must not escape the `if`.
    pass.narrowed = saved;
}

/// Apply a guard fact to the narrowing environment for one branch: narrow the
/// guarded symbol's **current** type (its already-narrowed type if an enclosing
/// `if` narrowed it, else its declared type — so nested `if`s compose) by the
/// fact's operation under `positive`, and install the result.
///
/// `positive` is the branch polarity (the then-branch passes `then_positive`, the
/// else-branch its negation). A symbol with no resolvable current type (out of
/// subset) is left untouched.
fn apply_guard(pass: &mut Pass, fact: &GuardFact, positive: bool) {
    let current = resolve_identifier_type(pass, fact.symbol);
    let narrowed = narrow(pass.interner, current, fact.op, positive);
    pass.narrowed.insert(fact.symbol, narrowed);
}

/// Analyze a condition expression into a `GuardFact`, or `None` if it is not a
/// recognized M7 guard (in which case nothing is narrowed — soundness: an
/// unknown guard must never narrow). Recognizes, over a plain identifier operand:
///
///  - **typeof**: `typeof x === "string" | "number" | "boolean"` and `!==`/`==`/
///    `!=`, with the `typeof …` on either side of the comparison;
///  - **truthiness**: bare `x`, and `!x` (which flips the polarity);
///  - **null/undefined equality**: `x === null` / `x === undefined` and `!==`,
///    with the literal on either side.
///
/// A leading `!` flips `then_positive`. Anything else (an unrecognized tag, a
/// non-identifier operand, a `&&`/`||`, a member-access operand, …) returns `None`.
fn analyze_guard(pass: &Pass, scope: ScopeId, test: &Expression<'_>) -> Option<GuardFact> {
    match test {
        // `!cond` — recurse and flip the then-branch polarity.
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
            let mut inner = analyze_guard(pass, scope, &unary.argument)?;
            inner.then_positive = !inner.then_positive;
            Some(inner)
        }
        // A parenthesized condition is transparent.
        Expression::ParenthesizedExpression(paren) => analyze_guard(pass, scope, &paren.expression),
        // Bare truthiness `if (x)`.
        Expression::Identifier(_) => {
            let symbol = condition_symbol(pass, scope, test)?;
            Some(GuardFact {
                symbol,
                op: NarrowOp::Truthy,
                then_positive: true,
            })
        }
        // An equality comparison: `typeof x === "tag"`, `x === null`, etc.
        Expression::BinaryExpression(binary) => analyze_equality_guard(pass, scope, binary),
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
    pass: &Pass,
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
    if let Some(fact) = typeof_guard(pass, scope, left, right, eq_positive)
        .or_else(|| typeof_guard(pass, scope, right, left, eq_positive))
    {
        return Some(fact);
    }

    // null/undefined form: `x === null` (either operand order). Strict only — loose
    // `== null` also matches `undefined`, a different (deferred) rule, so a loose
    // null/undefined comparison is treated as an unrecognized guard (narrows
    // nothing — sound).
    if strict {
        if let Some(fact) = nullish_guard(pass, scope, left, right, eq_positive)
            .or_else(|| nullish_guard(pass, scope, right, left, eq_positive))
        {
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
    pass: &Pass,
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
    let symbol = condition_symbol(pass, scope, &unary.argument)?;
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
    pass: &Pass,
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
    let symbol = condition_symbol(pass, scope, ident_side)?;
    Some(GuardFact {
        symbol,
        op: NarrowOp::EqNullish { is_undefined },
        then_positive: eq_positive,
    })
}

/// Resolve a condition operand to the value `SymbolId` it narrows, or `None` if it
/// is not a narrowable plain identifier in scope. A non-identifier operand (member
/// access, call, literal, the `undefined` keyword, …) is not narrowable — returning
/// `None` keeps narrowing keyed strictly to a real local/parameter binding.
fn condition_symbol(pass: &Pass, scope: ScopeId, expr: &Expression<'_>) -> Option<SymbolId> {
    let Expression::Identifier(ident) = expr else {
        return None;
    };
    // The `undefined` keyword is an identifier reference but not a narrowable
    // binding; exclude it so `x === undefined` does not treat `undefined` as the
    // narrowed symbol.
    if ident.name.as_str() == "undefined" {
        return None;
    }
    let symbol_id = pass.binder.graph.resolve(scope, ident.name.as_str())?;
    // Only a value binding (a local/parameter) is narrowable.
    pass.binder.symbols.get(symbol_id)?.value?;
    Some(symbol_id)
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

    let annotation = match declarator.type_annotation.as_ref() {
        Some(ann) => lower_annotation(pass, scope, &ann.type_annotation),
        None => None,
    };

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
fn check_assignment(pass: &mut Pass, scope: ScopeId, assign: &AssignmentExpression<'_>) {
    let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left else {
        // A non-identifier target (`obj.x = …`, destructuring) is out of subset:
        // no symbol to reset, and no obligation collected.
        return;
    };

    // Infer the RHS first so any reference inside it resolves (and emits TK2304
    // before we look at the target), and any nested function body is checked. The
    // RHS is evaluated *before* the assignment, so it still sees the target's
    // pre-assignment narrowing (e.g. `x = x` reads the narrowed `x`). Done for both
    // simple and compound forms so the RHS of a compound assignment is still walked.
    let rhs = infer_expr(pass, scope, &assign.right);

    let symbol_id = match pass.binder.graph.resolve(scope, target.name.as_str()) {
        Some(symbol_id) => symbol_id,
        None => {
            pass.diagnostics.push(Diagnostic::cannot_find_name(
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
    pass.narrowed.remove(&symbol_id);

    // Compound assignment (`+=`, `||=`, …): assignability is unchecked baseline-wide
    // (out of the M7 subset). The narrowing reset above already ran, so it is sound
    // to stop here without collecting an obligation.
    if assign.operator != AssignmentOperator::Assign {
        return;
    }

    // The target type is always the symbol's *declared* type (you may assign
    // anything assignable to the declaration, regardless of the current narrowing).
    let target_ty = pass
        .binder
        .symbols
        .get(symbol_id)
        .and_then(|s| s.value)
        .and_then(|decl_id| pass.decl_types.get(decl_id));

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

/// Lower an annotation type to its `TypeId`. Supports intrinsic keywords, object
/// type literals, function types (`(x: number) => string`), unions, and (M5) type
/// **references** (`Point`, `Num`, `List`) resolved through the binder's type slot.
///
/// Reference resolution returns a **stored id** for the referenced declaration
/// (the interface's reserved nominal id, or the alias's resolved target) — never
/// an inlined copy of the referenced structure. That is what makes lowering a
/// recursive type terminate: `tail: List | null` stores the union of `List`'s id
/// and `null`, not an expansion of `List` (mvp-plan M5, §3, §6.3).
fn lower_annotation(pass: &mut Pass, scope: ScopeId, ts_type: &TSType<'_>) -> Option<TypeId> {
    let wk = pass.interner.well_known();
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
        TSType::TSTypeLiteral(lit) => return lower_object_annotation(pass, scope, &lit.members),
        TSType::TSFunctionType(func) => {
            return lower_function_annotation(
                pass,
                scope,
                &func.params,
                &func.return_type.type_annotation,
            );
        }
        TSType::TSUnionType(union) => return lower_union_annotation(pass, scope, &union.types),
        TSType::TSParenthesizedType(paren) => {
            return lower_annotation(pass, scope, &paren.type_annotation);
        }
        // M5: a type reference (`Point`, `Num`, `List`) resolves through the
        // binder's type slot. Type arguments / qualified names are out of subset.
        TSType::TSTypeReference(reference) => {
            return resolve_type_reference(
                pass,
                scope,
                &reference.type_name,
                reference.type_arguments.is_some(),
            );
        }
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
fn lower_union_annotation(
    pass: &mut Pass,
    scope: ScopeId,
    members: &[TSType<'_>],
) -> Option<TypeId> {
    let mut lowered: Vec<TypeId> = Vec::with_capacity(members.len());
    for member in members {
        lowered.push(lower_annotation(pass, scope, member)?);
    }
    Some(pass.interner.union(lowered))
}

/// Lower an object type literal's members to an interned (structural) object
/// `TypeId`. Object **type literals** stay structurally hash-consed (only nominal
/// interfaces bypass interning). A member whose type cannot be lowered (or an
/// optional/index/call/method/construct signature) aborts the lowering (`None`).
fn lower_object_annotation(
    pass: &mut Pass,
    scope: ScopeId,
    members: &[TSSignature<'_>],
) -> Option<TypeId> {
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
        let ty = lower_annotation(pass, scope, &annotation.type_annotation)?;
        properties.push(PropertyType {
            name: name.into_owned(),
            ty,
            optional: false,
        });
    }
    Some(pass.interner.intern_object(ObjectType { properties }))
}

/// Lower a function type annotation's parameters and return type to an interned
/// function `TypeId`. Parameters are kept **positional** (never sorted). A
/// parameter without a type annotation, or one whose type cannot be lowered, or
/// any optional/rest parameter, aborts the lowering (`None`) — these are out of
/// the M3 subset and dropping them silently would mis-state the type.
fn lower_function_annotation(
    pass: &mut Pass,
    scope: ScopeId,
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
        let ty = lower_annotation(pass, scope, &annotation.type_annotation)?;
        lowered.push(ParameterType {
            name,
            ty,
            optional: false,
        });
    }
    let ret = lower_annotation(pass, scope, return_type)?;
    Some(pass.interner.intern_function(FunctionType {
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
                Some(symbol_id) => Some((resolve_identifier_type(pass, symbol_id), span)),
                None => {
                    pass.diagnostics
                        .push(Diagnostic::cannot_find_name(span, ident.name.as_str()));
                    Some((well_known.error, span))
                }
            }
        }
        // M7: condition shapes (`typeof x`, `!x`, `x === null`, …). They are walked
        // for their operands' side effects (resolving references / descending into
        // nested constructs); their *value* type is only ever a condition, never an
        // assignment source in the subset, so a coarse result type is sufficient.
        Expression::UnaryExpression(unary) => Some(infer_unary(pass, scope, unary)),
        Expression::BinaryExpression(binary) => Some(infer_binary(pass, scope, binary)),
        Expression::LogicalExpression(logical) => Some(infer_logical(pass, scope, logical)),
        // TODO(M4+): array literals, etc.
        _ => None,
    }
}

/// Resolve a value symbol's *current* type, consulting the narrowing environment
/// first (M7). A `SymbolId` present in [`Pass::narrowed`] uses its narrowed type;
/// otherwise the declared/inferred type from `decl_types` is used; a resolved
/// symbol with no computed type yet (out of subset) is the error type (no cascade).
///
/// This single seam is where control-flow narrowing takes effect: every identifier
/// reference — assignment sources, member-access bases, returned expressions, call
/// arguments — resolves through [`infer_expr`], which calls this. Keying on the
/// `SymbolId` (not the name, not the `DeclId` of an unrelated binding) is the
/// soundness guarantee that narrowing applies to exactly the guarded binding.
fn resolve_identifier_type(pass: &Pass, symbol_id: SymbolId) -> TypeId {
    if let Some(&narrowed) = pass.narrowed.get(&symbol_id) {
        return narrowed;
    }
    pass.binder
        .symbols
        .get(symbol_id)
        .and_then(|s| s.value)
        .and_then(|decl_id| pass.decl_types.get(decl_id))
        .unwrap_or(pass.interner.well_known().error)
}

/// Infer a unary expression (M7 condition support). Descends into the operand for
/// its side effects, then returns a coarse result type by operator:
///
///  - `typeof x` → `string` (the runtime tag string),
///  - `!x` → `boolean`,
///  - everything else (`+`/`-`/`~`/`void`/`delete`) is out of the subset → the
///    error type (no diagnostic; never an assignment source in the corpus).
fn infer_unary(pass: &mut Pass, scope: ScopeId, unary: &UnaryExpression<'_>) -> (TypeId, Span) {
    let wk = pass.interner.well_known();
    let span = Span::from_oxc(unary.span);
    // Walk the operand so references inside the condition resolve (and nested
    // functions are checked).
    infer_expr(pass, scope, &unary.argument);
    let ty = match unary.operator {
        UnaryOperator::Typeof => wk.string,
        UnaryOperator::LogicalNot => wk.boolean,
        _ => wk.error,
    };
    (ty, span)
}

/// Infer a binary expression (M7 condition support). Descends into both operands
/// for their side effects, then returns `boolean` for a comparison/equality
/// operator (`===`, `!==`, `<`, …) and the error type for any other operator
/// (arithmetic/bitwise are out of the subset; never an assignment source here).
fn infer_binary(pass: &mut Pass, scope: ScopeId, binary: &BinaryExpression<'_>) -> (TypeId, Span) {
    let wk = pass.interner.well_known();
    let span = Span::from_oxc(binary.span);
    infer_expr(pass, scope, &binary.left);
    infer_expr(pass, scope, &binary.right);
    let ty = if is_comparison_operator(binary.operator) {
        wk.boolean
    } else {
        wk.error
    };
    (ty, span)
}

/// Infer a logical expression (`&&`/`||`/`??`, M7 condition support). Both operands
/// are walked for side effects; the result type is the error type — `&&`/`||`
/// condition narrowing is deferred (mvp-plan, README "Deferred checks"), so a
/// logical expression is treated as an unrecognized guard (it narrows nothing).
fn infer_logical(pass: &mut Pass, scope: ScopeId, logical: &LogicalExpression<'_>) -> (TypeId, Span) {
    let wk = pass.interner.well_known();
    let span = Span::from_oxc(logical.span);
    infer_expr(pass, scope, &logical.left);
    infer_expr(pass, scope, &logical.right);
    (wk.error, span)
}

/// Whether a binary operator is a comparison/equality operator (its result is
/// `boolean`). Equality operators (`==`/`!=`/`===`/`!==`) and the relational
/// operators all qualify; arithmetic/bitwise/`in`/`instanceof` do not (the latter
/// two are out of the M7 subset).
fn is_comparison_operator(op: BinaryOperator) -> bool {
    matches!(
        op,
        BinaryOperator::Equality
            | BinaryOperator::Inequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
            | BinaryOperator::LessThan
            | BinaryOperator::LessEqualThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterEqualThan
    )
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
    let params = lower_parameters(pass, enclosing, fn_scope, &func.params);

    // Declared return type from the annotation, if any. Type references in the
    // signature resolve from the enclosing scope (where the type names live).
    let declared_ret = match func.return_type.as_ref() {
        Some(ann) => lower_annotation(pass, enclosing, &ann.type_annotation),
        None => None,
    };

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
    let params = lower_parameters(pass, enclosing, fn_scope, &arrow.params);

    let declared_ret = match arrow.return_type.as_ref() {
        Some(ann) => lower_annotation(pass, enclosing, &ann.type_annotation),
        None => None,
    };

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
    enclosing: ScopeId,
    fn_scope: Option<ScopeId>,
    params: &FormalParameters<'_>,
) -> Vec<ParameterType> {
    let error_ty = pass.interner.well_known().error;
    let mut lowered: Vec<ParameterType> = Vec::with_capacity(params.items.len());
    for param in &params.items {
        let name = parameter_name(&param.pattern).unwrap_or_default();
        // Annotated type, or the error type for an un-annotated parameter. Type
        // references in the annotation resolve from the enclosing scope.
        let ty = match param.type_annotation.as_ref() {
            Some(ann) => lower_annotation(pass, enclosing, &ann.type_annotation).unwrap_or(error_ty),
            None => error_ty,
        };

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

    // A function boundary resets narrowing: this body's parameters/locals are
    // distinct symbols, and a closure may run after any enclosing narrowing no
    // longer holds, so the body must not inherit the caller's narrowing
    // environment. Save and restore it around the walk (the enclosing walk — e.g.
    // a `const f = () => …` initializer mid-`if` — keeps its own narrowing intact).
    let saved = std::mem::take(&mut pass.narrowed);

    for stmt in &body.statements {
        check_stmt(pass, scope, stmt, declared_ret, &mut inferred);
    }

    pass.narrowed = saved;
    inferred.unwrap_or(void_ty)
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

#[cfg(test)]
mod narrowing_tests {
    //! M7 end-to-end soundness tests for control-flow narrowing. These drive the
    //! whole pipeline (parse → bind → check) and assert the *set* of `(line, code)`
    //! diagnostics, so they pin the four soundness properties the review hammers:
    //! narrowing does not escape its branch, never affects another symbol, resets
    //! on reassignment, and never fires for an unrecognized guard. The
    //! per-operation narrowing math is unit-tested in `flow.rs`; these guard the
    //! env/driver (the structured-flow slice).

    use crate::driver::check_source;

    /// Run the checker and return the sorted `(1-based line, code)` of every
    /// diagnostic, keyed on its primary-span start line (matching the conformance
    /// harness's line mapping).
    fn diags(source: &str) -> Vec<(u32, String)> {
        let out = check_source(source);
        assert!(
            out.parse_errors.is_empty(),
            "unexpected parse error(s): {:?}",
            out.parse_errors
        );
        let index = crate::span::LineIndex::new(source);
        let mut v: Vec<(u32, String)> = out
            .diagnostics
            .iter()
            .map(|d| (index.line_of(d.span.start), d.code.as_str().to_string()))
            .collect();
        v.sort();
        v
    }

    /// The narrowed branch is genuinely clean while the wide references error —
    /// narrowing both *enables* the in-branch assignment and *does not escape* the
    /// `if` (the trailing reference re-errors). This is the headline behaviour the
    /// `typeof.ts` fixture observes.
    #[test]
    fn narrowing_clears_in_branch_and_does_not_escape() {
        let src = "\
function f(x: string | number) {
  const wide: string = x;
  if (typeof x === \"string\") {
    const s: string = x;
  } else {
    const n: number = x;
  }
  const after: string = x;
}
";
        // Only the two wide references (lines 2 and 8) error; the narrowed
        // then/else assignments (lines 4, 6) are clean.
        assert_eq!(
            diags(src),
            vec![(2, "TK2322".to_string()), (8, "TK2322".to_string())]
        );
    }

    /// Narrowing `x` must never affect a *different* symbol `y` (symbol-keying).
    #[test]
    fn narrowing_does_not_affect_other_symbol() {
        let src = "\
function f(x: string | number, y: string | number) {
  if (typeof x === \"string\") {
    const sx: string = x;
    const sy: string = y;
  }
}
";
        // `x` is narrowed (line 3 clean); `y` is untouched, so line 4 errors.
        assert_eq!(diags(src), vec![(4, "TK2322".to_string())]);
    }

    /// Reassigning a narrowed symbol resets it — a later reference sees the wide
    /// (declared) type again, never the stale narrowing.
    #[test]
    fn reassignment_resets_narrowing() {
        let src = "\
function f(x: string | number) {
  if (typeof x === \"string\") {
    const s1: string = x;
    x = 1;
    const s2: string = x;
  }
}
";
        // Before the reassignment `x` is `string` (line 3 clean); after `x = 1` it
        // is reset to `string | number`, so line 5 errors.
        assert_eq!(diags(src), vec![(5, "TK2322".to_string())]);
    }

    /// An unrecognized guard narrows nothing (no false negatives): both branches
    /// see the wide type.
    #[test]
    fn unknown_guard_does_not_narrow() {
        let src = "\
function f(x: string | number, c: boolean) {
  if (c) {
    const bad: string = x;
  } else {
    const bad2: string = x;
  }
}
";
        assert_eq!(
            diags(src),
            vec![(3, "TK2322".to_string()), (5, "TK2322".to_string())]
        );
    }

    /// Nested `if`s compose, and the complement of a typeof guard over a >2-member
    /// union keeps the remaining members.
    #[test]
    fn nested_ifs_compose_over_three_member_union() {
        let src = "\
function f(x: string | number | boolean) {
  if (typeof x !== \"string\") {
    if (typeof x === \"number\") {
      const n: number = x;
    } else {
      const b: boolean = x;
    }
  }
}
";
        // Every in-branch assignment is satisfied by composed narrowing: no errors.
        assert!(diags(src).is_empty(), "got {:?}", diags(src));
    }

    /// `!x` truthiness flips the branches; the else of `!z` is the truthy (object)
    /// one, and the falsy then-branch keeps the nullish member.
    #[test]
    fn negated_truthiness_flips_branches() {
        let src = "\
function f(z: { a: number } | null) {
  if (!z) {
    const bad: { a: number } = z;
  } else {
    const ok: { a: number } = z;
  }
}
";
        // then-branch of `!z` has `z: null` → assignment errors (line 3); else is
        // the object → clean (line 5).
        assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
    }

    /// Narrowing enables otherwise-rejected member access (`TK2339`) and does not
    /// escape: the pre-`if` access still errors.
    #[test]
    fn narrowing_enables_member_access() {
        let src = "\
function f(x: { a: number } | null) {
  const bad = x.a;
  if (x !== null) {
    const ok: number = x.a;
  }
}
";
        // The wide `x.a` (line 2) is `TK2339`; after narrowing out null the access
        // (line 4) is clean.
        assert_eq!(diags(src), vec![(2, "TK2339".to_string())]);
    }

    /// Soundness hardening — a **compound** assignment (`+=`, …) to a narrowed
    /// variable resets its narrowing too, just like a simple `=`. After `x += "b"`
    /// inside a `typeof`-guarded branch, `x` must no longer be assumed `string`, so a
    /// `const s: string = x` errors (the variable is back to `string | number`).
    /// This guards the defense-in-depth reset hoisted above the compound-operator
    /// early-return in `check_assignment`: were it stale, this would silently pass.
    #[test]
    fn compound_assignment_resets_narrowing() {
        let src = "\
function f() {
  let x: string | number = \"a\";
  if (typeof x === \"string\") {
    const s1: string = x;
    x += \"b\";
    const s2: string = x;
  }
}
";
        // Before `x += "b"` the narrowing holds (line 4 clean); the compound
        // assignment resets it, so line 6 errors against `string | number`.
        assert_eq!(diags(src), vec![(6, "TK2322".to_string())]);
    }
}

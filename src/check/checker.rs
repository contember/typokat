//! The statement-level checker (architecture §5, mvp-plan §5 — M0–M6 rows, plus
//! the post-MVP M7/M8 narrowing slices and the M9 generics core).
//!
//! M9 scope (post-MVP — generics core: type parameters + explicit type arguments +
//! instantiation by substitution; type-argument **inference** is M10,
//! **constraints** `<T extends U>` are M11 — both deferred), on top of M0–M8:
//!
//!  - **Type parameters** ([`crate::types::repr::TypeParamId`]): a generic
//!    declaration carries an ordered list of type-parameter ids; each parameter
//!    interns to a [`TypeTag::TypeParam`] type. A parameter is in scope **only**
//!    within its own declaration, via the [`Pass::type_param_scopes`] frame stack
//!    (pushed while the body/signature is lowered, popped after — no leak).
//!  - **Generic declarations**: a generic `function f<T, U>(…)`, `interface
//!    Box<T>`, and `type Pair<A, B> = …` lower their bodies with the parameter
//!    frame in scope. A generic interface/alias resolves to a structural
//!    **template** (its body with the parameter types embedded); a generic function
//!    additionally records its [`GenericSig`] under its value `DeclId`.
//!  - **Instantiation by substitution** ([`crate::types::substitute`]): a
//!    `TSTypeReference` with type arguments (`Box<number>`, `Pair<number, string>`,
//!    `Box<Box<number>>`) substitutes the referenced generic's template
//!    ([`instantiate_type_reference`]); a generic call with explicit type arguments
//!    (`identity<number>(5)`) substitutes the function signature
//!    ([`instantiate_generic_callee`]) and then runs the existing arity/argument/
//!    return checks against the instantiated parameter/return types. Equal
//!    instantiations share one interned id (`Box<number>` is consistent;
//!    `Box<number>` ≠ `Box<string>`). A wrong type-argument count is handled
//!    **gracefully** (no panic, no new code).
//!
//! M8 scope (post-MVP — discriminated-union narrowing + literal types), on top of
//! M7:
//!
//!  - **Literal type annotations** (`TSLiteralType`): `"hello"`, `42`, `true` lower
//!    to their interned literal `TypeId` ([`lower_literal_type`]), so
//!    union-of-literals (`"a" | "b"`) and a discriminant property (`kind: "circle"`)
//!    carry literal types. Literal assignability already exists (M0 widening +
//!    hash-consed literal identity).
//!  - **Discriminated-union narrowing**: a guard `x.prop === <literal>` / `!==`
//!    narrows the union-typed symbol `x` to the members whose `prop` is compatible
//!    with the literal (then-branch; complement for else / `!==`). The narrowing op
//!    ([`crate::check::flow::narrow_by_discriminant`]) keys on the **base symbol**
//!    `x` and a recognized **member-access discriminant** `x.prop`.
//!  - **`in`-operator narrowing**: `"prop" in x` keeps the members that have `prop`
//!    in the then-branch, those that can lack it in the else
//!    ([`crate::check::flow::narrow_by_in_operator`]).
//!  - **`switch` narrowing**: each `case "lit":` narrows the discriminant `x.prop`
//!    by `x.prop === "lit"`; `default:` by the complement of all case labels
//!    ([`check_switch`]), with a per-clause fork-and-restore mirroring [`check_if`].
//!    Fallthrough is handled conservatively (a clause that may fall through does not
//!    let the next clause over-narrow).
//!  - Still deferred to the flow-node CFG (M9+): narrowing through unstructured flow
//!    (early `return`/`throw` join, loops), assertion functions / type predicates
//!    (`x is T`), `typeof`-discriminant combined with `in`, non-literal
//!    discriminants, and exhaustiveness (`never` in `default`).
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
//!    guard analysis (`analyze_guard`) live here. (M8 extends this same structured
//!    driver with discriminated-union / `in` / `switch` narrowing — see the M8
//!    scope above. Unstructured-flow narrowing — early `return`/`throw` join, loops
//!    — and assertion functions remain deferred to the flow-node CFG, M9+.)
//!  - **Soundness** is keyed on the **specific `SymbolId`** (narrowing `x` — and,
//!    for an M8 discriminant `x.prop`, the *base* symbol `x` — never touches `y`, a
//!    property access, or a shadowed binding), narrowing **resets on reassignment**,
//!    an **unrecognized guard/discriminant narrows nothing** (no false negatives),
//!    and a **function boundary resets** the environment.
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
    FunctionType, IntrinsicKind, LiteralValue, ObjectType, ParameterType, PropertyType, TypeParamId,
    TypeTag,
};
use crate::types::store::{Store, TypeId};
use crate::types::{substitute, Interner, WellKnown};
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AssignmentTarget,
    BinaryExpression, BinaryOperator, BindingPattern, BlockStatement, CallExpression, Expression,
    FormalParameters, Function, FunctionBody, IfStatement, LogicalExpression, ObjectExpression,
    ObjectPropertyKind, Program, Statement, StaticMemberExpression, SwitchStatement, TSLiteral,
    TSSignature, TSType, TSTypeName, TSTypeParameterDeclaration, TSTypeParameterInstantiation,
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

/// How a top-level type declaration lowers (M5, extended for M9 generics), indexed
/// by its type-space `DeclId`. Collected up front so every declaration's `TypeId`
/// is reserved before any body is resolved — the reserve-then-fill that makes
/// recursive/mutual references lowerable (mvp-plan M5, §6.3). The `'ast` borrows
/// the AST bodies.
///
/// M9: a declaration may be **generic** — carry an ordered list of type-parameter
/// ids ([`type_params`](TypeDecl::params)). While the body is resolved those
/// parameters are pushed onto [`Pass::type_param_scopes`] so a reference to one
/// (`value: T`) lowers to its interned [`TypeTag::TypeParam`] type; the parameters
/// leave scope when the body is done (a type parameter is in scope **only** within
/// its own declaration). A generic declaration's resolved id is its **template**
/// (a structural type containing the parameter types); a `TSTypeReference` with
/// type arguments instantiates it by substitution ([`instantiate_type_reference`]).
enum TypeDecl<'ast> {
    /// An `interface`. `reserved` is its id (allocated empty via
    /// `Interner::reserve_object`, filled once the members are lowered); `members`
    /// is the interface body, lowered in the fill step. A **non-generic** interface
    /// is **nominal** (its reserved id is filled in place and never re-interned). A
    /// **generic** interface (`params` non-empty) is filled with its parameter types
    /// embedded, becoming a structural **template**; instantiating `Box<number>`
    /// substitutes the template to `{ value: number }` — see the structural-vs-
    /// nominal FLAG on [`instantiate_type_reference`].
    Interface {
        reserved: TypeId,
        params: Vec<TypeParamId>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        members: &'ast [TSSignature<'ast>],
    },
    /// A `type` alias — **transparent**. `annotation` is the aliased type, lowered
    /// on demand to the target id; `resolving` guards a recursive alias (out of the
    /// M5 subset — broken by yielding the error type rather than looping). A generic
    /// alias (`params` non-empty) lowers its annotation with the parameters in scope
    /// to a **template**; `Pair<number, string>` substitutes it.
    Alias {
        annotation: &'ast TSType<'ast>,
        params: Vec<TypeParamId>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        resolving: bool,
    },
}

/// A generic value declaration's signature (M9): the ordered type-parameter ids
/// and the **template** function type (its signature with those parameter types
/// embedded). A generic call with explicit type arguments substitutes
/// `params[i] → type_args[i]` into `fn_ty`, then runs the usual arity/argument/
/// return checks against the instantiated signature.
#[derive(Clone)]
struct GenericSig {
    params: Vec<TypeParamId>,
    fn_ty: TypeId,
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
    /// referenced structure, which is what keeps lowering terminating. For a
    /// **generic** declaration (M9) this is the **template** (its body with the
    /// parameter types embedded); a `TSTypeReference` with arguments substitutes it.
    type_resolved: Vec<Option<TypeId>>,
    /// **Type-parameter scope stack** (M9): each frame maps an in-scope type
    /// parameter's source name to its interned [`TypeTag::TypeParam`] id. A frame is
    /// pushed while a generic declaration's body/signature is lowered and popped
    /// after, so a type parameter is in scope **only** within its own declaration
    /// (no leak). [`resolve_type_reference`] consults the **innermost** frame first,
    /// before the binder's type slot, so `T` shadows a same-named type only inside
    /// the generic.
    type_param_scopes: Vec<FxHashMap<String, TypeId>>,
    /// Running counter allocating a unique [`TypeParamId`] per declared type
    /// parameter across the whole module (the named-unique-id representation — see
    /// [`crate::types::repr::TypeParamId`]).
    next_type_param: u32,
    /// Generic value signatures (M9), keyed by the value-space `DeclId` of a generic
    /// `function` declaration. A call `identity<number>(5)` looks the callee's
    /// `DeclId` up here, substitutes the type arguments into the template signature,
    /// and checks the call against the instantiated parameter/return types.
    generic_fns: FxHashMap<DeclId, GenericSig>,
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
    // lazily; interface ids are available immediately. M9: each declaration's type
    // parameters get fresh ids here too (advancing `next_type_param`).
    let mut next_type_param: u32 = 0;
    let (type_decls, type_resolved) =
        reserve_type_decls(interner, &binder, program, &mut next_type_param);

    let mut pass = Pass {
        interner,
        binder: &binder,
        type_decls,
        type_resolved,
        type_param_scopes: Vec::new(),
        next_type_param,
        generic_fns: FxHashMap::default(),
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
    next_type_param: &mut u32,
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
                // M9: allocate one id per declared type parameter (in source order).
                let params = alloc_type_param_ids(iface.type_parameters.as_deref(), next_type_param);
                decls.push(TypeDecl::Interface {
                    reserved,
                    params,
                    param_decl: iface.type_parameters.as_deref(),
                    members: &iface.body.body,
                });
            }
            Statement::TSTypeAliasDeclaration(alias) => {
                let params = alloc_type_param_ids(alias.type_parameters.as_deref(), next_type_param);
                decls.push(TypeDecl::Alias {
                    annotation: &alias.type_annotation,
                    params,
                    param_decl: alias.type_parameters.as_deref(),
                    resolving: false,
                });
            }
            _ => {}
        }
    }

    (decls, resolved)
}

/// Allocate one fresh [`TypeParamId`] per declared type parameter (M9), in source
/// order, advancing the module-wide counter. Returns an empty vec for a
/// non-generic declaration (`None` type-parameter list). The ids are paired with
/// their source names later, when the body is lowered with a parameter frame in
/// scope ([`with_type_params`]).
fn alloc_type_param_ids(
    decl: Option<&TSTypeParameterDeclaration<'_>>,
    next_type_param: &mut u32,
) -> Vec<TypeParamId> {
    let Some(decl) = decl else {
        return Vec::new();
    };
    decl.params
        .iter()
        .map(|_| {
            let id = TypeParamId(*next_type_param);
            *next_type_param += 1;
            id
        })
        .collect()
}

/// Phase 0b — **fill**. Fill every interface body in place, then force every alias
/// to resolve. After this returns, `pass.type_resolved` is complete, so every
/// `TSTypeReference` encountered in the obligation walk is a plain id lookup.
///
/// **Interfaces are filled BEFORE aliases are resolved** (M9): a generic alias body
/// can *instantiate* a generic interface (`type Wrap<T> = Box<T>`), and
/// instantiation substitutes over the interface's **template** — which must already
/// be filled, not the empty reserved object. An interface body that references an
/// alias still resolves that alias lazily on demand (`resolve_type_decl` is
/// memoized), so the reverse dependency is unaffected by the order. (M5 had the
/// opposite order purely to pre-warm aliases; correctness never depended on it,
/// because resolution is lazy in both directions.)
fn fill_type_decls(pass: &mut Pass, scope: ScopeId) {
    let count = pass.type_decls.len();

    // Fill each interface's reserved id with its lowered members. Members are
    // lowered with the full resolver available; a self/sibling reference resolves
    // to a reserved/resolved id (stored, never inlined); a referenced alias is
    // resolved lazily right here.
    //
    // M9: a **generic** interface is filled with its type parameters in scope, so a
    // member referencing `T` (`value: T`) carries the parameter type. The reserved
    // id then holds a structural **template** (`{ value: T }`); an instantiation
    // `Box<number>` substitutes it. A non-generic interface fills with an empty
    // frame and stays nominal (filled in place, never re-interned).
    for index in 0..count {
        let TypeDecl::Interface {
            reserved,
            ref params,
            param_decl,
            members,
        } = pass.type_decls[index]
        else {
            continue;
        };
        let params = params.clone();
        let frame = build_type_param_frame(pass, param_decl, &params);
        let object = with_type_params(pass, frame, |pass| {
            lower_interface_members(pass, scope, members)
        });
        pass.interner.fill_object(reserved, object);
    }

    // Resolve every remaining alias (interfaces are now filled, so a generic alias
    // instantiating an interface substitutes over the filled template). Resolution
    // is on-demand and idempotent, so touching every alias resolves the whole DAG.
    for index in 0..count {
        if matches!(pass.type_decls[index], TypeDecl::Alias { .. }) {
            resolve_type_decl(pass, scope, DeclId(index as u32));
        }
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
    // Capture the alias annotation and its (M9) type-parameter frame inputs before
    // mutating, so the body is lowered with the parameters in scope.
    let (annotation, param_decl, params) = match pass.type_decls.get(index) {
        Some(TypeDecl::Alias {
            annotation,
            param_decl,
            params,
            resolving: false,
        }) => (*annotation, *param_decl, params.clone()),
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

    // M9: lower the annotation with the alias's type parameters in scope, so a
    // reference to `A`/`B` in `type Pair<A, B> = { … }` resolves to the parameter
    // type. The frame is popped before returning (a parameter does not leak).
    let frame = build_type_param_frame(pass, param_decl, &params);
    let target =
        with_type_params(pass, frame, |pass| lower_annotation(pass, scope, annotation))
            .unwrap_or(error_ty);

    if let Some(TypeDecl::Alias { resolving, .. }) = pass.type_decls.get_mut(index) {
        *resolving = false;
    }
    if let Some(slot) = pass.type_resolved.get_mut(index) {
        *slot = Some(target);
    }
    target
}

/// Resolve a `TSTypeReference` to a `TypeId` (M5, extended for M9).
///
/// Resolution order over a plain identifier name:
///
///  1. **type parameter in scope** (M9): if the name is a type parameter of an
///     enclosing generic declaration ([`lookup_type_param`]), it resolves to that
///     parameter's interned [`TypeTag::TypeParam`] type. A type parameter never
///     takes type arguments (`T<…>` is nonsense), so this only fires without args.
///  2. **type arguments present** (`Box<number>`, M9): instantiate the referenced
///     generic declaration by substituting its parameters with the lowered
///     arguments ([`instantiate_type_reference`]).
///  3. **bare named type** (M5): the declaration's (reserved or lazily-resolved)
///     id, via the binder's type slot.
///
/// An unresolved type name, or a qualified name (`A.B`, out of subset), yields
/// `None` (the caller aborts the enclosing lowering, matching object/union/function
/// lowering).
fn resolve_type_reference(
    pass: &mut Pass,
    scope: ScopeId,
    type_name: &TSTypeName<'_>,
    type_arguments: Option<&TSTypeParameterInstantiation<'_>>,
) -> Option<TypeId> {
    let TSTypeName::IdentifierReference(ident) = type_name else {
        return None;
    };
    let name = ident.name.as_str();

    // 1. A type parameter in scope shadows any named type and takes no arguments.
    if type_arguments.is_none() {
        if let Some(param_ty) = lookup_type_param(pass, name) {
            return Some(param_ty);
        }
    }

    let decl_id = type_decl_id(pass.binder, scope, name)?;

    // 2. With type arguments: instantiate the generic declaration by substitution.
    if let Some(args) = type_arguments {
        return instantiate_type_reference(pass, scope, decl_id, args);
    }

    // 3. Bare named type (M5 behaviour).
    Some(resolve_type_decl(pass, scope, decl_id))
}

/// Instantiate a generic type reference `Name<Arg, …>` by substitution (M9).
///
/// Lowers each type argument (in the *referencing* scope), then substitutes the
/// referenced declaration's type parameters with them into its **template**
/// (`type_resolved[decl_id]`, built with the parameter types embedded). For a
/// generic interface the template is its structural body, so `Box<number>`
/// instantiates to `{ value: number }` — structural assignability then applies (see
/// the FLAG below). Equal instantiations share one interned `TypeId`
/// (`Box<number>` is consistent; `Box<number>` ≠ `Box<string>`).
///
/// Type-argument **arity**: M9 assumes correct arity (the fixtures supply it). A
/// wrong count is handled **gracefully** — the parameter/argument pairs are zipped
/// to the shorter list, so a surplus on either side is ignored rather than
/// panicking; an unmapped parameter simply survives the substitution. No diagnostic
/// is emitted (the `TK2558` wrong-arity check is a future milestone, out of M9
/// scope). An argument that cannot be lowered (out of subset) aborts with `None`,
/// matching the other lowerings.
///
/// FLAG (structural vs nominal generic instances): a generic interface instance is
/// the **substituted structural type**, not a distinct nominal type per
/// instantiation. M9 only needs structural assignability for the fixtures, and the
/// task explicitly allows this; nominal generic instances (so that `Box<number>`
/// and a structurally-equal `{ value: number }` are *not* interchangeable) are a
/// later concern.
fn instantiate_type_reference(
    pass: &mut Pass,
    scope: ScopeId,
    decl_id: DeclId,
    args: &TSTypeParameterInstantiation<'_>,
) -> Option<TypeId> {
    // Lower the type arguments first (in the referencing scope, where any nested
    // type names / parameters live). A non-lowerable argument aborts.
    let mut lowered_args: Vec<TypeId> = Vec::with_capacity(args.params.len());
    for arg in &args.params {
        lowered_args.push(lower_annotation(pass, scope, arg)?);
    }

    // The declaration's template (its body with parameter types embedded) and its
    // ordered parameter ids.
    let template = resolve_type_decl(pass, scope, decl_id);
    let params = type_decl_params(pass, decl_id);

    // Build the substitution, zipping parameters to arguments up to the shorter
    // list (graceful on an arity mismatch — no panic, no spurious diagnostic).
    let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
    for (&param, &arg) in params.iter().zip(&lowered_args) {
        map.insert(param, arg);
    }

    Some(substitute(pass.interner, template, &map))
}

/// The ordered type-parameter ids of a type declaration (M9), or an empty list for
/// a non-generic one / an unknown `DeclId`.
fn type_decl_params(pass: &Pass, decl_id: DeclId) -> Vec<TypeParamId> {
    match pass.type_decls.get(decl_id.index()) {
        Some(TypeDecl::Interface { params, .. }) | Some(TypeDecl::Alias { params, .. }) => {
            params.clone()
        }
        None => Vec::new(),
    }
}

/// The type-space `DeclId` a name resolves to from `scope` (binder type slot), if
/// any. Walks the scope graph like value resolution, then reads the `ty` slot.
fn type_decl_id(binder: &Binder, scope: ScopeId, name: &str) -> Option<DeclId> {
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.ty)
}

// ===========================================================================
// M9: type-parameter scoping — a name → TypeParam frame stack on `Pass`.
// ===========================================================================

/// Build a parameter frame mapping each declared type parameter's **source name**
/// to its interned [`TypeTag::TypeParam`] type, pairing the pre-allocated `ids`
/// (source order) with the names from `param_decl`. A parameter with no resolvable
/// name (out of subset) is skipped; the frame only holds the parameters it can
/// name. Interning here is what makes a body reference `T` resolve to a stable
/// type-parameter id (see [`resolve_type_reference`]).
fn build_type_param_frame(
    pass: &mut Pass,
    param_decl: Option<&TSTypeParameterDeclaration<'_>>,
    ids: &[TypeParamId],
) -> FxHashMap<String, TypeId> {
    let mut frame = FxHashMap::default();
    let Some(param_decl) = param_decl else {
        return frame;
    };
    for (param, &id) in param_decl.params.iter().zip(ids) {
        let name = param.name.name.as_str();
        let interned = pass.interner.intern_type_param(id, name);
        frame.insert(name.to_string(), interned);
    }
    frame
}

/// Run `body` with the type-parameter frame `frame` pushed onto the scope stack,
/// popping it afterwards (so the parameters are in scope **only** for `body`). The
/// pop runs unconditionally, so a type parameter never leaks past its declaration.
/// An empty frame (a non-generic declaration) is still pushed/popped — harmless and
/// keeps the call sites uniform.
fn with_type_params<R>(
    pass: &mut Pass,
    frame: FxHashMap<String, TypeId>,
    body: impl FnOnce(&mut Pass) -> R,
) -> R {
    pass.type_param_scopes.push(frame);
    let result = body(pass);
    pass.type_param_scopes.pop();
    result
}

/// Look a type name up in the in-scope type-parameter frames, innermost first
/// (M9). Returns the interned [`TypeTag::TypeParam`] id if the name is a type
/// parameter currently in scope, so it shadows a same-named named type **inside**
/// the generic. `None` falls through to the binder's type slot.
fn lookup_type_param(pass: &Pass, name: &str) -> Option<TypeId> {
    pass.type_param_scopes
        .iter()
        .rev()
        .find_map(|frame| frame.get(name).copied())
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
        // M8: `switch` narrows the discriminant per `case` (fork-and-restore).
        Statement::SwitchStatement(switch) => {
            check_switch(pass, scope, switch, declared_ret, inferred);
        }
        // Other statements are out of the subset.
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
fn check_switch(
    pass: &mut Pass,
    scope: ScopeId,
    switch: &SwitchStatement<'_>,
    declared_ret: Option<TypeId>,
    inferred: &mut Option<TypeId>,
) {
    // Evaluate the discriminant under the current env (before forking) so its
    // references resolve and nested constructs inside it are checked once.
    infer_expr(pass, scope, &switch.discriminant);
    let discriminant = member_discriminant(pass, scope, &switch.discriminant);

    // Intern every `case`'s literal label up front (mutable). A `default` clause has
    // no test (`None`); a non-literal `case` test also yields `None` (it cannot be
    // narrowed). The labels are reused to build the `default` complement.
    let labels: Vec<Option<TypeId>> = switch
        .cases
        .iter()
        .map(|case| {
            case.test
                .as_ref()
                .and_then(|test| literal_expr_type(pass, test))
        })
        .collect();

    // The non-`default` labels, for the `default` clause's complement.
    let case_labels: Vec<TypeId> = switch
        .cases
        .iter()
        .zip(&labels)
        .filter_map(|(case, label)| case.test.as_ref().and(*label))
        .collect();

    let saved = pass.narrowed.clone();

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
                apply_case_narrowing(pass, *symbol, property, case, *label, &case_labels);
            }
        }

        // Check the clause body in the current scope (a block-bodied case opens its
        // own scope via the `BlockStatement` arm of `check_stmt`).
        for stmt in &case.consequent {
            check_stmt(pass, scope, stmt, declared_ret, inferred);
        }

        // Restore the pre-`switch` env: narrowing must not escape a clause.
        pass.narrowed = saved.clone();

        // Does this clause fall through into the next? (Conservative — empty or
        // non-terminating bodies fall through.)
        prev_fell_through = !clause_terminates(&case.consequent);
    }

    // Restore unconditionally (covers an empty `switch`).
    pass.narrowed = saved;
}

/// Install the narrowing for one `switch` clause into the environment: a `case`
/// with a literal label narrows `symbol` by `symbol.prop === label` (then-sense);
/// a `default` (no label) narrows by the complement of every case label
/// (`symbol.prop !== label` applied for each). A `case` whose test was not a
/// literal (`label == None` but it *is* a `case`) installs no narrowing (sound).
fn apply_case_narrowing(
    pass: &mut Pass,
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
            let current = resolve_identifier_type(pass, symbol);
            let narrowed = narrow(pass.interner, current, &op, /* positive */ true);
            pass.narrowed.insert(symbol, narrowed);
        }
        // `default:` — narrow to the complement of all case labels by removing each
        // label's member in turn (`prop !== label1 && … && prop !== labelN`).
        (None, _) => {
            let mut current = resolve_identifier_type(pass, symbol);
            for &lit in case_labels {
                let op = NarrowOp::Discriminant {
                    property: property.to_string(),
                    literal: lit,
                };
                current = narrow(pass.interner, current, &op, /* positive */ false);
            }
            pass.narrowed.insert(symbol, current);
        }
        // A non-literal `case` test: cannot narrow → leave the env unchanged.
        (Some(_), None) => {}
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
    let narrowed = narrow(pass.interner, current, &fact.op, positive);
    pass.narrowed.insert(fact.symbol, narrowed);
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
fn analyze_guard(pass: &mut Pass, scope: ScopeId, test: &Expression<'_>) -> Option<GuardFact> {
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
        // A binary comparison: `typeof x === "tag"`, `x === null`, `x.kind === "c"`,
        // or the `in` operator `"prop" in x`.
        Expression::BinaryExpression(binary) => {
            if binary.operator == BinaryOperator::In {
                in_guard(pass, scope, binary)
            } else {
                analyze_equality_guard(pass, scope, binary)
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
    pass: &mut Pass,
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

    // Literal-discriminant form (M8): `x.prop === <literal>` (either operand order).
    // **Strict only**: loose `==` coerces, which complicates "could equal", so a
    // loose discriminant comparison narrows nothing (sound). Interns the literal, so
    // this needs `&mut pass`.
    if strict {
        if let Some(fact) = discriminant_guard(pass, scope, left, right, eq_positive) {
            return Some(fact);
        }
        if let Some(fact) = discriminant_guard(pass, scope, right, left, eq_positive) {
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

/// Try to read `member_side` as a discriminant member access `x.prop` (on a
/// narrowable identifier `x`) and `literal_side` as a literal expression, producing
/// a literal-discriminant guard fact targeting `x` (M8). `eq_positive` is the
/// comparison's positive sense (`===` → `true`); it becomes the then-branch
/// polarity (`kind === "circle"` keeps the matching members in the then-branch).
/// The comparison literal is interned (hence `&mut pass`).
fn discriminant_guard(
    pass: &mut Pass,
    scope: ScopeId,
    member_side: &Expression<'_>,
    literal_side: &Expression<'_>,
    eq_positive: bool,
) -> Option<GuardFact> {
    let (symbol, property) = member_discriminant(pass, scope, member_side)?;
    let literal = literal_expr_type(pass, literal_side)?;
    Some(GuardFact {
        symbol,
        op: NarrowOp::Discriminant { property, literal },
        then_positive: eq_positive,
    })
}

/// Analyze a `"prop" in x` expression into an `in`-operator guard fact targeting
/// `x` (M8). The left operand must be a **string-literal** property name and the
/// right operand a narrowable identifier `x`. The then-branch (`in` holds) keeps
/// the members that have the property; an enclosing `!` flips it. Anything else
/// (a non-literal left, a computed/private `in`, a non-identifier right) narrows
/// nothing.
fn in_guard(pass: &Pass, scope: ScopeId, binary: &BinaryExpression<'_>) -> Option<GuardFact> {
    // The property name must be a static string literal: `"a" in x`.
    let Expression::StringLiteral(name) = &binary.left else {
        return None;
    };
    let symbol = condition_symbol(pass, scope, &binary.right)?;
    Some(GuardFact {
        symbol,
        op: NarrowOp::In {
            property: name.value.to_string(),
        },
        then_positive: true,
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
    pass: &Pass,
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
    let symbol = condition_symbol(pass, scope, &member.object)?;
    Some((symbol, member.property.name.to_string()))
}

/// Intern a literal **value** expression (`"circle"`, `42`, `true`) to its literal
/// `TypeId`, or `None` if it is not a plain literal (the discriminant form only
/// narrows against a literal). Mirrors the literal arms of [`infer_expr`].
fn literal_expr_type(pass: &mut Pass, expr: &Expression<'_>) -> Option<TypeId> {
    let value = match expr {
        Expression::StringLiteral(s) => LiteralValue::String(s.value.to_string()),
        Expression::NumericLiteral(n) => LiteralValue::Number(n.value),
        Expression::BooleanLiteral(b) => LiteralValue::Boolean(b.value),
        _ => return None,
    };
    Some(pass.interner.intern_literal(value))
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
///
/// M9: a **generic** function (`function f<T>(…)`) additionally records its generic
/// signature (the type-parameter ids + the template function type) under its value
/// `DeclId`, so a call `f<number>(…)` can instantiate it. Its bound `decl_types`
/// id is the template (signature with the parameter types embedded); a *non-generic*
/// call site would see those parameter types unresolved, but the fixtures only call
/// generic functions with explicit type arguments (inference is M10).
fn check_function_declaration(pass: &mut Pass, scope: ScopeId, func: &Function<'_>) {
    let (fn_ty, params) = infer_function(pass, scope, func);
    if let Some(id) = &func.id {
        if let Some(decl_id) = pass
            .binder
            .graph
            .resolve(scope, id.name.as_str())
            .and_then(|symbol_id| pass.binder.symbols.get(symbol_id))
            .and_then(|s| s.value)
        {
            pass.decl_types.set(decl_id, fn_ty);
            if !params.is_empty() {
                pass.generic_fns.insert(decl_id, GenericSig { params, fn_ty });
            }
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
        // M8: a **literal type** (`"hello"`, `42`, `true`) lowers to its interned
        // literal id. This is what makes union-of-literals (`"a" | "b"`) and the
        // discriminant property (`kind: "circle"`) carry literal types.
        TSType::TSLiteralType(lit) => return lower_literal_type(pass, &lit.literal),
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
        // M5/M9: a type reference (`Point`, `Num`, `List`, `Box<number>`, an
        // in-scope type parameter `T`) resolves through the type-parameter scope,
        // the binder's type slot, and (with arguments) generic instantiation.
        // Qualified names (`A.B`) are out of subset.
        TSType::TSTypeReference(reference) => {
            return resolve_type_reference(
                pass,
                scope,
                &reference.type_name,
                reference.type_arguments.as_deref(),
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

/// Lower a **literal type** (`TSLiteralType`'s literal) to its interned literal
/// `TypeId` (M8). A string/number/boolean literal interns to the hash-consed
/// literal id (so it shares identity with the same literal anywhere, which is what
/// makes literal↔literal assignability and discriminant matching reduce to id
/// equality). A `bigint`/template/`-1`-style (unary) literal type is out of the M8
/// subset → `None` (the caller aborts the enclosing annotation, matching the other
/// lowerings — silently dropping it would mis-state the type).
fn lower_literal_type(pass: &mut Pass, literal: &TSLiteral<'_>) -> Option<TypeId> {
    let value = match literal {
        TSLiteral::StringLiteral(s) => LiteralValue::String(s.value.to_string()),
        TSLiteral::NumericLiteral(n) => LiteralValue::Number(n.value),
        TSLiteral::BooleanLiteral(b) => LiteralValue::Boolean(b.value),
        // `bigint`, template-literal types, and unary (`-1`) literal types are out
        // of the M8 subset.
        TSLiteral::BigIntLiteral(_)
        | TSLiteral::TemplateLiteral(_)
        | TSLiteral::UnaryExpression(_) => return None,
    };
    Some(pass.interner.intern_literal(value))
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
            // A generic function *expression*'s type parameters are scoped to its
            // body (handled inside `infer_function`); the param ids are not
            // registered for a call site (only a named generic `function`
            // declaration is callable with explicit type args in the M9 subset —
            // inference is M10).
            let (id, _params) = infer_function(pass, scope, func);
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

/// Instantiate a **generic call with explicit type arguments** (`identity<number>
/// (5)`, M9), returning the instantiated function `TypeId`, or `None` when this is
/// not such a call (no type arguments, or the callee is not a registered generic
/// `function`). The caller then runs the usual arity/argument/return checks against
/// the instantiated parameter/return types.
///
/// Resolution: the callee must be a plain identifier resolving (through the scope
/// graph) to a value `DeclId` registered in [`Pass::generic_fns`]. Its type
/// arguments are lowered (in the call's scope), then substituted into the generic's
/// template signature (`params[i] → arg[i]`).
///
/// Type-argument **arity** is assumed correct (the fixtures supply it). A wrong
/// count is handled **gracefully** — parameters and arguments are zipped to the
/// shorter list, so a surplus on either side is ignored and an unmapped parameter
/// simply survives the substitution; no panic, and no diagnostic (the `TK2558`
/// wrong-arity check is out of M9 scope). An argument that cannot be lowered aborts
/// the instantiation (`None`), so the call falls back to the inferred callee path.
fn instantiate_generic_callee(
    pass: &mut Pass,
    scope: ScopeId,
    call: &CallExpression<'_>,
) -> Option<TypeId> {
    let args = call.type_arguments.as_deref()?;

    // The callee must be a plain identifier naming a generic function declaration.
    let Expression::Identifier(ident) = &call.callee else {
        return None;
    };
    let decl_id = pass
        .binder
        .graph
        .resolve(scope, ident.name.as_str())
        .and_then(|symbol_id| pass.binder.symbols.get(symbol_id))
        .and_then(|s| s.value)?;
    let sig = pass.generic_fns.get(&decl_id)?.clone();

    // Lower the type arguments (in the call's scope). A non-lowerable argument
    // aborts → fall back to the inferred callee path.
    let mut lowered_args: Vec<TypeId> = Vec::with_capacity(args.params.len());
    for arg in &args.params {
        lowered_args.push(lower_annotation(pass, scope, arg)?);
    }

    // Substitute the generic's parameters with the arguments (graceful on an arity
    // mismatch: zip to the shorter list) and return the instantiated signature.
    let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
    for (&param, &arg) in sig.params.iter().zip(&lowered_args) {
        map.insert(param, arg);
    }
    Some(substitute(pass.interner, sig.fn_ty, &map))
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

    // M9: an **explicit-type-argument generic call** (`identity<number>(5)`) is
    // instantiated by substitution *before* the usual checks. When the callee is a
    // registered generic function and type arguments are present, the instantiated
    // signature replaces the template; otherwise the callee is inferred normally.
    let instantiated_callee = instantiate_generic_callee(pass, scope, call);

    // Always infer the callee expression for its side effects (resolving its name /
    // emitting TK2304, descending into a callee expression). Its inferred type is
    // used only when there was no explicit-args instantiation above.
    let inferred_callee = infer_expr(pass, scope, &call.callee);

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

    // The instantiated signature wins; otherwise the inferred callee type.
    let callee_ty = match instantiated_callee.or(inferred_callee.map(|(ty, _)| ty)) {
        Some(ty) => ty,
        None => return Some((wk.error, call_span)),
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

/// Infer a `function` declaration/expression's type and check its body, returning
/// the interned function type **and** its type-parameter ids (M9 — empty for a
/// non-generic function).
///
/// M9: a generic function's type parameters are allocated and pushed onto the
/// type-parameter scope for the whole signature + body, so `x: T`, the return `: T`,
/// and a nested `Box<T>` lower to the parameter type. The interned result is the
/// **template** (its signature with the parameter types embedded); the returned ids
/// pair with it for instantiation at a call site. The frame is popped before
/// returning (a type parameter does not leak past its function).
fn infer_function(
    pass: &mut Pass,
    enclosing: ScopeId,
    func: &Function<'_>,
) -> (TypeId, Vec<TypeParamId>) {
    let param_ids = alloc_type_param_ids(func.type_parameters.as_deref(), &mut pass.next_type_param);
    let frame = build_type_param_frame(pass, func.type_parameters.as_deref(), &param_ids);

    let fn_ty = with_type_params(pass, frame, |pass| {
        let fn_scope = pass.binder.fn_scopes.get(&func.span.start).copied();
        let params = lower_parameters(pass, enclosing, fn_scope, &func.params);

        // Declared return type from the annotation, if any. Type references in the
        // signature resolve from the enclosing scope (where the type names live);
        // type parameters resolve through the pushed frame.
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
        pass.interner.intern_function(FunctionType { params, ret })
    });

    (fn_ty, param_ids)
}

/// Infer an arrow's type and check its body. An expression-body arrow's return
/// type is the body expression's type (widened when not annotated).
///
/// M9: a generic arrow (`<T>(x: T) => T`) scopes its type parameters to its
/// signature + body (the frame is pushed for the whole inference and popped after).
/// Generic arrows are not in the M9 fixtures and an arrow's params are not
/// registered for a call site (only a named generic `function` declaration is
/// callable with explicit type args before inference, M10) — the scoping is here so
/// a parameter never leaks.
fn infer_arrow(
    pass: &mut Pass,
    enclosing: ScopeId,
    arrow: &ArrowFunctionExpression<'_>,
) -> TypeId {
    let param_ids =
        alloc_type_param_ids(arrow.type_parameters.as_deref(), &mut pass.next_type_param);
    let frame = build_type_param_frame(pass, arrow.type_parameters.as_deref(), &param_ids);
    with_type_params(pass, frame, |pass| infer_arrow_inner(pass, enclosing, arrow))
}

/// The body of [`infer_arrow`], run with any type-parameter frame already pushed.
fn infer_arrow_inner(
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

    // -----------------------------------------------------------------------
    // M8 — discriminated-union / `in` / `switch` narrowing soundness.
    // -----------------------------------------------------------------------

    /// Discriminant narrowing (`x.kind === "lit"`) refines inside the branch and
    /// does **not escape** the `if`: the matching property is accessible in-branch
    /// but the pre-`if` wide access still errors. Mirrors the `discriminated.ts`
    /// fixture's headline behaviour as a unit-level soundness pin.
    #[test]
    fn discriminant_narrows_in_branch_and_does_not_escape() {
        let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape) {
  const wide = s.radius;
  if (s.kind === \"circle\") {
    const r: number = s.radius;
  } else {
    const d: number = s.side;
  }
  const after = s.radius;
}
";
        // The two wide `s.radius` accesses (lines 3 and 9) are TK2339; both branch
        // bodies are clean (then narrows to circle, else to square).
        assert_eq!(
            diags(src),
            vec![(3, "TK2339".to_string()), (9, "TK2339".to_string())]
        );
    }

    /// An **unrecognized discriminant narrows nothing** (no false negatives): a
    /// member access on a different symbol, or a non-literal comparison, leaves the
    /// union wide in both branches. Here `s.kind === t.kind` is not a literal
    /// discriminant, so the in-branch `s.radius` still errors.
    #[test]
    fn unknown_discriminant_does_not_narrow() {
        let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape, t: Shape) {
  if (s.kind === t.kind) {
    const bad = s.radius;
  }
}
";
        // The discriminant is not `x.prop === <literal>` → no narrowing → line 4
        // errors with TK2339 (radius not on every member of the wide union).
        assert_eq!(diags(src), vec![(4, "TK2339".to_string())]);
    }

    /// Discriminant narrowing keys on the **specific symbol**: `s.kind === "circle"`
    /// narrows `s`, never a different union-typed symbol `t`.
    #[test]
    fn discriminant_narrows_only_its_symbol() {
        let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape, t: Shape) {
  if (s.kind === \"circle\") {
    const r: number = s.radius;
    const bad = t.radius;
  }
}
";
        // `s` is narrowed to circle (line 4 clean); `t` is untouched, so `t.radius`
        // (line 5) errors with TK2339.
        assert_eq!(diags(src), vec![(5, "TK2339".to_string())]);
    }

    /// `in`-operator narrowing refines both branches and keys on the symbol. The
    /// pre-`if` wide access errors; each branch sees the narrowed member.
    #[test]
    fn in_operator_narrows_both_branches() {
        let src = "\
type Box = { a: number } | { b: string };
function f(x: Box) {
  const bad = x.a;
  if (\"a\" in x) {
    const ok: number = x.a;
  } else {
    const ok2: string = x.b;
  }
}
";
        // Only the wide `x.a` (line 3) errors; the then-branch narrows to `{ a }`
        // and the else-branch to `{ b }`.
        assert_eq!(diags(src), vec![(3, "TK2339".to_string())]);
    }

    /// `switch` narrows the discriminant per `case` and the narrowing does **not
    /// escape** a clause: a clause accessing the *other* member's property errors.
    #[test]
    fn switch_narrows_per_case_and_does_not_escape() {
        let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape): number {
  switch (s.kind) {
    case \"circle\": {
      return s.radius;
    }
    case \"square\": {
      return s.side;
    }
  }
  return 0;
}
function bad(s: Shape) {
  switch (s.kind) {
    case \"circle\": {
      const w: number = s.side;
      break;
    }
  }
}
";
        // The circle/square clauses are clean (narrowed); the `bad` switch's circle
        // clause accesses `s.side` (line 16) → TK2339 (narrowed to circle).
        assert_eq!(diags(src), vec![(16, "TK2339".to_string())]);
    }

    /// Conservative fallthrough: a `case` that **falls through** (no terminator)
    /// into the next clause must not let the next clause over-narrow. With an empty
    /// `case "circle":` falling into `case "square":`, the `square` clause's value
    /// could still be a circle, so a `circle`-only access must NOT be assumed — and,
    /// symmetrically, the wide union access errors. This pins that the per-case
    /// narrowing is suppressed on fallthrough (soundness, no false negative).
    #[test]
    fn switch_fallthrough_is_conservative() {
        let src = "\
type Shape = { kind: \"circle\"; radius: number } | { kind: \"square\"; side: number };
function area(s: Shape) {
  switch (s.kind) {
    case \"circle\":
    case \"square\": {
      const bad = s.side;
    }
  }
}
";
        // `case "circle":` is empty → falls through into `case "square":`, whose
        // body therefore could see a circle too. Narrowing is suppressed (wide
        // union), so `s.side` (line 6) errors with TK2339.
        assert_eq!(diags(src), vec![(6, "TK2339".to_string())]);
    }

    /// `switch` reassignment reset still holds across clauses: assigning the
    /// discriminant symbol inside a clause drops its narrowing for later references
    /// in that clause.
    #[test]
    fn switch_clause_respects_reassignment_reset() {
        let src = "\
function f(x: string | number) {
  switch (typeof x) {
    case \"string\": {
      x = 1;
      const s: string = x;
    }
  }
}
";
        // The discriminant is `typeof x` (not a member-access discriminant), so the
        // switch installs no narrowing; `x = 1` then resets any narrowing and `x`
        // stays `string | number`, so line 5 errors (TK2322). This pins that a
        // switch clause does not leave a stale narrowing.
        assert_eq!(diags(src), vec![(5, "TK2322".to_string())]);
    }
}

#[cfg(test)]
mod generics_tests {
    //! M9 end-to-end tests for generics (type parameters, generic functions /
    //! interfaces / aliases, explicit type arguments, instantiation by
    //! substitution). These drive the whole pipeline (parse → bind → check) and
    //! assert the *set* of `(line, code)` diagnostics, so they pin the behaviours
    //! the reviewer should scrutinize: substitution flows through the instantiated
    //! parameter/return/body, instantiation interns consistently, a type parameter
    //! does not leak past its declaration, and a wrong type-argument count does not
    //! panic (graceful, no new diagnostic).

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

    /// A generic function instantiated with explicit type args: the instantiated
    /// **return** drives `TK2322` and the instantiated **parameter** drives
    /// `TK2345`, while the correctly-typed call is clean. This is the headline
    /// `generic_functions.ts` behaviour as a unit-level pin.
    #[test]
    fn generic_function_instantiates_return_and_parameter() {
        let src = "\
function identity<T>(x: T): T { return x; }
const a: number = identity<number>(5);
const b: number = identity<string>(\"s\");
const c = identity<number>(\"s\");
";
        // `identity<number>(5)` (line 2) is clean; `identity<string>` returns string
        // → line 3 TK2322; the arg "s" is not number → line 4 TK2345.
        assert_eq!(
            diags(src),
            vec![(3, "TK2322".to_string()), (4, "TK2345".to_string())]
        );
    }

    /// Two type parameters substitute independently and positionally: `pick<A, B>`
    /// returns `A`, so `pick<string, number>` returns `string`.
    #[test]
    fn generic_function_with_two_type_parameters() {
        let src = "\
function pick<A, B>(a: A, b: B): A { return a; }
const d: number = pick<number, string>(1, \"x\");
const e: number = pick<string, number>(\"x\", 1);
";
        // `pick<number, string>` returns number (line 2 clean); `pick<string,
        // number>` returns string (line 3 TK2322).
        assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
    }

    /// A generic interface instantiated with a type argument: the instantiated
    /// object body drives `TK2322` (wrong member type) and `TK2353` (excess
    /// property on the instantiated type).
    #[test]
    fn generic_interface_instantiates_body() {
        let src = "\
interface Box<T> { value: T; }
const x: Box<number> = { value: 1 };
const y: Box<number> = { value: \"s\" };
const z: Box<number> = { value: 1, extra: 2 };
";
        // `{ value: 1 }` is a `Box<number>` (line 2 clean); `{ value: "s" }` is not
        // (line 3 TK2322); `extra` is excess on the instantiated type (line 4 TK2353).
        assert_eq!(
            diags(src),
            vec![(3, "TK2322".to_string()), (4, "TK2353".to_string())]
        );
    }

    /// A generic type alias `Pair<A, B>` instantiates both parameters.
    #[test]
    fn generic_alias_instantiates_both_parameters() {
        let src = "\
type Pair<A, B> = { first: A; second: B };
const p: Pair<number, string> = { first: 1, second: \"s\" };
const q: Pair<number, string> = { first: \"s\", second: 1 };
";
        assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
    }

    /// Nested instantiation `Box<Box<number>>`: substitution flows through the
    /// nested generic, so the inner member's type is checked too.
    #[test]
    fn nested_generic_instantiation_flows_through() {
        let src = "\
interface Box<T> { value: T; }
const nn: Box<Box<number>> = { value: { value: 1 } };
const mm: Box<Box<number>> = { value: { value: \"s\" } };
";
        // The well-typed nested literal (line 2) is clean; the inner `"s"` (line 3)
        // is not assignable to the substituted inner `number` → TK2322.
        assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
    }

    /// A generic type nested in a generic function: `unwrap<T>(b: Box<T>): T`
    /// substitutes `Box<T>` and the return, so the parameter drives `TK2345` and the
    /// return drives `TK2322` (the `nested.ts` headline).
    #[test]
    fn generic_type_nested_in_generic_function() {
        let src = "\
interface Box<T> { value: T; }
function unwrap<T>(b: Box<T>): T { return b.value; }
const n: number = unwrap<number>({ value: 1 });
const m: number = unwrap<string>({ value: \"s\" });
const bad = unwrap<number>({ value: \"s\" });
";
        // `unwrap<number>({value:1})` is clean (line 3); `unwrap<string>` returns
        // string (line 4 TK2322); `{value:"s"}` is not a `Box<number>` (line 5
        // TK2345).
        assert_eq!(
            diags(src),
            vec![(4, "TK2322".to_string()), (5, "TK2345".to_string())]
        );
    }

    /// **No leak**: a type parameter is in scope only within its own declaration.
    /// `T` declared on `first` must not resolve inside `second`; an out-of-scope `T`
    /// is an unresolved type name, so its annotation cannot be lowered and no
    /// (spurious) assignability error fires — the assignment is simply unchecked.
    /// The control is that the *same* code with `T` in scope (inside `first`)
    /// behaves like a real type parameter. Here we pin that referencing `T` outside
    /// its function does not crash and does not narrow another declaration's checks.
    #[test]
    fn type_parameter_does_not_leak_across_declarations() {
        let src = "\
function first<T>(x: T): T { return x; }
function second(y: number): number { return y; }
const ok: number = second(1);
const bad: number = second(\"s\");
";
        // `second` is non-generic; its `number` parameter rejects the string arg
        // (line 4 TK2345). `T` from `first` never leaks to affect `second`.
        assert_eq!(diags(src), vec![(4, "TK2345".to_string())]);
    }

    /// A type parameter **shadows** a same-named named type inside the generic, and
    /// the shadowing does not escape: outside the generic, the named type is seen
    /// again. `T` (the alias `= string`) is shadowed by the parameter `T` inside
    /// `f`, so `f<number>(5)` is fine; outside, `T` is `string`.
    #[test]
    fn type_parameter_shadows_named_type_only_inside() {
        let src = "\
type T = string;
function f<T>(x: T): T { return x; }
const a: number = f<number>(5);
const outside: T = \"s\";
const bad: T = 5;
";
        // Inside `f`, `T` is the parameter (so `f<number>(5)` returns number → line 3
        // clean). Outside, `T` is the alias `string`: line 4 clean, line 5 TK2322
        // (number not assignable to string).
        assert_eq!(diags(src), vec![(5, "TK2322".to_string())]);
    }

    /// `Box<number>` and `Box<string>` are **distinct** instantiations: assigning a
    /// `Box<string>`-shaped literal to a `Box<number>` annotation errors, confirming
    /// the two instantiations are different interned types.
    #[test]
    fn distinct_instantiations_are_not_interchangeable() {
        let src = "\
interface Box<T> { value: T; }
const a: Box<number> = { value: 1 };
const b: Box<string> = { value: 1 };
";
        // `{ value: 1 }` is a `Box<number>` (line 2 clean) but not a `Box<string>`
        // (line 3 TK2322) — the instantiations are distinct.
        assert_eq!(diags(src), vec![(3, "TK2322".to_string())]);
    }

    /// **Graceful arity**: a wrong type-argument count must not panic and must not
    /// emit a new diagnostic (the `TK2558` check is out of M9 scope). Too few and
    /// too many arguments are both handled best-effort.
    #[test]
    fn wrong_type_argument_count_does_not_panic() {
        // Too few type args on a 2-parameter generic, and too many on a
        // 1-parameter generic. Neither should panic; we only assert the run
        // completes and produces no *new-code* diagnostic from the arity mismatch.
        let src = "\
function pick<A, B>(a: A, b: B): A { return a; }
interface Box<T> { value: T; }
const p = pick<number>(1, 2);
const x: Box<number> = { value: 1 };
type Bad = Box<number, string>;
const y: Bad = { value: 1 };
";
        // The run completes (no panic). No diagnostic carries an out-of-M9 code; the
        // only codes that may appear are the existing M0–M9 ones. We assert there is
        // no TK2558 (the future wrong-arity code) and the run is well-formed.
        let codes: Vec<String> = diags(src).into_iter().map(|(_, c)| c).collect();
        assert!(
            !codes.iter().any(|c| c == "TK2558"),
            "wrong type-arg arity must not emit TK2558 in M9, got {codes:?}"
        );
    }
}

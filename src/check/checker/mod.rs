//! The statement-level checker (architecture §5, mvp-plan §5 — M0–M6 rows, plus
//! the post-MVP M7/M8 narrowing slices, the M9/M10 generics core + inference, and
//! the M11 classes slice).
//!
//! M11 scope (post-MVP — classes, first slice), on top of M0–M10:
//!
//!  - **Class declarations** ([`Statement::ClassDeclaration`]): a `class C { … }`
//!    binds `C` into **both** declaration spaces — the **type** slot (its instance
//!    type) and the **value** slot (its constructor). The instance type is reserved
//!    up front (the M5 reserve-then-fill, [`TypeDecl::Class`]) so a field can
//!    reference the class's own type (`next: Node | null`) or a sibling.
//!  - **Instance type** ([`fill_class`]): a (nominal) **object** type whose members
//!    are the class's **fields** (each `PropertyDefinition` with a type annotation)
//!    plus its **methods** (`MethodDefinition` of kind `method`) as function-typed
//!    properties — so member access (`p.x`, `p.method`) and the structural relation
//!    reuse the M2 object rules unchanged. The constructor is **not** an instance
//!    member.
//!  - **`new ClassName(args)`** ([`infer_new`]): resolves the class via its value
//!    slot, checks the constructor signature's **arity** (`TK2554`) and argument
//!    **assignability** (`TK2345`) exactly like an M3 call ([`check_call_arguments`]),
//!    and yields the **instance type**. A class with no explicit `constructor`
//!    defaults to zero parameters.
//!  - **`this`** ([`Pass::current_this`]): inside a method/constructor body, a
//!    `ThisExpression` resolves to the instance type (so `this.field`/`this.method()`
//!    resolve). It is set by [`check_class`] (save/restore) and does **not leak** — a
//!    `this` outside any class member is the error type (no narrowing, no crash).
//!  - **Method bodies** ([`check_class`] via [`infer_function`]): parameters bind in
//!    the method scope (like an M3 function) with `this` available, and each
//!    `return <expr>` is checked against the method's declared return type (`TK2322`,
//!    the M3 return path).
//!  - **Structural assignability**: the instance type is an object type, so the
//!    existing relation makes instances interchangeable with matching object types
//!    both ways (driving `TK2322`/`TK2741`).
//!  - **Deferred at M11** (subsequently implemented in later slices — listed here as
//!    the original M11 snapshot): inheritance (`extends`/heritage) and `super` (M12);
//!    access modifiers (`private`/`protected`) + nominal typing and `static` members
//!    (M13); **member-assignment-target checking** (`obj.prop = expr`) and `readonly`
//!    (M14 — see the "M14 scope" block below). Still deferred: getters/setters,
//!    `abstract`, generic classes, `implements`, parameter properties, method overloads.
//!
//! M14 scope (post-MVP — member-assignment targets + `readonly`), on top of M0–M13:
//!
//!  - **Member-assignment-target checking** ([`check_member_assignment`]): an
//!    assignment whose target is a **static member** (`obj.prop = expr`, including
//!    `this.prop`) now resolves the base's type, looks up the property, and checks the
//!    RHS is assignable to the property's type → `TK2322` (primary span = the RHS) —
//!    filling the gap deferred since M11, so the existing M11–M13 constructor/method
//!    assignments are checked too (and stay clean: they are all type-correct). The
//!    obligation is **skipped** when the base/property is unresolved or error/`any`-typed
//!    **or the RHS type is the error type**, mirroring how the M3/M11 return path
//!    tolerates incomplete expression inference (e.g. an arithmetic `this.count + by`),
//!    so no spurious error and no regression. Compound assignment (`+=`) on a member
//!    target stays deferred (unchecked, M7 narrowing-reset behaviour unaffected); so do
//!    element-access (`obj[k] = …`) and destructuring targets.
//!  - **`readonly`** ([`crate::types::repr::PropertyType::readonly`]): the `readonly`
//!    field modifier is read into the member's structural identity (folded into the
//!    object hash + `object_props_eq`, like visibility), so it is preserved through
//!    interning — but the **relation engine ignores it** for assignability (a `readonly`
//!    and a mutable member relate both ways). It gates only assignment **targets**:
//!    assigning to a `readonly` member is `TK2540`, **except** `this.prop` inside the
//!    **declaring class's constructor** (tracked by [`Pass::current_in_ctor`] alongside
//!    [`Pass::current_class`]).
//!  - **Deferred** (skipped without error/crash): getters/setters, `abstract`, generic
//!    classes, method-override compatibility (`TK2416`), `readonly` arrays /
//!    `ReadonlyArray` / deep readonly, element-access/computed/destructuring assignment
//!    targets, and compound-assignment type checking. New code: `TK2540`.
//!
//! M9 scope (post-MVP — generics core: type parameters + explicit type arguments +
//! instantiation by substitution; type-argument **inference** is M10,
//! **constraints** `<T extends U>` follow), on top of M0–M8:
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

use crate::binder::bind_module_with_prelude;
use crate::binder::Binder;
use crate::diagnostics::Diagnostic;
use crate::relate::{Relater, Relation};
use crate::types::repr::TypeParamId;
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rustc_hash::{FxHashMap, FxHashSet};

mod annotations;
mod assignment;
mod calls;
mod classes;
mod context;
mod decls;
pub(in crate::check) mod eval;
mod expr;
mod flowgraph;
mod narrowing;
mod statements;

use context::{ClassFillState, DeclTypes, Pass, TypeDecl};
use decls::{reserve_type_decls, type_decl_id};
use statements::{emit_obligation_failure, emit_override_failures};

/// The embedded **prelude** source (M28): the built-in utility-type slice, checked
/// as a real compilation unit before every user program. A trusted asset — it must
/// check clean (asserted in debug builds + pinned by unit test); its diagnostics
/// never surface (their spans would be meaningless against the user source).
pub(crate) const PRELUDE_SOURCE: &str = include_str!("../../prelude.ts");

/// Check a parsed program and return the diagnostics it produces.
pub fn check_program<'ast>(
    interner: &mut Interner,
    program: &'ast Program<'ast>,
) -> Vec<Diagnostic> {
    // --- M28 prelude unit: parse + bind + resolve the built-in utility aliases
    // BEFORE user code. The prelude AST lives only inside this function (its own
    // allocator); everything the user pass consumes from it — scope-graph entries,
    // resolved `TypeId`s interned into the SHARED store, type-parameter ids — is
    // lifetime-free, so the user decl table holds [`TypeDecl::Resolved`]
    // placeholders for prelude indices instead of AST borrows. ---
    let prelude_allocator = Allocator::default();
    let prelude_parsed = Parser::new(&prelude_allocator, PRELUDE_SOURCE, SourceType::ts()).parse();
    debug_assert!(
        !prelude_parsed.panicked && prelude_parsed.diagnostics.is_empty(),
        "the prelude must parse clean: {:?}",
        prelude_parsed.diagnostics
    );

    let binder = bind_module_with_prelude(&prelude_parsed.program, program);

    // Reserve a nominal id for every interface and record every type-declaration
    // body, indexed by its type-space `DeclId`, BEFORE any body is lowered (the
    // reserve half of reserve-then-fill — mvp-plan M5, §6.3). Aliases are resolved
    // lazily; interface ids are available immediately. M9: each declaration's type
    // parameters get fresh ids here too (advancing `next_type_param`).
    let mut next_type_param: u32 = 0;
    // M13: allocate one stable `ClassId` per declared class (in source order),
    // advancing this counter as `reserve_type_decls` walks the declarations.
    let mut next_class_id: u32 = 0;
    let total_type_decls = binder.type_decl_count as usize;
    let mut type_resolved: Vec<Option<TypeId>> = vec![None; total_type_decls];

    // --- Prelude pass: reserve + fill ONLY the prelude declarations, in a scoped
    // `Pass` whose decl table borrows the (function-local) prelude AST. Every
    // prelude alias resolves here — in the PRELUDE scope, so `Omit`'s
    // `Pick`/`Exclude` references can never be captured by user shadows — and is
    // memoized into `type_resolved` before any user lowering runs. ---
    let mut prelude_decls: Vec<TypeDecl> = Vec::new();
    reserve_type_decls(
        interner,
        &binder,
        binder.prelude_module,
        &prelude_parsed.program,
        &mut next_type_param,
        &mut next_class_id,
        &mut prelude_decls,
        &mut type_resolved,
    );
    let mut prelude_pass = build_pass(
        interner,
        &binder,
        prelude_decls,
        type_resolved,
        DeclTypes::new(0),
        next_type_param,
    );
    prelude_pass.fill_type_decls(binder.prelude_module);
    // The prelude is a TRUSTED asset: it must check clean (unit-tested + asserted
    // here in debug builds); anything a release build would produce is filtered by
    // the extraction below, never surfaced to users.
    debug_assert!(
        prelude_pass.diagnostics.is_empty(),
        "the prelude must check clean: {:?}",
        prelude_pass.diagnostics
    );
    // Extract the lifetime-free outputs and drop the prelude pass (releasing the
    // prelude AST borrows). The prelude produces no obligations/overrides (it holds
    // only type aliases).
    let prelude_params: Vec<Vec<TypeParamId>> = prelude_pass
        .type_decls
        .iter()
        .map(|decl| match decl {
            TypeDecl::Interface { params, .. }
            | TypeDecl::Alias { params, .. }
            | TypeDecl::Class { params, .. }
            | TypeDecl::Resolved { params } => params.clone(),
        })
        .collect();
    let Pass {
        mut type_resolved,
        next_type_param: prelude_next_type_param,
        ..
    } = prelude_pass;
    next_type_param = prelude_next_type_param;

    // Seed the four **string intrinsics** by prelude-declared identity (M28/WU3):
    // each `type Uppercase<S extends string> = intrinsic;` alias resolves to its
    // well-known marker id, which the evaluator intercepts on instantiation. Done
    // AFTER the prelude fill so the aliases' `S extends string` constraints were
    // recorded by the ordinary resolution path (the `= intrinsic` body itself
    // lowers to nothing, silently).
    {
        let wk = interner.well_known();
        for (name, marker) in [
            ("Uppercase", wk.uppercase),
            ("Lowercase", wk.lowercase),
            ("Capitalize", wk.capitalize),
            ("Uncapitalize", wk.uncapitalize),
        ] {
            if let Some(decl_id) = type_decl_id(&binder, binder.prelude_module, name) {
                if let Some(slot) = type_resolved.get_mut(decl_id.index()) {
                    *slot = Some(marker);
                }
            }
        }
    }

    // --- User pass: prelude indices become lifetime-free `Resolved` placeholders
    // (their `type_resolved` slots are complete); user declarations append after
    // them, index-aligned with the binder's DeclIds. ---
    let mut type_decls: Vec<TypeDecl<'ast>> = prelude_params
        .into_iter()
        .map(|params| TypeDecl::Resolved { params })
        .collect();
    reserve_type_decls(
        interner,
        &binder,
        binder.module,
        program,
        &mut next_type_param,
        &mut next_class_id,
        &mut type_decls,
        &mut type_resolved,
    );
    let decl_types = DeclTypes::new(binder.decl_count);
    let mut pass = build_pass(
        interner,
        &binder,
        type_decls,
        type_resolved,
        decl_types,
        next_type_param,
    );

    // --- Phase 0 (fill): resolve every alias and fill every interface body. From
    // here on `type_resolved` is complete, so a `TSTypeReference` in the walk is a
    // plain id lookup. ---
    pass.fill_type_decls(binder.module);

    // --- Phase 0.5 (flow): build the control-flow graph for the whole module —
    // every function body / the top level — recording each reference's flow node.
    // Runs before the check walk so the loop back edges are complete (the check
    // walk resolves narrowed types against this finished graph). ---
    pass.build_flow_graph(binder.module, &program.body);

    // --- Phase 1: bind-resolved walk over the module body. ---
    pass.check_statements(binder.module, &program.body);

    // Move the working set out before borrowing the store immutably for phase 2.
    let Pass {
        interner,
        obligations,
        override_checks,
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

    // Backlog 06: decide class-member override compatibility on the same relater
    // (its cache is shared with the obligation queries above — sound).
    emit_override_failures(
        store,
        well_known,
        &mut relater,
        &override_checks,
        &mut diagnostics,
    );

    diagnostics
}

/// Construct a fresh phase-1 [`Pass`] over the given decl tables (M28 — one for the
/// prelude unit, one for the user unit; the working-set defaults are identical).
///
/// M12/B28/B29 fill states are derived from the decl table: a class index starts
/// `Pending` (instance type / constructor built on demand, base first); an interface
/// or seeded object-literal-alias index starts `Pending` for the template machinery;
/// everything else — including M28 `Resolved` prelude placeholders — is `Done`.
fn build_pass<'a, 'ast>(
    interner: &'a mut Interner,
    binder: &'a Binder,
    type_decls: Vec<TypeDecl<'ast>>,
    type_resolved: Vec<Option<TypeId>>,
    decl_types: DeclTypes,
    next_type_param: u32,
) -> Pass<'a, 'ast> {
    let class_fill: Vec<ClassFillState> = type_decls
        .iter()
        .map(|decl| match decl {
            TypeDecl::Class { .. } => ClassFillState::Pending,
            _ => ClassFillState::Done,
        })
        .collect();
    let template_fill: Vec<ClassFillState> = type_decls
        .iter()
        .map(|decl| match decl {
            TypeDecl::Interface { .. } => ClassFillState::Pending,
            TypeDecl::Alias {
                object_template: Some(_),
                ..
            } => ClassFillState::Pending,
            _ => ClassFillState::Done,
        })
        .collect();

    Pass {
        interner,
        binder,
        type_decls,
        type_resolved,
        type_param_scopes: Vec::new(),
        next_type_param,
        class_parents: FxHashMap::default(),
        generic_fns: FxHashMap::default(),
        class_ctors: FxHashMap::default(),
        class_type_params: FxHashMap::default(),
        class_pending_abstract: FxHashMap::default(),
        class_member_kinds: FxHashMap::default(),
        class_names: FxHashMap::default(),
        class_fill,
        template_fill,
        decl_types,
        obligations: Vec::new(),
        override_checks: Vec::new(),
        diagnostics: Vec::new(),
        // M23 flow-graph state. Slots 0/1 are the UNREACHABLE/START sentinels the
        // whole arena reserves (see `FlowNodeId::{UNREACHABLE,START}`).
        flow_nodes: vec![
            crate::check::flow::FlowNode::Unreachable,
            crate::check::flow::FlowNode::Start,
        ],
        flow_cursor: crate::check::flow::FlowNodeId::START,
        flow_loops: Vec::new(),
        reference_flow: FxHashMap::default(),
        flow_memo: FxHashMap::default(),
        flow_provisional: FxHashMap::default(),
        flow_loop_depth: 0,
        cond_memo: FxHashMap::default(),
        cond_frames: Vec::new(),
        building_template: false,
        resolving_conditional_alias: None,
        resolving_alias: None,
        resolving_alias_stack: Vec::new(),
        circular_aliases: FxHashSet::default(),
        alias_indirection_depth: 0,
        mapped_frames: Vec::new(),
        current_this: None,
        current_class: None,
        current_super_ctor: None,
        current_in_ctor: false,
    }
}

// ===========================================================================
// M5: named types — reserve-then-fill (mvp-plan M5, §3, §6.3).
// ===========================================================================

// ===========================================================================
// M9: type-parameter scoping — a name → TypeParam frame stack on `Pass`.
// ===========================================================================

#[cfg(test)]
mod tests;

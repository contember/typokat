//! AST → scope graph + symbols (architecture §4, mvp-plan §3).
//!
//! The binder walks the program, builds the scope graph, and declares a
//! value-space [`Symbol`] for each binding. A `DeclId` is assigned per value
//! declaration; the checker later keys its `DeclId → TypeId` table on it
//! (architecture §4.1: the declared/inferred type lives with the declaration).
//!
//! M5 scope, on top of M3's value bindings:
//!
//!  - **Type declarations.** A `type X = …` and a single-declaration
//!    `interface X { … }` declare `X` in the **type space** (`Symbol.ty`), using a
//!    separate `DeclId` numbering space (`type_decl_count`) so a name can occupy
//!    both the value and type slots without their `DeclId`s colliding. The checker
//!    keys a `type DeclId → TypeId` table on it. Type-declaration names are bound
//!    **before** any body is walked (declared up front in [`bind_module`]), so a
//!    body can reference itself or a sibling — the two-phase reserve-then-fill the
//!    recursive-type fixture relies on lives in the checker, but the *names* must
//!    already resolve, which is what the up-front type-space declarations give it.
//!
//! M3 scope, on top of M1's top-level variable bindings:
//!
//!  - **Function declarations.** A `function foo(...) {...}` declares `foo` in the
//!    value space of its enclosing scope (so a call `foo()` resolves).
//!  - **Function/arrow scopes.** Every function declaration, function expression,
//!    and arrow gets its own [`ScopeKind::Function`] scope whose parent is the
//!    enclosing scope, with each parameter declared as a value symbol inside it.
//!    The scope is recorded in [`Binder::fn_scopes`], keyed by `(module scope,
//!    span start)`, so the checker can descend into the body with the parameters in
//!    scope and resolve `return x`.
//!
//! M11 scope, on top of M5's type declarations:
//!
//!  - **Class declarations.** A `class C { … }` declares `C` in **both** the type
//!    space (its instance type — up front in [`bind_type_declarations`], so a field
//!    can reference the class's own type or a sibling) and the value space (its
//!    constructor — in [`bind_class_declaration`]). Each method/constructor is a
//!    [`Function`], so it gets its own [`ScopeKind::Function`] scope with parameters
//!    bound, exactly like a free function; field initializers are walked for nested
//!    functions. Inheritance, `static`/accessor members, and parameter properties
//!    are out of the M11 subset (see [`bind_class`]).
//!
//! The walk recurses through expression positions and statement bodies so that
//! nested functions (an arrow inside a `const`, a function expression in a call
//! argument, …) each get a scope. Destructuring patterns, namespace declarations,
//! and control-flow statements outside the subset contribute no bindings (their
//! sub-expressions are still walked for nested functions where the AST shape is in
//! the subset).

use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{DeclId, Symbol, SymbolId, SymbolTable};
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, BlockStatement, Class, ClassElement, Expression,
    Declaration, Function, FunctionBody, Program, Statement, SwitchStatement, VariableDeclarator,
};
use rustc_hash::FxHashMap;

/// The binder's output for one file: the scope graph, the symbol table, the
/// module scope id, and the per-function scope map. `decl_count` is the number of
/// value declarations bound, which sizes the checker's `DeclId → TypeId` table.
pub struct Binder {
    pub graph: ScopeGraph,
    pub symbols: SymbolTable,
    /// The **user** module scope. M28: its parent is [`Binder::prelude_module`], so a
    /// user reference falls through to the prelude names and a user declaration
    /// shadows them by ordinary innermost-first resolution (no duplicate-name
    /// diagnostics — the two units are distinct scopes).
    pub module: ScopeId,
    /// The **prelude** root scope (M28) — the compilation unit holding the built-in
    /// utility aliases, bound BEFORE the user program. Its parent is `None`.
    pub prelude_module: ScopeId,
    /// Number of value declarations assigned a `DeclId` (`DeclId`s run
    /// `0..decl_count`). Includes variable bindings, function declaration names,
    /// and function parameters.
    pub decl_count: u32,
    /// Number of **type** declarations assigned a `DeclId` (type-space `DeclId`s
    /// run `0..type_decl_count`). This is a **separate numbering space** from the
    /// value `DeclId`s above: the type slot's `DeclId` keys the checker's
    /// `type DeclId → TypeId` table, while the value slot's keeps keying the value
    /// `DeclId → TypeId` table. Keeping them separate lets one name occupy both
    /// slots (`namespace`/`interface`/`class` merging, §4.1) without collision.
    pub type_decl_count: u32,
    /// Maps a function/arrow node to the [`ScopeKind::Function`] scope holding its
    /// parameters. The checker uses this to descend into the body with parameters
    /// resolvable. Keyed by `(module scope, span start)`: span starts are unique
    /// only *within a file*, so in a shared project `BindState` (many modules) the
    /// module scope disambiguates offset-aligned nodes across files (backlog 58).
    pub fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// Maps a `{ … }` block to its [`ScopeKind::Block`] lexical scope (M7). Each
    /// block gets its own scope so a `let`/`const` declared inside an `if`/`else`
    /// branch lives in that branch, not the enclosing function scope — keeping
    /// branch-local names from colliding across branches. Keyed by
    /// `(module scope, span start)`, like `fn_scopes` (backlog 58).
    pub block_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
}

/// Mutable binder state threaded through the recursive walk.
pub(crate) struct ImportedSymbol {
    name: String,
    value: Option<ImportedSlot>,
    ty: Option<ImportedSlot>,
}

impl ImportedSymbol {
    pub(crate) fn new(name: String, value: Option<DeclId>, ty: Option<DeclId>) -> Self {
        ImportedSymbol {
            name,
            value: value.map(ImportedSlot::Existing),
            ty: ty.map(ImportedSlot::Existing),
        }
    }

    pub(crate) fn placeholder_type(name: String) -> Self {
        ImportedSymbol {
            name,
            value: None,
            ty: Some(ImportedSlot::Placeholder),
        }
    }

    pub(crate) fn placeholder_value_and_type(name: String) -> Self {
        ImportedSymbol {
            name,
            value: Some(ImportedSlot::Placeholder),
            ty: Some(ImportedSlot::Placeholder),
        }
    }
}

pub(crate) enum ImportedSlot {
    Existing(DeclId),
    Placeholder,
}

pub(crate) struct ImportPlaceholder {
    pub(crate) value: Option<DeclId>,
    pub(crate) ty: Option<DeclId>,
}

struct BindState {
    graph: ScopeGraph,
    symbols: SymbolTable,
    fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// Per-block lexical scopes (M7), keyed by `(module scope, block span start)`.
    block_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// The module scope currently being bound — the disambiguating half of the
    /// scope-map keys (backlog 58). Set before each module's body is walked.
    current_module: ScopeId,
    /// Running `DeclId` counter for value declarations.
    next_decl: u32,
    /// Running `DeclId` counter for **type** declarations (separate space).
    next_type_decl: u32,
}

impl BindState {
    /// Allocate the next value-space `DeclId`.
    fn fresh_decl(&mut self) -> DeclId {
        let id = DeclId(self.next_decl);
        self.next_decl += 1;
        id
    }

    /// Allocate the next type-space `DeclId` (separate numbering space).
    fn fresh_type_decl(&mut self) -> DeclId {
        let id = DeclId(self.next_type_decl);
        self.next_type_decl += 1;
        id
    }
}

/// Build the scope graph and symbol table for the **prelude + user** program pair
/// (M28).
///
/// The prelude unit binds first, into its own root [`ScopeKind::Module`] scope; the
/// user module scope is then created **with the prelude scope as its parent**, so
/// user references fall through to the prelude names and user declarations shadow
/// them (innermost-first resolution — tsc-like, and no duplicate-name diagnostics
/// since the two units are distinct scopes). `DeclId` numbering (both spaces) runs
/// prelude-first, matching the checker's prelude-then-user decl-table layout.
///
/// Within each unit the M5 shape is kept: every top-level **type** name (type
/// aliases + interfaces + class type sides) is declared up front, before value
/// declarations and bodies, so forward/mutual references resolve regardless of
/// textual order — the precondition for the checker's reserve-then-fill.
pub fn bind_module_with_prelude(prelude: &Program<'_>, program: &Program<'_>) -> Binder {
    let mut builder = ProjectBinderBuilder::new(prelude);
    let (module, _) = builder.add_module(program, &[]);
    builder.finish(module)
}

/// Incremental binder for one serial project graph (M29 slice 1).
pub(crate) struct ProjectBinderBuilder {
    state: BindState,
    prelude_module: ScopeId,
}

impl ProjectBinderBuilder {
    /// Bind the prelude first so its declarations keep the low `DeclId` ranges.
    pub(crate) fn new(prelude: &Program<'_>) -> Self {
        let mut state = BindState {
            graph: ScopeGraph::new(),
            symbols: SymbolTable::new(),
            fn_scopes: FxHashMap::default(),
            block_scopes: FxHashMap::default(),
            current_module: ScopeId(0),
            next_decl: 0,
            next_type_decl: 0,
        };

        let prelude_module = state.graph.push(Scope::new(ScopeKind::Module, None));
        state.current_module = prelude_module;
        bind_type_declarations(&mut state, prelude_module, &prelude.body);
        bind_statements(&mut state, prelude_module, &prelude.body);

        ProjectBinderBuilder {
            state,
            prelude_module,
        }
    }

    /// Add one project module. Imported symbols are declared before local names so
    /// declarations in this file can reference imports during reserve/fill.
    pub(crate) fn add_module(
        &mut self,
        program: &Program<'_>,
        imports: &[ImportedSymbol],
    ) -> (ScopeId, Vec<ImportPlaceholder>) {
        let module = self
            .state
            .graph
            .push(Scope::new(ScopeKind::Module, Some(self.prelude_module)));
        self.state.current_module = module;
        let mut placeholders = Vec::new();
        for import in imports {
            placeholders.push(declare_import(
                &mut self.state,
                module,
                &import.name,
                &import.value,
                &import.ty,
            ));
        }
        bind_type_declarations(&mut self.state, module, &program.body);
        bind_statements(&mut self.state, module, &program.body);
        (module, placeholders)
    }

    pub(crate) fn finish(self, module: ScopeId) -> Binder {
        Binder {
            graph: self.state.graph,
            symbols: self.state.symbols,
            module,
            prelude_module: self.prelude_module,
            decl_count: self.state.next_decl,
            type_decl_count: self.state.next_type_decl,
            fn_scopes: self.state.fn_scopes,
            block_scopes: self.state.block_scopes,
        }
    }

    pub(crate) fn symbol_slots(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> (Option<DeclId>, Option<DeclId>) {
        let Some(symbol_id) = self.state.graph.resolve(scope, name) else {
            return (None, None);
        };
        self.state
            .symbols
            .get(symbol_id)
            .map(|symbol| (symbol.value, symbol.ty))
            .unwrap_or((None, None))
    }
}

/// Declare every top-level `type`/`interface` name into the **type space** of
/// `scope`, each with a fresh type-space `DeclId`. Run before any body walk so a
/// type body can reference itself or a (later-declared) sibling. Bodies are not
/// inspected here — only the names are introduced (the checker reserves the
/// `TypeId` and fills the body in its own two-phase pass).
fn bind_type_declarations(state: &mut BindState, scope: ScopeId, statements: &[Statement<'_>]) {
    for stmt in statements {
        bind_type_declaration_statement(state, scope, stmt);
    }
}

fn bind_type_declaration_statement(state: &mut BindState, scope: ScopeId, stmt: &Statement<'_>) {
    match stmt {
        Statement::TSTypeAliasDeclaration(alias) => {
            let decl_id = state.fresh_type_decl();
            declare_type(state, scope, alias.id.name.as_str(), decl_id);
        }
        Statement::TSInterfaceDeclaration(iface) => {
            let decl_id = state.fresh_type_decl();
            declare_type(state, scope, iface.id.name.as_str(), decl_id);
        }
        // M11: a `class` declares a **type-space** name (its instance type), so a
        // self/sibling field reference (`next: Node | null`) resolves. The
        // *value*-space name (the constructor) is declared in `bind_statement`
        // alongside the rest of the value bindings. A class with no name is out
        // of subset (an anonymous class expression statement); skip it.
        Statement::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                let decl_id = state.fresh_type_decl();
                declare_type(state, scope, id.name.as_str(), decl_id);
            }
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                bind_type_declaration(state, scope, decl);
            }
        }
        _ => {}
    }
}

fn bind_type_declaration(state: &mut BindState, scope: ScopeId, decl: &Declaration<'_>) {
    match decl {
        Declaration::TSTypeAliasDeclaration(alias) => {
            let decl_id = state.fresh_type_decl();
            declare_type(state, scope, alias.id.name.as_str(), decl_id);
        }
        Declaration::TSInterfaceDeclaration(iface) => {
            let decl_id = state.fresh_type_decl();
            declare_type(state, scope, iface.id.name.as_str(), decl_id);
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                let decl_id = state.fresh_type_decl();
                declare_type(state, scope, id.name.as_str(), decl_id);
            }
        }
        _ => {}
    }
}

/// Bind a list of statements into `scope`.
fn bind_statements(state: &mut BindState, scope: ScopeId, statements: &[Statement<'_>]) {
    for stmt in statements {
        bind_statement(state, scope, stmt);
    }
}

/// Bind one statement into `scope` (declarations) and recurse into its
/// expressions/bodies for nested functions.
fn bind_statement(state: &mut BindState, scope: ScopeId, stmt: &Statement<'_>) {
    match stmt {
        Statement::VariableDeclaration(decl) => {
            for declarator in &decl.declarations {
                bind_declarator(state, scope, declarator);
            }
        }
        Statement::FunctionDeclaration(func) => {
            bind_function_declaration(state, scope, func);
        }
        // M11: a `class` declares its name in the value space (the constructor) and
        // binds its body (each method/constructor gets its own function scope with
        // its parameters, and property initializers are walked for nested functions).
        Statement::ClassDeclaration(class) => {
            bind_class_declaration(state, scope, class);
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                bind_declaration(state, scope, decl);
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            bind_expression(state, scope, &expr_stmt.expression);
        }
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                bind_expression(state, scope, arg);
            }
        }
        // M7: control-flow statements. The `if` test is bound for nested functions;
        // each branch statement is bound recursively (a `{ … }` branch gets its own
        // block scope via the `BlockStatement` arm below).
        Statement::IfStatement(if_stmt) => {
            bind_expression(state, scope, &if_stmt.test);
            bind_statement(state, scope, &if_stmt.consequent);
            if let Some(alternate) = &if_stmt.alternate {
                bind_statement(state, scope, alternate);
            }
        }
        // M7: a `{ … }` block opens its own lexical scope (so branch-local
        // `let`/`const` do not leak into the enclosing scope or collide across
        // branches). A bare block statement (not an `if` branch) is handled the
        // same way.
        Statement::BlockStatement(block) => {
            bind_block(state, scope, block);
        }
        // M8: a `switch` binds its discriminant (for nested functions) and each
        // clause's statements in the enclosing scope — a block-bodied clause
        // (`case x: { … }`) opens its own scope via the `BlockStatement` arm above.
        Statement::SwitchStatement(switch) => {
            bind_switch(state, scope, switch);
        }
        // M23: a `while` loop binds its condition (for nested functions) and its body
        // recursively (a `{ … }` body opens its own block scope via the
        // `BlockStatement` arm above, so a `let`/`const` declared in the loop body is
        // resolvable — the flow checker now walks these bodies).
        Statement::WhileStatement(while_stmt) => {
            bind_expression(state, scope, &while_stmt.test);
            bind_statement(state, scope, &while_stmt.body);
        }
        // Other statements declare no names in the subset; their sub-expressions (if
        // any) are not in the subset either.
        _ => {}
    }
}

fn bind_declaration(state: &mut BindState, scope: ScopeId, decl: &Declaration<'_>) {
    match decl {
        Declaration::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                bind_declarator(state, scope, declarator);
            }
        }
        Declaration::FunctionDeclaration(func) => {
            bind_function_declaration(state, scope, func);
        }
        Declaration::ClassDeclaration(class) => {
            bind_class_declaration(state, scope, class);
        }
        _ => {}
    }
}

/// Bind a `{ … }` block into its own [`ScopeKind::Block`] child scope under
/// `parent`, recording it under `(module scope, block span start)` so the checker
/// descends into the matching scope. The block's statements are bound inside it.
fn bind_block(state: &mut BindState, parent: ScopeId, block: &BlockStatement<'_>) {
    let block_scope = state
        .graph
        .push(Scope::new(ScopeKind::Block, Some(parent)));
    state
        .block_scopes
        .insert((state.current_module, block.span.start), block_scope);
    bind_statements(state, block_scope, &block.body);
}

/// Bind a `switch` statement (M8): bind the discriminant expression (for nested
/// functions) and every clause's statements into `scope`. A block-bodied clause
/// (`case x: { … }`) opens its own lexical scope through the `BlockStatement` arm
/// of [`bind_statement`], so a `let`/`const` declared in that block stays local to
/// the clause. The `case` *test* expressions are literals in the subset (no nested
/// functions), but are bound defensively for any in-subset shape.
fn bind_switch(state: &mut BindState, scope: ScopeId, switch: &SwitchStatement<'_>) {
    bind_expression(state, scope, &switch.discriminant);
    for case in &switch.cases {
        if let Some(test) = &case.test {
            bind_expression(state, scope, test);
        }
        bind_statements(state, scope, &case.consequent);
    }
}

/// Bind a variable declarator: declare its identifier (if a plain identifier) in
/// `scope`, then recurse into the initializer for nested functions.
fn bind_declarator(state: &mut BindState, scope: ScopeId, declarator: &VariableDeclarator<'_>) {
    if let Some(name) = binding_name(&declarator.id) {
        let decl_id = state.fresh_decl();
        declare_value(state, scope, name, decl_id);
    }
    if let Some(init) = &declarator.init {
        bind_expression(state, scope, init);
    }
}

/// Bind a function declaration: declare its name (value space) in `scope`, then
/// bind the function itself (its own scope + parameters + body).
fn bind_function_declaration(state: &mut BindState, scope: ScopeId, func: &Function<'_>) {
    if let Some(id) = &func.id {
        let decl_id = state.fresh_decl();
        declare_value(state, scope, id.name.as_str(), decl_id);
    }
    bind_function(state, scope, func);
}

/// Bind a class declaration (M11): declare its name in the **value** space (the
/// constructor side — its type side is declared up front in
/// [`bind_type_declarations`]), then bind the class body. A class with no name is
/// out of the M11 subset (an anonymous class) — its body is still bound so any
/// nested function is given a scope.
fn bind_class_declaration(state: &mut BindState, scope: ScopeId, class: &Class<'_>) {
    if let Some(id) = &class.id {
        let decl_id = state.fresh_decl();
        declare_value(state, scope, id.name.as_str(), decl_id);
    }
    bind_class(state, scope, class);
}

/// Bind a class's body (M11): each method/constructor is a [`Function`] value, so
/// it gets its own [`ScopeKind::Function`] scope with its parameters bound (via
/// [`bind_function`]) — the checker descends into the method body with the
/// parameters resolvable, exactly like a free function. Each property
/// initializer expression is walked for nested functions.
///
/// M12 (`extends`/`super`) needs **no** binder change: the `super_class` clause is a
/// type/value *reference* resolved later by the checker (it declares no name and holds
/// no nested function in the subset), and a `super(args)` call inside the constructor
/// body is reached through the normal `CallExpression` walk (its `Super` callee binds
/// to nothing; its arguments are walked for nested functions). So inheritance is
/// handled entirely in the checker.
///
/// M15: `get`/`set` accessors are `MethodDefinition`s, so they already get a function
/// scope here (the loop binds **every** `MethodDefinition`) — a setter's parameter and
/// an accessor body resolve exactly like a method's, with no binder change. An
/// `abstract method(): T;` is also a `MethodDefinition` (with no body); `bind_function`
/// handles the absent body. `abstract` on the class itself declares no name and is
/// recorded by the checker, so it needs nothing here either.
///
/// F3 / backlog 01: constructor **parameter properties** (`constructor(private x: T)`) need
/// **no** binder change — the constructor is a `MethodDefinition`, so `bind_function` already
/// binds its parameters (and body) like any method's; the *member* a parameter property
/// declares is synthesized later by the checker ([`collect_class_own_members`]) and accessed
/// through the instance type (`this.x`), not via a name bound here.
///
/// DEFERRED (still out of scope): `implements`. That element kind is skipped here (no
/// binding); its sub-expressions in the subset are still safe.
fn bind_class(state: &mut BindState, parent: ScopeId, class: &Class<'_>) {
    for element in &class.body.body {
        match element {
            // A method/constructor/accessor: bind its `Function` value (own scope +
            // parameters + body). A `get`/`set` accessor (M15) is bound the same way, so
            // its body and a setter's parameter resolve; an `abstract` method has no body
            // (handled by `bind_function`).
            ClassElement::MethodDefinition(method) => {
                bind_function(state, parent, &method.value);
            }
            // A field: walk its initializer for nested functions (the field's type
            // itself is an annotation, which holds no value bindings).
            ClassElement::PropertyDefinition(prop) => {
                if let Some(init) = &prop.value {
                    bind_expression(state, parent, init);
                }
            }
            // Static blocks, accessor properties, and index signatures are out of
            // the M11 subset — no value bindings.
            _ => {}
        }
    }
}

/// Bind a function/arrow's own scope: create a [`ScopeKind::Function`] scope
/// under `parent`, declare each parameter as a value symbol in it, record the
/// scope under `(module scope, function span start)`, and recurse into the body.
///
/// Each parameter gets a fresh `DeclId`; the checker fills its type from the
/// parameter annotation when it descends into the function.
fn bind_function(state: &mut BindState, parent: ScopeId, func: &Function<'_>) {
    let fn_scope = state
        .graph
        .push(Scope::new(ScopeKind::Function, Some(parent)));
    state
        .fn_scopes
        .insert((state.current_module, func.span.start), fn_scope);

    for param in &func.params.items {
        if let Some(name) = binding_name(&param.pattern) {
            let decl_id = state.fresh_decl();
            declare_value(state, fn_scope, name, decl_id);
        }
    }

    if let Some(body) = &func.body {
        bind_function_body(state, fn_scope, body);
    }
}

/// Bind an arrow's own scope, mirroring [`bind_function`]. An arrow always has a
/// body (an expression body or a block); the body is bound inside the arrow's
/// function scope.
fn bind_arrow(state: &mut BindState, parent: ScopeId, arrow: &ArrowFunctionExpression<'_>) {
    let fn_scope = state
        .graph
        .push(Scope::new(ScopeKind::Function, Some(parent)));
    state
        .fn_scopes
        .insert((state.current_module, arrow.span.start), fn_scope);

    for param in &arrow.params.items {
        if let Some(name) = binding_name(&param.pattern) {
            let decl_id = state.fresh_decl();
            declare_value(state, fn_scope, name, decl_id);
        }
    }

    bind_function_body(state, fn_scope, &arrow.body);
}

/// Bind a function body's statements into the function scope. An expression-body
/// arrow is parsed as a block holding a single `return <expr>`, so walking the
/// statements covers both forms.
fn bind_function_body(state: &mut BindState, fn_scope: ScopeId, body: &FunctionBody<'_>) {
    bind_statements(state, fn_scope, &body.statements);
}

/// Recurse into an expression, binding any nested function/arrow scopes. Only the
/// expression shapes in the M3 subset are descended; others are left untouched
/// (no nested function there in the corpus).
fn bind_expression(state: &mut BindState, scope: ScopeId, expr: &Expression<'_>) {
    match expr {
        Expression::FunctionExpression(func) => bind_function(state, scope, func),
        Expression::ArrowFunctionExpression(arrow) => bind_arrow(state, scope, arrow),
        // M11: a class expression (`const C = class { … }`) binds its body so a
        // method gets a function scope. Class *expressions* are out of the M11
        // fixture subset (their instance type is not named), but binding keeps the
        // walk uniform and never panics.
        Expression::ClassExpression(class) => bind_class(state, scope, class),
        // M11: `new C(args)` — bind the callee and each argument for nested
        // functions, mirroring the call-expression arm.
        Expression::NewExpression(new_expr) => {
            bind_expression(state, scope, &new_expr.callee);
            for arg in &new_expr.arguments {
                if let Some(arg_expr) = arg.as_expression() {
                    bind_expression(state, scope, arg_expr);
                }
            }
        }
        Expression::CallExpression(call) => {
            bind_expression(state, scope, &call.callee);
            for arg in &call.arguments {
                if let Some(arg_expr) = arg.as_expression() {
                    bind_expression(state, scope, arg_expr);
                }
            }
        }
        Expression::AssignmentExpression(assign) => {
            bind_expression(state, scope, &assign.right);
        }
        Expression::StaticMemberExpression(member) => {
            bind_expression(state, scope, &member.object);
        }
        Expression::ObjectExpression(obj) => {
            for member in &obj.properties {
                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) = member {
                    bind_expression(state, scope, &prop.value);
                }
            }
        }
        Expression::ParenthesizedExpression(paren) => {
            bind_expression(state, scope, &paren.expression);
        }
        // Literals, identifiers, and other expression shapes hold no nested
        // function in the M3 subset.
        _ => {}
    }
}

/// Declare a value-space binding `name` in `scope`, merging into an existing
/// symbol if the name is already present (so the multi-slot symbol carries the
/// value slot under the same id — architecture §4.1). Redeclaration in the same
/// space (`TK2451`) is deferred (mvp-plan); the later binding wins.
fn declare_value(state: &mut BindState, scope: ScopeId, name: &str, decl_id: DeclId) {
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.value = Some(decl_id);
        }
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.value = Some(decl_id);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
}

/// Declare a **type-space** binding `name` in `scope`, merging into an existing
/// symbol if the name is already present so the type slot lives under the same id
/// as any value slot (architecture §4.1). Declaration merging across two type
/// declarations (`interface`+`interface`, `TK2451`) is deferred (mvp-plan M5
/// scope); the later binding wins, and M5 fixtures use unique type names.
fn declare_type(state: &mut BindState, scope: ScopeId, name: &str, decl_id: DeclId) {
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.ty = Some(decl_id);
        }
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.ty = Some(decl_id);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
}

fn declare_import(
    state: &mut BindState,
    scope: ScopeId,
    name: &str,
    value: &Option<ImportedSlot>,
    ty: &Option<ImportedSlot>,
) -> ImportPlaceholder {
    let (value_decl, value_placeholder) = match value {
        Some(ImportedSlot::Existing(decl_id)) => (Some(*decl_id), None),
        Some(ImportedSlot::Placeholder) => {
            let decl_id = state.fresh_decl();
            (Some(decl_id), Some(decl_id))
        }
        None => (None, None),
    };
    let (type_decl, type_placeholder) = match ty {
        Some(ImportedSlot::Existing(decl_id)) => (Some(*decl_id), None),
        Some(ImportedSlot::Placeholder) => {
            let decl_id = state.fresh_type_decl();
            (Some(decl_id), Some(decl_id))
        }
        None => (None, None),
    };
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.value = value_decl;
            symbol.ty = type_decl;
        }
        return ImportPlaceholder {
            value: value_placeholder,
            ty: type_placeholder,
        };
    }
    let mut symbol = Symbol::new(name);
    symbol.value = value_decl;
    symbol.ty = type_decl;
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
    ImportPlaceholder {
        value: value_placeholder,
        ty: type_placeholder,
    }
}

/// The bound name of a binding pattern, if it is a plain identifier. Returns
/// `None` for destructuring patterns (out of the M3 subset).
fn binding_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

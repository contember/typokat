//! AST → scope graph + symbols (architecture §4, mvp-plan §3).
//!
//! The binder walks the program, builds the scope graph, and declares a
//! value-space [`Symbol`] for each binding. A `DeclId` is assigned per value
//! declaration; the checker later keys its `DeclId → TypeId` table on it
//! (architecture §4.1: the declared/inferred type lives with the declaration).
//!
//! M3 scope, on top of M1's top-level variable bindings:
//!
//!  - **Function declarations.** A `function foo(...) {...}` declares `foo` in the
//!    value space of its enclosing scope (so a call `foo()` resolves).
//!  - **Function/arrow scopes.** Every function declaration, function expression,
//!    and arrow gets its own [`ScopeKind::Function`] scope whose parent is the
//!    enclosing scope, with each parameter declared as a value symbol inside it.
//!    The scope is recorded in [`Binder::fn_scopes`], keyed by the function node's
//!    span start, so the checker can descend into the body with the parameters in
//!    scope and resolve `return x`.
//!
//! The walk recurses through expression positions and statement bodies so that
//! nested functions (an arrow inside a `const`, a function expression in a call
//! argument, …) each get a scope. Destructuring patterns, type/namespace
//! declarations, classes, and control-flow statements are out of the M3 subset
//! and contribute no bindings (their sub-expressions are still walked for nested
//! functions where the AST shape is in the subset).

use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{DeclId, Symbol, SymbolId, SymbolTable};
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, Expression, Function, FunctionBody, Program,
    Statement, VariableDeclarator,
};
use rustc_hash::FxHashMap;

/// The binder's output for one file: the scope graph, the symbol table, the
/// module scope id, and the per-function scope map. `decl_count` is the number of
/// value declarations bound, which sizes the checker's `DeclId → TypeId` table.
pub struct Binder {
    pub graph: ScopeGraph,
    pub symbols: SymbolTable,
    pub module: ScopeId,
    /// Number of value declarations assigned a `DeclId` (`DeclId`s run
    /// `0..decl_count`). Includes variable bindings, function declaration names,
    /// and function parameters.
    pub decl_count: u32,
    /// Maps a function/arrow node's span start to the [`ScopeKind::Function`]
    /// scope holding its parameters. The checker uses this to descend into the
    /// body with parameters resolvable. Span starts are unique per node within a
    /// file, so they are a stable key shared by the binder and the checker.
    pub fn_scopes: FxHashMap<u32, ScopeId>,
}

/// Mutable binder state threaded through the recursive walk.
struct BindState {
    graph: ScopeGraph,
    symbols: SymbolTable,
    fn_scopes: FxHashMap<u32, ScopeId>,
    /// Running `DeclId` counter for value declarations.
    next_decl: u32,
}

impl BindState {
    /// Allocate the next `DeclId`.
    fn fresh_decl(&mut self) -> DeclId {
        let id = DeclId(self.next_decl);
        self.next_decl += 1;
        id
    }
}

/// Build the scope graph and symbol table for a program.
///
/// Creates the module scope, declares every top-level value binding (variables
/// and function declaration names) up front so references resolve regardless of
/// textual order, then recurses to give each function its own scope. The checker
/// still relies on initialization order for *types* (fixtures declare before use).
pub fn bind_module(program: &Program<'_>) -> Binder {
    let mut graph = ScopeGraph::new();
    let symbols = SymbolTable::new();
    let module = graph.push(Scope::new(ScopeKind::Module, None));

    let mut state = BindState {
        graph,
        symbols,
        fn_scopes: FxHashMap::default(),
        next_decl: 0,
    };

    bind_statements(&mut state, module, &program.body);

    Binder {
        graph: state.graph,
        symbols: state.symbols,
        module,
        decl_count: state.next_decl,
        fn_scopes: state.fn_scopes,
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
        Statement::ExpressionStatement(expr_stmt) => {
            bind_expression(state, scope, &expr_stmt.expression);
        }
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                bind_expression(state, scope, arg);
            }
        }
        // Other statements declare no names in the M3 subset; their
        // sub-expressions (if any) are not in the subset either.
        _ => {}
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

/// Bind a function/arrow's own scope: create a [`ScopeKind::Function`] scope
/// under `parent`, declare each parameter as a value symbol in it, record the
/// scope under the function node's span start, and recurse into the body.
///
/// Each parameter gets a fresh `DeclId`; the checker fills its type from the
/// parameter annotation when it descends into the function.
fn bind_function(state: &mut BindState, parent: ScopeId, func: &Function<'_>) {
    let fn_scope = state
        .graph
        .push(Scope::new(ScopeKind::Function, Some(parent)));
    state.fn_scopes.insert(func.span.start, fn_scope);

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
    state.fn_scopes.insert(arrow.span.start, fn_scope);

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

/// The bound name of a binding pattern, if it is a plain identifier. Returns
/// `None` for destructuring patterns (out of the M3 subset).
fn binding_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

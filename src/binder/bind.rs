//! AST → scope graph + symbols (architecture §4, mvp-plan §3).
//!
//! M1 walks `program.body`, builds the module scope, and declares a value-space
//! [`Symbol`] for each variable binding. A `DeclId` is assigned per value
//! declaration; the checker later keys its `DeclId → TypeId` table on it
//! (architecture §4.1: the declared/inferred type lives with the declaration).
//!
//! Only top-level `BindingIdentifier` bindings are bound — the M1 subset.
//! Destructuring patterns, functions, and type/namespace declarations are bound
//! by later milestones (they fill the `ty`/`ns` slots and add nested scopes).

use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{DeclId, Symbol, SymbolId, SymbolTable};
use oxc_ast::ast::{BindingPattern, Program, Statement};

/// The binder's output for one file: the scope graph, the symbol table, and the
/// module scope id. `decl_count` is the number of value declarations bound,
/// which sizes the checker's `DeclId → TypeId` table.
pub struct Binder {
    pub graph: ScopeGraph,
    pub symbols: SymbolTable,
    pub module: ScopeId,
    /// Number of value declarations assigned a `DeclId` (`DeclId`s run
    /// `0..decl_count`).
    pub decl_count: u32,
}

/// Build the scope graph and symbol table for a program.
///
/// Creates the module scope, then declares a value symbol for every top-level
/// variable binding. All module-level names are declared up front so that
/// references resolve regardless of textual order; the checker still relies on
/// initialization order for *types* (M1 fixtures declare before use).
pub fn bind_module(program: &Program<'_>) -> Binder {
    let mut graph = ScopeGraph::new();
    let mut symbols = SymbolTable::new();
    let module = graph.push(Scope::new(ScopeKind::Module, None));

    // Running `DeclId` counter for value declarations.
    let mut next_decl: u32 = 0;

    for stmt in &program.body {
        if let Statement::VariableDeclaration(decl) = stmt {
            for declarator in &decl.declarations {
                let Some(name) = binding_name(&declarator.id) else {
                    // Non-identifier binding (destructuring) — out of M1 scope.
                    continue;
                };
                let decl_id = DeclId(next_decl);
                next_decl += 1;
                declare_value(&mut graph, &mut symbols, module, name, decl_id);
            }
        }
        // Other statements declare no names in the M1 subset.
    }

    Binder {
        graph,
        symbols,
        module,
        decl_count: next_decl,
    }
}

/// Declare a value-space binding `name` in `scope`, merging into an existing
/// symbol if the name is already present (so the multi-slot symbol carries the
/// value slot under the same id — architecture §4.1). Redeclaration in the same
/// space (`TK2451`) is deferred (mvp-plan); the later binding wins.
fn declare_value(
    graph: &mut ScopeGraph,
    symbols: &mut SymbolTable,
    scope: ScopeId,
    name: &str,
    decl_id: DeclId,
) {
    if let Some(existing) = graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = symbols.get_mut(existing) {
            symbol.value = Some(decl_id);
        }
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.value = Some(decl_id);
    let symbol_id: SymbolId = symbols.push(symbol);
    graph.declare(scope, name, symbol_id);
}

/// The bound name of a declarator's pattern, if it is a plain identifier.
/// Returns `None` for destructuring patterns (out of M1 scope).
fn binding_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

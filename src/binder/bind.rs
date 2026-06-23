//! AST → scope graph + symbols (architecture §4, mvp-plan §3).
//!
//! TODO(M1): walk the program, create scopes, and declare a multi-slot `Symbol`
//! for each binding/type/namespace declaration, merging into the correct
//! declaration space (architecture §4.1). M0 has no name resolution to do — its
//! fixtures contain only `const NAME: T = literal;` with no references — so this
//! module is intentionally empty beyond the entry-point sketch below.

#![allow(dead_code)] // TODO(M1): implemented by the binder milestone.

use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};

/// Build the scope graph for a program. M0 stub: creates only the module scope
/// so the shape is in place; M1 walks `program.body` and declares symbols.
///
/// This is a documented placeholder, not a reachable panic — M0's driver does
/// not call it.
pub fn bind_module() -> (ScopeGraph, ScopeId) {
    let mut graph = ScopeGraph::new();
    let module = graph.push(Scope::new(ScopeKind::Module, None));
    // TODO(M1): for stmt in program.body { declare bindings / types here }
    (graph, module)
}

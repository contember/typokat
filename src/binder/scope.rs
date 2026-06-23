//! The scope graph (architecture §4, mvp-plan §4.3).
//!
//! Name resolution is modelled as a scope graph (the Visser/Delft line): a tree
//! of scopes with parent-walk resolution, giving a unified resolution model and
//! a basis for later incrementality and per-unit parallel checking.
//!
//! M0 does not resolve names (its fixtures have none), so the binder does not
//! yet build this graph. The structs exist as the M1 foundation. Marked
//! `#[allow(dead_code)]` deliberately — TODO(M1) gives them readers.

#![allow(dead_code)] // TODO(M1): the binder builds and the checker walks this.

use crate::binder::symbol::SymbolId;
use rustc_hash::FxHashMap;

/// Index of a scope within the scope graph.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ScopeId(pub u32);

/// What kind of region a scope covers. Drives hoisting and shadowing rules in
/// M1+. Only the variants M1 needs first are listed; more are added as the
/// subset grows (functions in M3, blocks for narrowing later).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScopeKind {
    /// The top-level module scope.
    Module,
    /// A function body scope. TODO(M3).
    Function,
    /// A lexical block `{ … }`. TODO(M7: needed for flow/narrowing).
    Block,
}

/// One node in the scope graph: a parent link plus the names declared directly
/// in this scope. Resolution walks `parent` until a name is found.
#[derive(Debug)]
pub struct Scope {
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub symbols: FxHashMap<String, SymbolId>,
}

impl Scope {
    pub fn new(kind: ScopeKind, parent: Option<ScopeId>) -> Self {
        Scope {
            parent,
            kind,
            symbols: FxHashMap::default(),
        }
    }
}

/// The whole scope graph for a file: scopes plus their symbols.
/// TODO(M1): `bind.rs` populates this from the AST; the checker resolves against
/// it via parent-walk.
#[derive(Default)]
pub struct ScopeGraph {
    scopes: Vec<Scope>,
}

impl ScopeGraph {
    pub fn new() -> Self {
        ScopeGraph::default()
    }

    /// Append a scope and return its id.
    pub fn push(&mut self, scope: Scope) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(scope);
        id
    }

    pub fn get(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.0 as usize)
    }
}

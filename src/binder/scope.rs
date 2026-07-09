//! The scope graph (architecture §4).
//!
//! Name resolution is modelled as a scope graph (the Visser/Delft line): a tree
//! of scopes with parent-walk resolution, giving a unified resolution model and
//! a basis for later incrementality and per-unit parallel checking.

use crate::binder::symbol::SymbolId;
use rustc_hash::FxHashMap;

/// Index of a scope within the scope graph.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ScopeId(pub u32);

impl ScopeId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// What kind of region a scope covers; drives hoisting and shadowing rules.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScopeKind {
    /// The top-level module scope.
    Module,
    /// A function/arrow body scope, holding the function's parameters.
    Function,
    /// A lexical block `{ … }`.
    #[allow(dead_code)] // Kept for lexical block scopes recorded by the binder.
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

    /// The symbol declared directly in this scope under `name`, if any (no
    /// parent walk).
    pub fn lookup_local(&self, name: &str) -> Option<SymbolId> {
        self.symbols.get(name).copied()
    }
}

/// The whole scope graph for a file: scopes plus the names they declare. The
/// binder (`bind.rs`) populates it; the checker resolves references against it
/// via [`ScopeGraph::resolve`].
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
        self.scopes.get(id.index())
    }

    pub fn get_mut(&mut self, id: ScopeId) -> Option<&mut Scope> {
        self.scopes.get_mut(id.index())
    }

    /// Declare `name → symbol` directly in `scope`; duplicate-name diagnostics are deferred.
    pub fn declare(&mut self, scope: ScopeId, name: impl Into<String>, symbol: SymbolId) -> Option<SymbolId> {
        match self.get_mut(scope) {
            Some(s) => s.symbols.insert(name.into(), symbol),
            None => None,
        }
    }

    /// Resolve `name` by walking parent links; the caller reports `TK2304` on `None`.
    pub fn resolve(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        let mut current = Some(scope);
        while let Some(id) = current {
            let s = self.get(id)?;
            if let Some(symbol) = s.lookup_local(name) {
                return Some(symbol);
            }
            current = s.parent;
        }
        None
    }
}

//! The scope graph (architecture §4).
//!
//! Name resolution follows lexical parents plus the shared public edge owned by
//! each namespace fragment, leaving room for later incrementality and per-unit
//! parallel checking.

use crate::binder::symbol::SymbolId;
use crate::types::layered::LayeredVec;
use rustc_hash::FxHashMap;

/// A refused write into a scope that belongs to the frozen library prefix.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FrozenScopeWrite;

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
    /// Shared namespace publication surface for every reopening.
    NamespacePublic,
    /// Lexical context for one namespace reopening.
    NamespacePrivate,
    /// Legal project-wide type-side augmentation surface.
    CompilationGlobal,
    /// Shared owner for top-level namespace declarations in script files.
    ScriptNamespaceRoot,
    /// Lexical view for one `declare global` body.
    GlobalOverlay,
}

/// One node in the scope graph: a lexical parent, an optional namespace-public
/// edge, and the names declared directly in this scope.
#[derive(Debug)]
pub struct Scope {
    pub parent: Option<ScopeId>,
    /// Shared public namespace surface consulted before the lexical parent.
    pub namespace_public: Option<ScopeId>,
    pub kind: ScopeKind,
    pub symbols: FxHashMap<String, SymbolId>,
}

impl Scope {
    pub fn new(kind: ScopeKind, parent: Option<ScopeId>) -> Self {
        Scope {
            parent,
            namespace_public: None,
            kind,
            symbols: FxHashMap::default(),
        }
    }

    pub fn with_namespace_public(mut self, namespace_public: ScopeId) -> Self {
        self.namespace_public = Some(namespace_public);
        self
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
    scopes: LayeredVec<Scope>,
}

impl ScopeGraph {
    pub fn new() -> Self {
        ScopeGraph::default()
    }

    /// Append a scope and return its id.
    pub fn push(&mut self, scope: Scope) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push_local(scope);
        id
    }

    pub fn get(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.index())
    }

    pub fn get_mut(&mut self, id: ScopeId) -> Option<&mut Scope> {
        self.scopes.get_mut_local(id.index())
    }

    pub(crate) fn len(&self) -> usize {
        self.scopes.len()
    }

    #[cfg(test)]
    pub(crate) fn base_len_for_test(&self) -> usize {
        self.scopes.base_len()
    }

    #[cfg(test)]
    pub(crate) fn all_scopes(&self) -> impl Iterator<Item = &Scope> {
        self.scopes.iter()
    }

    #[cfg(test)]
    pub(crate) fn local_scopes(&self) -> impl Iterator<Item = (ScopeId, &Scope)> {
        let base_len = self.scopes.base_len();
        self.scopes
            .local_iter()
            .enumerate()
            .map(move |(index, scope)| {
                let id = u32::try_from(base_len + index).expect("scope id fits u32");
                (ScopeId(id), scope)
            })
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.scopes.freeze_as_base()
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            scopes: self.scopes.fork_delta()?,
        })
    }

    #[cfg(test)]
    pub(crate) fn scopes_share_base_with(&self, other: &Self) -> bool {
        self.scopes.shares_base_with(&other.scopes)
    }

    /// Declare `name → symbol` directly in `scope`; duplicate-name diagnostics are deferred.
    ///
    /// `Err` means `scope` lives in a frozen prefix a delta may not mutate (ADR-0011). The write
    /// is refused rather than silently dropped so the caller must decide (backlog 102).
    pub fn declare(
        &mut self,
        scope: ScopeId,
        name: impl Into<String>,
        symbol: SymbolId,
    ) -> Result<Option<SymbolId>, FrozenScopeWrite> {
        match self.get_mut(scope) {
            Some(s) => Ok(s.symbols.insert(name.into(), symbol)),
            None => Err(FrozenScopeWrite),
        }
    }

    /// Resolve `name` through local, namespace-public, then lexical-parent scopes.
    pub fn resolve(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        let mut current = Some(scope);
        while let Some(id) = current {
            let s = self.get(id)?;
            if let Some(symbol) = s.lookup_local(name) {
                return Some(symbol);
            }
            if let Some(public) = s.namespace_public {
                if let Some(symbol) = self.get(public)?.lookup_local(name) {
                    return Some(symbol);
                }
            }
            current = s.parent;
        }
        None
    }

    /// Resolve `name` by walking parent links until a matching symbol satisfies
    /// `accept`. A declaration in the other namespace does not block a value/type
    /// lookup from reaching an ancestor's matching slot.
    pub fn resolve_matching<F>(&self, scope: ScopeId, name: &str, mut accept: F) -> Option<SymbolId>
    where
        F: FnMut(SymbolId) -> bool,
    {
        let mut current = Some(scope);
        while let Some(id) = current {
            let s = self.get(id)?;
            if let Some(symbol) = s.lookup_local(name) {
                if accept(symbol) {
                    return Some(symbol);
                }
            }
            if let Some(public) = s.namespace_public {
                if let Some(symbol) = self.get(public)?.lookup_local(name) {
                    if accept(symbol) {
                        return Some(symbol);
                    }
                }
            }
            current = s.parent;
        }
        None
    }

    /// The scope that owns a `var` declaration originating in `scope`.
    /// Blocks and loop heads are skipped; a function boundary keeps nested
    /// functions isolated and the module owns top-level `var`s.
    pub fn var_scope(&self, scope: ScopeId) -> Option<ScopeId> {
        let mut current = Some(scope);
        while let Some(id) = current {
            let current_scope = self.get(id)?;
            if matches!(
                current_scope.kind,
                ScopeKind::Function
                    | ScopeKind::Module
                    | ScopeKind::NamespacePrivate
                    | ScopeKind::CompilationGlobal
                    | ScopeKind::ScriptNamespaceRoot
                    | ScopeKind::GlobalOverlay
            ) {
                return Some(id);
            }
            current = current_scope.parent;
        }
        None
    }
}

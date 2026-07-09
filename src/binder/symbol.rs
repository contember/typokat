//! Multi-slot symbols (architecture §4.1).
//!
//! One symbol owns separate value/type/namespace slots, so declaration merging is
//! represented directly in the scope graph instead of by a parallel model.

/// Index of a declaration site; the checker stores declared/inferred types by `DeclId`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeclId(pub u32);

impl DeclId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index of a symbol within the binder's [`SymbolTable`].
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SymbolId(pub u32);

impl SymbolId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A name with up to three meanings, one per declaration space. tsc models this
/// with a `SymbolFlags` bitmask; we use explicit slots so the three spaces never
/// collide (architecture §4.1).
#[derive(Clone, Debug, Default)]
pub struct Symbol {
    /// The bound name.
    pub name: String,
    /// Value-space declaration (`const`/`let`/`var`/`function`/`class` value side).
    pub value: Option<DeclId>,
    /// Type-space declaration; uses the binder's separate type `DeclId` range.
    pub ty: Option<DeclId>,
    /// Namespace-space declaration (`namespace`/module).
    /// TODO(post-MVP): filled by the namespace binder.
    pub ns: Option<DeclId>,
}

impl Symbol {
    /// Create a symbol with no declarations yet bound.
    pub fn new(name: impl Into<String>) -> Self {
        Symbol {
            name: name.into(),
            value: None,
            ty: None,
            ns: None,
        }
    }
}

/// The symbol table for a file: symbols addressed by [`SymbolId`]. Scopes
/// (`scope.rs`) map a name to a `SymbolId` here; the multi-slot `Symbol` then
/// merges declarations across spaces under that one id (architecture §4.1).
#[derive(Default)]
pub struct SymbolTable {
    symbols: Vec<Symbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable::default()
    }

    /// Append a symbol and return its id.
    pub fn push(&mut self, symbol: Symbol) -> SymbolId {
        let id = SymbolId(self.symbols.len() as u32);
        self.symbols.push(symbol);
        id
    }

    pub fn get(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.index())
    }

    pub fn get_mut(&mut self, id: SymbolId) -> Option<&mut Symbol> {
        self.symbols.get_mut(id.index())
    }
}

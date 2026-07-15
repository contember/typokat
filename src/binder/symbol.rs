//! Multi-slot symbols (architecture §4.1).
//!
//! One symbol owns separate value/type/namespace slots, so declaration merging is
//! represented directly in the scope graph instead of by a parallel model.

use crate::binder::declaration::{DeclId, LegacyTypeStorageId, TypeGroupId, ValueStorageId};
use crate::binder::namespace::NamespaceId;

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
    pub value: Option<ValueStorageId>,
    /// An import whose source value is erased must hide parent value slots. Ordinary
    /// type-only declarations leave this false, so cross-space lookup still falls through.
    pub blocks_value_lookup: bool,
    /// Function declarations for this value symbol, in source order.
    ///
    /// Empty for non-function values. Overload checking uses this ordered list to
    /// separate external overload signatures from the implementation declaration
    /// while keeping the ordinary `value` slot as the current externally resolved
    /// declaration.
    pub function_values: Vec<ValueStorageId>,
    /// Legacy type-storage declaration retained until the atomic WU3 cutover.
    pub ty: Option<LegacyTypeStorageId>,
    /// Dormant ordered type declaration group; production consumers ignore it in WU1a.
    pub type_group: Option<TypeGroupId>,
    /// Namespace-space declaration (`namespace`/module).
    /// Dormant until the namespace publication cutover.
    pub ns: Option<NamespaceId>,
    /// Ordered source declarations contributing to this lexical symbol.
    /// Production lookup ignores this dormant classifier input.
    pub declarations: Vec<DeclId>,
}

impl Symbol {
    /// Create a symbol with no declarations yet bound.
    pub fn new(name: impl Into<String>) -> Self {
        Symbol {
            name: name.into(),
            value: None,
            blocks_value_lookup: false,
            function_values: Vec::new(),
            ty: None,
            type_group: None,
            ns: None,
            declarations: Vec::new(),
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

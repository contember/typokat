//! Multi-slot symbols (architecture §4.1).
//!
//! One symbol owns separate value/type/namespace slots, so declaration merging is
//! represented directly in the scope graph instead of by a parallel model.

use crate::binder::declaration::{DeclId, TypeGroupId, ValueStorageId};
use crate::binder::namespace::NamespaceId;
use crate::types::layered::LayeredVec;

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
    /// The blocked import surface never had a value slot, so value use reports TK2693.
    pub type_only_value_absence: bool,
    /// Function declarations for this value symbol, in source order.
    ///
    /// Empty for non-function values. Overload checking uses this ordered list to
    /// separate external overload signatures from the implementation declaration
    /// while keeping the ordinary `value` slot as the current externally resolved
    /// declaration.
    pub function_values: Vec<ValueStorageId>,
    /// Stable ordered type declaration group.
    pub ty: Option<TypeGroupId>,
    /// Imported group ids are references, never declaration-merge owners.
    pub owns_type_group: bool,
    /// An unavailable import must hide matching type slots in parent scopes.
    pub blocks_type_lookup: bool,
    /// An unavailable namespace root must hide matching parent namespace slots.
    pub blocks_namespace_lookup: bool,
    /// Namespace-space declaration (`namespace`/module).
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
            type_only_value_absence: false,
            function_values: Vec::new(),
            ty: None,
            owns_type_group: false,
            blocks_type_lookup: false,
            blocks_namespace_lookup: false,
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
    symbols: LayeredVec<Symbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable::default()
    }

    /// Append a symbol and return its id.
    pub fn push(&mut self, symbol: Symbol) -> SymbolId {
        let id = SymbolId(self.symbols.len() as u32);
        self.symbols.push_local(symbol);
        id
    }

    pub fn get(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.index())
    }

    pub fn get_mut(&mut self, id: SymbolId) -> Option<&mut Symbol> {
        self.symbols.get_mut_local(id.index())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn all_symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn prefix_replacements(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.prefix_replacements().map(|(_, symbol)| symbol)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_symbols(&self) -> impl Iterator<Item = (SymbolId, &Symbol)> {
        let base_len = self.symbols.base_len();
        self.symbols
            .local_iter()
            .enumerate()
            .map(move |(index, symbol)| {
                let id = u32::try_from(base_len + index).expect("symbol id fits u32");
                (SymbolId(id), symbol)
            })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn replacement_row_count_for_test(&self) -> usize {
        self.symbols.replacement_len()
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.symbols.freeze_as_base()
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            symbols: self.symbols.fork_delta()?,
        })
    }

    pub(crate) fn fork_sparse_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            symbols: self.symbols.fork_sparse_delta()?,
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn symbols_share_base_with(&self, other: &Self) -> bool {
        self.symbols.shares_base_with(&other.symbols)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, &Symbol)> {
        self.symbols.iter().enumerate().map(|(index, symbol)| {
            (
                SymbolId(u32::try_from(index).expect("symbol table index fits u32")),
                symbol,
            )
        })
    }
}

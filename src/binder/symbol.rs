//! Multi-slot symbols (architecture §4.1, mvp-plan §4.3).
//!
//! A symbol is NOT a single binding but a set of slots over **separate
//! declaration spaces** — value / type / namespace. One name can occupy several
//! (the canonical `namespace A {} interface A {} class A {}` case, and the
//! `namespace`+`interface` merges in `lib.d.ts` itself). This multiplicity is
//! built in from day 1 because a clean scope graph only carries it cleanly if
//! designed for it — retrofitting it is a rewrite (mvp-plan §1.3).
//!
//! M1 only ever fills the **value** slot (`const`/`let`/`var`); the `ty`/`ns`
//! slots exist so M2+ (`interface`/`type`/`namespace`) fill them without any
//! structural change.

/// Index of a declaration site (an AST node). A value declaration's `DeclId`
/// keys the checker's `DeclId → TypeId` table — the seam where a symbol's
/// declared/inferred type is looked up (architecture §4.1: the type lives with
/// the declaration, not the symbol). A newtype so the declaration-space slots
/// are typed.
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
    /// Value-space declaration (`const`/`let`/`var`/`function`/`class` value
    /// side). The only slot M1 fills.
    pub value: Option<DeclId>,
    /// Type-space declaration (`interface`/`type`/`class` type side).
    /// TODO(M2/M5): filled by the type-declaration binder.
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

//! Multi-slot symbols (architecture §4.1, mvp-plan §4.3).
//!
//! A symbol is NOT a single binding but a set of slots over **separate
//! declaration spaces** — value / type / namespace. One name can occupy several
//! (the canonical `namespace A {} interface A {} class A {}` case, and the
//! `namespace`+`interface` merges in `lib.d.ts` itself). This multiplicity is
//! built in from day 1 because a clean scope graph only carries it cleanly if
//! designed for it — retrofitting it is a rewrite (mvp-plan §1.3).
//!
//! M0 has no name references in its fixtures, so the binder does not yet run;
//! these types exist so M1 fills them in without structural change. Marked
//! `#[allow(dead_code)]` deliberately — TODO(M1) removes the attribute as the
//! fields gain readers.

#![allow(dead_code)] // TODO(M1): binder + name resolution populate/read these.

use crate::types::store::TypeId;

/// Index of a declaration site (an AST node). Resolved against a side table the
/// binder builds in M1. A newtype so the declaration-space slots are typed.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeclId(pub u32);

/// Index of a symbol within the binder's symbol table.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SymbolId(pub u32);

/// A name with up to three meanings, one per declaration space. tsc models this
/// with a `SymbolFlags` bitmask; we use explicit slots so the three spaces never
/// collide (architecture §4.1).
#[derive(Clone, Debug, Default)]
pub struct Symbol {
    /// The bound name.
    pub name: String,
    /// Value-space declaration (`const`/`let`/`function`/`class` value side).
    pub value: Option<DeclId>,
    /// Type-space declaration (`interface`/`type`/`class` type side).
    pub ty: Option<DeclId>,
    /// Namespace-space declaration (`namespace`/module).
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

    /// The resolved type of this symbol's value declaration, once the checker
    /// has assigned one. TODO(M1): the checker stores inferred/annotated types
    /// in a parallel table keyed by `DeclId`; this is the lookup seam.
    pub fn value_type(&self) -> Option<TypeId> {
        // M1 wires DeclId -> TypeId resolution here.
        None
    }
}

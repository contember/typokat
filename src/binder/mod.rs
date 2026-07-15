//! Binder layer (architecture §4): scope graph with multi-slot symbols.
//!
//! Symbols keep separate value/type/namespace slots for declaration merging.
//! Resolution is parent-walk over the scope graph; the checker chooses the slot.

pub mod bind;
pub mod declaration;
pub mod scope;
pub mod symbol;

pub use bind::{bind_module_with_prelude, Binder};

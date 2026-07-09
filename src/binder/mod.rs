//! Binder layer (architecture §4): scope graph with multi-slot symbols.
//!
//! Symbols keep separate value/type/namespace slots because declaration merging is
//! a design requirement, not a special case. Resolution is parent-walk over the
//! scope graph; the checker chooses the slot it needs.

pub mod bind;
pub mod scope;
pub mod symbol;

pub use bind::{bind_module_with_prelude, Binder};

//! Binder layer (architecture §4): scope graph with multi-slot symbols.
//!
//! The multi-slot `Symbol` (value/type/namespace spaces) is built from day 1
//! because declaration merging is a design requirement, not a special case, and
//! retrofitting the multiplicity is a rewrite (mvp-plan §1.3, §4.3).
//!
//! M1 builds the module scope, binds value declarations into the value slot, and
//! exposes parent-walk resolution that the checker uses for name lookup. The
//! `ty`/`ns` slots and nested scopes are populated by later milestones.

pub mod bind;
pub mod scope;
pub mod symbol;

pub use bind::{bind_module_with_prelude, Binder};

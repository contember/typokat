//! Binder layer (architecture §4): scope graph with multi-slot symbols.
//!
//! Symbols keep separate value/type/namespace slots for declaration merging.
//! Resolution is parent-walk over the scope graph; the checker chooses the slot.

pub mod bind;
pub mod declaration;
pub mod namespace;
pub mod product;
pub mod scope;
pub mod symbol;

#[cfg(any(test, feature = "test-utils"))]
pub mod references;
pub mod roots;

pub use bind::{bind_module_with_prelude, Binder};

#[cfg(test)]
#[path = "spec/attach_declaration_scaling_spec.rs"]
mod attach_declaration_scaling_spec;

#[cfg(test)]
#[path = "spec/type_group_attach_scaling_spec.rs"]
mod type_group_attach_scaling_spec;

#[cfg(test)]
#[path = "spec/exact_declaration_site_spec.rs"]
mod exact_declaration_site_spec;

#[cfg(test)]
#[path = "spec/compilation_origin_spec.rs"]
mod compilation_origin_spec;

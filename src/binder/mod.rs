//! Binder layer (architecture §4): scope graph with multi-slot symbols.
//!
//! The multi-slot `Symbol` (value/type/namespace spaces) is built from day 1
//! because declaration merging is a design requirement, not a special case, and
//! retrofitting the multiplicity is a rewrite (mvp-plan §1.3, §4.3).
//!
//! M0 performs no binding (its fixtures have no references); these modules are
//! the M1 foundation.

pub mod bind;
pub mod scope;
pub mod symbol;

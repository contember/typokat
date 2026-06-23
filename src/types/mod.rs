//! Type store layer (architecture §3): SoA arena, hash-consing interner,
//! pluggable structural hash, type representation.
//!
//! This is "the foundation, not an optimization" — built correct-from-day-1
//! because retrofitting the arena/interner shape is a rewrite (mvp-plan §1.3).

pub mod hash;
pub mod intern;
pub mod repr;
pub mod store;

pub use intern::{Interner, WellKnown};
pub use repr::{IntrinsicKind, LiteralValue};
pub use store::TypeId;

//! Type store layer: SoA arena, hash-consing interner, structural hashing, and
//! type representation.

pub mod hash;
pub mod intern;
pub mod repr;
pub mod store;
pub mod substitute;

pub use intern::{Interner, WellKnown};
pub use repr::{ClassId, GenericTypeParam, IntrinsicKind, LiteralValue, TypeParamId, Visibility};
pub use store::TypeId;
pub use substitute::{instantiate_function, substitute, Substitution};

//! Type store layer: SoA arena, hash-consing interner, structural hashing, and
//! type representation.

pub mod hash;
pub mod intern;
pub(crate) mod layered;
pub mod repr;
pub mod store;
pub mod substitute;

pub use intern::{Interner, WellKnown};
pub use repr::{ClassId, GenericTypeParam, IntrinsicKind, LiteralValue, TypeParamId, Visibility};
pub use store::TypeId;
pub use substitute::{instantiate_function, substitute, Substitution};
#[cfg(test)]
pub(crate) use substitute::{
    start_substitution_run_visit_measure, substitution_run_visit_measure,
    SubstitutionRunVisitMeasure,
};
pub(crate) use substitute::{substitute_with_outcome, SubstitutionOutcome};

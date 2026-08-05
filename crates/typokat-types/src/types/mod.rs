//! Type store layer: SoA arena, hash-consing interner, structural hashing, and
//! type representation.

pub mod hash;
pub mod intern;
pub mod layered;
pub mod repr;
pub mod store;
pub mod substitute;

pub use intern::{
    DeclaredMaterializationError, DerivationEdge, DerivationId, DerivedType, Interner, WellKnown,
};
pub use repr::{
    ClassId, GenericTypeParam, IntrinsicKind, LiteralValue, PropertyKey, TypeParamId, Visibility,
    WellKnownSymbol,
};
pub use store::TypeId;
pub use substitute::{
    derive_substitution_result, instantiate_function, substitute, substitute_derived,
    substitute_derived_with_outcome, substitute_with_outcome, DerivedSubstitutionOutcome,
    Substitution, SubstitutionOutcome,
};
#[cfg(any(test, feature = "test-utils"))]
pub use substitute::{
    start_substitution_run_visit_measure, substitution_run_visit_measure,
    SubstitutionRunVisitMeasure,
};

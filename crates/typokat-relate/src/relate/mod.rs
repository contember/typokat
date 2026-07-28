//! Relation engine (architecture §6): assignability/subtyping with a durable cache,
//! assume-true cycle handling, and structured failure reasons.

pub mod cache;
pub mod relation;

pub use relation::{Reason, ReasonChain, Relation, RelationKind};
pub use relation::{
    Relater, RelationAttempt, RelationDemand, RelationNormalization, RelationOutcome,
};

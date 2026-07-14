//! Relation engine (architecture §6): assignability/subtyping with a durable cache,
//! assume-true cycle handling, and structured failure reasons.

pub mod cache;
pub mod relation;

pub use relation::{Reason, ReasonChain, Relater, Relation, RelationKind};
pub(crate) use relation::{
    RelationAttempt, RelationDemand, RelationNormalization, RelationOutcome,
};

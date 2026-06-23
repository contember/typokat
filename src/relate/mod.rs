//! Relation engine (architecture §6): subtyping/assignability with caches and
//! cycle handling. The largest CPU consumer in a real checker, so the cache
//! (`cache.rs`), the cycle stack, and the reason chain (`relation.rs`) are built
//! correct-from-day-1 (mvp-plan §1.3).

pub mod cache;
pub mod relation;

pub use relation::{Reason, ReasonChain, Relater, Relation, RelationKind};

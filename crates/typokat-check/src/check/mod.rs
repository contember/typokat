//! Statement-level checker (architecture §5): a structural interpreter close to
//! `checker.ts` in shape, driving inference, flow/narrowing, and the relation
//! engine.

pub mod checker;
pub mod flow;
pub mod infer;
pub mod query;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support;

pub use checker::CheckResult;

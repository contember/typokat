//! Statement-level checker (architecture §5): a structural interpreter close to
//! `checker.ts` in shape, driving inference, flow/narrowing, and the relation
//! engine.

pub mod checker;
pub mod flow;
pub mod infer;
pub(crate) mod query;
#[cfg(test)]
pub(crate) mod test_support;

pub(crate) use checker::check_project_programs;
pub use checker::{check_program, CheckResult};

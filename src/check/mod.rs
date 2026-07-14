//! Statement-level checker (architecture §5): a structural interpreter close to
//! `checker.ts` in shape, driving inference, flow/narrowing, and the relation
//! engine.

pub mod checker;
pub mod flow;
pub mod infer;
pub(crate) mod query;

pub use checker::{
    check_program, check_project_programs, CheckResult, ProjectImport, ProjectImportSource,
    ProjectProgram,
};

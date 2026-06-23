//! Statement-level checker (architecture §5): a structural interpreter close to
//! `checker.ts` in shape (the `stc` philosophy), driving the relation engine and
//! (later) the type-level VM.
//!
//! `infer.rs` and `flow.rs` are documented foundation stubs — inference (§5.1)
//! and flow/narrowing (§5) are first-class concerns the checker is structured
//! around from day 1, even though M0 implements neither.

pub mod checker;
pub mod flow;
pub mod infer;

pub use checker::check_program;

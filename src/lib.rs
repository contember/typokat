//! typokat — a from-scratch TypeScript type checker in Rust.
//!
//! Library crate exposing the checker pipeline so it can be driven both by the
//! `typokat` binary (`main.rs`) and by the conformance harness
//! (`tests/conformance.rs`). The module layout mirrors the architecture layers
//! 1:1 (mvp-plan §3), so promotion to a cargo workspace in Phase 2 is a
//! mechanical split, not a redesign.

pub mod binder;
pub mod check;
pub mod diagnostics;
pub mod driver;
pub mod relate;
pub mod span;
pub mod types;

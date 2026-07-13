//! typokat — a from-scratch TypeScript type checker in Rust.
//!
//! Library crate for the CLI and conformance harness; module layout mirrors the
//! architecture layers.

pub mod binder;
pub mod check;
#[allow(dead_code)] // Dormant ADR-0006 domain; consumers switch in WU1b-WU1d.
mod class_semantics;
pub mod diagnostics;
pub mod driver;
pub mod relate;
pub mod span;
pub mod surface;
pub mod types;

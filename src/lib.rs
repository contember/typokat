//! typokat — a from-scratch TypeScript type checker in Rust.
//!
//! Library crate for the CLI and conformance harness; module layout mirrors the
//! architecture layers.

pub mod binder;
pub mod check;
mod class_semantics;
pub mod diagnostics;
pub mod driver;
pub mod relate;
#[cfg(test)]
pub(crate) mod snapshot_codec;
mod source;
pub mod span;
pub mod surface;
pub mod types;

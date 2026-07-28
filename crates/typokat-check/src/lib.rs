//! Statement checking, flow analysis, and type inference.

pub(crate) use typokat_binder::binder;
pub(crate) use typokat_core::{source, span};
pub(crate) use typokat_diagnostics::diagnostics;
pub(crate) use typokat_frontend::frontend;
pub(crate) use typokat_relate::relate;
pub(crate) use typokat_types::{class_semantics, types};

pub mod check;

#[cfg(test)]
pub(crate) use typokat_core::test_support;

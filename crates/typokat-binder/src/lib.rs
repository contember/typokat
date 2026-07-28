//! TypeScript declaration binding and namespace construction.

pub mod binder;
pub(crate) use typokat_core::source;
pub use typokat_core::span;
pub use typokat_types::types;

#[cfg(test)]
pub(crate) use typokat_core::test_support;

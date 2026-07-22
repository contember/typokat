pub mod artifact;
pub mod compiler;
pub mod profile;

pub use crate::source::LibraryFileOrdinal;

#[cfg(test)]
mod wu2_spec;

// Activate after the production strict decoder and immutable base provider exist.
// #[cfg(test)]
// mod snapshot_base_spec;

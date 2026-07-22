pub mod artifact;
mod base;
pub mod compiler;
pub mod profile;
mod provider;
mod snapshot;

pub use crate::source::LibraryFileOrdinal;
pub use base::FrozenLibraryBase;
pub use provider::{
    LibraryBaseProvider, LibraryInitCause, LibraryInitError, LibraryInitStage,
    LibrarySnapshotViolation,
};

#[cfg(test)]
mod artifact_spec;

#[cfg(test)]
mod snapshot_base_spec;

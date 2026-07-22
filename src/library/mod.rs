pub mod artifact;
mod base;
pub mod compiler;
pub mod profile;
mod provider;
mod snapshot;

pub use crate::source::LibraryFileOrdinal;
/// Immutable canonical library state shared by user checks.
///
/// The mutable user delta is deliberately not part of the library API.
///
/// ```compile_fail
/// use typokat::library::LayeredUserDelta;
/// ```
pub use base::FrozenLibraryBase;
pub use provider::{
    LibraryBaseProvider, LibraryInitCause, LibraryInitError, LibraryInitStage,
    LibrarySnapshotViolation,
};

pub(crate) struct CollisionFreeUserDeltaCapability(());

#[cfg(test)]
mod artifact_spec;

#[cfg(test)]
mod snapshot_base_spec;

#[cfg(test)]
mod user_delta_spec;

#[cfg(test)]
mod user_delta_project_scale_spec;

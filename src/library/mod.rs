mod base;
mod collision_preflight;
pub mod compiler;
pub mod profile;
mod provider;

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
    LibraryProjectBinderContinuation,
};

pub(crate) use collision_preflight::CollisionFreeUserDeltaCapability;

#[cfg(test)]
mod user_delta_spec;

#[cfg(test)]
mod user_delta_project_scale_spec;

#[cfg(test)]
mod collision_preflight_spec;

// Activate after the WU5 private combined-universe implementation lands.
// #[cfg(test)]
// mod private_combined_universe_spec;

#[cfg(test)]
mod collision_replay_index_spec;

#[cfg(test)]
mod collision_epoch_scheduler_spec;

// Activate after the WU5 replay index and work receipts land.
// #[cfg(test)]
// mod private_replay_scale_spec;

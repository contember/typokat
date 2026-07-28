mod base;
mod collision_preflight;
pub mod compiler;
pub mod profile;
mod provider;
pub mod records;

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
/// The library's own records are inspectable only by asking for them (ADR-0018); no base,
/// provider, or check retains one.
pub use records::{
    LibraryRecordCensus, LibraryRecordCensusDifference, LibraryRecordEntry, LibraryRecordKind,
};

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

#[cfg(test)]
mod compiler_profile_spec;

// Activate after the WU5 replay index and work receipts land.
// #[cfg(test)]
// mod private_replay_scale_spec;

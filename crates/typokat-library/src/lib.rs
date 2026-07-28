mod base;
mod collision_preflight;
pub mod compiler;
pub mod profile;
mod provider;
pub mod records;

pub(crate) use typokat_binder::binder;
pub(crate) use typokat_check::check;
pub(crate) use typokat_core::{source, span};
pub(crate) use typokat_frontend::frontend;

#[cfg(test)]
pub(crate) use typokat_core::test_support;
#[cfg(test)]
pub(crate) use typokat_diagnostics::diagnostics;
#[cfg(test)]
pub(crate) use typokat_relate::relate;
#[cfg(test)]
pub(crate) use typokat_types::types;

pub use crate::source::LibraryFileOrdinal;
/// Immutable canonical library state shared by user checks.
///
/// The mutable user delta is deliberately not part of the library API.
///
/// ```compile_fail
/// use typokat_library::LayeredUserDelta;
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

/// Stable public identity of the packaged default-library source profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddedLibraryProfileMetadata {
    profile: profile::ExactLibraryProfileMetadata,
}

impl EmbeddedLibraryProfileMetadata {
    pub const fn profile_identity(self) -> &'static str {
        self.profile.profile_identity()
    }

    pub const fn file_count(self) -> usize {
        self.profile.file_count()
    }
}

pub const fn embedded_library_profile_metadata() -> EmbeddedLibraryProfileMetadata {
    EmbeddedLibraryProfileMetadata {
        profile: profile::ExactLibraryProfile::packaged_metadata(),
    }
}

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

#[cfg(test)]
mod type_group_ordering_spec;

// Activate after the WU5 replay index and work receipts land.
// #[cfg(test)]
// mod private_replay_scale_spec;

// Activate with ADR-0020's source-compiled compact collision plan.
// #[cfg(test)]
// mod source_compiled_collision_plan_spec;

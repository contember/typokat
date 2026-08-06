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
pub use base::{
    compile_complete_source_fallback_runtime, CompleteSourceFallbackRuntime, FrozenLibraryBase,
    LibraryProjectRouteError, RoutedLibraryProject, RoutedPrivateExecution,
    RoutedPrivateLibraryProject,
};
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

/// Load the exact packaged source profile for a same-run complete-source compiler.
#[doc(hidden)]
pub fn packaged_library_source_inputs(
) -> Result<Vec<typokat_frontend::frontend::AuxiliarySourceInput>, LibraryInitError> {
    typokat_check::check::checker::library_compiler::record_complete_source_profile_load_for_test();
    let profile = profile::ExactLibraryProfile::load_packaged().map_err(|error| {
        LibraryInitError::new(
            LibraryInitStage::ProfileLoad,
            LibraryInitCause::ProfileRejected {
                message: error.to_string(),
            },
        )
    })?;
    profile
        .sources()
        .iter()
        .map(|source| {
            let text = std::str::from_utf8(source.bytes()).map_err(|_| {
                LibraryInitError::new(
                    LibraryInitStage::ProfileLoad,
                    LibraryInitCause::ProfileRejected {
                        message: format!("library source {:?} is not UTF-8", source.name()),
                    },
                )
            })?;
            Ok(typokat_frontend::frontend::AuxiliarySourceInput {
                source_ordinal: source.ordinal().index(),
                name: source.name().to_owned(),
                source: text.to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod user_delta_spec;

#[cfg(test)]
mod user_delta_project_scale_spec;

#[cfg(test)]
mod collision_preflight_spec;

#[cfg(test)]
mod private_route_receipt_spec;

#[cfg(test)]
mod private_combined_universe_spec;

#[cfg(test)]
mod collision_replay_index_spec;

#[cfg(test)]
mod collision_epoch_scheduler_spec;

#[cfg(test)]
mod compiler_profile_spec;

#[cfg(test)]
mod type_group_ordering_spec;

#[cfg(test)]
mod private_replay_scale_spec;

#[cfg(test)]
mod source_compiled_collision_plan_spec;

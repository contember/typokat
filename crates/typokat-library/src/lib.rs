#[cfg(any(test, feature = "library-record-census",))]
mod base;
#[cfg(any(test, feature = "library-record-census",))]
mod collision_preflight;
#[cfg(any(
    test,
    feature = "library-record-census",
    feature = "project-binder-probe"
))]
pub mod compiler;
mod declaration_product;
pub mod profile;
#[cfg(feature = "project-binder-probe")]
pub mod project_binder_probe;
#[cfg(any(test, feature = "project-binder-probe"))]
mod project_binder_projection;
mod provider;
#[cfg(feature = "library-record-census")]
pub mod records;

pub(crate) use typokat_binder::binder;
#[cfg(any(
    test,
    feature = "library-record-census",
    feature = "project-binder-probe"
))]
pub(crate) use typokat_check::check;
pub(crate) use typokat_core::source;
#[cfg(any(test, feature = "library-record-census"))]
pub(crate) use typokat_core::span;
#[cfg(any(
    test,
    feature = "library-record-census",
    feature = "project-binder-probe"
))]
pub(crate) use typokat_frontend::frontend;

#[cfg(test)]
pub(crate) use typokat_core::test_support;
#[cfg(test)]
pub(crate) use typokat_diagnostics::diagnostics;
#[cfg(any(test, feature = "project-binder-probe"))]
pub(crate) use typokat_relate::relate;
#[cfg(any(test, feature = "project-binder-probe"))]
pub(crate) use typokat_types::types;

pub use crate::source::LibraryFileOrdinal;
/// Immutable canonical library state shared by user checks.
///
/// The mutable user delta is deliberately not part of the library API.
///
/// ```compile_fail
/// use typokat_library::LayeredUserDelta;
/// ```
#[cfg(any(test, feature = "library-record-census",))]
pub use base::{
    compile_complete_source_fallback_runtime, CompleteSourceFallbackRuntime, FrozenLibraryBase,
    LibraryProjectRouteError, RoutedLibraryProject, RoutedPrivateExecution,
    RoutedPrivateLibraryProject,
};
#[cfg(any(test, feature = "library-record-census",))]
pub use provider::LibraryProjectBinderContinuation;
pub use provider::{LibraryBaseProvider, LibraryInitCause, LibraryInitError, LibraryInitStage};
/// The library's own records are inspectable only by asking for them (ADR-0018); no base,
/// provider, or check retains one.
#[cfg(feature = "library-record-census")]
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

#[cfg(any(test, feature = "library-record-census",))]
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
#[path = "spec/user_delta_spec.rs"]
mod user_delta_spec;

#[cfg(test)]
#[path = "spec/user_delta_project_scale_spec.rs"]
mod user_delta_project_scale_spec;

#[cfg(test)]
#[path = "spec/collision_preflight_spec.rs"]
mod collision_preflight_spec;

#[cfg(test)]
#[path = "spec/private_route_receipt_spec.rs"]
mod private_route_receipt_spec;

#[cfg(test)]
#[path = "spec/private_combined_universe_spec.rs"]
mod private_combined_universe_spec;

#[cfg(test)]
#[path = "spec/collision_replay_index_spec.rs"]
mod collision_replay_index_spec;

#[cfg(test)]
#[path = "spec/collision_epoch_scheduler_spec.rs"]
mod collision_epoch_scheduler_spec;

#[cfg(test)]
#[path = "spec/compiler_profile_spec.rs"]
mod compiler_profile_spec;

#[cfg(test)]
#[path = "spec/library_declaration_product_spec.rs"]
mod library_declaration_product_spec;

#[cfg(test)]
#[path = "spec/library_declaration_product_integration_spec.rs"]
mod library_declaration_product_integration_spec;

#[cfg(test)]
#[path = "spec/project_local_declaration_universe_spec.rs"]
mod project_local_declaration_universe_spec;

#[cfg(test)]
#[path = "spec/project_local_semantic_universe_spec.rs"]
mod project_local_semantic_universe_spec;

#[cfg(test)]
#[path = "spec/type_group_ordering_spec.rs"]
mod type_group_ordering_spec;

#[cfg(test)]
#[path = "spec/private_replay_scale_spec.rs"]
mod private_replay_scale_spec;

#[cfg(test)]
#[path = "spec/source_compiled_collision_plan_spec.rs"]
mod source_compiled_collision_plan_spec;

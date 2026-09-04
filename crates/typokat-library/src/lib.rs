mod declaration_product;
pub mod profile;
#[cfg(feature = "project-binder-probe")]
pub mod project_binder_probe;
#[cfg(feature = "project-binder-probe")]
mod project_binder_projection;
mod provider;
#[cfg(feature = "library-record-census")]
pub mod records;

pub(crate) use typokat_binder::binder;
#[cfg(any(feature = "library-record-census", feature = "project-binder-probe"))]
pub(crate) use typokat_check::check;
pub(crate) use typokat_core::source;
#[cfg(feature = "library-record-census")]
pub(crate) use typokat_core::span;
#[cfg(any(feature = "library-record-census", feature = "project-binder-probe"))]
pub(crate) use typokat_frontend::frontend;
#[cfg(feature = "project-binder-probe")]
pub(crate) use typokat_relate::relate;
#[cfg(feature = "project-binder-probe")]
pub(crate) use typokat_types::types;

#[cfg(all(test, feature = "library-record-census"))]
pub(crate) use typokat_diagnostics::diagnostics;

pub use crate::source::LibraryFileOrdinal;
pub use provider::{LibraryBaseProvider, LibraryInitCause, LibraryInitError, LibraryInitStage};

/// The library's own records are inspectable only by asking for them; no provider or check retains
/// a census.
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

#[cfg(test)]
#[path = "spec/library_declaration_product_spec.rs"]
mod library_declaration_product_spec;

#[cfg(test)]
#[path = "spec/library_declaration_product_integration_spec.rs"]
mod library_declaration_product_integration_spec;

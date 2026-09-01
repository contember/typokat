//! Namespace, merge, global-augmentation, and UMD context metadata.
//!
//! Lexical namespace declarations remain independent of checker storage. Standalone groups keep
//! dormant owner slots in dense side columns, while admitted class/function augmentations reuse
//! the ordinary value binder for exported members.

mod classification;
mod deferred;
mod layered;
mod lookup;
mod metadata;
mod models_ids;
mod product;
mod values;
mod walking;

pub use layered::NamespaceTable;
#[allow(unused_imports)]
#[cfg(any(test, feature = "test-utils"))]
pub(crate) use layered::{NamespacePrimaryRows, NamespaceReferenceOffsets, NamespaceReferenceRows};
pub(crate) use metadata::collect_project_namespace_metadata;
pub(super) use metadata::{
    collect_namespace_metadata, fill_namespace_value_attachments, finalize_namespace_metadata,
    NamespaceMetadataRoot,
};
#[allow(unused_imports)]
pub(crate) use models_ids::NameId;
pub use models_ids::*;
pub(in crate::binder) use product::instantiate_library_declaration_product;
pub use product::{
    LibraryProductBinderMap, LibraryProductBinderMapError, LibraryProductDeclarationBinding,
    LibraryProductGroupBinding, LibraryProductInstantiationError, LibraryProductRegionBinding,
    LibraryProductSourceBinding,
};
#[cfg(test)]
pub(crate) use product::{NamespaceRegionLookupWorkForTest, NamespaceRegionLookupWorkScopeForTest};
#[allow(unused_imports)]
pub(crate) use values::NamespaceValueAttachmentPlan;
pub(super) use values::{
    allocate_dormant_namespace_value_storages, plan_namespace_value_attachments,
    replay_namespace_value_attachments,
};

#[cfg(test)]
mod tests;

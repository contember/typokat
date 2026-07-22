//! Immutable semantic state decoded from the canonical default-library snapshot.

#[cfg(test)]
use super::provider::LibraryInitError;
#[cfg(test)]
use super::snapshot::map_snapshot_error;
use super::snapshot::DecodedCanonicalLibrary;
use crate::check::checker::library_compiler::OwnedLibraryRuntimeState;
#[cfg(test)]
use crate::check::checker::library_snapshot_codec::{
    recompute_runtime_projection, validate_runtime_references, RuntimeProjectionForTest,
};
use std::collections::BTreeSet;
use std::fmt;

#[cfg(test)]
const COMPONENT_NAMES: [&str; 10] = [
    "store",
    "interner",
    "binder",
    "declaration-types",
    "published-types",
    "namespace-terminals",
    "class-metadata",
    "semantic-identities",
    "root-name-index",
    "id-prefixes",
];

#[cfg(test)]
const RUNTIME_FAMILIES: [&str; 10] = [
    "store",
    "interner",
    "binder",
    "decl-types",
    "published-types",
    "namespace-terminals",
    "class-metadata",
    "semantic-identities",
    "root-name-index",
    "next-ids",
];

#[cfg(test)]
const PROJECTION_SUBTABLES: [&str; 31] = [
    "store.rows",
    "store.payload-tables",
    "store.type-param-constraints",
    "store.frozen-type-params",
    "store.template-names",
    "interner.dedup-buckets",
    "interner.reserved-terminals",
    "interner.well-known",
    "binder.scopes",
    "binder.symbols",
    "binder.declarations",
    "binder.declaration-site-index",
    "binder.type-groups",
    "binder.namespaces",
    "binder.namespace-indexes",
    "binder.module-sources",
    "decl-types.slots",
    "published-types.groups",
    "published-types.classes",
    "namespace-terminals",
    "function-groups.symbols",
    "class.application-parameters",
    "class.parameter-defaults",
    "class.parents",
    "class.names",
    "class.new-metadata",
    "class.value-identities",
    "class.aliases",
    "semantic-identities",
    "root-name-index.entries",
    "next-ids",
];

pub struct FrozenLibraryBase {
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "retained for decoded-base user checking")
    )]
    runtime: OwnedLibraryRuntimeState,
    root_names: BTreeSet<String>,
    prefixes: FrozenLibraryPrefixes,
    identity: FrozenLibraryIdentity,
}

impl fmt::Debug for FrozenLibraryBase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrozenLibraryBase")
            .field("root_name_count", &self.root_names.len())
            .field("prefixes", &self.prefixes)
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl FrozenLibraryBase {
    pub(super) fn from_decoded(decoded: DecodedCanonicalLibrary) -> Self {
        let DecodedCanonicalLibrary {
            runtime,
            root_names,
            prefixes,
            typed_validation_sha256: _,
            identity,
        } = decoded;
        Self {
            runtime,
            root_names,
            prefixes: FrozenLibraryPrefixes::from_array(prefixes),
            identity,
        }
    }

    #[cfg(test)]
    pub(super) const fn identity(&self) -> &FrozenLibraryIdentity {
        &self.identity
    }

    #[cfg(test)]
    pub(super) fn inventory_for_test(&self) -> FrozenLibraryInventory<'_> {
        FrozenLibraryInventory { base: self }
    }

    #[cfg(test)]
    pub(super) fn root_names_for_test(&self) -> &BTreeSet<String> {
        &self.root_names
    }

    #[cfg(test)]
    pub(super) fn prefixes_for_test(&self) -> &FrozenLibraryPrefixes {
        &self.prefixes
    }

    #[cfg(test)]
    pub(super) fn type_count_for_test(&self) -> usize {
        self.runtime.type_count()
    }

    #[cfg(test)]
    pub(super) fn recompute_canonical_projection_for_test(
        &self,
    ) -> Result<CanonicalLibraryProjection, LibraryInitError> {
        recompute_runtime_projection(&self.runtime)
            .map(CanonicalLibraryProjection::new)
            .map_err(map_snapshot_error)
    }

    #[cfg(test)]
    pub(super) fn validate_frozen_reference_boundaries_for_test(
        &self,
    ) -> Result<FrozenReferenceBoundarySummary, LibraryInitError> {
        validate_runtime_references(&self.runtime)
            .map(|summary| FrozenReferenceBoundarySummary {
                checked: summary.checked,
                outside_frozen_prefix: summary.outside_frozen_prefix,
                base_to_delta: summary.base_to_delta,
                untyped_or_unowned: summary.untyped_or_unowned,
            })
            .map_err(map_snapshot_error)
    }

    #[cfg(test)]
    pub(super) const fn retained_source_bytes_for_test(&self) -> usize {
        0
    }

    #[cfg(test)]
    pub(super) const fn retained_archive_bytes_for_test(&self) -> usize {
        0
    }

    #[cfg(test)]
    pub(super) const fn retained_projection_witnesses_for_test(&self) -> usize {
        0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FrozenLibraryIdentity {
    profile_sha256: &'static str,
    schema_sha256: &'static str,
    artifact_sha256: &'static str,
    artifact_bytes: usize,
}

impl FrozenLibraryIdentity {
    pub(super) const fn canonical() -> Self {
        Self {
            profile_sha256: super::snapshot::PROFILE_SHA256,
            schema_sha256: super::snapshot::SCHEMA_SHA256,
            artifact_sha256: super::artifact::CANONICAL_SNAPSHOT_SHA256,
            artifact_bytes: super::artifact::CANONICAL_SNAPSHOT_BYTES,
        }
    }

    #[cfg(test)]
    pub(super) const fn profile_sha256(&self) -> &'static str {
        self.profile_sha256
    }

    #[cfg(test)]
    pub(super) const fn schema_sha256(&self) -> &'static str {
        self.schema_sha256
    }

    #[cfg(test)]
    pub(super) const fn artifact_sha256(&self) -> &'static str {
        self.artifact_sha256
    }

    #[cfg(test)]
    pub(super) const fn artifact_bytes(&self) -> usize {
        self.artifact_bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FrozenLibraryPrefixes {
    pub(super) types: usize,
    pub(super) type_params: usize,
    pub(super) classes: usize,
    pub(super) scopes: usize,
    pub(super) symbols: usize,
    pub(super) declarations: usize,
    pub(super) type_groups: usize,
    pub(super) namespaces: usize,
    pub(super) value_storages: usize,
}

impl FrozenLibraryPrefixes {
    fn from_array(values: [usize; 9]) -> Self {
        let [types, type_params, classes, scopes, symbols, declarations, type_groups, namespaces, value_storages] =
            values;
        Self {
            types,
            type_params,
            classes,
            scopes,
            symbols,
            declarations,
            type_groups,
            namespaces,
            value_storages,
        }
    }
}

#[cfg(test)]
pub(super) struct FrozenLibraryInventory<'base> {
    base: &'base FrozenLibraryBase,
}

#[cfg(test)]
impl FrozenLibraryInventory<'_> {
    pub(super) fn source_file_count(&self) -> u32 {
        self.base.runtime.source_file_count()
    }

    pub(super) fn reference_count(&self) -> u64 {
        validate_runtime_references(&self.base.runtime)
            .map(|summary| summary.checked)
            .unwrap_or(0)
    }

    pub(super) const fn runtime_family_count(&self) -> usize {
        RUNTIME_FAMILIES.len()
    }

    pub(super) const fn projection_subtable_count(&self) -> usize {
        PROJECTION_SUBTABLES.len()
    }

    pub(super) const fn component_names(&self) -> [&'static str; 10] {
        COMPONENT_NAMES
    }

    pub(super) fn root_name_count(&self) -> usize {
        self.base.root_names.len()
    }

    pub(super) const fn prefixes(&self) -> &FrozenLibraryPrefixes {
        &self.base.prefixes
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CanonicalLibraryProjection {
    projection: RuntimeProjectionForTest,
    prefixes: FrozenLibraryPrefixes,
}

#[cfg(test)]
impl CanonicalLibraryProjection {
    pub(super) fn new(projection: RuntimeProjectionForTest) -> Self {
        Self {
            prefixes: FrozenLibraryPrefixes::from_array(projection.prefixes_for_library()),
            projection,
        }
    }

    pub(super) const fn runtime_families(&self) -> &[&'static str; 10] {
        &RUNTIME_FAMILIES
    }

    pub(super) const fn subtables(&self) -> &[&'static str; 31] {
        &PROJECTION_SUBTABLES
    }

    pub(super) fn reference_family_counts(&self) -> [u64; 9] {
        self.projection.reference_family_counts_for_library()
    }

    pub(super) fn root_names(&self) -> &BTreeSet<String> {
        self.projection.root_names_for_library()
    }

    pub(super) const fn prefixes(&self) -> &FrozenLibraryPrefixes {
        &self.prefixes
    }

    pub(super) fn typed_validation_sha256(&self) -> String {
        self.projection.typed_validation_sha256_for_library()
    }
}

#[cfg(test)]
pub(super) struct FrozenReferenceBoundarySummary {
    pub(super) checked: u64,
    pub(super) outside_frozen_prefix: u64,
    pub(super) base_to_delta: u64,
    pub(super) untyped_or_unowned: u64,
}

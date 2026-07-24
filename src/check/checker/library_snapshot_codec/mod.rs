//! Canonical typed-state snapshot codec with test-only generation and evidence support.

#[cfg(test)]
pub(crate) mod profile;
#[cfg(test)]
mod runtime;
#[cfg(test)]
mod spec;

use super::classes::application::ClassTypeParameterDefault;
use super::classes::construction::DraftClassTypeParameterSnapshot;
use super::context::{PublishedClassNewMetadata, PublishedClassValueBinding};
#[cfg(test)]
use super::library_compiler::BorrowedLibraryRuntimeSnapshotParts;
#[cfg(test)]
use super::library_compiler::{
    compile_owned_injected_profile, CompiledLibraryRuntimeProduct, InjectedLibrarySource,
};
use super::library_compiler::{OwnedLibraryRuntimeSnapshotParts, OwnedLibraryRuntimeState};
use super::library_identities::{
    LibraryIdentityTerminal, LibraryIdentityUnavailable, LibraryTypeIdentity,
};
use super::namespace_values::{
    FrozenNamespaceValueTerminalSnapshot, FrozenNamespaceValueTerminalSnapshotRow,
};
#[cfg(test)]
use super::replay_index::admit_collision_replay_index;
use super::replay_index::{
    admit_decoded_collision_replay_index, decode_authenticated_collision_replay_index,
    ReplayIndexAdmissionError, ReplayIndexAdmissionLimits, COLLISION_REPLAY_MANIFEST_SHA256,
};
use super::type_groups::{
    InterfaceAlternativeKind, InterfaceTypedAlternative, PublishedTypeEnvironmentSnapshotParts,
    PublishedTypeGroup, PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
    PublishedTypeGroupUnavailable, PublishedTypeParameterDefault, TypeGroupUnavailableCause,
};
use super::FrozenCheckerRuntimeSnapshotParts;
use crate::binder::bind::{LibraryBinderCheckpointEnds, LibraryBinderUnit};
use crate::binder::declaration::{TypeGroupId, ValueStorageId};
use crate::binder::namespace::NamespaceId;
#[cfg(test)]
use crate::binder::snapshot::decode_binder_snapshot;
use crate::binder::snapshot::encode_binder_snapshot;
use crate::binder::snapshot::{
    decode_binder_snapshot_with_evidence, snapshot_reference_records,
    RetainedScopeMapSnapshotEvidence,
};
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::class_semantics::{
    PublishedClassPoison, PublishedClassSnapshotTerminal, PublishedClassSurface,
};
#[cfg(test)]
use crate::diagnostics::{Diagnostic, IncompleteSurface};
use crate::library::artifact::AdmittedCanonicalSnapshot;
use crate::snapshot_codec::SnapshotWriter;
use crate::snapshot_codec::{SnapshotCodecError, SnapshotReader};
use crate::types::repr::{ClassId, TypeParamId, Visibility};
#[cfg(test)]
use crate::types::repr::{IntrinsicKind, TypeTag};
#[cfg(test)]
use crate::types::store::Store;
use crate::types::store::TypeId;
use crate::types::Interner;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
#[cfg(test)]
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
#[cfg(test)]
use std::fs;
use std::ops::Range;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::time::Instant;

#[cfg(test)]
pub(super) use runtime::check_source_with_decoded_base_for_test;

const MAGIC: &[u8] = b"typokat-semantic-snapshot";
const VERSION: u32 = 1;
const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const SCHEMA_IDENTITY: &str = "6cf27cde368f8b2ff3bdafd5fce8fb3550ec8e2264aab7249362e2294e3f5be0";
const CANONICAL_ARCHIVE_BYTES: usize = 21_003_926;
#[cfg(test)]
const CANONICAL_ARCHIVE_SHA256: &str =
    "47a8a6fd349f3b3fbb3aae1baccedbc67530edc35227707d79afac5395ca7d2f";
#[cfg(test)]
const SECTION_NAMES: [&str; 11] = [
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
    "collision-replay-index",
];
#[cfg(test)]
const SUBTABLE_NAMES: [&str; 30] = [
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
    // `next-ids` is returned separately below to preserve the contract's exact order.
];
const FIXED_HEADER_LEN: usize = MAGIC.len() + 4 + 32 + 32 + 4 + 8 + 32;
const DIRECTORY_ENTRY_LEN: usize = 52;
const SECTION_COUNT: usize = 11;
const PROJECTION_WITNESS_VERSION: u32 = 1;
const PROJECTION_WITNESS_COUNT: usize = 31;
const ABSENT_ID: u32 = u32::MAX;
const MAX_REFERENCE_DOMAIN: u8 = 30;
const SEMANTIC_IDENTITY_DOMAIN: u8 = 30;
const ROW_IDENTITY_FIELD: u8 = 31;
const APPLICATION_ROW_FIELD: u8 = 10;
const NEW_METADATA_ROW_FIELD: u8 = 11;
const PARENT_ROW_FIELD: u8 = 12;
const CLASS_ALIAS_ROW_FIELD: u8 = 13;
const CLASS_BINDING_ROW_FIELD: u8 = 14;
const NAMESPACE_ALIAS_ROW_FIELD: u8 = 15;
const NAMED_FUNCTION_ROW_FIELD: u8 = 16;
const CLASS_NAME_ROW_FIELD: u8 = 17;

fn invalid(stage: SnapshotErrorStage, message: impl Into<String>) -> SnapshotError {
    SnapshotError {
        stage,
        message: message.into(),
        kind: SnapshotErrorKind::Rejected,
    }
}

fn rejected_replay_index(error: ReplayIndexAdmissionError) -> SnapshotError {
    SnapshotError {
        stage: SnapshotErrorStage::CollisionReplayIndexAdmission,
        message: format!("collision replay index rejected: {error:?}"),
        kind: SnapshotErrorKind::ReplayIndexRejected(error),
    }
}

fn codec(stage: SnapshotErrorStage, error: SnapshotCodecError) -> SnapshotError {
    invalid(stage, error.to_string())
}

fn digest32(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[cfg(test)]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex32(value: &str) -> [u8; 32] {
    let mut result = [0; 32];
    if value.len() != 64 {
        return result;
    }
    for (byte, pair) in result.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        let Some(high) = hex_nibble(pair[0]) else {
            return [0; 32];
        };
        let Some(low) = hex_nibble(pair[1]) else {
            return [0; 32];
        };
        *byte = (high << 4) | low;
    }
    result
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum SnapshotDecodeStrategy {
    EagerComplete,
    ImmutableIndexed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotErrorStage {
    HeaderValidation,
    DirectoryValidation,
    PayloadValidation,
    ReferenceValidation,
    Publication,
    Decode,
    CollisionReplayIndexAdmission,
    #[cfg(test)]
    UnsupportedStrategy,
    #[cfg(test)]
    Io,
    Generation,
    #[cfg(test)]
    UserCheck,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotError {
    stage: SnapshotErrorStage,
    message: String,
    kind: SnapshotErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotErrorKind {
    InvalidId { id: u32, limit: usize },
    WorkerSpawnFailed { worker: &'static str },
    WorkerPanicked { worker: &'static str },
    ReplayIndexRejected(ReplayIndexAdmissionError),
    Rejected,
}

impl SnapshotError {
    pub(crate) fn stage(&self) -> SnapshotErrorStage {
        self.stage
    }

    pub(crate) fn kind(&self) -> &SnapshotErrorKind {
        &self.kind
    }
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.stage, self.message)
    }
}

impl std::error::Error for SnapshotError {}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct SnapshotArchiveForTest {
    bytes: Vec<u8>,
    sha256: [u8; 32],
}

#[cfg(test)]
impl SnapshotArchiveForTest {
    pub(in crate::check::checker) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(in crate::check::checker) fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct RuntimeFamilyForTest {
    pub(in crate::check::checker) name: &'static str,
    pub(in crate::check::checker) byte_len: usize,
    pub(in crate::check::checker) sha256: [u8; 32],
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct RuntimeSubtableForTest {
    pub(in crate::check::checker) name: &'static str,
    pub(in crate::check::checker) row_count: u64,
    pub(in crate::check::checker) sha256: [u8; 32],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct NextIds {
    pub(in crate::check::checker) store: usize,
    pub(in crate::check::checker) types: usize,
    pub(in crate::check::checker) type_params: usize,
    pub(in crate::check::checker) classes: usize,
    pub(in crate::check::checker) scopes: usize,
    pub(in crate::check::checker) symbols: usize,
    pub(in crate::check::checker) declarations: usize,
    pub(in crate::check::checker) type_groups: usize,
    pub(in crate::check::checker) namespaces: usize,
    pub(in crate::check::checker) value_storages: usize,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeProjectionForTest {
    families: Vec<RuntimeFamilyForTest>,
    subtables: Vec<RuntimeSubtableForTest>,
    global_names: BTreeSet<String>,
    root_counts: [u64; 4],
    next_ids: NextIds,
    reference_counts: [u64; 9],
    reference_manifest_sha256: [u8; 32],
    typed_validation_sha256: [u8; 32],
    sha256: [u8; 32],
}

#[cfg(test)]
impl RuntimeProjectionForTest {
    pub(crate) fn reference_family_counts_for_library(&self) -> [u64; 9] {
        self.reference_counts
    }

    pub(crate) fn root_names_for_library(&self) -> &BTreeSet<String> {
        &self.global_names
    }

    pub(crate) fn prefixes_for_library(&self) -> [usize; 9] {
        [
            self.next_ids.types,
            self.next_ids.type_params,
            self.next_ids.classes,
            self.next_ids.scopes,
            self.next_ids.symbols,
            self.next_ids.declarations,
            self.next_ids.type_groups,
            self.next_ids.namespaces,
            self.next_ids.value_storages,
        ]
    }

    pub(crate) fn typed_validation_sha256_for_library(&self) -> String {
        hex(&self.typed_validation_sha256)
    }

    pub(crate) fn family_names(&self) -> [&'static str; 11] {
        SECTION_NAMES
    }

    pub(in crate::check::checker) fn families(&self) -> &[RuntimeFamilyForTest] {
        &self.families
    }

    pub(in crate::check::checker) fn subtable_names(&self) -> [&'static str; 31] {
        [
            SUBTABLE_NAMES[0],
            SUBTABLE_NAMES[1],
            SUBTABLE_NAMES[2],
            SUBTABLE_NAMES[3],
            SUBTABLE_NAMES[4],
            SUBTABLE_NAMES[5],
            SUBTABLE_NAMES[6],
            SUBTABLE_NAMES[7],
            SUBTABLE_NAMES[8],
            SUBTABLE_NAMES[9],
            SUBTABLE_NAMES[10],
            SUBTABLE_NAMES[11],
            SUBTABLE_NAMES[12],
            SUBTABLE_NAMES[13],
            SUBTABLE_NAMES[14],
            SUBTABLE_NAMES[15],
            SUBTABLE_NAMES[16],
            SUBTABLE_NAMES[17],
            SUBTABLE_NAMES[18],
            SUBTABLE_NAMES[19],
            SUBTABLE_NAMES[20],
            SUBTABLE_NAMES[21],
            SUBTABLE_NAMES[22],
            SUBTABLE_NAMES[23],
            SUBTABLE_NAMES[24],
            SUBTABLE_NAMES[25],
            SUBTABLE_NAMES[26],
            SUBTABLE_NAMES[27],
            SUBTABLE_NAMES[28],
            SUBTABLE_NAMES[29],
            "next-ids",
        ]
    }

    pub(in crate::check::checker) fn subtables(&self) -> &[RuntimeSubtableForTest] {
        &self.subtables
    }

    pub(in crate::check::checker) fn subtable(
        &self,
        name: &str,
    ) -> Option<&RuntimeSubtableForTest> {
        self.subtables.iter().find(|table| table.name == name)
    }

    pub(in crate::check::checker) fn compilation_global_names(&self) -> BTreeSet<String> {
        self.global_names.clone()
    }

    pub(in crate::check::checker) fn root_name_index_counts(&self) -> [u64; 4] {
        self.root_counts
    }

    pub(in crate::check::checker) fn next_ids(&self) -> &NextIds {
        &self.next_ids
    }

    pub(in crate::check::checker) fn reference_counts(&self) -> [u64; 9] {
        self.reference_counts
    }

    pub(in crate::check::checker) fn reference_manifest_sha256(&self) -> [u8; 32] {
        self.reference_manifest_sha256
    }

    pub(in crate::check::checker) fn runtime_counts(&self) -> &NextIds {
        &self.next_ids
    }

    pub(in crate::check::checker) fn sha256(&self) -> String {
        hex(&self.sha256)
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct IdentityWitnessForTest {
    pub(super) classes: BTreeMap<String, ClassId>,
    groups: BTreeMap<String, TypeGroupId>,
}

#[cfg(test)]
impl IdentityWitnessForTest {
    pub(in crate::check::checker) fn class_id(&self, name: &str) -> Option<ClassId> {
        self.classes.get(name).copied()
    }

    pub(in crate::check::checker) fn type_group_id(&self, name: &str) -> Option<TypeGroupId> {
        self.groups.get(name).copied()
    }
}

#[cfg(test)]
pub(in crate::check::checker) struct CompiledSnapshotForTest {
    archive: SnapshotArchiveForTest,
    projection: RuntimeProjectionForTest,
    identity: IdentityWitnessForTest,
}

#[cfg(test)]
impl CompiledSnapshotForTest {
    pub(in crate::check::checker) fn archive(&self) -> &SnapshotArchiveForTest {
        &self.archive
    }

    pub(in crate::check::checker) fn runtime_projection(&self) -> &RuntimeProjectionForTest {
        &self.projection
    }

    pub(in crate::check::checker) fn identity_witness(&self) -> &IdentityWitnessForTest {
        &self.identity
    }
}

#[derive(Clone)]
pub(in crate::check::checker) struct RootNameRow {
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) symbol: Option<SymbolId>,
    pub(in crate::check::checker) value: Option<ValueStorageId>,
    pub(in crate::check::checker) ty: Option<TypeGroupId>,
    pub(in crate::check::checker) namespace: Option<NamespaceId>,
}

#[derive(Clone, Debug)]
struct DirectorySection {
    range: Range<usize>,
    digest: [u8; 32],
}

#[derive(Clone, Debug)]
pub(in crate::check::checker) struct ValidatedSnapshot {
    bytes: Cow<'static, [u8]>,
    sections: Vec<DirectorySection>,
}

pub(crate) struct AdmittedLibrarySnapshotEvidence {
    section_digests: [[u8; 32]; 11],
    prefixes: LibraryBinderCheckpointEnds,
    library_units: Vec<LibraryBinderUnit>,
    retained_scope_maps_sha256: [u8; 32],
}

impl AdmittedLibrarySnapshotEvidence {
    pub(crate) fn section_digest(&self, tag: usize) -> Option<[u8; 32]> {
        tag.checked_sub(1)
            .and_then(|index| self.section_digests.get(index))
            .copied()
    }

    pub(crate) fn library_prefixes(&self) -> LibraryBinderCheckpointEnds {
        self.prefixes
    }

    #[cfg(test)]
    pub(crate) fn next_source(&self) -> usize {
        self.prefixes.next_source
    }

    pub(crate) fn library_units(&self) -> &[LibraryBinderUnit] {
        &self.library_units
    }

    pub(crate) fn retained_scope_maps_sha256(&self) -> [u8; 32] {
        self.retained_scope_maps_sha256
    }
}

pub(in crate::check::checker) struct DecodedLibraryBase {
    pub(super) state: OwnedLibraryRuntimeState,
    #[cfg(test)]
    pub(super) typed_validation_sha256: [u8; 32],
    #[cfg(test)]
    pub(super) projection: RuntimeProjectionForTest,
    #[cfg(test)]
    pub(super) identity: IdentityWitnessForTest,
    #[cfg(test)]
    source_file_count: u32,
    pub(super) prefix_lengths: NextIds,
    root_names: BTreeSet<String>,
    #[cfg(test)]
    root_counts: [u64; 4],
    #[cfg(test)]
    strategy: SnapshotDecodeStrategy,
}

pub(crate) struct DecodedFrozenLibrary {
    pub(crate) runtime: OwnedLibraryRuntimeState,
    pub(crate) root_names: BTreeSet<String>,
    pub(crate) prefixes: [usize; 9],
    #[cfg(test)]
    pub(crate) typed_validation_sha256: [u8; 32],
}

#[cfg(test)]
pub(crate) struct FrozenReferenceValidation {
    pub(crate) checked: u64,
    pub(crate) outside_frozen_prefix: u64,
    pub(crate) base_to_delta: u64,
    pub(crate) untyped_or_unowned: u64,
}

#[cfg(test)]
impl fmt::Debug for DecodedLibraryBase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecodedLibraryBase")
            .field("source_file_count", &self.source_file_count)
            .field("prefix_lengths", &self.prefix_lengths)
            .field("strategy", &self.strategy)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct IdentityRangeSetForTest {
    pub(in crate::check::checker) store: Range<usize>,
    pub(in crate::check::checker) type_params: Range<usize>,
    pub(in crate::check::checker) classes: Range<usize>,
    pub(in crate::check::checker) scopes: Range<usize>,
    pub(in crate::check::checker) symbols: Range<usize>,
    pub(in crate::check::checker) declarations: Range<usize>,
    pub(in crate::check::checker) type_groups: Range<usize>,
    pub(in crate::check::checker) namespaces: Range<usize>,
    pub(in crate::check::checker) value_storages: Range<usize>,
}

#[cfg(test)]
pub(in crate::check::checker) struct DecodedUserCheckResultForTest {
    pub(in crate::check::checker) parse_errors: Vec<String>,
    pub(in crate::check::checker) diagnostics: Vec<Diagnostic>,
    pub(in crate::check::checker) incomplete: Vec<IncompleteSurface>,
    pub(in crate::check::checker) base_projection_after_user_check: RuntimeProjectionForTest,
    pub(in crate::check::checker) user_identity_ranges: IdentityRangeSetForTest,
    pub(in crate::check::checker) reused_base_shape: Option<ReusedBaseShapeForTest>,
    pub(super) user_types: BTreeMap<String, TypeId>,
    pub(super) observed_classes: BTreeMap<String, ClassId>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct ReusedBaseShapeForTest {
    type_id: TypeId,
    tag: TypeTag,
}

#[cfg(test)]
impl ReusedBaseShapeForTest {
    pub(in crate::check::checker) fn new(type_id: TypeId, tag: TypeTag) -> Self {
        Self { type_id, tag }
    }

    pub(in crate::check::checker) fn index(self) -> usize {
        debug_assert!(matches!(
            self.tag,
            TypeTag::Object | TypeTag::ClassInstance | TypeTag::Function
        ));
        self.type_id.index()
    }
}

#[cfg(test)]
impl DecodedUserCheckResultForTest {
    pub(in crate::check::checker) fn user_type_id(&self, name: &str) -> Option<TypeId> {
        self.user_types.get(name).copied()
    }
    pub(in crate::check::checker) fn observed_base_class_id(&self, name: &str) -> Option<ClassId> {
        self.observed_classes.get(name).copied()
    }
}

#[cfg(test)]
impl DecodedLibraryBase {
    pub(in crate::check::checker) fn profile_identity(&self) -> &'static str {
        PROFILE_IDENTITY
    }

    pub(in crate::check::checker) fn source_file_count(&self) -> u32 {
        self.source_file_count
    }

    pub(in crate::check::checker) fn present_semantic_roots<'a>(
        &self,
        roots: &BTreeSet<&'a str>,
    ) -> BTreeSet<&'a str> {
        roots
            .iter()
            .copied()
            .filter(|root| self.root_names.contains(*root))
            .collect()
    }

    pub(in crate::check::checker) fn runtime_projection(&self) -> &RuntimeProjectionForTest {
        &self.projection
    }

    pub(in crate::check::checker) fn reference_counts(&self) -> [u64; 9] {
        self.projection.reference_counts
    }

    pub(in crate::check::checker) fn reference_manifest_sha256(&self) -> [u8; 32] {
        self.projection.reference_manifest_sha256
    }

    pub(in crate::check::checker) fn runtime_counts(&self) -> &NextIds {
        &self.prefix_lengths
    }

    pub(in crate::check::checker) fn section_inventory(&self) -> [&'static str; 11] {
        SECTION_NAMES
    }

    pub(in crate::check::checker) fn root_name_index_names(&self) -> BTreeSet<String> {
        self.root_names.clone()
    }

    pub(in crate::check::checker) fn root_name_index_counts(&self) -> [u64; 4] {
        self.root_counts
    }

    pub(in crate::check::checker) fn prefix_lengths(&self) -> &NextIds {
        &self.prefix_lengths
    }

    pub(in crate::check::checker) fn identity_witness(&self) -> &IdentityWitnessForTest {
        &self.identity
    }
}

impl DecodedFrozenLibrary {
    fn from_decoded(decoded: DecodedLibraryBase) -> Self {
        let DecodedLibraryBase {
            state: runtime,
            #[cfg(test)]
            typed_validation_sha256,
            prefix_lengths,
            root_names,
            ..
        } = decoded;
        Self {
            runtime,
            root_names,
            prefixes: [
                prefix_lengths.types,
                prefix_lengths.type_params,
                prefix_lengths.classes,
                prefix_lengths.scopes,
                prefix_lengths.symbols,
                prefix_lengths.declarations,
                prefix_lengths.type_groups,
                prefix_lengths.namespaces,
                prefix_lengths.value_storages,
            ],
            #[cfg(test)]
            typed_validation_sha256,
        }
    }
}

#[cfg(test)]
fn write_optional_u32(writer: &mut SnapshotWriter, value: Option<u32>) {
    match value {
        None => writer.u8(0),
        Some(value) => {
            writer.u8(1);
            writer.u32(value);
        }
    }
}

fn read_optional_u32(reader: &mut SnapshotReader<'_>) -> Result<Option<u32>, SnapshotCodecError> {
    let tag_offset = reader.position();
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(reader.u32()?)),
        _ => Err(SnapshotCodecError::invalid(
            tag_offset,
            "invalid optional-u32 tag",
        )),
    }
}

fn write_root_optional_u32(writer: &mut SnapshotWriter, value: Option<u32>) {
    writer.u32(value.unwrap_or(ABSENT_ID));
}

fn read_root_optional_u32(
    reader: &mut SnapshotReader<'_>,
) -> Result<Option<u32>, SnapshotCodecError> {
    let value = reader.u32()?;
    Ok((value != ABSENT_ID).then_some(value))
}

#[cfg(test)]
fn write_type_id(writer: &mut SnapshotWriter, value: TypeId) {
    writer.u32(value.0);
}

fn read_type_id(reader: &mut SnapshotReader<'_>) -> Result<TypeId, SnapshotCodecError> {
    Ok(TypeId(reader.u32()?))
}

#[cfg(test)]
fn write_type_param(writer: &mut SnapshotWriter, value: TypeParamId) {
    writer.u32(value.0);
}

fn read_type_param(reader: &mut SnapshotReader<'_>) -> Result<TypeParamId, SnapshotCodecError> {
    Ok(TypeParamId(reader.u32()?))
}

#[cfg(test)]
fn write_class(writer: &mut SnapshotWriter, value: ClassId) {
    writer.u32(value.0);
}

fn read_class(reader: &mut SnapshotReader<'_>) -> Result<ClassId, SnapshotCodecError> {
    Ok(ClassId(reader.u32()?))
}

#[cfg(test)]
fn write_len(writer: &mut SnapshotWriter, len: usize) -> Result<(), SnapshotCodecError> {
    writer.usize(len)
}

fn id32(value: usize) -> Result<u32, SnapshotCodecError> {
    u32::try_from(value)
        .map_err(|_| SnapshotCodecError::invalid(0, "snapshot identity exceeds u32"))
}

#[cfg(test)]
fn count64(value: usize) -> u64 {
    u64::try_from(value).expect("snapshot count fits u64")
}

fn validate_id(id: u32, limit: usize, label: &str) -> Result<(), SnapshotError> {
    if usize::try_from(id).ok().is_some_and(|id| id < limit) {
        Ok(())
    } else {
        Err(SnapshotError {
            stage: SnapshotErrorStage::ReferenceValidation,
            message: format!("{label} id {id} exceeds prefix {limit}"),
            kind: SnapshotErrorKind::InvalidId { id, limit },
        })
    }
}

#[cfg(test)]
fn encode_decl_types(slots: &[Option<TypeId>]) -> Result<Vec<u8>, SnapshotError> {
    let mut writer = SnapshotWriter::new();
    writer.u32(1);
    write_len(&mut writer, slots.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for slot in slots {
        write_optional_u32(&mut writer, slot.map(|id| id.0));
    }
    Ok(writer.into_bytes())
}

fn decode_decl_types(
    bytes: &[u8],
    type_limit: usize,
) -> Result<Vec<Option<TypeId>>, SnapshotError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported decl-types version",
        ));
    }
    let count = reader
        .collection_len(4)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut slots = Vec::with_capacity(count);
    for _ in 0..count {
        let slot = read_optional_u32(&mut reader)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        if let Some(id) = slot {
            validate_id(id, type_limit, "declaration type")?;
        }
        slots.push(slot.map(TypeId));
    }
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    Ok(slots)
}

#[cfg(test)]
fn write_class_surface(
    writer: &mut SnapshotWriter,
    surface: &PublishedClassSurface,
) -> Result<(), SnapshotCodecError> {
    write_class(writer, surface.class());
    write_len(writer, surface.type_params().len())?;
    for parameter in surface.type_params() {
        write_type_param(writer, *parameter);
    }
    write_type_id(writer, surface.instance_template());
    write_type_id(writer, surface.static_template());
    write_optional_u32(writer, surface.constructor_template().map(|id| id.0));
    Ok(())
}

fn read_class_surface(
    reader: &mut SnapshotReader<'_>,
) -> Result<PublishedClassSurface, SnapshotCodecError> {
    let class = read_class(reader)?;
    let count = reader.collection_len(4)?;
    let mut parameters = Vec::with_capacity(count);
    for _ in 0..count {
        parameters.push(read_type_param(reader)?);
    }
    let instance = read_type_id(reader)?;
    let static_template = read_type_id(reader)?;
    let constructor = read_optional_u32(reader)?.map(TypeId);
    Ok(PublishedClassSurface::new(
        class,
        parameters,
        instance,
        static_template,
        constructor,
    ))
}

#[cfg(test)]
fn write_group(
    writer: &mut SnapshotWriter,
    terminal: &PublishedTypeGroupTerminal,
) -> Result<(), SnapshotCodecError> {
    match terminal {
        PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
            cause: TypeGroupUnavailableCause::UnsupportedComposition,
        }) => writer.u8(0),
        PublishedTypeGroupTerminal::Ready(group) => {
            writer.u8(1);
            writer.string(&group.name)?;
            match group.surface {
                PublishedTypeGroupSurface::Template(id) => {
                    writer.u8(0);
                    write_type_id(writer, id);
                }
                PublishedTypeGroupSurface::Class(id) => {
                    writer.u8(1);
                    write_class(writer, id);
                }
            }
            write_len(writer, group.parameters.len())?;
            for id in &group.parameters {
                write_type_param(writer, *id);
            }
            write_len(writer, group.parameter_names.len())?;
            for name in &group.parameter_names {
                writer.string(name)?;
            }
            write_len(writer, group.parameter_defaults.len())?;
            for default in &group.parameter_defaults {
                match default {
                    PublishedTypeParameterDefault::Absent => writer.u8(0),
                    PublishedTypeParameterDefault::Ready(id) => {
                        writer.u8(1);
                        write_type_id(writer, *id);
                    }
                    PublishedTypeParameterDefault::Unsupported => writer.u8(2),
                }
            }
            write_len(writer, group.conflict_alternatives.len())?;
            for alternative in &group.conflict_alternatives {
                writer.u8(match alternative.kind {
                    InterfaceAlternativeKind::Member => 0,
                    InterfaceAlternativeKind::StringIndex => 1,
                    InterfaceAlternativeKind::NumberIndex => 2,
                    InterfaceAlternativeKind::Heritage => 3,
                });
                writer.string(&alternative.key)?;
                write_len(writer, alternative.types.len())?;
                for id in &alternative.types {
                    write_type_id(writer, *id);
                }
            }
        }
    }
    Ok(())
}

fn read_group(
    reader: &mut SnapshotReader<'_>,
) -> Result<PublishedTypeGroupTerminal, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(PublishedTypeGroupTerminal::Unavailable(
            PublishedTypeGroupUnavailable {
                cause: TypeGroupUnavailableCause::UnsupportedComposition,
            },
        )),
        1 => {
            let name = reader.string()?.to_owned();
            let surface = match reader.u8()? {
                0 => PublishedTypeGroupSurface::Template(read_type_id(reader)?),
                1 => PublishedTypeGroupSurface::Class(read_class(reader)?),
                _ => {
                    return Err(SnapshotCodecError::invalid(
                        reader.position() - 1,
                        "invalid type-group surface",
                    ))
                }
            };
            let parameter_count = reader.collection_len(4)?;
            let mut parameters = Vec::with_capacity(parameter_count);
            for _ in 0..parameter_count {
                parameters.push(read_type_param(reader)?);
            }
            let name_count = reader.collection_len(8)?;
            let mut parameter_names = Vec::with_capacity(name_count);
            for _ in 0..name_count {
                parameter_names.push(reader.string()?.to_owned());
            }
            let default_count = reader.collection_len(1)?;
            let mut parameter_defaults = Vec::with_capacity(default_count);
            for _ in 0..default_count {
                parameter_defaults.push(match reader.u8()? {
                    0 => PublishedTypeParameterDefault::Absent,
                    1 => PublishedTypeParameterDefault::Ready(read_type_id(reader)?),
                    2 => PublishedTypeParameterDefault::Unsupported,
                    _ => {
                        return Err(SnapshotCodecError::invalid(
                            reader.position() - 1,
                            "invalid parameter default",
                        ))
                    }
                });
            }
            let alternative_count = reader.collection_len(1)?;
            let mut conflict_alternatives = Vec::with_capacity(alternative_count);
            for _ in 0..alternative_count {
                let kind = match reader.u8()? {
                    0 => InterfaceAlternativeKind::Member,
                    1 => InterfaceAlternativeKind::StringIndex,
                    2 => InterfaceAlternativeKind::NumberIndex,
                    3 => InterfaceAlternativeKind::Heritage,
                    _ => {
                        return Err(SnapshotCodecError::invalid(
                            reader.position() - 1,
                            "invalid interface alternative",
                        ))
                    }
                };
                let key = reader.string()?.to_owned();
                let count = reader.collection_len(4)?;
                let mut types = Vec::with_capacity(count);
                for _ in 0..count {
                    types.push(read_type_id(reader)?);
                }
                conflict_alternatives.push(InterfaceTypedAlternative { kind, key, types });
            }
            Ok(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                name,
                surface,
                parameters,
                parameter_names,
                parameter_defaults,
                conflict_alternatives,
            }))
        }
        _ => Err(SnapshotCodecError::invalid(
            reader.position() - 1,
            "invalid type-group terminal",
        )),
    }
}

#[cfg(test)]
fn encode_published(
    parts: &PublishedTypeEnvironmentSnapshotParts,
) -> Result<Vec<u8>, SnapshotError> {
    let mut writer = SnapshotWriter::new();
    writer.u32(1);
    write_len(&mut writer, parts.groups.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for group in &parts.groups {
        write_group(&mut writer, group)
            .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    }
    write_len(&mut writer, parts.classes.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (class, terminal) in &parts.classes {
        write_class(&mut writer, *class);
        match terminal {
            PublishedClassSnapshotTerminal::Ready(surface) => {
                writer.u8(0);
                write_class_surface(&mut writer, surface)
                    .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
            }
            PublishedClassSnapshotTerminal::Poisoned(cause) => writer.u8(match cause {
                PublishedClassPoison::Heritage => 1,
                PublishedClassPoison::Initializer => 2,
                PublishedClassPoison::Surface => 3,
            }),
        }
    }
    Ok(writer.into_bytes())
}

fn decode_published(bytes: &[u8]) -> Result<PublishedTypeEnvironmentSnapshotParts, SnapshotError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported published-types version",
        ));
    }
    let count = reader
        .collection_len(1)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut groups = Vec::with_capacity(count);
    for _ in 0..count {
        groups.push(
            read_group(&mut reader).map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
        );
    }
    let count = reader
        .collection_len(5)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut classes = Vec::with_capacity(count);
    for _ in 0..count {
        let class =
            read_class(&mut reader).map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        let terminal = match reader
            .u8()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        {
            0 => PublishedClassSnapshotTerminal::Ready(
                read_class_surface(&mut reader)
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            ),
            1 => PublishedClassSnapshotTerminal::Poisoned(PublishedClassPoison::Heritage),
            2 => PublishedClassSnapshotTerminal::Poisoned(PublishedClassPoison::Initializer),
            3 => PublishedClassSnapshotTerminal::Poisoned(PublishedClassPoison::Surface),
            _ => {
                return Err(invalid(
                    SnapshotErrorStage::Decode,
                    "invalid class terminal",
                ))
            }
        };
        classes.push((class, terminal));
    }
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    Ok(PublishedTypeEnvironmentSnapshotParts { classes, groups })
}

#[cfg(test)]
fn encode_namespace_terminals(
    rows: &[FrozenNamespaceValueTerminalSnapshotRow],
) -> Result<Vec<u8>, SnapshotError> {
    let mut writer = SnapshotWriter::new();
    writer.u32(1);
    write_len(&mut writer, rows.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for row in rows {
        writer.u32(row.namespace.0);
        match row.terminal {
            FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } => {
                writer.u8(0);
                writer.u32(storage.0);
                write_type_id(&mut writer, ty);
            }
            FrozenNamespaceValueTerminalSnapshot::Unavailable(cause) => {
                writer.u8(1);
                writer.u8(match cause {
                    super::namespace_values::NamespaceValueUnavailableCause::MissingExportedMemberName => 0,
                    super::namespace_values::NamespaceValueUnavailableCause::DuplicateExportedValue => 1,
                    super::namespace_values::NamespaceValueUnavailableCause::UnboundExportedVariable => 2,
                    super::namespace_values::NamespaceValueUnavailableCause::MissingExportedVariableSyntax => 3,
                    super::namespace_values::NamespaceValueUnavailableCause::InvalidUsingDeclaration => 4,
                    super::namespace_values::NamespaceValueUnavailableCause::VariableSurfaceUnavailable => 5,
                    super::namespace_values::NamespaceValueUnavailableCause::UnboundExportedFunction => 6,
                    super::namespace_values::NamespaceValueUnavailableCause::MissingExportedFunctionSyntax => 7,
                    super::namespace_values::NamespaceValueUnavailableCause::UnboundExportedClass => 8,
                    super::namespace_values::NamespaceValueUnavailableCause::UnboundExportedClassIdentity => 9,
                    super::namespace_values::NamespaceValueUnavailableCause::UnboundNestedNamespace => 10,
                    super::namespace_values::NamespaceValueUnavailableCause::UnsupportedExportedMember => 11,
                    super::namespace_values::NamespaceValueUnavailableCause::DeferredExportedMember => 12,
                    super::namespace_values::NamespaceValueUnavailableCause::FunctionNamespacePayloadUnavailable => 13,
                    super::namespace_values::NamespaceValueUnavailableCause::FunctionOwnerCallSurfaceUnavailable => 14,
                    super::namespace_values::NamespaceValueUnavailableCause::FunctionSurfaceUnavailable => 15,
                    super::namespace_values::NamespaceValueUnavailableCause::ClassSurfaceUnavailable => 16,
                    super::namespace_values::NamespaceValueUnavailableCause::NestedNamespaceUnavailable => 17,
                    super::namespace_values::NamespaceValueUnavailableCause::ExistingOwnerUnavailable => 18,
                    super::namespace_values::NamespaceValueUnavailableCause::NamespaceContainmentCycle => 19,
                    super::namespace_values::NamespaceValueUnavailableCause::InvalidPrivateNamespaceMember => 20,
                    super::namespace_values::NamespaceValueUnavailableCause::ClassValueSurfaceUnavailable => 21,
                });
            }
        }
    }
    Ok(writer.into_bytes())
}

fn decode_namespace_terminals(
    bytes: &[u8],
) -> Result<Vec<FrozenNamespaceValueTerminalSnapshotRow>, SnapshotError> {
    use super::namespace_values::NamespaceValueUnavailableCause as Cause;
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported namespace terminal version",
        ));
    }
    let count = reader
        .collection_len(5)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let namespace = NamespaceId(
            reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
        );
        let terminal = match reader
            .u8()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        {
            0 => FrozenNamespaceValueTerminalSnapshot::Ready {
                storage: ValueStorageId(
                    reader
                        .u32()
                        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
                ),
                ty: read_type_id(&mut reader)
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            },
            1 => {
                let raw = reader
                    .u8()
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
                let cause = match raw {
                    0 => Cause::MissingExportedMemberName,
                    1 => Cause::DuplicateExportedValue,
                    2 => Cause::UnboundExportedVariable,
                    3 => Cause::MissingExportedVariableSyntax,
                    4 => Cause::InvalidUsingDeclaration,
                    5 => Cause::VariableSurfaceUnavailable,
                    6 => Cause::UnboundExportedFunction,
                    7 => Cause::MissingExportedFunctionSyntax,
                    8 => Cause::UnboundExportedClass,
                    9 => Cause::UnboundExportedClassIdentity,
                    10 => Cause::UnboundNestedNamespace,
                    11 => Cause::UnsupportedExportedMember,
                    12 => Cause::DeferredExportedMember,
                    13 => Cause::FunctionNamespacePayloadUnavailable,
                    14 => Cause::FunctionOwnerCallSurfaceUnavailable,
                    15 => Cause::FunctionSurfaceUnavailable,
                    16 => Cause::ClassSurfaceUnavailable,
                    17 => Cause::NestedNamespaceUnavailable,
                    18 => Cause::ExistingOwnerUnavailable,
                    19 => Cause::NamespaceContainmentCycle,
                    20 => Cause::InvalidPrivateNamespaceMember,
                    21 => Cause::ClassValueSurfaceUnavailable,
                    _ => {
                        return Err(invalid(
                            SnapshotErrorStage::Decode,
                            "invalid namespace unavailable cause",
                        ))
                    }
                };
                FrozenNamespaceValueTerminalSnapshot::Unavailable(cause)
            }
            _ => {
                return Err(invalid(
                    SnapshotErrorStage::Decode,
                    "invalid namespace terminal",
                ))
            }
        };
        rows.push(FrozenNamespaceValueTerminalSnapshotRow {
            namespace,
            terminal,
        });
    }
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    Ok(rows)
}

#[cfg(test)]
fn write_visibility(writer: &mut SnapshotWriter, visibility: Visibility) {
    writer.u8(match visibility {
        Visibility::Public => 0,
        Visibility::Private => 1,
        Visibility::Protected => 2,
    });
}

fn read_visibility(reader: &mut SnapshotReader<'_>) -> Result<Visibility, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(Visibility::Public),
        1 => Ok(Visibility::Private),
        2 => Ok(Visibility::Protected),
        _ => Err(SnapshotCodecError::invalid(
            reader.position() - 1,
            "invalid visibility",
        )),
    }
}

#[cfg(test)]
fn encode_class_metadata(
    parts: &FrozenCheckerRuntimeSnapshotParts,
) -> Result<Vec<u8>, SnapshotError> {
    let mut writer = SnapshotWriter::new();
    writer.u32(1);
    write_len(&mut writer, parts.class_application_parameters.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (class, parameters) in &parts.class_application_parameters {
        write_class(&mut writer, *class);
        write_len(&mut writer, parameters.len())
            .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
        for parameter in parameters {
            write_type_param(&mut writer, parameter.id);
            write_optional_u32(&mut writer, parameter.constraint.map(|id| id.0));
            match parameter.default {
                ClassTypeParameterDefault::Absent => writer.u8(0),
                ClassTypeParameterDefault::Ready(id) => {
                    writer.u8(1);
                    write_type_id(&mut writer, id);
                }
                ClassTypeParameterDefault::Unsupported(()) => writer.u8(2),
            }
        }
    }
    write_len(&mut writer, parts.class_new_metadata.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (class, metadata) in &parts.class_new_metadata {
        write_class(&mut writer, *class);
        writer.bool(metadata.is_abstract);
        write_visibility(&mut writer, metadata.ctor_visibility);
        write_class(&mut writer, metadata.ctor_declaring_class);
        writer.bool(metadata.has_source_overloads);
    }
    write_len(&mut writer, parts.class_parents.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (class, parent) in &parts.class_parents {
        write_class(&mut writer, *class);
        write_class(&mut writer, *parent);
    }
    write_len(&mut writer, parts.class_value_aliases.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (alias, target) in &parts.class_value_aliases {
        writer.u32(alias.0);
        writer.u32(target.0);
    }
    write_len(&mut writer, parts.class_value_bindings.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (storage, binding) in &parts.class_value_bindings {
        writer.u32(storage.0);
        write_class(&mut writer, binding.class_id);
        writer.bool(binding.has_header_type_params);
    }
    write_len(&mut writer, parts.standalone_namespace_value_aliases.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (alias, target) in &parts.standalone_namespace_value_aliases {
        writer.u32(alias.0);
        writer.u32(target.0);
    }
    write_len(&mut writer, parts.class_names.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (class, name) in &parts.class_names {
        write_class(&mut writer, *class);
        writer
            .string(name)
            .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    }
    write_len(&mut writer, parts.named_function_symbols.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for symbol in &parts.named_function_symbols {
        writer.u32(symbol.0);
    }
    Ok(writer.into_bytes())
}

fn decode_class_metadata(
    bytes: &[u8],
    namespace_terminals: Vec<FrozenNamespaceValueTerminalSnapshotRow>,
) -> Result<FrozenCheckerRuntimeSnapshotParts, SnapshotError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported class metadata version",
        ));
    }
    let count = reader
        .collection_len(4)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut class_application_parameters = Vec::with_capacity(count);
    for _ in 0..count {
        let class =
            read_class(&mut reader).map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        let parameter_count = reader
            .collection_len(9)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        let mut parameters = Vec::with_capacity(parameter_count);
        for _ in 0..parameter_count {
            let id = read_type_param(&mut reader)
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
            let constraint = read_optional_u32(&mut reader)
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
                .map(TypeId);
            let default = match reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
            {
                0 => ClassTypeParameterDefault::Absent,
                1 => ClassTypeParameterDefault::Ready(
                    read_type_id(&mut reader)
                        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
                ),
                2 => ClassTypeParameterDefault::Unsupported(()),
                _ => {
                    return Err(invalid(
                        SnapshotErrorStage::Decode,
                        "invalid class parameter default",
                    ))
                }
            };
            parameters.push(DraftClassTypeParameterSnapshot {
                id,
                constraint,
                default,
            });
        }
        class_application_parameters.push((class, parameters));
    }
    let count = reader
        .collection_len(7)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut class_new_metadata = Vec::with_capacity(count);
    for _ in 0..count {
        let class =
            read_class(&mut reader).map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        class_new_metadata.push((
            class,
            PublishedClassNewMetadata {
                is_abstract: reader
                    .bool()
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
                ctor_visibility: read_visibility(&mut reader)
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
                ctor_declaring_class: read_class(&mut reader)
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
                has_source_overloads: reader
                    .bool()
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            },
        ));
    }
    let count = reader
        .collection_len(8)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut class_parents = Vec::with_capacity(count);
    for _ in 0..count {
        class_parents.push((
            read_class(&mut reader).map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            read_class(&mut reader).map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
        ));
    }
    let count = reader
        .collection_len(8)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut class_value_aliases = Vec::with_capacity(count);
    for _ in 0..count {
        class_value_aliases.push((
            ValueStorageId(
                reader
                    .u32()
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            ),
            ValueStorageId(
                reader
                    .u32()
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            ),
        ));
    }
    let count = reader
        .collection_len(9)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut class_value_bindings = Vec::with_capacity(count);
    for _ in 0..count {
        let storage = ValueStorageId(
            reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
        );
        class_value_bindings.push((
            storage,
            PublishedClassValueBinding {
                class_id: read_class(&mut reader)
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
                has_header_type_params: reader
                    .bool()
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            },
        ));
    }
    let count = reader
        .collection_len(8)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut standalone_namespace_value_aliases = Vec::with_capacity(count);
    for _ in 0..count {
        standalone_namespace_value_aliases.push((
            ValueStorageId(
                reader
                    .u32()
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            ),
            ValueStorageId(
                reader
                    .u32()
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            ),
        ));
    }
    let count = reader
        .collection_len(12)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut class_names = Vec::with_capacity(count);
    for _ in 0..count {
        class_names.push((
            read_class(&mut reader).map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            reader
                .string()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
                .to_owned(),
        ));
    }
    let count = reader
        .collection_len(4)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut named_function_symbols = Vec::with_capacity(count);
    for _ in 0..count {
        named_function_symbols.push(SymbolId(
            reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
        ));
    }
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    Ok(FrozenCheckerRuntimeSnapshotParts {
        class_application_parameters,
        class_new_metadata,
        class_parents,
        class_value_aliases,
        class_value_bindings,
        standalone_namespace_value_aliases,
        class_names,
        namespace_terminals,
        named_function_symbols,
    })
}

#[cfg(test)]
fn write_identity_terminal(
    writer: &mut SnapshotWriter,
    terminal: &LibraryIdentityTerminal,
) -> Result<(), SnapshotCodecError> {
    match terminal {
        LibraryIdentityTerminal::Unavailable(cause) => {
            writer.u8(0);
            writer.u8(match cause {
                LibraryIdentityUnavailable::MissingGlobal => 0,
                LibraryIdentityUnavailable::Unpublished => 1,
                LibraryIdentityUnavailable::UnsupportedSurface => 2,
                LibraryIdentityUnavailable::WrongArity => 3,
                LibraryIdentityUnavailable::ContainsError => 4,
            });
        }
        LibraryIdentityTerminal::Ready(identity) => {
            writer.u8(1);
            writer.u32(identity.group.0);
            write_type_id(writer, identity.template);
            write_len(writer, identity.parameters.len())?;
            for parameter in &identity.parameters {
                write_type_param(writer, *parameter);
            }
        }
    }
    Ok(())
}

fn read_identity_terminal(
    reader: &mut SnapshotReader<'_>,
) -> Result<LibraryIdentityTerminal, SnapshotCodecError> {
    match reader.u8()? {
        0 => Ok(LibraryIdentityTerminal::Unavailable(match reader.u8()? {
            0 => LibraryIdentityUnavailable::MissingGlobal,
            1 => LibraryIdentityUnavailable::Unpublished,
            2 => LibraryIdentityUnavailable::UnsupportedSurface,
            3 => LibraryIdentityUnavailable::WrongArity,
            4 => LibraryIdentityUnavailable::ContainsError,
            _ => {
                return Err(SnapshotCodecError::invalid(
                    reader.position() - 1,
                    "invalid semantic identity cause",
                ))
            }
        })),
        1 => {
            let group = TypeGroupId(reader.u32()?);
            let template = read_type_id(reader)?;
            let count = reader.collection_len(4)?;
            let mut parameters = Vec::with_capacity(count);
            for _ in 0..count {
                parameters.push(read_type_param(reader)?);
            }
            Ok(LibraryIdentityTerminal::Ready(LibraryTypeIdentity {
                group,
                template,
                parameters,
            }))
        }
        _ => Err(SnapshotCodecError::invalid(
            reader.position() - 1,
            "invalid semantic identity terminal",
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ManifestReference {
    owner_family: u8,
    owner_domain: u8,
    target_domain: u8,
    field: u8,
    owner: u32,
    target: u32,
}

struct ManifestInputs<'a> {
    type_count: usize,
    roots: &'a [RootNameRow],
    store_references: &'a [(u8, u8, u8, u32, u32)],
    interner_references: &'a [(u8, u8, u8, u32, u32)],
    binder_references: &'a [(u8, u8, u8, u32, u32)],
    decl_types: &'a [Option<TypeId>],
    published: &'a PublishedTypeEnvironmentSnapshotParts,
    namespace_terminals: &'a [FrozenNamespaceValueTerminalSnapshotRow],
    runtime: &'a FrozenCheckerRuntimeSnapshotParts,
    semantic_identities: &'a Option<[LibraryIdentityTerminal; 8]>,
}

fn build_manifest_references(
    inputs: ManifestInputs<'_>,
) -> Result<Vec<ManifestReference>, SnapshotCodecError> {
    let ManifestInputs {
        type_count,
        roots,
        store_references,
        interner_references,
        binder_references,
        decl_types,
        published,
        namespace_terminals,
        runtime,
        semantic_identities,
    } = inputs;
    let mut references = Vec::new();
    for owner in 0..type_count {
        let owner = id32(owner)?;
        references.push(ManifestReference {
            owner_family: 1,
            owner_domain: 1,
            target_domain: 1,
            field: ROW_IDENTITY_FIELD,
            owner,
            target: owner,
        });
    }
    for (family, rows) in [(1, store_references), (2, interner_references)] {
        references.extend(rows.iter().map(
            |&(owner_domain, target_domain, field, owner, target)| ManifestReference {
                owner_family: family,
                owner_domain,
                target_domain,
                field,
                owner,
                target,
            },
        ));
    }
    references.extend(binder_references.iter().map(
        |&(owner_domain, target_domain, field, owner, target)| ManifestReference {
            owner_family: 3,
            owner_domain,
            target_domain,
            field,
            owner,
            target,
        },
    ));

    for (owner, ty) in decl_types.iter().enumerate() {
        references.push(ManifestReference {
            owner_family: 4,
            owner_domain: 9,
            target_domain: 9,
            field: ROW_IDENTITY_FIELD,
            owner: id32(owner)?,
            target: id32(owner)?,
        });
        if let Some(ty) = ty {
            references.push(ManifestReference {
                owner_family: 4,
                owner_domain: 9,
                target_domain: 1,
                field: 0,
                owner: id32(owner)?,
                target: ty.0,
            });
        }
    }
    for (owner, group) in published.groups.iter().enumerate() {
        references.push(ManifestReference {
            owner_family: 5,
            owner_domain: 7,
            target_domain: 7,
            field: ROW_IDENTITY_FIELD,
            owner: id32(owner)?,
            target: id32(owner)?,
        });
        let PublishedTypeGroupTerminal::Ready(group) = group else {
            continue;
        };
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) => references.push(ManifestReference {
                owner_family: 5,
                owner_domain: 7,
                target_domain: 1,
                field: 0,
                owner: id32(owner)?,
                target: ty.0,
            }),
            PublishedTypeGroupSurface::Class(class) => references.push(ManifestReference {
                owner_family: 5,
                owner_domain: 7,
                target_domain: 3,
                field: 1,
                owner: id32(owner)?,
                target: class.0,
            }),
        }
        for parameter in &group.parameters {
            references.push(ManifestReference {
                owner_family: 5,
                owner_domain: 7,
                target_domain: 2,
                field: 2,
                owner: id32(owner)?,
                target: parameter.0,
            });
        }
        for default in &group.parameter_defaults {
            if let PublishedTypeParameterDefault::Ready(ty) = default {
                references.push(ManifestReference {
                    owner_family: 5,
                    owner_domain: 7,
                    target_domain: 1,
                    field: 3,
                    owner: id32(owner)?,
                    target: ty.0,
                });
            }
        }
        for alternative in &group.conflict_alternatives {
            for ty in &alternative.types {
                references.push(ManifestReference {
                    owner_family: 5,
                    owner_domain: 7,
                    target_domain: 1,
                    field: 4,
                    owner: id32(owner)?,
                    target: ty.0,
                });
            }
        }
    }
    for (class, terminal) in &published.classes {
        references.push(ManifestReference {
            owner_family: 5,
            owner_domain: 3,
            target_domain: 3,
            field: ROW_IDENTITY_FIELD,
            owner: class.0,
            target: class.0,
        });
        let PublishedClassSnapshotTerminal::Ready(surface) = terminal else {
            continue;
        };
        references.push(ManifestReference {
            owner_family: 5,
            owner_domain: 3,
            target_domain: 3,
            field: 5,
            owner: class.0,
            target: surface.class().0,
        });
        for parameter in surface.type_params() {
            references.push(ManifestReference {
                owner_family: 5,
                owner_domain: 3,
                target_domain: 2,
                field: 6,
                owner: class.0,
                target: parameter.0,
            });
        }
        for (field, ty) in [
            (7, Some(surface.instance_template())),
            (8, Some(surface.static_template())),
            (9, surface.constructor_template()),
        ] {
            if let Some(ty) = ty {
                references.push(ManifestReference {
                    owner_family: 5,
                    owner_domain: 3,
                    target_domain: 1,
                    field,
                    owner: class.0,
                    target: ty.0,
                });
            }
        }
    }
    for row in namespace_terminals {
        references.push(ManifestReference {
            owner_family: 6,
            owner_domain: 8,
            target_domain: 8,
            field: ROW_IDENTITY_FIELD,
            owner: row.namespace.0,
            target: row.namespace.0,
        });
        if let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal {
            references.push(ManifestReference {
                owner_family: 6,
                owner_domain: 8,
                target_domain: 9,
                field: 0,
                owner: row.namespace.0,
                target: storage.0,
            });
            references.push(ManifestReference {
                owner_family: 6,
                owner_domain: 8,
                target_domain: 1,
                field: 1,
                owner: row.namespace.0,
                target: ty.0,
            });
        }
    }
    for (class, parameters) in &runtime.class_application_parameters {
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 3,
            target_domain: 3,
            field: APPLICATION_ROW_FIELD,
            owner: class.0,
            target: class.0,
        });
        for parameter in parameters {
            references.push(ManifestReference {
                owner_family: 7,
                owner_domain: 3,
                target_domain: 2,
                field: 0,
                owner: class.0,
                target: parameter.id.0,
            });
            if let Some(ty) = parameter.constraint {
                references.push(ManifestReference {
                    owner_family: 7,
                    owner_domain: 3,
                    target_domain: 1,
                    field: 1,
                    owner: class.0,
                    target: ty.0,
                });
            }
            if let ClassTypeParameterDefault::Ready(ty) = parameter.default {
                references.push(ManifestReference {
                    owner_family: 7,
                    owner_domain: 3,
                    target_domain: 1,
                    field: 2,
                    owner: class.0,
                    target: ty.0,
                });
            }
        }
    }
    for (class, metadata) in &runtime.class_new_metadata {
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 3,
            target_domain: 3,
            field: NEW_METADATA_ROW_FIELD,
            owner: class.0,
            target: class.0,
        });
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 3,
            target_domain: 3,
            field: 3,
            owner: class.0,
            target: metadata.ctor_declaring_class.0,
        });
    }
    for (class, parent) in &runtime.class_parents {
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 3,
            target_domain: 3,
            field: PARENT_ROW_FIELD,
            owner: class.0,
            target: class.0,
        });
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 3,
            target_domain: 3,
            field: 4,
            owner: class.0,
            target: parent.0,
        });
    }
    for (alias, target) in &runtime.class_value_aliases {
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 9,
            target_domain: 9,
            field: CLASS_ALIAS_ROW_FIELD,
            owner: alias.0,
            target: alias.0,
        });
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 9,
            target_domain: 9,
            field: 5,
            owner: alias.0,
            target: target.0,
        });
    }
    for (storage, binding) in &runtime.class_value_bindings {
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 9,
            target_domain: 9,
            field: CLASS_BINDING_ROW_FIELD,
            owner: storage.0,
            target: storage.0,
        });
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 9,
            target_domain: 3,
            field: 6,
            owner: storage.0,
            target: binding.class_id.0,
        });
    }
    for (alias, target) in &runtime.standalone_namespace_value_aliases {
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 9,
            target_domain: 9,
            field: NAMESPACE_ALIAS_ROW_FIELD,
            owner: alias.0,
            target: alias.0,
        });
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 9,
            target_domain: 9,
            field: 7,
            owner: alias.0,
            target: target.0,
        });
    }
    for symbol in &runtime.named_function_symbols {
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 5,
            target_domain: 5,
            field: NAMED_FUNCTION_ROW_FIELD,
            owner: symbol.0,
            target: symbol.0,
        });
    }
    for (class, _) in &runtime.class_names {
        references.push(ManifestReference {
            owner_family: 7,
            owner_domain: 3,
            target_domain: 3,
            field: CLASS_NAME_ROW_FIELD,
            owner: class.0,
            target: class.0,
        });
    }
    if let Some(identities) = semantic_identities {
        for (owner, terminal) in identities.iter().enumerate() {
            let owner = id32(owner)?;
            references.push(ManifestReference {
                owner_family: 8,
                owner_domain: SEMANTIC_IDENTITY_DOMAIN,
                target_domain: SEMANTIC_IDENTITY_DOMAIN,
                field: ROW_IDENTITY_FIELD,
                owner,
                target: owner,
            });
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                continue;
            };
            references.push(ManifestReference {
                owner_family: 8,
                owner_domain: SEMANTIC_IDENTITY_DOMAIN,
                target_domain: 7,
                field: 0,
                owner,
                target: identity.group.0,
            });
            references.push(ManifestReference {
                owner_family: 8,
                owner_domain: SEMANTIC_IDENTITY_DOMAIN,
                target_domain: 1,
                field: 1,
                owner,
                target: identity.template.0,
            });
            for parameter in &identity.parameters {
                references.push(ManifestReference {
                    owner_family: 8,
                    owner_domain: SEMANTIC_IDENTITY_DOMAIN,
                    target_domain: 2,
                    field: 2,
                    owner,
                    target: parameter.0,
                });
            }
        }
    }
    for (owner, root) in roots.iter().enumerate() {
        let owner = u32::try_from(owner)
            .map_err(|_| SnapshotCodecError::invalid(0, "root manifest owner exceeds u32"))?;
        references.push(ManifestReference {
            owner_family: 9,
            owner_domain: 17,
            target_domain: 17,
            field: ROW_IDENTITY_FIELD,
            owner,
            target: owner,
        });
        if let Some(symbol) = root.symbol {
            references.push(ManifestReference {
                owner_family: 9,
                owner_domain: 17,
                target_domain: 5,
                field: 0,
                owner,
                target: symbol.0,
            });
        }
        if let Some(value) = root.value {
            references.push(ManifestReference {
                owner_family: 9,
                owner_domain: 17,
                target_domain: 9,
                field: 1,
                owner,
                target: value.0,
            });
        }
        if let Some(ty) = root.ty {
            references.push(ManifestReference {
                owner_family: 9,
                owner_domain: 17,
                target_domain: 7,
                field: 2,
                owner,
                target: ty.0,
            });
        }
        if let Some(namespace) = root.namespace {
            references.push(ManifestReference {
                owner_family: 9,
                owner_domain: 17,
                target_domain: 8,
                field: 3,
                owner,
                target: namespace.0,
            });
        }
    }
    references.sort_unstable();
    Ok(references)
}

struct TailManifestInputs<'a> {
    roots: &'a [RootNameRow],
    decl_types: &'a [Option<TypeId>],
    published: &'a PublishedTypeEnvironmentSnapshotParts,
    namespace_terminals: &'a [FrozenNamespaceValueTerminalSnapshotRow],
    runtime: &'a FrozenCheckerRuntimeSnapshotParts,
    semantic_identities: &'a Option<[LibraryIdentityTerminal; 8]>,
}

fn build_tail_manifest_references(
    inputs: TailManifestInputs<'_>,
) -> Result<Vec<ManifestReference>, SnapshotCodecError> {
    build_manifest_references(ManifestInputs {
        type_count: 0,
        roots: inputs.roots,
        store_references: &[],
        interner_references: &[],
        binder_references: &[],
        decl_types: inputs.decl_types,
        published: inputs.published,
        namespace_terminals: inputs.namespace_terminals,
        runtime: inputs.runtime,
        semantic_identities: inputs.semantic_identities,
    })
}

fn typed_validation_identity(
    next: &NextIds,
    source_file_count: u32,
    roots: &[RootNameRow],
    store_references: &[(u8, u8, u8, u32, u32)],
    interner_references: &[(u8, u8, u8, u32, u32)],
    binder_references: &[(u8, u8, u8, u32, u32)],
    tail_references: &[ManifestReference],
) -> Result<[u8; 32], SnapshotError> {
    let mut digest = Sha256::new();
    digest.update(b"typokat-typed-projection-v1");
    digest.update(source_file_count.to_be_bytes());
    for count in [
        next.types,
        next.type_params,
        next.classes,
        next.scopes,
        next.symbols,
        next.declarations,
        next.type_groups,
        next.namespaces,
        next.value_storages,
    ] {
        digest.update(
            u64::try_from(count)
                .map_err(|_| {
                    invalid(
                        SnapshotErrorStage::ReferenceValidation,
                        "typed projection count exceeds u64",
                    )
                })?
                .to_be_bytes(),
        );
    }
    for root in roots {
        digest.update(
            u64::try_from(root.name.len())
                .map_err(|_| {
                    invalid(
                        SnapshotErrorStage::ReferenceValidation,
                        "typed projection root name exceeds u64",
                    )
                })?
                .to_be_bytes(),
        );
        digest.update(root.name.as_bytes());
        for identity in [
            root.symbol.map(|id| id.0),
            root.value.map(|id| id.0),
            root.ty.map(|id| id.0),
            root.namespace.map(|id| id.0),
        ] {
            digest.update(identity.unwrap_or(ABSENT_ID).to_be_bytes());
        }
    }

    let mut identity = 0usize;
    let mut store = 0usize;
    while identity < next.types || store < store_references.len() {
        let identity_reference = if identity < next.types {
            let id = u32::try_from(identity).map_err(|_| {
                invalid(
                    SnapshotErrorStage::ReferenceValidation,
                    "typed projection TypeId exceeds u32",
                )
            })?;
            Some(ManifestReference {
                owner_family: 1,
                owner_domain: 1,
                target_domain: 1,
                field: ROW_IDENTITY_FIELD,
                owner: id,
                target: id,
            })
        } else {
            None
        };
        let store_reference = store_references
            .get(store)
            .copied()
            .map(|row| tuple_manifest_reference(1, row));
        let reference = match (identity_reference, store_reference) {
            (Some(identity_reference), Some(store_reference)) => {
                if identity_reference <= store_reference {
                    identity += 1;
                    identity_reference
                } else {
                    store += 1;
                    store_reference
                }
            }
            (Some(reference), None) => {
                identity += 1;
                reference
            }
            (None, Some(reference)) => {
                store += 1;
                reference
            }
            (None, None) => {
                return Err(invalid(
                    SnapshotErrorStage::ReferenceValidation,
                    "typed projection reference merge ended unexpectedly",
                ));
            }
        };
        update_typed_validation_reference(&mut digest, reference);
    }
    for &reference in interner_references {
        update_typed_validation_reference(&mut digest, tuple_manifest_reference(2, reference));
    }
    for &reference in binder_references {
        update_typed_validation_reference(&mut digest, tuple_manifest_reference(3, reference));
    }
    for &reference in tail_references {
        update_typed_validation_reference(&mut digest, reference);
    }
    Ok(digest.finalize().into())
}

fn validate_dense_identity_prefixes(
    next: &NextIds,
    interner: &Interner,
    published: &PublishedTypeEnvironmentSnapshotParts,
) -> Result<(), SnapshotError> {
    let mut type_parameters = interner
        .store()
        .snapshot_type_param_ids()
        .collect::<Vec<_>>();
    type_parameters.sort_unstable();
    if type_parameters.len() != next.type_params
        || type_parameters
            .iter()
            .enumerate()
            .any(|(index, parameter)| usize::try_from(parameter.0) != Ok(index))
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "frozen type-parameter identities do not form the declared dense prefix",
        ));
    }
    if published.classes.len() != next.classes
        || published
            .classes
            .iter()
            .enumerate()
            .any(|(index, (class, _))| usize::try_from(class.0) != Ok(index))
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "published class identities do not form the declared dense prefix",
        ));
    }
    Ok(())
}

fn update_typed_validation_reference(digest: &mut Sha256, reference: ManifestReference) {
    digest.update([
        reference.owner_family,
        reference.owner_domain,
        reference.target_domain,
        reference.field,
    ]);
    digest.update(reference.owner.to_be_bytes());
    digest.update(reference.target.to_be_bytes());
}

#[cfg(test)]
fn write_manifest(
    writer: &mut SnapshotWriter,
    references: &[ManifestReference],
) -> Result<[u64; 9], SnapshotCodecError> {
    let mut counts = [0u64; 9];
    for reference in references {
        counts[usize::from(reference.owner_family - 1)] += 1;
    }
    writer.u32(1);
    writer.u64(9);
    writer.usize(references.len())?;
    for (index, count) in counts.iter().enumerate() {
        writer.u16(u16::try_from(index + 1).expect("family tag"));
        writer.u16(0);
        writer.u64(*count);
    }
    for reference in references {
        writer.u8(reference.owner_family);
        writer.u8(reference.owner_domain);
        writer.u8(reference.target_domain);
        writer.u8(reference.field);
        writer.u32(reference.owner);
        writer.u32(reference.target);
    }
    Ok(counts)
}

#[cfg(test)]
struct EncodedSemanticSection {
    bytes: Vec<u8>,
    reference_counts: [u64; 9],
    manifest_hash: [u8; 32],
}

#[cfg(test)]
fn encode_semantic_identities(
    parts: &Option<[LibraryIdentityTerminal; 8]>,
    references: &[ManifestReference],
    projection_subtables: &[RuntimeSubtableForTest],
) -> Result<EncodedSemanticSection, SnapshotError> {
    let mut writer = SnapshotWriter::new();
    let counts = write_manifest(&mut writer, references)
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    let manifest_len = writer.position();
    writer.bool(parts.is_some());
    if let Some(terminals) = parts {
        for terminal in terminals {
            write_identity_terminal(&mut writer, terminal)
                .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
        }
    }
    write_projection_witness(&mut writer, projection_subtables)?;
    let bytes = writer.into_bytes();
    let manifest_hash = digest32(&bytes[..manifest_len]);
    Ok(EncodedSemanticSection {
        bytes,
        reference_counts: counts,
        manifest_hash,
    })
}

struct DecodedSemanticSection {
    identities: Option<[LibraryIdentityTerminal; 8]>,
    #[cfg(test)]
    reference_counts: [u64; 9],
    #[cfg(test)]
    manifest_hash: [u8; 32],
    #[cfg(test)]
    projection_subtables: Vec<RuntimeSubtableForTest>,
}

#[cfg(test)]
fn projection_subtable_names() -> impl Iterator<Item = &'static str> {
    SUBTABLE_NAMES
        .into_iter()
        .chain(std::iter::once("next-ids"))
}

#[cfg(test)]
fn write_projection_witness(
    writer: &mut SnapshotWriter,
    subtables: &[RuntimeSubtableForTest],
) -> Result<(), SnapshotError> {
    if subtables.len() != PROJECTION_WITNESS_COUNT
        || projection_subtable_names()
            .zip(subtables)
            .any(|(expected, actual)| expected != actual.name)
    {
        return Err(invalid(
            SnapshotErrorStage::Generation,
            "projection witness inventory is not exact",
        ));
    }
    writer.u32(PROJECTION_WITNESS_VERSION);
    writer.u32(u32::try_from(PROJECTION_WITNESS_COUNT).expect("witness count fits u32"));
    for subtable in subtables {
        writer.u64(subtable.row_count);
        writer.raw(&subtable.sha256);
    }
    Ok(())
}

#[cfg(test)]
fn read_projection_witness(
    reader: &mut SnapshotReader<'_>,
) -> Result<Vec<RuntimeSubtableForTest>, SnapshotError> {
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != PROJECTION_WITNESS_VERSION
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported projection witness version",
        ));
    }
    let count = usize::try_from(
        reader
            .u32()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
    )
    .map_err(|_| invalid(SnapshotErrorStage::Decode, "witness count exceeds usize"))?;
    if count != PROJECTION_WITNESS_COUNT {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "projection witness inventory is not exact",
        ));
    }
    projection_subtable_names()
        .map(|name| {
            let row_count = reader
                .u64()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
            let mut sha256 = [0; 32];
            sha256.copy_from_slice(
                reader
                    .raw(32)
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            );
            Ok(RuntimeSubtableForTest {
                name,
                row_count,
                sha256,
            })
        })
        .collect()
}

#[cfg(not(test))]
fn read_projection_witness(reader: &mut SnapshotReader<'_>) -> Result<(), SnapshotError> {
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != PROJECTION_WITNESS_VERSION
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported projection witness version",
        ));
    }
    let count = usize::try_from(
        reader
            .u32()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
    )
    .map_err(|_| invalid(SnapshotErrorStage::Decode, "witness count exceeds usize"))?;
    if count != PROJECTION_WITNESS_COUNT {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "projection witness inventory is not exact",
        ));
    }
    for _ in 0..PROJECTION_WITNESS_COUNT {
        reader
            .u64()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        reader
            .raw(32)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    }
    Ok(())
}

struct ReferenceLimits {
    domains: [usize; 31],
    container_owners: BTreeMap<(u8, u8), usize>,
}

impl ReferenceLimits {
    fn from_canonical_references(
        next: &NextIds,
        roots: &[RootNameRow],
        store_references: &[(u8, u8, u8, u32, u32)],
        interner_references: &[(u8, u8, u8, u32, u32)],
        binder_references: &[(u8, u8, u8, u32, u32)],
    ) -> Result<Self, SnapshotError> {
        let mut domains = [0usize; 31];
        for (domain, limit) in [
            (1, next.types),
            (2, next.type_params),
            (3, next.classes),
            (4, next.scopes),
            (5, next.symbols),
            (6, next.declarations),
            (7, next.type_groups),
            (8, next.namespaces),
            (9, next.value_storages),
            (17, roots.len()),
            (usize::from(SEMANTIC_IDENTITY_DOMAIN), 8),
        ] {
            domains[domain] = limit;
        }
        let mut container_owners: BTreeMap<(u8, u8), usize> = BTreeMap::new();
        for (family, references) in [
            (1, store_references),
            (2, interner_references),
            (3, binder_references),
        ] {
            for &(owner_domain, target_domain, field, owner, target) in references {
                if owner_domain == 0 {
                    let limit = usize::try_from(owner)
                        .ok()
                        .and_then(|owner| owner.checked_add(1))
                        .ok_or_else(|| {
                            invalid(
                                SnapshotErrorStage::ReferenceValidation,
                                "container owner range overflow",
                            )
                        })?;
                    container_owners
                        .entry((family, field))
                        .and_modify(|current| *current = (*current).max(limit))
                        .or_insert(limit);
                } else if owner_domain > 9 {
                    let domain = usize::from(owner_domain);
                    let limit = usize::try_from(owner)
                        .ok()
                        .and_then(|owner| owner.checked_add(1))
                        .ok_or_else(|| {
                            invalid(
                                SnapshotErrorStage::ReferenceValidation,
                                "owner domain range overflow",
                            )
                        })?;
                    let current = domains.get_mut(domain).ok_or_else(|| {
                        invalid(
                            SnapshotErrorStage::ReferenceValidation,
                            "owner reference domain exceeds the schema",
                        )
                    })?;
                    *current = (*current).max(limit);
                }
                if target_domain > 9 {
                    let domain = usize::from(target_domain);
                    let limit = usize::try_from(target)
                        .ok()
                        .and_then(|target| target.checked_add(1))
                        .ok_or_else(|| {
                            invalid(
                                SnapshotErrorStage::ReferenceValidation,
                                "target domain range overflow",
                            )
                        })?;
                    let current = domains.get_mut(domain).ok_or_else(|| {
                        invalid(
                            SnapshotErrorStage::ReferenceValidation,
                            "target reference domain exceeds the schema",
                        )
                    })?;
                    *current = (*current).max(limit);
                }
            }
        }
        Ok(Self {
            domains,
            container_owners,
        })
    }

    fn owner_limit(&self, reference: ManifestReference) -> Option<usize> {
        if reference.owner_domain == 0 {
            self.container_owners
                .get(&(reference.owner_family, reference.field))
                .copied()
        } else {
            self.domains
                .get(usize::from(reference.owner_domain))
                .copied()
        }
    }

    fn target_limit(&self, domain: u8) -> Option<usize> {
        self.domains.get(usize::from(domain)).copied()
    }
}

#[cfg(test)]
fn decode_semantic_identities(
    bytes: &[u8],
    limits: &ReferenceLimits,
) -> Result<DecodedSemanticSection, SnapshotError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "unsupported manifest version",
        ));
    }
    if reader
        .u64()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
        != 9
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "wrong reference family count",
        ));
    }
    let reference_count = reader
        .usize()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let mut counts = [0u64; 9];
    for (index, count) in counts.iter_mut().enumerate() {
        let expected_family = u16::try_from(index + 1).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family index exceeds u16",
            )
        })?;
        if reader
            .u16()
            .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
            != expected_family
            || reader
                .u16()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
                != 0
        {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "invalid reference family directory",
            ));
        }
        *count = reader
            .u64()
            .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    }
    let family_total = counts.iter().try_fold(0u64, |total, count| {
        total.checked_add(*count).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family count sum overflow",
            )
        })
    })?;
    if family_total
        != u64::try_from(reference_count).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference count overflow",
            )
        })?
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference counts disagree",
        ));
    }
    let mut previous = None;
    let mut actual = [0u64; 9];
    for _ in 0..reference_count {
        let reference = ManifestReference {
            owner_family: reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            owner_domain: reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            target_domain: reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            field: reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            owner: reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            target: reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
        };
        if !(1..=9).contains(&reference.owner_family)
            || reference.owner_domain > MAX_REFERENCE_DOMAIN
            || reference.target_domain == 0
            || reference.target_domain > MAX_REFERENCE_DOMAIN
            || reference.field > 31
        {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "invalid reference discriminant",
            ));
        }
        if previous.is_some_and(|previous| previous > reference) {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference manifest is not sorted",
            ));
        }
        previous = Some(reference);
        actual[usize::from(reference.owner_family - 1)] += 1;
        let owner_limit = limits.owner_limit(reference).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "unknown manifest owner domain",
            )
        })?;
        validate_id(reference.owner, owner_limit, "manifest owner")?;
        let target_limit = limits
            .target_limit(reference.target_domain)
            .ok_or_else(|| {
                invalid(
                    SnapshotErrorStage::ReferenceValidation,
                    "unknown manifest target domain",
                )
            })?;
        validate_id(reference.target, target_limit, "manifest target")?;
    }
    if actual != counts {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference family counts are not canonical",
        ));
    }
    let manifest_end = reader.position();
    let manifest_hash = digest32(&bytes[..manifest_end]);
    let identities = if reader
        .bool()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
    {
        let mut values = Vec::with_capacity(8);
        for _ in 0..8 {
            if bytes.get(reader.position()) == Some(&0) {
                return Err(invalid(
                    SnapshotErrorStage::Publication,
                    "semantic identity publication is not complete",
                ));
            }
            values.push(
                read_identity_terminal(&mut reader)
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            );
        }
        Some(values.try_into().map_err(|_| {
            invalid(
                SnapshotErrorStage::Decode,
                "semantic identity count is not canonical",
            )
        })?)
    } else {
        None
    };
    let projection_subtables = read_projection_witness(&mut reader)?;
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    Ok(DecodedSemanticSection {
        identities,
        #[cfg(test)]
        reference_counts: counts,
        #[cfg(test)]
        manifest_hash,
        #[cfg(test)]
        projection_subtables,
    })
}

fn decode_canonical_semantic_section(
    bytes: &[u8],
) -> Result<DecodedSemanticSection, SnapshotError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "unsupported manifest version",
        ));
    }
    if reader
        .u64()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
        != 9
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "wrong reference family count",
        ));
    }
    let reference_count = reader
        .usize()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let mut counts = [0u64; 9];
    for (index, count) in counts.iter_mut().enumerate() {
        let expected_family = u16::try_from(index + 1).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family index exceeds u16",
            )
        })?;
        if reader
            .u16()
            .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
            != expected_family
            || reader
                .u16()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
                != 0
        {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "invalid reference family directory",
            ));
        }
        *count = reader
            .u64()
            .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    }
    let family_total = counts.iter().try_fold(0u64, |total, count| {
        total.checked_add(*count).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family count sum overflow",
            )
        })
    })?;
    if family_total != u64::try_from(reference_count).unwrap_or(u64::MAX) {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference counts disagree",
        ));
    }
    let manifest_bytes = reference_count.checked_mul(12).ok_or_else(|| {
        invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference manifest length overflow",
        )
    })?;
    reader
        .raw(manifest_bytes)
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    #[cfg(test)]
    let manifest_end = reader.position();
    #[cfg(test)]
    let manifest_hash = digest32(&bytes[..manifest_end]);
    let identities = if reader
        .bool()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
    {
        let mut values = Vec::with_capacity(8);
        for _ in 0..8 {
            if bytes.get(reader.position()) == Some(&0) {
                return Err(invalid(
                    SnapshotErrorStage::Publication,
                    "semantic identity publication is not complete",
                ));
            }
            values.push(
                read_identity_terminal(&mut reader)
                    .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
            );
        }
        Some(values.try_into().map_err(|_| {
            invalid(
                SnapshotErrorStage::Decode,
                "semantic identity count is not canonical",
            )
        })?)
    } else {
        None
    };
    #[cfg(test)]
    let projection_subtables = read_projection_witness(&mut reader)?;
    #[cfg(not(test))]
    read_projection_witness(&mut reader)?;
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    Ok(DecodedSemanticSection {
        identities,
        #[cfg(test)]
        reference_counts: counts,
        #[cfg(test)]
        manifest_hash,
        #[cfg(test)]
        projection_subtables,
    })
}

fn tuple_manifest_reference(family: u8, row: (u8, u8, u8, u32, u32)) -> ManifestReference {
    ManifestReference {
        owner_family: family,
        owner_domain: row.0,
        target_domain: row.1,
        field: row.2,
        owner: row.3,
        target: row.4,
    }
}

struct ManifestStreamVerifier<'a, 'limits> {
    reader: SnapshotReader<'a>,
    limits: &'limits ReferenceLimits,
    previous: Option<ManifestReference>,
    actual: [u64; 9],
}

impl ManifestStreamVerifier<'_, '_> {
    fn verify_expected(&mut self, expected: ManifestReference) -> Result<(), SnapshotError> {
        let reference = ManifestReference {
            owner_family: self
                .reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            owner_domain: self
                .reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            target_domain: self
                .reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            field: self
                .reader
                .u8()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            owner: self
                .reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
            target: self
                .reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?,
        };
        if !(1..=9).contains(&reference.owner_family)
            || reference.owner_domain > MAX_REFERENCE_DOMAIN
            || reference.target_domain == 0
            || reference.target_domain > MAX_REFERENCE_DOMAIN
            || reference.field > 31
        {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "invalid reference discriminant",
            ));
        }
        if self.previous.is_some_and(|previous| previous > reference) {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference manifest is not sorted",
            ));
        }
        self.previous = Some(reference);
        let family_index = usize::from(reference.owner_family - 1);
        self.actual[family_index] = self.actual[family_index].checked_add(1).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family count overflows u64",
            )
        })?;
        let owner_limit = self.limits.owner_limit(reference).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "unknown manifest owner domain",
            )
        })?;
        validate_id(reference.owner, owner_limit, "manifest owner")?;
        let target_limit = self
            .limits
            .target_limit(reference.target_domain)
            .ok_or_else(|| {
                invalid(
                    SnapshotErrorStage::ReferenceValidation,
                    "unknown manifest target domain",
                )
            })?;
        validate_id(reference.target, target_limit, "manifest target")?;
        if reference != expected {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference manifest disagrees with decoded state",
            ));
        }
        Ok(())
    }
}

fn verify_reference_manifest_streaming(
    bytes: &[u8],
    limits: &ReferenceLimits,
    type_count: usize,
    store_references: &[(u8, u8, u8, u32, u32)],
    interner_references: &[(u8, u8, u8, u32, u32)],
    binder_references: &[(u8, u8, u8, u32, u32)],
    tail_references: &[ManifestReference],
) -> Result<(), SnapshotError> {
    for references in [store_references, interner_references, binder_references] {
        if references.windows(2).any(|rows| rows[0] > rows[1]) {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "decoded reference records are not sorted",
            ));
        }
    }
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
        != 1
        || reader
            .u64()
            .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?
            != 9
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "unsupported reference manifest header",
        ));
    }
    let reference_count = reader
        .usize()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let mut expected_counts = [0u64; 9];
    expected_counts[0] = u64::try_from(type_count)
        .ok()
        .and_then(|count| count.checked_add(u64::try_from(store_references.len()).ok()?))
        .ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family count overflow",
            )
        })?;
    expected_counts[1] = u64::try_from(interner_references.len()).map_err(|_| {
        invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference family count overflow",
        )
    })?;
    expected_counts[2] = u64::try_from(binder_references.len()).map_err(|_| {
        invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference family count overflow",
        )
    })?;
    for reference in tail_references {
        let family_index = usize::from(reference.owner_family - 1);
        let count = expected_counts.get_mut(family_index).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "tail reference family is outside the manifest",
            )
        })?;
        *count = count.checked_add(1).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family count overflows u64",
            )
        })?;
    }
    for (index, expected) in expected_counts.iter().enumerate() {
        let expected_family = u16::try_from(index + 1).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family index exceeds u16",
            )
        })?;
        let family = reader
            .u16()
            .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
        let reserved = reader
            .u16()
            .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
        let count = reader
            .u64()
            .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
        if family != expected_family || reserved != 0 || count != *expected {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference family counts are not canonical",
            ));
        }
    }
    let expected_total = expected_counts.iter().try_fold(0u64, |total, count| {
        total.checked_add(*count).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "reference count overflow",
            )
        })
    })?;
    if usize::try_from(expected_total).ok() != Some(reference_count) {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference counts disagree",
        ));
    }
    let mut verifier = ManifestStreamVerifier {
        reader,
        limits,
        previous: None,
        actual: [0; 9],
    };
    let mut identity = 0usize;
    let mut store = 0usize;
    while identity < type_count || store < store_references.len() {
        let identity_reference = if identity < type_count {
            let id = u32::try_from(identity).map_err(|_| {
                invalid(
                    SnapshotErrorStage::ReferenceValidation,
                    "type identity exceeds u32",
                )
            })?;
            Some(ManifestReference {
                owner_family: 1,
                owner_domain: 1,
                target_domain: 1,
                field: ROW_IDENTITY_FIELD,
                owner: id,
                target: id,
            })
        } else {
            None
        };
        let store_reference = store_references
            .get(store)
            .copied()
            .map(|row| tuple_manifest_reference(1, row));
        let expected = match (identity_reference, store_reference) {
            (Some(identity_reference), Some(store_reference)) => {
                if identity_reference <= store_reference {
                    identity += 1;
                    identity_reference
                } else {
                    store += 1;
                    store_reference
                }
            }
            (Some(identity_reference), None) => {
                identity += 1;
                identity_reference
            }
            (None, Some(store_reference)) => {
                store += 1;
                store_reference
            }
            (None, None) => {
                return Err(invalid(
                    SnapshotErrorStage::ReferenceValidation,
                    "reference manifest merge ended unexpectedly",
                ));
            }
        };
        verifier.verify_expected(expected)?;
    }
    for &row in interner_references {
        verifier.verify_expected(tuple_manifest_reference(2, row))?;
    }
    for &row in binder_references {
        verifier.verify_expected(tuple_manifest_reference(3, row))?;
    }
    for &reference in tail_references {
        verifier.verify_expected(reference)?;
    }
    if verifier.actual != expected_counts {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference family counts are not canonical",
        ));
    }
    Ok(())
}

pub(in crate::check::checker) fn collect_root_rows(
    binder: &Binder,
) -> Result<Vec<RootNameRow>, SnapshotError> {
    let scope = binder.graph.get(binder.compilation_global).ok_or_else(|| {
        invalid(
            SnapshotErrorStage::ReferenceValidation,
            "missing compilation-global scope",
        )
    })?;
    let mut rows = scope
        .symbols
        .iter()
        .map(|(name, symbol)| {
            let record = binder.symbols.get(*symbol).ok_or_else(|| {
                invalid(
                    SnapshotErrorStage::ReferenceValidation,
                    "global symbol id is missing",
                )
            })?;
            Ok(RootNameRow {
                name: name.clone(),
                symbol: Some(*symbol),
                value: record.value,
                ty: record.ty,
                namespace: record.ns,
            })
        })
        .collect::<Result<Vec<_>, SnapshotError>>()?;
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(rows)
}

fn encode_root_index(rows: &[RootNameRow]) -> Result<Vec<u8>, SnapshotError> {
    let mut writer = SnapshotWriter::new();
    writer.u32(1);
    writer
        .usize(rows.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for row in rows {
        let name_len = u32::try_from(row.name.len())
            .map_err(|_| invalid(SnapshotErrorStage::Generation, "root name exceeds u32"))?;
        writer.u32(name_len);
        writer.raw(row.name.as_bytes());
        let mask = u8::from(row.symbol.is_some())
            | (u8::from(row.value.is_some()) << 1)
            | (u8::from(row.ty.is_some()) << 2)
            | (u8::from(row.namespace.is_some()) << 3);
        writer.u8(mask);
        write_root_optional_u32(&mut writer, row.symbol.map(|id| id.0));
        write_root_optional_u32(&mut writer, row.value.map(|id| id.0));
        write_root_optional_u32(&mut writer, row.ty.map(|id| id.0));
        write_root_optional_u32(&mut writer, row.namespace.map(|id| id.0));
    }
    Ok(writer.into_bytes())
}

pub(crate) struct SourceBinderCheckpointDigests {
    pub(crate) binder: [u8; 32],
    pub(crate) roots: [u8; 32],
    pub(crate) retained_scope_maps: [u8; 32],
}

pub(crate) fn source_binder_checkpoint_digests(
    binder: &Binder,
) -> Result<SourceBinderCheckpointDigests, SnapshotError> {
    let binder_bytes = encode_binder_snapshot(binder)
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    let roots = collect_root_rows(binder)?;
    let root_bytes = encode_root_index(&roots)?;
    let retained_scope_maps = crate::binder::snapshot::encode_retained_scope_maps(binder)
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    Ok(SourceBinderCheckpointDigests {
        binder: digest32(&binder_bytes),
        roots: digest32(&root_bytes),
        retained_scope_maps: digest32(&retained_scope_maps),
    })
}

fn decode_root_index(bytes: &[u8], next: &NextIds) -> Result<Vec<RootNameRow>, SnapshotError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported root index version",
        ));
    }
    let count = reader
        .usize()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let mut rows = Vec::with_capacity(count);
    let mut previous: Option<String> = None;
    for _ in 0..count {
        let name_len = usize::try_from(
            reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
        )
        .map_err(|_| invalid(SnapshotErrorStage::Decode, "root name length exceeds usize"))?;
        let name = std::str::from_utf8(
            reader
                .raw(name_len)
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
        )
        .map_err(|_| invalid(SnapshotErrorStage::Decode, "root name is not UTF-8"))?
        .to_owned();
        if previous.as_ref().is_some_and(|previous| previous >= &name) {
            return Err(invalid(
                SnapshotErrorStage::Decode,
                "root names are not strictly ordered",
            ));
        }
        previous = Some(name.clone());
        let mask = reader
            .u8()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        if mask & !0x0f != 0 {
            return Err(invalid(
                SnapshotErrorStage::Decode,
                "invalid root slot mask",
            ));
        }
        let symbol = read_root_optional_u32(&mut reader)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
            .map(SymbolId);
        let value = read_root_optional_u32(&mut reader)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
            .map(ValueStorageId);
        let ty = read_root_optional_u32(&mut reader)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
            .map(TypeGroupId);
        let namespace = read_root_optional_u32(&mut reader)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
            .map(NamespaceId);
        let actual = u8::from(symbol.is_some())
            | (u8::from(value.is_some()) << 1)
            | (u8::from(ty.is_some()) << 2)
            | (u8::from(namespace.is_some()) << 3);
        if mask != actual {
            return Err(invalid(
                SnapshotErrorStage::ReferenceValidation,
                "root slot mask disagrees with identities",
            ));
        }
        if let Some(id) = symbol {
            validate_id(id.0, next.symbols, "root symbol")?;
        }
        if let Some(id) = value {
            validate_id(id.0, next.value_storages, "root value")?;
        }
        if let Some(id) = ty {
            validate_id(id.0, next.type_groups, "root type group")?;
        }
        if let Some(id) = namespace {
            validate_id(id.0, next.namespaces, "root namespace")?;
        }
        rows.push(RootNameRow {
            name,
            symbol,
            value,
            ty,
            namespace,
        });
    }
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    Ok(rows)
}

#[cfg(test)]
fn encode_next_ids(next: &NextIds, source_file_count: u32) -> Vec<u8> {
    let mut writer = SnapshotWriter::new();
    writer.u32(1);
    writer.u32(source_file_count);
    for value in [
        next.types,
        next.type_params,
        next.classes,
        next.scopes,
        next.symbols,
        next.declarations,
        next.type_groups,
        next.namespaces,
        next.value_storages,
    ] {
        writer.u64(u64::try_from(value).expect("runtime prefix fits u64"));
    }
    writer.into_bytes()
}

fn decode_next_ids(bytes: &[u8]) -> Result<(NextIds, u32), SnapshotError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported next-id version",
        ));
    }
    let source_file_count = reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    if source_file_count == 0 {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "snapshot has no library sources",
        ));
    }
    let mut values = [0usize; 9];
    for value in &mut values {
        *value = usize::try_from(
            reader
                .u64()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?,
        )
        .map_err(|_| invalid(SnapshotErrorStage::Decode, "runtime prefix exceeds usize"))?;
    }
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let [types, type_params, classes, scopes, symbols, declarations, type_groups, namespaces, value_storages] =
        values;
    if types == 0 || scopes == 0 {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "required runtime prefix is empty",
        ));
    }
    Ok((
        NextIds {
            store: types,
            types,
            type_params,
            classes,
            scopes,
            symbols,
            declarations,
            type_groups,
            namespaces,
            value_storages,
        },
        source_file_count,
    ))
}

#[cfg(test)]
fn root_counts(rows: &[RootNameRow]) -> [u64; 4] {
    [
        count64(rows.iter().filter(|row| row.symbol.is_some()).count()),
        count64(rows.iter().filter(|row| row.value.is_some()).count()),
        count64(rows.iter().filter(|row| row.ty.is_some()).count()),
        count64(rows.iter().filter(|row| row.namespace.is_some()).count()),
    ]
}

#[cfg(test)]
fn identity_witness(
    rows: &[RootNameRow],
    published: &PublishedTypeEnvironmentSnapshotParts,
) -> IdentityWitnessForTest {
    let mut witness = IdentityWitnessForTest::default();
    for row in rows {
        let Some(group_id) = row.ty else {
            continue;
        };
        witness.groups.insert(row.name.clone(), group_id);
        if let Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
            surface: PublishedTypeGroupSurface::Class(class),
            ..
        })) = published.groups.get(group_id.index())
        {
            witness.classes.insert(row.name.clone(), *class);
        }
    }
    witness
}

#[cfg(test)]
fn debug_bytes<T: fmt::Debug + ?Sized>(value: &T) -> Vec<u8> {
    format!("{value:#?}").into_bytes()
}

#[cfg(test)]
fn debug_rows<T: fmt::Debug>(rows: &[T]) -> Vec<u8> {
    if rows.is_empty() {
        Vec::new()
    } else {
        debug_bytes(rows)
    }
}

#[cfg(test)]
fn store_projection_subtables(store: &Store) -> Vec<(u64, Vec<u8>)> {
    let mut rows = Vec::new();
    let mut payloads = Vec::new();
    let mut payload_count = 0usize;
    for index in 0..store.len() {
        let id = TypeId(u32::try_from(index).expect("snapshot TypeId fits u32"));
        let tag = store.tag(id);
        rows.extend_from_slice(format!("{id:?}:{tag:?}:{:?}:", store.flags(id)).as_bytes());
        let payload = match tag {
            TypeTag::Intrinsic => store.intrinsic_kind(id).map(|value| debug_bytes(&value)),
            TypeTag::Literal => store.literal_value(id).map(debug_bytes),
            TypeTag::Object => store.object_type(id).map(debug_bytes),
            TypeTag::Union => store.union_members(id).map(debug_bytes),
            TypeTag::Intersection => store.intersection_members(id).map(debug_bytes),
            TypeTag::Function => store.function_type(id).map(debug_bytes),
            TypeTag::TypeParam => store.type_param(id).map(debug_bytes),
            TypeTag::Array => store.array_type(id).map(debug_bytes),
            TypeTag::Tuple => store.tuple_type(id).map(debug_bytes),
            TypeTag::Readonly => store.readonly_operand(id).map(|value| debug_bytes(&value)),
            TypeTag::Conditional => store.conditional_type(id).map(debug_bytes),
            TypeTag::Instantiation => store.instantiation_type(id).map(debug_bytes),
            TypeTag::Infer => store.infer_index(id).map(|value| debug_bytes(&value)),
            TypeTag::ClassInstance => store.class_instance_type(id).map(debug_bytes),
            TypeTag::DeferredIndexedAccess => {
                store.deferred_indexed_access_type(id).map(debug_bytes)
            }
            TypeTag::Mapped => store.mapped_type(id).map(debug_bytes),
            TypeTag::MappedValue => None,
            TypeTag::Template => store.template_type(id).map(debug_bytes),
            TypeTag::Keyof => store.keyof_operand(id).map(|value| debug_bytes(&value)),
            TypeTag::Declared => store.declared_type(id).map(debug_bytes),
        };
        if let Some(payload) = payload {
            if !matches!(
                tag,
                TypeTag::Intrinsic
                    | TypeTag::Readonly
                    | TypeTag::Infer
                    | TypeTag::MappedValue
                    | TypeTag::Keyof
            ) {
                payload_count += 1;
                payloads.extend_from_slice(&payload);
                payloads.push(0xff);
            } else {
                rows.extend_from_slice(&payload);
            }
        }
        rows.push(0xff);
    }
    for (_, recipe) in store.snapshot_declared_recipes() {
        payload_count += 1;
        payloads.extend_from_slice(&debug_bytes(recipe));
        payloads.push(0xff);
    }
    let constraints = store.snapshot_type_param_constraints();
    let frozen = store.snapshot_frozen_type_params();
    let mut template_names = store
        .snapshot_template_name_ids()
        .map(|id| (id, store.template_name(id).unwrap_or_default().to_owned()))
        .collect::<Vec<_>>();
    template_names.sort_by_key(|(id, _)| id.0);
    vec![
        (count64(store.len()), rows),
        (count64(payload_count), payloads),
        (count64(constraints.len()), debug_rows(&constraints)),
        (count64(frozen.len()), debug_rows(&frozen)),
        (count64(template_names.len()), debug_rows(&template_names)),
    ]
}

#[cfg(test)]
fn interner_projection_subtables(bytes: &[u8]) -> Result<Vec<(u64, Vec<u8>)>, SnapshotError> {
    let mut reader = SnapshotReader::new(bytes);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?
        != 1
    {
        return Err(invalid(
            SnapshotErrorStage::Decode,
            "unsupported interner projection version",
        ));
    }
    let bucket_start = reader.position();
    let bucket_count = reader
        .collection_len(16)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    for _ in 0..bucket_count {
        reader
            .u64()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        let candidate_count = reader
            .collection_len(4)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        for _ in 0..candidate_count {
            reader
                .u32()
                .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        }
    }
    let bucket_end = reader.position();
    let reserved_start = bucket_end;
    let reserved_count = reader
        .collection_len(5)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    for _ in 0..reserved_count {
        reader
            .u32()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
        reader
            .u8()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    }
    let reserved_end = reader.position();
    let well_known_start = reserved_end;
    for _ in 0..IntrinsicKind::ALL.len() {
        reader
            .u32()
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    }
    let well_known_end = reader.position();
    reader
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    Ok(vec![
        (
            count64(bucket_count),
            bytes[bucket_start..bucket_end].to_vec(),
        ),
        (
            count64(reserved_count),
            bytes[reserved_start..reserved_end].to_vec(),
        ),
        (
            count64(IntrinsicKind::ALL.len()),
            bytes[well_known_start..well_known_end].to_vec(),
        ),
    ])
}

#[cfg(test)]
fn binder_projection_subtables(
    binder: &Binder,
    binder_references: &[(u8, u8, u8, u32, u32)],
) -> Vec<(u64, Vec<u8>)> {
    let mut scopes = Vec::new();
    for (index, scope) in binder.graph.snapshot_scopes().enumerate() {
        let mut symbols = scope.symbols.iter().collect::<Vec<_>>();
        symbols.sort_by(|left, right| left.0.cmp(right.0));
        scopes.extend_from_slice(
            format!(
                "{index}:{:?}:{:?}:{:?}:{symbols:?}",
                scope.parent, scope.namespace_public, scope.kind
            )
            .as_bytes(),
        );
        scopes.push(0xff);
    }
    let symbols = binder.symbols.snapshot_symbols().collect::<Vec<_>>();
    let declarations = binder.declarations.iter().collect::<Vec<_>>();
    let mut declaration_sites = declarations
        .iter()
        .map(|declaration| {
            (
                declaration.site.module.0,
                declaration.site.scope.map(|scope| scope.0),
                declaration.site.declaration_span.start,
                declaration.site.declaration_span.end,
                declaration.site.binding_span.start,
                declaration.site.binding_span.end,
                declaration.id.0,
            )
        })
        .collect::<Vec<_>>();
    declaration_sites.sort_unstable();
    let type_groups = binder.type_groups.iter().collect::<Vec<_>>();
    let primary = binder.namespaces.snapshot_primary();
    let namespace_row_count = primary.namespaces.len()
        + primary.fragments.len()
        + primary.members.len()
        + primary.placements.len()
        + primary.globals.len()
        + primary.deferred_modules.len()
        + primary.deferred_children.len()
        + primary.umd_exports.len()
        + primary.export_contexts.len()
        + primary.source_units.len();
    let namespace_index_count = primary.namespaces.len() * 2
        + primary.source_units.len()
        + primary.globals.len()
        + primary.deferred_modules.len()
        + primary.deferred_children.len()
        + primary.umd_exports.len()
        + primary.export_contexts.len();
    let namespace_indexes = binder_references
        .iter()
        .copied()
        .filter(|(owner_domain, _, _, _, _)| (22..=29).contains(owner_domain))
        .collect::<Vec<_>>();
    let mut module_sources = binder
        .snapshot_module_sources()
        .iter()
        .map(|(scope, source)| (scope.0, source.0))
        .collect::<Vec<_>>();
    module_sources.sort_unstable();
    vec![
        (count64(binder.graph.snapshot_len()), scopes),
        (count64(symbols.len()), debug_rows(&symbols)),
        (count64(declarations.len()), debug_rows(&declarations)),
        (
            count64(declaration_sites.len()),
            debug_rows(&declaration_sites),
        ),
        (count64(type_groups.len()), debug_rows(&type_groups)),
        (count64(namespace_row_count), debug_bytes(&primary)),
        (
            count64(namespace_index_count),
            debug_rows(&namespace_indexes),
        ),
        (count64(module_sources.len()), debug_rows(&module_sources)),
    ]
}

#[cfg(test)]
struct CheckerProjectionInputs<'a> {
    decl_types: &'a [Option<TypeId>],
    published: &'a PublishedTypeEnvironmentSnapshotParts,
    namespace_terminals: &'a [FrozenNamespaceValueTerminalSnapshotRow],
    runtime: &'a FrozenCheckerRuntimeSnapshotParts,
    semantic_identities: &'a Option<[LibraryIdentityTerminal; 8]>,
    roots: &'a [RootNameRow],
    next: &'a NextIds,
    source_file_count: u32,
}

#[cfg(test)]
fn checker_projection_subtables(
    inputs: CheckerProjectionInputs<'_>,
) -> Result<Vec<(u64, Vec<u8>)>, SnapshotError> {
    let CheckerProjectionInputs {
        decl_types,
        published,
        namespace_terminals,
        runtime,
        semantic_identities,
        roots,
        next,
        source_file_count,
    } = inputs;
    let mut group_writer = SnapshotWriter::new();
    write_len(&mut group_writer, published.groups.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for group in &published.groups {
        write_group(&mut group_writer, group)
            .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    }
    let mut class_writer = SnapshotWriter::new();
    write_len(&mut class_writer, published.classes.len())
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    for (class, terminal) in &published.classes {
        write_class(&mut class_writer, *class);
        match terminal {
            PublishedClassSnapshotTerminal::Ready(surface) => {
                class_writer.u8(0);
                write_class_surface(&mut class_writer, surface)
                    .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
            }
            PublishedClassSnapshotTerminal::Poisoned(cause) => class_writer.u8(match cause {
                PublishedClassPoison::Heritage => 1,
                PublishedClassPoison::Initializer => 2,
                PublishedClassPoison::Surface => 3,
            }),
        }
    }
    let mut parameter_defaults = Vec::new();
    let mut parameter_default_count = 0usize;
    for (class, parameters) in &runtime.class_application_parameters {
        for parameter in parameters {
            parameter_defaults.extend_from_slice(
                format!("{class:?}:{:?}:{:?}", parameter.id, parameter.default).as_bytes(),
            );
            parameter_defaults.push(0xff);
            parameter_default_count += 1;
        }
    }
    let mut aliases = Vec::new();
    aliases.extend_from_slice(&debug_rows(&runtime.class_value_aliases));
    aliases.extend_from_slice(&debug_rows(&runtime.standalone_namespace_value_aliases));
    let mut semantic_writer = SnapshotWriter::new();
    semantic_writer.bool(semantic_identities.is_some());
    if let Some(identities) = semantic_identities {
        for terminal in identities {
            write_identity_terminal(&mut semantic_writer, terminal)
                .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
        }
    }
    let group_bytes = if published.groups.is_empty() {
        Vec::new()
    } else {
        group_writer.into_bytes()
    };
    let class_bytes = if published.classes.is_empty() {
        Vec::new()
    } else {
        class_writer.into_bytes()
    };
    let semantic_bytes = if semantic_identities.is_some() {
        semantic_writer.into_bytes()
    } else {
        Vec::new()
    };
    Ok(vec![
        (
            count64(decl_types.len()),
            if decl_types.is_empty() {
                Vec::new()
            } else {
                encode_decl_types(decl_types)?
            },
        ),
        (count64(published.groups.len()), group_bytes),
        (count64(published.classes.len()), class_bytes),
        (
            count64(namespace_terminals.len()),
            if namespace_terminals.is_empty() {
                Vec::new()
            } else {
                encode_namespace_terminals(namespace_terminals)?
            },
        ),
        (
            count64(runtime.named_function_symbols.len()),
            debug_rows(&runtime.named_function_symbols),
        ),
        (
            count64(runtime.class_application_parameters.len()),
            debug_rows(&runtime.class_application_parameters),
        ),
        (count64(parameter_default_count), parameter_defaults),
        (
            count64(runtime.class_parents.len()),
            debug_rows(&runtime.class_parents),
        ),
        (
            count64(runtime.class_names.len()),
            debug_rows(&runtime.class_names),
        ),
        (
            count64(runtime.class_new_metadata.len()),
            debug_rows(&runtime.class_new_metadata),
        ),
        (
            count64(runtime.class_value_bindings.len()),
            debug_rows(&runtime.class_value_bindings),
        ),
        (
            count64(
                runtime.class_value_aliases.len()
                    + runtime.standalone_namespace_value_aliases.len(),
            ),
            aliases,
        ),
        (
            semantic_identities.as_ref().map_or(0, |_| 8),
            semantic_bytes,
        ),
        (count64(roots.len()), encode_root_index(roots)?),
        (1, encode_next_ids(next, source_file_count)),
    ])
}

#[cfg(test)]
struct ProjectionSubtableInputs<'a> {
    interner: &'a Interner,
    interner_section: &'a [u8],
    binder: &'a Binder,
    binder_references: &'a [(u8, u8, u8, u32, u32)],
    checker: CheckerProjectionInputs<'a>,
}

#[cfg(test)]
fn projection_subtables(
    inputs: ProjectionSubtableInputs<'_>,
) -> Result<Vec<RuntimeSubtableForTest>, SnapshotError> {
    let mut values = store_projection_subtables(inputs.interner.store());
    values.extend(interner_projection_subtables(inputs.interner_section)?);
    values.extend(binder_projection_subtables(
        inputs.binder,
        inputs.binder_references,
    ));
    values.extend(checker_projection_subtables(inputs.checker)?);
    if values.len() != 31 {
        return Err(invalid(
            SnapshotErrorStage::Generation,
            "projection subtable inventory is not exact",
        ));
    }
    Ok(projection_subtable_names()
        .zip(values)
        .map(|(name, (row_count, bytes))| RuntimeSubtableForTest {
            name,
            row_count,
            sha256: digest32(&bytes),
        })
        .collect())
}

#[cfg(test)]
fn build_projection(
    sections: &[DirectorySection],
    subtables: Vec<RuntimeSubtableForTest>,
    rows: &[RootNameRow],
    next: NextIds,
    reference_counts: [u64; 9],
    manifest_hash: [u8; 32],
    typed_validation_sha256: [u8; 32],
) -> RuntimeProjectionForTest {
    let families = sections
        .iter()
        .zip(SECTION_NAMES)
        .map(|(section, name)| RuntimeFamilyForTest {
            name,
            byte_len: section.range.len(),
            sha256: section.digest,
        })
        .collect::<Vec<_>>();
    let mut projection_bytes = Vec::new();
    for section in sections {
        projection_bytes.extend_from_slice(&section.digest);
    }
    projection_bytes.extend_from_slice(&manifest_hash);
    RuntimeProjectionForTest {
        families,
        subtables,
        global_names: rows.iter().map(|row| row.name.clone()).collect(),
        root_counts: root_counts(rows),
        next_ids: next,
        reference_counts,
        reference_manifest_sha256: manifest_hash,
        typed_validation_sha256,
        sha256: digest32(&projection_bytes),
    }
}

#[cfg(test)]
fn assemble_archive(
    payloads: &[Vec<u8>; 11],
) -> Result<(SnapshotArchiveForTest, Vec<DirectorySection>), SnapshotError> {
    let directory_len = SECTION_COUNT
        .checked_mul(DIRECTORY_ENTRY_LEN)
        .ok_or_else(|| invalid(SnapshotErrorStage::Generation, "directory overflow"))?;
    let body_offset = FIXED_HEADER_LEN
        .checked_add(directory_len)
        .ok_or_else(|| invalid(SnapshotErrorStage::Generation, "archive offset overflow"))?;
    let body_len = payloads
        .iter()
        .try_fold(0usize, |sum, payload| sum.checked_add(payload.len()))
        .ok_or_else(|| invalid(SnapshotErrorStage::Generation, "archive body overflow"))?;
    let mut body = Vec::with_capacity(body_len);
    for payload in payloads {
        body.extend_from_slice(payload);
    }
    let mut writer = SnapshotWriter::new();
    writer.raw(MAGIC);
    writer.u32(VERSION);
    writer.raw(&decode_hex32(PROFILE_IDENTITY));
    writer.raw(&decode_hex32(SCHEMA_IDENTITY));
    writer.u32(u32::try_from(SECTION_COUNT).expect("section count fits u32"));
    writer.u64(count64(body_len));
    writer.raw(&digest32(&body));
    let mut offset = body_offset;
    let mut sections = Vec::with_capacity(SECTION_COUNT);
    for (index, payload) in payloads.iter().enumerate() {
        let digest = digest32(payload);
        writer.u16(u16::try_from(index + 1).expect("section tag"));
        writer.u16(0);
        writer.u64(count64(offset));
        writer.u64(count64(payload.len()));
        writer.raw(&digest);
        sections.push(DirectorySection {
            range: offset..offset + payload.len(),
            digest,
        });
        offset += payload.len();
    }
    writer.raw(&body);
    let bytes = writer.into_bytes();
    Ok((
        SnapshotArchiveForTest {
            sha256: digest32(&bytes),
            bytes,
        },
        sections,
    ))
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct LibraryCompilerMeasureForTest {
    pub(in crate::check::checker) source_loads: u64,
    pub(in crate::check::checker) parse_units: u64,
    pub(in crate::check::checker) bind_units: u64,
    pub(in crate::check::checker) semantic_units: u64,
    pub(in crate::check::checker) snapshot_generations: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct ThreadEvidenceState {
    compiler: LibraryCompilerMeasureForTest,
    decoded_user_checks: u64,
    last_route_projection: Option<[u8; 32]>,
}

#[cfg(test)]
thread_local! {
    static THREAD_EVIDENCE: RefCell<ThreadEvidenceState> = RefCell::new(ThreadEvidenceState::default());
}

#[cfg(test)]
pub(in crate::check::checker) struct LibraryCompilerMeasureScopeForTest(
    LibraryCompilerMeasureForTest,
);

#[cfg(test)]
pub(in crate::check::checker) fn start_library_compiler_measure_for_test(
) -> LibraryCompilerMeasureScopeForTest {
    THREAD_EVIDENCE.with(|state| LibraryCompilerMeasureScopeForTest(state.borrow().compiler))
}

#[cfg(test)]
impl LibraryCompilerMeasureScopeForTest {
    pub(in crate::check::checker) fn finish(self) -> LibraryCompilerMeasureForTest {
        THREAD_EVIDENCE.with(|state| {
            let current = state.borrow().compiler;
            LibraryCompilerMeasureForTest {
                source_loads: current.source_loads - self.0.source_loads,
                parse_units: current.parse_units - self.0.parse_units,
                bind_units: current.bind_units - self.0.bind_units,
                semantic_units: current.semantic_units - self.0.semantic_units,
                snapshot_generations: current.snapshot_generations - self.0.snapshot_generations,
            }
        })
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct DecodedBaseRouteMeasureForTest {
    pub(in crate::check::checker) user_checks: u64,
    pub(in crate::check::checker) runtime_projection_sha256: String,
}

#[cfg(test)]
pub(in crate::check::checker) struct DecodedBaseRouteMeasureScopeForTest {
    checks: u64,
}

#[cfg(test)]
pub(in crate::check::checker) fn start_decoded_base_route_measure_for_test(
) -> DecodedBaseRouteMeasureScopeForTest {
    THREAD_EVIDENCE.with(|state| DecodedBaseRouteMeasureScopeForTest {
        checks: state.borrow().decoded_user_checks,
    })
}

#[cfg(test)]
impl DecodedBaseRouteMeasureScopeForTest {
    pub(in crate::check::checker) fn finish(self) -> DecodedBaseRouteMeasureForTest {
        THREAD_EVIDENCE.with(|state| {
            let state = state.borrow();
            DecodedBaseRouteMeasureForTest {
                user_checks: state.decoded_user_checks - self.checks,
                runtime_projection_sha256: state
                    .last_route_projection
                    .map(|digest| hex(&digest))
                    .unwrap_or_default(),
            }
        })
    }
}

#[cfg(test)]
pub(super) fn record_decoded_route(projection: &RuntimeProjectionForTest) {
    THREAD_EVIDENCE.with(|state| {
        let mut state = state.borrow_mut();
        state.decoded_user_checks += 1;
        state.last_route_projection = Some(projection.sha256);
    });
}

#[cfg(test)]
pub(in crate::check::checker) fn compile_snapshot_for_test(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<CompiledSnapshotForTest, SnapshotError> {
    let (run, state) = compile_owned_injected_profile(sources)
        .map_err(|error| invalid(SnapshotErrorStage::Generation, format!("{error:?}")))?;
    THREAD_EVIDENCE.with(|evidence| {
        let mut evidence = evidence.borrow_mut();
        evidence.compiler.source_loads += count64(sources.len());
        evidence.compiler.parse_units += count64(run.phase_counts.parse_units);
        evidence.compiler.bind_units += count64(run.phase_counts.bind_units);
        evidence.compiler.semantic_units += count64(
            run.phase_counts.publication_validations + run.phase_counts.statement_check_units,
        );
    });
    let product = super::library_compiler::freeze_library_runtime_product(state)
        .map_err(|message| invalid(SnapshotErrorStage::Generation, message))?;
    encode_snapshot_parts(
        &product._parts,
        &product._replay_index.canonical_manifest_bytes,
    )
}

#[cfg(test)]
pub(in crate::check::checker) fn encode_library_runtime_product(
    product: &CompiledLibraryRuntimeProduct,
) -> Result<CompiledSnapshotForTest, SnapshotError> {
    encode_snapshot_parts(
        &product._parts,
        &product._replay_index.canonical_manifest_bytes,
    )
}

#[cfg(test)]
fn encode_snapshot_parts(
    parts: &OwnedLibraryRuntimeSnapshotParts,
    replay_index: &[u8],
) -> Result<CompiledSnapshotForTest, SnapshotError> {
    encode_snapshot_inputs(SnapshotEncodeInputs {
        interner: &parts.interner,
        binder: &parts.binder,
        published_types: &parts.published_types,
        decl_types: &parts.decl_types,
        semantic_identities: &parts.semantic_identities,
        runtime: &parts.runtime,
        next_type_param: parts.next_type_param,
        next_class_id: parts.next_class_id,
        source_file_count: parts.source_file_count,
        replay_index,
    })
}

#[cfg(test)]
struct SnapshotEncodeInputs<'parts> {
    interner: &'parts Interner,
    binder: &'parts Binder,
    published_types: &'parts PublishedTypeEnvironmentSnapshotParts,
    decl_types: &'parts [Option<TypeId>],
    semantic_identities: &'parts Option<[LibraryIdentityTerminal; 8]>,
    runtime: &'parts FrozenCheckerRuntimeSnapshotParts,
    next_type_param: u32,
    next_class_id: u32,
    source_file_count: u32,
    replay_index: &'parts [u8],
}

#[cfg(test)]
fn encode_snapshot_inputs(
    parts: SnapshotEncodeInputs<'_>,
) -> Result<CompiledSnapshotForTest, SnapshotError> {
    let next = NextIds {
        store: parts.interner.store().len(),
        types: parts.interner.store().len(),
        type_params: usize::try_from(parts.next_type_param).expect("type-param prefix fits usize"),
        classes: usize::try_from(parts.next_class_id).expect("class prefix fits usize"),
        scopes: parts.binder.graph.snapshot_len(),
        symbols: parts.binder.symbols.len(),
        declarations: parts.binder.declarations.len(),
        type_groups: parts.binder.type_groups.len(),
        namespaces: parts.binder.namespaces.len(),
        value_storages: usize::try_from(parts.binder.decl_count)
            .expect("storage prefix fits usize"),
    };
    let roots = collect_root_rows(parts.binder)?;
    let identity = identity_witness(&roots, parts.published_types);
    let (store_references, interner_references) = parts
        .interner
        .snapshot_reference_records()
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    let binder_references = snapshot_reference_records(parts.binder)
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    let references = build_manifest_references(ManifestInputs {
        type_count: next.types,
        roots: &roots,
        store_references: &store_references,
        interner_references: &interner_references,
        binder_references: &binder_references,
        decl_types: parts.decl_types,
        published: parts.published_types,
        namespace_terminals: &parts.runtime.namespace_terminals,
        runtime: parts.runtime,
        semantic_identities: parts.semantic_identities,
    })
    .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    let tail_start = references.partition_point(|reference| reference.owner_family <= 3);
    let typed_validation_sha256 = typed_validation_identity(
        &next,
        parts.source_file_count,
        &roots,
        &store_references,
        &interner_references,
        &binder_references,
        &references[tail_start..],
    )?;
    let (store, interner) = parts
        .interner
        .encode_split_snapshot_sections_for_test()
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    let binder = encode_binder_snapshot(parts.binder)
        .map_err(|error| codec(SnapshotErrorStage::Generation, error))?;
    let decl_types = encode_decl_types(parts.decl_types)?;
    let published = encode_published(parts.published_types)?;
    let namespace_terminals = encode_namespace_terminals(&parts.runtime.namespace_terminals)?;
    let class_metadata = encode_class_metadata(parts.runtime)?;
    let root_index = encode_root_index(&roots)?;
    let next_ids = encode_next_ids(&next, parts.source_file_count);
    let subtables = projection_subtables(ProjectionSubtableInputs {
        interner: parts.interner,
        interner_section: &interner,
        binder: parts.binder,
        binder_references: &binder_references,
        checker: CheckerProjectionInputs {
            decl_types: parts.decl_types,
            published: parts.published_types,
            namespace_terminals: &parts.runtime.namespace_terminals,
            runtime: parts.runtime,
            semantic_identities: parts.semantic_identities,
            roots: &roots,
            next: &next,
            source_file_count: parts.source_file_count,
        },
    })?;
    let semantic = encode_semantic_identities(parts.semantic_identities, &references, &subtables)?;
    let payloads = [
        store,
        interner,
        binder,
        decl_types,
        published,
        namespace_terminals,
        class_metadata,
        semantic.bytes,
        root_index,
        next_ids,
        parts.replay_index.to_vec(),
    ];
    let (archive, sections) = assemble_archive(&payloads)?;
    let projection = build_projection(
        &sections,
        subtables,
        &roots,
        next,
        semantic.reference_counts,
        semantic.manifest_hash,
        typed_validation_sha256,
    );
    THREAD_EVIDENCE.with(|evidence| evidence.borrow_mut().compiler.snapshot_generations += 1);
    Ok(CompiledSnapshotForTest {
        archive,
        projection,
        identity,
    })
}

#[cfg(test)]
pub(in crate::check::checker) fn validate_snapshot(
    bytes: &[u8],
) -> Result<ValidatedSnapshot, SnapshotError> {
    if bytes.len() < FIXED_HEADER_LEN || !bytes.starts_with(MAGIC) {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "invalid snapshot magic or truncated header",
        ));
    }
    let mut reader = SnapshotReader::new(&bytes[MAGIC.len()..]);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?
        != VERSION
    {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "unsupported snapshot version",
        ));
    }
    if reader
        .raw(32)
        .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?
        != decode_hex32(PROFILE_IDENTITY)
    {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "profile identity mismatch",
        ));
    }
    if reader
        .raw(32)
        .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?
        != decode_hex32(SCHEMA_IDENTITY)
    {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "schema identity mismatch",
        ));
    }
    if usize::try_from(
        reader
            .u32()
            .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?,
    )
    .map_err(|_| {
        invalid(
            SnapshotErrorStage::HeaderValidation,
            "section count exceeds usize",
        )
    })? != SECTION_COUNT
    {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "wrong section count",
        ));
    }
    let body_len = usize::try_from(
        reader
            .u64()
            .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?,
    )
    .map_err(|_| {
        invalid(
            SnapshotErrorStage::HeaderValidation,
            "body length exceeds usize",
        )
    })?;
    let mut expected_body_digest = [0; 32];
    expected_body_digest.copy_from_slice(
        reader
            .raw(32)
            .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?,
    );
    let directory_end = FIXED_HEADER_LEN
        .checked_add(SECTION_COUNT * DIRECTORY_ENTRY_LEN)
        .ok_or_else(|| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "directory overflow",
            )
        })?;
    if directory_end > bytes.len() || body_len > bytes.len() - directory_end {
        return Err(invalid(
            SnapshotErrorStage::PayloadValidation,
            "body length mismatch",
        ));
    }
    if body_len < bytes.len() - directory_end {
        return Err(invalid(
            SnapshotErrorStage::DirectoryValidation,
            "trailing archive bytes",
        ));
    }
    if digest32(&bytes[directory_end..]) != expected_body_digest {
        return Err(invalid(
            SnapshotErrorStage::PayloadValidation,
            "body digest mismatch",
        ));
    }
    let mut directory = SnapshotReader::new(&bytes[FIXED_HEADER_LEN..directory_end]);
    let mut sections = Vec::with_capacity(SECTION_COUNT);
    let mut expected_offset = directory_end;
    for index in 0..SECTION_COUNT {
        let expected_tag = u16::try_from(index + 1).map_err(|_| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section tag exceeds u16",
            )
        })?;
        let tag = directory
            .u16()
            .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?;
        if tag != expected_tag
            || directory
                .u16()
                .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?
                != 0
        {
            return Err(invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section tags are not canonical",
            ));
        }
        let offset = usize::try_from(
            directory
                .u64()
                .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?,
        )
        .map_err(|_| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section offset exceeds usize",
            )
        })?;
        let len = usize::try_from(
            directory
                .u64()
                .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?,
        )
        .map_err(|_| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section length exceeds usize",
            )
        })?;
        let mut digest = [0; 32];
        digest.copy_from_slice(
            directory
                .raw(32)
                .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?,
        );
        let end = offset.checked_add(len).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section range overflow",
            )
        })?;
        if offset != expected_offset || end > bytes.len() || len == 0 {
            return Err(invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section ranges are not contiguous",
            ));
        }
        if digest32(&bytes[offset..end]) != digest {
            return Err(invalid(
                SnapshotErrorStage::PayloadValidation,
                "section digest mismatch",
            ));
        }
        sections.push(DirectorySection {
            range: offset..end,
            digest,
        });
        expected_offset = end;
    }
    directory
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?;
    if expected_offset != bytes.len() {
        return Err(invalid(
            SnapshotErrorStage::DirectoryValidation,
            "trailing archive bytes",
        ));
    }
    Ok(ValidatedSnapshot {
        bytes: Cow::Owned(bytes.to_vec()),
        sections,
    })
}

fn validate_canonical_snapshot(
    bytes: Cow<'static, [u8]>,
) -> Result<ValidatedSnapshot, SnapshotError> {
    if bytes.len() != CANONICAL_ARCHIVE_BYTES {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "canonical snapshot identity mismatch",
        ));
    }
    if bytes.len() < FIXED_HEADER_LEN || !bytes.starts_with(MAGIC) {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "invalid snapshot magic or truncated header",
        ));
    }
    let mut reader = SnapshotReader::new(&bytes[MAGIC.len()..]);
    if reader
        .u32()
        .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?
        != VERSION
    {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "unsupported snapshot version",
        ));
    }
    if reader
        .raw(32)
        .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?
        != decode_hex32(PROFILE_IDENTITY)
    {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "profile identity mismatch",
        ));
    }
    if reader
        .raw(32)
        .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?
        != decode_hex32(SCHEMA_IDENTITY)
    {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "schema identity mismatch",
        ));
    }
    if usize::try_from(
        reader
            .u32()
            .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?,
    )
    .map_err(|_| {
        invalid(
            SnapshotErrorStage::HeaderValidation,
            "section count exceeds usize",
        )
    })? != SECTION_COUNT
    {
        return Err(invalid(
            SnapshotErrorStage::HeaderValidation,
            "wrong section count",
        ));
    }
    let body_len = usize::try_from(
        reader
            .u64()
            .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?,
    )
    .map_err(|_| {
        invalid(
            SnapshotErrorStage::HeaderValidation,
            "body length exceeds usize",
        )
    })?;
    reader
        .raw(32)
        .map_err(|error| codec(SnapshotErrorStage::HeaderValidation, error))?;
    let directory_end = FIXED_HEADER_LEN
        .checked_add(SECTION_COUNT * DIRECTORY_ENTRY_LEN)
        .ok_or_else(|| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "directory overflow",
            )
        })?;
    if directory_end > bytes.len() || body_len > bytes.len() - directory_end {
        return Err(invalid(
            SnapshotErrorStage::PayloadValidation,
            "body length mismatch",
        ));
    }
    if body_len < bytes.len() - directory_end {
        return Err(invalid(
            SnapshotErrorStage::DirectoryValidation,
            "trailing archive bytes",
        ));
    }
    let mut directory = SnapshotReader::new(&bytes[FIXED_HEADER_LEN..directory_end]);
    let mut sections = Vec::with_capacity(SECTION_COUNT);
    let mut expected_offset = directory_end;
    for index in 0..SECTION_COUNT {
        let expected_tag = u16::try_from(index + 1).map_err(|_| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section tag exceeds u16",
            )
        })?;
        let tag = directory
            .u16()
            .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?;
        if tag != expected_tag
            || directory
                .u16()
                .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?
                != 0
        {
            return Err(invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section tags are not canonical",
            ));
        }
        let offset = usize::try_from(
            directory
                .u64()
                .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?,
        )
        .map_err(|_| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section offset exceeds usize",
            )
        })?;
        let len = usize::try_from(
            directory
                .u64()
                .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?,
        )
        .map_err(|_| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section length exceeds usize",
            )
        })?;
        let mut digest = [0; 32];
        digest.copy_from_slice(
            directory
                .raw(32)
                .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?,
        );
        let end = offset.checked_add(len).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section range overflow",
            )
        })?;
        if offset != expected_offset || end > bytes.len() || len == 0 {
            return Err(invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section ranges are not contiguous",
            ));
        }
        sections.push(DirectorySection {
            range: offset..end,
            digest,
        });
        expected_offset = end;
    }
    directory
        .finish()
        .map_err(|error| codec(SnapshotErrorStage::DirectoryValidation, error))?;
    if expected_offset != bytes.len() {
        return Err(invalid(
            SnapshotErrorStage::DirectoryValidation,
            "trailing archive bytes",
        ));
    }
    Ok(ValidatedSnapshot { bytes, sections })
}

fn section(validated: &ValidatedSnapshot, tag: usize) -> Result<&[u8], SnapshotError> {
    let index = tag.checked_sub(1).ok_or_else(|| {
        invalid(
            SnapshotErrorStage::DirectoryValidation,
            "section tag underflows directory index",
        )
    })?;
    let section = validated.sections.get(index).ok_or_else(|| {
        invalid(
            SnapshotErrorStage::DirectoryValidation,
            "section tag is absent from the directory",
        )
    })?;
    validated.bytes.get(section.range.clone()).ok_or_else(|| {
        invalid(
            SnapshotErrorStage::DirectoryValidation,
            "section range is outside the admitted artifact",
        )
    })
}

fn section_digest(validated: &ValidatedSnapshot, tag: usize) -> Result<[u8; 32], SnapshotError> {
    let index = tag.checked_sub(1).ok_or_else(|| {
        invalid(
            SnapshotErrorStage::DirectoryValidation,
            "section tag underflows directory index",
        )
    })?;
    validated
        .sections
        .get(index)
        .map(|section| section.digest)
        .ok_or_else(|| {
            invalid(
                SnapshotErrorStage::DirectoryValidation,
                "section tag exceeds directory",
            )
        })
}

#[cfg(test)]
pub(in crate::check::checker) fn decode_snapshot_for_test(
    validated: ValidatedSnapshot,
    strategy: SnapshotDecodeStrategy,
) -> Result<DecodedLibraryBase, SnapshotError> {
    if strategy == SnapshotDecodeStrategy::ImmutableIndexed {
        return Err(invalid(
            SnapshotErrorStage::UnsupportedStrategy,
            "immutable-indexed materialization is not supported by the generic test decoder",
        ));
    }
    let (next, source_file_count) = decode_next_ids(section(&validated, 10)?)?;
    let interner =
        Interner::decode_split_snapshot_sections(section(&validated, 1)?, section(&validated, 2)?)
            .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let binder = decode_binder_snapshot(section(&validated, 3)?)
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    if interner.store().len() != next.types
        || binder.graph.snapshot_len() != next.scopes
        || binder.symbols.len() != next.symbols
        || binder.declarations.len() != next.declarations
        || binder.type_groups.len() != next.type_groups
        || binder.namespaces.len() != next.namespaces
        || usize::try_from(binder.decl_count).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "value-storage prefix exceeds usize",
            )
        })? != next.value_storages
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "decoded runtime counts disagree with next-id prefix",
        ));
    }
    let roots = decode_root_index(section(&validated, 9)?, &next)?;
    let canonical_roots = collect_root_rows(&binder)
        .map_err(|error| invalid(SnapshotErrorStage::ReferenceValidation, error.message))?;
    if roots
        .iter()
        .map(|row| (&row.name, row.symbol, row.value, row.ty, row.namespace))
        .ne(canonical_roots
            .iter()
            .map(|row| (&row.name, row.symbol, row.value, row.ty, row.namespace)))
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "root index disagrees with binder global scope",
        ));
    }
    let replay_roots = roots
        .iter()
        .map(|root| (root.name.clone(), root.value, root.ty, root.namespace))
        .collect::<Vec<_>>();
    let (store_references, interner_references) = interner
        .snapshot_reference_records()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let binder_references = snapshot_reference_records(&binder)
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let reference_limits = ReferenceLimits::from_canonical_references(
        &next,
        &roots,
        &store_references,
        &interner_references,
        &binder_references,
    )?;
    let semantic = decode_semantic_identities(section(&validated, 8)?, &reference_limits)?;
    let decl_types = decode_decl_types(section(&validated, 4)?, next.types)?;
    let published_types = decode_published(section(&validated, 5)?)?;
    validate_dense_identity_prefixes(&next, &interner, &published_types)?;
    let replay_index = admit_collision_replay_index(
        section(&validated, 11)?,
        ReplayIndexAdmissionLimits {
            type_groups: next.type_groups,
            value_storages: next.value_storages,
            namespaces: next.namespaces,
            classes: next.classes,
            source_files: usize::try_from(source_file_count).map_err(|_| {
                invalid(
                    SnapshotErrorStage::CollisionReplayIndexAdmission,
                    "source-file count exceeds usize",
                )
            })?,
            roots: &replay_roots,
        },
        None,
    )
    .map_err(rejected_replay_index)?;
    let namespace_terminals = decode_namespace_terminals(section(&validated, 6)?)?;
    let runtime = decode_class_metadata(section(&validated, 7)?, namespace_terminals.clone())?;
    let expected_references = build_manifest_references(ManifestInputs {
        type_count: next.types,
        roots: &roots,
        store_references: &store_references,
        interner_references: &interner_references,
        binder_references: &binder_references,
        decl_types: &decl_types,
        published: &published_types,
        namespace_terminals: &namespace_terminals,
        runtime: &runtime,
        semantic_identities: &semantic.identities,
    })
    .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let mut expected_writer = SnapshotWriter::new();
    write_manifest(&mut expected_writer, &expected_references)
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let expected_manifest = expected_writer.into_bytes();
    if !section(&validated, 8)?.starts_with(&expected_manifest) {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "reference manifest disagrees with decoded state",
        ));
    }
    let tail_start = expected_references.partition_point(|reference| reference.owner_family <= 3);
    let typed_validation_sha256 = typed_validation_identity(
        &next,
        source_file_count,
        &roots,
        &store_references,
        &interner_references,
        &binder_references,
        &expected_references[tail_start..],
    )?;
    let identity = identity_witness(&roots, &published_types);
    let projection = build_projection(
        &validated.sections,
        semantic.projection_subtables,
        &roots,
        next.clone(),
        semantic.reference_counts,
        semantic.manifest_hash,
        typed_validation_sha256,
    );
    let state = OwnedLibraryRuntimeState::from_snapshot_parts_with_replay(
        OwnedLibraryRuntimeSnapshotParts {
            interner,
            binder,
            published_types,
            decl_types,
            semantic_identities: semantic.identities,
            runtime,
            next_type_param: u32::try_from(next.type_params).map_err(|_| {
                invalid(SnapshotErrorStage::Decode, "type-param prefix exceeds u32")
            })?,
            next_class_id: u32::try_from(next.classes)
                .map_err(|_| invalid(SnapshotErrorStage::Decode, "class prefix exceeds u32"))?,
            source_file_count,
        },
        Some(replay_index),
    )
    .map_err(|message| invalid(SnapshotErrorStage::ReferenceValidation, message))?;
    Ok(DecodedLibraryBase {
        state,
        typed_validation_sha256,
        projection,
        identity,
        source_file_count,
        prefix_lengths: next,
        root_names: roots.iter().map(|row| row.name.clone()).collect(),
        root_counts: root_counts(&roots),
        strategy,
    })
}

fn decode_canonical_snapshot_with_evidence(
    validated: ValidatedSnapshot,
) -> Result<(DecodedLibraryBase, AdmittedLibrarySnapshotEvidence), SnapshotError> {
    #[cfg(test)]
    let strategy = SnapshotDecodeStrategy::EagerComplete;
    let (next, source_file_count) = decode_next_ids(section(&validated, 10)?)?;
    let store_section = section(&validated, 1)?;
    let interner_section = section(&validated, 2)?;
    let binder_section = section(&validated, 3)?;
    let replay_section = section(&validated, 11)?;
    let replay_digest = section_digest(&validated, 11)?;
    let (interner_result, binder_result, replay_decode_result) = std::thread::scope(|scope| {
        let interner = std::thread::Builder::new()
            .name("typokat-library-interner-decode".to_owned())
            .spawn_scoped(scope, || {
                Interner::decode_split_snapshot_sections(store_section, interner_section)
            })
            .map_err(|_| SnapshotError {
                stage: SnapshotErrorStage::Decode,
                message: "could not create interner decode worker".to_owned(),
                kind: SnapshotErrorKind::WorkerSpawnFailed { worker: "interner" },
            })?;
        let binder = match std::thread::Builder::new()
            .name("typokat-library-binder-decode".to_owned())
            .spawn_scoped(scope, || {
                decode_binder_snapshot_with_evidence(binder_section)
            }) {
            Ok(binder) => binder,
            Err(_) => {
                return match interner.join() {
                    Err(_) => Err(SnapshotError {
                        stage: SnapshotErrorStage::Decode,
                        message: "interner decode worker panicked".to_owned(),
                        kind: SnapshotErrorKind::WorkerPanicked { worker: "interner" },
                    }),
                    Ok(Err(error)) => Err(codec(SnapshotErrorStage::ReferenceValidation, error)),
                    Ok(Ok(_)) => Err(SnapshotError {
                        stage: SnapshotErrorStage::Decode,
                        message: "could not create binder decode worker".to_owned(),
                        kind: SnapshotErrorKind::WorkerSpawnFailed { worker: "binder" },
                    }),
                };
            }
        };
        let replay = match std::thread::Builder::new()
            .name("typokat-library-replay-decode".to_owned())
            .spawn_scoped(scope, || {
                decode_authenticated_collision_replay_index(replay_section, replay_digest)
            }) {
            Ok(replay) => replay,
            Err(_) => {
                let interner_result = interner.join();
                let binder_result = binder.join();
                return match interner_result {
                    Err(_) => Err(SnapshotError {
                        stage: SnapshotErrorStage::Decode,
                        message: "interner decode worker panicked".to_owned(),
                        kind: SnapshotErrorKind::WorkerPanicked { worker: "interner" },
                    }),
                    Ok(Err(error)) => Err(codec(SnapshotErrorStage::ReferenceValidation, error)),
                    Ok(Ok(_)) => match binder_result {
                        Err(_) => Err(SnapshotError {
                            stage: SnapshotErrorStage::Decode,
                            message: "binder decode worker panicked".to_owned(),
                            kind: SnapshotErrorKind::WorkerPanicked { worker: "binder" },
                        }),
                        Ok(Err(error)) => Err(codec(SnapshotErrorStage::Decode, error)),
                        Ok(Ok(_)) => Err(SnapshotError {
                            stage: SnapshotErrorStage::CollisionReplayIndexAdmission,
                            message: "could not create replay-index decode worker".to_owned(),
                            kind: SnapshotErrorKind::WorkerSpawnFailed {
                                worker: "replay-index",
                            },
                        }),
                    },
                };
            }
        };
        Ok::<_, SnapshotError>((interner.join(), binder.join(), replay.join()))
    })?;
    let interner = interner_result
        .map_err(|_| SnapshotError {
            stage: SnapshotErrorStage::Decode,
            message: "interner decode worker panicked".to_owned(),
            kind: SnapshotErrorKind::WorkerPanicked { worker: "interner" },
        })?
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let decoded_binder = binder_result
        .map_err(|_| SnapshotError {
            stage: SnapshotErrorStage::Decode,
            message: "binder decode worker panicked".to_owned(),
            kind: SnapshotErrorKind::WorkerPanicked { worker: "binder" },
        })?
        .map_err(|error| codec(SnapshotErrorStage::Decode, error))?;
    let binder = decoded_binder.binder;
    let retained_scope_maps = decoded_binder.retained_scope_maps;
    if interner.store().len() != next.types
        || binder.graph.snapshot_len() != next.scopes
        || binder.symbols.len() != next.symbols
        || binder.declarations.len() != next.declarations
        || binder.type_groups.len() != next.type_groups
        || binder.namespaces.len() != next.namespaces
        || usize::try_from(binder.decl_count).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "value-storage prefix exceeds usize",
            )
        })? != next.value_storages
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "decoded runtime counts disagree with next-id prefix",
        ));
    }
    let evidence = admitted_library_snapshot_evidence(
        &validated,
        &next,
        source_file_count,
        &binder,
        retained_scope_maps,
    )?;
    let roots = decode_root_index(section(&validated, 9)?, &next)?;
    let canonical_roots = collect_root_rows(&binder)
        .map_err(|error| invalid(SnapshotErrorStage::ReferenceValidation, error.message))?;
    if roots
        .iter()
        .map(|row| (&row.name, row.symbol, row.value, row.ty, row.namespace))
        .ne(canonical_roots
            .iter()
            .map(|row| (&row.name, row.symbol, row.value, row.ty, row.namespace)))
    {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "root index disagrees with binder global scope",
        ));
    }
    let replay_roots = roots
        .iter()
        .map(|root| (root.name.clone(), root.value, root.ty, root.namespace))
        .collect::<Vec<_>>();
    let replay_limits = ReplayIndexAdmissionLimits {
        type_groups: next.type_groups,
        value_storages: next.value_storages,
        namespaces: next.namespaces,
        classes: next.classes,
        source_files: usize::try_from(source_file_count).map_err(|_| {
            invalid(
                SnapshotErrorStage::CollisionReplayIndexAdmission,
                "source-file count exceeds usize",
            )
        })?,
        roots: &replay_roots,
    };
    let semantic_section = section(&validated, 8)?;
    let decl_types_section = section(&validated, 4)?;
    let published_types_section = section(&validated, 5)?;
    let (
        interner_references_result,
        binder_references_result,
        replay_admission_result,
        semantic_result,
        decl_types_result,
        published_types_result,
    ) = std::thread::scope(|scope| {
        let interner_references = std::thread::Builder::new()
            .name("typokat-library-interner-references".to_owned())
            .spawn_scoped(scope, || interner.snapshot_reference_records())
            .map_err(|_| SnapshotError {
                stage: SnapshotErrorStage::ReferenceValidation,
                message: "could not create interner reference worker".to_owned(),
                kind: SnapshotErrorKind::WorkerSpawnFailed {
                    worker: "interner-reference",
                },
            })?;
        let binder_references = match std::thread::Builder::new()
            .name("typokat-library-binder-references".to_owned())
            .spawn_scoped(scope, || snapshot_reference_records(&binder))
        {
            Ok(binder_references) => binder_references,
            Err(_) => {
                return match interner_references.join() {
                    Err(_) => Err(SnapshotError {
                        stage: SnapshotErrorStage::ReferenceValidation,
                        message: "interner reference worker panicked".to_owned(),
                        kind: SnapshotErrorKind::WorkerPanicked {
                            worker: "interner-reference",
                        },
                    }),
                    Ok(Err(error)) => Err(codec(SnapshotErrorStage::ReferenceValidation, error)),
                    Ok(Ok(_)) => Err(SnapshotError {
                        stage: SnapshotErrorStage::ReferenceValidation,
                        message: "could not create binder reference worker".to_owned(),
                        kind: SnapshotErrorKind::WorkerSpawnFailed {
                            worker: "binder-reference",
                        },
                    }),
                };
            }
        };
        let replay_admission = match std::thread::Builder::new()
            .name("typokat-library-replay-admission".to_owned())
            .spawn_scoped(scope, move || {
                let replay_decoded = replay_decode_result
                    .map_err(|_| SnapshotError {
                        stage: SnapshotErrorStage::CollisionReplayIndexAdmission,
                        message: "replay-index decode worker panicked".to_owned(),
                        kind: SnapshotErrorKind::WorkerPanicked {
                            worker: "replay-index",
                        },
                    })?
                    .map_err(rejected_replay_index)?;
                admit_decoded_collision_replay_index(
                    replay_decoded,
                    replay_limits,
                    Some(COLLISION_REPLAY_MANIFEST_SHA256),
                )
                .map_err(rejected_replay_index)
            }) {
            Ok(replay_admission) => replay_admission,
            Err(_) => {
                let interner_result = interner_references.join();
                let binder_result = binder_references.join();
                return match interner_result {
                    Err(_) => Err(SnapshotError {
                        stage: SnapshotErrorStage::ReferenceValidation,
                        message: "interner reference worker panicked".to_owned(),
                        kind: SnapshotErrorKind::WorkerPanicked {
                            worker: "interner-reference",
                        },
                    }),
                    Ok(Err(error)) => Err(codec(SnapshotErrorStage::ReferenceValidation, error)),
                    Ok(Ok(_)) => match binder_result {
                        Err(_) => Err(SnapshotError {
                            stage: SnapshotErrorStage::ReferenceValidation,
                            message: "binder reference worker panicked".to_owned(),
                            kind: SnapshotErrorKind::WorkerPanicked {
                                worker: "binder-reference",
                            },
                        }),
                        Ok(Err(error)) => {
                            Err(codec(SnapshotErrorStage::ReferenceValidation, error))
                        }
                        Ok(Ok(_)) => Err(SnapshotError {
                            stage: SnapshotErrorStage::CollisionReplayIndexAdmission,
                            message: "could not create replay-index admission worker".to_owned(),
                            kind: SnapshotErrorKind::WorkerSpawnFailed {
                                worker: "replay-index",
                            },
                        }),
                    },
                };
            }
        };
        let semantic = decode_canonical_semantic_section(semantic_section);
        let decl_types = decode_decl_types(decl_types_section, next.types);
        let published_types = decode_published(published_types_section);
        Ok::<_, SnapshotError>((
            interner_references.join(),
            binder_references.join(),
            replay_admission.join(),
            semantic,
            decl_types,
            published_types,
        ))
    })?;
    let (store_references, interner_references) = interner_references_result
        .map_err(|_| SnapshotError {
            stage: SnapshotErrorStage::ReferenceValidation,
            message: "interner reference worker panicked".to_owned(),
            kind: SnapshotErrorKind::WorkerPanicked {
                worker: "interner-reference",
            },
        })?
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let binder_references = binder_references_result
        .map_err(|_| SnapshotError {
            stage: SnapshotErrorStage::ReferenceValidation,
            message: "binder reference worker panicked".to_owned(),
            kind: SnapshotErrorKind::WorkerPanicked {
                worker: "binder-reference",
            },
        })?
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let reference_limits = ReferenceLimits::from_canonical_references(
        &next,
        &roots,
        &store_references,
        &interner_references,
        &binder_references,
    )?;
    let semantic = semantic_result?;
    if semantic.identities.as_ref().is_none_or(|identities| {
        identities
            .iter()
            .any(|identity| !matches!(identity, LibraryIdentityTerminal::Ready(_)))
    }) {
        return Err(invalid(
            SnapshotErrorStage::Publication,
            "semantic identity publication is not complete",
        ));
    }
    let decl_types = decl_types_result?;
    let published_types = published_types_result?;
    validate_dense_identity_prefixes(&next, &interner, &published_types)?;
    let replay_index = replay_admission_result.map_err(|_| SnapshotError {
        stage: SnapshotErrorStage::CollisionReplayIndexAdmission,
        message: "replay-index admission worker panicked".to_owned(),
        kind: SnapshotErrorKind::WorkerPanicked {
            worker: "replay-index",
        },
    })??;
    let namespace_terminals = decode_namespace_terminals(section(&validated, 6)?)?;
    let runtime = decode_class_metadata(section(&validated, 7)?, namespace_terminals.clone())?;
    let tail_references = build_tail_manifest_references(TailManifestInputs {
        roots: &roots,
        decl_types: &decl_types,
        published: &published_types,
        namespace_terminals: &namespace_terminals,
        runtime: &runtime,
        semantic_identities: &semantic.identities,
    })
    .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    verify_reference_manifest_streaming(
        section(&validated, 8)?,
        &reference_limits,
        next.types,
        &store_references,
        &interner_references,
        &binder_references,
        &tail_references,
    )?;
    let typed_validation_sha256 = typed_validation_identity(
        &next,
        source_file_count,
        &roots,
        &store_references,
        &interner_references,
        &binder_references,
        &tail_references,
    )?;
    #[cfg(not(test))]
    let _ = typed_validation_sha256;
    #[cfg(test)]
    let identity = identity_witness(&roots, &published_types);
    #[cfg(test)]
    let projection = build_projection(
        &validated.sections,
        semantic.projection_subtables,
        &roots,
        next.clone(),
        semantic.reference_counts,
        semantic.manifest_hash,
        typed_validation_sha256,
    );
    let state = OwnedLibraryRuntimeState::from_snapshot_parts_with_replay(
        OwnedLibraryRuntimeSnapshotParts {
            interner,
            binder,
            published_types,
            decl_types,
            semantic_identities: semantic.identities,
            runtime,
            next_type_param: u32::try_from(next.type_params).map_err(|_| {
                invalid(SnapshotErrorStage::Decode, "type-param prefix exceeds u32")
            })?,
            next_class_id: u32::try_from(next.classes)
                .map_err(|_| invalid(SnapshotErrorStage::Decode, "class prefix exceeds u32"))?,
            source_file_count,
        },
        Some(replay_index),
    )
    .map_err(|message| invalid(SnapshotErrorStage::Publication, message))?;
    Ok((
        DecodedLibraryBase {
            state,
            #[cfg(test)]
            typed_validation_sha256,
            #[cfg(test)]
            projection,
            #[cfg(test)]
            identity,
            #[cfg(test)]
            source_file_count,
            prefix_lengths: next,
            root_names: roots.iter().map(|row| row.name.clone()).collect(),
            #[cfg(test)]
            root_counts: root_counts(&roots),
            #[cfg(test)]
            strategy,
        },
        evidence,
    ))
}

#[cfg(test)]
pub(in crate::check::checker) fn decode_canonical_snapshot(
    validated: ValidatedSnapshot,
) -> Result<DecodedLibraryBase, SnapshotError> {
    decode_canonical_snapshot_with_evidence(validated).map(|(decoded, _)| decoded)
}

#[cfg(test)]
pub(in crate::check::checker) fn decode_snapshot_bytes_for_test(
    bytes: &[u8],
    strategy: SnapshotDecodeStrategy,
) -> Result<DecodedLibraryBase, SnapshotError> {
    let validated = validate_snapshot(bytes)?;
    decode_snapshot_for_test(validated, strategy)
}

#[allow(
    dead_code,
    reason = "retained as the validated single-output decode seam"
)]
pub(crate) fn decode_canonical_library_snapshot_with_evidence(
    admitted: AdmittedCanonicalSnapshot,
) -> Result<(DecodedFrozenLibrary, AdmittedLibrarySnapshotEvidence), SnapshotError> {
    let validated = validate_canonical_snapshot(admitted.into_bytes())?;
    decode_canonical_snapshot_with_evidence(validated)
        .map(|(decoded, evidence)| (DecodedFrozenLibrary::from_decoded(decoded), evidence))
}

fn admitted_library_snapshot_evidence(
    validated: &ValidatedSnapshot,
    next: &NextIds,
    source_file_count: u32,
    binder: &Binder,
    retained_scope_maps: RetainedScopeMapSnapshotEvidence,
) -> Result<AdmittedLibrarySnapshotEvidence, SnapshotError> {
    let mut modules = binder
        .snapshot_module_sources()
        .iter()
        .filter(|(_, source)| **source != crate::binder::namespace::SourceUnitKey::PRELUDE)
        .map(|(module, source)| (*module, *source))
        .collect::<Vec<_>>();
    modules.sort_by_key(|(_, source)| source.0);
    let source_count = usize::try_from(source_file_count).map_err(|_| {
        invalid(
            SnapshotErrorStage::ReferenceValidation,
            "source count exceeds usize",
        )
    })?;
    if modules.len() != source_count {
        return Err(invalid(
            SnapshotErrorStage::ReferenceValidation,
            "binder module ownership disagrees with source count",
        ));
    }
    let library_units = modules
        .into_iter()
        .enumerate()
        .map(|(index, (module, source))| LibraryBinderUnit {
            ordinal: crate::source::LibraryFileOrdinal::new(index),
            source,
            module,
        })
        .collect();
    let section_digests = std::array::from_fn(|index| validated.sections[index].digest);
    let evidence = AdmittedLibrarySnapshotEvidence {
        section_digests,
        prefixes: LibraryBinderCheckpointEnds {
            scopes: next.scopes,
            symbols: next.symbols,
            declarations: next.declarations,
            type_groups: next.type_groups,
            namespaces: next.namespaces,
            value_storages: next.value_storages,
            next_source: source_count + 1,
        },
        library_units,
        retained_scope_maps_sha256: retained_scope_maps.sha256(),
    };
    Ok(evidence)
}

#[cfg(test)]
pub(crate) fn decode_pre_admitted_library_snapshot_with_evidence(
    bytes: &[u8],
) -> Result<(DecodedFrozenLibrary, AdmittedLibrarySnapshotEvidence), SnapshotError> {
    let validated = validate_snapshot(bytes)?;
    decode_canonical_snapshot_with_evidence(validated)
        .map(|(decoded, evidence)| (DecodedFrozenLibrary::from_decoded(decoded), evidence))
}

#[cfg(test)]
pub(crate) fn projection_from_library_product(
    product: &CompiledLibraryRuntimeProduct,
) -> Result<RuntimeProjectionForTest, SnapshotError> {
    encode_snapshot_parts(
        &product._parts,
        &product._replay_index.canonical_manifest_bytes,
    )
    .map(|compiled| compiled.projection)
}

#[cfg(test)]
pub(crate) fn recompute_runtime_projection(
    runtime: &OwnedLibraryRuntimeState,
) -> Result<RuntimeProjectionForTest, SnapshotError> {
    let parts = runtime
        .borrowed_snapshot_parts()
        .map_err(|message| invalid(SnapshotErrorStage::Publication, message))?;
    encode_borrowed_snapshot_parts(&parts).map(|compiled| compiled.projection)
}

#[cfg(test)]
pub(crate) fn validate_runtime_references(
    runtime: &OwnedLibraryRuntimeState,
) -> Result<FrozenReferenceValidation, SnapshotError> {
    let parts = runtime
        .borrowed_snapshot_parts()
        .map_err(|message| invalid(SnapshotErrorStage::Publication, message))?;
    let next = next_ids_from_borrowed_parts(&parts)?;
    let roots = collect_root_rows(parts.binder)?;
    let (store_references, interner_references) = parts
        .interner
        .snapshot_reference_records()
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let binder_references = snapshot_reference_records(parts.binder)
        .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    let limits = ReferenceLimits::from_canonical_references(
        &next,
        &roots,
        &store_references,
        &interner_references,
        &binder_references,
    )?;
    let references = build_manifest_references(ManifestInputs {
        type_count: next.types,
        roots: &roots,
        store_references: &store_references,
        interner_references: &interner_references,
        binder_references: &binder_references,
        decl_types: &parts.decl_types,
        published: &parts.published_types,
        namespace_terminals: &parts.runtime.namespace_terminals,
        runtime: &parts.runtime,
        semantic_identities: &parts.semantic_identities,
    })
    .map_err(|error| codec(SnapshotErrorStage::ReferenceValidation, error))?;
    for reference in &references {
        let owner_limit = limits.owner_limit(*reference).ok_or_else(|| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "unknown retained reference owner domain",
            )
        })?;
        validate_id(reference.owner, owner_limit, "retained reference owner")?;
        let target_limit = limits
            .target_limit(reference.target_domain)
            .ok_or_else(|| {
                invalid(
                    SnapshotErrorStage::ReferenceValidation,
                    "unknown retained reference target domain",
                )
            })?;
        validate_id(reference.target, target_limit, "retained reference target")?;
    }
    Ok(FrozenReferenceValidation {
        checked: u64::try_from(references.len()).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "retained reference count exceeds u64",
            )
        })?,
        outside_frozen_prefix: 0,
        base_to_delta: 0,
        untyped_or_unowned: 0,
    })
}

#[cfg(test)]
fn next_ids_from_borrowed_parts(
    parts: &BorrowedLibraryRuntimeSnapshotParts<'_>,
) -> Result<NextIds, SnapshotError> {
    Ok(NextIds {
        store: parts.interner.store().len(),
        types: parts.interner.store().len(),
        type_params: usize::try_from(parts.next_type_param).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "type prefix exceeds usize",
            )
        })?,
        classes: usize::try_from(parts.next_class_id).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "class prefix exceeds usize",
            )
        })?,
        scopes: parts.binder.graph.snapshot_len(),
        symbols: parts.binder.symbols.len(),
        declarations: parts.binder.declarations.len(),
        type_groups: parts.binder.type_groups.len(),
        namespaces: parts.binder.namespaces.len(),
        value_storages: usize::try_from(parts.binder.decl_count).map_err(|_| {
            invalid(
                SnapshotErrorStage::ReferenceValidation,
                "value-storage prefix exceeds usize",
            )
        })?,
    })
}

#[cfg(test)]
fn encode_borrowed_snapshot_parts(
    parts: &BorrowedLibraryRuntimeSnapshotParts<'_>,
) -> Result<CompiledSnapshotForTest, SnapshotError> {
    let replay_index = parts
        .replay_index
        .encode_manifest_for_test()
        .map_err(|error| invalid(SnapshotErrorStage::Generation, format!("{error:?}")))?;
    encode_snapshot_inputs(SnapshotEncodeInputs {
        interner: parts.interner,
        binder: parts.binder,
        published_types: &parts.published_types,
        decl_types: &parts.decl_types,
        semantic_identities: &parts.semantic_identities,
        runtime: &parts.runtime,
        next_type_param: parts.next_type_param,
        next_class_id: parts.next_class_id,
        source_file_count: parts.source_file_count,
        replay_index: &replay_index,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(in crate::check::checker) struct ProbeSemanticsCaseForTest {
    pub(in crate::check::checker) exit: u8,
    pub(in crate::check::checker) diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(in crate::check::checker) struct ProbeSemanticsForTest {
    pub(in crate::check::checker) fast_clean: ProbeSemanticsCaseForTest,
    pub(in crate::check::checker) fast_errors: ProbeSemanticsCaseForTest,
}

#[cfg(test)]
pub(in crate::check::checker) struct SnapshotFastCleanProbeRecordForTest {
    pub(in crate::check::checker) profile_identity: String,
    pub(in crate::check::checker) strategy: SnapshotDecodeStrategy,
    pub(in crate::check::checker) route: &'static str,
    pub(in crate::check::checker) artifact_bytes: usize,
    pub(in crate::check::checker) validated_bytes: usize,
    pub(in crate::check::checker) compiler_measure: LibraryCompilerMeasureForTest,
    pub(in crate::check::checker) runtime_projection_sha256: String,
    pub(in crate::check::checker) semantics: ProbeSemanticsCaseForTest,
    pub(in crate::check::checker) validation_us: u64,
    pub(in crate::check::checker) decode_us: u64,
    pub(in crate::check::checker) user_check_us: u64,
    pub(in crate::check::checker) wall_us: u64,
    pub(in crate::check::checker) peak_rss_bytes: u64,
    artifact_sha256: String,
    input_path: String,
}

#[cfg(test)]
fn json_string(value: &str) -> String {
    let mut rendered = String::from("\"");
    for character in value.chars() {
        match character {
            '\"' => rendered.push_str("\\\""),
            '\\' => rendered.push_str("\\\\"),
            '\n' => rendered.push_str("\\n"),
            '\r' => rendered.push_str("\\r"),
            '\t' => rendered.push_str("\\t"),
            c if c.is_control() => rendered.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => rendered.push(c),
        }
    }
    rendered.push('\"');
    rendered
}

#[cfg(test)]
fn json_strings(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string(value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[cfg(test)]
impl SnapshotFastCleanProbeRecordForTest {
    pub(in crate::check::checker) fn render(&self) -> String {
        format!(concat!("TYPOKAT_WU0B_PROBE={{\"schema\":1,\"kind\":\"eager-fast-clean\",\"route\":\"{}\",\"profile_sha256\":\"{}\",", "\"strategy\":\"eager-complete\",\"artifact_sha256\":\"{}\",\"artifact_bytes\":{},\"validated_bytes\":{},", "\"runtime_projection_sha256\":\"{}\",\"input_path\":{},", "\"semantics\":{{\"fast-clean\":{{\"exit\":{},\"diagnostics\":{}}}}},", "\"compiler_measure\":{{\"source_loads\":{},\"parse_units\":{},\"bind_units\":{},\"semantic_units\":{},\"snapshot_generations\":{}}},", "\"internal\":{{\"validation_us\":{},\"decode_us\":{},\"user_check_us\":{},\"wall_us\":{},\"peak_rss_bytes\":{}}}}}"),
            self.route, self.profile_identity, self.artifact_sha256, self.artifact_bytes, self.validated_bytes, self.runtime_projection_sha256, json_string(&self.input_path),
            self.semantics.exit, json_strings(&self.semantics.diagnostics),
            self.compiler_measure.source_loads, self.compiler_measure.parse_units, self.compiler_measure.bind_units, self.compiler_measure.semantic_units, self.compiler_measure.snapshot_generations,
            self.validation_us, self.decode_us, self.user_check_us, self.wall_us, self.peak_rss_bytes)
    }
}

#[cfg(test)]
pub(in crate::check::checker) struct SnapshotSemanticCalibrationRecordForTest {
    semantics: ProbeSemanticsForTest,
    artifact_sha256: String,
    artifact_bytes: usize,
}

#[cfg(test)]
impl SnapshotSemanticCalibrationRecordForTest {
    pub(in crate::check::checker) fn render(&self) -> String {
        format!(concat!("TYPOKAT_WU0B_SEMANTICS={{\"schema\":1,\"kind\":\"decoded-semantic-calibration\",", "\"profile_sha256\":\"{}\",\"artifact_sha256\":\"{}\",\"artifact_bytes\":{},", "\"semantics\":{{\"fast-clean\":{{\"exit\":{},\"diagnostics\":{}}},\"fast-errors\":{{\"exit\":{},\"diagnostics\":{}}}}}}}"),
            PROFILE_IDENTITY, self.artifact_sha256, self.artifact_bytes,
            self.semantics.fast_clean.exit, json_strings(&self.semantics.fast_clean.diagnostics),
            self.semantics.fast_errors.exit, json_strings(&self.semantics.fast_errors.diagnostics))
    }
}

#[cfg(test)]
fn elapsed_us(start: Instant) -> Result<u64, SnapshotError> {
    u64::try_from(start.elapsed().as_micros()).map_err(|_| {
        invalid(
            SnapshotErrorStage::UserCheck,
            "probe duration exceeds u64 microseconds",
        )
    })
}

#[cfg(test)]
fn peak_rss_bytes() -> Result<u64, SnapshotError> {
    let status = fs::read_to_string("/proc/self/status")
        .map_err(|error| invalid(SnapshotErrorStage::Io, error.to_string()))?;
    let raw_kib = status
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")
                .and_then(|value| value.split_whitespace().next())
        })
        .ok_or_else(|| {
            invalid(
                SnapshotErrorStage::Io,
                "VmHWM is missing from /proc/self/status",
            )
        })?;
    let kib = raw_kib
        .parse::<u64>()
        .map_err(|_| invalid(SnapshotErrorStage::Io, "VmHWM is not a u64"))?;
    let bytes = kib
        .checked_mul(1024)
        .ok_or_else(|| invalid(SnapshotErrorStage::Io, "VmHWM byte count overflows u64"))?;
    if bytes == 0 {
        return Err(invalid(SnapshotErrorStage::Io, "VmHWM is zero"));
    }
    Ok(bytes)
}

#[cfg(test)]
fn diagnostic_identities(name: &str, source: &str, diagnostics: &[Diagnostic]) -> Vec<String> {
    let lines = crate::span::LineIndex::new(source);
    diagnostics
        .iter()
        .map(|diagnostic| {
            let position = lines.line_col(diagnostic.span.start);
            format!(
                "{name}:{}:{}:{}",
                position.line,
                position.column,
                diagnostic.code.as_str().trim_start_matches("TK")
            )
        })
        .collect()
}

#[cfg(test)]
pub(in crate::check::checker) fn snapshot_fast_clean_probe_for_test(
    path: &Path,
    fast_clean: &str,
) -> Result<SnapshotFastCleanProbeRecordForTest, SnapshotError> {
    let compiler_scope = start_library_compiler_measure_for_test();
    let wall = Instant::now();
    let bytes: Cow<'static, [u8]> = Cow::Owned(
        fs::read(path).map_err(|error| invalid(SnapshotErrorStage::Io, error.to_string()))?,
    );
    let artifact_bytes = bytes.len();
    let validation = Instant::now();
    let validated = validate_canonical_snapshot(bytes)?;
    let validation_us = elapsed_us(validation)?;
    let decode = Instant::now();
    let clean_base = decode_canonical_snapshot(validated)?;
    let projection_sha = clean_base.projection.sha256();
    let decode_us = elapsed_us(decode)?;
    let user = Instant::now();
    let clean = check_source_with_decoded_base_for_test(clean_base, fast_clean);
    let user_check_us = elapsed_us(user)?;
    if !clean.parse_errors.is_empty() || !clean.incomplete.is_empty() {
        return Err(invalid(
            SnapshotErrorStage::UserCheck,
            "probe encountered parse or incomplete surfaces",
        ));
    }
    let compiler_measure = compiler_scope.finish();
    Ok(SnapshotFastCleanProbeRecordForTest {
        profile_identity: PROFILE_IDENTITY.to_owned(),
        strategy: SnapshotDecodeStrategy::EagerComplete,
        route: "decoded-base-user-check",
        artifact_bytes,
        validated_bytes: artifact_bytes,
        compiler_measure,
        runtime_projection_sha256: projection_sha,
        semantics: ProbeSemanticsCaseForTest {
            exit: u8::from(!clean.diagnostics.is_empty()),
            diagnostics: diagnostic_identities(
                "fast-clean/main.ts",
                fast_clean,
                &clean.diagnostics,
            ),
        },
        validation_us,
        decode_us,
        user_check_us,
        wall_us: elapsed_us(wall)?,
        peak_rss_bytes: peak_rss_bytes()?,
        artifact_sha256: CANONICAL_ARCHIVE_SHA256.to_owned(),
        input_path: path.to_string_lossy().into_owned(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(in crate::check::checker) struct SnapshotStrategyProbeRecordForTest {
    pub(in crate::check::checker) strategy: SnapshotDecodeStrategy,
    pub(in crate::check::checker) runtime_projection: RuntimeProjectionForTest,
    pub(in crate::check::checker) diagnostic_identities: Vec<String>,
    pub(in crate::check::checker) validated_bytes: usize,
    pub(in crate::check::checker) artifact_bytes: usize,
}

#[cfg(test)]
impl SnapshotStrategyProbeRecordForTest {
    pub(in crate::check::checker) fn render(&self) -> String {
        format!(
            "{{\"strategy\":{:?},\"artifact_bytes\":{}}}",
            self.strategy, self.artifact_bytes
        )
    }
}

#[cfg(test)]
pub(in crate::check::checker) fn snapshot_decode_strategy_probe_for_test(
    path: &Path,
    source: &str,
) -> Result<Vec<SnapshotStrategyProbeRecordForTest>, SnapshotError> {
    let bytes =
        fs::read(path).map_err(|error| invalid(SnapshotErrorStage::Io, error.to_string()))?;
    let base = decode_snapshot_bytes_for_test(&bytes, SnapshotDecodeStrategy::EagerComplete)?;
    let projection = base.projection.clone();
    let result = check_source_with_decoded_base_for_test(base, source);
    let record = SnapshotStrategyProbeRecordForTest {
        strategy: SnapshotDecodeStrategy::EagerComplete,
        runtime_projection: projection,
        diagnostic_identities: diagnostic_identities(
            "fast-clean/main.ts",
            source,
            &result.diagnostics,
        ),
        validated_bytes: bytes.len(),
        artifact_bytes: bytes.len(),
    };
    Err(invalid(
        SnapshotErrorStage::UnsupportedStrategy,
        format!(
            "eager record ready ({} bytes), immutable-indexed strategy is not implemented",
            record.artifact_bytes
        ),
    ))
}

#[cfg(test)]
pub(in crate::check::checker) fn snapshot_regeneration_artifact_for_test(
    path: &Path,
    fast_clean: &str,
    fast_errors: &str,
) -> Result<SnapshotSemanticCalibrationRecordForTest, SnapshotError> {
    let profile = profile::load_strict_profile()
        .map_err(|error| invalid(SnapshotErrorStage::Generation, error.to_string()))?;
    let compiled = compile_snapshot_for_test(&profile.injected_sources())?;
    fs::write(path, compiled.archive.as_bytes())
        .map_err(|error| invalid(SnapshotErrorStage::Io, error.to_string()))?;
    let bytes =
        fs::read(path).map_err(|error| invalid(SnapshotErrorStage::Io, error.to_string()))?;
    let validated = validate_snapshot(&bytes)?;
    let clean_base =
        decode_snapshot_for_test(validated.clone(), SnapshotDecodeStrategy::EagerComplete)?;
    let error_base = decode_snapshot_for_test(validated, SnapshotDecodeStrategy::EagerComplete)?;
    let clean = check_source_with_decoded_base_for_test(clean_base, fast_clean);
    let errors = check_source_with_decoded_base_for_test(error_base, fast_errors);
    if !clean.parse_errors.is_empty()
        || !clean.incomplete.is_empty()
        || !errors.parse_errors.is_empty()
        || !errors.incomplete.is_empty()
    {
        return Err(invalid(
            SnapshotErrorStage::UserCheck,
            "semantic calibration encountered parse or incomplete surfaces",
        ));
    }
    Ok(SnapshotSemanticCalibrationRecordForTest {
        semantics: ProbeSemanticsForTest {
            fast_clean: ProbeSemanticsCaseForTest {
                exit: u8::from(!clean.diagnostics.is_empty()),
                diagnostics: diagnostic_identities(
                    "fast-clean/main.ts",
                    fast_clean,
                    &clean.diagnostics,
                ),
            },
            fast_errors: ProbeSemanticsCaseForTest {
                exit: u8::from(!errors.diagnostics.is_empty()),
                diagnostics: diagnostic_identities(
                    "fast-errors/main.ts",
                    fast_errors,
                    &errors.diagnostics,
                ),
            },
        },
        artifact_sha256: hex(&digest32(&bytes)),
        artifact_bytes: bytes.len(),
    })
}

#[derive(Clone, Debug)]
#[cfg(test)]
pub(in crate::check::checker) struct ScalingSampleForTest {
    checks: usize,
    projection: String,
    compiler_measure: LibraryCompilerMeasureForTest,
}

#[cfg(test)]
pub(in crate::check::checker) struct SnapshotScalingProbeRecordForTest {
    pub(in crate::check::checker) route: &'static str,
    samples: Vec<ScalingSampleForTest>,
}

#[cfg(test)]
impl SnapshotScalingProbeRecordForTest {
    pub(in crate::check::checker) fn check_counts(&self) -> Vec<usize> {
        self.samples.iter().map(|sample| sample.checks).collect()
    }
    pub(in crate::check::checker) fn all_semantically_identical(&self) -> bool {
        self.samples
            .windows(2)
            .all(|pair| pair[0].projection == pair[1].projection)
    }
    pub(in crate::check::checker) fn all_library_compilation_counts_are_zero(&self) -> bool {
        self.samples
            .iter()
            .all(|sample| sample.compiler_measure == LibraryCompilerMeasureForTest::default())
    }
    pub(in crate::check::checker) fn render(&self) -> String {
        format!(
            "{{\"route\":\"{}\",\"counts\":{:?}}}",
            self.route,
            self.check_counts()
        )
    }
}

#[cfg(test)]
pub(in crate::check::checker) fn snapshot_scaling_probe_for_test(
    path: &Path,
    source: &str,
    counts: &[usize],
) -> Result<SnapshotScalingProbeRecordForTest, SnapshotError> {
    let bytes =
        fs::read(path).map_err(|error| invalid(SnapshotErrorStage::Io, error.to_string()))?;
    let mut samples = Vec::new();
    for &checks in counts {
        let compiler_scope = start_library_compiler_measure_for_test();
        let mut projection = None;
        for _ in 0..checks {
            let base =
                decode_snapshot_bytes_for_test(&bytes, SnapshotDecodeStrategy::EagerComplete)?;
            projection = Some(base.projection.sha256());
            let result = check_source_with_decoded_base_for_test(base, source);
            if !result.parse_errors.is_empty()
                || !result.diagnostics.is_empty()
                || !result.incomplete.is_empty()
            {
                return Err(invalid(
                    SnapshotErrorStage::UserCheck,
                    "scaling probe semantics differ",
                ));
            }
        }
        samples.push(ScalingSampleForTest {
            checks,
            projection: projection.unwrap_or_default(),
            compiler_measure: compiler_scope.finish(),
        });
    }
    Ok(SnapshotScalingProbeRecordForTest {
        route: "private-decode-per-caller",
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::LibraryFileOrdinal;

    fn rehash_semantic_section(bytes: &mut [u8], section: &DirectorySection) {
        let section_digest = digest32(&bytes[section.range.clone()]);
        let directory_digest_offset = FIXED_HEADER_LEN + 7 * DIRECTORY_ENTRY_LEN + 20;
        bytes[directory_digest_offset..directory_digest_offset + 32]
            .copy_from_slice(&section_digest);
        let directory_end = FIXED_HEADER_LEN + SECTION_COUNT * DIRECTORY_ENTRY_LEN;
        let body_digest_offset = MAGIC.len() + 4 + 32 + 32 + 4 + 8;
        let body_digest = digest32(&bytes[directory_end..]);
        bytes[body_digest_offset..body_digest_offset + 32].copy_from_slice(&body_digest);
    }

    #[test]
    fn internal_optional_u32_is_tagged_and_preserves_full_domain() {
        let mut writer = SnapshotWriter::new();
        write_optional_u32(&mut writer, Some(u32::MAX));
        write_optional_u32(&mut writer, None);
        let bytes = writer.into_bytes();
        assert_eq!(bytes, [1, 0xff, 0xff, 0xff, 0xff, 0]);
        let mut reader = SnapshotReader::new(&bytes);
        assert_eq!(
            read_optional_u32(&mut reader).expect("full-domain option decodes"),
            Some(u32::MAX)
        );
        assert_eq!(
            read_optional_u32(&mut reader).expect("absent option decodes"),
            None
        );
        assert!(reader.finish().is_ok());

        let mut invalid_tag = SnapshotReader::new(&[2]);
        assert!(read_optional_u32(&mut invalid_tag).is_err());
        let mut truncated = SnapshotReader::new(&[1, 0, 0, 0]);
        assert!(read_optional_u32(&mut truncated).is_err());
    }

    #[test]
    fn small_archive_roundtrip_restores_consumable_runtime() {
        let sources = [InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "snapshot-small.d.ts",
            source: "interface SnapshotSmall { value: string }\ndeclare const snapshotSmall: SnapshotSmall;",
        }];
        let compiled = compile_snapshot_for_test(&sources).expect("small snapshot compiles");
        let decoded = decode_snapshot_bytes_for_test(
            compiled.archive().as_bytes(),
            SnapshotDecodeStrategy::EagerComplete,
        )
        .expect("small snapshot decodes");
        assert_eq!(decoded.runtime_projection(), compiled.runtime_projection());
        let result = check_source_with_decoded_base_for_test(
            decoded,
            "const value: string = snapshotSmall.value;",
        );
        assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);

        let empty_digest = digest32(&[]);
        for name in [
            "class.parents",
            "class.new-metadata",
            "class.value-identities",
            "class.aliases",
        ] {
            let subtable = compiled
                .runtime_projection()
                .subtable(name)
                .expect("small projection subtable");
            assert_eq!(subtable.row_count, 0, "{name}");
            assert_eq!(subtable.sha256, empty_digest, "{name}");
        }
    }

    #[test]
    fn projection_witness_codec_is_versioned_and_fixed_order() {
        let subtables = projection_subtable_names()
            .enumerate()
            .map(|(index, name)| RuntimeSubtableForTest {
                name,
                row_count: count64(index),
                sha256: digest32(name.as_bytes()),
            })
            .collect::<Vec<_>>();
        let mut writer = SnapshotWriter::new();
        write_projection_witness(&mut writer, &subtables).expect("projection witness encodes");
        let bytes = writer.into_bytes();
        let mut reader = SnapshotReader::new(&bytes);
        assert_eq!(read_projection_witness(&mut reader).unwrap(), subtables);
        assert!(reader.finish().is_ok());

        for (offset, replacement) in [(0, 2u32), (4, 30u32)] {
            let mut corrupt = bytes.clone();
            corrupt[offset..offset + 4].copy_from_slice(&replacement.to_be_bytes());
            assert!(read_projection_witness(&mut SnapshotReader::new(&corrupt)).is_err());
        }
    }

    #[test]
    fn manifest_owner_and_family_count_overflow_reject_with_valid_digests() {
        let sources = [InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "snapshot-corruption.d.ts",
            source: "interface SnapshotCorruption { value: string }\ndeclare const snapshotCorruption: SnapshotCorruption;",
        }];
        let compiled = compile_snapshot_for_test(&sources).expect("snapshot compiles");
        let validated =
            validate_snapshot(compiled.archive().as_bytes()).expect("snapshot validates");
        let semantic = validated.sections[7].clone();

        let mut owner_corruption = validated.bytes.to_vec();
        let references_start = semantic.range.start + 4 + 8 + 8 + 9 * 12;
        owner_corruption[references_start + 4..references_start + 8]
            .copy_from_slice(&u32::MAX.to_be_bytes());
        rehash_semantic_section(&mut owner_corruption, &semantic);
        let error = decode_snapshot_bytes_for_test(
            &owner_corruption,
            SnapshotDecodeStrategy::EagerComplete,
        )
        .expect_err("out-of-range manifest owner rejects");
        assert_eq!(
            error.stage(),
            SnapshotErrorStage::ReferenceValidation,
            "{error}"
        );

        let mut count_overflow = validated.bytes.to_vec();
        let family_directory = semantic.range.start + 4 + 8 + 8;
        count_overflow[family_directory + 4..family_directory + 12]
            .copy_from_slice(&u64::MAX.to_be_bytes());
        count_overflow[family_directory + 12 + 4..family_directory + 12 + 12]
            .copy_from_slice(&u64::MAX.to_be_bytes());
        rehash_semantic_section(&mut count_overflow, &semantic);
        let error =
            decode_snapshot_bytes_for_test(&count_overflow, SnapshotDecodeStrategy::EagerComplete)
                .expect_err("overflowing family count sum rejects");
        assert_eq!(
            error.stage(),
            SnapshotErrorStage::ReferenceValidation,
            "{error}"
        );
    }

    #[test]
    fn manifest_retains_unavailable_poisoned_and_empty_typed_rows() {
        use super::super::namespace_values::{
            FrozenNamespaceValueTerminalSnapshot, NamespaceValueUnavailableCause,
        };
        let published = PublishedTypeEnvironmentSnapshotParts {
            groups: vec![PublishedTypeGroupTerminal::Unavailable(
                PublishedTypeGroupUnavailable {
                    cause: TypeGroupUnavailableCause::UnsupportedComposition,
                },
            )],
            classes: vec![(
                ClassId(0),
                PublishedClassSnapshotTerminal::Poisoned(PublishedClassPoison::Surface),
            )],
        };
        let namespace_terminals = vec![FrozenNamespaceValueTerminalSnapshotRow {
            namespace: NamespaceId(0),
            terminal: FrozenNamespaceValueTerminalSnapshot::Unavailable(
                NamespaceValueUnavailableCause::UnsupportedExportedMember,
            ),
        }];
        let runtime = FrozenCheckerRuntimeSnapshotParts {
            class_application_parameters: vec![(ClassId(0), Vec::new())],
            class_new_metadata: Vec::new(),
            class_parents: Vec::new(),
            class_value_aliases: Vec::new(),
            class_value_bindings: Vec::new(),
            standalone_namespace_value_aliases: Vec::new(),
            class_names: vec![(ClassId(0), "EmptyClass".to_owned())],
            namespace_terminals: namespace_terminals.clone(),
            named_function_symbols: Vec::new(),
        };
        let references = build_manifest_references(ManifestInputs {
            type_count: 0,
            roots: &[],
            store_references: &[],
            interner_references: &[],
            binder_references: &[],
            decl_types: &[None],
            published: &published,
            namespace_terminals: &namespace_terminals,
            runtime: &runtime,
            semantic_identities: &None,
        })
        .expect("typed empty rows enumerate");
        for expected in [
            ManifestReference {
                owner_family: 4,
                owner_domain: 9,
                target_domain: 9,
                field: ROW_IDENTITY_FIELD,
                owner: 0,
                target: 0,
            },
            ManifestReference {
                owner_family: 5,
                owner_domain: 7,
                target_domain: 7,
                field: ROW_IDENTITY_FIELD,
                owner: 0,
                target: 0,
            },
            ManifestReference {
                owner_family: 5,
                owner_domain: 3,
                target_domain: 3,
                field: ROW_IDENTITY_FIELD,
                owner: 0,
                target: 0,
            },
            ManifestReference {
                owner_family: 6,
                owner_domain: 8,
                target_domain: 8,
                field: ROW_IDENTITY_FIELD,
                owner: 0,
                target: 0,
            },
            ManifestReference {
                owner_family: 7,
                owner_domain: 3,
                target_domain: 3,
                field: APPLICATION_ROW_FIELD,
                owner: 0,
                target: 0,
            },
            ManifestReference {
                owner_family: 7,
                owner_domain: 3,
                target_domain: 3,
                field: CLASS_NAME_ROW_FIELD,
                owner: 0,
                target: 0,
            },
        ] {
            assert!(references.contains(&expected), "{expected:?}");
        }
    }

    #[test]
    fn canonical_decoder_matches_generic_and_uses_scoped_evidence() {
        let bytes = crate::library::artifact::packaged_canonical_snapshot().bytes();
        let generic = decode_snapshot_bytes_for_test(bytes, SnapshotDecodeStrategy::EagerComplete)
            .expect("generic decoder succeeds");
        let canonical = decode_canonical_snapshot(
            validate_snapshot(bytes).expect("canonical artifact validates"),
        )
        .expect("canonical decoder succeeds");
        assert_eq!(canonical.projection, generic.projection);
        assert_eq!(canonical.identity, generic.identity);
        assert_eq!(canonical.source_file_count, generic.source_file_count);
        assert_eq!(canonical.prefix_lengths, generic.prefix_lengths);
        assert_eq!(canonical.root_names, generic.root_names);
        assert_eq!(canonical.root_counts, generic.root_counts);
        assert_eq!(canonical.strategy, generic.strategy);
        let projection_sha256 = canonical.projection.sha256();
        let compiler = start_library_compiler_measure_for_test();
        let route = start_decoded_base_route_measure_for_test();
        let result = check_source_with_decoded_base_for_test(
            canonical,
            "const value: string = ''.toUpperCase();",
        );
        let route = route.finish();
        assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
        assert_eq!(compiler.finish(), LibraryCompilerMeasureForTest::default());
        assert_eq!(route.user_checks, 1);
        assert_eq!(route.runtime_projection_sha256, projection_sha256);
    }
}

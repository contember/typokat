//! Strict admission and decoding of the canonical semantic snapshot.

use super::artifact::{
    packaged_canonical_snapshot, verify_and_admit_canonical_snapshot, AdmittedCanonicalSnapshot,
    SnapshotVerificationError, CANONICAL_SNAPSHOT_BYTES, CANONICAL_SNAPSHOT_SHA256,
};
use super::base::FrozenLibraryIdentity;
use super::provider::{
    CollisionReplayIndexViolation, LibraryInitCause, LibraryInitError, LibraryInitStage,
    LibrarySnapshotViolation,
};
use crate::check::checker::library_compiler::OwnedLibraryRuntimeState;
use crate::check::checker::library_snapshot_codec::{
    SnapshotError, SnapshotErrorKind, SnapshotErrorStage,
};
use std::borrow::Cow;
use std::collections::BTreeSet;

#[cfg(test)]
thread_local! {
    static SNAPSHOT_VALIDATIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static SNAPSHOT_DECODES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SnapshotWorkForTest {
    pub(super) validations: u64,
    pub(super) decodes: u64,
}

#[cfg(test)]
fn snapshot_work_for_test() -> SnapshotWorkForTest {
    SnapshotWorkForTest {
        validations: SNAPSHOT_VALIDATIONS.get(),
        decodes: SNAPSHOT_DECODES.get(),
    }
}

#[cfg(test)]
pub(super) struct SnapshotWorkScopeForTest(SnapshotWorkForTest);

#[cfg(test)]
impl SnapshotWorkScopeForTest {
    pub(super) fn start() -> Self {
        Self(snapshot_work_for_test())
    }

    pub(super) fn finish(self) -> SnapshotWorkForTest {
        let end = snapshot_work_for_test();
        SnapshotWorkForTest {
            validations: end.validations.saturating_sub(self.0.validations),
            decodes: end.decodes.saturating_sub(self.0.decodes),
        }
    }
}

pub(super) const PROFILE_SHA256: &str =
    "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
pub(super) const SCHEMA_SHA256: &str =
    "6cf27cde368f8b2ff3bdafd5fce8fb3550ec8e2264aab7249362e2294e3f5be0";

pub(super) struct DecodedCanonicalLibrary {
    pub(super) runtime: OwnedLibraryRuntimeState,
    pub(super) root_names: BTreeSet<String>,
    pub(super) prefixes: [usize; 9],
    #[cfg(test)]
    pub(super) typed_validation_sha256: [u8; 32],
    pub(super) identity: FrozenLibraryIdentity,
}

pub(super) fn admit_packaged_canonical() -> Result<AdmittedCanonicalSnapshot, LibraryInitError> {
    #[cfg(test)]
    SNAPSHOT_VALIDATIONS.set(SNAPSHOT_VALIDATIONS.get().saturating_add(1));
    let bytes = packaged_canonical_snapshot().bytes();
    verify_and_admit_canonical_snapshot(
        Cow::Borrowed(bytes),
        packaged_canonical_snapshot().binding(),
    )
    .map_err(map_admission_error)
}

pub(super) fn decode_admitted_canonical_with_evidence(
    admitted: AdmittedCanonicalSnapshot,
) -> Result<
    (
        DecodedCanonicalLibrary,
        crate::check::checker::library_snapshot_codec::AdmittedLibrarySnapshotEvidence,
    ),
    LibraryInitError,
> {
    #[cfg(test)]
    SNAPSHOT_DECODES.set(SNAPSHOT_DECODES.get().saturating_add(1));
    crate::check::checker::library_snapshot_codec::decode_canonical_library_snapshot_with_evidence(
        admitted,
    )
    .map(|(decoded, evidence)| {
        (
            DecodedCanonicalLibrary {
                runtime: decoded.runtime,
                root_names: decoded.root_names,
                prefixes: decoded.prefixes,
                #[cfg(test)]
                typed_validation_sha256: decoded.typed_validation_sha256,
                identity: FrozenLibraryIdentity::canonical(),
            },
            evidence,
        )
    })
    .map_err(map_snapshot_error)
}

#[cfg(test)]
pub(super) fn admit_canonical_for_test(
    bytes: Vec<u8>,
) -> Result<AdmittedCanonicalSnapshot, LibraryInitError> {
    SNAPSHOT_VALIDATIONS.set(SNAPSHOT_VALIDATIONS.get().saturating_add(1));
    verify_and_admit_canonical_snapshot(Cow::Owned(bytes), packaged_canonical_snapshot().binding())
        .map_err(map_admission_error)
}

fn map_admission_error(error: SnapshotVerificationError) -> LibraryInitError {
    let (expected_bytes, actual_bytes, expected_sha256, actual_sha256) = match error {
        SnapshotVerificationError::ArtifactIdentityMismatch {
            expected_bytes,
            actual_bytes,
            expected_sha256,
            actual_sha256,
        } => (expected_bytes, actual_bytes, expected_sha256, actual_sha256),
        SnapshotVerificationError::ProfileIdentityMismatch { expected, actual }
        | SnapshotVerificationError::CompilerSchemaMismatch { expected, actual } => {
            (CANONICAL_SNAPSHOT_BYTES, 0, expected, actual)
        }
        SnapshotVerificationError::Generation(_) | SnapshotVerificationError::Io(_) => (
            CANONICAL_SNAPSHOT_BYTES,
            0,
            CANONICAL_SNAPSHOT_SHA256.to_owned(),
            "unavailable".to_owned(),
        ),
    };
    LibraryInitError::new(
        LibraryInitStage::ArtifactAdmission,
        LibraryInitCause::ArtifactIdentity {
            expected_bytes,
            actual_bytes,
            expected_sha256,
            actual_sha256,
        },
    )
}

pub(super) fn map_snapshot_error(error: SnapshotError) -> LibraryInitError {
    let stage = match error.kind() {
        SnapshotErrorKind::WorkerPanicked { worker }
        | SnapshotErrorKind::WorkerSpawnFailed { worker }
            if worker.starts_with("interner") =>
        {
            LibraryInitStage::DecodeInterner
        }
        SnapshotErrorKind::WorkerPanicked { worker }
        | SnapshotErrorKind::WorkerSpawnFailed { worker }
            if worker.starts_with("binder") =>
        {
            LibraryInitStage::DecodeBinder
        }
        _ => match error.stage() {
            SnapshotErrorStage::HeaderValidation => LibraryInitStage::Header,
            SnapshotErrorStage::DirectoryValidation => LibraryInitStage::Directory,
            SnapshotErrorStage::PayloadValidation => LibraryInitStage::Payload,
            SnapshotErrorStage::ReferenceValidation => LibraryInitStage::ReferenceValidation,
            SnapshotErrorStage::Publication => LibraryInitStage::Publication,
            SnapshotErrorStage::Decode => LibraryInitStage::Decode,
            SnapshotErrorStage::CollisionReplayIndexAdmission => {
                LibraryInitStage::CollisionReplayIndexAdmission
            }
            SnapshotErrorStage::Generation => LibraryInitStage::Decode,
            #[cfg(test)]
            SnapshotErrorStage::UnsupportedStrategy
            | SnapshotErrorStage::Io
            | SnapshotErrorStage::UserCheck => LibraryInitStage::Decode,
        },
    };
    let cause = match error.kind() {
        SnapshotErrorKind::InvalidId { id, limit } => LibraryInitCause::InvalidId {
            id: *id,
            limit: *limit,
        },
        SnapshotErrorKind::WorkerPanicked { worker } => LibraryInitCause::WorkerPanicked { worker },
        SnapshotErrorKind::WorkerSpawnFailed { worker } => {
            LibraryInitCause::WorkerSpawnFailed { worker }
        }
        SnapshotErrorKind::ReplayIndexRejected(violation) => {
            LibraryInitCause::ReplayIndexRejected {
                violation: match violation {
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::InvalidEncoding => CollisionReplayIndexViolation::InvalidEncoding,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::InvalidOwnerPartition => CollisionReplayIndexViolation::InvalidOwnerPartition,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::InvalidRootIndex => CollisionReplayIndexViolation::InvalidRootIndex,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::InvalidDependencyGraph => CollisionReplayIndexViolation::InvalidDependencyGraph,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::InvalidOwnerSites => CollisionReplayIndexViolation::InvalidOwnerSites,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::InvalidSccPartition => CollisionReplayIndexViolation::InvalidSccPartition,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::InvalidStatementPartition => CollisionReplayIndexViolation::InvalidStatementPartition,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::InvalidBaselinePartition => CollisionReplayIndexViolation::InvalidBaselinePartition,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::NonzeroGenerationHealthCounter => CollisionReplayIndexViolation::NonzeroGenerationHealthCounter,
                    crate::check::checker::replay_index::ReplayIndexAdmissionError::ManifestIdentityMismatch => CollisionReplayIndexViolation::ManifestIdentityMismatch,
                },
            }
        }
        SnapshotErrorKind::Rejected => LibraryInitCause::SnapshotRejected {
            violation: match stage {
                LibraryInitStage::Header => LibrarySnapshotViolation::MalformedHeader,
                LibraryInitStage::Directory => LibrarySnapshotViolation::MalformedDirectory,
                LibraryInitStage::Payload => LibrarySnapshotViolation::InvalidPayload,
                LibraryInitStage::ReferenceValidation => LibrarySnapshotViolation::InvalidReference,
                LibraryInitStage::Publication => LibrarySnapshotViolation::IncompletePublication,
                LibraryInitStage::ArtifactAdmission
                | LibraryInitStage::Decode
                | LibraryInitStage::DecodeInterner
                | LibraryInitStage::DecodeBinder
                | LibraryInitStage::CollisionReplayIndexAdmission => {
                    LibrarySnapshotViolation::InvalidEncoding
                }
            },
        },
    };
    LibraryInitError::new(stage, cause)
}

#[cfg(test)]
pub(super) fn decode_pre_admitted_with_evidence(
    snapshot: &test_support::PreAdmittedSnapshot,
) -> Result<
    (
        DecodedCanonicalLibrary,
        crate::check::checker::library_snapshot_codec::AdmittedLibrarySnapshotEvidence,
    ),
    LibraryInitError,
> {
    SNAPSHOT_DECODES.set(SNAPSHOT_DECODES.get().saturating_add(1));
    crate::check::checker::library_snapshot_codec::decode_pre_admitted_library_snapshot_with_evidence(
        &snapshot.bytes,
    )
    .map(|(decoded, evidence)| {
        (
            DecodedCanonicalLibrary {
                runtime: decoded.runtime,
                root_names: decoded.root_names,
                prefixes: decoded.prefixes,
                #[cfg(test)]
                typed_validation_sha256: decoded.typed_validation_sha256,
                identity: FrozenLibraryIdentity::canonical(),
            },
            evidence,
        )
    })
    .map_err(map_snapshot_error)
}

#[cfg(test)]
pub(super) fn pre_admitted_replay_index_mutation_for_test(
    mutation: super::base::ReplayIndexMutationForTest,
) -> test_support::PreAdmittedSnapshot {
    test_support::mutate_replay_index(mutation)
        .map(|bytes| test_support::PreAdmittedSnapshot { bytes })
        .expect("replay-index mutation fixture")
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;
    use crate::library::base::CanonicalLibraryProjection;
    use crate::library::compiler::CompiledLibrary;
    use sha2::{Digest, Sha256};

    const MAGIC: &[u8] = b"typokat-semantic-snapshot";
    const VERSION_OFFSET: usize = MAGIC.len();
    const PROFILE_DIGEST_LEN: usize = 32;
    const SCHEMA_DIGEST_LEN: usize = 32;
    const BODY_DIGEST_LEN: usize = 32;
    const BODY_DIGEST_OFFSET: usize =
        MAGIC.len() + 4 + PROFILE_DIGEST_LEN + SCHEMA_DIGEST_LEN + 4 + 8;
    const FIXED_HEADER_LEN: usize = BODY_DIGEST_OFFSET + BODY_DIGEST_LEN;
    const DIRECTORY_ENTRY_LEN: usize = 52;
    const SECTION_COUNT: usize = 11;
    const REFERENCE_FAMILY_COUNT: usize = 9;
    const REFERENCE_FAMILY_ENTRY_LEN: usize = 12;
    const REFERENCE_ENTRY_LEN: usize = 12;

    #[derive(Clone, Copy)]
    struct Section {
        tag: u16,
        directory_offset: usize,
        payload_offset: usize,
        payload_len: usize,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum ReferenceEndpoint {
        First,
        Last,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum SnapshotTestMutation {
        BadMagic,
        UnknownVersion,
        WrongProfileIdentity,
        WrongSchemaIdentity,
        WrongBodyDigest,
        WrongSectionDigest,
        UnknownSectionTag,
        DuplicateSectionTag,
        ReorderedSections,
        NonZeroReservedDirectoryField,
        OverlappingSection,
        GappedSection,
        LengthOverflow,
        TruncatedPayload,
        TrailingBytes,
        NextIdMismatch,
        NextTypeParamIdMismatch,
        NextClassIdMismatch,
        InternerBucketMismatch,
        RootIndexBinderMismatch,
        NonTerminalPublication,
        DanglingReference {
            family: usize,
            endpoint: ReferenceEndpoint,
        },
        InvalidReferenceOwner,
        InvalidReferenceDomain,
        InvalidReferenceField,
    }

    pub(crate) struct PreAdmittedSnapshot {
        pub(super) bytes: Vec<u8>,
    }

    pub(crate) fn canonical_projection_from_compiled_for_test(
        compiled: &CompiledLibrary,
    ) -> Result<CanonicalLibraryProjection, LibraryInitError> {
        crate::check::checker::library_snapshot_codec::projection_from_library_product(
            &compiled.runtime_projection()._runtime,
        )
        .map(CanonicalLibraryProjection::new)
        .map_err(map_snapshot_error)
    }

    pub(crate) fn canonical_bytes_with_mutation_for_test(
        mutation: SnapshotTestMutation,
    ) -> Result<Vec<u8>, LibraryInitError> {
        mutate_canonical(mutation).map_err(|violation| {
            LibraryInitError::new(
                LibraryInitStage::Decode,
                LibraryInitCause::SnapshotRejected { violation },
            )
        })
    }

    pub(crate) fn pre_admitted_snapshot_case_for_test(
        mutation: SnapshotTestMutation,
    ) -> Result<PreAdmittedSnapshot, LibraryInitError> {
        mutate_canonical(mutation)
            .map(|bytes| PreAdmittedSnapshot { bytes })
            .map_err(|violation| {
                LibraryInitError::new(
                    LibraryInitStage::Decode,
                    LibraryInitCause::SnapshotRejected { violation },
                )
            })
    }

    fn skip_owner(bytes: &[u8], cursor: &mut usize) -> Result<(), LibrarySnapshotViolation> {
        let tag = *bytes
            .get(*cursor)
            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
        *cursor += match tag {
            0..=3 => 5,
            4 => 1,
            5 => 29,
            _ => return Err(LibrarySnapshotViolation::InvalidEncoding),
        };
        Ok(())
    }

    fn skip_optional(bytes: &[u8], cursor: &mut usize) -> Result<(), LibrarySnapshotViolation> {
        let tag = *bytes
            .get(*cursor)
            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
        *cursor += match tag {
            0 => 1,
            1 => 5,
            _ => return Err(LibrarySnapshotViolation::InvalidEncoding),
        };
        Ok(())
    }

    struct ReplayLayout {
        schema: usize,
        owner_count: usize,
        first_owner_tag: usize,
        first_root_name_len: usize,
        first_root_name_byte: usize,
        first_optional_tag: usize,
        first_boolean_tag: usize,
        first_root_consumer_slot: usize,
        health: usize,
    }

    fn replay_layout(
        bytes: &[u8],
        section: Section,
    ) -> Result<ReplayLayout, LibrarySnapshotViolation> {
        let schema = section.payload_offset + b"typokat-collision-replay-index-v1".len();
        let mut cursor = schema + 4;
        let owner_count = cursor;
        let owners = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
        cursor += 8;
        let first_owner_tag = cursor;
        for _ in 0..owners {
            skip_owner(bytes, &mut cursor)?;
        }

        let roots = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
        cursor += 8;
        let first_root_name_len = cursor;
        let first_root_name_byte = cursor + 8;
        let mut first_optional_tag = None;
        let mut first_boolean_tag = None;
        for _ in 0..roots {
            let name_len = usize::try_from(read_u64(bytes, cursor)?)
                .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
            cursor += 8 + name_len;
            for _ in 0..3 {
                first_optional_tag.get_or_insert(cursor);
                skip_optional(bytes, &mut cursor)?;
            }
            first_boolean_tag.get_or_insert(cursor);
            cursor += 2;
        }

        let sites = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
        cursor += 8;
        for _ in 0..sites {
            skip_owner(bytes, &mut cursor)?;
            cursor += 16;
        }

        let edges = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
        cursor += 8;
        for _ in 0..edges {
            skip_owner(bytes, &mut cursor)?;
            skip_owner(bytes, &mut cursor)?;
        }

        let root_consumers = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
        cursor += 8;
        let mut first_root_consumer_slot = None;
        for _ in 0..root_consumers {
            let name_len = usize::try_from(read_u64(bytes, cursor)?)
                .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
            cursor += 8 + name_len;
            first_root_consumer_slot.get_or_insert(cursor);
            cursor += 1;
            skip_owner(bytes, &mut cursor)?;
        }

        let sccs = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
        cursor += 8;
        for _ in 0..sccs {
            cursor += 4;
            let members = usize::try_from(read_u64(bytes, cursor)?)
                .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
            cursor += 8;
            for _ in 0..members {
                skip_owner(bytes, &mut cursor)?;
            }
        }

        let statements = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
        cursor += 8;
        for _ in 0..statements {
            skip_owner(bytes, &mut cursor)?;
            skip_owner(bytes, &mut cursor)?;
        }

        let baselines = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
        cursor += 8;
        for _ in 0..baselines {
            skip_owner(bytes, &mut cursor)?;
            cursor += 40;
        }
        Ok(ReplayLayout {
            schema,
            owner_count,
            first_owner_tag,
            first_root_name_len,
            first_root_name_byte,
            first_optional_tag: first_optional_tag
                .ok_or(LibrarySnapshotViolation::InvalidEncoding)?,
            first_boolean_tag: first_boolean_tag
                .ok_or(LibrarySnapshotViolation::InvalidEncoding)?,
            first_root_consumer_slot: first_root_consumer_slot
                .ok_or(LibrarySnapshotViolation::InvalidEncoding)?,
            health: cursor,
        })
    }

    fn replace_replay_payload(
        bytes: &mut Vec<u8>,
        section: Section,
        payload: &[u8],
        directory_end: usize,
    ) -> Result<(), LibrarySnapshotViolation> {
        if section.payload_offset + section.payload_len != bytes.len() {
            return Err(LibrarySnapshotViolation::MalformedDirectory);
        }
        bytes.truncate(section.payload_offset);
        bytes.extend_from_slice(payload);
        write_u64(
            bytes,
            section.directory_offset + 12,
            u64::try_from(payload.len()).map_err(|_| LibrarySnapshotViolation::InvalidPayload)?,
        )?;
        let body_len = u64::try_from(bytes.len() - directory_end)
            .map_err(|_| LibrarySnapshotViolation::InvalidPayload)?;
        write_u64(bytes, BODY_DIGEST_OFFSET - 8, body_len)?;
        rehash_section_and_body(
            bytes,
            Section {
                payload_len: payload.len(),
                ..section
            },
            directory_end,
        )
    }

    pub(super) fn mutate_replay_index(
        mutation: super::super::base::ReplayIndexMutationForTest,
    ) -> Result<Vec<u8>, LibrarySnapshotViolation> {
        use super::super::base::ReplayIndexMutationForTest as M;

        let original = packaged_canonical_snapshot().bytes();
        let sections = parse_directory(original)?;
        let replay = find_section(&sections, 11)?;
        let directory_end = FIXED_HEADER_LEN + SECTION_COUNT * DIRECTORY_ENTRY_LEN;
        let mut bytes = original.to_vec();
        match mutation {
            M::MissingSection => {
                write_u32(
                    &mut bytes,
                    VERSION_OFFSET + 4 + PROFILE_DIGEST_LEN + SCHEMA_DIGEST_LEN,
                    10,
                )?;
                return Ok(bytes);
            }
            M::WrongSectionTag => {
                write_u16(&mut bytes, replay.directory_offset, u16::MAX)?;
                return Ok(bytes);
            }
            M::WrongSectionDigest => {
                bytes[replay.directory_offset + 20] ^= 1;
                return Ok(bytes);
            }
            M::TruncatedSection => {
                bytes.pop();
                return Ok(bytes);
            }
            M::TrailingSectionBytes => {
                bytes.push(0);
                return Ok(bytes);
            }
            _ => {}
        }

        let layout = replay_layout(&bytes, replay)?;
        match mutation {
            M::WrongManifestDomain => bytes[replay.payload_offset] ^= 1,
            M::UnknownSchema => write_u32(&mut bytes, layout.schema, 2)?,
            M::TruncatedInternalLength => {
                let mut payload = bytes
                    [replay.payload_offset..replay.payload_offset + replay.payload_len]
                    .to_vec();
                payload.pop();
                replace_replay_payload(&mut bytes, replay, &payload, directory_end)?;
                return Ok(bytes);
            }
            M::InternalLengthOverflow => {
                write_u64(&mut bytes, layout.owner_count, u64::MAX)?;
            }
            M::InvalidUtf8RootName => bytes[layout.first_root_name_byte] = 0xff,
            M::InvalidOptionalTag => bytes[layout.first_optional_tag] = 2,
            M::InvalidBooleanTag => bytes[layout.first_boolean_tag] = 2,
            M::SemanticTrailingBytes => {
                let mut payload = bytes
                    [replay.payload_offset..replay.payload_offset + replay.payload_len]
                    .to_vec();
                payload.push(0);
                replace_replay_payload(&mut bytes, replay, &payload, directory_end)?;
                return Ok(bytes);
            }
            M::InvalidOwnerTag => bytes[layout.first_owner_tag] = u8::MAX,
            M::RootNameLengthOverflow => {
                write_u64(&mut bytes, layout.first_root_name_len, u64::MAX)?;
            }
            M::InvalidRootConsumerSlot => bytes[layout.first_root_consumer_slot] = u8::MAX,
            M::NonzeroUnownedDemands => write_u64(&mut bytes, layout.health, 1)?,
            M::NonzeroInvalidOwnerSites => write_u64(&mut bytes, layout.health + 8, 1)?,
            M::NonzeroNoncanonicalEdges => write_u64(&mut bytes, layout.health + 16, 1)?,
            M::NonzeroTypedReferenceMisses => write_u64(&mut bytes, layout.health + 24, 1)?,
            M::MissingOwner
            | M::MissingStaleButInRangeOwner
            | M::DuplicateOwner
            | M::ReorderedOwner
            | M::UnknownOwner
            | M::MissingGlobalObjectOwner
            | M::DuplicateGlobalObjectOwner
            | M::DuplicateRoot
            | M::EmptyRootName
            | M::RootIdOutsidePrefix
            | M::PopulatedRootIndexMismatch
            | M::MissingCanonicalPopulatedRoot
            | M::UnusedPlaceholderRoot
            | M::DuplicateReverseEdge
            | M::SelfReverseEdge
            | M::ReorderedReverseEdge
            | M::UnknownReverseEdgeOwner
            | M::DuplicateRootConsumer
            | M::ReorderedRootConsumer
            | M::UnknownRootConsumerOwner
            | M::UnknownRootConsumerName
            | M::MissingOwnerSite
            | M::DuplicateOwnerSite
            | M::InvalidOwnerSiteSpan
            | M::UnknownOwnerSiteOwner
            | M::OwnerSiteFileOutsideProfile
            | M::InvalidScc
            | M::MissingSccOwner
            | M::DuplicateSccOwner
            | M::ReorderedScc
            | M::WrongDependencyFirstScc
            | M::WrongStatementOwner
            | M::DuplicateStatementOwner
            | M::MissingStatementOwner
            | M::StatementFileOutsideProfile
            | M::ReorderedStatementOwner
            | M::NonStatementBaselineCountNonzero
            | M::NonStatementBaselineDigestNoncanonical
            | M::DuplicateBaseline
            | M::MissingBaseline
            | M::SelfConsistentMissingReverseEdge
            | M::SelfConsistentMissingRootConsumer
            | M::SelfConsistentMissingOwnerSite
            | M::SelfConsistentWrongBaseline
            | M::SelfConsistentWrongBaselineCount
            | M::SelfConsistentWrongRootProvenance
            | M::SelfConsistentCrossArtifactSection
            | M::SelfConsistentButUnpinned => {
                use crate::binder::declaration::{TypeGroupId, ValueStorageId};
                use crate::check::checker::replay_index::ReplayOwner;
                use crate::source::LibraryFileOrdinal;
                use crate::span::Span;

                let payload =
                    &bytes[replay.payload_offset..replay.payload_offset + replay.payload_len];
                let mut index =
                    crate::check::checker::replay_index::decode_collision_replay_index_for_test(
                        payload,
                    )
                    .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
                match mutation {
                    M::MissingOwner => {
                        index.owner_partition.remove(0);
                    }
                    M::MissingStaleButInRangeOwner => {
                        index.owner_partition.remove(1);
                    }
                    M::DuplicateOwner => {
                        index.owner_partition.insert(1, index.owner_partition[0]);
                    }
                    M::ReorderedOwner => index.owner_partition.swap(0, 1),
                    M::UnknownOwner => {
                        index.owner_partition[0] = ReplayOwner::TypeGroup(TypeGroupId(u32::MAX));
                    }
                    M::MissingGlobalObjectOwner => index
                        .owner_partition
                        .retain(|owner| *owner != ReplayOwner::GlobalObject),
                    M::DuplicateGlobalObjectOwner => {
                        let position = index
                            .owner_partition
                            .iter()
                            .position(|owner| *owner == ReplayOwner::GlobalObject)
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        index
                            .owner_partition
                            .insert(position + 1, ReplayOwner::GlobalObject);
                    }
                    M::DuplicateRoot => index.root_slots.insert(1, index.root_slots[0].clone()),
                    M::EmptyRootName => index.root_slots[0].name.clear(),
                    M::RootIdOutsidePrefix => {
                        let root = index
                            .root_slots
                            .iter_mut()
                            .find(|root| root.value.is_some())
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        root.value = Some(ValueStorageId(u32::MAX));
                    }
                    M::PopulatedRootIndexMismatch => {
                        let root = index
                            .root_slots
                            .iter_mut()
                            .find(|root| root.value.is_some())
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        let current = root.value.expect("selected populated root").0;
                        root.value = Some(ValueStorageId(if current == 0 { 1 } else { 0 }));
                    }
                    M::MissingCanonicalPopulatedRoot => {
                        let consumed_names = index
                            .root_slot_consumers
                            .iter()
                            .map(|consumer| consumer.name.as_str())
                            .collect::<rustc_hash::FxHashSet<_>>();
                        let position = index
                            .root_slots
                            .iter()
                            .position(|root| {
                                (root.value.is_some()
                                    || root.ty.is_some()
                                    || root.namespace.is_some())
                                    && !consumed_names.contains(root.name.as_str())
                            })
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        index.root_slots.remove(position);
                    }
                    M::UnusedPlaceholderRoot => {
                        index.root_slots.push(
                            crate::check::checker::replay_index::ReplayRootSlot {
                                name: "\u{10ffff}-unused-placeholder".to_owned(),
                                value: None,
                                ty: None,
                                namespace: None,
                                global_object_contributor: false,
                                explicit_global_this: false,
                            },
                        );
                        index
                            .root_slots
                            .sort_by(|left, right| left.name.cmp(&right.name));
                    }
                    M::DuplicateReverseEdge => {
                        index.reverse_edges.insert(1, index.reverse_edges[0]);
                    }
                    M::SelfReverseEdge => {
                        index.reverse_edges[0].consumer = index.reverse_edges[0].dependency;
                    }
                    M::ReorderedReverseEdge => index.reverse_edges.swap(0, 1),
                    M::UnknownReverseEdgeOwner => {
                        index.reverse_edges[0].dependency =
                            ReplayOwner::TypeGroup(TypeGroupId(u32::MAX));
                    }
                    M::DuplicateRootConsumer => index
                        .root_slot_consumers
                        .insert(1, index.root_slot_consumers[0].clone()),
                    M::ReorderedRootConsumer => index.root_slot_consumers.swap(0, 1),
                    M::UnknownRootConsumerOwner => {
                        index.root_slot_consumers[0].consumer =
                            ReplayOwner::TypeGroup(TypeGroupId(u32::MAX));
                    }
                    M::UnknownRootConsumerName => {
                        index.root_slot_consumers[0].name.push_str("-unknown");
                    }
                    M::MissingOwnerSite => {
                        let owner = index.owner_sites[0].owner;
                        index.owner_sites.retain(|site| site.owner != owner);
                    }
                    M::DuplicateOwnerSite => {
                        index.owner_sites.insert(1, index.owner_sites[0].clone());
                    }
                    M::InvalidOwnerSiteSpan => {
                        index.owner_sites[0].span = Span::new(1, 0);
                    }
                    M::UnknownOwnerSiteOwner => {
                        index.owner_sites[0].owner = ReplayOwner::TypeGroup(TypeGroupId(u32::MAX));
                    }
                    M::OwnerSiteFileOutsideProfile => {
                        index.owner_sites[0].file_ordinal = LibraryFileOrdinal::new(usize::MAX);
                    }
                    M::InvalidScc => index.scc_membership[0].replay_ordinal = u32::MAX,
                    M::MissingSccOwner => {
                        index.scc_membership[0].owners.remove(0);
                    }
                    M::DuplicateSccOwner => {
                        let owner = index.scc_membership[0].owners[0];
                        index.scc_membership[0].owners.push(owner);
                        index.scc_membership[0].owners.sort_unstable();
                    }
                    M::ReorderedScc => {
                        let owner_scc = index
                            .scc_membership
                            .iter()
                            .enumerate()
                            .flat_map(|(ordinal, component)| {
                                component
                                    .owners
                                    .iter()
                                    .copied()
                                    .map(move |owner| (owner, ordinal))
                            })
                            .collect::<rustc_hash::FxHashMap<_, _>>();
                        let pair = (0..index.scc_membership.len() - 1)
                            .find(|left| {
                                let right = left + 1;
                                !index.reverse_edges.iter().any(|edge| {
                                    let dependency = owner_scc[&edge.dependency];
                                    let consumer = owner_scc[&edge.consumer];
                                    (dependency == *left && consumer == right)
                                        || (dependency == right && consumer == *left)
                                })
                            })
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        index.scc_membership.swap(pair, pair + 1);
                        for (ordinal, component) in index.scc_membership.iter_mut().enumerate() {
                            component.replay_ordinal = u32::try_from(ordinal)
                                .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
                        }
                    }
                    M::WrongDependencyFirstScc => {
                        let owner_scc = index
                            .scc_membership
                            .iter()
                            .enumerate()
                            .flat_map(|(ordinal, component)| {
                                component
                                    .owners
                                    .iter()
                                    .copied()
                                    .map(move |owner| (owner, ordinal))
                            })
                            .collect::<rustc_hash::FxHashMap<_, _>>();
                        let edge = index
                            .reverse_edges
                            .iter()
                            .find(|edge| owner_scc[&edge.dependency] != owner_scc[&edge.consumer])
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        let dependency = owner_scc[&edge.dependency];
                        let consumer = owner_scc[&edge.consumer];
                        index.scc_membership.swap(dependency, consumer);
                        for (ordinal, component) in index.scc_membership.iter_mut().enumerate() {
                            component.replay_ordinal = u32::try_from(ordinal)
                                .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
                        }
                    }
                    M::WrongStatementOwner => {
                        index.statement_owners[0].1 = ReplayOwner::GlobalObject;
                    }
                    M::DuplicateStatementOwner => {
                        index.statement_owners.insert(1, index.statement_owners[0]);
                    }
                    M::MissingStatementOwner => {
                        index.statement_owners.remove(0);
                    }
                    M::StatementFileOutsideProfile => {
                        index.statement_owners[0].0.file_ordinal =
                            LibraryFileOrdinal::new(usize::MAX);
                    }
                    M::ReorderedStatementOwner => index.statement_owners.swap(0, 1),
                    M::NonStatementBaselineCountNonzero => {
                        index.baseline_records[0].record_count = 1;
                    }
                    M::NonStatementBaselineDigestNoncanonical => {
                        index.baseline_records[0].digest[0] ^= 1;
                    }
                    M::DuplicateBaseline => index
                        .baseline_records
                        .insert(1, index.baseline_records[0].clone()),
                    M::MissingBaseline => {
                        index.baseline_records.remove(0);
                    }
                    M::SelfConsistentMissingReverseEdge => {
                        let owner_scc = index
                            .scc_membership
                            .iter()
                            .enumerate()
                            .flat_map(|(ordinal, component)| {
                                component
                                    .owners
                                    .iter()
                                    .copied()
                                    .map(move |owner| (owner, ordinal))
                            })
                            .collect::<rustc_hash::FxHashMap<_, _>>();
                        let position = index
                            .reverse_edges
                            .iter()
                            .position(|edge| {
                                owner_scc[&edge.dependency] != owner_scc[&edge.consumer]
                            })
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        index.reverse_edges.remove(position);
                    }
                    M::SelfConsistentMissingRootConsumer => {
                        index.root_slot_consumers.remove(0);
                    }
                    M::SelfConsistentMissingOwnerSite => {
                        let mut counts = rustc_hash::FxHashMap::default();
                        for site in &index.owner_sites {
                            *counts.entry(site.owner).or_insert(0usize) += 1;
                        }
                        let position = index
                            .owner_sites
                            .iter()
                            .position(|site| counts[&site.owner] > 1)
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        index.owner_sites.remove(position);
                    }
                    M::SelfConsistentWrongBaseline => {
                        let baseline = index
                            .baseline_records
                            .iter_mut()
                            .find(|record| matches!(record.owner, ReplayOwner::Statement(_)))
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        baseline.digest[0] ^= 1;
                    }
                    M::SelfConsistentWrongBaselineCount => {
                        let baseline = index
                            .baseline_records
                            .iter_mut()
                            .find(|record| matches!(record.owner, ReplayOwner::Statement(_)))
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        baseline.record_count = baseline.record_count.saturating_add(1);
                    }
                    M::SelfConsistentWrongRootProvenance => {
                        index.root_slots[0].global_object_contributor ^= true;
                    }
                    M::SelfConsistentCrossArtifactSection => {
                        index.root_slots[1].explicit_global_this ^= true;
                    }
                    M::SelfConsistentButUnpinned => {
                        let baseline = index
                            .baseline_records
                            .iter_mut()
                            .filter(|record| matches!(record.owner, ReplayOwner::Statement(_)))
                            .nth(1)
                            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                        baseline.digest[1] ^= 1;
                    }
                    M::MissingSection
                    | M::WrongSectionTag
                    | M::WrongSectionDigest
                    | M::TruncatedSection
                    | M::TrailingSectionBytes
                    | M::WrongManifestDomain
                    | M::UnknownSchema
                    | M::TruncatedInternalLength
                    | M::InternalLengthOverflow
                    | M::InvalidUtf8RootName
                    | M::InvalidOptionalTag
                    | M::InvalidBooleanTag
                    | M::SemanticTrailingBytes
                    | M::InvalidOwnerTag
                    | M::RootNameLengthOverflow
                    | M::InvalidRootConsumerSlot
                    | M::NonzeroUnownedDemands
                    | M::NonzeroInvalidOwnerSites
                    | M::NonzeroNoncanonicalEdges
                    | M::NonzeroTypedReferenceMisses => unreachable!(),
                }
                let payload = index
                    .encode()
                    .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
                replace_replay_payload(&mut bytes, replay, &payload, directory_end)?;
                return Ok(bytes);
            }
            M::MissingSection
            | M::WrongSectionTag
            | M::WrongSectionDigest
            | M::TruncatedSection
            | M::TrailingSectionBytes => unreachable!(),
        }
        rehash_section_and_body(&mut bytes, replay, directory_end)?;
        Ok(bytes)
    }

    fn mutate_canonical(
        mutation: SnapshotTestMutation,
    ) -> Result<Vec<u8>, LibrarySnapshotViolation> {
        let original = packaged_canonical_snapshot().bytes();
        let sections = parse_directory(original)?;
        let directory_end = FIXED_HEADER_LEN + SECTION_COUNT * DIRECTORY_ENTRY_LEN;
        let mut bytes = original.to_vec();

        match mutation {
            SnapshotTestMutation::BadMagic => bytes[0] ^= 0xff,
            SnapshotTestMutation::UnknownVersion => {
                write_u32(&mut bytes, VERSION_OFFSET, u32::MAX)?;
            }
            SnapshotTestMutation::WrongProfileIdentity => bytes[VERSION_OFFSET + 4] ^= 1,
            SnapshotTestMutation::WrongSchemaIdentity => {
                bytes[VERSION_OFFSET + 4 + PROFILE_DIGEST_LEN] ^= 1;
            }
            SnapshotTestMutation::WrongBodyDigest => bytes[BODY_DIGEST_OFFSET] ^= 1,
            SnapshotTestMutation::WrongSectionDigest => {
                bytes[sections[0].directory_offset + 20] ^= 1;
            }
            SnapshotTestMutation::UnknownSectionTag => {
                write_u16(&mut bytes, sections[0].directory_offset, u16::MAX)?;
            }
            SnapshotTestMutation::DuplicateSectionTag => {
                write_u16(&mut bytes, sections[1].directory_offset, sections[0].tag)?;
            }
            SnapshotTestMutation::ReorderedSections => {
                let first = sections[0].directory_offset;
                let second = sections[1].directory_offset;
                for offset in 0..DIRECTORY_ENTRY_LEN {
                    bytes.swap(first + offset, second + offset);
                }
            }
            SnapshotTestMutation::NonZeroReservedDirectoryField => {
                write_u16(&mut bytes, sections[0].directory_offset + 2, 1)?;
            }
            SnapshotTestMutation::OverlappingSection => {
                write_u64(
                    &mut bytes,
                    sections[1].directory_offset + 4,
                    u64::try_from(sections[0].payload_offset)
                        .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?,
                )?;
            }
            SnapshotTestMutation::GappedSection => {
                write_u64(
                    &mut bytes,
                    sections[1].directory_offset + 4,
                    u64::try_from(sections[1].payload_offset + 1)
                        .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?,
                )?;
            }
            SnapshotTestMutation::LengthOverflow => {
                write_u64(&mut bytes, sections[0].directory_offset + 12, u64::MAX)?;
            }
            SnapshotTestMutation::TruncatedPayload => {
                bytes.pop();
            }
            SnapshotTestMutation::TrailingBytes => bytes.push(0),
            SnapshotTestMutation::NextIdMismatch => {
                let section = find_section(&sections, 10)?;
                let types_offset = section.payload_offset + 8;
                let types = read_u64(&bytes, types_offset)?;
                write_u64(
                    &mut bytes,
                    types_offset,
                    types
                        .checked_add(1)
                        .ok_or(LibrarySnapshotViolation::InvalidEncoding)?,
                )?;
                rehash_section_and_body(&mut bytes, section, directory_end)?;
            }
            SnapshotTestMutation::NextTypeParamIdMismatch => {
                let section = find_section(&sections, 10)?;
                let type_params_offset = section.payload_offset + 16;
                let type_params = read_u64(&bytes, type_params_offset)?;
                write_u64(
                    &mut bytes,
                    type_params_offset,
                    type_params
                        .checked_add(1)
                        .ok_or(LibrarySnapshotViolation::InvalidEncoding)?,
                )?;
                rehash_section_and_body(&mut bytes, section, directory_end)?;
            }
            SnapshotTestMutation::NextClassIdMismatch => {
                let section = find_section(&sections, 10)?;
                let classes_offset = section.payload_offset + 24;
                let classes = read_u64(&bytes, classes_offset)?;
                write_u64(
                    &mut bytes,
                    classes_offset,
                    classes
                        .checked_add(1)
                        .ok_or(LibrarySnapshotViolation::InvalidEncoding)?,
                )?;
                rehash_section_and_body(&mut bytes, section, directory_end)?;
            }
            SnapshotTestMutation::InternerBucketMismatch => {
                let section = find_section(&sections, 2)?;
                let first_hash_offset = section.payload_offset + 12;
                bytes[first_hash_offset] ^= 1;
                rehash_section_and_body(&mut bytes, section, directory_end)?;
            }
            SnapshotTestMutation::RootIndexBinderMismatch => {
                let section = find_section(&sections, 9)?;
                let mut cursor = section.payload_offset + 12;
                let name_len = usize::try_from(read_u32(&bytes, cursor)?)
                    .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?;
                cursor = cursor
                    .checked_add(4 + name_len + 1)
                    .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
                let id_offset = (0..4)
                    .map(|index| cursor + index * 4)
                    .find(|offset| read_u32(&bytes, *offset).is_ok_and(|id| id != u32::MAX))
                    .ok_or(LibrarySnapshotViolation::InvalidReference)?;
                write_u32(&mut bytes, id_offset, u32::MAX - 1)?;
                rehash_section_and_body(&mut bytes, section, directory_end)?;
            }
            SnapshotTestMutation::NonTerminalPublication => {
                let section = find_section(&sections, 8)?;
                let (_, references) = parse_references(&bytes, section)?;
                let identities_present = references
                    .last()
                    .map_or(section.payload_offset + 20 + 9 * 12, |reference| {
                        reference.offset + REFERENCE_ENTRY_LEN
                    });
                if bytes.get(identities_present) != Some(&1) {
                    return Err(LibrarySnapshotViolation::InvalidEncoding);
                }
                let terminal = identities_present + 1;
                *bytes
                    .get_mut(terminal)
                    .ok_or(LibrarySnapshotViolation::InvalidEncoding)? = 0;
                rehash_section_and_body(&mut bytes, section, directory_end)?;
            }
            SnapshotTestMutation::DanglingReference { family, endpoint } => {
                let section = find_section(&sections, 8)?;
                let (_, references) = parse_references(&bytes, section)?;
                let family = u8::try_from(family + 1)
                    .map_err(|_| LibrarySnapshotViolation::InvalidReference)?;
                let mut matching = references
                    .iter()
                    .filter(|reference| reference.owner_family == family);
                let reference = match endpoint {
                    ReferenceEndpoint::First => matching.next(),
                    ReferenceEndpoint::Last => matching.next_back(),
                }
                .ok_or(LibrarySnapshotViolation::InvalidReference)?;
                write_u32(&mut bytes, reference.offset + 8, u32::MAX - 1)?;
                rehash_section_and_body(&mut bytes, section, directory_end)?;
            }
            SnapshotTestMutation::InvalidReferenceOwner
            | SnapshotTestMutation::InvalidReferenceDomain
            | SnapshotTestMutation::InvalidReferenceField => {
                let section = find_section(&sections, 8)?;
                let (_, references) = parse_references(&bytes, section)?;
                let reference = references
                    .first()
                    .ok_or(LibrarySnapshotViolation::InvalidReference)?;
                let field_offset = match mutation {
                    SnapshotTestMutation::InvalidReferenceOwner => 0,
                    SnapshotTestMutation::InvalidReferenceDomain => 2,
                    SnapshotTestMutation::InvalidReferenceField => 3,
                    _ => return Err(LibrarySnapshotViolation::InvalidEncoding),
                };
                bytes[reference.offset + field_offset] = u8::MAX;
                rehash_section_and_body(&mut bytes, section, directory_end)?;
            }
        }

        Ok(bytes)
    }

    #[derive(Clone, Copy)]
    struct Reference {
        offset: usize,
        owner_family: u8,
    }

    fn parse_directory(bytes: &[u8]) -> Result<[Section; SECTION_COUNT], LibrarySnapshotViolation> {
        if bytes.len() < FIXED_HEADER_LEN + SECTION_COUNT * DIRECTORY_ENTRY_LEN
            || !bytes.starts_with(MAGIC)
        {
            return Err(LibrarySnapshotViolation::MalformedHeader);
        }
        let mut sections = Vec::with_capacity(SECTION_COUNT);
        for index in 0..SECTION_COUNT {
            let directory_offset = FIXED_HEADER_LEN + index * DIRECTORY_ENTRY_LEN;
            let payload_offset = usize::try_from(read_u64(bytes, directory_offset + 4)?)
                .map_err(|_| LibrarySnapshotViolation::MalformedDirectory)?;
            let payload_len = usize::try_from(read_u64(bytes, directory_offset + 12)?)
                .map_err(|_| LibrarySnapshotViolation::MalformedDirectory)?;
            let end = payload_offset
                .checked_add(payload_len)
                .ok_or(LibrarySnapshotViolation::MalformedDirectory)?;
            if end > bytes.len() {
                return Err(LibrarySnapshotViolation::MalformedDirectory);
            }
            sections.push(Section {
                tag: read_u16(bytes, directory_offset)?,
                directory_offset,
                payload_offset,
                payload_len,
            });
        }
        sections
            .try_into()
            .map_err(|_| LibrarySnapshotViolation::MalformedDirectory)
    }

    fn find_section(
        sections: &[Section; SECTION_COUNT],
        tag: u16,
    ) -> Result<Section, LibrarySnapshotViolation> {
        sections
            .iter()
            .copied()
            .find(|section| section.tag == tag)
            .ok_or(LibrarySnapshotViolation::MalformedDirectory)
    }

    fn parse_references(
        bytes: &[u8],
        section: Section,
    ) -> Result<([u64; REFERENCE_FAMILY_COUNT], Vec<Reference>), LibrarySnapshotViolation> {
        let mut cursor = section.payload_offset;
        if read_u32(bytes, cursor)? != 1 {
            return Err(LibrarySnapshotViolation::InvalidReference);
        }
        cursor += 4;
        if read_u64(bytes, cursor)? != REFERENCE_FAMILY_COUNT as u64 {
            return Err(LibrarySnapshotViolation::InvalidReference);
        }
        cursor += 8;
        let count = usize::try_from(read_u64(bytes, cursor)?)
            .map_err(|_| LibrarySnapshotViolation::InvalidReference)?;
        cursor += 8;
        let mut family_counts = [0; REFERENCE_FAMILY_COUNT];
        for family_count in &mut family_counts {
            *family_count = read_u64(bytes, cursor + 4)?;
            cursor += REFERENCE_FAMILY_ENTRY_LEN;
        }
        let end = cursor
            .checked_add(
                count
                    .checked_mul(REFERENCE_ENTRY_LEN)
                    .ok_or(LibrarySnapshotViolation::InvalidReference)?,
            )
            .ok_or(LibrarySnapshotViolation::InvalidReference)?;
        if end > section.payload_offset + section.payload_len || end > bytes.len() {
            return Err(LibrarySnapshotViolation::InvalidReference);
        }
        let mut references = Vec::with_capacity(count);
        while cursor < end {
            references.push(Reference {
                offset: cursor,
                owner_family: bytes[cursor],
            });
            cursor += REFERENCE_ENTRY_LEN;
        }
        Ok((family_counts, references))
    }

    fn rehash_section_and_body(
        bytes: &mut [u8],
        section: Section,
        directory_end: usize,
    ) -> Result<(), LibrarySnapshotViolation> {
        let payload_end = section
            .payload_offset
            .checked_add(section.payload_len)
            .ok_or(LibrarySnapshotViolation::InvalidPayload)?;
        let payload = bytes
            .get(section.payload_offset..payload_end)
            .ok_or(LibrarySnapshotViolation::InvalidPayload)?;
        let digest = Sha256::digest(payload);
        bytes
            .get_mut(section.directory_offset + 20..section.directory_offset + 52)
            .ok_or(LibrarySnapshotViolation::MalformedDirectory)?
            .copy_from_slice(&digest);
        let body = bytes
            .get(directory_end..)
            .ok_or(LibrarySnapshotViolation::InvalidPayload)?;
        let digest = Sha256::digest(body);
        bytes
            .get_mut(BODY_DIGEST_OFFSET..BODY_DIGEST_OFFSET + BODY_DIGEST_LEN)
            .ok_or(LibrarySnapshotViolation::MalformedHeader)?
            .copy_from_slice(&digest);
        Ok(())
    }

    fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, LibrarySnapshotViolation> {
        let raw = bytes
            .get(offset..offset + 2)
            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
        Ok(u16::from_be_bytes(
            raw.try_into()
                .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?,
        ))
    }

    fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, LibrarySnapshotViolation> {
        let raw = bytes
            .get(offset..offset + 4)
            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
        Ok(u32::from_be_bytes(
            raw.try_into()
                .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?,
        ))
    }

    fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, LibrarySnapshotViolation> {
        let raw = bytes
            .get(offset..offset + 8)
            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?;
        Ok(u64::from_be_bytes(
            raw.try_into()
                .map_err(|_| LibrarySnapshotViolation::InvalidEncoding)?,
        ))
    }

    fn write_u16(
        bytes: &mut [u8],
        offset: usize,
        value: u16,
    ) -> Result<(), LibrarySnapshotViolation> {
        bytes
            .get_mut(offset..offset + 2)
            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?
            .copy_from_slice(&value.to_be_bytes());
        Ok(())
    }

    fn write_u32(
        bytes: &mut [u8],
        offset: usize,
        value: u32,
    ) -> Result<(), LibrarySnapshotViolation> {
        bytes
            .get_mut(offset..offset + 4)
            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?
            .copy_from_slice(&value.to_be_bytes());
        Ok(())
    }

    fn write_u64(
        bytes: &mut [u8],
        offset: usize,
        value: u64,
    ) -> Result<(), LibrarySnapshotViolation> {
        bytes
            .get_mut(offset..offset + 8)
            .ok_or(LibrarySnapshotViolation::InvalidEncoding)?
            .copy_from_slice(&value.to_be_bytes());
        Ok(())
    }
}

//! Strict admission and decoding of the canonical semantic snapshot.

use super::artifact::{
    packaged_canonical_snapshot, verify_canonical_snapshot, SnapshotVerificationError,
    CANONICAL_SNAPSHOT_BYTES, CANONICAL_SNAPSHOT_SHA256,
};
use super::base::FrozenLibraryIdentity;
use super::provider::{
    LibraryInitCause, LibraryInitError, LibraryInitStage, LibrarySnapshotViolation,
};
use crate::check::checker::library_compiler::OwnedLibraryRuntimeState;
use crate::check::checker::library_snapshot_codec::{
    decode_canonical_library_snapshot, SnapshotError, SnapshotErrorKind, SnapshotErrorStage,
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
    "a78ea0521c7c375669bfdb08f0929a5e4b1d0b0d6928de60fbfe09b222a8bc65";

pub(super) struct DecodedCanonicalLibrary {
    pub(super) runtime: OwnedLibraryRuntimeState,
    pub(super) root_names: BTreeSet<String>,
    pub(super) prefixes: [usize; 9],
    pub(super) typed_validation_sha256: [u8; 32],
    pub(super) identity: FrozenLibraryIdentity,
}

pub(super) fn admit_packaged_canonical() -> Result<Cow<'static, [u8]>, LibraryInitError> {
    #[cfg(test)]
    SNAPSHOT_VALIDATIONS.set(SNAPSHOT_VALIDATIONS.get().saturating_add(1));
    let bytes = packaged_canonical_snapshot().bytes();
    verify_canonical_snapshot(bytes, packaged_canonical_snapshot().binding())
        .map_err(map_admission_error)?;
    Ok(Cow::Borrowed(bytes))
}

pub(super) fn decode_admitted_canonical(
    bytes: Cow<'static, [u8]>,
) -> Result<DecodedCanonicalLibrary, LibraryInitError> {
    #[cfg(test)]
    SNAPSHOT_DECODES.set(SNAPSHOT_DECODES.get().saturating_add(1));
    decode_canonical_library_snapshot(bytes)
        .map(|decoded| DecodedCanonicalLibrary {
            runtime: decoded.runtime,
            root_names: decoded.root_names,
            prefixes: decoded.prefixes,
            typed_validation_sha256: decoded.typed_validation_sha256,
            identity: FrozenLibraryIdentity::canonical(),
        })
        .map_err(map_snapshot_error)
}

#[cfg(test)]
pub(super) fn admit_canonical_for_test(
    bytes: Vec<u8>,
) -> Result<Cow<'static, [u8]>, LibraryInitError> {
    SNAPSHOT_VALIDATIONS.set(SNAPSHOT_VALIDATIONS.get().saturating_add(1));
    verify_canonical_snapshot(&bytes, packaged_canonical_snapshot().binding())
        .map_err(map_admission_error)?;
    Ok(Cow::Owned(bytes))
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
            #[cfg(test)]
            SnapshotErrorStage::UnsupportedStrategy
            | SnapshotErrorStage::Io
            | SnapshotErrorStage::Generation
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
                | LibraryInitStage::DecodeBinder => LibrarySnapshotViolation::InvalidEncoding,
            },
        },
    };
    LibraryInitError::new(stage, cause)
}

#[cfg(test)]
pub(super) fn decode_pre_admitted(
    snapshot: &test_support::PreAdmittedSnapshot,
) -> Result<DecodedCanonicalLibrary, LibraryInitError> {
    SNAPSHOT_DECODES.set(SNAPSHOT_DECODES.get().saturating_add(1));
    crate::check::checker::library_snapshot_codec::decode_pre_admitted_library_snapshot(
        &snapshot.bytes,
    )
    .map(|decoded| DecodedCanonicalLibrary {
        runtime: decoded.runtime,
        root_names: decoded.root_names,
        prefixes: decoded.prefixes,
        typed_validation_sha256: decoded.typed_validation_sha256,
        identity: FrozenLibraryIdentity::canonical(),
    })
    .map_err(map_snapshot_error)
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
    const SECTION_COUNT: usize = 10;
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

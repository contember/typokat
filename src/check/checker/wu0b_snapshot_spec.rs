//! Disabled RED contract for the WU0B semantic-snapshot feasibility prototype.
//!
//! Activate with `#[cfg(test)] mod wu0b_snapshot;` and
//! `#[cfg(test)] mod wu0b_snapshot_spec;` in `checker/mod.rs` after the test-only
//! compiler/archive/decoder seam exists. The spec-only commit deliberately leaves both
//! modules inactive so the default suite remains green; activating this file at old HEAD
//! must fail because none of the imported snapshot API exists.
//!
//! This is not the production `FrozenLibraryBase` design. WU0B may consume one decoded
//! base into one user state to establish completeness, identity and startup feasibility.
//! `Arc` sharing, immutable-prefix/private-delta storage, and pointer identity belong to
//! WU3/WU4. The prototype nevertheless has to serialize the complete AST-free runtime
//! state; a reachable-name subset is forbidden.
//!
//! ## Required archive boundary
//!
//! The archive includes the complete type store and interner identity state; binder
//! scope/symbol/declaration/type-group/namespace tables and indexes; `DeclTypes`;
//! published groups/classes and namespace terminals; owner-free named-function-group
//! symbols plus class application, default, parent, name, `new`, value and alias metadata;
//! semantic root identities; and final ID counters. It excludes source bodies, OXC
//! AST/allocators, declaration drafts,
//! lexical tickets/ledgers, library `fn_scopes`/`fn_decl_ids`/`block_scopes` AST-site
//! indexes, flow/query/relation/evaluator/application caches, phase counters, benchmark
//! data and rendered diagnostics. Fresh user-local AST-site indexes remain necessary.
//!
//! The old WU0D seven-section product is evidence, not a runtime snapshot. It carries
//! source/probe material and omits much of the state above, so the new decoder must reject
//! it rather than treating a projection as a second semantic authority.
//!
//! The WU0B wire header is fixed by the constants below. Directory entries are
//! `(u16 tag, u16 zero, u64 absolute_offset, u64 length, sha256[32])`, in the
//! exact section order asserted by `snapshot_roundtrip_preserves_runtime_projection`;
//! the header also hashes the complete post-directory body. Section 9's root-name
//! index starts with `(u32 version=1, u64 count)` and each record starts with
//! `(u32 name_len, name bytes, u8 slot_mask, four u32 optional IDs)` where
//! `u32::MAX` means absent. This independently specified prefix lets the spec create
//! a digest-valid dangling identity instead of testing only outer checksum failures.
//! Section 8 starts with one canonical all-reference manifest before its semantic-
//! identity payload: `(u32 version=1, u64 family_count=9, u64 ref_count)`, nine
//! `(u16 family_tag, u16 zero, u64 count)` rows, then `ref_count` sorted records
//! `(u8 owner_family, u8 owner_domain, u8 target_domain, u8 field, u32 owner,
//! u32 target)`. It enumerates every typed reference owned by sections 1 through 9,
//! including interner buckets, declaration slots, publications and root identities,
//! and is cross-checked against the decoded owned state. The spec independently
//! corrupts the first and last reference of every family plus its discriminants.
//! Domain and field discriminants are append-only values in `0..=31`; other values
//! are invalid in this schema version.
//!
//! ## Loading strategies
//!
//! The mandatory GO witness is a complete eager decoded base. A second
//! `ImmutableIndexed` strategy measures the only admissible lazy idea: all bytes,
//! references, IDs and section boundaries validate before user checking; archive-assigned
//! IDs never change; immutable sections may materialize later, but no semantic declaration
//! is resolved on demand and no shared table grows. It cannot claim success by decoding
//! only the fast-clean reachable surface.
//!
//! ## Performance evidence
//!
//! `snapshot_fast_clean_probe_once` is an ignored release-process primitive. An external
//! WU0B coordinator runs each strategy in fresh processes, records validation, decode/base
//! construction, user check, wall time, artifact bytes and peak RSS, and compares semantic
//! projections. The selected complete-base strategy needs p95 at or below 120 ms; above
//! that early falsifier, the production 2x claim lacks engineering headroom. The separate
//! 1/2/32 probe is shape evidence labelled `private-decode-per-caller`, not a parallelism
//! or shared-`Arc` claim.

use super::wu0b_profile::load_strict_profile;
use super::wu0b_snapshot::{
    check_source_with_decoded_base_for_test, compile_snapshot_for_test,
    decode_snapshot_bytes_for_test, decode_snapshot_for_test,
    snapshot_decode_strategy_probe_for_test, snapshot_fast_clean_probe_for_test,
    snapshot_regeneration_artifact_for_test, snapshot_scaling_probe_for_test,
    start_decoded_base_route_measure_for_test, start_library_compiler_measure_for_test,
    validate_snapshot_for_test, CompiledSnapshotForTest, DecodedLibraryBaseForTest,
    SnapshotArchiveForTest, SnapshotDecodeStrategy, SnapshotErrorStage,
};
use crate::check::checker::wu0b_library::InjectedLibrarySource;
use crate::diagnostics::DiagnosticCode;
use crate::source::LibraryFileOrdinal;
use crate::span::LineIndex;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
// SHA-256 of the preceding schema identity plus `|function-groups.symbols-v1`.
const SNAPSHOT_SCHEMA_IDENTITY: &str =
    "b7f9c947fd684e45da2ef8f351f9d09c71d1d8330e7f52b7953bb80ef128a311";
const FAST_CLEAN: &str =
    include_str!("../../../tooling/full-lib-bench/workloads/fast-clean/main.ts");
const FAST_ERRORS: &str =
    include_str!("../../../tooling/full-lib-bench/workloads/fast-errors/main.ts");

// WU0B freezes a simple independently parseable wire header. The integration spec
// mutates these bytes itself; the implementation cannot choose convenient corruptions.
const SNAPSHOT_MAGIC: &[u8] = b"typokat-semantic-snapshot";
const VERSION_OFFSET: usize = SNAPSHOT_MAGIC.len();
const PROFILE_DIGEST_LEN: usize = 32;
const SCHEMA_DIGEST_LEN: usize = 32;
const SECTION_COUNT_LEN: usize = 4;
const BODY_LENGTH_LEN: usize = 8;
const BODY_DIGEST_LEN: usize = 32;
const BODY_DIGEST_OFFSET: usize = SNAPSHOT_MAGIC.len()
    + 4
    + PROFILE_DIGEST_LEN
    + SCHEMA_DIGEST_LEN
    + SECTION_COUNT_LEN
    + BODY_LENGTH_LEN;
const FIXED_HEADER_LEN: usize = SNAPSHOT_MAGIC.len()
    + 4
    + PROFILE_DIGEST_LEN
    + SCHEMA_DIGEST_LEN
    + SECTION_COUNT_LEN
    + BODY_LENGTH_LEN
    + BODY_DIGEST_LEN;
const DIRECTORY_ENTRY_LEN: usize = 2 + 2 + 8 + 8 + 32;
const ROOT_NAME_INDEX_TAG: u16 = 9;
const REFERENCE_MANIFEST_TAG: u16 = 8;
const SNAPSHOT_VERSION: u32 = 1;
const SECTION_COUNT: usize = 10;
const REFERENCE_FAMILY_COUNT: usize = 9;
const REFERENCE_FAMILY_ENTRY_LEN: usize = 2 + 2 + 8;
const REFERENCE_MANIFEST_ENTRY_LEN: usize = 1 + 1 + 1 + 1 + 4 + 4;
const MAX_REFERENCE_DISCRIMINANT: u8 = 31;

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(bytes[offset..offset + 2].try_into().expect("u16 bytes"))
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("u32 bytes"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(bytes[offset..offset + 8].try_into().expect("u64 bytes"))
}

fn decode_hex_32(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64);
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("hex digest");
    }
    bytes
}

#[derive(Clone, Copy)]
struct SpecSection {
    tag: u16,
    directory_offset: usize,
    payload_offset: usize,
    payload_len: usize,
}

fn parse_spec_directory(bytes: &[u8]) -> Vec<SpecSection> {
    assert!(bytes.len() >= FIXED_HEADER_LEN);
    assert!(bytes.starts_with(SNAPSHOT_MAGIC));
    assert_eq!(read_u32(bytes, VERSION_OFFSET), SNAPSHOT_VERSION);
    let profile_offset = VERSION_OFFSET + 4;
    assert_eq!(
        &bytes[profile_offset..profile_offset + PROFILE_DIGEST_LEN],
        &decode_hex_32(PROFILE_IDENTITY)
    );
    let schema_offset = profile_offset + PROFILE_DIGEST_LEN;
    assert_eq!(
        &bytes[schema_offset..schema_offset + SCHEMA_DIGEST_LEN],
        &decode_hex_32(SNAPSHOT_SCHEMA_IDENTITY)
    );
    let section_count_offset = VERSION_OFFSET + 4 + PROFILE_DIGEST_LEN + SCHEMA_DIGEST_LEN;
    let section_count = read_u32(bytes, section_count_offset) as usize;
    assert_eq!(section_count, SECTION_COUNT);
    let directory_end = FIXED_HEADER_LEN + section_count * DIRECTORY_ENTRY_LEN;
    assert!(directory_end <= bytes.len());
    let body_length_offset = section_count_offset + SECTION_COUNT_LEN;
    assert_eq!(
        usize::try_from(read_u64(bytes, body_length_offset)).expect("body length fits usize"),
        bytes.len() - directory_end
    );
    assert_eq!(
        &bytes[BODY_DIGEST_OFFSET..BODY_DIGEST_OFFSET + BODY_DIGEST_LEN],
        &Sha256::digest(&bytes[directory_end..])[..]
    );

    let mut expected_payload_offset = directory_end;
    let sections = (0..section_count)
        .map(|index| {
            let directory_offset = FIXED_HEADER_LEN + index * DIRECTORY_ENTRY_LEN;
            assert_eq!(
                read_u16(bytes, directory_offset),
                u16::try_from(index).expect("section index fits u16") + 1
            );
            assert_eq!(read_u16(bytes, directory_offset + 2), 0);
            let payload_offset = usize::try_from(read_u64(bytes, directory_offset + 4))
                .expect("section offset fits usize");
            let payload_len = usize::try_from(read_u64(bytes, directory_offset + 12))
                .expect("section length fits usize");
            assert_eq!(payload_offset, expected_payload_offset);
            expected_payload_offset += payload_len;
            assert!(expected_payload_offset <= bytes.len());
            assert_eq!(
                &bytes[directory_offset + 20..directory_offset + 52],
                &Sha256::digest(&bytes[payload_offset..payload_offset + payload_len])[..]
            );
            SpecSection {
                tag: read_u16(bytes, directory_offset),
                directory_offset,
                payload_offset,
                payload_len,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(expected_payload_offset, bytes.len());
    sections
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
}

fn rehash_section_and_body(bytes: &mut [u8], section: SpecSection, directory_end: usize) {
    let section_digest = Sha256::digest(
        &bytes[section.payload_offset..section.payload_offset + section.payload_len],
    );
    bytes[section.directory_offset + 20..section.directory_offset + 52]
        .copy_from_slice(&section_digest);
    let body_digest = Sha256::digest(&bytes[directory_end..]);
    bytes[BODY_DIGEST_OFFSET..BODY_DIGEST_OFFSET + BODY_DIGEST_LEN].copy_from_slice(&body_digest);
}

fn corrupt_root_index_reference_with_valid_digests(
    original: &[u8],
    sections: &[SpecSection],
) -> Vec<u8> {
    let section = sections
        .iter()
        .copied()
        .find(|section| section.tag == ROOT_NAME_INDEX_TAG)
        .expect("root-name-index section");
    let mut bytes = original.to_vec();
    let mut cursor = section.payload_offset;
    assert_eq!(read_u32(&bytes, cursor), 1, "root-index section version");
    cursor += 4;
    let count = read_u64(&bytes, cursor);
    cursor += 8;
    assert!(count > 0);
    let name_len = read_u32(&bytes, cursor) as usize;
    cursor += 4 + name_len;
    cursor += 1; // slot mask
    let id_offsets = [cursor, cursor + 4, cursor + 8, cursor + 12];
    let id_offset = id_offsets
        .into_iter()
        .find(|&offset| read_u32(&bytes, offset) != u32::MAX)
        .expect("first root-index record has one concrete identity");
    bytes[id_offset..id_offset + 4].copy_from_slice(&(u32::MAX - 1).to_be_bytes());
    let directory_end = FIXED_HEADER_LEN + sections.len() * DIRECTORY_ENTRY_LEN;
    rehash_section_and_body(&mut bytes, section, directory_end);
    bytes
}

#[derive(Clone, Copy)]
struct SpecReference {
    offset: usize,
    owner_family: u8,
}

fn parse_reference_manifest(
    bytes: &[u8],
    sections: &[SpecSection],
) -> (
    SpecSection,
    [u64; REFERENCE_FAMILY_COUNT],
    Vec<SpecReference>,
    [u8; 32],
) {
    let section = sections
        .iter()
        .copied()
        .find(|section| section.tag == REFERENCE_MANIFEST_TAG)
        .expect("reference-manifest section");
    let mut cursor = section.payload_offset;
    assert_eq!(read_u32(bytes, cursor), 1, "reference manifest version");
    cursor += 4;
    assert_eq!(
        read_u64(bytes, cursor),
        u64::try_from(REFERENCE_FAMILY_COUNT).expect("reference family count fits u64"),
        "reference family count"
    );
    cursor += 8;
    let reference_count = read_u64(bytes, cursor);
    cursor += 8;
    let mut family_counts = [0; REFERENCE_FAMILY_COUNT];
    for (index, count) in family_counts.iter_mut().enumerate() {
        assert_eq!(
            read_u16(bytes, cursor),
            u16::try_from(index).expect("reference family index fits u16") + 1
        );
        assert_eq!(read_u16(bytes, cursor + 2), 0);
        *count = read_u64(bytes, cursor + 4);
        assert!(*count > 0, "reference family {} is empty", index + 1);
        cursor += REFERENCE_FAMILY_ENTRY_LEN;
    }
    assert_eq!(family_counts.iter().sum::<u64>(), reference_count);
    assert!(
        cursor
            + usize::try_from(reference_count).expect("reference count fits usize")
                * REFERENCE_MANIFEST_ENTRY_LEN
            <= section.payload_offset + section.payload_len
    );
    let mut previous = None;
    let mut actual_counts = [0; REFERENCE_FAMILY_COUNT];
    let references = (0..reference_count)
        .map(|_| {
            let owner_family = bytes[cursor];
            let owner_domain = bytes[cursor + 1];
            let target_domain = bytes[cursor + 2];
            let field = bytes[cursor + 3];
            let owner = read_u32(bytes, cursor + 4);
            let target = read_u32(bytes, cursor + 8);
            assert!((1..=u8::try_from(REFERENCE_FAMILY_COUNT)
                .expect("reference family count fits u8"))
                .contains(&owner_family));
            actual_counts[usize::from(owner_family - 1)] += 1;
            assert!(owner_domain <= MAX_REFERENCE_DISCRIMINANT);
            assert!(target_domain <= MAX_REFERENCE_DISCRIMINANT);
            assert!(field <= MAX_REFERENCE_DISCRIMINANT);
            let key = (
                owner_family,
                owner_domain,
                target_domain,
                field,
                owner,
                target,
            );
            assert!(previous.is_none_or(|previous| previous <= key));
            previous = Some(key);
            let reference = SpecReference {
                offset: cursor,
                owner_family,
            };
            cursor += REFERENCE_MANIFEST_ENTRY_LEN;
            reference
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_counts, family_counts);
    let manifest_sha256 = Sha256::digest(&bytes[section.payload_offset..cursor]).into();
    (section, family_counts, references, manifest_sha256)
}

#[derive(Clone, Copy)]
enum ReferenceCorruption {
    OwnerFamily,
    OwnerDomain,
    TargetDomain,
    Field,
    Target,
}

fn corrupt_manifest_reference_with_valid_digests(
    original: &[u8],
    sections: &[SpecSection],
    reference: SpecReference,
    corruption: ReferenceCorruption,
) -> Vec<u8> {
    let (section, _, _, _) = parse_reference_manifest(original, sections);
    let mut bytes = original.to_vec();
    match corruption {
        ReferenceCorruption::OwnerFamily => bytes[reference.offset] = u8::MAX,
        ReferenceCorruption::OwnerDomain => bytes[reference.offset + 1] = u8::MAX,
        ReferenceCorruption::TargetDomain => bytes[reference.offset + 2] = u8::MAX,
        ReferenceCorruption::Field => bytes[reference.offset + 3] = u8::MAX,
        ReferenceCorruption::Target => {
            write_u32(&mut bytes, reference.offset + 8, u32::MAX - 1);
        }
    }
    let directory_end = FIXED_HEADER_LEN + sections.len() * DIRECTORY_ENTRY_LEN;
    rehash_section_and_body(&mut bytes, section, directory_end);
    bytes
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

fn assert_all_decoders_reject(label: &str, bytes: &[u8]) {
    for strategy in [
        SnapshotDecodeStrategy::EagerComplete,
        SnapshotDecodeStrategy::ImmutableIndexed,
    ] {
        decode_snapshot_bytes_for_test(bytes, strategy).expect_err(&format!(
            "{label} must fail before a {strategy:?} base exists"
        ));
    }
}

fn compile_exact_profile() -> &'static CompiledSnapshotForTest {
    static COMPILED: OnceLock<CompiledSnapshotForTest> = OnceLock::new();
    COMPILED.get_or_init(|| {
        let profile = load_strict_profile().expect("exact TS 6.0.3 profile");
        let sources = profile.injected_sources();
        assert_eq!(sources.len(), 82);
        compile_snapshot_for_test(&sources).expect("complete semantic snapshot compiles")
    })
}

fn decode_exact_profile(strategy: SnapshotDecodeStrategy) -> DecodedLibraryBaseForTest {
    let compiled = compile_exact_profile();
    let validated =
        validate_snapshot_for_test(compiled.archive().as_bytes()).expect("snapshot validates");
    decode_snapshot_for_test(validated, strategy).expect("snapshot decodes")
}

#[test]
fn snapshot_fast_clean_uses_decoded_full_base() {
    let base = decode_exact_profile(SnapshotDecodeStrategy::EagerComplete);
    assert_eq!(base.profile_identity(), PROFILE_IDENTITY);
    assert_eq!(base.source_file_count(), 82);

    let required = BTreeSet::from([
        "Generator",
        "HTMLDivElement",
        "Intl",
        "Iterator",
        "Promise",
        "document",
    ]);
    assert_eq!(base.present_semantic_roots(&required), required);

    let source_projection = base.runtime_projection().clone();
    let compiler_measure = start_library_compiler_measure_for_test();
    let result = check_source_with_decoded_base_for_test(base, FAST_CLEAN);
    let compiler_measure = compiler_measure.finish();
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
    assert_eq!(result.base_projection_after_user_check, source_projection);
    assert_eq!(compiler_measure.source_loads, 0);
    assert_eq!(compiler_measure.parse_units, 0);
    assert_eq!(compiler_measure.bind_units, 0);
    assert_eq!(compiler_measure.semantic_units, 0);
    assert_eq!(compiler_measure.snapshot_generations, 0);
}

#[test]
fn snapshot_fast_errors_exercises_real_decoded_semantics() {
    let base = decode_exact_profile(SnapshotDecodeStrategy::EagerComplete);
    let result = check_source_with_decoded_base_for_test(base, FAST_ERRORS);
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);

    let lines = LineIndex::new(FAST_ERRORS);
    let actual = result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let position = lines.line_col(diagnostic.span.start);
            (position.line, position.column, diagnostic.code)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        [4, 5, 6, 7, 8, 9].map(|line| (line, 7, DiagnosticCode::TK2322))
    );
}

#[test]
fn snapshot_roundtrip_preserves_runtime_projection() {
    let compiled = compile_exact_profile();
    let source_projection = compiled.runtime_projection().clone();
    let sections = parse_spec_directory(compiled.archive().as_bytes());
    let (_, reference_counts, references, reference_manifest_sha256) =
        parse_reference_manifest(compiled.archive().as_bytes(), &sections);
    let validated =
        validate_snapshot_for_test(compiled.archive().as_bytes()).expect("snapshot validates");
    let decoded = decode_snapshot_for_test(validated, SnapshotDecodeStrategy::EagerComplete)
        .expect("snapshot decodes");

    assert_eq!(decoded.runtime_projection(), &source_projection);
    assert_eq!(source_projection.reference_counts(), reference_counts);
    assert_eq!(decoded.reference_counts(), reference_counts);
    assert_eq!(
        source_projection.reference_manifest_sha256(),
        reference_manifest_sha256
    );
    assert_eq!(
        decoded.reference_manifest_sha256(),
        reference_manifest_sha256
    );
    assert_eq!(
        references.len(),
        usize::try_from(reference_counts.iter().sum::<u64>()).expect("reference count fits usize")
    );
    assert_eq!(decoded.runtime_counts(), source_projection.runtime_counts());
    assert_eq!(
        decoded.section_inventory(),
        [
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
        ]
    );

    let source_names = source_projection.compilation_global_names();
    assert_eq!(decoded.root_name_index_names(), source_names);
    assert_eq!(
        decoded.root_name_index_counts(),
        source_projection.root_name_index_counts()
    );

    assert_eq!(
        source_projection.subtable_names(),
        [
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
        ]
    );
    assert_eq!(
        decoded.runtime_projection().subtables(),
        source_projection.subtables()
    );
    for subtable in source_projection.subtables() {
        assert_ne!(
            subtable.sha256, [0; 32],
            "unhashed runtime subtable: {}",
            subtable.name
        );
    }
    for name in [
        "store.rows",
        "interner.dedup-buckets",
        "interner.reserved-terminals",
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
        "class.application-parameters",
        "semantic-identities",
        "root-name-index.entries",
    ] {
        assert!(
            source_projection
                .subtable(name)
                .expect("required subtable")
                .row_count
                > 0,
            "required runtime subtable is empty: {name}"
        );
    }

    let next = source_projection.next_ids();
    assert_eq!(decoded.prefix_lengths().store, next.types);
    assert_eq!(decoded.prefix_lengths().type_params, next.type_params);
    assert_eq!(decoded.prefix_lengths().classes, next.classes);
    assert_eq!(decoded.prefix_lengths().scopes, next.scopes);
    assert_eq!(decoded.prefix_lengths().symbols, next.symbols);
    assert_eq!(decoded.prefix_lengths().declarations, next.declarations);
    assert_eq!(decoded.prefix_lengths().type_groups, next.type_groups);
    assert_eq!(decoded.prefix_lengths().namespaces, next.namespaces);
    assert_eq!(decoded.prefix_lengths().value_storages, next.value_storages);

    let error = validate_snapshot_for_test(b"typokat-wu0d-frozen-library-product-v2")
        .expect_err("WU0D evidence projection is not a runtime snapshot");
    assert_eq!(error.stage(), SnapshotErrorStage::HeaderValidation);
}

#[test]
fn snapshot_preserves_nominal_and_structural_identity() {
    let compiled = compile_exact_profile();
    let source = compiled.identity_witness().clone();
    let validated =
        validate_snapshot_for_test(compiled.archive().as_bytes()).expect("snapshot validates");
    let decoded = decode_snapshot_for_test(validated, SnapshotDecodeStrategy::EagerComplete)
        .expect("snapshot decodes");

    assert_eq!(decoded.identity_witness(), &source);
    assert_ne!(
        source.class_id("SafeArray").expect("SafeArray class"),
        source.class_id("VarDate").expect("VarDate class")
    );
    let iterator_group = source
        .type_group_id("Iterator")
        .expect("Iterator interface type group");
    assert_eq!(
        decoded.identity_witness().type_group_id("Iterator"),
        Some(iterator_group)
    );

    let result = check_source_with_decoded_base_for_test(
        decoded,
        r#"
            export type FirstSafe = SafeArray<number>;
            export type SecondSafe = SafeArray<number>;
            export type FirstShape = { value: number; run(input: string): boolean };
            export type SecondShape = { value: number; run(input: string): boolean };
            export declare const safe: FirstSafe;
            export declare const shape: FirstShape;
        "#,
    );
    assert!(result.parse_errors.is_empty());
    assert!(result.diagnostics.is_empty());
    assert!(result.incomplete.is_empty());
    assert_eq!(
        result.user_type_id("FirstSafe"),
        result.user_type_id("SecondSafe")
    );
    assert_eq!(
        result.user_type_id("FirstShape"),
        result.user_type_id("SecondShape")
    );
    assert_eq!(
        result.observed_base_class_id("SafeArray"),
        source.class_id("SafeArray")
    );
    assert_eq!(
        result.observed_base_class_id("VarDate"),
        source.class_id("VarDate")
    );

    let nominal_source = concat!(
        "declare const safe: SafeArray<number>;\n",
        "const same: SafeArray<number> = safe;\n",
        "declare const date: VarDate;\n",
        "const wrongDate: SafeArray<number> = date;\n",
        "declare const structural: {};\n",
        "const wrongShape: SafeArray<number> = structural;\n",
    );
    let nominal = check_source_with_decoded_base_for_test(
        decode_exact_profile(SnapshotDecodeStrategy::EagerComplete),
        nominal_source,
    );
    assert!(
        nominal.parse_errors.is_empty(),
        "{:?}",
        nominal.parse_errors
    );
    assert!(nominal.incomplete.is_empty(), "{:?}", nominal.incomplete);
    let lines = LineIndex::new(nominal_source);
    assert_eq!(
        nominal
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let position = lines.line_col(diagnostic.span.start);
                (position.line, position.column, diagnostic.code)
            })
            .collect::<Vec<_>>(),
        [
            (4, 7, DiagnosticCode::TK2322),
            (6, 7, DiagnosticCode::TK2322),
        ]
    );

    let named_function_library = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "snapshot-named-function.d.ts",
        source: concat!(
            "declare function SnapshotNamedFunction(value: number): number;\n",
            "declare namespace SnapshotNamedFunction { const identity: string; }\n",
        ),
    }];
    let named_function_snapshot = compile_snapshot_for_test(&named_function_library)
        .expect("named function metadata compiles into the snapshot");
    let validated = validate_snapshot_for_test(named_function_snapshot.archive().as_bytes())
        .expect("named function snapshot validates");
    let decoded = decode_snapshot_for_test(validated, SnapshotDecodeStrategy::EagerComplete)
        .expect("named function snapshot decodes");
    let named_function = check_source_with_decoded_base_for_test(
        decoded,
        "SnapshotNamedFunction.notAFunctionMember;\n",
    );
    assert!(
        named_function.parse_errors.is_empty(),
        "{:?}",
        named_function.parse_errors
    );
    assert!(
        named_function.incomplete.is_empty(),
        "{:?}",
        named_function.incomplete
    );
    assert_eq!(named_function.diagnostics.len(), 1);
    assert_eq!(named_function.diagnostics[0].code, DiagnosticCode::TK2339);
    assert_eq!(
        named_function.diagnostics[0].message,
        "Property 'notAFunctionMember' does not exist on type 'typeof SnapshotNamedFunction'"
    );
}

#[test]
fn snapshot_user_ids_are_suffixes() {
    let base = decode_exact_profile(SnapshotDecodeStrategy::EagerComplete);
    let prefixes = base.prefix_lengths().clone();
    let projection = base.runtime_projection().clone();
    let result = check_source_with_decoded_base_for_test(
        base,
        r#"
            export declare namespace LocalSpace {
                export interface LocalInterface<T> { value: T }
                export class LocalClass<U> {
                    value: U;
                }
                export const localValue: LocalClass<number>;
            }
            export type LocalDiv = HTMLDivElement;
        "#,
    );

    assert!(result.parse_errors.is_empty());
    assert!(result.diagnostics.is_empty());
    assert!(result.incomplete.is_empty());
    for (prefix, range) in [
        (prefixes.store, result.user_identity_ranges.store),
        (
            prefixes.type_params,
            result.user_identity_ranges.type_params,
        ),
        (prefixes.classes, result.user_identity_ranges.classes),
        (prefixes.scopes, result.user_identity_ranges.scopes),
        (prefixes.symbols, result.user_identity_ranges.symbols),
        (
            prefixes.declarations,
            result.user_identity_ranges.declarations,
        ),
        (
            prefixes.type_groups,
            result.user_identity_ranges.type_groups,
        ),
        (prefixes.namespaces, result.user_identity_ranges.namespaces),
        (
            prefixes.value_storages,
            result.user_identity_ranges.value_storages,
        ),
    ] {
        assert_eq!(range.start, prefix);
        assert!(range.end > range.start);
    }
    let reused = result
        .reused_base_shape
        .expect("user lowering must reuse one existing base shape");
    assert!(reused.index() < prefixes.store);
    assert_eq!(result.base_projection_after_user_check, projection);
}

#[test]
fn semantically_relevant_library_mutation_changes_snapshot_identity() {
    let profile = load_strict_profile().expect("exact TS 6.0.3 profile");
    let original = profile.injected_sources();
    let mutation = format!(
        "{}\ninterface TypokatSnapshotMutation {{ value: string }}\n",
        original[0].source
    );
    let mutated = original
        .iter()
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.file_ordinal,
            name: source.name,
            source: if source.file_ordinal == LibraryFileOrdinal::new(0) {
                &mutation
            } else {
                source.source
            },
        })
        .collect::<Vec<_>>();
    let changed =
        compile_snapshot_for_test(&mutated).expect("semantic mutation remains compilable");
    let original = compile_exact_profile();
    assert_ne!(original.archive().sha256(), changed.archive().sha256());
    assert_ne!(original.runtime_projection(), changed.runtime_projection());
    assert!(
        changed.runtime_projection().next_ids().type_groups
            > original.runtime_projection().next_ids().type_groups
    );
    let changed_group = changed
        .identity_witness()
        .type_group_id("TypokatSnapshotMutation")
        .expect("semantic mutation publishes a concrete type group");
    assert_eq!(
        original
            .identity_witness()
            .type_group_id("TypokatSnapshotMutation"),
        None
    );
    let validated = validate_snapshot_for_test(changed.archive().as_bytes())
        .expect("changed snapshot validates");
    let decoded = decode_snapshot_for_test(validated, SnapshotDecodeStrategy::EagerComplete)
        .expect("changed snapshot decodes");
    assert_eq!(
        decoded
            .identity_witness()
            .type_group_id("TypokatSnapshotMutation"),
        Some(changed_group)
    );
    let result = check_source_with_decoded_base_for_test(
        decoded,
        concat!(
            "const mutation: TypokatSnapshotMutation = { value: 'ok' };\n",
            "const wrong: number = mutation.value;\n",
        ),
    );
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, DiagnosticCode::TK2322);
}

#[test]
fn snapshot_corruption_fails_before_user_check() {
    let compiled = compile_exact_profile();
    let original = compiled.archive().as_bytes();
    let sections = parse_spec_directory(original);
    assert!(sections.len() >= 2);
    assert!(sections[0].payload_len > 0);

    let mut cases = Vec::new();

    let mut bad_magic = original.to_vec();
    bad_magic[0] ^= 0xff;
    cases.push(("bad-magic", bad_magic));

    let mut unknown_version = original.to_vec();
    unknown_version[VERSION_OFFSET..VERSION_OFFSET + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    cases.push(("unknown-version", unknown_version));

    let mut wrong_profile = original.to_vec();
    wrong_profile[VERSION_OFFSET + 4] ^= 0x01;
    cases.push(("wrong-profile", wrong_profile));

    let mut wrong_schema = original.to_vec();
    wrong_schema[VERSION_OFFSET + 4 + PROFILE_DIGEST_LEN] ^= 0x01;
    cases.push(("wrong-schema", wrong_schema));

    let mut wrong_body_digest = original.to_vec();
    wrong_body_digest[BODY_DIGEST_OFFSET] ^= 0x01;
    cases.push(("wrong-body-digest", wrong_body_digest));

    let section_count_offset = VERSION_OFFSET + 4 + PROFILE_DIGEST_LEN + SCHEMA_DIGEST_LEN;
    let mut wrong_section_count = original.to_vec();
    write_u32(
        &mut wrong_section_count,
        section_count_offset,
        u32::try_from(SECTION_COUNT).expect("section count fits u32") - 1,
    );
    cases.push(("wrong-section-count", wrong_section_count));

    let body_length_offset = section_count_offset + SECTION_COUNT_LEN;
    let mut wrong_body_length = original.to_vec();
    write_u64(
        &mut wrong_body_length,
        body_length_offset,
        read_u64(original, body_length_offset) + 1,
    );
    cases.push(("wrong-body-length", wrong_body_length));

    let mut bad_tag = original.to_vec();
    write_u16(&mut bad_tag, FIXED_HEADER_LEN, u16::MAX);
    cases.push(("bad-tag", bad_tag));

    let mut nonzero_reserved = original.to_vec();
    write_u16(&mut nonzero_reserved, FIXED_HEADER_LEN + 2, 1);
    cases.push(("nonzero-reserved", nonzero_reserved));

    let mut duplicate_tag = original.to_vec();
    write_u16(
        &mut duplicate_tag,
        sections[1].directory_offset,
        sections[0].tag,
    );
    cases.push(("duplicate-tag", duplicate_tag));

    let mut bad_section_order = original.to_vec();
    let first = sections[0].directory_offset;
    let second = sections[1].directory_offset;
    for offset in 0..DIRECTORY_ENTRY_LEN {
        bad_section_order.swap(first + offset, second + offset);
    }
    cases.push(("bad-section-order", bad_section_order));

    let mut overlapping_sections = original.to_vec();
    write_u64(
        &mut overlapping_sections,
        sections[1].directory_offset + 4,
        u64::try_from(sections[0].payload_offset).expect("payload offset fits u64"),
    );
    cases.push(("overlapping-sections", overlapping_sections));

    let mut gapped_sections = original.to_vec();
    write_u64(
        &mut gapped_sections,
        sections[1].directory_offset + 4,
        u64::try_from(sections[1].payload_offset).expect("payload offset fits u64") + 1,
    );
    cases.push(("gapped-sections", gapped_sections));

    let mut wrong_length = original.to_vec();
    write_u64(
        &mut wrong_length,
        sections[0].directory_offset + 12,
        u64::MAX,
    );
    cases.push(("wrong-length", wrong_length));

    let mut digest_mismatch = original.to_vec();
    digest_mismatch[sections[0].payload_offset] ^= 0x01;
    cases.push(("payload-digest-mismatch", digest_mismatch));

    cases.push((
        "digest-valid-dangling-root-identity",
        corrupt_root_index_reference_with_valid_digests(original, &sections),
    ));
    cases.push(("truncated", original[..original.len() - 1].to_vec()));
    let mut trailing = original.to_vec();
    trailing.push(0);
    cases.push(("trailing-bytes", trailing));

    for (label, bytes) in cases {
        assert_all_decoders_reject(label, &bytes);
    }

    let (_, _, references, _) = parse_reference_manifest(original, &sections);
    for family in 1..=u8::try_from(REFERENCE_FAMILY_COUNT).expect("reference family count fits u8")
    {
        let family_references = references
            .iter()
            .copied()
            .filter(|reference| reference.owner_family == family)
            .collect::<Vec<_>>();
        let first = *family_references.first().expect("first family reference");
        let last = *family_references.last().expect("last family reference");
        for (position, reference) in [("first", first), ("last", last)] {
            let bytes = corrupt_manifest_reference_with_valid_digests(
                original,
                &sections,
                reference,
                ReferenceCorruption::Target,
            );
            assert_all_decoders_reject(
                &format!("digest-valid-dangling-family-{family}-{position}"),
                &bytes,
            );
        }
    }
    let first = references[0];
    for (label, corruption) in [
        ("invalid-owner-family", ReferenceCorruption::OwnerFamily),
        ("invalid-owner-domain", ReferenceCorruption::OwnerDomain),
        ("invalid-target-domain", ReferenceCorruption::TargetDomain),
        ("invalid-field", ReferenceCorruption::Field),
    ] {
        let bytes =
            corrupt_manifest_reference_with_valid_digests(original, &sections, first, corruption);
        assert_all_decoders_reject(&format!("digest-valid-{label}"), &bytes);
    }
}

#[test]
fn snapshot_contains_no_compiler_working_state() {
    let compiled = compile_exact_profile();
    let projection = compiled.runtime_projection();
    assert_eq!(
        projection.family_names(),
        [
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
        ]
    );
    for family in projection.families() {
        assert!(family.byte_len > 0, "empty runtime family: {}", family.name);
        assert_ne!(family.sha256, [0; 32], "unhashed family: {}", family.name);
    }

    let archive = compiled.archive().as_bytes();
    let profile = load_strict_profile().expect("exact TS 6.0.3 profile");
    for input in profile.injected_sources() {
        let source = input.source.as_bytes();
        assert!(source.len() >= 128, "profile source unexpectedly too small");
        let start = source.len() / 2 - 64;
        let fragment = &source[start..start + 128];
        assert!(
            !archive
                .windows(fragment.len())
                .any(|window| window == fragment),
            "archive retained a source-body fragment for {}",
            input.name
        );
    }
}

#[test]
fn library_compiler_counter_is_calibrated_to_real_entrypoints() {
    let source = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "counter-calibration.d.ts",
        source: "interface TypokatCounterCalibration { value: string }",
    }];
    let measure = start_library_compiler_measure_for_test();
    compile_snapshot_for_test(&source).expect("small library snapshot compiles");
    let measure = measure.finish();
    assert_eq!(measure.source_loads, 1);
    assert_eq!(measure.parse_units, 1);
    assert_eq!(measure.bind_units, 1);
    assert!(measure.semantic_units > 0);
    assert_eq!(measure.snapshot_generations, 1);
}

#[test]
fn decoded_base_route_counter_is_calibrated_to_real_user_check() {
    let base = decode_exact_profile(SnapshotDecodeStrategy::EagerComplete);
    let expected_projection_sha256 = base.runtime_projection().sha256();
    let route_measure = start_decoded_base_route_measure_for_test();
    let compiler_measure = start_library_compiler_measure_for_test();
    let result = check_source_with_decoded_base_for_test(base, FAST_CLEAN);
    let compiler_measure = compiler_measure.finish();
    let route_measure = route_measure.finish();
    assert!(result.parse_errors.is_empty(), "{:?}", result.parse_errors);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
    assert_eq!(route_measure.user_checks, 1);
    assert_eq!(
        route_measure.runtime_projection_sha256,
        expected_projection_sha256
    );
    assert_eq!(compiler_measure.source_loads, 0);
    assert_eq!(compiler_measure.parse_units, 0);
    assert_eq!(compiler_measure.bind_units, 0);
    assert_eq!(compiler_measure.semantic_units, 0);
    assert_eq!(compiler_measure.snapshot_generations, 0);
}

#[test]
fn decoded_user_route_has_no_source_compiler_dependency() {
    let runtime_source = include_str!("wu0b_snapshot_runtime.rs");
    let compiler_source = include_str!("wu0b_snapshot.rs");
    assert!(runtime_source.contains("fn check_source_with_decoded_base_for_test("));
    assert!(!compiler_source.contains("fn check_source_with_decoded_base_for_test("));
    for forbidden in [
        "compile_snapshot_for_test",
        "load_strict_profile",
        "run_injected_profile",
        "PRELUDE_SOURCE",
        "bootstrap_trusted_prelude",
        "LibraryCompiler",
    ] {
        assert!(
            !runtime_source.contains(forbidden),
            "decoded user route depends on source compiler symbol {forbidden}"
        );
    }
}

#[test]
fn snapshot_base_is_send_sync_static() {
    assert_send_sync_static::<CompiledSnapshotForTest>();
    assert_send_sync_static::<SnapshotArchiveForTest>();
    assert_send_sync_static::<DecodedLibraryBaseForTest>();
}

#[test]
#[ignore = "WU0B release-process evidence probe; run through the external coordinator"]
fn snapshot_fast_clean_probe_once() {
    let input = PathBuf::from(
        std::env::var_os("TYPOKAT_WU0B_SNAPSHOT_INPUT")
            .expect("coordinator supplies a prebuilt snapshot artifact"),
    );
    let artifact_bytes = usize::try_from(
        std::fs::metadata(&input)
            .expect("prebuilt snapshot metadata")
            .len(),
    )
    .expect("artifact length fits usize");
    let compiler_measure = start_library_compiler_measure_for_test();
    let route_measure = start_decoded_base_route_measure_for_test();
    let record = snapshot_fast_clean_probe_for_test(&input, FAST_CLEAN, FAST_ERRORS)
        .expect("complete snapshot fast-clean probe");
    let route_measure = route_measure.finish();
    let compiler_measure = compiler_measure.finish();
    assert_eq!(record.profile_identity, PROFILE_IDENTITY);
    assert_eq!(record.strategy, SnapshotDecodeStrategy::EagerComplete);
    assert_eq!(record.route, "decoded-base-user-check");
    assert_eq!(record.artifact_bytes, artifact_bytes);
    assert_eq!(record.validated_bytes, artifact_bytes);
    assert_eq!(record.compiler_measure, compiler_measure);
    assert_eq!(compiler_measure.source_loads, 0);
    assert_eq!(compiler_measure.parse_units, 0);
    assert_eq!(compiler_measure.bind_units, 0);
    assert_eq!(compiler_measure.semantic_units, 0);
    assert_eq!(compiler_measure.snapshot_generations, 0);
    assert_eq!(route_measure.user_checks, 2);
    assert_eq!(
        route_measure.runtime_projection_sha256,
        record.runtime_projection_sha256
    );
    assert_eq!(record.runtime_projection_sha256.len(), 64);
    assert_ne!(record.runtime_projection_sha256, "0".repeat(64));
    assert_eq!(record.semantics.fast_clean.exit, 0);
    assert!(record.semantics.fast_clean.diagnostics.is_empty());
    assert_eq!(record.semantics.fast_errors.exit, 1);
    assert_eq!(
        record.semantics.fast_errors.diagnostics,
        [
            "fast-errors/main.ts:4:7:2322",
            "fast-errors/main.ts:5:7:2322",
            "fast-errors/main.ts:6:7:2322",
            "fast-errors/main.ts:7:7:2322",
            "fast-errors/main.ts:8:7:2322",
            "fast-errors/main.ts:9:7:2322",
        ]
    );
    assert!(record.validation_us > 0);
    assert!(record.decode_us > 0);
    assert!(record.user_check_us > 0);
    assert!(record.wall_us >= record.validation_us + record.decode_us + record.user_check_us);
    assert!(record.peak_rss_bytes > 0);
    println!("{}", record.render());
}

#[test]
#[ignore = "optional immutable-indexed experiment; eager complete GO must exist first"]
fn snapshot_decode_strategy_probe_once() {
    let input = PathBuf::from(
        std::env::var_os("TYPOKAT_WU0B_SNAPSHOT_INPUT")
            .expect("coordinator supplies a prebuilt snapshot artifact"),
    );
    let records = snapshot_decode_strategy_probe_for_test(&input, FAST_CLEAN)
        .expect("eager and optional immutable-indexed probe");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].strategy, SnapshotDecodeStrategy::EagerComplete);
    assert_eq!(
        records[1].strategy,
        SnapshotDecodeStrategy::ImmutableIndexed
    );
    assert_eq!(records[0].runtime_projection, records[1].runtime_projection);
    assert_eq!(
        records[0].diagnostic_identities,
        records[1].diagnostic_identities
    );
    assert_eq!(records[0].validated_bytes, records[0].artifact_bytes);
    assert_eq!(records[1].validated_bytes, records[1].artifact_bytes);
    for record in records {
        println!("{}", record.render());
    }
}

#[test]
#[ignore = "fresh-process coordinator runs this twice and byte-compares artifacts"]
fn snapshot_regeneration_probe_once() {
    let path = PathBuf::from(
        std::env::var_os("TYPOKAT_WU0B_SNAPSHOT_OUTPUT")
            .expect("coordinator supplies an exact fresh artifact path"),
    );
    snapshot_regeneration_artifact_for_test(&path)
        .expect("fresh process writes one complete snapshot artifact");
    assert!(path.is_file());
}

#[test]
#[ignore = "WU0B 1/2/32 shape probe; not a shared-base or parallelism claim"]
fn snapshot_scaling_probe_once() {
    let input = PathBuf::from(
        std::env::var_os("TYPOKAT_WU0B_SNAPSHOT_INPUT")
            .expect("coordinator supplies a prebuilt snapshot artifact"),
    );
    let record = snapshot_scaling_probe_for_test(&input, FAST_CLEAN, &[1, 2, 32])
        .expect("complete private-decode scaling probe");
    assert_eq!(record.route, "private-decode-per-caller");
    assert_eq!(record.check_counts(), [1, 2, 32]);
    assert!(record.all_semantically_identical());
    assert!(record.all_library_compilation_counts_are_zero());
    println!("{}", record.render());
}

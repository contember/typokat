//! Strict offline loader for the measurement-only library profile.

use super::super::library_compiler::InjectedLibrarySource;
use crate::source::LibraryFileOrdinal;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

const PROFILE_RELATIVE_PATH: &str = "src/library/typescript-6.0.3";
const PROFILE_SHA256: &str = "1edef1b5e870024834762267ec532c3054f3b2279e9181844e21648243eb1407";
const RAW_CONCAT_SHA256: &str = "0c68516cfe1dff30ce17425b2566813cf6d00c7f589dd24f31f4ba879b69a267";
const LENGTH_FRAMED_SHA256: &str =
    "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const ROOT_NAME: &str = "lib.es2025.full.d.ts";
const SOLE_EXTERNAL_MODULE: &str = "lib.es2025.iterator.d.ts";
const GIT_ATTRIBUTES: &str = "lib/*.d.ts -text -diff\n\
LICENSE.txt -text -diff\n\
ThirdPartyNoticeText.txt -text -diff\n\
profile.toml text eol=lf\n\
README.md text eol=lf\n\
THIRD_PARTY_NOTICE.md text eol=lf\n";

const ROOT_KEYS: &[&str] = &[
    "schema",
    "typescript_version",
    "upstream_revision",
    "root",
    "file_count",
    "script_file_count",
    "external_module_file_count",
    "reference_edge_count",
    "source_bytes",
    "source_lf",
    "source_cr",
    "root_sha256",
    "raw_concat_sha256",
    "length_framed_sha256",
    "length_frame",
    "order",
    "lib_entries_count",
    "lib_entries_filename_count",
    "lib_entries_framed_sha256",
    "license",
    "third_party_notice",
    "file",
];
const FILE_KEYS: &[&str] = &[
    "ordinal",
    "name",
    "npm_path",
    "git_path",
    "git_blob",
    "sha256",
    "bytes",
    "lf",
    "cr",
    "final_lf",
    "source_kind",
    "references",
];
const REFERENCE_KEYS: &[&str] = &["lib", "file"];
const NOTICE_KEYS: &[&str] = &[
    "name", "npm_path", "git_path", "git_blob", "sha256", "bytes", "lf", "cr", "final_lf",
];

#[derive(Clone, Copy)]
struct ExpectedNotice<'a> {
    label: &'a str,
    name: &'a str,
    git_blob: &'a str,
    sha256: &'a str,
    bytes: usize,
    lf: usize,
    cr: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedProfileSource {
    pub(crate) file_ordinal: LibraryFileOrdinal,
    pub(crate) name: String,
    pub(crate) source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StrictLibraryProfile {
    pub(crate) sources: Vec<OwnedProfileSource>,
}

impl StrictLibraryProfile {
    pub(crate) fn injected_sources(&self) -> Vec<InjectedLibrarySource<'_>> {
        self.sources
            .iter()
            .map(|source| InjectedLibrarySource {
                file_ordinal: source.file_ordinal,
                name: &source.name,
                source: &source.source,
            })
            .collect()
    }
}

pub(crate) fn load_strict_profile() -> Result<StrictLibraryProfile, String> {
    let source_root = match std::env::var_os("TYPOKAT_WU0B_PROFILE_ROOT") {
        Some(root) => {
            let root = PathBuf::from(root);
            if !root.is_absolute() {
                return Err("TYPOKAT_WU0B_PROFILE_ROOT must be absolute".to_owned());
            }
            root
        }
        None => std::env::current_dir()
            .map_err(|error| format!("cannot resolve the ordinary-test profile root: {error}"))?,
    };
    let profile_dir = source_root.join(PROFILE_RELATIVE_PATH);
    let (actual_files, actual_dirs) = collect_tree(&profile_dir)?;
    let library_dir = profile_dir.join("lib");
    require_real_directory(&library_dir)?;

    let manifest_bytes = read_regular(&profile_dir.join("profile.toml"))?;
    expect_digest(&manifest_bytes, PROFILE_SHA256, "profile.toml")?;
    if manifest_bytes.contains(&b'\r') || !manifest_bytes.ends_with(b"\n") {
        return Err("profile.toml must use LF line endings and end with LF".to_owned());
    }
    let manifest_text = std::str::from_utf8(&manifest_bytes)
        .map_err(|error| format!("profile.toml is not UTF-8: {error}"))?;
    let manifest = manifest_text
        .parse::<Value>()
        .map_err(|error| format!("invalid profile.toml: {error}"))?;
    let root = manifest
        .as_table()
        .ok_or_else(|| "profile.toml root must be a table".to_owned())?;
    exact_keys(root, ROOT_KEYS, "profile root")?;
    validate_root_contract(root)?;
    verify_notice(
        &profile_dir,
        required_table(root, "license")?,
        ExpectedNotice {
            label: "license",
            name: "LICENSE.txt",
            git_blob: "8746124b277914d0f0fd9cf4aef2ed3b587143d9",
            sha256: "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47",
            bytes: 9_197,
            lf: 55,
            cr: 55,
        },
    )?;
    verify_notice(
        &profile_dir,
        required_table(root, "third_party_notice")?,
        ExpectedNotice {
            label: "third-party notice",
            name: "ThirdPartyNoticeText.txt",
            git_blob: "a857fb3ce77c3b43c145f94aa8d910c7791394a5",
            sha256: "1af3c68039c57e539422da82a4faada506ce6d0ea6f90e0b699d02dbcdb7a90c",
            bytes: 37_824,
            lf: 193,
            cr: 193,
        },
    )?;
    let attributes = read_regular(&profile_dir.join(".gitattributes"))?;
    if attributes != GIT_ATTRIBUTES.as_bytes() {
        return Err("the profile .gitattributes contract changed".to_owned());
    }

    let rows = required_array(root, "file")?;
    if rows.len() != 82 {
        return Err(format!("expected 82 file entries, got {}", rows.len()));
    }

    let mut names = BTreeSet::new();
    let mut package_paths = BTreeSet::new();
    let mut git_paths = BTreeSet::new();
    let mut tables = Vec::with_capacity(rows.len());
    for (position, value) in rows.iter().enumerate() {
        let row = value
            .as_table()
            .ok_or_else(|| format!("file[{position}] must be a table"))?;
        exact_keys(row, FILE_KEYS, &format!("file[{position}]"))?;
        expect_usize(row, "ordinal", position)?;
        let name = required_string(row, "name")?;
        validate_library_name(name)?;
        let expected_path = format!("lib/{name}");
        expect_string(row, "npm_path", &expected_path)?;
        expect_string(row, "git_path", &expected_path)?;
        validate_safe_relative_posix(required_string(row, "npm_path")?)?;
        validate_safe_relative_posix(required_string(row, "git_path")?)?;
        validate_lower_hex(required_string(row, "git_blob")?, 40)?;
        validate_lower_hex(required_string(row, "sha256")?, 64)?;
        required_usize(row, "bytes")?;
        required_usize(row, "lf")?;
        required_usize(row, "cr")?;
        required_bool(row, "final_lf")?;
        required_string(row, "source_kind")?;
        for (reference_index, reference) in required_array(row, "references")?.iter().enumerate() {
            let reference = reference.as_table().ok_or_else(|| {
                format!("file[{position}].references[{reference_index}] must be a table")
            })?;
            exact_keys(
                reference,
                REFERENCE_KEYS,
                &format!("file[{position}].references[{reference_index}]"),
            )?;
            validate_logical_lib_name(required_string(reference, "lib")?)?;
            validate_library_name(required_string(reference, "file")?)?;
        }
        if !names.insert(name.to_owned()) {
            return Err(format!("duplicate manifest name: {name}"));
        }
        if !package_paths.insert(required_string(row, "npm_path")?.to_owned()) {
            return Err(format!("duplicate npm path: {expected_path}"));
        }
        if !git_paths.insert(required_string(row, "git_path")?.to_owned()) {
            return Err(format!("duplicate git path: {expected_path}"));
        }
        tables.push(row);
    }
    if required_string(tables[0], "name")? != "lib.es5.d.ts" {
        return Err("the canonical registry must start with lib.es5.d.ts".to_owned());
    }
    if required_string(
        tables
            .last()
            .ok_or_else(|| "the file registry is empty".to_owned())?,
        "name",
    )? != ROOT_NAME
    {
        return Err("the non-libEntries root must be the final registry entry".to_owned());
    }
    verify_tree_membership(&actual_files, &actual_dirs, &names)?;

    let mut graph = BTreeMap::new();
    let mut raw_concat = Sha256::new();
    let mut length_framed = Sha256::new();
    let mut sources = Vec::with_capacity(tables.len());
    let mut total_bytes = 0usize;
    let mut total_lf = 0usize;
    let mut total_cr = 0usize;
    let mut edge_count = 0usize;
    let mut script_count = 0usize;
    let mut external_modules = Vec::new();

    for (position, row) in tables.into_iter().enumerate() {
        let name = required_string(row, "name")?;
        let bytes = read_regular(&library_dir.join(name))?;
        let actual_sha = sha256_hex(&bytes);
        let actual_lf = byte_count(&bytes, b'\n');
        let actual_cr = byte_count(&bytes, b'\r');
        let actual_final_lf = bytes.ends_with(b"\n");
        expect_string(row, "sha256", &actual_sha)?;
        expect_usize(row, "bytes", bytes.len())?;
        expect_usize(row, "lf", actual_lf)?;
        expect_usize(row, "cr", actual_cr)?;
        expect_bool(row, "final_lf", actual_final_lf)?;
        if actual_cr != 0 || !actual_final_lf {
            return Err(format!("{name} must have zero CR bytes and a final LF"));
        }

        let source_kind = classify_source_kind(&bytes)?;
        expect_string(row, "source_kind", source_kind)?;
        match source_kind {
            "script" => script_count += 1,
            "external-module" => external_modules.push(name.to_owned()),
            _ => return Err(format!("unexpected source kind {source_kind}")),
        }

        let raw_references = parse_reference_libs(&bytes)?;
        let manifest_references = required_array(row, "references")?;
        if raw_references.len() != manifest_references.len() {
            return Err(format!(
                "{name} has {} raw references but {} manifest references",
                raw_references.len(),
                manifest_references.len()
            ));
        }
        let mut logical_names = BTreeSet::new();
        let mut resolved_names = BTreeSet::new();
        let mut resolved_edges = Vec::with_capacity(raw_references.len());
        for (reference_index, (logical_name, reference)) in raw_references
            .into_iter()
            .zip(manifest_references)
            .enumerate()
        {
            validate_logical_lib_name(&logical_name)?;
            if !logical_names.insert(logical_name.clone()) {
                return Err(format!("{name} repeats reference lib={logical_name:?}"));
            }
            let resolved_name = format!("lib.{logical_name}.d.ts");
            if !names.contains(&resolved_name) {
                return Err(format!(
                    "{name} references unknown or out-of-closure {logical_name:?} ({resolved_name})"
                ));
            }
            if !resolved_names.insert(resolved_name.clone()) {
                return Err(format!("{name} repeats resolved reference {resolved_name}"));
            }
            let reference = reference
                .as_table()
                .ok_or_else(|| format!("{name}.references[{reference_index}] must be a table"))?;
            expect_string(reference, "lib", &logical_name)?;
            expect_string(reference, "file", &resolved_name)?;
            resolved_edges.push(resolved_name);
        }

        let name_len = u64::try_from(name.len())
            .map_err(|_| format!("{name} name length does not fit u64"))?;
        let source_len = u64::try_from(bytes.len())
            .map_err(|_| format!("{name} source length does not fit u64"))?;
        raw_concat.update(&bytes);
        length_framed.update(name_len.to_be_bytes());
        length_framed.update(source_len.to_be_bytes());
        length_framed.update(name.as_bytes());
        length_framed.update(&bytes);
        total_bytes += bytes.len();
        total_lf += actual_lf;
        total_cr += actual_cr;
        edge_count += resolved_edges.len();
        graph.insert(name.to_owned(), resolved_edges);
        let source =
            String::from_utf8(bytes).map_err(|error| format!("{name} is not UTF-8: {error}"))?;
        sources.push(OwnedProfileSource {
            file_ordinal: LibraryFileOrdinal::new(position),
            name: name.to_owned(),
            source,
        });
    }

    if (total_bytes, total_lf, total_cr) != (2_936_611, 58_349, 0) {
        return Err(format!(
            "unexpected aggregate source counts: bytes={total_bytes}, lf={total_lf}, cr={total_cr}"
        ));
    }
    if edge_count != 110 || script_count != 81 {
        return Err(format!(
            "unexpected aggregate graph/kind counts: edges={edge_count}, scripts={script_count}"
        ));
    }
    if external_modules != [SOLE_EXTERNAL_MODULE] {
        return Err(format!(
            "the sole external module must be {SOLE_EXTERNAL_MODULE}, got {external_modules:?}"
        ));
    }
    let actual_raw = format!("{:x}", raw_concat.finalize());
    if actual_raw != RAW_CONCAT_SHA256 {
        return Err(format!(
            "raw concatenation fingerprint changed: expected {RAW_CONCAT_SHA256}, found {actual_raw}"
        ));
    }
    let actual_framed = format!("{:x}", length_framed.finalize());
    if actual_framed != LENGTH_FRAMED_SHA256 {
        return Err(format!(
            "length-framed registry fingerprint changed: expected {LENGTH_FRAMED_SHA256}, found {actual_framed}"
        ));
    }
    let reachable = validate_graph(&graph, ROOT_NAME)?;
    if reachable != names {
        let unreachable = names.difference(&reachable).cloned().collect::<Vec<_>>();
        return Err(format!(
            "root does not reach the whole closure: {unreachable:?}"
        ));
    }

    Ok(StrictLibraryProfile { sources })
}

fn validate_root_contract(root: &Table) -> Result<(), String> {
    for (key, expected) in [
        ("schema", 1),
        ("file_count", 82),
        ("script_file_count", 81),
        ("external_module_file_count", 1),
        ("reference_edge_count", 110),
        ("source_bytes", 2_936_611),
        ("source_lf", 58_349),
        ("source_cr", 0),
        ("lib_entries_count", 107),
        ("lib_entries_filename_count", 95),
    ] {
        expect_usize(root, key, expected)?;
    }
    for (key, expected) in [
        ("typescript_version", "6.0.3"),
        (
            "upstream_revision",
            "050880ce59e30b356b686bd3144efe24f875ebc8",
        ),
        ("root", ROOT_NAME),
        (
            "root_sha256",
            "e03da518b01b46a4c99a1f88cd727ee98ddf14492c43dae1ae7a63e992971bab",
        ),
        ("raw_concat_sha256", RAW_CONCAT_SHA256),
        ("length_framed_sha256", LENGTH_FRAMED_SHA256),
        (
            "length_frame",
            "u64be(name_len) || u64be(source_len) || name_utf8 || source",
        ),
        (
            "order",
            "getDefaultLibFilePriority/libEntries first occurrence; non-entry root last",
        ),
        (
            "lib_entries_framed_sha256",
            "7e237445dc1c4c7f32b6e829da48858fb3eafb0d0b3f3d9f5fe031d5b7a6d6f6",
        ),
    ] {
        expect_string(root, key, expected)?;
    }
    Ok(())
}

fn verify_notice(
    profile_dir: &Path,
    notice: &Table,
    expected: ExpectedNotice<'_>,
) -> Result<(), String> {
    exact_keys(notice, NOTICE_KEYS, expected.label)?;
    expect_string(notice, "name", expected.name)?;
    expect_string(notice, "npm_path", expected.name)?;
    expect_string(notice, "git_path", expected.name)?;
    validate_safe_relative_posix(required_string(notice, "npm_path")?)?;
    validate_safe_relative_posix(required_string(notice, "git_path")?)?;
    validate_lower_hex(required_string(notice, "git_blob")?, 40)?;
    validate_lower_hex(required_string(notice, "sha256")?, 64)?;
    expect_string(notice, "git_blob", expected.git_blob)?;
    expect_string(notice, "sha256", expected.sha256)?;
    expect_usize(notice, "bytes", expected.bytes)?;
    expect_usize(notice, "lf", expected.lf)?;
    expect_usize(notice, "cr", expected.cr)?;
    expect_bool(notice, "final_lf", true)?;

    let bytes = read_regular(&profile_dir.join(expected.name))?;
    verify_notice_bytes(&bytes, expected)
}

fn verify_notice_bytes(bytes: &[u8], expected: ExpectedNotice<'_>) -> Result<(), String> {
    if bytes.len() != expected.bytes
        || byte_count(bytes, b'\n') != expected.lf
        || byte_count(bytes, b'\r') != expected.cr
        || !bytes.ends_with(b"\n")
        || sha256_hex(bytes) != expected.sha256
    {
        return Err(format!(
            "{} bytes do not match the pinned snapshot",
            expected.label
        ));
    }
    Ok(())
}

fn verify_tree_membership(
    actual_files: &BTreeSet<String>,
    actual_dirs: &BTreeSet<String>,
    names: &BTreeSet<String>,
) -> Result<(), String> {
    let mut expected_files = [
        ".gitattributes",
        "LICENSE.txt",
        "README.md",
        "THIRD_PARTY_NOTICE.md",
        "ThirdPartyNoticeText.txt",
        "canonical.snapshot",
        "profile.toml",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    expected_files.extend(names.iter().map(|name| format!("lib/{name}")));
    let expected_dirs = BTreeSet::from(["lib".to_owned()]);
    if actual_files != &expected_files {
        let missing = expected_files
            .difference(actual_files)
            .cloned()
            .collect::<Vec<_>>();
        let extra = actual_files
            .difference(&expected_files)
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "profile tree/file manifest mismatch; missing={missing:?}, extra={extra:?}"
        ));
    }
    if actual_dirs != &expected_dirs {
        return Err(format!(
            "profile tree has unexpected directories: {actual_dirs:?}"
        ));
    }
    Ok(())
}

fn collect_tree(root: &Path) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    require_real_directory(root)?;
    let mut files = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    collect_tree_from(root, root, &mut files, &mut dirs)?;
    Ok((files, dirs))
}

fn collect_tree_from(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<String>,
    dirs: &mut BTreeSet<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("cannot read directory entry: {error}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot stat {}: {error}", path.display()))?;
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("cannot relativize {}: {error}", path.display()))?;
        let relative = relative_posix(relative)?;
        validate_safe_relative_posix(&relative)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(format!("profile tree contains symlink: {relative}"));
        }
        if file_type.is_dir() {
            dirs.insert(relative);
            collect_tree_from(root, &path, files, dirs)?;
        } else if file_type.is_file() {
            files.insert(relative);
        } else {
            return Err(format!(
                "profile tree contains non-regular entry: {relative}"
            ));
        }
    }
    Ok(())
}

fn relative_posix(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            return Err(format!(
                "unsafe relative path component in {}",
                path.display()
            ));
        };
        parts.push(
            part.to_str()
                .ok_or_else(|| format!("non-UTF-8 profile path: {}", path.display()))?,
        );
    }
    Ok(parts.join("/"))
}

fn require_real_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot stat {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{} must be a real directory", path.display()));
    }
    Ok(())
}

fn read_regular(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot stat {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "refusing to read non-regular or symlinked file: {}",
            path.display()
        ));
    }
    fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn exact_keys(table: &Table, expected: &[&str], label: &str) -> Result<(), String> {
    let actual = table.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let extra = actual.difference(&expected).copied().collect::<Vec<_>>();
        return Err(format!(
            "{label} has wrong keys; missing={missing:?}, extra={extra:?}"
        ));
    }
    Ok(())
}

fn required_value<'a>(table: &'a Table, key: &str) -> Result<&'a Value, String> {
    table
        .get(key)
        .ok_or_else(|| format!("missing required key {key:?}"))
}

fn required_table<'a>(table: &'a Table, key: &str) -> Result<&'a Table, String> {
    required_value(table, key)?
        .as_table()
        .ok_or_else(|| format!("{key:?} must be a table"))
}

fn required_array<'a>(table: &'a Table, key: &str) -> Result<&'a [Value], String> {
    required_value(table, key)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{key:?} must be an array"))
}

fn required_string<'a>(table: &'a Table, key: &str) -> Result<&'a str, String> {
    required_value(table, key)?
        .as_str()
        .ok_or_else(|| format!("{key:?} must be a string"))
}

fn required_usize(table: &Table, key: &str) -> Result<usize, String> {
    let value = required_value(table, key)?
        .as_integer()
        .ok_or_else(|| format!("{key:?} must be an integer"))?;
    usize::try_from(value).map_err(|_| format!("{key:?} must be a non-negative usize"))
}

fn required_bool(table: &Table, key: &str) -> Result<bool, String> {
    required_value(table, key)?
        .as_bool()
        .ok_or_else(|| format!("{key:?} must be a boolean"))
}

fn expect_string(table: &Table, key: &str, expected: &str) -> Result<(), String> {
    let actual = required_string(table, key)?;
    if actual != expected {
        return Err(format!("{key:?}: expected {expected:?}, found {actual:?}"));
    }
    Ok(())
}

fn expect_usize(table: &Table, key: &str, expected: usize) -> Result<(), String> {
    let actual = required_usize(table, key)?;
    if actual != expected {
        return Err(format!("{key:?}: expected {expected}, found {actual}"));
    }
    Ok(())
}

fn expect_bool(table: &Table, key: &str, expected: bool) -> Result<(), String> {
    let actual = required_bool(table, key)?;
    if actual != expected {
        return Err(format!("{key:?}: expected {expected}, found {actual}"));
    }
    Ok(())
}

fn validate_safe_relative_posix(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(format!("unsafe relative POSIX path: {path:?}"));
    }
    Ok(())
}

fn validate_library_name(name: &str) -> Result<(), String> {
    if !name.starts_with("lib.")
        || !name.ends_with(".d.ts")
        || name.contains('/')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.')
    {
        return Err(format!("invalid library filename: {name:?}"));
    }
    Ok(())
}

fn validate_logical_lib_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.split('.').any(str::is_empty)
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.')
    {
        return Err(format!("invalid reference lib name: {name:?}"));
    }
    Ok(())
}

fn validate_lower_hex(value: &str, expected_len: usize) -> Result<(), String> {
    if value.len() != expected_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "expected {expected_len} lowercase hex characters, got {value:?}"
        ));
    }
    Ok(())
}

fn parse_reference_libs(source: &[u8]) -> Result<Vec<String>, String> {
    let source = std::str::from_utf8(source)
        .map_err(|error| format!("declaration source is not UTF-8: {error}"))?;
    let mut references = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let Some(after_slashes) = trimmed.strip_prefix("///") else {
            continue;
        };
        let after_slashes = after_slashes.trim_start();
        if !after_slashes.starts_with("<reference") {
            continue;
        }
        let body = after_slashes
            .strip_prefix("<reference")
            .and_then(|body| body.trim().strip_suffix("/>"))
            .map(str::trim)
            .ok_or_else(|| format!("malformed reference directive on line {}", line_index + 1))?;
        let value = body
            .strip_prefix("lib=\"")
            .ok_or_else(|| format!("unsupported reference directive on line {}", line_index + 1))?;
        let (logical_name, trailing) = value.split_once('"').ok_or_else(|| {
            format!(
                "unterminated reference directive on line {}",
                line_index + 1
            )
        })?;
        if !trailing.trim().is_empty() {
            return Err(format!(
                "unexpected reference attributes on line {}",
                line_index + 1
            ));
        }
        references.push(logical_name.to_owned());
    }
    Ok(references)
}

fn classify_source_kind(source: &[u8]) -> Result<&'static str, String> {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum State {
        Normal,
        LineComment,
        BlockComment,
        SingleQuote,
        DoubleQuote,
        Template,
    }

    let mut state = State::Normal;
    let mut brace_depth = 0usize;
    let mut index = 0usize;
    while index < source.len() {
        let byte = source[index];
        match state {
            State::Normal => match byte {
                b'/' if source.get(index + 1) == Some(&b'/') => {
                    state = State::LineComment;
                    index += 2;
                    continue;
                }
                b'/' if source.get(index + 1) == Some(&b'*') => {
                    state = State::BlockComment;
                    index += 2;
                    continue;
                }
                b'\'' => state = State::SingleQuote,
                b'"' => state = State::DoubleQuote,
                b'`' => state = State::Template,
                b'{' => brace_depth += 1,
                b'}' => {
                    brace_depth = brace_depth
                        .checked_sub(1)
                        .ok_or_else(|| "source has an unmatched closing brace".to_owned())?;
                }
                _ if brace_depth == 0 && is_identifier_start(byte) => {
                    let start = index;
                    index += 1;
                    while source
                        .get(index)
                        .is_some_and(|next| is_identifier_continue(*next))
                    {
                        index += 1;
                    }
                    let token = &source[start..index];
                    if token == b"export" {
                        return Ok("external-module");
                    }
                    if token == b"import" {
                        let next = source[index..]
                            .iter()
                            .copied()
                            .find(|next| !next.is_ascii_whitespace());
                        if !matches!(next, Some(b'(' | b'.')) {
                            return Ok("external-module");
                        }
                    }
                    continue;
                }
                _ => {}
            },
            State::LineComment => {
                if byte == b'\n' {
                    state = State::Normal;
                }
            }
            State::BlockComment => {
                if byte == b'*' && source.get(index + 1) == Some(&b'/') {
                    state = State::Normal;
                    index += 2;
                    continue;
                }
            }
            State::SingleQuote | State::DoubleQuote | State::Template => {
                if byte == b'\\' {
                    index += 2;
                    continue;
                }
                let terminator = match state {
                    State::SingleQuote => b'\'',
                    State::DoubleQuote => b'"',
                    State::Template => b'`',
                    State::Normal | State::LineComment | State::BlockComment => unreachable!(),
                };
                if byte == terminator {
                    state = State::Normal;
                }
            }
        }
        index += 1;
    }
    if brace_depth != 0 || !matches!(state, State::Normal | State::LineComment) {
        return Err("source ends inside an unterminated lexical construct".to_owned());
    }
    Ok("script")
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn validate_graph(
    graph: &BTreeMap<String, Vec<String>>,
    root: &str,
) -> Result<BTreeSet<String>, String> {
    fn visit(
        name: &str,
        graph: &BTreeMap<String, Vec<String>>,
        states: &mut BTreeMap<String, u8>,
        reachable: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        match states.get(name).copied() {
            Some(1) => return Err(format!("reference cycle reaches {name}")),
            Some(2) => return Ok(()),
            _ => {}
        }
        states.insert(name.to_owned(), 1);
        reachable.insert(name.to_owned());
        for target in graph
            .get(name)
            .ok_or_else(|| format!("unknown graph node {name}"))?
        {
            visit(target, graph, states, reachable)?;
        }
        states.insert(name.to_owned(), 2);
        Ok(())
    }

    if !graph.contains_key(root) {
        return Err(format!("graph is missing root {root}"));
    }
    let mut states = BTreeMap::new();
    let mut reachable = BTreeSet::new();
    visit(root, graph, &mut states, &mut reachable)?;
    Ok(reachable)
}

fn byte_count(bytes: &[u8], needle: u8) -> usize {
    bytes.iter().filter(|byte| **byte == needle).count()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn expect_digest(bytes: &[u8], expected: &str, label: &str) -> Result<(), String> {
    let actual = sha256_hex(bytes);
    if actual != expected {
        return Err(format!(
            "{label} fingerprint changed: expected {expected}, found {actual}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_loader_returns_the_exact_owned_registry_and_borrowed_view() -> Result<(), String> {
        let profile = load_strict_profile()?;
        assert_eq!(profile.sources.len(), 82);
        assert_eq!(profile.sources[0].file_ordinal.index(), 0);
        assert_eq!(profile.sources[0].name, "lib.es5.d.ts");
        assert_eq!(profile.sources[81].file_ordinal.index(), 81);
        assert_eq!(profile.sources[81].name, ROOT_NAME);
        let injected = profile.injected_sources();
        assert_eq!(injected.len(), 82);
        assert_eq!(injected[76].name, SOLE_EXTERNAL_MODULE);
        assert_eq!(injected[76].file_ordinal.index(), 76);
        assert!(injected.iter().all(|source| !source.source.is_empty()));
        Ok(())
    }

    #[test]
    fn strict_path_and_graph_helpers_fail_closed() {
        for invalid in ["", "/absolute", "../escape", "lib/../escape", "lib\\x"] {
            assert!(
                validate_safe_relative_posix(invalid).is_err(),
                "{invalid:?}"
            );
        }
        let cycle = BTreeMap::from([
            ("root".to_owned(), vec!["dependency".to_owned()]),
            ("dependency".to_owned(), vec!["root".to_owned()]),
        ]);
        assert!(validate_graph(&cycle, "root").is_err());
    }

    #[test]
    fn notice_bytes_and_full_tree_membership_reject_mutations() {
        let notice_bytes = b"pinned notice\r\n";
        let notice_sha = sha256_hex(notice_bytes);
        let notice = ExpectedNotice {
            label: "test notice",
            name: "NOTICE.txt",
            git_blob: "0000000000000000000000000000000000000000",
            sha256: &notice_sha,
            bytes: notice_bytes.len(),
            lf: 1,
            cr: 1,
        };
        assert!(verify_notice_bytes(notice_bytes, notice).is_ok());
        assert!(verify_notice_bytes(b"mutated notice\r\n", notice).is_err());

        let names = BTreeSet::from(["lib.example.d.ts".to_owned()]);
        let mut files = [
            ".gitattributes",
            "LICENSE.txt",
            "README.md",
            "THIRD_PARTY_NOTICE.md",
            "ThirdPartyNoticeText.txt",
            "canonical.snapshot",
            "profile.toml",
            "lib/lib.example.d.ts",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        let dirs = BTreeSet::from(["lib".to_owned()]);
        assert!(verify_tree_membership(&files, &dirs, &names).is_ok());
        files.remove("LICENSE.txt");
        assert!(verify_tree_membership(&files, &dirs, &names).is_err());
        files.insert("LICENSE.txt".to_owned());
        files.insert("unexpected.txt".to_owned());
        assert!(verify_tree_membership(&files, &dirs, &names).is_err());
        assert!(verify_tree_membership(
            &files
                .into_iter()
                .filter(|name| name != "unexpected.txt")
                .collect(),
            &BTreeSet::from(["lib".to_owned(), "nested".to_owned()]),
            &names,
        )
        .is_err());
    }
}

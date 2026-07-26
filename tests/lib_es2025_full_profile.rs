use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
use toml::{Table, Value};

const PROFILE_RELATIVE_PATH: &str = "src/library/typescript-6.0.3";
const ROOT_NAME: &str = "lib.es2025.full.d.ts";
const TYPESCRIPT_VERSION: &str = "6.0.3";
const UPSTREAM_REVISION: &str = "050880ce59e30b356b686bd3144efe24f875ebc8";
const PROFILE_SHA256: &str = "1edef1b5e870024834762267ec532c3054f3b2279e9181844e21648243eb1407";
const ROOT_GIT_BLOB: &str = "0870bb4f53d7b42022c46324b6fb001660283c86";
const ROOT_SHA256: &str = "e03da518b01b46a4c99a1f88cd727ee98ddf14492c43dae1ae7a63e992971bab";
const RAW_CONCAT_SHA256: &str = "0c68516cfe1dff30ce17425b2566813cf6d00c7f589dd24f31f4ba879b69a267";
const LENGTH_FRAMED_SHA256: &str =
    "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const LIB_ENTRIES_FRAMED_SHA256: &str =
    "7e237445dc1c4c7f32b6e829da48858fb3eafb0d0b3f3d9f5fe031d5b7a6d6f6";

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
const NOTICE_KEYS: &[&str] = &[
    "name", "npm_path", "git_path", "git_blob", "sha256", "bytes", "lf", "cr", "final_lf",
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

const GIT_ATTRIBUTES: &str = "lib/*.d.ts -text -diff\n\
LICENSE.txt -text -diff\n\
ThirdPartyNoticeText.txt -text -diff\n\
profile.toml text eol=lf\n\
README.md text eol=lf\n\
THIRD_PARTY_NOTICE.md text eol=lf\n";

#[test]
fn committed_full_profile_is_an_exact_offline_snapshot() -> Result<(), String> {
    let profile_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PROFILE_RELATIVE_PATH);
    let (actual_files, actual_dirs) = collect_tree(&profile_dir)?;
    let manifest_bytes = read_regular(&profile_dir.join("profile.toml"))?;
    verify_profile_sha256(&manifest_bytes)?;
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

    expect_usize(root, "schema", 1)?;
    expect_string(root, "typescript_version", TYPESCRIPT_VERSION)?;
    expect_string(root, "upstream_revision", UPSTREAM_REVISION)?;
    expect_string(root, "root", ROOT_NAME)?;
    expect_usize(root, "file_count", 82)?;
    expect_usize(root, "script_file_count", 81)?;
    expect_usize(root, "external_module_file_count", 1)?;
    expect_usize(root, "reference_edge_count", 110)?;
    expect_usize(root, "source_bytes", 2_936_611)?;
    expect_usize(root, "source_lf", 58_349)?;
    expect_usize(root, "source_cr", 0)?;
    expect_string(root, "root_sha256", ROOT_SHA256)?;
    expect_string(root, "raw_concat_sha256", RAW_CONCAT_SHA256)?;
    expect_string(root, "length_framed_sha256", LENGTH_FRAMED_SHA256)?;
    expect_string(
        root,
        "length_frame",
        "u64be(name_len) || u64be(source_len) || name_utf8 || source",
    )?;
    expect_string(
        root,
        "order",
        "getDefaultLibFilePriority/libEntries first occurrence; non-entry root last",
    )?;
    expect_usize(root, "lib_entries_count", 107)?;
    expect_usize(root, "lib_entries_filename_count", 95)?;
    expect_string(root, "lib_entries_framed_sha256", LIB_ENTRIES_FRAMED_SHA256)?;

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

    let files = required_array(root, "file")?;
    if files.len() != 82 {
        return Err(format!("expected 82 file entries, got {}", files.len()));
    }

    let mut names = BTreeSet::new();
    let mut package_paths = BTreeSet::new();
    let mut git_paths = BTreeSet::new();
    let mut file_tables = Vec::with_capacity(files.len());
    for (ordinal, value) in files.iter().enumerate() {
        let file = value
            .as_table()
            .ok_or_else(|| format!("file[{ordinal}] must be a table"))?;
        exact_keys(file, FILE_KEYS, &format!("file[{ordinal}]"))?;
        expect_usize(file, "ordinal", ordinal)?;

        let name = required_string(file, "name")?;
        validate_library_name(name)?;
        let expected_path = format!("lib/{name}");
        expect_string(file, "npm_path", &expected_path)?;
        expect_string(file, "git_path", &expected_path)?;
        validate_safe_relative_posix(required_string(file, "npm_path")?)?;
        validate_safe_relative_posix(required_string(file, "git_path")?)?;
        validate_lower_hex(required_string(file, "git_blob")?, 40)?;
        validate_lower_hex(required_string(file, "sha256")?, 64)?;
        required_usize(file, "bytes")?;
        required_usize(file, "lf")?;
        required_usize(file, "cr")?;
        required_bool(file, "final_lf")?;
        required_string(file, "source_kind")?;

        let references = required_array(file, "references")?;
        for (reference_index, reference) in references.iter().enumerate() {
            let reference = reference.as_table().ok_or_else(|| {
                format!("file[{ordinal}].references[{reference_index}] must be a table")
            })?;
            exact_keys(
                reference,
                REFERENCE_KEYS,
                &format!("file[{ordinal}].references[{reference_index}]"),
            )?;
            required_string(reference, "lib")?;
            required_string(reference, "file")?;
        }

        if !names.insert(name.to_owned()) {
            return Err(format!("duplicate manifest name: {name}"));
        }
        if !package_paths.insert(required_string(file, "npm_path")?.to_owned()) {
            return Err(format!("duplicate npm path: {expected_path}"));
        }
        if !git_paths.insert(required_string(file, "git_path")?.to_owned()) {
            return Err(format!("duplicate git path: {expected_path}"));
        }
        file_tables.push(file);
    }

    if required_string(file_tables[0], "name")? != "lib.es5.d.ts" {
        return Err("the canonical registry must start with lib.es5.d.ts".to_owned());
    }
    let last = file_tables
        .last()
        .ok_or_else(|| "the file registry is empty".to_owned())?;
    if required_string(last, "name")? != ROOT_NAME {
        return Err("the non-libEntries root must be the final registry entry".to_owned());
    }

    verify_tree_membership(&actual_files, &actual_dirs, &names)?;

    let mut graph = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut digest_names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut raw_concat = Sha256::new();
    let mut length_framed = Sha256::new();
    let mut total_bytes = 0usize;
    let mut total_lf = 0usize;
    let mut total_cr = 0usize;
    let mut edge_count = 0usize;
    let mut script_count = 0usize;
    let mut external_module_names = Vec::new();

    for file in &file_tables {
        let name = required_string(file, "name")?;
        let source = read_regular(&profile_dir.join("lib").join(name))?;
        let actual_sha = sha256_hex(&source);
        let actual_lf = byte_count(&source, b'\n');
        let actual_cr = byte_count(&source, b'\r');
        let actual_final_lf = source.ends_with(b"\n");

        expect_usize(file, "bytes", source.len())?;
        expect_usize(file, "lf", actual_lf)?;
        expect_usize(file, "cr", actual_cr)?;
        expect_bool(file, "final_lf", actual_final_lf)?;
        expect_string(file, "sha256", &actual_sha)?;
        if actual_cr != 0 || !actual_final_lf {
            return Err(format!("{name} must have zero CR bytes and a final LF"));
        }

        let source_kind = classify_source_kind(&source)?;
        expect_string(file, "source_kind", source_kind)?;
        match source_kind {
            "script" => script_count += 1,
            "external-module" => external_module_names.push(name.to_owned()),
            _ => {
                return Err(format!(
                    "unexpected independently classified kind: {source_kind}"
                ))
            }
        }

        let raw_references = parse_reference_libs(&source)?;
        let mut logical_names = BTreeSet::new();
        let mut resolved_names = BTreeSet::new();
        let mut resolved_edges = Vec::with_capacity(raw_references.len());
        for logical_name in raw_references {
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
            resolved_edges.push((logical_name, resolved_name));
        }

        let manifest_references = required_array(file, "references")?;
        if manifest_references.len() != resolved_edges.len() {
            return Err(format!(
                "{name} has {} raw references but {} manifest references",
                resolved_edges.len(),
                manifest_references.len()
            ));
        }
        for (reference_index, ((logical_name, resolved_name), reference)) in
            resolved_edges.iter().zip(manifest_references).enumerate()
        {
            let reference = reference
                .as_table()
                .ok_or_else(|| format!("{name}.references[{reference_index}] must be a table"))?;
            expect_string(reference, "lib", logical_name)?;
            expect_string(reference, "file", resolved_name)?;
        }

        let source_len = u64::try_from(source.len())
            .map_err(|_| format!("{name} source length does not fit u64"))?;
        let name_len = u64::try_from(name.len())
            .map_err(|_| format!("{name} name length does not fit u64"))?;
        raw_concat.update(&source);
        length_framed.update(name_len.to_be_bytes());
        length_framed.update(source_len.to_be_bytes());
        length_framed.update(name.as_bytes());
        length_framed.update(&source);

        total_bytes += source.len();
        total_lf += actual_lf;
        total_cr += actual_cr;
        edge_count += resolved_edges.len();
        graph.insert(
            name.to_owned(),
            resolved_edges
                .into_iter()
                .map(|(_, resolved)| resolved)
                .collect(),
        );
        digest_names
            .entry(actual_sha)
            .or_default()
            .push(name.to_owned());
        sources.insert(name.to_owned(), source);
    }

    if total_bytes != 2_936_611 || total_lf != 58_349 || total_cr != 0 {
        return Err(format!(
            "unexpected aggregate source counts: bytes={total_bytes}, lf={total_lf}, cr={total_cr}"
        ));
    }
    if edge_count != 110 || script_count != 81 {
        return Err(format!(
            "unexpected aggregate graph/kind counts: edges={edge_count}, scripts={script_count}"
        ));
    }
    if external_module_names != ["lib.es2025.iterator.d.ts"] {
        return Err(format!(
            "the sole external module must be lib.es2025.iterator.d.ts, got {external_module_names:?}"
        ));
    }
    if format!("{:x}", raw_concat.finalize()) != RAW_CONCAT_SHA256 {
        return Err("raw concatenation fingerprint changed".to_owned());
    }
    if format!("{:x}", length_framed.finalize()) != LENGTH_FRAMED_SHA256 {
        return Err("length-framed registry fingerprint changed".to_owned());
    }

    let root_file = file_tables
        .iter()
        .find(|file| required_string(file, "name") == Ok(ROOT_NAME))
        .ok_or_else(|| format!("missing root {ROOT_NAME}"))?;
    expect_string(root_file, "git_blob", ROOT_GIT_BLOB)?;
    expect_string(root_file, "sha256", ROOT_SHA256)?;
    let root_source = sources
        .get(ROOT_NAME)
        .ok_or_else(|| format!("missing root source {ROOT_NAME}"))?;
    if sha256_hex(root_source) != ROOT_SHA256 {
        return Err("root source fingerprint changed".to_owned());
    }

    let reachable = validate_graph(&graph, ROOT_NAME)?;
    if reachable != names {
        let unreachable: Vec<_> = names.difference(&reachable).cloned().collect();
        return Err(format!(
            "root does not reach the whole closure: {unreachable:?}"
        ));
    }

    let mut duplicate_digest_groups: Vec<Vec<String>> = digest_names
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();
    for group in &mut duplicate_digest_groups {
        group.sort();
    }
    duplicate_digest_groups.sort();
    if duplicate_digest_groups
        != [vec![
            "lib.dom.asynciterable.d.ts".to_owned(),
            "lib.dom.iterable.d.ts".to_owned(),
        ]]
    {
        return Err(format!(
            "unexpected duplicate source bodies: {duplicate_digest_groups:?}"
        ));
    }
    if sources.get("lib.dom.asynciterable.d.ts") != sources.get("lib.dom.iterable.d.ts") {
        return Err("the intentional DOM iterable source pair diverged".to_owned());
    }
    let async_dom = file_tables
        .iter()
        .find(|file| required_string(file, "name") == Ok("lib.dom.asynciterable.d.ts"))
        .ok_or_else(|| "missing lib.dom.asynciterable.d.ts".to_owned())?;
    let iterable_dom = file_tables
        .iter()
        .find(|file| required_string(file, "name") == Ok("lib.dom.iterable.d.ts"))
        .ok_or_else(|| "missing lib.dom.iterable.d.ts".to_owned())?;
    if required_string(async_dom, "git_blob")? != required_string(iterable_dom, "git_blob")?
        || required_string(async_dom, "sha256")? != required_string(iterable_dom, "sha256")?
    {
        return Err("the intentional DOM iterable pair must share blob and SHA-256".to_owned());
    }

    Ok(())
}

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
    if bytes.len() != expected.bytes
        || byte_count(&bytes, b'\n') != expected.lf
        || byte_count(&bytes, b'\r') != expected.cr
        || !bytes.ends_with(b"\n")
        || sha256_hex(&bytes) != expected.sha256
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
    let mut expected_files: BTreeSet<String> = [
        ".gitattributes",
        "LICENSE.txt",
        "README.md",
        "THIRD_PARTY_NOTICE.md",
        "ThirdPartyNoticeText.txt",
        "profile.toml",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    expected_files.extend(names.iter().map(|name| format!("lib/{name}")));
    let expected_dirs = BTreeSet::from(["lib".to_owned()]);

    if actual_files != &expected_files {
        let missing: Vec<_> = expected_files.difference(actual_files).cloned().collect();
        let extra: Vec<_> = actual_files.difference(&expected_files).cloned().collect();
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
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("cannot stat {}: {error}", root.display()))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(format!("{} must be a real directory", root.display()));
    }

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
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
    for entry in entries {
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

fn exact_keys(table: &Table, expected: &[&str], label: &str) -> Result<(), String> {
    let actual: BTreeSet<&str> = table.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    if actual != expected {
        let missing: Vec<_> = expected.difference(&actual).copied().collect();
        let extra: Vec<_> = actual.difference(&expected).copied().collect();
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
    {
        return Err(format!("unsafe relative POSIX path: {path:?}"));
    }
    if path
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
        || name.split('.').any(|part| part.is_empty())
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
    if !graph.contains_key(root) {
        return Err(format!("graph is missing root {root}"));
    }
    for (source, targets) in graph {
        for target in targets {
            if !graph.contains_key(target) {
                return Err(format!("{source} references unknown graph node {target}"));
            }
        }
    }

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

    let mut states = BTreeMap::new();
    let mut reachable = BTreeSet::new();
    visit(root, graph, &mut states, &mut reachable)?;
    Ok(reachable)
}

fn read_regular(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot stat {}: {error}", path.display()))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() || !file_type.is_file() {
        return Err(format!(
            "refusing to read non-regular or symlinked file: {}",
            path.display()
        ));
    }
    fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn byte_count(bytes: &[u8], needle: u8) -> usize {
    bytes.iter().filter(|byte| **byte == needle).count()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verify_profile_sha256(bytes: &[u8]) -> Result<(), String> {
    let actual = sha256_hex(bytes);
    if actual != PROFILE_SHA256 {
        return Err(format!(
            "profile.toml fingerprint changed: expected {PROFILE_SHA256}, found {actual}"
        ));
    }
    Ok(())
}

#[cfg(unix)]
static NEXT_TEMPORARY_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
struct TemporaryDirectory {
    path: PathBuf,
}

#[cfg(unix)]
impl TemporaryDirectory {
    fn new(label: &str) -> Result<Self, String> {
        let unique = NEXT_TEMPORARY_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("typokat-{label}-{}-{unique}", std::process::id()));
        fs::create_dir(&path)
            .map_err(|error| format!("cannot create test directory {}: {error}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn profile_hash_rejects_a_non_root_git_blob_mutation() -> Result<(), String> {
    let profile_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PROFILE_RELATIVE_PATH);
    collect_tree(&profile_dir)?;
    let manifest_bytes = read_regular(&profile_dir.join("profile.toml"))?;
    verify_profile_sha256(&manifest_bytes)?;

    let manifest = std::str::from_utf8(&manifest_bytes)
        .map_err(|error| format!("profile.toml is not UTF-8: {error}"))?;
    let original = "git_blob = \"496166ca309c28ab7e07ea0154a406f26b6cf26a\"";
    let replacement = "git_blob = \"596166ca309c28ab7e07ea0154a406f26b6cf26a\"";
    if !manifest.contains(original) {
        return Err("non-root lib.es5.d.ts git_blob fixture is missing".to_owned());
    }
    let mutated = manifest.replacen(original, replacement, 1);
    if verify_profile_sha256(mutated.as_bytes()).is_ok() {
        return Err("profile hash accepted a non-root git_blob mutation".to_owned());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn early_scan_and_direct_read_reject_a_symlinked_profile_toml() -> Result<(), String> {
    use std::os::unix::fs::symlink;

    let directory = TemporaryDirectory::new("profile-symlink")?;
    let target = directory.path().join("target.toml");
    fs::write(&target, b"schema = 1\n")
        .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
    let profile = directory.path().join("profile.toml");
    symlink("target.toml", &profile)
        .map_err(|error| format!("cannot create {}: {error}", profile.display()))?;

    if collect_tree(directory.path()).is_ok() {
        return Err("early profile scan accepted a symlink".to_owned());
    }
    if read_regular(&profile).is_ok() {
        return Err("direct guarded read accepted a symlink".to_owned());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn early_scan_and_direct_read_reject_a_socket_via_the_non_regular_branch() -> Result<(), String> {
    use std::os::unix::net::UnixListener;

    let directory = TemporaryDirectory::new("profile-socket")?;
    let profile = directory.path().join("profile.toml");
    let _listener = UnixListener::bind(&profile)
        .map_err(|error| format!("cannot bind {}: {error}", profile.display()))?;

    if collect_tree(directory.path()).is_ok() {
        return Err("early profile scan accepted a non-regular socket".to_owned());
    }
    if read_regular(&profile).is_ok() {
        return Err("direct guarded read accepted a non-regular socket".to_owned());
    }
    Ok(())
}

#[test]
fn exact_key_validation_rejects_manifest_shape_mutations() {
    let mut table = Table::new();
    table.insert("required".to_owned(), Value::Integer(1));
    assert!(exact_keys(&table, &["required"], "fixture").is_ok());

    table.insert("extra".to_owned(), Value::Integer(2));
    assert!(exact_keys(&table, &["required"], "fixture").is_err());
    assert!(exact_keys(&Table::new(), &["required"], "fixture").is_err());
}

#[test]
fn exact_tree_membership_requires_every_source_asset_and_rejects_extras() {
    let names = BTreeSet::from(["lib.es5.d.ts".to_owned()]);
    let dirs = BTreeSet::from(["lib".to_owned()]);
    let mut files: BTreeSet<String> = [
        ".gitattributes",
        "LICENSE.txt",
        "README.md",
        "THIRD_PARTY_NOTICE.md",
        "ThirdPartyNoticeText.txt",
        "profile.toml",
        "lib/lib.es5.d.ts",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    assert!(verify_tree_membership(&files, &dirs, &names).is_ok());

    files.remove("profile.toml");
    assert!(verify_tree_membership(&files, &dirs, &names).is_err());

    files.insert("profile.toml".to_owned());
    files.insert("unexpected.snapshot".to_owned());
    assert!(verify_tree_membership(&files, &dirs, &names).is_err());
}

#[test]
fn path_validation_rejects_non_posix_or_escaping_paths() {
    for invalid in [
        "",
        "/absolute",
        "../escape",
        "lib/../escape",
        "lib/./file.d.ts",
        "lib//file.d.ts",
        "lib\\file.d.ts",
        "lib/",
    ] {
        assert!(
            validate_safe_relative_posix(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
    assert!(validate_safe_relative_posix("lib/lib.es5.d.ts").is_ok());
}

#[test]
fn lowercase_hex_validation_rejects_mutations() {
    assert!(validate_lower_hex("0123456789abcdef", 16).is_ok());
    assert!(validate_lower_hex("0123456789abcdeF", 16).is_err());
    assert!(validate_lower_hex("0123456789abcdeg", 16).is_err());
    assert!(validate_lower_hex("0123456789abcde", 16).is_err());
}

#[test]
fn graph_validation_rejects_unknown_nodes_and_cycles() {
    let unknown = BTreeMap::from([("root".to_owned(), vec!["missing".to_owned()])]);
    assert!(validate_graph(&unknown, "root").is_err());

    let cycle = BTreeMap::from([
        ("root".to_owned(), vec!["dependency".to_owned()]),
        ("dependency".to_owned(), vec!["root".to_owned()]),
    ]);
    assert!(validate_graph(&cycle, "root").is_err());
}

#[test]
fn raw_reference_parser_rejects_malformed_directives() {
    assert!(parse_reference_libs(b"/// <reference lib=\"es5\" />\n").is_ok());
    assert!(parse_reference_libs(b"/// <reference path=\"other.d.ts\" />\n").is_err());
    assert!(parse_reference_libs(b"/// <reference lib=\"es5\">\n").is_err());
}

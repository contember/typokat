#!/usr/bin/env python3
"""Fail-closed packaged-crate coordinator for the default-library sources."""

from __future__ import annotations

import hashlib
import json
import os
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from pathlib import Path, PurePosixPath
from typing import Any, Iterable


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
PACKAGE_RELATIVE_ROOT = Path("crates/typokat-library")
PACKAGE_ROOT = ROOT / PACKAGE_RELATIVE_ROOT
CONTRACT_PATH = HERE / "contract.toml"
PROFILE_PREFIX = "src/typescript-6.0.3/"
LIB_PREFIX = PROFILE_PREFIX + "lib/"
INTERNAL_PACKAGES = (
    "typokat-binder",
    "typokat-check",
    "typokat-core",
    "typokat-diagnostics",
    "typokat-frontend",
    "typokat-relate",
    "typokat-types",
)
HEX = frozenset("0123456789abcdef")
PROFILE_ROOT_KEYS = {
    "schema", "typescript_version", "upstream_revision", "root", "file_count",
    "script_file_count", "external_module_file_count", "reference_edge_count",
    "source_bytes", "source_lf", "source_cr", "root_sha256", "raw_concat_sha256",
    "length_framed_sha256", "length_frame", "order", "lib_entries_count",
    "lib_entries_filename_count", "lib_entries_framed_sha256", "license",
    "third_party_notice", "file",
}
PROFILE_NOTICE_KEYS = {
    "name", "npm_path", "git_path", "git_blob", "sha256", "bytes", "lf", "cr",
    "final_lf",
}
PROFILE_FILE_KEYS = {
    "ordinal", "name", "npm_path", "git_path", "git_blob", "sha256", "bytes",
    "lf", "cr", "final_lf", "source_kind", "references",
}
PROFILE_REFERENCE_KEYS = {"lib", "file"}


class ContractError(RuntimeError):
    """The release/package proof did not satisfy its exact contract."""


def _exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        actual = sorted(value) if isinstance(value, dict) else type(value).__name__
        raise ContractError(
            f"{label} keys differ: expected={sorted(expected)!r} actual={actual!r}"
        )
    return value


def _integer(value: Any, label: str, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise ContractError(f"{label} must be an integer >= {minimum}")
    return value


def _string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise ContractError(f"{label} must be a nonempty string")
    return value


def _digest(value: Any, label: str) -> str:
    text = _string(value, label)
    if len(text) != 64 or any(character not in HEX for character in text):
        raise ContractError(f"{label} must be a lowercase SHA-256 digest")
    return text


def _string_list(value: Any, label: str) -> list[str]:
    if (
        not isinstance(value, list)
        or not value
        or any(not isinstance(item, str) or not item for item in value)
        or len(value) != len(set(value))
    ):
        raise ContractError(f"{label} must be a nonempty unique string list")
    return value


def load_contract(path: Path = CONTRACT_PATH) -> dict[str, Any]:
    try:
        _, data = _read_regular_nofollow(path)
        contract = tomllib.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ContractError(f"cannot load package contract: {error}") from error
    validate_contract(contract)
    return contract


def validate_contract(contract: Any) -> dict[str, Any]:
    value = _exact_keys(
        contract,
        {
            "schema",
            "clean_roots",
            "typescript_version",
            "typescript_upstream_revision",
            "profile_sha256",
            "profile_manifest_bytes",
            "profile_manifest_sha256",
            "profile_root_name",
            "profile_root_sha256",
            "profile_raw_concat_sha256",
            "profile_source_bytes",
            "profile_source_lf",
            "profile_source_cr",
            "profile_script_sources",
            "profile_external_module_sources",
            "profile_reference_edges",
            "dts_sources",
            "licenses",
            "license_bytes",
            "license_sha256",
            "third_party_notice_bytes",
            "third_party_notice_sha256",
            "gitattributes_bytes",
            "gitattributes_sha256",
            "profile_readme_bytes",
            "profile_readme_sha256",
            "profile_notice_bytes",
            "profile_notice_sha256",
            "required_package_assets",
            "cargo_checks",
            "build_scripts",
            "source_mutations",
        },
        "package contract",
    )
    expected_integers = {
        "schema": 1,
        "clean_roots": 2,
        "dts_sources": 82,
        "profile_manifest_bytes": 36_558,
        "profile_source_bytes": 2_936_611,
        "profile_source_lf": 58_349,
        "profile_source_cr": 0,
        "profile_script_sources": 81,
        "profile_external_module_sources": 1,
        "profile_reference_edges": 110,
        "license_bytes": 9_197,
        "third_party_notice_bytes": 37_824,
        "gitattributes_bytes": 165,
        "profile_readme_bytes": 1_127,
        "profile_notice_bytes": 1_036,
        "cargo_checks": 2,
        "build_scripts": 0,
        "source_mutations": 0,
    }
    for key, expected in expected_integers.items():
        if _integer(value[key], f"contract {key}") != expected:
            raise ContractError(f"contract {key} differs from {expected}")
    if value["typescript_version"] != "6.0.3":
        raise ContractError("contract TypeScript version differs")
    expected_strings = {
        "typescript_upstream_revision": "050880ce59e30b356b686bd3144efe24f875ebc8",
        "profile_root_name": "lib.es2025.full.d.ts",
    }
    for key, expected in expected_strings.items():
        if value[key] != expected:
            raise ContractError(f"contract {key} differs")
    expected_digests = {
        "profile_sha256": "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d",
        "profile_manifest_sha256": (
            "1edef1b5e870024834762267ec532c3054f3b2279e9181844e21648243eb1407"
        ),
        "profile_root_sha256": (
            "e03da518b01b46a4c99a1f88cd727ee98ddf14492c43dae1ae7a63e992971bab"
        ),
        "profile_raw_concat_sha256": (
            "0c68516cfe1dff30ce17425b2566813cf6d00c7f589dd24f31f4ba879b69a267"
        ),
        "license_sha256": "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47",
        "third_party_notice_sha256": (
            "1af3c68039c57e539422da82a4faada506ce6d0ea6f90e0b699d02dbcdb7a90c"
        ),
        "gitattributes_sha256": "3f9153093b34fc664e5b6e0670710f9aed8f64c5f17b8a2b79fba1ae8e17b9f9",
        "profile_readme_sha256": "7706999fba59de5c5ce251208315f7b59c42fc3fb92a777d1073f59d0ca1a09c",
        "profile_notice_sha256": "401062e3feb78e115fabc467683d92574b1a99d776ce8b4962bdf422cfe7a5cc",
    }
    for key, expected in expected_digests.items():
        if _digest(value[key], f"contract {key}") != expected:
            raise ContractError(f"contract {key} differs")
    if _string_list(value["licenses"], "contract licenses") != [
        "LICENSE.txt",
        "ThirdPartyNoticeText.txt",
    ]:
        raise ContractError("contract license inventory differs")
    required = _string_list(
        value["required_package_assets"], "contract required_package_assets"
    )
    if required != [
        PROFILE_PREFIX + ".gitattributes",
        PROFILE_PREFIX + "README.md",
        PROFILE_PREFIX + "profile.toml",
        PROFILE_PREFIX + "LICENSE.txt",
        PROFILE_PREFIX + "ThirdPartyNoticeText.txt",
        PROFILE_PREFIX + "THIRD_PARTY_NOTICE.md",
    ]:
        raise ContractError("contract required package asset inventory differs")
    return value


def validate_record(record: Any, contract: dict[str, Any]) -> dict[str, Any]:
    validate_contract(contract)
    value = _exact_keys(
        record,
        {
            "schema",
            "clean_roots",
            "profile_sha256",
            "dts_sources",
            "licenses",
            "cargo_checks",
            "build_scripts",
            "source_mutations",
        },
        "package record",
    )
    if _integer(value["schema"], "record schema") != 1:
        raise ContractError("record schema differs")
    roots = _string_list(value["clean_roots"], "record clean_roots")
    if len(roots) != contract["clean_roots"]:
        raise ContractError("record clean-root count differs")
    normalized_roots = []
    for root in roots:
        path = Path(root)
        if not path.is_absolute():
            raise ContractError("record clean roots must be absolute")
        normalized_roots.append(os.path.normpath(root))
    if len(set(normalized_roots)) != len(normalized_roots):
        raise ContractError("record clean roots must be distinct")
    for key in (
        "dts_sources",
        "cargo_checks",
        "build_scripts",
        "source_mutations",
    ):
        if _integer(value[key], f"record {key}") != contract[key]:
            raise ContractError(f"record {key} differs")
    if _digest(value["profile_sha256"], "record profile_sha256") != contract[
        "profile_sha256"
    ]:
        raise ContractError("record profile_sha256 differs")
    if _string_list(value["licenses"], "record licenses") != contract["licenses"]:
        raise ContractError("record license inventory differs")
    return value


def _safe_package_path(value: str) -> str:
    if not value or "\\" in value:
        raise ContractError(f"unsafe package path {value!r}")
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in ("", ".", "..") for part in path.parts):
        raise ContractError(f"unsafe package path {value!r}")
    return value


def validate_package_inventory(
    inventory: Iterable[str],
    contract: dict[str, Any],
    *,
    expected_profile_assets: set[str] | None = None,
) -> set[str]:
    validate_contract(contract)
    if isinstance(inventory, (str, bytes)):
        raise ContractError("package inventory must be a path collection")
    paths = list(inventory)
    if any(not isinstance(path, str) for path in paths) or len(paths) != len(set(paths)):
        raise ContractError("package inventory contains a non-string or duplicate path")
    normalized = {_safe_package_path(path) for path in paths}
    build_scripts = sorted(
        path for path in normalized if PurePosixPath(path).name == "build.rs"
    )
    if build_scripts:
        raise ContractError(f"package contains build scripts: {build_scripts!r}")
    expected = (
        expected_profile_assets
        if expected_profile_assets is not None
        else _profile_asset_inventory(PACKAGE_ROOT / PROFILE_PREFIX, contract)
    )
    missing = sorted(expected - normalized)
    if missing:
        raise ContractError(f"package is missing required library assets: {missing!r}")
    profile_entries = {path for path in normalized if path.startswith(PROFILE_PREFIX)}
    unexpected = sorted(profile_entries - expected)
    if unexpected:
        raise ContractError(f"package contains unexpected library assets: {unexpected!r}")
    return normalized


def _stat_identity(info: os.stat_result) -> tuple[int, int, int, int, int, int, int]:
    return (
        info.st_dev,
        info.st_ino,
        info.st_mode,
        info.st_nlink,
        info.st_size,
        info.st_mtime_ns,
        info.st_ctime_ns,
    )


def _read_regular_nofollow(path: Path) -> tuple[os.stat_result, bytes]:
    try:
        before = path.lstat()
    except OSError as error:
        raise ContractError(f"cannot inspect {path}: {error}") from error
    if not stat.S_ISREG(before.st_mode):
        raise ContractError(f"expected a real regular file: {path}")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise ContractError(
            f"cannot open regular file without following links: {path}: {error}"
        ) from error
    try:
        opened_before = os.fstat(descriptor)
        if _stat_identity(opened_before) != _stat_identity(before):
            raise ContractError(f"file changed while opening: {path}")
        chunks: list[bytes] = []
        while chunk := os.read(descriptor, 1024 * 1024):
            chunks.append(chunk)
        opened_after = os.fstat(descriptor)
        if _stat_identity(opened_after) != _stat_identity(opened_before):
            raise ContractError(f"file changed while reading: {path}")
    finally:
        os.close(descriptor)
    try:
        after = path.lstat()
    except OSError as error:
        raise ContractError(f"file disappeared after reading: {path}: {error}") from error
    if _stat_identity(after) != _stat_identity(before):
        raise ContractError(f"file changed after reading: {path}")
    return before, b"".join(chunks)


def _file_identity(path: Path) -> tuple[int, str]:
    info, data = _read_regular_nofollow(path)
    return info.st_size, hashlib.sha256(data).hexdigest()


def _directory_identity(path: Path) -> os.stat_result:
    try:
        before = path.lstat()
    except OSError as error:
        raise ContractError(f"cannot inspect directory {path}: {error}") from error
    if not stat.S_ISDIR(before.st_mode):
        raise ContractError(f"expected a real directory: {path}")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    flags |= getattr(os, "O_DIRECTORY", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise ContractError(
            f"cannot open directory without following links: {path}: {error}"
        ) from error
    try:
        opened_before = os.fstat(descriptor)
        opened_after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    try:
        after = path.lstat()
    except OSError as error:
        raise ContractError(f"directory disappeared after opening {path}: {error}") from error
    if not (
        _stat_identity(opened_before)
        == _stat_identity(opened_after)
        == _stat_identity(before)
        == _stat_identity(after)
    ):
        raise ContractError(f"directory changed while opening: {path}")
    return before


def _tree_snapshot(root: Path) -> dict[str, tuple[str, int, int, str]]:
    root_before = _directory_identity(root)
    snapshot: dict[str, tuple[str, int, int, str]] = {}

    def visit(directory: Path) -> None:
        try:
            entries = list(os.scandir(directory))
        except OSError as error:
            raise ContractError(f"cannot scan source directory {directory}: {error}") from error
        for entry in sorted(entries, key=lambda item: item.name):
            path = Path(entry.path)
            relative = path.relative_to(root).as_posix()
            _safe_package_path(relative)
            try:
                scanned = entry.stat(follow_symlinks=False)
                direct = path.lstat()
            except OSError as error:
                raise ContractError(f"cannot inspect source entry {path}: {error}") from error
            if _stat_identity(scanned) != _stat_identity(direct):
                raise ContractError(f"source entry changed while scanning: {path}")
            mode = stat.S_IMODE(direct.st_mode)
            if stat.S_ISDIR(direct.st_mode):
                before_directory = _directory_identity(path)
                snapshot[relative] = ("directory", mode, 0, "")
                visit(path)
                after_directory = _directory_identity(path)
                if _stat_identity(before_directory) != _stat_identity(
                    after_directory
                ):
                    raise ContractError(f"source directory changed while scanning: {path}")
            elif stat.S_ISREG(direct.st_mode):
                info, data = _read_regular_nofollow(path)
                snapshot[relative] = (
                    "file",
                    stat.S_IMODE(info.st_mode),
                    info.st_size,
                    hashlib.sha256(data).hexdigest(),
                )
            elif stat.S_ISLNK(direct.st_mode):
                raise ContractError(f"source tree contains a symlink: {path}")
            else:
                raise ContractError(f"source tree contains a special entry: {path}")

    visit(root)
    root_after = _directory_identity(root)
    if _stat_identity(root_before) != _stat_identity(root_after):
        raise ContractError(f"source root changed while scanning: {root}")
    return snapshot


def _snapshot_mutations(
    before: dict[str, tuple[str, int, int, str]],
    after: dict[str, tuple[str, int, int, str]],
) -> list[str]:
    return sorted(
        path
        for path in before.keys() | after.keys()
        if before.get(path) != after.get(path)
    )


def _line_facts(data: bytes) -> tuple[int, int, bool]:
    return data.count(b"\n"), data.count(b"\r"), data.endswith(b"\n")


def _parse_toml_nofollow(path: Path, label: str) -> tuple[bytes, dict[str, Any]]:
    _, data = _read_regular_nofollow(path)
    try:
        text = data.decode("utf-8")
        value = tomllib.loads(text)
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ContractError(f"cannot parse {label}: {error}") from error
    if not isinstance(value, dict):
        raise ContractError(f"{label} is not a TOML table")
    return data, value


def _exact_text_asset(
    path: Path,
    *,
    expected_size: int,
    expected_digest: str,
    expected_lf: int,
    expected_cr: int,
    expected_final_lf: bool,
    label: str,
) -> bytes:
    info, data = _read_regular_nofollow(path)
    actual = (
        info.st_size,
        hashlib.sha256(data).hexdigest(),
        *_line_facts(data),
    )
    expected = (
        expected_size,
        expected_digest,
        expected_lf,
        expected_cr,
        expected_final_lf,
    )
    if actual != expected:
        raise ContractError(f"{label} byte or line identity differs")
    return data


def _validate_notice(
    row: Any,
    *,
    label: str,
    name: str,
    git_blob: str,
    expected_size: int,
    expected_digest: str,
    expected_lf: int,
    expected_cr: int,
) -> None:
    value = _exact_keys(row, PROFILE_NOTICE_KEYS, f"profile {label}")
    expected = {
        "name": name,
        "npm_path": name,
        "git_path": name,
        "git_blob": git_blob,
        "sha256": expected_digest,
        "bytes": expected_size,
        "lf": expected_lf,
        "cr": expected_cr,
        "final_lf": True,
    }
    if value != expected:
        raise ContractError(f"profile {label} facts differ")


def _profile_asset_inventory(profile_dir: Path, contract: dict[str, Any]) -> set[str]:
    validate_contract(contract)
    manifest_data, manifest = _parse_toml_nofollow(
        profile_dir / "profile.toml", "library profile manifest"
    )
    if (
        len(manifest_data) != contract["profile_manifest_bytes"]
        or hashlib.sha256(manifest_data).hexdigest()
        != contract["profile_manifest_sha256"]
        or b"\r" in manifest_data
        or not manifest_data.endswith(b"\n")
    ):
        raise ContractError("library profile manifest byte identity differs")
    value = _exact_keys(manifest, PROFILE_ROOT_KEYS, "library profile manifest")
    scalar_expected = {
        "schema": 1,
        "typescript_version": contract["typescript_version"],
        "upstream_revision": contract["typescript_upstream_revision"],
        "root": contract["profile_root_name"],
        "file_count": contract["dts_sources"],
        "script_file_count": contract["profile_script_sources"],
        "external_module_file_count": contract["profile_external_module_sources"],
        "reference_edge_count": contract["profile_reference_edges"],
        "source_bytes": contract["profile_source_bytes"],
        "source_lf": contract["profile_source_lf"],
        "source_cr": contract["profile_source_cr"],
        "root_sha256": contract["profile_root_sha256"],
        "raw_concat_sha256": contract["profile_raw_concat_sha256"],
        "length_framed_sha256": contract["profile_sha256"],
        "length_frame": "u64be(name_len) || u64be(source_len) || name_utf8 || source",
        "order": "getDefaultLibFilePriority/libEntries first occurrence; non-entry root last",
        "lib_entries_count": 107,
        "lib_entries_filename_count": 95,
        "lib_entries_framed_sha256": (
            "7e237445dc1c4c7f32b6e829da48858fb3eafb0d0b3f3d9f5fe031d5b7a6d6f6"
        ),
    }
    for key, expected in scalar_expected.items():
        actual = value[key]
        if isinstance(expected, int):
            actual = _integer(actual, f"profile {key}")
        elif not isinstance(actual, str):
            raise ContractError(f"profile {key} must be a string")
        if actual != expected:
            raise ContractError(f"profile {key} differs")
    _validate_notice(
        value["license"],
        label="license",
        name="LICENSE.txt",
        git_blob="8746124b277914d0f0fd9cf4aef2ed3b587143d9",
        expected_size=contract["license_bytes"],
        expected_digest=contract["license_sha256"],
        expected_lf=55,
        expected_cr=55,
    )
    _validate_notice(
        value["third_party_notice"],
        label="third-party notice",
        name="ThirdPartyNoticeText.txt",
        git_blob="a857fb3ce77c3b43c145f94aa8d910c7791394a5",
        expected_size=contract["third_party_notice_bytes"],
        expected_digest=contract["third_party_notice_sha256"],
        expected_lf=193,
        expected_cr=193,
    )
    _exact_text_asset(
        profile_dir / "LICENSE.txt",
        expected_size=contract["license_bytes"],
        expected_digest=contract["license_sha256"],
        expected_lf=55,
        expected_cr=55,
        expected_final_lf=True,
        label="profile license",
    )
    _exact_text_asset(
        profile_dir / "ThirdPartyNoticeText.txt",
        expected_size=contract["third_party_notice_bytes"],
        expected_digest=contract["third_party_notice_sha256"],
        expected_lf=193,
        expected_cr=193,
        expected_final_lf=True,
        label="profile third-party notice",
    )
    for name, size_key, digest_key, line_facts in (
        (".gitattributes", "gitattributes_bytes", "gitattributes_sha256", (6, 0, True)),
        ("README.md", "profile_readme_bytes", "profile_readme_sha256", (19, 0, True)),
        ("THIRD_PARTY_NOTICE.md", "profile_notice_bytes", "profile_notice_sha256", (19, 0, True)),
    ):
        _exact_text_asset(
            profile_dir / name,
            expected_size=contract[size_key],
            expected_digest=contract[digest_key],
            expected_lf=line_facts[0],
            expected_cr=line_facts[1],
            expected_final_lf=line_facts[2],
            label=f"profile {name}",
        )
    rows = value["file"]
    if not isinstance(rows, list) or len(rows) != contract["dts_sources"]:
        raise ContractError("profile source registry count differs")
    names: list[str] = []
    npm_paths: set[str] = set()
    git_paths: set[str] = set()
    graph: dict[str, list[str]] = {}
    raw = hashlib.sha256()
    framed = hashlib.sha256()
    total_bytes = 0
    total_lf = 0
    total_cr = 0
    script_count = 0
    external_count = 0
    reference_count = 0
    reference_pattern = re.compile(rb'^/// <reference lib="([^"]+)" />$', re.MULTILINE)
    for ordinal, raw_row in enumerate(rows):
        row = _exact_keys(raw_row, PROFILE_FILE_KEYS, f"profile file {ordinal}")
        if _integer(row["ordinal"], f"profile file {ordinal} ordinal") != ordinal:
            raise ContractError("profile source ordinals differ from array order")
        name = _string(row["name"], f"profile file {ordinal} name")
        if (
            not name.startswith("lib.")
            or not name.endswith(".d.ts")
            or PurePosixPath(name).name != name
            or name in names
        ):
            raise ContractError(f"profile contains invalid or duplicate source {name!r}")
        npm_path = _string(row["npm_path"], f"profile source {name} npm_path")
        git_path = _string(row["git_path"], f"profile source {name} git_path")
        if npm_path != f"lib/{name}" or npm_path in npm_paths:
            raise ContractError(f"profile npm path differs or duplicates for {name}")
        if git_path != f"lib/{name}" or git_path in git_paths:
            raise ContractError(f"profile git path differs or duplicates for {name}")
        blob = _string(row["git_blob"], f"profile source {name} git_blob")
        if len(blob) != 40 or any(character not in HEX for character in blob):
            raise ContractError(f"profile git blob is invalid for {name}")
        expected_digest = _digest(row["sha256"], f"profile source {name} sha256")
        expected_size = _integer(row["bytes"], f"profile source {name} bytes")
        expected_lf = _integer(row["lf"], f"profile source {name} lf")
        expected_cr = _integer(row["cr"], f"profile source {name} cr")
        if not isinstance(row["final_lf"], bool):
            raise ContractError(f"profile source {name} final_lf must be a boolean")
        _, data = _read_regular_nofollow(profile_dir / "lib" / name)
        if (
            len(data),
            hashlib.sha256(data).hexdigest(),
            *_line_facts(data),
        ) != (expected_size, expected_digest, expected_lf, expected_cr, row["final_lf"]):
            raise ContractError(f"profile source facts differ for {name}")
        references = row["references"]
        if not isinstance(references, list):
            raise ContractError(f"profile references must be an array for {name}")
        normalized_references: list[tuple[str, str]] = []
        for raw_reference in references:
            reference = _exact_keys(
                raw_reference, PROFILE_REFERENCE_KEYS, f"profile reference in {name}"
            )
            logical = _string(reference["lib"], f"profile reference lib in {name}")
            target = _string(reference["file"], f"profile reference file in {name}")
            if target != f"lib.{logical}.d.ts":
                raise ContractError(f"profile reference resolution differs in {name}")
            normalized_references.append((logical, target))
        if len(normalized_references) != len(set(normalized_references)):
            raise ContractError(f"profile contains duplicate references in {name}")
        source_references = [
            logical.decode("ascii") for logical in reference_pattern.findall(data)
        ]
        if source_references != [logical for logical, _ in normalized_references]:
            raise ContractError(f"profile source reference facts differ for {name}")
        source_kind = _string(row["source_kind"], f"profile source kind for {name}")
        inferred_kind = "external-module" if b"\nexport {};\n" in data else "script"
        if source_kind != inferred_kind:
            raise ContractError(f"profile source kind differs for {name}")
        script_count += source_kind == "script"
        external_count += source_kind == "external-module"
        reference_count += len(normalized_references)
        raw.update(data)
        encoded = name.encode("utf-8")
        framed.update(len(encoded).to_bytes(8, "big"))
        framed.update(len(data).to_bytes(8, "big"))
        framed.update(encoded)
        framed.update(data)
        total_bytes += len(data)
        total_lf += data.count(b"\n")
        total_cr += data.count(b"\r")
        names.append(name)
        npm_paths.add(npm_path)
        git_paths.add(git_path)
        graph[name] = [target for _, target in normalized_references]
    if any(target not in graph for targets in graph.values() for target in targets):
        raise ContractError("profile reference graph contains an unknown target")
    aggregate = (
        len(names),
        script_count,
        external_count,
        reference_count,
        total_bytes,
        total_lf,
        total_cr,
        raw.hexdigest(),
        framed.hexdigest(),
        names[-1],
        _file_identity(profile_dir / "lib" / names[-1])[1],
    )
    expected_aggregate = (
        contract["dts_sources"],
        contract["profile_script_sources"],
        contract["profile_external_module_sources"],
        contract["profile_reference_edges"],
        contract["profile_source_bytes"],
        contract["profile_source_lf"],
        contract["profile_source_cr"],
        contract["profile_raw_concat_sha256"],
        contract["profile_sha256"],
        contract["profile_root_name"],
        contract["profile_root_sha256"],
    )
    if aggregate != expected_aggregate:
        raise ContractError("profile source aggregate facts differ")

    visiting: set[str] = set()
    reachable: set[str] = set()

    def visit(name: str) -> None:
        if name in visiting:
            raise ContractError("profile reference graph contains a cycle")
        if name in reachable:
            return
        visiting.add(name)
        for target in graph[name]:
            visit(target)
        visiting.remove(name)
        reachable.add(name)

    visit(contract["profile_root_name"])
    if reachable != set(names):
        raise ContractError("profile root does not reach the complete source closure")

    relative_assets = {
        relative.removeprefix(PROFILE_PREFIX)
        for relative in contract["required_package_assets"]
    }
    relative_assets.update(f"lib/{name}" for name in names)
    snapshot = _tree_snapshot(profile_dir)
    expected_tree = {"lib", *relative_assets}
    if set(snapshot) != expected_tree:
        missing = sorted(expected_tree - set(snapshot))
        extra = sorted(set(snapshot) - expected_tree)
        raise ContractError(
            f"profile subtree inventory differs: missing={missing!r} extra={extra!r}"
        )
    return {PROFILE_PREFIX + relative for relative in relative_assets}


def _run(
    command: list[str],
    *,
    cwd: Path,
    environment: dict[str, str] | None = None,
    timeout: int = 1_200,
) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            env=environment,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ContractError(f"command failed to execute: {command!r}: {error}") from error
    if result.returncode != 0:
        raise ContractError(
            f"command failed ({result.returncode}): {command!r}\n"
            f"stdout:\n{result.stdout[-8192:]}\nstderr:\n{result.stderr[-8192:]}"
        )
    return result


def _reject_ancestor_cargo_configs(path: Path) -> None:
    cursor = path.resolve().parent
    while True:
        for name in ("config", "config.toml"):
            candidate = cursor / ".cargo" / name
            if candidate.exists() or candidate.is_symlink():
                raise ContractError(f"untrusted ancestor Cargo config: {candidate}")
        if cursor.parent == cursor:
            return
        cursor = cursor.parent


def _prepare_cargo_home(destination: Path) -> Path:
    destination.mkdir()
    host = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()
    for relative in ("registry/cache", "registry/index"):
        source = host / relative
        if source.is_dir() and not source.is_symlink():
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.symlink_to(source, target_is_directory=True)
    for name in (".package-cache", ".package-cache-mutate"):
        (destination / name).touch()
    return destination


def _clean_environment(target: Path, cargo_home: Path) -> dict[str, str]:
    allowed = {
        "HOME",
        "PATH",
        "RUSTUP_HOME",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "TERM",
        "TMPDIR",
    }
    environment = {key: value for key, value in os.environ.items() if key in allowed}
    environment.update(
        {
            "CARGO_INCREMENTAL": "0",
            "CARGO_HOME": str(cargo_home.resolve()),
            "CARGO_NET_OFFLINE": "true",
            "CARGO_TARGET_DIR": str(target.resolve()),
        }
    )
    return environment


def _git(root: Path, arguments: list[str]) -> str:
    environment = dict(os.environ)
    environment["GIT_OPTIONAL_LOCKS"] = "0"
    return _run(
        ["git", "--no-optional-locks", *arguments],
        cwd=root,
        environment=environment,
        timeout=120,
    ).stdout


def _tracked_identity(root: Path) -> str:
    names = _git(root, ["ls-files", "-z", "--cached"]).split("\0")
    inventory = [name for name in names if name]
    if (
        not inventory
        or inventory != sorted(inventory)
        or len(inventory) != len(set(inventory))
    ):
        raise ContractError("tracked source inventory is empty, duplicated, or reordered")
    digest = hashlib.sha256()
    for name in inventory:
        _safe_package_path(name)
        path = root / name
        size, identity = _file_identity(path)
        encoded = name.encode("utf-8")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
        digest.update(size.to_bytes(8, "big"))
        digest.update(bytes.fromhex(identity))
    return digest.hexdigest()


def _git_status_all(root: Path) -> str:
    return _git(
        root,
        [
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
        ],
    )


def _require_git_clean_all(root: Path, label: str) -> None:
    status = _git_status_all(root)
    if status:
        raise ContractError(f"{label} is not clean, including ignored files:\n{status}")


def _run_observed(
    command: list[str],
    *,
    cwd: Path,
    environment: dict[str, str],
    source_root: Path,
    observations: dict[str, int],
    require_git_clean: bool,
    additional_source_roots: tuple[Path, ...] = (),
    timeout: int = 1_200,
) -> subprocess.CompletedProcess[str]:
    if require_git_clean:
        _require_git_clean_all(source_root, "subprocess source root")
    source_roots = (source_root, *additional_source_roots)
    before = {root: _tree_snapshot(root) for root in source_roots}
    process_error: ContractError | None = None
    result: subprocess.CompletedProcess[str] | None = None
    try:
        result = _run(
            command,
            cwd=cwd,
            environment=environment,
            timeout=timeout,
        )
    except ContractError as error:
        process_error = error
    after = {root: _tree_snapshot(root) for root in source_roots}
    mutations = [
        f"{root}:{path}"
        for root in source_roots
        for path in _snapshot_mutations(before[root], after[root])
    ]
    observations["source_mutations"] += len(mutations)
    if require_git_clean:
        _require_git_clean_all(source_root, "subprocess source root after command")
    if mutations:
        raise ContractError(
            f"subprocess mutated source entries: {mutations!r}"
        ) from process_error
    if process_error is not None:
        raise process_error
    if result is None:
        raise ContractError("subprocess returned no result")
    return result


def _validate_manifest_build_surface(manifest_path: Path) -> int:
    _, manifest = _parse_toml_nofollow(manifest_path, f"Cargo manifest {manifest_path}")
    package = manifest.get("package")
    if not isinstance(package, dict):
        raise ContractError(f"Cargo manifest lacks a package table: {manifest_path}")
    build = package.get("build", False)
    if build is not False:
        raise ContractError(
            f"Cargo manifest enables a custom build target: {manifest_path}"
        )
    return 0


def _validate_cargo_metadata(value: Any, expected_manifest: Path) -> int:
    metadata = _exact_keys(
        value,
        {
            "packages",
            "workspace_members",
            "workspace_default_members",
            "resolve",
            "target_directory",
            "build_directory",
            "version",
            "workspace_root",
            "metadata",
        },
        "cargo metadata",
    )
    packages = metadata["packages"]
    if not isinstance(packages, list):
        raise ContractError("cargo metadata packages must be an array")
    expected = expected_manifest.resolve()
    selected = []
    for package in packages:
        if not isinstance(package, dict):
            raise ContractError("cargo metadata contains an invalid package")
        manifest_path = package.get("manifest_path")
        if isinstance(manifest_path, str) and Path(manifest_path).resolve() == expected:
            selected.append(package)
    if len(selected) != 1:
        raise ContractError("cargo metadata did not identify exactly one target package")
    targets = selected[0].get("targets")
    if not isinstance(targets, list):
        raise ContractError("cargo metadata package targets must be an array")
    custom_builds = 0
    for target in targets:
        if not isinstance(target, dict) or not isinstance(target.get("kind"), list):
            raise ContractError("cargo metadata contains an invalid target")
        kinds = target["kind"]
        if any(not isinstance(kind, str) for kind in kinds):
            raise ContractError("cargo metadata target kinds must be strings")
        custom_builds += "custom-build" in kinds
    if custom_builds:
        raise ContractError("cargo metadata exposes a custom-build target")
    return custom_builds


def _cargo_metadata(
    root: Path,
    environment: dict[str, str],
    observations: dict[str, int],
    *,
    require_git_clean: bool,
    cargo_config: tuple[str, ...] = (),
    additional_source_roots: tuple[Path, ...] = (),
) -> int:
    result = _run_observed(
        [
            "cargo",
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--locked",
            "--offline",
            *cargo_config,
        ],
        cwd=root,
        environment=environment,
        source_root=root,
        observations=observations,
        require_git_clean=require_git_clean,
        additional_source_roots=additional_source_roots,
    )
    try:
        metadata = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ContractError(f"cargo metadata returned invalid JSON: {error}") from error
    custom_builds = _validate_cargo_metadata(metadata, root / "Cargo.toml")
    observations["build_scripts"] += custom_builds
    return custom_builds


def _require_clean_repository() -> str:
    status = _git(ROOT, ["status", "--porcelain=v1", "--untracked-files=all"])
    if status:
        raise ContractError("package verification requires a clean repository")
    return _git(ROOT, ["rev-parse", "HEAD"]).strip()


def _clone_clean_root(destination: Path, revision: str) -> None:
    _reject_ancestor_cargo_configs(destination)
    _run(
        ["git", "clone", "--quiet", "--local", "--no-hardlinks", str(ROOT), str(destination)],
        cwd=destination.parent,
        timeout=300,
    )
    _git(destination, ["checkout", "--quiet", "--detach", revision])
    _require_git_clean_all(destination, "isolated package root")


def _package_list(
    package_root: Path,
    source_root: Path,
    environment: dict[str, str],
    observations: dict[str, int],
    cargo_config: tuple[str, ...],
) -> set[str]:
    result = _run_observed(
        ["cargo", "package", "--locked", "--offline", "--list", *cargo_config],
        cwd=package_root,
        environment=environment,
        source_root=source_root,
        observations=observations,
        require_git_clean=True,
    )
    paths = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    if len(paths) != len(set(paths)):
        raise ContractError("cargo package --list emitted duplicate paths")
    return set(paths)


def _cargo_patch_config(repository_root: Path) -> tuple[str, ...]:
    arguments: list[str] = []
    for package in INTERNAL_PACKAGES:
        path = (repository_root / "crates" / package).resolve()
        arguments.extend(
            ["--config", f'patch.crates-io.{package}.path="{path}"']
        )
    return tuple(arguments)


def _extract_crate(
    archive: Path, destination: Path, expected_inventory: set[str]
) -> Path:
    destination.mkdir()
    archive_before = archive.lstat()
    if not stat.S_ISREG(archive_before.st_mode):
        raise ContractError("cargo package archive is not a real regular file")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(archive, flags)
    except OSError as error:
        raise ContractError(f"cannot open cargo package archive safely: {error}") from error
    try:
        opened_before = os.fstat(descriptor)
        if _stat_identity(opened_before) != _stat_identity(archive_before):
            raise ContractError("cargo package archive changed while opening")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            with tarfile.open(fileobj=stream, mode="r:gz") as package:
                members = package.getmembers()
                if not members:
                    raise ContractError("cargo package archive is empty")
                roots: set[str] = set()
                member_paths: set[str] = set()
                regular_paths: set[str] = set()
                for member in members:
                    path = PurePosixPath(member.name)
                    if path.is_absolute() or any(
                        part in ("", ".", "..") for part in path.parts
                    ):
                        raise ContractError(
                            f"unsafe cargo package member {member.name!r}"
                        )
                    normalized_member = path.as_posix()
                    if normalized_member in member_paths:
                        raise ContractError(
                            "cargo package archive contains duplicate paths"
                        )
                    member_paths.add(normalized_member)
                    roots.add(path.parts[0])
                    if not (member.isdir() or member.isfile()):
                        raise ContractError(
                            f"cargo package contains a non-file member {member.name!r}"
                        )
                    if member.isfile():
                        if len(path.parts) < 2:
                            raise ContractError("cargo package contains a root-level file")
                        relative = PurePosixPath(*path.parts[1:]).as_posix()
                        _safe_package_path(relative)
                        if relative in regular_paths:
                            raise ContractError(
                                "cargo package archive contains duplicate regular paths"
                            )
                        regular_paths.add(relative)
                if len(roots) != 1:
                    raise ContractError("cargo package archive has multiple roots")
                if regular_paths != expected_inventory:
                    missing = sorted(expected_inventory - regular_paths)
                    extra = sorted(regular_paths - expected_inventory)
                    raise ContractError(
                        "cargo package archive inventory differs from cargo package --list: "
                        f"missing={missing!r} extra={extra!r}"
                    )
                package.extractall(destination, filter="data")
        opened_after = os.fstat(descriptor)
        if _stat_identity(opened_after) != _stat_identity(opened_before):
            raise ContractError("cargo package archive changed while extracting")
    except (OSError, tarfile.TarError) as error:
        raise ContractError(f"cannot safely extract cargo package archive: {error}") from error
    finally:
        os.close(descriptor)
    archive_after = archive.lstat()
    if _stat_identity(archive_after) != _stat_identity(archive_before):
        raise ContractError("cargo package archive changed after extraction")
    root = destination / next(iter(roots))
    _directory_identity(root)
    snapshot = _tree_snapshot(root)
    expected_directories = {
        PurePosixPath(*PurePosixPath(path).parts[:index]).as_posix()
        for path in expected_inventory
        for index in range(1, len(PurePosixPath(path).parts))
    }
    if set(snapshot) != expected_inventory | expected_directories:
        raise ContractError("extracted cargo package tree differs from its archive inventory")
    return root


def _verify_extracted_assets(
    source_root: Path, extracted: Path, contract: dict[str, Any]
) -> None:
    source_assets = _profile_asset_inventory(source_root / PROFILE_PREFIX, contract)
    packaged_assets = _profile_asset_inventory(extracted / PROFILE_PREFIX, contract)
    if packaged_assets != source_assets:
        raise ContractError("packaged profile asset inventory differs from its source")
    for relative in sorted(source_assets):
        source = source_root / relative
        packaged = extracted / relative
        if _file_identity(source) != _file_identity(packaged):
            raise ContractError(f"packaged asset differs from its source: {relative}")


def _package_and_check(
    package_root: Path,
    repository_root: Path,
    run_root: Path,
    ordinal: int,
    cargo_home: Path,
    contract: dict[str, Any],
    observations: dict[str, int],
) -> None:
    target = run_root / f"package-target-{ordinal}"
    environment = _clean_environment(target, cargo_home)
    cargo_config = _cargo_patch_config(repository_root)
    expected_profile_assets = _profile_asset_inventory(
        package_root / PROFILE_PREFIX, contract
    )
    inventory = _package_list(
        package_root, repository_root, environment, observations, cargo_config
    )
    validate_package_inventory(
        inventory,
        contract,
        expected_profile_assets=expected_profile_assets,
    )
    observations["build_scripts"] += sum(
        PurePosixPath(path).name == "build.rs" for path in inventory
    )
    _run_observed(
        [
            "cargo",
            "package",
            "--locked",
            "--offline",
            "--no-verify",
            *cargo_config,
        ],
        cwd=package_root,
        environment=environment,
        source_root=repository_root,
        observations=observations,
        require_git_clean=True,
    )
    archives = list((target / "package").glob("typokat-library-*.crate"))
    if len(archives) != 1:
        raise ContractError(f"cargo package produced {len(archives)} crate archives")
    extracted = _extract_crate(
        archives[0], run_root / f"extracted-{ordinal}", inventory
    )
    _verify_extracted_assets(package_root, extracted, contract)
    observations["build_scripts"] += _validate_manifest_build_surface(
        extracted / "Cargo.toml"
    )
    if "Cargo.toml.orig" not in inventory:
        raise ContractError("cargo package omitted Cargo.toml.orig")
    observations["build_scripts"] += _validate_manifest_build_surface(
        extracted / "Cargo.toml.orig"
    )
    check_environment = _clean_environment(
        run_root / f"check-target-{ordinal}", cargo_home
    )
    _cargo_metadata(
        extracted,
        check_environment,
        observations,
        require_git_clean=False,
        cargo_config=cargo_config,
        additional_source_roots=(repository_root,),
    )
    _run_observed(
        [
            "cargo",
            "check",
            "--locked",
            "--offline",
            "--all-targets",
            *cargo_config,
        ],
        cwd=extracted,
        environment=check_environment,
        source_root=extracted,
        observations=observations,
        require_git_clean=False,
        additional_source_roots=(repository_root,),
    )
    observations["cargo_checks"] += 1


def _format_record(record: dict[str, Any]) -> str:
    return (
        "typokat-library-package-v1 "
        f"clean_roots={len(record['clean_roots'])} "
        f"profile_sha256={record['profile_sha256']} "
        f"dts_sources={record['dts_sources']} "
        f"licenses={len(record['licenses'])} "
        f"cargo_checks={record['cargo_checks']} "
        f"build_scripts={record['build_scripts']} "
        f"source_mutations={record['source_mutations']}"
    )


def coordinate(contract: dict[str, Any]) -> dict[str, Any]:
    revision = _require_clean_repository()
    with tempfile.TemporaryDirectory(prefix="typokat-library-package-") as temporary:
        run_root = Path(temporary)
        observations = {
            "cargo_checks": 0,
            "build_scripts": 0,
            "source_mutations": 0,
        }
        roots = [run_root / "clean-1", run_root / "clean-2"]
        for root in roots:
            _clone_clean_root(root, revision)
        repository_identity = _tracked_identity(ROOT)
        if any(_tracked_identity(root) != repository_identity for root in roots):
            raise ContractError("isolated clean roots differ from the committed source")
        cargo_homes = [
            _prepare_cargo_home(run_root / "cargo-home-1"),
            _prepare_cargo_home(run_root / "cargo-home-2"),
        ]
        for ordinal, (root, cargo_home) in enumerate(
            zip(roots, cargo_homes, strict=True), 1
        ):
            package_root = root / PACKAGE_RELATIVE_ROOT
            observations["build_scripts"] += _validate_manifest_build_surface(
                package_root / "Cargo.toml"
            )
            _profile_asset_inventory(package_root / PROFILE_PREFIX, contract)
            _cargo_metadata(
                package_root,
                _clean_environment(
                    run_root / f"metadata-target-{ordinal}", cargo_home
                ),
                observations,
                require_git_clean=True,
            )
        for ordinal, (root, cargo_home) in enumerate(
            zip(roots, cargo_homes, strict=True), 1
        ):
            _package_and_check(
                root / PACKAGE_RELATIVE_ROOT,
                root,
                run_root,
                ordinal,
                cargo_home,
                contract,
                observations,
            )
            _require_git_clean_all(root, "completed package-verification root")
        record = {
            "schema": 1,
            "clean_roots": [str(root.resolve()) for root in roots],
            "profile_sha256": contract["profile_sha256"],
            "dts_sources": contract["dts_sources"],
            "licenses": list(contract["licenses"]),
            "cargo_checks": observations["cargo_checks"],
            "build_scripts": observations["build_scripts"],
            "source_mutations": observations["source_mutations"],
        }
        validate_record(record, contract)
        return record


def main() -> int:
    try:
        contract = load_contract()
        record = coordinate(contract)
    except ContractError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(_format_record(record))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

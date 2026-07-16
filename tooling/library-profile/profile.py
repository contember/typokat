#!/usr/bin/env python3
"""Generate and verify typokat's pinned TypeScript default-library profile."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
import tomllib
from collections.abc import Callable, Iterable
from dataclasses import dataclass


TYPESCRIPT_VERSION = "6.0.3"
UPSTREAM_REVISION = "050880ce59e30b356b686bd3144efe24f875ebc8"
ROOT_FILE = "lib.es2025.full.d.ts"
EXPECTED_FILE_COUNT = 82
EXPECTED_REFERENCE_EDGE_COUNT = 110
EXPECTED_LIB_ENTRY_COUNT = 107
EXPECTED_LIB_ENTRY_FILENAME_COUNT = 95
EXPECTED_LIB_ENTRIES_FRAMED_SHA256 = (
    "7e237445dc1c4c7f32b6e829da48858fb3eafb0d0b3f3d9f5fe031d5b7a6d6f6"
)
EXPECTED_SOURCE_BYTES = 2_936_611
EXPECTED_SOURCE_LF = 58_349
EXPECTED_SOURCE_CR = 0
EXPECTED_ROOT_SHA256 = "e03da518b01b46a4c99a1f88cd727ee98ddf14492c43dae1ae7a63e992971bab"
EXPECTED_RAW_CONCAT_SHA256 = "0c68516cfe1dff30ce17425b2566813cf6d00c7f589dd24f31f4ba879b69a267"
EXPECTED_FRAMED_SHA256 = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d"
EXPECTED_ROOT_GIT_BLOB = "0870bb4f53d7b42022c46324b6fb001660283c86"
EXPECTED_EXTERNAL_MODULES = frozenset({"lib.es2025.iterator.d.ts"})

LICENSE_NAME = "LICENSE.txt"
LICENSE_BYTES = 9_197
LICENSE_SHA256 = "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47"
LICENSE_GIT_BLOB = "8746124b277914d0f0fd9cf4aef2ed3b587143d9"
NOTICE_NAME = "ThirdPartyNoticeText.txt"
NOTICE_BYTES = 37_824
NOTICE_SHA256 = "1af3c68039c57e539422da82a4faada506ce6d0ea6f90e0b699d02dbcdb7a90c"
NOTICE_GIT_BLOB = "a857fb3ce77c3b43c145f94aa8d910c7791394a5"

TEMPLATE_NAMES = (".gitattributes", "README.md", "THIRD_PARTY_NOTICE.md")
BOOTSTRAP_FILES = (
    ("bin/tsc", "100755"),
    ("lib/tsc.js", "100644"),
    ("lib/_tsc.js", "100644"),
    ("lib/typescript.js", "100644"),
)
SAFE_LIB_FILE_RE = re.compile(r"^lib\.[a-z0-9.]+\.d\.ts$")
SAFE_LIB_NAME_RE = re.compile(r"^[a-z0-9.]+$")
REFERENCE_LINE_RE = re.compile(
    rb"^[ \t]*///[ \t]*<reference[ \t]+lib=(['\"])([a-z0-9.]+)\1[ \t]*/>[ \t]*$"
)
REFERENCE_LIB_LIKE_RE = re.compile(rb"<reference\b[^\r\n>]*[ \t]lib\b")
LIB_ENTRY_RE = re.compile(r'\[\s*"([a-z0-9.]+)"\s*,\s*"(lib\.[a-z0-9.]+\.d\.ts)"\s*\]')
EXTERNAL_MODULE_LINE_RE = re.compile(rb"^(?:export|import)[ \t]", re.MULTILINE)

PROFILE_KEYS = frozenset({
    "schema", "typescript_version", "upstream_revision", "root", "file_count",
    "script_file_count", "external_module_file_count", "reference_edge_count",
    "source_bytes", "source_lf", "source_cr", "root_sha256", "raw_concat_sha256",
    "length_framed_sha256", "length_frame", "order", "lib_entries_count",
    "lib_entries_filename_count", "lib_entries_framed_sha256", "license",
    "third_party_notice", "file",
})
AUXILIARY_KEYS = frozenset({
    "name", "npm_path", "git_path", "git_blob", "sha256", "bytes", "lf", "cr",
    "final_lf",
})
FILE_KEYS = frozenset({
    "ordinal", "name", "npm_path", "git_path", "git_blob", "sha256", "bytes", "lf",
    "cr", "final_lf", "source_kind", "references",
})
REFERENCE_KEYS = frozenset({"lib", "file"})


class ProfileError(RuntimeError):
    """Expected profile input or verification failure."""


@dataclass(frozen=True)
class ReferenceEdge:
    lib: str
    file: str


@dataclass(frozen=True)
class FileRecord:
    ordinal: int
    name: str
    npm_path: str
    git_path: str
    git_blob: str
    sha256: str
    bytes: int
    lf: int
    cr: int
    final_lf: bool
    source_kind: str
    references: tuple[ReferenceEdge, ...]


@dataclass(frozen=True)
class AuxiliaryRecord:
    name: str
    npm_path: str
    git_path: str
    git_blob: str
    sha256: str
    bytes: int
    lf: int
    cr: int
    final_lf: bool


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def toml_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def validate_lib_file_name(name: str) -> None:
    if not SAFE_LIB_FILE_RE.fullmatch(name) or Path(name).name != name:
        raise ProfileError(f"unsafe library filename: {name!r}")


def validate_lib_name(name: str) -> None:
    if not SAFE_LIB_NAME_RE.fullmatch(name):
        raise ProfileError(f"unsafe reference-lib name: {name!r}")


def require_regular_file(path: Path, description: str) -> None:
    try:
        mode = path.lstat().st_mode
    except OSError as error:
        raise ProfileError(f"cannot inspect {description} {path}: {error}") from error
    if not stat.S_ISREG(mode):
        raise ProfileError(f"{description} is not a regular non-symlink file: {path}")


def require_real_directory(path: Path, description: str) -> None:
    try:
        mode = path.lstat().st_mode
    except OSError as error:
        raise ProfileError(f"cannot inspect {description} {path}: {error}") from error
    if not stat.S_ISDIR(mode):
        raise ProfileError(f"{description} is not a real directory: {path}")


def parse_reference_libs(name: str, source: bytes) -> tuple[str, ...]:
    """Return exact ordered reference-lib names and reject malformed/duplicate directives."""
    validate_lib_file_name(name)
    references: list[str] = []
    for line_number, line in enumerate(source.splitlines(), 1):
        if REFERENCE_LIB_LIKE_RE.search(line) is None:
            continue
        match = REFERENCE_LINE_RE.fullmatch(line)
        if match is None:
            raise ProfileError(f"{name}:{line_number}: malformed reference-lib directive")
        lib = match.group(2).decode("ascii")
        validate_lib_name(lib)
        if lib in references:
            raise ProfileError(f"{name}:{line_number}: duplicate reference-lib edge {lib!r}")
        references.append(lib)
    return tuple(references)


def parse_lib_entries(typescript_js: bytes) -> tuple[tuple[str, str], ...]:
    """Strictly extract the ordered libEntries table used by getDefaultLibFilePriority."""
    try:
        text = typescript_js.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ProfileError(f"lib/typescript.js is not UTF-8: {error}") from error
    start_marker = "var libEntries = ["
    end_marker = "];\nvar libs = libEntries.map"
    start = text.find(start_marker)
    if start < 0:
        raise ProfileError("lib/typescript.js has no libEntries table")
    body_start = start + len(start_marker)
    end = text.find(end_marker, body_start)
    if end < 0 or text.find(start_marker, body_start) >= 0:
        raise ProfileError("lib/typescript.js has an ambiguous libEntries table")
    body = text[body_start:end]
    uncommented = re.sub(r"//[^\n]*", "", body)
    entries = tuple(
        (match.group(1), match.group(2)) for match in LIB_ENTRY_RE.finditer(uncommented)
    )
    residue = LIB_ENTRY_RE.sub("", uncommented)
    residue = re.sub(r"[\s,]", "", residue)
    if residue:
        raise ProfileError(f"unparsed libEntries syntax: {residue[:80]!r}")
    if not entries:
        raise ProfileError("libEntries is empty")
    seen_names: set[str] = set()
    for lib, filename in entries:
        validate_lib_name(lib)
        validate_lib_file_name(filename)
        if lib in seen_names:
            raise ProfileError(f"duplicate libEntries name: {lib!r}")
        seen_names.add(lib)
    return entries


def validate_lib_entries_fingerprint(
    entries: tuple[tuple[str, str], ...],
) -> str:
    digest = framed_registry_digest(
        (lib, filename.encode("utf-8")) for lib, filename in entries
    )
    actual = (len(entries), len({filename for _, filename in entries}), digest)
    expected = (
        EXPECTED_LIB_ENTRY_COUNT,
        EXPECTED_LIB_ENTRY_FILENAME_COUNT,
        EXPECTED_LIB_ENTRIES_FRAMED_SHA256,
    )
    if actual != expected:
        raise ProfileError(
            f"pinned libEntries fingerprint drift: actual={actual!r}, expected={expected!r}"
        )
    return digest


def resolve_closure(
    root: str,
    lib_map: dict[str, str],
    load: Callable[[str], bytes],
) -> tuple[dict[str, bytes], dict[str, tuple[ReferenceEdge, ...]]]:
    """Resolve the recursive reference-lib graph, rejecting unknown, missing, and cyclic edges."""
    validate_lib_file_name(root)
    sources: dict[str, bytes] = {}
    graph: dict[str, tuple[ReferenceEdge, ...]] = {}
    visiting: list[str] = []

    def visit(filename: str) -> None:
        validate_lib_file_name(filename)
        if filename in visiting:
            cycle = visiting[visiting.index(filename) :] + [filename]
            raise ProfileError(f"cyclic reference-lib graph: {' -> '.join(cycle)}")
        if filename in sources:
            return
        visiting.append(filename)
        try:
            try:
                source = load(filename)
            except (FileNotFoundError, OSError) as error:
                raise ProfileError(f"missing referenced library file {filename!r}: {error}") from error
            refs: list[ReferenceEdge] = []
            target_files: set[str] = set()
            for lib in parse_reference_libs(filename, source):
                target = lib_map.get(lib)
                if target is None:
                    raise ProfileError(f"{filename}: unknown reference-lib name {lib!r}")
                validate_lib_file_name(target)
                if target in target_files:
                    raise ProfileError(
                        f"{filename}: duplicate reference-lib target {target!r} through aliases"
                    )
                target_files.add(target)
                refs.append(ReferenceEdge(lib=lib, file=target))
                visit(target)
            sources[filename] = source
            graph[filename] = tuple(refs)
        finally:
            visiting.pop()

    visit(root)
    return sources, graph


def canonical_order(
    filenames: Iterable[str], lib_entries: tuple[tuple[str, str], ...], root: str
) -> tuple[str, ...]:
    """Apply TypeScript's getDefaultLibFilePriority, leaving the non-libEntries root last."""
    names = list(filenames)
    if len(set(names)) != len(names):
        raise ProfileError("duplicate file in reference-lib closure")
    if root not in names:
        raise ProfileError(f"closure is missing root {root!r}")
    lib_priorities: dict[str, int] = {}
    for index, (lib, _filename) in enumerate(lib_entries):
        lib_priorities.setdefault(lib, index)
    priorities: dict[str, int] = {}
    for filename in names:
        validate_lib_file_name(filename)
        if filename == root:
            priority = len(lib_entries) + 1
        else:
            stem = filename.removeprefix("lib.").removesuffix(".d.ts")
            if stem not in lib_priorities:
                raise ProfileError(
                    f"closure file {filename!r} has no getDefaultLibFilePriority/libEntries entry"
                )
            priority = lib_priorities[stem]
        if priority in priorities.values():
            other = next(name for name, value in priorities.items() if value == priority)
            raise ProfileError(f"ambiguous library priority for {other!r} and {filename!r}")
        priorities[filename] = priority
    ordered = tuple(sorted(names, key=priorities.__getitem__))
    if ordered[-1] != root:
        raise ProfileError("closure root is not last in canonical order")
    return ordered


def framed_registry_digest(ordered: Iterable[tuple[str, bytes]]) -> str:
    digest = hashlib.sha256()
    for name, source in ordered:
        name_bytes = name.encode("utf-8")
        digest.update(struct.pack(">Q", len(name_bytes)))
        digest.update(struct.pack(">Q", len(source)))
        digest.update(name_bytes)
        digest.update(source)
    return digest.hexdigest()


def run_command(command: list[str], *, context: str) -> bytes:
    try:
        completed = subprocess.run(command, check=False, capture_output=True)
    except OSError as error:
        raise ProfileError(f"cannot run {context}: {error}") from error
    if completed.returncode != 0:
        stderr = completed.stderr.decode("utf-8", errors="replace").strip()
        raise ProfileError(f"{context} failed with exit {completed.returncode}: {stderr}")
    return completed.stdout


class GitSource:
    def __init__(self, git_dir: Path) -> None:
        self.git_dir = git_dir.resolve(strict=True)
        if not self.git_dir.is_dir():
            raise ProfileError(f"--typescript-git-dir is not a directory: {self.git_dir}")
        bare = self._run("rev-parse", "--is-bare-repository").decode("ascii").strip()
        if bare != "true":
            raise ProfileError(f"--typescript-git-dir is not a bare repository: {self.git_dir}")
        commit = self._run("rev-parse", f"{UPSTREAM_REVISION}^{{commit}}").decode("ascii").strip()
        if commit != UPSTREAM_REVISION:
            raise ProfileError(
                f"pinned revision resolves to {commit!r}, expected {UPSTREAM_REVISION!r}"
            )

    def _run(self, *arguments: str) -> bytes:
        return run_command(
            ["git", f"--git-dir={self.git_dir}", *arguments],
            context=f"git {' '.join(arguments)}",
        )

    def blob(self, path: str, *, expected_mode: str = "100644") -> tuple[str, bytes]:
        if Path(path).is_absolute() or ".." in Path(path).parts or "\\" in path:
            raise ProfileError(f"unsafe Git path: {path!r}")
        if expected_mode not in ("100644", "100755"):
            raise ProfileError(f"unsupported expected Git mode: {expected_mode!r}")
        tree_entry = self._run("ls-tree", "-z", UPSTREAM_REVISION, "--", path)
        try:
            header, listed_path = tree_entry.removesuffix(b"\0").split(b"\t", 1)
            mode, kind, raw_object_id = header.split(b" ", 2)
            object_id = raw_object_id.decode("ascii")
            decoded_path = listed_path.decode("utf-8")
        except (UnicodeDecodeError, ValueError) as error:
            raise ProfileError(f"malformed Git tree entry for {path!r}") from error
        if tree_entry.count(b"\0") != 1 or decoded_path != path:
            raise ProfileError(f"ambiguous Git tree entry for {path!r}")
        if mode != expected_mode.encode("ascii") or kind != b"blob":
            raise ProfileError(
                f"pinned Git path {path!r} has mode/type {mode.decode(errors='replace')} "
                f"{kind.decode(errors='replace')}, expected {expected_mode} blob"
            )
        if re.fullmatch(r"[0-9a-f]{40}", object_id) is None:
            raise ProfileError(f"unexpected Git object id for {path!r}: {object_id!r}")
        return object_id, self._run("cat-file", "blob", object_id)


def validate_package(package: Path) -> Path:
    package = package.resolve(strict=True)
    require_real_directory(package, "--typescript-package")
    require_real_directory(package / "lib", "TypeScript package lib directory")
    require_real_directory(package / "bin", "TypeScript package bin directory")
    package_json = package / "package.json"
    require_regular_file(package_json, "TypeScript package metadata")
    try:
        metadata = json.loads(package_json.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ProfileError(f"cannot read {package_json}: {error}") from error
    if metadata.get("name") != "typescript" or metadata.get("version") != TYPESCRIPT_VERSION:
        raise ProfileError(
            f"expected npm typescript@{TYPESCRIPT_VERSION}, got "
            f"{metadata.get('name')!r}@{metadata.get('version')!r}"
        )
    for relative in (
        *(path for path, _mode in BOOTSTRAP_FILES),
        LICENSE_NAME,
        NOTICE_NAME,
    ):
        require_regular_file(package / relative, f"TypeScript package input {relative}")
    return package


def verify_bootstrap(package: Path, git: GitSource) -> dict[str, bytes]:
    """Verify every npm JavaScript bootstrap input before parsing or executing it."""
    verified: dict[str, bytes] = {}
    for relative, mode in BOOTSTRAP_FILES:
        package_bytes = (package / relative).read_bytes()
        _git_blob, git_bytes = git.blob(relative, expected_mode=mode)
        if package_bytes != git_bytes:
            raise ProfileError(f"npm {relative} differs from pinned Git {relative}")
        verified[relative] = package_bytes
    return verified


def tsc_default_library_order(package: Path) -> tuple[str, ...]:
    with tempfile.TemporaryDirectory(prefix="typokat-library-profile-tsc-") as raw_temp:
        temp = Path(raw_temp)
        source = temp / "empty.ts"
        source.write_bytes(b"")
        source_path = source.resolve(strict=True)
        output = run_command(
            [
                str(package / "bin/tsc"),
                "--ignoreConfig",
                "--strict",
                "--target",
                "es2025",
                "--noEmit",
                "--listFilesOnly",
                str(source),
            ],
            context="explicit TypeScript package tsc --listFilesOnly",
        )
        lib_dir = (package / "lib").resolve(strict=True)
        ordered: list[str] = []
        unexpected: list[str] = []
        for raw_line in output.decode("utf-8").splitlines():
            path = Path(raw_line).resolve(strict=True)
            if path.parent == lib_dir and SAFE_LIB_FILE_RE.fullmatch(path.name):
                ordered.append(path.name)
            elif path != source_path:
                unexpected.append(raw_line)
    if unexpected:
        raise ProfileError(f"tsc --listFilesOnly returned unexpected paths: {unexpected!r}")
    if len(set(ordered)) != len(ordered):
        raise ProfileError("tsc --listFilesOnly returned duplicate libraries")
    return tuple(ordered)


def validate_source_bytes(name: str, source: bytes) -> None:
    try:
        source.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ProfileError(f"{name} is not UTF-8: {error}") from error
    if not source.endswith(b"\n"):
        raise ProfileError(f"{name} does not end in LF")
    if b"\r" in source:
        raise ProfileError(f"{name} contains CR bytes")


def auxiliary_record(name: str, package: Path, git: GitSource) -> tuple[AuxiliaryRecord, bytes]:
    package_path = package / name
    require_regular_file(package_path, f"TypeScript package input {name}")
    package_bytes = package_path.read_bytes()
    git_blob, git_bytes = git.blob(name)
    if package_bytes != git_bytes:
        raise ProfileError(f"npm {name} differs from pinned Git {name}")
    record = AuxiliaryRecord(
        name=name,
        npm_path=name,
        git_path=name,
        git_blob=git_blob,
        sha256=sha256(package_bytes),
        bytes=len(package_bytes),
        lf=package_bytes.count(b"\n"),
        cr=package_bytes.count(b"\r"),
        final_lf=package_bytes.endswith(b"\n"),
    )
    return record, package_bytes


def validate_record_graph(
    records: tuple[FileRecord, ...], ordered: tuple[str, ...], sources: dict[str, bytes],
    graph: dict[str, tuple[ReferenceEdge, ...]],
) -> None:
    """Require manifest rows, source files, and the recursive closure to agree exactly."""
    names = tuple(record.name for record in records)
    if names != ordered or tuple(record.ordinal for record in records) != tuple(range(len(records))):
        raise ProfileError("manifest rows do not match canonical closure order")
    if set(names) != set(sources) or set(names) != set(graph):
        raise ProfileError("manifest, source directory, and reference closure disagree")
    for record in records:
        if record.references != graph[record.name]:
            raise ProfileError(f"manifest reference edges drift for {record.name!r}")
        for edge in record.references:
            if edge.file not in sources:
                raise ProfileError(
                    f"manifest edge {record.name!r} -> {edge.file!r} is outside the closure"
                )

    reachable: set[str] = set()
    pending = [ROOT_FILE]
    while pending:
        name = pending.pop()
        if name in reachable:
            continue
        reachable.add(name)
        pending.extend(edge.file for edge in graph[name])
    if reachable != set(names):
        raise ProfileError("manifest contains files outside the recursive root closure")


def validate_profile_schema(profile_bytes: bytes, expected_file_count: int) -> None:
    """Reject additions or omissions in the provenance manifest schema."""
    try:
        document = tomllib.loads(profile_bytes.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ProfileError(f"generated profile.toml is invalid: {error}") from error
    if set(document) != PROFILE_KEYS:
        raise ProfileError("profile.toml has unexpected or missing root keys")
    for section in ("license", "third_party_notice"):
        value = document[section]
        if not isinstance(value, dict) or set(value) != AUXILIARY_KEYS:
            raise ProfileError(f"profile.toml [{section}] has unexpected or missing keys")
    files = document["file"]
    if not isinstance(files, list) or len(files) != expected_file_count:
        raise ProfileError("profile.toml has an unexpected file row count")
    for row in files:
        if not isinstance(row, dict) or set(row) != FILE_KEYS:
            raise ProfileError("profile.toml [[file]] has unexpected or missing keys")
        if row["source_kind"] not in ("script", "external-module"):
            raise ProfileError("profile.toml file source_kind is invalid")
        references = row["references"]
        if not isinstance(references, list):
            raise ProfileError("profile.toml file references is not an array")
        for reference in references:
            if not isinstance(reference, dict) or set(reference) != REFERENCE_KEYS:
                raise ProfileError("profile.toml reference has unexpected or missing keys")


def enforce_expected_profile(
    ordered: tuple[str, ...], sources: dict[str, bytes], graph: dict[str, tuple[ReferenceEdge, ...]],
    lib_entries: tuple[tuple[str, str], ...], license_record: AuxiliaryRecord,
    notice_record: AuxiliaryRecord, root_git_blob: str,
) -> tuple[str, str, str]:
    ordered_pairs = tuple((name, sources[name]) for name in ordered)
    source_bytes = sum(len(source) for _, source in ordered_pairs)
    source_lf = sum(source.count(b"\n") for _, source in ordered_pairs)
    source_cr = sum(source.count(b"\r") for _, source in ordered_pairs)
    root_sha = sha256(sources[ROOT_FILE])
    raw_sha = sha256(b"".join(source for _, source in ordered_pairs))
    framed_sha = framed_registry_digest(ordered_pairs)
    external_modules = frozenset(
        name for name, source in ordered_pairs if EXTERNAL_MODULE_LINE_RE.search(source)
    )
    actual = (
        len(ordered), sum(len(edges) for edges in graph.values()), len(lib_entries),
        len({filename for _, filename in lib_entries}), source_bytes, source_lf, source_cr,
        root_sha, raw_sha, framed_sha, root_git_blob, external_modules,
        license_record.bytes, license_record.sha256, license_record.git_blob,
        notice_record.bytes, notice_record.sha256, notice_record.git_blob,
    )
    expected = (
        EXPECTED_FILE_COUNT, EXPECTED_REFERENCE_EDGE_COUNT, EXPECTED_LIB_ENTRY_COUNT,
        EXPECTED_LIB_ENTRY_FILENAME_COUNT, EXPECTED_SOURCE_BYTES, EXPECTED_SOURCE_LF,
        EXPECTED_SOURCE_CR, EXPECTED_ROOT_SHA256, EXPECTED_RAW_CONCAT_SHA256,
        EXPECTED_FRAMED_SHA256, EXPECTED_ROOT_GIT_BLOB, EXPECTED_EXTERNAL_MODULES,
        LICENSE_BYTES, LICENSE_SHA256, LICENSE_GIT_BLOB,
        NOTICE_BYTES, NOTICE_SHA256, NOTICE_GIT_BLOB,
    )
    if actual != expected:
        raise ProfileError(f"pinned profile fingerprint drift:\nactual={actual!r}\nexpected={expected!r}")
    return root_sha, raw_sha, framed_sha


def render_auxiliary_section(section: str, record: AuxiliaryRecord) -> list[str]:
    return [
        f"[{section}]",
        f"name = {toml_string(record.name)}",
        f"npm_path = {toml_string(record.npm_path)}",
        f"git_path = {toml_string(record.git_path)}",
        f"git_blob = {toml_string(record.git_blob)}",
        f"sha256 = {toml_string(record.sha256)}",
        f"bytes = {record.bytes}",
        f"lf = {record.lf}",
        f"cr = {record.cr}",
        f"final_lf = {'true' if record.final_lf else 'false'}",
        "",
    ]


def render_profile(
    records: tuple[FileRecord, ...], root_sha: str, raw_sha: str, framed_sha: str,
    lib_entries: tuple[tuple[str, str], ...], license_record: AuxiliaryRecord,
    notice_record: AuxiliaryRecord,
) -> bytes:
    entry_digest = framed_registry_digest(
        (lib, filename.encode("utf-8")) for lib, filename in lib_entries
    )
    lines = [
        "schema = 1",
        f"typescript_version = {toml_string(TYPESCRIPT_VERSION)}",
        f"upstream_revision = {toml_string(UPSTREAM_REVISION)}",
        f"root = {toml_string(ROOT_FILE)}",
        f"file_count = {len(records)}",
        f"script_file_count = {sum(record.source_kind == 'script' for record in records)}",
        "external_module_file_count = "
        f"{sum(record.source_kind == 'external-module' for record in records)}",
        f"reference_edge_count = {sum(len(record.references) for record in records)}",
        f"source_bytes = {sum(record.bytes for record in records)}",
        f"source_lf = {sum(record.lf for record in records)}",
        f"source_cr = {sum(record.cr for record in records)}",
        f"root_sha256 = {toml_string(root_sha)}",
        f"raw_concat_sha256 = {toml_string(raw_sha)}",
        f"length_framed_sha256 = {toml_string(framed_sha)}",
        'length_frame = "u64be(name_len) || u64be(source_len) || name_utf8 || source"',
        'order = "getDefaultLibFilePriority/libEntries first occurrence; non-entry root last"',
        f"lib_entries_count = {len(lib_entries)}",
        f"lib_entries_filename_count = {len({filename for _, filename in lib_entries})}",
        f"lib_entries_framed_sha256 = {toml_string(entry_digest)}",
        "",
    ]
    lines.extend(render_auxiliary_section("license", license_record))
    lines.extend(render_auxiliary_section("third_party_notice", notice_record))
    for record in records:
        lines.extend(
            [
                "[[file]]",
                f"ordinal = {record.ordinal}",
                f"name = {toml_string(record.name)}",
                f"npm_path = {toml_string(record.npm_path)}",
                f"git_path = {toml_string(record.git_path)}",
                f"git_blob = {toml_string(record.git_blob)}",
                f"sha256 = {toml_string(record.sha256)}",
                f"bytes = {record.bytes}",
                f"lf = {record.lf}",
                f"cr = {record.cr}",
                f"final_lf = {'true' if record.final_lf else 'false'}",
                f"source_kind = {toml_string(record.source_kind)}",
            ]
        )
        if record.references:
            rendered_refs = ", ".join(
                "{ lib = " + toml_string(edge.lib) + ", file = " + toml_string(edge.file) + " }"
                for edge in record.references
            )
            lines.append(f"references = [{rendered_refs}]")
        else:
            lines.append("references = []")
        lines.append("")
    return ("\n".join(lines).rstrip() + "\n").encode("utf-8")


def copy_templates(destination: Path) -> None:
    template_dir = Path(__file__).resolve().parent / "templates"
    for name in TEMPLATE_NAMES:
        source = template_dir / name
        if not source.is_file():
            raise ProfileError(f"missing authored template: {source}")
        shutil.copyfile(source, destination / name)


def generate_tree(package_path: Path, git_dir: Path, destination: Path) -> None:
    package = validate_package(package_path)
    git = GitSource(git_dir)
    bootstrap = verify_bootstrap(package, git)
    typescript_js = bootstrap["lib/typescript.js"]
    lib_entries = parse_lib_entries(typescript_js)
    validate_lib_entries_fingerprint(lib_entries)
    lib_map = dict(lib_entries)

    def load(filename: str) -> bytes:
        path = package / "lib" / filename
        require_regular_file(path, f"TypeScript declaration {filename}")
        return path.read_bytes()

    sources, graph = resolve_closure(ROOT_FILE, lib_map, load)
    ordered = canonical_order(sources, lib_entries, ROOT_FILE)
    tsc_order = tsc_default_library_order(package)
    if ordered != tsc_order:
        raise ProfileError(
            "reference closure/libEntries order differs from explicit tsc --listFilesOnly:\n"
            f"computed={ordered!r}\ntsc={tsc_order!r}"
        )

    records: list[FileRecord] = []
    for ordinal, name in enumerate(ordered):
        source = sources[name]
        validate_source_bytes(name, source)
        git_path = f"lib/{name}"
        git_blob, git_source = git.blob(git_path)
        if source != git_source:
            raise ProfileError(f"npm lib/{name} differs from pinned Git {git_path}")
        records.append(
            FileRecord(
                ordinal=ordinal,
                name=name,
                npm_path=f"lib/{name}",
                git_path=git_path,
                git_blob=git_blob,
                sha256=sha256(source),
                bytes=len(source),
                lf=source.count(b"\n"),
                cr=source.count(b"\r"),
                final_lf=source.endswith(b"\n"),
                source_kind=(
                    "external-module" if EXTERNAL_MODULE_LINE_RE.search(source) else "script"
                ),
                references=graph[name],
            )
        )

    license_record, license_bytes = auxiliary_record(LICENSE_NAME, package, git)
    notice_record, notice_bytes = auxiliary_record(NOTICE_NAME, package, git)
    records_tuple = tuple(records)
    validate_record_graph(records_tuple, ordered, sources, graph)
    root_sha, raw_sha, framed_sha = enforce_expected_profile(
        ordered, sources, graph, lib_entries, license_record, notice_record,
        next(record.git_blob for record in records_tuple if record.name == ROOT_FILE),
    )

    destination.mkdir(parents=True, exist_ok=False)
    lib_destination = destination / "lib"
    lib_destination.mkdir()
    copy_templates(destination)
    (destination / LICENSE_NAME).write_bytes(license_bytes)
    (destination / NOTICE_NAME).write_bytes(notice_bytes)
    for name in ordered:
        (lib_destination / name).write_bytes(sources[name])
    profile_bytes = render_profile(
        records_tuple, root_sha, raw_sha, framed_sha, lib_entries,
        license_record, notice_record,
    )
    validate_profile_schema(profile_bytes, len(records_tuple))
    (destination / "profile.toml").write_bytes(profile_bytes)


def collect_tree(root: Path) -> dict[str, bytes]:
    try:
        root_mode = root.lstat().st_mode
    except OSError as error:
        raise ProfileError(f"cannot inspect profile output {root}: {error}") from error
    if not stat.S_ISDIR(root_mode):
        raise ProfileError(f"profile output is not a real directory: {root}")
    files: dict[str, bytes] = {}
    for path in sorted(root.rglob("*")):
        try:
            mode = path.lstat().st_mode
        except OSError as error:
            raise ProfileError(f"cannot inspect profile output entry {path}: {error}") from error
        if stat.S_ISLNK(mode):
            raise ProfileError(f"profile output contains a symlink: {path}")
        if stat.S_ISDIR(mode):
            continue
        if not stat.S_ISREG(mode):
            raise ProfileError(f"profile output contains a non-regular file: {path}")
        relative = path.relative_to(root).as_posix()
        if relative.startswith("/") or ".." in Path(relative).parts:
            raise ProfileError(f"unsafe output path: {relative!r}")
        files[relative] = path.read_bytes()
    return files


def compare_trees(expected: Path, actual: Path) -> None:
    expected_files = collect_tree(expected)
    actual_files = collect_tree(actual)
    missing = sorted(expected_files.keys() - actual_files.keys())
    extra = sorted(actual_files.keys() - expected_files.keys())
    changed = sorted(
        path for path in expected_files.keys() & actual_files.keys()
        if expected_files[path] != actual_files[path]
    )
    if missing or extra or changed:
        raise ProfileError(
            "profile tree drift: "
            f"missing={missing!r}, extra={extra!r}, changed={changed!r}"
        )


def validated_output_path(output: Path) -> Path:
    """Normalize lexically and reject output paths containing existing symlinks."""
    output = Path(os.path.abspath(os.fspath(output)))
    if output.name != f"typescript-{TYPESCRIPT_VERSION}":
        raise ProfileError(
            f"refusing output with unexpected basename (expected typescript-{TYPESCRIPT_VERSION}): "
            f"{output}"
        )
    for candidate in (*reversed(output.parents), output):
        try:
            mode = candidate.lstat().st_mode
        except FileNotFoundError:
            continue
        except OSError as error:
            raise ProfileError(f"cannot inspect output path component {candidate}: {error}") from error
        if stat.S_ISLNK(mode):
            raise ProfileError(f"refusing symlink in output path: {candidate}")
    return output


def safe_replace(staged: Path, output: Path) -> None:
    output = validated_output_path(output)
    if output.exists():
        if not output.is_dir() or not (output / "profile.toml").is_file():
            raise ProfileError(
                f"refusing to replace non-profile directory; remove it explicitly: {output}"
            )
        shutil.rmtree(output)
    os.replace(staged, output)


def parse_args(arguments: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--typescript-package", required=True, type=Path)
    parser.add_argument("--typescript-git-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--check", action="store_true",
        help="regenerate in a temporary directory and byte-compare with --output",
    )
    return parser.parse_args(arguments)


def main(arguments: list[str]) -> int:
    args = parse_args(arguments)
    output = validated_output_path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="typokat-library-profile-", dir=output.parent) as raw:
        staged = Path(raw) / "typescript-6.0.3"
        generate_tree(args.typescript_package, args.typescript_git_dir, staged)
        if args.check:
            repeated = Path(raw) / "typescript-6.0.3-repeat"
            generate_tree(args.typescript_package, args.typescript_git_dir, repeated)
            compare_trees(staged, repeated)
            if not output.is_dir():
                raise ProfileError(f"--check output does not exist: {output}")
            compare_trees(staged, output)
            print(f"profile OK: {output}")
        else:
            safe_replace(staged, output)
            print(f"generated profile: {output}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except ProfileError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)

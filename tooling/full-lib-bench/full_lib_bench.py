#!/usr/bin/env python3
"""Fail-closed full-default-library cross-tool benchmark."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import resource
import shutil
import signal
import selectors
import stat
import statistics
import subprocess
import sys
import tempfile
import time
import tomllib
from datetime import datetime, timezone
from collections import defaultdict
from dataclasses import dataclass
from typing import Any, Iterable


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
PROFILE = ROOT / "crates/typokat-library/src/typescript-6.0.3"
WORKLOADS = HERE / "workloads"
CONTRACT_PATH = HERE / "contract.toml"
LIBRARIES_LOCK = HERE / "expected-libraries.txt"
WORKLOADS_LOCK = HERE / "workloads.lock"
ORACLES_PATH = HERE / "oracles.json"
BUILD_HOME_ROOT = ROOT / "target/full-lib-bench/build-home"
CANONICAL_CARGO_HOME = BUILD_HOME_ROOT / "cargo-home"
ISOLATED_SOURCE_ROOT = Path("/tmp/typokat-full-lib-bench-source")
ISOLATED_TARGET_DIR = ISOLATED_SOURCE_ROOT / "target"
ROWS = ("fast-clean", "fast-errors", "collision", "fanout")
MIN_WINDOW_GAP_SECONDS = 60.0
TS_DIAGNOSTIC_RE = re.compile(
    r"^(?P<path>.+)\((?P<line>\d+),(?P<column>\d+)\): error TS(?P<code>\d+):"
)
TK_DIAGNOSTIC_RE = re.compile(
    r"^(?P<path>.+)\((?P<line>\d+),(?P<column>\d+)\): error TK(?P<code>\d+):"
)
TK_REASON_CHAIN_PRIMARY_CODES = frozenset({"2322", "2344", "2345", "2416"})
TK_REASON_CHAIN_FULL_DEPTH = 16
TK_REASON_CHAIN_ELISION_DEPTH = TK_REASON_CHAIN_FULL_DEPTH + 1
TK_REASON_CHAIN_TERMINAL_DEPTH = TK_REASON_CHAIN_ELISION_DEPTH + 1
TK_REASON_CHAIN_RE = re.compile(
    r"^(?P<indent>(?:  )+)(?:"
    r"(?P<wrapper>"
    r"Types of property '.*' are incompatible\.|"
    r"Types of parameters '[^']+'(?: and '[^']+')? are incompatible\.|"
    r"Types of parameters at position \d+ are incompatible\.|"
    r"Call signature return types are incompatible\."
    r")|"
    r"(?P<type>Type '.+' is not assignable to type '.+'\.)|"
    r"(?P<missing>Property '.*' is missing in type '.+'\.)|"
    r"(?P<arity>Type '.+' provides more parameters than type '.+' expects\.)|"
    r"(?P<elision>\.\.\. (?:1 more nested level|"
    r"(?:[2-9]|[1-9]\d+) more nested levels) omitted\.)"
    r")$"
)
TK_INCOMPLETE_RE = re.compile(
    r"^(?P<path>.+)\((?P<line>\d+),(?P<column>\d+)\): "
    r"incomplete\[(?P<id>[^\]\r\n]+)\]: (?P<context>[^\r\n]*)$"
)
RSS_RE = re.compile(r"Maximum resident set size \(kbytes\):\s*(\d+)")
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")
HEX40_RE = re.compile(r"^[0-9a-f]{40}$")


class ContractError(RuntimeError):
    """A fail-closed contract, execution, or evidence failure."""


class ProductionSemanticMismatch(ContractError):
    """A verified production binary ran canonically but differed from an oracle."""


@dataclass(frozen=True)
class LockedFile:
    ordinal: int
    path: str
    size: int
    sha256: str


@dataclass(frozen=True)
class ProcessResult:
    argv: tuple[str, ...]
    returncode: int
    stdout: str
    stderr: str
    wall_seconds: float
    pid: int
    started_monotonic_ns: int = 0
    ended_monotonic_ns: int = 0
    group_clean: bool = True


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def strict_json_loads(text: str, label: str) -> Any:
    def object_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        value: dict[str, Any] = {}
        for key, item in pairs:
            if key in value:
                raise ContractError(f"{label} contains duplicate key {key!r}")
            value[key] = item
        return value

    def invalid_constant(value: str) -> None:
        raise ContractError(f"{label} contains nonstandard numeric constant {value}")

    try:
        return json.loads(text, object_pairs_hook=object_pairs, parse_constant=invalid_constant)
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid {label}: {error}") from error


def load_contract(path: Path = CONTRACT_PATH) -> dict[str, Any]:
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ContractError(f"cannot load contract: {error}") from error
    exact_keys(
        data,
        {
            "schema", "rows", "typescript_flags", "typokat_flags", "forbidden_flags",
            "provider_probe", "profile", "comparator", "sampling", "controls",
        },
        "contract",
    )
    if data["schema"] != 1 or tuple(data["rows"]) != ROWS:
        raise ContractError("unsupported contract schema or benchmark row order")
    exact_keys(data["profile"], {
        "typescript_version", "upstream_revision", "root", "file_count", "source_bytes",
        "length_framed_sha256", "manifest_sha256",
    }, "contract.profile")
    exact_keys(
        data["provider_probe"], {"args", "schema", "check_route", "provider_route"},
        "contract.provider_probe",
    )
    require_int(
        data["provider_probe"]["schema"], "provider_probe.schema", minimum=1
    )
    if data["provider_probe"] != {
        "args": ["library-info", "--format", "json"],
        "schema": 2,
        "check_route": "production-complete-source-once",
        "provider_route": "production-default-library",
    }:
        raise ContractError("canonical provider probe contract differs")
    exact_keys(data["comparator"], {
        "package", "version", "npm_integrity", "npm_shasum", "upstream_revision",
        "platform_package", "platform", "platform_npm_integrity", "platform_npm_shasum",
        "binary_relative_path", "binary_size", "binary_sha256", "sentinel_relative_path",
        "sentinel_size", "sentinel_sha256",
    }, "contract.comparator")
    exact_keys(data["sampling"], {
        "trials", "warmups_per_tool", "blocks_per_trial", "schedule",
        "samples_per_tool_per_trial", "bootstrap_resamples", "bootstrap_seed",
        "timeout_seconds", "max_output_bytes", "memory_samples_per_tool",
        "max_typokat_rss_kib", "max_typokat_to_tsgo_median_rss", "minimum_speedup",
        "engineering_speedup",
    }, "contract.sampling")
    exact_keys(data["controls"], {
        "rename_comment_perturbations_per_row", "require_fresh_process_per_sample",
        "require_production_cli", "require_release_binary",
    }, "contract.controls")
    for key in ("file_count", "source_bytes"):
        require_int(data["profile"][key], f"profile.{key}", minimum=1)
    for key in ("binary_size", "sentinel_size"):
        require_int(data["comparator"][key], f"comparator.{key}", minimum=1)
    expected_ts_flags = [
        "--strict", "--target", "es2025", "--module", "preserve", "--noEmit", "--pretty", "false",
    ]
    expected_typokat_flags = ["check", "--format", "compact"]
    if data["typescript_flags"] != expected_ts_flags or data["typokat_flags"] != expected_typokat_flags:
        raise ContractError("canonical compiler flag arrays differ")
    expected_forbidden = [
        "--noLib", "--skipLibCheck", "--skipDefaultLibCheck", "--noCheck",
        "--incremental", "--watch", "--build",
    ]
    if data["forbidden_flags"] != expected_forbidden:
        raise ContractError("canonical forbidden flag array differs")
    reject_forbidden_command(["tsgo", *data["typescript_flags"]], data, allow_incremental_false=False)
    sampling = data["sampling"]
    expected_sampling = {
        "trials": 3,
        "warmups_per_tool": 5,
        "blocks_per_trial": 15,
        "schedule": "A,B,B,A",
        "samples_per_tool_per_trial": 30,
        "bootstrap_resamples": 100_000,
        "memory_samples_per_tool": 10,
        "minimum_speedup": 1.0,
        "engineering_speedup": 1.25,
    }
    for key, expected in expected_sampling.items():
        if sampling.get(key) != expected:
            raise ContractError(f"sampling.{key} must be {expected!r}")
    for key in (
        "trials", "warmups_per_tool", "blocks_per_trial", "samples_per_tool_per_trial",
        "bootstrap_resamples", "bootstrap_seed", "timeout_seconds", "max_output_bytes",
        "memory_samples_per_tool", "max_typokat_rss_kib",
    ):
        require_int(sampling[key], f"sampling.{key}", minimum=1)
    for key in (
        "max_typokat_to_tsgo_median_rss", "minimum_speedup", "engineering_speedup",
    ):
        require_number(sampling[key], f"sampling.{key}", positive=True)
    controls = data["controls"]
    require_int(
        controls["rename_comment_perturbations_per_row"],
        "controls.rename_comment_perturbations_per_row", minimum=1,
    )
    for key in (
        "require_fresh_process_per_sample", "require_production_cli", "require_release_binary",
    ):
        if controls[key] is not True:
            raise ContractError(f"controls.{key} must be true")
    return data


def exact_keys(value: Any, expected: set[str], label: str) -> None:
    if not isinstance(value, dict):
        raise ContractError(f"{label} must be an object/table")
    actual = set(value)
    if actual != expected:
        raise ContractError(
            f"{label} keys differ: missing={sorted(expected - actual)}, "
            f"extra={sorted(actual - expected)}"
        )


def parse_lock(path: Path, with_row: bool) -> dict[str, list[LockedFile]]:
    grouped: dict[str, list[LockedFile]] = defaultdict(list)
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise ContractError(f"cannot read {path.name}: {error}") from error
    for line_number, line in enumerate(lines, 1):
        if not line or line.startswith("#"):
            continue
        fields = line.split()
        expected_fields = 5 if with_row else 4
        if len(fields) != expected_fields:
            raise ContractError(f"{path.name}:{line_number}: malformed lock row")
        if with_row:
            row, ordinal_text, relative, size_text, digest = fields
        else:
            row, (ordinal_text, relative, size_text, digest) = "profile", fields
        if row not in ROWS and with_row:
            raise ContractError(f"{path.name}:{line_number}: unknown row {row!r}")
        if not ordinal_text.isdigit() or not size_text.isdigit() or not HEX64_RE.fullmatch(digest):
            raise ContractError(f"{path.name}:{line_number}: invalid ordinal, size, or digest")
        if Path(relative).is_absolute() or ".." in Path(relative).parts:
            raise ContractError(f"{path.name}:{line_number}: unsafe path")
        grouped[row].append(LockedFile(int(ordinal_text), relative, int(size_text), digest))
    for row, files in grouped.items():
        if [file.ordinal for file in files] != list(range(len(files))):
            raise ContractError(f"{path.name}: {row} ordinals are not contiguous and ordered")
        if len({file.path for file in files}) != len(files):
            raise ContractError(f"{path.name}: duplicate path in {row}")
    return dict(grouped)


def profile_lock() -> list[LockedFile]:
    records = parse_lock(LIBRARIES_LOCK, with_row=False).get("profile", [])
    if len(records) != 82:
        raise ContractError(f"expected exactly 82 locked libraries, got {len(records)}")
    return records


def workload_lock() -> dict[str, list[LockedFile]]:
    records = parse_lock(WORKLOADS_LOCK, with_row=True)
    if tuple(records) != ROWS:
        raise ContractError(f"workload lock row order differs: {tuple(records)!r}")
    if [len(records[row]) for row in ROWS] != [1, 1, 2, 32]:
        raise ContractError("workload matrix must contain 1, 1, 2, and 32 files")
    return records


def verify_regular(path: Path, label: str) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise ContractError(f"{label} is not a regular non-symlink file: {path}")
    try:
        return path.read_bytes()
    except OSError as error:
        raise ContractError(f"cannot read {label} {path}: {error}") from error


def verify_locked_file(path: Path, locked: LockedFile, label: str) -> None:
    data = verify_regular(path, label)
    actual = (len(data), sha256_bytes(data))
    expected = (locked.size, locked.sha256)
    if actual != expected:
        raise ContractError(f"{label} drift: actual={actual!r}, expected={expected!r}")


def verify_profile(contract: dict[str, Any]) -> None:
    profile_contract = contract["profile"]
    manifest_path = PROFILE / "profile.toml"
    manifest_bytes = verify_regular(manifest_path, "profile manifest")
    if sha256_bytes(manifest_bytes) != profile_contract["manifest_sha256"]:
        raise ContractError("profile manifest digest differs from the benchmark lock")
    try:
        manifest = tomllib.loads(manifest_bytes.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ContractError(f"invalid profile manifest: {error}") from error
    for key in (
        "typescript_version", "upstream_revision", "root", "file_count", "source_bytes",
        "length_framed_sha256",
    ):
        if manifest.get(key) != profile_contract[key]:
            raise ContractError(f"profile manifest {key} differs from contract")
    locked = profile_lock()
    entries = manifest.get("file")
    if not isinstance(entries, list) or len(entries) != 82:
        raise ContractError("profile manifest does not contain exactly 82 files")
    for expected, entry in zip(locked, entries, strict=True):
        actual_metadata = (entry.get("ordinal"), entry.get("name"), entry.get("bytes"), entry.get("sha256"))
        expected_metadata = (expected.ordinal, expected.path, expected.size, expected.sha256)
        if actual_metadata != expected_metadata:
            raise ContractError(
                f"profile order/metadata drift at ordinal {expected.ordinal}: "
                f"actual={actual_metadata!r}, expected={expected_metadata!r}"
            )
        verify_locked_file(PROFILE / "lib" / expected.path, expected, f"library {expected.path}")


def verify_workloads() -> None:
    lock = workload_lock()
    actual_paths = {
        path.relative_to(WORKLOADS).as_posix()
        for path in WORKLOADS.rglob("*.ts")
        if path.is_file() and not path.is_symlink()
    }
    expected_paths = {file.path for files in lock.values() for file in files}
    if actual_paths != expected_paths:
        raise ContractError(
            f"workload inventory differs: missing={sorted(expected_paths - actual_paths)}, "
            f"extra={sorted(actual_paths - expected_paths)}"
        )
    for row, files in lock.items():
        for locked in files:
            verify_locked_file(WORKLOADS / locked.path, locked, f"{row} workload {locked.path}")


def load_oracles() -> dict[str, Any]:
    try:
        data = strict_json_loads(ORACLES_PATH.read_text(encoding="utf-8"), "semantic oracles")
    except OSError as error:
        raise ContractError(f"cannot load semantic oracles: {error}") from error
    exact_keys(data, {"schema", "normalization", "rows"}, "oracles")
    if data["schema"] != 1 or tuple(data["rows"]) != ROWS:
        raise ContractError("oracle schema or row order differs")
    for row in ROWS:
        exact_keys(data["rows"][row], {"exit", "diagnostics"}, f"oracle {row}")
        require_int(data["rows"][row]["exit"], f"oracle {row}.exit", minimum=0)
        diagnostics = data["rows"][row]["diagnostics"]
        if not isinstance(diagnostics, list) or any(not isinstance(item, str) for item in diagnostics):
            raise ContractError(f"oracle {row} diagnostics must be a string list")
        if diagnostics != sorted(diagnostics) or len(diagnostics) != len(set(diagnostics)):
            raise ContractError(f"oracle {row} diagnostics must be sorted and unique")
    return data


def verify_contract() -> dict[str, Any]:
    contract = load_contract()
    verify_profile(contract)
    verify_workloads()
    load_oracles()
    return contract


def verify_package_root(package_root: Path, contract: dict[str, Any]) -> None:
    comparator = contract["comparator"]
    package_json = verify_regular(package_root / "package.json", "comparator package.json")
    try:
        metadata = strict_json_loads(package_json.decode("utf-8"), "comparator package.json")
    except UnicodeDecodeError as error:
        raise ContractError(f"invalid comparator package.json: {error}") from error
    if not isinstance(metadata, dict):
        raise ContractError("comparator package.json root must be an object")
    expected = {
        "name": comparator["platform_package"],
        "version": comparator["version"],
        "gitHead": comparator["upstream_revision"],
    }
    for key, value in expected.items():
        if metadata.get(key) != value:
            raise ContractError(f"comparator package {key} differs from lock")
    verify_npm_install_integrities(package_root, comparator)
    verify_binary(package_root / comparator["binary_relative_path"], comparator)
    verify_exact_file(
        package_root / comparator["sentinel_relative_path"],
        comparator["sentinel_size"],
        comparator["sentinel_sha256"],
        "comparator lib.d.ts sentinel",
    )


def verify_npm_install_integrities(package_root: Path, comparator: dict[str, Any]) -> None:
    lock_path = package_root.parents[1] / ".package-lock.json"
    try:
        lock = strict_json_loads(lock_path.read_text(encoding="utf-8"), "npm install lock")
    except OSError as error:
        raise ContractError(f"cannot read npm install integrity lock: {error}") from error
    if not isinstance(lock, dict):
        raise ContractError("npm install lock root must be an object")
    packages = lock.get("packages")
    if not isinstance(packages, dict):
        raise ContractError("npm install lock has no packages map")
    expected = {
        "node_modules/typescript": (comparator["version"], comparator["npm_integrity"]),
        "node_modules/@typescript/typescript-linux-x64": (
            comparator["version"], comparator["platform_npm_integrity"],
        ),
    }
    for name, (version, integrity) in expected.items():
        entry = packages.get(name)
        if not isinstance(entry, dict) or entry.get("version") != version or entry.get("integrity") != integrity:
            raise ContractError(f"npm install lock does not attest {name} integrity")


def verify_binary(path: Path, comparator: dict[str, Any]) -> None:
    verify_exact_file(
        path, comparator["binary_size"], comparator["binary_sha256"], "comparator binary"
    )
    if not os.access(path, os.X_OK):
        raise ContractError(f"comparator binary is not executable: {path}")


def verify_exact_file(path: Path, size: int, digest: str, label: str) -> None:
    data = verify_regular(path, label)
    if (len(data), sha256_bytes(data)) != (size, digest):
        raise ContractError(f"{label} identity differs from lock")


def stage_comparator(package_root: Path, destination: Path, contract: dict[str, Any]) -> Path:
    verify_package_root(package_root, contract)
    if destination.exists() or destination.is_symlink():
        raise ContractError(f"stage destination already exists: {destination}")
    lib_dir = destination / "lib"
    lib_dir.mkdir(parents=True)
    comparator = contract["comparator"]
    source_binary = package_root / comparator["binary_relative_path"]
    staged_binary = lib_dir / "tsc"
    shutil.copyfile(source_binary, staged_binary)
    staged_binary.chmod(0o755)
    shutil.copyfile(package_root / comparator["sentinel_relative_path"], lib_dir / "lib.d.ts")
    for library in profile_lock():
        shutil.copyfile(PROFILE / "lib" / library.path, lib_dir / library.path)
    stage_lock = {
        "schema": 1,
        "binary_sha256": comparator["binary_sha256"],
        "profile_sha256": contract["profile"]["length_framed_sha256"],
        "library_count": 82,
    }
    (destination / "stage-lock.json").write_text(
        json.dumps(stage_lock, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    verify_staged_comparator(staged_binary, contract)
    return staged_binary


def verify_staged_comparator(binary: Path, contract: dict[str, Any]) -> None:
    comparator = contract["comparator"]
    if binary.name != "tsc" or binary.parent.name != "lib":
        raise ContractError("staged comparator must be <stage>/lib/tsc")
    lib_dir = binary.parent
    verify_binary(binary, comparator)
    verify_exact_file(
        lib_dir / "lib.d.ts", comparator["sentinel_size"], comparator["sentinel_sha256"],
        "staged comparator sentinel",
    )
    expected_names = {library.path for library in profile_lock()} | {"lib.d.ts"}
    candidates = list(lib_dir.glob("lib*.d.ts"))
    if any(path.is_symlink() or not path.is_file() for path in candidates):
        raise ContractError("staged default-library inventory contains a non-regular entry")
    actual_names = {path.name for path in candidates}
    if actual_names != expected_names:
        raise ContractError(
            f"staged default-library inventory differs: "
            f"missing={sorted(expected_names - actual_names)}, extra={sorted(actual_names - expected_names)}"
        )
    for library in profile_lock():
        verify_locked_file(lib_dir / library.path, library, f"staged library {library.path}")
    result = run_process([str(binary), "--version"], timeout=5, max_output=4096)
    if result.returncode != 0 or result.stdout.strip() != f"Version {comparator['version']}":
        raise ContractError("staged comparator version probe differs")


def sanitized_environment() -> dict[str, str]:
    return {
        "PATH": "/usr/bin:/bin",
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "TZ": "UTC",
    }


def cargo_executable() -> Path:
    found = shutil.which("cargo")
    if found is None:
        raise ContractError("cargo is unavailable")
    return Path(found).absolute()


def rustc_executable() -> Path:
    found = shutil.which("rustc")
    if found is None:
        raise ContractError("rustc is unavailable")
    return Path(found).absolute()


def build_environment() -> dict[str, str]:
    environment = sanitized_environment()
    environment.update({
        "PATH": f"{cargo_executable().parent}:/usr/bin:/bin",
        "CARGO_TERM_COLOR": "never",
        "CARGO_HOME": str(CANONICAL_CARGO_HOME.resolve()),
        "CARGO_NET_OFFLINE": "true",
        "CARGO_ENCODED_RUSTFLAGS": "",
        "CARGO_BUILD_RUSTFLAGS": "",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER": "/usr/bin/cc",
        "CARGO_TARGET_DIR": str(ISOLATED_TARGET_DIR.resolve()),
    })
    return environment


def run_process(
    argv: Iterable[str], *, timeout: int, max_output: int, cwd: Path = ROOT,
    extra_environment: dict[str, str] | None = None,
) -> ProcessResult:
    argv_tuple = tuple(str(item) for item in argv)
    if not argv_tuple or any("\x00" in item for item in argv_tuple):
        raise ContractError("empty or NUL-containing command")
    environment = sanitized_environment()
    if extra_environment:
        forbidden = {
            "RUSTFLAGS", "RAYON_NUM_THREADS", "GOMAXPROCS", "TOKIO_WORKER_THREADS",
            "TYPOKAT_WU0B_PROCESS",
        }
        if forbidden & set(extra_environment):
            raise ContractError(f"forbidden environment override: {sorted(forbidden & set(extra_environment))}")
        environment.update(extra_environment)
    started = time.monotonic_ns()
    try:
        process = subprocess.Popen(
            argv_tuple,
            cwd=cwd,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
            close_fds=True,
        )
    except OSError as error:
        raise ContractError(f"cannot start command {argv_tuple!r}: {error}") from error
    assert process.stdout is not None and process.stderr is not None
    stdout_bytes = bytearray()
    stderr_bytes = bytearray()
    stream_buffers = {process.stdout: stdout_bytes, process.stderr: stderr_bytes}
    selector = selectors.DefaultSelector()
    for stream in stream_buffers:
        os.set_blocking(stream.fileno(), False)
        selector.register(stream, selectors.EVENT_READ)
    deadline = time.monotonic() + timeout
    leader_exited = False
    killed = False
    failure: ContractError | None = None
    failure_drain_deadline: float | None = None
    try:
        while selector.get_map():
            now = time.monotonic()
            if now >= deadline and failure is None:
                failure = ContractError(f"command timed out after {timeout}s: {argv_tuple!r}")
                failure_drain_deadline = now + 0.25
                kill_process_group(process.pid)
                killed = True
            if process.poll() is not None and not leader_exited:
                leader_exited = True
                # A successful leader may not leave pipe-holding descendants behind.
                if process_group_exists(process.pid):
                    kill_process_group(process.pid)
                    killed = True
            events = selector.select(timeout=min(0.05, max(0.0, deadline - now)))
            for key, _ in events:
                stream = key.fileobj
                target = stream_buffers[stream]
                try:
                    chunk = os.read(stream.fileno(), min(65_536, max_output + 1 - len(target)))
                except BlockingIOError:
                    continue
                if not chunk:
                    selector.unregister(stream)
                    stream.close()
                    continue
                target.extend(chunk)
                if len(target) > max_output and failure is None:
                    failure = ContractError(
                        f"command output exceeded {max_output} bytes while running: {argv_tuple!r}"
                    )
                    failure_drain_deadline = time.monotonic() + 0.25
                    kill_process_group(process.pid)
                    killed = True
            if failure is not None and leader_exited and not events:
                # Killed descendants have a bounded grace period to close inherited pipes.
                if failure_drain_deadline is not None and time.monotonic() >= failure_drain_deadline:
                    break
        if process.poll() is None:
            process.wait(timeout=1)
        returncode = process.returncode
    except subprocess.TimeoutExpired as error:
        kill_process_group(process.pid)
        process.wait()
        raise ContractError(f"process group did not terminate: {argv_tuple!r}") from error
    finally:
        selector.close()
        for stream in stream_buffers:
            try:
                stream.close()
            except OSError:
                pass
    if killed and not wait_for_group_exit(process.pid, 1.0):
        raise ContractError(f"process group containment failed: {argv_tuple!r}")
    if failure is not None:
        raise failure
    ended = time.monotonic_ns()
    wall_seconds = (ended - started) / 1_000_000_000
    try:
        stdout = stdout_bytes.decode("utf-8")
        stderr = stderr_bytes.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ContractError(f"command emitted malformed UTF-8: {error}") from error
    return ProcessResult(
        argv_tuple, returncode, stdout, stderr, wall_seconds, process.pid,
        started, ended, not process_group_exists(process.pid),
    )


def process_group_exists(pgid: int) -> bool:
    try:
        os.killpg(pgid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def kill_process_group(pgid: int) -> None:
    try:
        os.killpg(pgid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def wait_for_group_exit(pgid: int, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while process_group_exists(pgid) and time.monotonic() < deadline:
        time.sleep(0.005)
    return not process_group_exists(pgid)


def row_sources(row: str) -> list[Path]:
    return [WORKLOADS / item.path for item in workload_lock()[row]]


def tsgo_command(binary: Path, row: str, contract: dict[str, Any], list_files: bool = False) -> list[str]:
    command = [str(binary), *contract["typescript_flags"]]
    if list_files:
        command.append("--listFilesOnly")
    command.extend(str(path) for path in row_sources(row))
    reject_forbidden_command(command, contract, allow_incremental_false=False)
    return command


def typokat_command(binary: Path, row: str, contract: dict[str, Any]) -> list[str]:
    command = [str(binary), *contract["typokat_flags"], *map(str, row_sources(row))]
    reject_forbidden_command(command, contract, allow_incremental_false=False)
    return command


def reject_forbidden_command(
    command: list[str], contract: dict[str, Any], *, allow_incremental_false: bool
) -> None:
    del allow_incremental_false
    forbidden = {flag.casefold() for flag in contract["forbidden_flags"]}
    used = {
        argument.split("=", 1)[0]
        for argument in command[1:]
        if argument.startswith("--") and argument.split("=", 1)[0].casefold() in forbidden
    }
    if used:
        raise ContractError(f"forbidden benchmark flags: {sorted(used)}")


def attest_file_inventory(binary: Path, row: str, contract: dict[str, Any]) -> None:
    result = run_process(
        tsgo_command(binary, row, contract, list_files=True),
        timeout=contract["sampling"]["timeout_seconds"],
        max_output=contract["sampling"]["max_output_bytes"],
    )
    if result.returncode != 0 or result.stderr:
        raise ContractError(f"{row}: comparator --listFilesOnly failed: {result.stderr!r}")
    listed = [Path(line).resolve() for line in result.stdout.splitlines() if line]
    expected_libraries = [(binary.parent / item.path).resolve() for item in profile_lock()]
    expected_inputs = [path.resolve() for path in row_sources(row)]
    verify_listed_inventory(listed, expected_libraries + expected_inputs, row)


def verify_listed_inventory(listed: list[Path], expected: list[Path], row: str) -> None:
    if listed != expected:
        raise ContractError(
            f"{row}: --listFilesOnly order/inventory differs at first mismatch; "
            f"actual_count={len(listed)}, expected_count={len(expected)}"
        )


def logical_path(raw: str, path_map: dict[str, str]) -> str:
    normalized = str(Path(raw).resolve()) if Path(raw).is_absolute() else str((ROOT / raw).resolve())
    try:
        return path_map[normalized]
    except KeyError as error:
        raise ContractError(f"diagnostic references unexpected path: {raw!r}") from error


def normalize_diagnostics(
    result: ProcessResult, tool: str, row: str, path_map: dict[str, str] | None = None
) -> list[str]:
    regex = TS_DIAGNOSTIC_RE if tool == "tsgo" else TK_DIAGNOSTIC_RE
    if tool == "tsgo":
        if result.stderr:
            raise ContractError(f"tsgo emitted unexpected stderr for {row}")
        output = result.stdout
    else:
        if result.stdout:
            raise ContractError(f"typokat emitted unexpected stdout for {row}")
        output = result.stderr
    if path_map is None:
        path_map = {
            str(path.resolve()): path.relative_to(WORKLOADS).as_posix() for path in row_sources(row)
        }
    diagnostics: list[str] = []
    next_reason_depth: int | None = None
    reason_line_required = False
    terminal_reason_only = False
    for line in output.splitlines():
        match = regex.match(line)
        if match:
            if reason_line_required:
                raise ContractError(
                    f"{tool} emitted malformed/unrecognized diagnostic line: {line!r}"
                )
            diagnostics.append(
                f"{logical_path(match.group('path'), path_map)}:{match.group('line')}:"
                f"{match.group('column')}:{match.group('code')}"
            )
            next_reason_depth = (
                1
                if tool == "typokat"
                and match.group("code") in TK_REASON_CHAIN_PRIMARY_CODES
                else None
            )
            reason_line_required = False
            terminal_reason_only = False
            continue
        if line.startswith((" ", "\t")) and tool == "tsgo":
            continue
        incomplete = TK_INCOMPLETE_RE.fullmatch(line) if tool == "typokat" else None
        if incomplete:
            if reason_line_required:
                raise ContractError(
                    f"{tool} emitted malformed/unrecognized diagnostic line: {line!r}"
                )
            logical_path(incomplete.group("path"), path_map)
            diagnostics.append("INCOMPLETE:" + line)
            next_reason_depth = None
            terminal_reason_only = False
            continue
        if tool == "typokat" and next_reason_depth is not None:
            reason = TK_REASON_CHAIN_RE.fullmatch(line)
            if reason and len(reason.group("indent")) == next_reason_depth * 2:
                wrapper = reason.group("wrapper") is not None
                terminal = any(
                    reason.group(group) is not None for group in ("missing", "arity")
                )
                type_line = reason.group("type") is not None
                elision = reason.group("elision") is not None
                if next_reason_depth <= TK_REASON_CHAIN_FULL_DEPTH and not elision:
                    next_reason_depth = None if terminal else next_reason_depth + 1
                    reason_line_required = wrapper
                    terminal_reason_only = False
                    continue
                if (
                    next_reason_depth == TK_REASON_CHAIN_ELISION_DEPTH
                    and (terminal or type_line)
                ):
                    next_reason_depth = None
                    reason_line_required = False
                    terminal_reason_only = False
                    continue
                if (
                    next_reason_depth == TK_REASON_CHAIN_ELISION_DEPTH
                    and elision
                    and not terminal_reason_only
                ):
                    next_reason_depth = TK_REASON_CHAIN_TERMINAL_DEPTH
                    reason_line_required = True
                    terminal_reason_only = True
                    continue
                if (
                    next_reason_depth == TK_REASON_CHAIN_TERMINAL_DEPTH
                    and terminal_reason_only
                    and (terminal or type_line)
                ):
                    next_reason_depth = None
                    reason_line_required = False
                    terminal_reason_only = False
                    continue
        next_reason_depth = None
        reason_line_required = False
        terminal_reason_only = False
        raise ContractError(f"{tool} emitted malformed/unrecognized diagnostic line: {line!r}")
    if reason_line_required:
        raise ContractError(f"{tool} emitted malformed/unrecognized diagnostic continuation")
    return sorted(diagnostics)


def verify_semantics(binary: Path, tool: str, contract: dict[str, Any]) -> dict[str, Any]:
    if tool == "tsgo":
        verify_staged_comparator(binary, contract)
    elif tool == "typokat":
        verify_typokat_binary(binary)
    else:
        raise ContractError(f"unknown tool {tool!r}")
    oracles = load_oracles()["rows"]
    evidence: dict[str, Any] = {}
    for row in ROWS:
        if tool == "tsgo":
            attest_file_inventory(binary, row, contract)
            command = tsgo_command(binary, row, contract)
        else:
            command = typokat_command(binary, row, contract)
        result = run_process(
            command,
            timeout=contract["sampling"]["timeout_seconds"],
            max_output=contract["sampling"]["max_output_bytes"],
        )
        diagnostics = normalize_diagnostics(result, tool, row)
        expected = oracles[row]
        parity = result.returncode == expected["exit"] and diagnostics == expected["diagnostics"]
        evidence[row] = {
            "parity": parity,
            "exit": result.returncode,
            "diagnostics": diagnostics,
            "pid": result.pid,
        }
    return evidence


def verify_typokat_binary(binary: Path) -> None:
    canonical = (ROOT / "target/release/typokat").resolve()
    if binary.resolve() != canonical:
        raise ContractError(f"typokat benchmark must use standard release path {canonical}")
    verify_regular(binary, "typokat release binary")
    if not os.access(binary, os.X_OK):
        raise ContractError("typokat release binary is not executable")
    if os.environ.get("RUSTFLAGS"):
        raise ContractError("RUSTFLAGS must be unset for the benchmark build")
    build_inputs = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", *ROOT.glob("src/**/*.rs")]
    newest_input = max(path.stat().st_mtime_ns for path in build_inputs if path.is_file())
    if binary.stat().st_mtime_ns < newest_input:
        raise ContractError("typokat release binary is stale relative to Rust build inputs")


def assert_production(binary: Path, contract: dict[str, Any]) -> None:
    semantic = verify_semantics(binary, "typokat", contract)
    failed = {row: value for row, value in semantic.items() if not value["parity"]}
    if failed:
        raise ProductionSemanticMismatch(
            "typokat production semantic acceptance is RED: " + json.dumps(failed, sort_keys=True)
        )
    verify_perturbation_controls(binary, contract)


def verify_perturbation_controls(binary: Path, contract: dict[str, Any]) -> None:
    oracles = load_oracles()["rows"]
    for row in ROWS:
        originals = row_sources(row)
        with tempfile.TemporaryDirectory(prefix=f"typokat-full-lib-{row}-") as temporary:
            temporary_root = Path(temporary)
            perturbed: list[Path] = []
            path_map: dict[str, str] = {}
            for ordinal, original in enumerate(originals):
                target = temporary_root / f"renamed-{ordinal:02d}.ts"
                target.write_bytes(b"// route perturbation; semantics unchanged\n" + original.read_bytes())
                perturbed.append(target)
                path_map[str(target.resolve())] = original.relative_to(WORKLOADS).as_posix()
            command = [str(binary), *contract["typokat_flags"], *map(str, perturbed)]
            result = run_process(
                command,
                timeout=contract["sampling"]["timeout_seconds"],
                max_output=contract["sampling"]["max_output_bytes"],
            )
            diagnostics = normalize_diagnostics(result, "typokat", row, path_map)
            # One prepended line moves all expected diagnostic line numbers by one.
            adjusted = adjusted_oracle(row)
            if result.returncode != oracles[row]["exit"] or diagnostics != adjusted:
                raise ContractError(f"{row}: rename/comment perturbation changed production semantics")


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        raise ContractError("cannot calculate a percentile of an empty sample")
    ordered = sorted(values)
    index = (len(ordered) - 1) * fraction
    lower = math.floor(index)
    upper = math.ceil(index)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (index - lower)


def summarize(values: list[float]) -> dict[str, float]:
    median = statistics.median(values)
    return {
        "median": median,
        "mean": statistics.fmean(values),
        "p95": percentile(values, 0.95),
        "mad": statistics.median(abs(value - median) for value in values),
        "min": min(values),
        "max": max(values),
    }


def bootstrap_block_speedup_lower(
    blocks: list[list[dict[str, Any]]], resamples: int, seed: int
) -> float:
    if len(blocks) != 15 or any(len(block) != 4 for block in blocks):
        raise ContractError("block bootstrap requires fifteen complete ABBA blocks")
    expected = ("typokat", "tsgo", "tsgo", "typokat")
    for block in blocks:
        if tuple(sample["tool"] for sample in block) != expected:
            raise ContractError("block bootstrap received a non-ABBA block")
    generator = random.Random(seed)
    ratios: list[float] = []
    for _ in range(resamples):
        selected = generator.choices(blocks, k=15)
        typokat = [
            float(sample["wall_seconds"])
            for block in selected for sample in block if sample["tool"] == "typokat"
        ]
        tsgo = [
            float(sample["wall_seconds"])
            for block in selected for sample in block if sample["tool"] == "tsgo"
        ]
        ratios.append(statistics.median(tsgo) / statistics.median(typokat))
    return percentile(ratios, 0.05)


def current_affinity() -> list[int]:
    if not hasattr(os, "sched_getaffinity"):
        raise ContractError("the frozen Linux benchmark requires sched_getaffinity")
    affinity = sorted(os.sched_getaffinity(0))
    if len(affinity) < 2:
        raise ContractError("benchmark requires a normal multi-CPU affinity (at least two CPUs)")
    return affinity


RLIMIT_NAMES = ("RLIMIT_AS", "RLIMIT_CORE", "RLIMIT_NOFILE", "RLIMIT_NPROC", "RLIMIT_STACK")


def current_rlimits() -> dict[str, list[int | str]]:
    limits: dict[str, list[int | str]] = {}
    for name in RLIMIT_NAMES:
        resource_id = getattr(resource, name)
        values = resource.getrlimit(resource_id)
        limits[name] = ["infinity" if value == resource.RLIM_INFINITY else value for value in values]
    return limits


def execution_conditions() -> dict[str, Any]:
    nice = os.getpriority(os.PRIO_PROCESS, 0)
    if nice != 0:
        raise ContractError(f"benchmark requires normal priority (nice 0), got {nice}")
    return {"affinity": current_affinity(), "nice": nice, "rlimits": current_rlimits()}


def invocation_descriptor(
    argv: list[str], environment: dict[str, str] | None = None, cwd: Path = ROOT,
) -> dict[str, Any]:
    conditions = execution_conditions()
    return {
        "argv": [str(item) for item in argv],
        "env": environment or sanitized_environment(),
        "cwd": str(cwd.resolve()),
        **conditions,
    }


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def register_invocation(
    registry: dict[str, Any], argv: list[str], environment: dict[str, str] | None = None,
    cwd: Path = ROOT,
) -> str:
    descriptor = invocation_descriptor(argv, environment, cwd)
    digest = sha256_bytes(canonical_json(descriptor))
    previous = registry.setdefault(digest, descriptor)
    if previous != descriptor:
        raise ContractError("invocation digest collision")
    return digest


def execute_invocation(
    registry: dict[str, Any], argv: list[str], contract: dict[str, Any],
    environment_overrides: dict[str, str] | None = None,
    timeout: int | None = None, cwd: Path = ROOT,
) -> dict[str, Any]:
    environment = sanitized_environment()
    if environment_overrides:
        environment.update(environment_overrides)
    invocation = register_invocation(registry, argv, environment, cwd)
    guarded = guarded_executable(argv)
    before = executable_identity(guarded)
    result = run_process(
        argv,
        timeout=timeout or contract["sampling"]["timeout_seconds"],
        max_output=contract["sampling"]["max_output_bytes"],
        extra_environment=environment_overrides,
        cwd=cwd,
    )
    after = executable_identity(guarded)
    if before != after:
        raise ContractError(f"executable changed during collection: {guarded}")
    return process_record(invocation, result, {"path": str(guarded), "before": before, "after": after})


def process_record(
    invocation: str, result: ProcessResult, executable: dict[str, Any]
) -> dict[str, Any]:
    return {
        "invocation": invocation,
        "pid": result.pid,
        "started_monotonic_ns": result.started_monotonic_ns,
        "ended_monotonic_ns": result.ended_monotonic_ns,
        "wall_seconds": result.wall_seconds,
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "group_clean": result.group_clean,
        "executable": executable,
    }


def guarded_executable(argv: list[str]) -> Path:
    index = 2 if argv and argv[0] == "/usr/bin/time" and len(argv) > 2 else 0
    path = Path(argv[index])
    if not path.is_absolute():
        raise ContractError("canonical executable path must be absolute")
    return path.resolve()


def executable_identity(path: Path) -> dict[str, Any]:
    data = verify_regular(path, "invoked executable")
    return {"size": len(data), "sha256": sha256_bytes(data)}


def result_from_record(record: dict[str, Any], descriptor: dict[str, Any]) -> ProcessResult:
    return ProcessResult(
        tuple(descriptor["argv"]), record["returncode"], record["stdout"], record["stderr"],
        record["wall_seconds"], record["pid"], record["started_monotonic_ns"],
        record["ended_monotonic_ns"], record["group_clean"],
    )


def preread_paths(tsgo: Path, typokat: Path, row: str) -> list[Path]:
    return [
        typokat.resolve(), tsgo.resolve(),
        *(tsgo.parent / item.path for item in profile_lock()),
        *row_sources(row),
    ]


def preread_attestation(tsgo: Path, typokat: Path, row: str, block: int) -> dict[str, Any]:
    paths = preread_paths(tsgo, typokat, row)
    started = time.monotonic_ns()
    digest = hashlib.sha256()
    total = 0
    for path in paths:
        data = verify_regular(path, "pre-read input")
        encoded = str(path.resolve()).encode("utf-8")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(encoded)
        digest.update(data)
        total += len(data)
    ended = time.monotonic_ns()
    return {
        "block": block,
        "started_monotonic_ns": started,
        "ended_monotonic_ns": ended,
        "paths": [str(path.resolve()) for path in paths],
        "bytes": total,
        "sha256": digest.hexdigest(),
    }


def create_control_sources(output: Path) -> tuple[Path, dict[str, list[Path]]]:
    assets = output.with_suffix(output.suffix + ".assets")
    if assets.exists() or assets.is_symlink():
        raise ContractError(f"evidence assets already exist: {assets}")
    assets.mkdir(parents=True)
    controls: dict[str, list[Path]] = {}
    for row in ROWS:
        row_dir = assets / "controls" / row
        row_dir.mkdir(parents=True)
        controls[row] = []
        for ordinal, source in enumerate(row_sources(row)):
            target = row_dir / f"renamed-{ordinal:02d}.ts"
            target.write_bytes(b"// route perturbation; semantics unchanged\n" + source.read_bytes())
            controls[row].append(target.resolve())
    return assets, controls


def control_path_map(row: str, paths: list[Path]) -> dict[str, str]:
    originals = row_sources(row)
    if len(paths) != len(originals):
        raise ContractError(f"{row}: control source count differs")
    return {
        str(path.resolve()): original.relative_to(WORKLOADS).as_posix()
        for path, original in zip(paths, originals, strict=True)
    }


def adjusted_oracle(row: str) -> list[str]:
    adjusted = []
    for diagnostic in load_oracles()["rows"][row]["diagnostics"]:
        path, line, column, code = diagnostic.rsplit(":", 3)
        adjusted.append(f"{path}:{int(line) + 1}:{column}:{code}")
    return sorted(adjusted)


def binary_identities(typokat: Path, tsgo: Path, contract: dict[str, Any]) -> dict[str, Any]:
    verify_typokat_binary(typokat)
    verify_staged_comparator(tsgo, contract)
    git = run_process(
        ["/usr/bin/git", "rev-parse", "HEAD"], timeout=5, max_output=1024, cwd=ROOT
    )
    commit = git.stdout.strip()
    if git.returncode != 0 or not HEX40_RE.fullmatch(commit) or git.stderr:
        raise ContractError("cannot identify the benchmark git commit")
    return {
        "staged_profile": staged_profile_digest(tsgo),
        "comparator": {
            "sha256": sha256_file(tsgo), "size": tsgo.stat().st_size,
            "version": contract["comparator"]["version"],
            "platform": contract["comparator"]["platform"],
        },
        "typokat": {"sha256": sha256_file(typokat), "size": typokat.stat().st_size, "git_commit": commit},
        "collector_sha256": sha256_file(Path(__file__).resolve()),
    }


def staged_profile_digest(tsgo: Path) -> str:
    digest = hashlib.sha256()
    for item in profile_lock():
        data = verify_regular(tsgo.parent / item.path, "staged profile source")
        name = item.path.encode("utf-8")
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(name)
        digest.update(data)
    return digest.hexdigest()


def host_identity() -> dict[str, Any]:
    cpu_model = "unknown"
    try:
        for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
            if line.startswith("model name"):
                cpu_model = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    conditions = execution_conditions()
    return {
        "hostname": platform.node(), "kernel": platform.release(), "machine": platform.machine(),
        "cpu_model": cpu_model, "cpu_count": os.cpu_count() or 0,
        "cpu_affinity": conditions["affinity"], "nice": conditions["nice"],
        "rlimits": conditions["rlimits"], "timezone": "UTC",
        "started_utc": datetime.now(timezone.utc).isoformat(),
    }


def compiler_result_without_time(record: dict[str, Any], descriptor: dict[str, Any]) -> tuple[ProcessResult, int]:
    result = result_from_record(record, descriptor)
    marker = "\tCommand being timed:"
    position = result.stderr.find(marker)
    if position < 0:
        raise ContractError("memory sample has no /usr/bin/time -v report")
    compiler_stderr = result.stderr[:position]
    report = result.stderr[position:]
    matches = RSS_RE.findall(report)
    if len(matches) != 1:
        raise ContractError("memory sample has malformed RSS evidence")
    compiler = ProcessResult(
        result.argv, result.returncode, result.stdout, compiler_stderr, result.wall_seconds,
        result.pid, result.started_monotonic_ns, result.ended_monotonic_ns, result.group_clean,
    )
    return compiler, int(matches[0])


def collect_semantics(
    typokat: Path, tsgo: Path, controls: dict[str, list[Path]],
    registry: dict[str, Any], contract: dict[str, Any],
) -> tuple[dict[str, Any], dict[str, Any], bool]:
    semantics: dict[str, Any] = {}
    control_records: dict[str, Any] = {}
    all_match = True
    oracles = load_oracles()["rows"]
    for row in ROWS:
        inventory_argv = tsgo_command(tsgo, row, contract, list_files=True)
        inventory = execute_invocation(registry, inventory_argv, contract)
        listed = [Path(line).resolve() for line in inventory["stdout"].splitlines() if line]
        expected = [
            *(tsgo.parent / item.path for item in profile_lock()),
            *row_sources(row),
        ]
        inventory_match = (
            inventory["returncode"] == 0 and not inventory["stderr"] and
            listed == [path.resolve() for path in expected]
        )
        tsgo_record = execute_invocation(registry, tsgo_command(tsgo, row, contract), contract)
        typokat_record = execute_invocation(
            registry, typokat_command(typokat, row, contract), contract
        )
        tsgo_result = result_from_record(tsgo_record, registry[tsgo_record["invocation"]])
        typokat_result = result_from_record(typokat_record, registry[typokat_record["invocation"]])
        tsgo_diagnostics = normalize_diagnostics(tsgo_result, "tsgo", row)
        typokat_diagnostics = normalize_diagnostics(typokat_result, "typokat", row)
        expected_oracle = oracles[row]
        tsgo_match = (
            tsgo_record["returncode"] == expected_oracle["exit"] and
            tsgo_diagnostics == expected_oracle["diagnostics"]
        )
        typokat_match = (
            typokat_record["returncode"] == expected_oracle["exit"] and
            typokat_diagnostics == expected_oracle["diagnostics"]
        )
        semantics[row] = {
            "inventory": inventory,
            "tsgo": tsgo_record,
            "typokat": typokat_record,
            "oracle_sha256": sha256_bytes(canonical_json(expected_oracle)),
        }

        control_paths = controls[row]
        tsgo_control_argv = [
            str(tsgo), *contract["typescript_flags"], *map(str, control_paths)
        ]
        typokat_control_argv = [
            str(typokat), *contract["typokat_flags"], *map(str, control_paths)
        ]
        tsgo_control = execute_invocation(registry, tsgo_control_argv, contract)
        typokat_control = execute_invocation(registry, typokat_control_argv, contract)
        path_map = control_path_map(row, control_paths)
        tsgo_control_result = result_from_record(
            tsgo_control, registry[tsgo_control["invocation"]]
        )
        typokat_control_result = result_from_record(
            typokat_control, registry[typokat_control["invocation"]]
        )
        expected_adjusted = adjusted_oracle(row)
        control_exit = expected_oracle["exit"]
        tsgo_control_match = (
            tsgo_control["returncode"] == control_exit and
            normalize_diagnostics(tsgo_control_result, "tsgo", row, path_map) == expected_adjusted
        )
        typokat_control_match = (
            typokat_control["returncode"] == control_exit and
            normalize_diagnostics(typokat_control_result, "typokat", row, path_map) == expected_adjusted
        )
        control_records[row] = {
            "sources": [str(path) for path in control_paths],
            "tsgo": tsgo_control,
            "typokat": typokat_control,
        }
        all_match &= (
            inventory_match and tsgo_match and typokat_match and
            tsgo_control_match and typokat_control_match
        )
    return semantics, control_records, all_match


def collect_timing(
    typokat: Path, tsgo: Path, registry: dict[str, Any], contract: dict[str, Any],
    labels: list[str], window_pause: float,
) -> list[dict[str, Any]]:
    windows: list[dict[str, Any]] = []
    sampling = contract["sampling"]
    for trial_index, label in enumerate(labels):
        if trial_index and window_pause > 0:
            time.sleep(window_pause)
        window_started_utc = datetime.now(timezone.utc).isoformat()
        window_started = time.monotonic_ns()
        rows: dict[str, Any] = {}
        for row_index, row in enumerate(ROWS):
            warmups: list[dict[str, Any]] = []
            for _ in range(sampling["warmups_per_tool"]):
                warmups.append(tag_record(
                    execute_invocation(registry, typokat_command(typokat, row, contract), contract),
                    "typokat",
                ))
                warmups.append(tag_record(
                    execute_invocation(registry, tsgo_command(tsgo, row, contract), contract),
                    "tsgo",
                ))
            samples: list[dict[str, Any]] = []
            pre_reads: list[dict[str, Any]] = []
            commands = (
                ("typokat", typokat_command(typokat, row, contract)),
                ("tsgo", tsgo_command(tsgo, row, contract)),
                ("tsgo", tsgo_command(tsgo, row, contract)),
                ("typokat", typokat_command(typokat, row, contract)),
            )
            for block in range(sampling["blocks_per_trial"]):
                pre_reads.append(preread_attestation(tsgo, typokat, row, block))
                block_records = []
                for slot, (tool, command) in enumerate(commands):
                    record = tag_record(execute_invocation(registry, command, contract), tool)
                    record.update({"block": block, "slot": slot})
                    samples.append(record)
                    block_records.append(record)
            tool_values = {
                tool: [record["wall_seconds"] for record in samples if record["tool"] == tool]
                for tool in ("typokat", "tsgo")
            }
            blocks = [samples[index:index + 4] for index in range(0, len(samples), 4)]
            summaries = {tool: summarize(values) for tool, values in tool_values.items()}
            rows[row] = {
                "warmups": warmups,
                "pre_reads": pre_reads,
                "samples": samples,
                "summary": {
                    "typokat": summaries["typokat"],
                    "tsgo": summaries["tsgo"],
                    "speedup": summaries["tsgo"]["median"] / summaries["typokat"]["median"],
                    "p95_ratio": summaries["tsgo"]["p95"] / summaries["typokat"]["p95"],
                    "bootstrap_lower_95": bootstrap_block_speedup_lower(
                        blocks, sampling["bootstrap_resamples"],
                        sampling["bootstrap_seed"] + row_index * 10 + trial_index,
                    ),
                },
            }
        window_ended = time.monotonic_ns()
        windows.append({
            "label": label,
            "started_utc": window_started_utc,
            "ended_utc": datetime.now(timezone.utc).isoformat(),
            "started_monotonic_ns": window_started,
            "ended_monotonic_ns": window_ended,
            "rows": rows,
        })
    return windows


def tag_record(record: dict[str, Any], tool: str) -> dict[str, Any]:
    tagged = dict(record)
    tagged["tool"] = tool
    return tagged


def collect_memory(
    typokat: Path, tsgo: Path, registry: dict[str, Any], contract: dict[str, Any]
) -> dict[str, Any]:
    memory: dict[str, Any] = {}
    for row in ROWS:
        pre_reads: list[dict[str, Any]] = []
        samples: list[dict[str, Any]] = []
        commands = (
            ("typokat", ["/usr/bin/time", "-v", *typokat_command(typokat, row, contract)]),
            ("tsgo", ["/usr/bin/time", "-v", *tsgo_command(tsgo, row, contract)]),
            ("tsgo", ["/usr/bin/time", "-v", *tsgo_command(tsgo, row, contract)]),
            ("typokat", ["/usr/bin/time", "-v", *typokat_command(typokat, row, contract)]),
        )
        for block in range(5):
            pre_reads.append(preread_attestation(tsgo, typokat, row, block))
            for slot, (tool, command) in enumerate(commands):
                record = tag_record(execute_invocation(registry, command, contract), tool)
                _, rss = compiler_result_without_time(record, registry[record["invocation"]])
                record.update({"block": block, "slot": slot, "rss_kib": rss})
                samples.append(record)
        memory[row] = {"pre_reads": pre_reads, "samples": samples}
    return memory


def optional_identity(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    return executable_identity(path.resolve())


def file_identity(path: Path) -> dict[str, Any]:
    data = verify_regular(path, "build provenance file")
    return {"path": str(path.resolve()), "size": len(data), "sha256": sha256_bytes(data)}


def tracked_source_paths() -> list[Path]:
    result = run_process(
        ["/usr/bin/git", "ls-files", "-z", "--cached"], timeout=10,
        max_output=16 * 1024 * 1024, cwd=ROOT,
    )
    if result.returncode != 0 or result.stderr:
        raise ContractError("cannot enumerate the tracked source snapshot")
    raw_paths = result.stdout.split("\0")
    if raw_paths[-1:] != [""]:
        raise ContractError("tracked source inventory is not NUL-terminated")
    inventory = raw_paths[:-1]
    paths = [Path(raw) for raw in inventory]
    if not paths or len(paths) != len(set(paths)) or inventory != sorted(inventory):
        raise ContractError("tracked source inventory is empty, duplicated, or reordered")
    if any(path.is_absolute() or ".." in path.parts for path in paths):
        raise ContractError("tracked source inventory contains an unsafe path")
    return paths


def reject_ancestor_cargo_configs(source_root: Path) -> None:
    cursor = source_root.resolve().parent
    while True:
        for name in ("config", "config.toml"):
            candidate = cursor / ".cargo" / name
            if candidate.exists() or candidate.is_symlink():
                raise ContractError(f"isolated source has an untrusted ancestor Cargo config: {candidate}")
        if cursor.parent == cursor:
            break
        cursor = cursor.parent


def prepare_isolated_source() -> dict[str, Any]:
    source_root = ISOLATED_SOURCE_ROOT
    resolved = source_root.resolve()
    temporary_root = Path(tempfile.gettempdir()).resolve()
    if not resolved.is_relative_to(temporary_root) or resolved == temporary_root:
        raise ContractError("isolated source root escaped the dedicated temporary tree")
    if source_root.is_symlink():
        raise ContractError("isolated source root must not be a symlink")
    reject_ancestor_cargo_configs(source_root)
    if source_root.exists():
        if not source_root.is_dir():
            raise ContractError("isolated source root is not a directory")
        shutil.rmtree(source_root)
    source_root.mkdir(parents=True)
    paths = tracked_source_paths()
    for relative in paths:
        source = ROOT / relative
        if source.is_symlink() or not source.is_file():
            raise ContractError(f"tracked build source is not a regular file: {relative}")
        destination = source_root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        mode = source.stat().st_mode
        destination.chmod(0o755 if mode & stat.S_IXUSR else 0o644)
    return source_snapshot(source_root, paths)


def source_snapshot(source_root: Path, paths: list[Path] | None = None) -> dict[str, Any]:
    paths = paths or tracked_source_paths()
    digest = hashlib.sha256()
    total = 0
    for relative in paths:
        path = source_root / relative
        data = verify_regular(path, "isolated tracked source")
        executable = bool(path.stat().st_mode & stat.S_IXUSR)
        encoded = relative.as_posix().encode("utf-8")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(b"\x01" if executable else b"\x00")
        digest.update(encoded)
        digest.update(data)
        total += len(data)
    revision = run_process(
        ["/usr/bin/git", "rev-parse", "HEAD", "HEAD^{tree}"], timeout=5,
        max_output=1024, cwd=ROOT,
    )
    ids = revision.stdout.splitlines()
    if revision.returncode != 0 or revision.stderr or len(ids) != 2 or any(
        HEX40_RE.fullmatch(value) is None for value in ids
    ):
        raise ContractError("cannot bind isolated source to Git HEAD/tree")
    return {
        "root": str(source_root.resolve()), "git_commit": ids[0], "git_tree": ids[1],
        "file_count": len(paths), "source_bytes": total, "sha256": digest.hexdigest(),
    }


def source_cargo_home() -> Path:
    configured = os.environ.get("CARGO_HOME")
    source = Path(configured).expanduser() if configured else cargo_executable().parent.parent
    source = source.resolve()
    if source == CANONICAL_CARGO_HOME.resolve():
        raise ContractError("canonical Cargo home cannot also be the cache source")
    return source


def prepare_canonical_cargo_home() -> dict[str, Any]:
    root = BUILD_HOME_ROOT.resolve()
    expected_root = (ROOT.resolve() / "target/full-lib-bench/build-home").resolve()
    if root != expected_root:
        raise ContractError("canonical build-home path escaped target/full-lib-bench")
    home = CANONICAL_CARGO_HOME
    if home.is_symlink():
        raise ContractError("canonical Cargo home must not be a symlink")
    if home.exists():
        if not home.is_dir():
            raise ContractError("canonical Cargo home is not a directory")
        shutil.rmtree(home)
    home.mkdir(parents=True)
    source = source_cargo_home()
    exposed: list[dict[str, str]] = []
    for relative in (Path("registry/cache"), Path("registry/index")):
        origin = source / relative
        if not origin.exists() or not origin.is_dir():
            continue
        destination = home / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.symlink_to(origin.resolve(), target_is_directory=True)
        exposed.append({"path": relative.as_posix(), "target": str(origin.resolve())})
    for name in (".package-cache", ".package-cache-mutate"):
        (home / name).touch()
    layout = canonical_cargo_home_layout()
    return {"cache_source": str(source), "exposed": exposed, **layout}


def canonical_cargo_home_layout() -> dict[str, Any]:
    home = CANONICAL_CARGO_HOME
    if not home.is_dir() or home.is_symlink():
        raise ContractError("canonical Cargo home is missing or unsafe")
    entries: list[dict[str, Any]] = []

    def visit(directory: Path) -> None:
        for path in sorted(directory.iterdir(), key=lambda item: item.name):
            relative = path.relative_to(home).as_posix()
            if path.is_symlink():
                entries.append({"path": relative, "kind": "symlink", "target": os.readlink(path)})
            elif path.is_dir():
                entries.append({"path": relative, "kind": "directory"})
                visit(path)
            elif path.is_file():
                data = path.read_bytes()
                entries.append({
                    "path": relative, "kind": "file", "size": len(data),
                    "sha256": sha256_bytes(data),
                })
            else:
                raise ContractError(f"unsupported canonical Cargo home entry: {relative}")
    visit(home)
    if any(entry["path"] in {"config", "config.toml"} for entry in entries):
        raise ContractError("canonical Cargo home must remain configless")
    return {"path": str(home.resolve()), "entries": entries, "sha256": sha256_bytes(canonical_json(entries))}


def toolchain_file_identities() -> list[dict[str, Any]]:
    identities: list[dict[str, Any]] = []
    for path in (ROOT / "rust-toolchain", ROOT / "rust-toolchain.toml"):
        if path.exists() or path.is_symlink():
            identities.append({"path": str(path.resolve()), "present": True, **executable_identity(path)})
        else:
            identities.append({"path": str(path.resolve()), "present": False})
    return identities


def collect_effective_build_fingerprint() -> dict[str, Any]:
    candidates = list(
        (ISOLATED_TARGET_DIR / "release/.fingerprint").glob("typokat-*/bin-typokat.json")
    )
    if len(candidates) != 1:
        raise ContractError("fresh release build must produce exactly one typokat bin fingerprint")
    fingerprint = candidates[0]
    observed = strict_json_loads(fingerprint.read_text(encoding="utf-8"), "Cargo fingerprint")
    if not isinstance(observed, dict):
        raise ContractError("release build fingerprint is malformed")
    validate_effective_rustflags(observed.get("rustflags"))
    output = ISOLATED_TARGET_DIR / "release/typokat"
    return {
        "file": file_identity(fingerprint),
        "invoked_timestamp": file_identity(fingerprint.parent / "invoked.timestamp"),
        "output": file_identity(output),
        "rustflags": observed["rustflags"],
        "features": observed.get("features"),
        "profile": observed.get("profile"),
        "config": observed.get("config"),
    }


def validate_effective_rustflags(rustflags: Any) -> None:
    if rustflags != []:
        raise ContractError("release build effective rustflags must be empty")


def verify_toolchain_file_identities(actual: Any) -> None:
    if actual != toolchain_file_identities():
        raise ContractError("rust-toolchain file presence or identity drifted")


def collect_build_provenance(
    registry: dict[str, Any], contract: dict[str, Any], typokat: Path, tsgo: Path,
) -> dict[str, Any]:
    git_command = clean_worktree_command()
    worktree = execute_invocation(registry, git_command, contract, cwd=ROOT)
    if worktree["returncode"] != 0 or worktree["stderr"] or worktree["stdout"]:
        raise ContractError("authoritative run requires a completely clean worktree")
    source_before = prepare_isolated_source()
    cargo_home_before = prepare_canonical_cargo_home()
    toolchain_files = toolchain_file_identities()
    cargo = cargo_executable()
    rustc = rustc_executable()
    overrides = {
        key: value for key, value in build_environment().items()
        if sanitized_environment().get(key) != value
    }
    cargo_version = execute_invocation(
        registry, [str(cargo), "--version", "--verbose"], contract, overrides,
        cwd=ISOLATED_SOURCE_ROOT,
    )
    rustc_version = execute_invocation(
        registry, [str(rustc), "--version", "--verbose"], contract, overrides,
        cwd=ISOLATED_SOURCE_ROOT,
    )
    if cargo_version["returncode"] != 0 or rustc_version["returncode"] != 0:
        raise ContractError("cannot attest Cargo/rustc versions")
    if cargo_version["stderr"] or rustc_version["stderr"]:
        raise ContractError("Cargo/rustc version probes emitted stderr")
    before = {"typokat": optional_identity(typokat), "tsgo": executable_identity(tsgo)}
    compile_record = execute_invocation(
        registry, release_build_command(), contract, overrides, timeout=600,
        cwd=ISOLATED_SOURCE_ROOT,
    )
    if compile_record["returncode"] != 0:
        raise ContractError(
            "canonical cargo build --release failed: " + compile_record["stderr"][-4000:]
        )
    effective_build = collect_effective_build_fingerprint()
    install_isolated_build_output(typokat)
    verify_typokat_binary(typokat)
    source_after = source_snapshot(ISOLATED_SOURCE_ROOT)
    if source_before != source_after:
        raise ContractError("isolated tracked source changed during release build")
    reject_ancestor_cargo_configs(ISOLATED_SOURCE_ROOT)
    cargo_home_after = canonical_cargo_home_layout()
    after = {"typokat": executable_identity(typokat), "tsgo": executable_identity(tsgo)}
    isolated_output = effective_build["output"]
    if {
        "size": isolated_output["size"], "sha256": isolated_output["sha256"],
    } != after["typokat"]:
        raise ContractError("installed typokat differs from isolated release output")
    if before["tsgo"] != after["tsgo"]:
        raise ContractError("staged comparator changed during the release build")
    config_paths = [ROOT / ".cargo/config", ROOT / ".cargo/config.toml"]
    configs = [file_identity(path) for path in config_paths if path.exists()]
    return {
        "worktree": worktree,
        "cargo_version": cargo_version,
        "rustc_version": rustc_version,
        "cargo_lock": file_identity(ROOT / "Cargo.lock"),
        "cargo_configs": configs,
        "source_before": source_before,
        "source_after": source_after,
        "cargo_home_before": cargo_home_before,
        "cargo_home_after": cargo_home_after,
        "toolchain_files": toolchain_files,
        "effective_build": effective_build,
        "binary_before": before,
        "compile": compile_record,
        "binary_after": after,
    }


def release_build_command() -> list[str]:
    return [str(cargo_executable()), "build", "--release"]


def install_isolated_build_output(typokat: Path) -> None:
    source = ISOLATED_TARGET_DIR / "release/typokat"
    data = verify_regular(source, "isolated release output")
    typokat.parent.mkdir(parents=True, exist_ok=True)
    temporary = typokat.parent / ".typokat-full-lib-bench-install"
    if temporary.exists() or temporary.is_symlink():
        temporary.unlink()
    temporary.write_bytes(data)
    temporary.chmod(0o755)
    os.replace(temporary, typokat)


def clean_worktree_command() -> list[str]:
    return ["/usr/bin/git", "status", "--porcelain=v1", "--untracked-files=all"]


def collect_final_worktree(
    registry: dict[str, Any], contract: dict[str, Any],
) -> dict[str, Any]:
    record = execute_invocation(registry, clean_worktree_command(), contract, cwd=ROOT)
    if record["returncode"] != 0 or record["stderr"] or record["stdout"]:
        raise ContractError("authoritative run requires the worktree to remain clean")
    return record


def collect_provider_probe(
    registry: dict[str, Any], contract: dict[str, Any], typokat: Path,
) -> tuple[dict[str, Any], bool]:
    command = [str(typokat), *contract["provider_probe"]["args"]]
    record = execute_invocation(registry, command, contract)
    observed: Any = None
    if record["returncode"] == 0 and not record["stderr"]:
        try:
            observed = strict_json_loads(record["stdout"], "provider probe output")
            validate_provider_observation(observed, contract)
        except ContractError:
            return {"record": record, "observed": observed}, False
        return {"record": record, "observed": observed}, True
    return {"record": record, "observed": observed}, False


def validate_provider_observation(observed: Any, contract: dict[str, Any]) -> None:
    if not isinstance(observed, dict):
        raise ContractError("provider probe output must be an object")
    exact_keys(observed, {
        "schema", "profile_sha256", "file_count", "check_route", "provider_route",
    }, "provider probe output")
    if (
        require_int(observed["schema"], "provider schema", minimum=1)
        != contract["provider_probe"]["schema"]
    ):
        raise ContractError("provider probe schema differs")
    if observed["profile_sha256"] != contract["profile"]["length_framed_sha256"]:
        raise ContractError("provider profile identity differs")
    if (
        require_int(observed["file_count"], "provider file_count", minimum=1)
        != contract["profile"]["file_count"]
    ):
        raise ContractError("provider file count differs")
    if observed["provider_route"] != contract["provider_probe"]["provider_route"]:
        raise ContractError("provider route differs")
    if observed["check_route"] != contract["provider_probe"]["check_route"]:
        raise ContractError("check route differs")


def collect_evidence(
    typokat: Path, tsgo: Path, output: Path, labels: list[str], window_pause: float,
    contract: dict[str, Any],
) -> dict[str, Any]:
    if output.exists() or output.is_symlink():
        raise ContractError(f"evidence output already exists: {output}")
    if len(labels) != 3 or len(set(labels)) != 3 or any(not label for label in labels):
        raise ContractError("run requires three distinct non-empty trial window labels")
    registry: dict[str, Any] = {}
    host = host_identity()
    build = collect_build_provenance(registry, contract, typokat, tsgo)
    identities = binary_identities(typokat, tsgo, contract)
    provider_probe, provider_match = collect_provider_probe(registry, contract, typokat)
    if not provider_match:
        artifact = {
            "schema": 2, "verdict": "NO-GO",
            "contract_sha256": sha256_file(CONTRACT_PATH),
            "identities": identities, "host": host, "build": build,
            "provider_probe": provider_probe, "invocations": registry,
            "semantics": {}, "controls": {}, "windows": [], "memory": {},
            "final_worktree": collect_final_worktree(registry, contract),
        }
        write_evidence(output, artifact)
        return artifact
    assets, controls = create_control_sources(output)
    semantics, control_records, semantic_match = collect_semantics(
        typokat, tsgo, controls, registry, contract
    )
    artifact: dict[str, Any] = {
        "schema": 2,
        "verdict": "NO-GO",
        "contract_sha256": sha256_file(CONTRACT_PATH),
        "identities": identities,
        "host": host,
        "build": build,
        "provider_probe": provider_probe,
        "invocations": registry,
        "semantics": semantics,
        "controls": control_records,
        "windows": [],
        "memory": {},
        "final_worktree": None,
    }
    if semantic_match:
        artifact["windows"] = collect_timing(
            typokat, tsgo, registry, contract, labels, window_pause
        )
        artifact["memory"] = collect_memory(typokat, tsgo, registry, contract)
        if evidence_performance_passes(artifact, contract):
            artifact["verdict"] = "GO"
    artifact["final_worktree"] = collect_final_worktree(registry, contract)
    if artifact["verdict"] == "GO":
        verify_evidence(artifact, typokat, tsgo, contract, rerun_semantics=False)
    write_evidence(output, artifact)
    return artifact


def write_evidence(output: Path, artifact: dict[str, Any]) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(artifact, indent=2, sort_keys=False) + "\n", encoding="utf-8")


def evidence_performance_passes(artifact: dict[str, Any], contract: dict[str, Any]) -> bool:
    minimum = contract["sampling"]["minimum_speedup"]
    for window in artifact["windows"]:
        for row in ROWS:
            summary = window["rows"][row]["summary"]
            if any(
                summary[key] <= minimum
                for key in ("speedup", "p95_ratio", "bootstrap_lower_95")
            ):
                return False
    for row in ROWS:
        samples = artifact["memory"][row]["samples"]
        values = {
            tool: [sample["rss_kib"] for sample in samples if sample["tool"] == tool]
            for tool in ("typokat", "tsgo")
        }
        if max(values["typokat"]) > contract["sampling"]["max_typokat_rss_kib"]:
            return False
        if statistics.median(values["typokat"]) > (
            contract["sampling"]["max_typokat_to_tsgo_median_rss"] * statistics.median(values["tsgo"])
        ):
            return False
    return True


def verify_evidence(
    evidence: dict[str, Any], typokat: Path, tsgo: Path, contract: dict[str, Any],
    *, rerun_semantics: bool = True,
) -> None:
    exact_keys(evidence, {
        "schema", "verdict", "contract_sha256", "identities", "host", "build",
        "provider_probe", "invocations", "semantics", "controls", "windows", "memory",
        "final_worktree",
    }, "evidence")
    if evidence["schema"] != 2 or evidence["verdict"] != "GO":
        raise ContractError("complete schema-2 GO evidence is required for full validation")
    if evidence["contract_sha256"] != sha256_file(CONTRACT_PATH):
        raise ContractError("evidence contract identity differs")
    actual_identities = binary_identities(typokat, tsgo, contract)
    if evidence["identities"] != actual_identities:
        raise ContractError("evidence is not bound to the supplied real binaries")
    verify_host(evidence["host"])
    registry = verify_invocation_registry(evidence["invocations"], contract)
    seen_pids: set[int] = set()
    used_invocations: set[str] = set()
    verify_build_provenance(
        evidence["build"], registry, used_invocations, seen_pids, typokat, tsgo, contract
    )
    verify_provider_probe(
        evidence["provider_probe"], registry, used_invocations, seen_pids, typokat, contract
    )
    verify_global_chronology(evidence)
    controls = verify_control_assets(evidence["controls"])
    verify_semantic_records(
        evidence["semantics"], evidence["controls"], controls, registry, used_invocations,
        seen_pids, typokat, tsgo, contract, rerun_semantics,
    )
    verify_windows_v2(
        evidence["windows"], registry, used_invocations, seen_pids, typokat, tsgo, contract
    )
    verify_memory_v2(
        evidence["memory"], registry, used_invocations, seen_pids, typokat, tsgo, contract
    )
    verify_final_worktree(
        evidence["final_worktree"], registry, used_invocations, seen_pids
    )
    if used_invocations != set(registry):
        raise ContractError("invocation registry contains unused or unbound commands")


def inspect_evidence(
    evidence: dict[str, Any], typokat: Path, tsgo: Path, contract: dict[str, Any],
) -> None:
    if not isinstance(evidence, dict):
        raise ContractError("evidence must be an object")
    if evidence.get("schema") != 2 or evidence.get("verdict") not in {"GO", "NO-GO"}:
        raise ContractError("unsupported evidence schema or verdict")
    if evidence["verdict"] == "GO":
        verify_evidence(evidence, typokat, tsgo, contract, rerun_semantics=False)
        return
    exact_keys(evidence, {
        "schema", "verdict", "contract_sha256", "identities", "host", "build",
        "provider_probe", "invocations", "semantics", "controls", "windows", "memory",
        "final_worktree",
    }, "evidence")
    if evidence["contract_sha256"] != sha256_file(CONTRACT_PATH):
        raise ContractError("evidence contract identity differs")
    verify_host(evidence["host"])
    verify_invocation_registry(evidence["invocations"], contract)


def require_int(value: Any, label: str, *, minimum: int | None = None) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ContractError(f"{label} must be an integer, not bool/other")
    if minimum is not None and value < minimum:
        raise ContractError(f"{label} is below {minimum}")
    return value


def require_number(value: Any, label: str, *, positive: bool = False) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ContractError(f"{label} must be a finite number, not bool/other")
    number = float(value)
    if positive and number <= 0:
        raise ContractError(f"{label} must be positive")
    return number


def verify_host(host: Any) -> None:
    if not isinstance(host, dict):
        raise ContractError("host identity must be an object")
    exact_keys(host, {
        "hostname", "kernel", "machine", "cpu_model", "cpu_count", "cpu_affinity",
        "nice", "rlimits", "timezone", "started_utc",
    }, "host identity")
    current = host_identity()
    for key in (
        "hostname", "kernel", "machine", "cpu_model", "cpu_count", "cpu_affinity",
        "nice", "rlimits", "timezone",
    ):
        if host[key] != current[key]:
            raise ContractError(f"host.{key} differs from the verification host")
    parse_utc(host["started_utc"], "host.started_utc")


def parse_utc(value: Any, label: str) -> datetime:
    if not isinstance(value, str):
        raise ContractError(f"{label} must be an ISO timestamp")
    try:
        parsed = datetime.fromisoformat(value)
    except ValueError as error:
        raise ContractError(f"{label} is invalid: {error}") from error
    if parsed.tzinfo is None or parsed.utcoffset() != timezone.utc.utcoffset(parsed):
        raise ContractError(f"{label} must carry UTC offset")
    return parsed


def verify_invocation_registry(registry: Any, contract: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(registry, dict) or not registry:
        raise ContractError("invocation registry is empty or malformed")
    for digest, descriptor in registry.items():
        if not isinstance(digest, str) or not HEX64_RE.fullmatch(digest):
            raise ContractError("invocation registry has malformed digest")
        if not isinstance(descriptor, dict):
            raise ContractError("invocation descriptor must be an object")
        exact_keys(
            descriptor, {"argv", "env", "cwd", "affinity", "nice", "rlimits"},
            "invocation descriptor",
        )
        if sha256_bytes(canonical_json(descriptor)) != digest:
            raise ContractError("invocation descriptor digest differs")
        if descriptor["env"] not in (sanitized_environment(), build_environment()):
            raise ContractError("invocation environment is not canonical")
        if any(
            key in descriptor["env"]
            for key in ("RAYON_NUM_THREADS", "GOMAXPROCS", "TOKIO_WORKER_THREADS")
        ):
            raise ContractError("thread-limiting environment is forbidden")
        if descriptor["cwd"] not in {str(ROOT.resolve()), str(ISOLATED_SOURCE_ROOT.resolve())}:
            raise ContractError("invocation environment/cwd is not canonical")
        current = execution_conditions()
        for key in ("affinity", "nice", "rlimits"):
            if descriptor[key] != current[key]:
                raise ContractError(f"invocation {key} is not canonical")
        argv = descriptor["argv"]
        if not isinstance(argv, list) or not argv or any(not isinstance(arg, str) for arg in argv):
            raise ContractError("invocation argv is malformed")
        reject_forbidden_command(argv, contract, allow_incremental_false=False)
    return registry


def verify_control_assets(controls: Any) -> dict[str, list[Path]]:
    if not isinstance(controls, dict) or tuple(controls) != ROWS:
        raise ContractError("control rows are incomplete or reordered")
    paths: dict[str, list[Path]] = {}
    for row in ROWS:
        control = controls[row]
        if not isinstance(control, dict):
            raise ContractError(f"{row}: control evidence is malformed")
        exact_keys(control, {"sources", "tsgo", "typokat"}, f"{row} controls")
        raw_paths = control["sources"]
        if not isinstance(raw_paths, list):
            raise ContractError(f"{row}: control source list is malformed")
        paths[row] = [Path(raw).resolve() for raw in raw_paths]
        originals = row_sources(row)
        if len(paths[row]) != len(originals):
            raise ContractError(f"{row}: control source count differs")
        for path, source in zip(paths[row], originals, strict=True):
            expected = b"// route perturbation; semantics unchanged\n" + source.read_bytes()
            if verify_regular(path, "control source") != expected:
                raise ContractError(f"{row}: control source bytes differ")
    return paths


def verify_process_record(
    record: Any, expected_invocation: str, registry: dict[str, Any], used: set[str],
    seen_pids: set[int], label: str,
) -> ProcessResult:
    if not isinstance(record, dict):
        raise ContractError(f"{label} process record is malformed")
    exact_keys(record, {
        "invocation", "pid", "started_monotonic_ns", "ended_monotonic_ns", "wall_seconds",
        "returncode", "stdout", "stderr", "group_clean", "executable",
    }, f"{label} process record")
    if record["invocation"] != expected_invocation or expected_invocation not in registry:
        raise ContractError(f"{label} invocation binding differs")
    used.add(expected_invocation)
    require_fresh_pid(require_int(record["pid"], f"{label}.pid", minimum=1), seen_pids, label)
    started = require_int(record["started_monotonic_ns"], f"{label}.started", minimum=1)
    ended = require_int(record["ended_monotonic_ns"], f"{label}.ended", minimum=1)
    if ended <= started:
        raise ContractError(f"{label} monotonic interval is invalid")
    wall = require_number(record["wall_seconds"], f"{label}.wall", positive=True)
    compare_float(wall, (ended - started) / 1_000_000_000, f"{label}.wall interval")
    require_int(record["returncode"], f"{label}.returncode")
    if not isinstance(record["stdout"], str) or not isinstance(record["stderr"], str):
        raise ContractError(f"{label} raw output is not text")
    limit = load_contract()["sampling"]["max_output_bytes"]
    if len(record["stdout"].encode()) > limit or len(record["stderr"].encode()) > limit:
        raise ContractError(f"{label} raw output exceeds the live cap")
    if record["group_clean"] is not True:
        raise ContractError(f"{label} process group was not contained")
    executable = record["executable"]
    if not isinstance(executable, dict):
        raise ContractError(f"{label} executable attestation is malformed")
    exact_keys(executable, {"path", "before", "after"}, f"{label} executable")
    expected_path = guarded_executable(registry[expected_invocation]["argv"])
    if executable["path"] != str(expected_path):
        raise ContractError(f"{label} guarded executable path differs")
    current = executable_identity(expected_path)
    if executable["before"] != current or executable["after"] != current:
        raise ContractError(f"{label} executable bytes changed or differ")
    return result_from_record(record, registry[expected_invocation])


def expected_invocation(
    registry: dict[str, Any], argv: list[str], environment: dict[str, str] | None = None,
    cwd: Path = ROOT,
) -> str:
    descriptor = invocation_descriptor(argv, environment, cwd)
    digest = sha256_bytes(canonical_json(descriptor))
    if registry.get(digest) != descriptor:
        raise ContractError("canonical invocation is absent or altered")
    return digest


def build_invocation_environment() -> dict[str, str]:
    return build_environment()


def verify_file_identity(actual: Any, path: Path, label: str) -> None:
    if actual != file_identity(path):
        raise ContractError(f"{label} identity differs")


def verify_build_provenance(
    build: Any, registry: dict[str, Any], used: set[str], seen: set[int],
    typokat: Path, tsgo: Path, contract: dict[str, Any],
) -> None:
    if not isinstance(build, dict):
        raise ContractError("build provenance is malformed")
    exact_keys(build, {
        "worktree", "cargo_version", "rustc_version", "cargo_lock", "cargo_configs",
        "cargo_home_before", "cargo_home_after", "toolchain_files", "effective_build",
        "source_before", "source_after", "binary_before", "compile", "binary_after",
    }, "build provenance")
    worktree_command = ["/usr/bin/git", "status", "--porcelain=v1", "--untracked-files=all"]
    worktree_digest = expected_invocation(registry, worktree_command, cwd=ROOT)
    worktree = verify_process_record(
        build["worktree"], worktree_digest, registry, used, seen, "clean worktree probe"
    )
    if worktree.returncode != 0 or worktree.stdout or worktree.stderr:
        raise ContractError("authoritative run did not start from a clean worktree")
    cargo = cargo_executable()
    rustc = rustc_executable()
    build_env = build_invocation_environment()
    for label, command, record in (
        ("Cargo version", [str(cargo), "--version", "--verbose"], build["cargo_version"]),
        ("rustc version", [str(rustc), "--version", "--verbose"], build["rustc_version"]),
        ("release build", release_build_command(), build["compile"]),
    ):
        digest = expected_invocation(registry, command, build_env, ISOLATED_SOURCE_ROOT)
        result = verify_process_record(record, digest, registry, used, seen, label)
        if result.returncode != 0 or (label != "release build" and result.stderr):
            raise ContractError(f"{label} did not complete canonically")
    verify_file_identity(build["cargo_lock"], ROOT / "Cargo.lock", "Cargo.lock")
    expected_configs = [
        file_identity(path)
        for path in (ROOT / ".cargo/config", ROOT / ".cargo/config.toml")
        if path.exists()
    ]
    if build["cargo_configs"] != expected_configs:
        raise ContractError("Cargo config identities differ")
    current_source = source_snapshot(ISOLATED_SOURCE_ROOT)
    if build["source_before"] != current_source or build["source_after"] != current_source:
        raise ContractError("isolated source snapshot drifted after collection")
    repository_source = source_snapshot(ROOT)
    for key in ("git_commit", "git_tree", "file_count", "source_bytes", "sha256"):
        if current_source[key] != repository_source[key]:
            raise ContractError(f"isolated source snapshot {key} differs from repository HEAD")
    reject_ancestor_cargo_configs(ISOLATED_SOURCE_ROOT)
    verify_cargo_home_record(build["cargo_home_before"], before=True)
    verify_cargo_home_record(build["cargo_home_after"], before=False)
    if build["cargo_home_after"] != canonical_cargo_home_layout():
        raise ContractError("canonical Cargo home layout drifted after collection")
    verify_toolchain_file_identities(build["toolchain_files"])
    if build["effective_build"] != collect_effective_build_fingerprint():
        raise ContractError("effective release fingerprint or rustflags drifted")
    if not isinstance(build["binary_before"], dict) or not isinstance(build["binary_after"], dict):
        raise ContractError("build binary identities are malformed")
    exact_keys(build["binary_before"], {"typokat", "tsgo"}, "pre-build identities")
    exact_keys(build["binary_after"], {"typokat", "tsgo"}, "post-build identities")
    current_typokat = executable_identity(typokat)
    current_tsgo = executable_identity(tsgo)
    if build["binary_before"]["tsgo"] != current_tsgo:
        raise ContractError("comparator bytes before build differ")
    if build["binary_after"] != {"typokat": current_typokat, "tsgo": current_tsgo}:
        raise ContractError("post-build executable bytes differ")
    output = build["effective_build"]["output"]
    if {"size": output["size"], "sha256": output["sha256"]} != current_typokat:
        raise ContractError("installed typokat differs from isolated release output")


def verify_cargo_home_record(record: Any, *, before: bool) -> None:
    if not isinstance(record, dict):
        raise ContractError("canonical Cargo home record is malformed")
    keys = {"path", "entries", "sha256"}
    if before:
        keys |= {"cache_source", "exposed"}
    exact_keys(record, keys, "canonical Cargo home record")
    if record["path"] != str(CANONICAL_CARGO_HOME.resolve()):
        raise ContractError("canonical Cargo home path differs")
    entries = record["entries"]
    if not isinstance(entries, list) or record["sha256"] != sha256_bytes(canonical_json(entries)):
        raise ContractError("canonical Cargo home layout digest differs")
    if any(
        not isinstance(entry, dict) or entry.get("path") in {"config", "config.toml"}
        for entry in entries
    ):
        raise ContractError("canonical Cargo home contains config or malformed entries")
    if before:
        if not isinstance(record["cache_source"], str) or not isinstance(record["exposed"], list):
            raise ContractError("canonical Cargo cache exposure is malformed")
        symlinks = {
            entry["path"]: entry.get("target")
            for entry in entries if entry.get("kind") == "symlink"
        }
        expected = {item.get("path"): item.get("target") for item in record["exposed"]}
        if symlinks != expected:
            raise ContractError("canonical Cargo cache exposure differs from layout")


def verify_provider_probe(
    probe: Any, registry: dict[str, Any], used: set[str], seen: set[int],
    typokat: Path, contract: dict[str, Any],
) -> None:
    if not isinstance(probe, dict):
        raise ContractError("provider probe evidence is malformed")
    exact_keys(probe, {"record", "observed"}, "provider probe evidence")
    command = [str(typokat), *contract["provider_probe"]["args"]]
    digest = expected_invocation(registry, command)
    result = verify_process_record(probe["record"], digest, registry, used, seen, "provider probe")
    if result.returncode != 0 or result.stderr:
        raise ContractError("production provider probe failed")
    observed = strict_json_loads(result.stdout, "provider probe output")
    if observed != probe["observed"]:
        raise ContractError("provider probe raw output and observation differ")
    validate_provider_observation(observed, contract)


def verify_final_worktree(
    record: Any, registry: dict[str, Any], used: set[str], seen: set[int],
) -> None:
    digest = expected_invocation(registry, clean_worktree_command(), cwd=ROOT)
    result = verify_process_record(record, digest, registry, used, seen, "final worktree probe")
    if result.returncode != 0 or result.stdout or result.stderr:
        raise ContractError("worktree was not clean after collection")


def verify_semantic_records(
    semantics: Any, controls_raw: dict[str, Any], controls: dict[str, list[Path]],
    registry: dict[str, Any], used: set[str], seen: set[int], typokat: Path, tsgo: Path,
    contract: dict[str, Any], rerun: bool,
) -> None:
    if not isinstance(semantics, dict) or tuple(semantics) != ROWS:
        raise ContractError("semantic rows are incomplete or reordered")
    oracles = load_oracles()["rows"]
    for row in ROWS:
        semantic = semantics[row]
        exact_keys(semantic, {"inventory", "tsgo", "typokat", "oracle_sha256"}, f"{row} semantics")
        oracle = oracles[row]
        verify_oracle_digest(semantic["oracle_sha256"], row, oracle)
        inventory_argv = tsgo_command(tsgo, row, contract, list_files=True)
        inventory_hash = expected_invocation(registry, inventory_argv)
        inventory_result = verify_process_record(
            semantic["inventory"], inventory_hash, registry, used, seen, f"{row} inventory"
        )
        expected_files = [
            *(tsgo.parent / item.path for item in profile_lock()), *row_sources(row),
        ]
        listed = [Path(line).resolve() for line in inventory_result.stdout.splitlines() if line]
        verify_listed_inventory(listed, [path.resolve() for path in expected_files], row)
        if inventory_result.returncode != 0 or inventory_result.stderr:
            raise ContractError(f"{row}: inventory command failed")
        for tool, binary, command in (
            ("tsgo", tsgo, tsgo_command(tsgo, row, contract)),
            ("typokat", typokat, typokat_command(typokat, row, contract)),
        ):
            digest = expected_invocation(registry, command)
            result = verify_process_record(
                semantic[tool], digest, registry, used, seen, f"{row} {tool} semantic"
            )
            assert_oracle_result(result, tool, row, oracle)
            if rerun:
                live = run_process(
                    command, timeout=contract["sampling"]["timeout_seconds"],
                    max_output=contract["sampling"]["max_output_bytes"],
                )
                assert_oracle_result(live, tool, row, oracle)
        control = controls_raw[row]
        path_map = control_path_map(row, controls[row])
        adjusted = {"exit": oracle["exit"], "diagnostics": adjusted_oracle(row)}
        for tool, command in (
            ("tsgo", [str(tsgo), *contract["typescript_flags"], *map(str, controls[row])]),
            ("typokat", [str(typokat), *contract["typokat_flags"], *map(str, controls[row])]),
        ):
            digest = expected_invocation(registry, command)
            result = verify_process_record(
                control[tool], digest, registry, used, seen, f"{row} {tool} control"
            )
            assert_oracle_result(result, tool, row, adjusted, path_map)
            if rerun:
                live = run_process(
                    command, timeout=contract["sampling"]["timeout_seconds"],
                    max_output=contract["sampling"]["max_output_bytes"],
                )
                assert_oracle_result(live, tool, row, adjusted, path_map)


def assert_oracle_result(
    result: ProcessResult, tool: str, row: str, oracle: dict[str, Any],
    path_map: dict[str, str] | None = None,
) -> None:
    diagnostics = normalize_diagnostics(result, tool, row, path_map)
    if result.returncode != oracle["exit"] or diagnostics != oracle["diagnostics"]:
        raise ContractError(f"{row} {tool}: raw exit/output differs from oracle")


def verify_oracle_digest(actual: Any, row: str, oracle: dict[str, Any] | None = None) -> None:
    oracle = oracle or load_oracles()["rows"][row]
    if actual != sha256_bytes(canonical_json(oracle)):
        raise ContractError(f"{row}: oracle digest differs")


def verify_abba_schedule(records: Any, blocks: int, label: str) -> None:
    if not isinstance(records, list) or len(records) != blocks * 4:
        raise ContractError(f"{label}: complete ABBA schedule required")
    tools = ("typokat", "tsgo", "tsgo", "typokat")
    for ordinal, record in enumerate(records):
        block, slot = divmod(ordinal, 4)
        if not isinstance(record, dict):
            raise ContractError(f"{label}: reordered ABBA schedule")
        actual_block = require_int(record.get("block"), f"{label}.block", minimum=0)
        actual_slot = require_int(record.get("slot"), f"{label}.slot", minimum=0)
        if (actual_block, actual_slot, record.get("tool")) != (block, slot, tools[slot]):
            raise ContractError(f"{label}: reordered ABBA schedule")


def verify_global_chronology(evidence: dict[str, Any]) -> None:
    ordered: list[tuple[str, dict[str, Any]]] = []
    build = evidence["build"]
    for name in ("worktree", "cargo_version", "rustc_version", "compile"):
        ordered.append((f"build.{name}", build[name]))
    ordered.append(("provider_probe", evidence["provider_probe"]["record"]))
    for row in ROWS:
        semantic = evidence["semantics"][row]
        ordered.extend((
            (f"{row}.inventory", semantic["inventory"]),
            (f"{row}.tsgo", semantic["tsgo"]),
            (f"{row}.typokat", semantic["typokat"]),
            (f"{row}.control.tsgo", evidence["controls"][row]["tsgo"]),
            (f"{row}.control.typokat", evidence["controls"][row]["typokat"]),
        ))
    for window_index, window in enumerate(evidence["windows"]):
        for row in ROWS:
            trial = window["rows"][row]
            ordered.extend(
                (f"window.{window_index}.{row}.warmup.{index}", record)
                for index, record in enumerate(trial["warmups"])
            )
            for block, preread in enumerate(trial["pre_reads"]):
                ordered.append((f"window.{window_index}.{row}.preread.{block}", preread))
                ordered.extend(
                    (f"window.{window_index}.{row}.sample.{index}", trial["samples"][index])
                    for index in range(block * 4, block * 4 + 4)
                )
    for row in ROWS:
        memory = evidence["memory"][row]
        for block, preread in enumerate(memory["pre_reads"]):
            ordered.append((f"memory.{row}.preread.{block}", preread))
            ordered.extend(
                (f"memory.{row}.sample.{index}", memory["samples"][index])
                for index in range(block * 4, block * 4 + 4)
            )
    ordered.append(("final_worktree", evidence["final_worktree"]))
    verify_chronological_intervals(ordered)


def verify_chronological_intervals(
    ordered: Iterable[tuple[str, dict[str, Any]]],
) -> None:
    cursor: int | None = None
    for label, record in ordered:
        if not isinstance(record, dict):
            raise ContractError(f"{label}: chronology record is malformed")
        started = require_int(record.get("started_monotonic_ns"), f"{label}.started", minimum=1)
        ended = require_int(record.get("ended_monotonic_ns"), f"{label}.ended", minimum=1)
        if ended < started or (cursor is not None and started < cursor):
            raise ContractError(f"{label}: records overlap or are reordered")
        cursor = ended


def require_window_gap(
    start_utc: datetime, previous_end_utc: datetime,
    started_monotonic_ns: int, previous_ended_monotonic_ns: int,
) -> None:
    utc_gap = (start_utc - previous_end_utc).total_seconds()
    monotonic_gap = (started_monotonic_ns - previous_ended_monotonic_ns) / 1_000_000_000
    if utc_gap < MIN_WINDOW_GAP_SECONDS or monotonic_gap < MIN_WINDOW_GAP_SECONDS:
        raise ContractError("trial windows require a recorded gap of at least 60 seconds")


def verify_windows_v2(
    windows: Any, registry: dict[str, Any], used: set[str], seen: set[int],
    typokat: Path, tsgo: Path, contract: dict[str, Any],
) -> None:
    if not isinstance(windows, list) or len(windows) != 3:
        raise ContractError("three complete trial windows are required")
    labels: set[str] = set()
    previous_utc: datetime | None = None
    previous_monotonic: int | None = None
    oracles = load_oracles()["rows"]
    for trial_index, window in enumerate(windows):
        if not isinstance(window, dict):
            raise ContractError("trial window is malformed")
        exact_keys(window, {
            "label", "started_utc", "ended_utc", "started_monotonic_ns", "ended_monotonic_ns", "rows",
        }, f"trial window {trial_index}")
        label = window["label"]
        if not isinstance(label, str) or not label or label in labels:
            raise ContractError("trial window labels must be distinct and non-empty")
        labels.add(label)
        start_utc = parse_utc(window["started_utc"], "window.started_utc")
        end_utc = parse_utc(window["ended_utc"], "window.ended_utc")
        started = require_int(window["started_monotonic_ns"], "window.started", minimum=1)
        ended = require_int(window["ended_monotonic_ns"], "window.ended", minimum=1)
        if end_utc <= start_utc or ended <= started:
            raise ContractError("trial window interval is empty or reversed")
        if previous_utc is not None:
            require_window_gap(start_utc, previous_utc, started, previous_monotonic)
        previous_utc, previous_monotonic = end_utc, ended
        rows = window["rows"]
        if not isinstance(rows, dict) or tuple(rows) != ROWS:
            raise ContractError("trial window row matrix differs")
        for row_index, row in enumerate(ROWS):
            trial = rows[row]
            exact_keys(trial, {"warmups", "pre_reads", "samples", "summary"}, f"{row} trial")
            verify_prereads(
                trial["pre_reads"], 15, tsgo, typokat, row, started, ended, f"{row} timing"
            )
            warmups = trial["warmups"]
            if not isinstance(warmups, list) or len(warmups) != 10:
                raise ContractError(f"{row}: exactly five alternating warmups per tool required")
            for ordinal, record in enumerate(warmups):
                tool = ("typokat", "tsgo")[ordinal % 2]
                verify_tagged_record(
                    record, tool, row, typokat, tsgo, contract, registry, used, seen,
                    started, ended, oracles[row], f"{row} warmup {ordinal}",
                )
            samples = trial["samples"]
            verify_abba_schedule(samples, 15, f"{row} timing")
            for block, pre_read in enumerate(trial["pre_reads"]):
                first = samples[block * 4]
                if pre_read["ended_monotonic_ns"] > first["started_monotonic_ns"]:
                    raise ContractError(f"{row}: pre-read did not finish before its timing block")
                if block and pre_read["started_monotonic_ns"] < samples[block * 4 - 1]["ended_monotonic_ns"]:
                    raise ContractError(f"{row}: pre-read overlaps the preceding timing block")
            expected_tools = ("typokat", "tsgo", "tsgo", "typokat")
            for ordinal, record in enumerate(samples):
                block, slot = divmod(ordinal, 4)
                if not isinstance(record, dict) or record.get("block") != block or record.get("slot") != slot:
                    raise ContractError(f"{row}: timing block/slot order differs")
                verify_tagged_record(
                    record, expected_tools[slot], row, typokat, tsgo, contract, registry, used,
                    seen, started, ended, oracles[row], f"{row} sample {ordinal}",
                    extra_keys={"block", "slot"},
                )
            verify_trial_summary(trial, row_index, trial_index, contract)


def verify_tagged_record(
    record: dict[str, Any], tool: str, row: str, typokat: Path, tsgo: Path,
    contract: dict[str, Any], registry: dict[str, Any], used: set[str], seen: set[int],
    window_start: int, window_end: int, oracle: dict[str, Any], label: str,
    extra_keys: set[str] | None = None,
) -> ProcessResult:
    extra_keys = extra_keys or set()
    expected_keys = {
        "invocation", "pid", "started_monotonic_ns", "ended_monotonic_ns", "wall_seconds",
        "returncode", "stdout", "stderr", "group_clean", "executable", "tool", *extra_keys,
    }
    if not isinstance(record, dict) or set(record) != expected_keys:
        raise ContractError(f"{label}: tagged record schema differs")
    if record["tool"] != tool:
        raise ContractError(f"{label}: tool order differs")
    command = (
        typokat_command(typokat, row, contract) if tool == "typokat"
        else tsgo_command(tsgo, row, contract)
    )
    digest = expected_invocation(registry, command)
    bare = {key: value for key, value in record.items() if key not in {"tool", *extra_keys}}
    result = verify_process_record(bare, digest, registry, used, seen, label)
    if result.started_monotonic_ns < window_start or result.ended_monotonic_ns > window_end:
        raise ContractError(f"{label}: process lies outside its trial window")
    assert_oracle_result(result, tool, row, oracle)
    return result


def verify_prereads(
    records: Any, count: int, tsgo: Path, typokat: Path, row: str,
    window_start: int | None, window_end: int | None, label: str,
) -> None:
    if not isinstance(records, list) or len(records) != count:
        raise ContractError(f"{label}: missing pre-read attestations")
    expected = preread_attestation(tsgo, typokat, row, 0)
    for block, record in enumerate(records):
        if not isinstance(record, dict):
            raise ContractError(f"{label}: malformed pre-read")
        exact_keys(record, {
            "block", "started_monotonic_ns", "ended_monotonic_ns", "paths", "bytes", "sha256",
        }, f"{label} pre-read")
        actual_block = require_int(record["block"], "pre-read.block", minimum=0)
        if actual_block != block or record["paths"] != expected["paths"]:
            raise ContractError(f"{label}: pre-read order/paths differ")
        actual_bytes = require_int(record["bytes"], "pre-read.bytes", minimum=1)
        if actual_bytes != expected["bytes"] or record["sha256"] != expected["sha256"]:
            raise ContractError(f"{label}: pre-read byte attestation differs")
        started = require_int(record["started_monotonic_ns"], "pre-read.started", minimum=1)
        ended = require_int(record["ended_monotonic_ns"], "pre-read.ended", minimum=1)
        if ended < started:
            raise ContractError(f"{label}: pre-read interval is reversed")
        if window_start is not None and (started < window_start or ended > window_end):
            raise ContractError(f"{label}: pre-read lies outside its window")


def verify_trial_summary(
    trial: dict[str, Any], row_index: int, trial_index: int, contract: dict[str, Any]
) -> None:
    samples = trial["samples"]
    values = {
        tool: [float(record["wall_seconds"]) for record in samples if record["tool"] == tool]
        for tool in ("typokat", "tsgo")
    }
    computed = {tool: summarize(tool_values) for tool, tool_values in values.items()}
    summary = trial["summary"]
    exact_keys(summary, {"typokat", "tsgo", "speedup", "p95_ratio", "bootstrap_lower_95"}, "trial summary")
    for tool in ("typokat", "tsgo"):
        compare_float_maps(summary[tool], computed[tool], f"{tool} summary")
    speedup = computed["tsgo"]["median"] / computed["typokat"]["median"]
    p95_ratio = computed["tsgo"]["p95"] / computed["typokat"]["p95"]
    blocks = [samples[index:index + 4] for index in range(0, 60, 4)]
    lower = bootstrap_block_speedup_lower(
        blocks, contract["sampling"]["bootstrap_resamples"],
        contract["sampling"]["bootstrap_seed"] + row_index * 10 + trial_index,
    )
    compare_float(summary["speedup"], speedup, "speedup")
    compare_float(summary["p95_ratio"], p95_ratio, "p95 ratio")
    compare_float(summary["bootstrap_lower_95"], lower, "block-bootstrap lower")
    minimum = contract["sampling"]["minimum_speedup"]
    if speedup <= minimum or p95_ratio <= minimum or lower <= minimum:
        raise ContractError("strict speedup timing gate failed")


def verify_memory_v2(
    memory: Any, registry: dict[str, Any], used: set[str], seen: set[int],
    typokat: Path, tsgo: Path, contract: dict[str, Any],
) -> None:
    if not isinstance(memory, dict) or tuple(memory) != ROWS:
        raise ContractError("memory row matrix differs")
    oracles = load_oracles()["rows"]
    for row in ROWS:
        evidence = memory[row]
        exact_keys(evidence, {"pre_reads", "samples"}, f"{row} memory")
        verify_prereads(evidence["pre_reads"], 5, tsgo, typokat, row, None, None, f"{row} memory")
        samples = evidence["samples"]
        verify_abba_schedule(samples, 5, f"{row} memory")
        for block, pre_read in enumerate(evidence["pre_reads"]):
            first = samples[block * 4]
            if pre_read["ended_monotonic_ns"] > first["started_monotonic_ns"]:
                raise ContractError(f"{row}: pre-read did not finish before its memory block")
            if block and pre_read["started_monotonic_ns"] < samples[block * 4 - 1]["ended_monotonic_ns"]:
                raise ContractError(f"{row}: pre-read overlaps the preceding memory block")
        expected_tools = ("typokat", "tsgo", "tsgo", "typokat")
        values = {"typokat": [], "tsgo": []}
        for ordinal, record in enumerate(samples):
            block, slot = divmod(ordinal, 4)
            tool = expected_tools[slot]
            expected_keys = {
                "invocation", "pid", "started_monotonic_ns", "ended_monotonic_ns", "wall_seconds",
                "returncode", "stdout", "stderr", "group_clean", "executable", "tool", "block",
                "slot", "rss_kib",
            }
            if not isinstance(record, dict) or set(record) != expected_keys:
                raise ContractError(f"{row}: memory record schema differs")
            if (record["block"], record["slot"], record["tool"]) != (block, slot, tool):
                raise ContractError(f"{row}: memory interleaving/order differs")
            command = [
                "/usr/bin/time", "-v",
                *(typokat_command(typokat, row, contract) if tool == "typokat" else tsgo_command(tsgo, row, contract)),
            ]
            digest = expected_invocation(registry, command)
            bare = {
                key: value for key, value in record.items()
                if key not in {"tool", "block", "slot", "rss_kib"}
            }
            raw = verify_process_record(bare, digest, registry, used, seen, f"{row} memory {ordinal}")
            compiler, rss = compiler_result_without_time(bare, registry[digest])
            require_int(record["rss_kib"], "memory.rss_kib", minimum=1)
            if record["rss_kib"] != rss:
                raise ContractError(f"{row}: memory RSS does not match raw /usr/bin/time output")
            assert_oracle_result(compiler, tool, row, oracles[row])
            values[tool].append(rss)
        if max(values["typokat"]) > contract["sampling"]["max_typokat_rss_kib"]:
            raise ContractError(f"{row}: absolute RSS gate failed")
        if statistics.median(values["typokat"]) > (
            contract["sampling"]["max_typokat_to_tsgo_median_rss"] * statistics.median(values["tsgo"])
        ):
            raise ContractError(f"{row}: relative RSS gate failed")


def require_fresh_pid(pid: Any, seen: set[int], label: str) -> None:
    if not isinstance(pid, int) or pid <= 0 or pid in seen:
        raise ContractError(f"{label}: invalid/reused process id {pid!r}")
    seen.add(pid)


def compare_float_maps(actual: Any, expected: dict[str, float], label: str) -> None:
    if not isinstance(actual, dict) or set(actual) != set(expected):
        raise ContractError(f"{label} fields differ")
    for key, value in expected.items():
        compare_float(actual[key], value, f"{label}.{key}")


def compare_float(actual: Any, expected: float, label: str) -> None:
    if isinstance(actual, bool) or not isinstance(actual, (int, float)) or not math.isclose(
        float(actual), expected, rel_tol=1e-12, abs_tol=1e-12
    ):
        raise ContractError(f"{label} differs: actual={actual!r}, expected={expected!r}")


def contract_summary(contract: dict[str, Any]) -> dict[str, Any]:
    return {
        "rows": list(ROWS),
        "profile": contract["profile"]["length_framed_sha256"],
        "libraries": len(profile_lock()),
        "workload_files": {row: len(files) for row, files in workload_lock().items()},
        "comparator": {
            "version": contract["comparator"]["version"],
            "platform": contract["comparator"]["platform"],
            "sha256": contract["comparator"]["binary_sha256"],
        },
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("verify-contract")
    stage = subparsers.add_parser("stage-comparator")
    stage.add_argument("--package-root", type=Path, required=True)
    stage.add_argument("--destination", type=Path, required=True)
    oracles = subparsers.add_parser("verify-oracles")
    oracles.add_argument("--tsgo", type=Path, required=True)
    production = subparsers.add_parser("assert-production")
    production.add_argument("--typokat", type=Path, required=True)
    red = subparsers.add_parser("assert-red")
    red.add_argument("--typokat", type=Path, required=True)
    run = subparsers.add_parser("run")
    run.add_argument("--typokat", type=Path, required=True)
    run.add_argument("--tsgo", type=Path, required=True)
    run.add_argument("--output", type=Path, required=True)
    run.add_argument("--window-label", action="append", required=True)
    run.add_argument("--window-pause-seconds", type=float, default=MIN_WINDOW_GAP_SECONDS)
    evidence = subparsers.add_parser("inspect-evidence")
    evidence.add_argument("path", type=Path)
    evidence.add_argument("--typokat", type=Path, required=True)
    evidence.add_argument("--tsgo", type=Path, required=True)
    arguments = parser.parse_args(argv)
    try:
        contract = verify_contract()
        if arguments.command == "verify-contract":
            print(json.dumps(contract_summary(contract), indent=2, sort_keys=True))
        elif arguments.command == "stage-comparator":
            binary = stage_comparator(arguments.package_root.resolve(), arguments.destination.resolve(), contract)
            print(binary)
        elif arguments.command == "verify-oracles":
            semantic = verify_semantics(arguments.tsgo.resolve(), "tsgo", contract)
            if not all(value["parity"] for value in semantic.values()):
                raise ContractError("TypeScript comparator differs from committed semantic oracles")
            print(json.dumps(semantic, indent=2, sort_keys=True))
        elif arguments.command == "assert-production":
            assert_production(arguments.typokat.resolve(), contract)
            print("production full-library acceptance: PASS")
        elif arguments.command == "assert-red":
            try:
                assert_production(arguments.typokat.resolve(), contract)
            except ProductionSemanticMismatch as error:
                print(f"expected RED: {error}")
            else:
                raise ContractError("production acceptance unexpectedly passed; retire assert-red")
        elif arguments.command == "run":
            if (
                isinstance(arguments.window_pause_seconds, bool)
                or not math.isfinite(arguments.window_pause_seconds)
                or arguments.window_pause_seconds < MIN_WINDOW_GAP_SECONDS
            ):
                raise ContractError("window pause must be at least 60 seconds")
            artifact = collect_evidence(
                arguments.typokat.resolve(), arguments.tsgo.resolve(), arguments.output.resolve(),
                arguments.window_label, arguments.window_pause_seconds, contract,
            )
            print(f"{artifact['verdict']} evidence written to {arguments.output.resolve()}")
            if artifact["verdict"] != "GO":
                return 1
        elif arguments.command == "inspect-evidence":
            try:
                artifact = strict_json_loads(
                    arguments.path.read_text(encoding="utf-8"), "evidence artifact"
                )
            except OSError as error:
                raise ContractError(f"cannot load evidence: {error}") from error
            inspect_evidence(
                artifact, arguments.typokat.resolve(), arguments.tsgo.resolve(), contract,
            )
            print(
                "evidence inspection: PASS; authoritative result requires a trusted "
                "run/CI log or an independent rerun"
            )
        else:
            raise AssertionError(arguments.command)
    except ContractError as error:
        print(f"full-lib-bench: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

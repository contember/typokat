#!/usr/bin/env python3
"""Fail-closed release coordinator for the production frozen library base."""

from __future__ import annotations

import argparse
import functools
import hashlib
import json
import math
import os
import platform
import pwd
import re
import selectors
import signal
import stat
import subprocess
import sys
import tempfile
import time
import tomllib
from pathlib import Path
from typing import Any, Iterable


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONTRACT_PATH = HERE / "contract.toml"
EVIDENCE_ROOT = HERE / "evidence"
TIME = Path("/usr/bin/time")
GIT = Path("/usr/bin/git")
PYTHON = Path("/usr/bin/python3")
RUST_TOOLCHAIN = "1.95.0"
HEX = frozenset("0123456789abcdef")

BUILD_TIMEOUT_SECONDS = 900
PROBE_TIMEOUT_SECONDS = 30
BUILD_OUTPUT_LIMIT = 16 * 1024 * 1024
PROBE_OUTPUT_LIMIT = 1024 * 1024
EXEC_WRAPPER_CODE = (
    "import os,sys;"
    "p=sys.argv[1];"
    "f=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600);"
    "os.write(f,(str(os.getpid())+'\\n').encode('ascii'));"
    "os.fsync(f);os.close(f);"
    "os.execve(sys.argv[2],sys.argv[2:],"
    "{'PATH':'/usr/bin:/bin','LANG':'C.UTF-8','LC_ALL':'C.UTF-8','TZ':'UTC'})"
)

CONTRACT_KEYS = {
    "schema",
    "clean_builds",
    "release_filter",
    "record_prefix",
    "route",
    "windows",
    "warmups_per_window",
    "samples_per_window",
    "required_total_processes",
    "require_unique_pids",
    "require_nonoverlapping_windows",
    "required_binary_profile",
    "wall_measurement",
    "rss_measurement",
    "p95_wall_us_max",
    "rss_bytes_max",
    "profile_sha256",
    "schema_sha256",
    "artifact_bytes",
    "artifact_sha256",
    "required_initializations",
    "required_publications",
    "required_compiler_invocations",
    "required_generator_invocations",
    "required_source_bytes_read",
    "required_typed_validation_identities",
}
EVIDENCE_KEYS = {
    "schema",
    "contract_sha256",
    "revision",
    "tree",
    "host",
    "toolchain",
    "source",
    "builds",
    "windows",
    "final_source",
}
HOST_KEYS = {"os", "arch", "cpu_count"}
FILE_KEYS = {"path", "bytes", "sha256", "device", "inode"}
TOOL_KEYS = FILE_KEYS | {"invocation", "version"}
SOURCE_KEYS = {
    "root",
    "device",
    "inode",
    "git_commit",
    "git_tree",
    "git_status",
    "tracked_files",
    "tracked_bytes",
    "tracked_sha256",
}
PROCESS_KEYS = {
    "command",
    "cwd",
    "environment",
    "pid",
    "pgid",
    "exit",
    "stdout",
    "stderr",
    "started_monotonic_ns",
    "finished_monotonic_ns",
    "wall_ns",
    "group_clean",
    "failure",
}
BUILD_KEYS = {
    "ordinal",
    "root",
    "device",
    "inode",
    "source_before",
    "source_after",
    "command",
    "environment",
    "process",
    "cargo",
    "rustc",
    "binary",
}
WINDOW_KEYS = {
    "ordinal",
    "started_monotonic_ns",
    "finished_monotonic_ns",
    "records",
}
RECORD_KEYS = {
    "window",
    "ordinal",
    "kind",
    "build",
    "command",
    "launcher_command",
    "environment",
    "wrapper_pid",
    "pgid",
    "pid",
    "exit",
    "stdout",
    "stderr",
    "started_monotonic_ns",
    "finished_monotonic_ns",
    "wall_ns",
    "group_clean",
    "failure",
    "pid_path",
    "time_report_path",
    "time_report",
    "peak_rss_bytes",
    "binary_before",
    "binary_after",
}
PROBE_KEYS = {
    "schema",
    "route",
    "profile_sha256",
    "schema_sha256",
    "artifact_sha256",
    "artifact_bytes",
    "typed_validation_sha256",
    "initializations",
    "publications",
    "compiler_invocations",
    "generator_invocations",
    "source_bytes_read",
    "validation_us",
    "decode_us",
    "publication_us",
}

EXPECTED_CONTRACT: dict[str, Any] = {
    "schema": 1,
    "clean_builds": 2,
    "release_filter": (
        "library::snapshot_base_spec::frozen_library_base_release_probe_once"
    ),
    "record_prefix": "TYPOKAT_LIBRARY_BASE_PROBE=",
    "route": "production-frozen-library-base",
    "windows": 3,
    "warmups_per_window": 5,
    "samples_per_window": 10,
    "required_total_processes": 45,
    "require_unique_pids": True,
    "require_nonoverlapping_windows": True,
    "required_binary_profile": "release",
    "wall_measurement": "coordinator-monotonic-ns",
    "rss_measurement": "/usr/bin/time -v maximum-resident-set-size",
    "p95_wall_us_max": 120_000,
    "rss_bytes_max": 536_870_912,
    "profile_sha256": (
        "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d"
    ),
    "schema_sha256": (
        "6cf27cde368f8b2ff3bdafd5fce8fb3550ec8e2264aab7249362e2294e3f5be0"
    ),
    "artifact_bytes": 21_003_926,
    "artifact_sha256": (
        "47a8a6fd349f3b3fbb3aae1baccedbc67530edc35227707d79afac5395ca7d2f"
    ),
    "required_initializations": 1,
    "required_publications": 1,
    "required_compiler_invocations": 0,
    "required_generator_invocations": 0,
    "required_source_bytes_read": 0,
    "required_typed_validation_identities": 1,
}


class ContractError(RuntimeError):
    """The release evidence does not satisfy the frozen contract."""


class RecordedProcessError(ContractError):
    """A bounded child failed after producing inspectable process evidence."""

    def __init__(self, message: str, record: dict[str, Any]):
        super().__init__(message)
        self.record = record


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


def _string(value: Any, label: str, *, allow_empty: bool = False) -> str:
    if not isinstance(value, str) or (not allow_empty and not value):
        suffix = "a string" if allow_empty else "a nonempty string"
        raise ContractError(f"{label} must be {suffix}")
    return value


def _digest(value: Any, label: str) -> str:
    text = _string(value, label)
    if len(text) != 64 or any(character not in HEX for character in text):
        raise ContractError(f"{label} must be a lowercase SHA-256 digest")
    return text


def _strict_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"JSON contains duplicate key {key!r}")
        result[key] = value
    return result


def _reject_constant(value: str) -> Any:
    raise ContractError(f"JSON contains non-finite value {value}")


def strict_json(text: str, label: str) -> Any:
    try:
        return json.loads(
            text,
            object_pairs_hook=_strict_pairs,
            parse_constant=_reject_constant,
        )
    except (json.JSONDecodeError, ContractError) as error:
        raise ContractError(f"cannot parse {label}: {error}") from error


def validate_contract(contract: Any) -> dict[str, Any]:
    value = _exact_keys(contract, CONTRACT_KEYS, "library-base contract")
    for key, expected in EXPECTED_CONTRACT.items():
        actual = value[key]
        if isinstance(expected, bool):
            if type(actual) is not bool or actual is not expected:
                raise ContractError(f"contract {key} differs")
        elif isinstance(expected, int):
            if _integer(actual, f"contract {key}") != expected:
                raise ContractError(f"contract {key} differs")
        elif not isinstance(actual, str) or actual != expected:
            raise ContractError(f"contract {key} differs")
    for key in ("profile_sha256", "schema_sha256", "artifact_sha256"):
        _digest(value[key], f"contract {key}")
    expected_processes = value["windows"] * (
        value["warmups_per_window"] + value["samples_per_window"]
    )
    if value["required_total_processes"] != expected_processes:
        raise ContractError("contract process total differs from its schedule")
    return value


def load_contract(path: Path = CONTRACT_PATH) -> dict[str, Any]:
    try:
        data = _read_regular(path)
        contract = tomllib.loads(data.decode("utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ContractError(f"cannot load library-base contract: {error}") from error
    return validate_contract(contract)


def _extract_probe(stdout: str, prefix: str) -> dict[str, Any]:
    if len(stdout.encode("utf-8")) > PROBE_OUTPUT_LIMIT:
        raise ContractError("probe stdout exceeds its evidence cap")
    if stdout.count(prefix) != 1:
        raise ContractError("probe must emit exactly one JSON record prefix")
    if stdout.count("running 1 test") != 1:
        raise ContractError("probe did not run exactly one test")
    if stdout.count("test result: ok. 1 passed; 0 failed; 0 ignored") != 1:
        raise ContractError("probe did not report exactly one passing ignored test")
    tail = stdout.split(prefix, 1)[1]
    decoder = json.JSONDecoder(
        object_pairs_hook=_strict_pairs,
        parse_constant=_reject_constant,
    )
    try:
        parsed, end = decoder.raw_decode(tail)
    except (json.JSONDecodeError, ContractError) as error:
        raise ContractError(f"probe emitted malformed JSON: {error}") from error
    if end < len(tail) and not tail[end].isspace():
        raise ContractError("probe JSON has a malformed record boundary")
    return _exact_keys(parsed, PROBE_KEYS, "probe record")


def _validate_probe(probe: Any, contract: dict[str, Any]) -> dict[str, Any]:
    value = _exact_keys(probe, PROBE_KEYS, "probe record")
    expected = {
        "schema": contract["schema"],
        "route": contract["route"],
        "profile_sha256": contract["profile_sha256"],
        "schema_sha256": contract["schema_sha256"],
        "artifact_sha256": contract["artifact_sha256"],
        "artifact_bytes": contract["artifact_bytes"],
        "initializations": contract["required_initializations"],
        "publications": contract["required_publications"],
        "compiler_invocations": contract["required_compiler_invocations"],
        "generator_invocations": contract["required_generator_invocations"],
        "source_bytes_read": contract["required_source_bytes_read"],
    }
    for key, wanted in expected.items():
        actual = value[key]
        if isinstance(wanted, int):
            actual = _integer(actual, f"probe {key}")
        elif not isinstance(actual, str):
            raise ContractError(f"probe {key} must be a string")
        if actual != wanted:
            raise ContractError(f"probe {key} differs")
    _digest(value["typed_validation_sha256"], "probe typed validation")
    for key in ("validation_us", "decode_us", "publication_us"):
        _integer(value[key], f"probe {key}", 1)
    return value


def _canonical_absolute_path(value: Any, label: str) -> str:
    text = _string(value, label)
    path = Path(text)
    if not path.is_absolute() or os.path.normpath(text) != text:
        raise ContractError(f"{label} must be a canonical absolute path")
    return text


def _nearest_rank_p95(values: list[int]) -> int:
    if not values:
        raise ContractError("cannot calculate p95 from an empty sample")
    ordered = sorted(values)
    return ordered[math.ceil(0.95 * len(ordered)) - 1]


def _expected_probe_command(binary: str, contract: dict[str, Any]) -> list[str]:
    return [
        binary,
        contract["release_filter"],
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]


def _same_source_content(left: dict[str, Any], right: dict[str, Any]) -> bool:
    keys = {"git_commit", "git_tree", "tracked_files", "tracked_bytes", "tracked_sha256"}
    return {key: left[key] for key in keys} == {key: right[key] for key in keys}


def _validate_process(value: Any, label: str) -> dict[str, Any]:
    process = _exact_keys(value, PROCESS_KEYS, label)
    if not isinstance(process["command"], list) or any(
        not isinstance(part, str) or not part for part in process["command"]
    ):
        raise ContractError(f"{label} command is malformed")
    _canonical_absolute_path(process["command"][0], f"{label} executable")
    _canonical_absolute_path(process["cwd"], f"{label} cwd")
    if not isinstance(process["environment"], dict) or any(
        not isinstance(key, str) or not isinstance(item, str)
        for key, item in process["environment"].items()
    ):
        raise ContractError(f"{label} environment is malformed")
    for key in ("pid", "pgid", "started_monotonic_ns", "finished_monotonic_ns", "wall_ns"):
        _integer(process[key], f"{label} {key}", 1)
    if process["pgid"] != process["pid"]:
        raise ContractError(f"{label} process group differs from its leader")
    if monotonic_wall_ns(
        process["started_monotonic_ns"], process["finished_monotonic_ns"]
    ) != process["wall_ns"]:
        raise ContractError(f"{label} monotonic wall interval differs")
    _integer(process["exit"], f"{label} exit")
    _string(process["stdout"], f"{label} stdout", allow_empty=True)
    _string(process["stderr"], f"{label} stderr", allow_empty=True)
    if type(process["group_clean"]) is not bool:
        raise ContractError(f"{label} group-clean flag is malformed")
    if process["failure"] is not None and not isinstance(process["failure"], str):
        raise ContractError(f"{label} failure is malformed")
    return process


def validate_evidence(
    evidence: Any, contract: dict[str, Any], *, require_live: bool = True
) -> dict[str, Any]:
    """Validate raw records and recompute every identity and threshold decision."""

    validate_contract(contract)
    value = _exact_keys(evidence, EVIDENCE_KEYS, "library-base evidence")
    if _integer(value["schema"], "evidence schema") != contract["schema"]:
        raise ContractError("evidence schema differs")
    contract_digest = _digest(value["contract_sha256"], "contract digest")
    if contract_digest != _file_identity(CONTRACT_PATH)[1]:
        raise ContractError("evidence contract identity differs")

    source = _validate_source_snapshot(value["source"], "source before", require_live=require_live)
    final_source = _validate_source_snapshot(
        value["final_source"], "source after", require_live=require_live
    )
    if source != final_source:
        raise ContractError("authoritative source changed during the gate")
    if value["revision"] != source["git_commit"] or value["tree"] != source["git_tree"]:
        raise ContractError("evidence revision/tree differs from live source provenance")

    host = _exact_keys(value["host"], HOST_KEYS, "host evidence")
    if host["os"] != "linux":
        raise ContractError("the release gate requires Linux")
    _string(host["arch"], "host architecture")
    _integer(host["cpu_count"], "host CPU count", 1)

    toolchain = _exact_keys(
        value["toolchain"], {"python", "cargo", "rustc", "git", "time"}, "toolchain evidence"
    )
    for name in ("python", "cargo", "rustc", "git", "time"):
        _validate_tool_identity(toolchain[name], f"{name} tool", require_live=require_live)
    if toolchain["python"]["invocation"] != str(PYTHON):
        raise ContractError("Python invocation is not the trusted absolute interpreter")
    if toolchain["git"]["invocation"] != str(GIT) or toolchain["time"]["invocation"] != str(TIME):
        raise ContractError("Git/time invocation paths differ")
    if require_live and toolchain != resolve_trusted_tools():
        raise ContractError("retained toolchain identities differ from trusted live tools")

    builds = value["builds"]
    if not isinstance(builds, list) or len(builds) != contract["clean_builds"]:
        raise ContractError("exactly two clean release builds are required")
    build_by_ordinal: dict[int, dict[str, Any]] = {}
    roots: list[Path] = []
    cargo_homes: list[Path] = []
    binaries: list[dict[str, Any]] = []
    for expected_ordinal, raw_build in enumerate(builds, 1):
        build = _exact_keys(raw_build, BUILD_KEYS, "release build evidence")
        if _integer(build["ordinal"], "release build ordinal", 1) != expected_ordinal:
            raise ContractError("release build ordinals differ")
        root = Path(_canonical_absolute_path(build["root"], "release build root"))
        _integer(build["device"], "release build device")
        _integer(build["inode"], "release build inode", 1)
        if require_live:
            try:
                info = root.lstat()
            except OSError as error:
                raise ContractError(f"cannot inspect release build root: {error}") from error
            if not stat.S_ISDIR(info.st_mode) or (info.st_dev, info.st_ino) != (
                build["device"], build["inode"]
            ):
                raise ContractError("release build root identity differs")
        roots.append(root)
        before = _validate_source_snapshot(
            build["source_before"], "build source before", require_live=require_live
        )
        after = _validate_source_snapshot(
            build["source_after"], "build source after", require_live=require_live
        )
        if before != after or before["root"] != str(root) or not _same_source_content(before, source):
            raise ContractError("release build source provenance differs")
        if build["cargo"] != toolchain["cargo"] or build["rustc"] != toolchain["rustc"]:
            raise ContractError("release build toolchain identity differs")
        command = build["command"]
        expected_command = [
            toolchain["cargo"]["invocation"], "test", "--release", "--locked",
            "--offline", "--lib", "--no-run", "--message-format=json-render-diagnostics",
        ]
        if command != expected_command:
            raise ContractError("release build command differs")
        environment = build["environment"]
        if not isinstance(environment, dict):
            raise ContractError("release build environment is malformed")
        cargo_home = Path(_canonical_absolute_path(environment.get("CARGO_HOME"), "Cargo home"))
        cargo_homes.append(cargo_home)
        process = _validate_process(build["process"], "release build process")
        if (
            len(process["stdout"].encode("utf-8")) > BUILD_OUTPUT_LIMIT
            or len(process["stderr"].encode("utf-8")) > BUILD_OUTPUT_LIMIT
        ):
            raise ContractError("release build output exceeds its retained cap")
        if (
            process["command"] != command
            or process["cwd"] != str(root)
            or process["environment"] != environment
            or process["exit"] != 0
            or process["failure"] is not None
            or not process["group_clean"]
        ):
            raise ContractError("release build process did not complete exactly")
        binary = _validate_file_identity(
            build["binary"], "release libtest", require_live=require_live
        )
        selected = _select_libtest(
            process["stdout"], root / "target", require_live=require_live
        )
        if str(selected) != binary["path"]:
            raise ContractError("retained release libtest differs from raw Cargo output")
        binary_path = Path(binary["path"])
        try:
            parts = binary_path.relative_to(root).parts
        except ValueError as error:
            raise ContractError("release libtest escaped its build root") from error
        if len(parts) != 4 or parts[:3] != ("target", "release", "deps") or not parts[3].startswith("typokat-"):
            raise ContractError("release build selected the wrong libtest")
        binaries.append(binary)
        build_by_ordinal[expected_ordinal] = build

    if len({(build["device"], build["inode"]) for build in builds}) != len(builds):
        raise ContractError("release build roots are not physically distinct")
    if len({build["process"]["pid"] for build in builds}) != len(builds):
        raise ContractError("release build processes reused a PID")
    for left, right in ((roots[0], roots[1]),):
        if left == right or left in right.parents or right in left.parents:
            raise ContractError("release build roots overlap")
    if len(set(cargo_homes)) != len(cargo_homes):
        raise ContractError("release builds reused a Cargo home")
    flags = [
        *(f"--remap-path-prefix={root}=/typokat-library-base/build" for root in roots),
        *(f"--remap-path-prefix={home}=/typokat-library-base/cargo" for home in cargo_homes),
        "--remap-path-scope=all",
    ]
    for build, root, cargo_home in zip(builds, roots, cargo_homes, strict=True):
        expected_environment = _sanitized_environment() | {
            "CARGO_HOME": str(cargo_home),
            "CARGO_TARGET_DIR": str(root / "target"),
            "CARGO_NET_OFFLINE": "true",
            "CARGO_INCREMENTAL": "0",
            "CARGO_TERM_COLOR": "never",
            "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(flags),
            "CARGO_BUILD_RUSTFLAGS": "",
            "RUSTC": toolchain["rustc"]["invocation"],
        }
        if build["environment"] != expected_environment:
            raise ContractError("release build environment differs")
    if binaries[0]["path"] == binaries[1]["path"] or {
        key: binaries[0][key] for key in ("bytes", "sha256")
    } != {key: binaries[1][key] for key in ("bytes", "sha256")}:
        raise ContractError("release libtests are reused or not byte-identical")
    if (binaries[0]["device"], binaries[0]["inode"]) == (
        binaries[1]["device"], binaries[1]["inode"]
    ):
        raise ContractError("release builds reused one physical libtest")
    if require_live and _read_regular(Path(binaries[0]["path"])) != _read_regular(Path(binaries[1]["path"])):
        raise ContractError("release libtest byte comparison differs")

    windows = value["windows"]
    if not isinstance(windows, list) or len(windows) != contract["windows"]:
        raise ContractError("timing window count differs")
    seen_pids: set[int] = set()
    seen_wrappers: set[int] = set()
    seen_pid_paths: set[str] = set()
    seen_report_paths: set[str] = set()
    used_builds: set[int] = set()
    typed_validation_identities: set[str] = set()
    sample_walls_ns: list[int] = []
    window_p95s_ns: list[int] = []
    rss_values: list[int] = []
    previous_finish: int | None = None
    process_count = 0
    for expected_window, raw_window in enumerate(windows, 1):
        window = _exact_keys(raw_window, WINDOW_KEYS, "timing window")
        if _integer(window["ordinal"], "timing window ordinal", 1) != expected_window:
            raise ContractError("timing window ordinals differ")
        started = _integer(window["started_monotonic_ns"], "timing window start", 1)
        finished = _integer(window["finished_monotonic_ns"], "timing window finish", 1)
        if finished <= started or (previous_finish is not None and started < previous_finish):
            raise ContractError("timing windows overlap, reverse, or are empty")
        previous_finish = finished
        records = window["records"]
        expected_records = contract["warmups_per_window"] + contract["samples_per_window"]
        if not isinstance(records, list) or len(records) != expected_records:
            raise ContractError("timing window schedule is incomplete")
        cursor = started
        window_walls: list[int] = []
        for expected_record, raw_record in enumerate(records, 1):
            record = _exact_keys(raw_record, RECORD_KEYS, "probe process record")
            process_count += 1
            if record["window"] != expected_window or record["ordinal"] != expected_record:
                raise ContractError("probe schedule ordinals differ")
            expected_kind = "warmup" if expected_record <= contract["warmups_per_window"] else "sample"
            if record["kind"] != expected_kind:
                raise ContractError("warmup/sample schedule differs")
            build_ordinal = _integer(record["build"], "probe build", 1)
            if build_ordinal not in build_by_ordinal:
                raise ContractError("probe selected an unknown build")
            used_builds.add(build_ordinal)
            build = build_by_ordinal[build_ordinal]
            binary = binaries[build_ordinal - 1]
            if record["binary_before"] != binary or record["binary_after"] != binary:
                raise ContractError("release libtest was not sealed around each launch")
            command = record["command"]
            if command != _expected_probe_command(binary["path"], contract):
                raise ContractError("probe did not use the exact release filter")
            environment = record["environment"]
            if environment != _sanitized_environment():
                raise ContractError("probe environment is not exactly sanitized")
            pid_path = _canonical_absolute_path(record["pid_path"], "exec PID path")
            report_path = _canonical_absolute_path(record["time_report_path"], "time report path")
            if pid_path in seen_pid_paths or report_path in seen_report_paths:
                raise ContractError("probe PID/time evidence path was reused")
            seen_pid_paths.add(pid_path)
            seen_report_paths.add(report_path)
            launcher = [
                toolchain["time"]["invocation"], "-v", "-o", report_path, "--",
                toolchain["python"]["invocation"], "-I", "-S", "-c",
                EXEC_WRAPPER_CODE, pid_path, *command,
            ]
            if record["launcher_command"] != launcher:
                raise ContractError("probe launcher command differs")
            wrapper_pid = _integer(record["wrapper_pid"], "time wrapper PID", 1)
            pgid = _integer(record["pgid"], "probe process group", 1)
            pid = _integer(record["pid"], "libtest PID", 1)
            if wrapper_pid != pgid or pid == wrapper_pid:
                raise ContractError("libtest PID was confused with the time wrapper")
            if pid in seen_pids or wrapper_pid in seen_wrappers:
                raise ContractError("a fresh libtest or wrapper PID was reused")
            seen_pids.add(pid)
            seen_wrappers.add(wrapper_pid)
            if require_live and read_exec_pid(Path(pid_path)) != pid:
                raise ContractError("retained libtest PID differs from exec wrapper evidence")
            wall_ns = monotonic_wall_ns(
                record["started_monotonic_ns"], record["finished_monotonic_ns"]
            )
            if record["wall_ns"] != wall_ns:
                raise ContractError("probe wall time differs from coordinator monotonic evidence")
            if record["started_monotonic_ns"] < cursor or record["finished_monotonic_ns"] > finished:
                raise ContractError("probe intervals overlap or escape the timing window")
            cursor = record["finished_monotonic_ns"]
            stdout = _string(record["stdout"], "probe stdout", allow_empty=True)
            stderr = _string(record["stderr"], "probe stderr", allow_empty=True)
            if (
                record["exit"] != 0
                or stderr
                or record["failure"] is not None
                or record["group_clean"] is not True
            ):
                raise ContractError("probe process did not complete exactly")
            if len(stdout.encode()) > PROBE_OUTPUT_LIMIT or len(stderr.encode()) > PROBE_OUTPUT_LIMIT:
                raise ContractError("probe output exceeds its retained cap")
            time_report = _string(record["time_report"], "raw /usr/bin/time report")
            if require_live and _read_regular(Path(report_path)).decode("utf-8") != time_report:
                raise ContractError("raw /usr/bin/time report differs from its file")
            peak_rss = parse_time_rss_bytes(time_report)
            if record["peak_rss_bytes"] != peak_rss or peak_rss > contract["rss_bytes_max"]:
                raise ContractError("external peak RSS evidence differs or exceeds 512 MiB")
            rss_values.append(peak_rss)
            if expected_kind == "sample":
                sample_walls_ns.append(wall_ns)
                window_walls.append(wall_ns)
            probe = _validate_probe(_extract_probe(stdout, contract["record_prefix"]), contract)
            typed_validation_identities.add(probe["typed_validation_sha256"])
        window_p95 = _nearest_rank_p95(window_walls)
        if window_p95 > contract["p95_wall_us_max"] * 1000:
            raise ContractError("per-window monotonic p95 exceeds 120 ms")
        window_p95s_ns.append(window_p95)

    if process_count != contract["required_total_processes"] or len(seen_pids) != process_count:
        raise ContractError("fresh-process schedule or libtest PID count differs")
    if used_builds != set(build_by_ordinal):
        raise ContractError("both independent release builds must be sampled")
    if len(typed_validation_identities) != contract["required_typed_validation_identities"]:
        raise ContractError("typed-validation identity changed between processes")
    overall_p95_ns = _nearest_rank_p95(sample_walls_ns)
    if overall_p95_ns > contract["p95_wall_us_max"] * 1000:
        raise ContractError("overall monotonic p95 exceeds 120 ms")
    return {
        "outcome": "GO",
        "windows": len(windows),
        "processes": process_count,
        "samples": len(sample_walls_ns),
        "p95_wall_us": (overall_p95_ns + 999) // 1000,
        "window_p95_wall_us": [(value + 999) // 1000 for value in window_p95s_ns],
        "peak_rss_bytes": max(rss_values),
        "typed_validation_identities": len(typed_validation_identities),
        "initializations": contract["required_initializations"],
        "publications": contract["required_publications"],
    }


def _sanitized_environment() -> dict[str, str]:
    return {
        "PATH": "/usr/bin:/bin",
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "TZ": "UTC",
    }


def _group_exists(pid: int) -> bool:
    try:
        os.killpg(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _kill_group(pid: int) -> None:
    try:
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def _wait_group_gone(pid: int) -> bool:
    deadline = time.monotonic() + 1.0
    while _group_exists(pid) and time.monotonic() < deadline:
        time.sleep(0.005)
    return not _group_exists(pid)


def run_bounded_record(
    command: Iterable[str],
    *,
    cwd: Path,
    environment: dict[str, str],
    timeout: int,
    stdout_limit: int,
    stderr_limit: int,
) -> dict[str, Any]:
    argv = tuple(str(part) for part in command)
    if not argv or any(not part or "\0" in part for part in argv):
        raise ContractError("invalid subprocess command")
    if not Path(argv[0]).is_absolute():
        raise ContractError("subprocess executable must be an absolute path")
    started = time.monotonic_ns()
    try:
        process = subprocess.Popen(
            argv,
            cwd=cwd,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
            close_fds=True,
        )
    except OSError as error:
        raise ContractError(f"cannot start subprocess {argv!r}: {error}") from error
    assert process.stdout is not None and process.stderr is not None
    streams = (process.stdout, process.stderr)
    buffers = {process.stdout: bytearray(), process.stderr: bytearray()}
    limits = {process.stdout: stdout_limit, process.stderr: stderr_limit}
    selector = selectors.DefaultSelector()
    for stream in streams:
        os.set_blocking(stream.fileno(), False)
        selector.register(stream, selectors.EVENT_READ)
    deadline = time.monotonic() + timeout
    failure: str | None = None
    try:
        while selector.get_map() or process.poll() is None:
            now = time.monotonic()
            if now >= deadline:
                failure = f"subprocess timed out after {timeout}s"
                _kill_group(process.pid)
                break
            for key, _ in selector.select(min(0.05, deadline - now)):
                stream = key.fileobj
                room = limits[stream] - len(buffers[stream])
                try:
                    chunk = os.read(stream.fileno(), 65_536)
                except BlockingIOError:
                    continue
                if not chunk:
                    selector.unregister(stream)
                    stream.close()
                    continue
                buffers[stream].extend(chunk[: max(0, room)])
                if len(chunk) > room:
                    failure = "subprocess output exceeded its live cap"
                    _kill_group(process.pid)
                    break
            if failure is not None:
                break
        if failure is not None:
            _kill_group(process.pid)
        try:
            returncode = process.wait(timeout=2)
        except subprocess.TimeoutExpired as error:
            _kill_group(process.pid)
            process.wait(timeout=2)
            failure = "subprocess leader survived process-group kill"
            returncode = process.returncode if process.returncode is not None else -signal.SIGKILL
    finally:
        selector.close()
        for stream in streams:
            try:
                stream.close()
            except OSError:
                pass
    ended = time.monotonic_ns()
    group_clean = _wait_group_gone(process.pid)
    if not group_clean:
        _kill_group(process.pid)
        failure = failure or "subprocess process group survived leader exit"
        _kill_group(process.pid)
    try:
        stdout = bytes(buffers[process.stdout]).decode("utf-8")
        stderr = bytes(buffers[process.stderr]).decode("utf-8")
    except UnicodeDecodeError as error:
        failure = failure or f"subprocess emitted malformed UTF-8: {error}"
        stdout = bytes(buffers[process.stdout]).decode("utf-8", errors="replace")
        stderr = bytes(buffers[process.stderr]).decode("utf-8", errors="replace")
    return {
        "command": list(argv),
        "cwd": str(cwd.resolve()),
        "environment": dict(environment),
        "pid": process.pid,
        "pgid": process.pid,
        "exit": returncode,
        "stdout": stdout,
        "stderr": stderr,
        "started_monotonic_ns": started,
        "finished_monotonic_ns": ended,
        "wall_ns": ended - started,
        "group_clean": group_clean,
        "failure": failure,
    }


def _run_bounded(
    command: Iterable[str],
    *,
    cwd: Path,
    environment: dict[str, str],
    timeout: int,
    stdout_limit: int,
    stderr_limit: int,
) -> dict[str, Any]:
    record = run_bounded_record(
        command,
        cwd=cwd,
        environment=environment,
        timeout=timeout,
        stdout_limit=stdout_limit,
        stderr_limit=stderr_limit,
    )
    if record["failure"] is not None:
        raise ContractError(record["failure"])
    return record


def _run_checked(
    command: Iterable[str],
    *,
    cwd: Path,
    environment: dict[str, str],
    timeout: int = 120,
    output_limit: int = 1024 * 1024,
) -> dict[str, Any]:
    result = _run_bounded(
        command,
        cwd=cwd,
        environment=environment,
        timeout=timeout,
        stdout_limit=output_limit,
        stderr_limit=output_limit,
    )
    if result["exit"] != 0:
        raise ContractError(
            f"command failed ({result['exit']}): {list(command)!r}\n"
            f"stdout:\n{result['stdout'][-8192:]}\n"
            f"stderr:\n{result['stderr'][-8192:]}"
        )
    return result


def _successful_output(
    command: list[str], environment: dict[str, str], *, label: str
) -> str:
    record = run_bounded_record(
        command,
        cwd=ROOT,
        environment=environment,
        timeout=10,
        stdout_limit=64 * 1024,
        stderr_limit=64 * 1024,
    )
    if record["failure"] is not None or record["exit"] != 0:
        raise ContractError(f"cannot attest {label}: {record['failure'] or record['stderr']}")
    output = (record["stdout"] + record["stderr"]).strip()
    if not output:
        raise ContractError(f"{label} emitted no version identity")
    return output


@functools.lru_cache(maxsize=1)
def resolve_trusted_tools() -> dict[str, dict[str, Any]]:
    """Resolve tools without consulting caller-controlled PATH entries."""

    if not PYTHON.is_absolute() or not PYTHON.exists():
        raise ContractError("trusted /usr/bin/python3 is unavailable")
    if Path(sys.executable).resolve() != PYTHON.resolve():
        raise ContractError("coordinator must run under /usr/bin/python3")
    account_home = Path(pwd.getpwuid(os.getuid()).pw_dir).resolve()
    rustup = account_home / ".cargo/bin/rustup"
    if not rustup.is_absolute() or not rustup.exists():
        raise ContractError("trusted rustup installation is unavailable")
    rust_environment = _sanitized_environment() | {
        "HOME": str(account_home),
        "RUSTUP_HOME": str(account_home / ".rustup"),
    }

    def rustup_which(name: str) -> Path:
        output = _successful_output(
            [str(rustup), "which", "--toolchain", RUST_TOOLCHAIN, name],
            rust_environment,
            label=f"pinned {name}",
        )
        path = Path(output)
        if not path.is_absolute() or not path.exists():
            raise ContractError(f"rustup returned an invalid {name} path")
        expected_root = (account_home / ".rustup/toolchains").resolve()
        try:
            path.resolve().relative_to(expected_root)
        except ValueError as error:
            raise ContractError(f"pinned {name} escaped the rustup toolchain root") from error
        return path

    cargo = rustup_which("cargo")
    rustc = rustup_which("rustc")
    paths = {
        "python": PYTHON,
        "cargo": cargo,
        "rustc": rustc,
        "git": GIT,
        "time": TIME,
    }
    version_commands = {
        "python": [str(PYTHON), "--version"],
        "cargo": [str(cargo), "--version", "--verbose"],
        "rustc": [str(rustc), "--version", "--verbose"],
        "git": [str(GIT), "--version"],
        "time": [str(TIME), "--version"],
    }
    result: dict[str, dict[str, Any]] = {}
    for name, path in paths.items():
        identity = file_identity(path)
        environment = rust_environment if name in ("cargo", "rustc") else _sanitized_environment()
        version = _successful_output(
            version_commands[name], environment, label=f"{name} version"
        )
        result[name] = {
            **identity,
            "invocation": str(path),
            "version": version,
        }
    if not result["cargo"]["version"].startswith(f"cargo {RUST_TOOLCHAIN} "):
        raise ContractError("Cargo version differs from the pinned toolchain")
    if not result["rustc"]["version"].startswith(f"rustc {RUST_TOOLCHAIN} "):
        raise ContractError("rustc version differs from the pinned toolchain")
    return result


def _git(root: Path, arguments: list[str]) -> str:
    environment = _sanitized_environment()
    environment["GIT_OPTIONAL_LOCKS"] = "0"
    return _run_checked(
        [str(GIT), "--no-optional-locks", *arguments],
        cwd=root,
        environment=environment,
    )["stdout"]


def _git_status(root: Path) -> str:
    return _git(root, ["status", "--porcelain=v1", "--untracked-files=all"])


def _require_clean_repository() -> str:
    status = _git_status(ROOT)
    if status:
        raise ContractError(f"library-base release gate requires clean HEAD:\n{status}")
    revision = _git(ROOT, ["rev-parse", "HEAD"]).strip()
    if len(revision) != 40 or any(character not in HEX for character in revision):
        raise ContractError("cannot resolve a canonical Git revision")
    return revision


def _read_regular(path: Path) -> bytes:
    try:
        before = path.lstat()
    except OSError as error:
        raise ContractError(f"cannot inspect {path}: {error}") from error
    if not stat.S_ISREG(before.st_mode):
        raise ContractError(f"expected a regular file: {path}")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise ContractError(f"cannot safely open {path}: {error}") from error
    try:
        opened = os.fstat(descriptor)
        chunks: list[bytes] = []
        while chunk := os.read(descriptor, 1024 * 1024):
            chunks.append(chunk)
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    try:
        final = path.lstat()
    except OSError as error:
        raise ContractError(f"file disappeared after reading: {path}: {error}") from error
    identity = lambda item: (
        item.st_dev,
        item.st_ino,
        item.st_mode,
        item.st_nlink,
        item.st_size,
        item.st_mtime_ns,
        item.st_ctime_ns,
    )
    if not (identity(before) == identity(opened) == identity(after) == identity(final)):
        raise ContractError(f"file changed while reading: {path}")
    return b"".join(chunks)


def _file_identity(path: Path) -> tuple[int, str]:
    data = _read_regular(path)
    return len(data), hashlib.sha256(data).hexdigest()


def file_identity(path: Path) -> dict[str, Any]:
    resolved = path.resolve()
    data = _read_regular(resolved)
    info = resolved.stat()
    return {
        "path": str(resolved),
        "bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "device": info.st_dev,
        "inode": info.st_ino,
    }


def require_same_identity(left: Any, right: Any, label: str) -> None:
    if left != right:
        raise ContractError(f"{label} identity changed")


def _validate_file_identity(
    value: Any, label: str, *, require_live: bool
) -> dict[str, Any]:
    identity = _exact_keys(value, FILE_KEYS, label)
    path = Path(_canonical_absolute_path(identity["path"], f"{label} path"))
    _integer(identity["bytes"], f"{label} bytes", 1)
    _digest(identity["sha256"], f"{label} digest")
    _integer(identity["device"], f"{label} device")
    _integer(identity["inode"], f"{label} inode", 1)
    if require_live and file_identity(path) != identity:
        raise ContractError(f"{label} differs from the live file")
    return identity


def _validate_tool_identity(
    value: Any, label: str, *, require_live: bool
) -> dict[str, Any]:
    tool = _exact_keys(value, TOOL_KEYS, label)
    _validate_file_identity(
        {key: tool[key] for key in FILE_KEYS}, label, require_live=require_live
    )
    _canonical_absolute_path(tool["invocation"], f"{label} invocation")
    _string(tool["version"], f"{label} version")
    return tool


def write_exec_pid(path: Path, pid: int | None = None) -> None:
    value = os.getpid() if pid is None else _integer(pid, "exec PID", 1)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_CLOEXEC", 0)
    flags |= getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags, 0o600)
    except OSError as error:
        raise ContractError(f"cannot create exec PID record: {error}") from error
    try:
        os.write(descriptor, f"{value}\n".encode("ascii"))
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def read_exec_pid(path: Path) -> int:
    try:
        text = _read_regular(path).decode("ascii")
    except UnicodeDecodeError as error:
        raise ContractError("exec PID record is not ASCII") from error
    if not re.fullmatch(r"[1-9][0-9]*\n", text):
        raise ContractError("exec PID record is malformed")
    return int(text)


def source_snapshot(root: Path) -> dict[str, Any]:
    resolved = root.resolve()
    try:
        info = resolved.lstat()
    except OSError as error:
        raise ContractError(f"cannot inspect source root {resolved}: {error}") from error
    if not stat.S_ISDIR(info.st_mode):
        raise ContractError(f"source root is not a real directory: {resolved}")
    names = [
        name
        for name in _git(resolved, ["ls-files", "-z", "--cached"]).split("\0")
        if name
    ]
    if not names or names != sorted(names) or len(names) != len(set(names)):
        raise ContractError("tracked source inventory is empty, duplicated, or reordered")
    digest = hashlib.sha256()
    total = 0
    for name in names:
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ContractError("tracked source inventory contains an unsafe path")
        path = resolved / relative
        data = _read_regular(path)
        encoded = name.encode("utf-8")
        executable = bool(path.stat().st_mode & stat.S_IXUSR)
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(b"\x01" if executable else b"\x00")
        digest.update(data)
        total += len(data)
    revisions = _git(resolved, ["rev-parse", "HEAD", "HEAD^{tree}"]).splitlines()
    if len(revisions) != 2:
        raise ContractError("cannot resolve source commit and tree")
    return {
        "root": str(resolved),
        "device": info.st_dev,
        "inode": info.st_ino,
        "git_commit": revisions[0],
        "git_tree": revisions[1],
        "git_status": _git_status(resolved),
        "tracked_files": len(names),
        "tracked_bytes": total,
        "tracked_sha256": digest.hexdigest(),
    }


def _validate_source_snapshot(
    value: Any, label: str, *, require_live: bool
) -> dict[str, Any]:
    source = _exact_keys(value, SOURCE_KEYS, label)
    root = Path(_canonical_absolute_path(source["root"], f"{label} root"))
    for key in ("device", "inode", "tracked_files", "tracked_bytes"):
        _integer(source[key], f"{label} {key}", 1 if key != "tracked_bytes" else 0)
    for key in ("git_commit", "git_tree"):
        revision = _string(source[key], f"{label} {key}")
        if len(revision) != 40 or any(character not in HEX for character in revision):
            raise ContractError(f"{label} {key} is malformed")
    _digest(source["tracked_sha256"], f"{label} tracked digest")
    if _string(source["git_status"], f"{label} status", allow_empty=True):
        raise ContractError(f"{label} is not clean")
    if require_live:
        actual = source_snapshot(root)
        if actual != source:
            raise ContractError(f"{label} differs from the live clean source root")
    return source


def _clone_clean_root(destination: Path, revision: str) -> None:
    environment = _sanitized_environment()
    environment["GIT_OPTIONAL_LOCKS"] = "0"
    _run_checked(
        [
            str(GIT),
            "clone",
            "--quiet",
            "--local",
            "--no-hardlinks",
            "--no-checkout",
            str(ROOT),
            str(destination),
        ],
        cwd=destination.parent,
        environment=environment,
        timeout=300,
        output_limit=4 * 1024 * 1024,
    )
    _git(destination, ["checkout", "--quiet", "--detach", revision])
    _reject_cargo_configs(destination)
    if _git_status(destination):
        raise ContractError("isolated release root is not clean")


def _reject_cargo_configs(root: Path) -> None:
    cursor = root.resolve()
    while True:
        for name in ("config", "config.toml"):
            candidate = cursor / ".cargo" / name
            if candidate.exists() or candidate.is_symlink():
                raise ContractError(f"untrusted Cargo config affects release build: {candidate}")
        if cursor.parent == cursor:
            return
        cursor = cursor.parent


def _prepare_cargo_home(destination: Path) -> Path:
    destination.mkdir()
    host = (Path(pwd.getpwuid(os.getuid()).pw_dir) / ".cargo").resolve()
    for relative in ("registry/cache", "registry/index"):
        source = host / relative
        if source.is_dir() and not source.is_symlink():
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.symlink_to(source, target_is_directory=True)
    for name in (".package-cache", ".package-cache-mutate"):
        (destination / name).touch()
    return destination


def _build_environment(
    root: Path,
    cargo_home: Path,
    all_roots: list[Path],
    all_cargo_homes: list[Path],
    rustc: str,
) -> dict[str, str]:
    flags = [
        *(f"--remap-path-prefix={path.resolve()}=/typokat-library-base/build" for path in all_roots),
        *(f"--remap-path-prefix={path.resolve()}=/typokat-library-base/cargo" for path in all_cargo_homes),
        "--remap-path-scope=all",
    ]
    environment = _sanitized_environment()
    environment.update(
        {
            "CARGO_HOME": str(cargo_home.resolve()),
            "CARGO_TARGET_DIR": str((root / "target").resolve()),
            "CARGO_NET_OFFLINE": "true",
            "CARGO_INCREMENTAL": "0",
            "CARGO_TERM_COLOR": "never",
            "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(flags),
            "CARGO_BUILD_RUSTFLAGS": "",
            "RUSTC": rustc,
        }
    )
    return environment


def _select_libtest(
    stdout: str, target: Path, *, require_live: bool = True
) -> Path:
    candidates: set[Path] = set()
    for ordinal, line in enumerate(stdout.splitlines(), 1):
        if not line.startswith("{"):
            continue
        value = strict_json(line, f"Cargo JSON line {ordinal}")
        if not isinstance(value, dict) or value.get("reason") != "compiler-artifact":
            continue
        target_record = value.get("target")
        profile = value.get("profile")
        executable = value.get("executable")
        if (
            isinstance(target_record, dict)
            and isinstance(profile, dict)
            and target_record.get("name") == "typokat"
            and target_record.get("kind") == ["lib"]
            and profile.get("test") is True
            and isinstance(executable, str)
            and executable
        ):
            candidates.add(Path(executable).resolve())
    if len(candidates) != 1:
        raise ContractError(f"release build selected {len(candidates)} libtests")
    binary = next(iter(candidates))
    try:
        binary.relative_to(target.resolve())
    except ValueError as error:
        raise ContractError("release libtest escaped its target directory") from error
    if require_live and not os.access(binary, os.X_OK):
        raise ContractError("selected release libtest is not executable")
    return binary


def _build_release_libtest(
    root: Path,
    cargo_home: Path,
    all_roots: list[Path],
    all_cargo_homes: list[Path],
    tools: dict[str, dict[str, Any]],
) -> tuple[Path, dict[str, Any], list[str], dict[str, str]]:
    environment = _build_environment(
        root,
        cargo_home,
        all_roots,
        all_cargo_homes,
        tools["rustc"]["invocation"],
    )
    command = [
        tools["cargo"]["invocation"],
        "test",
        "--release",
        "--locked",
        "--offline",
        "--lib",
        "--no-run",
        "--message-format=json-render-diagnostics",
    ]
    result = run_bounded_record(
        command,
        cwd=root,
        environment=environment,
        timeout=BUILD_TIMEOUT_SECONDS,
        stdout_limit=BUILD_OUTPUT_LIMIT,
        stderr_limit=BUILD_OUTPUT_LIMIT,
    )
    if result["failure"] is not None or result["exit"] != 0:
        raise RecordedProcessError("release libtest build failed", result)
    binary = _select_libtest(result["stdout"], root / "target")
    return binary, result, command, environment


def monotonic_wall_ns(started: Any, finished: Any) -> int:
    start = _integer(started, "process monotonic start", 1)
    finish = _integer(finished, "process monotonic finish", 1)
    if finish <= start:
        raise ContractError("process monotonic interval is empty or reversed")
    return finish - start


def parse_time_rss_bytes(report: str) -> int:
    _string(report, "/usr/bin/time report")
    rss = re.findall(
        r"^\s*Maximum resident set size \(kbytes\):\s*([0-9]+)\s*$",
        report,
        flags=re.MULTILINE,
    )
    if len(rss) != 1:
        raise ContractError("/usr/bin/time -v RSS report is incomplete or duplicated")
    rss_kib = int(rss[0])
    if rss_kib <= 0:
        raise ContractError("/usr/bin/time reported zero peak RSS")
    return rss_kib * 1024


def _run_probe(
    binary: Path,
    source_root: Path,
    build: int,
    expected_binary: dict[str, Any],
    contract: dict[str, Any],
    report: Path,
    pid_path: Path,
    tools: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    if report.exists() or report.is_symlink() or pid_path.exists() or pid_path.is_symlink():
        raise ContractError("probe report/PID output path already exists")
    child_command = _expected_probe_command(str(binary.resolve()), contract)
    outer_command = [
        tools["time"]["invocation"],
        "-v",
        "-o",
        str(report.resolve()),
        "--",
        tools["python"]["invocation"],
        "-I",
        "-S",
        "-c",
        EXEC_WRAPPER_CODE,
        str(pid_path.resolve()),
        *child_command,
    ]
    before = file_identity(binary)
    result = run_bounded_record(
        outer_command,
        cwd=source_root,
        environment=_sanitized_environment(),
        timeout=PROBE_TIMEOUT_SECONDS,
        stdout_limit=PROBE_OUTPUT_LIMIT,
        stderr_limit=PROBE_OUTPUT_LIMIT,
    )
    after = file_identity(binary)
    failure = result["failure"]
    libtest_pid: int | None = None
    report_text = ""
    try:
        libtest_pid = read_exec_pid(pid_path)
    except ContractError as error:
        failure = failure or str(error)
    try:
        report_text = _read_regular(report).decode("utf-8")
    except (ContractError, UnicodeDecodeError) as error:
        failure = failure or f"cannot retain /usr/bin/time report: {error}"
    peak_rss_bytes = 0
    if report_text:
        try:
            peak_rss_bytes = parse_time_rss_bytes(report_text)
        except ContractError as error:
            failure = failure or str(error)
    if before != expected_binary or after != expected_binary:
        failure = failure or "release libtest identity changed around launch"
    record = {
        "window": 0,
        "ordinal": 0,
        "kind": "",
        "build": build,
        "command": child_command,
        "launcher_command": outer_command,
        "environment": _sanitized_environment(),
        "wrapper_pid": result["pid"],
        "pgid": result["pgid"],
        "pid": libtest_pid or 0,
        "exit": result["exit"],
        "stdout": result["stdout"],
        "stderr": result["stderr"],
        "started_monotonic_ns": result["started_monotonic_ns"],
        "finished_monotonic_ns": result["finished_monotonic_ns"],
        "wall_ns": result["wall_ns"],
        "group_clean": result["group_clean"],
        "failure": failure,
        "pid_path": str(pid_path.resolve()),
        "time_report_path": str(report.resolve()),
        "time_report": report_text,
        "peak_rss_bytes": peak_rss_bytes,
        "binary_before": before,
        "binary_after": after,
    }
    if (
        failure is not None
        or result["exit"] != 0
        or result["stderr"]
        or not result["group_clean"]
    ):
        raise RecordedProcessError("release probe process failed", record)
    return record


def _host_evidence() -> dict[str, Any]:
    return {
        "os": platform.system().lower(),
        "arch": platform.machine(),
        "cpu_count": os.cpu_count() or 0,
    }


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".candidate-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as destination:
            json.dump(value, destination, sort_keys=True, separators=(",", ":"))
            destination.write("\n")
            destination.flush()
            os.fsync(destination.fileno())
        os.replace(temporary, path)
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def _new_evidence_path() -> Path:
    if EVIDENCE_ROOT.is_symlink():
        raise ContractError("evidence directory must not be a symlink")
    EVIDENCE_ROOT.mkdir(parents=True, exist_ok=True)
    if not EVIDENCE_ROOT.is_dir() or EVIDENCE_ROOT.is_symlink():
        raise ContractError("evidence directory is not a real directory")
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    candidate = EVIDENCE_ROOT / f"run-{stamp}-{os.getpid()}.json"
    if candidate.exists() or candidate.is_symlink():
        raise ContractError("evidence output path already exists")
    return candidate


def persist_no_go(path: Path, error: str, partial: Any) -> None:
    _write_json(
        path,
        {
            "schema": 1,
            "verdict": "NO-GO",
            "error": _string(error, "NO-GO error"),
            "partial": partial,
        },
    )


def coordinate(
    contract: dict[str, Any], *, evidence_path: Path | None = None
) -> tuple[dict[str, Any], dict[str, Any]]:
    validate_contract(contract)
    tools = resolve_trusted_tools()
    revision = _require_clean_repository()
    source = source_snapshot(ROOT)
    if source["git_commit"] != revision or source["git_status"]:
        raise ContractError("authoritative source is not clean committed HEAD")
    partial: dict[str, Any] = {
        "stage": "host",
        "contract_sha256": _file_identity(CONTRACT_PATH)[1],
        "revision": revision,
        "tree": source["git_tree"],
        "toolchain": tools,
        "source": source,
    }
    if evidence_path is not None:
        _write_json(
            evidence_path,
            {"schema": 1, "verdict": "RUNNING", "partial": partial},
        )

    with tempfile.TemporaryDirectory(prefix="typokat-library-base-") as temporary:
        run_root = Path(temporary)
        roots = [run_root / "build-1", run_root / "build-2"]
        for root in roots:
            _clone_clean_root(root, revision)
        if any(not _same_source_content(source_snapshot(root), source) for root in roots):
            raise ContractError("isolated release roots differ from committed HEAD")

        cargo_homes = [
            _prepare_cargo_home(run_root / "cargo-home-1"),
            _prepare_cargo_home(run_root / "cargo-home-2"),
        ]
        partial.update({"stage": "release-builds", "builds": []})
        binaries: list[Path] = []
        identities: list[dict[str, Any]] = []
        builds: list[dict[str, Any]] = []
        for ordinal, (root, cargo_home) in enumerate(
            zip(roots, cargo_homes, strict=True), 1
        ):
            before = source_snapshot(root)
            try:
                binary, process, command, environment = _build_release_libtest(
                    root, cargo_home, roots, cargo_homes, tools
                )
            except RecordedProcessError as error:
                partial.update(
                    {
                        "stage": f"release-build-{ordinal}",
                        "failed_process": error.record,
                    }
                )
                if evidence_path is not None:
                    _write_json(
                        evidence_path,
                        {"schema": 1, "verdict": "RUNNING", "partial": partial},
                    )
                raise
            after = source_snapshot(root)
            if before != after:
                raise ContractError("release build mutated its clean source root")
            identity = file_identity(binary)
            binaries.append(binary)
            identities.append(identity)
            info = root.lstat()
            build = {
                "ordinal": ordinal,
                "root": str(root.resolve()),
                "device": info.st_dev,
                "inode": info.st_ino,
                "source_before": before,
                "source_after": after,
                "command": command,
                "environment": environment,
                "process": process,
                "cargo": tools["cargo"],
                "rustc": tools["rustc"],
                "binary": identity,
            }
            builds.append(build)
            partial["builds"] = builds
            if evidence_path is not None:
                _write_json(
                    evidence_path,
                    {"schema": 1, "verdict": "RUNNING", "partial": partial},
                )
        if {key: identities[0][key] for key in ("bytes", "sha256")} != {
            key: identities[1][key] for key in ("bytes", "sha256")
        }:
            raise ContractError("independent release builds produced different libtests")
        if _read_regular(binaries[0]) != _read_regular(binaries[1]):
            raise ContractError("independent release libtests are not byte-identical")
        partial.update({"stage": "timing", "builds": builds, "windows": []})
        if evidence_path is not None:
            _write_json(
                evidence_path,
                {"schema": 1, "verdict": "RUNNING", "partial": partial},
            )

        windows: list[dict[str, Any]] = []
        seen_probe_pids: set[int] = set()
        seen_typed_validation_identities: set[str] = set()
        report_root = run_root / "time-reports"
        report_root.mkdir()
        pid_root = run_root / "exec-pids"
        pid_root.mkdir()
        for window_ordinal in range(1, contract["windows"] + 1):
            started = time.monotonic_ns()
            records: list[dict[str, Any]] = []
            records_per_window = (
                contract["warmups_per_window"] + contract["samples_per_window"]
            )
            for record_ordinal in range(1, records_per_window + 1):
                build_ordinal = 1 + ((window_ordinal + record_ordinal) % 2)
                try:
                    raw = _run_probe(
                        binaries[build_ordinal - 1],
                        roots[build_ordinal - 1],
                        build_ordinal,
                        identities[build_ordinal - 1],
                        contract,
                        report_root / f"window-{window_ordinal}-{record_ordinal}.txt",
                        pid_root / f"window-{window_ordinal}-{record_ordinal}.pid",
                        tools,
                    )
                except RecordedProcessError as error:
                    error.record.update(
                        {
                            "window": window_ordinal,
                            "ordinal": record_ordinal,
                            "kind": (
                                "warmup"
                                if record_ordinal <= contract["warmups_per_window"]
                                else "sample"
                            ),
                        }
                    )
                    partial.update(
                        {
                            "stage": "timing",
                            "current": {
                                "window": window_ordinal,
                                "record": record_ordinal,
                            },
                            "windows": windows,
                            "failed_process": error.record,
                        }
                    )
                    if evidence_path is not None:
                        _write_json(
                            evidence_path,
                            {"schema": 1, "verdict": "RUNNING", "partial": partial},
                        )
                    raise
                raw.update(
                    {
                        "window": window_ordinal,
                        "ordinal": record_ordinal,
                        "kind": (
                            "warmup"
                            if record_ordinal <= contract["warmups_per_window"]
                            else "sample"
                        ),
                    }
                )
                record = {key: raw[key] for key in RECORD_KEYS}
                records.append(record)
                partial["current"] = {
                    "window": window_ordinal,
                    "record": record_ordinal,
                }
                if evidence_path is not None:
                    partial["windows"] = [*windows, {"ordinal": window_ordinal, "records": records}]
                    _write_json(
                        evidence_path,
                        {"schema": 1, "verdict": "RUNNING", "partial": partial},
                    )
                if record["pid"] in seen_probe_pids:
                    raise ContractError("a fresh probe process PID was reused")
                seen_probe_pids.add(record["pid"])
                if record["exit"] != 0 or record["stderr"]:
                    raise ContractError("release probe did not exit cleanly")
                if record["peak_rss_bytes"] > contract["rss_bytes_max"]:
                    raise ContractError("external peak RSS exceeds 512 MiB")
                probe = _validate_probe(
                    _extract_probe(record["stdout"], contract["record_prefix"]),
                    contract,
                )
                seen_typed_validation_identities.add(probe["typed_validation_sha256"])
                if len(seen_typed_validation_identities) > contract[
                    "required_typed_validation_identities"
                ]:
                    raise ContractError("typed-validation identity changed between processes")
            windows.append(
                {
                    "ordinal": window_ordinal,
                    "started_monotonic_ns": started,
                    "finished_monotonic_ns": time.monotonic_ns(),
                    "records": records,
                }
            )
            partial["windows"] = windows
            if evidence_path is not None:
                _write_json(
                    evidence_path,
                    {"schema": 1, "verdict": "RUNNING", "partial": partial},
                )
        for root, build in zip(roots, builds, strict=True):
            if source_snapshot(root) != build["source_after"]:
                raise ContractError("probe execution mutated an isolated source root")
        final_source = source_snapshot(ROOT)
        evidence = {
            "schema": 1,
            "contract_sha256": _file_identity(CONTRACT_PATH)[1],
            "revision": revision,
            "tree": source["git_tree"],
            "host": _host_evidence(),
            "toolchain": tools,
            "source": source,
            "builds": builds,
            "windows": windows,
            "final_source": final_source,
        }
        summary = validate_evidence(evidence, contract)
        if evidence_path is not None:
            _write_json(evidence_path, evidence)
        return evidence, summary


def _format_summary(summary: dict[str, Any], contract: dict[str, Any]) -> str:
    return (
        "typokat-library-base-v1 "
        f"windows={summary['windows']} "
        f"samples={summary['samples']} "
        f"p95_wall_us_max={contract['p95_wall_us_max']} "
        f"rss_bytes_max={contract['rss_bytes_max']} "
        f"route={contract['route']} "
        f"artifact_sha256={contract['artifact_sha256']} "
        f"typed_validation_identities={summary['typed_validation_identities']} "
        f"initializations={summary['initializations']} "
        f"publications={summary['publications']} "
        f"outcome={summary['outcome']}"
    )


def _inspect(path: Path, contract: dict[str, Any]) -> dict[str, Any]:
    try:
        evidence = strict_json(path.read_text(encoding="utf-8"), "evidence")
    except OSError as error:
        raise ContractError(f"cannot read evidence: {error}") from error
    return validate_evidence(evidence, contract, require_live=False)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--inspect-evidence",
        type=Path,
        metavar="PATH",
        help="revalidate a completed raw evidence file without running builds",
    )
    arguments = parser.parse_args(argv)
    evidence_path: Path | None = None
    try:
        contract = load_contract()
        if arguments.inspect_evidence is not None:
            summary = _inspect(arguments.inspect_evidence, contract)
        else:
            evidence_path = _new_evidence_path()
            _, summary = coordinate(contract, evidence_path=evidence_path)
    except ContractError as error:
        if evidence_path is not None:
            try:
                existing: Any = None
                if evidence_path.exists():
                    existing = strict_json(
                        evidence_path.read_text(encoding="utf-8"), "partial evidence"
                    )
                    if (
                        isinstance(existing, dict)
                        and existing.get("verdict") == "RUNNING"
                        and "partial" in existing
                    ):
                        existing = existing["partial"]
                persist_no_go(evidence_path, str(error), existing)
            except (ContractError, OSError):
                pass
        suffix = f"; evidence: {evidence_path}" if evidence_path is not None else ""
        print(f"error: {error}{suffix}", file=sys.stderr)
        return 1
    print(_format_summary(summary, contract))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

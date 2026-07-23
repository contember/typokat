#!/usr/bin/env python3
"""Fail-closed WU0B semantic-snapshot feasibility coordinator."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import re
import resource
import selectors
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import tomllib
from datetime import datetime, timezone
from typing import Any, Iterable


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONTRACT_PATH = HERE / "contract.toml"
WU0A = ROOT / "tooling/full-lib-bench"
EVIDENCE_ROOT = HERE / "evidence"
HEX64 = re.compile(r"^[0-9a-f]{64}$")
HARNESS_ONE = re.compile(
    r"test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out"
)
HARNESS_ZERO = re.compile(
    r"test result: ok\. 0 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out"
)
HARNESS_RECORD_SUFFIX = re.compile(
    r"^\r?\n(?:ok|test [^\r\n]+ \.\.\. ok)\r?\n\r?\n"
    r"test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; "
    r"\d+ filtered out; finished in [0-9.]+s(?:\r?\n){1,2}$"
)
STRATEGY = "eager-complete"
RECORD_KIND = "eager-fast-clean"
SCHEMA_IDENTITY = hashlib.sha256(
    b"a78ea0521c7c375669bfdb08f0929a5e4b1d0b0d6928de60fbfe09b222a8bc65"
    b"|collision-replay-index-v1"
).hexdigest()


class ContractError(RuntimeError):
    """The candidate or retained evidence violates the frozen WU0B contract."""


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def file_identity(path: Path) -> dict[str, Any]:
    try:
        info = path.lstat()
    except OSError as error:
        raise ContractError(f"cannot stat {path}: {error}") from error
    if not stat.S_ISREG(info.st_mode) or path.is_symlink():
        raise ContractError(f"expected a non-symlink regular file: {path}")
    return {
        "path": str(path.resolve()),
        "bytes": info.st_size,
        "sha256": sha256_file(path),
    }


def parse_snapshot_wire(path: Path, contract: dict[str, Any]) -> dict[str, Any]:
    """Independently validate every byte of the frozen semantic archive."""
    data = path.read_bytes()
    minimum = contract["artifact"]["minimum_bytes"]
    maximum = contract["artifact"]["maximum_bytes"]
    if not minimum <= len(data) <= maximum:
        raise ContractError(f"snapshot bytes must be in [{minimum}, {maximum}]")
    magic = contract["wire"]["magic"].encode("ascii")
    fixed = len(magic) + 4 + 32 + 32 + 4 + 8 + 32
    entry_size = 2 + 2 + 8 + 8 + 32
    if len(data) < fixed or data[:len(magic)] != magic:
        raise ContractError("snapshot wire magic differs or header is truncated")
    cursor = len(magic)
    version = int.from_bytes(data[cursor:cursor + 4], "big"); cursor += 4
    profile = data[cursor:cursor + 32].hex(); cursor += 32
    schema = data[cursor:cursor + 32].hex(); cursor += 32
    section_count = int.from_bytes(data[cursor:cursor + 4], "big"); cursor += 4
    body_length = int.from_bytes(data[cursor:cursor + 8], "big"); cursor += 8
    body_digest = data[cursor:cursor + 32].hex(); cursor += 32
    if version != contract["wire"]["version"]:
        raise ContractError("snapshot wire version differs")
    if profile != contract["profile_sha256"]:
        raise ContractError("snapshot wire profile digest differs")
    if schema != contract["wire"]["schema_sha256"]:
        raise ContractError("snapshot wire schema digest differs")
    names = contract["wire"]["section_names"]
    tags = contract["wire"]["section_tags"]
    if section_count != len(tags):
        raise ContractError("snapshot wire section count differs")
    directory_end = fixed + section_count * entry_size
    if directory_end > len(data):
        raise ContractError("snapshot wire directory is truncated")
    expected_offset = directory_end
    sections = []
    for ordinal, (expected_tag, name) in enumerate(zip(tags, names, strict=True)):
        offset = fixed + ordinal * entry_size
        tag = int.from_bytes(data[offset:offset + 2], "big")
        reserved = int.from_bytes(data[offset + 2:offset + 4], "big")
        payload_offset = int.from_bytes(data[offset + 4:offset + 12], "big")
        payload_length = int.from_bytes(data[offset + 12:offset + 20], "big")
        digest = data[offset + 20:offset + 52].hex()
        if tag != expected_tag or reserved != 0:
            raise ContractError("snapshot wire tag order or reserved bits differ")
        if payload_offset != expected_offset or payload_length <= 0:
            raise ContractError("snapshot wire sections are empty, overlapping, or noncontiguous")
        end = payload_offset + payload_length
        if end > len(data):
            raise ContractError("snapshot wire section exceeds archive bounds")
        payload = data[payload_offset:end]
        if sha256_bytes(payload) != digest:
            raise ContractError(f"snapshot wire {name} section digest differs")
        sections.append({"ordinal": ordinal, "tag": tag, "name": name, "offset": payload_offset, "bytes": payload_length, "sha256": digest})
        expected_offset = end
    if expected_offset != len(data):
        raise ContractError("snapshot wire has trailing bytes or an unclaimed gap")
    body = data[directory_end:]
    if body_length != len(body) or sha256_bytes(body) != body_digest:
        raise ContractError("snapshot wire body length or digest differs")
    return {
        "magic": contract["wire"]["magic"], "version": version,
        "profile_sha256": profile, "schema_sha256": schema,
        "section_count": section_count, "directory_bytes": section_count * entry_size,
        "body_bytes": body_length, "body_sha256": body_digest, "sections": sections,
    }


def strict_json(text: str, label: str) -> Any:
    def pairs(items: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in items:
            if key in result:
                raise ContractError(f"{label} contains duplicate key {key!r}")
            result[key] = value
        return result

    def invalid(value: str) -> None:
        raise ContractError(f"{label} contains nonstandard number {value}")

    try:
        return json.loads(text, object_pairs_hook=pairs, parse_constant=invalid)
    except json.JSONDecodeError as error:
        raise ContractError(f"invalid {label}: {error}") from error


def exact_keys(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        actual = set(value) if isinstance(value, dict) else type(value).__name__
        raise ContractError(f"{label} keys differ: expected={sorted(keys)!r} actual={actual!r}")
    return value


def require_int(value: Any, label: str, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise ContractError(f"{label} must be an integer >= {minimum}")
    return value


def load_contract(path: Path = CONTRACT_PATH) -> dict[str, Any]:
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ContractError(f"cannot read contract: {error}") from error
    exact_keys(data, {"schema", "profile_sha256", "profile_file_count", "wu0a_oracles_sha256", "wu0a_workloads_sha256", "libtest", "artifact", "wire", "sampling", "controls"}, "contract")
    if data["schema"] != 1 or data["profile_file_count"] != 82:
        raise ContractError("unsupported contract schema or profile size")
    for key in ("profile_sha256", "wu0a_oracles_sha256", "wu0a_workloads_sha256"):
        if not isinstance(data[key], str) or not HEX64.fullmatch(data[key]):
            raise ContractError(f"contract {key} is malformed")
    exact_keys(data["libtest"], {"package", "target", "build_args", "preflight_filter", "preflight_args", "preflight_passed", "preflight_ignored", "regeneration_filter", "timing_filter", "strategy_filter", "scaling_filter", "test_args", "record_prefix", "semantic_record_prefix"}, "contract.libtest")
    expected_build = ["test", "--release", "--lib", "--no-run", "--message-format=json-render-diagnostics"]
    expected_args = ["--ignored", "--exact", "{filter}", "--nocapture", "--test-threads=1"]
    if data["libtest"]["package"] != "typokat" or data["libtest"]["target"] != "typokat":
        raise ContractError("libtest package/target differs")
    if data["libtest"]["build_args"] != expected_build or data["libtest"]["test_args"] != expected_args:
        raise ContractError("canonical release libtest commands differ")
    if data["libtest"]["preflight_filter"] != "check::checker::wu0b_snapshot_spec::" or data["libtest"]["preflight_args"] != ["{filter}", "--nocapture", "--test-threads=1"] or data["libtest"]["preflight_passed"] != 17 or data["libtest"]["preflight_ignored"] != 4:
        raise ContractError("canonical full WU0B preflight differs")
    expected_filters = {
        "regeneration_filter": "check::checker::wu0b_snapshot_spec::snapshot_regeneration_probe_once",
        "timing_filter": "check::checker::wu0b_snapshot_spec::snapshot_fast_clean_probe_once",
        "strategy_filter": "check::checker::wu0b_snapshot_spec::snapshot_decode_strategy_probe_once",
        "scaling_filter": "check::checker::wu0b_snapshot_spec::snapshot_scaling_probe_once",
    }
    for key, expected in expected_filters.items():
        if data["libtest"][key] != expected:
            raise ContractError(f"canonical {key} differs")
    if data["libtest"]["record_prefix"] != "TYPOKAT_WU0B_PROBE=":
        raise ContractError("probe record prefix differs")
    if data["libtest"]["semantic_record_prefix"] != "TYPOKAT_WU0B_SEMANTICS=":
        raise ContractError("semantic calibration record prefix differs")
    exact_keys(data["artifact"], {"maximum_bytes", "minimum_bytes", "regenerations", "schema", "canonical_bytes", "canonical_sha256"}, "contract.artifact")
    if data["artifact"] != {
        "maximum_bytes": 32 * 1024 * 1024,
        "minimum_bytes": 1024 * 1024,
        "regenerations": 2,
        "schema": 1,
        "canonical_bytes": 21_000_266,
        "canonical_sha256": "539a52fdd66130c35172d2405032e442f52d161dfd2ebcae873a03151a7e2960",
    }:
        raise ContractError("artifact contract differs")
    exact_keys(data["wire"], {"magic", "version", "schema_sha256", "section_names", "section_tags"}, "contract.wire")
    if data["wire"] != {
        "magic": "typokat-semantic-snapshot", "version": 1,
        "schema_sha256": SCHEMA_IDENTITY,
        "section_names": ["store", "interner", "binder", "decl-types", "published-types", "namespace-terminals", "class-metadata", "semantic-identities", "root-name-index", "next-ids", "collision-replay-index"],
        "section_tags": list(range(1, 12)),
    }:
        raise ContractError("snapshot wire contract differs")
    exact_keys(data["sampling"], {"windows", "warmups_per_window", "recorded_per_window", "minimum_window_gap_seconds", "timeout_seconds", "maximum_stdout_bytes", "maximum_stderr_bytes", "maximum_p95_wall_ms", "maximum_peak_rss_kib"}, "contract.sampling")
    expected_sampling = {
        "windows": 3,
        "warmups_per_window": 5,
        "recorded_per_window": 10,
        "minimum_window_gap_seconds": 60,
        "timeout_seconds": 30,
        "maximum_stdout_bytes": 1024 * 1024,
        "maximum_stderr_bytes": 1024 * 1024,
        "maximum_p95_wall_ms": 120,
        "maximum_peak_rss_kib": 512 * 1024,
    }
    if data["sampling"] != expected_sampling:
        raise ContractError("canonical sampling contract differs")
    exact_keys(data["controls"], {"require_clean_worktree", "require_release_libtest", "require_fresh_process", "require_external_wall", "require_external_rss", "generation_is_outside_timing"}, "contract.controls")
    if set(data["controls"].values()) != {True}:
        raise ContractError("all WU0B controls must remain enabled")
    return data


def load_wu0a() -> Any:
    path = WU0A / "full_lib_bench.py"
    spec = importlib.util.spec_from_file_location("typokat_wu0a_contract", path)
    if spec is None or spec.loader is None:
        raise ContractError("cannot import WU0A contract verifier")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def verify_contract() -> dict[str, Any]:
    contract = load_contract()
    if sha256_file(WU0A / "oracles.json") != contract["wu0a_oracles_sha256"]:
        raise ContractError("WU0A oracle bytes differ")
    if sha256_file(WU0A / "workloads.lock") != contract["wu0a_workloads_sha256"]:
        raise ContractError("WU0A workload lock bytes differ")
    wu0a = load_wu0a()
    source = wu0a.verify_contract()
    if source["profile"]["length_framed_sha256"] != contract["profile_sha256"]:
        raise ContractError("WU0A live profile identity differs")
    if source["profile"]["file_count"] != contract["profile_file_count"]:
        raise ContractError("WU0A live profile count differs")
    return contract


def sanitized_environment() -> dict[str, str]:
    return {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "TZ": "UTC"}


def host_provenance() -> dict[str, Any]:
    affinity = sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else list(range(os.cpu_count() or 0))
    priority = os.getpriority(os.PRIO_PROCESS, 0) if hasattr(os, "getpriority") else 0
    if len(affinity) < 2:
        raise ContractError("authoritative WU0B gate requires at least two eligible CPUs")
    if priority != 0:
        raise ContractError("authoritative WU0B gate requires normal process priority")
    cpu_model = "unknown"
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.is_file():
        for line in cpuinfo.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.lower().startswith("model name") and ":" in line:
                cpu_model = line.split(":", 1)[1].strip()
                break
    return {
        "hostname": platform.node(), "kernel": platform.release(), "machine": platform.machine(),
        "python": platform.python_version(), "cpu_model": cpu_model, "cpu_count": os.cpu_count(), "affinity": affinity,
        "priority": priority, "timezone": "UTC",
        "rlimit_as": list(resource.getrlimit(resource.RLIMIT_AS)),
        "rlimit_cpu": list(resource.getrlimit(resource.RLIMIT_CPU)),
        "rlimit_nofile": list(resource.getrlimit(resource.RLIMIT_NOFILE)),
    }


def executable(name: str) -> Path:
    value = shutil.which(name)
    if value is None:
        raise ContractError(f"{name} is unavailable")
    # Preserve rustup proxy argv[0]; resolving cargo/rustc turns both into `rustup`.
    return Path(value).absolute()


def kill_group(pid: int) -> None:
    try:
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def group_exists(pid: int) -> bool:
    try:
        os.killpg(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def wait_group_gone(pid: int) -> bool:
    deadline = time.monotonic() + 1.0
    while group_exists(pid) and time.monotonic() < deadline:
        time.sleep(0.005)
    return not group_exists(pid)


def run_process(argv: Iterable[str], *, cwd: Path, environment: dict[str, str], timeout: int, stdout_cap: int, stderr_cap: int) -> dict[str, Any]:
    command = tuple(str(part) for part in argv)
    if not command or any("\x00" in part for part in command):
        raise ContractError("invalid child command")
    if environment.get("TYPOKAT_WU0B_SNAPSHOT_INPUT") and environment.get("TYPOKAT_WU0B_SNAPSHOT_OUTPUT"):
        raise ContractError("snapshot generation and consumption cannot share one child")
    started = time.monotonic_ns()
    try:
        child = subprocess.Popen(command, cwd=cwd, env=environment, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True, close_fds=True)
    except OSError as error:
        raise ContractError(f"cannot start child: {error}") from error
    assert child.stdout is not None and child.stderr is not None
    buffers = {child.stdout: bytearray(), child.stderr: bytearray()}
    caps = {child.stdout: stdout_cap, child.stderr: stderr_cap}
    selector = selectors.DefaultSelector()
    for stream in buffers:
        os.set_blocking(stream.fileno(), False)
        selector.register(stream, selectors.EVENT_READ)
    deadline = time.monotonic() + timeout
    failure: str | None = None
    try:
        while selector.get_map():
            if time.monotonic() >= deadline:
                failure = f"child timed out after {timeout}s"
                kill_group(child.pid)
                break
            for key, _ in selector.select(0.05):
                stream = key.fileobj
                chunk = os.read(stream.fileno(), min(65536, caps[stream] + 1 - len(buffers[stream])))
                if not chunk:
                    selector.unregister(stream)
                    stream.close()
                    continue
                buffers[stream].extend(chunk)
                if len(buffers[stream]) > caps[stream]:
                    failure = "child output exceeded its live cap"
                    kill_group(child.pid)
                    break
            if failure:
                break
        if failure:
            kill_group(child.pid)
            for stream in list(selector.get_map().values()):
                try:
                    stream.fileobj.close()
                except OSError:
                    pass
            selector.close()
        waited_pid, status, usage = os.wait4(child.pid, 0)
        if waited_pid != child.pid:
            raise ContractError("wait4 reaped an unexpected process")
        child.returncode = os.waitstatus_to_exitcode(status)
    finally:
        selector.close()
        for stream in buffers:
            try:
                stream.close()
            except OSError:
                pass
    ended = time.monotonic_ns()
    group_clean = wait_group_gone(child.pid)
    if failure:
        raise ContractError(failure)
    if not group_clean:
        kill_group(child.pid)
        raise ContractError("child process group survived leader exit")
    try:
        stdout = bytes(buffers[child.stdout]).decode("utf-8")
        stderr = bytes(buffers[child.stderr]).decode("utf-8")
    except UnicodeDecodeError as error:
        raise ContractError(f"child emitted malformed UTF-8: {error}") from error
    return {
        "argv": list(command),
        "cwd": str(cwd.resolve()),
        "env": environment,
        "pid": child.pid,
        "returncode": child.returncode,
        "stdout": stdout,
        "stderr": stderr,
        "started_monotonic_ns": started,
        "ended_monotonic_ns": ended,
        "wall_ns": ended - started,
        "peak_rss_kib": int(usage.ru_maxrss),
        "group_clean": group_clean,
    }


def git_output(args: list[str]) -> str:
    result = subprocess.run([str(executable("git")), *args], cwd=ROOT, env=sanitized_environment(), stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=False)
    if result.returncode != 0 or result.stderr:
        raise ContractError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def source_snapshot(source_root: Path = ROOT, *, include_status: bool = True) -> dict[str, Any]:
    status = git_output(["status", "--porcelain=v1", "--untracked-files=all"])
    names = git_output(["ls-files", "-z", "--cached"]).split("\0")
    inventory = [name for name in names if name]
    if not inventory or inventory != sorted(inventory) or len(inventory) != len(set(inventory)):
        raise ContractError("tracked source inventory is empty, duplicated, or reordered")
    framed = hashlib.sha256()
    count = 0
    total = 0
    for name in inventory:
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ContractError("tracked source inventory contains an unsafe path")
        path = source_root / relative
        if not path.is_file() or path.is_symlink():
            raise ContractError(f"tracked source is not a regular file: {name}")
        data = path.read_bytes()
        encoded = name.encode("utf-8")
        executable_bit = bool(path.stat().st_mode & stat.S_IXUSR)
        framed.update(len(encoded).to_bytes(8, "big"))
        framed.update(encoded)
        framed.update(len(data).to_bytes(8, "big"))
        framed.update(b"\x01" if executable_bit else b"\x00")
        framed.update(data)
        count += 1
        total += len(data)
    revisions = git_output(["rev-parse", "HEAD", "HEAD^{tree}"]).splitlines()
    if len(revisions) != 2:
        raise ContractError("cannot bind source snapshot to Git commit/tree")
    return {"root": str(source_root.resolve()), "git_commit": revisions[0], "git_tree": revisions[1], "git_status": status if include_status else "", "tracked_files": count, "tracked_bytes": total, "tracked_sha256": framed.hexdigest()}


def tracked_paths() -> list[Path]:
    return [Path(name) for name in git_output(["ls-files", "-z", "--cached"]).split("\0") if name]


def reject_ancestor_cargo_configs(path: Path) -> None:
    cursor = path.resolve().parent
    while True:
        for name in ("config", "config.toml"):
            candidate = cursor / ".cargo" / name
            if candidate.exists() or candidate.is_symlink():
                raise ContractError(f"untrusted ancestor Cargo config: {candidate}")
        if cursor.parent == cursor:
            break
        cursor = cursor.parent


def prepare_isolated_source(build_root: Path) -> tuple[Path, dict[str, Any]]:
    source_root = build_root / "source"
    reject_ancestor_cargo_configs(source_root)
    source_root.mkdir(parents=True)
    for relative in tracked_paths():
        source = ROOT / relative
        if not source.is_file() or source.is_symlink():
            raise ContractError(f"tracked build source is not regular: {relative}")
        destination = source_root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        destination.chmod(0o755 if source.stat().st_mode & stat.S_IXUSR else 0o644)
    return source_root, source_snapshot(source_root, include_status=False)


def tree_digest(path: Path) -> dict[str, Any]:
    digest = hashlib.sha256()
    count = 0
    total = 0
    if not path.exists():
        return {"path": str(path.resolve()), "files": 0, "bytes": 0, "sha256": sha256_bytes(b"")}
    for item in sorted(path.rglob("*")):
        relative = item.relative_to(path).as_posix().encode("utf-8")
        if item.is_symlink():
            target = os.readlink(item).encode("utf-8")
            digest.update(b"L" + len(relative).to_bytes(8, "big") + relative + len(target).to_bytes(8, "big") + target)
        elif item.is_file():
            data = item.read_bytes()
            digest.update(b"F" + len(relative).to_bytes(8, "big") + relative + len(data).to_bytes(8, "big") + data)
            count += 1
            total += len(data)
        elif not item.is_dir():
            raise ContractError(f"unsupported dependency source entry: {item}")
    return {"path": str(path.resolve()), "files": count, "bytes": total, "sha256": digest.hexdigest()}


def cargo_home_layout(cargo_home: Path) -> dict[str, Any]:
    for forbidden in (cargo_home / "config", cargo_home / "config.toml"):
        if forbidden.exists() or forbidden.is_symlink():
            raise ContractError("isolated Cargo home acquired a config file")
    return {
        "root": str(cargo_home.resolve()),
        "registry_sources": tree_digest(cargo_home / "registry/src"),
        "git_checkouts": tree_digest(cargo_home / "git/checkouts"),
        "git_db": tree_digest(cargo_home / "git/db"),
    }


def prepare_cargo_home(build_root: Path) -> tuple[Path, Path, dict[str, Any]]:
    cargo_home = build_root / "cargo-home"
    target = build_root / "target"
    cargo_home.mkdir()
    target.mkdir()
    host_home = Path(os.environ.get("CARGO_HOME", str(executable("cargo").parent.parent))).resolve()
    exposed = []
    for relative in ("registry/cache", "registry/index"):
        source = host_home / relative
        if source.is_dir():
            destination = cargo_home / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.symlink_to(source.resolve(), target_is_directory=True)
            exposed.append({"path": relative, "target": str(source.resolve())})
    for name in (".package-cache", ".package-cache-mutate"):
        (cargo_home / name).touch()
    before = {"cache_source": str(host_home), "exposed": exposed, **cargo_home_layout(cargo_home)}
    return cargo_home, target, before


def canonical_rustflags(build_roots: Iterable[Path]) -> list[str]:
    roots = [root.resolve() for root in build_roots]
    if not roots or len(roots) != len(set(roots)):
        raise ContractError("canonical build roots must be nonempty and distinct")
    return [
        *(f"--remap-path-prefix={root}=/typokat-wu0b/build" for root in roots),
        "--remap-path-scope=all",
    ]


def build_environment(
    cargo_home: Path, target: Path, build_roots: Iterable[Path] = ()
) -> dict[str, str]:
    cargo = executable("cargo")
    rustflags = canonical_rustflags(build_roots) if build_roots else []
    result = sanitized_environment()
    result.update({
        "PATH": f"{cargo.parent}:/usr/bin:/bin",
        "CARGO_HOME": str(cargo_home.resolve()),
        "CARGO_TARGET_DIR": str(target.resolve()),
        "CARGO_NET_OFFLINE": "true",
        "CARGO_TERM_COLOR": "never",
        "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(rustflags),
        "CARGO_BUILD_RUSTFLAGS": "",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER": "/usr/bin/cc",
    })
    return result


def select_libtest(stdout: str, contract: dict[str, Any], target_root: Path) -> Path:
    candidates: set[Path] = set()
    for ordinal, line in enumerate(stdout.splitlines(), 1):
        if not line.startswith("{"):
            continue
        value = strict_json(line, f"Cargo JSON line {ordinal}")
        if not isinstance(value, dict) or value.get("reason") != "compiler-artifact":
            continue
        target = value.get("target")
        profile = value.get("profile")
        executable_path = value.get("executable")
        if not isinstance(target, dict) or not isinstance(profile, dict):
            continue
        if target.get("name") == contract["libtest"]["target"] and target.get("kind") == ["lib"] and profile.get("test") is True and executable_path:
            candidates.add(Path(executable_path).resolve())
    if len(candidates) != 1:
        raise ContractError(f"release build selected {len(candidates)} libtest executables")
    selected = next(iter(candidates))
    try:
        selected.relative_to(target_root.resolve())
    except ValueError as error:
        raise ContractError("selected libtest escaped the fresh build target") from error
    identity = file_identity(selected)
    if not os.access(selected, os.X_OK) or identity["bytes"] <= 0:
        raise ContractError("selected libtest is not executable")
    return selected


def toolchain_identities() -> list[dict[str, Any]]:
    result = []
    for name in ("rust-toolchain", "rust-toolchain.toml"):
        path = ROOT / name
        result.append(file_identity(path) if path.exists() else {"path": str(path.resolve()), "present": False})
    return result


def repo_cargo_configs() -> list[dict[str, Any]]:
    return [file_identity(path) for path in (ROOT / ".cargo/config", ROOT / ".cargo/config.toml") if path.exists()]


def effective_fingerprint(target: Path, rustflags: list[str]) -> dict[str, Any]:
    candidates = list((target / "release/.fingerprint").glob("typokat-*/test-lib-typokat.json"))
    if len(candidates) != 1:
        raise ContractError(f"fresh build produced {len(candidates)} libtest fingerprints")
    path = candidates[0]
    value = strict_json(path.read_text(encoding="utf-8"), "Cargo libtest fingerprint")
    if not isinstance(value, dict) or value.get("rustflags") != rustflags:
        raise ContractError("effective release libtest rustflags differ")
    return {"file": file_identity(path), "invoked_timestamp": file_identity(path.parent / "invoked.timestamp"), "rustflags": value["rustflags"], "features": value.get("features"), "profile": value.get("profile"), "config": value.get("config")}


def collect_build(
    contract: dict[str, Any],
    run_root: Path,
    repository_source: dict[str, Any],
    build_roots: list[Path],
    progress: list[dict[str, Any]] | None = None,
) -> tuple[dict[str, Any], Path]:
    record: dict[str, Any] = {"stage": "preparing-isolated-source"}
    if progress is not None:
        progress.append(record)
    source_root, source_before = prepare_isolated_source(run_root)
    record.update({"stage": "preparing-cargo-home", "source_before": source_before})
    comparable_repository = {**repository_source, "root": str(source_root.resolve()), "git_status": ""}
    if source_before != comparable_repository:
        raise ContractError("isolated tracked source differs from clean repository HEAD")
    cargo_home, target, cargo_home_before = prepare_cargo_home(run_root)
    command = [str(executable("cargo")), *contract["libtest"]["build_args"]]
    rustflags = canonical_rustflags(build_roots)
    environment = build_environment(cargo_home, target, build_roots)
    record.update({"stage": "toolchain-probes", "command": command, "environment": environment, "cargo_home_before": cargo_home_before})
    cargo_version = run_process([str(executable("cargo")), "--version", "--verbose"], cwd=source_root, environment=environment, timeout=10, stdout_cap=65536, stderr_cap=65536)
    record["cargo_version"] = cargo_version
    rustc_version = run_process([str(executable("rustc")), "--version", "--verbose"], cwd=source_root, environment=environment, timeout=10, stdout_cap=65536, stderr_cap=65536)
    record["rustc_version"] = rustc_version
    if cargo_version["returncode"] or cargo_version["stderr"] or rustc_version["returncode"] or rustc_version["stderr"] or not cargo_version["stdout"].startswith("cargo ") or not rustc_version["stdout"].startswith("rustc "):
        raise ContractError("Cargo/rustc version attestation failed")
    record["stage"] = "release-libtest-build"
    process = run_process(command, cwd=source_root, environment=environment, timeout=600, stdout_cap=16 * 1024 * 1024, stderr_cap=16 * 1024 * 1024)
    record["process"] = process
    if process["returncode"] != 0:
        raise ContractError("canonical release libtest build failed")
    selected = select_libtest(process["stdout"], contract, target)
    source_after = source_snapshot(source_root, include_status=False)
    if source_before != source_after:
        raise ContractError("isolated tracked source changed during release build")
    reject_ancestor_cargo_configs(source_root)
    completed = {
        "command": command,
        "environment": environment,
        "cargo_version": cargo_version,
        "rustc_version": rustc_version,
        "process": process,
        "cargo_lock": file_identity(ROOT / "Cargo.lock"),
        "cargo_configs": repo_cargo_configs(),
        "toolchain_files": toolchain_identities(),
        "cargo_home_before": cargo_home_before,
        "cargo_home_after": cargo_home_layout(cargo_home),
        "effective_fingerprint": effective_fingerprint(target, rustflags),
        "source_before": source_before,
        "source_after": source_after,
        "libtest": file_identity(selected),
    }
    record.clear()
    record.update(completed)
    return completed, selected


def test_command(binary: Path, filter_name: str, contract: dict[str, Any]) -> list[str]:
    args = [filter_name if item == "{filter}" else item for item in contract["libtest"]["test_args"]]
    return [str(binary.resolve()), *args]


def preflight_command(binary: Path, contract: dict[str, Any]) -> list[str]:
    filter_name = contract["libtest"]["preflight_filter"]
    args = [filter_name if item == "{filter}" else item for item in contract["libtest"]["preflight_args"]]
    return [str(binary.resolve()), *args]


def execute_preflight(
    binary: Path,
    source_root: Path,
    contract: dict[str, Any],
    progress: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    before = file_identity(binary)
    source_before = source_snapshot(source_root, include_status=False)
    environment = sanitized_environment() | {
        "TYPOKAT_WU0B_PROFILE_ROOT": str(source_root.resolve())
    }
    process = run_process(preflight_command(binary, contract), cwd=source_root, environment=environment, timeout=600, stdout_cap=contract["sampling"]["maximum_stdout_bytes"], stderr_cap=contract["sampling"]["maximum_stderr_bytes"])
    after = file_identity(binary)
    source_after = source_snapshot(source_root, include_status=False)
    record = {"filter": contract["libtest"]["preflight_filter"], "binary_before": before, "binary_after": after, "source_before": source_before, "source_after": source_after, "process": process}
    if progress is not None:
        progress.append(record)
    if before != after:
        raise ContractError("release libtest mutated during WU0B preflight")
    if source_before != source_after:
        raise ContractError("isolated tracked source mutated during WU0B preflight")
    passed = contract["libtest"]["preflight_passed"]
    ignored = contract["libtest"]["preflight_ignored"]
    pattern = re.compile(rf"test result: ok\. {passed} passed; 0 failed; {ignored} ignored; 0 measured; \d+ filtered out")
    if process["returncode"] != 0 or process["stderr"] or not pattern.search(process["stdout"]):
        raise ContractError("full release WU0B preflight did not pass exactly")
    return record


def assert_one_test(process: dict[str, Any], filter_name: str) -> None:
    if process["returncode"] != 0 or process["stderr"]:
        raise ContractError(f"{filter_name} did not pass cleanly")
    stdout = process["stdout"]
    if stdout.count("running 1 test") != 1 or not HARNESS_ONE.search(stdout):
        raise ContractError(f"{filter_name} did not execute exactly one passing test")


def extract_prefixed_record(stdout: str, prefix: str, label: str) -> Any:
    if stdout.count(prefix) != 1:
        raise ContractError(f"{label} child must emit exactly one record")
    tail = stdout.split(prefix, 1)[1]
    decoder = json.JSONDecoder(object_pairs_hook=lambda pairs: _strict_pairs(pairs, f"{label} record"), parse_constant=lambda value: (_ for _ in ()).throw(ContractError(f"{label} record contains {value}")))
    try:
        value = tail.lstrip()
        record, end = decoder.raw_decode(value)
    except (json.JSONDecodeError, ContractError) as error:
        raise ContractError(f"malformed {label} record: {error}") from error
    suffix = value[end:]
    if suffix and not HARNESS_RECORD_SUFFIX.fullmatch(suffix):
        raise ContractError(f"malformed {label} record boundary")
    return record


def extract_probe(stdout: str, contract: dict[str, Any]) -> dict[str, Any]:
    if contract["libtest"]["semantic_record_prefix"] in stdout:
        raise ContractError("timing child emitted untimed semantic calibration")
    record = extract_prefixed_record(
        stdout, contract["libtest"]["record_prefix"], "timing probe"
    )
    return validate_probe(record, contract)


def _strict_pairs(pairs: list[tuple[str, Any]], label: str) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"{label} contains duplicate key {key!r}")
        result[key] = value
    return result


def expected_semantics() -> dict[str, Any]:
    oracle = strict_json((WU0A / "oracles.json").read_text(encoding="utf-8"), "WU0A oracles")
    rows = oracle["rows"]
    return {key: rows[key] for key in ("fast-clean", "fast-errors")}


def validate_semantic_calibration(record: Any, contract: dict[str, Any], artifact: dict[str, Any]) -> dict[str, Any]:
    exact_keys(record, {"schema", "kind", "profile_sha256", "artifact_sha256", "artifact_bytes", "semantics"}, "semantic calibration record")
    if record["schema"] != 1 or record["kind"] != "decoded-semantic-calibration":
        raise ContractError("semantic calibration schema or kind differs")
    if record["profile_sha256"] != contract["profile_sha256"]:
        raise ContractError("semantic calibration profile identity differs")
    if not isinstance(record["artifact_sha256"], str) or not HEX64.fullmatch(record["artifact_sha256"]):
        raise ContractError("semantic calibration artifact digest is malformed")
    require_int(record["artifact_bytes"], "semantic calibration artifact bytes", 1)
    if record["artifact_sha256"] != artifact["sha256"] or record["artifact_bytes"] != artifact["bytes"]:
        raise ContractError("semantic calibration artifact identity differs")
    if record["semantics"] != expected_semantics():
        raise ContractError("semantic calibration differs from WU0A")
    return record


def extract_semantic_calibration(stdout: str, contract: dict[str, Any], artifact: dict[str, Any]) -> dict[str, Any]:
    if contract["libtest"]["record_prefix"] in stdout:
        raise ContractError("generation child emitted a timing probe record")
    record = extract_prefixed_record(
        stdout, contract["libtest"]["semantic_record_prefix"], "semantic calibration"
    )
    return validate_semantic_calibration(record, contract, artifact)


def validate_probe(record: Any, contract: dict[str, Any], *, artifact: dict[str, Any] | None = None, input_path: str | None = None) -> dict[str, Any]:
    exact_keys(record, {"schema", "kind", "route", "profile_sha256", "strategy", "artifact_sha256", "artifact_bytes", "validated_bytes", "runtime_projection_sha256", "input_path", "semantics", "compiler_measure", "internal"}, "probe record")
    if record["schema"] != 1 or record["kind"] != RECORD_KIND or record["strategy"] != STRATEGY or record["route"] != "decoded-base-user-check":
        raise ContractError("probe schema, kind, strategy, or route differs")
    if record["profile_sha256"] != contract["profile_sha256"]:
        raise ContractError("probe profile identity differs")
    for key in ("artifact_bytes", "validated_bytes"):
        require_int(record[key], f"probe {key}")
    if record["validated_bytes"] != record["artifact_bytes"]:
        raise ContractError("probe did not validate the complete archive")
    if not isinstance(record["runtime_projection_sha256"], str) or not HEX64.fullmatch(record["runtime_projection_sha256"]):
        raise ContractError("runtime projection identity is malformed")
    exact_keys(record["compiler_measure"], {"source_loads", "parse_units", "bind_units", "semantic_units", "snapshot_generations"}, "probe compiler measure")
    for key, value in record["compiler_measure"].items():
        if require_int(value, f"probe compiler measure {key}") != 0:
            raise ContractError("source/compiler/generation work occurred inside timing")
    if record["semantics"] != {"fast-clean": expected_semantics()["fast-clean"]}:
        raise ContractError("probe semantic identities differ from WU0A")
    exact_keys(record["internal"], {"validation_us", "decode_us", "user_check_us", "wall_us", "peak_rss_bytes"}, "probe internal telemetry")
    for key, value in record["internal"].items():
        require_int(value, f"probe internal {key}", 1)
    if artifact is not None:
        if record["artifact_sha256"] != artifact["sha256"] or record["artifact_bytes"] != artifact["bytes"]:
            raise ContractError("probe artifact identity differs from external input")
    if input_path is not None and record["input_path"] != input_path:
        raise ContractError("probe did not report the exact canonical input path")
    return record


def child_environment(*, input_path: Path | None = None, output_path: Path | None = None, profile_root: Path | None = None) -> dict[str, str]:
    if (input_path is None) == (output_path is None):
        raise ContractError("child must exclusively generate or consume a snapshot")
    result = sanitized_environment()
    if input_path is not None:
        if profile_root is not None:
            raise ContractError("timing child cannot receive a profile root")
        result["TYPOKAT_WU0B_SNAPSHOT_INPUT"] = str(input_path.resolve())
    else:
        assert output_path is not None
        if profile_root is None:
            raise ContractError("generation child requires an isolated profile root")
        result["TYPOKAT_WU0B_SNAPSHOT_OUTPUT"] = str(output_path.resolve())
        result["TYPOKAT_WU0B_PROFILE_ROOT"] = str(profile_root.resolve())
    return result


def execute_child(binary: Path, filter_name: str, contract: dict[str, Any], environment: dict[str, str], *, cwd: Path = ROOT, source_root: Path | None = None) -> dict[str, Any]:
    before = file_identity(binary)
    artifact_path = environment.get("TYPOKAT_WU0B_SNAPSHOT_INPUT")
    artifact_before = file_identity(Path(artifact_path)) if artifact_path else None
    source_before = source_snapshot(source_root, include_status=False) if source_root else None
    process = run_process(test_command(binary, filter_name, contract), cwd=cwd, environment=environment, timeout=contract["sampling"]["timeout_seconds"], stdout_cap=contract["sampling"]["maximum_stdout_bytes"], stderr_cap=contract["sampling"]["maximum_stderr_bytes"])
    after = file_identity(binary)
    if before != after:
        raise ContractError("release libtest mutated during child execution")
    artifact_after = file_identity(Path(artifact_path)) if artifact_path else None
    if artifact_before != artifact_after:
        raise ContractError("prebuilt snapshot artifact mutated during timing")
    record = {"role": "generation" if artifact_path is None else "timing", "filter": filter_name, "binary_before": before, "binary_after": after, "artifact_before": artifact_before, "artifact_after": artifact_after, "process": process}
    if source_root is not None:
        source_after = source_snapshot(source_root, include_status=False)
        record.update({"source_before": source_before, "source_after": source_after})
        if source_before != source_after:
            raise ContractError("isolated tracked source mutated during snapshot regeneration")
    return record


def collect_generations(binaries: list[Path], source_roots: list[Path], contract: dict[str, Any], run_root: Path, progress: list[dict[str, Any]] | None = None) -> tuple[list[dict[str, Any]], Path, dict[str, Any]]:
    if len(binaries) != 2 or len(source_roots) != 2:
        raise ContractError("two independent release libtests are required")
    records = []
    outputs = []
    for ordinal in range(contract["artifact"]["regenerations"]):
        directory = run_root / f"regeneration-{ordinal + 1}"
        directory.mkdir()
        output = directory / "library.snapshot"
        if output.exists():
            raise ContractError("regeneration output path already exists")
        source_root = source_roots[ordinal]
        record = execute_child(
            binaries[ordinal],
            contract["libtest"]["regeneration_filter"],
            contract,
            child_environment(output_path=output, profile_root=source_root),
            cwd=source_root,
            source_root=source_root,
        )
        if progress is not None:
            progress.append(record)
        assert_one_test(record["process"], contract["libtest"]["regeneration_filter"])
        entries = sorted(path.name for path in directory.iterdir())
        if entries != [output.name]:
            raise ContractError("regeneration child wrote outside its exact artifact path")
        identity = file_identity(output)
        if identity["bytes"] != contract["artifact"]["canonical_bytes"] or identity["sha256"] != contract["artifact"]["canonical_sha256"]:
            raise ContractError("regeneration differs from the canonical snapshot identity")
        wire = parse_snapshot_wire(output, contract)
        semantic_calibration = extract_semantic_calibration(
            record["process"]["stdout"], contract, identity
        )
        record["output"] = identity
        record["wire"] = wire
        record["semantic_calibration"] = semantic_calibration
        records.append(record)
        outputs.append((output, identity))
    if outputs[0][1]["sha256"] != outputs[1][1]["sha256"] or outputs[0][1]["bytes"] != outputs[1][1]["bytes"] or records[0]["wire"] != records[1]["wire"]:
        raise ContractError("fresh-process snapshot regenerations differ")
    if outputs[0][0].read_bytes() != outputs[1][0].read_bytes():
        raise ContractError("snapshot regeneration byte comparison failed")
    return records, outputs[0][0], outputs[0][1]


def collect_timing_sample(binary: Path, artifact_path: Path, artifact: dict[str, Any], contract: dict[str, Any], *, window: int, ordinal: int, recorded: bool, progress: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    record = execute_child(binary, contract["libtest"]["timing_filter"], contract, child_environment(input_path=artifact_path))
    if progress is not None:
        progress.append(record)
    assert_one_test(record["process"], contract["libtest"]["timing_filter"])
    probe = extract_probe(record["process"]["stdout"], contract)
    validate_probe(probe, contract, artifact=artifact, input_path=str(artifact_path.resolve()))
    record.update({"window": window, "ordinal": ordinal, "recorded": recorded, "probe": probe})
    return record


def nearest_rank_p95_ms(values_ns: list[int]) -> float:
    if not values_ns:
        raise ContractError("cannot calculate p95 of an empty sample")
    ordered = sorted(values_ns)
    return ordered[math.ceil(0.95 * len(ordered)) - 1] / 1_000_000


def derive(windows: list[dict[str, Any]], contract: dict[str, Any]) -> dict[str, Any]:
    all_values: list[int] = []
    per_window = []
    rss: list[int] = []
    for window in windows:
        values = [entry["process"]["wall_ns"] for entry in window["recorded"]]
        observed_rss = [entry["process"]["peak_rss_kib"] for entry in [*window["warmups"], *window["recorded"]]]
        all_values.extend(values)
        rss.extend(observed_rss)
        per_window.append({"label": window["label"], "p95_wall_ms": nearest_rank_p95_ms(values), "maximum_peak_rss_kib": max(observed_rss)})
    result = {"p95_wall_ms": nearest_rank_p95_ms(all_values), "maximum_peak_rss_kib": max(rss), "windows": per_window}
    limit_ms = contract["sampling"]["maximum_p95_wall_ms"]
    if result["p95_wall_ms"] > limit_ms or any(item["p95_wall_ms"] > limit_ms for item in per_window):
        raise ContractError("external p95 exceeds 120 ms")
    if result["maximum_peak_rss_kib"] > contract["sampling"]["maximum_peak_rss_kib"] or any(item["maximum_peak_rss_kib"] > contract["sampling"]["maximum_peak_rss_kib"] for item in per_window):
        raise ContractError("external peak RSS exceeds 512 MiB")
    return result


def validate_process_shape(process: Any, label: str, *, allow_stderr: bool = False) -> dict[str, Any]:
    exact_keys(process, {"argv", "cwd", "env", "pid", "returncode", "stdout", "stderr", "started_monotonic_ns", "ended_monotonic_ns", "wall_ns", "peak_rss_kib", "group_clean"}, label)
    for key in ("pid", "started_monotonic_ns", "ended_monotonic_ns", "wall_ns", "peak_rss_kib"):
        require_int(process[key], f"{label}.{key}", 1)
    if process["ended_monotonic_ns"] - process["started_monotonic_ns"] != process["wall_ns"]:
        raise ContractError(f"{label} raw monotonic interval differs")
    if not process["group_clean"] or process["returncode"] != 0 or (process["stderr"] and not allow_stderr):
        raise ContractError(f"{label} child result is ineligible")
    if not isinstance(process["argv"], list) or not all(isinstance(item, str) for item in process["argv"]):
        raise ContractError(f"{label} argv is malformed")
    if not isinstance(process["env"], dict) or not all(isinstance(k, str) and isinstance(v, str) for k, v in process["env"].items()):
        raise ContractError(f"{label} environment is malformed")
    return process


def validate_sample(sample: Any, *, binary: dict[str, Any], artifact: dict[str, Any], contract: dict[str, Any], window: int, ordinal: int, recorded: bool, seen_pids: set[int]) -> None:
    exact_keys(sample, {"role", "filter", "binary_before", "binary_after", "artifact_before", "artifact_after", "process", "window", "ordinal", "recorded", "probe"}, "timing sample")
    if sample["role"] != "timing" or sample["filter"] != contract["libtest"]["timing_filter"] or sample["window"] != window or sample["ordinal"] != ordinal or sample["recorded"] is not recorded:
        raise ContractError("timing sample schedule differs")
    if sample["binary_before"] != binary or sample["binary_after"] != binary:
        raise ContractError("timing libtest identity differs")
    if sample["artifact_before"] != artifact or sample["artifact_after"] != artifact:
        raise ContractError("timing artifact identity differs")
    process = validate_process_shape(sample["process"], "timing process")
    if process["pid"] in seen_pids:
        raise ContractError("timing process PID was reused")
    seen_pids.add(process["pid"])
    expected_env = sanitized_environment() | {"TYPOKAT_WU0B_SNAPSHOT_INPUT": artifact["path"]}
    if process["env"] != expected_env or "TYPOKAT_WU0B_SNAPSHOT_OUTPUT" in process["env"]:
        raise ContractError("timing child environment permits generation or differs")
    expected_argv = test_command(Path(binary["path"]), contract["libtest"]["timing_filter"], contract)
    if process["argv"] != expected_argv or process["cwd"] != str(ROOT.resolve()):
        raise ContractError("timing child command differs")
    assert_one_test(process, contract["libtest"]["timing_filter"])
    parsed = extract_probe(process["stdout"], contract)
    if parsed != sample["probe"]:
        raise ContractError("retained probe differs from raw child output")
    validate_probe(parsed, contract, artifact=artifact, input_path=artifact["path"])


def same_source(left: dict[str, Any], right: dict[str, Any]) -> bool:
    keys = {"git_commit", "git_tree", "tracked_files", "tracked_bytes", "tracked_sha256"}
    return {key: left.get(key) for key in keys} == {key: right.get(key) for key in keys}


def validated_source_root(value: Any, label: str) -> Path:
    keys = {"root", "git_commit", "git_tree", "git_status", "tracked_files", "tracked_bytes", "tracked_sha256"}
    source = exact_keys(value, keys, label)
    root_value = source["root"]
    if not isinstance(root_value, str) or not root_value:
        raise ContractError(f"{label} root is malformed")
    try:
        root = Path(root_value)
        resolved = root.resolve()
    except (OSError, RuntimeError, ValueError) as error:
        raise ContractError(f"{label} root is malformed: {error}") from error
    if not root.is_absolute() or str(resolved) != root_value:
        raise ContractError(f"{label} root is not a canonical absolute path")
    return resolved


def validate_identity(value: Any, label: str) -> dict[str, Any]:
    exact_keys(value, {"path", "bytes", "sha256"}, label)
    if not isinstance(value["path"], str) or require_int(value["bytes"], f"{label}.bytes", 1) <= 0 or not isinstance(value["sha256"], str) or not HEX64.fullmatch(value["sha256"]):
        raise ContractError(f"{label} is malformed")
    return value


def validate_wire_record(wire: Any, artifact_bytes: int, contract: dict[str, Any]) -> dict[str, Any]:
    exact_keys(wire, {"magic", "version", "profile_sha256", "schema_sha256", "section_count", "directory_bytes", "body_bytes", "body_sha256", "sections"}, "snapshot wire evidence")
    section_count = len(contract["wire"]["section_tags"])
    directory_bytes = section_count * (2 + 2 + 8 + 8 + 32)
    if wire["magic"] != contract["wire"]["magic"] or wire["version"] != 1 or wire["profile_sha256"] != contract["profile_sha256"] or wire["schema_sha256"] != contract["wire"]["schema_sha256"] or wire["section_count"] != section_count or wire["directory_bytes"] != directory_bytes:
        raise ContractError("snapshot wire evidence header differs")
    if not isinstance(wire["body_sha256"], str) or not HEX64.fullmatch(wire["body_sha256"]) or wire["body_bytes"] + len(contract["wire"]["magic"].encode("ascii")) + 4 + 32 + 32 + 4 + 8 + 32 + directory_bytes != artifact_bytes:
        raise ContractError("snapshot wire evidence body differs")
    if not isinstance(wire["sections"], list) or len(wire["sections"]) != section_count:
        raise ContractError("snapshot wire evidence section inventory differs")
    expected_offset = artifact_bytes - wire["body_bytes"]
    total = 0
    for ordinal, (section, tag, name) in enumerate(zip(wire["sections"], contract["wire"]["section_tags"], contract["wire"]["section_names"], strict=True)):
        exact_keys(section, {"ordinal", "tag", "name", "offset", "bytes", "sha256"}, "snapshot wire section")
        if section["ordinal"] != ordinal or section["tag"] != tag or section["name"] != name or section["offset"] != expected_offset or require_int(section["bytes"], "snapshot wire section bytes", 1) <= 0 or not isinstance(section["sha256"], str) or not HEX64.fullmatch(section["sha256"]):
            raise ContractError("snapshot wire section order/bounds differ")
        expected_offset += section["bytes"]
        total += section["bytes"]
    if expected_offset != artifact_bytes or total != wire["body_bytes"]:
        raise ContractError("snapshot wire sections do not exactly cover the body")
    return wire


def validate_build_record(
    build: Any,
    contract: dict[str, Any],
    repository_source: dict[str, Any],
    build_roots: list[Path],
) -> dict[str, Any]:
    exact_keys(build, {"command", "environment", "cargo_version", "rustc_version", "process", "cargo_lock", "cargo_configs", "toolchain_files", "cargo_home_before", "cargo_home_after", "effective_fingerprint", "source_before", "source_after", "libtest"}, "build evidence")
    source_keys = {"root", "git_commit", "git_tree", "git_status", "tracked_files", "tracked_bytes", "tracked_sha256"}
    exact_keys(build["source_before"], source_keys, "isolated source before build")
    exact_keys(build["source_after"], source_keys, "isolated source after build")
    if build["source_before"]["git_status"] or build["source_after"]["git_status"]:
        raise ContractError("isolated build source is not clean")
    binary = validate_identity(build["libtest"], "release libtest identity")
    expected = [str(executable("cargo")), *contract["libtest"]["build_args"]]
    if build["command"] != expected:
        raise ContractError("release libtest build command differs")
    process = validate_process_shape(build["process"], "release build process", allow_stderr=True)
    if process["argv"] != expected or process["cwd"] != build["source_before"]["root"] or process["env"] != build["environment"]:
        raise ContractError("release build raw invocation differs")
    for label in ("cargo_version", "rustc_version"):
        version = validate_process_shape(build[label], label)
        if version["cwd"] != build["source_before"]["root"] or version["env"] != build["environment"]:
            raise ContractError("toolchain version provenance differs")
    if not build["cargo_version"]["stdout"].startswith("cargo ") or not build["rustc_version"]["stdout"].startswith("rustc ") or build["cargo_version"]["argv"][0] == build["rustc_version"]["argv"][0]:
        raise ContractError("Cargo/rustc identities are not distinct real tool proxies")
    build_root = Path(build["source_before"]["root"]).parent.resolve()
    expected_environment = build_environment(
        build_root / "cargo-home", build_root / "target", build_roots
    )
    if build["environment"] != expected_environment:
        raise ContractError("release build environment is not configless/offline")
    if not same_source(build["source_before"], repository_source) or build["source_before"] != build["source_after"]:
        raise ContractError("isolated tracked source provenance differs")
    if build["cargo_configs"] != repo_cargo_configs() or build["toolchain_files"] != toolchain_identities():
        raise ContractError("Cargo config/toolchain file provenance differs")
    validate_identity(build["cargo_lock"], "Cargo.lock provenance")
    if build["cargo_lock"]["sha256"] != sha256_file(ROOT / "Cargo.lock"):
        raise ContractError("Cargo.lock provenance differs")
    exact_keys(build["effective_fingerprint"], {"file", "invoked_timestamp", "rustflags", "features", "profile", "config"}, "effective fingerprint")
    if build["effective_fingerprint"]["rustflags"] != canonical_rustflags(build_roots):
        raise ContractError("effective release rustflags differ")
    validate_identity(build["effective_fingerprint"]["file"], "Cargo fingerprint file")
    validate_identity(build["effective_fingerprint"]["invoked_timestamp"], "Cargo invoked timestamp")
    for when in ("cargo_home_before", "cargo_home_after"):
        exact_keys(build[when], {"cache_source", "exposed", "root", "registry_sources", "git_checkouts", "git_db"} if when == "cargo_home_before" else {"root", "registry_sources", "git_checkouts", "git_db"}, when)
        for tree_name in ("registry_sources", "git_checkouts", "git_db"):
            tree = exact_keys(build[when][tree_name], {"path", "files", "bytes", "sha256"}, f"{when}.{tree_name}")
            if not isinstance(tree["path"], str) or require_int(tree["files"], f"{when}.{tree_name}.files") < 0 or require_int(tree["bytes"], f"{when}.{tree_name}.bytes") < 0 or not isinstance(tree["sha256"], str) or not HEX64.fullmatch(tree["sha256"]):
                raise ContractError("Cargo dependency-source tree identity is malformed")
    if build["cargo_home_before"]["registry_sources"]["files"] != 0 or build["cargo_home_after"]["registry_sources"]["files"] <= 0:
        raise ContractError("dependency sources were not freshly materialized and attested")
    expected_cargo_home = str((build_root / "cargo-home").resolve())
    if build["cargo_home_before"]["root"] != expected_cargo_home or build["cargo_home_after"]["root"] != expected_cargo_home:
        raise ContractError("Cargo home changed during build")
    return binary


def validate_source_pair(
    before: Any, after: Any, expected: dict[str, Any], label: str
) -> None:
    keys = {"root", "git_commit", "git_tree", "git_status", "tracked_files", "tracked_bytes", "tracked_sha256"}
    exact_keys(before, keys, f"{label} source before")
    exact_keys(after, keys, f"{label} source after")
    if before != expected or after != expected or before != after:
        raise ContractError(f"{label} source provenance differs or mutated")


def validate_preflight(item: Any, binary: dict[str, Any], source: dict[str, Any], contract: dict[str, Any], seen: set[int]) -> dict[str, Any]:
    exact_keys(item, {"filter", "binary_before", "binary_after", "source_before", "source_after", "process"}, "preflight evidence")
    if item["filter"] != contract["libtest"]["preflight_filter"] or item["binary_before"] != binary or item["binary_after"] != binary:
        raise ContractError("preflight target identity differs")
    validate_source_pair(item["source_before"], item["source_after"], source, "preflight")
    process = validate_process_shape(item["process"], "preflight process")
    expected_environment = sanitized_environment() | {
        "TYPOKAT_WU0B_PROFILE_ROOT": source["root"]
    }
    if process["pid"] in seen or process["argv"] != preflight_command(Path(binary["path"]), contract) or process["cwd"] != source["root"] or process["env"] != expected_environment:
        raise ContractError("preflight process is reused or noncanonical")
    seen.add(process["pid"])
    passed = contract["libtest"]["preflight_passed"]
    ignored = contract["libtest"]["preflight_ignored"]
    if not re.search(rf"test result: ok\. {passed} passed; 0 failed; {ignored} ignored; 0 measured; \d+ filtered out", process["stdout"]):
        raise ContractError("preflight raw harness outcome differs")
    return process


def validate_evidence(evidence: Any, contract: dict[str, Any]) -> dict[str, Any]:
    exact_keys(evidence, {"schema", "verdict", "contract_sha256", "started_utc", "host", "source", "builds", "preflights", "profile", "artifact", "generations", "windows", "derived", "final_source"}, "evidence")
    if evidence["schema"] != 1 or evidence["verdict"] != "GO" or evidence["contract_sha256"] != sha256_file(CONTRACT_PATH):
        raise ContractError("evidence envelope differs")
    exact_keys(evidence["profile"], {"sha256", "file_count", "wu0a_oracles_sha256", "wu0a_workloads_sha256"}, "profile evidence")
    expected_profile = {"sha256": contract["profile_sha256"], "file_count": 82, "wu0a_oracles_sha256": contract["wu0a_oracles_sha256"], "wu0a_workloads_sha256": contract["wu0a_workloads_sha256"]}
    if evidence["profile"] != expected_profile:
        raise ContractError("profile provenance differs")
    exact_keys(evidence["source"], {"root", "git_commit", "git_tree", "git_status", "tracked_files", "tracked_bytes", "tracked_sha256"}, "source provenance")
    exact_keys(evidence["final_source"], {"root", "git_commit", "git_tree", "git_status", "tracked_files", "tracked_bytes", "tracked_sha256"}, "final source provenance")
    if not same_source(evidence["final_source"], evidence["source"]):
        raise ContractError("final source provenance changed")
    if evidence["source"].get("git_status") != "" or evidence["final_source"].get("git_status") != "":
        raise ContractError("authoritative source was not clean")
    host = exact_keys(evidence["host"], {"hostname", "kernel", "machine", "python", "cpu_model", "cpu_count", "affinity", "priority", "timezone", "rlimit_as", "rlimit_cpu", "rlimit_nofile"}, "host provenance")
    if host["priority"] != 0 or require_int(host["cpu_count"], "host cpu count", 2) < 2 or not isinstance(host["cpu_model"], str) or not host["cpu_model"] or not isinstance(host["affinity"], list) or len(host["affinity"]) < 2:
        raise ContractError("host priority/affinity is ineligible")
    builds = evidence["builds"]
    if not isinstance(builds, list) or len(builds) != 2:
        raise ContractError("two independent clean builds are required")
    build_roots = []
    for ordinal, build in enumerate(builds, 1):
        exact_keys(build, {"command", "environment", "cargo_version", "rustc_version", "process", "cargo_lock", "cargo_configs", "toolchain_files", "cargo_home_before", "cargo_home_after", "effective_fingerprint", "source_before", "source_after", "libtest"}, "build evidence")
        source_root = validated_source_root(
            build["source_before"], "isolated source before build"
        )
        build_root = source_root.parent.resolve()
        if source_root != build_root / "source" or build_root.name != f"build-{ordinal}":
            raise ContractError("isolated source root does not match its canonical build root")
        build_roots.append(build_root)
    if build_roots[0] == build_roots[1] or build_roots[0].parent != build_roots[1].parent:
        raise ContractError("isolated build roots are reused or do not share one run root")
    binaries = [validate_build_record(build, contract, evidence["source"], build_roots) for build in builds]
    if binaries[0]["path"] == binaries[1]["path"] or {key: binaries[0][key] for key in ("bytes", "sha256")} != {key: binaries[1][key] for key in ("bytes", "sha256")}:
        raise ContractError("independent builds were reused or produced different libtests")
    preflights = evidence["preflights"]
    if not isinstance(preflights, list) or len(preflights) != 2:
        raise ContractError("both independent libtests require full preflight")
    seen_pids: set[int] = set()
    build_sources = [build["source_after"] for build in builds]
    preflight_processes = [validate_preflight(item, binary, source, contract, seen_pids) for item, binary, source in zip(preflights, binaries, build_sources, strict=True)]
    artifact = evidence["artifact"]
    exact_keys(artifact, {"path", "bytes", "sha256", "wire"}, "artifact identity")
    validate_identity({key: artifact[key] for key in ("path", "bytes", "sha256")}, "artifact file identity")
    if not contract["artifact"]["minimum_bytes"] <= artifact["bytes"] <= contract["artifact"]["maximum_bytes"]:
        raise ContractError("artifact identity or size is ineligible")
    if artifact["bytes"] != contract["artifact"]["canonical_bytes"] or artifact["sha256"] != contract["artifact"]["canonical_sha256"]:
        raise ContractError("artifact does not match the canonical snapshot identity")
    validate_wire_record(artifact["wire"], artifact["bytes"], contract)
    generations = evidence["generations"]
    if not isinstance(generations, list) or len(generations) != 2:
        raise ContractError("two regenerations are required")
    outputs = []
    previous_generation_end = None
    for ordinal, item in enumerate(generations, 1):
        exact_keys(item, {"role", "filter", "binary_before", "binary_after", "artifact_before", "artifact_after", "source_before", "source_after", "process", "output", "wire", "semantic_calibration"}, "generation record")
        if item["role"] != "generation" or item["filter"] != contract["libtest"]["regeneration_filter"] or item["artifact_before"] is not None or item["artifact_after"] is not None:
            raise ContractError("generation record route differs")
        binary = binaries[ordinal - 1]
        if item["binary_before"] != binary or item["binary_after"] != binary:
            raise ContractError("generation libtest identity differs")
        source = build_sources[ordinal - 1]
        validate_source_pair(item["source_before"], item["source_after"], source, "generation")
        process = validate_process_shape(item["process"], "generation process")
        if process["pid"] in seen_pids:
            raise ContractError("regenerations reused one process")
        seen_pids.add(process["pid"])
        if process["started_monotonic_ns"] < preflight_processes[ordinal - 1]["ended_monotonic_ns"]:
            raise ContractError("regeneration preceded its full semantic preflight")
        if previous_generation_end is not None and process["started_monotonic_ns"] < previous_generation_end:
            raise ContractError("regeneration processes overlap or are reordered")
        previous_generation_end = process["ended_monotonic_ns"]
        output_identity = validate_identity(item["output"], "regenerated artifact identity")
        expected_output = output_identity["path"]
        expected_env = sanitized_environment() | {
            "TYPOKAT_WU0B_SNAPSHOT_OUTPUT": expected_output,
            "TYPOKAT_WU0B_PROFILE_ROOT": source["root"],
        }
        if process["env"] != expected_env or "TYPOKAT_WU0B_SNAPSHOT_INPUT" in process["env"]:
            raise ContractError("generation child environment differs")
        if process["argv"] != test_command(Path(binary["path"]), contract["libtest"]["regeneration_filter"], contract) or process["cwd"] != source["root"]:
            raise ContractError("generation command differs")
        assert_one_test(process, contract["libtest"]["regeneration_filter"])
        parsed_calibration = extract_semantic_calibration(process["stdout"], contract, output_identity)
        if item["semantic_calibration"] != parsed_calibration:
            raise ContractError("retained semantic calibration differs from raw output")
        if item["wire"] != artifact["wire"]:
            raise ContractError("external snapshot wire projections differ")
        outputs.append(output_identity)
    artifact_file = {key: artifact[key] for key in ("path", "bytes", "sha256")}
    if outputs[0] != artifact_file or outputs[1]["bytes"] != artifact["bytes"] or outputs[1]["sha256"] != artifact["sha256"] or outputs[0]["path"] == outputs[1]["path"]:
        raise ContractError("regeneration outputs are reused or differ")
    windows = evidence["windows"]
    if not isinstance(windows, list) or len(windows) != 3:
        raise ContractError("three timing windows are required")
    labels = set()
    previous_end = None
    for window_index, window in enumerate(windows, 1):
        exact_keys(window, {"index", "label", "started_monotonic_ns", "ended_monotonic_ns", "warmups", "recorded"}, "timing window")
        require_int(window["started_monotonic_ns"], "timing window start", 1)
        require_int(window["ended_monotonic_ns"], "timing window end", 1)
        if window["ended_monotonic_ns"] <= window["started_monotonic_ns"]:
            raise ContractError("timing window is empty or reversed")
        if window["index"] != window_index or not isinstance(window["label"], str) or not window["label"] or window["label"] in labels:
            raise ContractError("window indices/labels differ")
        labels.add(window["label"])
        if window_index == 1 and previous_generation_end is not None and window["started_monotonic_ns"] < previous_generation_end:
            raise ContractError("timing began before snapshot regeneration completed")
        if previous_end is not None and window["started_monotonic_ns"] - previous_end < contract["sampling"]["minimum_window_gap_seconds"] * 1_000_000_000:
            raise ContractError("timing windows are too close")
        previous_end = window["ended_monotonic_ns"]
        if len(window["warmups"]) != 5 or len(window["recorded"]) != 10:
            raise ContractError("timing schedule is incomplete")
        ordered = [*window["warmups"], *window["recorded"]]
        cursor = window["started_monotonic_ns"]
        for sample in ordered:
            process = sample["process"]
            if process["started_monotonic_ns"] < cursor or process["ended_monotonic_ns"] > window["ended_monotonic_ns"]:
                raise ContractError("timing intervals overlap, reverse, or escape the window")
            cursor = process["ended_monotonic_ns"]
        for ordinal, sample in enumerate(window["warmups"], 1):
            validate_sample(sample, binary=binaries[0], artifact=artifact_file, contract=contract, window=window_index, ordinal=ordinal, recorded=False, seen_pids=seen_pids)
        for ordinal, sample in enumerate(window["recorded"], 1):
            validate_sample(sample, binary=binaries[0], artifact=artifact_file, contract=contract, window=window_index, ordinal=ordinal, recorded=True, seen_pids=seen_pids)
    derived = derive(windows, contract)
    if evidence["derived"] != derived:
        raise ContractError("derived p95/RSS do not match raw external records")
    return evidence


def validate_output_path(output: Path) -> Path:
    evidence_root = EVIDENCE_ROOT.resolve()
    resolved = output.resolve()
    if resolved.parent != evidence_root or resolved.suffix != ".json" or resolved.name.startswith("."):
        raise ContractError("evidence output must be a direct non-hidden .json file in tooling/full-lib-snapshot/evidence")
    if output.exists():
        raise ContractError("evidence output already exists")
    if output.is_symlink() or EVIDENCE_ROOT.is_symlink():
        raise ContractError("evidence output/root cannot be symlinks")
    return resolved


def write_evidence(output: Path, evidence: dict[str, Any], *, require_clean: bool) -> None:
    EVIDENCE_ROOT.mkdir(parents=True, exist_ok=True)
    candidate = EVIDENCE_ROOT / f".{output.name}.{os.getpid()}.candidate"
    if candidate.exists() or candidate.is_symlink():
        raise ContractError("evidence candidate path already exists")
    with candidate.open("x", encoding="utf-8") as destination:
        json.dump(evidence, destination, sort_keys=True, separators=(",", ":"))
        destination.write("\n")
        destination.flush()
        os.fsync(destination.fileno())
    after_candidate = source_snapshot()
    if require_clean and after_candidate["git_status"]:
        candidate.unlink()
        raise ContractError("worktree became dirty while writing authoritative evidence")
    os.replace(candidate, output)
    after_output = source_snapshot()
    if require_clean and after_output["git_status"]:
        raise ContractError("worktree became dirty after final evidence installation")
    if require_clean and not same_source(after_candidate, after_output):
        raise ContractError("tracked source changed while writing evidence")


def no_go_envelope(started: str, stage: str, error: str, partial: dict[str, Any]) -> dict[str, Any]:
    try:
        final = source_snapshot()
    except (ContractError, OSError) as failure:
        final = {"capture_error": str(failure)}
    return {"schema": 1, "verdict": "NO-GO", "contract_sha256": sha256_file(CONTRACT_PATH), "started_utc": started, "failed_stage": stage, "error": error, "partial": partial, "final_source": final}


def validate_no_go(evidence: Any) -> dict[str, Any]:
    exact_keys(evidence, {"schema", "verdict", "contract_sha256", "started_utc", "failed_stage", "error", "partial", "final_source"}, "NO-GO evidence")
    if evidence["schema"] != 1 or evidence["verdict"] != "NO-GO" or evidence["contract_sha256"] != sha256_file(CONTRACT_PATH) or not isinstance(evidence["error"], str) or not evidence["error"] or not isinstance(evidence["partial"], dict):
        raise ContractError("NO-GO evidence envelope differs")
    return evidence


def collect_run(output: Path, labels: list[str]) -> dict[str, Any]:
    output = validate_output_path(output)
    started = datetime.now(timezone.utc).isoformat()
    partial: dict[str, Any] = {"host": None, "source": None, "builds": [], "preflights": [], "generations": [], "timing_processes": [], "windows": []}
    stage = "contract"
    try:
        contract = verify_contract()
        if len(labels) != 3 or len(set(labels)) != 3 or any(not label.strip() for label in labels):
            raise ContractError("three distinct non-empty window labels are required")
        stage = "host"
        partial["host"] = host_provenance()
        stage = "clean-source"
        source = source_snapshot()
        partial["source"] = source
        if source["git_status"]:
            raise ContractError("authoritative run requires a completely clean worktree")
        run_root = Path(tempfile.mkdtemp(prefix="typokat-full-lib-snapshot-"))
        stage = "independent-builds"
        builds = []
        binaries = []
        build_roots = [run_root / f"build-{ordinal}" for ordinal in (1, 2)]
        for build_root in build_roots:
            build, binary = collect_build(
                contract,
                build_root,
                source,
                build_roots,
                partial["builds"],
            )
            builds.append(build)
            binaries.append(binary)
        identities = [file_identity(binary) for binary in binaries]
        if identities[0]["path"] == identities[1]["path"] or {key: identities[0][key] for key in ("bytes", "sha256")} != {key: identities[1][key] for key in ("bytes", "sha256")}:
            raise ContractError("two independent clean builds produced different/reused libtests")
        source_roots = [Path(build["source_after"]["root"]) for build in builds]
        stage = "release-preflight"
        preflights = [
            execute_preflight(binary, source_root, contract, partial["preflights"])
            for binary, source_root in zip(binaries, source_roots, strict=True)
        ]
        stage = "snapshot-regeneration"
        generations, artifact_path, artifact_file = collect_generations(binaries, source_roots, contract, run_root, partial["generations"])
        artifact = {**artifact_file, "wire": generations[0]["wire"]}
        stage = "timing"
        windows = []
        previous_end: int | None = None
        for index, label in enumerate(labels, 1):
            if previous_end is not None:
                remaining = contract["sampling"]["minimum_window_gap_seconds"] - ((time.monotonic_ns() - previous_end) / 1_000_000_000)
                if remaining > 0:
                    time.sleep(remaining)
            window = {"index": index, "label": label, "started_monotonic_ns": time.monotonic_ns(), "ended_monotonic_ns": 0, "warmups": [], "recorded": []}
            windows.append(window)
            partial["windows"] = windows
            for ordinal in range(1, 6):
                window["warmups"].append(collect_timing_sample(binaries[0], artifact_path, artifact_file, contract, window=index, ordinal=ordinal, recorded=False, progress=partial["timing_processes"]))
            for ordinal in range(1, 11):
                window["recorded"].append(collect_timing_sample(binaries[0], artifact_path, artifact_file, contract, window=index, ordinal=ordinal, recorded=True, progress=partial["timing_processes"]))
            window["ended_monotonic_ns"] = time.monotonic_ns()
            previous_end = window["ended_monotonic_ns"]
        stage = "gates"
        evidence = {"schema": 1, "verdict": "GO", "contract_sha256": sha256_file(CONTRACT_PATH), "started_utc": started, "host": partial["host"], "source": source, "builds": builds, "preflights": preflights, "profile": {"sha256": contract["profile_sha256"], "file_count": 82, "wu0a_oracles_sha256": contract["wu0a_oracles_sha256"], "wu0a_workloads_sha256": contract["wu0a_workloads_sha256"]}, "artifact": artifact, "generations": generations, "windows": windows, "derived": derive(windows, contract), "final_source": source_snapshot()}
        validate_evidence(evidence, contract)
        stage = "evidence-write"
        write_evidence(output, evidence, require_clean=True)
    except (ContractError, OSError) as error:
        envelope = no_go_envelope(started, stage, str(error), partial)
        validate_no_go(envelope)
        if output.exists() and output.is_file() and not output.is_symlink():
            output.unlink()
        write_evidence(output, envelope, require_clean=False)
        raise ContractError(f"NO-GO evidence written to {output}: {error}") from error
    print("GO")
    return evidence


def assert_red() -> None:
    contract = verify_contract()
    source = source_snapshot()
    if source["git_status"]:
        raise ContractError("RED witness requires a completely clean worktree")
    run_root = Path(tempfile.mkdtemp(prefix="typokat-full-lib-snapshot-red-"))
    build_roots = [run_root / f"build-{ordinal}" for ordinal in (1, 2)]
    build, binary = collect_build(contract, build_roots[0], source, build_roots)
    source_root = Path(build["source_after"]["root"])
    for key in ("regeneration_filter", "timing_filter"):
        output = run_root / f"absent-{key}.snapshot"
        generation = key == "regeneration_filter"
        environment = child_environment(output_path=output, profile_root=source_root) if generation else child_environment(input_path=ROOT / "Cargo.lock")
        result = execute_child(
            binary,
            contract["libtest"][key],
            contract,
            environment,
            cwd=source_root if generation else ROOT,
            source_root=source_root if generation else None,
        )["process"]
        if result["returncode"] != 0 or result["stderr"] or result["stdout"].count("running 0 tests") != 1 or not HARNESS_ZERO.search(result["stdout"]):
            raise ContractError(f"{key} is not the expected absent RED probe")
        if output.exists():
            raise ContractError("absent regeneration probe unexpectedly wrote an artifact")
    print("RED: exact WU0B release probes are absent")


def inspect(path: Path) -> None:
    contract = verify_contract()
    evidence = strict_json(path.read_text(encoding="utf-8"), "evidence")
    if isinstance(evidence, dict) and evidence.get("verdict") == "NO-GO":
        validate_no_go(evidence)
    else:
        validate_evidence(evidence, contract)
    print("INSPECTED-NON-AUTHORITATIVE")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("verify-contract")
    commands.add_parser("assert-red")
    run = commands.add_parser("run")
    run.add_argument("--output", type=Path, required=True)
    run.add_argument("--window-label", action="append", required=True)
    inspect_parser = commands.add_parser("inspect-evidence")
    inspect_parser.add_argument("path", type=Path)
    return result


def main() -> int:
    arguments = parser().parse_args()
    try:
        if arguments.command == "verify-contract":
            verify_contract()
            print("contract: PASS")
        elif arguments.command == "assert-red":
            assert_red()
        elif arguments.command == "run":
            collect_run(arguments.output, arguments.window_label)
        elif arguments.command == "inspect-evidence":
            inspect(arguments.path)
        else:
            raise AssertionError(arguments.command)
        return 0
    except (ContractError, OSError) as error:
        print(f"full-lib-snapshot: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

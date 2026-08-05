#!/usr/bin/env python3
"""Collect and validate the non-authoritative complete-combined probe."""

from __future__ import annotations

import argparse
import copy
from functools import lru_cache
import hashlib
import json
import math
from pathlib import Path
import statistics
import sys
import tempfile
import time
from typing import Any, Callable, Iterable


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
FULL_BENCH = ROOT / "tooling/full-lib-bench"
sys.path.insert(0, str(FULL_BENCH))
import full_lib_bench as bench


ROWS = ("fast-clean", "fast-errors", "collision", "fanout")
ROUTES = ("production", "one-pass", "tsgo")
TIMING_PANELS = (
    ("OT", "one-pass", "tsgo"),
    ("OP", "one-pass", "production"),
    ("PT", "production", "tsgo"),
)
MEMORY_PANELS = (
    ("OT", "one-pass", "tsgo"),
    ("PT", "production", "tsgo"),
)
BOOTSTRAP_RESAMPLES = 100_000
BOOTSTRAP_SEED = 20_260_721
COMMENT = b"// one-pass probe perturbation; semantics unchanged\n"
MAX_RSS_KIB = 524_288
MAX_RSS_RATIO = 1.25
PROFILE_SHA256 = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d"

FROZEN_DIGESTS = {
    "collector_sha256": "72038536ed20541e2245a887aeb842957f4fb75e1a507bc5e1dae1b3b9134a13",
    "contract_sha256": "8f86a78d6c8e0b4427cba7044180567dd776ba91a881268516798b2baffe0b22",
    "libraries_sha256": "eb9c05d9e53c95c1690a986c3bc5367d0f16dcf466e39e20712cfb443cc8f675",
    "workloads_sha256": "71372ae0c63f9b393f2f383ee3a15aae54a2fd25f8752b18fc296256a9955b6a",
    "oracles_sha256": "1b1297705791cd4702e3512561b5312c56477d63f8f4e5c14011562a74d220c8",
}


class ProbeError(RuntimeError):
    """A fail-closed probe collection or evidence failure."""


def _fail(label: str, error: Exception) -> ProbeError:
    return ProbeError(f"{label}: {error}")


def _exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProbeError(f"{label} must be an object")
    actual = set(value)
    if actual != expected:
        raise ProbeError(
            f"{label} schema keys differ: missing={sorted(expected - actual)}, "
            f"extra={sorted(actual - expected)}"
        )
    return value


def _int(value: Any, label: str, *, minimum: int | None = None) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ProbeError(f"{label} numeric value must be an integer, not bool")
    if minimum is not None and value < minimum:
        raise ProbeError(f"{label} numeric value is below {minimum}")
    return value


def _number(value: Any, label: str, *, positive: bool = False) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ProbeError(f"{label} must be a finite numeric value, not bool")
    result = float(value)
    if positive and result <= 0:
        raise ProbeError(f"{label} must be a positive finite number")
    return result


def _rotated(items: tuple[Any, ...], amount: int) -> tuple[Any, ...]:
    offset = amount % len(items)
    return items[offset:] + items[:offset]


def _validate_execution_conditions(value: Any, label: str) -> dict[str, Any]:
    conditions = _exact_keys(value, {"affinity", "nice", "rlimits"}, label)
    affinity = conditions["affinity"]
    if (
        not isinstance(affinity, list)
        or len(affinity) < 2
        or any(
            isinstance(cpu, bool) or not isinstance(cpu, int) or cpu < 0
            for cpu in affinity
        )
        or affinity != sorted(set(affinity))
    ):
        raise ProbeError(f"{label} affinity is malformed or non-canonical")
    if _int(conditions["nice"], f"{label} nice") != 0:
        raise ProbeError(f"{label} priority is not canonical")
    rlimits = _exact_keys(
        conditions["rlimits"], set(bench.RLIMIT_NAMES), f"{label} rlimits"
    )
    for name, values in rlimits.items():
        if not isinstance(values, list) or len(values) != 2:
            raise ProbeError(f"{label} {name} rlimit is malformed")
        for current in values:
            if current != "infinity":
                _int(current, f"{label} {name} rlimit", minimum=0)
    return conditions


def _capture_execution_conditions() -> dict[str, Any]:
    try:
        observed = copy.deepcopy(bench.execution_conditions())
    except bench.ContractError as error:
        raise _fail("execution condition snapshot", error) from error
    return _validate_execution_conditions(observed, "execution condition snapshot")


def _assert_execution_conditions(
    expected: dict[str, Any], label: str = "execution condition snapshot"
) -> None:
    try:
        observed = copy.deepcopy(bench.execution_conditions())
    except bench.ContractError as error:
        raise _fail(label, error) from error
    _validate_execution_conditions(observed, label)
    if bench.canonical_json(observed) != bench.canonical_json(expected):
        raise ProbeError(f"{label} drifted during collection")


def _sha256_file(path: Path) -> str:
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError as error:
        raise ProbeError(f"cannot read frozen reference {path}: {error}") from error


def verify_profile_inventory(directory: Path) -> int:
    try:
        locked = bench.profile_lock()
        expected = {item.path for item in locked}
        actual = {
            path.name
            for path in directory.iterdir()
            if path.is_file() and not path.is_symlink()
        }
        if actual != expected or any((directory / name).is_symlink() for name in expected):
            raise ProbeError(
                "library profile inventory drift: "
                f"missing={sorted(expected - actual)}, extra={sorted(actual - expected)}"
            )
        for item in locked:
            data = bench.verify_regular(directory / item.path, f"library {item.path}")
            if (len(data), bench.sha256_bytes(data)) != (item.size, item.sha256):
                raise ProbeError(f"library profile drift at {item.path}: bytes or SHA differs")
        return len(locked)
    except bench.ContractError as error:
        raise _fail("library profile drift", error) from error
    except OSError as error:
        raise _fail("library profile inventory", error) from error


def verify_references() -> dict[str, object]:
    paths = {
        "collector_sha256": FULL_BENCH / "full_lib_bench.py",
        "contract_sha256": bench.CONTRACT_PATH,
        "libraries_sha256": bench.LIBRARIES_LOCK,
        "workloads_sha256": bench.WORKLOADS_LOCK,
        "oracles_sha256": bench.ORACLES_PATH,
    }
    for name, path in paths.items():
        if _sha256_file(path) != FROZEN_DIGESTS[name]:
            noun = "oracle" if name == "oracles_sha256" else "reference"
            raise ProbeError(f"{noun} drift: {path.name} SHA differs from the frozen probe")
    try:
        contract = bench.verify_contract()
        if tuple(contract["rows"]) != ROWS:
            raise ProbeError("frozen row order differs")
        if contract["profile"]["length_framed_sha256"] != PROFILE_SHA256:
            raise ProbeError("library profile SHA differs")
        if contract["profile"]["file_count"] != 82:
            raise ProbeError("library profile count differs")
        verify_profile_inventory(bench.PROFILE / "lib")
        return {
            **FROZEN_DIGESTS,
            "profile_sha256": PROFILE_SHA256,
            "library_count": 82,
            "rows": list(ROWS),
        }
    except bench.ContractError as error:
        raise _fail("frozen reference drift", error) from error


def validate_production_info(value: Any) -> None:
    observed = _exact_keys(
        value,
        {"schema", "profile_sha256", "file_count", "provider_route"},
        "production route probe",
    )
    expected = {
        "schema": 1,
        "profile_sha256": PROFILE_SHA256,
        "file_count": 82,
        "provider_route": "production-default-library",
    }
    _int(observed["schema"], "production route probe schema", minimum=1)
    _int(observed["file_count"], "production route probe file count", minimum=1)
    if bench.canonical_json(observed) != bench.canonical_json(expected):
        raise ProbeError("production route probe differs from the exact provider contract")


def validate_probe_info(value: Any) -> None:
    observed = _exact_keys(
        value,
        {
            "schema",
            "profile_sha256",
            "file_count",
            "probe_route",
            "source_backed",
            "replay_index",
        },
        "one-pass route probe",
    )
    expected = {
        "schema": 1,
        "profile_sha256": PROFILE_SHA256,
        "file_count": 82,
        "probe_route": "test-only-complete-combined",
        "source_backed": True,
        "replay_index": False,
    }
    _int(observed["schema"], "one-pass route probe schema", minimum=1)
    _int(observed["file_count"], "one-pass route probe file count", minimum=1)
    if observed["source_backed"] is not True or observed["replay_index"] is not False:
        raise ProbeError("one-pass route probe booleans differ from the exact contract")
    if bench.canonical_json(observed) != bench.canonical_json(expected):
        raise ProbeError("one-pass route probe differs from the exact source-backed contract")


def three_route_preread_attestation(
    tsgo: Path, production: Path, one_pass: Path, row: str, block: int
) -> dict[str, Any]:
    paths = [
        one_pass.resolve(),
        production.resolve(),
        tsgo.resolve(),
        *(tsgo.parent / item.path for item in bench.profile_lock()),
        *bench.row_sources(row),
    ]
    started = time.monotonic_ns()
    digest = hashlib.sha256()
    total = 0
    try:
        for path in paths:
            data = bench.verify_regular(path, "one-pass pre-read input")
            encoded = str(path.resolve()).encode("utf-8")
            digest.update(len(encoded).to_bytes(8, "big"))
            digest.update(len(data).to_bytes(8, "big"))
            digest.update(encoded)
            digest.update(data)
            total += len(data)
    except bench.ContractError as error:
        raise _fail("pre-read attestation", error) from error
    ended = time.monotonic_ns()
    return {
        "block": block,
        "started_monotonic_ns": started,
        "ended_monotonic_ns": ended,
        "paths": [str(path.resolve()) for path in paths],
        "bytes": total,
        "sha256": digest.hexdigest(),
    }


def validate_output_path(output: Path) -> Path:
    candidate = output.expanduser().resolve(strict=False)
    authoritative = FULL_BENCH.resolve()
    if candidate == authoritative or authoritative in candidate.parents:
        raise ProbeError("output under authoritative full-lib-bench is forbidden")
    if output.is_symlink() or (candidate.exists() and not candidate.is_file()):
        raise ProbeError("output must be a non-symlink regular-file destination")
    if not candidate.parent.is_dir() or candidate.parent.is_symlink():
        raise ProbeError("output parent must be an existing non-symlink directory")
    return candidate


def validate_live_cli_paths(production: Path, one_pass: Path) -> None:
    expected = {
        "production": ROOT / "target/release/typokat",
        "one-pass": ROOT / "target/release/examples/one_pass_probe",
    }
    supplied = {"production": production.expanduser(), "one-pass": one_pass.expanduser()}
    for route, raw in supplied.items():
        lexical = raw if raw.is_absolute() else ROOT / raw
        canonical = expected[route]
        if lexical != canonical:
            raise ProbeError(
                f"live CLI {route} path is not the lexical canonical release path"
            )
        if lexical.is_symlink():
            raise ProbeError(f"live CLI canonical {route} release path is a symlink")
        if lexical.resolve(strict=False) != canonical:
            raise ProbeError(
                f"live CLI canonical {route} release path resolves through a symlink"
            )


class _Collector:
    def __init__(
        self,
        production: Path,
        one_pass: Path,
        tsgo: Path,
        contract: dict[str, Any],
        executor: Callable[..., bench.ProcessResult],
        identity_reader: Callable[[Path], dict[str, object]],
        prereader: Callable[[Path, Path, Path, str, int], dict[str, Any]],
        execution_conditions: dict[str, Any],
    ) -> None:
        self.paths = {
            "production": production.resolve(),
            "one-pass": one_pass.resolve(),
            "tsgo": tsgo.resolve(),
        }
        self.contract = contract
        self.executor = executor
        self.identity_reader = identity_reader
        self.prereader = prereader
        self.execution_conditions = copy.deepcopy(execution_conditions)
        self.registry: dict[str, Any] = {}
        self.sequence = 0
        self.identities = {
            route: self._read_identity(route, path) for route, path in self.paths.items()
        }

    def _read_identity(self, route: str, path: Path) -> dict[str, Any]:
        try:
            raw = self.identity_reader(path)
        except (OSError, bench.ContractError) as error:
            raise _fail(f"{route} executable identity", error) from error
        value = _exact_keys(raw, {"size", "sha256"}, f"{route} executable identity")
        size = _int(value["size"], f"{route} executable identity size", minimum=1)
        digest = value["sha256"]
        if not isinstance(digest, str) or not bench.HEX64_RE.fullmatch(digest):
            raise ProbeError(f"{route} executable identity SHA is malformed")
        return {"path": str(path), "size": size, "sha256": digest}

    def command(self, route: str, row: str, inputs: list[Path] | None = None) -> list[str]:
        binary = self.paths[route]
        if inputs is None:
            if route == "tsgo":
                return bench.tsgo_command(binary, row, self.contract)
            return bench.typokat_command(binary, row, self.contract)
        if route == "tsgo":
            command = [str(binary), *self.contract["typescript_flags"], *map(str, inputs)]
        else:
            command = [str(binary), *self.contract["typokat_flags"], *map(str, inputs)]
        try:
            bench.reject_forbidden_command(command, self.contract, allow_incremental_false=False)
        except bench.ContractError as error:
            raise _fail("canonical command", error) from error
        return command

    def execute(
        self, route: str, phase: str, argv: list[str], *, memory: bool = False
    ) -> dict[str, Any]:
        _assert_execution_conditions(
            self.execution_conditions, f"{phase} execution condition snapshot"
        )
        command = ["/usr/bin/time", "-v", *argv] if memory else argv
        try:
            descriptor = bench.invocation_descriptor(
                command, bench.sanitized_environment(), ROOT
            )
            descriptor_conditions = {
                "affinity": descriptor["affinity"],
                "nice": descriptor["nice"],
                "rlimits": descriptor["rlimits"],
            }
            _validate_execution_conditions(
                descriptor_conditions,
                f"{phase} invocation execution condition snapshot",
            )
            if bench.canonical_json(descriptor_conditions) != bench.canonical_json(
                self.execution_conditions
            ):
                raise ProbeError(
                    f"{phase} invocation execution condition snapshot drifted"
                )
            invocation = bench.sha256_bytes(bench.canonical_json(descriptor))
            previous = self.registry.setdefault(invocation, descriptor)
            if previous != descriptor:
                raise ProbeError("invocation digest collision")
            guarded = self.paths[route]
            before = self._read_identity(route, guarded)
            result = self.executor(
                command,
                timeout=self.contract["sampling"]["timeout_seconds"],
                max_output=self.contract["sampling"]["max_output_bytes"],
                cwd=ROOT,
                extra_environment=None,
            )
            after = self._read_identity(route, guarded)
        except bench.ContractError as error:
            raise _fail(f"{phase} process execution", error) from error
        if before != after or before != self.identities[route]:
            raise ProbeError(f"{route} executable identity changed during collection")
        self.sequence += 1
        record = bench.process_record(
            invocation,
            result,
            {
                "path": str(guarded),
                "before": {"size": before["size"], "sha256": before["sha256"]},
                "after": {"size": after["size"], "sha256": after["sha256"]},
            },
        )
        record.update(
            {
                "sequence": self.sequence,
                "route": route,
                "phase": phase,
                "argv": list(command),
            }
        )
        if memory:
            try:
                _, rss = bench.compiler_result_without_time(record, descriptor)
            except bench.ContractError as error:
                raise _fail("memory RSS report", error) from error
            record["rss_kib"] = rss
        return record


def _parse_json_output(record: dict[str, Any], label: str) -> Any:
    if record["returncode"] != 0 or record["stderr"] or not record["stdout"].endswith("\n"):
        raise ProbeError(f"{label} route probe exit or output channel differs")
    if record["stdout"].count("\n") != 1:
        raise ProbeError(f"{label} route probe must emit exactly one JSON line")
    try:
        return bench.strict_json_loads(record["stdout"], f"{label} route probe JSON")
    except bench.ContractError as error:
        raise _fail(f"{label} route probe JSON", error) from error


def _oracle(row: str, *, shifted: bool = False) -> dict[str, Any]:
    try:
        original = bench.load_oracles()["rows"][row]
    except bench.ContractError as error:
        raise _fail("semantic oracle", error) from error
    diagnostics = list(original["diagnostics"])
    if shifted:
        moved = []
        for diagnostic in diagnostics:
            path, line, column, code = diagnostic.rsplit(":", 3)
            moved.append(f"{path}:{int(line) + 1}:{column}:{code}")
        diagnostics = sorted(moved)
    return {"exit": original["exit"], "diagnostics": diagnostics}


def _strip_gnu_time_exit_status(
    result: bench.ProcessResult, expected_returncode: int
) -> bench.ProcessResult:
    lines = result.stderr.splitlines(keepends=True)
    time_owned_prefixes = (
        "Command exited with non-zero status ",
        "Command terminated by signal ",
        "Command stopped by signal ",
    )
    owned = [
        (ordinal, line)
        for ordinal, line in enumerate(lines)
        if line.startswith(time_owned_prefixes)
    ]
    if expected_returncode == 0 and not owned:
        return result
    expected_line = f"Command exited with non-zero status {expected_returncode}\n"
    if (
        expected_returncode <= 0
        or result.returncode != expected_returncode
        or owned != [(len(lines) - 1, expected_line)]
    ):
        raise ProbeError(
            "GNU time status/signal line does not match the expected child exit"
        )
    return bench.ProcessResult(
        result.argv,
        result.returncode,
        result.stdout,
        "".join(lines[:-1]),
        result.wall_seconds,
        result.pid,
        result.started_monotonic_ns,
        result.ended_monotonic_ns,
        result.group_clean,
    )


def _assert_oracle(
    record: dict[str, Any], row: str, descriptor: dict[str, Any], *,
    shifted: bool = False, memory: bool = False, path_map: dict[str, str] | None = None,
) -> None:
    expected = _oracle(row, shifted=shifted)
    try:
        result = bench.result_from_record(record, descriptor)
        if memory:
            result, parsed_rss = bench.compiler_result_without_time(record, descriptor)
            if parsed_rss != record.get("rss_kib"):
                raise ProbeError(f"{row} memory RSS differs from raw /usr/bin/time report")
            result = _strip_gnu_time_exit_status(result, expected["exit"])
        tool = "tsgo" if record["route"] == "tsgo" else "typokat"
        diagnostics = bench.normalize_diagnostics(result, tool, row, path_map)
    except bench.ContractError as error:
        raise _fail(f"{row} semantic oracle/output", error) from error
    if result.returncode != expected["exit"] or diagnostics != expected["diagnostics"]:
        raise ProbeError(f"{row} semantic oracle/output differs for {record['route']}")


def _control_assets(root: Path) -> tuple[dict[str, list[Path]], dict[str, Any]]:
    paths: dict[str, list[Path]] = {}
    retained: dict[str, Any] = {}
    for row in ROWS:
        directory = root / row
        directory.mkdir(parents=True)
        paths[row] = []
        files = []
        for ordinal, original in enumerate(bench.row_sources(row)):
            target = (directory / f"renamed-{ordinal:02d}.ts").resolve()
            source = COMMENT + original.read_bytes()
            target.write_bytes(source)
            paths[row].append(target)
            files.append(
                {
                    "original": str(original.resolve()),
                    "renamed": str(target),
                    "source": source.decode("utf-8"),
                }
            )
        retained[row] = {"files": files}
    return paths, retained


def _path_map(row: str, files: list[dict[str, Any]]) -> dict[str, str]:
    originals = bench.row_sources(row)
    return {
        str(Path(item["renamed"]).resolve()): original.relative_to(bench.WORKLOADS).as_posix()
        for item, original in zip(files, originals, strict=True)
    }


def _panel(
    collector: _Collector, name: str, left: str, right: str, row: str, phase: str,
    *, memory: bool = False,
) -> dict[str, Any]:
    routes = (left, right, right, left)
    records = [
        collector.execute(route, phase, collector.command(route, row), memory=memory)
        for route in routes
    ]
    return {"name": name, "left": left, "right": right, "records": records}


def _collect_raw(
    collector: _Collector, controls: dict[str, list[Path]], retained: dict[str, Any]
) -> dict[str, Any]:
    production_args = ["library-info", "--format", "json"]
    one_pass_args = ["probe-info", "--format", "json"]
    route_probes: dict[str, Any] = {}
    for route, args in (("production", production_args), ("one-pass", one_pass_args)):
        record = collector.execute(
            route, "route-probe", [str(collector.paths[route]), *args]
        )
        observed = _parse_json_output(record, route)
        (validate_production_info if route == "production" else validate_probe_info)(observed)
        route_probes[route] = {"args": args, "observed": observed, "record": record}

    inventories: dict[str, Any] = {}
    for row in ROWS:
        argv = bench.tsgo_command(collector.paths["tsgo"], row, collector.contract, list_files=True)
        record = collector.execute("tsgo", "inventory", argv)
        if record["returncode"] != 0 or record["stderr"]:
            raise ProbeError(f"{row} listFiles inventory process failed")
        listed = [Path(line).resolve() for line in record["stdout"].splitlines() if line]
        expected = [
            *(collector.paths["tsgo"].parent / item.path for item in bench.profile_lock()),
            *bench.row_sources(row),
        ]
        try:
            bench.verify_listed_inventory(listed, [path.resolve() for path in expected], row)
        except bench.ContractError as error:
            raise _fail(f"{row} listFiles inventory", error) from error
        inventories[row] = record

    semantics: dict[str, Any] = {}
    control_evidence: dict[str, Any] = {}
    for row in ROWS:
        semantics[row] = {}
        for route in ROUTES:
            record = collector.execute(route, "semantics", collector.command(route, row))
            _assert_oracle(record, row, collector.registry[record["invocation"]])
            semantics[row][route] = record
        files = retained[row]["files"]
        path_map = _path_map(row, files)
        records: dict[str, Any] = {}
        for route in ROUTES:
            record = collector.execute(
                route, "control", collector.command(route, row, controls[row])
            )
            _assert_oracle(
                record,
                row,
                collector.registry[record["invocation"]],
                shifted=True,
                path_map=path_map,
            )
            records[route] = record
        control_evidence[row] = {"files": files, "records": records}

    timing: dict[str, Any] = {}
    route_cycle = ("one-pass", "production", "tsgo")
    for row in ROWS:
        warmups = []
        for ordinal in range(5):
            current = []
            for route in _rotated(route_cycle, ordinal):
                record = collector.execute(route, "warmup", collector.command(route, row))
                _assert_oracle(record, row, collector.registry[record["invocation"]])
                current.append(record)
            warmups.append(current)
        superblocks = []
        for ordinal in range(15):
            pre_read = collector.prereader(
                collector.paths["tsgo"],
                collector.paths["production"],
                collector.paths["one-pass"],
                row,
                ordinal,
            )
            panels = []
            for name, left, right in _rotated(TIMING_PANELS, ordinal):
                current = _panel(collector, name, left, right, row, "timing")
                for record in current["records"]:
                    _assert_oracle(record, row, collector.registry[record["invocation"]])
                panels.append(current)
            superblocks.append({"block": ordinal, "pre_read": pre_read, "panels": panels})
        timing[row] = {"warmups": warmups, "superblocks": superblocks}

    memory: dict[str, Any] = {}
    for row in ROWS:
        superblocks = []
        for ordinal in range(5):
            pre_read = collector.prereader(
                collector.paths["tsgo"],
                collector.paths["production"],
                collector.paths["one-pass"],
                row,
                ordinal,
            )
            order = MEMORY_PANELS if ordinal % 2 == 0 else tuple(reversed(MEMORY_PANELS))
            panels = []
            for name, left, right in order:
                current = _panel(
                    collector, name, left, right, row, "memory", memory=True
                )
                for record in current["records"]:
                    _assert_oracle(
                        record,
                        row,
                        collector.registry[record["invocation"]],
                        memory=True,
                    )
                panels.append(current)
            superblocks.append({"block": ordinal, "pre_read": pre_read, "panels": panels})
        memory[row] = {"superblocks": superblocks}

    return {
        "route_probes": route_probes,
        "inventories": inventories,
        "semantics": semantics,
        "controls": control_evidence,
        "timing": timing,
        "memory": memory,
    }


def run_probe(
    *,
    production: Path,
    one_pass: Path,
    tsgo: Path,
    output: Path,
    executor: Callable[..., bench.ProcessResult] = bench.run_process,
    identity_reader: Callable[[Path], dict[str, object]] = bench.executable_identity,
    prereader: Callable[[Path, Path, Path, str, int], dict[str, Any]] = three_route_preread_attestation,
    comparator_verifier: Callable[[Path, dict[str, Any]], None] = bench.verify_staged_comparator,
) -> dict[str, Any]:
    destination = validate_output_path(output)
    if destination.exists():
        raise ProbeError("output already exists; refusing to overwrite evidence")
    references = verify_references()
    execution_conditions = _capture_execution_conditions()
    _assert_execution_conditions(
        execution_conditions, "comparator execution condition snapshot"
    )
    try:
        contract = bench.verify_contract()
        comparator_verifier(tsgo.resolve(), contract)
    except bench.ContractError as error:
        raise _fail("staged comparator verification", error) from error
    with tempfile.TemporaryDirectory(prefix="typokat-one-pass-controls-") as temporary:
        controls, retained = _control_assets(Path(temporary))
        collector = _Collector(
            production,
            one_pass,
            tsgo,
            contract,
            executor,
            identity_reader,
            prereader,
            execution_conditions,
        )
        raw = _collect_raw(collector, controls, retained)
        evidence = {
            "schema": 1,
            "authority": "non-authoritative-probe",
            "execution_conditions": copy.deepcopy(execution_conditions),
            "references": references,
            "identities": collector.identities,
            "invocations": collector.registry,
            **raw,
            "verdict": "NOT-PROMISING",
        }
        evidence["verdict"] = _derive_verdict(evidence)
        _assert_execution_conditions(
            execution_conditions, "pre-validation execution condition snapshot"
        )
        validate_evidence(evidence, destination)
        _assert_execution_conditions(
            execution_conditions, "pre-write execution condition snapshot"
        )
        destination.write_text(
            json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        return evidence


def _identity(value: Any, route: str) -> dict[str, Any]:
    item = _exact_keys(value, {"path", "size", "sha256"}, f"{route} identity")
    path = item["path"]
    if not isinstance(path, str) or not Path(path).is_absolute():
        raise ProbeError(f"{route} identity path must be absolute")
    if path != str(Path(path).resolve()):
        raise ProbeError(f"{route} identity path is not canonical")
    _int(item["size"], f"{route} identity size", minimum=1)
    if not isinstance(item["sha256"], str) or not bench.HEX64_RE.fullmatch(item["sha256"]):
        raise ProbeError(f"{route} identity SHA is malformed")
    return item


def _expected_paths(identities: dict[str, Any], row: str) -> list[Path]:
    tsgo = Path(identities["tsgo"]["path"])
    return [
        Path(identities["one-pass"]["path"]),
        Path(identities["production"]["path"]),
        tsgo,
        *(tsgo.parent / item.path for item in bench.profile_lock()),
        *bench.row_sources(row),
    ]


def _validate_preread(value: Any, identities: dict[str, Any], row: str, block: int) -> None:
    record = _exact_keys(
        value,
        {"block", "started_monotonic_ns", "ended_monotonic_ns", "paths", "bytes", "sha256"},
        f"{row} pre-read attestation",
    )
    if _int(record["block"], f"{row} pre-read block", minimum=0) != block:
        raise ProbeError(f"{row} pre-read block order differs")
    started = _int(record["started_monotonic_ns"], f"{row} pre-read start", minimum=1)
    ended = _int(record["ended_monotonic_ns"], f"{row} pre-read end", minimum=1)
    if ended <= started:
        raise ProbeError(f"{row} pre-read chronology is reversed")
    paths = _expected_paths(identities, row)
    expected_names = [str(path.resolve()) for path in paths]
    if record["paths"] != expected_names:
        raise ProbeError(f"{row} pre-read path inventory/order differs")
    digest = hashlib.sha256()
    total = 0
    try:
        for path in paths:
            data = bench.verify_regular(path, f"{row} pre-read input")
            encoded = str(path.resolve()).encode("utf-8")
            digest.update(len(encoded).to_bytes(8, "big"))
            digest.update(len(data).to_bytes(8, "big"))
            digest.update(encoded)
            digest.update(data)
            total += len(data)
    except bench.ContractError as error:
        raise _fail(f"{row} pre-read attestation", error) from error
    if record["bytes"] != total or record["sha256"] != digest.hexdigest():
        raise ProbeError(f"{row} pre-read attestation bytes/SHA differs")


def _expected_command(
    route: str,
    row: str | None,
    phase: str,
    paths: dict[str, Path],
    contract: dict[str, Any],
    control_files: list[dict[str, Any]] | None = None,
    *,
    memory: bool = False,
) -> list[str]:
    if phase == "route-probe":
        args = (
            ["library-info", "--format", "json"]
            if route == "production"
            else ["probe-info", "--format", "json"]
        )
        command = [str(paths[route]), *args]
    elif phase == "inventory":
        if row is None or route != "tsgo":
            raise ProbeError("canonical inventory command route/row differs")
        command = bench.tsgo_command(paths["tsgo"], row, contract, list_files=True)
    elif phase == "control":
        if row is None or control_files is None:
            raise ProbeError("canonical control command row/files are absent")
        inputs = [Path(item["renamed"]) for item in control_files]
        if route == "tsgo":
            command = [str(paths[route]), *contract["typescript_flags"], *map(str, inputs)]
        else:
            command = [str(paths[route]), *contract["typokat_flags"], *map(str, inputs)]
    else:
        if row is None:
            raise ProbeError(f"canonical {phase} command row is absent")
        command = (
            bench.tsgo_command(paths[route], row, contract)
            if route == "tsgo"
            else bench.typokat_command(paths[route], row, contract)
        )
    return ["/usr/bin/time", "-v", *command] if memory else command


class _EvidenceValidator:
    def __init__(self, evidence: dict[str, Any], contract: dict[str, Any]) -> None:
        self.evidence = evidence
        self.contract = contract
        self.execution_conditions = copy.deepcopy(
            _validate_execution_conditions(
                evidence["execution_conditions"],
                "evidence execution condition snapshot",
            )
        )
        self.registry = _exact_keys(
            evidence["invocations"], set(evidence["invocations"]), "invocation registry"
        )
        if not self.registry:
            raise ProbeError("invocation registry is empty")
        self.used: set[str] = set()
        self.pids: set[int] = set()
        self.next_sequence = 1
        self.cursor: int | None = None
        self.identities = {
            route: _identity(evidence["identities"][route], route) for route in ROUTES
        }
        self.paths = {route: Path(value["path"]).resolve() for route, value in self.identities.items()}
        for route in ("production", "one-pass"):
            try:
                current = bench.executable_identity(self.paths[route])
            except bench.ContractError as error:
                raise _fail(f"{route} executable identity", error) from error
            expected = {
                "size": self.identities[route]["size"],
                "sha256": self.identities[route]["sha256"],
            }
            if current != expected:
                raise ProbeError(f"{route} executable identity is stale")
        comparator = contract["comparator"]
        if (
            self.identities["tsgo"]["size"] != comparator["binary_size"]
            or self.identities["tsgo"]["sha256"] != comparator["binary_sha256"]
        ):
            raise ProbeError("tsgo executable identity differs from the frozen comparator")
        self._validate_registry()

    def _validate_registry(self) -> None:
        for digest, descriptor in self.registry.items():
            if not isinstance(digest, str) or not bench.HEX64_RE.fullmatch(digest):
                raise ProbeError("invocation digest is malformed")
            item = _exact_keys(
                descriptor,
                {"argv", "env", "cwd", "affinity", "nice", "rlimits"},
                "invocation descriptor",
            )
            if bench.sha256_bytes(bench.canonical_json(item)) != digest:
                raise ProbeError("invocation descriptor digest differs")
            if item["env"] != bench.sanitized_environment():
                raise ProbeError("invocation environment is not the canonical sanitized environment")
            if item["cwd"] != str(ROOT.resolve()):
                raise ProbeError("invocation cwd is not canonical")
            invocation_conditions = {
                "affinity": item["affinity"],
                "nice": item["nice"],
                "rlimits": item["rlimits"],
            }
            _validate_execution_conditions(
                invocation_conditions, "invocation execution condition snapshot"
            )
            if bench.canonical_json(invocation_conditions) != bench.canonical_json(
                self.execution_conditions
            ):
                raise ProbeError(
                    "invocation execution condition snapshot differs from evidence"
                )
            argv = item["argv"]
            if not isinstance(argv, list) or not argv or any(not isinstance(arg, str) for arg in argv):
                raise ProbeError("invocation argv is malformed")
            try:
                bench.reject_forbidden_command(argv, self.contract, allow_incremental_false=False)
            except bench.ContractError as error:
                raise _fail("canonical invocation flag", error) from error

    def interval(self, value: dict[str, Any], label: str, *, process: bool) -> None:
        started = _int(value["started_monotonic_ns"], f"{label} start time", minimum=1)
        ended = _int(value["ended_monotonic_ns"], f"{label} end time", minimum=1)
        if ended < started or (process and ended == started):
            raise ProbeError(f"{label} chronology interval is reversed or empty")
        if self.cursor is not None and started < self.cursor:
            raise ProbeError(f"{label} chronology overlaps or is out of order")
        self.cursor = ended

    def record(
        self,
        value: Any,
        route: str,
        row: str | None,
        phase: str,
        *,
        control_files: list[dict[str, Any]] | None = None,
        memory: bool = False,
        shifted: bool = False,
    ) -> dict[str, Any]:
        keys = {
            "invocation",
            "pid",
            "started_monotonic_ns",
            "ended_monotonic_ns",
            "wall_seconds",
            "returncode",
            "stdout",
            "stderr",
            "group_clean",
            "executable",
            "sequence",
            "route",
            "phase",
            "argv",
        }
        if memory:
            keys.add("rss_kib")
        item = _exact_keys(value, keys, f"{phase} process record")
        if item["route"] != route or item["phase"] != phase:
            raise ProbeError(f"{phase} process route/phase differs")
        sequence = _int(item["sequence"], f"{phase} sequence", minimum=1)
        if sequence != self.next_sequence:
            raise ProbeError("global process sequence is duplicated or out of order")
        self.next_sequence += 1
        pid = _int(item["pid"], f"{phase} PID", minimum=1)
        if pid in self.pids:
            raise ProbeError(f"{phase} PID was reused")
        self.pids.add(pid)
        self.interval(item, f"{phase} process", process=True)
        wall = _number(item["wall_seconds"], f"{phase} wall time", positive=True)
        interval = (item["ended_monotonic_ns"] - item["started_monotonic_ns"]) / 1_000_000_000
        if not math.isclose(wall, interval, rel_tol=1e-12, abs_tol=1e-12):
            raise ProbeError(f"{phase} wall time differs from monotonic interval")
        _int(item["returncode"], f"{phase} returncode")
        if not isinstance(item["stdout"], str) or not isinstance(item["stderr"], str):
            raise ProbeError(f"{phase} raw output must be text")
        limit = self.contract["sampling"]["max_output_bytes"]
        if len(item["stdout"].encode()) > limit or len(item["stderr"].encode()) > limit:
            raise ProbeError(f"{phase} raw output exceeds the live cap")
        if item["group_clean"] is not True:
            raise ProbeError(f"{phase} process group was not contained")
        expected = _expected_command(
            route,
            row,
            phase,
            self.paths,
            self.contract,
            control_files,
            memory=memory,
        )
        if item["argv"] != expected:
            raise ProbeError(f"{phase} canonical command/flag argv differs")
        invocation = item["invocation"]
        if not isinstance(invocation, str) or invocation not in self.registry:
            raise ProbeError(f"{phase} invocation binding is absent")
        descriptor = self.registry[invocation]
        expected_descriptor = {
            "argv": expected,
            "env": bench.sanitized_environment(),
            "cwd": str(ROOT.resolve()),
            **copy.deepcopy(self.execution_conditions),
        }
        expected_digest = bench.sha256_bytes(bench.canonical_json(expected_descriptor))
        if invocation != expected_digest or self.registry[invocation] != expected_descriptor:
            raise ProbeError(f"{phase} canonical invocation descriptor differs")
        self.used.add(invocation)
        executable = _exact_keys(
            item["executable"], {"path", "before", "after"}, f"{phase} executable"
        )
        identity = self.identities[route]
        expected_identity = {"size": identity["size"], "sha256": identity["sha256"]}
        if (
            executable["path"] != str(self.paths[route])
            or executable["before"] != expected_identity
            or executable["after"] != expected_identity
        ):
            raise ProbeError(f"{phase} executable identity changed or differs")
        if row is not None and phase != "inventory":
            path_map = _path_map(row, control_files) if shifted and control_files is not None else None
            _assert_oracle(
                item,
                row,
                descriptor,
                shifted=shifted,
                memory=memory,
                path_map=path_map,
            )
        if memory:
            rss = _int(item["rss_kib"], f"{phase} RSS", minimum=1)
            try:
                _, parsed = bench.compiler_result_without_time(item, descriptor)
            except bench.ContractError as error:
                raise _fail("memory RSS report", error) from error
            if parsed != rss:
                raise ProbeError("memory RSS differs from raw /usr/bin/time report")
        return item

    def preread(self, value: Any, row: str, block: int) -> None:
        _validate_preread(value, self.identities, row, block)
        self.interval(value, f"{row} pre-read", process=False)

    def finish(self) -> None:
        if self.used != set(self.registry):
            raise ProbeError("invocation registry contains unused or unbound descriptors")


def _validate_control_files(value: Any, row: str) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise ProbeError(f"{row} control files must be a list")
    originals = bench.row_sources(row)
    if len(value) != len(originals):
        raise ProbeError(f"{row} control file count differs")
    result = []
    renamed: set[str] = set()
    for item, original in zip(value, originals, strict=True):
        file = _exact_keys(item, {"original", "renamed", "source"}, f"{row} control file")
        if file["original"] != str(original.resolve()):
            raise ProbeError(f"{row} control original path differs")
        if not isinstance(file["renamed"], str) or not Path(file["renamed"]).is_absolute():
            raise ProbeError(f"{row} control renamed path is malformed")
        if file["renamed"] == file["original"] or file["renamed"] in renamed:
            raise ProbeError(f"{row} control renamed path is unchanged or duplicated")
        renamed.add(file["renamed"])
        try:
            expected = (COMMENT + original.read_bytes()).decode("utf-8")
        except (OSError, UnicodeDecodeError) as error:
            raise _fail(f"{row} control source", error) from error
        if file["source"] != expected:
            raise ProbeError(f"{row} control source bytes differ")
        result.append(file)
    return result


def _panels_from(evidence: dict[str, Any], row: str, name: str, *, memory: bool) -> list[dict[str, Any]]:
    section = evidence["memory" if memory else "timing"][row]
    return [
        next(panel for panel in block["panels"] if panel["name"] == name)
        for block in section["superblocks"]
    ]


@lru_cache(maxsize=128)
def _bootstrap_lower_cached(
    blocks: tuple[tuple[tuple[str, float], ...], ...], seed: int
) -> float:
    converted = [
        [
            {"tool": "typokat" if index in (0, 3) else "tsgo", "wall_seconds": value}
            for index, (_, value) in enumerate(block)
        ]
        for block in blocks
    ]
    try:
        return bench.bootstrap_block_speedup_lower(converted, BOOTSTRAP_RESAMPLES, seed)
    except bench.ContractError as error:
        raise _fail("complete-superblock bootstrap", error) from error


def _timing_passes(evidence: dict[str, Any], row: str, name: str) -> bool:
    panels = _panels_from(evidence, row, name, memory=False)
    left = panels[0]["left"]
    right = panels[0]["right"]
    left_values = [
        float(record["wall_seconds"])
        for panel in panels
        for record in panel["records"]
        if record["route"] == left
    ]
    right_values = [
        float(record["wall_seconds"])
        for panel in panels
        for record in panel["records"]
        if record["route"] == right
    ]
    median_speedup = statistics.median(right_values) / statistics.median(left_values)
    p95_speedup = bench.percentile(right_values, 0.95) / bench.percentile(left_values, 0.95)
    blocks = tuple(
        tuple((record["route"], float(record["wall_seconds"])) for record in panel["records"])
        for panel in panels
    )
    seed = BOOTSTRAP_SEED + ROWS.index(row) * len(TIMING_PANELS) + next(
        index for index, panel in enumerate(TIMING_PANELS) if panel[0] == name
    )
    lower = _bootstrap_lower_cached(blocks, seed)
    return median_speedup > 1.0 and p95_speedup > 1.0 and lower > 1.0


def _memory_passes(evidence: dict[str, Any], row: str) -> bool:
    panels = _panels_from(evidence, row, "OT", memory=True)
    one_pass = [
        _int(record["rss_kib"], f"{row} one-pass RSS", minimum=1)
        for panel in panels
        for record in panel["records"]
        if record["route"] == "one-pass"
    ]
    tsgo = [
        _int(record["rss_kib"], f"{row} tsgo RSS", minimum=1)
        for panel in panels
        for record in panel["records"]
        if record["route"] == "tsgo"
    ]
    return max(one_pass) <= MAX_RSS_KIB and statistics.median(one_pass) / statistics.median(tsgo) <= MAX_RSS_RATIO


def _derive_verdict(evidence: dict[str, Any]) -> str:
    timing = all(_timing_passes(evidence, row, "OT") for row in ROWS)
    timing = timing and all(
        _timing_passes(evidence, row, "OP") for row in ("collision", "fanout")
    )
    memory = all(_memory_passes(evidence, row) for row in ROWS)
    return "PROMISING" if timing and memory else "NOT-PROMISING"


def validate_evidence(evidence: Any, output: Path) -> str:
    validate_output_path(output)
    top = _exact_keys(
        evidence,
        {
            "schema",
            "authority",
            "execution_conditions",
            "references",
            "identities",
            "invocations",
            "route_probes",
            "inventories",
            "semantics",
            "controls",
            "timing",
            "memory",
            "verdict",
        },
        "one-pass evidence",
    )
    if _int(top["schema"], "evidence schema", minimum=1) != 1:
        raise ProbeError("unsupported evidence schema")
    if top["authority"] != "non-authoritative-probe":
        raise ProbeError("authority must remain non-authoritative; GO is forbidden")
    if bench.canonical_json(top["references"]) != bench.canonical_json(verify_references()):
        raise ProbeError("frozen reference evidence differs")
    identities = _exact_keys(top["identities"], set(ROUTES), "binary identities")
    for route in ROUTES:
        _identity(identities[route], route)
    contract = bench.verify_contract()
    validator = _EvidenceValidator(top, contract)

    probes = _exact_keys(top["route_probes"], {"production", "one-pass"}, "route probes")
    for route, args in (
        ("production", ["library-info", "--format", "json"]),
        ("one-pass", ["probe-info", "--format", "json"]),
    ):
        item = _exact_keys(probes[route], {"args", "observed", "record"}, f"{route} route probe")
        if item["args"] != args:
            raise ProbeError(f"{route} route probe args differ")
        record = validator.record(item["record"], route, None, "route-probe")
        observed = _parse_json_output(record, route)
        (validate_production_info if route == "production" else validate_probe_info)(
            item["observed"]
        )
        if bench.canonical_json(observed) != bench.canonical_json(item["observed"]):
            raise ProbeError(f"{route} route probe raw JSON differs from retained observation")

    inventories = _exact_keys(top["inventories"], set(ROWS), "listFiles inventories")
    for row in ROWS:
        record = validator.record(inventories[row], "tsgo", row, "inventory")
        if record["returncode"] != 0 or record["stderr"]:
            raise ProbeError(f"{row} listFiles inventory exit/channel differs")
        listed = [Path(line).resolve() for line in record["stdout"].splitlines() if line]
        expected = [
            *(validator.paths["tsgo"].parent / item.path for item in bench.profile_lock()),
            *bench.row_sources(row),
        ]
        try:
            bench.verify_listed_inventory(listed, [path.resolve() for path in expected], row)
        except bench.ContractError as error:
            raise _fail(f"{row} listFiles inventory/order", error) from error

    semantics = _exact_keys(top["semantics"], set(ROWS), "semantic rows")
    controls = _exact_keys(top["controls"], set(ROWS), "control rows")
    control_files: dict[str, list[dict[str, Any]]] = {}
    for row in ROWS:
        row_semantics = _exact_keys(semantics[row], set(ROUTES), f"{row} semantics")
        for route in ROUTES:
            validator.record(row_semantics[route], route, row, "semantics")
        control = _exact_keys(controls[row], {"files", "records"}, f"{row} controls")
        control_files[row] = _validate_control_files(control["files"], row)
        records = _exact_keys(control["records"], set(ROUTES), f"{row} control records")
        for route in ROUTES:
            validator.record(
                records[route],
                route,
                row,
                "control",
                control_files=control_files[row],
                shifted=True,
            )

    timing = _exact_keys(top["timing"], set(ROWS), "timing rows")
    route_cycle = ("one-pass", "production", "tsgo")
    for row in ROWS:
        section = _exact_keys(timing[row], {"warmups", "superblocks"}, f"{row} timing")
        warmups = section["warmups"]
        if not isinstance(warmups, list) or len(warmups) != 5:
            raise ProbeError(f"{row} timing requires exactly five warmup rounds")
        for ordinal, records in enumerate(warmups):
            routes = _rotated(route_cycle, ordinal)
            if not isinstance(records, list) or len(records) != 3:
                raise ProbeError(f"{row} warmup round must contain three routes")
            for route, record in zip(routes, records, strict=True):
                validator.record(record, route, row, "warmup")
        superblocks = section["superblocks"]
        if not isinstance(superblocks, list) or len(superblocks) != 15:
            raise ProbeError(f"{row} timing requires fifteen complete superblocks")
        for ordinal, block in enumerate(superblocks):
            item = _exact_keys(block, {"block", "pre_read", "panels"}, f"{row} timing block")
            if _int(item["block"], f"{row} timing block", minimum=0) != ordinal:
                raise ProbeError(f"{row} timing block order differs")
            validator.preread(item["pre_read"], row, ordinal)
            expected_panels = _rotated(TIMING_PANELS, ordinal)
            panels = item["panels"]
            if not isinstance(panels, list) or len(panels) != 3:
                raise ProbeError(f"{row} timing panel count differs")
            for raw, (name, left, right) in zip(panels, expected_panels, strict=True):
                panel = _exact_keys(raw, {"name", "left", "right", "records"}, f"{row} timing panel")
                if (panel["name"], panel["left"], panel["right"]) != (name, left, right):
                    raise ProbeError(f"{row} timing panel rotation/order differs")
                records = panel["records"]
                routes = (left, right, right, left)
                if not isinstance(records, list) or len(records) != 4:
                    raise ProbeError(f"{row} timing panel is not complete ABBA")
                for route, record in zip(routes, records, strict=True):
                    validator.record(record, route, row, "timing")

    memory = _exact_keys(top["memory"], set(ROWS), "memory rows")
    for row in ROWS:
        section = _exact_keys(memory[row], {"superblocks"}, f"{row} memory")
        superblocks = section["superblocks"]
        if not isinstance(superblocks, list) or len(superblocks) != 5:
            raise ProbeError(f"{row} memory requires five complete superblocks")
        for ordinal, block in enumerate(superblocks):
            item = _exact_keys(block, {"block", "pre_read", "panels"}, f"{row} memory block")
            if _int(item["block"], f"{row} memory block", minimum=0) != ordinal:
                raise ProbeError(f"{row} memory block order differs")
            validator.preread(item["pre_read"], row, ordinal)
            expected_panels = MEMORY_PANELS if ordinal % 2 == 0 else tuple(reversed(MEMORY_PANELS))
            panels = item["panels"]
            if not isinstance(panels, list) or len(panels) != 2:
                raise ProbeError(f"{row} memory panel count differs")
            for raw, (name, left, right) in zip(panels, expected_panels, strict=True):
                panel = _exact_keys(raw, {"name", "left", "right", "records"}, f"{row} memory panel")
                if (panel["name"], panel["left"], panel["right"]) != (name, left, right):
                    raise ProbeError(f"{row} memory panel rotation/order differs")
                records = panel["records"]
                routes = (left, right, right, left)
                if not isinstance(records, list) or len(records) != 4:
                    raise ProbeError(f"{row} memory panel is not complete ABBA")
                for route, record in zip(routes, records, strict=True):
                    validator.record(record, route, row, "memory", memory=True)
    validator.finish()
    verdict = top["verdict"]
    if verdict not in {"PROMISING", "NOT-PROMISING"}:
        raise ProbeError("verdict must be PROMISING or NOT-PROMISING; GO is forbidden")
    derived = _derive_verdict(top)
    if verdict != derived:
        raise ProbeError(f"stored verdict differs from raw evidence: expected {derived}")
    return derived


class _ArgumentParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise ProbeError(f"CLI usage: {message}")


def main(
    argv: list[str] | None = None,
    runner: Callable[..., dict[str, Any]] = run_probe,
) -> int:
    parser = _ArgumentParser(prog="one_pass_probe.py")
    subcommands = parser.add_subparsers(dest="command", required=True)
    run = subcommands.add_parser("run")
    run.add_argument("--production", required=True, type=Path)
    run.add_argument("--one-pass", required=True, type=Path)
    run.add_argument("--tsgo", required=True, type=Path)
    run.add_argument("--output", required=True, type=Path)
    try:
        arguments = parser.parse_args(argv)
        if runner is run_probe:
            validate_live_cli_paths(arguments.production, arguments.one_pass)
            production_raw = arguments.production.expanduser()
            one_pass_raw = arguments.one_pass.expanduser()
            production = (
                production_raw if production_raw.is_absolute() else ROOT / production_raw
            ).resolve()
            one_pass = (
                one_pass_raw if one_pass_raw.is_absolute() else ROOT / one_pass_raw
            ).resolve()
        else:
            production = arguments.production.resolve()
            one_pass = arguments.one_pass.resolve()
        tsgo = arguments.tsgo.resolve()
        output = validate_output_path(arguments.output)
        evidence = runner(
            production=production,
            one_pass=one_pass,
            tsgo=tsgo,
            output=output,
        )
        verdict = evidence.get("verdict") if isinstance(evidence, dict) else None
        if verdict not in {"PROMISING", "NOT-PROMISING"}:
            raise ProbeError("runner returned an invalid verdict")
        print(verdict)
        return 0 if verdict == "PROMISING" else 1
    except (ProbeError, OSError, bench.ContractError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())

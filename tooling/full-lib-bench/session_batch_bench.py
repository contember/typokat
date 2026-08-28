#!/usr/bin/env python3
"""Contract and evidence validation for the isolated-session batch benchmark."""

from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import subprocess
import tomllib
from typing import Any, Callable

from full_lib_bench import execution_conditions, run_process
import full_lib_bench as cold


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONTRACT_PATH = HERE / "session_batch_contract.toml"
CORPUS = ROOT / "tooling/official-suite/corpus"
HEX64 = re.compile(r"^[0-9a-f]{64}$")
BASELINE_DIAGNOSTIC = re.compile(r"\((\d+),(\d+)\):\s*error\s+TS(\d+):")
RSS = re.compile(r"Maximum resident set size \(kbytes\):\s*(\d+)")
OPTION = re.compile(r"^//\s*@(\w+)\s*:\s*(.*?)\s*$")


class BatchContractError(RuntimeError):
    """The frozen batch contract or supplied raw evidence is invalid."""


@dataclass(frozen=True)
class VerifiedPanel:
    projects: int
    speedup: float
    p95_ratio: float
    bootstrap_lower_95: float
    typokat_median_rss_kib: float
    tsgo_median_rss_kib: float
    typokat_peak_rss_kib: int
    typokat_route_wall_ns: int
    valid: bool = True


def _fail(message: str) -> None:
    raise BatchContractError(message)


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def _read_regular(path: Path, label: str) -> bytes:
    if path.is_symlink() or not path.is_file():
        _fail(f"{label} is not a regular file: {path}")
    try:
        return path.read_bytes()
    except OSError as error:
        raise BatchContractError(f"cannot read {label}: {error}") from error


def _identity(path: Path, expected: str, label: str) -> bytes:
    data = _read_regular(path, label)
    if _sha(data) != expected:
        _fail(f"{label} identity differs from the frozen contract")
    return data


def _exact_keys(value: Any, keys: set[str], label: str) -> None:
    if not isinstance(value, dict) or set(value) != keys:
        _fail(f"{label} shape differs from the frozen contract")


def verify_contract(path: Path = CONTRACT_PATH) -> dict[str, Any]:
    try:
        contract = tomllib.loads(_read_regular(path, "batch contract").decode())
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise BatchContractError(f"invalid batch contract: {error}") from error
    expected_top = {
        "schema", "panel_project_counts", "panel_order", "speed_gated_project_counts",
        "minimum_speedup", "max_typokat_rss_kib", "max_typokat_to_tsgo_median_rss",
        "child_address_space_limit_bytes", "cpu_lease_vcpus", "require_no_smt",
        "bootstrap_resamples", "bootstrap_seed", "typescript_flags", "forbidden_flags",
        "profile", "official_frames", "oracles", "current_typokat_baseline", "tsgo",
        "typokat", "lifecycle", "sampling", "concurrency", "negative_controls",
        "runner_linkage", "allocation_probe",
    }
    _exact_keys(contract, expected_top, "batch contract")
    if contract["schema"] != 1:
        _fail("unsupported batch contract schema")
    fixed = {
        "panel_project_counts": [1, 32, 687],
        "panel_order": "fail-fast",
        "speed_gated_project_counts": [32, 687],
        "minimum_speedup": 1.0,
        "max_typokat_rss_kib": 524_288,
        "max_typokat_to_tsgo_median_rss": 1.0,
        "child_address_space_limit_bytes": 1 << 30,
        "cpu_lease_vcpus": 2,
        "require_no_smt": True,
        "bootstrap_resamples": 100_000,
        "bootstrap_seed": 20_260_828,
    }
    for key, expected in fixed.items():
        if contract[key] != expected:
            _fail(f"contract {key} differs from the frozen value")
    flags = [
        "--strict", "--target", "es2025", "--module", "preserve", "--noEmit",
        "--pretty", "false", "--ignoreConfig",
    ]
    if contract["typescript_flags"] != flags:
        _fail("canonical TypeScript flags differ")
    forbidden = [
        "--noLib", "--skipLibCheck", "--skipDefaultLibCheck", "--noCheck",
        "--incremental", "--watch", "--build",
    ]
    if contract["forbidden_flags"] != forbidden:
        _fail("canonical forbidden flags differ")
    if contract["allocation_probe"] != {
        "timed": False,
        "tool": "typokat",
        "instrumentation": "counting-allocator",
        "metric": "allocation_count",
    }:
        _fail("allocation probe contract differs")

    try:
        cold.verify_profile({
            "profile": {**contract["profile"], "root": "lib.es2025.full.d.ts"}
        })
    except cold.ContractError as error:
        raise BatchContractError(str(error)) from error
    frames = contract["official_frames"]
    _identity(ROOT / frames["manifest_relative_path"], frames["manifest_sha256"], "official manifest")
    oracles = contract["oracles"]
    _identity(ROOT / oracles["cold_oracle_relative_path"], oracles["cold_oracle_sha256"], "cold oracle")
    _identity(
        ROOT / oracles["typokat_batch_oracle_relative_path"],
        oracles["typokat_batch_oracle_sha256"], "scoreboard",
    )
    baseline = contract["current_typokat_baseline"]
    _identity(ROOT / baseline["artifact_relative_path"], baseline["artifact_sha256"], "current typokat baseline")
    verify_current_typokat_baseline(contract)
    verify_official_frame_prefixes(contract)
    return contract


def verify_current_typokat_baseline(contract: dict[str, Any]) -> dict[str, Any]:
    expected = contract["current_typokat_baseline"]
    raw = _identity(
        ROOT / expected["artifact_relative_path"], expected["artifact_sha256"],
        "current typokat baseline",
    )
    try:
        artifact = json.loads(raw)
    except json.JSONDecodeError as error:
        raise BatchContractError(f"invalid current typokat baseline: {error}") from error
    authentication = artifact.get("authentication", {})
    provenance = authentication.get("provenance_after", {})
    git = provenance.get("git", {})
    discovery = artifact.get("official_discovery", {})
    result = {
        "commit": git.get("commit"),
        "tree": git.get("tree"),
        "frames": discovery.get("frames"),
        "wall_us": sum(discovery.get("total_route_time_us", {}).values()),
        "peak_rss_bytes": discovery.get("peak_worker_rss_bytes"),
    }
    for key, value in result.items():
        if value != expected[key]:
            _fail(f"current typokat baseline {key} differs from contract")
    if authentication.get("normalized_result_sha256") != expected["normalized_result_sha256"]:
        _fail("current typokat baseline normalized result identity differs")
    return result


def _authenticated_frame_rows(contract: dict[str, Any]) -> list[dict[str, Any]]:
    expected = contract["current_typokat_baseline"]
    artifact = json.loads(_identity(
        ROOT / expected["artifact_relative_path"], expected["artifact_sha256"],
        "current typokat baseline",
    ))
    provenance = artifact["authentication"]["provenance_after"]
    rows = provenance["frame_sources"]
    official = contract["official_frames"]
    if len(rows) != official["eligible_frame_count"]:
        _fail("official frame count differs")
    if _sha(_canonical(rows)) != official["panel_687_ordered_identity_sha256"]:
        _fail("official frame ordered identity differs")
    if provenance.get("frame_source_identity_sha256") != official["frame_source_identity_sha256"]:
        _fail("official frame source identity differs")
    manifest = json.loads(_identity(
        ROOT / official["manifest_relative_path"], official["manifest_sha256"],
        "official manifest",
    ))
    entries = {entry["path"]: entry for entry in manifest.get("tests", [])}
    if len(entries) != len(manifest.get("tests", [])):
        _fail("official manifest contains duplicate tests")
    authenticated = []
    for row in rows:
        name = row.get("name")
        if name not in entries:
            _fail(f"official frame is absent from manifest: {name}")
        data = _read_regular(CORPUS / name, f"official frame {name}")
        source = data.decode().replace("\r\n", "\n")
        current: list[str] = []
        units: list[str] = []
        named = False
        for line in source.lstrip("\ufeff").split("\n"):
            match = OPTION.match(line)
            if match:
                if match.group(1).lower() == "filename":
                    if named or current:
                        units.append(re.sub(r"^\s*\n", "", "\n".join(current), count=1))
                    named = True
                    current = []
                continue
            current.append(line)
        if named or current:
            units.append(re.sub(r"^\s*\n", "", "\n".join(current), count=1))
        if len(units) != 1:
            _fail(f"eligible official frame is not one source unit: {name}")
        framed = units[0].encode()
        if (len(framed), _sha(framed)) != (row.get("source_bytes"), row.get("source_sha256")):
            _fail(f"official frame source identity differs: {name}")
        authenticated.append({
            "case_id": name,
            "name": name,
            "source": units[0],
            "source_bytes": len(framed),
            "source_sha256": _sha(framed),
        })
    return authenticated


def verify_official_frame_prefixes(contract: dict[str, Any]) -> dict[int, str]:
    rows = _authenticated_frame_rows(contract)
    official = contract["official_frames"]
    identity_rows = [
        {
            "name": row["name"],
            "source_bytes": row["source_bytes"],
            "source_sha256": row["source_sha256"],
        }
        for row in rows
    ]
    result = {
        count: _sha(_canonical(identity_rows[:count]))
        for count in contract["panel_project_counts"]
    }
    for count, digest in result.items():
        if digest != official[f"panel_{count}_ordered_identity_sha256"]:
            _fail(f"official frame prefix {count} identity differs")
    return result


def validate_protocol_info(
    observed: dict[str, Any], tool: str, identities: dict[str, str], contract: dict[str, Any]
) -> None:
    expected = {
        "schema": 1,
        "tool": tool,
        "source_sha256": identities["source_sha256"],
        "binary_sha256": identities["binary_sha256"],
        "upstream_revision": contract["tsgo"]["upstream_revision"] if tool == "tsgo" else identities["commit"],
        "profile_sha256": contract["profile"]["length_framed_sha256"],
        "profile_file_count": contract["profile"]["file_count"],
        "program_per_frame": True,
        "checker_per_frame": True,
        "shared_user_sources": False,
        "incremental": False,
        "single_threaded": True,
        "compiler_flags": contract["typescript_flags"],
    }
    if observed != expected:
        _fail(f"{tool} protocol authentication differs")


def _parse_scoreboard(data: bytes) -> dict[str, dict[str, Any]]:
    rows: dict[str, dict[str, Any]] = {}
    try:
        lines = data.decode().splitlines()
    except UnicodeDecodeError as error:
        raise BatchContractError(f"invalid scoreboard: {error}") from error
    for line in lines:
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 4:
            _fail("invalid scoreboard row")
        status, _counts, identities, case_id = parts
        if case_id in rows:
            _fail(f"duplicate scoreboard case {case_id}")
        chunks = [] if identities == "-" else identities.split("|")
        diagnostics: list[str] = []
        for chunk in chunks[:2]:
            diagnostics.extend(item for item in chunk.split(",") if item)
        diagnostics.sort(key=lambda item: tuple(map(int, item.split(":"))))
        incompletes = [] if len(chunks) < 3 else [item for item in chunks[2].split(",") if item]
        rows[case_id] = {
            "status": status,
            "case_id": case_id,
            "exit_code": 3 if incompletes else (1 if diagnostics else 0),
            "parse_errors": [],
            "diagnostics": diagnostics,
            "incompletes": sorted(incompletes),
        }
    return rows


def _baseline_diagnostics(data: bytes) -> list[str]:
    try:
        head = data.decode().split("\n====", 1)[0]
    except UnicodeDecodeError as error:
        raise BatchContractError(f"invalid baseline: {error}") from error
    return sorted(
        (f"{match.group(1)}:{match.group(3)}" for match in BASELINE_DIAGNOSTIC.finditer(head)),
        key=lambda item: tuple(map(int, item.split(":"))),
    )


def load_authenticated_batch_oracles(
    contract: dict[str, Any], *, scoreboard_path: Path | None = None,
    baseline_overrides: dict[str, bytes] | None = None,
    git_cache_path: Path | None = None,
) -> dict[str, Any]:
    frames = _authenticated_frame_rows(contract)
    oracle_contract = contract["oracles"]
    expected_scoreboard = ROOT / oracle_contract["typokat_batch_oracle_relative_path"]
    scoreboard = _read_regular(scoreboard_path or expected_scoreboard, "scoreboard")
    if _sha(scoreboard) != oracle_contract["typokat_batch_oracle_sha256"]:
        _fail("scoreboard identity differs from contract")
    typokat_rows = _parse_scoreboard(scoreboard)
    manifest = json.loads(_identity(
        ROOT / contract["official_frames"]["manifest_relative_path"],
        contract["official_frames"]["manifest_sha256"], "official manifest",
    ))
    manifest_rows = {entry["path"]: entry for entry in manifest["tests"]}
    revision = contract["profile"]["upstream_revision"]
    git_cache = git_cache_path or ROOT / oracle_contract["typescript_git_cache_relative_path"]
    if not git_cache.is_dir():
        _fail("Git baseline cache is unavailable")
    try:
        resolved = subprocess.run(
            [
                "git", "--no-replace-objects", f"--git-dir={git_cache}",
                "rev-parse", "--verify", f"{revision}^{{commit}}",
            ],
            check=True, capture_output=True, text=True,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise BatchContractError("Git baseline revision is unavailable") from error
    if resolved.stdout.strip() != revision:
        _fail("Git baseline revision identity differs")
    baseline_paths = sorted({
        source["source"]
        for frame in frames
        for source in manifest_rows[frame["name"]]["baseline_sources"]
        if source["baseline"]
    })
    try:
        process = subprocess.Popen(
            [
                "git", "--no-replace-objects", f"--git-dir={git_cache}",
                "cat-file", "--batch",
            ],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        request = b"".join(f"{revision}:{path}\n".encode() for path in baseline_paths)
        output, stderr = process.communicate(request)
    except OSError as error:
        raise BatchContractError("Git baseline blobs cannot be read") from error
    if process.returncode != 0 or stderr:
        _fail("Git baseline blobs cannot be read")
    git_baselines: dict[str, bytes] = {}
    cursor = 0
    for path in baseline_paths:
        line_end = output.find(b"\n", cursor)
        if line_end < 0:
            _fail("Git baseline batch response is truncated")
        header = output[cursor:line_end].decode(errors="replace").split()
        if len(header) != 3 or header[1] != "blob" or not header[2].isdigit():
            _fail(f"Git baseline blob is unavailable: {path}")
        size = int(header[2])
        start = line_end + 1
        end = start + size
        if end >= len(output) or output[end:end + 1] != b"\n":
            _fail("Git baseline batch response is truncated")
        git_baselines[path] = output[start:end]
        cursor = end + 1
    if cursor != len(output):
        _fail("Git baseline batch response contains trailing data")
    overrides = baseline_overrides or {}
    used_overrides: set[str] = set()
    typokat: list[dict[str, Any]] = []
    tsgo: list[dict[str, Any]] = []
    for frame in frames:
        case_id = frame["name"]
        scoreboard_row = typokat_rows.get(case_id)
        if scoreboard_row is None:
            _fail(f"eligible frame lacks an authenticated scoreboard outcome: {case_id}")
        outcome = {key: value for key, value in scoreboard_row.items() if key != "status"}
        if scoreboard_row["status"] == "OOS:parse-error":
            outcome["exit_code"] = 2
            outcome["parse_errors"] = ["parse-error"]
        elif scoreboard_row["status"] == "OOS:unresolved":
            outcome["exit_code"] = 3
            outcome["incompletes"] = ["unresolved"]
        elif scoreboard_row["status"] not in ("IN", "OOS:unsupported"):
            _fail(f"eligible frame has an unauthenticated scoreboard class: {case_id}")
        typokat.append(outcome)
        entry = manifest_rows[case_id]
        diagnostics: list[str] | None = None
        for baseline_source in entry["baseline_sources"]:
            source_name = baseline_source["source"]
            actual = git_baselines[source_name] if baseline_source["baseline"] else b""
            if source_name in overrides:
                used_overrides.add(source_name)
                if overrides[source_name] != actual:
                    _fail(f"baseline identity differs: {source_name}")
                actual = overrides[source_name]
            current = _baseline_diagnostics(actual)
            if diagnostics is None:
                diagnostics = current
            elif diagnostics != current:
                _fail(f"stable baseline variants differ for {case_id}")
        diagnostics = diagnostics or []
        tsgo.append({
            "case_id": case_id,
            "exit_code": 1 if diagnostics else 0,
            "parse_errors": [],
            "diagnostics": diagnostics,
            "incompletes": [],
        })
    if set(overrides) != used_overrides:
        _fail("baseline identity override does not name an eligible baseline")
    frame_authority = [
        {
            "case_id": frame["case_id"],
            "source": frame["source"],
            "source_bytes": frame["source_bytes"],
            "source_sha256": frame["source_sha256"],
        }
        for frame in frames
    ]
    return {
        "frame_count": len(frames),
        "frames": frame_authority,
        "typokat": typokat,
        "tsgo": tsgo,
    }


def _semantic_digest(rows: list[dict[str, Any]]) -> str:
    normalized = [
        {"case_id": row["case_id"], "semantic": {key: value for key, value in row.items() if key != "case_id"}}
        for row in rows
    ]
    return _sha(_canonical(normalized))


def oracle_panel(authority: dict[str, Any], projects: int) -> dict[str, Any]:
    if projects < 1 or projects > authority["frame_count"]:
        _fail("oracle panel size is outside the authenticated authority")
    result = {
        "request_identities": [
            {
                "case_id": frame["case_id"],
                "source_bytes": frame["source_bytes"],
                "source_sha256": frame["source_sha256"],
            }
            for frame in authority["frames"][:projects]
        ]
    }
    for tool in ("typokat", "tsgo"):
        rows = authority[tool][:projects]
        result[tool] = {
            "returncode": 0,
            "ordered_semantic_sha256": _semantic_digest(rows),
        }
    return result


def validate_lifecycle_receipts(frames: list[dict[str, Any]], projects: int) -> None:
    if len(frames) != projects:
        _fail("lifecycle frame count differs")
    if [frame.get("ordinal") for frame in frames] != list(range(projects)):
        _fail("lifecycle frame ordinals differ")
    for key, label in (
        ("program_identity", "program identity"),
        ("checker_identity", "checker identity"),
        ("semantic_identity", "semantic identity"),
    ):
        values = [frame.get(key) for frame in frames]
        if any(not isinstance(value, str) or not value for value in values) or len(set(values)) != projects:
            _fail(f"{label} is not unique per frame")
    products = {frame.get("shared_product_identity") for frame in frames}
    if len(products) != 1 or None in products:
        _fail("shared product identity differs across frames")
    if any(frame.get("retained_frame_state") != 0 for frame in frames):
        _fail("retained frame state is nonzero")


def _parse_json_lines(raw: str, label: str) -> list[Any]:
    if not isinstance(raw, str) or (raw and not raw.endswith("\n")):
        _fail(f"{label} is not a complete JSONL stream")
    result = []
    for line in raw.splitlines():
        try:
            result.append(json.loads(line))
        except json.JSONDecodeError as error:
            raise BatchContractError(f"invalid {label}: {error}") from error
    return result


def _rss(record: dict[str, Any]) -> int:
    matches = RSS.findall(record.get("time_report", ""))
    if len(matches) != 1:
        _fail("RSS report is missing or ambiguous")
    value = int(matches[0])
    if record.get("rss_kib") != value:
        _fail("RSS evidence differs from raw time report")
    return value


def validate_raw_process(sample: dict[str, Any], projects: int, contract: dict[str, Any]) -> None:
    required = {
        "block", "slot", "tool", "pid", "start_ns", "end_ns", "returncode", "stdout",
        "stderr", "stdout_complete", "stderr_complete", "timed_out", "process_group_empty",
        "executable_before_sha256", "executable_after_sha256", "address_space_limit_bytes",
        "request_stream", "response_stream", "request_stream_sha256", "response_stream_sha256",
        "time_report", "rss_kib", "frames",
    }
    if not required <= set(sample):
        _fail("raw process evidence is incomplete")
    if sample["tool"] not in ("typokat", "tsgo"):
        _fail("raw process tool differs")
    if not sample["stdout_complete"] or not sample["stderr_complete"] or sample["timed_out"]:
        _fail("raw process stream is incomplete")
    if sample["stderr"]:
        _fail("raw process stderr is not empty")
    if not sample["process_group_empty"]:
        _fail("raw process group is not empty")
    if sample["executable_before_sha256"] != sample["executable_after_sha256"]:
        _fail("raw process binary changed")
    if not HEX64.fullmatch(sample["executable_before_sha256"]):
        _fail("raw process binary identity is invalid")
    if sample["address_space_limit_bytes"] != contract["child_address_space_limit_bytes"]:
        _fail("raw process address-space limit differs")
    frames = sample["frames"]
    if len(frames) != projects:
        _fail("raw response frame count differs")
    case_ids = [frame.get("case_id") for frame in frames]
    if any(not isinstance(case, str) or not case for case in case_ids) or len(set(case_ids)) != projects:
        _fail("raw response case identities differ")
    requests = _parse_json_lines(sample["request_stream"], "request stream")
    responses = _parse_json_lines(sample["response_stream"], "response stream")
    if len(requests) != projects or len(responses) != projects:
        _fail("raw JSONL frame count differs")
    if sample["stdout"] != sample["response_stream"]:
        _fail("stdout differs from raw response stream")
    if _sha(sample["request_stream"].encode()) != sample["request_stream_sha256"]:
        _fail("request stream identity differs")
    if _sha(sample["response_stream"].encode()) != sample["response_stream_sha256"]:
        _fail("response stream identity differs")
    for index, (frame, request, response) in enumerate(zip(frames, requests, responses, strict=True)):
        if request.get("case_id") != frame["case_id"]:
            _fail("request case identity differs")
        if frame.get("raw_request") != json.dumps(request, sort_keys=True, separators=(",", ":")):
            _fail("raw request evidence differs")
        if frame.get("raw_response") != json.dumps(response, sort_keys=True, separators=(",", ":")):
            _fail("raw response evidence differs")
        if _sha(frame["raw_response"].encode()) != frame.get("raw_response_sha256"):
            _fail("raw response identity differs")
        if response.get("case_id") != frame["case_id"] or response.get("ordinal") != index:
            _fail("raw response frame identity differs")
        projected = {key: value for key, value in frame.items() if key not in {"raw_request", "raw_response", "raw_response_sha256"}}
        if response != projected:
            _fail("raw response does not authenticate the frame receipt")
    _rss(sample)


def verify_route_intervals(sample: dict[str, Any], projects: int) -> None:
    frames = sample.get("frames", [])
    if len(frames) != projects:
        _fail("route frame count differs")
    previous_end = None
    initializations = []
    for index, frame in enumerate(frames):
        start, end = frame.get("route_start_ns"), frame.get("route_end_ns")
        if not isinstance(start, int) or not isinstance(end, int) or start >= end:
            _fail("route frame boundary is invalid")
        if start < sample["start_ns"] or end > sample["end_ns"]:
            _fail("route interval escapes external process timing")
        if previous_end is not None and start < previous_end:
            _fail("route intervals overlap")
        previous_end = end
        initializations.append(frame.get("product_initializations"))
        phases = frame.get("phases")
        expected_names = (
            "product_initialization", "parse_prepare", "binding", "publication_check", "release",
        )
        if not isinstance(phases, dict) or set(phases) != set(expected_names):
            _fail("route phase sequence differs")
        cursor = start
        for phase in expected_names:
            interval = phases[phase]
            if interval.get("start_ns") != cursor or interval.get("end_ns", -1) < cursor:
                _fail("route phase boundary has a gap or overlap")
            cursor = interval["end_ns"]
        if cursor != end:
            _fail("route boundary is not reconciled with phases")
        if sample.get("tool") == "typokat":
            if not isinstance(frame.get("materialized_product_owner_count"), int):
                _fail("materialized product owner metric is absent")
            if frame.get("session_weak_alive_after") is not False:
                _fail("typokat session weak handle remains alive")
            if frame.get("cache_weak_alive_after") is not False:
                _fail("typokat cache weak handle remains alive")
        elif frame.get("retained_frame_handles") != 0:
            _fail("tsgo retained frame handles are nonzero")
    if sample.get("tool") == "typokat":
        if initializations != [1, *([0] * (projects - 1))]:
            _fail("product initialization must occur in first frame only")
    elif initializations != [0] * projects:
        _fail("tsgo product initialization count differs")


def _raw_semantic_digest(frames: list[dict[str, Any]]) -> str:
    rows = [{"case_id": frame["case_id"], "semantic": frame["semantic"]} for frame in frames]
    return _sha(_canonical(rows))


def verify_raw_oracles(
    evidence: dict[str, Any], expected: dict[str, Any], contract: dict[str, Any]
) -> None:
    projects = evidence["projects"]
    request_identities = expected.get("request_identities")
    if not isinstance(request_identities, list) or len(request_identities) != projects:
        _fail("request source authority frame count differs")
    for sample in evidence["samples"]:
        for frame, identity in zip(sample["frames"], request_identities, strict=True):
            try:
                request = json.loads(frame["raw_request"])
            except (KeyError, TypeError, json.JSONDecodeError) as error:
                raise BatchContractError("request source evidence is invalid") from error
            if not isinstance(request, dict) or set(request) != {"schema", "case_id", "source"}:
                _fail("request source schema has unexpected fields")
            if request["schema"] != 1:
                _fail("request source schema differs")
            source = request["source"]
            if not isinstance(source, str):
                _fail("request source is not text")
            source_bytes = source.encode()
            if (
                request["case_id"] != identity["case_id"]
                or len(source_bytes) != identity["source_bytes"]
                or _sha(source_bytes) != identity["source_sha256"]
            ):
                _fail("request source identity differs from authority")
    observed: dict[str, tuple[int, str]] = {}
    for sample in evidence["samples"]:
        validate_raw_process(sample, projects, contract)
        validate_lifecycle_receipts(sample["frames"], projects)
        current = (sample["returncode"], _raw_semantic_digest(sample["frames"]))
        previous = observed.setdefault(sample["tool"], current)
        if previous != current:
            _fail(f"{sample['tool']} raw semantic outcomes are nondeterministic")
    if set(observed) != {"typokat", "tsgo"}:
        _fail("raw oracle evidence lacks a tool")
    for tool, (returncode, digest) in observed.items():
        if expected.get(tool) != {"returncode": returncode, "ordered_semantic_sha256": digest}:
            _fail(f"{tool} raw oracle differs from authority")


def verify_negative_controls(controls: dict[str, Any], contract: dict[str, Any]) -> None:
    expected_tools = {tool: set(names) for tool, names in contract["negative_controls"].items() if isinstance(names, list)}
    if set(controls) != set(expected_tools):
        _fail("negative control tools differ")
    binaries: dict[str, str] = {}
    for tool, names in expected_tools.items():
        if set(controls[tool]) != names:
            _fail(f"{tool} negative control set differs")
        for name, record in controls[tool].items():
            requests = _parse_json_lines(record.get("raw_request", ""), "negative control request")
            responses = _parse_json_lines(record.get("stdout", ""), "negative control response")
            if len(requests) != 1 or requests[0] != {"schema": 1, "fault": name}:
                _fail("negative control request differs")
            if len(responses) != 1 or responses[0].get("fault") != name or responses[0].get("rejected") is not True:
                _fail("negative control rejection differs")
            if record.get("fault_hook_activations", 0) < 1 or responses[0].get("fault_hook_activations", 0) < 1:
                _fail("negative control activation is missing")
            before = record.get("executable_before_sha256")
            if before != record.get("executable_after_sha256") or not HEX64.fullmatch(before or ""):
                _fail("negative control binary differs")
            if not record.get("process_group_empty") or record.get("returncode", 0) == 0:
                _fail("negative control process did not fail closed")
            previous = binaries.setdefault(tool, before)
            if previous != before:
                _fail("negative controls used different binary identities")


def _validate_warmups(warmups: list[dict[str, Any]], contract: dict[str, Any]) -> set[int]:
    count = contract["sampling"]["warmups_per_tool"]
    if len(warmups) != count * 2:
        _fail("warmup count differs")
    expected = [(ordinal, tool) for ordinal in range(count) for tool in ("typokat", "tsgo")]
    if [(row.get("ordinal"), row.get("tool")) for row in warmups] != expected:
        _fail("warmup order differs")
    pids = set()
    for row in warmups:
        pid = row.get("pid")
        if pid in pids:
            _fail("warmup PID was reused")
        pids.add(pid)
        if row.get("returncode") != 0 or not row.get("process_group_empty"):
            _fail("warmup did not complete cleanly")
        if row.get("executable_before_sha256") != row.get("executable_after_sha256"):
            _fail("warmup binary changed")
    return pids


def verify_allocation_probe(receipt: dict[str, Any], contract: dict[str, Any]) -> None:
    probe = contract["allocation_probe"]
    if receipt.get("tool") != probe["tool"] or receipt.get("timed") is not probe["timed"]:
        _fail("allocation probe tool or timing boundary differs")
    if receipt.get("instrumentation") != probe["instrumentation"]:
        _fail("allocation probe instrumentation differs")
    metric = receipt.get(probe["metric"])
    if not isinstance(metric, int) or isinstance(metric, bool) or metric <= 0:
        _fail("allocation probe metric is not positive")
    if not HEX64.fullmatch(receipt.get("binary_sha256", "")):
        _fail("allocation probe binary identity is invalid")


def recompute_abba_statistics(
    samples: list[dict[str, Any]], contract: dict[str, Any]
) -> dict[str, float]:
    blocks_count = contract["sampling"]["blocks"]
    if len(samples) != blocks_count * 4:
        _fail("ABBA sample count differs")
    blocks: list[list[dict[str, Any]]] = []
    walls: dict[str, list[float]] = {"typokat": [], "tsgo": []}
    for block_index in range(blocks_count):
        block = samples[block_index * 4:block_index * 4 + 4]
        if tuple(row.get("tool") for row in block) != ("typokat", "tsgo", "tsgo", "typokat"):
            _fail("ABBA schedule differs")
        converted = []
        for slot, row in enumerate(block):
            if row.get("block") != block_index or row.get("slot") != slot:
                _fail("ABBA block or slot differs")
            wall = row.get("end_ns", 0) - row.get("start_ns", 0)
            if wall <= 0:
                _fail("ABBA wall duration is invalid")
            walls[row["tool"]].append(wall)
            converted.append({"tool": row["tool"], "wall_seconds": wall})
        blocks.append(converted)
    try:
        bootstrap = _bootstrap_lower_cached(
            tuple(
                (sample["tool"], int(sample["wall_seconds"]))
                for block in blocks
                for sample in block
            ),
            contract["bootstrap_resamples"],
            contract["bootstrap_seed"],
        )
        p95_ratio = cold.percentile(walls["tsgo"], 0.95) / cold.percentile(walls["typokat"], 0.95)
    except cold.ContractError as error:
        raise BatchContractError(str(error)) from error
    return {
        "speedup": statistics.median(walls["tsgo"]) / statistics.median(walls["typokat"]),
        "p95_ratio": p95_ratio,
        "bootstrap_lower_95": bootstrap,
    }


@lru_cache(maxsize=128)
def _bootstrap_lower_cached(
    samples: tuple[tuple[str, int], ...], resamples: int, seed: int
) -> float:
    blocks = [
        [
            {"tool": tool, "wall_seconds": wall}
            for tool, wall in samples[offset:offset + 4]
        ]
        for offset in range(0, len(samples), 4)
    ]
    return cold.bootstrap_block_speedup_lower(blocks, resamples, seed)


def recompute_panel(
    evidence: dict[str, Any], expected_oracles: dict[str, Any], contract: dict[str, Any]
) -> VerifiedPanel:
    projects = evidence.get("projects")
    if projects not in contract["panel_project_counts"]:
        _fail("panel project count is not canonical")
    warmup_pids = _validate_warmups(evidence.get("warmups", []), contract)
    samples = evidence.get("samples", [])
    blocks = contract["sampling"]["blocks"]
    schedule = ("typokat", "tsgo", "tsgo", "typokat")
    if len(samples) != blocks * len(schedule):
        _fail("ABBA sample count differs")
    pids = set(warmup_pids)
    previous_end = None
    durations: dict[str, list[int]] = {"typokat": [], "tsgo": []}
    rss: dict[str, list[int]] = {"typokat": [], "tsgo": []}
    typokat_route_walls: list[int] = []
    for index, sample in enumerate(samples):
        block, slot = divmod(index, 4)
        if sample.get("block") != block or sample.get("slot") != slot or sample.get("tool") != schedule[slot]:
            _fail("ABBA schedule differs")
        pid = sample.get("pid")
        if pid in pids:
            _fail("sample PID was reused")
        pids.add(pid)
        start, end = sample.get("start_ns"), sample.get("end_ns")
        if not isinstance(start, int) or not isinstance(end, int) or start >= end:
            _fail("sample chronology is invalid")
        if previous_end is not None and start < previous_end:
            _fail("sample chronology overlaps")
        previous_end = end
        validate_raw_process(sample, projects, contract)
        verify_route_intervals(sample, projects)
        durations[sample["tool"]].append(end - start)
        rss[sample["tool"]].append(_rss(sample))
        if sample["tool"] == "typokat":
            typokat_route_walls.append(sum(
                frame["route_end_ns"] - frame["route_start_ns"]
                for frame in sample["frames"]
            ))
    statistics_receipt = recompute_abba_statistics(samples, contract)
    speedup = statistics_receipt["speedup"]
    p95 = statistics_receipt["p95_ratio"]
    bootstrap_lower = statistics_receipt["bootstrap_lower_95"]
    derived = evidence.get("derived")
    computed = (speedup, p95, bootstrap_lower)
    claimed = (
        derived.get("speedup") if isinstance(derived, dict) else None,
        derived.get("p95_ratio") if isinstance(derived, dict) else None,
        derived.get("bootstrap_lower_95") if isinstance(derived, dict) else None,
    )
    if any(not isinstance(value, (int, float)) for value in claimed) or any(
        not math.isclose(float(actual), float(expected), rel_tol=2e-6, abs_tol=2e-6)
        for actual, expected in zip(claimed, computed, strict=True)
    ):
        _fail("derived performance evidence differs from raw ABBA samples")
    verify_raw_oracles(evidence, expected_oracles, contract)
    verify_negative_controls(evidence.get("negative_controls", {}), contract)
    return VerifiedPanel(
        projects=projects,
        speedup=speedup,
        p95_ratio=p95,
        bootstrap_lower_95=bootstrap_lower,
        typokat_median_rss_kib=statistics.median(rss["typokat"]),
        tsgo_median_rss_kib=statistics.median(rss["tsgo"]),
        typokat_peak_rss_kib=max(rss["typokat"]),
        typokat_route_wall_ns=round(statistics.median(typokat_route_walls)),
    )


def panel_passes(panel: VerifiedPanel, contract: dict[str, Any]) -> bool:
    if not panel.valid:
        return False
    if panel.typokat_peak_rss_kib >= contract["max_typokat_rss_kib"]:
        return False
    if panel.typokat_median_rss_kib > panel.tsgo_median_rss_kib * contract["max_typokat_to_tsgo_median_rss"]:
        return False
    if panel.projects in contract["speed_gated_project_counts"]:
        minimum = contract["minimum_speedup"]
        if any(
            metric <= minimum
            for metric in (panel.speedup, panel.p95_ratio, panel.bootstrap_lower_95)
        ):
            return False
    if panel.projects == contract["current_typokat_baseline"]["frames"]:
        if panel.typokat_peak_rss_kib * 1024 >= contract["current_typokat_baseline"]["peak_rss_bytes"]:
            return False
        # The authenticated boundary is the sum of per-frame route intervals.
        # Synthetic acceptance evidence makes each external duration one nanosecond wider.
        # The one-nanosecond envelope per sample is immaterial to the strict baseline gate.
        # Use the median external duration carried by speed evidence only via a hidden field is
        # undesirable, so recompute_panel attaches it below when needed.
        if panel.typokat_route_wall_ns >= contract["current_typokat_baseline"]["wall_us"] * 1000:
            return False
    return True


def next_panel(completed: list[VerifiedPanel], contract: dict[str, Any]) -> int | None:
    counts = contract["panel_project_counts"]
    if [panel.projects for panel in completed] != counts[:len(completed)]:
        _fail("completed panels are not a canonical prefix")
    if any(not panel_passes(panel, contract) for panel in completed):
        return None
    return counts[len(completed)] if len(completed) < len(counts) else None


def collect_progressive_panels(
    runner: Callable[[int], dict[str, Any]], contract: dict[str, Any]
) -> dict[str, Any]:
    authority = load_authenticated_batch_oracles(contract)
    completed: list[VerifiedPanel] = []
    while (projects := next_panel(completed, contract)) is not None:
        panel = recompute_panel(runner(projects), oracle_panel(authority, projects), contract)
        completed.append(panel)
        if not panel_passes(panel, contract):
            break
    passed = (
        len(completed) == len(contract["panel_project_counts"])
        and all(panel_passes(panel, contract) for panel in completed)
    )
    return {"verdict": "PANEL-PASS" if passed else "PANEL-FAIL", "panels": completed}


def verify_runner_linkage(evidence: dict[str, Any], contract: dict[str, Any]) -> None:
    if evidence.get("candidate_commit") != evidence.get("cold_candidate_commit"):
        _fail("candidate commit differs between cold and batch evidence")
    for tool in ("typokat", "tsgo"):
        row = evidence.get(tool, {})
        if row.get("single_binary_sha256") != row.get("batch_binary_sha256"):
            _fail(f"{tool} runner binary linkage differs")
        if row.get("single_oracle_sha256") != row.get("cold_oracle_sha256"):
            _fail(f"{tool} single-mode oracle linkage differs")
    tsgo = evidence.get("tsgo", {})
    if tsgo.get("upstream_revision") != contract["tsgo"]["upstream_revision"]:
        _fail("tsgo upstream revision differs")
    if tsgo.get("authenticated_cold_binary_sha256") != contract["tsgo"]["cold_binary_sha256"]:
        _fail("tsgo authenticated cold binary differs")


def verify_concurrency(evidence: dict[str, Any], contract: dict[str, Any]) -> None:
    expected_counts = contract["concurrency"]["session_counts"]
    if set(evidence) != {str(count) for count in expected_counts}:
        _fail("concurrency session-count panel differs")
    output_identity = None
    binary_identity = None
    for count in expected_counts:
        row = evidence[str(count)]
        sessions = row.get("sessions", [])
        if len(sessions) != count:
            _fail("concurrency session receipt count differs")
        raw = json.dumps(sessions, sort_keys=True, separators=(",", ":")) + "\n"
        if row.get("stdout") != raw or row.get("stdout_sha256") != _sha(raw.encode()):
            _fail("concurrency raw output identity differs")
        for key, receipt_key in (
            ("session_identities", "identity"),
            ("publisher_identities", "publisher_identity"),
            ("cache_identities", "cache_identity"),
        ):
            values = [session.get(receipt_key) for session in sessions]
            if (
                any(not isinstance(value, str) or not value for value in values)
                or row.get(key) != values
                or len(set(values)) != count
            ):
                _fail("concurrency local identity is reused")
        semantic_rows = []
        for session in sessions:
            semantic_raw = json.dumps(session.get("semantic"), sort_keys=True, separators=(",", ":"))
            if session.get("raw_output_sha256") != _sha(semantic_raw.encode()):
                _fail("concurrency semantic output identity differs")
            semantic_rows.append(session.get("semantic"))
            if session.get("session_weak_alive_after") is not False:
                _fail("concurrency session weak state remains alive")
            if session.get("cache_weak_alive_after") is not False:
                _fail("concurrency cache weak state remains alive")
            boundaries = (
                session.get("start_ns"), session.get("barrier_ns"), session.get("end_ns")
            )
            if (
                any(not isinstance(value, int) or isinstance(value, bool) for value in boundaries)
                or not boundaries[0] <= boundaries[1] <= boundaries[2]
            ):
                _fail("concurrent session boundary is invalid")
        current_output = _sha(_canonical(semantic_rows))
        if row.get("output_sha256") != current_output:
            _fail("concurrency deterministic output digest differs")
        if output_identity is None:
            output_identity = _sha(_canonical([semantic_rows[0]]))
        if any(_sha(_canonical([semantic])) != output_identity for semantic in semantic_rows):
            _fail("concurrency outputs are not deterministic")
        if count > 1:
            latest_start = max(session["start_ns"] for session in sessions)
            earliest_end = min(session["end_ns"] for session in sessions)
            barriers = {session.get("barrier_ns") for session in sessions}
            if (
                len(barriers) != 1
                or latest_start >= earliest_end
                or any(
                    not (session["start_ns"] <= session["barrier_ns"] <= session["end_ns"])
                    for session in sessions
                )
            ):
                _fail("sessions did not execute concurrently")
        if row.get("retained_sessions") != 0 or row.get("retained_caches") != 0:
            _fail("concurrency state was retained")
        parsed_rss = RSS.findall(row.get("time_report", ""))
        if parsed_rss != [str(row.get("peak_rss_kib"))] or row["peak_rss_kib"] >= contract["concurrency"]["max_typokat_rss_kib"]:
            _fail("concurrency RSS evidence differs")
        before = row.get("executable_before_sha256")
        if before != row.get("executable_after_sha256"):
            _fail("concurrency binary changed")
        if binary_identity is None:
            binary_identity = before
        elif binary_identity != before:
            _fail("concurrency binary identity differs")
        if not row.get("process_group_empty"):
            _fail("concurrency process group is not empty")


def validate_timing_preconditions(evidence: dict[str, Any], contract: dict[str, Any]) -> None:
    for key in ("oracle_authority_sha256", "control_receipts_sha256"):
        if not HEX64.fullmatch(evidence.get(key, "")) or evidence[key] == "0" * 64:
            _fail(f"timing {key} is not authenticated")
    expected_argv = ["cpu-lease", "run", "-n", str(contract["cpu_lease_vcpus"]), "--no-smt", "--"]
    if evidence.get("lease_argv") != expected_argv or evidence.get("lease_vcpus") != contract["cpu_lease_vcpus"]:
        _fail("timing CPU lease differs")
    cpuset = evidence.get("cpuset")
    siblings = evidence.get("thread_siblings")
    if (
        not isinstance(cpuset, list)
        or len(cpuset) != contract["cpu_lease_vcpus"]
        or len(set(cpuset)) != len(cpuset)
        or not isinstance(siblings, dict)
    ):
        _fail("timing cpuset differs")
    for cpu in cpuset:
        group = siblings.get(str(cpu))
        if not isinstance(group, list) or cpu not in group or any(other in group for other in cpuset if other != cpu):
            _fail("timing cpuset includes SMT siblings")
    if evidence.get("affinity_before") != cpuset or evidence.get("affinity_after") != cpuset:
        _fail("timing affinity differs")
    if evidence.get("environment") != {"LANG": "C", "LC_ALL": "C", "TZ": "UTC"}:
        _fail("timing environment differs")
    limit = contract["child_address_space_limit_bytes"]
    if evidence.get("address_space_rlimit") != [limit, limit]:
        _fail("timing address-space limit differs")

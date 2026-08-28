#!/usr/bin/env python3
"""Live, fail-closed preflight collection for the WU6 session comparator."""

from __future__ import annotations

from dataclasses import dataclass
import errno
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import resource
import selectors
import shutil
import signal
import subprocess
import tempfile
import time
from typing import Any

import session_batch_bench as batch


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
RSS = re.compile(r"Maximum resident set size \(kbytes\):\s*(\d+)")
TIME_MARKER = "\tCommand being timed:"
MAX_STREAM_BYTES = 64 * 1024 * 1024
BUILD_ADDRESS_SPACE_LIMIT_BYTES = 4 << 30
BUILD_TIMEOUT_SECONDS = 900
RUNTIME_TIMEOUT_SECONDS = 10
GO_TOOLCHAIN = "go1.26.0"


@dataclass(frozen=True)
class _ProcessMeasurement:
    pid: int
    start_ns: int
    end_ns: int
    returncode: int
    stdout: str
    stderr: str
    process_group_empty: bool
    time_report: str
    rss_kib: int


def _fail(message: str) -> None:
    raise batch.BatchContractError(message)


def _canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def _read_regular(path: Path, label: str) -> bytes:
    if path.is_symlink() or not path.is_file():
        _fail(f"{label} is not a regular file: {path}")
    try:
        return path.read_bytes()
    except OSError as error:
        raise batch.BatchContractError(f"cannot read {label}: {error}") from error


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _sha_file(path: Path, label: str) -> str:
    return _sha(_read_regular(path, label))


def _exact_keys(value: Any, keys: set[str], label: str) -> None:
    if not isinstance(value, dict) or set(value) != keys:
        _fail(f"{label} shape differs")


def _validate_candidate(candidate: Any) -> None:
    _exact_keys(candidate, {"commit", "tree", "dirty"}, "candidate")
    if (
        not HEX40.fullmatch(candidate["commit"] or "")
        or not HEX40.fullmatch(candidate["tree"] or "")
        or candidate["dirty"] is not False
    ):
        _fail("candidate is not a clean frozen Git identity")


def _validate_runner(
    runner: Any, tool: str, candidate: dict[str, Any], contract: dict[str, Any]
) -> None:
    _exact_keys(
        runner, {"tool", "path", "source_sha256", "binary_sha256", "commit"},
        f"{tool} built runner",
    )
    expected_commit = (
        contract["tsgo"]["upstream_revision"] if tool == "tsgo" else candidate["commit"]
    )
    if (
        runner["tool"] != tool
        or not isinstance(runner["path"], str)
        or not runner["path"]
        or not HEX64.fullmatch(runner["source_sha256"] or "")
        or not HEX64.fullmatch(runner["binary_sha256"] or "")
        or runner["commit"] != expected_commit
    ):
        _fail(f"{tool} built runner identity differs")


def _link_process_record(record: Any, runner: dict[str, Any], label: str) -> None:
    if not isinstance(record, dict):
        _fail(f"{label} process record is not an object")
    expected = runner["binary_sha256"]
    if (
        record.get("executable_before_sha256") != expected
        or record.get("executable_after_sha256") != expected
    ):
        _fail(f"{label} process record differs from built runner identity")


def _single_frame_request(contract: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    authority = batch.load_authenticated_batch_oracles(contract)
    frame = authority["frames"][0]
    request = {
        "schema": 1,
        "case_id": frame["case_id"],
        "source": frame["source"],
    }
    return _canonical(request).decode() + "\n", authority


def _retain_fault_response(
    record: dict[str, Any], tool: str, fault: str
) -> dict[str, Any]:
    lines = record.get("stdout", "").splitlines()
    if len(lines) != 1:
        _fail(f"{tool} {fault} response is not one JSON object")
    try:
        response = json.loads(lines[0])
    except json.JSONDecodeError as error:
        raise batch.BatchContractError(f"invalid {tool} fault JSON: {error}") from error
    if not isinstance(response, dict):
        _fail(f"{tool} {fault} response is not an object")
    if (
        response.get("schema") != 1
        or response.get("fault") != fault
        or response.get("rejected") is not True
        or response.get("fault_hook_activations") != 1
        or record.get("fault_hook_activations") != 1
    ):
        _fail(f"{tool} {fault} activation or rejection receipt differs")
    retained = dict(record)
    retained["response"] = response
    return retained


def _collect_live_preflight(host: Any, contract: dict[str, Any]) -> dict[str, Any]:
    candidate = host.freeze_candidate()
    _validate_candidate(candidate)

    runners: dict[str, dict[str, Any]] = {}
    for tool in ("typokat", "tsgo"):
        runner = host.build_runner(tool, candidate)
        _validate_runner(runner, tool, candidate, contract)
        runners[tool] = runner

    protocols: dict[str, dict[str, Any]] = {}
    for tool in ("typokat", "tsgo"):
        protocol = host.protocol_info(tool, runners[tool])
        batch.validate_protocol_info(protocol, tool, runners[tool], contract)
        protocols[tool] = protocol

    request_stream, authority = _single_frame_request(contract)
    samples = []
    for tool in ("typokat", "tsgo"):
        sample = host.run_batch(
            tool,
            runners[tool],
            request_stream,
            timeout_seconds=RUNTIME_TIMEOUT_SECONDS,
            address_space_limit_bytes=contract["child_address_space_limit_bytes"],
        )
        _link_process_record(sample, runners[tool], f"{tool} batch")
        if sample.get("stderr"):
            _fail(f"{tool} batch stderr is not empty")
        samples.append(sample)

    expected = batch.oracle_panel(authority, 1)
    raw_evidence = {"projects": 1, "samples": samples}
    batch.verify_raw_oracles(raw_evidence, expected, contract)
    for sample in samples:
        batch.validate_lifecycle_receipts(sample["frames"], 1)
        batch.verify_route_intervals(sample, 1)
        for frame in sample["frames"]:
            if (
                frame.get("shared_product_identity")
                != contract["profile"]["length_framed_sha256"]
            ):
                _fail("shared product identity differs from authenticated profile")

    controls: dict[str, dict[str, Any]] = {"typokat": {}, "tsgo": {}}
    for tool in ("typokat", "tsgo"):
        for fault in contract["negative_controls"][tool]:
            record = host.run_fault(
                tool,
                runners[tool],
                fault,
                timeout_seconds=RUNTIME_TIMEOUT_SECONDS,
                address_space_limit_bytes=contract["child_address_space_limit_bytes"],
            )
            _link_process_record(record, runners[tool], f"{tool} {fault}")
            if record.get("stderr"):
                _fail(f"{tool} {fault} stderr is not empty")
            controls[tool][fault] = _retain_fault_response(record, tool, fault)
    batch.verify_negative_controls(controls, contract)

    attestation = host.final_attestation(candidate, runners)
    _exact_keys(attestation, {"candidate", "runners"}, "final attestation")
    if attestation["candidate"] != candidate:
        _fail("final candidate identity differs")
    if attestation["runners"] != runners:
        _fail("final runner identity differs")

    evidence = {
        "schema": 1,
        "candidate": candidate,
        "projects": 1,
        "runners": runners,
        "protocols": protocols,
        "samples": samples,
        "negative_controls": controls,
        "final_attestation": attestation,
    }
    return {
        "verdict": "PREFLIGHT-PASS",
        "evidence_sha256": _sha(_canonical(evidence)),
        "evidence": evidence,
    }


def _atomic_write_new(path: Path, data: bytes) -> None:
    parent = path.parent
    if parent.is_symlink() or not parent.is_dir():
        _fail(f"output parent is not an existing regular directory: {parent}")
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=parent)
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        try:
            os.link(temporary, path)
        except FileExistsError as error:
            raise batch.BatchContractError(f"output already exists: {path}") from error
        directory_fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    except OSError as error:
        if error.errno == errno.EEXIST:
            raise batch.BatchContractError(f"output already exists: {path}") from error
        raise batch.BatchContractError(f"cannot publish preflight evidence: {error}") from error
    finally:
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def collect_live_preflight(*, output: Path) -> dict[str, Any]:
    output = Path(output)
    if output.exists() or output.is_symlink():
        _fail(f"output already exists: {output}")
    contract = batch.verify_contract()
    result = _collect_live_preflight(SystemLiveHost(contract), contract)
    _atomic_write_new(output, _canonical(result) + b"\n")
    return result


class SystemLiveHost:
    """Real build and process boundary used only by the public collector."""

    def __init__(self, contract: dict[str, Any]) -> None:
        self.contract = contract
        self.live_root = Path(tempfile.mkdtemp(prefix="typokat-wu6-", dir="/tmp"))
        self.live_root.chmod(0o700)
        self.system_home = Path(pwd.getpwuid(os.getuid()).pw_dir)
        self.git_home = self.live_root / "git-home"
        self.git_home.mkdir(parents=True, exist_ok=True)
        self._tsgo_checkout_tree: str | None = None
        self._go_toolchain_identity: str | None = None

    def freeze_candidate(self) -> dict[str, Any]:
        commit = self._git(ROOT, "rev-parse", "HEAD")
        tree = self._git(ROOT, "rev-parse", "HEAD^{tree}")
        status = self._git(ROOT, "status", "--porcelain=v1", "--untracked-files=all")
        return {"commit": commit, "tree": tree, "dirty": bool(status)}

    def build_runner(
        self, tool: str, candidate: dict[str, Any]
    ) -> dict[str, Any]:
        if tool == "typokat":
            return self._build_typokat(candidate)
        if tool == "tsgo":
            return self._build_tsgo(candidate)
        _fail(f"unsupported live runner: {tool}")

    def protocol_info(
        self, tool: str, runner: dict[str, Any]
    ) -> dict[str, Any]:
        measurement = self._run_runner(
            tool, runner, ["--protocol-info"], "",
            timeout_seconds=RUNTIME_TIMEOUT_SECONDS,
            address_space_limit_bytes=self.contract["child_address_space_limit_bytes"],
            measured=False,
        )
        if measurement.returncode != 0 or measurement.stderr:
            _fail(f"{tool} protocol-info process failed")
        lines = measurement.stdout.splitlines()
        if len(lines) != 1:
            _fail(f"{tool} protocol-info output is not one JSON object")
        try:
            value = json.loads(lines[0])
        except json.JSONDecodeError as error:
            raise batch.BatchContractError(f"invalid {tool} protocol-info JSON: {error}") from error
        if not isinstance(value, dict):
            _fail(f"{tool} protocol-info is not an object")
        return value

    def run_batch(
        self,
        tool: str,
        runner: dict[str, Any],
        request_stream: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, Any]:
        before = _sha_file(Path(runner["path"]), f"{tool} runner")
        measurement = self._run_runner(
            tool, runner, [], request_stream,
            timeout_seconds=timeout_seconds,
            address_space_limit_bytes=address_space_limit_bytes,
            measured=True,
        )
        after = _sha_file(Path(runner["path"]), f"{tool} runner")
        request_lines = request_stream.splitlines()
        response_lines = measurement.stdout.splitlines()
        if len(request_lines) != 1 or len(response_lines) != 1:
            _fail(f"{tool} one-frame JSONL stream count differs")
        try:
            response = json.loads(response_lines[0])
        except json.JSONDecodeError as error:
            raise batch.BatchContractError(f"invalid {tool} frame JSON: {error}") from error
        if not isinstance(response, dict):
            _fail(f"{tool} frame response is not an object")
        canonical_response = _canonical(response)
        frame = dict(response)
        frame.update({
            "raw_request": request_lines[0],
            "raw_response": canonical_response.decode(),
            "raw_response_sha256": _sha(canonical_response),
        })
        return {
            "block": 0,
            "slot": 0,
            "tool": tool,
            "pid": measurement.pid,
            "start_ns": measurement.start_ns,
            "end_ns": measurement.end_ns,
            "returncode": measurement.returncode,
            "stdout": measurement.stdout,
            "stderr": measurement.stderr,
            "stdout_complete": True,
            "stderr_complete": True,
            "timed_out": False,
            "process_group_empty": measurement.process_group_empty,
            "executable_before_sha256": before,
            "executable_after_sha256": after,
            "address_space_limit_bytes": address_space_limit_bytes,
            "request_stream": request_stream,
            "response_stream": measurement.stdout,
            "request_stream_sha256": _sha(request_stream.encode()),
            "response_stream_sha256": _sha(measurement.stdout.encode()),
            "time_report": measurement.time_report,
            "rss_kib": measurement.rss_kib,
            "frames": [frame],
        }

    def run_fault(
        self,
        tool: str,
        runner: dict[str, Any],
        fault: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, Any]:
        request = _canonical({"schema": 1, "fault": fault}).decode() + "\n"
        before = _sha_file(Path(runner["path"]), f"{tool} runner")
        measurement = self._run_runner(
            tool, runner, [], request,
            timeout_seconds=timeout_seconds,
            address_space_limit_bytes=address_space_limit_bytes,
            measured=True,
        )
        after = _sha_file(Path(runner["path"]), f"{tool} runner")
        lines = measurement.stdout.splitlines()
        if len(lines) != 1:
            _fail(f"{tool} {fault} response is not one JSON object")
        try:
            response = json.loads(lines[0])
        except json.JSONDecodeError as error:
            raise batch.BatchContractError(f"invalid {tool} fault JSON: {error}") from error
        if not isinstance(response, dict):
            _fail(f"{tool} {fault} response is not an object")
        self._validate_live_fault_response(tool, fault, response)
        if measurement.returncode != 2:
            _fail(f"{tool} {fault} process did not use the rejection exit status")
        return {
            "raw_request": request,
            "stdout": measurement.stdout,
            "stderr": measurement.stderr,
            "returncode": measurement.returncode,
            "process_group_empty": measurement.process_group_empty,
            "executable_before_sha256": before,
            "executable_after_sha256": after,
            "address_space_limit_bytes": address_space_limit_bytes,
            "time_report": measurement.time_report,
            "rss_kib": measurement.rss_kib,
            "fault_hook_activations": response.get("fault_hook_activations"),
            "response": response,
        }

    def final_attestation(
        self,
        candidate: dict[str, Any],
        runners: dict[str, dict[str, Any]],
    ) -> dict[str, Any]:
        observed_candidate = self.freeze_candidate()
        typokat_source = ROOT / self.contract["typokat"]["runner_source_relative_path"]
        tsgo_source = ROOT / self.contract["tsgo"]["runner_source_relative_path"]
        observed_runners = {
            "typokat": {
                **runners["typokat"],
                "source_sha256": _sha_file(typokat_source, "final typokat runner source"),
                "binary_sha256": _sha_file(
                    Path(runners["typokat"]["path"]), "final typokat runner binary"
                ),
            },
            "tsgo": {
                **runners["tsgo"],
                "source_sha256": _sha_file(tsgo_source, "final TSGo runner source"),
                "binary_sha256": _sha_file(
                    Path(runners["tsgo"]["path"]), "final TSGo runner binary"
                ),
            },
        }
        state = self._authenticate_tsgo_checkout(
            _read_regular(tsgo_source, "final TSGo runner source")
        )
        if self._tsgo_checkout_tree is None or state["tree"] != self._tsgo_checkout_tree:
            _fail("authenticated TSGo checkout tree changed during preflight")
        ending_candidate = self.freeze_candidate()
        if ending_candidate != observed_candidate:
            _fail("candidate changed during final attestation")
        return {"candidate": ending_candidate, "runners": observed_runners}

    def _validate_live_fault_response(
        self, tool: str, fault: str, response: dict[str, Any]
    ) -> None:
        if tool == "tsgo":
            _exact_keys(
                response,
                {"schema", "fault", "fault_hook_activations", "rejected"},
                f"{tool} {fault} response",
            )
        elif tool == "typokat":
            _exact_keys(
                response,
                {
                    "schema", "fault", "fault_seam", "fault_hook_activations",
                    "rejected", "shared_product_identity", "project_identity",
                    "publisher_identity", "cache_identity", "session_weak_alive_after",
                    "cache_weak_alive_after",
                },
                f"{tool} {fault} response",
            )
            seams = {"reuse-session": "session-consume", "retain-cache": "cache-release"}
            local_identities = [
                response.get("project_identity"),
                response.get("publisher_identity"),
                response.get("cache_identity"),
            ]
            if (
                response.get("fault_seam") != seams.get(fault)
                or response.get("shared_product_identity")
                != self.contract["profile"]["length_framed_sha256"]
                or any(
                    not isinstance(value, str) or not value
                    for value in local_identities
                )
                or len(set(local_identities)) != len(local_identities)
                or response.get("session_weak_alive_after") is not False
                or response.get("cache_weak_alive_after") is not False
            ):
                _fail(f"{tool} {fault} lifecycle receipt differs")
        else:
            _fail(f"unsupported fault response tool: {tool}")
        if (
            response.get("schema") != 1
            or response.get("fault") != fault
            or response.get("fault_hook_activations") != 1
            or response.get("rejected") is not True
        ):
            _fail(f"{tool} {fault} rejection receipt differs")

    def _build_typokat(self, candidate: dict[str, Any]) -> dict[str, Any]:
        if self.freeze_candidate() != candidate:
            _fail("candidate changed before typokat build")
        source = ROOT / self.contract["typokat"]["runner_source_relative_path"]
        source_sha = _sha_file(source, "typokat runner source")
        target = self.live_root / "rust-target"
        cargo = self.system_home / ".cargo/bin/cargo"
        rustup_home = self.system_home / ".rustup"
        if not os.access(cargo, os.X_OK) or not rustup_home.is_dir():
            _fail("cargo is unavailable")
        cargo_home = self.live_root / "cargo-home"
        build_home = self.live_root / "rust-home"
        cargo_home.mkdir(parents=True, exist_ok=True)
        build_home.mkdir(parents=True, exist_ok=True)
        environment = {
            "PATH": f"{cargo.parent}:/usr/bin:/bin",
            "HOME": str(build_home),
            "RUSTUP_HOME": str(rustup_home),
            "RUSTUP_TOOLCHAIN": "1.95.0-x86_64-unknown-linux-gnu",
            "CARGO_HOME": str(cargo_home),
            "CARGO_TARGET_DIR": str(target),
            "CARGO_BUILD_JOBS": "1",
            "CARGO_TERM_COLOR": "never",
            "CARGO_ENCODED_RUSTFLAGS": "",
            "RUSTFLAGS": "",
            "RUSTC_WRAPPER": "",
            "RUSTC_WORKSPACE_WRAPPER": "",
            "TYPOKAT_CANDIDATE_COMMIT": candidate["commit"],
        }
        argv = [
            *self.contract["typokat"]["build_argv"],
            "--manifest-path",
            str(ROOT / "Cargo.toml"),
        ]
        self._validate_cargo_configuration(self.live_root, cargo_home)
        try:
            self._run_build(argv, self.live_root, environment)
        finally:
            self._validate_cargo_configuration(self.live_root, cargo_home)
        if self.freeze_candidate() != candidate:
            _fail("candidate changed during typokat build")
        built = target / "release" / self.contract["typokat"]["runner_binary"]
        binary_sha = _sha_file(built, "built typokat runner")
        return {
            "tool": "typokat",
            "path": str(built.resolve()),
            "source_sha256": source_sha,
            "binary_sha256": binary_sha,
            "commit": candidate["commit"],
        }

    def _validate_cargo_configuration(self, cwd: Path, cargo_home: Path) -> None:
        candidates = [cargo_home / "config", cargo_home / "config.toml"]
        for directory in (cwd, *cwd.parents):
            candidates.extend(
                (directory / ".cargo/config", directory / ".cargo/config.toml")
            )
        for candidate in candidates:
            try:
                candidate.lstat()
            except FileNotFoundError:
                continue
            except OSError as error:
                raise batch.BatchContractError(
                    f"cannot authenticate Cargo configuration path {candidate}: {error}"
                ) from error
            _fail(f"Cargo configuration is present: {candidate}")

    def _build_tsgo(self, candidate: dict[str, Any]) -> dict[str, Any]:
        if self.freeze_candidate() != candidate:
            _fail("candidate changed before TSGo build")
        checkout = ROOT / "target/wu6/typescript-go"
        revision = self.contract["tsgo"]["upstream_revision"]
        source = ROOT / self.contract["tsgo"]["runner_source_relative_path"]
        source_data = _read_regular(source, "TSGo runner source")
        initial_state = self._authenticate_tsgo_checkout(None)
        install = checkout / self.contract["tsgo"]["runner_install_path"]
        install.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, install)
        if _read_regular(install, "installed TSGo runner source") != source_data:
            _fail("installed TSGo runner source identity differs")
        installed_state = self._authenticate_tsgo_checkout(source_data)
        if installed_state != initial_state:
            _fail("authenticated TSGo checkout changed while installing runner source")
        self._tsgo_checkout_tree = installed_state["tree"]

        cold_binary = ROOT / self.contract["tsgo"]["cold_binary_relative_path"]
        output = cold_binary.parent / f"typokat-wu6-batch-{self.live_root.name}"
        descriptor, temporary_name = tempfile.mkstemp(
            prefix=f".{output.name}.", dir=output.parent
        )
        os.close(descriptor)
        temporary = Path(temporary_name)
        temporary.unlink()
        argv = [
            temporary_name if item == "{output}" else item
            for item in self.contract["tsgo"]["build_argv"]
        ]
        go_environment = self._go_build_environment()
        self._validate_go_toolchain(go_environment)
        try:
            self._run_build(
                argv,
                checkout,
                go_environment,
            )
            _sha_file(temporary, "built TSGo runner")
            try:
                os.link(temporary, output)
            except FileExistsError as error:
                raise batch.BatchContractError(
                    "unique TSGo runner output already exists"
                ) from error
        finally:
            try:
                temporary.unlink()
            except FileNotFoundError:
                pass
        if self.freeze_candidate() != candidate:
            _fail("candidate changed during TSGo build")
        if self._authenticate_tsgo_checkout(source_data) != installed_state:
            _fail("authenticated TSGo checkout changed during build")
        self._validate_go_toolchain(go_environment)
        return {
            "tool": "tsgo",
            "path": str(output.resolve()),
            "source_sha256": _sha(source_data),
            "binary_sha256": _sha_file(output, "built TSGo runner"),
            "commit": revision,
        }

    def _authenticate_tsgo_checkout(
        self, installed_source: bytes | None
    ) -> dict[str, str]:
        checkout = ROOT / "target/wu6/typescript-go"
        revision = self.contract["tsgo"]["upstream_revision"]
        head = self._git(checkout, "rev-parse", "HEAD")
        tree = self._git(checkout, "rev-parse", "HEAD^{tree}")
        origin = self._git(checkout, "remote", "get-url", "origin")
        if head != revision or not HEX40.fullmatch(tree):
            _fail("authenticated TSGo checkout revision or tree differs")
        if origin != self.contract["tsgo"]["repository"]:
            _fail("authenticated TSGo checkout repository differs")
        status = self._git(
            checkout,
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
        )
        allowed = f"?? {self.contract['tsgo']['runner_install_path']}"
        lines = status.splitlines() if status else []
        expected_statuses = ([allowed],) if installed_source is not None else ([], [allowed])
        if lines not in expected_statuses:
            _fail("authenticated TSGo checkout contains build-affecting files")
        if installed_source is not None:
            install = checkout / self.contract["tsgo"]["runner_install_path"]
            if _read_regular(install, "installed TSGo runner source") != installed_source:
                _fail("installed TSGo runner source identity differs")
        cold_binary = ROOT / self.contract["tsgo"]["cold_binary_relative_path"]
        cold_sha = _sha_file(cold_binary, "authenticated cold TSGo binary")
        if cold_sha != self.contract["tsgo"]["cold_binary_sha256"]:
            _fail("authenticated cold TSGo binary identity differs")
        return {"head": head, "tree": tree, "origin": origin, "cold_binary_sha256": cold_sha}

    def _go_build_environment(self) -> dict[str, str]:
        go = Path("/usr/bin/go")
        if not os.access(go, os.X_OK):
            _fail("Go bootstrap executable is unavailable")
        go_home = self.live_root / "go-home"
        go_cache = self.live_root / "go-cache"
        go_path = self.live_root / "go-path"
        go_mod_cache = self.live_root / "go-mod-cache"
        for directory in (go_home, go_cache, go_path, go_mod_cache):
            directory.mkdir(mode=0o700)
        return {
            "PATH": f"{go.parent}:/usr/bin:/bin",
            "HOME": str(go_home),
            "GOCACHE": str(go_cache),
            "GOPATH": str(go_path),
            "GOMODCACHE": str(go_mod_cache),
            "GOENV": "off",
            "GOFLAGS": "",
            "GOTOOLCHAIN": GO_TOOLCHAIN,
            "GOMAXPROCS": "1",
        }

    def _validate_go_toolchain(self, environment: dict[str, str]) -> None:
        result = _supervise(
            ["go", "version"],
            "",
            cwd=ROOT / "target/wu6/typescript-go",
            environment=environment,
            timeout_seconds=BUILD_TIMEOUT_SECONDS,
            address_space_limit_bytes=BUILD_ADDRESS_SPACE_LIMIT_BYTES,
            measured=False,
        )
        expected = f"go version {GO_TOOLCHAIN} linux/amd64"
        allowed_download = f"go: downloading {GO_TOOLCHAIN} (linux/amd64)\n"
        if (
            result.returncode != 0
            or result.stdout.strip() != expected
            or result.stderr not in ("", allowed_download)
        ):
            _fail("effective Go toolchain identity differs")
        observed = result.stdout.strip()
        if self._go_toolchain_identity not in (None, observed):
            _fail("effective Go toolchain changed during build")
        self._go_toolchain_identity = observed

    def _run_build(
        self, argv: list[str], cwd: Path, environment: dict[str, str]
    ) -> None:
        result = _supervise(
            argv,
            "",
            cwd=cwd,
            environment=environment,
            timeout_seconds=BUILD_TIMEOUT_SECONDS,
            address_space_limit_bytes=BUILD_ADDRESS_SPACE_LIMIT_BYTES,
            measured=False,
        )
        if result.returncode != 0:
            detail = result.stderr[-4000:] or result.stdout[-4000:]
            _fail(f"runner build failed: {detail}")

    def _run_runner(
        self,
        tool: str,
        runner: dict[str, Any],
        arguments: list[str],
        stdin: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
        measured: bool,
    ) -> _ProcessMeasurement:
        environment = {"GOMAXPROCS": "1"} if tool == "tsgo" else {}
        return _supervise(
            [runner["path"], *arguments],
            stdin,
            cwd=ROOT,
            environment=environment,
            timeout_seconds=timeout_seconds,
            address_space_limit_bytes=address_space_limit_bytes,
            measured=measured,
        )

    def _git(self, cwd: Path, *arguments: str) -> str:
        environment = {
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
            "TZ": "UTC",
            "HOME": str(self.git_home),
            "XDG_CONFIG_HOME": str(self.git_home),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_SYSTEM": "/dev/null",
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_NO_REPLACE_OBJECTS": "1",
            "GIT_OPTIONAL_LOCKS": "0",
        }
        try:
            result = subprocess.run(
                [
                    "/usr/bin/git", "--no-replace-objects",
                    "-c", "core.fsmonitor=false",
                    "-c", "core.untrackedCache=false",
                    "-c", "core.hooksPath=/dev/null",
                    *arguments,
                ],
                cwd=cwd,
                env=environment,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise batch.BatchContractError(f"cannot authenticate Git checkout: {error}") from error
        if result.returncode != 0 or result.stderr:
            _fail("cannot authenticate Git checkout")
        return result.stdout.strip()


def _supervise(
    argv: list[str],
    stdin_text: str,
    *,
    cwd: Path,
    environment: dict[str, str],
    timeout_seconds: int,
    address_space_limit_bytes: int,
    measured: bool,
) -> _ProcessMeasurement:
    if not argv or any(not isinstance(item, str) or not item or "\x00" in item for item in argv):
        _fail("live child command is invalid")
    if address_space_limit_bytes <= 0:
        _fail("live child address-space limit is invalid")
    command = ["/usr/bin/time", "-v", *argv] if measured else list(argv)
    child_environment = {
        "PATH": "/usr/bin:/bin",
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "TZ": "UTC",
    }
    child_environment.update(environment)

    def child_limits() -> None:
        # Descendants inherit this per-process ceiling; it is not aggregate tree accounting.
        resource.setrlimit(
            resource.RLIMIT_AS,
            (address_space_limit_bytes, address_space_limit_bytes),
        )
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    started = time.monotonic_ns()
    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=child_environment,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
            close_fds=True,
            preexec_fn=child_limits,
        )
    except OSError as error:
        raise batch.BatchContractError(f"cannot start live child: {error}") from error
    assert process.stdin is not None and process.stdout is not None and process.stderr is not None
    input_data = stdin_text.encode()
    input_offset = 0
    output = bytearray()
    errors = bytearray()
    selector = selectors.DefaultSelector()
    for stream in (process.stdin, process.stdout, process.stderr):
        os.set_blocking(stream.fileno(), False)
    selector.register(process.stdout, selectors.EVENT_READ, output)
    selector.register(process.stderr, selectors.EVENT_READ, errors)
    if input_data:
        selector.register(process.stdin, selectors.EVENT_WRITE, None)
    else:
        process.stdin.close()
    deadline = time.monotonic() + timeout_seconds
    failure: str | None = None
    try:
        while selector.get_map():
            if time.monotonic() >= deadline:
                failure = f"live child timed out after {timeout_seconds}s"
                break
            for key, _ in selector.select(timeout=0.05):
                stream = key.fileobj
                if stream is process.stdin:
                    try:
                        written = os.write(stream.fileno(), input_data[input_offset:])
                    except BrokenPipeError:
                        failure = "live child closed stdin before the complete request was written"
                        selector.unregister(stream)
                        stream.close()
                        break
                    if written <= 0:
                        failure = "live child accepted no bytes from the pending request"
                        break
                    input_offset += written
                    if input_offset == len(input_data):
                        selector.unregister(stream)
                        stream.close()
                    continue
                target = key.data
                try:
                    chunk = os.read(stream.fileno(), 65_536)
                except BlockingIOError:
                    continue
                if not chunk:
                    selector.unregister(stream)
                    stream.close()
                    continue
                target.extend(chunk)
                if len(target) > MAX_STREAM_BYTES:
                    failure = "live child output exceeded the bounded stream limit"
                    break
            if failure:
                break
            if process.poll() is not None and process.stdin in [key.fileobj for key in selector.get_map().values()]:
                if input_offset != len(input_data):
                    failure = "live child exited before the complete request was written"
                    break
                selector.unregister(process.stdin)
                process.stdin.close()
        if input_offset != len(input_data) and failure is None:
            failure = "live child request stream was not written completely"
        if failure:
            _kill_group(process.pid)
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            _kill_group(process.pid)
            process.wait(timeout=1)
    finally:
        selector.close()
        for stream in (process.stdin, process.stdout, process.stderr):
            try:
                stream.close()
            except OSError:
                pass
    group_empty = _wait_group_empty(process.pid, 1.0)
    if failure:
        _fail(failure)
    if not group_empty:
        _fail("live child process-group containment failed")
    ended = time.monotonic_ns()
    try:
        stdout = output.decode()
        raw_stderr = errors.decode()
    except UnicodeDecodeError as error:
        raise batch.BatchContractError(f"live child emitted malformed UTF-8: {error}") from error

    stderr = raw_stderr
    time_report = ""
    rss_kib = 0
    if measured:
        if raw_stderr.count(TIME_MARKER) != 1:
            _fail("live child /usr/bin/time report is missing or ambiguous")
        marker = raw_stderr.index(TIME_MARKER)
        stderr = raw_stderr[:marker]
        status = f"Command exited with non-zero status {process.returncode}\n"
        if process.returncode and stderr.endswith(status):
            stderr = stderr[:-len(status)]
        time_report = raw_stderr[marker:]
        matches = RSS.findall(time_report)
        if len(matches) != 1:
            _fail("live child RSS report is missing or ambiguous")
        rss_kib = int(matches[0])
    return _ProcessMeasurement(
        pid=process.pid,
        start_ns=started,
        end_ns=ended,
        returncode=process.returncode,
        stdout=stdout,
        stderr=stderr,
        process_group_empty=group_empty,
        time_report=time_report,
        rss_kib=rss_kib,
    )


def _kill_group(pid: int) -> None:
    try:
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def _group_exists(pid: int) -> bool:
    try:
        os.killpg(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _wait_group_empty(pid: int, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while _group_exists(pid) and time.monotonic() < deadline:
        time.sleep(0.005)
    return not _group_exists(pid)

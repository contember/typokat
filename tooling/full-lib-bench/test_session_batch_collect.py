#!/usr/bin/env python3
"""RED acceptance for the live WU6 preflight collector."""

from __future__ import annotations

import copy
import hashlib
import inspect
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import session_batch_bench as batch
import session_batch_collect as collect
from test_session_batch_bench import (
    apply_real_oracle,
    concurrency_evidence,
    make_cold_process,
    make_single_project_process,
    protocol_info,
    raw_panel_evidence,
    refresh_concurrency_raw,
    refresh_sample_streams,
    refresh_single_project_response,
    single_project_evidence,
)


class FakeLiveHost:
    def __init__(self, contract: dict[str, object]) -> None:
        self.contract = contract
        self._next_pid = 40_000
        self.candidate = {
            "commit": "a" * 40,
            "tree": "b" * 40,
            "dirty": False,
        }
        authority = batch.load_authenticated_batch_oracles(contract)
        fixture = raw_panel_evidence(1, 2.0, 100_000, 150_000)
        apply_real_oracle(fixture, authority)
        for sample in fixture["samples"]:
            sample["frames"][0]["shared_product_identity"] = contract["profile"][
                "length_framed_sha256"
            ]
            refresh_responses(sample)
        self.samples = {
            tool: copy.deepcopy(next(row for row in fixture["samples"] if row["tool"] == tool))
            for tool in ("typokat", "tsgo")
        }
        self.controls = copy.deepcopy(fixture["negative_controls"])
        self.calls: list[tuple[object, ...]] = []

    def freeze_candidate(self) -> dict[str, object]:
        self.calls.append(("freeze_candidate",))
        return copy.deepcopy(self.candidate)

    def build_runner(self, tool: str, candidate: dict[str, object]) -> dict[str, object]:
        self.calls.append(("build_runner", tool, candidate["commit"]))
        commit = (
            self.contract["tsgo"]["upstream_revision"]
            if tool == "tsgo"
            else candidate["commit"]
        )
        return {
            "tool": tool,
            "path": f"/tmp/{tool}-runner",
            "source_sha256": "1" * 64,
            "binary_sha256": "2" * 64,
            "commit": commit,
        }

    def protocol_info(
        self, tool: str, runner: dict[str, object]
    ) -> dict[str, object]:
        self.calls.append(("protocol_info", tool, runner["binary_sha256"]))
        info = protocol_info(tool)
        info["upstream_revision"] = runner["commit"]
        return info

    def run_batch(
        self,
        tool: str,
        runner: dict[str, object],
        request_stream: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        self.calls.append(
            (
                "run_batch",
                tool,
                timeout_seconds,
                address_space_limit_bytes,
            )
        )
        sample = copy.deepcopy(self.samples[tool])
        if request_stream != sample["request_stream"]:
            raise AssertionError("collector changed authenticated request bytes")
        if runner["binary_sha256"] != sample["executable_before_sha256"]:
            raise AssertionError("collector changed runner identity")
        return sample

    def run_fault(
        self,
        tool: str,
        runner: dict[str, object],
        fault: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        self.calls.append(
            (
                "run_fault",
                tool,
                fault,
                timeout_seconds,
                address_space_limit_bytes,
            )
        )
        record = copy.deepcopy(self.controls[tool][fault])
        if runner["binary_sha256"] != record["executable_before_sha256"]:
            raise AssertionError("collector changed fault runner identity")
        return record

    def final_attestation(
        self,
        candidate: dict[str, object],
        runners: dict[str, dict[str, object]],
    ) -> dict[str, object]:
        self.calls.append(("final_attestation", candidate["commit"]))
        return {
            "candidate": copy.deepcopy(candidate),
            "runners": copy.deepcopy(runners),
        }


class FakePanelHost:
    def __init__(
        self,
        contract: dict[str, object],
        *,
        fail_at: int | None = None,
        final_mutation=None,
        reuse_cross_panel_pid: bool = False,
        runner_collision: str | None = None,
        profile_mutation: str | None = None,
    ) -> None:
        self.contract = contract
        self.fail_at = fail_at
        self.final_mutation = final_mutation
        self.reuse_cross_panel_pid = reuse_cross_panel_pid
        self.runner_collision = runner_collision
        self.profile_mutation = profile_mutation
        self.candidate = {
            "commit": "a" * 40,
            "tree": "b" * 40,
            "dirty": False,
        }
        self.authority = batch.load_authenticated_batch_oracles(contract)
        self.calls: list[tuple[object, ...]] = []
        self.panel_order: list[int] = []
        self.process_receipts: list[dict[str, object]] = []
        self._panels: dict[int, dict[str, object]] = {}
        self._batch_offsets: dict[int, int] = {}
        self._fault_offset = 0
        self._first_batch_pid: int | None = None
        self.final_execution_affinity = [8]
        control_fixture = raw_panel_evidence(1, 2.0, 100_000, 150_000)
        self.controls = copy.deepcopy(control_fixture["negative_controls"])
        for tool, binary in (("typokat", "2" * 64), ("tsgo", "3" * 64)):
            for receipt in self.controls[tool].values():
                receipt["executable_before_sha256"] = binary
                receipt["executable_after_sha256"] = binary
        self.runners: dict[str, dict[str, object]] = {}
        self.timing_boundary = {
            "lease_argv": ["cpu-lease", "run", "-n", "2", "--no-smt", "--"],
            "lease_vcpus": 2,
            "reserved_cpuset": [8, 9],
            "thread_siblings": {"8": [8, 9], "9": [8, 9]},
            "environment": {"LANG": "C", "LC_ALL": "C", "TZ": "UTC"},
            "address_space_rlimit": [1 << 30, 1 << 30],
        }

    def freeze_candidate(self) -> dict[str, object]:
        self.calls.append(("freeze_candidate",))
        return copy.deepcopy(self.candidate)

    def build_runner(self, tool: str, candidate: dict[str, object]) -> dict[str, object]:
        self.calls.append(("build_runner", tool, candidate["commit"]))
        runner = {
            "tool": tool,
            "path": f"/tmp/{tool}-panel-runner",
            "source_sha256": "4" * 64 if tool == "typokat" else "5" * 64,
            "binary_sha256": "2" * 64 if tool == "typokat" else "3" * 64,
            "commit": (
                candidate["commit"]
                if tool == "typokat"
                else self.contract["tsgo"]["upstream_revision"]
            ),
        }
        if tool == "tsgo" and self.runner_collision in {
            "path", "source_sha256", "binary_sha256"
        }:
            runner[self.runner_collision] = self.runners["typokat"][
                self.runner_collision
            ]
        self.runners[tool] = copy.deepcopy(runner)
        if tool == "tsgo" and self.runner_collision == "binary_sha256":
            for receipt in self.controls["tsgo"].values():
                receipt["executable_before_sha256"] = runner["binary_sha256"]
                receipt["executable_after_sha256"] = runner["binary_sha256"]
        return runner

    def _authenticate_runner(self, tool: str, runner: dict[str, object]) -> None:
        if runner != self.runners.get(tool):
            raise AssertionError("collector used the wrong tool runner")

    def protocol_info(
        self, tool: str, runner: dict[str, object]
    ) -> dict[str, object]:
        self._authenticate_runner(tool, runner)
        self.calls.append(("protocol_info", tool, runner["binary_sha256"]))
        info = protocol_info(tool)
        info.update(
            source_sha256=runner["source_sha256"],
            binary_sha256=runner["binary_sha256"],
            upstream_revision=runner["commit"],
        )
        return info

    def observe_timing_boundary(self) -> dict[str, object]:
        if self.runner_collision is not None:
            raise AssertionError("collector reached timing after a runner collision")
        ordinal = sum(call[0] == "observe_timing_boundary" for call in self.calls)
        self.calls.append(("observe_timing_boundary", ordinal))
        value = copy.deepcopy(self.timing_boundary)
        value["execution_affinity"] = (
            [8] if ordinal == 0 else copy.deepcopy(self.final_execution_affinity)
        )
        return value

    def _panel(self, projects: int) -> dict[str, object]:
        if projects not in self._panels:
            typokat_rss = 600_000 if projects == self.fail_at else 100_000
            panel = raw_panel_evidence(projects, 2.0, typokat_rss, 150_000)
            apply_real_oracle(panel, self.authority)
            pid_offset = self.contract["panel_project_counts"].index(projects) * 1_000_000
            for process in [*panel["warmups"], *panel["samples"]]:
                process["pid"] += pid_offset
                binary = "2" * 64 if process["tool"] == "typokat" else "3" * 64
                process["executable_before_sha256"] = binary
                process["executable_after_sha256"] = binary
            for sample in panel["samples"]:
                for frame in sample["frames"]:
                    frame["shared_product_identity"] = self.contract["profile"][
                        "length_framed_sha256"
                    ]
                refresh_responses(sample)
            for warmup in panel["warmups"]:
                source = next(
                    sample
                    for sample in panel["samples"]
                    if sample["tool"] == warmup["tool"]
                )
                warmup["frames"] = copy.deepcopy(source["frames"])
            if self.profile_mutation == "warmup" and panel["warmups"]:
                panel["warmups"][0]["frames"][0]["shared_product_identity"] = "0" * 64
            elif self.profile_mutation == "sample":
                panel["samples"][0]["frames"][0]["shared_product_identity"] = "0" * 64
                refresh_responses(panel["samples"][0])
            self._panels[projects] = panel
            self._batch_offsets[projects] = 0
            self.panel_order.append(projects)
        return self._panels[projects]

    def run_batch(
        self,
        tool: str,
        runner: dict[str, object],
        request_stream: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        self._authenticate_runner(tool, runner)
        projects = len(request_stream.splitlines())
        panel = self._panel(projects)
        offset = self._batch_offsets[projects]
        expected_rows = [*panel["warmups"], *panel["samples"]]
        if offset >= len(expected_rows):
            raise AssertionError("collector launched an extra panel process")
        expected = expected_rows[offset]
        if tool != expected["tool"]:
            raise AssertionError("collector changed warmup/ABBA tool order")
        if request_stream != panel["samples"][0]["request_stream"]:
            raise AssertionError("collector changed authenticated panel request bytes")
        panel_index = self.contract["panel_project_counts"].index(projects)
        expected_timeout = self.contract["sampling"]["panel_timeout_seconds"][panel_index]
        if timeout_seconds != expected_timeout:
            raise AssertionError("collector changed panel timeout")
        if address_space_limit_bytes != self.contract["child_address_space_limit_bytes"]:
            raise AssertionError("collector changed child address-space limit")
        if runner["binary_sha256"] != expected["executable_before_sha256"]:
            raise AssertionError("collector changed runner identity")
        phase = "warmup" if offset < len(panel["warmups"]) else "sample"
        self.calls.append(("run_batch", projects, phase, offset, tool))
        self._batch_offsets[projects] = offset + 1
        record = copy.deepcopy(expected)
        if self._first_batch_pid is None:
            self._first_batch_pid = record["pid"]
        elif self.reuse_cross_panel_pid and projects != 1 and offset == 0:
            record["pid"] = self._first_batch_pid
        self.process_receipts.append(copy.deepcopy(record))
        return record

    def run_fault(
        self,
        tool: str,
        runner: dict[str, object],
        fault: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        self._authenticate_runner(tool, runner)
        expected_faults = [
            ("typokat", "reuse-session"),
            ("typokat", "retain-cache"),
            ("tsgo", "reuse-checker"),
            ("tsgo", "retain-frame"),
        ]
        offset = self._fault_offset
        if offset >= len(expected_faults) or (tool, fault) != expected_faults[offset]:
            raise AssertionError("collector changed negative-control order")
        expected_timeout = self.contract["sampling"]["panel_timeout_seconds"][0]
        if timeout_seconds != expected_timeout:
            raise AssertionError("collector changed fault timeout")
        if address_space_limit_bytes != self.contract["child_address_space_limit_bytes"]:
            raise AssertionError("collector changed fault address-space limit")
        record = copy.deepcopy(self.controls[tool][fault])
        if runner["binary_sha256"] != record["executable_before_sha256"]:
            raise AssertionError("collector changed fault runner identity")
        self.calls.append(("run_fault", tool, fault))
        self._fault_offset = offset + 1
        self.process_receipts.append(copy.deepcopy(record))
        return record

    def final_panel_attestation(
        self,
        candidate: dict[str, object],
        runners: dict[str, dict[str, object]],
    ) -> dict[str, object]:
        self.calls.append(("final_panel_attestation", candidate["commit"]))
        value = {
            "candidate": copy.deepcopy(candidate),
            "runners": copy.deepcopy(runners),
            "profile": {
                "file_count": self.contract["profile"]["file_count"],
                "length_framed_sha256": self.contract["profile"][
                    "length_framed_sha256"
                ],
            },
            "process_receipts_sha256": hashlib.sha256(
                json.dumps(
                    self.process_receipts, sort_keys=True, separators=(",", ":")
                ).encode()
            ).hexdigest(),
        }
        if self.final_mutation is not None:
            self.final_mutation(value)
        return value


class FakeLinkageHost:
    def __init__(self, contract: dict[str, object]) -> None:
        self.contract = contract
        self.candidate = {"commit": "a" * 40, "tree": "b" * 40, "dirty": False}
        pure = single_project_evidence(contract)
        self.typokat_cold_build = copy.deepcopy(pure["typokat_cold_build"])
        self.cold_binaries = {
            "typokat": {
                "tool": "typokat",
                "path": self.typokat_cold_build["binary_path"],
                "binary_sha256": self.typokat_cold_build["binary_sha256"],
                "candidate_commit": self.candidate["commit"],
                "candidate_tree": self.candidate["tree"],
                "source_identity_sha256": self.typokat_cold_build[
                    "source_identity_sha256"
                ],
            },
            "tsgo": {
                "tool": "tsgo",
                "path": pure["tools"]["tsgo"]["cold_binary_path"],
                "binary_sha256": pure["tools"]["tsgo"][
                    "authenticated_cold_binary_sha256"
                ],
                "upstream_revision": contract["tsgo"]["upstream_revision"],
            },
        }
        self.single_processes = {
            tool: [self._live_record(row) for row in pure["tools"][tool]["processes"]]
            for tool in ("typokat", "tsgo")
        }
        self.cold_processes = {
            tool: [self._live_record(row) for row in pure["tools"][tool]["cold_processes"]]
            for tool in ("typokat", "tsgo")
        }
        authority = batch.load_authenticated_batch_oracles(contract)
        smoke = raw_panel_evidence(1, 2.0, 100_000, 150_000)
        apply_real_oracle(smoke, authority)
        for sample in smoke["samples"]:
            sample["frames"][0]["shared_product_identity"] = contract["profile"][
                "length_framed_sha256"
            ]
            refresh_responses(sample)
        self.smoke = {
            tool: copy.deepcopy(next(row for row in smoke["samples"] if row["tool"] == tool))
            for tool in ("typokat", "tsgo")
        }
        self.calls: list[tuple[object, ...]] = []

    def _live_record(self, record: dict[str, object]) -> dict[str, object]:
        current = copy.deepcopy(record)
        pid = getattr(self, "_next_pid", 40_000)
        self._next_pid = pid + 1
        current.update(pid=pid, start_ns=pid * 1_000, end_ns=pid * 1_000 + 100)
        return current

    def freeze_candidate(self) -> dict[str, object]:
        self.calls.append(("freeze_candidate",))
        return copy.deepcopy(self.candidate)

    def build_runner(self, tool: str, candidate: dict[str, object]) -> dict[str, object]:
        self.calls.append(("build_runner", tool, candidate["commit"], candidate["tree"]))
        binary = "2" * 64 if tool == "typokat" else "3" * 64
        source = "4" * 64 if tool == "typokat" else "5" * 64
        commit = (
            candidate["commit"]
            if tool == "typokat"
            else self.contract["tsgo"]["upstream_revision"]
        )
        runner = {
            "tool": tool,
            "path": f"/tmp/{tool}-linkage-runner",
            "source_sha256": source,
            "binary_sha256": binary,
            "commit": commit,
        }
        for process in self.single_processes[tool]:
            process["executable_before_sha256"] = binary
            process["executable_after_sha256"] = binary
        self.smoke[tool]["executable_before_sha256"] = binary
        self.smoke[tool]["executable_after_sha256"] = binary
        return runner

    def protocol_info(
        self, tool: str, runner: dict[str, object]
    ) -> dict[str, object]:
        self.calls.append(("protocol_info", tool, runner["binary_sha256"]))
        info = protocol_info(tool)
        info.update(
            source_sha256=runner["source_sha256"],
            binary_sha256=runner["binary_sha256"],
            upstream_revision=runner["commit"],
        )
        return info

    def build_cold_typokat(
        self, candidate: dict[str, object]
    ) -> dict[str, object]:
        self.calls.append(("build_cold_typokat", candidate["commit"], candidate["tree"]))
        return copy.deepcopy(self.cold_binaries["typokat"])

    def authenticate_cold_tsgo(self) -> dict[str, object]:
        self.calls.append(("authenticate_cold_tsgo",))
        return copy.deepcopy(self.cold_binaries["tsgo"])

    def run_batch(
        self,
        tool: str,
        runner: dict[str, object],
        request_stream: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        self.calls.append(("run_batch", tool, timeout_seconds, address_space_limit_bytes))
        record = copy.deepcopy(self.smoke[tool])
        if record["request_stream"] != request_stream:
            raise AssertionError("linkage smoke request differs")
        if record["executable_before_sha256"] != runner["binary_sha256"]:
            raise AssertionError("linkage smoke runner differs")
        return record

    def run_cold(
        self,
        tool: str,
        row: str,
        cold_binary: dict[str, object],
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        self.calls.append(("run_cold", tool, row, timeout_seconds, address_space_limit_bytes))
        expected = self.cold_binaries[tool]
        if cold_binary != expected:
            raise AssertionError("collector changed authenticated cold binary")
        matches = [item for item in self.cold_processes[tool] if item["case_id"] == row]
        return copy.deepcopy(matches[0]) if matches else None

    def run_single_project(
        self,
        tool: str,
        row: str,
        runner: dict[str, object],
        request_stream: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        self.calls.append(
            ("run_single_project", tool, row, timeout_seconds, address_space_limit_bytes)
        )
        matches = [item for item in self.single_processes[tool] if item["case_id"] == row]
        if not matches:
            return None
        record = matches[0]
        if record["request_stream"] != request_stream:
            raise AssertionError("collector changed canonical single-project request")
        if record["executable_before_sha256"] != runner["binary_sha256"]:
            raise AssertionError("collector changed single-project runner")
        return copy.deepcopy(record)

    def final_linkage_attestation(
        self,
        candidate: dict[str, object],
        runners: dict[str, dict[str, object]],
        cold: dict[str, dict[str, object]],
    ) -> dict[str, object]:
        self.calls.append(
            ("final_linkage_attestation", candidate["commit"], candidate["tree"])
        )
        return {
            "candidate": copy.deepcopy(candidate),
            "runners": copy.deepcopy(runners),
            "cold": copy.deepcopy(cold),
            "profile": {
                "file_count": self.contract["profile"]["file_count"],
                "length_framed_sha256": self.contract["profile"][
                    "length_framed_sha256"
                ],
            },
        }


def refresh_responses(sample: dict[str, object]) -> None:
    for frame in sample["frames"]:
        response = {
            key: value
            for key, value in frame.items()
            if key not in {"raw_request", "raw_response", "raw_response_sha256"}
        }
        frame["raw_response"] = json.dumps(
            response, sort_keys=True, separators=(",", ":")
        )
        frame["raw_response_sha256"] = hashlib.sha256(
            frame["raw_response"].encode()
        ).hexdigest()
    refresh_sample_streams(sample)


class FakePostPanelHost:
    NORMAL_BINARY_SHA256 = "2" * 64
    FEATURE_BINARY_SHA256 = "6" * 64
    SOURCE_SHA256 = "4" * 64

    def __init__(self, contract: dict[str, object]) -> None:
        self.contract = contract
        self.candidate = {"commit": "a" * 40, "tree": "b" * 40, "dirty": False}
        self.authority = batch.load_authenticated_batch_oracles(contract)
        self.calls: list[tuple[object, ...]] = []
        self.final_attestation_mutation = None
        self.normal_runner = self._runner(
            "/tmp/typokat-post-panel-runner", self.NORMAL_BINARY_SHA256
        )
        self.feature_runner = self._runner(
            "/tmp/typokat-allocation-runner", self.FEATURE_BINARY_SHA256
        )

        first = self.authority["frames"][0]
        self.first_request_stream = self._request_stream([first])
        first_semantic = {
            key: copy.deepcopy(value)
            for key, value in self.authority["typokat"][0].items()
            if key != "case_id"
        }
        self.concurrency = concurrency_evidence()
        for count in self.contract["concurrency"]["session_counts"]:
            row = self.concurrency[str(count)]
            for session in row["sessions"]:
                session["semantic"] = copy.deepcopy(first_semantic)
            row.update(
                executable_before_sha256=self.NORMAL_BINARY_SHA256,
                executable_after_sha256=self.NORMAL_BINARY_SHA256,
                request_stream=self.first_request_stream,
                request_stream_sha256=hashlib.sha256(
                    self.first_request_stream.encode()
                ).hexdigest(),
                pid=50_000 + count,
                start_ns=count * 1_000,
                end_ns=count * 1_000 + 100,
                returncode=0,
                tool="typokat",
                stderr="",
                address_space_limit_bytes=contract[
                    "child_address_space_limit_bytes"
                ],
            )
            refresh_concurrency_raw(row)

        allocation_panel = raw_panel_evidence(687, 1.0, 100_000, 100_000)
        apply_real_oracle(allocation_panel, self.authority)
        allocation_panel["derived"]["typokat_accepted_progress"] = (
            batch.recompute_typokat_accepted_progress(
                allocation_panel,
                batch.oracle_panel(self.authority, 687),
                contract,
            )
        )
        self.allocation_record = allocation_panel["samples"][0]
        for frame in self.allocation_record["frames"]:
            frame["shared_product_identity"] = contract["profile"][
                "length_framed_sha256"
            ]
        self.allocation_record.update(
            timed=False,
            executable_before_sha256=self.FEATURE_BINARY_SHA256,
            executable_after_sha256=self.FEATURE_BINARY_SHA256,
            allocation_receipt={
                "tool": "typokat",
                "timed": False,
                "instrumentation": "counting-allocator",
                "allocation_count": 123_456,
                "binary_sha256": self.FEATURE_BINARY_SHA256,
            },
        )
        refresh_responses(self.allocation_record)
        allocation_line = json.dumps(
            self.allocation_record["allocation_receipt"],
            sort_keys=True,
            separators=(",", ":"),
        )
        self.allocation_record["stdout"] = (
            self.allocation_record["response_stream"] + allocation_line + "\n"
        )
        self.allocation_request_stream = self._request_stream(self.authority["frames"])
        if self.allocation_record["request_stream"] != self.allocation_request_stream:
            raise AssertionError("allocation fixture lost authenticated request bytes")

    def _runner(self, path: str, binary_sha256: str) -> dict[str, object]:
        return {
            "tool": "typokat",
            "path": path,
            "source_sha256": self.SOURCE_SHA256,
            "binary_sha256": binary_sha256,
            "commit": self.candidate["commit"],
        }

    @staticmethod
    def _request_stream(frames: list[dict[str, object]]) -> str:
        return "".join(
            json.dumps(
                {
                    "schema": 1,
                    "case_id": frame["case_id"],
                    "source": frame["source"],
                },
                sort_keys=True,
                separators=(",", ":"),
            )
            + "\n"
            for frame in frames
        )

    def freeze_candidate(self) -> dict[str, object]:
        self.calls.append(("freeze_candidate",))
        return copy.deepcopy(self.candidate)

    def build_runner(
        self, tool: str, candidate: dict[str, object]
    ) -> dict[str, object]:
        if tool != "typokat" or candidate != self.candidate:
            raise AssertionError("collector changed the normal runner build identity")
        self.calls.append(
            ("build_runner", tool, candidate["commit"], candidate["tree"])
        )
        return copy.deepcopy(self.normal_runner)

    def build_allocation_runner(
        self, candidate: dict[str, object]
    ) -> dict[str, object]:
        if candidate != self.candidate:
            raise AssertionError("collector changed the feature runner build identity")
        self.calls.append(
            ("build_allocation_runner", candidate["commit"], candidate["tree"])
        )
        return copy.deepcopy(self.feature_runner)

    def protocol_info(
        self, tool: str, runner: dict[str, object]
    ) -> dict[str, object]:
        if tool != "typokat" or runner not in (
            self.normal_runner,
            self.feature_runner,
        ):
            raise AssertionError("collector requested protocol from an unknown runner")
        self.calls.append(("protocol_info", runner["path"], runner["binary_sha256"]))
        info = protocol_info("typokat")
        info.update(
            source_sha256=runner["source_sha256"],
            binary_sha256=runner["binary_sha256"],
            upstream_revision=runner["commit"],
        )
        return info

    def run_concurrency(
        self,
        runner: dict[str, object],
        request_stream: str,
        count: int,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        if runner != self.normal_runner:
            raise AssertionError("collector changed the concurrency runner")
        if request_stream != self.first_request_stream:
            raise AssertionError("collector changed authenticated first-frame source bytes")
        if count not in self.contract["concurrency"]["session_counts"]:
            raise AssertionError("collector requested an unofficial concurrency count")
        if timeout_seconds != 10:
            raise AssertionError("collector changed the concurrency timeout")
        if address_space_limit_bytes != self.contract[
            "child_address_space_limit_bytes"
        ]:
            raise AssertionError("collector changed the child address-space limit")
        self.calls.append(
            (
                "run_concurrency",
                count,
                timeout_seconds,
                address_space_limit_bytes,
            )
        )
        return copy.deepcopy(self.concurrency[str(count)])

    def run_allocation_probe(
        self,
        runner: dict[str, object],
        request_stream: str,
        *,
        timeout_seconds: int,
        address_space_limit_bytes: int,
    ) -> dict[str, object]:
        if runner != self.feature_runner:
            raise AssertionError("collector changed the allocation feature runner")
        if request_stream != self.allocation_request_stream:
            raise AssertionError("collector changed authenticated 687-frame source bytes")
        if timeout_seconds != self.contract["sampling"]["panel_timeout_seconds"][-1]:
            raise AssertionError("collector changed the allocation timeout")
        if address_space_limit_bytes != self.contract[
            "child_address_space_limit_bytes"
        ]:
            raise AssertionError("collector changed the child address-space limit")
        self.calls.append(
            (
                "run_allocation_probe",
                687,
                timeout_seconds,
                address_space_limit_bytes,
            )
        )
        return copy.deepcopy(self.allocation_record)

    def final_typokat_attestation(
        self,
        candidate: dict[str, object],
        runner: dict[str, object],
    ) -> dict[str, object]:
        if candidate != self.candidate or runner not in (
            self.normal_runner,
            self.feature_runner,
        ):
            raise AssertionError("collector changed the final attestation identity")
        self.calls.append(
            (
                "final_typokat_attestation",
                candidate["commit"],
                runner["binary_sha256"],
            )
        )
        value = {
            "candidate": copy.deepcopy(candidate),
            "runner": copy.deepcopy(runner),
        }
        if self.final_attestation_mutation is not None:
            self.final_attestation_mutation(value)
        return value


class SessionBatchLiveCollectorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = batch.verify_contract()

    def test_public_collector_cannot_accept_supplied_evidence_or_process_callbacks(self) -> None:
        signature = inspect.signature(collect.collect_live_preflight)
        self.assertEqual(list(signature.parameters), ["output"])
        self.assertEqual(signature.parameters["output"].kind, inspect.Parameter.KEYWORD_ONLY)

    def test_system_host_preserves_stdout_and_canonicalizes_frame_projection(self) -> None:
        response = '{"ordinal":0,"case_id":"case"}\n'
        measurement = collect._ProcessMeasurement(
            pid=123,
            start_ns=10,
            end_ns=20,
            returncode=0,
            stdout=response,
            stderr="",
            process_group_empty=True,
            time_report="Maximum resident set size (kbytes): 1\n",
            rss_kib=1,
        )
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "runner"
            executable.write_bytes(b"runner")
            runner = {
                "path": str(executable),
                "binary_sha256": hashlib.sha256(b"runner").hexdigest(),
            }
            host = object.__new__(collect.SystemLiveHost)
            host._run_runner = mock.Mock(return_value=measurement)
            record = host.run_batch(
                "typokat",
                runner,
                '{"case_id":"case","schema":1,"source":""}\n',
                timeout_seconds=10,
                address_space_limit_bytes=1 << 30,
            )

        self.assertEqual(record["stdout"], response)
        self.assertEqual(record["response_stream"], response)
        self.assertEqual(
            record["frames"][0]["raw_response"],
            '{"case_id":"case","ordinal":0}',
        )
        self.assertEqual(
            record["frames"][0]["raw_response_sha256"],
            hashlib.sha256(b'{"case_id":"case","ordinal":0}').hexdigest(),
        )

    def test_system_allocation_host_accepts_noncanonical_frame_and_preserves_raw_streams(
        self,
    ) -> None:
        authority = batch.load_authenticated_batch_oracles(self.contract)
        fixture = raw_panel_evidence(1, 1.0, 100_000, 100_000)
        apply_real_oracle(fixture, authority)
        sample = next(
            row for row in fixture["samples"] if row["tool"] == "typokat"
        )
        sample["frames"][0]["shared_product_identity"] = self.contract["profile"][
            "length_framed_sha256"
        ]
        refresh_responses(sample)
        frame = json.loads(sample["frames"][0]["raw_response"])
        canonical_frame = json.dumps(
            frame, sort_keys=True, separators=(",", ":")
        )
        noncanonical_frame = json.dumps(
            dict(reversed(tuple(frame.items()))), separators=(",", ":")
        )
        self.assertNotEqual(noncanonical_frame, canonical_frame)

        executable_bytes = b"feature allocation runner"
        binary_sha256 = hashlib.sha256(executable_bytes).hexdigest()
        receipt = {
            "allocation_count": 123,
            "binary_sha256": binary_sha256,
            "instrumentation": "counting-allocator",
            "timed": False,
            "tool": "typokat",
        }
        canonical_receipt = json.dumps(
            receipt, sort_keys=True, separators=(",", ":")
        )
        response_stream = noncanonical_frame + "\n"
        stdout = response_stream + canonical_receipt + "\n"
        measurement = collect._ProcessMeasurement(
            pid=321,
            start_ns=10,
            end_ns=20,
            returncode=0,
            stdout=stdout,
            stderr="",
            process_group_empty=True,
            time_report="Maximum resident set size (kbytes): 2\n",
            rss_kib=2,
        )

        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "allocation-runner"
            executable.write_bytes(executable_bytes)
            runner = {
                "path": str(executable),
                "binary_sha256": binary_sha256,
            }
            host = object.__new__(collect.SystemLiveHost)
            host.contract = self.contract
            host._run_runner = mock.Mock(return_value=measurement)
            record = host.run_allocation_probe(
                runner,
                sample["request_stream"],
                timeout_seconds=600,
                address_space_limit_bytes=1 << 30,
            )

        host._run_runner.assert_called_once_with(
            "typokat",
            runner,
            ["--allocation-probe"],
            sample["request_stream"],
            timeout_seconds=600,
            address_space_limit_bytes=1 << 30,
            measured=True,
        )
        self.assertEqual(record["stdout"], stdout)
        self.assertEqual(record["response_stream"], response_stream)
        self.assertEqual(
            record["response_stream_sha256"],
            hashlib.sha256(response_stream.encode()).hexdigest(),
        )
        self.assertEqual(record["frames"][0]["raw_response"], canonical_frame)
        self.assertEqual(
            record["frames"][0]["raw_response_sha256"],
            hashlib.sha256(canonical_frame.encode()).hexdigest(),
        )
        self.assertEqual(record["allocation_receipt"], receipt)

    def test_system_concurrency_host_bounds_glibc_arenas_and_records_environment(
        self,
    ) -> None:
        concurrency = concurrency_evidence()["1"]
        request_stream = json.dumps(
            {"case_id": "case", "schema": 1, "source": "const value = 1;"},
            sort_keys=True,
            separators=(",", ":"),
        ) + "\n"
        measurement = collect._ProcessMeasurement(
            pid=456,
            start_ns=10,
            end_ns=20,
            returncode=0,
            stdout=concurrency["stdout"],
            stderr="",
            process_group_empty=True,
            time_report="Maximum resident set size (kbytes): 189528\n",
            rss_kib=189_528,
        )
        effective_environment = {
            "LANG": "C",
            "LC_ALL": "C",
            "TZ": "UTC",
            "MALLOC_ARENA_MAX": "2",
        }

        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "concurrency-runner"
            executable_bytes = b"concurrency runner"
            executable.write_bytes(executable_bytes)
            runner = {
                "path": str(executable),
                "binary_sha256": hashlib.sha256(executable_bytes).hexdigest(),
            }
            host = object.__new__(collect.SystemLiveHost)
            host.contract = self.contract
            host._run_runner = mock.Mock(return_value=measurement)
            record = host.run_concurrency(
                runner,
                request_stream,
                1,
                timeout_seconds=10,
                address_space_limit_bytes=1 << 30,
            )

        host._run_runner.assert_called_once_with(
            "typokat",
            runner,
            ["--concurrency", "1"],
            request_stream,
            timeout_seconds=10,
            address_space_limit_bytes=1 << 30,
            measured=True,
            environment_overrides={"MALLOC_ARENA_MAX": "2"},
        )
        self.assertEqual(record["environment"], effective_environment)

    def test_tsgo_runner_receives_authenticated_staged_library_directory(self) -> None:
        arguments = ["--batch", "--protocol-version", "1"]
        stdin = '{"case_id":"case"}\n'
        measurement = mock.sentinel.measurement

        with tempfile.TemporaryDirectory() as directory:
            live_root = Path(directory)
            host = object.__new__(collect.SystemLiveHost)
            host.contract = self.contract
            host.live_root = live_root
            host.collection_deadline = None
            tsgo_runner = live_root / "tsgo-runner"
            typokat_runner = live_root / "typokat-runner"

            with (
                mock.patch.object(collect, "ROOT", live_root),
                mock.patch.object(
                    collect, "_supervise", return_value=measurement
                ) as supervise,
            ):
                staged_library_directory = (
                    collect.ROOT
                    / self.contract["tsgo"]["cold_binary_relative_path"]
                ).resolve().parent
                tsgo_result = host._run_runner(
                    "tsgo",
                    {"path": str(tsgo_runner)},
                    arguments,
                    stdin,
                    timeout_seconds=11,
                    address_space_limit_bytes=123,
                    measured=True,
                )
                typokat_result = host._run_runner(
                    "typokat",
                    {"path": str(typokat_runner)},
                    arguments,
                    stdin,
                    timeout_seconds=11,
                    address_space_limit_bytes=123,
                    measured=True,
                )

            self.assertTrue(staged_library_directory.is_absolute())
            self.assertIs(tsgo_result, measurement)
            self.assertIs(typokat_result, measurement)
            self.assertEqual(
                supervise.call_args_list,
                [
                    mock.call(
                        [
                            str(tsgo_runner),
                            "--lib-dir",
                            str(staged_library_directory),
                            *arguments,
                        ],
                        stdin,
                        cwd=live_root,
                        environment={
                            "LANG": "C",
                            "LC_ALL": "C",
                            "TZ": "UTC",
                            "GOMAXPROCS": "1",
                        },
                        timeout_seconds=11,
                        address_space_limit_bytes=123,
                        measured=True,
                    ),
                    mock.call(
                        [str(typokat_runner), *arguments],
                        stdin,
                        cwd=live_root,
                        environment={"LANG": "C", "LC_ALL": "C", "TZ": "UTC"},
                        timeout_seconds=11,
                        address_space_limit_bytes=123,
                        measured=True,
                    ),
                ],
            )

    def test_typokat_build_cannot_load_home_ancestor_cargo_configuration(self) -> None:
        host = collect.SystemLiveHost(self.contract)
        candidate = {"commit": "a" * 40, "tree": "b" * 40, "dirty": False}
        captured: dict[str, object] = {}

        def build(
            argv: list[str], cwd: Path, environment: dict[str, str]
        ) -> None:
            captured.update(argv=argv, cwd=cwd, environment=environment)
            binary = (
                Path(environment["CARGO_TARGET_DIR"])
                / "release"
                / self.contract["typokat"]["runner_binary"]
            )
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"runner")

        try:
            self.assertNotIn(
                host.system_home,
                (host.live_root, *host.live_root.parents),
            )
            host.freeze_candidate = mock.Mock(return_value=candidate)
            host._run_build = build
            with mock.patch.object(
                host,
                "_validate_cargo_configuration",
                wraps=host._validate_cargo_configuration,
            ) as validate:
                host._build_typokat(candidate)
        finally:
            shutil.rmtree(host.live_root)

        self.assertEqual(captured["cwd"], host.live_root)
        self.assertEqual(
            captured["argv"],
            [
                *self.contract["typokat"]["build_argv"],
                "--manifest-path",
                str(collect.ROOT / "Cargo.toml"),
            ],
        )
        environment = captured["environment"]
        target = Path(environment["CARGO_TARGET_DIR"])
        cargo_home = Path(environment["CARGO_HOME"])
        build_home = Path(environment["HOME"])
        self.assertEqual(target, host.live_root / "rust-target")
        self.assertTrue(target.is_relative_to(host.live_root))
        self.assertEqual(cargo_home, host.live_root / "cargo-home")
        self.assertEqual(build_home, host.live_root / "rust-home")
        self.assertEqual(
            environment["RUSTUP_TOOLCHAIN"],
            "1.95.0-x86_64-unknown-linux-gnu",
        )
        self.assertEqual(environment["CARGO_BUILD_JOBS"], "1")
        self.assertEqual(environment["TYPOKAT_CANDIDATE_COMMIT"], candidate["commit"])
        self.assertEqual(environment["RUSTC_WRAPPER"], "")
        self.assertEqual(environment["RUSTC_WORKSPACE_WRAPPER"], "")
        self.assertEqual(environment["RUSTFLAGS"], "")
        self.assertEqual(environment["CARGO_ENCODED_RUSTFLAGS"], "")
        self.assertEqual(
            validate.call_args_list,
            [
                mock.call(host.live_root, cargo_home),
                mock.call(host.live_root, cargo_home),
            ],
        )

    def test_opt_in_rust_build_cache_uses_deterministic_ignored_target(self) -> None:
        candidate = {"commit": "a" * 40, "tree": "b" * 40, "dirty": False}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with mock.patch.object(collect, "ROOT", root):
                host = collect.SystemLiveHost(
                    self.contract,
                    persistent_rust_build_cache=True,
                )
                try:
                    target, cargo_home, environment = host._rust_build_context(candidate)
                finally:
                    shutil.rmtree(host.live_root)

            cache_root = root / "target/wu6/session-batch-live"
            self.assertEqual(target, cache_root)
            self.assertFalse(target.is_relative_to(host.live_root))
            self.assertEqual(Path(environment["CARGO_TARGET_DIR"]), target)
            self.assertEqual(cargo_home, cache_root / "cargo-home")
            self.assertEqual(Path(environment["CARGO_HOME"]), cargo_home)
            for path in (target, cargo_home):
                self.assertTrue(path.is_dir())
                self.assertFalse(path.is_symlink())
            self.assertEqual(Path(environment["HOME"]), host.live_root / "rust-home")
            self.assertEqual(host.git_home, host.live_root / "git-home")
            self.assertEqual(
                environment["RUSTUP_TOOLCHAIN"],
                "1.95.0-x86_64-unknown-linux-gnu",
            )
            self.assertEqual(environment["CARGO_BUILD_JOBS"], "1")
            self.assertEqual(
                environment["TYPOKAT_CANDIDATE_COMMIT"], candidate["commit"]
            )
            self.assertEqual(environment["RUSTC_WRAPPER"], "")
            self.assertEqual(environment["RUSTC_WORKSPACE_WRAPPER"], "")
            self.assertEqual(environment["RUSTFLAGS"], "")
            self.assertEqual(environment["CARGO_ENCODED_RUSTFLAGS"], "")

    def test_persistent_rust_build_returns_private_runner_copy(self) -> None:
        candidate = {"commit": "a" * 40, "tree": "b" * 40, "dirty": False}
        built_runner: Path | None = None

        def build(
            argv: list[str], cwd: Path, environment: dict[str, str]
        ) -> None:
            nonlocal built_runner
            built_runner = (
                Path(environment["CARGO_TARGET_DIR"])
                / "release"
                / self.contract["typokat"]["runner_binary"]
            )
            built_runner.parent.mkdir(parents=True, exist_ok=True)
            built_runner.write_bytes(b"cached-runner")

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / self.contract["typokat"]["runner_source_relative_path"]
            source.parent.mkdir(parents=True)
            source.write_bytes(b"runner-source")
            with mock.patch.object(collect, "ROOT", root):
                host = collect.SystemLiveHost(
                    self.contract,
                    persistent_rust_build_cache=True,
                )
                try:
                    host.freeze_candidate = mock.Mock(return_value=candidate)
                    host._run_build = build
                    with mock.patch.object(
                        host,
                        "_validate_cargo_configuration",
                        wraps=host._validate_cargo_configuration,
                    ) as validate:
                        runner = host._build_typokat(candidate)
                    self.assertIsNotNone(built_runner)
                    private_runner = Path(runner["path"])
                    self.assertTrue(private_runner.is_relative_to(host.live_root))
                    self.assertNotEqual(private_runner, built_runner)
                    self.assertFalse(private_runner.is_symlink())
                    self.assertTrue(private_runner.is_file())
                    self.assertNotEqual(
                        private_runner.stat().st_ino,
                        built_runner.stat().st_ino,
                    )
                    self.assertEqual(private_runner.read_bytes(), b"cached-runner")
                    built_runner.write_bytes(b"changed-cache")
                    self.assertEqual(private_runner.read_bytes(), b"cached-runner")
                    self.assertEqual(
                        runner["source_sha256"],
                        hashlib.sha256(b"runner-source").hexdigest(),
                    )
                    self.assertEqual(
                        runner["binary_sha256"],
                        hashlib.sha256(b"cached-runner").hexdigest(),
                    )
                    self.assertEqual(
                        validate.call_args_list,
                        [
                            mock.call(
                                host.live_root,
                                root / "target/wu6/session-batch-live/cargo-home",
                            ),
                            mock.call(
                                host.live_root,
                                root / "target/wu6/session-batch-live/cargo-home",
                            ),
                        ],
                    )
                finally:
                    shutil.rmtree(host.live_root)

    def test_persistent_cargo_home_is_validated_around_cold_rust_build(self) -> None:
        candidate = {"commit": "a" * 40, "tree": "b" * 40, "dirty": False}

        def build(
            argv: list[str], cwd: Path, environment: dict[str, str]
        ) -> None:
            binary = Path(environment["CARGO_TARGET_DIR"]) / "release/typokat"
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_bytes(b"cold-typokat")

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with mock.patch.object(collect, "ROOT", root):
                host = collect.SystemLiveHost(
                    self.contract,
                    persistent_rust_build_cache=True,
                )
                try:
                    host.freeze_candidate = mock.Mock(return_value=candidate)
                    host._run_build = build
                    with mock.patch.object(
                        host,
                        "_validate_cargo_configuration",
                        wraps=host._validate_cargo_configuration,
                    ) as validate:
                        host.build_cold_typokat(candidate)
                    cargo_home = root / "target/wu6/session-batch-live/cargo-home"
                    self.assertEqual(
                        validate.call_args_list,
                        [
                            mock.call(host.live_root, cargo_home),
                            mock.call(host.live_root, cargo_home),
                        ],
                    )
                finally:
                    shutil.rmtree(host.live_root)

    def test_default_go_build_environment_is_private_to_live_root(self) -> None:
        host = collect.SystemLiveHost(self.contract)
        try:
            environment = host._go_build_environment()
            expected = {
                "HOME": host.live_root / "go-home",
                "GOCACHE": host.live_root / "go-cache",
                "GOMODCACHE": host.live_root / "go-mod-cache",
                "GOPATH": host.live_root / "go-path",
            }
            for name, path in expected.items():
                self.assertEqual(Path(environment[name]), path)
                self.assertTrue(path.is_relative_to(host.live_root))
                self.assertTrue(path.is_dir())
                self.assertFalse(path.is_symlink())
            self.assertEqual(host.git_home, host.live_root / "git-home")
            self.assertEqual(environment["GOENV"], "off")
            self.assertEqual(environment["GOFLAGS"], "")
            self.assertEqual(environment["GOTOOLCHAIN"], "go1.26.0")
            self.assertEqual(environment["GOMAXPROCS"], "1")
        finally:
            shutil.rmtree(host.live_root)

    def test_opt_in_build_cache_uses_persistent_go_paths_and_private_home(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with mock.patch.object(collect, "ROOT", root):
                host = collect.SystemLiveHost(
                    self.contract,
                    persistent_rust_build_cache=True,
                )
                try:
                    environment = host._go_build_environment()
                    cache_root = root / "target/wu6/session-batch-live"
                    expected = {
                        "GOCACHE": cache_root / "go-cache",
                        "GOMODCACHE": cache_root / "go-mod-cache",
                        "GOPATH": cache_root / "go-path",
                    }
                    for name, path in expected.items():
                        self.assertEqual(Path(environment[name]), path)
                        self.assertFalse(path.is_relative_to(host.live_root))
                        self.assertTrue(path.is_dir())
                        self.assertFalse(path.is_symlink())
                    self.assertEqual(
                        Path(environment["HOME"]), host.live_root / "go-home"
                    )
                    self.assertEqual(host.git_home, host.live_root / "git-home")
                    self.assertEqual(environment["GOENV"], "off")
                    self.assertEqual(environment["GOFLAGS"], "")
                    self.assertEqual(environment["GOTOOLCHAIN"], "go1.26.0")
                    self.assertEqual(environment["GOMAXPROCS"], "1")
                finally:
                    shutil.rmtree(host.live_root)

    def test_cargo_configuration_discovery_fails_on_any_external_config(self) -> None:
        host = object.__new__(collect.SystemLiveHost)
        with tempfile.TemporaryDirectory(dir="/tmp") as directory:
            root = Path(directory)
            cwd = root / "run"
            cargo_home = root / "cargo-home"
            cwd.mkdir()
            cargo_home.mkdir()
            host._validate_cargo_configuration(cwd, cargo_home)

            ancestor_config = root / ".cargo" / "config.toml"
            ancestor_config.parent.mkdir()
            ancestor_config.write_text("[build]\nrustc-wrapper = 'other'\n")
            with self.assertRaisesRegex(
                batch.BatchContractError,
                "Cargo configuration",
            ):
                host._validate_cargo_configuration(cwd, cargo_home)
            ancestor_config.unlink()

            for name in ("config", "config.toml"):
                cargo_home_config = cargo_home / name
                cargo_home_config.write_text("[build]\njobs = 99\n")
                with self.subTest(name=name), self.assertRaisesRegex(
                    batch.BatchContractError,
                    "Cargo configuration",
                ):
                    host._validate_cargo_configuration(cwd, cargo_home)
                cargo_home_config.unlink()

    def test_live_preflight_owns_build_protocol_process_and_fault_sequence(self) -> None:
        host = FakeLiveHost(self.contract)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "preflight.json"
            with mock.patch.object(collect, "SystemLiveHost", return_value=host) as constructor:
                result = collect.collect_live_preflight(output=output)
                with self.assertRaisesRegex(batch.BatchContractError, "exists"):
                    collect.collect_live_preflight(output=output)
                constructor.assert_called_once_with(self.contract)
                self.assertEqual(
                    output.read_bytes(),
                    json.dumps(
                        result, sort_keys=True, separators=(",", ":")
                    ).encode()
                    + b"\n",
                )

        self.assertEqual(result["verdict"], "PREFLIGHT-PASS")
        self.assertNotEqual(result["verdict"], "GO")
        self.assertRegex(result["evidence_sha256"], r"^[0-9a-f]{64}$")
        self.assertEqual(
            host.calls,
            [
                ("freeze_candidate",),
                ("build_runner", "typokat", "a" * 40),
                ("build_runner", "tsgo", "a" * 40),
                ("protocol_info", "typokat", "2" * 64),
                ("protocol_info", "tsgo", "2" * 64),
                ("run_batch", "typokat", 10, 1 << 30),
                ("run_batch", "tsgo", 10, 1 << 30),
                ("run_fault", "typokat", "reuse-session", 10, 1 << 30),
                ("run_fault", "typokat", "retain-cache", 10, 1 << 30),
                ("run_fault", "tsgo", "reuse-checker", 10, 1 << 30),
                ("run_fault", "tsgo", "retain-frame", 10, 1 << 30),
                ("final_attestation", "a" * 40),
            ],
        )
        evidence = result["evidence"]
        self.assertEqual(
            set(evidence),
            {
                "candidate",
                "negative_controls",
                "projects",
                "protocols",
                "final_attestation",
                "runners",
                "samples",
                "schema",
            },
        )
        self.assertEqual(evidence["schema"], 1)
        self.assertEqual(evidence["candidate"], host.candidate)
        self.assertEqual(evidence["projects"], 1)
        self.assertEqual(len(evidence["samples"]), 2)
        self.assertEqual(set(evidence["negative_controls"]), {"typokat", "tsgo"})
        self.assertEqual(set(evidence["runners"]), {"typokat", "tsgo"})
        self.assertEqual(set(evidence["protocols"]), {"typokat", "tsgo"})
        self.assertEqual(
            evidence["final_attestation"]["candidate"], host.candidate
        )
        self.assertEqual(
            result["evidence_sha256"],
            hashlib.sha256(
                json.dumps(
                    evidence, sort_keys=True, separators=(",", ":")
                ).encode()
            ).hexdigest(),
        )

    def test_preflight_requires_a_clean_frozen_candidate_before_building(self) -> None:
        for mutation in (
            lambda value: value.update(dirty=True),
            lambda value: value.update(commit="candidate"),
            lambda value: value.update(commit="g" * 40),
            lambda value: value.update(tree="0" * 39),
            lambda value: value.update(tree="z" * 40),
            lambda value: value.update(dirty=None),
            lambda value: value.pop("dirty"),
            lambda value: value.pop("commit"),
            lambda value: value.pop("tree"),
        ):
            host = FakeLiveHost(self.contract)
            mutation(host.candidate)
            with self.assertRaisesRegex(batch.BatchContractError, "candidate"):
                collect._collect_live_preflight(host, self.contract)
            self.assertEqual(host.calls, [("freeze_candidate",)])

    def test_preflight_fails_closed_before_verdict_on_forged_protocol(self) -> None:
        for target_tool in ("typokat", "tsgo"):
            for field, value in (
                ("binary_sha256", "0" * 64),
                ("source_sha256", "0" * 64),
                ("upstream_revision", "wrong"),
            ):
                host = FakeLiveHost(self.contract)
                original = host.protocol_info

                def forged(
                    tool: str,
                    runner: dict[str, object],
                    *,
                    target_tool: str = target_tool,
                    field: str = field,
                    value: str = value,
                ) -> dict[str, object]:
                    protocol = original(tool, runner)
                    if tool == target_tool:
                        protocol[field] = value
                    return protocol

                host.protocol_info = forged
                with self.assertRaises(batch.BatchContractError):
                    collect._collect_live_preflight(host, self.contract)
                self.assertFalse(any(call[0] == "run_batch" for call in host.calls))

    def test_preflight_fails_closed_on_semantic_or_fault_control_drift(self) -> None:
        semantic = FakeLiveHost(self.contract)
        semantic.samples["typokat"]["frames"][0]["semantic"]["diagnostics"].append(
            "999:9999"
        )
        refresh_responses(semantic.samples["typokat"])
        with self.assertRaises(batch.BatchContractError):
            collect._collect_live_preflight(semantic, self.contract)

        fault = FakeLiveHost(self.contract)
        fault.controls["typokat"]["retain-cache"]["fault_hook_activations"] = 0
        with self.assertRaisesRegex(batch.BatchContractError, "activation"):
            collect._collect_live_preflight(fault, self.contract)

        fault_stderr = FakeLiveHost(self.contract)
        fault_stderr.controls["tsgo"]["reuse-checker"]["stderr"] = "forged warning\n"
        with self.assertRaisesRegex(batch.BatchContractError, "stderr"):
            collect._collect_live_preflight(fault_stderr, self.contract)

        for tool, field, value, message in (
            ("typokat", "session_weak_alive_after", True, "weak handle"),
            ("typokat", "cache_weak_alive_after", True, "weak handle"),
            ("tsgo", "retained_frame_handles", 1, "retained frame"),
        ):
            lifecycle = FakeLiveHost(self.contract)
            lifecycle.samples[tool]["frames"][0][field] = value
            refresh_responses(lifecycle.samples[tool])
            with self.assertRaisesRegex(batch.BatchContractError, message):
                collect._collect_live_preflight(lifecycle, self.contract)

        for tool in ("typokat", "tsgo"):
            profile = FakeLiveHost(self.contract)
            profile.samples[tool]["frames"][0]["shared_product_identity"] = "0" * 64
            refresh_responses(profile.samples[tool])
            with self.assertRaisesRegex(batch.BatchContractError, "shared product identity"):
                collect._collect_live_preflight(profile, self.contract)

    def test_preflight_rechecks_candidate_and_runners_before_publish(self) -> None:
        for mutation, message in (
            (
                lambda attestation: attestation["candidate"].update(commit="c" * 40),
                "final candidate identity",
            ),
            (
                lambda attestation: attestation["candidate"].update(tree="c" * 40),
                "final candidate identity",
            ),
            (
                lambda attestation: attestation["candidate"].update(dirty=True),
                "final candidate identity",
            ),
            (
                lambda attestation: attestation["runners"]["typokat"].update(
                    source_sha256="9" * 64
                ),
                "final runner identity",
            ),
            (
                lambda attestation: attestation["runners"]["typokat"].update(
                    binary_sha256="9" * 64
                ),
                "final runner identity",
            ),
            (
                lambda attestation: attestation["runners"]["tsgo"].update(
                    source_sha256="9" * 64
                ),
                "final runner identity",
            ),
            (
                lambda attestation: attestation["runners"]["tsgo"].update(
                    binary_sha256="9" * 64
                ),
                "final runner identity",
            ),
        ):
            host = FakeLiveHost(self.contract)
            original = host.final_attestation

            def mismatched(
                candidate: dict[str, object],
                runners: dict[str, dict[str, object]],
                *,
                mutation=mutation,
            ) -> dict[str, object]:
                attestation = original(candidate, runners)
                mutation(attestation)
                return attestation

            host.final_attestation = mismatched
            with self.assertRaisesRegex(batch.BatchContractError, message):
                collect._collect_live_preflight(host, self.contract)

    def test_process_records_are_linked_to_the_built_runner_identity(self) -> None:
        sample = FakeLiveHost(self.contract)
        original_batch = sample.run_batch

        def forged_batch(
            tool: str,
            runner: dict[str, object],
            request_stream: str,
            **kwargs,
        ) -> dict[str, object]:
            record = original_batch(tool, runner, request_stream, **kwargs)
            if tool == "typokat":
                record.update(
                    executable_before_sha256="9" * 64,
                    executable_after_sha256="9" * 64,
                )
            return record

        sample.run_batch = forged_batch
        with self.assertRaisesRegex(batch.BatchContractError, "built runner identity"):
            collect._collect_live_preflight(sample, self.contract)

        controls = FakeLiveHost(self.contract)
        original_fault = controls.run_fault

        def forged_fault(
            tool: str,
            runner: dict[str, object],
            fault: str,
            **kwargs,
        ) -> dict[str, object]:
            record = original_fault(tool, runner, fault, **kwargs)
            if tool == "typokat":
                record.update(
                    executable_before_sha256="9" * 64,
                    executable_after_sha256="9" * 64,
                )
            return record

        controls.run_fault = forged_fault
        with self.assertRaisesRegex(batch.BatchContractError, "built runner identity"):
            collect._collect_live_preflight(controls, self.contract)

    def test_preflight_validates_internal_route_and_phase_boundaries(self) -> None:
        host = FakeLiveHost(self.contract)
        sample = host.samples["typokat"]
        frame = sample["frames"][0]
        frame["phases"]["parse_prepare"]["start_ns"] += 1
        refresh_responses(sample)
        with self.assertRaisesRegex(batch.BatchContractError, "phase boundary"):
            collect._collect_live_preflight(host, self.contract)

        initialization = FakeLiveHost(self.contract)
        initialization.samples["typokat"]["frames"][0]["product_initializations"] = 0
        refresh_responses(initialization.samples["typokat"])
        with self.assertRaisesRegex(batch.BatchContractError, "initialization"):
            collect._collect_live_preflight(initialization, self.contract)


class SessionBatchLivePanelCollectorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = batch.verify_contract()

    def test_public_live_panel_collector_is_non_injectable_atomic_and_progressive(
        self,
    ) -> None:
        signature = inspect.signature(collect.collect_live_panels)
        self.assertEqual(list(signature.parameters), ["output"])
        self.assertEqual(
            signature.parameters["output"].kind,
            inspect.Parameter.KEYWORD_ONLY,
        )

        host = FakePanelHost(self.contract)
        timing_receipt = {
            **host.timing_boundary,
            "oracle_authority_sha256": hashlib.sha256(
                json.dumps(
                    host.authority, sort_keys=True, separators=(",", ":")
                ).encode()
            ).hexdigest(),
            "control_receipts_sha256": hashlib.sha256(
                json.dumps(
                    host.controls, sort_keys=True, separators=(",", ":")
                ).encode()
            ).hexdigest(),
            "execution_affinity_before": [8],
            "execution_affinity_after": [8],
        }
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "progressive-panels.json"
            with (
                mock.patch.object(collect, "SystemLiveHost", return_value=host) as constructor,
                mock.patch.object(collect.time, "monotonic", return_value=100.0),
                mock.patch.object(
                    batch,
                    "validate_timing_preconditions",
                    wraps=batch.validate_timing_preconditions,
                ) as timing_validation,
                mock.patch.object(
                    batch,
                    "collect_progressive_panels",
                    wraps=batch.collect_progressive_panels,
                ) as progressive,
            ):
                result = collect.collect_live_panels(output=output)
                with self.assertRaisesRegex(batch.BatchContractError, "exists"):
                    collect.collect_live_panels(output=output)

            constructor.assert_called_once_with(
                self.contract,
                collection_deadline=(
                    100.0 + self.contract["sampling"]["max_collection_wall_seconds"]
                ),
                persistent_rust_build_cache=True,
            )
            timing_validation.assert_called_once_with(timing_receipt, self.contract)
            progressive.assert_called_once()
            self.assertIs(progressive.call_args.args[1], self.contract)
            self.assertEqual(
                output.read_bytes(),
                json.dumps(result, sort_keys=True, separators=(",", ":")).encode()
                + b"\n",
            )

        failure_contract = copy.deepcopy(self.contract)
        failure_contract["panel_project_counts"] = [1]
        failure_contract["speed_gated_project_counts"] = []
        for key in (
            "panel_modes",
            "warmups_per_tool",
            "blocks",
            "samples_per_tool",
            "panel_timeout_seconds",
        ):
            failure_contract["sampling"][key] = failure_contract["sampling"][key][:1]
        failure_host = FakePanelHost(
            failure_contract,
            final_mutation=lambda row: row["candidate"].update(commit="c" * 40),
        )
        with tempfile.TemporaryDirectory() as directory:
            failed_output = Path(directory) / "failed-panels.json"
            with (
                mock.patch.object(batch, "verify_contract", return_value=failure_contract),
                mock.patch.object(collect, "SystemLiveHost", return_value=failure_host),
            ):
                with self.assertRaises(batch.BatchContractError):
                    collect.collect_live_panels(output=failed_output)
            self.assertFalse(failed_output.exists())
            self.assertEqual(list(Path(directory).iterdir()), [])

        self.assertEqual(result["verdict"], "PANEL-PASS")
        self.assertIn(result["verdict"], {"PANEL-PASS", "PANEL-FAIL"})
        self.assertNotIn(result["verdict"], {"GO", "PASS", "LINKAGE-PASS"})
        self.assertEqual(host.panel_order, [1, 32, 687])
        self.assertEqual(
            host.calls[:10],
            [
                ("freeze_candidate",),
                ("build_runner", "typokat", "a" * 40),
                ("build_runner", "tsgo", "a" * 40),
                ("protocol_info", "typokat", "2" * 64),
                ("protocol_info", "tsgo", "3" * 64),
                ("observe_timing_boundary", 0),
                ("run_fault", "typokat", "reuse-session"),
                ("run_fault", "typokat", "retain-cache"),
                ("run_fault", "tsgo", "reuse-checker"),
                ("run_fault", "tsgo", "retain-frame"),
            ],
        )
        self.assertEqual(host.calls[-2], ("observe_timing_boundary", 1))
        self.assertEqual(host.calls[-1], ("final_panel_attestation", "a" * 40))
        process_calls = [
            call for call in host.calls if call[0] in {"run_fault", "run_batch"}
        ]
        expected_process_calls = [
            ("run_fault", "typokat", "reuse-session"),
            ("run_fault", "typokat", "retain-cache"),
            ("run_fault", "tsgo", "reuse-checker"),
            ("run_fault", "tsgo", "retain-frame"),
            ("run_batch", 1, "sample", 0, "typokat"),
            ("run_batch", 1, "sample", 1, "tsgo"),
        ]
        expected_process_calls.extend(
            ("run_batch", 32, "warmup", ordinal, tool)
            for ordinal, tool in enumerate(
                ["typokat", "tsgo", "typokat", "tsgo", "typokat", "tsgo"]
            )
        )
        expected_process_calls.extend(
            ("run_batch", 32, "sample", ordinal, tool)
            for ordinal, tool in enumerate(
                ["typokat", "tsgo", "tsgo", "typokat"] * 5,
                start=6,
            )
        )
        expected_process_calls.append(("run_batch", 687, "sample", 0, "typokat"))
        self.assertEqual(process_calls, expected_process_calls)
        self.assertEqual(len(host.process_receipts), 4 + 2 + 26 + 1)

        evidence = result["evidence"]
        self.assertEqual(
            set(evidence),
            {
                "candidate",
                "final_attestation",
                "panel_receipts",
                "process_receipts_sha256",
                "profile",
                "protocols",
                "raw_panels",
                "runners",
                "schema",
                "timing",
            },
        )
        self.assertEqual(
            [row["projects"] for row in evidence["raw_panels"]], [1, 32, 687]
        )
        self.assertEqual(
            [row["projects"] for row in evidence["panel_receipts"]], [1, 32, 687]
        )
        for panel in evidence["raw_panels"]:
            self.assertEqual(panel["negative_controls"], host.controls)
            for process in [*panel["warmups"], *panel["samples"]]:
                expected_binary = (
                    "2" * 64 if process["tool"] == "typokat" else "3" * 64
                )
                self.assertEqual(process["executable_before_sha256"], expected_binary)
                self.assertEqual(process["executable_after_sha256"], expected_binary)
        panel_pids = [
            process["pid"]
            for panel in evidence["raw_panels"]
            for process in [*panel["warmups"], *panel["samples"]]
        ]
        self.assertEqual(len(panel_pids), len(set(panel_pids)))
        self.assertEqual(evidence["candidate"], host.candidate)
        self.assertEqual(
            evidence["profile"],
            {
                "file_count": 82,
                "length_framed_sha256": self.contract["profile"][
                    "length_framed_sha256"
                ],
            },
        )
        self.assertEqual(
            evidence["process_receipts_sha256"],
            evidence["final_attestation"]["process_receipts_sha256"],
        )
        self.assertEqual(evidence["timing"], timing_receipt)
        serialized_scope = json.dumps(evidence, sort_keys=True, separators=(",", ":"))
        for excluded in ("cold", "linkage", "concurrency", "allocation"):
            self.assertNotIn(f'"{excluded}"', serialized_scope)
        self.assertEqual(
            result["evidence_sha256"],
            hashlib.sha256(serialized_scope.encode()).hexdigest(),
        )

    def test_live_panel_collector_is_fail_fast_at_each_progressive_boundary(self) -> None:
        swapped = FakePanelHost(self.contract)
        candidate = swapped.freeze_candidate()
        typokat_runner = swapped.build_runner("typokat", candidate)
        tsgo_runner = swapped.build_runner("tsgo", candidate)
        self.assertNotEqual(typokat_runner, tsgo_runner)
        with self.assertRaisesRegex(AssertionError, "wrong tool runner"):
            swapped.run_fault(
                "typokat",
                tsgo_runner,
                "reuse-session",
                timeout_seconds=10,
                address_space_limit_bytes=1 << 30,
            )

        dirty = FakePanelHost(self.contract)
        dirty.candidate["dirty"] = True
        with self.assertRaisesRegex(batch.BatchContractError, "candidate"):
            collect._collect_live_panels(dirty, self.contract)
        self.assertEqual(dirty.calls, [("freeze_candidate",)])

        for failed, expected in ((1, [1]), (32, [1, 32])):
            host = FakePanelHost(self.contract, fail_at=failed)
            result = collect._collect_live_panels(host, self.contract)
            with self.subTest(failed=failed):
                self.assertEqual(result["verdict"], "PANEL-FAIL")
                self.assertEqual(host.panel_order, expected)
                self.assertFalse(any(projects > failed for projects in host.panel_order))
                self.assertEqual(
                    [row["projects"] for row in result["evidence"]["raw_panels"]],
                    expected,
                )

        reused_pid = FakePanelHost(self.contract, reuse_cross_panel_pid=True)
        with self.assertRaisesRegex(batch.BatchContractError, "pid|process"):
            collect._collect_live_panels(reused_pid, self.contract)
        self.assertEqual(reused_pid.panel_order, [1, 32])

    def test_live_panel_collector_rejects_each_runner_identity_collision(self) -> None:
        for axis in ("path", "source_sha256", "binary_sha256", "commit"):
            contract = copy.deepcopy(self.contract)
            host = FakePanelHost(contract, runner_collision=axis)
            if axis == "commit":
                host.candidate["commit"] = contract["tsgo"]["upstream_revision"]
            with self.subTest(axis=axis), self.assertRaisesRegex(
                batch.BatchContractError,
                rf"runner.*{axis}.*collision",
            ):
                collect._collect_live_panels(host, contract)
            self.assertFalse(
                any(call[0] in {"run_fault", "run_batch"} for call in host.calls)
            )

    def test_live_panel_collector_authenticates_every_frame_product_identity(self) -> None:
        for phase in ("warmup", "sample"):
            host = FakePanelHost(self.contract, profile_mutation=phase)
            with self.subTest(phase=phase), self.assertRaisesRegex(
                batch.BatchContractError,
                "shared product identity",
            ):
                collect._collect_live_panels(host, self.contract)

    def test_collection_timeout_is_clamped_to_the_global_deadline(self) -> None:
        with mock.patch.object(collect.time, "monotonic", return_value=100.0):
            self.assertEqual(collect._bounded_timeout(10, 150.0), 10)
            self.assertEqual(collect._bounded_timeout(60, 150.0), 50.0)
            with self.assertRaisesRegex(batch.BatchContractError, "budget|deadline"):
                collect._bounded_timeout(10, 100.0)

    def test_live_panel_collector_rejects_every_final_attestation_drift(self) -> None:
        contract = copy.deepcopy(self.contract)
        contract["panel_project_counts"] = [1]
        contract["speed_gated_project_counts"] = []
        for key in (
            "panel_modes",
            "warmups_per_tool",
            "blocks",
            "samples_per_tool",
            "panel_timeout_seconds",
        ):
            contract["sampling"][key] = contract["sampling"][key][:1]
        mutations = {
            "candidate": lambda row: row["candidate"].update(commit="c" * 40),
            "runner": lambda row: row["runners"]["typokat"].update(
                binary_sha256="8" * 64
            ),
            "profile": lambda row: row["profile"].update(
                length_framed_sha256="8" * 64
            ),
            "process": lambda row: row.update(process_receipts_sha256="8" * 64),
        }
        for label, mutation in mutations.items():
            host = FakePanelHost(contract, final_mutation=mutation)
            with self.subTest(label=label), self.assertRaises(batch.BatchContractError):
                collect._collect_live_panels(host, contract)

        affinity = FakePanelHost(contract)
        affinity.final_execution_affinity = [9]
        with self.assertRaisesRegex(batch.BatchContractError, "affinity|timing"):
            collect._collect_live_panels(affinity, contract)


class SessionBatchLiveLinkageCollectorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = batch.verify_contract()

    def test_public_linkage_collector_is_non_injectable_and_publishes_canonical_new_file(
        self,
    ) -> None:
        signature = inspect.signature(collect.collect_live_linkage)
        self.assertEqual(list(signature.parameters), ["output"])
        self.assertEqual(signature.parameters["output"].kind, inspect.Parameter.KEYWORD_ONLY)

        host = FakeLinkageHost(self.contract)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "linkage.json"
            with mock.patch.object(collect, "SystemLiveHost", return_value=host) as constructor:
                result = collect.collect_live_linkage(output=output)
                with self.assertRaisesRegex(batch.BatchContractError, "exists"):
                    collect.collect_live_linkage(output=output)
                constructor.assert_called_once_with(self.contract)
                self.assertEqual(
                    output.read_bytes(),
                    json.dumps(result, sort_keys=True, separators=(",", ":")).encode()
                    + b"\n",
                )

        self.assertEqual(result["verdict"], "LINKAGE-PASS")
        self.assertNotIn(result["verdict"], {"GO", "PASS"})
        self.assertEqual(
            result["evidence_sha256"],
            hashlib.sha256(
                json.dumps(
                    result["evidence"], sort_keys=True, separators=(",", ":")
                ).encode()
            ).hexdigest(),
        )

    def test_private_linkage_flow_owns_exact_live_sequence_and_recomputation(self) -> None:
        host = FakeLinkageHost(self.contract)
        with mock.patch.object(
            batch,
            "recompute_single_project_linkage",
            wraps=batch.recompute_single_project_linkage,
        ) as recompute:
            result = collect._collect_live_linkage(host, self.contract)
        recompute.assert_called_once()

        rows = self.contract["single_project"]["rows"]
        expected_calls = [
            ("freeze_candidate",),
            ("build_runner", "typokat", "a" * 40, "b" * 40),
            ("build_runner", "tsgo", "a" * 40, "b" * 40),
            ("protocol_info", "typokat", "2" * 64),
            ("protocol_info", "tsgo", "3" * 64),
            ("build_cold_typokat", "a" * 40, "b" * 40),
            ("authenticate_cold_tsgo",),
            ("run_batch", "typokat", 10, 1 << 30),
            ("run_batch", "tsgo", 10, 1 << 30),
        ]
        for tool in ("typokat", "tsgo"):
            expected_calls.extend(
                ("run_cold", tool, row, 60, 1 << 30) for row in rows
            )
            expected_calls.extend(
                ("run_single_project", tool, row, 60, 1 << 30) for row in rows
            )
        expected_calls.append(("final_linkage_attestation", "a" * 40, "b" * 40))
        self.assertEqual(host.calls, expected_calls)

        self.assertEqual(result["verdict"], "LINKAGE-PASS")
        self.assertNotEqual(result["verdict"], "GO")
        evidence = result["evidence"]
        self.assertEqual(
            set(evidence),
            {
                "batch_smoke", "candidate", "cold_binaries", "cold_processes",
                "final_attestation", "protocols", "recomputation", "runners",
                "schema", "single_project_processes",
            },
        )
        self.assertEqual(evidence["recomputation"]["verdict"], "LINKAGE-RECOMPUTED")
        self.assertNotIn(evidence["recomputation"]["verdict"], {"GO", "PASS"})
        self.assertEqual(evidence["candidate"], host.candidate)
        self.assertEqual(evidence["final_attestation"]["candidate"], host.candidate)
        self.assertEqual(
            evidence["final_attestation"]["profile"],
            {
                "file_count": 82,
                "length_framed_sha256": self.contract["profile"][
                    "length_framed_sha256"
                ],
            },
        )
        self.assertEqual(
            evidence["final_attestation"]["runners"], evidence["runners"]
        )
        self.assertEqual(
            evidence["final_attestation"]["cold"], evidence["cold_binaries"]
        )
        self.assertEqual(
            [row["case_id"] for row in evidence["cold_processes"]["typokat"]],
            rows,
        )
        self.assertEqual(
            [row["case_id"] for row in evidence["single_project_processes"]["tsgo"]],
            rows,
        )
        file_counts = self.contract["single_project"]["file_counts"]
        for tool in ("typokat", "tsgo"):
            runner_hash = evidence["runners"][tool]["binary_sha256"]
            smoke = evidence["batch_smoke"][tool]
            self.assertEqual(smoke["executable_before_sha256"], runner_hash)
            self.assertIs(type(smoke["pid"]), int)
            self.assertLess(smoke["start_ns"], smoke["end_ns"])
            self.assertTrue(smoke["process_group_empty"])
            self.assertEqual(smoke["address_space_limit_bytes"], 1 << 30)
            for expected_count, record in zip(
                file_counts, evidence["single_project_processes"][tool], strict=True
            ):
                request = json.loads(record["request_stream"])
                self.assertEqual(len(request["files"]), expected_count)
                self.assertEqual(
                    record["request_stream"],
                    json.dumps(request, sort_keys=True, separators=(",", ":")) + "\n",
                )
                self.assertEqual(record["executable_before_sha256"], runner_hash)
            for record in (
                *evidence["cold_processes"][tool],
                *evidence["single_project_processes"][tool],
            ):
                self.assertIs(type(record["pid"]), int)
                self.assertLess(record["start_ns"], record["end_ns"])
                self.assertTrue(record["process_group_empty"])
                self.assertEqual(record["address_space_limit_bytes"], 1 << 30)
                self.assertEqual(record["timeout_seconds"], 60)

    def test_linkage_rechecks_every_final_identity_before_pass(self) -> None:
        mutations = {
            "candidate-commit": lambda value: value["candidate"].update(commit="c" * 40),
            "candidate-tree": lambda value: value["candidate"].update(tree="c" * 40),
            "runner-source": lambda value: value["runners"]["typokat"].update(
                source_sha256="8" * 64
            ),
            "runner-binary": lambda value: value["runners"]["tsgo"].update(
                binary_sha256="8" * 64
            ),
            "cold-path": lambda value: value["cold"]["typokat"].update(path="/tmp/other"),
            "cold-hash": lambda value: value["cold"]["typokat"].update(
                binary_sha256="8" * 64
            ),
            "cold-source": lambda value: value["cold"]["typokat"].update(
                source_identity_sha256="8" * 64
            ),
            "tsgo-binary": lambda value: value["cold"]["tsgo"].update(
                binary_sha256="8" * 64
            ),
            "profile-digest": lambda value: value["profile"].update(
                length_framed_sha256="8" * 64
            ),
            "profile-count": lambda value: value["profile"].update(file_count=81),
        }
        for label, mutation in mutations.items():
            host = FakeLinkageHost(self.contract)
            original = host.final_linkage_attestation

            def forged(candidate, runners, cold, *, mutation=mutation):
                value = original(candidate, runners, cold)
                mutation(value)
                return value

            host.final_linkage_attestation = forged
            with self.subTest(label=label), self.assertRaises(batch.BatchContractError):
                collect._collect_live_linkage(host, self.contract)

    def test_linkage_fails_closed_on_missing_or_changed_live_process_evidence(self) -> None:
        missing = FakeLinkageHost(self.contract)
        missing.single_processes["typokat"].pop()
        with self.assertRaises(batch.BatchContractError):
            collect._collect_live_linkage(missing, self.contract)

        cold_output = FakeLinkageHost(self.contract)
        cold_output.cold_processes["typokat"][1]["stderr"] = cold_output.cold_processes[
            "typokat"
        ][1]["stderr"].replace("(4,7)", "(4,8)", 1)
        with self.assertRaises(batch.BatchContractError):
            collect._collect_live_linkage(cold_output, self.contract)

        single_output = FakeLinkageHost(self.contract)
        record = single_output.single_processes["tsgo"][1]
        response = json.loads(record["response_stream"])
        response["semantic"]["diagnostics"][0] = "fast-errors/main.ts:4:8:2322"
        refresh_single_project_response(record, response)
        with self.assertRaises(batch.BatchContractError):
            collect._collect_live_linkage(single_output, self.contract)

        batch_hash = FakeLinkageHost(self.contract)
        original_batch = batch_hash.run_batch

        def forged_batch(tool, runner, request_stream, **kwargs):
            record = original_batch(tool, runner, request_stream, **kwargs)
            if tool == "typokat":
                record["executable_after_sha256"] = "8" * 64
            return record

        batch_hash.run_batch = forged_batch
        with self.assertRaisesRegex(batch.BatchContractError, "binary|runner|identity"):
            collect._collect_live_linkage(batch_hash, self.contract)

        missing_pid = FakeLinkageHost(self.contract)
        del missing_pid.cold_processes["tsgo"][0]["pid"]
        with self.assertRaisesRegex(batch.BatchContractError, "pid|process|live"):
            collect._collect_live_linkage(missing_pid, self.contract)

        retained = FakeLinkageHost(self.contract)
        record = retained.single_processes["typokat"][0]
        response = json.loads(record["response_stream"])
        response["session_weak_alive_after"] = True
        refresh_single_project_response(record, response)
        with self.assertRaisesRegex(batch.BatchContractError, "session|state|alive"):
            collect._collect_live_linkage(retained, self.contract)


class SessionBatchLiveConcurrencyCollectorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = batch.verify_contract()

    def test_public_concurrency_collector_is_non_injectable_atomic_and_canonical(
        self,
    ) -> None:
        signature = inspect.signature(collect.collect_live_concurrency)
        self.assertEqual(list(signature.parameters), ["output"])
        self.assertEqual(
            signature.parameters["output"].kind,
            inspect.Parameter.KEYWORD_ONLY,
        )
        private = inspect.signature(collect._collect_live_concurrency)
        self.assertEqual(list(private.parameters), ["host", "contract"])

        host = FakePostPanelHost(self.contract)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "concurrency.json"
            with mock.patch.object(
                collect, "SystemLiveHost", return_value=host
            ) as constructor:
                result = collect.collect_live_concurrency(output=output)
                self.assertEqual(json.loads(output.read_bytes()), result)
                with self.assertRaisesRegex(batch.BatchContractError, "exists"):
                    collect.collect_live_concurrency(output=output)
            constructor.assert_called_once_with(
                self.contract,
                collection_deadline=mock.ANY,
                persistent_rust_build_cache=True,
            )
            self.assertEqual(
                output.read_bytes(),
                json.dumps(result, sort_keys=True, separators=(",", ":")).encode()
                + b"\n",
            )
            self.assertEqual(list(Path(directory).iterdir()), [output])

        self.assertEqual(result["verdict"], "CONCURRENCY-PASS")
        self.assertEqual(set(result), {"verdict", "evidence_sha256", "evidence"})

        failed = FakePostPanelHost(self.contract)
        failed.final_attestation_mutation = lambda value: value["candidate"].update(
            dirty=True
        )
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "failed-concurrency.json"
            with mock.patch.object(collect, "SystemLiveHost", return_value=failed):
                with self.assertRaises(batch.BatchContractError):
                    collect.collect_live_concurrency(output=output)
            self.assertFalse(output.exists())
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_private_concurrency_flow_owns_authenticated_source_counts_and_verification(
        self,
    ) -> None:
        host = FakePostPanelHost(self.contract)
        with mock.patch.object(
            batch, "verify_concurrency", wraps=batch.verify_concurrency
        ) as verification:
            result = collect._collect_live_concurrency(host, self.contract)

        verification.assert_called_once()
        self.assertEqual(verification.call_args.args, (host.concurrency, self.contract))
        self.assertEqual(
            host.calls,
            [
                ("freeze_candidate",),
                ("build_runner", "typokat", "a" * 40, "b" * 40),
                (
                    "protocol_info",
                    "/tmp/typokat-post-panel-runner",
                    FakePostPanelHost.NORMAL_BINARY_SHA256,
                ),
                ("run_concurrency", 1, 10, 1 << 30),
                ("run_concurrency", 2, 10, 1 << 30),
                ("run_concurrency", 32, 10, 1 << 30),
                (
                    "final_typokat_attestation",
                    "a" * 40,
                    FakePostPanelHost.NORMAL_BINARY_SHA256,
                ),
            ],
        )

        evidence = result["evidence"]
        self.assertEqual(evidence["schema"], 1)
        self.assertEqual(evidence["candidate"], host.candidate)
        self.assertEqual(evidence["runner"], host.normal_runner)
        self.assertEqual(
            evidence["protocol"]["source_sha256"],
            host.normal_runner["source_sha256"],
        )
        self.assertEqual(
            evidence["protocol"]["binary_sha256"],
            host.normal_runner["binary_sha256"],
        )
        self.assertEqual(
            evidence["final_attestation"],
            {"candidate": host.candidate, "runner": host.normal_runner},
        )
        self.assertEqual(evidence["concurrency"], host.concurrency)
        first = host.authority["frames"][0]
        expected_request = {
            "schema": 1,
            "case_id": first["case_id"],
            "source": first["source"],
        }
        first_source = first["source"].encode()
        self.assertEqual(len(first_source), first["source_bytes"])
        self.assertEqual(
            hashlib.sha256(first_source).hexdigest(), first["source_sha256"]
        )
        expected_semantic = {
            key: value
            for key, value in host.authority["typokat"][0].items()
            if key != "case_id"
        }
        for count in self.contract["concurrency"]["session_counts"]:
            record = evidence["concurrency"][str(count)]
            self.assertEqual(record["request_stream"], host.first_request_stream)
            self.assertEqual(
                json.loads(record["request_stream"]), expected_request
            )
            self.assertEqual(
                hashlib.sha256(record["request_stream"].encode()).hexdigest(),
                record["request_stream_sha256"],
            )
            self.assertTrue(
                all(
                    session["semantic"] == expected_semantic
                    for session in record["sessions"]
                )
            )
            for session in record["sessions"]:
                semantic_bytes = json.dumps(
                    session["semantic"], sort_keys=True, separators=(",", ":")
                ).encode()
                self.assertEqual(
                    session["raw_output_sha256"],
                    hashlib.sha256(semantic_bytes).hexdigest(),
                )
            self.assertEqual(
                record["executable_before_sha256"],
                host.normal_runner["binary_sha256"],
            )
            self.assertEqual(
                record["executable_after_sha256"],
                host.normal_runner["binary_sha256"],
            )
        self.assertEqual(
            result["evidence_sha256"],
            hashlib.sha256(
                json.dumps(evidence, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
        )
        repeated = collect._collect_live_concurrency(
            FakePostPanelHost(self.contract), self.contract
        )
        self.assertEqual(repeated, result)

    def test_concurrency_fails_closed_on_source_semantic_binary_and_attestation_drift(
        self,
    ) -> None:
        source = FakePostPanelHost(self.contract)
        forged_request = json.dumps(
            {"schema": 1, "case_id": "forged", "source": "const forged = true;"},
            sort_keys=True,
            separators=(",", ":"),
        ) + "\n"
        source.concurrency["1"].update(
            request_stream=forged_request,
            request_stream_sha256=hashlib.sha256(forged_request.encode()).hexdigest(),
        )

        semantic = FakePostPanelHost(self.contract)
        semantic.concurrency["32"]["sessions"][-1]["semantic"]["diagnostics"].append(
            "999:9999"
        )
        refresh_concurrency_raw(semantic.concurrency["32"])

        binary = FakePostPanelHost(self.contract)
        binary.concurrency["2"]["executable_after_sha256"] = "9" * 64

        candidate_attestation = FakePostPanelHost(self.contract)
        candidate_attestation.final_attestation_mutation = lambda value: value[
            "candidate"
        ].update(tree="9" * 40)

        runner_attestation = FakePostPanelHost(self.contract)
        runner_attestation.final_attestation_mutation = lambda value: value[
            "runner"
        ].update(source_sha256="9" * 64)

        for label, host in (
            ("source", source),
            ("semantic", semantic),
            ("binary", binary),
            ("candidate-attestation", candidate_attestation),
            ("runner-attestation", runner_attestation),
        ):
            with self.subTest(label=label), self.assertRaises(
                batch.BatchContractError
            ):
                collect._collect_live_concurrency(host, self.contract)

    def test_concurrency_authenticates_protocol_before_running_and_requires_clean_candidate(
        self,
    ) -> None:
        dirty = FakePostPanelHost(self.contract)
        dirty.candidate["dirty"] = True
        with self.assertRaisesRegex(batch.BatchContractError, "candidate"):
            collect._collect_live_concurrency(dirty, self.contract)
        self.assertEqual(dirty.calls, [("freeze_candidate",)])

        forged = FakePostPanelHost(self.contract)
        original = forged.protocol_info

        def wrong_source(tool, runner):
            value = original(tool, runner)
            value["source_sha256"] = "9" * 64
            return value

        forged.protocol_info = wrong_source
        with self.assertRaises(batch.BatchContractError):
            collect._collect_live_concurrency(forged, self.contract)
        self.assertFalse(any(call[0] == "run_concurrency" for call in forged.calls))


class SessionBatchLiveAllocationCollectorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = batch.verify_contract()

    def test_public_allocation_collector_is_non_injectable_atomic_and_canonical(
        self,
    ) -> None:
        signature = inspect.signature(collect.collect_live_allocation)
        self.assertEqual(list(signature.parameters), ["output"])
        self.assertEqual(
            signature.parameters["output"].kind,
            inspect.Parameter.KEYWORD_ONLY,
        )
        private = inspect.signature(collect._collect_live_allocation)
        self.assertEqual(list(private.parameters), ["host", "contract"])

        host = FakePostPanelHost(self.contract)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "allocation.json"
            with mock.patch.object(
                collect, "SystemLiveHost", return_value=host
            ) as constructor:
                result = collect.collect_live_allocation(output=output)
                self.assertEqual(json.loads(output.read_bytes()), result)
                with self.assertRaisesRegex(batch.BatchContractError, "exists"):
                    collect.collect_live_allocation(output=output)
            constructor.assert_called_once_with(
                self.contract,
                collection_deadline=mock.ANY,
                persistent_rust_build_cache=True,
            )
            self.assertEqual(
                output.read_bytes(),
                json.dumps(result, sort_keys=True, separators=(",", ":")).encode()
                + b"\n",
            )
            self.assertEqual(list(Path(directory).iterdir()), [output])

        self.assertEqual(result["verdict"], "ALLOCATION-PASS")
        self.assertEqual(set(result), {"verdict", "evidence_sha256", "evidence"})

        failed = FakePostPanelHost(self.contract)
        failed.final_attestation_mutation = lambda value: value["runner"].update(
            binary_sha256="9" * 64
        )
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "failed-allocation.json"
            with mock.patch.object(collect, "SystemLiveHost", return_value=failed):
                with self.assertRaises(batch.BatchContractError):
                    collect.collect_live_allocation(output=output)
            self.assertFalse(output.exists())
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_private_allocation_flow_owns_full_source_panel_and_all_verifiers(
        self,
    ) -> None:
        host = FakePostPanelHost(self.contract)
        with (
            mock.patch.object(
                batch, "verify_raw_oracles", wraps=batch.verify_raw_oracles
            ) as raw_verification,
            mock.patch.object(
                batch,
                "validate_lifecycle_receipts",
                wraps=batch.validate_lifecycle_receipts,
            ) as lifecycle_verification,
            mock.patch.object(
                batch, "verify_route_intervals", wraps=batch.verify_route_intervals
            ) as route_verification,
            mock.patch.object(
                batch,
                "recompute_typokat_accepted_progress",
                wraps=batch.recompute_typokat_accepted_progress,
            ) as ratchet_verification,
            mock.patch.object(
                batch, "verify_allocation_probe", wraps=batch.verify_allocation_probe
            ) as allocation_verification,
        ):
            result = collect._collect_live_allocation(host, self.contract)

        raw_verification.assert_called_once()
        raw_input, raw_expected, raw_contract = raw_verification.call_args.args
        self.assertEqual(raw_input["projects"], 687)
        verification_record = copy.deepcopy(host.allocation_record)
        verification_record["stdout"] = verification_record["response_stream"]
        self.assertEqual(raw_input["samples"], [verification_record])
        self.assertEqual(raw_expected, batch.oracle_panel(host.authority, 687))
        self.assertIs(raw_contract, self.contract)
        lifecycle_verification.assert_called()
        route_verification.assert_called_once_with(host.allocation_record, 687)
        ratchet_verification.assert_called()
        allocation_verification.assert_called_once_with(
            host.allocation_record["allocation_receipt"], self.contract
        )
        self.assertEqual(
            host.calls,
            [
                ("freeze_candidate",),
                ("build_allocation_runner", "a" * 40, "b" * 40),
                (
                    "protocol_info",
                    "/tmp/typokat-allocation-runner",
                    FakePostPanelHost.FEATURE_BINARY_SHA256,
                ),
                ("run_allocation_probe", 687, 600, 1 << 30),
                (
                    "final_typokat_attestation",
                    "a" * 40,
                    FakePostPanelHost.FEATURE_BINARY_SHA256,
                ),
            ],
        )

        evidence = result["evidence"]
        self.assertEqual(evidence["schema"], 1)
        self.assertEqual(evidence["candidate"], host.candidate)
        self.assertEqual(evidence["runner"], host.feature_runner)
        self.assertEqual(
            evidence["protocol"]["source_sha256"],
            host.feature_runner["source_sha256"],
        )
        self.assertEqual(
            evidence["protocol"]["binary_sha256"],
            host.feature_runner["binary_sha256"],
        )
        self.assertEqual(evidence["process"], host.allocation_record)
        self.assertEqual(
            evidence["allocation_receipt"],
            host.allocation_record["allocation_receipt"],
        )
        self.assertEqual(
            evidence["final_attestation"],
            {"candidate": host.candidate, "runner": host.feature_runner},
        )
        self.assertEqual(len(evidence["process"]["frames"]), 687)
        self.assertEqual(
            evidence["process"]["request_stream"], host.allocation_request_stream
        )
        for request, authority in zip(
            evidence["process"]["request_stream"].splitlines(),
            host.authority["frames"],
            strict=True,
        ):
            self.assertEqual(
                json.loads(request),
                {
                    "schema": 1,
                    "case_id": authority["case_id"],
                    "source": authority["source"],
                },
            )
            source_bytes = authority["source"].encode()
            self.assertEqual(len(source_bytes), authority["source_bytes"])
            self.assertEqual(
                hashlib.sha256(source_bytes).hexdigest(),
                authority["source_sha256"],
            )
        receipt = evidence["allocation_receipt"]
        self.assertEqual(
            set(receipt),
            {
                "allocation_count",
                "binary_sha256",
                "instrumentation",
                "timed",
                "tool",
            },
        )
        self.assertIs(evidence["process"]["timed"], False)
        self.assertGreater(receipt["allocation_count"], 0)
        self.assertIs(receipt["timed"], False)
        self.assertEqual(receipt["instrumentation"], "counting-allocator")
        self.assertEqual(
            receipt["binary_sha256"], host.feature_runner["binary_sha256"]
        )
        self.assertEqual(
            evidence["process"]["executable_before_sha256"],
            host.feature_runner["binary_sha256"],
        )
        self.assertEqual(
            evidence["process"]["executable_after_sha256"],
            host.feature_runner["binary_sha256"],
        )
        self.assertEqual(
            result["evidence_sha256"],
            hashlib.sha256(
                json.dumps(evidence, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
        )
        repeated = collect._collect_live_allocation(
            FakePostPanelHost(self.contract), self.contract
        )
        self.assertEqual(repeated, result)

    def test_allocation_fails_closed_on_frame_allocation_binary_and_attestation_drift(
        self,
    ) -> None:
        source = FakePostPanelHost(self.contract)
        source_request = json.loads(
            source.allocation_record["frames"][0]["raw_request"]
        )
        source_request["source"] += "\nconst forged = true;"
        source.allocation_record["frames"][0]["raw_request"] = json.dumps(
            source_request, sort_keys=True, separators=(",", ":")
        )
        refresh_sample_streams(source.allocation_record)

        semantic = FakePostPanelHost(self.contract)
        semantic.allocation_record["frames"][0]["semantic"]["diagnostics"].append(
            "999:9999"
        )
        refresh_responses(semantic.allocation_record)

        lifecycle = FakePostPanelHost(self.contract)
        lifecycle.allocation_record["frames"][0]["session_weak_alive_after"] = True
        refresh_responses(lifecycle.allocation_record)

        allocation = FakePostPanelHost(self.contract)
        allocation.allocation_record["allocation_receipt"]["allocation_count"] = 0

        allocation_shape = FakePostPanelHost(self.contract)
        allocation_shape.allocation_record["allocation_receipt"]["claimed"] = True

        receipt_binary = FakePostPanelHost(self.contract)
        receipt_binary.allocation_record["allocation_receipt"][
            "binary_sha256"
        ] = FakePostPanelHost.NORMAL_BINARY_SHA256

        process_binary = FakePostPanelHost(self.contract)
        process_binary.allocation_record["executable_after_sha256"] = "9" * 64

        candidate_attestation = FakePostPanelHost(self.contract)
        candidate_attestation.final_attestation_mutation = lambda value: value[
            "candidate"
        ].update(commit="9" * 40)

        runner_attestation = FakePostPanelHost(self.contract)
        runner_attestation.final_attestation_mutation = lambda value: value[
            "runner"
        ].update(binary_sha256="9" * 64)

        for label, host in (
            ("source-frame", source),
            ("semantic-frame", semantic),
            ("lifecycle-frame", lifecycle),
            ("allocation", allocation),
            ("allocation-shape", allocation_shape),
            ("allocation-binary", receipt_binary),
            ("process-binary", process_binary),
            ("candidate-attestation", candidate_attestation),
            ("runner-attestation", runner_attestation),
        ):
            with self.subTest(label=label), self.assertRaises(
                batch.BatchContractError
            ):
                collect._collect_live_allocation(host, self.contract)

    def test_allocation_authenticates_feature_protocol_before_running_and_requires_clean_candidate(
        self,
    ) -> None:
        dirty = FakePostPanelHost(self.contract)
        dirty.candidate["dirty"] = True
        with self.assertRaisesRegex(batch.BatchContractError, "candidate"):
            collect._collect_live_allocation(dirty, self.contract)
        self.assertEqual(dirty.calls, [("freeze_candidate",)])

        forged = FakePostPanelHost(self.contract)
        original = forged.protocol_info

        def wrong_binary(tool, runner):
            value = original(tool, runner)
            value["binary_sha256"] = FakePostPanelHost.NORMAL_BINARY_SHA256
            return value

        forged.protocol_info = wrong_binary
        with self.assertRaises(batch.BatchContractError):
            collect._collect_live_allocation(forged, self.contract)
        self.assertFalse(
            any(call[0] == "run_allocation_probe" for call in forged.calls)
        )


if __name__ == "__main__":
    unittest.main()

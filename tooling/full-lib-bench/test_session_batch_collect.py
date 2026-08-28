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
    make_cold_process,
    make_single_project_process,
    protocol_info,
    raw_panel_evidence,
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
        self.assertEqual(
            environment["RUSTUP_TOOLCHAIN"],
            "1.95.0-x86_64-unknown-linux-gnu",
        )
        self.assertEqual(environment["RUSTC_WRAPPER"], "")
        self.assertEqual(environment["RUSTC_WORKSPACE_WRAPPER"], "")

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

            cargo_home_config = cargo_home / "config"
            cargo_home_config.write_text("[build]\njobs = 99\n")
            with self.assertRaisesRegex(
                batch.BatchContractError,
                "Cargo configuration",
            ):
                host._validate_cargo_configuration(cwd, cargo_home)

    def test_live_preflight_owns_build_protocol_process_and_fault_sequence(self) -> None:
        host = FakeLiveHost(self.contract)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "preflight.json"
            with mock.patch.object(collect, "SystemLiveHost", return_value=host) as constructor:
                result = collect.collect_live_preflight(output=output)
                with self.assertRaisesRegex(batch.BatchContractError, "exists"):
                    collect.collect_live_preflight(output=output)
                constructor.assert_called_once()
                (constructed_contract,) = constructor.call_args.args
                self.assertEqual(constructed_contract, self.contract)
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
                constructor.assert_called_once()
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
        batch_hash.smoke["typokat"]["executable_after_sha256"] = "8" * 64
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


if __name__ == "__main__":
    unittest.main()

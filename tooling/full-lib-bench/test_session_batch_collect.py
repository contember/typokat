#!/usr/bin/env python3
"""RED acceptance for the live WU6 preflight collector."""

from __future__ import annotations

import copy
import hashlib
import inspect
import json
from pathlib import Path
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
    protocol_info,
    raw_panel_evidence,
    refresh_sample_streams,
)


class FakeLiveHost:
    def __init__(self, contract: dict[str, object]) -> None:
        self.contract = contract
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


if __name__ == "__main__":
    unittest.main()

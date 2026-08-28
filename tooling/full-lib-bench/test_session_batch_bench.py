#!/usr/bin/env python3
"""RED acceptance for the WU6 isolated-session comparator."""

from __future__ import annotations

import copy
from dataclasses import replace
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import session_batch_bench as batch
import full_lib_bench as cold


class SessionBatchContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = batch.verify_contract()

    def test_exact_frozen_inputs_and_progressive_panels(self) -> None:
        self.assertEqual(self.contract["panel_project_counts"], [1, 32, 687])
        self.assertEqual(self.contract["speed_gated_project_counts"], [32, 687])
        self.assertEqual(self.contract["panel_order"], "fail-fast")
        self.assertEqual(self.contract["profile"]["file_count"], 82)
        self.assertEqual(self.contract["official_frames"]["eligible_frame_count"], 687)
        self.assertEqual(
            self.contract["official_frames"]["frame_source_identity_sha256"],
            "cf5104d353074604d8626864d6f5b7451796409c75113ce213f79c911cb5388f",
        )
        self.assertEqual(self.contract["current_typokat_baseline"]["wall_us"], 12_946_437)
        self.assertEqual(
            self.contract["current_typokat_baseline"]["wall_boundary"],
            "sum_frame_route_wall_us",
        )
        self.assertEqual(
            self.contract["current_typokat_baseline"]["peak_rss_bytes"], 144_109_568
        )
        self.assertEqual(self.contract["max_typokat_to_tsgo_median_rss"], 1.0)
        self.assertEqual(self.contract["child_address_space_limit_bytes"], 1 << 30)
        self.assertEqual(
            self.contract["current_typokat_baseline"]["artifact_sha256"],
            "950d5d7b38da7e7b5a0bc87da7ed423968b151ebf731c451021216c1d1880a18",
        )
        self.assertEqual(
            self.contract["oracles"]["typokat_batch_oracle_sha256"],
            "d3071df89d689e5d3da43a03cd74b9c7f6c91732699303a8ad33c3131af3575f",
        )

        baseline = batch.verify_current_typokat_baseline(self.contract)
        self.assertEqual(baseline["commit"], "58b5b1a80ad49d13dcfc66e6291c8f2932b1ad7f")
        self.assertEqual(baseline["tree"], "46f0b7284e401ddb9c035984a4d3f54e02f32bdd")
        self.assertEqual(baseline["frames"], 687)
        self.assertEqual(baseline["wall_us"], 12_946_437)
        self.assertEqual(baseline["peak_rss_bytes"], 144_109_568)

    def test_contract_pins_the_authenticated_upstream_and_cold_binary(self) -> None:
        self.assertEqual(
            self.contract["typescript_flags"],
            [
                "--strict",
                "--target",
                "es2025",
                "--module",
                "preserve",
                "--noEmit",
                "--pretty",
                "false",
                "--ignoreConfig",
            ],
        )
        self.assertEqual(
            self.contract["tsgo"]["upstream_revision"],
            "2bd066d87f5bafd315be9f40889d0a60b9e58e0b",
        )
        self.assertEqual(
            self.contract["tsgo"]["cold_binary_sha256"],
            "4f2de678286401759b3fb4475bafe35b8f32b4b3a07d92642bbf37eadc9b34a4",
        )
        self.assertEqual(
            self.contract["tsgo"]["build_argv"],
            ["go", "build", "-trimpath", "-o", "{output}", "./cmd/typokat-wu6-batch"],
        )
        self.assertEqual(
            self.contract["tsgo"]["runner_source_relative_path"],
            "tooling/full-lib-bench/tsgo-batch/main.go",
        )
        self.assertEqual(
            self.contract["typokat"]["build_argv"],
            ["cargo", "build", "--release", "--locked", "-p", "typokat-session-batch"],
        )
        self.assertEqual(self.contract["typokat"]["route"], "project-local-session-candidate")
        self.assertTrue(
            self.contract["runner_linkage"]["require_same_runner_binary_for_single_and_batch"]
        )
        self.assertTrue(
            self.contract["runner_linkage"]["require_tsgo_single_mode_match_cold_binary"]
        )

    def test_lifecycle_contract_allows_only_immutable_products_to_cross_frames(self) -> None:
        lifecycle = self.contract["lifecycle"]
        self.assertEqual(lifecycle["typokat_shared"], ["library_declaration_product"])
        self.assertEqual(lifecycle["tsgo_shared"], ["bound_immutable_library_source_files"])
        self.assertEqual(
            lifecycle["per_frame"],
            [
                "user_parse",
                "user_bind",
                "program",
                "checker",
                "project_global_merge",
                "semantic_state",
                "diagnostics",
            ],
        )
        self.assertIn("checker_reuse", lifecycle["forbidden"])
        self.assertIn("program_update", lifecycle["forbidden"])
        self.assertIn("cross_frame_semantic_cache", lifecycle["forbidden"])
        self.assertTrue(lifecycle["require_zero_retained_frames"])
        self.assertEqual(
            lifecycle["timed_phases"],
            [
                "product_initialization",
                "parse_prepare",
                "binding",
                "publication_check",
                "release",
            ],
        )
        self.assertEqual(
            lifecycle["typokat_teardown_proof"],
            "session-and-cache-weak-handles-dead",
        )
        self.assertEqual(
            lifecycle["tsgo_teardown_proof"],
            "runner-retained-frame-handles-zero",
        )
        self.assertTrue(lifecycle["tsgo_single_threaded"])
        self.assertEqual(
            self.contract["allocation_probe"],
            {
                "timed": False,
                "tool": "typokat",
                "instrumentation": "counting-allocator",
                "metric": "allocation_count",
            },
        )

    def test_protocol_info_requires_externally_recomputed_build_and_source_authentication(self) -> None:
        for tool in ("tsgo", "typokat"):
            expected = protocol_info(tool)
            identities = {
                "source_sha256": expected["source_sha256"],
                "binary_sha256": expected["binary_sha256"],
                "commit": expected["upstream_revision"],
            }
            batch.validate_protocol_info(expected, tool, identities, self.contract)
            for mutation in (
                lambda value: value.update(source_sha256="0" * 64),
                lambda value: value.update(binary_sha256="0" * 64),
                lambda value: value.update(upstream_revision="wrong"),
                lambda value: value.update(profile_sha256="0" * 64),
                lambda value: value.update(program_per_frame=False),
                lambda value: value.update(checker_per_frame=False),
                lambda value: value.update(shared_user_sources=True),
                lambda value: value.update(single_threaded=False),
                lambda value: value.update(compiler_flags=[]),
            ):
                candidate = copy.deepcopy(expected)
                mutation(candidate)
                with self.assertRaises(batch.BatchContractError):
                    batch.validate_protocol_info(candidate, tool, identities, self.contract)

    def test_all_panel_prefixes_have_exact_ordered_identities(self) -> None:
        observed = batch.verify_official_frame_prefixes(self.contract)
        self.assertEqual(
            observed,
            {
                1: "ab61a38bedaeea39deb13b87d9493807c996fada55b24b30361094eba46a58f4",
                32: "8ec0ad857e9e161bd4388aa5fb970b26041569f2d175ae7ea9cc0cda1ff8b0f5",
                687: "f901b5335e4627731db51737671b4d155005c936dff805b81d1dda74914a86c5",
            },
        )

    def test_batch_oracles_are_loaded_from_pinned_scoreboard_and_typescript_baselines(self) -> None:
        authority = batch.load_authenticated_batch_oracles(self.contract)
        self.assertEqual(authority["frame_count"], 687)
        self.assertEqual(
            authority["typokat"][0],
            {
                "case_id": "conformance/classes/members/accessibility/classPropertyAsPrivate.ts",
                "exit_code": 1,
                "parse_errors": [],
                "diagnostics": [
                    "15:2341",
                    "16:2341",
                    "17:2341",
                    "18:2341",
                    "20:2341",
                    "21:2341",
                    "21:2349",
                    "22:2341",
                    "23:2341",
                ],
                "incompletes": [],
            },
        )
        parse_row = next(
            row for row in authority["typokat"] if row["exit_code"] == 2
        )
        self.assertEqual(parse_row["parse_errors"], ["parse-error"])
        self.assertEqual(parse_row["incompletes"], [])
        unresolved_row = next(
            row
            for row in authority["typokat"]
            if row["incompletes"] == ["unresolved"]
        )
        self.assertEqual(unresolved_row["exit_code"], 3)
        self.assertEqual(unresolved_row["parse_errors"], [])
        self.assertEqual(
            authority["tsgo"][0]["diagnostics"],
            [
                "15:2341",
                "16:2341",
                "17:2341",
                "18:2341",
                "20:2341",
                "21:2341",
                "22:2341",
                "23:2341",
            ],
        )

        with tempfile.TemporaryDirectory() as temporary:
            changed = Path(temporary) / "scoreboard.txt"
            source = HERE.parent / "official-suite/scoreboard.txt"
            changed.write_bytes(source.read_bytes() + b"\n")
            with self.assertRaisesRegex(batch.BatchContractError, "scoreboard.*identity"):
                batch.load_authenticated_batch_oracles(
                    self.contract, scoreboard_path=changed
                )

        with self.assertRaisesRegex(batch.BatchContractError, "baseline.*identity"):
            batch.load_authenticated_batch_oracles(
                self.contract,
                baseline_overrides={
                    "tests/baselines/reference/classPropertyAsPrivate.errors.txt": b"wrong"
                },
            )

        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(batch.BatchContractError, "Git.*baseline"):
                batch.load_authenticated_batch_oracles(
                    self.contract, git_cache_path=Path(temporary)
                )

        one_frame = raw_panel_evidence(1, 1.0, 100_000, 110_000)
        apply_real_oracle(one_frame, authority)
        verified = batch.recompute_panel(
            one_frame, batch.oracle_panel(authority, 1), self.contract
        )
        self.assertEqual(verified.projects, 1)

    def test_frame_receipts_require_unique_local_state_and_zero_retention(self) -> None:
        frames = [frame_receipt(index) for index in range(32)]
        batch.validate_lifecycle_receipts(frames, 32)

        duplicate = copy.deepcopy(frames)
        duplicate[31]["program_identity"] = duplicate[0]["program_identity"]
        with self.assertRaisesRegex(batch.BatchContractError, "program identity"):
            batch.validate_lifecycle_receipts(duplicate, 32)

        leaked = copy.deepcopy(frames)
        leaked[-1]["retained_frame_state"] = 1
        with self.assertRaisesRegex(batch.BatchContractError, "retained"):
            batch.validate_lifecycle_receipts(leaked, 32)

        product_drift = copy.deepcopy(frames)
        product_drift[-1]["shared_product_identity"] = "other"
        with self.assertRaisesRegex(batch.BatchContractError, "shared product"):
            batch.validate_lifecycle_receipts(product_drift, 32)

    def test_next_panel_is_blocked_until_every_prior_gate_passes(self) -> None:
        green = verified_panel(1, 1.0, 100_000, 110_000, self.contract)
        self.assertEqual(batch.next_panel([green], self.contract), 32)
        green_32 = verified_panel(32, 1.2, 100_000, 110_000, self.contract)
        self.assertEqual(batch.next_panel([green, green_32], self.contract), 687)
        green_687 = verified_panel(
            687,
            1.2,
            100_000,
            110_000,
            self.contract,
            typokat_duration_ns=12_000_000_000,
        )
        self.assertIsNone(batch.next_panel([green, green_32, green_687], self.contract))

        red_memory = verified_panel(1, 1.2, 111_000, 110_000, self.contract)
        self.assertIsNone(batch.next_panel([red_memory], self.contract))

    def test_collector_never_launches_a_later_panel_after_failure(self) -> None:
        launched = []
        authority = batch.load_authenticated_batch_oracles(self.contract)

        def red_runner(projects: int):
            launched.append(projects)
            evidence = raw_panel_evidence(projects, 1.2, 111_000, 110_000)
            apply_real_oracle(evidence, authority)
            return evidence

        result = batch.collect_progressive_panels(red_runner, self.contract)
        self.assertEqual(launched, [1])
        self.assertEqual(result["verdict"], "PANEL-FAIL")

        launched.clear()

        def green_runner(projects: int):
            launched.append(projects)
            evidence = raw_panel_evidence(
                projects,
                1.2,
                100_000,
                110_000,
                typokat_duration_ns=12_000_000_000 if projects == 687 else 1_000_000,
            )
            apply_real_oracle(evidence, authority)
            return evidence

        result = batch.collect_progressive_panels(green_runner, self.contract)
        self.assertEqual(launched, [1, 32, 687])
        self.assertEqual(result["verdict"], "PANEL-PASS")
        self.assertNotEqual(result["verdict"], "GO")

    def test_authoritative_oracle_authenticates_each_request_source(self) -> None:
        authority = batch.load_authenticated_batch_oracles(self.contract)
        evidence = raw_panel_evidence(1, 1.0, 100_000, 110_000)
        apply_real_oracle(evidence, authority)
        expected = batch.oracle_panel(authority, 1)
        batch.recompute_panel(evidence, expected, self.contract)

        forged = copy.deepcopy(evidence)
        for sample in forged["samples"]:
            sample["frames"][0]["raw_request"] = json.dumps(
                {"schema": 1, "case_id": sample["frames"][0]["case_id"], "source": "let x = 1;"},
                sort_keys=True,
                separators=(",", ":"),
            )
            refresh_sample_streams(sample)
        with self.assertRaisesRegex(batch.BatchContractError, "source"):
            batch.recompute_panel(forged, expected, self.contract)

        wrong_schema = copy.deepcopy(evidence)
        for sample in wrong_schema["samples"]:
            request = json.loads(sample["frames"][0]["raw_request"])
            request["schema"] = 2
            sample["frames"][0]["raw_request"] = json.dumps(
                request, sort_keys=True, separators=(",", ":")
            )
            refresh_sample_streams(sample)
        with self.assertRaisesRegex(batch.BatchContractError, "schema"):
            batch.recompute_panel(wrong_schema, expected, self.contract)

    def test_speed_and_memory_thresholds_are_strict(self) -> None:
        green = verified_panel(32, 1.1, 100_000, 100_000, self.contract)
        self.assertFalse(
            batch.panel_passes(
                verified_panel(32, 1.0, 100_000, 100_000, self.contract), self.contract
            )
        )
        self.assertFalse(
            batch.panel_passes(
                verified_panel(32, 1.1, 100_001, 100_000, self.contract), self.contract
            )
        )
        self.assertTrue(
            batch.panel_passes(
                verified_panel(32, 1.000_001, 100_000, 100_000, self.contract), self.contract
            )
        )
        self.assertFalse(
            batch.panel_passes(replace(green, p95_ratio=1.0), self.contract)
        )
        self.assertFalse(
            batch.panel_passes(replace(green, bootstrap_lower_95=1.0), self.contract)
        )

        over_limit = verified_panel(32, 1.1, 524_288, 600_000, self.contract)
        self.assertFalse(batch.panel_passes(over_limit, self.contract))

    def test_raw_abba_samples_are_the_only_performance_authority(self) -> None:
        evidence = raw_panel_evidence(32, 1.2, 100_000, 110_000)
        expected = expected_oracles(evidence)
        recomputed = batch.recompute_panel(evidence, expected, self.contract)
        self.assertTrue(batch.panel_passes(recomputed, self.contract))

        wrong_derived = copy.deepcopy(evidence)
        wrong_derived["derived"]["speedup"] = 99.0
        with self.assertRaisesRegex(batch.BatchContractError, "derived"):
            batch.recompute_panel(wrong_derived, expected, self.contract)

        reordered = copy.deepcopy(evidence)
        reordered["samples"][0], reordered["samples"][1] = (
            reordered["samples"][1],
            reordered["samples"][0],
        )
        with self.assertRaisesRegex(batch.BatchContractError, "ABBA"):
            batch.recompute_panel(reordered, expected, self.contract)

        reused_pid = copy.deepcopy(evidence)
        reused_pid["samples"][-1]["pid"] = reused_pid["samples"][0]["pid"]
        with self.assertRaisesRegex(batch.BatchContractError, "PID"):
            batch.recompute_panel(reused_pid, expected, self.contract)

        missing_warmup = copy.deepcopy(evidence)
        missing_warmup["warmups"].pop()
        with self.assertRaisesRegex(batch.BatchContractError, "warmup"):
            batch.recompute_panel(missing_warmup, expected, self.contract)

        overlapping_processes = copy.deepcopy(evidence)
        overlapping_processes["samples"][1]["start_ns"] = overlapping_processes["samples"][0]["start_ns"]
        with self.assertRaisesRegex(batch.BatchContractError, "chronology"):
            batch.recompute_panel(overlapping_processes, expected, self.contract)

        forged_rss = copy.deepcopy(evidence)
        forged_rss["samples"][0]["rss_kib"] += 1
        with self.assertRaisesRegex(batch.BatchContractError, "RSS"):
            batch.recompute_panel(forged_rss, expected, self.contract)

    def test_raw_process_and_jsonl_protocol_fail_closed(self) -> None:
        sample = raw_panel_evidence(32, 1.2, 100_000, 110_000)["samples"][0]
        batch.validate_raw_process(sample, 32, self.contract)

        for label, mutate in (
            ("response", lambda value: value["frames"].pop()),
            ("case", lambda value: value["frames"][-1].update(case_id="case-0")),
            ("group", lambda value: value.update(process_group_empty=False)),
            ("stream", lambda value: value.update(stdout_complete=False)),
            ("binary", lambda value: value.update(executable_after_sha256="0" * 64)),
        ):
            broken = copy.deepcopy(sample)
            mutate(broken)
            with self.subTest(label=label), self.assertRaises(batch.BatchContractError):
                batch.validate_raw_process(broken, 32, self.contract)

    def test_session_collector_reuses_the_cold_process_supervisor(self) -> None:
        self.assertIs(batch.run_process, cold.run_process)
        self.assertIs(batch.execution_conditions, cold.execution_conditions)

    def test_single_batch_and_cold_linkage_is_recomputed(self) -> None:
        evidence = {
            "candidate_commit": "candidate",
            "cold_candidate_commit": "candidate",
            "typokat": {
                "single_binary_sha256": "2" * 64,
                "batch_binary_sha256": "2" * 64,
                "single_oracle_sha256": "8" * 64,
                "cold_oracle_sha256": "8" * 64,
            },
            "tsgo": {
                "single_binary_sha256": "3" * 64,
                "batch_binary_sha256": "3" * 64,
                "single_oracle_sha256": "9" * 64,
                "cold_oracle_sha256": "9" * 64,
                "upstream_revision": "2bd066d87f5bafd315be9f40889d0a60b9e58e0b",
                "authenticated_cold_binary_sha256": self.contract["tsgo"]["cold_binary_sha256"],
            },
        }
        batch.verify_runner_linkage(evidence, self.contract)

        for label, mutate in (
            ("binary", lambda value: value["typokat"].update(batch_binary_sha256="0" * 64)),
            ("commit", lambda value: value.update(cold_candidate_commit="old")),
            ("oracle", lambda value: value["tsgo"].update(cold_oracle_sha256="0" * 64)),
        ):
            broken = copy.deepcopy(evidence)
            mutate(broken)
            with self.subTest(label=label), self.assertRaises(batch.BatchContractError):
                batch.verify_runner_linkage(broken, self.contract)

    def test_raw_frame_outputs_drive_oracles_and_lifecycle_controls(self) -> None:
        evidence = raw_panel_evidence(1, 1.0, 100_000, 110_000)
        expected = expected_oracles(evidence)
        batch.verify_raw_oracles(evidence, expected, self.contract)
        batch.verify_negative_controls(evidence["negative_controls"], self.contract)

        forged = copy.deepcopy(evidence)
        forged["samples"][0]["stdout"] = "wrong\n"
        with self.assertRaises(batch.BatchContractError):
            batch.verify_raw_oracles(forged, expected, self.contract)

        missing_control = copy.deepcopy(evidence["negative_controls"])
        del missing_control["tsgo"]["reuse-checker"]
        with self.assertRaises(batch.BatchContractError):
            batch.verify_negative_controls(missing_control, self.contract)

        wrong_binary = copy.deepcopy(evidence["negative_controls"])
        wrong_binary["typokat"]["reuse-session"]["executable_after_sha256"] = "0" * 64
        with self.assertRaisesRegex(batch.BatchContractError, "binary"):
            batch.verify_negative_controls(wrong_binary, self.contract)

        missing_activation = copy.deepcopy(evidence["negative_controls"])
        missing_activation["tsgo"]["retain-frame"]["fault_hook_activations"] = 0
        with self.assertRaisesRegex(batch.BatchContractError, "activation"):
            batch.verify_negative_controls(missing_activation, self.contract)

    def test_internal_route_intervals_are_raw_bounded_and_reconciled(self) -> None:
        evidence = raw_panel_evidence(687, 1.2, 100_000, 110_000)
        sample = next(item for item in evidence["samples"] if item["tool"] == "typokat")
        batch.verify_route_intervals(sample, 687)

        overlap = copy.deepcopy(sample)
        overlap["frames"][1]["route_start_ns"] = overlap["frames"][0]["route_start_ns"]
        with self.assertRaisesRegex(batch.BatchContractError, "overlap"):
            batch.verify_route_intervals(overlap, 687)

        outside = copy.deepcopy(sample)
        outside["frames"][-1]["route_end_ns"] = outside["end_ns"] + 1
        with self.assertRaisesRegex(batch.BatchContractError, "external"):
            batch.verify_route_intervals(outside, 687)

        duplicate_init = copy.deepcopy(sample)
        duplicate_init["frames"][1]["product_initializations"] = 1
        with self.assertRaisesRegex(batch.BatchContractError, "initialization"):
            batch.verify_route_intervals(duplicate_init, 687)

        missing_init = copy.deepcopy(sample)
        missing_init["frames"][0]["product_initializations"] = 0
        with self.assertRaisesRegex(batch.BatchContractError, "initialization"):
            batch.verify_route_intervals(missing_init, 687)

        misplaced_init = copy.deepcopy(sample)
        misplaced_init["frames"][0]["product_initializations"] = 0
        misplaced_init["frames"][1]["product_initializations"] = 1
        with self.assertRaisesRegex(batch.BatchContractError, "first frame"):
            batch.verify_route_intervals(misplaced_init, 687)

        omitted = copy.deepcopy(sample)
        omitted["frames"].pop()
        with self.assertRaisesRegex(batch.BatchContractError, "frame"):
            batch.verify_route_intervals(omitted, 687)

        gap = copy.deepcopy(sample)
        gap["frames"][0]["phases"]["binding"]["start_ns"] += 1
        with self.assertRaisesRegex(batch.BatchContractError, "phase"):
            batch.verify_route_intervals(gap, 687)

        narrowed = copy.deepcopy(sample)
        narrowed["frames"][0]["route_start_ns"] += 1
        with self.assertRaisesRegex(batch.BatchContractError, "boundary"):
            batch.verify_route_intervals(narrowed, 687)

        missing_metrics = copy.deepcopy(sample)
        del missing_metrics["frames"][0]["materialized_product_owner_count"]
        with self.assertRaisesRegex(batch.BatchContractError, "owner"):
            batch.verify_route_intervals(missing_metrics, 687)

        missing_parse = copy.deepcopy(sample)
        del missing_parse["frames"][0]["phases"]["parse_prepare"]
        with self.assertRaisesRegex(batch.BatchContractError, "phase"):
            batch.verify_route_intervals(missing_parse, 687)

    def test_allocation_probe_is_separate_from_timed_evidence(self) -> None:
        receipt = {
            "tool": "typokat",
            "timed": False,
            "instrumentation": "counting-allocator",
            "allocation_count": 123,
            "binary_sha256": "2" * 64,
        }
        batch.verify_allocation_probe(receipt, self.contract)
        for mutation in (
            lambda value: value.update(timed=True),
            lambda value: value.update(allocation_count=0),
            lambda value: value.update(instrumentation="none"),
        ):
            broken = copy.deepcopy(receipt)
            mutation(broken)
            with self.assertRaises(batch.BatchContractError):
                batch.verify_allocation_probe(broken, self.contract)

    def test_abba_statistics_use_p95_walls_and_real_block_bootstrap(self) -> None:
        samples = []
        pid = 1
        for block in range(15):
            for slot, (tool, wall) in enumerate(
                (
                    ("typokat", 10 + block),
                    ("tsgo", 15 + block * 2),
                    ("tsgo", 16 + block * 2),
                    ("typokat", 11 + block),
                )
            ):
                samples.append(
                    {
                        "block": block,
                        "slot": slot,
                        "tool": tool,
                        "pid": pid,
                        "start_ns": 0,
                        "end_ns": wall,
                    }
                )
                pid += 1
        observed = batch.recompute_abba_statistics(samples, self.contract)
        cold_blocks = [
            [
                {"tool": sample["tool"], "wall_seconds": sample["end_ns"]}
                for sample in samples[block * 4 : block * 4 + 4]
            ]
            for block in range(15)
        ]
        typokat = [sample["end_ns"] for sample in samples if sample["tool"] == "typokat"]
        tsgo = [sample["end_ns"] for sample in samples if sample["tool"] == "tsgo"]
        self.assertEqual(
            observed["p95_ratio"],
            cold.percentile(tsgo, 0.95) / cold.percentile(typokat, 0.95),
        )
        self.assertEqual(
            observed["bootstrap_lower_95"],
            cold.bootstrap_block_speedup_lower(
                cold_blocks,
                self.contract["bootstrap_resamples"],
                self.contract["bootstrap_seed"],
            ),
        )

    def test_concurrent_1_2_32_sessions_are_isolated_deterministic_and_bounded(self) -> None:
        evidence = concurrency_evidence()
        batch.verify_concurrency(evidence, self.contract)
        for count in (1, 2, 32):
            self.assertEqual(evidence[str(count)]["retained_sessions"], 0)

        reused = copy.deepcopy(evidence)
        reused["32"]["session_identities"][-1] = reused["32"]["session_identities"][0]
        with self.assertRaisesRegex(batch.BatchContractError, "identity"):
            batch.verify_concurrency(reused, self.contract)

        nondeterministic = copy.deepcopy(evidence)
        nondeterministic["2"]["output_sha256"] = "f" * 64
        with self.assertRaisesRegex(batch.BatchContractError, "determin"):
            batch.verify_concurrency(nondeterministic, self.contract)

        sequential = copy.deepcopy(evidence)
        for index, session in enumerate(sequential["32"]["sessions"]):
            session["start_ns"] = index * 10
            session["end_ns"] = index * 10 + 5
        refresh_concurrency_raw(sequential["32"])
        with self.assertRaisesRegex(batch.BatchContractError, "concurrent"):
            batch.verify_concurrency(sequential, self.contract)

        wrong_binary = copy.deepcopy(evidence)
        wrong_binary["2"]["executable_after_sha256"] = "0" * 64
        with self.assertRaisesRegex(batch.BatchContractError, "binary"):
            batch.verify_concurrency(wrong_binary, self.contract)

        live_weak_state = copy.deepcopy(evidence)
        live_weak_state["32"]["sessions"][-1]["cache_weak_alive_after"] = True
        refresh_concurrency_raw(live_weak_state["32"])
        with self.assertRaisesRegex(batch.BatchContractError, "cache"):
            batch.verify_concurrency(live_weak_state, self.contract)

        changed_semantics = copy.deepcopy(evidence)
        changed_semantics["32"]["sessions"][-1]["semantic"]["diagnostics"] = ["1:9999"]
        refresh_concurrency_raw(changed_semantics["32"])
        with self.assertRaisesRegex(batch.BatchContractError, "determin"):
            batch.verify_concurrency(changed_semantics, self.contract)

        missing_weak = copy.deepcopy(evidence)
        del missing_weak["1"]["sessions"][0]["session_weak_alive_after"]
        refresh_concurrency_raw(missing_weak["1"])
        with self.assertRaisesRegex(batch.BatchContractError, "session"):
            batch.verify_concurrency(missing_weak, self.contract)

        missing_identity = copy.deepcopy(evidence)
        missing_identity["1"]["sessions"][0]["identity"] = None
        missing_identity["1"]["session_identities"][0] = None
        refresh_concurrency_raw(missing_identity["1"])
        with self.assertRaisesRegex(batch.BatchContractError, "identity"):
            batch.verify_concurrency(missing_identity, self.contract)

        invalid_boundary = copy.deepcopy(evidence)
        invalid_boundary["1"]["sessions"][0]["barrier_ns"] = None
        refresh_concurrency_raw(invalid_boundary["1"])
        with self.assertRaisesRegex(batch.BatchContractError, "boundary"):
            batch.verify_concurrency(invalid_boundary, self.contract)

    def test_687_panel_must_beat_the_authenticated_typokat_baseline(self) -> None:
        candidate = verified_panel(
            687,
            1.2,
            100_000,
            110_000,
            self.contract,
            typokat_duration_ns=12_000_000_000,
        )
        self.assertTrue(batch.panel_passes(candidate, self.contract))
        self.assertLess(
            candidate.typokat_route_wall_ns,
            self.contract["current_typokat_baseline"]["wall_us"] * 1000,
        )
        too_slow = verified_panel(
            687,
            1.2,
            100_000,
            110_000,
            self.contract,
            typokat_duration_ns=13_000_000_000,
        )
        self.assertFalse(batch.panel_passes(too_slow, self.contract))
        self.assertGreaterEqual(
            too_slow.typokat_route_wall_ns,
            self.contract["current_typokat_baseline"]["wall_us"] * 1000,
        )
        too_large = verified_panel(
            687,
            1.2,
            140_800,
            150_000,
            self.contract,
            typokat_duration_ns=12_000_000_000,
        )
        self.assertFalse(batch.panel_passes(too_large, self.contract))
        equal_old_peak = verified_panel(
            687,
            1.2,
            self.contract["current_typokat_baseline"]["peak_rss_bytes"] // 1024,
            150_000,
            self.contract,
            typokat_duration_ns=12_000_000_000,
        )
        self.assertFalse(batch.panel_passes(equal_old_peak, self.contract))

        one_outlier_raw = raw_panel_evidence(
            687, 1.2, 100_000, 150_000, typokat_duration_ns=12_000_000_000
        )
        typokat_sample = next(
            item for item in one_outlier_raw["samples"] if item["tool"] == "typokat"
        )
        typokat_sample["rss_kib"] = 140_800
        typokat_sample["time_report"] = "Maximum resident set size (kbytes): 140800\n"
        one_outlier = batch.recompute_panel(
            one_outlier_raw, expected_oracles(one_outlier_raw), self.contract
        )
        self.assertFalse(batch.panel_passes(one_outlier, self.contract))

    def test_timing_is_ineligible_without_oracles_and_execution_controls(self) -> None:
        preconditions = {
            "oracle_authority_sha256": "6" * 64,
            "control_receipts_sha256": "7" * 64,
            "lease_argv": ["cpu-lease", "run", "-n", "2", "--no-smt", "--"],
            "lease_vcpus": 2,
            "cpuset": [8, 10],
            "thread_siblings": {"8": [8, 9], "10": [10, 11]},
            "affinity_before": [8, 10],
            "affinity_after": [8, 10],
            "environment": {"LANG": "C", "LC_ALL": "C", "TZ": "UTC"},
            "address_space_rlimit": [1 << 30, 1 << 30],
        }
        batch.validate_timing_preconditions(preconditions, self.contract)
        mutations = {
            "oracle_authority_sha256": "0" * 64,
            "control_receipts_sha256": "0" * 64,
            "lease_argv": ["cpu-lease", "run", "-n", "2", "--"],
            "lease_vcpus": 1,
            "cpuset": [8, 9],
            "affinity_after": [8],
            "environment": {"RAYON_NUM_THREADS": "1"},
            "address_space_rlimit": [0, 0],
        }
        for key, bad_value in mutations.items():
            red = copy.deepcopy(preconditions)
            red[key] = bad_value
            with self.assertRaises(batch.BatchContractError):
                batch.validate_timing_preconditions(red, self.contract)


def protocol_info(tool: str) -> dict[str, object]:
    return {
        "schema": 1,
        "tool": tool,
        "source_sha256": "1" * 64,
        "binary_sha256": "2" * 64,
        "upstream_revision": "2bd066d87f5bafd315be9f40889d0a60b9e58e0b" if tool == "tsgo" else "candidate",
        "profile_sha256": "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d",
        "profile_file_count": 82,
        "program_per_frame": True,
        "checker_per_frame": True,
        "shared_user_sources": False,
        "incremental": False,
        "single_threaded": True,
        "compiler_flags": [
            "--strict",
            "--target",
            "es2025",
            "--module",
            "preserve",
            "--noEmit",
            "--pretty",
            "false",
            "--ignoreConfig",
        ],
    }


def frame_receipt(index: int) -> dict[str, object]:
    return {
        "ordinal": index,
        "program_identity": f"program-{index}",
        "checker_identity": f"checker-{index}",
        "semantic_identity": f"semantic-{index}",
        "shared_product_identity": "shared-product",
        "retained_frame_state": 0,
    }


def expected_oracles(evidence: dict[str, object]) -> dict[str, object]:
    source_sample = next(item for item in evidence["samples"] if item["tool"] == "typokat")
    result = {
        "request_identities": [
            {
                "case_id": frame["case_id"],
                "source_bytes": len(json.loads(frame["raw_request"])["source"].encode()),
                "source_sha256": hashlib.sha256(
                    json.loads(frame["raw_request"])["source"].encode()
                ).hexdigest(),
            }
            for frame in source_sample["frames"]
        ]
    }
    for tool in ("typokat", "tsgo"):
        sample = next(item for item in evidence["samples"] if item["tool"] == tool)
        result[tool] = {
            "returncode": 0,
            "ordered_semantic_sha256": semantic_oracle_sha256(sample["frames"]),
        }
    return result


def semantic_oracle_sha256(frames: list[dict[str, object]]) -> str:
    rows = [{"case_id": frame["case_id"], "semantic": frame["semantic"]} for frame in frames]
    raw = json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(raw).hexdigest()


def verified_panel(
    projects: int,
    ratio: float,
    typokat_rss: int,
    tsgo_rss: int,
    contract: dict[str, object],
    *,
    typokat_duration_ns: int = 1_000_000,
):
    evidence = raw_panel_evidence(
        projects,
        ratio,
        typokat_rss,
        tsgo_rss,
        typokat_duration_ns=typokat_duration_ns,
    )
    return batch.recompute_panel(evidence, expected_oracles(evidence), contract)


def raw_panel_evidence(
    projects: int,
    ratio: float,
    typokat_rss: int,
    tsgo_rss: int,
    *,
    typokat_duration_ns: int = 1_000_000,
) -> dict[str, object]:
    samples = []
    pid = 10_000
    slots = ("typokat", "tsgo", "tsgo", "typokat")
    warmups = []
    warmup_pid = 9_000
    for ordinal in range(3):
        for tool in ("typokat", "tsgo"):
            warmup_start = warmup_pid * 100_000_000_000
            warmups.append(
                {
                    "ordinal": ordinal,
                    "tool": tool,
                    "pid": warmup_pid,
                    "start_ns": warmup_start,
                    "end_ns": warmup_start + 1_000_000,
                    "returncode": 0,
                    "executable_before_sha256": "2" * 64,
                    "executable_after_sha256": "2" * 64,
                    "process_group_empty": True,
                }
            )
            warmup_pid += 1
    for block in range(15):
        for slot, tool in enumerate(slots):
            duration = (
                typokat_duration_ns
                if tool == "typokat"
                else round(typokat_duration_ns * ratio)
            )
            frames = []
            frame_width = max(1, duration // projects)
            process_start = pid * 100_000_000_000
            for ordinal in range(projects):
                start = process_start + ordinal * frame_width
                end = start + frame_width
                init_end = start + (max(1, frame_width // 10) if tool == "typokat" and ordinal == 0 else 0)
                binding_end = start + max(2, frame_width // 3)
                publication_end = start + max(3, (frame_width * 2) // 3)
                frame = {
                    **frame_receipt(ordinal),
                    "case_id": f"case-{ordinal}",
                    "semantic": {
                        "exit_code": 0,
                        "parse_errors": [],
                        "diagnostics": [],
                        "incompletes": [],
                    },
                    "route_start_ns": start,
                    "route_end_ns": end,
                    "product_initializations": 1 if tool == "typokat" and ordinal == 0 else 0,
                    "phases": {
                        "product_initialization": {"start_ns": start, "end_ns": init_end},
                        "parse_prepare": {"start_ns": init_end, "end_ns": init_end},
                        "binding": {"start_ns": init_end, "end_ns": binding_end},
                        "publication_check": {
                            "start_ns": binding_end,
                            "end_ns": publication_end,
                        },
                        "release": {"start_ns": publication_end, "end_ns": end},
                    },
                }
                if tool == "typokat":
                    frame.update(
                        materialized_product_owner_count=10,
                        session_weak_alive_after=False,
                        cache_weak_alive_after=False,
                    )
                else:
                    frame["retained_frame_handles"] = 0
                request = json.dumps(
                    {"schema": 1, "case_id": frame["case_id"], "source": "let value = 1;"},
                    sort_keys=True,
                    separators=(",", ":"),
                )
                response = json.dumps(frame, sort_keys=True, separators=(",", ":"))
                frame.update(
                    raw_request=request,
                    raw_response=response,
                    raw_response_sha256=hashlib.sha256(response.encode()).hexdigest(),
                )
                frames.append(frame)
            request_stream = "".join(frame["raw_request"] + "\n" for frame in frames)
            response_stream = "".join(frame["raw_response"] + "\n" for frame in frames)
            rss = typokat_rss if tool == "typokat" else tsgo_rss
            samples.append(
                {
                    "block": block,
                    "slot": slot,
                    "tool": tool,
                    "pid": pid,
                    "start_ns": process_start,
                    "end_ns": process_start + duration + 1,
                    "returncode": 0,
                    "stdout": response_stream,
                    "stderr": "",
                    "stdout_complete": True,
                    "stderr_complete": True,
                    "timed_out": False,
                    "process_group_empty": True,
                    "executable_before_sha256": "2" * 64,
                    "executable_after_sha256": "2" * 64,
                    "address_space_limit_bytes": 1 << 30,
                    "request_stream": request_stream,
                    "response_stream": response_stream,
                    "request_stream_sha256": hashlib.sha256(request_stream.encode()).hexdigest(),
                    "response_stream_sha256": hashlib.sha256(response_stream.encode()).hexdigest(),
                    "time_report": f"Maximum resident set size (kbytes): {rss}\n",
                    "rss_kib": rss,
                    "frames": frames,
                }
            )
            pid += 1
    return {
        "projects": projects,
        "warmups": warmups,
        "samples": samples,
        "derived": {
            "speedup": ratio,
            "p95_ratio": ratio,
            "bootstrap_lower_95": ratio,
        },
        "negative_controls": {
            "tsgo": {
                "reuse-checker": negative_control("reuse-checker"),
                "retain-frame": negative_control("retain-frame"),
            },
            "typokat": {
                "reuse-session": negative_control("reuse-session"),
                "retain-cache": negative_control("retain-cache"),
            },
        },
    }


def apply_real_oracle(evidence: dict[str, object], authority: dict[str, object]) -> None:
    for sample in evidence["samples"]:
        for ordinal, frame in enumerate(sample["frames"]):
            oracle = authority[sample["tool"]][ordinal]
            source = authority["frames"][ordinal]["source"]
            frame["case_id"] = oracle["case_id"]
            frame["semantic"] = {
                key: value for key, value in oracle.items() if key != "case_id"
            }
            frame["raw_request"] = json.dumps(
                {"schema": 1, "case_id": frame["case_id"], "source": source},
                sort_keys=True,
                separators=(",", ":"),
            )
            response_source = {
                key: value
                for key, value in frame.items()
                if key not in {"raw_request", "raw_response", "raw_response_sha256"}
            }
            frame["raw_response"] = json.dumps(
                response_source, sort_keys=True, separators=(",", ":")
            )
            frame["raw_response_sha256"] = hashlib.sha256(
                frame["raw_response"].encode()
            ).hexdigest()
        refresh_sample_streams(sample)


def refresh_sample_streams(sample: dict[str, object]) -> None:
    request_stream = "".join(frame["raw_request"] + "\n" for frame in sample["frames"])
    response_stream = "".join(frame["raw_response"] + "\n" for frame in sample["frames"])
    sample["request_stream"] = request_stream
    sample["response_stream"] = response_stream
    sample["stdout"] = response_stream
    sample["request_stream_sha256"] = hashlib.sha256(request_stream.encode()).hexdigest()
    sample["response_stream_sha256"] = hashlib.sha256(response_stream.encode()).hexdigest()


def negative_control(name: str) -> dict[str, object]:
    request = json.dumps({"schema": 1, "fault": name}, sort_keys=True, separators=(",", ":"))
    response = json.dumps(
        {"schema": 1, "fault": name, "fault_hook_activations": 1, "rejected": True},
        sort_keys=True,
        separators=(",", ":"),
    )
    return {
        "fault": name,
        "executable_before_sha256": "2" * 64,
        "executable_after_sha256": "2" * 64,
        "returncode": 2,
        "raw_request": request + "\n",
        "stdout": response + "\n",
        "stderr": "",
        "fault_hook_activations": 1,
        "process_group_empty": True,
    }


def concurrency_evidence() -> dict[str, object]:
    result = {}
    for count in (1, 2, 32):
        sessions = [
            {
                "identity": f"session-{count}-{index}",
                "publisher_identity": f"publisher-{count}-{index}",
                "cache_identity": f"cache-{count}-{index}",
                "start_ns": 0,
                "barrier_ns": 50,
                "end_ns": 100,
                "session_weak_alive_after": False,
                "cache_weak_alive_after": False,
                "semantic": {
                    "exit_code": 0,
                    "parse_errors": [],
                    "diagnostics": [],
                    "incompletes": [],
                },
                "raw_output_sha256": "",
            }
            for index in range(count)
        ]
        result[str(count)] = {
            "sessions": sessions,
            "session_identities": [f"session-{count}-{index}" for index in range(count)],
            "publisher_identities": [f"publisher-{count}-{index}" for index in range(count)],
            "cache_identities": [f"cache-{count}-{index}" for index in range(count)],
            "output_sha256": "",
            "retained_sessions": 0,
            "retained_caches": 0,
            "peak_rss_kib": 100_000,
            "time_report": "Maximum resident set size (kbytes): 100000\n",
            "stdout": "",
            "stdout_sha256": "",
            "executable_before_sha256": "2" * 64,
            "executable_after_sha256": "2" * 64,
            "process_group_empty": True,
        }
        refresh_concurrency_raw(result[str(count)])
    return result


def refresh_concurrency_raw(record: dict[str, object]) -> None:
    semantic_rows = []
    for session in record["sessions"]:
        semantic_raw = json.dumps(session["semantic"], sort_keys=True, separators=(",", ":"))
        session["raw_output_sha256"] = hashlib.sha256(semantic_raw.encode()).hexdigest()
        semantic_rows.append(session["semantic"])
    raw = json.dumps(record["sessions"], sort_keys=True, separators=(",", ":")) + "\n"
    record["stdout"] = raw
    record["stdout_sha256"] = hashlib.sha256(raw.encode()).hexdigest()
    record["output_sha256"] = hashlib.sha256(
        json.dumps(semantic_rows, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


if __name__ == "__main__":
    unittest.main()

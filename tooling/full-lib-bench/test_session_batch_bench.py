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
            self.contract["tsgo"]["cold_binary_relative_path"],
            "tooling/full-lib-bench/.stage/tsgo-7.0.2/lib/tsc",
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
        self.assertEqual(
            self.contract["single_project"],
            {
                "mode_argument": "--single-project",
                "rows": ["fast-clean", "fast-errors", "collision", "fanout"],
                "file_counts": [1, 1, 2, 32],
                "request_schema": 1,
                "normalization": "relative-path:line:column:code",
                "timeout_seconds": 60,
                "require_exact_ordered_files": True,
            },
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
        self.assertEqual(lifecycle["frame_dispositions"], ["semantic", "parse-rejected"])
        self.assertEqual(
            lifecycle["parse_rejected_identity"], "explicit-absence-no-fabrication"
        )
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

    def test_batch_oracles_use_external_exact_flags_cold_tsgo_truth(self) -> None:
        oracles = self.contract["oracles"]
        self.assertEqual(
            oracles["tsgo_batch_oracle"],
            "authenticated-exact-flags-cold-tsgo-artifact",
        )
        self.assertEqual(
            oracles["typescript_baseline_authority"],
            "source-and-version-evidence-only",
        )
        self.assertEqual(
            oracles["batch_oracle_rule"],
            "ordered-normalized-exact-flags-cold-tsgo-outcomes",
        )
        self.assertEqual(oracles["exact_flags_truth_tools"], ["tsgo-7.0.2"])
        self.assertEqual(oracles["exact_flags_truth_mode"], "fresh-process-per-frame")
        self.assertEqual(
            oracles["tsgo_batch_oracle_sha256"],
            "f56e73cc9a60c4bcfd57fe291d72417c74ff4c480f81ee04abee70272e20aa3e",
        )
        self.assertEqual(
            oracles["tsgo_batch_oracle_ordered_semantic_sha256"],
            "dfb565aeb79264a18293652a5b6d4ff6fc14c5f7f6ccbbd1e771ccdbd60ce6db",
        )
        self.assertEqual(oracles["cross_tool_agreement_scope"], "cold-four-row-panel-only")
        self.assertFalse(oracles["require_full_batch_cross_tool_agreement"])
        self.assertTrue(oracles["require_cold_cross_tool_agreement"])
        self.assertEqual(
            oracles["cross_tool_rows"],
            ["fast-clean", "fast-errors", "collision", "fanout"],
        )
        self.assertEqual(oracles["tsc_tsgo_matching_prefix_frames"], 76)
        self.assertEqual(
            oracles["tsc_tsgo_first_mismatch_case_id"],
            "conformance/classes/members/privateNames/privateNameDuplicateField.ts",
        )
        self.assertEqual(
            oracles["tsc_tsgo_first_mismatch_source_sha256"],
            "1932666cae238490328c1a3e1535c90ebd061f22a9f567efdede116566201a2f",
        )

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
                "2:2564",
                "15:2341",
                "16:2341",
                "17:2322",
                "17:2341",
                "18:2341",
                "20:2341",
                "21:2341",
                "21:2721",
                "22:2322",
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

        one_frame = raw_panel_evidence(1, 1.0, 100_000, 110_000)
        apply_real_oracle(one_frame, authority)
        verified = batch.recompute_panel(
            one_frame, batch.oracle_panel(authority, 1), self.contract
        )
        self.assertEqual(verified.projects, 1)

    def test_exact_flags_tsgo_artifact_authenticates_every_raw_cold_invocation(self) -> None:
        exact = batch.load_authenticated_exact_flags_tsgo_oracle(self.contract)
        self.assertEqual(len(exact), 687)
        self.assertEqual([row["ordinal"] for row in exact], list(range(687)))
        self.assertEqual(
            exact[0]["normalized_semantic"]["diagnostics"],
            [
                "2:2564",
                "15:2341",
                "16:2341",
                "17:2322",
                "17:2341",
                "18:2341",
                "20:2341",
                "21:2341",
                "21:2721",
                "22:2322",
                "22:2341",
                "23:2341",
            ],
        )
        self.assertEqual(
            exact[5]["normalized_semantic"]["diagnostics"].count("20:2341"), 2
        )

        oracle_path = HERE / "exact-flags-tsgo-oracle.json"
        with tempfile.TemporaryDirectory() as temporary:
            changed = Path(temporary) / "oracle.json"
            changed.write_bytes(oracle_path.read_bytes() + b"\n")
            with self.assertRaisesRegex(batch.BatchContractError, "exact-flags.*identity"):
                batch.load_authenticated_exact_flags_tsgo_oracle(
                    self.contract, oracle_path=changed
                )

    def test_exact_flags_tsgo_artifact_rejects_rehashed_internal_forgery(self) -> None:
        oracle_path = HERE / "exact-flags-tsgo-oracle.json"
        original = json.loads(oracle_path.read_bytes())

        def swap_first_two(value: dict[str, object]) -> None:
            value["frames"][0], value["frames"][1] = value["frames"][1], value["frames"][0]

        mutations = (
            ("binary", lambda value: value["tool"].update(binary_sha256="0" * 64)),
            ("source", lambda value: value["frames"][0].update(source_sha256="0" * 64)),
            ("command", lambda value: value["frames"][0]["argv"].append("--skipLibCheck")),
            ("stdout", lambda value: value["frames"][0].update(stdout="")),
            ("stderr", lambda value: value["frames"][0].update(stderr="forged")),
            ("returncode", lambda value: value["frames"][0].update(returncode=0)),
            ("profile", lambda value: value["profile"].update(length_framed_sha256="0" * 64)),
            ("flags", lambda value: value["flags"].append("--skipLibCheck")),
            ("process-mode", lambda value: value["execution_contract"].update(mode="batch")),
            ("frame-order", swap_first_two),
            (
                "semantic",
                lambda value: value["frames"][0]["normalized_semantic"]["diagnostics"].append(
                    "999:9999"
                ),
            ),
            (
                "semantic-digest",
                lambda value: value.update(ordered_normalized_semantic_sha256="0" * 64),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary:
                candidate = copy.deepcopy(original)
                mutate(candidate)
                raw = json.dumps(
                    candidate, sort_keys=True, separators=(",", ":"), ensure_ascii=False
                ).encode() + b"\n"
                changed = Path(temporary) / "oracle.json"
                changed.write_bytes(raw)
                contract = copy.deepcopy(self.contract)
                contract["oracles"]["tsgo_batch_oracle_sha256"] = hashlib.sha256(raw).hexdigest()
                with self.assertRaises(batch.BatchContractError):
                    batch.load_authenticated_exact_flags_tsgo_oracle(
                        contract, oracle_path=changed
                    )

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

        parse_rejected = frame_receipt(32)
        parse_rejected.update(
            lifecycle_disposition="parse-rejected",
            program_identity=None,
            checker_identity=None,
            semantic_identity=None,
            semantic={
                "exit_code": 2,
                "parse_errors": ["parse-error"],
                "diagnostics": [],
                "incompletes": [],
            },
        )
        batch.validate_lifecycle_receipts([*frames, parse_rejected], 33)

        fabricated = copy.deepcopy(parse_rejected)
        fabricated["checker_identity"] = "parse-rejected-checker-32"
        with self.assertRaisesRegex(batch.BatchContractError, "parse-rejected.*identity"):
            batch.validate_lifecycle_receipts([*frames, fabricated], 33)

        false_rejection = copy.deepcopy(parse_rejected)
        false_rejection["semantic"] = {
            "exit_code": 0,
            "parse_errors": [],
            "diagnostics": [],
            "incompletes": [],
        }
        with self.assertRaisesRegex(batch.BatchContractError, "parse-rejected.*semantic"):
            batch.validate_lifecycle_receipts([*frames, false_rejection], 33)

        mislabeled_semantic = copy.deepcopy(frames)
        mislabeled_semantic[-1]["semantic"] = {
            "exit_code": 2,
            "parse_errors": ["parse-error"],
            "diagnostics": [],
            "incompletes": [],
        }
        with self.assertRaisesRegex(batch.BatchContractError, "semantic.*parse"):
            batch.validate_lifecycle_receipts(mislabeled_semantic, 32)

        missing = copy.deepcopy(frames)
        missing[-1]["semantic_identity"] = None
        with self.assertRaisesRegex(batch.BatchContractError, "semantic identity"):
            batch.validate_lifecycle_receipts(missing, 32)

    def test_parse_rejected_route_requires_absent_session_receipts(self) -> None:
        evidence = raw_panel_evidence(1, 1.0, 100_000, 110_000)
        sample = next(row for row in evidence["samples"] if row["tool"] == "typokat")
        frame = sample["frames"][0]
        frame.update(
            lifecycle_disposition="parse-rejected",
            program_identity=None,
            checker_identity=None,
            semantic_identity=None,
            materialized_product_owner_count=None,
            session_weak_alive_after=None,
            cache_weak_alive_after=None,
            semantic={
                "exit_code": 2,
                "parse_errors": ["parse-error"],
                "diagnostics": [],
                "incompletes": [],
            },
        )
        batch.validate_lifecycle_receipts(sample["frames"], 1)
        batch.verify_route_intervals(sample, 1)

        for field, value in (
            ("materialized_product_owner_count", 0),
            ("session_weak_alive_after", False),
            ("cache_weak_alive_after", False),
        ):
            with self.subTest(field=field):
                fabricated = copy.deepcopy(sample)
                fabricated["frames"][0][field] = value
                with self.assertRaisesRegex(batch.BatchContractError, "parse-rejected.*session"):
                    batch.verify_route_intervals(fabricated, 1)

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

    def test_single_project_linkage_recomputes_exact_raw_cold_projects(self) -> None:
        evidence = single_project_evidence(self.contract)
        receipt = batch.verify_single_project_linkage(evidence, self.contract)

        self.assertEqual(
            receipt,
            {
                "verdict": "LINKAGE-PASS",
                "candidate_commit": "a" * 40,
                "candidate_tree": "b" * 40,
                "rows": ["fast-clean", "fast-errors", "collision", "fanout"],
                "file_counts": [1, 1, 2, 32],
                "oracle_sha256": single_project_oracle_sha256(self.contract),
            },
        )
        self.assertNotEqual(receipt["verdict"], "GO")

    def test_single_project_linkage_rejects_file_inventory_forgery(self) -> None:
        mutations = {
            "dropped": lambda files: files.pop(),
            "reordered": lambda files: files.reverse(),
            "duplicated": lambda files: files.__setitem__(1, copy.deepcopy(files[0])),
            "renamed": lambda files: files[0].update(path="collision/renamed.ts"),
            "concatenated": lambda files: (
                files[0].update(source=files[0]["source"] + files[1]["source"]),
                files[1].update(source=""),
            ),
        }
        for label, mutate in mutations.items():
            evidence = single_project_evidence(self.contract)
            process = single_project_process(evidence, "typokat", "collision")
            request = json.loads(process["request_stream"])
            mutate(request["files"])
            refresh_single_project_request(process, request)
            with self.subTest(label=label), self.assertRaises(batch.BatchContractError):
                batch.verify_single_project_linkage(evidence, self.contract)

    def test_single_project_linkage_rejects_row_and_semantic_forgery(self) -> None:
        missing = single_project_evidence(self.contract)
        missing["tools"]["typokat"]["processes"].pop()
        with self.assertRaisesRegex(batch.BatchContractError, "row|cardinality"):
            batch.verify_single_project_linkage(missing, self.contract)

        reordered = single_project_evidence(self.contract)
        reordered["tools"]["tsgo"]["processes"].reverse()
        with self.assertRaisesRegex(batch.BatchContractError, "row|order"):
            batch.verify_single_project_linkage(reordered, self.contract)

        for label, diagnostic in (
            ("path", "renamed/main.ts:4:7:2322"),
            ("column", "fast-errors/main.ts:4:8:2322"),
        ):
            evidence = single_project_evidence(self.contract)
            process = single_project_process(evidence, "typokat", "fast-errors")
            response = json.loads(process["response_stream"])
            response["semantic"]["diagnostics"][0] = diagnostic
            refresh_single_project_response(process, response)
            refresh_single_project_claimed_oracle(evidence, "typokat")
            with self.subTest(label=label), self.assertRaisesRegex(
                batch.BatchContractError, "oracle|semantic|diagnostic"
            ):
                batch.verify_single_project_linkage(evidence, self.contract)

    def test_single_project_linkage_requires_raw_data_and_same_runner(self) -> None:
        no_raw_request = single_project_evidence(self.contract)
        process = single_project_process(no_raw_request, "typokat", "fast-clean")
        del process["request_stream"]
        with self.assertRaises(batch.BatchContractError):
            batch.verify_single_project_linkage(no_raw_request, self.contract)

        no_raw_response = single_project_evidence(self.contract)
        process = single_project_process(no_raw_response, "tsgo", "fast-clean")
        del process["response_stream"]
        with self.assertRaises(batch.BatchContractError):
            batch.verify_single_project_linkage(no_raw_response, self.contract)

        different_binary = single_project_evidence(self.contract)
        different_binary["tools"]["typokat"]["batch_binary_sha256"] = "f" * 64
        with self.assertRaisesRegex(batch.BatchContractError, "binary"):
            batch.verify_single_project_linkage(different_binary, self.contract)

        go_claim = single_project_evidence(self.contract)
        go_claim["verdict"] = "GO"
        with self.assertRaisesRegex(batch.BatchContractError, "GO|verdict"):
            batch.verify_single_project_linkage(go_claim, self.contract)

    def test_single_project_linkage_requires_raw_cold_processes(self) -> None:
        missing = single_project_evidence(self.contract)
        del missing["tools"]["typokat"]["cold_processes"]
        with self.assertRaisesRegex(batch.BatchContractError, "cold|process|shape"):
            batch.verify_single_project_linkage(missing, self.contract)

        forged = single_project_evidence(self.contract)
        process = forged["tools"]["typokat"]["cold_processes"][1]
        process["stderr"] = process["stderr"].replace("(4,7)", "(4,8)", 1)
        process["normalized_semantic"]["diagnostics"][0] = (
            "fast-errors/main.ts:4:8:2322"
        )
        refresh_claimed_cold_oracle(forged, "typokat")
        with self.assertRaisesRegex(
            batch.BatchContractError, "cold|oracle|semantic|diagnostic"
        ):
            batch.verify_single_project_linkage(forged, self.contract)

    def test_single_project_linkage_authenticates_cold_binary_provenance(self) -> None:
        wrong_candidate = single_project_evidence(self.contract)
        wrong_candidate["tools"]["typokat"]["cold_binary_candidate_commit"] = "b" * 40
        with self.assertRaisesRegex(batch.BatchContractError, "candidate|revision|build"):
            batch.verify_single_project_linkage(wrong_candidate, self.contract)

        wrong_tree = single_project_evidence(self.contract)
        wrong_tree["typokat_cold_build"]["candidate_tree"] = "c" * 40
        with self.assertRaisesRegex(batch.BatchContractError, "tree|build"):
            batch.verify_single_project_linkage(wrong_tree, self.contract)

        wrong_typokat_binary = single_project_evidence(self.contract)
        tool = wrong_typokat_binary["tools"]["typokat"]
        tool["authenticated_cold_binary_sha256"] = "7" * 64
        for process in tool["cold_processes"]:
            process["executable_before_sha256"] = "7" * 64
            process["executable_after_sha256"] = "7" * 64
        with self.assertRaisesRegex(batch.BatchContractError, "binary|build"):
            batch.verify_single_project_linkage(wrong_typokat_binary, self.contract)

        wrong_tsgo_binary = single_project_evidence(self.contract)
        wrong_tsgo_binary["tools"]["tsgo"]["authenticated_cold_binary_sha256"] = "8" * 64
        with self.assertRaisesRegex(batch.BatchContractError, "binary"):
            batch.verify_single_project_linkage(wrong_tsgo_binary, self.contract)

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


def single_project_oracle_sha256(contract: dict[str, object]) -> str:
    del contract
    oracle = cold.load_oracles()["rows"]
    rows = [
        {
            "case_id": case_id,
            "semantic": {
                "exit_code": oracle[case_id]["exit"],
                "parse_errors": [],
                "diagnostics": oracle[case_id]["diagnostics"],
                "incompletes": [],
            },
        }
        for case_id in ("fast-clean", "fast-errors", "collision", "fanout")
    ]
    return hashlib.sha256(
        json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def single_project_evidence(contract: dict[str, object]) -> dict[str, object]:
    oracle_digest = single_project_oracle_sha256(contract)
    typokat_cold_binary = str((Path("/tmp") / ("typokat-cold-" + "a" * 40)).resolve())
    typokat_cold_hash = "6" * 64
    evidence = {
        "schema": 1,
        "verdict": "LINKAGE-PASS",
        "candidate_commit": "a" * 40,
        "candidate_tree": "b" * 40,
        "cold_candidate_commit": "a" * 40,
        "cold_candidate_tree": "b" * 40,
        "typokat_cold_build": {
            "candidate_commit": "a" * 40,
            "candidate_tree": "b" * 40,
            "source_identity_sha256": "9" * 64,
            "binary_path": typokat_cold_binary,
            "binary_sha256": typokat_cold_hash,
        },
        "tools": {},
    }
    for tool, binary in (("typokat", "2" * 64), ("tsgo", "3" * 64)):
        cold_binary = (
            typokat_cold_binary
            if tool == "typokat"
            else str((batch.ROOT / contract["tsgo"]["cold_binary_relative_path"]).resolve())
        )
        cold_binary_sha256 = (
            typokat_cold_hash
            if tool == "typokat"
            else contract["tsgo"]["cold_binary_sha256"]
        )
        tool_evidence = {
            "runner_source_sha256": ("4" if tool == "typokat" else "5") * 64,
            "revision": (
                "a" * 40
                if tool == "typokat"
                else contract["tsgo"]["upstream_revision"]
            ),
            "single_binary_sha256": binary,
            "batch_binary_sha256": binary,
            "single_oracle_sha256": oracle_digest,
            "cold_oracle_sha256": oracle_digest,
            "authenticated_cold_binary_sha256": cold_binary_sha256,
            "cold_binary_path": cold_binary,
            "cold_binary_candidate_commit": "a" * 40 if tool == "typokat" else None,
            "processes": [
                make_single_project_process(tool, case_id, binary, ordinal, contract)
                for ordinal, case_id in enumerate(contract["single_project"]["rows"])
            ],
            "cold_processes": [
                make_cold_process(
                    tool, case_id, cold_binary, cold_binary_sha256, contract
                )
                for case_id in contract["single_project"]["rows"]
            ],
        }
        evidence["tools"][tool] = tool_evidence
    return evidence


def make_cold_process(
    tool: str,
    case_id: str,
    binary_path: str,
    binary_sha256: str,
    contract: dict[str, object],
) -> dict[str, object]:
    cold_contract = cold.verify_contract()
    sources = [str(path.resolve()) for path in cold.row_sources(case_id)]
    flags = (
        cold_contract["typokat_flags"]
        if tool == "typokat"
        else contract["typescript_flags"]
    )
    oracle = cold.load_oracles()["rows"][case_id]
    lines = []
    prefix = "TK" if tool == "typokat" else "TS"
    for diagnostic in oracle["diagnostics"]:
        relative, line, column, code = diagnostic.rsplit(":", 3)
        path = (cold.WORKLOADS / relative).resolve()
        lines.append(f"{path}({line},{column}): error {prefix}{code}: synthetic")
    output = "".join(line + "\n" for line in lines)
    semantic = {
        "exit_code": oracle["exit"],
        "parse_errors": [],
        "diagnostics": list(oracle["diagnostics"]),
        "incompletes": [],
    }
    return {
        "tool": tool,
        "case_id": case_id,
        "argv": [binary_path, *flags, *sources],
        "returncode": oracle["exit"],
        "stdout": output if tool == "tsgo" else "",
        "stderr": "" if tool == "tsgo" else output,
        "stdout_complete": True,
        "stderr_complete": True,
        "timed_out": False,
        "process_group_empty": True,
        "executable_before_sha256": binary_sha256,
        "executable_after_sha256": binary_sha256,
        "address_space_limit_bytes": contract["child_address_space_limit_bytes"],
        "timeout_seconds": contract["single_project"]["timeout_seconds"],
        "normalized_semantic": semantic,
    }


def make_single_project_process(
    tool: str,
    case_id: str,
    binary_sha256: str,
    ordinal: int,
    contract: dict[str, object],
) -> dict[str, object]:
    locked = cold.workload_lock()[case_id]
    files = [
        {
            "path": entry.path,
            "source": (cold.WORKLOADS / entry.path).read_text(encoding="utf-8"),
        }
        for entry in locked
    ]
    request = {"schema": 1, "case_id": case_id, "files": files}
    oracle = cold.load_oracles()["rows"][case_id]
    response = {
        "schema": 1,
        "tool": tool,
        "case_id": case_id,
        "file_count": len(files),
        "semantic": {
            "exit_code": oracle["exit"],
            "parse_errors": [],
            "diagnostics": list(oracle["diagnostics"]),
            "incompletes": [],
        },
        "shared_product_identity": contract["profile"]["length_framed_sha256"],
        "program_identity": f"{tool}-program-{ordinal}",
        "checker_identity": f"{tool}-checker-{ordinal}",
        "semantic_identity": f"{tool}-semantic-{ordinal}",
        "retained_frame_state": 0,
    }
    if tool == "typokat":
        response.update(
            session_weak_alive_after=False,
            cache_weak_alive_after=False,
        )
    else:
        response["retained_frame_handles"] = 0
    request_stream = json.dumps(request, sort_keys=True, separators=(",", ":")) + "\n"
    response_stream = json.dumps(response, sort_keys=True, separators=(",", ":")) + "\n"
    return {
        "tool": tool,
        "case_id": case_id,
        "arguments": [contract["single_project"]["mode_argument"]],
        "returncode": 0,
        "stdout": response_stream,
        "stderr": "",
        "stdout_complete": True,
        "stderr_complete": True,
        "timed_out": False,
        "process_group_empty": True,
        "executable_before_sha256": binary_sha256,
        "executable_after_sha256": binary_sha256,
        "address_space_limit_bytes": contract["child_address_space_limit_bytes"],
        "timeout_seconds": contract["single_project"]["timeout_seconds"],
        "request_stream": request_stream,
        "response_stream": response_stream,
        "request_stream_sha256": hashlib.sha256(request_stream.encode()).hexdigest(),
        "response_stream_sha256": hashlib.sha256(response_stream.encode()).hexdigest(),
    }


def single_project_process(
    evidence: dict[str, object], tool: str, case_id: str
) -> dict[str, object]:
    return next(
        process
        for process in evidence["tools"][tool]["processes"]
        if process["case_id"] == case_id
    )


def refresh_single_project_request(
    process: dict[str, object], request: dict[str, object]
) -> None:
    raw = json.dumps(request, sort_keys=True, separators=(",", ":")) + "\n"
    process["request_stream"] = raw
    process["request_stream_sha256"] = hashlib.sha256(raw.encode()).hexdigest()


def refresh_single_project_response(
    process: dict[str, object], response: dict[str, object]
) -> None:
    raw = json.dumps(response, sort_keys=True, separators=(",", ":")) + "\n"
    process["stdout"] = raw
    process["response_stream"] = raw
    process["response_stream_sha256"] = hashlib.sha256(raw.encode()).hexdigest()


def refresh_single_project_claimed_oracle(
    evidence: dict[str, object], tool: str
) -> None:
    rows = [
        {
            "case_id": process["case_id"],
            "semantic": json.loads(process["response_stream"])["semantic"],
        }
        for process in evidence["tools"][tool]["processes"]
    ]
    digest = hashlib.sha256(
        json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    evidence["tools"][tool]["single_oracle_sha256"] = digest


def refresh_claimed_cold_oracle(evidence: dict[str, object], tool: str) -> None:
    rows = [
        {
            "case_id": process["case_id"],
            "semantic": process["normalized_semantic"],
        }
        for process in evidence["tools"][tool]["cold_processes"]
    ]
    digest = hashlib.sha256(
        json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    evidence["tools"][tool]["cold_oracle_sha256"] = digest


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
        "lifecycle_disposition": "semantic",
        "program_identity": f"program-{index}",
        "checker_identity": f"checker-{index}",
        "semantic_identity": f"semantic-{index}",
        "shared_product_identity": "shared-product",
        "retained_frame_state": 0,
        "semantic": {
            "exit_code": 0,
            "parse_errors": [],
            "diagnostics": [],
            "incompletes": [],
        },
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
            if sample["tool"] == "typokat" and frame["semantic"]["parse_errors"]:
                frame.update(
                    lifecycle_disposition="parse-rejected",
                    program_identity=None,
                    checker_identity=None,
                    semantic_identity=None,
                    materialized_product_owner_count=None,
                    session_weak_alive_after=None,
                    cache_weak_alive_after=None,
                )
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

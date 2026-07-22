"""RED adversary contract for the frozen-library-base fresh-process gate."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
VERIFY_PATH = ROOT / "tooling/library-base/verify.py"
SPEC = importlib.util.spec_from_file_location("library_base_verify", VERIFY_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load library-base verifier")
verify = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verify)


PROFILE = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d"
SCHEMA = "a78ea0521c7c375669bfdb08f0929a5e4b1d0b0d6928de60fbfe09b222a8bc65"
ARTIFACT = "af97017b22c9f8ff3726de9dbd49a3039cf70f2dd5a4fd9df9f71328be721dd0"
BINARY = "1" * 64
PROJECTION = "2" * 64
FILTER = "library::snapshot_base_spec::frozen_library_base_release_probe_once"
PREFIX = "TYPOKAT_LIBRARY_BASE_PROBE="


def probe() -> dict[str, object]:
    return {
        "schema": 1,
        "route": "production-frozen-library-base",
        "profile_sha256": PROFILE,
        "schema_sha256": SCHEMA,
        "artifact_sha256": ARTIFACT,
        "artifact_bytes": 10_003_957,
        "canonical_projection_sha256": PROJECTION,
        "initializations": 1,
        "publications": 1,
        "compiler_invocations": 0,
        "generator_invocations": 0,
        "source_bytes_read": 0,
        "validation_us": 6_000,
        "decode_us": 52_000,
        "publication_us": 10,
    }


def render_stdout(payload: dict[str, object]) -> str:
    return (
        "running 1 test\n"
        + PREFIX
        + json.dumps(payload, sort_keys=True, separators=(",", ":"))
        + "\ntest result: ok. 1 passed; 0 failed; 0 ignored"
    )


def process(window: int, ordinal: int, kind: str, pid: int, build: int) -> dict[str, object]:
    payload = probe()
    return {
        "window": window,
        "ordinal": ordinal,
        "kind": kind,
        "pid": pid,
        "build": build,
        "command": [
            f"/tmp/typokat-library-base/build-{build}/target/release/deps/typokat-libtest",
            FILTER,
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ],
        "binary_profile": "release",
        "binary_sha256": BINARY,
        "exit": 0,
        "stdout": render_stdout(payload),
        "stderr": "",
        "external_measurement": "/usr/bin/time -v",
        "wall_us": 100_000 + ordinal,
        "peak_rss_bytes": 64 * 1024 * 1024,
    }


def evidence() -> dict[str, object]:
    pid = 10_000
    windows = []
    for window in range(1, 4):
        records = []
        for ordinal in range(1, 16):
            kind = "warmup" if ordinal <= 5 else "sample"
            records.append(process(window, ordinal, kind, pid, 1 + (window % 2)))
            pid += 1
        windows.append(
            {
                "ordinal": window,
                "started_monotonic_ns": window * 1_000_000_000,
                "finished_monotonic_ns": window * 1_000_000_000 + 500_000_000,
                "records": records,
            }
        )
    return {
        "schema": 1,
        "revision": "a" * 40,
        "host": {"os": "linux", "arch": "x86_64", "cpu_count": 8},
        "builds": [
            {
                "ordinal": 1,
                "root": "/tmp/typokat-library-base/build-1",
                "binary_sha256": BINARY,
                "binary_bytes": 20_000_000,
                "git_status": "",
            },
            {
                "ordinal": 2,
                "root": "/tmp/typokat-library-base/build-2",
                "binary_sha256": BINARY,
                "binary_bytes": 20_000_000,
                "git_status": "",
            },
        ],
        "windows": windows,
    }


class LibraryBaseVerifierContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.contract = verify.load_contract()
        self.evidence = evidence()

    def validate(self) -> dict[str, object]:
        return verify.validate_evidence(self.evidence, self.contract)

    def test_complete_raw_schedule_is_required(self) -> None:
        summary = self.validate()
        self.assertEqual(summary["samples"], 30)
        self.assertEqual(summary["projection_identities"], 1)
        self.assertEqual(summary["outcome"], "GO")

        self.evidence["windows"][1]["records"].pop()
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_two_distinct_clean_byte_identical_release_builds_are_required(self) -> None:
        self.evidence["builds"][1]["root"] = self.evidence["builds"][0]["root"]
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["builds"][1]["binary_sha256"] = "3" * 64
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["builds"][0]["git_status"] = "?? generated"
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_every_record_is_a_unique_fresh_process(self) -> None:
        records = self.evidence["windows"][0]["records"]
        records[1]["pid"] = records[0]["pid"]
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_windows_are_ordered_nonoverlapping_and_complete(self) -> None:
        self.evidence["windows"][1]["started_monotonic_ns"] = 1_100_000_000
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["windows"][2]["ordinal"] = 2
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_exact_release_filter_and_external_measurement_are_required(self) -> None:
        record = self.evidence["windows"][0]["records"][0]
        record["command"][1] = "some_other_test"
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["windows"][0]["records"][0]["binary_profile"] = "debug"
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["windows"][0]["records"][0]["external_measurement"] = "internal"
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_probe_record_is_exactly_once_valid_json_and_successful(self) -> None:
        record = self.evidence["windows"][0]["records"][0]
        record["stdout"] += "\n" + PREFIX + "{}"
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["windows"][0]["records"][0]["stdout"] = PREFIX + "{bad json}"
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["windows"][0]["records"][0]["exit"] = 1
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["windows"][0]["records"][0]["stderr"] = "warning"
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_all_pinned_identities_and_route_are_enforced(self) -> None:
        for key in (
            "route",
            "profile_sha256",
            "schema_sha256",
            "artifact_sha256",
            "artifact_bytes",
        ):
            with self.subTest(key=key):
                changed = copy.deepcopy(self.evidence)
                record = changed["windows"][0]["records"][0]
                parsed = probe()
                parsed[key] = "wrong" if key != "artifact_bytes" else 1
                record["stdout"] = render_stdout(parsed)
                self.evidence = changed
                with self.assertRaises(verify.ContractError):
                    self.validate()
                self.evidence = evidence()

    def test_projection_identity_must_be_stable_across_every_process(self) -> None:
        parsed = probe()
        parsed["canonical_projection_sha256"] = "4" * 64
        self.evidence["windows"][2]["records"][14]["stdout"] = render_stdout(parsed)
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_no_compilation_generation_source_work_or_partial_publication_is_allowed(self) -> None:
        for key in (
            "compiler_invocations",
            "generator_invocations",
            "source_bytes_read",
            "initializations",
            "publications",
        ):
            with self.subTest(key=key):
                changed = copy.deepcopy(self.evidence)
                parsed = probe()
                parsed[key] = 2 if key in ("initializations", "publications") else 1
                changed["windows"][0]["records"][0]["stdout"] = render_stdout(parsed)
                self.evidence = changed
                with self.assertRaises(verify.ContractError):
                    self.validate()
                self.evidence = evidence()

    def test_every_window_and_overall_p95_must_meet_the_external_wall_gate(self) -> None:
        for record in self.evidence["windows"][1]["records"]:
            if record["kind"] == "sample":
                record["wall_us"] = 120_001
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_every_process_must_meet_the_external_rss_gate(self) -> None:
        self.evidence["windows"][2]["records"][7]["peak_rss_bytes"] = 536_870_913
        with self.assertRaises(verify.ContractError):
            self.validate()


if __name__ == "__main__":
    unittest.main()

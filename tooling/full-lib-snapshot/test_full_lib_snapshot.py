from __future__ import annotations

import copy
import importlib.util
import io
import json
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest import mock


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("full_lib_snapshot", HERE / "full_lib_snapshot.py")
assert SPEC is not None and SPEC.loader is not None
snapshot = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(snapshot)


class SnapshotCoordinatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.contract = snapshot.load_contract()
        self.evidence = self.make_evidence()

    def identity(self, path: str, digest: str = "a" * 64, size: int = 1024) -> dict[str, object]:
        return {"path": path, "bytes": size, "sha256": digest}

    def archive_bytes(self) -> bytes:
        magic = self.contract["wire"]["magic"].encode("ascii")
        payloads = [bytes([ordinal]) * (105_000 + ordinal) for ordinal in range(1, 11)]
        body = b"".join(payloads)
        fixed = len(magic) + 4 + 32 + 32 + 4 + 8 + 32
        cursor = fixed + 10 * 52
        directory = bytearray()
        for tag, payload in zip(self.contract["wire"]["section_tags"], payloads, strict=True):
            directory.extend(tag.to_bytes(2, "big"))
            directory.extend((0).to_bytes(2, "big"))
            directory.extend(cursor.to_bytes(8, "big"))
            directory.extend(len(payload).to_bytes(8, "big"))
            directory.extend(hashlib.sha256(payload).digest())
            cursor += len(payload)
        return b"".join([magic, (1).to_bytes(4, "big"), bytes.fromhex(self.contract["profile_sha256"]), bytes.fromhex(self.contract["wire"]["schema_sha256"]), (10).to_bytes(4, "big"), len(body).to_bytes(8, "big"), hashlib.sha256(body).digest(), bytes(directory), body])

    def harness(self, probe: dict[str, object] | None = None) -> str:
        middle = ""
        if probe is not None:
            middle = self.contract["libtest"]["record_prefix"] + json.dumps(probe, sort_keys=True, separators=(",", ":")) + "\n"
        return (
            "\nrunning 1 test\n"
            f"test {self.contract['libtest']['timing_filter']} ... {middle}ok\n\n"
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 857 filtered out; finished in 0.01s\n"
        )

    def process(self, pid: int, argv: list[str], env: dict[str, str], stdout: str, *, start: int, wall: int = 100_000_000, rss: int = 100_000) -> dict[str, object]:
        return {
            "argv": argv,
            "cwd": str(snapshot.ROOT.resolve()),
            "env": env,
            "pid": pid,
            "returncode": 0,
            "stdout": stdout,
            "stderr": "",
            "started_monotonic_ns": start,
            "ended_monotonic_ns": start + wall,
            "wall_ns": wall,
            "peak_rss_kib": rss,
            "group_clean": True,
        }

    def probe(self, artifact: dict[str, object]) -> dict[str, object]:
        return {
            "schema": 1,
            "kind": snapshot.RECORD_KIND,
            "route": "decoded-base-user-check",
            "profile_sha256": self.contract["profile_sha256"],
            "strategy": snapshot.STRATEGY,
            "artifact_sha256": artifact["sha256"],
            "artifact_bytes": artifact["bytes"],
            "validated_bytes": artifact["bytes"],
            "runtime_projection_sha256": "9" * 64,
            "input_path": artifact["path"],
            "semantics": snapshot.expected_semantics(),
            "compiler_measure": {"source_loads": 0, "parse_units": 0, "bind_units": 0, "semantic_units": 0, "snapshot_generations": 0},
            "internal": {"validation_us": 10, "decode_us": 20, "user_check_us": 30, "wall_us": 60, "peak_rss_bytes": 1},
        }

    def make_evidence(self) -> dict[str, object]:
        run_root = Path("/tmp/typokat-wu0b-test")
        canonical_rustflags = [
            f"--remap-path-prefix={run_root}/build-1=/typokat-wu0b/build",
            f"--remap-path-prefix={run_root}/build-2=/typokat-wu0b/build",
            "--remap-path-scope=all",
        ]
        encoded_rustflags = "\x1f".join(canonical_rustflags)
        binaries = [self.identity(f"/tmp/typokat-wu0b-test/build-{ordinal}/libtest", "b" * 64, 4096) for ordinal in (1, 2)]
        artifact_file = self.identity("/tmp/typokat-wu0b-test/regeneration-1/library.snapshot", "c" * 64, 1024 * 1024)
        header_bytes = len(self.contract["wire"]["magic"].encode("ascii")) + 4 + 32 + 32 + 4 + 8 + 32 + 520
        body_bytes = artifact_file["bytes"] - header_bytes
        base = body_bytes // 10
        sections = []
        offset = header_bytes
        for ordinal, (tag, name) in enumerate(zip(self.contract["wire"]["section_tags"], self.contract["wire"]["section_names"], strict=True)):
            size = base if ordinal < 9 else artifact_file["bytes"] - offset
            sections.append({"ordinal": ordinal, "tag": tag, "name": name, "offset": offset, "bytes": size, "sha256": format(ordinal + 1, "064x")})
            offset += size
        wire = {"magic": self.contract["wire"]["magic"], "version": 1, "profile_sha256": self.contract["profile_sha256"], "schema_sha256": self.contract["wire"]["schema_sha256"], "section_count": 10, "directory_bytes": 520, "body_bytes": body_bytes, "body_sha256": "8" * 64, "sections": sections}
        artifact = {**artifact_file, "wire": wire}
        source = {"root": str(snapshot.ROOT.resolve()), "git_commit": "d" * 40, "git_tree": "f" * 40, "git_status": "", "tracked_files": 100, "tracked_bytes": 1000, "tracked_sha256": "e" * 64}
        cargo_command = [str(snapshot.executable("cargo")), *self.contract["libtest"]["build_args"]]
        builds = []
        for ordinal, binary in enumerate(binaries, 1):
            source_copy = {**source, "root": f"/tmp/typokat-wu0b-test/build-{ordinal}/source", "git_status": ""}
            build_env = snapshot.sanitized_environment() | {"CARGO_HOME": f"/tmp/typokat-wu0b-test/build-{ordinal}/cargo-home", "CARGO_TARGET_DIR": f"/tmp/typokat-wu0b-test/build-{ordinal}/target", "CARGO_NET_OFFLINE": "true", "CARGO_TERM_COLOR": "never", "CARGO_ENCODED_RUSTFLAGS": encoded_rustflags, "CARGO_BUILD_RUSTFLAGS": "", "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER": "/usr/bin/cc"}
            cargo_version = self.process(100 + ordinal, [str(snapshot.executable("cargo")), "--version", "--verbose"], build_env, "cargo 1\n", start=ordinal * 100_000_000)
            cargo_version["cwd"] = source_copy["root"]
            rustc_version = self.process(110 + ordinal, [str(snapshot.executable("rustc")), "--version", "--verbose"], build_env, "rustc 1\n", start=ordinal * 200_000_000)
            rustc_version["cwd"] = source_copy["root"]
            build_process = self.process(120 + ordinal, cargo_command, build_env, "{}\n", start=ordinal * 300_000_000)
            build_process["cwd"] = source_copy["root"]
            build_process["stderr"] = "Finished release profile"
            empty_tree = {"path": f"/tmp/typokat-wu0b-test/build-{ordinal}/cargo-home/empty", "files": 0, "bytes": 0, "sha256": snapshot.sha256_bytes(b"")}
            source_tree = {"path": f"/tmp/typokat-wu0b-test/build-{ordinal}/cargo-home/registry/src", "files": 10, "bytes": 100, "sha256": "7" * 64}
            builds.append({"command": cargo_command, "environment": build_env, "cargo_version": cargo_version, "rustc_version": rustc_version, "process": build_process, "cargo_lock": snapshot.file_identity(snapshot.ROOT / "Cargo.lock"), "cargo_configs": snapshot.repo_cargo_configs(), "toolchain_files": snapshot.toolchain_identities(), "cargo_home_before": {"cache_source": "/tmp/cache", "exposed": [], "root": build_env["CARGO_HOME"], "registry_sources": empty_tree, "git_checkouts": empty_tree, "git_db": empty_tree}, "cargo_home_after": {"root": build_env["CARGO_HOME"], "registry_sources": source_tree, "git_checkouts": empty_tree, "git_db": empty_tree}, "effective_fingerprint": {"file": self.identity(f"/tmp/fingerprint-{ordinal}", "1" * 64), "invoked_timestamp": self.identity(f"/tmp/invoked-{ordinal}", "2" * 64), "rustflags": canonical_rustflags, "features": "[]", "profile": 1, "config": 1}, "source_before": source_copy, "source_after": source_copy, "libtest": binary})
        preflight_stdout = f"test result: ok. {self.contract['libtest']['preflight_passed']} passed; 0 failed; {self.contract['libtest']['preflight_ignored']} ignored; 0 measured; 0 filtered out"
        preflights = []
        for ordinal, binary in enumerate(binaries, 1):
            source_copy = builds[ordinal - 1]["source_after"]
            environment = snapshot.sanitized_environment() | {"TYPOKAT_WU0B_PROFILE_ROOT": source_copy["root"]}
            process = self.process(130 + ordinal, snapshot.preflight_command(Path(binary["path"]), self.contract), environment, preflight_stdout, start=3_000_000_000 + ordinal * 200_000_000)
            process["cwd"] = source_copy["root"]
            preflights.append({"filter": self.contract["libtest"]["preflight_filter"], "binary_before": binary, "binary_after": binary, "source_before": source_copy, "source_after": source_copy, "process": process})
        generations = []
        generation_stdout = "\nrunning 1 test\ntest regeneration ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 857 filtered out; finished in 0.01s\n"
        for ordinal in (1, 2):
            output = copy.deepcopy(artifact)
            output["path"] = f"/tmp/typokat-wu0b-test/regeneration-{ordinal}/library.snapshot"
            source_copy = builds[ordinal - 1]["source_after"]
            env = snapshot.sanitized_environment() | {"TYPOKAT_WU0B_SNAPSHOT_OUTPUT": output["path"], "TYPOKAT_WU0B_PROFILE_ROOT": source_copy["root"]}
            process = self.process(ordinal + 1, snapshot.test_command(Path(binaries[ordinal - 1]["path"]), self.contract["libtest"]["regeneration_filter"], self.contract), env, generation_stdout, start=5_000_000_000 + ordinal * 1_000_000_000)
            process["cwd"] = source_copy["root"]
            generations.append({
                "role": "generation",
                "filter": self.contract["libtest"]["regeneration_filter"],
                "binary_before": binaries[ordinal - 1],
                "binary_after": binaries[ordinal - 1],
                "artifact_before": None,
                "artifact_after": None,
                "source_before": source_copy,
                "source_after": source_copy,
                "process": process,
                "output": {key: output[key] for key in ("path", "bytes", "sha256")},
                "wire": wire,
            })
        windows = []
        pid = 10
        cursor = 10_000_000_000
        for window_index, label in enumerate(("morning", "afternoon", "evening"), 1):
            if windows:
                cursor = windows[-1]["ended_monotonic_ns"] + 60_000_000_000
            window_start = cursor
            warmups = []
            recorded = []
            for recorded_flag, count, destination in ((False, 5, warmups), (True, 10, recorded)):
                for ordinal in range(1, count + 1):
                    probe = self.probe(artifact_file)
                    env = snapshot.sanitized_environment() | {"TYPOKAT_WU0B_SNAPSHOT_INPUT": artifact_file["path"]}
                    process = self.process(pid, snapshot.test_command(Path(binaries[0]["path"]), self.contract["libtest"]["timing_filter"], self.contract), env, self.harness(probe), start=cursor)
                    destination.append({
                        "role": "timing",
                        "filter": self.contract["libtest"]["timing_filter"],
                        "binary_before": binaries[0],
                        "binary_after": binaries[0],
                        "artifact_before": artifact_file,
                        "artifact_after": artifact_file,
                        "process": process,
                        "window": window_index,
                        "ordinal": ordinal,
                        "recorded": recorded_flag,
                        "probe": probe,
                    })
                    pid += 1
                    cursor += 200_000_000
            windows.append({"index": window_index, "label": label, "started_monotonic_ns": window_start, "ended_monotonic_ns": cursor, "warmups": warmups, "recorded": recorded})
        evidence = {
            "schema": 1,
            "verdict": "GO",
            "contract_sha256": snapshot.sha256_file(snapshot.CONTRACT_PATH),
            "started_utc": "2026-07-21T00:00:00+00:00",
            "host": {"hostname": "host", "kernel": "kernel", "machine": "x86_64", "python": "3", "cpu_model": "test-cpu", "cpu_count": 8, "affinity": [0, 1], "priority": 0, "timezone": "UTC", "rlimit_as": [-1, -1], "rlimit_cpu": [-1, -1], "rlimit_nofile": [1024, 1024]},
            "source": source,
            "builds": builds,
            "preflights": preflights,
            "profile": {"sha256": self.contract["profile_sha256"], "file_count": 82, "wu0a_oracles_sha256": self.contract["wu0a_oracles_sha256"], "wu0a_workloads_sha256": self.contract["wu0a_workloads_sha256"]},
            "artifact": artifact,
            "generations": generations,
            "windows": windows,
            "derived": {},
            "final_source": source,
        }
        evidence["derived"] = snapshot.derive(windows, self.contract)
        return evidence

    def rewrite_probe_stdout(self, sample: dict[str, object]) -> None:
        sample["process"]["stdout"] = self.harness(sample["probe"])

    def assert_invalid(self, evidence: dict[str, object]) -> None:
        with self.assertRaises(snapshot.ContractError):
            snapshot.validate_evidence(evidence, self.contract)

    def test_contract_and_complete_evidence_pass(self) -> None:
        snapshot.validate_evidence(self.evidence, self.contract)

    def test_cargo_and_rustc_proxies_keep_distinct_real_identities(self) -> None:
        cargo = snapshot.executable("cargo")
        rustc = snapshot.executable("rustc")
        self.assertEqual(cargo.name, "cargo")
        self.assertEqual(rustc.name, "rustc")
        self.assertNotEqual(cargo, rustc)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "cargo-home").mkdir()
            (root / "target").mkdir()
            environment = snapshot.build_environment(root / "cargo-home", root / "target")
            cargo_result = snapshot.run_process([str(cargo), "--version", "--verbose"], cwd=snapshot.ROOT, environment=environment, timeout=5, stdout_cap=65536, stderr_cap=65536)
            rustc_result = snapshot.run_process([str(rustc), "--version", "--verbose"], cwd=snapshot.ROOT, environment=environment, timeout=5, stdout_cap=65536, stderr_cap=65536)
        self.assertTrue(cargo_result["stdout"].startswith("cargo "))
        self.assertTrue(rustc_result["stdout"].startswith("rustc "))
        self.assertEqual(cargo_result["stderr"], "")
        self.assertEqual(rustc_result["stderr"], "")

    def test_external_wire_parser_accepts_complete_archive(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "snapshot.bin"
            path.write_bytes(self.archive_bytes())
            record = snapshot.parse_snapshot_wire(path, self.contract)
            self.assertEqual(record["section_count"], 10)
            self.assertGreater(record["body_bytes"], 1024 * 1024)

    def test_external_wire_parser_rejects_malformed_archives(self) -> None:
        original = self.archive_bytes()
        magic_len = len(self.contract["wire"]["magic"].encode("ascii"))
        cases = {"empty": b"", "truncated": original[:-1]}
        wrong_magic = bytearray(original); wrong_magic[0] ^= 1; cases["wrong-magic"] = bytes(wrong_magic)
        wrong_profile = bytearray(original); wrong_profile[magic_len + 4] ^= 1; cases["wrong-profile"] = bytes(wrong_profile)
        wrong_body = bytearray(original); wrong_body[-1] ^= 1; cases["wrong-body-digest"] = bytes(wrong_body)
        # Directory metadata is outside the body digest: valid hashes must not excuse bad reserved bits.
        reserved = bytearray(original)
        fixed = magic_len + 4 + 32 + 32 + 4 + 8 + 32
        reserved[fixed + 2:fixed + 4] = (1).to_bytes(2, "big")
        cases["digest-valid-reserved-bit"] = bytes(reserved)
        gap = bytearray(original)
        offset = int.from_bytes(gap[fixed + 4:fixed + 12], "big")
        gap[fixed + 4:fixed + 12] = (offset + 1).to_bytes(8, "big")
        cases["digest-valid-gap"] = bytes(gap)
        with tempfile.TemporaryDirectory() as temporary:
            for label, value in cases.items():
                with self.subTest(label=label):
                    path = Path(temporary) / f"{label}.bin"
                    path.write_bytes(value)
                    with self.assertRaises(snapshot.ContractError):
                        snapshot.parse_snapshot_wire(path, self.contract)

    def test_generation_inside_timing_is_rejected(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["probe"]["compiler_measure"]["snapshot_generations"] = 1
        self.rewrite_probe_stdout(sample)
        self.assert_invalid(self.evidence)

    def test_parse_or_bind_inside_timing_is_rejected(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["probe"]["compiler_measure"]["parse_units"] = 1
        self.rewrite_probe_stdout(sample)
        self.assert_invalid(self.evidence)

    def test_wrong_decoded_base_route_is_rejected(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["probe"]["route"] = "compile-and-check"
        self.rewrite_probe_stdout(sample)
        self.assert_invalid(self.evidence)

    def test_missing_runtime_projection_identity_is_rejected(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["probe"]["runtime_projection_sha256"] = ""
        self.rewrite_probe_stdout(sample)
        self.assert_invalid(self.evidence)

    def test_output_variable_inside_timing_is_rejected(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["process"]["env"]["TYPOKAT_WU0B_SNAPSHOT_OUTPUT"] = "/tmp/cheat"
        self.assert_invalid(self.evidence)

    def test_artifact_mutation_is_rejected(self) -> None:
        self.evidence["windows"][0]["recorded"][0]["artifact_after"]["sha256"] = "f" * 64
        self.assert_invalid(self.evidence)

    def test_same_process_regeneration_is_rejected(self) -> None:
        self.evidence["generations"][1]["process"]["pid"] = self.evidence["generations"][0]["process"]["pid"]
        self.assert_invalid(self.evidence)

    def test_reused_regeneration_output_is_rejected(self) -> None:
        self.evidence["generations"][1]["output"]["path"] = self.evidence["generations"][0]["output"]["path"]
        self.evidence["generations"][1]["process"]["env"]["TYPOKAT_WU0B_SNAPSHOT_OUTPUT"] = self.evidence["generations"][0]["output"]["path"]
        self.assert_invalid(self.evidence)

    def test_incomplete_schedule_is_rejected(self) -> None:
        self.evidence["windows"][1]["recorded"].pop()
        self.assert_invalid(self.evidence)

    def test_full_release_preflight_is_mandatory(self) -> None:
        self.evidence["preflights"].pop()
        self.assert_invalid(self.evidence)

    def test_preflight_must_precede_regeneration(self) -> None:
        self.evidence["preflights"][0]["process"]["ended_monotonic_ns"] = self.evidence["generations"][0]["process"]["started_monotonic_ns"] + 1
        self.evidence["preflights"][0]["process"]["wall_ns"] = self.evidence["preflights"][0]["process"]["ended_monotonic_ns"] - self.evidence["preflights"][0]["process"]["started_monotonic_ns"]
        self.assert_invalid(self.evidence)

    def test_two_independent_build_paths_are_required(self) -> None:
        self.evidence["builds"][1]["libtest"] = self.evidence["builds"][0]["libtest"]
        self.assert_invalid(self.evidence)

    def test_dependency_source_integrity_is_required(self) -> None:
        self.evidence["builds"][0]["cargo_home_after"]["registry_sources"]["files"] = 0
        self.assert_invalid(self.evidence)

    def test_noncanonical_build_rustflags_are_rejected(self) -> None:
        self.evidence["builds"][0]["effective_fingerprint"]["rustflags"].extend(["--cfg", "cheat"])
        self.assert_invalid(self.evidence)

    def test_preflight_profile_source_is_pinned(self) -> None:
        self.evidence["preflights"][0]["process"]["cwd"] = str(snapshot.ROOT.resolve())
        self.assert_invalid(self.evidence)

    def test_generation_profile_source_mutation_is_rejected(self) -> None:
        self.evidence["generations"][1]["source_after"]["tracked_sha256"] = "0" * 64
        self.assert_invalid(self.evidence)

    def test_warmup_peak_rss_is_gated(self) -> None:
        self.evidence["windows"][0]["warmups"][0]["process"]["peak_rss_kib"] = 999_999
        self.assert_invalid(self.evidence)

    def test_reversed_or_outside_window_intervals_are_rejected(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]["process"]
        sample["started_monotonic_ns"] = self.evidence["windows"][0]["ended_monotonic_ns"] + 1
        sample["ended_monotonic_ns"] = sample["started_monotonic_ns"] + sample["wall_ns"]
        self.assert_invalid(self.evidence)

    def test_zero_gap_windows_are_rejected(self) -> None:
        self.evidence["windows"][1]["started_monotonic_ns"] = self.evidence["windows"][0]["ended_monotonic_ns"]
        self.assert_invalid(self.evidence)

    def test_child_self_reported_p95_cannot_override_external_wall(self) -> None:
        for sample in self.evidence["windows"][0]["recorded"]:
            sample["process"]["wall_ns"] = 200_000_000
            sample["process"]["ended_monotonic_ns"] = sample["process"]["started_monotonic_ns"] + 200_000_000
            sample["probe"]["internal"]["wall_us"] = 1
            self.rewrite_probe_stdout(sample)
        self.assert_invalid(self.evidence)

    def test_child_self_reported_rss_cannot_override_wait4_rss(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["process"]["peak_rss_kib"] = 600_000
        sample["probe"]["internal"]["peak_rss_bytes"] = 1
        self.rewrite_probe_stdout(sample)
        self.assert_invalid(self.evidence)

    def test_child_self_reported_parity_is_checked_against_wu0a(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["probe"]["semantics"]["fast-errors"]["diagnostics"] = []
        self.rewrite_probe_stdout(sample)
        self.assert_invalid(self.evidence)

    def test_malformed_probe_output_is_rejected(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["process"]["stdout"] = "running 1 test\nTYPOKAT_WU0B_PROBE={bad json}\n" + "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out"
        self.assert_invalid(self.evidence)

    def test_duplicate_probe_record_is_rejected(self) -> None:
        sample = self.evidence["windows"][0]["recorded"][0]
        sample["process"]["stdout"] += sample["process"]["stdout"]
        self.assert_invalid(self.evidence)

    def test_stale_or_wrong_libtest_identity_is_rejected(self) -> None:
        sample = self.evidence["windows"][2]["recorded"][4]
        sample["binary_before"] = copy.deepcopy(sample["binary_before"])
        sample["binary_before"]["sha256"] = "0" * 64
        self.assert_invalid(self.evidence)

    def test_timing_pid_reuse_is_rejected(self) -> None:
        self.evidence["windows"][2]["recorded"][4]["process"]["pid"] = self.evidence["windows"][0]["recorded"][0]["process"]["pid"]
        self.assert_invalid(self.evidence)

    def test_malicious_derived_p95_and_rss_are_recomputed(self) -> None:
        self.evidence["derived"]["p95_wall_ms"] = 1.0
        self.evidence["derived"]["maximum_peak_rss_kib"] = 1
        self.assert_invalid(self.evidence)

    def test_artifact_size_limit_is_external(self) -> None:
        oversized = 32 * 1024 * 1024 + 1
        self.evidence["artifact"]["bytes"] = oversized
        self.assert_invalid(self.evidence)

    def test_structural_inspection_never_prints_go(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "evidence.json"
            path.write_text(json.dumps(self.evidence), encoding="utf-8")
            output = io.StringIO()
            with mock.patch.object(snapshot, "verify_contract", return_value=self.contract), mock.patch("sys.stdout", output):
                snapshot.inspect(path)
            self.assertEqual(output.getvalue().strip(), "INSPECTED-NON-AUTHORITATIVE")

    def test_existing_output_is_never_overwritten(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "evidence.json"
            path.write_text("keep", encoding="utf-8")
            with self.assertRaises(snapshot.ContractError):
                snapshot.validate_output_path(path)
            self.assertEqual(path.read_text(encoding="utf-8"), "keep")

    def test_output_is_restricted_to_ignored_evidence_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(snapshot.ContractError):
                snapshot.validate_output_path(Path(temporary) / "escape.json")

    def test_gate_failure_writes_canonical_no_go(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            evidence_root = Path(temporary) / "evidence"
            output = evidence_root / "failed.json"
            with mock.patch.object(snapshot, "EVIDENCE_ROOT", evidence_root), mock.patch.object(snapshot, "verify_contract", return_value=self.contract):
                with self.assertRaises(snapshot.ContractError):
                    snapshot.collect_run(output, ["duplicate", "duplicate", "third"])
            retained = json.loads(output.read_text(encoding="utf-8"))
            snapshot.validate_no_go(retained)
            self.assertEqual(retained["verdict"], "NO-GO")

    def test_go_write_rechecks_cleanliness_after_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            evidence_root = Path(temporary) / "evidence"
            evidence_root.mkdir()
            output = evidence_root / "candidate.json"
            clean = copy.deepcopy(self.evidence["source"])
            dirty = {**clean, "git_status": "?? injected.rs\n"}
            with mock.patch.object(snapshot, "EVIDENCE_ROOT", evidence_root), mock.patch.object(snapshot, "source_snapshot", side_effect=[dirty]):
                with self.assertRaises(snapshot.ContractError):
                    snapshot.write_evidence(output, {"verdict": "GO"}, require_clean=True)
            self.assertFalse(output.exists())

    def test_generation_and_input_environment_cannot_be_combined(self) -> None:
        environment = snapshot.sanitized_environment() | {
            "TYPOKAT_WU0B_SNAPSHOT_INPUT": "/tmp/input",
            "TYPOKAT_WU0B_SNAPSHOT_OUTPUT": "/tmp/output",
        }
        with self.assertRaises(snapshot.ContractError):
            snapshot.run_process(["/bin/true"], cwd=snapshot.ROOT, environment=environment, timeout=1, stdout_cap=10, stderr_cap=10)

    def test_nearest_rank_p95_uses_raw_samples(self) -> None:
        values = [index * 1_000_000 for index in range(1, 21)]
        self.assertEqual(snapshot.nearest_rank_p95_ms(values), 19.0)


if __name__ == "__main__":
    unittest.main()

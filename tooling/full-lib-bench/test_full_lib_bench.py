#!/usr/bin/env python3
"""Self-tests for the fail-closed full-library benchmark contract."""

from __future__ import annotations

import json
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stdout
from datetime import datetime, timedelta, timezone
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import full_lib_bench as bench


class ContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = bench.verify_contract()

    def test_committed_contract_and_exact_inventories_pass(self) -> None:
        self.assertEqual(len(bench.profile_lock()), 82)
        self.assertEqual(
            [len(files) for files in bench.workload_lock().values()], [1, 1, 2, 32]
        )

    def test_contract_nested_schema_and_flags_are_exact(self) -> None:
        source = bench.CONTRACT_PATH.read_text(encoding="utf-8")
        with tempfile.TemporaryDirectory() as temporary:
            extra = Path(temporary) / "extra.toml"
            extra.write_text(source + "unexpected = 1\n", encoding="utf-8")
            with self.assertRaisesRegex(bench.ContractError, "contract.controls keys differ"):
                bench.load_contract(extra)
            flags = Path(temporary) / "flags.toml"
            flags.write_text(source.replace('"--strict"', '"--STRICT"', 1), encoding="utf-8")
            with self.assertRaisesRegex(bench.ContractError, "canonical compiler flag arrays"):
                bench.load_contract(flags)

    def test_library_lock_header_is_not_an_83rd_entry(self) -> None:
        lines = bench.LIBRARIES_LOCK.read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(lines), 83)
        self.assertTrue(lines[0].startswith("#"))
        self.assertEqual(len(bench.profile_lock()), 82)

    def test_wrong_library_order_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            lock = Path(temporary) / "libraries.txt"
            lines = bench.LIBRARIES_LOCK.read_text(encoding="utf-8").splitlines()
            lines[1], lines[2] = lines[2], lines[1]
            lock.write_text("\n".join(lines) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(bench.ContractError, "ordinals are not contiguous"):
                bench.parse_lock(lock, with_row=False)

    def test_wrong_library_bytes_are_rejected(self) -> None:
        locked = bench.profile_lock()[0]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / locked.path
            path.write_bytes(b"wrong")
            with self.assertRaisesRegex(bench.ContractError, "drift"):
                bench.verify_locked_file(path, locked, "mutated library")

    def test_workload_byte_mutation_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            copied = Path(temporary) / "workloads"
            shutil.copytree(bench.WORKLOADS, copied)
            target = copied / "fast-clean/main.ts"
            target.write_bytes(target.read_bytes() + b"// changed\n")
            with mock.patch.object(bench, "WORKLOADS", copied):
                with self.assertRaisesRegex(bench.ContractError, "drift"):
                    bench.verify_workloads()

    def test_forbidden_flags_are_rejected(self) -> None:
        command = ["tsgo", *self.contract["typescript_flags"], "--noLib", "input.ts"]
        with self.assertRaisesRegex(bench.ContractError, "forbidden benchmark flags"):
            bench.reject_forbidden_command(command, self.contract, allow_incremental_false=False)

    def test_stale_binary_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "tsc"
            path.write_bytes(b"stale")
            with self.assertRaisesRegex(bench.ContractError, "identity differs"):
                bench.verify_exact_file(path, 5, "0" * 64, "test binary")

    def test_npm_install_integrity_is_required(self) -> None:
        comparator = self.contract["comparator"]
        with tempfile.TemporaryDirectory() as temporary:
            node_modules = Path(temporary) / "node_modules"
            package = node_modules / "@typescript/typescript-linux-x64"
            package.mkdir(parents=True)
            (node_modules / ".package-lock.json").write_text(
                '{"packages":{"node_modules/@typescript/typescript-linux-x64":'
                '{"version":"7.0.2","integrity":"wrong"}}}',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(bench.ContractError, "does not attest"):
                bench.verify_npm_install_integrities(package, comparator)

    def test_extra_or_reordered_list_files_is_rejected(self) -> None:
        expected = [Path("/lib/a.d.ts"), Path("/src/main.ts")]
        for listed in (
            [Path("/src/main.ts"), Path("/lib/a.d.ts")],
            [*expected, Path("/lib/extra.d.ts")],
            expected[:1],
        ):
            with self.assertRaisesRegex(bench.ContractError, "order/inventory differs"):
                bench.verify_listed_inventory(listed, expected, "fast-clean")

    def test_malformed_output_is_rejected(self) -> None:
        result = bench.ProcessResult(("tsgo",), 1, "not a diagnostic\n", "", 0.1, 10)
        with self.assertRaisesRegex(bench.ContractError, "malformed/unrecognized"):
            bench.normalize_diagnostics(result, "tsgo", "fast-errors")

    def test_unbounded_output_is_rejected(self) -> None:
        started = time.monotonic()
        with self.assertRaisesRegex(bench.ContractError, "output exceeded"):
            bench.run_process(
                [sys.executable, "-c", "import os;\nwhile True: os.write(1, b'x' * 4096)"],
                timeout=2,
                max_output=128,
            )
        self.assertLess(time.monotonic() - started, 1.0)

    def test_normal_leader_cannot_leave_orphan_descendant(self) -> None:
        script = (
            "import subprocess,sys; "
            "subprocess.Popen([sys.executable,'-c','import time; time.sleep(10)']); "
            "sys.exit(0)"
        )
        started = time.monotonic()
        result = bench.run_process([sys.executable, "-c", script], timeout=2, max_output=128)
        self.assertEqual(result.returncode, 0)
        self.assertTrue(result.group_clean)
        self.assertLess(time.monotonic() - started, 1.0)

    def test_timeout_kills_the_process_group(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "timed out"):
            bench.run_process(
                [sys.executable, "-c", "import time; time.sleep(10)"],
                timeout=0.05,
                max_output=128,
            )

    def test_partial_schedule_cannot_be_go(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "three complete trial windows"):
            bench.verify_windows_v2(
                [], {}, set(), set(), Path("/missing-typokat"), Path("/missing-tsgo"),
                self.contract,
            )

    def test_reused_pid_is_rejected_before_statistics(self) -> None:
        descriptor = bench.invocation_descriptor([sys.executable, "-c", "pass"])
        digest = bench.sha256_bytes(bench.canonical_json(descriptor))
        registry = {digest: descriptor}
        record = raw_record(digest, 123, descriptor)
        seen: set[int] = set()
        bench.verify_process_record(record, digest, registry, set(), seen, "first")
        with self.assertRaisesRegex(bench.ContractError, "reused process id"):
            bench.verify_process_record(record, digest, registry, set(), seen, "second")

    def test_missing_semantic_row_cannot_be_go(self) -> None:
        semantics = {row: {} for row in bench.ROWS if row != "collision"}
        with self.assertRaisesRegex(bench.ContractError, "semantic rows"):
            bench.verify_semantic_records(
                semantics, {}, {}, {}, set(), set(), Path("/missing"), Path("/missing"),
                self.contract, False,
            )

    def test_failed_rename_control_cannot_be_go(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "control rows"):
            bench.verify_control_assets({"fast-clean": {}})

    def test_incomplete_memory_matrix_is_rejected(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "memory row matrix"):
            bench.verify_memory_v2(
                {}, {}, set(), set(), Path("/missing"), Path("/missing"), self.contract
            )

    def test_reordered_memory_schedule_is_rejected(self) -> None:
        records = [
            {"block": block, "slot": slot, "tool": tool}
            for block in range(5)
            for slot, tool in enumerate(("typokat", "tsgo", "tsgo", "typokat"))
        ]
        records[0], records[1] = records[1], records[0]
        with self.assertRaisesRegex(bench.ContractError, "reordered ABBA"):
            bench.verify_abba_schedule(records, 5, "memory")

    def test_missing_preread_is_rejected(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "missing pre-read"):
            bench.verify_prereads(
                [], 15, Path("/missing-tsgo"), Path("/missing-typokat"), "fast-clean",
                1, 2, "timing",
            )

    def test_offline_inspection_never_certifies_go(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "fake.json"
            path.write_text("{}", encoding="utf-8")
            output = io.StringIO()
            with mock.patch.object(bench, "inspect_evidence"), redirect_stdout(output):
                status = bench.main([
                    "inspect-evidence", str(path),
                    "--typokat", "/forged/typokat", "--tsgo", "/forged/tsgo",
                ])
            self.assertEqual(status, 0)
            self.assertIn("inspection: PASS", output.getvalue())
            self.assertNotIn("GO", output.getvalue())

    def test_preread_uses_staged_libraries_not_vendored_profile(self) -> None:
        paths = bench.preread_paths(Path("/stage/lib/tsc"), Path("/bin/typokat"), "fast-clean")
        staged = {Path("/stage/lib") / item.path for item in bench.profile_lock()}
        self.assertTrue(staged.issubset(set(paths)))
        self.assertFalse(any(str(path).startswith(str(bench.PROFILE)) for path in paths))

    def test_wrong_provider_probe_is_rejected(self) -> None:
        observed = {
            "schema": 1,
            "profile_sha256": self.contract["profile"]["length_framed_sha256"],
            "file_count": 82,
            "provider_route": "test-only-provider",
        }
        with self.assertRaisesRegex(bench.ContractError, "provider route differs"):
            bench.validate_provider_observation(observed, self.contract)

    def test_timing_windows_require_real_sixty_second_gap(self) -> None:
        previous = datetime(2026, 1, 1, tzinfo=timezone.utc)
        with self.assertRaisesRegex(bench.ContractError, "at least 60 seconds"):
            bench.require_window_gap(
                previous + timedelta(seconds=60), previous,
                59_999_999_999, 1,
            )
        bench.require_window_gap(
            previous + timedelta(seconds=60), previous,
            60_000_000_001, 1,
        )

    def test_run_defaults_to_sixty_second_pause(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "evidence.json"
            with mock.patch.object(
                bench, "collect_evidence", return_value={"verdict": "NO-GO"}
            ) as collect:
                status = bench.main([
                    "run", "--typokat", "/bin/typokat", "--tsgo", "/bin/tsgo",
                    "--output", str(output), "--window-label", "one",
                    "--window-label", "two", "--window-label", "three",
                ])
            self.assertEqual(status, 1)
            self.assertEqual(collect.call_args.args[4], 60.0)

    def test_overlapping_process_intervals_are_rejected(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "overlap"):
            bench.verify_chronological_intervals([
                ("first", {"started_monotonic_ns": 10, "ended_monotonic_ns": 20}),
                ("second", {"started_monotonic_ns": 19, "ended_monotonic_ns": 30}),
            ])

    def test_single_cpu_affinity_is_rejected(self) -> None:
        with mock.patch.object(os, "sched_getaffinity", return_value={0}):
            with self.assertRaisesRegex(bench.ContractError, "at least two CPUs"):
                bench.current_affinity()

    def test_priority_and_rlimit_drift_are_rejected(self) -> None:
        with mock.patch.object(os, "getpriority", return_value=1):
            with self.assertRaisesRegex(bench.ContractError, "nice 0"):
                bench.execution_conditions()
        descriptor = bench.invocation_descriptor([sys.executable, "-c", "pass"])
        altered = json.loads(json.dumps(descriptor))
        altered["rlimits"]["RLIMIT_NOFILE"][0] = 1
        digest = bench.sha256_bytes(bench.canonical_json(altered))
        with self.assertRaisesRegex(bench.ContractError, "rlimits is not canonical"):
            bench.verify_invocation_registry({digest: altered}, self.contract)

    def test_transient_executable_mutation_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            executable = Path(temporary) / "mutator"
            executable.write_text(
                "#!/usr/bin/python3\n"
                "from pathlib import Path\n"
                "p = Path(__file__)\n"
                "p.write_bytes(p.read_bytes() + b'\\n# changed')\n",
                encoding="utf-8",
            )
            executable.chmod(0o755)
            with self.assertRaisesRegex(bench.ContractError, "executable changed"):
                bench.execute_invocation({}, [str(executable)], self.contract)

    def test_build_requires_clean_worktree_and_exact_command(self) -> None:
        dirty = {"returncode": 0, "stderr": "", "stdout": "?? scratch\n"}
        with mock.patch.object(bench, "execute_invocation", return_value=dirty):
            with self.assertRaisesRegex(bench.ContractError, "clean worktree"):
                bench.collect_build_provenance(
                    {}, self.contract, Path("/missing-typokat"), Path("/missing-tsgo")
                )
        self.assertEqual(
            bench.release_build_command(),
            [str(bench.cargo_executable()), "build", "--release"],
        )

    def test_user_cargo_config_cannot_influence_authoritative_build(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            root = base / "repo"
            source_home = base / "user-cargo"
            source_home.mkdir(parents=True)
            (source_home / "config.toml").write_text(
                '[build]\nrustflags = ["-C", "target-cpu=native", "-C", "profile-use=evil"]\n',
                encoding="utf-8",
            )
            for relative in ("registry/cache", "registry/index", "registry/src", "git/db"):
                (source_home / relative).mkdir(parents=True)
            build_root = root / "target/full-lib-bench/build-home"
            canonical_home = build_root / "cargo-home"
            with (
                mock.patch.object(bench, "ROOT", root),
                mock.patch.object(bench, "BUILD_HOME_ROOT", build_root),
                mock.patch.object(bench, "CANONICAL_CARGO_HOME", canonical_home),
                mock.patch.dict(os.environ, {"CARGO_HOME": str(source_home)}),
            ):
                record = bench.prepare_canonical_cargo_home()
                environment = bench.build_environment()
                self.assertEqual(environment["CARGO_HOME"], str(canonical_home.resolve()))
                self.assertEqual(environment["CARGO_NET_OFFLINE"], "true")
                self.assertEqual(environment["CARGO_ENCODED_RUSTFLAGS"], "")
                self.assertEqual(environment["CARGO_BUILD_RUSTFLAGS"], "")
                self.assertEqual(
                    environment["CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER"],
                    "/usr/bin/cc",
                )
                self.assertFalse((canonical_home / "config.toml").exists())
                self.assertNotIn("config.toml", {entry["path"] for entry in record["entries"]})
                self.assertEqual(
                    {item["path"] for item in record["exposed"]},
                    {"registry/cache", "registry/index"},
                )
                self.assertFalse((canonical_home / "registry/src").is_symlink())
                self.assertFalse((canonical_home / "git").exists())
                with self.assertRaisesRegex(bench.ContractError, "rustflags must be empty"):
                    bench.validate_effective_rustflags([
                        "-C", "target-cpu=native", "-C", "profile-use=evil",
                    ])

    def test_real_cargo_ancestor_config_is_detected_and_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            ancestor = Path(temporary)
            cargo_config = ancestor / ".cargo/config.toml"
            cargo_config.parent.mkdir()
            cargo_config.write_text(
                '[build]\nrustflags = ["--cfg", "ancestor_injected"]\n', encoding="utf-8"
            )
            crate = ancestor / "crate"
            (crate / "src").mkdir(parents=True)
            (crate / "Cargo.toml").write_text(
                '[package]\nname = "ancestor-probe"\nversion = "0.0.0"\nedition = "2021"\n',
                encoding="utf-8",
            )
            (crate / "src/main.rs").write_text("fn main() {}\n", encoding="utf-8")
            cargo_home = ancestor / "empty-cargo-home"
            cargo_home.mkdir()
            target = ancestor / "target"
            result = bench.run_process(
                [str(bench.cargo_executable()), "build", "--release"],
                timeout=60, max_output=1_048_576, cwd=crate,
                extra_environment={
                    "PATH": f"{bench.cargo_executable().parent}:/usr/bin:/bin",
                    "CARGO_HOME": str(cargo_home), "CARGO_TARGET_DIR": str(target),
                    "CARGO_NET_OFFLINE": "true",
                },
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            fingerprints = list(
                (target / "release/.fingerprint").glob("ancestor-probe-*/bin-ancestor-probe.json")
            )
            self.assertEqual(len(fingerprints), 1)
            observed = json.loads(fingerprints[0].read_text(encoding="utf-8"))
            self.assertEqual(observed["rustflags"], ["--cfg", "ancestor_injected"])
            with self.assertRaisesRegex(bench.ContractError, "ancestor Cargo config"):
                bench.reject_ancestor_cargo_configs(crate)

    def test_stale_bin_fingerprint_injection_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "target"
            fingerprint_root = target / "release/.fingerprint"
            for suffix in ("fresh", "stale"):
                directory = fingerprint_root / f"typokat-{suffix}"
                directory.mkdir(parents=True)
                (directory / "bin-typokat.json").write_text(
                    '{"rustflags": [], "features": "[]", "profile": 1, "config": 2}',
                    encoding="utf-8",
                )
                (directory / "invoked.timestamp").write_text("timestamp", encoding="utf-8")
            output = target / "release/typokat"
            output.write_bytes(b"binary")
            with mock.patch.object(bench, "ISOLATED_TARGET_DIR", target):
                with self.assertRaisesRegex(bench.ContractError, "exactly one"):
                    bench.collect_effective_build_fingerprint()

    def test_isolated_build_provenance_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            root = base / "repository"
            root.mkdir()
            (root / "src").mkdir()
            (root / "Cargo.toml").write_text(
                '[package]\nname = "typokat"\nversion = "0.0.0"\nedition = "2021"\n',
                encoding="utf-8",
            )
            (root / "src/main.rs").write_text("fn main() {}\n", encoding="utf-8")
            (root / ".gitignore").write_text("target/\n", encoding="utf-8")
            subprocess.run(
                [str(bench.cargo_executable()), "generate-lockfile"], cwd=root, check=True,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            git_environment = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}
            for command in (
                ["git", "init", "-q"],
                ["git", "add", "Cargo.toml", "Cargo.lock", "src/main.rs", ".gitignore"],
                [
                    "git", "-c", "user.name=WU0A", "-c", "user.email=wu0a@example.invalid",
                    "commit", "-qm", "fixture",
                ],
            ):
                subprocess.run(command, cwd=root, env=git_environment, check=True)
            comparator = base / "tsgo"
            shutil.copyfile("/bin/true", comparator)
            comparator.chmod(0o755)
            build_home = root / "target/full-lib-bench/build-home"
            cargo_home = build_home / "cargo-home"
            isolated_source = base / "isolated-source"
            isolated_target = isolated_source / "target"
            typokat = root / "target/release/typokat"
            with (
                mock.patch.object(bench, "ROOT", root),
                mock.patch.object(bench, "BUILD_HOME_ROOT", build_home),
                mock.patch.object(bench, "CANONICAL_CARGO_HOME", cargo_home),
                mock.patch.object(bench, "ISOLATED_SOURCE_ROOT", isolated_source),
                mock.patch.object(bench, "ISOLATED_TARGET_DIR", isolated_target),
            ):
                registry: dict[str, object] = {}
                build = bench.collect_build_provenance(
                    registry, self.contract, typokat, comparator
                )
                verified = bench.verify_invocation_registry(registry, self.contract)
                used: set[str] = set()
                seen: set[int] = set()
                bench.verify_build_provenance(
                    build, verified, used, seen, typokat, comparator, self.contract
                )
                self.assertEqual(used, set(registry))
                self.assertEqual(build["source_before"], build["source_after"])
                self.assertEqual(build["effective_build"]["rustflags"], [])

    def test_rust_toolchain_presence_drift_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with mock.patch.object(bench, "ROOT", root):
                recorded = bench.toolchain_file_identities()
                (root / "rust-toolchain.toml").write_text(
                    '[toolchain]\nchannel = "stable"\n', encoding="utf-8"
                )
                with self.assertRaisesRegex(bench.ContractError, "rust-toolchain"):
                    bench.verify_toolchain_file_identities(recorded)

    def test_final_worktree_drift_is_rejected(self) -> None:
        dirty = {"returncode": 0, "stderr": "", "stdout": " M src/main.rs\n"}
        with mock.patch.object(bench, "execute_invocation", return_value=dirty):
            with self.assertRaisesRegex(bench.ContractError, "remain clean"):
                bench.collect_final_worktree({}, self.contract)

    def test_assert_red_rejects_missing_binary_precondition(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            status = bench.main([
                "assert-red", "--typokat", str(Path(temporary) / "missing-typokat")
            ])
        self.assertEqual(status, 1)

    def test_case_insensitive_and_equals_forbidden_flags(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "forbidden benchmark flags"):
            bench.reject_forbidden_command(
                ["tsgo", "--SKIPDEFAULTLIBCHECK=true", "input.ts"], self.contract,
                allow_incremental_false=False,
            )

    def test_strict_json_rejects_duplicates_and_nan(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "duplicate key"):
            bench.strict_json_loads('{"a": 1, "a": 2}', "test")
        with self.assertRaisesRegex(bench.ContractError, "nonstandard numeric"):
            bench.strict_json_loads('{"a": NaN}', "test")

    def test_numeric_validators_reject_bool(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "not bool"):
            bench.require_int(True, "integer")
        with self.assertRaisesRegex(bench.ContractError, "not bool"):
            bench.require_number(False, "number")

    def test_oracle_digest_is_recomputed(self) -> None:
        with self.assertRaisesRegex(bench.ContractError, "oracle digest differs"):
            bench.verify_oracle_digest("0" * 64, "fast-clean")

    def test_fast_crash_cannot_count_as_timing_sample(self) -> None:
        result = bench.ProcessResult(("typokat",), 0, "", "", 0.01, 1)
        with self.assertRaisesRegex(bench.ContractError, "raw exit/output differs"):
            bench.assert_oracle_result(
                result, "typokat", "fast-errors", bench.load_oracles()["rows"]["fast-errors"]
            )

    def test_statistics_are_deterministic(self) -> None:
        blocks = [
            [
                {"tool": "typokat", "wall_seconds": 0.1 + block / 10_000},
                {"tool": "tsgo", "wall_seconds": 0.3 + block / 10_000},
                {"tool": "tsgo", "wall_seconds": 0.301 + block / 10_000},
                {"tool": "typokat", "wall_seconds": 0.101 + block / 10_000},
            ]
            for block in range(15)
        ]
        first = bench.bootstrap_block_speedup_lower(blocks, 1_000, 123)
        second = bench.bootstrap_block_speedup_lower(blocks, 1_000, 123)
        self.assertEqual(first, second)
        self.assertGreater(first, 2.0)

    @unittest.skipUnless(
        os.environ.get("TYPOKAT_FULL_LIB_ACCEPTANCE") == "1",
        "RED until the production full-library provider replaces crates/typokat-check/src/prelude.ts",
    )
    def test_production_path_acceptance(self) -> None:
        binary = bench.ROOT / "target/release/typokat"
        bench.assert_production(binary, self.contract)


def raw_record(
    invocation: str, pid: int, descriptor: dict[str, object],
) -> dict[str, object]:
    executable = bench.guarded_executable(descriptor["argv"])
    identity = bench.executable_identity(executable)
    return {
        "invocation": invocation,
        "pid": pid,
        "started_monotonic_ns": 1_000_000_000,
        "ended_monotonic_ns": 1_100_000_000,
        "wall_seconds": 0.1,
        "returncode": 0,
        "stdout": "",
        "stderr": "",
        "group_clean": True,
        "executable": {
            "path": str(executable), "before": identity, "after": identity,
        },
    }


if __name__ == "__main__":
    unittest.main()

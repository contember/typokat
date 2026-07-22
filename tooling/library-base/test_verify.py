"""RED adversary contract for the frozen-library-base fresh-process gate."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock


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
TYPED_VALIDATION = "2" * 64
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
        "typed_validation_sha256": TYPED_VALIDATION,
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


_FIXTURE_TEMP: tempfile.TemporaryDirectory[str] | None = None
_FIXTURE_EVIDENCE: dict[str, object] | None = None


def _git(root: Path, *arguments: str) -> str:
    result = subprocess.run(
        ["/usr/bin/git", *arguments],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return result.stdout


def _successful_process(
    command: list[str],
    cwd: Path,
    environment: dict[str, str],
    pid: int,
    stdout: str = "build complete",
) -> dict[str, object]:
    return {
        "command": list(command),
        "cwd": str(cwd.resolve()),
        "environment": dict(environment),
        "pid": pid,
        "pgid": pid,
        "exit": 0,
        "stdout": stdout,
        "stderr": "",
        "started_monotonic_ns": pid * 1_000_000,
        "finished_monotonic_ns": pid * 1_000_000 + 100_000_000,
        "wall_ns": 100_000_000,
        "group_clean": True,
        "failure": None,
    }


def _fixture_evidence() -> dict[str, object]:
    global _FIXTURE_TEMP, _FIXTURE_EVIDENCE
    if _FIXTURE_EVIDENCE is not None:
        return _FIXTURE_EVIDENCE
    _FIXTURE_TEMP = tempfile.TemporaryDirectory(prefix="typokat-library-base-test-")
    run_root = Path(_FIXTURE_TEMP.name).resolve()
    origin = run_root / "source"
    origin.mkdir()
    _git(origin, "init", "--quiet", "--initial-branch=main")
    _git(origin, "config", "user.email", "test@example.invalid")
    _git(origin, "config", "user.name", "Library Base Test")
    (origin / ".gitignore").write_text("/target/\n", encoding="utf-8")
    (origin / "source.txt").write_text("frozen source\n", encoding="utf-8")
    _git(origin, "add", ".gitignore", "source.txt")
    _git(origin, "commit", "--quiet", "-m", "fixture")
    roots = [run_root / "build-1", run_root / "build-2"]
    for root in roots:
        _git(run_root, "clone", "--quiet", str(origin), str(root))
    binaries = []
    for root in roots:
        binary = root / "target/release/deps/typokat-libtest"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"byte-identical-release-libtest")
        binary.chmod(0o755)
        binaries.append(verify.file_identity(binary))
    cargo_homes = [run_root / "cargo-home-1", run_root / "cargo-home-2"]
    for cargo_home in cargo_homes:
        cargo_home.mkdir()
    tools = verify.resolve_trusted_tools()
    flags = [
        *(f"--remap-path-prefix={root}=/typokat-library-base/build" for root in roots),
        *(f"--remap-path-prefix={home}=/typokat-library-base/cargo" for home in cargo_homes),
        "--remap-path-scope=all",
    ]
    build_command = [
        tools["cargo"]["invocation"],
        "test",
        "--release",
        "--locked",
        "--offline",
        "--lib",
        "--no-run",
        "--message-format=json-render-diagnostics",
    ]
    builds = []
    for ordinal, (root, cargo_home, binary) in enumerate(
        zip(roots, cargo_homes, binaries, strict=True), 1
    ):
        info = root.lstat()
        environment = {
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
            "TZ": "UTC",
            "CARGO_HOME": str(cargo_home),
            "CARGO_TARGET_DIR": str(root / "target"),
            "CARGO_NET_OFFLINE": "true",
            "CARGO_INCREMENTAL": "0",
            "CARGO_TERM_COLOR": "never",
            "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(flags),
            "CARGO_BUILD_RUSTFLAGS": "",
            "RUSTC": tools["rustc"]["invocation"],
        }
        source = verify.source_snapshot(root)
        builds.append(
            {
                "ordinal": ordinal,
                "root": str(root),
                "device": info.st_dev,
                "inode": info.st_ino,
                "source_before": copy.deepcopy(source),
                "source_after": copy.deepcopy(source),
                "command": list(build_command),
                "environment": dict(environment),
                "process": _successful_process(
                    build_command,
                    root,
                    environment,
                    5_000 + ordinal,
                    json.dumps(
                        {
                            "reason": "compiler-artifact",
                            "target": {"name": "typokat", "kind": ["lib"]},
                            "profile": {"test": True},
                            "executable": binary["path"],
                        },
                        separators=(",", ":"),
                    ),
                ),
                "cargo": copy.deepcopy(tools["cargo"]),
                "rustc": copy.deepcopy(tools["rustc"]),
                "binary": copy.deepcopy(binary),
            }
        )
    records_root = run_root / "records"
    records_root.mkdir()
    pid = 10_000
    wrapper_pid = 20_000
    windows = []
    for window in range(1, 4):
        window_start = window * 2_000_000_000
        cursor = window_start
        records = []
        for ordinal in range(1, 16):
            build = 1 + ((window + ordinal) % 2)
            command = [
                binaries[build - 1]["path"],
                FILTER,
                "--ignored",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ]
            pid_path = records_root / f"{window}-{ordinal}.pid"
            report_path = records_root / f"{window}-{ordinal}.time"
            verify.write_exec_pid(pid_path, pid)
            time_report = "Maximum resident set size (kbytes): 65536\n"
            report_path.write_text(time_report, encoding="utf-8")
            wall_ns = 100_000_000 + ordinal
            launcher = [
                tools["time"]["invocation"],
                "-v",
                "-o",
                str(report_path),
                "--",
                tools["python"]["invocation"],
                "-I",
                "-S",
                "-c",
                verify.EXEC_WRAPPER_CODE,
                str(pid_path),
                *command,
            ]
            records.append(
                {
                    "window": window,
                    "ordinal": ordinal,
                    "kind": "warmup" if ordinal <= 5 else "sample",
                    "build": build,
                    "command": command,
                    "launcher_command": launcher,
                    "environment": {
                        "PATH": "/usr/bin:/bin",
                        "LANG": "C.UTF-8",
                        "LC_ALL": "C.UTF-8",
                        "TZ": "UTC",
                    },
                    "wrapper_pid": wrapper_pid,
                    "pgid": wrapper_pid,
                    "pid": pid,
                    "exit": 0,
                    "stdout": render_stdout(probe()),
                    "stderr": "",
                    "started_monotonic_ns": cursor,
                    "finished_monotonic_ns": cursor + wall_ns,
                    "wall_ns": wall_ns,
                    "group_clean": True,
                    "failure": None,
                    "pid_path": str(pid_path),
                    "time_report_path": str(report_path),
                    "time_report": time_report,
                    "peak_rss_bytes": 64 * 1024 * 1024,
                    "binary_before": copy.deepcopy(binaries[build - 1]),
                    "binary_after": copy.deepcopy(binaries[build - 1]),
                }
            )
            cursor += wall_ns
            pid += 1
            wrapper_pid += 1
        windows.append(
            {
                "ordinal": window,
                "started_monotonic_ns": window_start,
                "finished_monotonic_ns": cursor,
                "records": records,
            }
        )
    source = verify.source_snapshot(origin)
    _FIXTURE_EVIDENCE = {
        "schema": 1,
        "contract_sha256": hashlib.sha256(
            (ROOT / "tooling/library-base/contract.toml").read_bytes()
        ).hexdigest(),
        "revision": source["git_commit"],
        "tree": source["git_tree"],
        "host": {"os": "linux", "arch": "x86_64", "cpu_count": 8},
        "toolchain": copy.deepcopy(tools),
        "source": copy.deepcopy(source),
        "builds": builds,
        "windows": windows,
        "final_source": copy.deepcopy(source),
    }
    return _FIXTURE_EVIDENCE


def evidence() -> dict[str, object]:
    return copy.deepcopy(_fixture_evidence())


class LibraryBaseVerifierContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.contract = verify.load_contract()
        self.evidence = evidence()

    def validate(self) -> dict[str, object]:
        return verify.validate_evidence(self.evidence, self.contract)

    def test_complete_raw_schedule_is_required(self) -> None:
        summary = self.validate()
        self.assertEqual(summary["samples"], 30)
        self.assertEqual(summary["typed_validation_identities"], 1)
        self.assertEqual(summary["outcome"], "GO")

        self.evidence["windows"][1]["records"].pop()
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_two_distinct_clean_byte_identical_release_builds_are_required(self) -> None:
        self.evidence["builds"][1]["root"] = self.evidence["builds"][0]["root"]
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["builds"][1]["binary"]["sha256"] = "3" * 64
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["builds"][0]["source_after"]["git_status"] = "?? generated"
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

    def test_exact_release_filter_and_measurement_launcher_are_required(self) -> None:
        record = self.evidence["windows"][0]["records"][0]
        record["command"][1] = "some_other_test"
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["windows"][0]["records"][0]["launcher_command"][0] = "/bin/true"
        with self.assertRaises(verify.ContractError):
            self.validate()

        self.evidence = evidence()
        self.evidence["windows"][0]["records"][0]["environment"]["RUSTFLAGS"] = "--cfg cheat"
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

    def test_typed_validation_identity_must_be_stable_across_every_process(self) -> None:
        parsed = probe()
        parsed["typed_validation_sha256"] = "4" * 64
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
                record["finished_monotonic_ns"] = record["started_monotonic_ns"] + 120_000_001
                record["wall_ns"] = 120_000_001
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_every_process_must_meet_the_external_rss_gate(self) -> None:
        self.evidence["windows"][2]["records"][7]["peak_rss_bytes"] = 536_870_913
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_trusted_absolute_interpreter_and_toolchain_are_attested(self) -> None:
        tools = verify.resolve_trusted_tools()
        self.assertEqual(tools["python"]["invocation"], "/usr/bin/python3")
        for name in ("python", "cargo", "rustc", "git", "time"):
            with self.subTest(name=name):
                self.assertTrue(Path(tools[name]["path"]).is_absolute())
                self.assertEqual(len(tools[name]["sha256"]), 64)
                self.assertGreater(tools[name]["bytes"], 0)
                self.assertTrue(tools[name]["version"])

    def test_caller_path_cannot_spoof_python_cargo_or_rustc(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fake = Path(temporary)
            for name in ("python3", "cargo", "rustc"):
                path = fake / name
                path.write_text("#!/bin/sh\nexit 99\n", encoding="utf-8")
                path.chmod(0o755)
            verify.resolve_trusted_tools.cache_clear()
            try:
                with mock.patch.dict(os.environ, {"PATH": str(fake)}):
                    tools = verify.resolve_trusted_tools()
            finally:
                verify.resolve_trusted_tools.cache_clear()
        self.assertEqual(tools["python"]["invocation"], "/usr/bin/python3")
        self.assertNotEqual(Path(tools["cargo"]["invocation"]).parent, fake)
        self.assertNotEqual(Path(tools["rustc"]["invocation"]).parent, fake)

    def test_monotonic_nanoseconds_not_gnu_time_centiseconds_authorize_wall(self) -> None:
        report = """\
Elapsed (wall clock) time (h:mm:ss or m:ss): 0:00.12
Maximum resident set size (kbytes): 65536
"""
        self.assertEqual(verify.parse_time_rss_bytes(report), 64 * 1024 * 1024)
        self.assertEqual(verify.monotonic_wall_ns(1, 121_000_002), 121_000_001)

    def test_nonexistent_build_roots_and_revision_are_rejected(self) -> None:
        self.evidence["builds"][0]["root"] = "/definitely/missing/build-root"
        with self.assertRaises(verify.ContractError):
            self.validate()
        self.evidence = evidence()
        self.evidence["source"]["root"] = "/definitely/missing/source-root"
        with self.assertRaises(verify.ContractError):
            self.validate()
        self.evidence = evidence()
        self.evidence["revision"] = "b" * 40
        with self.assertRaises(verify.ContractError):
            self.validate()

    def test_contract_source_toolchain_build_and_process_provenance_are_bound(self) -> None:
        mutations = (
            lambda item: item.__setitem__("contract_sha256", "0" * 64),
            lambda item: item["source"].__setitem__("tracked_sha256", "0" * 64),
            lambda item: item["toolchain"]["cargo"].__setitem__("version", "cargo 9.9.9"),
            lambda item: item["builds"][0]["command"].append("--features=cheat"),
            lambda item: item["builds"][0]["environment"].__setitem__("RUSTFLAGS", "--cfg cheat"),
            lambda item: item["builds"][0]["process"]["command"].append("--features=cheat"),
        )
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                changed = evidence()
                mutate(changed)
                with self.assertRaises(verify.ContractError):
                    verify.validate_evidence(changed, self.contract)

    def test_raw_monotonic_wall_controls_the_gate_not_time_display(self) -> None:
        record = self.evidence["windows"][0]["records"][5]
        record["time_report"] = (
            "Elapsed (wall clock) time (h:mm:ss or m:ss): 0:00.13\n"
            "Maximum resident set size (kbytes): 65536\n"
        )
        verify.validate_evidence(self.evidence, self.contract, require_live=False)
        record["time_report"] = (
            "Elapsed (wall clock) time (h:mm:ss or m:ss): 0:00.12\n"
            "Maximum resident set size (kbytes): 65536\n"
        )
        record["finished_monotonic_ns"] = record["started_monotonic_ns"] + 121_000_000
        record["wall_ns"] = 121_000_000
        with self.assertRaises(verify.ContractError):
            verify.validate_evidence(self.evidence, self.contract, require_live=False)

    def test_raw_exec_pid_and_per_launch_binary_seals_are_bound(self) -> None:
        record = self.evidence["windows"][1]["records"][3]
        record["pid"] = self.evidence["windows"][0]["records"][0]["pid"]
        with self.assertRaises(verify.ContractError):
            verify.validate_evidence(self.evidence, self.contract, require_live=False)

        self.evidence = evidence()
        self.evidence["windows"][1]["records"][3]["binary_after"]["sha256"] = "0" * 64
        with self.assertRaises(verify.ContractError):
            verify.validate_evidence(self.evidence, self.contract, require_live=False)

    def test_actual_libtest_pid_is_distinct_from_wrapper_pid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            pid_file = Path(temporary) / "pid"
            expected = os.getpid()
            verify.write_exec_pid(pid_file, expected)
            self.assertEqual(verify.read_exec_pid(pid_file), expected)

    def test_exec_wrapper_records_the_real_timed_child_pid(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            binary = root / "typokat-libtest"
            binary.write_text(
                "#!/usr/bin/python3\n"
                f"print({render_stdout(probe())!r}, end='')\n",
                encoding="utf-8",
            )
            binary.chmod(0o755)
            identity = verify.file_identity(binary)
            record = verify._run_probe(
                binary,
                root,
                1,
                identity,
                self.contract,
                root / "time.txt",
                root / "pid.txt",
                verify.resolve_trusted_tools(),
            )
            self.assertNotEqual(record["wrapper_pid"], record["pid"])
            self.assertEqual(record["pgid"], record["wrapper_pid"])
            self.assertEqual(verify.read_exec_pid(root / "pid.txt"), record["pid"])
            self.assertEqual(record["binary_before"], record["binary_after"])
            self.assertEqual(
                record["wall_ns"],
                record["finished_monotonic_ns"] - record["started_monotonic_ns"],
            )
            self.assertGreater(record["peak_rss_bytes"], 0)

    def test_each_launch_seals_binary_immediately_before_and_after(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "typokat-libtest"
            binary.write_bytes(b"one")
            before = verify.file_identity(binary)
            binary.write_bytes(b"two")
            after = verify.file_identity(binary)
            with self.assertRaises(verify.ContractError):
                verify.require_same_identity(before, after, "release libtest")

    def test_timeout_and_output_overflow_return_capped_failure_records(self) -> None:
        environment = {
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
            "TZ": "UTC",
        }
        for command, timeout, cap, message in (
            (["/bin/sh", "-c", "sleep 10"], 1, 1024, "timed out"),
            (["/bin/sh", "-c", "yes x"], 5, 128, "output exceeded"),
        ):
            with self.subTest(message=message):
                record = verify.run_bounded_record(
                    command,
                    cwd=ROOT,
                    environment=environment,
                    timeout=timeout,
                    stdout_limit=cap,
                    stderr_limit=cap,
                )
                self.assertIn(message, record["failure"])
                self.assertGreater(record["pid"], 0)
                self.assertEqual(record["pgid"], record["pid"])
                self.assertLessEqual(len(record["stdout"].encode()), cap)
                self.assertLessEqual(len(record["stderr"].encode()), cap)
                self.assertGreater(record["finished_monotonic_ns"], record["started_monotonic_ns"])
                self.assertTrue(record["group_clean"])

    def test_capped_failure_process_is_persisted_in_no_go_evidence(self) -> None:
        environment = {
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
            "TZ": "UTC",
        }
        record = verify.run_bounded_record(
            ["/bin/sh", "-c", "yes x"],
            cwd=ROOT,
            environment=environment,
            timeout=5,
            stdout_limit=128,
            stderr_limit=128,
        )
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "failed.json"
            verify.persist_no_go(path, "output exceeded", {"failed_process": record})
            retained = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(retained["verdict"], "NO-GO")
        failed = retained["partial"]["failed_process"]
        self.assertEqual(failed["pid"], record["pid"])
        self.assertEqual(failed["command"], record["command"])
        self.assertEqual(failed["started_monotonic_ns"], record["started_monotonic_ns"])
        self.assertEqual(failed["finished_monotonic_ns"], record["finished_monotonic_ns"])
        self.assertTrue(failed["group_clean"])

    def test_probe_output_overflow_retains_wrapper_and_libtest_process_record(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            binary = root / "typokat-overflow"
            binary.write_text(
                "#!/usr/bin/python3\n"
                "import sys\n"
                "sys.stdout.write('x' * (2 * 1024 * 1024))\n",
                encoding="utf-8",
            )
            binary.chmod(0o755)
            identity = verify.file_identity(binary)
            with self.assertRaises(verify.RecordedProcessError) as raised:
                verify._run_probe(
                    binary,
                    root,
                    1,
                    identity,
                    self.contract,
                    root / "time.txt",
                    root / "pid.txt",
                    verify.resolve_trusted_tools(),
                )
            failed = raised.exception.record
            self.assertGreater(failed["wrapper_pid"], 0)
            self.assertGreater(failed["pid"], 0)
            self.assertEqual(failed["pgid"], failed["wrapper_pid"])
            self.assertEqual(len(failed["stdout"].encode()), verify.PROBE_OUTPUT_LIMIT)
            self.assertIn("output exceeded", failed["failure"])
            self.assertTrue(failed["group_clean"])


if __name__ == "__main__":
    unittest.main()

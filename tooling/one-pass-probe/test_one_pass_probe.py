#!/usr/bin/env python3
"""Acceptance spec for the non-authoritative complete-combined probe."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import inspect
import io
import json
import math
from pathlib import Path
import re
import shutil
import statistics
import sys
import tempfile
import unittest
from unittest import mock
from contextlib import redirect_stderr, redirect_stdout


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
FULL_BENCH = REPO / "tooling/full-lib-bench"
sys.path.insert(0, str(FULL_BENCH))
import full_lib_bench as bench


IMPLEMENTATION = HERE / "one_pass_probe.py"
EXAMPLE = REPO / "examples/one_pass_probe.rs"
ROWS = ("fast-clean", "fast-errors", "collision", "fanout")
ROUTES = ("production", "one-pass", "tsgo")
TIMING_PANELS = (
    ("OT", "one-pass", "tsgo"),
    ("OP", "one-pass", "production"),
    ("PT", "production", "tsgo"),
)
MEMORY_PANELS = (
    ("OT", "one-pass", "tsgo"),
    ("PT", "production", "tsgo"),
)
PROFILE_SHA256 = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d"
TSGO_SHA256 = "4f2de678286401759b3fb4475bafe35b8f32b4b3a07d92642bbf37eadc9b34a4"
COMMENT = b"// one-pass probe perturbation; semantics unchanged\n"
CANONICAL_PRODUCTION = REPO / "target/release/typokat"
CANONICAL_ONE_PASS = REPO / "target/release/examples/one_pass_probe"


def load_implementation():
    if not IMPLEMENTATION.is_file():
        return None
    spec = importlib.util.spec_from_file_location("one_pass_probe", IMPLEMENTATION)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not construct one-pass probe module spec")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


probe = load_implementation()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def expected_references() -> dict[str, object]:
    return {
        "collector_sha256": sha256(FULL_BENCH / "full_lib_bench.py"),
        "contract_sha256": sha256(FULL_BENCH / "contract.toml"),
        "libraries_sha256": sha256(FULL_BENCH / "expected-libraries.txt"),
        "workloads_sha256": sha256(FULL_BENCH / "workloads.lock"),
        "oracles_sha256": sha256(FULL_BENCH / "oracles.json"),
        "profile_sha256": PROFILE_SHA256,
        "library_count": 82,
        "rows": list(ROWS),
    }


def rotated(items: tuple, amount: int) -> tuple:
    offset = amount % len(items)
    return items[offset:] + items[:offset]


class FakeExecutor:
    """Deterministic stand-in at the authoritative run_process seam."""

    def __init__(
        self, production: Path, one_pass: Path, tsgo: Path, *, corrupt_semantics: bool = False
    ) -> None:
        self.production = production.resolve()
        self.one_pass = one_pass.resolve()
        self.tsgo = tsgo.resolve()
        self.corrupt_semantics = corrupt_semantics
        self.corrupted = False
        self.pid = 20_000
        self.clock = 10_000_000_000
        self.calls: list[dict[str, object]] = []
        self.original_command_counts: dict[tuple[str, str], int] = {}

    def monotonic_ns(self) -> int:
        current = self.clock
        self.clock += 1_000_000
        return current

    def identity(self, path: Path) -> dict[str, object]:
        resolved = path.resolve()
        if resolved == self.tsgo:
            return {"size": 24_101_026, "sha256": TSGO_SHA256}
        return bench.executable_identity(resolved)

    def route(self, argv: tuple[str, ...]) -> tuple[str, bool]:
        memory = argv[0] == "/usr/bin/time"
        executable = Path(argv[2] if memory else argv[0]).resolve()
        if executable == self.production:
            return "production", memory
        if executable == self.one_pass:
            return "one-pass", memory
        if executable == self.tsgo:
            return "tsgo", memory
        raise AssertionError(f"unexpected fake executable: {executable}")

    @staticmethod
    def input_paths(argv: tuple[str, ...]) -> list[Path]:
        return [Path(argument) for argument in argv if argument.endswith((".ts", ".d.ts"))]

    def row(self, argv: tuple[str, ...]) -> tuple[str | None, bool]:
        paths = self.input_paths(argv)
        if not paths:
            return None, False
        for row in ROWS:
            originals = bench.row_sources(row)
            if paths[-len(originals) :] == originals:
                return row, False
            if len(paths) == len(originals) and all(path.is_file() for path in paths):
                candidate = [path.read_bytes() for path in paths]
                expected = [COMMENT + original.read_bytes() for original in originals]
                if candidate == expected:
                    return row, True
        raise AssertionError(f"cannot identify fake row from {paths!r}")

    @staticmethod
    def diagnostic_output(
        route: str, row: str, inputs: list[Path], *, shifted: bool
    ) -> tuple[int, str, str]:
        if row != "fast-errors":
            return 0, "", ""
        lines: list[str] = []
        offset = 1 if shifted else 0
        prefix = "TS" if route == "tsgo" else "TK"
        for line in range(4 + offset, 10 + offset):
            lines.append(
                f"{inputs[0]}({line},7): error {prefix}2322: "
                "Type 'string' is not assignable to type 'number'."
            )
            if route != "tsgo" and line == 4 + offset:
                lines.append("  Type 'string' is not assignable to type 'number'.")
        rendered = "\n".join(lines) + "\n"
        return (1, rendered, "") if route == "tsgo" else (1, "", rendered)

    @staticmethod
    def wall(route: str, row: str | None) -> float:
        if route == "one-pass":
            return 0.5
        if route == "tsgo":
            return 1.0
        return 2.0 if row in {"collision", "fanout"} else 0.8

    @staticmethod
    def rss(route: str, row: str) -> int:
        if route == "one-pass":
            return 80_000
        if route == "tsgo":
            return 100_000
        return 200_000 if row in {"collision", "fanout"} else 90_000

    def __call__(
        self,
        argv,
        *,
        timeout: int,
        max_output: int,
        cwd: Path = REPO,
        extra_environment: dict[str, str] | None = None,
    ) -> bench.ProcessResult:
        command = tuple(str(item) for item in argv)
        route, memory = self.route(command)
        row, shifted = self.row(command)
        self.calls.append(
            {
                "route": route,
                "row": row,
                "argv": command,
                "memory": memory,
                "shifted": shifted,
                "timeout": timeout,
                "max_output": max_output,
                "cwd": Path(cwd).resolve(),
                "extra_environment": extra_environment,
            }
        )

        if command[-3:] == ("library-info", "--format", "json"):
            payload = {
                "schema": 2,
                "profile_sha256": PROFILE_SHA256,
                "file_count": 82,
                "check_route": "production-complete-source-once",
                "provider_route": "production-default-library",
            }
            return self.result(command, 0, json.dumps(payload, separators=(",", ":")) + "\n", "", 0.01)
        if command[-3:] == ("probe-info", "--format", "json"):
            payload = {
                "schema": 1,
                "profile_sha256": PROFILE_SHA256,
                "file_count": 82,
                "probe_route": "test-only-complete-combined",
                "source_backed": True,
                "replay_index": False,
            }
            return self.result(command, 0, json.dumps(payload, separators=(",", ":")) + "\n", "", 0.01)
        if "--version" in command:
            return self.result(command, 0, "Version 7.0.2\n", "", 0.01)
        if "--listFilesOnly" in command:
            if row is None:
                raise AssertionError("listFiles command has no row")
            listed = [
                *(self.tsgo.parent / locked.path for locked in bench.profile_lock()),
                *bench.row_sources(row),
            ]
            return self.result(command, 0, "".join(f"{path.resolve()}\n" for path in listed), "", 0.02)
        if row is None:
            raise AssertionError("compiler command has no row")

        if not shifted and not memory:
            key = (route, row)
            self.original_command_counts[key] = self.original_command_counts.get(key, 0) + 1
        exit_code, stdout, stderr = self.diagnostic_output(
            route, row, self.input_paths(command), shifted=shifted
        )
        if (
            self.corrupt_semantics
            and not self.corrupted
            and route == "production"
            and row == "fast-clean"
            and not shifted
            and not memory
        ):
            stderr = "unexpected semantic output\n"
            self.corrupted = True
        wall = self.wall(route, row)
        if memory:
            if exit_code != 0:
                stderr += f"Command exited with non-zero status {exit_code}\n"
            report = (
                '\tCommand being timed: "probe"\n'
                f"\tMaximum resident set size (kbytes): {self.rss(route, row)}\n"
            )
            stderr += report
        return self.result(command, exit_code, stdout, stderr, wall)

    def result(
        self, argv: tuple[str, ...], returncode: int, stdout: str, stderr: str, wall: float
    ) -> bench.ProcessResult:
        self.pid += 1
        started = self.clock
        ended = started + int(wall * 1_000_000_000)
        self.clock = ended + 10_000_000_000
        return bench.ProcessResult(
            argv, returncode, stdout, stderr, wall, self.pid, started, ended, True
        )


class FakePrereader:
    """Three-route authoritative framing with the fake executor's clock."""

    def __init__(self, executor: FakeExecutor) -> None:
        self.executor = executor

    def __call__(
        self,
        tsgo: Path,
        production: Path,
        one_pass: Path,
        row: str,
        block: int,
    ):
        paths = [
            one_pass.resolve(),
            production.resolve(),
            tsgo.resolve(),
            *(tsgo.parent / locked.path for locked in bench.profile_lock()),
            *bench.row_sources(row),
        ]
        started = self.executor.monotonic_ns()
        digest = hashlib.sha256()
        total = 0
        for path in paths:
            data = bench.verify_regular(path, "fake pre-read input")
            encoded = str(path.resolve()).encode("utf-8")
            digest.update(len(encoded).to_bytes(8, "big"))
            digest.update(len(data).to_bytes(8, "big"))
            digest.update(encoded)
            digest.update(data)
            total += len(data)
        ended = self.executor.monotonic_ns()
        return {
            "block": block,
            "started_monotonic_ns": started,
            "ended_monotonic_ns": ended,
            "paths": [str(path.resolve()) for path in paths],
            "bytes": total,
            "sha256": digest.hexdigest(),
        }


class FakeComparatorVerifier:
    def __init__(self) -> None:
        self.calls: list[tuple[Path, dict[str, object]]] = []

    def __call__(self, binary: Path, contract: dict[str, object]) -> None:
        self.calls.append((binary.resolve(), copy.deepcopy(contract)))


def process_records(evidence: dict[str, object]) -> list[dict[str, object]]:
    records = []
    records.extend(item["record"] for item in evidence["route_probes"].values())
    records.extend(evidence["inventories"].values())
    for row in ROWS:
        records.extend(evidence["semantics"][row].values())
        records.extend(evidence["controls"][row]["records"].values())
        for round_records in evidence["timing"][row]["warmups"]:
            records.extend(round_records)
        for superblock in evidence["timing"][row]["superblocks"]:
            for panel in superblock["panels"]:
                records.extend(panel["records"])
        for superblock in evidence["memory"][row]["superblocks"]:
            for panel in superblock["panels"]:
                records.extend(panel["records"])
    return records


def panel(evidence: dict[str, object], row: str, name: str, *, memory: bool = False):
    section = evidence["memory" if memory else "timing"][row]
    return [
        next(item for item in superblock["panels"] if item["name"] == name)
        for superblock in section["superblocks"]
    ]


def route_values(panels, route: str, key: str) -> list[float]:
    return [
        float(record[key])
        for current in panels
        for record in current["records"]
        if record["route"] == route
    ]


def set_rss(record: dict[str, object], value: int) -> None:
    record["rss_kib"] = value
    record["stderr"] = re.sub(
        r"Maximum resident set size \(kbytes\):\s*\d+",
        f"Maximum resident set size (kbytes): {value}",
        record["stderr"],
    )


class ImplementationPresenceTests(unittest.TestCase):
    def test_probe_runner_exists(self) -> None:
        self.assertIsNotNone(probe, "one-pass probe implementation is absent")
        self.assertTrue(EXAMPLE.is_file(), "one-pass probe implementation is absent")


@unittest.skipUnless(probe is not None, "one-pass probe implementation is absent")
class ProbeContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temporary = tempfile.TemporaryDirectory(prefix="one-pass-probe-spec-")
        cls.root = Path(cls.temporary.name)
        cls.production = cls.root / "typokat"
        cls.one_pass = cls.root / "one_pass_probe"
        cls.tsgo = cls.root / "stage/lib/tsc"
        cls.tsgo.parent.mkdir(parents=True)
        cls.production.write_bytes(b"production-test-binary")
        cls.one_pass.write_bytes(b"one-pass-test-binary")
        cls.tsgo.write_bytes(b"injected-tsgo-binary")
        for binary in (cls.production, cls.one_pass, cls.tsgo):
            binary.chmod(0o755)
        for locked in bench.profile_lock():
            shutil.copyfile(bench.PROFILE / "lib" / locked.path, cls.tsgo.parent / locked.path)
        cls.output = cls.root / "evidence.json"
        cls.executor = FakeExecutor(cls.production, cls.one_pass, cls.tsgo)
        cls.prereader = FakePrereader(cls.executor)
        cls.comparator_verifier = FakeComparatorVerifier()
        cls.execution_conditions = copy.deepcopy(bench.execution_conditions())
        cls.invocation_descriptor_spy = mock.Mock(
            wraps=bench.invocation_descriptor
        )
        with (
            mock.patch.object(
                bench,
                "execution_conditions",
                return_value=copy.deepcopy(cls.execution_conditions),
            ),
            mock.patch.object(
                bench,
                "invocation_descriptor",
                cls.invocation_descriptor_spy,
            ),
        ):
            cls.evidence = probe.run_probe(
                production=cls.production,
                one_pass=cls.one_pass,
                tsgo=cls.tsgo,
                output=cls.output,
                executor=cls.executor,
                identity_reader=cls.executor.identity,
                prereader=cls.prereader,
                comparator_verifier=cls.comparator_verifier,
            )

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temporary.cleanup()

    def validate(self, evidence: dict[str, object] | None = None) -> str:
        candidate = self.evidence if evidence is None else evidence
        return probe.validate_evidence(candidate, Path("/tmp/one-pass-probe-validation.json"))

    def test_full_injected_collection_writes_valid_non_authoritative_evidence(self) -> None:
        self.assertEqual(json.loads(self.output.read_text(encoding="utf-8")), self.evidence)
        self.assertEqual(self.validate(), "PROMISING")
        self.assertEqual(self.evidence["authority"], "non-authoritative-probe")
        self.assertEqual(self.evidence["verdict"], "PROMISING")

    def test_every_collection_call_uses_the_frozen_canonical_invocation(self) -> None:
        contract = bench.verify_contract()
        binaries = {
            "production": self.production,
            "one-pass": self.one_pass,
            "tsgo": self.tsgo,
        }
        for call in self.executor.calls:
            self.assertEqual(call["timeout"], 30)
            self.assertEqual(call["max_output"], 1_048_576)
            self.assertEqual(call["cwd"], REPO.resolve())
            self.assertIsNone(call["extra_environment"])
            route = call["route"]
            row = call["row"]
            argv = call["argv"]
            if row is None:
                expected_args = (
                    ("library-info", "--format", "json")
                    if route == "production"
                    else ("probe-info", "--format", "json")
                )
                self.assertEqual(argv, (str(binaries[route].resolve()), *expected_args))
                continue
            if "--listFilesOnly" in argv:
                self.assertEqual(
                    argv,
                    tuple(bench.tsgo_command(self.tsgo.resolve(), row, contract, list_files=True)),
                )
                continue
            if not call["shifted"] and route == "tsgo":
                compiler = bench.tsgo_command(self.tsgo.resolve(), row, contract)
            elif not call["shifted"]:
                compiler = bench.typokat_command(binaries[route].resolve(), row, contract)
            else:
                inputs = [Path(item["renamed"]) for item in self.evidence["controls"][row]["files"]]
                compiler = (
                    [str(self.tsgo.resolve()), *contract["typescript_flags"], *map(str, inputs)]
                    if route == "tsgo"
                    else [
                        str(binaries[route].resolve()),
                        *contract["typokat_flags"],
                        *map(str, inputs),
                    ]
                )
            expected = ["/usr/bin/time", "-v", *compiler] if call["memory"] else compiler
            self.assertEqual(argv, tuple(expected))

    def test_authoritative_process_diagnostic_and_rss_primitives_are_reused(self) -> None:
        self.assertIs(probe.bench, bench)
        for name in (
            "run_process",
            "sanitized_environment",
            "invocation_descriptor",
            "process_record",
            "executable_identity",
            "normalize_diagnostics",
            "compiler_result_without_time",
            "verify_staged_comparator",
            "verify_listed_inventory",
            "profile_lock",
            "workload_lock",
            "load_oracles",
            "verify_regular",
        ):
            self.assertIs(getattr(probe.bench, name), getattr(bench, name))
        signature = inspect.signature(probe.run_probe)
        self.assertIs(signature.parameters["executor"].default, bench.run_process)
        self.assertIs(
            signature.parameters["identity_reader"].default, bench.executable_identity
        )
        self.assertIs(
            signature.parameters["prereader"].default,
            probe.three_route_preread_attestation,
        )
        self.assertIs(
            signature.parameters["comparator_verifier"].default,
            bench.verify_staged_comparator,
        )
        self.assertEqual(len(self.comparator_verifier.calls), 1)
        binary, contract = self.comparator_verifier.calls[0]
        self.assertEqual(binary, self.tsgo.resolve())
        self.assertEqual(contract, bench.verify_contract())
        self.assertEqual(
            self.invocation_descriptor_spy.call_count,
            len(self.executor.calls),
        )
        for descriptor_call in self.invocation_descriptor_spy.call_args_list:
            self.assertEqual(descriptor_call.args[1], bench.sanitized_environment())
            self.assertEqual(Path(descriptor_call.args[2]).resolve(), REPO.resolve())

    def test_python_cli_wires_exact_paths_status_and_fail_closed_usage(self) -> None:
        self.assertIs(inspect.signature(probe.main).parameters["runner"].default, probe.run_probe)
        calls: list[dict[str, Path]] = []

        def promising_runner(**kwargs):
            calls.append(kwargs)
            return {"verdict": "PROMISING"}

        argv = [
            "run",
            "--production",
            str(self.production),
            "--one-pass",
            str(self.one_pass),
            "--tsgo",
            str(self.tsgo),
            "--output",
            str(self.root / "cli-evidence.json"),
        ]
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = probe.main(argv, runner=promising_runner)
        self.assertEqual(status, 0)
        self.assertEqual(stdout.getvalue(), "PROMISING\n")
        self.assertEqual(stderr.getvalue(), "")
        self.assertEqual(
            calls,
            [
                {
                    "production": self.production.resolve(),
                    "one_pass": self.one_pass.resolve(),
                    "tsgo": self.tsgo.resolve(),
                    "output": (self.root / "cli-evidence.json").resolve(),
                }
            ],
        )

        def negative_runner(**kwargs):
            del kwargs
            return {"verdict": "NOT-PROMISING"}

        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = probe.main(argv, runner=negative_runner)
        self.assertEqual(status, 1)
        self.assertEqual(stdout.getvalue(), "NOT-PROMISING\n")
        self.assertEqual(stderr.getvalue(), "")

        for invalid in ([], ["unknown"], ["run", "--production", str(self.production)]):
            with self.subTest(argv=invalid):
                called = []

                def must_not_run(**kwargs):
                    called.append(kwargs)
                    return {"verdict": "PROMISING"}

                with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                    status = probe.main(invalid, runner=must_not_run)
                self.assertEqual(status, 2)
                self.assertEqual(called, [])

        forbidden = [*argv[:-1], str(FULL_BENCH / "evidence/forbidden.json")]
        called = []

        def forbidden_runner(**kwargs):
            called.append(kwargs)
            return {"verdict": "PROMISING"}

        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            status = probe.main(forbidden, runner=forbidden_runner)
        self.assertEqual(status, 2)
        self.assertEqual(called, [])

        probe.validate_live_cli_paths(CANONICAL_PRODUCTION, CANONICAL_ONE_PASS)
        with self.assertRaisesRegex(probe.ProbeError, "canonical|production|one-pass|release"):
            probe.validate_live_cli_paths(self.production, self.one_pass)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            canonical_production = root / "target/release/typokat"
            canonical_one_pass = root / "target/release/examples/one_pass_probe"
            canonical_one_pass.parent.mkdir(parents=True)
            canonical_production.write_bytes(b"canonical production")
            canonical_one_pass.write_bytes(b"canonical one-pass")
            with mock.patch.object(probe, "ROOT", root):
                probe.validate_live_cli_paths(canonical_production, canonical_one_pass)
                wrapper = root / "wrapper"
                wrapper.write_bytes(b"wrapper")
                canonical_production.unlink()
                canonical_production.symlink_to(wrapper)
                with self.assertRaisesRegex(
                    probe.ProbeError, "canonical|symlink|production|release"
                ):
                    probe.validate_live_cli_paths(
                        canonical_production, canonical_one_pass
                    )

        canonical_argv = [
            "run",
            "--production",
            str(CANONICAL_PRODUCTION),
            "--one-pass",
            str(CANONICAL_ONE_PASS),
            "--tsgo",
            str(self.tsgo),
            "--output",
            str(self.root / "canonical-cli-evidence.json"),
        ]
        stdout = io.StringIO()
        stderr = io.StringIO()
        with (
            mock.patch.object(
                probe,
                "validate_live_cli_paths",
                side_effect=probe.ProbeError("live path sentinel"),
            ),
            mock.patch.object(
                probe,
                "verify_references",
                side_effect=AssertionError("collection started before live path validation"),
            ),
            redirect_stdout(stdout),
            redirect_stderr(stderr),
        ):
            status = probe.main(canonical_argv)
        self.assertEqual(status, 2)
        self.assertEqual(stdout.getvalue(), "")
        self.assertRegex(stderr.getvalue(), "live path sentinel")

    def test_references_parse_exact_locks_and_verify_all_profile_bytes(self) -> None:
        self.assertEqual(tuple(probe.ROUTES), ROUTES)
        self.assertEqual(tuple(probe.ROWS), ROWS)
        self.assertEqual(tuple(tuple(item) for item in probe.TIMING_PANELS), TIMING_PANELS)
        self.assertEqual(probe.BOOTSTRAP_RESAMPLES, 100_000)
        self.assertEqual(probe.BOOTSTRAP_SEED, 20_260_721)
        self.assertEqual(probe.verify_references(), expected_references())
        self.assertEqual(probe.verify_profile_inventory(bench.PROFILE / "lib"), 82)

        copied = self.root / "mutated-profile"
        shutil.copytree(bench.PROFILE / "lib", copied)
        target = copied / bench.profile_lock()[17].path
        target.write_bytes(target.read_bytes() + b"\n")
        with self.assertRaisesRegex(probe.ProbeError, "library|profile|drift|SHA"):
            probe.verify_profile_inventory(copied)

        copied_workloads = self.root / "mutated-workloads"
        shutil.copytree(bench.WORKLOADS, copied_workloads)
        workload = copied_workloads / "fast-clean/main.ts"
        workload.write_bytes(workload.read_bytes() + b"\n")
        with mock.patch.object(bench, "WORKLOADS", copied_workloads):
            with self.assertRaisesRegex(probe.ProbeError, "workload|drift|SHA|bytes"):
                probe.verify_references()

        copied_oracles = self.root / "mutated-oracles.json"
        copied_oracles.write_bytes(bench.ORACLES_PATH.read_bytes() + b"\n")
        with mock.patch.object(bench, "ORACLES_PATH", copied_oracles):
            with self.assertRaisesRegex(probe.ProbeError, "oracle|drift|SHA|JSON"):
                probe.verify_references()

    def test_staged_listfiles_inventory_is_raw_exact_and_ordered(self) -> None:
        for row in ROWS:
            lines = self.evidence["inventories"][row]["stdout"].splitlines()
            self.assertEqual(len(lines), 82 + len(bench.row_sources(row)))
        candidate = copy.deepcopy(self.evidence)
        lines = candidate["inventories"]["fast-clean"]["stdout"].splitlines()
        lines[0], lines[1] = lines[1], lines[0]
        candidate["inventories"]["fast-clean"]["stdout"] = "\n".join(lines) + "\n"
        with self.assertRaisesRegex(probe.ProbeError, "listFiles|inventory|order"):
            self.validate(candidate)

    def test_route_probes_are_raw_and_binary_identities_are_exact(self) -> None:
        production = self.evidence["route_probes"]["production"]
        one_pass = self.evidence["route_probes"]["one-pass"]
        self.assertEqual(
            production["args"], ["library-info", "--format", "json"]
        )
        self.assertEqual(one_pass["args"], ["probe-info", "--format", "json"])
        probe.validate_production_info(production["observed"])
        probe.validate_probe_info(one_pass["observed"])
        self.assertEqual(one_pass["observed"]["source_backed"], True)
        self.assertEqual(one_pass["observed"]["replay_index"], False)
        for route in ROUTES:
            identity = self.evidence["identities"][route]
            self.assertGreater(identity["size"], 0)
            self.assertRegex(identity["sha256"], r"^[0-9a-f]{64}$")
        one_pass_sha = self.evidence["identities"]["one-pass"]["sha256"]
        for record in process_records(self.evidence):
            self.assertEqual(record["executable"]["before"], record["executable"]["after"])
            if record["route"] == "one-pass":
                self.assertEqual(record["executable"]["before"]["sha256"], one_pass_sha)

        candidate = copy.deepcopy(self.evidence)
        candidate["route_probes"]["one-pass"]["observed"]["unexpected"] = True
        with self.assertRaisesRegex(probe.ProbeError, "schema|keys"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        candidate["identities"]["one-pass"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(probe.ProbeError, "identity|SHA|sha"):
            self.validate(candidate)

    def test_timing_has_cyclic_warmups_prereads_and_rotating_three_panel_superblocks(self) -> None:
        route_cycle = ("one-pass", "production", "tsgo")
        panel_cycle = ("OT", "OP", "PT")
        for row in ROWS:
            expected_paths = [
                str(self.one_pass.resolve()),
                str(self.production.resolve()),
                str(self.tsgo.resolve()),
                *(str((self.tsgo.parent / locked.path).resolve()) for locked in bench.profile_lock()),
                *(str(path.resolve()) for path in bench.row_sources(row)),
            ]
            timing = self.evidence["timing"][row]
            self.assertEqual(len(timing["warmups"]), 5)
            for ordinal, current in enumerate(timing["warmups"]):
                self.assertEqual(
                    tuple(record["route"] for record in current),
                    rotated(route_cycle, ordinal),
                )
            self.assertEqual(len(timing["superblocks"]), 15)
            for ordinal, superblock in enumerate(timing["superblocks"]):
                self.assertEqual(
                    tuple(item["name"] for item in superblock["panels"]),
                    rotated(panel_cycle, ordinal),
                )
                self.assertEqual(superblock["pre_read"]["paths"], expected_paths)
                self.assertGreater(superblock["pre_read"]["bytes"], 2_936_611)
                self.assertRegex(superblock["pre_read"]["sha256"], r"^[0-9a-f]{64}$")
                for current in superblock["panels"]:
                    left = current["left"]
                    right = current["right"]
                    self.assertEqual(
                        tuple(record["route"] for record in current["records"]),
                        (left, right, right, left),
                    )

        candidate = copy.deepcopy(self.evidence)
        candidate["timing"]["fast-clean"]["warmups"].pop()
        with self.assertRaisesRegex(probe.ProbeError, "warmup|five|5"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        paths = candidate["timing"]["fast-clean"]["superblocks"][0]["pre_read"]["paths"]
        paths[0], paths[1] = paths[1], paths[0]
        with self.assertRaisesRegex(probe.ProbeError, "pre-read|path|inventory"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        pre_read = candidate["timing"]["fast-clean"]["superblocks"][0]["pre_read"]
        pre_read["ended_monotonic_ns"] = pre_read["started_monotonic_ns"] - 1
        with self.assertRaisesRegex(probe.ProbeError, "pre-read|chronology|time"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        candidate["timing"]["fast-clean"]["superblocks"][0]["pre_read"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(probe.ProbeError, "pre-read|attestation|SHA|sha"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        candidate["timing"]["fast-clean"]["superblocks"][0]["panels"].reverse()
        with self.assertRaisesRegex(probe.ProbeError, "panel|rotation|order"):
            self.validate(candidate)

    def test_known_production_tsgo_failures_are_evidence_only(self) -> None:
        for row in ("collision", "fanout"):
            pt = panel(self.evidence, row, "PT")
            production = statistics.median(route_values(pt, "production", "wall_seconds"))
            tsgo = statistics.median(route_values(pt, "tsgo", "wall_seconds"))
            self.assertLess(tsgo / production, 1.0)
            pt_memory = panel(self.evidence, row, "PT", memory=True)
            production_rss = statistics.median(route_values(pt_memory, "production", "rss_kib"))
            tsgo_rss = statistics.median(route_values(pt_memory, "tsgo", "rss_kib"))
            self.assertGreater(production_rss / tsgo_rss, 1.25)
        self.assertEqual(self.validate(), "PROMISING")

    def test_every_binding_timing_surface_has_an_independent_falsifier(self) -> None:
        bindings = [(row, "OT") for row in ROWS] + [
            ("collision", "OP"),
            ("fanout", "OP"),
        ]
        for row, name in bindings:
            with self.subTest(row=row, panel=name):
                candidate = copy.deepcopy(self.evidence)
                for current in panel(candidate, row, name):
                    for record in current["records"]:
                        if record["route"] == "one-pass":
                            record["wall_seconds"] = 4.0
                            record["ended_monotonic_ns"] = (
                                record["started_monotonic_ns"] + 4_000_000_000
                            )
                candidate["verdict"] = "NOT-PROMISING"
                self.assertEqual(self.validate(candidate), "NOT-PROMISING")

    def test_p95_and_complete_superblock_bootstrap_are_binding(self) -> None:
        candidate = copy.deepcopy(self.evidence)
        ot = panel(candidate, "fast-clean", "OT")
        for record in ot[0]["records"]:
            if record["route"] == "one-pass":
                record["wall_seconds"] = 2.0
                record["ended_monotonic_ns"] = record["started_monotonic_ns"] + 2_000_000_000
        candidate["verdict"] = "NOT-PROMISING"
        self.assertEqual(self.validate(candidate), "NOT-PROMISING")

        candidate = copy.deepcopy(self.evidence)
        ot = panel(candidate, "fast-clean", "OT")
        for ordinal, current in enumerate(ot):
            one_pass, tsgo = (0.5, 1.1) if ordinal < 8 else (0.9, 0.8)
            for record in current["records"]:
                value = one_pass if record["route"] == "one-pass" else tsgo
                record["wall_seconds"] = value
                record["ended_monotonic_ns"] = (
                    record["started_monotonic_ns"] + int(value * 1_000_000_000)
                )
        candidate["verdict"] = "NOT-PROMISING"
        self.assertEqual(self.validate(candidate), "NOT-PROMISING")

    def test_memory_has_five_preread_dual_abba_superblocks_and_only_o_is_binding(self) -> None:
        for row in ROWS:
            memory = self.evidence["memory"][row]
            expected_paths = [
                str(self.one_pass.resolve()),
                str(self.production.resolve()),
                str(self.tsgo.resolve()),
                *(str((self.tsgo.parent / locked.path).resolve()) for locked in bench.profile_lock()),
                *(str(path.resolve()) for path in bench.row_sources(row)),
            ]
            self.assertEqual(len(memory["superblocks"]), 5)
            counts = {route: 0 for route in ROUTES}
            for ordinal, superblock in enumerate(memory["superblocks"]):
                expected = ("OT", "PT") if ordinal % 2 == 0 else ("PT", "OT")
                self.assertEqual(tuple(item["name"] for item in superblock["panels"]), expected)
                self.assertEqual(
                    superblock["pre_read"]["paths"],
                    expected_paths,
                )
                for current in superblock["panels"]:
                    for record in current["records"]:
                        counts[record["route"]] += 1
                    self.assertEqual(
                        tuple(record["route"] for record in current["records"]),
                        (current["left"], current["right"], current["right"], current["left"]),
                    )
            self.assertEqual(counts, {"production": 10, "one-pass": 10, "tsgo": 20})

        candidate = copy.deepcopy(self.evidence)
        for current in panel(candidate, "collision", "OT", memory=True):
            for record in current["records"]:
                if record["route"] == "one-pass":
                    set_rss(record, 200_000)
                elif record["route"] == "tsgo":
                    set_rss(record, 100_000)
        candidate["verdict"] = "NOT-PROMISING"
        self.assertEqual(self.validate(candidate), "NOT-PROMISING")

        candidate = copy.deepcopy(self.evidence)
        for current in panel(candidate, "collision", "PT", memory=True):
            for record in current["records"]:
                if record["route"] == "production":
                    set_rss(record, 500_000)
        self.assertEqual(self.validate(candidate), "PROMISING")

    def test_every_phase_is_oracle_checked_and_controls_use_shifted_lines(self) -> None:
        for row in ROWS:
            files = self.evidence["controls"][row]["files"]
            originals = bench.row_sources(row)
            self.assertEqual(len(files), len(originals))
            for retained, original in zip(files, originals, strict=True):
                self.assertNotEqual(retained["original"], retained["renamed"])
                self.assertEqual(retained["source"].encode("utf-8"), COMMENT + original.read_bytes())
        controls = self.evidence["controls"]["fast-errors"]["records"].values()
        for record in controls:
            output = record["stdout"] + record["stderr"]
            self.assertIn("(5,7):", output)
            self.assertIn("(10,7):", output)
            self.assertNotIn("(4,7):", output)
        first_typokat = self.evidence["controls"]["fast-errors"]["records"]["one-pass"]
        self.assertIn("\n  Type 'string' is not assignable", first_typokat["stderr"])

        targets = [
            self.evidence["timing"]["fast-errors"]["warmups"][0][0],
            panel(self.evidence, "fast-errors", "OT")[0]["records"][0],
            panel(self.evidence, "fast-errors", "OT", memory=True)[0]["records"][0],
        ]
        for target in targets:
            with self.subTest(phase=target["phase"]):
                candidate = copy.deepcopy(self.evidence)
                match = next(
                    record for record in process_records(candidate) if record["sequence"] == target["sequence"]
                )
                channel = "stdout" if match["route"] == "tsgo" else "stderr"
                match[channel] = ""
                with self.assertRaisesRegex(probe.ProbeError, "oracle|diagnostic|output"):
                    self.validate(candidate)

    def test_fast_crash_channels_malformed_duplicate_reason_and_output_cap_fail_closed(self) -> None:
        base_target = self.evidence["timing"]["fast-clean"]["warmups"][0][0]
        candidate = copy.deepcopy(self.evidence)
        target = next(r for r in process_records(candidate) if r["sequence"] == base_target["sequence"])
        target["returncode"] = 101
        target["wall_seconds"] = 0.001
        target["ended_monotonic_ns"] = target["started_monotonic_ns"] + 1_000_000
        target["stderr"] = "panic\n"
        with self.assertRaisesRegex(probe.ProbeError, "crash|exit|oracle|output"):
            self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        target = next(r for r in process_records(candidate) if r["sequence"] == base_target["sequence"])
        target["stderr"] = "unknown stderr with a successful exit\n"
        with self.assertRaisesRegex(probe.ProbeError, "channel|output|diagnostic|oracle"):
            self.validate(candidate)

        error_target = self.evidence["timing"]["fast-errors"]["warmups"][0][0]
        mutations = (
            ("wrong channel", lambda r: r.update(stdout=r["stderr"], stderr="")),
            ("malformed", lambda r: r.update(stderr="malformed diagnostic\n")),
            (
                "extra",
                lambda r: r.update(
                    stderr=r["stderr"]
                    + f"{r['argv'][-1]}(11,7): error TK2322: Extra diagnostic.\n"
                ),
            ),
            ("duplicate", lambda r: r.update(stderr=r["stderr"] + r["stderr"].splitlines()[0] + "\n")),
            ("reason", lambda r: r.update(stderr="  Type 'string' is not assignable to type 'number'.\n")),
            ("cap", lambda r: r.update(stderr="x" * 1_048_577)),
        )
        for label, mutate in mutations:
            with self.subTest(label=label):
                candidate = copy.deepcopy(self.evidence)
                target = next(
                    r for r in process_records(candidate) if r["sequence"] == error_target["sequence"]
                )
                mutate(target)
                with self.assertRaisesRegex(
                    probe.ProbeError, "channel|diagnostic|Reason|output|cap|oracle"
                ):
                    self.validate(candidate)

    def test_process_numbers_chronology_pid_group_and_identity_are_fail_closed(self) -> None:
        first = process_records(self.evidence)[0]
        for field, values in (
            ("pid", (0, -1, True)),
            ("wall_seconds", (0.0, -1.0, True, math.nan, math.inf)),
            ("started_monotonic_ns", (0, -1, True)),
            ("ended_monotonic_ns", (0, -1, True)),
            ("returncode", (True, math.nan, math.inf)),
        ):
            for value in values:
                with self.subTest(field=field, value=value):
                    candidate = copy.deepcopy(self.evidence)
                    target = next(
                        r for r in process_records(candidate) if r["sequence"] == first["sequence"]
                    )
                    target[field] = value
                    with self.assertRaisesRegex(
                        probe.ProbeError, "numeric|finite|PID|time|bool|returncode"
                    ):
                        self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        target = process_records(candidate)[0]
        target["ended_monotonic_ns"] = target["started_monotonic_ns"] - 1
        with self.assertRaisesRegex(probe.ProbeError, "chronology|time|reversed"):
            self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        records = sorted(process_records(candidate), key=lambda item: item["sequence"])
        records[1]["started_monotonic_ns"] = records[0]["ended_monotonic_ns"] - 1
        records[1]["ended_monotonic_ns"] = (
            records[1]["started_monotonic_ns"] + int(records[1]["wall_seconds"] * 1_000_000_000)
        )
        with self.assertRaisesRegex(probe.ProbeError, "chronology|overlap|order"):
            self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        target = process_records(candidate)[0]
        target["group_clean"] = False
        with self.assertRaisesRegex(probe.ProbeError, "group|process"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        target = process_records(candidate)[0]
        target["executable"]["after"]["sha256"] = "f" * 64
        with self.assertRaisesRegex(probe.ProbeError, "identity|changed|executable"):
            self.validate(candidate)

    def test_invocation_descriptor_and_global_pid_sequence_are_exact(self) -> None:
        self.assertEqual(
            self.evidence["execution_conditions"],
            self.execution_conditions,
        )
        for descriptor in self.evidence["invocations"].values():
            self.assertEqual(descriptor["cwd"], str(REPO.resolve()))
            self.assertEqual(descriptor["env"], bench.sanitized_environment())
            self.assertEqual(
                {
                    "affinity": descriptor["affinity"],
                    "nice": descriptor["nice"],
                    "rlimits": descriptor["rlimits"],
                },
                self.execution_conditions,
            )
        candidate = copy.deepcopy(self.evidence)
        first = process_records(candidate)[0]
        descriptor = candidate["invocations"][first["invocation"]]
        descriptor["env"]["EXTRA"] = "not sanitized"
        with self.assertRaisesRegex(probe.ProbeError, "invocation|environment|digest"):
            self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        process_records(candidate)[0]["unexpected"] = None
        with self.assertRaisesRegex(probe.ProbeError, "schema|keys"):
            self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        record = panel(candidate, "fast-clean", "OT")[0]["records"][0]
        forged = copy.deepcopy(candidate["invocations"][record["invocation"]])
        forged["argv"].insert(-1, "--skipLibCheck")
        forged_key = bench.sha256_bytes(bench.canonical_json(forged))
        candidate["invocations"][forged_key] = forged
        record["invocation"] = forged_key
        with self.assertRaisesRegex(
            probe.ProbeError, "canonical|command|flag|invocation|skipLibCheck"
        ):
            self.validate(candidate)

        for condition in ("affinity", "rlimits"):
            with self.subTest(condition=condition):
                candidate = copy.deepcopy(self.evidence)
                record = panel(candidate, "fast-clean", "OT")[0]["records"][0]
                forged = copy.deepcopy(candidate["invocations"][record["invocation"]])
                if condition == "affinity":
                    forged["affinity"] = [*forged["affinity"], max(forged["affinity"]) + 1]
                else:
                    current = forged["rlimits"]["RLIMIT_CORE"]
                    forged["rlimits"]["RLIMIT_CORE"] = (
                        [0, 0] if current != [0, 0] else [1, 1]
                    )
                forged_key = bench.sha256_bytes(bench.canonical_json(forged))
                candidate["invocations"][forged_key] = forged
                record["invocation"] = forged_key
                with self.assertRaisesRegex(
                    probe.ProbeError,
                    "execution condition|affinity|rlimit|snapshot|invocation",
                ):
                    self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        records = process_records(candidate)
        records[-1]["pid"] = records[0]["pid"]
        with self.assertRaisesRegex(probe.ProbeError, "PID|pid|reused"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        records = process_records(candidate)
        records[-1]["sequence"] = records[0]["sequence"]
        with self.assertRaisesRegex(probe.ProbeError, "sequence|order"):
            self.validate(candidate)

    def test_raw_time_report_is_the_only_rss_authority(self) -> None:
        target = panel(self.evidence, "fast-clean", "OT", memory=True)[0]["records"][0]
        for value in (None, 0, -1, True, math.nan, math.inf):
            with self.subTest(value=value):
                candidate = copy.deepcopy(self.evidence)
                record = next(
                    r for r in process_records(candidate) if r["sequence"] == target["sequence"]
                )
                record["rss_kib"] = value
                with self.assertRaisesRegex(probe.ProbeError, "RSS|rss|memory|finite|bool"):
                    self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        record = next(r for r in process_records(candidate) if r["sequence"] == target["sequence"])
        record["rss_kib"] += 1
        with self.assertRaisesRegex(probe.ProbeError, "RSS|rss|report"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        record = next(r for r in process_records(candidate) if r["sequence"] == target["sequence"])
        record["stderr"] = ""
        with self.assertRaisesRegex(probe.ProbeError, "RSS|rss|time"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        record = next(
            r for r in process_records(candidate) if r["sequence"] == target["sequence"]
        )
        record["stderr"] = "unknown wrapper output\n" + record["stderr"]
        with self.assertRaisesRegex(
            probe.ProbeError, "channel|diagnostic|oracle|output"
        ):
            self.validate(candidate)

        error_target = panel(self.evidence, "fast-errors", "OT", memory=True)[0][
            "records"
        ][0]
        self.assertIn("Command exited with non-zero status 1\n", error_target["stderr"])
        for label, replacement in (
            ("missing", ""),
            (
                "moved",
                "Command exited with non-zero status 1\nunknown wrapper output\n",
            ),
            ("missing-newline", "Command exited with non-zero status 1"),
            ("mismatch", "Command exited with non-zero status 2\n"),
            ("signal", "Command terminated by signal 9\n"),
            ("stopped", "Command stopped by signal 9\n"),
        ):
            with self.subTest(time_status=label):
                candidate = copy.deepcopy(self.evidence)
                record = next(
                    r
                    for r in process_records(candidate)
                    if r["sequence"] == error_target["sequence"]
                )
                record["stderr"] = record["stderr"].replace(
                    "Command exited with non-zero status 1\n", replacement
                )
                with self.assertRaisesRegex(
                    probe.ProbeError, "time|status|signal|oracle|output"
                ):
                    self.validate(candidate)

        non_memory = self.evidence["timing"]["fast-errors"]["warmups"][0][0]
        candidate = copy.deepcopy(self.evidence)
        record = next(
            r
            for r in process_records(candidate)
            if r["sequence"] == non_memory["sequence"]
        )
        record["stderr"] += "Command exited with non-zero status 1\n"
        with self.assertRaisesRegex(probe.ProbeError, "diagnostic|oracle|output"):
            self.validate(candidate)

        candidate = copy.deepcopy(self.evidence)
        for current in panel(candidate, "fast-clean", "OT", memory=True):
            for record in current["records"]:
                if record["route"] == "one-pass":
                    set_rss(record, 524_289)
        candidate["verdict"] = "NOT-PROMISING"
        self.assertEqual(self.validate(candidate), "NOT-PROMISING")

    def test_semantic_mismatch_stops_before_warmup_timing_or_memory(self) -> None:
        output = self.root / "semantic-red.json"
        executor = FakeExecutor(
            self.production, self.one_pass, self.tsgo, corrupt_semantics=True
        )
        prereader = FakePrereader(executor)
        comparator_verifier = FakeComparatorVerifier()
        with self.assertRaisesRegex(probe.ProbeError, "semantic|oracle|output"):
            probe.run_probe(
                production=self.production,
                one_pass=self.one_pass,
                tsgo=self.tsgo,
                output=output,
                executor=executor,
                identity_reader=executor.identity,
                prereader=prereader,
                comparator_verifier=comparator_verifier,
            )
        self.assertTrue(executor.corrupted)
        self.assertFalse(any(call["memory"] for call in executor.calls))
        self.assertLessEqual(max(executor.original_command_counts.values()), 1)
        self.assertEqual(len(comparator_verifier.calls), 1)
        self.assertFalse(output.exists())

    def test_schema_authority_and_output_location_are_fail_closed(self) -> None:
        candidate = copy.deepcopy(self.evidence)
        candidate["unexpected"] = None
        with self.assertRaisesRegex(probe.ProbeError, "schema|keys"):
            self.validate(candidate)
        candidate = copy.deepcopy(self.evidence)
        candidate["authority"] = "authoritative"
        candidate["verdict"] = "GO"
        with self.assertRaisesRegex(probe.ProbeError, "authority|GO|verdict"):
            self.validate(candidate)

        forbidden = FULL_BENCH / "evidence/one-pass.json"
        with self.assertRaisesRegex(probe.ProbeError, "full-lib-bench|output"):
            probe.validate_output_path(forbidden)
        with tempfile.TemporaryDirectory() as temporary:
            link = Path(temporary) / "authoritative"
            link.symlink_to(FULL_BENCH, target_is_directory=True)
            with self.assertRaisesRegex(probe.ProbeError, "full-lib-bench|output"):
                probe.validate_output_path(link / "evidence.json")
        probe.validate_output_path(Path("/tmp/one-pass-probe.json"))


if __name__ == "__main__":
    unittest.main()

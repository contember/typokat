"""Acceptance contract for the transparent same-tree verifier."""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


VERIFY = Path(__file__).resolve().parent / "verify.py"
GIT_ENVIRONMENT = {
    **os.environ,
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_AUTHOR_NAME": "Verifier Contract",
    "GIT_AUTHOR_EMAIL": "verifier@example.invalid",
    "GIT_COMMITTER_NAME": "Verifier Contract",
    "GIT_COMMITTER_EMAIL": "verifier@example.invalid",
}


def run_git(root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *arguments],
        cwd=root,
        env=GIT_ENVIRONMENT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def initialize_repository(root: Path, *, untracked: bool = False) -> str:
    run_git(root, "init", "-q")
    (root / "tracked.txt").write_text("tracked v1\n", encoding="utf-8")
    run_git(root, "add", "tracked.txt")
    run_git(root, "commit", "-qm", "fixture")
    if untracked:
        (root / "untracked.txt").write_text("untracked v1\n", encoding="utf-8")
    return run_git(root, "rev-parse", "HEAD").stdout.strip()


def run_verify(
    root: Path, gates: list[str], *, dry_run: bool = False
) -> subprocess.CompletedProcess[str]:
    arguments = [sys.executable, str(VERIFY)]
    if dry_run:
        arguments.append("--dry-run")
    for gate in gates:
        arguments.extend(("--gate", gate))
    return subprocess.run(
        arguments,
        cwd=root,
        env=os.environ.copy(),
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def output_record(output: str, label: str):
    prefix = f"{label}: "
    matches = [line.removeprefix(prefix) for line in output.splitlines() if line.startswith(prefix)]
    if len(matches) != 1:
        raise AssertionError(f"expected one {label!r} record in:\n{output}")
    return json.loads(matches[0])


class ResolutionContractTests(unittest.TestCase):
    def resolve(self, gates: list[str]) -> tuple[list[str], list[list[str]]]:
        with tempfile.TemporaryDirectory(prefix="typokat-verify-spec-") as temporary:
            root = Path(temporary)
            initialize_repository(root)
            result = run_verify(root, gates, dry_run=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return (
            output_record(result.stdout, "requested"),
            output_record(result.stdout, "resolved"),
        )

    def test_exact_duplicates_are_removed_without_reordering(self) -> None:
        gates = [
            "cargo test --test manifest",
            "cargo test -p typokat-core --lib",
            "cargo test --test manifest",
            "cargo test -p typokat-core --lib",
        ]
        requested, resolved = self.resolve(gates)
        self.assertEqual(requested, gates)
        self.assertEqual(
            resolved,
            [
                ["cargo", "test", "--test", "manifest"],
                ["cargo", "test", "-p", "typokat-core", "--lib"],
            ],
        )

    def test_plain_workspace_test_subsumes_only_plain_package_tests(self) -> None:
        gates = [
            "cargo test -p typokat-core",
            "cargo test --package typokat-types",
            "cargo test",
        ]
        requested, resolved = self.resolve(gates)
        self.assertEqual(requested, gates)
        self.assertEqual(resolved, [["cargo", "test"]])

    def test_filtered_and_semantically_distinct_commands_are_retained(self) -> None:
        gates = [
            "cargo test -p typokat-core unit_name",
            "cargo test -p typokat-types -- --ignored",
            "cargo test -p typokat-binder -- --nocapture",
            "cargo test -p typokat-relate --example relation_demo",
            "cargo test -p typokat-check --features test-utils",
            "cargo test -p typokat-driver --lib",
            "cargo nextest run -p typokat-frontend",
            "python3 tooling/custom_gate.py --scope package",
            "cargo test",
        ]
        requested, resolved = self.resolve(gates)
        self.assertEqual(requested, gates)
        self.assertEqual(resolved, [shlex.split(gate) for gate in gates])

    def test_filtered_workspace_test_does_not_subsume_plain_package_test(self) -> None:
        gates = ["cargo test -p typokat-core", "cargo test workspace_filter"]
        _, resolved = self.resolve(gates)
        self.assertEqual(resolved, [shlex.split(gate) for gate in gates])


class ExecutionContractTests(unittest.TestCase):
    def test_dry_run_prints_requested_resolved_and_stable_tree_identity(self) -> None:
        with tempfile.TemporaryDirectory(prefix="typokat-verify-spec-") as temporary:
            root = Path(temporary)
            head = initialize_repository(root, untracked=True)
            marker = root / "must-not-exist"
            gate = shlex.join(
                [
                    sys.executable,
                    "-c",
                    "from pathlib import Path; Path('must-not-exist').touch()",
                ]
            )
            first = run_verify(root, [gate, gate], dry_run=True)
            second = run_verify(root, [gate, gate], dry_run=True)

            self.assertFalse(marker.exists())

        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(output_record(first.stdout, "requested"), [gate, gate])
        self.assertEqual(
            output_record(first.stdout, "resolved"),
            [[sys.executable, "-c", "from pathlib import Path; Path('must-not-exist').touch()"]],
        )
        first_tree = output_record(first.stdout, "tree")
        second_tree = output_record(second.stdout, "tree")
        self.assertEqual(first_tree, second_tree)
        self.assertEqual(first_tree["head"], head)
        self.assertRegex(first_tree["content"], r"^[0-9a-f]{64}$")

    def test_tree_identity_covers_tracked_and_untracked_content(self) -> None:
        with tempfile.TemporaryDirectory(prefix="typokat-verify-spec-") as temporary:
            root = Path(temporary)
            head = initialize_repository(root, untracked=True)
            harmless_gate = shlex.join([sys.executable, "-c", "pass"])
            initial = run_verify(root, [harmless_gate], dry_run=True)
            (root / "tracked.txt").write_text("tracked v2\n", encoding="utf-8")
            tracked_changed = run_verify(root, [harmless_gate], dry_run=True)
            (root / "tracked.txt").write_text("tracked v1\n", encoding="utf-8")
            (root / "untracked.txt").write_text("untracked v2\n", encoding="utf-8")
            untracked_changed = run_verify(root, [harmless_gate], dry_run=True)

        for result in (initial, tracked_changed, untracked_changed):
            self.assertEqual(result.returncode, 0, result.stderr)
        identities = [
            output_record(result.stdout, "tree")
            for result in (initial, tracked_changed, untracked_changed)
        ]
        self.assertEqual({identity["head"] for identity in identities}, {head})
        self.assertEqual(len({identity["content"] for identity in identities}), 3)

    def test_gate_is_shlex_parsed_and_never_executed_through_a_shell(self) -> None:
        with tempfile.TemporaryDirectory(prefix="typokat-verify-spec-") as temporary:
            root = Path(temporary)
            initialize_repository(root)
            shell_marker = root / "shell-created"
            script = "import json, sys; print('ARGV=' + json.dumps(sys.argv[1:]))"
            gate = (
                f"{shlex.quote(sys.executable)} -c {shlex.quote(script)} "
                f"'argument with spaces' ; touch {shlex.quote(str(shell_marker))}"
            )
            result = run_verify(root, [gate])

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(shell_marker.exists())
            expected_arguments = ["argument with spaces", ";", "touch", str(shell_marker)]
            self.assertIn("ARGV=" + json.dumps(expected_arguments), result.stdout)

    def test_gate_failure_status_is_propagated(self) -> None:
        with tempfile.TemporaryDirectory(prefix="typokat-verify-spec-") as temporary:
            root = Path(temporary)
            initialize_repository(root)
            gate = shlex.join([sys.executable, "-c", "raise SystemExit(7)"])
            result = run_verify(root, [gate])

        self.assertEqual(result.returncode, 7, result.stderr)

    def assert_mutation_rejected(self, script: str) -> None:
        with tempfile.TemporaryDirectory(prefix="typokat-verify-spec-") as temporary:
            root = Path(temporary)
            initialize_repository(root, untracked=True)
            gate = shlex.join([sys.executable, "-c", script])
            result = run_verify(root, [gate])

        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)tree.*chang|chang.*tree")

    def test_gate_that_changes_tracked_content_is_rejected(self) -> None:
        self.assert_mutation_rejected(
            "from pathlib import Path; Path('tracked.txt').write_text('changed')"
        )

    def test_gate_that_changes_untracked_state_is_rejected(self) -> None:
        self.assert_mutation_rejected(
            "from pathlib import Path; Path('created.txt').write_text('new')"
        )


if __name__ == "__main__":
    unittest.main()

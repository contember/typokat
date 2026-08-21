"""Static contract for the release-binary fanout in pull-request CI."""

from __future__ import annotations

import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "ci.yml"
ARTIFACT_NAME = "typokat-release-${{ github.sha }}"
ARTIFACT_FILES = (
    "target/release/typokat",
    "target/release/typokat.sha256",
    "target/release/typokat.commit",
)


def _job(workflow: str, name: str) -> str:
    match = re.search(
        rf"(?ms)^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [a-zA-Z0-9_-]+:\n|\Z)",
        workflow,
    )
    if match is None:
        raise AssertionError(f"workflow job not found: {name}")
    return match.group("body")


def _action_step(job: str, action: str) -> str:
    matches = re.findall(
        rf"(?ms)^      - (?:name: [^\n]+\n        )?uses: {re.escape(action)}\n"
        rf"(?P<body>.*?)(?=^      - |\Z)",
        job,
    )
    if len(matches) != 1:
        raise AssertionError(
            f"expected exactly one {action} step, found {len(matches)}"
        )
    return matches[0]


def _with_scalar(action_step: str, key: str) -> str:
    match = re.search(rf"(?m)^          {re.escape(key)}:\s*([^\n]+)\s*$", action_step)
    if match is None:
        raise AssertionError(f"action input not found: {key}")
    return match.group(1).strip().strip("'\"")


def _with_block(action_step: str, key: str) -> tuple[str, ...]:
    match = re.search(
        rf"(?ms)^          {re.escape(key)}:\s*\|\s*\n"
        rf"(?P<body>(?:^            [^\n]+\n?)+)",
        action_step,
    )
    if match is None:
        raise AssertionError(f"multiline action input not found: {key}")
    return tuple(
        line.strip()
        for line in match.group("body").splitlines()
        if line.strip()
    )


def _normalized(text: str) -> str:
    return " ".join(text.split())


class ReleaseArtifactWorkflowContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.workflow = WORKFLOW_PATH.read_text(encoding="utf-8")

    def test_build_uploads_authenticated_release_binary(self) -> None:
        build = _job(self.workflow, "build")
        normalized = _normalized(build)

        build_position = normalized.find("cargo build --release")
        digest_position = normalized.find(
            "sha256sum target/release/typokat > target/release/typokat.sha256"
        )
        commit_match = re.search(
            r"\$\{\{\s*github\.sha\s*\}\}.*?>\s*target/release/typokat\.commit",
            normalized,
        )
        upload_position = normalized.find("uses: actions/upload-artifact@v4")

        self.assertGreaterEqual(build_position, 0, "build must retain the release build")
        self.assertGreaterEqual(
            digest_position,
            0,
            "build must write the binary SHA-256 manifest",
        )
        self.assertIsNotNone(
            commit_match,
            "build must write the exact workflow commit identity",
        )
        self.assertGreaterEqual(upload_position, 0, "build must upload the release artifact")
        self.assertLess(build_position, digest_position)
        self.assertLess(build_position, commit_match.start())
        self.assertLess(digest_position, upload_position)
        self.assertLess(commit_match.start(), upload_position)

        upload = _action_step(build, "actions/upload-artifact@v4")
        self.assertEqual(_with_scalar(upload, "name"), ARTIFACT_NAME)
        self.assertEqual(_with_block(upload, "path"), ARTIFACT_FILES)

        retention = int(_with_scalar(upload, "retention-days"))
        self.assertGreaterEqual(retention, 1)
        self.assertLessEqual(retention, 7, "release artifact retention must stay short")

    def test_pr_consumers_use_the_authenticated_build_artifact(self) -> None:
        for job_name in ("official-suite", "differential"):
            with self.subTest(job=job_name):
                job = _job(self.workflow, job_name)
                normalized = _normalized(job)

                self.assertRegex(job, r"(?m)^    needs:\s*build\s*$")
                self.assertNotRegex(job, r"cargo\s+build\s+--release")

                download = _action_step(job, "actions/download-artifact@v4")
                self.assertEqual(_with_scalar(download, "name"), ARTIFACT_NAME)
                self.assertEqual(_with_scalar(download, "path"), "target/release")

                download_position = normalized.find("uses: actions/download-artifact@v4")
                commit_position = normalized.find(
                    'test "$(cat target/release/typokat.commit)" = "${{ github.sha }}"'
                )
                digest_position = normalized.find(
                    "sha256sum --check target/release/typokat.sha256"
                )
                chmod_position = normalized.find("chmod +x target/release/typokat")

                self.assertGreaterEqual(commit_position, 0, "consumer must verify commit identity")
                self.assertGreaterEqual(digest_position, 0, "consumer must verify binary digest")
                self.assertGreaterEqual(chmod_position, 0, "consumer must restore executable mode")
                self.assertLess(download_position, commit_position)
                self.assertLess(download_position, digest_position)
                self.assertLess(commit_position, chmod_position)
                self.assertLess(digest_position, chmod_position)

                first_harness_position = min(
                    position
                    for command in self._harness_commands(job_name)
                    if (position := normalized.find(command)) >= 0
                )
                self.assertLess(chmod_position, first_harness_position)

    def test_existing_pr_harness_commands_remain_explicit(self) -> None:
        for job_name in ("official-suite", "differential"):
            with self.subTest(job=job_name):
                normalized = _normalized(_job(self.workflow, job_name))
                for command in self._harness_commands(job_name):
                    self.assertIn(command, normalized)

    def test_weekly_tsc_truth_job_keeps_its_independent_build(self) -> None:
        job = _job(self.workflow, "differential-tsc")
        normalized = _normalized(job)

        self.assertNotRegex(job, r"(?m)^    needs:\s*build\s*$")
        self.assertNotIn("actions/download-artifact@", job)
        self.assertEqual(len(re.findall(r"cargo\s+build\s+--release", job)), 1)
        self.assertIn("if: github.event_name == 'schedule'", normalized)
        self.assertIn(
            "python3 differential.py repros --save --refresh-tsc",
            normalized,
        )
        self.assertIn("git diff --exit-code scoreboard.txt", normalized)
        self.assertIn("python3 differential.py fuzz --ref tsc", normalized)
        self.assertIn("--seed ${{ github.run_number }} --count 400 --time-budget 300", normalized)

    @staticmethod
    def _harness_commands(job_name: str) -> tuple[str, ...]:
        if job_name == "official-suite":
            return (
                "python3 tsofficial.py fetch",
                "python3 tsofficial.py run --check",
            )
        if job_name == "differential":
            return (
                "python3 -m unittest test_differential",
                "python3 differential.py repros --check",
                "python3 differential.py fuzz",
                "--ref ../../target/release/typokat",
                "--seed 96 --count 600 --time-budget 120 --check",
            )
        raise AssertionError(f"unsupported consumer job: {job_name}")


if __name__ == "__main__":
    unittest.main()

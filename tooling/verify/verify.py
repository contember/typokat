#!/usr/bin/env python3
"""Run an explicit list of verification gates against one unchanged Git tree."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shlex
import stat
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence


TREE_CHANGE_EXIT = 2


class VerificationError(Exception):
    """A verifier precondition failed."""


@dataclass(frozen=True)
class TreeIdentity:
    head: str
    content: str

    def record(self) -> dict[str, str]:
        return {"head": self.head, "content": self.content}


def git_output(arguments: Sequence[str], *, cwd: Path) -> bytes:
    try:
        result = subprocess.run(
            ["git", *arguments],
            cwd=cwd,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as error:
        raise VerificationError(f"cannot run git: {error}") from error
    if result.returncode != 0:
        detail = result.stderr.decode(errors="replace").strip()
        raise VerificationError(detail or f"git {' '.join(arguments)} failed")
    return result.stdout


def repository_root(cwd: Path) -> Path:
    raw_root = git_output(["rev-parse", "--show-toplevel"], cwd=cwd).rstrip(b"\n")
    if not raw_root:
        raise VerificationError("git returned an empty repository root")
    return Path(os.fsdecode(raw_root))


def hash_part(digest: "hashlib._Hash", label: bytes, value: bytes) -> None:
    digest.update(label)
    digest.update(len(value).to_bytes(8, byteorder="big"))
    digest.update(value)


def untracked_record(root: Path, relative_name: bytes) -> tuple[bytes, bytes, bytes]:
    path = os.path.join(os.fsencode(root), relative_name)
    try:
        metadata = os.lstat(path)
        if stat.S_ISLNK(metadata.st_mode):
            kind = b"symlink"
            content = os.fsencode(os.readlink(path))
        elif stat.S_ISREG(metadata.st_mode):
            kind = b"regular"
            with open(path, "rb") as file:
                content = file.read()
        else:
            name = os.fsdecode(relative_name)
            raise VerificationError(f"unsupported untracked file kind for {name!r}")
        permissions = stat.S_IMODE(metadata.st_mode).to_bytes(4, byteorder="big")
        return kind, permissions, content
    except OSError as error:
        name = os.fsdecode(relative_name)
        raise VerificationError(f"cannot read untracked file {name!r}: {error}") from error


def snapshot_tree(root: Path) -> TreeIdentity:
    head = git_output(["rev-parse", "HEAD"], cwd=root).decode("ascii").strip()
    staged_diff = git_output(
        ["diff", "--cached", "--binary", "--no-ext-diff", "HEAD", "--"], cwd=root
    )
    unstaged_diff = git_output(
        ["diff", "--binary", "--no-ext-diff", "--"], cwd=root
    )
    raw_untracked = git_output(
        ["ls-files", "--others", "--exclude-standard", "-z"], cwd=root
    )
    untracked_names = sorted(name for name in raw_untracked.split(b"\0") if name)

    digest = hashlib.sha256()
    hash_part(digest, b"staged-diff\0", staged_diff)
    hash_part(digest, b"unstaged-diff\0", unstaged_diff)
    for name in untracked_names:
        kind, permissions, content = untracked_record(root, name)
        hash_part(digest, b"untracked-name\0", name)
        hash_part(digest, b"untracked-kind\0", kind)
        hash_part(digest, b"untracked-permissions\0", permissions)
        hash_part(digest, b"untracked-content\0", content)
    return TreeIdentity(head=head, content=digest.hexdigest())


def parse_gates(gates: Sequence[str]) -> list[list[str]]:
    parsed: list[list[str]] = []
    for gate in gates:
        try:
            arguments = shlex.split(gate)
        except ValueError as error:
            raise VerificationError(f"cannot parse gate {gate!r}: {error}") from error
        if not arguments:
            raise VerificationError("gate must not be empty")
        parsed.append(arguments)
    return parsed


def is_plain_package_test(arguments: Sequence[str]) -> bool:
    return (
        len(arguments) == 4
        and arguments[0:2] == ["cargo", "test"]
        and arguments[2] in ("-p", "--package")
        and bool(arguments[3])
        and not arguments[3].startswith("-")
    )


def workspace_default_package_names(root: Path) -> set[str] | None:
    try:
        result = subprocess.run(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--locked",
            ],
            cwd=root,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except OSError:
        return None
    if result.returncode != 0:
        return None
    try:
        metadata = json.loads(result.stdout)
        packages = metadata["packages"]
        default_members = metadata["workspace_default_members"]
        workspace_members = metadata["workspace_members"]
        if not all(
            isinstance(value, list)
            for value in (packages, default_members, workspace_members)
        ):
            return None

        names_by_id: dict[str, list[str]] = {}
        for package in packages:
            package_id = package["id"]
            package_name = package["name"]
            if not isinstance(package_id, str) or not isinstance(package_name, str):
                return None
            names_by_id.setdefault(package_id, []).append(package_name)

        workspace_packages: list[tuple[str, str]] = []
        for package_id in workspace_members:
            if not isinstance(package_id, str):
                return None
            matching_names = names_by_id.get(package_id)
            if matching_names is None or len(matching_names) != 1:
                return None
            workspace_packages.append((package_id, matching_names[0]))
        if not all(isinstance(package_id, str) for package_id in default_members):
            return None
        default_ids = set(default_members)
        if len(default_ids) != len(default_members):
            return None

        name_counts: dict[str, int] = {}
        for _, name in workspace_packages:
            name_counts[name] = name_counts.get(name, 0) + 1
        return {
            name
            for package_id, name in workspace_packages
            if package_id in default_ids and name_counts[name] == 1
        }
    except (KeyError, TypeError, ValueError):
        return None


def resolve_gates(gates: Sequence[Sequence[str]], root: Path) -> list[list[str]]:
    unique: list[list[str]] = []
    seen: set[tuple[str, ...]] = set()
    for gate in gates:
        key = tuple(gate)
        if key not in seen:
            seen.add(key)
            unique.append(list(gate))

    has_plain_workspace_test = ("cargo", "test") in seen
    if not has_plain_workspace_test:
        return unique
    package_candidates = [gate for gate in unique if is_plain_package_test(gate)]
    if not package_candidates:
        return unique
    default_package_names = workspace_default_package_names(root)
    if default_package_names is None:
        return unique
    return [
        gate
        for gate in unique
        if not (
            is_plain_package_test(gate)
            and gate[3] in default_package_names
        )
    ]


def print_record(label: str, value: object) -> None:
    print(f"{label}: {json.dumps(value, ensure_ascii=True, separators=(',', ':'))}", flush=True)


def require_same_tree(expected: TreeIdentity, root: Path) -> None:
    actual = snapshot_tree(root)
    if actual != expected:
        raise VerificationError(
            "tree changed during verification: "
            f"expected {json.dumps(expected.record(), sort_keys=True)}, "
            f"found {json.dumps(actual.record(), sort_keys=True)}"
        )


def execute_gates(gates: Sequence[Sequence[str]], identity: TreeIdentity, root: Path) -> int:
    for gate in gates:
        require_same_tree(identity, root)
        try:
            result = subprocess.run(list(gate), cwd=root, check=False)
        except OSError as error:
            raise VerificationError(f"cannot execute gate {shlex.join(gate)}: {error}") from error
        require_same_tree(identity, root)
        if result.returncode != 0:
            if result.returncode < 0:
                return 128 - result.returncode
            return result.returncode
    return 0


def argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run explicit verification gates while requiring an unchanged Git tree."
    )
    parser.add_argument(
        "--gate",
        action="append",
        required=True,
        help="explicit command string; repeat for each gate",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved plan without executing it",
    )
    return parser


def main(arguments: Sequence[str] | None = None) -> int:
    options = argument_parser().parse_args(arguments)
    try:
        root = repository_root(Path.cwd())
        parsed = parse_gates(options.gate)
        resolved = resolve_gates(parsed, root)
        identity = snapshot_tree(root)
        print_record("requested", options.gate)
        print_record("resolved", resolved)
        print_record("tree", identity.record())
        if options.dry_run:
            return 0
        return execute_gates(resolved, identity, root)
    except VerificationError as error:
        print(f"verify: {error}", file=sys.stderr)
        return TREE_CHANGE_EXIT


if __name__ == "__main__":
    raise SystemExit(main())

"""Adversarial RED contract for the default-library package coordinator."""

from __future__ import annotations

import copy
import importlib.util
import io
import os
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent
IMPLEMENTATION = ROOT / "verify.py"
SPEC = importlib.util.spec_from_file_location("typokat_library_package_verify", IMPLEMENTATION)
if SPEC is None or SPEC.loader is None:
    raise ImportError(f"cannot load {IMPLEMENTATION}")
verify = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verify)


class PackageCoordinatorContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = verify.load_contract(ROOT / "contract.toml")
        cls.profile_assets = verify._profile_asset_inventory(
            verify.PACKAGE_ROOT / verify.PROFILE_PREFIX, cls.contract
        )

    def setUp(self) -> None:
        self.record = {
            "schema": 1,
            "clean_roots": ["/isolated/one", "/isolated/two"],
            "profile_sha256": self.contract["profile_sha256"],
            "dts_sources": self.contract["dts_sources"],
            "licenses": list(self.contract["licenses"]),
            "cargo_checks": 2,
            "build_scripts": 0,
            "source_mutations": 0,
        }
        self.inventory = {
            *self.profile_assets,
            "Cargo.toml",
            "Cargo.toml.orig",
        }

    def assert_record_rejected(self, mutate) -> None:
        candidate = copy.deepcopy(self.record)
        mutate(candidate)
        with self.assertRaises(verify.ContractError):
            verify.validate_record(candidate, self.contract)

    def test_contract_schema_is_exact(self) -> None:
        changed = dict(self.contract)
        changed["invented"] = True
        with self.assertRaises(verify.ContractError):
            verify.validate_contract(changed)
        changed = dict(self.contract)
        del changed["profile_sha256"]
        with self.assertRaises(verify.ContractError):
            verify.validate_contract(changed)

    def test_two_distinct_clean_roots_are_mandatory(self) -> None:
        verify.validate_record(self.record, self.contract)
        self.assert_record_rejected(
            lambda row: row.__setitem__("clean_roots", ["same", "same"])
        )
        self.assert_record_rejected(
            lambda row: row.__setitem__("clean_roots", ["/isolated/one"])
        )
        self.assert_record_rejected(
            lambda row: row.__setitem__("clean_roots", ["relative/one", "relative/two"])
        )

    def test_package_inventory_is_exact(self) -> None:
        verify.validate_package_inventory(self.inventory, self.contract)
        missing = set(self.inventory)
        missing.remove("src/typescript-6.0.3/profile.toml")
        with self.assertRaises(verify.ContractError):
            verify.validate_package_inventory(missing, self.contract)
        extra = set(self.inventory)
        extra.add("build.rs")
        with self.assertRaises(verify.ContractError):
            verify.validate_package_inventory(extra, self.contract)
        extra = set(self.inventory)
        extra.add(verify.PROFILE_PREFIX + "unrecorded.bin")
        with self.assertRaises(verify.ContractError):
            verify.validate_package_inventory(extra, self.contract)

    def test_build_and_package_cannot_generate_or_mutate_sources(self) -> None:
        self.assert_record_rejected(lambda row: row.__setitem__("build_scripts", 1))
        self.assert_record_rejected(lambda row: row.__setitem__("source_mutations", 1))

    def test_exact_profile_assets_and_checks_are_bound(self) -> None:
        for key, changed in (
            ("profile_sha256", "0" * 64),
            ("dts_sources", 81),
            ("licenses", ["LICENSE.txt"]),
            ("cargo_checks", 1),
        ):
            self.assert_record_rejected(
                lambda row, key=key, changed=changed: row.__setitem__(key, changed)
            )

    def test_nonstandard_custom_build_manifest_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            manifest = Path(temporary) / "Cargo.toml"
            manifest.write_text(
                '[package]\nname = "fixture"\nversion = "0.0.0"\n'
                'build = "scripts/generate.rs"\n',
                encoding="utf-8",
            )
            with self.assertRaises(verify.ContractError):
                verify._validate_manifest_build_surface(manifest)
            manifest.write_text(
                '[package]\nname = "fixture"\nversion = "0.0.0"\nbuild = false\n',
                encoding="utf-8",
            )
            self.assertEqual(verify._validate_manifest_build_surface(manifest), 0)

    def test_cargo_metadata_custom_build_target_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            manifest = Path(temporary) / "Cargo.toml"
            manifest.touch()
            metadata = {
                "packages": [
                    {
                        "manifest_path": str(manifest.resolve()),
                        "targets": [{"kind": ["custom-build"]}],
                    }
                ],
                "workspace_members": [],
                "workspace_default_members": [],
                "resolve": None,
                "target_directory": str(Path(temporary) / "target"),
                "build_directory": str(Path(temporary) / "target"),
                "version": 1,
                "workspace_root": temporary,
                "metadata": None,
            }
            with self.assertRaises(verify.ContractError):
                verify._validate_cargo_metadata(metadata, manifest)
            metadata["packages"][0]["targets"][0]["kind"] = ["lib"]
            self.assertEqual(verify._validate_cargo_metadata(metadata, manifest), 0)

    def test_tree_snapshot_covers_untracked_ignored_modes_and_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ignored = root / ".ignored-output"
            untracked = root / "untracked.txt"
            ignored.write_bytes(b"ignored")
            untracked.write_bytes(b"untracked")
            before = verify._tree_snapshot(root)
            self.assertEqual(set(before), {".ignored-output", "untracked.txt"})
            untracked.chmod(0o700)
            untracked.write_bytes(b"changed")
            after = verify._tree_snapshot(root)
            self.assertEqual(
                verify._snapshot_mutations(before, after), ["untracked.txt"]
            )

    def test_observed_subprocess_rejects_any_source_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "source.txt").write_bytes(b"before")
            observations = {
                "cargo_checks": 0,
                "build_scripts": 0,
                "source_mutations": 0,
            }
            with self.assertRaises(verify.ContractError):
                verify._run_observed(
                    [
                        sys.executable,
                        "-c",
                        "from pathlib import Path; Path('ignored.bin').write_bytes(b'x')",
                    ],
                    cwd=root,
                    environment=dict(os.environ),
                    source_root=root,
                    observations=observations,
                    require_git_clean=False,
                )
            self.assertEqual(observations["source_mutations"], 1)

    @unittest.skipUnless(hasattr(os, "symlink"), "symlinks are unavailable")
    def test_no_follow_inventory_rejects_symlinks_and_special_files(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            target.write_bytes(b"target")
            (root / "link").symlink_to(target)
            with self.assertRaises(verify.ContractError):
                verify._tree_snapshot(root)
        if hasattr(os, "mkfifo"):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                os.mkfifo(root / "pipe")
                with self.assertRaises(verify.ContractError):
                    verify._tree_snapshot(root)

    def test_archive_duplicate_and_inventory_drift_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            duplicate = root / "duplicate.crate"
            with tarfile.open(duplicate, "w:gz") as package:
                for payload in (b"first", b"second"):
                    member = tarfile.TarInfo("fixture-0.0.0/Cargo.toml")
                    member.size = len(payload)
                    package.addfile(member, io.BytesIO(payload))
            with self.assertRaises(verify.ContractError):
                verify._extract_crate(
                    duplicate, root / "duplicate-output", {"Cargo.toml"}
                )
            mismatch = root / "mismatch.crate"
            with tarfile.open(mismatch, "w:gz") as package:
                payload = b"[package]\n"
                member = tarfile.TarInfo("fixture-0.0.0/Cargo.toml")
                member.size = len(payload)
                package.addfile(member, io.BytesIO(payload))
            with self.assertRaises(verify.ContractError):
                verify._extract_crate(
                    mismatch, root / "mismatch-output", {"Cargo.lock"}
                )


if __name__ == "__main__":
    unittest.main()

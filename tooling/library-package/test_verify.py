"""Adversarial RED contract for the WU2 package coordinator."""

from __future__ import annotations

import copy
import importlib.util
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
    def setUp(self) -> None:
        self.contract = verify.load_contract(ROOT / "contract.toml")
        self.record = {
            "schema": 1,
            "clean_roots": ["/isolated/one", "/isolated/two"],
            "generation_sha256": [self.contract["artifact_sha256"]] * 2,
            "artifact_bytes": self.contract["artifact_bytes"],
            "artifact_sha256": self.contract["artifact_sha256"],
            "profile_sha256": self.contract["profile_sha256"],
            "dts_sources": self.contract["dts_sources"],
            "licenses": list(self.contract["licenses"]),
            "cargo_checks": 2,
            "build_scripts": 0,
            "build_generations": 0,
            "source_mutations": 0,
        }
        self.inventory = {
            *self.contract["required_package_assets"],
            *(f"src/library/typescript-6.0.3/lib/lib-{index}.d.ts" for index in range(82)),
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
        del changed["artifact_sha256"]
        with self.assertRaises(verify.ContractError):
            verify.validate_contract(changed)

    def test_two_distinct_clean_generations_are_mandatory(self) -> None:
        self.assert_record_rejected(
            lambda row: row.__setitem__("clean_roots", ["same", "same"])
        )
        self.assert_record_rejected(
            lambda row: row.__setitem__("generation_sha256", [row["artifact_sha256"]])
        )
        self.assert_record_rejected(
            lambda row: row["generation_sha256"].__setitem__(1, "0" * 64)
        )

    def test_package_inventory_is_exact(self) -> None:
        verify.validate_package_inventory(self.inventory, self.contract)
        missing = set(self.inventory)
        missing.remove("src/library/typescript-6.0.3/canonical.snapshot")
        with self.assertRaises(verify.ContractError):
            verify.validate_package_inventory(missing, self.contract)
        extra = set(self.inventory)
        extra.add("build.rs")
        with self.assertRaises(verify.ContractError):
            verify.validate_package_inventory(extra, self.contract)

    def test_build_and_package_cannot_generate_or_mutate_sources(self) -> None:
        self.assert_record_rejected(lambda row: row.__setitem__("build_scripts", 1))
        self.assert_record_rejected(lambda row: row.__setitem__("build_generations", 1))
        self.assert_record_rejected(lambda row: row.__setitem__("source_mutations", 1))

    def test_exact_artifact_profile_assets_and_checks_are_bound(self) -> None:
        for key, changed in (
            ("artifact_bytes", 1),
            ("artifact_sha256", "0" * 64),
            ("profile_sha256", "0" * 64),
            ("dts_sources", 81),
            ("licenses", ["LICENSE.txt"]),
            ("cargo_checks", 1),
        ):
            self.assert_record_rejected(
                lambda row, key=key, changed=changed: row.__setitem__(key, changed)
            )


if __name__ == "__main__":
    unittest.main()

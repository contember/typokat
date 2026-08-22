#!/usr/bin/env python3
"""Authenticate and evaluate the project-local library index feasibility corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
CASE_DIR = ROOT / "crates/typokat-library/src/project_local_index_feasibility_cases"
LIB_ES5 = ROOT / "crates/typokat-library/src/typescript-6.0.3/lib/lib.es5.d.ts"
LIB_REFLECT = ROOT / "crates/typokat-library/src/typescript-6.0.3/lib/lib.es2015.reflect.d.ts"
PROFILE_MANIFEST = ROOT / "crates/typokat-library/src/typescript-6.0.3/profile.toml"
PROFILE_IDENTITY = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d"
LIB_ES5_SHA256 = "bcd24271a113971ba9eb71ff8cb01bc6b0f872a85c23fdbe5d93065b375933cd"
LIB_REFLECT_SHA256 = "c0a4b8ac6ce74679c1da2b3795296f5896e31c38e888469a8e0f99dc3305de60"
DEMAND_ORDERS = (("Array", "ReadonlyArray", "Reflect"), ("Reflect", "ReadonlyArray", "Array"))
LIBRARY_SOURCE_ORDERS = ((0, 23), (23, 0))
CANDIDATE_RUNS = tuple(
    (library_source_order, demand_order, force_all)
    for force_all in (False, True)
    for demand_order in DEMAND_ORDERS
    for library_source_order in LIBRARY_SOURCE_ORDERS
)


@dataclass(frozen=True)
class Case:
    name: str
    files: tuple[str, ...]
    tsc_codes: tuple[str, ...]
    typokat_codes: tuple[str, ...]
    typokat_incomplete: tuple[str, ...] = ()
    reverse: bool = False


@dataclass(frozen=True)
class CandidateProject:
    project_id: str
    case: Case
    reverse: bool


CASES = (
    Case("array-clean", ("array-clean.ts",), (), ()),
    Case("array-cycle", ("array-cycle.ts",), ("TS2322",), ("TK2322",)),
    Case(
        "direct-array-augmentation",
        ("direct-array-augment.ts", "direct-array-consume.ts"),
        ("TS2322",),
        ("TK2322",),
        reverse=True,
    ),
    Case(
        "declare-global-array-augmentation",
        ("declare-global-array-augment.ts", "declare-global-array-consume.ts"),
        ("TS2322",),
        ("TK2322",),
        reverse=True,
    ),
    Case(
        "unique-global-type",
        ("unique-global-augment.ts", "unique-global-consume.ts"),
        ("TS2322",),
        ("TK2322",),
        reverse=True,
    ),
    Case("script-global-object", ("script-global-object.ts",), ("TS2322",), ("TK2339", "TK2339")),
    Case("reflect-namespace", ("reflect-namespace.ts",), ("TS2322",), ()),
    Case(
        "declare-global-value-tail",
        ("declare-global-value-augment.ts", "declare-global-value-consume.ts"),
        (),
        (),
        reverse=True,
    ),
    Case("isolation-a", ("isolation-a.ts",), ("TS2551",), ("TK2339",)),
    Case("isolation-b", ("isolation-b.ts",), ("TS2551",), ("TK2339",)),
)

CASE_BY_NAME = {case.name: case for case in CASES}

EXPECTED_TYPOKAT_RECORDS = {
    "array-clean": (),
    "array-cycle": ("array-cycle.ts:5:7 TK2322",),
    "direct-array-augmentation": ("direct-array-consume.ts:5:7 TK2322",),
    "declare-global-array-augmentation": ("declare-global-array-consume.ts:7:7 TK2322",),
    "unique-global-type": ("unique-global-consume.ts:5:7 TK2322",),
    "script-global-object": (
        "script-global-object.ts:2:40 TK2339",
        "script-global-object.ts:3:45 TK2339",
    ),
    "reflect-namespace": (),
    "declare-global-value-tail": (),
    "isolation-a": ("isolation-a.ts:7:16 TK2339",),
    "isolation-b": ("isolation-b.ts:7:16 TK2339",),
}

EXPECTED_USER_FRAGMENTS = {
    "direct-array-augmentation": {"Array": (("direct-array-augment.ts", 0),)},
    "declare-global-array-augmentation": {
        "Array": (("declare-global-array-augment.ts", 29),)
    },
    "unique-global-type": {
        "ProjectLocalUniqueGlobal": (("unique-global-augment.ts", 29),)
    },
    "isolation-a": {"Array": (("isolation-a.ts", 0),)},
    "isolation-b": {"Array": (("isolation-b.ts", 0),)},
}

EXPECTED_USER_ONLY_SLOTS = {"ProjectLocalUniqueGlobal": ("type",)}

EXPECTED_LIBRARY_GROUPS = {
    "PropertyKey": (
        ("type",),
        (((0, 3699), "declare type PropertyKey = string | number | symbol;"),),
    ),
    "PropertyDescriptor": (
        ("type",),
        (((0, 3753), "interface PropertyDescriptor {"),),
    ),
    "Function": (("type",), (((0, 9886), "interface Function {"),)),
    "ReadonlyArray": (("type",), (((0, 48000), "interface ReadonlyArray<T> {"),)),
    "ConcatArray": (("type",), (((0, 57473), "interface ConcatArray<T> {"),)),
    "Array": (
        ("type", "value"),
        (
            ((0, 57645), "interface Array<T> {"),
            ((0, 70878), "declare var Array: ArrayConstructor;"),
        ),
    ),
    "ArrayConstructor": (("type",), (((0, 70568), "interface ArrayConstructor {"),)),
    "TypedPropertyDescriptor": (
        ("type",),
        (((0, 70916), "interface TypedPropertyDescriptor<T> {"),),
    ),
    "ArrayLike": (("type",), (((0, 73638), "interface ArrayLike<T> {"),)),
    "Readonly": (("type",), (((0, 73963), "type Readonly<T> = {"),)),
    "ThisType": (("type",), (((0, 76011), "interface ThisType<T> {}"),)),
    "Reflect": (
        ("namespace", "value"),
        (((23, 814), "declare namespace Reflect {"),),
    ),
}

EXPECTED_EXCLUDED_FRAGMENTS = {
    "Array": ((17, 814), (20, 2671), (25, 3080), (26, 814), (40, 2113), (57, 814), (63, 814)),
    "ReadonlyArray": (
        (17, 13042),
        (20, 3691),
        (25, 3326),
        (26, 1171),
        (40, 1093),
        (57, 1068),
        (63, 4341),
    ),
    "ArrayConstructor": ((17, 3481), (20, 3093), (25, 10647)),
    "Function": ((17, 4312), (25, 4744)),
    "ConcatArray": (),
    "ArrayLike": (),
    "Reflect": (),
}

EXCLUDED_FRAGMENT_PREFIXES = {
    "Array": "interface Array<T> {",
    "ReadonlyArray": "interface ReadonlyArray<T> {",
    "ArrayConstructor": "interface ArrayConstructor {",
    "Function": "interface Function {",
}

EXPECTED_EVENT_ORDER = {
    0: (3699, 3753, 9886, 48000, 57473, 57645, 70568, 70878, 70916, 73638, 73963, 76011),
    23: (814,),
}

EXPECTED_REFLECT_MEMBER_ORDER = (
    1191,
    1358,
    1831,
    2016,
    2564,
    2982,
    3468,
    3950,
    4272,
    4535,
    4746,
    5097,
    5350,
    5824,
    6013,
    6380,
)


def run(command: list[str], *, stdin: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, input=stdin, text=True, capture_output=True, check=False)


def ordered_files(case: Case, reverse: bool) -> list[Path]:
    paths = [CASE_DIR / name for name in case.files]
    return list(reversed(paths)) if reverse else paths


def extract_codes(text: str, prefix: str) -> tuple[str, ...]:
    return tuple(re.findall(rf"\b{re.escape(prefix)}\d+\b", text))


def extract_incomplete(text: str) -> tuple[str, ...]:
    return tuple(re.findall(r"incomplete\[([^\]]+)\]", text))


def normalize_output(text: str, directory: Path) -> str:
    return text.replace(str(directory), "<project>").replace(str(ROOT), "<repo>")


def fragment_census_digest(
    library_sites: tuple[tuple[int, int], ...],
    user_sites: tuple[tuple[str, int], ...],
) -> str:
    census = {
        "library": [list(site) for site in library_sites],
        "user": [list(site) for site in user_sites],
    }
    return hashlib.sha256(
        json.dumps(census, separators=(",", ":"), sort_keys=True).encode()
    ).hexdigest()


def validate_manifest() -> None:
    import tomllib

    manifest = tomllib.loads((CASE_DIR / "locked-slice.toml").read_text())
    profile_manifest = tomllib.loads(PROFILE_MANIFEST.read_text())
    profile_files = {item["ordinal"]: item for item in profile_manifest["file"]}
    sources = {0: LIB_ES5.read_bytes(), 23: LIB_REFLECT.read_bytes()}
    expected_source_hashes = {0: LIB_ES5_SHA256, 23: LIB_REFLECT_SHA256}
    if manifest["profile_identity"] != PROFILE_IDENTITY:
        raise AssertionError("locked profile identity drifted")
    locked_source_hashes = {source["ordinal"]: source["sha256"] for source in manifest["sources"]}
    if locked_source_hashes != expected_source_hashes:
        raise AssertionError(f"locked manifest source identities drifted: {locked_source_hashes}")
    locked_source_names = {source["ordinal"]: source["name"] for source in manifest["sources"]}
    if locked_source_names != {0: "lib.es5.d.ts", 23: "lib.es2015.reflect.d.ts"}:
        raise AssertionError(f"locked manifest source names drifted: {locked_source_names}")
    for ordinal, source in sources.items():
        if hashlib.sha256(source).hexdigest() != expected_source_hashes[ordinal]:
            raise AssertionError(f"vendored source ordinal {ordinal} identity drifted")
    locked_groups = {
        group["name"]: (
            tuple(group["slots"]),
            tuple(tuple(site) for site in group["statement_sites"]),
        )
        for group in manifest["groups"]
    }
    expected_groups = {
        name: (slots, tuple(site for site, _ in statements))
        for name, (slots, statements) in EXPECTED_LIBRARY_GROUPS.items()
    }
    if locked_groups != expected_groups:
        raise AssertionError(f"locked group records drifted: {locked_groups}")
    for name, (_, statements) in EXPECTED_LIBRARY_GROUPS.items():
        for (ordinal, start), prefix in statements:
            source = sources[ordinal]
            actual = source[start : start + len(prefix)].decode("utf-8")
            if actual != prefix:
                raise AssertionError(
                    f"source site {name} {ordinal}:{start} expected {prefix!r}, got {actual!r}"
                )
    if manifest["required_cycle"] != ["Array", "ArrayConstructor", "ConcatArray"]:
        raise AssertionError("Array publication SCC witness drifted")
    if manifest["required_demands"] != ["Array", "ReadonlyArray", "Reflect"]:
        raise AssertionError("required demand roots drifted")
    if manifest["required_special_owner"] != "global-object":
        raise AssertionError("global-object owner witness drifted")
    if manifest["global_keys"] != ["Array", "Reflect"]:
        raise AssertionError("global-object key inventory drifted")
    edge_set = {(edge["from"], edge["to"]) for edge in manifest["edges"]}
    required_edges = {
        ("Function", "Array"),
        ("Function", "Function"),
        ("Array", "ConcatArray"),
        ("Array", "Array"),
        ("Array", "ArrayConstructor"),
        ("ConcatArray", "Array"),
        ("ReadonlyArray", "Array"),
        ("ReadonlyArray", "ConcatArray"),
        ("ReadonlyArray", "ReadonlyArray"),
        ("ArrayConstructor", "Array"),
        ("Reflect", "Array"),
        ("Reflect", "ArrayLike"),
        ("Reflect", "Function"),
        ("Reflect", "PropertyDescriptor"),
        ("Reflect", "PropertyKey"),
        ("Reflect", "Readonly"),
        ("Reflect", "ReadonlyArray"),
        ("Reflect", "ThisType"),
        ("Reflect", "TypedPropertyDescriptor"),
    }
    if edge_set != required_edges:
        raise AssertionError(f"locked dependency edges drifted: {sorted(edge_set ^ required_edges)}")
    owner_sccs = {tuple(scc["members"]) for scc in manifest["owner_sccs"]}
    if owner_sccs != {("Array", "ConcatArray")}:
        raise AssertionError(f"owner SCC inventory drifted: {owner_sccs}")
    publication_sccs = {tuple(scc["members"]) for scc in manifest["publication_sccs"]}
    if publication_sccs != {("Array", "ArrayConstructor", "ConcatArray")}:
        raise AssertionError(f"publication SCC inventory drifted: {publication_sccs}")
    excluded = {
        group["name"]: tuple(tuple(site) for site in group["sites"])
        for group in manifest["excluded_fragments"]
    }
    if excluded != EXPECTED_EXCLUDED_FRAGMENTS:
        raise AssertionError(f"excluded fragment inventory drifted: {excluded}")
    for name, sites in EXPECTED_EXCLUDED_FRAGMENTS.items():
        for ordinal, start in sites:
            profile_file = profile_files[ordinal]
            source_path = LIB_ES5.parent / profile_file["name"]
            source = source_path.read_bytes()
            if hashlib.sha256(source).hexdigest() != profile_file["sha256"]:
                raise AssertionError(f"excluded source ordinal {ordinal} identity drifted")
            prefix = EXCLUDED_FRAGMENT_PREFIXES[name]
            actual = source[start : start + len(prefix)].decode("utf-8")
            if actual != prefix:
                raise AssertionError(
                    f"excluded site {name} {ordinal}:{start} expected {prefix!r}, got {actual!r}"
                )
    event_order = {
        item["source_ordinal"]: tuple(item["statement_starts"])
        for item in manifest["event_order"]
    }
    if event_order != EXPECTED_EVENT_ORDER:
        raise AssertionError(f"source event order drifted: {event_order}")
    reflect_events = next(
        item for item in manifest["event_order"] if item["source_ordinal"] == 23
    )
    if tuple(reflect_events["member_starts"]) != EXPECTED_REFLECT_MEMBER_ORDER:
        raise AssertionError("Reflect member event order drifted")
    user_fragment_prefixes = {
        "Array": "interface Array<T> {",
        "ProjectLocalUniqueGlobal": "interface ProjectLocalUniqueGlobal {",
    }
    for groups in EXPECTED_USER_FRAGMENTS.values():
        for name, sites in groups.items():
            prefix = user_fragment_prefixes[name]
            for file_name, start in sites:
                source = (CASE_DIR / file_name).read_bytes()
                actual = source[start : start + len(prefix)].decode("utf-8")
                if actual != prefix:
                    raise AssertionError(
                        f"user fragment {file_name}:{start} expected {prefix!r}, got {actual!r}"
                    )


def run_tsc_case(case: Case, reverse: bool) -> dict[str, Any]:
    command = [
        "tsc",
        "--pretty",
        "false",
        "--strict",
        "--noEmit",
        "--module",
        "ESNext",
        "--moduleResolution",
        "Bundler",
        *map(str, ordered_files(case, reverse)),
    ]
    result = run(command)
    output = result.stdout + result.stderr
    codes = extract_codes(output, "TS")
    if codes != case.tsc_codes:
        raise AssertionError(f"{case.name} reverse={reverse}: tsc codes {codes} != {case.tsc_codes}")
    expected_exit = 0 if not case.tsc_codes else 2
    if result.returncode != expected_exit:
        raise AssertionError(f"{case.name} reverse={reverse}: tsc exit {result.returncode}")
    return {
        "exit": result.returncode,
        "codes": list(codes),
        "normalized_output": normalize_output(output, CASE_DIR),
    }


def run_typokat_case(case: Case, reverse: bool, typokat: Path) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix=f"typokat-{case.name}-") as raw_directory:
        directory = Path(raw_directory)
        file_names: list[str] = []
        for source_path in ordered_files(case, reverse):
            target = directory / source_path.name
            target.write_text(source_path.read_text())
            file_names.append(source_path.name)
        config = {
            "compilerOptions": {
                "strict": True,
                "noEmit": True,
                "module": "ESNext",
                "moduleResolution": "Bundler",
            },
            "files": file_names,
        }
        config_path = directory / "tsconfig.json"
        config_path.write_text(json.dumps(config, separators=(",", ":")))
        result = run(
            [
                str(typokat),
                "check",
                "--format",
                "compact",
                "--project-summary",
                "json",
                str(config_path),
            ]
        )
        output = result.stdout + result.stderr
        summary = json.loads(result.stdout)
        diagnostic_records = tuple(summary["diagnostics"])
        expected_records = EXPECTED_TYPOKAT_RECORDS[case.name]
        if diagnostic_records != expected_records:
            raise AssertionError(
                f"{case.name} reverse={reverse}: diagnostic records "
                f"{diagnostic_records} != {expected_records}"
            )
        codes = tuple(
            code
            for diagnostic in summary["diagnostics"]
            for code in extract_codes(diagnostic, "TK")
        )
        incomplete = tuple(item.split()[-1] for item in summary["incomplete"])
        if codes != case.typokat_codes:
            raise AssertionError(
                f"{case.name} reverse={reverse}: typokat codes {codes} != {case.typokat_codes}\n"
                f"{normalize_output(output, directory)}"
            )
        if incomplete != case.typokat_incomplete:
            raise AssertionError(
                f"{case.name} reverse={reverse}: incomplete {incomplete} != {case.typokat_incomplete}\n"
                f"{normalize_output(output, directory)}"
            )
        expected_exit = 3 if incomplete else (1 if codes else 0)
        if result.returncode != expected_exit:
            raise AssertionError(
                f"{case.name} reverse={reverse}: typokat exit {result.returncode} != {expected_exit}\n"
                f"{normalize_output(output, directory)}"
            )
        return {
            "exit": result.returncode,
            "codes": list(codes),
            "diagnostics": list(diagnostic_records),
            "incomplete": list(incomplete),
            "normalized_output": normalize_output(output, directory),
        }


def oracle(args: argparse.Namespace) -> int:
    version = run(["tsc", "--version"])
    if version.returncode != 0 or version.stdout.strip() != "Version 6.0.3":
        raise AssertionError(f"expected tsc 6.0.3, got {version.stdout.strip()!r}")
    typokat = Path(args.typokat).resolve()
    if not typokat.is_file():
        raise AssertionError(f"missing typokat binary: {typokat}")
    validate_manifest()
    receipts: dict[str, Any] = {}
    for case in CASES:
        variants = (False, True) if case.reverse else (False,)
        for reverse in variants:
            key = f"{case.name}:{'reverse' if reverse else 'forward'}"
            receipts[key] = {
                "tsc": run_tsc_case(case, reverse),
                "typokat": run_typokat_case(case, reverse, typokat),
            }
    digest = hashlib.sha256(json.dumps(receipts, sort_keys=True).encode()).hexdigest()
    commit = run(["git", "rev-parse", "HEAD"]).stdout.strip()
    tree = run(["git", "rev-parse", "HEAD^{tree}"]).stdout.strip()
    binary_sha256 = hashlib.sha256(typokat.read_bytes()).hexdigest()
    print(
        json.dumps(
            {
                "schema": 1,
                "cases": len(receipts),
                "digest": digest,
                "commit": commit,
                "tree": tree,
                "typokat_sha256": binary_sha256,
            },
            sort_keys=True,
        )
    )
    return 0


def candidate(args: argparse.Namespace) -> int:
    if not args.runner:
        raise AssertionError("candidate mode requires --runner; RED contract remains unsatisfied")
    runner = Path(args.runner).resolve()
    if not runner.is_file():
        raise AssertionError(f"missing candidate runner: {runner}")
    validate_manifest()
    typokat = Path(args.typokat).resolve()
    if not typokat.is_file():
        raise AssertionError(f"missing typokat oracle binary: {typokat}")
    oracle_receipts = {
        (case.name, reverse): run_typokat_case(case, reverse, typokat)
        for case in CASES
        for reverse in ((False, True) if case.reverse else (False,))
    }
    import tomllib

    manifest = tomllib.loads((CASE_DIR / "locked-slice.toml").read_text())
    expected_group_sites = {
        group["name"]: tuple(tuple(site) for site in group["statement_sites"])
        for group in manifest["groups"]
    }
    expected_group_slots = {
        group["name"]: tuple(group["slots"])
        for group in manifest["groups"]
    }
    batches: list[tuple[str, tuple[CandidateProject, ...]]] = []
    for case in CASES:
        if case.name.startswith("isolation-"):
            continue
        variants = (False, True) if case.reverse else (False,)
        for reverse in variants:
            order = "reverse" if reverse else "forward"
            project = CandidateProject(f"{case.name}:{order}", case, reverse)
            batches.append((project.project_id, (project,)))
    isolation_a = CandidateProject("isolation-a", CASE_BY_NAME["isolation-a"], False)
    isolation_b = CandidateProject("isolation-b", CASE_BY_NAME["isolation-b"], False)
    batches.extend(
        (
            ("isolation:a-then-b", (isolation_a, isolation_b)),
            ("isolation:b-then-a", (isolation_b, isolation_a)),
        )
    )

    index_digests: set[str] = set()
    universe_ids: set[str] = set()
    symbolic_group_identities: dict[str, str] | None = None
    project_symbolic_identities: dict[tuple[str, str], str] = {}
    for batch_id, projects in batches:
        for library_source_order, demand_order, force_all in CANDIDATE_RUNS:
            request = {
                "schema": 1,
                "profile_identity": PROFILE_IDENTITY,
                "batch_id": batch_id,
                "projects": [
                    {
                        "project_id": project.project_id,
                        "sources": [
                            {"name": path.name, "source": path.read_text()}
                            for path in ordered_files(project.case, project.reverse)
                        ],
                    }
                    for project in projects
                ],
                "library_source_order": list(library_source_order),
                "demand_order": list(demand_order),
                "force_all": force_all,
            }
            result = run([str(runner)], stdin=json.dumps(request))
            if result.returncode != 0:
                raise AssertionError(f"candidate runner failed: {result.stderr}")
            response = json.loads(result.stdout)
            if response.get("schema") != 1 or response.get("batch_id") != batch_id:
                raise AssertionError(f"invalid candidate response identity for {batch_id}")
            if response.get("library_source_order") != list(library_source_order):
                raise AssertionError(f"library source-order receipt drifted for {batch_id}")
            if response.get("demand_order") != list(demand_order):
                raise AssertionError(f"candidate demand-order receipt drifted for {batch_id}")
            if response.get("force_all") is not force_all:
                raise AssertionError(f"candidate force-all receipt drifted for {batch_id}")
            ownership = response["shared_ownership"]
            forbidden_shared = (
                "semantic_ids",
                "runtime_products",
                "retained_ast_nodes",
                "replay_owner_rows",
            )
            nonzero_shared = {
                name: ownership[name] for name in forbidden_shared if ownership[name] != 0
            }
            if nonzero_shared:
                raise AssertionError(f"forbidden shared ownership: {nonzero_shared}")
            shared_index_owner_id = response["shared_index_owner_id"]
            if not isinstance(shared_index_owner_id, str) or not shared_index_owner_id:
                raise AssertionError(f"missing shared index owner for {batch_id}")
            index_digests.add(response["symbolic_index_digest"])
            identities = response["symbolic_group_identities"]
            if set(identities) != set(expected_group_sites):
                raise AssertionError(f"symbolic group identity inventory drifted for {batch_id}")
            if symbolic_group_identities is None:
                symbolic_group_identities = identities
            elif identities != symbolic_group_identities:
                raise AssertionError("symbolic group identities depend on input or demand order")
            project_responses = response["projects"]
            if [project["project_id"] for project in project_responses] != [
                project.project_id for project in projects
            ]:
                raise AssertionError(f"project creation order drifted for {batch_id}")
            local_array_ids: set[str] = set()
            batch_owner_ids = {shared_index_owner_id}
            for expected, project_response in zip(projects, project_responses, strict=True):
                universe_id = project_response["project_universe_id"]
                if universe_id in universe_ids:
                    raise AssertionError(f"project universe identity reused: {universe_id}")
                universe_ids.add(universe_id)
                owner_ids = {
                    project_response["binder_owner_id"],
                    project_response["store_owner_id"],
                    project_response["global_object_owner_id"],
                    universe_id,
                }
                if any(not isinstance(owner_id, str) or not owner_id for owner_id in owner_ids):
                    raise AssertionError(f"missing project owner identity for {expected.project_id}")
                if len(owner_ids) != 4 or batch_owner_ids.intersection(owner_ids):
                    raise AssertionError(f"project ownership is not isolated for {expected.project_id}")
                batch_owner_ids.update(owner_ids)
                local_array_id = project_response["local_array_identity"]
                if local_array_id in local_array_ids:
                    raise AssertionError(f"local Array identity reused inside {batch_id}")
                local_array_ids.add(local_array_id)
                counters = project_response["route_counters"]
                forbidden_routes = (
                    "collision_preflight_calls",
                    "sparse_epoch_forks",
                    "complete_source_fallbacks",
                )
                nonzero_routes = {
                    name: counters[name] for name in forbidden_routes if counters[name] != 0
                }
                if nonzero_routes:
                    raise AssertionError(f"forbidden candidate routes: {nonzero_routes}")
                case = expected.case
                expected_user_fragments = EXPECTED_USER_FRAGMENTS.get(case.name, {})
                expected_project_groups = set(expected_group_sites) | (
                    set(expected_user_fragments) - set(expected_group_sites)
                )
                groups = project_response["groups"]
                if set(groups) != expected_project_groups:
                    raise AssertionError(f"candidate group inventory drifted for {expected.project_id}")
                local_group_ids: set[str] = set()
                for name, group in groups.items():
                    if group["state"] != "Ready":
                        raise AssertionError(f"demanded group {name} is not Ready")
                    library_sites = expected_group_sites.get(name, ())
                    actual_library_sites = tuple(
                        tuple(site) for site in group["library_statement_sites"]
                    )
                    if actual_library_sites != library_sites:
                        raise AssertionError(f"library source sites drifted for group {name}")
                    expected_user_sites = expected_user_fragments.get(name, ())
                    actual_user_sites = tuple(
                        (site[0], site[1]) for site in group["user_statement_sites"]
                    )
                    if actual_user_sites != expected_user_sites:
                        raise AssertionError(f"user source sites drifted for group {name}")
                    expected_slots = (
                        expected_group_slots[name]
                        if name in expected_group_slots
                        else EXPECTED_USER_ONLY_SLOTS[name]
                    )
                    if tuple(group["slots"]) != expected_slots:
                        raise AssertionError(f"group slots drifted for {name}")
                    if name in identities:
                        if group["symbolic_identity"] != identities[name]:
                            raise AssertionError(f"symbolic group identity drifted for {name}")
                    else:
                        project_symbolic_identity = group["project_symbolic_identity"]
                        if not isinstance(project_symbolic_identity, str) or not project_symbolic_identity:
                            raise AssertionError(f"missing project symbolic identity for {name}")
                        identity_key = (case.name, name)
                        previous_identity = project_symbolic_identities.setdefault(
                            identity_key, project_symbolic_identity
                        )
                        if previous_identity != project_symbolic_identity:
                            raise AssertionError(
                                f"project symbolic identity is order-dependent for {name}"
                            )
                    expected_census_digest = fragment_census_digest(
                        library_sites, expected_user_sites
                    )
                    if group["fragment_census_digest"] != expected_census_digest:
                        raise AssertionError(f"final fragment census drifted for {name}")
                    local_identity = group["local_identity"]
                    if not isinstance(local_identity, str) or not local_identity:
                        raise AssertionError(f"missing local group identity for {name}")
                    if local_identity in local_group_ids:
                        raise AssertionError(f"local group identity reused for {name}")
                    local_group_ids.add(local_identity)
                if groups["Array"]["local_identity"] != local_array_id:
                    raise AssertionError(f"Array identity receipt drifted for {expected.project_id}")
                if project_response["selected_global_keys"] != ["Array", "Reflect"]:
                    raise AssertionError(f"selected global-object keys drifted for {expected.project_id}")
                counts = project_response["counts"]
                if counts["published_group_count"] != len(expected_project_groups):
                    raise AssertionError(f"published group count drifted for {expected.project_id}")
                expected_minimum_demanded = (
                    len(expected_group_sites) if force_all else len(DEMAND_ORDERS[0])
                )
                if counts["demanded_group_count"] < expected_minimum_demanded:
                    raise AssertionError(f"demand closure was not exercised for {expected.project_id}")
                if counts["semantic_row_count"] <= 0 or counts["local_cache_entry_count"] <= 0:
                    raise AssertionError(f"missing semantic/cache rows for {expected.project_id}")
                trace = project_response["publication_trace"]
                transitions: dict[str, list[tuple[dict[str, Any], int]]] = {
                    name: [] for name in expected_project_groups
                }
                for position, event in enumerate(trace):
                    if event["group"] not in transitions:
                        raise AssertionError(f"unexpected publication group: {event['group']}")
                    transitions[event["group"]].append((event, position))
                expected_transitions = [
                    ("Unplanned", "Reserved"),
                    ("Reserved", "Resolving"),
                    ("Resolving", "Ready"),
                ]
                for name, events in transitions.items():
                    actual_transitions = [
                        (event["from"], event["to"]) for event, _ in events
                    ]
                    if actual_transitions != expected_transitions:
                        raise AssertionError(f"publication transitions drifted for {name}: {events}")
                    reservation_event = events[0][0]
                    if (
                        reservation_event["fragment_census_digest"]
                        != groups[name]["fragment_census_digest"]
                    ):
                        raise AssertionError(
                            f"group {name} was reserved before its final fragment census"
                        )
                publication_scc = ("Array", "ArrayConstructor", "ConcatArray")
                reserve_positions = [transitions[name][0][1] for name in publication_scc]
                resolve_positions = [transitions[name][1][1] for name in publication_scc]
                if max(reserve_positions) >= min(resolve_positions):
                    raise AssertionError("Array publication SCC was not reserved atomically")
                if project_response["observations_while_resolving"] != 0:
                    raise AssertionError(f"resolving group escaped for {expected.project_id}")
                reports = project_response["reports"]
                expected_sources = [path.name for path in ordered_files(expected.case, expected.reverse)]
                if [report["source"] for report in reports] != expected_sources:
                    raise AssertionError(f"per-source report order drifted for {expected.project_id}")
                codes = tuple(
                    code for report in reports for code in report["diagnostic_codes"]
                )
                diagnostic_records = tuple(
                    record for report in reports for record in report["diagnostic_records"]
                )
                incomplete = tuple(
                    item for report in reports for item in report["incomplete_ids"]
                )
                if (
                    codes != case.typokat_codes
                    or diagnostic_records != EXPECTED_TYPOKAT_RECORDS[case.name]
                    or incomplete != case.typokat_incomplete
                ):
                    raise AssertionError(
                        f"candidate output drift for {expected.project_id}: "
                        f"{codes}, {diagnostic_records}, {incomplete}"
                    )
                oracle_receipt = oracle_receipts[(case.name, expected.reverse)]
                if project_response["exit"] != oracle_receipt["exit"]:
                    raise AssertionError(f"candidate exit drift for {expected.project_id}")
                if project_response["normalized_output"] != oracle_receipt["normalized_output"]:
                    raise AssertionError(f"candidate rendered output drift for {expected.project_id}")
                if case.name.startswith("isolation-") and project_response["opposite_member_visible"]:
                    raise AssertionError(f"opposite-project Array member leaked into {case.name}")
    if len(index_digests) != 1:
        raise AssertionError(f"symbolic index identity drifted: {sorted(index_digests)}")
    print(json.dumps({"schema": 1, "verdict": "candidate-contract-pass"}, sort_keys=True))
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="mode", required=True)
    oracle_parser = subparsers.add_parser("oracle")
    oracle_parser.add_argument("--typokat", default=str(ROOT / "target/release/typokat"))
    candidate_parser = subparsers.add_parser("candidate")
    candidate_parser.add_argument("--runner")
    candidate_parser.add_argument("--typokat", default=str(ROOT / "target/release/typokat"))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        return oracle(args) if args.mode == "oracle" else candidate(args)
    except (AssertionError, KeyError, json.JSONDecodeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

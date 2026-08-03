#!/usr/bin/env python3
"""Unit witnesses for the official-suite harness hardening (sprint WU5).

Run with the stdlib test runner (no third-party deps):

    cd tooling/official-suite
    python3 -m unittest test_tsofficial -v

Each test is a *witness* for one verified finding: it pins the hardened behavior
and, where noted, would have passed under the pre-fix harness (the bug it closes).
"""

import contextlib
import io
import json
import os
import stat
import tempfile
import textwrap
import time
import unittest
from unittest import mock

import tsofficial as ts


# --- helpers -----------------------------------------------------------------

def rec(rel, *, bucket=None, matched=0, fn=0, fp=0, expected=0,
        matched_detail=None, fp_detail=None, fn_detail=None,
        incomplete_detail=None, strict=True):
    """Build one cmd_run-shaped result record."""
    return {
        "rel": rel, "bucket": bucket, "strict": strict,
        "matched": matched, "fn": fn, "fp": fp, "expected": expected,
        "matched_detail": matched_detail or [],
        "fp_detail": fp_detail or [],
        "fn_detail": fn_detail or [],
        "incomplete_detail": incomplete_detail or [],
    }


@contextlib.contextmanager
def temp_scoreboard(base_results):
    """Point ts.SCOREBOARD at a fresh temp file seeded from base_results."""
    fd, path = tempfile.mkstemp(suffix=".scoreboard.txt")
    os.close(fd)
    saved = ts.SCOREBOARD
    ts.SCOREBOARD = path
    try:
        ts.write_scoreboard(base_results)
        yield path
    finally:
        ts.SCOREBOARD = saved
        os.unlink(path)


def compare(cur_results):
    """compare_scoreboard with stdout muted; returns its bool (True = no regress)."""
    with contextlib.redirect_stdout(io.StringIO()):
        return ts.compare_scoreboard(cur_results)


def fake_binary(script_body):
    """Write an executable shell script fake `typokat`. It is invoked as
    `<bin> check <tmp.ts>`, so the checked file path is `$2`."""
    fd, path = tempfile.mkstemp(suffix=".sh")
    with os.fdopen(fd, "w") as f:
        f.write("#!/bin/sh\n" + script_body)
    os.chmod(path, os.stat(path).st_mode | stat.S_IEXEC | stat.S_IXGRP)
    return path


def fake_batch_binary(loop_body, before_loop=""):
    """Write a fake JSONL `typokat official-batch` worker."""
    fd, path = tempfile.mkstemp(suffix=".py")
    script = f"""#!/usr/bin/env python3
import json
import os
import signal
import sys
import time

if sys.argv[1:] != ["official-batch"]:
    sys.exit(64)
{textwrap.dedent(before_loop)}
provider_route = "production-default-library"
provider_profile_sha256 = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d"
for raw in sys.stdin:
    request = json.loads(raw)
{textwrap.indent(textwrap.dedent(loop_body), "    ")}
"""
    with os.fdopen(fd, "w") as f:
        f.write(script)
    os.chmod(path, os.stat(path).st_mode | stat.S_IEXEC | stat.S_IXGRP)
    return path


def fake_clean_batch_binary():
    return fake_batch_binary("""
print(json.dumps({
    "schema": 1,
    "case_id": request["case_id"],
    "worker_pid": os.getpid(),
    "provider_route": provider_route,
    "profile_sha256": provider_profile_sha256,
    "exit_code": 0,
    "stdout": "",
    "stderr": "",
}), flush=True)
""")


# Emits a well-formed diagnostic header + a location line referencing the tmp
# file run_typokat created (so it parses as exactly one (line, code) diagnostic).
DIAG_LINES = (
    'echo "error[TK2322]: Type \'string\' is not assignable to type \'number\'"\n'
    'echo "$(basename "$2"):1:5"\n'
)

# Emits a well-formed incomplete-surface record (rich form: header + location line).
INCOMPLETE_LINES = (
    'echo "incomplete[expr-infer/template-literal/interpolation]: skipped by expr walker"\n'
    'echo "  --> $(basename "$2"):1:5"\n'
)


# --- Official test-case unit parsing ----------------------------------------

class UnitParserTests(unittest.TestCase):
    def test_leading_bom_does_not_hide_directives_or_shift_units(self):
        source = textwrap.dedent("""\
            // @target: es5
            // @filename: first.ts
            const first: number = "bad";
            // @filename: second.ts
            const second: string = 1;
        """)
        expected = (
            {"target": "es5"},
            [
                ("first.ts", 'const first: number = "bad";'),
                ("second.ts", 'const second: string = 1;\n'),
            ],
        )

        self.assertEqual(ts.parse_units(source), expected)
        self.assertEqual(ts.parse_units("\ufeff" + source), expected)


# --- Finding 1: identity-based ratchet ---------------------------------------

class IdentityRatchetTests(unittest.TestCase):
    def test_same_count_matched_swap_is_a_regression(self):
        """A swap of WHICH diagnostic matches — same matched/fp counts — is a
        regression. PRE-FIX: compare_scoreboard compared integer counts only
        (matched 1 == 1, fp 0 == 0) and reported NO regression, hiding the drop."""
        base = [rec("a.ts", matched=1, fp=0, expected=1,
                    matched_detail=[(12, 2322)])]
        cur = [rec("a.ts", matched=1, fp=0, expected=1,
                   matched_detail=[(13, 2322)])]  # line 12 -> 13, same count
        with temp_scoreboard(base):
            self.assertFalse(compare(cur),
                             "same-count identity swap must be a regression")

    def test_new_false_positive_identity_is_a_regression(self):
        base = [rec("a.ts", matched=1, fp=1, expected=1,
                    matched_detail=[(1, 2322)], fp_detail=[(2, 2339)])]
        cur = [rec("a.ts", matched=1, fp=1, expected=1,
                   matched_detail=[(1, 2322)], fp_detail=[(5, 2339)])]  # fp moved
        with temp_scoreboard(base):
            self.assertFalse(compare(cur))

    def test_dropped_duplicate_matched_is_a_regression(self):
        """Two matched TK2322 on one line -> one is a regression even though the
        identity SET is unchanged (multiplicity is preserved)."""
        base = [rec("a.ts", matched=2, expected=2,
                    matched_detail=[(3, 2322), (3, 2322)])]
        cur = [rec("a.ts", matched=1, fn=1, expected=2,
                   matched_detail=[(3, 2322)])]
        with temp_scoreboard(base):
            self.assertFalse(compare(cur))

    def test_identical_run_has_no_regression(self):
        base = [rec("a.ts", matched=1, fp=1, expected=1,
                    matched_detail=[(1, 2322)], fp_detail=[(2, 2339)])]
        with temp_scoreboard(base):
            self.assertTrue(compare(list(base)))

    def test_gained_match_is_progress_not_regression(self):
        base = [rec("a.ts", matched=1, fn=1, expected=2,
                    matched_detail=[(1, 2322)])]
        cur = [rec("a.ts", matched=2, fn=0, expected=2,
                   matched_detail=[(1, 2322), (2, 2345)])]
        with temp_scoreboard(base):
            self.assertTrue(compare(cur))


# --- Finding 2: completeness enforced in both directions ---------------------

class CompletenessTests(unittest.TestCase):
    def test_scoreboard_entry_missing_from_corpus_is_a_regression(self):
        """A file in the scoreboard but absent from the checked corpus fails.
        PRE-FIX: `missing` was only printed as a note; return len(regress)==0
        ignored it, so an incomplete corpus passed --check."""
        base = [rec("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)]),
                rec("b.ts", matched=1, expected=1, matched_detail=[(1, 2322)])]
        cur = [rec("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)])]
        with temp_scoreboard(base):
            self.assertFalse(compare(cur))

    def test_corpus_file_missing_from_scoreboard_is_a_regression(self):
        """A corpus file absent from the scoreboard fails.
        PRE-FIX: `b is None: continue` treated a new corpus file as 'not a
        regression', so an unrecorded (and possibly wrong) file passed silently."""
        base = [rec("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)])]
        cur = [rec("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)]),
               rec("b.ts", matched=0, fp=1, expected=0, fp_detail=[(9, 2339)])]
        with temp_scoreboard(base):
            self.assertFalse(compare(cur))


# --- Fetch integrity ---------------------------------------------------------

class FetchIntegrityTests(unittest.TestCase):
    @contextlib.contextmanager
    def _isolated_paths(self, default_dirs=("conformance/x",), authoritative=True):
        """Run corpus/report tests without touching the ignored real artifacts."""
        with tempfile.TemporaryDirectory() as root:
            saved = (ts.CORPUS, ts.REPORT, ts.SCOREBOARD, ts.DEFAULT_DIRS,
                     ts.GIT_CACHE, ts.GIT_CACHE_MARKER, ts.TS_GIT_URL)
            ts.CORPUS = os.path.join(root, "corpus")
            ts.REPORT = os.path.join(root, "report")
            ts.SCOREBOARD = os.path.join(root, "scoreboard.txt")
            ts.DEFAULT_DIRS = list(default_dirs)
            ts.GIT_CACHE = os.path.join(root, ".tools", "typescript.git")
            ts.GIT_CACHE_MARKER = os.path.join(ts.GIT_CACHE, "typokat-cache-format")
            ts.TS_GIT_URL = "https://example.invalid/TypeScript.git"
            previous_inventory = getattr(self, "_trusted_manifest_inventory", None)
            self._trusted_manifest_inventory = None
            real_publish = ts.publish_corpus

            def publish_and_record(stage):
                with open(os.path.join(stage, "manifest.json")) as f:
                    self._trusted_manifest_inventory = json.load(f)["baseline_inventory"]
                return real_publish(stage)

            def pinned_inventory(_revision, stems):
                if self._trusted_manifest_inventory is None:
                    raise ts.FetchFailure("synthetic pinned baseline inventory was not prepared")
                return {stem: self._trusted_manifest_inventory[stem] for stem in stems}
            try:
                if authoritative:
                    with mock.patch.object(ts, "publish_corpus", side_effect=publish_and_record), \
                         mock.patch.object(
                             ts, "authoritative_baseline_inventory", side_effect=pinned_inventory
                         ):
                        yield root
                else:
                    yield root
            finally:
                self._trusted_manifest_inventory = previous_inventory
                (ts.CORPUS, ts.REPORT, ts.SCOREBOARD, ts.DEFAULT_DIRS,
                 ts.GIT_CACHE, ts.GIT_CACHE_MARKER, ts.TS_GIT_URL) = saved

    @staticmethod
    def _args(*, dirs=None, limit=None, check=False, save=False, rebaseline=False,
              binary=None):
        return type("Args", (), {
            "dir": dirs, "limit": limit, "check": check, "save": save,
            "rebaseline": rebaseline,
            "bin": binary or "/missing/binary",
        })()

    def _write_full_manifest(self, paths):
        os.makedirs(ts.CORPUS, exist_ok=True)
        for rel in paths:
            dst = os.path.join(ts.CORPUS, rel)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            with open(dst, "w") as f:
                f.write("const value = 1;\n")
        with open(os.path.join(ts.CORPUS, "manifest.json"), "w") as f:
            manifest = {
                "format": 4, "sha": ts.PINNED_SHA, "repo": ts.REPO,
                "dirs": ts.DEFAULT_DIRS, "limit": None,
                "mode": "full-default", "partial": False,
                "transport": {"kind": "git-cache-full-blob", "url": ts.TS_GIT_URL,
                              "revision": ts.PINNED_SHA, "cache_format": ts.GIT_CACHE_FORMAT},
                "baseline_inventory": {os.path.basename(rel)[:-3]: [] for rel in paths},
                "tests": [{"path": rel, "source": f"tests/cases/{rel}",
                           "baseline_mode": "single",
                           "baseline_sources": [{
                               "options": {},
                               "source": "tests/baselines/reference/"
                               + os.path.basename(rel)[:-3] + ".errors.txt",
                               "baseline": False,
                           }],
                           "baseline": False} for rel in paths],
            }
            json.dump(manifest, f)
        self._trusted_manifest_inventory = manifest["baseline_inventory"]

    def _scoreboard_bytes(self):
        with open(ts.SCOREBOARD, "rb") as f:
            return f.read()

    @staticmethod
    def _tree_entry(mode, typ, oid, path):
        return f"{mode} {typ} {oid}\t{path}\0".encode()

    def _raw_git(self, *args, input_data=None):
        proc = ts.subprocess.run(
            ["git", "-C", ts.GIT_CACHE, *args], input=input_data,
            capture_output=True,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr.decode("utf-8", "replace"))
        return proc.stdout.strip()

    def _write_compatible_bare_cache(self):
        os.makedirs(ts.GIT_CACHE, exist_ok=True)
        self._raw_git("init", "--bare")
        self._raw_git("remote", "add", "origin", ts.TS_GIT_URL)
        with open(ts.GIT_CACHE_MARKER, "w") as f:
            f.write(ts.GIT_CACHE_FORMAT + "\n")

    def _write_commit_object(self, tree, message):
        body = (
            b"tree " + tree + b"\n"
            b"author Test <test@example.invalid> 0 +0000\n"
            b"committer Test <test@example.invalid> 0 +0000\n\n"
            + message + b"\n"
        )
        return self._raw_git("hash-object", "-t", "commit", "-w", "--stdin",
                             input_data=body)

    def test_default_dirs_use_keyof_for_indexed_access_cases(self):
        """The pinned tree has indexed-access cases in `keyof`, not a separate
        `indexedAccessTypes` directory. Keep the curated input set resolvable."""
        self.assertIn("conformance/types/keyof", ts.DEFAULT_DIRS)
        self.assertNotIn("conformance/types/indexedAccessTypes", ts.DEFAULT_DIRS)

    def test_git_cache_initializes_and_fetches_only_the_pinned_sha(self):
        with self._isolated_paths() as root:
            calls = []

            def git(cache, *args, **_kwargs):
                calls.append((cache, args, _kwargs))
                if args[:2] == ("rev-parse", "--verify"):
                    return (ts.PINNED_SHA + "\n").encode()
                return b"true\n" if args == ("rev-parse", "--is-bare-repository") else b""

            with mock.patch.object(ts, "run_git", side_effect=git):
                revision = ts.prepare_git_cache()

            self.assertEqual(revision, ts.PINNED_SHA)
            self.assertIn((ts.GIT_CACHE, ("init", "--bare"), {}), calls)
            self.assertIn((ts.GIT_CACHE, ("remote", "add", "origin", ts.TS_GIT_URL), {}), calls)
            fetch = ("fetch", "--depth=1", "--no-tags", "origin",
                     f"{ts.PINNED_SHA}:{ts.PINNED_REF}")
            self.assertIn((ts.GIT_CACHE, fetch, {"timeout": 300}), calls)
            self.assertNotIn("--filter", fetch)
            self.assertIn((ts.GIT_CACHE, ("rev-parse", "--verify",
                                           f"{ts.PINNED_REF}^{{commit}}"), {}), calls)

    def test_git_cache_reuses_existing_cache_but_reverifies_exact_sha(self):
        with self._isolated_paths():
            os.makedirs(ts.GIT_CACHE)
            with open(ts.GIT_CACHE_MARKER, "w") as f:
                f.write(ts.GIT_CACHE_FORMAT + "\n")
            calls = []

            def git(cache, *args, **_kwargs):
                calls.append(args)
                if args[:2] == ("rev-parse", "--verify"):
                    return (ts.PINNED_SHA + "\n").encode()
                if args == ("remote", "get-url", "origin"):
                    return (ts.TS_GIT_URL + "\n").encode()
                if args[:1] == ("for-each-ref",):
                    return b""
                if args[:2] == ("config", "--get-regexp"):
                    return b""
                return b"true\n"

            with mock.patch.object(ts, "run_git", side_effect=git):
                ts.prepare_git_cache()

            self.assertNotIn(("init", "--bare"), calls)
            self.assertIn(("fetch", "--depth=1", "--no-tags", "origin",
                           f"{ts.PINNED_SHA}:{ts.PINNED_REF}"), calls)

    def test_run_git_disables_replace_objects_and_hostile_git_environment(self):
        with self._isolated_paths(authoritative=False):
            self._write_compatible_bare_cache()
            blob = self._raw_git("hash-object", "-w", "--stdin", input_data=b"trusted\n")
            tree = self._raw_git(
                "mktree", input_data=b"100644 blob " + blob + b"\ttrusted.txt\n")
            empty_tree = self._raw_git("mktree", input_data=b"")
            original = self._write_commit_object(tree, b"original")
            replacement = self._write_commit_object(empty_tree, b"replacement")
            self._raw_git("replace", original.decode(), replacement.decode())

            hostile = {
                "GIT_DIR": "/tmp/redirected",
                "GIT_WORK_TREE": "/tmp/redirected-worktree",
                "GIT_COMMON_DIR": "/tmp/redirected-common",
                "GIT_OBJECT_DIRECTORY": "/tmp/redirected-objects",
                "GIT_ALTERNATE_OBJECT_DIRECTORIES": "/tmp/redirected-alternates",
                "GIT_REPLACE_REF_BASE": "refs/other",
                "GIT_CONFIG_COUNT": "1",
                "GIT_CONFIG_KEY_0": "core.repositoryformatversion",
                "GIT_CONFIG_VALUE_0": "99",
            }
            with mock.patch.dict(os.environ, hostile):
                names = ts.run_git(
                    ts.GIT_CACHE, "ls-tree", "-r", "--name-only", original.decode())
            self.assertEqual(names, b"trusted.txt\n")

    def test_cache_rejects_replace_refs_and_legacy_grafts(self):
        with self._isolated_paths(authoritative=False):
            self._write_compatible_bare_cache()
            tree = self._raw_git("mktree", input_data=b"")
            original = self._write_commit_object(tree, b"original")
            replacement = self._write_commit_object(tree, b"replacement")
            pinned_ref = f"refs/typokat/pinned/{original.decode()}"
            self._raw_git("update-ref", pinned_ref, original.decode())
            self._raw_git("replace", original.decode(), replacement.decode())
            with mock.patch.object(ts, "PINNED_SHA", original.decode()), \
                 mock.patch.object(ts, "PINNED_REF", pinned_ref):
                self.assertFalse(ts.cache_is_compatible())

        with self._isolated_paths(authoritative=False):
            self._write_compatible_bare_cache()
            tree = self._raw_git("mktree", input_data=b"")
            original = self._write_commit_object(tree, b"original")
            replacement = self._write_commit_object(tree, b"replacement")
            pinned_ref = f"refs/typokat/pinned/{original.decode()}"
            self._raw_git("update-ref", pinned_ref, original.decode())
            grafts = os.path.join(ts.GIT_CACHE, "info", "grafts")
            with open(grafts, "wb") as f:
                f.write(original + b" " + replacement + b"\n")
            with mock.patch.object(ts, "PINNED_SHA", original.decode()), \
                 mock.patch.object(ts, "PINNED_REF", pinned_ref):
                self.assertFalse(ts.cache_is_compatible())

    def test_cache_requires_the_pinned_gc_reachable_ref(self):
        with self._isolated_paths(authoritative=False):
            self._write_compatible_bare_cache()
            tree = self._raw_git("mktree", input_data=b"")
            self._write_commit_object(tree, b"unreferenced")
            self.assertFalse(ts.cache_is_compatible())

    def test_git_cache_rejects_resolved_sha_mismatch(self):
        with self._isolated_paths():
            os.makedirs(ts.GIT_CACHE)
            with open(ts.GIT_CACHE_MARKER, "w") as f:
                f.write(ts.GIT_CACHE_FORMAT + "\n")

            def git(_cache, *args, **_kwargs):
                if args[:2] == ("rev-parse", "--verify"):
                    return ("f" * 40 + "\n").encode()
                if args == ("remote", "get-url", "origin"):
                    return (ts.TS_GIT_URL + "\n").encode()
                if args[:1] == ("for-each-ref",):
                    return b""
                if args[:2] == ("config", "--get-regexp"):
                    return b""
                return b"true\n"

            with mock.patch.object(ts, "run_git", side_effect=git):
                with self.assertRaises(ts.FetchFailure):
                    ts.prepare_git_cache()

    def test_corrupt_git_cache_is_reinitialized_before_fetching(self):
        with self._isolated_paths():
            os.makedirs(ts.GIT_CACHE)
            calls = []

            def git(_cache, *args, **_kwargs):
                calls.append(args)
                if args == ("rev-parse", "--is-bare-repository"):
                    raise ts.FetchFailure("corrupt cache")
                if args[:2] == ("rev-parse", "--verify"):
                    return (ts.PINNED_SHA + "\n").encode()
                return b""

            with mock.patch.object(ts, "run_git", side_effect=git):
                self.assertEqual(ts.prepare_git_cache(), ts.PINNED_SHA)
            self.assertIn(("init", "--bare"), calls)
            self.assertIn(("remote", "add", "origin", ts.TS_GIT_URL), calls)

    def test_git_fsck_failure_recreates_cache_before_fetching(self):
        with self._isolated_paths():
            os.makedirs(ts.GIT_CACHE)
            with open(ts.GIT_CACHE_MARKER, "w") as f:
                f.write(ts.GIT_CACHE_FORMAT + "\n")
            calls = []

            def git(_cache, *args, **_kwargs):
                calls.append(args)
                if args == ("remote", "get-url", "origin"):
                    return (ts.TS_GIT_URL + "\n").encode()
                if args[:1] == ("for-each-ref",):
                    return b""
                if args[:2] == ("config", "--get-regexp"):
                    return b""
                if args == ("fsck", "--no-dangling"):
                    raise ts.FetchFailure("broken object")
                if args[:2] == ("rev-parse", "--verify"):
                    return (ts.PINNED_SHA + "\n").encode()
                return b"true\n"

            with mock.patch.object(ts, "run_git", side_effect=git):
                ts.prepare_git_cache()

            self.assertIn(("init", "--bare"), calls)

    def test_partial_clone_cache_is_recreated_as_full_blob_cache(self):
        with self._isolated_paths():
            os.makedirs(ts.GIT_CACHE)
            with open(ts.GIT_CACHE_MARKER, "w") as f:
                f.write(ts.GIT_CACHE_FORMAT + "\n")
            calls = []

            def git(_cache, *args, **_kwargs):
                calls.append(args)
                if args[:2] == ("rev-parse", "--verify"):
                    return (ts.PINNED_SHA + "\n").encode()
                if args == ("remote", "get-url", "origin"):
                    return (ts.TS_GIT_URL + "\n").encode()
                if args[:1] == ("for-each-ref",):
                    return b""
                if args[:2] == ("config", "--get-regexp"):
                    return b"remote.origin.promisor true\n"
                return b"true\n"

            with mock.patch.object(ts, "run_git", side_effect=git):
                ts.prepare_git_cache()
            self.assertIn(("init", "--bare"), calls)

    def test_git_tree_enumerates_required_roots_and_rejects_empty_one(self):
        with self._isolated_paths(("conformance/x",)):
            root = b"040000 tree " + b"a" * 40 + b"\ttests/cases/conformance/x\0"
            files = self._tree_entry("100644", "blob", "b" * 40,
                                     "tests/cases/conformance/x/case.ts")

            def git(_cache, *args):
                return root if "-r" not in args else files

            with mock.patch.object(ts, "run_git", side_effect=git):
                self.assertEqual(ts.collect_git_ts_paths(ts.PINNED_SHA, ts.DEFAULT_DIRS),
                                 ["tests/cases/conformance/x/case.ts"])
            with mock.patch.object(ts, "run_git", return_value=b""):
                with self.assertRaises(ts.FetchFailure):
                    ts.collect_git_ts_paths(ts.PINNED_SHA, ts.DEFAULT_DIRS)

    def test_baseline_tree_uses_its_own_pinned_path_root(self):
        baseline = (
            self._tree_entry("100644", "blob", "a" * 40,
                             "tests/baselines/reference/2dArrays.js")
            + self._tree_entry("100644", "blob", "b" * 40,
                               "tests/baselines/reference/case.errors.txt")
        )
        with self._isolated_paths():
            with mock.patch.object(ts, "run_git", return_value=baseline):
                self.assertEqual(ts.baseline_paths(ts.PINNED_SHA), {
                    "tests/baselines/reference/case.errors.txt",
                })
        malformed = self._tree_entry("100644", "blob", "c" * 40,
                                     "tests/baselines/reference/../bad.errors.txt")
        with self._isolated_paths():
            with mock.patch.object(ts, "run_git", return_value=malformed):
                with self.assertRaises(ts.FetchFailure):
                    ts.baseline_paths(ts.PINNED_SHA)

    def test_variant_inventory_matches_the_exact_source_basename(self):
        exact = "tests/baselines/reference/case(target=es5).errors.txt"
        prefixed = "tests/baselines/reference/case(extra)(target=es5).errors.txt"
        self.assertEqual(ts.selected_baselines_for_stem({exact, prefixed}, "case"), {exact})

    def test_variant_inventory_rejects_one_baseline_assigned_to_two_stems(self):
        shared = "tests/baselines/reference/case(target=es5).errors.txt"
        with self.assertRaises(ts.FetchFailure):
            ts.selected_baseline_inventory({shared}, ["case", "case(target=es5)"])

    def test_equal_option_variants_publish_one_truthful_oracle(self):
        """Option-suffixed baselines with the same parsed diagnostics are one
        stable oracle, never an expected-clean test."""
        with self._isolated_paths():
            source = "tests/cases/conformance/x/case.ts"
            es5 = "tests/baselines/reference/case(target=es5).errors.txt"
            es2015 = "tests/baselines/reference/case(target=es2015).errors.txt"
            baseline = b"case.ts(2,7): error TS2322: mismatch\n"
            blobs = {
                source: b"// @target: es5, es2015\nconst x: string = 1;\n",
                es5: baseline,
                es2015: baseline,
            }
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[source]), \
                 mock.patch.object(ts, "baseline_paths", return_value={es5, es2015}), \
                 mock.patch.object(ts, "read_git_blobs", return_value=blobs):
                ts.cmd_fetch(self._args())

            with open(os.path.join(ts.CORPUS, "manifest.json")) as f:
                manifest = json.load(f)
            entry = manifest["tests"][0]
            self.assertEqual(manifest["format"], 4)
            self.assertEqual(entry["baseline_mode"], "stable-variants")
            self.assertEqual(entry["baseline_sources"], [
                {"options": {"target": "es5"}, "source": es5, "baseline": True},
                {"options": {"target": "es2015"}, "source": es2015,
                 "baseline": True},
            ])
            with open(os.path.join(ts.CORPUS, "conformance/x/case.errors.txt"), "rb") as f:
                self.assertEqual(f.read(), baseline)

    def test_option_sensitive_variants_are_explicitly_out_of_scope(self):
        """A missing clean variant and an error-bearing variant cannot be
        collapsed or silently called expected-clean."""
        with self._isolated_paths():
            source = "tests/cases/conformance/x/case.ts"
            es5 = "tests/baselines/reference/case(target=es5).errors.txt"
            es2015 = "tests/baselines/reference/case(target=es2015).errors.txt"
            blobs = {
                source: b"// @target: es5, es2015\nconst x: string = 1;\n",
                es2015: b"case.ts(2,7): error TS2322: mismatch\n",
            }
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[source]), \
                 mock.patch.object(ts, "baseline_paths", return_value={es2015}), \
                 mock.patch.object(ts, "read_git_blobs", return_value=blobs):
                ts.cmd_fetch(self._args())

            with open(os.path.join(ts.CORPUS, "manifest.json")) as f:
                manifest = json.load(f)
            entry = manifest["tests"][0]
            self.assertEqual(entry["baseline_mode"], "option-variant")
            self.assertEqual(entry["baseline_sources"], [
                {"options": {"target": "es5"}, "source": es5, "baseline": False},
                {"options": {"target": "es2015"}, "source": es2015,
                 "baseline": True},
            ])
            self.assertFalse(os.path.exists(
                os.path.join(ts.CORPUS, "conformance/x/case.errors.txt")))

            binary = fake_clean_batch_binary()
            try:
                ts.cmd_run(self._args(binary=binary))
            finally:
                os.unlink(binary)
            with open(os.path.join(ts.REPORT, "latest.json")) as f:
                report = json.load(f)
            row = next(item for item in report["files"] if item["rel"].endswith("case.ts"))
            self.assertEqual(row["bucket"], "option-variant")

    def test_manifest_rejects_tampered_option_variant_provenance(self):
        with self._isolated_paths():
            source = "tests/cases/conformance/x/case.ts"
            es5 = "tests/baselines/reference/case(target=es5).errors.txt"
            es2015 = "tests/baselines/reference/case(target=es2015).errors.txt"
            blobs = {
                source: b"// @target: es5, es2015\nconst x: string = 1;\n",
                es5: b"case.ts(2,7): error TS2322: mismatch\n",
                es2015: b"case.ts(3,7): error TS2322: mismatch\n",
            }
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[source]), \
                 mock.patch.object(ts, "baseline_paths", return_value={es5, es2015}), \
                 mock.patch.object(ts, "read_git_blobs", return_value=blobs):
                ts.cmd_fetch(self._args())

            manifest_path = os.path.join(ts.CORPUS, "manifest.json")
            with open(manifest_path) as f:
                original = json.load(f)
            mutations = [
                ("options", {"target": "esnext"}),
                ("source", "tests/baselines/reference/other(target=es5).errors.txt"),
                ("source", "tests/baselines/reference/../escape.errors.txt"),
            ]
            for field, value in mutations:
                manifest = json.loads(json.dumps(original))
                manifest["tests"][0]["baseline_sources"][0][field] = value
                with open(manifest_path, "w") as f:
                    json.dump(manifest, f)
                with self.assertRaises(ts.FetchFailure):
                    ts.load_manifest()

    def test_manifest_rejects_variant_relabelled_as_expected_clean_single(self):
        source = "tests/cases/conformance/x/case.ts"
        es5 = "tests/baselines/reference/case(target=es5).errors.txt"
        es2015 = "tests/baselines/reference/case(target=es2015).errors.txt"
        for equal in (True, False):
            with self.subTest(equal=equal), self._isolated_paths():
                blobs = {
                    source: b"// @target: es5, es2015\nconst x: string = 1;\n",
                    es5: b"case.ts(2,7): error TS2322: mismatch\n",
                    es2015: (
                        b"case.ts(2,7): error TS2322: mismatch\n" if equal
                        else b"case.ts(3,7): error TS2322: mismatch\n"
                    ),
                }
                with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                     mock.patch.object(ts, "collect_git_ts_paths", return_value=[source]), \
                     mock.patch.object(ts, "baseline_paths", return_value={es5, es2015}), \
                     mock.patch.object(ts, "read_git_blobs", return_value=blobs):
                    ts.cmd_fetch(self._args())

                manifest_path = os.path.join(ts.CORPUS, "manifest.json")
                with open(manifest_path) as f:
                    manifest = json.load(f)
                entry = manifest["tests"][0]
                entry["baseline_mode"] = "single"
                entry["baseline_sources"] = [{
                    "options": {},
                    "source": "tests/baselines/reference/case.errors.txt",
                    "baseline": False,
                }]
                entry["baseline"] = False
                canonical = os.path.join(ts.CORPUS, "conformance/x/case.errors.txt")
                if os.path.exists(canonical):
                    os.unlink(canonical)
                with open(manifest_path, "w") as f:
                    json.dump(manifest, f)
                with self.assertRaises(ts.FetchFailure):
                    ts.load_manifest()

    def test_manifest_rejects_shrinking_a_wildcard_option_domain(self):
        with self._isolated_paths():
            source = "tests/cases/conformance/x/case.ts"
            es5 = "tests/baselines/reference/case(target=es5).errors.txt"
            es2015 = "tests/baselines/reference/case(target=es2015).errors.txt"
            blobs = {
                source: b"// @target: *\nconst x: string = 1;\n",
                es5: b"case.ts(2,7): error TS2322: mismatch\n",
                es2015: b"case.ts(3,7): error TS2322: mismatch\n",
            }
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[source]), \
                 mock.patch.object(ts, "baseline_paths", return_value={es5, es2015}), \
                 mock.patch.object(ts, "read_git_blobs", return_value=blobs):
                ts.cmd_fetch(self._args())

            manifest_path = os.path.join(ts.CORPUS, "manifest.json")
            with open(manifest_path) as f:
                manifest = json.load(f)
            manifest["tests"][0]["baseline_sources"] = [
                manifest["tests"][0]["baseline_sources"][0]
            ]
            with open(manifest_path, "w") as f:
                json.dump(manifest, f)
            with self.assertRaises(ts.FetchFailure):
                ts.load_manifest()

    def test_manifest_rejects_simultaneous_inventory_and_entry_relabel(self):
        with self._isolated_paths():
            source = "tests/cases/conformance/x/case.ts"
            es5 = "tests/baselines/reference/case(target=es5).errors.txt"
            es2015 = "tests/baselines/reference/case(target=es2015).errors.txt"
            blobs = {
                source: b"// @target: es5, es2015\nconst x: string = 1;\n",
                es5: b"case.ts(2,7): error TS2322: mismatch\n",
                es2015: b"case.ts(3,7): error TS2322: mismatch\n",
            }
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[source]), \
                 mock.patch.object(ts, "baseline_paths", return_value={es5, es2015}), \
                 mock.patch.object(ts, "read_git_blobs", return_value=blobs):
                ts.cmd_fetch(self._args())

            manifest_path = os.path.join(ts.CORPUS, "manifest.json")
            with open(manifest_path) as f:
                manifest = json.load(f)
            manifest["baseline_inventory"]["case"] = []
            manifest["tests"][0].update({
                "baseline_mode": "single",
                "baseline_sources": [{
                    "options": {},
                    "source": "tests/baselines/reference/case.errors.txt",
                    "baseline": False,
                }],
                "baseline": False,
            })
            with open(manifest_path, "w") as f:
                json.dump(manifest, f)
            with self.assertRaises(ts.FetchFailure):
                ts.load_manifest()

    def test_manifest_rejects_substituted_authoritative_inventory(self):
        with self._isolated_paths():
            source = "tests/cases/conformance/x/case.ts"
            es5 = "tests/baselines/reference/case(target=es5).errors.txt"
            es2015 = "tests/baselines/reference/case(target=es2015).errors.txt"
            esnext = "tests/baselines/reference/case(target=esnext).errors.txt"
            baseline = b"case.ts(2,7): error TS2322: mismatch\n"
            blobs = {
                source: b"// @target: es5, es2015, esnext\nconst x: string = 1;\n",
                es5: baseline,
                es2015: baseline,
            }
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[source]), \
                 mock.patch.object(ts, "baseline_paths", return_value={es5, es2015}), \
                 mock.patch.object(ts, "read_git_blobs", return_value=blobs):
                ts.cmd_fetch(self._args())

            manifest_path = os.path.join(ts.CORPUS, "manifest.json")
            with open(manifest_path) as f:
                manifest = json.load(f)
            manifest["baseline_inventory"]["case"] = [es2015, esnext]
            for item in manifest["tests"][0]["baseline_sources"]:
                item["baseline"] = item["source"] in {es2015, esnext}
            with open(manifest_path, "w") as f:
                json.dump(manifest, f)
            with self.assertRaises(ts.FetchFailure):
                ts.load_manifest()

    def test_manifest_requires_matching_offline_pinned_cache(self):
        with self._isolated_paths(authoritative=False):
            path = "tests/cases/conformance/x/fresh.ts"
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[path]), \
                 mock.patch.object(ts, "baseline_paths", return_value=set()), \
                 mock.patch.object(ts, "read_git_blobs", return_value={
                     path: b"const fresh = 1;\n",
                 }):
                ts.cmd_fetch(self._args())

            with self.assertRaises(ts.FetchFailure):
                ts.load_manifest()
            os.makedirs(ts.GIT_CACHE, exist_ok=True)
            with open(ts.GIT_CACHE_MARKER, "w") as f:
                f.write(ts.GIT_CACHE_FORMAT + "\n")
            with mock.patch.object(ts, "cache_is_compatible", return_value=True), \
                 mock.patch.object(ts, "run_git", return_value=("f" * 40 + "\n").encode()):
                with self.assertRaises(ts.FetchFailure):
                    ts.load_manifest()

    def test_fetch_rejects_duplicate_selected_basename(self):
        with self._isolated_paths():
            first = "tests/cases/conformance/x/one/case.ts"
            second = "tests/cases/conformance/x/two/case.ts"
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[first, second]), \
                 mock.patch.object(ts, "baseline_paths", return_value=set()), \
                 mock.patch.object(ts, "read_git_blobs", return_value={
                     first: b"const first = 1;\n", second: b"const second = 2;\n",
                 }):
                with self.assertRaises(ts.FetchFailure):
                    ts.cmd_fetch(self._args())

    def test_git_tree_rejects_path_escape_and_links(self):
        escaped = self._tree_entry("100644", "blob", "a" * 40, "tests/cases/../escape.ts")
        with self.assertRaises(ts.FetchFailure):
            ts.parse_git_tree(escaped, "tests/cases/conformance/x")
        linked = self._tree_entry("120000", "blob", "a" * 40,
                                  "tests/cases/conformance/x/link.ts")
        with self.assertRaises(ts.FetchFailure):
            ts.parse_git_tree(linked, "tests/cases/conformance/x")

    def test_git_paths_reject_control_characters_before_batch_query(self):
        for bad in ("tests/cases/conformance/x/a\x00.ts",
                    "tests/cases/conformance/x/a\n.ts",
                    "tests/cases/conformance/x/a\r.ts"):
            with self.assertRaises(ts.FetchFailure):
                ts.read_git_blobs(ts.PINNED_SHA, {bad})

    def test_git_blob_batch_uses_one_command_and_rejects_missing_source(self):
        paths = {"tests/cases/conformance/x/a.ts", "tests/baselines/reference/a.errors.txt"}
        body = (b"a" * 40 + b" blob 1\na\n" + b"b" * 40 + b" blob 1\nb\n")
        with self._isolated_paths():
            with mock.patch.object(ts, "run_git", return_value=body) as run:
                self.assertEqual(ts.read_git_blobs(ts.PINNED_SHA, paths), {
                    "tests/baselines/reference/a.errors.txt": b"a",
                    "tests/cases/conformance/x/a.ts": b"b",
                })
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[1:3], ("cat-file", "--batch"))
            with mock.patch.object(ts, "run_git", return_value=b"missing\n"):
                with self.assertRaises(ts.FetchFailure):
                    ts.read_git_blobs(ts.PINNED_SHA, paths)

    def test_failed_fetch_keeps_published_corpus_and_removes_staging(self):
        """A source failure after a prior staged source cannot contaminate the
        previous corpus or make a partial manifest look published."""
        with self._isolated_paths() as root:
            os.makedirs(ts.CORPUS)
            with open(os.path.join(ts.CORPUS, "manifest.json"), "w") as f:
                f.write('{"old": true}\n')
            with open(os.path.join(ts.CORPUS, "old.ts"), "w") as f:
                f.write("old\n")
            paths = ["tests/cases/conformance/x/one.ts"]
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=paths), \
                 mock.patch.object(ts, "baseline_paths", return_value=set()), \
                 mock.patch.object(ts, "read_git_blobs", side_effect=ts.FetchFailure("source missing")):
                with self.assertRaises(ts.FetchFailure):
                    ts.cmd_fetch(self._args())

            self.assertTrue(os.path.exists(os.path.join(ts.CORPUS, "old.ts")))
            with open(os.path.join(ts.CORPUS, "manifest.json")) as f:
                self.assertEqual(f.read(), '{"old": true}\n')
            self.assertFalse(any(name.startswith(".corpus-fetch-")
                                 for name in os.listdir(root)))

    def test_successful_fetch_replaces_corpus_with_complete_manifest(self):
        with self._isolated_paths():
            os.makedirs(ts.CORPUS)
            with open(os.path.join(ts.CORPUS, "old.ts"), "w") as f:
                f.write("old\n")
            path = "tests/cases/conformance/x/fresh.ts"

            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[path]), \
                 mock.patch.object(ts, "baseline_paths", return_value=set()), \
                 mock.patch.object(ts, "read_git_blobs", return_value={path: b"const fresh = 1;\n"}):
                ts.cmd_fetch(self._args())

            with open(os.path.join(ts.CORPUS, "manifest.json")) as f:
                manifest = json.load(f)
            self.assertEqual(manifest["mode"], "full-default")
            self.assertFalse(manifest["partial"])
            self.assertIsNone(manifest["limit"])
            self.assertEqual(manifest["transport"]["revision"], ts.PINNED_SHA)
            self.assertNotIn(ts.GIT_CACHE, json.dumps(manifest))
            self.assertEqual(manifest["tests"], [{
                "path": "conformance/x/fresh.ts", "source": path,
                "baseline_mode": "single",
                "baseline_sources": [{
                    "options": {},
                    "source": "tests/baselines/reference/fresh.errors.txt",
                    "baseline": False,
                }],
                "baseline": False,
            }])
            self.assertFalse(os.path.exists(os.path.join(ts.CORPUS, "old.ts")))

    def test_v1_v2_and_v3_manifest_formats_are_rejected(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            manifest_path = os.path.join(ts.CORPUS, "manifest.json")
            for legacy_format in (1, 2, 3):
                with open(manifest_path) as f:
                    manifest = json.load(f)
                manifest["format"] = legacy_format
                with open(manifest_path, "w") as f:
                    json.dump(manifest, f)
                with self.assertRaises(ts.FetchFailure):
                    ts.validate_ratchet_manifest()
                self._write_full_manifest(["conformance/x/a.ts"])

    def test_partial_git_fetch_records_exploratory_mode(self):
        with self._isolated_paths():
            path = "tests/cases/conformance/x/fresh.ts"
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[path]), \
                 mock.patch.object(ts, "baseline_paths", return_value=set()), \
                 mock.patch.object(ts, "read_git_blobs", return_value={path: b"const fresh = 1;\n"}):
                ts.cmd_fetch(self._args(dirs=["conformance/x"]))
            with open(os.path.join(ts.CORPUS, "manifest.json")) as f:
                manifest = json.load(f)
            self.assertEqual(manifest["mode"], "partial")
            self.assertTrue(manifest["partial"])

    def test_check_rejects_extra_or_partial_manifest_before_discovery(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            with open(os.path.join(ts.CORPUS, "conformance/x/stale.ts"), "w") as f:
                f.write("const stale = 1;\n")
            with self.assertRaises(ts.FetchFailure):
                ts.validate_check_manifest()

    def test_manifest_rejects_orphan_baseline_and_unexpected_file(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            with open(os.path.join(ts.CORPUS, "conformance/x/orphan.errors.txt"), "w") as f:
                f.write("orphan\n")
            with self.assertRaises(ts.FetchFailure):
                ts.load_manifest()
            os.unlink(os.path.join(ts.CORPUS, "conformance/x/orphan.errors.txt"))
            with open(os.path.join(ts.CORPUS, "unexpected.json"), "w") as f:
                f.write("{}\n")
            with self.assertRaises(ts.FetchFailure):
                ts.load_manifest()

    def test_publish_rename_failure_restores_old_corpus_and_cleans_staging(self):
        with self._isolated_paths() as root:
            os.makedirs(ts.CORPUS)
            with open(os.path.join(ts.CORPUS, "old.ts"), "w") as f:
                f.write("old\n")
            path = "tests/cases/conformance/x/fresh.ts"
            real_replace = os.replace

            def fail_publish_stage(src, dst):
                if os.path.basename(src).startswith(".corpus-fetch-") and dst == ts.CORPUS:
                    raise OSError("publish rename failed")
                return real_replace(src, dst)

            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[path]), \
                 mock.patch.object(ts, "baseline_paths", return_value=set()), \
                 mock.patch.object(ts, "read_git_blobs", return_value={path: b"const fresh = 1;\n"}), \
                 mock.patch.object(ts.os, "replace", side_effect=fail_publish_stage):
                with self.assertRaises(OSError):
                    ts.cmd_fetch(self._args())

            with open(os.path.join(ts.CORPUS, "old.ts")) as f:
                self.assertEqual(f.read(), "old\n")
            self.assertFalse(any(name.startswith((".corpus-fetch-", ".corpus-previous-"))
                                 for name in os.listdir(root)))

    def test_backup_cleanup_failure_keeps_new_corpus_and_warns(self):
        with self._isolated_paths() as root:
            os.makedirs(ts.CORPUS)
            with open(os.path.join(ts.CORPUS, "old.ts"), "w") as f:
                f.write("old\n")
            path = "tests/cases/conformance/x/fresh.ts"
            real_rmtree = ts.shutil.rmtree

            def fail_only_backup(target, *args, **kwargs):
                if os.path.basename(target).startswith(".corpus-previous-"):
                    raise OSError("backup busy")
                return real_rmtree(target, *args, **kwargs)

            stderr = io.StringIO()
            with mock.patch.object(ts, "prepare_git_cache", return_value=ts.PINNED_SHA), \
                 mock.patch.object(ts, "collect_git_ts_paths", return_value=[path]), \
                 mock.patch.object(ts, "baseline_paths", return_value=set()), \
                 mock.patch.object(ts, "read_git_blobs", return_value={path: b"const fresh = 1;\n"}), \
                 mock.patch.object(ts.shutil, "rmtree", side_effect=fail_only_backup), \
                 contextlib.redirect_stderr(stderr):
                ts.cmd_fetch(self._args())

            self.assertTrue(os.path.exists(os.path.join(ts.CORPUS, "manifest.json")))
            self.assertFalse(os.path.exists(os.path.join(ts.CORPUS, "old.ts")))
            backups = [name for name in os.listdir(root)
                       if name.startswith(".corpus-previous-")]
            self.assertEqual(len(backups), 1)
            self.assertIn(backups[0], stderr.getvalue())
            self.assertIn("retry cleanup", stderr.getvalue())

    def test_check_and_save_reject_stale_scoreboard_header_without_writing(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            ts.write_scoreboard([rec("conformance/x/a.ts")])
            with open(ts.SCOREBOARD) as f:
                stale = f.read().replace(ts.PINNED_SHA, "0" * 40, 1)
            with open(ts.SCOREBOARD, "w") as f:
                f.write(stale)
            before = self._scoreboard_bytes()
            binary = fake_binary("exit 0\n")
            try:
                for check, save in ((True, False), (False, True)):
                    with self.assertRaises(SystemExit):
                        ts.cmd_run(self._args(check=check, save=save, binary=binary))
                    self.assertEqual(self._scoreboard_bytes(), before)
            finally:
                os.unlink(binary)

    def test_check_rejects_missing_or_malformed_scoreboard_header(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            ts.write_scoreboard([rec("conformance/x/a.ts")])
            binary = fake_binary("exit 0\n")
            try:
                for header in ("# scoreboard\n", "# TS @ not-a-sha\n"):
                    with open(ts.SCOREBOARD) as f:
                        body = f.read()
                    body = body.replace(f"# TS @ {ts.PINNED_SHA}\n", header, 1)
                    with open(ts.SCOREBOARD, "w") as f:
                        f.write(body)
                    before = self._scoreboard_bytes()
                    with self.assertRaises(SystemExit):
                        ts.cmd_run(self._args(check=True, binary=binary))
                    self.assertEqual(self._scoreboard_bytes(), before)
                    ts.write_scoreboard([rec("conformance/x/a.ts")])
            finally:
                os.unlink(binary)

    def test_partial_or_limited_save_rejects_without_writing_scoreboard(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            ts.write_scoreboard([rec("conformance/x/a.ts")])
            before = self._scoreboard_bytes()
            with open(os.path.join(ts.CORPUS, "manifest.json")) as f:
                manifest = json.load(f)
            manifest.update(mode="partial", partial=True, limit=1)
            with open(os.path.join(ts.CORPUS, "manifest.json"), "w") as f:
                json.dump(manifest, f)
            binary = fake_binary("exit 0\n")
            try:
                with self.assertRaises(SystemExit):
                    ts.cmd_run(self._args(save=True, binary=binary))
                self.assertEqual(self._scoreboard_bytes(), before)
            finally:
                os.unlink(binary)

            self._write_full_manifest(["conformance/x/a.ts"])
            binary = fake_binary("exit 0\n")
            try:
                with self.assertRaises(SystemExit):
                    ts.cmd_run(self._args(save=True, limit=1, binary=binary))
                self.assertEqual(self._scoreboard_bytes(), before)
            finally:
                os.unlink(binary)

    def test_rebaseline_replaces_stale_or_missing_scoreboard_from_full_corpus(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            with open(ts.SCOREBOARD, "w") as f:
                f.write("# TS @ " + "0" * 40 + "\nIN\t0 0 0 0\t|\tstale.ts\n")
            binary = fake_clean_batch_binary()
            try:
                ts.cmd_run(self._args(save=True, rebaseline=True, binary=binary))
            finally:
                os.unlink(binary)
            board = ts.read_scoreboard()
            self.assertEqual(set(board), {"conformance/x/a.ts"})
            with open(ts.SCOREBOARD) as f:
                self.assertIn(f"# TS @ {ts.PINNED_SHA}\n", f.read())

            os.unlink(ts.SCOREBOARD)
            binary = fake_clean_batch_binary()
            try:
                ts.cmd_run(self._args(save=True, rebaseline=True, binary=binary))
            finally:
                os.unlink(binary)
            self.assertTrue(os.path.exists(ts.SCOREBOARD))

    def test_rebaseline_requires_save_and_rejects_check_combination(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            binary = fake_binary("exit 0\n")
            try:
                for check, save in ((False, False), (True, False), (True, True)):
                    with self.assertRaises(SystemExit):
                        ts.cmd_run(self._args(check=check, save=save, rebaseline=True,
                                              binary=binary))
            finally:
                os.unlink(binary)

    def test_rebaseline_still_rejects_partial_manifest(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            ts.write_scoreboard([rec("conformance/x/a.ts")])
            before = self._scoreboard_bytes()
            with open(os.path.join(ts.CORPUS, "manifest.json")) as f:
                manifest = json.load(f)
            manifest.update(mode="partial", partial=True, limit=1)
            with open(os.path.join(ts.CORPUS, "manifest.json"), "w") as f:
                json.dump(manifest, f)
            binary = fake_binary("exit 0\n")
            try:
                with self.assertRaises(SystemExit):
                    ts.cmd_run(self._args(save=True, rebaseline=True, binary=binary))
            finally:
                os.unlink(binary)
            self.assertEqual(self._scoreboard_bytes(), before)

    def test_successful_check_writes_zero_regression_pass_report(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            ts.write_scoreboard([rec("conformance/x/a.ts")])
            binary = fake_clean_batch_binary()
            try:
                ts.cmd_run(self._args(check=True, binary=binary))
            finally:
                os.unlink(binary)
            with open(os.path.join(ts.REPORT, "latest.json")) as f:
                report = json.load(f)
            self.assertEqual(report["ratchet"], {
                "checked": True, "verdict": "pass", "exit_code": 0,
                "regressions": 0, "progress": 0,
                "missing_from_corpus": 0, "missing_from_scoreboard": 0,
            })

    def test_failed_ratchet_report_carries_post_comparison_verdict(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            ts.write_scoreboard([rec("conformance/x/a.ts", matched=1, expected=1,
                                     matched_detail=[(1, 2322)])])
            binary = fake_clean_batch_binary()
            try:
                with self.assertRaises(SystemExit) as exited:
                    ts.cmd_run(self._args(check=True, binary=binary))
            finally:
                os.unlink(binary)
            self.assertEqual(exited.exception.code, 1)
            with open(os.path.join(ts.REPORT, "latest.json")) as f:
                report = json.load(f)
            self.assertEqual(report["ratchet"]["verdict"], "fail")
            self.assertEqual(report["ratchet"]["exit_code"], 1)
            self.assertEqual(report["ratchet"]["regressions"], 1)

# --- Finding 3: process / exit-code handling ---------------------------------

class ProcessHandlingTests(unittest.TestCase):
    def _run(self, script_body, content="const x = 1;\n"):
        binary = fake_binary(script_body)
        try:
            return ts.run_typokat(binary, content, "probe.ts")
        finally:
            os.unlink(binary)

    def test_exit0_with_diagnostics_is_harness_failure(self):
        """PRE-FIX: run_typokat ignored returncode; an exit-0 run that still
        printed a diagnostic was scored as a real diagnostic."""
        with self.assertRaises(ts.HarnessFailure):
            self._run(DIAG_LINES + "exit 0\n")

    def test_exit1_with_no_diagnostics_is_harness_failure(self):
        """PRE-FIX: exit 1 with unparseable output scored as ZERO diagnostics —
        a dropped-error false negative masquerading as a clean file."""
        with self.assertRaises(ts.HarnessFailure):
            self._run('echo "unexpected noise"\nexit 1\n')

    def test_unparseable_output_is_harness_failure(self):
        with self.assertRaises(ts.HarnessFailure):
            self._run('echo "panicked at some/file.rs: boom"\nexit 1\n')

    def test_signal_exit_is_harness_failure(self):
        """PRE-FIX: a crash (SIGSEGV/SIGABRT) yielded no parsed output and was
        scored as zero diagnostics."""
        with self.assertRaises(ts.HarnessFailure):
            self._run("kill -SEGV $$\n")

    def test_usage_exit_code_is_harness_failure(self):
        with self.assertRaises(ts.HarnessFailure):
            self._run('echo "error: bad usage" 1>&2\nexit 2\n')

    def test_valid_exit1_with_diagnostic_parses(self):
        parse_errors, diags, incompletes = self._run(DIAG_LINES + "exit 1\n")
        self.assertEqual(parse_errors, [])
        self.assertEqual(diags, [(1, 2322)])
        self.assertEqual(incompletes, [])

    def test_valid_exit0_clean_parses_empty(self):
        parse_errors, diags, incompletes = self._run("exit 0\n")
        self.assertEqual((parse_errors, diags, incompletes), ([], [], []))

    def test_valid_parse_error_exit1_parses(self):
        parse_errors, diags, incompletes = self._run('echo "error: Unexpected token"\nexit 1\n')
        self.assertEqual(len(parse_errors), 1)
        self.assertEqual(diags, [])
        self.assertEqual(incompletes, [])

    # --- exit 3: the first-class incomplete outcome (WU2) --------------------

    def test_valid_exit3_with_incomplete_parses(self):
        parse_errors, diags, incompletes = self._run(INCOMPLETE_LINES + "exit 3\n")
        self.assertEqual((parse_errors, diags), ([], []))
        self.assertEqual(incompletes, ["expr-infer/template-literal/interpolation"])

    def test_exit3_with_no_incomplete_is_harness_failure(self):
        """Exit 3 with unparseable output (no incomplete record) is a hard failure —
        a lost incomplete outcome must not be scored as a silent zero."""
        with self.assertRaises(ts.HarnessFailure):
            self._run('echo "some noise"\nexit 3\n')

    def test_exit0_with_incomplete_is_harness_failure(self):
        """A clean exit that still prints an incomplete record is inconsistent —
        exit 0 must be silent (exit-0-with-anything = hard inconsistency)."""
        with self.assertRaises(ts.HarnessFailure):
            self._run(INCOMPLETE_LINES + "exit 0\n")

    def test_exit3_carries_both_diagnostics_and_incomplete(self):
        """An exit-3 test round-trips BOTH its diagnostic diff and its incomplete
        identities: the demotion keeps the diagnostic visible."""
        parse_errors, diags, incompletes = self._run(
            DIAG_LINES + INCOMPLETE_LINES + "exit 3\n")
        self.assertEqual(parse_errors, [])
        self.assertEqual(diags, [(1, 2322)])
        self.assertEqual(incompletes, ["expr-infer/template-literal/interpolation"])

    def test_source_snippet_mentioning_incomplete_is_not_a_record(self):
        """WU7-B BLOCKER witness: rich output quotes source lines; a snippet (or a
        diagnostic message body) merely CONTAINING `incomplete[...]` must not
        fabricate a phantom incomplete record. PRE-FIX: the unanchored
        `INCOMPLETE_RE.search` captured `x` / `0` from the quoted source, consumed
        the diagnostic header line, and misclassified a complete exit-1 test as
        OOS:unsupported with a garbage identity."""
        script = (
            # Message body mentions incomplete[x] (a user string literal in the type).
            "echo 'error[TK2322]: Type \"incomplete[x]\" is not assignable to type number'\n"
            'echo "  --> $(basename "$2"):1:19"\n'
            "echo '  |'\n"
            # Rich source-snippet lines quoting user code containing the token.
            "echo '1 | const n: number = \"incomplete[x]\";'\n"
            "echo '  |                   ^^^^^^^^^^^^^^^'\n"
            "echo '2 | arr.incomplete[0];'\n"
            "exit 1\n"
        )
        parse_errors, diags, incompletes = self._run(script)
        self.assertEqual(incompletes, [],
                         "quoted source/message text must not parse as records")
        self.assertEqual(diags, [(1, 2322)],
                         "the diagnostic must still parse (exit-1 test stays IN)")
        self.assertEqual(parse_errors, [])

    def test_snippet_with_paren_colon_prefix_is_not_a_record(self):
        """WU7-B re-review witness (gap a): the `\\): ` alternative of the first fix
        matched MID-LINE inside rich source snippets, so user code containing
        `): incomplete[evil/id]` — as a string literal or a trailing comment — still
        fabricated a phantom record whose id passes shape validation. The harness
        only ever sees rich output (run_typokat passes no --format), where a real
        record is anchored at column 0."""
        script = (
            DIAG_LINES  # a real diagnostic, so exit 1 stays consistent
            + "echo '  |'\n"
            # String-literal shape: quoted source containing `): incomplete[...]`.
            "echo '1 | const n: number = \"f(1): incomplete[evil/id]\";'\n"
            # Trailing-comment shape on an error line.
            "echo '2 | bad(); // note(): incomplete[evil/id]'\n"
            "exit 1\n"
        )
        parse_errors, diags, incompletes = self._run(script)
        self.assertEqual(incompletes, [],
                         "mid-line `): incomplete[...]` must not parse as a record")
        self.assertEqual(diags, [(1, 2322)])
        self.assertEqual(parse_errors, [])

    def test_exit1_with_incomplete_record_is_harness_failure(self):
        """WU7-B re-review witness (gap b / R5): the Rust CLI can never exit 1 while
        printing an incomplete record (any incomplete forces exit 3), so a parsed
        diagnostic AND an incomplete record under exit 1 means parser fabrication or
        a broken binary — a hard inconsistency, symmetric with the exit-0/exit-3
        checks."""
        with self.assertRaises(ts.HarnessFailure):
            self._run(DIAG_LINES + INCOMPLETE_LINES + "exit 1\n")


# --- WU7: isolated same-process protocol ------------------------------------

class IsolatedBatchProtocolSpec(unittest.TestCase):
    """RED contract for WU7's one-process official-suite transport.

    Requests and responses are schema-1 JSONL. A response echoes the exact case id,
    worker PID, provider route, and pinned source-profile identity, then carries the
    ordinary CLI exit code plus bounded stdout/stderr. The harness returns no partial
    result if any frame or worker is untrustworthy.
    """

    MAX_FRAME_BYTES = 2 * 1024 * 1024

    GOOD_LOOP = """
response = {
    "schema": 1,
    "case_id": request["case_id"],
    "worker_pid": os.getpid(),
    "provider_route": provider_route,
    "profile_sha256": provider_profile_sha256,
    "exit_code": 0,
    "stdout": "",
    "stderr": "",
}
print(json.dumps(response), flush=True)
"""

    def _run(self, binary, cases, timeout=1):
        try:
            return ts.run_typokat_batch(binary, cases, timeout=timeout)
        finally:
            os.unlink(binary)

    @contextlib.contextmanager
    def _official_run(self):
        with tempfile.TemporaryDirectory() as root:
            saved = (ts.CORPUS, ts.REPORT)
            ts.CORPUS = os.path.join(root, "corpus")
            ts.REPORT = os.path.join(root, "report")
            os.makedirs(ts.CORPUS)
            tests = []
            for rel, source in [
                ("first.ts", "const first = 1;\n"),
                ("second.ts", "const second = 2;\n"),
            ]:
                with open(os.path.join(ts.CORPUS, rel), "w") as f:
                    f.write(source)
                tests.append({
                    "path": rel,
                    "source": f"tests/cases/{rel}",
                    "baseline_mode": "single",
                    "baseline_sources": [{
                        "options": {},
                        "source": f"tests/baselines/reference/{rel[:-3]}.errors.txt",
                        "baseline": False,
                    }],
                    "baseline": False,
                })
            with open(os.path.join(ts.CORPUS, "manifest.json"), "w") as f:
                json.dump({
                    "format": 4,
                    "sha": ts.PINNED_SHA,
                    "repo": ts.REPO,
                    "dirs": ts.DEFAULT_DIRS,
                    "limit": None,
                    "mode": "full-default",
                    "partial": False,
                    "transport": {
                        "kind": "git-cache-full-blob",
                        "url": ts.TS_GIT_URL,
                        "revision": ts.PINNED_SHA,
                        "cache_format": ts.GIT_CACHE_FORMAT,
                    },
                    "baseline_inventory": {"first": [], "second": []},
                    "tests": tests,
                }, f)
            binary = fake_binary("exit 0\n")
            args = type("Args", (), {
                "bin": binary,
                "limit": None,
                "check": False,
                "save": False,
                "rebaseline": False,
            })()
            try:
                with mock.patch.object(ts, "authoritative_baseline_inventory", return_value={
                    "first": [], "second": [],
                }):
                    yield args
            finally:
                os.unlink(binary)
                ts.CORPUS, ts.REPORT = saved

    def test_real_official_run_uses_one_batch_call_not_per_case_subprocesses(self):
        with self._official_run() as args:
            results = {
                "first.ts": ([], [], []),
                "second.ts": ([], [(1, 2322)], []),
            }
            with mock.patch.object(
                ts, "run_typokat_batch", create=True, return_value=results
            ) as batch, mock.patch.object(
                ts,
                "run_typokat",
                side_effect=AssertionError("official run used the retired per-case path"),
            ), mock.patch.object(
                ts.subprocess,
                "run",
                side_effect=AssertionError("official run spawned a renamed per-case runner"),
            ), mock.patch.object(
                ts.subprocess,
                "Popen",
                side_effect=AssertionError("official run spawned a renamed per-case worker"),
            ):
                ts.cmd_run(args)

            batch.assert_called_once()
            call_args = batch.call_args.args
            self.assertEqual(call_args[0], args.bin)
            self.assertEqual(call_args[1], [
                ("first.ts", "const first = 1;\n"),
                ("second.ts", "const second = 2;\n"),
            ])
            with open(os.path.join(ts.REPORT, "latest.json")) as f:
                report = json.load(f)
            self.assertEqual(
                [(row["rel"], row["fp_detail"]) for row in report["files"]],
                [("first.ts", []), ("second.ts", [[1, 2322]])],
                "cmd_run must score each case from its exact batch mapping",
            )

    def test_two_cases_use_one_worker_and_preserve_exact_ids(self):
        with tempfile.TemporaryDirectory() as root:
            boots = os.path.join(root, "boots")
            binary = fake_batch_binary(
                self.GOOD_LOOP,
                before_loop=f'open({boots!r}, "a").write("boot\\n")',
            )
            observed = self._run(binary, [
                ("first/case.ts", "export const first = 1;\n"),
                ("second/case.ts", "export const second = 2;\n"),
            ])
            self.assertEqual(observed, {
                "first/case.ts": ([], [], []),
                "second/case.ts": ([], [], []),
            })
            with open(boots) as f:
                self.assertEqual(f.read().splitlines(), ["boot"])

    def test_initialization_failure_is_a_hard_failure_with_no_partial_result(self):
        binary = fake_batch_binary(
            self.GOOD_LOOP,
            before_loop="""
print("error: failed to initialize embedded TypeScript 6.0.3 library: injected", file=sys.stderr)
sys.exit(2)
""",
        )
        with self.assertRaises(ts.HarnessFailure):
            self._run(binary, [("never-runs.ts", "const value = 1;\n")])

    def _failing_middle_worker(self, root, failure, replacement_init_failure=False):
        boots = os.path.join(root, "boots")
        cases = os.path.join(root, "cases")
        replacement = """
if boot > 1:
    print("error: failed to initialize embedded TypeScript 6.0.3 library: replacement", file=sys.stderr)
    sys.exit(2)
""" if replacement_init_failure else ""
        before = f"""
try:
    boot = int(open({boots!r}).read()) + 1
except (OSError, ValueError):
    boot = 1
open({boots!r}, "w").write(str(boot))
{replacement}
"""
        loop = f"""
open({cases!r}, "a").write(str(boot) + ":" + request["case_id"] + "\\n")
if boot == 1 and request["case_id"] == "middle.ts":
    {failure}
print(json.dumps({{
    "schema": 1,
    "case_id": request["case_id"],
    "worker_pid": os.getpid(),
    "provider_route": provider_route,
    "profile_sha256": provider_profile_sha256,
    "exit_code": 0,
    "stdout": "",
    "stderr": "",
}}), flush=True)
"""
        return fake_batch_binary(loop, before_loop=before), boots, cases

    def test_middle_case_crash_restarts_and_collects_the_following_case(self):
        with tempfile.TemporaryDirectory() as root:
            binary, boots, cases = self._failing_middle_worker(
                root, "os.kill(os.getpid(), signal.SIGKILL)"
            )
            with self.assertRaises(ts.HarnessFailure):
                self._run(binary, [
                    ("first.ts", "const first = 1;\n"),
                    ("middle.ts", "const middle = 2;\n"),
                    ("last.ts", "const last = 3;\n"),
                ])
            with open(boots) as f:
                self.assertEqual(f.read(), "2")
            with open(cases) as f:
                self.assertEqual(
                    f.read().splitlines(),
                    ["1:first.ts", "1:middle.ts", "2:last.ts"],
                )

    @unittest.skipUnless(hasattr(os, "fork"), "requires POSIX process groups")
    def test_crashed_worker_descendant_inheriting_pipes_is_killed_with_group(self):
        with tempfile.TemporaryDirectory() as root:
            descendant = os.path.join(root, "descendant")
            failure = (
                "child = os.fork()\n"
                "    if child == 0:\n"
                "        time.sleep(10)\n"
                "        os._exit(0)\n"
                f"    open({descendant!r}, 'w').write(str(child))\n"
                "    os.kill(os.getpid(), signal.SIGKILL)"
            )
            binary, boots, cases = self._failing_middle_worker(root, failure)
            with self.assertRaises(ts.HarnessFailure):
                self._run(binary, [
                    ("first.ts", "const first = 1;\n"),
                    ("middle.ts", "const middle = 2;\n"),
                    ("last.ts", "const last = 3;\n"),
                ], timeout=0.1)
            with open(boots) as f:
                self.assertEqual(f.read(), "2")
            with open(cases) as f:
                self.assertEqual(
                    f.read().splitlines(),
                    ["1:first.ts", "1:middle.ts", "2:last.ts"],
                )
            with open(descendant) as f:
                child_pid = int(f.read())
            deadline = time.monotonic() + 1
            while True:
                try:
                    os.kill(child_pid, 0)
                except ProcessLookupError:
                    break
                if time.monotonic() >= deadline:
                    self.fail(f"worker descendant {child_pid} survived group termination")
                time.sleep(0.01)

    def test_middle_case_timeout_restarts_and_collects_the_following_case(self):
        with tempfile.TemporaryDirectory() as root:
            binary, boots, cases = self._failing_middle_worker(root, "time.sleep(1)")
            with self.assertRaises(ts.HarnessFailure):
                self._run(binary, [
                    ("first.ts", "const first = 1;\n"),
                    ("middle.ts", "const middle = 2;\n"),
                    ("last.ts", "const last = 3;\n"),
                ], timeout=0.05)
            with open(boots) as f:
                self.assertEqual(f.read(), "2")
            with open(cases) as f:
                self.assertEqual(
                    f.read().splitlines(),
                    ["1:first.ts", "1:middle.ts", "2:last.ts"],
                )

    def test_middle_case_infrastructure_frame_restarts_and_collects_the_following_case(self):
        with tempfile.TemporaryDirectory() as root:
            failure = (
                'print(json.dumps({"schema": 1, "case_id": "middle.ts", '
                '"worker_pid": os.getpid(), "provider_route": provider_route, '
                '"profile_sha256": provider_profile_sha256, "infrastructure_error": '
                '"check worker panicked"}), flush=True); time.sleep(10)'
            )
            binary, boots, cases = self._failing_middle_worker(root, failure)
            with self.assertRaises(ts.HarnessFailure):
                self._run(binary, [
                    ("first.ts", "const first = 1;\n"),
                    ("middle.ts", "const middle = 2;\n"),
                    ("last.ts", "const last = 3;\n"),
                ])
            with open(cases) as f:
                self.assertEqual(
                    f.read().splitlines(),
                    ["1:first.ts", "1:middle.ts", "2:last.ts"],
                )
            with open(boots) as f:
                self.assertEqual(f.read(), "2")

    def test_middle_malformed_and_mismatched_frames_restart_before_following_case(self):
        failures = [
            'print("{", flush=True); time.sleep(10)',
            (
                'print(json.dumps({"schema": 1, "case_id": "wrong.ts", '
                '"worker_pid": os.getpid(), "provider_route": provider_route, '
                '"profile_sha256": provider_profile_sha256, '
                '"exit_code": 0, "stdout": "", '
                '"stderr": ""}), flush=True); time.sleep(10)'
            ),
        ]
        for failure in failures:
            with self.subTest(failure=failure):
                with tempfile.TemporaryDirectory() as root:
                    binary, boots, cases = self._failing_middle_worker(root, failure)
                    with self.assertRaises(ts.HarnessFailure):
                        self._run(binary, [
                            ("first.ts", "const first = 1;\n"),
                            ("middle.ts", "const middle = 2;\n"),
                            ("last.ts", "const last = 3;\n"),
                        ])
                    with open(boots) as f:
                        self.assertEqual(f.read(), "2")
                    with open(cases) as f:
                        self.assertEqual(
                            f.read().splitlines(),
                            ["1:first.ts", "1:middle.ts", "2:last.ts"],
                        )

    def test_replacement_initialization_failure_stops_without_a_retry_loop(self):
        with tempfile.TemporaryDirectory() as root:
            binary, boots, cases = self._failing_middle_worker(
                root,
                "os.kill(os.getpid(), signal.SIGKILL)",
                replacement_init_failure=True,
            )
            with self.assertRaises(ts.HarnessFailure):
                self._run(binary, [
                    ("first.ts", "const first = 1;\n"),
                    ("middle.ts", "const middle = 2;\n"),
                    ("last.ts", "const last = 3;\n"),
                ])
            with open(boots) as f:
                self.assertEqual(f.read(), "2")
            with open(cases) as f:
                self.assertEqual(f.read().splitlines(), ["1:first.ts", "1:middle.ts"])

    def test_malformed_duplicate_missing_and_mismatched_ids_are_hard_failures(self):
        duplicate_input = fake_batch_binary(self.GOOD_LOOP)
        with self.assertRaises(ts.HarnessFailure):
            self._run(duplicate_input, [
                ("same.ts", "const first = 1;\n"),
                ("same.ts", "const second = 2;\n"),
            ])

        malformed = fake_batch_binary('print("{", flush=True)')
        with self.assertRaises(ts.HarnessFailure):
            self._run(malformed, [("malformed.ts", "const value = 1;\n")])

        mismatch = fake_batch_binary("""
print(json.dumps({
    "schema": 1,
    "case_id": request["case_id"] + "-wrong",
    "worker_pid": os.getpid(),
    "provider_route": provider_route,
    "profile_sha256": provider_profile_sha256,
    "exit_code": 0,
    "stdout": "",
    "stderr": "",
}), flush=True)
""")
        with self.assertRaises(ts.HarnessFailure):
            self._run(mismatch, [("expected.ts", "const value = 1;\n")])

        duplicate = fake_batch_binary("""
print(json.dumps({
    "schema": 1,
    "case_id": "first.ts",
    "worker_pid": os.getpid(),
    "provider_route": provider_route,
    "profile_sha256": provider_profile_sha256,
    "exit_code": 0,
    "stdout": "",
    "stderr": "",
}), flush=True)
""")
        with self.assertRaises(ts.HarnessFailure):
            self._run(duplicate, [
                ("first.ts", "const first = 1;\n"),
                ("second.ts", "const second = 2;\n"),
            ])

        missing = fake_batch_binary("""
if request["case_id"] == "first.ts":
    print(json.dumps({
        "schema": 1,
        "case_id": "first.ts",
        "worker_pid": os.getpid(),
        "provider_route": provider_route,
        "profile_sha256": provider_profile_sha256,
        "exit_code": 0,
        "stdout": "",
        "stderr": "",
    }), flush=True)
""")
        with self.assertRaises(ts.HarnessFailure):
            self._run(missing, [
                ("first.ts", "const first = 1;\n"),
                ("second.ts", "const second = 2;\n"),
            ])

    def test_request_names_sources_and_total_frame_size_are_validated_before_send(self):
        empty_frame = ts._validate_batch_cases([("boundary.ts", "")])[0][1]
        exact_source = "x" * (self.MAX_FRAME_BYTES - len(empty_frame))
        exact_frame = ts._validate_batch_cases([("boundary.ts", exact_source)])[0][1]
        self.assertEqual(len(exact_frame), self.MAX_FRAME_BYTES)
        with self.assertRaises(ts.HarnessFailure):
            ts._validate_batch_cases([("boundary.ts", exact_source + "x")])

        invalid_cases = [
            [("", "const value = 1;\n")],
            [(7, "const value = 1;\n")],
            [("wrong-source.ts", False)],
            [("oversized.ts", "x" * self.MAX_FRAME_BYTES)],
        ]
        for cases in invalid_cases:
            with self.subTest(cases=repr(cases)[:80]):
                binary = fake_batch_binary(self.GOOD_LOOP)
                with self.assertRaises(ts.HarnessFailure):
                    self._run(binary, cases)

    def test_provider_attestation_is_stable_and_truthful_across_the_session(self):
        changing_route = fake_batch_binary("""
print(json.dumps({
    "schema": 1,
    "case_id": request["case_id"],
    "worker_pid": os.getpid(),
    "provider_route": provider_route if request["case_id"] == "first.ts" else "prelude",
    "profile_sha256": provider_profile_sha256,
    "exit_code": 0,
    "stdout": "",
    "stderr": "",
}), flush=True)
""")
        with self.assertRaises(ts.HarnessFailure):
            self._run(changing_route, [
                ("first.ts", "const first = 1;\n"),
                ("second.ts", "const second = 2;\n"),
            ])

    def test_schema_key_type_bounds_and_stdout_contamination_fail_closed(self):
        valid_prefix = (
            '"case_id": request["case_id"], "worker_pid": os.getpid(), '
            '"provider_route": provider_route, '
            '"profile_sha256": provider_profile_sha256'
        )
        invalid_frames = [
            f'print(json.dumps({{"schema": 2, {valid_prefix}, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": True, {valid_prefix}, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": "0", "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": True, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": -1, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 2, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 4, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 0, "stdout": 7, "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 0, "stdout": "", "stderr": False}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": 7, "worker_pid": os.getpid(), "provider_route": provider_route, "profile_sha256": provider_profile_sha256, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "worker_pid": True, "provider_route": provider_route, "profile_sha256": provider_profile_sha256, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "worker_pid": os.getpid() + 1, "provider_route": provider_route, "profile_sha256": provider_profile_sha256, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "provider_route": provider_route, "profile_sha256": provider_profile_sha256, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "worker_pid": os.getpid(), "profile_sha256": provider_profile_sha256, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "worker_pid": os.getpid(), "provider_route": provider_route, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "worker_pid": os.getpid(), "provider_route": True, "profile_sha256": provider_profile_sha256, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "worker_pid": os.getpid(), "provider_route": "prelude", "profile_sha256": provider_profile_sha256, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "worker_pid": os.getpid(), "provider_route": provider_route, "profile_sha256": True, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, "case_id": request["case_id"], "worker_pid": os.getpid(), "provider_route": provider_route, "profile_sha256": "wrong", "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 0, "stdout": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 0, "stdout": "", "stderr": "", "extra": 1}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 0, "stdout": "x" * 1048577, "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 0, "stdout": "", "stderr": "x" * 1048577}}), flush=True)',
            f'print(" " * {self.MAX_FRAME_BYTES} + json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 0, "stdout": "", "stderr": ""}}), flush=True)',
            f'print(json.dumps({{"schema": 1, {valid_prefix}, "exit_code": 0, "stdout": "unexpected", "stderr": ""}}), flush=True)',
            'sys.stdout.write("{\\\"schema\\\":1"); sys.stdout.flush(); sys.exit(0)',
            'sys.exit(0)',
            'print("unframed stdout", flush=True)',
        ]
        for index, loop in enumerate(invalid_frames):
            with self.subTest(index=index):
                binary = fake_batch_binary(loop)
                with self.assertRaises(ts.HarnessFailure):
                    self._run(binary, [("probe.ts", "const value = 1;\n")])

    def test_batch_uses_the_same_diagnostic_and_incomplete_exit_contract(self):
        binary = fake_batch_binary("""
if request["case_id"] == "diagnostic.ts":
    stderr = (
        "error[TK2322]: Type 'string' is not assignable to type 'number'\\n"
        + request["name"] + ":1:7\\n"
    )
    exit_code = 1
elif request["case_id"] == "incomplete.ts":
    stderr = (
        "incomplete[expr-infer/template-literal/interpolation]: skipped\\n"
        + "  --> " + request["name"] + ":1:1\\n"
    )
    exit_code = 3
else:
    stderr = ""
    exit_code = 0
print(json.dumps({
    "schema": 1,
    "case_id": request["case_id"],
    "worker_pid": os.getpid(),
    "provider_route": provider_route,
    "profile_sha256": provider_profile_sha256,
    "exit_code": exit_code,
    "stdout": "",
    "stderr": stderr,
}), flush=True)
""")
        observed = self._run(binary, [
            ("diagnostic.ts", "const value: number = 'wrong';\n"),
            ("incomplete.ts", "const value = 1;\n"),
            ("clean.ts", "const value = 1;\n"),
        ])
        self.assertEqual(observed, {
            "diagnostic.ts": ([], [(1, 2322)], []),
            "incomplete.ts": (
                [], [], ["expr-infer/template-literal/interpolation"]
            ),
            "clean.ts": ([], [], []),
        })

        inconsistent = fake_batch_binary("""
print(json.dumps({
    "schema": 1,
    "case_id": request["case_id"],
    "worker_pid": os.getpid(),
    "provider_route": provider_route,
    "profile_sha256": provider_profile_sha256,
    "exit_code": 0,
    "stdout": "",
    "stderr": "error[TK2322]: hidden diagnostic\\n" + request["name"] + ":1:1\\n",
}), flush=True)
""")
        with self.assertRaises(ts.HarnessFailure):
            self._run(inconsistent, [("inconsistent.ts", "const value = 1;\n")])


# --- scoreboard format round-trip --------------------------------------------

class ScoreboardFormatTests(unittest.TestCase):
    def test_identity_round_trips_through_write_read(self):
        rows = [
            rec("z.ts", matched=2, fp=1, expected=2,
                matched_detail=[(3, 2322), (1, 2345)], fp_detail=[(9, 2339)]),
            rec("y.ts", bucket="syntax:enum"),
        ]
        with temp_scoreboard(rows):
            board = ts.read_scoreboard()
        self.assertEqual(sorted(board["z.ts"]["matched_ids"]),
                         [(1, 2345), (3, 2322)])
        self.assertEqual(board["z.ts"]["fp_ids"], [(9, 2339)])
        # Out-of-scope rows carry no identity.
        self.assertEqual(board["y.ts"]["status"], "OOS:syntax:enum")
        self.assertIsNone(board["y.ts"]["matched_ids"])
        self.assertIsNone(board["y.ts"]["incomplete_ids"])

    def test_unsupported_round_trips_diag_diff_and_incomplete_identities(self):
        """An exit-3 (OOS:unsupported) record round-trips BOTH its diagnostic diff
        (matched/fp identities) AND its incomplete surface identities."""
        rows = [
            rec("u.ts", bucket="unsupported", matched=1, fn=1, fp=1, expected=2,
                matched_detail=[(3, 2322)], fp_detail=[(9, 2339)],
                incomplete_detail=["expr-infer/template-literal/interpolation",
                                   "stmt-check/try-statement/handler"]),
        ]
        with temp_scoreboard(rows):
            board = ts.read_scoreboard()
        row = board["u.ts"]
        self.assertEqual(row["status"], "OOS:unsupported")
        self.assertEqual(row["matched_ids"], [(3, 2322)])
        self.assertEqual(row["fp_ids"], [(9, 2339)])
        self.assertEqual(sorted(row["incomplete_ids"]),
                         ["expr-infer/template-literal/interpolation",
                          "stmt-check/try-statement/handler"])
        # The numeric diag diff is preserved so recall stays comparable across demotion.
        self.assertEqual((row["matched"], row["fn"], row["fp"], row["expected"]),
                         (1, 1, 1, 2))


# --- WU7-B hardening: incomplete-id shape validation --------------------------

class IncompleteIdValidationTests(unittest.TestCase):
    HOSTILE = ["with,comma/x", "with|pipe/x", "with\ttab/x", "noslash",
               "Upper/Case/Bad", "spaced id/x"]

    def test_hostile_id_never_written_to_scoreboard(self):
        """An id containing `,`, `|`, TAB, or any off-vocabulary character would
        corrupt the `<matched>|<fp>|<incomplete>` column — serialization hard-fails
        instead of silently corrupting the committed board."""
        for bad in self.HOSTILE:
            rows = [rec("u.ts", bucket="unsupported", incomplete_detail=[bad])]
            with self.assertRaises(ValueError, msg=f"id {bad!r} must be rejected"):
                with temp_scoreboard(rows):
                    pass

    def test_hostile_id_in_committed_scoreboard_hard_fails_on_read(self):
        """A malformed id already in the committed board (e.g. hand-edited) fails
        loudly on read rather than flowing into the ratchet comparison."""
        fd, path = tempfile.mkstemp(suffix=".scoreboard.txt")
        with os.fdopen(fd, "w") as f:
            f.write("# header\n")
            f.write("OOS:unsupported\t0 0 0 0\t||noslash\tu.ts\n")
        saved = ts.SCOREBOARD
        ts.SCOREBOARD = path
        try:
            with self.assertRaises(ValueError):
                ts.read_scoreboard()
        finally:
            ts.SCOREBOARD = saved
            os.unlink(path)

    def test_malformed_rendered_id_is_a_harness_failure(self):
        """A well-anchored incomplete record whose id is off-vocabulary is a hard
        harness failure at parse time — never recorded."""
        binary = fake_binary(
            'echo "incomplete[NOT A VALID ID]: bad"\nexit 3\n')
        try:
            with self.assertRaises(ts.HarnessFailure):
                ts.run_typokat(binary, "const x = 1;\n", "probe.ts")
        finally:
            os.unlink(binary)

    def test_valid_ids_pass_validation(self):
        rows = [rec("u.ts", bucket="unsupported",
                    incomplete_detail=["expr-infer/template-literal/interpolation",
                                       "stmt-check/try-statement/handler",
                                       "a/b", "x-1/y-2/z-3"])]
        with temp_scoreboard(rows):
            board = ts.read_scoreboard()
        self.assertEqual(len(board["u.ts"]["incomplete_ids"]), 4)


# --- Finding 4: the exit-3 unsupported outcome ratchet (WU2) ------------------

class UnsupportedOutcomeTests(unittest.TestCase):
    def _unsup(self, rel, **kw):
        return rec(rel, bucket="unsupported", **kw)

    def test_in_to_unsupported_is_a_regression(self):
        base = [rec("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)])]
        cur = [self._unsup("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)],
                           incomplete_detail=["expr-infer/template-literal/interpolation"])]
        with temp_scoreboard(base):
            self.assertFalse(compare(cur), "IN → OOS:unsupported must regress")

    def test_unsupported_dropped_matched_diagnostic_is_a_regression(self):
        """Demotion must not blind the harness: a matched diagnostic dropped inside a
        now-unsupported test is still a regression."""
        base = [self._unsup("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)],
                            incomplete_detail=["x/y/z"])]
        cur = [self._unsup("a.ts", matched=0, fn=1, expected=1,
                           incomplete_detail=["x/y/z"])]
        with temp_scoreboard(base):
            self.assertFalse(compare(cur))

    def test_unsupported_dropped_incomplete_identity_is_a_regression(self):
        base = [self._unsup("a.ts", incomplete_detail=["x/y/z", "p/q/r"])]
        cur = [self._unsup("a.ts", incomplete_detail=["x/y/z"])]
        with temp_scoreboard(base):
            self.assertFalse(compare(cur))

    def test_unsupported_gained_incomplete_identity_is_progress(self):
        base = [self._unsup("a.ts", incomplete_detail=["x/y/z"])]
        cur = [self._unsup("a.ts", incomplete_detail=["x/y/z", "p/q/r"])]
        with temp_scoreboard(base):
            self.assertTrue(compare(cur))

    def test_identical_unsupported_has_no_regression(self):
        rows = [self._unsup("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)],
                            incomplete_detail=["x/y/z"])]
        with temp_scoreboard(rows):
            self.assertTrue(compare(list(rows)))

    def test_unsupported_to_in_is_progress(self):
        base = [self._unsup("a.ts", incomplete_detail=["x/y/z"])]
        cur = [rec("a.ts", matched=1, expected=1, matched_detail=[(1, 2322)])]
        with temp_scoreboard(base):
            self.assertTrue(compare(cur))


if __name__ == "__main__":
    unittest.main()

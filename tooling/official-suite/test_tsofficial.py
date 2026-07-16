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


# --- Structural module-syntax prefilter -------------------------------------

class SyntaxBucketTests(unittest.TestCase):
    def test_identifier_namespace_forms_are_not_external_modules(self):
        cases = {
            "namespace": "namespace N { interface Value {} }\n",
            "module": "module N { interface Value {} }\n",
            "nested": (
                "namespace Outer {\n"
                "  export namespace Inner {\n"
                "    export interface Value {}\n"
                "  }\n"
                "}\n"
            ),
            "dotted": "namespace A.B.C { export type Value = string; }\n",
            "ambient-namespace": (
                "declare namespace Ambient { export interface Value {} }\n"
            ),
            "ambient-identifier-module": (
                "declare module Ambient { export interface Value {} }\n"
            ),
            "namespace-body-exports": (
                "namespace N {\n"
                "  export interface Shape {}\n"
                "  export type Name = string;\n"
                "  export const value = 1;\n"
                "}\n"
            ),
        }
        for name, source in cases.items():
            with self.subTest(name=name):
                self.assertIsNone(ts.syntax_bucket(source))

    def test_external_module_forms_remain_in_the_module_bucket(self):
        cases = {
            "import": 'import { value } from "pkg";\n',
            "export": "const value = 1;\nexport { value };\n",
            "export-as-namespace": "export as namespace Widget;\n",
            "require": 'const value = require("pkg");\n',
            "reference-directive": '/// <reference types="node" />\n',
            "string-literal-module": (
                'declare module "pkg" { export interface Value {} }\n'
            ),
            "external-module-augmentation": (
                'export {};\n'
                'declare module "./observable" {\n'
                '  interface Observable<T> { map<U>(f: (x: T) => U): Observable<U>; }\n'
                '}\n'
            ),
        }
        for name, source in cases.items():
            with self.subTest(name=name):
                self.assertEqual(ts.syntax_bucket(source), "module")

    def test_real_module_syntax_wins_over_identifier_namespaces(self):
        cases = {
            "namespace-plus-import": (
                "namespace Local { export interface Value {} }\n"
                'import { remote } from "pkg";\n'
            ),
            "namespace-plus-export": (
                "namespace Local { export interface Value {} }\n"
                "export {};\n"
            ),
            "namespace-plus-require": (
                "namespace Local { export interface Value {} }\n"
                'const remote = require("pkg");\n'
            ),
            "namespace-plus-reference": (
                '/// <reference path="./other.ts" />\n'
                "namespace Local { export interface Value {} }\n"
            ),
            "namespace-plus-string-literal-module": (
                "namespace Local { export interface Value {} }\n"
                'declare module "pkg" { export interface Remote {} }\n'
            ),
        }
        for name, source in cases.items():
            with self.subTest(name=name):
                self.assertEqual(ts.syntax_bucket(source), "module")

    def test_module_words_in_comments_and_strings_do_not_trigger_the_bucket(self):
        cases = {
            "line-comments": (
                "// namespace Fake { export const value = 1; }\n"
                '// require("pkg")\n'
                "const value = 1;\n"
            ),
            "block-comment": (
                "/*\n"
                "namespace Fake {}\n"
                "module AlsoFake {}\n"
                "export {};\n"
                'require("pkg");\n'
                '/// <reference types="node" />\n'
                'declare module "pkg" {}\n'
                "*/\n"
                "const value = 1;\n"
            ),
            "quoted-strings": (
                'const a = "namespace Fake { export const value = 1; }";\n'
                'const b = \'declare module "pkg" {}\';\n'
                'const c = `require("pkg")`;\n'
            ),
        }
        for name, source in cases.items():
            with self.subTest(name=name):
                self.assertIsNone(ts.syntax_bucket(source))


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
    def _isolated_paths(self, default_dirs=("conformance/x",)):
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
            try:
                yield root
            finally:
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
            json.dump({
                "format": 3, "sha": ts.PINNED_SHA, "repo": ts.REPO,
                "dirs": ts.DEFAULT_DIRS, "limit": None,
                "mode": "full-default", "partial": False,
                "transport": {"kind": "git-cache-full-blob", "url": ts.TS_GIT_URL,
                              "revision": ts.PINNED_SHA, "cache_format": ts.GIT_CACHE_FORMAT},
                "tests": [{"path": rel, "source": f"tests/cases/{rel}",
                           "baseline_source": "tests/baselines/reference/"
                           + os.path.basename(rel)[:-3] + ".errors.txt",
                           "baseline": False} for rel in paths],
            }, f)

    def _scoreboard_bytes(self):
        with open(ts.SCOREBOARD, "rb") as f:
            return f.read()

    @staticmethod
    def _tree_entry(mode, typ, oid, path):
        return f"{mode} {typ} {oid}\t{path}\0".encode()

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
                if args[:2] == ("config", "--get-regexp"):
                    return b""
                return b"true\n"

            with mock.patch.object(ts, "run_git", side_effect=git):
                ts.prepare_git_cache()

            self.assertNotIn(("init", "--bare"), calls)
            self.assertIn(("fetch", "--depth=1", "--no-tags", "origin",
                           f"{ts.PINNED_SHA}:{ts.PINNED_REF}"), calls)

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
                "baseline_source": "tests/baselines/reference/fresh.errors.txt",
                "baseline": False,
            }])
            self.assertFalse(os.path.exists(os.path.join(ts.CORPUS, "old.ts")))

    def test_v1_and_v2_manifest_formats_are_rejected(self):
        with self._isolated_paths():
            self._write_full_manifest(["conformance/x/a.ts"])
            manifest_path = os.path.join(ts.CORPUS, "manifest.json")
            for legacy_format in (1, 2):
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
            binary = fake_binary("exit 0\n")
            try:
                ts.cmd_run(self._args(save=True, rebaseline=True, binary=binary))
            finally:
                os.unlink(binary)
            board = ts.read_scoreboard()
            self.assertEqual(set(board), {"conformance/x/a.ts"})
            with open(ts.SCOREBOARD) as f:
                self.assertIn(f"# TS @ {ts.PINNED_SHA}\n", f.read())

            os.unlink(ts.SCOREBOARD)
            binary = fake_binary("exit 0\n")
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
            binary = fake_binary("exit 0\n")
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
            binary = fake_binary("exit 0\n")
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

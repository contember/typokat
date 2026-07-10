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
import os
import stat
import tempfile
import unittest

import tsofficial as ts


# --- helpers -----------------------------------------------------------------

def rec(rel, *, bucket=None, matched=0, fn=0, fp=0, expected=0,
        matched_detail=None, fp_detail=None, fn_detail=None, strict=True):
    """Build one cmd_run-shaped result record."""
    return {
        "rel": rel, "bucket": bucket, "strict": strict,
        "matched": matched, "fn": fn, "fp": fp, "expected": expected,
        "matched_detail": matched_detail or [],
        "fp_detail": fp_detail or [],
        "fn_detail": fn_detail or [],
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
        parse_errors, diags = self._run(DIAG_LINES + "exit 1\n")
        self.assertEqual(parse_errors, [])
        self.assertEqual(diags, [(1, 2322)])

    def test_valid_exit0_clean_parses_empty(self):
        parse_errors, diags = self._run("exit 0\n")
        self.assertEqual((parse_errors, diags), ([], []))

    def test_valid_parse_error_exit1_parses(self):
        parse_errors, diags = self._run('echo "error: Unexpected token"\nexit 1\n')
        self.assertEqual(len(parse_errors), 1)
        self.assertEqual(diags, [])


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


if __name__ == "__main__":
    unittest.main()

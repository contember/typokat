#!/usr/bin/env python3
"""
Unit tests for the differential harness's own logic — stdlib only, no third-party
deps, no checker binary, no tsc. Run:

    cd tooling/differential && python3 -m unittest test_differential -v

What is worth testing here is everything that could silently *hide* a divergence:
output parsing, the comparison, allowlist cancellation, the scoreboard round-trip,
generator determinism, and the shrinker's refusal to wander to a different bug.
"""

import os
import tempfile
import unittest
from collections import Counter

import differential as D
import grammar
from shrink import shrink_program, shrink_text


def outcome(diags, exit_code=1, incompletes=()):
    return D.Outcome(exit_code, tuple(diags), tuple(incompletes), "")


def diag(line, code, col=1, message="m", detail=()):
    return D.Diag(line, col, code, message, detail)


def _has_trigger(node, bound):
    """Does `node` contain a supersedable argument (arrow / fresh object or array
    literal) that references a parameter bound by an enclosing callback?"""
    if isinstance(node, grammar.Block):
        return any(_has_trigger(s, bound) for s in node.stmts)
    if isinstance(node, grammar.Ret):
        return _has_trigger(node.expr, bound)
    if isinstance(node, grammar.Arrow):
        inner = bound | {node.param} if node.param else bound
        return _has_trigger(node.body, frozenset(inner))
    if isinstance(node, grammar.Call):
        for a in node.args:
            supersedable = isinstance(a, (grammar.ObjLit, grammar.ArrLit)) or (
                isinstance(a, grammar.Arrow) and not isinstance(a.body, grammar.Block))
            if supersedable and any(grammar.uses(a, p) for p in bound):
                return True
        return any(_has_trigger(a, bound) for a in node.args)
    if isinstance(node, grammar.ObjLit):
        return any(_has_trigger(v, bound) for _n, v in node.props)
    if isinstance(node, grammar.ArrLit):
        return any(_has_trigger(v, bound) for v in node.items)
    return False


def rule(token, ref="backlog:48", reason="test"):
    m = D.RULE_RE.match(token)
    tk = int(m.group(1)) if m.group(1) else None
    ts = int(m.group(2)) if m.group(2) else None
    return D.AllowEntry(token, tk, ts, ref, reason)


class TestBinaryComparison(unittest.TestCase):
    def test_identical_outcomes_have_no_tokens(self):
        a = outcome([diag(3, 2345)])
        self.assertEqual(D.compare_binaries(a, a), [])

    def test_dropped_and_invented_are_distinguished(self):
        ref = outcome([diag(3, 2322)])
        cand = outcome([diag(7, 2769)])
        self.assertEqual(D.compare_binaries(ref, cand),
                         ["dropped:TK2322", "invented:TK2769"])

    def test_same_line_and_code_with_different_text_is_a_message_change(self):
        ref = outcome([diag(3, 2345, message="Argument of type '() => number'")])
        cand = outcome([diag(3, 2345, message="Argument of type '() => any'")])
        self.assertEqual(D.compare_binaries(ref, cand), ["message:TK2345"])

    def test_a_changed_reason_chain_is_a_finding(self):
        """412f321 also rendered wrong types in messages — the reason chain is part of
        the compared identity, not decoration."""
        ref = outcome([diag(3, 2345, detail=("Type 'number' is not assignable",))])
        cand = outcome([diag(3, 2345, detail=("Type 'any' is not assignable",))])
        self.assertEqual(D.compare_binaries(ref, cand), ["message:TK2345"])

    def test_a_changed_column_is_a_finding(self):
        ref = outcome([diag(3, 2345, col=7)])
        cand = outcome([diag(3, 2345, col=26)])
        self.assertEqual(D.compare_binaries(ref, cand), ["message:TK2345"])

    def test_exit_code_change_is_reported(self):
        self.assertIn("exit:1->0", D.compare_binaries(outcome([diag(1, 2322)]),
                                                      outcome([], exit_code=0)))

    def test_incomplete_records_are_compared(self):
        ref = outcome([], exit_code=3, incompletes=("decl/enum-declaration/self",))
        cand = outcome([], exit_code=0)
        tokens = D.compare_binaries(ref, cand)
        self.assertIn("incomplete-gone:decl/enum-declaration/self", tokens)


class TestTscComparison(unittest.TestCase):
    def test_agreement(self):
        a = outcome([diag(3, 2322)])
        self.assertEqual(D.compare_tsc(a, a, []), ([], Counter()))

    def test_code_mismatch_on_one_line(self):
        residue, hits = D.compare_tsc(outcome([diag(3, 2345)]), outcome([diag(3, 2322)]), [])
        self.assertEqual(residue, ["tk[2345]!=ts[2322]"])
        self.assertEqual(hits, Counter())

    def test_a_rule_cancels_exactly_its_pair(self):
        rules = [rule("tk[2345]~ts[2322]")]
        residue, hits = D.compare_tsc(outcome([diag(3, 2345)]), outcome([diag(3, 2322)]), rules)
        self.assertEqual(residue, [])
        self.assertEqual(hits["tk[2345]~ts[2322]"], 1)

    def test_a_rule_does_not_cover_its_neighbour(self):
        """The anti-junk-drawer property: allowlisting one shape says nothing about
        the shape one code away."""
        rules = [rule("tk[2345]~ts[2322]")]
        residue, _ = D.compare_tsc(outcome([diag(3, 2345)]), outcome([diag(3, 2769)]), rules)
        self.assertEqual(residue, ["tk[2345]!=ts[2769]"])

    def test_a_rule_cannot_swallow_a_second_divergence_on_the_same_line(self):
        """A line carrying an allowlisted divergence AND a new one still reports the
        new one — this is why rules cancel pairs instead of labelling lines."""
        rules = [rule("tk[2345]~ts[2322]")]
        residue, hits = D.compare_tsc(outcome([diag(3, 2345)]),
                                      outcome([diag(3, 2322), diag(3, 2769)]), rules)
        self.assertEqual(residue, ["tk[]!=ts[2769]"])
        self.assertEqual(hits["tk[2345]~ts[2322]"], 1)

    def test_one_sided_rule(self):
        rules = [rule("tk[]~ts[7006]")]
        residue, hits = D.compare_tsc(outcome([], exit_code=0), outcome([diag(4, 7006)]), rules)
        self.assertEqual(residue, [])
        self.assertEqual(hits["tk[]~ts[7006]"], 1)

    def test_paired_rules_are_applied_before_one_sided_ones(self):
        """Otherwise the one-sided rule eats the code the paired rule needed and the
        typokat side is left as spurious residue."""
        rules = [rule("tk[]~ts[2322]"), rule("tk[2345]~ts[2322]")]
        residue, _ = D.compare_tsc(outcome([diag(3, 2345)]), outcome([diag(3, 2322)]), rules)
        self.assertEqual(residue, [])

    def test_classification(self):
        self.assertEqual(D.classify([], Counter()), "MATCH")
        self.assertEqual(D.classify([], Counter({"r": 1})), "ALLOWED")
        self.assertEqual(D.classify(["tk[]!=ts[2322]"], Counter({"r": 1})), "DIVERGE")


class TestOutputParsing(unittest.TestCase):
    """The parser is the harness's blind spot: anything it silently drops becomes a
    divergence nobody sees."""

    def _fake_bin(self, stderr, code):
        """A stand-in `typokat` that prints fixed output and exits with a fixed code —
        the harness is black-box, so a script is as good as the real binary."""
        tmp = tempfile.mkdtemp()
        path = os.path.join(tmp, "fake-typokat")
        with open(path, "w", encoding="utf-8") as fh:
            fh.write("#!/usr/bin/env python3\nimport sys\n"
                     f"sys.stderr.write({stderr!r})\nsys.exit({code})\n")
        os.chmod(path, 0o755)
        return tmp, path

    def test_parses_a_diagnostic_and_its_reason_chain(self):
        tmp, path = self._fake_bin(
            "probe.ts(3,26): error TK2345: Argument of type '() => number'\n"
            "  Call signature return types are incompatible.\n"
            "    Type 'number' is not assignable to type 'string'.\n", 1)
        out = D.run_typokat(path, tmp, "probe.ts")
        self.assertEqual(len(out.diags), 1)
        self.assertEqual(out.diags[0].code, 2345)
        self.assertEqual(out.diags[0].line, 3)
        self.assertEqual(len(out.diags[0].detail), 2)

    def test_incomplete_record_forces_exit_three(self):
        tmp, path = self._fake_bin(
            "probe.ts(1,1): incomplete[decl/enum-declaration/self]: not modeled\n", 3)
        out = D.run_typokat(path, tmp, "probe.ts")
        self.assertEqual(out.incompletes, ("decl/enum-declaration/self",))

    def test_crash_is_a_hard_failure(self):
        tmp, path = self._fake_bin("thread panicked\n", 101)
        with self.assertRaises(D.HarnessFailure):
            D.run_typokat(path, tmp, "probe.ts")

    def test_clean_exit_with_output_is_a_hard_failure(self):
        tmp, path = self._fake_bin("probe.ts(3,1): error TK2322: x\n", 0)
        with self.assertRaises(D.HarnessFailure):
            D.run_typokat(path, tmp, "probe.ts")

    def test_exit_one_without_diagnostics_is_a_hard_failure(self):
        tmp, path = self._fake_bin("", 1)
        with self.assertRaises(D.HarnessFailure):
            D.run_typokat(path, tmp, "probe.ts")

    def test_parse_error_on_generated_source_is_a_hard_failure(self):
        """The grammar only emits well-formed TypeScript, so a parse error means the
        generator (or the parser) broke — never a data point."""
        tmp, path = self._fake_bin("error: Unexpected token\n", 1)
        with self.assertRaises(D.HarnessFailure):
            D.run_typokat(path, tmp, "probe.ts")

    def test_unparseable_line_is_a_hard_failure(self):
        tmp, path = self._fake_bin("something entirely unexpected\n", 1)
        with self.assertRaises(D.HarnessFailure):
            D.run_typokat(path, tmp, "probe.ts")

    def test_non_strict_mode_reports_rather_than_raises(self):
        tmp, path = self._fake_bin("error: Unexpected token\n", 1)
        self.assertEqual(D.run_typokat(path, tmp, "probe.ts", strict=False).exit_code, -1)


class TestAllowlist(unittest.TestCase):
    def _write(self, text):
        fd, path = tempfile.mkstemp(suffix=".txt")
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(text)
        return path

    def test_parses_rules_and_ignores_comments(self):
        path = self._write("# a comment\n\ntk[2345]~ts[2322]\tbacklog:48\tbecause\n")
        entries = D.load_allowlist(path, validate=False)
        self.assertEqual(list(entries), ["tk[2345]~ts[2322]"])
        self.assertEqual(entries["tk[2345]~ts[2322]"].tk, 2345)
        self.assertEqual(entries["tk[2345]~ts[2322]"].ts, 2322)

    def test_rejects_a_malformed_rule(self):
        path = self._write("dropped:TK2345\tbacklog:48\tnope\n")
        with self.assertRaises(D.HarnessFailure):
            D.load_allowlist(path, validate=False)

    def test_rejects_a_rule_that_cancels_nothing(self):
        path = self._write("tk[]~ts[]\tbacklog:48\tnope\n")
        with self.assertRaises(D.HarnessFailure):
            D.load_allowlist(path, validate=False)

    def test_rejects_a_duplicate_rule(self):
        path = self._write("tk[]~ts[7006]\tbacklog:48\ta\ntk[]~ts[7006]\tbacklog:48\tb\n")
        with self.assertRaises(D.HarnessFailure):
            D.load_allowlist(path, validate=False)

    def test_rejects_an_unowned_entry(self):
        path = self._write("tk[]~ts[7006]\tbecause-i-said-so\tnope\n")
        with self.assertRaises(D.HarnessFailure):
            D.load_allowlist(path)

    def test_rejects_a_dead_ledger_id(self):
        path = self._write("tk[]~ts[7006]\tdiv:no/such/entry\tnope\n")
        with self.assertRaises(D.HarnessFailure):
            D.load_allowlist(path)

    def test_rejects_a_dead_backlog_owner(self):
        path = self._write("tk[]~ts[7006]\tbacklog:99999\tnope\n")
        with self.assertRaises(D.HarnessFailure):
            D.load_allowlist(path)

    def test_the_committed_allowlist_validates(self):
        """Every committed rule names a live ledger id or backlog item."""
        D.load_allowlist()


class TestScoreboard(unittest.TestCase):
    def test_round_trip(self):
        rows = [D.Row("repros/a.ts", "MATCH", ("3:2345",), ("3:2345",), ()),
                D.Row("repros/b.ts", "DIVERGE", ("4:2345",), ("4:2322", "4:2322"),
                      ("tk[]!=ts[2322]",))]
        fd, path = tempfile.mkstemp(suffix=".txt")
        os.close(fd)
        D.write_scoreboard(rows, "6.0.3", path)
        back, meta = D.read_scoreboard(path)
        self.assertEqual(set(back), {"repros/a.ts", "repros/b.ts"})
        self.assertEqual(back["repros/b.ts"].ts_ids, ("4:2322", "4:2322"))
        self.assertEqual(back["repros/b.ts"].tokens, ("tk[]!=ts[2322]",))
        self.assertTrue(meta["tsc"].startswith("6.0.3"))

    def test_status_is_recomputed_from_frozen_ids(self):
        """`repros --check` re-derives the status without tsc, from the committed
        baseline — that is what keeps the CI gate free of a tsc install."""
        residue, hits = D.compare_ids(("3:2345",), ("3:2322",),
                                      [rule("tk[2345]~ts[2322]")])
        self.assertEqual((residue, D.classify(residue, hits)), ([], "ALLOWED"))

    def test_the_committed_scoreboard_is_readable_and_complete(self):
        rows, meta = D.read_scoreboard()
        self.assertIn("format", meta)
        on_disk = {f"repros/{f}" for f in D.repro_files()}
        self.assertEqual(set(rows), on_disk,
                         "scoreboard.txt and repros/ disagree — run `repros --save`")


class TestGenerator(unittest.TestCase):
    def test_is_deterministic(self):
        a = grammar.generate(42, 7).render()
        b = grammar.generate(42, 7).render()
        self.assertEqual(a, b)

    def test_different_indices_differ(self):
        self.assertNotEqual(grammar.generate(42, 7).render(),
                            grammar.generate(42, 8).render())

    def test_names_are_namespaced_for_batching(self):
        """A whole batch goes to one tsc process, so top-level names must not collide
        across programs."""
        src = grammar.generate(3, 11).render()
        for line in src.splitlines():
            if line.startswith("declare function "):
                self.assertTrue(line[len("declare function "):].startswith("g3_11_"), line)

    def test_reaches_the_trigger_shape(self):
        """The whole point of the grammar (backlog 96): an argument a contextual
        re-walk can supersede — an arrow or a fresh object/array literal — nested
        inside a contextually typed callback, whose value depends on that callback's
        contextually typed parameter. If this stops holding, the harness is blind
        again and every "zero diff" it reports is worthless."""
        hits = sum(1 for i in range(60) if _has_trigger(grammar.generate(99, i).body, frozenset()))
        self.assertGreater(hits, 42, "the grammar stopped generating the trigger shape")

    def test_generic_arrow_parameters_are_parenthesized(self):
        """`<U,>q => …` is a syntax error; `<U,>(q) => …` is not."""
        for seed in range(6):
            for i in range(60):
                src = grammar.generate(seed, i).render()
                for line in src.splitlines():
                    if "<U,>" in line:
                        self.assertNotRegex(line, r"<U,>[A-Za-z_]")


class TestShrinker(unittest.TestCase):
    def test_reduces_to_the_statement_the_oracle_cares_about(self):
        prog = grammar.generate(5, 3)
        needle = "MARKER"
        prog.decls.append(grammar.FuncDecl(
            needle, [grammar.Sig((), (("x", grammar.NUMBER),), grammar.VOID)]))
        prog.body.stmts.append(grammar.Call(needle, [grammar.Raw("1")]))
        best, _calls = shrink_program(prog, lambda src: needle in src)
        self.assertIn(needle, best.render())
        self.assertLess(len(best.render()), len(prog.render()))

    def test_never_accepts_a_program_the_oracle_rejects(self):
        prog = grammar.generate(5, 4)
        best, _ = shrink_program(prog, lambda src: False)
        self.assertEqual(best.render(), grammar.prune_decls(prog).render())

    def test_prunes_declarations_the_body_stopped_calling(self):
        prog = grammar.generate(5, 5)
        prog.decls.append(grammar.FuncDecl(
            "unused_decl", [grammar.Sig((), (), grammar.VOID)]))
        self.assertNotIn("unused_decl", grammar.prune_decls(prog).render())

    def test_drops_the_class_wrapper_only_when_this_is_unused(self):
        prog = grammar.generate(5, 6)
        prog.class_name = "C"
        prog.body.stmts = [grammar.Call("f", [grammar.Raw("this.fld")])]
        self.assertIsNone(grammar.drop_class(prog))
        prog.body.stmts = [grammar.Call("f", [grammar.Raw("1")])]
        self.assertIsNotNone(grammar.drop_class(prog))

    def test_text_shrinking_is_oracle_guarded(self):
        source = "// differential: seed=1 index=2 depth=1\ndeclare function g1_2_f(): void;\ng1_2_f();\n"
        out = shrink_text(source, lambda src: "f()" in src, prefix="g1_2_")
        self.assertNotIn("g1_2_", out)
        self.assertNotIn("// differential:", out)

    def test_text_shrinking_keeps_lines_the_oracle_needs(self):
        source = "keep me\ndrop me\n"
        self.assertIn("keep me", shrink_text(source, lambda src: "keep me" in src))


class TestReproCorpus(unittest.TestCase):
    def test_every_repro_carries_a_provenance_header(self):
        for name in D.repro_files():
            with open(os.path.join(D.REPROS, name), encoding="utf-8") as fh:
                first = fh.readline()
            self.assertTrue(first.startswith("//"),
                            f"{name}: a repro must open with a comment saying where it came from")

    def test_repro_corpus_is_not_empty(self):
        self.assertTrue(D.repro_files())


if __name__ == "__main__":
    unittest.main()

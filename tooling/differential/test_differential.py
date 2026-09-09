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
import hashlib
import re
import tempfile
import unittest
from collections import Counter

import differential as D
import grammar
from shrink import shrink_program, shrink_text


class TestPublicationTransitions(unittest.TestCase):
    def test_role_profile_composes_grouping_context_and_copy_writes(self):
        options = grammar.GenOptions(publication_transitions=True)
        for seed in (1, 2, 3):
            for index in range(24):
                program = grammar.generate(seed, index, options)
                text = program.render()
                self.assertEqual(text, grammar.generate(seed, index, options).render())
                self.assertIn(" satisfies ", text)
                self.assertIn("Copy = ", text)
                nodes = [node for _, node in grammar.slots(program)]
                self.assertEqual(sum(isinstance(n, grammar.Conditional) for n in nodes), 5)
                self.assertEqual(sum(isinstance(n, grammar.Satisfies) for n in nodes), 6)
                self.assertEqual(sum(isinstance(n, grammar.Binding) for n in nodes), 12)
                self.assertEqual(sum(isinstance(n, grammar.Arrow) for n in nodes), 2)
                for node in nodes:
                    if isinstance(node, grammar.Conditional):
                        self.assertEqual(grammar.simplifications(node), [node.yes, node.no])

    def test_original_three_seed_stream_remains_frozen(self):
        digest = hashlib.sha256()
        for seed in (1, 2, 3):
            for index in range(400):
                digest.update(grammar.generate(seed, index).render().encode())
        self.assertEqual(digest.hexdigest(), "94daa7207f6ad72c1230130286d7a62e3a0baaf876445fda79d5cbeee2bfe665")


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

    def test_incomplete_candidate_is_a_finding(self):
        tk = outcome([diag(3, 2322)], exit_code=3,
                     incompletes=("bind/binding-pattern/object-pattern",))
        ts = outcome([diag(3, 2322)])
        residue, hits = D.compare_tsc(tk, ts)
        self.assertEqual(
            residue,
            ["incomplete-new:bind/binding-pattern/object-pattern"],
        )
        self.assertEqual(hits, Counter())

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

    def test_indented_output_without_a_primary_diagnostic_is_a_hard_failure(self):
        tmp, path = self._fake_bin("  internal failure\n", 0)
        with self.assertRaises(D.HarnessFailure):
            D.run_typokat(path, tmp, "probe.ts")

    def test_tsc_indented_output_without_a_primary_diagnostic_is_a_hard_failure(self):
        tmp, path = self._fake_bin("  internal failure\n", 0)
        with self.assertRaises(D.HarnessFailure):
            D.run_tsc(path, tmp, ["probe.ts"])

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


class TestBindingMatrix(unittest.TestCase):
    EXPECTED = (
        (
            "bm0_shorthand_rename.ts",
            "// binding-matrix: shorthand-rename\n"
            "const bm0_source: { bm0_count: number; label: string } = "
            "{ bm0_count: 1, label: \"ready\" };\n"
            "const { bm0_count, label: bm0_label } = bm0_source;\n"
            "const bm0_count_good: number = bm0_count;\n"
            "const bm0_label_good: string = bm0_label;\n"
            "const bm0_count_wrong: string = bm0_count;\n",
        ),
        (
            "bm1_static_keys.ts",
            "// binding-matrix: static-keys\n"
            "const bm1_source: { \"string-key\": string; 7: boolean } = "
            "{ \"string-key\": \"ready\", 7: true };\n"
            "const { \"string-key\": bm1_text, 7: bm1_flag } = bm1_source;\n"
            "const bm1_text_good: string = bm1_text;\n"
            "const bm1_flag_good: boolean = bm1_flag;\n"
            "const bm1_text_wrong: number = bm1_text;\n",
        ),
        (
            "bm2_let_independence.ts",
            "// binding-matrix: let-independence\n"
            "const bm2_source = { count: 1, label: \"ready\" };\n"
            "let { count: bm2_count, label: bm2_label } = bm2_source;\n"
            "bm2_count = 2;\n"
            "bm2_label = \"changed\";\n"
            "const bm2_count_good: number = bm2_count;\n"
            "const bm2_label_good: string = bm2_label;\n"
            "const bm2_count_wrong: string = bm2_count;\n",
        ),
        (
            "bm3_var_pre_source.ts",
            "// binding-matrix: var-pre-source\n"
            "function bm3_owner(): void {\n"
            "  const bm3_before: number = bm3_leaf;\n"
            "  var { bm3_leaf } = { bm3_leaf: 1 };\n"
            "  const bm3_leaf_good: number = bm3_leaf;\n"
            "  const bm3_leaf_wrong: string = bm3_leaf;\n"
            "}\n",
        ),
        (
            "bm4_optional_default.ts",
            "// binding-matrix: optional-default\n"
            "const bm4_source: { maybe?: number; label?: string } = {};\n"
            "const { maybe: bm4_maybe, label: bm4_label = \"fallback\" } = bm4_source;\n"
            "const bm4_maybe_good: number | undefined = bm4_maybe;\n"
            "const bm4_label_good: string = bm4_label;\n"
            "const bm4_maybe_wrong: number = bm4_maybe;\n",
        ),
        (
            "bm5_annotation_precedence.ts",
            "// binding-matrix: annotation-precedence\n"
            "const { value: bm5_value }: { value: number } = { value: 1 };\n"
            "const bm5_value_good: number = bm5_value;\n"
            "const bm5_value_wrong: 1 = bm5_value;\n",
        ),
        (
            "bm6_default_diagnostics.ts",
            "// binding-matrix: default-diagnostics\n"
            "declare function bm6_needNumber(value: number): number;\n"
            "const {\n"
            "  direct: bm6_direct = \"bad\",\n"
            "  callback: bm6_callback = () => bm6_needNumber(\"bad\"),\n"
            "}: { direct?: number; callback?: () => number } = {};\n"
            "const bm6_direct_good: number = bm6_direct;\n"
            "const bm6_callback_good: () => number = bm6_callback;\n"
            "const bm6_direct_wrong: string = bm6_direct;\n",
        ),
        (
            "bm7_missing_shadow.ts",
            "// binding-matrix: missing-shadow\n"
            "declare function bm7_needNumber(value: number): number;\n"
            "declare const bm7_source: { present: number };\n"
            "let bm7_shadow = \"outer\";\n"
            "{\n"
            "  const { missing: bm7_shadow = () => bm7_needNumber(\"bad\") } = bm7_source;\n"
            "  const bm7_inner_good: () => number = bm7_shadow;\n"
            "  const bm7_inner_wrong: number = bm7_shadow;\n"
            "}\n"
            "const bm7_outer_good: string = bm7_shadow;\n"
            "const bm7_outer_wrong: number = bm7_shadow;\n",
        ),
    )

    def test_exact_deterministic_matrix(self):
        first = tuple(D.binding_matrix_programs())
        second = tuple(D.binding_matrix_programs())
        self.assertEqual(first, second)
        self.assertEqual(first, self.EXPECTED)

    def test_every_program_is_namespaced_and_has_a_binding_oracle(self):
        matrix = tuple(D.binding_matrix_programs())
        self.assertEqual(len(matrix), 8)
        for index, (filename, source) in enumerate(matrix):
            prefix = f"bm{index}_"
            self.assertTrue(filename.startswith(prefix), filename)
            identifiers = re.findall(r"\bbm\d+_[A-Za-z_]\w*", source)
            self.assertTrue(identifiers, filename)
            self.assertTrue(all(name.startswith(prefix) for name in identifiers), filename)
            self.assertRegex(source, r"\b(?:const|let|var)\s*\{")
            self.assertIn("_good", source)
            self.assertIn("_wrong", source)
            self.assertNotIn("...", source)
            self.assertNotIn("incomplete[", source)
            self.assertNotIn("TK2448", source)

    def test_matrix_pins_all_eight_admitted_shapes(self):
        sources = dict(D.binding_matrix_programs())
        self.assertIn("const { bm0_count, label: bm0_label }", sources["bm0_shorthand_rename.ts"])
        self.assertIn("\"string-key\": bm1_text, 7: bm1_flag", sources["bm1_static_keys.ts"])
        self.assertIn("let { count: bm2_count, label: bm2_label }", sources["bm2_let_independence.ts"])
        self.assertLess(sources["bm3_var_pre_source.ts"].index("bm3_before"),
                        sources["bm3_var_pre_source.ts"].index("var { bm3_leaf }"))
        self.assertIn("number | undefined", sources["bm4_optional_default.ts"])
        self.assertIn("bm4_label = \"fallback\"", sources["bm4_optional_default.ts"])
        self.assertIn(": { value: number } = { value: 1 }", sources["bm5_annotation_precedence.ts"])
        defaults = sources["bm6_default_diagnostics.ts"]
        self.assertEqual(defaults.count("bm6_direct = \"bad\""), 1)
        self.assertEqual(defaults.count("bm6_needNumber(\"bad\")"), 1)
        missing = sources["bm7_missing_shadow.ts"]
        self.assertEqual(missing.count("missing: bm7_shadow ="), 1)
        self.assertEqual(missing.count("bm7_needNumber(\"bad\")"), 1)
        self.assertIn("{\n  const { missing:", missing)

    def test_cli_exposes_the_matrix_and_reference_falsifier(self):
        args = D.build_parser().parse_args([
            "binding-matrix",
            "--ref", "/tmp/prechange-typokat",
            "--check",
            "--require-reference-divergence",
        ])
        self.assertIs(args.func, D.cmd_binding_matrix)
        self.assertTrue(args.check)
        self.assertTrue(args.require_reference_divergence)

    def test_reference_falsifier_cannot_pass_on_zero_divergence(self):
        self.assertEqual(D.binding_matrix_check_exit_code(
            0, require_reference_divergence=False), 0)
        self.assertEqual(D.binding_matrix_check_exit_code(
            1, require_reference_divergence=False), 1)
        self.assertEqual(D.binding_matrix_check_exit_code(
            0, require_reference_divergence=True), 1)
        self.assertEqual(D.binding_matrix_check_exit_code(
            1, require_reference_divergence=True), 0)


class TestOptionalParameterMatrix(unittest.TestCase):
    EXPECTED = (
        (
            "opm0_fixed_boundaries.ts",
            "// optional-parameter-matrix: fixed-boundaries\n"
            "declare const opm0_maybe: number | undefined;\n"
            "function opm0_optional(value?: number): void {}\n"
            "function opm0_defaulted(value: number = 1): void {}\n"
            "function opm0_required(value: number): void {}\n"
            "opm0_optional(undefined);\n"
            "opm0_optional(opm0_maybe);\n"
            "opm0_defaulted(undefined);\n"
            "opm0_defaulted(opm0_maybe);\n"
            "opm0_optional(\"wrong\");\n"
            "opm0_required(undefined);\n",
        ),
        (
            "opm1_call_consumers.ts",
            "// optional-parameter-matrix: call-consumers\n"
            "class opm1_Box { constructor(value?: number) {} "
            "method(value?: number): void {} }\n"
            "const opm1_instance = new opm1_Box(undefined);\n"
            "opm1_instance.method(undefined);\n"
            "const opm1_function: (value?: number) => void = "
            "(value?: number): void => {};\n"
            "opm1_function(undefined);\n"
            "class opm1_Base { constructor(value?: number) {} }\n"
            "class opm1_Derived extends opm1_Base { constructor() { super(undefined); } }\n"
            "interface opm1_Call { (value?: number): \"called\"; }\n"
            "declare const opm1_call: opm1_Call;\n"
            "const opm1_called: \"called\" = opm1_call(undefined);\n"
            "interface opm1_Construct { "
            "new (value?: number): { kind: \"constructed\" }; }\n"
            "declare const opm1_construct: opm1_Construct;\n"
            "const opm1_constructed: { kind: \"constructed\" } = "
            "new opm1_construct(undefined);\n",
        ),
        (
            "opm2_overload_rest.ts",
            "// optional-parameter-matrix: overload-rest\n"
            "function opm2_select(value?: number): \"optional\";\n"
            "function opm2_select(value: string): \"required\";\n"
            "function opm2_select(value?: number | string): \"optional\" | \"required\" "
            "{ return \"optional\"; }\n"
            "const opm2_selected: \"optional\" = opm2_select(undefined);\n"
            "opm2_select(true);\n"
            "function opm2_before_rest(prefix: string, value?: number, "
            "...tail: boolean[]): void {}\n"
            "opm2_before_rest(\"prefix\", undefined, true);\n"
            "opm2_before_rest(\"prefix\", 1, \"wrong\");\n"
            "opm2_before_rest();\n"
            "declare function opm2_union_rest(value?: number, "
            "...tail: [] | [boolean]): void;\n"
            "opm2_union_rest(undefined);\n"
            "opm2_union_rest(undefined, true);\n"
            "function opm2_default_rest(prefix: string, value: number = 1, "
            "...tail: boolean[]): void {}\n"
            "opm2_default_rest(\"prefix\", undefined, true);\n",
        ),
        (
            "opm3_inference.ts",
            "// optional-parameter-matrix: inference\n"
            "declare function opm3_locked<T>(first: T, second?: T): T;\n"
            "declare const opm3_maybe: number | undefined;\n"
            "const opm3_literal: 1 = opm3_locked(1, undefined);\n"
            "const opm3_number: number = opm3_locked(1, opm3_maybe);\n"
            "declare function opm3_callback<T>(value?: T, "
            "callback?: (value: T) => void): void;\n"
            "opm3_callback(opm3_maybe, (value) => {\n"
            "  const opm3_callback_number: number = value;\n"
            "  const opm3_callback_undefined: undefined = value;\n"
            "});\n"
            "function opm3_explicit<T>(value?: T): T | undefined { return value; }\n"
            "const opm3_explicit_result: number | undefined = "
            "opm3_explicit<number>(undefined);\n"
            "function opm3_inferred<T extends number>(seed: T[], value?: T): T[] "
            "{ return seed; }\n"
            "const opm3_inferred_result: number[] = opm3_inferred([1], undefined);\n",
        ),
        (
            "opm4_contextual.ts",
            "// optional-parameter-matrix: contextual\n"
            "function opm4_object(value?: { tag: \"ok\" }): void {}\n"
            "opm4_object(undefined);\n"
            "opm4_object({ tag: \"bad\" });\n"
            "function opm4_callback(value?: (input: number) => string): void {}\n"
            "opm4_callback(undefined);\n"
            "opm4_callback((input) => input.toString());\n"
            "opm4_callback((input) => true);\n"
            "function opm4_array(value?: { items: number[] }): void {}\n"
            "opm4_array(undefined);\n"
            "opm4_array({ items: [\"bad\"] });\n",
        ),
    )

    def matrix(self):
        self.assertTrue(
            hasattr(D, "optional_parameter_matrix_programs"),
            "optional_parameter_matrix_programs is not implemented",
        )
        return tuple(D.optional_parameter_matrix_programs())

    def test_exact_deterministic_matrix(self):
        first = self.matrix()
        second = self.matrix()
        self.assertEqual(first, second)
        self.assertEqual(first, self.EXPECTED)

    def test_every_program_is_namespaced_and_reaches_the_changed_rule(self):
        matrix = self.matrix()
        self.assertEqual(len(matrix), 5)
        for index, (filename, source) in enumerate(matrix):
            prefix = f"opm{index}_"
            self.assertTrue(filename.startswith(prefix), filename)
            identifiers = re.findall(r"\bopm\d+_[A-Za-z_]\w*", source)
            self.assertTrue(identifiers, filename)
            self.assertTrue(all(name.startswith(prefix) for name in identifiers), filename)
            self.assertIn("undefined", source)
            self.assertRegex(source, r"\w+\?\s*:\s*")
            self.assertNotIn("incomplete[", source)

    def test_matrix_pins_fixed_consumers_overloads_and_rest_controls(self):
        sources = dict(self.matrix())
        fixed = sources["opm0_fixed_boundaries.ts"]
        self.assertIn("value?: number", fixed)
        self.assertIn("value: number = 1", fixed)
        self.assertIn("opm0_optional(opm0_maybe)", fixed)
        self.assertIn("opm0_optional(\"wrong\")", fixed)
        self.assertIn("opm0_required(undefined)", fixed)

        consumers = sources["opm1_call_consumers.ts"]
        self.assertIn("new opm1_Box(undefined)", consumers)
        self.assertIn("opm1_instance.method(undefined)", consumers)
        self.assertIn("opm1_function(undefined)", consumers)
        self.assertIn("super(undefined)", consumers)
        self.assertIn("opm1_call(undefined)", consumers)
        self.assertIn("new opm1_construct(undefined)", consumers)

        rest = sources["opm2_overload_rest.ts"]
        self.assertIn("opm2_select(undefined)", rest)
        self.assertIn("opm2_select(true)", rest)
        self.assertIn("value?: number, ...tail: boolean[]", rest)
        self.assertIn("...tail: [] | [boolean]", rest)
        self.assertIn("value: number = 1, ...tail: boolean[]", rest)
        self.assertIn("opm2_before_rest(\"prefix\", 1, \"wrong\")", rest)
        self.assertIn("opm2_before_rest()", rest)

    def test_matrix_pins_inference_and_contextual_non_widening(self):
        sources = dict(self.matrix())
        inference = sources["opm3_inference.ts"]
        self.assertIn("opm3_locked(1, undefined)", inference)
        self.assertIn("opm3_locked(1, opm3_maybe)", inference)
        self.assertIn("opm3_callback_number: number = value", inference)
        self.assertIn("opm3_callback_undefined: undefined = value", inference)
        self.assertIn("opm3_explicit<number>(undefined)", inference)
        self.assertIn("opm3_inferred([1], undefined)", inference)

        contextual = sources["opm4_contextual.ts"]
        self.assertIn("opm4_object(undefined)", contextual)
        self.assertIn("opm4_object({ tag: \"bad\" })", contextual)
        self.assertIn("opm4_callback((input) => input.toString())", contextual)
        self.assertIn("opm4_callback((input) => true)", contextual)
        self.assertIn("opm4_array(undefined)", contextual)
        self.assertIn("opm4_array({ items: [\"bad\"] })", contextual)

    def test_cli_exposes_the_matrix_and_reference_falsifier(self):
        parser = D.build_parser()
        subcommands = next(action for action in parser._actions if action.dest == "cmd").choices
        self.assertIn(
            "optional-parameter-matrix",
            subcommands,
            "optional-parameter-matrix CLI is not implemented",
        )
        args = parser.parse_args([
            "optional-parameter-matrix",
            "--ref", "/tmp/prechange-typokat",
            "--check",
            "--require-reference-divergence",
        ])
        self.assertIs(args.func, D.cmd_optional_parameter_matrix)
        self.assertTrue(args.check)
        self.assertTrue(args.require_reference_divergence)


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

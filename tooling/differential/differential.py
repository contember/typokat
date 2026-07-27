#!/usr/bin/env python3
"""
typokat randomized differential harness — black-box, seeded, self-shrinking.

Generates deep compositions of contextually typed calls (see `grammar.py`), runs two
checkers over each program, and reports every place they disagree. Like
`tooling/official-suite/`, it never imports or builds the checker crate: it shells out
to a prebuilt binary and parses the rendered diagnostics, so a work-in-progress `src/`
does not block it.

Two reference modes — the reason the harness exists twice over:

  --ref <path-to-typokat-binary>   Regression mode. ANY difference between the two
                                   binaries is a finding. This is what a work unit
                                   runs before claiming "zero diff": build the
                                   pre-change binary, point `--ref` at it. It is the
                                   mode that reproduces 412f321.

  --ref tsc                        Truth mode. Compares against real `tsc --strict`.
                                   typokat diverges from tsc in documented ways, so
                                   divergence *shapes* are suppressible through the
                                   committed `allowlist.txt` — each entry naming a
                                   ledger id, and matching one exact shape only.

  --ref none                       Sanity mode. One binary, no comparison: asserts the
                                   generator's programs parse, never crash the
                                   checker, and never produce output inconsistent with
                                   the exit code. Deterministic and cheap — the CI gate.

Findings are shrunk automatically (structurally, on the generated tree) and written
out as minimal `.ts` repros, because "seed 42 file 317" is not an actionable report.
Adopted repros live in `repros/` with their behaviour pinned in `scoreboard.txt`.

See README.md for the design and the workflow.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from collections import Counter, OrderedDict, defaultdict
from dataclasses import dataclass
from typing import Dict, List, Optional, Sequence, Tuple

from grammar import GenOptions, Program, generate
from shrink import shrink_program, shrink_text

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
REPROS = os.path.join(HERE, "repros")
REPORT = os.path.join(HERE, "report")
WORK = os.path.join(HERE, "work")
SCOREBOARD = os.path.join(HERE, "scoreboard.txt")
ALLOWLIST = os.path.join(HERE, "allowlist.txt")
LEDGER = os.path.join(ROOT, "docs", "reference", "divergences.md")
BACKLOG = os.path.join(ROOT, "docs", "backlog")

DEFAULT_BIN = os.path.join(ROOT, "target", "release", "typokat")
# The tsc the checker is validated against (README, official-suite PINNED_SHA).
EXPECTED_TSC_VERSION = "6.0.3"
TSC_FLAGS = ["--strict", "--noEmit", "--pretty", "false", "--target", "es2020"]
SCOREBOARD_FORMAT = "1"

# Exit codes the typokat CLI documents: 0 clean, 1 diagnostics, 3 incomplete surface.
OK_EXIT_CODES = (0, 1, 3)
# "cannot find name" and "property does not exist" (both checkers use tsc's numbers).
# The generator declares everything it references, so these can only appear *during
# shrinking*, when a reduction has deleted a declaration (or a class field) the body
# still uses. Such a candidate usually still satisfies the oracle — both checkers
# report the dangling reference identically, so the target signature survives — but it
# is a bad repro: it demonstrates a hole rather than the shape. The oracle rejects it.
DANGLING_REFERENCE_CODES = (2304, 2339)


class HarnessFailure(Exception):
    """An invocation the harness cannot trust: a crash, a timeout, an undocumented
    exit code, or output inconsistent with that exit code. Always fatal — scoring a
    crash as "no diagnostics" would manufacture exactly the false negative this
    harness exists to catch."""


# --- checker invocation ------------------------------------------------------

TK_LINE_RE = re.compile(r"^(?P<file>[^(]+)\((?P<line>\d+),(?P<col>\d+)\): error TK(?P<code>\d+): (?P<msg>.*)$")
TK_INC_RE = re.compile(r"^(?P<file>[^(]+)\((?P<line>\d+),(?P<col>\d+)\): incomplete\[(?P<id>[^\]]+)\]:")
TK_ERR_RE = re.compile(r"^error: (?P<msg>.*)$")
TSC_LINE_RE = re.compile(r"^(?P<file>[^(]+)\((?P<line>\d+),(?P<col>\d+)\): error TS(?P<code>\d+): (?P<msg>.*)$")


@dataclass(frozen=True)
class Diag:
    line: int
    col: int
    code: int
    message: str
    detail: Tuple[str, ...] = ()

    def ident(self) -> str:
        return f"{self.line}:{self.code}"

    def full(self) -> Tuple:
        return (self.line, self.col, self.code, self.message, self.detail)


@dataclass
class Outcome:
    """What one checker said about one file."""

    exit_code: int
    diags: Tuple[Diag, ...]
    incompletes: Tuple[str, ...]
    raw: str

    def line_codes(self) -> Dict[int, Counter]:
        out: Dict[int, Counter] = defaultdict(Counter)
        for d in self.diags:
            out[d.line][d.code] += 1
        return out


def run_typokat(binary: str, cwd: str, filename: str, timeout: int = 60,
                strict: bool = True) -> Outcome:
    """Run `typokat check --format compact <filename>` with `cwd` as the working
    directory, so the rendered path is exactly `filename` and two binaries produce
    byte-comparable output.

    With `strict`, an untrusted invocation raises `HarnessFailure`. The shrinker turns
    it off: a reduction that happens to produce unparseable source is simply "did not
    reproduce", not a harness abort."""
    try:
        proc = subprocess.run([binary, "check", "--format", "compact", filename],
                              cwd=cwd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise HarnessFailure(f"{filename}: {binary} timed out after {timeout}s")
    out = proc.stderr + proc.stdout
    diags: List[Diag] = []
    incompletes: List[str] = []
    parse_errors: List[str] = []
    for raw in out.split("\n"):
        if not raw:
            continue
        m = TK_LINE_RE.match(raw)
        if m:
            diags.append(Diag(int(m["line"]), int(m["col"]), int(m["code"]), m["msg"]))
            continue
        mi = TK_INC_RE.match(raw)
        if mi:
            incompletes.append(mi["id"])
            continue
        me = TK_ERR_RE.match(raw)
        if me:
            parse_errors.append(me["msg"])
            continue
        if raw.startswith(" ") or raw.startswith("\t"):
            # A reason-chain continuation line: attach it to the diagnostic above.
            if diags:
                last = diags[-1]
                diags[-1] = Diag(last.line, last.col, last.code, last.message,
                                 last.detail + (raw.strip(),))
            continue
        if strict:
            raise HarnessFailure(
                f"{filename}: unparseable output line from {binary}: {raw!r}\n"
                f"--- captured output ---\n{out}")

    if not strict:
        if parse_errors or proc.returncode not in OK_EXIT_CODES:
            return Outcome(-1, (), (), out)
        return Outcome(proc.returncode, tuple(diags), tuple(incompletes), out)

    if proc.returncode not in OK_EXIT_CODES:
        raise HarnessFailure(
            f"{filename}: {binary} exited with unexpected code {proc.returncode} "
            f"(expected 0, 1 or 3; negative = killed by a signal)\n"
            f"--- captured output ---\n{out}")
    if parse_errors:
        # The grammar only emits well-formed TypeScript, so this is a generator bug
        # (or a parser regression) — either way it must not be scored as a data point.
        raise HarnessFailure(
            f"{filename}: {binary} reported a parse error on generated source: "
            f"{parse_errors[0]}\n--- captured output ---\n{out}")
    if proc.returncode == 0 and (diags or incompletes):
        raise HarnessFailure(
            f"{filename}: {binary} exited 0 but printed diagnostics/incomplete records\n"
            f"--- captured output ---\n{out}")
    if proc.returncode == 1 and not diags:
        raise HarnessFailure(
            f"{filename}: {binary} exited 1 but no diagnostic could be parsed\n"
            f"--- captured output ---\n{out}")
    if proc.returncode == 1 and incompletes:
        raise HarnessFailure(
            f"{filename}: {binary} exited 1 with an incomplete record "
            f"(an incomplete surface must force exit 3)\n--- captured output ---\n{out}")
    if proc.returncode == 3 and not incompletes:
        raise HarnessFailure(
            f"{filename}: {binary} exited 3 but no incomplete record could be parsed\n"
            f"--- captured output ---\n{out}")
    return Outcome(proc.returncode, tuple(diags), tuple(incompletes), out)


def tsc_version(tsc: str) -> str:
    try:
        proc = subprocess.run([tsc, "--version"], capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise HarnessFailure(f"cannot run tsc at {tsc}: {exc}")
    m = re.search(r"(\d+\.\d+\.\d+)", proc.stdout)
    if not m:
        raise HarnessFailure(f"cannot parse `tsc --version` output: {proc.stdout!r}")
    return m.group(1)


def run_tsc(tsc: str, cwd: str, filenames: Sequence[str],
            timeout: int = 900) -> Dict[str, Outcome]:
    """Check a whole batch in ONE tsc process — tsc costs ~1 s of start-up per
    invocation, which would dominate the run. Generated programs prefix every
    top-level name with their seed/index, so a batch shares the global scope without
    colliding. Returns one Outcome per requested file (clean files included)."""
    if not filenames:
        return {}
    cmd = [tsc] + TSC_FLAGS + list(filenames)
    try:
        proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise HarnessFailure(f"tsc timed out after {timeout}s on {len(filenames)} files")
    if proc.returncode not in (0, 1, 2):
        raise HarnessFailure(
            f"tsc exited {proc.returncode}\n--- captured output ---\n"
            f"{proc.stdout}\n{proc.stderr}")
    per_file: Dict[str, List[Diag]] = {f: [] for f in filenames}
    for raw in (proc.stdout + proc.stderr).split("\n"):
        if not raw.strip():
            continue
        m = TSC_LINE_RE.match(raw)
        if m:
            f = m["file"]
            if f not in per_file:
                raise HarnessFailure(f"tsc reported on an unexpected file {f!r}: {raw}")
            per_file[f].append(Diag(int(m["line"]), int(m["col"]), int(m["code"]), m["msg"]))
            continue
        if raw.startswith(" ") or raw.startswith("\t"):
            continue  # elaboration continuation
        raise HarnessFailure(f"unparseable tsc output line: {raw!r}")
    return {f: Outcome(0 if not ds else 1, tuple(ds), (), "") for f, ds in per_file.items()}


# --- comparison --------------------------------------------------------------


def compare_binaries(ref: Outcome, cand: Outcome) -> List[str]:
    """Signature tokens for a typokat-vs-typokat comparison. Empty list == identical.

    The comparison is on the FULL diagnostic — line, column, code, message and reason
    chain — because 412f321's symptoms included "rendered wrong types in messages",
    not only dropped and invented diagnostics."""
    tokens: List[str] = []
    if ref.exit_code != cand.exit_code:
        tokens.append(f"exit:{ref.exit_code}->{cand.exit_code}")
    rc = Counter(d.full() for d in ref.diags)
    cc = Counter(d.full() for d in cand.diags)
    dropped = list((rc - cc).elements())
    invented = list((cc - rc).elements())
    # A same-(line,code) pair that differs only in text/column is a *message* change,
    # not a dropped-plus-invented pair — classifying it precisely keeps the report
    # readable and keeps the shrinker on the same bug.
    d_keys = Counter((d[0], d[2]) for d in dropped)
    i_keys = Counter((d[0], d[2]) for d in invented)
    both = d_keys & i_keys
    for (_line, code), n in sorted(both.items()):
        tokens.extend([f"message:TK{code}"] * n)
    for (_line, code), n in sorted((d_keys - both).items()):
        tokens.extend([f"dropped:TK{code}"] * n)
    for (_line, code), n in sorted((i_keys - both).items()):
        tokens.extend([f"invented:TK{code}"] * n)
    ri, ci = Counter(ref.incompletes), Counter(cand.incompletes)
    for i, n in sorted((ri - ci).items()):
        tokens.extend([f"incomplete-gone:{i}"] * n)
    for i, n in sorted((ci - ri).items()):
        tokens.extend([f"incomplete-new:{i}"] * n)
    return sorted(tokens)


def compare_tsc(tk: Outcome, ts: Outcome,
                rules: Sequence["AllowEntry"] = ()) -> Tuple[List[str], Counter]:
    """Compare typokat against tsc. Returns `(residue_tokens, allowlist_hits)`;
    an empty residue and no hits means the two agree.

    Granularity is (line, code-number) — the same robust contract the official-suite
    harness uses, since columns and message wording diverge by design. Codes map by
    number (`TK2322` == `TS2322`).

    Each allowlist entry is a **cancellation rule** applied per line: it removes one
    specific `(typokat code, tsc code)` pair from that line's difference. Whatever
    survives cancellation is the residue and is reported. Rules cancel rather than
    label whole lines, so a line carrying a known divergence *plus* a new one still
    reports the new one — the allowlist can suppress a documented shape but can never
    swallow the divergence sitting next to it."""
    a, b = tk.line_codes(), ts.line_codes()
    paired = [r for r in rules if r.tk is not None and r.ts is not None]
    single = [r for r in rules if (r.tk is None) != (r.ts is None)]
    residue: List[str] = []
    hits: Counter = Counter()
    for line in sorted(set(a) | set(b)):
        only_tk = a[line] - b[line]
        only_ts = b[line] - a[line]
        # Paired rules first: a one-sided rule must not eat a code a paired rule needs.
        for r in paired:
            n = min(only_tk[r.tk], only_ts[r.ts])
            if n:
                only_tk[r.tk] -= n
                only_ts[r.ts] -= n
                hits[r.token] += n
        for r in single:
            if r.tk is not None and only_tk[r.tk]:
                hits[r.token] += only_tk[r.tk]
                only_tk[r.tk] = 0
            if r.ts is not None and only_ts[r.ts]:
                hits[r.token] += only_ts[r.ts]
                only_ts[r.ts] = 0
        only_tk, only_ts = +only_tk, +only_ts  # drop the zeroed entries
        if only_tk or only_ts:
            left = ",".join(str(c) for c in sorted(only_tk.elements()))
            right = ",".join(str(c) for c in sorted(only_ts.elements()))
            residue.append(f"tk[{left}]!=ts[{right}]")
    return sorted(residue), hits


# --- allowlist ---------------------------------------------------------------


RULE_RE = re.compile(r"^tk\[(\d*)\]~ts\[(\d*)\]$")


@dataclass
class AllowEntry:
    """One cancellation rule: "a `TK<tk>` here cancels a `TS<ts>` there". Either side
    may be empty — `tk[]~ts[7006]` is "tsc reports 7006 and typokat is silent"."""

    token: str
    tk: Optional[int]
    ts: Optional[int]
    ref: str
    reason: str


def load_allowlist(path: str = ALLOWLIST, validate: bool = True) -> "OrderedDict[str, AllowEntry]":
    """Read `allowlist.txt`. Each line is `<rule>\\t<ref>\\t<reason>`.

    `<ref>` must be either `div:<id>` naming a marker in
    `docs/reference/divergences.md`, or `backlog:NN` naming a live backlog item. That
    validation is what stops the allowlist becoming a junk drawer: an entry cannot
    exist without a documented reason someone owns. Each rule cancels exactly ONE
    code pair, so a divergence shape one code away from an allowlisted one is still
    reported."""
    entries: "OrderedDict[str, AllowEntry]" = OrderedDict()
    if not os.path.exists(path):
        return entries
    with open(path, encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                raise HarnessFailure(
                    f"{path}:{lineno}: expected `<rule>\\t<ref>\\t<reason>`, got {line!r}")
            token, ref, reason = parts[0].strip(), parts[1].strip(), "\t".join(parts[2:]).strip()
            m = RULE_RE.match(token)
            if not m:
                raise HarnessFailure(
                    f"{path}:{lineno}: malformed rule {token!r} "
                    f"(expected `tk[<code>]~ts[<code>]`, either side may be empty)")
            tk = int(m.group(1)) if m.group(1) else None
            ts = int(m.group(2)) if m.group(2) else None
            if tk is None and ts is None:
                raise HarnessFailure(f"{path}:{lineno}: rule {token!r} cancels nothing")
            if token in entries:
                raise HarnessFailure(f"{path}:{lineno}: duplicate allowlist rule {token!r}")
            if validate:
                _validate_allow_ref(path, lineno, ref)
            entries[token] = AllowEntry(token, tk, ts, ref, reason)
    return entries


def _validate_allow_ref(path: str, lineno: int, ref: str) -> None:
    if ref.startswith("div:"):
        ident = ref[4:]
        try:
            with open(LEDGER, encoding="utf-8") as fh:
                ledger = fh.read()
        except OSError as exc:
            raise HarnessFailure(f"{path}:{lineno}: cannot read the divergence ledger: {exc}")
        if not re.search(r"id=" + re.escape(ident) + r"\b", ledger):
            raise HarnessFailure(
                f"{path}:{lineno}: `{ref}` names no `div: id={ident}` marker in "
                f"docs/reference/divergences.md")
        return
    if ref.startswith("backlog:"):
        num = ref[len("backlog:"):]
        if not re.fullmatch(r"\d+", num):
            raise HarnessFailure(f"{path}:{lineno}: malformed backlog reference {ref!r}")
        matches = [f for f in os.listdir(BACKLOG) if f.startswith(f"{num}-")]
        if not matches:
            raise HarnessFailure(
                f"{path}:{lineno}: `{ref}` names no live item in docs/backlog/")
        return
    raise HarnessFailure(
        f"{path}:{lineno}: reference must be `div:<ledger-id>` or `backlog:<NN>`, got {ref!r}")


def classify(residue: Sequence[str], hits: Counter) -> str:
    """MATCH = the two agree. ALLOWED = every difference was cancelled by a committed
    rule. DIVERGE = something survived cancellation — the only status that gates."""
    if residue:
        return "DIVERGE"
    return "ALLOWED" if hits else "MATCH"


# --- the run -----------------------------------------------------------------


@dataclass
class Finding:
    seed: int
    index: int
    tokens: Tuple[str, ...]
    status: str
    source: str
    ref_raw: str
    cand_raw: str


def _prepare_dir(path: str, clean: bool = True) -> str:
    if clean and os.path.isdir(path):
        shutil.rmtree(path)
    os.makedirs(path, exist_ok=True)
    return path


def cmd_fuzz(args) -> int:
    opts = GenOptions(depth_min=args.depth_min, depth_max=args.depth_max)
    allow = load_allowlist() if args.ref == "tsc" else OrderedDict()
    if args.ref not in ("tsc", "none") and not os.path.exists(args.ref):
        print(f"error: reference binary {args.ref} does not exist", file=sys.stderr)
        return 2
    if not os.path.exists(args.bin):
        print(f"error: candidate binary {args.bin} does not exist "
              f"(build it: cargo build --release)", file=sys.stderr)
        return 2

    # Fail fast on a wrong tsc, before spending the run on the candidate side.
    ts_version = None
    if args.ref == "tsc":
        ts_version = tsc_version(args.tsc)
        if ts_version != EXPECTED_TSC_VERSION and not args.allow_tsc_version:
            print(f"error: tsc {ts_version} != pinned {EXPECTED_TSC_VERSION} "
                  f"(pass --allow-tsc-version to override)", file=sys.stderr)
            return 2

    work = _prepare_dir(os.path.join(WORK, f"seed{args.seed}"))
    started = time.time()
    deadline = started + args.time_budget if args.time_budget else None

    # 1. Generate + render every program up front (cheap, deterministic).
    programs: List[Tuple[int, Program, str]] = []
    for i in range(args.count):
        prog = generate(args.seed, i, opts)
        name = f"g{args.seed}_{i}.ts"
        with open(os.path.join(work, name), "w", encoding="utf-8") as fh:
            fh.write(prog.render())
        programs.append((i, prog, name))

    # 2. Candidate side, one process per file (~4 ms each).
    cand: Dict[str, Outcome] = {}
    ran = 0
    for _i, _p, name in programs:
        if deadline and time.time() > deadline:
            break
        cand[name] = run_typokat(args.bin, work, name)
        ran += 1
    truncated = ran < len(programs)
    live = programs[:ran]

    # 3. Reference side.
    ref: Dict[str, Outcome] = {}
    if args.ref == "tsc":
        names = [n for _i, _p, n in live]
        for start in range(0, len(names), args.tsc_batch):
            ref.update(run_tsc(args.tsc, work, names[start:start + args.tsc_batch]))
    elif args.ref != "none":
        for _i, _p, name in live:
            ref[name] = run_typokat(args.ref, work, name)

    # 4. Compare.
    findings: List[Finding] = []
    status_counts: Counter = Counter()
    token_counts: Counter = Counter()
    allowed_hits: Counter = Counter()
    rules = list(allow.values())
    for i, prog, name in live:
        if args.ref == "none":
            status_counts["MATCH"] += 1
            continue
        if args.ref == "tsc":
            tokens, hits = compare_tsc(cand[name], ref[name], rules)
        else:
            tokens, hits = compare_binaries(ref[name], cand[name]), Counter()
        status = classify(tokens, hits)
        status_counts[status] += 1
        token_counts.update(tokens)
        allowed_hits.update(hits)
        if status != "MATCH":
            findings.append(Finding(args.seed, i, tuple(tokens), status, prog.render(),
                                    ref[name].raw, cand[name].raw))

    # 5. Shrink one representative per signature — the reduction is what makes a
    #    report actionable (backlog 96), so it is on by default.
    reported = [f for f in findings if f.status == "DIVERGE"]
    by_sig: "OrderedDict[Tuple[str, ...], List[Finding]]" = OrderedDict()
    for f in reported:
        by_sig.setdefault(f.tokens, []).append(f)
    shrunk: Dict[Tuple[str, ...], Tuple[str, int]] = {}
    if args.shrink:
        _prepare_dir(REPORT, clean=False)
        for sig, group in by_sig.items():
            if deadline and time.time() > deadline:
                break
            f = group[0]
            oracle = make_oracle(args, work, sig, rules)
            prog = generate(args.seed, f.index, opts)
            best, calls = shrink_program(prog, oracle, budget=args.shrink_budget)
            text = shrink_text(best.render(), oracle, prefix=f"g{args.seed}_{f.index}_")
            shrunk[sig] = (text, calls)

    elapsed = time.time() - started
    _print_report(args, live, status_counts, token_counts, allowed_hits, by_sig, shrunk,
                  allow, elapsed, truncated, ts_version)
    _write_json_report(args, status_counts, by_sig, shrunk, elapsed, ts_version)

    if args.adopt:
        _adopt(by_sig, shrunk, args)
    if not args.keep_work:
        shutil.rmtree(work, ignore_errors=True)

    if args.check and status_counts["DIVERGE"]:
        return 1
    return 0


def make_oracle(args, work: str, sig: Tuple[str, ...], rules: Sequence[AllowEntry]):
    """A `source -> bool` predicate: does this source still show *this* divergence?

    Requiring the reduced program's signature to be a non-empty subset of the original
    is standard delta-debugging hygiene — without it a shrink run happily wanders off
    to a different, unrelated disagreement and reports it as the minimal repro of the
    first one."""
    cache: Dict[str, bool] = {}
    target = set(sig)
    counter = [0]

    def oracle(source: str) -> bool:
        if source in cache:
            return cache[source]
        counter[0] += 1
        name = f"shrink_{counter[0]}.ts"
        path = os.path.join(work, name)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(source)
        try:
            cand = run_typokat(args.bin, work, name, strict=False)
            if cand.exit_code < 0 or _unresolved(cand):
                return _cache(cache, source, False)
            if args.ref == "tsc":
                ref = run_tsc(args.tsc, work, [name])[name]
                if _unresolved(ref):
                    return _cache(cache, source, False)
                tokens, _hits = compare_tsc(cand, ref, rules)
            else:
                ref = run_typokat(args.ref, work, name, strict=False)
                if ref.exit_code < 0 or _unresolved(ref):
                    return _cache(cache, source, False)
                tokens = compare_binaries(ref, cand)
            ok = bool(tokens) and set(tokens) <= target
            return _cache(cache, source, ok)
        finally:
            if os.path.exists(path):
                os.unlink(path)

    return oracle


def _unresolved(o: Outcome) -> bool:
    """A repro that references something it does not declare is not a repro."""
    return any(d.code in DANGLING_REFERENCE_CODES for d in o.diags)


def _cache(cache: Dict[str, bool], key: str, value: bool) -> bool:
    cache[key] = value
    return value


def _sig_slug(sig: Sequence[str]) -> str:
    slug = "-".join(sig) if sig else "none"
    slug = re.sub(r"[^A-Za-z0-9]+", "-", slug).strip("-").lower()
    return slug[:60] or "none"


def _print_report(args, live, status_counts, token_counts, allowed_hits, by_sig, shrunk,
                  allow, elapsed, truncated, ts_version) -> None:
    ref_desc = {"tsc": f"tsc {ts_version} --strict", "none": "(sanity only)"}.get(
        args.ref, args.ref)
    print("=" * 78)
    print(f"typokat differential — seed {args.seed}, {len(live)} programs, "
          f"depth {args.depth_min}-{args.depth_max}")
    print(f"  candidate : {args.bin}")
    print(f"  reference : {ref_desc}")
    print(f"  elapsed   : {elapsed:.1f}s" + ("  [TIME BUDGET EXHAUSTED]" if truncated else ""))
    print("=" * 78)
    if args.ref == "none":
        print(f"sanity: {len(live)} programs checked, no crash / parse error / "
              f"inconsistent exit code")
        return
    total = max(1, sum(status_counts.values()))
    for status in ("MATCH", "ALLOWED", "DIVERGE"):
        n = status_counts[status]
        print(f"  {status:<8} {n:>6}  ({100.0 * n / total:5.1f} %)")
    if allowed_hits:
        print("\ncancelled by allowlist.txt (count · rule · owner):")
        for token, n in allowed_hits.most_common():
            print(f"  {n:>6}  {token:<26} {allow[token].ref}")
        stale = [t for t in allow if t not in allowed_hits]
        if stale:
            print(f"  stale (matched nothing this run): {', '.join(stale)}")
    if token_counts:
        print("\nunexplained divergence tokens (count):")
        for token, n in token_counts.most_common():
            print(f"  {n:>6}  {token}")
    if by_sig:
        print(f"\n{len(by_sig)} distinct divergence signature(s); minimal repro each:")
    for sig, group in by_sig.items():
        print("-" * 78)
        print(f"signature: {' '.join(sig)}   ({len(group)} program(s), "
              f"first: seed={group[0].seed} index={group[0].index})")
        if sig in shrunk:
            text, calls = shrunk[sig]
            path = os.path.join(REPORT, f"{_sig_slug(sig)}.ts")
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(_repro_header(args, group[0], sig) + text)
            print(f"minimal repro ({calls} oracle calls) -> "
                  f"{os.path.relpath(path, ROOT)}")
            print(text.rstrip())
        else:
            print(group[0].source.rstrip())
    if by_sig and not args.adopt:
        print("-" * 78)
        print("adopt a repro:  cp <report path> tooling/differential/repros/  "
              "&& python3 differential.py repros --save --refresh-tsc")


def _repro_header(args, finding: Finding, sig: Sequence[str]) -> str:
    ref_desc = "tsc" if args.ref == "tsc" else os.path.basename(args.ref)
    return ("// typokat differential repro — minimized automatically.\n"
            f"// signature : {' '.join(sig)}\n"
            f"// reference : {ref_desc}\n"
            f"// found     : seed={finding.seed} index={finding.index} "
            f"depth={args.depth_min}-{args.depth_max}\n")


def _write_json_report(args, status_counts, by_sig, shrunk, elapsed, ts_version) -> None:
    _prepare_dir(REPORT, clean=False)
    payload = {
        "seed": args.seed,
        "count": args.count,
        "ref": args.ref,
        "tsc_version": ts_version,
        "bin": args.bin,
        "elapsed_s": round(elapsed, 2),
        "status": dict(status_counts),
        "signatures": [
            {
                "tokens": list(sig),
                "programs": [{"seed": f.seed, "index": f.index} for f in group],
                "repro": shrunk.get(sig, (None, None))[0],
                # Both sides verbatim for the first program of the group, so a finding
                # can be read without re-running anything.
                "first_program": {
                    "source": group[0].source,
                    "reference_output": group[0].ref_raw,
                    "candidate_output": group[0].cand_raw,
                },
            }
            for sig, group in by_sig.items()
        ],
    }
    with open(os.path.join(REPORT, "latest.json"), "w", encoding="utf-8") as fh:
        json.dump(payload, fh, indent=2)


def _adopt(by_sig, shrunk, args) -> None:
    os.makedirs(REPROS, exist_ok=True)
    for sig in by_sig:
        if sig not in shrunk:
            continue
        name = f"{_sig_slug(sig)}.ts"
        path = os.path.join(REPROS, name)
        if os.path.exists(path):
            print(f"adopt: {name} already exists, keeping the committed one")
            continue
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(_repro_header(args, by_sig[sig][0], sig) + shrunk[sig][0])
        print(f"adopt: wrote repros/{name}")


# --- committed repro corpus + scoreboard -------------------------------------


@dataclass
class Row:
    rel: str
    status: str
    tk_ids: Tuple[str, ...]
    ts_ids: Tuple[str, ...]
    tokens: Tuple[str, ...]

    def render(self) -> str:
        return "\t".join([self.status, _ids(self.tk_ids), _ids(self.ts_ids),
                          " ".join(self.tokens) or "-", self.rel])


def _ids(ids: Sequence[str]) -> str:
    return ",".join(ids) if ids else "-"


def _parse_ids(field: str) -> Tuple[str, ...]:
    return () if field == "-" else tuple(field.split(","))


def repro_files() -> List[str]:
    if not os.path.isdir(REPROS):
        return []
    return sorted(f for f in os.listdir(REPROS) if f.endswith(".ts"))


def outcome_ids(o: Outcome) -> Tuple[str, ...]:
    return tuple(sorted(d.ident() for d in o.diags))


def read_scoreboard(path: str = SCOREBOARD) -> Tuple[Dict[str, Row], Dict[str, str]]:
    rows: Dict[str, Row] = {}
    meta: Dict[str, str] = {}
    if not os.path.exists(path):
        return rows, meta
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            line = raw.rstrip("\n")
            if line.startswith("#"):
                m = re.match(r"#\s*(\w[\w-]*):\s*(.*)", line)
                if m:
                    meta[m.group(1)] = m.group(2).strip()
                continue
            if not line.strip():
                continue
            parts = line.split("\t")
            if len(parts) != 5:
                raise HarnessFailure(f"{path}: malformed row {line!r}")
            status, tk, ts, tokens, rel = parts
            rows[rel] = Row(rel, status, _parse_ids(tk), _parse_ids(ts),
                            () if tokens == "-" else tuple(tokens.split(" ")))
    return rows, meta


def write_scoreboard(rows: Sequence[Row], ts_version: str, path: str = SCOREBOARD) -> None:
    counts = Counter(r.status for r in rows)
    header = [
        "# typokat differential — committed repro scoreboard.",
        "# Deterministic: no timestamps, no machine paths. A behaviour change on a",
        "# committed minimal repro shows up as a single changed line in `git diff`.",
        f"# format: {SCOREBOARD_FORMAT}",
        f"# tsc: {ts_version} " + " ".join(TSC_FLAGS),
        f"# repros: {len(rows)}  match: {counts['MATCH']}  allowed: {counts['ALLOWED']}  "
        f"diverge: {counts['DIVERGE']}",
        "# columns: status\ttypokat-ids\ttsc-ids\tsignature\tpath   (ids are line:code)",
    ]
    body = [r.render() for r in sorted(rows, key=lambda r: r.rel)]
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(header + body) + "\n")


def cmd_repros(args) -> int:
    if not os.path.exists(args.bin):
        print(f"error: candidate binary {args.bin} does not exist "
              f"(build it: cargo build --release)", file=sys.stderr)
        return 2
    allow = load_allowlist()
    committed, meta = read_scoreboard()
    files = repro_files()
    if not files:
        print("no repros committed yet (tooling/differential/repros/ is empty)")
        return 0

    ts_version = meta.get("tsc", "unknown").split()[0]
    if args.refresh_tsc:
        ts_version = tsc_version(args.tsc)
        if ts_version != EXPECTED_TSC_VERSION and not args.allow_tsc_version:
            print(f"error: tsc {ts_version} != pinned {EXPECTED_TSC_VERSION} "
                  f"(pass --allow-tsc-version to override)", file=sys.stderr)
            return 2

    rows: List[Row] = []
    regressions: List[str] = []
    progress: List[str] = []
    for name in files:
        rel = f"repros/{name}"
        tk = run_typokat(args.bin, REPROS, name)
        tk_ids = outcome_ids(tk)
        if args.refresh_tsc:
            ts = run_tsc(args.tsc, REPROS, [name])[name]
            ts_ids = outcome_ids(ts)
        elif rel in committed:
            ts_ids = committed[rel].ts_ids
        else:
            print(f"error: {rel} has no committed tsc baseline; run "
                  f"`repros --save --refresh-tsc`", file=sys.stderr)
            return 2
        residue, hits = compare_ids(tk_ids, ts_ids, list(allow.values()))
        tokens = tuple(residue)
        row = Row(rel, classify(residue, hits), tk_ids, ts_ids, tokens)
        rows.append(row)
        if rel not in committed:
            (progress if args.save else regressions).append(
                f"{rel}: not in the scoreboard (run `repros --save`)")
            continue
        old = committed[rel]
        if old.tk_ids != tk_ids:
            regressions.append(
                f"{rel}: typokat ids changed {_ids(old.tk_ids)} -> {_ids(tk_ids)}")
        elif old.status != row.status:
            regressions.append(f"{rel}: status {old.status} -> {row.status}")
    for rel in committed:
        if rel not in {r.rel for r in rows}:
            regressions.append(f"{rel}: in the scoreboard but missing from repros/")

    print(f"repros: {len(rows)}  " + "  ".join(
        f"{k}: {v}" for k, v in sorted(Counter(r.status for r in rows).items())))
    for row in sorted(rows, key=lambda r: r.rel):
        print(f"  {row.status:<8} {row.rel}  typokat={_ids(row.tk_ids)} "
              f"tsc={_ids(row.ts_ids)}")

    if args.save:
        write_scoreboard(rows, ts_version)
        print(f"wrote {os.path.relpath(SCOREBOARD, ROOT)}")
        return 0
    for msg in progress:
        print(f"progress: {msg}")
    for msg in regressions:
        print(f"REGRESS: {msg}")
    if regressions:
        print(f"\n{len(regressions)} regression(s) against the committed scoreboard.")
        return 1
    print("\nno regressions against the committed scoreboard.")
    return 0


def _outcome_from_ids(ids: Sequence[str]) -> Outcome:
    diags = []
    for i in ids:
        line, code = i.split(":")
        diags.append(Diag(int(line), 0, int(code), ""))
    return Outcome(1 if diags else 0, tuple(diags), (), "")


def compare_ids(tk_ids: Sequence[str], ts_ids: Sequence[str],
                rules: Sequence[AllowEntry]) -> Tuple[List[str], Counter]:
    """Re-derive a repro's divergence from two committed id lists (`line:code`), so
    `repros --check` needs typokat only — the tsc side is frozen in the scoreboard,
    exactly like the official suite's committed `.errors.txt` baselines."""
    return compare_tsc(_outcome_from_ids(tk_ids), _outcome_from_ids(ts_ids), rules)


# --- standalone shrink -------------------------------------------------------


def cmd_shrink(args) -> int:
    opts = GenOptions(depth_min=args.depth_min, depth_max=args.depth_max)
    work = _prepare_dir(os.path.join(WORK, "shrink"))
    if args.file:
        with open(args.file, encoding="utf-8") as fh:
            source = fh.read()
        prog = None
    else:
        prog = generate(args.seed, args.index, opts)
        source = prog.render()

    name = "probe.ts"
    with open(os.path.join(work, name), "w", encoding="utf-8") as fh:
        fh.write(source)
    rules = list(load_allowlist().values()) if args.ref == "tsc" else []
    cand = run_typokat(args.bin, work, name)
    if args.ref == "tsc":
        ref = run_tsc(args.tsc, work, [name])[name]
        tokens, _hits = compare_tsc(cand, ref, rules)
    else:
        ref = run_typokat(args.ref, work, name)
        tokens = compare_binaries(ref, cand)
    if not tokens:
        print("the input does not diverge — nothing to shrink")
        return 0
    print(f"signature: {' '.join(tokens)}")
    oracle = make_oracle(args, work, tuple(sorted(tokens)), rules)
    if prog is not None:
        best, calls = shrink_program(prog, oracle, budget=args.shrink_budget)
        text = shrink_text(best.render(), oracle, prefix=f"g{args.seed}_{args.index}_")
    else:
        text, calls = shrink_text(source, oracle), 0
    print(f"--- minimal repro ({calls} structural oracle calls) ---")
    print(text.rstrip())
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write(text)
        print(f"wrote {args.out}")
    shutil.rmtree(work, ignore_errors=True)
    return 0


# --- main --------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    def common(sp):
        sp.add_argument("--bin", default=DEFAULT_BIN,
                        help="candidate typokat binary (default: target/release/typokat)")
        sp.add_argument("--ref", default="tsc",
                        help="'tsc', 'none' (sanity sweep), or a path to a reference "
                             "typokat binary")
        sp.add_argument("--tsc", default=os.environ.get("TYPOKAT_TSC", "tsc"),
                        help="tsc executable (env: TYPOKAT_TSC)")
        sp.add_argument("--allow-tsc-version", action="store_true",
                        help=f"accept a tsc other than the pinned {EXPECTED_TSC_VERSION}")
        sp.add_argument("--depth-min", type=int, default=1)
        sp.add_argument("--depth-max", type=int, default=4)
        sp.add_argument("--shrink-budget", type=int, default=600,
                        help="max oracle calls per signature while shrinking")

    f = sub.add_parser("fuzz", help="generate programs and diff two checkers")
    common(f)
    f.add_argument("--seed", type=int, default=1)
    f.add_argument("--count", type=int, default=200)
    f.add_argument("--time-budget", type=float, default=0,
                   help="seconds; stop generating past this (0 = unbounded)")
    f.add_argument("--tsc-batch", type=int, default=200,
                   help="files per tsc invocation (tsc start-up dominates otherwise)")
    f.add_argument("--check", action="store_true",
                   help="exit 1 if any non-allowlisted divergence was found")
    f.add_argument("--no-shrink", dest="shrink", action="store_false", default=True)
    f.add_argument("--shrink-allowed", action="store_true",
                   help="also shrink ALLOWED signatures (normally only DIVERGE)")
    f.add_argument("--adopt", action="store_true",
                   help="write shrunk repros into repros/ (then run `repros --save`)")
    f.add_argument("--keep-work", action="store_true",
                   help="keep the generated corpus in work/ for inspection")
    f.set_defaults(func=cmd_fuzz)

    s = sub.add_parser("shrink", help="minimize one program that diverges")
    common(s)
    s.add_argument("--seed", type=int, default=1)
    s.add_argument("--index", type=int, default=0)
    s.add_argument("--file", help="shrink an arbitrary .ts file (line-level only)")
    s.add_argument("-o", "--out", help="write the minimized repro here")
    s.set_defaults(func=cmd_shrink)

    r = sub.add_parser("repros", help="the committed minimal-repro corpus + scoreboard")
    r.add_argument("--bin", default=DEFAULT_BIN)
    r.add_argument("--tsc", default=os.environ.get("TYPOKAT_TSC", "tsc"))
    r.add_argument("--allow-tsc-version", action="store_true")
    r.add_argument("--save", action="store_true", help="rewrite scoreboard.txt")
    r.add_argument("--refresh-tsc", action="store_true",
                   help="re-run tsc to refresh the frozen tsc baselines (needs tsc)")
    r.add_argument("--check", action="store_true",
                   help="exit 1 on any change vs the committed scoreboard (default)")
    r.set_defaults(func=cmd_repros)
    return p


def _resolve_exe(path: str) -> str:
    """Absolutize a path-like executable argument.

    Every checker invocation runs with `cwd` set to the corpus directory, and POSIX
    resolves a relative program path against the CHILD's cwd — so `--bin
    ../../target/release/typokat` would silently look inside `work/`. A bare name
    (`tsc`) is left alone for the PATH lookup."""
    return os.path.abspath(path) if os.sep in path else path


def main(argv: Optional[List[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    for attr in ("bin", "ref", "tsc"):
        value = getattr(args, attr, None)
        if isinstance(value, str) and value not in ("tsc", "none"):
            setattr(args, attr, _resolve_exe(value))
    try:
        return args.func(args)
    except HarnessFailure as exc:
        print(f"HARNESS FAILURE: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())

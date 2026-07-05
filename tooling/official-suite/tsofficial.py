#!/usr/bin/env python3
"""
typokat × official TypeScript conformance suite — black-box harness.

This tool treats the typokat *binary* as a black box: it never imports or builds
the checker crate, it shells out to `typokat check <file>` and parses the rendered
diagnostics. That keeps it usable while the checker source is mid-refactor — point
it at any prebuilt binary (default: target/release/typokat).

Two subcommands:

  fetch   Pull a curated slice of microsoft/TypeScript conformance tests + their
          `.errors.txt` baselines at a pinned commit into ./corpus (gitignored).

  run     For every fetched test: replicate TS's unit parsing (strip `// @option`
          directive lines so line numbers line up with the baseline), gate each
          test into in-scope / out-of-scope buckets ("discover"), then diff
          typokat's diagnostics against the baseline on the in-scope set and print
          a triage dashboard (matched / FALSE NEGATIVE / false positive). It is a
          *dashboard*, never a pass/fail gate — the project's #1 fear is dropped
          errors (false negatives), so those are surfaced loudly.

Code mapping is the identity on the number: typokat mirrors tsc, so TS2322 == TK2322.

See README.md for the design and the gating rules.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.request
import urllib.error
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone

# --- Configuration -----------------------------------------------------------

# Pinned to the v6.0.3 tag — the same tsc release typokat was cross-checked
# against (README: "real tsc 6.0.3 --strict"). Baselines at this SHA match it.
PINNED_SHA = "050880ce59e30b356b686bd3144efe24f875ebc8"
REPO = "microsoft/TypeScript"

# Curated conformance subtrees that map onto typokat's M0–M22 scope. The harness
# auto-buckets whatever is still out of scope, so this list errs broad. Paths are
# under tests/cases/. Override per-run with repeated --dir.
DEFAULT_DIRS = [
    "conformance/types/typeRelationships/assignmentCompatibility",
    "conformance/types/typeRelationships/comparable",
    "conformance/types/typeRelationships/subtypesAndSuperTypes",
    "conformance/types/typeRelationships/typeAndMemberIdentity",
    "conformance/types/members",
    "conformance/types/objectTypeLiteral",
    "conformance/types/primitives",
    "conformance/types/literal",
    "conformance/types/union",
    "conformance/types/intersection",
    "conformance/types/keyof",
    "conformance/types/indexedAccessTypes",
    "conformance/types/tuple",
    "conformance/controlFlow",
    "conformance/expressions/typeGuards",
    "conformance/classes/members",
    "conformance/classes/propertyMemberDeclarations",
    "conformance/interfaces/interfaceDeclarations",
]

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus")
REPORT = os.path.join(HERE, "report")
# Committed (NOT gitignored) regression baseline — deterministic, diff-friendly.
SCOREBOARD = os.path.join(HERE, "scoreboard.txt")

# --- TS test-case parsing (replicates the bits we need of TestCaseParser) -----

# A compiler-option directive line: `// @name: value`. TS strips these from the
# unit content, which shifts every following line up — so baseline line N maps to
# the stripped content's line N, NOT the raw file's. We must run typokat on the
# stripped content for line numbers to align. `@ts-ignore`/`@ts-expect-error` have
# no colon, so they don't match and are (correctly) kept.
OPTION_RE = re.compile(r"^//\s*@(\w+)\s*:\s*(.*?)\s*$")


def parse_units(source):
    """Return (options, units). options: lowercased name->value of global @opts.
    units: list of (filename, stripped_content). Multi-file tests yield >1 unit.
    Line numbers in each unit's stripped_content align with the baseline."""
    options = {}
    units = []
    cur_name = None
    cur_lines = []

    def flush():
        if cur_name is not None or cur_lines:
            # Strip the leading blank run left by removed @option headers, like TS's
            # makeUnitsFromTest — baselines number lines from the first non-blank line.
            # NOTE: `\s*` is deliberately greedy across newlines: one substitution eats
            # the WHOLE run. Don't "harden" it to `[ \t]*\n` — that would strip a single
            # line and re-break multi-blank files (constructorParameterShadowsOuterScopes2.ts).
            content = re.sub(r"^\s*\n", "", "\n".join(cur_lines), count=1)
            units.append((cur_name, content))

    for raw in source.split("\n"):
        m = OPTION_RE.match(raw)
        if m:
            name = m.group(1).lower()
            val = m.group(2)
            if name == "filename":
                flush()
                cur_name = val
                cur_lines = []
            else:
                # Global option line — stripped from content (don't emit).
                options[name] = val
            continue
        cur_lines.append(raw)
    flush()

    # If no @filename ever appeared, the single unit is unnamed (the whole file).
    if not units:
        units = [(None, source)]
    return options, units


def is_strict(options):
    """typokat is always strictNullChecks-on. Tag tests by whether tsc ran strict
    so the dashboard can separate config-comparable results from null-divergent
    ones."""
    def truthy(v):
        return str(v).strip().lower() in ("true", "1")
    if "strict" in options and truthy(options["strict"]):
        return True
    if "strictnullchecks" in options and truthy(options["strictnullchecks"]):
        return True
    return False


# Out-of-scope *checking* features. typokat's parser (oxc) accepts all of these,
# but the checker doesn't model them — so they'd diverge silently (no TK2304 to
# self-gate on). Bucket them as `syntax` up front. Heuristic, documented as such.
OUT_OF_SCOPE_SYNTAX = [
    ("module", re.compile(r"^\s*import\s|^\s*export\s|\brequire\s*\(|^///\s*<reference"
                          r"|\bdeclare\s+module\b|\bnamespace\b|\bmodule\s+\w+\s*\{", re.M)),
    ("enum", re.compile(r"\benum\b")),
    ("decorator", re.compile(r"^\s*@[A-Za-z_]\w*\s*[\(\n]", re.M)),
    # conditional-type + infer gates removed — M25 models them (backlog 09).
    # mapped-type gate removed — M26 models them (backlog 10).
    # template-lit-type gate removed — M27 models them (backlog 11).
    ("satisfies", re.compile(r"\bsatisfies\b")),
    ("as-const", re.compile(r"\bas\s+const\b")),
    # Deferred narrowing (README: type predicates + unstructured-flow CFG = M23).
    # The conformance typeGuards/ tests exercise exactly these, so gate them until
    # the flow-node CFG lands; otherwise they flood the false-negative bucket.
    ("type-predicate", re.compile(r"\)\s*:\s*[\w.]+\s+is\b")),
    ("asserts", re.compile(r"\basserts\s+")),
    ("instanceof", re.compile(r"\binstanceof\b")),
]


def syntax_bucket(content):
    for label, rx in OUT_OF_SCOPE_SYNTAX:
        if rx.search(content):
            return label
    return None


# --- typokat invocation + output parsing -------------------------------------

# A diagnostic header `error[TK2322]: ...`, then a location line `... :line:col`.
TK_HEAD_RE = re.compile(r"^error\[TK(\d+)\]:")
PARSE_ERR_RE = re.compile(r"^error: ")


def run_typokat(binary, content):
    """Run the binary on `content`; return (parse_errors, diagnostics) where
    diagnostics is a list of (line, code:int). Lines are 1-based, aligned to
    `content` (which the caller has already stripped of @option directives)."""
    with tempfile.NamedTemporaryFile("w", suffix=".ts", delete=False) as f:
        f.write(content)
        tmp = f.name
    try:
        proc = subprocess.run([binary, "check", tmp],
                              capture_output=True, text=True, timeout=30)
        out = proc.stderr + proc.stdout
    except subprocess.TimeoutExpired:
        return (["<timeout>"], [])
    finally:
        os.unlink(tmp)

    tmp_base = os.path.basename(tmp)
    loc_re = re.compile(re.escape(tmp_base) + r":(\d+):(\d+)")
    parse_errors = []
    diags = []
    pending = None  # code awaiting its location line
    for line in out.split("\n"):
        h = TK_HEAD_RE.match(line)
        if h:
            pending = int(h.group(1))
            continue
        if PARSE_ERR_RE.match(line):
            parse_errors.append(line)
            continue
        if pending is not None:
            lm = loc_re.search(line)
            if lm:
                diags.append((int(lm.group(1)), pending))
                pending = None
    return parse_errors, diags


# --- baseline (.errors.txt) parsing ------------------------------------------

# Top-section lines: `path(line,col): error TS2322: message`. Reason-chain
# continuation lines are indented and lack the `(line,col): error TS` prefix, so
# they don't match. We only read the head section (before the `====` reproduction).
BASELINE_RE = re.compile(r"\((\d+),(\d+)\):\s*error\s+TS(\d+):")


def parse_baseline(text):
    """Return list of (line, code:int) expected by tsc. Empty if no baseline."""
    if text is None:
        return []
    head = text.split("\n====", 1)[0]
    out = []
    for line in head.split("\n"):
        m = BASELINE_RE.search(line)
        if m:
            out.append((int(m.group(1)), int(m.group(3))))
    return out


# --- diff / classification ---------------------------------------------------

def diff(expected, actual):
    """expected/actual: lists of (line, code). Returns dict with matched / fn / fp
    counts and concrete per-line detail for the misses."""
    exp = defaultdict(Counter)
    act = defaultdict(Counter)
    for ln, c in expected:
        exp[ln][c] += 1
    for ln, c in actual:
        act[ln][c] += 1
    matched = fn = fp = 0
    fn_detail = []  # (line, code) tsc had, typokat missed  <-- the scary bucket
    fp_detail = []  # (line, code) typokat had, tsc didn't
    for ln in set(exp) | set(act):
        e, a = exp[ln], act[ln]
        inter = e & a  # Counter intersection = min per code
        matched += sum(inter.values())
        for c, n in (e - a).items():
            fn += n
            fn_detail.extend([(ln, c)] * n)
        for c, n in (a - e).items():
            fp += n
            fp_detail.extend([(ln, c)] * n)
    return {"matched": matched, "fn": fn, "fp": fp,
            "fn_detail": fn_detail, "fp_detail": fp_detail}


# --- corpus on disk ----------------------------------------------------------

def corpus_tests():
    """Yield (relpath, ts_path, baseline_path_or_None) for every fetched test."""
    for root, _dirs, files in os.walk(CORPUS):
        for fn in sorted(files):
            if fn.endswith(".ts") and not fn.endswith(".d.ts"):
                ts = os.path.join(root, fn)
                base = ts[:-3] + ".errors.txt"
                rel = os.path.relpath(ts, CORPUS)
                yield rel, ts, (base if os.path.exists(base) else None)


# --- fetch -------------------------------------------------------------------

def gh_list_dir(path):
    """List a repo dir at the pinned SHA via gh (authenticated, reliable).
    Returns list of (type, name, path)."""
    r = subprocess.run(
        ["gh", "api", f"repos/{REPO}/contents/{path}?ref={PINNED_SHA}",
         "--jq", r'.[] | [.type, .name, .path] | @tsv'],
        capture_output=True, text=True)
    if r.returncode != 0:
        sys.stderr.write(f"  ! skip {path}: {r.stderr.strip().splitlines()[-1:] }\n")
        return []
    rows = []
    for line in r.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) == 3:
            rows.append(tuple(parts))
    return rows


def collect_ts_paths(dirs):
    """BFS the configured dirs, returning all .ts file repo-paths."""
    found = []
    stack = [f"tests/cases/{d}" for d in dirs]
    while stack:
        path = stack.pop()
        for typ, name, full in gh_list_dir(path):
            if typ == "dir":
                stack.append(full)
            elif typ == "file" and name.endswith(".ts"):
                found.append(full)
    return found


def raw_url(path):
    return f"https://raw.githubusercontent.com/{REPO}/{PINNED_SHA}/{path}"


def fetch_blob(path):
    try:
        with urllib.request.urlopen(raw_url(path), timeout=30) as r:
            return r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise


def cmd_fetch(args):
    dirs = args.dir or DEFAULT_DIRS
    os.makedirs(CORPUS, exist_ok=True)
    print(f"Listing {len(dirs)} dir(s) at {REPO}@{PINNED_SHA[:9]} ...")
    ts_paths = collect_ts_paths(dirs)
    if args.limit:
        ts_paths = ts_paths[:args.limit]
    print(f"Found {len(ts_paths)} test files; fetching tests + baselines ...")

    manifest = {"sha": PINNED_SHA, "repo": REPO, "dirs": dirs, "tests": []}

    def fetch_one(ts_repo_path):
        # ts_repo_path like tests/cases/conformance/.../foo.ts
        rel = os.path.relpath(ts_repo_path, "tests/cases")
        dst_ts = os.path.join(CORPUS, rel)
        os.makedirs(os.path.dirname(dst_ts), exist_ok=True)
        src = fetch_blob(ts_repo_path)
        if src is None:
            return None
        with open(dst_ts, "w") as f:
            f.write(src)
        base_name = os.path.basename(ts_repo_path)[:-3] + ".errors.txt"
        base = fetch_blob(f"tests/baselines/reference/{base_name}")
        if base is not None:
            with open(dst_ts[:-3] + ".errors.txt", "w") as f:
                f.write(base)
        return (rel, base is not None)

    done = 0
    with ThreadPoolExecutor(max_workers=12) as ex:
        for res in ex.map(fetch_one, ts_paths):
            if res:
                rel, has_base = res
                manifest["tests"].append({"path": rel, "baseline": has_base})
                done += 1
                if done % 25 == 0:
                    print(f"  ... {done}/{len(ts_paths)}")
    with open(os.path.join(CORPUS, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=2)
    n_base = sum(1 for t in manifest["tests"] if t["baseline"])
    print(f"Done: {done} tests in ./corpus ({n_base} with an errors baseline, "
          f"{done - n_base} expected-clean).")


# --- run ---------------------------------------------------------------------

def cmd_run(args):
    binary = args.bin
    if not os.path.exists(binary):
        sys.exit(f"binary not found: {binary} (build it or pass --bin)")

    tests = list(corpus_tests())
    if args.limit:
        tests = tests[:args.limit]
    if not tests:
        sys.exit("empty corpus — run `tsofficial.py fetch` first.")

    # One record per test. bucket=None means in-scope (diffed); otherwise the
    # out-of-scope reason. Out-of-scope records keep zeroed diff fields so the
    # scoreboard can still track scope changes (IN <-> OOS) as regressions/progress.
    results = []
    for rel, ts_path, base_path in tests:
        with open(ts_path) as f:
            source = f.read()
        options, units = parse_units(source)
        rec = {"rel": rel, "bucket": None, "strict": is_strict(options),
               "matched": 0, "fn": 0, "fp": 0, "expected": 0,
               "fn_detail": [], "fp_detail": []}

        if len(units) > 1:
            rec["bucket"] = "multifile"
            results.append(rec); continue
        content = units[0][1]

        sb = syntax_bucket(content)
        if sb:
            rec["bucket"] = f"syntax:{sb}"
            results.append(rec); continue

        baseline = None
        if base_path:
            with open(base_path) as f:
                baseline = f.read()
        expected = parse_baseline(baseline)
        rec["expected"] = len(expected)

        parse_errors, diags = run_typokat(binary, content)
        if parse_errors:
            rec["bucket"] = "parse-error"
            results.append(rec); continue

        # Self-gate: any TK2304 typokat raised that the baseline does NOT have is a
        # name typokat couldn't resolve but tsc could → lib / module / unknown
        # type → out of scope. Catches lib.d.ts + imports without a denylist.
        exp_2304 = {ln for (ln, c) in expected if c == 2304}
        act_2304 = {ln for (ln, c) in diags if c == 2304}
        if act_2304 - exp_2304:
            rec["bucket"] = "unresolved"
            results.append(rec); continue

        d = diff(expected, diags)
        rec.update(matched=d["matched"], fn=d["fn"], fp=d["fp"],
                   fn_detail=d["fn_detail"], fp_detail=d["fp_detail"])
        results.append(rec)

    print_dashboard(binary, results)

    if args.save:
        path = write_scoreboard(results)
        print(f"  saved baseline → {path}")
    if args.check:
        if not compare_scoreboard(results):
            sys.exit(1)


def print_dashboard(binary, results):
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    n = len(results)
    inscope = [r for r in results if r["bucket"] is None]
    buckets = Counter(r["bucket"] for r in results if r["bucket"] is not None)
    n_oos = sum(buckets.values())
    n_in = len(inscope)

    def agg(rows):
        return (sum(r["matched"] for r in rows), sum(r["fn"] for r in rows),
                sum(r["fp"] for r in rows), sum(r["expected"] for r in rows))

    strict_rows = [r for r in inscope if r["strict"]]
    nonstrict_rows = [r for r in inscope if not r["strict"]]

    # Surface the binary's mtime — a stale prebuilt binary (not rebuilt after a
    # checker change) silently measures old behavior. Rebuild before trusting.
    try:
        import time
        mt = time.strftime("%Y-%m-%d %H:%M", time.localtime(os.path.getmtime(binary)))
    except OSError:
        mt = "?"
    print()
    print(f"typokat × official TS conformance — {now}")
    print(f"  TS @ {PINNED_SHA[:9]}   bin: {binary} (built {mt} — rebuild if stale)")
    print(f"  corpus: {n} tests   in-scope: {n_in}   out-of-scope: {n_oos}")
    print()
    print("  out-of-scope buckets (discover):")
    for k, v in sorted(buckets.items(), key=lambda kv: -kv[1]):
        print(f"    {k:24} {v}")
    print()

    for label, rows in (("strict", strict_rows), ("non-strict (null-divergent)", nonstrict_rows)):
        if not rows:
            continue
        matched, fn, fp, exp = agg(rows)
        total = matched + fn
        pct = (100.0 * matched / total) if total else 100.0
        # File-level exact agreement: a file where typokat matches the baseline
        # with zero misses AND zero over-reports. The most intuitive "shoda".
        passed = sum(1 for r in rows if r["fn"] == 0 and r["fp"] == 0)
        fpct = 100.0 * passed / len(rows)
        print(f"  in-scope diff — {label}: {len(rows)} tests, {exp} expected diagnostics")
        print(f"    files matched exactly: {passed}/{len(rows)}  ({fpct:.1f}%)")
        print(f"    diagnostics matched:   {matched}/{total}  ({pct:.1f}%)")
        print(f"    FALSE NEGATIVE:        {fn}   <-- tsc errored, typokat silent")
        print(f"    false positive:        {fp}")
        print()

    # Concrete false-negative repros — the bucket the project cares about most.
    fn_files = sorted([r for r in inscope if r["fn"]],
                      key=lambda r: -r["fn"])
    if fn_files:
        print("  top false-negative files (tsc errored, typokat missed):")
        for r in fn_files[:15]:
            lines = ",".join(str(ln) for (ln, _c) in r["fn_detail"][:8])
            print(f"    {r['fn']:3}  {r['rel']}  (lines {lines})")
        print()

    os.makedirs(REPORT, exist_ok=True)
    report = {
        "when": now, "sha": PINNED_SHA, "binary": binary,
        "corpus": n, "in_scope": n_in, "buckets": dict(buckets),
        "files": inscope,
    }
    with open(os.path.join(REPORT, "latest.json"), "w") as f:
        json.dump(report, f, indent=2)
    print(f"  wrote {os.path.join(REPORT, 'latest.json')}")


# --- regression scoreboard (committed, deterministic) ------------------------

def _scoreboard_stats(results):
    inscope = [r for r in results if r["bucket"] is None]
    clean = [r for r in inscope if r["expected"] == 0]
    err = [r for r in inscope if r["expected"] > 0]
    clean_kept = sum(1 for r in clean if r["fp"] == 0)
    err_exact = sum(1 for r in err if r["fn"] == 0 and r["fp"] == 0)
    rec_m = sum(r["matched"] for r in err)
    rec_t = rec_m + sum(r["fn"] for r in err)
    return inscope, clean, err, clean_kept, err_exact, rec_m, rec_t


def write_scoreboard(results):
    """Write the committed baseline: one sorted line per test, no timestamps or
    machine paths, so a regression shows up as a single changed line in `git diff`.
    Out-of-scope tests are recorded too (so scope flips are tracked)."""
    rows = sorted(results, key=lambda r: r["rel"])
    inscope, clean, err, clean_kept, err_exact, rec_m, rec_t = _scoreboard_stats(rows)
    with open(SCOREBOARD, "w") as f:
        f.write("# typokat × official TS conformance — regression scoreboard\n")
        f.write(f"# TS @ {PINNED_SHA}\n")
        f.write(f"# corpus {len(rows)}  in-scope {len(inscope)}  "
                f"out-of-scope {len(rows) - len(inscope)}\n")
        f.write(f"# clean-kept {clean_kept}/{len(clean)}  "
                f"error-exact {err_exact}/{len(err)}  diag-recall {rec_m}/{rec_t}\n")
        f.write("# cols: status<TAB>matched fn fp expected<TAB>rel  "
                "(status=IN | OOS:<bucket>; '-' = n/a)\n")
        for r in rows:
            if r["bucket"] is None:
                f.write(f"IN\t{r['matched']} {r['fn']} {r['fp']} {r['expected']}\t{r['rel']}\n")
            else:
                f.write(f"OOS:{r['bucket']}\t- - - -\t{r['rel']}\n")
    return SCOREBOARD


def read_scoreboard():
    out = {}
    with open(SCOREBOARD) as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 3:
                continue
            status, nums, rel = parts
            def num(x):
                return None if x == "-" else int(x)
            m, fn, fp, exp = (num(x) for x in nums.split())
            out[rel] = {"status": status, "matched": m, "fn": fn, "fp": fp,
                        "expected": exp}
    return out


def compare_scoreboard(results):
    """Diff the current run against the committed baseline. Returns True if there
    are no regressions. A regression = a previously in-scope file that now matches
    fewer diagnostics, over-reports more, or fell out of scope."""
    if not os.path.exists(SCOREBOARD):
        print("\n  no committed scoreboard.txt yet — run `run --save` to create one.")
        return True
    base = read_scoreboard()
    cur = {r["rel"]: r for r in results}
    regress, progress = [], []
    for rel, c in cur.items():
        b = base.get(rel)
        if b is None:
            continue  # new test — not a regression
        cur_in = c["bucket"] is None
        base_in = b["status"] == "IN"
        if base_in and cur_in:
            if c["matched"] < b["matched"] or c["fp"] > b["fp"]:
                regress.append((rel, f"matched {b['matched']}→{c['matched']}, "
                                     f"fp {b['fp']}→{c['fp']}"))
            elif c["matched"] > b["matched"] or c["fp"] < b["fp"]:
                progress.append((rel, f"matched {b['matched']}→{c['matched']}, "
                                      f"fp {b['fp']}→{c['fp']}"))
        elif base_in and not cur_in:
            regress.append((rel, f"IN → OOS:{c['bucket']} (lost coverage)"))
        elif not base_in and cur_in:
            progress.append((rel, f"{b['status']} → IN (gained coverage)"))
    missing = [rel for rel in base if rel not in cur]

    print(f"\n  regression check vs scoreboard.txt — "
          f"regressions: {len(regress)}   progress: {len(progress)}   "
          f"missing-from-corpus: {len(missing)}")
    for rel, msg in sorted(regress)[:30]:
        print(f"    ✗ REGRESS  {rel}  ({msg})")
    for rel, msg in sorted(progress)[:15]:
        print(f"    ✓ progress {rel}  ({msg})")
    if len(progress) > 15:
        print(f"    … +{len(progress) - 15} more improved")
    if missing:
        print(f"    note: {len(missing)} baseline files absent from corpus "
              f"(re-fetch at the pinned SHA for a complete check)")
    return len(regress) == 0


# --- main --------------------------------------------------------------------

def main():
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    pf = sub.add_parser("fetch", help="pull pinned conformance tests + baselines")
    pf.add_argument("--dir", action="append",
                    help="conformance dir under tests/cases/ (repeatable; "
                         "defaults to the curated set)")
    pf.add_argument("--limit", type=int, help="cap number of test files")
    pf.set_defaults(func=cmd_fetch)

    pr = sub.add_parser("run", help="diff typokat vs baselines, print dashboard")
    pr.add_argument("--bin", default=os.path.join(
        HERE, "..", "..", "target", "release", "typokat"),
        help="path to the typokat binary (default: target/release/typokat)")
    pr.add_argument("--limit", type=int, help="cap number of tests")
    pr.add_argument("--save", action="store_true",
                    help="write/update the committed scoreboard.txt baseline")
    pr.add_argument("--check", action="store_true",
                    help="compare against scoreboard.txt; exit 1 on any regression")
    pr.set_defaults(func=cmd_run)

    args = p.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()

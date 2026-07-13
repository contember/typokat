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
import shutil
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

# Curated conformance subtrees that map onto typokat's implemented checker scope.
# The harness auto-buckets whatever is still out of scope, so this list errs
# broad. Paths are under tests/cases/. Override per-run with repeated --dir.
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
# An incomplete-surface record `incomplete[<stable-id>]: ...` (WU2). The id is the
# stable `role/surface/slot-or-variant` identity; it carries no TK code. Anchored to
# COLUMN 0, rich-format-only: run_typokat invokes the binary with no --format, so the
# harness only ever sees rich output, where a real record starts the line. Rich output
# also quotes source-snippet lines (and diagnostic messages can embed user strings),
# so any looser pattern — unanchored, or with a mid-line `\): ` compact-style
# alternative — lets user code containing `incomplete[...]` fabricate phantom records
# (WU7-B blocker + re-review gap a). If a future harness mode passes --format compact,
# select a compact-anchored regex BY that format parameter; never widen this one.
INCOMPLETE_RE = re.compile(r"^incomplete\[([^\]]+)\]")

# The stable surface-id shape `role/surface/slot-or-variant` (kebab-case segments,
# two or more slashes' worth). Enforced on parse AND serialize: an id containing
# `,`, `|`, a TAB, or any other stray character would corrupt the scoreboard's
# `<matched>|<fp>|<incomplete>` column, so a malformed id is a hard failure, never
# silently written or read (WU7-B hardening).
INC_ID_RE = re.compile(r"^[a-z0-9-]+(?:/[a-z0-9-]+)+$")

# Exit codes the typokat CLI is documented to return: 0 (clean), 1 (type/parse
# errors), and 3 (incomplete — the checker skipped an in-scope surface; WU2). 2 is a
# usage error and anything else (or a negative code = killed by a signal) means the
# checker crashed or the harness misused it. Exit 3 is a *discovery* result
# (OOS:unsupported), never a crash — but with unparseable output it is still a hard
# failure, and it never becomes a silent success path.
OK_EXIT_CODES = (0, 1, 3)


class HarnessFailure(Exception):
    """A checker invocation the harness cannot trust: an unexpected exit code, a
    signal/crash, a timeout, or output whose exit-code-vs-diagnostics story is
    inconsistent. Raised so the run aborts loudly instead of scoring the file as a
    silent zero — dropping a crash into the ``0 diagnostics`` bucket would hide
    exactly the false negatives this suite exists to catch."""


def run_typokat(binary, content, rel="<unknown>"):
    """Run the binary on `content`; return (parse_errors, diagnostics, incompletes).
    `diagnostics` is a list of (line, code:int); `incompletes` is a list of stable
    surface-id strings (WU2, exit 3). Lines are 1-based, aligned to `content` (which
    the caller has already stripped of @option directives).

    Raises `HarnessFailure` (never scored as success or zero) when the process times
    out, is killed by a signal, exits with an undocumented code, or produces output
    inconsistent with its exit code: exit 0 with ANY diagnostic OR incomplete record,
    exit 1 with nothing parseable OR with an incomplete record (incomplete forces
    exit 3), or exit 3 with no incomplete record (lost output)."""
    with tempfile.NamedTemporaryFile("w", suffix=".ts", delete=False) as f:
        f.write(content)
        tmp = f.name
    try:
        try:
            proc = subprocess.run([binary, "check", tmp],
                                  capture_output=True, text=True, timeout=30)
        except subprocess.TimeoutExpired:
            raise HarnessFailure(f"{rel}: typokat timed out after 30s")
        out = proc.stderr + proc.stdout
        returncode = proc.returncode
    finally:
        os.unlink(tmp)

    tmp_base = os.path.basename(tmp)
    loc_re = re.compile(re.escape(tmp_base) + r":(\d+):(\d+)")
    parse_errors = []
    diags = []
    incompletes = []
    pending = None  # code awaiting its location line
    for line in out.split("\n"):
        # Diagnostic/parse-error heads are anchored at column 0, so match them first:
        # a real incomplete record can never start with `error[`/`error: `, while a
        # diagnostic message could in principle embed incomplete-looking text.
        h = TK_HEAD_RE.match(line)
        if h:
            pending = int(h.group(1))
            continue
        if PARSE_ERR_RE.match(line):
            parse_errors.append(line)
            continue
        # Incomplete records carry no TK code and never a pending diag location, so
        # scan them independently of the diagnostic state machine. Anchored .match,
        # same as TK_HEAD_RE — quoted source must never fabricate a record.
        im = INCOMPLETE_RE.match(line)
        if im:
            inc_id = im.group(1)
            # Defensive shape check: a malformed id would corrupt the scoreboard
            # column format, so it is a hard failure, never recorded.
            if not INC_ID_RE.match(inc_id):
                raise HarnessFailure(
                    f"{rel}: typokat printed an incomplete record with a malformed "
                    f"surface id {inc_id!r} (expected role/surface/slot-or-variant, "
                    f"kebab-case)\n--- captured output ---\n{out}")
            incompletes.append(inc_id)
            continue
        if pending is not None:
            lm = loc_re.search(line)
            if lm:
                diags.append((int(lm.group(1)), pending))
                pending = None

    # Validate the exit code and its consistency with the parsed output. A failure
    # here is a harness failure, not a data point: it must never be silently scored.
    has_diag_output = bool(parse_errors) or bool(diags)
    if returncode not in OK_EXIT_CODES:
        raise HarnessFailure(
            f"{rel}: typokat exited with unexpected code {returncode} "
            f"(expected 0, 1, or 3; negative = killed by signal)\n"
            f"--- captured output ---\n{out}")
    if returncode == 0 and (has_diag_output or incompletes):
        raise HarnessFailure(
            f"{rel}: typokat exited 0 but reported diagnostics/parse errors/incomplete "
            f"records (inconsistent: a clean exit must be silent)\n"
            f"--- captured output ---\n{out}")
    if returncode == 1 and not has_diag_output:
        raise HarnessFailure(
            f"{rel}: typokat exited 1 but no diagnostic or parse error could be "
            f"parsed (unparseable / lost output)\n"
            f"--- captured output ---\n{out}")
    if returncode == 1 and incompletes:
        # The CLI can never exit 1 with an incomplete record (incomplete forces exit
        # 3), so this means parser fabrication or a broken binary (re-review gap b).
        raise HarnessFailure(
            f"{rel}: typokat exited 1 but printed incomplete record(s) "
            f"(inconsistent: any incomplete surface must force exit 3)\n"
            f"--- captured output ---\n{out}")
    if returncode == 3 and not incompletes:
        raise HarnessFailure(
            f"{rel}: typokat exited 3 (incomplete) but no incomplete record could be "
            f"parsed (unparseable / lost output)\n"
            f"--- captured output ---\n{out}")
    return parse_errors, diags, incompletes


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
    counts and concrete per-(line, code) detail. `matched_detail` is the *identity*
    of the matched diagnostics (with multiplicity) — the scoreboard ratchet compares
    these sets, so a same-count swap of WHICH diagnostics match is a regression."""
    exp = defaultdict(Counter)
    act = defaultdict(Counter)
    for ln, c in expected:
        exp[ln][c] += 1
    for ln, c in actual:
        act[ln][c] += 1
    matched = fn = fp = 0
    matched_detail = []  # (line, code) both tsc and typokat agree on (identity)
    fn_detail = []  # (line, code) tsc had, typokat missed  <-- the scary bucket
    fp_detail = []  # (line, code) typokat had, tsc didn't
    for ln in set(exp) | set(act):
        e, a = exp[ln], act[ln]
        inter = e & a  # Counter intersection = min per code
        matched += sum(inter.values())
        for c, n in inter.items():
            matched_detail.extend([(ln, c)] * n)
        for c, n in (e - a).items():
            fn += n
            fn_detail.extend([(ln, c)] * n)
        for c, n in (a - e).items():
            fp += n
            fp_detail.extend([(ln, c)] * n)
    return {"matched": matched, "fn": fn, "fp": fp,
            "matched_detail": matched_detail,
            "fn_detail": fn_detail, "fp_detail": fp_detail}


# --- corpus on disk ----------------------------------------------------------

def manifest_tests(manifest):
    """Return manifest-ordered `(rel, ts, baseline-or-None)` corpus entries."""
    out = []
    for entry in manifest["tests"]:
        rel = entry["path"]
        ts = os.path.join(CORPUS, rel)
        base = ts[:-3] + ".errors.txt"
        out.append((rel, ts, base if entry["baseline"] else None))
    return out


# --- fetch -------------------------------------------------------------------

class FetchFailure(RuntimeError):
    """The pinned corpus could not be reproduced exactly."""


def _validate_posix_path(path, root, label):
    """Return a safe, normalized repo path rooted at ``root``."""
    if not isinstance(path, str) or not path or "\\" in path or path.startswith("/"):
        raise FetchFailure(f"invalid {label} path: {path!r}")
    parts = path.split("/")
    root_parts = root.split("/")
    if (any(part in ("", ".", "..") for part in parts)
            or parts[:len(root_parts)] != root_parts):
        raise FetchFailure(f"invalid {label} path: {path!r}")
    return "/".join(parts)


def validate_case_path(path):
    return _validate_posix_path(path, "tests/cases", "test source")


def validate_baseline_path(path):
    return _validate_posix_path(path, "tests/baselines/reference", "baseline")


def validate_case_relpath(rel):
    if not isinstance(rel, str) or not rel or "\\" in rel or rel.startswith("/"):
        raise FetchFailure(f"invalid corpus test path: {rel!r}")
    parts = rel.split("/")
    if any(part in ("", ".", "..") for part in parts):
        raise FetchFailure(f"invalid corpus test path: {rel!r}")
    return "/".join(parts)


def stage_path(stage, rel):
    """Map a validated corpus-relative path into the staging tree safely."""
    rel = validate_case_relpath(rel)
    root = os.path.realpath(stage)
    candidate = os.path.realpath(os.path.join(root, *rel.split("/")))
    try:
        contained = os.path.commonpath((root, candidate)) == root
    except ValueError:
        contained = False
    if not contained:
        raise FetchFailure(f"corpus staging path escapes its root: {rel!r}")
    return candidate


def gh_list_dir(path, *, configured_root=False):
    """List a repo dir at the pinned SHA via gh (authenticated, reliable).
    Returns list of (type, name, path)."""
    path = validate_case_path(path)
    r = subprocess.run(
        ["gh", "api", f"repos/{REPO}/contents/{path}?ref={PINNED_SHA}",
         "--jq", r'.[] | [.type, .name, .path] | @tsv'],
        capture_output=True, text=True)
    if r.returncode != 0:
        detail = r.stderr.strip().splitlines()[-1] if r.stderr.strip() else "no stderr"
        raise FetchFailure(f"cannot list pinned directory {path}: {detail}")
    rows = []
    for line in r.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) != 3 or parts[0] not in ("dir", "file") or not all(parts):
            raise FetchFailure(f"malformed pinned directory response for {path}: {line!r}")
        typ, name, full = parts
        if name in (".", "..") or "/" in name or "\\" in name:
            raise FetchFailure(f"malformed pinned directory response for {path}: {line!r}")
        full = validate_case_path(full)
        if full != f"{path}/{name}":
            raise FetchFailure(f"malformed pinned directory response for {path}: {line!r}")
        rows.append((typ, name, full))
    if configured_root and not rows:
        raise FetchFailure(f"configured pinned directory is empty: {path}")
    return rows


def collect_ts_paths(dirs):
    """BFS the configured dirs, returning all .ts file repo-paths."""
    found = []
    stack = [(validate_case_path(f"tests/cases/{d}"), True) for d in dirs]
    while stack:
        path, configured_root = stack.pop()
        for typ, name, full in gh_list_dir(path, configured_root=configured_root):
            full = validate_case_path(full)
            if typ == "dir":
                stack.append((full, False))
            elif typ == "file" and name.endswith(".ts"):
                found.append(full)
    return sorted(set(found))


def raw_url(path):
    return f"https://raw.githubusercontent.com/{REPO}/{PINNED_SHA}/{path}"


def fetch_blob(path, *, allow_missing=False):
    """Fetch one pinned blob.

    A listed test source must exist. A missing `.errors.txt` is intentional for
    expected-clean tests and is the sole allowed per-file 404.
    """
    if path.startswith("tests/cases/"):
        path = validate_case_path(path)
    elif path.startswith("tests/baselines/reference/"):
        path = validate_baseline_path(path)
    else:
        raise FetchFailure(f"invalid pinned blob path: {path!r}")
    try:
        with urllib.request.urlopen(raw_url(path), timeout=30) as r:
            status = getattr(r, "status", 200)
            if status != 200:
                raise FetchFailure(f"unexpected pinned blob status {status}: {path}")
            content_type = r.headers.get_content_type() if getattr(r, "headers", None) else None
            if content_type in ("application/json", "text/html"):
                raise FetchFailure(f"unexpected pinned blob transport {content_type}: {path}")
            try:
                text = r.read().decode("utf-8")
            except UnicodeDecodeError as e:
                raise FetchFailure(f"non-text pinned blob transport: {path}") from e
            if text.lstrip().lower().startswith(("<!doctype html", "<html")):
                raise FetchFailure(f"HTML returned for pinned blob: {path}")
            return text
    except urllib.error.HTTPError as e:
        optional_baseline = (path.startswith("tests/baselines/reference/")
                             and path.endswith(".errors.txt"))
        if e.code == 404 and allow_missing and optional_baseline:
            return None
        if e.code == 404:
            raise FetchFailure(f"listed pinned blob is missing: {path}") from e
        raise FetchFailure(f"cannot fetch pinned blob {path}: HTTP {e.code}") from e


def cmd_fetch(args):
    dirs = args.dir or DEFAULT_DIRS
    print(f"Listing {len(dirs)} dir(s) at {REPO}@{PINNED_SHA[:9]} ...")
    ts_paths = collect_ts_paths(dirs)
    if args.limit:
        ts_paths = ts_paths[:args.limit]
    if not ts_paths:
        raise FetchFailure("configured fetch selected no TypeScript tests")
    print(f"Found {len(ts_paths)} test files; fetching tests + baselines ...")

    partial = bool(args.dir or args.limit)
    manifest = {
        "format": 2, "sha": PINNED_SHA, "repo": REPO, "dirs": dirs,
        "limit": args.limit, "mode": "partial" if partial else "full-default",
        "partial": partial, "tests": [],
    }
    stage = tempfile.mkdtemp(prefix=".corpus-fetch-", dir=os.path.dirname(CORPUS))

    def fetch_one(ts_repo_path):
        # ts_repo_path like tests/cases/conformance/.../foo.ts
        ts_repo_path = validate_case_path(ts_repo_path)
        rel = validate_case_relpath(ts_repo_path.removeprefix("tests/cases/"))
        dst_ts = stage_path(stage, rel)
        os.makedirs(os.path.dirname(dst_ts), exist_ok=True)
        src = fetch_blob(ts_repo_path)
        with open(dst_ts, "w") as f:
            f.write(src)
        base_name = os.path.basename(ts_repo_path)[:-3] + ".errors.txt"
        baseline_source = validate_baseline_path(f"tests/baselines/reference/{base_name}")
        base = fetch_blob(baseline_source, allow_missing=True)
        if base is not None:
            with open(dst_ts[:-3] + ".errors.txt", "w") as f:
                f.write(base)
        return {"path": rel, "source": ts_repo_path,
                "baseline_source": baseline_source, "baseline": base is not None}

    try:
        with ThreadPoolExecutor(max_workers=12) as ex:
            for done, entry in enumerate(ex.map(fetch_one, ts_paths), start=1):
                manifest["tests"].append(entry)
                if done % 25 == 0:
                    print(f"  ... {done}/{len(ts_paths)}")
        manifest["tests"].sort(key=lambda entry: entry["path"])
        with open(os.path.join(stage, "manifest.json"), "w") as f:
            json.dump(manifest, f, indent=2)
        publish_corpus(stage)
    except Exception:
        shutil.rmtree(stage, ignore_errors=True)
        raise
    n_base = sum(1 for t in manifest["tests"] if t["baseline"])
    print(f"Done: {len(manifest['tests'])} tests in ./corpus ({n_base} with an errors baseline, "
          f"{len(manifest['tests']) - n_base} expected-clean).")


def publish_corpus(stage):
    """Replace the published corpus only after a complete staged fetch succeeds."""
    backup = None
    try:
        if os.path.exists(CORPUS):
            backup = tempfile.mkdtemp(prefix=".corpus-previous-", dir=os.path.dirname(CORPUS))
            os.rmdir(backup)
            os.replace(CORPUS, backup)
        os.replace(stage, CORPUS)
    except Exception:
        if backup and os.path.exists(backup) and not os.path.exists(CORPUS):
            os.replace(backup, CORPUS)
        raise
    else:
        if backup:
            try:
                shutil.rmtree(backup)
            except OSError as e:
                sys.stderr.write(
                    f"WARNING: published new corpus, but could not remove previous corpus "
                    f"backup {backup}: {e}. The new corpus is live; retry cleanup manually.\n")


def load_manifest():
    """Load and structurally validate the published corpus manifest."""
    path = os.path.join(CORPUS, "manifest.json")
    try:
        with open(path) as f:
            manifest = json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        raise FetchFailure("invalid or missing corpus manifest — run `tsofficial.py fetch`") from e
    required = {"format", "sha", "repo", "dirs", "limit", "mode", "partial", "tests"}
    if not isinstance(manifest, dict) or required - manifest.keys() or manifest["format"] != 2:
        raise FetchFailure("corpus manifest has an unsupported format")
    tests = manifest["tests"]
    if not isinstance(tests, list) or not tests:
        raise FetchFailure("corpus manifest has no tests")
    paths = []
    expected_files = {"manifest.json"}
    for entry in tests:
        if (not isinstance(entry, dict)
                or set(entry) != {"path", "source", "baseline_source", "baseline"}
                or not isinstance(entry["path"], str) or not isinstance(entry["source"], str)
                or not isinstance(entry["baseline_source"], str)
                or not isinstance(entry["baseline"], bool)
                or validate_case_relpath(entry["path"]) != entry["path"]
                or validate_case_path(entry["source"]) != entry["source"]
                or entry["source"] != f"tests/cases/{entry['path']}"):
            raise FetchFailure("corpus manifest has an invalid test entry")
        expected_baseline = validate_baseline_path(
            "tests/baselines/reference/" + os.path.basename(entry["path"])[:-3] + ".errors.txt")
        if entry["baseline_source"] != expected_baseline:
            raise FetchFailure("corpus manifest has an invalid baseline entry")
        paths.append(entry["path"])
        expected_files.add(entry["path"])
        if entry["baseline"]:
            expected_files.add(entry["path"][:-3] + ".errors.txt")
    if len(paths) != len(set(paths)):
        raise FetchFailure("corpus manifest has duplicate test paths")
    filesystem_paths = {
        os.path.relpath(os.path.join(root, fn), CORPUS).replace(os.sep, "/")
        for root, _dirs, files in os.walk(CORPUS) for fn in files
    }
    if filesystem_paths != expected_files:
        raise FetchFailure("corpus filesystem does not exactly match manifest files")
    for entry in tests:
        ts_path = os.path.join(CORPUS, entry["path"])
        baseline = ts_path[:-3] + ".errors.txt"
        if entry["baseline"] != os.path.exists(baseline):
            raise FetchFailure(f"baseline presence disagrees with manifest: {entry['path']}")
    return manifest


SCOREBOARD_SHA_RE = re.compile(r"^# TS @ ([0-9a-f]{40})$")


def validate_scoreboard_metadata():
    """Reject a missing, malformed, or stale ratchet input before a run writes."""
    try:
        with open(SCOREBOARD) as f:
            headers = [line.rstrip("\n") for line in f if line.startswith("# TS @")]
    except OSError as e:
        raise FetchFailure("missing scoreboard.txt for ratchet operation") from e
    if len(headers) != 1:
        raise FetchFailure("scoreboard.txt has missing or malformed TS SHA metadata")
    match = SCOREBOARD_SHA_RE.fullmatch(headers[0])
    if not match:
        raise FetchFailure("scoreboard.txt has missing or malformed TS SHA metadata")
    if match.group(1) != PINNED_SHA:
        raise FetchFailure("scoreboard.txt TS SHA does not match the pinned corpus")


def validate_full_default_manifest():
    """Require a complete format-2 default corpus before any scoreboard write."""
    manifest = load_manifest()
    if (manifest["sha"] != PINNED_SHA or manifest["repo"] != REPO
            or manifest["dirs"] != DEFAULT_DIRS or manifest["limit"] is not None
            or manifest["mode"] != "full-default" or manifest["partial"]):
        raise FetchFailure("--check/--save require a fresh full default corpus manifest")
    return manifest


def validate_ratchet_manifest():
    """Require full default corpus and matching scoreboard before check or save."""
    manifest = validate_full_default_manifest()
    validate_scoreboard_metadata()
    board = read_scoreboard()
    corpus_paths = {entry["path"] for entry in manifest["tests"]}
    if set(board) != corpus_paths:
        raise FetchFailure("--check/--save corpus manifest and scoreboard.txt are incomplete or stale")
    return manifest


def validate_check_manifest():
    """Backward-compatible name for callers that run the check ratchet."""
    return validate_ratchet_manifest()


# --- run ---------------------------------------------------------------------

def cmd_run(args):
    binary = args.bin
    if not os.path.exists(binary):
        sys.exit(f"binary not found: {binary} (build it or pass --bin)")
    rebaseline = getattr(args, "rebaseline", False)
    if rebaseline and not args.save:
        sys.exit("--rebaseline is only valid together with --save")
    if args.save and args.check:
        sys.exit("--save and --check cannot be combined")

    if args.limit and (args.check or args.save):
        sys.exit("--check/--save require the full corpus (they enforce scoreboard "
                 "completeness in both directions); drop --limit.")

    try:
        if rebaseline:
            manifest = validate_full_default_manifest()
        elif args.check or args.save:
            manifest = validate_ratchet_manifest()
        else:
            manifest = load_manifest()
    except FetchFailure as e:
        sys.exit(f"corpus integrity failure: {e}")

    tests = manifest_tests(manifest)
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
               "matched_detail": [], "fn_detail": [], "fp_detail": [],
               "incomplete_detail": []}

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

        try:
            parse_errors, diags, incompletes = run_typokat(binary, content, rel)
        except HarnessFailure as e:
            sys.exit(f"\nHARNESS FAILURE (aborting — never scored as success):\n  {e}")

        # Exit-3 discovery: typokat recorded an in-scope surface it does not yet check.
        # Demote to OOS:unsupported but KEEP the full diagnostic diff, so a diagnostic
        # regression inside a now-unsupported test stays visible (the demotion must not
        # blind the harness). The incomplete identities ride alongside the diff.
        # Checked before parse-error deliberately: incompletes come only from exit 3
        # (run_typokat enforces consistency), and exit 3 outranks everything else — if
        # the CLI ever emitted both, the unsupported bucket must win over parse-error.
        if incompletes:
            d = diff(expected, diags)
            rec.update(matched=d["matched"], fn=d["fn"], fp=d["fp"],
                       matched_detail=d["matched_detail"],
                       fn_detail=d["fn_detail"], fp_detail=d["fp_detail"],
                       incomplete_detail=sorted(incompletes))
            rec["bucket"] = "unsupported"
            results.append(rec); continue

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
                   matched_detail=d["matched_detail"],
                   fn_detail=d["fn_detail"], fp_detail=d["fp_detail"])
        results.append(rec)

    report = print_dashboard(binary, results)

    if args.save:
        path = write_scoreboard(results)
        print(f"  saved baseline → {path}")
        report["ratchet"] = {"checked": False, "verdict": "saved", "exit_code": 0,
                             "regressions": 0, "progress": 0,
                             "missing_from_corpus": 0, "missing_from_scoreboard": 0}
    if args.check:
        passed, summary = compare_scoreboard(results, with_summary=True)
        report["ratchet"] = {"checked": True, "verdict": "pass" if passed else "fail",
                             "exit_code": 0 if passed else 1, **summary}
        write_report(report)
        if not passed:
            sys.exit(1)
    else:
        report.setdefault("ratchet", {"checked": False, "verdict": "unchecked", "exit_code": 0,
                                        "regressions": 0, "progress": 0,
                                        "missing_from_corpus": 0, "missing_from_scoreboard": 0})
        write_report(report)


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

    return {
        "when": now, "sha": PINNED_SHA, "binary": binary,
        "corpus": n, "in_scope": n_in, "buckets": dict(buckets),
        "files": inscope,
    }


def write_report(report):
    """Persist only a post-verdict dashboard/report."""
    os.makedirs(REPORT, exist_ok=True)
    with open(os.path.join(REPORT, "latest.json"), "w") as f:
        json.dump(report, f, indent=2)
    print(f"  wrote {os.path.join(REPORT, 'latest.json')}")


# --- regression scoreboard (committed, deterministic) ------------------------

def _scoreboard_stats(results):
    inscope = [r for r in results if r["bucket"] is None]
    unsupported = [r for r in results if r["bucket"] == "unsupported"]
    clean = [r for r in inscope if r["expected"] == 0]
    err = [r for r in inscope if r["expected"] > 0]
    clean_kept = sum(1 for r in clean if r["fp"] == 0)
    err_exact = sum(1 for r in err if r["fn"] == 0 and r["fp"] == 0)
    # diag-recall deliberately spans the IN↔unsupported boundary: an exit-3 demotion
    # keeps its diagnostic diff, so a test moving IN→unsupported must not vanish from
    # the recall denominator (WU2). clean-kept / error-exact stay in-scope-only (they
    # are file-level exactness metrics for the diffed set).
    recall_rows = err + [r for r in unsupported if r["expected"] > 0]
    rec_m = sum(r["matched"] for r in recall_rows)
    rec_t = rec_m + sum(r["fn"] for r in recall_rows)
    return inscope, clean, err, clean_kept, err_exact, rec_m, rec_t


def _fmt_ids(pairs):
    """Serialize a list of (line, code) identities to a stable, multiplicity-
    preserving string: sorted, `line:code` tokens joined by commas."""
    return ",".join(f"{ln}:{c}" for ln, c in sorted(pairs))


def _check_inc_id(inc_id):
    """Hard-fail on an incomplete surface id that does not match the stable
    `role/surface/slot-or-variant` kebab-case shape. An id containing `,`, `|`, or a
    TAB would corrupt the scoreboard's column format on write or read, so the board
    is never silently corrupted by a future emission (WU7-B hardening)."""
    if not INC_ID_RE.match(inc_id):
        raise ValueError(
            f"invalid incomplete surface id {inc_id!r}: expected "
            f"role/surface/slot-or-variant (kebab-case segments)")


def _fmt_inc(ids):
    """Serialize incomplete surface identities (stable id strings) to a stable,
    multiplicity-preserving string: sorted, comma-joined. Ids are validated first —
    a malformed id must never be written into the scoreboard."""
    for inc_id in ids:
        _check_inc_id(inc_id)
    return ",".join(sorted(ids))


def _parse_ids(field):
    """Inverse of the `<matched>|<fp>[|<incomplete>]` identity column. Returns
    (matched, fp, incomplete): matched/fp as lists of (line, code) with multiplicity;
    incomplete as a list of id strings. Returns (None, None, None) when identity was
    not recorded (`-`, i.e. a non-diffed out-of-scope or pre-identity-format line).
    A two-segment field (IN lines) yields an empty incomplete list, so those round-trip
    byte-for-byte."""
    if field in ("-", ""):
        return None, None, None
    parts = field.split("|")
    m_str = parts[0] if len(parts) > 0 else ""
    f_str = parts[1] if len(parts) > 1 else ""
    i_str = parts[2] if len(parts) > 2 else ""

    def parse_half(s):
        out = []
        for tok in s.split(","):
            if not tok:
                continue
            ln, _, c = tok.partition(":")
            out.append((int(ln), int(c)))
        return out

    inc = [tok for tok in i_str.split(",") if tok]
    for tok in inc:
        # A malformed id in the committed board means the file is corrupt — fail loudly.
        _check_inc_id(tok)
    return parse_half(m_str), parse_half(f_str), inc


def _diag_delta(b, c):
    """Compare matched/fp diagnostic identities of a diffed (IN or unsupported)
    base/cur pair. Returns (regress_msg, progress_msg); either may be None. This is
    the same identity discipline for IN↔IN and unsupported↔unsupported — a demotion
    keeps its diff, so a dropped matched identity inside an unsupported test regresses."""
    if b["matched_ids"] is None:
        # Pre-identity-format baseline line: fall back to count comparison.
        if c["matched"] < b["matched"] or c["fp"] > b["fp"]:
            return (f"matched {b['matched']}→{c['matched']}, fp {b['fp']}→{c['fp']}", None)
        if c["matched"] > b["matched"] or c["fp"] < b["fp"]:
            return (None, f"matched {b['matched']}→{c['matched']}, fp {b['fp']}→{c['fp']}")
        return (None, None)
    base_m = Counter(b["matched_ids"])
    cur_m = Counter(c["matched_detail"])
    base_fp = Counter(b["fp_ids"])
    cur_fp = Counter(c["fp_detail"])
    lost = base_m - cur_m
    new_fp = cur_fp - base_fp
    if lost or new_fp:
        bits = []
        if lost:
            bits.append("dropped " + _fmt_ids(lost.elements()))
        if new_fp:
            bits.append("new fp " + _fmt_ids(new_fp.elements()))
        return ("; ".join(bits), None)
    gained = cur_m - base_m
    removed_fp = base_fp - cur_fp
    if gained or removed_fp:
        bits = []
        if gained:
            bits.append("matched " + _fmt_ids(gained.elements()))
        if removed_fp:
            bits.append("fixed fp " + _fmt_ids(removed_fp.elements()))
        return (None, "; ".join(bits))
    return (None, None)


def _inc_delta(b, c):
    """Compare incomplete surface identities for an unsupported base/cur pair. A
    dropped incomplete identity is a regression (the accounting silently stopped
    recording a surface); a gained one is progress. Returns (regress_msg, progress_msg)."""
    base_i = Counter(b.get("incomplete_ids") or [])
    cur_i = Counter(c.get("incomplete_detail") or [])
    lost = base_i - cur_i
    gained = cur_i - base_i
    reg = ("dropped incomplete " + ",".join(sorted(lost.elements()))) if lost else None
    pro = ("incomplete " + ",".join(sorted(gained.elements()))) if gained else None
    return (reg, pro)


def write_scoreboard(results):
    """Write the committed baseline: one sorted line per test, no timestamps or
    machine paths, so a regression shows up as a single changed line in `git diff`.
    Out-of-scope tests are recorded too (so scope flips are tracked). Each in-scope
    line also carries the *identity* of its matched and false-positive diagnostics
    (`<matched>|<fp>` as sorted `line:code` tokens) so the ratchet compares which
    diagnostics match, not just how many — a same-count swap is a regression."""
    rows = sorted(results, key=lambda r: r["rel"])
    inscope, clean, err, clean_kept, err_exact, rec_m, rec_t = _scoreboard_stats(rows)
    with open(SCOREBOARD, "w") as f:
        f.write("# typokat × official TS conformance — regression scoreboard\n")
        f.write(f"# TS @ {PINNED_SHA}\n")
        f.write(f"# corpus {len(rows)}  in-scope {len(inscope)}  "
                f"out-of-scope {len(rows) - len(inscope)}\n")
        f.write(f"# clean-kept {clean_kept}/{len(clean)}  "
                f"error-exact {err_exact}/{len(err)}  diag-recall {rec_m}/{rec_t}\n")
        f.write("# cols: status<TAB>matched fn fp expected<TAB>ids<TAB>rel  "
                "(status=IN | OOS:<bucket>; nums/ids '-' = n/a; "
                "ids=<matched>|<fp> as sorted line:code identities; "
                "OOS:unsupported carries nums + <matched>|<fp>|<incomplete> — an exit-3 "
                "demotion keeps its diagnostic diff plus its surface identities)\n")
        for r in rows:
            if r["bucket"] is None:
                ids = f"{_fmt_ids(r['matched_detail'])}|{_fmt_ids(r['fp_detail'])}"
                f.write(f"IN\t{r['matched']} {r['fn']} {r['fp']} {r['expected']}"
                        f"\t{ids}\t{r['rel']}\n")
            elif r["bucket"] == "unsupported":
                # Exit-3 demotion: keep the diagnostic diff AND the incomplete identities
                # so the ratchet still catches a regression inside a now-unsupported test.
                ids = (f"{_fmt_ids(r['matched_detail'])}|{_fmt_ids(r['fp_detail'])}"
                       f"|{_fmt_inc(r.get('incomplete_detail', []))}")
                f.write(f"OOS:unsupported\t{r['matched']} {r['fn']} {r['fp']} {r['expected']}"
                        f"\t{ids}\t{r['rel']}\n")
            else:
                f.write(f"OOS:{r['bucket']}\t- - - -\t-\t{r['rel']}\n")
    return SCOREBOARD


def read_scoreboard():
    out = {}
    with open(SCOREBOARD) as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            parts = line.rstrip("\n").split("\t")
            # New format: status<TAB>nums<TAB>ids<TAB>rel (4 fields). The pre-
            # identity format (3 fields) is still read, with identity absent.
            if len(parts) == 4:
                status, nums, ids, rel = parts
            elif len(parts) == 3:
                status, nums, rel = parts
                ids = "-"
            else:
                continue

            def num(x):
                return None if x == "-" else int(x)
            m, fn, fp, exp = (num(x) for x in nums.split())
            matched_ids, fp_ids, incomplete_ids = _parse_ids(ids)
            out[rel] = {"status": status, "matched": m, "fn": fn, "fp": fp,
                        "expected": exp, "matched_ids": matched_ids,
                        "fp_ids": fp_ids, "incomplete_ids": incomplete_ids}
    return out


def compare_scoreboard(results, *, with_summary=False):
    """Diff the current run against the committed baseline by stable diagnostic
    identity. Returns True iff there are no regressions.

    A regression is any of:
      * a previously-matched diagnostic identity (line+code) no longer matched, or a
        new false-positive identity (so a same-count swap of *which* diagnostics
        match is caught, not hidden);
      * a previously in-scope file that fell out of scope;
      * a scoreboard entry missing from the checked corpus, or a corpus file missing
        from the scoreboard (completeness is enforced in both directions)."""
    if not os.path.exists(SCOREBOARD):
        print("\n  no committed scoreboard.txt yet — run `run --save` to create one.")
        summary = {"regressions": 0, "progress": 0, "missing_from_corpus": 0,
                   "missing_from_scoreboard": 0}
        return (True, summary) if with_summary else True
    base = read_scoreboard()
    cur = {r["rel"]: r for r in results}
    regress, progress = [], []
    missing_from_corpus = []   # in scoreboard, absent from this run's corpus
    missing_from_board = []    # in corpus, absent from the scoreboard

    for rel in sorted(set(base) | set(cur)):
        b = base.get(rel)
        c = cur.get(rel)
        if c is None:
            missing_from_corpus.append(rel)
            regress.append((rel, "in scoreboard.txt but absent from corpus "
                                 "(re-fetch at the pinned SHA)"))
            continue
        if b is None:
            missing_from_board.append(rel)
            regress.append((rel, "in corpus but absent from scoreboard.txt "
                                 "(run --save to record it)"))
            continue

        base_in = b["status"] == "IN"
        base_unsup = b["status"] == "OOS:unsupported"
        cur_in = c["bucket"] is None
        cur_unsup = c["bucket"] == "unsupported"

        # Both IN and OOS:unsupported carry a comparable diagnostic diff; only these
        # two statuses are "diffed". Transitions in/out of the diffed set are the
        # coverage regress/progress signals — but an exit-3 demotion (IN→unsupported)
        # is a regression *and* its diagnostic drops are still surfaced.
        def diffed_delta(with_incomplete):
            reg, pro = _diag_delta(b, c)
            if reg:
                regress.append((rel, reg))
            if pro:
                progress.append((rel, pro))
            if with_incomplete:
                ireg, ipro = _inc_delta(b, c)
                if ireg:
                    regress.append((rel, ireg))
                if ipro:
                    progress.append((rel, ipro))

        if base_in and cur_in:
            diffed_delta(with_incomplete=False)
        elif base_unsup and cur_unsup:
            diffed_delta(with_incomplete=True)
        elif base_in and cur_unsup:
            regress.append((rel, "IN → OOS:unsupported (lost coverage)"))
            diffed_delta(with_incomplete=False)
        elif base_unsup and cur_in:
            progress.append((rel, "OOS:unsupported → IN (gained coverage)"))
            diffed_delta(with_incomplete=False)
        elif (base_in or base_unsup) and not (cur_in or cur_unsup):
            regress.append((rel, f"{b['status']} → OOS:{c['bucket']} (lost coverage)"))
        elif not (base_in or base_unsup) and cur_in:
            progress.append((rel, f"{b['status']} → IN (gained coverage)"))
        elif not (base_in or base_unsup) and cur_unsup:
            progress.append((rel, f"{b['status']} → OOS:unsupported (gained accounting)"))

    print(f"\n  regression check vs scoreboard.txt — "
          f"regressions: {len(regress)}   progress: {len(progress)}   "
          f"missing-from-corpus: {len(missing_from_corpus)}   "
          f"missing-from-scoreboard: {len(missing_from_board)}")
    for rel, msg in sorted(regress)[:40]:
        print(f"    ✗ REGRESS  {rel}  ({msg})")
    for rel, msg in sorted(progress)[:15]:
        print(f"    ✓ progress {rel}  ({msg})")
    if len(progress) > 15:
        print(f"    … +{len(progress) - 15} more improved")
    passed = len(regress) == 0
    summary = {"regressions": len(regress), "progress": len(progress),
               "missing_from_corpus": len(missing_from_corpus),
               "missing_from_scoreboard": len(missing_from_board)}
    return (passed, summary) if with_summary else passed


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
    pr.add_argument("--rebaseline", action="store_true",
                    help="with --save, intentionally replace a stale/missing scoreboard")
    pr.add_argument("--check", action="store_true",
                    help="compare against scoreboard.txt; exit 1 on any regression")
    pr.set_defaults(func=cmd_run)

    args = p.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()

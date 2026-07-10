//! Validator for the structured divergence census in
//! `docs/reference/divergences.md`.
//!
//! `divergences.md` is BOTH the human ledger and the machine-checked census
//! (completion-manifest criterion `C-deferred-divergence-census`, backlog `75`).
//! Every divergence entry carries a one-line HTML-comment marker:
//!
//! ```text
//! <!-- div: id=<slug> dir=<under|over|cosmetic> scope=<family|design-oos> owner=<owner> witness=<path> -->
//! ```
//!
//! This test parses those markers and enforces: unmarked divergence rows,
//! duplicate ids, bad `dir`/`scope` enums, dead/missing owner or witness links,
//! and any `under`-report without a live backlog owner all **fail** `cargo test`
//! (and therefore CI). Style mirrors the hand-rolled `tests/manifest.rs`: no
//! serde/regex, aggregated errors, table-driven mutation witnesses.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

const DIRS: &[&str] = &["under", "over", "cosmetic"];
const MARKER_KEYS: &[&str] = &["id", "dir", "scope", "owner", "witness"];
const DESIGN_OOS: &str = "design-oos";
const REGION_HEADING: &str = "## Cross-cutting conventions";

/// A divergence sentinel makes a list item a divergence row that MUST be marked.
const SENTINELS: &[&str] = &[
    "over-report",
    "under-report",
    "dropped error",
    "deferred",
    "out of scope",
    "cosmetic",
    "skipped",
    "not emitted",
    "not reported",
    "not produced",
];

/// Rows that explicitly disclaim being a divergence are exempt from marking.
const NEGATIVE_SENTINELS: &[&str] = &["not a tsc divergence", "not a corpus divergence"];

/// The only sentinel-bearing bullets allowed ABOVE the ledger region: the two
/// definitional entry-kind bullets in the intro. Anything else up there is a
/// divergence smuggled past the census and must move below `REGION_HEADING`.
const PREAMBLE_ALLOWED: &[&str] = &["- **Over-report (divergence)**", "- **Deferred check**"];

struct Marker {
    id: String,
    dir: String,
    scope: String,
    owner: String,
    witness: String,
    line: usize,
}

/// One scanned (non-fenced) line; `pre` marks the definitional preamble above
/// `REGION_HEADING`.
struct Line {
    no: usize,
    trimmed: String,
    indent: usize,
    is_marker: bool,
    pre: bool,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate(
    text: &str,
    base_dir: &Path,
    scope_families: &BTreeSet<String>,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let scanned = scan_region(text);

    // --- parse + field-check every marker ---
    let mut markers = Vec::new();
    let mut seen_ids: BTreeMap<String, usize> = BTreeMap::new();
    for l in scanned.iter().filter(|l| l.is_marker) {
        if let Some(m) = parse_marker(&l.trimmed, l.no, &mut errors) {
            if !m.id.is_empty() {
                if let Some(prev) = seen_ids.insert(m.id.clone(), m.line) {
                    errors.push(format!(
                        "line {}: duplicate divergence id {:?} (also line {prev})",
                        m.line, m.id
                    ));
                }
            }
            check_fields(&m, base_dir, scope_families, &mut errors);
            markers.push(m);
        }
    }

    if markers.is_empty() {
        errors.push("no divergence markers found — the census cannot be empty".to_string());
    }

    // --- unmarked divergence rows + preamble smuggling ---
    check_unmarked(&scanned, &mut errors);
    check_preamble(&scanned, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Collect every non-fenced line. Lines before `REGION_HEADING` (or none, if the
/// heading is absent) are flagged `pre` — the definitional preamble, which gets
/// its own no-smuggling check instead of the per-row marker check.
fn scan_region(text: &str) -> Vec<Line> {
    let has_region = text.contains(REGION_HEADING);
    let mut in_pre = has_region;
    let mut in_fence = false;
    let mut out = Vec::new();

    for (i, raw) in text.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if in_pre && trimmed == REGION_HEADING {
            in_pre = false;
        }
        let indent = raw.len() - raw.trim_start().len();
        out.push(Line {
            no: i + 1,
            trimmed: trimmed.to_string(),
            indent,
            is_marker: trimmed.starts_with("<!-- div"),
            pre: in_pre,
        });
    }
    out
}

/// Parse `<!-- div: k=v k=v ... -->`. Pushes errors for malformed/missing keys.
fn parse_marker(trimmed: &str, line: usize, errors: &mut Vec<String>) -> Option<Marker> {
    let inner = trimmed
        .strip_prefix("<!--")
        .and_then(|s| s.strip_suffix("-->"))
        .map(str::trim)?;
    let inner = inner.strip_prefix("div").unwrap_or(inner);
    let inner = inner.trim_start_matches(':').trim();

    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for tok in inner.split_whitespace() {
        match tok.split_once('=') {
            Some((k, v)) => {
                if map.insert(k.to_string(), v.to_string()).is_some() {
                    errors.push(format!("line {line}: duplicate marker key {k:?}"));
                }
            }
            None => errors.push(format!(
                "line {line}: malformed marker token {tok:?} (want key=value)"
            )),
        }
    }
    for key in map.keys() {
        if !MARKER_KEYS.contains(&key.as_str()) {
            errors.push(format!(
                "line {line}: unknown marker key {key:?} — allowed keys are {MARKER_KEYS:?}"
            ));
        }
    }
    for key in MARKER_KEYS {
        match map.get(*key) {
            Some(v) if !v.is_empty() => {}
            _ => errors.push(format!(
                "line {line}: missing or empty marker field {key:?}"
            )),
        }
    }
    Some(Marker {
        id: map.get("id").cloned().unwrap_or_default(),
        dir: map.get("dir").cloned().unwrap_or_default(),
        scope: map.get("scope").cloned().unwrap_or_default(),
        owner: map.get("owner").cloned().unwrap_or_default(),
        witness: map.get("witness").cloned().unwrap_or_default(),
        line,
    })
}

fn check_fields(
    m: &Marker,
    base_dir: &Path,
    scope_families: &BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    let at = format!("divergence {:?} (line {})", m.id, m.line);

    // Lowercase-only ids: duplicates cannot hide behind case differences.
    if !m.id.is_empty() && !is_divergence_id(&m.id) {
        errors.push(format!(
            "{at}: id must be lowercase kebab segments separated by '/' (e.g. area/topic-detail)"
        ));
    }

    if !m.dir.is_empty() && !DIRS.contains(&m.dir.as_str()) {
        errors.push(format!("{at}: dir {:?} not one of {DIRS:?}", m.dir));
    }

    if !m.scope.is_empty() && m.scope != DESIGN_OOS && !scope_families.contains(&m.scope) {
        errors.push(format!(
            "{at}: scope {:?} is not a scope.md family or {DESIGN_OOS:?}",
            m.scope
        ));
    }

    // owner: a live backlog path, or design-oos (never design-oos for under).
    if m.owner == DESIGN_OOS {
        if m.dir == "under" {
            errors.push(format!(
                "{at}: an under-report must name a live backlog owner, not {DESIGN_OOS:?}"
            ));
        }
    } else if !m.owner.is_empty() {
        check_link(base_dir, &m.owner, &format!("{at}.owner"), errors);
    }

    // witness: an existing corpus/tooling path.
    if !m.witness.is_empty() {
        check_link(base_dir, &m.witness, &format!("{at}.witness"), errors);
    }
}

/// Segments of `[a-z0-9-]` joined by `/`, at least two, no edge hyphens.
fn is_divergence_id(id: &str) -> bool {
    let segments: Vec<&str> = id.split('/').collect();
    segments.len() >= 2
        && segments.iter().all(|s| {
            !s.is_empty()
                && !s.starts_with('-')
                && !s.ends_with('-')
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// A list item carrying a divergence sentinel must have a marker inside its block
/// (the item's own lines plus any deeper-indented children).
fn check_unmarked(scanned: &[Line], errors: &mut Vec<String>) {
    for (i, l) in scanned.iter().enumerate() {
        if l.pre || !l.trimmed.starts_with("- ") {
            continue;
        }
        let lower = l.trimmed.to_lowercase();
        if NEGATIVE_SENTINELS.iter().any(|n| lower.contains(n)) {
            continue;
        }
        if !SENTINELS.iter().any(|s| lower.contains(s)) {
            continue;
        }
        // Block: subsequent lines until a heading or a list item at indent <= this.
        let mut marked = false;
        for next in &scanned[i + 1..] {
            let ends = next.trimmed.starts_with('#')
                || (next.trimmed.starts_with("- ") && next.indent <= l.indent);
            if ends {
                break;
            }
            if next.is_marker {
                marked = true;
                break;
            }
        }
        if !marked {
            errors.push(format!(
                "line {}: unmarked divergence row (sentinel present, no `<!-- div ... -->` marker): {:.60}",
                l.no, l.trimmed
            ));
        }
    }
}

/// The preamble is definitional only: any sentinel-bearing list item there other
/// than the allowlisted entry-kind definitions is a smuggled divergence and must
/// move below `REGION_HEADING` (where the per-row marker check applies).
fn check_preamble(scanned: &[Line], errors: &mut Vec<String>) {
    for l in scanned.iter().filter(|l| l.pre) {
        if !l.trimmed.starts_with("- ") {
            continue;
        }
        let lower = l.trimmed.to_lowercase();
        if NEGATIVE_SENTINELS.iter().any(|n| lower.contains(n)) {
            continue;
        }
        if !SENTINELS.iter().any(|s| lower.contains(s)) {
            continue;
        }
        if PREAMBLE_ALLOWED.iter().any(|p| l.trimmed.starts_with(p)) {
            continue;
        }
        errors.push(format!(
            "line {}: divergence bullet in the definitional preamble — move it below `{REGION_HEADING}`: {:.60}",
            l.no, l.trimmed
        ));
    }
}

/// A link/owner/witness path must start with `./` or `../` and resolve to an
/// existing path (file or directory).
fn check_link(base_dir: &Path, rel: &str, what: &str, errors: &mut Vec<String>) {
    if !(rel.starts_with("./") || rel.starts_with("../")) {
        errors.push(format!(
            "{what}: link {rel:?} must be a relative path (./ or ../)"
        ));
        return;
    }
    let path = base_dir.join(rel);
    if !path.exists() {
        errors.push(format!(
            "{what}: link {rel:?} does not exist ({})",
            path.display()
        ));
    }
}

// ---------------------------------------------------------------------------
// Scope-family set (parsed from scope.md, independent of manifest.rs).
// ---------------------------------------------------------------------------

fn scope_families(scope_text: &str) -> BTreeSet<String> {
    scope_text
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("<!-- scope-family: ")
                .and_then(|rest| rest.strip_suffix(" -->"))
                .map(str::to_string)
        })
        .collect()
}

fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("reference")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// The committed ledger must validate.
// ---------------------------------------------------------------------------

#[test]
fn committed_divergences_validate() {
    let dir = reference_dir();
    let text = read(&dir.join("divergences.md"));
    let families = scope_families(&read(&dir.join("scope.md")));
    if let Err(errors) = validate(&text, &dir, &families) {
        panic!(
            "committed divergences.md failed validation ({}):\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }
}

// ---------------------------------------------------------------------------
// WU0 rejection fixture: the malformed divergence row must be rejected.
// ---------------------------------------------------------------------------

#[test]
fn rejects_wu0_malformed_divergence_fixture() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("surface")
        .join("fixtures");
    let text = read(&fixtures.join("malformed_divergence.md"));
    let families = scope_families(&read(&reference_dir().join("scope.md")));
    match validate(&text, &fixtures, &families) {
        Ok(()) => panic!("WU0 malformed_divergence.md fixture was accepted but must be rejected"),
        Err(errors) => {
            let joined = errors.join(" | ");
            // The malformed row: `dir=sideways`, empty `owner`, no `witness`/`scope`.
            for needle in ["sideways", "owner", "witness"] {
                assert!(
                    joined.contains(needle),
                    "expected rejection to mention {needle:?}, got: {joined}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mutation witnesses (table-driven), built from a controlled valid sample whose
// links resolve against the real docs/reference tree.
// ---------------------------------------------------------------------------

fn valid_sample() -> String {
    concat!(
        "## Cross-cutting conventions\n",
        "\n",
        "- **A safe over-report.** typokat over-reports here.\n",
        "  <!-- div: id=area/over-one dir=over scope=s-assignability owner=design-oos witness=../../tests/cases/m0_assign_primitives -->\n",
        "- **A dropped error (under-report).** typokat drops a real error.\n",
        "  <!-- div: id=area/under-one dir=under scope=s-name-resolution owner=../backlog/52-type-reference-tail.md witness=../../tests/cases/m22_unresolved_type/positions.ts -->\n",
    )
    .to_string()
}

#[test]
fn valid_sample_itself_passes() {
    let families = scope_families(&read(&reference_dir().join("scope.md")));
    assert!(
        validate(&valid_sample(), &reference_dir(), &families).is_ok(),
        "the sample ledger must itself be valid"
    );
}

#[test]
fn rejects_malformed_divergences() {
    let dir = reference_dir();
    let families = scope_families(&read(&dir.join("scope.md")));

    // (name, mutated ledger, substring the aggregated error must mention)
    let over_marker = "  <!-- div: id=area/over-one dir=over scope=s-assignability owner=design-oos witness=../../tests/cases/m0_assign_primitives -->\n";
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "unmarked divergence row",
            valid_sample().replace(over_marker, ""),
            "unmarked divergence row",
        ),
        (
            "duplicate id",
            valid_sample().replace("id=area/under-one", "id=area/over-one"),
            "duplicate divergence id",
        ),
        (
            "missing witness field",
            valid_sample().replace(" witness=../../tests/cases/m22_unresolved_type/positions.ts", ""),
            "missing or empty marker field \"witness\"",
        ),
        (
            "empty owner field",
            valid_sample().replace("owner=../backlog/52-type-reference-tail.md", "owner="),
            "owner",
        ),
        (
            "bad dir enum",
            valid_sample().replace("dir=under", "dir=sideways"),
            "dir \"sideways\" not one of",
        ),
        (
            "bad scope enum",
            valid_sample().replace("scope=s-assignability", "scope=z-bogus"),
            "is not a scope.md family",
        ),
        (
            "dead owner link",
            valid_sample().replace(
                "owner=../backlog/52-type-reference-tail.md",
                "owner=../backlog/999-nope.md",
            ),
            "does not exist",
        ),
        (
            "dead witness link",
            valid_sample().replace(
                "witness=../../tests/cases/m0_assign_primitives",
                "witness=../../tests/cases/nope-fixture.ts",
            ),
            "does not exist",
        ),
        (
            "ownerless under-report",
            valid_sample().replace(
                "id=area/under-one dir=under scope=s-name-resolution owner=../backlog/52-type-reference-tail.md",
                "id=area/under-one dir=under scope=s-name-resolution owner=design-oos",
            ),
            "under-report must name a live backlog owner",
        ),
        (
            "unknown marker key",
            valid_sample().replace("dir=over scope=s-assignability", "dir=over category=x scope=s-assignability"),
            "unknown marker key",
        ),
        (
            // N4: a divergence bullet smuggled into the intro must not escape.
            "divergence smuggled into the preamble",
            format!(
                "# Ledger\n\n- A sneaky deferred divergence smuggled into the intro.\n\n{}",
                valid_sample()
            ),
            "definitional preamble",
        ),
        (
            // N5: ids are lowercase-only, so duplicates cannot hide behind case.
            "non-lowercase id",
            valid_sample().replace("id=area/over-one", "id=Area/Over-One"),
            "lowercase kebab segments",
        ),
        (
            "case-variant duplicate id rejected via case rule",
            valid_sample().replace("id=area/under-one", "id=AREA/OVER-ONE"),
            "lowercase kebab segments",
        ),
    ];

    let mut leaked = Vec::new();
    for (name, text, expect) in cases {
        match validate(&text, &dir, &families) {
            Ok(()) => leaked.push(format!("case {name:?} was accepted but must be rejected")),
            Err(errors) => {
                let joined = errors.join(" | ");
                if !joined.contains(expect) {
                    leaked.push(format!(
                        "case {name:?} rejected, but no error mentioned {expect:?}:\n    {joined}"
                    ));
                }
            }
        }
    }
    assert!(leaked.is_empty(), "{}", leaked.join("\n"));
}

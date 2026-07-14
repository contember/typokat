//! Validator for the executable 1.0 completion manifest
//! (`docs/backlog/completion-1.0.toml`).
//!
//! The manifest is the machine-validated source of truth for "is typokat 1.0
//! done?". This test parses it and enforces the invariants the sprint requires:
//! duplicate/missing ids, unknown backlog/divergence links, missing owners or
//! witnesses, inconsistent states, an unpinned TypeScript lib audit, and gaps
//! between the Tier S/A/B scope map and criterion ownership all **fail**
//! `cargo test` (and therefore CI). The parser is a small hand-rolled
//! TOML subset — no serde/toml dependency — matching the committed-status-file
//! precedent of `tooling/official-suite/scoreboard.txt`.
//!
//! Format: `#` comments and blank lines ignored; `[meta]` opens the singleton
//! meta table; `[[criterion]]` opens a criterion; values are quoted strings
//! `"..."` or arrays of quoted strings `["a", "b"]`. Paths (owner/links/deps) are
//! resolved relative to the manifest's directory and must exist; `scope` arrays
//! map criteria to stable Tier S/A/B family ids declared in scope.md.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Str(String),
    Arr(Vec<String>),
}

impl Value {
    fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_str()),
            Value::Arr(_) => None,
        }
    }
}

/// One `[meta]` or `[[criterion]]` table: an ordered key -> value map.
type Table = BTreeMap<String, Value>;

struct Manifest {
    meta: Vec<Table>,
    criteria: Vec<Table>,
}

/// Parse the manifest text. Returns `Err` with every parse error found.
fn parse(text: &str) -> Result<Manifest, Vec<String>> {
    let mut errors = Vec::new();
    let mut meta: Vec<Table> = Vec::new();
    let mut criteria: Vec<Table> = Vec::new();

    // Section cursor: (kind, index into the owning Vec). `None` before any header.
    enum Cur {
        None,
        Meta,
        Criterion,
    }
    let mut cur = Cur::None;

    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[meta]" {
            meta.push(Table::new());
            cur = Cur::Meta;
            continue;
        }
        if line == "[[criterion]]" {
            criteria.push(Table::new());
            cur = Cur::Criterion;
            continue;
        }
        if line.starts_with('[') {
            errors.push(format!("line {line_no}: unknown section header {line:?}"));
            continue;
        }
        // key = value
        let Some(eq) = line.find('=') else {
            errors.push(format!(
                "line {line_no}: expected `key = value`, got {line:?}"
            ));
            continue;
        };
        let key = line[..eq].trim().to_string();
        let rhs = line[eq + 1..].trim();
        let value = match parse_value(rhs) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("line {line_no}: {e} in {line:?}"));
                continue;
            }
        };
        let table = match cur {
            Cur::Meta => meta.last_mut(),
            Cur::Criterion => criteria.last_mut(),
            Cur::None => {
                errors.push(format!(
                    "line {line_no}: key {key:?} before any section header"
                ));
                continue;
            }
        };
        if let Some(t) = table {
            if t.insert(key.clone(), value).is_some() {
                errors.push(format!("line {line_no}: duplicate key {key:?} in table"));
            }
        }
    }

    if errors.is_empty() {
        Ok(Manifest { meta, criteria })
    } else {
        Err(errors)
    }
}

/// Parse a single RHS value: a quoted string or an array of quoted strings.
fn parse_value(rhs: &str) -> Result<Value, String> {
    if let Some(inner) = rhs.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if inner.is_empty() {
            return Ok(Value::Arr(Vec::new()));
        }
        let mut out = Vec::new();
        for part in inner.split(',') {
            let p = part.trim();
            let s = p
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or_else(|| format!("array element {p:?} must be a quoted string"))?;
            if s.contains('"') {
                return Err(format!("array element {p:?} has an embedded quote"));
            }
            out.push(s.to_string());
        }
        return Ok(Value::Arr(out));
    }
    let s = rhs
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| "value must be a quoted string or array".to_string())?;
    if s.contains('"') {
        return Err("string has an embedded quote".to_string());
    }
    Ok(Value::Str(s.to_string()))
}

#[test]
fn parser_characterization_preserves_manifest_cursor_and_errors() {
    let parsed = parse(concat!(
        "# comment\n",
        "[meta]\n",
        "name = \"alpha\"\n",
        "empty = []\n",
        "items = [\"a\", \"b\"]\n",
        "[[criterion]]\n",
        "name = \"beta\"\n",
    ))
    .expect("strings, arrays, comments, and known tables must parse");
    assert_eq!(parsed.meta.len(), 1);
    assert_eq!(parsed.criteria.len(), 1);
    assert_eq!(
        parsed.meta[0].get("name"),
        Some(&Value::Str("alpha".to_string()))
    );
    assert_eq!(parsed.meta[0].get("empty"), Some(&Value::Arr(Vec::new())));
    assert_eq!(
        parsed.meta[0].get("items"),
        Some(&Value::Arr(vec!["a".to_string(), "b".to_string()]))
    );
    assert_eq!(
        parsed.criteria[0].get("name"),
        Some(&Value::Str("beta".to_string()))
    );

    let invalid = concat!(
        "orphan = \"value\"\n",
        "[meta]\n",
        "key = \"first\"\n",
        "key = \"second\"\n",
        "[unknown]\n",
        "after_unknown = \"kept in meta\"\n",
        "bad = unquoted\n",
        "array = [\"ok\", bad]\n",
        "embedded = \"a\"b\"\n",
    );
    let errors = match parse(invalid) {
        Err(errors) => errors,
        Ok(_) => panic!("all parser failures must be aggregated"),
    };
    assert_eq!(
        errors,
        vec![
            "line 1: key \"orphan\" before any section header",
            "line 4: duplicate key \"key\" in table",
            "line 5: unknown section header \"[unknown]\"",
            "line 7: value must be a quoted string or array in \"bad = unquoted\"",
            "line 8: array element \"bad\" must be a quoted string in \"array = [\\\"ok\\\", bad]\"",
            "line 9: string has an embedded quote in \"embedded = \\\"a\\\"b\\\"\"",
        ]
    );
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

const TRACKS: &[&str] = &["A", "B", "C", "D"];
const STATES: &[&str] = &["complete", "incomplete"];
const BOOLS: &[&str] = &["true", "false"];
const REQUIRED_TS_VERSION: &str = "6.0.3";

/// The only keys a table may carry. A typo'd key (`dpes`, `linsk`) must FAIL
/// validation — silently skipping it would skip its link/state checks.
const META_KEYS: &[&str] = &[
    "schema_version",
    "lib_audit_ts_version",
    "lib_audit_artifact",
];
const CRITERION_KEYS: &[&str] = &[
    "id",
    "track",
    "title",
    "state",
    "blocks_release",
    "owner",
    "witness",
    "links",
    "deps",
    "scope",
    "deps_exception",
];

/// Full validation entry point: parse then check every invariant. `manifest_dir`
/// is the directory the manifest lives in (paths resolve relative to it).
fn validate(text: &str, manifest_dir: &Path) -> Result<(), Vec<String>> {
    let manifest = parse(text)?;
    let mut errors = Vec::new();

    // --- meta ---
    if manifest.meta.len() != 1 {
        errors.push(format!(
            "expected exactly one [meta] table, found {}",
            manifest.meta.len()
        ));
    }
    if let Some(meta) = manifest.meta.first() {
        check_known_keys(meta, META_KEYS, "[meta]", &mut errors);
        require_nonempty_str(meta, "schema_version", "[meta]", &mut errors);

        // Unpinned lib audit is a hard failure.
        match meta.get("lib_audit_ts_version").and_then(Value::as_str) {
            Some(v) if v == REQUIRED_TS_VERSION => {}
            Some(v) => errors.push(format!(
                "[meta].lib_audit_ts_version is {v:?}, must be pinned to {REQUIRED_TS_VERSION:?}"
            )),
            None => errors.push(
                "[meta].lib_audit_ts_version missing — the TypeScript lib audit is unpinned"
                    .to_string(),
            ),
        }
        match meta.get("lib_audit_artifact").and_then(Value::as_str) {
            Some(rel) if !rel.is_empty() => {
                let path = manifest_dir.join(rel);
                if !path.exists() {
                    errors.push(format!(
                        "[meta].lib_audit_artifact {rel:?} does not exist ({})",
                        path.display()
                    ));
                } else if let Ok(body) = std::fs::read_to_string(&path) {
                    if !body.contains(REQUIRED_TS_VERSION) {
                        errors.push(format!(
                            "lib audit artifact {rel:?} does not record the pinned version \
                             {REQUIRED_TS_VERSION:?}"
                        ));
                    }
                }
            }
            _ => {
                errors.push("[meta].lib_audit_artifact missing — no lib audit recorded".to_string())
            }
        }
    }

    // --- criteria ---
    if manifest.criteria.is_empty() {
        errors.push("manifest has no [[criterion]] entries".to_string());
    }

    let mut seen_ids: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, c) in manifest.criteria.iter().enumerate() {
        let label = c
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| format!("criterion {s:?}"))
            .unwrap_or_else(|| format!("criterion #{}", idx + 1));

        check_known_keys(c, CRITERION_KEYS, &label, &mut errors);

        // Required non-empty string fields.
        for field in [
            "id",
            "track",
            "title",
            "state",
            "blocks_release",
            "owner",
            "witness",
        ] {
            require_nonempty_str(c, field, &label, &mut errors);
        }

        // Duplicate / missing id.
        if let Some(id) = c
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            if let Some(prev) = seen_ids.insert(id.to_string(), idx) {
                errors.push(format!(
                    "duplicate criterion id {id:?} (also #{})",
                    prev + 1
                ));
            }
        }

        // Enumerated fields.
        check_enum(c, "track", TRACKS, &label, &mut errors);
        check_enum(c, "state", STATES, &label, &mut errors);
        check_enum(c, "blocks_release", BOOLS, &label, &mut errors);

        match c.get("scope") {
            Some(Value::Arr(items)) => {
                let mut seen_families = BTreeSet::new();
                for family in items {
                    if !is_scope_family_id(family) {
                        errors.push(format!(
                            "{label}.scope: {family:?} must be a stable scope-family id such as s-assignability"
                        ));
                    }
                    if !seen_families.insert(family) {
                        errors.push(format!(
                            "{label}.scope: duplicate scope-family {family:?} within one criterion"
                        ));
                    }
                }
            }
            Some(Value::Str(_)) => errors.push(format!("{label}.scope must be an array")),
            None => {}
        }

        // State/owner consistency.
        let state = c.get("state").and_then(Value::as_str).unwrap_or("");
        let owner = c.get("owner").and_then(Value::as_str).unwrap_or("");
        match state {
            "complete" if owner != "shipped" => errors.push(format!(
                "{label}: state=complete requires owner=\"shipped\", got {owner:?}"
            )),
            "incomplete" if owner == "shipped" => errors.push(format!(
                "{label}: state=incomplete but owner=\"shipped\" — inconsistent"
            )),
            "incomplete" if !owner.is_empty() => {
                check_link(manifest_dir, owner, &format!("{label}.owner"), &mut errors);
            }
            _ => {}
        }

        // Every link/dep must be an existing path.
        for field in ["links", "deps"] {
            match c.get(field) {
                None => {}
                Some(Value::Arr(items)) => {
                    for item in items {
                        check_link(manifest_dir, item, &format!("{label}.{field}"), &mut errors);
                    }
                }
                Some(Value::Str(_)) => {
                    errors.push(format!("{label}.{field} must be an array"));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check that every explicit Tier S/A/B scope family is owned by exactly one
/// completion-manifest criterion. Family ownership, rather than diagnostic-code
/// ownership, is deliberate: one TK code can be reused by several semantic
/// families, each with a different completion state.
fn validate_scope_coverage(manifest_text: &str, scope_text: &str) -> Result<(), Vec<String>> {
    let manifest = parse(manifest_text)?;
    let mut errors = Vec::new();
    let expected = extract_scope_families(scope_text, &mut errors);
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (idx, criterion) in manifest.criteria.iter().enumerate() {
        let id = criterion
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("criterion #{}", idx + 1));

        match criterion.get("scope") {
            Some(Value::Arr(families)) => {
                for family in families {
                    if is_scope_family_id(family) {
                        owners.entry(family.clone()).or_default().push(id.clone());
                    }
                }
            }
            Some(Value::Str(_)) => {
                errors.push(format!("criterion {id:?}.scope must be an array"));
            }
            None => {}
        }
    }

    for family in expected.keys() {
        match owners.get(family) {
            None => errors.push(format!(
                "scope family {family:?} has no completion-manifest criterion owner"
            )),
            Some(ids) if ids.len() > 1 => errors.push(format!(
                "scope family {family:?} has multiple completion-manifest owners: {}",
                ids.join(", ")
            )),
            Some(_) => {}
        }
    }

    for (family, ids) in &owners {
        if !expected.contains_key(family) {
            errors.push(format!(
                "manifest maps unknown scope family {family:?} to {}",
                ids.join(", ")
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn extract_scope_families(scope_text: &str, errors: &mut Vec<String>) -> BTreeMap<String, String> {
    let mut families = BTreeMap::new();
    let mut current_tier: Option<&str> = None;
    let mut pending: Option<(String, usize)> = None;
    let mut reached_oos = false;

    for (index, raw) in scope_text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.trim();
        if !line.is_empty() && !line.starts_with("<!--") && !line.starts_with("- ") {
            if let Some((family, marker_line)) = pending.take() {
                errors.push(format!(
                    "scope.md line {marker_line}: scope-family {family:?} is separated from its Tier row by line {line_no}"
                ));
            }
        }
        if line.starts_with("## Out of scope by design") {
            reached_oos = true;
            break;
        }
        if line.starts_with("### Tier S") {
            current_tier = Some("s");
            continue;
        }
        if line.starts_with("### Tier A") {
            current_tier = Some("a");
            continue;
        }
        if line.starts_with("### Tier B") {
            current_tier = Some("b");
            continue;
        }
        let Some(tier) = current_tier else {
            continue;
        };

        if let Some(family) = line
            .strip_prefix("<!-- scope-family: ")
            .and_then(|rest| rest.strip_suffix(" -->"))
        {
            if let Some((previous, previous_line)) = pending.replace((family.to_string(), line_no))
            {
                errors.push(format!(
                    "scope.md line {previous_line}: scope-family {previous:?} is not followed by a Tier row"
                ));
            }
            continue;
        }

        if line.starts_with("- ") {
            let Some((family, marker_line)) = pending.take() else {
                errors.push(format!(
                    "scope.md line {line_no}: Tier {tier} row has no preceding `scope-family` marker"
                ));
                continue;
            };
            if !is_scope_family_id(&family) || !family.starts_with(&format!("{tier}-")) {
                errors.push(format!(
                    "scope.md line {marker_line}: scope-family {family:?} must be a stable Tier {tier} id"
                ));
                continue;
            }
            if let Some(previous_tier) = families.insert(family.clone(), tier.to_string()) {
                errors.push(format!(
                    "scope.md line {marker_line}: duplicate scope-family {family:?} (already in Tier {previous_tier})"
                ));
            }
        }
    }

    if let Some((family, marker_line)) = pending {
        errors.push(format!(
            "scope.md line {marker_line}: scope-family {family:?} is not followed by a Tier row"
        ));
    }
    if !reached_oos {
        errors.push("scope.md is missing the `## Out of scope by design` boundary".to_string());
    }
    if families.is_empty() {
        errors.push("scope.md Tier S/A/B sections contain no marked scope families".to_string());
    }
    families
}

/// Compare each incomplete criterion's `deps` with its backlog owner's
/// `blocked-by` frontmatter. A mismatch is an error unless the criterion carries
/// an explicit non-empty `deps_exception` rationale — never an implicit drift.
/// This is what catches the historical `14`/`70` drift (a criterion claiming a
/// dependency its owner's `blocked-by` does not list).
fn validate_deps_parity(text: &str, backlog_dir: &Path) -> Result<(), Vec<String>> {
    let manifest = parse(text)?;
    let mut errors = Vec::new();

    for (idx, c) in manifest.criteria.iter().enumerate() {
        let id = c
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("criterion #{}", idx + 1));

        let owner = c.get("owner").and_then(Value::as_str).unwrap_or("");
        if owner.is_empty() || owner == "shipped" {
            continue; // only backlog-path owners have a blocked-by graph
        }

        let deps: BTreeSet<String> = match c.get("deps") {
            Some(Value::Arr(v)) => v.iter().cloned().collect(),
            _ => BTreeSet::new(),
        };
        let exception = c
            .get("deps_exception")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty());

        match owner_blocked_by(backlog_dir, owner) {
            None => errors.push(format!(
                "criterion {id:?}: cannot read owner {owner:?} to check deps parity"
            )),
            Some(blocked_by) => {
                if deps != blocked_by && exception.is_none() {
                    errors.push(format!(
                        "criterion {id:?}: deps {:?} disagree with owner {owner:?} blocked-by {:?} \
                         — reconcile them or add deps_exception=\"rationale\" for a deliberate slice",
                        deps.iter().collect::<Vec<_>>(),
                        blocked_by.iter().collect::<Vec<_>>()
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Parse a backlog item's `blocked-by:` frontmatter list into a set of relative
/// paths (comparable directly to manifest `deps`, since both resolve against
/// docs/backlog/). Returns `None` only if the owner file cannot be read; a file
/// with no `blocked-by` line yields the empty set.
fn owner_blocked_by(backlog_dir: &Path, owner_rel: &str) -> Option<BTreeSet<String>> {
    let text = std::fs::read_to_string(backlog_dir.join(owner_rel)).ok()?;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("blocked-by:") else {
            continue;
        };
        let inner = rest
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or("");
        return Some(
            inner
                .split(',')
                .map(|s| s.trim().trim_matches('"').trim())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        );
    }
    Some(BTreeSet::new())
}

fn is_scope_family_id(value: &str) -> bool {
    let Some((tier, slug)) = value.split_once('-') else {
        return false;
    };
    matches!(tier, "s" | "a" | "b")
        && !slug.is_empty()
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Every key in the table must be one of `allowed` — a typo'd key (`dpes`,
/// `linsk`) must fail rather than silently skipping its checks.
fn check_known_keys(t: &Table, allowed: &[&str], label: &str, errors: &mut Vec<String>) {
    for key in t.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(format!(
                "{label}: unknown key {key:?} — allowed keys are {allowed:?}"
            ));
        }
    }
}

fn require_nonempty_str(t: &Table, field: &str, label: &str, errors: &mut Vec<String>) {
    match t.get(field) {
        Some(Value::Str(s)) if !s.trim().is_empty() => {}
        Some(Value::Str(_)) => errors.push(format!("{label}: field {field:?} is empty")),
        Some(Value::Arr(_)) => errors.push(format!("{label}: field {field:?} must be a string")),
        None => errors.push(format!("{label}: missing required field {field:?}")),
    }
}

fn check_enum(t: &Table, field: &str, allowed: &[&str], label: &str, errors: &mut Vec<String>) {
    if let Some(v) = t.get(field).and_then(Value::as_str) {
        if !allowed.contains(&v) {
            errors.push(format!(
                "{label}: field {field:?} = {v:?} not one of {allowed:?}"
            ));
        }
    }
}

/// A link/dep/owner path must start with `./` or `../` and resolve to an existing file.
fn check_link(manifest_dir: &Path, rel: &str, what: &str, errors: &mut Vec<String>) {
    if !(rel.starts_with("./") || rel.starts_with("../")) {
        errors.push(format!(
            "{what}: link {rel:?} must be a relative path (./ or ../)"
        ));
        return;
    }
    let path = manifest_dir.join(rel);
    if !path.exists() {
        errors.push(format!(
            "{what}: link {rel:?} does not exist ({})",
            path.display()
        ));
    }
}

// ---------------------------------------------------------------------------
// The committed manifest must validate.
// ---------------------------------------------------------------------------

fn backlog_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("backlog")
}

#[test]
fn committed_manifest_is_valid() {
    let dir = backlog_dir();
    let path = dir.join("completion-1.0.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    if let Err(errors) = validate(&text, &dir) {
        panic!(
            "committed 1.0 manifest failed validation ({}):\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }

    let scope_path = dir.join("../reference/scope.md");
    let scope_text = std::fs::read_to_string(&scope_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", scope_path.display()));
    if let Err(errors) = validate_scope_coverage(&text, &scope_text) {
        panic!(
            "scope-to-manifest coverage validation failed ({}):\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }

    if let Err(errors) = validate_deps_parity(&text, &dir) {
        panic!(
            "deps-vs-blocked-by parity validation failed ({}):\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }
}

// ---------------------------------------------------------------------------
// Invalid-manifest witnesses (table-driven).
//
// Each case is a deliberately broken manifest built from a valid base; the
// validator must reject it. Link/owner paths resolve against the REAL
// docs/backlog dir so only the injected defect fails.
// ---------------------------------------------------------------------------

/// A syntactically valid, semantically clean manifest for mutation in the tests.
/// Its links resolve against the real docs/backlog directory.
fn valid_base() -> String {
    concat!(
        "[meta]\n",
        "schema_version = \"1\"\n",
        "lib_audit_ts_version = \"6.0.3\"\n",
        "lib_audit_artifact = \"./lib-audit-6.0.3.md\"\n",
        "[[criterion]]\n",
        "id = \"shipped-one\"\n",
        "track = \"A\"\n",
        "title = \"a shipped thing\"\n",
        "state = \"complete\"\n",
        "blocks_release = \"true\"\n",
        "owner = \"shipped\"\n",
        "witness = \"cargo test conformance\"\n",
        "links = []\n",
        "deps = []\n",
        "[[criterion]]\n",
        "id = \"open-one\"\n",
        "track = \"C\"\n",
        "title = \"an open thing\"\n",
        "state = \"incomplete\"\n",
        "blocks_release = \"false\"\n",
        "owner = \"./43-namespaces-declaration-merging.md\"\n",
        "witness = \"spec-first corpus\"\n",
        "links = [\"../reference/divergences.md\"]\n",
        "deps = []\n",
    )
    .to_string()
}

fn scope_fixture() -> &'static str {
    concat!(
        "### Tier S — structural core\n",
        "<!-- scope-family: s-assignability -->\n",
        "- `TK2322` assignment\n",
        "### Tier A — strict type model & flow\n",
        "<!-- scope-family: a-argument-checking -->\n",
        "- `TK2345` argument\n",
        "### Tier B — broader semantic surface\n",
        "<!-- scope-family: b-indexed-access -->\n",
        "- `TK2536` indexed access\n",
        "## Out of scope by design\n",
        "<!-- scope-family: s-parser -->\n",
        "- `TK1005` parser diagnostic\n",
    )
}

fn coverage_base() -> String {
    valid_base().replacen(
        "deps = []\n",
        "deps = []\nscope = [\"s-assignability\", \"a-argument-checking\", \"b-indexed-access\"]\n",
        1,
    )
}

#[test]
fn valid_base_itself_passes() {
    assert!(
        validate(&valid_base(), &backlog_dir()).is_ok(),
        "the test base manifest must itself be valid"
    );
}

#[test]
fn rejects_invalid_manifests() {
    let dir = backlog_dir();

    // (name, mutated manifest, substring the error output must mention)
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "duplicate id",
            valid_base().replace("id = \"open-one\"", "id = \"shipped-one\""),
            "duplicate criterion id",
        ),
        (
            "missing id",
            valid_base().replace("id = \"open-one\"\n", ""),
            "id",
        ),
        (
            "empty id",
            valid_base().replace("id = \"open-one\"", "id = \"\""),
            "empty",
        ),
        (
            "missing owner",
            valid_base().replace("owner = \"./43-namespaces-declaration-merging.md\"\n", ""),
            "owner",
        ),
        (
            "missing witness",
            valid_base().replace("witness = \"spec-first corpus\"\n", ""),
            "witness",
        ),
        (
            // flip the shipped criterion's owner to a path while keeping state=complete
            "inconsistent: complete but owner is a link",
            valid_base().replace("owner = \"shipped\"", "owner = \"./43-namespaces-declaration-merging.md\""),
            "requires owner",
        ),
        (
            "inconsistent: incomplete but owner=shipped",
            valid_base().replace("owner = \"./43-namespaces-declaration-merging.md\"", "owner = \"shipped\""),
            "inconsistent",
        ),
        (
            "unknown backlog owner link",
            valid_base().replace(
                "owner = \"./43-namespaces-declaration-merging.md\"",
                "owner = \"./999-does-not-exist.md\"",
            ),
            "does not exist",
        ),
        (
            "unknown link in links array",
            valid_base().replace(
                "links = [\"../reference/divergences.md\"]",
                "links = [\"../reference/does-not-exist.md\"]",
            ),
            "does not exist",
        ),
        (
            "unpinned lib audit (wrong version)",
            valid_base().replace(
                "lib_audit_ts_version = \"6.0.3\"",
                "lib_audit_ts_version = \"5.0.0\"",
            ),
            "must be pinned",
        ),
        (
            "missing lib audit artifact file",
            valid_base().replace(
                "lib_audit_artifact = \"./lib-audit-6.0.3.md\"",
                "lib_audit_artifact = \"./no-such-audit.md\"",
            ),
            "does not exist",
        ),
        (
            "bad track",
            valid_base().replace("track = \"C\"", "track = \"Z\""),
            "track",
        ),
        (
            "bad state",
            valid_base().replace("state = \"incomplete\"", "state = \"wip\""),
            "state",
        ),
        (
            "bad blocks_release",
            valid_base().replace("blocks_release = \"false\"", "blocks_release = \"maybe\""),
            "blocks_release",
        ),
        (
            "scope mapping must be an array",
            valid_base().replace(
                "witness = \"cargo test conformance\"",
                "witness = \"cargo test conformance\"\nscope = \"s-assignability\"",
            ),
            "scope must be an array",
        ),
        (
            "malformed scope family",
            valid_base().replace(
                "witness = \"cargo test conformance\"",
                "witness = \"cargo test conformance\"\nscope = [\"S Assignability\"]",
            ),
            "stable scope-family id",
        ),
        (
            "duplicate scope family within criterion",
            valid_base().replace(
                "witness = \"cargo test conformance\"",
                "witness = \"cargo test conformance\"\nscope = [\"s-assignability\", \"s-assignability\"]",
            ),
            "duplicate scope-family",
        ),
        (
            // A typo'd `deps` key must fail loudly, not silently skip link checking.
            "unknown criterion key (typo'd deps)",
            valid_base().replace("deps = []\n[[criterion]]", "dpes = []\n[[criterion]]"),
            "unknown key",
        ),
        (
            "unknown meta key",
            valid_base().replace(
                "schema_version = \"1\"",
                "schema_version = \"1\"\nschema_vresion = \"1\"",
            ),
            "unknown key",
        ),
        (
            "parse error: unquoted value",
            valid_base().replace("title = \"an open thing\"", "title = an open thing"),
            "quoted string",
        ),
        (
            "no meta table",
            valid_base().replace(
                concat!(
                    "[meta]\n",
                    "schema_version = \"1\"\n",
                    "lib_audit_ts_version = \"6.0.3\"\n",
                    "lib_audit_artifact = \"./lib-audit-6.0.3.md\"\n",
                ),
                "",
            ),
            "meta",
        ),
    ];

    let mut leaked = Vec::new();
    for (name, text, expect) in cases {
        match validate(&text, &dir) {
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

#[test]
fn scope_coverage_accepts_exact_mapping_and_ignores_oos_section() {
    assert!(validate_scope_coverage(&coverage_base(), scope_fixture()).is_ok());
}

#[test]
fn scope_coverage_rejects_missing_duplicate_and_unknown_mappings() {
    let cases = [
        (
            "missing",
            coverage_base().replace(", \"b-indexed-access\"", ""),
            "b-indexed-access\" has no completion-manifest criterion owner",
        ),
        (
            "duplicate",
            coverage_base().replace(
                "links = [\"../reference/divergences.md\"]",
                "scope = [\"s-assignability\"]\nlinks = [\"../reference/divergences.md\"]",
            ),
            "s-assignability\" has multiple completion-manifest owners",
        ),
        (
            "unknown",
            coverage_base().replace(
                "\"b-indexed-access\"]",
                "\"b-indexed-access\", \"b-unknown\"]",
            ),
            "b-unknown",
        ),
    ];

    let mut leaked = Vec::new();
    for (name, manifest, expected_error) in cases {
        match validate_scope_coverage(&manifest, scope_fixture()) {
            Ok(()) => leaked.push(format!("case {name:?} was accepted but must be rejected")),
            Err(errors) if !errors.join(" | ").contains(expected_error) => leaked.push(format!(
                "case {name:?} did not mention {expected_error:?}: {}",
                errors.join(" | ")
            )),
            Err(_) => {}
        }
    }
    assert!(leaked.is_empty(), "{}", leaked.join("\n"));
}

#[test]
fn scope_inventory_rejects_unmarked_duplicate_and_wrong_tier_families() {
    let cases = [
        (
            "unmarked row",
            scope_fixture().replace("<!-- scope-family: a-argument-checking -->\n", ""),
            "row has no preceding `scope-family` marker",
        ),
        (
            "duplicate family",
            scope_fixture().replace(
                "### Tier A — strict type model & flow",
                concat!(
                    "<!-- scope-family: s-assignability -->\n",
                    "- another assignment row\n",
                    "### Tier A — strict type model & flow",
                ),
            ),
            "duplicate scope-family",
        ),
        (
            "wrong tier prefix",
            scope_fixture().replace("s-assignability", "a-assignability"),
            "must be a stable Tier s id",
        ),
        (
            "marker drifts across prose",
            scope_fixture().replace(
                "<!-- scope-family: s-assignability -->\n",
                "<!-- scope-family: s-assignability -->\nIntervening prose.\n",
            ),
            "is separated from its Tier row",
        ),
    ];

    let mut leaked = Vec::new();
    for (name, scope, expected_error) in cases {
        let mut errors = Vec::new();
        extract_scope_families(&scope, &mut errors);
        if !errors.join(" | ").contains(expected_error) {
            leaked.push(format!(
                "case {name:?} did not mention {expected_error:?}: {}",
                errors.join(" | ")
            ));
        }
    }
    assert!(leaked.is_empty(), "{}", leaked.join("\n"));
}

// ---------------------------------------------------------------------------
// deps <-> blocked-by parity witnesses (table-driven).
//
// The `open-one` criterion owns backlog `14`, whose real `blocked-by` frontmatter
// is `[43]`; a matching `deps` array makes the base pass. Each mutation
// then drifts one side and must be rejected unless a `deps_exception` justifies it.
// ---------------------------------------------------------------------------

fn deps_parity_base() -> String {
    valid_base()
        .replace(
            "owner = \"./43-namespaces-declaration-merging.md\"",
            "owner = \"./14-libdts-loading.md\"",
        )
        .replace(
            "deps = []\n",
            "deps = [\"./43-namespaces-declaration-merging.md\"]\n",
        )
}

#[test]
fn deps_parity_base_itself_passes() {
    assert!(
        validate_deps_parity(&deps_parity_base(), &backlog_dir()).is_ok(),
        "the deps-parity base must itself be valid"
    );
    // The committed manifest must also be in parity.
    let dir = backlog_dir();
    let text = std::fs::read_to_string(dir.join("completion-1.0.toml")).unwrap();
    assert!(validate_deps_parity(&text, &dir).is_ok());
}

#[test]
fn rejects_deps_parity_drift() {
    let dir = backlog_dir();

    let cases: Vec<(&str, String, &str)> = vec![
        (
            // Drop the owner's only dependency.
            "14 drift (deps drop 43)",
            deps_parity_base().replace(
                "deps = [\"./43-namespaces-declaration-merging.md\"]",
                "deps = []",
            ),
            "disagree with owner",
        ),
        (
            // Extra dep the owner's blocked-by does not list.
            "deps names an unlisted dependency",
            deps_parity_base().replace(
                "deps = [\"./43-namespaces-declaration-merging.md\"]",
                "deps = [\"./43-namespaces-declaration-merging.md\", \"./15-modules-imports.md\"]",
            ),
            "disagree with owner",
        ),
        (
            // Owner (43) has no blocked-by, but the criterion claims a dep.
            "deps on an owner with empty blocked-by",
            valid_base().replace(
                "owner = \"./43-namespaces-declaration-merging.md\"\nwitness = \"spec-first corpus\"\nlinks = [\"../reference/divergences.md\"]\ndeps = []",
                "owner = \"./43-namespaces-declaration-merging.md\"\nwitness = \"spec-first corpus\"\nlinks = [\"../reference/divergences.md\"]\ndeps = [\"./15-modules-imports.md\"]",
            ),
            "disagree with owner",
        ),
    ];

    let mut leaked = Vec::new();
    for (name, text, expect) in cases {
        match validate_deps_parity(&text, &dir) {
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

#[test]
fn deps_exception_permits_a_declared_slice() {
    // A drift with an explicit rationale is allowed (a deliberate dependency slice).
    let text = deps_parity_base().replace(
        "deps = [\"./43-namespaces-declaration-merging.md\"]",
        "deps = []\ndeps_exception = \"slice defers the namespace prerequisite\"",
    );
    assert!(
        validate_deps_parity(&text, &backlog_dir()).is_ok(),
        "an explicit deps_exception must permit a declared slice"
    );
}

#[test]
fn rejects_wu0_dependency_drift_fixture() {
    // The WU0 rejection fixture: criterion claims a dep (`./70-this-parameters.md`)
    // absent from owner 14's blocked-by, with no deps_exception.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("surface")
        .join("fixtures")
        .join("dependency_drift.toml");
    let text = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", fixture.display()));
    match validate_deps_parity(&text, &backlog_dir()) {
        Ok(()) => panic!("WU0 dependency_drift.toml fixture was accepted but must be rejected"),
        Err(errors) => assert!(
            errors.join(" | ").contains("disagree with owner"),
            "expected a deps-vs-blocked-by drift error, got: {}",
            errors.join(" | ")
        ),
    }
}

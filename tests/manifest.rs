//! Validator for the executable 1.0 completion manifest
//! (`docs/backlog/completion-1.0.toml`).
//!
//! The manifest is the machine-validated source of truth for "is typokat 1.0
//! done?". This test parses it and enforces the invariants the sprint requires:
//! duplicate/missing ids, unknown backlog/divergence links, missing owners or
//! witnesses, inconsistent states, and an unpinned TypeScript lib audit all
//! **fail** `cargo test` (and therefore CI). The parser is a small hand-rolled
//! TOML subset — no serde/toml dependency — matching the committed-status-file
//! precedent of `tooling/official-suite/scoreboard.txt`.
//!
//! Format: `#` comments and blank lines ignored; `[meta]` opens the singleton
//! meta table; `[[criterion]]` opens a criterion; values are quoted strings
//! `"..."` or arrays of quoted strings `["a", "b"]`. Paths (owner/links/deps) are
//! resolved relative to the manifest's directory and must exist.

use std::collections::BTreeMap;
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
        "owner = \"./41-generic-methods.md\"\n",
        "witness = \"spec-first corpus\"\n",
        "links = [\"../reference/divergences.md\"]\n",
        "deps = []\n",
    )
    .to_string()
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
            valid_base().replace("owner = \"./41-generic-methods.md\"\n", ""),
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
            valid_base().replace("owner = \"shipped\"", "owner = \"./41-generic-methods.md\""),
            "requires owner",
        ),
        (
            "inconsistent: incomplete but owner=shipped",
            valid_base().replace("owner = \"./41-generic-methods.md\"", "owner = \"shipped\""),
            "inconsistent",
        ),
        (
            "unknown backlog owner link",
            valid_base().replace(
                "owner = \"./41-generic-methods.md\"",
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

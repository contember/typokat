//! Validator for the executable surface-accounting inventory
//! (`tests/surface/inventory.toml`, sprint 2026-07-10 WU1).
//!
//! The inventory is the machine-validated record of what AST surface typokat
//! checks, silently skips (unsupported-in), or excludes by design. This test
//! parses it and enforces the WU1 invariants — all of which must **fail**
//! `cargo test` (and CI) when violated:
//!
//!   * OXC version drift (manifest `[meta].oxc_version` vs the `Cargo.lock` pin;
//!     the pinned witness is `Cargo.lock`, never a Cargo-registry source path);
//!   * missing / duplicate / structurally-inconsistent surface identities;
//!   * missing owner / witness, a bad disposition enum, an unknown role/key;
//!   * `unsupported-in` without a live backlog owner file;
//!   * a `supported` wrapper whose `requires_slots` child has no coverage record;
//!   * (inventory-only) a dispatcher role from the census left unclassified, or a
//!     compile-time classifier family (`typokat::surface::FAMILY_ROLES`) without a
//!     record **under its owning role** — the bridge that ties the exhaustive OXC
//!     matches to this file; a record filed under the wrong role does not count.
//!
//! Parser: the same hand-rolled TOML subset as `tests/manifest.rs` — no
//! serde/toml dependency. `#` comments and blank lines ignored; `[meta]` opens the
//! singleton meta table; `[[surface]]` opens a record; values are a quoted string
//! or an array of quoted strings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use typokat::surface::{FAMILY_ROLES, SURFACE_ROLES};

// ---------------------------------------------------------------------------
// Parser (shared shape with tests/manifest.rs)
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

type Table = BTreeMap<String, Value>;

struct Manifest {
    meta: Vec<Table>,
    surfaces: Vec<Table>,
}

fn parse(text: &str) -> Result<Manifest, Vec<String>> {
    let mut errors = Vec::new();
    let mut meta: Vec<Table> = Vec::new();
    let mut surfaces: Vec<Table> = Vec::new();

    enum Cur {
        None,
        Meta,
        Surface,
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
        if line == "[[surface]]" {
            surfaces.push(Table::new());
            cur = Cur::Surface;
            continue;
        }
        if line.starts_with('[') {
            errors.push(format!("line {line_no}: unknown section header {line:?}"));
            cur = Cur::None;
            continue;
        }
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
            Cur::Surface => surfaces.last_mut(),
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
        Ok(Manifest { meta, surfaces })
    } else {
        Err(errors)
    }
}

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
// Schema
// ---------------------------------------------------------------------------

const DISPOSITIONS: &[&str] = &["supported", "unsupported-in", "design-oos"];
const META_KEYS: &[&str] = &["oxc_version"];
const SURFACE_KEYS: &[&str] = &[
    "id",
    "role",
    "surface",
    "slot",
    "oxc_variant",
    "disposition",
    "owner",
    "witness",
    "requires_slots",
];

/// Schema + intra-manifest validation. Applied to the committed inventory and to
/// every fixture. `expected_oxc` is the `Cargo.lock` pin (the version witness).
fn validate_schema(text: &str, dir: &Path, expected_oxc: &str) -> Result<Manifest, Vec<String>> {
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
        match meta.get("oxc_version").and_then(Value::as_str) {
            Some(v) if v == expected_oxc => {}
            Some(v) => errors.push(format!(
                "[meta].oxc_version is {v:?} but Cargo.lock pins oxc {expected_oxc:?} \
                 — version drift"
            )),
            None => errors
                .push("[meta].oxc_version missing — the OXC surface pin is unrecorded".to_string()),
        }
    }

    // --- surface records ---
    if manifest.surfaces.is_empty() {
        errors.push("manifest has no [[surface]] records".to_string());
    }

    let mut seen_ids: BTreeMap<String, usize> = BTreeMap::new();
    // (role, surface, slot) present, for requires_slots child coverage.
    let mut present_slots: BTreeSet<(String, String, String)> = BTreeSet::new();
    for c in &manifest.surfaces {
        if let (Some(role), Some(surface), Some(slot)) = (
            c.get("role").and_then(Value::as_str),
            c.get("surface").and_then(Value::as_str),
            c.get("slot").and_then(Value::as_str),
        ) {
            present_slots.insert((role.to_string(), surface.to_string(), slot.to_string()));
        }
    }

    for (idx, c) in manifest.surfaces.iter().enumerate() {
        let label = c
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| format!("surface {s:?}"))
            .unwrap_or_else(|| format!("surface #{}", idx + 1));

        check_known_keys(c, SURFACE_KEYS, &label, &mut errors);
        for field in [
            "id",
            "role",
            "surface",
            "slot",
            "oxc_variant",
            "disposition",
            "owner",
            "witness",
        ] {
            require_nonempty_str(c, field, &label, &mut errors);
        }

        let id = c.get("id").and_then(Value::as_str).unwrap_or("");
        let role = c.get("role").and_then(Value::as_str).unwrap_or("");
        let surface = c.get("surface").and_then(Value::as_str).unwrap_or("");
        let slot = c.get("slot").and_then(Value::as_str).unwrap_or("");
        let disposition = c.get("disposition").and_then(Value::as_str).unwrap_or("");
        let owner = c.get("owner").and_then(Value::as_str).unwrap_or("");

        // Duplicate id.
        if !id.is_empty() {
            if let Some(prev) = seen_ids.insert(id.to_string(), idx) {
                errors.push(format!("duplicate surface id {id:?} (also #{})", prev + 1));
            }
        }

        // Structural: id must be exactly role/surface/slot (catches typos/desync).
        if !id.is_empty() && !role.is_empty() && !surface.is_empty() && !slot.is_empty() {
            let expected = format!("{role}/{surface}/{slot}");
            if id != expected {
                errors.push(format!(
                    "{label}: id {id:?} does not equal role/surface/slot {expected:?}"
                ));
            }
        }

        if !role.is_empty() && !SURFACE_ROLES.contains(&role) {
            errors.push(format!(
                "{label}: role {role:?} not one of {SURFACE_ROLES:?}"
            ));
        }
        check_enum(c, "disposition", DISPOSITIONS, &label, &mut errors);

        // Disposition / owner consistency.
        match disposition {
            "supported" if owner != "shipped" => errors.push(format!(
                "{label}: disposition=supported requires owner=\"shipped\", got {owner:?}"
            )),
            "design-oos" if owner != "by-design" => errors.push(format!(
                "{label}: disposition=design-oos requires owner=\"by-design\", got {owner:?}"
            )),
            "unsupported-in" => {
                if owner == "shipped" || owner == "by-design" {
                    errors.push(format!(
                        "{label}: disposition=unsupported-in needs a live backlog owner, got {owner:?}"
                    ));
                } else if !owner.is_empty() {
                    check_backlog_owner(dir, owner, &label, &mut errors);
                }
            }
            _ => {}
        }

        // Witness: non-empty (checked above); if it looks like a path, it must exist.
        if let Some(w) = c.get("witness").and_then(Value::as_str) {
            if w.starts_with("../") || w.starts_with("./") {
                let path = dir.join(w);
                if !path.exists() {
                    errors.push(format!(
                        "{label}: witness path {w:?} does not exist ({})",
                        path.display()
                    ));
                }
            }
        }

        // requires_slots: only on supported wrappers; each child must have a record.
        match c.get("requires_slots") {
            None => {}
            Some(Value::Str(_)) => errors.push(format!("{label}: requires_slots must be an array")),
            Some(Value::Arr(children)) => {
                if disposition != "supported" {
                    errors.push(format!(
                        "{label}: requires_slots is only valid on a supported wrapper"
                    ));
                }
                for child in children {
                    let key = (role.to_string(), surface.to_string(), child.to_string());
                    if !present_slots.contains(&key) {
                        errors.push(format!(
                            "{label}: requires_slots child {child:?} has no covering record \
                             ({role}/{surface}/{child})"
                        ));
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(manifest)
    } else {
        Err(errors)
    }
}

/// Inventory-only completeness: every census role classified, and every
/// compile-time classifier family (`typokat::surface`) carried by a record. This
/// is the check that consumes the exhaustive OXC classifiers.
fn validate_completeness(manifest: &Manifest) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    let roles: BTreeSet<&str> = manifest
        .surfaces
        .iter()
        .filter_map(|c| c.get("role").and_then(Value::as_str))
        .collect();
    for role in SURFACE_ROLES {
        if !roles.contains(role) {
            errors.push(format!("census role {role:?} has no inventory record"));
        }
    }

    // Per-role coverage: a family filed under the wrong role does not count.
    let surface_roles: BTreeSet<(&str, &str)> = manifest
        .surfaces
        .iter()
        .filter_map(|c| {
            Some((
                c.get("surface").and_then(Value::as_str)?,
                c.get("role").and_then(Value::as_str)?,
            ))
        })
        .collect();
    for (family, role) in FAMILY_ROLES {
        if !surface_roles.contains(&(family, role)) {
            errors.push(format!(
                "classifier surface family {family:?} has no inventory record under role \
                 {role:?} (src/surface.rs emits it but tests/surface/inventory.toml lacks a \
                 covering record in that role)"
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// An `unsupported-in` owner must be a live backlog file under `docs/backlog/`.
fn check_backlog_owner(dir: &Path, rel: &str, label: &str, errors: &mut Vec<String>) {
    if !(rel.starts_with("./") || rel.starts_with("../")) {
        errors.push(format!(
            "{label}: owner {rel:?} must be a relative backlog path (./ or ../)"
        ));
        return;
    }
    if !rel.contains("docs/backlog/") {
        errors.push(format!(
            "{label}: owner {rel:?} must point into docs/backlog/"
        ));
        return;
    }
    let path = dir.join(rel);
    if !path.exists() {
        errors.push(format!(
            "{label}: owner {rel:?} is not a live backlog file ({})",
            path.display()
        ));
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn surface_dir() -> PathBuf {
    repo_root().join("tests").join("surface")
}

/// Read the pinned oxc version from `Cargo.lock` (the checked-in version witness;
/// never a Cargo-registry source path). Any oxc_* crate carries the workspace pin.
fn cargo_lock_oxc_version() -> String {
    let lock = repo_root().join("Cargo.lock");
    let text = std::fs::read_to_string(&lock)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", lock.display()));
    let mut in_oxc_ast = false;
    for line in text.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            in_oxc_ast = false;
        } else if line == "name = \"oxc_ast\"" {
            in_oxc_ast = true;
        } else if in_oxc_ast {
            if let Some(v) = line
                .strip_prefix("version = \"")
                .and_then(|s| s.strip_suffix('"'))
            {
                return v.to_string();
            }
        }
    }
    panic!("could not find oxc_ast version in Cargo.lock");
}

// ---------------------------------------------------------------------------
// The committed inventory must validate, with every role and family covered.
// ---------------------------------------------------------------------------

#[test]
fn committed_inventory_is_valid() {
    let dir = surface_dir();
    let path = dir.join("inventory.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let expected = cargo_lock_oxc_version();

    let manifest = match validate_schema(&text, &dir, &expected) {
        Ok(m) => m,
        Err(errors) => panic!(
            "committed surface inventory failed schema validation ({}):\n  {}",
            errors.len(),
            errors.join("\n  ")
        ),
    };
    if let Err(errors) = validate_completeness(&manifest) {
        panic!(
            "committed surface inventory failed completeness ({}):\n  {}",
            errors.len(),
            errors.join("\n  ")
        );
    }
}

// ---------------------------------------------------------------------------
// WU0 fixtures: valid accepts, the rest reject for their documented reason.
// dependency_drift.toml and malformed_divergence.md are WU6-shaped (a completion
// manifest and a divergences.md excerpt); WU1 rejects them structurally as not a
// surface inventory — their semantic reason is validated by WU6, not here.
// ---------------------------------------------------------------------------

#[test]
fn wu0_fixtures_reject_for_their_reason() {
    let dir = surface_dir().join("fixtures");
    let expected = cargo_lock_oxc_version();

    // (file, accept?, substring an error must mention when rejected)
    let cases: &[(&str, bool, &str)] = &[
        ("valid.surface.toml", true, ""),
        ("duplicate.surface.toml", false, "duplicate"),
        ("missing.surface.toml", false, "spread-element"),
        // WU6-shaped fixtures: rejected structurally by the surface validator.
        ("dependency_drift.toml", false, "criterion"),
        ("malformed_divergence.md", false, ""),
    ];

    let mut leaked = Vec::new();
    for (file, accept, expect) in cases {
        let path = dir.join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        match (validate_schema(&text, &dir, &expected), accept) {
            (Ok(_), true) => {}
            (Ok(_), false) => leaked.push(format!(
                "fixture {file:?} was accepted but must be rejected"
            )),
            (Err(errors), true) => leaked.push(format!(
                "fixture {file:?} must validate but was rejected:\n    {}",
                errors.join(" | ")
            )),
            (Err(errors), false) => {
                let joined = errors.join(" | ");
                if !expect.is_empty() && !joined.contains(expect) {
                    leaked.push(format!(
                        "fixture {file:?} rejected but no error mentioned {expect:?}:\n    {joined}"
                    ));
                }
            }
        }
    }
    assert!(leaked.is_empty(), "{}", leaked.join("\n"));
}

// ---------------------------------------------------------------------------
// Mutation witnesses (table-driven): a valid base whose owner/witness paths
// resolve against the REAL tests/surface dir, so only the injected defect fails.
// ---------------------------------------------------------------------------

/// A self-consistent surface inventory for mutation. oxc_version equals the
/// Cargo.lock pin; a supported wrapper covers its required children; one
/// unsupported-in links a live backlog owner; one design-oos is by-design.
fn valid_base(oxc: &str) -> String {
    format!(
        concat!(
            "[meta]\n",
            "oxc_version = \"{oxc}\"\n",
            "[[surface]]\n",
            "id = \"expr-infer/object-literal/self\"\n",
            "role = \"expr-infer\"\n",
            "surface = \"object-literal\"\n",
            "slot = \"self\"\n",
            "oxc_variant = \"Expression::ObjectExpression\"\n",
            "disposition = \"supported\"\n",
            "owner = \"shipped\"\n",
            "witness = \"../cases/m2_objects\"\n",
            "requires_slots = [\"value\", \"spread-element\"]\n",
            "[[surface]]\n",
            "id = \"expr-infer/object-literal/value\"\n",
            "role = \"expr-infer\"\n",
            "surface = \"object-literal\"\n",
            "slot = \"value\"\n",
            "oxc_variant = \"ObjectPropertyKind::ObjectProperty\"\n",
            "disposition = \"supported\"\n",
            "owner = \"shipped\"\n",
            "witness = \"../cases/m2_objects\"\n",
            "[[surface]]\n",
            "id = \"expr-infer/object-literal/spread-element\"\n",
            "role = \"expr-infer\"\n",
            "surface = \"object-literal\"\n",
            "slot = \"spread-element\"\n",
            "oxc_variant = \"ObjectPropertyKind::SpreadProperty\"\n",
            "disposition = \"unsupported-in\"\n",
            "owner = \"../../docs/backlog/71-expression-inference-fn-tail.md\"\n",
            "witness = \"../cases/b73_surface_accounting/array_spread.ts\"\n",
            "[[surface]]\n",
            "id = \"stmt-check/with-statement/self\"\n",
            "role = \"stmt-check\"\n",
            "surface = \"with-statement\"\n",
            "slot = \"self\"\n",
            "oxc_variant = \"Statement::WithStatement\"\n",
            "disposition = \"design-oos\"\n",
            "owner = \"by-design\"\n",
            "witness = \"strict-mode syntax error, no type model\"\n",
        ),
        oxc = oxc
    )
}

#[test]
fn valid_base_itself_passes() {
    let oxc = cargo_lock_oxc_version();
    assert!(
        validate_schema(&valid_base(&oxc), &surface_dir(), &oxc).is_ok(),
        "the mutation base must itself validate"
    );
}

#[test]
fn rejects_mutated_inventories() {
    let dir = surface_dir();
    let oxc = cargo_lock_oxc_version();
    let base = valid_base(&oxc);

    // (name, mutated text, substring the error output must mention)
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "omitted record (missing required child slot)",
            base.replace(
                concat!(
                    "[[surface]]\n",
                    "id = \"expr-infer/object-literal/spread-element\"\n",
                    "role = \"expr-infer\"\n",
                    "surface = \"object-literal\"\n",
                    "slot = \"spread-element\"\n",
                    "oxc_variant = \"ObjectPropertyKind::SpreadProperty\"\n",
                    "disposition = \"unsupported-in\"\n",
                    "owner = \"../../docs/backlog/71-expression-inference-fn-tail.md\"\n",
                    "witness = \"../cases/b73_surface_accounting/array_spread.ts\"\n",
                ),
                "",
            ),
            "has no covering record",
        ),
        (
            "requires_slots names a slot never recorded",
            base.replace(
                "requires_slots = [\"value\", \"spread-element\"]",
                "requires_slots = [\"value\", \"spread-element\", \"computed-key\"]",
            ),
            "computed-key",
        ),
        (
            "duplicate identity",
            base.replace(
                "id = \"expr-infer/object-literal/value\"",
                "id = \"expr-infer/object-literal/self\"",
            ),
            "duplicate surface id",
        ),
        (
            "bad disposition enum",
            base.replace("disposition = \"unsupported-in\"", "disposition = \"wip\""),
            "not one of",
        ),
        (
            "dead owner link",
            base.replace(
                "owner = \"../../docs/backlog/71-expression-inference-fn-tail.md\"",
                "owner = \"../../docs/backlog/999-does-not-exist.md\"",
            ),
            "not a live backlog file",
        ),
        (
            "OXC version drift",
            base.replace(
                &format!("oxc_version = \"{oxc}\""),
                "oxc_version = \"9.9.9\"",
            ),
            "version drift",
        ),
        (
            "missing owner",
            base.replace(
                "owner = \"../../docs/backlog/71-expression-inference-fn-tail.md\"\n",
                "",
            ),
            "owner",
        ),
        (
            "missing witness",
            base.replace(
                "witness = \"../cases/b73_surface_accounting/array_spread.ts\"\n",
                "",
            ),
            "witness",
        ),
        (
            "unknown role",
            base.replace(
                concat!(
                    "id = \"stmt-check/with-statement/self\"\n",
                    "role = \"stmt-check\""
                ),
                concat!(
                    "id = \"gremlin/with-statement/self\"\n",
                    "role = \"gremlin\""
                ),
            ),
            "role",
        ),
        (
            "id not equal to role/surface/slot",
            base.replace(
                "id = \"expr-infer/object-literal/value\"",
                "id = \"expr-infer/object-literal/typo\"",
            ),
            "does not equal role/surface/slot",
        ),
        (
            "unsupported-in with owner shipped",
            base.replace(
                "owner = \"../../docs/backlog/71-expression-inference-fn-tail.md\"",
                "owner = \"shipped\"",
            ),
            "live backlog owner",
        ),
        (
            // Flip the first supported record (object-literal/self) off "shipped".
            "supported owner not shipped",
            base.replacen("owner = \"shipped\"", "owner = \"by-design\"", 1),
            "requires owner=\"shipped\"",
        ),
        (
            "requires_slots on a non-supported record",
            base.replace(
                "witness = \"strict-mode syntax error, no type model\"\n",
                "witness = \"strict-mode syntax error, no type model\"\nrequires_slots = [\"self\"]\n",
            ),
            "only valid on a supported wrapper",
        ),
        (
            "unknown key (typo)",
            base.replace("oxc_variant = \"Expression::ObjectExpression\"", "oxc_varaint = \"x\""),
            "unknown key",
        ),
        (
            "bad witness path",
            base.replace("witness = \"../cases/m2_objects\"", "witness = \"../cases/nope-not-here\""),
            "does not exist",
        ),
    ];

    let mut leaked = Vec::new();
    for (name, text, expect) in cases {
        match validate_schema(&text, &dir, &oxc) {
            Ok(_) => leaked.push(format!("case {name:?} was accepted but must be rejected")),
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

// ---------------------------------------------------------------------------
// Completeness mutations on the COMMITTED inventory: moving a family's records
// to a different (valid) role must fail per-role coverage, and dropping a
// role's records entirely must fail the census-role check.
// ---------------------------------------------------------------------------

#[test]
fn completeness_rejects_family_under_wrong_role_and_missing_role() {
    let dir = surface_dir();
    let oxc = cargo_lock_oxc_version();
    let text = std::fs::read_to_string(dir.join("inventory.toml")).unwrap();

    // Refile every `type-name` record from annotation-lower under expr-infer
    // (ids kept consistent so schema still passes; only per-role coverage fails).
    let refiled = text
        .replace(
            "id = \"annotation-lower/type-name/",
            "id = \"expr-infer/type-name/",
        )
        .replace(
            concat!("role = \"annotation-lower\"\n", "surface = \"type-name\""),
            concat!("role = \"expr-infer\"\n", "surface = \"type-name\""),
        );
    let manifest = validate_schema(&refiled, &dir, &oxc)
        .expect("refiled inventory must still pass schema validation");
    let errors = validate_completeness(&manifest)
        .expect_err("family under the wrong role must fail per-role coverage");
    let joined = errors.join(" | ");
    assert!(
        joined.contains("\"type-name\" has no inventory record under role \"annotation-lower\""),
        "expected a per-role coverage error for type-name, got:\n{joined}"
    );

    // Refile the two `call` records under decl: the census-role check must fire.
    let no_call = text
        .replace("id = \"call/call-arguments/", "id = \"decl/call-arguments/")
        .replace(
            concat!("role = \"call\"\n", "surface = \"call-arguments\""),
            concat!("role = \"decl\"\n", "surface = \"call-arguments\""),
        );
    let manifest = validate_schema(&no_call, &dir, &oxc)
        .expect("call-less inventory must still pass schema validation");
    let errors = validate_completeness(&manifest)
        .expect_err("a census role with no records must fail completeness");
    assert!(
        errors
            .join(" | ")
            .contains("census role \"call\" has no inventory record"),
        "expected a missing-role error for call, got:\n{}",
        errors.join(" | ")
    );
}

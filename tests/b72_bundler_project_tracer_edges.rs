//! Adversarial RED contract for project boundaries that the B72 tracer must reject explicitly.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

const SUMMARY_KEYS: [&str; 10] = [
    "schema",
    "profile",
    "config",
    "roots",
    "files",
    "resolutions",
    "project_notices",
    "parse_errors",
    "incomplete",
    "diagnostics",
];

struct TempProject {
    root: PathBuf,
    project: PathBuf,
}

impl TempProject {
    fn new(label: &str) -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("typokat-b72-{label}-{}-{id}", std::process::id()));
        let project = root.join("project");
        std::fs::create_dir_all(&project).expect("create B72 edge project");
        Self { root, project }
    }

    fn write(&self, relative: &str, source: &str) {
        let path = self.project.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create B72 edge source directory");
        }
        std::fs::write(path, source).expect("write B72 edge source");
    }

    fn write_config(&self, roots: &[&str]) {
        let config = json!({
            "compilerOptions": {
                "strict": true,
                "noEmit": true,
                "module": "ESNext",
                "moduleResolution": "Bundler"
            },
            "files": roots
        });
        self.write(
            "tsconfig.json",
            &serde_json::to_string_pretty(&config).expect("serialize B72 edge config"),
        );
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run_project(project: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typokat"))
        .args([
            "check",
            "--format",
            "compact",
            "--project-summary",
            "json",
            &project.to_string_lossy(),
        ])
        .output()
        .expect("run typokat B72 edge contract")
}

fn expected_summary(
    roots: &[&str],
    checked: &[&str],
    skipped: &[&str],
    resolutions: &[&str],
    notices: &[&str],
    diagnostics: &[&str],
) -> Value {
    json!({
        "schema": 1,
        "profile": "bundler",
        "config": "tsconfig.json",
        "roots": roots,
        "files": {
            "checked": checked,
            "skipped": skipped,
            "excluded": []
        },
        "resolutions": resolutions,
        "project_notices": notices,
        "parse_errors": [],
        "incomplete": [],
        "diagnostics": diagnostics
    })
}

fn assert_summary(output: &Output, expected_exit: i32, expected: &Value, label: &str) {
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "{label} exit; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.ends_with(b"\n"), "{label} JSONL newline");
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "{label} emits exactly one summary"
    );

    let summary_text = String::from_utf8_lossy(&output.stdout);
    let mut previous = None;
    for key in SUMMARY_KEYS {
        let needle = format!("\"{key}\":");
        let position = summary_text
            .find(&needle)
            .unwrap_or_else(|| panic!("{label} summary is missing {needle}"));
        if let Some(previous) = previous {
            assert!(position > previous, "{label} top-level key order");
        }
        previous = Some(position);
    }

    let actual: Value =
        serde_json::from_slice(&output.stdout).expect("B72 edge summary is valid JSON");
    assert_eq!(actual, *expected, "{label} exact summary");
}

fn assert_unsupported_case(
    label: &str,
    source: &str,
    roots: &[&str],
    extra_files: &[(&str, &str)],
    resolution: &str,
    notice: &str,
) {
    let temp = TempProject::new(label);
    temp.write_config(roots);
    temp.write("main.ts", source);
    for (path, contents) in extra_files {
        temp.write(path, contents);
    }

    let output = run_project(&temp.project);
    let expected = expected_summary(roots, &[], roots, &[resolution], &[notice], &[]);
    assert_summary(&output, 3, &expected, label);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("project: {notice}\n"),
        "{label} renders the exact project notice"
    );
}

#[test]
fn explicit_ts_extension_is_explicitly_unsupported() {
    assert_local_specifier("explicit-ts", "./value.ts", "explicit-ts-extension");
}

#[test]
fn query_specifier_is_explicitly_unsupported() {
    assert_local_specifier("query", "./value.js?raw", "query-or-fragment");
}

#[test]
fn fragment_specifier_is_explicitly_unsupported() {
    assert_local_specifier("fragment", "./value.js#raw", "query-or-fragment");
}

fn assert_local_specifier(label: &str, specifier: &str, reason: &str) {
    let source = format!("import {{ value }} from \"{specifier}\";\n");
    let resolution = format!("main.ts:1:1 named-import {specifier} -> unsupported");
    let notice = format!("unsupported-module-specifier {reason} main.ts:1:1 {specifier}");
    assert_unsupported_case(
        label,
        &source,
        &["main.ts", "value.ts"],
        &[("value.ts", "export const value = 1;\n")],
        &resolution,
        &notice,
    );
}

#[test]
fn import_attributes_are_explicitly_unsupported() {
    assert_unsupported_form(
        "import-attributes",
        "import { value } from \"./value.js\" with { type: \"json\" };\n",
        "import-attributes",
    );
}

#[test]
fn string_literal_imported_name_is_explicitly_unsupported() {
    assert_unsupported_form(
        "string-literal-import-name",
        "import { \"value\" as value } from \"./value.js\";\n",
        "string-literal-import-name",
    );
}

#[test]
fn empty_named_import_is_explicitly_unsupported() {
    assert_unsupported_case(
        "empty-named-import",
        "import {} from \"./missing.js\";\nexport const value = 1;\n",
        &["main.ts"],
        &[],
        "main.ts:1:1 empty-named-import ./missing.js -> unsupported",
        "unsupported-module-form empty-named-import main.ts:1:1 ./missing.js",
    );
}

fn assert_unsupported_form(label: &str, source: &str, form: &str) {
    let resolution = format!("main.ts:1:1 {form} ./value.js -> unsupported");
    let notice = format!("unsupported-module-form {form} main.ts:1:1 ./value.js");
    assert_unsupported_case(
        label,
        source,
        &["main.ts", "value.ts"],
        &[("value.ts", "export const value = 1;\n")],
        &resolution,
        &notice,
    );
}

#[test]
fn package_import_specifier_is_explicitly_unsupported() {
    assert_non_local_specifier("package-import", "#value", "package-import");
}

#[test]
fn uri_specifier_is_explicitly_unsupported() {
    assert_non_local_specifier("uri", "https://example.test/value.ts", "uri");
}

#[test]
fn absolute_specifier_is_explicitly_unsupported() {
    assert_non_local_specifier(
        "absolute",
        "/tmp/typokat-b72-absolute-module.ts",
        "absolute",
    );
}

fn assert_non_local_specifier(label: &str, specifier: &str, reason: &str) {
    let source = format!("import {{ value }} from \"{specifier}\";\n");
    let resolution = format!("main.ts:1:1 named-import {specifier} -> unsupported");
    let notice = format!("unsupported-module-specifier {reason} main.ts:1:1 {specifier}");
    assert_unsupported_case(label, &source, &["main.ts"], &[], &resolution, &notice);
}

#[test]
fn resolver_target_outside_configured_roots_is_reported_not_loaded() {
    let temp = TempProject::new("unconfigured-target");
    temp.write_config(&["main.ts"]);
    temp.write("main.ts", "import { value } from \"./value.js\";\n");
    temp.write("value.ts", "export const value = ;\n");

    let output = run_project(&temp.project);
    let notice = "unsupported-module-target unconfigured main.ts:1:1 ./value.js -> value.ts";
    let expected = expected_summary(
        &["main.ts"],
        &[],
        &["main.ts"],
        &["main.ts:1:1 named-import ./value.js -> unsupported"],
        &[notice],
        &[],
    );
    assert_summary(&output, 3, &expected, "unconfigured target");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("project: {notice}\n")
    );
}

#[cfg(unix)]
#[test]
fn configured_symlink_cannot_escape_the_project_directory() {
    use std::os::unix::fs::symlink;

    let temp = TempProject::new("symlink-root-escape");
    temp.write_config(&["main.ts"]);
    let outside = temp.root.join("outside.ts");
    std::fs::write(&outside, "export const outside = ;\n").expect("write outside source");
    symlink(&outside, temp.project.join("main.ts")).expect("create escaping source symlink");

    let output = run_project(&temp.project);
    let notice = "unsupported-config-root symlink-escape main.ts";
    let expected = expected_summary(&["main.ts"], &[], &["main.ts"], &[], &[notice], &[]);
    assert_summary(&output, 3, &expected, "symlink root escape");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("project: {notice}\n")
    );
}

#[test]
fn missing_module_summary_uses_the_import_declaration_location() {
    let temp = TempProject::new("missing-location");
    temp.write_config(&["main.ts"]);
    temp.write(
        "main.ts",
        "// The first ./missing.js occurrence is not an import.\nimport { missingValue } from \"./missing.js\";\n",
    );

    let output = run_project(&temp.project);
    let expected = expected_summary(
        &["main.ts"],
        &["main.ts"],
        &[],
        &["main.ts:2:1 named-import ./missing.js -> unresolved"],
        &[],
        &["main.ts:2:30 TK2307"],
    );
    assert_summary(&output, 1, &expected, "missing module location");

    assert_eq!(
        output.stderr, b"main.ts(2,10): error TK2307: Cannot find module './missing.js'\n",
        "human diagnostic keeps the checker span"
    );
}

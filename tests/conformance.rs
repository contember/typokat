//! Conformance harness (mvp-plan §6, `tests/cases/README.md`).
//!
//! This is the spec corpus runner: for each `.ts` fixture it parses the inline
//! `// error[CODE]: substring` markers, runs the checker, maps each diagnostic's
//! primary-span START to a 1-based source line, and asserts that — per line — the
//! multiset of expected codes equals the multiset of actual codes, and each
//! expected substring is contained (case-sensitive) in the corresponding
//! diagnostic's fully-rendered text.
//!
//! M0–M22 (`m0_assign_primitives/`, `m1_binder_inference/`, `m2_objects/`,
//! `m3_functions/`, `m4_unions/`, `m5_named_recursive/`, `m6_reporting/`,
//! `m7_narrowing/`, `m8_discriminated/`, `m9_generics/`, `m10_inference/`,
//! `m11_classes/`, `m12_inheritance/`, `m13_modifiers/`, `m14_readonly/`,
//! `m15_accessors/`, `m16_generic_classes/`, `m17_arrays/`, `m18_tuples/`,
//! `m19_index_sig/`, `m20_keyof/`, `m21_optional/`, `m22_unresolved_type/`) are
//! enabled. The `MILESTONE_DIRS` table is the extension point: flip a row to
//! turn a milestone's fixtures on as it lands (mvp-plan §5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use typokat::driver::{check_project, check_source, FileInput};
use typokat::span::LineIndex;

/// Milestone fixture directories under `tests/cases/`. Only enabled dirs run.
/// Enable later milestones here as they are implemented.
const MILESTONE_DIRS: &[(&str, bool)] = &[
    ("m0_assign_primitives", true),
    ("m1_binder_inference", true),
    ("m2_objects", true),
    ("m3_functions", true),
    ("m4_unions", true),
    ("m5_named_recursive", true),
    ("m6_reporting", true),
    ("m7_narrowing", true),
    ("m8_discriminated", true),
    ("m9_generics", true),
    ("m10_inference", true),
    ("m11_classes", true),
    ("m12_inheritance", true),
    ("m13_modifiers", true),
    ("m14_readonly", true),
    ("m15_accessors", true),
    ("m16_generic_classes", true),
    ("m17_arrays", true),
    ("m18_tuples", true),
    ("m19_index_sig", true),
    ("m20_keyof", true),
    ("m21_optional", true),
    ("m22_unresolved_type", true),
    ("m23_unstructured_narrowing", true),
    ("m24_generic_constraints", true),
    ("m25_conditional_types", true),
    ("m26_mapped_types", true),
    ("m27_template_literals", true),
    ("m28_utility_types", true),
    // M29 modules are project fixtures (subdirectories with multiple files), not
    // flat single-file fixtures. Registered false by the spec commit; the
    // implementation commit extends the harness before flipping this on.
    ("m29_modules", true),
    ("m30_contextual_literals", true),
    // Bug-fix corpora (official-suite findings / backlog items). Each is
    // committed `false` as a behavior-neutral spec, then flipped `true` by the
    // commit that lands its fix. See tests/cases/README.md ("Bug-fix corpora").
    ("f1_object_interface_methods", true),
    ("f1_object_interface_call", true),
    ("f1_object_interface_construct", true),
    ("f3_class_member_collection", true),
    ("f4_destructuring_access", true),
    ("f5_union_readonly", true),
    // Backlog corpora (same mechanism, named by backlog item ID).
    ("b06_class_completeness", true),
    ("b20_ctor_accessibility", true),
    ("b28_interface_extends", true),
    ("b29_alias_cycles", true),
    ("b30_negative_literals", true),
    ("b55_template_memo", true),
    // Project fixtures (subdirectories, m29 convention) — see `PROJECT_DIRS`.
    ("b58_project_scopes", true),
    ("b61_field_initializers", true),
];

/// Milestone dirs whose fixtures are **project subdirectories** (multiple `.ts`
/// files per project, checked together via [`check_project`]) rather than flat
/// single-file fixtures. Every other dir is a flat corpus. Keep in sync with the
/// project-shaped corpora in `MILESTONE_DIRS`.
const PROJECT_DIRS: &[&str] = &["m29_modules", "b58_project_scopes"];

/// An expectation parsed from a single inline marker.
#[derive(Debug, Clone)]
struct ExpectedMarker {
    code: String,
    /// Optional case-sensitive substring the rendered diagnostic must contain.
    substring: Option<String>,
}

/// The M0 conformance entry point. Runs every enabled milestone dir and reports
/// all failures together so a regression is debuggable in one shot.
#[test]
fn conformance() {
    let cases_root = cases_root();
    let mut failures: Vec<String> = Vec::new();
    let mut files_checked = 0usize;

    for (dir, enabled) in MILESTONE_DIRS {
        if !*enabled {
            continue;
        }
        let dir_path = cases_root.join(dir);
        if PROJECT_DIRS.contains(dir) {
            let mut projects = discover_project_dirs(&dir_path);
            projects.sort();
            assert!(
                !projects.is_empty(),
                "no project fixtures found in {}",
                dir_path.display()
            );
            for project in projects {
                let fixtures = discover_ts_files_recursive(&project);
                files_checked += fixtures.len();
                if let Err(project_failures) = run_project_fixture(&project, fixtures) {
                    failures.extend(project_failures);
                }
            }
            continue;
        }
        let mut fixtures = discover_ts_files(&dir_path);
        fixtures.sort();
        assert!(
            !fixtures.is_empty(),
            "no .ts fixtures found in {}",
            dir_path.display()
        );
        for fixture in fixtures {
            files_checked += 1;
            if let Err(file_failures) = run_fixture(&fixture) {
                failures.extend(file_failures);
            }
        }
    }

    assert!(files_checked > 0, "no fixtures were checked");

    if !failures.is_empty() {
        panic!(
            "conformance failures ({} across {} files):\n\n{}",
            failures.len(),
            files_checked,
            failures.join("\n\n")
        );
    }
}

/// Run one fixture; return `Err(messages)` on any mismatch.
fn run_fixture(path: &Path) -> Result<(), Vec<String>> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));

    let output = check_source(&source);
    compare_fixture_output(path, &source, &output)
}

fn run_project_fixture(project: &Path, mut fixtures: Vec<PathBuf>) -> Result<(), Vec<String>> {
    fixtures.sort();
    let mut inputs = Vec::with_capacity(fixtures.len());
    for fixture in &fixtures {
        let source = std::fs::read_to_string(fixture)
            .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", fixture.display()));
        inputs.push(FileInput {
            name: fixture.display().to_string(),
            source,
        });
    }
    let reports = check_project(inputs);
    let mut failures = Vec::new();
    for (fixture, report) in fixtures.iter().zip(&reports) {
        if let Err(file_failures) = compare_fixture_output(fixture, &report.source, &report.output)
        {
            failures.extend(file_failures);
        }
    }
    if reports.len() != fixtures.len() {
        failures.push(format!(
            "{}: project report count mismatch, expected {}, got {}",
            display_path(project),
            fixtures.len(),
            reports.len()
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn compare_fixture_output(
    path: &Path,
    source: &str,
    output: &typokat::driver::CheckOutput,
) -> Result<(), Vec<String>> {
    let expected = parse_markers(source);
    let line_index = LineIndex::new(source);

    // A parse error in an M0 fixture is always a harness/spec problem — surface
    // it loudly rather than masking it as "0 diagnostics".
    if !output.parse_errors.is_empty() {
        return Err(vec![format!(
            "{}: unexpected parse error(s): {}",
            display_path(path),
            output.parse_errors.join("; ")
        )]);
    }

    // line -> list of (code, rendered_text)
    let mut actual: BTreeMap<u32, Vec<(String, String)>> = BTreeMap::new();
    for diag in &output.diagnostics {
        let line = line_index.line_of(diag.span.start);
        actual
            .entry(line)
            .or_default()
            .push((diag.code.as_str().to_string(), diag.rendered_text()));
    }

    let mut failures = Vec::new();
    let all_lines: std::collections::BTreeSet<u32> =
        expected.keys().copied().chain(actual.keys().copied()).collect();

    for line in all_lines {
        let empty_exp = Vec::new();
        let empty_act = Vec::new();
        let exp = expected.get(&line).unwrap_or(&empty_exp);
        let act = actual.get(&line).unwrap_or(&empty_act);

        // 1. Multiset of codes must match exactly.
        let exp_codes = sorted_codes(exp.iter().map(|m| m.code.as_str()));
        let act_codes = sorted_codes(act.iter().map(|(c, _)| c.as_str()));
        if exp_codes != act_codes {
            failures.push(format!(
                "{}:{}: code mismatch\n    expected: {:?}\n    actual:   {:?}",
                display_path(path),
                line,
                exp_codes,
                act_codes,
            ));
            // Skip substring checks on this line when codes already disagree.
            continue;
        }

        // 2. Each expected substring must appear in SOME actual diagnostic on the
        //    line with the same code. Match greedily so duplicate codes on a line
        //    pair up correctly.
        let mut remaining: Vec<&(String, String)> = act.iter().collect();
        for marker in exp {
            let Some(substr) = &marker.substring else {
                continue;
            };
            let pos = remaining.iter().position(|(code, text)| {
                *code == marker.code && text.contains(substr)
            });
            match pos {
                Some(i) => {
                    remaining.remove(i);
                }
                None => {
                    let rendered: Vec<&str> = act
                        .iter()
                        .filter(|(c, _)| *c == marker.code)
                        .map(|(_, t)| t.as_str())
                        .collect();
                    failures.push(format!(
                        "{}:{}: no [{}] diagnostic contains substring {:?}\n    rendered: {:?}",
                        display_path(path),
                        line,
                        marker.code,
                        substr,
                        rendered,
                    ));
                }
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// Parse inline markers from a source file.
///
/// A marker lives in a trailing line-comment on the line where the diagnostic's
/// primary span starts. The recognized form is `error[TK<digits>]` optionally
/// followed by `: <substring>`. Multiple markers on one line are separated by
/// ` | `. Prose comments (not matching the `error[TK...]` pattern) are ignored.
fn parse_markers(source: &str) -> BTreeMap<u32, Vec<ExpectedMarker>> {
    let mut map: BTreeMap<u32, Vec<ExpectedMarker>> = BTreeMap::new();
    for (idx, line) in source.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        // Only the comment tail can carry markers. Find the first `//`.
        let Some(comment_at) = line.find("//") else {
            continue;
        };
        let comment = &line[comment_at + 2..];
        // Quick reject: a comment without `error[` carries no markers.
        if !comment.contains("error[") {
            continue;
        }
        for segment in comment.split(" | ") {
            if let Some(marker) = parse_one_marker(segment) {
                map.entry(line_no).or_default().push(marker);
            }
        }
    }
    map
}

/// Parse a single `error[CODE]` / `error[CODE]: substring` marker from a comment
/// segment, or `None` if the segment is not a marker.
fn parse_one_marker(segment: &str) -> Option<ExpectedMarker> {
    let segment = segment.trim();
    let rest = segment.strip_prefix("error[")?;
    let close = rest.find(']')?;
    let code = &rest[..close];
    // Validate the code shape: `TK` followed by digits. Ignores prose that
    // happens to contain "error[" but isn't a real marker.
    if !is_valid_code(code) {
        return None;
    }
    let after = rest[close + 1..].trim_start();
    let substring = after
        .strip_prefix(':')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(ExpectedMarker {
        code: code.to_string(),
        substring,
    })
}

/// A valid code is `TK` followed by one or more ASCII digits.
fn is_valid_code(code: &str) -> bool {
    match code.strip_prefix("TK") {
        Some(digits) => !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

fn sorted_codes<'a>(codes: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut v: Vec<String> = codes.map(str::to_string).collect();
    v.sort();
    v
}

/// All `.ts` files directly under `dir` (non-recursive; the corpus is flat per
/// milestone).
fn discover_ts_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("ts") {
            out.push(path);
        }
    }
    out
}

fn discover_project_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out
}

fn discover_ts_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    discover_ts_files_recursive_into(dir, &mut out);
    out.sort();
    out
}

fn discover_ts_files_recursive_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            discover_ts_files_recursive_into(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ts") {
            out.push(path);
        }
    }
}

/// The `tests/cases` directory, relative to the crate manifest.
fn cases_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("cases")
}

/// A short, stable path for failure messages (relative to `tests/cases`).
fn display_path(path: &Path) -> String {
    let root = cases_root();
    path.strip_prefix(&root)
        .unwrap_or(path)
        .display()
        .to_string()
}

//! Black-box CLI witnesses for the first-class incomplete outcome (sprint 2026-07-10, WU2).
//!
//! These drive the built `typokat` binary as a subprocess (isolated env, no in-process
//! env-var races) and assert the four-way exit contract:
//!   0 = complete clean · 1 = complete with type/parse diagnostics · 2 = usage/IO ·
//!   3 = incomplete (takes precedence over 1, all output still rendered).
//!
//! Nothing in the real checker emits an incomplete surface yet (WU3–5 do). To prove the
//! plumbing end to end, the checker carries a test-only emission hook gated on the
//! `TYPOKAT_TEST_EMIT_INCOMPLETE` env var — set here on the subprocess only.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_typokat");

/// Write `source` to a uniquely named temp `.ts` file and return its path.
fn temp_ts(tag: &str, source: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!(
        "typokat_wu2_{tag}_{nanos}_{}.ts",
        std::process::id()
    ));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(source.as_bytes()).unwrap();
    path
}

/// Run `typokat check <files...>`; `emit` sets the test-only incomplete hook value.
fn run_check(files: &[&PathBuf], format: &str, emit: Option<&str>) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.arg("check").arg("--format").arg(format);
    for f in files {
        cmd.arg(f);
    }
    match emit {
        Some(v) => {
            cmd.env("TYPOKAT_TEST_EMIT_INCOMPLETE", v);
        }
        None => {
            cmd.env_remove("TYPOKAT_TEST_EMIT_INCOMPLETE");
        }
    }
    cmd.output().expect("run typokat binary")
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("no signal")
}

fn combined(out: &Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stderr).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stdout));
    s
}

#[test]
fn exit_0_clean() {
    let f = temp_ts("clean", "const x: number = 1;\n");
    let out = run_check(&[&f], "rich", None);
    std::fs::remove_file(&f).ok();
    assert_eq!(code(&out), 0, "clean file must exit 0:\n{}", combined(&out));
    assert!(combined(&out).is_empty(), "clean file must be silent");
}

#[test]
fn exit_1_diagnostics() {
    let f = temp_ts("diag", "const x: number = \"nope\";\n");
    let out = run_check(&[&f], "rich", None);
    std::fs::remove_file(&f).ok();
    assert_eq!(code(&out), 1, "type error must exit 1:\n{}", combined(&out));
    assert!(combined(&out).contains("TK2322"));
}

#[test]
fn exit_2_usage() {
    // Unknown command → usage error → exit 2.
    let out = Command::new(BIN).arg("frobnicate").output().unwrap();
    assert_eq!(code(&out), 2, "usage error must exit 2");
    // Missing file argument is also usage.
    let out2 = Command::new(BIN).arg("check").output().unwrap();
    assert_eq!(code(&out2), 2);
}

#[test]
fn exit_3_incomplete() {
    let f = temp_ts("inc", "const x: number = 1;\n");
    let out = run_check(
        &[&f],
        "rich",
        Some("expr-infer/template-literal/interpolation"),
    );
    std::fs::remove_file(&f).ok();
    let text = combined(&out);
    assert_eq!(code(&out), 3, "incomplete must exit 3:\n{text}");
    assert!(
        text.contains("incomplete[expr-infer/template-literal/interpolation]"),
        "the incomplete record must render:\n{text}"
    );
}

#[test]
fn exit_3_takes_precedence_over_diagnostics_and_renders_both() {
    // A file that BOTH has a type error AND (via the hook) an incomplete surface.
    let f = temp_ts("both", "const x: number = \"nope\";\n");
    let out = run_check(&[&f], "rich", Some("stmt-check/try-statement/handler"));
    std::fs::remove_file(&f).ok();
    let text = combined(&out);
    // Precedence: exit 3, not 1.
    assert_eq!(code(&out), 3, "incomplete outranks diagnostics:\n{text}");
    // All output still renders: the diagnostic AND the incomplete record.
    assert!(
        text.contains("TK2322"),
        "diagnostic must still render:\n{text}"
    );
    assert!(
        text.contains("incomplete[stmt-check/try-statement/handler]"),
        "incomplete must still render:\n{text}"
    );
}

#[test]
fn incomplete_is_deterministic_and_deduped_rich_and_compact() {
    // Two distinct ids plus an exact duplicate of one; all at span 0..0.
    let emit = "b/two/self,a/one/slot,a/one/slot";
    let f = temp_ts("order", "const x: number = 1;\n");

    for format in ["rich", "compact"] {
        let out = run_check(&[&f], format, Some(emit));
        let text = combined(&out);
        assert_eq!(code(&out), 3, "{format}: exit 3 expected:\n{text}");
        // Duplicate suppressed → exactly two records.
        assert_eq!(
            text.matches("incomplete[").count(),
            2,
            "{format}: duplicate id+span must be suppressed:\n{text}"
        );
        // Deterministic order: both ids share span 0, so ties break by id → a/one before b/two.
        let a = text.find("incomplete[a/one/slot]").unwrap();
        let b = text.find("incomplete[b/two/self]").unwrap();
        assert!(a < b, "{format}: records must be in stable order:\n{text}");
    }
    std::fs::remove_file(&f).ok();
}

#[test]
fn project_aggregation_reports_incomplete_per_file() {
    // Two independent files checked as one project; each gets a record via the hook.
    let a = temp_ts("proj_a", "const a: number = 1;\n");
    let b = temp_ts("proj_b", "const b: number = 2;\n");
    let out = run_check(
        &[&a, &b],
        "compact",
        Some("annotation-lower/type-query/typeof"),
    );
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
    let text = combined(&out);
    assert_eq!(code(&out), 3, "project incomplete must exit 3:\n{text}");
    // Both files' records render, keyed to their own paths.
    let a_name = a.file_name().unwrap().to_string_lossy().into_owned();
    let b_name = b.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        text.contains(&a_name) && text.contains("incomplete["),
        "file a record:\n{text}"
    );
    assert!(text.contains(&b_name), "file b record:\n{text}");
    assert_eq!(
        text.matches("incomplete[annotation-lower/type-query/typeof]")
            .count(),
        2,
        "one record per file:\n{text}"
    );
}

//! The named pin for the library's own records (ADR-0018).
//!
//! Compiling the pinned 82-file profile reports diagnostics and incomplete surfaces against the
//! library itself. They are not user-facing errors — they are typokat's own model gaps against a
//! library the real `tsc` checks clean — so nothing retains them: the frozen base carries none
//! and every ordinary check pays nothing for them. That makes this file their sole witness, and
//! it is a named `(code, site)` multiset rather than a count and a digest, because a digest only
//! ever says that something moved. Backlog `98` is what that costs: an eight-diagnostic delta
//! drifted for 102 commits behind a hash.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use typokat::driver::{check_project_with_library, check_source_with_library};
use typokat::frontend::FileInput;
use typokat::library::LibraryRecordCensus;

/// The committed multiset. Relative to the repository root, which is the test working directory.
const PIN_PATH: &str = "tests/fixtures/library-owned-records.txt";
/// Rewrite the pin instead of failing. Deliberate, and never set in CI.
const BLESS: &str = "TYPOKAT_BLESS_LIBRARY_RECORDS";

const PINNED_DIAGNOSTICS: usize = 265;
const PINNED_INCOMPLETES: usize = 610;

fn repository_root() -> PathBuf {
    let root = std::env::current_dir().expect("test process current directory");
    assert!(
        root.join("Cargo.lock").is_file() && root.join("src/lib.rs").is_file(),
        "tests must run from the typokat repository root"
    );
    root
}

/// The explicit means: one command materializes the library's own records and names them.
///
/// ```text
/// cargo test --test library_owned_records
/// TYPOKAT_BLESS_LIBRARY_RECORDS=1 cargo test --test library_owned_records   # rewrite the pin
/// ```
#[test]
fn library_owned_records_match_their_named_pin() {
    let census = LibraryRecordCensus::compile_packaged_profile().expect("library record census");
    let rendered = census.render();
    let pin = repository_root().join(PIN_PATH);

    if std::env::var_os(BLESS).is_some() {
        fs::write(&pin, &rendered).expect("rewrite the library record pin");
        println!(
            "{BLESS} rewrote {PIN_PATH}: {} diagnostics, {} incompletes",
            census.diagnostics(),
            census.incompletes()
        );
        return;
    }

    let pinned = read_pin(&pin);
    let difference = census.difference_from(&pinned);
    assert!(
        difference.is_empty(),
        "the library's own records drifted from {PIN_PATH}.\n\
         '-' is an outcome the pin carries that the checker no longer produces — the direction \
         that hides a dropped diagnostic. '+' is a new one. A '-'/'+' pair at the same code and \
         site is a rendering change, not a lost outcome.\n{difference}\
         Re-pin with: {BLESS}=1 cargo test --test library_owned_records"
    );
    assert_eq!(rendered, pinned, "the pin header drifted from the census");
    assert_eq!(census.diagnostics(), PINNED_DIAGNOSTICS);
    assert_eq!(census.incompletes(), PINNED_INCOMPLETES);
    // Visible under `-- --nocapture`: the whole set is one file away, by name.
    println!(
        "{} library-owned records ({} diagnostics, {} incompletes) — see {PIN_PATH}",
        census.entries().len(),
        census.diagnostics(),
        census.incompletes()
    );
}

/// Every pinned entry names both what it is and where it is, so a drift is attributable.
#[test]
fn every_pinned_entry_names_a_code_and_a_site() {
    let pinned = read_pin(&repository_root().join(PIN_PATH));
    let mut records = 0usize;
    for line in pinned.lines().filter(|line| !line.starts_with('#')) {
        let columns = line.split('\t').collect::<Vec<_>>();
        assert_eq!(columns.len(), 4, "malformed census line: {line}");
        let [kind, name, site, _detail] = columns[..] else {
            unreachable!("a four-column line has four columns")
        };
        assert!(
            kind == "diagnostic" || kind == "incomplete",
            "unknown record kind {kind:?}"
        );
        assert!(!name.is_empty(), "unnamed record: {line}");
        let (file, position) = site.rsplit_once(':').expect("site carries a column");
        let (file, line_number) = file.rsplit_once(':').expect("site carries a line");
        assert!(
            file.ends_with(".d.ts"),
            "site is not a library file: {site}"
        );
        assert!(line_number.parse::<u32>().expect("1-based line") >= 1);
        assert!(position.parse::<u32>().expect("1-based column") >= 1);
        records += 1;
    }
    assert_eq!(records, PINNED_DIAGNOSTICS + PINNED_INCOMPLETES);
}

/// No library-owned record reaches a user's output on the library-backed driver path.
///
/// This is the production shape, not a reading of the source: `check_project_with_library` and
/// `check_source_with_library` are the entry points the WU7 cutover moves the CLI onto. The
/// library contributes 875 records on the way to the base; a user check must see none of them,
/// and must still see its own.
#[test]
fn no_library_owned_record_reaches_user_output() {
    let census = LibraryRecordCensus::compile_packaged_profile().expect("library record census");
    assert!(
        census.entries().len() > 800,
        "the library must actually own records for this proof to mean anything"
    );

    let clean = "export const greeting: string = \"typokat\";\n";
    let faulty = "export const count: number = \"not a number\";\n";
    let reports = check_project_with_library(vec![
        FileInput {
            name: "/probe/clean.ts".to_owned(),
            source: clean.to_owned(),
        },
        FileInput {
            name: "/probe/faulty.ts".to_owned(),
            source: faulty.to_owned(),
        },
    ])
    .expect("the library-backed project path publishes its base");

    assert_eq!(reports.len(), 2);
    let clean_report = &reports[0];
    assert_eq!(clean_report.name, "/probe/clean.ts");
    assert!(
        clean_report.output.diagnostics.is_empty()
            && clean_report.output.incomplete.is_empty()
            && clean_report.output.parse_errors.is_empty(),
        "a clean user file reported {:?} / {:?}",
        clean_report.output.diagnostics,
        clean_report.output.incomplete
    );

    let faulty_report = &reports[1];
    assert_eq!(faulty_report.name, "/probe/faulty.ts");
    assert!(faulty_report.output.incomplete.is_empty());
    assert_eq!(faulty_report.output.diagnostics.len(), 1);
    let reported = &faulty_report.output.diagnostics[0];
    assert_eq!(reported.code.as_str(), "TK2322");
    // A library record would carry a span into a library source, which the user file cannot hold.
    let user_length = u32::try_from(faulty.len()).expect("probe source length");
    assert!(reported.span.start < user_length && reported.span.end <= user_length);

    let single = check_source_with_library(clean).expect("the library-backed single-file path");
    assert!(
        single.diagnostics.is_empty()
            && single.incomplete.is_empty()
            && single.parse_errors.is_empty()
    );
}

/// The same proof at the process boundary: the CLI prints nothing for a clean file.
///
/// The CLI still forks from `src/prelude.ts` until the WU7 cutover, so this is the weaker half
/// of the pair above — it holds the boundary while the entry point moves.
#[test]
fn the_cli_prints_no_record_for_a_clean_file() {
    let probe = std::env::temp_dir().join(format!(
        "typokat-cli-record-probe-{}.ts",
        std::process::id()
    ));
    fs::write(&probe, "export const greeting: string = \"typokat\";\n").expect("write CLI probe");

    let output = Command::new(env!("CARGO_BIN_EXE_typokat"))
        .args(["check", "--format", "compact"])
        .arg(&probe)
        .output()
        .expect("run the typokat CLI");
    let _ = fs::remove_file(&probe);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "the CLI printed for a clean file:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_pin(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("cannot read {PIN_PATH}: {error}. Generate it with {BLESS}=1 cargo test --test library_owned_records")
    })
}

//! typokat CLI entry point.
//!
//! One command: `typokat check <file.ts>...`, which parses and checks one or more
//! files (in parallel across files — [`check_files`]), renders any diagnostics to
//! stderr per file, and exits non-zero if any file has errors. A single-file
//! invocation renders exactly as it always has. All pipeline logic lives in the
//! `typokat` library crate (`lib.rs`).

use std::io::Write;
use std::process::ExitCode;

use typokat::diagnostics;
use typokat::driver::{check_files, FileInput};

/// Exit code for a usage error (bad/missing arguments).
const EXIT_USAGE: u8 = 2;
/// Exit code when the file has type/parse errors.
const EXIT_ERRORS: u8 = 1;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(had_errors) => {
            if had_errors {
                ExitCode::from(EXIT_ERRORS)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Parse arguments and dispatch. Returns `Ok(had_errors)` on a well-formed
/// invocation, or `Err(message)` for a usage/IO error. No panics: all failure
/// modes are reported gracefully.
fn run(args: &[String]) -> Result<bool, String> {
    // args[0] is the program name.
    let command = args.get(1).map(String::as_str);
    match command {
        Some("check") => {
            let paths = &args[2..];
            if paths.is_empty() {
                return Err(
                    "missing <file> argument\nusage: typokat check <file.ts>...".to_string(),
                );
            }
            check_paths(paths)
        }
        Some(other) => Err(format!(
            "unknown command '{other}'\nusage: typokat check <file.ts>..."
        )),
        None => Err("usage: typokat check <file.ts>...".to_string()),
    }
}

/// Read, check (in parallel across files), and report. Returns `Ok(had_errors)`.
///
/// Every file is read up front; a read failure aborts with a usage/IO error (exit
/// 2), exactly as the single-file path always did. The per-file rendering is
/// unchanged — parse errors first, then type diagnostics — so the official-suite
/// harness (which shells `typokat check <file>` and substring-matches stderr)
/// sees identical output for a single file; multiple files just repeat the block,
/// each under its own name.
fn check_paths(paths: &[String]) -> Result<bool, String> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        let source = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read '{path}': {e}"))?;
        inputs.push(FileInput {
            name: path.clone(),
            source,
        });
    }

    let reports = check_files(inputs);

    let stderr = std::io::stderr();
    let mut handle = stderr.lock();

    let mut had_errors = false;
    for report in &reports {
        for parse_error in &report.output.parse_errors {
            // Parser errors come pre-formatted from oxc; surface them plainly.
            let _ = writeln!(handle, "error: {parse_error}");
        }

        diagnostics::render_to_writer(
            &mut handle,
            &report.name,
            &report.source,
            &report.output.diagnostics,
        )
        .map_err(|e| format!("failed to render diagnostics: {e}"))?;

        had_errors |= report.output.has_errors();
    }

    Ok(had_errors)
}

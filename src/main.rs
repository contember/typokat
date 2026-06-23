//! typokat CLI entry point.
//!
//! M0 supports one command: `typokat check <file.ts>`, which parses, checks,
//! renders any diagnostics to stderr, and exits non-zero if the file has errors.
//! All pipeline logic lives in the `typokat` library crate (`lib.rs`).

use std::io::Write;
use std::process::ExitCode;

use typokat::diagnostics;
use typokat::driver::check_source;

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
            let path = args.get(2).ok_or_else(|| {
                "missing <file> argument\nusage: typokat check <file.ts>".to_string()
            })?;
            if args.len() > 3 {
                return Err(
                    "too many arguments\nusage: typokat check <file.ts>".to_string()
                );
            }
            check_file(path)
        }
        Some(other) => Err(format!(
            "unknown command '{other}'\nusage: typokat check <file.ts>"
        )),
        None => Err("usage: typokat check <file.ts>".to_string()),
    }
}

/// Read, check, and report on a single file. Returns `Ok(had_errors)`.
fn check_file(path: &str) -> Result<bool, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{path}': {e}"))?;

    let output = check_source(&source);

    // Render parse errors first (if any), then type diagnostics, to stderr.
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();

    for parse_error in &output.parse_errors {
        // Parser errors come pre-formatted from oxc; surface them plainly.
        let _ = writeln!(handle, "error: {parse_error}");
    }

    diagnostics::render_to_writer(&mut handle, path, &source, &output.diagnostics)
        .map_err(|e| format!("failed to render diagnostics: {e}"))?;

    Ok(output.has_errors())
}

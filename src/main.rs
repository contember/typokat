//! typokat CLI entry point.
//!
//! Implements `typokat check [--format rich|compact] <file.ts>...`, checking the
//! inputs as one local-relative project and rendering diagnostics to stderr.

use std::io::Write;
use std::process::ExitCode;

use typokat::diagnostics::{self, DiagnosticFormat};
use typokat::driver::{check_project, FileInput};

/// Exit code for a usage error (bad/missing arguments).
const EXIT_USAGE: u8 = 2;
/// Exit code when the file has type/parse errors.
const EXIT_ERRORS: u8 = 1;
const USAGE: &str = "usage: typokat check [--format rich|compact] <file.ts>...";

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
            let check_args = parse_check_args(&args[2..])?;
            check_paths(&check_args.paths, check_args.format)
        }
        Some(other) => Err(format!("unknown command '{other}'\n{USAGE}")),
        None => Err(USAGE.to_string()),
    }
}

struct CheckArgs {
    format: DiagnosticFormat,
    paths: Vec<String>,
}

fn parse_check_args(args: &[String]) -> Result<CheckArgs, String> {
    let mut format = DiagnosticFormat::Rich;
    let mut paths = Vec::new();
    let mut after_options = false;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        if !after_options && arg == "--" {
            after_options = true;
            i += 1;
            continue;
        }
        if !after_options && arg == "--format" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| format!("missing value for --format\n{USAGE}"))?;
            format = parse_diagnostic_format(value)?;
            i += 2;
            continue;
        }
        if !after_options {
            if let Some(value) = arg.strip_prefix("--format=") {
                format = parse_diagnostic_format(value)?;
                i += 1;
                continue;
            }
            if arg == "--compact" {
                format = DiagnosticFormat::Compact;
                i += 1;
                continue;
            }
            if arg.starts_with("--") {
                return Err(format!("unknown option '{arg}'\n{USAGE}"));
            }
        }

        paths.push(arg.clone());
        i += 1;
    }

    if paths.is_empty() {
        return Err(format!("missing <file> argument\n{USAGE}"));
    }

    Ok(CheckArgs { format, paths })
}

fn parse_diagnostic_format(value: &str) -> Result<DiagnosticFormat, String> {
    match value {
        "rich" => Ok(DiagnosticFormat::Rich),
        "compact" => Ok(DiagnosticFormat::Compact),
        _ => Err(format!(
            "unknown diagnostic format '{value}'; expected 'rich' or 'compact'\n{USAGE}"
        )),
    }
}

/// Read inputs, check them as one local-relative project, and report diagnostics.
/// Read failures remain usage/IO errors; single-file rendering is unchanged for
/// the official-suite harness.
fn check_paths(paths: &[String], format: DiagnosticFormat) -> Result<bool, String> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        let source =
            std::fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
        inputs.push(FileInput {
            name: path.clone(),
            source,
        });
    }

    let reports = check_project(inputs);

    let stderr = std::io::stderr();
    let mut handle = std::io::BufWriter::new(stderr.lock());

    let mut had_errors = false;
    for report in &reports {
        for parse_error in &report.output.parse_errors {
            // Parser errors come pre-formatted from oxc; surface them plainly.
            let _ = writeln!(handle, "error: {parse_error}");
        }

        diagnostics::render_to_writer_with_format(
            &mut handle,
            &report.name,
            &report.source,
            &report.output.diagnostics,
            format,
        )
        .map_err(|e| format!("failed to render diagnostics: {e}"))?;

        had_errors |= report.output.has_errors();
    }

    handle
        .flush()
        .map_err(|e| format!("failed to flush diagnostics: {e}"))?;

    Ok(had_errors)
}

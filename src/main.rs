//! typokat CLI entry point.
//!
//! Implements project checking and default-library provider introspection.

use std::io::Write;
use std::process::ExitCode;

use typokat::diagnostics::{self, DiagnosticFormat};
use typokat::driver::check_project;
use typokat::frontend::FileInput;

// jemalloc: the run is allocation-churn heavy (building the default library from
// source), and glibc malloc costs ~20 ms of it. Declared here, not in the library:
// `#[global_allocator]` is a whole-program choice that belongs to the binary.
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Exit code when the file has type/parse errors (complete run, errors found).
const EXIT_ERRORS: u8 = 1;
/// Exit code for a usage error (bad/missing arguments).
const EXIT_USAGE: u8 = 2;
/// Exit code when the run is incomplete — the checker skipped an in-scope surface.
/// Takes precedence over [`EXIT_ERRORS`] even when ordinary diagnostics also exist.
const EXIT_INCOMPLETE: u8 = 3;
const USAGE: &str = "usage: typokat check [--format rich|compact] <file.ts>...";
const LIBRARY_INFO_USAGE: &str = "usage: typokat library-info --format json";
const LIBRARY_INFO_SCHEMA: u32 = 1;
const CURRENT_LIBRARY_ROUTE: &str = "prelude";

/// The outcome of a well-formed `check` invocation. Distinct from a usage/IO error
/// (which is `Err` and maps to exit `2`). Incomplete outranks diagnostics: a run that
/// both skipped a surface and found errors is `Incomplete`.
enum CheckStatus {
    /// No diagnostics, no parse errors, no incomplete surfaces → exit `0`.
    Clean,
    /// Type/parse diagnostics, and nothing incomplete → exit `1`.
    Diagnostics,
    /// At least one incomplete surface (regardless of diagnostics) → exit `3`.
    Incomplete,
}

impl CheckStatus {
    fn exit_code(self) -> ExitCode {
        match self {
            CheckStatus::Clean => ExitCode::SUCCESS,
            CheckStatus::Diagnostics => ExitCode::from(EXIT_ERRORS),
            CheckStatus::Incomplete => ExitCode::from(EXIT_INCOMPLETE),
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(status) => status.exit_code(),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Parse arguments and dispatch. Returns `Ok(status)` on a well-formed invocation,
/// or `Err(message)` for a usage/IO error. No panics: all failure modes are reported
/// gracefully.
fn run(args: &[String]) -> Result<CheckStatus, String> {
    // args[0] is the program name.
    let command = args.get(1).map(String::as_str);
    match command {
        Some("check") => {
            let check_args = parse_check_args(&args[2..])?;
            check_paths(&check_args.paths, check_args.format)
        }
        Some("library-info") => {
            parse_library_info_args(&args[2..])?;
            write_library_info()?;
            Ok(CheckStatus::Clean)
        }
        Some(other) => Err(format!("unknown command '{other}'\n{USAGE}")),
        None => Err(USAGE.to_string()),
    }
}

fn parse_library_info_args(args: &[String]) -> Result<(), String> {
    let format = match args {
        [option, value] if option == "--format" => value.as_str(),
        [option] if option == "--format" => {
            return Err(format!("missing value for --format\n{LIBRARY_INFO_USAGE}"));
        }
        [option] => option
            .strip_prefix("--format=")
            .ok_or_else(|| format!("unknown option '{option}'\n{LIBRARY_INFO_USAGE}"))?,
        [] => return Err(format!("missing --format argument\n{LIBRARY_INFO_USAGE}")),
        _ => {
            return Err(format!(
                "unexpected library-info arguments\n{LIBRARY_INFO_USAGE}"
            ));
        }
    };
    if format != "json" {
        return Err(format!(
            "unknown library-info format '{format}'; expected 'json'\n{LIBRARY_INFO_USAGE}"
        ));
    }
    Ok(())
}

fn write_library_info() -> Result<(), String> {
    let metadata = typokat::library::embedded_library_profile_metadata();
    let stdout = std::io::stdout();
    let mut handle = std::io::BufWriter::new(stdout.lock());
    writeln!(
        handle,
        "{{\"schema\":{},\"profile_sha256\":\"{}\",\"file_count\":{},\"provider_route\":\"{}\"}}",
        LIBRARY_INFO_SCHEMA,
        metadata.profile_identity(),
        metadata.file_count(),
        CURRENT_LIBRARY_ROUTE,
    )
    .map_err(|error| format!("failed to write library info: {error}"))?;
    handle
        .flush()
        .map_err(|error| format!("failed to flush library info: {error}"))
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

/// Read inputs, check them as one local-relative project, and report diagnostics
/// plus incomplete surfaces. Read failures remain usage/IO errors. All three channels
/// (parse errors, diagnostics, incomplete) are always rendered; the returned status
/// gives exit `3` precedence over exit `1` when any surface was incomplete.
fn check_paths(paths: &[String], format: DiagnosticFormat) -> Result<CheckStatus, String> {
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
    let mut had_incomplete = false;
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

        // The incomplete channel renders separately (no TK code); still shown even
        // when the same file also has diagnostics.
        diagnostics::render_incomplete_to_writer_with_format(
            &mut handle,
            &report.name,
            &report.source,
            &report.output.incomplete,
            format,
        )
        .map_err(|e| format!("failed to render incomplete surfaces: {e}"))?;

        had_errors |= report.output.has_errors();
        had_incomplete |= report.output.is_incomplete();
    }

    handle
        .flush()
        .map_err(|e| format!("failed to flush diagnostics: {e}"))?;

    // Precedence: incomplete outranks diagnostics.
    Ok(if had_incomplete {
        CheckStatus::Incomplete
    } else if had_errors {
        CheckStatus::Diagnostics
    } else {
        CheckStatus::Clean
    })
}

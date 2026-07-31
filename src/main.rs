//! typokat CLI entry point.
//!
//! Implements project checking and default-library provider introspection.

use std::io::{BufRead, Write};
use std::process::ExitCode;

use typokat::diagnostics::{self, DiagnosticFormat};
use typokat::driver::{check_project, check_project_with_library, FileReport};
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
const OFFICIAL_BATCH_SCHEMA: u64 = 1;
const OFFICIAL_BATCH_MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
const OFFICIAL_BATCH_MAX_OUTPUT_BYTES: usize = 1024 * 1024;
// Temporary until the atomic cutover replaces `check_project_with_library` below;
// the final worker must derive this attestation from `production_library_route`.
const OFFICIAL_BATCH_PROVIDER_ROUTE: &str = "production-default-library";

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
        Some("official-batch") => {
            if args.len() != 2 {
                return Err("official-batch takes no arguments".to_owned());
            }
            run_official_batch()?;
            Ok(CheckStatus::Clean)
        }
        Some(other) => Err(format!("unknown command '{other}'\n{USAGE}")),
        None => Err(USAGE.to_string()),
    }
}

struct OfficialBatchRequest {
    case_id: String,
    name: String,
    source: String,
}

enum BoundedFrame {
    Eof,
    Data(Vec<u8>),
}

fn read_official_batch_frame<R: BufRead>(reader: &mut R) -> Result<BoundedFrame, String> {
    let mut frame = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .map_err(|error| format!("failed to read stdin: {error}"))?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(BoundedFrame::Eof)
            } else {
                Err("unterminated JSONL frame".to_owned())
            };
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |position| position + 1);
        let content_len = newline.unwrap_or(available.len());
        // The wire cap includes the mandatory JSONL delimiter.
        if frame.len().saturating_add(content_len) > OFFICIAL_BATCH_MAX_FRAME_BYTES - 1 {
            return Err(format!(
                "frame exceeds {OFFICIAL_BATCH_MAX_FRAME_BYTES} bytes"
            ));
        }
        frame.extend_from_slice(&available[..content_len]);
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(BoundedFrame::Data(frame));
        }
    }
}

fn parse_official_batch_request(frame: &[u8]) -> Result<OfficialBatchRequest, String> {
    let value: serde_json::Value =
        serde_json::from_slice(frame).map_err(|error| format!("invalid JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "request must be a JSON object".to_owned())?;
    const KEYS: [&str; 4] = ["schema", "case_id", "name", "source"];
    if object.len() != KEYS.len() || KEYS.iter().any(|key| !object.contains_key(*key)) {
        return Err("request must contain exactly schema, case_id, name, and source".to_owned());
    }
    if object.get("schema").and_then(serde_json::Value::as_u64) != Some(OFFICIAL_BATCH_SCHEMA) {
        return Err(format!("schema must be {OFFICIAL_BATCH_SCHEMA}"));
    }

    let required_string = |key: &str| -> Result<String, String> {
        let value = object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{key} must be a string"))?;
        if value.is_empty() {
            return Err(format!("{key} must not be empty"));
        }
        Ok(value.to_owned())
    };
    Ok(OfficialBatchRequest {
        case_id: required_string("case_id")?,
        name: required_string("name")?,
        source: object
            .get("source")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "source must be a string".to_owned())?
            .to_owned(),
    })
}

struct BoundedOutput {
    bytes: Vec<u8>,
}

impl BoundedOutput {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn into_string(self) -> Result<String, String> {
        String::from_utf8(self.bytes)
            .map_err(|error| format!("diagnostic output was not UTF-8: {error}"))
    }
}

impl Write for BoundedOutput {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(buffer.len()) > OFFICIAL_BATCH_MAX_OUTPUT_BYTES {
            return Err(std::io::Error::other(format!(
                "official-batch case output exceeds {OFFICIAL_BATCH_MAX_OUTPUT_BYTES} bytes"
            )));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn render_official_batch_reports(reports: &[FileReport]) -> Result<(CheckStatus, String), String> {
    let mut output = BoundedOutput::new();
    let mut had_errors = false;
    let mut had_incomplete = false;
    for report in reports {
        for parse_error in &report.output.parse_errors {
            writeln!(output, "error: {parse_error}")
                .map_err(|error| format!("failed to render parse error: {error}"))?;
        }
        diagnostics::render_to_writer_with_format(
            &mut output,
            &report.name,
            &report.source,
            &report.output.diagnostics,
            DiagnosticFormat::Rich,
        )
        .map_err(|error| format!("failed to render diagnostics: {error}"))?;
        diagnostics::render_incomplete_to_writer_with_format(
            &mut output,
            &report.name,
            &report.source,
            &report.output.incomplete,
            DiagnosticFormat::Rich,
        )
        .map_err(|error| format!("failed to render incomplete surfaces: {error}"))?;
        had_errors |= report.output.has_errors();
        had_incomplete |= report.output.is_incomplete();
    }
    let status = if had_incomplete {
        CheckStatus::Incomplete
    } else if had_errors {
        CheckStatus::Diagnostics
    } else {
        CheckStatus::Clean
    };
    Ok((status, output.into_string()?))
}

fn official_batch_exit_code(status: CheckStatus) -> u8 {
    match status {
        CheckStatus::Clean => 0,
        CheckStatus::Diagnostics => EXIT_ERRORS,
        CheckStatus::Incomplete => EXIT_INCOMPLETE,
    }
}

fn run_official_batch_case(request: OfficialBatchRequest) -> Result<Vec<u8>, String> {
    let reports = check_project_with_library(vec![FileInput {
        name: request.name,
        source: request.source,
    }])
    .map_err(|error| format!("failed to initialize embedded TypeScript 6.0.3 library: {error}"))?;
    let (status, stderr) = render_official_batch_reports(&reports)?;
    let metadata = typokat::library::embedded_library_profile_metadata();
    let response = serde_json::json!({
        "schema": OFFICIAL_BATCH_SCHEMA,
        "case_id": request.case_id,
        "worker_pid": std::process::id(),
        "provider_route": OFFICIAL_BATCH_PROVIDER_ROUTE,
        "profile_sha256": metadata.profile_identity(),
        "exit_code": official_batch_exit_code(status),
        "stdout": "",
        "stderr": stderr,
    });
    let mut frame = serde_json::to_vec(&response)
        .map_err(|error| format!("failed to serialize official-batch response: {error}"))?;
    if frame.len().saturating_add(1) > OFFICIAL_BATCH_MAX_FRAME_BYTES {
        return Err(format!(
            "official-batch response exceeds {OFFICIAL_BATCH_MAX_FRAME_BYTES} bytes"
        ));
    }
    frame.push(b'\n');
    Ok(frame)
}

fn run_official_batch() -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut reader = std::io::BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut writer = std::io::BufWriter::new(stdout.lock());
    loop {
        let frame = match read_official_batch_frame(&mut reader) {
            Ok(BoundedFrame::Eof) => break,
            Ok(BoundedFrame::Data(frame)) => frame,
            Err(error) => return Err(format!("malformed official-batch request: {error}")),
        };
        let request = parse_official_batch_request(&frame)
            .map_err(|error| format!("malformed official-batch request: {error}"))?;
        let response = run_official_batch_case(request)?;
        writer
            .write_all(&response)
            .map_err(|error| format!("failed to write official-batch response: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("failed to flush official-batch response: {error}"))?;
    }
    Ok(())
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

#[cfg(test)]
mod wu7_cli_fault_spec;

//! Terminal rendering of diagnostics (rich `codespan` output and compact tsc-style).

use super::{Diagnostic, DiagnosticFormat, Severity};
use crate::span::LineIndex;
use codespan_reporting::diagnostic::{Diagnostic as CsDiagnostic, Label};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{self, Config};

/// Render a batch of diagnostics for one file to a writer (the CLI uses stderr),
/// via `codespan-reporting`. Returns an IO/formatting error rather than
/// panicking. Color is intentionally off (plain writer) for stable, dependency-
/// light output.
pub fn render_to_writer(
    writer: &mut impl std::io::Write,
    file_name: &str,
    source: &str,
    diagnostics: &[Diagnostic],
) -> std::io::Result<()> {
    render_to_writer_with_format(
        writer,
        file_name,
        source,
        diagnostics,
        DiagnosticFormat::Rich,
    )
}

/// Render diagnostics in the requested human-facing format.
pub fn render_to_writer_with_format(
    writer: &mut impl std::io::Write,
    file_name: &str,
    source: &str,
    diagnostics: &[Diagnostic],
    format: DiagnosticFormat,
) -> std::io::Result<()> {
    if diagnostics.is_empty() {
        return Ok(());
    }

    match format {
        DiagnosticFormat::Rich => render_rich_to_writer(writer, file_name, source, diagnostics),
        DiagnosticFormat::Compact => {
            render_compact_to_writer(writer, file_name, source, diagnostics)
        }
    }
}

fn render_rich_to_writer(
    writer: &mut impl std::io::Write,
    file_name: &str,
    source: &str,
    diagnostics: &[Diagnostic],
) -> std::io::Result<()> {
    let file = SimpleFile::new(file_name, source);
    let config = Config::default();
    for diag in diagnostics {
        let cs = to_codespan(diag);
        // The only `Error` cases here are a missing file id (we always pass id
        // `()`) or an IO failure; map both to an IO error for the caller.
        term::emit_to_io_write(writer, &config, &file, &cs)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    Ok(())
}

fn render_compact_to_writer(
    writer: &mut impl std::io::Write,
    file_name: &str,
    source: &str,
    diagnostics: &[Diagnostic],
) -> std::io::Result<()> {
    let line_index = LineIndex::new(source);
    for diag in diagnostics {
        let pos = line_index.line_col(diag.span.start);
        let severity = match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        writeln!(
            writer,
            "{}({},{}): {} {}: {}",
            file_name,
            pos.line,
            pos.column,
            severity,
            diag.code.as_str(),
            diag.message
        )?;
        for line in &diag.elaboration {
            writeln!(writer, "{line}")?;
        }
    }
    Ok(())
}

/// Convert our structured diagnostic into a codespan-reporting one. `SimpleFile`
/// uses `()` as its file id. The reason-chain elaboration (M6) is attached as a
/// trailing note so the nested "because…" cascade shows under the primary label
/// in the human-facing CLI output, readably indented (already done in the lines).
fn to_codespan(diag: &Diagnostic) -> CsDiagnostic<()> {
    let base = match diag.severity {
        Severity::Error => CsDiagnostic::error(),
        Severity::Warning => CsDiagnostic::warning(),
    };
    let cs = base
        .with_code(diag.code.as_str())
        .with_message(diag.message.clone())
        .with_labels(vec![Label::primary((), diag.span.range())]);
    if diag.elaboration.is_empty() {
        cs
    } else {
        cs.with_notes(vec![diag.elaboration.join("\n")])
    }
}

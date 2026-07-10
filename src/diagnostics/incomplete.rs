//! The third structured checker channel: incomplete-surface records.
//!
//! An `IncompleteSurface` marks an **in-scope** AST position the checker did not
//! visit — a skipped child slot, statement container, or annotation form that would
//! otherwise exit clean while `tsc` rejects it (sprint 2026-07-10, WU2). It is NOT a
//! diagnostic: it carries no `TK` code and drives exit `3` (incomplete), which takes
//! precedence over ordinary diagnostics. Nothing in the checker emits one yet — WU3–5
//! wire the emissions; this module is the plumbing plus its rendering.

use super::DiagnosticFormat;
use crate::span::{LineIndex, Span};

/// One skipped in-scope surface. `id` is the stable `role/surface/slot-or-variant`
/// identity (see `tests/surface/README.md`), independent of the oxc variant name;
/// `context` is owner-facing prose (which layer skipped it / who owns the gap). The
/// file name is supplied at render time, exactly like [`super::Diagnostic`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncompleteSurface {
    /// Stable surface identity `role/surface/slot-or-variant`.
    pub id: String,
    /// Primary span of the skipped position; its start maps to a 1-based line.
    pub span: Span,
    /// Owner-facing context (the layer/owner of the gap).
    pub context: String,
}

impl IncompleteSurface {
    /// Construct a record from a stable id, its primary span, and owner-facing context.
    pub fn new(id: impl Into<String>, span: Span, context: impl Into<String>) -> Self {
        IncompleteSurface {
            id: id.into(),
            span,
            context: context.into(),
        }
    }

    /// The headline text mirroring `Diagnostic::rendered_text` — `incomplete[<id>]: <context>`.
    pub fn rendered_text(&self) -> String {
        format!("incomplete[{}]: {}", self.id, self.context)
    }
}

/// Render incomplete records for one file in the default (rich) format.
pub fn render_incomplete_to_writer(
    writer: &mut impl std::io::Write,
    file_name: &str,
    source: &str,
    records: &[IncompleteSurface],
) -> std::io::Result<()> {
    render_incomplete_to_writer_with_format(
        writer,
        file_name,
        source,
        records,
        DiagnosticFormat::Rich,
    )
}

/// Render incomplete records in the requested human-facing format. Order is
/// deterministic (by span start, then id) and duplicates — the same `id` at the
/// same span — are suppressed so one skipped position reports once.
pub fn render_incomplete_to_writer_with_format(
    writer: &mut impl std::io::Write,
    file_name: &str,
    source: &str,
    records: &[IncompleteSurface],
    format: DiagnosticFormat,
) -> std::io::Result<()> {
    if records.is_empty() {
        return Ok(());
    }

    let ordered = ordered_deduped(records);
    let line_index = LineIndex::new(source);

    for rec in ordered {
        let pos = line_index.line_col(rec.span.start);
        match format {
            DiagnosticFormat::Rich => {
                writeln!(writer, "incomplete[{}]: {}", rec.id, rec.context)?;
                writeln!(writer, "  --> {}:{}:{}", file_name, pos.line, pos.column)?;
            }
            DiagnosticFormat::Compact => {
                writeln!(
                    writer,
                    "{}({},{}): incomplete[{}]: {}",
                    file_name, pos.line, pos.column, rec.id, rec.context
                )?;
            }
        }
    }
    Ok(())
}

/// Sort by `(span.start, span.end, id)` and drop exact `(id, span)` duplicates, so
/// rendering is deterministic and one skipped position reports once.
fn ordered_deduped(records: &[IncompleteSurface]) -> Vec<&IncompleteSurface> {
    let mut ordered: Vec<&IncompleteSurface> = records.iter().collect();
    ordered.sort_by(|a, b| {
        (a.span.start, a.span.end, a.id.as_str()).cmp(&(b.span.start, b.span.end, b.id.as_str()))
    });
    ordered.dedup_by(|a, b| a.id == b.id && a.span == b.span);
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recs() -> Vec<IncompleteSurface> {
        // Deliberately out of order, with an exact duplicate of the first.
        vec![
            IncompleteSurface::new("b/x/self", Span::new(10, 12), "later position"),
            IncompleteSurface::new("a/y/slot", Span::new(0, 3), "earlier position"),
            IncompleteSurface::new("a/y/slot", Span::new(0, 3), "earlier position"),
        ]
    }

    fn render(format: DiagnosticFormat) -> String {
        let source = "aaa\nbbbbbbbbbbbb\n";
        let mut buf = Vec::new();
        render_incomplete_to_writer_with_format(&mut buf, "f.ts", source, &recs(), format).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn rich_is_ordered_and_deduped() {
        let out = render(DiagnosticFormat::Rich);
        // Duplicate suppressed: exactly two headline lines.
        assert_eq!(out.matches("incomplete[").count(), 2);
        // Deterministic order: the earlier span (a/y) prints before the later (b/x).
        let a = out.find("incomplete[a/y/slot]").unwrap();
        let b = out.find("incomplete[b/x/self]").unwrap();
        assert!(a < b, "records must render in span order:\n{out}");
    }

    #[test]
    fn compact_is_ordered_and_deduped() {
        let out = render(DiagnosticFormat::Compact);
        assert_eq!(
            out.lines().count(),
            2,
            "compact = one line each, deduped:\n{out}"
        );
        assert!(out.contains("f.ts(1,1): incomplete[a/y/slot]: earlier position"));
        assert!(out.contains("f.ts(2,7): incomplete[b/x/self]: later position"));
    }

    #[test]
    fn empty_renders_nothing() {
        let mut buf = Vec::new();
        render_incomplete_to_writer(&mut buf, "f.ts", "x", &[]).unwrap();
        assert!(buf.is_empty());
    }
}

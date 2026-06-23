//! Span handling: a byte-offset range and offset → 1-based line/column mapping
//! over the source (mvp-plan §3).
//!
//! oxc spans are zero-based byte offsets (`oxc_span::Span { start, end: u32 }`).
//! We keep our own tiny `Span` so the rest of the checker doesn't depend on oxc
//! span details, and so the harness can map a diagnostic's primary-span start to
//! a 1-based line number (the marker convention in `tests/cases/README.md`).

/// A half-open byte range `[start, end)` into the source text.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Span { start, end }
    }

    /// Convert from an oxc span.
    pub fn from_oxc(span: oxc_span::Span) -> Self {
        Span {
            start: span.start,
            end: span.end,
        }
    }

    /// The byte range as a `usize` range, for slicing the source.
    pub fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// A 1-based line/column position (column counted in UTF-8 bytes from the line
/// start, +1). 1-based to match editor/`tsc` conventions and the harness.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct LineCol {
    pub line: u32,
    pub column: u32,
}

/// Maps byte offsets to 1-based line/column over one source string. Precomputes
/// line-start offsets so each lookup is a binary search.
pub struct LineIndex {
    /// Byte offset of the start of each line. `line_starts[0] == 0`.
    line_starts: Vec<u32>,
    len: u32,
}

impl LineIndex {
    /// Build the index for `source`.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                // The next line starts just after the newline.
                line_starts.push((i + 1) as u32);
            }
        }
        LineIndex {
            line_starts,
            len: source.len() as u32,
        }
    }

    /// 1-based line/column for a byte `offset`. Offsets past the end clamp to the
    /// end of input (defensive; never panics).
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = offset.min(self.len);
        // Largest line-start <= offset. `partition_point` returns the count of
        // starts that are <= offset, which is the 1-based line number directly.
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset);
        let line = line.max(1);
        let line_start = self.line_starts[line - 1];
        LineCol {
            line: line as u32,
            column: offset - line_start + 1,
        }
    }

    /// 1-based line number for a byte offset — the value the conformance harness
    /// keys markers on.
    pub fn line_of(&self, offset: u32) -> u32 {
        self.line_col(offset).line
    }
}

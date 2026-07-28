//! Span handling: byte-offset ranges and 1-based line/column mapping.
//!
//! The checker uses its own `Span` wrapper around oxc byte offsets so diagnostics
//! can map primary spans to the harness's 1-based marker lines.

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
        let line = self.line_starts.partition_point(|&start| start <= offset);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Columns are 1-based UTF-8 **byte** offsets from the line start. Two offsets
    /// on the same line map to the same line but distinct columns — the property
    /// that lets diagnostics sharing a line be told apart.
    #[test]
    fn same_line_offsets_distinct_columns() {
        let idx = LineIndex::new("ab = cd;");
        assert_eq!(idx.line_col(0), LineCol { line: 1, column: 1 });
        assert_eq!(idx.line_col(5), LineCol { line: 1, column: 6 });
    }

    /// A span's start and end map independently; the end column reflects the
    /// half-open range's byte length.
    #[test]
    fn span_start_and_end_columns() {
        // `"hi"` occupies bytes 8..12 in this line.
        let src = "let s = \"hi\";";
        let idx = LineIndex::new(src);
        assert_eq!(idx.line_col(8), LineCol { line: 1, column: 9 });
        assert_eq!(
            idx.line_col(12),
            LineCol {
                line: 1,
                column: 13
            }
        );
    }

    #[test]
    fn multiline_resets_column_per_line() {
        // "ab\ncde\nf": line starts at bytes 0, 3, 7.
        let idx = LineIndex::new("ab\ncde\nf");
        assert_eq!(idx.line_col(3), LineCol { line: 2, column: 1 }); // 'c'
        assert_eq!(idx.line_col(5), LineCol { line: 2, column: 3 }); // 'e'
        assert_eq!(idx.line_col(7), LineCol { line: 3, column: 1 }); // 'f'
    }

    /// A tab is a single byte, so it advances the byte column by one.
    #[test]
    fn tabs_count_as_one_byte_column() {
        let idx = LineIndex::new("\t\tx");
        assert_eq!(idx.line_col(2), LineCol { line: 1, column: 3 }); // 'x'
    }

    /// A 2-byte character (`é` = U+00E9) shifts the byte column by two.
    #[test]
    fn multibyte_char_advances_column_by_byte_length() {
        // bytes: [C3 A9] '=' 'x'  -> '=' at byte 2, 'x' at byte 3.
        let idx = LineIndex::new("é=x");
        assert_eq!(idx.line_col(2), LineCol { line: 1, column: 3 }); // '='
        assert_eq!(idx.line_col(3), LineCol { line: 1, column: 4 }); // 'x'
    }

    /// Wide characters are counted in bytes: a 3-byte CJK char and a 4-byte emoji.
    #[test]
    fn wide_chars_count_in_bytes() {
        // '中' = 3 bytes -> 'x' at byte 3 -> column 4.
        let idx = LineIndex::new("中x");
        assert_eq!(idx.line_col(3), LineCol { line: 1, column: 4 });
        // '🎉' = 4 bytes -> 'x' at byte 4 -> column 5.
        let idx = LineIndex::new("🎉x");
        assert_eq!(idx.line_col(4), LineCol { line: 1, column: 5 });
    }

    /// A multibyte char followed by a newline still starts the next line at column 1.
    #[test]
    fn multibyte_then_newline() {
        // "é\nx": 'é' bytes 0..2, '\n' byte 2, 'x' byte 3.
        let idx = LineIndex::new("é\nx");
        assert_eq!(idx.line_col(3), LineCol { line: 2, column: 1 });
    }

    /// An offset at end-of-input (== len) — an EOF span — resolves to the position
    /// just past the last character, and offsets beyond the end clamp there.
    #[test]
    fn eof_span_and_out_of_range_clamp() {
        let idx = LineIndex::new("abc");
        assert_eq!(idx.line_col(3), LineCol { line: 1, column: 4 }); // exactly EOF
        assert_eq!(idx.line_col(100), LineCol { line: 1, column: 4 }); // clamped
    }

    /// An EOF offset on a later line reports that line, not line 1.
    #[test]
    fn eof_after_trailing_content_reports_last_line() {
        let src = "a\nbc";
        let idx = LineIndex::new(src);
        assert_eq!(
            idx.line_col(src.len() as u32),
            LineCol { line: 2, column: 3 }
        );
    }
}

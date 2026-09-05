//! The library's own records, named and materialized only when asked (ADR-0018, ADR-0029).
//!
//! Compiling the pinned profile produces diagnostics and incomplete surfaces that belong to
//! the library, not to any user file: they are typokat's own model gaps against a library the
//! real `tsc` checks clean. No process retains them, and the project-local session behind this
//! census drops its semantic product. The census exists so a checker
//! change that adds or removes one of those outcomes shows up as a named `(code, site)`
//! difference against the committed pin instead of as a moved count.

use super::profile::ExactLibraryProfile;
use super::provider::{LibraryBaseProvider, LibraryInitCause, LibraryInitError, LibraryInitStage};
use crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest;
use crate::check::checker::project_local::{
    project_local_semantic_candidate_for_test, ProjectLocalOrderForTest,
    ProjectLocalSemanticRequestForTest, ProjectSourceReexportPolicy,
};
#[cfg(test)]
use crate::check::checker::reporting_record::CheckerRecord;
#[cfg(test)]
use crate::source::LibraryFileOrdinal;
use crate::span::LineIndex;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// The channel a library-owned record came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LibraryRecordKind {
    /// A diagnostic the checker reported against a library declaration.
    Diagnostic,
    /// An in-scope position the checker skipped in a library declaration.
    Incomplete,
}

impl LibraryRecordKind {
    /// The stable tag used in the rendered census.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Diagnostic => "diagnostic",
            Self::Incomplete => "incomplete",
        }
    }
}

/// One library-owned outcome: what it is, and where it is.
///
/// Field order is the census sort order — site first, so the pin reads as a walk through the
/// profile, and `line`/`column` sort numerically rather than as text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryRecordEntry {
    file: String,
    line: u32,
    column: u32,
    kind: LibraryRecordKind,
    name: String,
    detail: String,
}

impl LibraryRecordEntry {
    /// The library file the record was reported in, e.g. `lib.dom.d.ts`.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// 1-based line of the record's primary span.
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// 1-based column (UTF-8 bytes) of the record's primary span.
    pub const fn column(&self) -> u32 {
        self.column
    }

    /// Diagnostic or incomplete surface.
    pub const fn kind(&self) -> LibraryRecordKind {
        self.kind
    }

    /// The name: a `TK` code for a diagnostic, the surface id for an incomplete.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The message (diagnostic) or context (incomplete) carried with the record.
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// `file:line:column`.
    pub fn site(&self) -> String {
        format!("{}:{}:{}", self.file, self.line, self.column)
    }
}

/// One tab-separated census line: kind, name, site, detail.
impl fmt::Display for LibraryRecordEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}\t{}\t{}\t{}",
            self.kind.tag(),
            escape(&self.name),
            self.site(),
            escape(&self.detail)
        )
    }
}

/// Every record the pinned profile produces, as a named multiset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryRecordCensus {
    profile_identity: String,
    entries: Vec<LibraryRecordEntry>,
}

impl LibraryRecordCensus {
    /// Publish the packaged 82-file profile and collect the records it reports against itself.
    ///
    /// The deliberate inspection entry point uses one project-local force-all session. Nothing
    /// calls it on an ordinary checking path, and the semantic product is dropped as this returns.
    pub fn compile_packaged_profile() -> Result<Self, LibraryInitError> {
        let profile = ExactLibraryProfile::load_packaged()
            .map_err(|error| profile_failed(error.to_string()))?;
        let product = LibraryBaseProvider::new()
            .get()
            .map_err(|error| error.as_ref().clone())?;
        Self::compile_product(&profile, product)
    }

    /// [`compile_packaged_profile`](Self::compile_packaged_profile) for an already-loaded profile.
    pub fn compile(profile: &ExactLibraryProfile) -> Result<Self, LibraryInitError> {
        let product = Arc::new(
            super::declaration_product::build_declaration_product(profile)
                .map_err(super::provider::declaration_product_failed)?,
        );
        Self::compile_product(profile, product)
    }

    fn compile_product(
        profile: &ExactLibraryProfile,
        product: Arc<typokat_library_product::LibraryDeclarationProduct>,
    ) -> Result<Self, LibraryInitError> {
        let product_for_run = Arc::clone(&product);
        let run = crate::frontend::run_prepared_project_frontend(
            vec![crate::frontend::FileInput {
                name: "/__typokat_library_record_census__.ts".to_owned(),
                source: String::new(),
            }],
            move |programs, source_reexports, default_modules| {
                let (library_units, bound) =
                    crate::check::checker::bind_library_product_project_programs(
                        &product_for_run,
                        programs,
                        ProjectSourceReexportPolicy::Admit(source_reexports),
                        default_modules,
                    )?;
                Ok::<_, crate::check::checker::LibraryProductProjectBindingError>(
                    project_local_semantic_candidate_for_test(
                        &product_for_run,
                        library_units,
                        bound,
                        programs,
                        ProjectLocalSemanticRequestForTest {
                            case_id: "library-record-census",
                            source_order: ProjectLocalOrderForTest::Forward,
                            demand_order: ProjectLocalOrderForTest::Forward,
                            demand_roots: Vec::new(),
                            force_all: true,
                            fault: None,
                        },
                    ),
                )
            },
        );
        let trace = run
            .product
            .ok_or_else(|| {
                compilation_failed(run.preparation_error.map_or_else(
                    || format!("benign census project parse failed: {:?}", run.parse_errors),
                    |error| error.to_string(),
                ))
            })?
            .map_err(|error| compilation_failed(error.to_string()))?
            .map_err(|error| compilation_failed(error.to_string()))?;
        let records = trace
            .force_all
            .ok_or_else(|| {
                compilation_failed(
                    "project-local census did not return force-all output".to_owned(),
                )
            })?
            .library_records;

        let mut sites = BTreeMap::new();
        for source in profile.sources() {
            let text = std::str::from_utf8(source.bytes()).map_err(|_| {
                profile_failed(format!(
                    "library source ordinal {} ({:?}) is not UTF-8",
                    source.ordinal().index(),
                    source.name()
                ))
            })?;
            sites.insert(source.name(), LineIndex::new(text));
        }

        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            let lines = sites.get(record.source_path.as_str()).ok_or_else(|| {
                compilation_failed(format!(
                    "project-local record names source {:?} outside the profile",
                    record.source_path
                ))
            })?;
            let (kind, span_start, name, detail) = match record.record {
                NormalizedCheckerRecordForTest::Diagnostic(diagnostic) => (
                    LibraryRecordKind::Diagnostic,
                    diagnostic.span.start,
                    diagnostic.code,
                    diagnostic.message,
                ),
                NormalizedCheckerRecordForTest::Incomplete(incomplete) => (
                    LibraryRecordKind::Incomplete,
                    incomplete.span.start,
                    incomplete.id,
                    incomplete.context,
                ),
            };
            let position = lines.line_col(span_start);
            entries.push(LibraryRecordEntry {
                file: record.source_path,
                line: position.line,
                column: position.column,
                kind,
                name,
                detail,
            });
        }
        entries.sort();
        Ok(Self {
            profile_identity: profile.profile_identity().to_owned(),
            entries,
        })
    }

    /// The sorted entries.
    pub fn entries(&self) -> &[LibraryRecordEntry] {
        &self.entries
    }

    /// How many entries are diagnostics.
    pub fn diagnostics(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.kind == LibraryRecordKind::Diagnostic)
            .count()
    }

    /// How many entries are incomplete surfaces.
    pub fn incompletes(&self) -> usize {
        self.entries.len() - self.diagnostics()
    }

    /// The census as committed pin text: a comment header, then one line per record.
    pub fn render(&self) -> String {
        let mut rendered = String::new();
        rendered.push_str("# typokat production-owned library record census (ADR-0028).\n");
        rendered.push_str(
            "# Regenerate: TYPOKAT_BLESS_LIBRARY_RECORDS=1 cargo test --test library_owned_records\n",
        );
        rendered.push_str("# Columns are tab-separated: kind, name, file:line:column, detail.\n");
        rendered.push_str(&format!("# profile: {}\n", self.profile_identity));
        rendered.push_str(&format!(
            "# diagnostics: {}, incompletes: {}, records: {}\n",
            self.diagnostics(),
            self.incompletes(),
            self.entries.len()
        ));
        for entry in &self.entries {
            rendered.push_str(&entry.to_string());
            rendered.push('\n');
        }
        rendered
    }

    /// The named multiset difference against pin text, ignoring comment and blank lines.
    ///
    /// This is the whole point of the census: a checker change that drops or adds a
    /// library-owned outcome names the entry it moved, instead of moving a count.
    pub fn difference_from(&self, pinned: &str) -> LibraryRecordCensusDifference {
        let mut counts = BTreeMap::<String, i64>::new();
        for line in census_lines(pinned) {
            *counts.entry(line.to_owned()).or_default() -= 1;
        }
        for entry in &self.entries {
            *counts.entry(entry.to_string()).or_default() += 1;
        }
        let mut added = Vec::new();
        let mut removed = Vec::new();
        for (line, count) in counts {
            for _ in 0..count.max(0) {
                added.push(line.clone());
            }
            for _ in 0..(-count).max(0) {
                removed.push(line.clone());
            }
        }
        LibraryRecordCensusDifference { added, removed }
    }
}

/// What the live census has that the pin does not, and the reverse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryRecordCensusDifference {
    added: Vec<String>,
    removed: Vec<String>,
}

impl LibraryRecordCensusDifference {
    /// Census lines the checker now produces that the pin does not carry.
    pub fn added(&self) -> &[String] {
        &self.added
    }

    /// Census lines the pin carries that the checker no longer produces — the direction that
    /// hides a dropped library-owned outcome.
    pub fn removed(&self) -> &[String] {
        &self.removed
    }

    /// Whether the live census and the pin hold the same multiset.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

impl fmt::Display for LibraryRecordCensusDifference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{} added, {} removed",
            self.added.len(),
            self.removed.len()
        )?;
        for line in &self.removed {
            writeln!(formatter, "  - {line}")?;
        }
        for line in &self.added {
            writeln!(formatter, "  + {line}")?;
        }
        Ok(())
    }
}

/// The census lines of pin text: comments and blank lines are not part of the multiset, so a
/// count in the header never masquerades as a record.
fn census_lines(pinned: &str) -> impl Iterator<Item = &str> {
    pinned
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

fn profile_failed(message: String) -> LibraryInitError {
    LibraryInitError::new(
        LibraryInitStage::ProfileLoad,
        LibraryInitCause::ProfileRejected { message },
    )
}

fn compilation_failed(message: String) -> LibraryInitError {
    LibraryInitError::new(
        LibraryInitStage::Compile,
        LibraryInitCause::CompilationFailed { message },
    )
}

#[cfg(test)]
fn entry(
    sites: &BTreeMap<LibraryFileOrdinal, (&str, LineIndex)>,
    file_ordinal: LibraryFileOrdinal,
    record: &CheckerRecord,
) -> Result<LibraryRecordEntry, LibraryInitError> {
    let (file, lines) = sites.get(&file_ordinal).ok_or_else(|| {
        compilation_failed(format!(
            "library record names file ordinal {} outside the profile",
            file_ordinal.index()
        ))
    })?;
    let (kind, span_start, name, detail) = match record {
        CheckerRecord::Diagnostic(diagnostic) => (
            LibraryRecordKind::Diagnostic,
            diagnostic.span.start,
            diagnostic.code.as_str().to_owned(),
            diagnostic.message.clone(),
        ),
        CheckerRecord::Incomplete(incomplete) => (
            LibraryRecordKind::Incomplete,
            incomplete.span.start,
            incomplete.id.clone(),
            incomplete.context.clone(),
        ),
    };
    let position = lines.line_col(span_start);
    Ok(LibraryRecordEntry {
        file: (*file).to_owned(),
        line: position.line,
        column: position.column,
        kind,
        name,
        detail,
    })
}

/// Keep one record on one line: tabs separate columns and newlines separate records.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{Diagnostic, IncompleteSurface};
    use crate::span::Span;

    fn sites(source: &'static str) -> BTreeMap<LibraryFileOrdinal, (&'static str, LineIndex)> {
        BTreeMap::from([(
            LibraryFileOrdinal::new(0),
            ("lib.probe.d.ts", LineIndex::new(source)),
        )])
    }

    fn census(entries: Vec<LibraryRecordEntry>) -> LibraryRecordCensus {
        LibraryRecordCensus {
            profile_identity: "probe".to_owned(),
            entries,
        }
    }

    fn diagnostic_entry(source: &'static str, start: u32, name: &str) -> LibraryRecordEntry {
        entry(
            &sites(source),
            LibraryFileOrdinal::new(0),
            &CheckerRecord::Diagnostic(Diagnostic::cannot_find_name(
                Span::new(start, start + 1),
                name,
            )),
        )
        .expect("record inside the profile")
    }

    #[test]
    fn an_entry_names_its_code_and_its_site() {
        let entry = diagnostic_entry("declare const a: A;\ndeclare const b: B;\n", 37, "B");
        assert_eq!(entry.kind(), LibraryRecordKind::Diagnostic);
        assert_eq!(entry.name(), "TK2304");
        assert_eq!(entry.site(), "lib.probe.d.ts:2:18");
        assert_eq!(
            entry.to_string(),
            "diagnostic\tTK2304\tlib.probe.d.ts:2:18\tCannot find name 'B'"
        );
    }

    #[test]
    fn an_incomplete_entry_names_its_surface_id() {
        let record = entry(
            &sites("type T = A.B;\n"),
            LibraryFileOrdinal::new(0),
            &CheckerRecord::Incomplete(IncompleteSurface::new(
                "annotation-lower/type-name/qualified-name",
                Span::new(9, 12),
                "qualified type path classified",
            )),
        )
        .expect("record inside the profile");
        assert_eq!(
            record.to_string(),
            concat!(
                "incomplete\tannotation-lower/type-name/qualified-name\t",
                "lib.probe.d.ts:1:10\tqualified type path classified"
            )
        );
    }

    #[test]
    fn a_record_outside_the_profile_is_a_typed_failure() {
        let outside = entry(
            &sites("declare const a: A;\n"),
            LibraryFileOrdinal::new(7),
            &CheckerRecord::Diagnostic(Diagnostic::cannot_find_name(Span::new(17, 18), "A")),
        );
        assert!(outside
            .expect_err("record outside profile")
            .to_string()
            .contains("file ordinal 7"));
    }

    #[test]
    fn tabs_and_newlines_in_a_message_stay_on_one_line() {
        assert_eq!(escape("a\tb\nc\\d\r"), "a\\tb\\nc\\\\d\\r");
    }

    #[test]
    fn the_difference_names_what_moved_rather_than_counting_it() {
        let source = "declare const a: A;\ndeclare const b: B;\ndeclare const c: C;\n";
        let census = census(vec![
            diagnostic_entry(source, 17, "A"),
            diagnostic_entry(source, 37, "B"),
        ]);
        let pinned = format!(
            "# diagnostics: 2, incompletes: 0, records: 2\n{}\n{}\n",
            diagnostic_entry(source, 17, "A"),
            diagnostic_entry(source, 57, "C"),
        );

        let difference = census.difference_from(&pinned);
        assert!(!difference.is_empty());
        assert_eq!(
            difference.added(),
            ["diagnostic\tTK2304\tlib.probe.d.ts:2:18\tCannot find name 'B'"]
        );
        assert_eq!(
            difference.removed(),
            ["diagnostic\tTK2304\tlib.probe.d.ts:3:18\tCannot find name 'C'"]
        );
        assert!(census.difference_from(&census.render()).is_empty());
    }

    #[test]
    fn one_record_of_two_identical_ones_moving_is_still_named() {
        let source = "declare const a: A;\n";
        let twice = census(vec![
            diagnostic_entry(source, 17, "A"),
            diagnostic_entry(source, 17, "A"),
        ]);
        let once = census(vec![diagnostic_entry(source, 17, "A")]);

        let dropped = once.difference_from(&twice.render());
        assert!(dropped.added().is_empty());
        assert_eq!(dropped.removed().len(), 1);
        assert!(dropped.removed()[0].contains("Cannot find name 'A'"));

        let gained = twice.difference_from(&once.render());
        assert!(gained.removed().is_empty());
        assert_eq!(gained.added().len(), 1);
    }

    #[test]
    fn the_rendered_header_counts_both_channels() {
        let source = "declare const a: A;\n";
        let rendered = census(vec![diagnostic_entry(source, 17, "A")]).render();
        assert!(rendered.contains("# diagnostics: 1, incompletes: 0, records: 1\n"));
        assert!(rendered.contains("# profile: probe\n"));
    }

    #[test]
    fn packaged_census_uses_the_cached_declaration_product() {
        let census = LibraryRecordCensus::compile_packaged_profile().expect("packaged census");

        assert_eq!(census.entries().len(), 703);
        assert_eq!(census.diagnostics(), 318);
        assert_eq!(census.incompletes(), 385);
    }
}

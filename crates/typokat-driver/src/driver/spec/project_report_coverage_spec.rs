//! RED coverage contract for translating checker results into user reports.

use super::*;
use typokat_core::source::{ModuleOrdinal, UnitSlot};

fn input(name: &str) -> FileInput {
    FileInput {
        name: name.to_owned(),
        source: "export {};\n".to_owned(),
    }
}

fn result(original: usize, slot: usize) -> CheckResult {
    CheckResult {
        module_ordinal: ModuleOrdinal::new(original),
        unit_slot: UnitSlot::new(slot),
        diagnostics: Vec::new(),
        incomplete: Vec::new(),
    }
}

fn result_with_payload(original: usize, slot: usize, label: &str) -> CheckResult {
    CheckResult {
        module_ordinal: ModuleOrdinal::new(original),
        unit_slot: UnitSlot::new(slot),
        diagnostics: vec![Diagnostic::cannot_find_name(Span::new(0, 1), label)],
        incomplete: vec![IncompleteSurface::new(
            format!("report-coverage/{label}"),
            Span::new(1, 2),
            format!("incomplete {label}"),
        )],
    }
}

fn translate(
    project_units_by_slot: Vec<usize>,
    ordered_results: Vec<CheckResult>,
) -> Result<Vec<FileReport>, &'static str> {
    project_reports_from_frontend_run(ProjectFrontendRun {
        inputs: vec![input("a.ts"), input("b.ts")],
        parse_errors: vec![Vec::new(), Vec::new()],
        product: (project_units_by_slot, Ok(ordered_results)),
    })
}

fn assert_translation_error(
    project_units_by_slot: Vec<usize>,
    ordered_results: Vec<CheckResult>,
    expected: &'static str,
) {
    assert_eq!(
        translate(project_units_by_slot, ordered_results).err(),
        Some(expected)
    );
}

#[test]
fn missing_checker_result_fails_closed() {
    assert_translation_error(
        vec![0, 1],
        vec![result(0, 0)],
        "checker result count does not match the input count",
    );
}

#[test]
fn duplicate_checker_result_fails_closed_at_the_misindexed_boundary() {
    assert_translation_error(
        vec![0, 1],
        vec![result(0, 0), result(0, 1)],
        "checker result does not match its project unit",
    );
}

#[test]
fn out_of_range_checker_result_fails_closed() {
    assert_translation_error(
        vec![0, 1],
        vec![result(2, 0), result(1, 1)],
        "checker result references an out-of-range input",
    );
}

#[test]
fn misindexed_checker_result_fails_closed() {
    assert_translation_error(
        vec![0, 1],
        vec![result(1, 0), result(0, 1)],
        "checker result does not match its project unit",
    );
}

#[test]
fn out_of_order_checker_result_fails_closed() {
    assert_translation_error(
        vec![0, 1],
        vec![result(0, 1), result(1, 0)],
        "checker results are not in dependency order",
    );
}

#[test]
fn complete_dependency_ordered_coverage_remains_valid() {
    let reports = translate(
        vec![1, 0],
        vec![
            result_with_payload(1, 0, "for-b"),
            result_with_payload(0, 1, "for-a"),
        ],
    )
    .expect("complete result coverage");
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].name, "a.ts");
    assert_eq!(reports[1].name, "b.ts");
    assert_eq!(reports[0].output.diagnostics.len(), 1);
    assert!(reports[0].output.diagnostics[0].message.contains("for-a"));
    assert_eq!(reports[0].output.incomplete[0].id, "report-coverage/for-a");
    assert_eq!(reports[1].output.diagnostics.len(), 1);
    assert!(reports[1].output.diagnostics[0].message.contains("for-b"));
    assert_eq!(reports[1].output.incomplete[0].id, "report-coverage/for-b");
    assert!(reports
        .iter()
        .all(|report| report.output.parse_errors.is_empty()));
}

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

#[test]
fn missing_checker_result_fails_closed() {
    let translated = translate(vec![0], vec![result(0, 0)]);
    assert!(
        translated.is_err(),
        "a missing result must not synthesize a clean report"
    );
}

#[test]
fn duplicate_checker_result_fails_closed() {
    let translated = translate(vec![0, 0], vec![result(0, 0), result(0, 1)]);
    assert!(
        translated.is_err(),
        "a duplicate result must not overwrite one slot and leave another clean"
    );
}

#[test]
fn out_of_range_checker_result_fails_closed() {
    let translated = translate(vec![2], vec![result(2, 0)]);
    assert!(
        translated.is_err(),
        "an out-of-range result must not be discarded as a clean project"
    );
}

#[test]
fn complete_dependency_ordered_coverage_remains_valid() {
    let reports =
        translate(vec![1, 0], vec![result(1, 0), result(0, 1)]).expect("complete result coverage");
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].name, "a.ts");
    assert_eq!(reports[1].name, "b.ts");
    assert!(reports.iter().all(|report| {
        report.output.diagnostics.is_empty()
            && report.output.parse_errors.is_empty()
            && report.output.incomplete.is_empty()
    }));
}

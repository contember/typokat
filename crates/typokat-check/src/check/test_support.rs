//! Checker-owned parse/check helpers for checker unit specifications.

use super::{check_program, check_project_programs, CheckResult};
use crate::diagnostics::{Diagnostic, IncompleteSurface};
use crate::frontend::{run_project_frontend, run_source_frontend, FileInput, ProjectProgram};
use crate::types::Interner;

pub struct CheckOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub parse_errors: Vec<String>,
    pub incomplete: Vec<IncompleteSurface>,
}

pub struct FileReport {
    pub name: String,
    pub source: String,
    pub output: CheckOutput,
}

pub fn check_source(source: &str) -> CheckOutput {
    let run = run_source_frontend(source, |program| {
        let mut interner = Interner::with_intrinsics();
        check_program(&mut interner, program)
    });
    match run.product {
        Some(CheckResult {
            diagnostics,
            incomplete,
            ..
        }) => CheckOutput {
            diagnostics,
            parse_errors: run.parse_errors,
            incomplete,
        },
        None => CheckOutput {
            diagnostics: Vec::new(),
            parse_errors: run.parse_errors,
            incomplete: Vec::new(),
        },
    }
}

pub fn check_project(inputs: Vec<FileInput>) -> Vec<FileReport> {
    check_project_with_checker(inputs, |interner, units| {
        check_project_programs(interner, units)
    })
}

pub fn check_project_with_checker<F>(inputs: Vec<FileInput>, check_project: F) -> Vec<FileReport>
where
    F: for<'ast> FnOnce(&mut Interner, &[ProjectProgram<'ast>]) -> Vec<CheckResult>,
{
    if inputs.is_empty() {
        return Vec::new();
    }
    let run = run_project_frontend(inputs, |interner, units| {
        let module_ordinals = units
            .iter()
            .map(|unit| unit.module_ordinal)
            .collect::<Vec<_>>();
        (module_ordinals, check_project(interner, units))
    });
    reports_from_run(run.inputs, run.parse_errors, run.product)
}

pub fn check_project_with_owned_checker_for_test<F>(
    inputs: Vec<FileInput>,
    check_project: F,
) -> Vec<FileReport>
where
    F: for<'ast> FnOnce(&[ProjectProgram<'ast>]) -> Vec<CheckResult>,
{
    check_project_with_checker(inputs, |_, units| {
        super::checker::library_compiler::record_user_source_parses_for_test(units.len());
        check_project(units)
    })
}

fn reports_from_run(
    inputs: Vec<FileInput>,
    parse_errors: Vec<Vec<String>>,
    product: (Vec<crate::source::ModuleOrdinal>, Vec<CheckResult>),
) -> Vec<FileReport> {
    let (project_units_by_slot, ordered_results) = product;
    let mut diagnostics_by_original: Vec<Vec<Diagnostic>> =
        (0..inputs.len()).map(|_| Vec::new()).collect();
    let mut incomplete_by_original: Vec<Vec<IncompleteSurface>> =
        (0..inputs.len()).map(|_| Vec::new()).collect();
    for result in ordered_results {
        let original = result.module_ordinal.index();
        debug_assert_eq!(
            project_units_by_slot.get(result.unit_slot.index()).copied(),
            Some(result.module_ordinal)
        );
        if let Some(slot) = diagnostics_by_original.get_mut(original) {
            *slot = result.diagnostics;
        }
        if let Some(slot) = incomplete_by_original.get_mut(original) {
            *slot = result.incomplete;
        }
    }

    inputs
        .into_iter()
        .enumerate()
        .map(|(index, input)| FileReport {
            name: input.name,
            source: input.source,
            output: CheckOutput {
                diagnostics: diagnostics_by_original
                    .get_mut(index)
                    .map(std::mem::take)
                    .unwrap_or_default(),
                parse_errors: parse_errors.get(index).cloned().unwrap_or_default(),
                incomplete: incomplete_by_original
                    .get_mut(index)
                    .map(std::mem::take)
                    .unwrap_or_default(),
            },
        })
        .collect()
}

//! RED contract for per-module declaration-owner attachment over a growing project.

use super::{check_project_programs_with_binding_inspector, DeclarationOwnerScanScopeForTest};
use crate::driver::{run_project_frontend, FileInput};
use crate::types::layered::LocalFullViewScanScopeForTest;
use std::cell::Cell;

const SMALL_MODULES: usize = 8;
const LARGE_MODULES: usize = 16;
const DECLARATION_BLOCKS_PER_MODULE: usize = 4;

/// Owner attachment visits each declaration once, so the whole project may be walked
/// about once in total — never once per module.
const OWNER_SCAN_PASSES: u64 = 2;
/// Doubling the modules doubles the project's declarations; the scan must not outpace that.
const GROWTH_MULTIPLE: u64 = 2;
const GROWTH_SLACK: u64 = 256;

struct MeasuredRun {
    modules: usize,
    /// Every declaration in the project — the collection a per-module pass may not re-scan.
    declarations: u64,
    /// Declaration rows exposed to `attach_type_decl_owners`.
    owner_scan_rows: u64,
    /// Every local-layer row exposed anywhere in the run, for context on the failure.
    project_scan_rows: u64,
    diagnostics: Vec<String>,
    incomplete: Vec<String>,
}

impl MeasuredRun {
    fn record(&self) -> String {
        format!(
            "{} modules / {} declarations: {} rows scanned for owner attachment \
             ({} project-wide)",
            self.modules, self.declarations, self.owner_scan_rows, self.project_scan_rows
        )
    }
}

/// Every module carries the same declarations, so per-module work is constant by construction.
fn project_inputs(modules: usize) -> Vec<FileInput> {
    let mut inputs = Vec::with_capacity(modules + 1);
    inputs.push(FileInput {
        name: "/project/shared.ts".to_owned(),
        source: "export interface Shared { value: string }\n".to_owned(),
    });
    for module in 0..modules {
        let mut source = String::from("import { Shared } from \"./shared\";\n");
        source.push_str(&format!(
            "export const shared{module:03}: Shared = {{ value: \"s{module:03}\" }};\n"
        ));
        for block in 0..DECLARATION_BLOCKS_PER_MODULE {
            source.push_str(&format!(
                "export interface Shape{module:03}_{block:03} {{ value: string; tag: number }}\n\
                 export const value{module:03}_{block:03}: Shape{module:03}_{block:03} = {{ value: \"v\", tag: {block} }};\n\
                 export function read{module:03}_{block:03}(input: Shape{module:03}_{block:03}): number {{ return input.tag; }}\n\
                 export class Holder{module:03}_{block:03} {{ constructor(readonly held: Shape{module:03}_{block:03}) {{}} }}\n"
            ));
        }
        inputs.push(FileInput {
            name: format!("/project/module{module:03}.ts"),
            source,
        });
    }
    inputs
}

fn check_measured(modules: usize) -> MeasuredRun {
    let declarations = Cell::new(0_u64);
    let owner_scan = DeclarationOwnerScanScopeForTest::start();
    let project_scan = LocalFullViewScanScopeForTest::start();
    let results = run_project_frontend(project_inputs(modules), |interner, units| {
        check_project_programs_with_binding_inspector(interner, units, |binder, _, _| {
            declarations.set(u64::try_from(binder.declarations.len()).expect("count fits u64"));
        })
    })
    .into_product();

    MeasuredRun {
        modules,
        declarations: declarations.get(),
        owner_scan_rows: owner_scan.finish(),
        project_scan_rows: project_scan.finish(),
        diagnostics: results
            .iter()
            .flat_map(|result| &result.diagnostics)
            .map(|diagnostic| format!("{} {}", diagnostic.code.as_str(), diagnostic.message))
            .collect(),
        incomplete: results
            .iter()
            .flat_map(|result| &result.incomplete)
            .map(|incomplete| incomplete.id.to_string())
            .collect(),
    }
}

/// `attach_type_decl_owners` filters the whole project's declarations once per module, so owner
/// attachment alone costs Θ(modules × declarations) instead of Θ(declarations).
#[test]
fn per_module_declaration_owner_attachment_does_not_scan_the_whole_project() {
    let small = check_measured(SMALL_MODULES);
    let large = check_measured(LARGE_MODULES);
    for run in [&small, &large] {
        assert!(
            run.diagnostics.is_empty(),
            "{}: {:?}",
            run.record(),
            run.diagnostics
        );
        assert!(
            run.incomplete.is_empty(),
            "{}: {:?}",
            run.record(),
            run.incomplete
        );
    }

    // Premise: the project grows linearly, so a per-declaration cost must too.
    assert!(
        large.declarations <= 2 * small.declarations,
        "the corpus must double, not more: small={}, large={}",
        small.record(),
        large.record()
    );

    let mut violations = Vec::new();
    for run in [&small, &large] {
        let bound = OWNER_SCAN_PASSES * run.declarations;
        if run.owner_scan_rows > bound {
            violations.push(format!(
                "{}: owner attachment scanned {} rows, past the {OWNER_SCAN_PASSES}-pass bound {bound}",
                run.record(),
                run.owner_scan_rows
            ));
        }
    }
    let growth_bound = GROWTH_MULTIPLE * small.owner_scan_rows + GROWTH_SLACK;
    if large.owner_scan_rows > growth_bound {
        violations.push(format!(
            "owner-attachment rows grew from {} to {}, past the linear bound {growth_bound}",
            small.owner_scan_rows, large.owner_scan_rows
        ));
    }

    assert!(
        violations.is_empty(),
        "declaration-owner attachment scans the whole project per module:\n{}\nsmall={}\nlarge={}",
        violations.join("\n"),
        small.record(),
        large.record()
    );
}

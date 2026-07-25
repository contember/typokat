//! RED contract for per-module declaration-owner attachment over a growing project.

use super::library_compiler::{compile_owned_injected_profile, InjectedLibrarySource};
use super::{
    check_project_programs_with_binding_inspector, check_project_programs_with_owned_library,
    DeclarationOwnerScanScopeForTest,
};
use crate::driver::{check_project_with_owned_checker_for_test, run_project_frontend, FileInput};
use crate::source::LibraryFileOrdinal;
use crate::types::layered::LocalFullViewScanScopeForTest;
use std::cell::{Cell, RefCell};

const SMALL_MODULES: usize = 8;
const LARGE_MODULES: usize = 16;
const DECLARATION_BLOCKS_PER_MODULE: usize = 4;

/// Owner attachment visits each declaration once, so the whole project may be walked
/// about once in total — never once per module.
const OWNER_SCAN_PASSES: u64 = 2;
/// Doubling the modules doubles the project's declarations; the scan must not outpace that.
const GROWTH_MULTIPLE: u64 = 2;
const GROWTH_SLACK: u64 = 256;

const LIBRARY_SOURCE: &str = "interface FrozenBase { frozen: string }\n";

struct MeasuredRun {
    route: &'static str,
    modules: usize,
    /// Every declaration in the delta — the collection a per-module pass may not re-scan.
    declarations: u64,
    /// Declarations owner attachment must cover: those in an attached module scope.
    attachable: u64,
    /// Declaration rows visited by `attach_type_decl_owners`.
    owner_scan_rows: u64,
    /// Every local-layer row exposed anywhere in the run, for context on the failure.
    project_scan_rows: u64,
    diagnostics: Vec<String>,
    incomplete: Vec<String>,
}

impl MeasuredRun {
    fn record(&self) -> String {
        format!(
            "{} route, {} modules / {} declarations ({} attachable): {} rows visited for \
             owner attachment ({} project-wide)",
            self.route,
            self.modules,
            self.declarations,
            self.attachable,
            self.owner_scan_rows,
            self.project_scan_rows
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

/// The delta's declaration count, and how many of them sit in an attached module scope.
fn count_declarations(
    binder: &crate::binder::Binder,
    module_scopes: &[crate::binder::scope::ScopeId],
) -> (u64, u64) {
    let scopes = module_scopes
        .iter()
        .map(|scope| scope.0)
        .collect::<std::collections::BTreeSet<_>>();
    let mut declarations = 0_u64;
    let mut attachable = 0_u64;
    for declaration in binder.declarations.local_declarations() {
        declarations += 1;
        if scopes.contains(&declaration.site.module.0) {
            attachable += 1;
        }
    }
    (declarations, attachable)
}

fn measured(
    route: &'static str,
    modules: usize,
    counts: (u64, u64),
    owner_scan_rows: u64,
    project_scan_rows: u64,
    results: &[crate::check::CheckResult],
) -> MeasuredRun {
    MeasuredRun {
        route,
        modules,
        declarations: counts.0,
        attachable: counts.1,
        owner_scan_rows,
        project_scan_rows,
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

/// The ordinary route: the trusted prelude plus every user module in one delta.
fn check_measured_fresh(modules: usize) -> MeasuredRun {
    let counts = Cell::new((0_u64, 0_u64));
    let owner_scan = DeclarationOwnerScanScopeForTest::start();
    let project_scan = LocalFullViewScanScopeForTest::start();
    let results = run_project_frontend(project_inputs(modules), |interner, units| {
        check_project_programs_with_binding_inspector(
            interner,
            units,
            |binder, _, module_scopes| {
                counts.set(count_declarations(binder, module_scopes));
            },
        )
    })
    .into_product();

    measured(
        "fresh",
        modules,
        counts.get(),
        owner_scan.finish(),
        project_scan.finish(),
        &results,
    )
}

/// The frozen-library route: only the user delta is attached, over a sealed library base.
fn check_measured_continuation(modules: usize) -> MeasuredRun {
    let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "scaling.d.ts",
        source: LIBRARY_SOURCE,
    }])
    .expect("the scaling profile compiles into an owned library base");
    let state = RefCell::new(Some(state));
    let counts = Cell::new((0_u64, 0_u64));
    let owner_scan = DeclarationOwnerScanScopeForTest::start();
    let project_scan = LocalFullViewScanScopeForTest::start();
    let results = check_project_with_owned_checker_for_test(project_inputs(modules), |units| {
        let state = state
            .borrow_mut()
            .take()
            .expect("the owned delta is consumed once");
        check_project_programs_with_owned_library(
            state,
            units,
            |binder, module_scopes| {
                // Only the delta is attachable; the frozen library keeps its compile-time owners.
                counts.set(count_declarations(binder, module_scopes));
            },
            |_, _| {},
        )
    });

    let reports =
        results
            .iter()
            .flat_map(|report| {
                report.output.diagnostics.iter().map(|diagnostic| {
                    format!("{} {}", diagnostic.code.as_str(), diagnostic.message)
                })
            })
            .collect::<Vec<_>>();
    let incomplete = results
        .iter()
        .flat_map(|report| {
            report
                .output
                .incomplete
                .iter()
                .map(|incomplete| incomplete.id.to_string())
        })
        .collect::<Vec<_>>();

    MeasuredRun {
        route: "frozen-library",
        modules,
        declarations: counts.get().0,
        attachable: counts.get().1,
        owner_scan_rows: owner_scan.finish(),
        project_scan_rows: project_scan.finish(),
        diagnostics: reports,
        incomplete,
    }
}

/// `attach_type_decl_owners` must walk its own module's declarations. Filtering the whole
/// project once per module costs Θ(modules × declarations) instead of Θ(declarations).
fn assert_owner_attachment_scales(small: &MeasuredRun, large: &MeasuredRun) {
    for run in [small, large] {
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
    for run in [small, large] {
        // Non-vacuity: every declaration in an attached module is visited, so the count
        // cannot collapse to zero without dropping owners.
        if run.owner_scan_rows < run.attachable {
            violations.push(format!(
                "{}: owner attachment visited fewer rows than the project has attachable declarations",
                run.record()
            ));
        }
        let bound = OWNER_SCAN_PASSES * run.declarations;
        if run.owner_scan_rows > bound {
            violations.push(format!(
                "{}: owner attachment visited {} rows, past the {OWNER_SCAN_PASSES}-pass bound {bound}",
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

#[test]
fn per_module_declaration_owner_attachment_does_not_scan_the_whole_project() {
    assert_owner_attachment_scales(
        &check_measured_fresh(SMALL_MODULES),
        &check_measured_fresh(LARGE_MODULES),
    );
}

#[test]
fn frozen_library_owner_attachment_does_not_scan_the_whole_user_delta() {
    assert_owner_attachment_scales(
        &check_measured_continuation(SMALL_MODULES),
        &check_measured_continuation(LARGE_MODULES),
    );
}

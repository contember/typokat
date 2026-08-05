//! RED acceptance for ADR-0021's standalone complete-source CLI lifecycle.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use typokat::driver::{
    check_project, check_project_once, production_cli_route, CheckOutput, DriverError, FileReport,
};
use typokat::frontend::FileInput;

const BIN: &str = env!("CARGO_BIN_EXE_typokat");
const PROFILE_SHA256: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const COLD_ROUTE: &str = "production-complete-source-once";
const SHARED_ROUTE: &str = "production-default-library";

type ProjectCheck = fn(Vec<FileInput>) -> Result<Vec<FileReport>, DriverError>;
const _: ProjectCheck = check_project_once;
const _: fn() -> &'static str = production_cli_route;

#[derive(Debug, PartialEq, Eq)]
struct OutputShape {
    diagnostics: Vec<String>,
    parse_errors: Vec<String>,
    incomplete: Vec<String>,
}

fn shape(output: &CheckOutput) -> OutputShape {
    OutputShape {
        diagnostics: output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                format!(
                    "{}:{}..{}:{}",
                    diagnostic.code.as_str(),
                    diagnostic.span.start,
                    diagnostic.span.end,
                    diagnostic.message
                )
            })
            .collect(),
        parse_errors: output.parse_errors.clone(),
        incomplete: output
            .incomplete
            .iter()
            .map(|surface| {
                format!(
                    "{}:{}..{}:{}",
                    surface.id, surface.span.start, surface.span.end, surface.context
                )
            })
            .collect(),
    }
}

fn by_name(reports: &[FileReport]) -> BTreeMap<&str, OutputShape> {
    reports
        .iter()
        .map(|report| (report.name.as_str(), shape(&report.output)))
        .collect()
}

fn diagnostic_codes(output: &CheckOutput) -> Vec<&str> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn import_project(reverse: bool) -> Vec<FileInput> {
    let producer = FileInput {
        name: "/wu6/a.ts".to_owned(),
        source: concat!(
            "export const goodNumber: number = 1;\n",
            "export const badString: string = \"s\";\n",
            "export function takesNumber(value: number): number { return value; }\n",
        )
        .to_owned(),
    };
    let consumer = FileInput {
        name: "/wu6/b.ts".to_owned(),
        source: concat!(
            "import { goodNumber, badString, takesNumber } from \"./a\";\n",
            "const ok: number = goodNumber;\n",
            "const bad: number = badString;\n",
            "takesNumber(\"x\");\n",
        )
        .to_owned(),
    };
    if reverse {
        vec![consumer, producer]
    } else {
        vec![producer, consumer]
    }
}

fn collision_project(reverse: bool) -> Vec<FileInput> {
    let augmentation = FileInput {
        name: "/wu6/augment.d.ts".to_owned(),
        source: "interface Array<T> { wu6First(): T; }\n".to_owned(),
    };
    let consumer = FileInput {
        name: "/wu6/use.ts".to_owned(),
        source: concat!(
            "const ok: number = [1, 2, 3].wu6First();\n",
            "const bad: string = [1, 2, 3].wu6First();\n",
        )
        .to_owned(),
    };
    if reverse {
        vec![consumer, augmentation]
    } else {
        vec![augmentation, consumer]
    }
}

fn read_inputs(paths: &[PathBuf], reverse: bool) -> Result<Vec<FileInput>, String> {
    let mut paths = paths.to_vec();
    paths.sort();
    if reverse {
        paths.reverse();
    }
    paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            Ok(FileInput {
                name: path.display().to_string(),
                source,
            })
        })
        .collect()
}

fn ts_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("read {}: {error}", directory.display()))?
        {
            let path = entry
                .map_err(|error| format!("read entry in {}: {error}", directory.display()))?
                .path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("ts") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn compare_routes(inputs: Vec<FileInput>) -> Result<Vec<FileReport>, String> {
    let expected_names = inputs
        .iter()
        .map(|input| input.name.clone())
        .collect::<Vec<_>>();
    let shared_inputs = inputs
        .iter()
        .map(|input| FileInput {
            name: input.name.clone(),
            source: input.source.clone(),
        })
        .collect();
    let shared = check_project(shared_inputs).map_err(|error| error.to_string())?;
    let cold = check_project_once(inputs).map_err(|error| error.to_string())?;
    let cold_names = cold
        .iter()
        .map(|report| report.name.clone())
        .collect::<Vec<_>>();
    if cold_names != expected_names {
        return Err(format!(
            "cold route changed original report order: expected={expected_names:?}, actual={cold_names:?}"
        ));
    }
    if by_name(&cold) != by_name(&shared) {
        return Err(format!(
            "cold/shared semantic mismatch: cold={:?}, shared={:?}",
            by_name(&cold),
            by_name(&shared)
        ));
    }
    Ok(cold)
}

#[test]
fn cold_route_preserves_resolved_imports_and_original_order() -> Result<(), String> {
    for reverse in [false, true] {
        let reports = compare_routes(import_project(reverse))?;
        let consumer = reports
            .iter()
            .find(|report| report.name == "/wu6/b.ts")
            .ok_or_else(|| "cold import project omitted its consumer report".to_owned())?;
        let codes = diagnostic_codes(&consumer.output);
        if codes != ["TK2322", "TK2345"] {
            return Err(format!(
                "resolved imports must report TK2322/TK2345 without false TK2304: {codes:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn cold_route_preserves_missing_module_and_invalid_global_diagnostics() -> Result<(), String> {
    let missing_path = PathBuf::from("tests/cases/m29_modules/missing_resolution/use.ts");
    let missing = compare_routes(read_inputs(&[missing_path], false)?)?;
    let [missing] = missing.as_slice() else {
        return Err("missing-module project must return one report".to_owned());
    };
    if diagnostic_codes(&missing.output) != ["TK2307"] {
        return Err(format!(
            "missing module must fail closed with TK2307: {:?}",
            diagnostic_codes(&missing.output)
        ));
    }

    let invalid = compare_routes(vec![FileInput {
        name: "/wu6/invalid-global.ts".to_owned(),
        source: concat!(
            "declare global {\n",
            "  interface InvalidScriptGlobal { value: number }\n",
            "}\n",
            "declare const value: InvalidScriptGlobal;\n",
        )
        .to_owned(),
    }])?;
    let [invalid] = invalid.as_slice() else {
        return Err("invalid-global project must return one report".to_owned());
    };
    if diagnostic_codes(&invalid.output) != ["TK2669", "TK2304"] {
        return Err(format!(
            "invalid global augmentation must report its binder error and suppress its body: {:?}",
            diagnostic_codes(&invalid.output)
        ));
    }

    let missing_export = compare_routes(vec![
        FileInput {
            name: "/wu6/exporter.ts".to_owned(),
            source: "export const present = 1;\n".to_owned(),
        },
        FileInput {
            name: "/wu6/importer.ts".to_owned(),
            source: "import { absent } from \"./exporter\";\nconst value = absent;\n".to_owned(),
        },
    ])?;
    let importer = missing_export
        .iter()
        .find(|report| report.name == "/wu6/importer.ts")
        .ok_or_else(|| "missing-export project omitted its importer".to_owned())?;
    if diagnostic_codes(&importer.output) != ["TK2305"] {
        return Err(format!(
            "missing export must fail closed with TK2305: {:?}",
            diagnostic_codes(&importer.output)
        ));
    }
    Ok(())
}

#[test]
fn cold_route_preserves_dependency_cycles_and_mixed_result_channels() -> Result<(), String> {
    for reverse in [false, true] {
        let mut cycle = vec![
            FileInput {
                name: "/wu6/cycle-a.ts".to_owned(),
                source: concat!(
                    "import { b } from \"./cycle-b\";\n",
                    "export const a: number = b;\n",
                )
                .to_owned(),
            },
            FileInput {
                name: "/wu6/cycle-b.ts".to_owned(),
                source: concat!(
                    "import { a } from \"./cycle-a\";\n",
                    "export const b: number = a;\n",
                )
                .to_owned(),
            },
        ];
        if reverse {
            cycle.reverse();
        }
        let reports = compare_routes(cycle)?;
        if reports
            .iter()
            .flat_map(|report| diagnostic_codes(&report.output))
            .collect::<Vec<_>>()
            != ["TK2305"]
        {
            return Err(format!(
                "the admitted module slice must preserve its safe cycle refusal: {:?}",
                by_name(&reports)
            ));
        }
    }

    let reports = compare_routes(vec![
        FileInput {
            name: "/wu6/mixed.d.ts".to_owned(),
            source: concat!(
                "export as namespace Mixed;\n",
                "export = Mixed;\n",
                "declare function Mixed(): void;\n",
            )
            .to_owned(),
        },
        FileInput {
            name: "/wu6/diagnostic.ts".to_owned(),
            source: "const wrong: string = 1;\n".to_owned(),
        },
    ])?;
    if reports[0].output.incomplete.len() != 2
        || !reports[0].output.diagnostics.is_empty()
        || diagnostic_codes(&reports[1].output) != ["TK2322"]
        || !reports[1].output.incomplete.is_empty()
    {
        return Err(format!("mixed channels changed: {:?}", by_name(&reports)));
    }
    Ok(())
}

#[test]
fn cold_route_matches_the_complete_b102_b103_matrix_in_both_orders() -> Result<(), String> {
    for flat in [
        "tests/cases/b102_frozen_prefix_writes",
        "tests/cases/b103_library_merge_correctness",
    ] {
        for file in ts_files(Path::new(flat))? {
            compare_routes(read_inputs(&[file], false)?)?;
        }
    }

    for projects in [
        "tests/cases/b102_frozen_prefix_writes_project",
        "tests/cases/b103_library_merge_correctness_project",
    ] {
        let mut directories = fs::read_dir(projects)
            .map_err(|error| format!("read {projects}: {error}"))?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| format!("read entry in {projects}: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        directories.retain(|path| path.is_dir());
        directories.sort();
        for directory in directories {
            let files = ts_files(&directory)?;
            for reverse in [false, true] {
                compare_routes(read_inputs(&files, reverse)?)?;
            }
        }
    }
    Ok(())
}

#[test]
fn cold_route_preserves_library_value_winners_on_identifiers_and_global_this() -> Result<(), String>
{
    let reports = compare_routes(vec![FileInput {
        name: "/wu6/value-winners.ts".to_owned(),
        source: concat!(
            "const JSON = 1;\n",
            "const jsonLocal: number = JSON;\n",
            "const jsonGlobal: number = globalThis.JSON;\n",
            "declare var document: number;\n",
            "const documentLocal: number = document;\n",
            "const documentGlobal: number = globalThis.document;\n",
            "declare var isNaN: number;\n",
            "const isNaNLocal: number = isNaN;\n",
            "const isNaNGlobal: number = globalThis.isNaN;\n",
        )
        .to_owned(),
    }])?;
    let [report] = reports.as_slice() else {
        return Err("value-winner project must return one report".to_owned());
    };
    let codes = diagnostic_codes(&report.output);
    if codes != ["TK2322"; 6] {
        return Err(format!(
            "library value winners must govern local and globalThis reads: {codes:?}"
        ));
    }
    Ok(())
}

#[test]
fn cold_route_matches_shared_collision_semantics_in_both_orders() -> Result<(), String> {
    for reverse in [false, true] {
        let reports = compare_routes(collision_project(reverse))?;
        let consumer = reports
            .iter()
            .find(|report| report.name == "/wu6/use.ts")
            .ok_or_else(|| "cold collision project omitted its consumer report".to_owned())?;
        let codes = diagnostic_codes(&consumer.output);
        if codes != ["TK2322"] {
            return Err(format!(
                "Array augmentation must merge before the dependent read: {codes:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn cold_route_preserves_parse_and_incomplete_channels() -> Result<(), String> {
    let parse_reports = compare_routes(vec![FileInput {
        name: "/wu6/recoverable.ts".to_owned(),
        source: concat!(
            "declare namespace Broken { export = Broken; }\n",
            "const semanticLeak: string = 1;\n",
        )
        .to_owned(),
    }])?;
    let [parse] = parse_reports.as_slice() else {
        return Err("recoverable parse project must return one report".to_owned());
    };
    if parse.output.parse_errors.len() != 1
        || !parse.output.diagnostics.is_empty()
        || !parse.output.incomplete.is_empty()
    {
        return Err(format!(
            "recoverable syntax reached semantics: {:?}",
            shape(&parse.output)
        ));
    }

    let incomplete_reports = compare_routes(vec![FileInput {
        name: "/wu6/valid.d.ts".to_owned(),
        source: concat!(
            "export as namespace Valid;\n",
            "export = Valid;\n",
            "declare function Valid(): void;\n",
        )
        .to_owned(),
    }])?;
    let [incomplete] = incomplete_reports.as_slice() else {
        return Err("declaration project must return one report".to_owned());
    };
    let ids = incomplete
        .output
        .incomplete
        .iter()
        .map(|surface| surface.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected = BTreeSet::from(["decl/export-assignment/self", "decl/namespace-export/self"]);
    if ids != expected || !incomplete.output.diagnostics.is_empty() {
        return Err(format!(
            "cold declaration route changed incomplete output: {:?}",
            shape(&incomplete.output)
        ));
    }
    Ok(())
}

#[test]
fn cli_and_library_info_attest_distinct_lifecycle_routes() -> Result<(), String> {
    if production_cli_route() != COLD_ROUTE {
        return Err(format!(
            "cold driver attestation differs: {}",
            production_cli_route()
        ));
    }
    let main = include_str!("../src/main.rs");
    if !main.contains("check_project_once") {
        return Err("ordinary CLI dispatch must use check_project_once".to_owned());
    }

    let output = Command::new(BIN)
        .args(["library-info", "--format", "json"])
        .output()
        .map_err(|error| format!("run library-info: {error}"))?;
    if output.status.code() != Some(0) || !output.stderr.is_empty() {
        return Err(format!("library-info failed: {output:#?}"));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("library-info JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "library-info must return an object".to_owned())?;
    let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected_keys = BTreeSet::from([
        "check_route",
        "file_count",
        "profile_sha256",
        "provider_route",
        "schema",
    ]);
    if keys != expected_keys
        || value["schema"] != 2
        || value["profile_sha256"] != PROFILE_SHA256
        || value["file_count"] != 82
        || value["check_route"] != COLD_ROUTE
        || value["provider_route"] != SHARED_ROUTE
    {
        return Err(format!("library-info route receipt differs: {value}"));
    }
    Ok(())
}

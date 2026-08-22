//! RED contract for the project-local default-library candidate in ADR-0023.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use typokat::driver::{check_project_once, FileReport};
use typokat::frontend::FileInput;

const PROFILE_SHA256: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const LOCKED_SLICE_SHA256: &str =
    "1d1d96a4196ccc77f9f8d1713425f252e6c49ca7b8efe44c39d326a2b100a514";
const ORACLE_SHA256: &str = "ebd05c48fe8e3e88ecfd77b90b9e2a6cd9d5a7bebf87016fa543123611651414";
const CASE_DIR: &str = "tests/cases/sr_project_local_library_candidate";

struct ExpectedRun {
    id: &'static str,
    sources: &'static [&'static str],
    diagnostics: &'static [ExpectedDiagnostic],
    normalized_semantics: &'static str,
}

#[derive(Clone, Copy)]
struct ExpectedDiagnostic {
    source: &'static str,
    code: u64,
    start_line: u64,
    start_column: u64,
    end_line: u64,
    end_column: u64,
    message: &'static str,
}

#[test]
fn manifest_and_oracles_are_frozen() {
    let root = repo_root();
    let candidate_dir = root.join(CASE_DIR);
    let locked_bytes = fs::read(candidate_dir.join("locked-slice.toml")).expect("locked slice");
    assert_eq!(
        format!("{:x}", Sha256::digest(&locked_bytes)),
        LOCKED_SLICE_SHA256,
        "the complete declaration/dependency/event slice changed"
    );
    let locked_text = String::from_utf8(locked_bytes).expect("locked slice is UTF-8");
    let locked = toml::from_str::<toml::Value>(&locked_text).expect("valid locked slice TOML");
    assert_eq!(locked["profile_identity"].as_str(), Some(PROFILE_SHA256));
    assert_eq!(
        string_array(&locked["required_cycle"]),
        ["Array", "ArrayConstructor", "ConcatArray"]
    );
    assert_eq!(
        string_array(&locked["required_demands"]),
        ["Array", "ReadonlyArray", "Reflect"]
    );
    assert_eq!(string_array(&locked["global_keys"]), ["Array", "Reflect"]);

    let sources = locked["sources"].as_array().expect("source rows");
    assert_eq!(sources.len(), 2);
    for source in sources {
        let name = source["name"].as_str().expect("source name");
        let expected = source["sha256"].as_str().expect("source digest");
        let bytes = fs::read(
            root.join("crates/typokat-library/src/typescript-6.0.3/lib")
                .join(name),
        )
        .expect("packaged source");
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), expected);
    }

    let fixture_names = discover_ts_files(&candidate_dir)
        .into_iter()
        .map(|path| relative_fixture_name(&candidate_dir, &path))
        .collect::<BTreeSet<_>>();
    assert_eq!(fixture_names, expected_fixture_names());

    let oracle_bytes = fs::read(candidate_dir.join("oracle.toml")).expect("frozen oracle");
    assert_eq!(
        format!("{:x}", Sha256::digest(&oracle_bytes)),
        ORACLE_SHA256
    );
    let oracle =
        toml::from_str::<toml::Value>(&String::from_utf8(oracle_bytes).expect("oracle is UTF-8"))
            .expect("valid oracle TOML");
    assert_eq!(oracle["typescript_version"].as_str(), Some("6.0.3"));
    assert_eq!(oracle["cases"].as_array().map(Vec::len), Some(3));

    let conformance = fs::read_to_string(root.join("tests/conformance.rs")).expect("harness");
    assert!(conformance.contains("(\"sr_project_local_library_candidate\", false)"));
    let adr = fs::read_to_string(
        root.join("docs/decisions/0023-own-default-library-semantics-per-project.md"),
    )
    .expect("ADR-0023");
    assert!(adr.contains("status: proposed"));
    let readme = fs::read_to_string(candidate_dir.join("README.md")).expect("candidate README");
    assert!(readme.contains("A count-only assertion is not an"));
    assert!(readme.contains("acceptable witness."));
}

#[test]
fn current_complete_source_divergences_are_frozen() {
    let root = repo_root();
    let reflect = check_fixture_once(
        &root.join(CASE_DIR).join("reflect_packaged.ts"),
        "/candidate/reflect_packaged.ts",
    );
    assert_eq!(diagnostic_codes(&reflect), Vec::<&str>::new());
    assert_eq!(incomplete_ids(&reflect), Vec::<&str>::new());

    let array = check_fixture_once(
        &root.join(CASE_DIR).join("array_library_scc.ts"),
        "/candidate/array_library_scc.ts",
    );
    assert_eq!(diagnostic_codes(&array), ["TK2322", "TK2322"]);
    assert_eq!(incomplete_ids(&array), Vec::<&str>::new());

    let symbol = check_fixture_once(
        &root.join("tests/cases/b121_symbol_namespace_surface/symbol_namespace_surface.ts"),
        "/candidate/symbol_namespace_surface.ts",
    );
    assert_eq!(diagnostic_codes(&symbol), Vec::<&str>::new());
    assert_eq!(
        incomplete_ids(&symbol),
        [
            "annotation-lower/type-syntax/unsupported",
            "annotation-lower/type-syntax/unsupported",
            "annotation-lower/type-syntax/unsupported"
        ]
    );
}

#[test]
fn every_candidate_gate_rejects_its_known_broken_control() {
    let valid = valid_candidate_receipt();
    validate_candidate_receipt(&valid).expect("control receipt is valid");

    let mutations = [
        broken(
            &valid,
            "shared-ast",
            "forbidden shared artifact tag: oxc-ast",
            |value| {
                value["shared_artifacts"] = json!([{"tag": "oxc-ast", "key": "program"}]);
            },
        ),
        broken(
            &valid,
            "shared-type-id",
            "forbidden shared artifact tag: type-id",
            |value| {
                value["shared_artifacts"] = json!([{"tag": "type-id", "key": "Array"}]);
            },
        ),
        broken(
            &valid,
            "shared-replay",
            "forbidden shared artifact tag: replay-owner",
            |value| {
                value["shared_artifacts"] = json!([{"tag": "replay-owner", "key": "Array"}]);
            },
        ),
        broken(
            &valid,
            "fallback",
            "candidate used a second production route",
            |value| {
                value["production_route"] = json!("complete-source-fallback");
            },
        ),
        broken(
            &valid,
            "missing-run",
            "semantic run inventory differs from the frozen contract",
            |value| {
                value["runs"].as_array_mut().expect("runs").pop();
            },
        ),
        broken(
            &valid,
            "diagnostic-drift",
            "semantic receipt drift for reflect-packaged/array-first",
            |value| {
                value["runs"][0]["diagnostics"][0]["start_line"] = json!(1);
            },
        ),
        broken(
            &valid,
            "diagnostic-message-drift",
            "semantic receipt drift for reflect-packaged/array-first",
            |value| {
                value["runs"][0]["diagnostics"][0]["message"] = json!("copied code");
            },
        ),
        broken(
            &valid,
            "source-not-executed",
            "semantic receipt drift for reflect-packaged/array-first",
            |value| {
                value["runs"][0]["sources"] = json!(["copied-oracle.ts"]);
            },
        ),
        broken(
            &valid,
            "demand-order-drift",
            "semantic receipt drift for array-scc/reflect-first",
            |value| {
                value["runs"][3]["normalized_semantics"] = json!("different");
            },
        ),
        broken(
            &valid,
            "source-order-drift",
            "semantic receipt drift for mixed-reverse/array-first",
            |value| {
                value["runs"][6]["normalized_semantics"] = json!("different");
            },
        ),
        broken(
            &valid,
            "reused-binder",
            "project-local owner was reused",
            |value| {
                value["projects"][1]["binder_owner"] = value["projects"][0]["binder_owner"].clone();
            },
        ),
        broken(
            &valid,
            "resolving-visible",
            "Resolving state became observable",
            |value| {
                value["projects"][0]["observed_resolving"] = json!(true);
            },
        ),
        broken(
            &valid,
            "missing-scc-member",
            "Array publication SCC is incomplete",
            |value| {
                value["projects"][0]["reserved_sccs"][0] = json!(["Array", "ConcatArray"]);
            },
        ),
        broken(
            &valid,
            "augmentation-leak",
            "foreign project augmentation leaked",
            |value| {
                value["projects"][1]["foreign_augmentation_keys"] = json!(["projectAOnly"]);
            },
        ),
    ];

    for (name, receipt, expected) in mutations {
        assert_eq!(
            validate_candidate_receipt(&receipt).expect_err("known-broken receipt must fail"),
            expected,
            "known-broken control did not reach its owning guard: {name}"
        );
    }
}

#[test]
#[ignore = "RED until the ADR-0023 candidate runner exists"]
fn candidate_matches_complete_source_contract() {
    let runner = std::env::var_os("TYPOKAT_PROJECT_LOCAL_CANDIDATE_RUNNER")
        .expect("set TYPOKAT_PROJECT_LOCAL_CANDIDATE_RUNNER to the reviewed candidate binary");
    let request = json!({
        "schema": 1,
        "profile_sha256": PROFILE_SHA256,
        "case_root": repo_root().join(CASE_DIR),
        "run_ids": expected_runs().iter().map(|run| run.id).collect::<Vec<_>>(),
        "creation_orders": [["isolation_a", "isolation_b"], ["isolation_b", "isolation_a"]],
        "demand_orders": [
            ["Array", "ReadonlyArray", "Reflect"],
            ["Reflect", "ReadonlyArray", "Array"]
        ]
    });
    let mut child = Command::new(runner)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("candidate runner starts");
    child
        .stdin
        .take()
        .expect("candidate stdin")
        .write_all(request.to_string().as_bytes())
        .expect("candidate request");
    let output = child.wait_with_output().expect("candidate response");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response =
        serde_json::from_slice::<Value>(&output.stdout).expect("candidate JSON response");
    validate_candidate_receipt(&response).expect("candidate satisfies ADR-0023 contract");
}

fn valid_candidate_receipt() -> Value {
    let runs = expected_runs()
        .into_iter()
        .map(|run| {
            json!({
                "id": run.id,
                "sources": run.sources,
                "diagnostics": diagnostics_json(run.diagnostics),
                "incomplete_ids": [],
                "normalized_semantics": run.normalized_semantics,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": 1,
        "profile_sha256": PROFILE_SHA256,
        "production_route": "checker-session",
        "shared_artifacts": [
            {"tag": "profile-identity", "key": PROFILE_SHA256},
            {"tag": "source-identity", "key": "lib.es5.d.ts"},
            {"tag": "declaration-site", "key": "0:57645"},
            {"tag": "symbolic-group", "key": "Array"},
            {"tag": "dependency-edge", "key": "Array->ConcatArray"},
            {"tag": "publication-scc", "key": "Array,ArrayConstructor,ConcatArray"},
            {"tag": "lexical-order", "key": "0:57645"}
        ],
        "runs": runs,
        "projects": [
            project_receipt("a-b", "isolation_a", "a-ab", "isolation-a"),
            project_receipt("a-b", "isolation_b", "b-ab", "isolation-b"),
            project_receipt("b-a", "isolation_b", "b-ba", "isolation-b"),
            project_receipt("b-a", "isolation_a", "a-ba", "isolation-a")
        ]
    })
}

fn validate_candidate_receipt(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or("candidate receipt is not an object")?;
    require_exact_keys(
        object,
        &[
            "profile_sha256",
            "production_route",
            "projects",
            "runs",
            "schema",
            "shared_artifacts",
        ],
    )?;
    if value["schema"] != 1 || value["profile_sha256"] != PROFILE_SHA256 {
        return Err("candidate identity differs from the locked profile".to_owned());
    }
    if value["production_route"] != "checker-session" {
        return Err("candidate used a second production route".to_owned());
    }
    validate_shared_artifacts(&value["shared_artifacts"])?;
    validate_semantic_runs(&value["runs"])?;
    validate_project_ownership(&value["projects"])
}

fn validate_shared_artifacts(value: &Value) -> Result<(), String> {
    let allowed = BTreeSet::from([
        "profile-identity",
        "source-identity",
        "declaration-site",
        "symbolic-group",
        "dependency-edge",
        "publication-scc",
        "lexical-order",
    ]);
    for artifact in value.as_array().ok_or("shared artifacts are not rows")? {
        let row = artifact
            .as_object()
            .ok_or("shared artifact is not an object")?;
        require_exact_keys(row, &["key", "tag"])?;
        let tag = artifact["tag"]
            .as_str()
            .ok_or("shared artifact tag is not text")?;
        if !allowed.contains(tag) {
            return Err(format!("forbidden shared artifact tag: {tag}"));
        }
    }
    Ok(())
}

fn validate_semantic_runs(value: &Value) -> Result<(), String> {
    let actual = value.as_array().ok_or("semantic runs are not rows")?;
    let expected = expected_runs()
        .into_iter()
        .map(|run| (run.id, run))
        .collect::<BTreeMap<_, _>>();
    let actual_ids = actual
        .iter()
        .filter_map(|run| run["id"].as_str())
        .collect::<BTreeSet<_>>();
    if actual.len() != expected.len() || actual_ids != expected.keys().copied().collect() {
        return Err("semantic run inventory differs from the frozen contract".to_owned());
    }
    for run in actual {
        let row = run.as_object().ok_or("semantic run is not an object")?;
        require_exact_keys(
            row,
            &[
                "diagnostics",
                "id",
                "incomplete_ids",
                "normalized_semantics",
                "sources",
            ],
        )?;
        let id = run["id"].as_str().ok_or("semantic run id is not text")?;
        let expected = &expected[id];
        let matches = run["sources"] == json!(expected.sources)
            && run["diagnostics"] == diagnostics_json(expected.diagnostics)
            && run["incomplete_ids"] == json!([])
            && run["normalized_semantics"] == expected.normalized_semantics;
        if !matches {
            return Err(format!("semantic receipt drift for {id}"));
        }
    }
    Ok(())
}

fn validate_project_ownership(value: &Value) -> Result<(), String> {
    let projects = value.as_array().ok_or("projects are not rows")?;
    let expected = BTreeSet::from([
        ("a-b", "isolation_a"),
        ("a-b", "isolation_b"),
        ("b-a", "isolation_a"),
        ("b-a", "isolation_b"),
    ]);
    let actual = projects
        .iter()
        .filter_map(|project| {
            Some((
                project["creation_order"].as_str()?,
                project["project"].as_str()?,
            ))
        })
        .collect::<BTreeSet<_>>();
    if projects.len() != expected.len() || actual != expected {
        return Err("both project creation orders were not executed".to_owned());
    }

    let mut owners = BTreeSet::new();
    for project in projects {
        let row = project
            .as_object()
            .ok_or("project receipt is not an object")?;
        require_exact_keys(
            row,
            &[
                "binder_owner",
                "cache_owner",
                "creation_order",
                "foreign_augmentation_keys",
                "global_object_owner",
                "interner_owner",
                "normalized_semantics",
                "observed_resolving",
                "project",
                "reserved_sccs",
                "store_owner",
            ],
        )?;
        if project["observed_resolving"] != false {
            return Err("Resolving state became observable".to_owned());
        }
        if project["reserved_sccs"][0] != json!(["Array", "ArrayConstructor", "ConcatArray"]) {
            return Err("Array publication SCC is incomplete".to_owned());
        }
        if project["foreign_augmentation_keys"] != json!([]) {
            return Err("foreign project augmentation leaked".to_owned());
        }
        let project_name = project["project"]
            .as_str()
            .ok_or("project name is not text")?;
        let expected_semantics = match project_name {
            "isolation_a" => "isolation-a",
            "isolation_b" => "isolation-b",
            _ => return Err("unknown project receipt".to_owned()),
        };
        if project["normalized_semantics"] != expected_semantics {
            return Err("project meaning changed with creation order".to_owned());
        }
        for field in [
            "binder_owner",
            "store_owner",
            "interner_owner",
            "cache_owner",
            "global_object_owner",
        ] {
            let owner = project[field]
                .as_str()
                .ok_or("project-local owner is not text")?;
            if !owners.insert(owner) {
                return Err("project-local owner was reused".to_owned());
            }
        }
    }
    Ok(())
}

fn expected_runs() -> Vec<ExpectedRun> {
    const REFLECT: &[&str] = &["reflect_packaged.ts"];
    const ARRAY: &[&str] = &["array_library_scc.ts"];
    const MIXED_FORWARD: &[&str] = &[
        "mixed_group_forward/00_script.ts",
        "mixed_group_forward/50_module.ts",
        "mixed_group_forward/99_consume.ts",
    ];
    const MIXED_REVERSE: &[&str] = &[
        "mixed_group_reverse/00_consume.ts",
        "mixed_group_reverse/50_module.ts",
        "mixed_group_reverse/99_script.ts",
    ];
    const ISOLATION_A: &[&str] = &["isolation_a/00_augment.ts", "isolation_a/99_consume.ts"];
    const ISOLATION_B: &[&str] = &["isolation_b/00_augment.ts", "isolation_b/99_consume.ts"];
    const NUMBER_TO_STRING: &str = "Type 'number' is not assignable to type 'string'.";
    const REFLECT_TO_NUMBER: &str =
        "Type '(string | symbol)[]' is not assignable to type 'number'.";
    const READONLY_TO_MUTABLE: &str = "The type 'readonly number[]' is 'readonly' and cannot be assigned to the mutable type 'string[]'.";
    const REFLECT_DIAGNOSTICS: &[ExpectedDiagnostic] = &[expected_diagnostic(
        "reflect_packaged.ts",
        2322,
        2,
        7,
        2,
        16,
        REFLECT_TO_NUMBER,
    )];
    const ARRAY_DIAGNOSTICS: &[ExpectedDiagnostic] = &[
        expected_diagnostic("array_library_scc.ts", 2322, 7, 7, 7, 25, NUMBER_TO_STRING),
        expected_diagnostic("array_library_scc.ts", 2322, 8, 7, 8, 23, NUMBER_TO_STRING),
        expected_diagnostic(
            "array_library_scc.ts",
            2322,
            10,
            7,
            10,
            25,
            REFLECT_TO_NUMBER,
        ),
    ];
    const MIXED_FORWARD_DIAGNOSTICS: &[ExpectedDiagnostic] = &[
        expected_diagnostic(
            "mixed_group_forward/99_consume.ts",
            2322,
            4,
            7,
            4,
            18,
            NUMBER_TO_STRING,
        ),
        expected_diagnostic(
            "mixed_group_forward/99_consume.ts",
            4104,
            5,
            7,
            5,
            18,
            READONLY_TO_MUTABLE,
        ),
    ];
    const MIXED_REVERSE_DIAGNOSTICS: &[ExpectedDiagnostic] = &[
        expected_diagnostic(
            "mixed_group_reverse/00_consume.ts",
            2322,
            4,
            7,
            4,
            18,
            NUMBER_TO_STRING,
        ),
        expected_diagnostic(
            "mixed_group_reverse/00_consume.ts",
            4104,
            5,
            7,
            5,
            18,
            READONLY_TO_MUTABLE,
        ),
    ];
    const ISOLATION_A_DIAGNOSTICS: &[ExpectedDiagnostic] = &[expected_diagnostic(
        "isolation_a/99_consume.ts",
        2551,
        2,
        5,
        2,
        17,
        "Property 'projectBOnly' does not exist on type 'number[]'. Did you mean 'projectAOnly'?",
    )];
    const ISOLATION_B_DIAGNOSTICS: &[ExpectedDiagnostic] = &[expected_diagnostic(
        "isolation_b/99_consume.ts",
        2551,
        2,
        5,
        2,
        17,
        "Property 'projectAOnly' does not exist on type 'number[]'. Did you mean 'projectBOnly'?",
    )];
    vec![
        expected_run(
            "reflect-packaged/array-first",
            REFLECT,
            REFLECT_DIAGNOSTICS,
            "reflect",
        ),
        expected_run(
            "reflect-packaged/reflect-first",
            REFLECT,
            REFLECT_DIAGNOSTICS,
            "reflect",
        ),
        expected_run(
            "array-scc/array-first",
            ARRAY,
            ARRAY_DIAGNOSTICS,
            "array-scc",
        ),
        expected_run(
            "array-scc/reflect-first",
            ARRAY,
            ARRAY_DIAGNOSTICS,
            "array-scc",
        ),
        expected_run(
            "mixed-forward/array-first",
            MIXED_FORWARD,
            MIXED_FORWARD_DIAGNOSTICS,
            "mixed",
        ),
        expected_run(
            "mixed-forward/reflect-first",
            MIXED_FORWARD,
            MIXED_FORWARD_DIAGNOSTICS,
            "mixed",
        ),
        expected_run(
            "mixed-reverse/array-first",
            MIXED_REVERSE,
            MIXED_REVERSE_DIAGNOSTICS,
            "mixed",
        ),
        expected_run(
            "mixed-reverse/reflect-first",
            MIXED_REVERSE,
            MIXED_REVERSE_DIAGNOSTICS,
            "mixed",
        ),
        expected_run(
            "isolation-a/array-first",
            ISOLATION_A,
            ISOLATION_A_DIAGNOSTICS,
            "isolation-a",
        ),
        expected_run(
            "isolation-a/reflect-first",
            ISOLATION_A,
            ISOLATION_A_DIAGNOSTICS,
            "isolation-a",
        ),
        expected_run(
            "isolation-b/array-first",
            ISOLATION_B,
            ISOLATION_B_DIAGNOSTICS,
            "isolation-b",
        ),
        expected_run(
            "isolation-b/reflect-first",
            ISOLATION_B,
            ISOLATION_B_DIAGNOSTICS,
            "isolation-b",
        ),
    ]
}

fn expected_run(
    id: &'static str,
    sources: &'static [&'static str],
    diagnostics: &'static [ExpectedDiagnostic],
    normalized_semantics: &'static str,
) -> ExpectedRun {
    ExpectedRun {
        id,
        sources,
        diagnostics,
        normalized_semantics,
    }
}

const fn expected_diagnostic(
    source: &'static str,
    code: u64,
    start_line: u64,
    start_column: u64,
    end_line: u64,
    end_column: u64,
    message: &'static str,
) -> ExpectedDiagnostic {
    ExpectedDiagnostic {
        source,
        code,
        start_line,
        start_column,
        end_line,
        end_column,
        message,
    }
}

fn diagnostics_json(diagnostics: &[ExpectedDiagnostic]) -> Value {
    Value::Array(
        diagnostics
            .iter()
            .map(|diagnostic| {
                json!({
                    "source": diagnostic.source,
                    "code": diagnostic.code,
                    "start_line": diagnostic.start_line,
                    "start_column": diagnostic.start_column,
                    "end_line": diagnostic.end_line,
                    "end_column": diagnostic.end_column,
                    "message": diagnostic.message,
                })
            })
            .collect(),
    )
}

fn project_receipt(order: &str, project: &str, suffix: &str, semantics: &str) -> Value {
    json!({
        "creation_order": order,
        "project": project,
        "binder_owner": format!("binder-{suffix}"),
        "store_owner": format!("store-{suffix}"),
        "interner_owner": format!("interner-{suffix}"),
        "cache_owner": format!("cache-{suffix}"),
        "global_object_owner": format!("global-{suffix}"),
        "normalized_semantics": semantics,
        "reserved_sccs": [["Array", "ArrayConstructor", "ConcatArray"]],
        "observed_resolving": false,
        "foreign_augmentation_keys": []
    })
}

fn broken(
    valid: &Value,
    name: &'static str,
    expected: &'static str,
    edit: impl FnOnce(&mut Value),
) -> (&'static str, Value, &'static str) {
    let mut value = valid.clone();
    edit(&mut value);
    (name, value, expected)
}

fn require_exact_keys(object: &Map<String, Value>, expected: &[&str]) -> Result<(), String> {
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("unexpected receipt keys: {actual:?}"))
    }
}

fn expected_fixture_names() -> BTreeSet<String> {
    expected_runs()
        .into_iter()
        .flat_map(|run| run.sources.iter().copied())
        .map(str::to_owned)
        .collect()
}

fn check_fixture_once(path: &Path, logical_name: &str) -> FileReport {
    let source = fs::read_to_string(path).expect("oracle fixture");
    let mut reports = check_project_once(vec![FileInput {
        name: logical_name.to_owned(),
        source,
    }])
    .expect("complete-source oracle");
    assert_eq!(reports.len(), 1, "one report per oracle fixture");
    reports.pop().expect("oracle report")
}

fn diagnostic_codes(report: &FileReport) -> Vec<&str> {
    report
        .output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn incomplete_ids(report: &FileReport) -> Vec<&str> {
    report
        .output
        .incomplete
        .iter()
        .map(|surface| surface.id.as_str())
        .collect()
}

fn relative_fixture_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("candidate-relative path")
        .to_string_lossy()
        .replace('\\', "/")
}

fn string_array(value: &toml::Value) -> Vec<&str> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("text item"))
        .collect()
}

fn discover_ts_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(current).expect("candidate directory") {
            let path = entry.expect("candidate entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "ts") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use toml::Value;
use typokat::driver::{check_project, CheckOutput, FileInput};
use typokat::span::LineIndex;

const FIXTURE_DIR: &str = "tests/fixtures/lib-es5-6.0.3";
const MANIFEST_PATH: &str = "tests/fixtures/lib-es5-6.0.3/readiness.toml";
const BACKLOG_43: &str = "43-namespaces-declaration-merging.md";
const TYPESCRIPT_VERSION: &str = "6.0.3";
const UPSTREAM_REVISION: &str = "050880ce59e30b356b686bd3144efe24f875ebc8";
const UPSTREAM_PACKAGE_PATH: &str = "lib/lib.es5.d.ts";
const UPSTREAM_GIT_BLOB: &str = "496166ca309c28ab7e07ea0154a406f26b6cf26a";
const ARTIFACT_SHA256: &str = "bcd24271a113971ba9eb71ff8cb01bc6b0f872a85c23fdbe5d93065b375933cd";
const ARTIFACT_BYTES: usize = 218_972;
const ARTIFACT_LFS: usize = 4_599;
const LICENSE_SHA256: &str = "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47";
const LICENSE_BYTES: usize = 9_197;
const LICENSE_LFS: usize = 55;
const THIRD_PARTY_SHA256: &str = "1af3c68039c57e539422da82a4faada506ce6d0ea6f90e0b699d02dbcdb7a90c";
const THIRD_PARTY_BYTES: usize = 37_824;
const THIRD_PARTY_LFS: usize = 193;
const TARGET: &str = "typescript-6.0.3-lib-es5-model-readiness";
const REASON: &str =
    "The pinned explicit-input type model has no backlog-43 residual, so backlog 14 may start; owners 50 and 75 still block checker 1.0.";
const ARTIFACT_PATH: &str = "tests/fixtures/lib-es5-6.0.3/lib.es5.d.ts";
const WITNESS_PATH: &str = "tests/fixtures/lib-es5-6.0.3/semantic-witnesses.ts";
const CHECKER_REVISION: &str = "23bad42";
const RAW_COMMAND: &str =
    "target/debug/typokat check --format compact tests/fixtures/lib-es5-6.0.3/lib.es5.d.ts";
const SYNTHETIC_COMMAND: &str = "target/debug/typokat check --format compact $COMBINED";
const TSC_EXPLICIT_COMMAND: &str = "tsc --strict --noEmit --pretty false --noLib tests/fixtures/lib-es5-6.0.3/lib.es5.d.ts tests/fixtures/lib-es5-6.0.3/semantic-witnesses.ts";
const TSC_SYNTHETIC_COMMAND: &str = "tsc --strict --noEmit --pretty false --noLib $COMBINED";
const OWNER_14: &str = "../../../docs/backlog/14-libdts-loading.md";
const OWNER_43: &str = "../../../docs/backlog/43-namespaces-declaration-merging.md";
const OWNER_50: &str = "../../../docs/backlog/50-type-predicates-assertions.md";
const OWNER_63: &str = "../../../docs/backlog/63-review-parity-tail.md";
const OWNER_75: &str = "../../../docs/backlog/75-scope-surface-tail.md";
const DEEP_WITNESS_IDS: &[&str] = &[
    "deep.Array.element",
    "deep.Date.member",
    "deep.Intl.type",
    "deep.Intl.value",
    "deep.Number.member",
    "deep.Object.member",
    "deep.String.member",
    "deep.repeat.Date",
    "deep.repeat.Number",
    "deep.repeat.String",
];
const RAW_OUTPUT_SHA256: &str = "00b45da6ed7d88713970cb355915317204d5e15dfda97571d0ddcde4218169b3";

#[derive(Clone, Debug, PartialEq, Eq)]
struct WitnessRecord {
    oracle: String,
    typokat: String,
    owner: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Marker {
    id: String,
    oracle: String,
    typokat: String,
    owner: Option<String>,
    witness_line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DiagnosticSite {
    code: String,
    line: u32,
    column: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpanSnapshot {
    line: u32,
    column: u32,
    code: String,
    start: u32,
    end: u32,
    text: String,
}

enum ResidualWitness {
    Lib { line: u32, column: u32 },
    Marker(String),
}

#[test]
fn pinned_lib_es5_model_readiness_is_exact_and_reproducible() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = root.join(MANIFEST_PATH);
    let manifest_text = read_utf8(&manifest_path);
    let manifest: Value = manifest_text
        .parse()
        .unwrap_or_else(|error| panic!("{} must be valid TOML: {error}", manifest_path.display()));
    let root_table = table(&manifest, "manifest root");

    validate_manifest_shape(root_table);
    validate_manifest_contract(root_table);
    validate_decision_and_owners(root_table, &root, &manifest_path);

    let artifact_record = child_table(root_table, "artifact", "manifest root");
    let synthetic_record = child_table(root_table, "synthetic_source", "manifest root");
    let artifact_path = root.join(string(artifact_record, "path", "artifact"));
    let witness_path = root.join(string(synthetic_record, "witness_path", "synthetic_source"));
    let artifact_bytes = std::fs::read(&artifact_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", artifact_path.display()));
    let witness_bytes = std::fs::read(&witness_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", witness_path.display()));
    assert_eq!(
        artifact_bytes.last(),
        Some(&b'\n'),
        "artifact must end in LF"
    );
    assert_eq!(
        witness_bytes.last(),
        Some(&b'\n'),
        "witness source must end in LF"
    );

    verify_blob(&artifact_bytes, artifact_record, "artifact");
    verify_fixed_blob(
        &artifact_bytes,
        ARTIFACT_SHA256,
        ARTIFACT_BYTES,
        ARTIFACT_LFS,
        "upstream artifact",
    );
    assert_eq!(
        string(artifact_record, "typescript_version", "artifact"),
        TYPESCRIPT_VERSION
    );
    assert_eq!(
        string(artifact_record, "upstream_revision", "artifact"),
        UPSTREAM_REVISION
    );
    assert_eq!(
        string(artifact_record, "npm_package_path", "artifact"),
        UPSTREAM_PACKAGE_PATH
    );
    assert_eq!(
        string(artifact_record, "upstream_git_blob", "artifact"),
        UPSTREAM_GIT_BLOB
    );
    assert_eq!(
        string(artifact_record, "sha256", "artifact"),
        ARTIFACT_SHA256
    );
    assert_eq!(
        usize_integer(artifact_record, "bytes", "artifact"),
        ARTIFACT_BYTES
    );
    assert_eq!(
        usize_integer(artifact_record, "lines", "artifact"),
        ARTIFACT_LFS
    );
    verify_fixed_file(
        &root.join(FIXTURE_DIR).join("LICENSE.txt"),
        LICENSE_SHA256,
        LICENSE_BYTES,
        LICENSE_LFS,
        "vendored TypeScript license",
    );
    verify_fixed_file(
        &root.join(FIXTURE_DIR).join("ThirdPartyNoticeText.txt"),
        THIRD_PARTY_SHA256,
        THIRD_PARTY_BYTES,
        THIRD_PARTY_LFS,
        "vendored TypeScript third-party notice",
    );
    let oracle = child_table(
        child_table(root_table, "measurements", "manifest root"),
        "tsc_oracle",
        "measurements",
    );
    assert_eq!(
        string(artifact_record, "typescript_version", "artifact"),
        string(oracle, "typescript_version", "measurements.tsc_oracle")
    );
    assert_eq!(
        string(synthetic_record, "artifact_path", "synthetic_source"),
        string(artifact_record, "path", "artifact"),
        "synthetic_source.artifact_path must identify the pinned artifact"
    );
    verify_named_blob(
        &witness_bytes,
        synthetic_record,
        "witness_sha256",
        "witness_bytes",
        "witness_lines",
        "semantic witness",
    );

    assert_eq!(
        string(synthetic_record, "separator", "synthetic_source"),
        "one additional LF",
        "the synthetic source separator is part of the proof contract"
    );
    let mut synthetic_bytes = artifact_bytes.clone();
    synthetic_bytes.push(b'\n');
    synthetic_bytes.extend_from_slice(&witness_bytes);
    verify_blob(&synthetic_bytes, synthetic_record, "synthetic source");

    let artifact = String::from_utf8(artifact_bytes)
        .unwrap_or_else(|error| panic!("{} must be UTF-8: {error}", artifact_path.display()));
    let witnesses = String::from_utf8(witness_bytes)
        .unwrap_or_else(|error| panic!("{} must be UTF-8: {error}", witness_path.display()));
    let synthetic = String::from_utf8(synthetic_bytes)
        .unwrap_or_else(|error| panic!("synthetic lib.es5 proof source must be UTF-8: {error}"));

    let repeated_names = validate_declaration_inventory(root_table, &artifact);
    let markers = validate_witness_markers(root_table, &witnesses, &manifest_path, &repeated_names);

    let raw = run_one(
        string(artifact_record, "path", "artifact"),
        artifact.clone(),
    );
    validate_raw_measurement(root_table, &artifact, &markers, &raw);

    let synthetic_name = format!("{FIXTURE_DIR}/combined-lib-es5-readiness.d.ts");
    let first_synthetic = run_one(&synthetic_name, synthetic.clone());
    validate_synthetic_measurement(
        root_table,
        &artifact,
        &witnesses,
        &synthetic,
        &markers,
        &raw,
        &first_synthetic,
    );

    let second_synthetic = run_one(&synthetic_name, synthetic);
    assert_eq!(
        first_synthetic.parse_errors, second_synthetic.parse_errors,
        "repeated synthetic checks must have identical parse output"
    );
    assert_eq!(
        first_synthetic.diagnostics, second_synthetic.diagnostics,
        "repeated synthetic checks must preserve every diagnostic and its order"
    );
    assert_eq!(
        first_synthetic.incomplete, second_synthetic.incomplete,
        "repeated synthetic checks must preserve every incomplete record and its order"
    );
}

fn validate_manifest_shape(root: &toml::Table) {
    exact_key_words(root, "schema target conclusion decision_target reason witness_marker_grammar artifact synthetic_source measurements pair_set deep_witness_set witness residual", "manifest root");
    assert_eq!(integer(root, "schema", "manifest root"), 1);
    assert_eq!(
        string(root, "witness_marker_grammar", "manifest root"),
        "// witness[ID] oracle[TSdddd] typokat[TKdddd] [owner[relative-path]]"
    );
    exact_key_words(child_table(root, "artifact", "manifest root"), "path typescript_version upstream_revision npm_package_path upstream_git_blob sha256 bytes lines", "artifact");
    exact_key_words(child_table(root, "synthetic_source", "manifest root"), "artifact_path witness_path separator witness_sha256 witness_bytes witness_lines sha256 bytes lines", "synthetic_source");
    exact_key_words(
        child_table(root, "pair_set", "manifest root"),
        "count names",
        "pair_set",
    );
    exact_key_words(
        child_table(root, "deep_witness_set", "manifest root"),
        "count ids",
        "deep_witness_set",
    );

    let measurements = child_table(root, "measurements", "manifest root");
    exact_key_words(
        measurements,
        "raw_typokat synthetic_typokat tsc_oracle",
        "measurements",
    );
    exact_key_words(
        child_table(measurements, "raw_typokat", "measurements"),
        "checker_revision command exit_code parse_errors diagnostics incompletes",
        "measurements.raw_typokat",
    );
    exact_key_words(child_table(measurements, "synthetic_typokat", "measurements"), "checker_revision command exit_code parse_errors diagnostics incompletes artifact_diagnostics witness_diagnostics tk2304 tk2322 tk2430", "measurements.synthetic_typokat");
    exact_key_words(child_table(measurements, "tsc_oracle", "measurements"), "typescript_version explicit_command synthetic_command explicit_exit_code synthetic_exit_code explicit_diagnostics synthetic_diagnostics ts2322", "measurements.tsc_oracle");

    let witness = child_table(root, "witness", "manifest root");
    for (id, value) in witness {
        let record = table(value, &format!("witness.{id}"));
        let oracle = string(record, "oracle", &format!("witness.{id}"));
        let typokat = string(record, "typokat", &format!("witness.{id}"));
        let keys = if same_code(oracle, typokat) {
            vec!["oracle", "typokat"]
        } else {
            vec!["oracle", "typokat", "owner"]
        };
        exact_keys(record, &keys, &format!("witness.{id}"));
    }

    let residuals = array(root, "residual", "manifest root");
    let mut ids = BTreeSet::new();
    for value in residuals {
        let residual = table(value, "residual record");
        let id = string(residual, "id", "residual record");
        assert!(ids.insert(id), "duplicate residual id {id:?}");
        let expected = match (string(residual, "outcome", id), id) {
            ("diagnostic", _) => vec![
                "id",
                "outcome",
                "code",
                "count",
                "owner",
                "witnesses",
                "actual_cause",
            ],
            ("incomplete", "standalone-namespace-value") => vec![
                "id",
                "outcome",
                "surface",
                "count",
                "owner",
                "witnesses",
                "architecture_stop",
                "actual_cause",
            ],
            ("incomplete", _) => vec![
                "id",
                "outcome",
                "surface",
                "count",
                "owner",
                "witness",
                "actual_cause",
            ],
            (outcome, _) => panic!("residual {id:?} has unsupported outcome {outcome:?}"),
        };
        exact_keys(residual, &expected, &format!("residual {id}"));
        assert!(
            !string(residual, "actual_cause", id).is_empty(),
            "residual {id}.actual_cause must be nonempty"
        );
        assert!(usize_integer(residual, "count", id) > 0);
        if let Some(stop) = residual.get("architecture_stop") {
            assert_eq!(
                stop.as_bool(),
                Some(true),
                "{id}.architecture_stop must be boolean true"
            );
        }
    }
}

fn validate_manifest_contract(root: &toml::Table) {
    assert_eq!(string(root, "target", "manifest root"), TARGET);
    assert_eq!(string(root, "reason", "manifest root"), REASON);
    let artifact = child_table(root, "artifact", "manifest root");
    assert_eq!(string(artifact, "path", "artifact"), ARTIFACT_PATH);
    assert_eq!(
        string(artifact, "typescript_version", "artifact"),
        TYPESCRIPT_VERSION
    );
    assert_eq!(
        string(artifact, "upstream_revision", "artifact"),
        UPSTREAM_REVISION
    );
    assert_eq!(
        string(artifact, "npm_package_path", "artifact"),
        UPSTREAM_PACKAGE_PATH
    );
    assert_eq!(
        string(artifact, "upstream_git_blob", "artifact"),
        UPSTREAM_GIT_BLOB
    );

    let synthetic = child_table(root, "synthetic_source", "manifest root");
    assert_eq!(
        string(synthetic, "artifact_path", "synthetic_source"),
        ARTIFACT_PATH
    );
    assert_eq!(
        string(synthetic, "witness_path", "synthetic_source"),
        WITNESS_PATH
    );

    let measurements = child_table(root, "measurements", "manifest root");
    let raw = child_table(measurements, "raw_typokat", "measurements");
    assert_eq!(
        string(raw, "checker_revision", "raw measurement"),
        CHECKER_REVISION
    );
    assert_eq!(string(raw, "command", "raw measurement"), RAW_COMMAND);
    let combined = child_table(measurements, "synthetic_typokat", "measurements");
    assert_eq!(
        string(combined, "checker_revision", "synthetic measurement"),
        CHECKER_REVISION
    );
    assert_eq!(
        string(combined, "command", "synthetic measurement"),
        SYNTHETIC_COMMAND
    );

    // This checks the committed oracle contract; the offline gate never invokes tsc.
    let oracle = child_table(measurements, "tsc_oracle", "measurements");
    assert_eq!(
        string(oracle, "typescript_version", "tsc oracle"),
        TYPESCRIPT_VERSION
    );
    assert_eq!(
        string(oracle, "explicit_command", "tsc oracle"),
        TSC_EXPLICIT_COMMAND
    );
    assert_eq!(
        string(oracle, "synthetic_command", "tsc oracle"),
        TSC_SYNTHETIC_COMMAND
    );
    assert_eq!(integer(oracle, "explicit_exit_code", "tsc oracle"), 2);
    assert_eq!(integer(oracle, "synthetic_exit_code", "tsc oracle"), 2);
    assert_eq!(integer(oracle, "explicit_diagnostics", "tsc oracle"), 66);
    assert_eq!(integer(oracle, "synthetic_diagnostics", "tsc oracle"), 66);
    assert_eq!(integer(oracle, "ts2322", "tsc oracle"), 66);
}

fn validate_decision_and_owners(root: &toml::Table, repo_root: &Path, manifest_path: &Path) {
    let manifest_dir = manifest_path
        .parent()
        .expect("readiness manifest must have a parent directory");
    let decision_target = string(root, "decision_target", "manifest root");
    assert_safe_repo_path(
        repo_root,
        manifest_dir,
        decision_target,
        OWNER_14,
        "decision_target",
    );

    let expected_owners = BTreeMap::from([
        ("annotation-bigint-keyword", OWNER_75),
        ("annotation-intrinsic-keyword", OWNER_75),
        ("annotation-object-keyword", OWNER_75),
        ("annotation-symbol-keyword", OWNER_75),
        ("annotation-this-type", OWNER_75),
        ("annotation-type-predicate", OWNER_50),
        ("callable-heritage-canonical", OWNER_14),
        ("callable-heritage-cardinality", OWNER_63),
    ]);

    let mut architecture_stops = Vec::new();
    let mut backlog_43_owners = Vec::new();
    for value in array(root, "residual", "manifest root") {
        let residual = table(value, "residual record");
        let id = string(residual, "id", "residual record");
        let owner = string(residual, "owner", id);
        let expected = expected_owners
            .get(id)
            .unwrap_or_else(|| panic!("residual {id:?} has no pinned owner"));
        assert_safe_repo_path(
            repo_root,
            manifest_dir,
            owner,
            expected,
            &format!("residual {id} owner"),
        );
        if owner.ends_with(BACKLOG_43) {
            backlog_43_owners.push(id);
        }
        if residual.get("architecture_stop").and_then(Value::as_bool) == Some(true) {
            architecture_stops.push((id, owner));
        }
    }
    assert_eq!(
        array(root, "residual", "manifest root").len(),
        expected_owners.len(),
        "residual owner map must be exhaustive"
    );
    let witness_table = child_table(root, "witness", "manifest root");
    let witness_owns_backlog_43 = witness_table.values().any(|value| {
        table(value, "witness record")
            .get("owner")
            .and_then(Value::as_str)
            .is_some_and(|owner| owner.ends_with(BACKLOG_43))
    });

    let conclusion = string(root, "conclusion", "manifest root");
    let has_exact_stop =
        architecture_stops.len() == 1 && architecture_stops[0].1.ends_with(BACKLOG_43);
    assert_eq!(
        conclusion == "NO-GO",
        has_exact_stop,
        "NO-GO must be equivalent to exactly one backlog-43 architecture stop"
    );
    match conclusion {
        "NO-GO" => assert_eq!(
            backlog_43_owners,
            vec!["standalone-namespace-value"],
            "the current NO-GO may have only the declared backlog-43 owner"
        ),
        "GO" => assert!(
            backlog_43_owners.is_empty() && !witness_owns_backlog_43,
            "a future GO manifest must not retain any backlog-43 residual"
        ),
        other => panic!("conclusion must be GO or NO-GO, got {other:?}"),
    }
}

fn validate_declaration_inventory(root: &toml::Table, source: &str) -> Vec<String> {
    let mut interface_counts = BTreeMap::<String, usize>::new();
    let mut vars = BTreeSet::<String>::new();
    for line in source.lines() {
        if let Some(name) = declaration_name(line, "interface ") {
            *interface_counts.entry(name).or_default() += 1;
        }
        if let Some(name) = declaration_name(line, "declare var ") {
            vars.insert(name);
        }
    }
    let actual_pairs: Vec<String> = interface_counts
        .keys()
        .filter(|name| vars.contains(*name))
        .cloned()
        .collect();
    let pair_set = child_table(root, "pair_set", "manifest root");
    let expected_pairs = string_array(pair_set, "names", "pair_set");
    assert_sorted_unique(&expected_pairs, "pair_set.names");
    assert_eq!(
        actual_pairs, expected_pairs,
        "interface/declare-var pair set drifted"
    );
    assert_eq!(
        expected_pairs.len(),
        usize_integer(pair_set, "count", "pair_set"),
        "pair_set.count must equal pair_set.names length"
    );

    let repeated: BTreeMap<String, usize> = interface_counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .collect();
    assert_eq!(
        repeated,
        BTreeMap::from([
            ("Date".to_string(), 2),
            ("Number".to_string(), 2),
            ("String".to_string(), 2),
        ]),
        "repeated interface declarations must remain exactly Date/Number/String twice each"
    );
    repeated.into_keys().collect()
}

fn validate_witness_markers(
    root: &toml::Table,
    source: &str,
    manifest_path: &Path,
    repeated_names: &[String],
) -> Vec<Marker> {
    let manifest_witnesses = manifest_witness_records(root);
    let mut markers = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, line) in source.lines().enumerate() {
        let occurrences = line.matches("// witness[").count();
        assert!(
            occurrences <= 1,
            "semantic witness line {} contains {occurrences} markers",
            index + 1
        );
        let Some(comment_start) = line.find("// witness[") else {
            continue;
        };
        let marker = parse_marker(
            &line[comment_start..],
            u32::try_from(index + 1).expect("witness line number fits u32"),
        );
        assert!(
            seen.insert(marker.id.clone()),
            "duplicate witness id {:?}",
            marker.id
        );
        if same_code(&marker.oracle, &marker.typokat) {
            assert!(
                marker.owner.is_none(),
                "matching witness {:?} must not have an owner",
                marker.id
            );
        } else {
            let owner = marker
                .owner
                .as_deref()
                .unwrap_or_else(|| panic!("divergent witness {:?} requires an owner", marker.id));
            let manifest_dir = manifest_path.parent().expect("manifest has parent");
            assert_safe_repo_path(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                manifest_dir,
                owner,
                OWNER_43,
                &format!("witness {} owner", marker.id),
            );
        }
        markers.push(marker);
    }

    let marker_records: BTreeMap<String, WitnessRecord> = markers
        .iter()
        .map(|marker| {
            (
                marker.id.clone(),
                WitnessRecord {
                    oracle: marker.oracle.clone(),
                    typokat: marker.typokat.clone(),
                    owner: marker.owner.clone(),
                },
            )
        })
        .collect();
    assert_eq!(
        marker_records, manifest_witnesses,
        "source markers and manifest witness table must match exactly"
    );

    let deep = child_table(root, "deep_witness_set", "manifest root");
    let deep_ids = string_array(deep, "ids", "deep_witness_set");
    assert_eq!(
        deep_ids,
        DEEP_WITNESS_IDS
            .iter()
            .map(|id| (*id).to_string())
            .collect::<Vec<_>>(),
        "deep witness ids must match the pinned ordered contract"
    );
    assert_eq!(
        deep_ids.len(),
        usize_integer(deep, "count", "deep_witness_set")
    );
    let pair_names = string_array(
        child_table(root, "pair_set", "manifest root"),
        "names",
        "pair_set",
    );
    let mut expected_order = deep_ids;
    let repeated_ids: Vec<String> = repeated_names
        .iter()
        .map(|name| format!("deep.repeat.{name}"))
        .collect();
    assert_eq!(
        &expected_order[expected_order.len() - repeated_ids.len()..],
        repeated_ids,
        "deep repeat witnesses must be derived from repeated source declarations"
    );
    for name in pair_names {
        expected_order.push(format!("pair.{name}.type"));
        expected_order.push(format!("pair.{name}.value"));
    }
    let actual_order: Vec<&str> = markers.iter().map(|marker| marker.id.as_str()).collect();
    let expected_order_refs: Vec<&str> = expected_order.iter().map(String::as_str).collect();
    assert_eq!(
        actual_order, expected_order_refs,
        "witness source order drifted"
    );
    let oracle = child_table(
        child_table(root, "measurements", "manifest root"),
        "tsc_oracle",
        "measurements",
    );
    assert!(markers.iter().all(|marker| marker.oracle == "TS2322"));
    assert_eq!(
        markers.len(),
        usize_integer(oracle, "explicit_diagnostics", "tsc oracle")
    );
    assert_eq!(
        markers.len(),
        usize_integer(oracle, "synthetic_diagnostics", "tsc oracle")
    );
    assert_eq!(markers.len(), usize_integer(oracle, "ts2322", "tsc oracle"));
    validate_pair_marker_semantics(source, &markers);
    markers
}

fn validate_raw_measurement(
    root: &toml::Table,
    source: &str,
    markers: &[Marker],
    output: &CheckOutput,
) {
    let measurement = child_table(
        child_table(root, "measurements", "manifest root"),
        "raw_typokat",
        "measurements",
    );
    assert_eq!(
        output.parse_errors.len(),
        usize_integer(measurement, "parse_errors", "raw measurement")
    );
    assert_eq!(
        output.diagnostics.len(),
        usize_integer(measurement, "diagnostics", "raw measurement")
    );
    assert_eq!(
        output.incomplete.len(),
        usize_integer(measurement, "incompletes", "raw measurement")
    );
    assert_eq!(
        output.parse_errors.len(),
        0,
        "the pinned artifact must parse cleanly"
    );
    assert_eq!(
        output.diagnostics.len(),
        4,
        "raw diagnostic cardinality drifted"
    );
    assert_eq!(
        output.incomplete.len(),
        187,
        "raw incomplete cardinality drifted"
    );
    assert_eq!(integer(measurement, "exit_code", "raw measurement"), 3);
    assert_eq!(
        raw_output_fingerprint(output),
        RAW_OUTPUT_SHA256,
        "the complete ordered raw output changed"
    );

    let line_index = LineIndex::new(source);
    let marker_by_id: BTreeMap<&str, &Marker> = markers
        .iter()
        .map(|marker| (marker.id.as_str(), marker))
        .collect();
    let mut incomplete_counts = BTreeMap::<String, usize>::new();
    for record in &output.incomplete {
        *incomplete_counts.entry(record.id.clone()).or_default() += 1;
    }
    let mut expected_incomplete = BTreeMap::new();
    let mut expected_diagnostic_sites = BTreeMap::<DiagnosticSite, usize>::new();
    for value in array(root, "residual", "manifest root") {
        let residual = table(value, "residual record");
        let id = string(residual, "id", "residual record");
        let count = usize_integer(residual, "count", id);
        let owner = string(residual, "owner", id);
        let sites = residual_sites(residual, id);
        match string(residual, "outcome", id) {
            "incomplete" => {
                let surface = string(residual, "surface", id).to_string();
                assert!(
                    expected_incomplete.insert(surface.clone(), count).is_none(),
                    "duplicate residual surface {surface:?}"
                );
                for site in sites {
                    match parse_residual_witness(&site) {
                        ResidualWitness::Lib { line, column } => {
                            assert_source_position(source, line, column, &site);
                            assert!(
                                output.incomplete.iter().any(|record| {
                                    record.id == surface
                                        && line_index.line_col(record.span.start).line == line
                                        && line_index.line_col(record.span.start).column == column
                                }),
                                "residual {id:?} witness site {site:?} is absent from raw output"
                            );
                        }
                        ResidualWitness::Marker(marker_id) => {
                            let marker =
                                marker_by_id.get(marker_id.as_str()).unwrap_or_else(|| {
                                    panic!(
                                        "residual {id:?} references unknown marker {marker_id:?}"
                                    )
                                });
                            assert_eq!(
                                marker.owner.as_deref(),
                                Some(owner),
                                "residual {id:?} marker owner must equal the residual owner"
                            );
                        }
                    }
                }
            }
            "diagnostic" => {
                assert_eq!(
                    count,
                    sites.len(),
                    "diagnostic residual {id:?} count must equal witnesses length"
                );
                let code = string(residual, "code", id);
                assert_tk_code(code, &format!("residual {id}.code"));
                for site in sites {
                    let ResidualWitness::Lib { line, column } = parse_residual_witness(&site)
                    else {
                        panic!("diagnostic residual {id:?} must use lib.es5.d.ts:L:C witnesses")
                    };
                    assert_source_position(source, line, column, &site);
                    *expected_diagnostic_sites
                        .entry(DiagnosticSite {
                            code: code.to_string(),
                            line,
                            column,
                        })
                        .or_default() += 1;
                }
            }
            other => panic!("unsupported residual outcome {other:?}"),
        }
    }
    assert_eq!(
        incomplete_counts, expected_incomplete,
        "raw incomplete residual groups drifted"
    );
    let mut actual_diagnostic_sites = BTreeMap::<DiagnosticSite, usize>::new();
    for diagnostic in &output.diagnostics {
        let pos = line_index.line_col(diagnostic.span.start);
        *actual_diagnostic_sites
            .entry(DiagnosticSite {
                code: diagnostic.code.as_str().to_string(),
                line: pos.line,
                column: pos.column,
            })
            .or_default() += 1;
    }
    assert_eq!(
        actual_diagnostic_sites, expected_diagnostic_sites,
        "raw diagnostic residual witnesses drifted in code, line, column, or multiplicity"
    );

    let mut tk2430_by_line = BTreeMap::<u32, usize>::new();
    for diagnostic in &output.diagnostics {
        assert_eq!(
            diagnostic.code.as_str(),
            "TK2430",
            "raw output contains an undeclared diagnostic code"
        );
        *tk2430_by_line
            .entry(line_index.line_of(diagnostic.span.start))
            .or_default() += 1;
    }
    assert_eq!(
        tk2430_by_line,
        BTreeMap::from([(329, 2), (366, 2)]),
        "TK2430 multiplicity must remain visible without deduplication or suppression"
    );
}

fn validate_synthetic_measurement(
    root: &toml::Table,
    artifact: &str,
    witnesses: &str,
    synthetic: &str,
    markers: &[Marker],
    raw: &CheckOutput,
    output: &CheckOutput,
) {
    let measurement = child_table(
        child_table(root, "measurements", "manifest root"),
        "synthetic_typokat",
        "measurements",
    );
    assert_eq!(
        output.parse_errors.len(),
        usize_integer(measurement, "parse_errors", "synthetic measurement")
    );
    assert_eq!(
        output.diagnostics.len(),
        usize_integer(measurement, "diagnostics", "synthetic measurement")
    );
    assert_eq!(
        output.incomplete.len(),
        usize_integer(measurement, "incompletes", "synthetic measurement")
    );
    assert_eq!(
        integer(measurement, "exit_code", "synthetic measurement"),
        3
    );

    assert_eq!(
        &output.diagnostics[..raw.diagnostics.len()],
        raw.diagnostics.as_slice(),
        "synthetic checking must preserve the raw diagnostic prefix exactly"
    );
    assert_eq!(
        output.incomplete, raw.incomplete,
        "the witness suffix must add no incompletes or alter raw incomplete structures"
    );
    assert_eq!(
        output.parse_errors, raw.parse_errors,
        "the witness suffix must parse cleanly"
    );

    let suffix = &output.diagnostics[raw.diagnostics.len()..];
    assert_eq!(
        suffix.len(),
        markers.len(),
        "each marker must produce exactly one suffix diagnostic"
    );
    let expected: Vec<SpanSnapshot> = markers
        .iter()
        .map(|marker| expected_marker_span(artifact, witnesses, synthetic, marker))
        .collect();
    let combined_index = LineIndex::new(synthetic);
    let actual: Vec<SpanSnapshot> = suffix
        .iter()
        .map(|diagnostic| {
            assert!(
                diagnostic.is_error(),
                "synthetic suffix diagnostics must be errors"
            );
            let pos = combined_index.line_col(diagnostic.span.start);
            SpanSnapshot {
                line: pos.line,
                column: pos.column,
                code: diagnostic.code.as_str().to_string(),
                start: diagnostic.span.start,
                end: diagnostic.span.end,
                text: synthetic
                    .get(diagnostic.span.range())
                    .unwrap_or_else(|| {
                        panic!(
                            "diagnostic span {:?} is outside synthetic source",
                            diagnostic.span
                        )
                    })
                    .to_string(),
            }
        })
        .collect();
    assert_eq!(
        actual, expected,
        "synthetic suffix diagnostic order, locations, codes, spans, or texts drifted"
    );

    let suffix_counts = code_counts(suffix);
    assert_eq!(
        suffix_counts,
        BTreeMap::from([("TK2322".to_string(), 66)]),
        "the witness suffix must remain exactly 66 TK2322 with no TK2304"
    );
    assert_eq!(
        suffix_counts.get("TK2304").copied().unwrap_or(0),
        usize_integer(measurement, "tk2304", "synthetic measurement")
    );
    assert_eq!(
        suffix_counts.get("TK2322").copied(),
        Some(usize_integer(
            measurement,
            "tk2322",
            "synthetic measurement"
        ))
    );
    assert_eq!(
        code_counts(&output.diagnostics).get("TK2430").copied(),
        Some(usize_integer(
            measurement,
            "tk2430",
            "synthetic measurement"
        ))
    );
    assert_eq!(
        raw.diagnostics.len(),
        usize_integer(measurement, "artifact_diagnostics", "synthetic measurement")
    );
    assert_eq!(
        suffix.len(),
        usize_integer(measurement, "witness_diagnostics", "synthetic measurement")
    );
}

fn run_one(name: &str, source: String) -> CheckOutput {
    let mut reports = check_project(vec![FileInput {
        name: name.to_string(),
        source,
    }]);
    assert_eq!(
        reports.len(),
        1,
        "one FileInput must produce one FileReport"
    );
    reports.remove(0).output
}

fn declaration_name(line: &str, prefix: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(prefix)?;
    let name: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
        .collect();
    (!name.is_empty()).then_some(name)
}

fn validate_pair_marker_semantics(source: &str, markers: &[Marker]) {
    let lines: Vec<&str> = source.lines().collect();
    for marker in markers
        .iter()
        .filter(|marker| marker.id.starts_with("pair."))
    {
        let rest = marker
            .id
            .strip_prefix("pair.")
            .expect("filtered pair marker has prefix");
        let (name, kind) = rest
            .rsplit_once('.')
            .unwrap_or_else(|| panic!("pair marker {:?} must end in .type or .value", marker.id));
        let line_index =
            usize::try_from(marker.witness_line - 1).expect("witness line index fits usize");
        let code = lines[line_index]
            .split_once("// witness[")
            .map(|(code, _)| code)
            .expect("marker line contains marker comment");
        let rhs = assignment_rhs(code, &marker.id);
        match kind {
            "value" => assert_eq!(rhs, name, "{} must target the RHS value {name}", marker.id),
            "type" => {
                let declaration = lines
                    .get(line_index.wrapping_sub(1))
                    .unwrap_or_else(|| panic!("{} requires its preceding declaration", marker.id));
                let declared = declaration
                    .trim()
                    .strip_prefix("declare const ")
                    .and_then(|rest| rest.strip_suffix(';'))
                    .unwrap_or_else(|| {
                        panic!("{} preceding line must be a declare const", marker.id)
                    });
                let (binding, ty) = declared
                    .split_once(':')
                    .unwrap_or_else(|| panic!("{} declaration requires a type", marker.id));
                assert_eq!(
                    rhs,
                    binding.trim(),
                    "{} must target its declared source",
                    marker.id
                );
                let type_head: String = ty
                    .trim()
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .collect();
                assert_eq!(
                    type_head, name,
                    "{} must exercise the {name} type",
                    marker.id
                );
            }
            other => panic!("pair marker {:?} has unsupported kind {other:?}", marker.id),
        }
    }
}

fn assignment_rhs<'a>(code: &'a str, context: &str) -> &'a str {
    code.split_once('=')
        .map(|(_, rhs)| rhs.trim().trim_end_matches(';').trim())
        .unwrap_or_else(|| panic!("{context} requires an assignment"))
}

fn expected_marker_span(
    artifact: &str,
    witnesses: &str,
    synthetic: &str,
    marker: &Marker,
) -> SpanSnapshot {
    let witness_line_start = line_start_offset(witnesses, marker.witness_line);
    let line = witnesses
        .lines()
        .nth(usize::try_from(marker.witness_line - 1).expect("witness line fits usize"))
        .unwrap_or_else(|| panic!("marker {:?} line is outside witness source", marker.id));
    let code = line
        .split_once("// witness[")
        .map(|(code, _)| code)
        .expect("marker line contains marker comment");
    let (local_start, local_end) = match marker.typokat.as_str() {
        "TK2322" if marker.id == "deep.Intl.type" => {
            let start = code
                .rfind('.')
                .map(|offset| offset + 1)
                .expect("deep.Intl.type requires a member access");
            let end = code
                .rfind(';')
                .expect("deep.Intl.type requires a trailing semicolon");
            (start, end)
        }
        "TK2322" => {
            let mut start = code
                .find('=')
                .map(|offset| offset + 1)
                .unwrap_or_else(|| panic!("marker {:?} requires an assignment", marker.id));
            let mut end = code
                .rfind(';')
                .unwrap_or_else(|| panic!("marker {:?} requires a trailing semicolon", marker.id));
            let bytes = code.as_bytes();
            while start < end && bytes[start].is_ascii_whitespace() {
                start += 1;
            }
            while end > start && bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
            (start, end)
        }
        code => panic!(
            "marker {:?} has unsupported expected code {code:?}",
            marker.id
        ),
    };
    let start = artifact.len() + 1 + witness_line_start + local_start;
    let end = artifact.len() + 1 + witness_line_start + local_end;
    let start_u32 = u32::try_from(start).expect("synthetic marker start fits u32");
    let end_u32 = u32::try_from(end).expect("synthetic marker end fits u32");
    let pos = LineIndex::new(synthetic).line_col(start_u32);
    SpanSnapshot {
        line: pos.line,
        column: pos.column,
        code: marker.typokat.clone(),
        start: start_u32,
        end: end_u32,
        text: synthetic[start..end].to_string(),
    }
}

fn line_start_offset(source: &str, line: u32) -> usize {
    assert!(line > 0, "line numbers are one-based");
    source
        .split_inclusive('\n')
        .take(usize::try_from(line - 1).expect("line count fits usize"))
        .map(str::len)
        .sum()
}

fn parse_marker(comment: &str, witness_line: u32) -> Marker {
    let tokens: Vec<&str> = comment.split_whitespace().collect();
    assert!(
        tokens.len() == 4 || tokens.len() == 5,
        "line {witness_line} has malformed witness marker {comment:?}"
    );
    assert_eq!(
        tokens[0], "//",
        "line {witness_line} must use the strict comment marker grammar"
    );
    let id = bracket_token(tokens[1], "witness", witness_line);
    let oracle = bracket_token(tokens[2], "oracle", witness_line);
    let typokat = bracket_token(tokens[3], "typokat", witness_line);
    let owner = tokens
        .get(4)
        .map(|token| bracket_token(token, "owner", witness_line));
    Marker {
        id,
        oracle,
        typokat,
        owner,
        witness_line,
    }
}

fn bracket_token(token: &str, label: &str, line: u32) -> String {
    let prefix = format!("{label}[");
    let value = token
        .strip_prefix(&prefix)
        .and_then(|rest| rest.strip_suffix(']'))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("line {line} requires {label}[...] in strict marker grammar"))
        .to_string();
    if label == "oracle" || label == "typokat" {
        let expected_prefix = if label == "oracle" { "TS" } else { "TK" };
        assert!(
            value.strip_prefix(expected_prefix).is_some_and(|digits| {
                digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit())
            }),
            "line {line} has malformed {label} code {value:?}"
        );
    }
    value
}

fn same_code(oracle: &str, typokat: &str) -> bool {
    oracle.strip_prefix("TS") == typokat.strip_prefix("TK")
}

fn manifest_witness_records(root: &toml::Table) -> BTreeMap<String, WitnessRecord> {
    child_table(root, "witness", "manifest root")
        .iter()
        .map(|(id, value)| {
            let record = table(value, &format!("witness.{id}"));
            (
                id.clone(),
                WitnessRecord {
                    oracle: string(record, "oracle", id).to_string(),
                    typokat: string(record, "typokat", id).to_string(),
                    owner: record
                        .get("owner")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                },
            )
        })
        .collect()
}

fn verify_blob(bytes: &[u8], record: &toml::Table, label: &str) {
    verify_named_blob(bytes, record, "sha256", "bytes", "lines", label);
}

fn verify_fixed_file(
    path: &Path,
    expected_sha: &str,
    expected_bytes: usize,
    expected_lfs: usize,
    label: &str,
) {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    verify_fixed_blob(&bytes, expected_sha, expected_bytes, expected_lfs, label);
}

fn verify_fixed_blob(
    bytes: &[u8],
    expected_sha: &str,
    expected_bytes: usize,
    expected_lfs: usize,
    label: &str,
) {
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        expected_sha,
        "{label} SHA-256 drifted"
    );
    assert_eq!(bytes.len(), expected_bytes, "{label} byte count drifted");
    assert_eq!(lf_count(bytes), expected_lfs, "{label} LF count drifted");
}

fn verify_named_blob(
    bytes: &[u8],
    record: &toml::Table,
    sha_key: &str,
    bytes_key: &str,
    lines_key: &str,
    label: &str,
) {
    let digest = format!("{:x}", Sha256::digest(bytes));
    assert_eq!(
        digest,
        string(record, sha_key, label),
        "{label} SHA-256 drifted"
    );
    assert_eq!(
        bytes.len(),
        usize_integer(record, bytes_key, label),
        "{label} byte count drifted"
    );
    assert_eq!(
        lf_count(bytes),
        usize_integer(record, lines_key, label),
        "{label} LF count drifted"
    );
}

fn lf_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}

fn code_counts(diagnostics: &[typokat::diagnostics::Diagnostic]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for diagnostic in diagnostics {
        *counts
            .entry(diagnostic.code.as_str().to_string())
            .or_default() += 1;
    }
    counts
}

fn raw_output_fingerprint(output: &CheckOutput) -> String {
    let mut hasher = Sha256::new();
    for diagnostic in &output.diagnostics {
        fingerprint_field(&mut hasher, b"D");
        fingerprint_field(&mut hasher, diagnostic.code.as_str().as_bytes());
        fingerprint_field(
            &mut hasher,
            if diagnostic.is_error() {
                b"error"
            } else {
                b"non-error"
            },
        );
        fingerprint_field(&mut hasher, diagnostic.span.start.to_string().as_bytes());
        fingerprint_field(&mut hasher, diagnostic.span.end.to_string().as_bytes());
        fingerprint_field(&mut hasher, diagnostic.rendered_text().as_bytes());
    }
    for incomplete in &output.incomplete {
        fingerprint_field(&mut hasher, b"I");
        fingerprint_field(&mut hasher, incomplete.id.as_bytes());
        fingerprint_field(&mut hasher, incomplete.span.start.to_string().as_bytes());
        fingerprint_field(&mut hasher, incomplete.span.end.to_string().as_bytes());
        fingerprint_field(&mut hasher, incomplete.context.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn fingerprint_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(bytes.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(bytes);
    hasher.update(b"\n");
}

fn residual_sites(record: &toml::Table, context: &str) -> Vec<String> {
    if let Some(value) = record.get("witness") {
        return vec![value
            .as_str()
            .unwrap_or_else(|| panic!("{context}.witness must be a string"))
            .to_string()];
    }
    string_array(record, "witnesses", context)
}

fn parse_residual_witness(site: &str) -> ResidualWitness {
    if let Some(rest) = site.strip_prefix("lib.es5.d.ts:") {
        let mut parts = rest.split(':');
        let line = parts
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or_else(|| panic!("invalid residual witness line in {site:?}"));
        let column = parts
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or_else(|| panic!("invalid residual witness column in {site:?}"));
        assert!(
            parts.next().is_none(),
            "residual witness {site:?} has extra fields"
        );
        ResidualWitness::Lib { line, column }
    } else {
        assert!(
            !site.is_empty()
                && site
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
            "residual witness {site:?} must be lib.es5.d.ts:L:C or a marker id"
        );
        ResidualWitness::Marker(site.to_string())
    }
}

fn assert_source_position(source: &str, line: u32, column: u32, context: &str) {
    let text = source
        .lines()
        .nth(usize::try_from(line - 1).expect("source line fits usize"))
        .unwrap_or_else(|| panic!("{context} line {line} is outside the artifact"));
    assert!(
        usize::try_from(column - 1).expect("column fits usize") < text.len(),
        "{context} column {column} is outside line {line}"
    );
}

fn assert_tk_code(code: &str, context: &str) {
    assert!(
        code.strip_prefix("TK").is_some_and(|digits| {
            digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit())
        }),
        "{context} must be TK followed by four digits"
    );
}

fn assert_safe_repo_path(
    repo_root: &Path,
    base: &Path,
    value: &str,
    expected: &str,
    context: &str,
) {
    assert_eq!(value, expected, "{context} mapping drifted");
    let relative = Path::new(value);
    assert!(!relative.is_absolute(), "{context} must not be absolute");
    let path = base.join(relative);
    assert!(
        path.is_file(),
        "{context} path does not exist: {}",
        path.display()
    );
    let canonical_root = repo_root
        .canonicalize()
        .unwrap_or_else(|error| panic!("cannot canonicalize repo root: {error}"));
    let canonical_path = path
        .canonicalize()
        .unwrap_or_else(|error| panic!("cannot canonicalize {context}: {error}"));
    assert!(
        canonical_path.starts_with(canonical_root),
        "{context} must not traverse outside the repository"
    );
}

fn read_utf8(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {} as UTF-8: {error}", path.display()))
}

fn assert_sorted_unique(values: &[String], context: &str) {
    let mut sorted = values.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(values, sorted, "{context} must be sorted and unique");
}

fn exact_keys(table: &toml::Table, expected: &[&str], context: &str) {
    let actual: BTreeSet<&str> = table.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    assert_eq!(actual, expected, "{context} keys drifted");
}

fn exact_key_words(table: &toml::Table, expected: &str, context: &str) {
    let words: Vec<&str> = expected.split_whitespace().collect();
    exact_keys(table, &words, context);
}

fn table<'a>(value: &'a Value, context: &str) -> &'a toml::Table {
    value
        .as_table()
        .unwrap_or_else(|| panic!("{context} must be a TOML table"))
}

fn child_table<'a>(parent: &'a toml::Table, key: &str, context: &str) -> &'a toml::Table {
    table(
        parent
            .get(key)
            .unwrap_or_else(|| panic!("{context}.{key} is required")),
        &format!("{context}.{key}"),
    )
}

fn array<'a>(parent: &'a toml::Table, key: &str, context: &str) -> &'a Vec<Value> {
    parent
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{context}.{key} must be an array"))
}

fn string<'a>(parent: &'a toml::Table, key: &str, context: &str) -> &'a str {
    parent
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{context}.{key} must be a string"))
}

fn integer(parent: &toml::Table, key: &str, context: &str) -> i64 {
    parent
        .get(key)
        .and_then(Value::as_integer)
        .unwrap_or_else(|| panic!("{context}.{key} must be an integer"))
}

fn usize_integer(parent: &toml::Table, key: &str, context: &str) -> usize {
    usize::try_from(integer(parent, key, context))
        .unwrap_or_else(|_| panic!("{context}.{key} must be a non-negative usize"))
}

fn string_array(parent: &toml::Table, key: &str, context: &str) -> Vec<String> {
    array(parent, key, context)
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("{context}.{key} entries must be strings"))
                .to_string()
        })
        .collect()
}

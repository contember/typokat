//! WU6 witness for the locked collision rows through the production driver route.

use super::*;
use crate::check::checker::library_compiler::{
    CanonicalLibraryFrontendWorkForTest, CanonicalLibraryFrontendWorkScopeForTest,
    PrivateReplayScaleRouteScopeForTest, PrivateReplayScaleRouteTraceForTest,
    PrivateReplayScaleRunForTest,
};
use sha2::{Digest, Sha256};
use std::fs;

struct ProductionRouteWitness {
    reports: Vec<FileReport>,
    route: PrivateReplayScaleRouteTraceForTest,
    canonical: CanonicalLibraryFrontendWorkForTest,
}

#[derive(Clone)]
struct LockedWorkloadEntry {
    path: String,
    bytes: usize,
    digest: [u8; 32],
}

fn lowercase_hex_nibble(byte: u8) -> Result<u8, &'static str> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err("WU6 workload digest is not lowercase hexadecimal"),
    }
}

fn parse_lowercase_sha256(value: &str) -> Result<[u8; 32], &'static str> {
    if value.len() != 64 {
        return Err("WU6 workload digest has the wrong length");
    }
    let mut digest = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = lowercase_hex_nibble(pair[0])?;
        let low = lowercase_hex_nibble(pair[1])?;
        digest[index] = high * 16 + low;
    }
    Ok(digest)
}

fn expected_paths(row: &str) -> Vec<String> {
    match row {
        "collision" => vec![
            "collision/00_augment.ts".to_owned(),
            "collision/99_consume.ts".to_owned(),
        ],
        "fanout" => std::iter::once("fanout/00_global.ts".to_owned())
            .chain((1..=31).map(|ordinal| format!("fanout/{ordinal:02}.ts")))
            .collect(),
        _ => panic!("unknown WU6 workload row"),
    }
}

fn locked_entries(row: &str) -> Vec<LockedWorkloadEntry> {
    let root = typokat_core::test_support::repository_root().join("tooling/full-lib-bench");
    let lock = fs::read_to_string(root.join("workloads.lock")).expect("WU6 workload lock");
    let mut entries = Vec::new();
    for line in lock.lines().skip(1) {
        let mut columns = line.split_whitespace();
        let locked_row = columns.next().expect("WU6 workload row name");
        let ordinal = columns.next().expect("WU6 workload ordinal");
        let path = columns.next().expect("WU6 workload path");
        let bytes = columns.next().expect("WU6 workload byte count");
        let digest = columns.next().expect("WU6 workload digest");
        assert!(columns.next().is_none(), "malformed WU6 workload lock row");
        if locked_row != row {
            continue;
        }
        assert_eq!(ordinal, format!("{:02}", entries.len()));
        entries.push(LockedWorkloadEntry {
            path: path.to_owned(),
            bytes: bytes.parse().expect("WU6 workload byte count is decimal"),
            digest: parse_lowercase_sha256(digest).expect("valid WU6 workload digest"),
        });
    }
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
        expected_paths(row)
    );
    entries
}

fn validate_locked_source(source: &[u8], entry: &LockedWorkloadEntry) -> Result<(), &'static str> {
    if source.len() != entry.bytes {
        return Err("WU6 workload byte count changed");
    }
    let actual = Sha256::digest(source);
    if actual[..] != entry.digest {
        return Err("WU6 workload SHA-256 changed");
    }
    Ok(())
}

fn locked_inputs(row: &str) -> Vec<FileInput> {
    let root = typokat_core::test_support::repository_root().join("tooling/full-lib-bench");
    let mut inputs = Vec::new();
    for entry in locked_entries(row) {
        let source = fs::read_to_string(root.join("workloads").join(&entry.path))
            .expect("locked WU6 workload source");
        validate_locked_source(source.as_bytes(), &entry).expect("locked WU6 workload bytes");
        inputs.push(FileInput {
            name: entry.path,
            source,
        });
    }
    inputs
}

fn run_production_route(inputs: Vec<FileInput>) -> ProductionRouteWitness {
    let base = library_base().expect("production default-library provider");
    let worker_base = Arc::clone(&base);
    let scale_run = PrivateReplayScaleRunForTest::start();
    on_check_worker(&base, move || {
        let route_scope =
            PrivateReplayScaleRouteScopeForTest::start(&scale_run, false, false, false, false)
                .expect("private replay route witness starts inside the production worker");
        let canonical_scope = CanonicalLibraryFrontendWorkScopeForTest::start();
        let reports = check_project_inner(&worker_base, inputs)
            .expect("locked WU6 workload completes through the production driver");
        let canonical = canonical_scope.finish();
        let route = route_scope
            .finish()
            .expect("private replay route witness finishes inside the production worker");
        ProductionRouteWitness {
            reports,
            route,
            canonical,
        }
    })
    .expect("production check worker")
}

fn assert_clean_reports(reports: &[FileReport], expected: usize) {
    assert_eq!(reports.len(), expected);
    for report in reports {
        assert!(report.output.diagnostics.is_empty(), "{}", report.name);
        assert!(report.output.parse_errors.is_empty(), "{}", report.name);
        assert!(report.output.incomplete.is_empty(), "{}", report.name);
    }
}

fn assert_sparse_production_route(witness: &ProductionRouteWitness) {
    assert_eq!(
        witness.route.sparse_replay_invocations, 1,
        "{:#?}",
        witness.route
    );
    assert_eq!(
        witness.route.full_source_fallback_invocations, 0,
        "{:#?}",
        witness.route
    );
    assert!(
        witness.route.sparse_library_source_units > 0,
        "{:#?}",
        witness.route
    );
    assert!(
        witness.route.sparse_library_source_units < 82,
        "{:#?}",
        witness.route
    );
    assert_eq!(
        witness.route.full_base_scan_units, 0,
        "{:#?}",
        witness.route
    );
    assert_eq!(witness.canonical.entries, 0);
    assert_eq!(witness.canonical.parse_units, 0);
    assert_eq!(witness.canonical.bind_batches, 0);
    assert_eq!(witness.canonical.bind_units, 0);
    assert_eq!(witness.canonical.full_source_products_consumed, 0);
    assert_eq!(witness.canonical.checkpoint_products_consumed, 0);
    assert_eq!(
        witness.route.production_route_invocations, 1,
        "{:#?}",
        witness.route
    );
}

#[test]
fn workload_lock_rejects_a_same_length_source_mutation() {
    let root = typokat_core::test_support::repository_root().join("tooling/full-lib-bench");
    let entry = locked_entries("collision")
        .into_iter()
        .next()
        .expect("locked collision entry");
    let mut mutated =
        fs::read(root.join("workloads").join(&entry.path)).expect("locked collision source bytes");
    let first = mutated.first_mut().expect("locked source is non-empty");
    *first = if *first == b'i' { b'I' } else { b'i' };
    assert_eq!(mutated.len(), entry.bytes);
    assert_eq!(
        validate_locked_source(&mutated, &entry),
        Err("WU6 workload SHA-256 changed")
    );
}

#[test]
fn locked_wu6_collision_and_fanout_use_one_sparse_production_driver_route() {
    let collision = run_production_route(locked_inputs("collision"));
    assert_clean_reports(&collision.reports, 2);
    assert_sparse_production_route(&collision);

    let fanout = run_production_route(locked_inputs("fanout"));
    assert_clean_reports(&fanout.reports, 32);
    assert_sparse_production_route(&fanout);
}

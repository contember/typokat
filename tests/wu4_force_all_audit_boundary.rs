use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn project_local_force_all_has_one_semantic_producer() {
    let root = repo_root();
    let project_local = root.join("crates/typokat-check/src/check/checker/project_local");
    let audit_path = project_local.join("force_all_record_audit.rs");
    assert!(
        !audit_path.exists(),
        "force-all must not compile a second semantic evaluator"
    );

    let module = read(&project_local.join("mod.rs"));
    assert!(!module.contains("force_all_record_audit"));

    let entry = read(&project_local.join("entry.rs"));
    assert!(!entry.contains("Interner::with_intrinsics()"));
    assert!(!entry.contains("lower_product_groups("));

    let sources = project_local_sources(&project_local);
    assert_eq!(count(&sources, "Interner::with_intrinsics()"), 1);
    assert_eq!(count(&sources, "fn lower_type("), 1);
}

#[test]
fn ordinary_project_local_coordinator_has_no_test_fault_authority() {
    let coordinator = read(
        &repo_root().join("crates/typokat-check/src/check/checker/project_local/coordinator.rs"),
    );

    assert!(!coordinator.contains("ProjectLocalSemanticFaultForTest"));
    assert!(!coordinator.contains("fault_reached"));
    assert!(!coordinator.contains("partial_publication_scc"));
}

#[test]
fn force_all_record_authority_is_the_adr_0028_production_pin() {
    let root = repo_root();
    let oracle_source = read(
        &root.join("tests/cases/sr_project_local_library_candidate/semantic-demand-oracle.json"),
    );
    let oracle: serde_json::Value =
        serde_json::from_str(&oracle_source).expect("semantic demand oracle");
    assert_eq!(oracle["library_force_all"]["count"], 709);
    assert_eq!(oracle["library_force_all"]["diagnostics"], 322);
    assert_eq!(oracle["library_force_all"]["incompletes"], 387);
    assert!(!oracle_source.contains("\"count\": 558"));

    let named_census = read(&root.join("tests/fixtures/library-owned-records.txt"));
    assert!(
        named_census.starts_with("# typokat production-owned library record census (ADR-0028).\n")
    );
    assert!(named_census.contains("# diagnostics: 322, incompletes: 387, records: 709"));
    assert!(!named_census.contains("ForceAllRecordAudit"));
    assert!(!named_census.contains("ADR-0018"));
    assert!(!named_census.contains("# diagnostics: 267, incompletes: 291, records: 558"));
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn project_local_sources(root: &Path) -> String {
    let mut paths = fs::read_dir(root)
        .expect("project-local source directory")
        .map(|entry| entry.expect("project-local source entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .iter()
        .map(|path| read(path))
        .collect::<Vec<_>>()
        .join("\n")
}

fn count(source: &str, needle: &str) -> usize {
    source.match_indices(needle).count()
}

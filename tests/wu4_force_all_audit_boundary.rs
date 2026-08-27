use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn force_all_record_audit_is_not_a_publication_lowering_mode() {
    let root = repo_root();
    let project_local = root.join("crates/typokat-check/src/check/checker/project_local");
    let audit_path = project_local.join("force_all_record_audit.rs");
    assert!(
        audit_path.is_file(),
        "missing branded force-all audit module"
    );

    let module = read(&project_local.join("mod.rs"));
    assert!(module.contains("mod force_all_record_audit;"));

    let lowerer = read(&project_local.join("lowerer.rs"));
    assert!(!lowerer.contains("force_all: bool"));
    assert!(!lowerer.contains("self.force_all"));

    let entry = read(&project_local.join("entry.rs"));
    assert!(entry.contains("force_all_record_audit"));

    let audit = read(&audit_path);
    for required in [
        "ForceAllRecordAudit",
        "ForceAllRecordProvenance",
        "ForceAllVisitReceipt",
        "Store",
        "Interner",
    ] {
        assert!(
            audit.contains(required),
            "missing audit boundary {required}"
        );
    }
    for forbidden in [
        "LibraryBaseProvider",
        "complete_source",
        "library_compiler",
        "FrozenLibraryBase",
        "replay",
        "semantic-demand-oracle",
        "library-owned-records",
    ] {
        assert!(
            !audit.contains(forbidden),
            "force-all audit reaches forbidden capability {forbidden}"
        );
    }
}

#[test]
fn force_all_record_authority_remains_the_adr_0018_multiset() {
    let root = repo_root();
    let oracle = read(
        &root.join("tests/cases/sr_project_local_library_candidate/semantic-demand-oracle.json"),
    );
    let oracle: serde_json::Value = serde_json::from_str(&oracle).expect("semantic demand oracle");
    assert_eq!(oracle["library_force_all"]["count"], 558);
    assert_eq!(oracle["library_force_all"]["diagnostics"], 267);
    assert_eq!(oracle["library_force_all"]["incompletes"], 291);

    let named_census = read(&root.join("tests/fixtures/library-owned-records.txt"));
    assert!(named_census.contains("# diagnostics: 267, incompletes: 291, records: 558"));
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

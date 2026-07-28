//! Acceptance specification for ADR-0019's workspace split.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MEMBERS: &[&str] = &[
    "typokat-core",
    "typokat-types",
    "typokat-binder",
    "typokat-relate",
    "typokat-diagnostics",
    "typokat-surface",
    "typokat-frontend",
    "typokat-check",
    "typokat-library",
    "typokat-driver",
];

fn repository_root() -> PathBuf {
    let root = std::env::current_dir().expect("test process current directory");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("root Cargo.toml");
    assert!(
        manifest.starts_with("[package]\nname = \"typokat\"\n"),
        "workspace must retain the root typokat package"
    );
    root
}

fn manifest(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    })
}

#[test]
#[ignore = "enabled when ADR-0019's workspace migration lands"]
fn workspace_has_the_accepted_members_and_source_owners() {
    let root = repository_root();
    let root_manifest = manifest(&root.join("Cargo.toml"));
    assert!(root_manifest.contains("[workspace]"));
    assert!(root_manifest.contains("members = [\"crates/*\"]"));

    for member in MEMBERS {
        let member_root = root.join("crates").join(member);
        let member_manifest = manifest(&member_root.join("Cargo.toml"));
        assert!(
            member_manifest.starts_with(&format!("[package]\nname = \"{member}\"\n")),
            "{member} manifest must retain package identity"
        );
        assert!(
            member_root.join("src/lib.rs").is_file(),
            "{member} must have a library root"
        );
    }

    for old_owner in [
        "binder",
        "check",
        "diagnostics",
        "library",
        "relate",
        "types",
    ] {
        assert!(
            !root.join("src").join(old_owner).exists(),
            "src/{old_owner} must move to its workspace owner"
        );
    }
    for old_owner in [
        "class_semantics.rs",
        "driver.rs",
        "source.rs",
        "span.rs",
        "surface.rs",
    ] {
        assert!(
            !root.join("src").join(old_owner).exists(),
            "src/{old_owner} must move to its workspace owner"
        );
    }

    assert!(root.join("src/lib.rs").is_file());
    assert!(root.join("src/main.rs").is_file());
    assert!(root
        .join("crates/typokat-library/src/typescript-6.0.3/profile.toml")
        .is_file());
}

#[test]
#[ignore = "enabled when ADR-0019's workspace migration lands"]
fn workspace_internal_dependencies_only_point_downward() {
    let root = repository_root();
    let ranks = BTreeMap::from([
        ("typokat-core", 0),
        ("typokat-surface", 0),
        ("typokat-types", 1),
        ("typokat-binder", 2),
        ("typokat-relate", 2),
        ("typokat-diagnostics", 3),
        ("typokat-frontend", 3),
        ("typokat-check", 4),
        ("typokat-library", 5),
        ("typokat-driver", 6),
    ]);

    for member in MEMBERS {
        let body = manifest(&root.join("crates").join(member).join("Cargo.toml"));
        let member_rank = ranks[member];
        for dependency in MEMBERS {
            if dependency == member || !body.contains(dependency) {
                continue;
            }
            assert!(
                ranks[dependency] < member_rank,
                "{member} must not depend on same-level or upward crate {dependency}"
            );
        }
    }
}

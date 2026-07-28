//! Acceptance specification for ADR-0019's workspace split.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT_PACKAGE: &str = "typokat";
const MEMBERS: &[&str] = &[
    "typokat-core",
    "typokat-surface",
    "typokat-types",
    "typokat-binder",
    "typokat-relate",
    "typokat-diagnostics",
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

fn cargo_metadata(root: &Path) -> Value {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .current_dir(root)
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse cargo metadata")
}

fn object_string<'a>(object: &'a Value, key: &str) -> &'a str {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("metadata {key} must be a string"))
}

fn package_names_for_ids(metadata: &Value, field: &str) -> BTreeSet<String> {
    let packages = metadata["packages"]
        .as_array()
        .expect("metadata packages array");
    let names_by_id = packages
        .iter()
        .map(|package| (object_string(package, "id"), object_string(package, "name")))
        .collect::<BTreeMap<_, _>>();
    metadata[field]
        .as_array()
        .unwrap_or_else(|| panic!("metadata {field} array"))
        .iter()
        .map(|id| {
            let id = id
                .as_str()
                .unwrap_or_else(|| panic!("metadata {field} package id"));
            names_by_id
                .get(id)
                .unwrap_or_else(|| panic!("{field} id {id} must name a workspace package"))
                .to_string()
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DependencyKind {
    Normal,
    Dev,
    Build,
}

fn internal_edges(
    metadata: &Value,
) -> BTreeMap<String, BTreeMap<DependencyKind, BTreeSet<String>>> {
    let internal = std::iter::once(ROOT_PACKAGE)
        .chain(MEMBERS.iter().copied())
        .collect::<BTreeSet<_>>();
    metadata["packages"]
        .as_array()
        .expect("metadata packages array")
        .iter()
        .filter_map(|package| {
            let name = object_string(package, "name");
            internal.contains(name).then(|| {
                let mut by_kind = BTreeMap::from([
                    (DependencyKind::Normal, BTreeSet::new()),
                    (DependencyKind::Dev, BTreeSet::new()),
                    (DependencyKind::Build, BTreeSet::new()),
                ]);
                for dependency in package["dependencies"]
                    .as_array()
                    .expect("package dependencies array")
                {
                    let dependency_name = object_string(dependency, "name");
                    if !internal.contains(dependency_name) {
                        continue;
                    }
                    let kind = match dependency.get("kind").and_then(Value::as_str) {
                        None => DependencyKind::Normal,
                        Some("dev") => DependencyKind::Dev,
                        Some("build") => DependencyKind::Build,
                        Some(other) => panic!("unexpected Cargo dependency kind {other}"),
                    };
                    by_kind
                        .get_mut(&kind)
                        .expect("dependency kind bucket")
                        .insert(dependency_name.to_string());
                }
                (name.to_string(), by_kind)
            })
        })
        .collect()
}

fn names(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

fn exact_normal_edges() -> BTreeMap<String, BTreeSet<String>> {
    let mut edges = [
        ("typokat-core", names(&[])),
        ("typokat-surface", names(&[])),
        ("typokat-types", names(&[])),
        ("typokat-binder", names(&["typokat-core", "typokat-types"])),
        ("typokat-relate", names(&["typokat-types"])),
        (
            "typokat-diagnostics",
            names(&[
                "typokat-binder",
                "typokat-core",
                "typokat-relate",
                "typokat-types",
            ]),
        ),
        (
            "typokat-frontend",
            names(&["typokat-binder", "typokat-core", "typokat-types"]),
        ),
        (
            "typokat-check",
            names(&[
                "typokat-binder",
                "typokat-core",
                "typokat-diagnostics",
                "typokat-frontend",
                "typokat-relate",
                "typokat-types",
            ]),
        ),
        (
            "typokat-library",
            names(&[
                "typokat-binder",
                "typokat-check",
                "typokat-core",
                "typokat-frontend",
            ]),
        ),
        (
            "typokat-driver",
            names(&[
                "typokat-check",
                "typokat-diagnostics",
                "typokat-frontend",
                "typokat-library",
                "typokat-types",
            ]),
        ),
    ]
    .into_iter()
    .map(|(name, dependencies)| (name.to_string(), dependencies))
    .collect::<BTreeMap<_, _>>();
    edges.insert(ROOT_PACKAGE.to_string(), names(MEMBERS));
    edges
}

fn assert_acyclic(edges: &BTreeMap<String, BTreeSet<String>>) {
    let mut remaining = edges.clone();
    let mut removed = BTreeSet::new();
    while !remaining.is_empty() {
        let leaves = remaining
            .iter()
            .filter(|(_, dependencies)| dependencies.is_subset(&removed))
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        assert!(
            !leaves.is_empty(),
            "workspace member dependency cycle: {remaining:#?}"
        );
        for leaf in leaves {
            remaining.remove(&leaf);
            removed.insert(leaf);
        }
    }
    let mut expected = names(MEMBERS);
    expected.insert(ROOT_PACKAGE.to_string());
    assert_eq!(removed, expected);
}

#[test]
fn workspace_has_the_accepted_members_defaults_and_source_owners() {
    let root = repository_root();
    let metadata = cargo_metadata(&root);
    let expected_packages = names(
        &std::iter::once(ROOT_PACKAGE)
            .chain(MEMBERS.iter().copied())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        package_names_for_ids(&metadata, "workspace_members"),
        expected_packages
    );
    assert_eq!(
        package_names_for_ids(&metadata, "workspace_default_members"),
        expected_packages
    );

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

    let root_sources = std::fs::read_dir(root.join("src"))
        .expect("root src directory")
        .map(|entry| {
            entry
                .expect("root source entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(root_sources, names(&["lib.rs", "main.rs"]));

    assert!(root.join("crates/typokat-core/src/source.rs").is_file());
    assert!(root.join("crates/typokat-core/src/span.rs").is_file());
    assert!(root.join("crates/typokat-surface/src/surface.rs").is_file());
    assert!(root
        .join("crates/typokat-frontend/src/frontend.rs")
        .is_file());
    assert!(root.join("crates/typokat-driver/src/driver.rs").is_file());
    assert!(root
        .join("crates/typokat-library/src/typescript-6.0.3/profile.toml")
        .is_file());
}

#[test]
fn workspace_internal_dependencies_are_exact_downward_and_acyclic() {
    let root = repository_root();
    let metadata = cargo_metadata(&root);
    let edges = internal_edges(&metadata);
    let expected_normal = exact_normal_edges();
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
        (ROOT_PACKAGE, 7),
    ]);

    assert_eq!(edges.keys().cloned().collect::<BTreeSet<_>>(), {
        let mut packages = names(MEMBERS);
        packages.insert(ROOT_PACKAGE.to_string());
        packages
    });

    for package in std::iter::once(ROOT_PACKAGE).chain(MEMBERS.iter().copied()) {
        let by_kind = &edges[package];
        assert_eq!(
            by_kind[&DependencyKind::Normal],
            expected_normal[package],
            "{package} normal internal dependencies"
        );
        assert!(
            by_kind[&DependencyKind::Build].is_empty(),
            "{package} must have no internal build dependencies"
        );
        for dependency in by_kind[&DependencyKind::Normal]
            .iter()
            .chain(&by_kind[&DependencyKind::Dev])
        {
            assert!(
                ranks[dependency.as_str()] < ranks[package],
                "{package} must not depend on same-level or upward crate {dependency}"
            );
        }
    }

    let root_edges = &edges[ROOT_PACKAGE];
    assert_eq!(
        root_edges[&DependencyKind::Normal],
        names(MEMBERS),
        "the root facade must re-export every member"
    );
    assert_eq!(
        root_edges[&DependencyKind::Dev],
        names(&["typokat-core"]),
        "the root owns only its reproducibility test utility"
    );
    assert!(root_edges[&DependencyKind::Build].is_empty());

    let combined = std::iter::once(ROOT_PACKAGE)
        .chain(MEMBERS.iter().copied())
        .map(|package| {
            let by_kind = &edges[package];
            let dependencies = by_kind[&DependencyKind::Normal]
                .union(&by_kind[&DependencyKind::Dev])
                .cloned()
                .collect();
            (package.to_string(), dependencies)
        })
        .collect();
    assert_acyclic(&combined);
}

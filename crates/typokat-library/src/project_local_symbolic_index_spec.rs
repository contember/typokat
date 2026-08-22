use super::project_local_symbolic_index::{
    group_identity, ProjectLocalSymbolicIndex, SymbolicDeclarationForm, SymbolicLoweringRecipe,
    SymbolicSlot, SymbolicTypeForm,
};
use std::process::Command;

const FRESH_PROCESS_OUTPUT: &str = "TYPOKAT_WU2_SYMBOLIC_INDEX_OUTPUT";
const LOCKED_MANIFEST_BYTES: usize = 4_964;
const LOCKED_MANIFEST_SHA256: &str =
    "42f62d578751ae26a5c6e0c27d551b0caeb12fdfed0ba2519b1fbc2256544343";

fn build(order: &[usize]) -> ProjectLocalSymbolicIndex {
    ProjectLocalSymbolicIndex::build(order).expect("locked symbolic index should build")
}

#[test]
fn symbolic_index_uses_the_canonical_manifest_once_per_source() {
    let scope = crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest::start();
    let index = build(&[0, 23]);

    assert_eq!(scope.finish(), 2);
    assert_eq!(index.sources.len(), 2);
    assert_eq!(index.groups.len(), 12);
    assert_eq!(index.dependencies.len(), 19);
    assert_eq!(
        index.owner_sccs,
        [vec!["Array".to_owned(), "ConcatArray".to_owned()]]
    );
    assert_eq!(
        index.publication_sccs,
        [vec![
            "Array".to_owned(),
            "ArrayConstructor".to_owned(),
            "ConcatArray".to_owned(),
        ]]
    );
    assert_eq!(index.global_keys, ["Array", "Reflect"]);

    let array = index
        .groups
        .iter()
        .find(|group| group.name == "Array")
        .expect("Array group");
    assert_eq!(array.slots, [SymbolicSlot::Type, SymbolicSlot::Value]);
    assert_eq!(array.fragments.len(), 2);
    assert!(matches!(
        array.fragments[0].recipe,
        SymbolicLoweringRecipe::TypeFragment {
            form: SymbolicTypeForm::Interface,
            declaration_start: 57_645,
            ..
        }
    ));
    assert!(matches!(
        array.fragments[1].recipe,
        SymbolicLoweringRecipe::Declaration {
            form: SymbolicDeclarationForm::Variable,
            statement_start: 70_878,
            ..
        }
    ));
    let reflect = index
        .groups
        .iter()
        .find(|group| group.name == "Reflect")
        .expect("Reflect group");
    assert_eq!(
        reflect.slots,
        [SymbolicSlot::Namespace, SymbolicSlot::Value]
    );
    assert!(matches!(
        reflect.fragments[0].recipe,
        SymbolicLoweringRecipe::Declaration {
            form: SymbolicDeclarationForm::Namespace,
            statement_start: 814,
            ..
        }
    ));
    assert_eq!(index.sources[1].lexical_member_starts.len(), 16);
}

#[test]
fn symbolic_manifest_is_byte_stable_across_rebuild_and_source_enumeration() {
    let forward = build(&[0, 23]);
    let repeated = build(&[0, 23]);
    let reversed = build(&[23, 0]);
    let forward_bytes = forward.canonical_bytes().expect("canonical bytes");

    assert_eq!(
        forward_bytes,
        repeated.canonical_bytes().expect("repeated bytes")
    );
    assert_eq!(
        forward_bytes,
        reversed.canonical_bytes().expect("reversed bytes")
    );
    assert_eq!(
        forward.digest().expect("forward digest"),
        repeated.digest().expect("repeat digest")
    );
    assert_eq!(
        forward.digest().expect("forward digest"),
        reversed.digest().expect("reverse digest")
    );
    assert_eq!(forward_bytes.len(), LOCKED_MANIFEST_BYTES);
    assert_eq!(
        forward.digest().expect("locked digest"),
        LOCKED_MANIFEST_SHA256
    );
    println!(
        "WU2 symbolic manifest: bytes={} sha256={}",
        forward_bytes.len(),
        forward.digest().expect("receipt digest")
    );
    assert_eq!(
        forward
            .groups
            .iter()
            .map(|group| (&group.name, &group.identity))
            .collect::<Vec<_>>(),
        reversed
            .groups
            .iter()
            .map(|group| (&group.name, &group.identity))
            .collect::<Vec<_>>()
    );
}

#[test]
fn symbolic_manifest_is_byte_stable_in_a_fresh_process() {
    let parent_bytes = build(&[23, 0]).canonical_bytes().expect("parent bytes");
    let output_path = std::env::temp_dir().join(format!(
        "typokat-wu2-symbolic-index-{}.bin",
        std::process::id()
    ));
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("project_local_symbolic_index_spec::fresh_process_symbolic_manifest_probe")
        .arg("--test-threads=1")
        .env(FRESH_PROCESS_OUTPUT, &output_path)
        .status()
        .expect("fresh process should start");
    assert!(status.success());
    let child_bytes = std::fs::read(&output_path).expect("fresh process manifest");
    std::fs::remove_file(&output_path).expect("remove fresh process manifest");
    assert_eq!(parent_bytes, child_bytes);
}

#[test]
#[ignore = "spawned by symbolic_manifest_is_byte_stable_in_a_fresh_process"]
fn fresh_process_symbolic_manifest_probe() {
    let output = std::env::var_os(FRESH_PROCESS_OUTPUT).expect("fresh process output path");
    let bytes = build(&[0, 23])
        .canonical_bytes()
        .expect("fresh process bytes");
    std::fs::write(output, bytes).expect("write fresh process manifest");
}

#[test]
fn symbolic_group_ids_are_derived_from_profile_source_and_group_facts() {
    let index = build(&[0, 23]);
    for group in &index.groups {
        assert_eq!(
            group_identity(
                &index.profile_identity,
                &group.name,
                &group.slots,
                &group.fragments
            )
            .expect("recomputed group identity"),
            group.identity
        );
    }
    let identities = index
        .groups
        .iter()
        .map(|group| group.identity.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identities.len(), index.groups.len());

    let array = index
        .groups
        .iter()
        .find(|group| group.name == "Array")
        .expect("Array group");
    let changed_profile = group_identity(
        "different-profile",
        &array.name,
        &array.slots,
        &array.fragments,
    )
    .expect("changed profile identity");
    let mut changed_source = array.fragments.clone();
    changed_source[0].source_sha256 = "different-source".to_owned();
    let changed_source = group_identity(
        &index.profile_identity,
        &array.name,
        &array.slots,
        &changed_source,
    )
    .expect("changed source identity");
    let changed_group = group_identity(
        &index.profile_identity,
        "DifferentArray",
        &array.slots,
        &array.fragments,
    )
    .expect("changed group identity");
    assert_ne!(array.identity, changed_profile);
    assert_ne!(array.identity, changed_source);
    assert_ne!(array.identity, changed_group);
}

#[test]
fn shared_symbolic_artifact_has_no_universe_local_or_replay_state() {
    let source = include_str!("project_local_symbolic_index.rs");
    let forbidden = [
        "TypeId",
        "TypeParamId",
        "DeclaredRecipeId",
        "Store",
        "Interner",
        "LibraryBinderCheckpoint",
        "CollisionReplayPlan",
        "ReplayOwner",
        "PublishedTypeGroupTerminal",
        "RelationCache",
        "FlowCache",
    ];
    let present = forbidden
        .iter()
        .filter(|name| source.contains(*name))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        present.is_empty(),
        "forbidden symbolic-index content: {present:?}"
    );

    let forbidden_import_roots = ["crate::types", "crate::check", "crate::relate", "oxc_ast::"];
    let imported = forbidden_import_roots
        .iter()
        .filter(|root| source.contains(*root))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        imported.is_empty(),
        "forbidden symbolic-index imports: {imported:?}"
    );
}

//! Checker-side witness for the binder-authenticated declaration inventory cutover.

use super::replay_index::{
    CollisionReplayEventPhase, CollisionReplayOwnerSite, CollisionReplaySiteProvenance, ReplayOwner,
};
use crate::binder::declaration::TypeGroupId;
use crate::source::LibraryFileOrdinal;
use crate::span::Span;
use crate::types::repr::ClassId;

#[test]
fn class_attachment_consumes_inventory_identity_without_a_second_source_lookup() {
    let checker_root = include_str!("mod.rs");
    let attach_class_bindings = checker_root
        .split_once("fn attach_class_bindings_from_inventory<")
        .and_then(|(_, rest)| rest.split_once("\nfn declaration_owner_namespace"))
        .map(|(body, _)| body)
        .expect("inventory class binding attachment path");
    let attach_class_bindings = attach_class_bindings
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();

    assert!(attach_class_bindings.contains("binder.declarations.get(row.declaration)"));
    assert!(attach_class_bindings.contains("exact.kind!=DeclarationKind::Class"));
    assert!(attach_class_bindings.contains("exact.type_group!=Some(row.group)"));
    assert!(attach_class_bindings.contains("value_decl:exact.value_storage"));
    assert!(attach_class_bindings.contains("ClassBinding{declaration:row.declaration"));
    assert!(attach_class_bindings.contains("collision_replay_owner_sites_for("));
    for forbidden_lookup in [
        "walk_type_decls",
        "class_binding_start(",
        ".exact_declaration_at(",
        "exact_type_fragment_at(",
        "value_decl_id(",
        ".resolve_value(",
        ".lookup_local(",
    ] {
        assert!(
            !attach_class_bindings.contains(forbidden_lookup),
            "class attachment re-resolves its inventory identity through {forbidden_lookup}"
        );
    }

    let owner_range = checker_root
        .split_once("fn collision_replay_owner_sites_for(")
        .and_then(|(_, rest)| rest.split_once("\nfn attach_class_bindings_from_inventory"))
        .map(|(body, _)| body)
        .expect("selected replay owner-range lookup");
    assert_eq!(owner_range.matches("partition_point").count(), 2);
    assert!(!owner_range.contains(".iter()"));

    let inventory_source = include_str!("decls/inventory.rs");
    let exact_join = inventory_source
        .split_once("fn build_with_exact_lookup<'binder>(")
        .and_then(|(_, rest)| rest.split_once("\n    pub fn module("))
        .map(|(body, _)| body)
        .expect("inventory exact declaration join");
    let exact_join = exact_join
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(
        exact_join.contains("exact_declaration_at(module,source.binding_span.start,source.kind)")
    );
    assert!(exact_join.contains("exact.kind!=source.kind"));
    assert!(exact_join.contains("exact.site.module!=module"));
    assert!(exact_join.contains("fragment.declaration==exact.id"));
    assert!(exact_join.contains("declaration:exact.id"));
    for forbidden_join in [
        "walk_type_decls",
        "class_binding_start(",
        "exact_type_fragment_at(",
        ".lookup_local(",
        ".resolve_value(",
    ] {
        assert!(
            !exact_join.contains(forbidden_join),
            "inventory join uses a forbidden fallback through {forbidden_join}"
        );
    }
}

#[test]
fn selected_owner_range_excludes_unrelated_owners() {
    let site = |owner, start| CollisionReplayOwnerSite {
        owner,
        file_ordinal: LibraryFileOrdinal::new(0),
        span: Span::new(start, start + 1),
        provenance: CollisionReplaySiteProvenance::Event {
            phase: CollisionReplayEventPhase::Immediate,
        },
    };
    let target = ReplayOwner::Class(ClassId(7));
    let sites = [
        site(ReplayOwner::TypeGroup(TypeGroupId(0)), 0),
        site(ReplayOwner::Class(ClassId(6)), 1),
        site(target, 2),
        site(target, 3),
        site(ReplayOwner::Class(ClassId(8)), 4),
        site(ReplayOwner::GlobalObject, 5),
    ];

    let selected = super::collision_replay_owner_sites_for(&sites, target);
    assert_eq!(selected.len(), 2);
    assert!(selected.iter().all(|site| site.owner == target));
    assert!(
        super::collision_replay_owner_sites_for(&sites, ReplayOwner::Class(ClassId(9))).is_empty()
    );
}

//! Checker-side witness for the binder-owned exact declaration-site index.

#[test]
fn checker_consumes_exact_declaration_rows_without_an_upward_binder_test_edge() {
    let checker_source = include_str!("decls/mod.rs");
    assert!(!checker_source.contains("fn exact_type_fragment_at("));
    assert!(!checker_source.contains("declarations_by_site"));
    let checker_root = include_str!("mod.rs");
    assert!(!checker_root.contains("exact_type_fragment_at"));
    assert!(!checker_root.contains("declarations_by_site"));

    let attach_class_bindings = checker_root
        .split_once("fn attach_class_bindings<")
        .and_then(|(_, rest)| rest.split_once("\nfn "))
        .map(|(body, _)| body)
        .expect("class binding attachment path");
    let attach_class_bindings = attach_class_bindings
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(attach_class_bindings.contains(".exact_declaration_at("));
    assert!(
        attach_class_bindings.contains(".value_storage"),
        "class binding consumes storage from the exact lexical declaration row"
    );
    for forbidden_name_lookup in [
        "value_decl_id(",
        ".resolve_value(",
        ".lookup_local(",
        ".iter(",
        ".find(",
        ".find_map(",
        "letSome(name)=",
        ".map(|id|id.name.as_str())",
    ] {
        assert!(
            !attach_class_bindings.contains(forbidden_name_lookup),
            "class binding re-resolves by name through {forbidden_name_lookup}"
        );
    }

    let visit_bound_type = checker_source
        .split_once("fn visit_bound_type<'ast>(")
        .and_then(|(_, rest)| rest.split_once("\nfn walk_type_decl_namespace"))
        .map(|(body, _)| body)
        .expect("checker bound-type visitor");
    let visit_bound_type = visit_bound_type
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(visit_bound_type.contains(".exact_declaration_at("));
    assert!(!visit_bound_type.contains("type_groups"));
    assert!(!visit_bound_type.contains("source_units"));
    assert!(!visit_bound_type.contains(".iter("));
}

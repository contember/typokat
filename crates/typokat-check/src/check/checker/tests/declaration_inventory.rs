//! GREEN controls for the canonical declaration-inventory sprint.

use super::super::check_project_programs_with_binding_inspector;
use crate::binder::declaration::{
    source_declaration_occurrences, DeclarationKind, TypeFragmentKind,
};
use crate::check::test_support::check_project_with_checker;
use crate::frontend::FileInput;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn bind(source: &str) -> crate::binder::Binder {
    let prelude_allocator = Allocator::default();
    let user_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let user = Parser::new(&user_allocator, source, SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty(), "{:?}", prelude.diagnostics);
    assert!(user.diagnostics.is_empty(), "{:?}", user.diagnostics);
    crate::binder::bind_module_with_prelude(&prelude.program, &user.program)
}

#[test]
fn bound_inventory_keeps_merge_shadow_and_source_only_class_boundaries() {
    let source = r#"
interface Merged { first: string }
interface Merged { second: number }
{ interface Shadowed { left: string } }
{ interface Shadowed { right: number } }
export default class NamedDefault {}
const Expression = class NamedExpression {};
"#;
    let binder = bind(source);
    let groups = binder.type_groups.local_groups().collect::<Vec<_>>();
    assert_eq!(
        groups
            .iter()
            .map(|group| (group.name.as_str(), group.fragments.len()))
            .collect::<Vec<_>>(),
        [("Merged", 2), ("Shadowed", 1), ("Shadowed", 1),]
    );
    assert_eq!(groups[0].fragments[0].scope, groups[0].fragments[1].scope);
    assert_ne!(groups[1].fragments[0].scope, groups[2].fragments[0].scope);
    assert!(groups
        .iter()
        .flat_map(|group| &group.fragments)
        .all(|fragment| fragment.kind == TypeFragmentKind::Interface
            || fragment.kind == TypeFragmentKind::Class));

    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let source_classes = source_declaration_occurrences(&parsed.program)
        .into_iter()
        .filter(|occurrence| occurrence.kind == DeclarationKind::Class)
        .map(|occurrence| &source[occurrence.binding_span.range()])
        .collect::<Vec<_>>();
    assert_eq!(source_classes, ["NamedDefault", "NamedExpression"]);
    assert!(groups.iter().all(|group| group.name != "NamedDefault"));
    assert!(groups.iter().all(|group| group.name != "NamedExpression"));

    let anonymous_source = "export default class {}";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, anonymous_source, SourceType::ts()).parse();
    assert_eq!(
        source_declaration_occurrences(&parsed.program)
            .into_iter()
            .filter(|occurrence| occurrence.kind == DeclarationKind::Class)
            .count(),
        0
    );
    assert!(bind(anonymous_source)
        .type_groups
        .local_groups()
        .next()
        .is_none());
}

#[test]
fn multi_user_project_inventory_is_module_exact_and_ordered() {
    let inputs = vec![
        FileInput {
            name: "/project/first.ts".to_owned(),
            source: "export {}; interface Shape { first: string } const invalid: Shape = { first: 1 };".to_owned(),
        },
        FileInput {
            name: "/project/second.ts".to_owned(),
            source: "export {}; interface Shape { second: number } const invalid: Shape = { second: 'wrong' };".to_owned(),
        },
    ];
    let manifest_scope =
        crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest::start();
    let inventory_scope = crate::check::checker::DeclarationInventoryWorkScopeForTest::start();
    let reports = check_project_with_checker(inputs, |interner, units| {
        check_project_programs_with_binding_inspector(
            interner,
            units,
            |binder, reservations, module_scopes| {
                assert_eq!(module_scopes.len(), 2);
                let groups = binder
                    .type_groups
                    .local_groups()
                    .filter(|group| group.name == "Shape")
                    .collect::<Vec<_>>();
                assert_eq!(groups.len(), 2);
                for (group, module) in groups.iter().zip(module_scopes) {
                    let [fragment] = group.fragments.as_slice() else {
                        panic!("one module contributes one Shape fragment")
                    };
                    assert_eq!(fragment.site.module, *module);
                    assert_eq!(fragment.kind, TypeFragmentKind::Interface);
                    assert!(reservations
                        .declaration_owner(fragment.declaration)
                        .is_some());
                }
            },
        )
    });
    let manifests = manifest_scope.finish();
    let inventory_work = inventory_scope.finish();
    assert_eq!(manifests, 3);
    assert_eq!(inventory_work.builds, manifests);
    assert_eq!(
        inventory_work,
        crate::check::checker::DeclarationInventoryWorkForTest {
            builds: 3,
            source_type_rows: 20,
            admitted: 20,
            rejected: 0,
            exact_join_attempts: 20,
            dense_slots: 23,
            legacy_top_level_block_deltas: 0,
        }
    );
    assert_eq!(reports.len(), 2);
    for report in reports {
        assert!(report.output.parse_errors.is_empty());
        assert!(report.output.incomplete.is_empty());
        assert_eq!(
            report
                .output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            ["TK2322"]
        );
    }
}

#[test]
fn script_block_local_interface_and_alias_are_ready_while_enum_is_explicitly_unavailable() {
    let source = r#"
{
  interface LocalInterface { value: string }
  const invalidInterface: LocalInterface = { value: 1 };
}
{
  type LocalAlias = { value: string };
  const invalidAlias: LocalAlias = { value: 1 };
}
{
  enum LocalEnum { Ready }
  const ready: LocalEnum = LocalEnum.Ready;
}
"#;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();
    let result = super::super::check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, published, _| {
            for (name, expected) in [
                ("LocalInterface", "ready"),
                ("LocalAlias", "ready"),
                ("LocalEnum", "unavailable"),
            ] {
                let groups = binder
                    .type_groups
                    .local_groups()
                    .filter(|group| group.name == name)
                    .collect::<Vec<_>>();
                let [group] = groups.as_slice() else {
                    panic!("one exact final group for {name}")
                };
                let actual = match published.groups().get(group.id) {
                    Some(super::super::type_groups::PublishedTypeGroupTerminal::Ready(_)) => {
                        "ready"
                    }
                    Some(super::super::type_groups::PublishedTypeGroupTerminal::Unavailable(_)) => {
                        "unavailable"
                    }
                    None => "missing",
                };
                assert_eq!(actual, expected, "final terminal for {name}");
            }
        },
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2322", "TK2322"]
    );
    assert_eq!(result.incomplete.len(), 1);
    assert_eq!(result.incomplete[0].id, "decl/enum-declaration/self");
}

#[test]
#[ignore = "WU3c deferred GREEN: legacy class attachment checks to completion"]
fn arrow_local_class_attachment_legacy_control_checks_to_completion() {
    let reports = check_project_with_checker(
        vec![FileInput {
            name: "/project/script-local-class.ts".to_owned(),
            source: r#"
const checkLocalClass = () => {
  class LocalClass { value!: string }
  const invalid: LocalClass = { value: 1 };
};
checkLocalClass();
"#
            .to_owned(),
        }],
        super::super::check_project_programs,
    );
    let [report] = reports.as_slice() else {
        panic!("one script-local class report")
    };
    assert_eq!(
        report
            .output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2322"]
    );
    assert!(report.output.incomplete.is_empty());
}

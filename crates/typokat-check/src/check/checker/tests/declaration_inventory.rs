//! GREEN controls for the canonical declaration-inventory sprint.

use super::super::check_project_programs_with_binding_inspector;
use crate::binder::declaration::{
    source_declaration_occurrences, DeclarationKind, TypeFragmentKind,
};
use crate::check::test_support::check_project_with_checker;
use crate::frontend::FileInput;
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

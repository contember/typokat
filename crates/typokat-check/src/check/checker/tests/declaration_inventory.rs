//! GREEN controls for the canonical declaration-inventory sprint.

use super::super::{
    check_project_programs_with_binding_inspector, class_attachment_receipt_for_test,
    ClassAttachmentRowForTest,
};
use crate::binder::declaration::{
    source_declaration_occurrences, DeclarationKind, SourceDeclarationManifest,
    SourceTypeFragmentPayload, TypeFragmentKind,
};
use crate::check::test_support::check_project_with_checker;
use crate::frontend::FileInput;
use crate::source::{ModuleOrdinal, SourceOrdinal, UnitSlot};
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn class_attachment_observations(
    inputs: Vec<FileInput>,
) -> Vec<(Option<String>, ClassAttachmentRowForTest)> {
    let observations = std::cell::RefCell::new(None);
    let reports = check_project_with_checker(inputs, |interner, units| {
        check_project_programs_with_binding_inspector(interner, units, |binder, reservations, _| {
            let rows = class_attachment_receipt_for_test(reservations)
                .rows
                .into_iter()
                .map(|row| {
                    let name = row.group.and_then(|group| {
                        binder
                            .type_groups
                            .get(group)
                            .map(|group| group.name.clone())
                    });
                    (name, row)
                })
                .collect();
            observations.replace(Some(rows));
        })
    });
    assert!(reports
        .iter()
        .all(|report| report.output.parse_errors.is_empty()));
    observations
        .into_inner()
        .expect("project reaches class attachment inspection")
}

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
            raw_probe_attempts: 0,
            fragment_checks: 20,
            completeness_checks: 0,
            dense_slots: 23,
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
fn inventory_class_attachment_covers_nested_declarations_and_rejects_expressions() {
    let source = r#"
export {};
declare const condition: boolean;
class Ordinary<T> { value!: T }
class Host {
  method() {
    class MethodLocal {}
    return MethodLocal;
  }
}
function checkFunction() {
  class FunctionLocal {}
  { class BlockLocal {} }
  switch (condition) { case true: { class SwitchLocal {} break; } }
  while (condition) { class LoopLocal {} break; }
  try { throw 1; } catch { class CatchLocal {} }
}
const checkArrow = () => { class ArrowLocal {}; return ArrowLocal; };
const NamedExpression = class NamedClassExpression {};
const AnonymousExpression = class {};
"#;
    let observations = class_attachment_observations(vec![FileInput {
        name: "/project/class-attachment-matrix.ts".to_owned(),
        source: source.to_owned(),
    }]);
    let attached = observations
        .iter()
        .filter_map(|(name, row)| row.class.map(|_| name.as_deref().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        attached,
        [
            "Ordinary",
            "Host",
            "MethodLocal",
            "FunctionLocal",
            "BlockLocal",
            "SwitchLocal",
            "LoopLocal",
            "CatchLocal",
            "ArrowLocal",
        ]
    );
    assert!(observations.iter().take(9).all(|(_, row)| {
        row.declaration.is_some() && row.group.is_some() && row.class.is_some()
    }));
    assert_eq!(observations[0].1.header_type_params.len(), 1);
    assert!(observations[1..9]
        .iter()
        .all(|(_, row)| row.header_type_params.is_empty()));
    assert_eq!(observations.len(), 11);
    assert!(observations[9..].iter().all(|(name, row)| name.is_none()
        && row.declaration.is_none()
        && row.group.is_none()
        && row.class.is_none()));
}

#[test]
fn inventory_class_attachment_merges_interface_and_class_in_both_orders_once() {
    let observations = class_attachment_observations(vec![FileInput {
        name: "/project/class-interface-orders.ts".to_owned(),
        source: r#"
export {};
interface InterfaceFirst<T> { fromInterface: T }
class InterfaceFirst<T> { fromClass!: T }
class ClassFirst<T> { fromClass!: T }
interface ClassFirst<T> { fromInterface: T }
"#
        .to_owned(),
    }]);
    assert_eq!(observations.len(), 2);
    assert_eq!(
        observations
            .iter()
            .map(|(name, _)| name.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["InterfaceFirst", "ClassFirst"]
    );
    assert!(observations.iter().all(|(_, row)| {
        row.declaration.is_some()
            && row.group.is_some()
            && row.class.is_some()
            && row.header_type_params.len() == 1
    }));
    assert_ne!(observations[0].1.group, observations[1].1.group);
    assert_ne!(observations[0].1.declaration, observations[1].1.declaration);
}

#[test]
fn unvalidated_default_candidates_and_class_expressions_have_no_attachment() {
    let observations = class_attachment_observations(vec![
        FileInput {
            name: "/project/named-default.ts".to_owned(),
            source: r#"
export default class RejectedNamedDefault<T> {}
const NamedExpression = class RejectedNamedExpression {};
const AnonymousExpression = class {};
"#
            .to_owned(),
        },
        FileInput {
            name: "/project/anonymous-default.ts".to_owned(),
            source: "export default class<T> {}".to_owned(),
        },
    ]);
    assert_eq!(observations.len(), 4);
    assert!(observations.iter().all(|(name, row)| {
        name.is_none()
            && row.declaration.is_none()
            && row.group.is_none()
            && row.class.is_none()
            && row.header_type_params.is_empty()
    }));
}

#[test]
fn trusted_prelude_inventory_and_terminal_oracle_is_exact_in_source_order() {
    const EXPECTED_NAMES: [&str; 18] = [
        "Array",
        "Partial",
        "Required",
        "Readonly",
        "Pick",
        "Record",
        "Exclude",
        "Extract",
        "Omit",
        "NonNullable",
        "ReturnType",
        "ThisParameterType",
        "OmitThisParameter",
        "Uppercase",
        "Lowercase",
        "Capitalize",
        "Uncapitalize",
        "ThisType",
    ];
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(
        &prelude_allocator,
        super::super::TEST_AMBIENT_SOURCE,
        SourceType::ts(),
    )
    .parse();
    let user_allocator = Allocator::default();
    let user = Parser::new(&user_allocator, "", SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty(), "{:?}", prelude.diagnostics);
    let manifest = SourceDeclarationManifest::build(&prelude.program);
    assert_eq!(
        manifest
            .type_fragments()
            .iter()
            .map(|fragment| match fragment.payload {
                SourceTypeFragmentPayload::TypeAlias(alias) => alias.id.name.as_str(),
                _ => panic!("trusted prelude contains only alias fragments"),
            })
            .collect::<Vec<_>>(),
        EXPECTED_NAMES
    );
    let binder = crate::binder::bind_module_with_prelude(&prelude.program, &user.program);
    let source = binder
        .source_for_module(binder.prelude_module)
        .expect("trusted prelude has a source key");
    let inventory = super::super::decls::inventory::BoundTypeDeclarationInventory::build(
        &binder,
        binder.prelude_module,
        source,
        manifest.clone(),
        None,
    )
    .expect("trusted prelude inventory joins exactly");
    let receipt = inventory.receipt();
    assert_eq!(receipt.admitted, 18);
    assert_eq!(receipt.rejected, 0);
    assert_eq!(receipt.selected, 18);
    for fragment in manifest.type_fragments() {
        let exact = binder
            .exact_declaration_at(
                binder.prelude_module,
                fragment.binding_span.start,
                fragment.kind,
            )
            .expect("source-order prelude row has an exact declaration");
        let row = inventory
            .declarations()
            .get(exact.id)
            .expect("source-order prelude row is admitted");
        assert_eq!(row.declaration, exact.id);
        assert_eq!(row.group, exact.type_group.expect("prelude alias group"));
    }

    let prelude_ordinal = ModuleOrdinal::new(0);
    let prelude_slot = UnitSlot::new(0);
    let (mut reporting, _) = super::super::reserve_internal_reporting_with_declaration_manifest(
        &prelude.program,
        &manifest,
        prelude_ordinal,
        prelude_slot,
        None,
    );
    super::super::attach_type_decl_owners(
        &mut reporting.lexical_events,
        SourceOrdinal::User(prelude_ordinal),
        &binder,
        binder.prelude_module,
        &super::super::ModuleDeclarationSpans::index(&binder),
    );
    assert!(inventory.declarations().iter().all(|row| reporting
        .lexical_events
        .declaration_owner(row.declaration)
        .is_some()));

    let mut interner = Interner::with_intrinsics();
    let checked = super::super::check_program_with_publication_inspector(
        &mut interner,
        &prelude.program,
        |binder, published, _| {
            let local = binder
                .type_groups
                .local_groups()
                .filter(|group| group.id.0 >= binder.prelude_type_group_count)
                .collect::<Vec<_>>();
            assert_eq!(local.len(), 18);
            assert!(local
                .iter()
                .all(|group| published.groups().get(group.id).is_some()));
        },
    );
    assert!(checked.diagnostics.is_empty());
    let expected_incomplete = super::super::expected_trusted_prelude_incomplete(&inventory)
        .expect("trusted prelude intrinsic aliases are exact");
    assert_eq!(expected_incomplete.len(), 6);
    assert_eq!(checked.incomplete, expected_incomplete);
}

#[test]
fn script_continuation_block_class_inventory_attachment_is_exact() {
    let observations = class_attachment_observations(vec![FileInput {
        name: "/project/script-block-class.ts".to_owned(),
        source: "{ class ScriptBlockLocal<T> { value!: T } }".to_owned(),
    }]);
    let [(_, row)] = observations.as_slice() else {
        panic!("one script-block class reservation")
    };
    assert!(row.declaration.is_some());
    assert!(row.group.is_some());
    assert!(row.class.is_some());
    assert_eq!(row.header_type_params.len(), 1);
}

#[test]
fn arrow_local_class_inventory_attachment_checks_to_completion() {
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

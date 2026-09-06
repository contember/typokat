use super::declaration::SourceDeclarationManifest;
use super::source_references::{
    EdgeRole, LookupSpace, ReferenceCoverage, SourceReferenceManifest, WriteKind,
};
use oxc_allocator::Allocator;
use oxc_ast::AstType;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn project(source: &str, tsx: bool) -> SourceReferenceManifest {
    let allocator = Allocator::default();
    let parsed = Parser::new(
        &allocator,
        source,
        if tsx {
            SourceType::tsx()
        } else {
            SourceType::ts()
        },
    )
    .parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    assert!(manifest.belongs_to(&parsed.program));
    manifest.source_references().clone()
}

fn offset(value: usize) -> u32 {
    u32::try_from(value).expect("fixture offset fits u32")
}

#[test]
fn source_reference_manifest_marker_contract() {
    let source = include_str!("fixtures/source-reference-manifest.ts");
    let facts = project(source, false);
    assert_eq!(facts.coverage(), ReferenceCoverage::StructuralTraversal);
    let actual: Vec<_> = facts
        .references()
        .iter()
        .filter(|reference| reference.name == "producer")
        .collect();
    let mut expected_count = 0;
    for (start, _) in source.match_indices("/* use: ") {
        let marker_end = start + source[start..].find("*/").expect("marker closes");
        let (space, role) = source[start + 8..marker_end]
            .trim()
            .split_once(',')
            .expect("space and role");
        let name_start = marker_end
            + 2
            + source[marker_end + 2..]
                .find("producer")
                .expect("marked name");
        let reference = actual
            .iter()
            .find(|reference| reference.span.start == offset(name_start))
            .expect("every marked occurrence retained");
        assert_eq!(reference.span.end, offset(name_start + "producer".len()));
        let (space, write) = match space {
            "value" => (LookupSpace::Value, WriteKind::Read),
            "type" => (LookupSpace::Type, WriteKind::Read),
            "query" => (LookupSpace::QueryValue, WriteKind::Read),
            "export" => (LookupSpace::Export, WriteKind::Read),
            "write" => (LookupSpace::Value, WriteKind::Write),
            "read-write" => (LookupSpace::Value, WriteKind::ReadWrite),
            other => panic!("unexpected marker {other}"),
        };
        assert_eq!(
            (reference.space, reference.write),
            (space, write),
            "{role} at {name_start}"
        );
        let mut operation = &facts.operations()[reference.operation.index()];
        while operation.edge == EdgeRole::Parenthesized {
            operation = &facts.operations()[operation.parent.expect("parenthesis parent").index()];
        }
        let expected_edge = match role.trim() {
            "copy" => {
                assert!(facts
                    .copy_candidates()
                    .iter()
                    .any(|candidate| candidate.reference == reference.id));
                EdgeRole::Initializer
            }
            "unknown-sink" | "callable-sink" | "initializer" => EdgeRole::Initializer,
            "call" | "call-type-arguments" | "optional-call" => EdgeRole::Callee,
            "argument" => EdgeRole::Argument,
            "assertion" | "satisfies" => EdgeRole::Expression,
            "shorthand" => EdgeRole::Shorthand,
            "computed-key" => EdgeRole::ComputedKey,
            "spread" => EdgeRole::Spread,
            "value-query" => EdgeRole::QueryName,
            "type-reference" => EdgeRole::TypeName,
            "assignment" | "update" | "destructuring" | "loop-target" => EdgeRole::Target,
            "return" => EdgeRole::Return,
            "default" => EdgeRole::Default,
            "local-export" => EdgeRole::ExportLocal,
            other => panic!("unexpected role {other}"),
        };
        if role.trim() == "value-query" && operation.edge == EdgeRole::QualifiedRoot {
            operation = &facts.operations()[operation.parent.expect("qualified parent").index()];
        }
        assert_eq!(operation.edge, expected_edge, "{role} at {name_start}");
        if matches!(
            role.trim(),
            "call" | "call-type-arguments" | "optional-call"
        ) {
            let call = facts.operations()[operation.parent.expect("callee parent").index()]
                .call
                .expect("call facts");
            assert_eq!(
                call.explicit_type_arguments,
                role.trim() == "call-type-arguments"
            );
            assert_eq!(call.optional, role.trim() == "optional-call");
            assert_eq!(call.value_argument_count, 0);
        }
        if matches!(role.trim(), "unknown-sink" | "callable-sink") {
            let declaration = operation.parent.expect("initializer declaration");
            assert!(
                facts.operations()[declaration.index()]
                    .variable
                    .expect("variable facts")
                    .annotated
            );
            let annotation = facts
                .operations()
                .iter()
                .find(|operation| {
                    operation.parent == Some(declaration) && operation.edge == EdgeRole::Annotation
                })
                .expect("annotation edge");
            let expected = if role.trim() == "unknown-sink" {
                AstType::TSUnknownKeyword
            } else {
                AstType::TSFunctionType
            };
            assert!(facts
                .operations()
                .iter()
                .any(|operation| operation.parent == Some(annotation.id)
                    && operation.syntax == expected));
        }
        expected_count += 1;
    }
    assert_eq!(
        actual.len(),
        expected_count,
        "no declaration, key, label, qualifier or exported spelling becomes a read"
    );
    eprintln!(
        "C0 TS fixture: marked={expected_count}, references={}, operations={}, regions={}",
        facts.references().len(),
        facts.operations().len(),
        facts.regions().len()
    );
    assert!(facts
        .references()
        .iter()
        .any(|reference| reference.name == "consume"));
    assert_eq!(
        facts
            .copy_candidates()
            .iter()
            .filter(|candidate| facts.references()[candidate.reference.index()].name == "producer")
            .count(),
        2
    );
    let call_facts: Vec<_> = facts
        .operations()
        .iter()
        .filter_map(|operation| operation.call)
        .collect();
    assert!(call_facts
        .iter()
        .any(|call| call.explicit_type_arguments && call.value_argument_count == 0));
    assert!(call_facts.iter().any(|call| call.optional));
    let returns: Vec<_> = actual
        .iter()
        .filter(|reference| {
            facts.operations()[reference.operation.index()].edge == EdgeRole::Return
        })
        .collect();
    assert!(returns
        .windows(2)
        .any(|pair| pair[0].region != pair[1].region));
}

#[test]
fn source_reference_manifest_tsx_names_are_distinct() {
    let source = include_str!("fixtures/source-reference-manifest.tsx");
    let facts = project(source, true);
    let references: Vec<_> = facts
        .references()
        .iter()
        .filter(|reference| reference.name == "Producer")
        .collect();
    assert_eq!(references.len(), 4);
    let expected: Vec<_> = source
        .match_indices("Producer")
        .skip(1)
        .map(|(start, name)| (offset(start), offset(start + name.len())))
        .collect();
    assert_eq!(
        references
            .iter()
            .map(|reference| (reference.span.start, reference.span.end))
            .collect::<Vec<_>>(),
        expected
    );
    eprintln!(
        "C0 TSX fixture: references={}, operations={}, regions={}",
        facts.references().len(),
        facts.operations().len(),
        facts.regions().len()
    );
    assert_eq!(
        references
            .iter()
            .filter(
                |reference| facts.operations()[reference.operation.index()].edge
                    == EdgeRole::JsxRoot
            )
            .count(),
        2
    );
    assert!(!facts
        .references()
        .iter()
        .any(|reference| reference.name == "Item" || reference.name == "producer"));
    for name in ["Producer", "Item", "producer"] {
        assert!(facts
            .operations()
            .iter()
            .any(|operation| operation.name.as_deref() == Some(name)));
    }
}

#[test]
fn source_reference_manifest_owned_projection_and_empty_receipt() {
    let facts = project("const x = (((other)));", false);
    assert_eq!(facts.references()[0].name, "other");
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "", SourceType::ts()).parse();
    let empty = SourceDeclarationManifest::empty_for_program(&parsed.program);
    assert_eq!(
        empty.source_references().coverage(),
        ReferenceCoverage::Unavailable
    );
    assert_eq!(
        SourceDeclarationManifest::build(&parsed.program)
            .source_references()
            .coverage(),
        ReferenceCoverage::StructuralTraversal
    );
}

#[test]
fn source_reference_manifest_does_not_claim_parser_validity() {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "const broken = ;", SourceType::ts()).parse();
    assert!(!parsed.diagnostics.is_empty());
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    assert_eq!(
        manifest.source_references().coverage(),
        ReferenceCoverage::StructuralTraversal
    );
}

#[test]
fn source_reference_manifest_keeps_generic_names_in_captured_regions() {
    let depth = 64;
    let count = 1024;
    let mut source = String::new();
    for index in 0..depth {
        source.push_str(&format!("function f{index}() {{"));
    }
    for index in 0..count {
        source.push_str(&format!("unmarked{index};"));
    }
    source.push_str(&"}".repeat(depth));
    let facts = project(&source, false);
    assert_eq!(facts.references().len(), count);
    for (index, reference) in facts.references().iter().enumerate() {
        assert_eq!(reference.name, format!("unmarked{index}"));
        assert!(facts.operations()[reference.operation.index()]
            .owners
            .callable
            .is_some());
    }
    let work = facts.work_for_test();
    assert_eq!(work.nodes_entered, work.nodes_left);
    assert_eq!(work.edges_processed + 1, work.nodes_entered);
    assert!(work.rows_validated < 4 * work.nodes_entered);
    eprintln!("C0 captured depth={depth}, names={count}: {work:?}");
}

#[test]
fn source_reference_manifest_copy_syntax_is_exact() {
    let source = "const a = (((p))); let b = p; const c: unknown = p; const d = p, e = p; const {f} = p; const g = (p, p); const h = p(); { const i = p; } const j = p as unknown;";
    let facts = project(source, false);
    assert_eq!(facts.copy_candidates().len(), 1);
    let candidate = &facts.copy_candidates()[0];
    assert_eq!(candidate.binding_span.start, 6);
    assert_eq!(facts.references()[candidate.reference.index()].name, "p");
    assert!(facts
        .operations()
        .iter()
        .any(|operation| operation.syntax == AstType::TSAsExpression));
}

#[test]
fn source_reference_manifest_work_is_bounded_by_actual_nodes_and_edges() {
    for count in [16, 64, 256] {
        let source = format!(
            "const alias = {}p{}; {}{}{}",
            "(".repeat(count),
            ")".repeat(count),
            "{".repeat(count),
            "p;".repeat(count),
            "}".repeat(count)
        );
        let facts = project(&source, false);
        let work = facts.work_for_test();
        eprintln!("C0 work depth={count}: {work:?}");
        assert_eq!(
            work.nodes_entered,
            u64::try_from(facts.operations().len()).expect("node count")
        );
        assert_eq!(work.nodes_entered, work.nodes_left);
        assert_eq!(work.edges_processed + 1, work.nodes_entered);
        assert_eq!(
            work.reference_rows,
            u64::try_from(facts.references().len()).expect("reference count")
        );
        assert_eq!(facts.references().len(), count + 1);
        assert_eq!(facts.copy_candidates().len(), 1);
        assert!(work.nodes_entered < u64::try_from(count * 8 + 20).expect("bound"));
        assert_eq!(
            work.rows_validated,
            u64::try_from(
                2 * facts.operations().len()
                    + facts.references().len()
                    + facts.regions().len()
                    + facts.copy_candidates().len()
                    + facts.boundaries().len()
            )
            .expect("validation work")
        );
    }
}

#[test]
fn source_reference_manifest_is_independent_of_global_census() {
    use super::declaration::{
        source_declaration_occurrences, source_global_binding_census,
        SourceDeclarationManifestBuildScopeForTest,
    };
    use super::namespace::ModuleBindingContext;
    let source = include_str!("fixtures/source-reference-manifest.ts");
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let scope = SourceDeclarationManifestBuildScopeForTest::start();
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    assert_eq!(scope.finish(), 1);
    assert_eq!(
        manifest.declarations(),
        source_declaration_occurrences(&parsed.program)
    );
    for external_module in [false, true] {
        let context = ModuleBindingContext {
            external_module,
            ..ModuleBindingContext::default()
        };
        let (references, census) =
            SourceDeclarationManifest::references_with_global_census_for_test(
                &parsed.program,
                context,
            );
        assert_eq!(&references, manifest.source_references());
        assert_eq!(
            census,
            source_global_binding_census(&parsed.program, context)
        );
        assert_eq!(
            census.source_nodes_visited,
            references.work_for_test().nodes_entered
        );
    }
}

#[test]
fn source_reference_manifest_rejects_invalid_or_missing_projection_edges() {
    use super::source_references::{CoverageHole, SourceReferenceBuilder};
    use oxc_ast::{
        ast::{Expression, Statement},
        AstKind,
    };
    let allocator = Allocator::default();
    let mut parsed = Parser::new(&allocator, "name;", SourceType::ts()).parse();
    {
        let mut builder = SourceReferenceBuilder::default();
        builder.enter(AstKind::Program(&parsed.program));
        let interrupted = builder.finish();
        assert_eq!(interrupted.coverage(), ReferenceCoverage::InvalidProjection);
        assert!(interrupted
            .holes()
            .contains(&CoverageHole::UnbalancedTraversal));
    }
    let Statement::ExpressionStatement(statement) = &parsed.program.body[0] else {
        panic!("expression statement")
    };
    let Expression::Identifier(identifier) = &statement.expression else {
        panic!("identifier expression")
    };
    for remove_reference in [false, true] {
        let mut builder = SourceReferenceBuilder::default();
        for kind in [
            AstKind::Program(&parsed.program),
            AstKind::ExpressionStatement(statement),
            AstKind::IdentifierReference(identifier),
        ] {
            builder.enter(kind);
        }
        for kind in [
            AstKind::IdentifierReference(identifier),
            AstKind::ExpressionStatement(statement),
            AstKind::Program(&parsed.program),
        ] {
            builder.leave(kind);
        }
        if remove_reference {
            builder.remove_last_reference_for_test();
        } else {
            builder.remove_last_edge_for_test();
        }
        let broken = builder.finish();
        assert_eq!(broken.coverage(), ReferenceCoverage::InvalidProjection);
        assert!(broken.holes().contains(&CoverageHole::MissingRowsOrEdges));
    }
    parsed.program.span.end = 0;
    let malformed = SourceDeclarationManifest::build(&parsed.program);
    assert_eq!(
        malformed.source_references().coverage(),
        ReferenceCoverage::InvalidProjection
    );
    assert!(malformed
        .source_references()
        .holes()
        .contains(&CoverageHole::InvalidSpan));
}

#[test]
fn source_reference_manifest_type_arguments_writes_and_boundaries() {
    use super::source_references::SourceBoundaryKind;
    let source = "p<TypeArg>(); type Q = typeof p<QueryArg>; type I = import('m').Remote<ImportArg>; interface I extends Base<HeritageArg> {} class C implements InterfaceArg {} (target!) += value; object.member++; ({ [key]: target = fallback, short = fallback } = source); export { foreign as spelling } from 'm'; export type { Local }; import Alias = Namespace.Member; const g = globalThis; this[key]; (eval)('x');";
    let facts = project(source, false);
    for name in [
        "TypeArg",
        "QueryArg",
        "ImportArg",
        "Base",
        "HeritageArg",
        "InterfaceArg",
    ] {
        assert_eq!(
            facts
                .references()
                .iter()
                .find(|reference| reference.name == name)
                .expect("type reference")
                .space,
            LookupSpace::Type,
            "{name}"
        );
    }
    for name in [
        "Remote",
        "foreign",
        "spelling",
        "Namespace",
        "Member",
        "Alias",
        "member",
    ] {
        assert!(
            !facts
                .references()
                .iter()
                .any(|reference| reference.name == name),
            "{name} is not a local read"
        );
    }
    let targets: Vec<_> = facts
        .references()
        .iter()
        .filter(|reference| reference.name == "target")
        .map(|reference| reference.write)
        .collect();
    assert_eq!(targets, [WriteKind::ReadWrite, WriteKind::Write]);
    for name in ["object", "key", "fallback", "value"] {
        assert!(
            facts
                .references()
                .iter()
                .filter(|reference| reference.name == name)
                .all(|reference| reference.write == WriteKind::Read),
            "{name}"
        );
    }
    for boundary in [
        SourceBoundaryKind::Import,
        SourceBoundaryKind::Export,
        SourceBoundaryKind::ImportQualifier,
        SourceBoundaryKind::GlobalThisSpelling,
        SourceBoundaryKind::This,
        SourceBoundaryKind::Dynamic,
    ] {
        assert!(
            facts.boundaries().iter().any(|row| row.kind == boundary),
            "{boundary:?}"
        );
    }
}

#[test]
fn source_reference_manifest_context_and_anchor_facts_are_syntactic() {
    let source = "const result: unknown = (((p()))); const argument = consume(p()); const asserted = p() as unknown; function f<T>(arg: T = p) { return p; } for (p of p) { p; } switch (p) { case p: p; } class C { @decorate(p) [p]() {} static { p; } } namespace N { p; }";
    let facts = project(source, false);
    assert_eq!(facts.coverage(), ReferenceCoverage::StructuralTraversal);
    let calls: Vec<_> = facts
        .operations()
        .iter()
        .filter(|operation| operation.syntax == AstType::CallExpression)
        .collect();
    let first_context = &facts.operations()[calls[0]
        .result_context
        .expect("initializer context")
        .index()];
    assert!(
        first_context
            .variable
            .expect("variable initializer")
            .annotated
    );
    assert_eq!(
        facts.operations()[calls[2].result_context.expect("argument context").index()].syntax,
        AstType::CallExpression
    );
    assert_eq!(
        facts.operations()[calls[3].result_context.expect("assertion context").index()].syntax,
        AstType::TSAsExpression
    );
    assert!(facts.regions().iter().all(|region| region.exited));
    for syntax in [
        AstType::Function,
        AstType::FormalParameters,
        AstType::TSTypeParameterDeclaration,
        AstType::ForOfStatement,
        AstType::SwitchStatement,
        AstType::Class,
        AstType::StaticBlock,
        AstType::TSModuleDeclaration,
    ] {
        assert!(
            facts
                .regions()
                .iter()
                .any(|region| region.anchor.syntax == syntax),
            "{syntax:?}"
        );
    }
    let p_references: Vec<_> = facts
        .references()
        .iter()
        .filter(|reference| reference.name == "p")
        .collect();
    assert!(p_references
        .iter()
        .any(|reference| facts.operations()[reference.operation.index()]
            .owners
            .callable
            .is_some()));
    assert!(p_references
        .iter()
        .any(|reference| facts.operations()[reference.operation.index()]
            .owners
            .class
            .is_some()));
    assert!(p_references
        .iter()
        .any(|reference| facts.operations()[reference.operation.index()]
            .owners
            .namespace
            .is_some()));
    assert_eq!(
        facts
            .operations()
            .iter()
            .filter(|operation| operation.edge == EdgeRole::ComputedKey)
            .count(),
        1
    );
}

fn assert_c0a_reference_rows(
    source: &str,
    facts: &SourceReferenceManifest,
    expected: &[(&str, &str, LookupSpace, WriteKind)],
) {
    let mut cursor = 0;
    let expected: Vec<_> = expected
        .iter()
        .map(|&(needle, name, space, write)| {
            let start = cursor
                + source[cursor..]
                    .find(needle)
                    .expect("expected reference context")
                + needle.find(name).expect("name in context");
            cursor = start + name.len();
            (
                name.to_owned(),
                (offset(start), offset(cursor)),
                space,
                write,
            )
        })
        .collect();
    let actual: Vec<_> = facts
        .references()
        .iter()
        .map(|reference| {
            (
                reference.name.clone(),
                (reference.span.start, reference.span.end),
                reference.space,
                reference.write,
            )
        })
        .collect();
    assert_eq!(facts.coverage(), ReferenceCoverage::StructuralTraversal);
    assert_eq!(
        actual, expected,
        "every reference, including unrelated names"
    );
}

#[test]
fn source_reference_manifest_c0a_satisfies_target_kinds() {
    use LookupSpace::Value;
    use WriteKind::{Read, ReadWrite, Write};
    let source = include_str!("fixtures/source-reference-write-wrappers.ts");
    let facts = project(source, false);
    assert_c0a_reference_rows(
        source,
        &facts,
        &[
            ("satisfiesWrite", "satisfiesWrite", Value, Write),
            ("rhs", "rhs", Value, Read),
            ("satisfiesUpdate", "satisfiesUpdate", Value, ReadWrite),
            ("satisfiesCompound", "satisfiesCompound", Value, ReadWrite),
            ("rhs", "rhs", Value, Read),
            ("satisfiesDestructure", "satisfiesDestructure", Value, Write),
            ("source", "source", Value, Read),
            ("satisfiesLoop", "satisfiesLoop", Value, Write),
            ("values", "values", Value, Read),
            ("asWrite", "asWrite", Value, Write),
            ("rhs", "rhs", Value, Read),
            ("angleWrite", "angleWrite", Value, Write),
            ("rhs", "rhs", Value, Read),
            ("nonNullWrite", "nonNullWrite", Value, Write),
            ("rhs", "rhs", Value, Read),
            ("receiver", "receiver", Value, Read),
            ("rhs", "rhs", Value, Read),
            ("computedReceiver", "computedReceiver", Value, Read),
            ("computedKey", "computedKey", Value, Read),
            ("rhs", "rhs", Value, Read),
            ("copySource", "copySource", Value, Read),
        ],
    );
    assert!(facts.copy_candidates().is_empty());
    eprintln!(
        "C0a write fixture: {} exact reference rows",
        facts.references().len()
    );
}

#[test]
fn source_reference_manifest_c0a_computed_type_signature_keys() {
    use LookupSpace::{Type, Value};
    use WriteKind::Read;
    let source = include_str!("fixtures/source-reference-type-keys.ts");
    let facts = project(source, false);
    assert_c0a_reference_rows(
        source,
        &facts,
        &[
            ("aliasKey", "aliasKey", Value, Read),
            ("aliasMethod", "aliasMethod", Value, Read),
            ("interfaceKey", "interfaceKey", Value, Read),
            ("interfaceMethod", "interfaceMethod", Value, Read),
            ("uniqueKey", "uniqueKey", Value, Read),
            ("methodKey", "methodKey", Value, Read),
            ("source", "source", Value, Read),
            ("typed", "typed", Value, Read),
            ("nestedKey", "nestedKey", Value, Read),
            ("nestedMethod", "nestedMethod", Value, Read),
            ("returnKey", "returnKey", Value, Read),
            ("returnMethod", "returnMethod", Value, Read),
            ("valueKey", "valueKey", Value, Read),
            ("]: T", "T", Type, Read),
            ("makeKey", "makeKey", Value, Read),
            ("KeyType", "KeyType", Type, Read),
            ("ValueType", "ValueType", Type, Read),
        ],
    );
    for reference in facts.references().iter().filter(|reference| {
        matches!(
            reference.name.as_str(),
            "aliasKey"
                | "aliasMethod"
                | "interfaceKey"
                | "interfaceMethod"
                | "uniqueKey"
                | "methodKey"
                | "nestedKey"
                | "nestedMethod"
                | "returnKey"
                | "returnMethod"
                | "valueKey"
        )
    }) {
        assert_eq!(
            facts.operations()[reference.operation.index()].edge,
            EdgeRole::ComputedKey
        );
    }
    for name in ["staticKey", "staticMethod"] {
        let key = facts
            .operations()
            .iter()
            .find(|operation| operation.name.as_deref() == Some(name))
            .expect("static key operation");
        assert_eq!(key.edge, EdgeRole::StaticKey);
    }
    let make_key = facts
        .references()
        .iter()
        .find(|reference| reference.name == "makeKey")
        .expect("computed call callee");
    let callee = &facts.operations()[make_key.operation.index()];
    assert_eq!(callee.edge, EdgeRole::Callee);
    assert_eq!(
        facts.operations()[callee.parent.expect("call").index()].edge,
        EdgeRole::ComputedKey
    );
    eprintln!(
        "C0a type-key fixture: {} exact reference rows",
        facts.references().len()
    );
}

#[test]
fn source_reference_manifest_c0a_for_optional_slot_collisions() {
    let sources = [
        include_str!("fixtures/source-reference-for-init.ts"),
        include_str!("fixtures/source-reference-for-test.ts"),
        include_str!("fixtures/source-reference-for-update.ts"),
    ];
    let facts = sources.map(|source| project(source, false));
    for (left, right) in [(0, 1), (0, 2), (1, 2)] {
        assert_eq!(
            facts[left]
                .operations()
                .iter()
                .map(|operation| operation.span)
                .collect::<Vec<_>>(),
            facts[right]
                .operations()
                .iter()
                .map(|operation| operation.span)
                .collect::<Vec<_>>()
        );
        eprintln!(
            "C0a padded loop pair {left}/{right}: manifests_equal={}",
            facts[left] == facts[right]
        );
    }
    assert!(
        facts[0] != facts[1] && facts[0] != facts[2] && facts[1] != facts[2],
        "optional initializer/test/update slots must not collapse to equal manifests"
    );
    for ((source, facts), expected_role) in
        sources
            .iter()
            .zip(&facts)
            .zip(["ForInitializer", "ForTest", "ForUpdate"])
    {
        assert_c0a_reference_rows(
            source,
            facts,
            &[
                ("probe", "probe", LookupSpace::Value, WriteKind::Read),
                ("body", "body", LookupSpace::Value, WriteKind::Read),
            ],
        );
        assert_eq!(
            format!(
                "{:?}",
                facts.operations()[facts.references()[0].operation.index()].edge
            ),
            expected_role
        );
        let body = &facts.operations()[facts.references()[1].operation.index()];
        assert_eq!(
            facts.operations()[body.parent.expect("body statement").index()].edge,
            EdgeRole::Body
        );
    }
}

#[test]
fn source_reference_manifest_c0a_for_all_optional_combinations() {
    for init in ["", "init", "let bound = initialValue"] {
        for test in ["", "test"] {
            for update in ["", "update"] {
                let source = format!("for ({init}; {test}; {update}) ;");
                let facts = project(&source, false);
                let loop_node = facts
                    .operations()
                    .iter()
                    .find(|operation| operation.syntax == AstType::ForStatement)
                    .expect("for operation");
                let roles: Vec<_> = facts
                    .operations()
                    .iter()
                    .filter(|operation| operation.parent == Some(loop_node.id))
                    .map(|operation| format!("{:?}", operation.edge))
                    .collect();
                let expected: Vec<_> = [
                    (init, "ForInitializer"),
                    (test, "ForTest"),
                    (update, "ForUpdate"),
                    (";", "Body"),
                ]
                .into_iter()
                .filter(|(slot, _)| !slot.is_empty())
                .map(|(_, role)| role)
                .collect();
                assert_eq!(roles, expected, "{source}");
                let expected_names: Vec<_> = [
                    if init.starts_with("let") {
                        "initialValue"
                    } else {
                        init
                    },
                    test,
                    update,
                ]
                .into_iter()
                .filter(|name| !name.is_empty())
                .collect();
                let expected_rows: Vec<_> = expected_names
                    .iter()
                    .map(|name| (*name, *name, LookupSpace::Value, WriteKind::Read))
                    .collect();
                assert_c0a_reference_rows(&source, &facts, &expected_rows);
                let body = facts
                    .operations()
                    .iter()
                    .find(|operation| {
                        operation.parent == Some(loop_node.id) && operation.edge == EdgeRole::Body
                    })
                    .expect("mandatory body");
                assert_eq!(body.syntax, AstType::EmptyStatement);
                if init.starts_with("let") {
                    let value = &facts.operations()[facts.references()[0].operation.index()];
                    assert_eq!(value.edge, EdgeRole::Initializer);
                    let declaration =
                        &facts.operations()[value.parent.expect("variable declarator").index()];
                    assert_eq!(declaration.syntax, AstType::VariableDeclarator);
                    assert_eq!(
                        facts.operations()
                            [declaration.parent.expect("variable declaration").index()]
                        .syntax,
                        AstType::VariableDeclaration
                    );
                }
            }
        }
    }
}

#[test]
fn source_reference_manifest_c0a_satisfies_stays_opaque_and_annotations_stay_types() {
    use LookupSpace::{Type, Value};
    use WriteKind::Read;
    let source = "const wrapped = (((make() satisfies Annotation))); const notACopy = (copySource satisfies CopyType); function annotated(parameter: ParamType = defaultValue): ReturnType { return parameter; }";
    let facts = project(source, false);
    assert_c0a_reference_rows(
        source,
        &facts,
        &[
            ("make", "make", Value, Read),
            ("Annotation", "Annotation", Type, Read),
            ("copySource", "copySource", Value, Read),
            ("CopyType", "CopyType", Type, Read),
            ("ParamType", "ParamType", Type, Read),
            ("defaultValue", "defaultValue", Value, Read),
            ("ReturnType", "ReturnType", Type, Read),
            ("return parameter", "parameter", Value, Read),
        ],
    );
    assert!(facts.copy_candidates().is_empty());
    let call = facts
        .operations()
        .iter()
        .find(|operation| operation.syntax == AstType::CallExpression)
        .expect("call");
    assert_eq!(
        facts.operations()[call.result_context.expect("satisfies context").index()].syntax,
        AstType::TSSatisfiesExpression
    );
}

#[test]
fn source_reference_manifest_c0a_signature_annotation_and_header_edges() {
    use LookupSpace::{Type, Value};
    use WriteKind::Read;
    let source = "const annotated: { [makeKey<KeyType>()]: PropertyType; [methodKey]<T extends Constraint>(value: ParameterType): ReturnType; staticKey: StaticType; staticMethod(): StaticReturn } = source;";
    let facts = project(source, false);
    assert_c0a_reference_rows(
        source,
        &facts,
        &[
            ("makeKey", "makeKey", Value, Read),
            ("KeyType", "KeyType", Type, Read),
            ("PropertyType", "PropertyType", Type, Read),
            ("methodKey", "methodKey", Value, Read),
            ("Constraint", "Constraint", Type, Read),
            ("ParameterType", "ParameterType", Type, Read),
            ("ReturnType", "ReturnType", Type, Read),
            ("StaticType", "StaticType", Type, Read),
            ("StaticReturn", "StaticReturn", Type, Read),
            ("source;", "source", Value, Read),
        ],
    );
    for signature in facts.operations().iter().filter(|operation| {
        matches!(
            operation.syntax,
            AstType::TSPropertySignature | AstType::TSMethodSignature
        )
    }) {
        for child in facts
            .operations()
            .iter()
            .filter(|operation| operation.parent == Some(signature.id))
        {
            match child.syntax {
                AstType::TSTypeAnnotation => assert_eq!(child.edge, EdgeRole::Annotation),
                AstType::TSTypeParameterDeclaration | AstType::FormalParameters => {
                    assert_eq!(child.edge, EdgeRole::Header)
                }
                AstType::IdentifierName => assert_eq!(child.edge, EdgeRole::StaticKey),
                AstType::CallExpression | AstType::IdentifierReference => {
                    assert_eq!(child.edge, EdgeRole::ComputedKey)
                }
                other => panic!("unexpected signature child {other:?}"),
            }
        }
    }
}

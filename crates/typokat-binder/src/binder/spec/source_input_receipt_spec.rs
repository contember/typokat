use super::bind::ProjectBinderBuilder;
use super::declaration::SourceDeclarationManifest;
use super::namespace::{CompilationUnit, SourceUnitKey};
use super::scope::{ScopeId, ScopeKind};
use super::source_inputs::{InputError, RegionObservation};
use super::Binder;
use oxc_allocator::Allocator;
use oxc_ast::AstType;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::sync::Arc;

fn bind(source: &str) -> Binder {
    let arena = Allocator::default();
    let prelude = Parser::new(&arena, "", SourceType::ts()).parse();
    let parsed = Parser::new(&arena, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let (module, _) = builder.add_module(
        &parsed.program,
        &[],
        CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program),
    );
    builder.finish(module)
}

#[test]
fn consumed_origin_is_allocation_not_ast_or_structural_equality() {
    for text in ["", include_str!("fixtures/source-scope-reference-only.ts")] {
        let arena = Allocator::default();
        let prelude = Parser::new(&arena, "", SourceType::ts()).parse();
        let parsed = Parser::new(&arena, text, SourceType::ts()).parse();
        let equal = Parser::new(&arena, text, SourceType::ts()).parse();
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let cloned = manifest.clone();
        let rebuilt = SourceDeclarationManifest::build(&parsed.program);
        let foreign = SourceDeclarationManifest::build(&equal.program);
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let mut modules = Vec::new();
        for source in [SourceUnitKey(1), SourceUnitKey(2)] {
            let (module, _) = builder.add_module_with_declaration_manifest(
                &parsed.program,
                &manifest,
                &[],
                CompilationUnit::implementation(source, &parsed.program),
            );
            modules.push((module, source));
        }
        let binder = builder.finish(modules[1].0);
        let inputs = binder.source_input_observations();
        for (module, source) in &modules {
            let input = inputs
                .input(*module, *source, &cloned.reference_input())
                .unwrap();
            assert!(input.errors.is_empty(), "{:?}", input.errors);
            assert!(Arc::ptr_eq(&input.facts, &manifest.reference_input()));
            assert_eq!(&*input.facts, rebuilt.source_references());
            for replacement in [&rebuilt, &foreign] {
                assert_eq!(
                    inputs
                        .input(*module, *source, &replacement.reference_input())
                        .unwrap_err(),
                    InputError::ForeignInput,
                );
            }
        }
        assert_eq!(
            inputs
                .input(modules[0].0, modules[1].1, &manifest.reference_input())
                .unwrap_err(),
            InputError::WrongSource,
        );
        assert_eq!(
            inputs
                .input(ScopeId(u32::MAX), modules[0].1, &manifest.reference_input())
                .unwrap_err(),
            InputError::UnknownModule,
        );
    }
}

#[test]
fn dropped_arenas_keep_rows_and_actual_normalized_root() {
    for source in ["", "externalValue;", "function f() { { externalValue; } }"] {
        let binder = bind(source);
        let input = binder
            .source_input_observations()
            .get(binder.module)
            .unwrap();
        assert!(input.errors.is_empty(), "{:?}", input.errors);
        assert_eq!(input.regions.len(), input.facts.regions().len());
        let RegionObservation::Attached(root) = &input.regions[0] else {
            panic!("root is attached")
        };
        assert_eq!(root.scope, binder.module);
        assert_eq!(root.kind, ScopeKind::Module);
        assert_eq!(root.creation_parent, Some(binder.prelude_module));
        assert_eq!(
            input.normalization.unwrap().new_parent,
            Some(binder.script_namespace_root)
        );
        assert_eq!(
            input.facts.references().len(),
            usize::from(!source.is_empty())
        );
        for region in input.facts.regions() {
            if region.anchor.syntax == AstType::FunctionBody {
                assert_eq!(
                    input.regions[region.id.index()],
                    RegionObservation::NoRuntimeScopeHook
                );
            }
        }
    }
}

#[test]
fn fixture_receipts_preserve_every_actual_scope_and_parent() {
    use AstType::*;
    let text = include_str!("fixtures/source-scope-receipts.ts");
    let binder = bind(text);
    let input = binder
        .source_input_observations()
        .get(binder.module)
        .unwrap();
    assert!(input.errors.is_empty(), "{:?}", input.errors);
    let mut actual = Vec::new();
    for (region, observation) in input.facts.regions().iter().zip(&input.regions) {
        let RegionObservation::Attached(attachment) = observation else {
            continue;
        };
        assert_eq!(attachment.anchor, region.anchor);
        assert_eq!(attachment.region, region.id);
        let node = binder.graph.get(attachment.scope).unwrap();
        assert_eq!(node.kind, attachment.kind);
        if attachment.anchor.syntax != Program {
            assert_eq!(node.parent, attachment.creation_parent);
            let scopes = if matches!(attachment.anchor.syntax, Function | ArrowFunctionExpression) {
                &binder.fn_scopes
            } else {
                &binder.block_scopes
            };
            assert_eq!(
                scopes.get(&(binder.module, attachment.anchor.span.start)),
                Some(&attachment.scope)
            );
        }
        actual.push((
            attachment.scope.0,
            attachment.anchor.syntax,
            attachment.creation_parent.map(|id| id.0),
        ));
    }
    actual.sort_by_key(|row| row.0);
    assert_eq!(
        actual,
        vec![
            (3, Program, Some(0)),
            (4, Function, Some(3)),
            (5, Function, Some(3)),
            (6, ArrowFunctionExpression, Some(5)),
            (7, BlockStatement, Some(5)),
            (8, ArrowFunctionExpression, Some(7)),
            (9, Function, Some(7)),
            (10, ForStatement, Some(5)),
            (11, ArrowFunctionExpression, Some(10)),
            (12, ArrowFunctionExpression, Some(10)),
            (13, ArrowFunctionExpression, Some(10)),
            (14, BlockStatement, Some(10)),
            (15, ForOfStatement, Some(5)),
            (16, ArrowFunctionExpression, Some(5)),
            (17, BlockStatement, Some(15)),
            (18, ForInStatement, Some(5)),
            (19, ArrowFunctionExpression, Some(5)),
            (20, BlockStatement, Some(18)),
            (21, ArrowFunctionExpression, Some(5)),
            (22, SwitchStatement, Some(5)),
            (23, ArrowFunctionExpression, Some(22)),
            (24, BlockStatement, Some(22)),
            (25, BlockStatement, Some(5)),
            (26, CatchClause, Some(5)),
            (27, BlockStatement, Some(26)),
            (28, ArrowFunctionExpression, Some(27)),
            (29, BlockStatement, Some(5)),
            (30, Function, Some(3)),
            (31, ArrowFunctionExpression, Some(30)),
            (32, ArrowFunctionExpression, Some(3)),
            (33, ArrowFunctionExpression, Some(32)),
        ]
    );
    assert_eq!(binder.graph.all_scopes().count(), 34);
    for (snippet, scope) in [
        ("() => local", 8),
        ("() => [local]", 16),
        ("() => ({ local })", 19),
        ("() => caught", 28),
    ] {
        let start = u32::try_from(text.find(snippet).unwrap()).unwrap();
        let end = start + u32::try_from(snippet.len()).unwrap();
        assert!(input.regions.iter().any(|observation| matches!(observation,
            RegionObservation::Attached(attachment) if attachment.scope == ScopeId(scope)
                && attachment.anchor.span == crate::span::Span::new(start, end))));
    }
    let arena = Allocator::default();
    let parsed = Parser::new(&arena, text, SourceType::ts()).parse();
    let rebuilt = SourceDeclarationManifest::build(&parsed.program);
    assert_eq!(
        input.facts.references(),
        rebuilt.source_references().references()
    );
    assert_eq!(&*input.facts, rebuilt.source_references());
}

#[test]
fn skipped_grammar_is_missing_not_nearest_scope_and_retains_all_references() {
    let text = include_str!("fixtures/source-scope-holes.ts");
    let binder = bind(text);
    let input = binder
        .source_input_observations()
        .get(binder.module)
        .unwrap();
    assert!(input.errors.is_empty(), "{:?}", input.errors);
    let mut attached = Vec::new();
    let mut missing = Vec::new();
    for (region, observation) in input.facts.regions().iter().zip(&input.regions) {
        let start = usize::try_from(region.anchor.span.start).unwrap();
        let end = usize::try_from(region.anchor.span.end).unwrap();
        match observation {
            RegionObservation::Attached(attachment) if region.anchor.syntax != AstType::Program => {
                attached.push((
                    &text[start..end],
                    attachment.scope.0,
                    attachment.creation_parent,
                ));
            }
            RegionObservation::MissingAttachment => missing.push(&text[start..end]),
            _ => {}
        }
    }
    assert_eq!(
        attached,
        vec![
            ("() => rootValue", 4, Some(binder.module)),
            ("() => rootValue", 5, Some(binder.module)),
            ("() { return rootValue; }", 6, Some(binder.module)),
            ("() { return rootValue; }", 7, Some(binder.module)),
        ]
    );
    assert_eq!(
        missing,
        vec![
            "() => rootValue",
            "() => rootValue",
            "() => key",
            "() => key",
            "() => rootValue",
            "() => key",
            "() => rootValue",
            "() => key",
        ]
    );
    let arena = Allocator::default();
    let parsed = Parser::new(&arena, text, SourceType::ts()).parse();
    assert_eq!(
        &*input.facts,
        SourceDeclarationManifest::build(&parsed.program).source_references()
    );
    assert!(input
        .facts
        .regions()
        .iter()
        .any(|region| region.anchor.syntax == AstType::TSFunctionType
            && input.regions[region.id.index()] == RegionObservation::NoRuntimeScopeHook));
}

#[test]
fn duplicate_install_and_conflicting_exact_receipts_are_typed_invalid() {
    use super::source_inputs::SourceInputs;
    let binder = bind("function f() {} function g() {}");
    let input = binder
        .source_input_observations()
        .get(binder.module)
        .unwrap();
    let mut inputs = SourceInputs::default();
    inputs.install(
        input.module,
        input.source,
        Arc::clone(&input.facts),
        input.module_creation_parent,
        input.origin,
    );
    let region = input
        .facts
        .regions()
        .iter()
        .find(|region| region.anchor.syntax == AstType::Function)
        .unwrap();
    let RegionObservation::Attached(attachment) = input.regions[region.id.index()] else {
        panic!("function attached")
    };
    for _ in 0..2 {
        inputs.attach(
            input.module,
            region.anchor.syntax,
            region.anchor.span,
            attachment.scope,
            &binder.graph,
        );
    }
    assert!(inputs.get(input.module).unwrap().errors.is_empty());
    inputs.attach(
        input.module,
        region.anchor.syntax,
        region.anchor.span,
        ScopeId(5),
        &binder.graph,
    );
    assert_eq!(
        inputs.get(input.module).unwrap().errors,
        vec![InputError::ConflictingAttachment]
    );
    inputs.install(
        input.module,
        SourceUnitKey(99),
        Arc::clone(&input.facts),
        None,
        None,
    );
    let observed = inputs.get(input.module).unwrap();
    assert_eq!(observed.source, input.source);
    assert!(Arc::ptr_eq(&observed.facts, &input.facts));
    assert_eq!(
        observed.errors,
        vec![
            InputError::ConflictingAttachment,
            InputError::DuplicateInstall
        ]
    );
}

#[test]
fn unavailable_and_invalid_c0_never_become_complete_inputs() {
    use super::source_inputs::SourceInputs;
    use super::source_references::SourceReferenceBuilder;
    use oxc_ast::AstKind;
    let arena = Allocator::default();
    let parsed = Parser::new(&arena, "", SourceType::ts()).parse();
    let unavailable = SourceDeclarationManifest::empty_for_program(&parsed.program);
    let mut broken = SourceReferenceBuilder::default();
    broken.enter(AstKind::Program(&parsed.program));
    let mut inputs = SourceInputs::default();
    for (module, facts, error) in [
        (
            ScopeId(0),
            unavailable.reference_input(),
            InputError::Unavailable,
        ),
        (
            ScopeId(1),
            Arc::new(broken.finish()),
            InputError::InvalidProjection,
        ),
    ] {
        inputs.install(module, SourceUnitKey(1), Arc::clone(&facts), None, None);
        assert_eq!(
            inputs.input(module, SourceUnitKey(1), &facts).unwrap_err(),
            error
        );
    }
}

#[test]
fn duplicate_full_anchors_are_invalid_and_span_start_is_not_a_hook_key() {
    use super::source_inputs::SourceInputs;
    use super::source_references::{ReferenceCoverage, SourceReferenceBuilder};
    use oxc_ast::{ast::Statement, AstKind};
    let arena = Allocator::default();
    let parsed = Parser::new(&arena, "function f() {}", SourceType::ts()).parse();
    let Statement::FunctionDeclaration(function) = &parsed.program.body[0] else {
        panic!("function")
    };
    let mut projection = SourceReferenceBuilder::default();
    projection.enter(AstKind::Program(&parsed.program));
    for _ in 0..2 {
        projection.enter(AstKind::Function(function));
        projection.leave(AstKind::Function(function));
    }
    projection.leave(AstKind::Program(&parsed.program));
    let facts = Arc::new(projection.finish());
    assert_eq!(facts.coverage(), ReferenceCoverage::StructuralTraversal);
    let mut inputs = SourceInputs::default();
    inputs.install(ScopeId(3), SourceUnitKey(1), facts, Some(ScopeId(0)), None);
    assert_eq!(
        inputs.get(ScopeId(3)).unwrap().errors,
        vec![InputError::DuplicateAnchor]
    );

    let binder = bind("function f() {}");
    let input = binder
        .source_input_observations()
        .get(binder.module)
        .unwrap();
    let mut inputs = SourceInputs::default();
    inputs.install(
        input.module,
        input.source,
        Arc::clone(&input.facts),
        input.module_creation_parent,
        input.origin,
    );
    let span = crate::span::Span::from_oxc(function.span);
    for (syntax, span) in [
        (
            AstType::Function,
            crate::span::Span::new(span.start, span.end - 1),
        ),
        (AstType::ArrowFunctionExpression, span),
    ] {
        inputs.attach(input.module, syntax, span, ScopeId(4), &binder.graph);
    }
    assert_eq!(
        inputs.get(input.module).unwrap().errors,
        vec![InputError::MissingAnchor; 2]
    );
}

fn library_binder_and_product() -> (Binder, typokat_library_product::LibraryDeclarationProduct) {
    use crate::source::LibraryFileOrdinal;
    let arena = Allocator::default();
    let text = "interface LibraryType { field: string } declare function libraryFn(): void;";
    let parsed = Parser::new(&arena, text, SourceType::d_ts()).parse();
    let prelude = Parser::new(&arena, "", SourceType::d_ts()).parse();
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
    let modules = builder
        .try_add_library_modules_with_declaration_manifests(&[(
            &parsed.program,
            &manifest,
            CompilationUnit::library(
                SourceUnitKey(41),
                LibraryFileOrdinal::new(7),
                &parsed.program,
            ),
        )])
        .unwrap();
    let binder = builder.finish(modules[0]);
    let product = super::product::project_library_declaration_product(
        "source-input-receipt-spec",
        &binder,
        &[super::product::LibraryProductSource {
            ordinal: 7,
            name: "lib.receipt.d.ts",
            source_sha256: "00",
            byte_len: u32::try_from(text.len()).unwrap(),
            external_module: false,
            module: modules[0],
            manifest: &manifest,
            source_text: text,
        }],
        &[],
    )
    .unwrap();
    (binder, product)
}

#[test]
fn actual_roots_differ_for_library_product_and_continuation() {
    let (mut library, product) = library_binder_and_product();
    let input = library
        .source_input_observations()
        .get(library.module)
        .unwrap();
    assert!(input.errors.is_empty(), "{:?}", input.errors);
    let RegionObservation::Attached(root) = input.regions[0] else {
        panic!("library root")
    };
    assert_eq!(root.scope, library.compilation_global);
    assert_ne!(root.scope, library.module);
    assert_eq!(root.creation_parent, Some(library.prelude_module));
    assert_eq!(root.kind, ScopeKind::CompilationGlobal);
    assert_eq!(input.normalization.unwrap().module, library.module);
    assert_eq!(
        input.normalization.unwrap().old_parent,
        Some(library.prelude_module)
    );
    assert_eq!(
        library.source_for_module(library.module),
        Some(SourceUnitKey(41))
    );
    assert_eq!(
        input.origin,
        library
            .namespaces
            .compilation_origin_for_source(input.source)
    );
    assert!(
        matches!(input.origin, Some(crate::source::CompilationOrigin::Library(ordinal)) if ordinal == crate::source::LibraryFileOrdinal::new(7))
    );
    for region in &input.regions {
        if let RegionObservation::Attached(attachment) = region {
            if attachment.kind == ScopeKind::Function {
                assert_eq!(attachment.creation_parent, Some(library.compilation_global));
            }
        }
    }
    let facts = Arc::clone(&input.facts);
    library.freeze_as_base().unwrap();
    assert!(Arc::ptr_eq(
        &facts,
        &library
            .source_input_observations()
            .get(library.module)
            .unwrap()
            .facts
    ));
    let fork = library.fork_delta().unwrap();
    assert!(fork
        .source_input_observations()
        .get(library.module)
        .is_none());
    assert!(fork.shares_base_storage_with(&library));
    assert_eq!(
        fork.source_for_module(library.module),
        Some(SourceUnitKey(41))
    );

    for product_route in [false, true] {
        let (mut builder, library_modules) = if product_route {
            let (builder, units) =
                ProjectBinderBuilder::from_library_declaration_product(&product).unwrap();
            (
                builder,
                units.iter().map(|unit| unit.module).collect::<Vec<_>>(),
            )
        } else {
            let (builder, _) =
                ProjectBinderBuilder::resume_frozen_library(library.fork_delta().unwrap());
            (builder, vec![library.module])
        };
        let arena = Allocator::default();
        let parsed = Parser::new(
            &arena,
            "function userFn() { { libraryFn(); } } externalValue;",
            SourceType::ts(),
        )
        .parse();
        let unit = CompilationUnit::implementation(SourceUnitKey(42), &parsed.program);
        builder.reserve_script_namespace_roots([(&parsed.program, unit)]);
        let (module, _) = builder.add_module(&parsed.program, &[], unit);
        let binder = if product_route {
            builder
                .finish_library_product_project(Some(module))
                .unwrap()
                .0
        } else {
            builder
                .finish_frozen_library_continuation(Some(module))
                .unwrap()
        };
        for module in library_modules {
            assert!(binder.source_for_module(module).is_some());
            assert!(binder.source_input_observations().get(module).is_none());
        }
        let input = binder.source_input_observations().get(module).unwrap();
        assert!(input.errors.is_empty(), "{:?}", input.errors);
        let RegionObservation::Attached(root) = input.regions[0] else {
            panic!("user root")
        };
        assert_eq!(root.scope, module);
        assert_eq!(root.kind, ScopeKind::Module);
        assert_eq!(root.creation_parent, Some(binder.script_namespace_root));
        assert_eq!(
            input.normalization.unwrap().old_parent,
            root.creation_parent
        );
        assert_eq!(
            input.normalization.unwrap().new_parent,
            Some(binder.script_namespace_root)
        );
        for observation in &input.regions {
            if let RegionObservation::Attached(attachment) = observation {
                if attachment.kind == ScopeKind::Function {
                    assert_eq!(attachment.creation_parent, Some(module));
                }
                if attachment.kind == ScopeKind::Block {
                    assert_eq!(
                        binder
                            .graph
                            .get(attachment.creation_parent.unwrap())
                            .unwrap()
                            .kind,
                        ScopeKind::Function
                    );
                }
            }
        }
    }
}

#[test]
fn historical_observations_are_not_reissued_by_freeze_or_resume() {
    for partial_failure in [false, true] {
        let mut binder = bind("function f() { externalValue; }");
        let module = binder.module;
        let input = binder.source_input_observations().get(module).unwrap();
        let facts = Arc::clone(&input.facts);
        let observations = input.regions.clone();
        let transition = input.normalization;
        binder.graph.get_mut(module).unwrap().parent = Some(binder.compilation_global);
        if partial_failure {
            binder.symbols.freeze_as_base().unwrap();
        }
        assert_eq!(binder.freeze_as_base().is_err(), partial_failure);
        let historical = binder.source_input_observations().get(module).unwrap();
        assert!(Arc::ptr_eq(&facts, &historical.facts));
        assert_eq!(historical.regions, observations);
        assert_eq!(historical.normalization, transition);
        if partial_failure {
            assert!(binder.fork_delta().is_err());
        } else {
            assert!(binder
                .fork_delta()
                .unwrap()
                .source_input_observations()
                .get(module)
                .is_none());
            let (builder, _) = ProjectBinderBuilder::resume_frozen_library(binder);
            let continued = builder.finish_frozen_library_continuation(None).unwrap();
            assert!(continued.source_input_observations().get(module).is_none());
        }
    }
}

#[test]
fn normalization_requires_exact_original_source_and_parent_event() {
    use super::source_inputs::SourceInputs;
    let binder = bind("externalValue;");
    let input = binder
        .source_input_observations()
        .get(binder.module)
        .unwrap();
    for wrong_source in [false, true] {
        let mut inputs = SourceInputs::default();
        inputs.install(
            input.module,
            input.source,
            Arc::clone(&input.facts),
            input.module_creation_parent,
            input.origin,
        );
        inputs.normalize_root(
            input.module,
            if wrong_source {
                SourceUnitKey(99)
            } else {
                input.source
            },
            if wrong_source {
                input.module_creation_parent
            } else {
                Some(binder.compilation_global)
            },
            Some(binder.script_namespace_root),
        );
        assert_eq!(
            inputs.get(input.module).unwrap().errors,
            vec![InputError::InvalidNormalization]
        );
        assert!(inputs.get(input.module).unwrap().normalization.is_none());
    }
}

#[test]
fn new_work_is_one_index_and_bounded_hooks_with_no_second_manifest() {
    use super::declaration::SourceDeclarationManifestBuildScopeForTest;
    for n in [8, 32, 96] {
        let siblings = "function f() { externalValue; }".repeat(n);
        let deep = format!("{}externalValue;{}", "{".repeat(n), "}".repeat(n));
        for text in [siblings, deep] {
            let builds = SourceDeclarationManifestBuildScopeForTest::start();
            let binder = bind(&text);
            assert_eq!(builds.finish(), 2);
            let inputs = binder.source_input_observations();
            assert_eq!(
                inputs.work.indexed_operations,
                inputs
                    .iter()
                    .map(|input| input.facts.operations().len())
                    .sum::<usize>()
            );
            assert_eq!(
                inputs.work.indexed_regions,
                inputs
                    .iter()
                    .map(|input| input.facts.regions().len())
                    .sum::<usize>()
            );
            assert_eq!(inputs.work.finalized_regions, inputs.work.indexed_regions);
            assert_eq!(inputs.work.source_rows, 2);
            assert_eq!(inputs.work.hook_probes, n + 2);
            assert!(inputs.work.indexed_operations < 20 * (n + 1));
        }
        let arena = Allocator::default();
        let prelude = Parser::new(&arena, "", SourceType::ts()).parse();
        let parsed =
            Parser::new(&arena, "function f() { externalValue; }", SourceType::ts()).parse();
        let builds = SourceDeclarationManifestBuildScopeForTest::start();
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let mut module = ScopeId(0);
        for index in 1..=n {
            module = builder
                .add_module_with_declaration_manifest(
                    &parsed.program,
                    &manifest,
                    &[],
                    CompilationUnit::implementation(
                        SourceUnitKey(u32::try_from(index).unwrap()),
                        &parsed.program,
                    ),
                )
                .0;
        }
        let binder = builder.finish(module);
        assert_eq!(builds.finish(), 2);
        let inputs = binder.source_input_observations();
        assert_eq!(
            inputs.work.indexed_operations,
            1 + n * manifest.source_references().operations().len()
        );
        assert_eq!(
            inputs.work.indexed_regions,
            1 + n * manifest.source_references().regions().len()
        );
        assert_eq!(inputs.work.finalized_regions, inputs.work.indexed_regions);
        assert_eq!(inputs.work.source_rows, n + 1);
        assert_eq!(inputs.work.hook_probes, 2 * n + 1);
        assert!(inputs.iter().all(|input| input.errors.is_empty()));
    }
}

#[test]
fn source_library_empty_reference_only_and_external_entries_are_observed() {
    use crate::source::LibraryFileOrdinal;
    for text in ["", "externalValue;", "export {}; externalValue;"] {
        let arena = Allocator::default();
        let parsed = Parser::new(&arena, text, SourceType::d_ts()).parse();
        let prelude = Parser::new(&arena, "", SourceType::d_ts()).parse();
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let builds = super::declaration::SourceDeclarationManifestBuildScopeForTest::start();
        let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
        let unit = CompilationUnit::library(
            SourceUnitKey(23),
            LibraryFileOrdinal::new(5),
            &parsed.program,
        );
        let modules = builder
            .try_add_library_modules_with_declaration_manifests(&[(
                &parsed.program,
                &manifest,
                unit,
            )])
            .unwrap();
        let binder = builder.finish(modules[0]);
        assert_eq!(builds.finish(), 0);
        let input = binder
            .source_input_observations()
            .input(modules[0], unit.source, &manifest.reference_input())
            .unwrap();
        let RegionObservation::Attached(root) = input.regions[0] else {
            panic!("root attached")
        };
        assert_eq!(
            root.scope,
            if unit.binding.external_module {
                modules[0]
            } else {
                binder.compilation_global
            }
        );
        assert_eq!(
            root.anchor.span,
            crate::span::Span::from_oxc(parsed.program.span)
        );
        assert_eq!(
            input.facts.references(),
            manifest.source_references().references()
        );
        assert_eq!(
            input.normalization.unwrap().old_parent,
            Some(binder.prelude_module)
        );
        assert_eq!(
            input.normalization.unwrap().new_parent,
            Some(binder.script_namespace_root)
        );
    }
}

#[test]
fn source_unit_and_origin_views_cannot_be_substituted_for_module_ownership() {
    use super::source_inputs::SourceInputs;
    let binder = bind("externalValue;");
    let consumed = binder
        .source_input_observations()
        .get(binder.module)
        .unwrap();
    for wrong_source in [true, false] {
        let mut inputs = SourceInputs::default();
        inputs.install(
            consumed.module,
            if wrong_source {
                SourceUnitKey(99)
            } else {
                consumed.source
            },
            Arc::clone(&consumed.facts),
            consumed.module_creation_parent,
            if wrong_source { consumed.origin } else { None },
        );
        inputs.finish(
            &binder.graph,
            binder.module_sources(),
            &binder.namespaces,
            binder.prelude_module,
        );
        assert!(inputs
            .get(consumed.module)
            .unwrap()
            .errors
            .contains(&InputError::WrongSource));
    }
}

#[test]
fn invalid_observation_does_not_prevent_ordinary_binding() {
    let arena = Allocator::default();
    let prelude = Parser::new(&arena, "", SourceType::ts()).parse();
    let mut parsed =
        Parser::new(&arena, "function f() { externalValue; }", SourceType::ts()).parse();
    parsed.program.span.end = 0;
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    assert_eq!(
        manifest.source_references().coverage(),
        super::source_references::ReferenceCoverage::InvalidProjection
    );
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let module = builder
        .add_module_with_declaration_manifest(
            &parsed.program,
            &manifest,
            &[],
            CompilationUnit::implementation(SourceUnitKey(1), &parsed.program),
        )
        .0;
    let binder = builder.finish(module);
    assert!(binder.resolve_value(module, "f").is_some());
    let input = binder.source_input_observations().get(module).unwrap();
    assert!(input.errors.contains(&InputError::InvalidProjection));
    assert_eq!(input.facts.references().len(), 1);
    assert_eq!(
        binder
            .source_input_observations()
            .input(module, input.source, &manifest.reference_input())
            .unwrap_err(),
        InputError::InvalidProjection
    );
}

#[test]
fn malformed_operation_parent_is_typed_invalid() {
    use super::source_inputs::SourceInputs;
    use super::source_references::SourceReferenceBuilder;
    use oxc_ast::{ast::Statement, AstKind};
    let arena = Allocator::default();
    let parsed = Parser::new(&arena, "function f() {}", SourceType::ts()).parse();
    let Statement::FunctionDeclaration(function) = &parsed.program.body[0] else {
        panic!("function")
    };
    let mut projection = SourceReferenceBuilder::default();
    projection.enter(AstKind::Program(&parsed.program));
    projection.enter(AstKind::Function(function));
    projection.leave(AstKind::Function(function));
    projection.leave(AstKind::Program(&parsed.program));
    projection.remove_last_edge_for_test();
    let facts = Arc::new(projection.finish());
    let mut inputs = SourceInputs::default();
    inputs.install(
        ScopeId(3),
        SourceUnitKey(1),
        Arc::clone(&facts),
        Some(ScopeId(0)),
        None,
    );
    assert_eq!(
        inputs
            .input(ScopeId(3), SourceUnitKey(1), &facts)
            .unwrap_err(),
        InputError::InvalidProjection
    );
}

#[test]
fn conflicting_origins_reject_the_second_user_even_at_source_zero() {
    use crate::source::{CompilationOrigin, OriginalModuleOrdinal};
    for source in [SourceUnitKey(7), SourceUnitKey(0)] {
        let arena = Allocator::default();
        let prelude = Parser::new(&arena, "", SourceType::ts()).parse();
        let parsed = Parser::new(&arena, "outside;", SourceType::ts()).parse();
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let mut builder = ProjectBinderBuilder::new(&prelude.program);
        let mut modules = Vec::new();
        for ordinal in [0, 1] {
            let unit = CompilationUnit {
                origin: CompilationOrigin::User(OriginalModuleOrdinal::new(ordinal)),
                ..CompilationUnit::implementation(source, &parsed.program)
            };
            modules.push(
                builder
                    .add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit)
                    .0,
            );
        }
        let binder = builder.finish(modules[1]);
        let inputs = binder.source_input_observations();
        assert!(!binder.namespaces.compilation_origin_conflicts().is_empty());
        assert!(inputs
            .input(modules[0], source, &manifest.reference_input())
            .is_ok());
        assert_eq!(
            inputs
                .input(modules[1], source, &manifest.reference_input())
                .unwrap_err(),
            InputError::WrongSource
        );
        for module in modules {
            let input = inputs.get(module).unwrap();
            assert!(Arc::ptr_eq(&input.facts, &manifest.reference_input()));
            assert_eq!(
                input.facts.references(),
                manifest.source_references().references()
            );
        }
        assert!(inputs.get(binder.prelude_module).unwrap().errors.is_empty());
    }
}

#[test]
fn source_zero_without_conflict_requires_the_full_user_origin_join() {
    use super::source_inputs::SourceInputs;
    let arena = Allocator::default();
    let prelude = Parser::new(&arena, "", SourceType::ts()).parse();
    let parsed = Parser::new(&arena, "outside;", SourceType::ts()).parse();
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let module = builder
        .add_module_with_declaration_manifest(
            &parsed.program,
            &manifest,
            &[],
            CompilationUnit::implementation(SourceUnitKey(0), &parsed.program),
        )
        .0;
    let binder = builder.finish(module);
    assert!(binder.namespaces.compilation_origin_conflicts().is_empty());
    let input = binder
        .source_input_observations()
        .input(module, SourceUnitKey(0), &manifest.reference_input())
        .unwrap();
    let mut malformed = SourceInputs::default();
    malformed.install(
        module,
        input.source,
        Arc::clone(&input.facts),
        input.module_creation_parent,
        None,
    );
    malformed.finish(
        &binder.graph,
        binder.module_sources(),
        &binder.namespaces,
        binder.prelude_module,
    );
    assert!(malformed
        .get(module)
        .unwrap()
        .errors
        .contains(&InputError::WrongSource));
}

#[test]
fn trusted_prelude_role_still_requires_its_source_origin_and_root_invariants() {
    use super::source_inputs::SourceInputs;
    use crate::source::{CompilationOrigin, OriginalModuleOrdinal};
    let binder = bind("outside;");
    let prelude = binder
        .source_input_observations()
        .get(binder.prelude_module)
        .unwrap();
    for (source, origin, parent, valid) in [
        (SourceUnitKey(0), None, None, true),
        (SourceUnitKey(7), None, None, false),
        (
            SourceUnitKey(0),
            Some(CompilationOrigin::User(OriginalModuleOrdinal::new(0))),
            None,
            false,
        ),
        (SourceUnitKey(0), None, Some(binder.module), false),
    ] {
        let mut inputs = SourceInputs::default();
        inputs.install(
            prelude.module,
            source,
            Arc::clone(&prelude.facts),
            parent,
            origin,
        );
        let root = prelude.facts.regions()[0].anchor;
        inputs.attach(
            prelude.module,
            root.syntax,
            root.span,
            prelude.module,
            &binder.graph,
        );
        inputs.finish(
            &binder.graph,
            binder.module_sources(),
            &binder.namespaces,
            binder.prelude_module,
        );
        let observed = inputs.get(prelude.module).unwrap();
        assert_eq!(!observed.errors.contains(&InputError::WrongSource), valid);
        assert!(Arc::ptr_eq(&observed.facts, &prelude.facts));
        assert_eq!(observed.facts.references(), prelude.facts.references());
    }
}

#[test]
fn trusted_source_prelude_and_unavailable_empty_prelude_keep_their_roles() {
    for unavailable in [false, true] {
        let arena = Allocator::default();
        let prelude = Parser::new(
            &arena,
            if unavailable {
                ""
            } else {
                "declare function preludeFn(): void;"
            },
            SourceType::d_ts(),
        )
        .parse();
        let parsed = Parser::new(&arena, "outside;", SourceType::ts()).parse();
        let mut builder = if unavailable {
            ProjectBinderBuilder::new_empty_library_prelude(&prelude.program)
        } else {
            ProjectBinderBuilder::new(&prelude.program)
        };
        let module = builder
            .add_module(
                &parsed.program,
                &[],
                CompilationUnit::implementation(SourceUnitKey(0), &parsed.program),
            )
            .0;
        let binder = builder.finish(module);
        let prelude = binder
            .source_input_observations()
            .get(binder.prelude_module)
            .unwrap();
        assert_eq!(
            prelude.errors.contains(&InputError::Unavailable),
            unavailable
        );
        assert!(!prelude.errors.contains(&InputError::WrongSource));
        assert!(binder
            .source_input_observations()
            .get(module)
            .unwrap()
            .errors
            .is_empty());
    }
}

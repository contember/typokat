use super::super::{
    bind_library_product_project_programs,
    bind_library_product_project_programs_fully_perturbed_for_test,
    project_local::{CheckerSession, ProjectSourceReexportPolicy},
    CheckResult, LibraryProductProjectBindingError, ProjectBindingMeasurementScope,
    ProjectDeclarationStructuralError,
};
use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::SourceDeclarationManifest;
use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::binder::product::{project_library_declaration_product, LibraryProductSource};
use crate::frontend::{run_prepared_project_frontend, AdmittedSourceReexports, FileInput};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::sync::Arc;

fn library_product() -> typokat_library_product::LibraryDeclarationProduct {
    let source = r#"
        declare namespace ProductNs {
            interface Nested { value: number }
            const nestedValue: Nested;
            type NestedValueQuery = typeof nestedValue;
            function identity<T>(value: T): T;
        }
        interface ProductLibraryRoot { value: ProductNs.Nested }
        declare const productValue: ProductLibraryRoot;
    "#;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
    let unit = CompilationUnit {
        source: SourceUnitKey(1),
        origin: CompilationOrigin::Library(LibraryFileOrdinal::new(7)),
        binding: ModuleBindingContext::for_program(
            &parsed.program,
            source_file_kind("product-control.d.ts"),
        ),
    };
    let modules = builder
        .try_add_library_modules_with_declaration_manifests(&[(&parsed.program, &manifest, unit)])
        .expect("bind product control");
    let binder = builder.finish(modules[0]);
    project_library_declaration_product(
        "product-project-structure",
        &binder,
        &[LibraryProductSource {
            ordinal: 7,
            name: "product-control.d.ts",
            source_sha256: "00",
            byte_len: u32::try_from(source.len()).expect("source length"),
            external_module: false,
            module: modules[0],
            manifest: &manifest,
            source_text: source,
        }],
        &[],
    )
    .expect("project product control")
}

fn bind(
    sources: &[(&str, &str)],
) -> Result<super::super::BoundProjectBinder, LibraryProductProjectBindingError> {
    bind_with_perturbation(sources, 0)
}

fn bind_with_perturbation(
    sources: &[(&str, &str)],
    perturbation_rows: usize,
) -> Result<super::super::BoundProjectBinder, LibraryProductProjectBindingError> {
    let product = library_product();
    let inputs = sources
        .iter()
        .map(|(name, source)| FileInput {
            name: (*name).to_owned(),
            source: (*source).to_owned(),
        })
        .collect();
    let run = run_prepared_project_frontend(inputs, |units, reexports, defaults| {
        if perturbation_rows == 0 {
            bind_library_product_project_programs(
                &product,
                units,
                ProjectSourceReexportPolicy::Admit(reexports),
                defaults,
            )
            .map(|(_, bound)| bound)
        } else {
            bind_library_product_project_programs_fully_perturbed_for_test(
                &product,
                units,
                reexports,
                defaults,
                perturbation_rows,
            )
            .map(|(_, bound)| bound)
        }
    });
    assert!(run.preparation_error.is_none());
    run.product.expect("clean prepared project")
}

fn check(sources: &[(&str, &str)]) -> Vec<CheckResult> {
    let product = Arc::new(library_product());
    let inputs = sources
        .iter()
        .map(|(name, source)| FileInput {
            name: (*name).to_owned(),
            source: (*source).to_owned(),
        })
        .collect();
    let run = run_prepared_project_frontend(inputs, |units, reexports, defaults| {
        CheckerSession::new(Arc::clone(&product)).check_project_programs(
            units,
            ProjectSourceReexportPolicy::Admit(reexports),
            defaults,
        )
    });
    assert!(run.preparation_error.is_none());
    run.product
        .expect("clean prepared project")
        .expect("reportable project structure reaches the user Pass")
}

fn diagnostic_codes(results: &[CheckResult]) -> Vec<&str> {
    results
        .iter()
        .flat_map(|result| &result.diagnostics)
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn bind_without_source_reexport_admission(
    sources: &[(&str, &str)],
) -> Result<super::super::BoundProjectBinder, LibraryProductProjectBindingError> {
    let product = library_product();
    let inputs = sources
        .iter()
        .map(|(name, source)| FileInput {
            name: (*name).to_owned(),
            source: (*source).to_owned(),
        })
        .collect();
    let run = run_prepared_project_frontend(inputs, |units, _, defaults| {
        let empty = AdmittedSourceReexports::empty();
        bind_library_product_project_programs(
            &product,
            units,
            ProjectSourceReexportPolicy::Admit(&empty),
            defaults,
        )
        .map(|(_, bound)| bound)
    });
    assert!(run.preparation_error.is_none());
    run.product.expect("clean prepared project")
}

fn assert_exact_product_map(
    product: &typokat_library_product::LibraryDeclarationProduct,
    bound: &super::super::BoundProjectBinder,
) {
    let map = bound
        .library_product_map
        .as_ref()
        .expect("product project retains its exact local identity map");
    for index in 0..product.sources().len() {
        let key = product.source_key_at(index).expect("source key");
        let binding = map.source(key).expect("mapped source");
        assert_eq!(
            bound.binder.source_for_module(binding.module()),
            Some(binding.source())
        );
    }
    for index in 0..product.containers().len() {
        let key = product.container_key_at(index).expect("container key");
        assert!(bound
            .binder
            .graph
            .get(map.container(key).expect("mapped container"))
            .is_some());
    }
    for index in 0..product.regions().len() {
        let key = product.region_key_at(index).expect("region key");
        let binding = map.region(key).expect("mapped region");
        assert!(bound.binder.graph.get(binding.syntax_scope()).is_some());
        assert!(bound.binder.graph.get(binding.lexical_scope()).is_some());
    }
    for index in 0..product.callable_scopes().len() {
        let key = product
            .callable_scope_key_at(index)
            .expect("callable scope key");
        assert!(bound
            .binder
            .graph
            .get(map.callable_scope(key).expect("mapped callable scope"))
            .is_some());
    }
    for index in 0..product.declarations().len() {
        let key = product.declaration_key_at(index).expect("declaration key");
        let binding = map.declaration(key).expect("mapped declaration");
        let declaration = bound
            .binder
            .declarations
            .get(binding.declaration())
            .expect("mapped declaration exists");
        assert_eq!(declaration.value_storage, binding.value());
        if let Some(fragment) = binding.namespace_fragment() {
            assert_eq!(
                bound
                    .binder
                    .namespaces
                    .fragment(fragment)
                    .expect("mapped namespace fragment")
                    .declaration,
                binding.declaration()
            );
        }
    }
    for (index, recipe) in product.groups().iter().enumerate() {
        let key = product.group_key_at(index).expect("group key");
        let binding = map.group(key).expect("mapped group");
        let symbol = bound
            .binder
            .symbols
            .get(binding.symbol())
            .expect("mapped symbol exists");
        assert_eq!(symbol.ty, binding.ty());
        assert_eq!(symbol.value, binding.value());
        assert_eq!(symbol.ns, binding.namespace());
        let first = product
            .declarations()
            .get(recipe.fragments[0].index())
            .expect("group fragment");
        let scope = match first.lexical_scope {
            typokat_library_product::DeclarationLexicalScope::Callable(callable) => {
                map.callable_scope(callable).expect("group callable scope")
            }
            typokat_library_product::DeclarationLexicalScope::Region(region)
                if product.regions()[region.index()].kind
                    == typokat_library_product::RegionKind::Namespace =>
            {
                map.container(recipe.container).expect("group container")
            }
            typokat_library_product::DeclarationLexicalScope::Region(region)
                if !recipe.slots.r#type =>
            {
                map.region(region)
                    .expect("group lexical region")
                    .lexical_scope()
            }
            typokat_library_product::DeclarationLexicalScope::None
            | typokat_library_product::DeclarationLexicalScope::Region(_) => {
                map.container(recipe.container).expect("group container")
            }
        };
        let name = &product.strings()[recipe.name.index()];
        assert_eq!(
            bound
                .binder
                .graph
                .get(scope)
                .and_then(|scope| scope.lookup_local(name)),
            Some(binding.symbol()),
            "group {name} must map to its final public symbol"
        );
    }
}

#[test]
fn product_project_reports_external_structure_through_the_user_pass() {
    assert_eq!(
        diagnostic_codes(&check(&[(
            "/project/missing.ts",
            "import { X } from './absent'; export type Y = X;",
        )])),
        ["TK2307"],
    );
    assert_eq!(
        diagnostic_codes(&check(&[
            ("/project/source.ts", "export interface X {}"),
            (
                "/project/consumer.ts",
                "import { Y } from './source'; export type Z = Y;",
            ),
        ])),
        ["TK2305"],
    );
    assert_eq!(
        diagnostic_codes(&check(&[
            ("/project/source.ts", "export interface X {}"),
            (
                "/project/default.ts",
                "import Missing from './source'; export { Missing };",
            ),
        ])),
        ["TK1192"],
    );
    assert_eq!(
        diagnostic_codes(&check(&[
            ("/project/source.ts", "export interface X {}"),
            (
                "/project/duplicate.ts",
                "export { X } from './source'; export interface X {}",
            ),
        ])),
        ["TK2300", "TK2300"],
    );
}

#[test]
fn product_project_still_requires_source_reexport_admission() {
    assert!(matches!(
        bind_without_source_reexport_admission(&[
            ("/project/source.ts", "export interface X {}"),
            ("/project/bridge.ts", "export { X } from './source.js';"),
        ]),
        Err(LibraryProductProjectBindingError::Structural(
            ProjectDeclarationStructuralError::MissingSourceReexportAdmission { .. }
        ))
    ));
}

#[test]
fn product_project_accepts_an_authenticated_source_reexport() {
    bind(&[
        ("/project/source.ts", "export interface X {}"),
        ("/project/bridge.ts", "export { X } from './source.js';"),
        (
            "/project/consumer.ts",
            "import { X } from './bridge'; export interface Y extends X {}",
        ),
    ])
    .expect("authenticated source re-export binds without placeholders");
}

#[test]
fn test_utils_authenticates_the_heavy_binding_projection_counter() {
    let measurement = ProjectBindingMeasurementScope::start();
    bind(&[("/project/one.ts", "export interface One {}")]).expect("product project binds");
    let measurement = measurement.finish();
    assert_eq!(measurement.entries, 1);
    assert_eq!(measurement.typed_products, 1);
    assert_eq!(measurement.heavy_binding_projections, 1);
}

#[test]
fn product_key_map_tracks_final_public_symbols_after_a_live_namespace_merge() {
    let product = library_product();
    let bound = bind(&[(
        "/project/augment.ts",
        "namespace ProductNs { export const userValue = 1; export interface Added {} }",
    )])
    .expect("live namespace augmentation binds");

    assert_exact_product_map(&product, &bound);
}

#[test]
fn product_key_map_uses_exact_fully_perturbed_local_identities() {
    let product = library_product();
    let sources = [(
        "/project/augment.ts",
        "namespace ProductNs { export const userValue = 1; export interface Added {} }",
    )];
    let ordinary_rows = crate::types::layered::LocalRowAllocationScopeForTest::start();
    let ordinary = bind(&sources).expect("ordinary product binding");
    let ordinary_rows = ordinary_rows.finish();
    let perturbed_rows = crate::types::layered::LocalRowAllocationScopeForTest::start();
    let perturbed = bind_with_perturbation(&sources, 7).expect("perturbed product binding");
    let perturbed_rows = perturbed_rows.finish();
    let ordinary_families = ordinary.binder.local_family_row_counts_for_test();
    let perturbed_families = perturbed.binder.local_family_row_counts_for_test();
    assert_eq!(
        std::array::from_fn(|index| perturbed_families[index] - ordinary_families[index]),
        [7, 7, 7, 7, 7, 7, 49, 0]
    );
    assert_eq!(perturbed_rows - ordinary_rows, 140);
    assert_exact_product_map(&product, &ordinary);
    assert_exact_product_map(&product, &perturbed);
    let ordinary_map = ordinary.library_product_map.as_ref().expect("ordinary map");
    let perturbed_map = perturbed
        .library_product_map
        .as_ref()
        .expect("perturbed map");

    for index in 0..product.sources().len() {
        let key = product.source_key_at(index).expect("source key");
        assert_ne!(
            ordinary_map.source(key).expect("ordinary source").source(),
            perturbed_map
                .source(key)
                .expect("perturbed source")
                .source()
        );
        assert_ne!(
            ordinary_map.source(key).expect("ordinary source").module(),
            perturbed_map
                .source(key)
                .expect("perturbed source")
                .module()
        );
    }
    for index in 0..product.containers().len() {
        let key = product.container_key_at(index).expect("container key");
        assert_ne!(ordinary_map.container(key), perturbed_map.container(key));
    }
    for index in 0..product.regions().len() {
        let key = product.region_key_at(index).expect("region key");
        assert_ne!(ordinary_map.region(key), perturbed_map.region(key));
    }
    for index in 0..product.callable_scopes().len() {
        let key = product
            .callable_scope_key_at(index)
            .expect("callable scope key");
        assert_ne!(
            ordinary_map.callable_scope(key),
            perturbed_map.callable_scope(key)
        );
    }

    let mut saw_value = false;
    let mut saw_namespace_fragment = false;
    for index in 0..product.declarations().len() {
        let key = product.declaration_key_at(index).expect("declaration key");
        let ordinary_binding = ordinary_map.declaration(key).expect("ordinary declaration");
        let perturbed_binding = perturbed_map
            .declaration(key)
            .expect("perturbed declaration");
        assert_ne!(
            ordinary_binding.declaration(),
            perturbed_binding.declaration()
        );
        if let (Some(ordinary_value), Some(perturbed_value)) =
            (ordinary_binding.value(), perturbed_binding.value())
        {
            saw_value = true;
            assert_ne!(ordinary_value, perturbed_value);
        }
        if let (Some(ordinary_fragment), Some(perturbed_fragment)) = (
            ordinary_binding.namespace_fragment(),
            perturbed_binding.namespace_fragment(),
        ) {
            saw_namespace_fragment = true;
            assert_ne!(ordinary_fragment, perturbed_fragment);
        }
    }

    let mut saw_type_group = false;
    let mut saw_group_value = false;
    let mut saw_namespace = false;
    for index in 0..product.groups().len() {
        let key = product.group_key_at(index).expect("group key");
        let ordinary_binding = ordinary_map.group(key).expect("ordinary group");
        let perturbed_binding = perturbed_map.group(key).expect("perturbed group");
        assert_ne!(ordinary_binding.symbol(), perturbed_binding.symbol());
        if let (Some(ordinary_ty), Some(perturbed_ty)) =
            (ordinary_binding.ty(), perturbed_binding.ty())
        {
            saw_type_group = true;
            assert_ne!(ordinary_ty, perturbed_ty);
        }
        if let (Some(ordinary_value), Some(perturbed_value)) =
            (ordinary_binding.value(), perturbed_binding.value())
        {
            saw_group_value = true;
            assert_ne!(ordinary_value, perturbed_value);
        }
        if let (Some(ordinary_namespace), Some(perturbed_namespace)) =
            (ordinary_binding.namespace(), perturbed_binding.namespace())
        {
            saw_namespace = true;
            assert_ne!(ordinary_namespace, perturbed_namespace);
        }
    }
    assert!(saw_value);
    assert!(saw_namespace_fragment);
    assert!(saw_type_group);
    assert!(saw_group_value);
    assert!(saw_namespace);
}

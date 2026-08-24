use super::super::{
    bind_library_product_project_programs,
    bind_library_product_project_programs_fully_perturbed_for_test,
    LibraryProductProjectBindingError, ProjectBindingMeasurementScope,
    ProjectDeclarationStructuralError,
};
use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::SourceDeclarationManifest;
use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::binder::product::{project_library_declaration_product, LibraryProductSource};
use crate::frontend::{run_prepared_project_frontend, FileInput};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

#[derive(Default)]
struct SemanticTracePublisher {
    begins: Vec<Vec<crate::binder::symbol::SymbolId>>,
}

impl super::super::project_local_semantics::ProjectLocalSemanticPublisher
    for SemanticTracePublisher
{
    type Ready = crate::binder::symbol::SymbolId;
    type Unavailable = &'static str;
    type Error = std::convert::Infallible;

    fn begin_component(&mut self, members: &[crate::binder::symbol::SymbolId]) {
        self.begins.push(members.to_vec());
    }

    fn lower_component(
        &mut self,
        members: &[crate::binder::symbol::SymbolId],
    ) -> Result<Vec<Result<Self::Ready, Self::Unavailable>>, Self::Error> {
        Ok(members.iter().copied().map(Ok).collect())
    }

    fn commit_component(&mut self, _members: &[crate::binder::symbol::SymbolId]) {}

    fn rollback_component(&mut self, _members: &[crate::binder::symbol::SymbolId]) {}
}

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
            bind_library_product_project_programs(&product, units, reexports, defaults)
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
fn product_project_returns_typed_structural_failures_without_partial_surfaces() {
    assert!(matches!(
        bind(&[(
            "/project/missing.ts",
            "import { X } from './absent'; export type Y = X;",
        )]),
        Err(LibraryProductProjectBindingError::Structural(
            ProjectDeclarationStructuralError::CannotFindModule { .. }
        ))
    ));
    assert!(matches!(
        bind(&[
            ("/project/source.ts", "export interface X {}"),
            (
                "/project/consumer.ts",
                "import { Y } from './source'; export type Z = Y;",
            ),
        ]),
        Err(LibraryProductProjectBindingError::Structural(
            ProjectDeclarationStructuralError::NoExportedMember { .. }
        ))
    ));
    assert!(matches!(
        bind(&[
            ("/project/source.ts", "export interface X {}"),
            (
                "/project/default.ts",
                "import Missing from './source'; export { Missing };",
            ),
        ]),
        Err(LibraryProductProjectBindingError::Structural(
            ProjectDeclarationStructuralError::NoDefaultExport { .. }
        ))
    ));
    assert!(matches!(
        bind(&[
            ("/project/source.ts", "export interface X {}"),
            (
                "/project/duplicate.ts",
                "export { X } from './source'; export interface X {}",
            ),
        ]),
        Err(LibraryProductProjectBindingError::Structural(
            ProjectDeclarationStructuralError::DuplicateIdentifier { .. }
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

#[test]
fn perturbed_nonsemantic_symbol_is_not_admitted_or_forced() {
    let product = library_product();
    let bound = bind_with_perturbation(&[], 7).expect("perturbed product binding");
    let map = bound.library_product_map.as_ref().expect("product map");
    let owners = super::super::project_local_semantics::project_local_semantic_owners(
        &product,
        map,
        &bound.binder,
        &[],
    )
    .expect("semantic owners");
    let dummy = bound
        .binder
        .symbols
        .iter()
        .map(|(symbol, _)| symbol)
        .find(|symbol| !owners.contains(symbol))
        .expect("perturbation symbol outside semantic owners");
    let mut coordinator =
        super::super::project_local_semantics::ProjectLocalSemanticCoordinator::new(
            owners.clone(),
            [],
        );
    let mut publisher = SemanticTracePublisher::default();

    assert!(matches!(
        coordinator.demand(dummy, &mut publisher),
        Err(super::super::project_local_semantics::ProjectLocalDemandError::UnknownGroup(
            symbol
        )) if symbol == dummy
    ));
    coordinator.force_all(&mut publisher).expect("force all");

    let published = publisher.begins.into_iter().flatten().collect::<Vec<_>>();
    assert_eq!(published, owners);
}

#[test]
fn mapped_query_group_dependency_is_directional() {
    let product = library_product();
    let bound = bind(&[]).expect("product binding");
    let map = bound.library_product_map.as_ref().expect("product map");
    let edges = super::super::project_local_semantics::library_type_dependencies(&product, map)
        .expect("mapped dependencies");
    let symbol = |name: &str| {
        product
            .groups()
            .iter()
            .enumerate()
            .find(|(_, row)| product.strings()[row.name.index()] == name)
            .and_then(|(index, _)| product.group_key_at(index))
            .and_then(|key| map.group(key))
            .map(|binding| binding.symbol())
            .expect("named product group")
    };
    let owner = symbol("NestedValueQuery");
    let dependency = symbol("nestedValue");
    assert!(product.dependency_rows().iter().any(|row| {
        row.kind == typokat_library_product::DependencyKind::Query
            && map.group(row.owner).map(|binding| binding.symbol()) == Some(owner)
            && matches!(
                row.target,
                typokat_library_product::DependencyTarget::Group(target)
                    if map.group(target).map(|binding| binding.symbol()) == Some(dependency)
            )
    }));
    assert!(edges.contains(&(owner, dependency)));
    let owners = super::super::project_local_semantics::project_local_semantic_owners(
        &product,
        map,
        &bound.binder,
        &[],
    )
    .expect("semantic owners");
    let mut partition = super::super::project_local_semantics::ProjectLocalSemanticCoordinator::<
        crate::binder::symbol::SymbolId,
        &'static str,
    >::new(owners.clone(), edges.clone())
    .components_for_test();
    let mut expected_partition = product
        .dependency_sccs()
        .into_iter()
        .map(|component| {
            let mut members = component
                .into_iter()
                .map(|key| map.group(key).expect("mapped SCC member").symbol())
                .collect::<Vec<_>>();
            members.sort_by_key(|symbol| symbol.index());
            members
        })
        .collect::<Vec<_>>();
    partition.sort_by_key(|members| members.first().map(|symbol| symbol.index()));
    expected_partition.sort_by_key(|members| members.first().map(|symbol| symbol.index()));
    assert_eq!(partition, expected_partition);

    let mut owner_first =
        super::super::project_local_semantics::ProjectLocalSemanticCoordinator::new(
            owners.clone(),
            edges.clone(),
        );
    let mut owner_publisher = SemanticTracePublisher::default();
    owner_first
        .demand(owner, &mut owner_publisher)
        .expect("query owner demand");
    let owner_order = owner_publisher
        .begins
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert!(
        owner_order.iter().position(|symbol| *symbol == dependency)
            < owner_order.iter().position(|symbol| *symbol == owner)
    );

    let mut dependency_first =
        super::super::project_local_semantics::ProjectLocalSemanticCoordinator::new(owners, edges);
    let mut dependency_publisher = SemanticTracePublisher::default();
    dependency_first
        .demand(dependency, &mut dependency_publisher)
        .expect("query dependency demand");
    assert!(!dependency_publisher
        .begins
        .iter()
        .flatten()
        .any(|symbol| *symbol == owner));
}

#[test]
fn isolated_live_type_owner_is_admitted_and_forced() {
    let product = library_product();
    let inputs = vec![FileInput {
        name: "/project/isolated.ts".to_owned(),
        source: "interface UserOnly {}".to_owned(),
    }];
    let product_for_build = product.clone();
    let run = run_prepared_project_frontend(inputs, move |units, reexports, defaults| {
        let (_, bound) =
            bind_library_product_project_programs(&product_for_build, units, reexports, defaults)?;
        let inventories = units
            .iter()
            .zip(&bound.module_scopes)
            .map(|(unit, module)| {
                let source = bound
                    .binder
                    .source_for_module(*module)
                    .expect("user source");
                super::super::decls::inventory::BoundTypeDeclarationInventory::build(
                    &bound.binder,
                    *module,
                    source,
                    SourceDeclarationManifest::build(unit.program),
                    None,
                )
                .expect("user inventory")
            })
            .collect::<Vec<_>>();
        let map = bound.library_product_map.as_ref().expect("product map");
        let product_owners = super::super::project_local_semantics::project_local_semantic_owners(
            &product_for_build,
            map,
            &bound.binder,
            &[],
        )
        .expect("product owners");
        let owners = super::super::project_local_semantics::project_local_semantic_owners(
            &product_for_build,
            map,
            &bound.binder,
            &inventories,
        )
        .expect("combined owners");
        let isolated = owners
            .iter()
            .copied()
            .find(|owner| !product_owners.contains(owner))
            .expect("isolated user owner");
        let edges = super::super::project_local_semantics::library_type_dependencies(
            &product_for_build,
            map,
        )
        .expect("library dependencies");
        Ok::<_, LibraryProductProjectBindingError>((owners, edges, isolated))
    });
    let (owners, edges, isolated) = run
        .product
        .expect("prepared project")
        .expect("bound project");
    let mut coordinator =
        super::super::project_local_semantics::ProjectLocalSemanticCoordinator::new(
            owners.clone(),
            edges,
        );
    let mut publisher = SemanticTracePublisher::default();

    coordinator
        .demand(isolated, &mut publisher)
        .expect("isolated demand");
    assert_eq!(publisher.begins, vec![vec![isolated]]);
    coordinator.force_all(&mut publisher).expect("force all");
    let mut published = publisher.begins.into_iter().flatten().collect::<Vec<_>>();
    published.sort_by_key(|symbol| symbol.index());
    published.dedup();
    assert_eq!(published, owners);
}

#[test]
fn mapped_product_and_live_user_edges_form_one_atomic_component() {
    let product = library_product();
    let inputs = vec![FileInput {
        name: "/project/cycle.ts".to_owned(),
        source: "namespace ProductNs { export interface Nested { back: ProductLibraryRoot } }"
            .to_owned(),
    }];
    let run = run_prepared_project_frontend(inputs, |units, reexports, defaults| {
        let (_, bound) =
            bind_library_product_project_programs(&product, units, reexports, defaults)?;
        let inventories = units
            .iter()
            .zip(&bound.module_scopes)
            .map(|(unit, module)| {
                let source = bound
                    .binder
                    .source_for_module(*module)
                    .expect("user module source");
                super::super::decls::inventory::BoundTypeDeclarationInventory::build(
                    &bound.binder,
                    *module,
                    source,
                    SourceDeclarationManifest::build(unit.program),
                    None,
                )
                .expect("user declaration inventory")
            })
            .collect::<Vec<_>>();
        let map = bound
            .library_product_map
            .as_ref()
            .expect("product identity map");
        let library_edges =
            super::super::project_local_semantics::library_type_dependencies(&product, map)
                .expect("mapped library dependencies");
        let user_edges = super::super::project_local_semantics::user_type_dependencies(
            &product,
            map,
            &bound.binder,
            &inventories,
        );
        let group = |name: &str| {
            product
                .groups()
                .iter()
                .enumerate()
                .find(|(_, row)| product.strings()[row.name.index()] == name)
                .and_then(|(index, _)| product.group_key_at(index))
                .and_then(|key| map.group(key))
                .map(|binding| binding.symbol())
                .expect("named product semantic group")
        };
        let owners = super::super::project_local_semantics::project_local_semantic_owners(
            &product,
            map,
            &bound.binder,
            &inventories,
        )
        .expect("project-local semantic owners");
        Ok::<_, LibraryProductProjectBindingError>((
            library_edges,
            user_edges,
            owners,
            group("ProductLibraryRoot"),
            group("Nested"),
        ))
    });
    let (library_edges, user_edges, admitted, root, nested) = run
        .product
        .expect("prepared project")
        .expect("bound project");
    assert!(library_edges.contains(&(root, nested)));
    assert!(
        user_edges.contains(&(nested, root)),
        "expected live Nested→Root edge {nested:?}→{root:?}, got {user_edges:?}"
    );

    let mut edges = library_edges;
    edges.extend(user_edges);
    let mut coordinator =
        super::super::project_local_semantics::ProjectLocalSemanticCoordinator::new(
            admitted, edges,
        );
    let mut publisher = SemanticTracePublisher::default();
    coordinator
        .demand(root, &mut publisher)
        .expect("root demand");
    let mut expected = vec![root, nested];
    expected.sort_by_key(|symbol| symbol.index());
    assert_eq!(publisher.begins, vec![expected]);

    let base_sccs = product.dependency_sccs();
    let mut base_members = base_sccs.into_iter().flatten().collect::<Vec<_>>();
    base_members.sort_by_key(|key| key.index());
    base_members.dedup();
    assert_eq!(base_members.len(), product.group_count());
}

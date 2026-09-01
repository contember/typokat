//! RED contract: library-product namespace-region lookup scales with product rows.
//!
//! Every declaration already carries a region key. Resolving that key must not scan all product
//! declarations once per declaration when the product contains many independent namespaces.

use super::bind::ProjectBinderBuilder;
use super::declaration::SourceDeclarationManifest;
use super::namespace::{
    CompilationUnit, NamespaceRegionLookupWorkForTest, NamespaceRegionLookupWorkScopeForTest,
    SourceUnitKey,
};
use super::product::{project_library_declaration_product, LibraryProductSource};
use crate::source::LibraryFileOrdinal;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

const SMALL: usize = 32;
const SCALED: usize = 128;
const DECLARATIONS_PER_REGION: usize = 3;
const ROW_PROBES_PER_DECLARATION: u64 = 8;

fn source(namespace_regions: usize) -> String {
    (0..namespace_regions)
        .map(|index| {
            format!(
                "declare namespace Region{index} {{\n\
                 interface Type{index} {{ marker: number }}\n\
                 const value{index}: Type{index};\n\
                 }}\n"
            )
        })
        .collect()
}

fn declaration_product(
    namespace_regions: usize,
) -> typokat_library_product::LibraryDeclarationProduct {
    let source = source(namespace_regions);
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::d_ts()).parse();
    assert!(!parsed.panicked);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let manifest = SourceDeclarationManifest::build(&parsed.program);

    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
    let unit = CompilationUnit::library(
        SourceUnitKey(1),
        LibraryFileOrdinal::new(0),
        &parsed.program,
    );
    let modules = builder
        .try_add_library_modules_with_declaration_manifests(&[(&parsed.program, &manifest, unit)])
        .expect("synthetic library binds");
    let binder = builder.finish(modules[0]);
    let product = project_library_declaration_product(
        "namespace-region-lookup-scaling",
        &binder,
        &[LibraryProductSource {
            ordinal: 0,
            name: "lib.scaling.d.ts",
            source_sha256: "00",
            byte_len: u32::try_from(source.len()).expect("source length fits u32"),
            external_module: false,
            module: modules[0],
            manifest: &manifest,
            source_text: &source,
        }],
        &[],
    )
    .expect("synthetic declaration product projects");
    product.validate().expect("synthetic product validates");
    assert_eq!(
        product.declaration_count(),
        DECLARATIONS_PER_REGION * namespace_regions
    );
    assert_eq!(product.regions().len(), namespace_regions + 1);
    assert_eq!(product.containers().len(), namespace_regions + 1);
    product
}

fn measure(namespace_regions: usize) -> NamespaceRegionLookupWorkForTest {
    let product = declaration_product(namespace_regions);
    let scope = NamespaceRegionLookupWorkScopeForTest::start();
    let (builder, units) = ProjectBinderBuilder::from_library_declaration_product(&product)
        .expect("synthetic declaration product instantiates");
    let work = scope.finish();
    assert_eq!(units.len(), 1);

    let (binder, _) = builder
        .finish_library_product_project(None)
        .expect("synthetic product binder finishes");
    assert_eq!(
        binder.declarations.iter().count(),
        product.declaration_count()
    );
    let namespaces = binder.namespaces.namespaces().collect::<Vec<_>>();
    assert_eq!(namespaces.len(), namespace_regions);
    for index in 0..namespace_regions {
        let namespace = namespaces
            .iter()
            .find(|namespace| namespace.name == format!("Region{index}"))
            .expect("namespace is retained");
        let scope = binder
            .graph
            .get(namespace.public_scope)
            .expect("namespace public scope is retained");
        assert!(scope.lookup_local(&format!("Type{index}")).is_some());
        assert!(scope.lookup_local(&format!("value{index}")).is_some());
    }
    work
}

#[test]
fn namespace_region_lookup_work_scales_with_product_rows() {
    let small_product_rows = u64::try_from(DECLARATIONS_PER_REGION * SMALL).unwrap_or(u64::MAX);
    let scaled_product_rows = u64::try_from(DECLARATIONS_PER_REGION * SCALED).unwrap_or(u64::MAX);
    let step = scaled_product_rows / small_product_rows;
    let small = measure(SMALL).declaration_row_probes;
    let scaled = measure(SCALED).declaration_row_probes;
    let small_budget = ROW_PROBES_PER_DECLARATION * small_product_rows;
    let scaled_budget = ROW_PROBES_PER_DECLARATION * scaled_product_rows;

    assert!(
        small <= small_budget && scaled <= scaled_budget && scaled <= 2 * step * small.max(1),
        "namespace-region lookup exceeds linear product-row work: {small} -> {scaled} probes, \
         budgets {small_budget} / {scaled_budget} for a {step}x product"
    );
}

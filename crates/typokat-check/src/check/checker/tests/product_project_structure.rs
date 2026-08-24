use super::super::{
    bind_library_product_project_programs, LibraryProductProjectBindingError,
    ProjectBindingMeasurementScope, ProjectDeclarationStructuralError,
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

fn library_product() -> typokat_library_product::LibraryDeclarationProduct {
    let source = "interface ProductLibraryRoot { value: number }";
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
    let product = library_product();
    let inputs = sources
        .iter()
        .map(|(name, source)| FileInput {
            name: (*name).to_owned(),
            source: (*source).to_owned(),
        })
        .collect();
    let run = run_prepared_project_frontend(inputs, |units, reexports, defaults| {
        bind_library_product_project_programs(&product, units, reexports, defaults)
            .map(|(_, bound)| bound)
    });
    assert!(run.preparation_error.is_none());
    run.product.expect("clean prepared project")
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

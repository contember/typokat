use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::SourceDeclarationManifest;
use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::binder::product::{project_library_declaration_product, LibraryProductSource};
use crate::check::checker::project_local::CheckerSession;
use crate::frontend::{run_prepared_project_frontend, FileInput};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::sync::Arc;
use typokat_library_product::LibraryDeclarationProduct;

const LIBRARY_PATH: &str = "project-local-session.d.ts";
const USER_PATH: &str = "/project/main.ts";

fn extract_library_product(source: &str) -> LibraryDeclarationProduct {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "library fixture parses");
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
    let unit = CompilationUnit {
        source: SourceUnitKey(1),
        origin: CompilationOrigin::Library(LibraryFileOrdinal::new(0)),
        binding: ModuleBindingContext::for_program(&parsed.program, source_file_kind(LIBRARY_PATH)),
    };
    let modules = builder
        .try_add_library_modules_with_declaration_manifests(&[(&parsed.program, &manifest, unit)])
        .expect("bind library fixture");
    let binder = builder.finish(modules[0]);
    project_library_declaration_product(
        "project-local-session",
        &binder,
        &[LibraryProductSource {
            ordinal: 0,
            name: LIBRARY_PATH,
            source_sha256: "00",
            byte_len: u32::try_from(source.len()).expect("source length"),
            external_module: false,
            module: modules[0],
            manifest: &manifest,
            source_text: source,
        }],
        &[],
    )
    .expect("extract library product")
}

#[test]
fn checker_session_owns_product_binding_and_returns_real_check_results() {
    let product = Arc::new(extract_library_product("interface Box<T> { value: T }\n"));
    let run = run_prepared_project_frontend(
        vec![FileInput {
            name: USER_PATH.to_owned(),
            source: "const value: number = \"wrong\";\n".to_owned(),
        }],
        move |programs, source_reexports, default_modules| {
            CheckerSession::new(product).check_project_programs(
                programs,
                source_reexports,
                default_modules,
            )
        },
    );

    assert_eq!(run.parse_errors, [Vec::<String>::new()]);
    let results = run
        .product
        .expect("clean project reaches the session")
        .expect("project-local session checks");
    let [result] = results.as_slice() else {
        panic!("one user program must produce one real CheckResult")
    };
    assert_eq!(result.unit_slot.index(), 0);
    assert_eq!(result.module_ordinal.index(), 0);
    assert!(result.incomplete.is_empty());
    let [diagnostic] = result.diagnostics.as_slice() else {
        panic!("the session must return the user's one structured diagnostic")
    };
    assert_eq!(diagnostic.code.as_str(), "TK2322");
    assert_eq!(
        diagnostic.message,
        "Type 'string' is not assignable to type 'number'"
    );
}

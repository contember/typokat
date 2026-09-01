use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::SourceDeclarationManifest;
use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::binder::product::{project_library_declaration_product, LibraryProductSource};
use crate::check::checker::project_local::{
    observation_storage_counts_for_test, CheckerSession, ObservationEventKindForTest,
    ObservationScopeForTest, ProductLowererConstructionScopeForTest,
    ProductLoweringMetadataPlanConstructionScopeForTest,
};
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
    let observation = ObservationScopeForTest::start();
    let lowerer_constructions = ProductLowererConstructionScopeForTest::start();
    let metadata_plan_constructions = ProductLoweringMetadataPlanConstructionScopeForTest::start();
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
    let receipt = observation.finish();
    assert_eq!(
        receipt
            .events
            .iter()
            .filter(|event| matches!(
                event.kind,
                ObservationEventKindForTest::SessionCreated { .. }
            ))
            .count(),
        1,
        "ordinary checking creates exactly one semantic session"
    );
    assert_eq!(
        receipt
            .events
            .iter()
            .filter(|event| matches!(
                event.kind,
                ObservationEventKindForTest::SessionDropped { .. }
            ))
            .count(),
        1,
        "ordinary checking drops exactly one semantic session"
    );
    assert!(receipt
        .events
        .iter()
        .all(|event| !matches!(event.kind, ObservationEventKindForTest::ReceiptBuilt)));
    assert_eq!(receipt.session_weak.len(), 1);
    assert_eq!(receipt.cache_weak.len(), 1);
    assert!(receipt
        .session_weak
        .iter()
        .all(|witness| !witness.upgraded_at_finish));
    assert!(receipt
        .cache_weak
        .iter()
        .all(|witness| !witness.upgraded_at_finish));
    assert_eq!(
        lowerer_constructions.finish(),
        0,
        "a session without product demand must not construct a ProductLowerer"
    );
    assert_eq!(
        metadata_plan_constructions.finish(),
        0,
        "a session without product demand must not construct a metadata plan"
    );
}

#[test]
fn ordinary_preparation_copies_no_owned_product_group_names_for_scheduling() {
    const INDEPENDENT_GROUP_COUNT: usize = 24;
    let library_source = (0..INDEPENDENT_GROUP_COUNT)
        .map(|index| format!("interface Independent{index} {{ value: number }}\n"))
        .collect::<String>();
    let product = Arc::new(extract_library_product(&library_source));
    let run = run_prepared_project_frontend(
        vec![FileInput {
            name: USER_PATH.to_owned(),
            source: "const value: number = \"wrong\";\n".to_owned(),
        }],
        move |programs, source_reexports, default_modules| {
            let preparation = CheckerSession::new(Arc::clone(&product))
                .prepare_project_programs_for_test(programs, source_reexports, default_modules)?;
            let results = CheckerSession::new(product).check_project_programs(
                programs,
                source_reexports,
                default_modules,
            )?;
            Ok::<_, crate::check::checker::project_local::CheckerSessionError>((
                preparation,
                results,
            ))
        },
    );

    assert_eq!(run.parse_errors, [Vec::<String>::new()]);
    let (preparation, results) = run
        .product
        .expect("clean project reaches the session")
        .expect("ordinary preparation and checking succeed");
    assert_eq!(preparation.product_group_count, INDEPENDENT_GROUP_COUNT);
    assert_eq!(preparation.product_scc_count, INDEPENDENT_GROUP_COUNT);
    let [result] = results.as_slice() else {
        panic!("one user program must produce one real CheckResult")
    };
    assert!(result.incomplete.is_empty());
    let [diagnostic] = result.diagnostics.as_slice() else {
        panic!("the session must preserve the user's one structured diagnostic")
    };
    assert_eq!(diagnostic.code.as_str(), "TK2322");
    assert_eq!(
        preparation.owned_product_group_name_copy_count, 0,
        "ordinary preparation must schedule by numeric product identity without copying owned product-group names"
    );
}

#[test]
fn separate_generic_library_demands_reuse_one_metadata_plan_without_identity_collision() {
    let product = Arc::new(extract_library_product(
        "interface Box<T> { value: T }\ninterface Cell<T> { current: T }\n",
    ));
    let lowerer_constructions = ProductLowererConstructionScopeForTest::start();
    let metadata_plan_constructions = ProductLoweringMetadataPlanConstructionScopeForTest::start();
    let run = run_prepared_project_frontend(
        vec![FileInput {
            name: USER_PATH.to_owned(),
            source: r#"function identity<T>(value: T): T {
    return value;
}
const before: string = identity("before");
const box: Box<number> = { value: 1 };
const boxValue: number = box.value;
const between: string = identity("between");
const cell: Cell<string> = { current: "cell" };
const cellValue: string = cell.current;
const after: number = identity(42);
"#
            .to_owned(),
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
    assert!(result.incomplete.is_empty());
    assert!(result.diagnostics.is_empty());

    let lowerer_construction_count = lowerer_constructions.finish();
    let metadata_plan_construction_count = metadata_plan_constructions.finish();
    assert!(
        lowerer_construction_count >= 2,
        "the two independent demands must construct at least two real ProductLowerers, got {lowerer_construction_count}"
    );
    assert_ne!(
        metadata_plan_construction_count, 0,
        "the scope must count the real metadata-plan constructor"
    );
    assert_eq!(
        metadata_plan_construction_count, 1,
        "one immutable product-lowering metadata plan must serve the session; lowerers={lowerer_construction_count}, metadata_plans={metadata_plan_construction_count}"
    );
}

#[test]
fn ordinary_checker_sessions_do_not_retain_test_observations() {
    let product = Arc::new(extract_library_product("interface Box<T> { value: T }\n"));
    let counts_before = observation_storage_counts_for_test();

    for _ in 0..16 {
        let product = Arc::clone(&product);
        let run = run_prepared_project_frontend(
            vec![FileInput {
                name: USER_PATH.to_owned(),
                source: "const value: number = 1;\n".to_owned(),
            }],
            move |programs, source_reexports, default_modules| {
                CheckerSession::new(product).check_project_programs(
                    programs,
                    source_reexports,
                    default_modules,
                )
            },
        );
        assert!(
            run.product
                .expect("clean project reaches the session")
                .is_ok(),
            "ordinary project-local session checks"
        );
    }

    assert_eq!(observation_storage_counts_for_test(), counts_before);
}

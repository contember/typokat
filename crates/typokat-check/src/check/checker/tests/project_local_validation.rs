use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::SourceDeclarationManifest;
use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::binder::product::{project_library_declaration_product, LibraryProductSource};
use crate::check::checker::project_local::project_local_pass_port::{
    NormalizedCheckerRecordForTest, NormalizedDiagnosticForTest, NormalizedSpanForTest,
};
use crate::check::checker::project_local::{
    project_local_semantic_candidate_for_test, ProjectLocalLibraryRecordForTest,
    ProjectLocalOrderForTest, ProjectLocalProductGroupForTest, ProjectLocalSemanticRequestForTest,
    ProjectLocalSemanticTraceForTest,
};
use crate::check::checker::{
    bind_library_product_project_programs, LibraryProductProjectBindingError,
};
use crate::frontend::{run_prepared_project_frontend, FileInput};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::sync::Arc;
use typokat_library_product::LibraryDeclarationProduct;

const LIBRARY_PATH: &str = "project-local-validation.d.ts";
const USER_PATH: &str = "/project/main.ts";

fn extract_library_product(source: &str) -> LibraryDeclarationProduct {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "source fixture parses cleanly"
    );
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
        .expect("bind source-extracted product");
    let binder = builder.finish(modules[0]);
    project_library_declaration_product(
        "project-local-validation",
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
    .expect("extract source product")
}

fn candidate_trace(
    case_id: &'static str,
    library_source: &'static str,
    user_source: &'static str,
    force_all: bool,
) -> ProjectLocalSemanticTraceForTest {
    let product = Arc::new(extract_library_product(library_source));
    let candidate_product = Arc::clone(&product);
    let run = run_prepared_project_frontend(
        vec![FileInput {
            name: USER_PATH.to_owned(),
            source: user_source.to_owned(),
        }],
        move |programs, source_reexports, default_modules| {
            let (library_units, bound) = bind_library_product_project_programs(
                &candidate_product,
                programs,
                source_reexports,
                default_modules,
            )?;
            Ok::<_, LibraryProductProjectBindingError>(project_local_semantic_candidate_for_test(
                &candidate_product,
                library_units,
                bound,
                programs,
                ProjectLocalSemanticRequestForTest {
                    case_id,
                    source_order: ProjectLocalOrderForTest::Forward,
                    demand_order: ProjectLocalOrderForTest::Forward,
                    demand_roots: Vec::new(),
                    force_all,
                    fault: None,
                },
            ))
        },
    );
    run.product
        .expect("prepared project")
        .expect("product-backed project binder")
        .expect("project-local semantic candidate")
}

fn group_names(groups: &[ProjectLocalProductGroupForTest]) -> Vec<&str> {
    let mut names = groups
        .iter()
        .map(|group| group.name.as_str())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn exact_diagnostic(
    source: &str,
    owner: &str,
    site: &str,
    code: &str,
    message: &str,
) -> ProjectLocalLibraryRecordForTest {
    let owner_start = u32::try_from(source.find(owner).expect("owner fixture text"))
        .expect("owner offset fits u32");
    let site_start =
        u32::try_from(source.find(site).expect("site fixture text")).expect("site offset fits u32");
    ProjectLocalLibraryRecordForTest {
        source_path: LIBRARY_PATH.to_owned(),
        owner_start,
        record: NormalizedCheckerRecordForTest::Diagnostic(NormalizedDiagnosticForTest {
            span: NormalizedSpanForTest {
                start: site_start,
                end: site_start + u32::try_from(site.len()).expect("site length fits u32"),
            },
            code: code.to_owned(),
            severity: "error".to_owned(),
            message: message.to_owned(),
            elaboration: Vec::new(),
        }),
    }
}

fn assert_no_user_records(trace: &ProjectLocalSemanticTraceForTest) {
    assert!(trace.user_event_ledger.is_empty());
    assert!(trace
        .user_check_results
        .iter()
        .all(|result| { result.diagnostics.is_empty() && result.incomplete.is_empty() }));
}

#[test]
fn force_all_schedules_ready_interface_heritage_validation_through_the_ordinary_pass() {
    const SOURCE: &str = "\
interface Base { value: string }\n\
interface Derived extends Base { value: number }\n\
interface Clean { stable: boolean }\n";
    let trace = candidate_trace("force-all-heritage", SOURCE, "const marker = 0;", true);
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(
        group_names(force_all.validation_roots_for_test()),
        ["Base", "Clean", "Derived"]
    );
    assert_eq!(
        force_all.library_records_for_test(),
        [exact_diagnostic(
            SOURCE,
            "Derived",
            "Derived",
            "TK2430",
            "Interface 'Derived' incorrectly extends interface 'Base'.",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn force_all_schedules_ready_generic_application_validation_through_the_ordinary_pass() {
    const SOURCE: &str = "\
interface Box<T extends string> { value: T }\n\
type Bad = Box<number>;\n\
interface Clean { stable: boolean }\n";
    let trace = candidate_trace("force-all-constraint", SOURCE, "const marker = 0;", true);
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(
        group_names(force_all.validation_roots_for_test()),
        ["Bad", "Box", "Clean"]
    );
    assert_eq!(
        force_all.library_records_for_test(),
        [exact_diagnostic(
            SOURCE,
            "Bad",
            "number",
            "TK2344",
            "Type 'number' does not satisfy the constraint 'string'.",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn ordinary_augmentation_schedules_only_affected_library_validation_roots() {
    const LIBRARY_SOURCE: &str = "\
interface Root { method(value: string): void }\n\
interface Affected extends Root { method(value: string): void }\n\
interface Unaffected { stable: boolean }\n";
    const USER_SOURCE: &str = "interface Root { method(value: number): void }\n";
    let trace = candidate_trace(
        "ordinary-augmentation-validation",
        LIBRARY_SOURCE,
        USER_SOURCE,
        false,
    );

    assert_eq!(
        group_names(trace.validation_roots_for_test()),
        ["Affected", "Root"]
    );
    assert_eq!(
        trace.library_records_for_test(),
        [exact_diagnostic(
            LIBRARY_SOURCE,
            "Affected",
            "Affected",
            "TK2430",
            "Interface 'Affected' incorrectly extends interface 'Root'.",
        )]
    );
    assert_eq!(
        group_names(&trace.materialized_product_groups),
        ["Affected", "Root"]
    );
    assert_no_user_records(&trace);
}

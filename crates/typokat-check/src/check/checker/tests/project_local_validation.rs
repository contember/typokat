use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::SourceDeclarationManifest;
use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::binder::product::{project_library_declaration_product, LibraryProductSource};
use crate::check::checker::project_local::project_local_pass_port::{
    project_local_final_product_dependencies, NormalizedCheckerRecordForTest,
    NormalizedDiagnosticForTest, NormalizedIncompleteForTest, NormalizedSpanForTest,
};
use crate::check::checker::project_local::{
    project_local_semantic_candidate_for_test, ProjectLocalForceAllAliasTerminalForTest,
    ProjectLocalForceAllIntrinsicKindForTest, ProjectLocalForceAllObjectShapeForTest,
    ProjectLocalForceAllReadySurfaceForTest, ProjectLocalForceAllTerminalForTest,
    ProjectLocalForceAllTypeKindForTest, ProjectLocalLibraryRecordForTest,
    ProjectLocalOrderForTest, ProjectLocalProductGroupForTest, ProjectLocalSemanticRequestForTest,
    ProjectLocalSemanticTraceForTest, ProjectLocalTerminalStateForTest,
    ProjectLocalUnavailableReasonForTest,
};
use crate::check::checker::project_local_semantics::{
    LibraryTypeDependencyReconstructionScopeForTest, LibraryTypeDependencyReconstructionWorkForTest,
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

fn exact_incomplete(
    source: &str,
    owner: &str,
    site: &str,
    id: &str,
    context: &str,
) -> ProjectLocalLibraryRecordForTest {
    let owner_start = u32::try_from(source.find(owner).expect("owner fixture text"))
        .expect("owner offset fits u32");
    let site_start = owner_start
        + u32::try_from(
            source[usize::try_from(owner_start).expect("owner offset fits usize")..]
                .find(site)
                .expect("site fixture text after owner"),
        )
        .expect("relative site offset fits u32");
    ProjectLocalLibraryRecordForTest {
        source_path: LIBRARY_PATH.to_owned(),
        owner_start,
        record: NormalizedCheckerRecordForTest::Incomplete(NormalizedIncompleteForTest {
            span: NormalizedSpanForTest {
                start: site_start,
                end: site_start + u32::try_from(site.len()).expect("site length fits u32"),
            },
            id: id.to_owned(),
            context: context.to_owned(),
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

fn diagnostics_only(
    records: &[ProjectLocalLibraryRecordForTest],
) -> Vec<ProjectLocalLibraryRecordForTest> {
    records
        .iter()
        .filter(|record| matches!(record.record, NormalizedCheckerRecordForTest::Diagnostic(_)))
        .cloned()
        .collect()
}

fn object_surface(
    property_count: usize,
    call_signature_count: usize,
    construct_signature_count: usize,
    has_string_index: bool,
    has_number_index: bool,
) -> ProjectLocalForceAllReadySurfaceForTest {
    ProjectLocalForceAllReadySurfaceForTest::Object(ProjectLocalForceAllObjectShapeForTest {
        property_count,
        call_signature_count,
        construct_signature_count,
        has_string_index,
        has_number_index,
    })
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
        group_names(&force_all.production_validation_roots),
        ["Base", "Clean", "Derived"]
    );
    assert_eq!(
        force_all.library_records.as_slice(),
        [exact_diagnostic(
            SOURCE,
            "Derived",
            "Derived",
            "TK2430",
            "Interface 'Derived' incorrectly extends interface 'Base'.",
        )]
    );
    assert_eq!(
        force_all
            .terminal_census
            .iter()
            .map(|row| {
                let ProjectLocalForceAllTerminalForTest::Ready {
                    type_surface,
                    value_surface,
                } = &row.terminal
                else {
                    panic!("synthetic force_all group is unavailable")
                };
                assert!(value_surface.is_none());
                (row.group.name.as_str(), type_surface.clone())
            })
            .collect::<Vec<_>>(),
        [
            ("Base", Some(object_surface(1, 0, 0, false, false))),
            ("Derived", Some(object_surface(1, 0, 0, false, false))),
            ("Clean", Some(object_surface(1, 0, 0, false, false))),
        ]
    );
    let mut probe_names = force_all
        .semantic_probes
        .global_types
        .iter()
        .map(|probe| probe.name.as_str())
        .collect::<Vec<_>>();
    probe_names.sort_unstable();
    assert_eq!(probe_names, ["Base", "Clean", "Derived"]);
    assert_no_user_records(&trace);
}

#[test]
fn force_all_projects_live_augmented_surfaces_and_typed_terminals() {
    const SOURCE: &str = "\
interface Live { libraryMember: string }\n\
type Broken = typeof globalThis;\n";
    let trace = candidate_trace(
        "force-all-live-published-state",
        SOURCE,
        "interface Live { userMember: number }",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(force_all.terminal_census.len(), 2);
    assert!(force_all.terminal_census.iter().any(|row| {
        row.group.name == "Live"
            && row.terminal
                == ProjectLocalForceAllTerminalForTest::Ready {
                    type_surface: Some(object_surface(2, 0, 0, false, false)),
                    value_surface: None,
                }
    }));
    assert!(force_all.terminal_census.iter().any(|row| {
        row.group.name == "Broken"
            && row.terminal
                == ProjectLocalForceAllTerminalForTest::Unavailable(
                    ProjectLocalUnavailableReasonForTest::UnsupportedProductRecipe,
                )
    }));
    assert_eq!(force_all.library_records.len(), 1);
    let NormalizedCheckerRecordForTest::Incomplete(incomplete) =
        &force_all.library_records[0].record
    else {
        panic!("unsupported alias must retain its production incomplete")
    };
    assert_eq!(incomplete.id, "annotation-lower/type-query/typeof");
    let live_probe = force_all
        .semantic_probes
        .global_types
        .iter()
        .find(|probe| probe.name == "Live")
        .expect("Live semantic probe");
    assert_eq!(live_probe.member_names, ["libraryMember", "userMember"]);
    assert_eq!(group_names(&trace.materialized_product_groups), ["Live"]);
    assert_eq!(
        group_names(&force_all.requested_product_groups),
        ["Broken", "Live"]
    );
    assert_eq!(
        group_names(&force_all.additional_materialized_product_groups),
        ["Broken"]
    );
    assert_eq!(force_all.additional_materialized_owner_work_count, 1);
    assert_no_user_records(&trace);
}

#[test]
fn force_all_terminal_receipt_distinguishes_every_live_surface_channel() {
    const SOURCE: &str = "\
interface CallableIndex {\n\
    (value: string): number;\n\
    [key: string]: boolean;\n\
}\n\
declare class Dual {\n\
    constructor(value: string);\n\
    instance: number;\n\
    static staticMember: string;\n\
}\n\
type ObjectAlias = { value: string };\n\
type PrimitiveAlias = string;\n\
declare function ValueOnly(value: string): number;\n\
type Broken = typeof globalThis;\n";
    let trace = candidate_trace(
        "force-all-terminal-shapes",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");
    let ready = |name: &str, has_type: bool| {
        let terminal = &force_all
            .terminal_census
            .iter()
            .find(|row| row.group.name == name && row.group.has_type == has_type)
            .unwrap_or_else(|| panic!("missing {name} terminal"))
            .terminal;
        let ProjectLocalForceAllTerminalForTest::Ready {
            type_surface,
            value_surface,
        } = terminal
        else {
            panic!("{name} must be ready")
        };
        (type_surface.clone(), value_surface.clone())
    };

    assert_eq!(
        force_all.terminal_census.len(),
        force_all.requested_product_groups.len()
    );
    assert_eq!(
        ready("CallableIndex", true),
        (Some(object_surface(0, 1, 0, true, false)), None)
    );
    assert_eq!(
        ready("Dual", true),
        (
            Some(object_surface(1, 0, 0, false, false)),
            Some(object_surface(2, 0, 1, false, false)),
        )
    );
    assert_eq!(
        ready("ObjectAlias", true),
        (
            Some(ProjectLocalForceAllReadySurfaceForTest::Alias(
                ProjectLocalForceAllAliasTerminalForTest::Ready {
                    parameter_count: 0,
                    body: Box::new(object_surface(1, 0, 0, false, false)),
                },
            )),
            None,
        )
    );
    assert_eq!(
        ready("PrimitiveAlias", true),
        (
            Some(ProjectLocalForceAllReadySurfaceForTest::Alias(
                ProjectLocalForceAllAliasTerminalForTest::Ready {
                    parameter_count: 0,
                    body: Box::new(ProjectLocalForceAllReadySurfaceForTest::Other(
                        ProjectLocalForceAllTypeKindForTest::Intrinsic(
                            ProjectLocalForceAllIntrinsicKindForTest::String,
                        ),
                    )),
                },
            )),
            None,
        )
    );
    assert_eq!(
        ready("ValueOnly", false),
        (None, Some(object_surface(0, 1, 0, false, false)))
    );
    assert!(force_all.terminal_census.iter().any(|row| {
        row.group.name == "Broken"
            && row.terminal
                == ProjectLocalForceAllTerminalForTest::Unavailable(
                    ProjectLocalUnavailableReasonForTest::UnsupportedProductRecipe,
                )
    }));
    assert_no_user_records(&trace);
}

#[test]
fn class_value_failure_walks_every_static_and_constructor_member_before_atomic_unavailability() {
    const STATIC_FIRST: &str = "\
declare class Atomic {\n\
    static keep: string;\n\
    static broken: typeof globalThis;\n\
    constructor(value: typeof globalThis);\n\
}\n";
    const CONSTRUCTOR_FIRST: &str = "\
declare class Atomic {\n\
    constructor(value: typeof globalThis);\n\
    static broken: typeof globalThis;\n\
    static keep: string;\n\
}\n";

    for (case_id, source, static_owner, constructor_owner) in [
        (
            "class-value-atomic-static-first",
            STATIC_FIRST,
            "static broken",
            "constructor",
        ),
        (
            "class-value-atomic-constructor-first",
            CONSTRUCTOR_FIRST,
            "static broken",
            "constructor",
        ),
    ] {
        let trace = candidate_trace(case_id, source, "const marker = 0;", true);
        let force_all = trace.force_all.as_ref().expect("force_all trace");
        let atomic = force_all
            .terminal_census
            .iter()
            .find(|row| row.group.name == "Atomic")
            .expect("Atomic terminal");
        assert_eq!(
            atomic.terminal,
            ProjectLocalForceAllTerminalForTest::Unavailable(
                ProjectLocalUnavailableReasonForTest::UnsupportedProductRecipe,
            )
        );
        let mut expected = vec![
            exact_incomplete(
                source,
                static_owner,
                "typeof globalThis",
                "annotation-lower/type-query/typeof",
                "typeof type query not lowered",
            ),
            exact_incomplete(
                source,
                constructor_owner,
                "typeof globalThis",
                "annotation-lower/type-query/typeof",
                "typeof type query not lowered",
            ),
        ];
        expected.sort();
        assert_eq!(force_all.library_records, expected);
        let writes = trace
            .durable_cache_writes
            .iter()
            .filter(|write| write.owner.name == "Atomic")
            .collect::<Vec<_>>();
        assert_eq!(writes.len(), 1);
        assert_eq!(
            writes[0].state,
            ProjectLocalTerminalStateForTest::Unavailable(
                ProjectLocalUnavailableReasonForTest::UnsupportedProductRecipe,
            )
        );
        assert!(force_all
            .semantic_probes
            .global_types
            .iter()
            .all(|probe| probe.name != "Atomic"));
        assert!(force_all
            .semantic_probes
            .global_values
            .iter()
            .all(|probe| probe.name != "Atomic"));
        assert_no_user_records(&trace);
    }
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
        group_names(&force_all.production_validation_roots),
        ["Bad", "Box", "Clean"]
    );
    assert_eq!(
        force_all.library_records.as_slice(),
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
fn force_all_validates_a_no_own_member_conflict_between_two_base_interfaces() {
    const SOURCE: &str = "\
interface L { first: string; second: boolean }\n\
interface R { first: number; second: string }\n\
interface D extends L, R {}\n";
    let trace = candidate_trace(
        "force-all-base-pair-conflict",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(
        force_all.library_records.as_slice(),
        [exact_diagnostic(
            SOURCE,
            "D",
            "D",
            "TK2320",
            "Interface cannot simultaneously extend types 'L' and 'R'.",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn force_all_validates_conflicting_inherited_string_index_signatures() {
    const SOURCE: &str = "\
interface TextIndex { [key: string]: string }\n\
interface NumberIndex { [key: string]: number }\n\
interface D extends TextIndex, NumberIndex {}\n";
    let trace = candidate_trace(
        "force-all-inherited-index-conflict",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(
        force_all.library_records.as_slice(),
        [exact_diagnostic(
            SOURCE,
            "D",
            "D",
            "TK2430",
            "Interface 'D' incorrectly extends interface 'NumberIndex'.",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn ordinary_heritage_validation_uses_a_final_augmented_generic_argument_surface() {
    const LIBRARY_SOURCE: &str = "\
interface Arg { stable: string }\n\
interface Fixed { stable: string }\n\
interface Base<T> { value: T }\n\
interface Derived extends Base<Arg> { value: Fixed }\n";
    const USER_SOURCE: &str = "interface Arg { required: number }\n";
    let trace = candidate_trace(
        "ordinary-final-heritage-argument",
        LIBRARY_SOURCE,
        USER_SOURCE,
        false,
    );

    assert_eq!(
        trace.production_library_records.as_slice(),
        [exact_diagnostic(
            LIBRARY_SOURCE,
            "Derived",
            "Derived",
            "TK2430",
            "Interface 'Derived' incorrectly extends interface 'Base'.",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn ordinary_heritage_validation_uses_a_user_added_final_own_property() {
    const LIBRARY_SOURCE: &str = "\
interface Base { x: string }\n\
interface D extends Base {}\n";
    const USER_SOURCE: &str = "interface D { x: number }\n";
    let trace = candidate_trace(
        "ordinary-final-own-property",
        LIBRARY_SOURCE,
        USER_SOURCE,
        false,
    );

    assert_eq!(
        trace.production_library_records.as_slice(),
        [exact_diagnostic(
            LIBRARY_SOURCE,
            "D",
            "D",
            "TK2430",
            "Interface 'D' incorrectly extends interface 'Base'.",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn user_added_final_own_property_suppresses_base_pair_conflict_and_checks_each_base() {
    const LIBRARY_SOURCE: &str = "\
interface L { value: string }\n\
interface R { value: number }\n\
interface D extends L, R {}\n";
    const USER_SOURCE: &str = "interface D { value: boolean }\n";
    let trace = candidate_trace(
        "ordinary-final-own-property-base-pair",
        LIBRARY_SOURCE,
        USER_SOURCE,
        false,
    );

    assert_eq!(
        trace.production_library_records.as_slice(),
        [
            exact_diagnostic(
                LIBRARY_SOURCE,
                "D",
                "D",
                "TK2430",
                "Interface 'D' incorrectly extends interface 'L'.",
            ),
            exact_diagnostic(
                LIBRARY_SOURCE,
                "D",
                "D",
                "TK2430",
                "Interface 'D' incorrectly extends interface 'R'.",
            ),
        ]
    );
    assert_no_user_records(&trace);
}

#[test]
fn user_inherited_property_does_not_suppress_a_product_base_pair_conflict() {
    const LIBRARY_SOURCE: &str = "\
interface L { value: string }\n\
interface R { value: number }\n\
interface D extends L, R {}\n";
    const USER_SOURCE: &str = "\
interface U { value: boolean }\n\
interface D extends U {}\n";
    let trace = candidate_trace(
        "ordinary-user-inherited-property-is-not-own",
        LIBRARY_SOURCE,
        USER_SOURCE,
        false,
    );

    assert_eq!(
        trace.production_library_records.as_slice(),
        [exact_diagnostic(
            LIBRARY_SOURCE,
            "D",
            "D",
            "TK2320",
            "Interface cannot simultaneously extend types 'L' and 'R'.",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn force_all_reports_an_unresolved_qualified_type_root_from_product_resolution() {
    const SOURCE: &str = "interface D extends Missing.Root {}\n";
    let trace = candidate_trace(
        "force-all-qualified-missing-root",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(
        diagnostics_only(&force_all.library_records),
        [exact_diagnostic(
            SOURCE,
            "D",
            "Missing",
            "TK2503",
            "Cannot find namespace 'Missing'.",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn qualified_heritage_diagnostic_uses_the_actual_interface_fragment_owner() {
    const SOURCE: &str = "\
declare class Mixed {}\n\
interface Mixed extends Missing.Root {}\n";
    let trace = candidate_trace(
        "force-all-qualified-mixed-fragment-owner",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");
    let mut expected = exact_diagnostic(
        SOURCE,
        "Mixed",
        "Missing",
        "TK2503",
        "Cannot find namespace 'Missing'.",
    );
    expected.owner_start = u32::try_from(SOURCE.rfind("Mixed").expect("interface binding"))
        .expect("owner offset fits u32");

    assert_eq!(diagnostics_only(&force_all.library_records), [expected]);
    assert_no_user_records(&trace);
}

#[test]
fn force_all_reports_an_unresolved_bare_type_name_from_product_resolution() {
    const SOURCE: &str = "type D = typeof Missing;\n";
    let trace = candidate_trace(
        "force-all-bare-missing-root",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(
        diagnostics_only(&force_all.library_records),
        [exact_diagnostic(
            SOURCE,
            "D",
            "Missing",
            "TK2304",
            "Cannot find name 'Missing'",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn force_all_reports_each_declaration_type_parameter_qualified_failure_once() {
    const SOURCE: &str = "\
interface Box<T extends MissingConstraint = MissingDefault> { value: T }\n\
interface A { value: Box }\n\
interface B { value: Box }\n";
    let trace = candidate_trace(
        "force-all-declaration-type-parameter-qualified-failures",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(
        diagnostics_only(&force_all.library_records),
        [
            exact_diagnostic(
                SOURCE,
                "Box",
                "MissingConstraint",
                "TK2304",
                "Cannot find name 'MissingConstraint'",
            ),
            exact_diagnostic(
                SOURCE,
                "Box",
                "MissingDefault",
                "TK2304",
                "Cannot find name 'MissingDefault'",
            ),
        ]
    );
    assert_no_user_records(&trace);
}

#[test]
fn force_all_reports_later_merged_fragment_type_parameter_failures_once() {
    const SOURCE: &str = "\
interface Box<T> { first: T }\n\
interface Box<U extends MissingConstraint = MissingDefault> { second: U }\n\
interface A { value: Box<string> }\n\
interface B { value: Box<string> }\n";
    let trace = candidate_trace(
        "force-all-later-fragment-type-parameter-failures",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");
    let second_fragment_owner = u32::try_from(
        SOURCE
            .match_indices("Box")
            .nth(1)
            .expect("second Box declaration")
            .0,
    )
    .expect("owner offset fits u32");
    let mut constraint = exact_diagnostic(
        SOURCE,
        "Box",
        "MissingConstraint",
        "TK2304",
        "Cannot find name 'MissingConstraint'",
    );
    constraint.owner_start = second_fragment_owner;
    let mut default = exact_diagnostic(
        SOURCE,
        "Box",
        "MissingDefault",
        "TK2304",
        "Cannot find name 'MissingDefault'",
    );
    default.owner_start = second_fragment_owner;

    assert_eq!(
        diagnostics_only(&force_all.library_records),
        [constraint, default]
    );
    assert_no_user_records(&trace);
}

#[test]
fn constraint_qualified_failure_uses_its_declaration_owner_before_finalization() {
    const SOURCE: &str = "interface Box<T extends Missing> { m(args: [...T]): void }\n";
    let trace = candidate_trace(
        "force-all-constraint-qualified-owner-before-finalization",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(
        diagnostics_only(&force_all.library_records),
        [exact_diagnostic(
            SOURCE,
            "Box",
            "Missing",
            "TK2304",
            "Cannot find name 'Missing'",
        )]
    );
    assert_no_user_records(&trace);
}

#[test]
fn contextual_signature_tuple_rest_constraint_uses_the_member_owner_fallback() {
    const SOURCE: &str = "interface I { m<T extends any[]>(x: [...T]): void }\n";
    let trace = candidate_trace(
        "force-all-contextual-signature-tuple-rest-owner",
        SOURCE,
        "const marker = 0;",
        true,
    );
    let force_all = trace.force_all.as_ref().expect("force_all trace");

    assert_eq!(group_names(&force_all.production_validation_roots), ["I"]);
    assert!(force_all.library_records.is_empty());
    assert_no_user_records(&trace);
}

#[test]
fn non_augmenting_user_class_dependency_skips_library_dependency_reconstruction() {
    const LIBRARY_SOURCE: &str = "\
declare class LibraryBase { value: string }\n\
declare class LibraryDerived extends LibraryBase { stable: boolean }\n";
    const USER_SOURCE: &str = "class UserDerived extends LibraryDerived {}\n";
    let work = LibraryTypeDependencyReconstructionScopeForTest::start();
    let trace = candidate_trace(
        "non-augmenting-user-class-dependency",
        LIBRARY_SOURCE,
        USER_SOURCE,
        false,
    );
    let work = work.finish();

    assert_no_user_records(&trace);
    assert!(trace.production_validation_roots.is_empty());
    let mut demanded_groups = trace
        .publication_scc_commits
        .iter()
        .flat_map(|commit| commit.scc.members.iter())
        .map(|group| group.name.as_str())
        .collect::<Vec<_>>();
    demanded_groups.sort_unstable();
    demanded_groups.dedup();
    assert_eq!(demanded_groups, ["LibraryBase", "LibraryDerived"]);
    assert_eq!(
        work,
        LibraryTypeDependencyReconstructionWorkForTest::default(),
        "non-augmenting user demand must not rebuild library-to-library dependencies"
    );
}

#[test]
fn ordinary_augmentation_schedules_only_affected_library_validation_roots() {
    const LIBRARY_SOURCE: &str = "\
interface Root { method(value: string): void }\n\
interface Affected extends Root { method(value: string): void }\n\
interface Unaffected { stable: boolean }\n";
    const USER_SOURCE: &str = "interface Root { method(value: number): void }\n";
    let work = LibraryTypeDependencyReconstructionScopeForTest::start();
    let trace = candidate_trace(
        "ordinary-augmentation-validation",
        LIBRARY_SOURCE,
        USER_SOURCE,
        false,
    );
    let work = work.finish();

    assert_eq!(
        work,
        LibraryTypeDependencyReconstructionWorkForTest {
            invocations: 1,
            product_dependency_rows: 1,
        }
    );
    assert_eq!(
        group_names(&trace.production_validation_roots),
        ["Affected", "Root"]
    );
    assert_eq!(
        trace.production_library_records.as_slice(),
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

#[test]
fn final_validation_graph_includes_a_user_added_library_dependency_edge() {
    const LIBRARY_SOURCE: &str = "\
interface Root { value: string }\n\
interface Affected { stable: boolean }\n";
    const USER_SOURCE: &str = "interface Affected { linked: Root }\n";
    let product = Arc::new(extract_library_product(LIBRARY_SOURCE));
    let affected = product
        .groups()
        .iter()
        .enumerate()
        .find(|(_, group)| product.strings()[group.name.index()] == "Affected")
        .map(|(index, _)| index)
        .expect("Affected product group");
    let root = product
        .groups()
        .iter()
        .enumerate()
        .find(|(_, group)| product.strings()[group.name.index()] == "Root")
        .map(|(index, _)| index)
        .expect("Root product group");
    assert!(product.dependency_rows().iter().all(|dependency| {
        dependency.owner.index() != affected
            || !matches!(
                dependency.target,
                typokat_library_product::DependencyTarget::Group(target)
                    if target.index() == root
            )
    }));

    let candidate_product = Arc::clone(&product);
    let run = run_prepared_project_frontend(
        vec![FileInput {
            name: USER_PATH.to_owned(),
            source: USER_SOURCE.to_owned(),
        }],
        move |programs, source_reexports, default_modules| {
            let (_library_units, bound) = bind_library_product_project_programs(
                &candidate_product,
                programs,
                source_reexports,
                default_modules,
            )
            .map_err(|error| format!("{error:?}"))?;
            project_local_final_product_dependencies(&candidate_product, &bound, programs)
                .map_err(str::to_owned)
        },
    );
    let dependencies = run
        .product
        .expect("prepared project")
        .expect("product-backed project binder");
    assert!(dependencies.contains(&(affected, root)));
}

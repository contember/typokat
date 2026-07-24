//! RED contract for declaration-backed interface-property and namespace-variable types.
//!
//! Member names and modifiers remain eager. Only eligible annotation bodies become
//! AST-free recipes and materialize through semantic demand.

use super::declaration_surface_measure::{
    declaration_surface_measure, start_declaration_surface_measure, DeclarationSurfaceMeasure,
};
use super::eval::DEFAULT_STEP_BUDGET;
use super::library_compiler::{compile_owned_injected_profile, InjectedLibrarySource};
use super::reporting_record::CheckerRecord;
use crate::diagnostics::DiagnosticCode;
use crate::driver::CheckOutput;
use crate::source::LibraryFileOrdinal;
use crate::types::repr::DeclaredRecipeNode;
use crate::types::store::TypeId;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::BTreeSet;

const SMALL_PADDING: usize = 8;
const LARGE_PADDING: usize = 128;

struct MeasuredRun {
    source: String,
    output: CheckOutput,
    measure: DeclarationSurfaceMeasure,
    recipes: RecipeArenaMeasure,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RecipeArenaMeasure {
    durable_rows: usize,
    durable_edges: usize,
    reachable_rows: usize,
    reachable_edges: usize,
}

fn recipe_arena_measure(interner: &crate::types::Interner) -> RecipeArenaMeasure {
    let store = interner.store();
    let recipes = store
        .snapshot_declared_recipes()
        .map(|(id, recipe)| (id, recipe.node.clone()))
        .collect::<Vec<_>>();
    let mut pending = (0..store.len())
        .filter_map(|index| {
            store
                .declared_type(TypeId(u32::try_from(index).expect("store index fits u32")))
                .map(|declared| declared.recipe)
        })
        .collect::<Vec<_>>();
    let mut reachable = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let recipe = store
            .declared_recipe(id)
            .expect("declared root references a durable recipe");
        match &recipe.node {
            DeclaredRecipeNode::Type(_) => {}
            DeclaredRecipeNode::Array(child) | DeclaredRecipeNode::Readonly(child) => {
                pending.push(*child);
            }
            DeclaredRecipeNode::Tuple { elements, rest } => {
                pending.extend(elements.iter().copied());
                pending.extend(rest.iter().map(|(_, recipe)| *recipe));
            }
            DeclaredRecipeNode::Application { arguments, .. } => {
                pending.extend(arguments.iter().copied());
            }
        }
    }
    let edges = |node: &DeclaredRecipeNode| match node {
        DeclaredRecipeNode::Type(_) => 0,
        DeclaredRecipeNode::Array(_) | DeclaredRecipeNode::Readonly(_) => 1,
        DeclaredRecipeNode::Tuple { elements, rest } => {
            elements.len() + usize::from(rest.is_some())
        }
        DeclaredRecipeNode::Application { arguments, .. } => arguments.len(),
    };
    RecipeArenaMeasure {
        durable_rows: recipes.len(),
        durable_edges: recipes.iter().map(|(_, node)| edges(node)).sum(),
        reachable_rows: reachable.len(),
        reachable_edges: recipes
            .iter()
            .filter(|(id, _)| reachable.contains(id))
            .map(|(_, node)| edges(node))
            .sum(),
    }
}

fn check_measured(source: String) -> MeasuredRun {
    assert_eq!(declaration_surface_measure(), None);
    let guard = start_declaration_surface_measure();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
    let parse_errors = parsed.diagnostics.iter().map(ToString::to_string).collect();
    let mut interner = crate::types::Interner::with_intrinsics();
    let checked = super::check_program(&mut interner, &parsed.program);
    let recipes = recipe_arena_measure(&interner);
    let output = CheckOutput {
        diagnostics: checked.diagnostics,
        parse_errors,
        incomplete: checked.incomplete,
    };
    let measure =
        declaration_surface_measure().expect("declaration-surface measure remains active");
    drop(guard);
    assert_eq!(declaration_surface_measure(), None);
    MeasuredRun {
        source,
        output,
        measure,
        recipes,
    }
}

fn existing_reference_surface_source(properties: usize) -> String {
    let mut source = String::from("interface Existing {}\ninterface ExistingSurface {\n");
    for index in 0..properties {
        source.push_str(&format!("  p{index:04}: Existing;\n"));
    }
    source.push_str("}\n");
    source
}

fn mixed_existing_and_deferred_surface_source(existing_properties: usize) -> String {
    let mut source = String::from(
        "interface Existing {}\n\
         interface Box<T> {}\n\
         interface MixedSurface {\n",
    );
    for index in 0..existing_properties {
        source.push_str(&format!("  p{index:04}: Existing;\n"));
    }
    source.push_str(
        "  strings: Box<string[]>;\n\
         numbers: Box<number[]>;\n\
         }\n",
    );
    source
}

#[test]
fn zero_argument_existing_references_do_not_create_recipes_or_eager_relower() {
    const EXISTING_REFERENCE_ROOTS: usize = 4_072;

    let run = check_measured(existing_reference_surface_source(EXISTING_REFERENCE_ROOTS));
    assert_clean(&run);
    assert_eq!(
        run.measure.eligible_interface_property_roots,
        u64::try_from(EXISTING_REFERENCE_ROOTS).expect("root count fits u64")
    );
    assert_eq!(
        run.measure.eager_materialization_roots, 0,
        "an already-frozen zero-argument interface root must be reused directly"
    );
    assert_eq!(
        run.recipes,
        RecipeArenaMeasure::default(),
        "existing roots must add neither durable recipe rows nor recipe dependency edges"
    );
}

#[test]
fn durable_recipe_work_scales_with_deferred_nodes_not_existing_roots() {
    let small = check_measured(mixed_existing_and_deferred_surface_source(8));
    let large = check_measured(mixed_existing_and_deferred_surface_source(512));
    assert_clean(&small);
    assert_clean(&large);

    let expected = RecipeArenaMeasure {
        durable_rows: 6,
        durable_edges: 4,
        reachable_rows: 6,
        reachable_edges: 4,
    };
    assert_eq!(small.recipes, expected, "small={:#?}", small.recipes);
    assert_eq!(large.recipes, expected, "large={:#?}", large.recipes);
    assert_eq!(
        small.measure.eager_materialization_roots, 0,
        "small mixed surface must classify Existing without eager relowering"
    );
    assert_eq!(
        large.measure.eager_materialization_roots, 0,
        "existing-root padding must not add eager lowering work"
    );
}

#[test]
fn partial_recipe_failure_commits_no_rows_or_dependency_edges() {
    let source = r#"interface PartialFailureSurface {
  first: [string[], number | boolean];
  second: MissingSecond;
  third: [boolean[], string | number];
  fourth: MissingFourth;
}
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(
        run.output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                &run.source[diagnostic.span.range()],
                diagnostic.message.as_str(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                DiagnosticCode::TK2304,
                "MissingSecond",
                "Cannot find name 'MissingSecond'",
            ),
            (
                DiagnosticCode::TK2304,
                "MissingFourth",
                "Cannot find name 'MissingFourth'",
            ),
        ],
        "fallback diagnostics and source order are part of the transaction contract"
    );
    assert_eq!(
        run.recipes,
        RecipeArenaMeasure::default(),
        "a failed root plan must publish neither partial rows nor dependency edges"
    );
}

fn assert_clean(run: &MeasuredRun) {
    assert!(
        run.output.parse_errors.is_empty(),
        "parse={:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.diagnostics.is_empty(),
        "diagnostics={:?}",
        run.output.diagnostics
    );
    assert!(
        run.output.incomplete.is_empty(),
        "incomplete={:?}",
        run.output.incomplete
    );
}

fn padded_surface_source(padding: usize) -> String {
    let mut source = String::from(
        "interface WideSurface {\n\
           selectedInterface: string[];\n",
    );
    for index in 0..padding {
        source.push_str(&format!("  unusedInterface{index:03}: number[];\n"));
    }
    source.push_str(
        "}\n\
         declare namespace WideNamespace {\n\
           const selectedNamespace: [number, string];\n",
    );
    for index in 0..padding {
        source.push_str(&format!(
            "  const unusedNamespace{index:03}: readonly string[];\n"
        ));
    }
    source.push_str(
        "}\n\
         declare const wide: WideSurface;\n\
         const selectedInterface: string[] = wide.selectedInterface;\n\
         const selectedNamespace: [number, string] = WideNamespace.selectedNamespace;\n",
    );
    source
}

#[test]
fn widening_unused_eligible_annotations_does_not_increase_materialization_work() {
    let small = check_measured(padded_surface_source(SMALL_PADDING));
    let large = check_measured(padded_surface_source(LARGE_PADDING));
    assert_clean(&small);
    assert_clean(&large);

    assert_eq!(
        small.measure.eligible_interface_property_roots,
        u64::try_from(SMALL_PADDING + 1).expect("padding fits u64")
    );
    assert_eq!(
        large.measure.eligible_interface_property_roots,
        u64::try_from(LARGE_PADDING + 1).expect("padding fits u64")
    );
    assert_eq!(
        small.measure.eligible_namespace_variable_roots,
        u64::try_from(SMALL_PADDING + 1).expect("padding fits u64")
    );
    assert_eq!(
        large.measure.eligible_namespace_variable_roots,
        u64::try_from(LARGE_PADDING + 1).expect("padding fits u64")
    );
    assert_eq!(
        large.measure.materialization_roots(),
        small.measure.materialization_roots(),
        "unused declaration padding must add recipe planning, not semantic materialization: small={:#?}, large={:#?}",
        small.measure,
        large.measure
    );
    assert_eq!(
        large.measure.materialization_memo_hits,
        small.measure.materialization_memo_hits
    );
    assert_eq!(
        large.measure.materialization_memo_inserts,
        small.measure.materialization_memo_inserts
    );
}

#[test]
fn only_selected_interface_and_namespace_recipes_materialize() {
    let run = check_measured(padded_surface_source(SMALL_PADDING));
    assert_clean(&run);
    assert_eq!(
        run.measure.eligible_roots(),
        u64::try_from(2 * (SMALL_PADDING + 1)).expect("padding fits u64")
    );
    assert_eq!(
        run.measure.eager_materialization_roots, 0,
        "eligible declaration roots must not use the eager annotation lowerer"
    );
    assert_eq!(
        run.measure.demand_materialization_roots, 2,
        "the selected interface property and namespace variable are the only demanded recipes"
    );
    assert_eq!(run.measure.materialization_roots(), 2);
    assert!(run.measure.materialization_memo_inserts <= 2);
}

#[test]
fn eager_member_names_optional_read_shape_and_readonly_metadata_are_preserved() {
    let source = r#"interface MetadataSurface {
  readonly selected?: string[];
}
declare namespace MetadataNamespace {
  const selected: string[];
}
declare const metadata: MetadataSurface;
type MetadataKeys = keyof MetadataSurface;
const key: MetadataKeys = "selected";
metadata.selected = [];
MetadataNamespace.selected = [];
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(
        run.output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                &run.source[diagnostic.span.range()],
                diagnostic.message.as_str(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                DiagnosticCode::TK2540,
                "selected",
                "Cannot assign to 'selected' because it is a read-only property",
            ),
            (
                DiagnosticCode::TK2540,
                "selected",
                "Cannot assign to 'selected' because it is a read-only property",
            ),
        ]
    );
}

#[test]
fn mapper_specialized_interface_properties_do_not_alias() {
    let source = r#"interface Box<T> {
  value: T;
}
interface UsesBoxes {
  text: Box<string>;
  count: Box<number>;
}
declare const boxes: UsesBoxes;
const text: string = boxes.text.value;
const count: number = boxes.count.value;
const mismatch: number = boxes.text.value;
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(run.output.diagnostics.len(), 1);
    let diagnostic = &run.output.diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::TK2322);
    assert_eq!(&run.source[diagnostic.span.range()], "mismatch");
    assert_eq!(
        diagnostic.message,
        "Type 'string' is not assignable to type 'number'"
    );
}

#[test]
fn nested_excess_property_through_declared_application_is_retained() {
    let source = r#"interface Box<T> {
  value: T;
}
interface Outer {
  inner: Box<number>;
}
const outer: Outer = {
  inner: {
    value: 1,
    extra: true,
  },
};
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(run.output.diagnostics.len(), 1);
    assert_eq!(run.output.diagnostics[0].code, DiagnosticCode::TK2353);
    assert_eq!(&run.source[run.output.diagnostics[0].span.range()], "extra");
}

#[test]
fn undemanded_declared_application_diagnostics_render_specialized_arguments() {
    let source = r#"interface Box<T> {
  value: T;
}
interface Outer {
  text: Box<string>;
  count: Box<number>;
}
const outer: Outer = {
  text: {
    value: "ok",
    textExtra: true,
  },
  count: {
    value: 1,
    countExtra: true,
  },
};
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(
        run.output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                &run.source[diagnostic.span.range()],
                diagnostic.message.as_str(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                DiagnosticCode::TK2353,
                "textExtra",
                "Object literal may only specify known properties, and 'textExtra' does not exist in type '{ value: string }'.",
            ),
            (
                DiagnosticCode::TK2353,
                "countExtra",
                "Object literal may only specify known properties, and 'countExtra' does not exist in type '{ value: number }'.",
            ),
        ]
    );
}

#[test]
fn direct_declared_application_assignment_renders_specialized_arguments() {
    let source = r#"interface Box<T> {
  value: T;
}
interface UsesBoxes {
  text: Box<string>;
}
declare const boxes: UsesBoxes;
const mismatch: Box<number> = boxes.text;
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(run.output.diagnostics.len(), 1);
    assert_eq!(run.output.diagnostics[0].code, DiagnosticCode::TK2322);
    assert_eq!(
        run.output.diagnostics[0].message,
        "Type '{ value: string }' is not assignable to type '{ value: number }'"
    );
}

#[test]
fn nested_declared_application_assignment_renders_specialized_arguments() {
    let source = r#"interface Leaf<T> {
  value: T;
}
interface Mid<T> {
  leaf: Leaf<T>;
}
interface Holder {
  mid: Mid<string>;
}
declare const holder: Holder;
const mismatch: Mid<number> = holder.mid;
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(run.output.diagnostics.len(), 1);
    assert_eq!(run.output.diagnostics[0].code, DiagnosticCode::TK2322);
    assert_eq!(
        run.output.diagnostics[0].message,
        "Type '{ leaf: { value: string } }' is not assignable to type '{ leaf: { value: number } }'"
    );
}

#[test]
fn declared_application_index_signature_renders_specialized_arguments() {
    let source = r#"interface Dict<T> {
  [key: string]: T;
  value: T;
}
interface Holder {
  strings: Dict<string>;
}
declare const holder: Holder;
const mismatch: Dict<number> = holder.strings;
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(run.output.diagnostics.len(), 1);
    assert_eq!(run.output.diagnostics[0].code, DiagnosticCode::TK2322);
    assert_eq!(
        run.output.diagnostics[0].message,
        "Type '{ [x: string]: string; value: string }' is not assignable to type '{ [x: string]: number; value: number }'"
    );
}

#[test]
fn declared_application_call_and_construct_signatures_render_specialized_arguments() {
    let source = r#"interface Callable<T> {
  (value: T): T;
  new (value: T): { value: T };
  value: T;
}
interface Holder {
  callable: Callable<string>;
}
declare const holder: Holder;
const mismatch: Callable<number> = holder.callable;
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(run.output.diagnostics.len(), 1);
    assert_eq!(run.output.diagnostics[0].code, DiagnosticCode::TK2322);
    assert_eq!(
        run.output.diagnostics[0].message,
        "Type '{ (value: string): string; new (value: string): { value: string }; value: string }' is not assignable to type '{ (value: number): number; new (value: number): { value: number }; value: number }'"
    );
}

#[test]
fn earlier_excess_diagnostic_survives_later_child_demand_exhaustion() {
    let mut source = String::from("type Exhaustion0000 = { known: number };\n");
    for depth in 1..=DEFAULT_STEP_BUDGET + 1 {
        source.push_str(&format!(
            "type Exhaustion{depth:04} = number extends number ? Exhaustion{:04} : never;\n",
            depth - 1
        ));
    }
    source.push_str(&format!(
        "interface ExhaustionTarget {{\n\
           first: {{ known: number }};\n\
           later: Exhaustion{:04};\n\
         }}\n\
         const value: ExhaustionTarget = {{\n\
           first: {{ known: 1, extra: true }},\n\
           later: {{ known: 1 }},\n\
         }};\n",
        DEFAULT_STEP_BUDGET + 1
    ));

    let run = check_measured(source);
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::TK2589),
        "the later child must exercise semantic-demand exhaustion: {:?}",
        run.output.diagnostics
    );
    let excess = run
        .output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == DiagnosticCode::TK2353)
        .expect("the earlier source-ordered excess diagnostic must be retained");
    assert_eq!(&run.source[excess.span.range()], "extra");
}

#[test]
fn recursive_declared_property_terminates_without_cross_mapper_contamination() {
    let source = r#"interface RecursiveNode<T> {
  value: T;
  next: RecursiveNode<T>;
}
interface RecursiveRoots {
  text: RecursiveNode<string>;
  count: RecursiveNode<number>;
}
declare const roots: RecursiveRoots;
const text: string = roots.text.next.next.value;
const count: number = roots.count.next.next.value;
const mismatch: number = roots.text.next.value;
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(run.output.diagnostics.len(), 1);
    assert_eq!(run.output.diagnostics[0].code, DiagnosticCode::TK2322);
    assert_eq!(
        &run.source[run.output.diagnostics[0].span.range()],
        "mismatch"
    );
    assert_eq!(
        run.output.diagnostics[0].message,
        "Type 'string' is not assignable to type 'number'"
    );
}

#[test]
fn unused_invalid_declarations_keep_source_ordered_records() {
    let source = r#"declare const value: number;
interface BrokenSurface {
  first: MissingInterfaceType;
  second: typeof value;
}
declare namespace BrokenNamespace {
  const third: MissingNamespaceType;
  const fourth: typeof value;
}
"#;
    let (run, _state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "invalid.d.ts",
        source,
    }])
    .expect("invalid declarations complete through owned records");

    assert_eq!(run.library_records.len(), 4, "{:?}", run.library_records);
    assert!(run
        .library_records
        .windows(2)
        .all(|records| records[0].0 < records[1].0));
    let record_spans = run
        .library_records
        .iter()
        .map(|(_, record)| match record {
            CheckerRecord::Diagnostic(diagnostic) => {
                (diagnostic.span, diagnostic.code.as_str().to_string())
            }
            CheckerRecord::Incomplete(incomplete) => {
                (incomplete.span, format!("incomplete[{}]", incomplete.id))
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        record_spans
            .iter()
            .map(|(span, kind)| (&source[span.range()], kind.as_str()))
            .collect::<Vec<_>>(),
        [
            ("MissingInterfaceType", "TK2304"),
            (
                "typeof value",
                "incomplete[annotation-lower/type-query/typeof]"
            ),
            ("MissingNamespaceType", "TK2304"),
            (
                "typeof value",
                "incomplete[annotation-lower/type-query/typeof]"
            ),
        ]
    );
}

#[test]
fn merge_heritage_conflicts_and_declared_display_remain_byte_stable() {
    let source = r#"interface LeftSurface {
  conflict: string[];
}
interface RightSurface {
  conflict: number[];
}
interface ConflictedSurface extends LeftSurface, RightSurface {}
interface MergedSurface {
  repeated: string[];
}
interface MergedSurface {
  repeated: number[];
}
declare const left: LeftSurface;
const mismatch: number[] = left.conflict;
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(
        run.output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                &run.source[diagnostic.span.range()],
                diagnostic.message.as_str(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                DiagnosticCode::TK2320,
                "ConflictedSurface",
                "Interface cannot simultaneously extend types 'LeftSurface' and 'RightSurface'.",
            ),
            (
                DiagnosticCode::TK2717,
                "repeated: number[];",
                "Subsequent property declarations must have the same type. Property 'repeated' has a conflicting type.",
            ),
            (
                DiagnosticCode::TK2322,
                "mismatch",
                "Type 'string[]' is not assignable to type 'number[]'",
            ),
        ]
    );
}

#[test]
fn merged_interface_recovery_binders_preserve_invalid_arity_diagnostics() {
    let source = r#"interface Recovery<T> {
  first: T;
}
interface Recovery<U> {
  second: U;
}
type EagerBad = Recovery<number>;
interface Holder {
  lazyBad: Recovery<number>;
}
declare const recovered: Recovery<number, string>;
const first: number = recovered.first;
const second: string = recovered.second;
"#;
    let run = check_measured(source.to_string());
    assert!(
        run.output.parse_errors.is_empty(),
        "{:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "{:?}",
        run.output.incomplete
    );
    assert_eq!(
        run.output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                &run.source[diagnostic.span.range()],
                diagnostic.message.as_str(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                DiagnosticCode::TK2428,
                "Recovery",
                "All declarations of 'Recovery' must have identical type parameters",
            ),
            (
                DiagnosticCode::TK2428,
                "Recovery",
                "All declarations of 'Recovery' must have identical type parameters",
            ),
            (
                DiagnosticCode::TK2314,
                "Recovery<number>",
                "Generic type 'Recovery<T, U>' requires 2 type argument(s)",
            ),
            (
                DiagnosticCode::TK2314,
                "Recovery<number>",
                "Generic type 'Recovery<T, U>' requires 2 type argument(s)",
            ),
        ],
        "the lazy property must retain the eager reference's recovery arity and source order"
    );
}

fn nested_array_annotation(depth: usize) -> String {
    format!("number{}", "[]".repeat(depth))
}

fn assert_undemanded_declared_property_depth_contract(depth: usize) {
    const ANNOTATION_DEPTH_BUDGET: usize = 200;

    let annotation = nested_array_annotation(depth);
    let exhaustion_span = nested_array_annotation(depth - ANNOTATION_DEPTH_BUDGET);
    let source = format!(
        "type EagerDepth = {annotation};\n\
         interface LazyDepth {{\n\
           undemanded: {annotation};\n\
         }}\n\
         type Later = MissingAfterDepth;\n"
    );
    let run = check_measured(source);
    assert!(
        run.output.parse_errors.is_empty(),
        "depth={depth}: {:?}",
        run.output.parse_errors
    );
    assert!(
        run.output.incomplete.is_empty(),
        "depth={depth}: {:?}",
        run.output.incomplete
    );
    assert_eq!(
        run.output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                &run.source[diagnostic.span.range()],
                diagnostic.message.as_str(),
            ))
            .collect::<Vec<_>>(),
        [
            (
                DiagnosticCode::TK2589,
                exhaustion_span.as_str(),
                "Type instantiation is excessively deep and possibly infinite.",
            ),
            (
                DiagnosticCode::TK2589,
                exhaustion_span.as_str(),
                "Type instantiation is excessively deep and possibly infinite.",
            ),
            (
                DiagnosticCode::TK2304,
                "MissingAfterDepth",
                "Cannot find name 'MissingAfterDepth'",
            ),
        ],
        "depth={depth}: lazy fallback must match eager depth handling and record order"
    );
    assert_eq!(
        run.measure.eager_materialization_roots, 1,
        "depth={depth}: the over-budget property must use bounded eager lowering"
    );
    assert_eq!(
        run.recipes,
        RecipeArenaMeasure::default(),
        "depth={depth}: an over-budget plan must publish no recipe rows"
    );
}

#[test]
fn undemanded_declared_property_respects_eager_annotation_depth_budget() {
    assert_undemanded_declared_property_depth_contract(201);
}

#[test]
fn larger_undemanded_declared_property_depth_terminates_deterministically() {
    assert_undemanded_declared_property_depth_contract(400);
}
